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
