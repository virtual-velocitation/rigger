//! Spec 112, criterion 2 (A NEW RUN NEEDS A CLEAN CRITIQUE), and spec 123, criterion 3 (A NEW RUN
//! NEEDS AN EXHAUSTED CRITIQUE) - periphery: the run entries driven as the compiled binary. Given
//! a project whose workflow names a critic, when a command would begin a new run on a spec, then
//! it proceeds only when the critique of the spec's text - the union of every pass recorded for
//! the text - is CLEAN: it has a pass, it is exhausted or its attempt count is at the critic's
//! ceiling, and it holds no BLOCKING finding a `DecisionMade` recorded after that finding's pass
//! leaves open. Otherwise it refuses on stderr and appends nothing: on the open BLOCKING ids,
//! offering their resolution only once the critique is exhausted or at its ceiling; else as not
//! critiqued to exhaustion, offering the critique alone; and a spec outside the repository as not
//! critiqued. A step that adopts the spec's existing run proceeds whatever the critique; and under
//! a workflow naming no critic a new run proceeds with one line saying so. Spec 105 stands ahead
//! of the refusal: a plain step of a held run begins no run, so it prints the hold whatever the
//! critique, and a plain step of a run not proved unheld fails on that read, while a `--fresh`
//! step keeps the refusal ahead of its mint.
//!
//! NOT OWNED HERE: the critique verb and its record (criterion 1), and the plan-critique stop on
//! a spec defect (criterion 5).

mod common;

use std::path::Path;

use common::cli::Refusal::{self, BelowCeiling, Outside, Settled, Unexhausted};
use common::cli::{
    assert_stopped_at_the_grounder, critique_finding_id, emit, printed_decision, read_run_events,
    record_clean_critique, record_critique, refused_new_run, resolve, run_payloads, run_rigger,
    run_rigger_envs, seed_a_hold_no_fold_applies, seed_critique_spawn, seed_on_the_run,
    seed_run_events, stopping_at_the_grounder, temp_repoless_project, write_scaffold,
    write_spec_project, ADVISORY, CLEAN_CRITIQUE, CRITIQUE_ROUTE, NO_VERDICT, RESOLUTION_ROUTE,
};
use common::fixtures::{
    git_ok, git_out, hold_naming, run_held_event, temp_git_project_with_commit,
};
use common::repo::{critique_stub_spawns, stub_path, write_critique_stub_then};
use rigger::review::critique_hash;
use rigger::wave::NO_CRITIC_CLAUSE;
use serde_json::json;

/// A plan, its plan-critique gate naming `skeptic` as its adversary (the critic), and the
/// fan-out implement template the gate releases.
const SKEPTIC_WORKFLOW: &str = "defaults:\n  grounder: nop\nstages:\n  plan:\n    agent: \
     planner\n    produces: dag\n  plan-critique:\n    needs: [plan]\n    adversary: skeptic\n    \
     adjudicator: arbiter\n  implement:\n    needs: [plan-critique]\n    agent: planner\n    \
     strategy: fan-out\n    on_pass: none\n";

/// [`SKEPTIC_WORKFLOW`] with the gate's adversary dropped and no adversary step in
/// `defaults.review`: no critic.
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

/// `command` refused a new run on `spec` in the shape `refusal`: stderr ends with the refusal,
/// stdout carries no line of it, and the run stream still holds `runs_before` runs.
fn assert_refused(
    root: &Path,
    (out, err, ok): (String, String, bool),
    (command, spec, refusal): (&str, &str, Refusal),
    runs_before: usize,
) {
    assert!(!ok, "{command} refuses the new run; stdout:\n{out}");
    assert!(
        err.ends_with(&refused_new_run(command, spec, refusal)),
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
/// run on [`SPEC_REL`] in the shape `refusal` ([`assert_refused`]), the run stream still holding
/// `runs_before` runs.
fn assert_each_run_entry_refuses(
    root: &Path,
    path: &str,
    extra: &[&str],
    refusal: Refusal,
    runs_before: usize,
) {
    assert_each_run_entry_refuses_on(root, (path, SPEC_REL), extra, refusal, runs_before);
}

/// Each of the [`RUN_ENTRIES`] given `spec` in place of [`SPEC_REL`], plus `extra`, in `root` with
/// `path` as its `PATH`, refused a new run on `spec` in the shape `refusal` ([`assert_refused`]),
/// the run stream still holding `runs_before` runs.
fn assert_each_run_entry_refuses_on(
    root: &Path,
    (path, spec): (&str, &str),
    extra: &[&str],
    refusal: Refusal,
    runs_before: usize,
) {
    for (args, command) in RUN_ENTRIES {
        let args: Vec<&str> = args
            .iter()
            .map(|&arg| if arg == SPEC_REL { spec } else { arg })
            .collect();
        assert_refused(
            root,
            run_entry(root, path, &args, extra),
            (command, spec, refusal),
            runs_before,
        );
    }
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
    assert_refused(root, refused, ("rigger step", SPEC_REL, Unexhausted), 0);

    // A resolution recorded BEFORE the critique closes nothing; the critique's BLOCKING ids are
    // open, in finding order, and the NON-BLOCKING one is never listed.
    resolve(root, "early", SPEC_REL, std::slice::from_ref(one));
    record_critique(root, SPEC_REL, REJECT);
    assert_refused(
        root,
        step(root, SPEC_REL, &[]),
        ("rigger step", SPEC_REL, Settled(&open)),
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
        (
            "rigger step",
            SPEC_REL,
            Settled(std::slice::from_ref(three)),
        ),
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
        ("rigger step", SPEC_REL, Unexhausted),
        2,
    );
    std::fs::write(root.join(SPEC_REL), SPEC_DROPPED).unwrap();
    assert_refused(
        root,
        step(root, SPEC_REL, &[]),
        ("rigger step", SPEC_REL, Unexhausted),
        2,
    );
}

/// How a step of a held run ends.
enum HeldRunStep {
    /// It prints the held line, exits 0 and leaves the run stream as it stood.
    PrintsTheHold,
    /// It refuses the new run ([`assert_step_refused`]).
    Refused,
    /// It begins the new run and goes on ([`assert_step_proceeds`]).
    BeginsANewRun,
}

/// Spec 105, HELD MEANS NO LAUNCH AND NO CHARGE: given a held run on the spec, when a plain step
/// names the spec it prints the hold and exits 0 whatever the critique of the spec's text, since
/// a held step begins no run for the refusal to guard; when a `--fresh` step names it the refusal
/// still stands ahead of its mint, and a text critiqued clean lets that step mint the new run,
/// which released the hold it ended, and go on.
#[test]
fn a_plain_step_of_a_held_run_prints_the_hold_whatever_the_critique_and_fresh_keeps_its_refusal() {
    use HeldRunStep::{BeginsANewRun, PrintsTheHold, Refused};
    const HELD: &str = "{\"wave\":[],\"done\":false,\"halted\":\
                        \"run held - operator - the quota is being moved\"}\n";
    let uncritiqued: fn(&Path) = |_| {};
    let critiqued_clean: fn(&Path) = |root| record_clean_critique(root, SPEC_REL);
    let finding_open: fn(&Path) = |root| {
        record_critique(root, SPEC_REL, REJECT);
    };
    let plain: &[&str] = &[];
    let fresh: &[&str] = &["--fresh"];
    for (name, text, critique, extra, ends) in [
        (
            "the spec unchanged",
            SPEC,
            uncritiqued,
            plain,
            PrintsTheHold,
        ),
        (
            "the spec amended, not critiqued",
            SPEC_DROPPED,
            uncritiqued,
            plain,
            PrintsTheHold,
        ),
        (
            "the spec amended, a BLOCKING finding open",
            SPEC_DROPPED,
            finding_open,
            plain,
            PrintsTheHold,
        ),
        (
            "the spec amended, critiqued clean",
            SPEC_DROPPED,
            critiqued_clean,
            plain,
            PrintsTheHold,
        ),
        (
            "--fresh on the amended spec, not critiqued",
            SPEC_DROPPED,
            uncritiqued,
            fresh,
            Refused,
        ),
        (
            "--fresh on the amended spec, critiqued clean",
            SPEC_DROPPED,
            critiqued_clean,
            fresh,
            BeginsANewRun,
        ),
    ] {
        let dir = temp_git_project_with_commit();
        let root = dir.path();
        write_spec_project(root, &PERSONAS, SKEPTIC_WORKFLOW, SPEC_REL, SPEC);
        seed_the_specs_run(root);
        seed_on_the_run(
            root,
            run_held_event(&hold_naming(
                "operator",
                "the quota is being moved",
                rigger::run::HOLD_BY_OPERATOR,
            )),
        );
        std::fs::write(root.join(SPEC_REL), text).unwrap();
        critique(root);

        match ends {
            PrintsTheHold => {
                let events_before = read_run_events(root).len();
                let (out, err, ok) = step(root, SPEC_REL, extra);
                assert_eq!((out.as_str(), ok), (HELD, true), "{name}; stderr:\n{err}");
                assert_unrefused(("rigger step", SPEC_REL), &err, 0);
                assert_eq!(
                    (
                        run_payloads(root, "RunStarted").len(),
                        read_run_events(root).len()
                    ),
                    (1, events_before),
                    "{name}: a held step mints no run and records nothing"
                );
            }
            Refused => assert_step_refused(root, (SPEC_REL, extra), SPEC_REL, Unexhausted, 1),
            BeginsANewRun => assert_step_proceeds(root, (SPEC_REL, extra), SPEC_REL, (0, 2)),
        }
    }
}

/// Spec 105, a run not proved unheld launches nothing, on the shape a courier steps with: given a
/// `RunHeld` no fold applies on a spec amended since its run began, when a plain step names the
/// spec it fails on that read ahead of the refusal, whose routes would clear nothing the step
/// stopped on; when a `--fresh` step names it the refusal still stands ahead of its mint; and a
/// text critiqued clean lets that step reach its mint, which fails on the hold it cannot release.
/// Each leaves stdout empty, so a courier reports an error and never a wave, and appends nothing.
#[test]
fn a_step_of_a_run_not_proved_unheld_fails_ahead_of_any_mint_and_a_plain_one_ahead_of_its_refusal()
{
    let refused = refused_new_run("rigger step", SPEC_REL, Unexhausted);
    let rows: [(&str, bool, &[&str], &str); 3] = [
        (
            "a plain step, the text not critiqued",
            false,
            &[],
            "rigger: event store: fold the run's hold: expected ident at line 1 column 2\n",
        ),
        (
            "--fresh, the text not critiqued",
            false,
            &["--fresh"],
            &refused,
        ),
        (
            "--fresh, the text critiqued clean",
            true,
            &["--fresh"],
            "rigger: event store: fold the run's hold: expected ident at line 1 column 2\n",
        ),
    ];
    for (name, critiqued_clean, extra, failed_on) in rows {
        let dir = temp_git_project_with_commit();
        let root = dir.path();
        write_spec_project(root, &PERSONAS, SKEPTIC_WORKFLOW, SPEC_REL, SPEC);
        seed_the_specs_run(root);
        seed_a_hold_no_fold_applies(root);
        std::fs::write(root.join(SPEC_REL), SPEC_DROPPED).unwrap();
        if critiqued_clean {
            record_clean_critique(root, SPEC_REL);
        }
        let events_before = read_run_events(root).len();

        let (out, err, ok) = step(root, SPEC_REL, extra);

        assert_eq!((out.as_str(), ok), ("", false), "{name}; stderr:\n{err}");
        assert_eq!(
            err.split_once('\n').map(|(_, past_the_hint)| past_the_hint),
            Some(failed_on),
            "{name}: what the step failed on"
        );
        assert_eq!(
            read_run_events(root).len(),
            events_before,
            "{name}: the step appends nothing"
        );
    }
}

/// A run its plan-critique gate stopped on a spec defect (an escalation carrying the stop's
/// reason, spec 112 criterion 5) is terminal for adoption by spec: after an amendment that leaves
/// the criteria equal, a step on the spec, and every other run entry on it, begins a new run -
/// refused while the amended text is uncritiqued, appending nothing - and once the amended text is
/// critiqued clean the step begins it.
#[test]
fn a_run_stopped_on_a_spec_defect_is_never_adopted_by_spec_and_its_new_run_is_gated() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_spec_project(root, &PERSONAS, SKEPTIC_WORKFLOW, SPEC_REL, SPEC);
    seed_the_specs_run(root);
    seed_run_events(
        root,
        &[(
            "UnitEscalated",
            r#"{"id":"plan-critique","reason":"amend the spec and relaunch: a defect"}"#,
        )],
    );
    let amended = SPEC.replace("The gadget spins.", "The gadget spins once per call.");
    std::fs::write(root.join(SPEC_REL), amended).unwrap();

    assert_refused(
        root,
        step(root, SPEC_REL, &[]),
        ("rigger step", SPEC_REL, Unexhausted),
        1,
    );
    let (_work, path) = stand_in_claude();
    assert_each_run_entry_refuses(root, &path, &[], Unexhausted, 1);

    record_clean_critique(root, SPEC_REL);
    assert_step_proceeds(root, (SPEC_REL, &[]), SPEC_REL, (0, 2));
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
        ("rigger step", SPEC_REL, Unexhausted),
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
        ("rigger step", &outside, Outside),
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
    assert_each_run_entry_refuses(root, &path, &[], Unexhausted, 0);

    seed_the_specs_run(root);
    assert_each_run_entry_refuses(root, &path, &["--fresh"], Unexhausted, 1);
}

/// Given a clean critique of the spec's text inside the repository, when each run entry begins a
/// new run on the same bytes outside it, then it refuses as not critiqued, under its own name and
/// the spelling given, offering the critique and naming the resolution that follows one: the
/// critique of a spec is read only for a path inside the repository.
#[test]
fn every_run_entry_refuses_a_new_run_on_a_spec_outside_the_repository_as_not_critiqued() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    let workflow = stopping_at_the_grounder(SKEPTIC_WORKFLOW);
    write_spec_project(root, &PERSONAS, &workflow, SPEC_REL, SPEC);
    record_clean_critique(root, SPEC_REL);
    let (_work, path) = stand_in_claude();
    let (_elsewhere, outside) = spec_outside_the_repository();

    assert_each_run_entry_refuses_on(root, (&path, &outside), &[], Outside, 0);
}

/// THE AUTHORITY, an approve beside a BLOCKING line: the line counts as blocking whatever the
/// verdict, so the critique of a text whose only pass approves while holding one derives reject,
/// and a new run on it is refused on that finding's id alone.
#[test]
fn an_approving_critique_holding_a_blocking_finding_refuses_a_new_run_on_that_finding() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_spec_project(root, &PERSONAS, SKEPTIC_WORKFLOW, SPEC_REL, SPEC);
    let out = record_critique(root, SPEC_REL, APPROVE_BESIDE_BLOCKING);
    assert!(
        out.ends_with("\n{\"verdict\":\"reject\"}\n"),
        "the critique's derived verdict rejects though its one pass approves; stdout:\n{out}"
    );
    assert_step_refused(
        root,
        (SPEC_REL, &[]),
        SPEC_REL,
        Settled(&[critique_finding_id(SPEC, 0, 2)]),
        0,
    );
}

/// Spec 123, THE CRITIQUE IS THE UNION OF ITS PASSES, at the refusal: given a text whose critique
/// holds attempt 0's reject, then a resolution naming one BLOCKING id of attempt 0 and one of
/// attempt 1, then attempt 1's reject and attempt 2's pass with no finding line, when a step
/// begins a new run on it, then the step is refused on the open BLOCKING ids of every pass in pass
/// order, though the latest pass approves: the resolution closes the id of the pass recorded
/// before it and never the one of the pass recorded after it, though it names that id. Once the
/// rest are resolved, the step begins the run.
#[test]
fn a_new_run_is_refused_on_the_unions_open_ids_each_closed_only_after_its_pass() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_spec_project(root, &PERSONAS, SKEPTIC_WORKFLOW, SPEC_REL, SPEC);
    let hash = critique_hash(SPEC);
    let id = |attempt, k| critique_finding_id(SPEC, attempt, k);
    record_critique(root, SPEC_REL, REJECT);
    resolve(root, "r-between", SPEC_REL, &[id(0, 1), id(1, 1)]);
    seed_critique_spawn(root, &hash, 1, REJECT);
    seed_critique_spawn(root, &hash, 2, CLEAN_CRITIQUE);

    let open = [id(0, 3), id(1, 1), id(1, 3)];
    assert_step_refused(root, (SPEC_REL, &[]), SPEC_REL, Settled(&open), 0);

    resolve(root, "r-after", SPEC_REL, &open);
    assert_step_proceeds(root, (SPEC_REL, &[]), SPEC_REL, (0, 1));
}

/// [`SKEPTIC_WORKFLOW`] with its plan-critique gate naming its own `max_retries` of 2: the critic's
/// ceiling, below the built-in 3.
fn gate_two_workflow() -> String {
    let gate_two = SKEPTIC_WORKFLOW.replacen(
        "    adjudicator: arbiter\n",
        "    adjudicator: arbiter\n    max_retries: 2\n",
        1,
    );
    assert_ne!(gate_two, SKEPTIC_WORKFLOW, "the gate names its own ceiling");
    gate_two
}

/// A project carrying `workflow` and [`SPEC`] at [`SPEC_REL`], whose critic spawns on that text
/// recorded `answers` in attempt order ([`seed_critique_spawn`]), followed - when `resolved` - by a
/// resolution of attempt 0's BLOCKING ids ([`reject_ids`]): its directory, which keeps it alive.
fn critiqued_project(workflow: &str, answers: &[&str], resolved: bool) -> tempfile::TempDir {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_spec_project(root, &PERSONAS, workflow, SPEC_REL, SPEC);
    let hash = critique_hash(SPEC);
    for (attempt, answer) in (0..).zip(answers) {
        seed_critique_spawn(root, &hash, attempt, answer);
    }
    if resolved {
        resolve(root, "r-all", SPEC_REL, &reject_ids());
    }
    dir
}

/// A row of the CLEAN MEANS EXHAUSTED table below: its name, the workflow, the critic's answers in
/// attempt order, whether attempt 0's BLOCKING ids are resolved after them, and the refusal of the
/// new run, `None` when it begins.
type CeilingRow<'a> = (&'a str, &'a str, &'a [&'a str], bool, Option<Refusal<'a>>);

/// Spec 123, CLEAN MEANS EXHAUSTED: given a text whose critic spawns recorded a row's answers in
/// attempt order, under the critic's built-in ceiling of 3 or its plan-critique gate's own ceiling
/// of 2, and perhaps a resolution of attempt 0's BLOCKING ids recorded after them, when a step
/// begins a new run on it, then the run begins only when the critique has a pass, is exhausted or
/// has an attempt count at or above the ceiling - a spawn that recorded no pass counting as an
/// attempt - and leaves no BLOCKING id open. Else it is refused on the open BLOCKING ids, offering
/// their resolution only once the critique is exhausted or at its ceiling, and otherwise as not
/// critiqued to exhaustion, offering the critique alone.
#[test]
fn a_new_run_needs_its_critique_exhausted_or_at_the_ceiling_and_offers_a_resolution_only_then() {
    let gate_two = gate_two_workflow();
    let open = reject_ids();
    let rows: [CeilingRow; 18] = [
        (
            "one advisory pass, below the built-in ceiling",
            SKEPTIC_WORKFLOW,
            &[ADVISORY],
            false,
            Some(Unexhausted),
        ),
        (
            "two advisory passes, below the built-in ceiling",
            SKEPTIC_WORKFLOW,
            &[ADVISORY; 2],
            false,
            Some(Unexhausted),
        ),
        (
            "three advisory passes, at the built-in ceiling",
            SKEPTIC_WORKFLOW,
            &[ADVISORY; 3],
            false,
            None,
        ),
        (
            "four advisory passes, above the built-in ceiling",
            SKEPTIC_WORKFLOW,
            &[ADVISORY; 4],
            false,
            None,
        ),
        (
            "an advisory pass then one with no finding line: exhausted below the ceiling",
            SKEPTIC_WORKFLOW,
            &[ADVISORY, CLEAN_CRITIQUE],
            false,
            None,
        ),
        (
            "one advisory pass, below the gate's ceiling",
            &gate_two,
            &[ADVISORY],
            false,
            Some(Unexhausted),
        ),
        (
            "two advisory passes, at the gate's ceiling",
            &gate_two,
            &[ADVISORY; 2],
            false,
            None,
        ),
        (
            "three advisory passes, above the gate's ceiling",
            &gate_two,
            &[ADVISORY; 3],
            false,
            None,
        ),
        (
            "two spawns with no verdict then an advisory pass, at the built-in ceiling",
            SKEPTIC_WORKFLOW,
            &[NO_VERDICT, NO_VERDICT, ADVISORY],
            false,
            None,
        ),
        (
            "three spawns with no verdict: no pass, at the built-in ceiling",
            SKEPTIC_WORKFLOW,
            &[NO_VERDICT; 3],
            false,
            Some(Unexhausted),
        ),
        (
            "a reject, below the built-in ceiling",
            SKEPTIC_WORKFLOW,
            &[REJECT],
            false,
            Some(BelowCeiling(&open)),
        ),
        (
            "a reject then an advisory pass, below the built-in ceiling",
            SKEPTIC_WORKFLOW,
            &[REJECT, ADVISORY],
            false,
            Some(BelowCeiling(&open)),
        ),
        (
            "a reject then two advisory passes, at the built-in ceiling",
            SKEPTIC_WORKFLOW,
            &[REJECT, ADVISORY, ADVISORY],
            false,
            Some(Settled(&open)),
        ),
        (
            "a reject then a pass with no finding line: exhausted below the ceiling",
            SKEPTIC_WORKFLOW,
            &[REJECT, CLEAN_CRITIQUE],
            false,
            Some(Settled(&open)),
        ),
        (
            "a reject, below the gate's ceiling",
            &gate_two,
            &[REJECT],
            false,
            Some(BelowCeiling(&open)),
        ),
        (
            "a reject then an advisory pass, at the gate's ceiling",
            &gate_two,
            &[REJECT, ADVISORY],
            false,
            Some(Settled(&open)),
        ),
        (
            "a reject resolved, below the built-in ceiling",
            SKEPTIC_WORKFLOW,
            &[REJECT],
            true,
            Some(Unexhausted),
        ),
        (
            "a reject then two advisory passes, resolved at the built-in ceiling",
            SKEPTIC_WORKFLOW,
            &[REJECT, ADVISORY, ADVISORY],
            true,
            None,
        ),
    ];
    for (name, workflow, answers, resolved, refused) in rows {
        let dir = critiqued_project(workflow, answers, resolved);
        let root = dir.path();

        println!("row: {name}");
        match refused {
            Some(refusal) => assert_step_refused(root, (SPEC_REL, &[]), SPEC_REL, refusal, 0),
            None => assert_step_proceeds(root, (SPEC_REL, &[]), SPEC_REL, (0, 1)),
        }
    }
}

#[test]
fn a_repoless_project_resolves_its_spec_against_the_project_root() {
    let dir = temp_repoless_project();
    let root = dir.path();
    write_spec_project(root, &PERSONAS, SKEPTIC_WORKFLOW, SPEC_REL, SPEC);
    let step_repoless = || run_rigger(root, &["step", "--spec", SPEC_REL]);
    assert_refused(
        root,
        step_repoless(),
        ("rigger step", SPEC_REL, Unexhausted),
        0,
    );
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

/// [`DEFAULTS_CRITIC_WORKFLOW`] with `defaults.review` a list: `doubter` is its adversary step.
const LISTED_CRITIC_WORKFLOW: &str = "defaults:\n  grounder: nop\n  review:\n    steps: \
     [{ adversary: doubter }, { adjudicator: arbiter }]\nstages:\n  plan:\n    agent: planner\n    \
     produces: dag\n  plan-critique:\n    needs: [plan]\n    adjudicator: arbiter\n  \
     implement:\n    needs: [plan-critique]\n    agent: planner\n    strategy: fan-out\n    \
     on_pass: none\n";

const DOUBTER: &str =
    "---\nid: doubter\nmodel: sonnet\ntools: [Read]\nisolation: none\n---\nDoubt it again.\n";

/// [`SPEC`] at another path under the root.
const RENAMED_REL: &str = "specs/7-gadget-renamed.md";

/// `rigger step --spec <spec>` plus `extra` refused a new run on the spec it names `named`, in the
/// shape `refusal` ([`assert_refused`]), and appended no event of any type to the run stream.
fn assert_step_refused(
    root: &Path,
    (spec, extra): (&str, &[&str]),
    named: &str,
    refusal: Refusal,
    runs_before: usize,
) {
    let events_before = read_run_events(root).len();
    assert_refused(
        root,
        step(root, spec, extra),
        ("rigger step", named, refusal),
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

/// The command the refusal of `rigger step --spec <given>` prints after the route label `label`
/// ([`printed_command`]), asserted to be `printed`, then run as printed in `root` with extra
/// environment `envs`: (stdout, stderr, success).
fn run_printed_route(
    root: &Path,
    given: &str,
    (label, printed): (&str, &[&str]),
    envs: &[(&str, &str)],
) -> (String, String, bool) {
    let (_out, err, _ok) = step(root, given, &[]);
    let command = printed_command(&err, label);
    assert_eq!(command, printed, "the route printed after `{label}`");
    let command: Vec<&str> = command.iter().map(String::as_str).collect();
    run_rigger_envs(root, &command, envs)
}

/// The critique route of the refusal of a step on `given`, run as printed ([`run_printed_route`])
/// with the critique stub first on `PATH`, its first spawn answering `first` and every later one
/// `later`: the stub's work directory, which counts its spawns, and the verb's (stdout, stderr,
/// success).
fn run_printed_critique(
    root: &Path,
    given: &str,
    (first, later): (&str, &str),
) -> (tempfile::TempDir, (String, String, bool)) {
    let work = tempfile::tempdir().unwrap();
    let path = write_critique_stub_then(work.path(), first, later);
    let output = run_printed_route(
        root,
        given,
        (CRITIQUE_ROUTE, &["critique", SPEC_REL]),
        &[("PATH", path.as_str())],
    );
    (work, output)
}

/// The resolution route of the refusal of a step on `given`, resolving the open BLOCKING `ids`,
/// run as printed ([`run_printed_route`]) and recorded.
fn run_printed_resolution(root: &Path, given: &str, ids: &[String]) {
    let decision = printed_decision(SPEC_REL, &json!(ids).to_string());
    let (_out, err, ok) = run_printed_route(
        root,
        given,
        (
            RESOLUTION_ROUTE,
            &["emit", "DecisionMade", decision.as_str()],
        ),
        &[],
    );
    assert!(
        ok,
        "the printed resolution route records it; stderr:\n{err}"
    );
}

/// Given a refusal, when the operator runs each command it prints exactly as printed, then the
/// refusal moves from not critiqued to exhaustion to the open ids and the run begins. The refusal
/// names the spec repo-relative under every spelling that normalizes to it, so its printed routes
/// are the ones a later run start reads.
#[test]
fn an_operator_clears_a_refusal_by_running_the_commands_it_prints_as_printed() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_spec_project(root, &PERSONAS, SKEPTIC_WORKFLOW, SPEC_REL, SPEC);
    let dotted = format!("./{SPEC_REL}");
    let absolute = root.join(SPEC_REL).display().to_string();
    for given in [&dotted, &absolute] {
        assert_step_refused(root, (given, &[]), SPEC_REL, Unexhausted, 0);
    }

    let (_work, (_out, err, ok)) = run_printed_critique(root, &dotted, (REJECT, CLEAN_CRITIQUE));
    assert!(
        ok,
        "the printed critique route records a critique; stderr:\n{err}"
    );

    let open = reject_ids();
    assert_step_refused(root, (&dotted, &[]), SPEC_REL, Settled(&open), 0);
    run_printed_resolution(root, &absolute, &open);

    assert_step_proceeds(root, (&dotted, &[]), SPEC_REL, (0, 1));
}

/// A row of the walk table below: its name, the workflow, the critic's answers recorded before the
/// walk in attempt order and whether attempt 0's BLOCKING ids are resolved after them
/// ([`critiqued_project`]), the refusal the walk begins at, the critique stub's first and later
/// answers, the printed critique route's success and spawn count, and the refusal of the next new
/// run, `None` when it begins.
type WalkRow<'a> = (
    &'a str,
    &'a str,
    &'a [&'a str],
    bool,
    Refusal<'a>,
    (&'a str, &'a str),
    (bool, usize),
    Option<Refusal<'a>>,
);

/// Spec 123, CLEAN MEANS EXHAUSTED: given a new run refused on a critique that is neither
/// exhausted nor at its ceiling, or that has no pass at it, when the operator runs the critique
/// route the refusal prints, as printed, on the unchanged text, then the critic is spawned from the
/// critique's next attempt until a pass adds no finding line or the attempt count reaches the
/// ceiling - a spawn that recorded no pass counting toward it in the loop as in the refusal - and
/// the next new run begins, or is refused on the open BLOCKING ids with their resolution route,
/// which run as printed lets it begin. With no pass at the ceiling the route spawns nothing and
/// the refusal stands.
#[test]
fn an_operator_continues_a_critique_short_of_its_end_by_running_the_route_the_refusal_prints() {
    let gate_two = gate_two_workflow();
    let open = reject_ids();
    let rows: [WalkRow; 8] = [
        (
            "an advisory pass below the built-in ceiling, the next pass adding no finding line",
            SKEPTIC_WORKFLOW,
            &[ADVISORY],
            false,
            Unexhausted,
            (CLEAN_CRITIQUE, CLEAN_CRITIQUE),
            (true, 1),
            None,
        ),
        (
            "an advisory pass below the built-in ceiling, the critic restating to the ceiling",
            SKEPTIC_WORKFLOW,
            &[ADVISORY],
            false,
            Unexhausted,
            (ADVISORY, ADVISORY),
            (true, 2),
            None,
        ),
        (
            "an advisory pass then a spawn with no verdict, the critic restating to the ceiling",
            SKEPTIC_WORKFLOW,
            &[ADVISORY, NO_VERDICT],
            false,
            Unexhausted,
            (ADVISORY, ADVISORY),
            (true, 1),
            None,
        ),
        (
            "an advisory pass below the gate's ceiling, the critic restating to it",
            &gate_two,
            &[ADVISORY],
            false,
            Unexhausted,
            (ADVISORY, ADVISORY),
            (true, 1),
            None,
        ),
        (
            "a reject below the built-in ceiling, the next pass adding no finding line",
            SKEPTIC_WORKFLOW,
            &[REJECT],
            false,
            BelowCeiling(&open),
            (CLEAN_CRITIQUE, CLEAN_CRITIQUE),
            (true, 1),
            Some(Settled(&open)),
        ),
        (
            "a reject below the gate's ceiling, the critic restating to it",
            &gate_two,
            &[REJECT],
            false,
            BelowCeiling(&open),
            (ADVISORY, ADVISORY),
            (true, 1),
            Some(Settled(&open)),
        ),
        (
            "a reject resolved below the built-in ceiling, the next pass adding no finding line",
            SKEPTIC_WORKFLOW,
            &[REJECT],
            true,
            Unexhausted,
            (CLEAN_CRITIQUE, CLEAN_CRITIQUE),
            (true, 1),
            None,
        ),
        (
            "three spawns with no verdict: no pass at the built-in ceiling",
            SKEPTIC_WORKFLOW,
            &[NO_VERDICT; 3],
            false,
            Unexhausted,
            (CLEAN_CRITIQUE, CLEAN_CRITIQUE),
            (false, 0),
            Some(Unexhausted),
        ),
    ];
    for (name, workflow, answers, resolved, refused, stub, verb, then) in rows {
        let dir = critiqued_project(workflow, answers, resolved);
        let root = dir.path();

        println!("row: {name}");
        assert_step_refused(root, (SPEC_REL, &[]), SPEC_REL, refused, 0);
        let (work, (_out, err, ok)) = run_printed_critique(root, SPEC_REL, stub);
        assert_eq!(
            (ok, critique_stub_spawns(work.path())),
            verb,
            "the printed critique route's success and spawns; stderr:\n{err}"
        );
        match then {
            None => assert_step_proceeds(root, (SPEC_REL, &[]), SPEC_REL, (0, 1)),
            Some(Settled(ids)) => {
                assert_step_refused(root, (SPEC_REL, &[]), SPEC_REL, Settled(ids), 0);
                run_printed_resolution(root, SPEC_REL, ids);
                assert_step_proceeds(root, (SPEC_REL, &[]), SPEC_REL, (0, 1));
            }
            Some(refusal) => assert_step_refused(root, (SPEC_REL, &[]), SPEC_REL, refusal, 0),
        }
    }
}

/// DROPPED, a renamed spec: the critique is keyed on the spec's bytes, so the same bytes at a new
/// path are refused on the critique's open BLOCKING ids, never as not critiqued to exhaustion; the
/// resolutions recorded under the old path close none of them until recorded again under the new
/// one.
#[test]
fn a_renamed_spec_keeps_the_critique_of_its_bytes_but_not_its_old_paths_resolutions() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_spec_project(root, &PERSONAS, SKEPTIC_WORKFLOW, SPEC_REL, SPEC);
    record_critique(root, SPEC_REL, REJECT);
    let open = reject_ids();
    resolve(root, "r-old", SPEC_REL, &open);

    std::fs::rename(root.join(SPEC_REL), root.join(RENAMED_REL)).unwrap();
    assert_step_refused(root, (RENAMED_REL, &[]), RENAMED_REL, Settled(&open), 0);

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
/// another named only through an adversary step of `defaults.review`, in either form - judges
/// each new run on the critique already recorded for the text, which a changed critic leaves
/// standing.
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

    for workflow in [
        SKEPTIC_WORKFLOW,
        DEFAULTS_CRITIC_WORKFLOW,
        LISTED_CRITIC_WORKFLOW,
    ] {
        write_scaffold(root, &personas, workflow);
        assert_step_refused(root, (SPEC_REL, &["--fresh"]), SPEC_REL, Settled(&open), 1);
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
    assert_step_refused(root, (SPEC_REL, &[]), SPEC_REL, Unexhausted, 2);
    record_clean_critique(root, SPEC_REL);
    assert_step_proceeds(root, (SPEC_REL, &[]), SPEC_REL, (0, 3));
}

/// Existing data, a run branch holding another copy of the spec: the critique is judged on the
/// bytes the step read before it anchored the run branch, so a clean critique of the checked-out
/// text lets the run begin though the anchor puts the other copy in the tree; the next step reads
/// that copy, whose other criteria begin a new run that is refused as not critiqued to exhaustion.
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

    assert_step_refused(root, (SPEC_REL, &[]), SPEC_REL, Unexhausted, 1);
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
    assert_each_run_entry_refuses(root, &path, &["--fresh"], Settled(&open), 1);

    resolve(root, "r-all", SPEC_REL, &open);
    for (minted, entry) in (2..).zip(RUN_ENTRIES) {
        assert_past_the_run_start(root, &path, entry, &["--fresh"], (0, minted));
    }
}

/// Spec 123, CLEAN MEANS EXHAUSTED: given a workflow whose critique gate names its own
/// `max_retries` of 2 and a critique of the spec's text no pass of which is free of finding lines,
/// when each run entry begins a new run, then it is refused below that ceiling - on the open
/// BLOCKING ids with the critique route alone, or as not critiqued to exhaustion - and at it is
/// refused on the open ids with their resolution route, or proceeds to mint when none is open.
#[test]
fn every_run_entry_holds_a_new_run_to_the_ceiling_of_the_critique_it_reads() {
    let workflow = stopping_at_the_grounder(&gate_two_workflow());
    let (_work, path) = stand_in_claude();
    let open = reject_ids();
    let rows: [(&str, &[&str], Option<Refusal>); 4] = [
        (
            "a reject below the ceiling",
            &[REJECT],
            Some(BelowCeiling(&open)),
        ),
        (
            "an advisory pass below the ceiling",
            &[ADVISORY],
            Some(Unexhausted),
        ),
        (
            "a reject then an advisory pass at the ceiling",
            &[REJECT, ADVISORY],
            Some(Settled(&open)),
        ),
        (
            "two advisory passes at the ceiling",
            &[ADVISORY, ADVISORY],
            None,
        ),
    ];
    for (name, answers, refused) in rows {
        let dir = critiqued_project(&workflow, answers, false);
        let root = dir.path();

        println!("row: {name}");
        match refused {
            Some(refusal) => assert_each_run_entry_refuses(root, &path, &[], refusal, 0),
            None => {
                for (minted, entry) in (1..).zip(RUN_ENTRIES) {
                    assert_past_the_run_start(root, &path, entry, &["--fresh"], (0, minted));
                }
            }
        }
    }
}
