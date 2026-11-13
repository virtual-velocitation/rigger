//! THE WHOLE-LINE STDERR AUDIT: a run and the dash it starts share one stderr, so every line
//! either process says there goes through `rigger_process::whole_line`, which formats the line
//! first and hands it to the stream in one write. `eprintln!` and `eprint!` write each formatted
//! piece separately, so the other process's line lands inside theirs. A call to either macro in
//! production code under `src/` or a crate's `src/` fails the suite, naming file and line: library
//! code runs in whichever process calls it, a run's or a dash's among them, and the bin's shared
//! modules and the run's and the dash's verbs run in those processes. The verbs that run in a
//! process of their own ([`OWN_PROCESS_VERBS`]) are not audited.

use crate::common;
use common::repo::collect_rs_files;

use std::fs;
use std::path::Path;

use crate::source_audit;
use source_audit::{cfg_test_ranges, in_ranges, ref_punct, tokenize, Finding, RawKind};

/// The bin's verb files whose every stderr line is said by a verb that runs in a process of its
/// own, never by a run or the dash: `rigger validate`/`docs`, `critique`, `ground`/`graph build`,
/// `watch`, `guard-write`, `gate` and `setup`.
const OWN_PROCESS_VERBS: &[&str] = &[
    "src/cli/validate.rs",
    "src/cli/critique.rs",
    "src/cli/graph.rs",
    "src/cli/observe.rs",
    "src/cli/guard.rs",
    "src/cli/gate_verb.rs",
    "src/cli/setup.rs",
];

/// The macros that write a line to stderr a piece at a time.
const PIECEWISE: &[&str] = &["eprintln", "eprint"];

const BARE: &str = "stderr line written in pieces, not through whole_line";

/// Whether the file at `rel` is production source the audit reads: under `src/`, or under a
/// crate's own `src/`, and not one of the [`OWN_PROCESS_VERBS`].
fn audited(rel: &str) -> bool {
    let in_src = rel.starts_with("src/")
        || rel
            .strip_prefix("crates/")
            .and_then(|rest| rest.split_once('/'))
            .is_some_and(|(_, inside)| inside.starts_with("src/"));
    in_src && !OWN_PROCESS_VERBS.contains(&rel)
}

/// Every piecewise stderr write in the production code of `content`, the file at `rel`.
fn scan_file(rel: &str, content: &str) -> Vec<Finding> {
    let lines: Vec<&str> = content.lines().collect();
    let tests = cfg_test_ranges(&lines);
    let toks = tokenize(&content.chars().collect::<Vec<_>>());
    toks.iter()
        .enumerate()
        .filter(|(i, tok)| {
            tok.kind == RawKind::Ident
                && PIECEWISE.contains(&tok.text.as_str())
                && ref_punct(toks.get(i + 1), "!")
                && !in_ranges(tok.line - 1, &tests)
        })
        .map(|(_, tok)| Finding {
            file: rel.to_string(),
            line_no: tok.line,
            shape: BARE,
            line_text: lines.get(tok.line - 1).unwrap_or(&"").to_string(),
        })
        .collect()
}

/// Every piecewise stderr write in the audited files under `root`.
fn scan_tree(root: &Path) -> Vec<Finding> {
    let mut files = Vec::new();
    for top in ["src", "crates"] {
        collect_rs_files(&root.join(top), &mut files);
    }
    let mut findings = Vec::new();
    for path in &files {
        let rel = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        if !audited(&rel) {
            continue;
        }
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        findings.extend(scan_file(&rel, &content));
    }
    findings
}

#[test]
fn the_real_tree_says_every_shared_stderr_line_whole() {
    source_audit::assert_real_tree_clean(
        scan_tree,
        "the whole-line stderr audit",
        "stderr lines written in pieces",
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    use source_audit::write_file;

    /// The lines of the findings a fixture tree holding `content` at `rel` yields.
    fn flagged(rel: &str, content: &str) -> Vec<usize> {
        let root = tempfile::tempdir().unwrap();
        write_file(root.path(), rel, content);
        scan_tree(root.path())
            .into_iter()
            .map(|f| f.line_no)
            .collect()
    }

    // GIVEN a source file, WHEN the audit reads it, THEN it names each line that says a stderr
    // line in pieces in the production code it audits, and nothing else.
    rigger::test_cases! {
        an_eprintln_in_a_shared_module_is_caught: assert_eq!(
            flagged("src/cli/mod.rs", "fn f(e: &str) {\n    eprintln!(\"rigger: {e}\");\n}\n"),
            [2]
        );
        an_eprint_in_the_bin_entry_is_caught: assert_eq!(
            flagged("src/main.rs", "fn usage() {\n    eprint!(\"{USAGE}\");\n}\n"),
            [2]
        );
        an_eprintln_in_a_crate_is_caught: assert_eq!(
            flagged("crates/c/src/lib.rs", "fn f(l: &str) { eprintln!(\"{l}\") }\n"),
            [1]
        );
        an_eprintln_in_a_test_module_is_never_flagged: assert_eq!(
            flagged(
                "src/cli/run.rs",
                "#[cfg(test)]\nmod tests {\n    fn f() { eprintln!(\"skip\"); }\n}\n"
            ),
            Vec::<usize>::new()
        );
        an_eprintln_named_in_a_comment_or_a_literal_is_never_flagged: assert_eq!(
            flagged(
                "src/cli/run.rs",
                "/// a hardcoded `eprintln!` would split it\nfn f() -> &'static str { \"eprintln!\" }\n"
            ),
            Vec::<usize>::new()
        );
        an_eprintln_in_a_verb_of_its_own_process_is_never_flagged: assert_eq!(
            flagged("src/cli/validate.rs", "fn f(n: &str) { eprintln!(\"{n}\") }\n"),
            Vec::<usize>::new()
        );
        an_eprintln_in_a_crate_test_suite_is_never_flagged: assert_eq!(
            flagged("crates/c/tests/it.rs", "fn f() { eprintln!(\"x\") }\n"),
            Vec::<usize>::new()
        );
        a_line_said_through_whole_line_is_never_flagged: assert_eq!(
            flagged(
                "src/cli/run.rs",
                "fn f(e: &str) { rigger::whole_line::say(format_args!(\"rigger: {e}\")) }\n"
            ),
            Vec::<usize>::new()
        );
    }
}
