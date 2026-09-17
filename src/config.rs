//! The declarative surface of Rigger: agent definition files (markdown with YAML
//! frontmatter) and a workflow YAML, loaded and parsed into runtime types. Plain value
//! types plus the pure parsing/lint functions; nothing here depends on the conductor.
//!
//! The disk reads (`load`, `read_agents_dir`, `read_store_config`, ...) and
//! [`Config::validate`] (which also probes `PATH` via [`crate::gate`]) live in
//! [`crate::config_store`] (spec 93, criterion 1) - that module's doc has the split
//! rationale (mirrors [`crate::spawn`]/[`crate::spawn_store`]).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::Duration;

use serde::Deserialize;

use crate::failure;

#[derive(Debug, thiserror::Error)]
#[error("config: {0}")]
pub struct Error(pub String);

pub(crate) fn err(msg: impl Into<String>) -> Error {
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

/// One registered REGENERABLE artifact rule (spec 88, criterion 1): `paths` names the glob
/// patterns (the same authority `Gate::inputs` uses) a merge conflict is checked against, and
/// `run` is the shell command that regenerates them fresh from the tree. A merge conflict
/// CONFINED entirely to paths some rule matches is resolved by the conductor itself - running
/// `run` in the unit's worktree and committing - with no implementer spawn at all; a conflict
/// that also touches a non-matching (source) path still re-parks the implementer for that part,
/// and the matching paths are regenerated in a follow-up commit after the implementer's own
/// commit lands (the design's "in that order").
#[derive(Clone, Debug, Default, Deserialize)]
pub struct RegenerateRule {
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub run: String,
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
    /// cannot express. On the grep lane `.safe` equals the capped
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
    /// `symbols` (the structural symbol index and the default), NOT grep - so a
    /// workflow that says nothing gets structural grounding. `"symbols"` selects it
    /// explicitly; `"grep"` selects the literal substring grounder (reachable only
    /// when configured by name); `"nop"` grounds nothing. The retired vector engine
    /// (`"turbovec"` / `"vector"` / `"hybrid"`, spec 57) is REJECTED with a migration
    /// error naming the `symbols` default, never a silent grep degrade. When the
    /// resolved grounder is `symbols` but the binary was built WITHOUT the `symbols`
    /// feature, selection FAILS LOUDLY rather than silently degrading to grep (see
    /// `grounder::grounder_for` / `main::select_grounder`).
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
    /// This stage's own remediation depth (spec 91, criterion 1, rule 2): overrides
    /// `defaults.max_retries` for every unit this stage governs. Unset (`0`, the
    /// default) inherits `defaults.max_retries` exactly as before, so a stage that
    /// says nothing escalates on the historical run-wide bound - the same
    /// unset-means-inherit convention `speculation_width` above already uses.
    #[serde(default)]
    pub max_retries: u32,
    /// Set by the conductor (never authored, hence `serde(skip)`) on the deterministic
    /// per-criterion BASELINE units it synthesizes from the fan-out implement template.
    /// It marks a stage as the conductor's fallback decomposition for one criterion, so
    /// `harvest_proposed` can let a planner-proposed unit that cites the SAME criterion
    /// SUPERSEDE (remove) it - one unit per criterion, never baseline + refinement both
    /// doing the same work. False for every authored stage, the planner, the template,
    /// and every planner-proposed unit.
    #[serde(skip)]
    pub baseline: bool,
    /// The STABLE id of the acceptance criterion this stage serves (spec 18 §3.3): its
    /// 1-based position plus a content hash of the normalized criterion text, computed
    /// by `conductor::criterion_stable_id`. Set by the conductor (never authored, hence
    /// `serde(skip)`) on the per-criterion baseline units AND (spec 72, THE STAMP) on
    /// every planner-proposed stage `harvest_proposed`'s ADD path resolves to a
    /// criterion - without this second stamp a planner-added stage could never be found
    /// by a LATER proposal's supersede match, so only a baseline could ever be
    /// superseded and a planner unit superseding a planner unit silently duplicated
    /// instead (spec 72's Problem). The planner is shown this id next to each criterion
    /// and echoes it on every proposal, so `harvest_proposed` matches a proposal to
    /// whatever currently serves that criterion by this id rather than by
    /// re-normalized prose - a paraphrase or truncation of a long criterion the planner
    /// was told to copy verbatim no longer silently spawns a duplicate. Empty for every
    /// stage that serves no criterion (the plan/plan-critique infrastructure stages, and
    /// a genuinely-new unmatched-proposal sub-unit).
    #[serde(skip)]
    pub criterion_id: String,
    /// The identity of the PLANNING EPISODE that proposed this stage (spec 72,
    /// PLAN-EPISODE IDENTITY): one planner pass (the initial plan or a plan-critique
    /// reject's re-plan) is one episode, and `harvest_proposed` copies the winning
    /// proposal's own `UnitProposed::episode` here at insert time so a LATER episode's
    /// proposal for the same criterion can tell this stage's episode is EARLIER and
    /// supersede it - and so a SAME-episode sibling (a real split) can tell it must
    /// never supersede this stage. Never set on a conductor-synthesized baseline (`
    /// baseline` is the always-earliest sentinel instead - a baseline predates every
    /// episode by construction, so it needs no episode identity of its own); empty on a
    /// stage whose proposal carried no episode (the pre-spec-72 legacy shape, or a
    /// hand-authored proposal), which a later unit's supersede handling resolves.
    #[serde(skip)]
    pub episode: String,
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

/// The event-store SELECTION a project's committed config pins (§48 rung 4, the project-config
/// precedence rung). The CHOICE rides the repo so every member - and every worker's bare `rigger
/// result` - resolves the same store with no flags, while CREDENTIALS stay OUT of the committed
/// file (they ride the flag, the environment, or the gitignored `.rigger/store.conn` secret file,
/// all higher-precedence rungs). `backend` is `sqlite` (the default, and an empty value, both mean
/// "no opinion" so a project that pins nothing keeps today's behavior) or `kurrentdb`; `url` is the
/// OPTIONAL non-secret address (host/port only) for the server backend.
///
/// Deliberately its own small type, not a bare string, so the non-secret address rides alongside
/// the choice in the one committed place; the connection string is never required here, so no
/// secret can leak into a committed file.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct StoreConfig {
    #[serde(default)]
    pub backend: String,
    #[serde(default)]
    pub url: String,
}

/// BuildConfig is the committed `build:` config the shared build-environment resolver
/// reads (spec 65 - one build-environment authority): a single resolver
/// ([`crate::gate::BuildEnv::resolve`]) derives the actual env vars from these three
/// fields (`wrapper`/`cache_dir` as one facet, `jobs` as an independent one - see
/// below) and applies them to gate builds (run via [`crate::gate::Runner::run`] by
/// [`crate::gate::ExecRunner`]) and the blocking `driver::cli` agent driver - the
/// two sites wired so far (see [`crate::gate::BuildEnv`] for the disclosed gap on
/// the remaining agent-spawn paths). Plain config-shape data only - the resolution
/// logic itself (verbatim wrapper passthrough today; `auto` PATH-probing and the
/// named-but-absent hard error are spec 65 unit 2's job) lives in `gate.rs`, not
/// here, so this stays a pure value type like its siblings.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct BuildConfig {
    /// The `RUSTC_WRAPPER` binary name, `auto` to probe PATH, or `off`/empty (the
    /// default) to disable the shared-cache layer entirely.
    #[serde(default)]
    pub wrapper: String,
    /// The shared compilation-cache directory. Empty (the default) resolves to
    /// `<state home>/rigger/build-cache` (the resolver's own default), so every
    /// project and worktree on the machine shares one cache absent explicit
    /// configuration.
    #[serde(default)]
    pub cache_dir: String,
    /// The `CARGO_BUILD_JOBS` cap on each build's OWN internal parallelism (spec
    /// 65, JOBS CAP - unit 4). `0` (the default) means unset, matching this
    /// config's existing zero-as-unset convention (see
    /// [`Defaults::budget`]/[`Defaults::max_retries`]/[`Defaults::speculation_width`]):
    /// the ambient/cargo default jobs count is left untouched, byte-for-byte
    /// back-compatible with every workflow committed before this field existed. A
    /// positive value reaches every build via the ONE resolver
    /// ([`crate::gate::BuildEnv::resolve`]), independent of `wrapper`, so
    /// `build.max_concurrent` (unit 3's slot budget) times `jobs` can be sized to
    /// the machine.
    #[serde(default)]
    pub jobs: u32,
    /// The machine-wide build concurrency budget (spec 65): how many actual compiler
    /// invocations may run at once, across every rigger process on the machine, before
    /// the next one blocks for a free slot ([`crate::budget::BuildBudget`] is the
    /// resolver that reads this). Omitted resolves to 4 (a deliberately non-zero
    /// default distinct from this field's plain `u32::default()`); an EXPLICIT `0`
    /// means unlimited (the same `budget: 0` convention `defaults.budget` uses) -
    /// every build is admitted immediately and no slot directory is ever touched.
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: u32,
    /// RETIRED (spec 91): the implementer's diff-scoped per-round mutation-efficacy switch
    /// spec 73 introduced. [`Config::validate`] now REJECTS any explicit value naming spec
    /// 91 - mutation testing runs ONCE, at a workflow's own `checkin` stage via a `mutation`
    /// gate, never per implementer round; that stage requiring the `cargo-mutants` binary on
    /// PATH is now driven by whether the workflow DECLARES a gate named `mutation`
    /// ([`crate::gate::MUTATION_GATE_ID`]), not by this field. Kept as a `String` field
    /// (never removed from the struct) purely so a workflow still authored against the
    /// retired switch parses far enough for `validate` to name it in its rejection, rather
    /// than failing with an opaque serde "unknown field" error.
    #[serde(default)]
    pub mutation: String,
}

/// The `Workflow.build` field's serde default (spec 65): called only when the WHOLE
/// `build:` key is absent from a committed workflow.yml. Deliberately distinct from the
/// derived [`BuildConfig::default`] (which a Rust-constructed `Config::default()` still
/// gets, `max_concurrent: 0`/unlimited, in tests that build a config in memory) - a
/// missing `build:` section in a REAL workflow.yml must resolve `max_concurrent` to the
/// documented default of 4 exactly like a `build:` section present but missing the key
/// specifically ([`default_max_concurrent`]), so both absent-shapes agree.
fn default_build_config() -> BuildConfig {
    BuildConfig {
        max_concurrent: default_max_concurrent(),
        ..Default::default()
    }
}

/// The default `build.max_concurrent` (spec 65) when the key is absent from a committed
/// workflow.yml: 4 concurrent builds machine-wide. A named `fn` (not a bare literal) so
/// serde can call it only when the key is truly ABSENT - an explicit `max_concurrent: 0`
/// still parses as 0 (unlimited), never silently promoted back to this default.
fn default_max_concurrent() -> u32 {
    4
}

/// Workflow is the declarative loop: a DAG of stages, a gate library, and defaults.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Workflow {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub defaults: Defaults,
    /// The event-store selection this project's committed config pins (§48 rung 4). Absent (the
    /// common case) is "no opinion" - the resolver falls through to its default. Credentials never
    /// ride this committed file; only the CHOICE and an optional non-secret host/port URL do.
    #[serde(default)]
    pub store: StoreConfig,
    /// The always-on dashboard opt-out (spec 50, criterion 4). The step path ENSURES the
    /// machine-level singleton dash is up on every run unless this opts out: the documented
    /// `dash: off` (or the `false`/`no` synonyms, matched case- and whitespace-insensitively)
    /// suppresses the ensure entirely, so a headless or CI run proceeds with NO dash and NO
    /// port bind. Empty (the default) / `on` / any other value keeps the always-on promise, so
    /// a workflow that says nothing about the dash still gets one. Resolved through the single
    /// authority [`dash_enabled`](Self::dash_enabled). Modeled as a `String` (like `grounder`,
    /// `autonomy`, `partition`) rather than a `bool` so the documented `dash: off` parses (this
    /// deserializer treats `off` as a plain scalar, not a boolean).
    #[serde(default)]
    pub dash: String,
    /// The shared build-environment config (spec 65): `wrapper` / `cache_dir`, read
    /// by the ONE resolver ([`crate::gate::BuildEnv::resolve`]) that derives the env
    /// applied to gate builds and the blocking `driver::cli` agent driver - the two
    /// sites wired so far, not yet every agent-spawn path in the crate (see
    /// [`crate::gate::BuildEnv`]). Absent (the common case) resolves to no wrapper -
    /// today's ambient-environment behavior, unchanged - but `max_concurrent` still
    /// resolves to its documented default of 4 (`default_build_config`, not the plain
    /// derived `BuildConfig::default()`, which a whole-section-absent `build:` would
    /// otherwise silently fall back to).
    #[serde(default = "default_build_config")]
    pub build: BuildConfig,
    #[serde(default)]
    pub gates: BTreeMap<String, Gate>,
    #[serde(default)]
    pub stages: BTreeMap<String, Stage>,
    /// Registered regenerable artifacts (spec 88, criterion 1): an integration merge
    /// conflict confined entirely to paths one of these rules matches is resolved by the
    /// conductor itself (regenerate + commit, no spawn); absent (the common case, and
    /// every workflow.yml committed before this key existed) means no path is registered,
    /// so every conflict re-parks the implementer exactly as it would otherwise.
    #[serde(default)]
    pub regenerate: Vec<RegenerateRule>,
}

impl Workflow {
    /// The review panel a unit reviews ITSELF with (§3.2): a stage's own `review` override when it
    /// sets one, otherwise this workflow's `defaults.review`. Declared once and inherited by every
    /// implementer unit, including planner-proposed units.
    ///
    /// Extracted here (spec 92 criterion 2) from `conductor::RunCtx::effective_review_panel`, which
    /// now delegates to this - the ONE fallback-rule authority, so the live run's per-unit review
    /// routing and the workflow-DEFINITION graph indexer (which needs the identical rule to derive
    /// a stage's `REVIEWS` edges from `.rigger/workflow.yml` without a live run) read the same
    /// answer rather than two copies that could drift.
    #[cfg_attr(all(feature = "core", not(feature = "store")), allow(dead_code))] // only called from the store-gated half under core-only
    pub(crate) fn effective_review_panel<'a>(&'a self, st: &'a Stage) -> &'a ReviewPanel {
        if st.review.is_empty() {
            &self.defaults.review
        } else {
            &st.review
        }
    }

    /// Whether the always-on dash auto-ensure is enabled for this workflow (spec 50, criterion
    /// 4) - the SINGLE resolution authority the step-path opt-out reads. ON unless the workflow
    /// explicitly opts out: `off` (the documented form) or its `false`/`no` synonyms, matched
    /// case- and whitespace-insensitively, resolve to OFF; an unset/empty `dash` and every other
    /// value resolve to ON, so a workflow that says nothing about the dash keeps the always-on
    /// promise. Living here (not inlined at the call site) keeps the on-by-default rule in one
    /// place and cannot drift between callers.
    pub fn dash_enabled(&self) -> bool {
        !matches!(
            self.dash.trim().to_ascii_lowercase().as_str(),
            "off" | "false" | "no"
        )
    }

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

/// Fold `defaults.max_wall_clock` onto every agent that did not set its own (spec 10,
/// unit 3), so an agent's resolved `max_wall_clock` is authoritative wherever a spawn is
/// built - the replay driver reads it straight off the [`AgentDef`] with no access to the
/// workflow. Per-role by construction: an agent's own value wins; only an unset agent
/// inherits the default. A zero default leaves an unset agent unbounded (`None`).
#[cfg_attr(all(feature = "core", not(feature = "store")), allow(dead_code))] // only called from the store-gated half under core-only
pub(crate) fn resolve_wall_clocks(agents: &mut BTreeMap<String, AgentDef>, defaults: &Defaults) {
    if defaults.max_wall_clock == 0 {
        return;
    }
    for agent in agents.values_mut() {
        if agent.max_wall_clock.is_none() {
            agent.max_wall_clock = Some(defaults.max_wall_clock);
        }
    }
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
pub(crate) fn puts_verdict_on_result_channel(prompt: &str) -> bool {
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
pub(crate) fn literal_is_emit_payload(clause: &str) -> bool {
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
#[cfg_attr(all(feature = "core", not(feature = "store")), allow(dead_code))] // only called from the store-gated half under core-only
enum Color {
    White,
    Gray,
    Black,
}

#[cfg_attr(all(feature = "core", not(feature = "store")), allow(dead_code))] // only called from the store-gated half under core-only
pub(crate) fn find_cycle(stages: &BTreeMap<String, Stage>) -> Option<String> {
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
