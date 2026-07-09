//! The declarative surface of Rigger: agent definition files (markdown with YAML
//! frontmatter) and a workflow YAML, loaded and validated into runtime types.
//! Plain value types plus a loader; nothing here depends on the conductor.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::time::Duration;

use serde::Deserialize;

use crate::failure;

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
    /// A cheap-first model cascade (spec 10 unit 4): the successive model aliases this
    /// agent runs on across remediation attempts. Attempt 0 resolves rung 0 and each
    /// remediation attempt advances one rung, CLAMPED at the last rung once the ladder is
    /// exhausted, so a persistently-failing unit escalates from a cheap model to a strong
    /// one. Empty (the common case) means no cascade: the single [`model`](Self::model)
    /// alias is used on every attempt, exactly as before this field existed. When a ladder
    /// IS declared it takes precedence and `model` is ignored - `model` is the implicit
    /// one-rung ladder only when `model_ladder` is empty. Resolution lives in the single
    /// authority [`model_for_attempt`](Self::model_for_attempt).
    #[serde(default)]
    pub model_ladder: Vec<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub isolation: String,
    #[serde(default)]
    pub recurse: bool,
    /// The per-spawn wall-clock bound in SECONDS (spec 10, unit 3): a spawn of this
    /// agent whose liveness marker goes stale for longer than this is treated by
    /// `rigger step` as a hung/infra fault. `None` (unset in the agent's frontmatter)
    /// inherits `defaults.max_wall_clock`, folded in at [`load`] time; a resolved `0`
    /// (or absent default) means unbounded - the agent is never timed out. Per-role by
    /// construction: each agent's own value overrides the workflow default.
    #[serde(default)]
    pub max_wall_clock: Option<u64>,
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
    /// Optional blast-radius scope (spec 12, unit 3): the glob patterns naming the files
    /// this gate verifies. During the implement/remediate INNER LOOP the conductor runs
    /// only the gates whose `inputs` intersect the unit's grounded blast radius and logs a
    /// skip for the rest (never silent); the integrate step still runs the FULL library, so
    /// "done" is asserted against the exhaustive suite. An EMPTY `inputs` (the default) means
    /// the gate is UNSCOPED - it verifies the whole tree (e.g. a crate-wide `cargo test`) and
    /// so always runs, never narrowed away. Globs support `**` (any path segments), `*` (one
    /// segment), and `?` (one non-`/` char), matched against repo-relative paths.
    #[serde(default)]
    pub inputs: Vec<String>,
}

/// One declarative failure rule (spec 10, unit 2), authored under
/// `defaults.failure_rules`. It matches a failure signal (a process's exit status,
/// terminating signal, and/or captured output) and classifies it, with a per-rule rerun
/// `limit` and exponential `backoff`. The runtime form is [`failure::FailureRule`]; this
/// is only the declarative surface. Rules are evaluated first-match-wins.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct FailureRuleDef {
    /// The match predicate. `match` is a Rust keyword, so it is deserialized under the
    /// field name `match` into `match_`.
    #[serde(default, rename = "match")]
    pub match_: MatchDef,
    /// One of `infra` | `product` | `flaky`. An unknown value fails validation.
    #[serde(default)]
    pub class: String,
    /// The rerun budget for a matching gate failure (the Bazel flaky-attempts count):
    /// how many additional times the gate is rerun before the failure is believed.
    /// 0 (the default) never reruns.
    #[serde(default)]
    pub limit: u32,
    #[serde(default)]
    pub backoff: BackoffDef,
}

/// The declarative match predicate of a [`FailureRuleDef`]. Every PRESENT field must
/// match (logical AND); an absent field is a wildcard, so an all-absent `match` is the
/// catch-all a final `product` rule uses.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct MatchDef {
    #[serde(default)]
    pub exit_status: Option<i32>,
    #[serde(default)]
    pub signal: Option<i32>,
    /// A regular expression matched against the failure's captured output.
    #[serde(default)]
    pub output_regex: Option<String>,
}

/// The declarative exponential backoff of a [`FailureRuleDef`]. The spec's
/// `{duration, factor, max}` is expressed as unambiguous MILLISECONDS
/// (`duration_ms` / `max_ms`) so a value like `1000` never reads as an ambiguous
/// bare "duration".
#[derive(Clone, Debug, Default, Deserialize)]
pub struct BackoffDef {
    /// Base delay before the first rerun, in milliseconds. 0 = no wait.
    #[serde(default)]
    pub duration_ms: u64,
    /// Multiplier applied per rerun. Absent / non-positive is treated as `1.0` (a flat
    /// backoff), never `0` (which would collapse every delay after the first to zero).
    #[serde(default)]
    pub factor: f64,
    /// Cap on the computed delay, in milliseconds. 0 = uncapped.
    #[serde(default)]
    pub max_ms: u64,
}

impl FailureRuleDef {
    /// Convert this declarative rule into its runtime [`failure::FailureRule`], compiling
    /// the `output_regex` and validating the class. Errors (a bad regex, an unknown
    /// class) surface at config load so a misauthored rule fails fast rather than at the
    /// first classification.
    pub fn to_rule(&self) -> Result<failure::FailureRule, Error> {
        let class = failure::FailureClass::parse(&self.class).ok_or_else(|| {
            err(format!(
                "failure rule has unknown class {:?} (want infra | product | flaky)",
                self.class
            ))
        })?;
        let output_regex = match &self.match_.output_regex {
            Some(pat) => Some(
                regex::Regex::new(pat)
                    .map_err(|e| err(format!("failure rule output_regex {pat:?}: {e}")))?,
            ),
            None => None,
        };
        let factor = if self.backoff.factor > 0.0 {
            self.backoff.factor
        } else {
            1.0
        };
        Ok(failure::FailureRule {
            matcher: failure::Matcher {
                exit_status: self.match_.exit_status,
                signal: self.match_.signal,
                output_regex,
            },
            class,
            limit: self.limit,
            backoff: failure::Backoff {
                duration: Duration::from_millis(self.backoff.duration_ms),
                factor,
                max: Duration::from_millis(self.backoff.max_ms),
            },
        })
    }
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
    /// First-green-wins speculation width (spec 13, unit 3): how many parallel
    /// implementer candidates a unit parks in one deterministic speculation group.
    /// The first candidate to pass its gates AND the adjudicator wins and integrates;
    /// the rest are cancelled. `0`/`1` (the default) means OFF - one candidate, the
    /// historical single-implementer path byte-for-byte. A stage's own
    /// `speculation_width` overrides this default.
    #[serde(default)]
    pub speculation_width: u32,
    /// The default per-spawn wall-clock bound in SECONDS (spec 10, unit 3): every agent
    /// inherits this unless its own frontmatter sets `max_wall_clock`. `0` (the default)
    /// means unbounded - liveness timeouts are opt-in, so an un-set workflow is
    /// byte-for-byte back-compatible (no spawn is ever timed out). Applied to each agent
    /// at [`load`] time so a parked spawn carries its resolved bound.
    #[serde(default)]
    pub max_wall_clock: u64,
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
    /// The declarative failure taxonomy (spec 10, unit 2): an ordered list of rules,
    /// matched first-wins, that classify a failure into `infra` | `product` | `flaky`
    /// with a per-rule rerun `limit` and `backoff`. Empty (the default) means the
    /// conductor uses [`failure::Taxonomy::default`], whose shipped rules preserve
    /// spec-07 infra-vs-product semantics.
    #[serde(default)]
    pub failure_rules: Vec<FailureRuleDef>,
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
    /// This unit class's first-green-wins speculation width (spec 13, unit 3): when
    /// `> 1`, the conductor parks this many parallel implementer candidates in one
    /// deterministic speculation group and integrates the first gate-green
    /// adjudicator-approved one, cancelling the rest. Unset (`0`) inherits
    /// `defaults.speculation_width`; the effective width `1` is the historical
    /// single-implementer path unchanged (speculation defaults OFF).
    #[serde(default)]
    pub speculation_width: u32,
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

    /// The model alias this agent runs on for `attempt` (the 0-based remediation attempt),
    /// resolving the cheap-first cascade (spec 10 unit 4). This is the SINGLE model-selection
    /// authority: every driver (which sets the actual spawn model) and the conductor (which
    /// stamps the requested alias onto the spawn's unit events) resolve through it, so the
    /// model that runs and the model recorded agree by construction for every attempt.
    ///
    /// With a [`model_ladder`](Self::model_ladder) declared, attempt `n` resolves rung `n`,
    /// CLAMPED at the last rung once the ladder is exhausted (a unit that keeps failing stays
    /// on the strongest rung). Absent a ladder, the single [`model`](Self::model) alias is a
    /// one-rung ladder returned on every attempt - so an agent that only sets `model` behaves
    /// exactly as it did before the cascade existed. Empty when the agent declares neither
    /// (the driver's default model is inherited).
    pub fn model_for_attempt(&self, attempt: u32) -> String {
        if self.model_ladder.is_empty() {
            return self.model.clone();
        }
        let last = self.model_ladder.len() - 1;
        self.model_ladder[(attempt as usize).min(last)].clone()
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

impl Workflow {
    /// Build the runtime failure taxonomy this workflow classifies failures through
    /// (spec 10, unit 2). When `defaults.failure_rules` is authored, it is the ordered,
    /// first-match-wins rule set (each rule's `output_regex` compiled and `class`
    /// validated here - the SINGLE conversion the conductor and [`Config::validate`] both
    /// call, so a bad rule fails at load). When it is empty (the common case), the
    /// shipped [`failure::Taxonomy::default`] is used, preserving spec-07 semantics.
    pub fn failure_taxonomy(&self) -> Result<failure::Taxonomy, Error> {
        if self.defaults.failure_rules.is_empty() {
            return Ok(failure::Taxonomy::default());
        }
        let rules = self
            .defaults
            .failure_rules
            .iter()
            .map(FailureRuleDef::to_rule)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(failure::Taxonomy::new(rules))
    }
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
    let mut agents = load_agents(&base.join("agents"))?;
    let workflow = load_workflow(&base.join("workflow.yml"))?;
    resolve_wall_clocks(&mut agents, &workflow.defaults);
    let cfg = Config { agents, workflow };
    cfg.validate()?;
    Ok(cfg)
}

/// Fold `defaults.max_wall_clock` onto every agent that did not set its own (spec 10,
/// unit 3), so an agent's resolved `max_wall_clock` is authoritative wherever a spawn is
/// built - the replay driver reads it straight off the [`AgentDef`] with no access to the
/// workflow. Per-role by construction: an agent's own value wins; only an unset agent
/// inherits the default. A zero default leaves an unset agent unbounded (`None`).
fn resolve_wall_clocks(agents: &mut BTreeMap<String, AgentDef>, defaults: &Defaults) {
    if defaults.max_wall_clock == 0 {
        return;
    }
    for agent in agents.values_mut() {
        if agent.max_wall_clock.is_none() {
            agent.max_wall_clock = Some(defaults.max_wall_clock);
        }
    }
}

fn load_agents(dir: &Path) -> Result<BTreeMap<String, AgentDef>, Error> {
    index_agents(read_agents_dir(dir)?)
}

/// Read every `<dir>/*.md` agent definition into `(filename, AgentDef)` pairs, parsing
/// each through [`parse_agent`]. This is the ONE directory-read-and-parse loop: [`load`]
/// calls it then [`index_agents`]; a caller assembling a prospective fleet (e.g. `rigger
/// setup --agents`) calls it to enumerate the agents already on disk, then indexes that
/// list combined with its own additions - so both enumerate existing agents through the
/// same seam rather than re-implementing (and drifting from) the loop. Returns the pairs
/// UN-indexed so the caller controls when the fleet-wide id invariant is enforced.
pub fn read_agents_dir(dir: &Path) -> Result<Vec<(String, AgentDef)>, Error> {
    let entries = std::fs::read_dir(dir).map_err(|e| err(format!("read agents dir: {e}")))?;
    let mut parsed = Vec::new();
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
        parsed.push((name, a));
    }
    Ok(parsed)
}

/// Index a set of `(name, AgentDef)` pairs by id, enforcing the fleet invariants the
/// loader relies on: every agent has a non-empty id, and no two agents share one. This
/// is the SINGLE definition of a valid agent identity, so a caller assembling a
/// prospective fleet (e.g. `rigger setup --agents`) validates by the same rule [`load`]
/// does rather than re-implementing (and drifting from) it. `name` is a filename, used
/// only for error context.
pub fn index_agents(
    agents: impl IntoIterator<Item = (String, AgentDef)>,
) -> Result<BTreeMap<String, AgentDef>, Error> {
    let mut map = BTreeMap::new();
    for (name, a) in agents {
        if a.id.is_empty() {
            return Err(err(format!("{name}: agent is missing an id")));
        }
        if map.contains_key(&a.id) {
            return Err(err(format!("duplicate agent id {:?}", a.id)));
        }
        map.insert(a.id.clone(), a);
    }
    Ok(map)
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

/// Split a markdown-with-YAML-frontmatter document into `(frontmatter, body)`: the text
/// between the opening `---` line and the closing `---` line, and everything after the
/// closing delimiter. The single frontmatter-delimiter parser, so callers that only need
/// the frontmatter (e.g. `rigger setup --agents` normalizing the identity key) parse it
/// through this seam instead of a second copy of the delimiter logic. Errors when the
/// opening or closing `---` is missing.
pub fn split_frontmatter(s: &str) -> Result<(&str, &str), Error> {
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
        // Fail fast on a misauthored failure rule (an unknown class, an uncompilable
        // output_regex) at load rather than at the first classification, through the
        // SAME conversion the conductor uses.
        wf.failure_taxonomy()?;
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
    fn model_ladder_parses_from_frontmatter() {
        // Agent frontmatter accepts a `model_ladder` list (spec 10 unit 4): the cheap-first
        // cascade the agent escalates through under remediation.
        let b = b"---\nid: worker\nmodel_ladder: [haiku, sonnet, opus]\n---\nImplement.\n";
        let a = parse_agent(b).unwrap();
        assert_eq!(a.id, "worker");
        assert_eq!(a.model_ladder, ["haiku", "sonnet", "opus"]);
        // Absent a `model:` line, the single-model field stays empty (the ladder is authority).
        assert_eq!(a.model, "");
    }

    #[test]
    fn model_for_attempt_advances_one_rung_per_attempt_and_clamps_at_the_last() {
        // A unit's first attempt resolves the first rung and each remediation attempt advances
        // one rung, CLAMPED at the last once exhausted (spec 10 unit 4).
        let a = AgentDef {
            id: "worker".into(),
            model_ladder: vec!["haiku".into(), "sonnet".into(), "opus".into()],
            ..Default::default()
        };
        assert_eq!(a.model_for_attempt(0), "haiku", "attempt 0 -> rung 0");
        assert_eq!(a.model_for_attempt(1), "sonnet", "attempt 1 -> rung 1");
        assert_eq!(a.model_for_attempt(2), "opus", "attempt 2 -> rung 2 (last)");
        assert_eq!(
            a.model_for_attempt(3),
            "opus",
            "past the end clamps at the last rung"
        );
        assert_eq!(
            a.model_for_attempt(99),
            "opus",
            "far past the end still clamps"
        );
    }

    #[test]
    fn a_single_model_is_a_one_rung_ladder_used_on_every_attempt() {
        // With no ladder, the single `model` alias is returned on every attempt - so an agent
        // that only sets `model` behaves EXACTLY as before the cascade existed (back-compat).
        let one = AgentDef {
            id: "worker".into(),
            model: "sonnet".into(),
            ..Default::default()
        };
        for attempt in [0, 1, 5, 42] {
            assert_eq!(
                one.model_for_attempt(attempt),
                "sonnet",
                "a lone model: does not ladder - attempt {attempt} still resolves it"
            );
        }
        // Neither declared: empty (the driver's default model is inherited), on every attempt.
        let none = AgentDef {
            id: "worker".into(),
            ..Default::default()
        };
        assert_eq!(none.model_for_attempt(0), "");
        assert_eq!(none.model_for_attempt(3), "");
    }

    #[test]
    fn a_declared_ladder_takes_precedence_over_a_lone_model() {
        // When both are set the ladder is authority and `model` is ignored, so the resolved
        // rung is never silently overridden by the shorthand field.
        let a = AgentDef {
            id: "worker".into(),
            model: "haiku".into(),
            model_ladder: vec!["sonnet".into(), "opus".into()],
            ..Default::default()
        };
        assert_eq!(a.model_for_attempt(0), "sonnet");
        assert_eq!(a.model_for_attempt(1), "opus");
    }

    #[test]
    fn parses_agent_max_wall_clock_and_defaults_to_none_when_absent() {
        let with = parse_agent(b"---\nid: slow\nmax_wall_clock: 1800\n---\nbody\n").unwrap();
        assert_eq!(
            with.max_wall_clock,
            Some(1800),
            "an explicit per-role max_wall_clock parses through"
        );
        let without = parse_agent(b"---\nid: plain\n---\nbody\n").unwrap();
        assert_eq!(
            without.max_wall_clock, None,
            "an absent max_wall_clock is None (inherits the workflow default)"
        );
    }

    #[test]
    fn defaults_max_wall_clock_parses_and_is_zero_when_absent() {
        let present: Workflow =
            serde_yaml::from_str("name: w\ndefaults:\n  max_wall_clock: 600\n").unwrap();
        assert_eq!(present.defaults.max_wall_clock, 600);
        let absent: Workflow = serde_yaml::from_str("name: w\n").unwrap();
        assert_eq!(
            absent.defaults.max_wall_clock, 0,
            "an absent default is 0 (unbounded - liveness timeouts are opt-in)"
        );
    }

    #[test]
    fn resolve_wall_clocks_folds_the_default_only_onto_unset_agents() {
        let mut agents = BTreeMap::new();
        agents.insert(
            "unset".to_string(),
            AgentDef {
                id: "unset".into(),
                max_wall_clock: None,
                ..Default::default()
            },
        );
        agents.insert(
            "override".to_string(),
            AgentDef {
                id: "override".into(),
                max_wall_clock: Some(60),
                ..Default::default()
            },
        );
        let defaults = Defaults {
            max_wall_clock: 900,
            ..Default::default()
        };
        resolve_wall_clocks(&mut agents, &defaults);
        assert_eq!(
            agents["unset"].max_wall_clock,
            Some(900),
            "an unset agent inherits the workflow default"
        );
        assert_eq!(
            agents["override"].max_wall_clock,
            Some(60),
            "an agent's own per-role value overrides the default"
        );
    }

    #[test]
    fn resolve_wall_clocks_leaves_agents_unbounded_under_a_zero_default() {
        let mut agents = BTreeMap::new();
        agents.insert(
            "a".to_string(),
            AgentDef {
                id: "a".into(),
                max_wall_clock: None,
                ..Default::default()
            },
        );
        resolve_wall_clocks(&mut agents, &Defaults::default());
        assert_eq!(
            agents["a"].max_wall_clock, None,
            "a zero (absent) default leaves an unset agent unbounded, back-compatible"
        );
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
    fn failure_rules_parse_into_an_ordered_taxonomy() {
        // An authored `defaults.failure_rules` block parses into a first-match-wins
        // taxonomy: the matcher fields, the class, the per-rule limit, and the backoff
        // all convert to the runtime form (spec 10, unit 2).
        let yaml = "name: w\n\
defaults:\n  \
failure_rules:\n    \
- match: {output_regex: \"segfault|SIGSEGV\"}\n      \
class: flaky\n      \
limit: 3\n      \
backoff: {duration_ms: 500, factor: 2.0, max_ms: 8000}\n    \
- match: {exit_status: 137}\n      \
class: infra\n      \
limit: 1\n    \
- match: {}\n      \
class: product\n";
        let wf: Workflow = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(wf.defaults.failure_rules.len(), 3);
        let tax = wf.failure_taxonomy().expect("authored rules must convert");
        assert_eq!(tax.rules().len(), 3);
        // First-match-wins: the segfault signal hits the flaky rule with its limit and
        // a non-zero backoff, not the product catch-all.
        let flaky = tax
            .classify(&failure::Signal::from_output("thread panicked: SIGSEGV"))
            .unwrap();
        assert_eq!(flaky.class, failure::FailureClass::Flaky);
        assert_eq!(flaky.limit, 3);
        assert_eq!(flaky.backoff.duration, Duration::from_millis(500));
        // The exit-status rule matches a killed worker (137) as infra.
        let infra = tax
            .classify(&failure::Signal {
                exit_status: Some(137),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(infra.class, failure::FailureClass::Infra);
        // Anything else falls through to the product catch-all.
        assert_eq!(
            tax.classify(&failure::Signal::from_output("assertion failed"))
                .unwrap()
                .class,
            failure::FailureClass::Product
        );
    }

    #[test]
    fn absent_failure_rules_fall_back_to_the_spec07_preserving_defaults() {
        // The common case: a workflow authors no failure_rules, so the taxonomy is the
        // shipped default (infra faults reran, everything else product) - preserving
        // spec-07 semantics and every existing gate test.
        let wf: Workflow = serde_yaml::from_str("name: w\ndefaults:\n  budget: 10\n").unwrap();
        assert!(wf.defaults.failure_rules.is_empty());
        let tax = wf.failure_taxonomy().unwrap();
        assert!(!tax.is_empty(), "the default taxonomy ships rules");
        assert_eq!(
            tax.classify(&failure::Signal::from_output("FAIL\nassertion failed"))
                .unwrap()
                .class,
            failure::FailureClass::Product,
            "a plain gate failure classifies product by default (no rerun, today's behavior)"
        );
    }

    #[test]
    fn validate_rejects_an_unknown_failure_class_and_a_bad_regex() {
        let base = |rules: &str| -> Config {
            let wf: Workflow =
                serde_yaml::from_str(&format!("name: w\ndefaults:\n  failure_rules:\n{rules}"))
                    .unwrap();
            Config {
                workflow: wf,
                ..Default::default()
            }
        };
        // An unknown class is rejected at validation.
        assert!(base("    - match: {}\n      class: bogus\n")
            .validate()
            .is_err());
        // An uncompilable regex is rejected at validation.
        assert!(
            base("    - match: {output_regex: \"(\"}\n      class: flaky\n")
                .validate()
                .is_err()
        );
        // A well-formed rule validates.
        assert!(base(
            "    - match: {output_regex: \"flake\"}\n      class: flaky\n      limit: 2\n"
        )
        .validate()
        .is_ok());
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
