// Package eventstore defines the append-only, bi-temporal event store at the
// heart of Rigger's memory: an immutable log of facts that the context graph is
// projected from. The interface mirrors KurrentDB's primitives (streams, a
// global $all order, optimistic-concurrency append, and catch-up subscriptions)
// so a backend can be swapped without changing the rest of the system.
package eventstore

import (
	"context"
	"fmt"
	"time"
)

// Position is an event's place in the global $all order. Positions are 1-based,
// assigned by the store on append, and only ever increase.
type Position uint64

// Revision is an event's place within its own stream. Revisions are 0-based:
// the first event in a stream is revision 0. An empty stream sits at NoStream.
type Revision int64

// Direction selects read order.
type Direction int

const (
	// Forward reads from the given point toward the newest event.
	Forward Direction = iota
	// Backward reads from the given point toward the oldest event.
	Backward
)

// ExpectedRevision expresses the optimistic-concurrency expectation for Append.
// A value >= 0 requires the stream's current last revision to equal it exactly.
type ExpectedRevision int64

const (
	// Any performs no concurrency check.
	Any ExpectedRevision = -2
	// NoStream requires that the stream does not yet exist.
	NoStream ExpectedRevision = -1
)

// Event is a single immutable fact. Callers populate the input fields; the store
// stamps RecordedAt, Position, and Revision on append.
type Event struct {
	ID     string            // idempotency key, unique within a stream
	Stream string            // set by Append to the target stream
	Type   string            // event type discriminator
	Data   []byte            // opaque payload, typically JSON
	Meta   map[string]string // causation, correlation, and actor metadata

	ValidFrom  time.Time // when the fact became true (caller-supplied)
	RecordedAt time.Time // when the store ingested it (store-stamped)

	Position Position // global $all order (store-assigned)
	Revision Revision // per-stream order (store-assigned)
}

// ConflictError is returned by Append when the stream's actual revision does not
// match the caller's ExpectedRevision.
type ConflictError struct {
	Stream   string
	Expected ExpectedRevision
	Actual   Revision // the stream's real current revision; NoStream if absent
}

func (e *ConflictError) Error() string {
	return fmt.Sprintf("eventstore: concurrency conflict on stream %q: expected revision %d, actual %d",
		e.Stream, e.Expected, e.Actual)
}

// EventStore is the append-only, bi-temporal log. Implementations must be safe
// for concurrent use.
type EventStore interface {
	// Append writes events to the end of a stream, subject to the optimistic
	// expectation. On success it returns the Position of the last event. On a
	// concurrency mismatch it returns a *ConflictError.
	Append(ctx context.Context, stream string, expected ExpectedRevision, events ...Event) (Position, error)

	// ReadStream returns the events of one stream from the given revision, in the
	// given direction.
	ReadStream(ctx context.Context, stream string, from Revision, dir Direction) ([]Event, error)

	// ReadAll returns events across all streams from the given global position,
	// in the given direction, in global $all order.
	ReadAll(ctx context.Context, from Position, dir Direction) ([]Event, error)

	// SubscribeAll returns a catch-up subscription over the global order: every
	// event from `from` onward (history first), then live events as they are
	// appended.
	SubscribeAll(ctx context.Context, from Position) (Subscription, error)

	// Close releases the store's resources.
	Close() error
}

// Subscription is a live, ordered feed of events. Events are delivered on the
// channel in global order; the channel closes when the subscription ends, after
// which Err reports any terminal error.
type Subscription interface {
	Events() <-chan Event
	Err() error
	Close() error
}
