package gate_test

import (
	"context"
	"strings"
	"testing"

	"github.com/virtual-velocitation/rigger/gate"
)

func TestDecide(t *testing.T) {
	for autonomy, want := range map[gate.Autonomy]string{
		gate.Silent:     gate.ActionRunSilent,
		gate.AutoNotify: gate.ActionRunNotify,
		gate.Manual:     gate.ActionPause,
	} {
		if got := gate.Decide(gate.Gate{Autonomy: autonomy}); got != want {
			t.Errorf("Decide(%s) = %s, want %s", autonomy, got, want)
		}
	}
}

func TestRatchet(t *testing.T) {
	threePasses := []gate.HistoryEntry{{Pass: true}, {Pass: true}, {Pass: true}}

	if !gate.ProposePromotion(gate.Gate{Autonomy: gate.Manual, History: threePasses}) {
		t.Error("three clean passes should propose a promotion")
	}
	if gate.ProposePromotion(gate.Gate{Autonomy: gate.Silent, History: threePasses}) {
		t.Error("a Silent gate should not propose further promotion")
	}
	if gate.ProposePromotion(gate.Gate{Autonomy: gate.Manual, History: []gate.HistoryEntry{{Pass: true}, {Pass: false}, {Pass: true}}}) {
		t.Error("a recent failure should block promotion")
	}

	if gate.NextAutonomy(gate.Manual) != gate.AutoNotify || gate.NextAutonomy(gate.AutoNotify) != gate.Silent {
		t.Error("ratchet should climb manual -> auto_notify -> silent")
	}

	if a, demoted := gate.AutoDemote(gate.Gate{Autonomy: gate.Silent}, false); !demoted || a != gate.Manual {
		t.Errorf("a failing graduated gate must demote to manual; got %s demoted=%v", a, demoted)
	}
	if _, demoted := gate.AutoDemote(gate.Gate{Autonomy: gate.Silent}, true); demoted {
		t.Error("a passing gate must not demote")
	}
}

func TestActive(t *testing.T) {
	deferred := gate.Gate{Kind: gate.KindDeferred}
	if gate.Active(deferred, "u1", nil) {
		t.Error("a deferred gate is inert until its creating unit integrates")
	}
	if !gate.Active(deferred, "u1", []string{"u0", "u1"}) {
		t.Error("a deferred gate becomes active once its unit integrates")
	}
	if !gate.Active(gate.Gate{Kind: gate.KindCore}, "", nil) {
		t.Error("a core gate is always active")
	}
}

func TestExecRunner(t *testing.T) {
	r := gate.ExecRunner{}
	if got := r.Run(context.Background(), gate.Gate{Run: "true"}); !got.Pass || !strings.HasPrefix(got.Evidence, "PASS") {
		t.Errorf("passing gate: %+v", got)
	}
	got := r.Run(context.Background(), gate.Gate{Run: "echo 'boom: an error happened'; false"})
	if got.Pass {
		t.Error("a non-zero exit must be a failure")
	}
	if !strings.HasPrefix(got.Evidence, "FAIL") || !strings.Contains(got.Evidence, "error") {
		t.Errorf("failure evidence should be compact and show the error line: %q", got.Evidence)
	}
}
