//! `rigger prime`, what the SessionStart hook runs: the instruction layers in force, then the
//! project's recent decisions, rendered from `rigger::prompt_surface`'s `PRIME_*` texts after the
//! domain's `INSTRUCTIONS_HEADING` (spec 121, criterion 16).

use super::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompt_surface_fixtures::{assert_renders, rendered_groups};

    /// `rigger prime`'s hook form (no spec argument) is its groups concatenated in registry order
    /// on each branch: no store, a store with no decision, and a store with two, each beside an
    /// operator file count of none or some. The built-ins in force and the decisions are `{slot}`
    /// values; the count, a number, takes a fixed one. The heading is the domain's
    /// `instructions::compose` group, and the line breaks and the decision line's skeleton are
    /// the test's.
    #[test]
    fn the_prime_output_is_its_groups_on_each_branch() {
        let pick = |names: &[&str]| {
            rendered_groups(
                rigger::prompt_surface::prompt_groups(),
                "prime::cmd_prime",
                names,
            )
        };
        let heading = rendered_groups(
            rigger_domain::prompt_surface::prompt_groups(),
            "instructions::compose",
            &["INSTRUCTIONS_HEADING"],
        );
        let in_force = |extra: &str| {
            format!(
                "{heading}{}",
                fill(&pick(&["PRIME_IN_FORCE"]), &[("extra", extra)])
            )
        };
        let operator_files = |n: &str| fill(&pick(&["PRIME_OPERATOR_FILES"]), &[("operator", n)]);
        let decision = |id: &str, summary: &str| PeerDecision {
            id: id.to_string(),
            summary: summary.to_string(),
            governs: Vec::new(),
            live: true,
        };
        let two = [
            decision("{id1}", "{summary1}"),
            decision("{id2}", "{summary2}"),
        ];
        let rows: [(&str, usize, Option<&[PeerDecision]>, String); 3] = [
            (
                "no store, no operator file",
                0,
                None,
                format!("{}\n{}\n", in_force(""), pick(&["PRIME_NO_STORE"])),
            ),
            (
                "a store with no decision, three operator files",
                3,
                Some(&[]),
                format!(
                    "{}\n{}\n{}\n",
                    in_force(&operator_files("3")),
                    pick(&["PRIME_DECISIONS_HEADER"]),
                    pick(&["PRIME_NONE_YET"]),
                ),
            ),
            (
                "a store with two decisions, one operator file",
                1,
                Some(&two),
                format!(
                    "{}\n{}\n- {{id1}}: {{summary1}}\n- {{id2}}: {{summary2}}\n",
                    in_force(&operator_files("1")),
                    pick(&["PRIME_DECISIONS_HEADER"]),
                ),
            ),
        ];
        for (case, operator, shown, expected) in rows {
            let render = format!(
                "{}\n{}",
                in_force_line("{builtin}", operator),
                decisions_block(shown)
            );
            assert_renders(case, &render, &expected);
        }
    }

    /// `rigger prime`'s first line lists the built-ins in force over the named-entry fixture, a
    /// replaced one marked and a switched-off one left out, and counts only the operator files
    /// that shadow no built-in: one, then none once the unshadowed file is dropped.
    #[test]
    fn the_in_force_line_names_each_built_in_s_state_and_counts_only_unshadowed_files() {
        use crate::test_support::{shadowing_operator_layer, SHADOWING_LAYER_IN_FORCE_LINE};
        assert_eq!(
            instructions_in_force_line(&shadowing_operator_layer()),
            SHADOWING_LAYER_IN_FORCE_LINE,
            "the replaced built-in is marked, the switched-off one omitted, one file counted"
        );
        assert_eq!(
            instructions_in_force_line(&shadowing_operator_layer()[1..]),
            "# Rigger instructions in force: engineering-principles, communication (replaced) - \
             see rigger instructions",
            "a file that shadows a built-in is never counted as an operator file"
        );
    }

    /// With every built-in switched off, `rigger prime`'s first line says none is in force.
    #[test]
    fn the_in_force_line_says_none_when_every_built_in_is_switched_off() {
        use crate::test_support::op;
        let off = |name| op(name, Default::default());
        assert_eq!(
            instructions_in_force_line(&[
                off("communication"),
                off("engineering-principles"),
                off("working-discipline"),
            ]),
            "# Rigger instructions in force: none - see rigger instructions",
            "no built-in is in force and no operator file remains"
        );
    }
}
