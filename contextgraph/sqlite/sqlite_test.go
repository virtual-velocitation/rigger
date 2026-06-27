package sqlite_test

import (
	"context"
	"encoding/json"
	"path/filepath"
	"testing"
	"time"

	"github.com/virtual-velocitation/rigger/contextgraph"
	"github.com/virtual-velocitation/rigger/contextgraph/sqlite"
	"github.com/virtual-velocitation/rigger/eventstore"
)

// TestModifierSagaProjection is the architecture's worked example (section 7):
// a collapse decision is superseded by a split decision, and the next agent that
// looks at the file must be handed the current decision, never the invalidated
// one. It also covers idempotent replay.
func TestModifierSagaProjection(t *testing.T) {
	p, err := sqlite.Open(filepath.Join(t.TempDir(), "graph.db"))
	if err != nil {
		t.Fatalf("open: %v", err)
	}
	t.Cleanup(func() { _ = p.Close() })
	ctx := context.Background()
	base := time.Date(2026, 6, 27, 10, 0, 0, 0, time.UTC)

	apply := func(pos uint64, mins int, typ string, payload any) {
		t.Helper()
		data, err := json.Marshal(payload)
		if err != nil {
			t.Fatal(err)
		}
		e := eventstore.Event{
			Position:   eventstore.Position(pos),
			Type:       typ,
			Data:       data,
			RecordedAt: base.Add(time.Duration(mins) * time.Minute),
		}
		if err := p.Apply(ctx, e); err != nil {
			t.Fatalf("apply %s: %v", typ, err)
		}
	}

	apply(1, 0, contextgraph.TypeDecisionMade, contextgraph.DecisionMade{ID: "mod-collapse", Summary: "move whole modifier to ga-*", Governs: []string{"modifier.rs"}})
	apply(2, 90, contextgraph.TypeDecisionMade, contextgraph.DecisionMade{ID: "mod-split", Summary: "generic pipeline in engine", Governs: []string{"modifier.rs"}, Supersedes: "mod-collapse"})
	apply(3, 91, contextgraph.TypeFileTouched, contextgraph.FileTouched{Path: "modifier.rs", By: "impl-mod"})
	apply(4, 92, contextgraph.TypeGateVerdict, contextgraph.GateVerdict{Gate: "e7", Pass: true, Artifact: "modifier.rs"})
	// Replay event 2: must be a no-op (idempotency by global position).
	apply(2, 90, contextgraph.TypeDecisionMade, contextgraph.DecisionMade{ID: "mod-split", Summary: "ignored", Governs: []string{"modifier.rs"}, Supersedes: "mod-collapse"})

	g, err := p.Subgraph(ctx, []string{"modifier.rs"}, 2)
	if err != nil {
		t.Fatalf("subgraph: %v", err)
	}

	if !hasEdge(g, "mod-split", "modifier.rs", contextgraph.RelGoverns) {
		t.Error("expected mod-split GOVERNS modifier.rs (the current decision)")
	}
	if hasEdge(g, "mod-collapse", "modifier.rs", contextgraph.RelGoverns) {
		t.Error("mod-collapse GOVERNS modifier.rs must be invalidated, not in the valid subgraph")
	}
	if !hasEdge(g, "mod-split", "mod-collapse", contextgraph.RelSupersedes) {
		t.Error("expected the supersession pointer mod-split SUPERSEDES mod-collapse (history is traversable)")
	}
	if !hasNode(g, "impl-mod") || !hasNode(g, "e7") {
		t.Errorf("expected impl-mod and e7 reachable from modifier.rs; got nodes %v", nodeIDs(g))
	}
	if n := countEdge(g, "mod-split", "modifier.rs", contextgraph.RelGoverns); n != 1 {
		t.Errorf("idempotency: want exactly 1 mod-split GOVERNS edge, got %d (replay double-folded?)", n)
	}

	if id, ok, err := p.Resolve(ctx, "mod-split"); err != nil || !ok || id != "mod-split" {
		t.Errorf("Resolve(mod-split) = %q, %v, %v; want mod-split, true, nil", id, ok, err)
	}
	if _, ok, err := p.Resolve(ctx, "does-not-exist"); err != nil || ok {
		t.Errorf("Resolve(unknown) = ok %v, err %v; want false, nil", ok, err)
	}
}

func hasEdge(g contextgraph.Graph, from, to, rel string) bool {
	return countEdge(g, from, to, rel) > 0
}

func countEdge(g contextgraph.Graph, from, to, rel string) int {
	n := 0
	for _, e := range g.Edges {
		if e.From == from && e.To == to && e.Rel == rel {
			n++
		}
	}
	return n
}

func hasNode(g contextgraph.Graph, id string) bool {
	for _, n := range g.Nodes {
		if n.ID == id {
			return true
		}
	}
	return false
}

func nodeIDs(g contextgraph.Graph) []string {
	out := make([]string, len(g.Nodes))
	for i, n := range g.Nodes {
		out[i] = n.ID
	}
	return out
}
