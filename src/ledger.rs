//! The conductor's durable run state, projected from the event log: rebuildable
//! by replay, so a crashed or resumed run continues from the truth rather than
//! from conversation. Unknown event types are ignored, so the same log feeds both
//! this projection and the context graph.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::eventstore::Event;

/// Status of a unit of work, over its lifecycle
/// pending -> grounding -> red -> green -> verified -> reviewed -> integrated,
/// or the terminal failed / escalated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Pending,
    Grounding,
    Red,
    Green,
    Verified,
    Reviewed,
    Integrated,
    Failed,
    Escalated,
}

impl Status {
    pub fn parse(s: &str) -> Option<Status> {
        Some(match s {
            "pending" => Status::Pending,
            "grounding" => Status::Grounding,
            "red" => Status::Red,
            "green" => Status::Green,
            "verified" => Status::Verified,
            "reviewed" => Status::Reviewed,
            "integrated" => Status::Integrated,
            "failed" => Status::Failed,
            "escalated" => Status::Escalated,
            _ => return None,
        })
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Status::Pending => "pending",
            Status::Grounding => "grounding",
            Status::Red => "red",
            Status::Green => "green",
            Status::Verified => "verified",
            Status::Reviewed => "reviewed",
            Status::Integrated => "integrated",
            Status::Failed => "failed",
            Status::Escalated => "escalated",
        }
    }
}

/// Unit is one unit of work in the run.
#[derive(Clone, Debug)]
pub struct Unit {
    pub id: String,
    pub spec_criterion: String,
    pub depends_on: Vec<String>,
    pub status: Status,
    pub worktree: String,
    pub branch: String,
    /// red / green / verify / review summaries.
    pub evidence: BTreeMap<String, String>,
    pub attempts: u32,
    pub commit: String,
}

/// RunState is the projected run state.
#[derive(Default)]
pub struct RunState {
    pub units: BTreeMap<String, Unit>,
    /// Whether the run flagged a spec defect (an uncovered criterion, §4.4). Folded
    /// from the conductor's SpecDefect event; gates `fully_done`.
    pub spec_defect: bool,
    /// Whether a deferred gate failed at the run's phase boundary. Folded from the
    /// conductor's DeferredGateFailed event; gates both `done` and `fully_done` so a
    /// deferred failure can never be reported as a finished run.
    pub deferred_gate_failed: bool,
}

// Run-event types the conductor emits (folded here into run state).
pub const TYPE_UNIT_STARTED: &str = "UnitStarted";
pub const TYPE_UNIT_STATUS: &str = "UnitStatus";
pub const TYPE_UNIT_FAILED: &str = "UnitFailed";
pub const TYPE_UNIT_ESCALATED: &str = "UnitEscalated";
pub const TYPE_UNIT_INTEGRATED: &str = "UnitIntegrated";
/// The conductor's SpecDefect event (kept in sync with `conductor::TYPE_SPEC_DEFECT`):
/// an uncovered criterion the run flagged rather than deviating around (§4.4).
pub const TYPE_SPEC_DEFECT: &str = "SpecDefect";
/// The conductor's DeferredGateFailed event (kept in sync with
/// `conductor::TYPE_DEFERRED_GATE_FAILED`): a deferred gate that failed when it ran
/// at the run's phase boundary. A deferred failure is surfaced truthfully - it gates
/// both `done` and `fully_done` so the run never reports finished with a red
/// phase-boundary gate.
pub const TYPE_DEFERRED_GATE_FAILED: &str = "DeferredGateFailed";

#[derive(Deserialize)]
struct UnitStarted {
    id: String,
    #[serde(default)]
    spec_criterion: String,
    #[serde(default)]
    needs: Vec<String>,
    #[serde(default)]
    worktree: String,
    #[serde(default)]
    branch: String,
}
#[derive(Deserialize)]
struct UnitStatus {
    id: String,
    status: String,
    #[serde(default)]
    evidence: BTreeMap<String, String>,
}
#[derive(Deserialize)]
struct UnitFailed {
    id: String,
    #[serde(default)]
    attempts: u32,
}
#[derive(Deserialize)]
struct UnitEscalated {
    id: String,
}
#[derive(Deserialize)]
struct UnitIntegrated {
    id: String,
    #[serde(default)]
    commit: String,
}

impl RunState {
    pub fn new() -> Self {
        RunState::default()
    }

    fn unit(&mut self, id: &str) -> &mut Unit {
        self.units.entry(id.to_string()).or_insert_with(|| Unit {
            id: id.to_string(),
            spec_criterion: String::new(),
            depends_on: Vec::new(),
            status: Status::Pending,
            worktree: String::new(),
            branch: String::new(),
            evidence: BTreeMap::new(),
            attempts: 0,
            commit: String::new(),
        })
    }

    /// Fold one run event into the state.
    pub fn apply(&mut self, e: &Event) -> Result<(), serde_json::Error> {
        match e.type_.as_str() {
            TYPE_UNIT_STARTED => {
                let p: UnitStarted = serde_json::from_slice(&e.data)?;
                let u = self.unit(&p.id);
                u.spec_criterion = p.spec_criterion;
                u.depends_on = p.needs;
                u.worktree = p.worktree;
                u.branch = p.branch;
                u.status = Status::Grounding;
            }
            TYPE_UNIT_STATUS => {
                let p: UnitStatus = serde_json::from_slice(&e.data)?;
                let u = self.unit(&p.id);
                if let Some(s) = Status::parse(&p.status) {
                    u.status = s;
                }
                u.evidence.extend(p.evidence);
            }
            TYPE_UNIT_FAILED => {
                let p: UnitFailed = serde_json::from_slice(&e.data)?;
                let u = self.unit(&p.id);
                u.status = Status::Failed;
                u.attempts = p.attempts;
            }
            TYPE_UNIT_ESCALATED => {
                let p: UnitEscalated = serde_json::from_slice(&e.data)?;
                self.unit(&p.id).status = Status::Escalated;
            }
            TYPE_UNIT_INTEGRATED => {
                let p: UnitIntegrated = serde_json::from_slice(&e.data)?;
                let u = self.unit(&p.id);
                u.status = Status::Integrated;
                u.commit = p.commit;
            }
            TYPE_SPEC_DEFECT => {
                self.spec_defect = true;
            }
            TYPE_DEFERRED_GATE_FAILED => {
                self.deferred_gate_failed = true;
            }
            _ => {}
        }
        Ok(())
    }

    /// Done reports whether the run is complete: at least one unit, all integrated,
    /// and no deferred gate failed at the phase boundary. (Coverage and inline
    /// gate-green are enforced by the conductor's coverage gate and per-unit gates; a
    /// unit reaches Integrated only after its inline gates pass. A deferred gate runs
    /// ONCE at end-of-run, after every unit integrated, so its failure is folded in
    /// here rather than at any single unit.)
    pub fn done(&self) -> bool {
        !self.deferred_gate_failed
            && !self.units.is_empty()
            && self.units.values().all(|u| u.status == Status::Integrated)
    }

    /// The full "done" predicate (§4.1, R6): every criterion covered + every unit
    /// integrated + every gate green.
    ///
    /// The three conjuncts collapse to two checks here because the conductor already
    /// enforces the others by construction:
    /// - **every gate green** is implied by **every unit integrated**: a unit reaches
    ///   `Integrated` only after `run_gates` returns all-pass (and an adjudicator
    ///   verdict, when present, approves), so all-integrated already means gate-green.
    /// - **every criterion covered** is enforced as a gate at the start of the run
    ///   (and again after planning for a `produces` workflow); a remaining gap halts
    ///   the run with a SpecDefect rather than reaching here. So the live witness that
    ///   coverage held is the *absence* of a flagged spec defect.
    ///
    /// Hence: when there are criteria to satisfy, the run is fully done iff no spec
    /// defect was flagged and every unit integrated. With no criteria there is nothing
    /// to converge against, so this defers to the plain `done` predicate.
    pub fn fully_done(&self, criteria: &[String]) -> bool {
        if self.spec_defect || self.deferred_gate_failed {
            return false;
        }
        if criteria.is_empty() {
            return self.done();
        }
        !self.units.is_empty() && self.units.values().all(|u| u.status == Status::Integrated)
    }

    /// Whether a unit has reached a terminal state (integrated, failed, escalated).
    pub fn is_terminal(&self, id: &str) -> bool {
        matches!(
            self.units.get(id).map(|u| u.status),
            Some(Status::Integrated) | Some(Status::Failed) | Some(Status::Escalated)
        )
    }

    /// Whether a unit has been integrated (used by resume to skip completed work).
    pub fn is_integrated(&self, id: &str) -> bool {
        matches!(
            self.units.get(id).map(|u| u.status),
            Some(Status::Integrated)
        )
    }
}

/// Project rebuilds run state from an ordered slice of events.
pub fn project(events: &[Event]) -> Result<RunState, serde_json::Error> {
    let mut r = RunState::new();
    for e in events {
        r.apply(e)?;
    }
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(type_: &str, data: &str) -> Event {
        Event::new(type_, data.as_bytes().to_vec())
    }

    #[test]
    fn projects_unit_lifecycle() {
        let events = vec![
            ev(
                TYPE_UNIT_STARTED,
                r#"{"id":"u","needs":["x"],"worktree":"/wt","branch":"b"}"#,
            ),
            ev(
                TYPE_UNIT_STATUS,
                r#"{"id":"u","status":"green","evidence":{"green":"54 passed"}}"#,
            ),
            ev(TYPE_UNIT_INTEGRATED, r#"{"id":"u","commit":"abc"}"#),
        ];
        let r = project(&events).unwrap();
        assert_eq!(r.units["u"].status, Status::Integrated);
        assert_eq!(r.units["u"].commit, "abc");
        assert_eq!(r.units["u"].depends_on, ["x"]);
        assert_eq!(r.units["u"].branch, "b");
        assert_eq!(
            r.units["u"].evidence.get("green").map(String::as_str),
            Some("54 passed")
        );
        assert!(r.done());
        assert!(r.is_integrated("u"));
    }

    #[test]
    fn folds_intermediate_lifecycle_states() {
        let events = vec![
            ev(TYPE_UNIT_STARTED, r#"{"id":"u"}"#),
            ev(TYPE_UNIT_STATUS, r#"{"id":"u","status":"red"}"#),
            ev(TYPE_UNIT_STATUS, r#"{"id":"u","status":"verified"}"#),
        ];
        let r = project(&events).unwrap();
        assert_eq!(r.units["u"].status, Status::Verified);
        assert!(!r.done());
        assert!(!r.is_terminal("u"));
    }

    #[test]
    fn not_done_with_an_escalated_unit() {
        let r = project(&[ev(TYPE_UNIT_ESCALATED, r#"{"id":"u"}"#)]).unwrap();
        assert_eq!(r.units["u"].status, Status::Escalated);
        assert!(!r.done());
        assert!(r.is_terminal("u"));
    }

    #[test]
    fn fully_done_holds_for_a_clean_run_and_fails_on_escalation() {
        let criteria = vec!["crit".into()];

        // Clean run: every unit integrated, no spec defect -> fully done (§4.1, R6).
        let clean = project(&[
            ev(TYPE_UNIT_STARTED, r#"{"id":"u","spec_criterion":"crit"}"#),
            ev(TYPE_UNIT_INTEGRATED, r#"{"id":"u","commit":"abc"}"#),
        ])
        .unwrap();
        assert!(clean.fully_done(&criteria));

        // An escalated unit is not integrated -> not fully done.
        let escalated = project(&[
            ev(TYPE_UNIT_STARTED, r#"{"id":"u","spec_criterion":"crit"}"#),
            ev(TYPE_UNIT_ESCALATED, r#"{"id":"u"}"#),
        ])
        .unwrap();
        assert!(!escalated.fully_done(&criteria));

        // A flagged spec defect gates fully_done even if every unit integrated.
        let defect = project(&[
            ev(TYPE_UNIT_STARTED, r#"{"id":"u","spec_criterion":"crit"}"#),
            ev(TYPE_UNIT_INTEGRATED, r#"{"id":"u","commit":"abc"}"#),
            ev(TYPE_SPEC_DEFECT, r#"{"reason":"gap"}"#),
        ])
        .unwrap();
        assert!(defect.spec_defect);
        assert!(!defect.fully_done(&criteria));
    }

    #[test]
    fn a_failing_deferred_gate_gates_done_and_fully_done() {
        // A deferred gate that failed at the phase boundary must gate BOTH `done` and
        // `fully_done`, even when every unit integrated - a deferred failure is never
        // reported as a finished run.
        let criteria = vec!["crit".into()];
        let r = project(&[
            ev(TYPE_UNIT_STARTED, r#"{"id":"u","spec_criterion":"crit"}"#),
            ev(TYPE_UNIT_INTEGRATED, r#"{"id":"u","commit":"abc"}"#),
            ev(TYPE_DEFERRED_GATE_FAILED, r#"{"gate":"itest"}"#),
        ])
        .unwrap();
        assert!(r.deferred_gate_failed);
        // Every unit integrated, yet the run is not done because a deferred gate failed.
        assert!(r.units.values().all(|u| u.status == Status::Integrated));
        assert!(!r.done(), "a failing deferred gate must gate `done`");
        assert!(
            !r.fully_done(&criteria),
            "a failing deferred gate must gate `fully_done` with criteria"
        );
        assert!(
            !r.fully_done(&[]),
            "a failing deferred gate must gate `fully_done` with no criteria"
        );
    }
}
