//! The real turbovec engine behind the Grounder trait: fastembed embeds code
//! chunks and the query; turbovec (2-4 bit quantized SIMD search) finds the
//! nearest chunks. Native Rust crates, no cgo, no shim - the payoff of the port.
//!
//! Two capabilities layer on top of that base:
//!
//! 1. **GPU-with-CPU-fallback embedding.** The embedding model runs on a GPU
//!    execution provider when one is available and falls back to CPU otherwise.
//!    fastembed v4 takes an ordered `Vec<ExecutionProviderDispatch>` on its
//!    `InitOptions`; the underlying `ort` framework registers each in order and,
//!    on any registration failure (no CUDA runtime, the EP's Cargo feature not
//!    compiled in, no GPU on the box), *silently falls back* to the next provider
//!    and ultimately to CPU. We hand it `[CUDA, CPU]`, so it is GPU-accelerated
//!    where possible and robust-on-CPU everywhere, and we log which one we got.
//!    See [`select_execution_providers`] for the one fastembed/ort limitation we
//!    hit (the shipped ONNX Runtime binaries are CPU-only) and how we handle it.
//!
//! 2. **A persisted, auto-freshened, incrementally-updated index.** The embeddings +
//!    the id->(file, line, snippet) map + a per-file content hash are persisted under
//!    `<root>/.rigger/grounding/`. On construction we LOAD that store if present; if it
//!    has drifted from the tree we freshen it incrementally rather than rebuilding, and
//!    only a true cold start (no store) pays the whole-repo embed.
//!    [`Turbovec::reindex`] re-embeds ONLY the files it is given (drops their old
//!    chunks, embeds the new ones, persists) - an incremental delta, not a full
//!    rebuild. The workflow calls `rigger reindex <changed files>` after each unit
//!    lands to PRE-WARM the index; but the actual freshness GUARANTEE lives in
//!    `ground` itself: every query first runs `freshen`, which diffs the tree against
//!    the persisted per-file hashes and incrementally re-embeds only changed/new files
//!    (dropping deleted ones). So a RAG query reflects the latest code even if an
//!    explicit reindex was missed - and on an unchanged tree `freshen` is a cheap
//!    hash-walk no-op (no embedding, no persist).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use fastembed::{EmbeddingModel, ExecutionProviderDispatch, InitOptions, TextEmbedding};
use ort::execution_providers::{CPUExecutionProvider, CUDAExecutionProvider, ExecutionProvider};
use serde::{Deserialize, Serialize};
use turbovec::IdMapIndex;

use super::{Grounder, Ref};

const EMBED_DIM: usize = 384; // BGESmallENV15 is 384-dimensional (a multiple of 8)
const BIT_WIDTH: usize = 4;
const CHUNK_LINES: usize = 40;

/// How many chunks to embed per forward pass. fastembed's default (256) pads each
/// batch to the longest chunk (up to the model's 512-token max) and materializes the
/// attention-score tensor `[batch, heads, seq, seq]` - at `[256, 12, 512, 512]` f32
/// that is ~3 GB in ONE allocation. On CPU that allocates fine; on the **CUDA EP**
/// the BFC arena tries to serve it as a single block and FAILS (`Failed to allocate
/// memory for requested buffer of size ...`), which aborts the GPU embed. A bounded
/// batch keeps each GPU forward pass's attention tensor small (`[32, 12, 512, 512]`
/// f32 ~= 384 MB), well within the card, so the embed runs on the GPU instead of
/// crashing. It is harmless on CPU - just more, smaller batches. 32 is a safe default
/// for a >=8 GB card; the 3090 (24 GB) has ample headroom.
const EMBED_BATCH_SIZE: usize = 32;

/// The persisted store lives under `<root>/.rigger/grounding/`: the quantized
/// vector index (`index.tvim`, written by `IdMapIndex::write`) plus the sidecar
/// metadata (`meta.json`) that maps each external vector id back to its
/// `(file, line, snippet)` and records a content hash per file. turbovec persists
/// only vectors+ids; everything needed to turn a search hit back into a `Ref`, and
/// to decide which files changed, lives in `meta.json` next to it.
const GROUNDING_DIR: &str = ".rigger/grounding";
const INDEX_FILE: &str = "index.tvim";
const META_FILE: &str = "meta.json";

/// Turbovec grounds semantically: it embeds the codebase into a quantized vector
/// index and returns the chunks nearest a query. The index + its id->Ref map are
/// persisted under `.rigger/grounding/` and loaded on construction when present, so
/// successive `rigger ground` calls reuse the embeddings instead of rebuilding, and
/// [`Self::reindex`] updates them per-file incrementally.
pub struct Turbovec {
    model: TextEmbedding,
    root: String,
    store_dir: PathBuf,
    state: Mutex<State>,
}

/// The mutable index state, behind one lock: the quantized index and the sidecar
/// metadata (id->Ref, file->{hash, ids}, the next id to allocate). Kept together so
/// the two never drift - every mutation updates both under the same lock and then
/// persists them together.
struct State {
    index: IdMapIndex,
    meta: Meta,
}

/// The result of attempting to load a persisted store on construction. Distinguishes
/// "no usable store" (a cold start -> full build) from "store loaded", and for the
/// latter whether it already matched the tree or has drifted and needs an incremental
/// freshen. Collapsing absent and drifted (as the old `bool` did) would force a full
/// rebuild on any drift; keeping them apart lets a drifted store be freshened in place.
enum LoadOutcome {
    /// No store, or one too corrupt to reuse: build the index from scratch once.
    Absent,
    /// A store was loaded into memory; `matched` is whether it already describes the
    /// current tree (`true`) or has drifted and must be incrementally freshened (`false`).
    Loaded { matched: bool },
}

/// The persisted sidecar: everything turbovec's `.tvim` does NOT hold. `refs` maps
/// each live vector id to its source location + snippet; `files` records, per file,
/// the content hash (to detect staleness) and the ids of the chunks that file
/// produced (so `reindex` can drop exactly that file's old vectors); `next_id` is
/// the monotonic id allocator (never reused, so a removed slot's id is never
/// resurrected onto a different chunk).
#[derive(Default, Serialize, Deserialize)]
struct Meta {
    next_id: u64,
    /// id -> the location/snippet that id's vector was embedded from.
    refs: HashMap<u64, StoredRef>,
    /// file (repo-relative) -> its content hash + the ids of its chunks.
    files: HashMap<String, FileEntry>,
}

/// A `Ref` as persisted in `meta.json`. Mirrors [`Ref`] but owns its own
/// serde derives so the grounder's public type stays free of them.
#[derive(Clone, Serialize, Deserialize)]
struct StoredRef {
    file: String,
    line: u32,
    text: String,
}

impl From<&StoredRef> for Ref {
    fn from(s: &StoredRef) -> Self {
        Ref {
            file: s.file.clone(),
            line: s.line,
            text: s.text.clone(),
        }
    }
}

/// Per-file bookkeeping: the content hash that detects a stale chunk set, and the
/// ids of the vectors this file currently owns in the index.
#[derive(Serialize, Deserialize)]
struct FileEntry {
    hash: u64,
    ids: Vec<u64>,
}

impl Turbovec {
    /// Build (or load) the index over `root`, downloading the embedding model on
    /// first use. If a consistent persisted store exists under
    /// `<root>/.rigger/grounding/`, it is loaded and the whole-repo embed is
    /// skipped; otherwise the tree is embedded once and the store is written.
    pub fn new(root: &str) -> Result<Self, String> {
        let model = TextEmbedding::try_new(
            InitOptions::new(EmbeddingModel::BGESmallENV15)
                .with_show_download_progress(false)
                .with_execution_providers(select_execution_providers()),
        )
        .map_err(|e| format!("turbovec: load model: {e}"))?;

        let store_dir = Path::new(root).join(GROUNDING_DIR);
        let tv = Turbovec {
            model,
            root: root.to_string(),
            store_dir,
            state: Mutex::new(State {
                index: IdMapIndex::new(EMBED_DIM, BIT_WIDTH)
                    .map_err(|e| format!("turbovec: new index: {e}"))?,
                meta: Meta::default(),
            }),
        };

        // Three cases on construction:
        //  - a persisted store that already matches the tree: load it, done (no embed).
        //  - a persisted store that has drifted from the tree: load it, then INCREMENTALLY
        //    freshen only the changed/new/deleted files (re-embed the diff, not the whole
        //    repo). This replaces the old "any drift -> full rebuild" behaviour.
        //  - no persisted store at all (cold start): a one-time full build of the tree.
        match tv.load_persisted_any()? {
            LoadOutcome::Loaded { matched } => {
                if !matched {
                    // The store loaded but the tree drifted; bring it current incrementally.
                    tv.freshen()?;
                }
                eprintln!(
                    "turbovec: loaded persisted index ({} chunks) from {}{}",
                    tv.state.lock().unwrap().index.len(),
                    tv.store_dir.display(),
                    if matched {
                        ""
                    } else {
                        " (incrementally freshened)"
                    }
                );
            }
            LoadOutcome::Absent => {
                tv.build_from_tree()?;
                tv.persist()?;
                eprintln!(
                    "turbovec: built and persisted index ({} chunks) to {}",
                    tv.state.lock().unwrap().index.len(),
                    tv.store_dir.display()
                );
            }
        }
        Ok(tv)
    }

    /// Load the persisted index + metadata from `.rigger/grounding/` if a usable
    /// store is on disk, reporting whether it already matches the tree.
    ///
    /// - [`LoadOutcome::Absent`] - there is no store, or it is corrupt / unreadable
    ///   (a corrupt store cannot be freshened incrementally, so it is treated as a
    ///   cold start: a full rebuild). The in-memory state is left empty.
    /// - [`LoadOutcome::Loaded { matched: true }`] - the store loaded AND its file
    ///   set + per-file content hashes exactly match the tree; it is reusable as-is.
    /// - [`LoadOutcome::Loaded { matched: false }`] - the store loaded but the tree
    ///   has drifted (an edit / add / delete happened with no process around to
    ///   reindex). The loaded state IS installed so the caller can [`Self::freshen`]
    ///   it incrementally - re-embedding only the diff rather than the whole repo.
    fn load_persisted_any(&self) -> Result<LoadOutcome, String> {
        let index_path = self.store_dir.join(INDEX_FILE);
        let meta_path = self.store_dir.join(META_FILE);
        if !index_path.exists() || !meta_path.exists() {
            return Ok(LoadOutcome::Absent);
        }
        let index = match IdMapIndex::load(&index_path) {
            Ok(i) => i,
            Err(_) => return Ok(LoadOutcome::Absent), // corrupt / wrong-version -> rebuild
        };
        let meta_bytes =
            std::fs::read(&meta_path).map_err(|e| format!("turbovec: read meta: {e}"))?;
        let meta: Meta = match serde_json::from_slice(&meta_bytes) {
            Ok(m) => m,
            Err(_) => return Ok(LoadOutcome::Absent), // unreadable meta -> rebuild
        };
        let matched = self.tree_matches(&meta);
        // Install the loaded state either way: when it matches it is used as-is, and
        // when it has drifted it is the BASE that `freshen` updates incrementally
        // (drop deleted files' chunks, re-embed changed/new files) - never a full rebuild.
        let mut state = self.state.lock().unwrap();
        state.index = index;
        state.meta = meta;
        Ok(LoadOutcome::Loaded { matched })
    }

    /// Bring the in-memory + persisted index up to date with the current source tree,
    /// INCREMENTALLY: diff the tree against the persisted per-file hashes and touch only
    /// what changed. This is the freshness guarantee - called at the start of every
    /// [`Grounder::ground`] so any RAG query reflects the latest code, and on
    /// construction when a persisted store has drifted.
    ///
    /// The diff, vs. the persisted `meta.files`:
    /// - CHANGED (file present in both, content hash differs) and NEW (on disk, absent
    ///   from meta) files are fed to the existing incremental reindex path: drop the old
    ///   chunks (a no-op for a new file), re-embed the current content under fresh ids,
    ///   insert. Only these files are embedded.
    /// - DELETED (in meta, gone from the tree) files have their chunks dropped.
    ///
    /// The COMMON case is no change: the walk hashes each file, finds every hash equal
    /// and no additions/deletions, and returns WITHOUT embedding or persisting anything -
    /// the cost is just the hash walk. We persist once, and only when something actually
    /// changed, so a steady-state `ground` does no write either.
    fn freshen(&self) -> Result<(), String> {
        // 1. Snapshot the tree as (rel path -> content), the same file set the index covers.
        let mut on_disk = Vec::new();
        collect_files(Path::new(&self.root), &self.root, &mut on_disk);

        // 2. Diff against the persisted per-file hashes WITHOUT holding the lock across
        //    embedding. Take a brief lock only to read meta, then release it.
        let mut changed_or_new: Vec<(String, String)> = Vec::new();
        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        let deleted: Vec<String>;
        {
            let state = self.state.lock().unwrap();
            for (rel, content) in &on_disk {
                seen.insert(rel.as_str());
                match state.meta.files.get(rel) {
                    // Unchanged: same content hash -> skip (no embed).
                    Some(entry) if entry.hash == hash_content(content) => {}
                    // Changed or new: queue for an incremental re-embed.
                    _ => changed_or_new.push((rel.clone(), content.clone())),
                }
            }
            // In meta but no longer on disk -> deleted; queue its chunks for removal.
            deleted = state
                .meta
                .files
                .keys()
                .filter(|f| !seen.contains(f.as_str()))
                .cloned()
                .collect();
        }

        // 3. Nothing differs -> cheap no-op: no embedding, no persist. This is the
        //    steady-state path a `ground` on an unchanged tree takes.
        if changed_or_new.is_empty() && deleted.is_empty() {
            return Ok(());
        }

        // 4. Apply the delta, reusing the same internals `reindex` uses. `drop_file`
        //    and `index_file_content` each take the lock internally, so embedding (the
        //    slow part) runs unlocked.
        for rel in &deleted {
            self.drop_file(rel);
        }
        for (rel, content) in &changed_or_new {
            self.drop_file(rel); // no-op for a brand-new file; clears old chunks for a changed one
            self.index_file_content(rel, content)?;
        }

        // 5. Persist the updated index + metadata once.
        self.persist()
    }

    /// Whether the persisted `meta` still describes the on-disk tree: the same set
    /// of indexable files, each with an unchanged content hash. A mismatch means
    /// the tree drifted out from under the store (an edit, add, or delete with no
    /// process around to `reindex`), so the store cannot be reused verbatim.
    fn tree_matches(&self, meta: &Meta) -> bool {
        let mut on_disk = Vec::new();
        collect_files(Path::new(&self.root), &self.root, &mut on_disk);
        if on_disk.len() != meta.files.len() {
            return false;
        }
        for (rel, content) in on_disk {
            match meta.files.get(&rel) {
                Some(entry) if entry.hash == hash_content(&content) => {}
                _ => return false,
            }
        }
        true
    }

    /// Embed the whole tree once into a fresh index + metadata. Used on a cold
    /// start (no store) or when the persisted store is inconsistent. Replaces the
    /// in-memory state wholesale; the caller persists it.
    fn build_from_tree(&self) -> Result<(), String> {
        let mut on_disk = Vec::new();
        collect_files(Path::new(&self.root), &self.root, &mut on_disk);
        // Reset to an empty index/meta so a rebuild after an inconsistent load does
        // not accumulate on top of stale state.
        {
            let mut state = self.state.lock().unwrap();
            state.index = IdMapIndex::new(EMBED_DIM, BIT_WIDTH)
                .map_err(|e| format!("turbovec: new index: {e}"))?;
            state.meta = Meta::default();
        }
        for (rel, content) in on_disk {
            self.index_file_content(&rel, &content)?;
        }
        Ok(())
    }

    /// Chunk + embed one file's content and insert its vectors under fresh ids,
    /// recording the file's hash and chunk ids in the metadata. The file's PRIOR
    /// chunks (if any) must already have been removed by the caller - this only
    /// adds. Returns without embedding when the file has no non-blank chunks (the
    /// file is recorded with an empty id set so it still counts toward consistency).
    fn index_file_content(&self, rel: &str, content: &str) -> Result<(), String> {
        let (texts, refs) = chunk_content(rel, content);
        let hash = hash_content(content);
        if texts.is_empty() {
            let mut state = self.state.lock().unwrap();
            state.meta.files.insert(
                rel.to_string(),
                FileEntry {
                    hash,
                    ids: Vec::new(),
                },
            );
            return Ok(());
        }
        // Bound the batch so a single GPU forward pass's attention tensor stays small
        // enough for the CUDA arena (see EMBED_BATCH_SIZE) - an unbounded batch crashed
        // the GPU embed with a multi-GB single allocation. On CPU this is just more,
        // smaller batches.
        let embeddings = self
            .model
            .embed(texts, Some(EMBED_BATCH_SIZE))
            .map_err(|e| format!("turbovec: embed: {e}"))?;

        let mut state = self.state.lock().unwrap();
        let mut flat = Vec::with_capacity(embeddings.len() * EMBED_DIM);
        let mut ids = Vec::with_capacity(embeddings.len());
        for (emb, r) in embeddings.iter().zip(refs) {
            let id = state.meta.next_id;
            state.meta.next_id += 1;
            flat.extend_from_slice(emb);
            ids.push(id);
            state.meta.refs.insert(id, r);
        }
        state
            .index
            .add_with_ids(&flat, &ids)
            .map_err(|e| format!("turbovec: add: {e}"))?;
        state
            .meta
            .files
            .insert(rel.to_string(), FileEntry { hash, ids });
        Ok(())
    }

    /// Drop a file's existing chunks from BOTH the index and the metadata, so a
    /// re-index of that file starts clean. A file not previously indexed is a no-op.
    fn drop_file(&self, rel: &str) {
        let mut state = self.state.lock().unwrap();
        if let Some(entry) = state.meta.files.remove(rel) {
            for id in entry.ids {
                state.index.remove(id);
                state.meta.refs.remove(&id);
            }
        }
    }

    /// Persist the index (`index.tvim`) and the metadata (`meta.json`) to
    /// `.rigger/grounding/`, creating the directory if needed. Both are written so a
    /// later construction can reload them and skip the whole-repo embed.
    fn persist(&self) -> Result<(), String> {
        std::fs::create_dir_all(&self.store_dir)
            .map_err(|e| format!("turbovec: create {}: {e}", self.store_dir.display()))?;
        let state = self.state.lock().unwrap();
        state
            .index
            .write(self.store_dir.join(INDEX_FILE))
            .map_err(|e| format!("turbovec: write index: {e}"))?;
        let bytes = serde_json::to_vec(&state.meta)
            .map_err(|e| format!("turbovec: serialize meta: {e}"))?;
        std::fs::write(self.store_dir.join(META_FILE), bytes)
            .map_err(|e| format!("turbovec: write meta: {e}"))?;
        Ok(())
    }

    fn embed_query(&self, query: &str) -> Option<Vec<f32>> {
        self.model.embed(vec![query], None).ok()?.into_iter().next()
    }
}

impl Grounder for Turbovec {
    fn ground(&self, query: &str, k: usize) -> Vec<Ref> {
        if query.is_empty() || k == 0 {
            return Vec::new();
        }
        // Freshness guarantee: before answering ANY query, bring the index current with
        // the tree, INCREMENTALLY - re-embed only files that changed/were added since the
        // last index, drop deleted ones. On the common no-change tree this is just a hash
        // walk (no embedding, no persist). So every RAG result reflects the latest code,
        // whether or not an explicit `reindex` was run. We freshen BEFORE locking below;
        // `freshen` does its own internal locking, so there is no nested lock.
        if let Err(e) = self.freshen() {
            // A freshen failure must not silently serve stale results; surface it but
            // still answer from whatever the index currently holds.
            eprintln!("turbovec: freshen before ground failed: {e}");
        }
        let qv = match self.embed_query(query) {
            Some(v) => v,
            None => return Vec::new(),
        };
        let state = self.state.lock().unwrap();
        if state.index.is_empty() {
            return Vec::new();
        }
        let (_scores, ids) = state.index.search(&qv, k);
        ids.iter()
            .filter_map(|id| state.meta.refs.get(id).map(Ref::from))
            .collect()
    }

    /// Re-index ONLY the given files after a unit integrates, so the next agent
    /// grounds on the accepted code - an incremental delta, NOT a full rebuild. For
    /// each file: drop its old chunks from the index + metadata, re-embed its
    /// current content under fresh ids, insert them, then persist once. A file that
    /// no longer exists on disk is dropped (its chunks removed) without re-adding.
    fn reindex(&self, src_dir: &str, files: &[String]) {
        if files.is_empty() {
            return;
        }
        for f in files {
            self.drop_file(f);
            let path = Path::new(src_dir).join(f);
            // The file still exists: re-embed its current content under new ids. If
            // it was deleted (or is unreadable), its chunks were already dropped
            // above and there is nothing to re-add.
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Err(e) = self.index_file_content(f, &content) {
                    eprintln!("turbovec: reindex {f}: {e}");
                }
            }
        }
        if let Err(e) = self.persist() {
            eprintln!("turbovec: reindex persist: {e}");
        }
    }
}

/// Select the embedding model's execution providers, GPU-first with a CPU fallback.
///
/// We return `[CUDA, CPU]`. fastembed feeds this ordered list to `ort`, whose
/// framework registers each in turn and, on ANY registration failure, *silently
/// falls back* to the next provider (and finally to CPU) rather than erroring - the
/// dispatch's default is `fail_silently`. So on a CUDA box the model runs on the
/// GPU; on a box with no GPU / no CUDA runtime - or, as below, an ORT build without
/// the CUDA EP compiled in - the CUDA registration fails harmlessly and CPU is used.
/// This never panics for want of a GPU.
///
/// fastembed/ort LIMITATION worth stating plainly: the ONNX Runtime that fastembed
/// downloads via its default `ort-download-binaries` feature is the **CPU-only**
/// build, and fastembed does NOT enable `ort/cuda`. So with the dependency tree as
/// it ships today, the CUDA EP's Cargo feature is not compiled in and registration
/// fails with "...its corresponding Cargo feature is not enabled" - meaning GPU
/// acceleration requires either a CUDA-enabled ORT (e.g. building `ort` with
/// `-F cuda` against a CUDA toolkit, or `load-dynamic` against a GPU ORT .so) on the
/// host. The selection code is written so that the DAY such a runtime is present,
/// the GPU is used with no code change; until then we run correctly on CPU. We probe
/// `is_available()` only to LOG which provider actually backs this session.
fn select_execution_providers() -> Vec<ExecutionProviderDispatch> {
    let cuda = CUDAExecutionProvider::default();
    // `is_available()` reports whether the loaded ONNX Runtime was COMPILED with
    // CUDA support. It can fail if the runtime cannot even be queried; treat any
    // error as "not available" so a probe never crashes the grounder.
    let cuda_available = cuda.is_available().unwrap_or(false);
    if cuda_available {
        eprintln!(
            "turbovec: CUDA execution provider available; embedding on GPU (CPU fallback armed)"
        );
    } else {
        eprintln!("turbovec: no CUDA execution provider; embedding on CPU");
    }
    // Hand ort an ordered GPU-then-CPU list either way: when CUDA is unavailable its
    // registration fails silently and ort uses the explicit CPU provider, so the
    // model always has a working backend.
    vec![
        CUDAExecutionProvider::default().build(),
        CPUExecutionProvider::default().build(),
    ]
}

/// Read every indexable file under `root` as (repo-relative path, content),
/// skipping VCS / build / dependency dirs and unreadable (binary) files. The single
/// source of truth for "what the index covers", shared by the cold build and the
/// load-time consistency check so the two never disagree about the file set.
fn collect_files(dir: &Path, root: &str, out: &mut Vec<(String, String)>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if !matches!(
                name.as_ref(),
                ".git" | ".rigger" | "vendor" | "target" | "node_modules"
            ) {
                collect_files(&path, root, out);
            }
        } else if let Ok(content) = std::fs::read_to_string(&path) {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned();
            out.push((rel, content));
        }
    }
}

/// Chunk one file's content into fixed line windows, returning the embeddable text
/// of each non-blank chunk and its [`StoredRef`] (repo-relative file, 1-based start
/// line, first-non-blank snippet). The `rel` path is the chunk's recorded location.
fn chunk_content(rel: &str, content: &str) -> (Vec<String>, Vec<StoredRef>) {
    let mut texts = Vec::new();
    let mut refs = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut start = 0;
    while start < lines.len() {
        let end = (start + CHUNK_LINES).min(lines.len());
        let slice = &lines[start..end];
        let chunk = slice.join("\n");
        if !chunk.trim().is_empty() {
            texts.push(chunk);
            refs.push(StoredRef {
                file: rel.to_string(),
                line: (start + 1) as u32,
                text: first_non_blank(slice),
            });
        }
        start += CHUNK_LINES;
    }
    (texts, refs)
}

/// A stable content hash for staleness detection: same bytes -> same hash, across
/// processes and machines. Uses a fixed-seed FNV-1a so the value persisted in
/// `meta.json` compares equal on a later run (unlike `DefaultHasher`, whose seed is
/// not guaranteed stable across builds).
fn hash_content(content: &str) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = FNV_OFFSET;
    for byte in content.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

fn first_non_blank(lines: &[&str]) -> String {
    lines
        .iter()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Keep the test corpus TINY (a few small files): the embedding step is bounded
    /// in memory and time, so the suite never blows up the box.
    fn tiny_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("combat.rs"),
            "fn apply_damage(target: &mut Entity, amount: f32) {\n    target.health -= amount;\n}\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("render.rs"),
            "fn draw_sprite(sprite: &Sprite, x: f32, y: f32) {\n    // upload to the gpu\n}\n",
        )
        .unwrap();
        dir
    }

    /// EP selection must never panic for want of a GPU: on a box with no CUDA it
    /// returns a usable provider list with CPU as the guaranteed fallback (the last
    /// entry), and the model still constructs and embeds. This is the graceful-
    /// degradation guarantee - "attempt the GPU EP, fall back to CPU, never crash".
    #[test]
    fn ep_selection_falls_back_to_cpu_without_a_gpu() {
        // Selection itself is infallible and always offers CPU as the final option.
        let eps = select_execution_providers();
        assert_eq!(eps.len(), 2, "the list is GPU-then-CPU: [CUDA, CPU]");
        // The list ENDS in the CPU provider, so ort always has a working backend
        // even when CUDA registration fails.
        assert_eq!(
            format!("{:?}", eps.last().unwrap()),
            format!("{:?}", CPUExecutionProvider::default().build()),
            "CPU must be the guaranteed final fallback in the EP list"
        );
        // And constructing the model with that list succeeds and embeds on CPU when
        // there is no GPU - it does not panic.
        let dir = tiny_repo();
        let tv = Turbovec::new(dir.path().to_str().unwrap()).unwrap();
        assert!(
            !tv.ground("how is damage dealt to an enemy", 1).is_empty(),
            "the CPU-fallback model must still embed and ground"
        );
    }

    // Downloads the embedding model on first run; gated behind the turbovec feature.
    #[test]
    fn grounds_semantically() {
        let dir = tiny_repo();
        let tv = Turbovec::new(dir.path().to_str().unwrap()).unwrap();
        let refs = tv.ground("how is damage dealt to an enemy", 1);
        assert_eq!(
            refs.first().map(|r| r.file.as_str()),
            Some("combat.rs"),
            "semantic search should rank the damage code above the rendering code"
        );
    }

    /// Constructing the grounder PERSISTS the index to `.rigger/grounding/`, and a
    /// second construction over the same tree LOADS it (no rebuild) and grounds
    /// identically - the save->load round-trip the incremental story rests on.
    #[test]
    fn persisted_index_round_trips_save_then_load() {
        let dir = tiny_repo();
        let root = dir.path().to_str().unwrap();

        // First construction builds + persists the store.
        let first = Turbovec::new(root).unwrap();
        let first_hit = first.ground("how is damage dealt to an enemy", 1);
        assert_eq!(
            first_hit.first().map(|r| r.file.as_str()),
            Some("combat.rs")
        );
        let built_chunks = first.state.lock().unwrap().index.len();
        assert!(
            built_chunks > 0,
            "the freshly-built index must be non-empty"
        );
        drop(first);

        // The store files exist on disk.
        let store = dir.path().join(GROUNDING_DIR);
        assert!(
            store.join(INDEX_FILE).exists(),
            "the index file must be persisted"
        );
        assert!(
            store.join(META_FILE).exists(),
            "the metadata file must be persisted"
        );

        // A second construction LOADS the persisted store (the tree is unchanged) and
        // grounds identically - the round-trip preserves the searchable index.
        let second = Turbovec::new(root).unwrap();
        let second_hit = second.ground("how is damage dealt to an enemy", 1);
        assert_eq!(
            second_hit.first().map(|r| r.file.as_str()),
            Some("combat.rs"),
            "the reloaded index must ground identically to the freshly-built one"
        );
        // The loaded index has exactly the chunk count that was built and persisted -
        // the save->load round-trip neither dropped nor duplicated vectors.
        assert_eq!(
            second.state.lock().unwrap().index.len(),
            built_chunks,
            "the reloaded index must have the same chunk count as the built one"
        );
    }

    /// `reindex(file)` is an INCREMENTAL update: a term written into a file AFTER the
    /// index was built becomes findable once that one file is reindexed, without
    /// rebuilding the whole index. This is the "changes land before review" guarantee.
    #[test]
    fn reindex_makes_a_new_term_findable_incrementally() {
        let dir = tiny_repo();
        let root = dir.path().to_str().unwrap();
        let tv = Turbovec::new(root).unwrap();

        // A concept absent from the original corpus is not yet grounded to combat.rs.
        let before = tv.ground("teleport the player across the dungeon", 1);
        let combat_before = before.first().map(|r| r.file.as_str()) == Some("combat.rs");

        // The change lands: combat.rs now contains a teleport function.
        std::fs::write(
            dir.path().join("combat.rs"),
            "fn apply_damage(target: &mut Entity, amount: f32) {\n    target.health -= amount;\n}\n\
             fn teleport_player(player: &mut Player, dest: Tile) {\n    player.position = dest;\n}\n",
        )
        .unwrap();

        // Incrementally reindex ONLY that file (not a full rebuild).
        tv.reindex(root, &["combat.rs".to_string()]);

        // The just-landed term is now findable, ranked to the file it was added to.
        let after = tv.ground("teleport the player across the dungeon", 1);
        assert_eq!(
            after.first().map(|r| r.file.as_str()),
            Some("combat.rs"),
            "after reindex, the new teleport code must be the nearest chunk; before={combat_before}"
        );

        // The incremental update is persisted: a fresh construction loads it and the
        // term stays findable WITHOUT re-embedding the tree.
        drop(tv);
        let reloaded = Turbovec::new(root).unwrap();
        let after_reload = reloaded.ground("teleport the player across the dungeon", 1);
        assert_eq!(
            after_reload.first().map(|r| r.file.as_str()),
            Some("combat.rs"),
            "the reindexed term must survive persistence + reload"
        );
    }

    /// `reindex` drops a file's OLD chunks (it is not append-only): a file is
    /// reindexed to NEW content, and only the new content is findable; a removed
    /// file's chunks disappear from the index entirely.
    #[test]
    fn reindex_replaces_old_chunks_and_drops_deleted_files() {
        let dir = tiny_repo();
        let root = dir.path().to_str().unwrap();
        let tv = Turbovec::new(root).unwrap();
        let chunks_before = tv.state.lock().unwrap().index.len();

        // Overwrite render.rs with unrelated content, then reindex it.
        std::fs::write(
            dir.path().join("render.rs"),
            "fn parse_config(path: &str) -> Config {\n    Config::from_file(path)\n}\n",
        )
        .unwrap();
        tv.reindex(root, &["render.rs".to_string()]);
        // The old draw_sprite chunk is gone from the metadata (its id was removed).
        let has_sprite = tv
            .state
            .lock()
            .unwrap()
            .meta
            .refs
            .values()
            .any(|r| r.text.contains("draw_sprite"));
        assert!(
            !has_sprite,
            "reindex must drop the file's prior chunks, not append"
        );

        // Deleting a file and reindexing it removes its chunks entirely.
        std::fs::remove_file(dir.path().join("render.rs")).unwrap();
        tv.reindex(root, &["render.rs".to_string()]);
        assert!(
            !tv.state
                .lock()
                .unwrap()
                .meta
                .files
                .contains_key("render.rs"),
            "a deleted file must be dropped from the index on reindex"
        );
        // The index strictly shrank: render.rs's chunk(s) are gone, combat.rs's stay.
        let chunks_after = tv.state.lock().unwrap().index.len();
        assert!(
            chunks_after < chunks_before,
            "reindexing a deleted file must shrink the index (was {chunks_before}, now {chunks_after})"
        );
        // combat.rs's chunk is still there, so the index is not emptied.
        assert!(
            !tv.state.lock().unwrap().index.is_empty(),
            "reindexing one deleted file must not empty the whole index"
        );
        let still_has_damage = tv
            .state
            .lock()
            .unwrap()
            .meta
            .refs
            .values()
            .any(|r| r.text.contains("apply_damage"));
        assert!(
            still_has_damage,
            "the untouched file's chunks must remain after another file's reindex"
        );
    }

    /// A stable content hash: identical bytes hash equal (so a reload detects an
    /// unchanged tree), different bytes hash differently (so an edit is detected).
    #[test]
    fn content_hash_is_stable_and_distinguishes() {
        assert_eq!(hash_content("hello world"), hash_content("hello world"));
        assert_ne!(hash_content("hello world"), hash_content("hello worlds"));
    }

    /// The chunk ids a file currently owns in the index, read from the metadata.
    /// Sorted so two snapshots compare by value regardless of insertion order.
    fn file_ids(tv: &Turbovec, rel: &str) -> Vec<u64> {
        let state = tv.state.lock().unwrap();
        let mut ids = state
            .meta
            .files
            .get(rel)
            .map(|e| e.ids.clone())
            .unwrap_or_default();
        ids.sort_unstable();
        ids
    }

    /// The monotonic id allocator. It advances by exactly one per chunk EMBEDDED, and
    /// never for an unchanged file, so comparing it across a `ground` is a precise
    /// "did any embedding happen" probe: equal next_id <=> no chunk was (re-)embedded.
    fn next_id(tv: &Turbovec) -> u64 {
        tv.state.lock().unwrap().meta.next_id
    }

    /// (a) GUARANTEE: a `ground` AFTER an edit reflects the edit, with NO explicit
    /// reindex call. We write a distinctive new term into a file and immediately
    /// `ground` for it; the auto-freshen at the start of `ground` re-embeds the edited
    /// file, so it is the top hit - the freshness lives in the grounder, not the caller.
    #[test]
    fn ground_auto_freshens_after_an_edit_without_explicit_reindex() {
        let dir = tiny_repo();
        let root = dir.path().to_str().unwrap();
        let tv = Turbovec::new(root).unwrap();

        // A term absent from the original corpus.
        let term = "how does the quantum flux capacitor stabilize the warp core";

        // The change lands on disk - but we deliberately do NOT call reindex.
        std::fs::write(
            dir.path().join("render.rs"),
            "fn draw_sprite(sprite: &Sprite, x: f32, y: f32) {\n    // upload to the gpu\n}\n\
             fn stabilize_flux_capacitor(core: &mut WarpCore) {\n    core.quantum_flux = core.stabilize();\n}\n",
        )
        .unwrap();

        // Grounding alone must reflect the edit: the auto-freshen re-embeds render.rs.
        let hit = tv.ground(term, 1);
        assert_eq!(
            hit.first().map(|r| r.file.as_str()),
            Some("render.rs"),
            "ground must auto-freshen the edited file and rank it top WITHOUT an explicit reindex"
        );
    }

    /// (b) INCREMENTAL, not a full rebuild: editing one file and grounding re-embeds
    /// ONLY that file. We capture the UNCHANGED file's chunk ids before the edit and
    /// assert they are byte-for-byte preserved after grounding, while the edited file's
    /// ids change. Preserved ids prove the unchanged file was never dropped+re-embedded.
    #[test]
    fn auto_freshen_is_incremental_not_a_full_rebuild() {
        let dir = tiny_repo();
        let root = dir.path().to_str().unwrap();
        let tv = Turbovec::new(root).unwrap();

        // Snapshot ids of BOTH files from the freshly built index.
        let combat_ids_before = file_ids(&tv, "combat.rs");
        let render_ids_before = file_ids(&tv, "render.rs");
        assert!(!combat_ids_before.is_empty() && !render_ids_before.is_empty());

        // Edit ONLY render.rs.
        std::fs::write(
            dir.path().join("render.rs"),
            "fn draw_sprite(sprite: &Sprite, x: f32, y: f32) {\n    // upload to the gpu\n}\n\
             fn blit_overlay(layer: &Layer) {\n    layer.compose();\n}\n",
        )
        .unwrap();

        // A ground triggers the incremental freshen.
        let _ = tv.ground("compose an overlay layer", 1);

        // combat.rs was untouched: its chunk ids are exactly preserved - it was NOT
        // re-embedded (a re-embed would mint fresh, higher ids).
        let combat_ids_after = file_ids(&tv, "combat.rs");
        assert_eq!(
            combat_ids_before, combat_ids_after,
            "the unchanged file's chunk ids must be preserved - it must NOT be re-embedded"
        );

        // render.rs WAS edited: its old chunks were dropped and new ones minted, so its
        // id set changed (and the new ids are all freshly allocated, i.e. higher).
        let render_ids_after = file_ids(&tv, "render.rs");
        assert_ne!(
            render_ids_before, render_ids_after,
            "the edited file's chunk ids must change - only it is re-embedded"
        );
        assert!(
            render_ids_after.iter().min().unwrap() > render_ids_before.iter().max().unwrap(),
            "the edited file's new chunk ids must be freshly allocated (monotonic), proving a \
             targeted re-embed of just that file, not a whole-index rebuild"
        );
    }

    /// (c) A `ground` reflects a DELETION: removing a file makes its unique term
    /// unfindable, because the auto-freshen drops a vanished file's chunks.
    #[test]
    fn ground_drops_a_deleted_files_content() {
        let dir = tiny_repo();
        let root = dir.path().to_str().unwrap();
        let tv = Turbovec::new(root).unwrap();

        // draw_sprite (unique to render.rs) is findable while the file exists.
        let before = tv.ground("draw a sprite onto the screen", 1);
        assert_eq!(
            before.first().map(|r| r.file.as_str()),
            Some("render.rs"),
            "the rendering term should ground to render.rs while it exists"
        );

        // Delete render.rs - no explicit reindex.
        std::fs::remove_file(dir.path().join("render.rs")).unwrap();

        // The next ground auto-freshens, dropping render.rs's chunks; combat.rs is all
        // that is left, so the rendering term can no longer ground to render.rs.
        let after = tv.ground("draw a sprite onto the screen", 1);
        assert!(
            after.iter().all(|r| r.file != "render.rs"),
            "a deleted file's content must be gone from grounding results after auto-freshen"
        );
        // The deleted file is also gone from the metadata's file set.
        assert!(
            !tv.state
                .lock()
                .unwrap()
                .meta
                .files
                .contains_key("render.rs"),
            "the deleted file must be removed from the index metadata"
        );
    }

    /// (d) FAST no-op: a second ground on an UNCHANGED tree does no embedding work. The
    /// monotonic id allocator does not advance across the second ground, proving freshen
    /// hit the cheap hash-walk path (no chunk re-embedded, nothing persisted).
    #[test]
    fn unchanged_tree_grounds_without_re_embedding() {
        let dir = tiny_repo();
        let root = dir.path().to_str().unwrap();
        let tv = Turbovec::new(root).unwrap();

        // First ground freshens (tree already matches the just-built index, so even this
        // is a no-op) and records the id high-water mark.
        let _ = tv.ground("how is damage dealt to an enemy", 1);
        let next_before = next_id(&tv);

        // A second ground on the SAME, unchanged tree must embed nothing new.
        let _ = tv.ground("how is damage dealt to an enemy", 1);
        let next_after = next_id(&tv);

        assert_eq!(
            next_before, next_after,
            "grounding an unchanged tree must allocate no new chunk ids - freshen took the \
             cheap hash-walk no-op path with no re-embedding"
        );
    }
}
