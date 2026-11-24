//! Periphery (integration) tests for spec 117, GO STAGES ITS ROW, across the seam the grounder
//! crate's `tests/facts_go.rs` stops short of: that suite pins the mentions the bundled Go grammar
//! stages, a selector not called among them as a `field` mention; this one pins that the real
//! `rigger graph build` lands such a mention in the graph, through the resolver's field rule:
//!
//!  - an uncalled `x.f` lands as one `REFERENCES` edge onto the one field `f` names, `inferred`
//!    though that field is in the same file, while a called `x.g()` stays the call's two edges
//!    onto its method, and an uncalled selector whose field nothing defines lands no edge and no
//!    placeholder.
//!
//! Every test extracts real source, so the file runs on the `symbols` lane alone.

use crate::common;

/// A Go file defining a struct `T` with one field `f` (line 3) and a method `T::g` (line 5), and
/// a function `Move` (lines 7-10) taking a `T`, assigning `x.f` the call `x.g()` and reading
/// `os.Args`, a selector whose field `Args` nothing defines.
const MOVE_SOURCE: &str = "package p\n\ntype T struct{ f int }\n\nfunc (t T) g() int { return 0 }\n\nfunc Move(x T) {\n\tx.f = x.g()\n\t_ = os.Args\n}\n";

/// GIVEN a project whose one Go file ([`MOVE_SOURCE`]) defines a struct `T` with a field `f` and
/// a method `g`, and in `Move` assigns `x.f` the call `x.g()` and reads `os.Args`,
/// WHEN the operator runs `rigger graph build`,
/// THEN the graph's `REFERENCES` and `CALLS` edges are exactly: one `REFERENCES` edge from `Move`
/// onto the field `T.f`, `inferred` though the field is in `Move`'s own file, as an unqualified
/// field use never prefers its file; the call's two onto `T::g`, `extracted`, the called selector
/// staying a call; and one `REFERENCES` edge onto `T` from each of `Move` and `T::g`, whose
/// parameter and receiver name it, `extracted`. `os.Args` lands no edge, and the code entities are
/// the definitions alone, no placeholder standing for `Args`.
#[test]
fn graph_build_lands_an_uncalled_go_selector_on_its_one_field() {
    use rigger::contextgraph::query::entity_id;
    use rigger::contextgraph::{TIER_EXTRACTED, TIER_INFERRED};

    let file = "f.go";
    let (_project, graph) = common::cli::built_graph(&[(file, MOVE_SOURCE)]);

    let id = |path| entity_id(file, path);
    let mut edges = common::fixtures::a_call_from(file, "Move", &id("T::g"), TIER_EXTRACTED);
    edges.extend([
        common::fixtures::reference(&id("Move"), &id("T.f"), TIER_INFERRED),
        common::fixtures::reference(&id("Move"), &id("T"), TIER_EXTRACTED),
        common::fixtures::reference(&id("T::g"), &id("T"), TIER_EXTRACTED),
    ]);
    edges.sort();
    assert_eq!(
        common::fixtures::landed(&graph),
        edges,
        "the uncalled `x.f` lands on its one field, inferred; the called `x.g()` as a call; \
         `os.Args` nowhere"
    );
    assert_eq!(
        common::fixtures::live_file_and_entity_ids(&graph.whole().unwrap()),
        [
            vec![file.to_string()],
            ["Move", "T", "T.f", "T::g"].map(id).to_vec(),
        ],
        "no placeholder stands for `Args`, which nothing defines"
    );
}
