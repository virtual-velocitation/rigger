// Package cli is the default AgentDriver: it spawns agents by shelling out to
// the `claude` command-line tool, so Rigger depends on no particular editor or
// runtime and works for anyone who `go get`s it (architecture R7). The optional
// workflow driver is the in-Claude-Code alternative.
package cli

import (
	"context"
	"fmt"
	"os/exec"
	"strings"

	"github.com/virtual-velocitation/rigger/conductor"
	"github.com/virtual-velocitation/rigger/config"
)

// Driver spawns agents via the `claude` CLI.
type Driver struct {
	Bin string // the claude binary; defaults to "claude"
}

var _ conductor.AgentDriver = Driver{}

// Spawn runs one agent headlessly and returns its output.
func (d Driver) Spawn(ctx context.Context, agent config.AgentDef, prompt string) (conductor.AgentResult, error) {
	out, err := exec.CommandContext(ctx, d.bin(), BuildArgs(agent, prompt)...).Output()
	res := conductor.AgentResult{Output: string(out)}
	if err != nil {
		return res, fmt.Errorf("cli driver: spawn agent %q: %w", agent.ID, err)
	}
	return res, nil
}

func (d Driver) bin() string {
	if d.Bin != "" {
		return d.Bin
	}
	return "claude"
}

// BuildArgs builds the `claude` headless invocation for an agent and prompt: the
// agent's own instructions plus the task become the prompt, with the model and
// allowed tools the agent declares.
func BuildArgs(agent config.AgentDef, prompt string) []string {
	full := prompt
	if agent.Prompt != "" {
		full = agent.Prompt + "\n\n" + prompt
	}
	args := []string{"-p", full}
	if agent.Model != "" {
		args = append(args, "--model", agent.Model)
	}
	if len(agent.Tools) > 0 {
		args = append(args, "--allowed-tools", strings.Join(agent.Tools, ","))
	}
	return args
}
