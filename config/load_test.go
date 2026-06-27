package config_test

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/virtual-velocitation/rigger/config"
)

func TestParseAgent(t *testing.T) {
	src := "---\n" +
		"id: implementer\n" +
		"model: sonnet\n" +
		"tools: [Read, Edit, Bash]\n" +
		"isolation: worktree\n" +
		"recurse: false\n" +
		"---\n" +
		"You implement one finding.\nWrite the failing test first.\n"
	a, err := config.ParseAgent([]byte(src))
	if err != nil {
		t.Fatalf("ParseAgent: %v", err)
	}
	if a.ID != "implementer" || a.Model != "sonnet" || a.Isolation != "worktree" || a.Recurse {
		t.Errorf("fields wrong: %+v", a)
	}
	if len(a.Tools) != 3 || a.Tools[1] != "Edit" {
		t.Errorf("tools: %v", a.Tools)
	}
	if !strings.HasPrefix(a.Prompt, "You implement one finding.") {
		t.Errorf("prompt body: %q", a.Prompt)
	}
}

func TestParseAgentMissingFrontmatter(t *testing.T) {
	if _, err := config.ParseAgent([]byte("no frontmatter here")); err == nil {
		t.Fatal("expected an error for missing frontmatter")
	}
}

func TestLoadAndValidate(t *testing.T) {
	dir := writeProject(t, map[string]string{
		".rigger/agents/planner.md":  agentFile("planner", "sonnet"),
		".rigger/agents/impl.md":     agentFile("impl", "sonnet"),
		".rigger/agents/reviewer.md": agentFile("reviewer", "sonnet"),
		".rigger/agents/da.md":       agentFile("da", "opus"),
		".rigger/workflow.yml": "name: produce\n" +
			"gates:\n" +
			"  test: { run: \"go test ./...\", kind: core }\n" +
			"stages:\n" +
			"  plan:\n" +
			"    agent: planner\n" +
			"  implement:\n" +
			"    needs: [plan]\n" +
			"    agent: impl\n" +
			"    strategy: fan-out\n" +
			"    gates: [test]\n" +
			"  review:\n" +
			"    needs: [implement]\n" +
			"    agents: [reviewer]\n" +
			"    adjudicator: da\n",
	})
	cfg, err := config.Load(dir)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	if len(cfg.Agents) != 4 {
		t.Errorf("agents: %d, want 4", len(cfg.Agents))
	}
	if len(cfg.Workflow.Stages) != 3 {
		t.Errorf("stages: %d, want 3", len(cfg.Workflow.Stages))
	}
	if got := cfg.Workflow.Stages["implement"]; got.Name != "implement" || got.Strategy != "fan-out" {
		t.Errorf("stage not populated: %+v", got)
	}
}

func TestValidateErrors(t *testing.T) {
	cases := map[string]string{
		"unknown agent": "stages: { go: { agent: ghost } }",
		"unknown need":  "stages: { a: { agent: planner, needs: [nope] } }",
		"unknown gate":  "stages: { a: { agent: planner, gates: [nogate] } }",
		"cycle":         "stages: { a: { agent: planner, needs: [b] }, b: { agent: planner, needs: [a] } }",
	}
	for name, wf := range cases {
		t.Run(name, func(t *testing.T) {
			dir := writeProject(t, map[string]string{
				".rigger/agents/planner.md": agentFile("planner", "sonnet"),
				".rigger/workflow.yml":      "name: x\n" + wf + "\n",
			})
			if _, err := config.Load(dir); err == nil {
				t.Fatalf("expected a validation error for %q", name)
			}
		})
	}
}

func agentFile(id, model string) string {
	return "---\nid: " + id + "\nmodel: " + model + "\n---\nPrompt for " + id + ".\n"
}

func writeProject(t *testing.T, files map[string]string) string {
	t.Helper()
	dir := t.TempDir()
	for rel, content := range files {
		p := filepath.Join(dir, rel)
		if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(p, []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	return dir
}
