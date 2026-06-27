package conductor_test

import (
	"context"
	"path/filepath"
	"testing"

	"github.com/virtual-velocitation/rigger/conductor"
	"github.com/virtual-velocitation/rigger/config"
	"github.com/virtual-velocitation/rigger/eventstore/sqlite"
	"github.com/virtual-velocitation/rigger/gate"
	"github.com/virtual-velocitation/rigger/ledger"
)

type stubDriver struct{ spawns int }

func (d *stubDriver) Spawn(_ context.Context, _ config.AgentDef, _ string) (conductor.AgentResult, error) {
	d.spawns++
	return conductor.AgentResult{}, nil
}

func newStore(t *testing.T) *sqlite.Store {
	t.Helper()
	s, err := sqlite.Open(filepath.Join(t.TempDir(), "events.db"))
	if err != nil {
		t.Fatalf("open store: %v", err)
	}
	t.Cleanup(func() { _ = s.Close() })
	return s
}

func TestConductorIntegratesStagesInOrder(t *testing.T) {
	cfg := &config.Config{
		Agents: map[string]config.AgentDef{"planner": {ID: "planner"}, "impl": {ID: "impl"}},
		Workflow: config.Workflow{
			Gates: map[string]config.Gate{"ok": {Run: "true", Kind: "core"}},
			Stages: map[string]config.Stage{
				"plan":      {Name: "plan", Agent: "planner"},
				"implement": {Name: "implement", Agent: "impl", Needs: []string{"plan"}, Gates: []string{"ok"}},
			},
		},
	}
	driver := &stubDriver{}
	rs, err := conductor.Run(context.Background(), cfg, conductor.Deps{Store: newStore(t), Driver: driver, Gates: gate.ExecRunner{}})
	if err != nil {
		t.Fatalf("Run: %v", err)
	}
	if rs.Units["plan"].Status != ledger.Integrated || rs.Units["implement"].Status != ledger.Integrated {
		t.Errorf("both stages should integrate: plan=%+v implement=%+v", rs.Units["plan"], rs.Units["implement"])
	}
	if !rs.Done() {
		t.Error("the run should be done")
	}
	if driver.spawns != 2 {
		t.Errorf("expected one agent spawn per stage (2), got %d", driver.spawns)
	}
}

func TestConductorEscalatesOnPersistentGateFailure(t *testing.T) {
	cfg := &config.Config{
		Agents: map[string]config.AgentDef{"impl": {ID: "impl"}},
		Workflow: config.Workflow{
			Gates: map[string]config.Gate{"bad": {Run: "false", Kind: "core"}},
			Stages: map[string]config.Stage{
				"s": {Name: "s", Agent: "impl", Gates: []string{"bad"}},
			},
		},
	}
	driver := &stubDriver{}
	rs, err := conductor.Run(context.Background(), cfg, conductor.Deps{Store: newStore(t), Driver: driver, Gates: gate.ExecRunner{}})
	if err != nil {
		t.Fatalf("Run: %v", err)
	}
	if rs.Units["s"].Status != ledger.Escalated {
		t.Errorf("a persistently failing gate should escalate; got %+v", rs.Units["s"])
	}
	if rs.Done() {
		t.Error("a run with an escalated unit is not done")
	}
	if driver.spawns != 3 {
		t.Errorf("expected the bounded retries (3 attempts), got %d spawns", driver.spawns)
	}
}
