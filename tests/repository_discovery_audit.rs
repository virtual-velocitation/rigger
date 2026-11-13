//! THE REPOSITORY DISCOVERY AUDIT: "no repository" is what discovery found, never what a git
//! failure looked like. `rigger_worktree_git::discover::rev_parse` alone tells a start no
//! repository encloses from git refusing inside one, and every wrapper that resolves a
//! repository from it passes the refusal on as an error. Two shapes in production code under
//! `src/` and `crates/` fail the suite, naming file and line:
//!
//! 1. A RAW DISCOVERY FLAG: a string literal that is one of git's discovery flags, outside
//!    `crates/rigger-worktree-git/src/discover.rs`, so a caller runs discovery past the one place
//!    that tells the two apart.
//! 2. A SWALLOWED DISCOVERY FAILURE: a call to `rev_parse` or to one of those wrappers whose
//!    result, before any `?`, is chained into a method that drops the error (`.ok()`,
//!    `.unwrap_or_default()`, `.is_ok()`, ...), so a refusal reads as no repository.
//!
//! A site where no repository and a refusal legitimately share one outcome matches the error
//! by pattern instead (`let Ok(..) = .. else`, a `match`), with its reason stated beside it.

use crate::common;
use common::repo::collect_rs_files;

use std::fs;
use std::path::Path;

use crate::source_audit;
use source_audit::{cfg_test_ranges, in_ranges, ref_punct, tokenize, Finding, RawKind, RawTok};

/// The one file that runs repository discovery.
const DISCOVERY_FILE: &str = "crates/rigger-worktree-git/src/discover.rs";

/// The `git rev-parse` flags that discover a repository from a start directory.
const DISCOVERY_FLAGS: &[&str] = &[
    "--show-toplevel",
    "--git-common-dir",
    "--show-prefix",
    "--show-cdup",
    "--git-dir",
    "--absolute-git-dir",
    "--is-inside-work-tree",
];

/// `rev_parse` and every function that resolves a repository from it, passing its refusal on.
const DISCOVERY_CALLS: &[&str] = &[
    "rev_parse",
    "discover",
    "git_repo",
    "git_repo_at",
    "repository",
    "tree",
    "toplevel",
    "toplevel_of",
    "worktree_prefix",
    "project_root",
    "main_root",
    "owning_repo_root",
    "instruction_layer_root",
    "config_rigger_dir",
    "walk_stores_from",
    "walk_stores_in",
    "find_store_dir_from",
    "server_store_location",
    "project_identity",
    "project_identity_at",
    "legacy_identity_at",
    "has_tracked_project_id",
    "project_code_store",
    "git_hooks_dir",
];

/// The `Result` methods that drop the error.
const DROPPING_METHODS: &[&str] = &[
    "ok",
    "is_ok",
    "is_ok_and",
    "unwrap_or",
    "unwrap_or_default",
    "unwrap_or_else",
    "map_or",
    "map_or_else",
];

const RAW_FLAG: &str = "raw discovery flag outside discover.rs";
const SWALLOWED: &str = "discovery failure dropped into no repository";

/// The text between a string literal token's quotes (`"x"`, `r"x"`, `r#"x"#`, `b"x"`).
fn literal_body(text: &str) -> &str {
    text.trim_start_matches(['b', 'r', '#'])
        .trim_end_matches('#')
        .trim_matches('"')
}

/// The index just past the group `toks[open]` opens, counting only `(` and `)`.
fn past_group(toks: &[RawTok], open: usize) -> usize {
    let mut depth = 0usize;
    for (k, tok) in toks.iter().enumerate().skip(open) {
        if tok.kind == RawKind::Punct && tok.text == "(" {
            depth += 1;
        } else if tok.kind == RawKind::Punct && tok.text == ")" {
            depth -= 1;
            if depth == 0 {
                return k + 1;
            }
        }
    }
    toks.len()
}

/// Whether `toks[i]` calls a discovery function whose result, before any `?`, is chained into a
/// method that drops the error.
fn swallows_at(toks: &[RawTok], i: usize) -> bool {
    let tok = &toks[i];
    let is_call = tok.kind == RawKind::Ident
        && DISCOVERY_CALLS.contains(&tok.text.as_str())
        && ref_punct(toks.get(i + 1), "(");
    let is_definition = i > 0 && toks[i - 1].kind == RawKind::Keyword && toks[i - 1].text == "fn";
    if !is_call || is_definition {
        return false;
    }
    let mut j = past_group(toks, i + 1);
    while ref_punct(toks.get(j), ".") {
        let Some(method) = toks.get(j + 1).filter(|t| t.kind == RawKind::Ident) else {
            return false;
        };
        if DROPPING_METHODS.contains(&method.text.as_str()) {
            return true;
        }
        if !ref_punct(toks.get(j + 2), "(") {
            return false;
        }
        j = past_group(toks, j + 2);
    }
    false
}

/// Every audited shape in the production code of `content`, the file at `rel`.
fn scan_file(rel: &str, content: &str) -> Vec<Finding> {
    let lines: Vec<&str> = content.lines().collect();
    let tests = cfg_test_ranges(&lines);
    let toks = tokenize(&content.chars().collect::<Vec<_>>());
    let mut findings = Vec::new();
    for (i, tok) in toks.iter().enumerate() {
        if in_ranges(tok.line - 1, &tests) {
            continue;
        }
        let raw_flag = rel != DISCOVERY_FILE
            && tok.kind == RawKind::Lit
            && DISCOVERY_FLAGS.contains(&literal_body(&tok.text));
        let shape = if raw_flag {
            RAW_FLAG
        } else if swallows_at(&toks, i) {
            SWALLOWED
        } else {
            continue;
        };
        findings.push(Finding {
            file: rel.to_string(),
            line_no: tok.line,
            shape,
            line_text: lines.get(tok.line - 1).unwrap_or(&"").to_string(),
        });
    }
    findings
}

/// Every audited shape under `root`'s `src/` and `crates/`.
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
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        findings.extend(scan_file(&rel, &content));
    }
    findings
}

#[test]
fn the_real_tree_reads_no_repository_only_from_discovery() {
    source_audit::assert_real_tree_clean(
        scan_tree,
        "the repository discovery audit",
        "discovery sites that read a git failure as no repository",
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    use source_audit::write_file;

    /// The shapes a fixture tree holding `content` at `rel` yields.
    fn shapes(rel: &str, content: &str) -> Vec<&'static str> {
        let root = tempfile::tempdir().unwrap();
        write_file(root.path(), rel, content);
        scan_tree(root.path())
            .into_iter()
            .map(|f| f.shape)
            .collect()
    }

    fn caught_as(rel: &str, content: &str, shape: &'static str) {
        assert_eq!(shapes(rel, content), vec![shape], "{content}");
    }

    fn never_flagged(rel: &str, content: &str) {
        assert_eq!(shapes(rel, content), Vec::<&str>::new(), "{content}");
    }

    rigger::test_cases! {
        a_raw_flag_in_an_argument_list_is_caught: caught_as(
            "src/cli/x.rs",
            "fn f(r: &Path) { git_in(r).args([\"rev-parse\", \"--show-toplevel\"]); }\n",
            RAW_FLAG,
        );
        a_raw_flag_in_another_crate_is_caught: caught_as(
            "crates/c/src/lib.rs",
            "fn f(c: &mut Command) { c.arg(\"--git-common-dir\"); }\n",
            RAW_FLAG,
        );
        a_raw_flag_in_discover_rs_is_never_flagged: never_flagged(
            DISCOVERY_FILE,
            "fn flag() -> &'static str { \"--show-prefix\" }\n",
        );
        a_raw_flag_in_a_test_module_is_never_flagged: never_flagged(
            "src/cli/x.rs",
            "#[cfg(test)]\nmod tests {\n    fn f() { git(&[\"rev-parse\", \"--git-dir\"]); }\n}\n",
        );
        a_raw_flag_named_in_a_comment_is_never_flagged: never_flagged(
            "src/cli/x.rs",
            "// asks `git rev-parse \"--show-toplevel\"` through discovery\nfn f() {}\n",
        );
        an_unwrap_or_default_on_a_discovery_call_is_caught: caught_as(
            "src/cli/x.rs",
            "fn f(r: &Path) -> String { git_repo_at(r).unwrap_or_default() }\n",
            SWALLOWED,
        );
        an_ok_after_a_mapped_discovery_call_is_caught: caught_as(
            "src/cli/x.rs",
            "fn f(r: &Path) -> Option<String> {\n    rev_parse(r, Location::Toplevel)\n        \
             .map(|o| o)\n        .ok()\n        .flatten()\n}\n",
            SWALLOWED,
        );
        an_is_ok_on_a_discovery_call_is_caught: caught_as(
            "crates/c/src/lib.rs",
            "fn f(site: &Site) -> bool { site.main_root().is_ok() }\n",
            SWALLOWED,
        );
        an_option_dropped_after_the_question_mark_is_never_flagged: never_flagged(
            "src/cli/x.rs",
            "fn f(r: &Path) -> Result<PathBuf, String> {\n    \
             Ok(rev_parse(r, Location::Prefix)?.map(PathBuf::from).unwrap_or_default())\n}\n",
        );
        an_error_matched_by_pattern_is_never_flagged: never_flagged(
            "src/cli/x.rs",
            "fn f(r: &Path) {\n    let Ok(Some(c)) = rev_parse(r, Location::CommonDir) else {\n        \
             return;\n    };\n}\n",
        );
        a_wrapper_definition_is_never_flagged: never_flagged(
            "src/cli/x.rs",
            "fn git_repo_at(r: &Path) -> Result<String, String> {\n    \
             rev_parse(r, Location::Toplevel).map(Option::unwrap_or_default)\n}\n",
        );
        a_dropping_method_on_another_call_is_never_flagged: never_flagged(
            "src/cli/x.rs",
            "fn f(r: &Path) -> Option<Output> { git_in(r).output().ok() }\n",
        );
    }
}
