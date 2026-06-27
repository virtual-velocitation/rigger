package mcpserver

import (
	"context"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/virtual-velocitation/rigger/conductor"
	"github.com/virtual-velocitation/rigger/config"
	"github.com/virtual-velocitation/rigger/contextgraph"
	"github.com/virtual-velocitation/rigger/driver/workflow"
	"github.com/virtual-velocitation/rigger/eventstore"
	"github.com/virtual-velocitation/rigger/eventstore/sqlite"
)

func TestServerBuilds(t *testing.T) {
	if New(workflow.New(), newStore(t), "run") == nil {
		t.Fatal("New returned nil")
	}
}

func TestBridgeEmitAppendsToStore(t *testing.T) {
	store := newStore(t)
	b := &bridge{driver: workflow.New(), store: store, stream: "run"}
	if _, _, err := b.emit(context.Background(), nil, emitIn{Type: contextgraph.TypeDecisionMade, Data: map[string]any{"id": "d1", "summary": "x"}}); err != nil {
		t.Fatalf("emit: %v", err)
	}
	events, err := store.ReadAll(context.Background(), 0, eventstore.Forward, eventstore.Filter{})
	if err != nil {
		t.Fatal(err)
	}
	found := false
	for _, e := range events {
		if e.Type == contextgraph.TypeDecisionMade && strings.Contains(string(e.Data), "d1") {
			found = true
		}
	}
	if !found {
		t.Error("rigger_emit should append the decision to the store live")
	}
}

func TestBridgeNextAndResultDriveSpawn(t *testing.T) {
	driver := workflow.New()
	b := &bridge{driver: driver, store: newStore(t), stream: "run"}

	done := make(chan conductor.AgentResult, 1)
	go func() {
		res, _ := driver.Spawn(context.Background(), config.AgentDef{ID: "a", Model: "sonnet"}, "do it", conductor.SpawnOpts{}, nil)
		done <- res
	}()

	// rigger_next picks up the queued spawn.
	var out nextOut
	deadline := time.Now().Add(2 * time.Second)
	for {
		_, o, _ := b.next(context.Background(), nil, empty{})
		if o.ID != "" {
			out = o
			break
		}
		if time.Now().After(deadline) {
			t.Fatal("rigger_next never returned the queued spawn")
		}
		time.Sleep(time.Millisecond)
	}
	if out.Prompt != "do it" || out.Model != "sonnet" {
		t.Errorf("next out = %+v", out)
	}

	// rigger_result completes it.
	if _, _, err := b.result(context.Background(), nil, resultIn{ID: out.ID, Output: "done"}); err != nil {
		t.Fatalf("result: %v", err)
	}
	if res := <-done; res.Output != "done" {
		t.Errorf("spawn result = %q", res.Output)
	}
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
