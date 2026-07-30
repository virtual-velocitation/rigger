//! Periphery (integration) tests for spec 49 criterion 1: parallel parse with an ORDERED emit.
//! These run OUTSIDE the crate, over the library's PUBLIC surface (`rigger::...`), so they guard
//! boundaries the inside-out unit tests are structurally blind to. The unit tests reach their
//! subjects through in-crate paths (`super::` / `crate::`) and drive the PACED variant at a fixed
//! width; nothing there proves that:
//!
//!  - the parallel primitive and the ingest entry points are actually EXPORTED and behave when
//!    driven as an external consumer would drive them (an item that were accidentally `pub(crate)`
//!    would still pass every in-crate test yet be unreachable here);
//!  - the DEFAULT-width public entry `ingest_project` - the one the production sinks call, which
//!    picks the machine's core count via `default_workers()` and which no unit test drives - emits
//!    the byte-identical sequence a serial walk would;
//!  - the `project_batches_paced` grounder boundary is width-invariant and its output still equals
//!    the legacy `project_batches` facade the design half and the 29c suites depend on.
//!
//! Test 1 needs no extraction grammar, so it is UNGATED and runs in BOTH feature lanes (the
//! `parallel` module is compiled unconditionally, so a width-less minimal build still gets a usable
//! primitive). Tests 2 and 3 drive the symbols-gated ingest/extraction entry points, so - exactly
//! like the sibling ingest suites - they carry `#[cfg(feature = "symbols")]` and compile to nothing
//! in the light lane, keeping both lanes green.

use std::sync::Mutex;

/// The parallel primitive is a PUBLIC authority (`rigger::parallel::map_ordered`), so an external
/// caller must be able to fan a pure map across a real worker pool and get the results back in INPUT
/// order - never completion order - with an honest engagement count. The inside-out unit tests prove
/// this through `super::map_ordered`; this pins the same contract at the crate EDGE, and adds a
/// property they do not state outright: over a concurrent run every item is visited exactly once (no
/// drop, no duplicate under interleaving). Ungated: it runs in both feature lanes.
#[test]
fn map_ordered_public_boundary_preserves_order_visits_once_and_fans_out() {
    // A width passed EXPLICITLY spawns a real pool of `min(workers, len)` threads regardless of the
    // host's core count, so `engaged > 1` here is a by-construction guarantee, not a timing gamble.
    let items: Vec<usize> = (0..256).collect();
    let seen: Mutex<Vec<usize>> = Mutex::new(Vec::new());

    let (out, engaged) = rigger::parallel::map_ordered(&items, 8, |&x| {
        // A `Fn` closure sharing a Mutex records that this input was visited, proving the concurrent
        // map neither drops nor duplicates work.
        seen.lock().unwrap().push(x);
        x * x
    });

    let expected: Vec<usize> = items.iter().map(|&x| x * x).collect();
    assert_eq!(
        out, expected,
        "results come back in INPUT order (index-preserving), never completion order"
    );
    assert!(
        engaged > 1,
        "256 items over an explicit width of 8 fans across more than one worker; got {engaged}"
    );

    let mut visited = seen.into_inner().unwrap();
    visited.sort_unstable();
    assert_eq!(
        visited, items,
        "every input is visited exactly once under the concurrent map - no drop, no duplicate"
    );

    // The default width authority a width-less walk falls back to is always a usable pool.
    assert!(
        rigger::parallel::default_workers() >= 1,
        "the default parse width is never zero, even when parallelism cannot be queried"
    );
}

/// Capture the exact `(key, type, data)` triples a walk hands its sink, in emit order - the
/// observable the byte-identical contract is defined over - by driving a PUBLIC ingest entry point.
#[cfg(feature = "symbols")]
fn drive_default(root: &str) -> (Vec<(String, String, Vec<u8>)>, rigger::ingest::IngestStats) {
    let mut seq: Vec<(String, String, Vec<u8>)> = Vec::new();
    let stats = rigger::ingest::ingest_project(root, |key, ev| {
        seq.push((key.to_string(), ev.type_.clone(), ev.data.clone()));
    });
    (seq, stats)
}

#[cfg(feature = "symbols")]
fn drive_paced(
    root: &str,
    workers: usize,
) -> (Vec<(String, String, Vec<u8>)>, rigger::ingest::IngestStats) {
    let mut seq: Vec<(String, String, Vec<u8>)> = Vec::new();
    let stats = rigger::ingest::ingest_project_paced(root, workers, |key, ev| {
        seq.push((key.to_string(), ev.type_.clone(), ev.data.clone()));
    });
    (seq, stats)
}

/// A multi-file fixture whose files all extract real symbols, returned with its root path alive for
/// the duration of the test.
#[cfg(feature = "symbols")]
fn multi_file_fixture(n: usize) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..n {
        std::fs::write(
            dir.path().join(format!("m{i}.rs")),
            format!("pub fn f{i}() {{}}\npub fn g{i}() {{ f{i}(); }}\n"),
        )
        .unwrap();
    }
    dir
}

/// The DEFAULT-width public entry `ingest_project` is what the production sinks (`main.rs`
/// cmd_graph_build, `conductor.rs` ingest_project_batches) actually call, and it is what picks the
/// machine's core count through `default_workers()`. The inside-out criterion-1 test drives only the
/// PACED variant at a fixed width, so nothing pins the default entry. This proves it at the crate
/// boundary: its emit is byte-identical to the serial oracle (`ingest_project_paced(root, 1)`), its
/// engagement matches a paced walk at `default_workers()` width (so the default-width authority seam
/// is honored), and on a multi-core host it genuinely fans out.
#[cfg(feature = "symbols")]
#[test]
fn ingest_project_default_entry_is_byte_identical_to_the_serial_oracle() {
    let dir = multi_file_fixture(6);
    let root = dir.path().to_str().unwrap();

    let (default_seq, default_stats) = drive_default(root);
    let (serial_seq, serial_stats) = drive_paced(root, 1);
    let (paced_default_seq, paced_default_stats) =
        drive_paced(root, rigger::parallel::default_workers());

    assert!(
        !default_seq.is_empty(),
        "the six-file fixture emits code-ingest events through the default public entry"
    );
    // Byte-identical to serial: same keys, types, and payload bytes, in the same order.
    assert_eq!(
        default_seq, serial_seq,
        "the default-width public entry emits the byte-identical sequence a serial walk (width 1) \
         would - parallelism is observationally invisible"
    );
    // The default entry IS a paced walk at `default_workers()` width: same sequence and same
    // engagement, proving it routes through the one default-width authority rather than a private
    // copy of the policy.
    assert_eq!(
        default_seq, paced_default_seq,
        "ingest_project == ingest_project_paced(root, default_workers()) as a full sequence"
    );
    assert_eq!(
        default_stats, paced_default_stats,
        "ingest_project's stats match a paced walk at the default width (the default-width authority \
         seam is honored)"
    );

    // Batch bookkeeping: both widths emit the same number of file batches, at least one per source.
    assert_eq!(
        default_stats.batches_emitted, serial_stats.batches_emitted,
        "the default and serial walks emit the same number of file batches"
    );
    assert!(
        default_stats.batches_emitted >= 6,
        "each of the six source files contributes a batch; got {}",
        default_stats.batches_emitted
    );
    // The serial oracle reports exactly one engaged worker; the default entry never under-reports it.
    assert_eq!(
        serial_stats.workers_engaged, 1,
        "the serial oracle (width 1) runs inline: exactly one engaged worker"
    );
    assert!(
        default_stats.workers_engaged >= 1,
        "the default entry always engages at least one worker; got {}",
        default_stats.workers_engaged
    );
    // On a multi-core host the default entry genuinely parallelizes; on a single core it degrades to
    // the serial path. This conditional keeps the fan-out proof honest without flaking on one core.
    if rigger::parallel::default_workers() > 1 {
        assert!(
            default_stats.workers_engaged > 1,
            "on a multi-core host the default entry fans the six-file parse across workers; got {}",
            default_stats.workers_engaged
        );
    }
}

/// The `project_batches_paced` grounder entry is a separately-exported public item that ALSO feeds a
/// path the ingest emit does not observe: the legacy `project_batches` facade delegates to it, and
/// the 29c/live-ingest suites plus the design half depend on that facade's output being unchanged.
/// This pins the facade parity and width-invariance at the grounder's OWN boundary (distinct from
/// the ingest-emit observable): the per-file `(type, bytes)` content is identical at widths 1 and 8
/// and equal to the facade, in the same sorted-path order, while engagement honestly reflects width.
#[cfg(feature = "symbols")]
#[test]
fn project_batches_paced_is_width_invariant_and_matches_the_facade() {
    use rigger::eventstore::Event;
    use rigger::grounder::symbols::events::{project_batches, project_batches_paced};
    use std::collections::BTreeMap;

    // The content observable of one file's batch: its `(type, bytes)` events, order-carrying.
    type FileContent = (String, Vec<(String, Vec<u8>)>);

    // Normalize a batch set to its content observable: the file path (order-carrying) mapped to the
    // ordered `(type, bytes)` of its events - independent of any run-varying Event field.
    fn norm(batches: &[(String, Vec<Event>)]) -> Vec<FileContent> {
        batches
            .iter()
            .map(|(file, evs)| {
                (
                    file.clone(),
                    evs.iter()
                        .map(|e| (e.type_.clone(), e.data.clone()))
                        .collect(),
                )
            })
            .collect()
    }

    let dir = multi_file_fixture(6);
    let root = dir.path().to_str().unwrap();

    let (w1, engaged_1) = project_batches_paced(root, 1);
    let (w8, engaged_8) = project_batches_paced(root, 8);
    let facade = project_batches(root);

    let n1 = norm(&w1);
    let n8 = norm(&w8);
    let nf = norm(&facade);

    assert!(
        !n1.is_empty(),
        "the six-file fixture lowers to code batches"
    );
    assert_eq!(
        n1, n8,
        "project_batches_paced is width-invariant: widths 1 and 8 yield identical per-file content \
         in identical sorted-path order"
    );
    assert_eq!(
        n1, nf,
        "the legacy project_batches facade returns exactly what project_batches_paced produces - \
         the facade's Vec output is unchanged by the widening"
    );

    // The batches come back in ascending file-path order (index-preserving), so the emit sequence
    // never depends on which worker finished first.
    let files: Vec<&String> = n8.iter().map(|(f, _)| f).collect();
    let mut sorted = files.clone();
    sorted.sort();
    assert_eq!(
        files, sorted,
        "batches are returned in sorted file-path order; got {files:?}"
    );
    // No path is produced twice (the parallel concatenation preserves the index's set of files).
    let mut counts: BTreeMap<&String, usize> = BTreeMap::new();
    for f in &files {
        *counts.entry(f).or_default() += 1;
    }
    assert!(
        counts.values().all(|&c| c == 1),
        "each file appears exactly once across the concatenated chunks"
    );

    // Engagement is honest: width 1 runs inline (one worker), width 8 over several files fans out.
    assert_eq!(
        engaged_1, 1,
        "width 1 runs the lowering inline: exactly one engaged worker"
    );
    assert!(
        engaged_8 > 1,
        "width 8 over a multi-file index engages more than one parse worker; got {engaged_8}"
    );
}
