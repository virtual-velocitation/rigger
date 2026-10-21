//! Spawn fixtures.

use rigger::spawn::{spawn_id, SpawnRequest};

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

/// The environment variable naming the directory the checked-in
/// `tests/fixtures/prompt-capture-agent.sh` copies what it was handed into.
pub const PROMPT_CAPTURE_DIR: &str = "PROMPT_CAPTURE_DIR";

/// The system prompt the prompt-capture agent copied into `capture`: the text of the file its
/// `--system-prompt-file` argument named.
pub fn captured_system_prompt(capture: &std::path::Path) -> String {
    std::fs::read_to_string(capture.join("system-prompt")).unwrap()
}
