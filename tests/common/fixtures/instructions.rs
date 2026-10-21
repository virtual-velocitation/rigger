//! Instruction-layer fixtures: the headings a composed system prompt carries, the handbook
//! chapter that lists the built-in entries and their checked-in sources, the layers `rigger
//! instructions` prints, and the system prompt a spawn with no operator layer is composed to.

/// The heading line the communication built-in opens in a composed system prompt.
pub const COMMUNICATION_SECTION: &str = "\n## communication\n";

/// The heading the communication discipline carried while the conductor appended it after the
/// composed layers, before it became a built-in entry.
pub const RETIRED_COMMUNICATION_HEADING: &str =
    "# Rigger communication discipline (non-negotiable)";

/// The handbook chapter that lists the built-in instruction entries, relative to the repository
/// root.
pub const AUTHORING_AGENTS_HANDBOOK: &str = "docs/handbook/authoring-agents.md";

/// The checked-in source of the built-in instruction entry `name`, relative to the repository
/// root.
pub fn builtin_instruction_source(name: &str) -> String {
    format!("crates/rigger-domain/src/instructions/{name}.md")
}

/// The system prompt a spawn of the agent defined by `agent_file` receives with no operator
/// layer: `compose`'s output over the persona the definition parses to.
pub fn composed_system_prompt(agent_file: &str) -> String {
    rigger::instructions::compose(
        &rigger::config::parse_agent(agent_file.as_bytes())
            .unwrap()
            .prompt,
        &[],
    )
}

/// The built-in layer of `out`, what `rigger instructions` printed: every line before the
/// operator layer opens.
pub fn printed_built_in_layer(out: &str) -> &str {
    out.split_once("\nOperator (")
        .unwrap_or_else(|| panic!("the operator layer must follow the built-in one; got:\n{out}"))
        .0
}

/// Assert `text` - a composed system prompt, or the layers `rigger instructions` prints - carries
/// the communication built-in's section exactly once and the heading the conductor once appended
/// nowhere.
pub fn assert_communication_once(text: &str) {
    assert_eq!(
        text.matches(COMMUNICATION_SECTION).count(),
        1,
        "the communication section appears exactly once; got: {text:?}"
    );
    assert!(
        !text.contains(RETIRED_COMMUNICATION_HEADING),
        "the retired communication heading appears nowhere; got: {text:?}"
    );
}
