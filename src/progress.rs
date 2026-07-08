//! Live agent-progress telemetry (spec 14, Gap 27): a SEPARATE, non-replayed event store
//! that records what an agent is doing BETWEEN milestones.
//!
//! The event store the conductor drives records only milestones - `DecisionMade`,
//! `SpawnResult`, `GateVerdict` - so an agent can work for many minutes (grounding,
//! reading, editing, running gates) with the run stream showing a blackout. This store
//! closes that blind spot WITHOUT touching the replay-authoritative log: it lives in its
//! own file (`.rigger/progress.db`, a sibling of `.rigger/graph.db`), so NO run fold - the
//! ledger, the spawn frontier, the metrics, the conductor, the context graph - can ever
//! read it, and the run stream, its projections, and replay are byte-identical whether or
//! not any progress was ever emitted. Agents WRITE it via `rigger progress <id>
//! "<activity>"`; rigger READS it (unit 2's consolidator) to PRESENT a live per-agent view.

use serde::{Deserialize, Serialize};

use crate::eventstore::{Error, Event, EventStore, ExpectedRevision, Position};
use crate::run::META_RUN_ID;

/// The stream the progress store's events live on WITHIN its own db file. Its file already
/// isolates it from the run stream; the dedicated stream name keeps the store
/// self-describing (and lets a consolidator read exactly the progress events).
pub const STREAM: &str = "progress";

/// The event type an [`AgentProgress`] serializes as. It is deliberately NOT in the run
/// store, so nothing that folds the run stream can observe it - the isolation is the file
/// boundary, not a fold that skips this type.
pub const TYPE_AGENT_PROGRESS: &str = "AgentProgress";

/// One fine-grained progress report: the spawn it belongs to and a short human line of what
/// that agent just did (a grep, a build, a commit, a decision). The event's recorded
/// position and `recorded_at` order a spawn's reports and give each an age, so no explicit
/// sequence field is carried - the store's append ordering subsumes it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentProgress {
    /// The deterministic spawn id this report is about (e.g. `s12-unit4/implementer#0`).
    pub id: String,
    /// A short, human, one-line description of the agent's latest step.
    pub activity: String,
}

impl AgentProgress {
    /// Build the appendable event, stamped with the run it belongs to (via [`META_RUN_ID`],
    /// the same key the conductor stamps on run events) so unit 2's consolidator can scope
    /// progress to the current run. An empty `run_id` (no run started yet) carries no stamp.
    fn to_event(&self, run_id: &str) -> Result<Event, serde_json::Error> {
        let ev = Event::new(TYPE_AGENT_PROGRESS, serde_json::to_vec(self)?);
        Ok(if run_id.is_empty() {
            ev
        } else {
            ev.with_meta(META_RUN_ID, run_id)
        })
    }
}

/// Record one progress report to the progress `store`, stamped with `run_id`. Append-only
/// and side-effect-free beyond the one event: a pure write, cheap to call after every
/// significant step. Returns the global position of the appended event.
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
    store.append(STREAM, ExpectedRevision::Any, std::slice::from_ref(&ev))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conductor;
    use crate::eventstore::sqlite::Store;
    use crate::eventstore::Direction;

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
}
