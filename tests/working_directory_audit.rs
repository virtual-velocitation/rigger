//! THE WORKING DIRECTORY AUDIT: a command finds its store, its configuration and its repository
//! root through the one resolver, never from the directory it happens to run in. The resolver
//! reads the working directory once, in `Site::of`, and resolves every root from that site; the
//! writer resolver alone spells the root's store relative to the working directory, at the root
//! itself, and discovery alone takes an empty start as the working directory, as git does. Two
//! shapes in production code under `src/` and `crates/` fail the suite, naming file and line:
//!
//! 1. A WORKING DIRECTORY READ: a call to `current_dir` as a free function (`std::env::current_dir()`)
//!    or to the `cwd()` helper, so a command anchors at the directory it runs in.
//! 2. A WORKING-DIRECTORY-RELATIVE PATH: the literal `"."` or `".rigger"`, or the `RIGGER_DIR`
//!    constant, passed whole as an argument to a function (not a method, so `root.join(RIGGER_DIR)`
//!    stays anchored at `root`), so a store, a configuration or a root is opened relative to the
//!    directory the command runs in.
//!
//! The allow-list holds the resolver alone, each entry with its reason. A command builder's
//! `.current_dir(root)` sets a child's directory and reads nothing, so it is never flagged.

use crate::common;
use common::repo::collect_rs_files;

use std::fs;
use std::path::Path;

use crate::source_audit;
use source_audit::{
    cfg_test_ranges, enclosing_fn_line, fn_sig_lines, in_ranges, is_definition_site, ref_punct,
    tokenize, Finding, RawKind, RawTok,
};

/// The functions that may read the working directory or spell a path relative to it: the
/// resolver itself, as `(file, function, reason)`.
const RESOLVER: &[(&str, &str, &str)] = &[
    (
        "src/cli/mod.rs",
        "of",
        "Site::of is the one reader of the working directory every root is resolved from",
    ),
    (
        "src/cli/mod.rs",
        "store_to_write_with",
        "the writer resolver, run at the root it resolved, spells the root's store relative to \
         the working directory, so the files of a root whose path is not UTF-8 are still named; \
         the location it returns carries that root",
    ),
    (
        "crates/rigger-worktree-git/src/discover.rs",
        "as_git_takes",
        "discovery's own contract takes an empty start as the working directory, as `git -C \"\"` \
         does, pinned beside git by its parity rows; no rigger command hands it one, since \
         Site::of resolves an empty directory itself",
    ),
];

/// The literals a working-directory-relative path is spelled with.
const RELATIVE_LITERALS: &[&str] = &["\".\"", "\".rigger\""];

/// The constant naming the store directory.
const STORE_DIR: &str = "RIGGER_DIR";

const CWD_READ: &str = "working directory read outside the resolver";
const CWD_PATH: &str = "working-directory-relative path outside the resolver";

/// Whether `toks[i]` calls the working directory: `current_dir` called as a path (`env::current_dir()`,
/// never the builder method `.current_dir(dir)`), or the `cwd()` helper called.
fn reads_cwd_at(toks: &[RawTok], i: usize) -> bool {
    let tok = &toks[i];
    if tok.kind != RawKind::Ident || !ref_punct(toks.get(i + 1), "(") || is_definition_site(toks, i)
    {
        return false;
    }
    let method = i > 0 && ref_punct(toks.get(i - 1), ".");
    match tok.text.as_str() {
        "current_dir" => !method,
        "cwd" => !method && ref_punct(toks.get(i + 2), ")"),
        _ => false,
    }
}

/// The index of the bracket that opens the group `toks[i]` lies directly in, if any.
fn open_of(toks: &[RawTok], i: usize) -> Option<usize> {
    let mut depth = 0usize;
    for k in (0..i).rev() {
        let tok = &toks[k];
        if tok.kind != RawKind::Punct {
            continue;
        }
        match tok.text.as_str() {
            ")" | "]" | "}" => depth += 1,
            "(" | "[" | "{" if depth == 0 => return Some(k),
            "(" | "[" | "{" => depth -= 1,
            _ => {}
        }
    }
    None
}

/// Whether `toks[i]` is a working-directory-relative path passed whole as an argument to a
/// function call that is not a method call or a macro.
fn relative_path_at(toks: &[RawTok], i: usize) -> bool {
    let tok = &toks[i];
    let relative = (tok.kind == RawKind::Lit && RELATIVE_LITERALS.contains(&tok.text.as_str()))
        || (tok.kind == RawKind::Ident && tok.text == STORE_DIR);
    let whole = (ref_punct(toks.get(i.wrapping_sub(1)), "(")
        || ref_punct(toks.get(i.wrapping_sub(1)), ","))
        && (ref_punct(toks.get(i + 1), ")") || ref_punct(toks.get(i + 1), ","));
    if !relative || !whole {
        return false;
    }
    let Some(open) = open_of(toks, i) else {
        return false;
    };
    if !ref_punct(toks.get(open), "(") || open == 0 {
        return false;
    }
    let callee = &toks[open - 1];
    callee.kind == RawKind::Ident && !(open >= 2 && ref_punct(toks.get(open - 2), "."))
}

/// The name of the function whose signature is `line`.
fn fn_name(line: &str) -> &str {
    let after = line.split("fn ").nth(1).unwrap_or("");
    after
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .next()
        .unwrap_or("")
}

/// Every audited shape in the production code of `content`, the file at `rel`.
fn scan_file(rel: &str, content: &str) -> Vec<Finding> {
    let lines: Vec<&str> = content.lines().collect();
    let tests = cfg_test_ranges(&lines);
    let sigs = fn_sig_lines(&lines, &tests);
    let toks = tokenize(&content.chars().collect::<Vec<_>>());
    let mut findings = Vec::new();
    for (i, tok) in toks.iter().enumerate() {
        let at = tok.line - 1;
        if in_ranges(at, &tests) {
            continue;
        }
        let shape = if reads_cwd_at(&toks, i) {
            CWD_READ
        } else if relative_path_at(&toks, i) {
            CWD_PATH
        } else {
            continue;
        };
        let within = enclosing_fn_line(&lines, &sigs, at).map(|s| fn_name(lines[s]));
        if RESOLVER
            .iter()
            .any(|(file, function, _)| *file == rel && Some(*function) == within)
        {
            continue;
        }
        findings.push(Finding {
            file: rel.to_string(),
            line_no: tok.line,
            shape,
            line_text: lines.get(at).unwrap_or(&"").to_string(),
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
        a_current_dir_read_is_caught: caught_as(
            "src/cli/x.rs",
            "fn f() -> PathBuf { std::env::current_dir().unwrap() }\n",
            CWD_READ,
        );
        a_cwd_helper_call_is_caught: caught_as(
            "crates/c/src/lib.rs",
            "fn f() -> PathBuf { let cwd = cwd(); cwd }\n",
            CWD_READ,
        );
        a_dot_passed_to_a_loader_is_caught: caught_as(
            "src/cli/x.rs",
            "fn f() -> Res { let cfg = config_store::load(\".\")?; Ok(()) }\n",
            CWD_PATH,
        );
        a_dot_path_is_caught: caught_as(
            "src/cli/x.rs",
            "fn f() -> Res { write_docs(Path::new(\".\"))?; Ok(()) }\n",
            CWD_PATH,
        );
        a_dot_among_other_arguments_is_caught: caught_as(
            "src/cli/x.rs",
            "fn f(h: &[u8]) -> Res { definition_hash(\".\", h)?; Ok(()) }\n",
            CWD_PATH,
        );
        a_store_dir_path_is_caught: caught_as(
            "src/cli/x.rs",
            "fn f() -> bool { Path::new(RIGGER_DIR).is_dir() }\n",
            CWD_PATH,
        );
        a_store_dir_made_in_place_is_caught: caught_as(
            "src/cli/x.rs",
            "fn f() -> Res { std::fs::create_dir_all(RIGGER_DIR)?; Ok(()) }\n",
            CWD_PATH,
        );
        a_store_literal_path_is_caught: caught_as(
            "crates/c/src/lib.rs",
            "fn f() -> PathBuf { PathBuf::from(\".rigger\") }\n",
            CWD_PATH,
        );
        a_store_dir_joined_onto_a_root_is_never_flagged: never_flagged(
            "src/cli/x.rs",
            "fn f(root: &Path) -> PathBuf { root.join(RIGGER_DIR) }\n",
        );
        a_child_directory_set_on_a_command_is_never_flagged: never_flagged(
            "src/cli/x.rs",
            "fn f(root: &Path) { Command::new(\"git\").current_dir(root).status(); }\n",
        );
        a_command_directory_read_back_is_never_flagged: never_flagged(
            "crates/c/src/lib.rs",
            "fn f(cmd: &Command) -> Option<&Path> { cmd.get_current_dir() }\n",
        );
        a_dot_in_an_argument_array_is_never_flagged: never_flagged(
            "src/cli/x.rs",
            "fn f(root: &Path) { git(root, &[\"add\", \".\"]); }\n",
        );
        a_dot_in_a_macro_is_never_flagged: never_flagged(
            "src/cli/x.rs",
            "fn f() -> String { format!(\"{}\", \".\") }\n",
        );
        a_dot_inside_a_literal_is_never_flagged: never_flagged(
            "src/cli/x.rs",
            "fn f() -> &'static str { \"run it from . instead\" }\n",
        );
        a_read_in_a_test_module_is_never_flagged: never_flagged(
            "src/cli/x.rs",
            "#[cfg(test)]\nmod tests {\n    fn f() { let _ = std::env::current_dir(); }\n}\n",
        );
        a_helper_definition_is_never_flagged: never_flagged(
            "src/cli/x.rs",
            "fn cwd(at: &Path) -> PathBuf { at.to_path_buf() }\n",
        );
        the_working_directory_read_in_the_resolver_is_never_flagged: never_flagged(
            "src/cli/mod.rs",
            "impl Site {\n    fn of(dir: &Path) -> PathBuf {\n        \
             std::env::current_dir().unwrap_or_else(|_| PathBuf::from(\".\"))\n    }\n}\n",
        );
        the_store_spelled_at_the_root_by_the_writer_resolver_is_never_flagged: never_flagged(
            "src/cli/mod.rs",
            "fn store_to_write_with() -> PathBuf {\n    let dir = PathBuf::from(RIGGER_DIR);\n    \
             std::fs::create_dir_all(&dir).unwrap();\n    dir\n}\n",
        );
        the_empty_start_discovery_takes_as_git_does_is_never_flagged: never_flagged(
            "crates/rigger-worktree-git/src/discover.rs",
            "fn as_git_takes(start: &Path) -> &Path {\n    \
             if start.as_os_str().is_empty() { Path::new(\".\") } else { start }\n}\n",
        );
        a_store_spelled_by_a_writer_outside_the_resolver_is_caught: caught_as(
            "src/cli/mod.rs",
            "fn store_to_write_in() -> PathBuf { PathBuf::from(RIGGER_DIR) }\n",
            CWD_PATH,
        );
        a_resolver_name_in_another_file_is_caught: caught_as(
            "src/cli/x.rs",
            "fn of() -> PathBuf { std::env::current_dir().unwrap() }\n",
            CWD_READ,
        );
        /// The real `src/` and `crates/` trees, resolved from `CARGO_MANIFEST_DIR`, read the
        /// working directory and spell a path relative to it only in the resolver.
        the_real_tree_reads_the_working_directory_only_in_the_resolver:
            source_audit::assert_real_tree_clean(
                scan_tree,
                "the working directory audit",
                "working-directory read(s) or relative path(s) outside the resolver",
            );
    }
}
