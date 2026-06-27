package ledger_test

import (
	"encoding/json"
	"testing"

	"github.com/virtual-velocitation/rigger/eventstore"
	"github.com/virtual-velocitation/rigger/ledger"
)

func ev(typ string, payload any) eventstore.Event {
	b, _ := json.Marshal(payload)
	return eventstore.Event{Type: typ, Data: b}
}

func TestRunStateProjection(t *testing.T) {
	r, err := ledger.Project([]eventstore.Event{
		ev(ledger.TypeUnitStarted, ledger.UnitStarted{ID: "u1", SpecCriterion: "c1"}),
		ev(ledger.TypeUnitStarted, ledger.UnitStarted{ID: "u2"}),
		ev(ledger.TypeUnitFailed, ledger.UnitFailed{ID: "u1", Attempts: 1}),
		ev(ledger.TypeUnitIntegrated, ledger.UnitIntegrated{ID: "u1", Commit: "abc123"}),
		ev("SomeGraphEvent", map[string]string{"ignored": "yes"}), // not a run event
	})
	if err != nil {
		t.Fatalf("Project: %v", err)
	}
	u1 := r.Units["u1"]
	if u1.Status != ledger.Integrated || u1.Commit != "abc123" || u1.SpecCriterion != "c1" {
		t.Errorf("u1 projected wrong: %+v", u1)
	}
	if r.Units["u2"].Status != ledger.Running {
		t.Errorf("u2 should be running: %+v", r.Units["u2"])
	}
	if r.Done() {
		t.Error("run is not done: u2 is still running")
	}
}

func TestRunStateDone(t *testing.T) {
	r, _ := ledger.Project([]eventstore.Event{
		ev(ledger.TypeUnitStarted, ledger.UnitStarted{ID: "u1"}),
		ev(ledger.TypeUnitIntegrated, ledger.UnitIntegrated{ID: "u1"}),
	})
	if !r.Done() {
		t.Error("should be done: the only unit integrated")
	}
	if ledger.NewRunState().Done() {
		t.Error("an empty run is not done")
	}
}
