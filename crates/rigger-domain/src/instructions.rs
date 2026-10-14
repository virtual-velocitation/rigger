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
/// excellence, the hand-over an author's tree meets before it reports (each outcome with the
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
            "the one-pass section must send a blocked agent to the alternative its error names"
        );
    }

    /// The hand-over section states each outcome an author's tree must meet before it reports,
    /// with the check that proves it: the gates green in the author's worktree, generated files
    /// committed with their inputs, a test-side shape spelled once - any token two tests could
    /// spell - with every added shape recorded in a whole-tree audit whether merged or not, every
    /// sentence about the changed behaviour true at head over a walked set the author enumerates
    /// from its generators and records on every change, with a renamed name's old spelling
    /// counted to zero, nothing added that nothing reads, the red test's body untouched by the
    /// green commit, and a probe that edits the tree run to completion and restored before the
    /// turn ends. Read by its words, so a rewrap never moves a verdict.
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
            "the doc of every caller of a changed item (the project's graph, or your persona's \
             fallback)",
            "every tracked line outside the hunks that spells a changed item's name",
            "read each against the code at head, leaving it true or fixing it",
            "The record is owed on every change: one whole-tree DecisionMade naming the set \
             walked by its generators",
            "every sentence FIXED with its location",
            "a member whose generator returned nothing is named as empty",
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
        ] {
            assert!(
                words.contains(phrase),
                "the hand-over section must say {phrase:?}"
            );
        }
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
