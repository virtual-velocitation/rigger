use std::collections::{BTreeMap, BTreeSet};

pub use rigger_domain::config::*;

/// Config is a fully loaded, validated harness configuration.
#[derive(Clone, Debug, Default)]
pub struct Config {
    pub agents: BTreeMap<String, AgentDef>,
    pub workflow: Workflow,
    /// The operator instruction layer (`.rigger/instructions/*.md`, filename order), composed
    /// into every spawned agent's system prompt ([`crate::instructions::compose`]): a file
    /// named as a built-in entry replaces that entry in place, its body not blank, or switches
    /// it off, its body blank, and every other file follows the built-in layer.
    pub instructions: Vec<crate::instructions::Instruction>,
    /// The project directory this configuration was loaded from ([`crate::config_store::load`]):
    /// the directory a run's definition hash reads
    /// ([`crate::config_store::definition_hash`]). Empty for a configuration built in memory,
    /// which has no on-disk definition to hash.
    pub dir: String,
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

/// Advisory (spec 19c, unit 3): warn when some GATING role's `max_wall_clock` resolves to `0`,
/// unbounded - its own value is `0`, or it sets none and `defaults.max_wall_clock` is `0` - so a
/// gating agent that hangs is never swept (the liveness watchdog only times out a spawn with a
/// non-zero bound - see [`resolve_wall_clocks`], which no-ops on a `0` default). Returns the
/// warning line naming those gating roles and the fix, or `None` when every gating role
/// resolves to a non-zero bound.
///
/// Reuses the single [`gating_agent_ids`] authority - the SAME gating-role set the
/// verdict-line lint ([`lint_gating_verdict_lines`]) inspects - so "which roles gate" is
/// defined once, never a second parallel definition. Non-fatal by construction: the caller
/// (`rigger validate`) prints it to stderr without changing the exit status, like the other
/// advisories.
pub fn unbounded_wall_clock_advisory(cfg: &Config) -> Option<String> {
    // The gating roles whose bound resolves to `0`: their own value, or the default they
    // inherit (already folded onto them at load when it is non-zero). `gating_agent_ids`
    // returns a sorted `BTreeSet`, so the collected roster is deterministic. A gating id absent
    // from `agents` is a referential error (`Config::validate`), reported there, not here.
    let unbounded: Vec<String> = gating_agent_ids(cfg)
        .into_iter()
        .filter(|id| {
            cfg.agents.get(id).is_some_and(|a| {
                a.max_wall_clock
                    .unwrap_or(cfg.workflow.defaults.max_wall_clock)
                    == 0
            })
        })
        .map(|id| format!("{id:?}"))
        .collect();
    if unbounded.is_empty() {
        return None;
    }
    Some(format!(
        "warning: gating role(s) {} resolve max_wall_clock to 0 (unbounded), so a hung gating \
         agent is never swept and the run stalls silently. Set a non-zero max_wall_clock \
         (seconds) on those roles, or leave theirs unset under a non-zero \
         defaults.max_wall_clock.",
        unbounded.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The gating ids a config collects read each panel through its accessors, in either form:
    /// a map form naming one agent as lens and adjudicator, a list form naming one agent in two
    /// steps and a tiers panel, a list naming its adjudicator after the rest.
    #[test]
    fn gating_agent_ids_read_a_panel_of_either_form_and_its_light_panel() {
        for (panel, gating) in [
            ("{ lenses: [a, b], adjudicator: a }", vec!["a"]),
            (
                "{ steps: [{ lenses: [a, b] }, { adversary: a }, { adjudicator: b }] }",
                vec!["b"],
            ),
            (
                "{ adjudicator: z, tiers: { threshold: 1, light: { adjudicator: y } } }",
                vec!["y", "z"],
            ),
            (
                "{ steps: [{ adjudicator: z }], tiers: { light: { steps: [{ adjudicator: z }] } } }",
                vec!["z"],
            ),
            ("{ steps: [{ lenses: [a] }, { adversary: x }] }", vec![]),
        ] {
            let body = format!("stages:\n  s:\n    agent: worker\n    review: {panel}\n");
            let cfg = Config {
                workflow: parse_yaml_naming_unknown_keys(&body)
                    .unwrap_or_else(|e| panic!("parse {body}: {e}")),
                ..Default::default()
            };
            assert_eq!(
                gating_agent_ids(&cfg),
                gating.into_iter().map(str::to_string).collect(),
                "{panel}"
            );
        }
    }
}
