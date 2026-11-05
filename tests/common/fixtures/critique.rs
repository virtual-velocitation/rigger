//! Spec-critique fixtures: the critic's canned answers, read by the critique's own unit tests and
//! by the periphery tests that drive `rigger critique` and a new run over a critiqued spec (specs
//! 112 and 123), and the builders of the critique a unit test expects, read by the critique's and
//! the critic's unit tests.

use rigger::review::{CritiqueFinding, CritiquePass};

/// The critic's clean answer: prose, no finding line, an approve.
pub const CLEAN_CRITIQUE: &str = "No defects.\n{\"verdict\":\"approve\"}";

/// The critic's advisory answer: one NON-BLOCKING finding line and an approve.
pub const ADVISORY: &str = "C1 | NON-BLOCKING | Design | renders is vague | name the surface\n\
     {\"verdict\":\"approve\"}";

/// The result a critic answering with no verdict line records: no pass.
pub const NO_VERDICT: &str = "I could not decide.";

/// The finding a test expects, its fields in finding-line order.
pub fn finding(id: &str, blocking: bool, fields: [&str; 4]) -> CritiqueFinding {
    let [critic, location, reading, fix] = fields;
    CritiqueFinding {
        id: id.to_string(),
        blocking,
        critic: critic.to_string(),
        location: location.to_string(),
        reading: reading.to_string(),
        fix: fix.to_string(),
    }
}

/// A rejecting pass at `attempt` whose result sits at log position `position`, holding
/// `findings` as `(id, blocking)` in finding order.
pub fn pass_at(position: u64, attempt: u32, findings: &[(&str, bool)]) -> CritiquePass {
    CritiquePass {
        position,
        attempt,
        verdict: "reject".to_string(),
        findings: findings
            .iter()
            .map(|(id, blocking)| finding(id, *blocking, ["C", "criterion 1", "r", "f"]))
            .collect(),
    }
}
