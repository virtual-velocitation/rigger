// Command rigger is the harness CLI. `rigger run` executes the configured
// workflow with the default CLI agent driver and the embedded SQLite event store;
// `rigger graph` inspects the context graph; `rigger init` scaffolds a project.
package main

import (
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/virtual-velocitation/rigger/conductor"
	"github.com/virtual-velocitation/rigger/config"
	"github.com/virtual-velocitation/rigger/contextgraph"
	graphsqlite "github.com/virtual-velocitation/rigger/contextgraph/sqlite"
	"github.com/virtual-velocitation/rigger/driver/cli"
	"github.com/virtual-velocitation/rigger/driver/workflow"
	"github.com/virtual-velocitation/rigger/eventstore"
	eventsqlite "github.com/virtual-velocitation/rigger/eventstore/sqlite"
	"github.com/virtual-velocitation/rigger/gate"
	"github.com/virtual-velocitation/rigger/grounder"
	"github.com/virtual-velocitation/rigger/hooks"
	"github.com/virtual-velocitation/rigger/ledger"
	"github.com/virtual-velocitation/rigger/mcpserver"
	"github.com/virtual-velocitation/rigger/sidecar"
	"github.com/virtual-velocitation/rigger/spec"
)

const riggerDir = ".rigger"

func main() {
	if len(os.Args) < 2 {
		usage()
		os.Exit(2)
	}
	var err error
	switch os.Args[1] {
	case "run":
		err = cmdRun(os.Args[2:])
	case "serve":
		err = cmdServe(os.Args[2:])
	case "graph":
		err = cmdGraph(os.Args[2:])
	case "validate":
		err = cmdValidate()
	case "init":
		err = cmdInit()
	case "setup":
		err = cmdSetup()
	case "prime":
		err = cmdPrime()
	case "help", "-h", "--help":
		usage()
	default:
		fmt.Fprintf(os.Stderr, "rigger: unknown command %q\n", os.Args[1])
		usage()
		os.Exit(2)
	}
	if err != nil {
		fmt.Fprintln(os.Stderr, "rigger:", err)
		os.Exit(1)
	}
}

func usage() {
	fmt.Fprint(os.Stderr, `rigger - a config-driven, event-sourced multi-agent dev-loop harness

usage:
  rigger run                  run the workflow with the standalone CLI driver
  rigger serve                run as an MCP server for the Claude Code workflow shim
  rigger graph --around <id>  print the context subgraph around a node
  rigger validate             load and validate the workflow + agents
  rigger init                 scaffold .rigger/ (workflow.yml + an agents/ folder)
  rigger setup                init, then install a Claude Code SessionStart hook
  rigger prime                print recent decisions (what the hook runs)

storage and graph live in ./.rigger/ (per project, like .git/).
`)
}

func cmdRun(args []string) error {
	ctx := context.Background()
	cfg, err := config.Load(".")
	if err != nil {
		return err
	}
	var criteria []string
	if len(args) > 0 {
		specText, err := os.ReadFile(args[0])
		if err != nil {
			return fmt.Errorf("read spec %s: %w", args[0], err)
		}
		criteria = spec.ExtractCriteria(string(specText))
	}
	if err := os.MkdirAll(riggerDir, 0o755); err != nil {
		return err
	}
	store, err := eventsqlite.Open(filepath.Join(riggerDir, "events.db"))
	if err != nil {
		return err
	}
	defer func() { _ = store.Close() }()
	graph, err := graphsqlite.Open(filepath.Join(riggerDir, "graph.db"))
	if err != nil {
		return err
	}
	defer func() { _ = graph.Close() }()

	rs, err := conductor.Run(ctx, cfg, conductor.Deps{
		Store:    store,
		Driver:   cli.Driver{},
		Gates:    gate.ExecRunner{},
		Repo:     gitRepo(),
		Grounder: grounder.Grep{Root: "."},
		Graph:    graph,
		Criteria: criteria,
	})
	if err != nil {
		return err
	}
	printRunState(rs)
	return nil
}

// cmdServe runs the conductor in the background and serves the MCP bridge over
// stdio, so a Claude Code workflow shim drives the agents via agent() while the
// Go conductor orchestrates. It exits when the run completes.
func cmdServe(_ []string) error {
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	cfg, err := config.Load(".")
	if err != nil {
		return err
	}
	if err := os.MkdirAll(riggerDir, 0o755); err != nil {
		return err
	}
	store, err := eventsqlite.Open(filepath.Join(riggerDir, "events.db"))
	if err != nil {
		return err
	}
	defer func() { _ = store.Close() }()
	graph, err := graphsqlite.Open(filepath.Join(riggerDir, "graph.db"))
	if err != nil {
		return err
	}
	defer func() { _ = graph.Close() }()

	peers, err := sidecar.Start(ctx, store, 0, eventstore.Filter{})
	if err != nil {
		return err
	}
	defer func() { _ = peers.Close() }()

	driver := workflow.New()
	go func() {
		if _, runErr := conductor.Run(ctx, cfg, conductor.Deps{
			Store: store, Driver: driver, Gates: gate.ExecRunner{},
			Repo: gitRepo(), Grounder: grounder.Grep{Root: "."}, Graph: graph,
		}); runErr != nil {
			fmt.Fprintln(os.Stderr, "rigger: conductor:", runErr)
		}
		cancel() // the run is done; stop serving
	}()

	if err := mcpserver.New(driver, store, conductor.Stream, peers).Run(ctx, &mcp.StdioTransport{}); err != nil && ctx.Err() == nil {
		return err
	}
	return nil
}

// gitRepo returns the git repository root, for worktree isolation, or "" if the
// current directory is not a git repository (agents then run in place).
func gitRepo() string {
	out, err := exec.Command("git", "rev-parse", "--show-toplevel").Output()
	if err != nil {
		return ""
	}
	return strings.TrimSpace(string(out))
}

func printRunState(rs *ledger.RunState) {
	names := make([]string, 0, len(rs.Units))
	for name := range rs.Units {
		names = append(names, name)
	}
	sort.Strings(names)
	fmt.Println("run state:")
	for _, name := range names {
		u := rs.Units[name]
		fmt.Printf("  %-20s %s\n", name, u.Status)
	}
	if rs.Done() {
		fmt.Println("done: every unit integrated")
	} else {
		fmt.Println("incomplete: not every unit integrated")
	}
}

func cmdGraph(args []string) error {
	fs := flag.NewFlagSet("graph", flag.ContinueOnError)
	around := fs.String("around", "", "node id to center the subgraph on")
	depth := fs.Int("depth", 2, "how many hops to traverse")
	if err := fs.Parse(args); err != nil {
		return err
	}
	if *around == "" {
		return fmt.Errorf("graph: --around <id> is required")
	}
	gp, err := graphsqlite.Open(filepath.Join(riggerDir, "graph.db"))
	if err != nil {
		return err
	}
	defer func() { _ = gp.Close() }()
	g, err := gp.Subgraph(context.Background(), []string{*around}, *depth)
	if err != nil {
		return err
	}
	fmt.Printf("subgraph around %q (depth %d):\n", *around, *depth)
	for _, n := range g.Nodes {
		fmt.Printf("  node %-24s %s\n", n.ID, n.Kind)
	}
	for _, e := range g.Edges {
		fmt.Printf("  edge %s -%s-> %s\n", e.From, e.Rel, e.To)
	}
	if len(g.Nodes) == 0 {
		fmt.Println("  (nothing found; has `rigger run` been run yet?)")
	}
	return nil
}

func cmdValidate() error {
	cfg, err := config.Load(".")
	if err != nil {
		return err
	}
	fmt.Printf("config valid: %d agents, %d stages, %d gates\n",
		len(cfg.Agents), len(cfg.Workflow.Stages), len(cfg.Workflow.Gates))
	return nil
}

func cmdInit() error {
	if err := os.MkdirAll(filepath.Join(riggerDir, "agents"), 0o755); err != nil {
		return err
	}
	writeIfAbsent(filepath.Join(riggerDir, "workflow.yml"), scaffoldWorkflow)
	writeIfAbsent(filepath.Join(riggerDir, "agents", "builder.md"), scaffoldAgent)
	fmt.Println("scaffolded .rigger/workflow.yml and .rigger/agents/builder.md")
	return nil
}

func writeIfAbsent(path, content string) {
	if _, err := os.Stat(path); err == nil {
		fmt.Printf("kept existing %s\n", path)
		return
	}
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		fmt.Fprintf(os.Stderr, "rigger: write %s: %v\n", path, err)
	}
}

func cmdSetup() error {
	if err := cmdInit(); err != nil {
		return err
	}
	if err := os.MkdirAll(".claude", 0o755); err != nil {
		return err
	}
	settingsPath := filepath.Join(".claude", "settings.json")
	existing, err := os.ReadFile(settingsPath)
	if err != nil && !os.IsNotExist(err) {
		return fmt.Errorf("read %s: %w", settingsPath, err)
	}
	merged, err := hooks.InstallSessionStart(existing, "rigger prime")
	if err != nil {
		return err
	}
	if err := os.WriteFile(settingsPath, merged, 0o644); err != nil {
		return fmt.Errorf("write %s: %w", settingsPath, err)
	}
	fmt.Println("installed a SessionStart hook in .claude/settings.json (it runs `rigger prime`)")
	return nil
}

func cmdPrime() error {
	path := filepath.Join(riggerDir, "events.db")
	if _, err := os.Stat(path); err != nil {
		fmt.Println("# Rigger: no decisions recorded yet (run `rigger run` to start).")
		return nil
	}
	store, err := eventsqlite.Open(path)
	if err != nil {
		return err
	}
	defer func() { _ = store.Close() }()
	events, err := store.ReadAll(context.Background(), 0, eventstore.Backward, eventstore.Filter{})
	if err != nil {
		return err
	}
	fmt.Println("# Rigger: recent decisions")
	shown := 0
	for _, e := range events {
		if e.Type != contextgraph.TypeDecisionMade {
			continue
		}
		var d contextgraph.DecisionMade
		if json.Unmarshal(e.Data, &d) != nil {
			continue
		}
		fmt.Printf("- %s: %s\n", d.ID, d.Summary)
		if shown++; shown >= 10 {
			break
		}
	}
	if shown == 0 {
		fmt.Println("(none yet)")
	}
	return nil
}

const scaffoldWorkflow = `name: example
gates:
  smoke: { run: "echo 'no gate configured yet'; true", kind: core }
stages:
  build:
    agent: builder
    gates: [smoke]
`

const scaffoldAgent = `---
id: builder
model: sonnet
tools: [Read, Edit, Bash]
---
You are a builder agent. Implement the task, run the gates, and report concisely.
`
