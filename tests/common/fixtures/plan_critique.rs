//! Plan-critique fixtures: the widget unit a scripted planner proposes, the planning policy the
//! planner persona states in place of the plan protocol, the plan protocol a planner's task closes
//! with, the whole DAG critique prompt, each with the unit size cap sentence only when the workflow
//! declares a cap, the disjoint overlap text, the line a re-plan round opens with and the verdict
//! paragraph it is spelled from, the adjudicator's reject line and the records a spec-defect stop
//! appends (spec 112, criterion 5).

use rigger::conductor::{META_REPLAY_KEY, TYPE_SPEC_DEFECT};
use rigger::contextgraph::TYPE_LESSON_LEARNED;
use rigger::eventstore::Event;
use rigger::ledger::TYPE_UNIT_ESCALATED;

/// The one unit a scripted planner proposes.
pub const WIDGET_UNIT: &str = "u-widget";

/// The criterion [`WIDGET_UNIT`] serves, which the plan-critique fixtures plan against.
pub const WIDGET_CRITERION: &str = "the widget renderer is implemented";

/// The planning policy, in order: the sentences telling the planner how to plan, which the
/// planner persona and its scaffold seed state and the plan protocol never does.
pub const PLANNING_POLICY: [&str; 6] = [
    "Split a criterion whose one unit would be too large into ordered units (each `needs` the one \
     before it), each owning a named part.",
    "If you discover a NECESSARY sub-unit or an ordering dependency the baseline missed, propose \
     it.",
    "NEVER propose a unit that maps to no acceptance criterion - that is scope creep.",
    "Propose each refinement the moment you decide it.",
    "Do not write code.",
    "Only omit the id when you truly intend a new sub-unit.",
];

/// How many times `text` states each sentence of the [`PLANNING_POLICY`], in its order.
pub fn policy_counts(text: &str) -> [usize; 6] {
    PLANNING_POLICY.map(|sentence| text.matches(sentence).count())
}

/// The plan protocol that closes a planner's task, as the planner is served it: the framing and
/// contract sentences, `cap`'s [`unit_size_cap_then`] a space in its slot, `implementer` as the
/// agent of the `UnitProposed` shape and `criteria` as the criteria block, with none of the
/// [`PLANNING_POLICY`].
pub fn plan_protocol_text(implementer: &str, criteria: &str, cap: u32) -> String {
    format!(
        "\n\nYou are the planner. The conductor has ALREADY created one baseline implement \
         unit per acceptance criterion below - the spec is decomposed by construction. Your \
         job is to REFINE that baseline, not to re-decompose it. {}Each part of a split \
         echoes the split criterion's id and ends its `criterion` text with an OWNS sentence \
         naming its part (the conductor keeps that sentence after the criterion's exact text as \
         that unit's contract).\n\
         Each criterion below is shown with a STABLE id in [brackets]. When your unit serves \
         a criterion, your unit SUPERSEDES (replaces) that criterion's baseline - it does NOT \
         run alongside it - so identify the criterion you serve by ECHOING its id: copy the \
         id shown in brackets next to that criterion into the `criterion_id` field (the \
         brackets are display delimiters - the conductor accepts the id with or without \
         them). The conductor pairs your unit with its baseline by that id, so even if you \
         reword or truncate the criterion text in `criterion`, the correct id still \
         supersedes the one baseline (no duplicate). A wrong or missing `criterion_id` is \
         what makes your unit run as an EXTRA unit on top of the baseline (duplicated work). \
         Still copy the criterion text into `criterion` (verbatim is safest). Several units \
         echoing the SAME id (a real split) all run and replace the one baseline.\n\
         Propose each refinement by calling the rigger_emit tool with type \"UnitProposed\" \
         and data:\n\
         {{\"id\":\"<short-id>\",\"agent\":\"{implementer}\",\"criterion\":\"<the spec criterion \
         it serves>\",\"criterion_id\":\"<the id shown in [brackets] next to that \
         criterion>\",\"needs\":[\"<unit ids it depends on>\"]}}\n\
         A unit whose `criterion_id` is none of the ids below still runs, but as a \
         genuinely-new sub-unit that the conductor flags as unmatched. Every unit you propose \
         - a refinement, a split, or a new sub-unit - automatically runs the fan-out \
         template's own gates; you never need to name them.\n\n\
         The acceptance criteria to refine against (echo the [id] shown next to each \
         criterion into that unit's `criterion_id`, and copy the text into `criterion`):\n\
         {criteria}\n",
        unit_size_cap_then(cap, " ")
    )
}

/// The verdict paragraph every round of the DAG critique prompt closes with: its cause contract
/// asks for `spec-ambiguity` - the cause the spec-defect stop reads, spelled here as the
/// adjudicator must write it - only on a defect in a criterion's own text no decomposition can
/// remove, and for `decomposition-conflict` on every defect a re-plan can fix and when unsure,
/// naming [`DAG_SIZE_CAP_CAUSE`] among those defects only when `cap` declares a unit size cap.
pub fn dag_critique_verdict_paragraph(cap: u32) -> String {
    format!(
        "Render your final verdict as a JSON line: {{\"verdict\":\"approve\"}} to release the \
         fan-out, or {{\"verdict\":\"reject\"}} to send the decomposition back to the planner. \
         A reject's cause follows this gate's contract, which governs it over any generic cause \
         wording in your persona: \"cause\":\"spec-ambiguity\" only when the upheld defect is in \
         a criterion's own text and no decomposition can remove it (two criteria that contradict \
         under every landing order, a criterion no plan can satisfy, a demanded mitigation no \
         criterion owns); \"cause\":\"decomposition-conflict\" for every defect a re-plan can \
         fix (twin units, a missing exclusion between units, {}a unit owning no criterion, a \
         split the planner chose). When unsure, \"cause\":\"decomposition-conflict\".",
        when_capped(cap, DAG_SIZE_CAP_CAUSE)
    )
}

/// The cause example the DAG critique's cause contract names only when a unit size cap is
/// declared.
pub const DAG_SIZE_CAP_CAUSE: &str = "a unit over the size cap, ";

/// The first sentence of the unit size cap, stating `cap` lines: the one cap sentence the plan
/// protocol and every round of the DAG critique prompt state when the workflow declares a cap.
pub fn unit_size_cap_sentence(cap: u32) -> String {
    format!("A unit is too large when it is expected to add more than {cap} lines.")
}

/// What a prompt's cap slot holds: the [`unit_size_cap_sentence`] and the `separator` after it
/// (the plan protocol's space, the DAG critique prompt's blank line) when `cap` declares a unit
/// size cap, else nothing.
fn unit_size_cap_then(cap: u32, separator: &str) -> String {
    when_capped(cap, &format!("{}{separator}", unit_size_cap_sentence(cap))).to_string()
}

/// `text` when `cap` declares a unit size cap - any number of lines above 0 - else the empty
/// string: a workflow that declares no cap, or a cap of 0, states none.
fn when_capped(cap: u32, text: &str) -> &str {
    if cap > 0 {
        text
    } else {
        ""
    }
}

/// The overlap text of a DAG whose units share no file.
pub const DAG_DISJOINT_OVERLAP: &str = "\nBlast-radius overlap (informational): every proposed \
    unit has a DISJOINT blast radius.\n";

/// The line a re-plan round's DAG critique prompt opens with, above the reject it quotes.
pub const DAG_PRIOR_REJECT_HEADER: &str = "A prior plan-critique REJECTED this decomposition:\n";

/// The whole DAG critique prompt of a round no reject precedes over a workflow declaring `cap`: its
/// opener, `cap`'s [`unit_size_cap_then`] a blank line, one line per `[id, criterion, needs, blast
/// radius]` of `units`, the `overlap` text and the [`dag_critique_verdict_paragraph`].
pub fn dag_critique_prompt(units: &[[&str; 4]], overlap: &str, cap: u32) -> String {
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
         see.\n\n{}Proposed units:\n{list}{overlap}\n{}\n",
        unit_size_cap_then(cap, "\n\n"),
        dag_critique_verdict_paragraph(cap)
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
