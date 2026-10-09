//! Periphery (CLI) tests for spec 68, criterion 3 - THE RESET SURFACE: bare `rigger reset` (no
//! flags at all) is a MENU, not an error.
//!
//! Before this criterion, `rigger reset` with no mode flag refused
//! ("expected at least one mode: rigger reset --runs ... and/or rigger reset --derived ...").
//! Now it exits 0 and prints one line per prunable accumulation (`--runs`'s dead-run context-graph
//! nodes/edges, `--derived`'s derived events and the file identities holding them), each with a
//! MEASURED count and the flag that acts on it - read-only, so running the bare command never
//! prunes anything itself.
//!
//! What this file OWNS (criterion 3) and what it deliberately does not:
//!
//!   - OWNS: the bare-menu's exit code, its per-mode measured counts on an empty AND a populated
//!     store, and that the menu never mutates the store.
//!   - NOT OWNED: the flagged `--runs`/`--derived` behavior itself (pinned by `tests/cli.rs`,
//!     which also holds the menu's agreement with what `--derived` then sheds, and
//!     `tests/reset_derived_compaction.rs`); the per-backend honesty branch for `--derived` on a
//!     non-sqlite backend, which needs no live server to exercise (`StoreSelection` is a
//!     `main.rs`-private type) and is instead pinned by an in-crate unit test beside
//!     `derived_menu_line`.

mod common;

use common::cli::derived_menu_line_naming;
use common::cli::derived_menu_lines;
use common::cli::emit;
use common::cli::rigger_file;
use common::cli::run_rigger;
use common::cli::seed_run_events;
use common::cli::temp_store_project;
use common::cli::with_run_store;
use common::cli::NOTHING_TO_SHED_MENU_LINE;
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
    let dir = temp_store_project();
    let root = dir.path();

    let (out, err, ok) = run_rigger(root, &["reset"]);
    assert!(
        ok,
        "a bare `rigger reset` on an empty store must exit 0; stderr: {err}"
    );
    assert!(
        out.contains("--runs: 0 dead-run node(s)"),
        "the --runs line must report zero prunable on an empty store; got: {out:?}"
    );
    assert_eq!(
        derived_menu_lines(&out),
        [NOTHING_TO_SHED_MENU_LINE],
        "the --derived line must say an empty store holds no derived event to shed"
    );
}

/// Given a store whose only derived events name no file identity - one with no replay key and
/// one whose replay key does not parse - when the operator runs bare `rigger reset`, then the
/// `--derived` line still names the two events and the flag, of zero file identities, and never
/// says there is nothing to shed.
#[test]
fn bare_reset_names_the_derived_events_of_a_store_whose_rows_name_no_file_identity() {
    let dir = temp_store_project();
    let root = dir.path();
    let entity = common::cli::code_entity;
    let type_ = rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED;
    with_run_store(root, |store| {
        store
            .append(
                rigger::conductor::STREAM,
                rigger::eventstore::ExpectedRevision::Any,
                &[
                    rigger::eventstore::Event::new(type_, entity()),
                    common::cli::keyed(type_, entity(), "gc/src/odd.rs", 10),
                ],
            )
            .unwrap();
    });

    let (out, err, ok) = run_rigger(root, &["reset"]);
    assert!(ok, "bare reset must exit 0; stderr: {err}");
    assert_eq!(
        derived_menu_lines(&out),
        [derived_menu_line_naming(2, 0)],
        "two derived events naming no file identity are still events `--derived` sheds"
    );
}

#[test]
fn bare_reset_never_prunes_the_graph_even_when_only_dead_run_nodes_are_present() {
    // A narrower read-only proof, isolated from the derived-log fixture: the context graph
    // specifically survives a bare `rigger reset` byte-for-byte (row-count-for-row-count).
    let dir = temp_store_project();
    let root = dir.path();
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
