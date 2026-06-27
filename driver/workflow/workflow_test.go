package workflow_test

import (
	"context"
	"testing"
	"time"

	"github.com/virtual-velocitation/rigger/conductor"
	"github.com/virtual-velocitation/rigger/config"
	"github.com/virtual-velocitation/rigger/driver/workflow"
)

func TestWorkflowDriverBridge(t *testing.T) {
	d := workflow.New()

	// The conductor spawns an agent; Spawn blocks until the shim reports a result.
	done := make(chan conductor.AgentResult, 1)
	go func() {
		res, err := d.Spawn(context.Background(), config.AgentDef{ID: "a", Model: "sonnet"}, "review the diff", conductor.SpawnOpts{}, nil)
		if err != nil {
			t.Errorf("Spawn: %v", err)
		}
		done <- res
	}()

	// The MCP server (driven by the shim) picks up the queued request.
	var req workflow.SpawnRequest
	deadline := time.Now().Add(2 * time.Second)
	for {
		if r, ok := d.Next(); ok {
			req = r
			break
		}
		if time.Now().After(deadline) {
			t.Fatal("no spawn request was queued")
		}
		time.Sleep(time.Millisecond)
	}
	if req.Prompt != "review the diff" || req.Model != "sonnet" {
		t.Errorf("spawn request = %+v", req)
	}

	// The shim runs the agent and reports the result.
	d.Result(req.ID, "done", "")
	if res := <-done; res.Output != "done" {
		t.Errorf("result output = %q, want done", res.Output)
	}
}

func TestWorkflowDriverNextEmptyWhenNoSpawns(t *testing.T) {
	if _, ok := workflow.New().Next(); ok {
		t.Error("Next should report ok=false when nothing is queued")
	}
}
