package conductor_test

import (
	"context"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync/atomic"
	"testing"

	"github.com/virtual-velocitation/rigger/conductor"
	"github.com/virtual-velocitation/rigger/config"
	"github.com/virtual-velocitation/rigger/contextgraph"
	"github.com/virtual-velocitation/rigger/eventstore"
	"github.com/virtual-velocitation/rigger/eventstore/sqlite"
	"github.com/virtual-velocitation/rigger/gate"
	"github.com/virtual-velocitation/rigger/grounder"
	"github.com/virtual-velocitation/rigger/ledger"
)

type stubDriver struct {
	spawns     atomic.Int64
	writeFile  string   // if set, the "agent" creates this file in its working dir
	emitLines  []string // if set, the "agent" writes these to .rigger/emit.jsonl
	lastPrompt atomic.Value
}

func (d *stubDriver) Spawn(_ context.Context, _ config.AgentDef, prompt string, opts conductor.SpawnOpts) (conductor.AgentResult, error) {
	d.spawns.Add(1)
	d.lastPrompt.Store(prompt)
	if opts.Dir != "" {
		if d.writeFile != "" {
			_ = os.WriteFile(filepath.Join(opts.Dir, d.writeFile), []byte("// generated\n"), 0o644)
		}
		if len(d.emitLines) > 0 {
			_ = os.MkdirAll(filepath.Join(opts.Dir, ".rigger"), 0o755)
			_ = os.WriteFile(filepath.Join(opts.Dir, ".rigger", "emit.jsonl"), []byte(strings.Join(d.emitLines, "\n")+"\n"), 0o644)
		}
	}
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
	if driver.spawns.Load() != 2 {
		t.Errorf("expected one agent spawn per stage (2), got %d", driver.spawns.Load())
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
	if driver.spawns.Load() != 3 {
		t.Errorf("expected the bounded retries (3 attempts), got %d spawns", driver.spawns.Load())
	}
}

func TestConductorRunsIndependentStagesConcurrently(t *testing.T) {
	cfg := &config.Config{
		Agents: map[string]config.AgentDef{"a": {ID: "a"}, "b": {ID: "b"}},
		Workflow: config.Workflow{
			Stages: map[string]config.Stage{
				"alpha": {Name: "alpha", Agent: "a"}, // no deps
				"beta":  {Name: "beta", Agent: "b"},  // no deps - independent of alpha
			},
		},
	}
	driver := &stubDriver{}
	rs, err := conductor.Run(context.Background(), cfg, conductor.Deps{Store: newStore(t), Driver: driver, Gates: gate.ExecRunner{}})
	if err != nil {
		t.Fatalf("Run: %v", err)
	}
	if rs.Units["alpha"].Status != ledger.Integrated || rs.Units["beta"].Status != ledger.Integrated {
		t.Errorf("both independent stages should integrate: %+v", rs.Units)
	}
	if !rs.Done() {
		t.Error("the run should be done")
	}
	if driver.spawns.Load() != 2 {
		t.Errorf("expected 2 spawns, got %d", driver.spawns.Load())
	}
}

func TestConductorIsolatesAgentInWorktreeAndCapturesFiles(t *testing.T) {
	repo := initRepo(t)
	cfg := &config.Config{
		Agents: map[string]config.AgentDef{"impl": {ID: "impl"}},
		Workflow: config.Workflow{
			// The gate passes only because it runs inside the worktree, where the
			// agent created generated.go.
			Gates: map[string]config.Gate{"in-wt": {Run: "test -f generated.go", Kind: "core"}},
			Stages: map[string]config.Stage{
				"build": {Name: "build", Agent: "impl", Gates: []string{"in-wt"}},
			},
		},
	}
	store := newStore(t)
	driver := &stubDriver{writeFile: "generated.go"}
	rs, err := conductor.Run(context.Background(), cfg, conductor.Deps{
		Store: store, Driver: driver, Gates: gate.ExecRunner{}, Repo: repo,
	})
	if err != nil {
		t.Fatalf("Run: %v", err)
	}
	if rs.Units["build"].Status != ledger.Integrated {
		t.Fatalf("stage should integrate: %+v", rs.Units["build"])
	}

	// The file the agent wrote inside its worktree must surface as a FileTouched
	// event, feeding the context graph.
	events, err := store.ReadAll(context.Background(), 0, eventstore.Forward, eventstore.Filter{})
	if err != nil {
		t.Fatal(err)
	}
	if !hasFileTouched(events, "generated.go") {
		t.Error("expected a FileTouched event for generated.go captured from the worktree")
	}
}

func TestConductorHarvestsAgentDecisions(t *testing.T) {
	repo := initRepo(t)
	cfg := &config.Config{
		Agents: map[string]config.AgentDef{"impl": {ID: "impl"}},
		Workflow: config.Workflow{
			Gates: map[string]config.Gate{"ok": {Run: "true", Kind: "core"}},
			Stages: map[string]config.Stage{
				"build": {Name: "build", Agent: "impl", Gates: []string{"ok"}},
			},
		},
	}
	store := newStore(t)
	driver := &stubDriver{emitLines: []string{
		`{"type":"DecisionMade","data":{"id":"d1","summary":"chose the generic path","governs":["modifier.go"]}}`,
	}}
	if _, err := conductor.Run(context.Background(), cfg, conductor.Deps{Store: store, Driver: driver, Gates: gate.ExecRunner{}, Repo: repo}); err != nil {
		t.Fatalf("Run: %v", err)
	}
	events, err := store.ReadAll(context.Background(), 0, eventstore.Forward, eventstore.Filter{})
	if err != nil {
		t.Fatal(err)
	}
	if !hasDecision(events, "d1") {
		t.Error("the agent's DecisionMade should be harvested into the event store")
	}
}

func TestConductorGroundsAgentPrompt(t *testing.T) {
	root := t.TempDir()
	if err := os.WriteFile(filepath.Join(root, "target.go"), []byte("func ApplyDamage() {}\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	cfg := &config.Config{
		Agents: map[string]config.AgentDef{"impl": {ID: "impl"}},
		Workflow: config.Workflow{
			Stages: map[string]config.Stage{
				"build": {Name: "build", Agent: "impl", Coverage: "applydamage"},
			},
		},
	}
	driver := &stubDriver{}
	if _, err := conductor.Run(context.Background(), cfg, conductor.Deps{
		Store: newStore(t), Driver: driver, Gates: gate.ExecRunner{}, Grounder: grounder.Grep{Root: root},
	}); err != nil {
		t.Fatalf("Run: %v", err)
	}
	prompt, _ := driver.lastPrompt.Load().(string)
	if !strings.Contains(prompt, "target.go") {
		t.Errorf("agent prompt should include the grounded location; got %q", prompt)
	}
	if !strings.Contains(prompt, "emit.jsonl") {
		t.Errorf("agent prompt should include the emit protocol; got %q", prompt)
	}
}

func hasDecision(events []eventstore.Event, id string) bool {
	for _, e := range events {
		if e.Type == contextgraph.TypeDecisionMade && strings.Contains(string(e.Data), id) {
			return true
		}
	}
	return false
}

func hasFileTouched(events []eventstore.Event, path string) bool {
	for _, e := range events {
		if e.Type == contextgraph.TypeFileTouched && strings.Contains(string(e.Data), path) {
			return true
		}
	}
	return false
}

func initRepo(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	for _, args := range [][]string{
		{"init", "-q"},
		{"config", "user.email", "t@example.com"},
		{"config", "user.name", "t"},
		{"commit", "--allow-empty", "-q", "-m", "init"},
	} {
		if out, err := exec.Command("git", append([]string{"-C", dir}, args...)...).CombinedOutput(); err != nil {
			t.Fatalf("git %v: %v: %s", args, err, out)
		}
	}
	return dir
}
