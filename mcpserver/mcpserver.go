// Package mcpserver exposes the conductor's workflow bridge over MCP so a Claude
// Code Workflow shim can drive it (architecture (A): thin shim + Go core, via the
// real Workflow tool). rigger_next picks up the next queued agent spawn,
// rigger_result reports an agent's outcome, and rigger_emit records a decision
// live to the event store so other agents see it immediately.
package mcpserver

import (
	"context"
	"encoding/json"
	"fmt"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/virtual-velocitation/rigger/driver/workflow"
	"github.com/virtual-velocitation/rigger/eventstore"
	"github.com/virtual-velocitation/rigger/sidecar"
)

// New builds the rigger MCP server over the given workflow bridge and event store.
// Decisions emitted via rigger_emit are appended to stream.
func New(driver *workflow.Driver, store eventstore.EventStore, stream string, peers *sidecar.Sidecar) *mcp.Server {
	b := &bridge{driver: driver, store: store, stream: stream, peers: peers}
	srv := mcp.NewServer(&mcp.Implementation{Name: "rigger", Version: "0.1.0"}, nil)
	mcp.AddTool(srv, &mcp.Tool{Name: "rigger_next", Description: "Pick up the next queued agent spawn. The id is empty when nothing is waiting."}, b.next)
	mcp.AddTool(srv, &mcp.Tool{Name: "rigger_result", Description: "Report an agent's final result by spawn id."}, b.result)
	mcp.AddTool(srv, &mcp.Tool{Name: "rigger_emit", Description: "Record a decision on the shared event log, live, so other agents see it immediately."}, b.emit)
	mcp.AddTool(srv, &mcp.Tool{Name: "rigger_peers", Description: "List the decisions other agents have made so far this run, so you do not work blind to them. Call it before a significant action."}, b.listPeers)
	return srv
}

type bridge struct {
	driver *workflow.Driver
	store  eventstore.EventStore
	stream string
	peers  *sidecar.Sidecar
}

type empty struct{}

type nextOut struct {
	ID     string   `json:"id" jsonschema:"the spawn id; empty when nothing is queued"`
	Prompt string   `json:"prompt,omitempty"`
	Model  string   `json:"model,omitempty"`
	Tools  []string `json:"tools,omitempty"`
	Dir    string   `json:"dir,omitempty"`
}

func (b *bridge) next(_ context.Context, _ *mcp.CallToolRequest, _ empty) (*mcp.CallToolResult, nextOut, error) {
	req, ok := b.driver.Next()
	if !ok {
		return nil, nextOut{}, nil
	}
	return nil, nextOut{ID: req.ID, Prompt: req.Prompt, Model: req.Model, Tools: req.Tools, Dir: req.Dir}, nil
}

type resultIn struct {
	ID     string `json:"id" jsonschema:"the spawn id from rigger_next"`
	Output string `json:"output,omitempty"`
	Error  string `json:"error,omitempty"`
}

func (b *bridge) result(_ context.Context, _ *mcp.CallToolRequest, in resultIn) (*mcp.CallToolResult, empty, error) {
	b.driver.Result(in.ID, in.Output, in.Error)
	return nil, empty{}, nil
}

type emitIn struct {
	Type string         `json:"type" jsonschema:"the event type, e.g. DecisionMade"`
	Data map[string]any `json:"data" jsonschema:"the decision payload"`
}

func (b *bridge) emit(ctx context.Context, _ *mcp.CallToolRequest, in emitIn) (*mcp.CallToolResult, empty, error) {
	data, err := json.Marshal(in.Data)
	if err != nil {
		return nil, empty{}, fmt.Errorf("mcpserver: encode emit data: %w", err)
	}
	if _, err := b.store.Append(ctx, b.stream, eventstore.Any, eventstore.Event{Type: in.Type, Data: data}); err != nil {
		return nil, empty{}, fmt.Errorf("mcpserver: append emitted event: %w", err)
	}
	return nil, empty{}, nil
}

type peerDecision struct {
	ID      string   `json:"id"`
	Summary string   `json:"summary"`
	Governs []string `json:"governs,omitempty"`
}

type peersOut struct {
	Decisions []peerDecision `json:"decisions"`
}

func (b *bridge) listPeers(_ context.Context, _ *mcp.CallToolRequest, _ empty) (*mcp.CallToolResult, peersOut, error) {
	var out peersOut
	if b.peers != nil {
		for _, d := range b.peers.Decisions() {
			out.Decisions = append(out.Decisions, peerDecision{ID: d.ID, Summary: d.Summary, Governs: d.Governs})
		}
	}
	return nil, out, nil
}
