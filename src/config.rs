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

/// ReviewPanel is the three-tier review roster a unit reviews ITSELF with: the
/// expert lenses (tier 1, parallel), the adversary that refutes the lenses (tier
/// 2), and the neutral adjudicator whose verdict gates integration (tier 3). It is
/// declared once on `defaults.review` and applied to every implementer unit; a
/// stage may override it with its own `review` block (§3.2). All three are
/// optional and compose: lenses alone, lenses + adjudicator, or the full trio.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ReviewPanel {
    #[serde(default)]
    pub lenses: Vec<String>,
    #[serde(default)]
    pub adversary: String,
    #[serde(default)]
    pub adjudicator: String,
}

impl ReviewPanel {
    /// Whether this panel has any review tier configured. An empty panel runs no
    /// per-unit review (the historical implement-then-integrate behavior).
    pub fn is_empty(&self) -> bool {
        self.lenses.is_empty() && self.adversary.is_empty() && self.adjudicator.is_empty()
    }

    /// Every agent id this panel references (the lenses, the adversary, the
    /// adjudicator), for referential validation.
    pub fn agent_ids(&self) -> Vec<String> {
        let mut ids = self.lenses.clone();
        if !self.adversary.is_empty() {
            ids.push(self.adversary.clone());
        }
        if !self.adjudicator.is_empty() {
            ids.push(self.adjudicator.clone());
        }
        ids
    }
}

/// Defaults are workflow-wide fallbacks for stages that do not set their own.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Defaults {
    #[serde(default)]
    pub autonomy: String,
    /// Which grounder the loop uses (§3.2, §5.4, R4). UNSET / empty resolves to
    /// `turbovec` (the real semantic grounder and the default cargo feature), NOT
    /// grep - so a workflow that says nothing gets semantic grounding. `"turbovec"`
    /// / `"vector"` select it explicitly; `"grep"` selects the literal substring
    /// grounder (reachable only when configured by name); `"nop"` grounds nothing.
    /// When the resolved grounder is turbovec but the binary was built WITHOUT the
    /// `turbovec` feature, selection FAILS LOUDLY rather than silently degrading to
    /// grep (see `grounder::grounder_for` / `main::select_grounder`).
    #[serde(default)]
    pub grounder: String,
    /// The three-tier review panel applied to every implementer unit (§3.2): each
    /// unit runs its own lifecycle and reviews ITSELF with this panel unless its
    /// stage overrides it with a `review` block. Declared once here, inherited by
    /// every planner-proposed unit too.
    #[serde(default)]
    pub review: ReviewPanel,
    /// The token/spawn circuit-breaker budget (§4.4, §8): the maximum number of
    /// agent spawns a run may perform. 0 (the default) means unlimited.
    #[serde(default)]
    pub budget: u32,
    /// The remediation depth: how many attempts a failed unit gets before it
    /// escalates to a human (§4.4). This is the refinement-depth knob, NOT a
    /// review-rigor knob - it gives a subtle unit room to converge under the full
    /// strict review instead of escalating prematurely. Absent (`0`) falls back to
    /// `safety::MAX_RETRIES` (3), the exact historical bound, so an un-set workflow
    /// is byte-for-byte back-compatible.
    #[serde(default)]
    pub max_retries: u32,
    /// The default partition strategy applied to every wave (§3.2, §8); a stage's
    /// own `partition` overrides it. `by-blast-radius` makes each wave's ready
    /// stages disjoint by blast-radius before they run; empty (the default) leaves
    /// the wave un-partitioned.
    #[serde(default)]
    pub partition: String,
    /// Where the run's transient scratch (unit/review worktrees) lives. Empty (the
    /// default) resolves to `<repo>/.rigger/tmp` - the REPO's partition, which on the
    /// common small-root/large-home layout is the big one, and same-filesystem with
    /// the checkout so worktree adds are cheap. A leading `~/` expands to $HOME. The
    /// `RIGGER_TMPDIR` environment variable overrides this for machine-local
    /// placement without touching versioned config. Never the OS temp dir: a 5G
    /// cargo target per worktree on a 69G root partition is how a run fills the OS
    /// disk (design-intent Gap 14).
    #[serde(default)]
    pub workdir: String,
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
    /// An optional per-stage override of `defaults.review` (§3.2): when set, this
    /// stage's units review themselves with this panel instead of the workflow
    /// default. Absent (the common case) means the unit uses `defaults.review`.
    #[serde(default)]
    pub review: ReviewPanel,
    #[serde(default)]
    pub autonomy: String,
    #[serde(default)]
    pub produces: String,
    #[serde(default)]
    pub coverage: String,
    #[serde(default)]
    pub on_pass: String,
    /// Set by the conductor (never authored, hence `serde(skip)`) on the deterministic
    /// per-criterion BASELINE units it synthesizes from the fan-out implement template.
    /// It marks a stage as the conductor's fallback decomposition for one criterion, so
    /// `harvest_proposed` can let a planner-proposed unit that cites the SAME criterion
    /// SUPERSEDE (remove) it - one unit per criterion, never baseline + refinement both
    /// doing the same work. False for every authored stage, the planner, the template,
    /// and every planner-proposed unit.
    #[serde(skip)]
    pub baseline: bool,
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
    /// standalone-review adversary/adjudicator, and any per-stage `review` override
    /// panel).
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
        ids.extend(self.review.agent_ids());
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
        // The default review panel (applied to every unit) must reference real agents.
        for aid in wf.defaults.review.agent_ids() {
            if !self.agents.contains_key(&aid) {
                return Err(err(format!(
                    "defaults.review references unknown agent {aid:?}"
                )));
            }
        }
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
    fn validate_catches_unknown_default_review_agent() {
        // The default review panel's agent ids are validated like every other agent
        // reference: an unknown lens/adversary/adjudicator fails validation.
        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent_def("a"));
        cfg.workflow.defaults.review = ReviewPanel {
            lenses: vec!["ghost".into()],
            ..Default::default()
        };
        assert!(
            cfg.validate().is_err(),
            "an unknown agent in defaults.review must fail validation"
        );
    }

    #[test]
    fn default_review_panel_parses_and_validates() {
        // A workflow declaring `defaults.review` parses the three-tier panel and
        // validates it referentially: known lenses + adversary + adjudicator load.
        let yaml = "name: w\n\
defaults:\n  \
review:\n    \
lenses: [archlens, techlens]\n    \
adversary: adv\n    \
adjudicator: adj\n\
stages:\n  \
implement:\n    \
agent: worker\n";
        let mut wf: Workflow = serde_yaml::from_str(yaml).unwrap();
        for name in wf.stages.keys().cloned().collect::<Vec<_>>() {
            if let Some(st) = wf.stages.get_mut(&name) {
                st.name = name;
            }
        }
        let review = &wf.defaults.review;
        assert_eq!(review.lenses, ["archlens", "techlens"]);
        assert_eq!(review.adversary, "adv");
        assert_eq!(review.adjudicator, "adj");
        assert_eq!(
            review.agent_ids(),
            ["archlens", "techlens", "adv", "adj"],
            "the panel reports every agent it references for validation"
        );

        // With those agents present, the config validates.
        let mut cfg = Config {
            workflow: wf,
            ..Default::default()
        };
        for id in ["archlens", "techlens", "adv", "adj", "worker"] {
            cfg.agents.insert(id.into(), agent_def(id));
        }
        assert!(cfg.validate().is_ok());
    }

    fn agent_def(id: &str) -> AgentDef {
        AgentDef {
            id: id.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn max_retries_parses_from_defaults_and_defaults_to_zero_when_absent() {
        // The remediation-depth knob parses from `defaults.max_retries` and is 0 when
        // omitted - the sentinel the conductor reads as "fall back to the historical
        // default of 3", so an un-set workflow is exactly back-compatible.
        let present: Workflow =
            serde_yaml::from_str("name: w\ndefaults:\n  max_retries: 6\n").unwrap();
        assert_eq!(
            present.defaults.max_retries, 6,
            "an explicit defaults.max_retries must parse through"
        );

        let absent: Workflow = serde_yaml::from_str("name: w\ndefaults:\n  budget: 60\n").unwrap();
        assert_eq!(
            absent.defaults.max_retries, 0,
            "an absent defaults.max_retries must default to 0 (the fall-back-to-3 sentinel)"
        );
    }

    #[test]
    fn demo_example_loads() {
        // The worked example the architecture references (§10, §11) must load and
        // validate into a real DAG, so it never rots. The path is relative to the
        // crate root (cargo runs tests there).
        let cfg = load("examples/demo").expect("the demo example must load and validate");
        // The full agent roster still loads from the dir.
        assert_eq!(cfg.agents.len(), 7, "demo agent count");
        // Per-unit model: plan -> implement (each unit implements, three-tier-reviews
        // ITSELF, and integrates in one lifecycle). The separate review/integrate
        // stages are folded out.
        assert_eq!(cfg.workflow.stages.len(), 2, "demo stage count");
        assert_eq!(cfg.workflow.gates.len(), 4, "demo gate count");
        // The shape: a producer, then a worktree-isolated non-recursive implementer
        // stage that runs the full per-unit lifecycle and integrates on_pass: merge.
        assert_eq!(cfg.workflow.stages["plan"].produces, "dag");
        let implement = &cfg.workflow.stages["implement"];
        assert_eq!(implement.strategy, "fan-out");
        assert_eq!(implement.on_pass, "merge");
        let impl_agent = &cfg.agents[&implement.agent];
        assert!(impl_agent.isolated(), "the implementer runs in a worktree");
        assert!(
            !impl_agent.recurse,
            "the implementer must not be able to fan out"
        );
        // The three-tier review panel is declared once on defaults.review and applied
        // to every implementer unit.
        let review = &cfg.workflow.defaults.review;
        assert_eq!(review.lenses.len(), 3, "tier 1: three expert lenses");
        assert_eq!(
            review.adversary, "adversary",
            "tier 2: the adversary refutes the lenses"
        );
        assert_eq!(
            review.adjudicator, "adjudicator",
            "tier 3: the neutral adjudicator's verdict gates"
        );
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
