package cli_test

import (
	"slices"
	"strings"
	"testing"

	"github.com/virtual-velocitation/rigger/config"
	"github.com/virtual-velocitation/rigger/driver/cli"
)

func TestBuildArgs(t *testing.T) {
	args := cli.BuildArgs(
		config.AgentDef{ID: "impl", Model: "sonnet", Tools: []string{"Read", "Bash"}, Prompt: "You implement findings."},
		"do the thing",
	)
	if i := slices.Index(args, "-p"); i < 0 || !strings.Contains(args[i+1], "You implement findings.") || !strings.Contains(args[i+1], "do the thing") {
		t.Errorf("prompt should combine persona + task: %v", args)
	}
	if i := slices.Index(args, "--model"); i < 0 || args[i+1] != "sonnet" {
		t.Errorf("model arg missing: %v", args)
	}
	if i := slices.Index(args, "--allowed-tools"); i < 0 || args[i+1] != "Read,Bash" {
		t.Errorf("tools arg wrong: %v", args)
	}
}

func TestBuildArgsMinimal(t *testing.T) {
	args := cli.BuildArgs(config.AgentDef{ID: "bare"}, "task")
	if len(args) != 2 || args[0] != "-p" || args[1] != "task" {
		t.Errorf("a bare agent should yield just the prompt: %v", args)
	}
}
