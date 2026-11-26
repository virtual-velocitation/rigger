//! Spec 135, THE PLANNER RECORDS COMPLEXITY, at the periphery: a project's plan-critique gate
//! driven by real `rigger run` processes over its on-disk store, each spawn answered by the fake
//! host: each planner spawn records its proposal through `rigger emit --spawn`, and every other
//! spawn's answer is its session's result.
//!
//! The conductor's own tests fold hand-built proposals over an in-memory store and render the
//! critique prompt over a stub driver. What they cannot see is what this file pins: that a
//! planner's `complexity` and `complexity_reason` reach the log through the real record courier
//! exactly as the planner wrote them, a refused pair included; that the step and reason the run
//! takes are the ones its gate's adjudicator is served, round by round, across the re-plan a reject
//! drives; that a refused pair's `unassessed-complexity` record lands on the on-disk log once,
//! under its replay key, over every harvest the run makes; and that a second `rigger run` over the
//! same store, which re-enters the run with the widget integrated, records it no more and takes
//! nothing from a proposal logged after the widget started.
//!
//! Every run is minted through `run_store::start_fresh` on [`WIDGET_SPEC`]'s one criterion and
//! adopted by a `rigger run` naming that spec, so the planner's widget unit, which supersedes the
//! criterion's baseline, is the DAG's one unit; each run is bounded, so a run still waiting past
//! its bound fails its test rather than hanging the suite.

use crate::common;

use common::cli::{
    critique_arm_proposing, critique_project, current_run_id_of, how_the_run_ended,
    propose_unit_via_rigger_emit, read_run_events, run_on_arms_beside, widget_unit, with_run_store,
};
use common::fixtures::{
    critique_reject, dag_complexity, dag_critique_prompt, dag_re_review_prompt, pair_of,
    served_task, unassessed_complexity_record, unknown_step, DAG_DISJOINT_OVERLAP, DAG_UNASSESSED,
    WIDGET_CRITERION, WIDGET_SPEC, WIDGET_UNIT,
};
use rigger::conductor::{META_REPLAY_KEY, META_SPAWN, TYPE_UNIT_PROPOSED};
use rigger::config::Complexity;
use rigger::contextgraph::TYPE_DECISION_MADE;
use rigger::driver::claude_code::framed;
use rigger::eventstore::Event;
use serde_json::{json, Value};

/// A reason as a planner may write it, with quotes and a backslash the record courier and the
/// critique row carry through.
const REASON: &str = r#"the renderer is one "pure" function over a fixed C:\widgets grammar"#;

/// A later reason, holding a look-alike of a critique row placeholder that the row never fills.
const LATER_REASON: &str = "the renderer threads its error type through {radius} callers";

/// Where each project holds [`WIDGET_SPEC`], the spec its runs are launched on.
const SPEC: &str = "specs/widget.md";

/// The planner spawns of a gate that rejects once: the plan's planner, then the re-plan the
/// reject drives.
const PLANNERS: [&str; 2] = ["plan/implementer#0", "plan/replan#1"];

/// The spawn that records a proposal after the widget started, between the two runs.
const LATE_PLANNER: &str = "plan/replan#2";

/// The blast radius a unit of a project with no grounded source lists.
const NONE_GROUNDED: &str = "(none grounded)";

/// The line `rigger run` ends its run state with when every unit is integrated.
const DONE: &str = "done: every unit integrated";

/// The widget unit's proposal, handed to the project's `worker`, with `pair`'s fields merged in.
fn proposal(pair: &Value) -> Value {
    let mut unit = widget_unit("worker");
    let fields = pair.as_object().expect("a pair is a JSON object").clone();
    unit.as_object_mut().unwrap().extend(fields);
    unit
}

/// What a round's adjudicator is served when the widget's critique row shows `complexity`, the
/// re-plan round led by `reject`.
fn adjudicator_task(complexity: &str, reject: Option<&str>) -> String {
    let dag = dag_critique_prompt(
        &[[
            WIDGET_UNIT,
            WIDGET_CRITERION,
            "plan-critique",
            complexity,
            NONE_GROUNDED,
        ]],
        DAG_DISJOINT_OVERLAP,
    );
    framed(&reject.map_or(dag.clone(), |reject| dag_re_review_prompt(reject, &dag)))
}

/// Each `UnitProposed` of `events` as the spawn that recorded it and its payload, in log order.
fn proposals_on_record(events: &[Event]) -> Vec<(String, Value)> {
    events
        .iter()
        .filter(|e| e.type_ == TYPE_UNIT_PROPOSED)
        .map(|e| {
            (
                e.meta.get(META_SPAWN).cloned().unwrap_or_default(),
                serde_json::from_slice(&e.data).unwrap(),
            )
        })
        .collect()
}

/// Each `DecisionMade` of `events` as its replay key and its payload, in log order.
fn decisions_on_record(events: &[Event]) -> Vec<(Option<String>, Value)> {
    events
        .iter()
        .filter(|e| e.type_ == TYPE_DECISION_MADE)
        .map(|e| {
            (
                e.meta.get(META_REPLAY_KEY).cloned(),
                serde_json::from_slice(&e.data).unwrap(),
            )
        })
        .collect()
}

/// The log position of the `UnitProposed` the spawn `spawn` recorded in `events`.
fn proposed_at(events: &[Event], spawn: &str) -> u64 {
    events
        .iter()
        .find(|e| {
            e.type_ == TYPE_UNIT_PROPOSED
                && e.meta.get(META_SPAWN).map(String::as_str) == Some(spawn)
        })
        .unwrap_or_else(|| panic!("{spawn} recorded a proposal"))
        .position
}

/// One row: the given, the pair each of [`PLANNERS`] proposes the widget with, the widget's
/// critique cell each round's adjudicator is served, and each refused proposal as its index in
/// [`PLANNERS`] and what it carries.
struct Row {
    given: &'static str,
    pairs: [Value; 2],
    cells: [String; 2],
    refused: Vec<(usize, String)>,
}

/// Given a planner proposing the widget with the row's first pair and, after the gate's reject,
/// its re-plan proposing it again with the second, when `rigger run` drives the gate to its
/// approve and the widget to done, then each proposal is on the log as the planner wrote it, each
/// round's adjudicator is served the widget's step and reason the run took (or `unassessed`), and
/// the run's decisions are one `unassessed-complexity` record per refused proposal, keyed on its
/// log position. When a later proposal with a refused pair is recorded after the widget started
/// and a second `rigger run` re-enters the same run, it records nothing more.
#[test]
fn a_planners_step_and_reason_reach_the_critique_and_a_refused_pair_is_recorded_once() {
    let hard = |reason| dag_complexity(Complexity::Hard, reason);
    let rows = [
        Row {
            given: "a valid pair, then a refine with none",
            pairs: [pair_of(Complexity::Hard, REASON), json!({})],
            cells: [hard(REASON), hard(REASON)],
            refused: vec![],
        },
        Row {
            given: "a valid pair, then a refine with another valid pair",
            pairs: [
                pair_of(Complexity::Routine, REASON),
                pair_of(Complexity::Involved, LATER_REASON),
            ],
            cells: [
                dag_complexity(Complexity::Routine, REASON),
                dag_complexity(Complexity::Involved, LATER_REASON),
            ],
            refused: vec![],
        },
        Row {
            given: "a valid pair, then a refine with an unknown word",
            pairs: [
                pair_of(Complexity::Hard, REASON),
                json!({"complexity": "trivial", "complexity_reason": LATER_REASON}),
            ],
            cells: [hard(REASON), hard(REASON)],
            refused: vec![(1, unknown_step("trivial"))],
        },
        Row {
            given: "a step that is not a string, then a refine with none",
            pairs: [
                json!({"complexity": 3, "complexity_reason": REASON}),
                json!({}),
            ],
            cells: [DAG_UNASSESSED.to_string(), DAG_UNASSESSED.to_string()],
            refused: vec![(0, "a step that is not a string".to_string())],
        },
        Row {
            given: "a null step with a reason, then a refine with none",
            pairs: [
                json!({"complexity": null, "complexity_reason": REASON}),
                json!({}),
            ],
            cells: [DAG_UNASSESSED.to_string(), DAG_UNASSESSED.to_string()],
            refused: vec![(0, "a reason with no step".to_string())],
        },
        Row {
            given: "a step with no reason, then a refine with a valid pair",
            pairs: [
                json!({"complexity": Complexity::Hard.name()}),
                pair_of(Complexity::Routine, LATER_REASON),
            ],
            cells: [
                DAG_UNASSESSED.to_string(),
                dag_complexity(Complexity::Routine, LATER_REASON),
            ],
            refused: vec![(0, "a step with no reason".to_string())],
        },
        Row {
            given: "no pair, then a refine with none",
            pairs: [json!({}), json!({})],
            cells: [DAG_UNASSESSED.to_string(), DAG_UNASSESSED.to_string()],
            refused: vec![],
        },
    ];
    let reject = critique_reject("decomposition-conflict", &["adv-1"]);
    let late = proposal(&json!({"complexity": "trivial", "complexity_reason": REASON}));
    for row in rows {
        let given = row.given;
        let dir = critique_project();
        let root = dir.path();
        std::fs::create_dir_all(root.join("specs")).unwrap();
        std::fs::write(root.join(SPEC), WIDGET_SPEC).unwrap();
        with_run_store(root, |store| {
            let criteria = [WIDGET_CRITERION.to_string()];
            rigger::run_store::start_fresh(store, &criteria, "", "", "", SPEC).unwrap();
        });
        let run_on = |arms: &str| {
            run_on_arms_beside(root, &["run", SPEC], &root.join("scratch"), arms, |_| {})
        };
        let proposals = row.pairs.each_ref().map(proposal);
        let arm = critique_arm_proposing(&proposals, &[&reject, r#"{"verdict":"approve"}"#]);
        let (out, err, ok, fake) = run_on(&arm);
        let launches = fake.path().join("bin/launches");
        let events = read_run_events(root);
        let run = current_run_id_of(root);
        let positions = PLANNERS.map(|spawn| proposed_at(&events, spawn));
        let records: Vec<(Option<String>, Value)> = row
            .refused
            .iter()
            .map(|(k, what)| {
                (
                    Some(format!(
                        "{WIDGET_UNIT}/unassessed-complexity#{}",
                        positions[*k]
                    )),
                    unassessed_complexity_record(WIDGET_UNIT, positions[*k], what),
                )
            })
            .collect();
        assert_eq!(
            (
                (ok, how_the_run_ended(&out)),
                proposals_on_record(&events),
                [
                    served_task(&launches, "plan-critique/adjudicator#0"),
                    served_task(&launches, "plan-critique/adjudicator#1"),
                ],
                decisions_on_record(&events),
            ),
            (
                (true, vec![DONE]),
                PLANNERS
                    .map(String::from)
                    .into_iter()
                    .zip(proposals.clone())
                    .collect::<Vec<_>>(),
                [
                    adjudicator_task(&row.cells[0], None),
                    adjudicator_task(&row.cells[1], Some(&reject)),
                ],
                records.clone(),
            ),
            "{given}: the run ends done, each proposal is on the log as the planner wrote it, each \
             round's adjudicator is served the widget's step and reason, and each refused pair is \
             recorded once; stdout:\n{out}\nstderr:\n{err}"
        );

        propose_unit_via_rigger_emit(root, LATE_PLANNER, &late);
        let (out, err, ok, _fake) = run_on("");
        let events = read_run_events(root);
        assert_eq!(
            (
                (ok, how_the_run_ended(&out)),
                current_run_id_of(root),
                proposals_on_record(&events).last().cloned(),
                decisions_on_record(&events),
            ),
            (
                (true, vec![DONE]),
                run,
                Some((LATE_PLANNER.to_string(), late.clone())),
                records,
            ),
            "{given}: a second run re-enters the same run, the late proposal is on the log, and \
             neither it nor a refused pair before it is recorded again; stdout:\n{out}\n\
             stderr:\n{err}"
        );
    }
}
