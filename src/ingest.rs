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

use crate::contextgraph::Projection;
use crate::eventstore::{Event, EventStore, ExpectedRevision, Position};

/// Append a whole batch of events to `stream` in ONE store append and fold them into `graph` in ONE
/// transaction (via [`Projection::apply_batch`]) - the batched-fold cadence spec 49 needs: one store
/// transaction per file's batch, not one per event (the measured cold-build throughput was
/// transaction-cadence bound, not parse-bound). Each event is stamped with its global position
/// before it folds: a single append lands the batch at CONSECUTIVE positions ending at the returned
/// last position, so event `i` of an `n`-event batch sits at `last - (n - 1) + i`. The fold is
/// best-effort - a fold failure never fails the append, which already landed durably in the log,
/// exactly as the run's per-event `append_and_fold` folds best-effort. Returns the last appended
/// position (`0` for an empty batch, which appends nothing).
///
/// This is the ONE batched append-and-fold authority both ingest sinks share - the run's keyed emit
/// and a cold `rigger graph build` - so the batching can never diverge between them. It lives here
/// beside the walk-and-key authority rather than inside either sink, and it is deliberately NOT
/// `symbols`-gated: it only moves events through the store and graph ports, which both feature lanes
/// compile, so the run's single-event mutation path can route its one-event case through it in
/// either lane.
pub fn append_and_fold_batch(
    store: &dyn EventStore,
    graph: Option<&dyn Projection>,
    stream: &str,
    events: &[Event],
) -> Result<Position, crate::eventstore::Error> {
    if events.is_empty() {
        return Ok(0);
    }
    let last = store.append(stream, ExpectedRevision::Any, events)?;
    if let Some(g) = graph {
        let n = events.len() as Position;
        let base = last + 1 - n;
        let positioned: Vec<Event> = events
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let mut e = e.clone();
                e.position = base + i as Position;
                e
            })
            .collect();
        let _ = g.apply_batch(&positioned);
    }
    Ok(last)
}

/// What a walk did, reported back to the caller. `batches_emitted` counts the file batches the walk
/// handed to `emit` (code and design). `workers_engaged` is how many parse-worker threads actually
/// ran the code half: `> 1` proves the parse fanned across cores, and it is an HONEST count (the
/// distinct threads that ran), so a serial walk (width 1) reports exactly 1.
#[cfg(feature = "symbols")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IngestStats {
    pub batches_emitted: usize,
    pub workers_engaged: usize,
}

/// Walk the project tree at `root` and, for every extraction event the code (spec 29a) and
/// design (spec 29b) passes emit, call `emit(key, event)` with the event's deterministic content
/// key `<prefix>/<file>@<hash>#<i>` (`gc` for code, `gd` for design). The key is a pure function
/// of the batch's bytes, so an unchanged file yields identical keys (a caller dedups on them) and
/// a changed file's batch hashes differently - every key differs, so the whole batch, its `fresh`
/// head included, re-emits. This function owns only the walk and the keying; the sink decides what
/// a key MEANS (append-and-fold, or skip a replay), so the mutation authority stays with the caller.
///
/// The code half's per-file parse/lower fans across a default-sized worker pool (one worker per
/// logical core), but the EMIT stays in sorted file-path order - parallelism is observationally
/// invisible. The returned [`IngestStats`] is informational; existing callers discard it and are
/// unaffected.
#[cfg(feature = "symbols")]
pub fn ingest_project(root: &str, emit: impl FnMut(&str, &Event)) -> IngestStats {
    ingest_project_paced(root, crate::parallel::default_workers(), emit)
}

/// [`ingest_project`] at a chosen parse width, emitting one event at a time. The code half (spec
/// 29a) parses/lowers its files across up to `workers` threads yet EMITS them in the index's sorted
/// file-path order, so the event sequence a caller's sink observes is byte-identical to a serial
/// walk's regardless of scheduling (the rebuild-byte-identical discipline). `workers <= 1` runs the
/// lowering inline: it IS the serial walk a wider walk is proven byte-identical against - the same
/// code path, not a hand-rolled twin. The design half (spec 29b) stays serial; its walk lives in
/// `design/events.rs`, whose scoping a separate unit owns, so parallelizing it here would fork that
/// file.
///
/// This is the per-EVENT view of the one walk: it FLATTENS each file's keyed batch into one
/// `emit(key, event)` per event, in `#i` order, so it is a thin adapter over
/// [`ingest_project_batched_paced`] - not a second walk. A sink that appends and folds a file's
/// whole batch as a UNIT (the batched-fold cadence, spec 49) uses the batched entry instead.
#[cfg(feature = "symbols")]
pub fn ingest_project_paced(
    root: &str,
    workers: usize,
    mut emit: impl FnMut(&str, &Event),
) -> IngestStats {
    walk_batches(root, workers, |keyed| {
        for (key, ev) in keyed {
            emit(key, ev);
        }
    })
}

/// [`ingest_project`] handing a sink each file's WHOLE keyed batch at once (the whole file's events,
/// each paired with its `<prefix>/<file>@<hash>#<i>` content key, in `#i` order) rather than one
/// event at a time. A sink appends the file's batch in ONE store append and folds it in ONE graph
/// transaction (via [`append_and_fold_batch`]) - the batched-fold cadence spec 49 needs, since the
/// measured cold-build throughput was transaction-cadence bound. Same default parse width as
/// [`ingest_project`]; the per-event walk is this same core flattened.
#[cfg(feature = "symbols")]
pub fn ingest_project_batched(
    root: &str,
    on_batch: impl FnMut(&[(String, &Event)]),
) -> IngestStats {
    ingest_project_batched_paced(root, crate::parallel::default_workers(), on_batch)
}

/// [`ingest_project_batched`] at a chosen parse width - the batched analogue of
/// [`ingest_project_paced`]. Parse width changes only the code half's parallelism (criterion 1),
/// never the batching: the same files still emit as the same per-file batches, in sorted file-path
/// order.
#[cfg(feature = "symbols")]
pub fn ingest_project_batched_paced(
    root: &str,
    workers: usize,
    on_batch: impl FnMut(&[(String, &Event)]),
) -> IngestStats {
    walk_batches(root, workers, on_batch)
}

/// The ONE walk both public views share: parse/lower the project at `root` and hand each file's
/// WHOLE keyed batch to `on_batch`, in sorted file-path order (the code half first, then the design
/// half), each batch in `#i` order. The per-event [`ingest_project_paced`] and the per-batch
/// [`ingest_project_batched_paced`] are both thin views over this, so there is no forked walk to
/// drift - the emit order is defined once, here.
#[cfg(feature = "symbols")]
fn walk_batches(
    root: &str,
    workers: usize,
    mut on_batch: impl FnMut(&[(String, &Event)]),
) -> IngestStats {
    let mut batches_emitted = 0usize;
    // The code half (spec 29a): parallel parse feeds this ordered emit. Reuses the `symbols`
    // grounder's persisted index when present (no re-parse), so in a live run this is a cheap read of
    // what the grounder already built - not a second whole-tree parse.
    let (code_batches, workers_engaged) =
        crate::grounder::symbols::events::project_batches_paced(root, workers);
    for (file, batch) in &code_batches {
        key_batch("gc", file, batch, &mut on_batch);
        batches_emitted += 1;
    }
    // The design half (spec 29b): the project's design docs and inline source rationale, serial.
    let design_batches = crate::grounder::design::events::project_batches(root);
    for (file, batch) in &design_batches {
        key_batch("gd", file, batch, &mut on_batch);
        batches_emitted += 1;
    }
    IngestStats {
        batches_emitted,
        workers_engaged,
    }
}

/// Key one file's batch under `<prefix>/<file>@<hash>#<i>` and hand the WHOLE keyed batch to
/// `on_batch` at once. `hash` fingerprints the WHOLE batch's bytes with the SAME line-ending-
/// normalized content primitive the symbols reindex freshening keys on (reused, not a fresh copy, so
/// the change-detection key is one content-identity authority), so every event of a file shares one
/// `<hash>`. The batch bytes are JSON the emit pass just serialized, so they are valid UTF-8.
#[cfg(feature = "symbols")]
fn key_batch(
    prefix: &str,
    file: &str,
    batch: &[Event],
    on_batch: &mut impl FnMut(&[(String, &Event)]),
) {
    let concat: String = batch
        .iter()
        .filter_map(|e| std::str::from_utf8(&e.data).ok())
        .collect();
    let hash = crate::grounder::symbols::store::content_hash(&concat);
    let keyed: Vec<(String, &Event)> = batch
        .iter()
        .enumerate()
        .map(|(i, ev)| (format!("{prefix}/{file}@{hash}#{i}"), ev))
        .collect();
    on_batch(&keyed);
}

/// The light lane compiles no extraction pass, so there is nothing to walk - a no-op that emits
/// nothing. `graph build` still opens (creating) the store and degrades to an empty graph, never
/// an error, exactly as the run's ingest is a no-op here.
#[cfg(not(feature = "symbols"))]
pub fn ingest_project(_root: &str, _emit: impl FnMut(&str, &crate::eventstore::Event)) {}

/// The light lane's batched entry: no extraction pass, so nothing to walk - a no-op that hands the
/// sink no batches. Mirrors the light-lane [`ingest_project`], so a cold `graph build` degrades to
/// an empty graph in either lane (the batched append-and-fold kernel above stays compiled in both).
#[cfg(not(feature = "symbols"))]
pub fn ingest_project_batched(
    _root: &str,
    _on_batch: impl FnMut(&[(String, &crate::eventstore::Event)]),
) {
}

#[cfg(all(test, feature = "symbols"))]
mod tests {
    use super::{ingest_project_paced, IngestStats};
    use crate::eventstore::Event;

    /// Drive a walk at `workers` width and capture the exact `(key, type, data)` triples the sink
    /// sees, in emit order - the observable the byte-identical contract is defined over.
    fn walk(root: &str, workers: usize) -> (Vec<(String, String, Vec<u8>)>, IngestStats) {
        let mut seq: Vec<(String, String, Vec<u8>)> = Vec::new();
        let stats = ingest_project_paced(root, workers, |key, ev: &Event| {
            seq.push((key.to_string(), ev.type_.clone(), ev.data.clone()));
        });
        (seq, stats)
    }

    #[test]
    fn parallel_parse_emits_the_byte_identical_sequence_a_serial_walk_would() {
        // Criterion 1: ingesting a multi-file fixture at width 8 (parallel parse) must engage more
        // than one parse worker AND emit the SAME event sequence - same keys, types, and bytes, in
        // the SAME sorted-file-path order - that a serial walk (width 1) emits. The serial walk is
        // the SAME production function at width 1 (`map_ordered` short-circuits to an inline map), so
        // determinism is proven against the one code path, not a hand-rolled twin.
        let dir = tempfile::tempdir().unwrap();
        for i in 0..6 {
            std::fs::write(
                dir.path().join(format!("m{i}.rs")),
                format!("fn f{i}() {{}}\nfn g{i}() {{ f{i}(); }}\n"),
            )
            .unwrap();
        }
        let root = dir.path().to_str().unwrap();

        let (serial, serial_stats) = walk(root, 1);
        let (parallel, parallel_stats) = walk(root, 8);

        assert!(
            !serial.is_empty(),
            "the multi-file fixture emits code-ingest events"
        );
        assert_eq!(
            serial, parallel,
            "parallel parse feeds an ordered emit: the width-8 sequence is byte-identical to serial"
        );

        // Sorted file-path order: the code (`gc/`) keys name files in ascending path order.
        let gc_files: Vec<&str> = parallel
            .iter()
            .filter_map(|(k, _, _)| k.strip_prefix("gc/"))
            .filter_map(|rest| rest.split('@').next())
            .collect();
        let mut sorted = gc_files.clone();
        sorted.sort_unstable();
        assert_eq!(
            gc_files, sorted,
            "code batches emit in sorted file-path order; got {gc_files:?}"
        );

        // Stable batch keys: the two walks agree on every key (the content key is a pure function of
        // the batch bytes, independent of walk width).
        let keys = |seq: &[(String, String, Vec<u8>)]| -> Vec<String> {
            seq.iter().map(|(k, _, _)| k.clone()).collect()
        };
        assert_eq!(
            keys(&serial),
            keys(&parallel),
            "the batch keys are stable across walk width"
        );

        // Engagement: width 1 is the serial oracle (one worker); width 8 over six files fans out.
        assert_eq!(
            serial_stats.workers_engaged, 1,
            "width 1 runs inline: exactly one engaged worker"
        );
        assert!(
            parallel_stats.workers_engaged > 1,
            "width 8 over a six-file fixture engages more than one parse worker; got {}",
            parallel_stats.workers_engaged
        );
        assert_eq!(
            serial_stats.batches_emitted, parallel_stats.batches_emitted,
            "both widths emit the same number of file batches"
        );
        assert!(
            parallel_stats.batches_emitted >= 6,
            "each of the six source files contributes a code batch; got {}",
            parallel_stats.batches_emitted
        );
    }
}
