//! Spec 112, criterion 1 (A SPEC IS CRITIQUED BEFORE THE RUN) - periphery: `rigger critique
//! <spec>` driven as the compiled binary, with the checked-in critique stub first on `PATH` as
//! the critic's `claude`. Given a project whose workflow names a critic, when the operator
//! critiques a spec, then the plan-critique adversary runs once on the spec text through the
//! headless host, its findings and verdict are recorded keyed on the spec's content hash and
//! copied into the graph, and a second call on unchanged text answers from the store with zero
//! spawns; an edited text is critiqued afresh and inherits none of the earlier findings.
//!
//! NOT OWNED HERE: the refusal a new run meets without a clean critique (criterion 2), and the
//! plan-critique stop on a spec defect (criterion 5).

mod common;

use std::path::Path;

use common::cli::{
    read_run_events, rigger_file, run_rigger, run_rigger_envs, run_stream_identity, temp_project,
    temp_repoless_project, write_scaffold,
};
use common::repo::{
    critique_stub_argv, critique_stub_spawns, critique_stub_task, write_critique_stub,
};
use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, Event, EventStore, Filter};
use rigger::review::{critique_hash, critique_spawn_id, PLAN_CRITIQUE_RULES};
use rigger::spawn::{TYPE_SPAWN_REQUESTED, TYPE_SPAWN_RESULT};
use rigger::wave::NO_CRITIC_CLAUSE;
use serde_json::{json, Value};

/// A workflow whose plan-critique gate names the `critic` persona as its adversary.
const CRITIC_WORKFLOW: &str = "defaults:\n  grounder: nop\nstages:\n  plan:\n    agent: planner\n    \
     produces: dag\n  plan-critique:\n    needs: [plan]\n    adversary: critic\n    adjudicator: judge\n";

/// A workflow naming neither the plan-critique gate's adversary nor `defaults.review.adversary`.
const NO_CRITIC_WORKFLOW: &str =
    "defaults:\n  grounder: nop\nstages:\n  a:\n    agent: planner\n    on_pass: none\n";

const PLANNER: &str =
    "---\nid: planner\nmodel: sonnet\ntools: [Read]\nisolation: none\n---\nDecompose the spec.\n";
const CRITIC: &str = "---\nid: critic\nmodel_ladder: [sonnet, opus]\ntools: [Read, Bash, Grep, \
     Agent]\nisolation: none\n---\nYou are the CRITIC-PERSONA adversary.\n";
const JUDGE: &str = "---\nid: judge\nmodel: sonnet\ntools: [Read]\nisolation: none\n---\nJudge. \
     End with {\"verdict\":\"approve\"}.\n";

const SPEC_REL: &str = "specs/9-demo.md";
const SPEC: &str = "# 9 - A demo spec\n\n## Design\n\nThe widget renders.\n\n## Done when\n\n\
     - [ ] a test proves the widget renders\n- [ ] a test proves the widget hides\n";

/// The critic's canned answer: prose, one BLOCKING and one NON-BLOCKING finding line, a reject.
const REJECT: &str = "I read the spec.\n\
     C1 | BLOCKING | criterion 2 | hides is undecided when empty | decide the empty corner in Design\n\
     C2 | NON-BLOCKING | Design | renders names no surface | name the surface\n\
     {\"verdict\":\"reject\"}";
const APPROVE: &str = "No defects.\n{\"verdict\":\"approve\"}";

/// A project at `root` carrying `workflow` with its three personas and the demo spec.
fn scaffold(root: &Path, workflow: &str) {
    write_scaffold(
        root,
        &[("planner", PLANNER), ("critic", CRITIC), ("judge", JUDGE)],
        workflow,
    );
    std::fs::create_dir_all(root.join("specs")).unwrap();
    std::fs::write(root.join(SPEC_REL), SPEC).unwrap();
}

/// `rigger critique <spec>` in `root` with `path` as its PATH and `scratch` as its scratch root.
fn critique(root: &Path, spec: &str, path: &str, scratch: &Path) -> (String, String, bool) {
    run_rigger_envs(
        root,
        &["critique", spec],
        &[("PATH", path), ("RIGGER_TMPDIR", scratch.to_str().unwrap())],
    )
}

/// Every event of the critique's own stream: the project's namespace plus `-critique`.
fn critique_events(root: &Path) -> Vec<Event> {
    let backend = Store::open(rigger_file(root, "events.db").to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &format!("{}-critique", run_stream_identity(root)));
    store
        .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
        .unwrap()
}

/// The `(type, spawn id)` of each critique-stream event.
fn spawn_events(events: &[Event]) -> Vec<(String, String)> {
    events
        .iter()
        .map(|e| {
            let data: Value = serde_json::from_slice(&e.data).unwrap();
            (e.type_.clone(), data["id"].as_str().unwrap().to_string())
        })
        .collect()
}

/// The payloads of the project run stream's `ReviewFinding` events, oldest first.
fn review_findings(root: &Path) -> Vec<Value> {
    read_run_events(root)
        .iter()
        .filter(|e| e.type_ == "ReviewFinding")
        .map(|e| serde_json::from_slice(&e.data).unwrap())
        .collect()
}

/// Every event in the project's `events.db`, every stream and namespace included.
fn every_event(root: &Path) -> usize {
    let backend = Store::open(rigger_file(root, "events.db").to_str().unwrap()).unwrap();
    backend
        .read_all(0, Direction::Forward, &Filter::default())
        .unwrap()
        .len()
}

/// The value the argv carries right after `flag`.
fn flag_value<'a>(argv: &'a [String], flag: &str) -> &'a str {
    let at = argv
        .iter()
        .position(|a| a == flag)
        .unwrap_or_else(|| panic!("the critic's argv carries {flag}: {argv:?}"));
    &argv[at + 1]
}

#[test]
fn a_spec_is_critiqued_once_per_text_and_answered_from_the_store_after() {
    let dir = temp_project();
    let root = dir.path();
    scaffold(root, CRITIC_WORKFLOW);
    let scratch = root.join("scratch");
    let first = tempfile::tempdir().unwrap();
    let first_path = write_critique_stub(first.path(), REJECT);
    let hash = critique_hash(SPEC);
    let expected_out = format!(
        "sc-{hash}-0-1 | BLOCKING | criterion 2 | hides is undecided when empty | decide the empty \
         corner in Design\nsc-{hash}-0-2 | NON-BLOCKING | Design | renders names no surface | \
         name the surface\n{{\"verdict\":\"reject\"}}\n"
    );

    // When the spec is critiqued for the first time ...
    let (out, err, ok) = critique(root, SPEC_REL, &first_path, &scratch);
    assert!(
        ok,
        "a recorded critique exits 0 whatever its findings; stderr:\n{err}"
    );
    assert_eq!(
        out, expected_out,
        "the findings and the verdict, read back from the store"
    );

    // ... the plan-critique adversary ran once, on the spec text, through the headless host.
    assert_eq!(critique_stub_spawns(first.path()), 1, "one critic spawn");
    let task = critique_stub_task(first.path());
    assert!(
        task.ends_with(SPEC),
        "the critique prompt ends with the spec text verbatim:\n{task}"
    );
    assert!(
        task.contains(PLAN_CRITIQUE_RULES),
        "the prompt carries the shared rules"
    );
    assert!(
        task.contains("`specs/9-demo.md`"),
        "the prompt names the spec path"
    );
    let argv = critique_stub_argv(first.path());
    assert_eq!(
        flag_value(&argv, "--allowed-tools"),
        "Read,Glob,mcp__rigger__rigger_graph,mcp__rigger__rigger_ground,mcp__rigger__rigger_peers",
        "the critic's tools are replaced: no Bash, Agent, Grep or emit"
    );
    assert_eq!(
        flag_value(&argv, "--model"),
        "sonnet",
        "the persona's rung for attempt 0"
    );
    let system = flag_value(&argv, "--system-prompt");
    assert!(
        system.starts_with("You are the CRITIC-PERSONA adversary."),
        "the critic's system prompt opens with the persona the workflow names:\n{system}"
    );
    assert!(
        system.contains("# Rigger communication discipline"),
        "the system prompt carries the discipline every spawn gets:\n{system}"
    );

    // ... the request and the result are on the critique's own stream, keyed on the hash.
    let spawn = critique_spawn_id(&hash, 0);
    let recorded = vec![
        (TYPE_SPAWN_REQUESTED.to_string(), spawn.clone()),
        (TYPE_SPAWN_RESULT.to_string(), spawn.clone()),
    ];
    assert_eq!(spawn_events(&critique_events(root)), recorded);

    // ... and each finding is copied to the project run stream, where `rigger peers` shows it.
    let copies = vec![
        json!({
            "id": format!("sc-{hash}-0-1"),
            "by": "spec-critic",
            "summary": "BLOCKING | criterion 2 | hides is undecided when empty | decide the empty corner in Design",
            "about": [SPEC_REL],
        }),
        json!({
            "id": format!("sc-{hash}-0-2"),
            "by": "spec-critic",
            "summary": "NON-BLOCKING | Design | renders names no surface | name the surface",
            "about": [SPEC_REL],
        }),
    ];
    assert_eq!(review_findings(root), copies);
    let (peers, err, ok) = run_rigger(root, &["peers", SPEC_REL]);
    assert!(ok, "rigger peers succeeds; stderr:\n{err}");
    assert!(
        peers.contains(&format!("sc-{hash}-0-1")) && peers.contains(&format!("sc-{hash}-0-2")),
        "the graph shows the copied findings about the spec:\n{peers}"
    );
    for sub in ["agent-live", "agent-stream"] {
        assert!(
            !scratch.join(sub).join(format!("critique-{hash}")).exists(),
            "the critique run's {sub} directory goes once its spawn returns"
        );
    }

    // When the unchanged text is critiqued again, with a leftover critique scratch dir planted
    // beside a loop run's ...
    let leftover = scratch.join("agent-live").join("critique-fedcba9876543210");
    let loop_run = scratch.join("agent-live").join("run-1");
    for d in [&leftover, &loop_run] {
        std::fs::create_dir_all(d).unwrap();
        std::fs::write(d.join("marker"), "").unwrap();
    }
    let (again, err, ok) = critique(root, SPEC_REL, &first_path, &scratch);
    assert!(ok, "an answered critique exits 0; stderr:\n{err}");
    // ... it answers from the store with zero spawns and appends nothing.
    assert_eq!(
        again, expected_out,
        "the same recorded findings and verdict"
    );
    assert_eq!(
        critique_stub_spawns(first.path()),
        1,
        "zero spawns on unchanged text"
    );
    assert_eq!(
        spawn_events(&critique_events(root)),
        recorded,
        "no second request"
    );
    assert_eq!(review_findings(root), copies, "a copy is skipped by its id");
    assert!(
        !leftover.exists(),
        "every critique run's scratch goes with the next call"
    );
    assert!(
        loop_run.exists(),
        "a loop run's scratch is never a critique's to remove"
    );

    // When one line of the spec is deleted, the new text is critiqued afresh ...
    let edited = SPEC.replace("- [ ] a test proves the widget hides\n", "");
    std::fs::write(root.join(SPEC_REL), &edited).unwrap();
    let second = tempfile::tempdir().unwrap();
    let second_path = write_critique_stub(second.path(), APPROVE);
    let (fresh, err, ok) = critique(root, SPEC_REL, &second_path, &scratch);
    assert!(ok, "the edited text's critique exits 0; stderr:\n{err}");
    assert_eq!(
        critique_stub_spawns(second.path()),
        1,
        "the edited text spawns the critic"
    );
    assert_eq!(
        critique_stub_spawns(first.path()),
        1,
        "the first stub is not spawned again"
    );
    // ... and inherits none of the earlier findings.
    assert_eq!(fresh, "{\"verdict\":\"approve\"}\n");
    assert!(!fresh.contains(&hash), "no earlier finding is printed");
    assert!(
        critique_stub_task(second.path()).ends_with(&edited),
        "the critic read the edited text"
    );
    let edited_spawn = critique_spawn_id(&critique_hash(&edited), 0);
    assert_eq!(
        spawn_events(&critique_events(root))[2..],
        [
            (TYPE_SPAWN_REQUESTED.to_string(), edited_spawn.clone()),
            (TYPE_SPAWN_RESULT.to_string(), edited_spawn),
        ]
    );
    assert_eq!(
        review_findings(root),
        copies,
        "a clean critique copies nothing"
    );
}

#[test]
fn under_a_workflow_with_no_critic_the_verb_refuses_naming_both_keys_and_records_nothing() {
    let dir = temp_project();
    let root = dir.path();
    scaffold(root, CRITIC_WORKFLOW);
    let scratch = root.join("scratch");
    let stub = tempfile::tempdir().unwrap();
    let path = write_critique_stub(stub.path(), REJECT);
    let (_out, err, ok) = critique(root, SPEC_REL, &path, &scratch);
    assert!(ok, "the critique under a critic records; stderr:\n{err}");
    let before = every_event(root);

    // Given the workflow drops its critic, when the same, already-critiqued text is critiqued ...
    std::fs::write(root.join(".rigger/workflow.yml"), NO_CRITIC_WORKFLOW).unwrap();
    let (out, err, ok) = critique(root, SPEC_REL, &path, &scratch);

    // ... the verb refuses, naming both keys, answering nothing and recording nothing.
    assert!(!ok, "a workflow with no critic refuses the verb");
    assert!(
        err.contains(NO_CRITIC_CLAUSE),
        "the refusal names both keys:\n{err}"
    );
    assert_eq!(out, "", "a refusal prints no finding");
    assert_eq!(critique_stub_spawns(stub.path()), 1, "no spawn");
    assert_eq!(every_event(root), before, "nothing is recorded");
}

#[test]
fn a_spec_outside_the_repository_or_with_no_criteria_is_refused_before_any_store() {
    let dir = temp_project();
    let root = dir.path();
    scaffold(root, CRITIC_WORKFLOW);
    let scratch = root.join("scratch");
    let stub = tempfile::tempdir().unwrap();
    let path = write_critique_stub(stub.path(), REJECT);

    let elsewhere = tempfile::tempdir().unwrap();
    let outside = elsewhere.path().join("9-demo.md");
    std::fs::write(&outside, SPEC).unwrap();
    let (_out, err, ok) = critique(root, outside.to_str().unwrap(), &path, &scratch);
    assert!(!ok, "a spec outside the repository is refused");
    assert!(
        err.contains("outside the repository"),
        "the refusal says why:\n{err}"
    );

    std::fs::write(
        root.join("specs/no-criteria.md"),
        "# A spec\n\nNo checkboxes.\n",
    )
    .unwrap();
    let (_out, err, ok) = critique(root, "specs/no-criteria.md", &path, &scratch);
    assert!(!ok, "a spec with no Done-when criteria is refused");
    assert!(err.contains("loop-ready"), "the loop-ready refusal:\n{err}");

    assert_eq!(critique_stub_spawns(stub.path()), 0, "no spawn");
    assert!(
        !rigger_file(root, "events.db").exists(),
        "neither refusal opened or created the store"
    );
}

#[test]
fn a_project_with_no_git_repository_critiques_its_spec_against_the_project_root() {
    let dir = temp_repoless_project();
    let root = dir.path();
    scaffold(root, CRITIC_WORKFLOW);
    let stub = tempfile::tempdir().unwrap();
    let path = write_critique_stub(stub.path(), REJECT);
    let absolute = root.join(SPEC_REL);
    let (out, err, ok) = critique(
        root,
        absolute.to_str().unwrap(),
        &path,
        &root.join("scratch"),
    );
    assert!(ok, "a repo-less project critiques; stderr:\n{err}");
    let hash = critique_hash(SPEC);
    assert!(
        out.contains(&format!("sc-{hash}-0-1 | BLOCKING")),
        "the finding is printed:\n{out}"
    );
    let about: Vec<Value> = review_findings(root)
        .iter()
        .map(|f| f["about"].clone())
        .collect();
    assert_eq!(
        about,
        [json!([SPEC_REL]), json!([SPEC_REL])],
        "the absolute path is made relative to the project root"
    );
    assert_eq!(critique_stub_spawns(stub.path()), 1);
}

#[test]
fn a_result_that_is_no_critique_exits_non_zero_saying_why_and_the_next_call_spawns_attempt_one() {
    let dir = temp_project();
    let root = dir.path();
    scaffold(root, CRITIC_WORKFLOW);
    let scratch = root.join("scratch");
    let hash = critique_hash(SPEC);

    // When the critic answers with no verdict line ...
    let silent = tempfile::tempdir().unwrap();
    let silent_path = write_critique_stub(silent.path(), "I could not decide.");
    let (out, err, ok) = critique(root, SPEC_REL, &silent_path, &scratch);

    // ... its result is no critique: the verb says why, exits non-zero and copies nothing.
    assert!(!ok, "a result that is no critique exits non-zero");
    assert!(
        err.contains(&format!(
            "rigger critique: specs/9-demo.md (hash {hash}): no critique was recorded - the \
             critic's output carries no verdict line"
        )),
        "the verb prints why:\n{err}"
    );
    assert_eq!(out, "", "no finding or verdict is printed");
    assert_eq!(review_findings(root), Vec::<Value>::new());
    let first = critique_spawn_id(&hash, 0);
    assert_eq!(
        spawn_events(&critique_events(root)),
        [
            (TYPE_SPAWN_REQUESTED.to_string(), first.clone()),
            (TYPE_SPAWN_RESULT.to_string(), first),
        ]
    );

    // When the unchanged text is critiqued again, the recorded request counts: attempt 1 runs.
    let answering = tempfile::tempdir().unwrap();
    let answering_path = write_critique_stub(answering.path(), REJECT);
    let (out, err, ok) = critique(root, SPEC_REL, &answering_path, &scratch);
    assert!(ok, "the next attempt records a critique; stderr:\n{err}");
    assert_eq!(critique_stub_spawns(silent.path()), 1);
    assert_eq!(critique_stub_spawns(answering.path()), 1);
    assert!(
        out.starts_with(&format!("sc-{hash}-1-1 | BLOCKING | criterion 2 | ")),
        "the finding ids carry attempt 1:\n{out}"
    );
    let second = critique_spawn_id(&hash, 1);
    assert_eq!(
        spawn_events(&critique_events(root))[2..],
        [
            (TYPE_SPAWN_REQUESTED.to_string(), second.clone()),
            (TYPE_SPAWN_RESULT.to_string(), second),
        ]
    );
    assert_eq!(
        flag_value(&critique_stub_argv(answering.path()), "--model"),
        "opus",
        "attempt 1 runs the persona's rung for attempt 1"
    );
}

#[test]
fn the_verb_takes_the_store_flags_rigger_run_takes_and_refuses_malformed_arguments() {
    let dir = temp_project();
    let root = dir.path();
    scaffold(root, CRITIC_WORKFLOW);
    let stub = tempfile::tempdir().unwrap();
    let path = write_critique_stub(stub.path(), REJECT);
    let scratch = root.join("scratch");
    let envs = [
        ("PATH", path.as_str()),
        ("RIGGER_TMPDIR", scratch.to_str().unwrap()),
    ];
    for (args, refusal) in [
        (
            vec!["critique"],
            "critique: expected a spec path: rigger critique <spec>",
        ),
        (
            vec!["critique", SPEC_REL, "specs/other.md"],
            "critique: unexpected second positional argument \"specs/other.md\"",
        ),
        (
            vec!["critique", "--frob", SPEC_REL],
            "critique: unknown flag \"--frob\"",
        ),
        (
            vec!["critique", SPEC_REL, "--eventstore", "postgres"],
            "critique: --eventstore expects sqlite|kurrentdb, got Some(\"postgres\")",
        ),
        (
            vec!["critique", SPEC_REL, "--conn"],
            "critique: --conn expects a connection url",
        ),
    ] {
        let (_out, err, ok) = run_rigger_envs(root, &args, &envs);
        assert!(!ok, "{args:?} is refused");
        assert!(err.contains(refusal), "{args:?} says {refusal:?}:\n{err}");
    }
    assert_eq!(
        critique_stub_spawns(stub.path()),
        0,
        "a malformed call spawns nothing"
    );
    assert!(
        !rigger_file(root, "events.db").exists(),
        "a malformed call opens no store"
    );

    let (out, err, ok) = run_rigger_envs(
        root,
        &["critique", "--eventstore", "sqlite", SPEC_REL],
        &envs,
    );
    assert!(
        ok,
        "--eventstore sqlite selects the local store; stderr:\n{err}"
    );
    assert!(
        out.starts_with(&format!("sc-{}-0-1 | BLOCKING | ", critique_hash(SPEC))),
        "the critique is recorded and printed:\n{out}"
    );
    assert_eq!(critique_stub_spawns(stub.path()), 1);
}
