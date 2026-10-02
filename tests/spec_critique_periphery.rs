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
    assert_selected_server, emit, read_run_events, rigger_file, run_rigger, run_rigger_envs,
    run_stream_identity, seed_store, temp_project, temp_repoless_project, write_scaffold,
};
use common::fixtures::{git_ok, temp_git_project_with_commit};
use common::repo::{
    critique_stub_argv, critique_stub_conn, critique_stub_spawns, critique_stub_task,
    write_critique_stub, write_critique_stub_reporting,
};
use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, Event, EventStore, ExpectedRevision, Filter};
use rigger::review::{critique_hash, critique_spawn_id, spec_critique_prompt, PLAN_CRITIQUE_RULES};
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

/// What `rigger critique` prints for a [`REJECT`] critique of `hash` recorded at `attempt`: each
/// finding as its record id and summary, then the verdict.
fn reject_out(hash: &str, attempt: u32) -> String {
    format!(
        "sc-{hash}-{attempt}-1 | BLOCKING | criterion 2 | hides is undecided when empty | decide \
         the empty corner in Design\nsc-{hash}-{attempt}-2 | NON-BLOCKING | Design | renders \
         names no surface | name the surface\n{{\"verdict\":\"reject\"}}\n"
    )
}

/// The `ReviewFinding` copies of a [`REJECT`] critique of `hash` at `attempt`, about `spec`.
fn reject_copies(hash: &str, attempt: u32, spec: &str) -> Vec<Value> {
    vec![
        json!({
            "id": format!("sc-{hash}-{attempt}-1"),
            "by": "spec-critic",
            "summary": "BLOCKING | criterion 2 | hides is undecided when empty | decide the empty corner in Design",
            "about": [spec],
        }),
        json!({
            "id": format!("sc-{hash}-{attempt}-2"),
            "by": "spec-critic",
            "summary": "NON-BLOCKING | Design | renders names no surface | name the surface",
            "about": [spec],
        }),
    ]
}

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

/// A fresh critique stub answering `critique`: its work directory, which keeps it alive, and the
/// PATH that runs it.
fn stub(critique: &str) -> (tempfile::TempDir, String) {
    let work = tempfile::tempdir().unwrap();
    let path = write_critique_stub(work.path(), critique);
    (work, path)
}

#[test]
fn a_spec_is_critiqued_once_per_text_and_answered_from_the_store_after() {
    let dir = temp_project();
    let root = dir.path();
    scaffold(root, CRITIC_WORKFLOW);
    let scratch = root.join("scratch");
    let (first, first_path) = stub(REJECT);
    let hash = critique_hash(SPEC);
    let expected_out = reject_out(&hash, 0);

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
    let settings: Value = serde_json::from_str(flag_value(&argv, "--settings")).unwrap();
    assert_eq!(
        settings["permissions"]["deny"],
        json!([
            "Bash",
            "Agent",
            "Task",
            "Grep",
            "Edit",
            "MultiEdit",
            "Write",
            "NotebookEdit",
            "mcp__rigger__rigger_emit",
            "mcp__rigger__rigger_progress",
            "mcp__rigger__rigger_scratch"
        ]),
        "the critic is denied every tool that builds, records or edits, whatever the checkout's \
         own settings allow: {settings}"
    );
    assert_eq!(
        flag_value(&argv, "--model"),
        "sonnet",
        "the persona's rung for attempt 0"
    );
    assert_eq!(
        critique_stub_conn(first.path()),
        "unset",
        "a sqlite selection hands the critic no server: its bound rigger server resolves the \
         local store through configuration"
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
    let copies = reject_copies(&hash, 0, SPEC_REL);
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
    let (second, second_path) = stub(APPROVE);
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
    let (stub, path) = stub(REJECT);
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
    let (stub, path) = stub(REJECT);

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
    let (stub, path) = stub(REJECT);
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
    assert!(
        !root.join("scratch").exists(),
        "a project with no git repository has no scratch root: the session writes no liveness \
         marker or transcript, even with RIGGER_TMPDIR set"
    );
}

#[test]
fn a_result_that_is_no_critique_exits_non_zero_saying_why_and_the_next_call_spawns_attempt_one() {
    let dir = temp_project();
    let root = dir.path();
    scaffold(root, CRITIC_WORKFLOW);
    let scratch = root.join("scratch");
    let hash = critique_hash(SPEC);

    // When the critic answers with no verdict line ...
    let (silent, silent_path) = stub("I could not decide.");
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
    let (answering, answering_path) = stub(REJECT);
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
    let (stub, path) = stub(REJECT);
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

// ---------------------------------------------------------------------------------------------
// The SDET periphery layer: the verb's boundary corners - the persisted record's own form, the
// reverted, crash-resume and existing-data corners the store decides, the authority at the verb's
// edge, the critic lookup's fallback, and every refusal in the order the Design fixes.
// ---------------------------------------------------------------------------------------------

/// [`SPEC`]'s content hash, spelled out: FNV-1a 64 over its raw bytes as 16 lowercase hex digits,
/// the key every critique record of it is persisted under.
const SPEC_HASH: &str = "ff026c95e7c7af01";

/// [`CRITIC_WORKFLOW`] with a per-spawn wall-clock bound every persona inherits.
const BOUNDED_CRITIC_WORKFLOW: &str =
    "defaults:\n  grounder: nop\n  max_wall_clock: 900\nstages:\n  \
     plan:\n    agent: planner\n    produces: dag\n  plan-critique:\n    needs: [plan]\n    \
     adversary: critic\n    adjudicator: judge\n";

/// A workflow with no plan-critique gate whose `defaults.review.adversary` is the critic.
const DEFAULT_ADVERSARY_WORKFLOW: &str = "defaults:\n  grounder: nop\n  review:\n    adversary: \
     critic\nstages:\n  a:\n    agent: planner\n    on_pass: none\n";

/// A workflow whose plan-critique gate names `judge` while `defaults.review.adversary` names
/// `critic`: the gate's adversary is the critic.
const GATE_AND_DEFAULT_WORKFLOW: &str = "defaults:\n  grounder: nop\n  review:\n    adversary: \
     critic\nstages:\n  plan:\n    agent: planner\n    produces: dag\n  plan-critique:\n    \
     needs: [plan]\n    adversary: judge\n    adjudicator: judge\n";

/// The `(type, payload)` of each critique-stream event.
fn critique_payloads(root: &Path) -> Vec<(String, Value)> {
    critique_events(root)
        .iter()
        .map(|e| (e.type_.clone(), serde_json::from_slice(&e.data).unwrap()))
        .collect()
}

/// Append `(type, payload)` events to the critique's own stream of the project at `root`, in the
/// wire form the verb records them in - standing in for a critique an earlier call recorded.
fn seed_critique(root: &Path, events: &[(&str, Value)]) {
    std::fs::create_dir_all(root.join(".rigger")).unwrap();
    let backend = Store::open(rigger_file(root, "events.db").to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &format!("{}-critique", run_stream_identity(root)));
    for (type_, payload) in events {
        store
            .append(
                rigger::conductor::STREAM,
                ExpectedRevision::Any,
                &[Event::new(*type_, serde_json::to_vec(payload).unwrap())],
            )
            .unwrap();
    }
}

/// The types of the project run stream's events, oldest first.
fn run_types(root: &Path) -> Vec<String> {
    read_run_events(root)
        .iter()
        .map(|e| e.type_.clone())
        .collect()
}

#[test]
fn the_critique_record_persists_under_the_literal_content_hash_with_the_critics_request() {
    let dir = temp_project();
    let root = dir.path();
    scaffold(root, BOUNDED_CRITIC_WORKFLOW);
    let (work, path) = stub(REJECT);
    let scratch = root.join("scratch");

    // When the spec is named with a leading `./` ...
    let (out, err, ok) = critique(root, "./specs/9-demo.md", &path, &scratch);
    assert!(ok, "the critique records; stderr:\n{err}");

    // ... the findings carry the spec text's literal hash, and the spelling is made repo-relative.
    assert_eq!(out, reject_out(SPEC_HASH, 0));
    assert_eq!(review_findings(root), reject_copies(SPEC_HASH, 0, SPEC_REL));
    let prompt = spec_critique_prompt(SPEC_REL, SPEC);
    assert_eq!(
        critique_stub_task(work.path()),
        prompt,
        "the critic's task is the critique prompt of the repo-relative path and the raw text, \
         byte for byte"
    );

    // The request and the result are persisted on the critique's own stream in their wire form.
    let repo = std::fs::canonicalize(root).unwrap();
    let recorded = critique_payloads(root);
    assert_eq!(
        recorded
            .iter()
            .map(|(t, d)| (t.as_str(), d["id"].as_str().unwrap()))
            .collect::<Vec<_>>(),
        [
            (
                TYPE_SPAWN_REQUESTED,
                "critique-ff026c95e7c7af01/adversary#0"
            ),
            (TYPE_SPAWN_RESULT, "critique-ff026c95e7c7af01/adversary#0"),
        ]
    );
    assert_eq!(
        critique_events(root)[0]
            .meta
            .get(rigger::run::META_RUN_ID)
            .map(String::as_str),
        Some("critique-ff026c95e7c7af01"),
        "the request is parked in the critique run"
    );
    let request = &recorded[0].1;
    assert_eq!(
        (
            &request["unit"],
            &request["stage"],
            &request["title"],
            &request["dir"],
            &request["model"],
            &request["tools"],
            &request["max_wall_clock"],
            &request["prompt"],
        ),
        (
            &json!("critique-ff026c95e7c7af01"),
            &json!("critique"),
            &json!(SPEC_REL),
            &json!(repo.to_str().unwrap()),
            &json!("sonnet"),
            &json!([
                "Read",
                "Glob",
                "mcp__rigger__rigger_graph",
                "mcp__rigger__rigger_ground",
                "mcp__rigger__rigger_peers"
            ]),
            &json!(900),
            &json!(prompt),
        ),
        "the critic runs in the repository root on the persona's attempt-0 rung and wall-clock \
         bound with its tools replaced: {request}"
    );
    assert!(
        request["system_prompt"]
            .as_str()
            .unwrap()
            .starts_with("You are the CRITIC-PERSONA adversary."),
        "the recorded system prompt opens with the critic persona: {request}"
    );
    let result = &recorded[1].1;
    assert_eq!(
        (&result["output"], result.get("error")),
        (&json!(REJECT), None),
        "the result keeps the critic's whole output and no error: {result}"
    );

    // The session's liveness marker and transcript lived under the critique run and went with it.
    for sub in ["agent-live", "agent-stream"] {
        let left: Vec<std::ffi::OsString> = std::fs::read_dir(scratch.join(sub))
            .unwrap_or_else(|e| panic!("the session wrote under {sub}: {e}"))
            .flatten()
            .map(|entry| entry.file_name())
            .collect();
        assert_eq!(
            left,
            Vec::<std::ffi::OsString>::new(),
            "nothing the critique's session wrote is left under {sub}"
        );
    }

    // The project run stream holds the two copies and nothing of the spawn.
    assert_eq!(run_types(root), ["ReviewFinding", "ReviewFinding"]);
    assert_eq!(
        every_event(root),
        4,
        "two spawn events on the critique stream and two copies on the run stream, nothing else"
    );
}

#[test]
fn a_text_reverted_to_a_critiqued_hash_is_answered_by_that_critique_with_zero_spawns() {
    let dir = temp_project();
    let root = dir.path();
    scaffold(root, CRITIC_WORKFLOW);
    let scratch = root.join("scratch");
    let (_first, first_path) = stub(REJECT);
    let (_out, err, ok) = critique(root, SPEC_REL, &first_path, &scratch);
    assert!(ok, "the first text is critiqued; stderr:\n{err}");

    // Given the text was edited and the edit critiqued too ...
    let edited = SPEC.replace("The widget renders.", "The widget renders twice.");
    std::fs::write(root.join(SPEC_REL), &edited).unwrap();
    let (_second, second_path) = stub(APPROVE);
    let (out, err, ok) = critique(root, SPEC_REL, &second_path, &scratch);
    assert!(ok, "the edited text is critiqued; stderr:\n{err}");
    assert_eq!(out, "{\"verdict\":\"approve\"}\n");

    // ... when the text is reverted to the first bytes ...
    std::fs::write(root.join(SPEC_REL), SPEC).unwrap();
    let (third, third_path) = stub(APPROVE);
    let (out, err, ok) = critique(root, SPEC_REL, &third_path, &scratch);

    // ... the first hash's critique answers it, with zero spawns and nothing appended.
    assert!(ok, "an answered critique exits 0; stderr:\n{err}");
    assert_eq!(
        out,
        reject_out(SPEC_HASH, 0),
        "the first text's own findings"
    );
    assert!(
        err.contains(&format!(
            "rigger critique: {SPEC_REL} (hash {SPEC_HASH}): answered from the critique recorded \
             at attempt 0"
        )),
        "the verb says it answered from the record:\n{err}"
    );
    assert_eq!(critique_stub_spawns(third.path()), 0, "zero spawns");
    assert_eq!(
        critique_events(root).len(),
        4,
        "two critiques, no third request"
    );
    assert_eq!(review_findings(root), reject_copies(SPEC_HASH, 0, SPEC_REL));
}

#[test]
fn a_launch_that_records_no_result_says_why_and_the_next_call_runs_the_next_attempt() {
    let dir = temp_project();
    let root = dir.path();
    scaffold(root, CRITIC_WORKFLOW);
    let scratch = root.join("scratch");

    // When the critic's session reports its rigger server failed, the host stops it unrecorded ...
    let failed = tempfile::tempdir().unwrap();
    let failed_path = write_critique_stub_reporting(failed.path(), REJECT, "failed");
    let (out, err, ok) = critique(root, SPEC_REL, &failed_path, &scratch);

    // ... so no critique exists: the verb says why from the spawn's own error and exits non-zero.
    assert!(!ok, "a launch with no recorded result is no critique");
    assert!(
        err.contains(&format!(
            "rigger critique: {SPEC_REL} (hash {SPEC_HASH}): no critique was recorded - "
        )) && err.contains("MCP server reported status \"failed\" at init"),
        "the why is the launch fault:\n{err}"
    );
    assert_eq!(out, "", "nothing is printed on stdout");
    assert_eq!(critique_stub_spawns(failed.path()), 1);
    assert_eq!(
        spawn_events(&critique_events(root)),
        [(
            TYPE_SPAWN_REQUESTED.to_string(),
            critique_spawn_id(SPEC_HASH, 0)
        )],
        "the parked request stands with no result"
    );
    assert_eq!(review_findings(root), Vec::<Value>::new());

    // When the unchanged text is critiqued again, the request with no result counts: attempt 1.
    let (answering, answering_path) = stub(REJECT);
    let (out, err, ok) = critique(root, SPEC_REL, &answering_path, &scratch);
    assert!(ok, "the next attempt records a critique; stderr:\n{err}");
    assert_eq!(out, reject_out(SPEC_HASH, 1));
    assert_eq!(critique_stub_spawns(answering.path()), 1);
    assert_eq!(
        spawn_events(&critique_events(root))[1..],
        [
            (
                TYPE_SPAWN_REQUESTED.to_string(),
                critique_spawn_id(SPEC_HASH, 1)
            ),
            (
                TYPE_SPAWN_RESULT.to_string(),
                critique_spawn_id(SPEC_HASH, 1)
            ),
        ]
    );
    assert_eq!(review_findings(root), reject_copies(SPEC_HASH, 1, SPEC_REL));
}

#[test]
fn a_recorded_critique_whose_copies_were_never_appended_is_completed_with_zero_spawns() {
    let dir = temp_project();
    let root = dir.path();
    scaffold(root, CRITIC_WORKFLOW);

    // Given a store holding attempt 0's critique and, after it, attempt 1's result that is no
    // critique, with no copy ever appended (a crash between the result and the copies) ...
    let first = "critique-ff026c95e7c7af01/adversary#0";
    let second = "critique-ff026c95e7c7af01/adversary#1";
    let unit = "critique-ff026c95e7c7af01";
    seed_critique(
        root,
        &[
            (
                TYPE_SPAWN_REQUESTED,
                json!({"id": first, "unit": unit, "stage": "critique", "prompt": "p"}),
            ),
            (TYPE_SPAWN_RESULT, json!({"id": first, "output": REJECT})),
            (
                TYPE_SPAWN_REQUESTED,
                json!({"id": second, "unit": unit, "stage": "critique", "prompt": "p"}),
            ),
            (
                TYPE_SPAWN_RESULT,
                json!({"id": second, "output": "I could not decide."}),
            ),
        ],
    );
    let (work, path) = stub(APPROVE);
    let (out, err, ok) = critique(root, SPEC_REL, &path, &root.join("scratch"));

    // ... the next call answers from attempt 0's critique, the latest one, and appends its copies.
    assert!(ok, "an answered critique exits 0; stderr:\n{err}");
    assert_eq!(out, reject_out(SPEC_HASH, 0));
    assert!(
        err.contains("answered from the critique recorded at attempt 0"),
        "a later result that is no critique never displaces the critique:\n{err}"
    );
    assert_eq!(critique_stub_spawns(work.path()), 0, "zero spawns");
    assert_eq!(
        critique_events(root).len(),
        4,
        "nothing appended to the record"
    );
    assert_eq!(review_findings(root), reject_copies(SPEC_HASH, 0, SPEC_REL));
}

#[test]
fn a_reject_needs_a_blocking_line_and_an_approve_beside_one_is_a_critique_that_still_blocks() {
    let dir = temp_project();
    let root = dir.path();
    scaffold(root, CRITIC_WORKFLOW);
    let scratch = root.join("scratch");

    // When the critic rejects with only a NON-BLOCKING line, its result is no critique.
    let (_nb, nb_path) = stub("C1 | NON-BLOCKING | Design | x | y\n{\"verdict\":\"reject\"}");
    let (out, err, ok) = critique(root, SPEC_REL, &nb_path, &scratch);
    assert!(!ok, "a reject with no BLOCKING finding is no critique");
    assert!(
        err.contains(&format!(
            "rigger critique: {SPEC_REL} (hash {SPEC_HASH}): no critique was recorded - the \
             critic's verdict \"reject\" does not approve, yet its output carries no BLOCKING \
             finding line"
        )),
        "the verb prints why:\n{err}"
    );
    assert_eq!(out, "");
    assert_eq!(review_findings(root), Vec::<Value>::new());

    // When the critic approves beside a table-formatted BLOCKING line whose fix holds a pipe, and
    // prose follows the verdict line, the result is a critique and the line still blocks.
    let (_bl, bl_path) = stub(
        "| C1 | BLOCKING | criterion 1 | empty is undecided | decide it | then pin it |\n\
         {\"verdict\":\"approve\"}\nThat is all.",
    );
    let (out, err, ok) = critique(root, SPEC_REL, &bl_path, &scratch);
    assert!(
        ok,
        "an approve beside a BLOCKING line is a critique; stderr:\n{err}"
    );
    let summary = "BLOCKING | criterion 1 | empty is undecided | decide it | then pin it";
    assert_eq!(
        out,
        format!("sc-{SPEC_HASH}-1-1 | {summary}\n{{\"verdict\":\"approve\"}}\n")
    );
    assert_eq!(
        review_findings(root),
        [json!({
            "id": format!("sc-{SPEC_HASH}-1-1"),
            "by": "spec-critic",
            "summary": summary,
            "about": [SPEC_REL],
        })]
    );
}

#[test]
fn the_critic_is_the_plan_critique_gates_adversary_else_the_default_review_adversary() {
    for (workflow, persona) in [
        (
            DEFAULT_ADVERSARY_WORKFLOW,
            "You are the CRITIC-PERSONA adversary.",
        ),
        (GATE_AND_DEFAULT_WORKFLOW, "Judge."),
    ] {
        let dir = temp_project();
        let root = dir.path();
        scaffold(root, workflow);
        let (work, path) = stub(REJECT);
        let (out, err, ok) = critique(root, SPEC_REL, &path, &root.join("scratch"));
        assert!(ok, "a workflow naming a critic critiques; stderr:\n{err}");
        assert_eq!(out, reject_out(SPEC_HASH, 0));
        let argv = critique_stub_argv(work.path());
        let system = flag_value(&argv, "--system-prompt");
        assert!(
            system.starts_with(persona),
            "the critic opens with {persona:?}:\n{system}"
        );
    }
}

#[test]
fn a_workflow_with_no_critic_is_refused_first_and_no_store_or_graph_is_created() {
    let dir = temp_project();
    let root = dir.path();
    scaffold(root, NO_CRITIC_WORKFLOW);
    let (work, path) = stub(REJECT);
    let elsewhere = tempfile::tempdir().unwrap();
    let outside = elsewhere.path().join("9-demo.md");
    std::fs::write(&outside, "# no criteria\n").unwrap();
    let outside = outside.to_str().unwrap();

    // When a spec outside the repository, with no criteria, is critiqued under no critic ...
    let (out, err, ok) = critique(root, outside, &path, &root.join("scratch"));

    // ... the critic lookup refuses before the spec path or the loop-ready check is reached.
    assert!(!ok, "a workflow with no critic refuses");
    assert!(
        err.contains(&format!(
            "rigger critique: refusing to critique {outside}: the workflow names neither the \
             plan-critique gate's adversary nor defaults.review.adversary"
        )),
        "the no-critic refusal names the spec as given and both keys:\n{err}"
    );
    assert!(
        !err.contains("outside the repository") && !err.contains("loop-ready"),
        "only the first refusal reached is printed:\n{err}"
    );
    assert_eq!(out, "");
    assert_eq!(critique_stub_spawns(work.path()), 0);
    for file in ["events.db", "graph.db", "progress.db"] {
        assert!(
            !rigger_file(root, file).exists(),
            "no {file} is opened or created before the critic lookup passes"
        );
    }
}

#[test]
fn a_spec_path_that_climbs_above_the_root_is_outside_and_refused_before_the_loop_ready_check() {
    let dir = temp_project();
    let root = dir.path();
    scaffold(root, CRITIC_WORKFLOW);
    let (work, path) = stub(REJECT);
    let climbing = "specs/../../elsewhere.md";
    let (_out, err, ok) = critique(root, climbing, &path, &root.join("scratch"));
    assert!(!ok, "a path climbing above the root is outside it");
    assert!(
        err.contains(&format!(
            "rigger critique: refusing to critique {climbing}: it is outside the repository"
        )),
        "the outside refusal, ahead of reading a spec that does not exist:\n{err}"
    );
    assert!(!err.contains("read spec"), "the spec is never read:\n{err}");
    assert_eq!(critique_stub_spawns(work.path()), 0);
    assert!(!rigger_file(root, "events.db").exists());
}

#[test]
fn a_linked_worktree_is_refused_before_the_critic_lookup() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    let parent = tempfile::tempdir().unwrap();
    let linked = parent.path().join("linked");
    git_ok(
        root,
        &[
            "worktree",
            "add",
            linked.to_str().unwrap(),
            "-b",
            "linked-critique",
        ],
    );
    scaffold(&linked, NO_CRITIC_WORKFLOW);
    let (work, path) = stub(REJECT);
    let (_out, err, ok) = critique(&linked, SPEC_REL, &path, &linked.join("scratch"));
    assert!(!ok, "a linked worktree is refused");
    let main = std::fs::canonicalize(root).unwrap();
    let linked_canon = std::fs::canonicalize(&linked).unwrap();
    assert!(
        err.contains(&format!(
            "rigger critique: refusing to run from inside a linked worktree ({}) - the main \
             worktree is {}.",
            linked_canon.display(),
            main.display()
        )),
        "the linked-worktree refusal names the verb and both trees:\n{err}"
    );
    assert!(
        !err.contains("names neither"),
        "only the first refusal reached is printed:\n{err}"
    );
    assert_eq!(critique_stub_spawns(work.path()), 0);
    assert!(!rigger_file(&linked, "events.db").exists());
}

#[test]
fn a_project_nested_in_another_repository_is_refused_after_the_critic_lookup() {
    let dir = temp_project();
    let enclosing = dir.path();
    let scratch = enclosing.join("scratchroot");
    let nested = scratch.join("nested-fixture");
    let (work, path) = stub(REJECT);

    // Under a workflow with no critic, the critic lookup refuses first ...
    scaffold(&nested, NO_CRITIC_WORKFLOW);
    let (_out, err, ok) = critique(&nested, SPEC_REL, &path, &scratch);
    assert!(!ok);
    assert!(
        err.contains("names neither") && !err.contains("disagree on their root"),
        "the critic lookup comes before the one-root check:\n{err}"
    );

    // ... and under a workflow with a critic, the one-root check refuses, naming the verb.
    scaffold(&nested, CRITIC_WORKFLOW);
    let (_out, err, ok) = critique(&nested, SPEC_REL, &path, &scratch);
    assert!(
        !ok,
        "a store and a repository on different roots are refused"
    );
    assert!(
        err.contains(
            "rigger critique: refusing - the store this command would open and the repository \
             git resolved for this directory disagree on their root"
        ),
        "the one-root refusal leads with the verb:\n{err}"
    );
    assert_eq!(critique_stub_spawns(work.path()), 0);
    assert!(!rigger_file(&nested, "events.db").exists());
    assert!(!scratch.join("agent-live").exists() && !scratch.join("agent-stream").exists());
}

#[test]
fn a_scratch_root_inside_another_repository_is_refused_naming_the_verb() {
    let dir = temp_project();
    let root = dir.path();
    scaffold(root, CRITIC_WORKFLOW);
    let other = temp_project();
    let (work, path) = stub(REJECT);
    let (_out, err, ok) = critique(
        root,
        SPEC_REL,
        &path,
        &other.path().join("scratch-elsewhere"),
    );
    assert!(!ok, "a scratch root in another repository is refused");
    assert!(
        err.contains(
            "rigger critique: refusing - the scratch root this command would use belongs to a \
             DIFFERENT repository than the one this command resolved"
        ),
        "the scratch-root refusal leads with the verb:\n{err}"
    );
    assert_eq!(critique_stub_spawns(work.path()), 0);
    assert!(!rigger_file(root, "events.db").exists());
    let elsewhere = other.path().join("scratch-elsewhere");
    assert!(
        !elsewhere.join("agent-live").exists() && !elsewhere.join("agent-stream").exists(),
        "nothing is written into the other repository's scratch tree"
    );
}

#[test]
fn a_graph_that_owes_its_rebuild_refuses_every_call_answered_or_not_naming_rigger_setup() {
    let dir = temp_project();
    let root = dir.path();
    scaffold(root, CRITIC_WORKFLOW);
    let scratch = root.join("scratch");
    let (_first, first_path) = stub(REJECT);
    let (_out, err, ok) = critique(root, SPEC_REL, &first_path, &scratch);
    assert!(ok, "the text is critiqued; stderr:\n{err}");
    std::fs::write(rigger_file(root, "graph.db.owed"), "").unwrap();
    let refusal = rigger::contextgraph::rebuild_owed_refusal("critique");

    // When the graph owes its rebuild, a call the store would answer is refused ...
    let (later, later_path) = stub(REJECT);
    let (out, err, ok) = critique(root, SPEC_REL, &later_path, &scratch);
    assert!(!ok, "an answered call is refused");
    assert!(err.contains(&refusal), "it names `rigger setup`:\n{err}");
    assert_eq!(out, "", "nothing is answered");

    // ... and so is a call on new text, which parks no request and spawns nothing.
    std::fs::write(
        root.join(SPEC_REL),
        SPEC.replace("renders.", "renders now."),
    )
    .unwrap();
    let (out, err, ok) = critique(root, SPEC_REL, &later_path, &scratch);
    assert!(!ok, "a call on new text is refused");
    assert!(err.contains(&refusal), "it names `rigger setup`:\n{err}");
    assert_eq!(out, "");
    assert_eq!(critique_stub_spawns(later.path()), 0);
    assert_eq!(critique_events(root).len(), 2, "no request is parked");
    assert_eq!(review_findings(root), reject_copies(SPEC_HASH, 0, SPEC_REL));
}

#[test]
fn the_kurrentdb_flag_selects_the_server_backend_and_fabricates_no_local_store() {
    let dir = temp_project();
    let root = dir.path();
    scaffold(root, CRITIC_WORKFLOW);
    let (work, path) = stub(REJECT);
    let scratch = root.join("scratch");
    let state = tempfile::tempdir().unwrap();
    let out = common::cli::rigger_command(
        root,
        &[
            "critique",
            SPEC_REL,
            "--eventstore",
            "kurrentdb",
            "--conn",
            "kurrentdb://127.0.0.1:1/?tls=false",
        ],
        &[
            ("PATH", &path),
            ("RIGGER_TMPDIR", scratch.to_str().unwrap()),
        ],
        state.path(),
    )
    .output()
    .unwrap();
    assert_selected_server(&out, root, "rigger critique --eventstore kurrentdb");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("connect to kurrentdb://127.0.0.1:1/?tls=false"),
        "only a selected server addressed by --conn is dialled - never a refused flag or a \
         missing connection:\n{stderr}"
    );
    assert_eq!(critique_stub_spawns(work.path()), 0, "no critic is spawned");
}

/// Given a project whose configuration selects no server, when a spec is critiqued on a KurrentDB
/// server selected by the flags alone, then the critic is handed that selection as
/// `KURRENTDB_CONN`, and the critic's own bound rigger server, started under it, reads the
/// critique's findings back from that server through `rigger_peers`.
#[test]
fn a_server_selected_by_flags_alone_is_handed_to_the_critic_whose_bound_server_reads_it() {
    common::fixtures::with_kurrentdb(|conn| {
        let dir = temp_project();
        let root = dir.path();
        scaffold(root, CRITIC_WORKFLOW);
        let (work, path) = stub(REJECT);
        let scratch = root.join("scratch");
        let (out, err, ok) = run_rigger_envs(
            root,
            &[
                "critique",
                SPEC_REL,
                "--eventstore",
                "kurrentdb",
                "--conn",
                conn,
            ],
            &[
                ("PATH", &path),
                ("RIGGER_TMPDIR", scratch.to_str().unwrap()),
            ],
        );
        assert!(ok, "the critique records on the server; stderr:\n{err}");
        assert_eq!(out, reject_out(SPEC_HASH, 0));
        assert!(
            !rigger_file(root, "events.db").exists(),
            "the critique lives on the server, never a local store"
        );
        let handed = critique_stub_conn(work.path());
        assert_eq!(
            handed, conn,
            "the critic is handed the flag-selected server"
        );

        // The critic's bound server, under the environment the critic was handed, answers
        // `rigger_peers` from that server: the copies of the critique's findings about the spec.
        let state = tempfile::tempdir().unwrap();
        let spawn = critique_spawn_id(SPEC_HASH, 0);
        let mut mcp = common::cli::rigger_command(
            root,
            &["mcp", "--spawn", &spawn],
            &[("KURRENTDB_CONN", &handed)],
            state.path(),
        )
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
        let call = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {"name": "rigger_peers", "arguments": {"files": [SPEC_REL]}},
        });
        {
            use std::io::Write;
            let mut stdin = mcp.stdin.take().unwrap();
            writeln!(stdin, "{call}").unwrap();
        }
        let answered = mcp.wait_with_output().unwrap();
        let reply = String::from_utf8_lossy(&answered.stdout);
        assert!(
            answered.status.success(),
            "the critic's bound server starts under the handed selection; stderr:\n{}",
            String::from_utf8_lossy(&answered.stderr)
        );
        for id in [format!("sc-{SPEC_HASH}-0-1"), format!("sc-{SPEC_HASH}-0-2")] {
            assert!(
                reply.contains(&id),
                "rigger_peers reads the critique copy {id} from the server:\n{reply}"
            );
        }
    });
}

#[test]
fn a_store_from_before_the_minted_identity_is_migrated_before_the_verbs_first_append() {
    let dir = temp_project();
    let root = dir.path();
    scaffold(root, CRITIC_WORKFLOW);
    seed_store(root);
    // Given history recorded under the legacy basename namespace, then a minted identity ...
    emit(
        root,
        "DecisionMade",
        r#"{"id":"legacy-decision","summary":"pre-mint history","governs":["specs/9-demo.md"]}"#,
    );
    std::fs::write(root.join(".rigger/project.id"), "durablemint\n").unwrap();
    let (_work, path) = stub(REJECT);

    // ... when the spec is critiqued ...
    let (out, err, ok) = critique(root, SPEC_REL, &path, &root.join("scratch"));
    assert!(ok, "the critique records; stderr:\n{err}");
    assert_eq!(out, reject_out(SPEC_HASH, 0));

    // ... the history moved to the minted identity first, and the copies follow it there: the
    // legacy decision, the migration's own recorded decision, then the two copies.
    assert!(
        err.contains("migrated project identity") && err.contains("durablemint"),
        "the verb migrates the identity:\n{err}"
    );
    assert_eq!(run_stream_identity(root), "durablemint");
    assert_eq!(
        run_types(root),
        [
            "DecisionMade",
            "DecisionMade",
            "ReviewFinding",
            "ReviewFinding"
        ]
    );
    assert_eq!(
        critique_events(root).len(),
        2,
        "the record is under durablemint-critique"
    );
    let (peers, err, ok) = run_rigger(root, &["peers", SPEC_REL]);
    assert!(ok, "rigger peers succeeds; stderr:\n{err}");
    assert!(
        peers.contains("decision legacy-decision")
            && peers.contains(&format!("sc-{SPEC_HASH}-0-1")),
        "the migrated decision and the copies both read back about the spec:\n{peers}"
    );
}

#[test]
fn critique_is_a_known_command_with_its_usage_line() {
    let dir = temp_project();
    let (_out, err, ok) = run_rigger(dir.path(), &["no-such-command"]);
    assert!(!ok);
    let known = err
        .lines()
        .find_map(|line| line.strip_prefix("known commands: "))
        .unwrap_or_else(|| panic!("an unknown command lists the known ones:\n{err}"));
    assert!(
        known.split(", ").any(|command| command == "critique"),
        "critique is a known command: {known}"
    );
    let (_out, usage, _ok) = run_rigger(dir.path(), &["help"]);
    assert!(
        usage.contains(
            "rigger critique <spec>      critique the spec before any run: the workflow's critic\n"
        ),
        "the usage names the verb:\n{usage}"
    );
}
