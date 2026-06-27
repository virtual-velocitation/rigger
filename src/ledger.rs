//! The conductor's durable run state, projected from the event log: rebuildable
//! by replay, so a crashed or resumed run continues from the truth rather than
//! from conversation. Unknown event types are ignored, so the same log feeds both
//! this projection and the context graph.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::eventstore::Event;

/// Status of a unit of work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Pending,
    Running,
    Integrated,
    Failed,
    Escalated,
}

/// Unit is one unit of work in the run.
#[derive(Clone, Debug)]
pub struct Unit {
    pub id: String,
    pub spec_criterion: String,
    pub status: Status,
    pub attempts: u32,
    pub commit: String,
}

/// RunState is the projected run state.
#[derive(Default)]
pub struct RunState {
    pub units: BTreeMap<String, Unit>,
}

// Run-event types the conductor emits (folded here into run state).
pub const TYPE_UNIT_STARTED: &str = "UnitStarted";
pub const TYPE_UNIT_FAILED: &str = "UnitFailed";
pub const TYPE_UNIT_ESCALATED: &str = "UnitEscalated";
pub const TYPE_UNIT_INTEGRATED: &str = "UnitIntegrated";

#[derive(Deserialize)]
struct UnitStarted {
    id: String,
    #[serde(default)]
    spec_criterion: String,
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
            status: Status::Pending,
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
                u.status = Status::Running;
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
            _ => {}
        }
        Ok(())
    }

    /// Done reports whether the run is complete: at least one unit, all integrated.
    pub fn done(&self) -> bool {
        !self.units.is_empty() && self.units.values().all(|u| u.status == Status::Integrated)
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
            ev(TYPE_UNIT_STARTED, r#"{"id":"u"}"#),
            ev(TYPE_UNIT_INTEGRATED, r#"{"id":"u","commit":"abc"}"#),
        ];
        let r = project(&events).unwrap();
        assert_eq!(r.units["u"].status, Status::Integrated);
        assert_eq!(r.units["u"].commit, "abc");
        assert!(r.done());
    }

    #[test]
    fn not_done_with_an_escalated_unit() {
        let r = project(&[ev(TYPE_UNIT_ESCALATED, r#"{"id":"u"}"#)]).unwrap();
        assert_eq!(r.units["u"].status, Status::Escalated);
        assert!(!r.done());
    }
}
