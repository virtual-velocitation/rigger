// Package eventstoretest provides a backend-agnostic contract suite for
// eventstore.EventStore implementations. A backend proves it honors the
// interface by passing RunContract; the SQLite store passes it today, and the
// KurrentDB adapter must pass the identical suite (the proxy-fidelity guarantee
// from the architecture).
package eventstoretest

import (
	"context"
	"errors"
	"testing"
	"time"

	"github.com/virtual-velocitation/rigger/eventstore"
)

// Factory builds a fresh, empty EventStore for a single subtest. The factory
// owns any cleanup (via t.Cleanup); the test only uses the returned store.
type Factory func(t *testing.T) eventstore.EventStore

// RunContract exercises every behavioral guarantee of the EventStore interface
// against the store built by factory.
func RunContract(t *testing.T, factory Factory) {
	t.Helper()
	t.Run("AppendAndReadStreamInOrder", func(t *testing.T) { testAppendReadStream(t, factory) })
	t.Run("GlobalOrderAcrossStreams", func(t *testing.T) { testGlobalOrder(t, factory) })
	t.Run("OptimisticConcurrency", func(t *testing.T) { testOptimisticConcurrency(t, factory) })
	t.Run("CatchUpReplayThenLive", func(t *testing.T) { testCatchUp(t, factory) })
}

func ev(id, typ string) eventstore.Event {
	return eventstore.Event{ID: id, Type: typ, Data: []byte(`{}`), ValidFrom: time.Unix(0, 0).UTC()}
}

func mustAppend(t *testing.T, s eventstore.EventStore, stream string, expected eventstore.ExpectedRevision, evs ...eventstore.Event) eventstore.Position {
	t.Helper()
	pos, err := s.Append(context.Background(), stream, expected, evs...)
	if err != nil {
		t.Fatalf("Append(%q): unexpected error: %v", stream, err)
	}
	return pos
}

func testAppendReadStream(t *testing.T, factory Factory) {
	s := factory(t)
	mustAppend(t, s, "a", eventstore.NoStream, ev("e1", "T1"), ev("e2", "T2"))

	got, err := s.ReadStream(context.Background(), "a", 0, eventstore.Forward)
	if err != nil {
		t.Fatalf("ReadStream forward: %v", err)
	}
	if len(got) != 2 {
		t.Fatalf("forward: want 2 events, got %d", len(got))
	}
	if got[0].ID != "e1" || got[0].Revision != 0 {
		t.Errorf("event[0]: id=%q rev=%d, want e1/0", got[0].ID, got[0].Revision)
	}
	if got[1].ID != "e2" || got[1].Revision != 1 {
		t.Errorf("event[1]: id=%q rev=%d, want e2/1", got[1].ID, got[1].Revision)
	}
	if got[0].Stream != "a" {
		t.Errorf("Stream not populated on read: %q", got[0].Stream)
	}

	back, err := s.ReadStream(context.Background(), "a", 1, eventstore.Backward)
	if err != nil {
		t.Fatalf("ReadStream backward: %v", err)
	}
	if len(back) != 2 || back[0].ID != "e2" || back[1].ID != "e1" {
		t.Errorf("backward order wrong: got %v", ids(back))
	}
}

func testGlobalOrder(t *testing.T, factory Factory) {
	s := factory(t)
	// Interleave two streams; the global $all order must reflect append order.
	mustAppend(t, s, "a", eventstore.NoStream, ev("a1", "T"))
	mustAppend(t, s, "b", eventstore.NoStream, ev("b1", "T"))
	mustAppend(t, s, "a", 0, ev("a2", "T"))

	all, err := s.ReadAll(context.Background(), 0, eventstore.Forward)
	if err != nil {
		t.Fatalf("ReadAll: %v", err)
	}
	if want := []string{"a1", "b1", "a2"}; !equal(ids(all), want) {
		t.Errorf("global order: got %v, want %v", ids(all), want)
	}
	// Positions strictly increase in global order.
	for i := 1; i < len(all); i++ {
		if all[i].Position <= all[i-1].Position {
			t.Errorf("positions not strictly increasing: %d then %d", all[i-1].Position, all[i].Position)
		}
	}
}

func testOptimisticConcurrency(t *testing.T, factory Factory) {
	s := factory(t)
	ctx := context.Background()

	// NoStream onto a fresh stream: ok.
	mustAppend(t, s, "s", eventstore.NoStream, ev("e1", "T"))

	// NoStream onto an existing stream: conflict, Actual is the real revision.
	_, err := s.Append(ctx, "s", eventstore.NoStream, ev("e2", "T"))
	var conflict *eventstore.ConflictError
	if !errors.As(err, &conflict) {
		t.Fatalf("expected *ConflictError on NoStream over existing stream, got %v", err)
	}
	if conflict.Actual != 0 {
		t.Errorf("conflict.Actual = %d, want 0", conflict.Actual)
	}

	// Stale exact expectation: conflict.
	if _, err := s.Append(ctx, "s", 5, ev("e3", "T")); !errors.As(err, &conflict) {
		t.Fatalf("expected *ConflictError on stale expected revision, got %v", err)
	}

	// Correct exact expectation: ok.
	mustAppend(t, s, "s", 0, ev("e3", "T"))
}

func testCatchUp(t *testing.T, factory Factory) {
	s := factory(t)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	mustAppend(t, s, "a", eventstore.NoStream, ev("h1", "T"), ev("h2", "T"))

	sub, err := s.SubscribeAll(ctx, 0)
	if err != nil {
		t.Fatalf("SubscribeAll: %v", err)
	}
	defer func() { _ = sub.Close() }()

	// History first, in order.
	if got := recv(t, sub); got != "h1" {
		t.Fatalf("history[0] = %q, want h1", got)
	}
	if got := recv(t, sub); got != "h2" {
		t.Fatalf("history[1] = %q, want h2", got)
	}

	// Then a live append is delivered.
	mustAppend(t, s, "a", 1, ev("live1", "T"))
	if got := recv(t, sub); got != "live1" {
		t.Fatalf("live event = %q, want live1", got)
	}
}

func recv(t *testing.T, sub eventstore.Subscription) string {
	t.Helper()
	select {
	case e, ok := <-sub.Events():
		if !ok {
			t.Fatalf("subscription channel closed early: %v", sub.Err())
		}
		return e.ID
	case <-time.After(2 * time.Second):
		t.Fatal("timed out waiting for a subscription event")
		return ""
	}
}

func ids(evs []eventstore.Event) []string {
	out := make([]string, len(evs))
	for i, e := range evs {
		out[i] = e.ID
	}
	return out
}

func equal(a, b []string) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}
