//! Periphery (API / contract / integration) tests for spec 58 criterion 4: `grep-fallback:`
//! progress lines recorded during a run are COUNTED and surfaced in the dash's review-outcomes
//! data. The count is a read over the SEPARATE progress store, NEVER the run stream, so the
//! run-stream projections and replay stay byte-identical whether or not any progress was emitted.
//!
//! These run OUTSIDE the crate, over the library's PUBLIC surface
//! (`rigger::progress::{AgentProgress, GREP_FALLBACK_PREFIX, record, STREAM}`,
//! `rigger::metrics::grep_fallbacks`, `rigger::dash::{build_state, state_json, MetricsView}`), so
//! they guard exactly the boundaries the inside-out unit tests (`src/metrics.rs mod tests` and
//! `src/dash.rs mod tests`, which reach the same functions via `super::` and hand-build events
//! with `Event::new`) are structurally blind to:
//!
//!  - PUBLIC REACHABILITY. The unit tests reach the metric, the predicate, the prefix const, and
//!    the `MetricsView.grep_fallbacks` field in-module. They never prove all four are `pub` and
//!    reachable as `rigger::...`. If any were accidentally crate-private, only a crate-external
//!    test fails to COMPILE - the inside-out tests would stay green.
//!  - THE WRITER->READER WIRE FORM. The unit tests hand-build `Event::new(TYPE_AGENT_PROGRESS,
//!    to_vec(&ap))` directly, bypassing the REAL writer path `progress::record` -> the private
//!    `AgentProgress::to_event` (which also stamps `META_RUN_ID`). The round-trip test drives the
//!    real writer into an in-memory store, reads the exact stored bytes back, and counts them -
//!    proving the serialized form the writer actually emits is what the counter counts.
//!  - THE PREFIX SINGLE-SOURCE-OF-TRUTH. `GREP_FALLBACK_PREFIX` is documented as the one
//!    definition of a fallback line, shared by writer and reader. The contract test binds the
//!    public predicate AND the public metric to the const itself (not a duplicated literal), so a
//!    future edit that drifts the predicate off the const reddens here.
//!  - THE SERVED ISOLATION INVARIANT. The unit test checks only the =2 case of one `build_state`.
//!    The integration test drives `build_state` (the exact function `serve` calls) twice over the
//!    SAME run events - once with a fallback progress slice, once with an empty slice - and proves
//!    progress changes ONLY `grep_fallbacks`; every other `MetricsView` field is byte-identical.
//!    That is the spec-58 run-stream isolation, asserted at the served boundary.
//!  - THE SERIALIZED /api/state KEY BACK-COMPAT. With an EMPTY progress slice the emitted body
//!    still carries `"grep_fallbacks":0` (the field is always serialized, never skipped), so the
//!    panel's `metrics.grep_fallbacks || 0` always reads a number, never `undefined`.
//!
//! `progress`, `metrics`, `dash`, and `eventstore::sqlite` are not feature-gated, so this guards
//! the counted-fallback contract on BOTH the default and the `--no-default-features` lane.

use std::collections::HashMap;

use rigger::contextgraph::Graph;
use rigger::dash::{build_state, state_json};
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, Event, EventStore, Position};
use rigger::metrics::grep_fallbacks;
use rigger::progress::{self, AgentProgress, GREP_FALLBACK_PREFIX};

// --- helpers -------------------------------------------------------------------------------------

/// A run-stream event with JSON `data` (the shape the ledger fold reads).
fn run_ev(type_: &str, json: &str) -> Event {
    Event::new(type_, json.as_bytes().to_vec())
}

/// An `AgentProgress` event as it lives in the progress store, built through the PUBLIC
/// `AgentProgress` type + `serde_json` (the counter reads exactly this typed slice).
fn progress_ev(id: &str, activity: &str) -> Event {
    let ap = AgentProgress {
        id: id.to_string(),
        activity: activity.to_string(),
    };
    Event::new(
        progress::TYPE_AGENT_PROGRESS,
        serde_json::to_vec(&ap).unwrap(),
    )
}

/// Give a slice 1-based positions, as the store would on append, so position-sensitive reads
/// (`consolidate`, `/api/events?since=`) are exercised realistically.
fn positioned(mut events: Vec<Event>) -> Vec<Event> {
    for (i, e) in events.iter_mut().enumerate() {
        e.position = (i + 1) as Position;
    }
    events
}

// --- (4) + (5) prefix single-source-of-truth + public reachability -------------------------------

/// CONTRACT: the public predicate `AgentProgress::is_grep_fallback` is bound to the public
/// `GREP_FALLBACK_PREFIX` const - the one definition of a fallback line the writer and the reader
/// share - and honors the documented leading-whitespace tolerance. Binding the assertions to the
/// const (not a duplicated `"grep-fallback:"` literal) means a predicate that drifts off the const
/// reddens here even if it still matched some other hardcoded string.
#[test]
fn is_grep_fallback_is_bound_to_the_public_prefix_const_and_tolerates_leading_space() {
    // The wire literal agents type per spec 58 - pinned so the writer's documented instruction and
    // the reader's const cannot silently diverge from what a human reads in the handbook/spec.
    assert_eq!(
        GREP_FALLBACK_PREFIX, "grep-fallback:",
        "the fallback wire literal is the spec-58 `grep-fallback:` prefix"
    );

    let fallback = AgentProgress {
        id: "u1/implementer#0".into(),
        activity: format!("{GREP_FALLBACK_PREFIX} no --show for effective_max_retries"),
    };
    assert!(
        fallback.is_grep_fallback(),
        "a line beginning with the const is a fallback"
    );

    let padded = AgentProgress {
        id: "u1/adversary#0".into(),
        activity: format!("   {GREP_FALLBACK_PREFIX} quoting Blocker body"),
    };
    assert!(
        padded.is_grep_fallback(),
        "a single leading run of whitespace before the const is tolerated"
    );

    let ordinary = AgentProgress {
        id: "u1/implementer#0".into(),
        activity: "cargo build green".into(),
    };
    assert!(
        !ordinary.is_grep_fallback(),
        "ordinary narration is not a fallback"
    );

    let mid_line = AgentProgress {
        id: "u1/implementer#0".into(),
        activity: format!("did a {GREP_FALLBACK_PREFIX} thing later"),
    };
    assert!(
        !mid_line.is_grep_fallback(),
        "the prefix must be at the START; a mid-line occurrence is not a fallback"
    );
}

// --- (3) round-trip through the REAL writer path -------------------------------------------------

/// ROUND-TRIP / API EDGE: drive the actual writer `progress::record` (which serializes through the
/// crate-private `AgentProgress::to_event`, stamping `META_RUN_ID`) into an in-memory store, read
/// the exact stored bytes back off `progress::STREAM`, and count them with the public
/// `metrics::grep_fallbacks`. This proves the serialized wire form the writer REALLY emits - not a
/// hand-built `Event` - is exactly what the counter counts, and that a run-id-stamped event still
/// counts (the metric reads the typed body, not the meta).
#[test]
fn the_recorded_writer_wire_form_round_trips_into_the_counter() {
    let store = Store::open(":memory:").unwrap();
    let run_id = "436b81a9-run";

    progress::record(
        &store,
        run_id,
        "u1/implementer#0",
        "grep-fallback: no --show for effective_max_retries",
    )
    .unwrap();
    // Ordinary narration through the same writer - must NOT count.
    progress::record(&store, run_id, "u1/implementer#0", "cargo build green").unwrap();
    progress::record(
        &store,
        run_id,
        "u1/adversary#0",
        "grep-fallback: quoting Blocker body, graph gave structure only",
    )
    .unwrap();

    // Read the exact bytes the writer stored, off the progress stream.
    let stored = store
        .read_stream(progress::STREAM, 0, Direction::Forward)
        .unwrap();
    assert_eq!(
        stored.len(),
        3,
        "all three writes landed on the progress stream"
    );
    assert!(
        stored
            .iter()
            .all(|e| e.type_ == progress::TYPE_AGENT_PROGRESS),
        "the writer stamps every progress event with the AgentProgress type"
    );

    assert_eq!(
        grep_fallbacks(&stored),
        2,
        "exactly the two writer-emitted `grep-fallback:` lines round-trip into the count"
    );
}

// --- (1) + (2) served isolation invariant + serialized wire-shape --------------------------------

/// Fold every `MetricsView` field EXCEPT `grep_fallbacks` into a JSON value, so two builds can be
/// compared for byte-identical run-stream-derived metrics regardless of the progress slice.
fn metrics_without_fallbacks(m: &rigger::dash::MetricsView) -> serde_json::Value {
    let mut v = serde_json::to_value(m).unwrap();
    v.as_object_mut()
        .unwrap()
        .remove("grep_fallbacks")
        .expect("MetricsView serializes a grep_fallbacks key");
    v
}

/// INTEGRATION over the SERVED boundary (`build_state`, the exact function `serve` calls): the
/// fallback count is read off the progress slice ONLY, so swapping the progress slice changes
/// `grep_fallbacks` and NOTHING else in the projection. The fixture carries a real
/// `units_started`/`review_approve` off the RUN STREAM so "all other fields identical" is a
/// meaningful claim (non-zero anchors), not a vacuous all-zeros comparison.
#[test]
fn build_state_counts_fallbacks_off_the_progress_slice_only_run_metrics_unchanged() {
    // A run-stream slice that yields units_started=1 and review_approve=1 (a `reviewed` UnitStatus).
    let events = positioned(vec![
        run_ev("UnitStarted", r#"{"id":"u1"}"#),
        run_ev("UnitStatus", r#"{"id":"u1","status":"green"}"#),
        run_ev("UnitStatus", r#"{"id":"u1","status":"reviewed"}"#),
    ]);

    // A progress slice with two `grep-fallback:` lines and one ordinary line.
    let progress_events = positioned(vec![
        progress_ev("u1/implementer#0", "grep-fallback: no --show for X"),
        progress_ev("u1/implementer#0", "cargo build green"),
        progress_ev("u1/adversary#0", "grep-fallback: quoting Blocker body"),
    ]);
    let empty: Vec<Event> = Vec::new();
    let liveness = HashMap::new();

    let with = build_state(
        &events,
        &Graph::default(),
        false,
        &progress_events,
        &liveness,
        3,
        "rigger-run",
        "origin/main",
    )
    .unwrap();
    let without = build_state(
        &events,
        &Graph::default(),
        false,
        &empty,
        &liveness,
        3,
        "rigger-run",
        "origin/main",
    )
    .unwrap();

    // The count reads the progress slice.
    assert_eq!(
        with.metrics.grep_fallbacks, 2,
        "the two fallback lines are counted into the review-outcomes data"
    );
    assert_eq!(
        without.metrics.grep_fallbacks, 0,
        "an empty progress slice counts zero fallbacks"
    );

    // The run-stream anchors are real, and IDENTICAL across the two progress slices.
    assert_eq!(
        with.metrics.units_started, 1,
        "the run stream yields units_started=1"
    );
    assert_eq!(
        with.metrics.review_approve, 1,
        "the `reviewed` status yields review_approve=1"
    );

    // Spec 58 ISOLATION at the served boundary: progress changes ONLY grep_fallbacks. Every other
    // MetricsView field (units_started, first_pass_clean, units_escalated, review_approve/reject,
    // the two ratios, and the gates vec) is byte-identical whether or not progress was emitted.
    assert_eq!(
        metrics_without_fallbacks(&with.metrics),
        metrics_without_fallbacks(&without.metrics),
        "no run-stream-derived metric moves when the progress slice changes"
    );
}

/// SERIALIZED WIRE-SHAPE / back-compat: `grep_fallbacks` is ALWAYS serialized into the `/api/state`
/// body `serve` ships, including as `0` when no progress was emitted (never skipped). So the JS
/// panel's `metrics.grep_fallbacks || 0` always reads a number, never `undefined`.
#[test]
fn the_state_json_body_always_carries_the_grep_fallbacks_key_even_at_zero() {
    let events = positioned(vec![run_ev("UnitStarted", r#"{"id":"u1"}"#)]);
    let empty: Vec<Event> = Vec::new();
    let liveness = HashMap::new();

    let body = state_json(
        &events,
        &Graph::default(),
        &empty,
        &liveness,
        3,
        "rigger-run",
        "origin/main",
    )
    .unwrap();

    // Parse the served body and reach the metrics.grep_fallbacks the panel reads.
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    let count = v
        .get("metrics")
        .and_then(|m| m.get("grep_fallbacks"))
        .expect("the served /api/state body always carries metrics.grep_fallbacks");
    assert_eq!(
        count,
        &serde_json::json!(0),
        "with no progress emitted the key is present and numeric zero, not absent"
    );
}
