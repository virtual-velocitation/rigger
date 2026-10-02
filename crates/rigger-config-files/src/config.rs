use std::collections::{BTreeMap, BTreeSet};

pub use rigger_domain::config::*;

/// Config is a fully loaded, validated harness configuration.
#[derive(Clone, Debug, Default)]
pub struct Config {
    pub agents: BTreeMap<String, AgentDef>,
    pub workflow: Workflow,
    /// The operator instruction layer (`.rigger/instructions/*.md`, filename order), composed
    /// into every spawned agent's system prompt after the built-in layer
    /// ([`crate::instructions::compose`]).
    pub instructions: Vec<crate::instructions::Instruction>,
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
