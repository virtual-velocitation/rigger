//! Plan-critique fixtures: the adjudicator's reject line and the records a spec-defect stop
//! appends (spec 112, criterion 5).

use rigger::conductor::{META_REPLAY_KEY, TYPE_SPEC_DEFECT};
use rigger::contextgraph::TYPE_LESSON_LEARNED;
use rigger::eventstore::Event;
use rigger::ledger::TYPE_UNIT_ESCALATED;

/// A plan-critique adjudicator's reject line carrying `cause` and upholding `upheld`.
pub fn critique_reject(cause: &str, upheld: &[&str]) -> String {
    serde_json::json!({"verdict": "reject", "upheld": upheld, "discarded": [], "cause": cause})
        .to_string()
}

/// The `(type, replay key)` of every plan-critique spec-defect stop record in `events`, in log
/// order.
pub fn stop_records(events: &[Event]) -> Vec<(String, String)> {
    events
        .iter()
        .filter_map(|e| {
            let key = e.meta.get(META_REPLAY_KEY)?;
            key.contains("/spec-defect")
                .then(|| (e.type_.clone(), key.clone()))
        })
        .collect()
}

/// The three records the gate `plan-critique` appends when it stops on its reject at attempt 1,
/// each once, in order: the lesson, the `SpecDefect` and the escalation, each under its key.
pub fn the_stop_records() -> Vec<(String, String)> {
    [
        (TYPE_LESSON_LEARNED, "plan-critique/spec-defect-lesson#1"),
        (TYPE_SPEC_DEFECT, "plan-critique/spec-defect#1"),
        (TYPE_UNIT_ESCALATED, "plan-critique/spec-defect-escalated#1"),
    ]
    .into_iter()
    .map(|(t, k)| (t.to_string(), k.to_string()))
    .collect()
}

/// The index in `events` of the event recorded under replay `key`.
pub fn keyed_index(events: &[Event], key: &str) -> usize {
    events
        .iter()
        .position(|e| e.meta.get(META_REPLAY_KEY).map(String::as_str) == Some(key))
        .unwrap_or_else(|| panic!("no event under {key}"))
}

/// The payload of the event recorded under replay `key` in `events`.
pub fn keyed_payload(events: &[Event], key: &str) -> serde_json::Value {
    serde_json::from_slice(&events[keyed_index(events, key)].data).unwrap()
}
