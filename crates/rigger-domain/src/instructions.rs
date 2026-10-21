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
//! spawns anywhere, for every consumer, regardless of persona text; an operator file named
//! byte for byte as a built-in entry, its body not blank, replaces that entry's body in the
//! entry's place ([`resolve_layers`]). The OPERATOR layer is every other `*.md` under
//! `.rigger/instructions/`, in filename order, read by the store-gated loader in
//! `config_store` and carried on `Config::instructions`, so this module stays pure and
//! compiles in the `core` lane. `rigger instructions` prints the composed layers ([`render`])
//! so an operator can read exactly what their agents are held to.

/// One instruction section: `name` is the file stem (or the built-in id) and `body` its text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instruction {
    pub name: String,
    pub body: String,
}

/// The built-in layer, in the order it is injected. The first entry is the engineering law
/// the project holds every change to: Clean Architecture, SOLID, DRY, KISS, YAGNI, TDD and
/// BDD where relevant. The second is the working discipline every agent follows: one pass by
/// excellence, a turn's end being the session's end (every job waited on inside the turn), the
/// hand-over an author's tree meets before it reports (each outcome with the
/// check that proves it), and fanning mechanical work out to the `lookup` and `verify` helpers.
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
/// trimmed) and whose body is not blank - empty once one leading U+FEFF is stripped and
/// Unicode White_Space trimmed - replaces that built-in ([`LayerState::Replaced`]) and leaves
/// the operator layer; every other file, a near name or a blank body included, stays in the
/// operator layer in the order given.
pub fn resolve_layers(operator: &[Instruction]) -> Layers<'_> {
    let shadows = |ins: &Instruction| {
        BUILTIN.iter().any(|(name, _)| ins.name == *name)
            && !without_bom(&ins.body).trim().is_empty()
    };
    let builtins = BUILTIN
        .iter()
        .map(|&(name, body)| BuiltInLayer {
            name,
            state: match operator.iter().find(|ins| ins.name == name && shadows(ins)) {
                Some(ins) => LayerState::Replaced(without_bom(&ins.body)),
                None => LayerState::BuiltIn(body),
            },
        })
        .collect();
    let operator = operator.iter().filter(|ins| !shadows(ins)).collect();
    Layers { builtins, operator }
}

/// `body` with its one leading U+FEFF (a byte-order mark) stripped.
fn without_bom(body: &str) -> &str {
    body.strip_prefix('\u{feff}').unwrap_or(body)
}

/// Compose `persona` followed by the built-in layer and then `operator`, each section under
/// its own `## <name>` heading beneath [`HEADING`]. An empty persona still receives every
/// layer; an empty operator layer adds nothing after the built-ins. It is the one authority for
/// every spawn's system prompt: the conductor, the canary runner and the spec critic all
/// thread its output, appending nothing after it.
pub fn compose(persona: &str, operator: &[Instruction]) -> String {
    let layers = resolve_layers(operator);
    let mut out = String::from(persona);
    out.push_str("\n\n");
    out.push_str(HEADING);
    out.push('\n');
    for layer in &layers.builtins {
        let body = match layer.state {
            LayerState::BuiltIn(body) => body,
            LayerState::Replaced(body) => body,
        };
        push_section(&mut out, layer.name, body);
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
    use crate::test_support::builtin_body;

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
        let mut out = format!("P.\n\n{HEADING}\n");
        for (name, body) in sections {
            out.push_str(&format!("\n## {name}\n{}\n", body.trim_end()));
        }
        out
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
                (
                    "engineering-principles",
                    builtin_body("engineering-principles")
                ),
                ("working-discipline", working_discipline()),
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

    /// A replacing body is printed with its one leading U+FEFF (a byte-order mark) stripped.
    #[test]
    fn a_replacing_body_is_printed_without_its_leading_byte_order_mark() {
        let sys = compose(
            "P.",
            &[op("working-discipline", "\u{feff}House discipline.")],
        );
        assert_eq!(
            sys,
            expected_prompt(&[
                (
                    "engineering-principles",
                    builtin_body("engineering-principles")
                ),
                ("working-discipline", "House discipline."),
                ("communication", builtin_body("communication")),
            ]),
            "the replacing body loses its leading byte-order mark and nothing else"
        );
    }

    /// A name that differs from a built-in's by case or a trailing space shadows nothing: the
    /// built-in stays and the file appends after the built-ins, in the order given.
    #[test]
    fn a_near_name_shadows_nothing_and_appends_after_the_built_ins() {
        let sys = compose(
            "P.",
            &[
                op("Communication", "Case rule."),
                op("communication ", "Space rule."),
            ],
        );
        assert_eq!(
            sys,
            expected_prompt(&[
                (
                    "engineering-principles",
                    builtin_body("engineering-principles")
                ),
                ("working-discipline", working_discipline()),
                ("communication", builtin_body("communication")),
                ("Communication", "Case rule."),
                ("communication ", "Space rule."),
            ]),
            "a case change or a trailing space is another name, appended as an operator file"
        );
    }

    /// A same-named file whose body is blank once one leading U+FEFF is stripped and Unicode
    /// White_Space trimmed replaces nothing: the built-in stays and the file appends as an
    /// operator file (spec 120 Design: so until the switch-off arm lands).
    #[test]
    fn a_blank_same_named_file_replaces_nothing_and_appends() {
        for blank in ["", "\u{feff}", " \t\n", "\u{feff}\u{2003}\n"] {
            let sys = compose("P.", &[op("engineering-principles", blank)]);
            assert_eq!(
                sys,
                expected_prompt(&[
                    (
                        "engineering-principles",
                        builtin_body("engineering-principles")
                    ),
                    ("working-discipline", working_discipline()),
                    ("communication", builtin_body("communication")),
                    ("engineering-principles", blank),
                ]),
                "the blank body {blank:?} leaves the built-in in force and appends"
            );
        }
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
    /// committed with their inputs, a test-side shape spelled once - any token two tests could
    /// spell, a named inline spelling included - with every added or named shape recorded in a
    /// whole-tree audit whether merged or not, the audit one search per distinguishing
    /// expression of each statement of every test-side body added and of each edited statement
    /// of a body edited, the hunks of the unit's whole range inside it, a twin among untouched
    /// statements named to the operator as the tree's debt - each string
    /// literal alone and in full, however short, and each outside call with its first argument,
    /// never the body's name nor an expression wrapping a literal, the record organised by
    /// statement, each added or edited statement quoted with its `file:line` above its terms -
    /// whose command and hits the record carries and the reviewing lens whose persona assigns the
    /// duty re-runs and re-derives from
    /// the test hunks over the same statements, comparing the record statement by statement, a
    /// missing term or an unlisted statement blocking, one search per literal alone, a literal of an
    /// untouched statement named to the operator as debt and never blocking, every
    /// sentence about the changed behaviour true at head over a walked set the author enumerates
    /// from its generators (an item's callers read from the graph at depth 1) over the unit's
    /// whole range on every round, never a round's delta, and records on every change, the name
    /// count and the depth-1 caller read paired with every item of the hunk list, each new
    /// or FIXED sentence with the `file:line` witness that makes it true, a claim quantified over
    /// a set witnessed once per case the code splits it into, a failing case making it false, a
    /// sentence CHECKED as a
    /// different rule shown to answer that rule's question and one answering a rewritten rule's
    /// question a spelling of it, each spelling of a rule the change rewrites found by a
    /// recorded search per pair of short terms - the feature's name as the code spells it and
    /// the rule's noun phrase, both on one joined line, a sentence fragment as one term a record
    /// the author cannot write - for its old statement and each label its cases carried, a
    /// boolean term a production hunk adds to or removes from a filter, guard or predicate
    /// rewriting the rule its function answers, whose old statement its depth-1 callers' notes,
    /// Returns paragraphs, variant docs and oracles spell and are searched by in the same pair
    /// form (the function's name and the old answer's noun), the reviewing lens whose persona
    /// assigns the duty re-deriving each
    /// generator's set over that range, reading every witness, re-running each search and
    /// re-reading every hit against head, deriving the changed-condition set itself and reading
    /// every consumer sentence against it, running the two-term search itself for each
    /// rewritten rule and changed condition, and reading every sentence it
    /// re-reads claim by claim against the unit's change with a witness per claim at head, a
    /// sentence it cannot witness blocking on its own, a quantified claim's cases enumerated by
    /// the lens and listed each with its witness, one witnessed case or a failing case blocking
    /// on its own, its record listing every sentence it
    /// reads, the CHECKED members included, never a grouped verdict, with a renamed name's old
    /// spelling counted to zero, nothing added that nothing reads, the red test's body untouched
    /// by the green commit, every constant, static, list, closure or predicate the delta adds - a
    /// SQL conjunct inside a query string included - naming its one home in the sentence-walk
    /// record - no other spelling, its distinguishing literal searched over the whole tree, derived
    /// from or pinned to the existing one, a fact spelled twice lifted at all three sites - every
    /// boolean expression the hunks add listed by shape under `ADDED PREDICATES` with its line, a
    /// changed condition among them in both lists, two predicates of one fact lifted to one home
    /// or pinned to each other, an added set of named members searched by each member's name,
    /// with the reviewing lens whose persona assigns the duty deriving the added set itself by
    /// the same shapes from the
    /// production hunks, query strings included, running the member search itself, and blocking
    /// on a missing or false line, and a probe that edits the tree - the
    /// sharpest case of a turn's end being the session's end - run to completion and restored
    /// before the turn ends; the gates'
    /// check names each gate command run to completion inside the turn. Read by its words, so
    /// a rewrap never moves a verdict.
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
            "exists once across every test home, as production code does",
            "look it up where the project's test helpers live before you write it",
            "move it to the shared home and have both call it, changing no assertion or \
             expected value",
            "A table, fixture, helper, builder, seeder, expected value or oracle you add",
            "the list is open - a shape is any test-side token two tests could spell",
            "a const or id literal, a hand-built event or fixture row, a closure or a query \
             string",
            "The record is owed for every test-side shape you add, merged or not:",
            "one whole-tree audit DecisionMade naming every site of its shape FIXED or CHECKED",
            "FIXED when moved to the shared home with both calling it and no assertion or \
             expected value changed, CHECKED when kept apart, with the reason",
            "Naming a shape adds it: a builder, fixture or helper that gives an existing inline \
             spelling a name exists once across every test home afterwards",
            "every site that already spelled it inline calls it, the shared home itself and any \
             file you edit included",
            "The search is an instrument, not a claim:",
            "its unit is the statement, never the body's name",
            "Walk every test-side body you add or edit - a test function, a helper, a fixture \
             builder, a table row's closure - statement by statement",
            "each statement that is not a single call into the shared test home is a shape of \
             its own",
            "A body you add is walked whole, every statement; a body you edit is walked in its \
             edited statements, the hunks of the unit's whole range inside it - the merge base \
             with the run branch to head, the range the sentence walk uses - since a statement you did not touch carries no shape you added.",
            "A twin the search surfaces only among statements you did not touch, spelling no \
             literal or call of yours, is the tree's duplication debt: your hand-over names it to \
             the operator, and you do not lift it.",
            "every string literal it spells, in full (an assertion, expect or panic message, a \
             fixture path, a query text)",
            "every call it makes outside the shared home with its first argument token",
            "A literal's term is the literal alone - its text complete, quotes included, and \
             nothing around it - never the call that passes it, a format string that embeds it, \
             the statement that spells it or two literals joined; an expression wrapping a \
             literal is a term of its own only where this rule names it, the outside call with \
             its first argument token.",
            "One search per literal, however short or conventional it reads - a one-letter \
             name, `\":memory:\"`, a fixture path - and every hit dispositioned; hits may share \
             one named reason, each hit still listed.",
            "A statement searched whole finds only a twin spelled identically, while the literal \
             finds the twin spelled differently around the same value, which is the twin this \
             rule exists for.",
            "A literal set aside as a convention, a fixture name or an argument style, with no \
             search of its own and no hit list, is a search you did not run.",
            "The lens compares the record's terms with the literals of the bodies it derives, \
             one search per literal alone: a literal with no search of its own, or a term that \
             is an expression wrapping a literal where the literal's own search is owed, is a \
             BLOCKING finding on its own.",
            "run one search for each over every test home",
            "a hit in another home - an existing helper's body or another test's inline \
             statement - is a twin, lifted to the shared home and called from both",
            "A record whose expressions are names only - a function name, the callee a helper \
             wraps - for a body that spells a literal or an outside call is a record you cannot \
             write either.",
            "A statement listed with a literal or call it spells absent from its terms is a \
             record you cannot write.",
            "The record is organised by statement: each statement of a body you add and each \
             edited statement of a body you edit is quoted with its `file:line`, followed by its \
             terms - every string literal it spells, alone, and every call it makes outside the \
             shared home with its first argument token - each term with its search, or the \
             statement is marked as a single call into the shared home with the callee named.",
            "The lens derives the expression set itself from the unit's test hunks at head - \
             every string literal and every outside call with its first argument in every \
             statement of each test-side body the unit adds and in the edited statements of each \
             body it edits, over the unit's whole range - and runs each over every test home: a \
             hit the record does not carry, or a record whose expressions are names only for a \
             body that spells either, is a BLOCKING finding on its own.",
            "The lens derives each statement's terms itself and compares the record statement by \
             statement: a statement whose record terms lack a literal or call the statement \
             spells, or a statement the hunks touch that the record does not list, is a BLOCKING \
             finding on its own, citing the statement and the term.",
            "A literal of a statement the unit did not touch is the tree's duplication debt, \
             which the lens names to the operator, never a BLOCKING finding.",
            "carrying each search's command, its hit count and every hit",
            "CHECKED when kept apart, with the reason: the named way its shape differs",
            "A CHECKED hit whose shape does not differ is a twin.",
            "A search you did not run is a record you cannot write.",
            "The reviewing lens whose persona assigns the duty re-runs each recorded search on the \
             review tree: a hit the record does not carry, or a CHECKED hit whose reason does not \
             hold, is a BLOCKING finding on its own.",
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
            "The record is owed on every change: one whole-tree DecisionMade naming the set \
             walked by its generators",
            "every sentence FIXED with its location",
            "a member whose generator returned nothing is named as empty",
            "The name count and the depth-1 caller read run once per item the hunks touch, new or \
             pre-existing, its body or its doc changed, and the record pairs each item of its \
             hunk list with its count, the lines read and its callers or the named fallback; an \
             item the hunk list names without its count, or without its callers or the named \
             fallback, is a record you cannot write.",
            "each sentence the change writes marked new with its location",
            "The walk's range is the unit's whole range, the merge base with the run branch to \
             head, on every round, never a round's delta alone",
            "a sentence you wrote in an earlier round is a member again whenever a later round \
             changes the behaviour it states",
            "a record, or a round's addendum, whose sets are re-derived over a round's delta is \
             a record you cannot write",
            "Every sentence the record marks new or FIXED carries its witness, the `file:line` \
             of the code at head that makes it true",
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
            "a CHECKED sentence whose truth rests on code names its witness the same way, and \
             one CHECKED as a different rule sharing the words names that rule instead",
            "names that rule instead and shows the sentence's claim is that rule's question, \
             the question the sentence answers read claim by claim as the lens reads it; a \
             sentence whose claim is the question a rule the change rewrites answers is a \
             spelling of that rule, whatever other rule its words also fit.",
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
            "The record carries each search's command, its hit count and every hit",
            "CHECKED with the named way it is not a spelling of the retired rule (a spec's \
             account of the pre-change state, a history note, a different rule sharing the \
             words)",
            "A rewritten rule with no search is a record you cannot write; a change that \
             rewrites no rule names the set empty.",
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
            "every hit is FIXED, carrying the new qualifier, or CHECKED with the named reason \
             the qualifier does not reach it (a call that passes no path, say)",
            "The reviewing lens whose persona assigns the duty re-derives each generator's set on \
             the review tree from the diff and the graph",
            "each hunk's identifiers, cross-references and constants included",
            "the body comments of each changed item",
            "the depth-1 callers",
            "the tracked lines spelling a changed item's name",
            "compares it with the walk record, and re-runs each recorded rule-spelling search \
             and re-reads every hit, CHECKED ones included, against the code at head: a member \
             the record omits, a hit the record does not carry, or a CHECKED hit whose reason \
             does not hold at head, is a BLOCKING finding on its own.",
            "The lens derives every set over the unit's whole range, as the record does, never \
             the round's delta its prompt carries, and reads every witness the record carries \
             against the code at head: a sentence whose witness no longer makes it true, a new \
             or FIXED sentence with no witness, or a record whose sets are re-derived over a \
             round's delta, is a BLOCKING finding on its own.",
            "The lens derives the changed-condition set itself from the unit's production hunks \
             over the whole range - every added or removed boolean term inside a filter, guard \
             or predicate - names the rule each changes, and reads every consumer sentence of \
             that function - its depth-1 callers' notes, Returns paragraphs, variant docs and \
             oracles - against the condition at head: a consumer sentence stating the \
             pre-change answer that the record does not carry FIXED or CHECKED is a BLOCKING \
             finding on its own.",
            "The lens runs the two-term search itself - the feature's name AND the rule's noun \
             phrase for each rule the change rewrites, the function's name AND the old answer's \
             noun for each changed condition it derives, both terms on one joined line with any \
             words between them: a hit the record lacks, or a hit CHECKED with a reason that does \
             not hold at head, is a BLOCKING finding on its own.",
            "The lens reads every sentence it re-reads claim by claim against the unit's \
             change, with a witness per claim at head, and a sentence it cannot witness is a \
             BLOCKING finding on its own.",
            "For a quantified claim the lens enumerates the cases itself from the code at head \
             and its record lists them beside the claim, each with its witness",
            "a quantified claim witnessed at one case, or a case the claim does not hold for, is \
             a BLOCKING finding on its own.",
            "The lens's record lists every sentence it reads, the record's CHECKED members \
             included, each with its claims and a witness per claim; a grouped verdict over any \
             of them is a record the lens cannot write.",
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
            "The `<unit>-sentence-walk` record carries, beside the count of each constant, \
             static, list, closure or predicate your production hunks add, its one-home line in \
             one of three forms: `no other spelling` with the terms searched - the name count and \
             the graph lookup the walk already runs, and the fact's distinguishing literal (a \
             table name, a kind string); `derived from <file:line>`, the hunk reading the \
             existing spelling; or `pinned to <file:line> by <test>`, where a derivation is \
             impossible and a test holds the two spellings equal.",
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
             item, quoted with its `file:line` under the record's `ADDED PREDICATES` heading with \
             its one-home line.",
            "A predicate that is also a changed condition appears in both lists, its rule search \
             in one and its one-home line in the other, and neither entry stands in for the \
             other.",
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
             shapes, the `ADDED \
             PREDICATES` list and every added set of named members included, and runs the member \
             search itself for every added set: an added predicate with no line, two added \
             predicates of one fact with two lines, or a second spelling of a set the line does \
             not name, is a BLOCKING finding on its own.",
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
        // A shape's search terms are its statements' literals and outside calls, never the
        // body's name, so the rule no longer sends the author to a whole-body token sequence.
        assert!(
            !words.contains("the token sequence its body cannot be written without"),
            "the shape search's unit is the statement, not the whole body"
        );
        // A rule-spelling search conjoins the feature's name with the rule's noun phrase, so
        // the rule no longer offers a single phrase as the search's one term.
        assert!(
            !words.contains("two phrases when one would miss a spelling"),
            "the rule search runs a term pair, not one phrase or two"
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
        let discipline = sys
            .find("## working-discipline")
            .expect("the working discipline reaches every spawn");
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
