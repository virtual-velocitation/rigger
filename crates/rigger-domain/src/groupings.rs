//! The production groupings: the communities and concepts computed over a resolved code view at
//! advance. Both run over the view's product graph under the default filter, so no test code is
//! ever clustered. Communities detect over the coupling layer with each placeholder end taken to
//! the one product definition its down-hop reaches; each is labelled by its most-coupled member,
//! ties to the smaller name. Concepts detect over the tree's intent layer. Numbering is the
//! detection's own: a function of the tree alone.

use std::collections::BTreeMap;

use crate::code_graph::CodeGraph;
use crate::code_view::{Cluster, CodeView, Group, Groupings, ViewFilter};
use crate::community::{couples, detect, Coupling, DEFAULT_RESOLUTION};
use crate::concepts::{derive, intent_layer};
use crate::contextgraph::query::node_name;
use crate::contextgraph::Graph;
use crate::resolve::{Hop, Pool};

/// The groupings rigger computes over the production view.
pub struct ProductionGroupings;

/// The generation of the grouping algorithm: the community detection, the concept derivation,
/// the labels and the numbering. Bump it whenever any of them can give a view other groupings
/// than before; the store's fingerprint folds it in, so a snapshot grouped under an older one is
/// computed anew. A golden test pins it beside a digest of the groupings over a fixture, so a
/// change that is not bumped fails there.
const GROUPINGS_GENERATION: u64 = 1;

/// The coupling layer of `graph`, which `code` built: each coupling edge with its placeholder ends
/// taken to the one product definition they resolve to.
fn coupling(code: &CodeGraph, graph: &Graph) -> Coupling {
    Coupling::layer(graph, |id| {
        match code.resolver().down_hop(id, Pool::Product) {
            Some(Hop::One(c)) => c.id,
            _ => id.to_string(),
        }
    })
}

/// `members` grouped by cluster id, in the order the ids first appear, each labelled by `label`.
fn clusters<'a>(
    members: impl IntoIterator<Item = (&'a String, &'a String)>,
    resolution: f64,
    hash: &str,
    label: impl Fn(&str, &[String]) -> String,
) -> Vec<Cluster> {
    let mut by_id: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    let mut order = Vec::new();
    for (node, id) in members {
        by_id
            .entry(id)
            .or_insert_with(|| {
                order.push(id.as_str());
                Vec::new()
            })
            .push(node.clone());
    }
    order
        .into_iter()
        .map(|id| {
            let members = by_id.remove(id).unwrap_or_default();
            Cluster {
                id: id.to_string(),
                label: label(id, &members),
                resolution,
                hash: hash.to_string(),
                members,
            }
        })
        .collect()
}

impl Group for ProductionGroupings {
    fn generation(&self) -> u64 {
        GROUPINGS_GENERATION
    }

    fn group(&self, view: &dyn CodeView) -> Groupings {
        group_at(view, DEFAULT_RESOLUTION)
    }
}

/// The groupings of `view` at the grain `resolution` a reader computes: none at the stored grain,
/// which the view holds ([`CodeView::groupings`]), and any other computed here ([`group_at`]) on
/// each open that asks for it and never stored. The one rule every reader of a grain goes through.
pub fn computed_at(view: &dyn CodeView, resolution: f64) -> Option<Groupings> {
    (resolution != DEFAULT_RESOLUTION).then(|| group_at(view, resolution))
}

/// The production groupings of `view` at the grain `resolution`: the default grain is stored at
/// advance, and any other is computed here on demand and never stored.
pub fn group_at(view: &dyn CodeView, resolution: f64) -> Groupings {
    let code = CodeGraph::new(view, ViewFilter::default());
    let graph = code.whole();
    let kind_of = graph.kinds();
    let mut degree: BTreeMap<&str, usize> = BTreeMap::new();
    for e in &graph.edges {
        if couples(&e.rel, kind_of.get(e.to.as_str()).copied()) {
            *degree.entry(&e.from).or_default() += 1;
            if e.to != e.from {
                *degree.entry(&e.to).or_default() += 1;
            }
        }
    }
    let names: BTreeMap<&str, &str> = graph
        .nodes
        .iter()
        .filter_map(|n| node_name(n).map(|name| (n.id.as_str(), name)))
        .collect();
    let label = |_: &str, members: &[String]| {
        let ranked = members.iter().map(|m| {
            let name = names.get(m.as_str()).copied().unwrap_or(m.as_str());
            (
                std::cmp::Reverse(degree.get(m.as_str()).copied().unwrap_or(0)),
                name,
            )
        });
        ranked
            .min()
            .map(|(_, name)| name.to_string())
            .unwrap_or_default()
    };
    let found = detect(&coupling(&code, &graph), resolution);
    let pairs = found.members.iter().map(|(n, c)| (n, c));
    let communities = clusters(pairs, found.resolution, &found.hash, label);
    let derived = derive(&graph, &intent_layer(&graph), resolution);
    let titles: BTreeMap<&str, &str> = derived
        .concepts
        .iter()
        .map(|(id, title)| (id.as_str(), title.as_str()))
        .collect();
    let pairs = derived.members.iter().map(|(n, c)| (n, c));
    let concepts = clusters(pairs, derived.resolution, &derived.hash, |id, _| {
        titles.get(id).copied().unwrap_or_default().to_string()
    });
    Groupings {
        communities,
        concepts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code_view::{FactsView, Tests};
    use crate::community::fnv1a_hex;
    use crate::concepts::{derive, intent_layer};
    use crate::resolve::tests::{fixture, stamp};
    use std::collections::BTreeSet;

    #[test]
    fn groupings_never_cluster_test_entities() {
        let view = fixture();
        let shown = CodeGraph::new(
            &view,
            ViewFilter {
                tests: Tests::Include,
            },
        )
        .whole();
        let test_ids: BTreeSet<&str> = shown
            .nodes
            .iter()
            .filter(|n| n.attrs.contains_key("test"))
            .map(|n| n.id.as_str())
            .collect();
        assert!(
            test_ids.contains("src/a.rs::tests::checks"),
            "the fixture holds test code"
        );
        for linked in ["tests/it.rs", "src/a_tests.rs"] {
            assert!(test_ids.contains(linked), "a design link reaches {linked}");
        }
        let g = ProductionGroupings.group(&view);
        let members: BTreeSet<&str> = g
            .communities
            .iter()
            .chain(&g.concepts)
            .flat_map(|c| c.members.iter().map(String::as_str))
            .collect();
        assert!(members.contains("src/a.rs::run"), "product code is grouped");
        assert!(
            members.contains("docs/d.md"),
            "a doc linking tests is grouped"
        );
        let clustered: Vec<_> = members.intersection(&test_ids).collect();
        assert!(
            clustered.is_empty(),
            "test entities clustered: {clustered:?}"
        );
    }

    /// A CHANGE TO THE GROUPINGS SHIPS WITH A BUMP: the fixture's groupings, communities and
    /// concepts both, are pinned by digest beside the generation the store's fingerprint folds
    /// in, so a change to the detection, the derivation, the labels or the numbering fails here
    /// until `GROUPINGS_GENERATION` is bumped and the pair re-pinned.
    #[test]
    fn a_change_to_the_groupings_ships_with_a_generation_bump() {
        const PINNED: (u64, &str) = (1, "c06a7962f403c7a6");
        let groupings = ProductionGroupings.group(&fixture());
        assert!(
            !groupings.communities.is_empty() && !groupings.concepts.is_empty(),
            "the fixture holds communities and concepts"
        );
        let digest = fnv1a_hex(&format!("{groupings:#?}"));
        assert_eq!(
            (ProductionGroupings.generation(), digest.as_str()),
            PINNED,
            "this build's grouping generation and the digest of the fixture's groupings (left) \
             differ from the pinned pair: a change to what the groupings hold ships with a bump, \
             so bump `GROUPINGS_GENERATION` and re-pin the pair to this build's values together"
        );
    }

    #[test]
    fn community_numbering_is_a_function_of_the_tree() {
        let view = fixture();
        let files: Vec<_> = view
            .files()
            .map(|f| view.file(f.path).unwrap().into_owned())
            .collect();
        let mut restamped = stamp();
        restamped.seq = 7;
        restamped.tip.head = "elsewhere".into();
        let reread = FactsView::new(restamped, files.into_iter().rev().collect());
        let first = ProductionGroupings.group(&view);
        assert!(!first.communities.is_empty(), "the fixture couples code");
        assert_eq!(
            first,
            ProductionGroupings.group(&reread),
            "another stamp and read order"
        );
        let smallest: Vec<&str> = first
            .communities
            .iter()
            .enumerate()
            .map(|(n, c)| {
                assert_eq!(c.id, format!("community/1/{n}"), "numbered in order");
                c.members.iter().min().unwrap().as_str()
            })
            .collect();
        assert!(
            smallest.windows(2).all(|w| w[0] < w[1]),
            "ordered by smallest member: {smallest:?}"
        );
    }

    #[test]
    fn concepts_cluster_the_tree_intent_layer_only() {
        let view = fixture();
        let tree = CodeGraph::new(&view, ViewFilter::default()).whole();
        let derived = derive(&tree, &intent_layer(&tree), DEFAULT_RESOLUTION);
        let concepts = ProductionGroupings.group(&view).concepts;
        let expected: Vec<(String, String, Vec<String>)> = derived
            .concepts
            .iter()
            .map(|(id, label)| {
                let members = derived
                    .members
                    .iter()
                    .filter(|(_, c)| c == id)
                    .map(|(n, _)| n.clone());
                (id.clone(), label.clone(), members.collect())
            })
            .collect();
        let got: Vec<(String, String, Vec<String>)> = concepts
            .iter()
            .map(|c| (c.id.clone(), c.label.clone(), c.members.clone()))
            .collect();
        assert_eq!(got, expected);
        assert_eq!(
            got,
            vec![(
                "concept/1/0".to_string(),
                "D".to_string(),
                vec!["docs/d.md".to_string(), "src/b.rs".to_string()]
            )]
        );
        assert!(concepts
            .iter()
            .all(|c| c.hash == derived.hash && c.resolution == DEFAULT_RESOLUTION));
    }

    /// Each row: a community and its label, the member with the most coupling edges as written,
    /// by its name, else its id.
    #[test]
    fn communities_are_labelled_by_their_most_coupled_member() {
        let communities = ProductionGroupings.group(&fixture()).communities;
        let label = |id: &str| {
            communities
                .iter()
                .find(|c| c.id == id)
                .map(|c| c.label.as_str())
        };
        let cases = [
            (
                "a file outranks the definition its callers reach through placeholders",
                "community/1/0",
                "scripts/t.py",
            ),
            (
                "a file node is labelled by its id",
                "community/1/1",
                "src/a.rs",
            ),
            (
                "a definition is labelled by its name",
                "community/1/3",
                "Config",
            ),
        ];
        for (name, id, want) in cases {
            assert_eq!(label(id), Some(want), "{name}: {id}");
        }
    }

    /// Each row: a node of the fixture and its degree in the coupling layer, none when it is not
    /// in the layer.
    #[test]
    fn community_coupling_reads_resolved_edges() {
        let view = fixture();
        let code = CodeGraph::new(&view, ViewFilter::default());
        let graph = code.whole();
        let layer = coupling(&code, &graph);
        let degree = |id: &str| {
            let at = layer.nodes().iter().position(|n| n == id)?;
            Some(layer.degrees()[at])
        };
        let cases = [
            (
                "a placeholder with one candidate is that definition",
                "src/a.rs::%compute",
                None,
            ),
            (
                "the definition takes its edges",
                "src/b.rs::compute",
                Some(5.0),
            ),
            (
                "a placeholder with a frontier stays itself",
                "src/a.rs::%draw",
                Some(2.0),
            ),
            (
                "an ambiguous edge couples nothing",
                "src/a.rs::%vanish",
                None,
            ),
            (
                "a module declaration couples nothing",
                "src/a.rs::sub",
                Some(1.0),
            ),
        ];
        for (name, id, want) in cases {
            assert_eq!(degree(id), want, "{name}: {id}");
        }
    }
}
