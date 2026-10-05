//! THE TEMPLATE SETS ARE EMBEDDED BY THE BUILD SCRIPT (spec 113): `generate_template_sets`
//! enumerates `scaffold/` once and returns both the generated `TEMPLATE_SETS` source and the
//! paths `build.rs` watches. A build script cannot run under `cargo test`, so this file includes
//! the exact source `build.rs` includes (`build/template_sets.rs`, by `#[path]`), as
//! `tests/build_watch_paths.rs` includes `build/watch.rs`, and drives it over fixture roots.

#[path = "../build/template_sets.rs"]
mod template_sets;

use std::path::{Path, PathBuf};
use template_sets::generate_template_sets;

/// Write `files` (each `(repository-relative path, content)`) under a fresh root.
fn root_with(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (rel, content) in files {
        let path = dir.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }
    dir
}

/// The refusal `generate_template_sets` returns over a root holding `files`.
fn refusal(files: &[(&str, &str)]) -> String {
    let root = root_with(files);
    match generate_template_sets(root.path()) {
        Ok(generated) => panic!("expected a refusal, generated:\n{}", generated.source),
        Err(e) => e,
    }
}

/// The refusal over a one-set root whose `files` text is `listed`, beside a script it may name.
fn files_refusal(listed: &str) -> String {
    refusal(&[
        ("scaffold/demo/set.yml", "detect: []\n"),
        ("scaffold/demo/files", listed),
        ("gates/a.sh", "echo a\n"),
    ])
}

fn assert_names(err: &str, wanted: &[&str]) {
    for w in wanted {
        assert!(err.contains(w), "the refusal must name {w:?}: {err}");
    }
}

rigger::test_cases! {
    /// An absolute listed path is refused naming the set and the path.
    refuses_an_absolute_listed_path: assert_names(
        &files_refusal("/etc/passwd\n"),
        &["gate template set demo", "/etc/passwd", "absolute"],
    );
    /// A listed path holding a `..` segment is refused naming the set and the path.
    refuses_a_dotdot_listed_path: assert_names(
        &files_refusal("gates/../gates/a.sh\n"),
        &["gate template set demo", "gates/../gates/a.sh", ".."],
    );
    /// A listed path naming no file is refused naming the set and the path.
    refuses_a_listed_path_naming_no_file: assert_names(
        &files_refusal("gates/missing.sh\n"),
        &["gate template set demo", "gates/missing.sh", "names no file"],
    );
    /// A listed path naming a directory names no file.
    refuses_a_listed_path_naming_a_directory: assert_names(
        &files_refusal("gates\n"),
        &["gate template set demo", "gates", "names no file"],
    );
    /// A path listed twice in one set's `files` is refused naming the set and the path.
    refuses_a_path_listed_twice: assert_names(
        &files_refusal("gates/a.sh\n\ngates/a.sh\n"),
        &["gate template set demo", "gates/a.sh", "listed twice"],
    );
    /// A set directory missing `set.yml` is refused naming the set.
    refuses_a_set_missing_set_yml: assert_names(
        &refusal(&[("scaffold/demo/files", "")]),
        &["gate template set demo", "set.yml"],
    );
    /// A set directory missing `files` is refused naming the set.
    refuses_a_set_missing_files: assert_names(
        &refusal(&[("scaffold/demo/set.yml", "detect: []\n")]),
        &["gate template set demo", "files"],
    );
    /// A root with no `scaffold/` is refused naming `scaffold/`.
    refuses_a_root_with_no_scaffold_dir: assert_names(
        &refusal(&[("README.md", "no scaffold here\n")]),
        &["scaffold/"],
    );
    /// A `scaffold/` holding no set directory, only a file, is refused naming `scaffold/`.
    refuses_a_scaffold_dir_holding_no_set_directory: assert_names(
        &refusal(&[("scaffold/README.md", "not a set\n")]),
        &["scaffold/", "no gate template set"],
    );
}

/// A file directly under `scaffold/` is skipped, the sets are enumerated in name order, blank
/// lines of `files` are skipped, and the generated source embeds each set's key, its `set.yml`
/// and each listed file through `include_str!` of a manifest-relative path. The watch paths are
/// `scaffold/`, both files of every set and every listed file.
#[test]
fn generates_every_set_in_name_order_and_returns_its_watch_paths() {
    let root = root_with(&[
        ("scaffold/README.md", "skipped\n"),
        ("scaffold/zeta/set.yml", "detect: [z]\n"),
        ("scaffold/zeta/files", ""),
        ("scaffold/alpha/set.yml", "detect: [a]\n"),
        ("scaffold/alpha/files", "\ngates/a.sh\n\ngates/b.sh\n"),
        ("gates/a.sh", "echo a\n"),
        ("gates/b.sh", "echo b\n"),
    ]);
    let generated = generate_template_sets(root.path()).expect("a well-formed scaffold/");
    let include =
        |rel: &str| format!("include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/{rel}\"))");
    let expected = format!(
        "const TEMPLATE_SETS: &[TemplateSet] = &[\n    \
         TemplateSet {{\n        key: \"alpha\",\n        set: {},\n        files: &[\n            \
         (\"gates/a.sh\", {}),\n            (\"gates/b.sh\", {}),\n        ],\n    }},\n    \
         TemplateSet {{\n        key: \"zeta\",\n        set: {},\n        files: &[\n        ],\n    }},\n];\n",
        include("scaffold/alpha/set.yml"),
        include("gates/a.sh"),
        include("gates/b.sh"),
        include("scaffold/zeta/set.yml"),
    );
    assert_eq!(generated.source, expected);
    let at = |rel: &str| -> PathBuf { root.path().join(rel) };
    assert_eq!(
        generated.watch_paths,
        vec![
            at("scaffold"),
            at("scaffold/alpha/set.yml"),
            at("scaffold/alpha/files"),
            at("gates/a.sh"),
            at("gates/b.sh"),
            at("scaffold/zeta/set.yml"),
            at("scaffold/zeta/files"),
        ]
    );
}

/// This repository's own `scaffold/` generates: every set it ships embeds, and the Rust set's
/// listed files are watched.
#[test]
fn this_repositorys_scaffold_generates() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let generated = generate_template_sets(root).expect("this repository's scaffold/ generates");
    assert!(
        generated.source.contains("key: \"rust\""),
        "the Rust set embeds: {}",
        generated.source
    );
    let listed = std::fs::read_to_string(root.join("scaffold/rust/files")).unwrap();
    for rel in listed.lines().filter(|l| !l.trim().is_empty()) {
        assert!(
            generated.watch_paths.contains(&root.join(rel)),
            "{rel} is watched: {:?}",
            generated.watch_paths
        );
    }
}
