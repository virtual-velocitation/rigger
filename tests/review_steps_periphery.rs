//! Spec 120, criterion 6 (THE MAP FORM IS THE CANONICAL SEQUENCE), from outside the conductor.
//!
//! The conductor's own tests pin `ReviewPanel::steps` over hand-built panels and pin the
//! review identity surface over a recording driver. Three facts sit past their reach: the
//! panel a workflow FILE declares reaches `steps()` through the production parser, whatever
//! order the file spells its keys in; the repository panel those identity goldens are captured
//! over is the one `.rigger/workflow.yml` declares, not a copy that drifted from it; and a
//! speculation winner's deferred `reviewed` stamp reads the panel's adjudicator step through
//! the real `rigger::conductor::run`, or is absent when the panel has none.

mod common;

use common::cli::loaded_config;
use common::fixtures::{
    agent, count_status_marker, panel_of, repo_review_panel, speculation_regen_door_cfg,
    WriteAndApprove,
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

/// A lens step of `ids`.
fn lenses(ids: &[&str]) -> ReviewStep {
    ReviewStep::Lenses(ids.iter().map(|id| (*id).to_string()).collect())
}

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

/// The panel the identity goldens run over is the panel the repository declares: this repo's
/// real `.rigger/workflow.yml`, loaded and validated through the production path, gives its
/// implement stage exactly the fixture's steps, and those are the three canonical steps.
#[test]
fn the_repository_workflow_declares_the_identity_fixtures_panel() {
    let workflow = loaded_config(&repo_root()).workflow;
    let panel = workflow.effective_review_panel(&workflow.stages["implement"]);
    let expected = [
        lenses(&["architecture-reviewer", "sdet"]),
        Adversary("adversary".into()),
        Adjudicator("adjudicator".into()),
    ];
    assert_eq!(panel.steps(), expected);
    assert_eq!(repo_review_panel().steps(), expected);
    assert!(
        panel.depth().is_none(),
        "the fixture declares no tier policy, so the file's implement panel must not either"
    );
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
