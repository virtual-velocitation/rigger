//! Periphery for spec 135's ONE RESOLUTION, from outside the domain crate. `capability::resolve`'s
//! own table (`crates/rigger-domain/src/capability.rs`) covers every resolution over personas
//! built in code and a table parsed on its own. This file covers the seam a spawn resolves through:
//! a project's persona file and its workflow's `defaults.capability`, read by
//! `config_store::load`, resolve to the cell, the pins and the why the two files declare.

use crate::common;
use common::cli::{temp_project, write_scaffold, UNISOLATED_WORKER};
use common::fixtures::{capability, workflow_declaring_capability};
use rigger::config::Complexity::{self, Hard, Involved, Routine};
use rigger_domain::capability::{resolve, Capability};

/// A capability table the judging floor accepts, its judging `routine` and `involved` cells
/// naming one model with two efforts. Every other cell names a model and an effort of its own,
/// but for the writing row's hard cell, which names no effort.
const TABLE: &str = "{ \
    judging: { routine: { model: j, effort: jr }, involved: { model: j, effort: ji }, \
               hard: { model: jh, effort: jhe } }, \
    writing: { routine: { model: wr, effort: wre }, involved: { model: wi, effort: wie }, \
               hard: { model: wh } }, \
    reading: { routine: { model: rr, effort: rre }, involved: { model: ri, effort: rie }, \
               hard: { model: rh, effort: rhe } } }";

/// One row: what it proves, the persona file, the table its workflow declares (`None` for a
/// null table), the planned step, the raise, the attempt and the capability `resolve` returns.
type Row = (
    &'static str,
    &'static str,
    Option<&'static str>,
    Option<Complexity>,
    u32,
    u32,
    Capability,
);

/// Given a project whose persona file declares its work and pins its own way, and whose workflow
/// declares the capability table or none, when the loader reads it and the persona resolves at
/// a planned step, raise and attempt, then the persona runs at exactly the cell, pins and why
/// its two files declare. Every row is coherent: `work` only under a table, and under a table
/// `work` or a model pin.
#[test]
fn a_loaded_persona_resolves_at_the_cell_and_pins_its_files_declare() {
    #[rustfmt::skip]
    let rows: [Row; 9] = [
        ("the work key selects its row, raised to hard, whose cell's absent effort is empty",
         "---\nid: scribe\nwork: writing\n---\nDo the unit.\n", Some(TABLE), Some(Routine), 2, 0,
         capability(["wh", "", "rh", "rhe", "writing/hard raised 2"], true)),
        ("an effort pin wins the effort, the unassessed step's cell the model",
         "---\nid: judge\nwork: judging\neffort: max\n---\nDo the unit.\n", Some(TABLE), None, 0, 0,
         capability(["j", "max", "ri", "rie", "judging/involved unassessed pinned effort"], true)),
        ("a model_ladder list clamps at its last rung, the cell supplying the effort",
         "---\nid: climber\nwork: reading\nmodel_ladder: [l0, l1]\n---\nDo the unit.\n",
         Some(TABLE), Some(Routine), 0, 4,
         capability(["l1", "rre", "rr", "rre", "reading/routine pinned model_ladder"], true)),
        ("a fallback_model is no pin, so the cell supplies both fields",
         "---\nid: backup\nwork: writing\nfallback_model: fb\n---\nDo the unit.\n",
         Some(TABLE), Some(Involved), 0, 0,
         capability(["wi", "wie", "ri", "rie", "writing/involved"], true)),
        ("both fields pinned take nothing from the cell, the helpers still the reading cell",
         "---\nid: both\nwork: judging\nmodel: pm\neffort: pe\n---\nDo the unit.\n",
         Some(TABLE), Some(Hard), 0, 0,
         capability(["pm", "pe", "rh", "rhe", "pinned model,effort"], false)),
        ("a model pin with no work takes no cell under a table",
         "---\nid: pinned\nmodel: pm\n---\nDo the unit.\n", Some(TABLE), Some(Routine), 1, 0,
         capability(["pm", "", "ri", "rie", "pinned model"], false)),
        ("a pinned project with no table runs at its pin whatever its step, raise and attempt",
         UNISOLATED_WORKER, None, Some(Hard), 2, 3,
         capability(["sonnet", "", "", "", "pinned model"], false)),
        ("a ladder and an effort pinned with no table",
         "---\nid: laddered\nmodel_ladder: [l0, l1, l2]\neffort: high\n---\nDo the unit.\n",
         None, None, 0, 1,
         capability(["l1", "high", "", "", "pinned model_ladder,effort"], false)),
        ("no table, no work and no pin is the host default",
         "---\nid: plain\n---\nDo the unit.\n", None, Some(Routine), 1, 0,
         capability(["", "", "", "", ""], false)),
    ];
    let project = temp_project();
    let dir = project.path().to_str().expect("a UTF-8 temp dir");
    for (why, persona, table, complexity, raise, attempt, expected) in rows {
        let workflow = workflow_declaring_capability(table.unwrap_or("~"));
        write_scaffold(project.path(), &[("persona", persona)], &workflow);
        let config = rigger::config_store::load(dir)
            .unwrap_or_else(|e| panic!("{why}: load {persona:?} under {workflow:?}: {e}"));
        let agents: Vec<_> = config.agents.values().collect();
        let [agent] = agents[..] else {
            panic!("{why}: one persona loads, got {agents:?}")
        };
        assert_eq!(
            resolve(
                agent,
                config.workflow.defaults.capability.as_ref(),
                complexity,
                raise,
                attempt
            ),
            expected,
            "{why}: {persona:?} under {workflow:?} at {complexity:?} raised {raise}, attempt \
             {attempt}"
        );
    }
}
