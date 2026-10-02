//! The conductor's rails: a budget breaker and bounded remediation that
//! escalates rather than spinning. Every failure escalates or is bounded-retried,
//! never silently dropped and never infinite. Pure functions the conductor calls.

/// The default remediation bound before escalation.
pub const MAX_RETRIES: u32 = 3;

/// BudgetExhausted reports whether a token budget is spent. A non-positive
/// threshold means no limit.
pub fn budget_exhausted(threshold: i64, spent: i64) -> bool {
    threshold > 0 && spent >= threshold
}

/// Decision after a failed unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Retry,
    Escalate,
}

/// Remediation is the outcome of remediating a failed unit.
#[derive(Clone, Copy, Debug)]
pub struct Remediation {
    pub decision: Decision,
    pub attempts: u32,
}

/// Remediate advances a failed unit's attempt count: bounded retries, then
/// escalation to a human. `prior_attempts` is how many times it was already
/// tried; `max` is the bound (use MAX_RETRIES by default).
pub fn remediate(prior_attempts: u32, max: u32) -> Remediation {
    let attempts = prior_attempts + 1;
    let decision = if attempts >= max {
        Decision::Escalate
    } else {
        Decision::Retry
    };
    Remediation { decision, attempts }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_then_escalates() {
        assert_eq!(remediate(0, MAX_RETRIES).decision, Decision::Retry);
        assert_eq!(remediate(1, MAX_RETRIES).decision, Decision::Retry);
        assert_eq!(remediate(2, MAX_RETRIES).decision, Decision::Escalate);
    }

    #[test]
    fn budget() {
        assert!(budget_exhausted(100, 100));
        assert!(!budget_exhausted(100, 99));
        assert!(!budget_exhausted(0, 1_000_000)); // no limit
    }
}
