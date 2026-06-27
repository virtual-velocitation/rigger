package workflow_test

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"sync"
	"testing"

	"github.com/virtual-velocitation/rigger/conductor"
	"github.com/virtual-velocitation/rigger/config"
	"github.com/virtual-velocitation/rigger/driver/workflow"
)

func TestWorkflowDriverBridge(t *testing.T) {
	spawnR, spawnW := io.Pipe() // conductor -> shim
	respR, respW := io.Pipe()   // shim -> conductor
	d := workflow.New(spawnW, respR)

	// Simulate the JS shim: for each spawn request, forward a live emission, then
	// the final result.
	go func() {
		sc := bufio.NewScanner(spawnR)
		w := bufio.NewWriter(respW)
		for sc.Scan() {
			var req struct {
				ID     string `json:"id"`
				Prompt string `json:"prompt"`
			}
			if json.Unmarshal(sc.Bytes(), &req) != nil {
				continue
			}
			_, _ = fmt.Fprintf(w, `{"kind":"emit","spawn":%q,"type":"DecisionMade","data":{"id":"d1"}}`+"\n", req.ID)
			_, _ = fmt.Fprintf(w, `{"kind":"result","spawn":%q,"output":"done: %s"}`+"\n", req.ID, req.Prompt)
			_ = w.Flush()
		}
	}()

	var mu sync.Mutex
	var emitted []string
	emit := func(typ string, _ any) error {
		mu.Lock()
		emitted = append(emitted, typ)
		mu.Unlock()
		return nil
	}

	res, err := d.Spawn(context.Background(), config.AgentDef{ID: "a"}, "review the diff", conductor.SpawnOpts{}, emit)
	if err != nil {
		t.Fatalf("Spawn: %v", err)
	}
	if res.Output != "done: review the diff" {
		t.Errorf("result output = %q, want it to echo the prompt", res.Output)
	}
	mu.Lock()
	got := append([]string(nil), emitted...)
	mu.Unlock()
	if len(got) != 1 || got[0] != "DecisionMade" {
		t.Errorf("expected the agent's live emission to be forwarded; got %v", got)
	}
}
