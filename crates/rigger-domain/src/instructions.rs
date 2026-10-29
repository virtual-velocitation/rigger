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
//! and compiles in the `core` lane. `rigger instructions` prints the composed layers ([`render`])
//! so an operator can read exactly what their agents are held to.

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
        match layer.state {
            LayerState::BuiltIn(body) | LayerState::Replaced(body) => {
                push_section(&mut out, layer.name, body)
            }
            LayerState::SwitchedOff => {}
        }
    }
    for ins in layers.operator {
        push_section(&mut out, &ins.name, &ins.body);
    }
    out
}

/// The text `rigger instructions` prints: every layer in injection order, named.
pub fn render(operator: &[Instruction]) -> String {
    let mut out = String::new();
    out.push_str(HEADING);
    out.push_str("\n\nBuilt-in (every agent rigger spawns, not removable):\n");
    for (name, body) in BUILTIN {
        push_section(&mut out, name, body);
    }
    out.push_str("\nOperator (.rigger/instructions/*.md, filename order):\n");
    if operator.is_empty() {
        out.push_str("(none)\n");
    }
    for ins in operator {
        push_section(&mut out, &ins.name, &ins.body);
    }
    out
}

fn push_section(out: &mut String, name: &str, body: &str) {
    out.push_str("\n## ");
    out.push_str(name);
    out.push('\n');
    out.push_str(body.trim_end());
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        assert_carries_the_review_method, builtin_body, builtin_section, COMMUNICATION_NEAR_NAME,
    };

    fn op(name: &str, body: &str) -> Instruction {
        Instruction {
            name: name.into(),
            body: body.into(),
        }
    }

    #[test]
    fn compose_places_persona_then_builtin_then_operator_in_order() {
        let sys = compose(
            "You are the persona.",
            &[op("10-house", "House rule."), op("20-team", "Team rule.")],
        );
        let persona = sys.find("You are the persona.").expect("persona leads");
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

    /// The prompt `compose("P.", ..)` must return: the persona, the bare [`HEADING`], then each
    /// named section in order, its body with trailing whitespace trimmed.
    fn expected_prompt(sections: &[(&str, &str)]) -> String {
        crate::test_support::expected_composed_prompt("P.", sections)
    }

    /// Spec 120, criterion 2: an operator file named as a built-in byte for byte replaces that
    /// built-in's body in place, under the built-in's own heading; the file never also appends,
    /// and the layers open under the bare heading with no "(non-negotiable)" anywhere.
    #[test]
    fn a_same_named_operator_file_replaces_the_built_in_body_in_place_under_the_bare_heading() {
        let body = "Our team posts every decision in the run channel.\n- One line each.";
        let sys = compose("P.", &[op("communication", body)]);
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
            let sys = compose("P.", &[op("working-discipline", file_body)]);
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
            "P.",
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
        for blank in ["", "\u{feff}", " \t\n", "\u{feff}\u{2003}\n"] {
            let sys = compose("P.", &[op("engineering-principles", blank)]);
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
        let sys = compose("P.", &[op("engineering-principles", "\u{feff}\u{feff}")]);
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
            "P.",
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
            "P.",
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
    /// with the check that proves it: the gates green in the author's worktree, generated files
    /// committed with their inputs, tests table-driven - the cases of one behaviour over
    /// different values rows of one table, never copies of a test function - and each table,
    /// fixture, helper, builder or oracle added existing once, looked up where the test helpers
    /// live and moved to the shared home when a second test home needs it, duplication the author
    /// did not write named as the tree's debt and never lifted, a lift's call-site rewrite never
    /// making a statement the author's, checked by the duplication gate and each behaviour
    /// reading as one table, with no per-literal search and no per-statement record owed, every
    /// sentence about the changed behaviour true at head over a walked set the author enumerates
    /// from its generators (an item's callers read from the graph at depth 1) over the unit's
    /// whole range on every round, never a round's delta, and recorded on every change in one
    /// bounded pass that satisfies the hand-over - each generator and search command with its
    /// hit count, never its hits, every FIXED sentence and added doc block with the `file:line`
    /// witness that makes it true, each added item with its one-home line and the CHECKED
    /// remainder as one count per command - the name count and the depth-1 caller read run for
    /// every item the hunks touch, a claim quantified over a set witnessed once per case the
    /// code splits it into, a failing case making it false, a sentence answering a rewritten
    /// rule's question a spelling of it, each spelling of a rule the change rewrites found by a
    /// recorded search per pair of short terms - the feature's name as the code spells it and
    /// the rule's noun phrase, both on one joined line, a sentence fragment as one term a record
    /// the author cannot write - for its old statement and each label its cases carried, a
    /// boolean term a production hunk adds to or removes from a filter, guard or predicate
    /// rewriting the rule its function answers, whose old statement its depth-1 callers' notes,
    /// Returns paragraphs, variant docs and oracles spell and are searched by in the same pair
    /// form (the function's name and the old answer's noun), the reviewing lens whose persona
    /// assigns the duty re-deriving each generator's set over that range, re-running each
    /// search and reading the hits itself against head, deriving the changed-condition set
    /// itself and reading every consumer sentence against it, running the two-term search
    /// itself for each rewritten rule and changed condition, and reading every sentence it finds
    /// claim by claim against the unit's change with a witness per claim at head, a quantified
    /// claim's cases enumerated by the lens and the claim read against each, a sentence at head
    /// that misstates the changed behaviour blocking on its own with the code that contradicts
    /// it and a missing per-hit disposition, count block or heading never a finding, its
    /// review-complete record bounded the same way - each command it ran with its hit count,
    /// each BLOCKING finding with its citations, its own witness for each FIXED sentence and
    /// added doc block and the remainder as one count per command with its verdict, no
    /// per-sentence listing owed - and a record a reviewer requires, such as a pattern audit in
    /// a rejection's required items, bounded the same way, never every hit listed or
    /// dispositioned, a search term found inside a longer identifier no hit, with a renamed
    /// name's old spelling counted to zero, nothing added that nothing reads, the red
    /// test's body untouched by the green commit, every constant, static, list, closure or
    /// predicate the delta adds - a SQL conjunct inside a query string included - naming its one
    /// home in the sentence-walk record - no other spelling, its distinguishing literal searched
    /// over the whole tree, derived from or pinned to the existing one, a fact spelled twice
    /// lifted at all three sites - every boolean expression the hunks add an item by shape with
    /// its line, a changed condition among them both searched and given its line, two
    /// predicates of one fact lifted to one home or pinned to each other, an added set of named
    /// members searched by each member's name, with the reviewing lens whose persona assigns the
    /// duty deriving the added set itself by the same shapes from the production hunks, query
    /// strings included, running the member search itself, and blocking on a missing or false
    /// line, and a probe that edits the tree - the sharpest case of a turn's end being the
    /// session's end - run to completion and restored before the turn ends; the gates' check
    /// names each gate command run to completion inside the turn. Read by its words, so a rewrap
    /// never moves a verdict.
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
            "module docs, test names, test docs, assertion messages and operator docs say what \
             the code at head does",
            "enumerate the walked set, each member from what generates it",
            "the module doc of every file the change touches (the changed-file list)",
            "the doc comment and every comment in the body of each item the change touches \
             (the hunks)",
            "the doc of the interface item a changed function or any caller of one implements \
             and of the interface a changed implementation is for (the implementation headers)",
            "the doc of every item a changed hunk names, callee, constant or type returned (the \
             hunk's identifiers)",
            "the doc of every caller of a changed item, read from the project's graph at depth \
             1 (`rigger graph --around <file>::<item> --depth 1`, the lines naming callers; the \
             default depth returns the item's whole file neighbourhood and is not the caller \
             set), or your persona's fallback",
            "every tracked line outside the hunks that spells a changed item's name",
            "read each against the code at head, leaving it true or fixing it",
            "The record is owed on every change and is bounded, so you write it in one pass: one \
             whole-tree DecisionMade carrying each generator and search command as run with its \
             hit count, never its pasted hits; every sentence FIXED, with its location and its \
             witness; each doc block the change adds, by location, with its witness; each item \
             the change adds with its one-home line; and the CHECKED remainder as one count per \
             command, no per-line disposition, per-item count block or heading owed.",
            "That record satisfies the hand-over. The name count and the depth-1 caller read run \
             once per item the hunks touch, new or pre-existing, its body or its doc changed.",
            "The walk's range is the unit's whole range, the merge base with the run branch to \
             head, on every round, never a round's delta alone",
            "a sentence you wrote in an earlier round is a member again whenever a later round \
             changes the behaviour it states",
            "a record, or a round's addendum, whose sets are re-derived over a round's delta is \
             a record you cannot write",
            "Every sentence the record marks FIXED, and every doc block it names as added, \
             carries its witness, the `file:line` of the code at head that makes it true",
            "the arm, the guard, the call, the constant; two witnesses when one would not make \
             the whole sentence true",
            "a sentence with no witness is a claim, not a record",
            "a claim that quantifies over a set - every, all, always, any, a plural subject with a \
             predicate - is witnessed once per case the code splits that set into (each arm that \
             assigns the value, each component of a key or comparator, each condition of a filter \
             or guard, each branch of a case statement), the record naming the cases and a witness \
             per case",
            "a case the claim does not hold for makes the sentence false, to be narrowed or \
             FIXED, never witnessed at the case it does hold for",
            "never witnessed at the case it does hold for. A sentence you leave CHECKED is read \
             the same way, claim by claim, and counted, not listed; a sentence whose claim is \
             the question a rule the change rewrites answers is a spelling of that rule, \
             whatever other rule its words also fit.",
            "has spellings no generator above reaches: prose that states it without naming a \
             changed item",
            "every spelling of each rewritten rule is walked too",
            "write down, for the rule's OLD statement and for each label its cases carried, a \
             pair of short terms: the feature's name as the code spells it (the flag \
             `--around`, the function, field, variant or table the rule concerns) and the rule's \
             shortest distinguishing noun phrase (`exact id`, `import`, `collision`)",
            "the words a sentence stating it cannot be written without, and each name the docs, \
             test names and comments give a case the old rule decided",
            "run one recorded search for each pair over every tracked text line: code, doc \
             comments, body comments, test names and docs, assertion messages, specs and \
             operator docs",
            "a hit is a line, joined with the line before, that holds both terms, in either \
             order and with any words, names or code spans between them",
            "A term is a name or a noun phrase of at most three words: a search whose one term is \
             a sentence fragment of more than three words misses the sentence that puts the \
             feature's name or a qualifier between its words, and is a record you cannot write.",
            "Every hit is read: one that spells the retired rule is FIXED, and one that does not \
             - a spec's account of the pre-change state, a history note, a different rule \
             sharing the words - is CHECKED and counted.",
            "A rewritten rule with no search is a walk not done; a change that rewrites no rule \
             runs no rule search.",
            "A rule is rewritten whenever a condition of it changes - a boolean term your \
             production hunks add to or remove from a filter, guard, case-arm guard or \
             predicate a function applies - even when no sentence of yours moves",
            "its old statement is what the function's consumers spell: the notes they print, \
             their Returns paragraphs, the docs of the variants the answer maps to and the \
             oracles that assert them",
            "read from the function's callers at depth 1 in the project's graph and their \
             callers' output sentences",
            "For each changed condition write down the rule it changes - the question the \
             function answers - and, for each phrase of the old answer as the consumers spell \
             it, the pair of the function's name as the code spells it and the old answer's \
             noun, and run the rule search for each pair",
            "every hit is FIXED, carrying the new qualifier, or CHECKED where the qualifier does \
             not reach it (a call that passes no path, say)",
            "The reviewing lens whose persona assigns the duty re-derives each generator's set on \
             the review tree from the diff and the graph",
            "each hunk's identifiers, cross-references and constants included",
            "the body comments of each changed item",
            "the depth-1 callers",
            "the tracked lines spelling a changed item's name",
            "over the unit's whole range, never the round's delta its prompt carries, re-runs \
             each recorded rule-spelling search and reads the hits itself against the code at \
             head.",
            "The lens derives the changed-condition set itself from the unit's production hunks \
             over the whole range - every added or removed boolean term inside a filter, guard \
             or predicate - names the rule each changes, and reads every consumer sentence of \
             that function - its depth-1 callers' notes, Returns paragraphs, variant docs and \
             oracles - against the condition at head.",
            "The lens runs the two-term search itself - the feature's name AND the rule's noun \
             phrase for each rule the change rewrites, the function's name AND the old answer's \
             noun for each changed condition it derives, both terms on one joined line with any \
             words between them.",
            "The lens reads each sentence it finds claim by claim against the unit's change, with \
             a witness per claim at head; for a quantified claim it enumerates the cases itself \
             from the code at head and reads the claim against each.",
            "A sentence at head that misstates the changed behaviour - a claim the code at head \
             does not make true, a quantified claim failing at any of its cases, a consumer \
             sentence stating a changed condition's pre-change answer - is a BLOCKING finding on \
             its own, citing its location and the code that contradicts it; a missing per-hit \
             disposition, per-item count block or heading in the record is never one.",
            "The lens's review-complete record is bounded the same way, so it is written in one \
             pass: each generator and search command the lens ran, with its hit count; each \
             BLOCKING finding, citing the sentence's location and the location of the code at \
             head that contradicts it; the lens's own witness at head for each sentence the \
             author's record marks FIXED and each doc block the change adds, one per case for a \
             quantified sentence; and the remainder it read as one count per command, with its \
             verdict.",
            "The remainder owes no per-sentence listing of claims and witnesses, and a counted \
             verdict over it is a record the lens can write; the lens still re-derives every \
             set, re-runs every search and reads every hit at head.",
            "A record a reviewer requires, such as a pattern audit among a rejection's required \
             items, is bounded the same way: each search command with its hit count; each hit \
             that states the audited shape, FIXED, with its witness at head; and the remainder \
             as one count per command, with its verdict.",
            "A required item never asks for every hit to be listed or dispositioned, and a search \
             term found inside a longer identifier is not a hit.",
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
             the code's own expression, each operator and literal of it.",
            "The `<unit>-sentence-walk` record carries, for each constant, static, list, closure \
             or predicate your production hunks add, its one-home line in one of three forms: \
             `no other spelling` with the terms searched - the name count and the graph lookup \
             the walk already runs, and the fact's distinguishing literal (a table name, a kind \
             string); `derived from <file:line>`, the hunk reading the existing spelling; or \
             `pinned to <file:line> by <test>`, where a derivation is impossible and a test \
             holds the two spellings equal.",
            "The `no other spelling` search runs the fragment's distinguishing literal (the column \
             with its operator, the subquery's table) over the whole tree, pre-existing spellings \
             included; a fact the tree already spells is `derived from` that spelling, the hunk \
             and the existing sites reading one function, or `pinned to` it, and a fact the tree \
             already spells twice is lifted at all three sites - the pre-existing pair is the \
             tree's debt, and the unit's third spelling makes it the unit's.",
            "The walk's item list is derived from the production hunks by shape as well as by \
             name, since a predicate has no name the count could list: every boolean expression \
             the hunks add - an `if`, `else if` or `while` condition or a pattern guard, each \
             operand of an `&&` or `||` chain inside one, the body of a closure passed to \
             `filter`, `any`, `all`, `retain`, `position` or `find`, and every conjunct inside a \
             query string, format string or macro argument the production code executes - is an \
             item, named by its `file:line`, with its one-home line.",
            "A predicate that is also a changed condition is both: its rule search runs and its \
             one-home line is carried, and neither stands in for the other.",
            "Two added predicates stating one fact - the same condition at two sites, or one \
             condition and its negation - are one fact spelled twice: lifted to one home both \
             sites read (a function or a value) or `pinned to` each other by a test that holds \
             them equal, never two `no other spelling` lines.",
            "An added set of named members - an array or slice of constants, an `IN (..)` list, a \
             pattern test or a case statement with an arm per member, an `||` chain of equalities \
             - runs its `no other spelling` search as the name count of EACH member, and every \
             site where two or more of the members occur within one statement, one case statement \
             or one macro call is read as a candidate second spelling of the set: one found makes \
             the set `derived from` a single named home both sites read (a const list or a \
             function), or the hunk's spelling is a DRY defect lifted before hand-over, and the \
             one-home line names the member sites it read.",
            "The reviewing lens whose persona assigns the duty derives the set of added constants, \
             statics, lists, closures and predicates itself from the production hunks, query \
             strings included, and compares it with the record's one-home lines: an added item the \
             record has no line for is a BLOCKING finding on its own.",
            "The reviewing lens whose persona assigns the duty reads the walk record's one-home \
             lines for every item the unit adds: an added item with no line, or a line false at \
             head - the fact has another spelling the line does not name, or the named derivation \
             or pin does not exist - is a BLOCKING finding on its own (DRY, single \
             responsibility).",
            "The reviewing lens whose persona assigns the duty derives the added set by the same \
             shapes, every added predicate and every added set of named members included, and \
             runs the member search itself for every added set: an added predicate with no line, \
             two added predicates of one fact with two lines, or a second spelling of a set the \
             line does not name, is a BLOCKING finding on its own.",
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
        // The hand-over owes no record of hand-applied mutations; rigger's core is agnostic to
        // the mutation testing a project may wire as a gate of its own.
        assert!(
            !words.contains("Every mutable token") && !words.contains("<unit>-mutant-probes"),
            "the hand-over section carries no hand mutation rule"
        );
        // The item's callers come from the graph at depth 1; a bare `--around` answers the
        // item's whole file neighbourhood, so the generator never names the graph undepthed.
        assert!(
            !words.contains("(the project's graph, or your persona's fallback)"),
            "the caller generator must name the graph's depth"
        );
        // Tests are table-driven and the duplication gate is the instrument for twins, so the
        // hand-over owes no per-literal shape search and no per-statement shape record.
        assert!(
            !words.contains("One search per literal")
                && !words.contains("The record is owed for every test-side shape"),
            "the hand-over owes no per-literal shape search and no per-statement shape record"
        );
        // A rule-spelling search conjoins the feature's name with the rule's noun phrase, so
        // the rule no longer offers a single phrase as the search's one term.
        assert!(
            !words.contains("two phrases when one would miss a spelling"),
            "the rule search runs a term pair, not one phrase or two"
        );
        // The sentence-walk record is bounded so it is written in one pass: it pastes no hit,
        // pairs no count block with an item and keeps no heading of its own for predicates.
        assert!(
            !words.contains("its hit count and every hit")
                && !words.contains("the record pairs each item")
                && !words.contains("ADDED PREDICATES"),
            "the sentence-walk record pastes no hit, pairs no count block and keeps no heading"
        );
        // The lens's review-complete record is bounded too: it counts the remainder it read with
        // a verdict instead of listing every sentence with its claims and witnesses.
        assert!(
            !words.contains("grouped verdict")
                && !words.contains("a record the lens cannot write")
                && !words.contains("its record lists them beside the claim"),
            "the lens's record counts its remainder with a verdict and lists no sentence"
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
        let sys = compose("P.", &[op("10-house", "House rule.")]);
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
        let some = render(&[op("10-house", "House rule.")]);
        assert!(
            !some.contains("(none)") && some.contains("## 10-house"),
            "operator files render by name; got:\n{some}"
        );
        assert!(
            some.find("engineering-principles").unwrap() < some.find("10-house").unwrap(),
            "built-ins render first"
        );
    }
}
