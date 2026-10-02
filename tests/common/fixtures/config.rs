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

/// A config over the fixture repo at `repo_path` declaring the `worker`, `lens` and `judge`
/// agents, its scratch/worktree root nested inside that repo.
///
/// Spec 89, criterion 2 relocated the scratch/worktree DEFAULT off the fixture's own repo tree
/// onto a machine-wide `<cache-home>/rigger/<encoded repo>` root, so a fixture that leaves
/// `defaults.workdir` unconfigured shares that ONE real location with every other
/// concurrently-running fixture and agent on the machine - a real conductor run a test
/// drives in-process creates real git worktrees there, and an unrelated process's residue/reap
/// scan over that same shared root can legitimately (from its own logic's view) remove a live
/// one mid-test. Nesting the workdir inside THIS fixture's own unique repo tempdir restores the
/// isolation (unique per test, cleaned up when `repo` drops) without depending on any shared
/// machine state.
pub fn scratch_cfg(repo_path: &str) -> Config {
    let mut cfg = Config::default();
    cfg.workflow.defaults.workdir = format!("{repo_path}/.rigger-test-scratch");
    cfg.agents.insert("worker".into(), agent("worker"));
    cfg.agents.insert("lens".into(), agent("lens"));
    cfg.agents.insert("judge".into(), agent("judge"));
    cfg
}

/// [`scratch_cfg`] declaring the always-passing gate `g`.
pub fn gated_scratch_cfg(repo_path: &str) -> Config {
    let mut cfg = scratch_cfg(repo_path);
    cfg.workflow.gates.insert("g".into(), gate_def("exit 0"));
    cfg
}

/// A config of one bare [`agent`] per id in `agents`, one `core` gate per `(name, run)` in
/// `gates`, and `stages`, each keyed by its own name.
pub fn workflow_cfg(agents: &[&str], gates: &[(&str, &str)], stages: Vec<Stage>) -> Config {
    let mut cfg = cfg_for(agents);
    for (name, run) in gates {
        cfg.workflow
            .gates
            .insert((*name).to_string(), gate_def(run));
    }
    for stage in stages {
        cfg.workflow.stages.insert(stage.name.clone(), stage);
    }
    cfg
}

/// A config whose one stage `stage` is run by the bare agent `a` and gated by the always-passing
/// `core` gate `ok`.
pub fn one_gated_stage_cfg(stage: &str) -> Config {
    let mut cfg = Config::default();
    cfg.agents.insert("a".into(), agent("a"));
    cfg.workflow.gates.insert("ok".into(), gate_def("true"));
    cfg.workflow.stages.insert(
        stage.into(),
        Stage {
            name: stage.into(),
            agent: "a".into(),
            gates: vec!["ok".into()],
            ..Default::default()
        },
    );
    cfg
}

/// The `plan` stage: the `planner` agent produces the unit DAG.
pub fn plan_stage() -> Stage {
    Stage {
        name: "plan".into(),
        agent: "planner".into(),
        produces: "dag".into(),
        ..Default::default()
    }
}

/// The `plan-critique` gate after `plan`: `adversary` (empty for none) reviews the DAG and
/// `judge` adjudicates.
pub fn critique_stage(adversary: &str) -> Stage {
    Stage {
        name: "plan-critique".into(),
        needs: vec!["plan".into()],
        adversary: adversary.into(),
        adjudicator: "judge".into(),
        ..Default::default()
    }
}

/// A fan-out implement template `name` run by `worker` after `needs`, gated by `gates`, merging
/// on pass.
pub fn fan_out_stage(name: &str, needs: &[&str], gates: &[&str]) -> Stage {
    Stage {
        name: name.into(),
        agent: "worker".into(),
        strategy: "fan-out".into(),
        needs: needs.iter().map(|s| (*s).to_string()).collect(),
        gates: gates.iter().map(|s| (*s).to_string()).collect(),
        on_pass: "merge".into(),
        ..Default::default()
    }
}

/// A single implement + review stage `s` ([`mk_stage`]: worker implements, one lens, one
/// adjudicator, `on_pass: merge`) over the always-passing gate `gate`.
pub fn review_stage_cfg(gate: &str) -> Config {
    workflow_cfg(
        &["worker", "lens", "judge"],
        &[(gate, "true")],
        vec![mk_stage("s", gate)],
    )
}
