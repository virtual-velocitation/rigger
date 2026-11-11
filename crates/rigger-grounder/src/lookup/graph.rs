//! The graph half: each term resolved against the code graph ([`Projection::locate`] first, then
//! the name index's exact, prefix and fuzzy tiers) and answered with each entity's definition site
//! and the sites of its direct callers and callees. The graph is not the caller's tree: every hit
//! whose file's bytes in the caller's tree are not the bytes the graph ingested (the generation
//! those bytes extract to differs from the one the graph holds for the file, the file absent
//! included) is withheld and its file named, so the graph half never answers from bytes the
//! caller no longer holds. It is filtered, not worktree-true: what the graph lacks for the tree's
//! new bytes the text half answers.

use crate::contextgraph::{Projection, REBUILD_OWED};
#[cfg(feature = "symbols")]
use std::collections::{BTreeMap, BTreeSet};
#[cfg(feature = "symbols")]
use std::path::Path;

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
    /// The term reached entities: the first [`GRAPH_ENTITIES`] it reached whose files the caller's
    /// tree holds as the graph does, how many it reached in all, how many of those were never read
    /// once that many were kept, and the files of its hits that were withheld.
    Found {
        tier: &'static str,
        entities: Vec<Entity>,
        reached: usize,
        unread: usize,
        withheld: Vec<String>,
    },
    /// No entity: the nearest names.
    Missing { nearest: Vec<String> },
}

/// The graph to answer from, or the reason there is none to answer from: the caller has none, it
/// owes its rebuild, or it cannot say whether it does.
pub(crate) fn usable(graph: Result<&dyn Projection, String>) -> Result<&dyn Projection, String> {
    let graph = graph?;
    match graph.rebuild_owed() {
        Ok(false) => Ok(graph),
        Ok(true) => Err(REBUILD_OWED.to_string()),
        Err(e) => Err(format!("the context graph could not be read: {e}")),
    }
}

/// This build extracts no code, so its graph holds no code entity to answer from.
#[cfg(not(feature = "symbols"))]
pub(crate) fn answer(
    _root: &std::path::Path,
    _terms: &[&str],
    _graph: &dyn Projection,
) -> Result<Vec<TermGraph>, String> {
    Err(
        "this rigger build carries no code extraction (the `symbols` feature), so its graph \
         holds no code entity"
            .to_string(),
    )
}

/// Each term of `terms` answered from `graph`, in order, every hit checked against the tree at
/// `root`.
#[cfg(feature = "symbols")]
pub(crate) fn answer(
    root: &Path,
    terms: &[&str],
    graph: &dyn Projection,
) -> Result<Vec<TermGraph>, String> {
    let read = |e: crate::contextgraph::Error| format!("the context graph could not be read: {e}");
    let names = Names::build(graph.nodes_of_kind(KIND_CODE_ENTITY).map_err(read)?);
    let mut fresh = Freshness::new(root, graph);
    terms
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
            let mut withheld = BTreeSet::new();
            let mut examined = 0;
            for id in &ids {
                if entities.len() >= GRAPH_ENTITIES {
                    break;
                }
                examined += 1;
                let site = site_of(names.node(id), id);
                if !fresh.holds(&site, &mut withheld) {
                    continue;
                }
                let up = graph
                    .calls(std::slice::from_ref(id), Direction::Up, 1, TIER_INFERRED)
                    .map_err(read)?;
                let down = graph
                    .calls(std::slice::from_ref(id), Direction::Down, 1, TIER_INFERRED)
                    .map_err(read)?;
                entities.push(Entity {
                    site,
                    callers: fresh.kept(neighbours(&up), &mut withheld),
                    callees: fresh.kept(neighbours(&down), &mut withheld),
                    referenced_in: up
                        .referenced_not_called
                        .iter()
                        .filter_map(|n| node_file(&n.kind, &n.id).map(str::to_string))
                        .filter(|file| fresh.file_holds(file, &mut withheld))
                        .collect(),
                });
            }
            Ok(TermGraph::Found {
                tier: tier.label(),
                entities,
                reached: ids.len(),
                unread: ids.len() - examined,
                withheld: withheld.into_iter().collect(),
            })
        })
        .collect()
}

/// The ids [`Projection::locate`] resolves `term` to, `None` when it resolves none.
#[cfg(feature = "symbols")]
fn located(
    graph: &dyn Projection,
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

/// Whether each file's bytes in the caller's tree are the bytes the graph ingested, each file
/// checked once per lookup: the generation the tree's bytes extract to, under the exclusion flag
/// clear (a file the walk excludes is ingested with no entity, so a file holding a hit was
/// ingested with the flag clear), against the generation the graph holds for it. A file the tree
/// does not hold never holds a hit: a whole-project walk retires such a file by folding the batch
/// for no bytes, the very generation its absence extracts to, and a node a record still names
/// stays in the graph at that file.
#[cfg(feature = "symbols")]
struct Freshness<'a> {
    root: &'a Path,
    graph: &'a dyn Projection,
    checked: BTreeMap<String, bool>,
}

#[cfg(feature = "symbols")]
impl<'a> Freshness<'a> {
    fn new(root: &'a Path, graph: &'a dyn Projection) -> Self {
        Freshness {
            root,
            graph,
            checked: BTreeMap::new(),
        }
    }

    /// Whether `file` is held as the graph holds it; when not, it joins `withheld`.
    fn file_holds(&mut self, file: &str, withheld: &mut BTreeSet<String>) -> bool {
        let (root, graph) = (self.root, self.graph);
        let holds = *self.checked.entry(file.to_string()).or_insert_with(|| {
            let held = graph.current_generation(&crate::ingest::code_identity(file));
            crate::grounder::tree_holds(root, "gc", file)
                && matches!(held, Ok(Some(held)) if held == crate::ingest::tree_generation(root, file, false))
        });
        if !holds {
            withheld.insert(file.to_string());
        }
        holds
    }

    /// Whether `site`'s file, if it names one, is held as the graph holds it.
    fn holds(&mut self, site: &Site, withheld: &mut BTreeSet<String>) -> bool {
        match &site.file {
            Some((file, _)) => self.file_holds(file, withheld),
            None => true,
        }
    }

    /// The sites of `sites` whose files are held as the graph holds them.
    fn kept(&mut self, sites: Vec<Site>, withheld: &mut BTreeSet<String>) -> Vec<Site> {
        sites
            .into_iter()
            .filter(|site| self.holds(site, withheld))
            .collect()
    }
}
