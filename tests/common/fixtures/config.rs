//! Workflow-configuration fixtures: agents, review panels and gates.

use rigger::config::{AgentDef, Config, Gate, ReviewPanel};

/// An agent definition carrying only its `id`.
pub fn agent(id: &str) -> AgentDef {
    AgentDef {
        id: id.to_string(),
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
