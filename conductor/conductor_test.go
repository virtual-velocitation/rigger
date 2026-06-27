package conductor_test

import (
	"context"
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"github.com/virtual-velocitation/rigger/conductor"
	"github.com/virtual-velocitation/rigger/config"
	"github.com/virtual-velocitation/rigger/contextgraph"
	graphsqlite "github.com/virtual-velocitation/rigger/contextgraph/sqlite"
	"github.com/virtual-velocitation/rigger/eventstore"
	"github.com/virtual-velocitation/rigger/eventstore/sqlite"
	"github.com/virtual-velocitation/rigger/gate"
	"github.com/virtual-velocitation/rigger/grounder"
	"github.com/virtual-velocitation/rigger/ledger"
)

type stubEmit struct {
	typ  string
	data string
}

type stubDriver struct {
	spawns     atomic.Int64
	writeFile  string     // if set, the "agent" creates this file in its working dir
	emits      []stubEmit // if set, the "agent" emits these live during its run
	lastPrompt atomic.Value
}

func (d *stubDriver) Spawn(_ context.Context, _ config.AgentDef, prompt string, opts conductor.SpawnOpts, emit func(string, any) error) (conductor.AgentResult, error) {
	d.spawns.Add(1)
	d.lastPrompt.Store(prompt)
	for _, e := range d.emits {
		_ = emit(e.typ, json.RawMessage(e.data))
	}
	if opts.Dir != "" && d.writeFile != "" {
		_ = os.WriteFile(filepath.Join(opts.Dir, d.writeFile), []byte("// generated\n"), 0o644)
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

func TestConductorLandsWorkInRepo(t *testing.T) {
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
	driver := &stubDriver{writeFile: "feature.go"}
	rs, err := conductor.Run(context.Background(), cfg, conductor.Deps{
		Store: newStore(t), Driver: driver, Gates: gate.ExecRunner{}, Repo: repo,
	})
	if err != nil {
		t.Fatalf("Run: %v", err)
	}
	// The agent's file must be merged into the main repo, not abandoned in a worktree.
	if _, err := os.Stat(filepath.Join(repo, "feature.go")); err != nil {
		t.Errorf("the agent's work should be merged into the repo: %v", err)
	}
	// And the integrated unit must carry the resulting commit hash.
	if u := rs.Units["build"]; u.Status != ledger.Integrated || u.Commit == "" {
		t.Errorf("unit should be integrated with a commit hash: %+v", u)
	}
}

func TestConductorAgentsEmitLive(t *testing.T) {
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
	driver := &stubDriver{emits: []stubEmit{
		{typ: contextgraph.TypeDecisionMade, data: `{"id":"d1","summary":"chose the generic path","governs":["modifier.go"]}`},
	}}
	if _, err := conductor.Run(context.Background(), cfg, conductor.Deps{Store: store, Driver: driver, Gates: gate.ExecRunner{}}); err != nil {
		t.Fatalf("Run: %v", err)
	}
	events, err := store.ReadAll(context.Background(), 0, eventstore.Forward, eventstore.Filter{})
	if err != nil {
		t.Fatal(err)
	}
	if !hasDecision(events, "d1") {
		t.Error("the agent's live-emitted decision should be on the event log")
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
	if !strings.Contains(prompt, "rigger_emit") {
		t.Errorf("agent prompt should include the emit protocol; got %q", prompt)
	}
}

func TestConductorFansOutAndAdjudicates(t *testing.T) {
	repo := initRepo(t)
	cfg := &config.Config{
		Agents: map[string]config.AgentDef{
			"r1": {ID: "r1"}, "r2": {ID: "r2"}, "da": {ID: "da"},
		},
		Workflow: config.Workflow{
			Gates: map[string]config.Gate{"ok": {Run: "true", Kind: "core"}},
			Stages: map[string]config.Stage{
				"review": {Name: "review", Agents: []string{"r1", "r2"}, Adjudicator: "da", Gates: []string{"ok"}},
			},
		},
	}
	store := newStore(t)
	driver := &stubDriver{emits: []stubEmit{{typ: contextgraph.TypeDecisionMade, data: `{"id":"finding","summary":"a finding"}`}}}
	rs, err := conductor.Run(context.Background(), cfg, conductor.Deps{Store: store, Driver: driver, Gates: gate.ExecRunner{}, Repo: repo})
	if err != nil {
		t.Fatalf("Run: %v", err)
	}
	if rs.Units["review"].Status != ledger.Integrated {
		t.Errorf("the review stage should integrate: %+v", rs.Units["review"])
	}
	if driver.spawns.Load() != 3 {
		t.Errorf("expected 3 spawns (2 reviewers + the adjudicator), got %d", driver.spawns.Load())
	}
}

func TestConductorFeedsGraphDecisionsIntoPrompt(t *testing.T) {
	root := t.TempDir()
	// the content must contain the query term so the grep grounder seeds this file
	if err := os.WriteFile(filepath.Join(root, "modifier.go"), []byte("package modifier\nfunc Apply() {}\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	graph := newGraph(t)
	applyDecision(t, graph, "d1", "modifier.go uses the generic engine pipeline", []string{"modifier.go"})

	cfg := &config.Config{
		Agents: map[string]config.AgentDef{"impl": {ID: "impl"}},
		Workflow: config.Workflow{
			Stages: map[string]config.Stage{
				"build": {Name: "build", Agent: "impl", Coverage: "modifier"},
			},
		},
	}
	driver := &stubDriver{}
	if _, err := conductor.Run(context.Background(), cfg, conductor.Deps{
		Store: newStore(t), Driver: driver, Gates: gate.ExecRunner{},
		Grounder: grounder.Grep{Root: root}, Graph: graph,
	}); err != nil {
		t.Fatalf("Run: %v", err)
	}
	prompt, _ := driver.lastPrompt.Load().(string)
	if !strings.Contains(prompt, "generic engine pipeline") {
		t.Errorf("the agent should be fed the decision governing modifier.go; prompt was %q", prompt)
	}
}

func TestConductorRatchetPromotesAReliableGate(t *testing.T) {
	cfg := &config.Config{
		Agents: map[string]config.AgentDef{"a": {ID: "a"}},
		Workflow: config.Workflow{
			Gates: map[string]config.Gate{"ok": {Run: "true", Kind: "core"}},
			Stages: map[string]config.Stage{
				"s1": {Name: "s1", Agent: "a", Gates: []string{"ok"}},
				"s2": {Name: "s2", Agent: "a", Needs: []string{"s1"}, Gates: []string{"ok"}},
				"s3": {Name: "s3", Agent: "a", Needs: []string{"s2"}, Gates: []string{"ok"}},
			},
		},
	}
	store := newStore(t)
	if _, err := conductor.Run(context.Background(), cfg, conductor.Deps{Store: store, Driver: &stubDriver{}, Gates: gate.ExecRunner{}}); err != nil {
		t.Fatalf("Run: %v", err)
	}
	events, err := store.ReadAll(context.Background(), 0, eventstore.Forward, eventstore.Filter{})
	if err != nil {
		t.Fatal(err)
	}
	promoted := false
	for _, e := range events {
		if e.Type == conductor.TypeGatePromoted && strings.Contains(string(e.Data), `"ok"`) {
			promoted = true
		}
	}
	if !promoted {
		t.Error("a gate that passed PromoteThreshold times should earn a promotion")
	}
}

func TestConductorPlannerExtendsTheDAG(t *testing.T) {
	cfg := &config.Config{
		Agents: map[string]config.AgentDef{"planner": {ID: "planner"}, "worker": {ID: "worker"}},
		Workflow: config.Workflow{
			Stages: map[string]config.Stage{
				"plan": {Name: "plan", Agent: "planner", Produces: "dag"},
			},
		},
	}
	driver := &stubDriver{emits: []stubEmit{
		{typ: conductor.TypeUnitProposed, data: `{"id":"impl","agent":"worker"}`},
	}}
	rs, err := conductor.Run(context.Background(), cfg, conductor.Deps{Store: newStore(t), Driver: driver, Gates: gate.ExecRunner{}})
	if err != nil {
		t.Fatalf("Run: %v", err)
	}
	if rs.Units["impl"].Status != ledger.Integrated {
		t.Errorf("the planner-proposed unit should have run and integrated: %+v", rs.Units["impl"])
	}
}

func TestConductorCoverageGate(t *testing.T) {
	cfg := &config.Config{
		Agents: map[string]config.AgentDef{"a": {ID: "a"}},
		Workflow: config.Workflow{
			Stages: map[string]config.Stage{
				"s": {Name: "s", Agent: "a", Coverage: "criterion one"},
			},
		},
	}
	// a criterion with no covering stage -> the run is refused before it starts
	if _, err := conductor.Run(context.Background(), cfg, conductor.Deps{
		Store: newStore(t), Driver: &stubDriver{}, Gates: gate.ExecRunner{},
		Criteria: []string{"criterion one", "criterion two"},
	}); err == nil {
		t.Error("expected a coverage gap error for the uncovered criterion")
	}
	// every criterion covered -> it runs
	if _, err := conductor.Run(context.Background(), cfg, conductor.Deps{
		Store: newStore(t), Driver: &stubDriver{}, Gates: gate.ExecRunner{},
		Criteria: []string{"criterion one"},
	}); err != nil {
		t.Errorf("a fully-covered run should not error: %v", err)
	}
}

func newGraph(t *testing.T) *graphsqlite.Projector {
	t.Helper()
	g, err := graphsqlite.Open(filepath.Join(t.TempDir(), "graph.db"))
	if err != nil {
		t.Fatalf("open graph: %v", err)
	}
	t.Cleanup(func() { _ = g.Close() })
	return g
}

func applyDecision(t *testing.T, g *graphsqlite.Projector, id, summary string, governs []string) {
	t.Helper()
	data, err := json.Marshal(contextgraph.DecisionMade{ID: id, Summary: summary, Governs: governs})
	if err != nil {
		t.Fatal(err)
	}
	// a high position so it never collides with the run's event positions
	ev := eventstore.Event{Type: contextgraph.TypeDecisionMade, Data: data, RecordedAt: time.Now().UTC(), Position: 999999}
	if err := g.Apply(context.Background(), ev); err != nil {
		t.Fatalf("apply decision: %v", err)
	}
}

func TestConductorLearnsFromEscalation(t *testing.T) {
	repo := initRepo(t)
	cfg := &config.Config{
		Agents: map[string]config.AgentDef{"impl": {ID: "impl"}},
		Workflow: config.Workflow{
			Gates: map[string]config.Gate{"bad": {Run: "false", Kind: "core"}},
			Stages: map[string]config.Stage{
				"s": {Name: "s", Agent: "impl", Gates: []string{"bad"}},
			},
		},
	}
	store := newStore(t)
	driver := &stubDriver{writeFile: "broken.go"}
	rs, err := conductor.Run(context.Background(), cfg, conductor.Deps{
		Store: store, Driver: driver, Gates: gate.ExecRunner{}, Repo: repo, Graph: newGraph(t),
	})
	if err != nil {
		t.Fatalf("Run: %v", err)
	}
	if rs.Units["s"].Status != ledger.Escalated {
		t.Fatalf("the unit should have escalated: %+v", rs.Units["s"])
	}
	events, err := store.ReadAll(context.Background(), 0, eventstore.Forward, eventstore.Filter{})
	if err != nil {
		t.Fatal(err)
	}
	if !hasLessonAbout(events, "broken.go") {
		t.Error("escalation should record a lesson about the file the failed unit touched")
	}
}

func hasLessonAbout(events []eventstore.Event, path string) bool {
	for _, e := range events {
		if e.Type != contextgraph.TypeLessonLearned {
			continue
		}
		var l contextgraph.LessonLearned
		if json.Unmarshal(e.Data, &l) == nil {
			for _, a := range l.About {
				if a == path {
					return true
				}
			}
		}
	}
	return false
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
