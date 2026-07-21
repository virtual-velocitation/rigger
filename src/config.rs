//! The declarative surface of Rigger: agent definition files (markdown with YAML
//! frontmatter) and a workflow YAML, loaded and validated into runtime types.
//! Plain value types plus a loader; nothing here depends on the conductor.

use std::collections::{BTreeMap, BTreeSet, HashMap};
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
    /// The OPT-IN risk-tiered review-depth policy (spec 03 "adaptive review depth",
    /// spec 13 unit 4). When set, THIS panel is the FULL panel and `tiers.light` is
    /// the reduced roster a LOW-RISK unit reviews itself with; the conductor routes
    /// each unit to light-or-full by its observable risk before running the tiers
    /// (see `select_review_panel`). Absent (the shipped default and every existing
    /// workflow) means every unit runs this full panel and behavior is byte-for-byte
    /// unchanged - tiering is opt-in. Carried here (not on `Defaults`) so BOTH
    /// `defaults.review` and a per-stage `review` override inherit tiering through the
    /// one panel abstraction. Boxed to break the ReviewPanel -> ReviewDepth ->
    /// ReviewPanel type recursion (a fixed-size pointer instead of an infinite value).
    #[serde(default)]
    pub tiers: Option<Box<ReviewDepth>>,
}

impl ReviewPanel {
    /// Whether this panel has any review tier configured. An empty panel runs no
    /// per-unit review (the historical implement-then-integrate behavior). A
    /// `tiers` policy alone (no roster) is meaningless - there is no full panel to
    /// reduce from - so it does not make a panel non-empty.
    pub fn is_empty(&self) -> bool {
        self.lenses.is_empty() && self.adversary.is_empty() && self.adjudicator.is_empty()
    }

    /// The configured risk-tiered depth policy, if any.
    pub fn depth(&self) -> Option<&ReviewDepth> {
        self.tiers.as_deref()
    }

    /// Every agent id this panel references (the lenses, the adversary, the
    /// adjudicator, AND the light-tier roster when a depth policy is configured),
    /// for referential validation. Extending this to the light panel is what makes
    /// an unknown light-panel lens/adversary/adjudicator fail `config::load` like any
    /// other unresolved reference (spec 03: the light panel's agent ids are validated
    /// too). Terminates: the light panel's own `tiers` is `None` in every real config.
    pub fn agent_ids(&self) -> Vec<String> {
        let mut ids = self.lenses.clone();
        if !self.adversary.is_empty() {
            ids.push(self.adversary.clone());
        }
        if !self.adjudicator.is_empty() {
            ids.push(self.adjudicator.clone());
        }
        if let Some(depth) = self.depth() {
            ids.extend(depth.light.agent_ids());
        }
        ids
    }

    /// The GATING agent ids this panel names: the adjudicator on this tier and,
    /// recursively, the light tier's adjudicator. A gating agent's result-channel
    /// verdict gates integration; the lenses and the adversary do NOT render a gating
    /// verdict (the adversary "does not render the final verdict"), so only adjudicators
    /// are collected here. This is the roster the verdict-line lint
    /// ([`lint_gating_verdict_lines`]) inspects.
    pub fn gating_agent_ids(&self) -> Vec<String> {
        let mut ids = Vec::new();
        if !self.adjudicator.is_empty() {
            ids.push(self.adjudicator.clone());
        }
        if let Some(depth) = self.depth() {
            ids.extend(depth.light.gating_agent_ids());
        }
        ids
    }

    /// Validate the depth policy's structural invariant: the gating verdict is mandatory
    /// on EVERY tier - only the adversary (and the extra lenses) flex (spec 03 / spec 13
    /// unit 4) - so BOTH the reduced LIGHT tier AND the enclosing FULL panel this depth
    /// policy sits on MUST name an adjudicator.
    ///
    /// The full-panel check closes the inverted-guarantee hole: a high-risk unit routes
    /// to THIS (full) panel, and an empty full roster (or one that names no adjudicator)
    /// would let `review_unit` approve it trivially via `panel.is_empty()` - so the
    /// highest-risk units would get NO adjudicator while low-risk units got the light one.
    /// A full panel that names an adjudicator is necessarily non-empty, so this one check
    /// rejects both the empty-full-panel and the adjudicator-less-full-panel cases (and a
    /// per-stage `review:` that declares ONLY a `tiers:` policy with no roster - which
    /// `effective_review_panel` would otherwise silently discard back to defaults - now
    /// fails `config::load` loudly instead). The light-panel check likewise guards the
    /// low-risk route. Both fail `config::load` rather than silently, returning a bare
    /// message the caller wraps into its `Error` with the offending scope.
    pub fn validate_depth(&self) -> Result<(), String> {
        if let Some(depth) = self.depth() {
            // The enclosing FULL panel (this one) must name an adjudicator: a high-risk
            // unit routes here, and an empty/adjudicator-less full panel would approve it
            // trivially. A panel that names an adjudicator is necessarily non-empty, so
            // this rejects both the empty-full-panel and the adjudicator-less cases.
            if self.adjudicator.is_empty() {
                return Err(
                    "a review.tiers policy requires its enclosing (full) panel to name an \
                     adjudicator (the gating verdict is mandatory on every review tier - a \
                     high-risk unit routes to the full panel, so an empty/adjudicator-less \
                     full panel would approve it with no verdict)"
                        .to_string(),
                );
            }
            // ...and the reduced LIGHT tier a low-risk unit routes to must name one too.
            if depth.light.adjudicator.is_empty() {
                return Err(
                    "review.tiers.light must name an adjudicator (the gating verdict is \
                     mandatory on every review tier - only the adversary flexes)"
                        .to_string(),
                );
            }
        }
        Ok(())
    }
}

/// ReviewDepth is the OPT-IN risk-tiered review policy (spec 03 "adaptive review
/// depth", spec 13 unit 4). Declared under a `review` block's `tiers:`, it routes
/// each implementer unit to the reduced `light` panel or the enclosing full panel by
/// the unit's OBSERVABLE risk - the exact signals the loop already computes: the
/// unit's grounded blast-radius file count against `threshold`, whether any
/// blast-radius file matches a `high_risk_paths` prefix/glob, and whether the unit's
/// gates FLAPPED (it needed remediation to reach green). The adjudicator and the full
/// gate suite stay mandatory on every tier - only the adversary and the extra lenses
/// flex - so the light route still gates integration.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ReviewDepth {
    /// The reduced roster a LOW-RISK unit reviews itself with (typically fewer lenses
    /// and no adversary). It must name an adjudicator (`validate_depth`), and its
    /// agent ids are validated like the full panel's (`agent_ids`).
    #[serde(default)]
    pub light: ReviewPanel,
    /// The maximum grounded blast-radius file count for a unit to stay "low risk": a
    /// unit whose blast radius exceeds this runs the full panel. `0` (the default)
    /// means no unit qualifies as low-risk by size alone, so a workflow that declares
    /// `tiers` but forgets `threshold` keeps the full panel for every non-empty unit -
    /// tiering never silently weakens review.
    ///
    /// SIZE SIGNAL (spec 16 unit 3): the file count measured against `threshold` is the
    /// unit's `.safe` structural blast-radius view, NOT the capped grounded seed. On the
    /// STRUCTURAL (symbols) grounder `.safe` is the UNCAPPED grep-union-structural superset
    /// (the change's true structural width), so `threshold` is a LIVE size gate over that
    /// full width: it is NOT bounded above by the grounder's `k`, and a `threshold >= 8` is
    /// NO LONGER inert, so any change wider than the threshold routes to the full panel. Tune
    /// it to the structural-width distribution of your repo: set it too low and every unit
    /// exceeds it, collapsing tier routing to all-full and defeating the parallelism the
    /// light tier exists to retain; use `high_risk_paths` for breadth the size signal
    /// cannot express. On the default / turbovec / grep lane `.safe` equals the capped
    /// grounded seed (safe == precise == the pre-unit-3 seed), so there the count is still
    /// the grep-hit spread within the first `k` line-matches and this threshold behaves
    /// exactly as it did before unit 3.
    #[serde(default)]
    pub threshold: usize,
    /// Path prefixes / globs that force the FULL panel even for a small change: a unit
    /// whose blast radius touches any of these is high-risk regardless of size (e.g. a
    /// core trait or a spec file). An entry matches a blast-radius file by literal
    /// prefix OR by the same glob semantics gate `inputs:` use.
    #[serde(default)]
    pub high_risk_paths: Vec<String>,
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
    /// Whether the always-on SDET periphery-test AUTHOR role is spawned (spec 32). The
    /// sdet-author writes the periphery test layer (contract / API / integration) at the
    /// build seam so no boundary surface a unit exposes lands untested; it self-scopes to a
    /// fast no-op on a purely-internal unit. Read through the single resolution authority
    /// [`sdet_author_enabled`](Self::sdet_author_enabled): `None` (the default, and every
    /// workflow that omits the field) is ON, an explicit `false` opts a workflow out.
    ///
    /// Modeled as `Option<bool>` (not a bare `bool`) SO the on-by-default rule holds whether
    /// the field OR the whole `defaults:` block is omitted. `Defaults` derives `Default` and
    /// `Workflow.defaults` is `#[serde(default)]`, so a workflow with no `defaults:` block
    /// constructs the DERIVED `Defaults::default()` - where a bare `bool` reads `false` and
    /// would silently DISABLE the role. `Option::default()` is `None`, which
    /// `sdet_author_enabled` maps to ON, so the derived default and a serde omission agree.
    #[serde(default)]
    pub sdet_author: Option<bool>,
}

impl Defaults {
    /// Whether the SDET periphery-test author role is spawned (spec 32) - the single
    /// resolution authority for the on-by-default toggle. Unset (`None`, the default) is ON;
    /// only an explicit `sdet_author: false` disables it. The conductor's build-seam reads
    /// through here, so the on-by-default rule lives in exactly one place and cannot drift
    /// between the derived `Default` and a from-YAML omission.
    pub fn sdet_author_enabled(&self) -> bool {
        self.sdet_author.unwrap_or(true)
    }
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
    /// The STABLE id of the acceptance criterion this baseline serves (spec 18 §3.3):
    /// its 1-based position plus a content hash of the normalized criterion text,
    /// computed by `conductor::criterion_stable_id`. Set by the conductor (never
    /// authored, hence `serde(skip)`) on the per-criterion baseline units only. The
    /// planner is shown this id next to each criterion and echoes it on every
    /// proposal, so `harvest_proposed` matches a proposal to its baseline by this id
    /// rather than by re-normalized prose - a paraphrase or truncation of a long
    /// criterion the planner was told to copy verbatim no longer silently spawns a
    /// duplicate. Empty for every non-baseline stage.
    #[serde(skip)]
    pub criterion_id: String,
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
        // The default review panel (applied to every unit) must reference real agents,
        // including its light-tier roster, and its depth policy must be structurally
        // sound (a configured light tier names an adjudicator).
        for aid in wf.defaults.review.agent_ids() {
            if !self.agents.contains_key(&aid) {
                return Err(err(format!(
                    "defaults.review references unknown agent {aid:?}"
                )));
            }
        }
        wf.defaults
            .review
            .validate_depth()
            .map_err(|m| err(format!("defaults.{m}")))?;
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
            // A per-stage `review` override that declares a depth policy must satisfy the
            // same light-tier-names-an-adjudicator invariant as the default panel.
            st.review
                .validate_depth()
                .map_err(|m| err(format!("stage {name:?} {m}")))?;
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

/// The gating (verdict-bearing) agent ids across the WHOLE config: every review panel's
/// adjudicator (the `defaults.review` panel and every per-stage `review` override, each
/// including its light tier via [`ReviewPanel::gating_agent_ids`]) plus every stage's
/// own standalone `adjudicator` (the plan-critique / decomposition-review gate). Deduped
/// and ordered by a `BTreeSet` so the lint below reports a deterministic first offender.
fn gating_agent_ids(cfg: &Config) -> BTreeSet<String> {
    let critique_gate = plan_critique_gate_name(cfg);
    let mut ids: BTreeSet<String> = BTreeSet::new();
    ids.extend(cfg.workflow.defaults.review.gating_agent_ids());
    for (name, st) in &cfg.workflow.stages {
        // Skip the plan-critique gate stage's standalone adjudicator: the conductor builds ITS
        // prompt via `build_dag_critique_prompt`, which ALWAYS appends the result-channel verdict
        // line, so its persona need not carry one (adj-u18-1r3 FP#2). If that same agent id ALSO
        // serves a per-unit review adjudicator (via `defaults.review` above, or another stage's
        // `review`/`adjudicator` below), it is still collected through those paths and stays
        // linted - the per-unit `build_prompt` path injects no verdict line.
        if !st.adjudicator.is_empty() && critique_gate.as_deref() != Some(name.as_str()) {
            ids.insert(st.adjudicator.clone());
        }
        ids.extend(st.review.gating_agent_ids());
    }
    ids
}

/// The name of the plan-critique / DAG-critique gate stage, if the workflow wires one - the
/// review-only stage (no `agent`) that carries a standalone `adjudicator` and `needs` the producer
/// (the planner). This mirrors `conductor::critique_gate_name`'s ROLE-based recognition over the
/// same `Stage` fields. The conductor drives THAT gate's adjudicator with a prompt from
/// `build_dag_critique_prompt`, which appends the result-channel verdict line unconditionally, so
/// the gate's own persona need not carry it (adj-u18-1r3 FP#2). `config` is the lower layer and
/// cannot depend upward on `conductor`, so this small structural predicate is read here directly
/// from the config; it is a CLASSIFIER over the same fields, not a second copy of the gate's
/// behavior. Returns `None` for a non-decomposing workflow (no producer) or one with no such gate,
/// so an ordinary standalone stage adjudicator is never mistaken for the injected gate.
fn plan_critique_gate_name(cfg: &Config) -> Option<String> {
    let producer = cfg
        .workflow
        .stages
        .iter()
        .find(|(_, st)| !st.produces.is_empty())
        .map(|(name, _)| name.clone())?;
    cfg.workflow
        .stages
        .iter()
        .find(|(_, st)| {
            st.agent.is_empty() && !st.adjudicator.is_empty() && st.needs.contains(&producer)
        })
        .map(|(name, _)| name.clone())
}

/// Static lint (spec 18, unit 1): every GATING agent's persona prompt must instruct it to
/// put its verdict on the RESULT channel - the integration gate reads a gating spawn's
/// result output for a `{"verdict":...}` line and NEVER reads emitted events, so a persona
/// whose only verdict path is `rigger_emit` is a guaranteed stall (the gate finds no
/// verdict, folds it as a non-approval, and the unit remediates until it escalates). The
/// check is deterministic (a certain hang), so it is a HARD error naming the exact fix.
///
/// Called from `rigger validate` (and, once wired by unit 2, at run start) - NOT from
/// [`load`], so the run-start refusal stays a separate, deliberate wiring over this one
/// authority rather than a second copy of the check.
pub fn lint_gating_verdict_lines(cfg: &Config) -> Result<(), Error> {
    for id in gating_agent_ids(cfg) {
        // A missing id is already a referential error (`Config::validate`); skip here so
        // this lint reports only the verdict-line defect, never a duplicate "unknown agent".
        let Some(agent) = cfg.agents.get(&id) else {
            continue;
        };
        if !puts_verdict_on_result_channel(&agent.prompt) {
            return Err(err(format!(
                "agent {id:?} is a gating role but its prompt never instructs it to end its \
                 output with a verdict line (e.g. {{\"verdict\":\"approve\"}}). The integration \
                 gate reads the result channel, not emitted events; a verdict emitted only via \
                 rigger_emit will never gate. Add the verdict line to the agent's output."
            )));
        }
    }
    Ok(())
}

/// Advisory (spec 19c, unit 3): warn when `defaults.max_wall_clock` is unbounded (`0`) AND
/// some GATING role carries no per-agent bound, so a gating agent that hangs is never swept
/// (the liveness watchdog only times out a spawn with a resolved bound - see
/// [`resolve_wall_clocks`], which no-ops on a `0` default). Returns the warning line naming
/// the unbounded gating roles and the fix, or `None` when the risk is absent (a bounded
/// default, or every gating role sets its own `max_wall_clock`).
///
/// Reuses the single [`gating_agent_ids`] authority - the SAME gating-role set the
/// verdict-line lint ([`lint_gating_verdict_lines`]) inspects - so "which roles gate" is
/// defined once, never a second parallel definition. Non-fatal by construction: the caller
/// (`rigger validate`) prints it to stderr without changing the exit status, like the other
/// advisories.
pub fn unbounded_wall_clock_advisory(cfg: &Config) -> Option<String> {
    // Only the unbounded default is at risk: a non-zero default is folded onto every unset
    // agent at load time, so every gating role is already swept.
    if cfg.workflow.defaults.max_wall_clock != 0 {
        return None;
    }
    // The gating roles left unbounded under that `0` default. `gating_agent_ids` returns a
    // sorted `BTreeSet`, so the collected roster is deterministic. A gating id absent from
    // `agents` is a referential error (`Config::validate`), reported there, not here.
    let unbounded: Vec<String> = gating_agent_ids(cfg)
        .into_iter()
        .filter(|id| {
            cfg.agents
                .get(id)
                .is_some_and(|a| a.max_wall_clock.is_none())
        })
        .map(|id| format!("{id:?}"))
        .collect();
    if unbounded.is_empty() {
        return None;
    }
    Some(format!(
        "warning: defaults.max_wall_clock is unbounded (0) and gating role(s) {} carry no \
         per-agent bound, so a hung gating agent is never swept and the run stalls silently. \
         Set defaults.max_wall_clock (seconds), or a per-agent max_wall_clock on those roles.",
        unbounded.join(", ")
    ))
}

/// Whole words (lowercased) that present their clause as the agent's OUTPUT/RESULT - the
/// place the integration gate reads. Within a `{"verdict"` literal's own introducing clause
/// (see [`introducing_clause`]) any of these means the literal is presented as output, so it
/// is on the result channel even if an emit instruction shares that same clause. Over-
/// inclusion here is SAFE: it can only make a prompt PASS the lint (a false negative, which
/// is acceptable), never fail a compliant one. This whitelist is only ONE of three PASS signals
/// (alongside "no emit in the clause" and "the literal is presented AS the verdict" - see
/// [`is_result_channel_occurrence`]); a compliant clause need not hit it, because a literal is
/// flagged only when it genuinely follows an emit-payload construct.
const OUTPUT_CUES: &[&str] = &[
    "end",
    "ends",
    "output",
    "outputs",
    "result",
    "results",
    "final",
    "last",
    "line",
    "lines",
    "print",
    "prints",
    "return",
    "returns",
    "respond",
    "response",
    "reply",
    "stdout",
    "conclude",
    "concludes",
    "conclusion",
    "write",
    "writes",
    "written",
    "finish",
    "finishes",
    "answer",
    "answers",
    "state",
    "states",
    "give",
    "gives",
    "provide",
    "provides",
    "close",
    "closing",
];

/// Whole words (lowercased) that mark their clause as a `rigger_emit` instruction (the
/// literal `rigger_emit` itself is matched separately as a substring). An emit token in a
/// `{"verdict"` literal's clause is NECESSARY but not sufficient for the stall: the literal is
/// flagged only when it GENUINELY follows an emit-payload construct (see
/// [`literal_is_emit_payload`]), never merely because an unrelated emit word shares the clause.
const EMIT_CUES: &[&str] = &["emit", "emits", "emitted", "emitting"];

/// Whole words (lowercased) that join a trailing verdict literal to an earlier payload-bound
/// verdict literal as a list ALTERNATION (`... data {"verdict":"approve"} to approve OR
/// {"verdict":"reject"} to reject`). When the trailing literal abuts one of these and an earlier
/// `{"verdict"` sibling is the emit payload, the trailing literal is a payload alternative too (see
/// [`trailing_literal_is_payload_alternation`]). Kept narrow to genuine connectives so an ordinary
/// word before the literal never triggers the alternation path.
const ALT_CONNECTIVES: &[&str] = &["or", "and", "nor"];

/// Whole words (lowercased) that name the DATA/PAYLOAD slot of a `rigger_emit` call - the
/// argument the emit verb serializes. A `{"verdict"` literal is GENUINELY the emit payload only
/// when one of these words IMMEDIATELY introduces its own brace after an emit token
/// (`rigger_emit ... data {..}` - see [`brace_is_payload_bound`]),
/// the stall this lint targets. This list is the crux of false-positive freedom, so it is kept to
/// GENUINE emit-API tokens (the words a persona uses for the serialized argument) and deliberately
/// EXCLUDES ambiguous common nouns - `value`/`values`/`object`/`content`/`contents`/`field`/
/// `fields` - that read naturally as the verdict OUTPUT itself ("your verdict value is {..}", "the
/// verdict object", "report the value {..}"). Including those over-bound compliant personas
/// (adj-u18-1rr REJECT); narrowing them out biases the ambiguous case to PASS (Design L32). Also
/// excludes `json`/`type`/`with`, which appear in compliant "the JSON {..}" / "type Verdict"
/// phrasings.
const PAYLOAD_SLOT_WORDS: &[&str] = &[
    "data",
    "payload",
    "body",
    "argument",
    "arguments",
    "arg",
    "args",
    "param",
    "params",
    "parameter",
    "parameters",
];

/// Whole words (lowercased) that, when they immediately precede a `verdict` word, mark it as the
/// SUBJECT being presented ("your verdict", "the verdict", "a final verdict") - so the literal
/// after it is the verdict OUTPUT, not the emit payload. Excludes the emit type name `type
/// Verdict` (preceded by `type`, not a determiner) and the JSON key `{"verdict"` (preceded by a
/// quote). Generous by design: a wider set only makes more prompts PASS (an acceptable false
/// negative), never fails a compliant one (spec 18 unit 1: never a false positive).
const VERDICT_DETERMINERS: &[&str] = &[
    "your",
    "the",
    "a",
    "an",
    "its",
    "my",
    "our",
    "their",
    "final",
    "single",
    "one",
    "own",
    "overall",
    "closing",
    "last",
    "this",
    "that",
    "following",
    "resulting",
    "chosen",
    "actual",
];

/// Whether `prompt` puts a `{"verdict"...}` literal on the RESULT channel (what the
/// integration gate reads), rather than exclusively as the payload of a `rigger_emit`
/// instruction.
///
/// Returns `true` (compliant) when at least one `{"verdict"` literal is presented as output -
/// carrying an [`OUTPUT_CUES`] word in its clause, presented AS the verdict in its clause, or
/// with no emit instruction in its clause at all (a standalone example, or one whose only nearby
/// emit mention lives in a neighbouring sentence). Returns `false` (the stall this lint targets)
/// only when the prompt contains `{"verdict"` literals and EVERY one of them genuinely follows an
/// emit-payload construct in its own clause (see [`is_result_channel_occurrence`]). A prompt with
/// NO `{"verdict"` literal returns `true`: there is no literal to judge, and the lint deliberately
/// trades that false negative away to keep its promise that a prompt which DOES put the verdict on
/// the result is never flagged.
fn puts_verdict_on_result_channel(prompt: &str) -> bool {
    let lower = prompt.to_lowercase();
    let positions = verdict_literal_positions(&lower);
    if positions.is_empty() {
        return true;
    }
    positions
        .iter()
        .any(|&pos| is_result_channel_occurrence(&lower, pos))
}

/// Byte offsets in `lower` (an already-lowercased prompt) of every `{"verdict"` JSON
/// literal - a `"verdict"` key whose nearest preceding non-whitespace character is `{`.
/// This matches `{"verdict"`, `{ "verdict"`, and a `{` then a newline then `"verdict"`.
fn verdict_literal_positions(lower: &str) -> Vec<usize> {
    lower
        .match_indices("\"verdict\"")
        .filter(|(idx, _)| lower[..*idx].trim_end().ends_with('{'))
        .map(|(idx, _)| idx)
        .collect()
}

/// Whether the `{"verdict"` literal at byte offset `pos` in `lower` reads as a
/// result-channel verdict.
///
/// The judgement is scoped to the literal's OWN introducing clause (the sentence that presents
/// it - see [`introducing_clause`]). This is the crux of false-positive freedom (spec 18 unit
/// 1's one hard promise): rigger's own communication discipline tells every gating persona to
/// record decisions via `rigger_emit`, so an emit instruction is near-universal. The literal is
/// bound to emit - flagged as the stall - ONLY when it GENUINELY follows an emit-payload
/// construct; every other shape biases to PASS. A literal is on the result channel when:
/// - an OUTPUT cue appears in the clause -> presented as output, even if an emit instruction
///   shares the clause; or
/// - the clause has NO emit instruction at all (a standalone example, or the only nearby emit
///   mention lives in a neighbouring sentence); or
/// - the clause presents the literal AS the verdict (a determiner-preceded `verdict` word whose
///   span to the literal carries no emit-payload marker - `your verdict must be {..}`), even
///   though an unrelated emit instruction ("emit a DecisionMade") shares the sentence.
///
/// It is the stall ONLY when the clause has an emit instruction, no output cue, does not present
/// the literal as the verdict, AND the literal directly follows an emit-payload construct
/// (`rigger_emit ... data {..}` or a bare `emit {..}` - see [`literal_is_emit_payload`]). This
/// is what the earlier "any emit word in the clause" heuristic got wrong: it flagged a compliant
/// "you must emit a DecisionMade ... and your verdict must be {..}" because an unrelated emit
/// word shared the sentence, even though that emit governs a DIFFERENT target and the verdict is
/// independently presented as output.
fn is_result_channel_occurrence(lower: &str, pos: usize) -> bool {
    let clause = introducing_clause(lower, pos);
    // (a) An output cue anywhere in the clause presents the literal as output.
    if OUTPUT_CUES.iter().any(|w| contains_word(clause, w)) {
        return true;
    }
    // (c) No emit instruction in the clause at all: standalone example, or the only nearby emit
    // mention is in a neighbouring sentence - the literal is on the result channel.
    if !clause_mentions_emit(clause) {
        return true;
    }
    // (b) The literal is presented AS the verdict ("your verdict is {..}"), not as the emit
    // payload, even though an (unrelated) emit instruction shares the clause.
    if verdict_presented_as_output(clause) {
        return true;
    }
    // The clause has an emit instruction, no output cue, and does not present the literal as the
    // verdict. Flag it ONLY when the literal GENUINELY follows an emit-payload construct; every
    // other ambiguous shape biases to PASS (Design L32 / done-when L111: never a false positive).
    !literal_is_emit_payload(clause)
}

/// Whether `clause` mentions any `rigger_emit` instruction - the literal `rigger_emit` substring
/// or an [`EMIT_CUES`] whole word (`emit`/`emits`/`emitted`/`emitting`).
fn clause_mentions_emit(clause: &str) -> bool {
    clause.contains("rigger_emit") || EMIT_CUES.iter().any(|w| contains_word(clause, w))
}

/// Whether `clause` presents its trailing `{"verdict"` literal AS the verdict output: it
/// contains a `verdict` whole word introduced by a determiner ([`VERDICT_DETERMINERS`]) - `your
/// verdict`, `the verdict`, `a final verdict` - whose SPAN to the trailing literal is free of an
/// emit-payload binding, so the literal is NOT the serialized emit data.
///
/// The determiner-verdict presents the trailing literal ONLY when the run of text from that
/// `verdict` word to the literal carries NO [`PAYLOAD_SLOT_WORDS`]/emit token immediately
/// introducing a brace (see [`span_has_emit_payload_binding`]). Scoping the emit-payload test to
/// this SPAN - not the whole clause - is the crux of adj-u18-1r3 FP#1: an UNRELATED emit-payload
/// EXAMPLE brace EARLIER in the clause (`... rigger_emit with data {id}, and your verdict is {..}`,
/// the exact wording rigger's own communication discipline mandates of every gating persona) sits
/// BEFORE the `verdict` word, so it no longer defeats the presentation. A payload word abutting the
/// trailing literal itself (`your verdict as data {..}`) IS in the span and still defeats it - the
/// genuine EMIT_ONLY stall. A payload common-noun used descriptively in the span but not abutting a
/// brace (`the verdict payload is {..}`, `your verdict value is {..}`) is not a binding, so it does
/// NOT defeat the presentation (adj-u18-1rr). The determiner requirement excludes the emit type name
/// `type Verdict` and the JSON key `{"verdict"`. Reached only when the clause already mentions emit
/// (signal c passed), so a span binding genuinely marks the literal as that emit's data.
fn verdict_presented_as_output(clause: &str) -> bool {
    for idx in whole_word_positions(clause, "verdict") {
        // Skip a JSON-key `verdict` (`{"verdict"` / `"verdict"`): its immediate predecessor is a
        // double quote, never a determiner.
        if clause[..idx].ends_with('"') {
            continue;
        }
        let Some(det) = preceding_word(clause, idx) else {
            continue;
        };
        if !VERDICT_DETERMINERS.contains(&det) {
            continue;
        }
        // The determiner-verdict presents the trailing literal only when the span from this
        // `verdict` word onward carries no emit-payload binding. An example brace EARLIER in the
        // clause is outside this span and cannot defeat the presentation.
        let span_start = idx + "verdict".len();
        if !span_has_emit_payload_binding(&clause[span_start..]) {
            return true;
        }
    }
    false
}

/// Whether the trailing `{"verdict"` literal of `clause` is GENUINELY the emit payload - the narrow
/// shape that is a real stall. Reached (signal d) only when the clause already has an emit
/// instruction, no output cue, and does not present the literal as the verdict, so every OTHER
/// ambiguous shape has already biased to PASS.
///
/// The literal is the emit payload when EITHER:
/// - its OWN brace directly follows a [`PAYLOAD_SLOT_WORDS`]/emit token (`... data {..}`,
///   `rigger_emit {..}`), the direct `emit ... data {verdict}` stall; OR
/// - it is a disjunctive/conjunctive ALTERNATION continuation of an earlier payload-bound verdict
///   literal (`... data {"verdict":"approve"} to approve or {"verdict":"reject"} to reject`), where
///   the trailing literal abuts a connective (`or`/`and`/`nor`) rather than the payload word
///   directly, yet is still the emit's serialized argument (see
///   [`trailing_literal_is_payload_alternation`]).
///
/// Scoping the direct test to the trailing literal's OWN brace (not any brace in the clause) is what
/// keeps an UNRELATED `data {id}` example brace from marking a non-payload literal as emit data
/// (adj-u18-1r3 FP#1). A clause with no emit instruction is never the emit payload.
fn literal_is_emit_payload(clause: &str) -> bool {
    if last_emit_end(clause).is_none() {
        return false;
    }
    trailing_brace_is_payload_bound(clause) || trailing_literal_is_payload_alternation(clause)
}

/// Whether the `{` at byte offset `idx` in `s` is IMMEDIATELY introduced by an emit-payload marker -
/// a [`PAYLOAD_SLOT_WORDS`] word, `rigger_emit`, or an [`EMIT_CUES`] word (`data {..}`,
/// `rigger_emit {..}`, `emit {..}`). The single "this brace is the serialized emit argument" test.
fn brace_is_payload_bound(s: &str, idx: usize) -> bool {
    matches!(preceding_word(s, idx), Some(w)
        if PAYLOAD_SLOT_WORDS.contains(&w) || w == "rigger_emit" || EMIT_CUES.contains(&w))
}

/// Whether `span` contains ANY brace immediately introduced by an emit-payload marker (see
/// [`brace_is_payload_bound`]). Unlike [`literal_is_emit_payload`] it does NOT require an emit token
/// to also appear in `span`: it reads only the payload marker abutting a brace, because the emit
/// instruction may sit BEFORE `span` (`rigger_emit your verdict as data {..}` - the emit precedes the
/// determiner-verdict word, the `data {..}` binding follows it). This is the span-scoped test
/// [`verdict_presented_as_output`] applies from the `verdict` word to the trailing literal.
fn span_has_emit_payload_binding(span: &str) -> bool {
    span.match_indices('{')
        .any(|(idx, _)| brace_is_payload_bound(span, idx))
}

/// Whether the trailing `{` of `clause` (the trailing verdict literal's own brace) directly follows
/// an emit-payload marker (see [`brace_is_payload_bound`]).
fn trailing_brace_is_payload_bound(clause: &str) -> bool {
    clause
        .rfind('{')
        .is_some_and(|idx| brace_is_payload_bound(clause, idx))
}

/// Whether the trailing verdict literal of `clause` is a list ALTERNATION continuation of an EARLIER
/// verdict literal that is itself the emit payload - the `... data {"verdict":"approve"} to approve
/// or {"verdict":"reject"} to reject` shape. The trailing literal abuts a connective
/// ([`ALT_CONNECTIVES`]) rather than the payload word directly, but an earlier `{"verdict"` brace in
/// the clause IS payload-bound, so both literals are the serialized emit alternatives and the whole
/// persona is the emit-only stall. Requiring the earlier sibling to be a `{"verdict"` literal (not
/// any payload-bound brace) keeps an unrelated `data {id}` example from making a connective-joined
/// trailing literal look like an alternation (biases the ambiguous case to PASS).
fn trailing_literal_is_payload_alternation(clause: &str) -> bool {
    let Some(last) = clause.rfind('{') else {
        return false;
    };
    let abuts_connective =
        matches!(preceding_word(clause, last), Some(w) if ALT_CONNECTIVES.contains(&w));
    if !abuts_connective {
        return false;
    }
    clause
        .match_indices('{')
        .filter(|(idx, _)| *idx < last)
        .filter(|(idx, _)| clause[idx + 1..].trim_start().starts_with("\"verdict\""))
        .any(|(idx, _)| brace_is_payload_bound(clause, idx))
}

/// Byte offset just past the LAST emit token in `clause` (`rigger_emit` substring or an
/// [`EMIT_CUES`] whole word), or `None` if the clause mentions no emit instruction. The last
/// (nearest) emit token gives the narrowest emit->literal window, biasing the payload test toward
/// PASS.
fn last_emit_end(clause: &str) -> Option<usize> {
    let mut end = clause.rfind("rigger_emit").map(|i| i + "rigger_emit".len());
    for cue in EMIT_CUES {
        for start in whole_word_positions(clause, cue) {
            let e = start + cue.len();
            end = Some(end.map_or(e, |cur: usize| cur.max(e)));
        }
    }
    end
}

/// The introducing clause of the `{"verdict"` literal at byte offset `pos`: the run of text
/// from the nearest preceding clause boundary (`.`, `!`, `?`, `;`, or a line break) up to the
/// literal. This is the single instruction that presents the literal, scoped to one sentence
/// so an emit mention in a neighbouring sentence cannot bind it. `rfind` returns a char
/// boundary, so slicing a multi-byte prompt never panics.
fn introducing_clause(lower: &str, pos: usize) -> &str {
    let start = lower[..pos]
        .rfind(['.', '!', '?', ';', '\n', '\r'])
        .map(|i| i + 1)
        .unwrap_or(0);
    &lower[start..pos]
}

/// Start byte offsets of every WHOLE-word occurrence of `word` (lowercase ASCII) in `hay`
/// (already lowercased) - bounded on both sides by a non-word byte (anything that is not
/// `[A-Za-z0-9_]`). This is the single whole-word scanner the lint uses; [`contains_word`] is the
/// any-match view over it. Whole-word matching is what gives the lint teeth without false alarms:
/// it catches "end" in "end your output" but not in "append"/"recommend", and "emit" as its own
/// word but not buried inside "rigger_emit" (matched separately as a substring).
fn whole_word_positions(hay: &str, word: &str) -> Vec<usize> {
    let bytes = hay.as_bytes();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(rel) = hay[from..].find(word) {
        let start = from + rel;
        let end = start + word.len();
        let before_ok = start == 0 || !is_word_byte(bytes[start - 1]);
        let after_ok = end == bytes.len() || !is_word_byte(bytes[end]);
        if before_ok && after_ok {
            out.push(start);
        }
        from = start + 1;
    }
    out
}

/// Whether `word` occurs in `hay` as a WHOLE word (see [`whole_word_positions`]).
fn contains_word(hay: &str, word: &str) -> bool {
    !whole_word_positions(hay, word).is_empty()
}

/// The whole word (lowercase ASCII) immediately preceding byte offset `idx` in `s`, skipping any
/// run of non-word bytes (spaces, punctuation, quotes) between them; `None` if there is no
/// preceding word. Used to read the determiner in front of a `verdict` subject. Boundaries land
/// on non-word bytes (multi-byte UTF-8 bytes are all `>= 0x80`, hence non-word), so slicing never
/// splits a char and never panics.
fn preceding_word(s: &str, idx: usize) -> Option<&str> {
    let bytes = s.as_bytes();
    let mut end = idx;
    while end > 0 && !is_word_byte(bytes[end - 1]) {
        end -= 1;
    }
    if end == 0 {
        return None;
    }
    let mut start = end;
    while start > 0 && is_word_byte(bytes[start - 1]) {
        start -= 1;
    }
    Some(&s[start..end])
}

/// Whether `b` is a word byte (`[A-Za-z0-9_]`) for [`contains_word`]'s boundary test.
fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
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

    // ---- spec 18, unit 1: gating-persona verdict-line static lint ----

    fn agent(id: &str, prompt: &str) -> AgentDef {
        AgentDef {
            id: id.into(),
            prompt: prompt.into(),
            ..Default::default()
        }
    }

    /// A config whose default review panel gates on an adjudicator carrying `prompt`.
    fn config_with_default_adjudicator(prompt: &str) -> Config {
        let mut cfg = Config::default();
        cfg.agents.insert("adj".into(), agent("adj", prompt));
        cfg.workflow.defaults.review.adjudicator = "adj".into();
        cfg
    }

    /// An adjudicator persona whose ONLY verdict path is `rigger_emit`: the guaranteed
    /// stall this lint targets (the gate reads the result channel and finds no verdict).
    const EMIT_ONLY: &str = "You are the Adjudicator. Weigh the lenses against the adversary \
        and decide. Record your verdict the moment you reach it via the rigger_emit tool with \
        type Verdict and data {\"verdict\":\"approve\"} to approve or {\"verdict\":\"reject\"} \
        to reject. Do not add anything after you emit.";

    /// The compliant twin of [`EMIT_ONLY`]: it still records reasoning via `rigger_emit` but
    /// ENDS ITS OUTPUT with the verdict line the integration gate actually reads.
    const RESULT_LINE: &str = "You are the Adjudicator. Weigh the lenses against the adversary \
        and decide. Record your reasoning via the rigger_emit tool as you go. End your output \
        with a single line: {\"verdict\":\"approve\"} to approve or {\"verdict\":\"reject\"} to \
        reject.";

    #[test]
    fn verdict_line_matcher_flags_emit_only_and_passes_a_result_line_prompt() {
        assert!(
            !puts_verdict_on_result_channel(EMIT_ONLY),
            "a persona that records the verdict ONLY via rigger_emit puts no verdict on the \
             result channel"
        );
        assert!(
            puts_verdict_on_result_channel(RESULT_LINE),
            "a persona that ends its output with the verdict line is compliant (even though it \
             also emits reasoning)"
        );
    }

    #[test]
    fn verdict_line_matcher_never_flags_a_prompt_that_puts_the_verdict_on_the_result() {
        // A standalone JSON example with no emit instruction near it is on the result channel.
        assert!(puts_verdict_on_result_channel(
            "Decide, then write {\"verdict\":\"approve\"} and stop."
        ));
        // A prompt with no verdict literal at all is not flagged: there is no literal to judge,
        // and the lint trades that false negative away to never false-positive a compliant one.
        assert!(puts_verdict_on_result_channel(
            "Adjudicate the unit and record your decision."
        ));
    }

    /// spec 18, unit 1 hard promise (Design L32 / done-when L111): the lint may have false
    /// NEGATIVES but never a false POSITIVE - a persona that DOES present the verdict as its
    /// output must never be flagged. This pins that promise against the exact class the
    /// heuristic used to break on: a COMPLIANT verdict-line clause that happens to use words
    /// outside the output whitelist, while an UNRELATED `rigger_emit` instruction sits in a
    /// neighbouring sentence (rigger's own communication discipline puts one in every gating
    /// persona). Each of these presents `{"verdict"...}` AS OUTPUT and must pass.
    #[test]
    fn verdict_line_lint_never_false_positives_a_compliant_but_non_whitelisted_persona() {
        // Every string below is emit-adjacent (a `rigger_emit` / emit instruction is nearby,
        // as the discipline requires) yet presents the verdict literal as its OUTPUT, using
        // vocabulary the fixed output whitelist does not contain (finish/answer/closing/write).
        let compliant = [
            "Emit your reasoning as you go via rigger_emit. Then write your verdict as the \
             closing JSON: {\"verdict\":\"approve\"}",
            "Your verdict is the JSON {\"verdict\":\"approve\"}; also emit a DecisionMade as you \
             go.",
            "Weigh the lenses, record notes via rigger_emit. Finish with the JSON \
             {\"verdict\":\"approve\"}.",
            "Record your reasoning via rigger_emit. Your closing JSON must be \
             {\"verdict\":\"approve\"}.",
            "Emit each decision via rigger_emit. Write {\"verdict\":\"approve\"} as the closing \
             token.",
            "Deliberate and emit your reasoning via rigger_emit. Your answer is \
             {\"verdict\":\"approve\"}.",
        ];
        for prompt in compliant {
            assert!(
                puts_verdict_on_result_channel(prompt),
                "a persona that presents the verdict AS OUTPUT must never be flagged, even when \
                 an emit instruction sits in a neighbouring sentence and the verdict clause \
                 avoids the output whitelist:\n{prompt}"
            );
        }
        // The stall this lint targets is unchanged: a verdict literal that is genuinely the
        // payload of an emit call in its OWN clause, with no output cue, stays flagged.
        assert!(
            !puts_verdict_on_result_channel(EMIT_ONLY),
            "a persona whose verdict literal is the rigger_emit payload in its own clause is \
             still the stall this lint refuses"
        );
    }

    /// spec 18 unit 1's hard promise, pinned against the RESIDUAL false-positive class the prior
    /// fix left reachable (adj-u18-1 REJECT / adv-u18-1-residual-false-positive-same-clause-emit):
    /// an unrelated emit instruction sharing the SAME sentence as a verdict-output clause must NOT
    /// bind the literal. The earlier "any emit word in the clause" rule flagged these because an
    /// emit word (governing a DIFFERENT target - a DecisionMade, reasoning) sat in the clause with
    /// no whitelisted output cue; each nonetheless presents `{"verdict"...}` AS the verdict output
    /// and must PASS. None of these strings hit [`OUTPUT_CUES`], so they pin the emit-payload
    /// binding itself, not the whitelist. Compliance must NOT flip on clause order.
    #[test]
    fn verdict_line_lint_passes_an_unrelated_same_sentence_emit_before_a_verdict_output_clause() {
        let compliant = [
            // The exact string the adjudicator's directive named: an unrelated `emit a
            // DecisionMade` instruction precedes a non-whitelisted verdict-output clause.
            "You must emit a DecisionMade for each call and your verdict must be \
             {\"verdict\":\"approve\"}.",
            // Emit-clause first, verdict-output clause second, one sentence (no cue word).
            "Emit each decision via rigger_emit, then your verdict is the JSON \
             {\"verdict\":\"approve\"}.",
            "Having emitted your reasoning, your verdict {\"verdict\":\"approve\"} governs.",
            // The ORDER-FLIP of a previously-blessed string: verdict-first passed before; the
            // emit-first reordering must pass too - compliance cannot flip on clause order.
            "Also emit a DecisionMade as you go and your verdict is the JSON \
             {\"verdict\":\"approve\"}.",
            "Record each DecisionMade via rigger_emit as you go, and your verdict is \
             {\"verdict\":\"approve\"}.",
            "After you emit your DecisionMade events, your verdict is {\"verdict\":\"approve\"}.",
            "Deliver two things: reasoning emitted via rigger_emit, and the verdict \
             {\"verdict\":\"approve\"}.",
        ];
        for prompt in compliant {
            assert!(
                puts_verdict_on_result_channel(prompt),
                "an unrelated emit instruction in the same sentence must not bind a verdict that \
                 is independently presented as output:\n{prompt}"
            );
        }
        // Teeth preserved even IN-sentence: when the emit verb GENUINELY serializes the verdict
        // as its `data` payload (not an unrelated target), it is still the stall - regardless of a
        // determiner-`verdict` phrase, because the span to the literal carries the payload marker.
        for stall in [
            "Emit a note, then rigger_emit your verdict as data {\"verdict\":\"approve\"}.",
            "Record notes and rigger_emit the verdict with data {\"verdict\":\"approve\"}.",
        ] {
            assert!(
                !puts_verdict_on_result_channel(stall),
                "a verdict serialized as the emit `data` payload is still the stall:\n{stall}"
            );
        }
    }

    /// spec 18 unit 1's hard promise, pinned against the PAYLOAD-SLOT residual false-positive class
    /// (adj-u18-1rr REJECT / adv-u18-1rr-residual-fp-defeats-your-verdict-escape): an explicit
    /// determiner-`verdict` presentation ("your verdict ... is {..}") is on the result channel even
    /// when a payload-slot noun sits in the span but does NOT abut the literal. The prior rule
    /// treated ANY payload-slot word in the span as defeating the presentation, so it flagged these
    /// clearly-compliant personas; only a payload word IMMEDIATELY abutting the literal ("as data
    /// {..}") marks it as the serialized emit data. None of these strings carries an output-cue
    /// word, and each shares its sentence with an unrelated emit instruction, so they pin the
    /// determiner-verdict escape against a payload noun in the span - not the output whitelist.
    #[test]
    fn verdict_line_lint_passes_a_determiner_verdict_when_a_payload_noun_sits_in_the_span() {
        let compliant = [
            // A dropped ambiguous common noun ("value"/"object") after the verdict subject.
            "Emit each decision via rigger_emit and your verdict value is {\"verdict\":\"approve\"}.",
            "Emit each decision via rigger_emit and your verdict object is \
             {\"verdict\":\"approve\"}.",
            // A KEPT emit-API token ("payload"/"data") that does NOT abut the literal - descriptive
            // here, not the serialized argument, so the determiner-verdict presentation stands.
            "Emit each decision via rigger_emit and the verdict payload is \
             {\"verdict\":\"approve\"}.",
            "Record notes via rigger_emit as you go, and your verdict data is \
             {\"verdict\":\"approve\"}.",
        ];
        for prompt in compliant {
            assert!(
                puts_verdict_on_result_channel(prompt),
                "a determiner-verdict presentation is on the result channel even when a payload \
                 noun sits in its span but does not abut the literal:\n{prompt}"
            );
        }
        // Teeth: when the payload word IMMEDIATELY abuts the literal after an emit, the verdict IS
        // the serialized emit data - still the stall, even behind a determiner-`verdict` phrase.
        for stall in [
            "Emit a note, then rigger_emit your verdict as data {\"verdict\":\"approve\"}.",
            "Record decisions, then rigger_emit the verdict payload {\"verdict\":\"approve\"}.",
        ] {
            assert!(
                !puts_verdict_on_result_channel(stall),
                "a verdict literal a payload word directly introduces is the serialized emit data \
                 - still the stall:\n{stall}"
            );
        }
    }

    /// spec 18 unit 1's hard promise, pinned against the UNRELATED-EMIT-EXAMPLE-BRACE false-positive
    /// class (adj-u18-1r3 REJECT, FP#1): an explicit determiner-`verdict` presentation
    /// ("your verdict is {..}") is on the result channel even when an UNRELATED emit-payload EXAMPLE
    /// brace ("... data {id} ...") shares its clause EARLIER, before the `verdict` word. The prior
    /// rule scanned EVERY brace in the clause, so a different literal's `data {id}` example
    /// short-circuited the determiner-verdict escape to a false flag - it fired on the EXACT wording
    /// rigger's own communication discipline mandates of every gating persona. The fix scopes the
    /// emit-payload test to the SPAN from the `verdict` word to the trailing literal, so an example
    /// brace before the presentation no longer defeats it. None of these strings carries an output
    /// cue, and each shares its sentence with a genuine `rigger_emit ... data {id}` example.
    #[test]
    fn verdict_line_lint_passes_a_determiner_verdict_when_an_unrelated_emit_example_brace_precedes_it(
    ) {
        let compliant = [
            // A/B delta of the reject: only difference from a passing twin is an inline `data {id}`
            // example after the emit token; the determiner-verdict escape must survive it.
            "Record each decision via rigger_emit with data {id}, and your verdict is \
             {\"verdict\":\"approve\"}.",
            // The A-side twin (no example brace) - a control that must also pass.
            "Record each decision via rigger_emit with a DecisionMade payload, and your verdict is \
             {\"verdict\":\"approve\"}.",
            // The EXACT rigger DecisionMade-discipline wording mandated of every gating persona: an
            // emit-payload example `data {id,summary}` precedes the determiner-verdict presentation.
            "Record every decision via rigger_emit with type DecisionMade and data {id,summary}, \
             then your verdict is {\"verdict\":\"approve\"}.",
            // The example brace can even be a `{id}` immediately after `rigger_emit`; still unrelated
            // to the trailing determiner-verdict literal.
            "Emit each decision as rigger_emit {id}, and your verdict is {\"verdict\":\"approve\"}.",
        ];
        for prompt in compliant {
            assert!(
                puts_verdict_on_result_channel(prompt),
                "an unrelated emit-payload EXAMPLE brace earlier in the clause must not defeat a \
                 determiner-verdict presentation of the trailing literal:\n{prompt}"
            );
        }
        // Teeth preserved: when the payload word abuts the TRAILING verdict literal itself (in the
        // presentation span), it IS the serialized emit data - still the stall, even behind a
        // determiner-`verdict` phrase and even with an unrelated example brace elsewhere.
        for stall in [
            "Record notes via rigger_emit {id}, then rigger_emit your verdict as data \
             {\"verdict\":\"approve\"}.",
            "Emit a note with data {id}, then rigger_emit the verdict with data \
             {\"verdict\":\"approve\"}.",
        ] {
            assert!(
                !puts_verdict_on_result_channel(stall),
                "a verdict literal a payload word directly introduces IN the presentation span is \
                 the serialized emit data - still the stall:\n{stall}"
            );
        }
    }

    /// spec 18 unit 1 residual class (adj-u18-1rr): a natural output verb OUTSIDE the fixed output
    /// whitelist ("report"/"deliver"/"send"), whose object is a dropped ambiguous common noun
    /// ("the value {..}"/"the object {..}"), presents the verdict as output and must PASS. Each
    /// string reaches the final emit-payload test (signal d: an emit instruction in the clause, no
    /// output cue, no determiner-verdict) and passes ONLY because the literal is NOT the serialized
    /// emit data. This pins the `literal_is_emit_payload == false` direction directly (mutation gap
    /// adv/sdet-u18-1r-emit-payload-narrowing-unpinned): forcing it TRUE turns every line RED.
    #[test]
    fn verdict_line_lint_passes_a_natural_output_verb_only_via_the_literal_not_being_emit_payload()
    {
        let compliant = [
            "Emit your reasoning via rigger_emit, then report the value {\"verdict\":\"approve\"}.",
            "Emit your reasoning via rigger_emit, then deliver the object {\"verdict\":\"approve\"}.",
            "Emit your reasoning via rigger_emit, then send the value {\"verdict\":\"approve\"}.",
            // A KEPT emit-API token ("data") in the clause but not abutting the literal.
            "Emit your reasoning as structured data via rigger_emit, then {\"verdict\":\"approve\"}.",
        ];
        for prompt in compliant {
            assert!(
                puts_verdict_on_result_channel(prompt),
                "a natural output verb outside the whitelist, whose object is not an emit \
                 payload-slot word abutting the literal, presents the verdict as output - it must \
                 PASS:\n{prompt}"
            );
        }
    }

    /// Direct pin on the emit-payload recognizer (mutation gap adv/sdet-u18-1r-emit-payload-
    /// narrowing-unpinned): the literal is the serialized emit data ONLY when a payload-slot word
    /// (or the emit verb) IMMEDIATELY abuts it, never when a payload noun sits earlier in the
    /// clause. This pins BOTH directions - mutating `literal_is_emit_payload` to a constant turns
    /// one arm RED. The argument is the lowercased introducing clause, which ends at the `{` of the
    /// literal (see [`introducing_clause`]).
    #[test]
    fn literal_is_emit_payload_binds_only_an_abutting_payload_or_emit_word() {
        // Abutting payload word after an emit -> genuinely the serialized emit data.
        assert!(literal_is_emit_payload(
            "record notes and rigger_emit the verdict with data {"
        ));
        // Bare emit abutting the literal -> the emit verb directly introduces it.
        assert!(literal_is_emit_payload("record notes, then rigger_emit {"));
        // Payload word present but NOT abutting the literal ("value is {") -> not the emit data.
        assert!(!literal_is_emit_payload(
            "emit each decision via rigger_emit and your verdict value is {"
        ));
        // A dropped common noun abutting the literal -> not a payload slot at all.
        assert!(!literal_is_emit_payload(
            "emit your reasoning via rigger_emit, then report the value {"
        ));
        // No emit instruction in the clause -> never the emit data, even with an abutting payload.
        assert!(!literal_is_emit_payload("here is the data {"));

        // Alternation continuation: the trailing literal abuts a connective (`or`) but an earlier
        // `{"verdict"` sibling IS payload-bound (`data {"verdict":"approve"}`), so it is the emit's
        // other serialized alternative - still the payload. This pins
        // `trailing_literal_is_payload_alternation`: neutralizing it turns the EMIT_ONLY alternation
        // GREEN (a false negative the reject demanded stay FLAGGED).
        assert!(literal_is_emit_payload(
            "rigger_emit with data {\"verdict\":\"approve\"} to approve or {"
        ));
        // But a connective-joined trailing literal whose earlier sibling is an UNRELATED example
        // brace (`data {id}`, not a `{"verdict"` literal) is NOT an alternation - biases to PASS.
        assert!(!literal_is_emit_payload("rigger_emit with data {id} or {"));
        // A connective with no payload-bound `{"verdict"` sibling at all is not an alternation.
        assert!(!literal_is_emit_payload(
            "emit each decision via rigger_emit, then finish with a or {"
        ));
    }

    #[test]
    fn lint_hard_errors_on_an_emit_only_gating_adjudicator_naming_the_fix() {
        let cfg = config_with_default_adjudicator(EMIT_ONLY);
        let msg = lint_gating_verdict_lines(&cfg).unwrap_err().to_string();
        assert!(msg.contains("\"adj\""), "names the offending agent: {msg}");
        assert!(
            msg.contains("gating role") && msg.contains("verdict line"),
            "names the defect: {msg}"
        );
        assert!(
            msg.contains("rigger_emit will never gate"),
            "names why an emit-only verdict never gates: {msg}"
        );
    }

    #[test]
    fn lint_passes_a_gating_adjudicator_that_ends_with_the_verdict_line() {
        let cfg = config_with_default_adjudicator(RESULT_LINE);
        assert!(lint_gating_verdict_lines(&cfg).is_ok());
    }

    #[test]
    fn the_verdict_line_lint_ignores_non_gating_roles() {
        // Only the adjudicator's result-channel verdict gates integration; an emit-only lens or
        // adversary is not a gating role, so the lint leaves it alone.
        let mut cfg = Config::default();
        cfg.agents.insert("adj".into(), agent("adj", RESULT_LINE));
        cfg.agents.insert("adv".into(), agent("adv", EMIT_ONLY));
        cfg.agents.insert("lens".into(), agent("lens", EMIT_ONLY));
        cfg.workflow.defaults.review.adjudicator = "adj".into();
        cfg.workflow.defaults.review.adversary = "adv".into();
        cfg.workflow.defaults.review.lenses = vec!["lens".into()];
        assert!(
            lint_gating_verdict_lines(&cfg).is_ok(),
            "a compliant adjudicator passes even when the adversary/lens personas are emit-only"
        );
    }

    #[test]
    fn the_verdict_line_lint_reaches_the_light_tier_adjudicator() {
        let mut cfg = Config::default();
        cfg.agents.insert("full".into(), agent("full", RESULT_LINE));
        cfg.agents.insert("light".into(), agent("light", EMIT_ONLY));
        cfg.workflow.defaults.review.adjudicator = "full".into();
        let mut depth = ReviewDepth::default();
        depth.light.adjudicator = "light".into();
        cfg.workflow.defaults.review.tiers = Some(Box::new(depth));
        let msg = lint_gating_verdict_lines(&cfg).unwrap_err().to_string();
        assert!(
            msg.contains("\"light\""),
            "the light-tier adjudicator is a gating role too: {msg}"
        );
    }

    #[test]
    fn the_verdict_line_lint_reaches_a_standalone_stage_adjudicator_that_is_not_a_critique_gate() {
        // A stage's own `adjudicator` renders a gating verdict, so its persona is linted like a
        // review-panel adjudicator. This stage is NOT the conductor-injected plan-critique gate
        // (there is no producer stage for it to `needs`, and `build_dag_critique_prompt` only
        // injects the verdict line for that gate), so its persona must carry the verdict line
        // itself and an emit-only one is flagged. The excluded critique-gate case is pinned
        // separately by `the_verdict_line_lint_excludes_the_conductor_injected_plan_critique_gate`.
        let mut cfg = Config::default();
        cfg.agents
            .insert("critic".into(), agent("critic", EMIT_ONLY));
        let st = Stage {
            adjudicator: "critic".into(),
            ..Default::default()
        };
        cfg.workflow.stages.insert("decision-gate".into(), st);
        let msg = lint_gating_verdict_lines(&cfg).unwrap_err().to_string();
        assert!(
            msg.contains("\"critic\""),
            "a standalone stage adjudicator that is not the injected critique gate is linted: {msg}"
        );
    }

    /// spec 18 unit 1's hard promise, pinned against the PLAN-CRITIQUE false-positive class
    /// (adj-u18-1r3 REJECT, FP#2): the conductor builds the plan-critique / DAG-critique gate
    /// adjudicator's prompt via `build_dag_critique_prompt`, which ALWAYS appends the result-channel
    /// verdict line ("Render your final verdict as a JSON line: {..}"). So that gate's persona need
    /// NOT carry the verdict line, and linting an emit-only DAG-critique adjudicator is a false
    /// positive that would REFUSE a legitimate run (unit 2 escalates the lint to a run-start
    /// refusal). `gating_agent_ids` excludes the critique-gate stage's standalone adjudicator; the
    /// per-unit review adjudicator (whose `build_prompt` injects nothing) stays linted.
    #[test]
    fn the_verdict_line_lint_excludes_the_conductor_injected_plan_critique_gate() {
        // A DEDICATED emit-only DAG-critique adjudicator, used ONLY at the plan-critique gate: the
        // conductor injects its verdict line, so it must NOT be flagged.
        let mut cfg = Config::default();
        cfg.agents
            .insert("planner".into(), agent("planner", RESULT_LINE));
        cfg.agents
            .insert("dag-critic".into(), agent("dag-critic", EMIT_ONLY));
        cfg.agents.insert("adj".into(), agent("adj", RESULT_LINE));
        // The producer stage (a planner that `produces` a DAG).
        let plan = Stage {
            agent: "planner".into(),
            produces: "dag".into(),
            ..Default::default()
        };
        cfg.workflow.stages.insert("plan".into(), plan);
        // The plan-critique gate: review-only (no `agent`), carries an adjudicator, needs the
        // producer - exactly `conductor::critique_gate_name`'s recognition.
        let gate = Stage {
            adjudicator: "dag-critic".into(),
            needs: vec!["plan".into()],
            ..Default::default()
        };
        cfg.workflow.stages.insert("plan-critique".into(), gate);
        // A compliant per-unit review adjudicator so the config is otherwise clean.
        cfg.workflow.defaults.review.adjudicator = "adj".into();
        assert!(
            lint_gating_verdict_lines(&cfg).is_ok(),
            "the conductor-injected plan-critique gate adjudicator must not be flagged for an \
             emit-only persona: {:?}",
            lint_gating_verdict_lines(&cfg)
        );

        // But if that SAME emit-only persona ALSO serves the per-unit review adjudicator (whose
        // build_prompt injects no verdict line), it is a real stall and stays flagged - the
        // exclusion is scoped to the critique-gate role, not the agent id everywhere.
        let mut cfg2 = cfg.clone();
        cfg2.workflow.defaults.review.adjudicator = "dag-critic".into();
        let msg = lint_gating_verdict_lines(&cfg2).unwrap_err().to_string();
        assert!(
            msg.contains("\"dag-critic\""),
            "an emit-only adjudicator that also gates per-unit review (build_prompt injects \
             nothing) is still flagged: {msg}"
        );
    }

    #[test]
    fn the_verdict_line_lint_reaches_a_stage_review_override_adjudicator() {
        // A per-stage `review:` override names its OWN adjudicator, a gating role spec 18
        // enumerates; the lint reaches it through `st.review.gating_agent_ids()`. This pins the
        // stage-review collection line directly (the standalone-adjudicator test above exercises a
        // different field), so dropping it can no longer ship green.
        let mut cfg = Config::default();
        cfg.agents
            .insert("sjudge".into(), agent("sjudge", EMIT_ONLY));
        let mut st = Stage::default();
        st.review.adjudicator = "sjudge".into();
        cfg.workflow.stages.insert("implement".into(), st);
        let msg = lint_gating_verdict_lines(&cfg).unwrap_err().to_string();
        assert!(
            msg.contains("\"sjudge\""),
            "a stage review-override adjudicator is a gating role too: {msg}"
        );
    }

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
    fn sdet_author_is_a_distinct_write_capable_agent_separate_from_the_read_only_sdet_lens() {
        // Spec 32 c1: a NEW write-capable SDET-AUTHOR role exists as the shipped
        // `.rigger/agents/sdet-author.md` with Edit/Write tools + `isolation: worktree`,
        // DISTINCT from the read-only `sdet` review lens (which has neither Edit nor Write).
        // Proven over the REAL committed files (read from the crate manifest dir), so this
        // asserts the artifact the loop actually spawns, not a fixture.
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let author = parse_agent(
            &std::fs::read(root.join(".rigger/agents/sdet-author.md"))
                .expect("the shipped .rigger/agents/sdet-author.md must exist"),
        )
        .expect("sdet-author.md parses as an agent definition");
        assert_eq!(author.id, "sdet-author");

        // Write-capable: it grants BOTH Edit and Write (case-insensitive), and neither is
        // stripped by fan-out filtering (they are not spawn tools), so an isolated spawn of
        // this agent keeps its authoring capability.
        let grants =
            |a: &AgentDef, tool: &str| a.tools.iter().any(|t| t.eq_ignore_ascii_case(tool));
        assert!(grants(&author, "Edit"), "sdet-author must grant Edit");
        assert!(grants(&author, "Write"), "sdet-author must grant Write");
        let allowed = author.allowed_tools();
        assert!(
            allowed.iter().any(|t| t.eq_ignore_ascii_case("Edit"))
                && allowed.iter().any(|t| t.eq_ignore_ascii_case("Write")),
            "Edit/Write survive fan-out stripping - the spawned author can actually write"
        );

        // `isolation: worktree`: it authors its periphery tests IN an isolated worktree.
        assert_eq!(author.isolation, "worktree");
        assert!(author.isolated());

        // DISTINCT from the read-only `sdet` review lens: a different agent id, and the lens
        // has NEITHER Edit nor Write - it only reviews, it cannot author. This is the
        // independence the spec demands: authorship and review are separate roles.
        let lens = parse_agent(
            &std::fs::read(root.join(".rigger/agents/sdet.md"))
                .expect("the shipped .rigger/agents/sdet.md must exist"),
        )
        .expect("sdet.md parses as an agent definition");
        assert_ne!(
            author.id, lens.id,
            "the author and the review lens are distinct agents"
        );
        assert!(
            !grants(&lens, "Edit") && !grants(&lens, "Write"),
            "the sdet review lens stays read-only (no Edit/Write) - it reviews, never authors"
        );

        // The role token exists in src/spawn.rs and names this agent's role.
        assert_eq!(crate::spawn::ROLE_SDET_AUTHOR, "sdet-author");
    }

    #[test]
    fn the_sdet_author_role_is_on_by_default_and_opt_out() {
        // Spec 32: the SDET-author role is ALWAYS-ON (config-driven, default on) and
        // self-scopes. The on-by-default rule must hold whether the `sdet_author:` field, or
        // the whole `defaults:` block, is omitted - so it is checked against the DERIVED
        // `Defaults::default()` (which a missing `defaults:` block constructs) AND both
        // from-YAML omission paths, not just a hand-set `None`.
        assert!(
            Defaults::default().sdet_author_enabled(),
            "the sdet-author role is on by default (derived Default)"
        );
        // A present-but-empty `defaults:` block: still on.
        let empty_defaults: Workflow = serde_yaml::from_str("name: w\ndefaults: {}\n").unwrap();
        assert!(empty_defaults.defaults.sdet_author_enabled());
        // No `defaults:` block at all: still on (this is exactly the case a bare `bool` with
        // a serde default-true would silently flip OFF, because the missing block builds the
        // derived Default).
        let no_defaults: Workflow = serde_yaml::from_str("name: w\n").unwrap();
        assert!(no_defaults.defaults.sdet_author_enabled());
        // Explicit opt-out: off.
        let off: Workflow =
            serde_yaml::from_str("name: w\ndefaults:\n  sdet_author: false\n").unwrap();
        assert!(
            !off.defaults.sdet_author_enabled(),
            "an explicit `sdet_author: false` opts the workflow out"
        );
        // Explicit opt-in: on.
        let on: Workflow =
            serde_yaml::from_str("name: w\ndefaults:\n  sdet_author: true\n").unwrap();
        assert!(on.defaults.sdet_author_enabled());
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

    // ---- spec 19c, unit 3: `rigger validate` warns on an unbounded default ----

    #[test]
    fn unbounded_wall_clock_advisory_fires_when_default_unbounded_and_a_gating_role_is_unbounded() {
        // `config_with_default_adjudicator` leaves `defaults.max_wall_clock` at its `0`
        // (unbounded) default and inserts a gating adjudicator "adj" with no per-agent bound.
        let cfg = config_with_default_adjudicator(RESULT_LINE);
        let msg = unbounded_wall_clock_advisory(&cfg)
            .expect("an unbounded default over an unbounded gating role must warn");
        assert!(
            msg.contains("\"adj\""),
            "names the unbounded gating role: {msg}"
        );
        assert!(
            msg.contains("defaults.max_wall_clock"),
            "names the fix knob: {msg}"
        );
        assert!(
            msg.to_lowercase().contains("swept"),
            "explains a hung gating agent is never swept: {msg}"
        );
    }

    #[test]
    fn unbounded_wall_clock_advisory_is_silent_when_the_default_is_bounded() {
        let mut cfg = config_with_default_adjudicator(RESULT_LINE);
        cfg.workflow.defaults.max_wall_clock = 600;
        assert!(
            unbounded_wall_clock_advisory(&cfg).is_none(),
            "a bounded default sweeps every inheriting gating role, so no warning"
        );
    }

    #[test]
    fn unbounded_wall_clock_advisory_is_silent_when_every_gating_role_sets_its_own_bound() {
        let mut cfg = config_with_default_adjudicator(RESULT_LINE);
        // Default stays unbounded, but the gating role carries its own per-agent bound.
        cfg.agents.get_mut("adj").unwrap().max_wall_clock = Some(300);
        assert!(
            unbounded_wall_clock_advisory(&cfg).is_none(),
            "a per-agent bound on every gating role covers the risk even under a 0 default"
        );
    }

    #[test]
    fn unbounded_wall_clock_advisory_names_only_gating_roles_not_lenses() {
        // A lens/adversary that hangs is not a GATING role; the advisory targets only the
        // verdict-bearing roles the gate awaits, so an unbounded lens is not named.
        let mut cfg = config_with_default_adjudicator(RESULT_LINE);
        cfg.agents.insert("lens".into(), agent("lens", RESULT_LINE));
        cfg.workflow.defaults.review.lenses = vec!["lens".into()];
        let msg = unbounded_wall_clock_advisory(&cfg)
            .expect("the unbounded gating adjudicator still warns");
        assert!(msg.contains("\"adj\""), "names the gating role: {msg}");
        assert!(
            !msg.contains("\"lens\""),
            "an unbounded lens is not a gating role and must not be named: {msg}"
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

    #[test]
    fn review_tiers_depth_policy_parses_from_yaml() {
        // spec 03 / spec 13 unit 4: `defaults.review` carries an OPT-IN `tiers` depth
        // policy - a light panel, a blast-radius threshold, and a high-risk path list -
        // that parse from the workflow YAML alongside the full panel.
        let yaml = "name: w\n\
defaults:\n  \
review:\n    \
lenses: [archlens, techlens]\n    \
adversary: adv\n    \
adjudicator: adj\n    \
tiers:\n      \
threshold: 3\n      \
high_risk_paths: [\"src/conductor.rs\", \"specs/**\"]\n      \
light:\n        \
lenses: [archlens]\n        \
adjudicator: adj\n\
stages:\n  \
implement:\n    \
agent: worker\n";
        let wf: Workflow = serde_yaml::from_str(yaml).unwrap();
        let review = &wf.defaults.review;
        let depth = review.depth().expect("the tiers depth policy must parse");
        assert_eq!(depth.threshold, 3);
        assert_eq!(depth.high_risk_paths, ["src/conductor.rs", "specs/**"]);
        assert_eq!(depth.light.lenses, ["archlens"]);
        assert_eq!(depth.light.adjudicator, "adj");
        assert!(
            depth.light.adversary.is_empty(),
            "the light panel typically omits the adversary"
        );
    }

    #[test]
    fn validate_catches_an_unknown_light_panel_agent() {
        // The light panel's agent ids are validated exactly like the full panel's: an
        // unknown light-panel lens/adversary/adjudicator fails `config::load` (spec 03).
        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent_def("a"));
        cfg.workflow.defaults.review = ReviewPanel {
            lenses: vec!["a".into()],
            adjudicator: "a".into(),
            tiers: Some(Box::new(ReviewDepth {
                light: ReviewPanel {
                    lenses: vec!["ghost".into()],
                    adjudicator: "a".into(),
                    ..Default::default()
                },
                threshold: 2,
                ..Default::default()
            })),
            ..Default::default()
        };
        assert!(
            cfg.validate().is_err(),
            "an unknown light-panel lens must fail validation"
        );
    }

    #[test]
    fn validate_rejects_a_light_panel_with_no_adjudicator() {
        // The adjudicator's gating verdict is mandatory on every tier - only the
        // adversary flexes (spec 03 / spec 13 unit 4). A configured light panel that
        // names no adjudicator would let a low-risk unit approve trivially, so it fails
        // `config::load` loudly.
        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent_def("a"));
        cfg.workflow.defaults.review = ReviewPanel {
            lenses: vec!["a".into()],
            adjudicator: "a".into(),
            tiers: Some(Box::new(ReviewDepth {
                light: ReviewPanel {
                    lenses: vec!["a".into()],
                    ..Default::default()
                },
                threshold: 2,
                ..Default::default()
            })),
            ..Default::default()
        };
        assert!(
            cfg.validate().is_err(),
            "a light panel with no adjudicator must fail validation"
        );
    }

    #[test]
    fn validate_accepts_a_well_formed_depth_policy() {
        // A depth policy whose light panel names a known adjudicator and only known
        // agents validates cleanly.
        let mut cfg = Config::default();
        for id in ["arch", "tech", "adv", "judge"] {
            cfg.agents.insert(id.into(), agent_def(id));
        }
        cfg.workflow.defaults.review = ReviewPanel {
            lenses: vec!["arch".into(), "tech".into()],
            adversary: "adv".into(),
            adjudicator: "judge".into(),
            tiers: Some(Box::new(ReviewDepth {
                light: ReviewPanel {
                    lenses: vec!["arch".into()],
                    adjudicator: "judge".into(),
                    ..Default::default()
                },
                threshold: 3,
                high_risk_paths: vec!["src/conductor.rs".into()],
            })),
        };
        assert!(
            cfg.validate().is_ok(),
            "a well-formed depth policy must validate"
        );
    }

    #[test]
    fn validate_rejects_a_tiers_policy_on_a_full_panel_with_no_adjudicator() {
        // The gating verdict is mandatory on EVERY tier, including the FULL one a high-risk
        // unit routes to (remediation of sdet-u13-empty-full-tiers-skips-adjudicator). A
        // tiers policy whose ENCLOSING full panel names no adjudicator - even with a
        // perfectly valid light tier - would let a high-risk unit route to a panel that
        // approves trivially via `is_empty()`, skipping the adjudicator. So it must fail
        // `config::load` loudly. Before the fix, `validate_depth` guarded only the light
        // tier, so `{empty full roster + valid tiers.light}` was ACCEPTED.
        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent_def("a"));
        cfg.workflow.defaults.review = ReviewPanel {
            // A full panel that names NO adjudicator (here, no roster at all).
            tiers: Some(Box::new(ReviewDepth {
                light: ReviewPanel {
                    lenses: vec!["a".into()],
                    adjudicator: "a".into(),
                    ..Default::default()
                },
                threshold: 2,
                ..Default::default()
            })),
            ..Default::default()
        };
        assert!(
            cfg.validate().is_err(),
            "a tiers policy on a full panel that names no adjudicator must fail validation"
        );
    }

    #[test]
    fn validate_rejects_a_stage_review_declaring_only_a_tiers_policy() {
        // A per-stage `review:` that declares ONLY a tiers policy (no roster, so no
        // adjudicator on the full panel) is malformed the same way: the per-stage
        // `validate_depth` branch (remediation of sdet-u13-stage-override-tiers-untested,
        // part a) now rejects it loudly instead of the runtime silently discarding it back
        // to defaults. This pins the stage-level validation branch that was untested.
        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent_def("a"));
        cfg.workflow.stages.insert(
            "implement".into(),
            Stage {
                name: "implement".into(),
                agent: "a".into(),
                review: ReviewPanel {
                    // Only a tiers policy, no enclosing roster/adjudicator.
                    tiers: Some(Box::new(ReviewDepth {
                        light: ReviewPanel {
                            lenses: vec!["a".into()],
                            adjudicator: "a".into(),
                            ..Default::default()
                        },
                        threshold: 2,
                        ..Default::default()
                    })),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        assert!(
            cfg.validate().is_err(),
            "a stage review declaring only a tiers policy (no full adjudicator) must fail validation"
        );
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
