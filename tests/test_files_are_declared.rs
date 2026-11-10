//! With `autotests = false`, a `tests/*.rs` file nobody declares is never compiled: its tests
//! never run while every gate stays green. Every top-level test file is exactly one of a module
//! of the consolidated `all` executable (`mod <name>;` in `tests/all.rs`, cfg-gated ones
//! included) or the `path` of a `[[test]]` entry in the root `Cargo.toml`.

use std::collections::BTreeSet;

use crate::common::repo::{repo_root, repo_text};

/// The modules `tests/all.rs` declares that resolve to a file of their own; a `#[path]` module
/// is a helper under `tests/common`, not a test file.
fn modules_of_all() -> BTreeSet<String> {
    let mut modules = BTreeSet::new();
    let mut path_attribute = false;
    for line in repo_text("tests/all.rs").lines() {
        let line = line.trim();
        if line.starts_with("#[path") {
            path_attribute = true;
            continue;
        }
        if let Some(name) = line
            .strip_prefix("mod ")
            .and_then(|rest| rest.strip_suffix(';'))
        {
            if !path_attribute && name != "common" {
                modules.insert(name.to_string());
            }
        }
        if !line.starts_with("#[") && !line.is_empty() {
            path_attribute = false;
        }
    }
    modules
}

/// The files the root `Cargo.toml` declares as `[[test]]` executables, other than `all`.
fn separate_executables() -> BTreeSet<String> {
    let mut files = BTreeSet::new();
    let mut in_test = false;
    for line in repo_text("Cargo.toml").lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_test = line == "[[test]]";
        } else if let Some(path) = in_test
            .then(|| line.strip_prefix("path = \""))
            .flatten()
            .and_then(|rest| rest.strip_suffix('"'))
            .and_then(|path| path.strip_prefix("tests/"))
            .and_then(|path| path.strip_suffix(".rs"))
        {
            if path != "all" {
                files.insert(path.to_string());
            }
        }
    }
    files
}

#[test]
fn every_top_level_test_file_is_declared_exactly_once() {
    let on_disk: BTreeSet<String> = std::fs::read_dir(repo_root().join("tests"))
        .expect("tests/ is readable")
        .map(|entry| entry.expect("a readable entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        .filter_map(|path| path.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .filter(|name| name != "all")
        .collect();
    let modules = modules_of_all();
    let separate = separate_executables();
    let list = |names: Vec<&String>| {
        names
            .iter()
            .map(|n| format!("tests/{n}.rs"))
            .collect::<Vec<_>>()
    };
    let undeclared = list(
        on_disk
            .iter()
            .filter(|n| !modules.contains(*n) && !separate.contains(*n))
            .collect(),
    );
    let both = list(modules.intersection(&separate).collect());
    let missing = list(
        modules
            .iter()
            .chain(separate.iter())
            .filter(|n| {
                !on_disk.contains(*n) && !repo_root().join(format!("tests/{n}/mod.rs")).exists()
            })
            .collect(),
    );
    assert!(
        undeclared.is_empty() && both.is_empty() && missing.is_empty(),
        "a test file is declared exactly once, as a `mod` in tests/all.rs or a `[[test]]` entry \
         in Cargo.toml; with `autotests = false` an undeclared file never runs.\n\
         undeclared (never compiled): {undeclared:?}\n\
         declared both ways: {both:?}\n\
         declared but absent: {missing:?}"
    );
}
