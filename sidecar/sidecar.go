// Package sidecar is the live cross-agent awareness mechanism (the architecture's
// (A) loop): while an agent works, a filtered catch-up subscription surfaces the
// decisions other agents make on the same blast-radius, so no agent works blind
// to its peers. It never crosses the file-isolation boundary - worktrees isolate
// the files, the event stream shares the decisions (R5).
package sidecar

import (
	"context"
	"encoding/json"
	"sync"

	"github.com/virtual-velocitation/rigger/contextgraph"
	"github.com/virtual-velocitation/rigger/eventstore"
)

// Sidecar collects events relevant to one agent's blast-radius while it works.
type Sidecar struct {
	sub    eventstore.Subscription
	cancel context.CancelFunc

	mu   sync.Mutex
	seen []eventstore.Event
}

// Start opens a filtered catch-up subscription from a position and begins
// collecting matching events in the background.
func Start(ctx context.Context, store eventstore.EventStore, from eventstore.Position, filter eventstore.Filter) (*Sidecar, error) {
	subCtx, cancel := context.WithCancel(ctx)
	sub, err := store.SubscribeAll(subCtx, from, filter)
	if err != nil {
		cancel()
		return nil, err
	}
	sc := &Sidecar{sub: sub, cancel: cancel}
	go sc.collect()
	return sc, nil
}

func (sc *Sidecar) collect() {
	for e := range sc.sub.Events() {
		sc.mu.Lock()
		sc.seen = append(sc.seen, e)
		sc.mu.Unlock()
	}
}

// Decisions returns a snapshot of the DecisionMade events seen so far: the
// concurrent decisions the agent should be aware of.
func (sc *Sidecar) Decisions() []contextgraph.DecisionMade {
	sc.mu.Lock()
	defer sc.mu.Unlock()
	var out []contextgraph.DecisionMade
	for _, e := range sc.seen {
		if e.Type != contextgraph.TypeDecisionMade {
			continue
		}
		var d contextgraph.DecisionMade
		if json.Unmarshal(e.Data, &d) == nil {
			out = append(out, d)
		}
	}
	return out
}

// Len reports how many events the side-car has collected.
func (sc *Sidecar) Len() int {
	sc.mu.Lock()
	defer sc.mu.Unlock()
	return len(sc.seen)
}

// Close stops the subscription.
func (sc *Sidecar) Close() error {
	sc.cancel()
	return sc.sub.Close()
}
