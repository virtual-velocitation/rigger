//! Periphery (API / integration) tests for spec 119 criterion 1, THE LEAF KEYS RESOLUTION: one
//! id cut and one node leaf key every node-side reader, so a placeholder resolves onto a nested
//! definition (`src/a.rs::T::new`, named `new`) by its leaf, and a node with an empty `name` is
//! no definition. The crates' own tests pin each reader from inside its crate; these run OUTSIDE
//! the crates and guard the boundaries the rule crosses on its way to an operator:
//!
//!  - the readers agreeing across crates: the files lens whole and re-grained (`dash`), the
//!    served `/api/graph` route and the coupling layer (`community`), over one in-memory graph;
//!  - the public projection: an entity event with an empty `name`, folded by
//!    `Projector::apply`, answers no bare lookup through `Projector::locate` (the binary's
//!    `rigger graph --show` refuses an empty entity before it reaches the projection).
//!
//! No grammar yields a nested id at this tree, so the cross-crate readers are driven over an
//! in-memory graph; the empty `name` enters as the entity event the fold reads. Every test runs
//! on both lanes.

mod common;

use common::fixtures::{
    apply_code_entity, apply_def, def_node, definition, edge, empty_named_node, plain,
};
use common::lens::{coupling_rows, files_lens_rows, SUB_C, SUB_C_FILES_ROUTE};
use common::served::served_json;
use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{
    EntitySite, Graph, Located, Node, KIND_CODE_ENTITY, KIND_CONCEPT, REL_CALLS, REL_CONTAINS,
    REL_REALIZES, TIER_EXTRACTED, TIER_INFERRED,
};

/// The nested definition every placeholder below resolves onto.
const NESTED: &str = "src/a.rs::T::new";
/// The fn calling the placeholder.
const CALLER: &str = "src/c.rs::caller";
/// The placeholder `CALLER` calls, its leaf `new` behind one sigil.
const PLACEHOLDER: &str = "src/c.rs::%new";

/// A graph where the concept realizes [`NESTED`], [`CALLER`] and `placeholder`, which
/// [`CALLER`] calls; each of `beside` is one more node outside the concept.
fn leaf_graph(placeholder: &str, beside: Vec<Node>) -> Graph {
    let mut nodes = vec![
        plain(SUB_C, KIND_CONCEPT),
        def_node(NESTED, "new"),
        def_node(CALLER, "caller"),
        plain(placeholder, KIND_CODE_ENTITY),
    ];
    nodes.extend(beside);
    Graph {
        nodes,
        edges: vec![
            edge(NESTED, SUB_C, REL_REALIZES, TIER_EXTRACTED),
            edge(CALLER, SUB_C, REL_REALIZES, TIER_EXTRACTED),
            edge(placeholder, SUB_C, REL_REALIZES, TIER_EXTRACTED),
            edge(CALLER, placeholder, REL_CALLS, TIER_INFERRED),
        ],
    }
}

/// GIVEN a placeholder whose leaf is the name of a nested definition in another file, WHEN the
/// files lens folds the whole graph and re-grains the concept, THEN the placeholder folds under
/// the nested definition's file; a second definition of the leaf leaves it unresolved with both
/// candidates; a node of an empty `name` is no candidate but a placeholder of its id's leaf; a
/// doubled sigil leaves the leaf one sigil, which nothing defines; and the placeholder of the
/// empty leaf, `src/c.rs::%`, has no definition, which no node of an empty `name` gives.
#[test]
fn the_files_lens_folds_a_placeholder_onto_the_nested_definition_of_its_leaf() {
    assert_eq!(
        files_lens_rows(&leaf_graph(PLACEHOLDER, Vec::new()), SUB_C, 3),
        [
            "whole src/a.rs 2",
            "whole src/c.rs 1",
            "whole src/a.rs -- src/c.rs 1",
            "concept src/a.rs 2",
            "concept src/c.rs 1",
        ],
        "the placeholder's leaf `new` is the nested definition's name"
    );
    assert_eq!(
        files_lens_rows(
            &leaf_graph(PLACEHOLDER, vec![def_node("src/d.rs::new", "new")]),
            SUB_C,
            3
        ),
        [
            "whole src/a.rs 1",
            "whole src/c.rs 1",
            "whole src/d.rs 1",
            "concept src/a.rs 1",
            "concept src/c.rs 1",
            "concept src/c.rs::%new <- [\"src/a.rs::T::new\", \"src/d.rs::new\"]",
        ],
        "two definitions of the leaf name no one"
    );
    assert_eq!(
        files_lens_rows(
            &leaf_graph(PLACEHOLDER, vec![empty_named_node("src/e.rs::%new")]),
            SUB_C,
            3
        ),
        [
            "whole src/a.rs 3",
            "whole src/c.rs 1",
            "whole src/a.rs -- src/c.rs 1",
            "concept src/a.rs 2",
            "concept src/c.rs 1",
        ],
        "an empty `name` defines nothing: that node resolves by its id's leaf like a placeholder"
    );
    assert_eq!(
        files_lens_rows(&leaf_graph("src/c.rs::%%new", Vec::new()), SUB_C, 3),
        [
            "whole src/a.rs 1",
            "whole src/c.rs 1",
            "concept src/a.rs 1",
            "concept src/c.rs 1",
            "concept src/c.rs::%%new <- []",
        ],
        "one sigil is stripped, so the leaf `%new` has no definition"
    );
    assert_eq!(
        files_lens_rows(
            &leaf_graph("src/c.rs::%", vec![empty_named_node("src/e.rs::%")]),
            SUB_C,
            3
        ),
        [
            "whole src/a.rs 1",
            "whole src/c.rs 1",
            "concept src/a.rs 1",
            "concept src/c.rs 1",
            "concept src/c.rs::% <- []",
        ],
        "an empty `name` keys no definition, so the empty leaf of `src/c.rs::%` has none"
    );
}

/// GIVEN the same graph served, WHEN the operator re-grains the concept by files through the
/// `/api/graph` route, THEN the placeholder folds under the nested definition's file and no
/// member is unresolved.
#[test]
fn the_served_files_regrain_folds_a_placeholder_onto_the_nested_definition() {
    let body = served_json(&leaf_graph(PLACEHOLDER, Vec::new()), SUB_C_FILES_ROUTE);
    assert_eq!(
        body,
        serde_json::json!({
            "subject": "concept/1/0",
            "total": 3,
            "clusters": [
                { "key": "src/a.rs", "kind": "code-entity", "count": 2 },
                { "key": "src/c.rs", "kind": "code-entity", "count": 1 },
            ],
            "edges": [{ "from": "src/a.rs", "to": "src/c.rs", "weight": 1 }],
        }),
        "the served re-grain folds the placeholder under the nested definition's file and \
         carries no unresolved member"
    );
}

/// The coupling rows of a graph where `src/a.rs` contains [`NESTED`] and [`CALLER`] calls
/// [`PLACEHOLDER`], with each of `beside` one more edgeless node.
fn coupling_of_the_call(beside: Vec<Node>) -> Vec<String> {
    let mut nodes = vec![
        def_node(NESTED, "new"),
        def_node(CALLER, "caller"),
        plain(PLACEHOLDER, KIND_CODE_ENTITY),
    ];
    nodes.extend(beside);
    coupling_rows(&Graph {
        nodes,
        edges: vec![
            edge("src/a.rs", NESTED, REL_CONTAINS, TIER_EXTRACTED),
            edge(CALLER, PLACEHOLDER, REL_CALLS, TIER_INFERRED),
        ],
    })
}

/// GIVEN a call onto a placeholder whose leaf is a nested definition's name, WHEN the coupling
/// layer is built, THEN the call's edge lands on the nested definition; beside a second
/// definition of the leaf the placeholder stays its own node; beside a node of an empty `name`
/// the edge still lands on the nested definition.
#[test]
fn the_coupling_layer_canonicalises_a_placeholder_onto_the_nested_definition_of_its_leaf() {
    assert_eq!(
        coupling_of_the_call(Vec::new()),
        ["src/a.rs 1", "src/a.rs::T::new 2", "src/c.rs::caller 1"],
        "the call's edge lands on the nested definition"
    );
    assert_eq!(
        coupling_of_the_call(vec![def_node("src/d.rs::new", "new")]),
        [
            "src/a.rs 1",
            "src/a.rs::T::new 1",
            "src/c.rs::%new 1",
            "src/c.rs::caller 1"
        ],
        "two definitions of the leaf leave the placeholder its own node"
    );
    assert_eq!(
        coupling_of_the_call(vec![empty_named_node("src/e.rs::%new")]),
        ["src/a.rs 1", "src/a.rs::T::new 2", "src/c.rs::caller 1"],
        "a node of an empty `name` is no second definition"
    );
}

/// GIVEN a projection where `src/a.rs` holds an entity of an empty `name` and `src/b.rs` a fn
/// `open`, WHEN each name is looked up through `Projector::locate`, THEN the empty name answers
/// no entity and `open` its fn.
#[test]
fn an_entity_of_an_empty_name_answers_no_bare_lookup() {
    let p = Projector::open(":memory:", "test").unwrap();
    apply_code_entity(&p, 1, "src/a.rs", "", "function", 1, "rust");
    apply_def(&p, 2, "src/b.rs", "open", 1, true);
    assert_eq!(
        p.whole()
            .unwrap()
            .nodes
            .iter()
            .filter(|n| n.kind == KIND_CODE_ENTITY)
            .map(|n| (n.id.as_str(), n.attrs.get("name").map(String::as_str)))
            .collect::<Vec<_>>(),
        [("src/a.rs::", Some("")), ("src/b.rs::open", Some("open"))],
        "the fold records the empty name it was given"
    );
    assert_eq!(
        p.locate("").unwrap(),
        Located::None,
        "an empty name is no leaf"
    );
    assert_eq!(
        p.locate("open").unwrap(),
        Located::One(EntitySite {
            id: "src/b.rs::open".to_string(),
            kind: "function".to_string(),
            file: Some("src/b.rs".to_string()),
            line: 1,
            degree: 1,
            attrs: definition("open", "function", 1),
            unresolved: vec![],
        }),
        "a named fn answers its leaf"
    );
}
