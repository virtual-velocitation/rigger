//! Conductor-port fixtures.

use rigger::conductor::Error;

/// A no-op emit sink for an `AgentDriver::spawn` whose emits the test does not observe.
pub fn no_emit(_: &str, _: serde_json::Value) -> Result<(), Error> {
    Ok(())
}
