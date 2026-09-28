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
/// BDD where relevant, the one-pass-by-excellence rule and the mutation rule.
pub const BUILTIN: &[(&str, &str)] = &[(
    "engineering-principles",
    include_str!("instructions/engineering-principles.md"),
)];

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
            "One pass, by excellence",
            "A surviving mutant is always a failure",
        ] {
            assert!(
                body.contains(phrase),
                "the built-in law must name {phrase:?}"
            );
        }
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
