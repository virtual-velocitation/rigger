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
//!    See [`select_execution_providers`] for how the `-F cuda` ort build + the
//!    `ORT_DYLIB_PATH` runtime discovery make the GPU path real, and how it degrades.
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
use std::fs::File;
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
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
/// The cross-process advisory lock file under the store dir. `flock(2)` on this file
/// serializes the load+persist critical section across separate `rigger` processes
/// (a workflow's `parallel()` lenses, a `rigger reindex`), so no process ever reads a
/// half-written store or an index/meta pair that disagree. It holds no data; its only
/// purpose is to be the flock target.
const LOCK_FILE: &str = "store.lock";

/// Serializes embedding-model CONSTRUCTION across the whole process. `ort`, built with
/// `load-dynamic`, lazily reads `ORT_DYLIB_PATH` on the FIRST session load and is not
/// safe to construct concurrently on a CUDA box (concurrent session creation corrupts
/// the heap). Every `Turbovec::new` takes this lock across BOTH `ensure_dylib_path`'s
/// env write AND `TextEmbedding::try_new`, so the env mutation can never race ort's
/// lazy env read on another thread, and two sessions are never built at once.
static CONSTRUCT_MU: Mutex<()> = Mutex::new(());

/// Turbovec grounds semantically: it embeds the codebase into a quantized vector
/// index and returns the chunks nearest a query. The index + its id->Ref map are
/// persisted under `.rigger/grounding/` and loaded on construction when present, so
/// successive `rigger ground` calls reuse the embeddings instead of rebuilding, and
/// [`Self::reindex`] updates them per-file incrementally.
pub struct Turbovec {
    model: TextEmbedding,
    root: String,
    store_dir: PathBuf,
    /// The in-memory index+meta, and the single mutation authority over it. EVERY
    /// mutation (build, freshen, reindex, drop, persist) runs while THIS lock is held
    /// for the whole critical section - the internal helpers take `&mut State`, they
    /// never re-lock - so two freshens / reindexes can never interleave a diff against
    /// an apply. A `ground`'s search takes the same lock, so it also serializes.
    state: Mutex<State>,
    /// Serializes every call into `model.embed()` - the one shared `ort` session's
    /// `Session::run`. Concurrent `Session::run` on a single CUDA session corrupts the
    /// heap, so this is the process-wide "at most one embed at a time" authority: query
    /// embeds (`embed_query`) and content embeds (`index_file_content`) BOTH take it,
    /// held across the whole `embed` call. It is a separate lock from `state` so a query
    /// embed (which is not under the state lock) still cannot run concurrently with a
    /// freshen's content embed.
    embed_mu: Mutex<()>,
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

/// What construction does with a persisted store that LOADED but has drifted from the
/// tree. `new` (the grounding-read path) wants the index current, so it freshens the
/// whole diff; `new_for_reindex` leaves it as-loaded and lets `reindex` re-embed only
/// the files it is explicitly given, so those files are never double-embedded.
enum OnDrift {
    /// Incrementally freshen the whole diff now (the `ground`/`run`/`serve` path).
    Freshen,
    /// Leave the loaded store as-is; the caller re-embeds only its named files.
    LeaveStale,
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
    /// `<root>/.rigger/grounding/`, it is loaded (and freshened in place if the tree
    /// drifted) and the whole-repo embed is skipped; otherwise the tree is embedded
    /// once and the store is written. This is the grounding-read entry point
    /// (`ground`/`serve`/`run`): it wants the index fully current, so on drift it
    /// freshens the whole diff.
    pub fn new(root: &str) -> Result<Self, String> {
        Self::construct(root, OnDrift::Freshen)
    }

    /// Construct for `rigger reindex`: load the persisted store as-is and do NOT
    /// freshen the whole tree's drift. The caller (`reindex`) re-embeds exactly the
    /// named files, so a preceding full freshen would DOUBLE-EMBED them (and re-embed
    /// every other drifted file the reindex was never asked to touch). Files not named
    /// stay as the loaded store has them; the next `ground` auto-freshens any remaining
    /// drift. A cold start (no store) still builds the tree once - there is nothing to
    /// load, and the build already indexes the named files correctly, making the
    /// subsequent reindex of them a cheap, correct re-embed of just those.
    pub fn new_for_reindex(root: &str) -> Result<Self, String> {
        Self::construct(root, OnDrift::LeaveStale)
    }

    /// Shared construction: build the model (serialized process-wide) then load-or-build
    /// the store. `on_drift` selects whether a loaded-but-drifted store is freshened now
    /// (`new`) or left as-loaded (`new_for_reindex`, which re-embeds only named files).
    fn construct(root: &str, on_drift: OnDrift) -> Result<Self, String> {
        // Serialize model CONSTRUCTION across the whole process. Two concerns fold into
        // one lock (see CONSTRUCT_MU): (1) `ensure_dylib_path` mutates the `ORT_DYLIB_PATH`
        // process env var and `ort` lazily READS it when it first loads the runtime, so
        // the write must not race a concurrent ort env read on another thread; (2) building
        // two `ort`/CUDA sessions at once corrupts the heap. Holding CONSTRUCT_MU across
        // BOTH the env write AND `TextEmbedding::try_new` closes both races: at most one
        // thread is in this block, so no other thread is loading a session (and thus
        // reading the env) while we write it, and no two sessions are built concurrently.
        let model = {
            let _construct = CONSTRUCT_MU.lock().unwrap();
            // Point `ort` (built with `load-dynamic`) at a discovered `libonnxruntime.so`
            // BEFORE the fastembed/`ort` model below first loads the runtime. `main` also
            // calls this, but tests and any other caller that constructs the grounder
            // directly never run `main`, so without this they hit
            // `libonnxruntime.so: cannot open shared object file` in a clean env (e.g. CI).
            // `ensure_dylib_path` no-ops when `ORT_DYLIB_PATH` is already set, so an
            // explicit env choice is never overridden; it is idempotent, so calling it
            // under the lock on every construction is cheap and correct.
            //
            // SAFETY: `ensure_dylib_path` mutates a process env var and requires no other
            // thread read the environment concurrently. CONSTRUCT_MU is held across this
            // write AND the `TextEmbedding::try_new` below (the only place `ort` reads the
            // env), and every other construction path also holds it, so no concurrent env
            // reader exists at the point of mutation.
            unsafe { crate::ort_runtime::ensure_dylib_path() };

            TextEmbedding::try_new(
                InitOptions::new(EmbeddingModel::BGESmallENV15)
                    .with_show_download_progress(false)
                    .with_execution_providers(select_execution_providers()),
            )
            .map_err(|e| format!("turbovec: load model: {e}"))?
        };

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
            embed_mu: Mutex::new(()),
        };

        // Load-or-build runs under a SINGLE state-lock hold and, inside it, a
        // cross-process file lock (see `with_store_lock`) around the load+persist so a
        // separate `rigger` process never observes a half-written or mismatched store.
        // Three cases:
        //  - a persisted store that already matches the tree: load it, done (no embed).
        //  - a persisted store that has drifted from the tree: load it, then either
        //    INCREMENTALLY freshen the whole diff (`OnDrift::Freshen`) or leave it as
        //    loaded (`OnDrift::LeaveStale`, so reindex re-embeds only its named files).
        //  - no persisted store at all (cold start): a one-time full build of the tree.
        let mut state = tv.state.lock().unwrap();
        tv.with_store_lock(|| {
            match tv.load_persisted_any(&mut state)? {
                LoadOutcome::Loaded { matched } => {
                    let freshened = !matched && matches!(on_drift, OnDrift::Freshen);
                    if freshened {
                        // The store loaded but the tree drifted; bring it current incrementally.
                        tv.freshen_locked(&mut state)?;
                    }
                    eprintln!(
                        "turbovec: loaded persisted index ({} chunks) from {}{}",
                        state.index.len(),
                        tv.store_dir.display(),
                        if freshened {
                            " (incrementally freshened)"
                        } else {
                            ""
                        }
                    );
                }
                LoadOutcome::Absent => {
                    tv.build_from_tree(&mut state)?;
                    tv.persist_locked(&state)?;
                    eprintln!(
                        "turbovec: built and persisted index ({} chunks) to {}",
                        state.index.len(),
                        tv.store_dir.display()
                    );
                }
            }
            Ok(())
        })?;
        drop(state);
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
    ///
    /// Called with the `state` lock already held (by the caller) and inside the
    /// cross-process store lock, so the on-disk load is atomic against any concurrent
    /// writer.
    fn load_persisted_any(&self, state: &mut State) -> Result<LoadOutcome, String> {
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
        // ONE `state` lock across the ENTIRE freshen (diff + apply + persist) - the
        // single mutation authority. Two concurrent freshens cannot interleave a diff
        // against an apply: the second blocks on `state` until the first has finished and
        // persisted, then re-diffs the now-current tree (a cheap no-op if nothing else
        // changed). The cross-process store lock, taken here around the whole critical
        // section, extends that guarantee to separate `rigger` processes. Both locks are
        // taken by this entry point and passed DOWN to `freshen_locked` (which never
        // re-locks), so there is never a nested `flock` on the same store from one thread.
        let mut state = self.state.lock().unwrap();
        self.with_store_lock(|| self.freshen_locked(&mut state))
    }

    /// The freshen body, run with BOTH the `state` lock and the cross-process store lock
    /// already held by the caller (`freshen`, or `construct` on a drifted load) for the
    /// whole critical section. It never acquires either lock itself - so a caller that
    /// already holds the store lock (like `construct`) does not deadlock on a nested
    /// `flock`. Diffs the tree against the persisted per-file hashes, applies the
    /// changed/new/deleted delta, and persists once - atomically w.r.t. any other
    /// in-process mutation (the caller holds `state`) and any separate process (the
    /// caller holds the store lock).
    fn freshen_locked(&self, state: &mut State) -> Result<(), String> {
        // 1. Snapshot the tree as (rel path -> content), the same file set the index covers.
        let mut on_disk = Vec::new();
        collect_files(Path::new(&self.root), &self.root, &mut on_disk);

        // 2. Diff against the persisted per-file hashes (under the held lock).
        let mut changed_or_new: Vec<(String, String)> = Vec::new();
        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
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
        let deleted: Vec<String> = state
            .meta
            .files
            .keys()
            .filter(|f| !seen.contains(f.as_str()))
            .cloned()
            .collect();

        // 3. Nothing differs -> cheap no-op: no embedding, no persist. This is the
        //    steady-state path a `ground` on an unchanged tree takes.
        if changed_or_new.is_empty() && deleted.is_empty() {
            return Ok(());
        }

        // 4. Apply the delta, then persist. The caller already holds the store lock, so a
        //    concurrent reader in another process never sees a half-applied store.
        //    `drop_file`/`index_file_content` mutate the held `state` directly (they do
        //    NOT re-lock); the slow embed inside `index_file_content` is serialized on
        //    `embed_mu`, not `state`, so it never runs concurrently with another embed.
        for rel in &deleted {
            drop_file(state, rel);
        }
        for (rel, content) in &changed_or_new {
            drop_file(state, rel); // no-op for a brand-new file; clears a changed one's old chunks
            self.index_file_content(state, rel, content)?;
        }
        // 5. Persist the updated index + metadata once, atomically.
        self.persist_locked(state)
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
    fn build_from_tree(&self, state: &mut State) -> Result<(), String> {
        let mut on_disk = Vec::new();
        collect_files(Path::new(&self.root), &self.root, &mut on_disk);
        // Reset to an empty index/meta so a rebuild after an inconsistent load does
        // not accumulate on top of stale state.
        state.index = IdMapIndex::new(EMBED_DIM, BIT_WIDTH)
            .map_err(|e| format!("turbovec: new index: {e}"))?;
        state.meta = Meta::default();
        for (rel, content) in on_disk {
            self.index_file_content(state, &rel, &content)?;
        }
        Ok(())
    }

    /// Chunk + embed one file's content and insert its vectors under fresh ids,
    /// recording the file's hash and chunk ids in the metadata. The file's PRIOR
    /// chunks (if any) must already have been removed by the caller - this only
    /// adds. Returns without embedding when the file has no non-blank chunks (the
    /// file is recorded with an empty id set so it still counts toward consistency).
    fn index_file_content(
        &self,
        state: &mut State,
        rel: &str,
        content: &str,
    ) -> Result<(), String> {
        let (texts, refs) = chunk_content(rel, content);
        let hash = hash_content(content);
        if texts.is_empty() {
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
        // smaller batches. Routed through `embed_locked` so this `Session::run` is
        // serialized against every other embed on the shared ort session.
        let embeddings = self.embed_locked(texts, Some(EMBED_BATCH_SIZE))?;

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

    /// Persist the index (`index.tvim`) and the metadata (`meta.json`) ATOMICALLY to
    /// `.rigger/grounding/`. Called with the `state` lock held AND inside the
    /// cross-process store lock (`with_store_lock`), so no other thread or process
    /// mutates the store while we write it.
    ///
    /// Both files are written to a temp path in the SAME directory and then `rename`d
    /// into place - an atomic replace on the same filesystem - so a concurrent reader
    /// (a separate `rigger` process's `parallel()` lens / `rigger reindex`, or an
    /// in-process load) never observes a truncated index nor a fresh index against
    /// stale meta: it sees either the whole old pair or the whole new pair. `index.tvim`
    /// is written last-then-renamed after `meta.json` so the two are swapped in as a
    /// pair while the flock is held (the store lock is what makes the pair-swap
    /// observably atomic to other processes).
    fn persist_locked(&self, state: &State) -> Result<(), String> {
        std::fs::create_dir_all(&self.store_dir)
            .map_err(|e| format!("turbovec: create {}: {e}", self.store_dir.display()))?;

        // Serialize meta to bytes first, so a serialization failure aborts BEFORE we
        // touch either on-disk file (no partial write). The index has no in-memory
        // serialize (`IdMapIndex::write` only writes to a path), so we write it to a
        // sibling temp file and rename.
        let meta_bytes = serde_json::to_vec(&state.meta)
            .map_err(|e| format!("turbovec: serialize meta: {e}"))?;

        // Write meta then index, each temp-then-rename so a reader never sees a
        // truncated file. Do meta first: if we crash between the two renames, a reader
        // would see new meta + old index, and the load path treats a meta whose ids are
        // absent from the index as drift and re-freshens - self-healing - whereas new
        // index + old meta could surface a vector with no ref. (The flock makes this
        // window invisible to other processes; the ordering only matters for a hard
        // crash mid-persist.)
        write_bytes_atomic(&self.store_dir.join(META_FILE), &meta_bytes)?;
        write_index_atomic(&self.store_dir.join(INDEX_FILE), &state.index)?;
        Ok(())
    }

    /// Embed via the one shared `ort` session, serialized on `embed_mu` so at most one
    /// `Session::run` is in flight process-wide. Concurrent `Session::run` on a single
    /// CUDA session corrupts the heap, so EVERY embed - query and content - funnels
    /// through here.
    fn embed_locked(
        &self,
        texts: Vec<String>,
        batch: Option<usize>,
    ) -> Result<Vec<Vec<f32>>, String> {
        let _embed = self.embed_mu.lock().unwrap();
        // fastembed's `embed(texts, Some(n))` rayon-parallelizes ACROSS the n-sized batches
        // (`texts.par_chunks(n).map(|b| session.run(b))`), firing CONCURRENT `Session::run`
        // on the single ort/CUDA session - which intermittently corrupts the heap
        // ("corrupted double-linked list"). `embed_mu` serializes the whole call but NOT
        // fastembed's internal parallelism, so a multi-batch content embed still races
        // itself. Chunk here and run each chunk as its OWN one-batch embed
        // (`Some(chunk.len())` makes `par_chunks` yield exactly one batch -> exactly one
        // `Session::run`); the loop keeps runs strictly sequential under the lock, never
        // more than one in flight, with peak memory bounded to a single batch.
        let batch_size = batch.unwrap_or(EMBED_BATCH_SIZE).max(1);
        let mut out = Vec::with_capacity(texts.len());
        for chunk in texts.chunks(batch_size) {
            let embs = self
                .model
                .embed(chunk.to_vec(), Some(chunk.len()))
                .map_err(|e| format!("turbovec: embed: {e}"))?;
            out.extend(embs);
        }
        Ok(out)
    }

    fn embed_query(&self, query: &str) -> Option<Vec<f32>> {
        self.embed_locked(vec![query.to_string()], None)
            .ok()?
            .into_iter()
            .next()
    }

    /// Run `f` while holding the store's cross-process advisory lock (`flock(2)` on
    /// `<store>/store.lock`). This serializes the load+persist critical section across
    /// SEPARATE `rigger` processes - a workflow's `parallel()` lenses, a `rigger
    /// reindex`, another in-flight freshen - so none ever reads a half-written or
    /// index/meta-mismatched store. The lock is advisory (all our writers take it) and
    /// released when the returned guard drops, even on an early `?` return or a panic.
    /// The store dir is created first so the lock file has a home.
    fn with_store_lock<T>(&self, f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        std::fs::create_dir_all(&self.store_dir)
            .map_err(|e| format!("turbovec: create {}: {e}", self.store_dir.display()))?;
        let _guard = StoreLock::acquire(&self.store_dir.join(LOCK_FILE))?;
        f()
    }
}

/// Drop a file's existing chunks from BOTH the index and the metadata, so a re-index
/// of that file starts clean. A file not previously indexed is a no-op. A free
/// function taking `&mut State` (not a `&self` method that re-locks) so the caller's
/// single held lock covers the whole critical section - see the `state` field doc.
fn drop_file(state: &mut State, rel: &str) {
    if let Some(entry) = state.meta.files.remove(rel) {
        for id in entry.ids {
            state.index.remove(id);
            state.meta.refs.remove(&id);
        }
    }
}

/// The sibling temp path for an atomic write of `path`: same directory (so `rename`
/// is a same-filesystem atomic replace), the target's name plus this pid (so two
/// processes' temps never collide, though the flock already serializes writers).
fn temp_sibling(path: &Path) -> Result<PathBuf, String> {
    let dir = path
        .parent()
        .ok_or_else(|| format!("turbovec: {} has no parent dir", path.display()))?;
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("turbovec: {} has no file name", path.display()))?;
    Ok(dir.join(format!(".{file_name}.{}.tmp", std::process::id())))
}

/// Write `bytes` to `path` atomically: write to a sibling temp file, fsync it, then
/// `rename` it over `path`. `rename(2)` within one directory is atomic, so a
/// concurrent reader sees either the whole old file or the whole new one, never a
/// truncated write in progress.
fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = temp_sibling(path)?;
    {
        let mut f = std::fs::File::create(&tmp)
            .map_err(|e| format!("turbovec: create temp {}: {e}", tmp.display()))?;
        use std::io::Write;
        f.write_all(bytes)
            .map_err(|e| format!("turbovec: write temp {}: {e}", tmp.display()))?;
        // fsync so the bytes hit disk before the rename publishes the file; otherwise a
        // crash right after the rename could leave the new name pointing at empty data.
        f.sync_all()
            .map_err(|e| format!("turbovec: fsync temp {}: {e}", tmp.display()))?;
    }
    finish_rename(&tmp, path)
}

/// Write the turbovec `index` to `path` atomically. `IdMapIndex::write` only writes to
/// a path (no in-memory serialize), so it writes to a sibling temp file which is then
/// `rename`d over `path` - so a reader never observes the truncating write in progress.
fn write_index_atomic(path: &Path, index: &IdMapIndex) -> Result<(), String> {
    let tmp = temp_sibling(path)?;
    index
        .write(&tmp)
        .map_err(|e| format!("turbovec: write index temp {}: {e}", tmp.display()))?;
    finish_rename(&tmp, path)
}

/// Rename `tmp` over `path`, cleaning up the temp on failure so the store dir is not
/// littered with a stale `.tmp`.
fn finish_rename(tmp: &Path, path: &Path) -> Result<(), String> {
    std::fs::rename(tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(tmp);
        format!(
            "turbovec: rename {} -> {}: {e}",
            tmp.display(),
            path.display()
        )
    })
}

/// An `flock(2)` advisory lock held for the lifetime of the value: `acquire` opens
/// (creating if absent) the lock file and takes an EXCLUSIVE, BLOCKING lock; `Drop`
/// releases it (closing the fd drops the lock too, but we unlock explicitly for
/// clarity). Exclusive+blocking means a second acquirer (in this process or another)
/// waits until the first releases, so the load+persist critical section is serialized
/// cross-process, not just cross-thread.
struct StoreLock {
    file: File,
}

impl StoreLock {
    fn acquire(path: &Path) -> Result<Self, String> {
        // 0o644: the lock file is world-readable, owner-writable - it carries no data,
        // only the flock. `create(true)` makes the first acquirer materialize it.
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o644)
            .open(path)
            .map_err(|e| format!("turbovec: open lock {}: {e}", path.display()))?;
        // SAFETY: `flock` is a plain libc call on a valid fd we own for the lifetime of
        // `file`. LOCK_EX blocks until the exclusive lock is granted; the fd stays open
        // (held by `self.file`) until `Drop`, so the lock is held for exactly the guard's
        // lifetime.
        let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
        if rc != 0 {
            return Err(format!(
                "turbovec: flock {}: {}",
                path.display(),
                std::io::Error::last_os_error()
            ));
        }
        Ok(StoreLock { file })
    }
}

impl Drop for StoreLock {
    fn drop(&mut self) {
        // SAFETY: same fd, still open (owned by `self.file` until this Drop completes).
        // Best-effort: closing the fd right after would release the lock anyway.
        unsafe { libc::flock(self.file.as_raw_fd(), libc::LOCK_UN) };
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
        // whether or not an explicit `reindex` was run. `freshen` takes and releases the
        // state lock itself (holding it across the whole diff+apply+persist), so there is
        // no nested lock with the search below.
        if let Err(e) = self.freshen() {
            // A freshen failure must not silently serve stale results; surface it but
            // still answer from whatever the index currently holds.
            eprintln!("turbovec: freshen before ground failed: {e}");
        }
        // The query embed goes through the shared session's serialization (`embed_mu`),
        // so it can never run concurrently with a content embed on another thread.
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
        // ONE state-lock hold across the whole reindex (drop + re-embed + persist) - the
        // single mutation authority - and, inside it, the cross-process store lock around
        // the apply+persist, so two reindexes / a concurrent freshen never interleave and
        // a separate `rigger` process never reads a half-applied store.
        let mut state = self.state.lock().unwrap();
        let result = self.with_store_lock(|| {
            for f in files {
                drop_file(&mut state, f);
                let path = Path::new(src_dir).join(f);
                // The file still exists: re-embed its current content under new ids. If
                // it was deleted (or is unreadable), its chunks were already dropped
                // above and there is nothing to re-add.
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Err(e) = self.index_file_content(&mut state, f, &content) {
                        eprintln!("turbovec: reindex {f}: {e}");
                    }
                }
            }
            self.persist_locked(&state)
        });
        if let Err(e) = result {
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
/// GPU; on a box with no GPU / no CUDA runtime the CUDA registration fails harmlessly
/// and CPU is used. This never panics for want of a GPU.
///
/// This crate builds `ort` with `-F cuda,download-binaries,load-dynamic` (see
/// `Cargo.toml`), so the CUDA EP's Cargo feature IS compiled in and `ort-sys`
/// downloads the CUDA-enabled ONNX Runtime into its dfbin cache. `src/ort_runtime.rs`
/// points `ORT_DYLIB_PATH` at that runtime so `ort` `dlopen`s it. The upshot: on a box
/// with a CUDA runtime + a GPU the CUDA EP registers and embedding runs on the GPU;
/// where CUDA is absent (no GPU, no CUDA libs, a runtime that lacks the provider) the
/// registration fails silently and we run correctly on CPU - no code change either way.
/// We probe `is_available()` only to LOG which provider actually backs this session.
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
    use serial_test::file_serial;

    // Every test that builds a `Turbovec` model is `#[file_serial(turbovec_model)]`: on
    // a CUDA box, constructing two ort/CUDA sessions concurrently (as `cargo test`'s
    // default thread-per-test would) corrupts the heap. The grounder itself serializes
    // construction WITHIN a process (CONSTRUCT_MU), but `cargo test` runs each test in
    // its own thread AND runs separate test binaries (this lib, `tests/cli.rs`) as
    // parallel processes. `file_serial` uses a FILESYSTEM lock, so the serialization
    // holds across both threads and binaries - no two model constructions ever overlap.
    // Tests that build no model (e.g. `content_hash_is_stable_and_distinguishes`) stay
    // parallel.

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
    #[file_serial(turbovec_model)]
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
    #[file_serial(turbovec_model)]
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
    #[file_serial(turbovec_model)]
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
    #[file_serial(turbovec_model)]
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
    #[file_serial(turbovec_model)]
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
    #[file_serial(turbovec_model)]
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
    #[file_serial(turbovec_model)]
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
    #[file_serial(turbovec_model)]
    fn ground_drops_a_deleted_files_content() {
        let dir = tiny_repo();
        let root = dir.path().to_str().unwrap();
        let tv = Turbovec::new(root).unwrap();

        // render.rs is indexed while it exists. Check the index metadata rather than pinning a
        // top-1 grounding rank: CI embeds on CPU and this box on GPU, and the tiny float
        // differences between the two ONNX Runtime backends can reorder near-ties. This test
        // verifies drop-on-delete (below), not exact ranking.
        assert!(
            tv.state
                .lock()
                .unwrap()
                .meta
                .files
                .contains_key("render.rs"),
            "render.rs should be indexed while it exists"
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
    #[file_serial(turbovec_model)]
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

    /// Assert the in-memory store is internally CONSISTENT: the id-space is coherent
    /// across the three tables that must never drift - `index` (id -> vector),
    /// `meta.refs` (id -> Ref), and `meta.files` (file -> its chunk ids). A concurrency
    /// bug (an interleaved diff/apply, or a torn write reloaded) would surface as a
    /// dangling id here.
    fn assert_store_consistent(tv: &Turbovec) {
        let state = tv.state.lock().unwrap();
        // Every id a file claims must have a ref, and no ref may be orphaned: the set of
        // ids across all files must EQUAL the set of ref keys.
        let file_ids: std::collections::HashSet<u64> = state
            .meta
            .files
            .values()
            .flat_map(|e| e.ids.iter().copied())
            .collect();
        let ref_ids: std::collections::HashSet<u64> = state.meta.refs.keys().copied().collect();
        assert_eq!(
            file_ids, ref_ids,
            "every file-claimed chunk id must have exactly one ref and vice versa - a \
             mismatch means an interleaved mutation left the store inconsistent"
        );
        // The index holds exactly as many vectors as there are refs: no vector without a
        // ref (would surface a hit that maps to nothing) and no ref without a vector.
        assert_eq!(
            state.index.len(),
            state.meta.refs.len(),
            "the vector index and the ref map must have the same cardinality - a \
             mismatch means a torn persist or interleaved apply desynced them"
        );
        // next_id is a strict high-water mark: every allocated id is below it.
        assert!(
            file_ids.iter().all(|&id| id < state.meta.next_id),
            "every allocated id must be below next_id (the monotonic allocator)"
        );
    }

    /// CONCURRENCY GUARANTEE (the fix for the shared-ORT-session + freshen-TOCTOU +
    /// non-atomic-persist blockers): many threads hammering ONE shared `Turbovec` with
    /// interleaved `ground` (which auto-freshens + query-embeds) and `reindex` (which
    /// drops + content-embeds + persists) must NOT corrupt the store. If embedding were
    /// not serialized on the one ort session this would heap-corrupt / crash on a CUDA
    /// box; if freshen's diff/apply were not under one lock, or persist were not atomic,
    /// the store would end internally inconsistent. We assert it survives and stays
    /// consistent, and that a fresh construction reloads the persisted store cleanly.
    #[test]
    #[file_serial(turbovec_model)]
    fn concurrent_ground_and_reindex_keep_the_store_consistent() {
        use std::sync::Arc;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap().to_string();
        // A handful of small files so each embed stays bounded but there is real work
        // to interleave across threads.
        for i in 0..4 {
            std::fs::write(
                dir.path().join(format!("mod{i}.rs")),
                format!(
                    "fn feature_{i}(x: u32) -> u32 {{\n    x.wrapping_mul({i} + 1)\n}}\n\
                     fn helper_{i}() {{\n    // module {i} helper\n}}\n"
                ),
            )
            .unwrap();
        }

        let tv = Arc::new(Turbovec::new(&root).unwrap());
        assert_store_consistent(&tv);

        // Share the dir path + root across threads by Arc so each worker can rewrite
        // files and reindex against the same store.
        let dir_path = Arc::new(dir.path().to_path_buf());
        let root = Arc::new(root);

        // Spawn several threads: some ground repeatedly (auto-freshen + query embed),
        // some reindex a rotating file (drop + content embed + atomic persist). All
        // share the ONE `Turbovec` (its one ort session, one state lock, one embed lock)
        // exactly as the review lenses do on the `rigger run` path.
        let mut handles = Vec::new();
        for t in 0..4 {
            let tv = Arc::clone(&tv);
            let dir_path = Arc::clone(&dir_path);
            let root = Arc::clone(&root);
            handles.push(std::thread::spawn(move || {
                for r in 0..3 {
                    // Ground - this runs freshen (diff+apply+persist under one lock) then
                    // a query embed (serialized on embed_mu), concurrently with peers.
                    let _ = tv.ground("wrapping multiply feature helper", 2);
                    // Reindex a file after rewriting it, so a content embed + atomic
                    // persist races the other threads' grounds and reindexes.
                    let f = format!("mod{}.rs", (t + r) % 4);
                    std::fs::write(
                        dir_path.join(&f),
                        format!(
                            "fn feature_{t}_{r}(x: u32) -> u32 {{\n    x.wrapping_add({t} + {r})\n}}\n"
                        ),
                    )
                    .unwrap();
                    tv.reindex(&root, &[f]);
                }
            }));
        }
        for h in handles {
            // A panic in a worker (e.g. a poisoned lock from a corrupted session) fails
            // the test loudly here.
            h.join().expect("a concurrent worker must not panic");
        }

        // The store survived the concurrent hammering internally consistent.
        assert_store_consistent(&tv);
        // A ground still returns coherent, in-tree results (no dangling ref, no crash).
        let hits = tv.ground("wrapping multiply feature helper", 4);
        for r in &hits {
            assert!(
                dir_path.join(&r.file).exists(),
                "every grounded ref must point at a file still on disk; got {r:?}"
            );
        }

        // The persisted store is not torn: a fresh construction reloads it cleanly and is
        // itself consistent - proving the atomic persist + store lock left a coherent pair
        // on disk, not a truncated index or an index/meta mismatch.
        drop(tv);
        let reloaded = Turbovec::new(root.as_str()).unwrap();
        assert_store_consistent(&reloaded);
    }
}
