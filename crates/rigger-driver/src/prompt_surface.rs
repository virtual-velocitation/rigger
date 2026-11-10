//! The driver's prompt texts (spec 121, criterion 16), each one `pub const` whose doc comment
//! opens with its class: the progress frame both process hosts close a task with
//! (`driver::progress_framed`), each host's report verb filling its `{report}` slot, and the note
//! the headless host opens a resumed session with (`interruption_note`), whose `{why}` slot takes
//! one of three clauses. The registry types and `fill` are `rigger_domain::prompt_surface`'s. The
//! module is pure consts and compiles in every lane.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompt_surface_fixtures::registry_test;

    /// THE CLASS TABLE's driver rows: each const and the class its doc comment opens with.
    const CLASS_TABLE: &[(&str, &str)] = &[
        ("CLI_PROGRESS_VERB", "CONTRACT"),
        ("PROGRESS_FRAME", "FRAMING"),
        ("PROGRESS_LINE", "CONTRACT"),
        ("RESUME_CLASS", "FRAMING"),
        ("RESUME_HELD", "FRAMING"),
        ("RESUME_NOTE", "FRAMING"),
        ("RESUME_RESTARTED", "FRAMING"),
        ("STREAM_PROGRESS_VERB", "CONTRACT"),
    ];

    registry_test!(
        every_driver_prompt_text_is_a_group_of_its_class_table_class,
        CLASS_TABLE
    );
}
