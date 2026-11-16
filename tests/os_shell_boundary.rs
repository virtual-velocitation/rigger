//! THE SHELL BOUNDARY SCAN (spec 125, criterion 10): a child process is built through the
//! process-spawn port (`rigger_process::subprocess`), which owns the host's shell, so its file is
//! the one production file that spawns `sh` by name, and no production code names the external
//! lock command. In production code under `src/` or a crate's `src/`, outside that file, a
//! `"flock"` string literal, or `"sh"` as the first argument of `command`, `command_in` or
//! `Command::new`, fails the suite, naming file and line; a `#[cfg(test)]` module and a comment
//! line are not production code and are skipped.

use crate::common;
use crate::source_audit;
use source_audit::Finding;
use std::path::Path;

/// The process-spawn port, the one production file that spawns `sh` by name.
const SPAWN_PORT: &str = "crates/rigger-process/src/subprocess.rs";

/// Each literal the scan finds, with the shape it reports.
const NEEDLES: [(&str, &str); 4] = [
    (
        "\"flock\"",
        "the external lock command named outside the process-spawn port",
    ),
    (
        "command(\"sh\"",
        "an sh spawn outside the process-spawn port",
    ),
    (
        "command_in(\"sh\"",
        "an sh spawn outside the process-spawn port",
    ),
    (
        "Command::new(\"sh\"",
        "an sh spawn outside the process-spawn port",
    ),
];

/// Every needle's hit in the production sources under `root`, in file and line order.
fn scan_tree(root: &Path) -> Vec<Finding> {
    let sources = common::repo::production_sources(root);
    let mut findings: Vec<Finding> = NEEDLES
        .iter()
        .flat_map(|&(literal, shape)| {
            source_audit::literal_outside(&sources, SPAWN_PORT, literal, shape)
        })
        .collect();
    findings.sort_by(|a, b| (&a.file, a.line_no).cmp(&(&b.file, b.line_no)));
    findings
}

#[test]
fn the_real_tree_spawns_sh_and_names_no_lock_command_outside_the_spawn_port() {
    source_audit::assert_real_tree_clean(
        scan_tree,
        "the shell boundary scan",
        "sh spawns or lock command names outside the process-spawn port",
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
    // outside the process-spawn port that spawns `sh` by name or names the lock command, and
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
        a_needle_in_the_spawn_port_is_never_named: assert_eq!(
            flagged(SPAWN_PORT, "fn f() -> Command {\n    command(\"sh\")\n}\n"),
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
