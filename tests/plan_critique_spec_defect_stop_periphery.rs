//! Spec 112, criterion 5 (A SPEC DEFECT STOPS THE RUN AT PLAN-CRITIQUE), at the periphery: the
//! stop driven through real `rigger step` processes over a project's on-disk store, each spawn
//! answered the way a courier answers it (`rigger emit --spawn` for the planner's proposal,
//! `rigger result` for every output).
//!
//! The conductor's own tests drive `conductor::run` in one process over an in-memory store. What
//! they cannot see is what this file pins: that the halt reaches the step's printed `halted`
//! field through `rigger step`, that it is a condition of the stopping PROCESS only (a fresh step
//! process finds the gate terminal and reports no halt), that a step process re-entering a stop
//! a crash interrupted completes it from the persisted log alone, that the stop reads the
//! current run's slice of a store holding an earlier run, that the spec it names is the
//! latest run's `RunStarted.spec` exactly as recorded, through the exported
//! `run::current_run_spec_path` the conductor calls across the crate boundary, that only a
//! stopping reject whose cause is `spec-ambiguity` stops (through the exported
//! `spawn::Adjudication::is_spec_ambiguity`), and that the prompt `rigger prompt` serves every
//! round's adjudicator - the re-plan round whose reject stops included - asks for that cause on
//! exactly the defect the stop is for.
//!
//! Every run here is minted through `run_store::start_fresh` with a spec path and adopted by the
//! step (no `--spec`, so the step's criteria are empty and match the minted run's), never at a
//! CLI mint site: a mint is criterion 2's surface, and an adopted run is never refused there. The
//! one exception is the relaunch the halt directs (Design *Relaunch*), where the stopped run meets
//! criterion 2's refusal and criterion 1's verb: its new run is begun by `rigger step --spec
//! --fresh` on purpose, the boundary only the three criteria together hold.

mod common;

use common::cli::{
    read_run_events, record_clean_critique, refused_new_run, run_payloads, run_rigger,
    run_rigger_ok, step_line, temp_repoless_project, with_run_store, write_scaffold,
};
use common::fixtures::{
    critique_reject, keyed_index, keyed_payload, spec_defect_halt_text, stop_records,
    the_stop_records, DAG_CRITIQUE_VERDICT_PARAGRAPH, STOP_ESCALATED_KEY, STOP_LESSON_KEY,
    STOP_SPEC_DEFECT_KEY,
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

/// The one criterion of [`SPEC`], which the planner's unit ([`plan`]) covers.
const CRITERION: &str = "the widget renderer is implemented";

/// [`SPEC`]'s text as the operator amends it after the stop: a Design sentence closing the
/// defect, and [`CRITERION`].
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

/// Mint one run per spec in `specs`, in order, each with no criteria: the step adopts the last.
fn launch(root: &Path, specs: &[&str]) {
    with_run_store(root, |store| {
        for spec in specs {
            rigger::run_store::start_fresh(store, &[], "", "", "", spec).unwrap();
        }
    });
}

/// One real `rigger step` over `root`, parsed.
fn step(root: &Path, what: &str) -> Value {
    let line = step_line(root, what);
    serde_json::from_str(&line).unwrap_or_else(|e| panic!("{what}: {e}; line: {line}"))
}

/// The spawn ids `step` parked.
fn wave(step: &Value) -> Vec<String> {
    step["wave"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["id"].as_str().unwrap().to_string())
        .collect()
}

/// Step once, asserting it parks exactly `spawns` and reports no halt.
fn parks(root: &Path, spawns: &[&str]) {
    let s = step(root, &format!("the step parking {spawns:?}"));
    assert_eq!(
        (wave(&s), s.get("halted")),
        (spawns.iter().map(|id| id.to_string()).collect(), None),
        "the step parks {spawns:?} and does not halt"
    );
}

/// Answer the parked planner spawn `spawn` as a courier does: its proposal, then its result.
fn plan(root: &Path, spawn: &str) {
    let unit = json!({
        "id": "u-widget",
        "agent": "worker",
        "criterion": CRITERION,
        "needs": [],
    });
    run_rigger_ok(
        root,
        &["emit", "--spawn", spawn, "UnitProposed", &unit.to_string()],
    );
    run_rigger_ok(root, &["result", spawn, "proposed the DAG"]);
}

/// Answer critique round `k`'s parked adversary.
fn review(root: &Path, k: u32) {
    run_rigger_ok(
        root,
        &[
            "result",
            &format!("plan-critique/adversary#{k}"),
            "reviewed the DAG",
        ],
    );
}

/// The run's first step answered: it parks the planner and, in the same producer prelude,
/// round 0's adversary.
fn plan_and_review_first(root: &Path) {
    parks(root, &["plan-critique/adversary#0", "plan/implementer#0"]);
    plan(root, "plan/implementer#0");
    review(root, 0);
}

/// The re-plan at attempt `k` and the review of the round it feeds, each parked alone and
/// answered.
fn re_plan_and_review(root: &Path, k: u32) {
    let re_plan = format!("plan/replan#{k}");
    parks(root, &[&re_plan]);
    plan(root, &re_plan);
    parks(root, &[&format!("plan-critique/adversary#{k}")]);
    review(root, k);
}

/// Step to critique round `k`'s adjudicator and answer it with `verdict`, returning the prompt it
/// was served as a courier fetches it (`rigger prompt`).
fn adjudicate(root: &Path, k: u32, verdict: &str) -> String {
    let adjudicator = format!("plan-critique/adjudicator#{k}");
    parks(root, &[&adjudicator]);
    let served = run_rigger_ok(root, &["prompt", &adjudicator]);
    run_rigger_ok(root, &["result", &adjudicator, verdict]);
    served
}

/// The ids of every spawn `events` requested, in log order.
fn requested(events: &[Event]) -> Vec<String> {
    rigger::spawn::requests(events)
        .unwrap()
        .into_iter()
        .map(|r| r.id)
        .collect()
}

/// A project whose latest run, launched on [`SPEC`] after one on [`EARLIER_SPEC`], is driven up
/// to the step after its second critique round: a `spec-ambiguity` reject, the re-plan it drove,
/// and `second`, the round-1 reject line, answered but not yet stepped over. Asserts on the way
/// that the first reject re-plans as before. Returns the project and the prompt each round's
/// adjudicator was served.
fn answered_second_reject(second: &str) -> (tempfile::TempDir, [String; 2]) {
    let dir = critique_project();
    let root = dir.path();
    launch(root, &[EARLIER_SPEC, SPEC]);
    plan_and_review_first(root);
    let first_round = adjudicate(root, 0, &spec_ambiguity_first());
    // A first `spec-ambiguity` reject re-plans as before: no stop record, no halt.
    re_plan_and_review(root, 1);
    assert_eq!(
        stop_records(&read_run_events(root)),
        Vec::new(),
        "a first spec-ambiguity reject records no stop"
    );
    let re_plan_round = adjudicate(root, 1, second);
    (dir, [first_round, re_plan_round])
}

/// [`answered_second_reject`] up to its stopping step: the second reject is `spec-ambiguity` too,
/// upholding the `adv-2` and `adv-3` the halt names.
fn answered_second_spec_ambiguity_reject() -> (tempfile::TempDir, [String; 2]) {
    answered_second_reject(&critique_reject("spec-ambiguity", &["adv-2", "adv-3"]))
}

/// Given a run on [`SPEC`] whose plan-critique gate rejected with `spec-ambiguity`, re-planned,
/// and rejected with `spec-ambiguity` again, when the operator steps, then the step parks
/// nothing, halts with the amend route naming the spec as recorded and the second reject's
/// upheld findings, names the gate escalated, and the log carries the stop's three records - and
/// no re-plan after the stopping reject.
#[test]
fn a_spec_ambiguity_reject_after_a_re_plan_that_did_not_clear_it_halts_the_step() {
    let (dir, _) = answered_second_spec_ambiguity_reject();
    let root = dir.path();
    let s = step(root, "the stopping step");
    assert_eq!(
        (
            wave(&s),
            s["halted"].as_str(),
            s["escalated"].clone(),
            s["attention"]
                .as_array()
                .unwrap()
                .iter()
                .map(|a| (a["kind"].as_str().unwrap(), a["unit"].as_str().unwrap()))
                .collect::<Vec<_>>(),
        ),
        (
            Vec::new(),
            Some(halt().as_str()),
            json!(["plan-critique"]),
            vec![
                ("escalated", "plan-critique"),
                ("worker-death-recurred", "plan-critique"),
            ],
        ),
        "the step parks nothing, halts with the amend route and names the gate escalated; its \
         attention is the gate's escalation, never a halted entry"
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
            json!({"id": "plan-critique"}),
        ),
        "the lesson is about the spec as recorded, the SpecDefect carries the halt, the \
         escalation is the gate's own"
    );
    assert_eq!(
        (
            requested(&events)
                .into_iter()
                .filter(|id| id.starts_with("plan/"))
                .collect::<Vec<_>>(),
            rigger::run::current_run_spec_path(&events),
        ),
        (
            vec![
                "plan/implementer#0".to_string(),
                "plan/replan#1".to_string()
            ],
            SPEC.to_string(),
        ),
        "the stopping reject re-plans nothing, and the spec the halt names is the latest run's \
         as the exported lookup reads it"
    );
}

/// Given a stopped run, when the operator steps again in a fresh process, then nothing parks,
/// no halt is reported (the halt was the stopping process's), and the stop's records stand once
/// each.
#[test]
fn a_later_step_finds_the_stopped_gate_terminal_and_reports_no_halt() {
    let (dir, _) = answered_second_spec_ambiguity_reject();
    let root = dir.path();
    step(root, "the stopping step");
    let later = step(root, "a later step");
    assert_eq!(
        (
            wave(&later),
            later.get("halted"),
            later["escalated"].clone(),
            stop_records(&read_run_events(root)),
        ),
        (
            Vec::new(),
            None,
            json!(["plan-critique"]),
            the_stop_records()
        ),
        "the gate is terminal: nothing parks, no halt, the stop recorded once"
    );
}

/// Given a stop a crash interrupted - after the stopping reject's `UnitFailed`, or after its
/// `SpecDefect` - when a fresh step process enters a store holding that log, then it completes
/// the stop from the log alone: each record once, the halt reported, and no spawn requested.
#[test]
fn a_step_re_entering_a_crashed_stop_completes_it_in_a_fresh_process() {
    let (dir, _) = answered_second_spec_ambiguity_reject();
    step(dir.path(), "the stopping step");
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
        let s = step(resumed.path(), "the re-entering step");
        let events = read_run_events(resumed.path());
        assert_eq!(
            (
                wave(&s),
                s["halted"].as_str(),
                stop_records(&events),
                requested(&events),
            ),
            (
                Vec::new(),
                Some(halt().as_str()),
                the_stop_records(),
                requested_before
            ),
            "re-entered before {crashed_before}: the stop completes, each record once, without \
             a spawn"
        );
    }
}

/// Given a store whose earlier run's gate recorded a `spec-ambiguity` reject at attempt 0 and
/// re-planned, and a new run whose gate rejected with `decomposition-conflict` at attempt 0,
/// re-planned, then rejected with `spec-ambiguity` at attempt 1, when the operator steps, then
/// the new run re-plans again: the earlier run's reject is never read across the run boundary.
#[test]
fn an_earlier_runs_spec_ambiguity_reject_is_never_read_across_the_run_boundary() {
    let dir = critique_project();
    let root = dir.path();
    launch(root, &[EARLIER_SPEC]);
    plan_and_review_first(root);
    adjudicate(root, 0, &spec_ambiguity_first());
    parks(root, &["plan/replan#1"]);

    launch(root, &[SPEC]);
    plan_and_review_first(root);
    // A defect a re-plan can fix.
    adjudicate(
        root,
        0,
        &critique_reject("decomposition-conflict", &["adv-4"]),
    );
    re_plan_and_review(root, 1);
    adjudicate(root, 1, &spec_ambiguity_first());
    parks(root, &["plan/replan#2"]);
    assert_eq!(
        stop_records(&read_run_events(root)),
        Vec::new(),
        "only one of the new run's rejects is a spec-ambiguity one, so nothing stops"
    );
}

/// Given a run whose plan-critique gate rejected with `spec-ambiguity`, re-planned, and then
/// rejected with `decomposition-conflict` - a defect a re-plan can fix - when the operator steps,
/// then the gate re-plans again: the step parks the next re-plan, reports no halt, and the log
/// carries no stop record.
#[test]
fn a_re_plannable_reject_after_a_spec_ambiguity_one_re_plans_again() {
    let (dir, _) = answered_second_reject(&critique_reject("decomposition-conflict", &["adv-2"]));
    let root = dir.path();
    parks(root, &["plan/replan#2"]);
    assert_eq!(
        stop_records(&read_run_events(root)),
        Vec::new(),
        "only the stopping reject's own spec-ambiguity cause stops, so nothing stops"
    );
}

/// Given a run whose plan-critique adjudicator is parked at its first round and again at the
/// re-plan round its first reject drove, when each fetches its prompt as a courier does (`rigger
/// prompt`), then each prompt carries the DAG critique verdict paragraph exactly once: every
/// round's adjudicator - the re-plan round's, whose reject is the one that stops, included - is
/// asked for `spec-ambiguity`, spelled as the stop reads it, only on a defect in a criterion's
/// own text, and for `decomposition-conflict` on every defect a re-plan can fix and when unsure.
/// The stop that round's `spec-ambiguity` reject then drives is pinned by
/// [`a_spec_ambiguity_reject_after_a_re_plan_that_did_not_clear_it_halts_the_step`].
#[test]
fn every_critique_round_asks_its_adjudicator_for_the_cause_the_stop_reads() {
    let (_dir, served) = answered_second_spec_ambiguity_reject();
    assert_eq!(
        served
            .each_ref()
            .map(|prompt| prompt.matches(DAG_CRITIQUE_VERDICT_PARAGRAPH).count()),
        [1, 1],
        "the first round's and the re-plan round's prompts each carry the verdict paragraph \
         once:\n{}",
        served.join("\n---\n")
    );
}

/// Given a run its plan-critique gate stopped on a spec defect, when the operator relaunches as
/// the halt directs - amends the spec, then begins a new run on it with `--fresh` - then the new
/// run is refused as not critiqued, on stderr alone, and nothing is appended; and once `rigger
/// critique` has recorded a clean critique of the amended text, the same command begins the new
/// run, whose first step parks the planner and the gate's round-0 adversary with no halt, no
/// escalation and no attention: the stopped run's records stand once and never cross the run
/// boundary.
#[test]
fn a_stopped_run_relaunches_once_its_amended_spec_is_critiqued_and_the_new_run_starts_clean() {
    let (dir, _) = answered_second_spec_ambiguity_reject();
    let root = dir.path();
    assert_eq!(
        step(root, "the stopping step")["halted"].as_str(),
        Some(halt().as_str()),
        "the run is stopped on its spec defect"
    );
    std::fs::create_dir_all(root.join("specs")).unwrap();
    std::fs::write(root.join(SPEC), AMENDED_SPEC).unwrap();
    let relaunch = || run_rigger(root, &["step", "--spec", SPEC, "--fresh"]);

    let stopped = read_run_events(root).len();
    let (out, err, ok) = relaunch();
    assert_eq!(
        (
            ok,
            out.as_str(),
            err.ends_with(&refused_new_run("rigger step", "specs/widget.md", None)),
            read_run_events(root).len(),
        ),
        (false, "", true, stopped),
        "the relaunch on uncritiqued amended text is refused on stderr alone, naming the spec \
         repo-relative, and appends nothing; stderr:\n{err}"
    );

    record_clean_critique(root, SPEC);
    let (out, err, ok) = relaunch();
    assert!(
        ok,
        "the clean critique lets the relaunch begin its new run; stderr:\n{err}"
    );
    let first: Value = serde_json::from_str(out.trim())
        .unwrap_or_else(|e| panic!("the relaunch prints its step line: {e}; stdout: {out}"));
    let events = read_run_events(root);
    let runs = run_payloads(root, "RunStarted");
    assert_eq!(
        (
            wave(&first),
            [
                first.get("halted"),
                first.get("escalated"),
                first.get("attention")
            ],
            runs.iter()
                .map(|run| (run["spec"].clone(), run["criteria"].clone()))
                .collect::<Vec<_>>(),
            stop_records(&events),
            stop_records(rigger::run::current_run(&events)),
        ),
        (
            vec![
                "plan-critique/adversary#0".to_string(),
                "plan/implementer#0".to_string()
            ],
            [None, None, None],
            vec![
                (json!(EARLIER_SPEC), json!([])),
                (json!(SPEC), json!([])),
                (json!(SPEC), json!([CRITERION])),
            ],
            the_stop_records(),
            Vec::new(),
        ),
        "the new run on the amended spec starts its gate afresh: the planner and round 0's \
         adversary park, nothing halts, escalates or asks for attention, and the stopped run's \
         three records stand once, all before the new run's boundary"
    );
}
