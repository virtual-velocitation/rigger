//! THE PROCESS BOUNDARY SCAN (spec 125, criterion 9): a fact about a running process comes
//! through the process interface (`rigger_process::os_process`), whose Linux adapter is the one
//! production file that reads `/proc`. A string literal starting `/proc` in production code under
//! `src/` or a crate's `src/`, outside that adapter, fails the suite, naming file and line; a
//! `#[cfg(test)]` module and a comment line are not production code and are skipped.

use crate::common;
use crate::source_audit;
use source_audit::Finding;
use std::path::Path;

/// The process interface's Linux adapter, the one production file that reads `/proc`.
const LINUX_ADAPTER: &str = "crates/rigger-process/src/os_process/linux.rs";

/// The start of a string literal naming a path under `/proc`.
const PROC_LITERAL: &str = "\"/proc";

const SHAPE: &str = "a /proc read outside the process interface's Linux adapter";

/// Every `/proc` read in the production sources under `root`.
fn scan_tree(root: &Path) -> Vec<Finding> {
    source_audit::literal_outside(
        &common::repo::production_sources(root),
        LINUX_ADAPTER,
        PROC_LITERAL,
        SHAPE,
    )
}

#[test]
fn the_real_tree_reads_proc_only_in_the_linux_adapter() {
    source_audit::assert_real_tree_clean(
        scan_tree,
        "the process boundary scan",
        "/proc reads outside the process interface's Linux adapter",
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
    // outside the Linux adapter that reads /proc, and nothing else.
    rigger::test_cases! {
        each_proc_read_in_the_bin_is_named_by_file_and_line: assert_eq!(
            flagged(
                "src/cli/x.rs",
                "fn f() {\n    let _ = std::fs::read_dir(\"/proc\");\n    let _ = 1;\n    let _ = std::fs::read(\"/proc/self/stat\");\n}\n"
            ),
            [("src/cli/x.rs".to_string(), 2), ("src/cli/x.rs".to_string(), 4)]
        );
        a_proc_read_in_a_crate_is_named: assert_eq!(
            flagged("crates/c/src/lib.rs", "fn f(pid: u32) -> String {\n    format!(\"/proc/{pid}/cwd\")\n}\n"),
            [("crates/c/src/lib.rs".to_string(), 2)]
        );
        a_proc_read_in_the_linux_adapter_is_never_named: assert_eq!(
            flagged(LINUX_ADAPTER, "fn f() { let _ = std::fs::read_dir(\"/proc\"); }\n"),
            none()
        );
        a_proc_read_in_a_test_module_is_never_named: assert_eq!(
            flagged(
                "src/cli/x.rs",
                "#[cfg(test)]\nmod tests {\n    fn f() { let _ = std::fs::read_dir(\"/proc\"); }\n}\n"
            ),
            none()
        );
        a_proc_path_on_a_comment_line_is_never_named: assert_eq!(
            flagged("src/cli/x.rs", "/// reads `\"/proc\"` through the interface\nfn f() {}\n"),
            none()
        );
        a_proc_read_in_a_crate_test_suite_is_never_named: assert_eq!(
            flagged("crates/c/tests/it.rs", "fn f() { let _ = std::fs::read_dir(\"/proc\"); }\n"),
            none()
        );
        a_literal_naming_proc_past_its_start_is_never_named: assert_eq!(
            flagged("src/cli/x.rs", "fn f() -> &'static str { \"see /proc\" }\n"),
            none()
        );
    }
}
