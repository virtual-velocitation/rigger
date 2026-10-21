//! Built-in instruction fixtures, shared by the instruction layer's unit tests, the conductor's
//! prompt tests, the binary's shipped-text tests and the integration suites: the working-discipline
//! body's section headings and the review method its `## When you review` section holds; the
//! headings a composed system prompt carries, the handbook chapter that lists the built-in entries
//! and their checked-in sources, each entry's body by name, the layers `rigger instructions`
//! prints, the persona an agent definition parses to, the system prompt a spawn with no operator
//! layer is composed to, and the composed prompt spelled section by section.

/// The working-discipline body's hand-over section heading.
pub const HAND_OVER_HEADING: &str = "## Hand over a tree the gates only confirm";

/// The working-discipline body's review section heading.
pub const WHEN_YOU_REVIEW_HEADING: &str = "## When you review";

/// The working-discipline body's fan-out section heading.
pub const FAN_OUT_HEADING: &str = "## Fan out the mechanical work";

/// The three review sentences the working-discipline body's `## When you review` section holds,
/// in its order: the review method a reviewer prompt's discipline no longer carries.
pub const WHEN_YOU_REVIEW: [&str; 3] = [
    "Never re-run the gate battery, or any gate the gate evidence shows green on the tree under \
     review: the gates are the instrument that grades the work, and a review judges what they \
     cannot see.",
    "To reproduce a specific suspicion - one named test, a reversion probe, a single targeted test \
     run through the project's test command - create your own throwaway scratch worktree and run \
     it there.",
    "Never run a gate's command as the gate runs it, directly or through a verify helper: the gate \
     evidence in your prompt is that gate's verdict, and a reviewer's run only repeats it at review \
     cost; the one targeted reproduction above is not a gate run.",
];

/// The review `tier`'s task `prompt` carries the reviewer `discipline` exactly once and none of the
/// review method `## When you review` holds, the method reaching the tier through its system prompt.
pub fn assert_task_prompt_carries_only_the_reviewer_contract(
    tier: &str,
    prompt: &str,
    discipline: &str,
) {
    assert_eq!(
        prompt.matches(discipline).count(),
        1,
        "the {tier} task prompt carries the reviewer discipline exactly once; prompt was:\n{prompt}"
    );
    for sentence in WHEN_YOU_REVIEW {
        assert_eq!(
            prompt.matches(sentence).count(),
            0,
            "the {tier} task prompt leaves {sentence:?} to `## When you review`; prompt \
             was:\n{prompt}"
        );
    }
}

/// The review `tier`'s whitespace-normalized `system` prompt carries each review sentence
/// `## When you review` holds exactly once.
pub fn assert_system_prompt_carries_the_review_method_once(tier: &str, system: &str) {
    for sentence in WHEN_YOU_REVIEW {
        assert_eq!(
            system.matches(sentence).count(),
            1,
            "the {tier} system prompt carries {sentence:?} once; system prompt \
             was:\n{system}"
        );
    }
}

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

/// The body of the built-in instruction entry `name`.
pub fn builtin_body(name: &str) -> &'static str {
    match rigger::instructions::BUILTIN
        .iter()
        .find(|(entry, _)| *entry == name)
    {
        Some((_, body)) => body,
        None => panic!("the {name:?} entry is built in"),
    }
}

/// The persona the agent definition `agent_file` parses to: the markdown body after its front
/// matter.
pub fn persona(agent_file: &str) -> String {
    rigger::config::parse_agent(agent_file.as_bytes())
        .unwrap()
        .prompt
}

/// The system prompt a spawn of the agent defined by `agent_file` receives with no operator
/// layer: `compose`'s output over the persona the definition parses to.
pub fn composed_system_prompt(agent_file: &str) -> String {
    rigger::instructions::compose(&persona(agent_file), &[])
}

/// The system prompt `compose` must return over `persona`, spelled without calling it: the
/// persona, the bare heading, then each named section in order, its body with trailing
/// whitespace trimmed.
pub fn expected_composed_prompt(persona: &str, sections: &[(&str, &str)]) -> String {
    let mut out = format!("{persona}\n\n{}\n", rigger::instructions::HEADING);
    for (name, body) in sections {
        out.push_str(&format!("\n## {name}\n{}\n", body.trim_end()));
    }
    out
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
