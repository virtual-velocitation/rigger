//! Spec 48, criterion 2 (rung 4) - the LIB-API contract of the committed-config rung of the
//! store-selection precedence: `rigger::config::read_store_config` and the `StoreConfig` /
//! `Workflow.store` deserialization surface it rides on.
//!
//! `tests/store_precedence.rs` proves the two file-backed rungs are WIRED into the shipped binary
//! and sit at the right place in the order, observed at the sqlite-vs-server boundary. This file
//! proves the OTHER edge of the same seam - the public reader's OWN contract, which the
//! binary-driving file exercises only transitively and never asserts on directly:
//!
//!   * an ABSENT (or unreadable) workflow.yml is "no opinion" - the default, never an error, so a
//!     project that pins nothing keeps today's behavior (the backward-compatibility bar);
//!   * a present `store:` block deserializes to its exact backend and non-secret url;
//!   * a legacy workflow.yml with NO `store:` key still reads as the default, so adding the field
//!     and its reader breaks no existing config;
//!   * the reader is a LIGHTWEIGHT probe - a workflow.yml full of unrelated stages/gates/agents/
//!     defaults still yields just the store block, so a bare courier's store resolution never
//!     depends on, or fails on, a workflow concern it has no need for;
//!   * a syntactically MALFORMED workflow.yml surfaces as a clear parse error, never a silent
//!     wrong-store fallback to the default that would hide a typo behind today's sqlite behavior.
//!
//! Every assertion is hermetic - a temp `.rigger` directory, no process environment, no git - so
//! the committed rung's contract is regression-locked on every machine and on both feature lanes.

use std::path::Path;

use rigger::config::{read_store_config, StoreConfig, Workflow};
use tempfile::TempDir;

/// A temp `.rigger` directory the reader is anchored at (it joins `workflow.yml` onto this). The
/// returned `TempDir` must be kept alive by the caller or the directory is removed underneath it.
fn rigger_dir() -> (TempDir, std::path::PathBuf) {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let dir = tmp.path().join(".rigger");
    std::fs::create_dir_all(&dir).expect("create .rigger");
    (tmp, dir)
}

/// Write `<rigger_dir>/workflow.yml` with `body` - the committed project config the reader parses.
fn write_workflow(dir: &Path, body: &str) {
    std::fs::write(dir.join("workflow.yml"), body).expect("write workflow.yml");
}

#[test]
fn an_absent_workflow_is_no_opinion_not_an_error() {
    // No workflow.yml at all - a project that pins nothing. The reader returns the default, never
    // an error, so the store resolver falls through to its next (default) rung.
    let (_tmp, dir) = rigger_dir();
    let cfg =
        read_store_config(&dir).expect("an absent workflow.yml must be Ok(default), not an error");
    assert_eq!(
        cfg,
        StoreConfig::default(),
        "an absent config is no-opinion (the default)"
    );
    assert!(
        cfg.backend.is_empty() && cfg.url.is_empty(),
        "the default carries no backend and no url"
    );
}

#[test]
fn a_present_store_block_deserializes_backend_and_url() {
    let (_tmp, dir) = rigger_dir();
    write_workflow(
        &dir,
        "store:\n  backend: kurrentdb\n  url: \"kurrentdb://config-host:2113?tls=false\"\n",
    );
    let cfg = read_store_config(&dir).expect("a present store: block must parse");
    assert_eq!(cfg.backend, "kurrentdb", "the backend is read verbatim");
    assert_eq!(
        cfg.url, "kurrentdb://config-host:2113?tls=false",
        "the non-secret url is read verbatim"
    );
}

#[test]
fn a_workflow_without_a_store_key_reads_as_the_default() {
    // Back-compat: a legacy config predating the store: key still reads clean as no-opinion, so an
    // existing project is unaffected by the new rung.
    let (_tmp, dir) = rigger_dir();
    write_workflow(&dir, "stages: []\ngates: {}\n");
    let cfg = read_store_config(&dir).expect("a store-less workflow must still read");
    assert_eq!(
        cfg,
        StoreConfig::default(),
        "no store: key is no-opinion (the default)"
    );
}

#[test]
fn unrelated_workflow_keys_are_ignored_by_the_lightweight_probe() {
    // The reader parses ONLY the store: key through its throwaway probe; a workflow full of
    // stages/gates/agents/defaults that the bare courier has no need for must not make its store
    // resolution depend on - or fail on - any of them. The store block is still extracted exactly.
    let (_tmp, dir) = rigger_dir();
    write_workflow(
        &dir,
        "defaults:\n  max_wall_clock: 900\nstages: []\ngates: {}\nstore:\n  backend: sqlite\n",
    );
    let cfg = read_store_config(&dir).expect("unrelated keys must not break the store probe");
    assert_eq!(
        cfg.backend, "sqlite",
        "the store block is extracted past the unrelated keys"
    );
    assert!(
        cfg.url.is_empty(),
        "an omitted url defaults, unaffected by the surrounding keys"
    );
}

#[test]
fn a_malformed_workflow_surfaces_a_parse_error_not_a_silent_default() {
    // A syntactically broken workflow.yml must be a LOUD parse error, never a silent fallback to
    // the default that would hide a typo behind today's sqlite behavior (an unclosed flow mapping
    // is a scanner error the reader must not swallow).
    let (_tmp, dir) = rigger_dir();
    write_workflow(&dir, "store: {backend: kurrentdb\n");
    let err =
        read_store_config(&dir).expect_err("malformed yaml must be an error, not a silent default");
    let msg = err.to_string();
    assert!(
        msg.contains("parse store config"),
        "the parse failure must name itself as a store-config parse error; got: {msg}"
    );
}

#[test]
fn an_empty_store_block_and_empty_values_are_no_opinion() {
    // Empty / blank backend and url are "no opinion", matching the default - a project may write
    // the key skeleton without committing to a backend, and the resolver still falls through.
    let (_tmp, dir) = rigger_dir();
    write_workflow(&dir, "store:\n  backend: \"\"\n  url: \"\"\n");
    let cfg = read_store_config(&dir).expect("empty values must parse");
    assert_eq!(
        cfg,
        StoreConfig::default(),
        "empty backend and url are no-opinion (the default)"
    );
}

#[test]
fn the_workflow_store_field_deserializes_and_defaults_for_back_compat() {
    // The new `Workflow.store` field: a committed workflow deserializes its store block onto the
    // Workflow, and a legacy workflow with no store: key defaults the field - so adding it breaks
    // no existing config.
    let with_store: Workflow = serde_yaml::from_str(
        "store:\n  backend: kurrentdb\n  url: \"kurrentdb://h:2113?tls=false\"\n",
    )
    .expect("a workflow with a store block must deserialize");
    assert_eq!(with_store.store.backend, "kurrentdb");
    assert_eq!(with_store.store.url, "kurrentdb://h:2113?tls=false");

    let legacy: Workflow = serde_yaml::from_str("name: legacy\ngates: {}\n")
        .expect("a store-less workflow must still deserialize");
    assert_eq!(
        legacy.store,
        StoreConfig::default(),
        "an absent store: defaults for back-compat"
    );
}
