// Package namespace provides per-project segregation for any
// eventstore.EventStore (architecture R9). It is a single decorator over the
// EventStore port: it prefixes every stream a project writes with the project's
// namespace and scopes every global read and subscription to that namespace, so
// one backend (a shared SQLite file or a shared KurrentDB instance) can hold many
// projects without their streams or graphs ever mixing.
//
// Because it depends only on the port, it is written once and wraps every
// backend; that is dependency inversion buying the single implementation.
package namespace

import (
	"context"
	"strings"
	"sync"

	"github.com/virtual-velocitation/rigger/eventstore"
)

// Store wraps an EventStore so all of its data is scoped to one project. Callers
// use plain, unprefixed stream names and never see the namespace.
type Store struct {
	inner  eventstore.EventStore
	prefix string
}

var _ eventstore.EventStore = (*Store)(nil)

// New scopes inner to the named project.
func New(inner eventstore.EventStore, project string) *Store {
	return &Store{inner: inner, prefix: "proj-" + project + "-"}
}

// Append writes to the project-scoped stream.
func (s *Store) Append(ctx context.Context, stream string, expected eventstore.ExpectedRevision, events ...eventstore.Event) (eventstore.Position, error) {
	return s.inner.Append(ctx, s.prefix+stream, expected, events...)
}

// ReadStream reads one project-scoped stream, returning clean stream names.
func (s *Store) ReadStream(ctx context.Context, stream string, from eventstore.Revision, dir eventstore.Direction) ([]eventstore.Event, error) {
	evs, err := s.inner.ReadStream(ctx, s.prefix+stream, from, dir)
	return s.strip(evs), err
}

// ReadAll reads only this project's events, in global order.
func (s *Store) ReadAll(ctx context.Context, from eventstore.Position, dir eventstore.Direction, filter eventstore.Filter) ([]eventstore.Event, error) {
	evs, err := s.inner.ReadAll(ctx, from, dir, s.scopeFilter(filter))
	return s.strip(evs), err
}

// SubscribeAll subscribes to only this project's events.
func (s *Store) SubscribeAll(ctx context.Context, from eventstore.Position, filter eventstore.Filter) (eventstore.Subscription, error) {
	inner, err := s.inner.SubscribeAll(ctx, from, s.scopeFilter(filter))
	if err != nil {
		return nil, err
	}
	return newStrippingSub(inner, s.prefix), nil
}

// Close closes the wrapped store.
func (s *Store) Close() error { return s.inner.Close() }

// scopeFilter forces the project namespace, composing it with any caller prefix
// (interpreted within the namespace).
func (s *Store) scopeFilter(f eventstore.Filter) eventstore.Filter {
	return eventstore.Filter{StreamPrefix: s.prefix + f.StreamPrefix}
}

func (s *Store) strip(evs []eventstore.Event) []eventstore.Event {
	for i := range evs {
		evs[i].Stream = strings.TrimPrefix(evs[i].Stream, s.prefix)
	}
	return evs
}

// strippingSub wraps a subscription to strip the namespace prefix from each
// delivered event.
type strippingSub struct {
	inner     eventstore.Subscription
	prefix    string
	out       chan eventstore.Event
	done      chan struct{}
	closeOnce sync.Once
}

var _ eventstore.Subscription = (*strippingSub)(nil)

func newStrippingSub(inner eventstore.Subscription, prefix string) *strippingSub {
	sub := &strippingSub{
		inner:  inner,
		prefix: prefix,
		out:    make(chan eventstore.Event, 128),
		done:   make(chan struct{}),
	}
	go sub.run()
	return sub
}

func (sub *strippingSub) run() {
	defer close(sub.out)
	in := sub.inner.Events()
	for {
		select {
		case e, ok := <-in:
			if !ok {
				return
			}
			e.Stream = strings.TrimPrefix(e.Stream, sub.prefix)
			select {
			case sub.out <- e:
			case <-sub.done:
				return
			}
		case <-sub.done:
			return
		}
	}
}

func (sub *strippingSub) Events() <-chan eventstore.Event { return sub.out }
func (sub *strippingSub) Err() error                      { return sub.inner.Err() }

func (sub *strippingSub) Close() error {
	sub.closeOnce.Do(func() { close(sub.done) })
	return sub.inner.Close()
}
