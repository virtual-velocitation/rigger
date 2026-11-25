//! The graph half: each term resolved against the code graph ([`ContextGraph::locate`] first,
//! then the name index's exact, prefix and fuzzy tiers) and answered with each entity's definition
//! site, its doc, its product referrers under each label and the sites of its direct callees. The
//! graph describes the caller's tree:
//! the files it differs in are read from the tree over the store's stamp. When the tree cannot be
//! compared, the graph withholds every code hit and counts it in its label
//! ([`ContextGraph::stamp_label`]), which heads the half, so the graph half never answers from
//! bytes the caller no longer holds; the text half answers from the tree's bytes.

use crate::contextgraph::ContextGraph;
use rigger_domain::composed::Label;

#[cfg(feature = "symbols")]
use super::budget::GRAPH_ENTITIES;
#[cfg(feature = "symbols")]
use super::names::{Held, Names, Resolved, Tier, NEAREST};
#[cfg(feature = "symbols")]
use super::normalise;
#[cfg(feature = "symbols")]
use crate::contextgraph::query::{node_file, node_name, split_id};
#[cfg(feature = "symbols")]
use crate::contextgraph::{
    entity_site, CallGraph, Direction, Located, Node, Referrer, KIND_CODE_ENTITY, REFERRER_LABELS,
    TIER_INFERRED,
};

/// Where a graph hit is defined.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Site {
    pub id: String,
    pub kind: String,
    /// The file and 1-based line, when the graph records them (line `0` is unknown).
    pub file: Option<(String, u32)>,
    /// Whether the site is test code, which a lookup that shows tests marks.
    pub test: bool,
}

/// One entity a term reached.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Entity {
    pub site: Site,
    /// Its doc comment, when the graph reads one ([`ContextGraph::doc`]).
    pub doc: Option<String>,
    /// Its product referrers ([`ContextGraph::referrers`]) under each label, in label order: each
    /// a referrer's enclosing site at the mention's own line, in path then source order.
    pub referrers: Vec<(&'static str, Vec<Site>)>,
    /// Its direct callees' definition sites.
    pub callees: Vec<Site>,
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
/// answered, so it counts every hit the answers withheld (none unless the caller's tree cannot be
/// compared).
#[derive(Debug, PartialEq)]
pub(crate) struct Answered {
    pub label: Label,
    pub terms: Vec<TermGraph>,
    /// Per term, how many test entities the graph's filter hides that the term reaches: the
    /// separate count of test hits beside the product hits `terms` holds.
    pub test_hits: Vec<usize>,
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

/// Each term of `terms` answered from `graph`, in order. The names are indexed whole; the
/// entities a name carries are read only when a term's search reaches the name, so the graph is
/// read near the terms and its label counts only the hits a term reached and the graph withheld.
#[cfg(feature = "symbols")]
pub(crate) fn answer(asked: &[&str], graph: &dyn ContextGraph) -> Result<Answered, String> {
    let read = |e: crate::contextgraph::Error| format!("the context graph could not be read: {e}");
    let names = Names::build(graph.entity_names().map_err(read)?);
    let ids = |nodes: Vec<Node>| -> Vec<String> { nodes.into_iter().map(|n| n.id).collect() };
    let shown = |name: &str| graph.entities_named(name).map(ids);
    let hidden = |name: &str| graph.hidden_tests_named(name).map(ids);
    let (mut shown, mut hidden) = (Held::new(&shown), Held::new(&hidden));
    let mut terms = Vec::with_capacity(asked.len());
    for term in asked {
        let (tier, ids) = match located(graph, term).map_err(read)? {
            Some(ids) => (Tier::Exact, ids),
            None => match names.resolve(term, &mut shown).map_err(read)? {
                Resolved::Found { tier, ids } => (tier, ids),
                Resolved::Missing { nearest } => {
                    terms.push(TermGraph::Missing { nearest });
                    continue;
                }
            },
        };
        let mut entities = Vec::new();
        for id in ids.iter().take(GRAPH_ENTITIES) {
            let down = graph
                .calls(std::slice::from_ref(id), Direction::Down, 1, TIER_INFERRED)
                .map_err(read)?;
            let seed = (down.nodes.iter())
                .find(|n| n.layer == 0 && n.node.id == *id)
                .map(|n| &n.node);
            let referrers = graph.referrers(id);
            entities.push(Entity {
                site: site_of(seed, id),
                doc: seed.and_then(|node| graph.doc(node)),
                referrers: (REFERRER_LABELS.iter())
                    .map(|(_, label)| {
                        let listed = referrers.iter().filter(|r| r.label == *label);
                        (*label, listed.map(referrer_site).collect())
                    })
                    .collect(),
                callees: neighbours(&down),
            });
        }
        terms.push(TermGraph::Found {
            tier: tier.label(),
            unread: ids.len() - entities.len(),
            reached: ids.len(),
            entities,
        });
    }
    let mut test_hits = Vec::with_capacity(asked.len());
    for term in asked {
        let found = names.found(term, &mut hidden).map_err(read)?;
        test_hits.push(found.map_or(0, |(_, ids)| ids.len()));
    }
    Ok(Answered {
        label: graph.stamp_label(),
        terms,
        test_hits,
    })
}

/// What `term`, which [`ContextGraph::locate`] resolves to nothing, is nearest to, as [`answer`]
/// resolves it: the first [`NEAREST`] ids the name index's variant, prefix or fuzzy tier reaches,
/// else the nearest names, none when the graph holds no near name. The names are indexed whole and
/// an entity is read only when the search reaches its name.
#[cfg(feature = "symbols")]
pub(crate) fn nearest(
    term: &str,
    graph: &dyn ContextGraph,
) -> Result<Option<Vec<String>>, crate::contextgraph::Error> {
    let names = Names::build(graph.entity_names()?);
    let shown = |name: &str| {
        (graph.entities_named(name)).map(|nodes| nodes.into_iter().map(|n| n.id).collect())
    };
    Ok(Some(match names.resolve(term, &mut Held::new(&shown))? {
        Resolved::Found { ids, .. } => ids.into_iter().take(NEAREST).collect(),
        Resolved::Missing { nearest } => nearest,
    }))
}

/// This build extracts no code, so its graph holds no code entity to be near.
#[cfg(not(feature = "symbols"))]
pub(crate) fn nearest(
    _term: &str,
    _graph: &dyn ContextGraph,
) -> Result<Option<Vec<String>>, crate::contextgraph::Error> {
    Ok(None)
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

/// The site of the node `id`, `seeded` its node as a walk read it: that node's, when it is a code
/// entity a name indexes; else the bare `id` under the kind the walk read it as (a file, a
/// community), a code entity when the walk holds no seed.
#[cfg(feature = "symbols")]
fn site_of(seeded: Option<&Node>, id: &str) -> Site {
    match seeded {
        Some(node)
            if node.kind == KIND_CODE_ENTITY
                && node_name(node).is_some_and(|m| !normalise(m).is_empty()) =>
        {
            site(node)
        }
        _ => {
            let kind = seeded.map_or(KIND_CODE_ENTITY, |n| n.kind.as_str());
            Site {
                id: id.to_string(),
                kind: kind.to_string(),
                file: (node_file(kind, id).filter(|f| *f != id)).map(|f| (f.to_string(), 0)),
                test: false,
            }
        }
    }
}

/// The site of `referrer`: its enclosing definition, or its file, at the mention's own line.
#[cfg(feature = "symbols")]
fn referrer_site(referrer: &Referrer) -> Site {
    Site {
        id: referrer.site.clone(),
        kind: referrer.kind.clone(),
        file: Some((split_id(&referrer.site).0.to_string(), referrer.line)),
        test: referrer.test,
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
        test: node.attrs.contains_key("test"),
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
