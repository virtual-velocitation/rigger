//! Instruction injection: rigger is a harness, so the instructions every spawned agent
//! receives are LAYERED, not hard-coded into personas. A spawn's system prompt is composed by
//! [`compose`] as
//!
//! ```text
//! persona  +  BUILT-IN instructions  +  OPERATOR instructions  (+ the communication discipline,
//!                                                                appended by the conductor)
//! ```
//!
//! The BUILT-IN layer ships inside the binary ([`BUILTIN`]) and reaches every agent rigger
//! spawns anywhere, for every consumer, regardless of persona text; no configuration removes
//! it. The OPERATOR layer is every `*.md` under `.rigger/instructions/`, in filename order,
//! read by the store-gated loader in `config_store` and carried on `Config::instructions`, so
//! this module stays pure and compiles in the `core` lane. `rigger instructions` prints the
//! composed layers ([`render`]) so an operator can read exactly what their agents are held to.

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
/// check that proves it), fanning mechanical work out to the `lookup` and `verify` helpers, and
/// the rule that a surviving mutant is always a failure. A harness-wide rule lives here once,
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
];

/// The heading that opens the injected layers in a system prompt; tests and `rigger prime`
/// key on it.
pub const HEADING: &str = "# Rigger instructions";

/// Compose `persona` followed by the built-in layer and then `operator`, each section under
/// its own `## <name>` heading beneath [`HEADING`]. An empty persona still receives every
/// layer; an empty operator layer adds nothing after the built-ins.
pub fn compose(persona: &str, operator: &[Instruction]) -> String {
    let mut out = String::from(persona);
    out.push_str("\n\n");
    out.push_str(HEADING);
    out.push_str(" (non-negotiable)\n");
    for (name, body) in BUILTIN {
        push_section(&mut out, name, body);
    }
    for ins in operator {
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
    fn the_built_in_layer_is_the_law_then_the_working_discipline_in_that_order() {
        let names: Vec<&str> = BUILTIN.iter().map(|(name, _)| *name).collect();
        assert_eq!(
            names,
            ["engineering-principles", "working-discipline"],
            "the built-in layer is exactly the law followed by the working discipline"
        );
    }

    /// The built-in working discipline's body.
    fn working_discipline() -> &'static str {
        BUILTIN
            .iter()
            .find(|(name, _)| *name == "working-discipline")
            .map(|(_, body)| *body)
            .expect("the working-discipline entry is built in")
    }

    #[test]
    fn the_working_discipline_carries_one_pass_the_fan_out_helpers_and_the_survivor_rule() {
        let body = working_discipline();
        for heading in [
            "## One pass, by excellence",
            "## Fan out the mechanical work",
            "## A surviving mutant is always a failure",
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
            .find("## Hand over a tree the gates only confirm")
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
    /// expression of each statement of every test-side body added or edited - each string
    /// literal in full and each outside call with its first argument, never the body's name -
    /// whose command and hits the record carries and the SDET lens re-runs and re-derives from
    /// the test hunks, every
    /// sentence about the changed behaviour true at head over a walked set the author enumerates
    /// from its generators (an item's callers read from the graph at depth 1) over the unit's
    /// whole range on every round, never a round's delta, and records on every change, each new
    /// or FIXED sentence with the `file:line` witness that makes it true, each spelling of a rule the change rewrites found by a recorded search per
    /// distinguishing phrase of its old statement and of each label its cases carried, a
    /// boolean term a production hunk adds to or removes from a filter, guard or predicate
    /// rewriting the rule its function answers, whose old statement its depth-1 callers' notes,
    /// Returns paragraphs, variant docs and oracles spell and are searched by, the SDET
    /// lens re-deriving each generator's set over that range, reading every witness, re-running
    /// each search and re-reading every hit against head, deriving the changed-condition set
    /// itself and reading every consumer sentence against it, with a renamed name's old spelling
    /// counted to zero, nothing added that nothing reads, the red test's body untouched by the
    /// green commit, and a probe that edits the tree - the sharpest case of a turn's end being
    /// the session's end - run to completion and restored before the turn ends; the gates'
    /// check names each gate command run to completion inside the turn. Read by its words, so
    /// a rewrap never moves a verdict.
    #[test]
    fn the_working_discipline_states_each_hand_over_outcome_and_its_check() {
        let body = working_discipline();
        assert_eq!(
            body.matches("## Hand over a tree the gates only confirm")
                .count(),
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
            "Walk every test-side body you add or edit - a test fn, a helper, a fixture \
             builder, a table row's closure - statement by statement",
            "each statement that is not a single call into the shared test home is a shape of \
             its own",
            "every string literal it spells, in full (an assertion, expect or panic message, a \
             fixture path, a query text)",
            "every call it makes outside the shared home with its first argument token",
            "run one search for each over every test home",
            "a hit in another home - an existing helper's body or another test's inline \
             statement - is a twin, lifted to the shared home and called from both",
            "A record whose expressions are names only - a fn name, the callee a helper wraps - \
             for a body that spells a literal or an outside call is a record you cannot write \
             either.",
            "The lens derives the expression set itself from the unit's test hunks at head - \
             every string literal and every outside call with its first argument in each \
             test-side body the unit adds or edits - and runs each over every test home: a hit \
             the record does not carry, or a record whose expressions are names only for a body \
             that spells either, is a BLOCKING finding on its own.",
            "carrying each search's command, its hit count and every hit",
            "CHECKED when kept apart, with the reason: the named way its shape differs",
            "A CHECKED hit whose shape does not differ is a twin.",
            "A search you did not run is a record you cannot write.",
            "The SDET lens re-runs each recorded search on the review tree: a hit the record \
             does not carry, or a CHECKED hit whose reason does not hold, is a BLOCKING finding \
             on its own.",
            "module docs, test names, test docs, assertion messages and operator docs say what \
             the code at head does",
            "enumerate the walked set, each member from what generates it",
            "the `//!` module doc of every file the change touches (the changed-file list)",
            "the doc comment and every comment in the body of each item the change touches \
             (the hunks)",
            "the doc of the trait item a changed fn or any caller of one implements and of the \
             trait a changed impl is for (the impl headers)",
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
            "a CHECKED sentence whose truth rests on code names its witness the same way, and \
             one CHECKED as a different rule sharing the words names that rule instead",
            "has spellings no generator above reaches: prose that states it without naming a \
             changed item",
            "every spelling of each rewritten rule is walked too",
            "the distinguishing phrases of the rule's OLD statement and of the labels its cases \
             carried - the words a sentence stating it cannot be written without, and each name \
             the docs, test names and comments give a case the old rule decided",
            "two phrases when one would miss a spelling",
            "run one recorded search for each over every tracked text line: code, doc comments, \
             body comments, test names and docs, assertion messages, specs and operator docs",
            "The record carries each search's command, its hit count and every hit",
            "CHECKED with the named way it is not a spelling of the retired rule (a spec's \
             account of the pre-change state, a history note, a different rule sharing the \
             words)",
            "A rewritten rule with no search is a record you cannot write; a change that \
             rewrites no rule names the set empty.",
            "A rule is rewritten whenever a condition of it changes - a boolean term your \
             production hunks add to or remove from a filter, guard, match-arm guard or \
             predicate a function applies - even when no sentence of yours moves",
            "its old statement is what the function's consumers spell: the notes they print, \
             their Returns paragraphs, the docs of the variants the answer maps to and the \
             oracles that assert them",
            "read from the function's callers at depth 1 in the project's graph and their \
             callers' output sentences",
            "For each changed condition write down the rule it changes - the question the \
             function answers - and the distinguishing phrases of the old answer as the \
             consumers spell them, and run the rule search for each",
            "every hit is FIXED, carrying the new qualifier, or CHECKED with the named reason \
             the qualifier does not reach it (a call that passes no path, say)",
            "The SDET lens re-derives each generator's set on the review tree from the diff and \
             the graph",
            "each hunk's identifiers, doc links (`[...]`) and constants included",
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
            "when you rename or retire a name, walk every spelling of the old term through the \
             tree",
            "The old spelling occurs zero times in the tree outside git history, counted over \
             every root",
            "test docs, test comments and assertion messages are sentences too",
            "or your hand-over names each remaining occurrence as a deliberate mention with its \
             reason",
            "Nothing is added that nothing reads.",
            "makes the red test pass without rewriting its body",
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
