//! Periphery for spec 135's PINS ARE NOTED, through the compiled binary. `capability::pin_notes`'
//! own table (`crates/rigger-domain/src/capability.rs`) covers every field a persona can pin. This
//! file covers the operator's surface: `rigger validate` over persona files on disk prints each
//! pin as a note on stderr, persona by persona in id order, and stays valid. Every project is
//! coherent: `work` only under a table, and under a table `work` or a model pin.

use crate::common;
use common::cli::UNISOLATED_WORKER;
use common::cli::{config_valid_lines, lines_where, run_rigger, temp_project, write_scaffold};
use common::fixtures::{capability_table, workflow_declaring_capability, JUDGING_FLOOR};

/// One row: what it proves, the persona files (`(id, definition)`), the table the workflow
/// declares (`None` for a null table) and the note lines `rigger validate` prints, in order.
type Row = (
    &'static str,
    &'static [(&'static str, &'static str)],
    Option<String>,
    &'static [&'static str],
);

/// Given a project whose persona files pin their model, ladder, effort or fallback, or pin
/// nothing, under a capability table or none, when the operator runs `rigger validate`, then the
/// config is valid and stderr carries one note per pinned field naming the persona, the field and
/// its value, and none for a persona that pins nothing.
#[test]
fn rigger_validate_notes_each_pinned_field_of_each_persona_and_stays_valid() {
    let table = Some(capability_table(JUDGING_FLOOR[0].0));
    #[rustfmt::skip]
    let rows: [Row; 3] = [
        ("each pin of each persona under no table, in id order",
         &[("worker", UNISOLATED_WORKER),
           ("climber", "---\nid: climber\nmodel_ladder: [l0, l1]\neffort: high\n---\nDo the unit.\n"),
           ("plain", "---\nid: plain\n---\nDo the unit.\n"),
           ("backup", "---\nid: backup\nfallback_model: fb\n---\nDo the unit.\n")],
         None,
         &[r#"note: agent "backup" pins fallback_model "fb""#,
           r#"note: agent "climber" pins model_ladder ["l0", "l1"]"#,
           r#"note: agent "climber" pins effort "high""#,
           r#"note: agent "worker" pins model "sonnet""#]),
        ("a pin under a table is noted, and work is no pin",
         &[("judge", "---\nid: judge\nwork: judging\neffort: max\n---\nDo the unit.\n"),
           ("scribe", "---\nid: scribe\nwork: writing\n---\nDo the unit.\n"),
           ("pinned", "---\nid: pinned\nmodel: pm\n---\nDo the unit.\n")],
         table,
         &[r#"note: agent "judge" pins effort "max""#,
           r#"note: agent "pinned" pins model "pm""#]),
        ("a project pinning nothing has no note",
         &[("plain", "---\nid: plain\n---\nDo the unit.\n")],
         None,
         &[]),
    ];
    for (why, personas, table, expected) in rows {
        let project = temp_project();
        let workflow = workflow_declaring_capability(table.as_deref().unwrap_or("~"));
        write_scaffold(project.path(), personas, &workflow);
        let (out, err, ok) = run_rigger(project.path(), &["validate"]);
        let valid = format!("config valid: {} agents, 0 stages, 0 gates", personas.len());
        assert_eq!(
            (
                ok,
                config_valid_lines(&out),
                lines_where(&err, |line| line.starts_with("note: agent "))
            ),
            (true, vec![valid.as_str()], expected.to_vec()),
            "{why}: stderr:\n{err}"
        );
    }
}
