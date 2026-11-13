//! The binary crate's prompt texts (spec 121, criterion 16), each one `pub const` whose doc
//! comment opens with its class: `rigger grep-guard`'s deny reason, `rigger guard-write`'s deny
//! reason and the stderr texts of its exit-2 refusals, and `rigger prime`'s fixed texts. The
//! renderers in `src/cli` format from these through `fill` wherever a const carries a `{slot}`.
//! [`all_groups`] is the one enumeration of every crate's registry groups. The registry types
//! and `fill` are `rigger_domain::prompt_surface`'s. The consts and this crate's own groups are
//! pure and compile in every lane; the outer crates' groups carry the lib's store predicate.

use rigger_domain::prompt_surface::Group;

/// CONTRACT: the graph-first lookup hook's stated bounce message, whose `# --literal` marker the
/// guard parses: first the combined lookup, which answers a name, path or word from the graph,
/// the caller's tree and the project's prose in one call, then the graph calls that answer the
/// narrower name lookups (who calls a name, where a name is defined), with a `<name>` placeholder
/// so it names no language, then the guard's rule and its escape hatch. The hatch is named in the
/// one spelling that survives a sibling PreToolUse hook's rewrite: `--literal` inside a trailing
/// shell comment, which the shell discards before the command runs
/// (lesson-u101c2r3-grep-guard-literal-not-stripped).
pub const GREP_GUARD_MESSAGE: &str =
    "A name lookup is a graph call: rigger_graph `lookup` <term>... \
     answers a name, path or word from the graph, your working tree and the project's prose in one \
     call; who calls <name> is rigger_graph `around` <name>, and where <name> is defined is \
     rigger_graph `show` <name>. A grep, or a sed or awk pattern search, is for literal text - end \
     the command with a `# --literal` comment to proceed";

/// FRAMING: `rigger guard-write`'s deny reason, naming in `{root}` the first root the write falls
/// outside of, on every denial, the unreadable-target one included.
pub const GUARD_WRITE_DENY: &str = "write target is outside the allowed root: {root}";

/// FRAMING: the prefix of every exit-2 stderr text of `rigger guard-write`.
pub const GUARD_WRITE_STDERR_PREFIX: &str = "rigger guard-write: ";

/// FRAMING: the refusal of a `--root` with no directory after it.
pub const GUARD_WRITE_ROOT_MISSING: &str =
    "guard-write: --root expects a directory: rigger guard-write --root <dir>...";

/// FRAMING: the refusal of an empty `--root` directory.
pub const GUARD_WRITE_ROOT_EMPTY: &str = "guard-write: --root expects a non-empty directory";

/// FRAMING: the refusal of a relative `--root` directory, quoted in `{dir}` as its `Debug` form
/// escapes it.
pub const GUARD_WRITE_ROOT_RELATIVE: &str =
    "guard-write: --root expects an absolute directory, got \"{dir}\"";

/// FRAMING: the refusal of an argument other than `--root`, quoted in `{arg}` as its `Debug` form
/// escapes it.
pub const GUARD_WRITE_UNKNOWN_ARG: &str =
    "guard-write: unknown argument \"{arg}\": rigger guard-write --root <dir>...";

/// FRAMING: the refusal of an invocation that names no `--root`.
pub const GUARD_WRITE_NO_ROOT: &str = "guard-write: at least one --root is required";

/// FRAMING: the refusal when the PreToolUse payload cannot be read from stdin, the read's
/// `{error}` after it.
pub const GUARD_WRITE_PAYLOAD_UNREADABLE: &str =
    "failed to read the PreToolUse payload from stdin: {error}";

/// FRAMING: the instructions-in-force line after the domain's `INSTRUCTIONS_HEADING`: the
/// built-ins in force in `{builtin}`, and in `{extra}` the operator file count
/// ([`PRIME_OPERATOR_FILES`]) when an operator file is in force.
pub const PRIME_IN_FORCE: &str = " in force: {builtin}{extra} - see rigger instructions";

/// FRAMING: the `{extra}` of [`PRIME_IN_FORCE`], the count of operator files in `{operator}`.
pub const PRIME_OPERATOR_FILES: &str = " (+{operator} operator files)";

/// FRAMING: the decisions line of a project with no store.
pub const PRIME_NO_STORE: &str = "# Rigger: no decisions recorded yet (run `rigger run` to start).";

/// FRAMING: the header of the recent decisions a project's store holds.
pub const PRIME_DECISIONS_HEADER: &str = "# Rigger: recent decisions";

/// FRAMING: the line under the decisions header when no decision shows.
pub const PRIME_NONE_YET: &str = "(none yet)";

/// This crate's groups by surface, each named by its renderer and in render order: the
/// discipline docs' grep-guard message, the two guards' deny reasons, `rigger guard-write`'s
/// exit-2 prefix and refusals, and `rigger prime`'s fixed texts, the slot-filling
/// [`PRIME_OPERATOR_FILES`] included.
pub fn prompt_groups() -> &'static [Group] {
    rigger_domain::prompt_groups![
        "cli::docs_context" => [GREP_GUARD_MESSAGE],
        "guard::grep_guard_decision" => [GREP_GUARD_MESSAGE],
        "guard::guard_write_deny_outside_first_root" => [GUARD_WRITE_DENY],
        "guard::cmd_guard_write" => [
            GUARD_WRITE_STDERR_PREFIX,
            GUARD_WRITE_ROOT_MISSING,
            GUARD_WRITE_ROOT_EMPTY,
            GUARD_WRITE_ROOT_RELATIVE,
            GUARD_WRITE_UNKNOWN_ARG,
            GUARD_WRITE_NO_ROOT,
            GUARD_WRITE_PAYLOAD_UNREADABLE,
        ],
        "prime::cmd_prime" => [
            PRIME_IN_FORCE,
            PRIME_OPERATOR_FILES,
            PRIME_NO_STORE,
            PRIME_DECISIONS_HEADER,
            PRIME_NONE_YET,
        ],
    ]
}

/// Every crate's registry groups, the one enumeration: the domain's, the conductor's, the
/// driver's, the dash's, then this crate's, each list in its own order. The outer crates' lists
/// carry the lib's store predicate, so the `core` lane holds the domain's and this crate's alone.
pub fn all_groups() -> Vec<&'static Group> {
    [
        rigger_domain::prompt_surface::prompt_groups(),
        #[cfg(any(feature = "store", not(feature = "core")))]
        rigger_conductor::prompt_surface::prompt_groups(),
        #[cfg(any(feature = "store", not(feature = "core")))]
        rigger_driver::prompt_surface::prompt_groups(),
        #[cfg(any(feature = "store", not(feature = "core")))]
        rigger_dash::prompt_surface::prompt_groups(),
        prompt_groups(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
mod tests {
    use super::*;

    /// `all_groups()` holds every crate's `prompt_groups()`, each group once, in the order
    /// domain, conductor, driver, dash, then this crate's.
    #[test]
    fn all_groups_holds_every_crate_s_groups_in_crate_order() {
        let crate_lists = [
            rigger_domain::prompt_surface::prompt_groups(),
            rigger_conductor::prompt_surface::prompt_groups(),
            rigger_driver::prompt_surface::prompt_groups(),
            rigger_dash::prompt_surface::prompt_groups(),
            prompt_groups(),
        ];
        let expected: Vec<(&str, &str, &str)> = crate_lists
            .iter()
            .flat_map(|groups| groups.iter())
            .map(|g| (g.surface, g.name, g.text))
            .collect();
        let held: Vec<(&str, &str, &str)> = all_groups()
            .iter()
            .map(|g| (g.surface, g.name, g.text))
            .collect();
        assert_eq!(
            held, expected,
            "all_groups() is the five crates' groups in crate order"
        );
    }
}
