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

use common::cli::emit;
use common::cli::rigger_file;
use common::cli::run_rigger;
use common::cli::seed_derived_duplicates;
use common::cli::seed_run_events;
use common::cli::seed_store;
use common::cli::temp_project;
use common::cli::DUP_ROUNDS;
use std::path::Path;

// ---------------------------------------------------------------------------------------
// Harness (mirrors tests/cli.rs and tests/reset_derived_compaction.rs; each integration
// suite is its own binary, so a small harness is duplicated per file by this codebase's
// existing convention rather than shared).
// ---------------------------------------------------------------------------------------

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

/// Raw row counts of a store's two files, read directly (never through the command under test),
/// so a before/after comparison proves the bare menu is read-only.
fn store_row_counts(root: &Path) -> (i64, i64, i64) {
    let ev = rusqlite::Connection::open(rigger_file(root, "events.db")).unwrap();
    let events: i64 = ev
        .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
        .unwrap();
    let gr = rusqlite::Connection::open(rigger_file(root, "graph.db")).unwrap();
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
