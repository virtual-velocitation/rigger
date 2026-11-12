//! Spec 112, criterion 5 (A SPEC DEFECT STOPS THE RUN AT PLAN-CRITIQUE), at the periphery: the
//! stop driven through real `rigger run` processes over a project's on-disk store, each spawn
//! answered by the fake host: the planner records its proposal through `rigger emit --spawn`, and
//! every other spawn's answer is its session's result.
//!
//! The conductor's own tests drive `conductor::run` in one process over an in-memory store. What
//! they cannot see is what this file pins: that the halt reaches the `halted:` line `rigger run`
//! prints, that it is carried by the log, never by the stopping process (a relaunched run over
//! the same root finds the gate terminal and reports the same halt), that a run re-entering a
//! stop a crash interrupted completes it from the persisted log alone, that the stop reads the
//! current run's slice of a store holding an earlier run, that the spec it names is the latest
//! run's `RunStarted.spec` exactly as recorded, through the exported `run::current_run_spec_path`
//! the conductor calls across the crate boundary, that only a stopping reject whose cause is
//! `spec-ambiguity` stops (through the exported `spawn::Adjudication::is_spec_ambiguity`), and
//! that the task a run serves every round's adjudicator - the re-plan round whose reject stops
//! included - asks for that cause on exactly the defect the stop is for.
//!
//! Every run here is minted through `run_store::start_fresh` with a spec path and adopted by
//! `rigger run`, never at a CLI mint site: a mint is criterion 2's surface, and an adopted run is
//! never refused there. A run minted with no criteria is driven by a `rigger run` naming no spec
//! (its criteria are empty and match), one minted on [`SPEC`]'s criteria by `rigger run` [`SPEC`],
//! as a driver resumes a run it began on its spec. Each run ends where the fake host fails the
//! first spawn its arm does not answer ([`critique_arm`]), unless the stop halts it first. The one
//! exception to adoption is the relaunch the halt directs (Design *Relaunch*), where the stopped
//! run meets criterion 2's refusal and criterion 1's verb: its new run is begun by `rigger run`
//! [`SPEC`] - with `--fresh`, and with none after an amendment that leaves the criteria equal - the
//! boundary only the three criteria together hold.

use crate::common;

use common::cli::Refusal::Unexhausted;
use common::cli::{
    emit_unit_proposed, read_run_events, record_clean_critique, refused_new_run, run_on_arms,
    run_payloads, temp_repoless_project, widget_unit, with_run_store, write_scaffold,
};
use common::fixtures::{
    critique_reject, dag_critique_verdict_paragraph, keyed_index, keyed_payload, served_task,
    spec_defect_halt_text, stop_records, the_stop_records, RECORD_EACH_LAUNCH, STOP_ESCALATED_KEY,
    STOP_LESSON_KEY, STOP_SPEC_DEFECT_KEY, WIDGET_CRITERION,
};
use rigger::conductor::STREAM;
use rigger::eventstore::{Event, ExpectedRevision};
use serde_json::{json, Value};
use std::path::Path;

/// The spec the stopping run is launched with, in a spelling no normalization would keep: the
/// halt names it exactly as recorded.
const SPEC: &str = "./specs/widget.md";

/// The spec an earlier run in the same store was launched with.
const EARLIER_SPEC: &str = "specs/old.md";

/// [`SPEC`]'s text as a run is launched on it: a Design sentence and its one criterion,
/// [`WIDGET_CRITERION`], which the planner's unit ([`widget_unit`]) covers.
const ORIGINAL_SPEC: &str = "# Widget\n\n## Design\n\nThe renderer draws widgets.\n\n\
                             ## Done when\n\n- [ ] the widget renderer is implemented\n";

/// [`SPEC`]'s text as the operator amends it after the stop: a Design sentence closing the
/// defect, and [`WIDGET_CRITERION`].
const AMENDED_SPEC: &str = "# Widget\n\n## Design\n\nThe renderer draws one widget per call.\n\n\
                            ## Done when\n\n- [ ] the widget renderer is implemented\n";

/// The halt a stop on [`SPEC`] reports after a second `spec-ambiguity` reject that upheld
/// `adv-2` and `adv-3`.
fn halt() -> String {
    spec_defect_halt_text(SPEC, "adv-2, adv-3")
}

/// The first `spec-ambiguity` reject line of the stopping run's gate.
fn spec_ambiguity_first() -> String {
    critique_reject("spec-ambiguity", &["adv-1"])
}

/// A plan stage producing the DAG, the plan-critique gate over it, and the fan-out implement
/// template the gate releases, which covers each criterion of a run begun on a spec (the relaunch)
/// and none of a run with no criteria. `max_retries: 3` leaves the gate a re-plan after its second
/// reject, so a stop - not the remediation bound - is what ends the round that follows it.
const WORKFLOW: &str = "defaults:\n  grounder: nop\n  budget: 60\n  max_retries: 3\n\
                        stages:\n  plan:\n    agent: planner\n    produces: dag\n  \
                        plan-critique:\n    needs: [plan]\n    adversary: adversary\n    \
                        adjudicator: judge\n  implement:\n    needs: [plan-critique]\n    \
                        agent: worker\n    strategy: fan-out\n    on_pass: none\n";

/// A repo-less project scaffolded with [`WORKFLOW`] and its four agents.
fn critique_project() -> tempfile::TempDir {
    let dir = temp_repoless_project();
    let ids = ["planner", "worker", "adversary", "judge"];
    let definitions = ids.map(|id| {
        format!("---\nid: {id}\nmodel: sonnet\ntools: [Read]\nisolation: none\n---\nDo the work.\n")
    });
    let agents: Vec<(&str, &str)> = ids
        .into_iter()
        .zip(definitions.iter().map(String::as_str))
        .collect();
    write_scaffold(dir.path(), &agents, WORKFLOW);
    dir
}

/// Mint one run per spec in `specs`, in order, each on `criteria`: the next run adopts the last.
fn launch(root: &Path, specs: &[&str], criteria: &[String]) {
    with_run_store(root, |store| {
        for spec in specs {
            rigger::run_store::start_fresh(store, criteria, "", "", "", spec).unwrap();
        }
    });
}

/// The arm answering every spawn of [`critique_project`]'s agents, which share one prompt, each
/// launch on record ([`RECORD_EACH_LAUNCH`]): the plan's planner and each re-plan up to
/// `re_plans` propose the widget unit, each round's adversary reviews, round `k`'s adjudicator
/// answers `verdicts[k]`, and the widget's implementer reports it built. A re-plan past
/// `re_plans` fails, which ends the run there.
fn critique_arm(verdicts: &[&str], re_plans: u32) -> String {
    let planners = std::iter::once("plan/implementer#0".to_string())
        .chain((1..=re_plans).map(|k| format!("plan/replan#{k}")))
        .collect::<Vec<_>>()
        .join("|");
    let adjudicators: String = verdicts
        .iter()
        .enumerate()
        .map(|(k, verdict)| {
            format!("      plan-critique/adjudicator#{k}) printf '%s' '{verdict}' ;;\n")
        })
        .collect();
    format!(
        r#"  *"Do the work."*)
{RECORD_EACH_LAUNCH}    case "$RIGGER_SPAWN" in
      {planners})
{propose}    echo "proposed the DAG" ;;
      plan-critique/adversary#*) echo "reviewed the DAG" ;;
{adjudicators}      u-widget/*) echo "built the widget" ;;
      *) exit 1 ;;
    esac
    ;;
"#,
        propose = emit_unit_proposed(&widget_unit("worker")),
    )
}

/// What one `rigger run` left: its stdout, its stderr, and the fake host's directory, whose
/// `bin/launches` holds each launch it served.
struct Ran {
    out: String,
    err: String,
    fake: tempfile::TempDir,
}

impl Ran {
    /// The task the run served `spawn`.
    fn served(&self, spawn: &str) -> String {
        served_task(&self.fake.path().join("bin/launches"), spawn)
    }
}

/// One real `rigger run` over `root` answered by `arms`: on [`SPEC`] when the latest run was begun
/// on criteria, else naming no spec.
fn run(root: &Path, arms: &str) -> Ran {
    let on_spec = run_payloads(root, "RunStarted")
        .last()
        .is_some_and(|run| run["criteria"] != json!([]));
    let args: &[&str] = if on_spec { &["run", SPEC] } else { &["run"] };
    let (out, err, _ok, fake) = run_on_arms(root, args, &root.join("scratch"), arms);
    Ran { out, err, fake }
}

/// The `halted:` line a run stopped on [`SPEC`]'s defect prints last.
fn halted_line() -> String {
    format!("halted: {}\n", halt())
}

/// The line `rigger run` prints when every unit is integrated.
const DONE: &str = "done: every unit integrated\n";

/// The run-state line `rigger run` prints for the gate escalated by its stop.
const GATE_ESCALATED: &str = "  plan-critique        escalated\n";

/// The ids of every spawn `events` requested, in log order.
fn requested(events: &[Event]) -> Vec<String> {
    rigger::spawn::requests(events)
        .unwrap()
        .into_iter()
        .map(|r| r.id)
        .collect()
}

/// A project holding [`ORIGINAL_SPEC`] at [`SPEC`] whose latest run, launched on [`SPEC`] after
/// one on [`EARLIER_SPEC`], both on `criteria`, one `rigger run` drives through two critique
/// rounds: a `spec-ambiguity` reject, the re-plan it drives, and `second`, the round-1 reject
/// line, the second re-plan failing should the gate ask for one. Asserts on the way that the
/// first reject re-plans. Returns the project and what the run left.
fn driven_through_a_second_reject_on(
    criteria: &[String],
    second: &str,
) -> (tempfile::TempDir, Ran) {
    let dir = critique_project();
    let root = dir.path();
    std::fs::create_dir_all(root.join("specs")).unwrap();
    std::fs::write(root.join(SPEC), ORIGINAL_SPEC).unwrap();
    launch(root, &[EARLIER_SPEC, SPEC], criteria);
    let ran = run(root, &critique_arm(&[&spec_ambiguity_first(), second], 1));
    assert!(
        requested(&read_run_events(root)).contains(&"plan/replan#1".to_string()),
        "a first spec-ambiguity reject re-plans; stderr:\n{}",
        ran.err
    );
    (dir, ran)
}

/// [`driven_through_a_second_reject_on`] runs launched with no criteria.
fn driven_through_a_second_reject(second: &str) -> (tempfile::TempDir, Ran) {
    driven_through_a_second_reject_on(&[], second)
}

/// The second reject's `spec-ambiguity` line, upholding the `adv-2` and `adv-3` the halt names.
fn spec_ambiguity_second() -> String {
    critique_reject("spec-ambiguity", &["adv-2", "adv-3"])
}

/// [`driven_through_a_second_reject`] with a second `spec-ambiguity` reject: the run that stops.
fn stopped_on_a_second_spec_ambiguity_reject() -> (tempfile::TempDir, Ran) {
    driven_through_a_second_reject(&spec_ambiguity_second())
}

/// Given a run on [`SPEC`] whose plan-critique gate rejected with `spec-ambiguity`, re-planned,
/// and rejected with `spec-ambiguity` again, when `rigger run` drives it, then the run launches
/// nothing more, prints the gate escalated and halts with the amend route naming the spec as
/// recorded and the second reject's upheld findings, and the log carries the stop's three
/// records - and no re-plan after the stopping reject.
#[test]
fn a_spec_ambiguity_reject_after_a_re_plan_that_did_not_clear_it_halts_the_run() {
    let (dir, ran) = stopped_on_a_second_spec_ambiguity_reject();
    let root = dir.path();
    assert!(
        ran.out.ends_with(&halted_line()) && ran.out.contains(GATE_ESCALATED),
        "the run prints the gate escalated and halts with the amend route; stdout:\n{}\n\
         stderr:\n{}",
        ran.out,
        ran.err
    );
    let events = read_run_events(root);
    assert_eq!(
        stop_records(&events),
        the_stop_records(),
        "the lesson, the SpecDefect and the escalation, in that order, each under its key"
    );
    let lesson = keyed_payload(&events, STOP_LESSON_KEY);
    assert_eq!(
        (
            &lesson["about"],
            keyed_payload(&events, STOP_SPEC_DEFECT_KEY),
            keyed_payload(&events, STOP_ESCALATED_KEY),
        ),
        (
            &json!([SPEC]),
            json!({"reason": halt()}),
            json!({"id": "plan-critique", "reason": halt()}),
        ),
        "the lesson is about the spec as recorded; the SpecDefect and the gate's own escalation \
         carry the halt"
    );
    assert_eq!(
        (
            requested(rigger::run::current_run(&events)),
            rigger::run::current_run_spec_path(&events),
        ),
        (
            [
                "plan/implementer#0",
                "plan-critique/adversary#0",
                "plan-critique/adjudicator#0",
                "plan/replan#1",
                "plan-critique/adversary#1",
                "plan-critique/adjudicator#1",
            ]
            .map(String::from)
            .to_vec(),
            SPEC.to_string(),
        ),
        "the stopping reject re-plans and launches nothing, and the spec the halt names is the \
         latest run's as the exported lookup reads it"
    );
}

/// Given a stopped run, when the operator relaunches `rigger run` over the same root, then it
/// launches nothing, prints the gate escalated and reports the halt again (it is carried by the
/// log, never by the stopping process), and the stop's records stand once each.
#[test]
fn a_relaunched_run_finds_the_stopped_gate_terminal_and_reports_its_halt() {
    let (dir, _) = stopped_on_a_second_spec_ambiguity_reject();
    let root = dir.path();
    let stopped = requested(&read_run_events(root));
    let later = run(root, "");
    let events = read_run_events(root);
    assert_eq!(
        (
            later.out.ends_with(&halted_line()),
            later.out.contains(GATE_ESCALATED),
            requested(&events),
            stop_records(&events),
        ),
        (true, true, stopped, the_stop_records()),
        "the gate is terminal: nothing launches, the halt is reported again, the stop recorded \
         once; stdout:\n{}\nstderr:\n{}",
        later.out,
        later.err
    );
}

/// Given a stop a crash interrupted - after the stopping reject's `UnitFailed`, or after its
/// `SpecDefect` - when a `rigger run` enters a store holding that log, then it completes the stop
/// from the log alone: each record once, the halt reported, and no spawn requested.
#[test]
fn a_run_re_entering_a_crashed_stop_completes_it_in_a_fresh_process() {
    let (dir, _) = stopped_on_a_second_spec_ambiguity_reject();
    let log = read_run_events(dir.path());
    for crashed_before in [STOP_LESSON_KEY, STOP_ESCALATED_KEY] {
        let at = keyed_index(&log, crashed_before);
        let resumed = critique_project();
        with_run_store(resumed.path(), |store| {
            for e in &log[..at] {
                store
                    .append(STREAM, ExpectedRevision::Any, std::slice::from_ref(e))
                    .unwrap();
            }
        });
        let requested_before = requested(&log[..at]);
        let ran = run(resumed.path(), "");
        let events = read_run_events(resumed.path());
        assert_eq!(
            (
                ran.out.ends_with(&halted_line()),
                stop_records(&events),
                requested(&events),
            ),
            (true, the_stop_records(), requested_before),
            "re-entered before {crashed_before}: the stop completes, each record once, without \
             a spawn; stdout:\n{}\nstderr:\n{}",
            ran.out,
            ran.err
        );
    }
}

/// Given a store whose earlier run's gate rejected with `spec-ambiguity` at attempt 0 and
/// re-planned, and a new run whose gate rejected with `decomposition-conflict` at attempt 0,
/// re-planned, then rejected with `spec-ambiguity` at attempt 1, when `rigger run` drives each,
/// then the new run re-plans again: the earlier run's reject is never read across the run
/// boundary.
#[test]
fn an_earlier_runs_spec_ambiguity_reject_is_never_read_across_the_run_boundary() {
    let dir = critique_project();
    let root = dir.path();
    launch(root, &[EARLIER_SPEC], &[]);
    run(root, &critique_arm(&[&spec_ambiguity_first()], 0));
    assert!(
        requested(&read_run_events(root)).contains(&"plan/replan#1".to_string()),
        "the earlier run's spec-ambiguity reject re-plans"
    );

    launch(root, &[SPEC], &[]);
    // A defect a re-plan can fix, then a first spec-ambiguity reject.
    let conflict = critique_reject("decomposition-conflict", &["adv-4"]);
    let ran = run(
        root,
        &critique_arm(&[&conflict, &spec_ambiguity_first()], 1),
    );
    let events = read_run_events(root);
    assert_eq!(
        (
            requested(rigger::run::current_run(&events)).last().cloned(),
            stop_records(&events),
            ran.out.contains("halted:"),
        ),
        (Some("plan/replan#2".to_string()), Vec::new(), false),
        "only one of the new run's rejects is a spec-ambiguity one, so it re-plans and nothing \
         stops; stdout:\n{}\nstderr:\n{}",
        ran.out,
        ran.err
    );
}

/// Given a run whose plan-critique gate rejected with `spec-ambiguity`, re-planned, and then
/// rejected with `decomposition-conflict` - a defect a re-plan can fix - when `rigger run` drives
/// it, then the gate re-plans again: the run launches the next re-plan, reports no halt, and the
/// log carries no stop record.
#[test]
fn a_re_plannable_reject_after_a_spec_ambiguity_one_re_plans_again() {
    let (dir, ran) =
        driven_through_a_second_reject(&critique_reject("decomposition-conflict", &["adv-2"]));
    let events = read_run_events(dir.path());
    assert_eq!(
        (
            requested(rigger::run::current_run(&events)).last().cloned(),
            stop_records(&events),
            ran.out.contains("halted:"),
        ),
        (Some("plan/replan#2".to_string()), Vec::new(), false),
        "only the stopping reject's own spec-ambiguity cause stops, so the gate re-plans and \
         nothing stops; stdout:\n{}\nstderr:\n{}",
        ran.out,
        ran.err
    );
}

/// Given a run whose plan-critique adjudicator is launched at its first round and again at the
/// re-plan round its first reject drove, then the task the run serves each carries the DAG
/// critique verdict paragraph of a workflow declaring no unit size cap, as [`WORKFLOW`] does,
/// exactly once: every round's adjudicator - the re-plan round's, whose reject is the one that
/// stops, included - is asked for `spec-ambiguity`, spelled as the stop reads it, only on a defect
/// in a criterion's own text, and for `decomposition-conflict` on every defect a re-plan can fix
/// and when unsure. The stop that round's `spec-ambiguity` reject then drives is pinned by
/// [`a_spec_ambiguity_reject_after_a_re_plan_that_did_not_clear_it_halts_the_run`].
#[test]
fn every_critique_round_asks_its_adjudicator_for_the_cause_the_stop_reads() {
    let (_dir, ran) = stopped_on_a_second_spec_ambiguity_reject();
    let served = [0, 1].map(|k| ran.served(&format!("plan-critique/adjudicator#{k}")));
    let paragraph = dag_critique_verdict_paragraph(0);
    assert_eq!(
        served
            .each_ref()
            .map(|task| task.matches(&paragraph).count()),
        [1, 1],
        "the first round's and the re-plan round's tasks each carry the verdict paragraph \
         once:\n{}",
        served.join("\n---\n")
    );
}

/// The relaunch the halt directs, over `root` whose stopped run's spec now holds its amended text:
/// `rigger run` [`SPEC`] plus `flags` is refused as not critiqued to exhaustion, on stderr alone,
/// naming the spec repo-relative and appending nothing; once `rigger critique` has recorded a
/// clean critique of the amended text, the same command begins the new run, which starts its gate
/// afresh at round 0 - the planner, then round 0's adversary and adjudicator, which approves -
/// and runs to done through the widget's implementer with no halt, the stopped run's records standing once, all before the new
/// run's boundary. `runs` is the spec and criteria of every run the store then holds.
fn assert_the_relaunch_begins_a_clean_run_once_critiqued(
    root: &Path,
    flags: &[&str],
    runs: Vec<(Value, Value)>,
) {
    let args: Vec<&str> = ["run", SPEC].iter().chain(flags).copied().collect();
    let relaunch = |arms: &str| run_on_arms(root, &args, &root.join("scratch"), arms);

    let stopped = read_run_events(root).len();
    let (out, err, ok, _fake) = relaunch("");
    assert_eq!(
        (
            ok,
            out.contains("refusing"),
            err.ends_with(&refused_new_run("rigger run", "specs/widget.md", Unexhausted)),
            read_run_events(root).len(),
        ),
        (false, false, true, stopped),
        "the relaunch {flags:?} on uncritiqued amended text is refused on stderr alone, naming the \
         spec repo-relative, and appends nothing; stdout:\n{out}\nstderr:\n{err}"
    );

    record_clean_critique(root, SPEC);
    let (out, err, ok, _fake) = relaunch(&critique_arm(&[r#"{"verdict":"approve"}"#], 0));
    let events = read_run_events(root);
    assert_eq!(
        (
            requested(rigger::run::current_run(&events)),
            (ok, out.contains("halted:"), out.contains(DONE)),
            run_payloads(root, "RunStarted")
                .iter()
                .map(|run| (run["spec"].clone(), run["criteria"].clone()))
                .collect::<Vec<_>>(),
            stop_records(&events),
            stop_records(rigger::run::current_run(&events)),
        ),
        (
            [
                "plan/implementer#0",
                "plan-critique/adversary#0",
                "plan-critique/adjudicator#0",
                "u-widget/implementer#0",
            ]
            .map(String::from)
            .to_vec(),
            (true, false, true),
            runs,
            the_stop_records(),
            Vec::new(),
        ),
        "the new run on the amended spec starts its gate afresh at round 0 and nothing halts, \
         the run reaches done, and the stopped run's three records stand once, all before the \
         new run's boundary; \
         stdout:\n{out}\nstderr:\n{err}"
    );
}

/// Given a run its plan-critique gate stopped on a spec defect, when the operator amends the spec
/// and relaunches on it with `--fresh`, then the relaunch is refused until the amended text is
/// critiqued and then begins a new run that starts clean.
#[test]
fn a_stopped_run_relaunches_once_its_amended_spec_is_critiqued_and_the_new_run_starts_clean() {
    let (dir, ran) = stopped_on_a_second_spec_ambiguity_reject();
    let root = dir.path();
    assert!(
        ran.out.ends_with(&halted_line()),
        "the run is stopped on its spec defect; stdout:\n{}",
        ran.out
    );
    std::fs::write(root.join(SPEC), AMENDED_SPEC).unwrap();
    assert_the_relaunch_begins_a_clean_run_once_critiqued(
        root,
        &["--fresh"],
        vec![
            (json!(EARLIER_SPEC), json!([])),
            (json!(SPEC), json!([])),
            (json!(SPEC), json!([WIDGET_CRITERION])),
        ],
    );
}

/// Given a run begun on [`SPEC`]'s criteria that its plan-critique gate stopped on a spec defect,
/// when the operator amends only the spec's Design, its criteria unchanged, and relaunches with no
/// `--fresh`, then the stopped run is never adopted: the relaunch is refused until the amended text
/// is critiqued, and then begins a new run that starts clean.
#[test]
fn a_run_stopped_on_its_specs_criteria_relaunches_with_no_fresh_after_a_design_amendment() {
    let criteria = vec![WIDGET_CRITERION.to_string()];
    let (dir, ran) = driven_through_a_second_reject_on(&criteria, &spec_ambiguity_second());
    let root = dir.path();
    assert!(
        ran.out.ends_with(&halted_line()),
        "the run begun on the spec's criteria is stopped on its spec defect; stdout:\n{}",
        ran.out
    );
    std::fs::write(root.join(SPEC), AMENDED_SPEC).unwrap();
    assert_the_relaunch_begins_a_clean_run_once_critiqued(
        root,
        &[],
        vec![
            (json!(EARLIER_SPEC), json!([WIDGET_CRITERION])),
            (json!(SPEC), json!([WIDGET_CRITERION])),
            (json!(SPEC), json!([WIDGET_CRITERION])),
        ],
    );
}
