//! Spec 77, criterion 4 (BOUNDED SHARED CACHE) - the LIB-API contract of
//! `rigger::config::read_scratch_workdir`, the LIGHTWEIGHT probe `rigger reset --build-cache`
//! resolves `defaults.workdir` through, mirroring `read_store_config`'s own established shape
//! (`tests/store_config.rs`) for the identical reason: a pure filesystem reclaim over the
//! scratch root must not additionally require a fully loadable agent fleet or a `Config::validate`
//! pass just to learn one string field.
//!
//! What this proves, each independent of the others:
//!
//!   * an ABSENT workflow.yml is "no opinion" - resolves to `""` (the scratch-root resolver's own
//!     default, `<repo>/.rigger/tmp`), never an error;
//!   * a present `defaults.workdir` deserializes to its exact value;
//!   * a workflow.yml with unrelated stages/gates/agents/other-defaults still yields just the one
//!     field - the reader never depends on (or fails on) a workflow concern it has no need for,
//!     including one whose `agents:`/`stages:` a full `config::load` would refuse to resolve;
//!   * a syntactically MALFORMED workflow.yml surfaces as a clear parse error, never a silent
//!     fallback to the default that would hide a typo.

use std::path::Path;

use rigger::config::read_scratch_workdir;
use tempfile::TempDir;

/// A temp `.rigger` directory the reader is anchored at (it joins `workflow.yml` onto this). The
/// returned `TempDir` must be kept alive by the caller or the directory is removed underneath it.
fn rigger_dir() -> (TempDir, std::path::PathBuf) {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let dir = tmp.path().join(".rigger");
    std::fs::create_dir_all(&dir).expect("create .rigger");
    (tmp, dir)
}

fn write_workflow(dir: &Path, body: &str) {
    std::fs::write(dir.join("workflow.yml"), body).expect("write workflow.yml");
}

#[test]
fn an_absent_workflow_is_no_opinion_not_an_error() {
    let (_tmp, dir) = rigger_dir();
    let workdir = read_scratch_workdir(&dir)
        .expect("an absent workflow.yml must be Ok(default), not an error");
    assert_eq!(
        workdir, "",
        "no opinion resolves to empty, the resolver's own default rung"
    );
}

#[test]
fn a_present_workdir_deserializes_exactly() {
    let (_tmp, dir) = rigger_dir();
    write_workflow(&dir, "defaults:\n  workdir: /custom/scratch\n");
    let workdir = read_scratch_workdir(&dir).expect("a present defaults.workdir must parse");
    assert_eq!(workdir, "/custom/scratch");
}

#[test]
fn unrelated_keys_never_break_the_probe_even_when_they_would_fail_a_full_config_load() {
    // Real stages/gates/agents references that a full `config::load` would refuse to resolve
    // (no `agents/` dir on disk, an undeclared gate) must not stop this lightweight probe from
    // reading the one field it cares about - the whole point of NOT routing `reset --build-cache`
    // through the full loader.
    let (_tmp, dir) = rigger_dir();
    write_workflow(
        &dir,
        "defaults:\n  workdir: /scratch/here\n  budget: 5\nstages:\n  a:\n    agent: nonexistent-agent\n    gates: [also-nonexistent]\n",
    );
    let workdir = read_scratch_workdir(&dir)
        .expect("unrelated stage/gate references a full config::load would refuse must not break this probe");
    assert_eq!(workdir, "/scratch/here");
}

#[test]
fn a_workflow_with_no_defaults_block_at_all_reads_as_empty() {
    let (_tmp, dir) = rigger_dir();
    write_workflow(&dir, "stages: {}\n");
    let workdir =
        read_scratch_workdir(&dir).expect("a workflow with no defaults: key must still parse");
    assert_eq!(workdir, "");
}

#[test]
fn malformed_yaml_is_a_loud_error_never_a_silent_default() {
    let (_tmp, dir) = rigger_dir();
    write_workflow(&dir, "defaults: [this is not a mapping\n");
    read_scratch_workdir(&dir).expect_err("malformed yaml must be an error, not a silent default");
}
