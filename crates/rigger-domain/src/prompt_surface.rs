//! The prompt-surface registry (spec 121, criterion 16): every text the binary sends an agent is
//! one `pub const` of its crate's `prompt_surface` module, its one home, whose doc comment opens
//! with its class, `CONTRACT:` (the binary parses or acts on it) or `FRAMING:` (it orients the
//! agent). A renderer formats from those consts, through [`fill`] wherever a const carries a
//! `{slot}`, so no prompt literal stays in a renderer. Each crate lists its texts as [`Group`]s
//! from its own `prompt_groups()`, grouped by surface and in render order within one, so a
//! surface's groups concatenate to its render; the binary crate's `all_groups()` collects every
//! crate's list. This module holds the registry types, [`fill`] and [`debug_escaped`], and the
//! domain's own texts: the spec critique prompt's and the composed system prompt's heading. It is
//! pure and compiles in the `core` lane.

/// One sentence group of one class on one surface: `surface` names the renderer that prints it,
/// `name` is its const's identifier and `text` is that const, whose `{slot}` placeholders
/// [`fill`] replaces.
pub struct Group {
    pub surface: &'static str,
    pub name: &'static str,
    pub text: &'static str,
}

/// A crate's groups, a `&'static [Group]`, from its surfaces in registry order, each spelt once
/// beside its consts in render order: `prompt_groups!["surface" => [CONST, ...], ...]`. Each
/// group's name is its const's own identifier, so a group's name and its text are one spelling.
#[macro_export]
macro_rules! prompt_groups {
    ($($surface:literal => [$($text:ident),+ $(,)?]),+ $(,)?) => {
        &[$($($crate::prompt_surface::Group {
            surface: $surface,
            name: stringify!($text),
            text: $text,
        }),+),+]
    };
}

/// `text` with each `{slot}` token `slots` names replaced by its value, in one left-to-right
/// scan: an inserted value is never rescanned, and every other brace, a JSON shape's included,
/// stays literal.
pub fn fill(text: &str, slots: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let named = slots.iter().find_map(|(slot, value)| {
            let tail = after.strip_prefix(slot)?.strip_prefix('}')?;
            Some((value, tail))
        });
        match named {
            Some((value, tail)) => {
                out.push_str(value);
                rest = tail;
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// `s` as its `Debug` form prints it between its quotes: each quote, backslash and control
/// character escaped, so a value a prompt text quotes reads as one quoted string.
pub fn debug_escaped(s: &str) -> String {
    let quoted = format!("{s:?}");
    quoted[1..quoted.len() - 1].to_string()
}

/// FRAMING: the built-in layer's heading, opening the injected layers of every composed system
/// prompt; `instructions::render` and `rigger prime` print it too, and tests key on it.
pub const INSTRUCTIONS_HEADING: &str = "# Rigger instructions";

/// FRAMING: the spec critique's task framing and voided-duties clause: what is under critique,
/// how the critic's persona rules read on a spec, and the duties this task voids. The critique
/// method itself is the persona's, under its `## When you critique a spec or a plan` section.
pub const CRITIQUE_FRAMING: &str =
    "This is a critique of a SPEC, not code: the text below is a design \
     and its Done-when criteria, written before any run builds them. In your persona's critique \
     rules a unit reads as a criterion, and a reject as a BLOCKING finding. Every persona or \
     discipline duty that conflicts with a spec critique is void for this task: reviewing lenses \
     and a diff, the rule against rendering a verdict, running gates or any build or test command, \
     editing a file, and recording through `rigger_emit`, `rigger_progress` or `rigger_scratch`. \
     Read the spec and the repository; the finding lines and the verdict line of your final \
     message are the only output.\n\n";

/// CONTRACT: the critic's output contract, the finding line's five fields (the remedy field
/// included) and the verdict line the binary parses.
pub const CRITIQUE_OUTPUT_CONTRACT: &str = "The output contract: one finding per line, then the \
     verdict as the last line:\n\n\
     <critic id> | BLOCKING | <criterion n or Design block title> | <exact reading that breaks> \
     | <smallest Design change that closes it>\n\
     <critic id> | NON-BLOCKING | ...\n\
     {\"verdict\":\"reject\"}\n\n\
     A finding line has five `|`-separated fields whose second is exactly BLOCKING or \
     NON-BLOCKING; any other line is prose. `<critic id>` is your own short label for the \
     finding. The verdict is reject when any line is BLOCKING, else approve.\n\n";

/// CONTRACT: the clause that opens a pass's recorded findings (spec 123, THE PASS FRAMING IS
/// CONTRACT): each line after it is a finding the critique already holds, which the critic does
/// not report again.
pub const CRITIQUE_RECORDED_FINDINGS: &str = "The findings already recorded for this text \
     follow, one per line as <severity> | <location> | <reading> | <fix>. A finding among these \
     lines is recorded and is not reported again: report only findings not among them, and when \
     there is none to add, write no finding line and the verdict line {\"verdict\":\"approve\"}.";

/// FRAMING: the critique prompt's tail, the spec's path in `{spec_path}` and its text verbatim in
/// `{spec_text}`.
pub const CRITIQUE_SPEC_TAIL: &str =
    "The spec under critique, `{spec_path}`, verbatim:\n\n{spec_text}";

/// The domain's groups: the composed system prompt's heading, then the spec critique prompt's
/// four in render order.
pub fn prompt_groups() -> &'static [Group] {
    crate::prompt_groups![
        "instructions::compose" => [INSTRUCTIONS_HEADING],
        "review::spec_critique_prompt" => [
            CRITIQUE_FRAMING,
            CRITIQUE_OUTPUT_CONTRACT,
            CRITIQUE_RECORDED_FINDINGS,
            CRITIQUE_SPEC_TAIL,
        ],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompt_surface_fixtures::{
        assert_groups_name_the_consts, const_classes, registry_test, GroupPicks,
    };
    use crate::test_support::failure_message;

    /// THE CLASS TABLE's domain rows: each const and the class its doc comment opens with.
    const CLASS_TABLE: &[(&str, &str)] = &[
        ("CRITIQUE_FRAMING", "FRAMING"),
        ("CRITIQUE_OUTPUT_CONTRACT", "CONTRACT"),
        ("CRITIQUE_RECORDED_FINDINGS", "CONTRACT"),
        ("CRITIQUE_SPEC_TAIL", "FRAMING"),
        ("INSTRUCTIONS_HEADING", "FRAMING"),
    ];

    registry_test!(
        every_domain_prompt_text_is_a_group_of_its_class_table_class,
        CLASS_TABLE
    );

    /// The registry oracle refuses a `pub const` whose doc comment does not open with a class
    /// word and its colon, naming the const and the doc's first line: a const with no doc, a
    /// wrong word, a class word no colon follows, and a doc a blank line parts from its const.
    #[test]
    fn the_registry_oracle_refuses_a_const_whose_doc_opens_with_no_class_word() {
        // (the case, the module source, the refusal)
        let cases = [
            (
                "no doc",
                "pub const A: &str = \"a\";\n",
                "pub const A's doc opens with CONTRACT: or FRAMING:, not \"\"",
            ),
            (
                "a wrong word",
                "/// NOTE: a\npub const A: &str = \"a\";\n",
                "pub const A's doc opens with CONTRACT: or FRAMING:, not \"NOTE: a\"",
            ),
            (
                "a class word no colon follows",
                "/// CONTRACTS: a\npub const A: &str = \"a\";\n",
                "pub const A's doc opens with CONTRACT: or FRAMING:, not \"CONTRACTS: a\"",
            ),
            (
                "a doc a blank line parts from its const",
                "/// FRAMING: a\n\npub const A: &str = \"a\";\n",
                "pub const A's doc opens with CONTRACT: or FRAMING:, not \"\"",
            ),
        ];
        for (case, source, refusal) in cases {
            assert_eq!(failure_message(|| const_classes(source)), refusal, "{case}");
        }
    }

    /// An attribute line between a doc comment and its const keeps the doc, where a blank line
    /// parts them.
    #[test]
    fn the_registry_oracle_reads_a_doc_across_an_attribute_line() {
        assert_eq!(
            const_classes("/// FRAMING: a\n#[doc(hidden)]\npub const A: &str = \"a\";\n"),
            [("A".to_string(), "FRAMING".to_string())]
        );
    }

    /// The registry oracle refuses groups that list one (surface, name) pair twice and names the
    /// pair, while one const under two surfaces is two groups and passes.
    #[test]
    fn the_registry_oracle_refuses_a_surface_that_lists_a_const_twice() {
        let source = "/// FRAMING: a\npub const A: &str = \"a\";\n";
        let group = |surface| Group {
            surface,
            name: "A",
            text: "a",
        };
        assert_groups_name_the_consts(source, &[group("s"), group("t")]);
        assert_eq!(
            failure_message(|| assert_groups_name_the_consts(source, &[group("s"), group("s")])),
            "assertion `left == right` failed: no two groups share a surface and a name\n  \
             left: [(\"s\", \"A\")]\n right: []"
        );
    }

    /// One per-surface guard case: its name, each pick as its surface and names, and the
    /// refusal, when the guard refuses.
    type PicksCase<'a> = (&'a str, &'a [(&'a str, &'a [&'a str])], Option<&'a str>);

    /// A concatenation test's picks end checked: every group a surface it picked from lists was
    /// picked under that surface, else the test fails naming each unpicked (surface, name) pair
    /// in registry order, while a surface it never picked from is outside the check and a test
    /// already failing keeps its own message.
    #[test]
    fn group_picks_refuse_a_picked_surface_with_a_group_no_case_picks() {
        let group = |surface, name| Group {
            surface,
            name,
            text: name,
        };
        let groups = [
            group("s", "A"),
            group("s", "B"),
            group("s", "C"),
            group("t", "A"),
            group("t", "D"),
        ];
        let refusal = |unpicked: &str| {
            format!(
                "assertion `left == right` failed: each group a surface lists is picked by a \
                 case of that surface\n  left: [{unpicked}]\n right: []"
            )
        };
        let one = refusal("(\"s\", \"B\")");
        let two = refusal("(\"s\", \"A\"), (\"s\", \"C\")");
        let other = refusal("(\"s\", \"A\")");
        let misspelt = "assertion `left == right` failed: \"t\" holds each group the branch \
                        renders\n  left: []\n right: [\"X\"]";
        let cases: &[PicksCase] = &[
            ("no pick", &[], None),
            (
                "each group of each surface, over several picks",
                &[("s", &["A", "B"]), ("s", &["C"]), ("t", &["D", "A"])],
                None,
            ),
            ("a surface no case picks", &[("s", &["A", "B", "C"])], None),
            (
                "a group picked twice",
                &[("s", &["A", "B", "C"]), ("s", &["A"])],
                None,
            ),
            ("a group no case picks", &[("s", &["A", "C"])], Some(&one)),
            ("two groups no case picks", &[("s", &["B"])], Some(&two)),
            (
                "a group picked only under another surface",
                &[("t", &["A", "D"]), ("s", &["B", "C"])],
                Some(&other),
            ),
            (
                "a failing pick after a pick that leaves a group, failing with its own message",
                &[("s", &["A"]), ("t", &["X"])],
                Some(misspelt),
            ),
        ];
        for (case, picks, refused) in cases {
            let run = || {
                let recorder = GroupPicks::new(&groups);
                for (surface, names) in *picks {
                    recorder.rendered(surface, names);
                }
            };
            match refused {
                None => run(),
                Some(refused) => assert_eq!(failure_message(run), *refused, "{case}"),
            }
        }
    }

    /// One `fill` case: its name, the template, the slots passed and the filled text.
    type FillCase<'a> = (&'a str, &'a str, &'a [(&'a str, &'a str)], &'a str);

    /// `fill` scans the template once, left to right, and replaces only the `{slot}` tokens it is
    /// passed: an inserted value is never rescanned and every other brace stays literal.
    #[test]
    fn fill_replaces_only_the_named_slots_in_one_pass() {
        let cases: &[FillCase] = &[
            ("a named slot", "a {x} b", &[("x", "1")], "a 1 b"),
            ("an unnamed slot", "a {y} b", &[("x", "1")], "a {y} b"),
            ("no slot passed", "a {x} b", &[], "a {x} b"),
            ("a repeated slot", "{x}-{x}", &[("x", "1")], "1-1"),
            (
                "two slots",
                "{x} and {y}",
                &[("y", "2"), ("x", "1")],
                "1 and 2",
            ),
            (
                "a value is never rescanned",
                "{a}",
                &[("a", "{b}"), ("b", "2")],
                "{b}",
            ),
            (
                "a JSON shape",
                "{\"verdict\":\"reject\"} {x}",
                &[("x", "1")],
                "{\"verdict\":\"reject\"} 1",
            ),
            ("a doubled brace", "{{x}}", &[("x", "1")], "{1}"),
            ("an unclosed brace", "a {x b", &[("x", "1")], "a {x b"),
            ("a closing brace alone", "a } {x}", &[("x", "1")], "a } 1"),
            ("a brace at the end", "a {", &[("x", "1")], "a {"),
            (
                "a slot's prefix",
                "{ab} {a}",
                &[("a", "1"), ("ab", "2")],
                "2 1",
            ),
            ("an empty value", "a{x}b", &[("x", "")], "ab"),
            ("an empty template", "", &[("x", "1")], ""),
            ("no brace", "plain", &[("x", "1")], "plain"),
        ];
        for (case, text, slots, filled) in cases {
            assert_eq!(fill(text, slots), *filled, "{case}");
        }
    }

    /// `debug_escaped` is a value's `Debug` form less its outer quotes: each quote, backslash and
    /// control character escaped, every other character, the first and the last included, kept.
    #[test]
    fn debug_escaped_is_the_debug_form_between_its_quotes() {
        // (the case, the value, its escaped form)
        let cases = [
            ("plain text", "plain", "plain"),
            ("one character", "x", "x"),
            ("an empty value", "", ""),
            ("a quote", "a\"b", "a\\\"b"),
            ("a backslash", "a\\b", "a\\\\b"),
            ("a newline", "a\nb", "a\\nb"),
            ("a control character", "\u{1}", "\\u{1}"),
            ("a quote at each end", "\"x\"", "\\\"x\\\""),
            ("a letter outside ASCII", "\u{e9}", "\u{e9}"),
        ];
        for (case, value, escaped) in cases {
            assert_eq!(debug_escaped(value), escaped, "{case}");
        }
    }
}
