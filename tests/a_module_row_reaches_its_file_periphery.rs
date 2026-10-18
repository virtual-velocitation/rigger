//! Periphery (CLI / end-to-end) tests for spec 117, A MODULE ROW REACHES ITS FILE, over the built
//! binary. The crates' own tests pin the module row's emit, its settlement over hand-built rows and
//! the three consumers that skip its edge one module at a time; these run OUTSIDE the crates, over
//! a real tree `rigger graph build` walks, and guard the boundary a `mod x;` crosses on its way to
//! an operator:
//!
//!  - the graph `rigger graph build` folds: a `mod x;` lands one `extracted` `CONTAINS` from its
//!    `mod` node onto the first of `x.rs` and `x/mod.rs` the tree holds, none while it holds
//!    neither and no file node for either, the edge moving to `x.rs` when a later build adds it,
//!    retiring with the declaration and landing again on its return;
//!  - `rigger graph --around` on the `mod` node prints that edge;
//!  - `rigger graph communities` couples no `mod` node to its module's file, so a module file whose
//!    one structural edge is its declaration's joins no community.

mod common;
#[path = "common/symbol_index.rs"]
mod symbol_index;

#[cfg(feature = "symbols")]
use common::fixtures::{
    declared, module_edges, DECLARING_FILE, DIR_MODULE_FILE, MODULE_FILE, MOD_NODE,
};
#[cfg(feature = "symbols")]
use symbol_index::{FN_Y, MOD_X};

/// GIVEN a project whose `src/lib.rs` declares `mod x;` and which holds `src/x.rs`, `src/x/mod.rs`
/// or both, each defining `fn y`,
/// WHEN the operator runs `rigger graph build`,
/// THEN the graph's one module edge is an `extracted` `CONTAINS` from `src/lib.rs::x` onto
/// `src/x.rs` when the tree holds it, else onto `src/x/mod.rs`.
#[cfg(feature = "symbols")]
#[test]
fn graph_build_lands_a_mod_declaration_on_the_first_candidate_the_tree_holds() {
    let cases: [(&[&str], &str); 3] = [
        (&[MODULE_FILE], MODULE_FILE),
        (&[DIR_MODULE_FILE], DIR_MODULE_FILE),
        (&[MODULE_FILE, DIR_MODULE_FILE], MODULE_FILE),
    ];
    for (held, reached) in cases {
        let tree: Vec<(&str, &str)> = [(DECLARING_FILE, MOD_X)]
            .into_iter()
            .chain(held.iter().map(|file| (*file, FN_Y)))
            .collect();
        let (_project, graph) = common::cli::built_graph(&tree);
        assert_eq!(module_edges(&graph), declared(reached), "{held:?}");
    }
}

/// GIVEN a project whose `src/lib.rs` declares `mod x;` and which holds neither candidate file,
/// WHEN the operator runs `rigger graph build`, then adds `src/x/mod.rs` and rebuilds, then adds
/// `src/x.rs` and rebuilds, then rewrites `src/lib.rs` to declare no module and rebuilds, then
/// restores the declaration and rebuilds,
/// THEN the first build lands no module edge and writes no node for either candidate, the second
/// lands the edge onto `src/x/mod.rs`, the third moves it onto `src/x.rs`, the fourth retires it
/// and the fifth lands it onto `src/x.rs` again.
#[cfg(feature = "symbols")]
#[test]
fn graph_rebuild_settles_a_mod_declaration_as_its_candidates_and_its_declaration_come_and_go() {
    let (project, graph) = common::cli::built_graph(&[(DECLARING_FILE, MOD_X)]);
    assert_eq!(module_edges(&graph), [], "no candidate holds a file node");
    assert_eq!(
        common::fixtures::live_file_and_entity_ids(&graph.whole().unwrap()),
        common::fixtures::file_and_entity_ids(&[DECLARING_FILE], &[MOD_NODE]),
        "an unresolved module row writes no node"
    );
    drop(graph);

    let steps = [
        ((DIR_MODULE_FILE, FN_Y), declared(DIR_MODULE_FILE)),
        ((MODULE_FILE, FN_Y), declared(MODULE_FILE)),
        ((DECLARING_FILE, FN_Y), vec![]),
        ((DECLARING_FILE, MOD_X), declared(MODULE_FILE)),
    ];
    for (written, edges) in steps {
        let graph = common::cli::rebuilt_graph(project.path(), &[written]);
        assert_eq!(module_edges(&graph), edges, "after writing {}", written.0);
    }
}

/// GIVEN the built project whose `src/lib.rs` declares `mod x;` and whose `src/x.rs` defines
/// `fn y`,
/// WHEN the operator runs `rigger graph --around src/lib.rs::x --depth 1`,
/// THEN the subgraph is the declaring file, the `mod` node and its module file, joined by the
/// declaring file's `CONTAINS` onto the `mod` node and the `mod` node's onto its file.
#[cfg(feature = "symbols")]
#[test]
fn graph_around_a_mod_node_prints_its_contains_onto_its_module_file() {
    let (project, _graph) =
        common::cli::built_graph(&[(DECLARING_FILE, MOD_X), (MODULE_FILE, FN_Y)]);
    assert_eq!(
        common::cli::run_around(project.path(), MOD_NODE, 1).0,
        "subgraph around \"src/lib.rs::x\" (depth 1):\n\
         \x20 node src/lib.rs               file\n\
         \x20 node src/lib.rs::x            code-entity\n\
         \x20 node src/x.rs                 file\n\
         \x20 edge src/lib.rs -CONTAINS-> src/lib.rs::x\n\
         \x20 edge src/lib.rs::x -CONTAINS-> src/x.rs\n"
    );
}

/// GIVEN the built project whose `src/lib.rs` declares `mod x;` and whose `src/x.rs` holds one
/// `use` and nothing else, so its one structural edge is the declaration's `CONTAINS`,
/// WHEN the operator runs `rigger graph communities`,
/// THEN detection couples the two nodes of `src/lib.rs` alone: one community over two coupled
/// nodes, `src/x.rs` none of them.
#[cfg(feature = "symbols")]
#[test]
fn graph_communities_couples_no_mod_node_to_its_module_file() {
    let (project, _graph) = common::cli::built_graph(&[
        (DECLARING_FILE, MOD_X),
        (MODULE_FILE, symbol_index::USE_ONLY_SOURCE),
    ]);
    assert_eq!(
        common::cli::run_rigger_ok(project.path(), &["graph", "communities"]),
        "graph communities: detected 1 community over 2 coupled node(s) at resolution 1 \
         (2 membership event(s) recorded into .rigger/graph.db)\n"
    );
}
