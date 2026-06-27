// Package workflow is the in-Claude-Code agent driver for the (B) model: the real
// Workflow tool plus an MCP bridge. The Go conductor stays the orchestrator; its
// Spawn calls enqueue spawn requests here, and an MCP server (in the same process)
// drains them: the Workflow shim calls rigger_next to pick up a request, runs the
// agent in-process via the Workflow tool's agent(), and calls rigger_result when
// it finishes. Agents emit decisions live by calling the MCP rigger_emit tool,
// which appends straight to the event store (handled by the server, not here).
package workflow

import (
	"context"
	"errors"
	"strconv"
	"sync"

	"github.com/virtual-velocitation/rigger/conductor"
	"github.com/virtual-velocitation/rigger/config"
)

// Driver bridges the conductor to a polling MCP server.
type Driver struct {
	mu      sync.Mutex
	queue   []string // ids of undispatched spawns, in order
	pending map[string]*call
	nextID  int64
}

type call struct {
	req    SpawnRequest
	result chan result
}

type result struct {
	output string
	err    error
}

// SpawnRequest is what the shim picks up via rigger_next.
type SpawnRequest struct {
	ID     string   `json:"id"`
	Prompt string   `json:"prompt"`
	Model  string   `json:"model,omitempty"`
	Tools  []string `json:"tools,omitempty"`
	Dir    string   `json:"dir,omitempty"`
}

var _ conductor.AgentDriver = (*Driver)(nil)

// New returns an empty driver.
func New() *Driver { return &Driver{pending: map[string]*call{}} }

// Spawn enqueues a spawn request and blocks until the shim reports its result.
// Agents emit decisions live via the MCP rigger_emit tool, so the emit callback
// is unused here.
func (d *Driver) Spawn(ctx context.Context, agent config.AgentDef, prompt string, opts conductor.SpawnOpts, _ func(eventType string, data any) error) (conductor.AgentResult, error) {
	d.mu.Lock()
	d.nextID++
	id := strconv.FormatInt(d.nextID, 10)
	c := &call{
		req:    SpawnRequest{ID: id, Prompt: prompt, Model: agent.Model, Tools: agent.Tools, Dir: opts.Dir},
		result: make(chan result, 1),
	}
	d.pending[id] = c
	d.queue = append(d.queue, id)
	d.mu.Unlock()

	defer func() {
		d.mu.Lock()
		delete(d.pending, id)
		d.mu.Unlock()
	}()
	select {
	case r := <-c.result:
		return conductor.AgentResult{Output: r.output}, r.err
	case <-ctx.Done():
		return conductor.AgentResult{}, ctx.Err()
	}
}

// Next returns the next queued spawn request for the shim, or ok=false if none is
// waiting. The MCP server exposes this as rigger_next.
func (d *Driver) Next() (SpawnRequest, bool) {
	d.mu.Lock()
	defer d.mu.Unlock()
	for len(d.queue) > 0 {
		id := d.queue[0]
		d.queue = d.queue[1:]
		if c, ok := d.pending[id]; ok {
			return c.req, true
		}
	}
	return SpawnRequest{}, false
}

// Result delivers an agent's result to the waiting Spawn. The MCP server exposes
// this as rigger_result. A blank errStr means success.
func (d *Driver) Result(id, output, errStr string) {
	d.mu.Lock()
	c := d.pending[id]
	d.mu.Unlock()
	if c == nil {
		return
	}
	var err error
	if errStr != "" {
		err = errors.New(errStr)
	}
	c.result <- result{output: output, err: err}
}
