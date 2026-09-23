//! Periphery (CLI, real-binary) test for spec 102, criterion 3 - AN UNKNOWN KEY IS NAMED.
//!
//! `config_store::load_workflow`'s own unit tests (`src/config_store.rs`) already pin the
//! dotted-path reformatting at the library level. This file proves the criterion's OTHER
//! explicit clause - "`rigger validate` on the same file fails with the same text" - end to
//! end, through the compiled binary, and against the library call `load()`/`validate` itself
//! resolve through: both are driven against the byte-identical fixture file in one test, so
//! "same text" is measured, not assumed from the two call sites sharing code.
//!
//! The criterion OWNS unknown-key rejection "at every config level" - and `dotted_unknown_key`
//! is wired at THREE independent call sites (`config.rs`'s own doc comment on the commit that
//! added it), not one: `load_workflow` (proven above), `read_store_config`, and
//! `read_scratch_defaults`. The second test below closes `read_store_config`'s own wiring,
//! which no other test (unit or periphery) reaches - it is a SEPARATE `.map_err` call over a
//! DIFFERENT struct (`StoreConfig`, not `Workflow`/`Defaults`), driven by `rigger status`
//! rather than `rigger validate` because `read_store_config` is the store-selection probe
//! (§48 rung 4) every store-opening command resolves through BEFORE it ever requires an
//! existing run, so it is observable without first bootstrapping an `events.db`.
//! `read_scratch_defaults`'s own wiring is EXEMPT: it wraps the identical `Defaults` struct
//! through the identical `dotted_unknown_key` call and the identical `"parse workflow: {}"`
//! prefix `load_workflow` already uses (byte-for-byte, per `src/config_store.rs`), so its
//! dotted-path text is already proven by the first test above; its one CLI-propagating caller
//! (`reset --build-cache`, via `read_scratch_workdir`) has no additional branching logic to
//! diverge on, and its other production caller (`scratch_defaults` in `main.rs`) deliberately
//! discards the error via `.unwrap_or_default()` (a pre-existing, documented contract this
//! diff does not change), so no new behavior is observable there at all.

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

/// `read_store_config`'s own `dotted_unknown_key` wiring (a SEPARATE call site from
/// `load_workflow`'s, over the `StoreConfig` struct rather than `Workflow`/`Defaults`) -
/// unreached by any other test in this tree. `rigger status` drives it because store
/// selection (§48 rung 4, the committed `store:` block) resolves before any command
/// requires an existing `events.db`, so this needs no run bootstrapped first.
#[test]
fn rigger_status_and_read_store_config_name_the_same_dotted_path_for_an_unknown_store_key() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    std::fs::write(
        root.join(".rigger").join("workflow.yml"),
        "name: fixture\nstore:\n  backend: sqlite\n  urll: bogus\n",
    )
    .expect("overwrite workflow.yml with the unknown-key fixture");

    // The library call `read_store_config` itself resolves through.
    let lib_err = rigger::config_store::read_store_config(&root.join(".rigger"))
        .expect_err("a store: block carrying an unrecognized key must fail to load")
        .to_string();
    assert!(
        lib_err.contains("store.urll: unknown key"),
        "read_store_config must name the dotted path of the unrecognized key: {lib_err}"
    );

    // The CLI surface, through the compiled binary.
    let (_out, cli_err, cli_ok) = run_rigger(root, &["status"]);
    assert!(
        !cli_ok,
        "rigger status must fail on an unrecognized store: key"
    );
    assert!(
        cli_err.contains("store.urll: unknown key"),
        "rigger status's stderr must name the same dotted path; stderr:\n{cli_err}"
    );

    // Both surfaces must report the identical text, exactly as the load_workflow case above
    // proves for its own two call sites.
    assert!(
        cli_err.contains(&lib_err),
        "rigger status and read_store_config must fail with the SAME text: \
         lib=\"{lib_err}\" cli=\"{cli_err}\""
    );
}
