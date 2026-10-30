//! Built-in instruction fixtures, shared by the instruction layer's unit tests, the conductor's
//! prompt tests, the binary's shipped-text tests and the integration suites: the working-discipline
//! body's section headings and the review method its `## When you review` section holds; the
//! critique method the adversary and adjudicator personas hold under their
//! `## When you critique a spec or a plan` section; the
//! headings a composed system prompt carries, the handbook chapter that lists the built-in entries
//! and their checked-in sources, each entry's body by name, a near name that shadows no
//! built-in, the non-empty bodies that still switch a built-in off, two operator files that shadow
//! nothing, the layers `rigger instructions` prints, the persona an agent definition parses to,
//! the system prompt a spawn with no operator layer is composed to, and the composed prompt
//! spelled section by section; an operator instruction file by name and body; and the named-entry
//! operator layer the instruction reporters are read over, with what `rigger instructions`,
//! `rigger validate` and `rigger prime` report on it.

/// The working-discipline body's hand-over section heading.
pub const HAND_OVER_HEADING: &str = "## Hand over a tree the gates only confirm";

/// The working-discipline body's review section heading.
pub const WHEN_YOU_REVIEW_HEADING: &str = "## When you review";

/// The working-discipline body's fan-out section heading.
pub const FAN_OUT_HEADING: &str = "## Fan out the mechanical work";

/// The seven review sentences the working-discipline body's `## When you review` section holds,
/// in its order: the review method a reviewer prompt's discipline no longer carries, then the
/// review rule - the three classes a blocking finding falls in, every other finding a note, where
/// the run's notes go, and what a blocking finding names.
pub const WHEN_YOU_REVIEW: [&str; 7] = [
    "Never re-run the gate battery, or any gate the gate evidence shows green on the tree under \
     review: the gates are the instrument that grades the work, and a review judges what they \
     cannot see.",
    "To reproduce a specific suspicion - one named test, a reversion probe, a single targeted test \
     run through the project's test command - create your own throwaway scratch worktree and run \
     it there.",
    "Never run a gate's command as the gate runs it, directly or through a verify helper: the gate \
     evidence in your prompt is that gate's verdict, and a reviewer's run only repeats it at review \
     cost; the one targeted reproduction above is not a gate run.",
    "A finding blocks the change only when it is one of three classes: (a) wrong code - the change \
     does the wrong thing or breaks something; (b) a Done-when criterion the change leaves unmet; \
     (c) a missing test for a behaviour a Done-when criterion names.",
    "Every other finding - docs wording, naming, a fact with two homes, test shape, comment \
     density, a record, a principle bent without breaking anything - is a note: raise it with its \
     location; it never fails the unit.",
    "Where the workflow has a check-in stage - a stage that needs the fan-out implement template \
     - that stage fixes the run's notes once, before its gates; without one, the notes stay in the \
     run's log for the operator.",
    "A blocking finding names its class and, for (b) or (c), the criterion it reads; a verdict \
     whose findings are all notes is an approval, and a rejection requires only its blocking \
     findings.",
];

/// The section heading a critiquing persona holds the critique method under.
pub const WHEN_YOU_CRITIQUE_HEADING: &str = "## When you critique a spec or a plan";

/// The critique sentences both the adversary's and the adjudicator's section hold: the section's
/// scope, how a plan critique reads the text, Rules 7 and 8, the blast-radius note, the ownership
/// defects, the numberless unit-size rule and the overlap rule.
pub const CRITIQUE_METHOD: [&str; 8] = [
    "In a spec or plan critique this section's rules decide which finding blocks; the working \
     discipline's review rule decides a unit diff's findings.",
    "In a plan critique this section's criterion reads as a unit (twin and bundled criteria are \
     twin and bundled units), and Rules 7 and 8 and the unit-size rule read as written.",
    "- Rule 7 (mitigation ownership): every demanded mitigation must be owned by exactly one unit, \
     with the exclusion named on its neighbors; an unassigned or ambiguously-owned mitigation - \
     two units that will fight over the same concern through the shared context graph - is a \
     reject.",
    "- Rule 8 (open dispositions): a unit must not leave a disposition open for a reviewer to \
     re-litigate; an undecided disposition is a reject.",
    "NOTE on shared blast radius: units whose file footprints OVERLAP are NOT a defect. \
     `partition: by-blast-radius` runs them in SEPARATE sequential batches (each branches off the \
     prior batch's integrated tree), and per-unit worktree isolation keeps every reviewer on its \
     own diff - so overlap integrates cleanly and reviews independently. Do NOT reject merely \
     because two units touch the same file. Reject a shared-file split ONLY when it is a genuine \
     OWNERSHIP or COHERENCE defect (rule 7) - two units that cannot own their concern cleanly - \
     not for mechanical overlap the partitioner already serializes.",
    "Ownership defects: twin criteria (two checkboxes claiming one concern) and bundling (one \
     checkbox carrying two mitigations) are each a BLOCKING finding.",
    "- Unit size: a unit over the cap your prompt states is a BLOCKING finding.",
    "- Overlap: judge ownership/coherence, not the overlap.",
];

/// The critique sentences the adversary's section alone holds: the three classes the rules fall
/// in, the stance, the two hunts and the ban.
pub const ADVERSARY_CRITIQUE_METHOD: [&str; 6] = [
    "These rules fall in three classes. To a spec only: the two hunts and the ban. To a DAG only: \
     the unit-size rule and the overlap rule. To both a spec and a DAG: the stance, Rules 7 and 8, \
     the blast-radius note and the ownership defects.",
    "Default to skepticism: assume the author missed something, and try to prove the spec \
     self-contradictory or undecided; never soften a finding to converge. Cite the criterion \
     number or Design block title for every finding.",
    "Run these hunts, and name the hunt in each finding it produces:",
    "- LANDING ORDER: for every ordered pair of criteria (A, B) sharing a command, a store read, a \
     file or a counter, ask \"if A lands first on a tree without B, does A's own text hold?\"; \
     every no is a finding.",
    "- UNDECIDED CORNER: for every criterion's mechanism, walk the corners empty, repeated, \
     reverted, DROPPED (a fact present in an earlier generation and absent in a later one), \
     concurrent, crash-resume, cold start and existing data; each corner no Design sentence \
     decides is a finding.",
    "The ban: a fix is a Design or Global-constraint change, never a criterion edit.",
];

/// The critique sentences the adjudicator's section alone holds: its judging rule, the one home of
/// the "Reject ONLY for" scope, what that scope reads as, and the fix a reject's cause selects.
pub const ADJUDICATOR_JUDGING_RULE: [&str; 3] = [
    "Reject ONLY for a rule 7 (ownership) or rule 8 (open disposition) defect, a unit over the \
     size cap, or a defect in a criterion's own text that no decomposition can remove - never for \
     mechanical blast-radius overlap alone.",
    "A finding stands when it names a Rule 7 ownership defect, a Rule 8 open disposition, a \
     bundled or twin criterion, a unit over the cap the prompt states, or a defect in a \
     criterion's own text that no decomposition can remove; mechanical file overlap alone never \
     does; twin and bundled units count as Rule 7 defects.",
    "The fix is the cause rule's: a re-plan for `decomposition-conflict`, the run stopping for \
     `spec-ambiguity`.",
];

/// The review `tier`'s task `prompt` carries the reviewer `discipline` exactly once and none of the
/// review method `## When you review` holds, the method reaching the tier through its system prompt
/// when no operator file replaces or switches off the working discipline.
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
    assert_carries_the_review_method(&format!("the {tier} task prompt"), prompt, 0);
}

/// The review `tier`'s whitespace-normalized `system` prompt carries each review sentence
/// `## When you review` holds exactly once.
pub fn assert_system_prompt_carries_the_review_method_once(tier: &str, system: &str) {
    assert_carries_the_review_method(&format!("the {tier} system prompt"), system, 1);
}

/// `text`, which `what` names, carries each review sentence `## When you review` holds exactly
/// `times` times.
pub fn assert_carries_the_review_method(what: &str, text: &str, times: usize) {
    for sentence in WHEN_YOU_REVIEW {
        assert_eq!(
            text.matches(sentence).count(),
            times,
            "{what} carries {sentence:?} {times} time(s); it was:\n{text}"
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

/// A near name of the `communication` built-in: its name with a trailing space, which shadows
/// nothing and appends as an operator file.
pub const COMMUNICATION_NEAR_NAME: &str = "communication ";

/// Operator file bodies other than the empty one that are blank once one leading U+FEFF is
/// stripped and Unicode White_Space trimmed, so a same-named file holding one switches its
/// built-in off: a lone byte order mark, ASCII whitespace, and a byte order mark before an em
/// space.
pub const NON_EMPTY_BLANK_BODIES: [&str; 3] = ["\u{feff}", " \t\n", "\u{feff}\u{2003}\n"];

/// The persona the agent definition `agent_file` parses to: the markdown body after its front
/// matter.
pub fn persona(agent_file: &str) -> String {
    rigger::config::parse_agent(agent_file.as_bytes())
        .unwrap()
        .prompt
}

/// The built-in entry `name` as a `(name, body)` section of [`expected_composed_prompt`].
pub fn builtin_section(name: &'static str) -> (&'static str, &'static str) {
    (name, builtin_body(name))
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

/// The line that opens the operator layer `rigger instructions` prints, with the blank line before
/// it.
pub const RENDERED_OPERATOR_HEADING: &str =
    "\nOperator (.rigger/instructions/*.md, filename order):\n";

/// The heading `rigger instructions` gives the communication built-in when no operator file
/// shadows it.
pub const RENDERED_COMMUNICATION_SECTION: &str = "\n## communication (built-in)\n";

/// The body of the operator file that replaces the communication built-in in
/// [`shadowing_operator_layer`].
pub const REPLACING_COMMUNICATION_BODY: &str = "Log each decision in the team channel.";

/// The body of the operator file of [`shadowing_operator_layer`] that shadows no built-in.
pub const UNSHADOWING_STYLE_BODY: &str = "Keep the house style.";

/// An operator instruction file named `name` with `body`, as the loader reads it.
pub fn op(name: &str, body: &str) -> rigger::instructions::Instruction {
    rigger::instructions::Instruction {
        name: name.into(),
        body: body.into(),
    }
}

/// Two operator files that shadow no built-in, in filename order: `10-house`, then `20-team`.
pub fn house_and_team_operator_layer() -> Vec<rigger::instructions::Instruction> {
    vec![op("10-house", "House rule."), op("20-team", "Team rule.")]
}

/// The named-entry fixture the instruction reporters are read over, in filename order as the
/// loader returns it: `30-style`, which shadows no built-in; `communication`, replacing that
/// built-in; and an empty `working-discipline`, switching that built-in off. The
/// `engineering-principles` built-in stays in force.
pub fn shadowing_operator_layer() -> Vec<rigger::instructions::Instruction> {
    [
        ("30-style", Some(UNSHADOWING_STYLE_BODY)),
        ("communication", Some(REPLACING_COMMUNICATION_BODY)),
        ("working-discipline", None),
    ]
    .into_iter()
    .map(|(name, body)| op(name, body.unwrap_or_default()))
    .collect()
}

/// What `rigger instructions` prints over [`shadowing_operator_layer`], spelled without calling
/// `render`: each built-in in `BUILTIN` order with its state, then the one unshadowed file.
pub fn rendered_shadowing_layer() -> String {
    rendered_shadowing_layer_replaced_by(REPLACING_COMMUNICATION_BODY)
}

/// What `rigger instructions` prints over [`shadowing_operator_layer`] whose communication file's
/// body prints as `printed`, spelled without calling `render`: the communication built-in replaced
/// by that body.
pub fn rendered_shadowing_layer_replaced_by(printed: &str) -> String {
    rendered_beside_the_shadowing_entries(
        &format!(
            "\n## communication (replaced by .rigger/instructions/communication.md)\n{printed}\n"
        ),
        "",
    )
}

/// What `rigger instructions` prints over [`shadowing_operator_layer`] whose communication file is
/// renamed to the near name `near`, spelled without calling `render`: the communication built-in in
/// force with its shipped body, and the near-named file listed after `30-style` under the operator
/// heading.
pub fn rendered_near_named_layer(near: &str) -> String {
    rendered_beside_the_shadowing_entries(
        &format!(
            "{RENDERED_COMMUNICATION_SECTION}{}\n",
            builtin_body("communication").trim_end()
        ),
        &format!("\n## {near}\n{REPLACING_COMMUNICATION_BODY}\n"),
    )
}

/// What `rigger instructions` prints over a layer holding [`shadowing_operator_layer`]'s
/// `30-style` and empty `working-discipline` files: the engineering-principles built-in in force,
/// the working-discipline one switched off, then `communication` - the communication built-in's
/// section as printed - then the operator heading, `30-style`, and `tail`, the operator files
/// printed after it.
fn rendered_beside_the_shadowing_entries(communication: &str, tail: &str) -> String {
    format!(
        "{}\n\nBuilt-in:\n\n## engineering-principles (built-in)\n{}\n\n## working-discipline \
         (switched off by .rigger/instructions/working-discipline.md)\n{communication}\
         {RENDERED_OPERATOR_HEADING}\n## 30-style\n{UNSHADOWING_STYLE_BODY}\n{tail}",
        rigger::instructions::HEADING,
        builtin_body("engineering-principles").trim_end()
    )
}

/// What `rigger instructions` prints when no operator file is read, spelled without calling
/// `render`: every built-in in force, in `BUILTIN` order, then `(none)` under the operator heading.
pub fn rendered_built_ins_only_layer() -> String {
    let sections = [
        "engineering-principles",
        "working-discipline",
        "communication",
    ]
    .map(|name| {
        format!(
            "\n## {name} (built-in)\n{}\n",
            builtin_body(name).trim_end()
        )
    })
    .concat();
    format!(
        "{}\n\nBuilt-in:\n{sections}{RENDERED_OPERATOR_HEADING}(none)\n",
        rigger::instructions::HEADING
    )
}

/// The advisories `rigger validate` prints over [`shadowing_operator_layer`], in `BUILTIN` order.
pub const SHADOWING_LAYER_ADVISORIES: [&str; 2] = [
    "warning: built-in instruction working-discipline is switched off by \
     .rigger/instructions/working-discipline.md",
    "warning: built-in instruction communication is replaced by \
     .rigger/instructions/communication.md",
];

/// The first line `rigger prime` prints when no operator file is read: every built-in in force
/// and no operator file count.
pub const BUILT_INS_ONLY_IN_FORCE_LINE: &str = "# Rigger instructions in force: \
     engineering-principles, working-discipline, communication - see rigger instructions";

/// The first line `rigger prime` prints over [`shadowing_operator_layer`].
pub const SHADOWING_LAYER_IN_FORCE_LINE: &str = "# Rigger instructions in force: \
     engineering-principles, communication (replaced) (+1 operator files) - see rigger \
     instructions";

/// The built-in layer of `out`, what `rigger instructions` printed: every line before the
/// operator layer opens.
pub fn printed_built_in_layer(out: &str) -> &str {
    out.split_once(RENDERED_OPERATOR_HEADING)
        .unwrap_or_else(|| panic!("the operator layer must follow the built-in one; got:\n{out}"))
        .0
}

/// Assert `text`, a composed system prompt, carries the communication built-in's section exactly
/// once and the heading the conductor once appended nowhere.
pub fn assert_communication_once(text: &str) {
    assert_communication_section_once(text, COMMUNICATION_SECTION);
}

/// Assert `text` - a composed system prompt, or the layers `rigger instructions` prints - carries
/// the communication built-in's heading line `section` exactly once and the heading the conductor
/// once appended nowhere.
pub fn assert_communication_section_once(text: &str, section: &str) {
    assert_eq!(
        text.matches(section).count(),
        1,
        "the communication section appears exactly once; got: {text:?}"
    );
    assert!(
        !text.contains(RETIRED_COMMUNICATION_HEADING),
        "the retired communication heading appears nowhere; got: {text:?}"
    );
}
