//! The show-renderer seed read through the code store's view (spec 119, SHOW IS ONE RENDERER): a
//! tree of real source and documents, which the code store extracts, and the same tree planted
//! as the facts its extractors read, beside a record half holding one finding, with the oracles
//! the `rigger graph --show` and `rigger_graph` `show` and `around` outputs are checked by.
//!
//! The tree holds a const, a type and a function in [`SHOWN_TREE_FILE`], a test that proves the
//! function, an architecture decision and a design-doc section. The cases the fold seeded that
//! no extractor produces are not here: a definition whose kind is empty, a definition of a file
//! with no extractor (`Makefile`), a field with no language, stale notes for source not in the
//! tree, a document node whose `doc` holds a newline. The newline case is covered at the renderer
//! by the domain unit test of `site_fields` in `crates/rigger-domain/src/contextgraph.rs`.

use std::collections::BTreeMap;

use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{
    around, entity_site, site_fields, ContextGraph, EntitySite, Fold, Located, Neighbors,
    TYPE_REVIEW_FINDING,
};
use rigger::eventstore::Event;

/// The file the tree's three definitions live in.
pub const SHOWN_TREE_FILE: &str = "src/limits.rs";

/// The tree's files `(path, text)`: the definitions, the test that proves the function, a
/// design-doc section and an architecture decision.
pub const SHOWN_TREE_SOURCES: [(&str, &str); 4] = [
    (
        SHOWN_TREE_FILE,
        "//! limits\npub const LIMIT: u32 = 3;\n\npub struct Bounds;\n\npub fn blank() {}\n",
    ),
    (
        "tests/limits.rs",
        "#[test]\nfn blank_is_blank() {\n    blank();\n}\n",
    ),
    ("docs/x.md", "# Title\n\n## Sec\n\ntext\n"),
    (
        "docs/decisions/one-renderer.md",
        "# One renderer\n\nthe decision log\n",
    ),
];

/// The ids shown by exact id: the const, the type, the function, the finding the record half
/// holds, the architecture decision and the design-doc section.
pub const SHOWN_TREE_IDS: [&str; 6] = [
    "src/limits.rs::LIMIT",
    "src/limits.rs::Bounds",
    "src/limits.rs::blank",
    "f-shown",
    "docs/decisions/one-renderer.md",
    "docs/x.md#sec",
];

/// The code view of the tree planted as the facts [`SHOWN_TREE_SOURCES`] extract to.
pub fn shown_tree_planted_view() -> rigger::code_view::FactsView {
    use rigger::contextgraph::{KIND_ARCH_DECISION, KIND_DESIGN_DOC};
    let mut tree = super::PlantedTree::default();
    tree.define(SHOWN_TREE_FILE, "LIMIT", "constant", 2);
    tree.define(SHOWN_TREE_FILE, "Bounds", "type", 4);
    tree.define(SHOWN_TREE_FILE, "blank", "function", 6);
    tree.files.push(super::planted_file(
        "tests/limits.rs",
        &[("blank_is_blank", "function")],
        &[("blank_is_blank", "blank")],
        &[],
    ));
    tree.concept(
        KIND_ARCH_DECISION,
        "docs/decisions/one-renderer.md",
        "One renderer",
    );
    tree.concept(KIND_DESIGN_DOC, "docs/x.md#sec", "Sec");
    tree.view()
}

/// The record event of the finding `f-shown` ABOUT [`SHOWN_TREE_FILE`] and `docs/x.md`.
pub fn shown_tree_finding() -> Event {
    let mut e = Event::new(
        TYPE_REVIEW_FINDING,
        serde_json::to_vec(&serde_json::json!({
            "id": "f-shown", "by": "lens:sdet", "unit": "u7",
            "summary": "the width overflows", "about": [SHOWN_TREE_FILE, "docs/x.md"],
        }))
        .unwrap(),
    );
    e.position = 1;
    e
}

/// Fold [`shown_tree_finding`] into `p`.
pub fn fold_shown_tree_finding(p: &Projector) {
    let events = [shown_tree_finding()];
    assert_eq!(
        Fold::of_batch(|| rigger::contextgraph::wired(Some(p)), &events),
        Fold::Folded
    );
}

/// A rendered node object's keys less its `id`, which must be `id`, and less its `degree`: the
/// production composition counts the community edges the oracles' composition does not hold, so
/// the tests pin each degree by value.
fn site_keys(object: &serde_json::Value, id: &str) -> BTreeMap<String, serde_json::Value> {
    let mut keys = object.as_object().expect("a node object").clone();
    assert_eq!(
        keys.remove("id"),
        Some(serde_json::json!(id)),
        "the object names its node"
    );
    keys.remove("degree");
    keys.into_iter().collect()
}

/// The entries [`site_fields`] gives `site`, keyed for an unordered comparison.
fn fields_of(site: &EntitySite) -> BTreeMap<String, serde_json::Value> {
    site_fields(site)
        .into_iter()
        .filter(|(key, _)| *key != "degree")
        .map(|(key, value)| (key.to_string(), value))
        .collect()
}

/// The one site `graph` locates for `id`.
pub fn shown_tree_site(graph: &dyn Neighbors, id: &str) -> EntitySite {
    let Located::One(site) = rigger::composed::locate(graph, id).unwrap() else {
        panic!("{id} locates one node")
    };
    site
}

/// Each of [`SHOWN_TREE_IDS`] beside the one site `graph` locates for it, in their order.
pub fn shown_tree_sites(graph: &dyn Neighbors) -> Vec<(&'static str, EntitySite)> {
    SHOWN_TREE_IDS
        .into_iter()
        .map(|id| (id, shown_tree_site(graph, id)))
        .collect()
}

/// The tree's function is proven by its test, so it carries `proven_by`, an attr outside the printed
/// list: a surface printing no `proven_by` excludes an attr the node carries, never one it lacks.
pub fn assert_shown_tree_function_carries_proven_by(graph: &dyn ContextGraph) {
    let around_file = graph
        .subgraph(&[SHOWN_TREE_FILE.to_string()], 2)
        .expect("the file's neighbourhood reads");
    let card = rigger::contextgraph::query::card(&around_file, "src/limits.rs::blank")
        .expect("the function is in the graph");
    assert_eq!(
        card.proven_by, 1,
        "the function carries proven_by, an attr outside the printed list"
    );
}

/// Each of `sites`, the `rigger_graph` `show` site objects of [`SHOWN_TREE_IDS`] in their order, is
/// its node's `id` beside the [`site_fields`] entries of the site `graph` locates for it,
/// compared without order.
pub fn assert_shown_tree_sites_are_site_fields(graph: &dyn Neighbors, sites: &[serde_json::Value]) {
    for ((id, located), site) in shown_tree_sites(graph).iter().zip(sites) {
        assert_eq!(
            site_keys(site, id),
            fields_of(located),
            "{id}: show is site_fields"
        );
    }
}

/// The node objects of a `rigger_graph` `around` answer, sorted by id.
pub fn shown_tree_around_nodes(answer: &serde_json::Value) -> Vec<serde_json::Value> {
    let mut nodes = answer["nodes"].as_array().expect("a node list").clone();
    nodes.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    nodes
}

/// `finding`, the `rigger_graph` `show` site of the finding, carries the two `ABOUT` edges the
/// store holds, where an `around` of [`SHOWN_TREE_FILE`] at depth 1 loads one of them, and its
/// `summary`, which every `around` node omits.
pub fn assert_shown_tree_finding_shows_past_the_depth_bound(finding: &serde_json::Value) {
    assert_eq!(
        finding["degree"], 2,
        "show reads the finding's degree from the graph, past the depth bound"
    );
    assert_eq!(
        finding["summary"], "the width overflows",
        "show keeps the finding's summary"
    );
}

/// Each of `nodes`, the `rigger_graph` `around` node objects of [`SHOWN_TREE_FILE`] at depth 1,
/// carries no `summary` key and is [`entity_site`] over the `Node` [`around`] loads from `graph`
/// at the degree the object carries less its `summary` entry, compared without order.
pub fn assert_shown_tree_around_is_entity_sites(
    graph: &dyn ContextGraph,
    nodes: &[serde_json::Value],
) {
    let loaded = around(graph, &[SHOWN_TREE_FILE.to_string()], 1).unwrap();
    for node in nodes {
        let id = node["id"].as_str().unwrap();
        if node["kind"] == "community" {
            // Communities derive from code coupling in the production composition alone.
            continue;
        }
        let degree = node["degree"].as_u64().unwrap() as usize;
        let held = loaded
            .nodes
            .iter()
            .find(|n| n.id == id)
            .expect("a loaded node");
        assert_eq!(
            node.get("summary"),
            None,
            "{id}: an around node omits summary"
        );
        let mut fields = fields_of(&entity_site(held, degree));
        fields.remove("summary");
        assert_eq!(
            site_keys(node, id),
            fields,
            "{id}: an around node is entity_site over its loaded Node, less its summary"
        );
    }
}
