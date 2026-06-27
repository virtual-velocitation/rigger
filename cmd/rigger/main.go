// Command rigger is the harness CLI. `rigger run` executes the configured
// workflow with the default CLI agent driver and the embedded SQLite event store;
// `rigger graph` inspects the context graph; `rigger init` scaffolds a project.
package main

import (
	"context"
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"sort"

	"github.com/virtual-velocitation/rigger/conductor"
	"github.com/virtual-velocitation/rigger/config"
	graphsqlite "github.com/virtual-velocitation/rigger/contextgraph/sqlite"
	"github.com/virtual-velocitation/rigger/driver/cli"
	"github.com/virtual-velocitation/rigger/eventstore"
	eventsqlite "github.com/virtual-velocitation/rigger/eventstore/sqlite"
	"github.com/virtual-velocitation/rigger/gate"
	"github.com/virtual-velocitation/rigger/ledger"
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
	case "graph":
		err = cmdGraph(os.Args[2:])
	case "init":
		err = cmdInit()
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
  rigger run                  run the workflow in .rigger/workflow.yml
  rigger graph --around <id>  print the context subgraph around a node
  rigger init                 scaffold a workflow and an agents/ folder

storage and graph live in ./.rigger/ (per project, like .git/).
`)
}

func cmdRun(_ []string) error {
	ctx := context.Background()
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

	rs, err := conductor.Run(ctx, cfg, conductor.Deps{
		Store:  store,
		Driver: cli.Driver{},
		Gates:  gate.ExecRunner{},
	})
	if err != nil {
		return err
	}
	if err := projectGraph(ctx, store); err != nil {
		return err
	}
	printRunState(rs)
	return nil
}

func projectGraph(ctx context.Context, store *eventsqlite.Store) error {
	events, err := store.ReadAll(ctx, 0, eventstore.Forward, eventstore.Filter{})
	if err != nil {
		return err
	}
	gp, err := graphsqlite.Open(filepath.Join(riggerDir, "graph.db"))
	if err != nil {
		return err
	}
	defer func() { _ = gp.Close() }()
	for _, e := range events {
		if err := gp.Apply(ctx, e); err != nil {
			return err
		}
	}
	return nil
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

func cmdInit() error {
	if err := os.MkdirAll(filepath.Join(riggerDir), 0o755); err != nil {
		return err
	}
	if err := os.MkdirAll("agents", 0o755); err != nil {
		return err
	}
	writeIfAbsent(filepath.Join(riggerDir, "workflow.yml"), scaffoldWorkflow)
	writeIfAbsent(filepath.Join("agents", "builder.md"), scaffoldAgent)
	fmt.Println("scaffolded .rigger/workflow.yml and agents/builder.md")
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
