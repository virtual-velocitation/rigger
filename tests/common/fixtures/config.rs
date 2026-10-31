//! Workflow-configuration fixtures: agents, review panels and gates, the scopes a panel is
//! declared in with the refusals its rules give there, and the project directories a test
//! creates, the operator instruction directory among them.

use rigger::config::{AgentDef, Config, Gate, ReviewPanel, ReviewStep, Stage, Workflow};
use std::path::{Path, PathBuf};

/// `dir`, created with its parents.
pub fn created_dir(dir: PathBuf) -> PathBuf {
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("create {}: {e}", dir.display()));
    dir
}

/// `root`'s subdirectory `sub`, created, for a command run from below the store's owning root.
pub fn subdirectory_of(root: &Path) -> PathBuf {
    created_dir(root.join("sub"))
}

/// `root`'s operator instruction directory, `.rigger/instructions`, created.
pub fn instructions_dir(root: &Path) -> PathBuf {
    created_dir(root.join(rigger::config::RIGGER_DIR).join("instructions"))
}

/// An agent definition carrying only its `id`.
pub fn agent(id: &str) -> AgentDef {
    AgentDef {
        id: id.to_string(),
        ..Default::default()
    }
}

/// An agent `id` spawned with the model alias `<id>-model`, so a stamp names which agent it read.
pub fn agent_with_its_own_alias(id: &str) -> AgentDef {
    AgentDef {
        model: format!("{id}-model"),
        ..agent(id)
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

/// A lens step of `ids`.
pub fn lenses(ids: &[&str]) -> ReviewStep {
    ReviewStep::Lenses(ids.iter().map(|id| (*id).to_string()).collect())
}

/// A review panel of `lenses`, `adversary` and `adjudicator`, with no tier policy.
pub fn panel_of(lenses: &[&str], adversary: &str, adjudicator: &str) -> ReviewPanel {
    ReviewPanel {
        lenses: lenses.iter().map(|s| (*s).to_string()).collect(),
        adversary: adversary.into(),
        adjudicator: adjudicator.into(),
        steps: None,
        tiers: None,
    }
}

/// A review panel of `lenses`, adversary `adv` and adjudicator `adj`, with no tier policy.
pub fn panel_with_lenses(lenses: &[&str]) -> ReviewPanel {
    panel_of(lenses, "adv", "adj")
}

/// The panel this repository's `.rigger/workflow.yml` declares for its implement stage, held
/// equal to that file by `tests/review_steps_periphery.rs`.
pub fn repo_review_panel() -> ReviewPanel {
    panel_of(
        &["architecture-reviewer", "sdet"],
        "adversary",
        "adjudicator",
    )
}

/// The steps the repository's workflow and every rendering of it declare, spelled literally so
/// no parse or accessor under test stands behind the expected value.
pub fn repo_review_steps() -> [ReviewStep; 3] {
    [
        lenses(&["architecture-reviewer", "sdet"]),
        ReviewStep::Adversary("adversary".to_string()),
        ReviewStep::Adjudicator("adjudicator".to_string()),
    ]
}

/// The ids of the agents a panel of the tables below names.
pub const PANEL_AGENTS: [&str; 5] = ["a", "b", "x", "y", "z"];

/// `template` of [`PANEL_SCOPES`] declaring the flow mapping `panel`.
pub fn declaring(template: &str, panel: &str) -> String {
    template.replace("{panel}", panel)
}

/// A read of one scope's panel out of a parsed workflow.
pub type PanelRead = fn(&Workflow) -> &ReviewPanel;

/// The four scopes a panel is declared in, each as the name its refusal gives the scope, a
/// workflow declaring the flow mapping `{panel}` there and the read of that panel. The two
/// full scopes come first: only a light panel owes an adjudicator step of itself, so a row
/// with none runs over `PANEL_SCOPES[..2]`.
pub const PANEL_SCOPES: [(&str, &str, PanelRead); 4] = [
    ("defaults.review", "defaults:\n  review: {panel}\n", |wf| {
        &wf.defaults.review
    }),
    (
        "stage \"s\" review",
        "stages:\n  s:\n    agent: a\n    review: {panel}\n",
        |wf| &wf.stages["s"].review,
    ),
    (
        "defaults.review.tiers.light",
        "defaults:\n  review:\n    adjudicator: z\n    tiers:\n      light: {panel}\n",
        |wf| light_of(&wf.defaults.review),
    ),
    (
        "stage \"s\" review.tiers.light",
        "stages:\n  s:\n    agent: a\n    review:\n      adjudicator: z\n      tiers:\n        \
         light: {panel}\n",
        |wf| light_of(&wf.stages["s"].review),
    ),
];

/// The light panel of `panel`'s tiers policy.
pub fn light_of(panel: &ReviewPanel) -> &ReviewPanel {
    &panel.depth().expect("the scope declares tiers").light
}

/// A list repeating the lens kind and the adversary kind, one agent in both adversary steps,
/// declaring [`repeated_kind_steps`] of the lens `y`.
pub const REPEATED_KIND_LIST: &str = "{ steps: [{ lenses: [a, b] }, { adversary: x }, \
     { lenses: [y] }, { adversary: x }, { adjudicator: z }] }";

/// The steps of a list in [`REPEATED_KIND_LIST`]'s order, `lens` the one reviewer of its second
/// lens step.
pub fn repeated_kind_steps(lens: &str) -> Vec<ReviewStep> {
    vec![
        lenses(&["a", "b"]),
        ReviewStep::Adversary("x".into()),
        lenses(&[lens]),
        ReviewStep::Adversary("x".into()),
        ReviewStep::Adjudicator("z".into()),
    ]
}

/// Every list naming two steps of one kind, each with the steps it declares. Each step runs
/// under its own token, so a kind repeats freely: only the adjudicator step is one at most.
pub fn repeated_kind_lists() -> [(&'static str, Vec<ReviewStep>); 4] {
    let adjudicator = || ReviewStep::Adjudicator("z".into());
    [
        (
            "{ steps: [{ lenses: [a] }, { lenses: [b] }, { adjudicator: z }] }",
            vec![lenses(&["a"]), lenses(&["b"]), adjudicator()],
        ),
        (
            "{ steps: [{ lenses: [a] }, { lenses: [a] }, { adjudicator: z }] }",
            vec![lenses(&["a"]), lenses(&["a"]), adjudicator()],
        ),
        (
            "{ steps: [{ adversary: x }, { lenses: [a] }, { adversary: y }, { adjudicator: z }] }",
            vec![
                ReviewStep::Adversary("x".into()),
                lenses(&["a"]),
                ReviewStep::Adversary("y".into()),
                adjudicator(),
            ],
        ),
        (REPEATED_KIND_LIST, repeated_kind_steps("y")),
    ]
}

/// A list running the adversary ahead of the lenses, declaring [`adversary_first_steps`].
pub const ADVERSARY_FIRST_LIST: &str =
    "{ steps: [{ adversary: x }, { lenses: [a] }, { adjudicator: z }] }";

/// The steps of [`ADVERSARY_FIRST_LIST`], in its order.
pub fn adversary_first_steps() -> [ReviewStep; 3] {
    [
        ReviewStep::Adversary("x".into()),
        lenses(&["a"]),
        ReviewStep::Adjudicator("z".into()),
    ]
}

/// A full panel of the one step `Adjudicator(z)` over a light panel of the steps
/// `Adversary(x)`, `Adjudicator(y)`, in each form that pairing takes beyond map over map.
pub const LIGHT_PANEL_FORMS: [&str; 3] = [
    "{ steps: [{ adjudicator: z }], tiers: { light: { adversary: x, adjudicator: y } } }",
    "{ adjudicator: z, tiers: { light: { steps: [{ adversary: x }, { adjudicator: y }] } } }",
    "{ steps: [{ adjudicator: z }], tiers: { light: { steps: [{ adversary: x }, { adjudicator: y }] } } }",
];

/// The refusal of a tiers policy whose enclosing full panel has no adjudicator step.
pub const NO_FULL_ADJUDICATOR_STEP: &str = "a review.tiers policy requires its enclosing (full) \
    panel to name an adjudicator step (the gating verdict is mandatory on every review tier \
    - a high-risk unit routes to the full panel, so an empty/adjudicator-less full panel \
    would approve it with no verdict)";

/// The refusal of a tiers policy whose light panel has no adjudicator step.
pub const NO_LIGHT_ADJUDICATOR_STEP: &str = "review.tiers.light must name an adjudicator step \
    (the gating verdict is mandatory on every review tier - only the adversary flexes)";

/// What the refusal of an agent id holding `@` says after naming the agent.
const SEPARATOR_RULE: &str =
    "an agent id never contains '@', the separator of a step token; rename the agent";

/// An adjudicator persona whose ONLY verdict path is `rigger_emit`: the guaranteed
/// stall the verdict-line lint targets (the gate reads the result channel and finds no verdict).
pub const EMIT_ONLY: &str = "You are the Adjudicator. Weigh the lenses against the adversary \
    and decide. Record your verdict the moment you reach it via the rigger_emit tool with \
    type Verdict and data {\"verdict\":\"approve\"} to approve or {\"verdict\":\"reject\"} \
    to reject. Do not add anything after you emit.";

/// The panels that set `steps` and a map key, each with its refusal after the name of the scope:
/// the keys it set are named, an empty list is set, and this refusal comes ahead of the entries
/// it would otherwise read.
fn steps_beside_a_map_key_refusals() -> Vec<(&'static str, String)> {
    let both = |keys: &str| {
        format!(
            "sets steps and {keys}: a panel declares its steps as a list or as a map, \
             never both"
        )
    };
    vec![
        ("{ steps: [], lenses: [a] }", both("lenses")),
        ("{ steps: [], adversary: x }", both("adversary")),
        ("{ steps: [], adjudicator: z }", both("adjudicator")),
        (
            "{ steps: [{ adjudicator: z }], lenses: [a], adjudicator: z }",
            both("lenses, adjudicator"),
        ),
        (
            "{ steps: [{ adjudicator: z }], lenses: [a], adversary: x, adjudicator: z }",
            both("lenses, adversary, adjudicator"),
        ),
        ("{ steps: [{}], lenses: [a] }", both("lenses")),
    ]
}

/// The panels with an entry setting no key or several, each with its refusal after the name of
/// the scope: at the entry's 0-based index, a key being set when non-empty, and ahead of what its
/// keys name.
fn entry_key_count_refusals() -> Vec<(&'static str, String)> {
    let keys = |i: usize, n: usize| {
        format!(
            "steps[{i}] sets {n} keys: an entry sets exactly one of lenses, adversary, \
             adjudicator"
        )
    };
    vec![
        ("{ steps: [{}, { adjudicator: z }] }", keys(0, 0)),
        ("{ steps: [{ adjudicator: z }, {}] }", keys(1, 0)),
        ("{ steps: [{ lenses: [] }] }", keys(0, 0)),
        ("{ steps: [{ adversary: \"\" }] }", keys(0, 0)),
        ("{ steps: [{ lenses: [a], adversary: x }] }", keys(0, 2)),
        (
            "{ steps: [{ adjudicator: z }, { lenses: [a], adversary: x, adjudicator: z }] }",
            keys(1, 3),
        ),
        ("{ steps: [{ lenses: [a, a], adversary: x }] }", keys(0, 2)),
    ]
}

/// The panels naming an agent twice in one lens step, in either form, each with its refusal
/// after the name of the scope, a list naming the step's index.
fn lens_named_twice_refusals() -> Vec<(&'static str, String)> {
    let twice = |step: &str, id: &str| {
        format!("{step} names agent {id:?} twice: a lens step runs each agent once")
    };
    vec![
        ("{ lenses: [a, a], adjudicator: z }", twice("lenses", "a")),
        (
            "{ lenses: [a, b, b], adjudicator: z }",
            twice("lenses", "b"),
        ),
        (
            "{ steps: [{ lenses: [a, a] }, { adjudicator: z }] }",
            twice("steps[0]", "a"),
        ),
        (
            "{ steps: [{ adversary: x }, { lenses: [a, b, a] }, { adjudicator: z }] }",
            twice("steps[1]", "a"),
        ),
    ]
}

/// The panels naming an agent id holding `@`, in either form, each with its refusal after the
/// name of the scope, ahead of the unknown-agent refusal its id would otherwise draw.
fn panel_separator_refusals() -> Vec<(&'static str, String)> {
    vec![
        (
            "{ lenses: [a, \"l@k\"], adjudicator: z }",
            separator_refusal("lenses", "l@k"),
        ),
        (
            "{ adversary: \"@k\", adjudicator: z }",
            separator_refusal("adversary", "@k"),
        ),
        (
            "{ adjudicator: \"k@\" }",
            separator_refusal("adjudicator", "k@"),
        ),
        (
            "{ steps: [{ lenses: [\"l@k\"] }, { adjudicator: z }] }",
            separator_refusal("steps[0]", "l@k"),
        ),
        (
            "{ steps: [{ adversary: \"x@k\" }, { adjudicator: z }] }",
            separator_refusal("steps[0]", "x@k"),
        ),
        (
            "{ steps: [{ adversary: x }, { adjudicator: \"j@k\" }] }",
            separator_refusal("steps[1]", "j@k"),
        ),
    ]
}

/// The panels with a second adjudicator step or a step after the adjudicator, each with its
/// refusal after the name of the scope. A panel has at most one adjudicator step, last, the one
/// kind no list repeats.
fn adjudicator_placement_refusals() -> Vec<(&'static str, String)> {
    let second =
        |i: usize| format!("steps[{i}] is a second adjudicator step: a panel has at most one");
    let followed = |i: usize| {
        format!(
            "steps[{i}] is an adjudicator step with a step after it: the adjudicator step \
             is the last step"
        )
    };
    vec![
        (
            "{ steps: [{ adjudicator: y }, { adjudicator: z }] }",
            second(1),
        ),
        (
            "{ steps: [{ lenses: [a] }, { adjudicator: y }, { adjudicator: z }] }",
            second(2),
        ),
        (
            "{ steps: [{ adjudicator: y }, { lenses: [a] }, { adjudicator: z }] }",
            second(2),
        ),
        (
            "{ steps: [{ adjudicator: z }, { lenses: [a] }] }",
            followed(0),
        ),
        (
            "{ steps: [{ lenses: [a] }, { adjudicator: z }, { adversary: x }] }",
            followed(1),
        ),
    ]
}

/// A workflow declaring `panel` in each of the two full [`PANEL_SCOPES`], with `refusal` after
/// the prefix each of them gives a tier refusal.
fn tier_refusal_rows(panel: &str, refusal: &str) -> [(String, String); 2] {
    let [(_, defaults, _), (_, stage, _), ..] = PANEL_SCOPES;
    [(defaults, "defaults."), (stage, "stage \"s\" ")]
        .map(|(template, prefix)| (declaring(template, panel), format!("{prefix}{refusal}")))
}

/// The workflows whose tiers policy leaves a tier short of an adjudicator step, in either form,
/// each with its exact refusal: a policy owes one on both its tiers, read from `steps()` so
/// either form answers.
fn tier_adjudicator_refusals() -> Vec<(String, String)> {
    let light = "tiers: { light: { adjudicator: z } }";
    [
        (
            format!("{{ lenses: [a], {light} }}"),
            NO_FULL_ADJUDICATOR_STEP,
        ),
        (
            format!("{{ steps: [{{ lenses: [a] }}], {light} }}"),
            NO_FULL_ADJUDICATOR_STEP,
        ),
        (
            format!("{{ steps: [], {light} }}"),
            NO_FULL_ADJUDICATOR_STEP,
        ),
        (
            "{ adjudicator: z, tiers: { light: { lenses: [a] } } }".to_string(),
            NO_LIGHT_ADJUDICATOR_STEP,
        ),
        (
            "{ adjudicator: z, tiers: { light: { steps: [{ lenses: [a] }] } } }".to_string(),
            NO_LIGHT_ADJUDICATOR_STEP,
        ),
        (
            "{ steps: [{ adjudicator: z }], tiers: { light: { steps: [] } } }".to_string(),
            NO_LIGHT_ADJUDICATOR_STEP,
        ),
    ]
    .iter()
    .flat_map(|(panel, refusal)| tier_refusal_rows(panel, refusal))
    .collect()
}

/// The workflows naming an id holding `@` in each agent field of a stage, each with its exact
/// refusal naming that field.
fn stage_separator_refusals() -> Vec<(String, String)> {
    [
        ("agent", "agent: \"w@k\""),
        ("agents", "agents: [a, \"w@k\"]"),
        ("adversary", "adversary: \"w@k\""),
        ("adjudicator", "adjudicator: \"w@k\""),
    ]
    .map(|(field, declaration)| {
        (
            format!("stages:\n  s:\n    {declaration}\n"),
            separator_refusal(&format!("stage \"s\" {field}"), "w@k"),
        )
    })
    .to_vec()
}

/// The refusal of the agent id `id`, holding `@`, where `owner` names it.
pub fn separator_refusal(owner: &str, id: &str) -> String {
    format!("{owner} names agent {id:?}: {SEPARATOR_RULE}")
}

/// Every workflow the panel rules refuse, in every scope each rule applies to, with its exact
/// refusal.
pub fn panel_rule_refusals() -> Vec<(String, String)> {
    let panels = [
        steps_beside_a_map_key_refusals(),
        entry_key_count_refusals(),
        lens_named_twice_refusals(),
        panel_separator_refusals(),
        adjudicator_placement_refusals(),
    ]
    .concat();
    let mut rows = tier_adjudicator_refusals();
    rows.extend(stage_separator_refusals());
    for (panel, message) in panels {
        rows.extend(PANEL_SCOPES.map(|(scope, template, _)| {
            (declaring(template, panel), format!("{scope} {message}"))
        }));
    }
    rows
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
        requires: Vec::new(),
    }
}

/// A review panel of the single lens `lens`, no adversary, and adjudicator `adj`.
pub fn lens_only_panel() -> ReviewPanel {
    ReviewPanel {
        lenses: vec!["lens".to_string()],
        adversary: String::new(),
        adjudicator: "adj".to_string(),
        steps: None,
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
