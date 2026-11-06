//! Spec 112, criterion 1 (A SPEC IS CRITIQUED BEFORE THE RUN) - periphery: `rigger critique
//! <spec>` driven as the compiled binary, with the checked-in critique stub first on `PATH` as
//! the critic's `claude`. Given a project whose workflow names a critic, when the operator
//! critiques a spec, then the plan-critique adversary runs on the spec text through the headless
//! host, pass after pass with the recorded findings in its prompt until a pass adds none or the
//! ceiling stops it (spec 123, criterion 2), each pass's findings and verdict are recorded keyed on
//! the spec's content hash and copied into the graph, and a second call on unchanged text answers
//! from the store with zero spawns; an edited text is critiqued afresh and inherits none of the
//! earlier findings. A session
//! silent past its bound is stopped, since this verb's host is the one wired with no beat source
//! (spec 105 LIVENESS), and a session ending API-side closes `fault` whatever its class, since it
//! is the one wired with no hold sink (spec 105 HELD).
//!
//! NOT OWNED HERE: the refusal a new run meets without a clean critique (criterion 2), and the
//! plan-critique stop on a spec defect (criterion 5).

mod common;

use std::path::Path;

use common::cli::{
    assert_selected_server, critique_finding_id, declare_grep_guard, emit, no_result_failure,
    pin_identity, read_run_events, recorded_progress, resolve, rigger_file, run_payloads,
    run_rigger, run_rigger_envs, run_rigger_ok, run_stream_identity, seed_a_run, seed_critique,
    seed_critique_spawn, seed_on_the_run, seed_run_events, seed_store, temp_project,
    temp_repoless_project, write_spec_project,
};
use common::fixtures::{
    composed_system_prompt, git_ok, held_event, operator_helpers_agents_json,
    temp_git_project_with_commit, write_helper_copy, ADVISORY, CLEAN_CRITIQUE, NO_VERDICT,
    OPERATOR_HELPER_COPIES,
};
use common::repo::{
    assert_carries_spawn_hooks, collect_files, critique_stub_argv, critique_stub_record,
    critique_stub_spawns, critique_stub_task, flag_value, hold_critique_stub, hook_words,
    stub_path, write_critique_stub_reporting, write_critique_stub_then, CRITIQUE_CONN,
    CRITIQUE_SYSTEM_PROMPT, SILENT_PAST_THE_POLL,
};
use rigger::driver::claude_code::{framed, AGENT_STREAM_SUBDIR};
use rigger::driver::replay::spawn_scratch_path;
use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, Event, EventStore, Filter};
use rigger::liveness::{marker_path, scratch_subpath};
use rigger::progress::{RunStamped, SpawnLaunched, StopFailure};
use rigger::review::{
    critique_hash, critique_spawn_id, critique_unit, spec_critique_prompt, CRITIC_DENIED_TOOLS,
    CRITIQUE_RECORDED_FINDINGS,
};
use rigger::spawn::{SpawnEvent, SpawnResult, TYPE_SPAWN_REQUESTED, TYPE_SPAWN_RESULT};
use rigger::wave::NO_CRITIC_CLAUSE;
use serde_json::{json, Value};

/// A workflow whose plan-critique gate names the `critic` persona as its adversary.
const CRITIC_WORKFLOW: &str = "defaults:\n  grounder: nop\nstages:\n  plan:\n    agent: planner\n    \
     produces: dag\n  plan-critique:\n    needs: [plan]\n    adversary: critic\n    adjudicator: judge\n";

/// A workflow naming neither the plan-critique gate's adversary nor an adversary step in
/// `defaults.review`.
const NO_CRITIC_WORKFLOW: &str =
    "defaults:\n  grounder: nop\nstages:\n  a:\n    agent: planner\n    on_pass: none\n";

const PLANNER: &str =
    "---\nid: planner\nmodel: sonnet\ntools: [Read]\nisolation: none\n---\nDecompose the spec.\n";
const CRITIC: &str = "---\nid: critic\nmodel_ladder: [sonnet, opus]\ntools: [Read, Bash, Grep, \
     Agent]\nisolation: none\n---\nYou are the CRITIC-PERSONA adversary.\n";
const JUDGE: &str = "---\nid: judge\nmodel: sonnet\ntools: [Read]\nisolation: none\n---\nJudge. \
     End with {\"verdict\":\"approve\"}.\n";

/// The critic's `--allowed-tools`: its replacement tools, which already hold every rigger tool
/// the embedded helpers and the operator's copies declare, then the host's own grant.
const CRITIC_PRE_APPROVAL: &str =
    "Read,Glob,mcp__rigger__rigger_graph,mcp__rigger__rigger_ground,mcp__rigger__rigger_peers,\
     mcp__rigger__rigger_emit,mcp__rigger__rigger_progress,mcp__rigger__rigger_scratch";

const SPEC_REL: &str = "specs/9-demo.md";
const SPEC: &str = "# 9 - A demo spec\n\n## Design\n\nThe widget renders.\n\n## Done when\n\n\
     - [ ] a test proves the widget renders\n- [ ] a test proves the widget hides\n";

/// The critic's canned answer: prose, one BLOCKING and one NON-BLOCKING finding line, a reject.
const REJECT: &str = "I read the spec.\n\
     C1 | BLOCKING | criterion 2 | hides is undecided when empty | decide the empty corner in Design\n\
     C2 | NON-BLOCKING | Design | renders names no surface | name the surface\n\
     {\"verdict\":\"reject\"}";

/// [`ADVISORY`]'s one finding, as its summary.
const ADVISORY_SUMMARY: &str = "NON-BLOCKING | Design | renders is vague | name the surface";

/// What `rigger critique` prints for a [`REJECT`] critique of `text` recorded at `attempt`: each
/// finding as its record id and summary, then the verdict.
fn reject_out(text: &str, attempt: u32) -> String {
    format!(
        "{} | BLOCKING | criterion 2 | hides is undecided when empty | decide the empty corner in \
         Design\n{} | NON-BLOCKING | Design | renders names no surface | name the surface\n\
         {{\"verdict\":\"reject\"}}\n",
        critique_finding_id(text, attempt, 1),
        critique_finding_id(text, attempt, 2)
    )
}

/// The `ReviewFinding` copies of a [`REJECT`] critique of `text` at `attempt`, about `spec`.
fn reject_copies(text: &str, attempt: u32, spec: &str) -> Vec<Value> {
    vec![
        json!({
            "id": critique_finding_id(text, attempt, 1),
            "by": "spec-critic",
            "summary": "BLOCKING | criterion 2 | hides is undecided when empty | decide the empty corner in Design",
            "about": [spec],
        }),
        json!({
            "id": critique_finding_id(text, attempt, 2),
            "by": "spec-critic",
            "summary": "NON-BLOCKING | Design | renders names no surface | name the surface",
            "about": [spec],
        }),
    ]
}

/// The `ReviewFinding` copy of an [`ADVISORY`] critique of `text` at `attempt`, about [`SPEC_REL`].
fn advisory_copy(text: &str, attempt: u32) -> Value {
    json!({
        "id": critique_finding_id(text, attempt, 1),
        "by": "spec-critic",
        "summary": ADVISORY_SUMMARY,
        "about": [SPEC_REL],
    })
}

/// A project at `root` carrying `workflow` with its three personas and the demo spec.
fn scaffold(root: &Path, workflow: &str) {
    write_spec_project(
        root,
        &[("planner", PLANNER), ("critic", CRITIC), ("judge", JUDGE)],
        workflow,
        SPEC_REL,
        SPEC,
    );
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
    run_payloads(root, "ReviewFinding")
}

/// Every event in the project's `events.db`, every stream and namespace included.
fn every_event(root: &Path) -> usize {
    let backend = Store::open(rigger_file(root, "events.db").to_str().unwrap()).unwrap();
    backend
        .read_all(0, Direction::Forward, &Filter::default())
        .unwrap()
        .len()
}

/// A fresh critique stub answering `critique`: its work directory, which keeps it alive, and the
/// PATH that runs it.
fn stub(critique: &str) -> (tempfile::TempDir, String) {
    stub_then(critique, CLEAN_CRITIQUE)
}

/// [`stub`] whose later spawns answer `later`.
fn stub_then(critique: &str, later: &str) -> (tempfile::TempDir, String) {
    let work = tempfile::tempdir().unwrap();
    let path = write_critique_stub_then(work.path(), critique, later);
    (work, path)
}

/// Given a critique, when its critic launches, then its spawn-bound server runs the running
/// executable, and the launch carries the write guard rooted at the critic's working directory and
/// its spawn scratch container, the session's grep guard exactly when the workflow declares
/// `defaults.grep_guard`, and one `StopFailure` entry per category for its own spawn id, all on
/// that same executable, beside the deny the spawn's own settings hold.
#[test]
fn a_critique_launch_carries_the_spawn_hooks_on_its_own_settings() {
    for grep_guard in [false, true] {
        let dir = temp_project();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        scaffold(&root, CRITIC_WORKFLOW);
        if grep_guard {
            declare_grep_guard(&root);
        }
        let scratch = root.join("scratch");
        let (work, path) = stub(REJECT);
        let (_, err, ok) = critique(&root, SPEC_REL, &path, &scratch);
        assert!(ok, "the critique is recorded; stderr:\n{err}");

        let hash = critique_hash(SPEC);
        let spawn = critique_spawn_id(&hash, 0);
        let container =
            spawn_scratch_path(scratch.to_str().unwrap(), &critique_unit(&hash), &spawn).unwrap();
        let roots = [root, container].map(|root| root.to_str().unwrap().to_string());
        let argv = critique_stub_argv(work.path());
        let bin = std::fs::canonicalize(common::rigger_bin()).unwrap();
        let mcp: Value = serde_json::from_str(flag_value(&argv, "--mcp-config")).unwrap();
        assert_eq!(
            mcp["mcpServers"]["rigger"],
            serde_json::json!({"command": bin, "args": ["mcp", "--spawn", spawn]}),
            "the critic's spawn-bound server runs the running executable"
        );
        let settings: Value = serde_json::from_str(flag_value(&argv, "--settings")).unwrap();
        assert_carries_spawn_hooks(&settings, bin.to_str().unwrap(), &spawn, &roots, grep_guard);
        assert_eq!(
            settings["permissions"]["deny"].as_array().map(Vec::len),
            Some(CRITIC_DENIED_TOOLS.len()),
            "the spawn's own settings survive the merge: {settings}"
        );
    }
}

/// Given a project holding the operator's own copies of the fan-out helpers, when its critic
/// launches, then the launch defines exactly those helpers, and its pre-approval holds the rigger
/// tools they declare once each among the critic's own: the operator's copy is the definition,
/// resolved at the critique's root.
#[test]
fn a_critique_launch_carries_the_operators_helper_copies() {
    let dir = temp_project();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    scaffold(&root, CRITIC_WORKFLOW);
    for (file, text) in OPERATOR_HELPER_COPIES {
        write_helper_copy(&root, file, text);
    }
    let (work, path) = stub(REJECT);
    let (_, err, ok) = critique(&root, SPEC_REL, &path, &root.join("scratch"));
    assert!(ok, "the critique is recorded; stderr:\n{err}");

    let argv = critique_stub_argv(work.path());
    assert_eq!(
        serde_json::from_str::<Value>(flag_value(&argv, "--agents")).unwrap(),
        operator_helpers_agents_json(),
        "the critic's helpers are the operator's copies"
    );
    assert_eq!(
        flag_value(&argv, "--allowed-tools"),
        CRITIC_PRE_APPROVAL,
        "the critic's tools already hold the rigger tools the operator's copies declare, so the \
         pre-approval adds only the host's own"
    );
}

/// Spec 105's stray record: given a project with a run, when a critic's session ends on an API
/// error and fires the `StopFailure` command its launch's settings carry, then the record stands
/// on the project's run under the critic's own spawn id, which is what leaves every other launch's
/// class alone.
#[test]
fn a_critics_stop_failure_hook_leaves_its_record_on_the_run_under_its_own_spawn() {
    let dir = temp_project();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    scaffold(&root, CRITIC_WORKFLOW);
    seed_store(&root);
    seed_a_run(&root);
    let scratch = root.join("scratch");
    let (work, path) = stub(REJECT);
    let (_, err, ok) = critique(&root, SPEC_REL, &path, &scratch);
    assert!(ok, "the critique is recorded; stderr:\n{err}");

    let argv = critique_stub_argv(work.path());
    let settings: Value = serde_json::from_str(flag_value(&argv, "--settings")).unwrap();
    let (class, words) = hook_words(&settings, "StopFailure")
        .into_iter()
        .find(|(class, _)| class == "rate_limit")
        .expect("the launch carries an entry per category");
    let command: Vec<&str> = words[1..].iter().map(String::as_str).collect();
    run_rigger_ok(&root, &command);

    assert_eq!(
        recorded_progress::<StopFailure>(&root),
        vec![StopFailure {
            spawn: critique_spawn_id(&critique_hash(SPEC), 0),
            class,
        }],
        "the one record, on the project's run, naming the critic"
    );
}

#[test]
fn a_spec_is_critiqued_once_per_text_and_answered_from_the_store_after() {
    let dir = temp_project();
    let root = dir.path();
    scaffold(root, CRITIC_WORKFLOW);
    let scratch = root.join("scratch");
    let (first, first_path) = stub(REJECT);
    let hash = critique_hash(SPEC);
    let expected_out = reject_out(SPEC, 0);

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

    // ... the plan-critique adversary ran on the spec text through the headless host, and again
    // once its first pass left findings, the second pass adding none.
    assert_eq!(
        critique_stub_spawns(first.path()),
        2,
        "two critic spawns: the reject, then the pass that adds nothing"
    );
    let task = critique_stub_task(first.path());
    let prompt = spec_critique_prompt(SPEC_REL, SPEC, []);
    assert!(
        prompt.ends_with(SPEC),
        "the critique prompt ends with the spec text verbatim:\n{prompt}"
    );
    assert_eq!(
        task,
        framed(&prompt),
        "the task is the critique prompt under the host's frame and nothing else: the method is \
         the persona's"
    );
    assert!(
        task.contains("`specs/9-demo.md`"),
        "the prompt names the spec path"
    );
    let argv = critique_stub_argv(first.path());
    assert_eq!(
        flag_value(&argv, "--allowed-tools"),
        CRITIC_PRE_APPROVAL,
        "the critic's tools are replaced: no Bash, Agent or Grep, and the host's own grant, which \
         the deny below wins over"
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
        critique_stub_record(first.path(), CRITIQUE_CONN),
        "unset",
        "a sqlite selection hands the critic no server: its bound rigger server resolves the \
         local store through configuration"
    );
    assert!(
        !argv.iter().any(|a| a == "--system-prompt"),
        "the persona never travels as an argv string: {argv:?}"
    );
    let system = critique_stub_record(first.path(), CRITIQUE_SYSTEM_PROMPT);
    assert!(
        system.starts_with("You are the CRITIC-PERSONA adversary."),
        "the critic's system prompt opens with the persona the workflow names:\n{system}"
    );
    assert_eq!(
        system,
        composed_system_prompt(CRITIC),
        "the critic's system prompt is compose's output over its persona, as every spawn's is"
    );

    // ... each pass's request and result are on the critique's own stream, keyed on the hash.
    let spawn = critique_spawn_id(&hash, 0);
    let exhausting = critique_spawn_id(&hash, 1);
    let recorded = vec![
        (TYPE_SPAWN_REQUESTED.to_string(), spawn.clone()),
        (TYPE_SPAWN_RESULT.to_string(), spawn.clone()),
        (TYPE_SPAWN_REQUESTED.to_string(), exhausting.clone()),
        (TYPE_SPAWN_RESULT.to_string(), exhausting),
    ];
    assert_eq!(spawn_events(&critique_events(root)), recorded);

    // ... and each finding is copied to the project run stream, where `rigger peers` shows it.
    let copies = reject_copies(SPEC, 0, SPEC_REL);
    assert_eq!(review_findings(root), copies);
    let (peers, err, ok) = run_rigger(root, &["peers", SPEC_REL]);
    assert!(ok, "rigger peers succeeds; stderr:\n{err}");
    assert!(
        peers.contains(&critique_finding_id(SPEC, 0, 1))
            && peers.contains(&critique_finding_id(SPEC, 0, 2)),
        "the graph shows the copied findings about the spec:\n{peers}"
    );
    for sub in ["agent-live", "agent-stream", "agent-scratch"] {
        assert!(
            !scratch.join(sub).join(format!("critique-{hash}")).exists(),
            "the critique run's {sub} directory goes once its spawns return"
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
        2,
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
    let (second, second_path) = stub(CLEAN_CRITIQUE);
    let (fresh, err, ok) = critique(root, SPEC_REL, &second_path, &scratch);
    assert!(ok, "the edited text's critique exits 0; stderr:\n{err}");
    assert_eq!(
        critique_stub_spawns(second.path()),
        1,
        "the edited text spawns the critic"
    );
    assert_eq!(
        critique_stub_spawns(first.path()),
        2,
        "the first stub is not spawned again"
    );
    // ... and inherits none of the earlier findings.
    assert_eq!(fresh, "{\"verdict\":\"approve\"}\n");
    assert!(!fresh.contains(&hash), "no earlier finding is printed");
    assert_eq!(
        critique_stub_task(second.path()),
        framed(&spec_critique_prompt(SPEC_REL, &edited, [])),
        "the critic read the edited text"
    );
    let edited_spawn = critique_spawn_id(&critique_hash(&edited), 0);
    assert_eq!(
        spawn_events(&critique_events(root))[4..],
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

    // ... the verb refuses, naming both critic sources, answering nothing and recording nothing.
    assert!(!ok, "a workflow with no critic refuses the verb");
    assert!(
        err.contains(NO_CRITIC_CLAUSE),
        "the refusal names the gate adversary and the adversary step:\n{err}"
    );
    assert_eq!(out, "", "a refusal prints no finding");
    assert_eq!(
        critique_stub_spawns(stub.path()),
        2,
        "no spawn past the first call's two passes"
    );
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
    assert!(
        out.contains(&format!("{} | BLOCKING", critique_finding_id(SPEC, 0, 1))),
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
    assert_eq!(
        critique_stub_spawns(stub.path()),
        2,
        "the reject, then the pass that adds nothing"
    );
    assert!(
        !root.join("scratch").exists(),
        "a project with no git repository has no scratch root: the session writes no liveness \
         marker or transcript, even with RIGGER_TMPDIR set"
    );
}

/// Given a critic bounded to one second, when its session goes silent past the bound, then the
/// host stops it: no beat ever lands on a critique, so this verb wires no beat source and counts
/// from the session's last line, and a source here would read the whole silence as unsupervised
/// and let a hung critic run on. The verb fails on the stop, never on the answer the session would
/// have gone on to give.
#[test]
fn a_critique_session_silent_past_its_bound_is_stopped_since_no_beat_source_is_wired() {
    let dir = temp_project();
    let root = dir.path();
    let bounded = BOUNDED_CRITIC_WORKFLOW.replace("max_wall_clock: 900", "max_wall_clock: 1");
    scaffold(root, &bounded);
    let work = tempfile::tempdir().unwrap();
    let path = stub_path(work.path(), "claude", Some(SILENT_PAST_THE_POLL));

    let (out, err, ok) = critique(root, SPEC_REL, &path, &root.join("scratch"));

    assert!(!ok, "a stopped critique exits non-zero");
    assert_eq!(out, "", "no finding or verdict is printed");
    let id = critique_spawn_id(SPEC_HASH, 0);
    let stopped = format!(
        "rigger: rigger critique: {SPEC_REL} (hash {SPEC_HASH}): no critique was recorded - the \
         critic's spawn failed: spawn {id:?} stopped: its agent stream stayed silent for 1s (its \
         max_wall_clock bound) with no result"
    );
    assert_eq!(
        err.lines()
            .last()
            .and_then(|line| line.split_once(" - no remediation"))
            .map(|(head, _)| head),
        Some(stopped.as_str()),
        "stderr:\n{err}"
    );
    let events = critique_events(root);
    assert_eq!(
        spawn_events(&events),
        [
            (TYPE_SPAWN_REQUESTED.to_string(), id.clone()),
            (TYPE_SPAWN_RESULT.to_string(), id),
        ]
    );
    assert!(
        SpawnResult::from_event(&events[1])
            .unwrap()
            .is_liveness_fault(),
        "the session's one result is the stop's liveness fault: {:?}",
        events[1]
    );
}

/// How each launch of the critique of `hash` closed, in log order: its ordinal, ending and class.
fn critique_closes(root: &Path, hash: &str) -> Vec<(u32, String, Option<String>)> {
    let backend = Store::open(rigger_file(root, "progress.db").to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    rigger::progress::read_run(&store, &critique_unit(hash))
        .unwrap()
        .iter()
        .filter(|e| e.type_ == SpawnLaunched::EVENT_TYPE)
        .filter_map(|e| {
            let launch: SpawnLaunched = serde_json::from_slice(&e.data).unwrap();
            let ended = launch.ended?;
            Some((launch.launch, ended, launch.class))
        })
        .collect()
}

/// Spec 105, the host's `interrupted` close behind its hold sink: given a critic session that ends
/// API-side, when `rigger critique` hosts it, then the launch closes `fault` under its class, a
/// hold or wait class and a run standing held on the project included, the session's error is the
/// verb's and no hold lands: this verb's host is wired with no hold sink, since nothing decorates
/// it to wait out a hold or to launch again.
#[test]
fn a_critique_session_ending_api_side_closes_fault_since_no_hold_sink_is_wired() {
    struct Case {
        name: &'static str,
        ending: &'static str,
        retried: &'static str,
        class: &'static str,
        /// What the session left on its stderr, the tail of the verb's error.
        tail: &'static str,
        run_held: bool,
    }
    let cases = [
        Case {
            name: "a wait class",
            ending: "retried-eof",
            retried: "rate_limit",
            class: "rate_limit",
            tail: "",
            run_held: false,
        },
        Case {
            name: "a hold class",
            ending: "retried-eof",
            retried: "billing_error",
            class: "billing_error",
            tail: "",
            run_held: false,
        },
        Case {
            name: "an unclassed ending",
            ending: "refused-eof",
            retried: "",
            class: "unknown",
            tail: "No conversation found with session ID: sess-prior",
            run_held: false,
        },
        Case {
            name: "a FAULT class on a held run",
            ending: "retried-eof",
            retried: "invalid_request",
            class: "invalid_request",
            tail: "",
            run_held: true,
        },
    ];
    for case in cases {
        let Case { name, class, .. } = case;
        let dir = temp_project();
        let root = dir.path();
        scaffold(root, CRITIC_WORKFLOW);
        if case.run_held {
            seed_store(root);
            seed_a_run(root);
            seed_on_the_run(
                root,
                held_event("rate_limit", rigger::run::HOLD_BY_OPERATOR),
            );
        }
        let work = tempfile::tempdir().unwrap();
        let path = stub_path(
            work.path(),
            "claude",
            Some("claude-code-resume-ending-agent.sh"),
        );
        let scratch = root.join("scratch");

        let (out, err, ok) = run_rigger_envs(
            root,
            &["critique", SPEC_REL],
            &[
                ("PATH", path.as_str()),
                ("RIGGER_TMPDIR", scratch.to_str().unwrap()),
                ("RIGGER_TEST_FRESH_ENDING", case.ending),
                ("RIGGER_TEST_RETRY_CATEGORY", case.retried),
            ],
        );

        assert!(!ok, "{name}: the verb fails on the session's error");
        assert_eq!(out, "", "{name}: no finding or verdict is printed");
        assert_eq!(
            critique_closes(root, SPEC_HASH),
            [(0, "fault".to_string(), Some(class.to_string()))],
            "{name}: the one close; stderr:\n{err}"
        );
        let id = critique_spawn_id(SPEC_HASH, 0);
        let failed = format!(
            "rigger: rigger critique: {SPEC_REL} (hash {SPEC_HASH}): no critique was recorded - \
             conductor: {}",
            no_result_failure(&id, class, case.tail)
        );
        assert_eq!(
            err.lines().last(),
            Some(failed.as_str()),
            "{name}: the error the verb leaves with"
        );
        assert_eq!(
            spawn_events(&critique_events(root)),
            [(TYPE_SPAWN_REQUESTED.to_string(), id)],
            "{name}: the close records no result"
        );
        assert_eq!(
            run_payloads(root, rigger::run::TYPE_RUN_HELD).len(),
            usize::from(case.run_held),
            "{name}: no hold lands on the project's run"
        );
    }
}

#[test]
fn a_result_that_is_no_critique_exits_non_zero_saying_why_and_the_next_call_spawns_attempt_one() {
    let dir = temp_project();
    let root = dir.path();
    scaffold(root, CRITIC_WORKFLOW);
    let scratch = root.join("scratch");
    let hash = critique_hash(SPEC);

    // When the critic answers with no verdict line ...
    let (silent, silent_path) = stub(NO_VERDICT);
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
    let mut left = Vec::new();
    for area in ["agent-live", "agent-scratch", "agent-stream"] {
        collect_files(&scratch.join(area), &mut left);
    }
    assert_eq!(
        left,
        Vec::<std::path::PathBuf>::new(),
        "the spawn that recorded no pass is reaped like one that did"
    );

    // When the unchanged text is critiqued again, the recorded request counts: attempt 1 runs.
    let (answering, answering_path) = stub(REJECT);
    let (out, err, ok) = critique(root, SPEC_REL, &answering_path, &scratch);
    assert!(ok, "the next attempt records a critique; stderr:\n{err}");
    assert_eq!(critique_stub_spawns(silent.path()), 1);
    assert_eq!(
        critique_stub_spawns(answering.path()),
        2,
        "attempt 1's reject, then attempt 2's pass that adds nothing"
    );
    assert!(
        out.starts_with(&format!(
            "{} | BLOCKING | criterion 2 | ",
            critique_finding_id(SPEC, 1, 1)
        )),
        "the finding ids carry attempt 1:\n{out}"
    );
    let second = critique_spawn_id(&hash, 1);
    let third = critique_spawn_id(&hash, 2);
    assert_eq!(
        spawn_events(&critique_events(root))[2..],
        [
            (TYPE_SPAWN_REQUESTED.to_string(), second.clone()),
            (TYPE_SPAWN_RESULT.to_string(), second),
            (TYPE_SPAWN_REQUESTED.to_string(), third.clone()),
            (TYPE_SPAWN_RESULT.to_string(), third),
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
        out.starts_with(&format!(
            "{} | BLOCKING | ",
            critique_finding_id(SPEC, 0, 1)
        )),
        "the critique is recorded and printed:\n{out}"
    );
    assert_eq!(
        critique_stub_spawns(stub.path()),
        2,
        "the reject, then the pass that adds nothing"
    );
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

/// A workflow with no plan-critique gate whose `defaults.review` lists the adversary steps
/// `critic` then `judge`: the first is the critic.
const LISTED_CRITIC_WORKFLOW: &str = "defaults:\n  grounder: nop\n  review:\n    steps: \
     [{ adversary: critic }, { adversary: judge }, { adjudicator: judge }]\nstages:\n  a:\n    \
     agent: planner\n    on_pass: none\n";

/// [`LISTED_CRITIC_WORKFLOW`] with its two adversary steps swapped.
const LISTED_JUDGE_WORKFLOW: &str = "defaults:\n  grounder: nop\n  review:\n    steps: \
     [{ adversary: judge }, { adversary: critic }, { adjudicator: judge }]\nstages:\n  a:\n    \
     agent: planner\n    on_pass: none\n";

/// The `(type, payload)` of each critique-stream event.
fn critique_payloads(root: &Path) -> Vec<(String, Value)> {
    critique_events(root)
        .iter()
        .map(|e| (e.type_.clone(), serde_json::from_slice(&e.data).unwrap()))
        .collect()
}

/// The types of the project run stream's events, oldest first.
fn run_types(root: &Path) -> Vec<String> {
    read_run_events(root)
        .iter()
        .map(|e| e.type_.clone())
        .collect()
}

/// Whether `err` holds, as one whole line, the verb's answered-from line for [`SPEC_REL`] (hash
/// [`SPEC_HASH`]) recorded over `passes`, `1 pass` or `<n> passes` (spec 123): matched whole, so a
/// line that only begins with it is no match.
fn answered_over(err: &str, passes: &str) -> bool {
    verb_lines(err).contains(&answered(passes).as_str())
}

/// The head of every stderr line the verb prints about [`SPEC_REL`] (hash [`SPEC_HASH`]).
fn verb_head() -> String {
    format!("rigger critique: {SPEC_REL} (hash {SPEC_HASH}): ")
}

/// The verb's stderr lines about [`SPEC_REL`], in order, each past its [`verb_head`]; an error
/// line, which `rigger: ` opens, is none of them.
fn verb_lines(err: &str) -> Vec<&str> {
    let head = verb_head();
    err.lines()
        .filter_map(|line| line.strip_prefix(head.as_str()))
        .collect()
}

/// The answered-from line past its [`verb_head`], recorded over `passes`.
fn answered(passes: &str) -> String {
    format!("answered from the critique recorded over {passes}")
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
    assert_eq!(out, reject_out(SPEC, 0));
    assert_eq!(review_findings(root), reject_copies(SPEC, 0, SPEC_REL));
    let prompt = spec_critique_prompt(SPEC_REL, SPEC, []);
    assert_eq!(
        critique_stub_task(work.path()),
        framed(&prompt),
        "the critic's task is the critique prompt of the repo-relative path and the raw text, \
         byte for byte, under the host's frame"
    );

    // Each pass's request and result are persisted on the critique's own stream in their wire
    // form: the reject at attempt 0, then the pass at attempt 1 that adds nothing.
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
            (
                TYPE_SPAWN_REQUESTED,
                "critique-ff026c95e7c7af01/adversary#1"
            ),
            (TYPE_SPAWN_RESULT, "critique-ff026c95e7c7af01/adversary#1"),
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

    // Each session's liveness marker, transcript and spawn scratch (its system-prompt file) lived
    // under the critique run and went with it.
    for sub in ["agent-live", "agent-stream", "agent-scratch"] {
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
        6,
        "four spawn events on the critique stream and two copies on the run stream, nothing else"
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
    let (_second, second_path) = stub(CLEAN_CRITIQUE);
    let (out, err, ok) = critique(root, SPEC_REL, &second_path, &scratch);
    assert!(ok, "the edited text is critiqued; stderr:\n{err}");
    assert_eq!(out, "{\"verdict\":\"approve\"}\n");

    // ... when the text is reverted to the first bytes ...
    std::fs::write(root.join(SPEC_REL), SPEC).unwrap();
    let (third, third_path) = stub(CLEAN_CRITIQUE);
    let (out, err, ok) = critique(root, SPEC_REL, &third_path, &scratch);

    // ... the first hash's critique answers it, with zero spawns and nothing appended.
    assert!(ok, "an answered critique exits 0; stderr:\n{err}");
    assert_eq!(out, reject_out(SPEC, 0), "the first text's own findings");
    assert!(
        answered_over(&err, "2 passes"),
        "the verb says it answered from the record of the hash's two passes:\n{err}"
    );
    assert_eq!(critique_stub_spawns(third.path()), 0, "zero spawns");
    assert_eq!(
        critique_events(root).len(),
        6,
        "the first text's two passes and the edit's one, no further request"
    );
    assert_eq!(review_findings(root), reject_copies(SPEC, 0, SPEC_REL));
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
    assert_eq!(out, reject_out(SPEC, 1));
    assert_eq!(
        critique_stub_spawns(answering.path()),
        2,
        "attempt 1's reject, then attempt 2's pass that adds nothing"
    );
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
            (
                TYPE_SPAWN_REQUESTED.to_string(),
                critique_spawn_id(SPEC_HASH, 2)
            ),
            (
                TYPE_SPAWN_RESULT.to_string(),
                critique_spawn_id(SPEC_HASH, 2)
            ),
        ]
    );
    assert_eq!(review_findings(root), reject_copies(SPEC, 1, SPEC_REL));
}

#[test]
fn a_recorded_critique_whose_copies_were_never_appended_is_completed_with_zero_spawns() {
    let dir = temp_project();
    let root = dir.path();
    scaffold(root, CRITIC_WORKFLOW);

    // Given a store holding attempt 0's critique, after it attempt 1's result that is no critique
    // and attempt 2's pass that adds nothing, with no copy ever appended (a crash between the
    // results and the copies) ...
    let first = "critique-ff026c95e7c7af01/adversary#0";
    let second = "critique-ff026c95e7c7af01/adversary#1";
    let third = "critique-ff026c95e7c7af01/adversary#2";
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
                json!({"id": second, "output": NO_VERDICT}),
            ),
            (
                TYPE_SPAWN_REQUESTED,
                json!({"id": third, "unit": unit, "stage": "critique", "prompt": "p"}),
            ),
            (
                TYPE_SPAWN_RESULT,
                json!({"id": third, "output": CLEAN_CRITIQUE}),
            ),
        ],
    );
    let (work, path) = stub(CLEAN_CRITIQUE);
    let (out, err, ok) = critique(root, SPEC_REL, &path, &root.join("scratch"));

    // ... the next call answers from the hash's two passes, attempt 0's and attempt 2's, and
    // appends the copies.
    assert!(ok, "an answered critique exits 0; stderr:\n{err}");
    assert_eq!(out, reject_out(SPEC, 0));
    assert!(
        answered_over(&err, "2 passes"),
        "a later attempt's result that is no pass never displaces an earlier pass:\n{err}"
    );
    assert_eq!(critique_stub_spawns(work.path()), 0, "zero spawns");
    assert_eq!(
        critique_events(root).len(),
        6,
        "nothing appended to the record"
    );
    assert_eq!(review_findings(root), reject_copies(SPEC, 0, SPEC_REL));
}

/// Spec 123, THE RECORD IS THE UNION: given a hash whose store holds passes at attempts 0, 2 and
/// 3 around attempt 1's result that is no pass, when the unchanged text is critiqued, then the verb
/// answers from the record of every pass with zero spawns: it says it answered over three passes,
/// prints every finding of the union in pass order under the id its pass minted, then the verdict
/// the union derives, and copies every finding of the union.
#[test]
fn an_answered_critique_prints_the_union_of_its_passes_then_the_verdict_it_derives() {
    let dir = temp_project();
    let root = dir.path();
    scaffold(root, CRITIC_WORKFLOW);
    for (attempt, output) in [
        (0, ADVISORY),
        (1, NO_VERDICT),
        (2, REJECT),
        (3, CLEAN_CRITIQUE),
    ] {
        seed_critique_spawn(root, SPEC_HASH, attempt, output);
    }
    let (work, path) = stub(CLEAN_CRITIQUE);
    let (out, err, ok) = critique(root, SPEC_REL, &path, &root.join("scratch"));

    assert!(ok, "an answered critique exits 0; stderr:\n{err}");
    assert_eq!(
        out,
        format!(
            "{} | {ADVISORY_SUMMARY}\n{}",
            critique_finding_id(SPEC, 0, 1),
            reject_out(SPEC, 2)
        ),
        "attempt 0's finding, then attempt 2's two, then the union's BLOCKING line rejects"
    );
    assert!(
        answered_over(&err, "3 passes"),
        "the verb says how many passes it answered from:\n{err}"
    );
    assert_eq!(critique_stub_spawns(work.path()), 0, "zero spawns");
    assert_eq!(
        critique_events(root).len(),
        8,
        "nothing appended to the record"
    );
    let mut copies = vec![advisory_copy(SPEC, 0)];
    copies.extend(reject_copies(SPEC, 2, SPEC_REL));
    assert_eq!(review_findings(root), copies, "every finding of the union");
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
        format!(
            "{} | {summary}\n{{\"verdict\":\"reject\"}}\n",
            critique_finding_id(SPEC, 1, 1)
        ),
        "the verdict printed is the one the union derives: its BLOCKING line rejects"
    );
    assert_eq!(
        review_findings(root),
        [json!({
            "id": critique_finding_id(SPEC, 1, 1),
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
        (
            LISTED_CRITIC_WORKFLOW,
            "You are the CRITIC-PERSONA adversary.",
        ),
        (LISTED_JUDGE_WORKFLOW, "Judge."),
    ] {
        let dir = temp_project();
        let root = dir.path();
        scaffold(root, workflow);
        let (work, path) = stub(REJECT);
        let (out, err, ok) = critique(root, SPEC_REL, &path, &root.join("scratch"));
        assert!(ok, "a workflow naming a critic critiques; stderr:\n{err}");
        assert_eq!(out, reject_out(SPEC, 0));
        let system = critique_stub_record(work.path(), CRITIQUE_SYSTEM_PROMPT);
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
             plan-critique gate's adversary nor an adversary step in defaults.review"
        )),
        "the no-critic refusal names the spec as given and both places a critic is named:\n{err}"
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

/// A list naming no adversary step has no critic, whichever steps it does name.
#[test]
fn a_default_review_listing_no_adversary_step_has_no_critic() {
    for review in [
        "{ steps: [{ lenses: [judge] }, { adjudicator: judge }] }",
        "{ steps: [{ adjudicator: judge }] }",
    ] {
        let dir = temp_project();
        let root = dir.path();
        let workflow = format!(
            "defaults:\n  grounder: nop\n  review: {review}\nstages:\n  a:\n    agent: planner\n    \
             on_pass: none\n"
        );
        scaffold(root, &workflow);
        let (work, path) = stub(REJECT);
        let (out, err, ok) = critique(root, SPEC_REL, &path, &root.join("scratch"));
        assert!(!ok, "{review} is refused");
        assert_eq!(out, "", "{review}");
        assert_eq!(
            err,
            format!(
                "rigger: rigger critique: refusing to critique {SPEC_REL}: {NO_CRITIC_CLAUSE}\n"
            ),
            "{review}"
        );
        assert_eq!(critique_stub_spawns(work.path()), 0, "{review}");
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
    common::fixtures::stamp_user_version(
        &rigger_file(root, "graph.db"),
        rigger::contextgraph::sqlite::PROJECTION_VERSION - 1,
    );
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
    assert_eq!(
        critique_events(root).len(),
        4,
        "no request is parked past the first call's two passes"
    );
    assert_eq!(review_findings(root), reject_copies(SPEC, 0, SPEC_REL));
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
        assert_eq!(out, reject_out(SPEC, 0));
        assert!(
            !rigger_file(root, "events.db").exists(),
            "the critique lives on the server, never a local store"
        );
        let handed = critique_stub_record(work.path(), CRITIQUE_CONN);
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
        for id in [
            critique_finding_id(SPEC, 0, 1),
            critique_finding_id(SPEC, 0, 2),
        ] {
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
    pin_identity(root, "durablemint");
    let (_work, path) = stub(REJECT);

    // ... when the spec is critiqued ...
    let (out, err, ok) = critique(root, SPEC_REL, &path, &root.join("scratch"));
    assert!(ok, "the critique records; stderr:\n{err}");
    assert_eq!(out, reject_out(SPEC, 0));

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
        4,
        "the record of both passes is under durablemint-critique"
    );
    let (peers, err, ok) = run_rigger(root, &["peers", SPEC_REL]);
    assert!(ok, "rigger peers succeeds; stderr:\n{err}");
    assert!(
        peers.contains("decision legacy-decision")
            && peers.contains(&critique_finding_id(SPEC, 0, 1)),
        "the migrated decision and the copies both read back about the spec:\n{peers}"
    );
}

/// The critique block of the usage text, `rigger critique`'s one help source, which
/// `rigger critique --help` prints alone: each spelling of the bare help prints it whole, and no
/// key of one panel form.
#[test]
fn the_critique_help_names_an_adversary_step_of_either_panel_form() {
    const CRITIQUE_HELP: &str = "  \
rigger critique <spec>      critique the spec before any run: the workflow's critic
                              (the plan-critique gate's adversary, else
                              an adversary step in defaults.review) reads the spec
                              text pass after pass, each recorded keyed on the
                              text's content hash, until a pass adds no finding or
                              its attempts reach the ceiling (the effective
                              max_retries of the stage it is read from); the union
                              of the passes is printed, and a text exhausted, or at
                              the ceiling with a pass recorded, is answered from
                              the record with no spawn; takes --eventstore/--conn
                              as run does
";
    let dir = temp_project();
    for spelling in ["help", "--help", "-h"] {
        let (_out, usage, _ok) = run_rigger(dir.path(), &[spelling]);
        let start = usage.find("  rigger critique ").expect("the critique line");
        let end = usage
            .find("  rigger validate ")
            .expect("the next verb's line");
        assert_eq!(&usage[start..end], CRITIQUE_HELP, "rigger {spelling}");
        assert!(
            !usage.contains("defaults.review.adversary"),
            "the usage names no map key as the critic:\n{usage}"
        );
    }
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

// ---------------------------------------------------------------------------------------------
// The SDET periphery layer over the second attempt's delta: the spec-positional rule every
// spec-taking verb now shares, the result a concurrent call recorded first, an unreadable request
// on the critique stream, the removal a repo-less project never runs, and the store selection
// handed to the critic from a configured rung and over an ambient one.
// ---------------------------------------------------------------------------------------------

/// The names under the project's `.rigger/` directory, sorted.
fn rigger_dir_names(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(root.join(rigger::config::RIGGER_DIR))
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// Given a project, when the operator hands `rigger run`, `rigger workflow` or `rigger critique` a
/// second spec path or an unknown flag - before or after the spec - then each verb refuses through
/// the one spec-positional rule they share: exactly one stderr line naming the verb and the
/// argument, a non-zero exit, nothing on stdout and nothing written into the project.
#[test]
fn every_spec_taking_verb_refuses_a_second_spec_and_an_unknown_flag_through_the_one_rule() {
    let dir = temp_project();
    let root = dir.path();
    scaffold(root, CRITIC_WORKFLOW);
    let (work, path) = stub(REJECT);
    let scratch = root.join("scratch");
    let envs = [
        ("PATH", path.as_str()),
        ("RIGGER_TMPDIR", scratch.to_str().unwrap()),
    ];
    let before = rigger_dir_names(root);
    for (args, refusal) in [
        (
            ["run", SPEC_REL, "specs/other.md"],
            "run: unexpected second positional argument \"specs/other.md\"",
        ),
        (["run", "--frob", SPEC_REL], "run: unknown flag \"--frob\""),
        (
            ["workflow", SPEC_REL, "specs/other.md"],
            "workflow: unexpected second positional argument \"specs/other.md\"",
        ),
        (
            ["workflow", SPEC_REL, "--frob"],
            "workflow: unknown flag \"--frob\"",
        ),
        (
            ["critique", SPEC_REL, "--frob"],
            "critique: unknown flag \"--frob\"",
        ),
    ] {
        let (out, err, ok) = run_rigger_envs(root, &args, &envs);
        assert_eq!(
            (out.as_str(), err.as_str(), ok),
            (
                "",
                (common::cli::refusal_line(refusal) + "\n").as_str(),
                false
            ),
            "{args:?} refuses through the shared rule, naming its verb"
        );
    }
    assert_eq!(
        rigger_dir_names(root),
        before,
        "a refused argument list writes nothing into the project"
    );
    assert!(!scratch.exists(), "nor anything under the scratch root");
    assert_eq!(critique_stub_spawns(work.path()), 0, "no critic is spawned");
}

/// Given the critique stream already holds a result for the critic's attempt-0 spawn that is no
/// critique - a concurrent call that minted the same spawn id recorded it first - when the spec is
/// critiqued, then the headless host answers the critic's attempt-0 spawn from that result and
/// runs no session, and the verb judges what it reads back from the store: it says why that result
/// is no critique, exits non-zero and copies nothing. The next call counts the parked request and
/// critiques at attempt 1.
#[test]
fn a_result_recorded_first_for_the_critics_spawn_id_is_read_back_never_the_sessions_own() {
    let dir = temp_project();
    let root = dir.path();
    scaffold(root, CRITIC_WORKFLOW);
    let scratch = root.join("scratch");
    let first = critique_spawn_id(SPEC_HASH, 0);
    let raced = json!({"id": first, "output": NO_VERDICT});
    seed_critique(root, &[(TYPE_SPAWN_RESULT, raced.clone())]);

    // When the spec is critiqued by a critic whose session would answer with a real reject ...
    let (session, session_path) = stub(REJECT);
    let (out, err, ok) = critique(root, SPEC_REL, &session_path, &scratch);

    // ... the result recorded first answers attempt 0, so no session runs.
    assert_eq!(
        critique_stub_spawns(session.path()),
        0,
        "the recorded result answers the spawn"
    );
    assert!(
        err.contains(&format!(
            "rigger critique: {SPEC_REL} (hash {SPEC_HASH}): critiquing with critic at attempt 0"
        )),
        "no request was recorded, so the spawn runs at attempt 0:\n{err}"
    );
    assert!(!ok, "the result read back is no critique");
    assert!(
        err.contains(&format!(
            "rigger critique: {SPEC_REL} (hash {SPEC_HASH}): no critique was recorded - the \
             critic's output carries no verdict line"
        )),
        "the why is the recorded result's, not the session's reject:\n{err}"
    );
    assert_eq!(out, "", "the session's own findings are never printed");
    let recorded = critique_payloads(root);
    assert_eq!(
        recorded
            .iter()
            .map(|(t, d)| (t.as_str(), d["id"].as_str().unwrap()))
            .collect::<Vec<_>>(),
        [
            (TYPE_SPAWN_RESULT, first.as_str()),
            (TYPE_SPAWN_REQUESTED, first.as_str()),
        ],
        "the attempt-0 request is parked and the session's result is never recorded beside the \
         first one"
    );
    assert_eq!(
        recorded[0].1, raced,
        "the first recorded result stands unchanged"
    );
    assert_eq!(review_findings(root), Vec::<Value>::new(), "nothing copied");

    // When the unchanged text is critiqued again, the parked request counts: attempt 1 runs.
    let (next, next_path) = stub(REJECT);
    let (out, err, ok) = critique(root, SPEC_REL, &next_path, &scratch);
    assert!(ok, "attempt 1 records a critique; stderr:\n{err}");
    assert_eq!(out, reject_out(SPEC, 1));
    assert_eq!(
        critique_stub_spawns(next.path()),
        2,
        "attempt 1's reject, then attempt 2's pass that adds nothing"
    );
    assert_eq!(
        spawn_events(&critique_events(root))[2..],
        [
            (
                TYPE_SPAWN_REQUESTED.to_string(),
                critique_spawn_id(SPEC_HASH, 1)
            ),
            (
                TYPE_SPAWN_RESULT.to_string(),
                critique_spawn_id(SPEC_HASH, 1)
            ),
            (
                TYPE_SPAWN_REQUESTED.to_string(),
                critique_spawn_id(SPEC_HASH, 2)
            ),
            (
                TYPE_SPAWN_RESULT.to_string(),
                critique_spawn_id(SPEC_HASH, 2)
            ),
        ]
    );
    assert_eq!(review_findings(root), reject_copies(SPEC, 1, SPEC_REL));
}

/// Given the critique stream holds a spawn request that cannot be read - its attempt cannot be
/// told apart - when a spec with no critique is critiqued, then the attempt count fails rather
/// than reuse an attempt id: the verb exits non-zero naming the unreadable request, parks no
/// request, spawns no critic and copies nothing.
#[test]
fn an_unreadable_request_on_the_critique_stream_fails_the_attempt_count_and_spawns_nothing() {
    let dir = temp_project();
    let root = dir.path();
    scaffold(root, CRITIC_WORKFLOW);
    seed_critique(
        root,
        &[(TYPE_SPAWN_REQUESTED, json!("not a spawn request"))],
    );
    let (work, path) = stub(REJECT);
    let (out, err, ok) = critique(root, SPEC_REL, &path, &root.join("scratch"));
    assert!(!ok, "an unreadable request fails the count");
    assert!(
        err.contains(&common::cli::refusal_line(
            common::cli::UNREADABLE_CRITIQUE_REQUEST
        )),
        "the verb names the unreadable request:\n{err}"
    );
    assert!(
        !err.contains("critiquing with"),
        "no attempt is chosen, so no critic is announced:\n{err}"
    );
    assert_eq!(out, "");
    assert_eq!(critique_stub_spawns(work.path()), 0, "no critic is spawned");
    assert_eq!(
        critique_events(root).len(),
        1,
        "no request is parked beside the unreadable one"
    );
    assert_eq!(review_findings(root), Vec::<Value>::new());
}

/// Given a spawn request on the critique stream turns unreadable while a pass is in flight, when
/// that pass leaves the critique exhausted, then the call still fails on the attempt count it
/// reads with the re-read union to settle it (`Critic::settles`), as a call that starts on that
/// stream and a new run's refusal do: it exits non-zero naming the unreadable request and prints
/// no answer.
#[test]
fn a_request_made_unreadable_during_an_exhausting_pass_fails_the_call() {
    let dir = temp_project();
    let root = dir.path().to_path_buf();
    scaffold(&root, CRITIC_WORKFLOW);
    let (work, path) = stub(CLEAN_CRITIQUE);
    let corrupt = {
        let root = root.clone();
        move || {
            seed_critique(
                &root,
                &[(TYPE_SPAWN_REQUESTED, json!("not a spawn request"))],
            )
        }
    };
    let held = hold_critique_stub(work.path(), corrupt);
    let (out, err, ok) = critique(&root, SPEC_REL, &path, &root.join("scratch"));
    held.join().expect("the pass was held and released");
    assert_eq!(critique_stub_spawns(work.path()), 1, "the one pass ran");
    assert!(
        !ok,
        "the count read after the exhausting pass fails:\n{err}"
    );
    assert!(
        err.contains(&common::cli::refusal_line(
            common::cli::UNREADABLE_CRITIQUE_REQUEST
        )),
        "the verb names the unreadable request:\n{err}"
    );
    assert_eq!(out, "", "no answer is printed");
}

/// Given two calls that raced on one text minted one spawn id, so the critique stream holds two
/// attempt-0 requests and no result, when the unchanged text is critiqued again, then the next
/// attempt is the number of requests recorded for the hash, 2, never the number of distinct ids.
#[test]
fn two_requests_parked_under_one_spawn_id_make_the_next_attempt_two() {
    let dir = temp_project();
    let root = dir.path();
    scaffold(root, CRITIC_WORKFLOW);
    let first = critique_spawn_id(SPEC_HASH, 0);
    let request = json!({
        "id": first,
        "unit": format!("critique-{SPEC_HASH}"),
        "stage": "critique",
        "prompt": "p",
    });
    seed_critique(
        root,
        &[
            (TYPE_SPAWN_REQUESTED, request.clone()),
            (TYPE_SPAWN_REQUESTED, request),
        ],
    );
    let (work, path) = stub(REJECT);
    let (out, err, ok) = critique(root, SPEC_REL, &path, &root.join("scratch"));
    assert!(ok, "attempt 2 records a critique; stderr:\n{err}");
    assert_eq!(out, reject_out(SPEC, 2));
    assert_eq!(critique_stub_spawns(work.path()), 1);
    assert_eq!(
        spawn_events(&critique_events(root)),
        [
            (TYPE_SPAWN_REQUESTED.to_string(), first.clone()),
            (TYPE_SPAWN_REQUESTED.to_string(), first),
            (
                TYPE_SPAWN_REQUESTED.to_string(),
                critique_spawn_id(SPEC_HASH, 2)
            ),
            (
                TYPE_SPAWN_RESULT.to_string(),
                critique_spawn_id(SPEC_HASH, 2)
            ),
        ]
    );
    assert_eq!(review_findings(root), reject_copies(SPEC, 2, SPEC_REL));
}

/// Given one spawn id parked twice on the project's run stream with different prompts, another
/// spawn parked between the two parks, when a worker fetches each spawn's prompt, then the run's
/// fold over the one per-event request decode answers the re-parked id with the request parked
/// last and the other spawn with its own: the critique count reads every park, while the run's
/// fold collapses a re-parked id to its last request.
#[test]
fn a_spawn_id_parked_twice_on_the_run_answers_with_the_request_parked_last() {
    let dir = temp_project();
    let root = dir.path();
    let parked = |id: &str, prompt: &str| {
        json!({"id": id, "unit": "u", "stage": "implement", "prompt": prompt}).to_string()
    };
    let first = parked("u/implementer#0", "the first park");
    let other = parked("u/sdet-author#0", "the other spawn");
    let last = parked("u/implementer#0", "the last park");
    seed_run_events(
        root,
        &[
            (TYPE_SPAWN_REQUESTED, &first),
            (TYPE_SPAWN_REQUESTED, &other),
            (TYPE_SPAWN_REQUESTED, &last),
        ],
    );
    assert_eq!(
        run_rigger_ok(root, &["prompt", "u/implementer#0"]),
        "the last park\n",
        "a re-parked id answers with the request written last"
    );
    assert_eq!(
        run_rigger_ok(root, &["prompt", "u/sdet-author#0"]),
        "the other spawn\n",
        "a spawn parked between the two parks keeps its own request"
    );
}

/// Given a project with no git repository holding critique-named liveness and transcript
/// directories under its root and under `RIGGER_TMPDIR`, when its spec is critiqued, then the
/// critique records with an empty scratch root and its directory removal is a no-op: every planted
/// directory stays, and nothing new is written under either base.
#[test]
fn a_project_with_no_git_repository_removes_no_critique_directory_under_its_root_or_tmpdir() {
    let dir = temp_repoless_project();
    let root = dir.path();
    scaffold(root, CRITIC_WORKFLOW);
    let tmpdir = root.join("scratch");
    let mut planted = Vec::new();
    for base in [root.to_path_buf(), tmpdir.clone()] {
        for sub in ["agent-live", "agent-stream"] {
            let run = base.join(sub).join(format!("critique-{SPEC_HASH}"));
            std::fs::create_dir_all(&run).unwrap();
            std::fs::write(run.join("left"), "").unwrap();
            planted.push(run);
        }
    }
    let (work, path) = stub(REJECT);
    let (out, err, ok) = critique(root, SPEC_REL, &path, &tmpdir);
    assert!(ok, "a repo-less project critiques; stderr:\n{err}");
    assert_eq!(out, reject_out(SPEC, 0));
    assert_eq!(
        critique_stub_spawns(work.path()),
        2,
        "the reject, then the pass that adds nothing"
    );
    for run in &planted {
        let entries: Vec<std::ffi::OsString> = std::fs::read_dir(run)
            .unwrap_or_else(|e| panic!("{} stays: {e}", run.display()))
            .flatten()
            .map(|entry| entry.file_name())
            .collect();
        assert_eq!(
            entries,
            [std::ffi::OsString::from("left")],
            "{} is never a repo-less critique's to remove or write into",
            run.display()
        );
    }
}

/// Given a project whose `.rigger/store.conn` selects a KurrentDB server, with no flag and no
/// `KURRENTDB_CONN`, when its spec is critiqued, then the critic is handed that configured server
/// as `KURRENTDB_CONN`; and given an ambient `KURRENTDB_CONN` naming another address, when a spec
/// is critiqued on a server the `--conn` flag selects, then the critic is handed the flagged
/// server over the ambient one - whichever rung selected the server, the critic resolves the store
/// the critique is recorded to.
#[test]
fn a_configured_server_and_a_flagged_one_over_the_ambient_are_both_handed_to_the_critic() {
    common::fixtures::with_kurrentdb(|conn| {
        // A server the per-machine secret file selects.
        let configured = temp_project();
        let root = configured.path();
        scaffold(root, CRITIC_WORKFLOW);
        let secret = rigger_file(root, "store.conn");
        std::fs::write(&secret, conn).unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        let (work, path) = stub(REJECT);
        let scratch = root.join("scratch");
        let (out, err, ok) = run_rigger_envs(
            root,
            &["critique", SPEC_REL],
            &[
                ("PATH", &path),
                ("RIGGER_TMPDIR", scratch.to_str().unwrap()),
                ("KURRENTDB_CONN", ""),
            ],
        );
        assert!(
            ok,
            "the critique records on the configured server; stderr:\n{err}"
        );
        assert_eq!(out, reject_out(SPEC, 0));
        assert!(
            !rigger_file(root, "events.db").exists(),
            "the configured server holds the critique, never a local store"
        );
        assert_eq!(
            critique_stub_record(work.path(), CRITIQUE_CONN),
            conn,
            "the critic is handed the server the configuration selected"
        );

        // A server the flag selects, over an ambient KURRENTDB_CONN naming another address.
        let flagged = temp_project();
        let root = flagged.path();
        scaffold(root, CRITIC_WORKFLOW);
        let (work, path) = stub(REJECT);
        let scratch = root.join("scratch");
        let (out, err, ok) = run_rigger_envs(
            root,
            &["critique", SPEC_REL, "--conn", conn],
            &[
                ("PATH", &path),
                ("RIGGER_TMPDIR", scratch.to_str().unwrap()),
                ("KURRENTDB_CONN", "kurrentdb://127.0.0.1:1/?tls=false"),
            ],
        );
        assert!(
            ok,
            "the critique records on the flagged server; stderr:\n{err}"
        );
        assert_eq!(out, reject_out(SPEC, 0));
        assert_eq!(
            critique_stub_spawns(work.path()),
            2,
            "the reject, then the pass that adds nothing"
        );
        assert_eq!(
            critique_stub_record(work.path(), CRITIQUE_CONN),
            conn,
            "the handed selection outranks the ambient KURRENTDB_CONN the critic inherits"
        );
    });
}

// ---------------------------------------------------------------------------------------------
// Spec 123, criterion 2 (THE CRITIQUE PASSES TO EXHAUSTION): the loop, the recorded findings in
// each later pass's prompt, the ceiling and its notice, and the per-spawn reap.
// ---------------------------------------------------------------------------------------------

/// The CEILING NOTICE's close for a hash with at least one pass and no open BLOCKING finding.
const CLEAN_AT_THE_CEILING: &str = "no open BLOCKING finding, the critique is clean at the ceiling";

/// The CEILING NOTICE's close for a hash with no pass.
const NO_PASS_RECORDED: &str = "no pass recorded: raise the ceiling or edit the text";

/// The bound a ceiling read from the plan-critique gate's own `max_retries` names.
const GATE_BOUND: &str = "the ceiling of stages.plan-critique.max_retries";

/// [`CRITIC_WORKFLOW`] with its plan-critique gate's `max_retries` set to `max`.
fn gate_ceiling(max: u32) -> String {
    format!("{CRITIC_WORKFLOW}    max_retries: {max}\n")
}

/// The CEILING NOTICE past its [`verb_head`]: not exhausted after `attempts` attempts, `bound`,
/// then `close`.
fn ceiling_notice(attempts: u32, bound: &str, close: &str) -> String {
    format!("not exhausted after {attempts} attempts, {bound}: {close}")
}

/// The CEILING NOTICE's close naming the open BLOCKING finding `ids`.
fn open_blocking(ids: &[String]) -> String {
    format!(
        "open BLOCKING findings: {}: amend the spec or record a DecisionMade resolving them",
        ids.join(", ")
    )
}

/// The critique prompt of [`SPEC`] at [`SPEC_REL`] carrying the `recorded` finding summaries, one
/// line each in order, then a blank line, between the output contract and the spec text.
fn prompt_recording(recorded: &[&str]) -> String {
    let lines: String = recorded
        .iter()
        .map(|summary| format!("{summary}\n"))
        .collect();
    spec_critique_prompt(SPEC_REL, SPEC, []).replacen(
        "The spec under critique, ",
        &format!("{CRITIQUE_RECORDED_FINDINGS}\n{lines}\nThe spec under critique, "),
        1,
    )
}

/// The `(id, prompt, model)` of each request on the critique stream, oldest first.
fn requests(root: &Path) -> Vec<(String, Value, Value)> {
    critique_payloads(root)
        .into_iter()
        .filter(|(type_, _)| type_ == TYPE_SPAWN_REQUESTED)
        .map(|(_, request)| {
            (
                request["id"].as_str().unwrap().to_string(),
                request["prompt"].clone(),
                request["model"].clone(),
            )
        })
        .collect()
}

/// The last line of `err`: the error `rigger` exits on, behind its `rigger: ` prefix.
fn last_line(err: &str) -> &str {
    err.lines().last().unwrap_or_default()
}

/// Spec 123, THE LOOP: given a hash whose one recorded pass, a reject, is unexhausted and below
/// the ceiling, when its unchanged text is critiqued, then the critic is spawned again at the next
/// attempt with the recorded findings in its prompt, and again with the union so far, until a pass
/// carries no finding line: each pass on the persona's rung for its attempt, every finding of the
/// union copied, and the union printed with the verdict it derives, with no notice.
#[test]
fn the_critic_runs_again_with_the_recorded_findings_until_a_pass_adds_none() {
    let dir = temp_project();
    let root = dir.path();
    scaffold(root, CRITIC_WORKFLOW);
    seed_critique_spawn(root, SPEC_HASH, 0, REJECT);
    let (work, path) = stub(ADVISORY);
    let (out, err, ok) = critique(root, SPEC_REL, &path, &root.join("scratch"));

    assert!(
        ok,
        "the loop stops on a pass that adds nothing; stderr:\n{err}"
    );
    assert_eq!(
        critique_stub_spawns(work.path()),
        2,
        "attempt 1's advisory, then attempt 2's pass that adds nothing"
    );
    assert_eq!(
        verb_lines(&err),
        [
            "critiquing with critic at attempt 1",
            "critiquing with critic at attempt 2"
        ],
        "an exhausted union stops the loop with no notice:\n{err}"
    );
    let rejected = reject_copies(SPEC, 0, SPEC_REL);
    let mut recorded: Vec<&str> = rejected
        .iter()
        .map(|copy| copy["summary"].as_str().unwrap())
        .collect();
    let second = prompt_recording(&recorded);
    recorded.push(ADVISORY_SUMMARY);
    assert_eq!(
        requests(root),
        [
            (critique_spawn_id(SPEC_HASH, 0), json!("p"), Value::Null),
            (
                critique_spawn_id(SPEC_HASH, 1),
                json!(second),
                json!("opus")
            ),
            (
                critique_spawn_id(SPEC_HASH, 2),
                json!(prompt_recording(&recorded)),
                json!("opus")
            ),
        ],
        "each pass carries the union read just before it, on the persona's rung for its attempt"
    );
    assert_eq!(
        critique_stub_task(work.path()),
        framed(&second),
        "the critic's task is the prompt carrying the recorded findings, under the host's frame"
    );
    let every_spawn: Vec<(String, String)> = (0..3)
        .flat_map(|attempt| {
            [TYPE_SPAWN_REQUESTED, TYPE_SPAWN_RESULT]
                .map(|type_| (type_.to_string(), critique_spawn_id(SPEC_HASH, attempt)))
        })
        .collect();
    assert_eq!(spawn_events(&critique_events(root)), every_spawn);
    assert_eq!(
        out,
        format!(
            "{}{} | {ADVISORY_SUMMARY}\n{{\"verdict\":\"reject\"}}\n",
            reject_out(SPEC, 0)
                .strip_suffix("{\"verdict\":\"reject\"}\n")
                .unwrap(),
            critique_finding_id(SPEC, 1, 1)
        ),
        "the union in pass order, then the reject its BLOCKING line derives"
    );
    let mut copies = rejected;
    copies.push(advisory_copy(SPEC, 1));
    assert_eq!(review_findings(root), copies, "every finding of the union");
}

/// Spec 123, THE LOOP: given a hash whose recorded pass was never copied - recorded by an earlier
/// call, or by this call's own first spawn - when the next spawn records no pass, then that pass
/// was copied before the spawn: the verb exits non-zero saying why, prints nothing on stdout, and
/// the run stream holds the recorded pass's copies.
#[test]
fn a_recorded_pass_is_copied_before_the_next_spawn_so_a_failing_spawn_leaves_it_copied() {
    struct Row {
        name: &'static str,
        /// Whether an earlier call recorded attempt 0's [`REJECT`] pass.
        seeded: bool,
        /// The critic's answers: its first spawn's, then every later spawn's.
        answers: (&'static str, &'static str),
        spawns: usize,
        verb_lines: &'static [&'static str],
    }
    for row in [
        Row {
            name: "a pass an earlier call recorded",
            seeded: true,
            answers: (NO_VERDICT, CLEAN_CRITIQUE),
            spawns: 1,
            verb_lines: &["critiquing with critic at attempt 1"],
        },
        Row {
            name: "a pass this call recorded",
            seeded: false,
            answers: (REJECT, NO_VERDICT),
            spawns: 2,
            verb_lines: &[
                "critiquing with critic at attempt 0",
                "critiquing with critic at attempt 1",
            ],
        },
    ] {
        let name = row.name;
        let dir = temp_project();
        let root = dir.path();
        scaffold(root, CRITIC_WORKFLOW);
        if row.seeded {
            seed_critique_spawn(root, SPEC_HASH, 0, REJECT);
        }
        let (work, path) = stub_then(row.answers.0, row.answers.1);
        let (out, err, ok) = critique(root, SPEC_REL, &path, &root.join("scratch"));

        assert!(!ok, "{name}: a spawn that records no pass fails the call");
        assert_eq!(out, "", "{name}: nothing is printed on stdout");
        assert_eq!(verb_lines(&err), row.verb_lines, "{name}: {err}");
        assert_eq!(
            last_line(&err),
            format!(
                "rigger: {}no critique was recorded - the critic's output carries no verdict line",
                verb_head()
            ),
            "{name}"
        );
        assert_eq!(critique_stub_spawns(work.path()), row.spawns, "{name}");
        assert_eq!(
            spawn_events(&critique_events(root)),
            [0, 1]
                .map(|attempt| critique_spawn_id(SPEC_HASH, attempt))
                .iter()
                .flat_map(|id| [
                    (TYPE_SPAWN_REQUESTED.to_string(), id.clone()),
                    (TYPE_SPAWN_RESULT.to_string(), id.clone()),
                ])
                .collect::<Vec<_>>(),
            "{name}"
        );
        assert_eq!(
            review_findings(root),
            reject_copies(SPEC, 0, SPEC_REL),
            "{name}: the recorded pass was copied before the spawn that failed"
        );
    }
}

/// Spec 123, THE CEILING: given a critic whose ceiling is read from the plan-critique gate's own
/// `max_retries`, from `defaults.max_retries` under a `defaults.review` critic, or from the built-in
/// bound, when a pass leaves the hash unexhausted with its attempt count at the ceiling, then the
/// loop stops after that pass with the CEILING NOTICE naming the bound and either the open BLOCKING
/// ids or a clean union; and the next call on the unchanged text spawns nothing, says it answered
/// from the record, prints the notice again with the same union, and appends nothing.
#[test]
fn a_pass_at_the_ceiling_stops_the_loop_with_the_notice_and_the_next_call_answers() {
    struct Row {
        name: &'static str,
        workflow: String,
        /// The attempts seeded with a result that is no pass.
        seeded: &'static [u32],
        answer: &'static str,
        attempt: u32,
        out: String,
        notice: String,
        copies: Vec<Value>,
    }
    let open_at = |attempt| open_blocking(&[critique_finding_id(SPEC, attempt, 1)]);
    for row in [
        Row {
            name: "the gate's own max_retries",
            workflow: gate_ceiling(1),
            seeded: &[],
            answer: REJECT,
            attempt: 0,
            out: reject_out(SPEC, 0),
            notice: ceiling_notice(1, GATE_BOUND, &open_at(0)),
            copies: reject_copies(SPEC, 0, SPEC_REL),
        },
        Row {
            name: "the defaults' max_retries under a defaults.review critic",
            workflow: DEFAULT_ADVERSARY_WORKFLOW.replacen(
                "defaults:\n",
                "defaults:\n  max_retries: 1\n",
                1,
            ),
            seeded: &[],
            answer: REJECT,
            attempt: 0,
            out: reject_out(SPEC, 0),
            notice: ceiling_notice(1, "the ceiling of defaults.max_retries", &open_at(0)),
            copies: reject_copies(SPEC, 0, SPEC_REL),
        },
        Row {
            name: "the built-in bound, two attempts with no pass counted",
            workflow: CRITIC_WORKFLOW.to_string(),
            seeded: &[0, 1],
            answer: REJECT,
            attempt: 2,
            out: reject_out(SPEC, 2),
            notice: ceiling_notice(3, "the built-in bound of 3", &open_at(2)),
            copies: reject_copies(SPEC, 2, SPEC_REL),
        },
        Row {
            name: "a union holding only a NON-BLOCKING finding",
            workflow: gate_ceiling(1),
            seeded: &[],
            answer: ADVISORY,
            attempt: 0,
            out: format!(
                "{} | {ADVISORY_SUMMARY}\n{{\"verdict\":\"approve\"}}\n",
                critique_finding_id(SPEC, 0, 1)
            ),
            notice: ceiling_notice(1, GATE_BOUND, CLEAN_AT_THE_CEILING),
            copies: vec![advisory_copy(SPEC, 0)],
        },
    ] {
        let name = row.name;
        let dir = temp_project();
        let root = dir.path();
        scaffold(root, &row.workflow);
        for attempt in row.seeded {
            seed_critique_spawn(root, SPEC_HASH, *attempt, NO_VERDICT);
        }
        let scratch = root.join("scratch");

        let (work, path) = stub(row.answer);
        let (out, err, ok) = critique(root, SPEC_REL, &path, &scratch);
        assert!(ok, "{name}: a pass at the ceiling exits 0; stderr:\n{err}");
        assert_eq!(critique_stub_spawns(work.path()), 1, "{name}: one pass");
        assert_eq!(out, row.out, "{name}: the union and its verdict");
        assert_eq!(
            verb_lines(&err),
            [
                format!("critiquing with critic at attempt {}", row.attempt),
                row.notice.clone()
            ],
            "{name}: the loop stops after the pass with the notice"
        );
        assert_eq!(review_findings(root), row.copies, "{name}: the copies");
        let events = critique_events(root).len();
        assert_eq!(
            events,
            2 * row.seeded.len() + 2,
            "{name}: the seeded attempts and the one pass"
        );

        let (again, again_path) = stub(row.answer);
        let (out, err, ok) = critique(root, SPEC_REL, &again_path, &scratch);
        assert!(ok, "{name}: an answered call exits 0; stderr:\n{err}");
        assert_eq!(critique_stub_spawns(again.path()), 0, "{name}: zero spawns");
        assert_eq!(out, row.out, "{name}: the same union and verdict");
        assert_eq!(
            verb_lines(&err),
            [answered("1 pass"), row.notice],
            "{name}: answered from the record, then the notice"
        );
        assert_eq!(
            critique_events(root).len(),
            events,
            "{name}: nothing appended"
        );
        assert_eq!(review_findings(root), row.copies, "{name}: no copy added");
    }
}

/// Spec 123, THE CEILING: given a hash at its ceiling whose notice names an open BLOCKING id, when
/// a `DecisionMade` governing the spec resolves that id, then the next call answers from the record
/// with the notice saying the critique is clean at the ceiling, printing the same union.
#[test]
fn a_resolution_turns_the_ceiling_notice_clean() {
    let dir = temp_project();
    let root = dir.path();
    scaffold(root, &gate_ceiling(1));
    let scratch = root.join("scratch");
    let (_first, first_path) = stub(REJECT);
    let (_out, err, ok) = critique(root, SPEC_REL, &first_path, &scratch);
    assert!(ok, "the pass records; stderr:\n{err}");
    let blocking = critique_finding_id(SPEC, 0, 1);
    assert_eq!(
        verb_lines(&err),
        [
            "critiquing with critic at attempt 0".to_string(),
            ceiling_notice(
                1,
                GATE_BOUND,
                &open_blocking(std::slice::from_ref(&blocking))
            )
        ]
    );

    resolve(root, "r-c1", SPEC_REL, &[blocking]);
    let (again, again_path) = stub(REJECT);
    let (out, err, ok) = critique(root, SPEC_REL, &again_path, &scratch);
    assert!(ok, "an answered call exits 0; stderr:\n{err}");
    assert_eq!(critique_stub_spawns(again.path()), 0, "zero spawns");
    assert_eq!(
        out,
        reject_out(SPEC, 0),
        "the union is printed whatever resolves it"
    );
    assert_eq!(
        verb_lines(&err),
        [
            answered("1 pass"),
            ceiling_notice(1, GATE_BOUND, CLEAN_AT_THE_CEILING)
        ]
    );
}

/// Spec 123, THE CEILING: given a hash whose one attempt brought no pass and whose attempt count is
/// at the ceiling, when its unchanged text is critiqued, then the verb is not answered: it spawns
/// nothing, exits non-zero on the CEILING NOTICE naming no pass, and prints and copies nothing.
/// When the key is raised, the next call continues at the next attempt and stops at the raised
/// ceiling; when it is lowered below the attempt count, the ceiling reads as reached.
#[test]
fn a_hash_with_no_pass_at_the_ceiling_is_refused_until_the_key_is_raised() {
    let dir = temp_project();
    let root = dir.path();
    scaffold(root, &gate_ceiling(1));
    let scratch = root.join("scratch");
    let (_silent, silent_path) = stub(NO_VERDICT);
    let (_out, err, ok) = critique(root, SPEC_REL, &silent_path, &scratch);
    assert!(!ok, "attempt 0 records no pass; stderr:\n{err}");

    let (refused, refused_path) = stub(REJECT);
    let (out, err, ok) = critique(root, SPEC_REL, &refused_path, &scratch);
    assert!(!ok, "a hash with no pass at its ceiling is not answered");
    assert_eq!(out, "", "nothing is printed on stdout");
    assert_eq!(critique_stub_spawns(refused.path()), 0, "zero spawns");
    assert_eq!(
        verb_lines(&err),
        Vec::<&str>::new(),
        "nothing is announced or answered:\n{err}"
    );
    assert_eq!(
        last_line(&err),
        format!(
            "rigger: {}{}",
            verb_head(),
            ceiling_notice(1, GATE_BOUND, NO_PASS_RECORDED)
        )
    );
    assert_eq!(
        critique_events(root).len(),
        2,
        "attempt 0's request and result, nothing appended"
    );
    assert_eq!(review_findings(root), Vec::<Value>::new());

    // When the key is raised, the next call continues at attempt 1 and stops at the new ceiling.
    std::fs::write(rigger_file(root, "workflow.yml"), gate_ceiling(2)).unwrap();
    let (raised, raised_path) = stub(REJECT);
    let (out, err, ok) = critique(root, SPEC_REL, &raised_path, &scratch);
    assert!(ok, "the raised ceiling lets attempt 1 run; stderr:\n{err}");
    assert_eq!(critique_stub_spawns(raised.path()), 1, "one pass");
    assert_eq!(out, reject_out(SPEC, 1));
    let notice = ceiling_notice(
        2,
        GATE_BOUND,
        &open_blocking(&[critique_finding_id(SPEC, 1, 1)]),
    );
    assert_eq!(
        verb_lines(&err),
        [
            "critiquing with critic at attempt 1".to_string(),
            notice.clone()
        ]
    );

    // When the key is lowered below the attempt count, the ceiling reads as reached.
    std::fs::write(rigger_file(root, "workflow.yml"), gate_ceiling(1)).unwrap();
    let (lowered, lowered_path) = stub(REJECT);
    let (out, err, ok) = critique(root, SPEC_REL, &lowered_path, &scratch);
    assert!(ok, "an answered call exits 0; stderr:\n{err}");
    assert_eq!(critique_stub_spawns(lowered.path()), 0, "zero spawns");
    assert_eq!(out, reject_out(SPEC, 1));
    assert_eq!(verb_lines(&err), [answered("1 pass"), notice]);
}

/// Spec 123, THE LOOP's reap: given a scratch root holding another spawn of the same hash, whose
/// transcript's name opens with attempt 1's sanitized leaf, and a live marker of another hash, when
/// the text is critiqued over two passes, then each judged spawn's marker, transcripts and spawn
/// scratch go, and every other entry stays, with the hash's directories that still hold them.
#[test]
fn a_spawning_call_reaps_only_the_spawns_it_judged() {
    let dir = temp_project();
    let root = dir.path();
    scaffold(root, CRITIC_WORKFLOW);
    let scratch = root.join("scratch");
    let base = scratch.to_str().unwrap();
    let unit = critique_unit(SPEC_HASH);
    let other = critique_spawn_id(SPEC_HASH, 10);
    let elsewhere = "fedcba9876543210";
    let transcript = scratch_subpath(base, AGENT_STREAM_SUBDIR, &unit, &other).unwrap();
    let mut planted = vec![
        marker_path(base, &unit, &other).unwrap(),
        std::path::PathBuf::from(format!("{}.0.jsonl", transcript.to_str().unwrap())),
        spawn_scratch_path(base, &unit, &other)
            .unwrap()
            .join("left"),
        marker_path(
            base,
            &critique_unit(elsewhere),
            &critique_spawn_id(elsewhere, 0),
        )
        .unwrap(),
    ];
    for file in &planted {
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, "").unwrap();
    }

    let (work, path) = stub(REJECT);
    let (out, err, ok) = critique(root, SPEC_REL, &path, &scratch);
    assert!(ok, "the critique records; stderr:\n{err}");
    assert_eq!(out, reject_out(SPEC, 0));
    assert_eq!(
        critique_stub_spawns(work.path()),
        2,
        "the reject, then the pass that adds nothing"
    );
    let mut left = Vec::new();
    for area in ["agent-live", "agent-scratch", "agent-stream"] {
        collect_files(&scratch.join(area), &mut left);
    }
    planted.sort();
    assert_eq!(
        left, planted,
        "only the judged spawns' entries go; another spawn's and another hash's stay"
    );
}

/// Spec 123, THE LOOP and THE CEILING: given a fresh hash under the built-in bound of 3 and a
/// critic that restates its findings on every pass, when the text is critiqued, then the one call
/// spawns the critic pass after pass, each on the persona's rung for its attempt with the union
/// read just before it in its prompt, each restated line kept under a new id, until the attempt
/// count reaches the ceiling: the loop stops after the third pass with the notice naming every
/// open BLOCKING id, and prints the union with the reject it derives.
#[test]
fn a_critic_restating_its_findings_runs_pass_after_pass_until_the_ceiling() {
    let dir = temp_project();
    let root = dir.path();
    scaffold(root, CRITIC_WORKFLOW);
    let (work, path) = stub_then(REJECT, REJECT);
    let (out, err, ok) = critique(root, SPEC_REL, &path, &root.join("scratch"));

    assert!(ok, "a pass at the ceiling exits 0; stderr:\n{err}");
    assert_eq!(
        critique_stub_spawns(work.path()),
        3,
        "one pass per attempt, up to the ceiling"
    );
    let open: Vec<String> = (0..3)
        .map(|attempt| critique_finding_id(SPEC, attempt, 1))
        .collect();
    assert_eq!(
        verb_lines(&err),
        [
            "critiquing with critic at attempt 0".to_string(),
            "critiquing with critic at attempt 1".to_string(),
            "critiquing with critic at attempt 2".to_string(),
            ceiling_notice(3, "the built-in bound of 3", &open_blocking(&open)),
        ],
        "every pass in the one call, then the notice at the ceiling:\n{err}"
    );
    let copies: Vec<Value> = (0..3)
        .flat_map(|attempt| reject_copies(SPEC, attempt, SPEC_REL))
        .collect();
    let summaries: Vec<&str> = copies
        .iter()
        .map(|copy| copy["summary"].as_str().unwrap())
        .collect();
    assert_eq!(
        requests(root),
        [
            (
                critique_spawn_id(SPEC_HASH, 0),
                json!(spec_critique_prompt(SPEC_REL, SPEC, [])),
                json!("sonnet")
            ),
            (
                critique_spawn_id(SPEC_HASH, 1),
                json!(prompt_recording(&summaries[..2])),
                json!("opus")
            ),
            (
                critique_spawn_id(SPEC_HASH, 2),
                json!(prompt_recording(&summaries[..4])),
                json!("opus")
            ),
        ],
        "each pass carries the union read just before it, restated lines included, on the \
         persona's rung for its attempt"
    );
    let verdict = "{\"verdict\":\"reject\"}\n";
    let lines: String = (0..3)
        .map(|attempt| {
            reject_out(SPEC, attempt)
                .strip_suffix(verdict)
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(
        out,
        format!("{lines}{verdict}"),
        "the union in pass order, then the reject its BLOCKING lines derive"
    );
    assert_eq!(
        review_findings(root),
        copies,
        "every finding of the union, each restated one under its own id"
    );
}

/// Spec 123, THE CEILING: given an exhausted hash whose attempt count is below, at or above the
/// ceiling, when its unchanged text is critiqued, then the verb answers from the record: it spawns
/// nothing, its one line says it answered, with no CEILING NOTICE, it prints the empty union with
/// its approve, and it appends and copies nothing.
#[test]
fn an_exhausted_critique_is_answered_with_no_notice_wherever_its_attempts_stand() {
    struct Row {
        name: &'static str,
        max_retries: u32,
        /// The attempts seeded with a result that is no pass, before the exhausting one.
        silent: &'static [u32],
        exhausting: u32,
    }
    for row in [
        Row {
            name: "below the ceiling",
            max_retries: 2,
            silent: &[],
            exhausting: 0,
        },
        Row {
            name: "at the ceiling",
            max_retries: 1,
            silent: &[],
            exhausting: 0,
        },
        Row {
            name: "above the ceiling",
            max_retries: 1,
            silent: &[0],
            exhausting: 1,
        },
    ] {
        let name = row.name;
        let dir = temp_project();
        let root = dir.path();
        scaffold(root, &gate_ceiling(row.max_retries));
        for attempt in row.silent {
            seed_critique_spawn(root, SPEC_HASH, *attempt, NO_VERDICT);
        }
        seed_critique_spawn(root, SPEC_HASH, row.exhausting, CLEAN_CRITIQUE);
        let events = critique_events(root).len();

        let (work, path) = stub(REJECT);
        let (out, err, ok) = critique(root, SPEC_REL, &path, &root.join("scratch"));
        assert!(ok, "{name}: an answered call exits 0; stderr:\n{err}");
        assert_eq!(critique_stub_spawns(work.path()), 0, "{name}: zero spawns");
        assert_eq!(
            verb_lines(&err),
            [answered("1 pass")],
            "{name}: answered from the record, with no notice:\n{err}"
        );
        assert_eq!(
            out, "{\"verdict\":\"approve\"}\n",
            "{name}: the empty union and its approve"
        );
        assert_eq!(
            critique_events(root).len(),
            events,
            "{name}: nothing appended"
        );
        assert_eq!(
            review_findings(root),
            Vec::<Value>::new(),
            "{name}: nothing copied"
        );
    }
}
