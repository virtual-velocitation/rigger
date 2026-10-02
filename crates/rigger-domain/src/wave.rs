//! The wave logic: which stages a wave may run, the fan-out shape predicates, the
//! deterministic decomposition baseline and the coverage gate over the stage graph.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::config::{Stage, Workflow};
use crate::playbooks::fnv1a_64;

/// Normalize a criterion string for the supersede match (the duplication fix): trim,
/// then collapse every internal run of ASCII whitespace to a single space. The planner
/// is told to cite the criterion text VERBATIM (PLAN_PROTOCOL), so an exact match is
/// the contract; normalizing whitespace on both sides makes it robust to incidental
/// reflowing/indentation differences without loosening into fuzzy matching (a planner
/// that PARAPHRASES a criterion deliberately will not match, and is correctly treated
/// as a genuinely new sub-unit added on top of the surviving baseline).
pub fn normalize_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A baseline criterion's STABLE id (spec 18 §3.3, addendum "Planner ↔ baseline
/// robustness"): its 1-based `position` plus a content hash of the criterion, so the
/// planner can echo the id and `harvest_proposed` can match a proposal to its baseline
/// by ID rather than by re-normalized prose. That closes the highest-risk failure: a
/// planner that PARAPHRASES or TRUNCATES a long criterion it was told to copy verbatim
/// produces a proposal that no longer prose-matches its baseline, so both run - two
/// units claim the same files and plan-critique rejects in a loop. With the id echoed,
/// the paraphrase still resolves to its criterion and supersedes the one baseline.
///
/// The hash is over [`normalize_ws`] of the criterion - the SAME single normalization
/// authority the prose fallback uses, which already collapses every run of whitespace
/// (including CR/LF) to one space, so the id is inherently line-ending-normalized and
/// robust to incidental reflow without loosening into fuzzy matching. The `position`
/// disambiguates two criteria that normalize equal (each still gets a distinct id).
/// Deterministic by construction: the same criterion at the same position yields the
/// same id on every run, so the id the planner is shown equals the id the baseline
/// carries equals the id `harvest_proposed` matches.
pub fn criterion_stable_id(position: usize, criterion: &str) -> String {
    format!(
        "c{position}-{:016x}",
        fnv1a_64(normalize_ws(criterion).as_bytes())
    )
}

/// The rule-6 blast-radius conflicts in a proposed decomposition (Unit 1, spec 10;
/// `docs/handbook/authoring-loops.md` rule 6: "criteria that share a blast radius
/// belong in ONE unit"). `units` pairs each fan-out unit's id with the distinct
/// files in its blast radius - the SAFE-superset view [`RunCtx::grounded_blast_radius`]
/// computes (spec 17 unit 3, 3b: rule-6 detection is a SAFETY consumer, so it reads the same
/// `structural union grep` superset [`partition_by_blast_radius`] does, NOT the precise seed).
/// It returns every unordered pair of DISTINCT
/// units whose blast radii INTERSECT, each with the shared files, so a decomposition
/// that splits one blast radius across two units is surfaced as concrete evidence the
/// plan-critique reviewers judge. The order is deterministic (input order for the
/// pairs, sorted+deduped shared files), so the same DAG produces the same critique
/// prompt across replay steps. A partition that is already disjoint yields no
/// conflicts. This is the DETECTION half; the adjudicator renders the verdict.
pub fn blast_radius_conflicts(
    units: &[(String, Vec<String>)],
) -> Vec<(String, String, Vec<String>)> {
    let mut conflicts = Vec::new();
    for (i, (a_name, a_files)) in units.iter().enumerate() {
        let a_set: HashSet<&str> = a_files.iter().map(|s| s.as_str()).collect();
        for (b_name, b_files) in units.iter().skip(i + 1) {
            let mut shared: Vec<String> = b_files
                .iter()
                .filter(|f| a_set.contains(f.as_str()))
                .cloned()
                .collect();
            shared.sort();
            shared.dedup();
            if !shared.is_empty() {
                conflicts.push((a_name.clone(), b_name.clone(), shared));
            }
        }
    }
    conflicts
}

/// Whether a stage runs the fan-out (parallel-lens) path rather than the
/// single-worker path (§3.2). A stage takes the standalone fan-out path ONLY when it
/// is a standalone review stage: it carries an `agents` lens list (or `strategy:
/// fan-out`) and has NO `agent`. A stage that names an `agent` runs the per-unit
/// lifecycle in `run_single_stage` - implement -> the unit's gates -> the three-tier
/// review OF THIS UNIT -> integrate - even when it sets `strategy: fan-out` (which on
/// an implementer stage means "one implementer per ready unit", driven by the
/// partitioner and the planner-proposed units, not "run my lone agent as a lens").
/// So review and integration live INSIDE the unit's lifecycle, never as a separate
/// downstream stage.
pub fn is_fan_out(st: &Stage) -> bool {
    st.agent.is_empty() && (!st.agents.is_empty() || st.strategy.eq_ignore_ascii_case("fan-out"))
}

/// The name of the FIRST stage (in stable BTreeMap order) `shape` matches, or None. Two
/// shapes it finds:
///
/// - [`is_fan_out_template`]: the implement TEMPLATE stage the conductor expands into one
///   per-criterion unit (the deterministic decomposition baseline). There is normally exactly
///   one; None when the workflow has no fan-out implementer template (a non-decomposing
///   workflow), in which case the conductor synthesizes no baseline units and the no-spec
///   path is unchanged.
/// - [`is_producer`]: the (first) `produces` planner stage - baseline units depend on it so
///   they run only AFTER the planner has had its chance to refine the DAG.
pub fn first_stage_named(
    stages: &BTreeMap<String, Stage>,
    shape: fn(&Stage) -> bool,
) -> Option<String> {
    stages
        .iter()
        .find(|(_, st)| shape(st))
        .map(|(name, _)| name.clone())
}

/// Whether a stage is shaped like the implement fan-out TEMPLATE: it names an `agent`,
/// sets `strategy: fan-out` ("one implementer per ready unit"), and does NOT `produces`
/// a DAG (it is a worker, not the planner). Pulled out of [`first_stage_named`] so
/// `rigger validate`'s [`ungated_fan_out_templates`] advisory checks the EXACT same
/// shape the runtime decomposition matches - one predicate, never a second guess at it.
pub fn is_fan_out_template(st: &Stage) -> bool {
    !st.agent.is_empty() && st.strategy.eq_ignore_ascii_case("fan-out") && st.produces.is_empty()
}

/// NO UNGATED FAN-OUT TEMPLATE advisory (spec 103, criterion 2): names every fan-out
/// implement template ([`is_fan_out_template`]'s own shape - never a second guess at
/// it) that declares NO gates at all, so `rigger validate` can warn on it at author
/// time - before a spec ever decomposes against it and reaches the runtime invariant
/// [`assert_no_ungated_fanout_unit`] enforces. A template WITH gates is never named
/// here: an author who deliberately wrote an ungated fan-out stage gets silence, per
/// the Design's "empty template gates - a workflow authored with no gates keeps
/// running ungated" constraint - this only surfaces the case most likely to be an
/// oversight (gates omitted entirely).
pub fn ungated_fan_out_templates(stages: &BTreeMap<String, Stage>) -> Vec<String> {
    stages
        .iter()
        .filter(|(_, st)| is_fan_out_template(st) && st.gates.is_empty())
        .map(|(name, _)| name.clone())
        .collect()
}

/// Whether a stage `produces` a DAG at runtime (the planner that decomposes the spec).
pub fn is_producer(st: &Stage) -> bool {
    !st.produces.is_empty()
}

/// The name of the plan-critique gate stage, if the workflow wires one (Unit 1, spec
/// 10). The gate is recognized by ROLE, not by a hard-coded name: it is the review-only
/// stage (no `agent` - it critiques the DAG, it does not implement) that carries an
/// `adjudicator` (its verdict gates the fan-out) and `needs` the producer (it runs
/// AFTER the planner refined the DAG, BEFORE any implementer). A downstream standalone
/// review stage (which needs the implementer, not the producer) is therefore never
/// mistaken for it. Returns None when the workflow has no producer or no such gate, so
/// a non-decomposing or ungated workflow runs exactly as before.
pub fn critique_gate_name(stages: &BTreeMap<String, Stage>) -> Option<String> {
    let producer = first_stage_named(stages, is_producer)?;
    stages
        .iter()
        .find(|(_, st)| {
            st.agent.is_empty() && !st.adjudicator.is_empty() && st.needs.contains(&producer)
        })
        .map(|(name, _)| name.clone())
}

/// What a workflow with no critic names neither of (spec 112): the one clause every message
/// about a missing critic carries.
pub const NO_CRITIC_CLAUSE: &str =
    "the workflow names neither the plan-critique gate's adversary nor defaults.review.adversary";

/// The workflow's spec critic (spec 112): the adversary persona of the plan-critique gate
/// ([`critique_gate_name`]), else `defaults.review.adversary`, else `None` - a workflow naming
/// neither has no critic, and there is no built-in one.
pub fn critic(workflow: &Workflow) -> Option<String> {
    critique_gate_name(&workflow.stages)
        .map(|gate| workflow.stages[&gate].adversary.clone())
        .filter(|adversary| !adversary.is_empty())
        .or_else(|| Some(workflow.defaults.review.adversary.clone()))
        .filter(|adversary| !adversary.is_empty())
}

/// A stable, unique, human-legible unit id derived from a criterion's text plus its
/// ordinal: a lowercased, hyphen-joined slug of the first words, prefixed `unit-<n>-`
/// so the id is deterministic, collision-free across criteria, and references the
/// criterion it serves. The ordinal alone guarantees uniqueness even when two criteria
/// slug identically; the slug makes the id readable in the event log.
pub fn unit_slug(n: usize, criterion: &str) -> String {
    let mut slug = String::new();
    for ch in criterion.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.extend(ch.to_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
        if slug.trim_matches('-').len() >= 32 {
            break;
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        format!("unit-{n}")
    } else {
        format!("unit-{n}-{slug}")
    }
}

/// The deterministic decomposition BASELINE (§3.2): given a fan-out implement
/// `template` stage and the spec's acceptance `criteria`, synthesize ONE implement
/// unit per criterion. Each unit inherits the template's executable shape - its
/// `agent`, `gates`, `on_pass`, and `partition` - but carries THE CRITERION TEXT as
/// its `coverage`, so it grounds on the real criterion (not the template's label) and
/// its `UnitStarted` records the real `spec_criterion`. Each unit `needs` the planner
/// (`producer`) when one exists, so the baseline runs only after the planner refines.
/// The template itself is NOT run as a unit - these per-criterion units replace it.
pub fn baseline_units(
    template: &Stage,
    criteria: &[String],
    producer: Option<&str>,
) -> Vec<(String, Stage)> {
    let mut needs = template.needs.clone();
    if let Some(p) = producer {
        if !needs.iter().any(|n| n == p) {
            needs.push(p.to_string());
        }
    }
    let mut units = Vec::with_capacity(criteria.len());
    for (i, criterion) in criteria.iter().enumerate() {
        let name = unit_slug(i + 1, criterion);
        units.push((
            name.clone(),
            Stage {
                name,
                agent: template.agent.clone(),
                gates: template.gates.clone(),
                on_pass: template.on_pass.clone(),
                partition: template.partition.clone(),
                needs: needs.clone(),
                // The criterion text IS the unit's coverage: it grounds on the
                // criterion, and its UnitStarted spec_criterion is the real criterion.
                coverage: criterion.clone(),
                // Mark it the deterministic baseline for this criterion, so a
                // planner-proposed unit citing the same criterion supersedes it in
                // `harvest_proposed` rather than duplicating the work.
                baseline: true,
                // The stable id the planner echoes and `harvest_proposed` matches on
                // (spec 18 §3.3) - position + normalized-content hash, so a paraphrase
                // still resolves to this baseline instead of spawning a duplicate.
                criterion_id: criterion_stable_id(i + 1, criterion),
                ..Default::default()
            },
        ));
    }
    units
}

/// The lens set a standalone review stage runs concurrently: its `agents` list when
/// populated, else its single `agent`, else empty (§3.2). A standalone review stage
/// always has `agents` (it has no `agent` - that is what routes it to the fan-out
/// path), so the `agent` fallback is defensive; an implementer stage with an `agent`
/// runs its per-unit lifecycle instead and never reaches here.
pub fn fan_out_lenses(st: &Stage) -> Vec<String> {
    if !st.agents.is_empty() {
        st.agents.clone()
    } else if !st.agent.is_empty() {
        vec![st.agent.clone()]
    } else {
        Vec::new()
    }
}

/// Whether a stage carries an LLM judge, i.e. a real verifier and not a mechanical
/// proxy. A stage covers a criterion only if it has one (§8 proxy-gap guard, item 5):
/// a worker agent, a fan-out lens set, or an adjudicator. A gate-command-only stage
/// is a mechanical proxy and does not satisfy a conceptual criterion.
fn has_llm_verifier(st: &Stage) -> bool {
    !st.agent.is_empty() || !st.agents.is_empty() || !st.adjudicator.is_empty()
}

/// coverage_gap is the coverage gate (§3.2, §8). Every spec criterion must be
/// covered by a stage that has a real (LLM-judge) verifier; a criterion covered only
/// by a mechanical gate counts as NOT covered (the proxy-gap guard, item 5). It runs
/// against the live `stages` map, so proposed planner units (which carry their own
/// `coverage`) count toward closing the gap. Returns the gap reason, or None if every
/// criterion is covered (or there are no criteria to enforce).
pub fn coverage_gap(stages: &BTreeMap<String, Stage>, criteria: &[String]) -> Option<String> {
    if criteria.is_empty() {
        return None;
    }
    let covered: HashSet<&str> = stages
        .values()
        .filter(|st| has_llm_verifier(st))
        .map(|st| st.coverage.trim())
        .filter(|c| !c.is_empty())
        .collect();
    let gaps: Vec<&str> = criteria
        .iter()
        .map(|c| c.trim())
        .filter(|c| !covered.contains(c))
        .collect();
    if gaps.is_empty() {
        return None;
    }
    Some(format!(
        "coverage gap - no stage with an LLM verifier covers: {}",
        gaps.join("; ")
    ))
}

/// The stages a WAVE may run: [`ready_stages`] minus the plan-critique gate. The gate
/// belongs to the producer prelude EXCLUSIVELY (its reject re-runs the planner - a
/// coupling the per-stage scheduler cannot express), so no wave may ever schedule it.
/// Load-bearing on resume: a gate left verified-but-not-integrated by an interrupted
/// step satisfies `ready_stages` (needs the producer, not terminal), and driving it
/// through `run_single_stage`'s standalone-review path spawns lenses with no worktree -
/// the empty-cwd isolation refusal that killed the first adopted spec-10 run.
///
/// `fanout_criteria` is threaded straight through to [`ready_stages`] - see its own doc
/// comment for what it resolves (spec 91, criterion 1, rule 1).
pub fn wave_ready(
    stages: &BTreeMap<String, Stage>,
    integrated: &HashSet<String>,
    terminal: &HashSet<String>,
    critique_gate: Option<&str>,
    fanout_criteria: &HashMap<String, HashSet<String>>,
) -> Vec<String> {
    ready_stages(stages, integrated, terminal, fanout_criteria)
        .into_iter()
        .filter(|n| critique_gate != Some(n.as_str()))
        .collect()
}

/// A stage's `needs` entry `need` is satisfied against `stages`/`integrated` directly
/// when it names a LIVE stage (the historical rule, unchanged). When it instead names a
/// fan-out implement TEMPLATE - a stage `run` REMOVES from `stages` the moment it
/// expands into per-criterion baseline units (§ the baseline-decomposition block), so it
/// can never again satisfy a literal `integrated.contains(need)` - the entry is
/// satisfied once EVERY criterion id `fanout_criteria` records as covered by that
/// template's expansion has ALL of its CURRENT `stages` owners integrated (spec 91,
/// criterion 1, rule 1; round 3 fix for adj-u91c1-r2-verdict-reject). Each criterion id
/// is resolved LIVE against `stages` - never a frozen unit-id snapshot - because
/// `harvest_proposed`'s supersede fold can replace which unit id owns a criterion (a
/// planner refinement superseding a fan-out baseline member) without ever touching this
/// table; walking `stages` fresh on every call means whichever unit id(s) currently
/// carry that `criterion_id` are exactly the ones this checks, so a supersede can never
/// orphan the edge. A criterion id can name MORE THAN ONE live `stages` entry at once -
/// a same-episode planner SPLIT (spec 31/72's real-split guarantee: `harvest_proposed`
/// never reaps a genuinely-new same-episode sibling, only a strictly-earlier-episode
/// owner) leaves every split sibling live under the identical criterion_id
/// simultaneously (round 2's own `.find()`-first-match resolution wrongly assumed
/// exactly one live owner always exists, checked only the BTreeMap-key-first sibling,
/// and so could satisfy - or permanently fail to satisfy - the whole entry on that one
/// sibling's status alone while silently ignoring every other live sibling; round 2 was
/// rejected for this: arch-u91c1-r2-need-satisfied-ignores-real-split-siblings). This
/// resolves every criterion id against ALL of its current live owners via
/// `stages.iter().filter(..).all(..)`, not a single `.find()`, so the entry is
/// satisfied only once every live sibling under that criterion id has integrated. An
/// empty filtered set (no live `stages` entry names that criterion id at all) is
/// vacuously `true` by `Iterator::all`'s definition, but is unreachable in the designed
/// paths today: `harvest_proposed` never removes a criterion's last live owner without a
/// same-pass insertion replacing it (a bare `stages.remove` for a criterion-owning stage
/// must always be paired with a same-pass insert, never left standing alone), so a
/// tracked criterion id always resolves to at least one live entry in practice
/// (adv-u91c1-r2-cleared-fix-direction-vacuous-empty-owner-candidate). A member that is
/// merely open (never in `integrated`), or reached a terminal-but-not-integrated state
/// (escalated, or failed-terminal), leaves the whole entry unsatisfied - the run's
/// escalated fixpoint stays loud, never silently satisfied by a partial fan-out. A
/// `need` naming neither a live stage nor a tracked template resolves to the historical
/// `integrated.contains(need)` (false for a typo'd or already-consumed name), so a
/// workflow with no fan-out template is byte-for-byte unaffected.
fn need_satisfied(
    need: &str,
    stages: &BTreeMap<String, Stage>,
    integrated: &HashSet<String>,
    fanout_criteria: &HashMap<String, HashSet<String>>,
) -> bool {
    match fanout_criteria.get(need) {
        Some(criteria) => criteria.iter().all(|criterion_id| {
            stages
                .iter()
                .filter(|(_, st)| st.criterion_id == *criterion_id)
                .all(|(name, _)| integrated.contains(name))
        }),
        None => integrated.contains(need),
    }
}

/// `fanout_criteria` maps a fan-out implement TEMPLATE's name to the stable criterion
/// ids `run` synthesized ONE baseline unit per, at the moment the template was consumed
/// (§ the baseline-decomposition block) - the live resolution table [`need_satisfied`]
/// consults for a `needs` entry that names a template rather than a still-live stage
/// (spec 91, criterion 1, rule 1). It never names unit ids: which unit id currently
/// owns a criterion is looked up FRESH in `stages` on every call (round 2 fix for
/// adj-u91c1-verdict-reject), so a planner supersede that swaps the owning unit id
/// needs no companion update here - one authority (`stages`/`criterion_id`, already kept
/// in sync by `harvest_proposed`), not a second membership index to maintain. Empty for
/// a workflow with no fan-out template, so `ready_stages` degrades to its historical
/// literal-needs check.
pub fn ready_stages(
    stages: &BTreeMap<String, Stage>,
    integrated: &HashSet<String>,
    terminal: &HashSet<String>,
    fanout_criteria: &HashMap<String, HashSet<String>>,
) -> Vec<String> {
    let mut ready: Vec<String> = stages
        .iter()
        .filter(|(name, st)| {
            !terminal.contains(*name)
                && st
                    .needs
                    .iter()
                    .all(|n| need_satisfied(n, stages, integrated, fanout_criteria))
        })
        .map(|(name, _)| name.clone())
        .collect();
    ready.sort();
    ready
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Workflow;

    /// A workflow parsed from its YAML, as `workflow.yml` carries it.
    fn workflow(yaml: &str) -> Workflow {
        serde_yaml::from_str(yaml).expect("a well-formed workflow")
    }

    const PLAN: &str = "  plan:\n    agent: planner\n    produces: dag\n";

    #[test]
    fn the_critic_is_the_plan_critique_gates_adversary_first() {
        let wf = workflow(&format!(
            "defaults:\n  review:\n    adversary: fallback\nstages:\n{PLAN}  plan-critique:\n    \
             needs: [plan]\n    adversary: critic\n    adjudicator: judge\n"
        ));
        assert_eq!(critic(&wf).as_deref(), Some("critic"));
    }

    #[test]
    fn the_critic_falls_back_to_the_default_review_adversary() {
        let gate_without_adversary = workflow(&format!(
            "defaults:\n  review:\n    adversary: fallback\nstages:\n{PLAN}  plan-critique:\n    \
             needs: [plan]\n    adjudicator: judge\n"
        ));
        assert_eq!(critic(&gate_without_adversary).as_deref(), Some("fallback"));
        let no_gate = workflow(
            "defaults:\n  review:\n    adversary: fallback\nstages:\n  a:\n    agent: worker\n",
        );
        assert_eq!(critic(&no_gate).as_deref(), Some("fallback"));
    }

    #[test]
    fn a_workflow_naming_neither_key_has_no_critic() {
        let review_stage_only = workflow(&format!(
            "stages:\n{PLAN}  implement:\n    needs: [plan]\n    agent: worker\n  review:\n    \
             needs: [implement]\n    adversary: lens-adversary\n    adjudicator: judge\n"
        ));
        assert_eq!(
            critic(&review_stage_only),
            None,
            "a review stage that does not need the producer is not the plan-critique gate"
        );
        assert_eq!(
            critic(&workflow("stages:\n  a:\n    agent: worker\n")),
            None
        );
        assert_eq!(
            NO_CRITIC_CLAUSE,
            "the workflow names neither the plan-critique gate's adversary nor \
             defaults.review.adversary"
        );
    }
}
