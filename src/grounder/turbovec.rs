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
//!    The one thing that is NOT a silent fallback is a wholly *missing* runtime dylib:
//!    `ort` `panic!`s if it cannot `dlopen` `libonnxruntime.so`, so `construct` builds the
//!    model inside a `std::panic::catch_unwind` and turns that panic into a clear `Err`
//!    rather than letting it escape (the cleared-cache-post-install edge).
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

use std::collections::{HashMap, HashSet};
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

/// The MACHINE-WIDE advisory lock that serializes ort/CUDA session CONSTRUCTION across
/// every `rigger` process on the box (see the flock in [`Turbovec::construct`]). Building
/// two ort/CUDA sessions at once corrupts the driver heap, and that heap is per-GPU, not
/// per-store - so the lock lives under the OS temp dir where ALL processes share ONE
/// target, regardless of repo, matching the per-GPU scope of the hazard (and the scope the
/// tests' `file_serial(turbovec_model)` uses). It carries no data; it is only a flock
/// target, auto-released when the constructing process exits (even on crash/kill).
fn ort_construct_lock_path() -> std::path::PathBuf {
    std::env::temp_dir().join("rigger-ort-construct.lock")
}

/// Serializes embedding-model CONSTRUCTION across the whole process. `ort`, built with
/// `load-dynamic`, lazily reads `ORT_DYLIB_PATH` on the FIRST session load and is not
/// safe to construct concurrently on a CUDA box (concurrent session creation corrupts
/// the heap). Every `Turbovec::new` takes this lock across BOTH `ensure_dylib_path`'s
/// env write AND `TextEmbedding::try_new`, so no two sessions are built at once and no
/// OTHER GROUNDER-CONSTRUCTION thread reads the env while we mutate it.
///
/// What this lock does NOT and CANNOT guarantee: `std::env::set_var` mutates
/// process-global state, and this mutex only excludes threads that ALSO take it - the
/// grounder's own construction paths. It cannot bar an unrelated thread (a linked C
/// library, an allocator, the runtime) from calling `getenv`/`std::env::var`
/// concurrently; that residual is exactly why `ensure_dylib_path` is `unsafe` and why
/// both its callers arrange to run before any such thread exists (`main` calls it as
/// its first statement, pre-spawn) or with no other env reader plausibly live. The lock
/// closes the in-crate race; the `unsafe` marks the process-global one the lock cannot.
static CONSTRUCT_MU: Mutex<()> = Mutex::new(());

/// Set to `true` the first time a `TextEmbedding` model is successfully built, which is
/// the ONLY moment `ort` loads its runtime dylib and commits its process-global
/// environment. Read by [`ort_was_initialized`] so `main`'s teardown
/// ([`crate::ort_teardown::release_ort_runtime`]) knows whether there is an ORT
/// environment to release before process exit - and skips the release (a clean no-op)
/// on any run that never built a GPU/CPU session (grep grounder, missing runtime, ...).
/// Never reset: once ORT is loaded it stays loaded for the life of the process.
static ORT_INITIALIZED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether a turbovec model was built in this process - i.e. whether `ort` loaded its
/// runtime and committed an environment that must be released before exit. See
/// [`ORT_INITIALIZED`]; `main` uses this to gate the ORT/CUDA teardown so it runs
/// exactly on the runs that need it.
pub fn ort_was_initialized() -> bool {
    ORT_INITIALIZED.load(std::sync::atomic::Ordering::Acquire)
}

/// The embedding PORT: the one seam between the grounder's incremental-index machinery
/// (chunking, the honest content-keyed skip, batched re-embed, persistence) and the
/// concrete embedding model. `Turbovec` depends on this trait, not on `fastembed`
/// directly, so the machinery is exercised in tests by a lightweight counting fake
/// instead of building the multi-hundred-MB ONNX model - which is what lets a test COUNT
/// model invocations to prove the batching and the zero-embed skip.
///
/// Ports-and-adapters: the trait is the port, [`FastEmbedEmbedder`] is the production
/// adapter over `ort`/`fastembed`, and the test module's counting embedder is the test
/// adapter. `Send + Sync` so a `Box<dyn Embedder>` can live inside the `Send + Sync`
/// `Turbovec` the conductor shares across review threads.
trait Embedder: Send + Sync {
    /// A STABLE identity for the embedding this model produces, folded into the per-file
    /// skip key ([`chunk_key`]) so the incremental skip is HONEST: a file is skipped only
    /// when both its content AND the model that would embed it are unchanged. Two builds
    /// of the SAME model share this string (a mere binary reinstall re-embeds nothing);
    /// any change that alters the produced vectors MUST change it (so the stale vectors
    /// are re-embedded). It is derived from the model's own identity, never from the
    /// binary's build id, install time, or the index file's mtime.
    fn identity(&self) -> &str;

    /// Embed ONE batch of chunk texts in a SINGLE model invocation, returning one
    /// `EMBED_DIM`-dimensional vector per input, in input order. The caller
    /// ([`Turbovec::embed_locked`]) has already bounded the slice to a safe batch width
    /// and holds the embed serialization lock, so this runs exactly one `Session::run`.
    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String>;
}

/// The production [`Embedder`]: the real `fastembed`/`ort` model. Its [`identity`] is the
/// model's canonical code plus the embedding dimension, so swapping the embedding model
/// (or its dimension) changes the identity and re-embeds the tree, while rebuilding or
/// reinstalling the SAME binary does not.
///
/// [`identity`]: Embedder::identity
struct FastEmbedEmbedder {
    model: TextEmbedding,
    identity: String,
}

impl Embedder for FastEmbedEmbedder {
    fn identity(&self) -> &str {
        &self.identity
    }

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        // `Some(texts.len())` makes fastembed's internal `par_chunks` yield EXACTLY ONE
        // batch, hence exactly one `Session::run` - the caller loops chunk-by-chunk under
        // `embed_mu` so no two runs are ever in flight (concurrent `Session::run` on one
        // CUDA session corrupts the heap). `.max(1)` guards the empty slice.
        self.model
            .embed(texts.to_vec(), Some(texts.len().max(1)))
            .map_err(|e| format!("turbovec: embed: {e}"))
    }
}

/// The embedding model this grounder uses. Named once here so the model choice and the
/// [`FastEmbedEmbedder::identity`] derived from it can never drift apart.
const EMBEDDING_MODEL: EmbeddingModel = EmbeddingModel::BGESmallENV15;

/// The identity string persisted (folded into [`chunk_key`]) for the current embedding
/// model: its canonical model code (via `fastembed`'s `Display`, e.g.
/// `Xenova/bge-small-en-v1.5`) plus the embedding dimension. Changing the model or the
/// dimension changes this string, so a store built by a different model re-embeds; a
/// rebuild/reinstall of the same model keeps it identical, so nothing re-embeds.
fn fastembed_identity() -> String {
    format!("{EMBEDDING_MODEL}/dim={EMBED_DIM}")
}

/// Build the production [`FastEmbedEmbedder`]: construct the real `fastembed`/`ort`
/// model, serialized process-wide AND machine-wide, and record that `ort` committed its
/// runtime so the exit-time teardown runs. Extracted from construction so the store
/// machinery can be built over a test embedder without this heavy path.
fn build_fastembed_embedder() -> Result<FastEmbedEmbedder, String> {
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
        // Serialize the CUDA session build ACROSS PROCESSES too. `CONSTRUCT_MU` above
        // serializes it within THIS process, but building two ort/CUDA sessions
        // concurrently corrupts the driver heap across SEPARATE processes on one GPU box
        // as well (the concurrent-`rigger step` deadlock, and any `rigger ground` /
        // `rigger canary` / second driver that grounds at the same time). A plain mutex
        // is blind to other processes and the store flock guards store I/O, not the
        // build - so take a MACHINE-WIDE advisory flock: a concurrent grounder BLOCKS
        // here instead of racing the GPU. Held only for this block (released before the
        // store load below), so it never nests with `with_store_lock`, and auto-released
        // if this process dies mid-build.
        let _gpu = StoreLock::acquire(&ort_construct_lock_path())?;
        // Point `ort` (built with `load-dynamic`) at a discovered `libonnxruntime.so`
        // BEFORE the fastembed/`ort` model below first loads the runtime. `main` also
        // calls this, but tests and any other caller that constructs the grounder
        // directly never run `main`, so without this they hit
        // `libonnxruntime.so: cannot open shared object file` in a clean env (e.g. CI).
        // `ensure_dylib_path` no-ops when `ORT_DYLIB_PATH` is already set, so an
        // explicit env choice is never overridden; it is idempotent, so calling it
        // under the lock on every construction is cheap and correct.
        //
        // SAFETY: `ensure_dylib_path` mutates the process env var `ORT_DYLIB_PATH`.
        // CONSTRUCT_MU is held across this write AND the `TextEmbedding::try_new`
        // below (where `ort` reads the env on its first session load), and every
        // other GROUNDER-construction path also holds it - so no OTHER grounder
        // construction, and thus no ort env read WE INITIATE, can race this write.
        // The mutex cannot exclude an unrelated `getenv` from a linked C library or
        // the runtime on some other thread; that residual process-global race is the
        // reason the fn is `unsafe`, and it is minimized because construction happens
        // early (in practice under `main`, which itself calls `ensure_dylib_path`
        // pre-spawn) rather than eliminated by this lock alone.
        unsafe { crate::ort_runtime::ensure_dylib_path() };

        // Build the model, catching the one failure mode that is NOT a `Result::Err`:
        // a wholly MISSING runtime dylib. Both `select_execution_providers`'
        // `is_available()` probe and `TextEmbedding::try_new`'s session load reach
        // `ort`'s `lib_handle()`, whose `dlopen` is `.unwrap_or_else(|e| panic!(...))`
        // - so a runtime that cannot be `dlopen`ed (the narrow cleared-cache-after-
        // install edge) UNWINDS as a raw panic that `try_new`'s `Result` and
        // `is_available().unwrap_or(false)` both fail to catch. `catch_unwind` turns
        // that panic into the SAME clean `Err(String)` we return for any other load
        // failure, degrading gracefully instead of aborting - and, unlike a separate
        // resolvability probe, it observes EXACTLY the load `ort` actually performs, so
        // the two can never disagree. `AssertUnwindSafe` because we consume the closure
        // once and discard everything it borrows on the panic path (nothing is left in a
        // torn state to observe). `ensure_dylib_path` ran just above, so the load below
        // targets the path `ort` will use.
        let build = || {
            TextEmbedding::try_new(
                InitOptions::new(EMBEDDING_MODEL)
                    .with_show_download_progress(false)
                    .with_execution_providers(select_execution_providers()),
            )
            .map_err(|e| format!("turbovec: load model: {e}"))
        };
        // Silence ONLY ort's EXPECTED, CAUGHT dylib-load panic for the duration of
        // this build. `catch_unwind` absorbs the unwind, but the panic HOOK still runs
        // first and would dump `ort`'s raw `lib_handle()` backtrace
        // ("thread '..' panicked at .../ort/src/lib.rs: ... cannot open shared object
        // file") to stderr - alarming noise ahead of the clean, actionable `Err` we
        // return below. A graceful degrade should read as graceful.
        //
        // But a BLANKET no-op hook over this whole multi-second build would also
        // swallow the diagnostic of any UNRELATED thread that happens to panic in this
        // window - a real bug's message, silently lost. So instead of muting the hook,
        // we install a DISCRIMINATING one that FORWARDS every panic to the previous
        // hook EXCEPT ort's dylib-load panic, which alone it swallows. That panic is
        // identified by its payload (see `is_ort_dylib_load_panic`): ort's exact
        // "attempting to load the ONNX Runtime binary" load-failure message - so a
        // genuine session-init panic from ort keeps its backtrace. Everything else keeps
        // its diagnostics. We restore the previous hook after the `catch_unwind`.
        //
        // SAFETY of touching the process-global hook here: we are inside CONSTRUCT_MU
        // (held across this whole block), the only lock every grounder construction
        // takes, so no other grounder build races this swap; and construction runs
        // early (under `main`, pre-spawn - see `ensure_dylib_path`'s contract), so no
        // unrelated thread's panic message is plausibly lost in this narrow window.
        let prev_hook = std::sync::Arc::new(std::panic::take_hook());
        let hook_prev = std::sync::Arc::clone(&prev_hook);
        std::panic::set_hook(Box::new(move |info| {
            // Forward EVERYTHING to the previous hook except ort's own dylib-load
            // panic (the graceful-degrade path we already handle below). That one, and
            // only that one, is swallowed so its raw backtrace never reaches stderr.
            if !is_ort_dylib_load_panic(info) {
                hook_prev(info);
            }
        }));
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(build));
        // Restore the previous hook. We `take_hook()` first to drop our forwarding
        // closure (releasing its `Arc` clone), then reinstall the previous hook - it is
        // recoverable from the `Arc` because this is now its sole strong reference.
        let _ = std::panic::take_hook();
        match std::sync::Arc::try_unwrap(prev_hook) {
            Ok(hook) => std::panic::set_hook(hook),
            // Unreachable in practice (the forwarding closure that held the other clone
            // was just dropped by `take_hook`), but if a clone somehow outlived it, fall
            // back to a forwarding box so the previous hook is still reinstalled.
            Err(shared) => std::panic::set_hook(Box::new(move |info| shared(info))),
        }
        match caught {
            Ok(result) => result?,
            Err(_) => {
                return Err(
                    "turbovec: the ONNX Runtime shared library (libonnxruntime.so) could not \
                     be resolved for loading. It is normally downloaded into \
                     ~/.cache/ort.pyke.io/dfbin/ by the build; if that cache was cleared after \
                     install, rebuild (`cargo build`) to re-fetch it, set ORT_DYLIB_PATH to a \
                     valid libonnxruntime.so, or select `defaults.grounder: grep` to run \
                     without the semantic grounder"
                        .to_string(),
                );
            }
        }
    };

    // The model built, which means `ort` loaded its runtime dylib and committed its
    // process-global environment (`TextEmbedding::try_new` above is the only path that
    // does so). Record it so `ort_teardown::release_ort_runtime` knows an ORT
    // environment exists to release before process exit - see `ORT_INITIALIZED`.
    ORT_INITIALIZED.store(true, std::sync::atomic::Ordering::Release);

    Ok(FastEmbedEmbedder {
        model,
        identity: fastembed_identity(),
    })
}

/// Turbovec grounds semantically: it embeds the codebase into a quantized vector
/// index and returns the chunks nearest a query. The index + its id->Ref map are
/// persisted under `.rigger/grounding/` and loaded on construction when present, so
/// successive `rigger ground` calls reuse the embeddings instead of rebuilding, and
/// [`Self::reindex`] updates them per-file incrementally.
pub struct Turbovec {
    /// The embedding model, behind the [`Embedder`] port. Production wires a
    /// [`FastEmbedEmbedder`] (the real `ort`/`fastembed` model); tests wire a counting
    /// fake so the incremental-index machinery is provable without the heavy model.
    embedder: Box<dyn Embedder>,
    root: String,
    store_dir: PathBuf,
    /// The in-memory index+meta, and the single mutation authority over it. EVERY
    /// mutation (build, freshen, reindex, drop, persist) runs while THIS lock is held
    /// for the whole critical section - the internal helpers take `&mut State`, they
    /// never re-lock - so two freshens / reindexes can never interleave a diff against
    /// an apply. A `ground`'s search takes the same lock, so it also serializes.
    state: Mutex<State>,
    /// Serializes every call into the embedder - the one shared `ort` session's
    /// `Session::run`. Concurrent `Session::run` on a single CUDA session corrupts the
    /// heap, so this is the process-wide "at most one embed at a time" authority: query
    /// embeds (`embed_query`) and content embeds (`index_files`) BOTH take it, held
    /// across the whole `embed` call. It is a separate lock from `state` so a query embed
    /// (which is not under the state lock) still cannot run concurrently with a freshen's
    /// content embed.
    embed_mu: Mutex<()>,
    /// How many times `reload_persisted_locked` has actually run its expensive on-disk
    /// reload (full `IdMapIndex::load` + meta deserialize + consistency scan). The
    /// staleness gate in `freshen_locked` skips the reload when the on-disk stamp is
    /// unchanged, so on the hot no-change `ground` path this counter does NOT advance -
    /// which is exactly the property the perf-regression test observes.
    reload_count: std::sync::atomic::AtomicU64,
}

/// The mutable index state, behind one lock: the quantized index and the sidecar
/// metadata (id->Ref, file->{hash, ids}, the next id to allocate). Kept together so
/// the two never drift - every mutation updates both under the same lock and then
/// persists them together.
struct State {
    index: IdMapIndex,
    meta: Meta,
    /// The (inode, mtime, size) fingerprint of the two on-disk store files as of the
    /// last time THIS in-memory state was synced with disk - refreshed whenever the store is loaded
    /// (`load_persisted_any` / `reload_persisted_locked`) or persisted
    /// (`persist_locked`). `freshen_locked` compares it against a fresh `stat` to
    /// decide whether an external process wrote the store since we last synced; if it
    /// is UNCHANGED, the reload is skipped (our in-memory state is already current).
    /// `None` before the first sync (nothing on disk / never loaded).
    stamp: Option<StoreStamp>,
}

/// A cheap staleness fingerprint of the two on-disk store files: each file's inode,
/// modification time, and size. `freshen_locked` `stat`s the index + meta and compares
/// the result against the [`State::stamp`] cached on the last sync - a mismatch means
/// an EXTERNAL process (a separate `rigger reindex`) rewrote the store, so the
/// expensive `reload_persisted_locked` must run; an exact match means nothing changed,
/// so the reload is skipped. Two `stat`s are orders of magnitude cheaper than the full
/// `IdMapIndex::load` + meta deserialize + consistency scan the reload performs.
///
/// The INODE is the PRIMARY external-write signal; mtime + size are secondary. Every
/// external persist goes through `persist_locked`, which writes to a temp file and
/// `rename`s it into place - an atomic replace that installs a file with a NEW inode.
/// So any real external write necessarily moves the inode, and comparing the inode
/// (alongside mtime + size) closes the pathological "rewritten within the same coarse
/// mtime granularity AND identical byte size" collision that a bare (mtime, size)
/// fingerprint could FALSE-SKIP: even a same-mtime/same-size rewrite lands on a
/// different inode, so the gate still detects it and reloads. mtime + size remain in
/// the comparison as cheap secondary corroboration (they come from the same `stat`).
/// The inode is read from the SAME `metadata()` call as mtime + size, so the gate is
/// still a single cheap `stat` - no extra syscall, no file read. For the hot no-op path
/// (nothing written) all three fields are exactly equal and the gate correctly skips.
#[derive(Clone, PartialEq, Eq)]
struct StoreStamp {
    index: FileStamp,
    meta: FileStamp,
}

/// One file's (inode, mtime, size) fingerprint. The inode is the PRIMARY external-write
/// signal: `persist_locked`'s temp-file-then-`rename` installs a fresh inode on every
/// external write, so a same-mtime/same-size rewrite still moves the inode and is
/// detected; mtime + size are secondary corroboration. All three come from one
/// `metadata()` call. `None`-free: a file that cannot be `stat`ed (absent, or a
/// transient error) is represented by the caller as an absent [`StoreStamp`], never a
/// partial one, so a half-present pair never compares equal to a fully-present one.
#[derive(Clone, PartialEq, Eq)]
struct FileStamp {
    ino: u64,
    mtime: std::time::SystemTime,
    size: u64,
}

impl StoreStamp {
    /// `stat` the index + meta files and fingerprint them, or `None` if EITHER is
    /// missing / unstattable (an incomplete store is never a valid "current" stamp -
    /// treating it as absent forces the reload to run, which is the safe direction).
    fn of(index_path: &Path, meta_path: &Path) -> Option<StoreStamp> {
        Some(StoreStamp {
            index: FileStamp::of(index_path)?,
            meta: FileStamp::of(meta_path)?,
        })
    }
}

impl FileStamp {
    fn of(path: &Path) -> Option<FileStamp> {
        // A single `stat`: the inode (`ino()`), mtime, and size all come off this one
        // `Metadata`, so adding the inode costs no extra syscall. The inode is the
        // primary "was this file rewritten" signal - `persist_locked`'s rename installs
        // a fresh inode on every external write (see `FileStamp` / `StoreStamp` docs).
        use std::os::unix::fs::MetadataExt;
        let md = std::fs::metadata(path).ok()?;
        Some(FileStamp {
            ino: md.ino(),
            mtime: md.modified().ok()?,
            size: md.len(),
        })
    }
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
    /// The two halves are split so the store machinery ([`Self::from_embedder`]) can be
    /// exercised over the [`Embedder`] port by a test fake, without building the model.
    fn construct(root: &str, on_drift: OnDrift) -> Result<Self, String> {
        let embedder = build_fastembed_embedder()?;
        Self::from_embedder(root, Box::new(embedder), on_drift)
    }

    /// Wire a constructed [`Embedder`] into a `Turbovec` and load-or-build the store over
    /// it. Split out of [`Self::construct`] so tests can inject a counting fake and drive
    /// the whole incremental-index path (chunk, honest skip, batched re-embed, persist)
    /// without the multi-hundred-MB model. `on_drift` selects freshen-now vs leave-stale
    /// for a loaded-but-drifted store, exactly as the model-backed path does.
    fn from_embedder(
        root: &str,
        embedder: Box<dyn Embedder>,
        on_drift: OnDrift,
    ) -> Result<Self, String> {
        let store_dir = Path::new(root).join(GROUNDING_DIR);
        let tv = Turbovec {
            embedder,
            root: root.to_string(),
            store_dir,
            state: Mutex::new(State {
                index: IdMapIndex::new(EMBED_DIM, BIT_WIDTH)
                    .map_err(|e| format!("turbovec: new index: {e}"))?,
                meta: Meta::default(),
                stamp: None,
            }),
            embed_mu: Mutex::new(()),
            reload_count: std::sync::atomic::AtomicU64::new(0),
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
                    tv.persist_locked(&mut state)?;
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
        // SELF-HEAL: the index and meta must be internally consistent - the two are
        // persisted meta-then-index, so a hard crash BETWEEN the two renames can leave
        // new meta against the old index (ids in meta that the index lacks, or a
        // cardinality mismatch). A `freshen` would NOT repair that when the tree still
        // matches (it finds no file diff and does nothing), so an inconsistent pair is
        // treated like a corrupt store: `Absent`, forcing `construct` to rebuild the
        // index from the tree. This is the reconciliation the persist ordering promises.
        if let Err(reason) = check_index_meta_consistent(&index, &meta) {
            eprintln!(
                "turbovec: persisted store is internally inconsistent ({reason}); \
                 self-healing by rebuilding the index from the tree"
            );
            return Ok(LoadOutcome::Absent);
        }
        let matched = self.tree_matches(&meta);
        // Install the loaded state either way: when it matches it is used as-is, and
        // when it has drifted it is the BASE that `freshen` updates incrementally
        // (drop deleted files' chunks, re-embed changed/new files) - never a full rebuild.
        state.index = index;
        state.meta = meta;
        // Record the on-disk fingerprint we just adopted, so `freshen_locked`'s
        // staleness gate can later tell whether an EXTERNAL process has written since.
        state.stamp = StoreStamp::of(&index_path, &meta_path);
        Ok(LoadOutcome::Loaded { matched })
    }

    /// Reload the persisted store from disk into `state` at the START of a mutating op,
    /// so the diff/apply that follows works from the LATEST on-disk base rather than a
    /// possibly-stale in-memory snapshot.
    ///
    /// Why this is load-bearing: the conductor holds ONE long-lived `Turbovec` for a
    /// whole `rigger run`, while the workflow's Integrate step runs `rigger reindex` as
    /// a SEPARATE process against the same `.rigger/grounding` store. That subprocess
    /// takes the flock, mutates the on-disk store, and releases it. The long-lived
    /// instance's in-memory state is now BEHIND disk. Without this reload, its next
    /// `freshen`/`reindex` would diff+apply against the stale snapshot and then
    /// `persist` it - CLOBBERING the subprocess's write (a lost update). Reloading here,
    /// under the flock the caller already holds, folds the external write into our base
    /// before we touch it, so it survives.
    ///
    /// Called with BOTH the `state` lock and the cross-process store lock already held
    /// (by `freshen`/`reindex`), so it NEVER re-locks - no nested `flock`, no deadlock.
    /// If no store is on disk yet (nothing persisted), or the on-disk pair is internally
    /// inconsistent (a torn write from a crashed writer - the flock rules out a live
    /// one), the in-memory state is left as-is: there is nothing safe to adopt, and the
    /// mutation + persist that follows writes a consistent pair from what we hold.
    fn reload_persisted_locked(&self, state: &mut State) -> Result<(), String> {
        let index_path = self.store_dir.join(INDEX_FILE);
        let meta_path = self.store_dir.join(META_FILE);
        if !index_path.exists() || !meta_path.exists() {
            return Ok(()); // nothing persisted yet -> keep our in-memory base
        }
        let index = match IdMapIndex::load(&index_path) {
            Ok(i) => i,
            Err(_) => return Ok(()), // corrupt on disk -> do not adopt; persist heals it
        };
        let meta_bytes =
            std::fs::read(&meta_path).map_err(|e| format!("turbovec: read meta: {e}"))?;
        let meta: Meta = match serde_json::from_slice(&meta_bytes) {
            Ok(m) => m,
            Err(_) => return Ok(()), // unreadable meta -> keep in-memory base
        };
        // Only adopt an INTERNALLY CONSISTENT on-disk pair. An inconsistent one is a
        // torn write; adopting it would just re-persist the inconsistency. Keeping our
        // base lets the following persist overwrite it cleanly.
        if check_index_meta_consistent(&index, &meta).is_ok() {
            // Count the reload HERE - at the point a real on-disk reload (full
            // `IdMapIndex::load` + meta deserialize + consistency scan + apply) actually
            // lands - not at the top of the fn. The early returns above (nothing
            // persisted, corrupt index, unreadable meta, or an inconsistent pair) do NOT
            // adopt anything, so counting them would inflate the counter into "attempts"
            // when its name/doc promise "actual reloads". The staleness gate in
            // `freshen_locked` skips this whole fn on the hot no-change path, so this
            // counter staying flat there is the observable that proves the gate skipped.
            self.reload_count
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            state.index = index;
            state.meta = meta;
            // Re-stamp to the fingerprint we just adopted, so the gate's NEXT check
            // compares against the on-disk state we now mirror in memory.
            state.stamp = StoreStamp::of(&index_path, &meta_path);
        }
        Ok(())
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
        // 0. Reload the on-disk store into `state` FIRST, under the held flock, so the
        //    diff below runs against the latest persisted base - not a stale in-memory
        //    snapshot a separate `rigger reindex` process may have moved past. Without
        //    this, a long-lived grounder (held for a whole `rigger run`) would diff the
        //    tree against its stale state and persist over the subprocess's write, losing
        //    it.
        //
        //    GATED on a cheap staleness PRE-CHECK: `ground` is the HOT path (the MCP serve
        //    loop grounds per request over one long-lived Turbovec), and the reload is a
        //    full `IdMapIndex::load` (the whole `.tvim`) + meta deserialize + consistency
        //    scan. On the common no-change no-op path nothing external wrote, so that work
        //    is pure waste. We `stat` the two store files (two syscalls) and reload ONLY
        //    when their (inode, mtime, size) differs from the fingerprint we cached on our
        //    last sync - i.e. an external process wrote since. An external persist's
        //    temp-file-then-rename installs a NEW inode, so even a same-mtime/same-size
        //    rewrite moves the fingerprint and is caught. If the stamp is unchanged, our
        //    in-memory state already mirrors disk and we SKIP the reload.
        //
        //    This is a PRE-CHECK in front of the existing reload, NOT a deferral past the
        //    diff: steps 1-2 below diff the tree against `state.meta`, so the reload must
        //    PRECEDE the diff or an external reindex's refresh would be mis-classified as
        //    a local change. The gate only elides the reload when disk has NOT moved, in
        //    which case there is nothing to fold in and the diff is already correct.
        let on_disk_stamp = StoreStamp::of(
            &self.store_dir.join(INDEX_FILE),
            &self.store_dir.join(META_FILE),
        );
        if on_disk_stamp.is_none() || on_disk_stamp != state.stamp {
            // Either the store is incomplete/unstattable (reload will handle "nothing
            // persisted" safely) or an external write moved the fingerprint - reload.
            self.reload_persisted_locked(state)?;
        }

        // 1. Snapshot the tree as (rel path -> content), the same file set the index covers.
        let mut on_disk = Vec::new();
        collect_files(Path::new(&self.root), &self.root, &mut on_disk);

        // 2. Diff against the persisted per-file skip keys (under the held lock). The key
        //    folds the CURRENT model's identity in front of the content (see `chunk_key`),
        //    so an unchanged file skips only when both its content AND the embedding model
        //    are unchanged: a binary reinstall (same model) skips every file, while a model
        //    swap re-embeds them all.
        let identity = self.embedder.identity();
        let mut changed_or_new: Vec<(String, String)> = Vec::new();
        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for (rel, content) in &on_disk {
            seen.insert(rel.as_str());
            match state.meta.files.get(rel) {
                // Unchanged content AND unchanged model -> skip (no embed).
                Some(entry) if entry.hash == chunk_key(identity, content) => {}
                // Changed, new, or embedded by a different model: queue for a re-embed.
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
        //    concurrent reader in another process never sees a half-applied store. Drop the
        //    deleted files' and the changed files' OLD chunks first (a changed file is
        //    re-embedded, so its stale chunks go; a new file's drop is a no-op), THEN
        //    re-embed the whole changed/new set through `index_files` in one BATCHED pass -
        //    all its chunks across all its files feed the model in `EMBED_BATCH_SIZE`
        //    batches (one invocation per batch) instead of one small invocation per file.
        //    `drop_file`/`index_files` mutate the held `state` directly (they do NOT
        //    re-lock); the embed inside `index_files` is serialized on `embed_mu`, not
        //    `state`, so it never runs concurrently with another embed.
        for rel in &deleted {
            drop_file(state, rel);
        }
        for (rel, _content) in &changed_or_new {
            drop_file(state, rel); // no-op for a brand-new file; clears a changed one's old chunks
        }
        self.index_files(state, &changed_or_new)?;
        // 5. Persist the updated index + metadata once, atomically.
        self.persist_locked(state)
    }

    /// Whether the persisted `meta` still describes the on-disk tree: the same set of
    /// indexable files, each with an unchanged per-file skip key. The key folds the
    /// CURRENT model's identity in front of the content (see [`chunk_key`]), so a store
    /// written by a DIFFERENT embedding model reads as drifted (every key differs) and is
    /// re-embedded, while a rebuild/reinstall of the SAME model over an unchanged tree
    /// matches verbatim. A mismatch means the tree (or the model) drifted out from under
    /// the store, so it cannot be reused as-is.
    fn tree_matches(&self, meta: &Meta) -> bool {
        let identity = self.embedder.identity();
        let mut on_disk = Vec::new();
        collect_files(Path::new(&self.root), &self.root, &mut on_disk);
        if on_disk.len() != meta.files.len() {
            return false;
        }
        for (rel, content) in on_disk {
            match meta.files.get(&rel) {
                Some(entry) if entry.hash == chunk_key(identity, &content) => {}
                _ => return false,
            }
        }
        true
    }

    /// Embed the whole tree once into a fresh index + metadata. Used on a cold
    /// start (no store) or when the persisted store is inconsistent. Replaces the
    /// in-memory state wholesale; the caller persists it. Routes through the batched
    /// [`Self::index_files`] authority, so the cold build feeds the model at its batch
    /// width too - not one small invocation per file.
    fn build_from_tree(&self, state: &mut State) -> Result<(), String> {
        let mut on_disk = Vec::new();
        collect_files(Path::new(&self.root), &self.root, &mut on_disk);
        // Reset to an empty index/meta so a rebuild after an inconsistent load does
        // not accumulate on top of stale state.
        state.index = IdMapIndex::new(EMBED_DIM, BIT_WIDTH)
            .map_err(|e| format!("turbovec: new index: {e}"))?;
        state.meta = Meta::default();
        self.index_files(state, &on_disk)
    }

    /// The ONE embed-and-install authority. Chunk every file in `files`, embed ALL their
    /// chunks in `EMBED_BATCH_SIZE` batches (ONE model invocation per batch, so the
    /// accelerator is fed at its width instead of one small invocation per file), and
    /// install each file's vectors under fresh ids. Every re-embed path - the `freshen`
    /// drifted set, a `reindex`'s named files, and the cold `build_from_tree` - routes
    /// through here, so the chunk-embed-install concern lives in exactly one place.
    ///
    /// Each file's PRIOR chunks must already have been dropped by the caller (this only
    /// adds). Files are processed - and their ids allocated - in the given order, so the
    /// id assignment is byte-identical to a serial per-file walk (determinism: the same
    /// tree yields the same index entries regardless of the batching).
    ///
    /// MEMORY-BOUNDED: chunks accumulate into a pending GROUP that is flushed (embedded +
    /// installed) as soon as it reaches the batch width, and once more at the end - so at
    /// most ~one batch of embeddings is held at a time even for a whole-tree cold build,
    /// while multiple small files still share one batched invocation.
    fn index_files(&self, state: &mut State, files: &[(String, String)]) -> Result<(), String> {
        if files.is_empty() {
            return Ok(());
        }
        // Own the identity up front so no borrow of `self.embedder` lives across the
        // `&mut state` installs below.
        let identity = self.embedder.identity().to_string();
        // The pending group: a flat list of chunk texts, plus per-file staging of
        // (rel, skip key, its refs, the offset of its chunks in the flat list).
        let mut texts: Vec<String> = Vec::new();
        let mut group: Vec<(String, u64, Vec<StoredRef>, usize)> = Vec::new();
        for (rel, content) in files {
            let (chunk_texts, refs) = chunk_content(rel, content);
            let hash = chunk_key(&identity, content);
            let start = texts.len();
            texts.extend(chunk_texts);
            group.push((rel.clone(), hash, refs, start));
            // Flush once the group reaches the accelerator's batch width. A file is never
            // split across a flush (the check is AFTER the whole file is appended), so a
            // file's embeddings are always contiguous for its atomic install.
            if texts.len() >= EMBED_BATCH_SIZE {
                self.flush_group(state, &mut texts, &mut group)?;
            }
        }
        // Flush the tail (the last, sub-batch-width group; also the whole set when the
        // drifted set is smaller than one batch - the common freshen case).
        self.flush_group(state, &mut texts, &mut group)
    }

    /// Embed one pending group's chunk texts in `EMBED_BATCH_SIZE` model invocations and
    /// install each file's slice under fresh ids, then clear the group. A single
    /// `embed_locked` call embeds the whole group (its internal batching yields one
    /// `Session::run` per `EMBED_BATCH_SIZE` chunks); a group at or under the batch width
    /// is thus ONE invocation for all its files - the batching the criterion counts.
    fn flush_group(
        &self,
        state: &mut State,
        texts: &mut Vec<String>,
        group: &mut Vec<(String, u64, Vec<StoredRef>, usize)>,
    ) -> Result<(), String> {
        if group.is_empty() {
            return Ok(());
        }
        let embeddings = self.embed_locked(std::mem::take(texts), Some(EMBED_BATCH_SIZE))?;
        for (rel, hash, refs, start) in std::mem::take(group) {
            let end = start + refs.len();
            self.install_file_chunks(state, &rel, hash, refs, &embeddings[start..end])?;
        }
        Ok(())
    }

    /// Install ONE file's pre-embedded chunks under fresh ids, recording its skip key and
    /// chunk ids in the metadata. A file with no non-blank chunks is recorded with an
    /// empty id set so it still counts toward consistency; `embeddings` must hold exactly
    /// one vector per `ref`, in order.
    ///
    /// ATOMIC w.r.t. `state.meta`: the add-to-index happens FIRST, and NOTHING in
    /// `state.meta` (`refs`, `files`, `next_id`) is touched until that add succeeds. The
    /// chunk ids are allocated from a LOCAL counter seeded at `state.meta.next_id` and the
    /// `(id, StoredRef)` pairs + flat floats are accumulated in LOCALS, so if
    /// `add_with_ids` returns `Err` we `?` out having mutated NOTHING - no orphan ref
    /// stranded in `meta.refs` (which no `FileEntry.ids` would list, so `drop_file` could
    /// never reclaim it), no leaked `next_id`, no partial `FileEntry`. On success we commit
    /// all three together, mirroring exactly the vectors the index accepted.
    fn install_file_chunks(
        &self,
        state: &mut State,
        rel: &str,
        hash: u64,
        refs: Vec<StoredRef>,
        embeddings: &[Vec<f32>],
    ) -> Result<(), String> {
        debug_assert_eq!(
            refs.len(),
            embeddings.len(),
            "one embedding per ref is required"
        );
        if refs.is_empty() {
            state.meta.files.insert(
                rel.to_string(),
                FileEntry {
                    hash,
                    ids: Vec::new(),
                },
            );
            return Ok(());
        }
        // Stage everything in LOCALS, touching NOTHING in `state.meta`. Ids come from a
        // local counter seeded at (but not yet written back to) `state.meta.next_id`, so
        // an add failure below leaves `next_id` - and every other field of `meta` -
        // byte-for-byte unchanged.
        let mut flat = Vec::with_capacity(embeddings.len() * EMBED_DIM);
        let mut ids = Vec::with_capacity(embeddings.len());
        let mut pending_refs: Vec<(u64, StoredRef)> = Vec::with_capacity(embeddings.len());
        let mut next_id = state.meta.next_id;
        for (emb, r) in embeddings.iter().zip(refs) {
            let id = next_id;
            next_id += 1;
            flat.extend_from_slice(emb);
            ids.push(id);
            pending_refs.push((id, r));
        }
        // Add to the index FIRST. Only if this succeeds do we commit to `state.meta`;
        // on failure we `?` out with `state.meta` (refs, files, next_id) untouched, so
        // no ref is ever stranded without a `FileEntry` to reclaim it via `drop_file`.
        state
            .index
            .add_with_ids(&flat, &ids)
            .map_err(|e| format!("turbovec: add: {e}"))?;
        // The add landed: commit refs, the file entry, and the id high-water mark
        // together, so `state.meta` reflects exactly the vectors the index now holds.
        for (id, r) in pending_refs {
            state.meta.refs.insert(id, r);
        }
        state
            .meta
            .files
            .insert(rel.to_string(), FileEntry { hash, ids });
        state.meta.next_id = next_id;
        Ok(())
    }

    /// Chunk + embed ONE file and install its vectors - a thin convenience over the
    /// batched [`Self::index_files`] authority (a one-file "batch"), so the single-file
    /// callers and the per-file atomicity contract read as before while the embed-install
    /// concern stays implemented once. The file's prior chunks must already be dropped.
    #[cfg(test)]
    fn index_file_content(
        &self,
        state: &mut State,
        rel: &str,
        content: &str,
    ) -> Result<(), String> {
        self.index_files(state, &[(rel.to_string(), content.to_string())])
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
    ///
    /// After the write lands, refreshes `state.stamp` to the (inode, mtime, size) of the
    /// files we just wrote (the rename installed fresh inodes), so `freshen_locked`'s
    /// staleness gate treats OUR OWN persist as
    /// "already synced" - a subsequent `ground` on a still-unchanged tree then skips the
    /// reload rather than spuriously reloading the store we just wrote.
    fn persist_locked(&self, state: &mut State) -> Result<(), String> {
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
        // would see new meta + old index, i.e. meta ids the old index lacks. The load
        // path's `check_index_meta_consistent` catches exactly that (a meta ref id
        // absent from the index, or a cardinality mismatch) and self-heals by rebuilding
        // the index from the tree - so this ordering degrades to a rebuild, never to a
        // vector with no ref. (The flock makes this window invisible to other processes;
        // the ordering only matters for a hard crash mid-persist.)
        let meta_path = self.store_dir.join(META_FILE);
        let index_path = self.store_dir.join(INDEX_FILE);
        write_bytes_atomic(&meta_path, &meta_bytes)?;
        write_index_atomic(&index_path, &state.index)?;
        // Cache the fingerprint of what we just wrote so the gate recognizes this state
        // as current (see `State::stamp`) and does not reload our own write next time.
        state.stamp = StoreStamp::of(&index_path, &meta_path);
        Ok(())
    }

    /// Embed via the one shared embedder session, serialized on `embed_mu` so at most one
    /// `Session::run` is in flight process-wide. Concurrent `Session::run` on a single
    /// CUDA session corrupts the heap, so EVERY embed - query and content - funnels
    /// through here.
    fn embed_locked(
        &self,
        texts: Vec<String>,
        batch: Option<usize>,
    ) -> Result<Vec<Vec<f32>>, String> {
        let _embed = self.embed_mu.lock().unwrap();
        // Split into bounded batches and run each as its OWN single model invocation
        // ([`Embedder::embed_batch`] -> exactly one `Session::run`), strictly sequentially
        // under the lock. This bounds a GPU forward pass's peak memory (see
        // `EMBED_BATCH_SIZE`) AND keeps at most one `Session::run` in flight -
        // fastembed's own `embed(texts, Some(n))` would rayon-parallelize ACROSS the
        // n-sized batches, firing CONCURRENT `Session::run` on the single CUDA session
        // ("corrupted double-linked list"); the adapter passes `Some(len)` so each of OUR
        // batches is a single un-parallelized run, and this loop sequences them.
        let batch_size = batch.unwrap_or(EMBED_BATCH_SIZE).max(1);
        let mut out = Vec::with_capacity(texts.len());
        for chunk in texts.chunks(batch_size) {
            out.extend(self.embedder.embed_batch(chunk)?);
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

/// Whether a panic is ort's EXPECTED dylib-load failure - the ONE panic
/// `construct`'s discriminating hook swallows, so its raw backtrace does not clutter
/// stderr ahead of the clean `Err` we return. Every OTHER panic is forwarded to the
/// previous hook and keeps its diagnostics.
///
/// ort's `lib_handle()` does `libloading::Library::new(..).unwrap_or_else(|e|
/// panic!("An error occurred while attempting to load the ONNX Runtime binary at ..."))`
/// when the runtime `.so` cannot be `dlopen`ed. We key SOLELY on that message, NOT on the
/// panic's ort-crate origin: an ort-origin panic can ALSO be a genuine session-init
/// failure (a bad model, CUDA OOM, an internal assert inside `TextEmbedding::try_new`),
/// whose backtrace we must NOT swallow. Only the missing-runtime load panic - the one the
/// graceful degrade exists for - carries this exact message; anything else is forwarded.
fn is_ort_dylib_load_panic(info: &std::panic::PanicHookInfo<'_>) -> bool {
    // ort's exact `lib_handle()` load-failure message. Keying on the payload (not the
    // panic's ort-crate origin) keeps a genuine session-init failure's backtrace intact -
    // we suppress ONLY the expected missing-dylib panic. The payload is a `&str` for the
    // `panic!("literal {}", ..)` at that site (a `String` on some formatting paths).
    let payload = info.payload();
    let msg = payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str));
    matches!(msg, Some(m) if m.contains("attempting to load the ONNX Runtime binary"))
}

/// Whether a loaded `index`/`meta` pair is internally consistent - the invariant a
/// clean persist upholds and a torn one (a crash between the meta and index renames)
/// can break. `Err(reason)` names the first violation for the self-heal log:
///   - every id `meta.refs` claims must be present in the index (a ref without a
///     vector would surface a hit that maps to nothing, or - the torn-write case -
///     a meta id the OLD index never had);
///   - every id a file claims (`meta.files[*].ids`) must have a ref (else a file
///     points at a chunk with no location);
///   - the index and the ref map must have equal cardinality (a surplus vector would
///     have no ref; a surplus ref, no vector).
///
/// A consistent pair passes all three; an inconsistent one is rebuilt from the tree.
fn check_index_meta_consistent(index: &IdMapIndex, meta: &Meta) -> Result<(), String> {
    // Every ref id must exist as a vector in the index.
    for &id in meta.refs.keys() {
        if !index.contains(id) {
            return Err(format!("meta ref id {id} is absent from the index"));
        }
    }
    // Every file-claimed id must have a ref (and thus, by the check above, a vector).
    for (file, entry) in &meta.files {
        for &id in &entry.ids {
            if !meta.refs.contains_key(&id) {
                return Err(format!("file {file:?} claims id {id} with no ref"));
            }
        }
    }
    // The vector count and the ref count must agree exactly.
    if index.len() != meta.refs.len() {
        return Err(format!(
            "index holds {} vectors but meta has {} refs",
            index.len(),
            meta.refs.len()
        ));
    }
    Ok(())
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
    /// grounds on the accepted code - an incremental delta, NOT a full rebuild. Under
    /// the store flock it FIRST reloads the persisted base (so a concurrent external
    /// write is folded in, not clobbered), then drops the named files' old chunks and
    /// re-embeds the ones still on disk through the batched [`Self::index_files`]
    /// authority (so a multi-file reindex feeds the model at its batch width, and the
    /// chunk-embed-install concern lives in one place), then persists once. A file that
    /// no longer exists on disk is dropped (its chunks removed) without re-adding, and no
    /// named file is embedded more than once.
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
            // Reload the on-disk store into `state` FIRST, under the held flock, so the
            // drop+re-embed applies to the LATEST persisted base. A long-lived grounder's
            // in-memory state can be behind disk (another `rigger` process reindexed while
            // we held our instance); without this reload, persisting our stale base would
            // clobber that write. Reloading folds it in so only THIS reindex's named files
            // change and every other on-disk chunk survives.
            self.reload_persisted_locked(&mut state)?;
            // De-duplicate the named files up front, preserving first-occurrence order, so a
            // file named twice is dropped, re-embedded, and installed EXACTLY ONCE. The drop
            // below is idempotent (a second drop of the same rel is a no-op), but
            // `index_files` is NOT: two `(rel, content)` entries would install `rel` twice
            // with NO drop between the two installs, and the second install overwrites
            // `meta.files[rel]` with its own fresh id-run - ORPHANING the first run's vectors
            // (they stay in the index AND in `meta.refs`, but no `FileEntry.ids` lists them,
            // so `drop_file` can never reclaim them) and inflating `next_id`. `index_files`'
            // contract is that each rel's prior chunks are already dropped by the caller, so a
            // duplicated rel violates it; deduping HERE keeps the ingest OUTPUT byte-identical
            // to naming the file once.
            let mut seen = HashSet::new();
            let files: Vec<&String> = files.iter().filter(|f| seen.insert(f.as_str())).collect();
            // Drop every named file's old chunks FIRST (so a changed file's stale vectors
            // are gone and a deleted file stays gone), then collect the ones still on disk
            // and re-embed them ALL in one batched pass.
            for &f in &files {
                drop_file(&mut state, f);
            }
            let mut to_index: Vec<(String, String)> = Vec::new();
            for &f in &files {
                let path = Path::new(src_dir).join(f);
                // The file still exists: queue its current content for a re-embed under new
                // ids. If it was deleted (or is unreadable), its chunks were already dropped
                // above and there is nothing to re-add.
                if let Ok(content) = std::fs::read_to_string(&path) {
                    to_index.push((f.clone(), content));
                }
            }
            // PROPAGATE any embed/add error rather than swallowing it. `index_files` is
            // ATOMIC per file w.r.t. `state.meta` (it stages each file's ids + refs in
            // locals and commits them only AFTER `index.add_with_ids` succeeds), so a
            // failed add leaves `meta` untouched - no orphan ref. Even so, we must still
            // `?` out rather than swallow: the drops above already mutated `state` in
            // memory (the named files' old chunks are gone), so swallowing and persisting
            // would durably write that half-applied delta. `?`-ing out skips the persist
            // and, via the stamp invalidation below, forces the next `freshen` to reload
            // the clean persisted store.
            self.index_files(&mut state, &to_index)?;
            self.persist_locked(&mut state)
        });
        // Any failure in the reload/re-embed/persist critical section is surfaced here and
        // aborts BEFORE the persist for this reindex runs (a failed add / persist `?`s out
        // above). But a mid-batch failure leaves `state` DIVERGED from disk: `drop_file`
        // already removed some files' chunks in memory, yet nothing was persisted, and
        // `state.stamp` still equals disk (the reload at the top adopted it, or a prior
        // persist set it). The next `ground`'s `freshen_locked` staleness gate would then
        // see stamp == disk and SKIP the repairing reload, serving from the diverged
        // in-memory state. INVALIDATE the stamp (to the `None` sentinel - it is
        // `Option<StoreStamp>`, and `StoreStamp::of` never yields `None` for a present
        // store, so `None` can never equal a real on-disk stamp) so the gate detects the
        // mismatch and reloads the clean persisted store, discarding the divergence. The
        // SUCCESS path does NOT reach here: `persist_locked` re-stamped to what it wrote,
        // so a normal reindex leaves the stamp valid and forces no spurious next reload.
        if let Err(e) = result {
            state.stamp = None;
            eprintln!("turbovec: reindex: {e}");
        }
    }
}

/// Select the embedding model's execution providers, GPU-first with a CPU fallback.
///
/// We return `[CUDA, CPU]`. fastembed feeds this ordered list to `ort`, whose
/// framework registers each in turn and, on ANY registration failure, *silently
/// falls back* to the next provider (and finally to CPU) rather than erroring - the
/// dispatch's default is `fail_silently`. So on a CUDA box the model runs on the
/// GPU; on a box with no GPU (but a loadable runtime) the CUDA registration fails
/// harmlessly and CPU is used. Registration never panics for want of a GPU.
///
/// The one case that IS a panic - not a want of GPU but a want of the *runtime dylib*
/// itself - is handled by the CALLER: `is_available()` reaches `ort`'s `lib_handle()`,
/// whose `dlopen` `panic!`s if `libonnxruntime.so` cannot be loaded. `Turbovec::construct`
/// invokes this function (and `TextEmbedding::try_new`) inside a `catch_unwind`, so that
/// panic becomes a clean `Err` there rather than escaping. This function itself just
/// probes `is_available()` to LOG the backing provider; the `unwrap_or(false)` catches a
/// benign `Err` (runtime present but unqueryable), while the missing-dylib PANIC is left
/// for the caller's `catch_unwind` to turn into a graceful error - a single guard that
/// observes exactly the load `ort` performs, with no separate probe that could disagree.
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
    // `is_available()` reports whether the loaded ONNX Runtime was COMPILED with CUDA
    // support. It reaches `ort`'s `lib_handle()`, which `panic!`s if the runtime dylib
    // cannot be `dlopen`ed; `construct` calls this function inside a `catch_unwind`, so
    // that panic is caught there, not here. A benign `Err` (runtime present but
    // unqueryable) degrades to `false` via `unwrap_or` and we report CPU.
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

/// Read every indexable file under `root` as (repo-relative path, content), scoped to the
/// project's own sources and skipping unreadable (binary) files. The single source of truth for
/// "what the index covers", shared by the cold build and the load-time consistency check so the
/// two never disagree about the file set.
///
/// The scope lives in the SHARED [`super::walk_guarded`] skeleton (the same one grep and the
/// ingests use), so the traversals can never drift. This walk's ONLY leaf action is to read each
/// file and, when it decodes as UTF-8 (skipping binary / unreadable files), push its
/// `(repo-relative path, content)`. It always walks the whole (scoped) tree (leaf action returns
/// `Continue`). The scope skips hidden dotdirs - among them `.fastembed_cache` (the ~128 MB
/// embedding-model cache) so `freshen` never hashes it and a cold build never embeds its JSON
/// blobs, plus the other tooling dotdirs (`.github`/`.cargo`/`.claude`) and `.git`/`.rigger` - and
/// honors the repository's own `.gitignore`; symlinks are not followed, so a link cycle terminates
/// and nothing escapes the root.
fn collect_files(dir: &Path, root: &str, out: &mut Vec<(String, String)>) {
    // The walk always runs to completion (the leaf action never `Break`s), so the
    // `ControlFlow` result is `Continue` and discarded.
    let _ = super::walk_guarded(dir, &mut |path| {
        if let Ok(content) = std::fs::read_to_string(path) {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .into_owned();
            out.push((rel, content));
        }
        std::ops::ControlFlow::Continue(())
    });
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

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// One rolling FNV-1a step: fold `bytes` into `hash` and return the updated value.
/// Seeding from a prior hash lets [`chunk_key`] fold the model identity in front of the
/// content in a SINGLE rolling hash without a second primitive.
fn fnv1a(mut hash: u64, bytes: &[u8]) -> u64 {
    for &byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// A stable content hash for staleness detection: same bytes -> same hash, across
/// processes and machines. Uses a fixed-seed FNV-1a so the value persisted in
/// `meta.json` compares equal on a later run (unlike `DefaultHasher`, whose seed is
/// not guaranteed stable across builds). [`chunk_key`] builds the persisted skip key on
/// top of this by folding the embedding-model identity in front of the content.
///
/// Collision window: FNV-1a is a NON-cryptographic 64-bit hash used here only as a
/// change ORACLE ("did this file's bytes change since we indexed it?"). Two DIFFERENT
/// contents that hash equal (a ~1-in-2^64 accident, not adversarial input - these are
/// source files, not attacker-chosen) would be judged "unchanged" and skip a
/// re-embed, so grounding could serve the stale chunk for that one file until its next
/// real edit shifts the hash. The blast radius is a single file's freshness, self-
/// heals on the next edit, and the odds are negligible for a repo's file count, so a
/// stronger/wider hash is not worth the cost; left as FNV-1a deliberately.
fn hash_content(content: &str) -> u64 {
    fnv1a(FNV_OFFSET, content.as_bytes())
}

/// The persisted per-file SKIP KEY: a stable hash of the embedding-model identity
/// FOLLOWED by the file's content. Folding the model identity into the key is what makes
/// the incremental skip HONEST - a file is skipped (not re-embedded) only when BOTH its
/// content AND the model that would embed it are unchanged:
///
/// - A mere binary REINSTALL over an unchanged tree (same model, same bytes) yields the
///   SAME key for every file, so `freshen` skips them all and embeds ZERO chunks. The key
///   is a pure function of (model identity, content) - NEVER of the binary's build id,
///   install time, or the index file's mtime.
/// - Swapping the embedding MODEL (a different [`Embedder::identity`]) changes the key for
///   every file, so `freshen` re-embeds the whole tree - the index never keeps stale
///   vectors the current model never produced.
///
/// The identity is folded in FRONT of the content with a NUL separator that cannot
/// appear in the identity string, so `(identity="a", content="bc")` can never collide
/// with `(identity="ab", content="c")` - the boundary is unambiguous. It seeds FNV-1a
/// with the identity and the NUL, then folds in the file's [`hash_content`] - reusing the
/// one content-hash primitive (and its fixed-seed, stable-across-builds guarantee) rather
/// than a second content hash. The result is a 64-bit key with FNV-1a's collision
/// profile in both the identity and the content dimensions.
fn chunk_key(model_identity: &str, content: &str) -> u64 {
    let seeded = fnv1a(FNV_OFFSET, model_identity.as_bytes());
    let separated = fnv1a(seeded, &[0u8]);
    fnv1a(separated, &hash_content(content).to_le_bytes())
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

    /// The per-file SKIP KEY folds the model identity in: identical (identity, content)
    /// keys equal (so a same-model reinstall over an unchanged tree skips), a content
    /// change changes the key (so an edit re-embeds), AND a model-identity change changes
    /// the key (so a model swap re-embeds). The NUL separator keeps the identity/content
    /// boundary unambiguous, so a shift of bytes across it is not aliased as unchanged.
    #[test]
    fn chunk_key_folds_model_identity_and_content() {
        // Stable for the same (identity, content).
        assert_eq!(
            chunk_key("model-v1", "fn a() {}"),
            chunk_key("model-v1", "fn a() {}")
        );
        // A content change moves the key (an edit is detected).
        assert_ne!(
            chunk_key("model-v1", "fn a() {}"),
            chunk_key("model-v1", "fn b() {}")
        );
        // A model-identity change moves the key (a model swap re-embeds).
        assert_ne!(
            chunk_key("model-v1", "fn a() {}"),
            chunk_key("model-v2", "fn a() {}")
        );
        // The identity/content boundary is unambiguous: moving a byte across it changes
        // the key, so ("ab","c") and ("a","bc") never alias.
        assert_ne!(chunk_key("ab", "c"), chunk_key("a", "bc"));
    }

    // ---- HONEST EMBED SKIP (spec 49 criterion 3) ----------------------------------
    //
    // These tests drive the incremental-index machinery over the [`Embedder`] PORT with
    // a COUNTING FAKE - no ONNX model is built - so they run fast, in parallel (no
    // `file_serial`), and can COUNT model invocations to prove (1) the skip is honest
    // (content + model identity keyed, never binary identity) and (2) the drifted set
    // re-embeds in BATCHES (one invocation per batch, not one per file/chunk).

    /// A test [`Embedder`] that records how many times its model was invoked
    /// (`embed_batch` calls) and how many texts it embedded in total, and returns a
    /// deterministic vector per text so the real index accepts it. Its `identity`
    /// stands in for the embedding model's identity - two fakes with the SAME identity
    /// are "the same model rebuilt into a different binary"; a DIFFERENT identity is a
    /// model swap.
    struct CountingEmbedder {
        identity: String,
        invocations: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        embedded_texts: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    /// A deterministic, non-degenerate `EMBED_DIM`-vector seeded from the text, so the
    /// quantized index accepts it and identical text always embeds identically.
    fn deterministic_embedding(text: &str) -> Vec<f32> {
        let mut h = 0xcbf2_9ce4_8422_2325u64 ^ (text.len() as u64);
        for b in text.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        (0..EMBED_DIM)
            .map(|_| {
                // A plain LCG step per lane keeps the vector varied and stable.
                h = h
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                ((h >> 33) as f32 / (u32::MAX as f32)) * 2.0 - 1.0
            })
            .collect()
    }

    impl Embedder for CountingEmbedder {
        fn identity(&self) -> &str {
            &self.identity
        }
        fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
            // ONE invocation per call - this is the "model invocation" the batching
            // criterion counts. A batched caller feeds many texts per call; a per-chunk
            // or per-file caller makes many calls of few texts each.
            self.invocations
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            self.embedded_texts
                .fetch_add(texts.len(), std::sync::atomic::Ordering::Relaxed);
            Ok(texts.iter().map(|t| deterministic_embedding(t)).collect())
        }
    }

    /// Build a counting embedder plus the shared counters the test reads after the box
    /// is moved into the `Turbovec`.
    fn counting_embedder(
        identity: &str,
    ) -> (
        Box<dyn Embedder>,
        std::sync::Arc<std::sync::atomic::AtomicUsize>,
        std::sync::Arc<std::sync::atomic::AtomicUsize>,
    ) {
        let invocations = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let embedded_texts = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let embedder = Box::new(CountingEmbedder {
            identity: identity.to_string(),
            invocations: std::sync::Arc::clone(&invocations),
            embedded_texts: std::sync::Arc::clone(&embedded_texts),
        });
        (embedder, invocations, embedded_texts)
    }

    fn load(c: &std::sync::Arc<std::sync::atomic::AtomicUsize>) -> usize {
        c.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// (1) HONEST SKIP: a freshen over an UNCHANGED tree embeds ZERO chunks even though
    /// the store was written by a DIFFERENT binary (a fresh embedder instance) - because
    /// the skip key is content + MODEL identity, never the binary's identity. This is the
    /// "a reinstall over an unchanged tree embeds zero chunks" guarantee.
    #[test]
    fn honest_skip_reinstall_over_unchanged_tree_embeds_zero() {
        let dir = tiny_repo();
        let root = dir.path().to_str().unwrap();

        // Binary #1 builds + persists the store (a cold build embeds every file).
        {
            let (e, inv, _txt) = counting_embedder("model-v1");
            let built = Turbovec::from_embedder(root, e, OnDrift::Freshen).unwrap();
            assert!(load(&inv) > 0, "the cold build must embed the tree");
            drop(built);
        }

        // Binary #2: a DIFFERENT process (a fresh embedder instance) with the SAME model
        // identity re-opens the SAME store over the SAME tree. The store carries no binary
        // identity - only the model-keyed per-file skip key - so construction LOADS the
        // matching store and embeds NOTHING.
        let (e, inv, txt) = counting_embedder("model-v1");
        let reopened = Turbovec::from_embedder(root, e, OnDrift::Freshen).unwrap();
        assert_eq!(
            load(&inv),
            0,
            "re-opening an unchanged store with a different binary (same model) must embed \
             zero chunks - the skip key is content + model identity, never binary identity"
        );

        // An explicit freshen on the still-unchanged tree also embeds nothing.
        reopened.freshen().unwrap();
        assert_eq!(
            load(&inv),
            0,
            "a freshen over an unchanged tree must embed zero chunks"
        );
        assert_eq!(
            load(&txt),
            0,
            "no text may be embedded on the honest-skip path"
        );
    }

    /// (2) HONEST SKIP folds MODEL IDENTITY: swapping the embedding model (a different
    /// identity) over the SAME unchanged tree RE-EMBEDS every file - otherwise the index
    /// would keep stale vectors the new model never produced. This is the half a bare
    /// content hash cannot express, and the reason the model identity is folded into the
    /// skip key.
    #[test]
    fn honest_skip_model_change_re_embeds_the_whole_tree() {
        let dir = tiny_repo();
        let root = dir.path().to_str().unwrap();

        // Build the store with model v1.
        let total_chunks = {
            let (e, _inv, txt) = counting_embedder("model-v1");
            let built = Turbovec::from_embedder(root, e, OnDrift::Freshen).unwrap();
            let n = load(&txt);
            drop(built);
            n
        };
        assert!(total_chunks > 0, "the tiny repo must have produced chunks");

        // Re-open the SAME store over the SAME tree but with a DIFFERENT model identity.
        // Every file's stored skip key was folded with v1; recomputed with v2 it differs,
        // so construction (OnDrift::Freshen) re-embeds the WHOLE tree.
        let (e, inv, txt) = counting_embedder("model-v2");
        let _swapped = Turbovec::from_embedder(root, e, OnDrift::Freshen).unwrap();
        assert!(
            load(&inv) > 0,
            "a model swap over an unchanged tree MUST re-embed - the model identity is \
             folded into the skip key so stale vectors are never kept"
        );
        assert_eq!(
            load(&txt),
            total_chunks,
            "a model swap must re-embed EVERY chunk the tree produces, no more, no less"
        );
    }

    /// (3) BATCHED RE-EMBED: when SEVERAL files drift, the whole drifted set embeds in
    /// ONE batched model invocation (all chunks fit in a single `EMBED_BATCH_SIZE`
    /// batch), NOT one invocation per file. Under the old per-file loop this was N
    /// invocations; batching feeds the accelerator at its width.
    #[test]
    fn drifted_set_embeds_in_one_batched_invocation_not_per_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        // Several one-chunk files - fewer than EMBED_BATCH_SIZE so a single batch holds
        // them all.
        let n_files = 5usize;
        assert!(n_files <= EMBED_BATCH_SIZE);
        for i in 0..n_files {
            std::fs::write(
                dir.path().join(format!("m{i}.rs")),
                format!("fn f{i}() {{}}\n"),
            )
            .unwrap();
        }

        let (e, inv, txt) = counting_embedder("model-v1");
        let tv = Turbovec::from_embedder(root, e, OnDrift::Freshen).unwrap();

        // Reset the counters: measure only the freshen that follows the multi-file edit.
        inv.store(0, std::sync::atomic::Ordering::Relaxed);
        txt.store(0, std::sync::atomic::Ordering::Relaxed);

        // Edit ALL files so the whole set drifts at once.
        for i in 0..n_files {
            std::fs::write(
                dir.path().join(format!("m{i}.rs")),
                format!("fn f{i}() {{}}\nfn g{i}() {{}}\n"),
            )
            .unwrap();
        }
        tv.freshen().unwrap();

        assert_eq!(
            load(&txt),
            n_files,
            "each of the {n_files} one-chunk files must be re-embedded exactly once"
        );
        assert_eq!(
            load(&inv),
            1,
            "the whole drifted set must embed in ONE batched invocation (all {n_files} \
             chunks in a single batch), not one invocation per file"
        );
    }

    /// (4) BATCHED + SCOPED: a freshen after ONE file changes embeds ONLY that file's
    /// chunks (the honest per-file skip), and does so in a single batched invocation. The
    /// UNCHANGED file is not re-embedded (its ids are preserved).
    #[test]
    fn single_file_change_re_embeds_only_that_file_batched() {
        let dir = tiny_repo();
        let root = dir.path().to_str().unwrap();
        let (e, inv, txt) = counting_embedder("model-v1");
        let tv = Turbovec::from_embedder(root, e, OnDrift::Freshen).unwrap();

        let combat_ids_before = file_ids(&tv, "combat.rs");
        assert!(!combat_ids_before.is_empty());

        inv.store(0, std::sync::atomic::Ordering::Relaxed);
        txt.store(0, std::sync::atomic::Ordering::Relaxed);

        // Change ONLY render.rs (add a second, still-single-chunk function).
        std::fs::write(
            dir.path().join("render.rs"),
            "fn draw_sprite() {}\nfn blit_overlay() {}\n",
        )
        .unwrap();
        tv.freshen().unwrap();

        let render_chunks = file_ids(&tv, "render.rs").len();
        assert_eq!(
            load(&txt),
            render_chunks,
            "only the changed file's chunks may be embedded"
        );
        assert_eq!(
            load(&inv),
            1,
            "the changed file's chunks embed in one batched invocation, not one per chunk"
        );
        assert_eq!(
            file_ids(&tv, "combat.rs"),
            combat_ids_before,
            "the unchanged file must NOT be re-embedded - its chunk ids are preserved"
        );
    }

    /// (5) DUPLICATE-ARG REINDEX is idempotent: naming the SAME file twice in ONE
    /// `reindex` call embeds and installs it EXACTLY ONCE. A repeated arg must never
    /// leave an ORPHAN vector (a `meta.refs` id no `FileEntry` claims - the class of
    /// corruption a drop-all-then-install-all pass invites, since the drop is idempotent
    /// but the install is not), never inflate `next_id`, and never surface the file
    /// twice in a search. The ingest OUTPUT is a pure function of the tree, not of how
    /// many times the file was named.
    #[test]
    fn reindex_duplicate_arg_installs_the_file_exactly_once() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        // A distinctive term so a search can look for exactly this file's content.
        std::fs::write(
            dir.path().join("dup.rs"),
            "fn teleport_across_the_void() {}\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("other.rs"), "fn draw_the_hud() {}\n").unwrap();

        let (e, _inv, _txt) = counting_embedder("model-v1");
        let tv = Turbovec::from_embedder(root, e, OnDrift::Freshen).unwrap();

        // dup.rs is a single-chunk file; capture its chunk count and next_id right before
        // the duplicated reindex so we can prove the second mention added NOTHING.
        let dup_chunks = file_ids(&tv, "dup.rs").len() as u64;
        assert_eq!(dup_chunks, 1, "the fixture file must be exactly one chunk");
        let next_id_before = next_id(&tv);

        // Name dup.rs TWICE in one reindex. Under the pre-fix drop-all-then-install-all
        // path this dropped it once but installed it twice with no drop between the
        // installs, orphaning the first install's ids and doubling next_id's advance.
        tv.reindex(root, &["dup.rs".to_string(), "dup.rs".to_string()]);

        // (a) NO ORPHAN REFS: every id in `meta.refs` is claimed by exactly one
        // `FileEntry`, and the index / refs / claimed-id cardinalities all agree. The
        // orphan check is the decisive one - the pre-fix store passed the weaker
        // index.len() == refs.len() check because the orphans sat in BOTH.
        {
            let state = tv.state.lock().unwrap();
            let mut claimed: Vec<u64> = state
                .meta
                .files
                .values()
                .flat_map(|entry| entry.ids.iter().copied())
                .collect();
            let claimed_set: HashSet<u64> = claimed.iter().copied().collect();
            for &id in state.meta.refs.keys() {
                assert!(
                    claimed_set.contains(&id),
                    "meta.ref id {id} is ORPHANED - claimed by no FileEntry (double-install leak)"
                );
            }
            // No id is claimed by two files (the duplicated install would have listed the
            // same rel's ids twice were the entry not overwritten wholesale).
            claimed.sort_unstable();
            let claimed_len = claimed.len();
            claimed.dedup();
            assert_eq!(
                claimed_len,
                claimed.len(),
                "an id is claimed by more than one FileEntry"
            );
            // Every vector has a ref and every ref is a claimed vector - no stranding.
            assert_eq!(
                state.index.len(),
                state.meta.refs.len(),
                "index vector count and ref count diverged"
            );
            assert_eq!(
                state.meta.refs.len(),
                claimed.len(),
                "a ref exists that no FileEntry claims"
            );
        }

        // (b) next_id ADVANCED BY ONE FILE, NOT TWO: the reindex dropped dup.rs and
        // re-added it once, so the id high-water mark rose by exactly its chunk count.
        assert_eq!(
            next_id(&tv),
            next_id_before + dup_chunks,
            "the duplicated arg inflated next_id - the file was installed more than once"
        );
        // dup.rs still owns exactly its chunk count of ids (one live install).
        assert_eq!(
            file_ids(&tv, "dup.rs").len() as u64,
            dup_chunks,
            "dup.rs must own exactly one install's worth of ids"
        );

        // (c) NO DUPLICATE SEARCH HITS: grounding for dup.rs's exact content returns it
        // at most once. With an orphan vector (identical content -> identical embedding)
        // the pre-fix store would surface dup.rs twice among the top hits.
        let hits = tv.ground("fn teleport_across_the_void() {}", 5);
        let dup_hits = hits.iter().filter(|r| r.file == "dup.rs").count();
        assert!(
            dup_hits <= 1,
            "dup.rs surfaced {dup_hits} times - a duplicate/orphan vector is in the index"
        );
    }

    /// (6) BATCH-BOUNDARY DETERMINISM: a set of MORE than `EMBED_BATCH_SIZE` chunks
    /// routed through `index_files` (a) fires MORE THAN ONE model invocation - the
    /// pending group flushes at the batch width AND again for the tail, so the
    /// accelerator is fed across a real flush boundary, not in one oversized call - and
    /// (b) assigns every file's chunk ids BYTE-IDENTICALLY to a serial per-file walk
    /// (installing one file at a time). This proves `flush_group`'s `start..end` slice
    /// arithmetic is correct ACROSS multiple groups, including for multi-chunk files at a
    /// group's edge: batching changes the invocation CADENCE, never the index CONTENT or
    /// the id ORDER.
    #[test]
    fn index_files_batches_across_the_flush_boundary_with_serial_identical_ids() {
        // Content producing EXACTLY `chunks` chunks: (chunks-1)*CHUNK_LINES + 1 non-blank
        // lines span that many 40-line slices, each uniquely named so none trims empty.
        fn content_with_chunks(tag: &str, chunks: usize) -> String {
            let n = (chunks - 1) * CHUNK_LINES + 1;
            (0..n).map(|j| format!("fn {tag}_{j}() {{}}\n")).collect()
        }

        // A mixed set whose FIRST group fills to EXACTLY the batch width (32) so its flush
        // is one clean invocation, then a tail group of 8. Multi-chunk files sit at the
        // group's trailing edge (f30, width 2) and the next group's leading edge (f31,
        // width 3), so the `start..end` slicing is exercised with width > 1 in BOTH groups
        // and across the boundary. Total: 30 + 2 + 3 + 5 = 40 chunks in 37 files.
        let mut files: Vec<(String, String)> = Vec::new();
        for i in 0..30 {
            files.push((
                format!("f{i:02}.rs"),
                content_with_chunks(&format!("a{i}"), 1),
            ));
        }
        files.push(("f30.rs".to_string(), content_with_chunks("b", 2)));
        files.push(("f31.rs".to_string(), content_with_chunks("c", 3)));
        for i in 32..37 {
            files.push((
                format!("f{i:02}.rs"),
                content_with_chunks(&format!("d{i}"), 1),
            ));
        }
        let total_chunks: usize = 40;
        assert!(
            total_chunks > EMBED_BATCH_SIZE,
            "the set must exceed one batch to force a flush boundary"
        );

        // BATCHED path: one empty store, install the whole set through `index_files` in
        // one call, counting model invocations.
        let empty_a = tempfile::tempdir().unwrap();
        let (eb, inv_b, _txt_b) = counting_embedder("model-v1");
        let batched =
            Turbovec::from_embedder(empty_a.path().to_str().unwrap(), eb, OnDrift::Freshen)
                .unwrap();
        inv_b.store(0, std::sync::atomic::Ordering::Relaxed);
        {
            let mut state = batched.state.lock().unwrap();
            batched.index_files(&mut state, &files).unwrap();
        }

        // (a) The flush boundary was crossed: the group flushed at width 32 (one
        // invocation) and the 8-chunk tail flushed once more - exactly two invocations.
        // Since every flushed group is <= the batch width, invocation count equals the
        // number of `flush_group` calls, so 2 proves TWO groups were embedded (a single
        // oversized call would be one group; a per-file loop would be 37).
        let invocations = load(&inv_b);
        assert!(
            invocations > 1,
            "a >batch-width set must fire more than one model invocation (was {invocations})"
        );
        assert_eq!(
            invocations, 2,
            "the set must embed as two batched groups (width-32 flush + 8-chunk tail), \
             not one oversized call and not one call per file (was {invocations})"
        );

        // SERIAL path: a second empty store, install the SAME files ONE AT A TIME (each a
        // single-file batch) in the SAME order - the per-file walk the ids must match.
        let empty_b = tempfile::tempdir().unwrap();
        let (es, _inv_s, _txt_s) = counting_embedder("model-v1");
        let serial =
            Turbovec::from_embedder(empty_b.path().to_str().unwrap(), es, OnDrift::Freshen)
                .unwrap();
        {
            let mut state = serial.state.lock().unwrap();
            for (rel, content) in &files {
                serial.index_file_content(&mut state, rel, content).unwrap();
            }
        }

        // (b) BYTE-IDENTICAL id assignment: every file owns the SAME ids under the batched
        // walk as under the serial walk, next_id lands at the same high-water mark, and
        // the index holds the same vector count. Batching moved the cadence, not a byte.
        let ids_of = |tv: &Turbovec| -> std::collections::BTreeMap<String, Vec<u64>> {
            let state = tv.state.lock().unwrap();
            state
                .meta
                .files
                .iter()
                .map(|(rel, entry)| (rel.clone(), entry.ids.clone()))
                .collect()
        };
        assert_eq!(
            ids_of(&batched),
            ids_of(&serial),
            "batched id assignment diverged from the serial per-file walk"
        );
        assert_eq!(
            next_id(&batched),
            next_id(&serial),
            "batched next_id diverged from the serial walk"
        );
        assert_eq!(
            next_id(&batched),
            total_chunks as u64,
            "next_id must equal the total chunk count (0..40)"
        );
        assert_eq!(
            batched.state.lock().unwrap().index.len(),
            serial.state.lock().unwrap().index.len(),
            "batched index vector count diverged from the serial walk"
        );
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

    /// How many times this instance has run the expensive on-disk reload. The staleness
    /// gate in `freshen_locked` skips the reload when disk has not moved since our last
    /// sync, so this counter is a precise "did we pay for a reload" probe: unchanged
    /// across a `ground` <=> the gate took the cheap stat-only skip path.
    fn reload_count(tv: &Turbovec) -> u64 {
        tv.reload_count.load(std::sync::atomic::Ordering::Relaxed)
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

    // FINDING #1 (graceful ORT-dylib degradation) is verified OUT OF PROCESS in
    // `tests/cli.rs::ground_degrades_gracefully_when_the_ort_dylib_is_unresolvable`,
    // NOT here. The behavior is real - a missing/unresolvable `libonnxruntime.so` must
    // make grounder construction return a clean `Err` (via `construct`'s `catch_unwind`)
    // rather than aborting - but it CANNOT be tested in this shared lib-test binary: the
    // test must point `ORT_DYLIB_PATH` at a bad path, and `ort` caches the FIRST dylib it
    // dlopens in a process-global `OnceLock` (`G_ORT_DYLIB_PATH` / `G_ORT_LIB`) that no
    // env restore can undo. If such an in-process test won the `file_serial(turbovec_model)`
    // lock race and loaded ort first, it POISONED that global and every later model-building
    // test in this binary failed with `cannot open shared object file`. Spawning a fresh
    // `rigger` subprocess gives the degrade check its OWN ort globals, so it can never
    // poison a sibling. See that CLI test for the assertion.

    /// FINDING #5 (symlink cycle guard): `collect_files` must terminate on a directory
    /// symlink CYCLE rather than loop forever / blow the stack. We build a real cycle -
    /// `sub/loop -> root` (a link back to an ancestor) - and assert the walk returns,
    /// visits the real file exactly once, and does not duplicate it by re-entering
    /// through the link. Root confinement makes this hold BY CONSTRUCTION: the scoped walk
    /// never follows a symlink, so the cycle is never traversed. No model is built, so this
    /// stays parallel.
    #[test]
    fn collect_files_terminates_on_a_symlink_cycle() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("real.rs"), "fn only_once() {}\n").unwrap();
        let sub = root.join("sub");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("nested.rs"), "fn nested_once() {}\n").unwrap();
        // A directory symlink pointing back up at the root: following `sub/loop` would re-enter
        // the whole tree and recurse forever. The scoped walk does not follow it.
        std::os::unix::fs::symlink(root, sub.join("loop")).unwrap();

        // The scoped walk must RETURN (a hang here fails the test by timeout) ...
        let mut out = Vec::new();
        collect_files(root, root.to_str().unwrap(), &mut out);

        // ... and visit each real file exactly once, never re-collecting it through the
        // unfollowed link.
        let real_hits = out
            .iter()
            .filter(|(rel, _)| rel.ends_with("real.rs"))
            .count();
        let nested_hits = out
            .iter()
            .filter(|(rel, _)| rel.ends_with("nested.rs"))
            .count();
        assert_eq!(
            real_hits, 1,
            "the top-level file must be collected exactly once"
        );
        assert_eq!(
            nested_hits, 1,
            "the nested file must be collected exactly once, not re-entered via the cycle"
        );
    }

    /// `.fastembed_cache` (the ~128 MB model cache fastembed writes at the repo root or
    /// at FASTEMBED_CACHE_DIR) and the non-code tooling dotdirs (`.github`/`.cargo`/
    /// `.claude`) must be OMITTED from the index: hashing/embedding the model cache made
    /// every `freshen` hash 128 MB and a cold build embed the cache's JSON blobs (they
    /// surfaced as grounding hits). We seed a repo with a first-party source file plus a
    /// file inside each denied dir and assert only the source file is collected. No model
    /// is built, so this stays parallel.
    #[test]
    fn collect_files_skips_the_model_cache_and_tooling_dotdirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // A genuine source file that MUST be collected.
        std::fs::write(root.join("lib.rs"), "fn real_code() {}\n").unwrap();
        // A file inside each denied dir that must NOT be collected. `.fastembed_cache`
        // stands in for the model cache; the others for CI/cargo/agent config.
        for denied in [".fastembed_cache", ".github", ".cargo", ".claude"] {
            let sub = root.join(denied);
            std::fs::create_dir(&sub).unwrap();
            std::fs::write(sub.join("blob.json"), "{\"weights\": \"...\"}\n").unwrap();
        }

        let mut out = Vec::new();
        collect_files(root, root.to_str().unwrap(), &mut out);

        // Exactly the source file is indexed; nothing from any denied dir leaks in.
        assert_eq!(
            out.iter().map(|(rel, _)| rel.as_str()).collect::<Vec<_>>(),
            vec!["lib.rs"],
            "only first-party source is indexed - the model cache and tooling dotdirs \
             must be denied; got {out:?}"
        );
        for denied in [".fastembed_cache", ".github", ".cargo", ".claude"] {
            assert!(
                !out.iter().any(|(rel, _)| rel.starts_with(denied)),
                "no file under {denied} may be collected"
            );
        }
    }

    /// FINDING #3 (persist self-heal is REAL): a persisted store whose meta and index
    /// DISAGREE - the torn-write shape a crash between the two renames leaves (new meta
    /// referencing ids the OLD index lacks) - must be detected on load and self-healed
    /// by rebuilding from the tree, yielding a consistent, groundable index. We build a
    /// real store, then corrupt ONLY `meta.json` to add a phantom ref id absent from the
    /// index, and assert the next construction reconciles it.
    #[test]
    #[file_serial(turbovec_model)]
    fn load_self_heals_an_inconsistent_meta_index_pair() {
        let dir = tiny_repo();
        let root = dir.path().to_str().unwrap();

        // Build + persist a real, consistent store.
        {
            let tv = Turbovec::new(root).unwrap();
            assert_store_consistent(&tv);
        }

        // Corrupt meta.json: inject a phantom ref id the index does not contain, exactly
        // the "meta ids absent from the index" inconsistency the self-heal must catch.
        let meta_path = dir.path().join(GROUNDING_DIR).join(META_FILE);
        let mut meta: Meta = serde_json::from_slice(&std::fs::read(&meta_path).unwrap()).unwrap();
        let phantom_id = meta.next_id + 999;
        meta.refs.insert(
            phantom_id,
            StoredRef {
                file: "phantom.rs".to_string(),
                line: 1,
                text: "fn phantom() {}".to_string(),
            },
        );
        // Sanity: this really is inconsistent now (meta has a ref the index lacks).
        {
            let index = IdMapIndex::load(dir.path().join(GROUNDING_DIR).join(INDEX_FILE)).unwrap();
            assert!(
                check_index_meta_consistent(&index, &meta).is_err(),
                "the hand-corrupted pair must read as inconsistent"
            );
        }
        std::fs::write(&meta_path, serde_json::to_vec(&meta).unwrap()).unwrap();

        // The next construction must SELF-HEAL: rebuild from the tree into a consistent
        // store (the phantom ref gone), and ground normally.
        let healed = Turbovec::new(root).unwrap();
        assert_store_consistent(&healed);
        assert!(
            !healed
                .state
                .lock()
                .unwrap()
                .meta
                .refs
                .contains_key(&phantom_id),
            "the phantom ref must be gone after the self-heal rebuild"
        );
        assert!(
            !healed
                .ground("how is damage dealt to an enemy", 1)
                .is_empty(),
            "the self-healed index must still ground"
        );
    }

    /// FINDING #2 + #4 (cross-instance/-process flock: no lost update, no torn pair).
    /// TWO INDEPENDENT `Turbovec` instances over the SAME `.rigger/grounding` store -
    /// each with its OWN in-memory state and its OWN flock fd, exactly as the long-lived
    /// `rigger run` grounder and a separate `rigger reindex` process are - concurrently
    /// reindex DIFFERENT files. Each `StoreLock::acquire` `open()`s the lock file for a
    /// distinct open-file-description, so the two contend on the cross-process `flock`
    /// even in one test process. The guarantee: because a mutating op RELOADS the
    /// persisted base under the flock before applying, instance A's write of file X is
    /// NOT clobbered by instance B's later write of file Y (the lost-update finding), and
    /// no reader ever observes a torn index/meta pair.
    #[test]
    #[file_serial(turbovec_model)]
    fn two_instances_reindex_the_same_store_without_lost_updates() {
        use std::sync::Arc;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap().to_string();
        // Distinct files so each instance owns its own edit target - a lost update shows
        // up as one instance's file vanishing from the final store.
        for i in 0..4 {
            std::fs::write(
                dir.path().join(format!("mod{i}.rs")),
                format!("fn feature_{i}() {{\n    // module {i}\n}}\n"),
            )
            .unwrap();
        }

        // Build the store once so both instances load a shared, consistent base.
        {
            let tv = Turbovec::new(&root).unwrap();
            assert_store_consistent(&tv);
        }

        // Two INDEPENDENT instances: separate objects, separate in-memory state, separate
        // flock fds - the cross-process shape, in one process.
        let a = Arc::new(Turbovec::new(&root).unwrap());
        let b = Arc::new(Turbovec::new(&root).unwrap());
        let dir_path = Arc::new(dir.path().to_path_buf());

        // Each instance repeatedly edits + reindexes a DISTINCT file, racing the other.
        // A drives mod0/mod1; B drives mod2/mod3. If reindex did not reload-under-flock,
        // whichever instance persisted last would drop the other's chunks (its stale
        // in-memory base has no knowledge of the peer's write).
        let mut handles = Vec::new();
        for (inst, files) in [
            (Arc::clone(&a), ["mod0.rs", "mod1.rs"]),
            (Arc::clone(&b), ["mod2.rs", "mod3.rs"]),
        ] {
            let dir_path = Arc::clone(&dir_path);
            let root = root.clone();
            handles.push(std::thread::spawn(move || {
                for round in 0..3 {
                    for f in files {
                        std::fs::write(
                            dir_path.join(f),
                            format!("fn feature_{f}_{round}() {{\n    // edit {round}\n}}\n"),
                        )
                        .unwrap();
                        inst.reindex(&root, &[f.to_string()]);
                    }
                }
            }));
        }
        for h in handles {
            h.join().expect("a concurrent instance must not panic");
        }

        // Neither instance is internally corrupt after the race.
        assert_store_consistent(&a);
        assert_store_consistent(&b);

        // The DECISIVE lost-update check: a FRESH instance loads the final on-disk store
        // and must see ALL FOUR files - if either instance had clobbered the other's
        // write, one pair of files would be missing. The load itself asserts the on-disk
        // pair is not torn (it would fail construction / consistency otherwise).
        drop(a);
        drop(b);
        let reloaded = Turbovec::new(&root).unwrap();
        assert_store_consistent(&reloaded);
        let files = &reloaded.state.lock().unwrap().meta.files;
        for i in 0..4 {
            assert!(
                files.contains_key(&format!("mod{i}.rs")),
                "mod{i}.rs must survive in the final store - a missing file is a lost update \
                 (one instance persisted its stale base over the other's write)"
            );
        }
    }

    /// FINDING #2 (direct lost-update reproduction): the precise sequence the review
    /// flagged - a long-lived instance holds state, an EXTERNAL writer mutates the store,
    /// then the long-lived instance mutates and persists. Its persist must NOT drop the
    /// external write. We hold instance `long_lived`, have a SECOND instance reindex a
    /// new file into the store (the "subprocess"), then have `long_lived` reindex a
    /// DIFFERENT file. Without reload-under-flock, `long_lived`'s persist would overwrite
    /// the store with its stale base and the external file would vanish.
    #[test]
    #[file_serial(turbovec_model)]
    fn long_lived_instance_does_not_clobber_an_external_write() {
        let dir = tiny_repo();
        let root = dir.path().to_str().unwrap().to_string();

        // The long-lived grounder, constructed and held (as the conductor holds it for a
        // whole `rigger run`).
        let long_lived = Turbovec::new(&root).unwrap();
        assert!(long_lived
            .state
            .lock()
            .unwrap()
            .meta
            .files
            .contains_key("combat.rs"));

        // An EXTERNAL writer (a separate instance == a separate process) adds a new file
        // and reindexes it into the shared store, then goes away.
        std::fs::write(
            dir.path().join("external.rs"),
            "fn added_by_the_subprocess() {}\n",
        )
        .unwrap();
        {
            let subprocess = Turbovec::new(&root).unwrap();
            subprocess.reindex(&root, &["external.rs".to_string()]);
        }

        // Now the long-lived instance mutates a DIFFERENT file and persists. Its stale
        // in-memory base predates external.rs; reload-under-flock must fold that in so the
        // persist keeps it.
        std::fs::write(
            dir.path().join("combat.rs"),
            "fn apply_damage() {}\nfn extra() {}\n",
        )
        .unwrap();
        long_lived.reindex(&root, &["combat.rs".to_string()]);

        // The external write survived the long-lived instance's persist, in memory ...
        assert!(
            long_lived
                .state
                .lock()
                .unwrap()
                .meta
                .files
                .contains_key("external.rs"),
            "the external write must survive in the long-lived instance's state after its \
             own reindex reloaded the store under the flock"
        );
        // ... and on disk (a fresh instance sees it).
        drop(long_lived);
        let reloaded = Turbovec::new(&root).unwrap();
        assert_store_consistent(&reloaded);
        assert!(
            reloaded
                .state
                .lock()
                .unwrap()
                .meta
                .files
                .contains_key("external.rs"),
            "the external write must be present in the final on-disk store - not clobbered"
        );
    }

    /// PERF REGRESSION FIX (staleness-gated reload): `freshen_locked` must NOT reload
    /// the whole on-disk store on the hot no-change `ground` path, yet MUST still observe
    /// an external process's write (the lost-update fix the unconditional reload added).
    ///
    /// Two `Turbovec` instances over ONE shared store, mirroring the long-lived MCP-serve
    /// grounder (A) and a separate `rigger reindex` process (B):
    ///  1. A grounds repeatedly on an UNCHANGED store - the cheap stat-only gate skips the
    ///     expensive reload every time, so A's reload counter does not advance.
    ///  2. B reindexes a NEW file externally, moving the on-disk fingerprint.
    ///  3. A's next ground's gate sees the changed fingerprint, DOES reload, and A now
    ///     observes B's file - the lost-update guarantee still holds.
    #[test]
    #[file_serial(turbovec_model)]
    fn freshen_skips_reload_on_unchanged_store_but_observes_external_write() {
        let dir = tiny_repo();
        let root = dir.path().to_str().unwrap().to_string();

        // Build + persist the shared store once so both instances load the same base.
        {
            let seed = Turbovec::new(&root).unwrap();
            assert_store_consistent(&seed);
        }

        // Instance A: the long-lived grounder. Construction LOADS the matching store, so
        // it has cached the on-disk fingerprint and has NOT reloaded (reload is a
        // freshen-only, external-write path).
        let a = Turbovec::new(&root).unwrap();
        assert_eq!(
            reload_count(&a),
            0,
            "construction loads via load_persisted_any, not the freshen reload path"
        );

        // (1) Ground A twice on the UNCHANGED store. The staleness gate stats the two
        // store files, finds the fingerprint equal to what A cached on load, and SKIPS
        // the reload each time - the counter must stay at 0.
        let _ = a.ground("how is damage dealt to an enemy", 1);
        let _ = a.ground("how is damage dealt to an enemy", 1);
        assert_eq!(
            reload_count(&a),
            0,
            "grounding an unchanged store must NOT reload - the gate takes the cheap \
             stat-only skip path (this is the hot-path perf regression the fix targets)"
        );

        // (2) Instance B (a separate 'process') adds a NEW file and reindexes it into the
        // shared store, rewriting index.tvim + meta.json - the on-disk fingerprint moves.
        let unique_term = "how does the plasma conduit reroute reactor coolant";
        std::fs::write(
            dir.path().join("reactor.rs"),
            "fn reroute_plasma_conduit(reactor: &mut Reactor) {\n    reactor.coolant = reactor.reroute();\n}\n",
        )
        .unwrap();
        {
            let b = Turbovec::new(&root).unwrap();
            b.reindex(&root, &["reactor.rs".to_string()]);
        }

        // (3) A's next ground's gate sees the CHANGED fingerprint and reloads exactly
        // once, folding in B's write - so the externally-added term is now groundable.
        let before = reload_count(&a);
        let hit = a.ground(unique_term, 1);
        assert!(
            reload_count(&a) > before,
            "after an external write moves the on-disk fingerprint, the gate MUST reload \
             (the lost-update fix still holds - the reload is gated, not removed)"
        );
        assert_eq!(
            hit.first().map(|r| r.file.as_str()),
            Some("reactor.rs"),
            "A must observe B's externally-reindexed file after the gated reload"
        );
    }

    /// FINDING #3 unit-level: `check_index_meta_consistent` accepts a coherent pair and
    /// rejects each way it can be torn. No model built, so this stays parallel.
    #[test]
    fn check_index_meta_consistent_detects_each_inconsistency() {
        // Build a `Meta` describing one chunk id 7 in file a.rs. `FileEntry`/`Meta` are
        // not `Clone` (production types), so each case builds its own via this helper.
        fn meta_for(ids: &[(u64, &str)], files: &[(&str, Vec<u64>)]) -> Meta {
            let mut m = Meta {
                next_id: 100,
                ..Default::default()
            };
            for &(id, file) in ids {
                m.refs.insert(
                    id,
                    StoredRef {
                        file: file.into(),
                        line: 1,
                        text: String::new(),
                    },
                );
            }
            for (file, ids) in files {
                m.files.insert(
                    (*file).into(),
                    FileEntry {
                        hash: 0,
                        ids: ids.clone(),
                    },
                );
            }
            m
        }

        // A tiny consistent pair: one vector at id 7, one matching ref, one file owning it.
        let mut index = IdMapIndex::new(EMBED_DIM, BIT_WIDTH).unwrap();
        let vec7 = vec![0.1f32; EMBED_DIM];
        index.add_with_ids(&vec7, &[7]).unwrap();
        let good = meta_for(&[(7, "a.rs")], &[("a.rs", vec![7])]);
        assert!(
            check_index_meta_consistent(&index, &good).is_ok(),
            "a coherent index/meta pair must pass"
        );

        // Meta ref id absent from the index (the torn-write shape).
        let torn = meta_for(&[(7, "a.rs"), (42, "ghost.rs")], &[("a.rs", vec![7])]);
        assert!(
            check_index_meta_consistent(&index, &torn).is_err(),
            "a meta ref id the index lacks must be rejected"
        );

        // File claims an id with no ref.
        let orphan = meta_for(&[(7, "a.rs")], &[("a.rs", vec![7]), ("b.rs", vec![99])]);
        assert!(
            check_index_meta_consistent(&index, &orphan).is_err(),
            "a file-claimed id with no ref must be rejected"
        );

        // Cardinality mismatch: index has a vector the refs do not cover.
        let mut index2 = IdMapIndex::new(EMBED_DIM, BIT_WIDTH).unwrap();
        index2.add_with_ids(&vec7, &[7]).unwrap();
        index2.add_with_ids(&vec![0.2f32; EMBED_DIM], &[8]).unwrap();
        assert!(
            check_index_meta_consistent(&index2, &good).is_err(),
            "a surplus vector with no ref must be rejected (cardinality mismatch)"
        );
    }

    /// ORPHAN-REF LEAK FIX (`index_file_content` atomicity): after a normal index the
    /// in-memory `meta.refs` must mirror the index EXACTLY (same live id count) and hold
    /// NO orphan - every ref id must be listed by some `FileEntry.ids`. And when the
    /// index add FAILS, `meta` (refs, files, next_id) must be byte-for-byte unchanged, so
    /// a failed add can never strand a ref that `drop_file` (which only reclaims ids under
    /// a `FileEntry`) could never reach. The failure is forced with NO production-only
    /// seam: we rewind the TEST instance's `meta.next_id` so the next allocation collides
    /// with ids already in the index, making `add_with_ids` return `IdAlreadyPresent`.
    #[test]
    #[file_serial(turbovec_model)]
    fn index_file_content_is_atomic_no_orphan_refs_on_add_failure() {
        let dir = tiny_repo();
        let root = dir.path().to_str().unwrap();
        let tv = Turbovec::new(root).unwrap();

        // (A) After a normal build the store is orphan-free and cardinality-matched:
        //     refs.len() == the index's live id count, and every ref id is claimed by
        //     some file's `ids` (no ref absent from every `FileEntry.ids`).
        {
            let state = tv.state.lock().unwrap();
            assert_eq!(
                state.meta.refs.len(),
                state.index.len(),
                "meta.refs must have exactly one entry per live vector id in the index"
            );
            let file_ids: std::collections::HashSet<u64> = state
                .meta
                .files
                .values()
                .flat_map(|e| e.ids.iter().copied())
                .collect();
            for id in state.meta.refs.keys() {
                assert!(
                    file_ids.contains(id),
                    "meta.refs id {id} is an ORPHAN - it is listed by no FileEntry.ids, so \
                     drop_file could never reclaim it"
                );
            }
            assert!(
                !state.meta.refs.is_empty(),
                "the tiny repo must have produced at least one indexed chunk"
            );
        }

        // (B) Force an `add_with_ids` failure WITHOUT any production seam, then assert
        //     `meta` is untouched. `add_with_ids` rejects an id already present in the
        //     index (`IdAlreadyPresent`); `index_file_content` allocates its chunk ids
        //     from a LOCAL counter seeded at `state.meta.next_id`. Rewinding next_id to 0
        //     (ids the freshly-built index already holds) makes that add fail. Snapshot
        //     `meta` before the call and assert it is byte-for-byte unchanged after.
        let (refs_before, files_before, next_id_before, index_len_before) = {
            let mut state = tv.state.lock().unwrap();
            // Rewind the allocator so the next add collides with existing ids.
            state.meta.next_id = 0;
            let refs: std::collections::BTreeMap<u64, (String, u32, String)> = state
                .meta
                .refs
                .iter()
                .map(|(&id, r)| (id, (r.file.clone(), r.line, r.text.clone())))
                .collect();
            let files: std::collections::BTreeMap<String, (u64, Vec<u64>)> = state
                .meta
                .files
                .iter()
                .map(|(f, e)| (f.clone(), (e.hash, e.ids.clone())))
                .collect();
            (refs, files, state.meta.next_id, state.index.len())
        };

        // Attempt to index a NEW file. Its chunks embed fine, but the add allocates ids
        // starting at 0 - already in the index - so `add_with_ids` returns Err and
        // `index_file_content` `?`s out having touched NOTHING in `meta`.
        let result = {
            let mut state = tv.state.lock().unwrap();
            tv.index_file_content(
                &mut state,
                "atomicity_probe.rs",
                "fn probe_atomicity(store: &mut Store) {\n    store.commit();\n}\n",
            )
        };
        assert!(
            result.is_err(),
            "seeding next_id to collide with existing ids must make add_with_ids fail"
        );

        // `meta` (refs, files, next_id) is byte-for-byte unchanged - no orphan ref, no
        // leaked id, no partial FileEntry - and the index gained no vector.
        {
            let state = tv.state.lock().unwrap();
            let refs_after: std::collections::BTreeMap<u64, (String, u32, String)> = state
                .meta
                .refs
                .iter()
                .map(|(&id, r)| (id, (r.file.clone(), r.line, r.text.clone())))
                .collect();
            let files_after: std::collections::BTreeMap<String, (u64, Vec<u64>)> = state
                .meta
                .files
                .iter()
                .map(|(f, e)| (f.clone(), (e.hash, e.ids.clone())))
                .collect();
            assert_eq!(
                refs_after, refs_before,
                "a failed add must leave meta.refs untouched - no orphan ref stranded"
            );
            assert_eq!(
                files_after, files_before,
                "a failed add must leave meta.files untouched - no partial FileEntry"
            );
            assert_eq!(
                state.meta.next_id, next_id_before,
                "a failed add must not advance next_id - no id leaked"
            );
            assert!(
                !state.meta.files.contains_key("atomicity_probe.rs"),
                "the file whose add failed must not appear in meta.files"
            );
            assert_eq!(
                state.index.len(),
                index_len_before,
                "a failed add must leave the index unchanged"
            );
        }
    }
}

/// Periphery (contract) layer for the "honest embed skip" seam.
///
/// The inside-out unit tests prove the skip MACHINERY by injecting a fake [`Embedder`]
/// with a hand-chosen identity ("model-v1"/"model-v2"), so they establish that a file is
/// skipped only when both its content AND the reported model identity are unchanged -
/// GIVEN an honest identity. What a fake can never establish is whether the SHIPPED
/// adapter's identity is itself honest: `impl Embedder for FastEmbedEmbedder` derives its
/// identity from [`fastembed_identity`], and only a real value flowing through the real
/// [`chunk_key`] can prove the skip is honest in production. This module covers exactly
/// that production half of the [`Embedder`] port - the part the fake severs. It builds no
/// model (the identity and the key are pure functions of the model constants and content),
/// so it stays in the fast, always-run lane rather than the serialized model-test lane.
#[cfg(test)]
mod periphery {
    use super::*;

    /// HONESTY, first half - "a mere binary reinstall re-embeds nothing". The production
    /// identity must be DETERMINISTIC, a pure function of the fixed model constants and
    /// nothing that varies between two builds of the same model (build id, install time,
    /// index mtime, a path). Two evaluations must be byte-identical; otherwise a rebuild
    /// would silently re-key every file and re-embed the whole tree despite an unchanged
    /// model - a dishonest, expensive skip failure.
    #[test]
    fn production_identity_is_stable_across_reinstalls() {
        assert_eq!(
            fastembed_identity(),
            fastembed_identity(),
            "the production identity must be deterministic so a reinstall of the same model \
             re-embeds nothing"
        );
        // A non-empty identity is required, or the key's identity dimension collapses and the
        // fold degenerates to a content-only key.
        assert!(
            !fastembed_identity().is_empty(),
            "the production identity must be non-empty"
        );
    }

    /// HONESTY, second half - "any change that alters the produced vectors re-embeds". The
    /// two determinants of the vectors this grounder stores are WHICH embedding model and
    /// its DIMENSION, so the identity must fold BOTH. If it dropped the model name, swapping
    /// to a differently-producing model would keep stale vectors; if it dropped the
    /// dimension, a re-dimensioned model would too. Encoding both is what makes a model or
    /// dimension change re-embed the tree instead of serving vectors the current model never
    /// produced.
    #[test]
    fn production_identity_folds_model_and_dimension() {
        let id = fastembed_identity();
        assert!(
            id.contains(&EMBEDDING_MODEL.to_string()),
            "identity {id:?} must name the embedding model so a model swap re-embeds"
        );
        assert!(
            id.contains(&EMBED_DIM.to_string()),
            "identity {id:?} must carry the embedding dimension so a dimension change re-embeds"
        );
    }

    /// The INTEGRATION the fake severs: the SHIPPED identity, fed into the REAL
    /// [`chunk_key`], yields an honest skip. Same model + unchanged content collides (the
    /// skip is reached); a different model's identity, or a real edit, diverges (re-embed).
    /// The inside-out test drives `chunk_key` with arbitrary strings; this pins that the
    /// production [`fastembed_identity`] actually partitions the key space by model, so the
    /// two halves above are not merely internally consistent but wired to the key the
    /// grounder truly persists and compares.
    #[test]
    fn production_identity_binds_the_honest_skip_key() {
        let content = "fn render_frame(scene: &Scene) {}\n";
        let prod = fastembed_identity();

        // Same model + same content -> one key -> the file's stored hash matches and the
        // embed is skipped.
        assert_eq!(
            chunk_key(&prod, content),
            chunk_key(&prod, content),
            "same model + unchanged content must key identically so the honest skip is reached"
        );

        // A DIFFERENT model's identity over the SAME content -> a different key -> a model
        // swap re-embeds rather than serving vectors this model never produced.
        let other_model = format!("some-other-embedder/dim={EMBED_DIM}");
        assert_ne!(
            other_model, prod,
            "the fixture's foreign identity must differ from the production one"
        );
        assert_ne!(
            chunk_key(&prod, content),
            chunk_key(&other_model, content),
            "the production identity must distinguish this model's keys from another model's"
        );

        // A real edit under the SAME production model -> a different key -> the file
        // re-embeds instead of keeping a stale vector.
        let edited = "fn render_frame(scene: &Scene) { scene.draw(); }\n";
        assert_ne!(
            chunk_key(&prod, content),
            chunk_key(&prod, edited),
            "a content change under the same model must re-key so a stale vector is never kept"
        );
    }
}

/// PERIPHERY - the `reindex` CALLER CONTRACT at the public [`Grounder`] surface: naming a
/// file more than once in a single `reindex` is OBSERVABLY IDENTICAL to naming it once.
///
/// `Grounder::reindex` is the cross-module seam that `rigger reindex <files>` (main.rs),
/// the conductor's post-integrate reindex, and the hybrid grounder all drive; its stated
/// contract is that the ingest OUTPUT is byte-identical to naming the file once, because
/// [`Turbovec::index_files`] is a per-caller-deduped install authority - a rel named twice
/// with no drop between the two installs orphans the first run's vectors. The implementer's
/// inside-out unit test proves the de-dup by inspecting private store state (orphan refs,
/// next_id); this layer proves the SAME contract from OUTSIDE, through ONLY the public
/// `reindex` + `ground` surface: two grounders over byte-identical trees that differ solely
/// in whether the file is named once or twice must ground identically, and the duplicated
/// file must surface EXACTLY once. A regression to the un-deduped double-install leaves a
/// second, identically-embedded orphan vector that surfaces the file twice and diverges the
/// two grounders - the boundary bug this guards (the cause of the recorded reindex REJECT).
///
/// Fake-embedder-driven (deterministic vectors, no ONNX model), so it stays in the fast,
/// always-run lane. The counting fake lives in the sibling `mod tests` and is deliberately
/// not shared - the periphery layer stands on its own fixture.
#[cfg(test)]
mod periphery_reindex {
    use super::*;

    /// A deterministic fake [`Embedder`]: identical text always embeds to the identical,
    /// non-degenerate `EMBED_DIM` vector, so two byte-identical trees build byte-identical
    /// indices and ANY divergence in their public grounding is caused only by the reindex
    /// arg shape under test - never by embedding noise. It builds no model.
    struct StableEmbedder;

    impl Embedder for StableEmbedder {
        fn identity(&self) -> &str {
            "periphery-reindex-stable-embedder"
        }
        fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
            Ok(texts.iter().map(|t| stable_vec(t)).collect())
        }
    }

    /// A deterministic, non-degenerate `EMBED_DIM`-vector seeded from the text (an FNV seed
    /// then an LCG per lane), so the quantized index accepts it and identical text always
    /// embeds identically across the two grounders under comparison.
    fn stable_vec(text: &str) -> Vec<f32> {
        let mut h = 0xcbf2_9ce4_8422_2325u64 ^ (text.len() as u64);
        for b in text.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        (0..EMBED_DIM)
            .map(|_| {
                h = h
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                ((h >> 33) as f32 / (u32::MAX as f32)) * 2.0 - 1.0
            })
            .collect()
    }

    /// The projection through which a `ground` CALLER observes a result: [`Ref`] carries no
    /// `PartialEq`, and the id/vector bookkeeping the de-dup bug corrupts is invisible at
    /// this surface, so an EQUAL (file, line, snippet) projection means the two reindex
    /// shapes are interchangeable to every consumer of `ground`.
    fn grounding(tv: &Turbovec, query: &str, k: usize) -> Vec<(String, u32, String)> {
        tv.ground(query, k)
            .into_iter()
            .map(|r| (r.file, r.line, r.text))
            .collect()
    }

    /// A two-file tree: a distinctive single-chunk term so a query can target exactly one
    /// file, plus a second file so the grounding has more than one candidate to (mis)order.
    fn two_file_tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("dup.rs"),
            "fn teleport_across_the_void() {}\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("other.rs"), "fn draw_the_hud() {}\n").unwrap();
        dir
    }

    #[test]
    fn reindex_naming_a_file_twice_grounds_identically_to_naming_it_once() {
        // Two byte-identical trees + the same deterministic model => two byte-identical
        // cold-built indices. They then differ in exactly ONE thing: how many times dup.rs
        // is named in the reindex that follows.
        let once_dir = two_file_tree();
        let twice_dir = two_file_tree();
        let once_root = once_dir.path().to_str().unwrap();
        let twice_root = twice_dir.path().to_str().unwrap();

        // Construct exactly as `rigger reindex` does (new_for_reindex == LeaveStale): load or
        // cold-build the store, leaving whole-tree drift for the caller's named reindex.
        let once =
            Turbovec::from_embedder(once_root, Box::new(StableEmbedder), OnDrift::LeaveStale)
                .unwrap();
        let twice =
            Turbovec::from_embedder(twice_root, Box::new(StableEmbedder), OnDrift::LeaveStale)
                .unwrap();

        // The ONLY difference: `twice` names dup.rs TWICE in one reindex - the
        // `rigger reindex dup.rs dup.rs` shape the fix must make idempotent.
        once.reindex(once_root, &["dup.rs".to_string()]);
        twice.reindex(twice_root, &["dup.rs".to_string(), "dup.rs".to_string()]);

        // (a) EQUIVALENCE at the public surface: for every query the grounding projection is
        // identical. A duplicated install would leave `twice` holding a second,
        // identically-embedded orphan vector for dup.rs (surfacing it twice / reshuffling the
        // ranking) and diverge from `once`. Byte-identical projections prove naming twice ==
        // naming once to every `ground` caller. (The embedder is deterministic but not
        // semantic, so the ranking itself is arbitrary - only its EQUALITY across the two
        // grounders is load-bearing here.)
        for query in ["teleport across the void", "draw the hud", "fn"] {
            assert_eq!(
                grounding(&once, query, 5),
                grounding(&twice, query, 5),
                "reindex naming dup.rs twice must ground identically to naming it once \
                 (query {query:?})"
            );
        }

        // (b) EXACTLY ONCE: with only two files in the tree and k past that, `ground` returns
        // every chunk, so dup.rs appears once iff it owns exactly one install. The un-deduped
        // path installed it twice, putting a duplicate/orphan vector in the index that would
        // surface dup.rs a second time - the direct regression guard.
        let hits = twice.ground("fn teleport_across_the_void() {}", 5);
        let dup_hits = hits.iter().filter(|r| r.file == "dup.rs").count();
        assert_eq!(
            dup_hits, 1,
            "dup.rs surfaced {dup_hits} times after being named twice - a duplicate/orphan \
             vector is in the index"
        );
    }
}
