//! With `autotests = false`, a `tests/*.rs` file nobody declares is never compiled: its tests
//! never run while every gate stays green. Every top-level test file is exactly one of a module
//! of the consolidated `all` executable (`mod <name>;` in `tests/all.rs`, cfg-gated ones
//! included) or the file of a `[[test]]` entry in the root `Cargo.toml` (its `path`, else cargo's
//! default `tests/<name>.rs`), and a `[[test]]` entry is named after its file.

use std::collections::BTreeSet;

use crate::common::repo::{repo_root, repo_text};

/// The modules `tests/all.rs` declares that resolve to a file of their own; a `#[path]` module
/// is a helper under `tests/common`, not a test file.
fn modules_of(all_rs: &str) -> BTreeSet<String> {
    let mut modules = BTreeSet::new();
    let mut path_attribute = false;
    for line in all_rs.lines().map(str::trim) {
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

/// The `[[test]]` entries of a manifest, each as `(name, file stem)`; a missing `path` is
/// cargo's default `tests/<name>.rs`.
fn test_entries(manifest: &str) -> Vec<(String, String)> {
    let value = |line: &str, key: &str| {
        line.strip_prefix(key)
            .and_then(|rest| rest.strip_prefix(" = \""))
            .and_then(|rest| rest.strip_suffix('"'))
            .map(str::to_string)
    };
    let mut entries: Vec<(String, Option<String>)> = Vec::new();
    let mut in_test = false;
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            in_test = line == "[[test]]";
            if in_test {
                entries.push((String::new(), None));
            }
        } else if let (true, Some(entry)) = (in_test, entries.last_mut()) {
            if let Some(name) = value(line, "name") {
                entry.0 = name;
            } else if let Some(path) = value(line, "path") {
                entry.1 = Some(path);
            }
        }
    }
    entries
        .into_iter()
        .map(|(name, path)| {
            let path = path.unwrap_or_else(|| format!("tests/{name}.rs"));
            let stem = path
                .strip_prefix("tests/")
                .and_then(|p| p.strip_suffix(".rs"))
                .unwrap_or(&path)
                .to_string();
            (name, stem)
        })
        .collect()
}

/// What is wrong with how the top-level test files `on_disk` are declared; empty when nothing is.
/// `has_mod_dir(n)` says whether `tests/<n>/mod.rs` exists.
fn problems(
    on_disk: &BTreeSet<String>,
    all_rs: &str,
    manifest: &str,
    has_mod_dir: impl Fn(&str) -> bool,
) -> Vec<String> {
    let modules = modules_of(all_rs);
    let entries: Vec<(String, String)> = test_entries(manifest)
        .into_iter()
        .filter(|(_, stem)| stem != "all")
        .collect();
    let separate: BTreeSet<&String> = entries.iter().map(|(_, stem)| stem).collect();
    let mut found = Vec::new();
    for name in on_disk {
        if !modules.contains(name) && !separate.contains(name) {
            found.push(format!("tests/{name}.rs is undeclared (never compiled)"));
        }
        if modules.contains(name) && separate.contains(name) {
            found.push(format!("tests/{name}.rs is declared both ways"));
        }
    }
    for name in modules.iter().chain(separate.iter().copied()) {
        if !on_disk.contains(name) && !has_mod_dir(name) {
            found.push(format!("tests/{name}.rs is declared but absent"));
        }
    }
    for (name, stem) in &entries {
        if name != stem {
            found.push(format!(
                "[[test]] `{name}` is not named after its file tests/{stem}.rs"
            ));
        }
    }
    found
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
    let found = problems(
        &on_disk,
        &repo_text("tests/all.rs"),
        &repo_text("Cargo.toml"),
        |n| repo_root().join(format!("tests/{n}/mod.rs")).exists(),
    );
    assert!(
        found.is_empty(),
        "with `autotests = false` a test file is declared exactly once, as a `mod` in \
         tests/all.rs or a `[[test]]` entry named after it in Cargo.toml, else it never \
         runs: {found:#?}"
    );
}

/// Each way to declare a file wrongly, and the ways that are right, over a synthetic tree.
#[test]
fn the_declaration_check_flags_each_wrong_way_and_passes_each_right_one() {
    const ALL: &str = "mod common;\n#[path = \"common/h.rs\"]\nmod h;\nmod a;\n#[cfg(x)]\nmod b;\n";
    let tree = |names: &[&str]| names.iter().map(|n| n.to_string()).collect::<BTreeSet<_>>();
    let entry = |name: &str, path: Option<&str>| {
        let path = path
            .map(|p| format!("path = \"{p}\"\n"))
            .unwrap_or_default();
        format!("[[test]]\nname = \"{name}\"\n{path}")
    };
    for (label, files, all, manifest, expected) in [
        (
            "modules and a pathed entry",
            &["a", "b", "c"][..],
            ALL,
            entry("c", Some("tests/c.rs")),
            0,
        ),
        (
            "an entry with no path",
            &["a", "b", "c"],
            ALL,
            entry("c", None),
            0,
        ),
        (
            "an undeclared file",
            &["a", "b", "z"],
            ALL,
            String::new(),
            1,
        ),
        (
            "a file declared both ways",
            &["a", "b"],
            ALL,
            entry("a", None),
            1,
        ),
        ("a module with no file", &["a"], ALL, String::new(), 1),
        (
            "an entry with no file",
            &["a", "b"],
            ALL,
            entry("c", None),
            1,
        ),
        (
            "an entry named off its file",
            &["a", "b", "c"],
            ALL,
            entry("d", Some("tests/c.rs")),
            1,
        ),
    ] {
        let found = problems(&tree(files), all, &manifest, |_| false);
        assert_eq!(found.len(), expected, "{label}: {found:?}");
    }
}
