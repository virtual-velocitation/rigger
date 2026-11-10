//! The driver's prompt texts (spec 121, criterion 16), each one `pub const` whose doc comment
//! opens with its class: the progress frame both process hosts close a task with
//! (`driver::progress_framed`), each host's report verb filling its `{report}` slot, and the note
//! the headless host opens a resumed session with (`interruption_note`), whose `{why}` slot takes
//! one of three clauses. The registry types and `fill` are `rigger_domain::prompt_surface`'s. The
//! module is pure consts and compiles in every lane.

/// FRAMING: the separator that closes the spawn's task, `{task}`, ahead of the progress line.
pub const PROGRESS_FRAME: &str = "{task}\n\n--- rigger driver ---\n";

/// CONTRACT: how a worker records a progress line on this host, `{report}` holding the host's
/// report verb and its arguments.
pub const PROGRESS_LINE: &str = "LIVE PROGRESS: record a progress line {report}.";

/// CONTRACT: the cli host's report verb, the `rigger progress` command run through Bash for the
/// spawn `{id}`.
pub const CLI_PROGRESS_VERB: &str =
    "by running (Bash): rigger progress '{id}' '<one line: what you just did>'";

/// CONTRACT: the headless host's report verb, a call of the progress tool `{tool}` with the line
/// as its activity.
pub const STREAM_PROGRESS_VERB: &str = "by calling the {tool} tool with that line as its activity";

/// FRAMING: the note a resumed session opens with, `{why}` holding the clause that names what
/// interrupted it.
pub const RESUME_NOTE: &str = "This session was interrupted{why}. Continue where you left off.";

/// FRAMING: the `{why}` clause of a close with no class: rigger's supervisor restarted.
pub const RESUME_RESTARTED: &str = ": rigger's supervisor restarted";

/// FRAMING: the `{why}` clause after a released hold: how long it stood, `{held_for_s}` seconds,
/// and its `{reason}`.
pub const RESUME_HELD: &str = ": the run was held {held_for_s}s ({reason}) and is released now";

/// FRAMING: the `{why}` clause of a classed close that no released hold explains: the close's
/// `{class}`.
pub const RESUME_CLASS: &str = " by {class}";

/// The driver's groups by surface, each in render order: the progress frame, each process
/// host's report verb, then the resume note and its three `{why}` clauses. Its readers are
/// tests, so it is compiled for tests alone.
#[cfg(test)]
pub fn prompt_groups() -> &'static [rigger_domain::prompt_surface::Group] {
    &[
        rigger_domain::prompt_group!("driver::progress_framed", PROGRESS_FRAME),
        rigger_domain::prompt_group!("driver::progress_framed", PROGRESS_LINE),
        rigger_domain::prompt_group!("driver::cli::framed", CLI_PROGRESS_VERB),
        rigger_domain::prompt_group!("driver::claude_code::framed", STREAM_PROGRESS_VERB),
        rigger_domain::prompt_group!("driver::claude_code::interruption_note", RESUME_NOTE),
        rigger_domain::prompt_group!("driver::claude_code::interruption_note", RESUME_RESTARTED),
        rigger_domain::prompt_group!("driver::claude_code::interruption_note", RESUME_HELD),
        rigger_domain::prompt_group!("driver::claude_code::interruption_note", RESUME_CLASS),
    ]
}

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
