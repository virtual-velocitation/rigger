//! Periphery (API) tests for spec 119 criterion 2, THE EDGE SIDE BINDS THE LEAF: every edge-side
//! reader binds its seed's leaf and matches an edge's target text against the leaf and the
//! sigil-prefixed leaf. The crate's own tests write a nested definition and its edges straight
//! into the store; these run OUTSIDE the crate over the public projection, `Projector::apply`
//! folding the on-log events and `Projector::calls` walking UP, so a target spelled with the
//! placeholder sigil, as an event names it, reaches every edge-side reader:
//!
//!  - the callers walk, from a definition to its callers through `<file>::<leaf>` and
//!    `<file>::%<leaf>`, a C# `operator %` seed among them;
//!  - the reference tier, upgraded when a definition of the leaf folds and demoted when the next
//!    generation of its file drops it;
//!  - the referenced-but-not-called sidecar, through both spellings, and a seed with no leaf.
//!
//! No grammar yields a nested id at this tree and no payload field carries a path, so a nested
//! definition stays the crate's tests'; the sigil enters as the name an event names. The reference
//! arm tiers a reference by the name its event carries (its target choice is criterion 3's), so
//! each walk folds its calls before the definition, whose tier upgrade binds the leaf and lifts
//! them to the walk's `inferred` floor. Every test runs on both lanes.

mod common;

use common::fixtures::{
    apply_call, apply_code_entity, apply_def, apply_keyed, apply_ref, call_frontiers,
    call_node_ids, call_walk, def_json, edge_tier, referenced_not_called_ids,
};
use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{
    Direction, REL_REFERENCES, TIER_AMBIGUOUS, TIER_INFERRED, TYPE_CODE_ENTITY_EXTRACTED,
};

/// The definition every walk below starts from.
const SEED: &str = "src/a.rs::new";

/// Fold, at log position `pos`, the generation `generation` of `src/a.rs`, which defines `name`
/// alone, valid from `secs`: the replay-keyed batch a re-extraction appends.
fn fold_generation_of_a(p: &Projector, pos: u64, generation: &str, name: &str, secs: u64) {
    let key = format!("gc/src/a.rs@{generation}#0");
    let def = def_json("src/a.rs", name, 1, true);
    apply_keyed(p, pos, TYPE_CODE_ENTITY_EXTRACTED, def, &key, secs);
}

/// GIVEN `src/b.rs` calls `new` from `run` as `new` and from `go` as `%new`, and `src/c.rs` calls
/// `%%new` from `far`, before `src/a.rs` defines `new`, WHEN the callers walk ascends from
/// `src/a.rs::new`, THEN it reaches `run` and `go` as unambiguous callers and never `far`, whose
/// leaf is `%new`.
#[test]
fn the_callers_walk_reaches_a_caller_through_the_leaf_and_the_sigiled_leaf() {
    let p = Projector::open(":memory:", "test").unwrap();
    apply_def(&p, 1, "src/b.rs", "run", 1, true);
    apply_def(&p, 2, "src/b.rs", "go", 2, false);
    apply_def(&p, 3, "src/c.rs", "far", 1, true);
    apply_call(&p, 4, "src/b.rs", "new", "run");
    apply_call(&p, 5, "src/b.rs", "%new", "go");
    apply_call(&p, 6, "src/c.rs", "%%new", "far");
    apply_def(&p, 7, "src/a.rs", "new", 1, true);
    let up = call_walk(&p, SEED, Direction::Up);
    assert_eq!(
        call_node_ids(&up),
        [SEED, "src/b.rs::go", "src/b.rs::run"],
        "the walk reaches the caller of `new` and of `%new`, never of `%%new`"
    );
    assert_eq!(
        call_frontiers(&up),
        Vec::new(),
        "one definition is no frontier"
    );
}

/// GIVEN `src/b.cs` calls the C# `operator %` from `Run` as `%%` and from `Go` as `%`, before
/// `src/a.cs` defines it, its leaf `%`, WHEN the callers walk ascends from `src/a.cs::%`, THEN it
/// reaches both callers.
#[test]
fn a_csharp_operator_percent_seed_reaches_its_double_percent_caller() {
    let p = Projector::open(":memory:", "test").unwrap();
    apply_code_entity(&p, 1, "src/b.cs", "Run", "method", 1, "csharp");
    apply_code_entity(&p, 2, "src/b.cs", "Go", "method", 2, "csharp");
    apply_call(&p, 3, "src/b.cs", "%%", "Run");
    apply_call(&p, 4, "src/b.cs", "%", "Go");
    apply_code_entity(&p, 5, "src/a.cs", "%", "method", 1, "csharp");
    let up = call_walk(&p, "src/a.cs::%", Direction::Up);
    assert_eq!(
        call_node_ids(&up),
        ["src/a.cs::%", "src/b.cs::Go", "src/b.cs::Run"],
        "the operator's leaf `%` binds the caller's `%%` and its `%`"
    );
    assert_eq!(
        call_frontiers(&up),
        Vec::new(),
        "one definition is no frontier"
    );
}

/// GIVEN `src/b.rs` references `%new` and `src/c.rs` references `%%new`, both before anything
/// defines `new`, WHEN a generation of `src/a.rs` defining `new` folds and the next one, defining
/// `other`, replaces it, THEN `src/b.rs`'s reference goes ambiguous, inferred, ambiguous again,
/// and `src/c.rs`'s, whose leaf is `%new`, stays ambiguous throughout.
#[test]
fn a_sigiled_reference_follows_a_definition_of_its_leaf_in_and_out() {
    let p = Projector::open(":memory:", "test").unwrap();
    apply_ref(&p, 1, "src/b.rs", "%new");
    apply_ref(&p, 2, "src/c.rs", "%%new");
    let tiers = |p: &Projector| {
        let g = p.whole().unwrap();
        [
            edge_tier(&g, REL_REFERENCES, "src/b.rs::%new"),
            edge_tier(&g, REL_REFERENCES, "src/c.rs::%%new"),
        ]
    };
    assert_eq!(
        tiers(&p),
        [TIER_AMBIGUOUS, TIER_AMBIGUOUS],
        "nothing defines `new` yet"
    );
    fold_generation_of_a(&p, 3, "h1", "new", 10);
    assert_eq!(
        tiers(&p),
        [TIER_INFERRED, TIER_AMBIGUOUS],
        "a definition of `new` upgrades the `%new` reference and not the `%%new` one"
    );
    fold_generation_of_a(&p, 4, "h2", "other", 20);
    assert_eq!(
        tiers(&p),
        [TIER_AMBIGUOUS, TIER_AMBIGUOUS],
        "the generation dropping `new` demotes the `%new` reference"
    );
}

/// GIVEN `src/a.rs` defines `new`, `src/d.rs` references it as `%new`, `src/e.rs` as `new` and
/// `src/f.rs` as `%%new`, and `src/g.rs` references and calls `%new` from `run`, WHEN the UP view
/// is built from `src/a.rs::new`, THEN `src/d.rs` and `src/e.rs` are referenced but not called,
/// `src/f.rs` binds no leaf of the seed and `src/g.rs` calls it.
#[test]
fn referenced_not_called_counts_a_file_level_reference_through_both_spellings() {
    let p = Projector::open(":memory:", "test").unwrap();
    apply_def(&p, 1, "src/a.rs", "new", 1, true);
    apply_def(&p, 2, "src/g.rs", "run", 1, true);
    apply_ref(&p, 3, "src/d.rs", "%new");
    apply_ref(&p, 4, "src/e.rs", "new");
    apply_ref(&p, 5, "src/f.rs", "%%new");
    apply_call(&p, 6, "src/g.rs", "%new", "run");
    assert_eq!(
        referenced_not_called_ids(&p, SEED),
        ["src/d.rs", "src/e.rs"],
        "both spellings count as a reference, and as a call"
    );
}

/// GIVEN the file node `src/x.rs`, which has no `name` and no `::` in its id, and `src/z.rs`
/// referencing the text `src/x.rs`, WHEN the UP view is built from the file node, THEN it lists
/// nothing referenced: a seed with no leaf names nothing, never its whole id.
#[test]
fn a_seed_with_no_leaf_names_nothing_referenced() {
    let p = Projector::open(":memory:", "test").unwrap();
    apply_def(&p, 1, "src/x.rs", "f", 1, true);
    apply_ref(&p, 2, "src/z.rs", "src/x.rs");
    assert_eq!(
        p.whole()
            .unwrap()
            .nodes
            .iter()
            .filter(|n| n.id == "src/x.rs")
            .map(|n| n.attrs.get("name").cloned())
            .collect::<Vec<_>>(),
        [None],
        "the seed is a node with no name"
    );
    assert_eq!(
        referenced_not_called_ids(&p, "src/x.rs"),
        Vec::<String>::new(),
        "a leafless seed names nothing"
    );
}
