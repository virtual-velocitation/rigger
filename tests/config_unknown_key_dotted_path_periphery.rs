//! Periphery (CLI, real-binary) test for spec 102, criterion 3 - AN UNKNOWN KEY IS NAMED.
//!
//! `config_store::load_workflow`'s own unit tests (`src/config_store.rs`) already pin the
//! dotted-path reformatting at the library level. This file proves the criterion's OTHER
//! explicit clause - "`rigger validate` on the same file fails with the same text" - end to
//! end, through the compiled binary, and against the library call `load()`/`validate` itself
//! resolve through: both are driven against the byte-identical fixture file in one test, so
//! "same text" is measured, not assumed from the two call sites sharing code.

mod common;

use std::path::Path;
use std::process::Command;

fn temp_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create temp project");
    let _ = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status();
    dir
}

fn run_rigger(cwd: &Path, args: &[&str]) -> (String, String, bool) {
    let out = common::rigger_courier()
        .args(args)
        .current_dir(cwd)
        .env("RIGGER_NO_DASH", "1")
        .output()
        .expect("failed to spawn the rigger binary");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

/// `rigger init` scaffolds a `.rigger/agents/` fleet that parses cleanly on its own, so
/// overwriting ONLY `workflow.yml` afterward isolates the unknown-key failure to the one
/// key under test rather than an unrelated agents-dir problem masking it.
const WORKFLOW_WITH_UNKNOWN_KEY: &str =
    "name: fixture\ndefaults:\n  autonomy: auto_notify\n  max_parallel_unitz: 2\n";

#[test]
fn rigger_validate_and_config_store_load_name_the_same_dotted_path_for_an_unknown_key() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    std::fs::write(
        root.join(".rigger").join("workflow.yml"),
        WORKFLOW_WITH_UNKNOWN_KEY,
    )
    .expect("overwrite workflow.yml with the unknown-key fixture");

    // The library call `rigger validate` itself resolves through.
    let lib_err = rigger::config_store::load(root.to_str().unwrap())
        .expect_err("a workflow.yml carrying an unrecognized key must fail to load")
        .to_string();
    assert!(
        lib_err.contains("defaults.max_parallel_unitz: unknown key"),
        "config_store::load must name the dotted path of the unrecognized key: {lib_err}"
    );

    // The CLI surface, through the compiled binary.
    let (_out, cli_err, cli_ok) = run_rigger(root, &["validate"]);
    assert!(
        !cli_ok,
        "rigger validate must fail on an unrecognized config key"
    );
    assert!(
        cli_err.contains("defaults.max_parallel_unitz: unknown key"),
        "rigger validate's stderr must name the same dotted path; stderr:\n{cli_err}"
    );

    // Both surfaces must report the identical text (spec 102 criterion 3: "the same text"),
    // not merely each contain a match for the same substring pattern independently.
    assert!(
        cli_err.contains(&lib_err),
        "rigger validate and config_store::load must fail with the SAME text: \
         lib=\"{lib_err}\" cli=\"{cli_err}\""
    );
}
