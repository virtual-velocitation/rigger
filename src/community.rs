//! Deterministic coupling-community detection (spec 53, the CODE lens): the OFFLINE pass that
//! groups code entities and files by how densely they call and reference one another - the
//! "subsystem" a maintainer holds in their head - regardless of directory. It reads the live
//! coupling layer (the `CALLS` / `REFERENCES` / `CONTAINS` edges among code-entity / file nodes)
//! already folded into the projection, runs modularity-based community detection over it, and
//! turns the result into `CommunityAssigned` events. The fold (the always-compiled projector arm)
//! turns each event into an `IN_COMMUNITY` membership edge, so the derived grouping is EVENT-SOURCED
//! (a rebuildable projection over the log), never computed at request time.
//!
//! ALWAYS compiled, exactly like the fold it feeds. Detection reads only the folded coupling edges
//! and needs no grammar, so it carries no `symbols` gate: the pass, its determinism, and its
//! connectedness are proven identically in BOTH feature lanes. The event type, node kind, and
//! relation live in [`crate::contextgraph`]; this module is the one WRITER of `CommunityAssigned`.
//!
//! # Determinism (a hard requirement, not a wish)
//!
//! The same coupling graph at the same resolution yields BYTE-IDENTICAL assignments on every run
//! and every machine, so a rebuild from the recorded events reproduces the same membership rows:
//!
//! - Nodes are processed in SORTED node-id order (the coupling graph's node vector is sorted, and a
//!   node's index IS its rank in that order, so an index loop is an id-sorted loop).
//! - A community's identity for tie-breaking is its lexicographically-SMALLEST member id (its
//!   min-index member); ties in modularity gain break to the smallest such representative.
//! - Final community numbers are assigned by ascending representative, so `community/<res>/0` is
//!   always the community holding the lexicographically-smallest node.
//! - No randomness and no hash-set iteration reaches the output: every accumulator is a `BTreeMap` /
//!   `BTreeSet` or an index-ordered `Vec`, so float sums and label choices are order-stable.
//!
//! # Algorithm
//!
//! Modularity-based detection in two phases, exactly as the spec frames it - "local moving with a
//! refinement pass so every community is internally connected":
//!
//! 1. LOCAL MOVING. Every node starts in its own community. Repeatedly, in sorted-id order, each
//!    node is removed from its community and re-inserted into the neighboring community that most
//!    increases modularity `Q = (1/2m) Σ [A_ij - r·k_i·k_j/2m] δ(c_i,c_j)` at resolution `r` (higher
//!    `r` penalizes large communities more, yielding more and smaller ones). A move is made ONLY on
//!    a strict improvement over staying put, so modularity rises monotonically and the pass
//!    terminates; ties among equally-best target communities break to the smallest representative.
//! 2. CONNECTEDNESS REFINEMENT. Local moving can leave a community internally disconnected (the
//!    classic modularity-optimization defect); the refinement splits every community into its
//!    intra-community connected components, so each final community is internally CONNECTED by
//!    construction.

use std::collections::{BTreeMap, BTreeSet};

use crate::contextgraph::{
    CommunityAssigned, Graph, KIND_CODE_ENTITY, REL_CALLS, REL_CONTAINS, REL_REFERENCES,
    TIER_AMBIGUOUS, TYPE_COMMUNITY_ASSIGNED,
};
use crate::eventstore::Event;

/// The default resolution grain (spec 53): 1.0. Higher resolutions yield more and smaller
/// communities; the grain knob's SEMANTICS (monotonicity, coexisting grains) are a later criterion's
/// to prove, but the algorithm is parameterized by it here.
pub const DEFAULT_RESOLUTION: f64 = 1.0;

/// The strict-improvement threshold a local move must clear. Real modularity gains from
/// integer-weight coupling edges differ by at least `~1/2m` (far above this), while floating-point
/// noise sits far below it, so this cleanly separates a genuine improvement from numeric zero -
/// which both keeps the output stable across machines and guarantees termination (each accepted move
/// raises modularity by more than `EPS`, and modularity is bounded above).
const EPS: f64 = 1e-9;

// FNV-1a: the crate's stable, fixed-seed, dependency-free content-hash discipline (the SAME 64-bit
// constants `registry`, `grounder::symbols::store`, and the run's `definition_hash` use). An ungated
// module cannot call the feature-gated copies, so - per the recorded `arch-u2i-fnv1a-fourth-parallel-copy`
// note that a single shared primitive is a separate cross-cutting refactor - it keeps its own copy
// matching the algorithm, never `DefaultHasher` (whose seed the stdlib does not guarantee stable).
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// A stable 16-hex-digit FNV-1a of `s`, byte-identical across processes, builds, and machines.
fn fnv1a_hex(s: &str) -> String {
    let mut h = FNV_OFFSET;
    for &b in s.as_bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(FNV_PRIME);
    }
    format!("{h:016x}")
}

/// The "simple name" of a code-entity id: the text after the FIRST `::` (the `<name>` of a
/// `<file>::<name>` id). This is the pinned `substr(id, instr(id,'::')+2)` name-suffix the fold's
/// own cross-file resolution keys on, so a bare placeholder and the definition it reaches share it.
/// `None` for an id with no `::` (a file or non-code node), which never resolves.
fn simple_name(id: &str) -> Option<&str> {
    id.find("::").map(|pos| &id[pos + 2..])
}

/// An undirected, weighted coupling graph built from the projection's structure layer: the input
/// community detection runs over. Its nodes are exactly the code-entity / file ids that participate
/// in at least one coupling edge (a node with none is not part of any community and simply keeps its
/// kind bucket in the lens), stored SORTED so a node's index is its id rank. Edges collapse each
/// unordered pair of the live `CALLS` / `REFERENCES` / `CONTAINS` edges into ONE undirected edge
/// weighted by multiplicity; self-loops and the unresolved [`TIER_AMBIGUOUS`] tier are dropped
/// (the spec's "resolvable tiers only, weighted by edge multiplicity").
pub struct Coupling {
    /// Sorted, unique node ids; `nodes[i]` is the id of internal index `i`.
    nodes: Vec<String>,
    /// Undirected adjacency by index: `adj[i]` holds `(j, weight)` for each neighbor `j`, sorted by
    /// `j`. Each undirected edge appears in BOTH endpoints' lists.
    adj: Vec<Vec<(usize, f64)>>,
    /// Weighted degree per node (`deg[i]` = Σ weights incident to `i`).
    deg: Vec<f64>,
    /// `2m`, the total edge weight doubled (Σ of every `deg`), the modularity normalizer.
    m2: f64,
    /// A canonical, deterministic rendering of the weighted edge set - the content the pass hash is
    /// taken over (together with the resolution), so the recorded `hash` identifies the input.
    canon: String,
}

impl Coupling {
    /// Build the coupling graph from the WHOLE projection graph (as [`crate::contextgraph::sqlite::Projector::whole`]
    /// returns it - live edges only, project-scoped, sorted). Keeps only the `CALLS` / `REFERENCES`
    /// / `CONTAINS` structural edges at resolvable tiers (never [`TIER_AMBIGUOUS`]), collapses each
    /// unordered node pair to one undirected edge weighted by how many such edges join it, and drops
    /// self-loops. A node with no surviving coupling edge is excluded (it joins no community).
    pub fn from_graph(g: &Graph) -> Self {
        // Index real DEFINITIONS by simple name so a cross-file reference that folded onto a bare
        // same-file placeholder can be redirected to the definition it actually reaches. A
        // code-entity node carrying a `name` attr is a real definition (the extraction fold sets
        // it); a code-entity with NO `name` attr is a bare cross-file placeholder. The "simple
        // name" is the text after the FIRST `::` - the pinned `substr(id, instr(id,'::')+2)`
        // name-suffix the fold's own cross-file resolution keys on.
        let mut defs_by_name: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        let mut bare: BTreeMap<&str, bool> = BTreeMap::new();
        for node in &g.nodes {
            let is_code = node.kind == KIND_CODE_ENTITY;
            let named = node.attrs.contains_key("name");
            bare.insert(node.id.as_str(), is_code && !named);
            if is_code && named {
                if let Some(simple) = simple_name(&node.id) {
                    defs_by_name.entry(simple).or_default().push(&node.id);
                }
            }
        }
        // Canonicalize a node id: a bare cross-file placeholder resolves to the UNIQUE real
        // definition sharing its simple name (EXACTLY ONE resolves; ZERO or MANY stays itself,
        // honest by construction) - the SAME unique-name-suffix rule the directed calls walk
        // (spec 52) applies at query time, here over the in-memory whole-graph read so the
        // undirected coupling layer genuinely spans files instead of stopping at same-file bare
        // placeholders. Every other node (a real definition, a file, a non-code node) is itself.
        let canon = |id: &str| -> String {
            if *bare.get(id).unwrap_or(&false) {
                if let Some(simple) = simple_name(id) {
                    if let Some(defs) = defs_by_name.get(simple) {
                        if defs.len() == 1 {
                            return defs[0].to_string();
                        }
                    }
                }
            }
            id.to_string()
        };

        // Accumulate undirected pair weights over CANONICAL endpoints in a BTreeMap so the edge
        // iteration order - and thus the canonical hash string and every downstream float sum - is
        // deterministic.
        let mut pair_w: BTreeMap<(String, String), f64> = BTreeMap::new();
        for e in &g.edges {
            let coupling = e.rel == REL_CALLS || e.rel == REL_REFERENCES || e.rel == REL_CONTAINS;
            if !coupling || e.tier == TIER_AMBIGUOUS {
                continue;
            }
            let (from, to) = (canon(&e.from), canon(&e.to));
            // Drop self-loops AFTER resolution (a same-file reference onto its own caller, or a bare
            // placeholder that resolved back onto its file-mate).
            if from == to {
                continue;
            }
            // Canonicalize the pair so `a -> b` and `b -> a` collapse to one undirected edge.
            let pair = if from <= to { (from, to) } else { (to, from) };
            *pair_w.entry(pair).or_insert(0.0) += 1.0;
        }

        let mut node_set: BTreeSet<&String> = BTreeSet::new();
        for (a, b) in pair_w.keys() {
            node_set.insert(a);
            node_set.insert(b);
        }
        let nodes: Vec<String> = node_set.into_iter().cloned().collect();
        let index: BTreeMap<&str, usize> = nodes
            .iter()
            .enumerate()
            .map(|(i, s)| (s.as_str(), i))
            .collect();

        let n = nodes.len();
        let mut adj: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
        let mut deg = vec![0.0_f64; n];
        let mut canon = String::new();
        for ((a, b), w) in &pair_w {
            let (ia, ib) = (index[a.as_str()], index[b.as_str()]);
            adj[ia].push((ib, *w));
            adj[ib].push((ia, *w));
            deg[ia] += *w;
            deg[ib] += *w;
            canon.push_str(a);
            canon.push('\t');
            canon.push_str(b);
            canon.push('\t');
            canon.push_str(&format!("{w}"));
            canon.push('\n');
        }
        // The pair map is sorted by (a, b), so each `adj[i]` accumulates neighbors in a stable but
        // not j-sorted order; sort each so the neighbor walk is index-ordered and deterministic.
        for a in &mut adj {
            a.sort_by_key(|x| x.0);
        }
        let m2: f64 = deg.iter().sum();
        Coupling {
            nodes,
            adj,
            deg,
            m2,
            canon,
        }
    }

    /// The number of nodes participating in the coupling graph.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the coupling graph has no nodes (no coupling edges anywhere) - detection yields no
    /// assignment and the pass records nothing.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

/// A deterministic community assignment produced by [`detect`]: every coupling-graph member mapped
/// to its `community/<resolution>/<n>` id, plus the pass's `resolution`, content `hash`, and the
/// number of distinct communities. `members` is sorted by node id, so the first entry is always the
/// lexicographically-smallest node - the one whose `CommunityAssigned` event carries the pass
/// boundary [`CommunityAssigned::fresh`].
pub struct Assignment {
    /// The resolution grain this pass ran at.
    pub resolution: f64,
    /// The pass's content hash (FNV-1a of the canonical coupling edges plus the resolution).
    pub hash: String,
    /// `(node_id, community_id)` for every member, sorted by node id.
    pub members: Vec<(String, String)>,
    /// How many distinct communities the assignment holds.
    pub num_communities: usize,
}

/// Run deterministic modularity-based community detection over `coupling` at `resolution`, returning
/// a connected [`Assignment`]. The result is a pure function of the input: two calls on the same
/// coupling graph at the same resolution return equal assignments, and every community is internally
/// connected. An empty coupling graph yields an empty assignment.
pub fn detect(coupling: &Coupling, resolution: f64) -> Assignment {
    let res_str = format!("{resolution}");
    // The pass hash covers the canonical edge set AND the resolution, so distinct grains hash apart.
    let hash = fnv1a_hex(&format!("{}\n{res_str}", coupling.canon));

    let moved = local_moving(coupling, resolution);
    let refined = refine_connected(coupling, &moved);
    let community_ids = number_communities(coupling, &refined, &res_str);

    let members: Vec<(String, String)> = coupling
        .nodes
        .iter()
        .cloned()
        .zip(community_ids.iter().cloned())
        .collect();
    let num_communities = community_ids.iter().collect::<BTreeSet<_>>().len();
    Assignment {
        resolution,
        hash,
        members,
        num_communities,
    }
}

/// The `CommunityAssigned` events recording an [`Assignment`], in sorted-node-id order, with the
/// FIRST event carrying `fresh: true` (the pass boundary the fold supersedes the resolution grain's
/// prior memberships on - so a re-run REPLACES this grain's assignment set). Appending and folding
/// these events materializes the community layer; a rebuild replays them to byte-identical rows. An
/// empty assignment yields no events.
pub fn events(assignment: &Assignment) -> Vec<Event> {
    assignment
        .members
        .iter()
        .enumerate()
        .map(|(i, (node, community))| {
            let payload = CommunityAssigned {
                node: node.clone(),
                community: community.clone(),
                resolution: assignment.resolution,
                hash: assignment.hash.clone(),
                fresh: i == 0,
            };
            let data = serde_json::to_vec(&payload).expect("CommunityAssigned always serializes");
            Event::new(TYPE_COMMUNITY_ASSIGNED, data)
        })
        .collect()
}

/// Phase 1 - local moving. Returns `comm`, mapping each node index to an (arbitrary-integer)
/// community label. Each node begins alone; nodes are visited in sorted-id (index) order and moved
/// into the neighboring community of greatest modularity gain, only on a strict improvement over
/// staying, with ties broken to the smallest-representative community. Terminates because each
/// accepted move raises modularity by more than [`EPS`] and modularity is bounded.
fn local_moving(c: &Coupling, resolution: f64) -> Vec<usize> {
    let n = c.len();
    let mut comm: Vec<usize> = (0..n).collect();
    if c.m2 == 0.0 {
        // No edges: every node is its own (trivially connected) community.
        return comm;
    }
    // Per-label membership (a BTreeSet so its min element - the representative - is O(1) to read) and
    // total incident weight. Labels 0..n start as singletons; a node that chooses solitude while its
    // old community still has members takes a fresh label appended past `n`.
    let mut members: Vec<BTreeSet<usize>> = (0..n).map(|i| BTreeSet::from([i])).collect();
    let mut tot: Vec<f64> = c.deg.clone();

    let mut improved = true;
    while improved {
        improved = false;
        for i in 0..n {
            let ci = comm[i];
            // Weight from `i` into each neighboring community (sorted keys => deterministic sums).
            let mut wsum: BTreeMap<usize, f64> = BTreeMap::new();
            for &(j, w) in &c.adj[i] {
                if j != i {
                    *wsum.entry(comm[j]).or_insert(0.0) += w;
                }
            }
            // Remove `i` from its community before scoring insertions.
            members[ci].remove(&i);
            tot[ci] -= c.deg[i];

            // Gain of staying (re-inserting into `ci`): 0 if `ci` is now empty (that IS the isolated
            // option), else its edge weight minus the resolution-scaled degree penalty.
            let gain_ci = if members[ci].is_empty() {
                0.0
            } else {
                wsum.get(&ci).copied().unwrap_or(0.0) - resolution * c.deg[i] * tot[ci] / c.m2
            };

            // Best candidate over neighboring communities and the isolated option (gain 0, the
            // singleton whose representative is `i` itself). Maximize gain, then minimize the
            // community representative (its lexicographically-smallest member id).
            let mut best_comm = usize::MAX; // sentinel: a fresh singleton (isolated)
            let mut best_gain = 0.0_f64;
            let mut best_rep = i;
            for (&cand, &w_to) in &wsum {
                let Some(&rep) = members[cand].iter().next() else {
                    continue; // `ci` emptied by the removal - its option is the isolated baseline
                };
                let gain = w_to - resolution * c.deg[i] * tot[cand] / c.m2;
                if gain > best_gain + EPS || ((gain - best_gain).abs() <= EPS && rep < best_rep) {
                    best_gain = gain;
                    best_rep = rep;
                    best_comm = cand;
                }
            }

            // Move ONLY on a strict improvement over staying; otherwise `i` returns to `ci`
            // unchanged. This makes zero-gain ties never move (no thrashing) and guarantees the pass
            // converges.
            if best_gain > gain_ci + EPS {
                let target = if best_comm == usize::MAX {
                    // Isolated wins: reuse `ci` if the removal emptied it, else a fresh label.
                    if members[ci].is_empty() {
                        ci
                    } else {
                        members.push(BTreeSet::new());
                        tot.push(0.0);
                        members.len() - 1
                    }
                } else {
                    best_comm
                };
                members[target].insert(i);
                tot[target] += c.deg[i];
                if target != ci {
                    comm[i] = target;
                    improved = true;
                }
            } else {
                // Stay: put `i` back where it was.
                members[ci].insert(i);
                tot[ci] += c.deg[i];
            }
        }
    }
    comm
}

/// Phase 2 - connectedness refinement. Splits every community into its intra-community connected
/// components, so each returned label denotes an internally CONNECTED set. A component is discovered
/// by a walk that only crosses edges whose endpoints share a community; nodes are seeded in index
/// order, so the labeling is deterministic. Every node is labeled (a node with no intra-community
/// neighbor is its own trivially-connected component).
fn refine_connected(c: &Coupling, comm: &[usize]) -> Vec<usize> {
    let n = c.len();
    let mut refined = vec![usize::MAX; n];
    let mut visited = vec![false; n];
    let mut next_label = 0usize;
    for start in 0..n {
        if visited[start] {
            continue;
        }
        let label = next_label;
        next_label += 1;
        let home = comm[start];
        let mut stack = vec![start];
        visited[start] = true;
        refined[start] = label;
        while let Some(u) = stack.pop() {
            for &(v, _w) in &c.adj[u] {
                if !visited[v] && comm[v] == home {
                    visited[v] = true;
                    refined[v] = label;
                    stack.push(v);
                }
            }
        }
    }
    refined
}

/// Assign the final `community/<res>/<n>` id to every node, numbering communities by ASCENDING
/// representative (their lexicographically-smallest member id, i.e. smallest member index). So
/// `community/<res>/0` always holds the lexicographically-smallest node overall - a byte-stable
/// numbering independent of the intermediate labels the two phases produced.
fn number_communities(c: &Coupling, refined: &[usize], res_str: &str) -> Vec<String> {
    let n = c.len();
    // Smallest member index per refined label (== lexicographically-smallest id, nodes being sorted).
    let mut rep: BTreeMap<usize, usize> = BTreeMap::new();
    for (i, &label) in refined.iter().enumerate().take(n) {
        rep.entry(label)
            .and_modify(|m| {
                if i < *m {
                    *m = i;
                }
            })
            .or_insert(i);
    }
    // Order labels by ascending representative, then assign 0,1,2,... in that order.
    let mut order: Vec<(usize, usize)> = rep.iter().map(|(&label, &mi)| (mi, label)).collect();
    order.sort_unstable();
    let mut label_to_n: BTreeMap<usize, usize> = BTreeMap::new();
    for (n_idx, (_mi, label)) in order.into_iter().enumerate() {
        label_to_n.insert(label, n_idx);
    }
    refined
        .iter()
        .map(|label| format!("community/{res_str}/{}", label_to_n[label]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contextgraph::{Edge, Node, KIND_FILE, TIER_EXTRACTED, TIER_INFERRED};
    use std::collections::BTreeMap as Map;

    /// Build one graph [`Node`] of a given kind, with an optional `name` attr (present => a real
    /// definition; absent => a bare cross-file placeholder or a file node).
    fn node(id: &str, kind: &str, name: Option<&str>) -> Node {
        let mut attrs = Map::new();
        if let Some(n) = name {
            attrs.insert("name".to_string(), n.to_string());
        }
        Node {
            id: id.to_string(),
            kind: kind.to_string(),
            attrs,
        }
    }

    /// Build one graph [`Edge`] of a given rel and tier.
    fn edge(from: &str, to: &str, rel: &str, tier: &str) -> Edge {
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

    /// Build a coupling `Graph` from a list of undirected structural edges `(from, to, rel, tier)`,
    /// so a test can express a coupling topology directly without folding events. Nodes are inferred
    /// from the endpoints. Attrs are irrelevant to detection, so left empty.
    fn graph(edges: &[(&str, &str, &str, &str)]) -> Graph {
        let mut node_ids: BTreeSet<String> = BTreeSet::new();
        let mut es = Vec::new();
        for (from, to, rel, tier) in edges {
            node_ids.insert((*from).to_string());
            node_ids.insert((*to).to_string());
            es.push(Edge {
                from: (*from).to_string(),
                to: (*to).to_string(),
                rel: (*rel).to_string(),
                valid_from: 0,
                valid_to: None,
                source: 0,
                tier: (*tier).to_string(),
            });
        }
        let nodes = node_ids
            .into_iter()
            .map(|id| {
                // A real definition carries a `name` attr (the extraction fold sets it); the fixture
                // makes every endpoint a real definition so canonicalization is a no-op here and the
                // topology under test is exactly the edges given. Cross-file BARE resolution has its
                // own dedicated test.
                let mut attrs = Map::new();
                if let Some(simple) = super::simple_name(&id) {
                    attrs.insert("name".to_string(), simple.to_string());
                }
                Node {
                    id,
                    kind: "code-entity".to_string(),
                    attrs,
                }
            })
            .collect();
        Graph { nodes, edges: es }
    }

    /// The community id each node landed in, as a sorted `id -> community` map, for clear asserts.
    fn assignment_map(a: &Assignment) -> BTreeMap<String, String> {
        a.members.iter().cloned().collect()
    }

    /// A complete graph (clique) on `nodes` as `CALLS` edges at the extracted tier - a maximally
    /// dense cluster local moving must keep whole.
    fn clique(nodes: &[&str]) -> Vec<(&'static str, &'static str, &'static str, &'static str)> {
        // Returned lifetimes are 'static because the callers pass string literals; rebuild pairs.
        let mut out = Vec::new();
        for a in 0..nodes.len() {
            for b in (a + 1)..nodes.len() {
                out.push((leak(nodes[a]), leak(nodes[b]), REL_CALLS, TIER_EXTRACTED));
            }
        }
        out
    }

    // Test-only: intern a &str into a 'static so the clique helper can return 'static tuples without
    // threading lifetimes through the fixture. Bounded to the handful of fixture node names.
    fn leak(s: &str) -> &'static str {
        Box::leak(s.to_string().into_boxed_str())
    }

    /// The two-subsystem fixture: two dense 4-cliques, each spanning TWO directories (so a community
    /// groups entities across directory lines), joined by ONE thin bridge edge too weak to merge
    /// them. This is the canonical spec-53 coupling shape.
    fn two_subsystem_graph() -> Graph {
        // Subsystem A spans src/combat and src/net; subsystem B spans src/render and src/ui.
        let a = [
            "src/combat.rs::apply_damage",
            "src/combat.rs::resolve_hit",
            "src/net/sync.rs::replicate",
            "src/net/sync.rs::encode",
        ];
        let b = [
            "src/render.rs::draw",
            "src/render.rs::shade",
            "src/ui/panel.rs::layout",
            "src/ui/panel.rs::paint",
        ];
        let mut edges = clique(&a);
        edges.extend(clique(&b));
        // A single weak inter-subsystem bridge (one REFERENCES edge).
        edges.push((a[0], b[0], REL_REFERENCES, TIER_INFERRED));
        graph(&edges)
    }

    /// Every community in `a` is internally connected: the members sharing a community, under the
    /// coupling graph's edges restricted to that community, form one connected component. This is the
    /// invariant criterion 1 owns - proven directly over the assignment and the graph.
    fn assert_internally_connected(g: &Graph, a: &Assignment) {
        // Group members by community.
        let mut by_comm: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (node, comm) in &a.members {
            by_comm.entry(comm.clone()).or_default().push(node.clone());
        }
        // Undirected adjacency over the coupling edges (same filter as detection).
        let mut adj: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for e in &g.edges {
            let coupling = e.rel == REL_CALLS || e.rel == REL_REFERENCES || e.rel == REL_CONTAINS;
            if !coupling || e.tier == TIER_AMBIGUOUS || e.from == e.to {
                continue;
            }
            adj.entry(e.from.clone()).or_default().insert(e.to.clone());
            adj.entry(e.to.clone()).or_default().insert(e.from.clone());
        }
        for (comm, nodes) in &by_comm {
            let members: BTreeSet<&String> = nodes.iter().collect();
            // BFS from the first member, staying inside the community.
            let start = nodes[0].clone();
            let mut seen: BTreeSet<String> = BTreeSet::from([start.clone()]);
            let mut stack = vec![start];
            while let Some(u) = stack.pop() {
                if let Some(ns) = adj.get(&u) {
                    for v in ns {
                        if members.contains(v) && seen.insert(v.clone()) {
                            stack.push(v.clone());
                        }
                    }
                }
            }
            assert_eq!(
                seen.len(),
                nodes.len(),
                "community {comm} is internally connected ({} of {} reached)",
                seen.len(),
                nodes.len()
            );
        }
    }

    #[test]
    fn detects_two_dense_subsystems_across_directory_lines() {
        // The core criterion-1 claim: densely-coupled entities group into the same community
        // REGARDLESS of directory, and the thin bridge does not fuse the two subsystems.
        let g = two_subsystem_graph();
        let c = Coupling::from_graph(&g);
        let a = detect(&c, DEFAULT_RESOLUTION);
        let m = assignment_map(&a);

        assert_eq!(a.num_communities, 2, "two subsystems, not one merged blob");

        // Subsystem A's four entities (across src/combat and src/net) share one community.
        let ca = &m["src/combat.rs::apply_damage"];
        for node in [
            "src/combat.rs::resolve_hit",
            "src/net/sync.rs::replicate",
            "src/net/sync.rs::encode",
        ] {
            assert_eq!(
                &m[node], ca,
                "{node} joins subsystem A across directory lines"
            );
        }
        // Subsystem B's four entities (across src/render and src/ui) share one DIFFERENT community.
        let cb = &m["src/render.rs::draw"];
        for node in [
            "src/render.rs::shade",
            "src/ui/panel.rs::layout",
            "src/ui/panel.rs::paint",
        ] {
            assert_eq!(
                &m[node], cb,
                "{node} joins subsystem B across directory lines"
            );
        }
        assert_ne!(ca, cb, "the thin bridge does not merge the two subsystems");

        // Community 0 holds the lexicographically-smallest node (deterministic numbering).
        assert_eq!(
            m["src/combat.rs::apply_damage"], "community/1/0",
            "community/1/0 holds the lexicographically-smallest node"
        );
    }

    #[test]
    fn every_community_is_internally_connected() {
        // The connectedness invariant criterion 1 owns, over the two-subsystem fixture.
        let g = two_subsystem_graph();
        let c = Coupling::from_graph(&g);
        let a = detect(&c, DEFAULT_RESOLUTION);
        assert_internally_connected(&g, &a);
    }

    #[test]
    fn refinement_splits_an_internally_disconnected_community() {
        // Directly exercise the refinement: a community label spanning two groups with NO edge
        // between them must split into two connected communities. Build two disjoint cliques and
        // FORCE them into one label, then refine.
        let g = {
            let mut edges = clique(&["a1", "a2", "a3"]);
            edges.extend(clique(&["b1", "b2", "b3"]));
            graph(&edges)
        };
        let c = Coupling::from_graph(&g);
        // Force every node into the SAME (disconnected) community label 0.
        let forced = vec![0usize; c.len()];
        let refined = refine_connected(&c, &forced);
        // Two connected components => two distinct refined labels, each spanning one clique.
        let labels: BTreeSet<usize> = refined.iter().copied().collect();
        assert_eq!(labels.len(), 2, "a disconnected community splits into two");
        // The two `a*` nodes share one label, the `b*` nodes another (indices follow sorted ids).
        let idx: BTreeMap<&str, usize> = c
            .nodes
            .iter()
            .enumerate()
            .map(|(i, s)| (s.as_str(), i))
            .collect();
        assert_eq!(
            refined[idx["a1"]], refined[idx["a3"]],
            "a-clique stays together"
        );
        assert_eq!(
            refined[idx["b1"]], refined[idx["b3"]],
            "b-clique stays together"
        );
        assert_ne!(
            refined[idx["a1"]], refined[idx["b1"]],
            "the two disconnected halves land in different communities"
        );
    }

    #[test]
    fn detection_is_byte_identical_across_runs() {
        // Determinism: two independent detections of the same coupling graph produce equal
        // assignments (members, community ids, hash) AND byte-identical event payloads - the
        // property the rebuild-from-events guarantee rests on.
        let g = two_subsystem_graph();
        let c1 = Coupling::from_graph(&g);
        let c2 = Coupling::from_graph(&g);
        let a1 = detect(&c1, DEFAULT_RESOLUTION);
        let a2 = detect(&c2, DEFAULT_RESOLUTION);
        assert_eq!(
            a1.members, a2.members,
            "assignments are identical across runs"
        );
        assert_eq!(a1.hash, a2.hash, "the content hash is stable across runs");
        assert_eq!(a1.num_communities, a2.num_communities);

        // The recorded event payloads (the bytes a rebuild replays) are byte-identical.
        let wire = |a: &Assignment| -> Vec<(String, Vec<u8>)> {
            events(a).into_iter().map(|e| (e.type_, e.data)).collect()
        };
        assert_eq!(
            wire(&a1),
            wire(&a2),
            "event payloads are byte-identical across runs"
        );
    }

    #[test]
    fn the_first_event_carries_the_fresh_pass_boundary() {
        // The pass boundary marker rides the FIRST event only (the lexicographically-smallest node),
        // so the fold supersedes the grain's prior memberships exactly once per pass.
        let g = two_subsystem_graph();
        let c = Coupling::from_graph(&g);
        let a = detect(&c, DEFAULT_RESOLUTION);
        let evs = events(&a);
        assert_eq!(evs.len(), a.members.len(), "one event per member");
        let first: CommunityAssigned = serde_json::from_slice(&evs[0].data).unwrap();
        assert!(first.fresh, "the first event marks the pass boundary");
        assert_eq!(
            first.node, a.members[0].0,
            "the boundary rides the smallest node's event"
        );
        for e in &evs[1..] {
            let c: CommunityAssigned = serde_json::from_slice(&e.data).unwrap();
            assert!(!c.fresh, "no non-first event carries the pass boundary");
        }
    }

    #[test]
    fn higher_resolution_never_yields_fewer_communities() {
        // The algorithm supports the grain knob (the SEMANTICS proof is a later criterion's): a
        // higher resolution penalizes large communities more, so it never yields fewer communities
        // on the same graph. Two well-separated cliques at r=1 are two communities; a high
        // resolution splits, never merges.
        let g = two_subsystem_graph();
        let c = Coupling::from_graph(&g);
        let low = detect(&c, 1.0);
        let high = detect(&c, 4.0);
        assert!(
            high.num_communities >= low.num_communities,
            "higher resolution never merges: {} < {}",
            high.num_communities,
            low.num_communities
        );
    }

    #[test]
    fn from_graph_resolves_a_bare_cross_file_reference_onto_its_unique_definition() {
        // The cross-directory heart of the pass: a reference in file B to `foo` (defined in file A)
        // folds onto a BARE same-file placeholder `src/b.rs::foo` (no `name` attr). Detection must
        // redirect that bare node onto the UNIQUE real definition `src/a.rs::foo`, so B genuinely
        // couples to A across the directory line - not to a dead-end placeholder. Without resolution,
        // B and A would sit in different components and never share a community.
        let g = Graph {
            nodes: vec![
                node("src/a.rs", KIND_FILE, None),
                node("src/a.rs::foo", KIND_CODE_ENTITY, Some("foo")),
                node("src/b.rs", KIND_FILE, None),
                node("src/b.rs::foo", KIND_CODE_ENTITY, None), // bare cross-file placeholder
            ],
            edges: vec![
                edge("src/a.rs", "src/a.rs::foo", REL_CONTAINS, TIER_EXTRACTED),
                // The cross-file reference, landed on the bare placeholder at the inferred tier.
                edge("src/b.rs", "src/b.rs::foo", REL_REFERENCES, TIER_INFERRED),
            ],
        };
        let c = Coupling::from_graph(&g);
        let a = detect(&c, DEFAULT_RESOLUTION);
        let m = assignment_map(&a);

        assert!(
            !m.contains_key("src/b.rs::foo"),
            "the bare placeholder resolves away - it is never a member on its own"
        );
        assert!(
            m.contains_key("src/a.rs::foo") && m.contains_key("src/b.rs"),
            "the real definition and the referencing file are both members"
        );
        assert_eq!(
            m["src/b.rs"], m["src/a.rs::foo"],
            "file B couples to A's definition across the directory line (one community)"
        );
    }

    #[test]
    fn from_graph_leaves_an_ambiguous_bare_reference_unresolved() {
        // Honest by construction: a bare `foo` reference whose simple name matches MORE THAN ONE
        // definition does NOT resolve (it would be a guess). The bare node stays itself, so it does
        // not spuriously fuse the two candidate definitions' subsystems.
        let g = Graph {
            nodes: vec![
                node("src/a.rs::foo", KIND_CODE_ENTITY, Some("foo")),
                node("src/x.rs::foo", KIND_CODE_ENTITY, Some("foo")),
                node("src/b.rs", KIND_FILE, None),
                node("src/b.rs::foo", KIND_CODE_ENTITY, None),
            ],
            edges: vec![edge(
                "src/b.rs",
                "src/b.rs::foo",
                REL_REFERENCES,
                TIER_INFERRED,
            )],
        };
        let c = Coupling::from_graph(&g);
        let a = detect(&c, DEFAULT_RESOLUTION);
        let m = assignment_map(&a);
        // The bare node stays itself (two candidates => no resolution), so B couples only to the
        // unresolved placeholder, never to either real definition.
        assert!(
            m.contains_key("src/b.rs::foo"),
            "an ambiguous bare reference stays unresolved rather than guessing a definition"
        );
        assert!(
            !m.contains_key("src/a.rs::foo") && !m.contains_key("src/x.rs::foo"),
            "neither candidate definition is coupled by the ambiguous reference"
        );
    }

    #[test]
    fn an_empty_coupling_graph_yields_no_assignment() {
        // A graph with no coupling edges (all references ambiguous, or none at all) has no community
        // layer: detection returns nothing and records no event.
        let g = graph(&[("x", "y", REL_REFERENCES, TIER_AMBIGUOUS)]);
        let c = Coupling::from_graph(&g);
        assert!(
            c.is_empty(),
            "ambiguous-only edges leave the coupling graph empty"
        );
        let a = detect(&c, DEFAULT_RESOLUTION);
        assert_eq!(a.num_communities, 0);
        assert!(a.members.is_empty());
        assert!(
            events(&a).is_empty(),
            "an empty assignment records no event"
        );
    }

    /// Build a coupling `Graph` from WEIGHTED undirected edges `(from, to, weight)`, emitted as
    /// `weight` parallel `CALLS` edges at the extracted tier (the coupling layer weights each pair by
    /// edge multiplicity, so a weight-`w` edge is `w` collapsed calls). Node ids carry no `::`, so
    /// they are non-resolving by construction and the coupling topology is exactly the weighted edges
    /// given. Nodes sort lexicographically, so a node's coupling index is its id's rank.
    fn weighted_graph(edges: &[(&'static str, &'static str, usize)]) -> Graph {
        let mut tuples: Vec<(&str, &str, &str, &str)> = Vec::new();
        for &(a, b, w) in edges {
            for _ in 0..w {
                tuples.push((a, b, REL_CALLS, TIER_EXTRACTED));
            }
        }
        graph(&tuples)
    }

    /// How many connected components the nodes carrying local-moving `label` split into, over the
    /// coupling graph's edges (a label with `>= 2` is an INTERNALLY DISCONNECTED community - the
    /// defect the refinement phase must repair).
    fn components_of_label(c: &Coupling, moved: &[usize], label: usize) -> usize {
        let members: Vec<usize> = (0..c.len()).filter(|&i| moved[i] == label).collect();
        let mset: BTreeSet<usize> = members.iter().copied().collect();
        let mut seen: BTreeSet<usize> = BTreeSet::new();
        let mut comps = 0;
        for &s in &members {
            if !seen.insert(s) {
                continue;
            }
            comps += 1;
            let mut stack = vec![s];
            while let Some(u) = stack.pop() {
                for &(v, _w) in &c.adj[u] {
                    if mset.contains(&v) && seen.insert(v) {
                        stack.push(v);
                    }
                }
            }
        }
        comps
    }

    #[test]
    fn refinement_reconnects_a_community_local_moving_left_disconnected() {
        // Criterion 1's connectedness PROOF, driven end-to-end through `detect`. This weighted
        // fixture is deliberately one where local moving ALONE strands an internally-disconnected
        // community, so the refinement phase is LOAD-BEARING here, not a no-op: if the
        // `refine_connected` call is removed from `detect`, the connectedness assertion below reddens.
        //
        // Shape: a dense hub {n0,n1,n3,n6,n9} (heavy n6-n9, n0-n6, n3-n6 edges) with two edge-disjoint
        // peripheral pairs {n2,n10} and {n4,n8} hanging off it, plus an unrelated pair {n5,n7}. At
        // r=0.6 local moving transiently lumps the two peripheral pairs into ONE community and then
        // consolidates the hub away, leaving {n2,n10,n4,n8} in a single community with NO edge between
        // the {n2,n10} and {n4,n8} halves - internally disconnected. Refinement splits it into its two
        // connected components. (Unweighted graphs up to 7 nodes provably cannot exhibit this, so the
        // fixture is weighted, which is faithful to the real multiplicity-weighted coupling layer.)
        let g = weighted_graph(&[
            ("n0", "n1", 1),
            ("n0", "n2", 1),
            ("n0", "n4", 2),
            ("n0", "n6", 3),
            ("n0", "n9", 1),
            ("n1", "n3", 2),
            ("n2", "n6", 1),
            ("n2", "n10", 1),
            ("n3", "n6", 3),
            ("n4", "n8", 1),
            ("n5", "n7", 1),
            ("n6", "n9", 4),
        ]);
        let c = Coupling::from_graph(&g);
        let res = 0.6;

        // Local moving alone leaves at least one community internally DISCONNECTED (>= 2 components):
        // this is what makes the fixture guard the refinement phase rather than pass vacuously.
        let moved = local_moving(&c, res);
        let labels: BTreeSet<usize> = moved.iter().copied().collect();
        let disconnected: Vec<usize> = labels
            .iter()
            .copied()
            .filter(|&l| components_of_label(&c, &moved, l) >= 2)
            .collect();
        assert!(
            !disconnected.is_empty(),
            "fixture must leave local moving with an internally-disconnected community \
             (else the refinement phase is untested here); moved={moved:?}"
        );

        // `detect` runs the refinement, so EVERY output community is internally connected. Removing
        // the `refine_connected` call in `detect` makes this fail.
        let a = detect(&c, res);
        assert_internally_connected(&g, &a);

        // Pin the split: the two edge-disjoint halves land in DIFFERENT communities (proof the
        // disconnected community was actually split, not merely renamed), and the deterministic
        // numbering is stable.
        let m = assignment_map(&a);
        assert_eq!(
            a.num_communities, 4,
            "hub + two split halves + the unrelated pair"
        );
        assert_ne!(
            m["n2"], m["n4"],
            "refinement splits the internally-disconnected community into its two halves"
        );
        assert_eq!(
            m["n2"], m["n10"],
            "the {{n2,n10}} half stays one connected community"
        );
        assert_eq!(
            m["n4"], m["n8"],
            "the {{n4,n8}} half stays one connected community"
        );
        for x in ["n1", "n3", "n6", "n9"] {
            assert_eq!(m["n0"], m[x], "the dense hub stays one community");
        }
        assert_eq!(m["n5"], m["n7"], "the unrelated pair is its own community");
        // Deterministic numbering by ascending lexicographically-smallest member.
        assert_eq!(m["n0"], "community/0.6/0");
        assert_eq!(m["n2"], "community/0.6/1");
        assert_eq!(m["n4"], "community/0.6/2");
        assert_eq!(m["n5"], "community/0.6/3");
    }

    #[test]
    fn a_squeezed_bridge_node_takes_a_fresh_singleton_community() {
        // Determinism coverage for the local-moving "isolate to a FRESH singleton" arm: when a node
        // leaves a still-populated community to stand alone, it must take a brand-new label (not reuse
        // its old, still-occupied one). No dense-clique fixture ever isolates a node, so this arm was
        // reachable but untested; a bookkeeping regression there (wrong index, or a missed
        // `improved = true`) would otherwise pass the whole suite.
        //
        // Shape: a 6-node tree. n0 is a small hub with leaves {n4,n5}; n1 bridges n0's cluster to the
        // {n2,n3} pair (n0-n1, n1-n3, n2-n3). At r=2 the degree penalty is steep enough that n1 is
        // worth keeping in NEITHER neighbour, so it abandons its community and isolates into a fresh
        // singleton - reaching the arm.
        let g = weighted_graph(&[
            ("n0", "n1", 1),
            ("n0", "n4", 1),
            ("n0", "n5", 1),
            ("n1", "n3", 1),
            ("n2", "n3", 1),
        ]);
        let c = Coupling::from_graph(&g);
        let res = 2.0;

        // The fresh-singleton arm is the ONLY place a label at or beyond the initial `n` singletons is
        // minted, so a fresh label in the local-moving result proves the arm fired.
        let moved = local_moving(&c, res);
        assert!(
            moved.iter().copied().max().unwrap_or(0) >= c.len(),
            "a node must isolate into a FRESH singleton label (the untested arm); moved={moved:?}"
        );

        // Pin the resulting assignment end-to-end through `detect` (this closes the branch): n1 alone,
        // its former neighbours split into the hub {n0,n4,n5} and the pair {n2,n3}.
        let a = detect(&c, res);
        let m = assignment_map(&a);
        assert_eq!(a.num_communities, 3);
        assert_eq!(m["n1"], "community/2/1", "the squeezed bridge stands alone");
        assert_eq!(m["n0"], "community/2/0");
        assert_eq!(m["n4"], "community/2/0");
        assert_eq!(m["n5"], "community/2/0");
        assert_eq!(m["n2"], "community/2/2");
        assert_eq!(m["n3"], "community/2/2");
        // The isolated node's community is a genuine singleton (internally connected trivially), and
        // every community stays internally connected.
        assert_internally_connected(&g, &a);
    }
}
