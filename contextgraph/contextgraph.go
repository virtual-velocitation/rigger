// Package contextgraph defines the bi-temporal context graph: the read model
// projected from the event log that answers relationship questions vector search
// cannot ("what decisions govern this file? who else touched these nodes?").
//
// This package is the domain plus the port. Nodes and edges are the model;
// Projection is the port an adapter (contextgraph/sqlite) implements. The graph
// is always a local, per-project read model rebuilt from the log (architecture
// R2, R9), so it depends only on the eventstore domain, never on a backend.
package contextgraph

import (
	"context"
	"time"

	"github.com/virtual-velocitation/rigger/eventstore"
)

// Node kinds. The vocabulary is Rigger's own (general), never a consuming
// project's domain.
const (
	KindDecision = "decision"
	KindArtifact = "artifact"
	KindAgent    = "agent"
	KindGate     = "gate"
	KindUnit     = "unit"
)

// Edge relationships.
const (
	RelDecided    = "DECIDED"
	RelSupersedes = "SUPERSEDES"
	RelTouches    = "TOUCHES"
	RelGoverns    = "GOVERNS"
	RelGatedBy    = "GATED_BY"
)

// Node is an entity in the graph: a decision, artifact, agent, gate, or unit.
type Node struct {
	ID    string
	Kind  string
	Attrs map[string]string
}

// Edge is a typed, bi-temporal relationship between two nodes. ValidTo == nil
// means the edge currently holds; a non-nil ValidTo means it was invalidated
// (superseded) at that time and is never deleted (architecture R3). Source is
// the event position that asserted the edge (provenance).
type Edge struct {
	From      string
	To        string
	Rel       string
	ValidFrom time.Time
	ValidTo   *time.Time
	Source    eventstore.Position
}

// Graph is a set of nodes and the edges among them, e.g. the result of a
// Subgraph query.
type Graph struct {
	Nodes []Node
	Edges []Edge
}

// Projection is the context-graph read model. Apply folds one event into the
// graph; Subgraph and Resolve query it. Implementations maintain only currently
// valid edges in query results unless asked otherwise.
type Projection interface {
	// Apply folds a single event into the graph. It is idempotent per event
	// position so a replay rebuilds the same graph.
	Apply(ctx context.Context, e eventstore.Event) error

	// Subgraph returns the connected subgraph reachable from any seed node
	// within depth hops, following only currently valid edges. This is the
	// FEED arc: an agent's blast-radius context.
	Subgraph(ctx context.Context, seed []string, depth int) (Graph, error)

	// Resolve maps a mention (an alias or a node id) to a canonical node id,
	// collapsing synonyms onto one node (entity resolution).
	Resolve(ctx context.Context, mention string) (canonicalID string, ok bool, err error)
}

// --- Rigger's graph-relevant event payloads (the Data of an eventstore.Event) ---

// Event type discriminators carried in eventstore.Event.Type.
const (
	TypeDecisionMade   = "DecisionMade"
	TypeFileTouched    = "FileTouched"
	TypeGateVerdict    = "GateVerdict"
	TypeUnitIntegrated = "UnitIntegrated"
)

// DecisionMade records a decision an agent made, what artifacts it governs, and
// whether it supersedes a prior decision (which invalidates that decision's
// governing edges).
type DecisionMade struct {
	ID         string   `json:"id"`
	Summary    string   `json:"summary"`
	Governs    []string `json:"governs,omitempty"`
	Supersedes string   `json:"supersedes,omitempty"`
}

// FileTouched records that an agent touched an artifact.
type FileTouched struct {
	Path string `json:"path"`
	By   string `json:"by,omitempty"`
}

// GateVerdict records a gate's pass/fail, optionally against an artifact.
type GateVerdict struct {
	Gate     string `json:"gate"`
	Pass     bool   `json:"pass"`
	Artifact string `json:"artifact,omitempty"`
}

// UnitIntegrated records that a unit of work landed.
type UnitIntegrated struct {
	Unit   string `json:"unit"`
	Commit string `json:"commit,omitempty"`
}
