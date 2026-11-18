//! THE GIT COMMAND AUDIT: every git command production code runs is built by
//! `rigger_process::subprocess::git_in`, the one place that refuses an empty repository path. An
//! empty path handed to `git -C` names the process's working directory, so a caller with no
//! repository would otherwise run git against whatever repository its process sits in. The `git`
//! program literal in production code under `src/` or a crate's `src/`, anywhere but inside
//! `git_in`, fails the suite, naming file and line; a `#[cfg(test)]` module and a comment line
//! are not production code and are skipped.

use crate::common;
use crate::source_audit;
use source_audit::{cfg_test_ranges, enclosing_fn_line, fn_sig_lines, Finding};
use std::path::Path;

/// The file that holds the one git command builder.
const CHOKE_POINT_FILE: &str = "crates/rigger-process/src/subprocess.rs";

/// The signature of the one git command builder.
const CHOKE_POINT_FN: &str = "fn git_in(";

/// The `git` program, as a string literal.
const GIT_PROGRAM: &str = "\"git\"";

/// No file: every production file is scanned, the choke point's own file included.
const NO_HOME: &str = "";

const SHAPE: &str = "a git command built past git_in, which refuses an empty repository path";

/// Whether line `line_no` (1-based) of `text` lies inside the git command builder.
fn within_choke_point(text: &str, line_no: usize) -> bool {
    let lines: Vec<&str> = text.lines().collect();
    let sigs = fn_sig_lines(&lines, &cfg_test_ranges(&lines));
    enclosing_fn_line(&lines, &sigs, line_no - 1)
        .is_some_and(|sig| lines[sig].contains(CHOKE_POINT_FN))
}

/// Every git command production code under `root` builds past the choke point.
fn scan_tree(root: &Path) -> Vec<Finding> {
    let sources = common::repo::production_sources(root);
    source_audit::literal_outside(&sources, NO_HOME, GIT_PROGRAM, SHAPE)
        .into_iter()
        .filter(|finding| {
            !sources.iter().any(|(rel, text)| {
                *rel == finding.file
                    && rel == CHOKE_POINT_FILE
                    && within_choke_point(text, finding.line_no)
            })
        })
        .collect()
}

#[test]
fn the_real_tree_builds_every_git_command_through_git_in() {
    source_audit::assert_real_tree_clean(
        scan_tree,
        "the git command audit",
        "git commands built past git_in",
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

    // GIVEN a source file, WHEN the audit reads it, THEN it names each line of production code
    // that builds a git command anywhere but inside `git_in`, and nothing else.
    rigger::test_cases! {
        a_git_command_built_in_the_bin_is_named: assert_eq!(
            flagged(
                "src/cli/x.rs",
                "fn f(repo: &str) {\n    let _ = Command::new(\"git\").arg(\"-C\").arg(repo);\n}\n"
            ),
            [("src/cli/x.rs".to_string(), 2)]
        );
        a_git_command_built_in_a_crate_is_named: assert_eq!(
            flagged(
                "crates/c/src/lib.rs",
                "fn f(repo: &Path) {\n    let _ = command(\"git\").current_dir(repo);\n}\n"
            ),
            [("crates/c/src/lib.rs".to_string(), 2)]
        );
        a_second_builder_beside_git_in_is_named: assert_eq!(
            flagged(
                CHOKE_POINT_FILE,
                "pub fn git_in(dir: &OsStr) -> Command {\n    command(\"git\")\n}\n\
                 pub fn git_here() -> Command {\n    command(\"git\")\n}\n"
            ),
            [(CHOKE_POINT_FILE.to_string(), 5)]
        );
        the_builder_inside_git_in_is_never_named: assert_eq!(
            flagged(
                CHOKE_POINT_FILE,
                "pub fn git_in(dir: &OsStr) -> Command {\n    command(\"git\")\n}\n"
            ),
            none()
        );
        a_git_command_in_a_test_module_is_never_named: assert_eq!(
            flagged(
                "src/cli/x.rs",
                "#[cfg(test)]\nmod tests {\n    fn f() { let _ = Command::new(\"git\"); }\n}\n"
            ),
            none()
        );
        a_git_command_on_a_comment_line_is_never_named: assert_eq!(
            flagged("src/cli/x.rs", "/// never `Command::new(\"git\")` here\nfn f() {}\n"),
            none()
        );
        a_git_command_in_a_crate_test_suite_is_never_named: assert_eq!(
            flagged("crates/c/tests/it.rs", "fn f() { let _ = Command::new(\"git\"); }\n"),
            none()
        );
        a_message_naming_a_git_verb_is_never_named: assert_eq!(
            flagged("src/cli/x.rs", "fn f() -> String { format!(\"git worktree add failed\") }\n"),
            none()
        );
    }
}
