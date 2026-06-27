// Package config is the declarative surface of Rigger: agent definition files
// and a GitHub-Actions-style workflow YAML, loaded and validated into runtime
// types. It is how a project configures the loop without recompiling the binary
// (architecture R1). These are plain value types plus a file loader; nothing
// here depends on the conductor or any adapter.
package config

// AgentDef is one agent, declared in an agents/<id>.md file: YAML frontmatter
// plus a markdown prompt body.
type AgentDef struct {
	ID        string   `yaml:"id"`
	Model     string   `yaml:"model"`
	Tools     []string `yaml:"tools"`
	Isolation string   `yaml:"isolation"` // none | worktree
	Recurse   bool     `yaml:"recurse"`
	Prompt    string   `yaml:"-"` // the markdown body, filled by ParseAgent
}

// Gate is a verification command plus how much it is trusted.
type Gate struct {
	Run  string `yaml:"run"`
	Kind string `yaml:"kind"` // core | elevated | deferred
}

// Defaults are workflow-wide fallbacks for stages that do not set their own.
type Defaults struct {
	Autonomy string `yaml:"autonomy"` // manual | auto_notify | silent
	Grounder string `yaml:"grounder"`
}

// Stage is one node of the workflow DAG.
type Stage struct {
	Name        string   `yaml:"-"` // set from the stages map key
	Agent       string   `yaml:"agent"`
	Agents      []string `yaml:"agents"`
	Needs       []string `yaml:"needs"`
	Strategy    string   `yaml:"strategy"` // "" (single) | fan-out
	Partition   string   `yaml:"partition"`
	Gates       []string `yaml:"gates"`
	Adjudicator string   `yaml:"adjudicator"`
	Autonomy    string   `yaml:"autonomy"`
	Produces    string   `yaml:"produces"`
	Coverage    string   `yaml:"coverage"`
	OnPass      string   `yaml:"on_pass"`
}

// AgentIDs returns every agent a stage references (the worker, the fan-out lens
// set, and the adjudicator).
func (s Stage) AgentIDs() []string {
	var ids []string
	if s.Agent != "" {
		ids = append(ids, s.Agent)
	}
	ids = append(ids, s.Agents...)
	if s.Adjudicator != "" {
		ids = append(ids, s.Adjudicator)
	}
	return ids
}

// Workflow is the declarative loop: a DAG of stages, a reusable gate library,
// and defaults.
type Workflow struct {
	Name     string           `yaml:"name"`
	Defaults Defaults         `yaml:"defaults"`
	Gates    map[string]Gate  `yaml:"gates"`
	Stages   map[string]Stage `yaml:"stages"`
}

// Config is a fully loaded, validated harness configuration.
type Config struct {
	Agents   map[string]AgentDef
	Workflow Workflow
}
