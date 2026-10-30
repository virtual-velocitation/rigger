//! Instruction injection: rigger is a harness, so the instructions every spawned agent
//! receives are LAYERED, not hard-coded into personas. A spawn's system prompt is composed by
//! [`compose`] as
//!
//! ```text
//! persona  +  BUILT-IN instructions  +  OPERATOR instructions
//!             (the communication discipline among them)
//! ```
//!
//! The BUILT-IN layer ships inside the binary ([`BUILTIN`]) and reaches every agent rigger
//! spawns anywhere, for every consumer, regardless of persona text, unless replaced or switched
//! off; an operator file named byte for byte as a built-in entry, its body not blank, replaces
//! that entry's body in the entry's place, and one whose body is blank switches the entry off,
//! so neither the entry nor the file is injected ([`resolve_layers`]). The OPERATOR layer is
//! every other `*.md` under `.rigger/instructions/`, in filename order, read by the store-gated
//! loader in `config_store` and carried on `Config::instructions`, so this module stays pure
//! and compiles in the `core` lane. `rigger instructions` prints every layer, each built-in with
//! its state - in force, replaced or switched off - ([`render`]) so an operator can read exactly
//! what their agents are held to; `rigger validate` warns of each shadowed built-in
//! ([`shadow_advisories`]).

use crate::config::RIGGER_DIR;

/// One instruction section: `name` is the file stem (or the built-in id) and `body` its text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instruction {
    pub name: String,
    pub body: String,
}

/// The built-in layer, in the order it is injected, each entry unless a same-named operator file
/// replaces or switches it off ([`resolve_layers`]). The first entry is the engineering law
/// the project holds every change to: Clean Architecture, SOLID, DRY, KISS, YAGNI, TDD and
/// BDD where relevant. The second is the working discipline every agent follows: one pass by
/// excellence, a turn's end being the session's end (every job waited on inside the turn), the
/// hand-over an author's tree meets before it reports (each outcome with the
/// check that proves it), the review method with the rule that only wrong code, an unmet
/// Done-when criterion or a missing test for a behaviour one names blocks, every other finding
/// a note, and fanning mechanical work out to the `lookup` and `verify` helpers.
/// The third is the communication discipline: each decision and finding recorded the moment it
/// is made, peers checked between actions, a peer's decision superseded explicitly.
/// A harness-wide rule lives here once,
/// never copied into persona files; a persona carries only the project's anchors for it.
pub const BUILTIN: &[(&str, &str)] = &[
    (
        "engineering-principles",
        include_str!("instructions/engineering-principles.md"),
    ),
    (
        "working-discipline",
        include_str!("instructions/working-discipline.md"),
    ),
    (
        "communication",
        include_str!("instructions/communication.md"),
    ),
];

/// The heading that opens the injected layers in a system prompt; tests and `rigger prime`
/// key on it.
pub const HEADING: &str = "# Rigger instructions";

/// What the operator layer made of one built-in entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerState<'a> {
    /// No operator file shadows the entry: its shipped body is in force.
    BuiltIn(&'static str),
    /// An operator file of the same name replaces the entry: the file's body, its leading
    /// U+FEFF stripped, is in force in the entry's place.
    Replaced(&'a str),
    /// An operator file of the same name with a blank body switches the entry off: nothing is
    /// in force in its place.
    SwitchedOff,
}

impl<'a> LayerState<'a> {
    /// The body in force in the entry's place: the shipped body ([`LayerState::BuiltIn`]), the
    /// operator file's ([`LayerState::Replaced`]), or `None` for a switched-off entry.
    pub fn in_force(self) -> Option<&'a str> {
        match self {
            LayerState::BuiltIn(body) | LayerState::Replaced(body) => Some(body),
            LayerState::SwitchedOff => None,
        }
    }

    /// How a same-named operator file shadows the entry, as the reports word it: `replaced`,
    /// `switched off`, or `None` when no operator file shadows it.
    pub fn shadow_verb(self) -> Option<&'static str> {
        match self {
            LayerState::BuiltIn(_) => None,
            LayerState::Replaced(_) => Some("replaced"),
            LayerState::SwitchedOff => Some("switched off"),
        }
    }
}

/// One built-in entry, in [`BUILTIN`] order, with the state the operator layer left it in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuiltInLayer<'a> {
    pub name: &'static str,
    pub state: LayerState<'a>,
}

/// The resolved layers: every built-in entry with its state, then the operator files that
/// shadow no built-in, in the order given.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layers<'a> {
    pub builtins: Vec<BuiltInLayer<'a>>,
    pub operator: Vec<&'a Instruction>,
}

/// Resolve every built-in's state against `operator`, the one authority for shadowing. An
/// operator file whose `name` equals a built-in's name byte for byte (case-sensitive, never
/// trimmed) shadows that built-in and leaves the operator layer: a body that is not blank -
/// empty once one leading U+FEFF is stripped and Unicode White_Space trimmed - replaces it
/// ([`LayerState::Replaced`]), a blank one switches it off ([`LayerState::SwitchedOff`]).
/// Every other file, a near name included, stays in the operator layer in the order given.
pub fn resolve_layers(operator: &[Instruction]) -> Layers<'_> {
    let shadows = |ins: &Instruction, name: &str| ins.name == name;
    let builtins = BUILTIN
        .iter()
        .map(|&(name, body)| BuiltInLayer {
            name,
            state: match operator.iter().find(|ins| shadows(ins, name)) {
                Some(ins) => match without_bom(&ins.body) {
                    blank if blank.trim().is_empty() => LayerState::SwitchedOff,
                    body => LayerState::Replaced(body),
                },
                None => LayerState::BuiltIn(body),
            },
        })
        .collect();
    let operator = operator
        .iter()
        .filter(|ins| !BUILTIN.iter().any(|&(name, _)| shadows(ins, name)))
        .collect();
    Layers { builtins, operator }
}

/// `body` with its one leading U+FEFF (a byte-order mark) stripped.
fn without_bom(body: &str) -> &str {
    body.strip_prefix('\u{feff}').unwrap_or(body)
}

/// Compose `persona` followed by the built-in layer and then `operator`, each section under
/// its own `## <name>` heading beneath [`HEADING`]: a built-in in force or replaced prints its
/// section, a switched-off one prints none, and [`HEADING`] is printed even when no section
/// follows it. An empty persona still receives every built-in not switched off; an empty
/// operator layer adds nothing after the built-ins. It is the one authority for every spawn's
/// system prompt: the conductor, the canary runner and the spec critic all thread its output,
/// appending nothing after it.
pub fn compose(persona: &str, operator: &[Instruction]) -> String {
    let layers = resolve_layers(operator);
    let mut out = String::from(persona);
    out.push_str("\n\n");
    out.push_str(HEADING);
    out.push('\n');
    for layer in &layers.builtins {
        if let Some(body) = layer.state.in_force() {
            push_section(&mut out, layer.name, body);
        }
    }
    for ins in layers.operator {
        push_section(&mut out, &ins.name, &ins.body);
    }
    out
}

/// The text `rigger instructions` prints, read from [`resolve_layers`]: every built-in in
/// [`BUILTIN`] order labelled with its state - `(built-in)` with its shipped body, `(replaced by
/// .rigger/instructions/<name>.md)` with the operator file's body, or `(switched off by
/// .rigger/instructions/<name>.md)` with no body - then each operator file that shadows no
/// built-in, by name, or `(none)` when no such file remains.
pub fn render(operator: &[Instruction]) -> String {
    let layers = resolve_layers(operator);
    let mut out = String::new();
    out.push_str(HEADING);
    out.push_str("\n\nBuilt-in:\n");
    for layer in &layers.builtins {
        let label = shadowed_by(layer).unwrap_or_else(|| "built-in".to_string());
        let heading = format!("{} ({label})", layer.name);
        match layer.state.in_force() {
            Some(body) => push_section(&mut out, &heading, body),
            None => push_heading(&mut out, &heading),
        }
    }
    out.push_str(&format!(
        "\nOperator ({}, filename order):\n",
        operator_path("*.md")
    ));
    if layers.operator.is_empty() {
        out.push_str("(none)\n");
    }
    for ins in layers.operator {
        push_section(&mut out, &ins.name, &ins.body);
    }
    out
}

/// One warning line per built-in an operator file replaces or switches off, in [`BUILTIN`]
/// order, read from [`resolve_layers`]: `warning: built-in instruction <name> is replaced by
/// .rigger/instructions/<name>.md` or `... is switched off by .rigger/instructions/<name>.md`.
/// A layer that shadows no built-in yields none. `rigger validate` prints each to stderr and
/// still succeeds.
pub fn shadow_advisories(operator: &[Instruction]) -> Vec<String> {
    resolve_layers(operator)
        .builtins
        .iter()
        .filter_map(|layer| {
            shadowed_by(layer)
                .map(|by| format!("warning: built-in instruction {} is {by}", layer.name))
        })
        .collect()
}

/// What shadows `layer`: `replaced by <file>` or `switched off by <file>`, the file being the
/// operator file named as the built-in, or `None` for a built-in no operator file shadows.
fn shadowed_by(layer: &BuiltInLayer) -> Option<String> {
    layer
        .state
        .shadow_verb()
        .map(|verb| format!("{verb} by {}", operator_path(&format!("{}.md", layer.name))))
}

/// `file` under the operator layer's directory, `.rigger/instructions`, as the reports name it.
fn operator_path(file: &str) -> String {
    format!("{RIGGER_DIR}/instructions/{file}")
}

fn push_section(out: &mut String, name: &str, body: &str) {
    push_heading(out, name);
    out.push_str(body.trim_end());
    out.push('\n');
}

/// A section's `## <name>` heading line, after a blank line.
fn push_heading(out: &mut String, name: &str) {
    out.push_str("\n## ");
    out.push_str(name);
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        assert_carries_the_review_method, builtin_body, builtin_section,
        house_and_team_operator_layer, op, rendered_shadowing_layer, shadowing_operator_layer,
        COMMUNICATION_NEAR_NAME, NON_EMPTY_BLANK_BODIES, RENDERED_OPERATOR_HEADING,
        REPLACING_COMMUNICATION_BODY, SHADOWING_LAYER_ADVISORIES,
    };

    #[test]
    fn compose_places_persona_then_builtin_then_operator_in_order() {
        let persona = "You are the persona.";
        let sys = compose(persona, &house_and_team_operator_layer());
        let persona = sys.find(persona).expect("persona leads");
        let heading = sys
            .find(HEADING)
            .expect("the instructions heading follows the persona");
        let builtin = sys
            .find("## engineering-principles")
            .expect("the built-in layer is present");
        let house = sys
            .find("## 10-house")
            .expect("the first operator file follows the built-ins");
        let team = sys
            .find("## 20-team")
            .expect("the second operator file follows the first");
        assert!(
            persona < heading && heading < builtin && builtin < house && house < team,
            "layers must appear in injection order; got:\n{sys}"
        );
        assert!(
            sys.contains("House rule.") && sys.contains("Team rule."),
            "operator bodies are carried verbatim"
        );
    }

    #[test]
    fn an_empty_persona_still_receives_every_built_in_layer() {
        let sys = compose("", &[]);
        assert!(
            sys.starts_with("\n\n# Rigger instructions"),
            "no persona means the layers open the prompt; got: {sys:?}"
        );
        for (name, body) in BUILTIN {
            assert!(
                sys.contains(&format!("## {name}")),
                "built-in {name:?} must be present"
            );
            assert!(
                sys.contains(body.trim_end()),
                "built-in {name:?} body must be carried verbatim"
            );
        }
    }

    #[test]
    fn the_built_in_law_names_every_principle_the_project_holds() {
        let (_, body) = BUILTIN[0];
        for phrase in [
            "Clean Architecture",
            "SOLID",
            "DRY",
            "KISS",
            "YAGNI",
            "TDD",
            "BDD",
        ] {
            assert!(
                body.contains(phrase),
                "the built-in law must name {phrase:?}"
            );
        }
    }

    #[test]
    fn the_built_in_layer_is_the_law_then_the_working_discipline_then_communication() {
        let names: Vec<&str> = BUILTIN.iter().map(|(name, _)| *name).collect();
        assert_eq!(
            names,
            [
                "engineering-principles",
                "working-discipline",
                "communication"
            ],
            "the built-in layer is exactly the law, the working discipline and the communication \
             discipline, in that order"
        );
    }

    /// Spec 120, criterion 14: the third built-in entry, which the layer-order test above names
    /// `communication`, carries the communication discipline's body: the old conductor
    /// constant's every line after its heading line, verbatim.
    #[test]
    fn the_third_built_in_is_the_communication_discipline_verbatim() {
        assert_eq!(
            BUILTIN.get(2).map(|(_, body)| *body),
            Some(
                "You are one of several agents working concurrently against a shared event log. \
                 Other agents act on what you record, live - so your communication is part of the \
                 work, not an afterthought.\n\
                 - Record EVERY decision the MOMENT you make it by calling the `rigger_emit` tool \
                 with type `DecisionMade`. Emit immediately, one decision at a time - never batch \
                 them up to the end of your turn, because a concurrent agent must see your \
                 decision while it is still acting.\n\
                 - If you are a reviewer, record EVERY finding the moment you find it by calling \
                 `rigger_emit` with type `ReviewFinding`. Do not hold findings back to a final \
                 summary.\n\
                 - Between your actions, CHECK the `rigger_peers` tool (scoped to the files you \
                 are touching) to stay aware of what concurrent agents have already decided. Do \
                 this before you commit to a direction, not after.\n\
                 - NEVER silently contradict a peer's decision. If you must change a direction a \
                 peer already set, SUPERSEDE it explicitly: emit a new `DecisionMade` whose \
                 `supersedes` field cites the prior decision's id, and state why. Diverging \
                 silently splits the work; superseding explicitly keeps every agent on one \
                 story.\n\
                 The exact JSON shapes for these emits are given in your task instructions; this \
                 section governs the DISCIPLINE and cadence - follow it on every turn.\n"
            ),
            "the third built-in carries the communication discipline's body verbatim"
        );
    }

    /// The built-in working discipline's body.
    fn working_discipline() -> &'static str {
        builtin_body("working-discipline")
    }

    /// The persona this module's `compose` tests compose over and [`expected_prompt`] expects;
    /// only the tests of the persona's place and of an empty persona name their own.
    const PERSONA: &str = "P.";

    /// The prompt `compose(PERSONA, ..)` must return: the persona, the bare [`HEADING`], then each
    /// named section in order, its body with trailing whitespace trimmed.
    fn expected_prompt(sections: &[(&str, &str)]) -> String {
        crate::test_support::expected_composed_prompt(PERSONA, sections)
    }

    /// Spec 120, criterion 2: an operator file named as a built-in byte for byte replaces that
    /// built-in's body in place, under the built-in's own heading; the file never also appends,
    /// and the layers open under the bare heading with no "(non-negotiable)" anywhere.
    #[test]
    fn a_same_named_operator_file_replaces_the_built_in_body_in_place_under_the_bare_heading() {
        let body = "Our team posts every decision in the run channel.\n- One line each.";
        let sys = compose(PERSONA, &[op("communication", body)]);
        assert_eq!(
            sys,
            expected_prompt(&[
                builtin_section("engineering-principles"),
                builtin_section("working-discipline"),
                ("communication", body),
            ]),
            "the replacing body sits byte for byte in the communication section, in the \
             built-in's place, and is not appended again"
        );
        assert!(
            !sys.contains("(non-negotiable)"),
            "no \"(non-negotiable)\" remains anywhere in the composed prompt; got:\n{sys}"
        );
    }

    /// A replacing body is printed with its one leading U+FEFF (a byte-order mark) stripped; the
    /// strip takes one only, so a second leading U+FEFF is the body's own text and stays.
    #[test]
    fn a_replacing_body_is_printed_without_its_leading_byte_order_mark() {
        for (file_body, printed) in [
            ("\u{feff}House discipline.", "House discipline."),
            (
                "\u{feff}\u{feff}Team discipline.",
                "\u{feff}Team discipline.",
            ),
        ] {
            let sys = compose(PERSONA, &[op("working-discipline", file_body)]);
            assert_eq!(
                sys,
                expected_prompt(&[
                    builtin_section("engineering-principles"),
                    ("working-discipline", printed),
                    builtin_section("communication"),
                ]),
                "the replacing body {file_body:?} loses its one leading byte-order mark and nothing else"
            );
        }
    }

    /// A name that differs from a built-in's by case or a trailing space shadows nothing: the
    /// built-in stays and the file appends after the built-ins, in the order given.
    #[test]
    fn a_near_name_shadows_nothing_and_appends_after_the_built_ins() {
        let sys = compose(
            PERSONA,
            &[
                op("Communication", "Case rule."),
                op(COMMUNICATION_NEAR_NAME, "Space rule."),
            ],
        );
        assert_eq!(
            sys,
            expected_prompt(&[
                builtin_section("engineering-principles"),
                builtin_section("working-discipline"),
                builtin_section("communication"),
                ("Communication", "Case rule."),
                (COMMUNICATION_NEAR_NAME, "Space rule."),
            ]),
            "a case change or a trailing space is another name, appended as an operator file"
        );
    }

    /// Spec 120, criterion 3: a same-named file whose body is blank once one leading U+FEFF is
    /// stripped and Unicode White_Space trimmed switches its built-in off: neither the built-in's
    /// section nor the file's is injected, and every other section is unchanged.
    #[test]
    fn a_blank_same_named_file_switches_its_built_in_off() {
        for blank in std::iter::once(Default::default()).chain(NON_EMPTY_BLANK_BODIES) {
            let sys = compose(PERSONA, &[op("engineering-principles", blank)]);
            assert_eq!(
                sys,
                expected_prompt(&[
                    builtin_section("working-discipline"),
                    builtin_section("communication"),
                ]),
                "the blank body {blank:?} injects no section for the built-in or the file"
            );
        }
    }

    /// The strip that decides blankness takes one leading U+FEFF only: a body of two is the
    /// second U+FEFF, not blank, so it replaces the built-in rather than switching it off.
    #[test]
    fn a_body_of_two_byte_order_marks_replaces_rather_than_switches_off() {
        let sys = compose(PERSONA, &[op("engineering-principles", "\u{feff}\u{feff}")]);
        assert_eq!(
            sys,
            expected_prompt(&[
                ("engineering-principles", "\u{feff}"),
                builtin_section("working-discipline"),
                builtin_section("communication"),
            ]),
            "one U+FEFF survives the strip, so the body is not blank and replaces"
        );
    }

    /// Two blank same-named files switch two built-ins off and leave the third built-in and the
    /// operator file between those files in place, under the bare heading; the review method the
    /// working discipline's `## When you review` section holds goes with it.
    #[test]
    fn two_blank_same_named_files_switch_two_built_ins_off_and_leave_the_operator_layer() {
        let sys = compose(
            PERSONA,
            &[
                op("communication", "\u{3000}"),
                op("40-ops", "Ops rule."),
                op("working-discipline", "\u{2028}"),
            ],
        );
        assert_eq!(
            sys,
            expected_prompt(&[
                builtin_section("engineering-principles"),
                ("40-ops", "Ops rule."),
            ]),
            "the two switched-off built-ins and their files print nothing; the rest is unchanged"
        );
        assert_carries_the_review_method("the prompt with working-discipline off", &sys, 0);
    }

    /// With every built-in switched off and no other operator file, the prompt is the persona
    /// and the bare heading alone.
    #[test]
    fn the_heading_stays_when_every_built_in_is_switched_off() {
        let sys = compose(
            PERSONA,
            &[
                op("engineering-principles", "\u{a0}"),
                op("working-discipline", "\u{feff} "),
                op("communication", "\u{85}"),
            ],
        );
        assert_eq!(
            sys,
            expected_prompt(&[]),
            "the heading is printed though no section remains"
        );
    }

    #[test]
    fn the_working_discipline_carries_one_pass_and_the_fan_out_helpers() {
        let body = working_discipline();
        for heading in [
            "## One pass, by excellence",
            crate::test_support::FAN_OUT_HEADING,
        ] {
            assert_eq!(
                body.matches(heading).count(),
                1,
                "the working discipline carries {heading:?} exactly once"
            );
        }
        for phrase in [
            "`lookup` (Haiku, graph-only): ONE instance PER GRAPH NODE",
            "`verify` (Sonnet, with a shell): ONE instance",
            "`.claude/agents/`",
            "never over files",
        ] {
            assert!(
                body.contains(phrase),
                "the fan-out section must name {phrase:?}"
            );
        }
        // Only a gate command takes a machine-wide build slot; an agent's build does not, so
        // the verify helper runs one at a time because builds compete for the machine, never
        // because they hold a slot.
        // Read by its words, so a rewrap of the markdown never moves either verdict.
        let words = body.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            words.contains("the machine-wide build slots belong to gate commands alone"),
            "the verify rule must name who holds the build slots"
        );
        assert!(
            !words.contains("builds hold the machine-wide build slots"),
            "an agent's build holds no machine-wide build slot"
        );
        // Narrowing an instrument is named for any gate, and the verify helper for any run,
        // never one gate's tool.
        for phrase in [
            "ONE instance for a build, a test run or a reproduction.",
            "Narrowing any gate's instrument (an exclusion list, a skip attribute, a lint \
             suppression, a lowered threshold, a filter that hides a case, a deleted or weakened \
             test, a `--force`)",
            "deleting the code a finding lives in",
            "is the same defect as the finding it hides and a failure even when the gate goes \
             green - the adversary and the adjudicator treat it as one.",
        ] {
            assert!(
                words.contains(phrase),
                "the working discipline must say {phrase:?}"
            );
        }
        // A blocked command is an error naming its alternative, never the end of the turn.
        assert!(
            words.contains(
                "A denied or blocked command comes back to you as an error that names its \
                 alternative; your turn is not over - use the alternative."
            ),
            "the working discipline must send a blocked agent to the alternative its error names"
        );
    }

    /// A headless spawn's session ends with its turn: the discipline states that once, in a
    /// section every agent reads, as the outcome (every job waited on inside the turn) and its
    /// check, and the denied-command sentence hangs under it. Read by its words, so a rewrap
    /// never moves a verdict.
    #[test]
    fn the_working_discipline_states_once_that_a_turns_end_is_the_sessions_end() {
        let body = working_discipline();
        const HEADING: &str = "## Your turn's end is your session's end";
        assert_eq!(
            body.matches(HEADING).count(),
            1,
            "the working discipline carries {HEADING:?} exactly once"
        );
        let words = body.split_whitespace().collect::<Vec<_>>().join(" ");
        for phrase in [
            "When you end your turn the process exits and nothing re-invokes you",
            "a job you start in the background and any watch you arm die with the process",
            "So you wait for every job inside your turn - a foreground command, or an \
             `until`-loop that polls its output - and read its result before you write a word \
             of your report.",
            "A command the host moves to the background when it reaches its timeout is such a \
             job",
            "Check: when you end your turn, no job you started is running and every claim in \
             your report names the output it rests on.",
        ] {
            assert!(
                words.contains(phrase),
                "the turn's-end section must say {phrase:?}"
            );
        }
        let section = words
            .find(HEADING)
            .expect("the turn's-end section is present");
        let denied = words
            .find("A denied or blocked command comes back to you")
            .expect("the denied-command sentence is present");
        let hand_over = words
            .find(crate::test_support::HAND_OVER_HEADING)
            .expect("the hand-over section is present");
        assert!(
            section < denied && denied < hand_over,
            "the denied-command sentence hangs under the turn's-end section"
        );
    }

    /// The hand-over section states each outcome an author's tree must meet before it reports,
    /// with the check that proves it, and no check owes a record: the gates green in the author's
    /// worktree, each run to completion inside the turn; generated files committed with their
    /// inputs; tests table-driven - the cases of one behaviour over different values rows of one
    /// table, never copies of a test function - and each table, fixture, helper, builder or
    /// oracle added existing once, looked up where the test helpers live and moved to the shared
    /// home when a second test home needs it, duplication the author did not write named as the
    /// tree's debt and never lifted, a lift's call-site rewrite never making a statement the
    /// author's, checked by the duplication gate and each behaviour reading as one table; every
    /// sentence about the changed behaviour true at head, checked by reading the changed files'
    /// module docs and the docs of the changed items and of their callers, read from the graph at
    /// depth 1, against head, a renamed or retired name's old spelling counted to zero or each
    /// remaining occurrence named; nothing added that nothing reads; the red test's body
    /// untouched by the green commit; every fact the production delta states - a SQL conjunct
    /// inside a query string included - with one home the author names; a whole-tree audit a
    /// rejection's pattern item requires fixing every site of the shape, its record bounded to
    /// each search with its hit count and each site fixed, never every hit listed or
    /// dispositioned, a search term found inside a longer identifier no hit; and a probe that
    /// edits the tree - the sharpest case of a turn's end being the session's end - run to
    /// completion and restored before the turn ends. No sentence-walk record, witness, rule
    /// search, one-home line or lens re-derivation is owed, and no hand mutation rule or
    /// per-literal shape search is carried. Read by its words, so a rewrap never moves a verdict.
    #[test]
    fn the_working_discipline_states_each_hand_over_outcome_and_its_check() {
        let body = working_discipline();
        assert_eq!(
            body.matches(crate::test_support::HAND_OVER_HEADING).count(),
            1,
            "the working discipline carries the hand-over section exactly once"
        );
        let words = body.split_whitespace().collect::<Vec<_>>().join(" ");
        for phrase in [
            "run each check yourself, in your worktree, before you report; the checks owe no \
             record.",
            "The gates confirm, never discover.",
            "Every gate the stage names is green on your committed tree before you report.",
            "regenerated and committed in the same commit that changed its inputs",
            "`git status --porcelain` is empty at head",
            "Tests are table-driven, and a test-side shape you add exists once.",
            "Cases that check one behaviour over different values are rows of one table run by \
             one body, never copies of a test function that differ only in their values; a case \
             for a behaviour an existing table covers is a row added to that table.",
            "A table, fixture, helper, builder or oracle you add is looked up first where the \
             project's test helpers live and reused when it exists; when a second test home \
             needs one the first already spells, it moves to the shared home and both call it, \
             no assertion or expected value changed.",
            "Duplication you did not write is the tree's debt: your hand-over names it and you \
             do not lift it, and a lift's call-site rewrite does not make a statement yours.",
            "Check: the project's duplication gate is green, and each behaviour your tests check \
             over several values reads as one table.",
            "Every sentence about the changed behaviour is true at head.",
            "module docs, test names, test docs, assertion messages and operator docs say what \
             the code at head does",
            "Check: read the module doc of each file the change touches, and the doc of each item \
             it touches and of each caller of that item, which the project's graph names at depth \
             1 (`rigger graph --around <file>::<item> --depth 1`), against the code at head, \
             leaving each true or fixing it",
            "when you rename or retire a name, walk every spelling of the old term through the \
             tree",
            "The old spelling occurs zero times in the tree outside git history, counted over \
             every root",
            "test docs, test comments and assertion messages are sentences too",
            "or your hand-over names each remaining occurrence as a deliberate mention with its \
             reason",
            "Nothing is added that nothing reads.",
            "makes the red test pass without rewriting its body",
            "Every fact your production delta states has one home.",
            "A predicate is a boolean expression or a SQL conjunct, one inside a query string, a \
             format string or a macro argument the production code executes included, read as \
             the code's own expression, each operator and literal of it. Check: name its home.",
            "A whole-tree audit a rejection requires is bounded.",
            "When a rejection's required items name a pattern - one defect shape recurring across \
             sites - every site of that shape is fixed.",
            "Check: the audit record the rejection asks for names each search command you ran \
             with its hit count and each site you fixed with its location at head, and counts the \
             rest; it never lists or dispositions every hit, and a search term found inside a \
             longer identifier is not a hit.",
            "A probe that edits your tree is over before your turn is.",
            "runs to completion and restores the tree before your turn ends, never as a \
             detached or background job",
            "the attempt checkpoint commits whatever the worktree holds when your session ends",
            "the sharpest case of the rule that your turn's end is your session's end",
            "each run to completion inside your turn",
        ] {
            assert!(
                words.contains(phrase),
                "the hand-over section must say {phrase:?}"
            );
        }
        // No check owes a record: the walk, its witnesses, its rule searches, its one-home lines
        // and a reviewing lens's re-derivation of them are gone from the built-in layer, which
        // carries the review rule that makes a misstated sentence or a second spelling a note.
        for retired in [
            "<unit>-sentence-walk",
            "one-home line",
            "witness",
            "rule search",
            "walked set",
            "re-derive",
            "BLOCKING",
            "The record is owed on every change",
            "review-complete record",
        ] {
            assert!(
                !words.contains(retired),
                "the working discipline owes no per-unit record or lens re-derivation: {retired:?}"
            );
        }
        // The hand-over owes no record of hand-applied mutations; rigger's core is agnostic to
        // the mutation testing a project may wire as a gate of its own.
        assert!(
            !words.contains("Every mutable token") && !words.contains("<unit>-mutant-probes"),
            "the hand-over section carries no hand mutation rule"
        );
        // The item's callers come from the graph at depth 1; a bare `--around` answers the
        // item's whole file neighbourhood, so the check never names the graph undepthed.
        assert!(
            !words.contains("(the project's graph, or your persona's fallback)"),
            "the caller read must name the graph's depth"
        );
        // Tests are table-driven and the duplication gate is the instrument for twins, so the
        // hand-over owes no per-literal shape search and no per-statement shape record.
        assert!(
            !words.contains("One search per literal")
                && !words.contains("The record is owed for every test-side shape"),
            "the hand-over owes no per-literal shape search and no per-statement shape record"
        );
    }

    #[test]
    fn every_built_in_section_heading_has_exactly_one_home() {
        let mut seen: Vec<(&str, &str)> = Vec::new();
        for (name, body) in BUILTIN {
            for line in body.lines().filter(|l| l.starts_with("## ")) {
                if let Some((other, _)) = seen.iter().find(|(_, h)| *h == line) {
                    panic!("{line:?} is in both {other:?} and {name:?}; a rule has one home");
                }
                seen.push((name, line));
            }
        }
        assert!(
            seen.len() >= 4,
            "the law and the discipline each carry their sections; got {seen:?}"
        );
    }

    #[test]
    fn compose_places_the_working_discipline_after_the_law_and_before_the_operator_layer() {
        let sys = compose(PERSONA, &house_and_team_operator_layer()[..1]);
        let law = sys.find("## engineering-principles").expect("the law");
        let discipline = sys.find("## working-discipline").expect(
            "the working discipline reaches this spawn, which no operator file switches off",
        );
        let house = sys.find("## 10-house").expect("the operator file");
        assert!(
            law < discipline && discipline < house,
            "law, then discipline, then operator; got:\n{sys}"
        );
    }

    #[test]
    fn render_lists_built_in_then_operator_and_says_none_for_an_empty_operator_layer() {
        let none = render(&[]);
        assert!(
            none.contains("Built-in")
                && none.contains("## engineering-principles")
                && none.contains("(none)"),
            "an empty operator layer renders as (none); got:\n{none}"
        );
        let some = render(&house_and_team_operator_layer()[..1]);
        assert!(
            !some.contains("(none)") && some.contains("## 10-house"),
            "operator files render by name; got:\n{some}"
        );
        assert!(
            some.find("engineering-principles").unwrap() < some.find("10-house").unwrap(),
            "built-ins render first"
        );
    }

    /// Over the named-entry fixture, `render` prints the built-in kept in force with its shipped
    /// body, the replaced one with the operator file's body and the switched-off one with no body,
    /// each labelled with its state, then the one file that shadows nothing.
    #[test]
    fn render_names_each_built_in_of_the_fixture_as_in_force_replaced_or_switched_off() {
        assert_eq!(
            render(&shadowing_operator_layer()),
            rendered_shadowing_layer(),
            "each built-in is printed with its state and only the unshadowed file is an operator file"
        );
    }

    /// An operator layer whose every file shadows a built-in leaves no operator file to print.
    #[test]
    fn render_says_none_when_every_operator_file_shadows_a_built_in() {
        let out = render(&shadowing_operator_layer()[1..]);
        assert!(
            out.ends_with(&format!("{RENDERED_OPERATOR_HEADING}(none)\n")),
            "an all-shadowing operator layer renders as (none); got:\n{out}"
        );
    }

    /// `shadow_advisories` names each built-in an operator file replaces or switches off, in
    /// `BUILTIN` order, and nothing for a layer that shadows none: an empty one, a near name, or a
    /// file of another name.
    #[test]
    fn shadow_advisories_name_each_shadowed_built_in_and_nothing_else() {
        assert_eq!(
            shadow_advisories(&shadowing_operator_layer()),
            SHADOWING_LAYER_ADVISORIES,
            "one advisory per replaced or switched-off built-in"
        );
        for layer in [
            Vec::new(),
            vec![op(COMMUNICATION_NEAR_NAME, REPLACING_COMMUNICATION_BODY)],
            shadowing_operator_layer()[..1].to_vec(),
        ] {
            assert_eq!(
                shadow_advisories(&layer),
                Vec::<String>::new(),
                "a layer that shadows no built-in advises nothing: {layer:?}"
            );
        }
    }
}
