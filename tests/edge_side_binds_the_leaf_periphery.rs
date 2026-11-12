//! Periphery (API) tests for spec 119 criterion 2, THE EDGE SIDE BINDS THE LEAF: every edge-side
//! reader binds its seed's leaf and matches an edge's target text against the leaf and the
//! sigil-prefixed leaf. The crate's own tests write a nested definition and its edges straight
//! into the store; these run OUTSIDE the crate over the code store's view of planted file facts,
//! composed and walked UP, so a target spelled as the resolver spells it reaches every edge-side
//! reader:
//!
//!  - the callers walk, from a definition to its callers through `<file>::%<leaf>`, a C#
//!    `operator %` seed among them, and never through a longer sigiled leaf;
//!  - the reference tier, inferred while a definition of the leaf stands and ambiguous when none
//!    does;
//!  - the referenced-but-not-called sidecar, through both spellings, and a seed with no leaf.
//!
//! A reference with no one call target of its leaf in its own file lands on the placeholder
//! `<file>::%<leaf>` (criterion 3), so a mention naming `new` spells `%new` and one naming `%new`
//! spells `%%new`, the placeholder of the leaf `%new`; the spelling `<file>::<leaf>` is a
//! reference landed on its own file's definition of the leaf. Every test runs on both lanes.

use crate::common;

use common::fixtures::{
    call_frontiers, call_node_ids, call_walk, edge_tier, planted_file, referenced_not_called_ids,
    view_graph,
};
use rigger::code_facts::FileFacts;
use rigger::code_view::FactsView;
use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{
    Direction, Neighbors, CODE_KIND_FUNCTION, REL_REFERENCES, TIER_AMBIGUOUS, TIER_INFERRED,
};

/// The records (none) and the code store's view of a tree holding exactly `files`.
fn tree(files: Vec<FileFacts>) -> (Projector, FactsView) {
    (
        Projector::open(":memory:", "test").unwrap(),
        common::fixtures::facts_view(files),
    )
}

/// The facts of `path` defining each of `defs`, a function, with each `(caller, callee)` of
/// `calls` and each caller-less reference of `refs`.
fn fns(path: &str, defs: &[&str], calls: &[(&str, &str)], refs: &[&str]) -> FileFacts {
    let defs: Vec<(&str, &str)> = defs.iter().map(|d| (*d, CODE_KIND_FUNCTION)).collect();
    planted_file(path, &defs, calls, refs)
}

/// The definition every walk below starts from.
const SEED: &str = "src/a.rs::new";

/// GIVEN `src/b.rs` calls `new` from `run`, landing on `src/b.rs::%new`, and `%new` from `go`,
/// landing on `src/b.rs::%%new`, and `src/c.rs` calls `%%new` from `far`, and `src/a.rs` defines
/// `new`, WHEN the callers walk ascends from `src/a.rs::new`, THEN it reaches `run` as an
/// unambiguous caller and never `go` or `far`, whose leaves are `%new` and `%%new`.
#[test]
fn the_callers_walk_reaches_a_caller_through_the_sigiled_leaf_and_never_a_longer_one() {
    let (records, view) = tree(vec![
        fns(
            "src/b.rs",
            &["run", "go"],
            &[("run", "new"), ("go", "%new")],
            &[],
        ),
        fns("src/c.rs", &["far"], &[("far", "%%new")], &[]),
        fns("src/a.rs", &["new"], &[], &[]),
    ]);
    let up = call_walk(&view_graph(&records, &view), SEED, Direction::Up);
    assert_eq!(
        call_node_ids(&up),
        [SEED, "src/b.rs::run"],
        "the walk reaches the caller of `new`, never of `%new` or `%%new`"
    );
    assert_eq!(
        call_frontiers(&up),
        Vec::new(),
        "one definition is no frontier"
    );
}

/// GIVEN `src/b.cs` calls the C# `operator %` from `Go` as `%`, landing on its placeholder
/// `src/b.cs::%%`, and `%%` from `Run`, landing on `src/b.cs::%%%`, and `src/a.cs` defines it, its
/// leaf `%`, WHEN the callers walk ascends from `src/a.cs::%`, THEN it reaches `Go` alone.
#[test]
fn a_csharp_operator_percent_seed_reaches_its_double_percent_caller() {
    let csharp = |path: &str, defs: &[&str], calls: &[(&str, &str)]| {
        let defs: Vec<(&str, &str)> = defs.iter().map(|d| (*d, "method")).collect();
        FileFacts {
            lang: "csharp".to_string(),
            ..planted_file(path, &defs, calls, &[])
        }
    };
    let (records, view) = tree(vec![
        csharp("src/b.cs", &["Run", "Go"], &[("Run", "%%"), ("Go", "%")]),
        csharp("src/a.cs", &["%"], &[]),
    ]);
    let up = call_walk(&view_graph(&records, &view), "src/a.cs::%", Direction::Up);
    assert_eq!(
        call_node_ids(&up),
        ["src/a.cs::%", "src/b.cs::Go"],
        "the operator's leaf `%` binds the caller's `%%`, never `%%%`"
    );
    assert_eq!(
        call_frontiers(&up),
        Vec::new(),
        "one definition is no frontier"
    );
}

/// GIVEN `src/b.rs` references `new`, landing on `src/b.rs::%new`, and `src/c.rs` references
/// `%new`, landing on `src/c.rs::%%new`, WHEN `src/a.rs` defines `other`, then `new`, then
/// `other` again, THEN `src/b.rs`'s reference is ambiguous, inferred, ambiguous again, and
/// `src/c.rs`'s, whose leaf is `%new`, stays ambiguous throughout.
#[test]
fn a_sigiled_reference_follows_a_definition_of_its_leaf_in_and_out() {
    let tiers = |defined: &str| {
        let (records, view) = tree(vec![
            fns("src/b.rs", &[], &[], &["new"]),
            fns("src/c.rs", &[], &[], &["%new"]),
            fns("src/a.rs", &[defined], &[], &[]),
        ]);
        let g = Neighbors::whole(&view_graph(&records, &view)).unwrap();
        [
            edge_tier(&g, REL_REFERENCES, "src/b.rs::%new"),
            edge_tier(&g, REL_REFERENCES, "src/c.rs::%%new"),
        ]
    };
    assert_eq!(
        tiers("other"),
        [TIER_AMBIGUOUS, TIER_AMBIGUOUS],
        "nothing defines `new` yet"
    );
    assert_eq!(
        tiers("new"),
        [TIER_INFERRED, TIER_AMBIGUOUS],
        "a definition of `new` upgrades the `%new` reference and not the `%%new` one"
    );
    assert_eq!(
        tiers("other"),
        [TIER_AMBIGUOUS, TIER_AMBIGUOUS],
        "the tree dropping `new` demotes the `%new` reference"
    );
}

/// GIVEN `src/a.rs` defines `new`, `src/d.rs` references it, landing on `src/d.rs::%new`,
/// `src/e.rs` defines its own `new` and references it, landing on `src/e.rs::new`, `src/f.rs`
/// references `%new`, landing on `src/f.rs::%%new`, and `src/g.rs` references and calls `new`
/// from `run`, WHEN the UP view is built from `src/a.rs::new`, THEN `src/d.rs` and `src/e.rs` are
/// referenced but not called, `src/f.rs` binds no leaf of the seed and `src/g.rs` calls it.
#[test]
fn referenced_not_called_counts_a_file_level_reference_through_both_spellings() {
    let (records, view) = tree(vec![
        fns("src/a.rs", &["new"], &[], &[]),
        fns("src/g.rs", &["run"], &[("run", "new")], &[]),
        fns("src/d.rs", &[], &[], &["new"]),
        fns("src/e.rs", &["new"], &[], &["new"]),
        fns("src/f.rs", &[], &[], &["%new"]),
    ]);
    assert_eq!(
        referenced_not_called_ids(&view_graph(&records, &view), SEED),
        ["src/d.rs", "src/e.rs"],
        "both spellings count as a reference, and as a call"
    );
}

/// GIVEN the file node `src/x.rs`, which has no `name` and no `::` in its id, and `src/z.rs`
/// referencing the text `src/x.rs`, WHEN the UP view is built from the file node, THEN it lists
/// nothing referenced: a seed with no leaf names nothing, never its whole id.
#[test]
fn a_seed_with_no_leaf_names_nothing_referenced() {
    let (records, view) = tree(vec![
        fns("src/x.rs", &["f"], &[], &[]),
        fns("src/z.rs", &[], &[], &["src/x.rs"]),
    ]);
    let p = view_graph(&records, &view);
    assert_eq!(
        Neighbors::whole(&p)
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
