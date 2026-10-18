//! Context-graph fixtures: nodes, edges, whole graphs and projection doubles.

use std::collections::BTreeMap;

use rigger::contextgraph::query::{card, path_leaf, split_id};
use rigger::contextgraph::{
    CallEdge, CallGraph, Direction, Edge, Graph, Node, Projection, CODE_KIND_MODULE, KIND_ARTIFACT,
    KIND_CODE_ENTITY, KIND_COMMUNITY, KIND_CONCEPT, KIND_DECISION, KIND_FILE, KIND_FINDING,
    KIND_LESSON, KIND_UNIT, REL_ABOUT, REL_CALLS, REL_CONTAINS, REL_GOVERNS, REL_IN_COMMUNITY,
    REL_REALIZES, REL_REFERENCES, TIER_EXTRACTED, TIER_INFERRED,
};

/// A `kind` node with no attributes.
pub fn plain(id: &str, kind: &str) -> Node {
    Node {
        id: id.to_string(),
        kind: kind.to_string(),
        attrs: BTreeMap::new(),
    }
}

/// A `kind` node carrying each of `attrs` that has a value (an ingested document's `title`, a
/// code entity's `name`, a display `label`, ...).
pub fn node_with_optional_attrs(id: &str, kind: &str, attrs: &[(&str, Option<&str>)]) -> Node {
    let mut n = plain(id, kind);
    for (key, value) in attrs {
        if let Some(v) = value {
            n.attrs.insert(key.to_string(), v.to_string());
        }
    }
    n
}

/// A `kind` node carrying `attrs`.
pub fn node_with_attrs(id: &str, kind: &str, attrs: &[(&str, &str)]) -> Node {
    Node {
        id: id.to_string(),
        kind: kind.to_string(),
        attrs: attrs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    }
}

/// A `kind` node carrying a `summary` attribute, or no attribute at all when `summary` is empty.
pub fn summarized_node(id: &str, kind: &str, summary: &str) -> Node {
    Node {
        id: id.to_string(),
        kind: kind.to_string(),
        attrs: if summary.is_empty() {
            BTreeMap::new()
        } else {
            BTreeMap::from([("summary".to_string(), summary.to_string())])
        },
    }
}

/// A live `rel` edge `from -> to` at confidence `tier`.
pub fn edge(from: &str, to: &str, rel: &str, tier: &str) -> Edge {
    Edge {
        from: from.to_string(),
        to: to.to_string(),
        rel: rel.to_string(),
        valid_from: 0,
        valid_to: None,
        source: 0,
        tier: tier.to_string(),
    }
}

/// An [`edge`] whose validity ends at `valid_to` (`None`: still live).
pub fn edge_valid_to(from: &str, to: &str, rel: &str, tier: &str, valid_to: Option<i64>) -> Edge {
    Edge {
        valid_to,
        ..edge(from, to, rel, tier)
    }
}

/// A directed-walk `CALLS` edge `from -> to`, marked as a recursion arc when `back`.
pub fn calls_edge(from: &str, to: &str, back: bool) -> CallEdge {
    CallEdge {
        edge: edge(from, to, REL_CALLS, TIER_INFERRED),
        back,
    }
}

/// A unit `hub` referencing `spokes` code entities `<hub>-s<i>`.
pub fn star_graph(hub: &str, spokes: usize) -> Graph {
    let mut nodes = vec![plain(hub, KIND_UNIT)];
    let mut edges = Vec::new();
    for i in 0..spokes {
        let spoke = format!("{hub}-s{i}");
        nodes.push(plain(&spoke, "code-entity"));
        edges.push(edge(hub, &spoke, REL_REFERENCES, TIER_EXTRACTED));
    }
    Graph { nodes, edges }
}

/// `len` unit nodes `n0..` joined into one `REFERENCES` chain `n0 -> n1 -> ...`.
pub fn chain_graph(len: usize) -> Graph {
    let nodes = (0..len)
        .map(|i| plain(&format!("n{i}"), KIND_UNIT))
        .collect();
    let edges = (0..len.saturating_sub(1))
        .map(|i| {
            edge(
                &format!("n{i}"),
                &format!("n{}", i + 1),
                REL_REFERENCES,
                TIER_EXTRACTED,
            )
        })
        .collect();
    Graph { nodes, edges }
}

/// A `(key, value)` pair list (a derivation's membership or labels) as a lookup map.
pub fn pair_map(pairs: &[(String, String)]) -> BTreeMap<String, String> {
    pairs.iter().cloned().collect()
}

/// The ledger half of a `Projection` double that folds no ledger entry: it holds no generation
/// and refuses the ledger fold. Expanded inside that double's `impl Projection` block.
#[macro_export]
macro_rules! projection_folds_no_entry {
    () => {
        fn current_generation(
            &self,
            _identity: &str,
        ) -> Result<Option<String>, rigger::contextgraph::Error> {
            Ok(None)
        }
        fn apply_generation(
            &self,
            _entry: &rigger::eventstore::Event,
            _batch: rigger::contextgraph::EntryBatch<'_>,
        ) -> Result<rigger::contextgraph::EntryFold, rigger::contextgraph::Error> {
            Err(rigger::contextgraph::Error(
                "this double folds no ledger entry".to_string(),
            ))
        }
    };
}

/// The read half of a `Projection` double no test reads: asking it for a subgraph or a mention
/// fails the test. Expanded inside that double's `impl Projection` block.
#[macro_export]
macro_rules! projection_is_never_read {
    () => {
        fn subgraph(
            &self,
            _seed: &[String],
            _depth: i64,
        ) -> Result<rigger::contextgraph::Graph, rigger::contextgraph::Error> {
            panic!("this double is never read")
        }
        fn resolve(&self, _mention: &str) -> Result<Option<String>, rigger::contextgraph::Error> {
            panic!("this double is never read")
        }
    };
}

/// What a `Projection` double answers with nothing - an empty subgraph, an unresolved mention and
/// no rebuild owed - with the ledger half of a double that folds no entry
/// (`projection_folds_no_entry`), expanded inside that double's `impl Projection` block.
#[macro_export]
macro_rules! projection_reads_nothing {
    () => {
        fn subgraph(
            &self,
            _seed: &[String],
            _depth: i64,
        ) -> Result<rigger::contextgraph::Graph, rigger::contextgraph::Error> {
            Ok(rigger::contextgraph::Graph::default())
        }
        fn resolve(&self, _mention: &str) -> Result<Option<String>, rigger::contextgraph::Error> {
            Ok(None)
        }
        fn rebuild_owed(&self) -> Result<bool, rigger::contextgraph::Error> {
            Ok(false)
        }
        $crate::projection_folds_no_entry!();
    };
}

/// A `Projection` implementing only the trait's REQUIRED methods, each trivially, so every
/// provided method (`locate`, `calls`, ...) is the trait DEFAULT under test.
pub struct MinimalProjection;

impl rigger::contextgraph::Projection for MinimalProjection {
    fn apply(
        &self,
        _e: &rigger::eventstore::Event,
        _access: rigger::contextgraph::FoldAccess,
    ) -> Result<(), rigger::contextgraph::Error> {
        Ok(())
    }
    projection_reads_nothing!();
}

/// The id of spoke `i` of a many-spoke graph fixture: a code entity in `file`, zero-padded so the
/// ids sort in spoke order.
pub fn spoke_id(file: &str, i: usize) -> String {
    format!("{file}::s{i:05}")
}

/// Every `GOVERNS` edge in `graph_edges` as a sorted `(from, to, source, valid_from)` list.
pub fn governs(graph_edges: &[Edge]) -> Vec<(String, String, u64, i64)> {
    let mut out: Vec<_> = graph_edges
        .iter()
        .filter(|e| e.rel == rigger::contextgraph::REL_GOVERNS)
        .map(|e| (e.from.clone(), e.to.clone(), e.source, e.valid_from))
        .collect();
    out.sort();
    out
}

/// The tier of the one edge with relation `rel` landing on `to`, out of a subgraph.
pub fn edge_tier(g: &Graph, rel: &str, to: &str) -> String {
    let matches: Vec<&Edge> = g
        .edges
        .iter()
        .filter(|e| e.rel == rel && e.to == to)
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one {rel} edge to {to}; got {matches:?}"
    );
    matches[0].tier.clone()
}

/// The call walk of `p` from `seed` in `direction`, five hops deep, at the inferred floor.
pub fn call_walk(p: &dyn Projection, seed: &str, direction: Direction) -> CallGraph {
    p.calls(&[seed.to_string()], direction, 5, TIER_INFERRED)
        .unwrap()
}

/// The ids of the files the UP view of `p` from `seed` lists referenced but not called.
pub fn referenced_not_called_ids(p: &dyn Projection, seed: &str) -> Vec<String> {
    let up = call_walk(p, seed, Direction::Up);
    up.referenced_not_called.into_iter().map(|n| n.id).collect()
}

/// The reached nodes of a `CallGraph` carrying a frontier, each with its candidates, sorted.
pub fn call_frontiers(cg: &CallGraph) -> Vec<(String, Vec<String>)> {
    let mut marked: Vec<(String, Vec<String>)> = cg
        .nodes
        .iter()
        .filter_map(|n| Some((n.node.id.clone(), n.frontier.clone()?)))
        .collect();
    marked.sort();
    marked
}

/// The reached node ids of a `CallGraph`, sorted, for a stable membership assertion.
pub fn call_node_ids(cg: &CallGraph) -> Vec<String> {
    let mut v: Vec<String> = cg.nodes.iter().map(|n| n.node.id.clone()).collect();
    v.sort();
    v
}

/// The `(from, to)` endpoints of a `CallGraph`'s edges, sorted.
pub fn call_edge_pairs(cg: &CallGraph) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = cg
        .edges
        .iter()
        .map(|e| (e.edge.from.clone(), e.edge.to.clone()))
        .collect();
    v.sort();
    v
}

/// The layer a `CallGraph` places node `id` at, if it holds it.
pub fn call_layer(cg: &CallGraph, id: &str) -> Option<i64> {
    cg.nodes.iter().find(|n| n.node.id == id).map(|n| n.layer)
}

/// Whether a `CallGraph`'s `from -> to` edge is a back edge, if it holds one.
pub fn call_back_edge(cg: &CallGraph, from: &str, to: &str) -> Option<bool> {
    cg.edges
        .iter()
        .find(|e| e.edge.from == from && e.edge.to == to)
        .map(|e| e.back)
}

/// A code entity `combat.rs::fire` carrying a governing decision `d1`, an ABOUT finding `f1`, an
/// ABOUT lesson `l1` (build-process memory, which the memory rail excludes) and its own live
/// `REALIZES` edge to the concept `concept/combat` (the direction a MEMBER carries toward the
/// concept it realizes). A second, unrelated file `other.rs` carries none of it, for the empty
/// case.
pub fn subject_graph() -> Graph {
    let rel = |from: &str, to: &str, rel: &str| edge(from, to, rel, TIER_INFERRED);
    Graph {
        nodes: vec![
            plain("combat.rs::fire", KIND_CODE_ENTITY),
            plain("other.rs", KIND_FILE),
            summarized_node("d1", KIND_DECISION, "use the shared authority"),
            summarized_node("f1", KIND_FINDING, "the finding content"),
            summarized_node("l1", KIND_LESSON, "the lesson content"),
            node_with_attrs(
                "concept/combat",
                KIND_CONCEPT,
                &[("label", "combat resolution")],
            ),
        ],
        edges: vec![
            rel("d1", "combat.rs::fire", REL_GOVERNS),
            rel("f1", "combat.rs::fire", REL_ABOUT),
            rel("l1", "combat.rs::fire", REL_ABOUT),
            rel("combat.rs::fire", "concept/combat", REL_REALIZES),
        ],
    }
}

/// A code-entity DEFINITION node: its `name` attr marks it a real definition (not a bare
/// placeholder), exactly as the extraction fold records.
pub fn def_node(id: &str, name: &str) -> Node {
    node_with_optional_attrs(
        id,
        KIND_CODE_ENTITY,
        &[("title", None), ("name", Some(name))],
    )
}

/// A code-entity node whose `name` attr is present and EMPTY: no definition, since an empty name
/// reads as absent (spec 119), so it resolves as a bare placeholder does.
pub fn empty_named_node(id: &str) -> Node {
    node_with_attrs(id, KIND_CODE_ENTITY, &[("name", "")])
}

/// A code-entity definition named by its id's leaf (`src/a.rs::foo` named `foo`): [`def_node`]
/// over [`split_id`]'s rest half, the shape the extraction fold gives a flat definition.
pub fn leaf_named_node(id: &str) -> Node {
    def_node(id, split_id(id).1.expect("a code-entity id holds `::`"))
}

/// A small two-module + decision graph a route dispatch is proven over: the named definitions
/// ([`leaf_named_node`]) `src/a/mod.rs::foo` and `::bar` and `src/b/mod.rs::baz`, and a decision
/// `d1`, with the live references `foo -> bar` (one file), `bar -> baz` (across files) and
/// `d1 -> foo` (touching the decision no file cluster holds).
pub fn two_module_graph() -> Graph {
    let refs = |from: &str, to: &str| edge(from, to, REL_REFERENCES, TIER_EXTRACTED);
    Graph {
        nodes: vec![
            leaf_named_node("src/a/mod.rs::foo"),
            leaf_named_node("src/a/mod.rs::bar"),
            leaf_named_node("src/b/mod.rs::baz"),
            plain("d1", KIND_DECISION),
        ],
        edges: vec![
            refs("src/a/mod.rs::foo", "src/a/mod.rs::bar"),
            refs("src/a/mod.rs::bar", "src/b/mod.rs::baz"),
            refs("d1", "src/a/mod.rs::foo"),
        ],
    }
}

/// A code-entity DEFINITION node of code kind `kind`, named by its id's leaf ([`leaf_named_node`]):
/// what the extraction fold records for a definition of that kind.
pub fn kinded_def_node(id: &str, kind: &str) -> Node {
    let mut node = leaf_named_node(id);
    node.attrs.insert("kind".to_string(), kind.to_string());
    node
}

/// The community a test built by [`one_community_graph`] holds its members in, and the first
/// community the seeded community fixture labels.
pub const ONE_COMMUNITY: &str = "community/1/0";

/// A code kind string outside `CODE_KIND_PRECEDENCE`, which ranks with `other`.
pub const UNLISTED_KIND: &str = "alien";
/// A second code kind string outside `CODE_KIND_PRECEDENCE`, sorting after [`UNLISTED_KIND`] and
/// `other`.
pub const LATER_UNLISTED_KIND: &str = "zeta";

/// A graph of the one community node `community` followed by `members`, each held by a live
/// inferred `IN_COMMUNITY` edge in `members` order: the subject a re-projection folds.
pub fn one_community_graph(community: &str, members: Vec<Node>) -> Graph {
    let edges = members
        .iter()
        .map(|m| edge(&m.id, community, REL_IN_COMMUNITY, TIER_INFERRED))
        .collect();
    let mut nodes = vec![plain(community, KIND_COMMUNITY)];
    nodes.extend(members);
    Graph { nodes, edges }
}

/// The `file` row of the card [`card`] gives `id` in `graph`: `None` when `id` has no card or its
/// card names no file.
pub fn card_file(graph: &Graph, id: &str) -> Option<String> {
    card(graph, id).and_then(|c| c.file)
}

/// THE MEMBER RULE's nested method (spec 119): a definition two levels into its file `a.rs`.
pub const NESTED_METHOD: &str = "a.rs::T::new";
/// The flat id `<file>::<leaf>` of [`NESTED_METHOD`]'s leaf in its file: the id a decision holds
/// by leaf alone.
pub const FLAT_LEAF_ID: &str = "a.rs::new";
/// A file-level definition beside [`NESTED_METHOD`] in its file.
pub const FILE_MATE: &str = "a.rs::mate";
/// The placeholder of [`NESTED_METHOD`]'s leaf in its file: a nameless code entity.
pub const LEAF_PLACEHOLDER: &str = "a.rs::%new";
/// A bare parent in [`NESTED_METHOD`]'s file: a nameless code entity a child's parent edge made.
pub const BARE_PARENT: &str = "a.rs::P";
/// An `artifact` in [`NESTED_METHOD`]'s file range, the kind a finding's mention settles.
pub const RANGE_ARTIFACT: &str = "a.rs::x";
/// A target of the extensionless file `Makefile`.
pub const MAKE_TARGET: &str = "Makefile::all";
/// A definition whose file no node of [`member_graph`] holds.
pub const FILELESS_MEMBER: &str = "c.rs::U::run";

/// The file half of the code-entity id `id` ([`split_id`]): the file it is a member of.
pub fn file_half(id: &str) -> &str {
    split_id(id).0
}

/// The file declaring `mod x;` (spec 117, A MODULE ROW REACHES ITS FILE).
pub const DECLARING_FILE: &str = "src/lib.rs";
/// The `mod` node of [`DECLARING_FILE`]'s `mod x;`.
pub const MOD_NODE: &str = "src/lib.rs::x";
/// The first candidate file of [`MOD_NODE`]'s declaration.
pub const MODULE_FILE: &str = "src/x.rs";
/// The second candidate file of [`MOD_NODE`]'s declaration, the directory-module form.
pub const DIR_MODULE_FILE: &str = "src/x/mod.rs";
/// The fn `y` a module file defines, here [`MODULE_FILE`]'s.
pub const MODULE_FN: &str = "src/x.rs::y";

/// A module declaration's graph (spec 117): [`DECLARING_FILE`] containing [`MOD_NODE`], whose
/// module row's `CONTAINS` reaches the `file` node [`MODULE_FILE`].
pub fn module_graph() -> Graph {
    Graph {
        nodes: vec![
            plain(DECLARING_FILE, KIND_FILE),
            kinded_def_node(MOD_NODE, CODE_KIND_MODULE),
            plain(MODULE_FILE, KIND_FILE),
        ],
        edges: vec![
            edge(DECLARING_FILE, MOD_NODE, REL_CONTAINS, TIER_EXTRACTED),
            edge(MOD_NODE, MODULE_FILE, REL_CONTAINS, TIER_EXTRACTED),
        ],
    }
}

/// The one live module edge from [`MOD_NODE`] onto `file`, `extracted`, as `(from, to, tier)`.
pub fn declared(file: &str) -> Vec<(String, String, String)> {
    vec![(MOD_NODE.into(), file.into(), TIER_EXTRACTED.into())]
}

/// THE MEMBER RULE's graph (spec 119), holding no edge, so no `CONTAINS` reaches any node: the
/// file of [`NESTED_METHOD`] beside that method, [`FILE_MATE`], [`LEAF_PLACEHOLDER`], [`BARE_PARENT`] and
/// [`RANGE_ARTIFACT`]; the file of [`MAKE_TARGET`] beside that target; and [`FILELESS_MEMBER`].
/// Each definition and the artifact carry their path's leaf as `name` ([`def_node`]), so only the
/// artifact's kind keeps it from membership.
pub fn member_graph() -> Graph {
    let named = |id: &str| def_node(id, path_leaf(id));
    let mut artifact = named(RANGE_ARTIFACT);
    artifact.kind = KIND_ARTIFACT.to_string();
    Graph {
        nodes: vec![
            plain(file_half(NESTED_METHOD), KIND_FILE),
            named(NESTED_METHOD),
            named(FILE_MATE),
            plain(LEAF_PLACEHOLDER, KIND_CODE_ENTITY),
            plain(BARE_PARENT, KIND_CODE_ENTITY),
            artifact,
            plain(file_half(MAKE_TARGET), KIND_FILE),
            named(MAKE_TARGET),
            named(FILELESS_MEMBER),
        ],
        edges: Vec::new(),
    }
}
