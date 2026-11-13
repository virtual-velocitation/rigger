//! Spec 120, THE CRITIQUE METHOD IS THE PERSONA'S, at the periphery: a project the real
//! `rigger init` scaffolds, driven through its own plan-critique gate by a real `rigger run`, each
//! spawn answered by the fake host: the planner records its proposal through `rigger emit --spawn`,
//! and every other spawn's answer is its session's result.
//!
//! The conductor's and the binary's own tests pin the DAG critique prompt over a stub driver and
//! the scaffold templates as text. What they cannot see is what this file pins: that `rigger init`
//! writes each critiquing persona its method under its critique section, that a run serves each
//! spawn of the gate, byte for byte, that persona composed with the instruction layers as its
//! system prompt and then only the task framing and the output contract as its task - the
//! prompt's "rules in your persona" naming a persona the spawn really carries - and that a
//! consumer's persona with no such section is kept across a re-run of the scaffolding verbs and
//! still critiques over the same task. `rigger critique`'s task is held by
//! `tests/spec_critique_periphery.rs`, which compares it to `spec_critique_prompt`, and that
//! prompt's bytes by the whole-prompt pin in `crates/rigger-domain/src/review.rs`.
//!
//! Spec 121, THE PLANNER PROTOCOL CARRIES NO PLANNING POLICY, over the same gate: the conductor's
//! test pins the plan protocol a stub driver's planner receives, and the binary's the planning
//! policy in the planner persona and its seed as text. What this file adds is that the planner
//! `rigger init` wrote and this repository's committed planner each state the policy, and that a
//! run serves the plan's planner and its re-plan that planner composed, then a task closing with
//! the protocol's framing and contract alone and stating none of the policy.
//!
//! Spec 121, THE UNIT SIZE CAP IS DECLARED, over the same gate: the conductor's test pins both
//! prompts over a configuration it builds in memory, and the config store's the key as YAML reads
//! it. What this file adds is the `defaults.max_unit_added_lines` line a project's workflow.yml
//! carries, the 3000 `rigger init` writes, a cap of 1 or none, reaching what a run serves: the
//! planner's protocol and every critique round state the declared cap's sentence, the critique's
//! cause contract naming a unit over the cap, and a workflow declaring none is served neither;
//! the cap's scope sentence reaches the planner from its persona, never its task.
//!
//! The critique rows are each round's adversary and adjudicator: the first round's for the prompt
//! no reject precedes, the re-plan round's for the prompt that opens with the reject. Each task is
//! compared as the headless host frames it ([`framed`]), and each system prompt with its trailing
//! newlines trimmed, as the fake host reads its file. Every run is minted through
//! `run_store::start_fresh` and adopted by `rigger run`. The driven workflow differs from what the
//! verb wrote in its pinned grounder and, in a row declaring another cap or none, its cap line.

use crate::common;

use common::cli::{
    emit_unit_proposed, loaded_config, rigger_file, run_on_arms, run_rigger_envs_ok, run_rigger_ok,
    temp_repoless_project, widget_unit, with_run_store,
};
use common::fixtures::{
    assert_holds_its_critique_method, composed_system_prompt, critique_method_by_persona,
    critique_reject, dag_critique_prompt, markdown_section, plan_protocol_text, policy_counts,
    read_text, served_task, stand_in_launches, DAG_DISJOINT_OVERLAP, DAG_PRIOR_REJECT_HEADER,
    RECORD_EACH_LAUNCH, UNIT_SIZE_CAP_SCOPE, WHEN_YOU_CRITIQUE_HEADING, WIDGET_CRITERION,
    WIDGET_UNIT,
};
use common::repo::repo_text;
use rigger::driver::claude_code::framed;
use std::path::Path;

/// The agent the scaffolded workflow's implement template fans out to.
const WORKER: &str = "implementer";

/// The blast radius a unit of a project with no grounded source lists.
const NONE_GROUNDED: &str = "(none grounded)";

/// What an adversary's result says: its findings go to the graph, never through this line.
const REVIEWED: &str = "reviewed the DAG";

/// The findings protocol that closes the adversary's task, spelled whole: an oracle reading the
/// production text would pass over a method sentence added to it.
const ADVERSARY_FINDINGS_PROTOCOL: &str = "Record each review finding you raise by calling the \
    rigger_emit tool with type \"ReviewFinding\" and data:\n\
    {\"id\":\"<short-id>\",\"by\":\"adversary\",\"summary\":\"<one line>\",\
    \"about\":[\"<file>\"]}\n\
    The `by` field ATTRIBUTES the finding to you - keep it EXACTLY as \"adversary\" so the \
    review-quality metrics can measure your findings' survival even when you run out-of-process \
    (where the conductor stamps no actor for you). This writes the finding to the shared context \
    graph live, so the adversary, the adjudicator, and your fellow reviewers see it immediately \
    (via grounding and rigger_peers). Never write to this unit's own \
    worktree - it is the tree being judged, not yours to edit. Leave the unit's worktree exactly \
    as you found it.";

/// A repo-less project `rigger init` scaffolded, its fleet and workflow exactly as the verb wrote
/// them.
fn scaffolded_project() -> tempfile::TempDir {
    let dir = temp_repoless_project();
    run_rigger_ok(dir.path(), &["init"]);
    dir
}

/// The text of `root`'s agent definition `file`.
fn agent_text(root: &Path, file: &str) -> String {
    read_text(&rigger_file(root, &format!("agents/{file}")))
}

/// Rewrite the first `from` in `root`'s workflow as `to`; the workflow `rigger init` wrote holds
/// `from`.
fn rewrite_the_workflow(root: &Path, from: &str, to: &str) {
    let path = rigger_file(root, "workflow.yml");
    let workflow = read_text(&path);
    let rewritten = workflow.replacen(from, to, 1);
    assert_ne!(
        rewritten, workflow,
        "premise: the scaffolded workflow holds {from:?}"
    );
    std::fs::write(path, rewritten).unwrap();
}

/// Pin `root`'s scaffolded workflow to the `nop` grounder: the scaffold selects `symbols`, which a
/// binary built without that feature refuses to run over, and `nop` lists the same empty blast
/// radius on both lanes.
fn pin_the_nop_grounder(root: &Path) {
    rewrite_the_workflow(root, "  grounder: symbols", "  grounder: nop");
}

/// The unit size cap the workflow `rigger init` writes declares.
const SCAFFOLD_CAP: u32 = 3000;

/// Replace the unit size cap line of the workflow `rigger init` wrote into `root` with `line`; an
/// empty `line` leaves the workflow declaring no cap.
fn declare_the_cap(root: &Path, line: &str) {
    rewrite_the_workflow(
        root,
        &format!("  max_unit_added_lines: {SCAFFOLD_CAP}\n"),
        line,
    );
}

/// Assert that `root`'s workflow, as the given `given` left it, loads as declaring the unit size
/// cap `cap`, 0 being none.
fn assert_declares_the_cap(given: &str, root: &Path, cap: u32) {
    assert_eq!(
        loaded_config(root).workflow.defaults.max_unit_added_lines,
        cap,
        "premise: {given} loads as declaring the unit size cap {cap}"
    );
}

/// What a spawn was served: its system prompt, then its task.
type Served = (String, String);

/// The gate's spawns, in the order the run launches them: each round's adversary, then its
/// adjudicator.
const GATE_SPAWNS: [&str; 4] = [
    "adversary#0",
    "adjudicator#0",
    "adversary#1",
    "adjudicator#1",
];

/// The arm answering every spawn of a gate that rejects twice on `reject`, each launch on record
/// ([`RECORD_EACH_LAUNCH`]): the plan's planner and its first re-plan propose the widget unit, each
/// adversary reviews, and each round's adjudicator answers `reject`. Any other spawn, the second
/// re-plan first, fails, which ends the run there.
fn gate_arm(reject: &str) -> String {
    format!(
        r#"  *)
{RECORD_EACH_LAUNCH}    case "$RIGGER_SPAWN" in
      plan/implementer#0|plan/replan#1)
{propose}    echo "proposed the DAG" ;;
      plan-critique/adversary#*) echo "{REVIEWED}" ;;
      plan-critique/adjudicator#*) printf '%s' '{reject}' ;;
      *) exit 1 ;;
    esac
    ;;
"#,
        propose = emit_unit_proposed(&widget_unit(WORKER)),
    )
}

/// Drive `root`'s gate with `rigger run` through the first-round reject line `reject`, the
/// re-plan it feeds and the second round ([`gate_arm`]), returning what the plan's planner and its
/// re-plan were served, then what each of [`GATE_SPAWNS`] was served. The run is minted through
/// `run_store::start_fresh` and adopted by `rigger run`; it ends at the second re-plan, which
/// fails, so whether it succeeds is no part of what this pins.
fn served_over_a_reject_and_its_re_plan(root: &Path, reject: &str) -> ([Served; 2], [Served; 4]) {
    pin_the_nop_grounder(root);
    with_run_store(root, |store| {
        rigger::run_store::start_fresh(store, &[], "", "", "", "specs/widget.md").unwrap();
    });
    let (out, err, _ok, fake) =
        run_on_arms(root, &["run"], &root.join("scratch"), &gate_arm(reject));
    let launches = fake.path().join("bin/launches");
    let served = |spawn: &str| -> Served {
        let system = stand_in_launches(&launches)
            .into_iter()
            .find(|launch| launch.spawn == spawn)
            .and_then(|launch| launch.system_prompt)
            .unwrap_or_else(|| panic!("{spawn} was launched; stdout:\n{out}\nstderr:\n{err}"));
        (system, served_task(&launches, spawn))
    };
    (
        ["plan/implementer#0", "plan/replan#1"].map(served),
        GATE_SPAWNS.map(|spawn| served(&format!("plan-critique/{spawn}"))),
    )
}

/// The task each round's adversary and adjudicator must be served over `root`'s scaffolded workflow, declaring the unit size cap `cap` (0 being none),
/// after the reject line `reject`, each with the agent file its persona comes from: the framing,
/// the cap sentence when `cap` is declared, the unit list and the verdict paragraph, the re-plan
/// round led by the reject and the adversary's closed by its findings protocol, and nothing else.
fn expected_tasks(root: &Path, reject: &str, cap: u32) -> [(&'static str, String); 4] {
    let config = loaded_config(root);
    let checkin = config.workflow.stages["checkin"].coverage.as_str();
    let first_round = dag_critique_prompt(
        &[
            ["checkin", checkin, "implement", NONE_GROUNDED],
            [
                WIDGET_UNIT,
                WIDGET_CRITERION,
                "plan-critique",
                NONE_GROUNDED,
            ],
        ],
        DAG_DISJOINT_OVERLAP,
        cap,
    );
    let second_round = format!(
        "{DAG_PRIOR_REJECT_HEADER}{reject}\n\nThe planner has since revised the DAG; re-review it \
         below.\n\n{first_round}"
    );
    [
        (
            "adversary.md",
            format!("{first_round}{ADVERSARY_FINDINGS_PROTOCOL}"),
        ),
        ("adjudicator.md", first_round),
        (
            "adversary.md",
            format!("{second_round}{ADVERSARY_FINDINGS_PROTOCOL}"),
        ),
        ("adjudicator.md", second_round),
    ]
}

/// What a given does to a freshly scaffolded project before the verbs re-run.
type Arrange = fn(&Path);

/// The declared caps each gate's table closes with: a workflow declaring no unit size cap, and one
/// declaring a cap of 1 line, each with the cap it reads as.
const CAP_ROWS: [(&str, Arrange, u32); 2] = [
    (
        "a workflow declaring no unit size cap",
        |root| declare_the_cap(root, ""),
        0,
    ),
    (
        "a workflow declaring a cap of 1 line",
        |root| declare_the_cap(root, "  max_unit_added_lines: 1\n"),
        1,
    ),
];

/// Strike the critique section out of each critiquing persona of `root`, leaving the text a
/// consumer scaffolded before the method moved.
fn strike_the_critique_sections(root: &Path) {
    for (file, _, _) in critique_method_by_persona() {
        let text = agent_text(root, file);
        let section = markdown_section(&text, WHEN_YOU_CRITIQUE_HEADING);
        std::fs::write(
            rigger_file(root, &format!("agents/{file}")),
            text.replace(section, ""),
        )
        .unwrap();
        assert_eq!(
            agent_text(root, file)
                .matches(WHEN_YOU_CRITIQUE_HEADING)
                .count(),
            0,
            "premise: {file} has no critique section"
        );
    }
}

/// Replace the planner `rigger init` wrote into `root` with this repository's committed planner.
fn copy_in_the_committed_planner(root: &Path) {
    std::fs::write(
        rigger_file(root, "agents/planner.md"),
        repo_text(".rigger/agents/planner.md"),
    )
    .unwrap();
}

/// How many times `text`, its whitespace normalized, states each sentence of the
/// [`PLANNING_POLICY`], in its order, and the unit size cap's scope sentence.
fn planning_counts(text: &str) -> ([usize; 6], usize) {
    let text = rigger::wave::normalize_ws(text);
    (
        policy_counts(&text),
        text.matches(UNIT_SIZE_CAP_SCOPE).count(),
    )
}

/// Given a project `rigger init` scaffolded, its planner the one the verb wrote or this
/// repository's committed planner, which states each planning-policy sentence and the unit size
/// cap's scope sentence once, its workflow declaring the cap the verb wrote, a cap of 1 line or no
/// cap at all, when the gate rejects and re-plans, `rigger run` serves the plan's planner and its
/// re-plan that planner composed with the layers, then a task that closes with the plan
/// protocol's framing and contract alone, its cap slot stating the declared cap and empty when
/// none is declared, and states none of the planning policy and no scope sentence.
#[test]
fn each_planner_spawn_is_served_the_planning_policy_by_its_persona_and_none_by_its_task() {
    let givens: [(&str, Arrange, u32); 2] = [
        ("the planner rigger init wrote", |_| {}, SCAFFOLD_CAP),
        (
            "this repository's committed planner",
            copy_in_the_committed_planner,
            SCAFFOLD_CAP,
        ),
    ];
    for (given, arrange, cap) in givens.into_iter().chain(CAP_ROWS) {
        let dir = scaffolded_project();
        let root = dir.path();
        arrange(root);
        assert_declares_the_cap(given, root, cap);
        let protocol = framed(&plan_protocol_text(
            WORKER,
            "(no enumerated acceptance criteria)",
            cap,
        ));
        let planner = agent_text(root, "planner.md");
        assert_eq!(
            planning_counts(&planner),
            ([1; 6], 1),
            "{given}: the planner states each planning-policy sentence and the cap's scope \
             sentence once:\n{planner}"
        );
        let system = composed_system_prompt(&planner);
        let reject = critique_reject("decomposition-conflict", &["adv-1"]);
        let (served, _) = served_over_a_reject_and_its_re_plan(root, &reject);
        for (spawn, (served_system, task)) in ["plan/implementer#0", "plan/replan#1"]
            .into_iter()
            .zip(served)
        {
            assert_eq!(
                served_system.trim_end(),
                system.trim_end(),
                "{given}: {spawn} is served the planner composed with the layers"
            );
            assert_eq!(
                (
                    task.get(task.len().saturating_sub(protocol.len())..),
                    planning_counts(&task)
                ),
                (Some(protocol.as_str()), ([0; 6], 0)),
                "{given}: {spawn} is served the planner composed, then a task closing with the \
                 plan protocol alone, its cap slot filled for the cap {cap}, and stating no \
                 planning policy and no scope sentence:\n{task}"
            );
        }
    }
}

/// Given a project `rigger init` scaffolded, its adversary and adjudicator personas hold their
/// share of the critique method under the critique section and none of the other role's.
#[test]
fn rigger_init_writes_each_critiquing_persona_its_method_under_the_critique_section() {
    let dir = scaffolded_project();
    for (file, holds, leaves) in critique_method_by_persona() {
        assert_holds_its_critique_method(
            &format!("the {file} rigger init wrote"),
            &agent_text(dir.path(), file),
            &holds,
            leaves,
        );
    }
}

/// Given a project with the personas `rigger init` wrote, or a consumer's with no critique
/// section, its workflow declaring the unit size cap the verb wrote, a cap of 1 line or no cap at
/// all, when the scaffolding verbs re-run, the personas and the declared cap stay, and when the
/// gate rejects and re-plans, `rigger run` serves each spawn that persona composed with the
/// layers and then the same task: the framing, stating the declared cap and naming a unit over it
/// among the causes a re-plan fixes, or neither when no cap is declared, and the output contract,
/// byte for byte.
#[test]
fn each_gate_spawn_is_served_its_own_persona_then_only_the_framing_and_the_output_contract() {
    let givens: [(&str, Arrange, u32); 2] = [
        ("the personas rigger init wrote", |_| {}, SCAFFOLD_CAP),
        (
            "a consumer's personas with no critique section",
            strike_the_critique_sections,
            SCAFFOLD_CAP,
        ),
    ];
    for (given, arrange, cap) in givens.into_iter().chain(CAP_ROWS) {
        let dir = scaffolded_project();
        let root = dir.path();
        arrange(root);
        let files = critique_method_by_persona().map(|(file, _, _)| file);
        let before = files.map(|file| agent_text(root, file));
        for (verb, envs) in [("init", &[][..]), ("setup", &[][..])] {
            run_rigger_envs_ok(root, &[verb], envs);
            assert_eq!(
                files.map(|file| agent_text(root, file)),
                before,
                "{given}: rigger {verb} never rewrites a persona"
            );
        }
        assert_declares_the_cap(given, root, cap);
        let reject = critique_reject("decomposition-conflict", &["adv-1"]);
        let (_, served) = served_over_a_reject_and_its_re_plan(root, &reject);
        let tasks = expected_tasks(root, &reject, cap);
        for (((file, task), served), spawn) in tasks.into_iter().zip(served).zip(GATE_SPAWNS) {
            let system = composed_system_prompt(&agent_text(root, file));
            assert_eq!(
                (served.0.trim_end(), served.1),
                (system.trim_end(), framed(&task)),
                "{given}: plan-critique/{spawn} is served {file} composed, then only its task"
            );
        }
    }
}
