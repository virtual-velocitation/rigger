//! The write half of [`crate::progress`] (spec 93, criterion 1: THE CORE LANE IS PURE):
//! recording one live progress report to a LIVE progress store. [`crate::progress`] keeps
//! the model (`AgentProgress`, `AgentActivity`) and the pure fold (`consolidate`) -
//! everything computable from already-read `&[Event]` slices, with no store at all. This
//! module is the other half: `record`, the one function that appends to a
//! `&dyn EventStore`, split into its own file (never a function-by-function `#[cfg]`
//! inside `progress.rs`) so the pure half can compile for `wasm32-unknown-unknown`.

use crate::eventstore::{Error, EventStore, ExpectedRevision, Position};
use crate::progress::{AgentProgress, STREAM};

/// Record one progress report to the progress `store`, stamped with `run_id`. Append-only
/// and side-effect-free beyond the one event: a pure write, cheap to call after every
/// significant step. Returns the global position the store issued for the report, or the
/// failure a store that wrote nothing has earned ([`crate::eventstore::Appended::one`]).
pub fn record(
    store: &dyn EventStore,
    run_id: &str,
    id: &str,
    activity: &str,
) -> Result<Position, Error> {
    let progress = AgentProgress {
        id: id.to_string(),
        activity: activity.to_string(),
    };
    let ev = progress
        .to_event(run_id)
        .map_err(|e| Error::Backend(format!("serialize AgentProgress: {e}")))?;
    store
        .append(STREAM, ExpectedRevision::Any, std::slice::from_ref(&ev))?
        .one(&format!("the progress report of {id}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conductor;
    use crate::eventstore::sqlite::Store;
    use crate::eventstore::{Direction, Event};
    use crate::progress::TYPE_AGENT_PROGRESS;
    use crate::run::META_RUN_ID;

    #[test]
    fn progress_lands_in_its_own_store_never_the_run_stream() {
        // spec 14, criterion 1: `rigger progress` records to the progress store, NEVER the
        // run stream - the isolation is the file boundary. A separate store cannot enter a
        // run fold, so the run stream and its projection are byte-identical with or without
        // progress. Two distinct stores stand in for `.rigger/events.db` and
        // `.rigger/progress.db`.
        let run = Store::open(":memory:").unwrap();
        let progress = Store::open(":memory:").unwrap();

        // A run stream with a lifecycle event.
        run.append(
            conductor::STREAM,
            ExpectedRevision::Any,
            &[Event::new("UnitStarted", b"{\"id\":\"u\"}".to_vec())],
        )
        .unwrap();
        let before = run
            .read_stream(conductor::STREAM, 0, Direction::Forward)
            .unwrap();

        // Record progress to the SEPARATE store.
        record(
            &progress,
            "run-1",
            "u/implementer#0",
            "grep #3: conductor.rs",
        )
        .unwrap();

        // The run stream did not grow, and the run store holds NO AgentProgress anywhere -
        // progress never entered the replay-authoritative log.
        let after = run
            .read_stream(conductor::STREAM, 0, Direction::Forward)
            .unwrap();
        assert_eq!(
            before.len(),
            after.len(),
            "recording progress must not touch the run stream"
        );
        assert!(
            run.read_stream(STREAM, 0, Direction::Forward)
                .unwrap()
                .is_empty(),
            "no AgentProgress may land in the run store"
        );

        // The report landed in the progress store's progress stream, stamped with its run.
        let p = progress.read_stream(STREAM, 0, Direction::Forward).unwrap();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].type_, TYPE_AGENT_PROGRESS);
        assert_eq!(
            p[0].meta.get(META_RUN_ID).map(String::as_str),
            Some("run-1"),
            "the report is scoped to the run that emitted it"
        );
        let ap: AgentProgress = serde_json::from_slice(&p[0].data).unwrap();
        assert_eq!(ap.id, "u/implementer#0");
        assert_eq!(ap.activity, "grep #3: conductor.rs");
    }

    #[test]
    fn an_empty_run_id_records_without_a_run_stamp() {
        // Before any run has started (a legacy or bootstrapping store) progress still
        // records - it simply carries no run scope.
        let progress = Store::open(":memory:").unwrap();
        record(&progress, "", "u/implementer#0", "starting").unwrap();
        let p = progress.read_stream(STREAM, 0, Direction::Forward).unwrap();
        assert_eq!(p.len(), 1);
        assert!(
            !p[0].meta.contains_key(META_RUN_ID),
            "no run stamp when no run has started"
        );
    }

    /// A PROGRESS REPORT THE STORE DID NOT WRITE IS NOT A REPORT, and this seam is the one
    /// an agent calls after every step to prove it is not stalled.
    ///
    /// The absence used to come back as `Ok(None)` - a success the caller had to interpret,
    /// and the command above it interpreted it by printing that the report was recorded.
    /// A live view built on reports that were never written is the exact blackout this
    /// feature exists to close, so the seam asks the one authority instead: the position,
    /// or the failure.
    #[test]
    fn a_progress_report_the_store_did_not_write_is_reported_as_lost() {
        let err = record(
            &crate::eventstore::SilentStore,
            "run-1",
            "u1/implementer#0",
            "did a thing",
        )
        .expect_err("a progress report nobody can find was not recorded");
        let message = err.to_string();
        assert!(
            message.contains("nothing"),
            "the failure says the store wrote nothing: {message}"
        );
        assert!(
            message.contains("u1/implementer#0"),
            "and names the spawn whose report was lost: {message}"
        );
    }
}
