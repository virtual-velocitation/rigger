//! THE DASH OWNERSHIP AUDIT: a `rigger dash` a test or the run spawns is owned from its spawn, so
//! no path out of the spawning scope, a failed assertion's unwind or an early return included,
//! leaves it serving: a `std::process::Child` dropped bare leaves its process running. In every
//! `.rs` file under `src/`, `crates/` and `tests/`, a `.spawn()` whose statement builds a
//! `rigger dash` command (`"dash"` the first argument of `.args([..])` or `.arg(..)`) sits inside
//! the argument of an owning guard's constructor, `ReapedChild::new(..)` or `OwnedChild::new(..)`;
//! any other fails the suite, naming file and line.
//!
//! The audit reads one statement: a dash command built in one statement and spawned in another,
//! or spawned by a helper it is handed to, is not seen.

use crate::common;
use common::repo::collect_rs_files;

use std::fs;
use std::path::Path;

use crate::source_audit;
use source_audit::{qualifier_before, ref_punct, ref_token, tokenize, Finding, RawKind, RawTok};

/// The guards whose constructor takes a spawned child and ends it when the guard drops.
const OWNERS: &[&str] = &["ReapedChild", "OwnedChild"];

const BARE_DASH: &str = "dash spawned without an owning guard";

/// Whether `toks[k]` is the `"dash"` verb as the first argument of `.arg(..)`, `.args([..])` or
/// `.args(&[..])`.
fn is_dash_verb(toks: &[RawTok], k: usize) -> bool {
    let before = |back: usize| k.checked_sub(back).and_then(|at| toks.get(at));
    if !ref_token(toks.get(k), RawKind::Lit, "\"dash\"") {
        return false;
    }
    if ref_punct(before(1), "(") {
        return ref_token(before(2), RawKind::Ident, "arg");
    }
    let open = if ref_punct(before(2), "&") { 3 } else { 2 };
    ref_punct(before(1), "[")
        && ref_punct(before(open), "(")
        && ref_token(before(open + 1), RawKind::Ident, "args")
}

/// Whether `toks[k]`, a `(`, opens an owner's constructor call (`Owner::new(`).
fn opens_owner(toks: &[RawTok], k: usize) -> bool {
    k > 0
        && ref_token(toks.get(k - 1), RawKind::Ident, "new")
        && qualifier_before(toks, k - 1).is_some_and(|q| OWNERS.contains(&q.as_str()))
}

/// Walking back from `toks[at]`: the index the statement holding it starts at (just past the
/// `;`, `{` or `}` before it at its own depth), and whether a group it walks out of opens an
/// owner's constructor call.
fn statement_start(toks: &[RawTok], at: usize) -> (usize, bool) {
    let mut depth = 0usize;
    let mut owned = false;
    for k in (0..at).rev() {
        if toks[k].kind != RawKind::Punct {
            continue;
        }
        match (toks[k].text.as_str(), depth) {
            (";" | "{" | "}", 0) => return (k + 1, owned),
            ("(" | "[", 0) => owned |= opens_owner(toks, k),
            (")" | "]" | "}", _) => depth += 1,
            ("(" | "[" | "{", _) => depth -= 1,
            _ => {}
        }
    }
    (0, owned)
}

/// The index of the `;` or `}` that ends the statement holding `toks[at]`, walking forward out of
/// every group that encloses it.
fn statement_end(toks: &[RawTok], at: usize) -> usize {
    let mut depth = 0usize;
    for (k, tok) in toks.iter().enumerate().skip(at + 1) {
        if tok.kind != RawKind::Punct {
            continue;
        }
        match (tok.text.as_str(), depth) {
            (";" | "}", 0) => return k,
            (")" | "]", 0) => {}
            ("(" | "[" | "{", _) => depth += 1,
            (")" | "]" | "}", _) => depth -= 1,
            _ => {}
        }
    }
    toks.len()
}

/// Every dash spawned without an owning guard in `content`, the file at `rel`.
fn scan_file(rel: &str, content: &str) -> Vec<Finding> {
    let lines: Vec<&str> = content.lines().collect();
    let toks = tokenize(&content.chars().collect::<Vec<_>>());
    let mut findings = Vec::new();
    for (at, tok) in toks.iter().enumerate() {
        let is_spawn = ref_token(Some(tok), RawKind::Ident, "spawn")
            && at > 0
            && ref_punct(toks.get(at - 1), ".")
            && ref_punct(toks.get(at + 1), "(");
        if !is_spawn {
            continue;
        }
        let (start, owned) = statement_start(&toks, at);
        if owned || !(start..statement_end(&toks, at)).any(|k| is_dash_verb(&toks, k)) {
            continue;
        }
        findings.push(Finding {
            file: rel.to_string(),
            line_no: tok.line,
            shape: BARE_DASH,
            line_text: lines.get(tok.line - 1).unwrap_or(&"").to_string(),
        });
    }
    findings
}

/// Every dash spawned without an owning guard under `root`'s `src/`, `crates/` and `tests/`.
fn scan_tree(root: &Path) -> Vec<Finding> {
    let mut files = Vec::new();
    for top in ["src", "crates", "tests"] {
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
fn every_dash_the_tree_spawns_is_owned_from_its_spawn() {
    source_audit::assert_real_tree_clean(
        scan_tree,
        "the dash ownership audit",
        "dashes spawned without an owning guard",
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    use source_audit::write_file;

    /// The number of bare dash spawns a fixture tree holding `content` at `rel` yields.
    fn bare_spawns(rel: &str, content: &str) -> usize {
        let root = tempfile::tempdir().unwrap();
        write_file(root.path(), rel, content);
        scan_tree(root.path()).len()
    }

    fn caught(rel: &str, content: &str) {
        assert_eq!(bare_spawns(rel, content), 1, "{content}");
    }

    fn never_flagged(rel: &str, content: &str) {
        assert_eq!(bare_spawns(rel, content), 0, "{content}");
    }

    rigger::test_cases! {
        a_bare_spawn_of_an_args_dash_command_is_caught: caught(
            "tests/x.rs",
            "fn t() {\n    let d = rigger_courier()\n        .args([\"dash\", \"--port\", \"1\"])\n        \
             .spawn()\n        .unwrap();\n}\n",
        );
        a_bare_spawn_of_a_borrowed_args_dash_command_is_caught: caught(
            "tests/x.rs",
            "fn t() { let d = cmd.args(&[\"dash\"]).spawn().unwrap(); }\n",
        );
        a_bare_spawn_of_an_arg_dash_command_in_production_is_caught: caught(
            "src/cli/x.rs",
            "fn f() -> R { let d = command(exe).arg(\"dash\").spawn()?; Ok(d) }\n",
        );
        a_dash_spawn_handed_to_another_call_is_caught: caught(
            "crates/c/tests/x.rs",
            "fn t() { let d = Box::new(cmd.args([\"dash\"]).spawn().unwrap()); }\n",
        );
        a_dash_spawn_inside_a_reaped_child_is_never_flagged: never_flagged(
            "tests/x.rs",
            "fn t() {\n    let d = rigger::dash::ReapedChild::new(\n        \
             rigger_courier().args([\"dash\"]).spawn().expect(\"x\"),\n    );\n}\n",
        );
        a_dash_spawn_inside_an_owned_child_is_never_flagged: never_flagged(
            "tests/x.rs",
            "fn t() { let d = common::cli::OwnedChild::new(c.arg(\"dash\").spawn().unwrap()); }\n",
        );
        a_bare_spawn_of_another_verb_is_never_flagged: never_flagged(
            "tests/x.rs",
            "fn t() { let d = c.args([\"status\", \"dash\"]).env(\"V\", \"dash\").spawn().unwrap(); }\n",
        );
        a_dash_command_run_to_its_end_is_never_flagged: never_flagged(
            "tests/x.rs",
            "fn t() { let o = c.args([\"dash\", \"--export\", \"x\"]).output().unwrap(); }\n",
        );
        a_spawn_after_a_block_naming_the_dash_is_read_alone: never_flagged(
            "tests/x.rs",
            "fn t() {\n    if x {\n        c.args([\"dash\"]);\n    }\n    \
             let d = c.args([\"run\"]).spawn().unwrap();\n}\n",
        );
        a_thread_spawn_is_never_flagged: never_flagged(
            "tests/x.rs",
            "fn t() { std::thread::spawn(move || c.args([\"dash\"]).output()); }\n",
        );
    }
}
