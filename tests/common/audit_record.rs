//! Shared support for the suites that check a committed `docs/audit/*.json` record against the
//! real tree (`tests/compiler_pass_stage1_audit.rs`, `tests/core_lane_purity_audit.rs`).
//! Included by each suite through `#[path]`.

/// The committed audit record at repo-relative `path`, parsed - never a general-purpose JSON
/// library substitute, `serde_json::Value` already is that.
pub fn read_audit_record(path: &str) -> serde_json::Value {
    let raw = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{path} must exist and be readable: {e}"));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("{path} must be valid JSON: {e}"))
}
