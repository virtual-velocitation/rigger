//! Spec 120, criteria 6 (THE MAP FORM IS THE CANONICAL SEQUENCE) and 7 (THE LIST FORM DECLARES
//! ANY SEQUENCE), from outside the conductor.
//!
//! The conductor's own tests pin `ReviewPanel::steps` over hand-built panels and pin the
//! review identity surface over a recording driver. Four facts sit past their reach: the
//! panel a workflow FILE declares reaches `steps()` through the production parser, whatever
//! order the file spells its keys in; the repository panel those identity goldens are captured
//! over is the one `.rigger/workflow.yml` declares, not a copy that drifted from it; a
//! speculation winner's deferred `reviewed` stamp reads the panel's adjudicator step through
//! the real `rigger::conductor::run`, or is absent when the panel has none; and an operator's
//! `steps:` list in `.rigger/workflow.yml` is what the compiled binary accepts or refuses, in
//! every scope a panel is declared in, with the line each refusal prints.
//!
//! Past those panel rules sit what reads a loaded panel: the verdict-line lint, the wall-clock
//! advisory, the graph's review edges and the agents `rigger init` scaffolds, each driven here
//! off a real workflow file, and the workflows `rigger init` renders, which keep loading as the
//! canonical sequence.

mod common;

use common::cli::{
    cargo_project, loaded_config, rigger_file, run_rigger, run_rigger_ok, temp_project,
    write_scaffold,
};
#[cfg(feature = "symbols")]
use common::cli::{edge_lines, run_around};
use common::fixtures::{
    adversary_first_steps, agent, count_status_marker, lenses, light_of, panel_of,
    panel_rule_refusals, repo_review_panel, separator_refusal, speculation_regen_door_cfg,
    WriteAndApprove, ADVERSARY_FIRST_LIST, EMIT_ONLY, LIGHT_PANEL_FORMS, PANEL_AGENTS,
    PANEL_SCOPES,
};
use common::git::temp_git_project_with_commit;
use common::repo::repo_root;
use rigger::conductor::{run, Deps, STREAM};
use rigger::config::ReviewStep::{self, Adjudicator, Adversary};
use rigger::config::{AgentDef, ReviewPanel};
use rigger::config_store::parse_workflow;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, Event, EventStore};
use rigger::gate::ExecRunner;
use rigger::ledger;
use rigger::metrics::META_MODEL_ALIAS;
use std::path::Path;

/// The panel a stage named `s` carries when its `review:` key is the flow mapping `panel`, read
/// through the production workflow parser and the stage-panel resolution the conductor uses.
fn parsed_panel(panel: &str) -> ReviewPanel {
    let body = format!("stages:\n  s:\n    agent: worker\n    review: {panel}\n");
    let workflow = parse_workflow(&body).unwrap_or_else(|e| panic!("parse {panel}: {e}"));
    workflow
        .effective_review_panel(&workflow.stages["s"])
        .clone()
}

/// The steps the flow mapping `panel` declares are exactly `expected`, in that order.
fn assert_parsed_steps(panel: &str, expected: &[ReviewStep]) {
    assert_eq!(parsed_panel(panel).steps(), expected, "{panel}");
}

rigger::test_cases! {
    /// The sequence is the canonical one and never the file's key order.
    the_steps_of_a_full_map_follow_the_canonical_order_not_the_key_order: assert_parsed_steps(
        "{ adjudicator: z, adversary: x, lenses: [a, b] }",
        &[lenses(&["a", "b"]), Adversary("x".into()), Adjudicator("z".into())],
    );
    /// A map naming only lenses has the one lens step and nothing gates it.
    a_map_of_lenses_alone_has_one_lens_step: assert_parsed_steps(
        "{ lenses: [a, b] }",
        &[lenses(&["a", "b"])],
    );
    /// A map naming no adversary drops that step and keeps the adjudicator last.
    a_map_with_no_adversary_steps_from_its_lenses_to_its_adjudicator: assert_parsed_steps(
        "{ lenses: [a, b], adjudicator: z }",
        &[lenses(&["a", "b"]), Adjudicator("z".into())],
    );
    /// A map naming no lenses has no empty lens step in front of its adversary.
    a_map_with_no_lenses_starts_at_its_adversary: assert_parsed_steps(
        "{ adversary: x, adjudicator: z }",
        &[Adversary("x".into()), Adjudicator("z".into())],
    );
    /// A key the file spells but leaves empty names no step, exactly as an absent key does.
    a_key_spelled_empty_names_no_step: assert_parsed_steps(
        "{ lenses: [], adversary: \"\", adjudicator: z }",
        &[Adjudicator("z".into())],
    );
    /// The empty map is the panel of no steps.
    the_empty_map_has_no_steps: assert_parsed_steps("{}", &[]);
}

/// The light panel of a parsed `tiers` map is a canonical sequence of its own, and the tier
/// policy adds no step to the full panel it sits on.
#[test]
fn a_tiered_map_keeps_its_full_steps_and_gives_its_light_panel_its_own() {
    let panel = parsed_panel(
        "{ adjudicator: z, adversary: x, lenses: [a], \
         tiers: { threshold: 1, light: { adjudicator: w, lenses: [c] } } }",
    );
    assert_eq!(
        panel.steps(),
        [
            lenses(&["a"]),
            Adversary("x".into()),
            Adjudicator("z".into())
        ]
    );
    let light = &panel.depth().expect("the map declares tiers").light;
    assert_eq!(light.steps(), [lenses(&["c"]), Adjudicator("w".into())]);
}

/// The steps the repository's workflow and every rendering of it declare, spelled literally so
/// no parse or accessor under test stands behind the expected value.
fn repo_review_steps() -> [ReviewStep; 3] {
    [
        lenses(&["architecture-reviewer", "sdet"]),
        Adversary("adversary".into()),
        Adjudicator("adjudicator".into()),
    ]
}

/// The panel the identity goldens run over is the panel the repository declares: this repo's
/// real `.rigger/workflow.yml`, loaded and validated through the production path, gives its
/// implement stage exactly the fixture's steps, and those are the three canonical steps.
#[test]
fn the_repository_workflow_declares_the_identity_fixtures_panel() {
    let workflow = loaded_config(&repo_root()).workflow;
    let panel = workflow.effective_review_panel(&workflow.stages["implement"]);
    let expected = repo_review_steps();
    assert_eq!(panel.steps(), expected);
    assert_eq!(repo_review_panel().steps(), expected);
    assert!(
        panel.depth().is_none(),
        "the fixture declares no tier policy, so the file's implement panel must not either"
    );
}

/// A `rigger init` project whose `defaults.review` is the list `steps` in place of the three map
/// keys the scaffold writes.
fn project_reviewing_with(steps: &str) -> tempfile::TempDir {
    let project = temp_project();
    run_rigger_ok(project.path(), &["init"]);
    let workflow = rigger_file(project.path(), "workflow.yml");
    let scaffolded = std::fs::read_to_string(&workflow).expect("read the scaffolded workflow");
    let (before, panel) = scaffolded
        .split_once("  review:\n")
        .expect("the scaffold declares defaults.review");
    let after = panel
        .splitn(4, '\n')
        .nth(3)
        .expect("the scaffold's panel is its three map keys");
    std::fs::write(
        &workflow,
        format!("{before}  review:\n    steps: {steps}\n{after}"),
    )
    .expect("write the workflow");
    project
}

/// Given a project declaring its review as a list running the adversary ahead of the lenses,
/// when the operator validates it, it passes and the workflow loads that very sequence.
#[test]
fn rigger_validate_accepts_a_list_form_workflow_and_it_loads_as_declared() {
    let project = project_reviewing_with(
        "[{ adversary: adversary }, { lenses: [architecture-reviewer, sdet] }, \
         { adjudicator: adjudicator }]",
    );
    let (_out, err, ok) = run_rigger(project.path(), &["validate"]);
    assert!(ok, "validate must accept the list; stderr:\n{err}");
    assert_eq!(
        loaded_config(project.path())
            .workflow
            .defaults
            .review
            .steps(),
        [
            Adversary("adversary".into()),
            lenses(&["architecture-reviewer", "sdet"]),
            Adjudicator("adjudicator".into()),
        ]
    );
}

/// Given a project declaring a list the panel rules forbid, when the operator validates it, it
/// fails and its last line names the scope, the entry and the rule.
#[test]
fn rigger_validate_refuses_a_step_after_the_adjudicator_naming_the_scope_and_entry() {
    let project =
        project_reviewing_with("[{ adjudicator: adjudicator }, { adversary: adversary }]");
    let (_out, err, ok) = run_rigger(project.path(), &["validate"]);
    assert!(!ok, "validate must refuse the list; stderr:\n{err}");
    assert_eq!(
        err.lines().last(),
        Some(
            "rigger: config: defaults.review steps[0] is an adjudicator step with a step after \
             it: the adjudicator step is the last step"
        )
    );
}

/// Scaffold `root` as the project of the panel tables: `workflow` and a one-line persona per id
/// of [`PANEL_AGENTS`].
fn write_panel_project(root: &Path, workflow: &str) {
    let personas = PANEL_AGENTS.map(|id| (id, format!("---\nid: {id}\n---\nA panel agent.\n")));
    let agents = personas
        .each_ref()
        .map(|(id, persona)| (*id, persona.as_str()));
    write_scaffold(root, &agents, workflow);
}

/// `template` of [`PANEL_SCOPES`] declaring the flow mapping `panel`.
fn declaring(template: &str, panel: &str) -> String {
    template.replace("{panel}", panel)
}

/// `rigger <verb>` run in `root` fails and prints `line` as all of its stderr.
fn assert_stops_with(root: &Path, verb: &str, line: &str) {
    let (_out, err, ok) = run_rigger(root, &[verb]);
    assert!(!ok, "rigger {verb} must fail with {line:?}; stderr:\n{err}");
    assert_eq!(err, format!("{line}\n"), "rigger {verb}");
}

/// Given a panel the rules forbid, in each scope a panel is declared in, when the operator
/// validates it, it fails printing nothing but the one line that names the scope and the rule.
#[test]
fn rigger_validate_refuses_each_forbidden_panel_naming_its_scope() {
    let project = temp_project();
    for (workflow, refusal) in panel_rule_refusals() {
        write_panel_project(project.path(), &workflow);
        let (out, err, ok) = run_rigger(project.path(), &["validate"]);
        assert!(!ok, "validate must refuse {workflow}");
        assert_eq!(
            (out.as_str(), err.as_str()),
            ("", format!("rigger: config: {refusal}\n").as_str()),
            "{workflow}"
        );
    }
}

/// Given a persona file whose id holds the step-token separator, whatever names it, when the
/// operator validates the project or a run reaches its next step, each stops on the one refusal
/// telling the operator to rename the agent.
#[test]
fn an_agent_id_holding_the_separator_stops_validate_and_each_run_entry() {
    let project = temp_project();
    write_scaffold(
        project.path(),
        &[("w", "---\nid: \"w@k\"\n---\nA persona.\n")],
        "defaults: {}\n",
    );
    let line = format!("rigger: config: {}", separator_refusal("agents", "w@k"));
    for verb in ["validate", "step", "run"] {
        assert_stops_with(project.path(), verb, &line);
    }
}

/// The path a parse failure gives the panel of each of [`PANEL_SCOPES`], in that order.
const PANEL_PATHS: [&str; 4] = [
    "defaults.review",
    "stages.s.review",
    "defaults.review.tiers.light",
    "stages.s.review.tiers.light",
];

/// Given a list of the wrong shape, in each scope, when the operator validates it, the parse
/// fails on the path of the offending key, past which only the file position varies by scope.
#[test]
fn rigger_validate_refuses_a_malformed_list_at_the_parse_naming_its_path() {
    let project = temp_project();
    for (panel, failure) in [
        ("{ steps: [{ lens: [a] }] }", "steps[0].lens: unknown key"),
        (
            "{ steps: [{ adjudicator: z }, { lenses: [a], judge: z }] }",
            "steps[1].judge: unknown key",
        ),
        (
            "{ steps: 3 }",
            "steps: invalid type: integer `3`, expected a sequence",
        ),
        (
            "{ steps: [{ lenses: a }] }",
            "steps[0].lenses: invalid type: string \"a\", expected a sequence",
        ),
        (
            "{ steps: [adjudicator] }",
            "steps[0]: invalid type: string \"adjudicator\", expected struct ReviewStepDecl",
        ),
    ] {
        for ((_, template, _), path) in PANEL_SCOPES.iter().zip(PANEL_PATHS) {
            write_panel_project(project.path(), &declaring(template, panel));
            let (out, err, ok) = run_rigger(project.path(), &["validate"]);
            assert!(!ok, "validate must refuse {panel} at {path}");
            assert_eq!(out, "", "{panel} at {path}");
            assert_eq!(
                err.trim_end().split(" at line ").next(),
                Some(format!("rigger: config: parse workflow: {path}.{failure}").as_str()),
                "{panel} at {path}"
            );
        }
    }
}

/// The name an unknown-agent refusal gives the owner of each of [`PANEL_SCOPES`], in that order:
/// a light panel's agents are checked as its enclosing panel's.
const UNKNOWN_AGENT_OWNERS: [&str; 4] = [
    "defaults.review",
    "stage \"s\"",
    "defaults.review",
    "stage \"s\"",
];

/// Given a list step naming an agent the project does not declare, in each scope and on each
/// step kind, when the operator validates it, it fails naming the agent.
#[test]
fn rigger_validate_refuses_an_unknown_agent_on_any_step_of_a_list() {
    let project = temp_project();
    for panel in [
        "{ steps: [{ lenses: [a, ghost] }, { adjudicator: z }] }",
        "{ steps: [{ adversary: ghost }, { adjudicator: z }] }",
        "{ steps: [{ adjudicator: ghost }] }",
    ] {
        for ((_, template, _), owner) in PANEL_SCOPES.iter().zip(UNKNOWN_AGENT_OWNERS) {
            write_panel_project(project.path(), &declaring(template, panel));
            assert_stops_with(
                project.path(),
                "validate",
                &format!("rigger: config: {owner} references unknown agent \"ghost\""),
            );
        }
    }
}

/// Given a list running the adversary ahead of the lenses, in each scope a panel is declared
/// in, when the operator validates it, it passes and the workflow loads that very sequence.
#[test]
fn rigger_validate_accepts_a_list_in_every_scope_and_it_loads_as_declared() {
    let project = temp_project();
    for (scope, template, read) in PANEL_SCOPES {
        write_panel_project(project.path(), &declaring(template, ADVERSARY_FIRST_LIST));
        let out = run_rigger_ok(project.path(), &["validate"]);
        let workflow = loaded_config(project.path()).workflow;
        assert_eq!(
            out.lines().nth(1),
            Some(
                format!(
                    "config valid: 5 agents, {} stages, 0 gates",
                    workflow.stages.len()
                )
                .as_str()
            ),
            "{scope}"
        );
        assert_eq!(read(&workflow).steps(), adversary_first_steps(), "{scope}");
    }
}

/// Given a full panel over a light panel in any pairing of the two forms, when the operator
/// validates it, it passes and each tier loads its own sequence.
#[test]
fn rigger_validate_accepts_a_light_panel_of_either_form_under_either_form() {
    let project = temp_project();
    for panel in LIGHT_PANEL_FORMS {
        for (scope, template, read) in &PANEL_SCOPES[..2] {
            write_panel_project(project.path(), &declaring(template, panel));
            run_rigger_ok(project.path(), &["validate"]);
            let workflow = loaded_config(project.path()).workflow;
            let full = read(&workflow);
            assert_eq!(full.steps(), [Adjudicator("z".into())], "{scope}: {panel}");
            assert_eq!(
                light_of(full).steps(),
                [Adversary("x".into()), Adjudicator("y".into())],
                "{scope}: {panel}"
            );
        }
    }
}

/// Given a default list and a stage declaring its own review, when the operator validates the
/// project, the stage runs the default list when its own panel is empty in either form, and its
/// own list otherwise.
#[test]
fn a_stage_of_no_steps_inherits_the_default_list_and_a_stage_list_replaces_it() {
    let project = temp_project();
    let inherited = adversary_first_steps().to_vec();
    for (stage_review, expected) in [
        ("{ steps: [] }", inherited.clone()),
        ("{}", inherited),
        (
            "{ steps: [{ lenses: [b] }, { adjudicator: y }] }",
            vec![lenses(&["b"]), Adjudicator("y".into())],
        ),
    ] {
        write_panel_project(
            project.path(),
            &format!(
                "defaults:\n  review: {ADVERSARY_FIRST_LIST}\nstages:\n  s:\n    agent: a\n    \
                 review: {stage_review}\n"
            ),
        );
        run_rigger_ok(project.path(), &["validate"]);
        let workflow = loaded_config(project.path()).workflow;
        assert_eq!(
            workflow
                .effective_review_panel(&workflow.stages["s"])
                .steps(),
            expected,
            "{stage_review}"
        );
    }
}

/// Given a project `rigger init` rendered, with no gate template set and with one, when the
/// operator validates it, it passes and its review is the canonical sequence.
#[test]
fn every_workflow_rigger_init_renders_validates_as_the_canonical_sequence() {
    for (project, summary) in [
        (temp_project(), "config valid: 6 agents, 4 stages, 0 gates"),
        (cargo_project(), "config valid: 6 agents, 4 stages, 5 gates"),
    ] {
        run_rigger_ok(project.path(), &["init"]);
        let out = run_rigger_ok(project.path(), &["validate"]);
        assert_eq!(out.lines().nth(1), Some(summary));
        assert_eq!(
            loaded_config(project.path())
                .workflow
                .defaults
                .review
                .steps(),
            repo_review_steps(),
            "{summary}"
        );
    }
}

/// Given a workflow whose list, its light panel and its stage name part of the starter fleet, the
/// list under the defaults or under the stage, when the operator runs `rigger init`, exactly the
/// personas it names are scaffolded, the one only the light panel names among them.
#[test]
fn rigger_init_scaffolds_exactly_the_agents_a_list_names() {
    let panel = "{ steps: [{ adversary: adversary }, { adjudicator: sdet }], \
         tiers: { light: { steps: [{ adjudicator: architecture-reviewer }] } } }";
    for template in [
        "defaults:\n  review: {panel}\nstages:\n  s:\n    agent: rust-engineer\n",
        "stages:\n  s:\n    agent: rust-engineer\n    review: {panel}\n",
    ] {
        let project = temp_project();
        write_scaffold(project.path(), &[], &declaring(template, panel));
        run_rigger_ok(project.path(), &["init"]);
        let mut scaffolded: Vec<String> = std::fs::read_dir(rigger_file(project.path(), "agents"))
            .expect("read the scaffolded agents")
            .map(|entry| entry.expect("read an agent entry").file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .collect();
        scaffolded.sort();
        assert_eq!(
            scaffolded,
            [
                "adversary.md",
                "architecture-reviewer.md",
                "rust-engineer.md",
                "sdet.md",
            ],
            "{template}"
        );
    }
}

/// Given an unbounded default and a gating agent however many steps or tiers name it, when the
/// operator validates the project, the one advisory names each gating agent once and no other.
#[test]
fn rigger_validate_warns_of_each_unbounded_gating_agent_once() {
    let project = temp_project();
    for (panel, roles) in [
        (
            "{ steps: [{ adversary: z }, { lenses: [a, z] }, { adjudicator: z }] }",
            "\"z\"",
        ),
        ("{ lenses: [z], adversary: z, adjudicator: z }", "\"z\""),
        (
            "{ steps: [{ adjudicator: z }], tiers: { light: { steps: [{ adjudicator: z }] } } }",
            "\"z\"",
        ),
        (
            "{ steps: [{ adjudicator: z }], tiers: { light: { steps: [{ adjudicator: y }] } } }",
            "\"y\", \"z\"",
        ),
    ] {
        write_panel_project(project.path(), &format!("defaults:\n  review: {panel}\n"));
        let (_out, err, ok) = run_rigger(project.path(), &["validate"]);
        assert!(ok, "validate must accept {panel}; stderr:\n{err}");
        let advisories: Vec<&str> = err
            .lines()
            .filter(|line| line.contains("gating role(s)"))
            .collect();
        assert_eq!(
            advisories,
            [format!(
                "warning: defaults.max_wall_clock is unbounded (0) and gating role(s) {roles} \
                 carry no per-agent bound, so a hung gating agent is never swept and the run \
                 stalls silently. Set defaults.max_wall_clock (seconds), or a per-agent \
                 max_wall_clock on those roles."
            )],
            "{panel}"
        );
    }
}

/// Given a persona that records its verdict only through an emit, when it holds a list's
/// adjudicator step, on either tier, validation fails naming it, and when it holds any other
/// step the project passes: only the adjudicator step gates.
#[test]
fn rigger_validate_lints_the_adjudicator_step_of_a_list_and_no_other_step() {
    let project = temp_project();
    let refusal = "rigger: config: agent \"z\" is a gating role but its prompt never instructs \
                   it to end its output with a verdict line (e.g. {\"verdict\":\"approve\"}). \
                   The integration gate reads the result channel, not emitted events; a verdict \
                   emitted only via rigger_emit will never gate. Add the verdict line to the \
                   agent's output.";
    for (panel, gated) in [
        ("{ steps: [{ lenses: [a] }, { adjudicator: z }] }", true),
        (
            "{ steps: [{ adjudicator: a }], tiers: { light: { steps: [{ adjudicator: z }] } } }",
            true,
        ),
        ("{ steps: [{ lenses: [z] }, { adjudicator: a }] }", false),
        ("{ steps: [{ adversary: z }, { adjudicator: a }] }", false),
    ] {
        write_panel_project(project.path(), &format!("defaults:\n  review: {panel}\n"));
        std::fs::write(
            rigger_file(project.path(), "agents").join("z.md"),
            format!("---\nid: z\n---\n{EMIT_ONLY}\n"),
        )
        .expect("write the emit-only persona");
        if gated {
            assert_stops_with(project.path(), "validate", refusal);
        } else {
            run_rigger_ok(project.path(), &["validate"]);
        }
    }
}

/// Given a reviewer named on three steps of the full panel and again on the light panel, in
/// either form, the panel under the defaults or under the stage, when the graph is built, it
/// holds one `REVIEWS` and one `REVIEWS_LIGHT` edge for that agent, never a union of the two
/// tiers.
#[cfg(feature = "symbols")]
#[test]
fn graph_around_gives_a_reviewer_of_either_form_one_edge_of_each_tier() {
    let panels = [
        "{ steps: [{ adversary: dual }, { lenses: [dual, solo] }, { adjudicator: dual }], \
         tiers: { light: { steps: [{ lenses: [dual] }, { adjudicator: dual }] } } }",
        "{ lenses: [dual, solo], adversary: dual, adjudicator: dual, \
         tiers: { light: { lenses: [dual], adjudicator: dual } } }",
    ];
    let templates = [
        "defaults:\n  review: {panel}\ngates:\n  fmt:\n    run: \"true\"\n\
         stages:\n  implement:\n    agent: worker\n    gates: [fmt]\n",
        "gates:\n  fmt:\n    run: \"true\"\n\
         stages:\n  implement:\n    agent: worker\n    gates: [fmt]\n    review: {panel}\n",
    ];
    let workflows = panels.map(|panel| templates.map(|template| declaring(template, panel)));
    for workflow in workflows.as_flattened() {
        let project = temp_project();
        write_scaffold(project.path(), &[], workflow);
        run_rigger_ok(project.path(), &["graph", "build"]);
        let (around, err, ok) = run_around(project.path(), "stage:implement", 1);
        assert!(ok, "graph --around must answer; stderr:\n{err}");
        let mut review_edges: Vec<(String, String, String)> = edge_lines(&around)
            .into_iter()
            .filter(|(_, rel, _)| rel.starts_with("REVIEWS"))
            .collect();
        review_edges.sort();
        let edge = |from: &str, rel: &str| {
            (
                format!("agent:{from}"),
                rel.to_string(),
                "stage:implement".to_string(),
            )
        };
        assert_eq!(
            review_edges,
            [
                edge("dual", "REVIEWS"),
                edge("dual", "REVIEWS_LIGHT"),
                edge("solo", "REVIEWS"),
            ],
            "{workflow}"
        );
    }
}

/// An agent `id` spawned with the model alias `<id>-model`, so a stamp names which agent it read.
fn agent_with_its_own_alias(id: &str) -> AgentDef {
    AgentDef {
        model: format!("{id}-model"),
        ..agent(id)
    }
}

/// The model alias on each `reviewed` status of `events`, in log order.
fn reviewed_aliases(events: &[Event]) -> Vec<String> {
    events
        .iter()
        .filter(|e| {
            e.type_ == ledger::TYPE_UNIT_STATUS
                && String::from_utf8_lossy(&e.data).contains(r#""status":"reviewed""#)
        })
        .map(|e| e.meta.get(META_MODEL_ALIAS).cloned().unwrap_or_default())
        .collect()
}

/// A speculating unit reviewed by `panel` runs to its verified winner through the real
/// `rigger::conductor::run`, a real git repo and real worktrees, and the winner's `reviewed`
/// stamps carry exactly `expected` aliases. The one `verified` stamp proves each row reached the
/// stamp site, so an empty `expected` is an absent stamp and never an unreached one.
fn assert_winner_reviewed_aliases(panel: ReviewPanel, expected: &[&str]) {
    let repo = temp_git_project_with_commit();
    let repo_path = repo.path().to_str().unwrap().to_string();
    let mut cfg = speculation_regen_door_cfg("none");
    for id in ["lens", "adv", "judge"] {
        cfg.agents
            .insert(id.to_string(), agent_with_its_own_alias(id));
    }
    cfg.workflow.stages.get_mut("s").unwrap().review = panel;
    let store = Store::open(":memory:").unwrap();
    let driver = WriteAndApprove {
        file: "feature.rs",
        body: "REAL_WORK\n",
    };
    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &ExecRunner,
        repo: repo_path,
        grounder: None,
        graph: None,
        criteria: Vec::new(),
        log: &|_| {},
        hash_blob: &|_| Ok(String::new()),
        check_emit: &rigger::contextgraph::check_emit,
    };
    run(&cfg, &deps).unwrap();

    let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    assert_eq!(
        count_status_marker(&events, "verified"),
        1,
        "premise: the speculation winner must reach its deferred status stamps"
    );
    assert_eq!(reviewed_aliases(&events), expected);
}

rigger::test_cases! {
    /// The stamp names the adjudicator step, past the two steps that run ahead of it.
    a_winner_stamps_reviewed_with_the_adjudicator_steps_alias: assert_winner_reviewed_aliases(
        panel_of(&["lens"], "adv", "judge"),
        &["judge-model"],
    );
    /// The adjudicator step alone is still the step the stamp reads.
    a_winner_of_an_adjudicator_only_panel_stamps_its_alias: assert_winner_reviewed_aliases(
        panel_of(&[], "", "judge"),
        &["judge-model"],
    );
    /// A panel of lenses alone rendered no verdict, so the winner carries no `reviewed` stamp.
    a_winner_of_a_lenses_only_panel_stamps_no_reviewed: assert_winner_reviewed_aliases(
        panel_of(&["lens"], "", ""),
        &[],
    );
    /// An adversary is not an adjudicator: it renders no verdict, so it earns no stamp either.
    a_winner_of_a_panel_with_an_adversary_and_no_adjudicator_stamps_no_reviewed:
        assert_winner_reviewed_aliases(panel_of(&["lens"], "adv", ""), &[]);
    /// A unit with no panel at all approves with no stamp.
    a_winner_of_an_empty_panel_stamps_no_reviewed: assert_winner_reviewed_aliases(
        panel_of(&[], "", ""),
        &[],
    );
}
