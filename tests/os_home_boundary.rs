//! THE HOME BOUNDARY SCAN (spec 125, criterion 11): the user's home directory is read in one place,
//! `rigger_process::home_dir`, so its file is the one production file that names the `HOME`
//! variable. A `"HOME"` string literal in production code under `src/` or a crate's `src/`,
//! outside that file, fails the suite, naming file and line; a `#[cfg(test)]` module and a comment
//! line are not production code and are skipped.

use crate::common;
use crate::source_audit;
use source_audit::Finding;
use std::path::Path;

/// The home read, the one production file that names the `HOME` variable.
const HOME_READ: &str = "crates/rigger-process/src/home.rs";

/// The `HOME` variable's name as a string literal.
const HOME_LITERAL: &str = "\"HOME\"";

const SHAPE: &str = "a HOME read outside the home read";

/// Every `HOME` read in the production sources under `root`.
fn scan_tree(root: &Path) -> Vec<Finding> {
    source_audit::literal_outside(
        &common::repo::production_sources(root),
        HOME_READ,
        HOME_LITERAL,
        SHAPE,
    )
}

#[test]
fn the_real_tree_reads_home_only_in_the_home_read() {
    source_audit::assert_real_tree_clean(
        scan_tree,
        "the home boundary scan",
        "HOME reads outside the home read",
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
    // outside the home read that names `HOME`, and nothing else.
    rigger::test_cases! {
        each_home_read_in_the_bin_is_named_by_file_and_line: assert_eq!(
            flagged(
                "src/cli/x.rs",
                "fn f() {\n    let _ = std::env::var_os(\"HOME\");\n    let _ = 1;\n    const H: &str = \"HOME\";\n}\n"
            ),
            [("src/cli/x.rs".to_string(), 2), ("src/cli/x.rs".to_string(), 4)]
        );
        a_home_read_in_a_crate_is_named: assert_eq!(
            flagged("crates/c/src/lib.rs", "fn f() -> Option<String> {\n    std::env::var(\"HOME\").ok()\n}\n"),
            [("crates/c/src/lib.rs".to_string(), 2)]
        );
        a_home_read_in_the_home_read_is_never_named: assert_eq!(
            flagged(HOME_READ, "pub const HOME_ENV: &str = \"HOME\";\n"),
            none()
        );
        a_home_read_in_a_test_module_is_never_named: assert_eq!(
            flagged(
                "src/cli/x.rs",
                "#[cfg(test)]\nmod tests {\n    fn f() { let _ = std::env::var_os(\"HOME\"); }\n}\n"
            ),
            none()
        );
        a_home_name_on_a_comment_line_is_never_named: assert_eq!(
            flagged("src/cli/x.rs", "/// reads `\"HOME\"` through the home read\nfn f() {}\n"),
            none()
        );
        a_home_read_in_a_crate_test_suite_is_never_named: assert_eq!(
            flagged("crates/c/tests/it.rs", "fn f() { let _ = std::env::var_os(\"HOME\"); }\n"),
            none()
        );
        an_xdg_variable_ending_in_home_is_never_named: assert_eq!(
            flagged(
                "src/cli/x.rs",
                "fn f() {\n    let _ = std::env::var_os(\"XDG_CACHE_HOME\");\n    let _ = std::env::var_os(\"XDG_STATE_HOME\");\n}\n"
            ),
            none()
        );
    }
}
