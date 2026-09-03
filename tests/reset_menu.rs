//! Periphery (CLI) tests for spec 68, criterion 3 - THE RESET SURFACE: bare `rigger reset` (no
//! flags at all) is a MENU, not an error.
//!
//! Before this criterion, `rigger reset` with no mode flag refused
//! ("expected at least one mode: rigger reset --runs ... and/or rigger reset --derived ...").
//! Now it exits 0 and prints one line per prunable accumulation (`--runs`'s dead-run context-graph
//! nodes/edges, `--derived`'s duplicate derived-index events), each with a MEASURED count and the
//! flag that acts on it - read-only, so running the bare command never prunes anything itself.
//!
//! What this file OWNS (criterion 3) and what it deliberately does not:
//!
//!   - OWNS: the bare-menu's exit code, its per-mode measured counts on an empty AND a populated
//!     store, that the menu never mutates the store, and that its numbers agree with what a real
//!     flagged prune actually removes.
//!   - NOT OWNED: the flagged `--runs`/`--derived` prune behavior itself (already pinned by
//!     `tests/cli.rs` and `tests/reset_derived_compaction.rs`, both untouched by this criterion -
//!     that is what "flagged behavior is byte-for-byte unchanged" means and what leaving those
//!     suites passing proves); the per-backend honesty branch for `--derived` on a non-sqlite
//!     backend, which needs no live server to exercise (`StoreSelection` is a `main.rs`-private
//!     type) and is instead pinned by an in-crate unit test beside `derived_menu_line`.

mod common;

use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Event, EventStore, ExpectedRevision};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, UNIX_EPOCH};

// ---------------------------------------------------------------------------------------
// Harness (mirrors tests/cli.rs and tests/reset_derived_compaction.rs; each integration
// suite is its own binary, so a small harness is duplicated per file by this codebase's
// existing convention rather than shared).
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

fn graph_db(root: &Path) -> PathBuf {
    root.join(".rigger").join("graph.db")
}

/// Seed an initialized, otherwise-empty `.rigger/events.db`, standing in for the store a prior
/// `rigger run`/`step` would have created (an empty file is a valid empty SQLite database;
/// `Store::open` adds the schema on first open).
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

/// A single dead-run, prunable-by-`--runs` context-graph node: a superseded run `r1` records one
/// `DecisionMade`, then the active run `r2` starts and records its own. `reset --runs` drops
/// exactly the dead run's node (spec 21) - here, exactly one.
fn seed_one_dead_run_node(root: &Path) {
    seed_run_events(root, &[("RunStarted", r#"{"run":"r1","criteria":["c"]}"#)]);
    emit(
        root,
        "DecisionMade",
        r#"{"id":"dead-d","summary":"dead","governs":["f.rs"]}"#,
    );
    seed_run_events(root, &[("RunStarted", r#"{"run":"r2","criteria":["c"]}"#)]);
    emit(
        root,
        "DecisionMade",
        r#"{"id":"live-d","summary":"live","governs":["f.rs"]}"#,
    );
}

/// How many recordings of the SAME derived replay key the fixture below writes. `DUP_ROUNDS - 1`
/// of them are prunable duplicates by `--derived`'s own rule (every recording except the latest).
const DUP_ROUNDS: usize = 3;
const DUP_KEY: &str = "gc/src/a.rs@h1#0";

fn code_entity() -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "file": "src/a.rs", "name": "alpha", "kind": "function", "line": 1, "lang": "rust",
    }))
    .unwrap()
}

/// Seed `DUP_ROUNDS` re-recordings of one derived replay key - the duplication `--derived` exists
/// to compact, small enough to keep this suite fast.
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

/// Raw row counts of a store's two files, read directly (never through the command under test),
/// so a before/after comparison proves the bare menu is read-only.
fn store_row_counts(root: &Path) -> (i64, i64, i64) {
    let ev = rusqlite::Connection::open(event_log(root)).unwrap();
    let events: i64 = ev
        .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
        .unwrap();
    let gr = rusqlite::Connection::open(graph_db(root)).unwrap();
    let nodes: i64 = gr
        .query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0))
        .unwrap();
    let edges: i64 = gr
        .query_row("SELECT COUNT(*) FROM edges", [], |r| r.get(0))
        .unwrap();
    (events, nodes, edges)
}

// ---------------------------------------------------------------------------------------
// The menu
// ---------------------------------------------------------------------------------------

#[test]
fn bare_reset_on_an_empty_store_exits_zero_and_reports_nothing_prunable() {
    let dir = temp_project();
    let root = dir.path();
    seed_store(root);

    let (out, err, ok) = run_rigger(root, &["reset"]);
    assert!(
        ok,
        "a bare `rigger reset` on an empty store must exit 0; stderr: {err}"
    );
    assert!(
        out.contains("--runs: 0 dead-run node(s)"),
        "the --runs line must report zero prunable on an empty store; got: {out:?}"
    );
    assert!(
        out.contains("--derived: 0 duplicate event(s)"),
        "the --derived line must report zero prunable on an empty store; got: {out:?}"
    );
}

#[test]
fn bare_reset_on_a_populated_store_reports_measured_counts_matching_a_real_prune_and_mutates_nothing(
) {
    let dir = temp_project();
    let root = dir.path();
    seed_store(root);
    seed_one_dead_run_node(root);
    seed_derived_duplicates(root);

    let before = store_row_counts(root);

    let (out, err, ok) = run_rigger(root, &["reset"]);
    assert!(
        ok,
        "a bare `rigger reset` on a populated store must exit 0; stderr: {err}"
    );
    assert!(
        out.contains("--runs: 1 dead-run node(s)"),
        "the --runs line must report the one dead-run node the fixture seeds; got: {out:?}"
    );
    assert!(
        out.contains(&format!("--derived: {} duplicate event(s)", DUP_ROUNDS - 1)),
        "the --derived line must report the {} prunable duplicates the fixture seeds; got: {out:?}",
        DUP_ROUNDS - 1
    );

    // READ-ONLY: the bare menu must never prune anything itself.
    let after = store_row_counts(root);
    assert_eq!(
        before, after,
        "a bare `rigger reset` must not mutate the event log or the context graph"
    );

    // HONEST: the previewed counts must agree with what a REAL flagged prune actually removes.
    let (out2, err2, ok2) = run_rigger(root, &["reset", "--runs", "--derived"]);
    assert!(
        ok2,
        "reset --runs --derived must succeed; stderr: {err2}\n{out2}"
    );
    assert!(
        out2.contains("pruned 1 dead-run"),
        "the real --runs prune must remove exactly the node the menu previewed; got: {out2:?}"
    );
    assert!(
        out2.contains(&format!(
            "CodeEntityExtracted {}",
            DUP_ROUNDS - 1
        )),
        "the real --derived prune must remove exactly the duplicates the menu previewed; got: {out2:?}"
    );
}

#[test]
fn bare_reset_never_prunes_the_graph_even_when_only_dead_run_nodes_are_present() {
    // A narrower read-only proof, isolated from the derived-log fixture: the context graph
    // specifically survives a bare `rigger reset` byte-for-byte (row-count-for-row-count).
    let dir = temp_project();
    let root = dir.path();
    seed_store(root);
    seed_one_dead_run_node(root);

    let (_, nodes_before, edges_before) = store_row_counts(root);
    let (out, err, ok) = run_rigger(root, &["reset"]);
    assert!(ok, "bare reset must exit 0; stderr: {err}");
    assert!(out.contains("--runs: 1 dead-run node(s)"), "got: {out:?}");

    let (_, nodes_after, edges_after) = store_row_counts(root);
    assert_eq!(
        nodes_before, nodes_after,
        "bare reset must prune no graph node"
    );
    assert_eq!(
        edges_before, edges_after,
        "bare reset must prune no graph edge"
    );
}
