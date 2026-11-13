//! `rigger prime`, what the SessionStart hook runs: the instruction layers in force, then the
//! project's recent decisions, rendered from `rigger::prompt_surface`'s `PRIME_*` texts after the
//! domain's `INSTRUCTIONS_HEADING` (spec 121, criterion 16).

use super::*;
use rigger::prompt_surface::{
    PRIME_DECISIONS_HEADER, PRIME_IN_FORCE, PRIME_NONE_YET, PRIME_NO_STORE, PRIME_OPERATOR_FILES,
};
use rigger_domain::prompt_surface::{fill, INSTRUCTIONS_HEADING};

/// The first line `rigger prime` prints: the instruction layers in force for this project, read
/// from `instructions::resolve_layers` over its `operator` files, so a session starts knowing
/// which law its agents are held to. It lists the built-ins in force in `BUILTIN` order, a
/// replaced one as `<name> (replaced)` and a switched-off one not at all (`none` when no built-in
/// is in force), and counts only the operator files that shadow no built-in.
fn instructions_in_force_line(operator: &[instructions::Instruction]) -> String {
    let layers = instructions::resolve_layers(operator);
    let in_force = layers
        .builtins
        .iter()
        .filter_map(|layer| {
            layer
                .state
                .in_force()
                .map(|_| match layer.state.shadow_verb() {
                    Some(verb) => format!("{} ({verb})", layer.name),
                    None => layer.name.to_string(),
                })
        })
        .collect::<Vec<_>>();
    let builtin = if in_force.is_empty() {
        "none".to_string()
    } else {
        in_force.join(", ")
    };
    in_force_line(&builtin, layers.operator.len())
}

/// The instructions-in-force line over `builtin`, the built-ins in force, and the count of
/// `operator_files`: the domain's [`INSTRUCTIONS_HEADING`], then [`PRIME_IN_FORCE`], whose
/// `{extra}` is [`PRIME_OPERATOR_FILES`] only when an operator file is in force.
fn in_force_line(builtin: &str, operator_files: usize) -> String {
    let extra = match operator_files {
        0 => String::new(),
        operator => fill(PRIME_OPERATOR_FILES, &[("operator", &operator.to_string())]),
    };
    format!(
        "{INSTRUCTIONS_HEADING}{}",
        fill(PRIME_IN_FORCE, &[("builtin", builtin), ("extra", &extra)])
    )
}

/// The decisions lines `rigger prime` prints after its in-force line, each ending in a newline:
/// [`PRIME_NO_STORE`]
/// for a project with no store (`None`), else [`PRIME_DECISIONS_HEADER`], then a `- <id>:
/// <summary>` line per `shown` decision, or [`PRIME_NONE_YET`] when none shows.
fn decisions_block(shown: Option<&[PeerDecision]>) -> String {
    let Some(shown) = shown else {
        return format!("{PRIME_NO_STORE}\n");
    };
    let lines: String = shown
        .iter()
        .map(|d| format!("- {}: {}\n", d.id, d.summary))
        .collect();
    let none_yet = if shown.is_empty() {
        format!("{PRIME_NONE_YET}\n")
    } else {
        String::new()
    };
    format!("{PRIME_DECISIONS_HEADER}\n{lines}{none_yet}")
}

/// `rigger prime [<spec>]` - print recent decisions (what the SessionStart hook runs), and,
/// when a spec path is given, the DISCOVERABILITY next step naming the spec lint (spec 66,
/// criterion 5) so an operator or agent about to drive `rigger run <spec>` finds `rigger
/// validate <spec>` instead of stumbling onto it by accident. `args.first()` is the spec
/// path; the bare `rigger prime` the installed hook actually runs carries none, and stays
/// exactly as it was - no lint mention. The decisions shown are the ten newest, newest first.
pub(crate) fn cmd_prime(args: &[String]) -> Res {
    let spec_path = args.first();
    // A session started in a subdirectory or a unit worktree of a git repository reads the
    // instruction layer of the project at or above its place in the main checkout, never a
    // worktree's own, and outside git the layer at the cwd, with or without a store and inside a
    // gate's store fence alike ([`project_instruction_layer`]); its store and identity come from
    // the one store-location authority every courier resolves through.
    println!(
        "{}",
        instructions_in_force_line(&project_instruction_layer()?)
    );
    let shown = match store_if_any()? {
        None => None,
        Some((loc, selection)) => {
            // Every run's decisions BY TYPE on this project's run stream (spec 101): a session
            // start reads the decisions it prints and nothing else - no derived event, no other
            // project's.
            let decisions = over_the_run_store_at(&loc, &selection, |store| {
                Ok(store.read_stream_typed(
                    conductor::STREAM,
                    0,
                    rigger::eventstore::TypeSelection::Only(&[contextgraph::TYPE_DECISION_MADE]),
                )?)
            })?;
            Some(
                decisions
                    .iter()
                    .rev()
                    .filter_map(|e| serde_json::from_slice::<PeerDecision>(&e.data).ok())
                    .take(10)
                    .collect::<Vec<_>>(),
            )
        }
    };
    print!("{}", decisions_block(shown.as_deref()));
    if let Some(spec) = spec_path {
        println!("{}", spec_lint_next_step(spec));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompt_surface_fixtures::{assert_renders, rendered_groups};

    /// `rigger prime`'s hook form (no spec argument) is its groups concatenated in registry order
    /// on each branch: no store, a store with no decision, and a store with two, each beside an
    /// operator file count of none or some. The render composes the two seams `cmd_prime` prints,
    /// `in_force_line` then `decisions_block`, with no store read; the binary's whole stdout is
    /// pinned through the periphery. The built-ins in force and the decisions are `{slot}`
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
