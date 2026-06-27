package namespace_test

import (
	"context"
	"path/filepath"
	"testing"

	"github.com/virtual-velocitation/rigger/eventstore"
	"github.com/virtual-velocitation/rigger/eventstore/eventstoretest"
	"github.com/virtual-velocitation/rigger/eventstore/namespace"
	"github.com/virtual-velocitation/rigger/eventstore/sqlite"
)

// TestNamespaceContract proves the decorator is transparent: a namespaced store
// honors the full EventStore contract exactly like the raw backend.
func TestNamespaceContract(t *testing.T) {
	eventstoretest.RunContract(t, func(t *testing.T) eventstore.EventStore {
		t.Helper()
		backend, err := sqlite.Open(filepath.Join(t.TempDir(), "events.db"))
		if err != nil {
			t.Fatalf("open backend: %v", err)
		}
		t.Cleanup(func() { _ = backend.Close() })
		return namespace.New(backend, "contract")
	})
}

// TestNamespaceIsolatesProjectsOnOneStore proves two projects sharing one
// backend never see each other's events, and that an identical logical stream
// name in each project does not collide.
func TestNamespaceIsolatesProjectsOnOneStore(t *testing.T) {
	backend, err := sqlite.Open(filepath.Join(t.TempDir(), "events.db"))
	if err != nil {
		t.Fatalf("open backend: %v", err)
	}
	t.Cleanup(func() { _ = backend.Close() })

	alpha := namespace.New(backend, "alpha")
	beta := namespace.New(backend, "beta")
	ctx := context.Background()
	ev := func(id string) eventstore.Event { return eventstore.Event{ID: id, Type: "T", Data: []byte(`{}`)} }

	// Both write to the same logical stream "run"; NoStream must succeed for both
	// because the namespace makes them distinct streams.
	if _, err := alpha.Append(ctx, "run", eventstore.NoStream, ev("a1")); err != nil {
		t.Fatalf("alpha append: %v", err)
	}
	if _, err := beta.Append(ctx, "run", eventstore.NoStream, ev("b1")); err != nil {
		t.Fatalf("beta append (should not collide with alpha): %v", err)
	}

	alphaAll, err := alpha.ReadAll(ctx, 0, eventstore.Forward, eventstore.Filter{})
	if err != nil {
		t.Fatalf("alpha ReadAll: %v", err)
	}
	if got := idsOf(alphaAll); len(got) != 1 || got[0] != "a1" {
		t.Fatalf("alpha sees %v, want [a1] (beta leaked in?)", got)
	}
	if alphaAll[0].Stream != "run" {
		t.Errorf("namespace prefix not stripped: %q", alphaAll[0].Stream)
	}

	betaAll, err := beta.ReadAll(ctx, 0, eventstore.Forward, eventstore.Filter{})
	if err != nil {
		t.Fatalf("beta ReadAll: %v", err)
	}
	if got := idsOf(betaAll); len(got) != 1 || got[0] != "b1" {
		t.Fatalf("beta sees %v, want [b1]", got)
	}
}

func idsOf(evs []eventstore.Event) []string {
	out := make([]string, len(evs))
	for i, e := range evs {
		out[i] = e.ID
	}
	return out
}
