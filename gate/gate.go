// Package gate is Rigger's verification discipline: a gate is a command plus a
// trust level, it yields compact evidence (never a raw log), and its autonomy
// moves on a bidirectional ratchet so a graduated gate can never silently
// auto-pass bad work. The Runner is a port; the exec runner is the adapter and
// tests inject a stub.
package gate

import "context"

// Kind classifies a gate's authority lifecycle.
type Kind string

const (
	KindCore     Kind = "core"     // canonical, always active
	KindElevated Kind = "elevated" // project-declared invariant
	KindDeferred Kind = "deferred" // inert until its creating unit integrates
)

// Autonomy is how much a gate is trusted to run unattended.
type Autonomy string

const (
	Manual     Autonomy = "manual"      // pause; a human decides
	AutoNotify Autonomy = "auto_notify" // run, pass silently, log it
	Silent     Autonomy = "silent"      // run invisibly
)

// PromoteThreshold is the number of consecutive clean passes that proposes a
// promotion.
const PromoteThreshold = 3

// Result is a gate's verdict with compact evidence.
type Result struct {
	Pass     bool
	Evidence string
}

// HistoryEntry records one run of a gate for the ratchet's audit trail.
type HistoryEntry struct {
	Pass          bool
	HumanDecision string
}

// Gate is a verification command and its trust.
type Gate struct {
	ID       string
	Run      string // shell command
	Kind     Kind
	Autonomy Autonomy
	History  []HistoryEntry
}

// Runner runs a gate command and returns its compact result.
type Runner interface {
	Run(ctx context.Context, g Gate) Result
}

// Conductor action for a gate, given its autonomy.
const (
	ActionRunSilent = "run_silent"
	ActionRunNotify = "run_notify"
	ActionPause     = "pause"
)

// Decide maps a gate's autonomy to the conductor's action.
func Decide(g Gate) string {
	switch g.Autonomy {
	case Silent:
		return ActionRunSilent
	case AutoNotify:
		return ActionRunNotify
	default:
		return ActionPause
	}
}

// ProposePromotion reports whether a gate has earned a promotion proposal: the
// last PromoteThreshold runs all passed, and it is not already Silent.
func ProposePromotion(g Gate) bool {
	if g.Autonomy == Silent {
		return false
	}
	if len(g.History) < PromoteThreshold {
		return false
	}
	for _, h := range g.History[len(g.History)-PromoteThreshold:] {
		if !h.Pass {
			return false
		}
	}
	return true
}

// NextAutonomy returns the autonomy one notch up the ratchet, capping at Silent.
func NextAutonomy(a Autonomy) Autonomy {
	switch a {
	case Manual:
		return AutoNotify
	default:
		return Silent
	}
}

// AutoDemote drops a non-manual gate to Manual when it fails. It returns the new
// autonomy and whether a demotion happened.
func AutoDemote(g Gate, pass bool) (Autonomy, bool) {
	if !pass && g.Autonomy != Manual {
		return Manual, true
	}
	return g.Autonomy, false
}

// Active reports whether a gate runs now. Deferred gates are inert until their
// creating unit is among integrated.
func Active(g Gate, createdByUnit string, integrated []string) bool {
	if g.Kind != KindDeferred {
		return true
	}
	for _, id := range integrated {
		if id == createdByUnit {
			return true
		}
	}
	return false
}
