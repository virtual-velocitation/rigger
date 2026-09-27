//! Conductor-port fixtures.

use rigger::conductor::{AgentResult, Error, SpawnOpts};
use rigger::eventstore::Event;
use rigger::ledger::TYPE_UNIT_STATUS;

/// A no-op emit sink for an `AgentDriver::spawn` whose emits the test does not observe.
pub fn no_emit(_: &str, _: serde_json::Value) -> Result<(), Error> {
    Ok(())
}

/// The spawn options of an attempt-0 implementer spawn `id` carrying a fixed persona prompt.
pub fn implementer_opts(id: &str) -> SpawnOpts {
    SpawnOpts {
        id: id.to_string(),
        attempt: 0,
        system_prompt: "You implement findings.".to_string(),
        ..Default::default()
    }
}

/// A reviewer double: the adjudicator approves, every other reviewer returns a plain note.
pub fn review_or_adjudicate(opts: &SpawnOpts) -> AgentResult {
    if opts.id.contains("/adjudicator#") {
        return AgentResult {
            output: r#"{"verdict":"approve"}"#.into(),
            resolved_model: String::new(),
        };
    }
    AgentResult {
        output: "reviewed the diff".into(),
        resolved_model: String::new(),
    }
}

/// How many `UnitStatus` markers in `events` carry the `status` token.
pub fn count_status_marker(events: &[Event], status: &str) -> usize {
    events
        .iter()
        .filter(|e| {
            e.type_ == TYPE_UNIT_STATUS
                && String::from_utf8_lossy(&e.data).contains(&format!("\"status\":\"{status}\""))
        })
        .count()
}

/// Whether `events` carries a `UnitStatus` marker whose `status` is `status`.
pub fn has_status_marker(events: &[Event], status: &str) -> bool {
    count_status_marker(events, status) > 0
}
