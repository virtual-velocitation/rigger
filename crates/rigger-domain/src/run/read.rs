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

/// What the run slice refuses at the store: the derived index types (their fold is `graph.db`)
/// and the carried-over types (already read whole by type, so never read twice). Built from the
/// two lists, so a type added to either is refused here too.
const RUN_SLICE_REFUSES: [&str; DERIVED_INDEX_TYPES.len() + CARRY_OVER_TYPES.len()] = {
    let mut refused = [""; DERIVED_INDEX_TYPES.len() + CARRY_OVER_TYPES.len()];
    let mut i = 0;
    while i < refused.len() {
        refused[i] = if i < DERIVED_INDEX_TYPES.len() {
            DERIVED_INDEX_TYPES[i]
        } else {
            CARRY_OVER_TYPES[i - DERIVED_INDEX_TYPES.len()]
        };
        i += 1;
    }
    refused
};

/// The current run's own events, from its boundary forward, with the derived index types
/// excluded at the store, together with the carried-over knowledge by type - in log (position)
/// order, each event once. The run slice starts at the boundary's POSITION
/// ([`EventStore::read_stream_typed`] anchors `from` on the event at that revision), so a row a
/// stale writer reissued after the boundary at a low revision is read where the log recorded it,
/// and a fold over this sees the disorder. A command that folds [`crate::run::current_run`] over
/// this sees exactly the slice it saw over the whole stream, and a cross-run fold of decisions,
/// lessons and findings sees every one of them; what it never materializes is a prior run's
/// other events or any derived event.
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
        TypeSelection::Except(&RUN_SLICE_REFUSES),
    )?);
    events.sort_by_key(|e| e.position);
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_run_slice_refuses_exactly_the_derived_and_the_carried_over_types() {
        assert_eq!(
            RUN_SLICE_REFUSES,
            [
                "CodeEntityExtracted",
                "EdgeInferred",
                "DocConceptExtracted",
                "DocLinkExtracted",
                "DecisionMade",
                "LessonLearned",
                "ReviewFinding",
            ]
        );
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
