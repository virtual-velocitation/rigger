// Package conductor executes a workflow: it walks the stage DAG in dependency
// order, runs each stage's agent through the AgentDriver port and its gates
// through the gate.Runner port, advances units under the safety rails, and emits
// the event stream that both the ledger and the context graph project from. It
// is the top-level use case; it depends only on ports and domain, never on a
// concrete adapter (architecture R8).
package conductor

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"sync"
	"time"

	"github.com/google/uuid"

	"github.com/virtual-velocitation/rigger/config"
	"github.com/virtual-velocitation/rigger/contextgraph"
	"github.com/virtual-velocitation/rigger/eventstore"
	"github.com/virtual-velocitation/rigger/gate"
	"github.com/virtual-velocitation/rigger/grounder"
	"github.com/virtual-velocitation/rigger/ledger"
	"github.com/virtual-velocitation/rigger/safety"
	"github.com/virtual-velocitation/rigger/worktree"
)

// AgentResult is what an agent returns when it finishes.
type AgentResult struct {
	Output string
}

// SpawnOpts are per-spawn options.
type SpawnOpts struct {
	Dir string // working directory for the agent; "" means the current directory
}

// AgentDriver spawns an agent to completion. The agent records events it emits
// during its run by calling emit, so its decisions reach the event log live
// (architecture (A)); the workflow driver wires emit to an in-process tool the
// agent calls. tests inject a stub.
type AgentDriver interface {
	Spawn(ctx context.Context, agent config.AgentDef, prompt string, opts SpawnOpts, emit func(eventType string, data any) error) (AgentResult, error)
}

// Deps are the conductor's injected ports.
type Deps struct {
	Store  eventstore.EventStore
	Driver AgentDriver
	Gates  gate.Runner
	// Repo, when set, is a git repository the conductor isolates each agent in
	// via a throwaway worktree, and from which it captures the files the agent
	// touched. Empty disables isolation (the agent runs in the current directory).
	Repo string
	// Grounder, when set, grounds each agent (gives it relevant locations to read)
	// before it runs. Nil grounds nothing.
	Grounder grounder.Grounder
	// Graph, when set, is the live context-graph projection: the conductor folds
	// each emitted event into it during the run and feeds each agent the decisions
	// governing the files it is about to touch. Nil disables graph grounding.
	Graph contextgraph.Projection
	// Criteria are the spec's acceptance criteria (from `rigger run <spec>`). When
	// non-empty, the coverage gate refuses to start a run unless every criterion is
	// covered by a stage - so "done" means every criterion was addressed.
	Criteria []string
}

// Reindexer is an optional grounder capability: the conductor calls it after a
// unit integrates so the index reflects the accepted code (turbovec reindexDelta).
type Reindexer interface {
	Reindex(srcDir string, files []string) error
}

// integrateFunc commits and merges a worktree's changes, returning the commit.
type integrateFunc func(ctx context.Context, wt *worktree.Worktree, files []string, name string) (string, error)

// Stream is the run's event stream name.
const Stream = "run"

// Run executes the workflow and returns the final run state, projected from the
// events it emitted. Independent stages (those whose dependencies are all
// integrated) run concurrently in waves; the conductor appends with
// eventstore.Any since it is the sole logical writer of the run stream and the
// store serializes concurrent appends.
func Run(ctx context.Context, cfg *config.Config, deps Deps) (*ledger.RunState, error) {
	if _, err := topoSort(cfg.Workflow.Stages); err != nil {
		return nil, err
	}
	if err := checkCoverage(cfg, deps.Criteria); err != nil {
		return nil, err
	}

	emit := func(typ string, payload any) error {
		data, err := json.Marshal(payload)
		if err != nil {
			return fmt.Errorf("conductor: encode %s: %w", typ, err)
		}
		ev := eventstore.Event{ID: uuid.NewString(), Type: typ, Data: data, RecordedAt: time.Now().UTC()}
		pos, err := deps.Store.Append(ctx, Stream, eventstore.Any, ev)
		if err != nil {
			return err
		}
		if deps.Graph != nil {
			ev.Position = pos
			_ = deps.Graph.Apply(ctx, ev) // fold into the live graph so later agents can read it
		}
		return nil
	}

	var integrateMu sync.Mutex
	integrate := func(ctx context.Context, wt *worktree.Worktree, files []string, name string) (string, error) {
		integrateMu.Lock()
		defer integrateMu.Unlock()
		commit, err := wt.Integrate(ctx, "rigger: integrate "+name)
		if err != nil {
			return "", err
		}
		if commit != "" {
			if r, ok := deps.Grounder.(Reindexer); ok {
				_ = r.Reindex(deps.Repo, files)
			}
		}
		return commit, nil
	}

	integrated := map[string]bool{}
	terminal := map[string]bool{}
	for {
		ready := readyStages(cfg.Workflow.Stages, integrated, terminal)
		if len(ready) == 0 {
			break // either all done, or the rest are blocked behind an escalation
		}
		if err := runWave(ctx, cfg, deps, ready, emit, integrate, integrated, terminal); err != nil {
			return nil, err
		}
	}

	events, err := deps.Store.ReadStream(ctx, Stream, 0, eventstore.Forward)
	if err != nil {
		return nil, fmt.Errorf("conductor: read run stream: %w", err)
	}
	return ledger.Project(events)
}

// checkCoverage is the coverage gate: every spec criterion must be covered by a
// stage's `coverage` field, else the plan missed a requirement and the run is
// refused before it starts. No criteria (no spec provided) means no gate.
func checkCoverage(cfg *config.Config, criteria []string) error {
	if len(criteria) == 0 {
		return nil
	}
	covered := make(map[string]bool, len(cfg.Workflow.Stages))
	for _, st := range cfg.Workflow.Stages {
		if c := strings.TrimSpace(st.Coverage); c != "" {
			covered[c] = true
		}
	}
	var gaps []string
	for _, c := range criteria {
		if !covered[strings.TrimSpace(c)] {
			gaps = append(gaps, c)
		}
	}
	if len(gaps) > 0 {
		return fmt.Errorf("conductor: coverage gap - no stage covers: %s", strings.Join(gaps, "; "))
	}
	return nil
}

// readyStages returns the not-yet-terminal stages whose dependencies are all
// integrated, sorted for determinism.
func readyStages(stages map[string]config.Stage, integrated, terminal map[string]bool) []string {
	var ready []string
	for name, st := range stages {
		if terminal[name] {
			continue
		}
		ok := true
		for _, need := range st.Needs {
			if !integrated[need] {
				ok = false
				break
			}
		}
		if ok {
			ready = append(ready, name)
		}
	}
	sort.Strings(ready)
	return ready
}

// runWave runs the ready stages concurrently and records which integrated. The
// integrated/terminal maps are written only here, on the single draining
// goroutine, so they are race-free.
func runWave(ctx context.Context, cfg *config.Config, deps Deps, ready []string, emit func(string, any) error, integrate integrateFunc, integrated, terminal map[string]bool) error {
	type result struct {
		name string
		ok   bool
		err  error
	}
	results := make(chan result, len(ready))
	for _, name := range ready {
		go func(name string) {
			st := cfg.Workflow.Stages[name]
			if err := emit(ledger.TypeUnitStarted, ledger.UnitStarted{ID: name, SpecCriterion: st.Coverage}); err != nil {
				results <- result{name: name, err: err}
				return
			}
			ok, err := runStage(ctx, cfg, deps, st, emit, integrate)
			results <- result{name: name, ok: ok, err: err}
		}(name)
	}
	var firstErr error
	for range ready {
		r := <-results
		terminal[r.name] = true
		if r.ok {
			integrated[r.name] = true
		}
		if r.err != nil && firstErr == nil {
			firstErr = r.err
		}
	}
	return firstErr
}

// runStage runs one stage's agent and gates, retrying under the safety bound and
// escalating if the gates keep failing. It returns whether the stage integrated.
func runStage(ctx context.Context, cfg *config.Config, deps Deps, st config.Stage, emit func(string, any) error, integrate integrateFunc) (bool, error) {
	if len(st.Agents) > 0 {
		return runFanOutStage(ctx, cfg, deps, st, emit, integrate)
	}
	wt, finish, err := stageWorktree(ctx, deps, st)
	if err != nil {
		return false, err
	}
	defer finish()
	dir := ""
	if wt != nil {
		dir = wt.Dir
	}

	attempts := 0
	for {
		if st.Agent != "" {
			agentDef, ok := cfg.Agents[st.Agent]
			if !ok {
				return false, fmt.Errorf("conductor: stage %q references unknown agent %q", st.Name, st.Agent)
			}
			if _, err := deps.Driver.Spawn(ctx, agentDef, buildPrompt(ctx, deps, st), SpawnOpts{Dir: dir}, emit); err != nil {
				return false, fmt.Errorf("conductor: stage %q agent %q: %w", st.Name, st.Agent, err)
			}
		}

		allPass := true
		for _, gid := range st.Gates {
			gc := cfg.Workflow.Gates[gid]
			res := deps.Gates.Run(ctx, gate.Gate{ID: gid, Run: gc.Run, Kind: gate.Kind(gc.Kind)}, dir)
			if err := emit(contextgraph.TypeGateVerdict, contextgraph.GateVerdict{Gate: gid, Pass: res.Pass}); err != nil {
				return false, err
			}
			if !res.Pass {
				allPass = false
			}
		}

		if allPass {
			commit, err := integrateAndEmit(ctx, wt, st.Agent, st.Name, emit, integrate)
			if err != nil {
				return false, fmt.Errorf("conductor: integrate %q: %w", st.Name, err)
			}
			if err := emit(ledger.TypeUnitIntegrated, ledger.UnitIntegrated{ID: st.Name, Commit: commit}); err != nil {
				return false, err
			}
			return true, nil
		}

		rem := safety.Remediate(attempts, safety.MaxRetries)
		attempts = rem.Attempts
		if err := emit(ledger.TypeUnitFailed, ledger.UnitFailed{ID: st.Name, Attempts: attempts}); err != nil {
			return false, err
		}
		if rem.Decision == safety.Escalate {
			emitLesson(ctx, wt, emit, st.Name, fmt.Sprintf("unit %q escalated after %d attempts; its gates would not pass", st.Name, attempts))
			if err := emit(ledger.TypeUnitEscalated, ledger.UnitEscalated{ID: st.Name}); err != nil {
				return false, err
			}
			return false, nil
		}
		// otherwise loop and retry the stage
	}
}

// stageWorktree creates an isolated worktree for the stage's agent when a repo is
// configured. It returns the worktree (or nil), a cleanup func, and an error.
func stageWorktree(ctx context.Context, deps Deps, st config.Stage) (*worktree.Worktree, func(), error) {
	noop := func() {}
	if deps.Repo == "" || st.Agent == "" {
		return nil, noop, nil
	}
	id := uuid.NewString()[:8]
	dir := filepath.Join(os.TempDir(), "rigger-wt-"+id)
	wt, err := worktree.Create(ctx, deps.Repo, dir, "rigger/"+st.Name+"-"+id)
	if err != nil {
		return nil, noop, fmt.Errorf("conductor: stage %q worktree: %w", st.Name, err)
	}
	return wt, func() { _ = wt.Remove(ctx) }, nil
}

// integrateAndEmit emits FileTouched for the files the agent changed in its
// worktree and, if there are any, commits and merges them into the base,
// returning the commit hash. A read-only agent (no changes) returns "".
func integrateAndEmit(ctx context.Context, wt *worktree.Worktree, agentID, unitName string, emit func(string, any) error, integrate integrateFunc) (string, error) {
	if wt == nil {
		return "", nil
	}
	files, err := wt.ChangedFiles(ctx)
	if err != nil || len(files) == 0 {
		return "", nil
	}
	for _, f := range files {
		if err := emit(contextgraph.TypeFileTouched, contextgraph.FileTouched{Path: f, By: agentID}); err != nil {
			return "", err
		}
	}
	return integrate(ctx, wt, files, unitName)
}

// emitProtocol tells an agent how to record a decision: call the rigger_emit
// tool the instant the decision is made, which puts it on the shared event log
// live so other agents see it immediately.
const emitProtocol = `Record each decision you make by calling the rigger_emit tool the moment you make it, with type "DecisionMade" and data:
{"id":"<short-id>","summary":"<one line>","governs":["<file>"],"supersedes":"<prior-id-or-empty>"}
This writes it to the shared event log live, so other agents see it immediately.`

// buildPrompt assembles the task prompt for a stage's agent: grounded context
// (if a grounder is configured) plus the emit protocol.
func buildPrompt(ctx context.Context, deps Deps, st config.Stage) string {
	var b strings.Builder
	var seed []string
	if deps.Grounder != nil {
		query := st.Coverage
		if query == "" {
			query = st.Name
		}
		if refs, err := deps.Grounder.Ground(ctx, query, 8); err == nil && len(refs) > 0 {
			b.WriteString("Relevant locations to read first:\n")
			for _, r := range refs {
				fmt.Fprintf(&b, "- %s:%d  %s\n", r.File, r.Line, r.Text)
				seed = appendUnique(seed, r.File)
			}
			b.WriteString("\n")
		}
	}
	b.WriteString(graphContext(ctx, deps, seed))
	b.WriteString(emitProtocol)
	return b.String()
}

// graphContext returns the decisions that govern the seed files and the lessons
// learned about them, read from the live context graph - so an agent never works
// blind to what prior agents decided or what the loop already learned the hard way.
func graphContext(ctx context.Context, deps Deps, seed []string) string {
	if deps.Graph == nil || len(seed) == 0 {
		return ""
	}
	g, err := deps.Graph.Subgraph(ctx, seed, 2)
	if err != nil {
		return ""
	}
	var b strings.Builder
	writeNodes(&b, g, contextgraph.KindDecision, "Decisions that govern these files (do not contradict them; supersede explicitly if you must):")
	writeNodes(&b, g, contextgraph.KindLesson, "Lessons already learned about these files (do not repeat these mistakes):")
	return b.String()
}

func writeNodes(b *strings.Builder, g contextgraph.Graph, kind, header string) {
	first := true
	for _, n := range g.Nodes {
		if n.Kind != kind {
			continue
		}
		if s := n.Attrs["summary"]; s != "" {
			if first {
				b.WriteString(header + "\n")
				first = false
			}
			fmt.Fprintf(b, "- %s: %s\n", n.ID, s)
		}
	}
	if !first {
		b.WriteString("\n")
	}
}

// emitLesson records what the loop learned from a failure, about the files the
// failed unit touched, so future agents grounded on them surface it.
func emitLesson(ctx context.Context, wt *worktree.Worktree, emit func(string, any) error, unitName, summary string) {
	var about []string
	if wt != nil {
		about, _ = wt.ChangedFiles(ctx)
	}
	_ = emit(contextgraph.TypeLessonLearned, contextgraph.LessonLearned{
		ID:      "lesson-" + unitName + "-" + uuid.NewString()[:8],
		Summary: summary,
		About:   about,
	})
}

func appendUnique(s []string, v string) []string {
	for _, x := range s {
		if x == v {
			return s
		}
	}
	return append(s, v)
}

// runFanOutStage runs a stage's agents in parallel (each isolated and harvested),
// then its adjudicator, then its gates in the repo, under the same
// retry-then-escalate rails as a single-agent stage.
func runFanOutStage(ctx context.Context, cfg *config.Config, deps Deps, st config.Stage, emit func(string, any) error, integrate integrateFunc) (bool, error) {
	attempts := 0
	for {
		if err := runAgentsConcurrently(ctx, cfg, deps, st, st.Agents, emit, integrate); err != nil {
			return false, err
		}
		if st.Adjudicator != "" {
			if err := runAgentsConcurrently(ctx, cfg, deps, st, []string{st.Adjudicator}, emit, integrate); err != nil {
				return false, err
			}
		}

		allPass := true
		for _, gid := range st.Gates {
			gc := cfg.Workflow.Gates[gid]
			res := deps.Gates.Run(ctx, gate.Gate{ID: gid, Run: gc.Run, Kind: gate.Kind(gc.Kind)}, "")
			if err := emit(contextgraph.TypeGateVerdict, contextgraph.GateVerdict{Gate: gid, Pass: res.Pass}); err != nil {
				return false, err
			}
			if !res.Pass {
				allPass = false
			}
		}
		if allPass {
			if err := emit(ledger.TypeUnitIntegrated, ledger.UnitIntegrated{ID: st.Name}); err != nil {
				return false, err
			}
			return true, nil
		}

		rem := safety.Remediate(attempts, safety.MaxRetries)
		attempts = rem.Attempts
		if err := emit(ledger.TypeUnitFailed, ledger.UnitFailed{ID: st.Name, Attempts: attempts}); err != nil {
			return false, err
		}
		if rem.Decision == safety.Escalate {
			emitLesson(ctx, nil, emit, st.Name, fmt.Sprintf("review stage %q escalated after %d attempts", st.Name, attempts))
			if err := emit(ledger.TypeUnitEscalated, ledger.UnitEscalated{ID: st.Name}); err != nil {
				return false, err
			}
			return false, nil
		}
	}
}

// runAgentsConcurrently creates each agent's worktree sequentially (git worktree
// creation is not concurrency-safe), then runs the agents in parallel, harvesting
// each one's emissions and touched files.
func runAgentsConcurrently(ctx context.Context, cfg *config.Config, deps Deps, st config.Stage, agentIDs []string, emit func(string, any) error, integrate integrateFunc) error {
	type job struct {
		agentID string
		wt      *worktree.Worktree
	}
	var jobs []job
	var finishers []func()
	cleanup := func() {
		for _, f := range finishers {
			f()
		}
	}
	for _, a := range agentIDs {
		wt, finish, err := agentWorktree(ctx, deps, st, a)
		if err != nil {
			cleanup()
			return err
		}
		jobs = append(jobs, job{agentID: a, wt: wt})
		finishers = append(finishers, finish)
	}
	defer cleanup()

	errs := make(chan error, len(jobs))
	for _, j := range jobs {
		go func(j job) { errs <- runAgentInWorktree(ctx, cfg, deps, st, j.agentID, j.wt, emit, integrate) }(j)
	}
	var first error
	for range jobs {
		if e := <-errs; e != nil && first == nil {
			first = e
		}
	}
	return first
}

// runAgentInWorktree spawns one agent in a prepared worktree and harvests its work.
func runAgentInWorktree(ctx context.Context, cfg *config.Config, deps Deps, st config.Stage, agentID string, wt *worktree.Worktree, emit func(string, any) error, integrate integrateFunc) error {
	agentDef, ok := cfg.Agents[agentID]
	if !ok {
		return fmt.Errorf("conductor: stage %q references unknown agent %q", st.Name, agentID)
	}
	dir := ""
	if wt != nil {
		dir = wt.Dir
	}
	if _, err := deps.Driver.Spawn(ctx, agentDef, buildPrompt(ctx, deps, st), SpawnOpts{Dir: dir}, emit); err != nil {
		return fmt.Errorf("conductor: stage %q agent %q: %w", st.Name, agentID, err)
	}
	_, err := integrateAndEmit(ctx, wt, agentID, st.Name+"/"+agentID, emit, integrate)
	return err
}

// agentWorktree creates an isolated worktree for one agent in a fan-out, or nil
// when no repo is configured.
func agentWorktree(ctx context.Context, deps Deps, st config.Stage, agentID string) (*worktree.Worktree, func(), error) {
	noop := func() {}
	if deps.Repo == "" {
		return nil, noop, nil
	}
	id := uuid.NewString()[:8]
	dir := filepath.Join(os.TempDir(), "rigger-wt-"+id)
	wt, err := worktree.Create(ctx, deps.Repo, dir, "rigger/"+st.Name+"-"+agentID+"-"+id)
	if err != nil {
		return nil, noop, fmt.Errorf("conductor: stage %q agent %q worktree: %w", st.Name, agentID, err)
	}
	return wt, func() { _ = wt.Remove(ctx) }, nil
}

// topoSort returns the stages in dependency order (a stage's needs come first).
// The config is already validated acyclic; a residual cycle is a hard error.
func topoSort(stages map[string]config.Stage) ([]string, error) {
	indeg := make(map[string]int, len(stages))
	dependents := make(map[string][]string, len(stages))
	for name, st := range stages {
		indeg[name] = len(st.Needs)
		for _, need := range st.Needs {
			dependents[need] = append(dependents[need], name)
		}
	}
	var queue []string
	for name, d := range indeg {
		if d == 0 {
			queue = append(queue, name)
		}
	}
	sort.Strings(queue)

	var order []string
	for len(queue) > 0 {
		n := queue[0]
		queue = queue[1:]
		order = append(order, n)
		var ready []string
		for _, dep := range dependents[n] {
			indeg[dep]--
			if indeg[dep] == 0 {
				ready = append(ready, dep)
			}
		}
		sort.Strings(ready)
		queue = append(queue, ready...)
	}
	if len(order) != len(stages) {
		return nil, fmt.Errorf("conductor: workflow has a dependency cycle")
	}
	return order, nil
}
