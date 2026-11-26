//! The one capability resolution (spec 135): the host model and effort a spawn runs at, the cell
//! its helpers run at, and why. [`resolve`] reads only a persona, the workflow's capability table,
//! the unit's planned complexity step, its raise and the spawn's attempt, and never interprets a
//! model or effort string.

use crate::config::{AgentDef, CapabilityTable, Complexity};

/// What one spawn runs at, as [`resolve`] chose it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Capability {
    /// The host model the spawn requests; empty for the host default.
    pub model: String,
    /// The host effort the spawn requests; empty for the host default.
    pub effort: String,
    /// The model the spawn's helpers run at: the table's reading cell at the spawn's step, empty
    /// with no table.
    pub helper_model: String,
    /// The effort the spawn's helpers run at: the table's reading cell at the spawn's step, empty
    /// with no table.
    pub helper_effort: String,
    /// Why the spawn runs at `model` and `effort`, its segments joined by single spaces: when
    /// `from_table`, `<work>/<step>` (the step after the raise and the cap), then `raised <n>`
    /// when the raise is above 0, then `unassessed` when no step was planned; then
    /// `pinned <field>[,<field>]` naming each pinned field that supplied a value (`model` or
    /// `model_ladder`, then `effort`). Empty for a spawn at the host default.
    pub why: String,
    /// Whether a table cell supplied the model or the effort, whether or not the value it
    /// supplied is empty.
    pub from_table: bool,
}

/// The capability a spawn of `agent` runs at. Its step is `complexity` (`involved` when `None`,
/// the unit unassessed) raised by `raise` steps and capped at `hard`; its cell is the `table` row
/// of the persona's work at that step. A persona's own pin wins over the cell for its field: its
/// `model_ladder` rung `attempt`, else its `model`, for the model; its `effort` for the effort.
/// A field neither pinned nor taken from a cell is empty, the host default. The helper fields
/// are the table's reading cell at the step whatever the persona's work or pins.
pub fn resolve(
    agent: &AgentDef,
    table: Option<&CapabilityTable>,
    complexity: Option<Complexity>,
    raise: u32,
    attempt: u32,
) -> Capability {
    let _ = (agent, table, complexity, raise, attempt);
    Capability {
        model: String::new(),
        effort: String::new(),
        helper_model: String::new(),
        helper_effort: String::new(),
        why: String::new(),
        from_table: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse_yaml_naming_unknown_keys;
    use crate::config::Complexity::{Hard, Involved, Routine};
    use crate::config::Work::{self, Judging, Reading, Writing};

    /// A capability table whose nine cells each name a model and an effort of their own, but for
    /// the writing row's hard cell, which names no effort: a row or a step resolved wrong reads
    /// as another cell. Its judging routine and involved cells name different models, since
    /// [`resolve`] never applies the floor.
    const TABLE: &str = "{ \
        judging: { routine: { model: jr, effort: jre }, involved: { model: ji, effort: jie }, \
                   hard: { model: jh, effort: jhe } }, \
        writing: { routine: { model: wr, effort: wre }, involved: { model: wi, effort: wie }, \
                   hard: { model: wh } }, \
        reading: { routine: { model: rr, effort: rre }, involved: { model: ri, effort: rie }, \
                   hard: { model: rh, effort: rhe } } }";

    /// A persona declaring `work` and pinning `model`, `model_ladder` and `effort` (each empty or
    /// none pins nothing).
    fn persona(work: Option<Work>, model: &str, ladder: &[&str], effort: &str) -> AgentDef {
        AgentDef {
            id: "worker".into(),
            work,
            model: model.into(),
            model_ladder: ladder.iter().map(|rung| rung.to_string()).collect(),
            effort: effort.into(),
            ..Default::default()
        }
    }

    /// The capability with each field as given.
    fn capability(
        [model, effort, helper_model, helper_effort, why]: [&str; 5],
        from_table: bool,
    ) -> Capability {
        Capability {
            model: model.into(),
            effort: effort.into(),
            helper_model: helper_model.into(),
            helper_effort: helper_effort.into(),
            why: why.into(),
            from_table,
        }
    }

    /// One row of the resolution table: what it proves, the persona, the table, the planned step,
    /// the raise, the attempt and the capability [`resolve`] must return.
    type Row<'a> = (
        &'a str,
        AgentDef,
        Option<&'a CapabilityTable>,
        Option<Complexity>,
        u32,
        u32,
        Capability,
    );

    /// The table's cell, raised step by step and capped at `hard`; `involved` for an unassessed
    /// unit; the reading cell as the helper fields; each pin winning its own field; a ladder rung
    /// by attempt; the host default with no table and no pin; `from_table` exactly when a cell
    /// supplied a field; and the why in THE RECORD's format. The last thirteen rows are the
    /// three ladder tests `model_for_attempt` had, their inputs and models unchanged.
    #[test]
    fn resolve_takes_the_raised_cell_unless_a_pin_wins_its_field_and_says_why() {
        let table: CapabilityTable =
            parse_yaml_naming_unknown_keys(TABLE).unwrap_or_else(|e| panic!("parse {TABLE}: {e}"));
        let table = Some(&table);
        let w = Some(Writing);
        let unpinned = |work| persona(work, "", &[], "");
        #[rustfmt::skip]
        let rows: Vec<Row> = vec![
            ("a routine unit takes its own cell",
             unpinned(w), table, Some(Routine), 0, 0,
             capability(["wr", "wre", "rr", "rre", "writing/routine"], true)),
            ("the attempt never moves the step",
             unpinned(w), table, Some(Routine), 0, 2,
             capability(["wr", "wre", "rr", "rre", "writing/routine"], true)),
            ("one raise takes the next step, the helpers with it",
             unpinned(w), table, Some(Routine), 1, 0,
             capability(["wi", "wie", "ri", "rie", "writing/involved raised 1"], true)),
            ("two raises reach hard, whose cell names no effort and still supplies one",
             unpinned(w), table, Some(Routine), 2, 0,
             capability(["wh", "", "rh", "rhe", "writing/hard raised 2"], true)),
            ("a raise past hard stops there, and the why counts every raise",
             unpinned(w), table, Some(Routine), 3, 0,
             capability(["wh", "", "rh", "rhe", "writing/hard raised 3"], true)),
            ("a judging persona takes the judging row",
             unpinned(Some(Judging)), table, Some(Routine), 0, 0,
             capability(["jr", "jre", "rr", "rre", "judging/routine"], true)),
            ("involved raised once is hard",
             unpinned(Some(Judging)), table, Some(Involved), 1, 0,
             capability(["jh", "jhe", "rh", "rhe", "judging/hard raised 1"], true)),
            ("a hard unit takes the hard cell",
             unpinned(Some(Judging)), table, Some(Hard), 0, 0,
             capability(["jh", "jhe", "rh", "rhe", "judging/hard"], true)),
            ("hard raised once stays hard",
             unpinned(Some(Judging)), table, Some(Hard), 1, 0,
             capability(["jh", "jhe", "rh", "rhe", "judging/hard raised 1"], true)),
            ("the largest raise stops at hard",
             unpinned(Some(Judging)), table, Some(Routine), u32::MAX, 0,
             capability(["jh", "jhe", "rh", "rhe", "judging/hard raised 4294967295"], true)),
            ("a reading persona takes the reading row, the cell its helpers take too",
             unpinned(Some(Reading)), table, Some(Involved), 0, 0,
             capability(["ri", "rie", "ri", "rie", "reading/involved"], true)),
            ("an unassessed unit resolves at involved",
             unpinned(w), table, None, 0, 0,
             capability(["wi", "wie", "ri", "rie", "writing/involved unassessed"], true)),
            ("an unassessed unit raised once is hard",
             unpinned(w), table, None, 1, 0,
             capability(["wh", "", "rh", "rhe", "writing/hard raised 1 unassessed"], true)),
            ("a model pin wins the model and the cell supplies the effort",
             persona(w, "pm", &[], ""), table, Some(Routine), 0, 0,
             capability(["pm", "wre", "rr", "rre", "writing/routine pinned model"], true)),
            ("a model pin takes the cell's empty effort, from the table",
             persona(w, "pm", &[], ""), table, Some(Hard), 0, 0,
             capability(["pm", "", "rh", "rhe", "writing/hard pinned model"], true)),
            ("an effort pin wins the effort and the cell supplies the model",
             persona(w, "", &[], "pe"), table, Some(Involved), 1, 0,
             capability(["wh", "pe", "rh", "rhe", "writing/hard raised 1 pinned effort"], true)),
            ("every segment of the why, in its order",
             persona(w, "", &[], "pe"), table, None, 1, 0,
             capability(["wh", "pe", "rh", "rhe", "writing/hard raised 1 unassessed pinned effort"], true)),
            ("both fields pinned take nothing from the cell, and the why names the pins alone",
             persona(w, "pm", &[], "pe"), table, None, 2, 0,
             capability(["pm", "pe", "rh", "rhe", "pinned model,effort"], false)),
            ("a persona with no work takes no cell, its helpers still the reading cell",
             persona(None, "pm", &[], ""), table, Some(Routine), 1, 0,
             capability(["pm", "", "ri", "rie", "pinned model"], false)),
            ("a persona with no work and no pin runs at the host default under a table",
             unpinned(None), table, None, 0, 0,
             capability(["", "", "ri", "rie", ""], false)),
            ("a ladder rung is chosen by the attempt, never the raise",
             persona(w, "", &["l0", "l1", "l2"], ""), table, Some(Routine), 1, 0,
             capability(["l0", "wie", "ri", "rie", "writing/involved raised 1 pinned model_ladder"], true)),
            ("a later attempt takes a later rung at the unraised step",
             persona(w, "", &["l0", "l1", "l2"], ""), table, Some(Routine), 0, 2,
             capability(["l2", "wre", "rr", "rre", "writing/routine pinned model_ladder"], true)),
            ("an empty rung is still the ladder's pin, never the cell's model",
             persona(w, "", &["", "l1"], ""), table, Some(Routine), 0, 0,
             capability(["", "wre", "rr", "rre", "writing/routine pinned model_ladder"], true)),
            ("no table and no pin runs at the host default",
             unpinned(w), None, Some(Routine), 1, 0,
             capability(["", "", "", "", ""], false)),
            ("a model pin with no table names no effort",
             persona(w, "pm", &[], ""), None, None, 2, 0,
             capability(["pm", "", "", "", "pinned model"], false)),
            ("an effort pin with no table names no model",
             persona(None, "", &[], "pe"), None, Some(Hard), 0, 0,
             capability(["", "pe", "", "", "pinned effort"], false)),
            ("a ladder and an effort pinned with no table",
             persona(None, "", &["l0", "l1"], "pe"), None, None, 0, 1,
             capability(["l1", "pe", "", "", "pinned model_ladder,effort"], false)),
            ("attempt 0 -> rung 0",
             persona(None, "", &["haiku", "sonnet", "opus"], ""), None, None, 0, 0,
             capability(["haiku", "", "", "", "pinned model_ladder"], false)),
            ("attempt 1 -> rung 1",
             persona(None, "", &["haiku", "sonnet", "opus"], ""), None, None, 0, 1,
             capability(["sonnet", "", "", "", "pinned model_ladder"], false)),
            ("attempt 2 -> rung 2 (last)",
             persona(None, "", &["haiku", "sonnet", "opus"], ""), None, None, 0, 2,
             capability(["opus", "", "", "", "pinned model_ladder"], false)),
            ("past the end clamps at the last rung",
             persona(None, "", &["haiku", "sonnet", "opus"], ""), None, None, 0, 3,
             capability(["opus", "", "", "", "pinned model_ladder"], false)),
            ("far past the end still clamps",
             persona(None, "", &["haiku", "sonnet", "opus"], ""), None, None, 0, 99,
             capability(["opus", "", "", "", "pinned model_ladder"], false)),
            ("a lone model: does not ladder - attempt 0 still resolves it",
             persona(None, "sonnet", &[], ""), None, None, 0, 0,
             capability(["sonnet", "", "", "", "pinned model"], false)),
            ("a lone model: does not ladder - attempt 1 still resolves it",
             persona(None, "sonnet", &[], ""), None, None, 0, 1,
             capability(["sonnet", "", "", "", "pinned model"], false)),
            ("a lone model: does not ladder - attempt 5 still resolves it",
             persona(None, "sonnet", &[], ""), None, None, 0, 5,
             capability(["sonnet", "", "", "", "pinned model"], false)),
            ("a lone model: does not ladder - attempt 42 still resolves it",
             persona(None, "sonnet", &[], ""), None, None, 0, 42,
             capability(["sonnet", "", "", "", "pinned model"], false)),
            ("neither declared: empty (the driver's default model is inherited), attempt 0",
             unpinned(None), None, None, 0, 0,
             capability(["", "", "", "", ""], false)),
            ("neither declared: empty (the driver's default model is inherited), attempt 3",
             unpinned(None), None, None, 0, 3,
             capability(["", "", "", "", ""], false)),
            ("a declared ladder takes precedence over a lone model, attempt 0",
             persona(None, "haiku", &["sonnet", "opus"], ""), None, None, 0, 0,
             capability(["sonnet", "", "", "", "pinned model_ladder"], false)),
            ("a declared ladder takes precedence over a lone model, attempt 1",
             persona(None, "haiku", &["sonnet", "opus"], ""), None, None, 0, 1,
             capability(["opus", "", "", "", "pinned model_ladder"], false)),
        ];
        for (why, agent, table, complexity, raise, attempt, expected) in rows {
            assert_eq!(
                resolve(&agent, table, complexity, raise, attempt),
                expected,
                "{why}: {agent:?} at {complexity:?} raised {raise}, attempt {attempt}"
            );
        }
    }
}
