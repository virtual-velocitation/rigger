//! Integration test for `rigger graph --show <entity>` (spec 58, criterion 1: the show
//! surface's RESOLUTION and OUTPUT). It drives the COMPILED `rigger` binary against a throwaway
//! project whose `graph.db` is seeded with code-entity nodes - folded from the ALWAYS-compiled
//! `CodeEntityExtracted` events, so the test runs identically in BOTH feature lanes (default and
//! `--no-default-features`) - and whose working tree holds the real source the body is read from.
//!
//! It proves the three faces of criterion 1 through the real CLI path (`main::cmd_graph` ->
//! `cmd_graph_show` -> `Projector::locate` -> body read from the working tree):
//!   - SHOW BY UNIQUE NAME: a bare name with one definition prints its site + line-numbered body.
//!   - SHOW BY ID: the full `<file>::<name>` id resolves to the SAME entity and body.
//!   - AMBIGUOUS NAME: a bare name with several definitions LISTS them (sorted, with files) and
//!     prints NO body - the call-views honesty rule (never guess among candidates).
//!   - A PATH OR A FLAT ID (spec 119, A BARE SHOW FINDS A LEAF OR PATH): a path `T::new` and a
//!     pre-spec flat `<file>::<name>` each list every definition they name, and a flat id a
//!     decision holds shows the definition its file holds at its leaf, never the bare node.
//!   - AN AMBIGUOUS AROUND (spec 117, UNRESOLVED REFERENCES ARE A READ): `--around` on a held flat
//!     id whose show lists several definitions prints the held node's line and no `unresolved`
//!     line after it.
//!
//! RESOLUTION is feature-lane independent (the code-entity fold is always compiled), so the site
//! header and the ambiguous listing are asserted in BOTH lanes. The BODY is bounded through the
//! `symbols` extraction grammar (tree-sitter), so its content is asserted under `symbols`; the
//! light (`--no-default-features`) lane, which links no grammar, degrades a located entity to the
//! site plus an explicit extent-unavailable note and NO body - the honesty contract for a feature
//! lane that cannot derive the extent (asserted by [`assert_light_lane_extent_note`]).

use rigger::contextgraph::sqlite::Projector;

// The compiled `rigger` binary under test is located at RUNTIME by the shared authority in
// `tests/common`: a path baked in at compile time goes stale the moment the target dir moves,
// and every suite that spawns the product then dies with a bare NotFound.
mod common;

#[cfg(not(feature = "symbols"))]
use common::cli::assert_light_lane_extent_note;

use common::cli::ambiguous_show;
use common::cli::open_graph;
use common::cli::run_around;
use common::cli::run_rigger;
use common::cli::run_rigger_ok;
use common::cli::run_stream_identity;
use common::cli::seed_rigger_dir;
use common::cli::show_head;
use common::cli::temp_project;
use common::fixtures::apply_code_entity;
use common::fixtures::{fold_path_def_rows, hold, PathDef, A_RS_NEWS, FLAT_LEAF_ID, T_NEW, U_NEW};

#[test]
fn graph_show_resolves_by_id_and_name_and_lists_ambiguous_candidates() {
    let dir = temp_project();
    let root = dir.path();
    seed_rigger_dir(root);

    // Working tree the body is read from. `a.rs` defines `alpha` (line 1) then `shared` (line 3,
    // with a distinctive body token); `b.rs` defines a SECOND `shared` (line 1, its own token).
    std::fs::write(
        root.join("a.rs"),
        "fn alpha() {}\n\
         \n\
         fn shared() {\n\
         \x20\x20\x20\x20let marker_in_a = 1;\n\
         }\n",
    )
    .unwrap();
    std::fs::write(
        root.join("b.rs"),
        "fn shared() {\n\
         \x20\x20\x20\x20let marker_in_b = 2;\n\
         }\n",
    )
    .unwrap();

    // Seed the graph: alpha@a.rs:1, shared@a.rs:3, shared@b.rs:1. Distinct positions so each
    // fold is a distinct (non-deduped) apply.
    let id = run_stream_identity(root);
    {
        let p =
            Projector::open(root.join(".rigger").join("graph.db").to_str().unwrap(), &id).unwrap();
        apply_code_entity(&p, 1, "a.rs", "alpha", "function", 1, "rust");
        apply_code_entity(&p, 2, "a.rs", "shared", "function", 3, "rust");
        apply_code_entity(&p, 3, "b.rs", "shared", "function", 1, "rust");
    }

    // (1) SHOW BY UNIQUE NAME: `alpha` has one definition -> its site + kind + degree + a
    //     line-numbered body.
    let (out, err, ok) = run_rigger(root, &["graph", "--show", "alpha"]);
    assert!(ok, "graph --show alpha must succeed; stderr: {err}");
    assert!(
        out.contains("a.rs:1"),
        "prints the definition SITE (file:line); got:\n{out}"
    );
    assert!(
        out.contains("function"),
        "prints the entity KIND; got:\n{out}"
    );
    assert!(
        out.contains("degree"),
        "prints the entity's one-hop DEGREE; got:\n{out}"
    );
    // The BODY is bounded through the extraction grammar: asserted under `symbols`; the light lane
    // degrades to the extent-unavailable note with no body.
    #[cfg(feature = "symbols")]
    assert!(
        out.lines()
            .any(|l| l.contains("fn alpha") && l.contains('1')),
        "the definition BODY is shown, line-numbered with its 1-based line; got:\n{out}"
    );
    #[cfg(not(feature = "symbols"))]
    assert_light_lane_extent_note(&out);

    // (2) SHOW BY ID: the full `<file>::<name>` id resolves to the SAME entity, and its body is
    //     read from the working tree at the recorded line - a.rs's `shared`, never b.rs's.
    let (out_id, err, ok) = run_rigger(root, &["graph", "--show", "a.rs::shared"]);
    assert!(ok, "graph --show a.rs::shared must succeed; stderr: {err}");
    assert!(
        out_id.contains("a.rs:3"),
        "the full id resolves to shared@a.rs:3; got:\n{out_id}"
    );
    // Never b.rs's `shared` body, in EITHER lane (the light lane shows no body at all).
    assert!(
        !out_id.contains("marker_in_b"),
        "only a.rs's `shared` body is shown, never b.rs's; got:\n{out_id}"
    );
    #[cfg(feature = "symbols")]
    {
        assert!(
            out_id.contains("marker_in_a"),
            "the body read from the working tree at a.rs:3 is shown; got:\n{out_id}"
        );
        assert!(
            out_id
                .lines()
                .any(|l| l.contains("fn shared") && l.contains('3')),
            "the body is line-numbered from its recorded line (3); got:\n{out_id}"
        );
    }
    #[cfg(not(feature = "symbols"))]
    assert_light_lane_extent_note(&out_id);

    // (3) AMBIGUOUS NAME: `shared` has two definitions -> LIST both (sorted by id, each with its
    //     file) and print NO body (never guess among candidates).
    let (out_amb, err, ok) = run_rigger(root, &["graph", "--show", "shared"]);
    assert!(ok, "graph --show shared must succeed; stderr: {err}");
    assert!(
        out_amb.contains("a.rs::shared") && out_amb.contains("b.rs::shared"),
        "both candidates are listed by id; got:\n{out_amb}"
    );
    let ia = out_amb
        .find("a.rs::shared")
        .expect("a.rs::shared candidate present");
    let ib = out_amb
        .find("b.rs::shared")
        .expect("b.rs::shared candidate present");
    assert!(
        ia < ib,
        "candidates are listed SORTED by id (a.rs before b.rs); got:\n{out_amb}"
    );
    assert!(
        !out_amb.contains("marker_in_a") && !out_amb.contains("marker_in_b"),
        "an ambiguous name prints NO body (the honesty rule); got:\n{out_amb}"
    );
}

/// Spec 119, A BARE SHOW FINDS A LEAF OR PATH: GIVEN a project graph whose `a.rs` holds the
/// methods `T::new` and `U::new` and whose `b.rs` holds a `T::new`, WHEN the operator runs `rigger
/// graph --show` on the flat `a.rs::new` and on the path `T::new`, THEN each lists every
/// definition it names, sorted, and prints no body: the flat id `a.rs`'s two, never `b.rs`'s, and
/// the path both `T::new`, never `U::new`.
#[test]
fn graph_show_lists_every_definition_a_flat_id_or_a_path_names() {
    let dir = temp_project();
    let root = dir.path();
    seed_rigger_dir(root);
    fold_path_def_rows(
        &open_graph(root),
        &[T_NEW, U_NEW, ("b.rs", "new", "T::new", "method")],
    );

    assert_eq!(
        [
            run_rigger_ok(root, &["graph", "--show", "a.rs::new"]),
            run_rigger_ok(root, &["graph", "--show", "T::new"]),
        ],
        [
            ambiguous_show("a.rs::new", A_RS_NEWS),
            ambiguous_show(
                "T::new",
                &[("a.rs::T::new", "a.rs"), ("b.rs::T::new", "b.rs")]
            ),
        ],
        "a flat id lists its file's definitions of its leaf, a path every definition ending in it"
    );
}

/// Spec 119, A BARE SHOW FINDS A LEAF OR PATH: GIVEN a project graph whose `a.rs` holds the method
/// `T::new` and a decision holding the pre-spec flat id `a.rs::new`, WHEN the operator runs `rigger
/// graph --show a.rs::new`, THEN it shows `a.rs::T::new` at its site, never the decision's bare
/// node.
#[test]
fn graph_show_answers_a_flat_id_a_decision_holds_by_its_definition() {
    let dir = held_flat_id_project(&[T_NEW]);
    let root = dir.path();

    assert_eq!(
        show_head(root, "a.rs::new"),
        [
            "show a.rs::T::new",
            "  site: a.rs:1   kind method   degree 1"
        ],
        "a held flat id shows the definition its file holds at its leaf"
    );
}

/// Spec 117, UNRESOLVED REFERENCES ARE A READ: GIVEN a project graph whose `a.rs` holds the
/// methods `T::new` and `U::new` and a decision holding the pre-spec flat id `a.rs::new`, WHEN the
/// operator runs `rigger graph --around a.rs::new --depth 1`, a node whose show lists both
/// methods, THEN the held node's line prints with no `unresolved` line after it: a query whose
/// site is ambiguous lists no unresolved leaf.
#[test]
fn graph_around_an_ambiguous_flat_id_prints_its_node_and_no_unresolved_line() {
    let dir = held_flat_id_project(&[T_NEW, U_NEW]);
    assert_eq!(
        run_around(dir.path(), FLAT_LEAF_ID, 1).0,
        "subgraph around \"a.rs::new\" (depth 1):\n\
         \x20 node a.rs::new                artifact\n\
         \n\
         \x20 1 governing decision(s)/finding(s) (newest 1 shown):\n\
         \x20 node hold@3                   decision\n",
        "the ambiguous node prints its line and no unresolved line"
    );
}

/// A [`temp_project`] whose own graph folds `defs` from position 1 ([`fold_path_def_rows`]) and
/// then a decision holding the pre-spec flat id [`FLAT_LEAF_ID`] ([`hold`]).
fn held_flat_id_project(defs: &[PathDef]) -> tempfile::TempDir {
    let dir = temp_project();
    seed_rigger_dir(dir.path());
    let p = open_graph(dir.path());
    let next = fold_path_def_rows(&p, defs);
    hold(&p, next, &[FLAT_LEAF_ID]);
    dir
}
