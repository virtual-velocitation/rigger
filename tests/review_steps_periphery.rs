//! Spec 120, criteria 6 (THE MAP FORM IS THE CANONICAL SEQUENCE), 7 (THE LIST FORM DECLARES ANY
//! SEQUENCE), 8 (A LIST-FORM SEQUENCE RUNS), 9 (TOKEN READERS CLASSIFY EVERY STEP BY ITS KIND) and
//! 10 (PANEL READERS READ EVERY STEP), from outside the conductor.
//!
//! The domain's own tests pin `ReviewPanel::steps` over hand-built panels and the conductor's
//! pin the review identity surface over a recording driver. Four facts sit past their reach: the
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
//!
//! Past those sits a list running: the compiled `rigger step` walks one repeating a kind, a fresh
//! process per step, each step printed under its own token with the roster of every earlier one.
//! The walk runs through an adjudicator's reject into a next round, the log it leaves parses back
//! through `review_step_of`, and a map-form run rebased onto its canonical list keeps its ids.
//!
//! Past that sit the token readers: a walked round's own spawn ids and findings, a real respawn
//! among them, read back through the compiled `rigger stats` and `rigger dash --export` by kind,
//! the map form folding as ever.
//!
//! Past a run sits the canary, spec 120's criterion 10 (PANEL READERS READ EVERY STEP): the
//! compiled `rigger canary` walks every step of a panel under its token, each spawn's argv carrying
//! its tier's `--model` pin, shows the adjudicator every finding in order and reversed, scores a
//! corpus item into the tier of the step that caught it, and refuses a list with no adjudicator
//! step.

mod common;

#[cfg(unix)]
use common::cli::run_rigger_envs;
use common::cli::{
    cargo_project, loaded_config, read_run_events, rigger_file, run_rigger, run_rigger_ok,
    temp_project, write_scaffold,
};
#[cfg(feature = "symbols")]
use common::cli::{edge_lines, run_around};
use common::fixtures::{
    adversary_first_steps, agent_with_its_own_alias, count_status_marker, declaring, lenses,
    light_of, panel_of, panel_rule_refusals, repeated_kind_lists, repo_review_panel,
    repo_review_steps, separator_refusal, speculation_regen_door_cfg, status_markers,
    WriteAndApprove, ADVERSARY_FIRST_LIST, EMIT_ONLY, LIGHT_PANEL_FORMS, PANEL_AGENTS,
    PANEL_SCOPES, REJECT_FEATURE, REPEATED_KIND_LIST,
};
#[cfg(unix)]
use common::fixtures::{read_text, CONTROL_ITEM, PROMPT_CAPTURE_DIR};
use common::git::{commit_files, temp_git_project_with_commit};
use common::repo::repo_root;
#[cfg(unix)]
use common::repo::stub_path;
use rigger::conductor::{run, Deps, META_REPLAY_KEY, STREAM};
use rigger::config::ReviewPanel;
use rigger::config::ReviewStep::{self, Adjudicator, Adversary};
use rigger::config_store::parse_workflow;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, Event, EventStore};
use rigger::gate::ExecRunner;
use rigger::metrics::META_MODEL_ALIAS;
use rigger::spawn::{
    requests, review_step_of, spawn_role, ReviewKind, SpawnEvent, SpawnResult, TYPE_SPAWN_RESULT,
};
use std::fmt::Display;
use std::path::{Path, PathBuf};

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

/// `root` as a project declaring `panel` as its `defaults.review`, with `item` its one corpus file.
#[cfg(unix)]
fn write_canary_project(root: &Path, panel: &str, item: &str) {
    write_panel_project(
        root,
        &format!(
            "defaults:\n  grounder: nop\n  review: {panel}\nstages:\n  solo:\n    agent: a\n    \
             on_pass: none\n"
        ),
    );
    let corpus = root.join("corpus");
    std::fs::create_dir_all(&corpus).unwrap();
    std::fs::write(corpus.join("item.md"), item).unwrap();
}

/// `rigger canary` plus `flags`, run in `root` over the real subprocess driver with the checked-in
/// `stub` standing as `claude` and `capture` naming the path `target` in its environment.
#[cfg(unix)]
fn canary_on_stub(
    root: &Path,
    (stub, capture): (&str, &str),
    target: &Path,
    flags: &[&str],
) -> (String, String, bool) {
    let work = tempfile::tempdir().unwrap();
    let path = stub_path(work.path(), "claude", Some(stub));
    let args: Vec<&str> = ["canary", "--corpus", "corpus"]
        .iter()
        .chain(flags)
        .copied()
        .collect();
    run_rigger_envs(
        root,
        &args,
        &[("PATH", &path), (capture, target.to_str().unwrap())],
    )
}

/// `rigger canary` run in `root`, a one-item corpus beside `panel` as its `defaults.review`, over
/// the real subprocess driver standing on a stub agent.
#[cfg(unix)]
fn canary_of(root: &Path, panel: &str) -> (String, String, bool) {
    write_canary_project(root, panel, CONTROL_ITEM);
    let capture = tempfile::tempdir().unwrap();
    canary_on_stub(
        root,
        ("prompt-capture-agent.sh", PROMPT_CAPTURE_DIR),
        capture.path(),
        &[],
    )
}

/// Given a corpus and a panel declared as a list repeating a kind, when the operator runs
/// `rigger canary`, then the item is scored against that panel.
#[cfg(unix)]
#[test]
fn rigger_canary_scores_a_list_repeating_a_kind() {
    let dir = temp_project();
    let (out, err, ok) = canary_of(dir.path(), REPEATED_KIND_LIST);
    assert!(ok, "a list with an adjudicator step scores; stderr:\n{err}");
    let scored: Vec<&str> = out
        .lines()
        .filter_map(|line| line.split_once(": scored "))
        .map(|(_batch, scored)| scored)
        .collect();
    assert_eq!(scored, ["1 corpus item(s) against the review panel"]);
}

/// Given a list with no adjudicator step, when the operator runs `rigger canary`, then the run
/// stops on the one line naming the gating verdict the canary measures, before it scores anything.
#[cfg(unix)]
#[test]
fn rigger_canary_refuses_a_list_with_no_adjudicator_step() {
    for panel in [
        "{ steps: [{ lenses: [a] }, { adversary: x }] }",
        "{ steps: [{ lenses: [a] }] }",
    ] {
        let dir = temp_project();
        let (out, err, ok) = canary_of(dir.path(), panel);
        assert!(!ok, "{panel} is refused; stdout:\n{out}");
        assert_eq!(out, "", "{panel}");
        assert_eq!(
            err,
            "rigger: conductor: canary: the review panel names no adjudicator - the canary \
             measures the gating verdict, so an adjudicator is required\n",
            "{panel}"
        );
    }
}

/// The `--model` value a spawn's argv carries when the run pins nothing on it: the stub's word for
/// no flag.
#[cfg(unix)]
const NO_MODEL: &str = "none";

/// Each reviewer of [`REPEATED_KIND_LIST`], in its order, unpinned.
#[cfg(unix)]
const REPEATED_KIND_REVIEWERS: &[(&str, &str)] = &[
    ("lens:a", NO_MODEL),
    ("lens:b", NO_MODEL),
    ("adversary", NO_MODEL),
    ("lens:y@2", NO_MODEL),
    ("adversary@2", NO_MODEL),
];

/// A list whose later step of each kind names an agent no earlier step does, so a tier pin that
/// stops at the first step of its kind leaves that agent on no pin.
#[cfg(unix)]
const DISTINCT_AGENTS_LIST: &str =
    "{ steps: [{ adversary: x }, { lenses: [a] }, { adversary: y }, \
     { lenses: [b] }, { adjudicator: z }] }";

/// One agent in a lens step and an adversary step.
#[cfg(unix)]
const ONE_AGENT_TWO_KINDS_LIST: &str =
    "{ steps: [{ lenses: [x] }, { adversary: x }, { adjudicator: z }] }";

/// The file no reviewer raises a finding about, as its anchor token.
#[cfg(unix)]
const UNREVIEWED: &str = "unreviewed";

/// The stub that logs a canary's walk, and the variable naming its log.
#[cfg(unix)]
const WALK_STUB: (&str, &str) = ("canary-panel-agent.sh", "CANARY_WALK");

/// A `rigger canary --jobs 1` over a panel, the serial walk the stub's one log records in order.
#[cfg(unix)]
struct CanaryWalk {
    /// What the row proves.
    why: &'static str,
    /// The `defaults.review` the project declares.
    panel: &'static str,
    /// The value of each `--model` flag the run passes, in flag order.
    pins: &'static [&'static str],
    /// The role token whose `<token>.rs` the planted item is anchored at.
    anchor: &'static str,
    /// Each reviewer spawned, in order: its role token and its argv's `--model` value.
    reviewers: &'static [(&'static str, &'static str)],
    /// The `--model` value on each adjudicator spawn's argv.
    adjudicator: &'static str,
    /// The item's progress line up to its duration.
    line: &'static str,
    /// The findings the adversary tier then the lens tier raised.
    raised: [u32; 2],
}

/// The rows of [`rigger_canary_walks_every_step_of_a_panel_under_its_token_and_its_tiers_pin`].
#[cfg(unix)]
const CANARY_WALKS: [CanaryWalk; 13] = [
    CanaryWalk {
        why: "a later lens step scores the lens tier",
        panel: REPEATED_KIND_LIST,
        pins: &[],
        anchor: "lens:y@2",
        reviewers: REPEATED_KIND_REVIEWERS,
        adjudicator: NO_MODEL,
        line: "canary: planted verdict correct caught lens",
        raised: [2, 3],
    },
    CanaryWalk {
        why: "a later adversary step scores the adversary tier",
        panel: REPEATED_KIND_LIST,
        pins: &[],
        anchor: "adversary@2",
        reviewers: REPEATED_KIND_REVIEWERS,
        adjudicator: NO_MODEL,
        line: "canary: planted verdict correct caught adversary",
        raised: [2, 3],
    },
    CanaryWalk {
        why: "no finding names the anchor",
        panel: REPEATED_KIND_LIST,
        pins: &[],
        anchor: UNREVIEWED,
        reviewers: REPEATED_KIND_REVIEWERS,
        adjudicator: NO_MODEL,
        line: "canary: planted verdict correct caught none",
        raised: [2, 3],
    },
    CanaryWalk {
        why: "the list's order, the adversary ahead of the lenses",
        panel: ADVERSARY_FIRST_LIST,
        pins: &[],
        anchor: "adversary",
        reviewers: &[("adversary", NO_MODEL), ("lens:a", NO_MODEL)],
        adjudicator: NO_MODEL,
        line: "canary: planted verdict correct caught adversary",
        raised: [1, 1],
    },
    CanaryWalk {
        why: "the map form walks its canonical list under bare tokens",
        panel: "{ adjudicator: z, adversary: x, lenses: [a, b] }",
        pins: &[],
        anchor: "lens:b",
        reviewers: &[
            ("lens:a", NO_MODEL),
            ("lens:b", NO_MODEL),
            ("adversary", NO_MODEL),
        ],
        adjudicator: NO_MODEL,
        line: "canary: planted verdict correct caught lens",
        raised: [1, 2],
    },
    CanaryWalk {
        why: "an adversary pin reaches every adversary step and no other",
        panel: DISTINCT_AGENTS_LIST,
        pins: &["adversary=pin-adversary"],
        anchor: UNREVIEWED,
        reviewers: &[
            ("adversary", "pin-adversary"),
            ("lens:a", NO_MODEL),
            ("adversary@2", "pin-adversary"),
            ("lens:b@2", NO_MODEL),
        ],
        adjudicator: NO_MODEL,
        line: "canary: planted verdict correct caught none",
        raised: [2, 2],
    },
    CanaryWalk {
        why: "a lens pin reaches every lens step and no other",
        panel: DISTINCT_AGENTS_LIST,
        pins: &["lens=pin-lens"],
        anchor: UNREVIEWED,
        reviewers: &[
            ("adversary", NO_MODEL),
            ("lens:a", "pin-lens"),
            ("adversary@2", NO_MODEL),
            ("lens:b@2", "pin-lens"),
        ],
        adjudicator: NO_MODEL,
        line: "canary: planted verdict correct caught none",
        raised: [2, 2],
    },
    CanaryWalk {
        why: "an adjudicator pin reaches both probes and no reviewer",
        panel: DISTINCT_AGENTS_LIST,
        pins: &["adjudicator=pin-adjudicator"],
        anchor: UNREVIEWED,
        reviewers: &[
            ("adversary", NO_MODEL),
            ("lens:a", NO_MODEL),
            ("adversary@2", NO_MODEL),
            ("lens:b@2", NO_MODEL),
        ],
        adjudicator: "pin-adjudicator",
        line: "canary: planted verdict correct caught none",
        raised: [2, 2],
    },
    CanaryWalk {
        why: "one agent under two pins takes the last key, the lens",
        panel: ONE_AGENT_TWO_KINDS_LIST,
        pins: &["lens=pin-lens", "adversary=pin-adversary"],
        anchor: UNREVIEWED,
        reviewers: &[("lens:x", "pin-lens"), ("adversary", "pin-lens")],
        adjudicator: NO_MODEL,
        line: "canary: planted verdict correct caught none",
        raised: [1, 1],
    },
    CanaryWalk {
        why: "the flags' order does not change which key applies last",
        panel: ONE_AGENT_TWO_KINDS_LIST,
        pins: &["adversary=pin-adversary", "lens=pin-lens"],
        anchor: UNREVIEWED,
        reviewers: &[("lens:x", "pin-lens"), ("adversary", "pin-lens")],
        adjudicator: NO_MODEL,
        line: "canary: planted verdict correct caught none",
        raised: [1, 1],
    },
    CanaryWalk {
        why: "one finding has no reversed probe, and the adversary key follows the adjudicator's",
        panel: "{ steps: [{ adversary: x }, { adjudicator: x }] }",
        pins: &["adjudicator=pin-adjudicator", "adversary=pin-adversary"],
        anchor: "adversary",
        reviewers: &[("adversary", "pin-adversary")],
        adjudicator: "pin-adversary",
        line: "canary: planted verdict correct caught adversary",
        raised: [1, 0],
    },
    CanaryWalk {
        why: "two findings across two steps are the fewest with a reversed probe",
        panel: "{ steps: [{ adversary: x }, { adversary: y }, { adjudicator: z }] }",
        pins: &[],
        anchor: "adversary@2",
        reviewers: &[("adversary", NO_MODEL), ("adversary@2", NO_MODEL)],
        adjudicator: NO_MODEL,
        line: "canary: planted verdict correct caught adversary",
        raised: [2, 0],
    },
    CanaryWalk {
        why: "an adjudicator step alone judges no findings once",
        panel: "{ steps: [{ adjudicator: z }] }",
        pins: &[],
        anchor: UNREVIEWED,
        reviewers: &[],
        adjudicator: NO_MODEL,
        line: "canary: planted verdict WRONG caught none",
        raised: [0, 0],
    },
];

/// The log the stub leaves for `row`: each reviewer spawn in order, the adjudicator's first probe
/// with every finding in the order the steps raised them, and with two or more findings its
/// second probe, the whole list reversed.
#[cfg(unix)]
fn expected_walk(row: &CanaryWalk) -> String {
    let spawn = |role: &str, model: &str| format!("canary:planted:{role} {model}\n");
    let numbered = |roles: &[&str]| -> String {
        let lines = roles.iter().enumerate();
        lines
            .map(|(index, role)| format!("{}. [{role}.rs] {role}\n", index + 1))
            .collect()
    };
    let mut roles: Vec<&str> = row.reviewers.iter().map(|(role, _)| *role).collect();
    let mut walk: String = row
        .reviewers
        .iter()
        .map(|(role, model)| spawn(role, model))
        .collect();
    walk += &spawn("adjudicator:a", row.adjudicator);
    walk += &numbered(&roles);
    if roles.len() >= 2 {
        roles.reverse();
        walk += &spawn("adjudicator:b", row.adjudicator);
        walk += &numbered(&roles);
    }
    walk
}

/// Given a panel, a corpus item and `--model` pins, when the operator runs `rigger canary`, then
/// every step spawns in the panel's order under its own token, each spawn's argv carries its
/// tier's pin, the adjudicator is shown every finding in order and then reversed, and the item is
/// scored into the tier of the step that caught it.
#[cfg(unix)]
#[test]
fn rigger_canary_walks_every_step_of_a_panel_under_its_token_and_its_tiers_pin() {
    for row in &CANARY_WALKS {
        let why = row.why;
        let dir = temp_project();
        let root = dir.path();
        let item = format!(
            "---\nid: planted\ndefect_class: off-by-one\nplanted: true\nanchor: \"{}.rs\"\n\
             expected_verdict: reject\n---\nfn planted() {{}}\n",
            row.anchor
        );
        write_canary_project(root, row.panel, &item);
        let scratch = tempfile::tempdir().unwrap();
        let walk = scratch.path().join("walk");
        let mut flags = vec!["--jobs", "1"];
        for pin in row.pins {
            flags.extend(["--model", pin]);
        }
        let (out, err, ok) = canary_on_stub(root, WALK_STUB, &walk, &flags);
        assert!(ok, "{why}; stderr:\n{err}");
        assert_eq!(read_text(&walk), expected_walk(row), "{why}");
        let progress = out.lines().next().and_then(|line| line.split_once(" in "));
        assert_eq!(progress.map(|(line, _took)| line), Some(row.line), "{why}");
        let raised: Vec<&str> = out
            .lines()
            .skip_while(|line| !line.starts_with("  findings raised by tier"))
            .skip(1)
            .take_while(|line| line.starts_with("    "))
            .collect();
        assert_eq!(
            raised,
            [
                format!("    adversary        {}", row.raised[0]),
                format!("    lens             {}", row.raised[1]),
            ],
            "{why}"
        );
    }
}

/// Given a list with no adjudicator step, when the operator runs `rigger canary`, then the refusal
/// comes ahead of the panel: no step of the list ever spawns.
#[cfg(unix)]
#[test]
fn a_canary_refused_for_no_adjudicator_step_spawns_no_agent() {
    let dir = temp_project();
    let root = dir.path();
    write_canary_project(
        root,
        "{ steps: [{ lenses: [a] }, { adversary: x }] }",
        CONTROL_ITEM,
    );
    let scratch = tempfile::tempdir().unwrap();
    let walk = scratch.path().join("walk");
    let (out, _err, ok) = canary_on_stub(root, WALK_STUB, &walk, &[]);
    assert!(!ok, "the list is refused; stdout:\n{out}");
    assert!(!walk.exists(), "no agent spawned: {}", read_text(&walk));
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

/// Given a list naming a kind twice, in each scope a panel is declared in, when the operator
/// validates it, it passes and the workflow loads that very sequence.
#[test]
fn rigger_validate_accepts_a_list_repeating_a_step_kind_and_it_loads_as_declared() {
    let project = temp_project();
    for (panel, steps) in repeated_kind_lists() {
        for (scope, template, read) in PANEL_SCOPES {
            write_panel_project(project.path(), &declaring(template, panel));
            run_rigger_ok(project.path(), &["validate"]);
            let workflow = loaded_config(project.path()).workflow;
            assert_eq!(read(&workflow).steps(), steps, "{scope}: {panel}");
        }
    }
}

/// One `rigger step` run in `root`: the wave it prints, each spawn's id with the roster its item
/// carries, in id order since a lens step's spawns race, and whether it printed the run done.
fn printed_step(root: &Path) -> (Vec<(String, Vec<String>)>, bool) {
    let out = run_rigger_ok(root, &["step"]);
    let step: serde_json::Value = serde_json::from_str(&out).expect("the step prints json");
    let mut wave: Vec<(String, Vec<String>)> = step["wave"]
        .as_array()
        .expect("the step prints a wave")
        .iter()
        .map(|item| {
            let roster = item["reviews"].as_array().map_or(Vec::new(), |roster| {
                let tokens = roster
                    .iter()
                    .map(|token| token.as_str().unwrap().to_string());
                tokens.collect()
            });
            (item["id"].as_str().unwrap().to_string(), roster)
        })
        .collect();
    wave.sort();
    (wave, step["done"] == serde_json::json!(true))
}

/// The answer a finding reviewer gives a walk's step.
const NO_BLOCKER: &str = "reviewed: no blocker";

/// The adjudicator verdict that lands a walk's round.
const APPROVE: &str = r#"{"verdict":"approve"}"#;

/// The map-form panel a walk declares.
const MAP_PANEL: &str = "{ lenses: [a, b], adversary: x, adjudicator: z }";

/// A finding a walked round raises: its id, the id of the spawn raising it and the file it flags.
type RaisedFinding<'a> = (&'a str, &'a str, &'a str);

/// One step of a walk: its tokens, the roster each of them parks with and the answer each gets.
type WalkedStep<'a> = (&'a [&'a str], &'a [&'a str], &'a str);

/// The workflow of a walk, for [`declaring`]: one stage `s` reviewed by its `{panel}`.
const WALKED_WORKFLOW: &str = "defaults:\n  grounder: nop\n  budget: 60\ngates:\n  ok: { run: \
                               \"true\", kind: core }\nstages:\n  s:\n    agent: a\n    gates: \
                               [ok]\n    on_pass: none\n    review: {panel}\n";

/// Drives a project reviewed by `panel` past its implementer: `rigger step` parks that spawn
/// alone, and its commit of `feature.rs` and its result open the round. Returns the unit's
/// worktree.
fn implement_reviewed_by(root: &Path, panel: &str) -> PathBuf {
    write_panel_project(root, &declaring(WALKED_WORKFLOW, panel));
    let (wave, done) = printed_step(root);
    assert_eq!(wave, [("s/implementer#0".to_string(), vec![])]);
    assert!(!done);
    let worktree = common::default_scratch_root(root).join("rigger-wt-s");
    commit_files(&worktree, &[("feature.rs", "pub fn work() {}\n")], "wip");
    run_rigger_ok(root, &["result", "s/implementer#0", "implemented the unit"]);
    worktree
}

/// Walks `steps` of stage `s` at `attempt`, a process per step: each parks exactly its tokens
/// under its roster and `rigger result` answers every one of them. `attempt` is what follows the
/// `#` of each spawn id, so `0~retry1` walks a respawn of attempt 0.
fn walk_steps(root: &Path, attempt: impl Display, steps: &[WalkedStep]) {
    walk_raising(root, attempt, steps, &[]);
}

/// [`walk_steps`], each parked spawn first raising, through `rigger emit` under its step token,
/// the `raised` findings that name its id.
fn walk_raising(
    root: &Path,
    attempt: impl Display,
    steps: &[WalkedStep],
    raised: &[RaisedFinding],
) {
    for (tokens, roster, answer) in steps {
        let ids: Vec<String> = tokens
            .iter()
            .map(|token| format!("s/{token}#{attempt}"))
            .collect();
        let (wave, done) = printed_step(root);
        let roster: Vec<String> = roster.iter().map(|token| token.to_string()).collect();
        let parked = ids.iter().map(|id| (id.clone(), roster.clone()));
        assert_eq!(wave, parked.collect::<Vec<_>>(), "{ids:?}");
        assert!(!done, "{ids:?}");
        for id in &ids {
            let own = raised.iter().filter(|(_, raiser, _)| raiser == id);
            for (finding, _, file) in own {
                let by = spawn_role(id);
                let data = serde_json::json!({
                    "id": finding, "by": by, "summary": "a finding", "about": [file],
                });
                let data = data.to_string();
                run_rigger_ok(root, &["emit", "--spawn", id, "ReviewFinding", &data]);
            }
            run_rigger_ok(root, &["result", id, answer]);
        }
    }
}

/// The five steps of [`REPEATED_KIND_LIST`], its adjudicator step answering `verdict`.
fn repeated_kind_round(verdict: &str) -> [WalkedStep<'_>; 5] {
    [
        (&["lens:a", "lens:b"], &[], NO_BLOCKER),
        (&["adversary"], &["lens:a", "lens:b"], NO_BLOCKER),
        (&["lens:y@2"], &[], NO_BLOCKER),
        (
            &["adversary@2"],
            &["lens:a", "lens:b", "adversary", "lens:y@2"],
            NO_BLOCKER,
        ),
        (
            &["adjudicator"],
            &["lens:a", "lens:b", "adversary", "lens:y@2", "adversary@2"],
            verdict,
        ),
    ]
}

/// Given a list repeating the lens and adversary kinds, when the operator drives it through
/// `rigger step`, a process per step answered by `rigger result`, then each step parks under its
/// own token with the roster of every earlier spawn, and the adjudicator's approve ends the run.
#[test]
fn rigger_step_walks_a_list_repeating_a_kind_under_each_steps_token_and_roster() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    implement_reviewed_by(root, REPEATED_KIND_LIST);
    walk_steps(root, 0, &repeated_kind_round(APPROVE));
    assert_eq!(printed_step(root), (vec![], true));
}

/// Given a list repeating a kind, when its adjudicator step rejects, then the reject is the
/// round's as at HEAD: no `reviewed` stamp lands, the implementer is handed the required item at
/// the next attempt, a second `rigger step` on that parked spawn mints nothing new, and the next
/// round runs every step again under the same tokens and rosters. The log the walk leaves parses
/// back through `review_step_of`, and `is_adjudicator` picks out the two verdicts, each read.
#[test]
fn rigger_step_takes_a_rejected_list_through_remediation_into_a_round_of_the_same_tokens() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    let worktree = implement_reviewed_by(root, REPEATED_KIND_LIST);

    walk_steps(root, 0, &repeated_kind_round(REJECT_FEATURE));

    let remediation = (vec![("s/implementer#1".to_string(), vec![])], false);
    assert_eq!(printed_step(root), remediation);
    assert_eq!(
        printed_step(root),
        remediation,
        "a parked spawn parks again"
    );
    commit_files(
        &worktree,
        &[("feature.rs", "pub fn work() -> u8 { 1 }\n")],
        "fix",
    );
    run_rigger_ok(root, &["result", "s/implementer#1", "closed the finding"]);

    walk_steps(root, 1, &repeated_kind_round(APPROVE));
    assert_eq!(printed_step(root), (vec![], true));

    let events = read_run_events(root);
    let requests = requests(&events).unwrap();
    let mut parsed: Vec<_> = requests
        .iter()
        .map(|request| (request.id.as_str(), review_step_of(spawn_role(&request.id))))
        .collect();
    parsed.sort_by_key(|(id, _)| *id);
    assert_eq!(
        parsed,
        [
            ("s/adjudicator#0", Some((ReviewKind::Adjudicator, 1))),
            ("s/adjudicator#1", Some((ReviewKind::Adjudicator, 1))),
            ("s/adversary#0", Some((ReviewKind::Adversary, 1))),
            ("s/adversary#1", Some((ReviewKind::Adversary, 1))),
            ("s/adversary@2#0", Some((ReviewKind::Adversary, 2))),
            ("s/adversary@2#1", Some((ReviewKind::Adversary, 2))),
            ("s/implementer#0", None),
            ("s/implementer#1", None),
            ("s/lens:a#0", Some((ReviewKind::Lens, 1))),
            ("s/lens:a#1", Some((ReviewKind::Lens, 1))),
            ("s/lens:b#0", Some((ReviewKind::Lens, 1))),
            ("s/lens:b#1", Some((ReviewKind::Lens, 1))),
            ("s/lens:y@2#0", Some((ReviewKind::Lens, 2))),
            ("s/lens:y@2#1", Some((ReviewKind::Lens, 2))),
        ]
    );
    let request_of = |id: &str| requests.iter().find(|request| request.id == id).unwrap();
    for (id, handed) in [("s/implementer#0", 0), ("s/implementer#1", 1)] {
        let required = "- feature.rs: close the unscoped read";
        let prompt = &request_of(id).prompt;
        assert_eq!(prompt.matches(required).count(), handed, "{id}");
    }
    for id in ["s/adjudicator#0", "s/adjudicator#1"] {
        let reviews = &request_of(id).reviews;
        let roster: Vec<_> = reviews.iter().map(|token| review_step_of(token)).collect();
        assert_eq!(
            roster,
            [
                Some((ReviewKind::Lens, 1)),
                Some((ReviewKind::Lens, 1)),
                Some((ReviewKind::Adversary, 1)),
                Some((ReviewKind::Lens, 2)),
                Some((ReviewKind::Adversary, 2)),
            ],
            "{id}"
        );
    }

    let verdicts: Vec<(String, Option<String>)> = events
        .iter()
        .filter(|e| e.type_ == TYPE_SPAWN_RESULT)
        .map(|e| SpawnResult::from_event(e).unwrap())
        .filter(SpawnResult::is_adjudicator)
        .map(|result| {
            let verdict = result.adjudication().and_then(|read| read.verdict);
            (result.id, verdict)
        })
        .collect();
    assert_eq!(
        verdicts,
        [
            ("s/adjudicator#0".to_string(), Some("reject".to_string())),
            ("s/adjudicator#1".to_string(), Some("approve".to_string())),
        ]
    );
    let stamps: Vec<&str> = status_markers(&events, "reviewed")
        .map(|e| e.meta[META_REPLAY_KEY].as_str())
        .collect();
    assert_eq!(stamps, ["s/reviewed#1"]);
}

/// Given a run parked mid-round on a map-form panel, when the operator rewrites the panel as the
/// list spelling its canonical sequence and rebases, then the recorded step answers from the log
/// and the rest of the round parks under the ids and rosters the map form mints, each requested
/// once.
#[test]
fn rigger_step_rebased_from_a_map_onto_its_canonical_list_keeps_the_rounds_ids() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    implement_reviewed_by(root, MAP_PANEL);
    walk_steps(root, 0, &[(&["lens:a", "lens:b"], &[], NO_BLOCKER)]);

    write_panel_project(
        root,
        &declaring(
            WALKED_WORKFLOW,
            "{ steps: [{ lenses: [a, b] }, { adversary: x }, { adjudicator: z }] }",
        ),
    );
    let (_, err, ok) = run_rigger(root, &["step"]);
    assert!(!ok, "premise: the rewrite drifts the pinned definition");
    assert!(err.contains("definition drift"), "{err}");
    run_rigger_ok(root, &["step", "--rebase-definition"]);

    walk_steps(
        root,
        0,
        &[
            (&["adversary"], &["lens:a", "lens:b"], NO_BLOCKER),
            (
                &["adjudicator"],
                &["lens:a", "lens:b", "adversary"],
                APPROVE,
            ),
        ],
    );
    assert_eq!(printed_step(root), (vec![], true));

    let requests = requests(&read_run_events(root)).unwrap();
    let mut ids: Vec<&str> = requests.iter().map(|request| request.id.as_str()).collect();
    ids.sort();
    assert_eq!(
        ids,
        [
            "s/adjudicator#0",
            "s/adversary#0",
            "s/implementer#0",
            "s/lens:a#0",
            "s/lens:b#0",
        ]
    );
}

/// Walks `round` at attempt 0, the one spawn of step `respawned` dying with a recorded error, so
/// the conductor re-parks that step as `~retry1`, which gives the answer `round` spells.
fn walk_respawning(root: &Path, round: &[WalkedStep], respawned: usize, raised: &[RaisedFinding]) {
    let (before, rest) = round.split_at(respawned);
    walk_raising(root, 0, before, raised);
    let (died, after) = rest
        .split_first()
        .expect("the respawned step is in the round");
    run_rigger_ok(root, &["step"]);
    let id = format!("s/{}#0", died.0[0]);
    run_rigger_ok(root, &["result", &id, "--error", "the reviewer died"]);
    walk_raising(root, "0~retry1", std::slice::from_ref(died), raised);
    walk_raising(root, 0, after, raised);
}

/// The line `rigger stats` opens its spawn timing rows with.
const SPAWN_TIMING_HEADER: &str = "  spawn timing per agent (mean / count / total):";

/// What `rigger stats` prints of a run: every line of its review-quality panel, then each spawn
/// timing row as its role and its spawn count, timing's durations being the clock's.
fn stats_read_back(root: &Path) -> (Vec<String>, Vec<(String, String)>) {
    let out = run_rigger_ok(root, &["stats"]);
    let quality = out
        .lines()
        .skip_while(|line| *line != "  review quality:")
        .skip(1)
        .take_while(|line| *line != SPAWN_TIMING_HEADER)
        .map(str::to_string);
    let timing = out
        .lines()
        .skip_while(|line| *line != SPAWN_TIMING_HEADER)
        .skip(1)
        .take_while(|line| line.starts_with("    "))
        .map(|line| {
            let columns: Vec<&str> = line.split(" / ").collect();
            let role = columns[0].split_whitespace().next().unwrap();
            (role.to_string(), columns[1].to_string())
        });
    (quality.collect(), timing.collect())
}

/// The Review stage of unit `s` in the state `rigger dash --export` embeds in its snapshot: each
/// role group beside its agent labels, in the tree's order.
fn exported_review_stage(root: &Path) -> Vec<(String, Vec<String>)> {
    run_rigger_ok(root, &common::cli::DASH_SNAPSHOT_ARGS);
    let html = std::fs::read_to_string(root.join("snapshot.html")).unwrap();
    let state = html
        .lines()
        .find_map(|line| line.trim_start().strip_prefix("const EMBEDDED_STATE = "))
        .and_then(|state| state.strip_suffix(';'))
        .expect("the snapshot embeds the state on one line");
    let state: serde_json::Value = serde_json::from_str(state).unwrap();
    fn label(node: &serde_json::Value) -> String {
        node["label"].as_str().unwrap().to_string()
    }
    fn children(node: &serde_json::Value) -> &[serde_json::Value] {
        node["children"].as_array().unwrap()
    }
    let unit = &state["tree"][0]["children"][0];
    assert_eq!(label(unit), "s");
    let review = children(unit)
        .iter()
        .find(|stage| label(stage) == "Review")
        .expect("the unit has a Review stage");
    children(review)
        .iter()
        .map(|role| (label(role), children(role).iter().map(label).collect()))
        .collect()
}

/// One round the compiled binary walks and the readers read back, every expected value spelled.
struct ReadBack<'a> {
    /// The `review:` panel the workflow declares.
    panel: &'a str,
    /// The round's steps, in the order they park.
    round: &'a [WalkedStep<'a>],
    /// The index in `round` of the one-token step whose first spawn dies.
    respawned: usize,
    /// The findings the round's reviewers raise, by the spawn id that raises each.
    raised: &'a [RaisedFinding<'a>],
    /// Every line of the `rigger stats` review-quality panel, in print order.
    quality: &'a [&'a str],
    /// Each `rigger stats` spawn timing row: its role and its spawn count.
    timing: &'a [(&'a str, &'a str)],
    /// The dash's Review stage: each role group and its agent labels.
    review: &'a [(&'a str, &'a [&'a str])],
}

/// Walks `read_back`'s round to the adjudicator's approve through the compiled binary, then
/// asserts each operator surface over the spawn ids and findings that walk left on the log.
fn assert_read_back(read_back: ReadBack) {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    implement_reviewed_by(root, read_back.panel);
    walk_respawning(root, read_back.round, read_back.respawned, read_back.raised);
    assert_eq!(printed_step(root), (vec![], true));

    let (quality, timing) = stats_read_back(root);
    assert_eq!(quality, read_back.quality);
    let timing_of = |(role, spawns): &(&str, &str)| (role.to_string(), spawns.to_string());
    let expected_timing: Vec<_> = read_back.timing.iter().map(timing_of).collect();
    assert_eq!(timing, expected_timing);

    let review_of = |(role, agents): &(&str, &[&str])| {
        let agents = agents.iter().map(|agent| agent.to_string()).collect();
        (role.to_string(), agents)
    };
    let expected_review: Vec<_> = read_back.review.iter().map(review_of).collect();
    assert_eq!(exported_review_stage(root), expected_review);
}

rigger::test_cases! {
    /// Given a map-form panel, whose every token is bare, when its round runs through the
    /// compiled binary with its adversary respawned once, then `rigger stats` folds cost and the
    /// adversary-only findings by kind with a row per token, and the dash groups each token
    /// under its kind in Review, no agent label naming a step.
    the_readers_fold_a_map_forms_bare_tokens_as_ever: assert_read_back(ReadBack {
        panel: MAP_PANEL,
        round: &[
            (&["lens:a", "lens:b"], &[], NO_BLOCKER),
            (&["adversary"], &["lens:a", "lens:b"], NO_BLOCKER),
            (
                &["adjudicator"],
                &["lens:a", "lens:b", "adversary"],
                r#"{"verdict":"approve","upheld":["a1","l1"],"discarded":["a2","l2"]}"#,
            ),
        ],
        respawned: 1,
        raised: &[
            ("l1", "s/lens:a#0", "src/lens.rs"),
            ("l2", "s/lens:b#0", "src/lens.rs"),
            ("a1", "s/adversary#0~retry1", "src/only.rs"),
            ("a2", "s/adversary#0~retry1", "src/lens.rs"),
        ],
        quality: &[
            "    flip-flop rate     0.0% (0/0 rejects reversed on the same sha)",
            "    lens overlap       50.0% (1/2 flagged files hit by 2+ actors)",
            "    adversary precision 100.0% (1/1 adversary-only findings upheld)",
            "    finding survival per actor (upheld/raised):",
            "      adversary            1/2 (50%)",
            "      lens:a               1/1 (100%)",
            "      lens:b               0/1 (0%)",
            "    rejections by cause (none recorded)",
            "    cost per upheld finding per tier (spawns/upheld):",
            "      adjudicator  1 spawns / 0 upheld (-)",
            "      adversary    2 spawns / 1 upheld (2.0)",
            "      lens         2 spawns / 1 upheld (2.0)",
        ],
        timing: &[
            ("adjudicator", "1 spawns"),
            ("adversary", "2 spawns"),
            ("implementer", "1 spawns"),
            ("lens:a", "1 spawns"),
            ("lens:b", "1 spawns"),
        ],
        review: &[
            ("adjudicator", &["attempt#0"]),
            ("adversary", &["attempt#0", "attempt#0 retry1"]),
            ("lens", &["a", "b"]),
        ],
    });
    /// Given a list repeating the lens and adversary kinds, when its round runs through the
    /// compiled binary with `adversary@2` respawned once, then `rigger stats` keys cost and the
    /// adversary-only fold on each token's kind while finding survival and spawn timing give
    /// each token its own row: a1, a2 and a4 are adversary-only, and a3 is not, since `lens:y@2`
    /// flags its file after the adversary ran. The dash groups every token under its kind in
    /// Review, an agent label naming its step from the second on.
    the_readers_fold_each_step_of_a_repeated_list_by_its_kind: assert_read_back(ReadBack {
        panel: REPEATED_KIND_LIST,
        round: &repeated_kind_round(
            r#"{"verdict":"approve","upheld":["a1","a3","a4","l2"],"discarded":["a2","l1"]}"#,
        ),
        respawned: 3,
        raised: &[
            ("l1", "s/lens:a#0", "src/lens.rs"),
            ("a1", "s/adversary#0", "src/only.rs"),
            ("a3", "s/adversary#0", "src/late.rs"),
            ("l2", "s/lens:y@2#0", "src/late.rs"),
            ("a2", "s/adversary@2#0~retry1", "src/only.rs"),
            ("a4", "s/adversary@2#0~retry1", "src/own.rs"),
        ],
        quality: &[
            "    flip-flop rate     0.0% (0/0 rejects reversed on the same sha)",
            "    lens overlap       50.0% (2/4 flagged files hit by 2+ actors)",
            "    adversary precision 66.7% (2/3 adversary-only findings upheld)",
            "    finding survival per actor (upheld/raised):",
            "      adversary            2/2 (100%)",
            "      adversary@2          1/2 (50%)",
            "      lens:a               0/1 (0%)",
            "      lens:y@2             1/1 (100%)",
            "    rejections by cause (none recorded)",
            "    cost per upheld finding per tier (spawns/upheld):",
            "      adjudicator  1 spawns / 0 upheld (-)",
            "      adversary    3 spawns / 3 upheld (1.0)",
            "      lens         3 spawns / 1 upheld (3.0)",
        ],
        timing: &[
            ("adjudicator", "1 spawns"),
            ("adversary", "1 spawns"),
            ("adversary@2", "2 spawns"),
            ("implementer", "1 spawns"),
            ("lens:a", "1 spawns"),
            ("lens:b", "1 spawns"),
            ("lens:y@2", "1 spawns"),
        ],
        review: &[
            ("adjudicator", &["attempt#0"]),
            (
                "adversary",
                &["attempt#0", "attempt#0 step 2", "attempt#0 step 2 retry1"],
            ),
            ("lens", &["a", "b", "y step 2"]),
        ],
    });
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
fn a_workflow_rigger_init_renders_validates_as_the_canonical_sequence_with_or_without_gates() {
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

/// The model alias on each `reviewed` status of `events`, in log order.
fn reviewed_aliases(events: &[Event]) -> Vec<String> {
    status_markers(events, "reviewed")
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
    /// A list repeating a kind stamps its one adjudicator step, past every token ahead of it.
    a_winner_of_a_list_repeating_a_kind_stamps_its_adjudicator_steps_alias:
        assert_winner_reviewed_aliases(
            parsed_panel(
                "{ steps: [{ lenses: [lens] }, { adversary: adv }, { lenses: [lens] }, \
                 { adversary: adv }, { adjudicator: judge }] }",
            ),
            &["judge-model"],
        );
}
