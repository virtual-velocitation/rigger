//! THE ONE READ A ONE-SHOT COMMAND FOLDS (spec 101): the current run's own events and the
//! knowledge a fold carries over from earlier runs, read through the [`EventStore`] port so the
//! store - never the caller - refuses what a command must not materialize. [`crate::run`] folds
//! the slice this hands back ([`crate::run::current_run`] and friends); this module decides which
//! events reach it. A use case over the port only: it names no adapter and touches no file.

use crate::contextgraph::{TYPE_DECISION_MADE, TYPE_LESSON_LEARNED, TYPE_REVIEW_FINDING};
use crate::eventstore::{Error, Event, EventStore, Revision, TypeSelection};
use crate::ledger::{
    self, Hold, RunState, TYPE_UNIT_FAILED, TYPE_UNIT_INTEGRATED, TYPE_UNIT_STARTED,
    TYPE_UNIT_STATUS,
};
use crate::progress::closed_after_release;
use crate::retention::PERCEPTION_TYPES;

use super::{TYPE_RUN_HELD, TYPE_RUN_RELEASED, TYPE_RUN_STARTED};

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

/// The types a run's HOLD folds from ([`read_current_hold`], and every hold writer's re-read of
/// what landed behind it): the hold events, and the run boundaries, so a run started between the
/// hold read's two store calls still slices the read to the newest run. A boundary leaves the
/// hold a fold carries untouched.
pub const HOLD_TYPES: [&str; 3] = [TYPE_RUN_STARTED, TYPE_RUN_HELD, TYPE_RUN_RELEASED];

/// The current run's own events, from its boundary forward, with perception
/// ([`PERCEPTION_TYPES`]: the derived index types and the ledger entry that stands for them)
/// excluded at the store, together with the carried-over knowledge of every run by type - in log
/// (position) order, each event once.
///
/// A LOG PREFIX, whatever a concurrent writer appends. The carried-over knowledge is read first
/// and the run slice second, and the slice refuses only the perception types, so it holds the run's
/// own decisions, lessons and findings too. Every carried-over event at or after the boundary was
/// committed before the slice read began, so the slice holds it as well, and merging the two
/// reads by position and dropping the repeated positions leaves exactly every non-perception event
/// of the run up to the slice's head plus the carried-over events before the boundary: an event
/// appended between the two reads is in the slice, never missing beside a later one.
///
/// The run slice starts at the boundary's POSITION ([`EventStore::read_stream_typed`] anchors
/// `from` on the event at that revision), so a row a stale writer reissued after the boundary at a
/// low revision is read where the log recorded it, and a fold over this sees the disorder. A
/// command that folds [`crate::run::current_run`] over this sees exactly the slice it saw over the
/// whole stream, and a cross-run fold of decisions, lessons and findings sees every one of them;
/// what it never materializes is a prior run's other events or any perception event.
///
/// The boundary is [`EventStore::last_position`]'s answer for the run's `RunStarted`. With no
/// run started yet the whole stream is the run, so everything but the perception types is read.
pub fn read_run(store: &dyn EventStore, stream: &str) -> Result<Vec<Event>, Error> {
    let Some(boundary) = store.last_position(stream, TYPE_RUN_STARTED)? else {
        return store.read_stream_typed(stream, 0, TypeSelection::Except(&PERCEPTION_TYPES));
    };
    let mut events = store.read_stream_typed(stream, 0, TypeSelection::Only(&CARRY_OVER_TYPES))?;
    events.extend(store.read_stream_typed(
        stream,
        boundary,
        TypeSelection::Except(&PERCEPTION_TYPES),
    )?);
    events.sort_by_key(|e| e.position);
    events.dedup_by_key(|e| e.position);
    Ok(events)
}

/// [`read_run`]'s read and the newest revision in it, `None` on an empty stream: the head a
/// compare-and-append over that read pins to, and the revision a launch close stamps. Perception
/// events stay out of the read, so it can lag the stream's true head, and a close orders against
/// `RunHeld` and `RunReleased` alone.
pub fn read_run_at_head(
    store: &dyn EventStore,
    stream: &str,
) -> Result<(Vec<Event>, Option<Revision>), Error> {
    let run = read_run(store, stream)?;
    let head = run.iter().map(|e| e.revision).max();
    Ok((run, head))
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

/// THE CURRENT RUN'S ID, READ ALONE: [`crate::run::current_run_id`] of the run's `RunStarted`,
/// read from its boundary by `read_from_boundary`, `None` when no run has started - the read a
/// supervisor beat stamps its run by every interval, which wants one id and never the runs
/// before it or the run's other events.
pub fn read_current_run_id(store: &dyn EventStore, stream: &str) -> Result<Option<String>, Error> {
    let boundary = read_from_boundary(store, stream, &[TYPE_RUN_STARTED])?;
    Ok(super::current_run_id(&boundary))
}

/// THE ONE NARROW RUN READ: the events of `types` from the current run's boundary
/// ([`EventStore::last_position`]'s answer for the run's `RunStarted`; the whole stream with no
/// run started), which the hold read and the run-id read both take, so a read made every
/// interval never materializes the runs before the current one or the types it does not fold.
fn read_from_boundary(
    store: &dyn EventStore,
    stream: &str,
    types: &[&str],
) -> Result<Vec<Event>, Error> {
    let boundary = store.last_position(stream, TYPE_RUN_STARTED)?.unwrap_or(0);
    store.read_stream_typed(stream, boundary, TypeSelection::Only(types))
}

/// THE RUN'S HOLD, READ ONCE (spec 105): the hold the current run leaves open, the read every
/// launch gate and every poll through a hold decides on; an ending, whose close carries the run
/// stream's head, reads [`read_current_hold_at_head`] instead. It reads only [`HOLD_TYPES`] from
/// the run's boundary (`read_from_boundary`), and folds the hold from
/// [`crate::run::current_run`] of that read, so a read made
/// at every launch decision and through the whole of a hold never materializes the carried-over
/// knowledge or the run's other events. `None` proves the run unheld; a read or a fold that fails
/// answers the store's error, a fold's through [`fold_holds`] naming the run's hold, since a run
/// not proved unheld launches nothing.
pub fn read_current_hold(store: &dyn EventStore, stream: &str) -> Result<Option<Hold>, Error> {
    let events = read_from_boundary(store, stream, &HOLD_TYPES)?;
    let mut state = RunState::new();
    fold_holds(&mut state, super::current_run(&events))?;
    Ok(state.hold)
}

/// THE ONE HOLD FOLD: the [`HOLD_TYPES`] among `events` folded into `state` by
/// [`RunState::apply`], since a hold reads nothing else, which every hold writer folds the run it
/// read and what landed behind it through. A body no fold applies answers the store's error,
/// naming the run's hold.
pub fn fold_holds(state: &mut RunState, events: &[Event]) -> Result<(), Error> {
    events
        .iter()
        .filter(|e| HOLD_TYPES.contains(&e.type_.as_str()))
        .try_for_each(|e| state.apply(e))
        .map_err(|e| Error::Backend(format!("fold the run's hold: {e}")))
}

/// Whether the current run is held: [`read_current_hold`] answering an open hold, the one question
/// a launch's wait and a launch gate ask of it on every poll.
pub fn hold_stands(store: &dyn EventStore, stream: &str) -> Result<bool, Error> {
    Ok(read_current_hold(store, stream)?.is_some())
}

/// The hold [`read_current_hold`] answers and the head of the one read that answered it: the
/// stamp a close this hold decided carries, so a `RunReleased` landing after the read lies past the
/// close. It reads the run's slice ([`read_run_at_head`]) rather than the hold types alone, so its
/// head is the run stream's head, the stamp every other close carries, and closes keep one order.
/// An ending reads it once, never a poll.
pub fn read_current_hold_at_head(
    store: &dyn EventStore,
    stream: &str,
) -> Result<(Option<Hold>, Option<Revision>), crate::agent::Error> {
    let (read, head) = read_run_at_head(store, stream)?;
    Ok((ledger::project(super::current_run(&read))?.hold, head))
}

/// The hold the first `RunReleased` of the run on `store`'s `stream` past revision `stamp`
/// ended, the hold the run's current events before that release project to, with how long it
/// stood, or `None` when no release lies past `stamp` or the one that does ended no hold. A
/// release lies past `stamp` when a close so stamped does not follow it
/// ([`closed_after_release`]).
pub fn released_past(
    store: &dyn EventStore,
    stream: &str,
    stamp: Revision,
) -> Result<Option<(Hold, u64)>, crate::agent::Error> {
    let (events, _) = read_current_run(store, stream)?;
    let Some(at) = events.iter().position(|event| {
        event.type_ == TYPE_RUN_RELEASED && !closed_after_release(Some(stamp), Some(event.revision))
    }) else {
        return Ok(None);
    };
    let released: super::RunReleased = serde_json::from_slice(&events[at].data)?;
    let hold = ledger::project(&events[..at])?.hold;
    Ok(hold.map(|hold| (hold, released.held_for_s)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::DERIVED_INDEX_TYPES;
    use crate::retention::{GenerationIngested, TYPE_GENERATION_INGESTED};
    use crate::test_support::{
        ev, positions_of, types_of, CountedRead, HandBuiltLog, ReadCountingStore,
        ReadsFailWhenArmed, ONE_SHOT_PERCEPTION_TYPES,
    };
    use std::sync::atomic::AtomicBool;

    const STREAM: &str = "run";

    /// A hand-built ledger entry of `gc/src/a.rs` at `generation`.
    fn entry(generation: &str) -> Event {
        let entry = GenerationIngested {
            prefix: "gc".to_string(),
            file: "src/a.rs".to_string(),
            generation: generation.to_string(),
            blob: "b1".to_string(),
            excluded: false,
        };
        Event::new(
            TYPE_GENERATION_INGESTED,
            serde_json::to_vec(&entry).unwrap(),
        )
    }

    /// One event of each derived type, then a ledger entry at `generation`: all of perception.
    fn perception(generation: &str) -> Vec<Event> {
        let mut events: Vec<Event> = DERIVED_INDEX_TYPES.iter().map(|t| ev(t, "{}")).collect();
        events.push(entry(generation));
        events
    }

    /// READERS SKIP PERCEPTION, with a `RunStarted` before the entry: the run slice refuses the
    /// five perception types from the boundary, so the entry and every derived event recorded in
    /// the run are absent, as an earlier run's are, and the run's own events are all there.
    #[test]
    fn a_generation_ingested_after_the_run_started_is_absent_from_the_runs_slice() {
        let mut events = vec![ev("DecisionMade", r#"{"id":"d-prior"}"#), entry("h0")];
        events.push(ev("RunStarted", r#"{"run":"r2"}"#));
        events.push(ev("UnitStarted", r#"{"id":"u1"}"#));
        events.extend(perception("h1"));
        events.push(ev("DecisionMade", r#"{"id":"d-run"}"#));
        let log = HandBuiltLog::new(STREAM, events);
        let store = ReadCountingStore::new(&log);

        let (slice, run_id) = read_current_run(&store, STREAM).unwrap();
        assert_eq!(
            types_of(&slice),
            ["RunStarted", "UnitStarted", "DecisionMade"]
        );
        assert_eq!(positions_of(&slice), [3, 4, 10]);
        assert_eq!(run_id, "r2");
        assert_eq!(
            store.reads(),
            [
                CountedRead::LastPosition {
                    stream: STREAM.to_string(),
                    event_type: "RunStarted".to_string(),
                },
                CountedRead::Typed {
                    stream: STREAM.to_string(),
                    from: 0,
                    only: true,
                    types: CARRY_OVER_TYPES.map(String::from).to_vec(),
                    materialized: 2,
                },
                CountedRead::Typed {
                    stream: STREAM.to_string(),
                    from: 2,
                    only: false,
                    types: PERCEPTION_TYPES.map(String::from).to_vec(),
                    materialized: 3,
                },
            ]
        );
        assert_eq!(
            positions_of(&read_run(&log, STREAM).unwrap()),
            [1, 3, 4, 10]
        );
    }

    /// READERS SKIP PERCEPTION, with no `RunStarted`: the whole stream is the run, and its one
    /// read refuses the five perception types, so the entry and every derived event are absent.
    #[test]
    fn a_generation_ingested_with_no_run_started_is_absent_from_the_runs_slice() {
        let mut events = vec![ev("UnitStarted", r#"{"id":"u1"}"#)];
        events.extend(perception("h1"));
        events.push(ev("DecisionMade", r#"{"id":"d-run"}"#));
        let log = HandBuiltLog::new(STREAM, events);
        let store = ReadCountingStore::new(&log);

        let (slice, run_id) = read_current_run(&store, STREAM).unwrap();
        assert_eq!(types_of(&slice), ["UnitStarted", "DecisionMade"]);
        assert_eq!(positions_of(&slice), [1, 7]);
        assert_eq!(run_id, "");
        assert_eq!(
            store.reads(),
            [
                CountedRead::LastPosition {
                    stream: STREAM.to_string(),
                    event_type: "RunStarted".to_string(),
                },
                CountedRead::Typed {
                    stream: STREAM.to_string(),
                    from: 0,
                    only: false,
                    types: PERCEPTION_TYPES.map(String::from).to_vec(),
                    materialized: 2,
                },
            ]
        );
    }

    /// The hold [`read_current_hold`] answers is the one the current run's slice leaves open:
    /// none on an empty log or an unheld run, the open hold's latest cause, none once it is
    /// released and none for an earlier run's.
    #[test]
    fn the_current_hold_is_the_one_the_current_runs_slice_leaves_open() {
        let started = || ev("RunStarted", r#"{"run":"r1"}"#);
        let held = |cause: &str| ev(TYPE_RUN_HELD, &format!(r#"{{"cause":"{cause}"}}"#));
        let released = || ev(TYPE_RUN_RELEASED, "{}");
        for (name, events, cause) in [
            ("an empty log", vec![], None),
            ("an unheld run", vec![started()], None),
            (
                "a hold",
                vec![started(), held("rate_limit")],
                Some("rate_limit"),
            ),
            (
                "a hold whose cause moved",
                vec![started(), held("rate_limit"), held("billing_error")],
                Some("billing_error"),
            ),
            (
                "a released hold",
                vec![started(), held("rate_limit"), released()],
                None,
            ),
            (
                "a hold of an earlier run",
                vec![started(), held("rate_limit"), started()],
                None,
            ),
        ] {
            let log = HandBuiltLog::new(STREAM, events);
            let hold = read_current_hold(&log, STREAM).unwrap();
            assert_eq!(
                hold.as_ref().map(|hold| hold.latest.cause.as_str()),
                cause,
                "{name}"
            );
        }
    }

    /// A run-id read, a supervisor beat's every interval, asks the store for the run's
    /// `RunStarted` alone, from the boundary [`EventStore::last_position`] finds, the whole stream
    /// with no run started: the runs before the current one are never walked, and the answer is
    /// the current run's id.
    #[test]
    fn a_run_id_read_asks_only_the_runs_started_from_its_boundary() {
        let started = |run: &str| ev("RunStarted", &format!(r#"{{"run":"{run}"}}"#));
        let unit = || ev("UnitStarted", r#"{"id":"u1"}"#);
        // (row, the log, the boundary the read starts from, the events it materializes, the id)
        for (name, events, from, materialized, run) in [
            (
                "a run after an earlier one",
                vec![started("r1"), unit(), started("r2"), unit()],
                2,
                1,
                Some("r2"),
            ),
            ("a stream with no run started", vec![unit()], 0, 0, None),
        ] {
            let log = HandBuiltLog::new(STREAM, events);
            let store = ReadCountingStore::new(&log);

            let id = read_current_run_id(&store, STREAM).unwrap();
            assert_eq!(id.as_deref(), run, "{name}");
            assert_eq!(
                store.reads(),
                [
                    CountedRead::LastPosition {
                        stream: STREAM.to_string(),
                        event_type: "RunStarted".to_string(),
                    },
                    CountedRead::Typed {
                        stream: STREAM.to_string(),
                        from,
                        only: true,
                        types: vec!["RunStarted".to_string()],
                        materialized,
                    },
                ],
                "{name}"
            );
        }
    }

    /// A hold read asks the store for the run's `RunStarted`, `RunHeld` and `RunReleased` from
    /// the boundary [`EventStore::last_position`] finds, the whole stream with no run started,
    /// and nothing else (op-105-hold-read-reads-only-the-hold): no carried-over decision,
    /// lesson or finding and no other event of the run is materialized, and the answer is the
    /// hold the current run leaves open.
    #[test]
    fn a_hold_read_reads_only_the_runs_hold_types_from_its_boundary() {
        let started = |run: &str| ev("RunStarted", &format!(r#"{{"run":"{run}"}}"#));
        let held = |cause: &str| ev(TYPE_RUN_HELD, &format!(r#"{{"cause":"{cause}"}}"#));
        let hold_types = [TYPE_RUN_STARTED, TYPE_RUN_HELD, TYPE_RUN_RELEASED];
        // (row, the log, the boundary the read starts from, the events it materializes, the
        // open hold's cause)
        for (name, events, from, materialized, cause) in [
            (
                "a held run after an earlier one",
                vec![
                    started("r1"),
                    held("rate_limit"),
                    ev("DecisionMade", r#"{"id":"d-prior"}"#),
                    started("r2"),
                    ev("UnitStarted", r#"{"id":"u1"}"#),
                    held("billing_error"),
                    ev("LessonLearned", r#"{"id":"l-run"}"#),
                ],
                3,
                2,
                "billing_error",
            ),
            (
                "a held stream with no run started",
                vec![
                    ev("UnitStarted", r#"{"id":"u1"}"#),
                    held("rate_limit"),
                    ev("DecisionMade", r#"{"id":"d-run"}"#),
                ],
                0,
                1,
                "rate_limit",
            ),
        ] {
            let log = HandBuiltLog::new(STREAM, events);
            let store = ReadCountingStore::new(&log);

            let hold = read_current_hold(&store, STREAM).unwrap();
            assert_eq!(
                hold.as_ref().map(|hold| hold.latest.cause.as_str()),
                Some(cause),
                "{name}"
            );
            assert_eq!(
                store.reads(),
                [
                    CountedRead::LastPosition {
                        stream: STREAM.to_string(),
                        event_type: "RunStarted".to_string(),
                    },
                    CountedRead::Typed {
                        stream: STREAM.to_string(),
                        from,
                        only: true,
                        types: hold_types.map(String::from).to_vec(),
                        materialized,
                    },
                ],
                "{name}"
            );
        }
    }

    /// A run [`read_current_hold`] cannot prove unheld answers an error, never `None`: a failed
    /// read and a hold event no fold can apply each name their own failure.
    #[test]
    fn a_hold_read_that_fails_answers_its_error() {
        let armed = AtomicBool::new(true);
        let log = HandBuiltLog::new(STREAM, vec![]);
        let unreadable = ReadsFailWhenArmed {
            inner: &log,
            armed: &armed,
        };
        let unfoldable = HandBuiltLog::new(STREAM, vec![ev(TYPE_RUN_HELD, "not a hold")]);
        for (name, store, error) in [
            (
                "a failed read",
                &unreadable as &dyn EventStore,
                "event store: the run read failed",
            ),
            (
                "a hold no fold applies",
                &unfoldable as &dyn EventStore,
                "event store: fold the run's hold: expected ident at line 1 column 2",
            ),
        ] {
            assert_eq!(
                read_current_hold(store, STREAM).unwrap_err().to_string(),
                error,
                "{name}"
            );
        }
    }

    /// The fixture's spelled-out list is the perception list, name for name and in order.
    #[test]
    fn the_one_shot_fixtures_perception_list_is_the_perception_types() {
        assert_eq!(ONE_SHOT_PERCEPTION_TYPES, PERCEPTION_TYPES);
    }

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
