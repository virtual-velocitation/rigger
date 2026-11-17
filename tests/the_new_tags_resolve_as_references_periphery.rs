//! Periphery (API / integration) tests for spec 117, THE NEW TAGS RESOLVE AS REFERENCES: a
//! mention tagged `impl` resolves onto a type or a trait and one tagged `variant` onto a variant,
//! each a `REFERENCES` link and never a `CALLS`. The resolver's own test reads its links from
//! inside the crate; these plant the tagged facts as an extractor would read them and run OUTSIDE
//! the crates over the code store's composed view, guarding the readers the links reach:
//!
//!  - the whole graph, holding a reference onto a trait, a type and a nested variant, and no call;
//!  - the callers walk, which finds no caller of any of them;
//!  - the referenced-but-not-called sidecar, which names the referring file for each.
//!
//! Every test runs on both lanes.

use crate::common;

use common::fixtures::{
    call_edge_pairs, call_node_ids, call_walk, define_member, facts_view, fold_events, landed,
    planted_file, reference, referenced_not_called_ids, view_graph, with_tagged,
};
use rigger::code_view::FactsView;
use rigger::contextgraph::{
    Direction, CODE_KIND_CONSTANT, CODE_KIND_FUNCTION, CODE_KIND_IMPL, CODE_KIND_TRAIT,
    CODE_KIND_TYPE, CODE_KIND_VARIANT, REF_KIND_IMPL, REF_KIND_VARIANT, TIER_INFERRED,
};

/// The trait an `impl` mention names.
const TRAIT: &str = "src/view.rs::CodeView";
/// The type an `impl` mention names, whose leaf the referring file's `impl` block shares.
const TYPE: &str = "src/view.rs::Located";
/// The variant a `variant` mention names, nested in [`TYPE`].
const VARIANT: &str = "src/view.rs::Located::Many";
/// The file holding every tagged mention.
const REFERRING_FILE: &str = "src/facts.rs";

/// The code store's view of a tree where `src/view.rs` defines the trait `CodeView` and the type
/// `Located` holding the variant `Many`; [`REFERRING_FILE`] holds the `impl` block `Located`,
/// whose header names `CodeView` and `Located`, and the function `locate`, which names the variant
/// `Many`; and `src/decoys.rs` defines a function `CodeView` and a constant `Many`, kinds neither
/// tag names a candidate of.
fn tagged_tree() -> FactsView {
    let mut view = planted_file(
        "src/view.rs",
        &[("CodeView", CODE_KIND_TRAIT), ("Located", CODE_KIND_TYPE)],
        &[],
        &[],
    );
    define_member(&mut view, "Located", "Many", CODE_KIND_VARIANT, 3);
    let facts = planted_file(
        REFERRING_FILE,
        &[("Located", CODE_KIND_IMPL), ("locate", CODE_KIND_FUNCTION)],
        &[],
        &[],
    );
    let facts = with_tagged(
        facts,
        REF_KIND_IMPL,
        &[("Located", "CodeView"), ("Located", "Located")],
    );
    let facts = with_tagged(facts, REF_KIND_VARIANT, &[("locate", "Many")]);
    let decoys = planted_file(
        "src/decoys.rs",
        &[
            ("CodeView", CODE_KIND_FUNCTION),
            ("Many", CODE_KIND_CONSTANT),
        ],
        &[],
        &[],
    );
    facts_view(vec![view, facts, decoys])
}

/// GIVEN the tree of [`tagged_tree`] and no records, WHEN the composed graph serves its whole
/// edge set, THEN the `impl` block references the trait and the type and `locate` references the
/// nested variant, each inferred onto the one candidate of its tag's kinds, never onto the
/// same-leaf function, constant or `impl` block, and no edge is a call.
#[test]
fn an_impl_header_and_a_variant_mention_land_as_references_and_never_as_calls() {
    let records = fold_events([]);
    let view = tagged_tree();
    let impl_block = "src/facts.rs::Located";
    assert_eq!(
        landed(&view_graph(&records, &view)),
        [
            reference(impl_block, TRAIT, TIER_INFERRED),
            reference(impl_block, TYPE, TIER_INFERRED),
            reference("src/facts.rs::locate", VARIANT, TIER_INFERRED),
        ],
        "each tagged mention lands one reference on its trait, type or variant, and none a call"
    );
}

/// GIVEN the tree of [`tagged_tree`] and no records, WHEN the callers walk ascends from the
/// trait, the type and the nested variant, THEN each walk holds its seed alone with no call edge,
/// and the referenced-but-not-called sidecar names [`REFERRING_FILE`], whose references reach
/// each of them.
#[test]
fn each_referenced_definition_has_no_caller_and_its_referring_file_is_referenced_not_called() {
    let records = fold_events([]);
    let view = tagged_tree();
    let graph = view_graph(&records, &view);
    for seed in [TRAIT, TYPE, VARIANT] {
        let up = call_walk(&graph, seed, Direction::Up);
        assert_eq!(
            (
                call_node_ids(&up),
                call_edge_pairs(&up),
                referenced_not_called_ids(&graph, seed)
            ),
            (
                vec![seed.to_string()],
                Vec::new(),
                vec![REFERRING_FILE.to_string()]
            ),
            "{seed}: no caller reaches it, and the file referencing it is referenced, not calling"
        );
    }
}
