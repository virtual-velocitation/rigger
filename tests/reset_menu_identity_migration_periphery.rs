//! PERIPHERY (CLI) test for spec 68, criterion 3 - the bare-menu branch's OWN identity-migration
//! call.
//!
//! `cmd_reset`'s flagless branch runs the one-time spec-09 identity migration itself
//! (`if selection.is_sqlite() { migrate_identity_at(&loc)?; }`) BEFORE building the menu - a
//! SEPARATE occurrence of that call from the one the flagged path already runs, which
//! `tests/reset_derived_compaction.rs`'s
//! `reset_derived_compacts_a_log_whose_history_predates_the_minted_project_identity` already
//! proves migrates a legacy-identity store correctly for `--derived`. No existing test drives the
//! BARE path against a store whose history predates the minted project identity, so a bug that
//! dropped, reordered, or mis-scoped the bare branch's own call would read as: the menu silently
//! reports "0 dead-run node(s)" / "0 duplicate event(s)" on a store that is, in fact, full of
//! both - a perfectly successful preview of nothing, the exact silent-lie this whole feature
//! exists to prevent, and a shape a fixture that always seeds AFTER `rigger init` can never
//! reproduce.

mod common;

use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Event, EventStore, ExpectedRevision};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, UNIX_EPOCH};

// ---------------------------------------------------------------------------------------
// Harness (mirrors tests/reset_menu.rs and tests/reset_derived_compaction.rs; each integration
// suite is its own binary, so a small harness is duplicated per file by this codebase's existing
// convention rather than shared).
// ---------------------------------------------------------------------------------------

fn temp_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create temp project");
    let _ = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status();
    dir
}

fn run_stream_identity(root: &Path) -> String {
    let toplevel = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());
    let base = toplevel.as_deref().map(Path::new).unwrap_or(root);
    if let Ok(raw) = std::fs::read_to_string(base.join(".rigger").join("project.id")) {
        let id = raw.trim();
        if !id.is_empty() {
            return id.to_string();
        }
    }
    base.file_name()
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty())
        .map(String::from)
        .unwrap_or_else(|| "rigger".to_string())
}

fn event_log(root: &Path) -> PathBuf {
    root.join(".rigger").join("events.db")
}

fn seed_store(root: &Path) {
    std::fs::create_dir_all(root.join(".rigger")).unwrap();
    std::fs::File::create(event_log(root)).unwrap();
}

fn run_rigger(cwd: &Path, args: &[&str]) -> (String, String, bool) {
    let mut cmd = common::rigger_courier();
    cmd.args(args).current_dir(cwd);
    cmd.env("RIGGER_NO_DASH", "1");
    let state = tempfile::tempdir().expect("create a temp XDG_STATE_HOME");
    cmd.env("XDG_STATE_HOME", state.path());
    let out = cmd.output().expect("failed to spawn the rigger binary");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

fn emit(root: &Path, typ: &str, json: &str) {
    let (_o, err, ok) = run_rigger(root, &["emit", typ, json]);
    assert!(ok, "emit {typ} must succeed; stderr: {err}");
}

/// Seed lifecycle events directly into the namespaced run stream, standing in for the conductor
/// minting them (`rigger emit` refuses these conductor-owned boundary types).
fn seed_run_events(root: &Path, events: &[(&str, &str)]) {
    let backend = Store::open(event_log(root).to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    for &(ty, body) in events {
        store
            .append(
                rigger::conductor::STREAM,
                ExpectedRevision::Any,
                &[Event::new(ty, body.as_bytes().to_vec())],
            )
            .unwrap();
    }
}

const DUP_KEY: &str = "gc/src/a.rs@h1#0";
/// `DUP_ROUNDS - 1` recordings are prunable duplicates by `--derived`'s own rule.
const DUP_ROUNDS: usize = 3;

fn code_entity() -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "file": "src/a.rs", "name": "alpha", "kind": "function", "line": 1, "lang": "rust",
    }))
    .unwrap()
}

fn seed_derived_duplicates(root: &Path) {
    let backend = Store::open(event_log(root).to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    let mut events = Vec::with_capacity(DUP_ROUNDS);
    for r in 0..DUP_ROUNDS {
        events.push(
            Event::new(
                rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
                code_entity(),
            )
            .with_meta(rigger::ingest::META_REPLAY_KEY, DUP_KEY)
            .with_valid_from(UNIX_EPOCH + Duration::from_secs(1_000 + r as u64)),
        );
    }
    store
        .append(rigger::conductor::STREAM, ExpectedRevision::Any, &events)
        .unwrap();
}

#[test]
fn bare_reset_previews_the_migrated_stores_real_counts_when_history_predates_the_minted_project_identity(
) {
    let dir = temp_project();
    let root = dir.path();
    seed_store(root);

    // Seeded BEFORE `rigger init` mints an identity: filed under the LEGACY basename namespace,
    // exactly the shape a bloated store actually has (mirrors
    // reset_derived_compaction.rs::reset_derived_compacts_a_log_whose_history_predates_the_minted_project_identity,
    // now for the bare-menu's own migration call rather than the flagged path's).
    seed_run_events(root, &[("RunStarted", r#"{"run":"r1","criteria":["c"]}"#)]);
    emit(
        root,
        "DecisionMade",
        r#"{"id":"dead-d","summary":"dead","governs":["f.rs"]}"#,
    );
    // A second run starts, ALSO seeded under the (still unminted) legacy identity, so r1 is
    // provably dead (superseded by r2) before the migration ever runs. Seeding it here rather
    // than after `rigger init` keeps the WHOLE history under one namespace at mint time - the
    // migration is a one-time move of everything under the legacy prefix, not a merge of two
    // namespaces that each already hold data (a store that legitimately holds both is the
    // separate, deliberately-refused "ambiguous identity" case, not this one).
    seed_run_events(root, &[("RunStarted", r#"{"run":"r2","criteria":["c"]}"#)]);
    seed_derived_duplicates(root);
    let legacy = run_stream_identity(root);

    let (_, ierr, iok) = run_rigger(root, &["init"]);
    assert!(iok, "rigger init must scaffold the project; stderr: {ierr}");
    let minted = run_stream_identity(root);
    assert_ne!(
        minted, legacy,
        "rigger init must mint an identity distinct from the basename, or this fixture does not \
         reproduce the shape it exists for"
    );

    let (out, err, ok) = run_rigger(root, &["reset"]);
    assert!(
        ok,
        "a bare `rigger reset` must exit 0 even on a legacy-identity store; stderr: {err}"
    );

    // THE ASSERTION THIS TEST EXISTS FOR: a migration bug on the bare-menu's own call would leave
    // the menu reading under the minted identity while the seeded history still sits under the
    // legacy one, and it would report ZERO on both lines - a perfectly successful preview of
    // nothing, on a store that is not empty. Both counts must be the real, non-zero, migrated ones.
    assert!(
        out.contains("--runs: 1 dead-run node(s)"),
        "the bare menu must report the migrated store's real dead-run node count (1), not zero \
         from an unmigrated identity mismatch; got: {out:?}"
    );
    assert!(
        out.contains(&format!("--derived: {} duplicate event(s)", DUP_ROUNDS - 1)),
        "the bare menu must report the migrated store's real duplicate count ({}), not zero from \
         an unmigrated identity mismatch; got: {out:?}",
        DUP_ROUNDS - 1
    );

    // And the migration is real, not a duplication: every stream now lives under the MINTED
    // prefix, none under the legacy one.
    let minted_prefix = format!("proj-{minted}-");
    let legacy_prefix = format!("proj-{legacy}-");
    let conn = rusqlite::Connection::open(event_log(root)).unwrap();
    let mut stmt = conn.prepare("SELECT DISTINCT stream FROM events").unwrap();
    let streams: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    assert!(
        streams.iter().any(|s| s.starts_with(&minted_prefix)),
        "at least one stream must live under the minted namespace after the bare menu ran; got \
         {streams:?}"
    );
    assert!(
        !streams.iter().any(|s| s.starts_with(&legacy_prefix)),
        "no stream may be left behind under the legacy namespace after the bare menu ran; got \
         {streams:?}"
    );
}
