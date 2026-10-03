//! Spec 112, criterion 2 (A NEW RUN NEEDS A CLEAN CRITIQUE) - periphery: the run entries driven
//! as the compiled binary. Given a project whose workflow names a critic, when a command would
//! begin a new run on a spec whose text has no critique, or whose critique holds a BLOCKING
//! finding no later `DecisionMade` resolves, then it refuses on stderr - naming each open
//! BLOCKING finding id and the two commands that clear it - and appends nothing; a step that
//! adopts the spec's existing run proceeds whatever the critique; and under a workflow naming no
//! critic a new run proceeds with one line saying so.
//!
//! NOT OWNED HERE: the critique verb and its record (criterion 1), and the plan-critique stop on
//! a spec defect (criterion 5).

mod common;

use std::path::Path;

use common::cli::{
    assert_stopped_at_the_grounder, critique_finding_id, emit, printed_decision, read_run_events,
    record_clean_critique, record_critique, refused_new_run, run_payloads, run_rigger,
    run_rigger_envs, seed_run_events, stopping_at_the_grounder, temp_repoless_project,
    write_scaffold, write_spec_project, CRITIQUE_ROUTE, RESOLUTION_ROUTE,
};
use common::fixtures::{git_ok, git_out, temp_git_project_with_commit};
use common::repo::{stub_path, write_critique_stub};
use rigger::wave::NO_CRITIC_CLAUSE;
use serde_json::json;

/// A plan, its plan-critique gate naming `skeptic` as its adversary (the critic), and the
/// fan-out implement template the gate releases.
const SKEPTIC_WORKFLOW: &str = "defaults:\n  grounder: nop\nstages:\n  plan:\n    agent: \
     planner\n    produces: dag\n  plan-critique:\n    needs: [plan]\n    adversary: skeptic\n    \
     adjudicator: arbiter\n  implement:\n    needs: [plan-critique]\n    agent: planner\n    \
     strategy: fan-out\n    on_pass: none\n";

/// [`SKEPTIC_WORKFLOW`] with the gate's adversary dropped and no `defaults.review.adversary`:
/// no critic.
const CRITICLESS_WORKFLOW: &str = "defaults:\n  grounder: nop\nstages:\n  plan:\n    agent: \
     planner\n    produces: dag\n  plan-critique:\n    needs: [plan]\n    adjudicator: arbiter\n  \
     implement:\n    needs: [plan-critique]\n    agent: planner\n    strategy: fan-out\n    \
     on_pass: none\n";

const PLANNER: &str =
    "---\nid: planner\nmodel: sonnet\ntools: [Read]\nisolation: none\n---\nSplit the spec.\n";
const SKEPTIC: &str =
    "---\nid: skeptic\nmodel: sonnet\ntools: [Read]\nisolation: none\n---\nDoubt the spec.\n";
const ARBITER: &str = "---\nid: arbiter\nmodel: sonnet\ntools: [Read]\nisolation: none\n---\n\
     Rule. End with {\"verdict\":\"approve\"}.\n";

const SPEC_REL: &str = "specs/7-gadget.md";
const SPEC: &str = "# 7 - A gadget spec\n\n## Design\n\nThe gadget spins.\n\n## Done when\n\n\
     - [ ] a test proves the gadget spins\n- [ ] a test proves the gadget stops\n\
     - [ ] a test proves the gadget rests\n";
/// [`SPEC`] with its last criterion deleted.
const SPEC_DROPPED: &str = "# 7 - A gadget spec\n\n## Design\n\nThe gadget spins.\n\n## Done \
     when\n\n- [ ] a test proves the gadget spins\n- [ ] a test proves the gadget stops\n";

/// The critic's answer: findings 1 and 3 BLOCKING, finding 2 NON-BLOCKING, a reject.
const REJECT: &str = "Read it.\n\
     S | BLOCKING | criterion 1 | spins is unbounded | bound it in Design\n\
     S | NON-BLOCKING | Design | spins names no surface | name it\n\
     S | BLOCKING | criterion 3 | rests is undecided when empty | decide it in Design\n\
     {\"verdict\":\"reject\"}";

/// The critic's answer: finding 1 NON-BLOCKING, finding 2 BLOCKING, an approve.
const APPROVE_BESIDE_BLOCKING: &str = "Read it.\n\
     S | NON-BLOCKING | Design | spins names no surface | name it\n\
     S | BLOCKING | criterion 2 | stops names no bound | bound it in Design\n\
     {\"verdict\":\"approve\"}";

/// The ids of the two BLOCKING findings [`REJECT`] records as the first critique of [`SPEC`].
fn reject_ids() -> [String; 2] {
    [
        critique_finding_id(SPEC, 0, 1),
        critique_finding_id(SPEC, 0, 3),
    ]
}

/// The personas every workflow here names: a planner, the `skeptic` critic and the `arbiter`.
const PERSONAS: [(&str, &str); 3] = [
    ("planner", PLANNER),
    ("skeptic", SKEPTIC),
    ("arbiter", ARBITER),
];

/// `rigger <args...> --base HEAD` in `root` with extra environment `envs`: (stdout, stderr,
/// success).
fn on_head(root: &Path, args: &[&str], envs: &[(&str, &str)]) -> (String, String, bool) {
    let args: Vec<&str> = args.iter().copied().chain(["--base", "HEAD"]).collect();
    run_rigger_envs(root, &args, envs)
}

/// The run entries besides `rigger step`, each on [`SPEC_REL`] and named by its own command.
const RUN_ENTRIES: [(&[&str], &str); 3] = [
    (&["run", SPEC_REL], "rigger run"),
    (&["serve", SPEC_REL], "rigger serve"),
    (
        &["run", "--driver", "workflow", SPEC_REL],
        "rigger run --driver workflow",
    ),
];

/// A stand-in `claude` and the `PATH` that puts it first, so a run entry that gets past its run
/// start never reaches a real agent; the directory holding it must outlive every call.
fn stand_in_claude() -> (tempfile::TempDir, String) {
    let work = tempfile::tempdir().unwrap();
    let path = stub_path(work.path(), "claude", Some("fake-agent.sh"));
    (work, path)
}

/// [`SPEC`] written outside the repository: the directory holding it, which must outlive every
/// call, and the file's absolute path.
fn spec_outside_the_repository() -> (tempfile::TempDir, String) {
    let elsewhere = tempfile::tempdir().unwrap();
    let outside = elsewhere.path().join("7-gadget.md");
    std::fs::write(&outside, SPEC).unwrap();
    let outside = outside.display().to_string();
    (elsewhere, outside)
}

/// The run entry `args` plus `extra` flags, then `--base HEAD`, in `root` with `path` as its
/// `PATH`: (stdout, stderr, success).
fn run_entry(root: &Path, path: &str, args: &[&str], extra: &[&str]) -> (String, String, bool) {
    let args: Vec<&str> = args.iter().chain(extra).copied().collect();
    on_head(root, &args, &[("PATH", path)])
}

/// `rigger step --spec <spec> --base HEAD` in `root`, plus `extra` flags.
fn step(root: &Path, spec: &str, extra: &[&str]) -> (String, String, bool) {
    let args: Vec<&str> = ["step", "--spec", spec]
        .iter()
        .chain(extra)
        .copied()
        .collect();
    on_head(root, &args, &[])
}

/// The line `command` prints on stderr as it begins a new run, under a workflow naming no critic,
/// on the spec it names `named`.
fn no_critic_line(command: &str, named: &str) -> String {
    format!("{command}: no spec critique for {named}: {NO_CRITIC_CLAUSE}\n")
}

/// `command`'s stderr `err` refused nothing and holds its no-critic line naming `named`
/// ([`no_critic_line`]) `no_critic` times and no other mention of a spec critique.
fn assert_unrefused((command, named): (&str, &str), err: &str, no_critic: usize) {
    assert_eq!(
        (
            err.contains("refusing"),
            err.matches(&no_critic_line(command, named)).count(),
            err.matches("no spec critique").count()
        ),
        (false, no_critic, no_critic),
        "{command} refuses nothing and prints its no-critic line {no_critic} time(s); \
         stderr:\n{err}"
    );
}

/// `command` refused a new run on `spec` with `open`: stderr ends with the refusal, stdout carries
/// no line of it, and the run stream still holds `runs_before` runs.
fn assert_refused(
    root: &Path,
    (out, err, ok): (String, String, bool),
    (command, spec, open): (&str, &str, Option<&[String]>),
    runs_before: usize,
) {
    assert!(!ok, "{command} refuses the new run; stdout:\n{out}");
    assert!(
        err.ends_with(&refused_new_run(command, spec, open)),
        "{command} ends stderr with the refusal; stderr:\n{err}"
    );
    assert!(
        !out.contains("refusing"),
        "the refusal is on stderr only; stdout:\n{out}"
    );
    assert_eq!(
        run_payloads(root, "RunStarted").len(),
        runs_before,
        "a refused command mints no run"
    );
}

/// Each of the [`RUN_ENTRIES`] plus `extra`, in `root` with `path` as its `PATH`, refused a new
/// run on [`SPEC_REL`] with `open` ([`assert_refused`]), the run stream still holding
/// `runs_before` runs.
fn assert_each_run_entry_refuses(
    root: &Path,
    path: &str,
    extra: &[&str],
    open: Option<&[String]>,
    runs_before: usize,
) {
    for (args, command) in RUN_ENTRIES {
        assert_refused(
            root,
            run_entry(root, path, args, extra),
            (command, SPEC_REL, open),
            runs_before,
        );
    }
}

/// A `DecisionMade` resolving `ids` and governing `governs`, through `rigger emit`.
fn resolve(root: &Path, id: &str, governs: &str, ids: &[String]) {
    emit(
        root,
        "DecisionMade",
        &json!({"id": id, "summary": "closed", "governs": [governs], "resolves": ids}).to_string(),
    );
}

/// The spec's run already in `root`'s store: a `RunStarted` on [`SPEC_REL`] and [`SPEC`]'s
/// criteria, as a run start that minted it records.
fn seed_the_specs_run(root: &Path) {
    let started = json!({"run": "r-spec", "criteria": rigger::spec::extract_criteria(SPEC),
                         "spec": SPEC_REL})
    .to_string();
    seed_run_events(root, &[("RunStarted", started.as_str())]);
}

#[test]
fn a_new_step_refuses_until_every_blocking_finding_of_its_text_is_resolved() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_spec_project(root, &PERSONAS, SKEPTIC_WORKFLOW, SPEC_REL, SPEC);
    let open = reject_ids();
    let [one, three] = &open;

    // Not critiqued: refused, nothing minted, no JSON line on stdout.
    let refused = step(root, SPEC_REL, &[]);
    assert_eq!(refused.0, "", "a refused step prints no JSON line");
    assert_refused(root, refused, ("rigger step", SPEC_REL, None), 0);

    // A resolution recorded BEFORE the critique closes nothing; the critique's BLOCKING ids are
    // open, in finding order, and the NON-BLOCKING one is never listed.
    resolve(root, "early", SPEC_REL, std::slice::from_ref(one));
    record_critique(root, SPEC_REL, REJECT);
    assert_refused(
        root,
        step(root, SPEC_REL, &[]),
        ("rigger step", SPEC_REL, Some(&open)),
        0,
    );

    // A later resolution governing the spec (any spelling that normalizes to it) closes the ids
    // it names; one governing another spec closes nothing.
    resolve(
        root,
        "r1",
        &format!("./{SPEC_REL}"),
        &[one.clone(), critique_finding_id(SPEC, 0, 2)],
    );
    resolve(root, "r2", "specs/other.md", std::slice::from_ref(three));
    assert_refused(
        root,
        step(root, SPEC_REL, &[]),
        ("rigger step", SPEC_REL, Some(std::slice::from_ref(three))),
        0,
    );

    // The last open finding resolved under the spec's absolute path: the step proceeds and mints
    // the run.
    let absolute = root.join(SPEC_REL).display().to_string();
    resolve(root, "r3", &absolute, std::slice::from_ref(three));
    assert_step_proceeds(root, (SPEC_REL, &[]), SPEC_REL, (0, 1));
    assert_eq!(
        run_payloads(root, "RunStarted")[0]["spec"],
        SPEC_REL,
        "the run is on the spec"
    );
}

#[test]
fn a_step_adopting_the_specs_run_proceeds_while_one_beginning_a_new_run_refuses() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_spec_project(root, &PERSONAS, SKEPTIC_WORKFLOW, SPEC_REL, SPEC);

    // A run with no spec is never refused, under a workflow naming a critic.
    let (_out, err, ok) = on_head(root, &["step"], &[]);
    assert!(ok, "a spec-less step proceeds; stderr:\n{err}");
    assert_unrefused(("rigger step", SPEC_REL), &err, 0);
    assert_eq!(
        run_payloads(root, "RunStarted").len(),
        1,
        "the spec-less run is minted"
    );

    // The spec's run already in the store (no critique recorded for its text): a step adopts it,
    // with or without --rebase-definition, and is not refused.
    seed_the_specs_run(root);
    for extra in [&[][..], &["--rebase-definition"][..]] {
        assert_step_proceeds(root, (SPEC_REL, extra), SPEC_REL, (0, 2));
    }

    // --fresh on the same text begins a new run: refused, as is a new run on dropped criteria.
    assert_refused(
        root,
        step(root, SPEC_REL, &["--fresh"]),
        ("rigger step", SPEC_REL, None),
        2,
    );
    std::fs::write(root.join(SPEC_REL), SPEC_DROPPED).unwrap();
    assert_refused(
        root,
        step(root, SPEC_REL, &[]),
        ("rigger step", SPEC_REL, None),
        2,
    );
}

#[test]
fn a_critique_answers_only_its_own_text_and_a_spec_outside_the_repository_is_never_critiqued() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_spec_project(root, &PERSONAS, SKEPTIC_WORKFLOW, SPEC_REL, SPEC);
    record_clean_critique(root, SPEC_REL);

    // DROPPED: the edited text inherits nothing of the clean critique of the text before it.
    std::fs::write(root.join(SPEC_REL), SPEC_DROPPED).unwrap();
    assert_refused(
        root,
        step(root, SPEC_REL, &[]),
        ("rigger step", SPEC_REL, None),
        0,
    );

    // Reverted: the earlier text's clean critique applies again.
    std::fs::write(root.join(SPEC_REL), SPEC).unwrap();
    assert_step_proceeds(root, (SPEC_REL, &[]), SPEC_REL, (0, 1));

    // The same bytes outside the repository cannot be critiqued: a new run on them is refused as
    // not critiqued, named by the spelling given.
    let (_elsewhere, outside) = spec_outside_the_repository();
    assert_refused(
        root,
        step(root, &outside, &["--fresh"]),
        ("rigger step", &outside, None),
        1,
    );
}

/// Each run entry refuses a new run on the uncritiqued spec under its own command name: on an
/// empty store, and with `--fresh` beside the spec's existing run, which begins a new run though
/// the run it would otherwise adopt is in the store. The workflow stops at the grounder, so an
/// entry that misses its refusal fails here instead of driving or serving.
#[test]
fn every_cli_run_start_refuses_a_new_run_on_an_uncritiqued_spec_under_its_own_name() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    let workflow = stopping_at_the_grounder(SKEPTIC_WORKFLOW);
    write_spec_project(root, &PERSONAS, &workflow, SPEC_REL, SPEC);
    let (_work, path) = stand_in_claude();
    assert_each_run_entry_refuses(root, &path, &[], None, 0);

    seed_the_specs_run(root);
    assert_each_run_entry_refuses(root, &path, &["--fresh"], None, 1);
}

/// THE AUTHORITY, an approve beside a BLOCKING line: the line counts as blocking whatever the
/// verdict, so a new run on a text whose only critique approves while holding one is refused on
/// that finding's id alone.
#[test]
fn an_approving_critique_holding_a_blocking_finding_refuses_a_new_run_on_that_finding() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_spec_project(root, &PERSONAS, SKEPTIC_WORKFLOW, SPEC_REL, SPEC);
    let out = record_critique(root, SPEC_REL, APPROVE_BESIDE_BLOCKING);
    assert!(
        out.ends_with("\n{\"verdict\":\"approve\"}\n"),
        "the recorded critique approves; stdout:\n{out}"
    );
    assert_step_refused(
        root,
        (SPEC_REL, &[]),
        SPEC_REL,
        Some(&[critique_finding_id(SPEC, 0, 2)]),
        0,
    );
}

#[test]
fn a_repoless_project_resolves_its_spec_against_the_project_root() {
    let dir = temp_repoless_project();
    let root = dir.path();
    write_spec_project(root, &PERSONAS, SKEPTIC_WORKFLOW, SPEC_REL, SPEC);
    let step_repoless = || run_rigger(root, &["step", "--spec", SPEC_REL]);
    assert_refused(root, step_repoless(), ("rigger step", SPEC_REL, None), 0);
    record_clean_critique(root, SPEC_REL);
    let (_out, err, ok) = step_repoless();
    assert!(ok, "the clean critique lets the run begin; stderr:\n{err}");
    assert_eq!(run_payloads(root, "RunStarted").len(), 1, "one run minted");
}

#[test]
fn a_workflow_naming_no_critic_begins_each_new_run_with_one_line_saying_so() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_spec_project(root, &PERSONAS, CRITICLESS_WORKFLOW, SPEC_REL, SPEC);

    // A new run proceeds with the one line, minting the run; a step adopting it names no
    // critique; each further new run says it again.
    assert_step_proceeds(root, (SPEC_REL, &[]), SPEC_REL, (1, 1));
    assert_step_proceeds(root, (SPEC_REL, &[]), SPEC_REL, (0, 1));
    assert_step_proceeds(root, (SPEC_REL, &["--fresh"]), SPEC_REL, (1, 2));
}

/// Given a workflow naming no critic, when a step begins a new run on a spelling that normalizes to
/// the spec, then its one line names the spec repo-relative, as a refusal would; and when it begins
/// one on a spec outside the repository - which a critic would refuse as never critiqued - then the
/// run begins, its one line naming the spelling given.
#[test]
fn a_workflow_naming_no_critic_names_each_spec_as_a_refusal_would_and_begins_one_outside_the_repository(
) {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_spec_project(root, &PERSONAS, CRITICLESS_WORKFLOW, SPEC_REL, SPEC);
    assert_step_proceeds(root, ("specs/../specs/7-gadget.md", &[]), SPEC_REL, (1, 1));

    let (_elsewhere, outside) = spec_outside_the_repository();
    assert_step_proceeds(root, (&outside, &["--fresh"]), &outside, (1, 2));
}

/// [`CRITICLESS_WORKFLOW`] naming `doubter` as its critic through `defaults.review.adversary`
/// alone: the plan-critique gate still names no adversary.
const DEFAULTS_CRITIC_WORKFLOW: &str = "defaults:\n  grounder: nop\n  review:\n    adversary: \
     doubter\n    adjudicator: arbiter\nstages:\n  plan:\n    agent: planner\n    produces: dag\n  \
     plan-critique:\n    needs: [plan]\n    adjudicator: arbiter\n  implement:\n    needs: \
     [plan-critique]\n    agent: planner\n    strategy: fan-out\n    on_pass: none\n";

const DOUBTER: &str =
    "---\nid: doubter\nmodel: sonnet\ntools: [Read]\nisolation: none\n---\nDoubt it again.\n";

/// [`SPEC`] at another path under the root.
const RENAMED_REL: &str = "specs/7-gadget-renamed.md";

/// `rigger step --spec <spec>` plus `extra` refused a new run on the spec it names `named`, with
/// `open` ([`assert_refused`]), and appended no event of any type to the run stream.
fn assert_step_refused(
    root: &Path,
    (spec, extra): (&str, &[&str]),
    named: &str,
    open: Option<&[String]>,
    runs_before: usize,
) {
    let events_before = read_run_events(root).len();
    assert_refused(
        root,
        step(root, spec, extra),
        ("rigger step", named, open),
        runs_before,
    );
    assert_eq!(
        read_run_events(root).len(),
        events_before,
        "a refused step appends nothing to the run stream"
    );
}

/// `rigger step --spec <spec>` plus `extra` proceeded, printing its one JSON line, refusing
/// nothing and printing its no-critic line naming the spec `named` `no_critic` times
/// ([`assert_unrefused`]), and the run stream then holds `runs` runs.
fn assert_step_proceeds(
    root: &Path,
    (spec, extra): (&str, &[&str]),
    named: &str,
    (no_critic, runs): (usize, usize),
) {
    let (out, err, ok) = step(root, spec, extra);
    assert!(ok, "the step on {spec} {extra:?} proceeds; stderr:\n{err}");
    assert_eq!(out.lines().count(), 1, "one JSON line:\n{out}");
    assert_unrefused(("rigger step", named), &err, no_critic);
    assert_eq!(
        run_payloads(root, "RunStarted").len(),
        runs,
        "the run stream holds {runs} runs"
    );
}

/// The command a refusal on `err` prints after the route label `label`, as the arguments a shell
/// hands `rigger`: the words after `rigger`, then the trailing single-quoted argument unquoted.
fn printed_command(err: &str, label: &str) -> Vec<String> {
    let line = err
        .lines()
        .find_map(|line| line.trim_start().strip_prefix(label))
        .unwrap_or_else(|| panic!("the refusal prints a `{label}` route; stderr:\n{err}"))
        .trim();
    let (words, quoted) = match line.split_once(" '") {
        Some((words, quoted)) => (words, quoted.strip_suffix('\'')),
        None => (line, None),
    };
    let mut args: Vec<String> = words.split_whitespace().map(str::to_string).collect();
    assert_eq!(args.remove(0), "rigger", "the route runs rigger: {line}");
    args.extend(quoted.map(str::to_string));
    args
}

/// Given a refusal, when the operator runs each command it prints exactly as printed, then the
/// refusal moves from not critiqued to the open ids and the run begins. The refusal names the spec
/// repo-relative under every spelling that normalizes to it, so its printed routes are the ones a
/// later run start reads.
#[test]
fn an_operator_clears_a_refusal_by_running_the_commands_it_prints_as_printed() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_spec_project(root, &PERSONAS, SKEPTIC_WORKFLOW, SPEC_REL, SPEC);
    let dotted = format!("./{SPEC_REL}");
    let absolute = root.join(SPEC_REL).display().to_string();
    for given in [&dotted, &absolute] {
        assert_step_refused(root, (given, &[]), SPEC_REL, None, 0);
    }

    let (_out, err, _ok) = step(root, &dotted, &[]);
    let critique = printed_command(&err, CRITIQUE_ROUTE);
    assert_eq!(
        critique,
        ["critique", SPEC_REL],
        "the printed critique route"
    );
    let work = tempfile::tempdir().unwrap();
    let path = write_critique_stub(work.path(), REJECT);
    let critique: Vec<&str> = critique.iter().map(String::as_str).collect();
    let (_out, err, ok) = run_rigger_envs(root, &critique, &[("PATH", path.as_str())]);
    assert!(
        ok,
        "the printed critique route records a critique; stderr:\n{err}"
    );

    let open = reject_ids();
    assert_step_refused(root, (&dotted, &[]), SPEC_REL, Some(&open), 0);
    let (_out, err, _ok) = step(root, &absolute, &[]);
    let resolution = printed_command(&err, RESOLUTION_ROUTE);
    let decision = printed_decision(SPEC_REL, &json!(open).to_string());
    assert_eq!(
        resolution,
        ["emit", "DecisionMade", decision.as_str()],
        "the printed resolution route"
    );
    let resolution: Vec<&str> = resolution.iter().map(String::as_str).collect();
    let (_out, err, ok) = run_rigger(root, &resolution);
    assert!(
        ok,
        "the printed resolution route records it; stderr:\n{err}"
    );

    assert_step_proceeds(root, (&dotted, &[]), SPEC_REL, (0, 1));
}

/// DROPPED, a renamed spec: the critique is keyed on the spec's bytes, so the same bytes at a new
/// path are refused on the critique's open BLOCKING ids, never as not critiqued; the resolutions
/// recorded under the old path close none of them until recorded again under the new one.
#[test]
fn a_renamed_spec_keeps_the_critique_of_its_bytes_but_not_its_old_paths_resolutions() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_spec_project(root, &PERSONAS, SKEPTIC_WORKFLOW, SPEC_REL, SPEC);
    record_critique(root, SPEC_REL, REJECT);
    let open = reject_ids();
    resolve(root, "r-old", SPEC_REL, &open);

    std::fs::rename(root.join(SPEC_REL), root.join(RENAMED_REL)).unwrap();
    assert_step_refused(root, (RENAMED_REL, &[]), RENAMED_REL, Some(&open), 0);

    resolve(root, "r-new", RENAMED_REL, &open);
    assert_step_proceeds(root, (RENAMED_REL, &[]), RENAMED_REL, (0, 1));
    assert_eq!(
        run_payloads(root, "RunStarted")[0]["spec"],
        RENAMED_REL,
        "the run is on the renamed spec"
    );
}

/// DROPPED, a superseded resolution: supersession is not read, so a resolution a later decision
/// supersedes with no `resolves`, or with an empty one, still closes the ids it named.
#[test]
fn a_resolution_stands_when_a_later_decision_supersedes_it_without_resolves() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_spec_project(root, &PERSONAS, SKEPTIC_WORKFLOW, SPEC_REL, SPEC);
    record_critique(root, SPEC_REL, REJECT);
    resolve(root, "r1", SPEC_REL, &reject_ids());
    for later in [
        json!({"id": "r1-reworded", "summary": "reworded", "governs": [SPEC_REL],
               "supersedes": "r1"}),
        json!({"id": "r1-emptied", "summary": "emptied", "governs": [SPEC_REL],
               "supersedes": "r1-reworded", "resolves": []}),
    ] {
        emit(root, "DecisionMade", &later.to_string());
    }
    assert_step_proceeds(root, (SPEC_REL, &[]), SPEC_REL, (0, 1));
}

/// Reverted and DROPPED, the critic: under a workflow that drops its critic a new run proceeds with
/// the one line whatever the recorded critique holds; naming a critic again - the same persona, or
/// another named only through `defaults.review.adversary` - judges each new run on the critique
/// already recorded for the text, which a changed critic leaves standing.
#[test]
fn each_new_run_follows_the_workflows_critic_and_any_critic_reads_the_recorded_critique() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    let personas = [PERSONAS[0], PERSONAS[1], PERSONAS[2], ("doubter", DOUBTER)];
    write_spec_project(root, &personas, SKEPTIC_WORKFLOW, SPEC_REL, SPEC);
    record_critique(root, SPEC_REL, REJECT);
    let open = reject_ids();

    write_scaffold(root, &personas, CRITICLESS_WORKFLOW);
    assert_step_proceeds(root, (SPEC_REL, &[]), SPEC_REL, (1, 1));

    for workflow in [SKEPTIC_WORKFLOW, DEFAULTS_CRITIC_WORKFLOW] {
        write_scaffold(root, &personas, workflow);
        assert_step_refused(root, (SPEC_REL, &["--fresh"]), SPEC_REL, Some(&open), 1);
    }
    resolve(root, "r-all", SPEC_REL, &open);
    assert_step_proceeds(root, (SPEC_REL, &["--fresh"]), SPEC_REL, (0, 2));
}

/// The mint decision at the step: a spec returned to after a later run on other criteria begins a
/// new run - the earlier run on its criteria is never adopted past the later one - so it needs a
/// critique.
#[test]
fn a_spec_returned_to_after_a_later_run_on_other_criteria_needs_a_clean_critique() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_spec_project(root, &PERSONAS, SKEPTIC_WORKFLOW, SPEC_REL, SPEC);
    seed_the_specs_run(root);
    let later = json!({"run": "r-other", "criteria": ["another spec's criterion"],
                       "spec": "specs/8-other.md"})
    .to_string();
    seed_run_events(root, &[("RunStarted", later.as_str())]);
    assert_step_refused(root, (SPEC_REL, &[]), SPEC_REL, None, 2);
    record_clean_critique(root, SPEC_REL);
    assert_step_proceeds(root, (SPEC_REL, &[]), SPEC_REL, (0, 3));
}

/// Existing data, a run branch holding another copy of the spec: the critique is judged on the
/// bytes the step read before it anchored the run branch, so a clean critique of the checked-out
/// text lets the run begin though the anchor puts the other copy in the tree; the next step reads
/// that copy, whose other criteria begin a new run that is refused as not critiqued.
#[test]
fn a_run_branch_holding_another_copy_of_the_spec_cannot_change_the_critiqued_bytes() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_spec_project(root, &PERSONAS, SKEPTIC_WORKFLOW, SPEC_REL, SPEC);
    let home = git_out(root, &["rev-parse", "--abbrev-ref", "HEAD"]);
    git_ok(root, &["add", SPEC_REL]);
    git_ok(root, &["commit", "-q", "-m", "the spec"]);
    git_ok(root, &["checkout", "-q", "-b", "rigger-run"]);
    std::fs::write(root.join(SPEC_REL), SPEC_DROPPED).unwrap();
    git_ok(root, &["commit", "-q", "-am", "the run branch's copy"]);
    git_ok(root, &["checkout", "-q", &home]);
    record_clean_critique(root, SPEC_REL);

    assert_step_proceeds(root, (SPEC_REL, &[]), SPEC_REL, (0, 1));
    assert_eq!(
        git_out(root, &["rev-parse", "--abbrev-ref", "HEAD"]),
        "rigger-run",
        "the step anchored the run branch"
    );
    assert_eq!(
        std::fs::read_to_string(root.join(SPEC_REL)).unwrap(),
        SPEC_DROPPED,
        "the anchor put the run branch's copy in the tree"
    );
    assert_eq!(
        run_payloads(root, "RunStarted")[0]["criteria"],
        json!(rigger::spec::extract_criteria(SPEC)),
        "the run is on the critiqued bytes' criteria"
    );

    assert_step_refused(root, (SPEC_REL, &[]), SPEC_REL, None, 1);
}

/// The run entry `(args, command)` plus `extra` in `root`, with `path` as its `PATH`, got past its
/// run start and stopped at the rejected grounder ([`stopping_at_the_grounder`]): it refused
/// nothing, printed the no-critic line naming itself `no_critic` times and no other, and left the
/// run stream holding `runs` runs, the latest on the spec.
fn assert_past_the_run_start(
    root: &Path,
    path: &str,
    (args, command): (&[&str], &str),
    extra: &[&str],
    (no_critic, runs): (usize, usize),
) {
    let output = run_entry(root, path, args, extra);
    assert_stopped_at_the_grounder(&output, &format!("{command} {extra:?}"));
    assert_unrefused((command, SPEC_REL), &output.1, no_critic);
    let started = run_payloads(root, "RunStarted");
    assert_eq!(
        (started.len(), &started[started.len() - 1]["spec"]),
        (runs, &json!(SPEC_REL)),
        "{command} {extra:?} leaves {runs} run(s), the latest on the spec"
    );
}

/// Given a workflow naming no critic, when each run entry begins a new run on a spec that has no
/// critique, then it proceeds - minting the run - after one line on stderr naming its own command;
/// and when it adopts that run, it proceeds naming no critique.
#[test]
fn every_run_entry_under_a_criticless_workflow_names_each_new_run_it_begins_and_no_adopted_one() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    let workflow = stopping_at_the_grounder(CRITICLESS_WORKFLOW);
    write_spec_project(root, &PERSONAS, &workflow, SPEC_REL, SPEC);
    let (_work, path) = stand_in_claude();
    for (minted, entry) in (1..).zip(RUN_ENTRIES) {
        assert_past_the_run_start(root, &path, entry, &["--fresh"], (1, minted));
        assert_past_the_run_start(root, &path, entry, &[], (0, minted));
    }
}

/// Given a workflow naming a critic, when each run entry adopts the spec's existing run, then it
/// proceeds though the spec's text has no critique; when it begins a new run with `--fresh`, then
/// it reads the critique of the text it loaded: refused by the id of each open BLOCKING finding,
/// and proceeding to mint once a resolution closes them.
#[test]
fn every_run_entry_adopts_the_specs_run_uncritiqued_and_begins_a_new_one_only_on_a_clean_critique()
{
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    let workflow = stopping_at_the_grounder(SKEPTIC_WORKFLOW);
    write_spec_project(root, &PERSONAS, &workflow, SPEC_REL, SPEC);
    let (_work, path) = stand_in_claude();
    seed_the_specs_run(root);
    for entry in RUN_ENTRIES {
        assert_past_the_run_start(root, &path, entry, &[], (0, 1));
    }

    record_critique(root, SPEC_REL, REJECT);
    let open = reject_ids();
    assert_each_run_entry_refuses(root, &path, &["--fresh"], Some(&open), 1);

    resolve(root, "r-all", SPEC_REL, &open);
    for (minted, entry) in (2..).zip(RUN_ENTRIES) {
        assert_past_the_run_start(root, &path, entry, &["--fresh"], (0, minted));
    }
}
