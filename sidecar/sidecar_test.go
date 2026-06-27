package sidecar_test

import (
	"context"
	"encoding/json"
	"path/filepath"
	"testing"
	"time"

	"github.com/virtual-velocitation/rigger/contextgraph"
	"github.com/virtual-velocitation/rigger/eventstore"
	"github.com/virtual-velocitation/rigger/eventstore/sqlite"
	"github.com/virtual-velocitation/rigger/sidecar"
)

func TestSidecarSurfacesConcurrentDecisions(t *testing.T) {
	ctx := context.Background()
	store, err := sqlite.Open(filepath.Join(t.TempDir(), "events.db"))
	if err != nil {
		t.Fatalf("open store: %v", err)
	}
	t.Cleanup(func() { _ = store.Close() })

	sc, err := sidecar.Start(ctx, store, 0, eventstore.Filter{})
	if err != nil {
		t.Fatalf("Start: %v", err)
	}
	t.Cleanup(func() { _ = sc.Close() })

	// While this agent "works", another agent records a decision.
	appendDecision(t, store, contextgraph.DecisionMade{ID: "mod-split", Summary: "generic pipeline in engine"})

	// The side-car surfaces it live.
	waitFor(t, func() bool {
		for _, d := range sc.Decisions() {
			if d.ID == "mod-split" {
				return true
			}
		}
		return false
	})
}

func appendDecision(t *testing.T, store *sqlite.Store, d contextgraph.DecisionMade) {
	t.Helper()
	data, err := json.Marshal(d)
	if err != nil {
		t.Fatal(err)
	}
	_, err = store.Append(context.Background(), "decisions", eventstore.Any, eventstore.Event{
		Type: contextgraph.TypeDecisionMade,
		Data: data,
	})
	if err != nil {
		t.Fatalf("append decision: %v", err)
	}
}

func waitFor(t *testing.T, cond func() bool) {
	t.Helper()
	for range 200 {
		if cond() {
			return
		}
		time.Sleep(20 * time.Millisecond)
	}
	t.Fatal("the side-car did not surface the concurrent decision within the timeout")
}
