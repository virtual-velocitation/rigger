//! Hold-probe fixtures: the progress line a probe of a run hold records. Mounted only where the
//! library has `rigger::progress` - the domain crate and the root suites.

use rigger::progress::AgentProgress;

/// The `hold-probe` line of a probe of the `cause` hold bounded by a delay of `secs` seconds,
/// at the run-stream revision of the probed hold's latest `RunHeld`.
pub fn probe_line(cause: &str, secs: u64, revision: i64) -> AgentProgress {
    AgentProgress {
        id: "hold-probe".to_string(),
        activity: format!("probing the {cause} hold within {secs} s"),
        revision: Some(revision),
    }
}
