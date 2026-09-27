//! Workflow-configuration fixtures: agents, review panels and gates.

use rigger::config::{AgentDef, Config, Gate, ReviewPanel, Stage};

/// An agent definition carrying only its `id`.
pub fn agent(id: &str) -> AgentDef {
    AgentDef {
        id: id.to_string(),
        ..Default::default()
    }
}

/// An agent definition `id` whose persona (the markdown body of its definition) is `prompt`.
pub fn agent_with_prompt(id: &str, prompt: &str) -> AgentDef {
    AgentDef {
        id: id.to_string(),
        prompt: prompt.to_string(),
        ..Default::default()
    }
}

/// A config declaring one bare [`agent`] per id.
pub fn cfg_for(ids: &[&str]) -> Config {
    let mut c = Config::default();
    for id in ids {
        c.agents.insert((*id).to_string(), agent(id));
    }
    c
}

/// A review panel of `lenses`, adversary `adv` and adjudicator `adj`, with no tier policy.
pub fn panel_with_lenses(lenses: &[&str]) -> ReviewPanel {
    ReviewPanel {
        lenses: lenses.iter().map(|s| (*s).to_string()).collect(),
        adversary: "adv".into(),
        adjudicator: "adj".into(),
        tiers: None,
    }
}

/// A `core` gate running `run` with no declared inputs.
pub fn gate_def(run: &str) -> Gate {
    gate_def_inputs(run, &[])
}

/// A `core` gate running `run` over the declared `inputs`.
pub fn gate_def_inputs(run: &str, inputs: &[&str]) -> Gate {
    Gate {
        run: run.to_string(),
        kind: "core".to_string(),
        inputs: inputs.iter().map(|s| s.to_string()).collect(),
    }
}

/// A review panel of the single lens `lens`, no adversary, and adjudicator `adj`.
pub fn lens_only_panel() -> ReviewPanel {
    ReviewPanel {
        lenses: vec!["lens".to_string()],
        adversary: String::new(),
        adjudicator: "adj".to_string(),
        tiers: None,
    }
}

/// A review panel of the single lens `lens` adjudicated by `judge`.
pub fn review_panel() -> ReviewPanel {
    ReviewPanel {
        lenses: vec!["lens".into()],
        adjudicator: "judge".into(),
        ..Default::default()
    }
}

/// A `worker` stage `name` gated by `gate`, reviewed by [`review_panel`], merging on pass.
pub fn mk_stage(name: &str, gate: &str) -> Stage {
    Stage {
        name: name.into(),
        agent: "worker".into(),
        gates: vec![gate.into()],
        on_pass: "merge".into(),
        needs: vec![],
        review: review_panel(),
        ..Default::default()
    }
}
