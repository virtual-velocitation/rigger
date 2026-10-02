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
    emit, record_clean_critique, record_critique, run_payloads, run_rigger, run_rigger_envs,
    seed_run_events, temp_repoless_project, write_spec_project,
};
use common::fixtures::temp_git_project_with_commit;
use common::repo::stub_path;
use rigger::review::critique_hash;
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

/// The personas every workflow here names: a planner, the `skeptic` critic and the `arbiter`.
const PERSONAS: [(&str, &str); 3] = [
    ("planner", PLANNER),
    ("skeptic", SKEPTIC),
    ("arbiter", ARBITER),
];

/// `rigger <args...> --base HEAD` in `root`: (stdout, stderr, success).
fn on_head(root: &Path, args: &[&str]) -> (String, String, bool) {
    let args: Vec<&str> = args.iter().copied().chain(["--base", "HEAD"]).collect();
    run_rigger(root, &args)
}

/// `rigger step --spec <spec> --base HEAD` in `root`, plus `extra` flags.
fn step(root: &Path, spec: &str, extra: &[&str]) -> (String, String, bool) {
    let args: Vec<&str> = ["step", "--spec", spec]
        .iter()
        .chain(extra)
        .copied()
        .collect();
    on_head(root, &args)
}

/// The text the refusal of a new run on `spec` by `command` ends stderr with: `open` the open
/// BLOCKING finding ids, `None` when the spec's text has no critique.
fn refusal(command: &str, spec: &str, open: Option<&[&str]>) -> String {
    let governs = json!([spec]);
    let (why, route, resolves) = match open {
        None => (
            "not critiqued".to_string(),
            "or, once critiqued, record a resolution:",
            "[<ids>]".to_string(),
        ),
        Some(ids) => (
            format!("open BLOCKING findings: {}", ids.join(", ")),
            "or record a resolution:          ",
            json!(ids).to_string(),
        ),
    };
    format!(
        "rigger: {command}: refusing to begin a new run on {spec}: {why}\n  amend the spec and \
         critique it:   rigger critique {spec}\n  {route} rigger emit DecisionMade \
         '{{\"id\":\"...\",\"governs\":{governs},\"resolves\":{resolves},\"summary\":\"...\"}}'\n"
    )
}

/// `command` refused a new run on `spec` with `open`: stderr ends with the refusal, stdout carries
/// no line of it, and the run stream still holds `runs_before` runs.
fn assert_refused(
    root: &Path,
    (out, err, ok): (String, String, bool),
    (command, spec, open): (&str, &str, Option<&[&str]>),
    runs_before: usize,
) {
    assert!(!ok, "{command} refuses the new run; stdout:\n{out}");
    assert!(
        err.ends_with(&refusal(command, spec, open)),
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

/// A `DecisionMade` resolving `ids` and governing `governs`, through `rigger emit`.
fn resolve(root: &Path, id: &str, governs: &str, ids: &[String]) {
    emit(
        root,
        "DecisionMade",
        &json!({"id": id, "summary": "closed", "governs": [governs], "resolves": ids}).to_string(),
    );
}

#[test]
fn a_new_step_refuses_until_every_blocking_finding_of_its_text_is_resolved() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_spec_project(root, &PERSONAS, SKEPTIC_WORKFLOW, SPEC_REL, SPEC);
    let hash = critique_hash(SPEC);
    let id = |k: u32| format!("sc-{hash}-0-{k}");
    let [one, three] = [id(1), id(3)];

    // Not critiqued: refused, nothing minted, no JSON line on stdout.
    let refused = step(root, SPEC_REL, &[]);
    assert_eq!(refused.0, "", "a refused step prints no JSON line");
    assert_refused(root, refused, ("rigger step", SPEC_REL, None), 0);

    // A resolution recorded BEFORE the critique closes nothing; the critique's BLOCKING ids are
    // open, in finding order, and the NON-BLOCKING one is never listed.
    resolve(root, "early", SPEC_REL, std::slice::from_ref(&one));
    record_critique(root, SPEC_REL, REJECT);
    assert_refused(
        root,
        step(root, SPEC_REL, &[]),
        (
            "rigger step",
            SPEC_REL,
            Some(&[one.as_str(), three.as_str()]),
        ),
        0,
    );

    // A later resolution governing the spec (any spelling that normalizes to it) closes the ids
    // it names; one governing another spec closes nothing.
    resolve(root, "r1", &format!("./{SPEC_REL}"), &[one.clone(), id(2)]);
    resolve(root, "r2", "specs/other.md", std::slice::from_ref(&three));
    assert_refused(
        root,
        step(root, SPEC_REL, &[]),
        ("rigger step", SPEC_REL, Some(&[three.as_str()])),
        0,
    );

    // The last open finding resolved under the spec's absolute path: the step proceeds and mints
    // the run.
    let absolute = root.join(SPEC_REL).display().to_string();
    resolve(root, "r3", &absolute, std::slice::from_ref(&three));
    let (out, err, ok) = step(root, SPEC_REL, &[]);
    assert!(
        ok,
        "a critique with every BLOCKING finding resolved lets the run begin; stderr:\n{err}"
    );
    assert_eq!(
        out.lines().count(),
        1,
        "the step prints its one JSON line:\n{out}"
    );
    assert!(!err.contains("refusing"), "no refusal; stderr:\n{err}");
    let minted = run_payloads(root, "RunStarted");
    assert_eq!(minted.len(), 1, "one run minted");
    assert_eq!(minted[0]["spec"], SPEC_REL, "the run is on the spec");
}

#[test]
fn a_step_adopting_the_specs_run_proceeds_while_one_beginning_a_new_run_refuses() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_spec_project(root, &PERSONAS, SKEPTIC_WORKFLOW, SPEC_REL, SPEC);

    // A run with no spec is never refused, under a workflow naming a critic.
    let (_out, err, ok) = on_head(root, &["step"]);
    assert!(ok, "a spec-less step proceeds; stderr:\n{err}");
    assert!(
        !err.contains("refusing") && !err.contains("no spec critique"),
        "a spec-less step neither refuses nor names a critique; stderr:\n{err}"
    );
    assert_eq!(
        run_payloads(root, "RunStarted").len(),
        1,
        "the spec-less run is minted"
    );

    // The spec's run already in the store (no critique recorded for its text): a step adopts it,
    // with or without --rebase-definition, and is not refused.
    let criteria = rigger::spec::extract_criteria(SPEC);
    let started = json!({"run": "r-spec", "criteria": criteria, "spec": SPEC_REL}).to_string();
    seed_run_events(root, &[("RunStarted", started.as_str())]);
    for extra in [&[][..], &["--rebase-definition"][..]] {
        let (out, err, ok) = step(root, SPEC_REL, extra);
        assert!(ok, "an adopting step {extra:?} proceeds; stderr:\n{err}");
        assert_eq!(out.lines().count(), 1, "one JSON line:\n{out}");
        assert!(
            !err.contains("refusing") && !err.contains("no spec critique"),
            "an adopting step {extra:?} neither refuses nor names a critique; stderr:\n{err}"
        );
        assert_eq!(
            run_payloads(root, "RunStarted").len(),
            2,
            "the spec's run is adopted, none minted"
        );
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
    let (_out, err, ok) = step(root, SPEC_REL, &[]);
    assert!(
        ok,
        "the reverted text's clean critique lets the run begin; stderr:\n{err}"
    );
    assert_eq!(run_payloads(root, "RunStarted").len(), 1, "one run minted");

    // The same bytes outside the repository cannot be critiqued: a new run on them is refused as
    // not critiqued, named by the spelling given.
    let elsewhere = tempfile::tempdir().unwrap();
    let outside = elsewhere.path().join("7-gadget.md");
    std::fs::write(&outside, SPEC).unwrap();
    let outside = outside.display().to_string();
    assert_refused(
        root,
        step(root, &outside, &["--fresh"]),
        ("rigger step", &outside, None),
        1,
    );
}

#[test]
fn every_cli_run_start_refuses_a_new_run_on_an_uncritiqued_spec_under_its_own_name() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_spec_project(root, &PERSONAS, SKEPTIC_WORKFLOW, SPEC_REL, SPEC);
    // A stand-in `claude` first on PATH, so a run the refusal missed never reaches a real agent.
    let work = tempfile::tempdir().unwrap();
    let path = stub_path(work.path(), "claude", Some("fake-agent.sh"));
    for (args, command) in [
        (&["run", SPEC_REL][..], "rigger run"),
        (&["serve", SPEC_REL][..], "rigger serve"),
        (
            &["run", "--driver", "workflow", SPEC_REL][..],
            "rigger run --driver workflow",
        ),
    ] {
        let args: Vec<&str> = args.iter().copied().chain(["--base", "HEAD"]).collect();
        assert_refused(
            root,
            run_rigger_envs(root, &args, &[("PATH", path.as_str())]),
            (command, SPEC_REL, None),
            0,
        );
    }
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
    let line = format!("rigger step: no spec critique for {SPEC_REL}: {NO_CRITIC_CLAUSE}\n");
    let lines_in = |err: &str| err.matches(&line).count();

    let (_out, err, ok) = step(root, SPEC_REL, &[]);
    assert!(ok, "a new run with no critic proceeds; stderr:\n{err}");
    assert_eq!(lines_in(&err), 1, "one no-critic line; stderr:\n{err}");
    assert_eq!(
        run_payloads(root, "RunStarted").len(),
        1,
        "the run is minted"
    );

    let (_out, err, ok) = step(root, SPEC_REL, &[]);
    assert!(ok, "an adopting step proceeds; stderr:\n{err}");
    assert_eq!(
        lines_in(&err),
        0,
        "an adopted run names no critique; stderr:\n{err}"
    );

    let (_out, err, ok) = step(root, SPEC_REL, &["--fresh"]);
    assert!(ok, "each new run proceeds; stderr:\n{err}");
    assert_eq!(
        lines_in(&err),
        1,
        "each new run says it again; stderr:\n{err}"
    );
    assert_eq!(
        run_payloads(root, "RunStarted").len(),
        2,
        "the fresh run is minted"
    );
}
