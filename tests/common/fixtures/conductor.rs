//! Conductor-port fixtures.

use rigger::conductor::{AgentDriver, AgentResult, Deps, Error, SpawnOpts};
use rigger::eventstore::EventStore;
use rigger::gate;

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

/// The conductor's ports for one run over `repo`: no grounder, no graph and no criteria.
pub fn bare_deps<'a>(
    store: &'a dyn EventStore,
    driver: &'a dyn AgentDriver,
    gates: &'a dyn gate::Runner,
    repo: &str,
) -> Deps<'a> {
    Deps {
        store,
        driver,
        gates,
        repo: repo.to_string(),
        grounder: None,
        graph: None,
        criteria: Vec::new(),
    }
}
