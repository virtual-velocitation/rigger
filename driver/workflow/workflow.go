// Package workflow is the in-Claude-Code agent driver: a thin bridge to a JS
// Workflow shim that runs agents in-process via the Workflow tool's agent(). The
// Go conductor stays the orchestrator (architecture (A): thin JS shim + Go core);
// this driver writes a spawn request to the shim and reads back the agent's live
// emissions and final result over a line-delimited JSON protocol, forwarding each
// emission to the conductor's emit callback as it arrives.
package workflow

import (
	"bufio"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"strconv"
	"sync"
	"sync/atomic"

	"github.com/virtual-velocitation/rigger/conductor"
	"github.com/virtual-velocitation/rigger/config"
)

// Driver bridges the conductor to a JS Workflow shim over two streams.
type Driver struct {
	out   *bufio.Writer
	outMu sync.Mutex

	mu      sync.Mutex
	pending map[string]*call
	nextID  atomic.Int64
}

type call struct {
	emit   func(eventType string, data any) error
	result chan result
}

type result struct {
	output string
	err    error
}

var _ conductor.AgentDriver = (*Driver)(nil)

// New returns a driver that writes spawn requests to out and reads the shim's
// responses from in.
func New(out io.Writer, in io.Reader) *Driver {
	d := &Driver{out: bufio.NewWriter(out), pending: map[string]*call{}}
	go d.read(in)
	return d
}

type spawnReq struct {
	Kind   string   `json:"kind"`
	ID     string   `json:"id"`
	Prompt string   `json:"prompt"`
	Model  string   `json:"model,omitempty"`
	Tools  []string `json:"tools,omitempty"`
	Dir    string   `json:"dir,omitempty"`
}

type inMsg struct {
	Kind   string          `json:"kind"` // "emit" | "result"
	Spawn  string          `json:"spawn"`
	Type   string          `json:"type"`
	Data   json.RawMessage `json:"data"`
	Output string          `json:"output"`
	Error  string          `json:"error"`
}

// Spawn asks the shim to run the agent and blocks until it finishes, forwarding
// the agent's live emissions to emit as they arrive.
func (d *Driver) Spawn(ctx context.Context, agent config.AgentDef, prompt string, opts conductor.SpawnOpts, emit func(eventType string, data any) error) (conductor.AgentResult, error) {
	id := strconv.FormatInt(d.nextID.Add(1), 10)
	c := &call{emit: emit, result: make(chan result, 1)}
	d.mu.Lock()
	d.pending[id] = c
	d.mu.Unlock()
	defer func() {
		d.mu.Lock()
		delete(d.pending, id)
		d.mu.Unlock()
	}()

	if err := d.write(spawnReq{Kind: "spawn", ID: id, Prompt: prompt, Model: agent.Model, Tools: agent.Tools, Dir: opts.Dir}); err != nil {
		return conductor.AgentResult{}, fmt.Errorf("workflow driver: send spawn: %w", err)
	}
	select {
	case r := <-c.result:
		return conductor.AgentResult{Output: r.output}, r.err
	case <-ctx.Done():
		return conductor.AgentResult{}, ctx.Err()
	}
}

func (d *Driver) read(in io.Reader) {
	scanner := bufio.NewScanner(in)
	scanner.Buffer(make([]byte, 0, 64*1024), 4*1024*1024)
	for scanner.Scan() {
		var msg inMsg
		if json.Unmarshal(scanner.Bytes(), &msg) != nil {
			continue
		}
		d.mu.Lock()
		c := d.pending[msg.Spawn]
		d.mu.Unlock()
		if c == nil {
			continue
		}
		switch msg.Kind {
		case "emit":
			_ = c.emit(msg.Type, msg.Data)
		case "result":
			var err error
			if msg.Error != "" {
				err = errors.New(msg.Error)
			}
			c.result <- result{output: msg.Output, err: err}
		}
	}
}

func (d *Driver) write(v any) error {
	b, err := json.Marshal(v)
	if err != nil {
		return err
	}
	d.outMu.Lock()
	defer d.outMu.Unlock()
	if _, err := d.out.Write(append(b, '\n')); err != nil {
		return err
	}
	return d.out.Flush()
}
