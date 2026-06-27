// Package ledger is the conductor's durable run state, projected from the event
// log (architecture R2): it is rebuildable by replaying the log, so a crashed or
// resumed run continues from the truth rather than from conversation. The
// conductor reads it to decide what is ready and whether the run is done.
package ledger

import (
	"encoding/json"
	"fmt"

	"github.com/virtual-velocitation/rigger/eventstore"
)

// Status of a unit of work.
type Status string

const (
	Pending    Status = "pending"
	Running    Status = "running"
	Integrated Status = "integrated"
	Failed     Status = "failed"
	Escalated  Status = "escalated"
)

// Unit is one unit of work in the run.
type Unit struct {
	ID            string
	SpecCriterion string
	Status        Status
	Attempts      int
	Commit        string
}

// RunState is the projected run state.
type RunState struct {
	Units map[string]*Unit
}

// NewRunState returns an empty run state.
func NewRunState() *RunState { return &RunState{Units: map[string]*Unit{}} }

// Run-event types the conductor emits (folded here into run state).
const (
	TypeUnitStarted    = "UnitStarted"
	TypeUnitFailed     = "UnitFailed"
	TypeUnitEscalated  = "UnitEscalated"
	TypeUnitIntegrated = "UnitIntegrated"
)

// UnitStarted marks a unit as begun.
type UnitStarted struct {
	ID            string `json:"id"`
	SpecCriterion string `json:"spec_criterion,omitempty"`
}

// UnitFailed records a failed attempt.
type UnitFailed struct {
	ID       string `json:"id"`
	Attempts int    `json:"attempts"`
}

// UnitEscalated marks a unit as handed to a human.
type UnitEscalated struct {
	ID string `json:"id"`
}

// UnitIntegrated marks a unit as landed.
type UnitIntegrated struct {
	ID     string `json:"id"`
	Commit string `json:"commit,omitempty"`
}

// Apply folds one run event into the state. Unknown event types are ignored, so
// the same log feeds both this projection and the context graph.
func (r *RunState) Apply(e eventstore.Event) error {
	switch e.Type {
	case TypeUnitStarted:
		var p UnitStarted
		if err := json.Unmarshal(e.Data, &p); err != nil {
			return fmt.Errorf("ledger: UnitStarted: %w", err)
		}
		u := r.unit(p.ID)
		u.SpecCriterion = p.SpecCriterion
		u.Status = Running
	case TypeUnitFailed:
		var p UnitFailed
		if err := json.Unmarshal(e.Data, &p); err != nil {
			return fmt.Errorf("ledger: UnitFailed: %w", err)
		}
		u := r.unit(p.ID)
		u.Status = Failed
		u.Attempts = p.Attempts
	case TypeUnitEscalated:
		var p UnitEscalated
		if err := json.Unmarshal(e.Data, &p); err != nil {
			return fmt.Errorf("ledger: UnitEscalated: %w", err)
		}
		r.unit(p.ID).Status = Escalated
	case TypeUnitIntegrated:
		var p UnitIntegrated
		if err := json.Unmarshal(e.Data, &p); err != nil {
			return fmt.Errorf("ledger: UnitIntegrated: %w", err)
		}
		u := r.unit(p.ID)
		u.Status = Integrated
		u.Commit = p.Commit
	}
	return nil
}

func (r *RunState) unit(id string) *Unit {
	u, ok := r.Units[id]
	if !ok {
		u = &Unit{ID: id, Status: Pending}
		r.Units[id] = u
	}
	return u
}

// Done reports whether the run is complete: at least one unit, and every unit
// integrated.
func (r *RunState) Done() bool {
	if len(r.Units) == 0 {
		return false
	}
	for _, u := range r.Units {
		if u.Status != Integrated {
			return false
		}
	}
	return true
}

// Project rebuilds run state from an ordered slice of events.
func Project(events []eventstore.Event) (*RunState, error) {
	r := NewRunState()
	for _, e := range events {
		if err := r.Apply(e); err != nil {
			return nil, err
		}
	}
	return r, nil
}
