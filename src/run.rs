//! Run scoping (spec 06, unit 1 - Gap 11): a run is the slice of the append-only
//! `run` stream that begins with a [`TYPE_RUN_STARTED`] event carrying a fresh run id.
//! The conductor folds ready work from ONLY the current run's slice, so a fresh run
//! never resurrects the non-terminal residue of an aborted prior run; prior runs stay
//! visible as memory (decisions, findings, `rigger peers`) but can never become live
//! work.
//!
//! Three read-models fold run state from the one stream - the [`crate::ledger`]
//! (durable run state), the [`crate::spawn`] frontier (the stepwise wave), and the
//! [`crate::metrics`] (`rigger stats`) - and all three scope through [`current_run`].
//! The run vocabulary (the `RunStarted` event type and the `run_id` metadata key every
//! conductor-emitted event carries) lives here so those modules share one source of
//! truth rather than re-declaring it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::conductor::STREAM;
use crate::contextgraph::{TYPE_DECISION_MADE, TYPE_LESSON_LEARNED, TYPE_REVIEW_FINDING};
use crate::eventstore::{Direction, Error, Event, EventStore, ExpectedRevision};

/// The event a run opens with: a fresh run id plus the acceptance criteria the run
/// satisfies. Deliberately NOT one of the lifecycle events the ledger folds - the
/// ledger and the context graph ignore an unknown type - so only the run-scoping
/// helpers here read it, exactly as [`crate::spawn::TYPE_SPAWN_REQUESTED`] is read only
/// by the spawn fold.
pub const TYPE_RUN_STARTED: &str = "RunStarted";

/// The metadata key that stamps every conductor-emitted event with the id of the run it
/// belongs to (spec 06, unit 1). Its value is the current run's uuid; a pre-run-id
/// (legacy) event carries no such key and so belongs to no run - it folds as "before the
/// first RunStarted" and never becomes live work.
pub const META_RUN_ID: &str = "run_id";

/// The metadata key carrying a definition hash on the events that pin one (spec 13, unit 1).
/// On a `--rebase-definition` supersession record it is the NEW (re-pinned) hash; the pin a
/// run opens with lives in the [`RunStarted::definition`] body. [`effective_definition`] reads
/// both through this one key + the body, so the current pin is a single fold over the run slice.
pub const META_DEFINITION: &str = "definition";

/// The metadata key carrying the OLD (superseded) definition hash on a `--rebase-definition`
/// record (spec 13, unit 1), so the supersession `old -> new` is legible in the log itself.
pub const META_DEFINITION_PRIOR: &str = "definition_prior";

/// The body of a [`TYPE_RUN_STARTED`] event: the run id, the criteria fingerprint, and the
/// pinned definition hash.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunStarted {
    /// The fresh run id (a uuid) every event in this run is scoped by.
    pub run: String,
    /// The acceptance criteria this run satisfies - the per-campaign fingerprint that
    /// tells a RESUME (same criteria: adopt this run) from a NEW campaign (different
    /// criteria: begin a fresh run). Empty for a no-spec workflow run.
    #[serde(default)]
    pub criteria: Vec<String>,
    /// The definition hash pinned at run start (spec 13, unit 1): a stable digest over the
    /// on-disk workflow.yml + agent-prompt set (see `main::definition_hash`). A live-run step
    /// whose recomputed on-disk hash differs from the run's EFFECTIVE pin
    /// ([`effective_definition`]) HALTS loudly - so a mid-campaign prompt edit can never
    /// silently change replay semantics - unless `--rebase-definition` records the supersession
    /// and re-pins. Empty on a legacy run started before pinning existed, and on any unpinned
    /// start; an empty pin never drifts. New runs are always free: a fresh boundary just pins
    /// the current hash.
    #[serde(default)]
    pub definition: String,
}

impl RunStarted {
    /// Decode a [`TYPE_RUN_STARTED`] body, or `None` if it is malformed (so a corrupt
    /// event is simply ignored, like every other fold here).
    fn from_event(e: &Event) -> Option<RunStarted> {
        serde_json::from_slice(&e.data).ok()
    }

    /// Build the appendable event for this run start, with the run id stamped in
    /// [`META_RUN_ID`] so the RunStarted itself belongs to its own run's slice.
    fn to_event(&self) -> Result<Event, serde_json::Error> {
        Ok(Event::new(TYPE_RUN_STARTED, serde_json::to_vec(self)?)
            .with_meta(META_RUN_ID, &self.run))
    }
}

/// The latest [`RunStarted`] in `events`, decoded, or `None` when no run has started.
fn latest(events: &[Event]) -> Option<RunStarted> {
    events
        .iter()
        .rev()
        .find(|e| e.type_ == TYPE_RUN_STARTED)
        .and_then(RunStarted::from_event)
}

/// The current run's slice of `events`: the contiguous suffix from the LAST
/// [`TYPE_RUN_STARTED`] onward. When no run has started (a legacy store, or one this
/// feature has never scoped), the WHOLE slice is returned - so a store predating run
/// scoping folds exactly as before, and every fold that runs directly over raw events
/// is unchanged until a run actually begins.
///
/// A run's events are exactly this suffix: events are appended in order and a new run's
/// `RunStarted` is always appended after every prior run's events, so scoping by the
/// last boundary is scoping by run. Everything before that boundary - a prior run's
/// events, or pre-run-id legacy events - is excluded and can never become live work.
pub fn current_run(events: &[Event]) -> &[Event] {
    match events.iter().rposition(|e| e.type_ == TYPE_RUN_STARTED) {
        Some(i) => &events[i..],
        None => events,
    }
}

/// The id of the current (latest) run, or `None` when no run has started.
pub fn current_run_id(events: &[Event]) -> Option<String> {
    latest(events).map(|r| r.run)
}

/// How a provenance-bearing event is attributed to a run (spec 21, unit 1).
///
/// The context graph spans runs by design (a unit inherits prior decisions), so a
/// decision or finding must be traced to the run whose `[RunStarted, next RunStarted)`
/// window contains the event that produced it - that provenance is what tells a live
/// decision from dead-run noise. This is the SINGLE authority for run attribution:
/// `rigger reset --runs` and `rigger peers` both derive their disposition from this one
/// window rule, never a second inline boundary scan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunOf {
    /// A decision or finding that falls inside a run's window: it belongs to this run id.
    Run(String),
    /// A decision or finding recorded BEFORE the first [`TYPE_RUN_STARTED`]. It belongs to
    /// no run (pre-boundary) - it is neither the active run nor a lesson, so it is dead-run
    /// noise that `reset --runs` drops and `rigger peers` labels historical.
    PreBoundary,
    /// A [`TYPE_LESSON_LEARNED`]: durable cross-run value, EXEMPT from attribution. A lesson
    /// is never placed in a run and so is never pruned as dead-run noise, regardless of its
    /// position (even one recorded before the first boundary). This is the "never attributed
    /// away" guarantee - a lesson is kept by its own rule, not by belonging to the active run.
    Lesson,
}

impl RunOf {
    /// Whether this node belongs to the ACTIVE run `active` (the id from
    /// [`current_run_id`]). This is the single `not-active => historical` rule both read
    /// paths reuse: `rigger peers` labels a decision LIVE when this holds and HISTORICAL
    /// otherwise, and `reset --runs` keeps a node when this holds OR it is a lesson. A
    /// [`RunOf::Lesson`] is deliberately NOT "live" (it is exempt, kept by a different rule)
    /// and a [`RunOf::PreBoundary`] never matches (it belongs to no run).
    pub fn is_live(&self, active: Option<&str>) -> bool {
        matches!((self, active), (RunOf::Run(run), Some(a)) if run == a)
    }
}

/// Attribute every provenance-bearing event in the ordered stream to its run (spec 21,
/// unit 1) - the single authority the prune and the peers labels both reuse.
///
/// Returns a map from an event's index in `events` to its [`RunOf`], with an entry ONLY
/// for the three provenance types ([`TYPE_DECISION_MADE`], [`TYPE_REVIEW_FINDING`],
/// [`TYPE_LESSON_LEARNED`]); a [`TYPE_RUN_STARTED`] or any lifecycle event is absent.
/// Keying by index (not the event's own metadata) is deliberate: an agent-emitted
/// decision carries no run id in its metadata, so only its POSITION relative to the
/// `RunStarted` boundaries can attribute it - the window is the one uniform source.
///
/// `events` MUST be the whole [`STREAM`] in forward (append) order, exactly as
/// [`current_run`] expects; the window boundaries are read from it. The rule is a single
/// forward fold: the "current run" starts empty (no boundary seen yet) and advances to
/// each `RunStarted`'s id as the fold passes it, so a decision/finding is attributed to
/// the last boundary at or before it ([`RunOf::Run`]) or to [`RunOf::PreBoundary`] when
/// none precedes it. A `LessonLearned` is always [`RunOf::Lesson`] regardless of window -
/// it is exempt and never attributed away. The `BTreeMap` iterates in index order, so the
/// derivation is deterministic (a spec-21 global constraint).
pub fn run_attribution(events: &[Event]) -> BTreeMap<usize, RunOf> {
    let mut attribution = BTreeMap::new();
    let mut current: Option<String> = None;
    for (i, e) in events.iter().enumerate() {
        match e.type_.as_str() {
            TYPE_RUN_STARTED => {
                // Advance the window: every event after this boundary (until the next
                // RunStarted) belongs to this run. Decoded from the body, the same source
                // `current_run_id` reads, never the event's own metadata.
                if let Some(rs) = RunStarted::from_event(e) {
                    current = Some(rs.run);
                }
            }
            TYPE_LESSON_LEARNED => {
                // Exempt regardless of window - a lesson is never attributed away.
                attribution.insert(i, RunOf::Lesson);
            }
            TYPE_DECISION_MADE | TYPE_REVIEW_FINDING => {
                let run_of = match &current {
                    Some(run) => RunOf::Run(run.clone()),
                    None => RunOf::PreBoundary,
                };
                attribution.insert(i, run_of);
            }
            _ => {}
        }
    }
    attribution
}

/// The definition hash currently in force for a run (spec 13, unit 1): the hash the run
/// pinned at start ([`RunStarted::definition`]), advanced by any `--rebase-definition`
/// supersession recorded since (each carries the re-pinned hash in [`META_DEFINITION`]).
/// The LAST authority wins, so a rebased run's effective pin is the rebased-to hash and a
/// plain step after a rebase no longer re-halts. Empty when the run pinned nothing (a
/// legacy/unpinned start), which the drift check reads as "unpinned - never drifts".
///
/// `run_slice` must be the CURRENT run's slice ([`current_run`]): it opens with the run's
/// one [`TYPE_RUN_STARTED`] and any rebase records for this run follow it.
pub fn effective_definition(run_slice: &[Event]) -> String {
    let mut pinned = String::new();
    for e in run_slice {
        if e.type_ == TYPE_RUN_STARTED {
            if let Some(rs) = RunStarted::from_event(e) {
                pinned = rs.definition;
            }
        } else if let Some(rebased) = e.meta.get(META_DEFINITION) {
            pinned = rebased.clone();
        }
    }
    pinned
}

/// The outcome of ensuring a definition-pinned run (spec 13, unit 1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunStart {
    /// The free path: a fresh run was minted (empty store / new campaign / `--fresh`) pinning
    /// the current definition, OR an in-force run was adopted whose effective pin agrees with
    /// the on-disk definition (or is unpinned). `.0` is the run id; nothing halts.
    Ready(String),
    /// A live run was adopted but its effective pinned definition DRIFTED from the on-disk
    /// hash and no rebase was requested: the caller must HALT loudly. Carries the run id, the
    /// pinned (old) hash, and the current on-disk (new) hash for the halt message.
    Drifted {
        run: String,
        pinned: String,
        current: String,
    },
    /// A live run drifted and `--rebase-definition` recorded the supersession
    /// (`pinned -> current`); the run continues on the re-pinned definition.
    Rebased {
        run: String,
        pinned: String,
        current: String,
    },
}

impl RunStart {
    /// The run id, whichever outcome this is.
    pub fn run(&self) -> &str {
        match self {
            RunStart::Ready(run) => run,
            RunStart::Drifted { run, .. } | RunStart::Rebased { run, .. } => run,
        }
    }
}

/// Record a `--rebase-definition` supersession on the current run (spec 13, unit 1): the
/// operator explicitly accepted the on-disk definition drift, so the run re-pins from `old`
/// to `new` and continues. It rides the existing `DecisionMade` vocabulary (no new event
/// type - the spec-13 global constraint) so it folds into the context graph as a decision,
/// and stamps the re-pinned hash in [`META_DEFINITION`] (and the superseded hash in
/// [`META_DEFINITION_PRIOR`]) so [`effective_definition`] advances and subsequent steps see
/// no drift. Scoped to the run via [`META_RUN_ID`], appended after the run's `RunStarted`.
fn record_rebase(store: &dyn EventStore, run: &str, old: &str, new: &str) -> Result<(), Error> {
    let decision = serde_json::json!({
        "id": format!("definition-rebase-{new}"),
        "summary": format!(
            "rigger --rebase-definition: accepted definition drift on run {run}; re-pinned {old} -> {new}"
        ),
    });
    let data = serde_json::to_vec(&decision)
        .map_err(|e| Error::Backend(format!("serialize rebase: {e}")))?;
    let ev = Event::new(TYPE_DECISION_MADE, data)
        .with_meta(META_RUN_ID, run)
        .with_meta(META_DEFINITION, new)
        .with_meta(META_DEFINITION_PRIOR, old);
    store.append(STREAM, ExpectedRevision::Any, std::slice::from_ref(&ev))?;
    Ok(())
}

/// Ensure a run is active for `criteria`, returning its run id.
///
/// If the latest run in the store was started for the SAME criteria, that run is
/// ADOPTED (resumed): nothing is appended, so a resume, an idle re-run, and a replay
/// step are all idempotent - no duplicate RunStarted fragments the run, and the
/// driver's `done` detection is preserved. Otherwise a FRESH run BEGINS: a new uuid
/// RunStarted stamped with `criteria` is appended and its id returned.
///
/// The criteria are the per-campaign fingerprint. They are the one signal derivable
/// from the log alone that distinguishes "continue the campaign in flight" from "a new
/// campaign over the same store", without re-minting on every step (which would split
/// one campaign across many runs). A legacy store with no RunStarted begins its first
/// run here; its prior events stay before that boundary and never become live work
/// (Gap 11: a new run no longer resurrects history's zombies).
pub fn ensure_started(store: &dyn EventStore, criteria: &[String]) -> Result<String, Error> {
    // The UNPINNED run start: an empty definition never drifts, so this only ever adopts or
    // mints - the historical behavior. The conductor calls this (definition pinning is
    // enforced once at the CLI boundary via [`ensure_started_pinned`]), so the two never
    // fight over the boundary: the CLI ensures the pinned run, the conductor adopts it.
    Ok(ensure_started_pinned(store, criteria, "", false)?
        .run()
        .to_string())
}

/// Ensure a run is active for `criteria` AND enforce its definition pin (spec 13, unit 1).
///
/// This is the single adopt-or-mint authority; [`ensure_started`] is the unpinned
/// convenience over it (`definition == ""`, `rebase == false`). `definition` is the current
/// on-disk definition hash (`main::definition_hash`); an empty `definition` disables pinning
/// (the drift check is skipped and nothing is pinned), so an un-pinned caller behaves exactly
/// as before this feature.
///
/// - Same-criteria run in the store (a RESUME / live-run step): the run is ADOPTED and its
///   [`effective_definition`] is compared to `definition`. Agreement (or either side empty -
///   an unpinned run, or pinning disabled) is [`RunStart::Ready`]. A mismatch is definition
///   DRIFT: with `rebase` it records the supersession ([`record_rebase`]) and re-pins
///   ([`RunStart::Rebased`]); without `rebase` it is [`RunStart::Drifted`] and the caller
///   HALTS loudly. The mid-campaign prompt edit that silently changes replay semantics - the
///   sharpest exposure spec 13 names - can no longer pass unnoticed.
/// - No matching run (a NEW campaign / empty store): a FRESH run is minted pinning
///   `definition` and returned as [`RunStart::Ready`]. New runs are always free - only a LIVE
///   run pins (R1's edit-to-reconfigure holds for run boundaries).
pub fn ensure_started_pinned(
    store: &dyn EventStore,
    criteria: &[String],
    definition: &str,
    rebase: bool,
) -> Result<RunStart, Error> {
    let events = store.read_stream(STREAM, 0, Direction::Forward)?;
    if let Some(run) = latest(&events) {
        if run.criteria.as_slice() == criteria {
            let pinned = effective_definition(current_run(&events));
            // Free when pinning is disabled (`definition` empty), the run is unpinned
            // (`pinned` empty - a legacy start), or the pin agrees with what is on disk.
            if definition.is_empty() || pinned.is_empty() || pinned == definition {
                return Ok(RunStart::Ready(run.run));
            }
            // Definition drift on a LIVE run.
            if rebase {
                record_rebase(store, &run.run, &pinned, definition)?;
                return Ok(RunStart::Rebased {
                    run: run.run,
                    pinned,
                    current: definition.to_string(),
                });
            }
            return Ok(RunStart::Drifted {
                run: run.run,
                pinned,
                current: definition.to_string(),
            });
        }
    }
    // A new campaign / empty store: a fresh run is always free - it pins the current definition.
    Ok(RunStart::Ready(start_fresh(store, criteria, definition)?))
}

/// Begin a FRESH run for `criteria`, UNCONDITIONALLY: mint a new uuid `RunStarted` and
/// append it, returning the new run id.
///
/// Unlike [`ensure_started`], this never adopts the latest run even when its criteria
/// match. It is the operator's explicit "start over" (`rigger run --fresh`) - the evented
/// recovery from a run wedged in a terminal state whose spec is UNCHANGED. A plan-critique
/// escalation is terminal within its run slice (the resume short-circuit holds the fan-out
/// forever), and the escalation-recovery every other case relies on - fix the spec, so its
/// criteria change and `ensure_started` mints a fresh run - does not apply when the spec is
/// correct and the escalation was a defect since fixed. Without this, `ensure_started`
/// would adopt the wedged run on every relaunch.
///
/// This is additive, not destructive: the prior run stays in the log as history and
/// cross-run context (its decisions and findings remain visible through the whole-stream
/// graph and `rigger peers`); the new boundary simply begins a clean slice AFTER it, so
/// the conductor folds ready work from an empty prior state ([`current_run`] scopes to the
/// new boundary) and the wedged gate runs anew. It does not touch the git run branch, so a
/// fresh run starts over atop whatever that branch already holds.
///
/// `definition` is the on-disk definition hash the new run PINS (spec 13, unit 1): a fresh
/// boundary is always free (it never drifts against a prior pin), and pinning the current
/// definition here is what lets a later live-run step detect a mid-campaign edit.
pub fn start_fresh(
    store: &dyn EventStore,
    criteria: &[String],
    definition: &str,
) -> Result<String, Error> {
    let started = RunStarted {
        run: uuid::Uuid::new_v4().to_string(),
        criteria: criteria.to_vec(),
        definition: definition.to_string(),
    };
    let ev = started
        .to_event()
        .map_err(|e| Error::Backend(format!("serialize RunStarted: {e}")))?;
    store.append(STREAM, ExpectedRevision::Any, std::slice::from_ref(&ev))?;
    Ok(started.run)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eventstore::sqlite::Store;

    fn ev(type_: &str, data: &str) -> Event {
        Event::new(type_, data.as_bytes().to_vec())
    }

    fn run_started(run: &str, criteria: &[&str]) -> Event {
        RunStarted {
            run: run.to_string(),
            criteria: criteria.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
        .to_event()
        .unwrap()
    }

    fn decision(id: &str) -> Event {
        ev(TYPE_DECISION_MADE, &format!(r#"{{"id":"{id}"}}"#))
    }
    fn finding(id: &str) -> Event {
        ev(TYPE_REVIEW_FINDING, &format!(r#"{{"id":"{id}"}}"#))
    }
    fn lesson(id: &str) -> Event {
        ev(TYPE_LESSON_LEARNED, &format!(r#"{{"id":"{id}"}}"#))
    }

    #[test]
    fn run_attribution_maps_decisions_to_their_window_and_never_attributes_lessons_away() {
        // Spec 21, unit 1 done-when: a decision/finding is attributed to the run whose
        // [RunStarted, next RunStarted) window contains its producing event, and a
        // LessonLearned is NEVER attributed away - it is exempt, even before the first
        // boundary. One store, two runs, plus pre-boundary residue.
        let events = vec![
            // Pre-boundary: recorded before any RunStarted - belongs to no run.
            decision("pre-d"),                      // 0
            finding("pre-f"),                       // 1
            lesson("pre-lesson"), // 2  a lesson before any boundary - still exempt
            run_started("r1", &["crit"]), // 3
            decision("d1"),       // 4
            finding("f1"),        // 5
            lesson("lesson-1"),   // 6  a lesson inside r1's window - exempt, not Run(r1)
            ev("UnitStarted", r#"{"id":"noise"}"#), // 7  not a provenance event
            run_started("r2", &["crit"]), // 8
            decision("d2"),       // 9
            lesson("lesson-2"),   // 10
        ];

        let attr = run_attribution(&events);

        // Pre-boundary decision/finding => PreBoundary (dead-run noise, belongs to no run).
        assert_eq!(attr.get(&0), Some(&RunOf::PreBoundary));
        assert_eq!(attr.get(&1), Some(&RunOf::PreBoundary));
        // A lesson is exempt regardless of position - even a pre-boundary one is Lesson.
        assert_eq!(attr.get(&2), Some(&RunOf::Lesson));
        // r1's decision and finding are attributed to r1's window.
        assert_eq!(attr.get(&4), Some(&RunOf::Run("r1".into())));
        assert_eq!(attr.get(&5), Some(&RunOf::Run("r1".into())));
        // A lesson INSIDE a run's window is still exempt - Lesson, never Run("r1").
        assert_eq!(attr.get(&6), Some(&RunOf::Lesson));
        // r2's decision is attributed to r2's window.
        assert_eq!(attr.get(&9), Some(&RunOf::Run("r2".into())));
        assert_eq!(attr.get(&10), Some(&RunOf::Lesson));
        // Boundaries and non-provenance events carry no attribution entry.
        assert!(!attr.contains_key(&3), "a RunStarted is not attributed");
        assert!(
            !attr.contains_key(&7),
            "a lifecycle event is not attributed"
        );
        assert!(!attr.contains_key(&8), "a RunStarted is not attributed");
        // Exactly the provenance events (3 decisions + 2 findings + 3 lessons), and only
        // those - the two boundaries and the lifecycle event carry no entry.
        assert_eq!(
            attr.len(),
            8,
            "every decision/finding/lesson event, and only those"
        );

        // The shared not-active => historical rule (c2 and c3 reuse it): with r2 the active
        // run, r2's decision is LIVE and r1's is HISTORICAL; a lesson is never "live" (kept
        // by its own exempt rule) and a pre-boundary node never matches the active run.
        let active = current_run_id(&events);
        assert_eq!(active.as_deref(), Some("r2"));
        assert!(
            attr[&9].is_live(active.as_deref()),
            "active-run decision is live"
        );
        assert!(
            !attr[&4].is_live(active.as_deref()),
            "superseded-run decision is historical"
        );
        assert!(
            !attr[&10].is_live(active.as_deref()),
            "a lesson is exempt, not live"
        );
        assert!(
            !attr[&0].is_live(active.as_deref()),
            "a pre-boundary node is historical"
        );
    }

    #[test]
    fn current_run_is_the_whole_slice_when_no_run_has_started() {
        // A legacy store (pre-run-id events, no RunStarted) folds whole-stream, exactly
        // as before run scoping - so every existing direct fold is untouched.
        let events = vec![
            ev("UnitStarted", r#"{"id":"a"}"#),
            ev("UnitStarted", r#"{"id":"b"}"#),
        ];
        assert_eq!(current_run(&events).len(), 2);
        assert!(current_run_id(&events).is_none());
    }

    #[test]
    fn current_run_is_the_suffix_from_the_last_run_started() {
        // Two prior units, then a run begins, then one live unit. The current run is the
        // RunStarted and everything after it - never the prior units.
        let events = vec![
            ev("UnitStarted", r#"{"id":"zombie-1"}"#),
            ev("UnitStarted", r#"{"id":"zombie-2"}"#),
            run_started("r1", &["crit"]),
            ev("UnitStarted", r#"{"id":"live"}"#),
        ];
        let slice = current_run(&events);
        assert_eq!(
            slice.len(),
            2,
            "RunStarted + the one live unit, never the zombies"
        );
        assert_eq!(slice[0].type_, TYPE_RUN_STARTED);
        assert_eq!(current_run_id(&events).as_deref(), Some("r1"));
        // None of the prior zombie units are in the current run's slice.
        assert!(
            !slice
                .iter()
                .any(|e| String::from_utf8_lossy(&e.data).contains("zombie")),
            "prior-run residue is excluded from the current run"
        );
    }

    #[test]
    fn current_run_is_the_suffix_from_the_latest_of_several_runs() {
        let events = vec![
            run_started("r1", &["a"]),
            ev("UnitStarted", r#"{"id":"r1-unit"}"#),
            run_started("r2", &["b"]),
            ev("UnitStarted", r#"{"id":"r2-unit"}"#),
        ];
        let slice = current_run(&events);
        assert_eq!(slice.len(), 2);
        assert_eq!(current_run_id(&events).as_deref(), Some("r2"));
        assert!(String::from_utf8_lossy(&slice[1].data).contains("r2-unit"));
    }

    #[test]
    fn ensure_started_mints_a_fresh_run_on_an_empty_store() {
        let store = Store::open(":memory:").unwrap();
        let run = ensure_started(&store, &["crit".to_string()]).unwrap();
        assert!(!run.is_empty(), "a fresh run id is minted");

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        let starts: Vec<_> = events
            .iter()
            .filter(|e| e.type_ == TYPE_RUN_STARTED)
            .collect();
        assert_eq!(starts.len(), 1, "exactly one RunStarted is appended");
        assert_eq!(
            starts[0].meta.get(META_RUN_ID).map(String::as_str),
            Some(run.as_str()),
            "the RunStarted carries its own run id in metadata"
        );
        assert_eq!(
            RunStarted::from_event(starts[0]).unwrap().criteria,
            ["crit"]
        );
    }

    #[test]
    fn ensure_started_adopts_the_same_criteria_run_without_re_minting() {
        // Resume / idle re-run / replay step: the same criteria adopt the existing run and
        // append NOTHING, so a run is never fragmented across step processes.
        let store = Store::open(":memory:").unwrap();
        let first = ensure_started(&store, &["crit".to_string()]).unwrap();
        let again = ensure_started(&store, &["crit".to_string()]).unwrap();
        assert_eq!(first, again, "the same criteria adopt the same run id");

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        assert_eq!(
            events
                .iter()
                .filter(|e| e.type_ == TYPE_RUN_STARTED)
                .count(),
            1,
            "adopting a run appends no second RunStarted"
        );
    }

    #[test]
    fn start_fresh_mints_a_new_run_even_when_the_criteria_match() {
        // `rigger run --fresh`: the operator's explicit "start over". Where `ensure_started`
        // ADOPTS a same-criteria run, `start_fresh` ALWAYS appends a new boundary, so a run
        // wedged in a terminal state (e.g. an escalated plan-critique) whose spec is
        // unchanged can be re-run cleanly. The prior run stays in the log; the new slice
        // begins after it.
        let store = Store::open(":memory:").unwrap();
        let first = ensure_started(&store, &["crit".to_string()]).unwrap();
        // Some residue lands in the first run (a terminal escalation, say).
        store
            .append(
                STREAM,
                ExpectedRevision::Any,
                &[ev(
                    "UnitStatus",
                    r#"{"id":"plan-critique","status":"escalated"}"#,
                )],
            )
            .unwrap();

        let fresh = start_fresh(&store, &["crit".to_string()], "").unwrap();
        assert_ne!(
            first, fresh,
            "start_fresh mints a distinct run even though the criteria are identical"
        );

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        assert_eq!(
            events
                .iter()
                .filter(|e| e.type_ == TYPE_RUN_STARTED)
                .count(),
            2,
            "--fresh appended a second RunStarted rather than adopting the wedged run"
        );
        // The current slice is the fresh boundary onward - the prior run's escalated residue
        // sits BEFORE it and can never seed live work, so the gate runs anew.
        let slice = current_run(&events);
        assert_eq!(current_run_id(&events).as_deref(), Some(fresh.as_str()));
        assert!(
            !slice
                .iter()
                .any(|e| String::from_utf8_lossy(&e.data).contains("escalated")),
            "the prior run's terminal residue is excluded from the fresh run's slice"
        );
        // A subsequent ensure_started (as the conductor calls internally) ADOPTS the fresh
        // boundary - so `rigger run --fresh` drives the clean run it just began.
        let adopted = ensure_started(&store, &["crit".to_string()]).unwrap();
        assert_eq!(
            adopted, fresh,
            "the conductor adopts the freshly-started run"
        );
    }

    #[test]
    fn ensure_started_mints_a_new_run_when_the_criteria_change() {
        // A new campaign (different acceptance criteria) begins a fresh run, so the prior
        // campaign's residue is left behind the new boundary.
        let store = Store::open(":memory:").unwrap();
        let first = ensure_started(&store, &["old".to_string()]).unwrap();
        let second = ensure_started(&store, &["new".to_string()]).unwrap();
        assert_ne!(first, second, "changed criteria begin a distinct run");

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        assert_eq!(
            events
                .iter()
                .filter(|e| e.type_ == TYPE_RUN_STARTED)
                .count(),
            2,
            "the new campaign appended its own RunStarted"
        );
        assert_eq!(current_run_id(&events).as_deref(), Some(second.as_str()));
    }

    #[test]
    fn ensure_started_over_legacy_events_leaves_them_before_the_boundary() {
        // The Gap 11 case: a store holding stale non-terminal units of an aborted
        // pre-run-id run. The first run begins here; the stale units are before the
        // RunStarted, so the current run's slice never contains them.
        let store = Store::open(":memory:").unwrap();
        store
            .append(
                STREAM,
                ExpectedRevision::Any,
                &[
                    ev("UnitStarted", r#"{"id":"u-zombie"}"#),
                    ev(
                        "SpawnRequested",
                        r#"{"id":"u-zombie/implementer#0","unit":"u-zombie"}"#,
                    ),
                ],
            )
            .unwrap();

        ensure_started(&store, &["crit".to_string()]).unwrap();

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        let slice = current_run(&events);
        assert_eq!(slice.len(), 1, "only the RunStarted is in the fresh run");
        assert_eq!(slice[0].type_, TYPE_RUN_STARTED);
        assert!(
            !slice
                .iter()
                .any(|e| String::from_utf8_lossy(&e.data).contains("zombie")),
            "the aborted prior run's zombies are before the boundary and never live"
        );
    }

    // --- Definition pinning (spec 13, unit 1) ---

    #[test]
    fn a_fresh_run_pins_its_definition_hash_at_start() {
        // A new campaign pins the current definition hash on its RunStarted - the anchor a
        // later live-run step re-checks. This is the "a run pins its definition hash at
        // start" done-when, and the fresh-run-is-free path (no prior pin, no halt).
        let store = Store::open(":memory:").unwrap();
        let out = ensure_started_pinned(&store, &["crit".to_string()], "hash-A", false).unwrap();
        assert!(
            matches!(out, RunStart::Ready(_)),
            "a fresh run is always free"
        );

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        let started = latest(&events).unwrap();
        assert_eq!(
            started.definition, "hash-A",
            "the RunStarted pins the current definition hash"
        );
        assert_eq!(effective_definition(current_run(&events)), "hash-A");
    }

    #[test]
    fn adopting_a_run_whose_definition_is_unchanged_is_free() {
        // A plain step over an unchanged definition adopts the run and appends nothing - the
        // steady-state resume, unaffected by pinning.
        let store = Store::open(":memory:").unwrap();
        ensure_started_pinned(&store, &["crit".to_string()], "hash-A", false).unwrap();
        let out = ensure_started_pinned(&store, &["crit".to_string()], "hash-A", false).unwrap();
        assert!(
            matches!(out, RunStart::Ready(_)),
            "an unchanged definition adopts, does not drift"
        );
        assert_eq!(
            store
                .read_stream(STREAM, 0, Direction::Forward)
                .unwrap()
                .iter()
                .filter(|e| e.type_ == TYPE_RUN_STARTED)
                .count(),
            1,
            "adopting appends no second RunStarted"
        );
    }

    #[test]
    fn a_live_run_under_a_drifted_definition_reports_drift_and_appends_nothing() {
        // The sharpest exposure spec 13 names: a mid-campaign definition edit. A live-run
        // step whose on-disk hash differs from the pinned hash is RunStart::Drifted (the CLI
        // then HALTS loudly), and - crucially - drift is a pure READ: nothing is appended, so
        // re-running the drifted step re-surfaces the same halt every time until it is resolved.
        let store = Store::open(":memory:").unwrap();
        ensure_started_pinned(&store, &["crit".to_string()], "hash-A", false).unwrap();
        let before = store
            .read_stream(STREAM, 0, Direction::Forward)
            .unwrap()
            .len();

        let out = ensure_started_pinned(&store, &["crit".to_string()], "hash-B", false).unwrap();
        match out {
            RunStart::Drifted {
                pinned, current, ..
            } => {
                assert_eq!(pinned, "hash-A", "drift names the pinned hash");
                assert_eq!(current, "hash-B", "drift names the on-disk hash");
            }
            other => panic!("expected Drifted, got {other:?}"),
        }
        assert_eq!(
            store
                .read_stream(STREAM, 0, Direction::Forward)
                .unwrap()
                .len(),
            before,
            "a drift halt appends nothing - it re-surfaces on every step until resolved"
        );
    }

    #[test]
    fn rebase_definition_records_the_supersession_and_subsequent_steps_are_free() {
        // `--rebase-definition` records the supersession (old -> new) and continues; a plain
        // step AFTER the rebase sees the effective pin advanced to the new hash and no longer
        // halts. This is the "records the supersession and continues" done-when.
        let store = Store::open(":memory:").unwrap();
        ensure_started_pinned(&store, &["crit".to_string()], "hash-A", false).unwrap();

        let out = ensure_started_pinned(&store, &["crit".to_string()], "hash-B", true).unwrap();
        match out {
            RunStart::Rebased {
                pinned, current, ..
            } => {
                assert_eq!((pinned.as_str(), current.as_str()), ("hash-A", "hash-B"));
            }
            other => panic!("expected Rebased, got {other:?}"),
        }

        // The supersession is recorded on the log (old and new hashes both legible) without a
        // second RunStarted moving the run boundary.
        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        let rebase = events
            .iter()
            .find(|e| e.type_ == TYPE_DECISION_MADE)
            .expect("the rebase is recorded as a DecisionMade (no new event type)");
        assert_eq!(
            rebase.meta.get(META_DEFINITION).map(String::as_str),
            Some("hash-B")
        );
        assert_eq!(
            rebase.meta.get(META_DEFINITION_PRIOR).map(String::as_str),
            Some("hash-A")
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| e.type_ == TYPE_RUN_STARTED)
                .count(),
            1,
            "a rebase does NOT append a second RunStarted (the run boundary is unchanged)"
        );
        assert_eq!(
            effective_definition(current_run(&events)),
            "hash-B",
            "the effective pin advances to the rebased-to hash"
        );

        // A plain step on the (now new) definition is free - the rebase is not re-litigated.
        let after = ensure_started_pinned(&store, &["crit".to_string()], "hash-B", false).unwrap();
        assert!(
            matches!(after, RunStart::Ready(_)),
            "after a rebase, a plain step on the new definition no longer drifts"
        );
    }

    #[test]
    fn an_unpinned_definition_never_drifts() {
        // The back-compat guard: a run started with no pin (empty definition - a legacy run,
        // or the conductor's own unpinned `ensure_started`), and a caller passing no
        // definition (pinning disabled), both take the free path unconditionally.
        let store = Store::open(":memory:").unwrap();
        // A legacy/unpinned run start.
        ensure_started_pinned(&store, &["crit".to_string()], "", false).unwrap();
        // A pinned caller against an unpinned run: free (the run pinned nothing to drift from).
        assert!(matches!(
            ensure_started_pinned(&store, &["crit".to_string()], "hash-Z", false).unwrap(),
            RunStart::Ready(_)
        ));
        // A pin exists but the caller passes no definition (pinning disabled): free.
        let store2 = Store::open(":memory:").unwrap();
        ensure_started_pinned(&store2, &["crit".to_string()], "hash-A", false).unwrap();
        assert!(matches!(
            ensure_started_pinned(&store2, &["crit".to_string()], "", false).unwrap(),
            RunStart::Ready(_)
        ));
    }

    #[test]
    fn ensure_started_is_the_unpinned_convenience_and_still_adopts() {
        // The 2-arg `ensure_started` the conductor calls is unpinned: it delegates to
        // `ensure_started_pinned` with an empty definition, so it never drifts and adopts a
        // same-criteria run exactly as before pinning existed.
        let store = Store::open(":memory:").unwrap();
        let first = ensure_started(&store, &["crit".to_string()]).unwrap();
        let again = ensure_started(&store, &["crit".to_string()]).unwrap();
        assert_eq!(first, again, "unpinned ensure_started adopts the same run");
    }

    #[test]
    fn effective_definition_folds_the_last_rebase_over_the_pinned_start() {
        // The fold rule: the current pin is the RunStarted's definition advanced by the LAST
        // rebase record in the slice - so a run rebased A->B->C is effectively pinned at C.
        let store = Store::open(":memory:").unwrap();
        let run = start_fresh(&store, &["crit".to_string()], "hash-A").unwrap();
        record_rebase(&store, &run, "hash-A", "hash-B").unwrap();
        record_rebase(&store, &run, "hash-B", "hash-C").unwrap();
        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        assert_eq!(effective_definition(current_run(&events)), "hash-C");
    }

    #[test]
    fn a_pin_is_scoped_to_its_run_a_fresh_boundary_repins_free() {
        // The new-campaign / --fresh path: a fresh boundary over a DIFFERENT definition is
        // always free (it pins the current hash, drifting against no prior run), and the drift
        // check reads only the CURRENT run's pin - a prior run's pin never leaks across.
        let store = Store::open(":memory:").unwrap();
        // Run 1 pins hash-A.
        ensure_started_pinned(&store, &["crit".to_string()], "hash-A", false).unwrap();
        // A NEW campaign (different criteria) begins its own fresh run pinning the current def.
        let out = ensure_started_pinned(&store, &["other".to_string()], "hash-B", false).unwrap();
        assert!(
            matches!(out, RunStart::Ready(_)),
            "a fresh boundary for a new campaign is free even against a different definition"
        );
        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        assert_eq!(
            events
                .iter()
                .filter(|e| e.type_ == TYPE_RUN_STARTED)
                .count(),
            2,
            "the new campaign appended its own pinned RunStarted"
        );
        assert_eq!(
            effective_definition(current_run(&events)),
            "hash-B",
            "the current run's pin is the fresh boundary's, not run 1's"
        );
    }
}
