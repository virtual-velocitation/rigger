//! Plan-critique fixtures: the widget unit a scripted planner proposes, the whole DAG critique
//! prompt, with the unit size cap sentence, the disjoint overlap text, the line a re-plan round
//! opens with and the verdict paragraph it is spelled from, the adjudicator's reject line and the
//! records a spec-defect stop appends (spec 112, criterion 5).

use rigger::conductor::{META_REPLAY_KEY, TYPE_SPEC_DEFECT};
use rigger::contextgraph::TYPE_LESSON_LEARNED;
use rigger::eventstore::Event;
use rigger::ledger::TYPE_UNIT_ESCALATED;

/// The one unit a scripted planner proposes.
pub const WIDGET_UNIT: &str = "u-widget";

/// The criterion [`WIDGET_UNIT`] serves, which the plan-critique fixtures plan against.
pub const WIDGET_CRITERION: &str = "the widget renderer is implemented";

/// The verdict paragraph every round of the DAG critique prompt closes with: its cause contract
/// asks for `spec-ambiguity` - the cause the spec-defect stop reads, spelled here as the
/// adjudicator must write it - only on a defect in a criterion's own text no decomposition can
/// remove, and for `decomposition-conflict` on every defect a re-plan can fix and when unsure.
pub const DAG_CRITIQUE_VERDICT_PARAGRAPH: &str = "Render your final verdict as a JSON line: \
    {\"verdict\":\"approve\"} to release the fan-out, or {\"verdict\":\"reject\"} to send the \
    decomposition back to the planner. A reject's cause follows this gate's contract, which \
    governs it over any generic cause wording in your persona: \
    \"cause\":\"spec-ambiguity\" only when the upheld defect is in a criterion's own \
    text and no decomposition can remove it (two criteria that contradict under every landing \
    order, a criterion no plan can satisfy, a demanded mitigation no criterion owns); \
    \"cause\":\"decomposition-conflict\" for every defect a re-plan can fix (twin units, a \
    missing exclusion between units, a unit over the size cap, a unit owning no criterion, a \
    split the planner chose). When unsure, \"cause\":\"decomposition-conflict\".";

/// The unit size cap sentence the plan protocol and every round of the DAG critique prompt state.
pub const UNIT_SIZE_CAP_SENTENCE: &str = "A unit is too large when it is expected to add more \
    than 3000 lines. The cap measures review scope - how much one review round must read - never \
    code shape.";

/// The overlap text of a DAG whose units share no file.
pub const DAG_DISJOINT_OVERLAP: &str = "\nBlast-radius overlap (informational): every proposed \
    unit has a DISJOINT blast radius.\n";

/// The line a re-plan round's DAG critique prompt opens with, above the reject it quotes.
pub const DAG_PRIOR_REJECT_HEADER: &str = "A prior plan-critique REJECTED this decomposition:\n";

/// The whole DAG critique prompt of a round no reject precedes: its opener, the cap framing, one
/// line per `[id, criterion, needs, blast radius]` of `units`, the `overlap` text and the verdict
/// paragraph.
pub fn dag_critique_prompt(units: &[[&str; 4]], overlap: &str) -> String {
    let list: String = units
        .iter()
        .map(|[id, criterion, needs, radius]| {
            format!("- `{id}`: criterion={criterion:?}; needs=[{needs}]; blast radius=[{radius}]\n")
        })
        .collect();
    format!(
        "You are the plan-critique gate. Review the PROPOSED unit DAG below - the decomposition \
         the planner produced - BEFORE any implementer runs, and judge it against the \
         decomposition rules in your persona: the CROSS-UNIT rules that per-unit review cannot \
         see.\n\n{UNIT_SIZE_CAP_SENTENCE}\n\nProposed units:\n{list}{overlap}\n\
         {DAG_CRITIQUE_VERDICT_PARAGRAPH}\n"
    )
}

/// The halt a plan-critique spec-defect stop reports, naming the run's spec as `spec` spells it
/// and the stopping reject's upheld finding ids as `upheld` lists them: the one expected text the
/// stop's tests read (the conductor's formatter pin keeps its own literals).
pub fn spec_defect_halt_text(spec: &str, upheld: &str) -> String {
    format!(
        "amend the spec and relaunch: plan-critique found a spec defect in {spec} ({upheld}); \
         critique the amended spec, then start the run again"
    )
}

/// A plan-critique adjudicator's reject line carrying `cause` and upholding `upheld`.
pub fn critique_reject(cause: &str, upheld: &[&str]) -> String {
    serde_json::json!({"verdict": "reject", "upheld": upheld, "discarded": [], "cause": cause})
        .to_string()
}

/// The `(type, replay key)` of every plan-critique spec-defect stop record in `events`, in log
/// order.
pub fn stop_records(events: &[Event]) -> Vec<(String, String)> {
    events
        .iter()
        .filter_map(|e| {
            let key = e.meta.get(META_REPLAY_KEY)?;
            key.contains("/spec-defect")
                .then(|| (e.type_.clone(), key.clone()))
        })
        .collect()
}

/// The replay key of the lesson the gate `plan-critique` records when it stops on its reject at
/// attempt 1.
pub const STOP_LESSON_KEY: &str = "plan-critique/spec-defect-lesson#1";

/// The replay key of the `SpecDefect` that stop records.
pub const STOP_SPEC_DEFECT_KEY: &str = "plan-critique/spec-defect#1";

/// The replay key of the escalation that stop records.
pub const STOP_ESCALATED_KEY: &str = "plan-critique/spec-defect-escalated#1";

/// The three records the gate `plan-critique` appends when it stops on its reject at attempt 1,
/// each once, in order: the lesson, the `SpecDefect` and the escalation, each under its key.
pub fn the_stop_records() -> Vec<(String, String)> {
    [
        (TYPE_LESSON_LEARNED, STOP_LESSON_KEY),
        (TYPE_SPEC_DEFECT, STOP_SPEC_DEFECT_KEY),
        (TYPE_UNIT_ESCALATED, STOP_ESCALATED_KEY),
    ]
    .into_iter()
    .map(|(t, k)| (t.to_string(), k.to_string()))
    .collect()
}

/// The index in `events` of the event recorded under replay `key`.
pub fn keyed_index(events: &[Event], key: &str) -> usize {
    events
        .iter()
        .position(|e| e.meta.get(META_REPLAY_KEY).map(String::as_str) == Some(key))
        .unwrap_or_else(|| panic!("no event under {key}"))
}

/// The payload of the event recorded under replay `key` in `events`.
pub fn keyed_payload(events: &[Event], key: &str) -> serde_json::Value {
    serde_json::from_slice(&events[keyed_index(events, key)].data).unwrap()
}
