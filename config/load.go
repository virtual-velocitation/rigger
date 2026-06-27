package config

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"gopkg.in/yaml.v3"
)

// Load reads agent definitions from <dir>/agents/*.md and the workflow from
// <dir>/.rigger/workflow.yml, then validates referential and structural
// integrity.
func Load(dir string) (*Config, error) {
	agents, err := loadAgents(filepath.Join(dir, "agents"))
	if err != nil {
		return nil, err
	}
	wf, err := loadWorkflow(filepath.Join(dir, ".rigger", "workflow.yml"))
	if err != nil {
		return nil, err
	}
	cfg := &Config{Agents: agents, Workflow: wf}
	if err := cfg.Validate(); err != nil {
		return nil, err
	}
	return cfg, nil
}

func loadAgents(dir string) (map[string]AgentDef, error) {
	entries, err := os.ReadDir(dir)
	if err != nil {
		return nil, fmt.Errorf("config: read agents dir: %w", err)
	}
	agents := map[string]AgentDef{}
	for _, e := range entries {
		if e.IsDir() || !strings.HasSuffix(e.Name(), ".md") {
			continue
		}
		b, err := os.ReadFile(filepath.Join(dir, e.Name()))
		if err != nil {
			return nil, fmt.Errorf("config: read %s: %w", e.Name(), err)
		}
		a, err := ParseAgent(b)
		if err != nil {
			return nil, fmt.Errorf("config: %s: %w", e.Name(), err)
		}
		if a.ID == "" {
			return nil, fmt.Errorf("config: %s: agent is missing an id", e.Name())
		}
		if _, dup := agents[a.ID]; dup {
			return nil, fmt.Errorf("config: duplicate agent id %q", a.ID)
		}
		agents[a.ID] = a
	}
	return agents, nil
}

// ParseAgent parses a markdown-with-YAML-frontmatter agent definition: the
// frontmatter is the agent's fields, the body is its prompt.
func ParseAgent(b []byte) (AgentDef, error) {
	front, body, err := splitFrontmatter(b)
	if err != nil {
		return AgentDef{}, err
	}
	var a AgentDef
	if err := yaml.Unmarshal(front, &a); err != nil {
		return AgentDef{}, fmt.Errorf("frontmatter: %w", err)
	}
	a.Prompt = strings.TrimSpace(string(body))
	return a, nil
}

func splitFrontmatter(b []byte) (front, body []byte, err error) {
	s := string(b)
	if !strings.HasPrefix(s, "---") {
		return nil, nil, fmt.Errorf("missing YAML frontmatter (--- delimiters)")
	}
	rest := strings.TrimPrefix(strings.TrimPrefix(s, "---"), "\n")
	idx := strings.Index(rest, "\n---")
	if idx < 0 {
		return nil, nil, fmt.Errorf("unterminated frontmatter (no closing ---)")
	}
	front = []byte(rest[:idx])
	body = []byte(strings.TrimPrefix(rest[idx+len("\n---"):], "\n"))
	return front, body, nil
}

func loadWorkflow(path string) (Workflow, error) {
	b, err := os.ReadFile(path)
	if err != nil {
		return Workflow{}, fmt.Errorf("config: read workflow: %w", err)
	}
	var wf Workflow
	if err := yaml.Unmarshal(b, &wf); err != nil {
		return Workflow{}, fmt.Errorf("config: parse workflow: %w", err)
	}
	for name, st := range wf.Stages {
		st.Name = name
		wf.Stages[name] = st
	}
	return wf, nil
}

// Validate checks that every reference resolves and the stage graph is acyclic.
func (c *Config) Validate() error {
	wf := c.Workflow
	for name, st := range wf.Stages {
		for _, need := range st.Needs {
			if _, ok := wf.Stages[need]; !ok {
				return fmt.Errorf("config: stage %q needs unknown stage %q", name, need)
			}
		}
		for _, aid := range st.AgentIDs() {
			if _, ok := c.Agents[aid]; !ok {
				return fmt.Errorf("config: stage %q references unknown agent %q", name, aid)
			}
		}
		for _, g := range st.Gates {
			if _, ok := wf.Gates[g]; !ok {
				return fmt.Errorf("config: stage %q references unknown gate %q", name, g)
			}
		}
	}
	if cyc := findCycle(wf.Stages); cyc != "" {
		return fmt.Errorf("config: workflow has a dependency cycle involving stage %q", cyc)
	}
	return nil
}

func findCycle(stages map[string]Stage) string {
	const (
		white = iota
		gray
		black
	)
	color := make(map[string]int, len(stages))
	var bad string
	var visit func(string) bool
	visit = func(n string) bool {
		color[n] = gray
		for _, m := range stages[n].Needs {
			switch color[m] {
			case gray:
				bad = m
				return true
			case white:
				if visit(m) {
					return true
				}
			}
		}
		color[n] = black
		return false
	}
	for name := range stages {
		if color[name] == white && visit(name) {
			return bad
		}
	}
	return ""
}
