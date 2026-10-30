//! Spec 120, THE CRITIQUE METHOD IS THE PERSONA'S, at the periphery: a project the real
//! `rigger init` scaffolds, driven through its own plan-critique gate by real `rigger step`
//! processes, each spawn answered the way a courier answers it (`rigger emit --spawn` for the
//! planner's proposal, `rigger result` for every output).
//!
//! The conductor's and the binary's own tests pin the DAG critique prompt over a stub driver and
//! the scaffold templates as text. What they cannot see is what this file pins: that `rigger init`
//! writes each critiquing persona its method under its critique section, that what `rigger prompt`
//! serves a spawn of the gate is, byte for byte, that persona composed with the instruction layers
//! and then only the task framing and the output contract - the prompt's "rules in your persona"
//! naming a persona the spawn really carries - and that a consumer's persona with no such section
//! is kept across a re-run of the scaffolding verbs and still critiques over the same task.
//! `rigger critique`'s task is held by `tests/spec_critique_periphery.rs`.
//!
//! The rows are the adjudicator of the first round, for the prompt no reject precedes, and the
//! adversary and the adjudicator of the re-plan round, for the prompt that opens with the reject.
//! The first round's adversary is no row: the step that parks the planner parks it too, so the unit
//! list it was served predates the proposal the other rows list. Every run is minted through
//! `run_store::start_fresh` and adopted by the step, as the workflow driver's run is. The stepped
//! workflow differs from what the verb wrote in one line, its pinned grounder.

mod common;

use common::cli::{
    loaded_config, propose_unit_via_rigger_emit, rigger_file, run_rigger_envs_ok, run_rigger_ok,
    temp_repoless_project, with_run_store,
};
use common::fixtures::{
    assert_holds_its_critique_method, composed_system_prompt, critique_method_by_persona,
    critique_reject, dag_critique_prompt, persona_section, read_text, DAG_DISJOINT_OVERLAP,
    DAG_PRIOR_REJECT_HEADER, WHEN_YOU_CRITIQUE_HEADING,
};
use rigger::conductor::review_protocol;
use rigger::spawn::ROLE_ADVERSARY;
use serde_json::json;
use std::path::Path;

/// The agent the scaffolded workflow's implement template fans out to.
const WORKER: &str = "rust-engineer";

/// The one unit the planner proposes.
const UNIT: &str = "u-widget";

/// The criterion [`UNIT`] serves.
const CRITERION: &str = "the widget renderer is implemented";

/// The blast radius a unit of a project with no grounded source lists.
const NONE_GROUNDED: &str = "(none grounded)";

/// What an adversary's result says: its findings go to the graph, never through this line.
const REVIEWED: &str = "reviewed the DAG";

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

/// Answer the parked planner spawn `spawn` as a courier does: its proposal, then its result.
fn plan(root: &Path, spawn: &str) {
    propose_unit_via_rigger_emit(
        root,
        spawn,
        &json!({"id": UNIT, "agent": WORKER, "criterion": CRITERION, "needs": []}),
    );
    run_rigger_ok(root, &["result", spawn, "proposed the DAG"]);
}

/// Answer the parked spawn `spawn` with `output`, returning what `rigger prompt` serves it, as a
/// courier fetches it.
fn serve_and_answer(root: &Path, spawn: &str, output: &str) -> String {
    let served = run_rigger_ok(root, &["prompt", spawn]);
    run_rigger_ok(root, &["result", spawn, output]);
    served
}

/// Pin `root`'s scaffolded workflow to the `nop` grounder: the scaffold selects `symbols`, which a
/// binary built without that feature refuses to step over, and `nop` lists the same empty blast
/// radius on both lanes.
fn pin_the_nop_grounder(root: &Path) {
    let path = rigger_file(root, "workflow.yml");
    let workflow = read_text(&path);
    let pinned = workflow.replacen("  grounder: symbols", "  grounder: nop", 1);
    assert_ne!(
        pinned, workflow,
        "premise: the scaffolded workflow selects the symbols grounder"
    );
    std::fs::write(path, pinned).unwrap();
}

/// Drive `root`'s gate through the first-round reject line `reject`, the re-plan it feeds and the
/// approve that ends it, returning what `rigger prompt` served the first round's adjudicator and
/// the second round's adversary and adjudicator.
fn served_over_a_reject_and_its_re_plan(root: &Path, reject: &str) -> [String; 3] {
    pin_the_nop_grounder(root);
    with_run_store(root, |store| {
        rigger::run_store::start_fresh(store, &[], "", "", "", "specs/widget.md").unwrap();
    });
    run_rigger_ok(root, &["step"]);
    plan(root, "plan/implementer#0");
    run_rigger_ok(root, &["result", "plan-critique/adversary#0", REVIEWED]);
    run_rigger_ok(root, &["step"]);
    let first_adjudicator = serve_and_answer(root, "plan-critique/adjudicator#0", reject);
    run_rigger_ok(root, &["step"]);
    plan(root, "plan/replan#1");
    run_rigger_ok(root, &["step"]);
    let second_adversary = serve_and_answer(root, "plan-critique/adversary#1", REVIEWED);
    run_rigger_ok(root, &["step"]);
    let second_adjudicator = serve_and_answer(
        root,
        "plan-critique/adjudicator#1",
        r#"{"verdict":"approve"}"#,
    );
    [first_adjudicator, second_adversary, second_adjudicator]
}

/// The task the first round's adjudicator, the second round's adversary and its adjudicator must
/// be served over `root`'s scaffolded workflow after the reject line `reject`, each with the agent
/// file its persona comes from: the framing, the cap, the unit list and the verdict paragraph, the
/// re-plan round led by the reject and the adversary's closed by its findings protocol, and
/// nothing else.
fn expected_tasks(root: &Path, reject: &str) -> [(&'static str, String); 3] {
    let config = loaded_config(root);
    let checkin = config.workflow.stages["checkin"].coverage.as_str();
    let first_round = dag_critique_prompt(
        &[
            ["checkin", checkin, "implement", NONE_GROUNDED],
            [UNIT, CRITERION, "plan-critique", NONE_GROUNDED],
        ],
        DAG_DISJOINT_OVERLAP,
    );
    let second_round = format!(
        "{DAG_PRIOR_REJECT_HEADER}{reject}\n\nThe planner has since revised the DAG; re-review it \
         below.\n\n{first_round}"
    );
    [
        ("adjudicator.md", first_round),
        (
            "adversary.md",
            format!("{second_round}{}", review_protocol(ROLE_ADVERSARY)),
        ),
        ("adjudicator.md", second_round),
    ]
}

/// What a given does to a freshly scaffolded project before the verbs re-run.
type Arrange = fn(&Path);

/// Strike the critique section out of each critiquing persona of `root`, leaving the text a
/// consumer scaffolded before the method moved.
fn strike_the_critique_sections(root: &Path) {
    for (file, _, _) in critique_method_by_persona() {
        let text = agent_text(root, file);
        let section = persona_section(&text, WHEN_YOU_CRITIQUE_HEADING);
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
/// section, when the scaffolding verbs re-run, the personas stay byte for byte, and when the
/// gate rejects and re-plans, `rigger prompt` serves each spawn that persona composed with the
/// layers and then the same task: the framing and the output contract, byte for byte.
#[test]
fn each_gate_spawn_is_served_its_own_persona_then_only_the_framing_and_the_output_contract() {
    let givens: [(&str, Arrange); 2] = [
        ("the personas rigger init wrote", |_| {}),
        (
            "a consumer's personas with no critique section",
            strike_the_critique_sections,
        ),
    ];
    for (given, arrange) in givens {
        let dir = scaffolded_project();
        let root = dir.path();
        arrange(root);
        let files = critique_method_by_persona().map(|(file, _, _)| file);
        let before = files.map(|file| agent_text(root, file));
        for (verb, envs) in [("init", &[][..]), ("setup", &[("RIGGER_NPM", "true")][..])] {
            run_rigger_envs_ok(root, &[verb], envs);
            assert_eq!(
                files.map(|file| agent_text(root, file)),
                before,
                "{given}: rigger {verb} never rewrites a persona"
            );
        }
        let reject = critique_reject("decomposition-conflict", &["adv-1"]);
        let served = served_over_a_reject_and_its_re_plan(root, &reject);
        let tasks = expected_tasks(root, &reject);
        let spawns = ["adjudicator#0", "adversary#1", "adjudicator#1"];
        for (((file, task), served), spawn) in tasks.into_iter().zip(served).zip(spawns) {
            let system = composed_system_prompt(&agent_text(root, file));
            assert_eq!(
                served,
                format!("{system}\n\n---\n\n{task}\n"),
                "{given}: plan-critique/{spawn} is served {file} composed, then only its task"
            );
        }
    }
}
