// Package kurrentdb is the optional KurrentDB EventStore adapter. It maps the
// KurrentDB Go client onto the eventstore port so a project can swap the default
// embedded SQLite store for a shared KurrentDB server with no change to the rest
// of Rigger. It must pass the same contract suite SQLite does (proxy fidelity).
package kurrentdb

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"strings"
	"sync"
	"time"

	"github.com/google/uuid"
	kdb "github.com/kurrent-io/KurrentDB-Client-Go/kurrentdb"

	"github.com/virtual-velocitation/rigger/eventstore"
)

// Store is the KurrentDB-backed eventstore.EventStore.
type Store struct {
	client *kdb.Client
}

var _ eventstore.EventStore = (*Store)(nil)

// Open connects to KurrentDB using a connection string, e.g.
// "kurrentdb://localhost:2113?tls=false".
func Open(connString string) (*Store, error) {
	conf, err := kdb.ParseConnectionString(connString)
	if err != nil {
		return nil, fmt.Errorf("kurrentdb: parse connection string: %w", err)
	}
	client, err := kdb.NewClient(conf)
	if err != nil {
		return nil, fmt.Errorf("kurrentdb: new client: %w", err)
	}
	return &Store{client: client}, nil
}

// Close closes the client.
func (s *Store) Close() error { return s.client.Close() }

// envelope carries Rigger's caller-supplied id, valid-from time, and metadata in
// the KurrentDB event metadata (KurrentDB owns the event id and recorded time).
type envelope struct {
	ID        string            `json:"id"`
	ValidFrom time.Time         `json:"valid_from"`
	Meta      map[string]string `json:"meta,omitempty"`
}

// Append writes events to a stream under the optimistic expectation.
func (s *Store) Append(ctx context.Context, stream string, expected eventstore.ExpectedRevision, events ...eventstore.Event) (eventstore.Position, error) {
	if len(events) == 0 {
		return 0, nil
	}
	now := time.Now().UTC()
	data := make([]kdb.EventData, len(events))
	for i, e := range events {
		vf := e.ValidFrom
		if vf.IsZero() {
			vf = now
		}
		metaBytes, err := json.Marshal(envelope{ID: e.ID, ValidFrom: vf, Meta: e.Meta})
		if err != nil {
			return 0, fmt.Errorf("kurrentdb: encode metadata: %w", err)
		}
		data[i] = kdb.EventData{
			EventID:     uuid.New(),
			EventType:   e.Type,
			ContentType: kdb.ContentTypeJson,
			Data:        e.Data,
			Metadata:    metaBytes,
		}
	}
	res, err := s.client.AppendToStream(ctx, stream, kdb.AppendToStreamOptions{StreamState: toStreamState(expected)}, data...)
	if err != nil {
		if conflict := s.asConflict(ctx, stream, expected, err); conflict != nil {
			return 0, conflict
		}
		return 0, fmt.Errorf("kurrentdb: append: %w", err)
	}
	return eventstore.Position(res.CommitPosition), nil
}

func toStreamState(e eventstore.ExpectedRevision) kdb.StreamState {
	switch e {
	case eventstore.Any:
		return kdb.Any{}
	case eventstore.NoStream:
		return kdb.NoStream{}
	default:
		return kdb.Revision(uint64(e))
	}
}

// asConflict maps a wrong-expected-version error to a *ConflictError, reading the
// stream's real revision so Actual is accurate.
func (s *Store) asConflict(ctx context.Context, stream string, expected eventstore.ExpectedRevision, err error) *eventstore.ConflictError {
	kerr, _ := kdb.FromError(err)
	wrongVersion := (kerr != nil && kerr.IsErrorCode(kdb.ErrorCodeWrongExpectedVersion)) ||
		strings.Contains(err.Error(), "wrong expected version")
	if !wrongVersion {
		return nil
	}
	return &eventstore.ConflictError{Stream: stream, Expected: expected, Actual: s.currentRevision(ctx, stream)}
}

func (s *Store) currentRevision(ctx context.Context, stream string) eventstore.Revision {
	rs, err := s.client.ReadStream(ctx, stream, kdb.ReadStreamOptions{Direction: kdb.Backwards, From: kdb.End{}}, 1)
	if err != nil {
		return -1
	}
	defer rs.Close()
	resolved, err := rs.Recv()
	if err != nil {
		return -1 // empty stream
	}
	return eventstore.Revision(resolved.OriginalEvent().EventNumber)
}

// ReadStream returns one stream's events from a revision, in a direction.
func (s *Store) ReadStream(ctx context.Context, stream string, from eventstore.Revision, dir eventstore.Direction) ([]eventstore.Event, error) {
	opts := kdb.ReadStreamOptions{Direction: toDirection(dir), From: kdb.Revision(uint64(max64(from, 0)))}
	rs, err := s.client.ReadStream(ctx, stream, opts, ^uint64(0))
	if err != nil {
		if isNotFound(err) {
			return nil, nil
		}
		return nil, fmt.Errorf("kurrentdb: read stream: %w", err)
	}
	defer rs.Close()
	return collect(rs, eventstore.Filter{})
}

// ReadAll returns events across all streams from a global position, narrowed by
// filter, excluding system ($) streams.
func (s *Store) ReadAll(ctx context.Context, from eventstore.Position, dir eventstore.Direction, filter eventstore.Filter) ([]eventstore.Event, error) {
	opts := kdb.ReadAllOptions{Direction: toDirection(dir), From: toAllPosition(from)}
	rs, err := s.client.ReadAll(ctx, opts, ^uint64(0))
	if err != nil {
		return nil, fmt.Errorf("kurrentdb: read all: %w", err)
	}
	defer rs.Close()
	return collect(rs, filter)
}

// SubscribeAll returns a catch-up subscription, narrowed by filter, excluding
// system streams (via a server-side filter).
func (s *Store) SubscribeAll(ctx context.Context, from eventstore.Position, filter eventstore.Filter) (eventstore.Subscription, error) {
	ksub, err := s.client.SubscribeToAll(ctx, kdb.SubscribeToAllOptions{From: toAllPosition(from), Filter: toFilter(filter)})
	if err != nil {
		return nil, fmt.Errorf("kurrentdb: subscribe: %w", err)
	}
	sub := &subscription{events: make(chan eventstore.Event, 128), ksub: ksub, done: make(chan struct{})}
	go sub.run(ctx, filter)
	return sub, nil
}

func toDirection(d eventstore.Direction) kdb.Direction {
	if d == eventstore.Backward {
		return kdb.Backwards
	}
	return kdb.Forwards
}

func toAllPosition(from eventstore.Position) kdb.AllPosition {
	if from == 0 {
		return kdb.Start{}
	}
	return kdb.Position{Commit: uint64(from), Prepare: uint64(from)}
}

// toFilter excludes system streams and, when a prefix is set, scopes to it.
func toFilter(f eventstore.Filter) *kdb.SubscriptionFilter {
	if f.StreamPrefix != "" {
		return &kdb.SubscriptionFilter{Type: kdb.StreamFilterType, Prefixes: []string{f.StreamPrefix}}
	}
	return &kdb.SubscriptionFilter{Type: kdb.StreamFilterType, Regex: `^[^$].*`}
}

func collect(rs *kdb.ReadStream, filter eventstore.Filter) ([]eventstore.Event, error) {
	var out []eventstore.Event
	for {
		resolved, err := rs.Recv()
		if err != nil {
			if isNotFound(err) {
				return out, nil
			}
			break // io.EOF or end of stream
		}
		rec := resolved.OriginalEvent()
		if rec == nil || strings.HasPrefix(rec.StreamID, "$") {
			continue // skip system events
		}
		if filter.StreamPrefix != "" && !strings.HasPrefix(rec.StreamID, filter.StreamPrefix) {
			continue
		}
		e, err := toEvent(rec)
		if err != nil {
			return nil, err
		}
		out = append(out, e)
	}
	return out, nil
}

func toEvent(rec *kdb.RecordedEvent) (eventstore.Event, error) {
	var env envelope
	if len(rec.UserMetadata) > 0 {
		if err := json.Unmarshal(rec.UserMetadata, &env); err != nil {
			return eventstore.Event{}, fmt.Errorf("kurrentdb: decode metadata: %w", err)
		}
	}
	return eventstore.Event{
		ID:         env.ID,
		Stream:     rec.StreamID,
		Type:       rec.EventType,
		Data:       rec.Data,
		Meta:       env.Meta,
		ValidFrom:  env.ValidFrom,
		RecordedAt: rec.CreatedDate,
		Position:   eventstore.Position(rec.Position.Commit),
		Revision:   eventstore.Revision(rec.EventNumber),
	}, nil
}

func isNotFound(err error) bool {
	if kerr, ok := kdb.FromError(err); ok {
		return kerr.IsErrorCode(kdb.ErrorCodeResourceNotFound)
	}
	return false
}

func max64(a, b eventstore.Revision) eventstore.Revision {
	if a > b {
		return a
	}
	return b
}

// subscription wraps a KurrentDB subscription as an eventstore.Subscription.
type subscription struct {
	events    chan eventstore.Event
	ksub      *kdb.Subscription
	done      chan struct{}
	closeOnce sync.Once

	mu  sync.Mutex
	err error
}

func (sub *subscription) Events() <-chan eventstore.Event { return sub.events }

func (sub *subscription) Err() error {
	sub.mu.Lock()
	defer sub.mu.Unlock()
	return sub.err
}

func (sub *subscription) Close() error {
	sub.closeOnce.Do(func() {
		close(sub.done)
		_ = sub.ksub.Close()
	})
	return nil
}

func (sub *subscription) setErr(err error) {
	if err == nil || errors.Is(err, context.Canceled) {
		return
	}
	sub.mu.Lock()
	sub.err = err
	sub.mu.Unlock()
}

func (sub *subscription) run(ctx context.Context, filter eventstore.Filter) {
	defer close(sub.events)
	for {
		ev := sub.ksub.Recv()
		switch {
		case ev.SubscriptionDropped != nil:
			sub.setErr(ev.SubscriptionDropped.Error)
			return
		case ev.EventAppeared != nil:
			rec := ev.EventAppeared.OriginalEvent()
			if rec == nil || strings.HasPrefix(rec.StreamID, "$") {
				continue
			}
			if filter.StreamPrefix != "" && !strings.HasPrefix(rec.StreamID, filter.StreamPrefix) {
				continue
			}
			e, err := toEvent(rec)
			if err != nil {
				sub.setErr(err)
				return
			}
			select {
			case sub.events <- e:
			case <-ctx.Done():
				sub.setErr(ctx.Err())
				return
			case <-sub.done:
				return
			}
		default:
			// checkpoint / caught-up / fell-behind: ignore
		}
		select {
		case <-sub.done:
			return
		case <-ctx.Done():
			sub.setErr(ctx.Err())
			return
		default:
		}
	}
}
