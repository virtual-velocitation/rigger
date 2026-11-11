//! Acceptance test for `rigger graph --show <entity>` GRACEFUL STALENESS (spec 58, criterion 2:
//! the degrade path). It drives the COMPILED `rigger` binary against a throwaway project whose
//! `graph.db` is seeded from the ALWAYS-compiled `CodeEntityExtracted` fold (so the seeding runs
//! identically in BOTH feature lanes) and whose working tree no longer holds the recorded file:
//! the command EXITS SUCCESS (never an error) and prints the site header with a note in place of
//! the body. The degrade is pre-extent (the working-tree read fails before any grammar is
//! consulted), so it holds LANE-INDEPENDENTLY.

// The compiled `rigger` binary under test is located at RUNTIME by the shared authority in
// `tests/common`: a path baked in at compile time goes stale the moment the target dir moves,
// and every suite that spawns the product then dies with a bare NotFound.
use crate::common;

use common::cli::body_line_count;
use common::cli::open_graph;
use common::cli::run_rigger;
use common::cli::seed_rigger_dir;
use common::cli::temp_project;
use common::fixtures::apply_code_entity;

/// DRIFT SHAPE (a): the recorded FILE is missing from the working tree. The show surface still
/// prints the recorded SITE header (the graph facts survive), replaces the body with a STALE note,
/// and EXITS SUCCESS - never an error. This degrade is pre-extent (the working-tree read fails
/// before any grammar is consulted), so the whole contract holds LANE-INDEPENDENTLY.
#[test]
fn graph_show_degrades_gracefully_when_the_recorded_file_is_missing() {
    let dir = temp_project();
    let root = dir.path();
    seed_rigger_dir(root);

    // `ghost` is recorded at gone.rs:7, but gone.rs is never written to the working tree - so its
    // recorded location no longer matches the tree (the file was deleted since the graph was built).
    // A sibling `present.rs` exists and is folded too, so the graph is genuinely populated (the miss
    // is a real drift, not an empty db).
    std::fs::write(root.join("present.rs"), "fn present() {}\n").unwrap();
    {
        let p = open_graph(root);
        apply_code_entity(&p, 1, "present.rs", "present", "function", 1, "rust");
        apply_code_entity(&p, 2, "gone.rs", "ghost", "function", 7, "rust");
    }

    let (out, err, ok) = run_rigger(root, &["graph", "--show", "ghost"]);

    // NEVER AN ERROR: a missing working-tree file exits SUCCESS.
    assert!(
        ok,
        "a missing recorded file degrades gracefully (exit SUCCESS, never an error); stderr: {err}"
    );
    // THE SITE SURVIVES: the recorded site header and the graph facts (kind/degree) still print.
    assert!(
        out.contains("gone.rs:7"),
        "the recorded site header survives a missing file; got:\n{out}"
    );
    assert!(
        out.contains("degree"),
        "the graph facts (kind/degree) still print when the body is unavailable; got:\n{out}"
    );
    // A STALE NOTE stands in for the body - lane-independent, since the read fails before any extent
    // derivation.
    assert!(
        out.contains("stale"),
        "a stale-location note replaces the body for a missing file; got:\n{out}"
    );
    // NEVER WRONG TEXT: no line-numbered body is presented as the entity's current source.
    assert_eq!(
        body_line_count(&out),
        0,
        "no line-numbered body is printed for a missing file; got:\n{out}"
    );
}
