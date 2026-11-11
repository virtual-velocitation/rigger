//! The graph half: each term resolved against the code graph ([`ContextGraph::locate`] first,
//! then the name index's exact, prefix and fuzzy tiers) and answered with each entity's definition
//! site and the sites of its direct callers and callees. The graph is not the caller's tree: the
//! graph withholds every hit in a file the caller's tree holds differently and counts it in its
//! label ([`ContextGraph::stamp_label`]), which heads the half, so the graph half never answers
//! from bytes the caller no longer holds. What the graph lacks for the tree's new bytes the text
//! half answers.

use crate::contextgraph::ContextGraph;
use rigger_domain::composed::Label;

#[cfg(feature = "symbols")]
use super::budget::GRAPH_ENTITIES;
#[cfg(feature = "symbols")]
use super::names::{Names, Resolved, Tier};
#[cfg(feature = "symbols")]
use crate::contextgraph::query::node_file;
#[cfg(feature = "symbols")]
use crate::contextgraph::{
    entity_site, CallGraph, Direction, Located, Node, KIND_CODE_ENTITY, TIER_INFERRED,
};

/// Where a graph hit is defined.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Site {
    pub id: String,
    pub kind: String,
    /// The file and 1-based line, when the graph records them (line `0` is unknown).
    pub file: Option<(String, u32)>,
}

/// One entity a term reached.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Entity {
    pub site: Site,
    /// Its direct callers' definition sites.
    pub callers: Vec<Site>,
    /// Its direct callees' definition sites.
    pub callees: Vec<Site>,
    /// The files that reference it without calling it.
    pub referenced_in: Vec<String>,
}

/// How a term resolved against the graph. A build without code extraction refuses the graph half
/// before any term resolves, so outside its tests it constructs neither variant.
#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(
    all(not(feature = "symbols"), not(test)),
    expect(
        dead_code,
        reason = "only the symbols build resolves a term against the graph"
    )
)]
pub(crate) enum TermGraph {
    /// The term reached entities: the first [`GRAPH_ENTITIES`] it reached, how many it reached
    /// in all, and how many of those were never read once that many were kept.
    Found {
        tier: &'static str,
        entities: Vec<Entity>,
        reached: usize,
        unread: usize,
    },
    /// No entity: the nearest names.
    Missing { nearest: Vec<String> },
}

/// The graph half's answer: each term's, and the graph's label as it stands once every term was
/// answered, so it counts every hit the answers withheld.
#[derive(Debug, PartialEq)]
pub(crate) struct Answered {
    pub label: Label,
    pub terms: Vec<TermGraph>,
}

/// This build extracts no code, so its graph holds no code entity to answer from.
#[cfg(not(feature = "symbols"))]
pub(crate) fn answer(_terms: &[&str], _graph: &dyn ContextGraph) -> Result<Answered, String> {
    Err(
        "this rigger build carries no code extraction (the `symbols` feature), so its graph \
         holds no code entity"
            .to_string(),
    )
}

/// Each term of `terms` answered from `graph`, in order.
#[cfg(feature = "symbols")]
pub(crate) fn answer(terms: &[&str], graph: &dyn ContextGraph) -> Result<Answered, String> {
    let read = |e: crate::contextgraph::Error| format!("the context graph could not be read: {e}");
    let names = Names::build(graph.nodes_of_kind(KIND_CODE_ENTITY).map_err(read)?);
    let terms = terms
        .iter()
        .map(|term| {
            let (tier, ids) = match located(graph, term).map_err(read)? {
                Some(ids) => (Tier::Exact, ids),
                None => match names.resolve(term) {
                    Resolved::Found { tier, ids } => (tier, ids),
                    Resolved::Missing { nearest } => return Ok(TermGraph::Missing { nearest }),
                },
            };
            let mut entities = Vec::new();
            for id in ids.iter().take(GRAPH_ENTITIES) {
                let up = graph
                    .calls(std::slice::from_ref(id), Direction::Up, 1, TIER_INFERRED)
                    .map_err(read)?;
                let down = graph
                    .calls(std::slice::from_ref(id), Direction::Down, 1, TIER_INFERRED)
                    .map_err(read)?;
                entities.push(Entity {
                    site: site_of(names.node(id), id),
                    callers: neighbours(&up),
                    callees: neighbours(&down),
                    referenced_in: up
                        .referenced_not_called
                        .iter()
                        .filter_map(|n| node_file(&n.kind, &n.id).map(str::to_string))
                        .collect(),
                });
            }
            Ok(TermGraph::Found {
                tier: tier.label(),
                unread: ids.len() - entities.len(),
                reached: ids.len(),
                entities,
            })
        })
        .collect::<Result<_, String>>()?;
    Ok(Answered {
        label: graph.stamp_label(),
        terms,
    })
}

/// The ids [`ContextGraph::locate`] resolves `term` to, `None` when it resolves none.
#[cfg(feature = "symbols")]
fn located(
    graph: &dyn ContextGraph,
    term: &str,
) -> Result<Option<Vec<String>>, crate::contextgraph::Error> {
    Ok(match graph.locate(term)? {
        Located::One(site) => Some(vec![site.id]),
        Located::Many(candidates) => Some(candidates.into_iter().map(|c| c.id).collect()),
        Located::None => None,
    })
}

/// The site of `node`, or of the bare `id` when the name index does not hold it.
#[cfg(feature = "symbols")]
fn site_of(node: Option<&Node>, id: &str) -> Site {
    match node {
        Some(node) => site(node),
        None => Site {
            id: id.to_string(),
            kind: KIND_CODE_ENTITY.to_string(),
            file: node_file(KIND_CODE_ENTITY, id).map(|f| (f.to_string(), 0)),
        },
    }
}

/// The site [`entity_site`] reads off `node`.
#[cfg(feature = "symbols")]
fn site(node: &Node) -> Site {
    let site = entity_site(node, 0);
    Site {
        id: site.id,
        kind: site.kind,
        file: site.file.map(|file| (file, site.line)),
    }
}

/// The sites of `walk`'s first layer: the seed's direct neighbours in the walk's direction (the
/// seed itself is layer 0, a recursive call to it included).
#[cfg(feature = "symbols")]
fn neighbours(walk: &CallGraph) -> Vec<Site> {
    walk.nodes
        .iter()
        .filter(|n| n.layer == 1)
        .map(|n| site(&n.node))
        .collect()
}
