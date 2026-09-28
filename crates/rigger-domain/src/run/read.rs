//! THE ONE READ A ONE-SHOT COMMAND FOLDS (spec 101): the current run's own events and the
//! knowledge a fold carries over from earlier runs, read through the [`EventStore`] port so the
//! store - never the caller - refuses what a command must not materialize. [`crate::run`] folds
//! the slice this hands back ([`crate::run::current_run`] and friends); this module decides which
//! events reach it. A use case over the port only: it names no adapter and touches no file.

use crate::contextgraph::{TYPE_DECISION_MADE, TYPE_LESSON_LEARNED, TYPE_REVIEW_FINDING};
use crate::eventstore::{Error, Event, EventStore, TypeSelection};
use crate::ingest::DERIVED_INDEX_TYPES;
use crate::ledger::{TYPE_UNIT_FAILED, TYPE_UNIT_INTEGRATED, TYPE_UNIT_STARTED, TYPE_UNIT_STATUS};

use super::TYPE_RUN_STARTED;

/// The CARRIED-OVER KNOWLEDGE: the event types a fold consults across runs - the decisions, the
/// lessons (the playbooks distill from them) and the review findings - read by type over the
/// whole stream, however many runs came before.
pub const CARRY_OVER_TYPES: [&str; 3] =
    [TYPE_DECISION_MADE, TYPE_LESSON_LEARNED, TYPE_REVIEW_FINDING];

/// The lifecycle types criterion ADOPTION consults across runs (spec 88: a criterion's abandoned
/// attempt from an earlier run is adopted): the run boundaries that scope each attempt to its
/// spec, the unit starts that name a criterion, the integrations and compensated failures that
/// settle one, and the unit statuses that record an adoption or a quarantined branch. Read by
/// type over the whole stream, beside [`CARRY_OVER_TYPES`], so an adopting step never
/// materializes a prior run's other events.
pub const ADOPTION_TYPES: [&str; 5] = [
    TYPE_RUN_STARTED,
    TYPE_UNIT_STARTED,
    TYPE_UNIT_INTEGRATED,
    TYPE_UNIT_FAILED,
    TYPE_UNIT_STATUS,
];

/// The current run's own events, from its boundary forward, with the derived index types
/// excluded at the store, together with the carried-over knowledge of every run by type - in log
/// (position) order, each event once.
///
/// A LOG PREFIX, whatever a concurrent writer appends. The carried-over knowledge is read first
/// and the run slice second, and the slice refuses only the derived types, so it holds the run's
/// own decisions, lessons and findings too. Every carried-over event at or after the boundary was
/// committed before the slice read began, so the slice holds it as well, and merging the two
/// reads by position and dropping the repeated positions leaves exactly every non-derived event
/// of the run up to the slice's head plus the carried-over events before the boundary: an event
/// appended between the two reads is in the slice, never missing beside a later one.
///
/// The run slice starts at the boundary's POSITION ([`EventStore::read_stream_typed`] anchors
/// `from` on the event at that revision), so a row a stale writer reissued after the boundary at a
/// low revision is read where the log recorded it, and a fold over this sees the disorder. A
/// command that folds [`crate::run::current_run`] over this sees exactly the slice it saw over the
/// whole stream, and a cross-run fold of decisions, lessons and findings sees every one of them;
/// what it never materializes is a prior run's other events or any derived event.
///
/// The boundary is [`EventStore::last_position`]'s answer for the run's `RunStarted`. With no
/// run started yet the whole stream is the run, so everything but the derived types is read.
pub fn read_run(store: &dyn EventStore, stream: &str) -> Result<Vec<Event>, Error> {
    let Some(boundary) = store.last_position(stream, TYPE_RUN_STARTED)? else {
        return store.read_stream_typed(stream, 0, TypeSelection::Except(&DERIVED_INDEX_TYPES));
    };
    let mut events = store.read_stream_typed(stream, 0, TypeSelection::Only(&CARRY_OVER_TYPES))?;
    events.extend(store.read_stream_typed(
        stream,
        boundary,
        TypeSelection::Except(&DERIVED_INDEX_TYPES),
    )?);
    events.sort_by_key(|e| e.position);
    events.dedup_by_key(|e| e.position);
    Ok(events)
}

/// THE CURRENT RUN, READ ONCE: [`read_run`]'s slice of the current run
/// ([`crate::run::current_run`]) and the run's id ([`crate::run::current_run_id`], empty when no
/// run has started) - the one composition every one-shot command, the MCP tools and a step's
/// run-scoped folds share.
pub fn read_current_run(
    store: &dyn EventStore,
    stream: &str,
) -> Result<(Vec<Event>, String), Error> {
    let read = read_run(store, stream)?;
    let run_id = super::current_run_id(&read).unwrap_or_default();
    Ok((super::current_run(&read).to_vec(), run_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_carried_over_and_adoption_types_are_exactly_these() {
        assert_eq!(
            CARRY_OVER_TYPES,
            ["DecisionMade", "LessonLearned", "ReviewFinding"]
        );
        assert_eq!(
            ADOPTION_TYPES,
            [
                "RunStarted",
                "UnitStarted",
                "UnitIntegrated",
                "UnitFailed",
                "UnitStatus"
            ]
        );
    }
}
