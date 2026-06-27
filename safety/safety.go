// Package safety holds the conductor's rails: a budget breaker, bounded
// remediation that escalates rather than spinning, and the abort partition.
// Every failure escalates or is bounded-retried, never silently dropped and
// never infinite (architecture R6). These are pure functions the conductor calls.
package safety

// MaxRetries is the default remediation bound before escalation.
const MaxRetries = 3

// BudgetExhausted reports whether a token budget is spent. A non-positive
// threshold means no limit.
func BudgetExhausted(threshold, spent int) bool {
	return threshold > 0 && spent >= threshold
}

// Decision after a failed unit.
const (
	Retry    = "retry"
	Escalate = "escalate"
)

// Remediation is the outcome of remediating a failed unit.
type Remediation struct {
	Decision string // Retry | Escalate
	Attempts int    // total attempts so far (including this one)
}

// Remediate advances a failed unit's attempt count: bounded retries, then
// escalation to a human. priorAttempts is how many times it was already tried;
// max is the bound (use MaxRetries by default).
func Remediate(priorAttempts, max int) Remediation {
	attempts := priorAttempts + 1
	if attempts >= max {
		return Remediation{Decision: Escalate, Attempts: attempts}
	}
	return Remediation{Decision: Retry, Attempts: attempts}
}

// IntegratedID is anything with an id and an integrated flag, used by Abort.
type IntegratedID struct {
	ID         string
	Integrated bool
}

// Abort partitions units when a task is aborted: integrated work is kept,
// everything else (cheap, isolated worktrees) is discarded.
func Abort(units []IntegratedID) (kept, discarded []string) {
	for _, u := range units {
		if u.Integrated {
			kept = append(kept, u.ID)
		} else {
			discarded = append(discarded, u.ID)
		}
	}
	return kept, discarded
}
