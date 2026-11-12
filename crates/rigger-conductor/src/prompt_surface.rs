//! The conductor's and the canary's prompt texts (spec 121, criterion 16), each one `pub const`
//! whose doc comment opens with its class: the decision and finding protocols, the planner's
//! refine protocol, the adjudicator's verdict contract, the retry, review-round and gate-evidence
//! blocks, the grounding sections and their elision notes, the DAG critique and re-plan prompts,
//! and the canary's review and adjudicator prompts. A renderer in `conductor.rs` or
//! `canary_store.rs` formats from these through `fill` wherever a const carries a `{slot}`. The
//! registry types and `fill` are `rigger_domain::prompt_surface`'s. The module is pure consts and
//! compiles in every lane.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompt_surface_fixtures::registry_test;

    /// THE CLASS TABLE's conductor rows: each const and the class its doc comment opens with.
    const CLASS_TABLE: &[(&str, &str)] = &[
        ("CANARY_FINDINGS_LEAD", "FRAMING"),
        ("CANARY_NO_ANCHOR", "FRAMING"),
        ("CANARY_NO_FINDINGS", "FRAMING"),
        ("CANARY_REVIEW_HEADER", "FRAMING"),
        ("CANARY_VERDICT", "CONTRACT"),
        ("CODE_NEIGHBORHOOD_HEADER", "FRAMING"),
        ("CONFLICT_RESOLUTION", "FRAMING"),
        ("DAG_CRITIQUE_CONTRACT", "CONTRACT"),
        ("DAG_CRITIQUE_OPENER", "FRAMING"),
        ("DAG_NO_CRITERION", "FRAMING"),
        ("DAG_NO_NEEDS", "FRAMING"),
        ("DAG_NO_RADIUS", "FRAMING"),
        ("DAG_OVERLAP_DISJOINT", "FRAMING"),
        ("DAG_OVERLAP_HEADER", "FRAMING"),
        ("DAG_OVERLAP_ROW", "FRAMING"),
        ("DAG_PRIOR_REJECT", "FRAMING"),
        ("DAG_SIZE_CAP_CAUSE", "CONTRACT"),
        ("DAG_UNITS_HEADER", "FRAMING"),
        ("DAG_UNIT_ROW", "FRAMING"),
        ("DECISIONS_HEADER", "FRAMING"),
        ("DELTA_NONE", "FRAMING"),
        ("DESIGN_INTENT_HEADER", "FRAMING"),
        ("DOC_ONLY_ROUND", "FRAMING"),
        ("ELISION_NOTE_DEFINITIONS", "FRAMING"),
        ("ELISION_NOTE_DESIGN_INTENT", "FRAMING"),
        ("ELISION_NOTE_RECORDS", "FRAMING"),
        ("EMIT_LIVE", "FRAMING"),
        ("EMIT_PROTOCOL", "CONTRACT"),
        ("FINDINGS_HEADER", "FRAMING"),
        ("GATE_EVIDENCE_HEADER", "FRAMING"),
        ("GATE_EVIDENCE_ROW", "FRAMING"),
        ("GRAPH_AROUND_ANY", "CONTRACT"),
        ("GRAPH_AROUND_EACH", "CONTRACT"),
        ("GRAPH_AROUND_ONE", "CONTRACT"),
        ("LATER_ATTEMPT", "FRAMING"),
        ("LATER_ATTEMPT_GATE_CAUSE", "FRAMING"),
        ("LATER_ATTEMPT_REVIEW_CAUSE", "FRAMING"),
        ("LESSONS_HEADER", "FRAMING"),
        ("LOOKUP_LEAD_IN", "FRAMING"),
        ("LOOKUP_VERBS", "CONTRACT"),
        ("PEERS_POINTER", "FRAMING"),
        ("PLAN_COPY_TEXT", "CONTRACT"),
        ("PLAN_CRITERIA_LEAD", "CONTRACT"),
        ("PLAN_NO_CRITERIA", "FRAMING"),
        ("PLAN_PAIRING", "FRAMING"),
        ("PLAN_REAL_SPLIT", "FRAMING"),
        ("PLAN_ROLE", "FRAMING"),
        ("PLAN_SPLIT_ID_ECHO", "CONTRACT"),
        ("PLAN_UNIT_PROPOSED", "CONTRACT"),
        ("PLAN_UNMATCHED", "FRAMING"),
        ("PRIOR_CONTRADICTION", "FRAMING"),
        ("PRIOR_GATE", "FRAMING"),
        ("PRIOR_HALTED", "FRAMING"),
        ("PRIOR_PATTERN", "CONTRACT"),
        ("PRIOR_REQUIRED_LEAD", "FRAMING"),
        ("PRIOR_RETRY_HEADER", "FRAMING"),
        ("PRIOR_REVIEW", "FRAMING"),
        ("RECORD_RECOVERY", "CONTRACT"),
        ("REFUSED_RECORD_NOTE", "FRAMING"),
        ("REPLAN_ID_CONTRACT", "CONTRACT"),
        ("REPLAN_LEAD", "FRAMING"),
        ("REQUIRED_PROTOCOL", "CONTRACT"),
        ("REVIEWER_BUILD_CACHE", "FRAMING"),
        ("REVIEWER_DISCIPLINE", "FRAMING"),
        ("REVIEW_FINDING_LIVE", "FRAMING"),
        ("REVIEW_FINDING_PROTOCOL", "CONTRACT"),
        ("ROUND_LEAD_IN", "FRAMING"),
        ("ROUND_OUTSIDE_DELTA", "CONTRACT"),
        ("ROUND_PATTERN", "CONTRACT"),
        ("ROUND_REQUIRED_HEADER", "FRAMING"),
        ("ROUND_REQUIRED_NONE", "FRAMING"),
        ("RUN_NOTES_CLOSING", "FRAMING"),
        ("RUN_NOTES_HEADER", "FRAMING"),
        ("TASK_BLOCK", "FRAMING"),
        ("UNIT_SIZE_CAP", "FRAMING"),
        ("UPHELD_NOTES", "FRAMING"),
        ("UPHELD_PROTOCOL", "CONTRACT"),
    ];

    registry_test!(
        every_conductor_prompt_text_is_a_group_of_its_class_table_class,
        CLASS_TABLE
    );
}
