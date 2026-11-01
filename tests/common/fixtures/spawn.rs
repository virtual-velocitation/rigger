//! Spawn fixtures.

use rigger::eventstore::Event;
use rigger::spawn::{spawn_id, SpawnEvent, SpawnRequest, SpawnResult, TYPE_SPAWN_RESULT};

/// A minimal request for the crate's unit tests: the deterministic id derived from `unit` +
/// `role` + `attempt` (so it cannot drift from the labels), every optional field empty.
#[cfg(test)]
pub fn test_request(
    unit: &str,
    stage: &str,
    role: &str,
    attempt: u32,
    prompt: &str,
) -> SpawnRequest {
    SpawnRequest {
        id: spawn_id(unit, role, attempt),
        unit: unit.to_string(),
        stage: stage.to_string(),
        prompt: prompt.to_string(),
        ..SpawnRequest::default()
    }
}

/// One row per `SpawnResult` among `events`, in their order: its id, output and error, and
/// whether it is a liveness fault.
pub fn spawn_result_rows(events: &[Event]) -> Vec<(String, String, String, bool)> {
    events
        .iter()
        .filter(|e| e.type_ == TYPE_SPAWN_RESULT)
        .map(|e| {
            let result = SpawnResult::from_event(e).unwrap();
            let fault = result.is_liveness_fault();
            (result.id, result.output, result.error, fault)
        })
        .collect()
}

/// The absolute path of the checked-in fixture agent `script` under the workspace's
/// `tests/fixtures`, walking up from the manifest of the test crate compiling this file: the
/// root package, or a member crate below it.
pub fn fixture_script(script: &str) -> String {
    let relative = std::path::Path::new("tests/fixtures").join(script);
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .map(|dir| dir.join(&relative))
        .find(|path| path.exists())
        .unwrap_or_else(|| panic!("no checked-in fixture {script}"))
        .to_string_lossy()
        .into_owned()
}

/// The environment variable naming the directory the checked-in
/// `tests/fixtures/prompt-capture-agent.sh` copies what it was handed into.
pub const PROMPT_CAPTURE_DIR: &str = "PROMPT_CAPTURE_DIR";

/// The system prompt the prompt-capture agent copied into `capture`: the text of the file its
/// `--system-prompt-file` argument named.
pub fn captured_system_prompt(capture: &std::path::Path) -> String {
    std::fs::read_to_string(capture.join("system-prompt")).unwrap()
}
