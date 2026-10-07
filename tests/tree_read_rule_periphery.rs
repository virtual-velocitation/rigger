//! Periphery for spec 107, criterion 3 - THE TREE IS READ BY ONE RULE.
//!
//! `rigger::grounder::tree_bytes` and `rigger::grounder::in_walk_scope` are the read rule's public
//! surface: every reader of the tree outside the grounder crate reaches them through this path.
//! These tests hold the rule from that side - the exact bytes of a regular, readable file inside
//! the walk's scope under the root it is given, and none for any other path - and tie the one
//! `gw` path to the workflow definition the real `rigger init` binary writes.
//!
//! What this file OWNS: the rule as an outside caller sees it, each refusal beside a readable
//! in-scope control planted in the same tree. NOT OWNED: the rule's own corner cases (a FIFO, a
//! symlink, an ancestor that is a file), which the grounder crate's tests pin beside the code,
//! and the index-lag sample's use of the rule, which `tests/validate_advisories.rs` drives
//! through `rigger validate`.

mod common;

use common::cli::{rigger_file, run_rigger, temp_project};
use common::fixtures::{arm_read_fault, write_file};
use rigger::grounder::{in_walk_scope, tree_bytes};
use std::path::Path;

/// One row of the rule's answer: the identity prefix, the path relative to the root, whether the
/// walk's scope admits it, and the text whose bytes the rule hands for it.
type Answer<'a> = (&'a str, &'a str, bool, Option<&'a str>);

/// The rule answers every row of `expected`, in order, over `root`.
fn assert_rule(root: &Path, expected: &[Answer]) {
    let answered: Vec<(&str, &str, bool, Option<Vec<u8>>)> = expected
        .iter()
        .map(|&(prefix, path, _, _)| {
            (
                prefix,
                path,
                in_walk_scope(root, prefix, path),
                tree_bytes(root, prefix, path),
            )
        })
        .collect();
    let expected: Vec<(&str, &str, bool, Option<Vec<u8>>)> = expected
        .iter()
        .map(|&(prefix, path, scope, text)| {
            (prefix, path, scope, text.map(|t| t.as_bytes().to_vec()))
        })
        .collect();
    assert_eq!(answered, expected);
}

/// The rule answers `expected` over a fresh root holding exactly `files`.
fn assert_rule_over(files: &[(&str, &str)], expected: &[Answer]) {
    let dir = tempfile::tempdir().unwrap();
    for (rel, text) in files {
        write_file(&dir.path().join(rel), text.as_bytes());
    }
    assert_rule(dir.path(), expected);
}

rigger::test_cases! {
    /// A regular, readable file inside the walk's scope hands its exact bytes - the empty file's
    /// too - for the code and the design prefixes alike.
    a_regular_readable_file_in_scope_hands_its_exact_bytes: assert_rule_over(
        &[("src/only_here.rs", "fn only_here() {}\n"), ("empty.rs", "")],
        &[
            ("gc", "src/only_here.rs", true, Some("fn only_here() {}\n")),
            ("gd", "src/only_here.rs", true, Some("fn only_here() {}\n")),
            ("gc", "empty.rs", true, Some("")),
        ],
    );
    /// A path under a hidden directory hands none, though the file is there and readable; its
    /// twin under a visible directory hands its bytes.
    a_path_under_a_hidden_directory_hands_none: assert_rule_over(
        &[(".hidden/h.rs", "fn h() {}\n"), ("shown/h.rs", "fn h() {}\n")],
        &[
            ("gc", ".hidden/h.rs", false, None),
            ("gc", "shown/h.rs", true, Some("fn h() {}\n")),
        ],
    );
    /// A path the committed `.gitignore` names hands none, and the sibling it does not name
    /// hands its bytes.
    a_path_a_committed_gitignore_names_hands_none: assert_rule_over(
        &[
            (".gitignore", "ignored.rs\n"),
            ("ignored.rs", "fn ignored() {}\n"),
            ("kept.rs", "fn kept() {}\n"),
        ],
        &[
            ("gc", "ignored.rs", false, None),
            ("gc", "kept.rs", true, Some("fn kept() {}\n")),
        ],
    );
    /// With no `.gitignore` naming it, the same path hands its bytes: the refusal above is the
    /// ignore rule's and nothing else's.
    a_path_no_gitignore_names_hands_its_bytes: assert_rule_over(
        &[("ignored.rs", "fn ignored() {}\n")],
        &[("gc", "ignored.rs", true, Some("fn ignored() {}\n"))],
    );
}

/// A relative path is read from the root it is given: a name the working directory holds and the
/// root does not hands none, and once the root holds it too the rule hands the root's bytes.
#[test]
fn a_relative_path_is_read_from_the_given_root_and_not_the_working_directory() {
    let in_cwd = std::fs::read("Cargo.toml").expect("the working directory holds a manifest");
    let own = "# the root's own\n";
    assert_ne!(in_cwd, own.as_bytes());
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    assert_rule(root, &[("gc", "Cargo.toml", false, None)]);

    write_file(&root.join("Cargo.toml"), own.as_bytes());

    assert_rule(root, &[("gc", "Cargo.toml", true, Some(own))]);
}

/// A file of THE READ FAULT stays inside the scope and hands none: the same file hands its bytes
/// until the fault is armed, and its readable sibling after.
#[test]
fn a_file_of_the_read_fault_hands_none() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_file(&root.join("locked.rs"), b"fn locked() {}\n");
    write_file(&root.join("open.rs"), b"fn open() {}\n");

    assert_rule(root, &[("gc", "locked.rs", true, Some("fn locked() {}\n"))]);

    if !arm_read_fault(&root.join("locked.rs")) {
        return;
    }

    assert_rule(
        root,
        &[
            ("gc", "locked.rs", true, None),
            ("gc", "open.rs", true, Some("fn open() {}\n")),
        ],
    );
}

/// Given a project `rigger init` scaffolded, when the workflow definition it wrote is read under
/// `gw`, then the rule hands that file's exact bytes - and the same path under `gc` or `gd`,
/// which sits in a hidden directory no walk enters, hands none, as does any other path under
/// `gw`.
#[test]
fn the_workflow_definition_init_wrote_is_the_one_path_read_under_gw() {
    let dir = temp_project();
    let root = dir.path();
    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    let written = rigger_file(root, "workflow.yml");
    let workflow = written.strip_prefix(root).unwrap().to_str().unwrap();
    let text = std::fs::read_to_string(&written).expect("init wrote the workflow definition");
    assert!(!text.is_empty(), "init writes a non-empty definition");
    write_file(&root.join("other.yml"), b"stages: {}\n");

    assert_rule(
        root,
        &[
            ("gw", workflow, true, Some(text.as_str())),
            ("gc", workflow, false, None),
            ("gd", workflow, false, None),
            ("gw", "other.yml", false, None),
            ("gc", "other.yml", true, Some("stages: {}\n")),
        ],
    );
}
