//! The prompt-surface registry (spec 121, criterion 16): every text the binary sends an agent is
//! one `pub const` of its crate's `prompt_surface` module, its one home, whose doc comment opens
//! with its class, `CONTRACT:` (the binary parses or acts on it) or `FRAMING:` (it orients the
//! agent). A renderer formats from those consts, through [`fill`] wherever a const carries a
//! `{slot}`, so no prompt literal stays in a renderer. Each crate lists its texts as [`Group`]s
//! from its own `prompt_groups()`, grouped by surface and in render order within one, so a
//! surface's groups concatenate to its render. This module holds the registry types and the
//! domain's own texts: the spec critique prompt's and the composed system prompt's heading. It
//! is pure and compiles in the `core` lane.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompt_surface_fixtures::assert_registry;

    /// THE CLASS TABLE's domain rows: each const and the class its doc comment opens with.
    const CLASS_TABLE: &[(&str, &str)] = &[
        ("CRITIQUE_FRAMING", "FRAMING"),
        ("CRITIQUE_OUTPUT_CONTRACT", "CONTRACT"),
        ("CRITIQUE_RECORDED_FINDINGS", "CONTRACT"),
        ("CRITIQUE_SPEC_TAIL", "FRAMING"),
        ("INSTRUCTIONS_HEADING", "FRAMING"),
    ];

    /// Every domain prompt text is one const whose doc comment opens with THE CLASS TABLE's
    /// class, each const is a group's text, and no two groups share a surface and a name.
    #[test]
    fn every_domain_prompt_text_is_a_group_of_its_class_table_class() {
        assert_registry(
            include_str!("prompt_surface.rs"),
            prompt_groups(),
            CLASS_TABLE,
        );
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
}
