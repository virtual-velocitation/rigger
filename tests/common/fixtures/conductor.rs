//! Conductor-port fixtures.

use rigger::conductor::{AgentResult, Error, SpawnOpts};

/// A no-op emit sink for an `AgentDriver::spawn` whose emits the test does not observe.
pub fn no_emit(_: &str, _: serde_json::Value) -> Result<(), Error> {
    Ok(())
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
