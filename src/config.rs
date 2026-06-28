//! The declarative surface of Rigger: agent definition files (markdown with YAML
//! frontmatter) and a workflow YAML, loaded and validated into runtime types.
//! Plain value types plus a loader; nothing here depends on the conductor.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
#[error("config: {0}")]
pub struct Error(pub String);

fn err(msg: impl Into<String>) -> Error {
    Error(msg.into())
}

/// AgentDef is one agent, declared in a .rigger/agents/<id>.md file: YAML
/// frontmatter plus a markdown prompt body.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct AgentDef {
    pub id: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub isolation: String,
    #[serde(default)]
    pub recurse: bool,
    #[serde(skip)]
    pub prompt: String,
}

/// Gate is a verification command plus how much it is trusted.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Gate {
    #[serde(default)]
    pub run: String,
    #[serde(default)]
    pub kind: String,
}

/// Defaults are workflow-wide fallbacks for stages that do not set their own.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Defaults {
    #[serde(default)]
    pub autonomy: String,
    #[serde(default)]
    pub grounder: String,
    /// The token/spawn circuit-breaker budget (§4.4, §8): the maximum number of
    /// agent spawns a run may perform. 0 (the default) means unlimited.
    #[serde(default)]
    pub budget: u32,
    /// The default partition strategy applied to every wave (§3.2, §8); a stage's
    /// own `partition` overrides it. `by-blast-radius` makes each wave's ready
    /// stages disjoint by blast-radius before they run; empty (the default) leaves
    /// the wave un-partitioned.
    #[serde(default)]
    pub partition: String,
}

/// Stage is one node of the workflow DAG.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Stage {
    #[serde(skip)]
    pub name: String,
    #[serde(default)]
    pub agent: String,
    #[serde(default)]
    pub agents: Vec<String>,
    #[serde(default)]
    pub needs: Vec<String>,
    #[serde(default)]
    pub strategy: String,
    #[serde(default)]
    pub partition: String,
    #[serde(default)]
    pub gates: Vec<String>,
    /// The adversary reviews the lenses' findings and the diff and tries to prove
    /// the lenses wrong - it holds them to a higher bar, surfaces what they all
    /// missed, and refutes overreach. It runs AFTER the lenses and BEFORE the
    /// adjudicator (§3.2); it reviews the reviews, it is not a parallel lens, and
    /// it does not render the final verdict.
    #[serde(default)]
    pub adversary: String,
    #[serde(default)]
    pub adjudicator: String,
    #[serde(default)]
    pub autonomy: String,
    #[serde(default)]
    pub produces: String,
    #[serde(default)]
    pub coverage: String,
    #[serde(default)]
    pub on_pass: String,
}

impl AgentDef {
    /// Whether this agent runs in an isolated git worktree when a repo is set:
    /// `isolation: none` opts out (run in the current dir even with a repo);
    /// empty or `worktree` opts in (§3.1, §6).
    pub fn isolated(&self) -> bool {
        !self.isolation.eq_ignore_ascii_case("none")
    }

    /// The tools this agent is actually granted. When `recurse` is false (the
    /// runaway-proof default), any fan-out capability - an "Agent" or "Task" tool,
    /// case-insensitive - is stripped, so the agent cannot spawn sub-agents (§3.1,
    /// §6). When `recurse` is true the declared tools pass through unchanged.
    pub fn allowed_tools(&self) -> Vec<String> {
        if self.recurse {
            return self.tools.clone();
        }
        self.tools
            .iter()
            .filter(|t| !is_fan_out_tool(t))
            .cloned()
            .collect()
    }
}

/// Whether a tool name grants fan-out (the ability to spawn sub-agents), matched
/// case-insensitively against the canonical spawn tools.
fn is_fan_out_tool(tool: &str) -> bool {
    tool.eq_ignore_ascii_case("Agent") || tool.eq_ignore_ascii_case("Task")
}

impl Stage {
    /// Every agent a stage references (the worker, the fan-out lens set, the
    /// adversary, the adjudicator).
    pub fn agent_ids(&self) -> Vec<String> {
        let mut ids = Vec::new();
        if !self.agent.is_empty() {
            ids.push(self.agent.clone());
        }
        ids.extend(self.agents.iter().cloned());
        if !self.adversary.is_empty() {
            ids.push(self.adversary.clone());
        }
        if !self.adjudicator.is_empty() {
            ids.push(self.adjudicator.clone());
        }
        ids
    }
}

/// Workflow is the declarative loop: a DAG of stages, a gate library, and defaults.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Workflow {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub defaults: Defaults,
    #[serde(default)]
    pub gates: BTreeMap<String, Gate>,
    #[serde(default)]
    pub stages: BTreeMap<String, Stage>,
}

/// Config is a fully loaded, validated harness configuration.
#[derive(Clone, Debug, Default)]
pub struct Config {
    pub agents: BTreeMap<String, AgentDef>,
    pub workflow: Workflow,
}

/// Load reads agent definitions from <dir>/.rigger/agents/*.md and the workflow
/// from <dir>/.rigger/workflow.yml, then validates referential and structural
/// integrity.
pub fn load(dir: &str) -> Result<Config, Error> {
    let base = Path::new(dir).join(".rigger");
    let agents = load_agents(&base.join("agents"))?;
    let workflow = load_workflow(&base.join("workflow.yml"))?;
    let cfg = Config { agents, workflow };
    cfg.validate()?;
    Ok(cfg)
}

fn load_agents(dir: &Path) -> Result<BTreeMap<String, AgentDef>, Error> {
    let mut agents = BTreeMap::new();
    let entries = std::fs::read_dir(dir).map_err(|e| err(format!("read agents dir: {e}")))?;
    for entry in entries {
        let path = entry.map_err(|e| err(e.to_string()))?.path();
        if path.extension().and_then(|x| x.to_str()) != Some("md") {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|x| x.to_str())
            .unwrap_or("?")
            .to_string();
        let b = std::fs::read(&path).map_err(|e| err(format!("read {name}: {e}")))?;
        let a = parse_agent(&b).map_err(|e| err(format!("{name}: {e}")))?;
        if a.id.is_empty() {
            return Err(err(format!("{name}: agent is missing an id")));
        }
        if agents.contains_key(&a.id) {
            return Err(err(format!("duplicate agent id {:?}", a.id)));
        }
        agents.insert(a.id.clone(), a);
    }
    Ok(agents)
}

/// ParseAgent parses a markdown-with-YAML-frontmatter agent definition: the
/// frontmatter is the agent's fields, the body is its prompt.
pub fn parse_agent(b: &[u8]) -> Result<AgentDef, Error> {
    let s = std::str::from_utf8(b).map_err(|e| err(e.to_string()))?;
    let (front, body) = split_frontmatter(s)?;
    let mut a: AgentDef =
        serde_yaml::from_str(front).map_err(|e| err(format!("frontmatter: {e}")))?;
    a.prompt = body.trim().to_string();
    Ok(a)
}

fn split_frontmatter(s: &str) -> Result<(&str, &str), Error> {
    let rest = s
        .strip_prefix("---")
        .ok_or_else(|| err("missing YAML frontmatter (--- delimiters)"))?;
    let rest = rest.strip_prefix('\n').unwrap_or(rest);
    let idx = rest
        .find("\n---")
        .ok_or_else(|| err("unterminated frontmatter (no closing ---)"))?;
    let front = &rest[..idx];
    let after = &rest[idx + "\n---".len()..];
    let body = after.strip_prefix('\n').unwrap_or(after);
    Ok((front, body))
}

fn load_workflow(path: &Path) -> Result<Workflow, Error> {
    let b = std::fs::read_to_string(path).map_err(|e| err(format!("read workflow: {e}")))?;
    let mut wf: Workflow =
        serde_yaml::from_str(&b).map_err(|e| err(format!("parse workflow: {e}")))?;
    let names: Vec<String> = wf.stages.keys().cloned().collect();
    for name in names {
        if let Some(st) = wf.stages.get_mut(&name) {
            st.name = name;
        }
    }
    Ok(wf)
}

impl Config {
    /// Validate checks that every reference resolves and the stage graph is acyclic.
    pub fn validate(&self) -> Result<(), Error> {
        let wf = &self.workflow;
        for (name, st) in &wf.stages {
            for need in &st.needs {
                if !wf.stages.contains_key(need) {
                    return Err(err(format!("stage {name:?} needs unknown stage {need:?}")));
                }
            }
            for aid in st.agent_ids() {
                if !self.agents.contains_key(&aid) {
                    return Err(err(format!(
                        "stage {name:?} references unknown agent {aid:?}"
                    )));
                }
            }
            for g in &st.gates {
                if !wf.gates.contains_key(g) {
                    return Err(err(format!("stage {name:?} references unknown gate {g:?}")));
                }
            }
        }
        if let Some(cyc) = find_cycle(&wf.stages) {
            return Err(err(format!(
                "workflow has a dependency cycle involving stage {cyc:?}"
            )));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Color {
    White,
    Gray,
    Black,
}

fn find_cycle(stages: &BTreeMap<String, Stage>) -> Option<String> {
    fn visit(
        n: &str,
        stages: &BTreeMap<String, Stage>,
        color: &mut HashMap<String, Color>,
    ) -> Option<String> {
        color.insert(n.to_string(), Color::Gray);
        if let Some(st) = stages.get(n) {
            for m in &st.needs {
                match color.get(m).copied().unwrap_or(Color::White) {
                    Color::Gray => return Some(m.clone()),
                    Color::White => {
                        if let Some(bad) = visit(m, stages, color) {
                            return Some(bad);
                        }
                    }
                    Color::Black => {}
                }
            }
        }
        color.insert(n.to_string(), Color::Black);
        None
    }

    let mut color: HashMap<String, Color> = HashMap::new();
    for name in stages.keys() {
        if color.get(name).copied().unwrap_or(Color::White) == Color::White {
            if let Some(bad) = visit(name, stages, &mut color) {
                return Some(bad);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_agent_frontmatter_and_body() {
        let b = b"---\nid: builder\nmodel: sonnet\ntools: [Read, Edit]\n---\nYou are a builder.\n";
        let a = parse_agent(b).unwrap();
        assert_eq!(a.id, "builder");
        assert_eq!(a.model, "sonnet");
        assert_eq!(a.tools, ["Read", "Edit"]);
        assert_eq!(a.prompt, "You are a builder.");
    }

    #[test]
    fn rejects_missing_frontmatter() {
        assert!(parse_agent(b"no frontmatter here").is_err());
    }

    #[test]
    fn recurse_false_strips_fan_out_tools() {
        let a = AgentDef {
            id: "impl".into(),
            tools: vec!["Read".into(), "Agent".into()],
            recurse: false,
            ..Default::default()
        };
        assert_eq!(a.allowed_tools(), ["Read"]);
    }

    #[test]
    fn recurse_true_keeps_fan_out_tools() {
        let a = AgentDef {
            id: "lead".into(),
            tools: vec!["Read".into(), "Agent".into()],
            recurse: true,
            ..Default::default()
        };
        assert_eq!(a.allowed_tools(), ["Read", "Agent"]);
    }

    #[test]
    fn isolation_none_opts_out_of_worktrees() {
        let none = AgentDef {
            id: "rev".into(),
            isolation: "none".into(),
            ..Default::default()
        };
        assert!(!none.isolated());
        let wt = AgentDef {
            id: "impl".into(),
            isolation: "worktree".into(),
            ..Default::default()
        };
        assert!(wt.isolated());
        let unset = AgentDef {
            id: "bare".into(),
            ..Default::default()
        };
        assert!(unset.isolated());
    }

    #[test]
    fn validate_catches_unknown_ref() {
        let mut cfg = Config::default();
        cfg.agents.insert(
            "a".into(),
            AgentDef {
                id: "a".into(),
                ..Default::default()
            },
        );
        cfg.workflow.stages.insert(
            "s".into(),
            Stage {
                name: "s".into(),
                agent: "ghost".into(),
                ..Default::default()
            },
        );
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn golden_apple_example_loads() {
        // The worked example the architecture references (§10, §11) must load and
        // validate into a real DAG, so it never rots. The path is relative to the
        // crate root (cargo runs tests there).
        let cfg =
            load("examples/golden-apple").expect("the golden-apple example must load and validate");
        // The full lens set + planner + implementer + adversary + adjudicator + integrator.
        assert_eq!(cfg.agents.len(), 8, "golden-apple agent count");
        assert_eq!(cfg.workflow.stages.len(), 4, "golden-apple stage count");
        assert_eq!(cfg.workflow.gates.len(), 4, "golden-apple gate count");
        // The shape: a producer, a worktree-isolated non-recursive implementer, a
        // three-tier review (lenses -> adversary -> adjudicator), and an
        // on_pass: merge integrate.
        assert_eq!(cfg.workflow.stages["plan"].produces, "dag");
        let implement = &cfg.workflow.stages["implement"];
        assert_eq!(implement.strategy, "fan-out");
        let impl_agent = &cfg.agents[&implement.agent];
        assert!(impl_agent.isolated(), "the implementer runs in a worktree");
        assert!(
            !impl_agent.recurse,
            "the implementer must not be able to fan out"
        );
        let review = &cfg.workflow.stages["review"];
        assert_eq!(review.agents.len(), 3, "tier 1: three expert lenses");
        assert_eq!(
            review.adversary, "adversary",
            "tier 2: the adversary refutes the lenses"
        );
        assert_eq!(
            review.adjudicator, "devils-advocate",
            "tier 3: the neutral adjudicator's verdict gates"
        );
        assert_eq!(cfg.workflow.stages["integrate"].on_pass, "merge");
    }

    #[test]
    fn validate_catches_cycle() {
        let mut cfg = Config::default();
        cfg.agents.insert(
            "a".into(),
            AgentDef {
                id: "a".into(),
                ..Default::default()
            },
        );
        cfg.workflow.stages.insert(
            "x".into(),
            Stage {
                name: "x".into(),
                agent: "a".into(),
                needs: vec!["y".into()],
                ..Default::default()
            },
        );
        cfg.workflow.stages.insert(
            "y".into(),
            Stage {
                name: "y".into(),
                agent: "a".into(),
                needs: vec!["x".into()],
                ..Default::default()
            },
        );
        assert!(cfg.validate().is_err());
    }
}
