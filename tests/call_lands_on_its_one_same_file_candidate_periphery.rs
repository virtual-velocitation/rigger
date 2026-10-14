//! Periphery (integration) tests for spec 119, A CALL LANDS ON ITS ONE SAME-FILE CANDIDATE,
//! across the seam the crate's own tests fold by hand: the real `rigger graph build` extracts a
//! file's generations, retires the definition its next generation dropped, and folds that
//! generation's call, so where the call lands rests on what the live-assertion candidate rule
//! reads from what the real pipeline leaves behind, a decision still holding the dropped
//! definition's node included. The crate's tests pin the rule over hand-built events; these pin
//! that the binary's own extraction and supersession hand it the same facts.
//!
//! Every test extracts real source, so the file runs on the `symbols` lane alone.

#![cfg(feature = "symbols")]

mod common;

use common::cli::{built_graph, rebuilt_graph};
use common::fixtures::{a_call_from_main, hold, landed};
use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{KIND_ARTIFACT, KIND_CODE_ENTITY, TIER_AMBIGUOUS, TIER_EXTRACTED};

/// `f.rs` defining `foo` and calling it from `main`: the first generation.
const DEFINES_AND_CALLS: &str = "fn foo() {}\nfn main() { foo(); }\n";

/// `f.rs` only calling `foo`: a next generation dropping the definition.
const ONLY_CALLS: &str = "fn main() { foo(); }\n";

/// `f.rs` still defining and calling `foo` beside a new `bar`: a next generation keeping it.
const KEEPS_AND_CALLS: &str = "fn foo() {}\nfn bar() {}\nfn main() { foo(); }\n";

/// The `(id, kind)` of the node `f.rs::foo` in `p`, when one stands.
fn foo_node(p: &Projector) -> Vec<(String, String)> {
    p.whole()
        .unwrap()
        .nodes
        .into_iter()
        .filter(|n| n.id == "f.rs::foo")
        .map(|n| (n.id, n.kind))
        .collect()
}

/// GIVEN a built project whose `f.rs` defines `foo` and calls it, the call landing on its one
/// same-file candidate at `extracted`, and `held` naming the nodes a decision then holds, WHEN
/// the operator rewrites `f.rs` to `next` and runs `rigger graph build` again, THEN the call
/// lands on `to` at `tier` and the node `f.rs::foo` stands as each of `foo_kinds`.
fn a_rebuild_lands_the_call(held: &[&str], next: &str, to: &str, tier: &str, foo_kinds: &[&str]) {
    let (dir, graph) = built_graph(&[("f.rs", DEFINES_AND_CALLS)]);
    assert_eq!(
        landed(&graph),
        a_call_from_main("f.rs::foo", TIER_EXTRACTED),
        "while `foo` stands the call lands on its one same-file definition"
    );
    hold(&graph, 1_000_000, held);
    drop(graph);

    let graph = rebuilt_graph(dir.path(), &[("f.rs", next)]);
    assert_eq!(landed(&graph), a_call_from_main(to, tier));
    let expected: Vec<(String, String)> = foo_kinds
        .iter()
        .map(|kind| ("f.rs::foo".to_string(), kind.to_string()))
        .collect();
    assert_eq!(foo_node(&graph), expected);
}

rigger::test_cases! {
    a_rebuild_dropping_a_called_definition_lands_its_call_on_the_placeholder:
        a_rebuild_lands_the_call(&[], ONLY_CALLS, "f.rs::%foo", TIER_AMBIGUOUS, &[]);
    a_rebuild_dropping_a_held_definition_lands_its_call_on_the_placeholder:
        a_rebuild_lands_the_call(
            &["f.rs::foo"],
            ONLY_CALLS,
            "f.rs::%foo",
            TIER_AMBIGUOUS,
            &[KIND_ARTIFACT]
        );
    a_rebuild_keeping_a_held_definition_keeps_its_call_on_it:
        a_rebuild_lands_the_call(
            &["f.rs::foo"],
            KEEPS_AND_CALLS,
            "f.rs::foo",
            TIER_EXTRACTED,
            &[KIND_CODE_ENTITY]
        );
}
