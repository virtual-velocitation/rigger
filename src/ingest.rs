//! Project-source ingest into the context graph: the ONE walk-and-content-key authority both
//! the live run (`conductor::RunCtx::ingest_project_batches`) and the standalone
//! `rigger graph build` entry share, so the content key an event is deduped under can never
//! drift between the two ingest entries.
//!
//! Each caller supplies its OWN emit sink - the run's replay-keyed, concurrency-safe
//! `emit_keyed`; the cold build's log-seeded seen-set plus a direct append-and-fold - because
//! their mutation semantics legitimately differ. What must NOT fork is the drift-prone part:
//! the walk over the project's per-file extraction batches and the `<prefix>/<file>@<hash>#<i>`
//! content key. Those are derived here, once, so the run and a cold `graph build` agree on every
//! key and never double-ingest one another's work.
//!
//! Symbols-gated: the walk lowers the tree through the `symbols` extraction pass, so the light
//! lane has nothing to ingest - a no-op that emits nothing, exactly as the run's ingest is a
//! no-op there.

#[cfg(feature = "symbols")]
use crate::eventstore::Event;

/// Walk the project tree at `root` and, for every extraction event the code (spec 29a) and
/// design (spec 29b) passes emit, call `emit(key, event)` with the event's deterministic content
/// key `<prefix>/<file>@<hash>#<i>` (`gc` for code, `gd` for design). The key is a pure function
/// of the batch's bytes, so an unchanged file yields identical keys (a caller dedups on them) and
/// a changed file's batch hashes differently - every key differs, so the whole batch, its `fresh`
/// head included, re-emits. This function owns only the walk and the keying; the sink decides what
/// a key MEANS (append-and-fold, or skip a replay), so the mutation authority stays with the caller.
#[cfg(feature = "symbols")]
pub fn ingest_project(root: &str, mut emit: impl FnMut(&str, &Event)) {
    // The code half (spec 29a): the project's real definitions and references. Reuses the
    // `symbols` grounder's persisted index when present (no re-parse), so in a live run this is a
    // cheap read of what the grounder already built - not a second whole-tree parse.
    for (file, batch) in crate::grounder::symbols::events::project_batches(root) {
        emit_batch("gc", &file, &batch, &mut emit);
    }
    // The design half (spec 29b): the project's design docs and inline source rationale.
    for (file, batch) in crate::grounder::design::events::project_batches(root) {
        emit_batch("gd", &file, &batch, &mut emit);
    }
}

/// Key one file's batch under `<prefix>/<file>@<hash>#<i>` and hand each event to `emit`. `hash`
/// fingerprints the WHOLE batch's bytes with the SAME line-ending-normalized content primitive the
/// symbols reindex freshening keys on (reused, not a fresh copy, so the change-detection key is one
/// content-identity authority), so every event of a file shares one `<hash>`. The batch bytes are
/// JSON the emit pass just serialized, so they are valid UTF-8.
#[cfg(feature = "symbols")]
fn emit_batch(prefix: &str, file: &str, batch: &[Event], emit: &mut impl FnMut(&str, &Event)) {
    let concat: String = batch
        .iter()
        .filter_map(|e| std::str::from_utf8(&e.data).ok())
        .collect();
    let hash = crate::grounder::symbols::store::content_hash(&concat);
    for (i, ev) in batch.iter().enumerate() {
        let key = format!("{prefix}/{file}@{hash}#{i}");
        emit(&key, ev);
    }
}

/// The light lane compiles no extraction pass, so there is nothing to walk - a no-op that emits
/// nothing. `graph build` still opens (creating) the store and degrades to an empty graph, never
/// an error, exactly as the run's ingest is a no-op here.
#[cfg(not(feature = "symbols"))]
pub fn ingest_project(_root: &str, _emit: impl FnMut(&str, &crate::eventstore::Event)) {}
