//! Spec 48, criterion 2 (rung 4) - the LIB-API contract of the committed-config rung of the
//! store-selection precedence: `rigger::config_store::read_store_config` and the `StoreConfig` /
//! `Workflow.store` deserialization surface it rides on.
//!
//! `tests/store_precedence.rs` proves the two file-backed rungs are WIRED into the shipped binary
//! and sit at the right place in the order, observed at the sqlite-vs-server boundary. This file
//! proves the OTHER edge of the same seam - the public reader's OWN contract, which the
//! binary-driving file exercises only transitively and never asserts on directly:
//!
//!   * an ABSENT workflow.yml is "no opinion" - the default, never an error, so a project that
//!     pins nothing keeps today's behavior (the backward-compatibility bar); a PRESENT-but-
//!     unreadable file instead surfaces LOUDLY, never the same silent default an absent file gives;
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

use rigger::config::{StoreConfig, Workflow};
use rigger::config_store::read_store_config;

#[path = "common/workflow_probe.rs"]
mod workflow_probe;
use workflow_probe::{assert_probe_reads, rigger_dir, write_workflow};

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

rigger::test_cases! {
    a_present_store_block_deserializes_backend_and_url: assert_probe_reads(
        read_store_config,
        "store:\n  backend: kurrentdb\n  url: \"kurrentdb://config-host:2113?tls=false\"\n",
        StoreConfig {
            backend: "kurrentdb".into(),
            url: "kurrentdb://config-host:2113?tls=false".into(),
        },
        "a present store: block must parse, reading the backend and the non-secret url verbatim",
    );
    // The reader parses ONLY the store: key through its throwaway probe; a workflow full of
    // stages/gates/agents/defaults that the bare courier has no need for must not make its store
    // resolution depend on - or fail on - any of them. The store block is still extracted exactly.
    unrelated_workflow_keys_are_ignored_by_the_lightweight_probe: assert_probe_reads(
        read_store_config,
        "defaults:\n  max_wall_clock: 900\nstages: []\ngates: {}\nstore:\n  backend: sqlite\n",
        StoreConfig {
            backend: "sqlite".into(),
            url: String::new(),
        },
        "unrelated keys must not break the store probe: the store block is extracted past them \
         and an omitted url defaults, unaffected by the surrounding keys",
    );
    // Back-compat: a legacy config predating the store: key still reads clean as no-opinion, so an
    // existing project is unaffected by the new rung.
    a_workflow_without_a_store_key_reads_as_the_default:
        assert_probe_reads(
            read_store_config,
            "stages: []\ngates: {}\n",
            StoreConfig::default(),
            "a store-less workflow must still read: no store: key is no-opinion (the default)",
        );
    // Empty / blank backend and url are "no opinion", matching the default - a project may write
    // the key skeleton without committing to a backend, and the resolver still falls through.
    an_empty_store_block_and_empty_values_are_no_opinion:
        assert_probe_reads(
            read_store_config,
            "store:\n  backend: \"\"\n  url: \"\"\n",
            StoreConfig::default(),
            "empty backend and url must parse, as no-opinion (the default)",
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
fn a_present_but_unreadable_workflow_surfaces_loudly_not_a_silent_default() {
    // A workflow.yml that is PRESENT but cannot be read (here a directory stands where the file
    // should be - an IO error distinct from NotFound) must surface LOUDLY, never collapse into the
    // same "no opinion" default an ABSENT file returns. Otherwise a bare courier on a
    // server-configured project whose owning-root config is unreadable would silently resolve local
    // sqlite and misfile its self-report off the store the conductor reads (the spec-05 wrong-store
    // fracture). The reader's own doc promises "never a silent wrong-store fallback"; this pins it.
    let (_tmp, dir) = rigger_dir();
    std::fs::create_dir(dir.join("workflow.yml")).expect("place a directory where the file goes");
    let err = read_store_config(&dir).expect_err(
        "a present-but-unreadable workflow.yml must be a loud error, not a silent default",
    );
    let msg = err.to_string();
    assert!(
        msg.contains("read store config"),
        "the failure must name itself as a store-config read error; got: {msg}"
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
