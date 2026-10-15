//! Periphery (integration) tests for spec 119 criterion 15, A RENAMED PATH RETIRES ITS OLD LEAF:
//! a definition refolded at its id path under a different leaf retires what resolved against the
//! old leaf, as a cold rebuild folds it. The crate's own tests fold one event to a transaction
//! and model the rebuild as a fresh projector folding the next generation alone; these run
//! OUTSIDE the crate over one hand-built log and guard the seams the rule crosses:
//!
//!  - the live fold, one event to a transaction, against the production cold rebuild,
//!    `Projector::rebuild` streaming the same log from its start into a fresh graph file: both
//!    end with the old leaf's cross-file reference and call ambiguous and its test reference's
//!    proof off the node;
//!  - a differing-name pair at one id path folded as ONE batch, the one transaction a file's
//!    extraction batch folds in, live and in a rebuild folding the whole log as one batch.
//!
//! A tree yields one id path under two leaves as a twin, a TypeScript `namespace A.B` (leaf
//! `A.B`) beside `class A { B }` (leaf `B`), which shares the path until criterion 10's dedup
//! keeps one, and across two generations of a file, the namespace alone and then the class
//! alone. The log is hand-built, as the criterion names, so each case folds exactly the leaves,
//! kinds and order it pins; every test runs on both lanes.

mod common;

use std::path::Path;

use common::fixtures::{
    a_call_from, attrs_of, cross_file_call_and_proof, definition, ev_at, f_rs_batch_def, folds,
    landed, proven_by_attr, rebuild_from_the_log, DefRow, NEW_LEAF_MODULE, OLD_LEAF_FN,
    RENAMED_NODE, RENAMED_PATH,
};
use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{TIER_AMBIGUOUS, TIER_INFERRED};
use rigger::eventstore::Event;

/// `def` at [`RENAMED_PATH`] in `src/f.rs`, at log position `pos` and valid from `pos`, as event
/// `index` of generation `generation`'s batch.
fn at_the_path(pos: u64, generation: &str, index: usize, def: DefRow<'static>) -> Event {
    f_rs_batch_def(pos, generation, index, def, Some(RENAMED_PATH), pos)
}

/// From log position `pos`, `src/b.rs`'s fn `g`, the call it makes to the old leaf `B` and
/// `tests/t.rs`'s test reference to it ([`cross_file_call_and_proof`]).
fn the_references_from(pos: u64) -> Vec<Event> {
    cross_file_call_and_proof(OLD_LEAF_FN.0)
        .into_iter()
        .zip(pos..)
        .map(|((type_, json), at)| ev_at(at, type_, json))
        .collect()
}

/// The reference and the call `src/b.rs` makes to `B`, landed on its placeholder `src/b.rs::%B`
/// at `tier`, as [`landed`] answers them: the test reference lands as a proof on the definition it
/// names, never an edge.
fn the_references_at(tier: &str) -> Vec<(String, String, String, String)> {
    a_call_from("src/b.rs", "g", "src/b.rs::%B", tier)
}

/// The graph file `name` under `dir`, opened under the project `test` the rebuild folds into.
fn graph_file(dir: &Path, name: &str) -> Projector {
    Projector::open(dir.join(name).to_str().unwrap(), "test").unwrap()
}

/// `p` holds [`RENAMED_NODE`] carrying exactly the attrs of the definition `settled` - no
/// `proven_by` - and the reference and the call to `B` ambiguous: what a graph holding no call
/// target of the leaf `B` gives.
fn the_old_leaf_resolves_onto_nothing(p: &Projector, settled: DefRow, why: &str) {
    let (name, kind, line) = settled;
    assert_eq!(
        attrs_of(p, RENAMED_NODE),
        definition(name, kind, line),
        "{why}"
    );
    assert_eq!(landed(p), the_references_at(TIER_AMBIGUOUS), "{why}");
}

/// GIVEN the fn `B` at the id path `A.B` of `src/f.rs`, called across files by `src/b.rs`'s `g`
/// and proven by a test reference in `tests/t.rs`, WHEN generation 2's batch head refolds the
/// node as the module `A.B`, THEN the node no longer defines `B`: the reference and the call turn
/// ambiguous and the test reference's proof leaves the node, in the live fold and in a cold
/// rebuild of the same log.
#[test]
fn a_fn_refolded_as_another_leaf_at_its_path_retires_the_old_leaf_live_and_in_a_cold_rebuild() {
    let mut log = vec![at_the_path(1, "h1", 0, OLD_LEAF_FN)];
    log.extend(the_references_from(2));
    log.push(at_the_path(5, "h2", 0, NEW_LEAF_MODULE));
    let dir = tempfile::tempdir().unwrap();

    let live = graph_file(dir.path(), "live.db");
    for e in &log[..4] {
        folds(&live, std::slice::from_ref(e));
    }
    assert_eq!(
        landed(&live),
        the_references_at(TIER_INFERRED),
        "premise: the fn `B` infers the reference and the call to its leaf"
    );
    assert_eq!(
        proven_by_attr(&live, RENAMED_NODE).as_deref(),
        Some("1"),
        "premise: the test reference proves the fn `B`"
    );
    folds(&live, std::slice::from_ref(&log[4]));
    the_old_leaf_resolves_onto_nothing(&live, NEW_LEAF_MODULE, "the live fold of generation 2");

    rebuild_from_the_log(&dir.path().join("rebuilt.db"), &log, 1, &mut |_| Ok(None));
    the_old_leaf_resolves_onto_nothing(
        &graph_file(dir.path(), "rebuilt.db"),
        NEW_LEAF_MODULE,
        "a cold rebuild of the log, one event to a batch",
    );
}

/// GIVEN `src/b.rs`'s call to `B` and `tests/t.rs`'s test reference to it, WHEN one batch of
/// `src/f.rs` folds the fn `B` and then the module `A.B` at the one id path `A.B`, in one
/// transaction, THEN the node settles on the fn's kind and line under the last-folded name `A.B`
/// and defines no `B`: the reference and the call to `B` are ambiguous and no proof is on the
/// node, whether the references folded before the batch or after it, and in a rebuild folding the
/// whole log as one batch.
#[test]
fn a_differing_name_pair_in_one_batch_retires_the_first_leaf_live_and_in_a_cold_rebuild() {
    let pair = |pos: u64| {
        vec![
            at_the_path(pos, "h1", 0, OLD_LEAF_FN),
            at_the_path(pos + 1, "h1", 1, NEW_LEAF_MODULE),
        ]
    };
    let settled = (NEW_LEAF_MODULE.0, OLD_LEAF_FN.1, OLD_LEAF_FN.2);
    let dir = tempfile::tempdir().unwrap();

    let before = graph_file(dir.path(), "before.db");
    folds(&before, &the_references_from(1));
    folds(&before, &pair(4));
    the_old_leaf_resolves_onto_nothing(&before, settled, "the references, then the one-batch pair");

    let mut log = pair(1);
    log.extend(the_references_from(3));
    let after = graph_file(dir.path(), "after.db");
    folds(&after, &log[..2]);
    folds(&after, &log[2..]);
    the_old_leaf_resolves_onto_nothing(&after, settled, "the one-batch pair, then the references");

    rebuild_from_the_log(&dir.path().join("rebuilt.db"), &log, log.len(), &mut |_| {
        Ok(None)
    });
    the_old_leaf_resolves_onto_nothing(
        &graph_file(dir.path(), "rebuilt.db"),
        settled,
        "a cold rebuild of the log as one batch",
    );
}
