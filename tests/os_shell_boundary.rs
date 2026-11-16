//! THE SHELL BOUNDARY SCAN (spec 125, criterion 10): a child process is built through the
//! process-spawn interface (`rigger_process::subprocess`), which owns the host's shell, so its file is
//! the one production file that spawns `sh` by name, and no production code names the external
//! lock command. In production code under `src/` or a crate's `src/`, outside that file, a
//! `"flock"` string literal, or `"sh"` as the first argument of `command`, `command_in` or
//! `Command::new`, fails the suite, naming file and line; a `#[cfg(test)]` module and a comment
//! line are not production code and are skipped. A spawn's first argument is read through the
//! source audits' lexer, so a call rustfmt splits across lines is named at its `"sh"` line, and a
//! string literal or a block comment holds no call.

use crate::common;
use crate::source_audit;
use source_audit::{Finding, RawKind, RawTok};
use std::path::Path;

/// The process-spawn interface, the one production file that spawns `sh` by name.
const SPAWN_INTERFACE: &str = "crates/rigger-process/src/subprocess.rs";

/// The external lock command's name as a string literal.
const LOCK_COMMAND: &str = "\"flock\"";

/// The shape a hit of [`LOCK_COMMAND`] reports.
const LOCK_COMMAND_SHAPE: &str =
    "the external lock command named outside the process-spawn interface";

/// The shape an `sh` spawn's hit reports.
const SH_SPAWN_SHAPE: &str = "an sh spawn outside the process-spawn interface";

/// Every hit in the production sources under `root`, in file and line order: each
/// [`LOCK_COMMAND`] and each [`sh_spawns`] one.
fn scan_tree(root: &Path) -> Vec<Finding> {
    let sources = common::repo::production_sources(root);
    let mut findings =
        source_audit::literal_outside(&sources, SPAWN_INTERFACE, LOCK_COMMAND, LOCK_COMMAND_SHAPE);
    findings.extend(sh_spawns(&sources));
    findings.sort_by(|a, b| (&a.file, a.line_no).cmp(&(&b.file, b.line_no)));
    findings
}

/// Each `"sh"` that is the first argument of `command`, `command_in` or `Command::new` in a file
/// of `sources` other than the spawn interface, outside that file's `#[cfg(test)]` modules, named at
/// the line the `"sh"` is on. Each file is read through [`source_audit::tokenize`], which yields
/// no token from a comment or from inside a string literal, and each call through
/// [`source_audit::called_at`].
fn sh_spawns(sources: &[(String, String)]) -> Vec<Finding> {
    sources
        .iter()
        .filter(|(rel, _)| rel != SPAWN_INTERFACE)
        .flat_map(|(rel, content)| {
            let lines: Vec<&str> = content.lines().collect();
            let tests = source_audit::cfg_test_ranges(&lines);
            let toks = source_audit::tokenize(&content.chars().collect::<Vec<_>>());
            (0..toks.len())
                .filter(|&at| {
                    source_audit::called_at(&toks, at).is_some_and(|name| spawns(name, &toks, at))
                        && source_audit::ref_token(toks.get(at + 2), RawKind::Lit, "\"sh\"")
                })
                .map(|at| toks[at + 2].line - 1)
                .filter(|&line| !source_audit::in_ranges(line, &tests))
                .map(|line| Finding {
                    file: rel.clone(),
                    line_no: line + 1,
                    shape: SH_SPAWN_SHAPE,
                    line_text: lines[line].to_string(),
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Whether `name`, called at `toks[at]`, spawns the program its first argument names: `command`
/// or `command_in` under any qualifier, or `new` qualified by `Command`.
fn spawns(name: &str, toks: &[RawTok], at: usize) -> bool {
    match name {
        "command" | "command_in" => true,
        "new" => source_audit::qualifier_before(toks, at).as_deref() == Some("Command"),
        _ => false,
    }
}

#[test]
fn the_real_tree_spawns_sh_and_names_no_lock_command_outside_the_spawn_interface() {
    source_audit::assert_real_tree_clean(
        scan_tree,
        "the shell boundary scan",
        "sh spawns or lock command names outside the process-spawn interface",
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    use source_audit::none;

    /// The `(file, line)` of each finding a fixture tree holding `content` at `rel` yields.
    fn flagged(rel: &str, content: &str) -> Vec<(String, usize)> {
        source_audit::flagged_by(scan_tree, rel, content)
    }

    // GIVEN a source file, WHEN the scan reads it, THEN it names each line of production code
    // outside the process-spawn interface that spawns `sh` by name or names the lock command, and
    // nothing else.
    rigger::test_cases! {
        each_needle_in_the_bin_is_named_by_file_and_line: assert_eq!(
            flagged(
                "src/cli/x.rs",
                "fn f(dir: &str) {\n    let _ = subprocess::command_in(\"flock\", dir);\n    let _ = 1;\n    let _ = subprocess::command(\"sh\");\n    let _ = subprocess::command_in(\"sh\", dir);\n    let _ = std::process::Command::new(\"sh\");\n}\n"
            ),
            [
                ("src/cli/x.rs".to_string(), 2),
                ("src/cli/x.rs".to_string(), 4),
                ("src/cli/x.rs".to_string(), 5),
                ("src/cli/x.rs".to_string(), 6)
            ]
        );
        an_sh_spawn_split_across_lines_is_named_at_its_sh_line: assert_eq!(
            flagged(
                "src/cli/x.rs",
                "fn f(dir: &str) {\n    let _ = subprocess::command(\n        \"sh\",\n    );\n    let _ = subprocess::command_in(\n        \"sh\",\n        dir,\n    );\n    let _ = std::process::Command::new(\n        \"sh\",\n    );\n}\n"
            ),
            [
                ("src/cli/x.rs".to_string(), 3),
                ("src/cli/x.rs".to_string(), 6),
                ("src/cli/x.rs".to_string(), 10)
            ]
        );
        an_sh_spawn_inside_a_string_or_a_block_comment_is_never_named: assert_eq!(
            flagged(
                "src/cli/x.rs",
                "fn f() {\n    let _ = r#\"command(\"sh\")\"#;\n    /* Command::new(\"sh\") */\n}\n"
            ),
            none()
        );
        a_needle_in_a_crate_is_named: assert_eq!(
            flagged("crates/c/src/lib.rs", "fn f() -> std::process::Command {\n    std::process::Command::new(\"sh\")\n}\n"),
            [("crates/c/src/lib.rs".to_string(), 2)]
        );
        a_needle_in_the_spawn_interface_is_never_named: assert_eq!(
            flagged(SPAWN_INTERFACE, "fn f() -> Command {\n    command(\"sh\")\n}\n"),
            none()
        );
        a_needle_in_another_crates_subprocess_file_is_named: assert_eq!(
            flagged("crates/c/src/subprocess.rs", "fn f() -> Command {\n    command(\"sh\")\n}\n"),
            [("crates/c/src/subprocess.rs".to_string(), 2)]
        );
        a_needle_in_a_test_module_is_never_named: assert_eq!(
            flagged(
                "src/cli/x.rs",
                "#[cfg(test)]\nmod tests {\n    fn f() { let _ = std::process::Command::new(\"sh\"); }\n}\n"
            ),
            none()
        );
        a_needle_on_a_comment_line_is_never_named: assert_eq!(
            flagged("src/cli/x.rs", "/// replaces `command_in(\"flock\", dir)` and `command(\"sh\")`\nfn f() {}\n"),
            none()
        );
        a_needle_in_a_crate_test_suite_is_never_named: assert_eq!(
            flagged("crates/c/tests/it.rs", "fn f() { let _ = std::process::Command::new(\"sh\"); }\n"),
            none()
        );
        a_near_miss_of_each_needle_is_never_named: assert_eq!(
            flagged(
                "src/cli/x.rs",
                "fn f(run: &str, dir: &Path) {\n    let _ = shell_command(run, dir);\n    let _ = shell_command(\"sh\", dir);\n    let _ = Other::new(\"sh\");\n    let _ = (command, \"sh\");\n    let _ = command(\"shx\");\n    let _ = command_in(\"bash\", \"\");\n    let _ = std::process::Command::new(\"cmd\");\n    let _ = \"flocked\";\n}\n"
            ),
            none()
        );
    }
}
