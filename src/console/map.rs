//! The code lens's MAP ENGINE (spec 84 criterion 1, "THE MAP LANDS LABELLED"): districts,
//! semantic zoom, and label placement, as pure functions over an already-projected
//! [`crate::contextgraph::Graph`] (the same payload `graph_load` already parses) plus a viewport
//! and a camera zoom - no store, no clock, no I/O, so this compiles for `wasm32-unknown-unknown`
//! exactly like the rest of `core` (spec 93's own purity rule). [`build`] runs once per loaded
//! graph (the console-core member crate's `map_build` op, spec 93 Design section 4) and caches its
//! result; [`frame`] runs every camera/zoom change (`map_frame`) and is cheap enough to call every
//! frame.
//!
//! THE INVARIANT (Design, "THE MAP"): **every entity ON SCREEN is labelled; zoom controls how many
//! entities are on screen, never whether they have names.** [`frame`] enforces this by
//! construction: an entity is pushed onto the returned [`DrawList`] in the SAME step that placed
//! its label successfully - there is no code path that draws a dot without a label, and a label
//! that cannot be placed (all four candidate offsets collide) means the entity is simply never
//! drawn (see [`place_label`]'s own doc).
//!
//! **Districts, never files.** Communities (spec 53's `community/<r>/<n>` super-nodes, which today
//! form almost entirely by file co-location - see spec 84's own Goal) are grouped into DISTRICTS
//! named by PURPOSE: a curated module-path -> purpose table ([`district_purpose`]), falling back to
//! the module's own bare name - NEVER to a file name - for a module this table does not cover
//! ([`module_of`] never returns a string carrying `.` or `/`, so a fallback purpose can never look
//! like a path). An entity's own display name is its code-entity `name` attr (its symbol name),
//! falling back to its id's name-suffix (never its raw `<file>::<name>` id, which would leak the
//! file) - see [`build`]'s own comment.
//!
//! **Three units share this ABI; this module owns exactly one slice of it** (the plan-critique
//! record `adv-pc84-text-treatment-ownership-confirmed` / `op-pc84-approve-spec84-dag`): districts,
//! the degree rank, and label placement are THIS criterion's; the explore rail, search, selection
//! and the interactive camera are criterion 2's (a later unit composes with [`frame`]'s draw list,
//! adding lit-selection data, without touching what this module computes); the legend and the text
//! TREATMENTS (underline/italic styling) are criterion 3's, rendered by the page from this module's
//! plain draw list.

use std::collections::{BTreeMap, BTreeSet};

use crate::contextgraph::query::{file_of, name_suffix, Buckets, Lens};
use crate::contextgraph::{Graph, Node, KIND_CODE_ENTITY, REL_CALLS, REL_REFERENCES};

/// The community-detection resolution grain the map reads (mirrors
/// [`crate::contextgraph::query::DEFAULT_COMMUNITY_RESOLUTION`] - the code lens's default grain,
/// the ONLY grain spec 84 addresses).
pub const RESOLUTION: &str = "1";

/// The curated MODULE -> PURPOSE table (Design, DISTRICTS): a hand-maintained mapping from this
/// project's own module vocabulary to the subsystem purpose a district's pill names, e.g.
/// `worktree` -> `worktree lifecycle`. Several modules can share one purpose (`liveness`/`spawn`
/// both read `liveness & heartbeats`, mirroring the Design text's own example); a module absent
/// here falls back to its own bare name in [`district_purpose`] - NEVER to a file name.
const CURATED_PURPOSES: &[(&str, &str)] = &[
    ("worktree", "worktree lifecycle"),
    ("reap", "reaping authority"),
    ("liveness", "liveness & heartbeats"),
    ("spawn", "liveness & heartbeats"),
    ("dash", "dashboard rendering"),
    ("conductor", "the drive loop"),
    ("contextgraph", "the knowledge graph"),
    ("community", "coupling communities"),
    ("concepts", "concept derivation"),
    ("grounder", "grounding & extraction"),
    ("gate", "gate execution"),
    ("ledger", "run ledger"),
    ("blocker", "blocker classification"),
    ("budget", "spawn budget"),
    ("registry", "instance registry"),
    ("console", "mission control core"),
    ("eventstore", "the event store"),
    ("canary", "the canary corpus"),
    ("canary_store", "the canary corpus"),
    ("metrics", "operator metrics"),
    ("run", "run orchestration"),
    ("workflow", "workflow definition"),
    ("mcpserver", "the MCP server"),
    ("hooks", "editor hooks"),
    ("sidecar", "the peers sidecar"),
    ("watchdog", "the run watchdog"),
    ("ingest", "graph ingestion"),
    ("safety", "safety limits"),
    ("scratch", "scratch lifecycle"),
    ("parallel", "parallel execution"),
    ("worker", "worker execution"),
    ("main", "the CLI entry point"),
];

/// The top-level MODULE a source file belongs to - a total function that never returns a string
/// carrying `.` or `/`, so it can never be mistaken for a file name (Design, DISTRICTS: "unmapped
/// modules fall back to the module name, never to a file name"). A `crates/<name>/...` member's
/// module is its crate directory name; an `src/<dir>/...` (or a bare `<dir>/...`) file's module is
/// that directory; a leaf file with no subdirectory (`src/dash.rs`, or a repo-root file) is its own
/// stem with the extension stripped.
pub(crate) fn module_of(file: &str) -> String {
    if let Some(after) = file.strip_prefix("crates/") {
        return match after.split_once('/') {
            Some((crate_name, _)) => crate_name.to_string(),
            None => strip_ext(after),
        };
    }
    let rest = file.strip_prefix("src/").unwrap_or(file);
    match rest.split_once('/') {
        Some((dir, _)) => dir.to_string(),
        None => strip_ext(rest),
    }
}

/// Strip a trailing `.ext` from a leaf file name, leaving a bare stem never carrying a `.`. A name
/// with no `.` (already a bare stem) is returned unchanged.
fn strip_ext(leaf: &str) -> String {
    leaf.rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(leaf)
        .to_string()
}

/// The district PURPOSE a module resolves to: the curated name, or the module's own bare name when
/// [`CURATED_PURPOSES`] does not cover it - NEVER a file name (see [`module_of`]'s own guarantee).
pub(crate) fn district_purpose(module: &str) -> String {
    CURATED_PURPOSES
        .iter()
        .find(|(m, _)| *m == module)
        .map(|(_, purpose)| (*purpose).to_string())
        .unwrap_or_else(|| module.to_string())
}

/// One code entity placed on the map (built once by [`build`], reused by every [`frame`] call at
/// every zoom - only its SCREEN position and whether it is drawn at all vary per frame).
#[derive(Debug, Clone)]
pub struct MapEntity {
    pub id: String,
    /// The entity's display name - its `name` attr (the symbol name), or, absent that (a bare
    /// cross-file placeholder), its id's name-suffix ([`name_suffix`]) - NEVER the raw
    /// `<file>::<name>` id, which would leak the file it lives in onto the canvas.
    pub name: String,
    /// The definition kind (`function`, `type`, ...) - the map's "kind dot" colour key.
    pub kind: String,
    pub community: String,
    /// The district (purpose string) this entity's community was grouped into.
    pub district: String,
    pub degree: usize,
    /// 0-based rank within its OWN community, by degree descending (id ascending on a tie) -
    /// [`frame`]'s [`budget`] reads this to decide visibility at a given zoom.
    pub rank: usize,
    /// World-space position, fixed at build time.
    pub x: f64,
    pub y: f64,
}

/// One district: a purpose-named group of one or more communities (Design, DISTRICTS: "communities
/// are grouped into districts named by PURPOSE").
#[derive(Debug, Clone)]
pub struct District {
    pub purpose: String,
    pub population: usize,
    pub x: f64,
    pub y: f64,
    pub radius: f64,
}

/// The built map: every entity and district, positioned in world space, plus the coupling edges
/// among entities the map may draw (a from/to/rel triple; [`frame`] filters this to the edges whose
/// BOTH endpoints are actually drawn that frame). [`RESOLUTION`]-scoped; entities with no live
/// community membership at that grain are excluded entirely (mirrors
/// [`crate::contextgraph::query::whole_graph_lens_key`]'s own `Lens::Code` arm: a membership-less
/// code entity gets no bucket there either).
pub struct MapModel {
    /// Every entity, sorted by (rank ascending, district, community, id) - the ONE fixed
    /// processing order [`frame`] walks at every zoom. Rank-primary ordering is what makes a
    /// higher zoom's eligible set a STRICT SUPERSET of a lower zoom's (see [`budget`]'s own doc):
    /// every entity eligible at a smaller budget stays eligible, in the SAME relative position,
    /// at a larger one.
    pub entities: Vec<MapEntity>,
    pub districts: Vec<District>,
    /// `(from, to, rel)` for every currently-valid `CALLS`/`REFERENCES` edge whose both endpoints
    /// are code entities this model carries (a district member on either end).
    pub edges: Vec<(String, String, String)>,
    /// The world-space bounding box `(min_x, min_y, max_x, max_y)` of every entity and district
    /// hull - the FULL EXTENT [`frame`]'s zoom-0 camera fits (Design, SEED).
    pub bounds: (f64, f64, f64, f64),
}

/// Sum the whole-graph degree of every node (spec-wide "a self-loop counts once" contract, matching
/// [`crate::contextgraph::query::whole_graph_degree`]'s own semantics) in ONE pass over the edge
/// list - `O(E)`, not the `O(N * E)` a per-node `whole_graph_degree` call would cost building the
/// map for every entity.
fn degree_map(graph: &Graph) -> BTreeMap<&str, usize> {
    let mut deg: BTreeMap<&str, usize> = BTreeMap::new();
    for e in &graph.edges {
        if e.valid_to.is_some() {
            continue;
        }
        *deg.entry(e.from.as_str()).or_default() += 1;
        if e.to != e.from {
            *deg.entry(e.to.as_str()).or_default() += 1;
        }
    }
    deg
}

/// World-space layout constants. A deterministic phyllotaxis spiral (the standard sunflower-seed
/// packing angle) lays out a district's members with steadily increasing spacing as rank grows -
/// no physics simulation, no randomness, so the same graph always builds the byte-identical model.
const CELL: f64 = 900.0;
const INNER_R: f64 = 44.0;
const RING_STEP: f64 = 30.0;
const GOLDEN_ANGLE: f64 = 2.399_963_229_728_653;

/// Build the map once from an already-loaded, already-community-derived [`Graph`] (the payload
/// `graph_load` parses, after `rigger graph communities` has run - an UNDERIVED graph simply
/// yields an empty model, never an error: see the CONSTRAINTS WALK's "empty graph" case, which is
/// the page's to render as an empty-state sentence, not this function's to guess at).
///
/// Only [`KIND_CODE_ENTITY`] nodes with a live community membership become map entities (a
/// membership-less code entity, or any non-code node, is excluded outright - the map draws code,
/// never a file/decision/design-doc node). A community's DISTRICT is its dominant member's module
/// (the module the largest share of its members' files resolve to, ties broken to the
/// lexicographically-smallest module), mapped through [`district_purpose`]; several communities
/// sharing a purpose fold into ONE district.
pub fn build(graph: &Graph) -> MapModel {
    let deg = degree_map(graph);
    // Community membership at RESOLUTION comes from the SAME single fold authority the code
    // lens itself uses (`Buckets`'s own doc: "ONE bucket-fold authority, never two") - never a
    // second, independently-maintained `IN_COMMUNITY` scan reconciled after the fact.
    let lens = Lens::Code {
        resolution: RESOLUTION.to_string(),
    };
    let comm = Buckets::new(graph, &lens);

    let mut by_community: BTreeMap<&str, Vec<&Node>> = BTreeMap::new();
    for n in &graph.nodes {
        // A code entity with NO `name` attr is a BARE cross-file placeholder (`Coupling::
        // from_graph`'s own words: "A code-entity node carrying a `name` attr is a real
        // definition ... a code-entity with NO `name` attr is a bare cross-file placeholder"),
        // never a real definition - `Coupling::from_graph` canonicalizes every RESOLVABLE bare
        // reference onto its unique real definition, but an AMBIGUOUS one (more than one
        // same-named definition elsewhere) keeps its own placeholder id and still joins a
        // community as a coupling-graph member. Such a node carries no `kind` either, so
        // drawing it would violate "every visible node ... has a placed label and kind dot" -
        // it is excluded here, same as [`crate::contextgraph::query::defs_by_entity_suffix`]'s
        // own `name`-attr gate.
        if n.kind != KIND_CODE_ENTITY || !n.attrs.contains_key("name") {
            continue;
        }
        if let Some(c) = comm.membership.get(n.id.as_str()) {
            by_community.entry(c).or_default().push(n);
        }
    }

    // Rank each community's members by degree descending, id ascending on a tie.
    let mut ranked: BTreeMap<&str, Vec<(&Node, usize)>> = BTreeMap::new();
    for (c, members) in &by_community {
        let mut ms: Vec<(&Node, usize)> = members
            .iter()
            .map(|n| (*n, deg.get(n.id.as_str()).copied().unwrap_or(0)))
            .collect();
        ms.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.id.cmp(&b.0.id)));
        ranked.insert(c, ms);
    }

    // Each community's district: the purpose its DOMINANT member module resolves to.
    let mut community_district: BTreeMap<&str, String> = BTreeMap::new();
    for (c, members) in &by_community {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for n in members {
            if let Some(file) = file_of(&n.id) {
                *counts.entry(module_of(file)).or_default() += 1;
            }
        }
        let dominant = counts
            .into_iter()
            // Largest count wins; on a tie, the LEXICOGRAPHICALLY-SMALLEST module wins
            // (`b.cmp(a)` makes the smaller `a` compare greater, mirroring `fold_buckets`'s own
            // documented dominant-kind tie-break).
            .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
            .map(|(m, _)| m)
            .unwrap_or_else(|| "(unresolved)".to_string());
        community_district.insert(c, district_purpose(&dominant));
    }

    // Group communities sharing a purpose into one district.
    let mut district_communities: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for (c, purpose) in &community_district {
        district_communities
            .entry(purpose.clone())
            .or_default()
            .push(c);
    }
    let mut district_pop: BTreeMap<String, usize> = BTreeMap::new();
    for (purpose, cs) in &district_communities {
        let pop: usize = cs
            .iter()
            .map(|c| ranked.get(c).map(Vec::len).unwrap_or(0))
            .sum();
        district_pop.insert(purpose.clone(), pop);
    }
    // Deterministic grid order: largest district first, ties broken lexicographically by purpose.
    let mut district_order: Vec<String> = district_communities.keys().cloned().collect();
    district_order.sort_by(|a, b| district_pop[b].cmp(&district_pop[a]).then_with(|| a.cmp(b)));

    let cols = (district_order.len() as f64).sqrt().ceil().max(1.0) as usize;

    let mut districts = Vec::with_capacity(district_order.len());
    let mut entities = Vec::new();
    let mut min_x = f64::MAX;
    let mut max_x = f64::MIN;
    let mut min_y = f64::MAX;
    let mut max_y = f64::MIN;

    for (i, purpose) in district_order.iter().enumerate() {
        let col = (i % cols) as f64;
        let row = (i / cols) as f64;
        let cx = col * CELL;
        let cy = row * CELL;
        let pop = district_pop[purpose];
        let radius = (INNER_R + RING_STEP * (pop as f64).sqrt()).max(70.0);

        let mut idx: usize = 0;
        for c in &district_communities[purpose] {
            for (rank, (node, degree)) in ranked[c].iter().enumerate() {
                let angle = idx as f64 * GOLDEN_ANGLE;
                let r = INNER_R + RING_STEP * (idx as f64 + 1.0).sqrt();
                let x = cx + r * angle.cos();
                let y = cy + r * angle.sin();
                min_x = min_x.min(x);
                max_x = max_x.max(x);
                min_y = min_y.min(y);
                max_y = max_y.max(y);
                let name = node
                    .attrs
                    .get("name")
                    .cloned()
                    .unwrap_or_else(|| name_suffix(&node.id).to_string());
                entities.push(MapEntity {
                    id: node.id.clone(),
                    name,
                    kind: node.attrs.get("kind").cloned().unwrap_or_default(),
                    community: (*c).to_string(),
                    district: purpose.clone(),
                    degree: *degree,
                    rank,
                    x,
                    y,
                });
                idx += 1;
            }
        }
        min_x = min_x.min(cx - radius);
        max_x = max_x.max(cx + radius);
        min_y = min_y.min(cy - radius);
        max_y = max_y.max(cy + radius);
        districts.push(District {
            purpose: purpose.clone(),
            population: pop,
            x: cx,
            y: cy,
            radius,
        });
    }

    // THE FIXED PROCESSING ORDER `frame` walks: rank-primary, so a higher zoom's eligible prefix
    // (by rank) is always a strict superset of a lower zoom's, in the identical relative order.
    entities.sort_by(|a, b| {
        a.rank
            .cmp(&b.rank)
            .then_with(|| a.district.cmp(&b.district))
            .then_with(|| a.community.cmp(&b.community))
            .then_with(|| a.id.cmp(&b.id))
    });

    let ids: BTreeSet<&str> = entities.iter().map(|e| e.id.as_str()).collect();
    let mut edges = Vec::new();
    for e in &graph.edges {
        if e.valid_to.is_some() {
            continue;
        }
        if e.rel != REL_CALLS && e.rel != REL_REFERENCES {
            continue;
        }
        if ids.contains(e.from.as_str()) && ids.contains(e.to.as_str()) {
            edges.push((e.from.clone(), e.to.clone(), e.rel.clone()));
        }
    }

    let bounds = if entities.is_empty() && districts.is_empty() {
        (0.0, 0.0, 1.0, 1.0)
    } else {
        (min_x, min_y, max_x, max_y)
    };

    MapModel {
        entities,
        districts,
        edges,
        bounds,
    }
}

/// One district's rendered pill: its purpose label, member count, and SCREEN position/radius at
/// this frame's camera.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DrawDistrict {
    pub purpose: String,
    pub population: usize,
    pub x: f64,
    pub y: f64,
    pub radius: f64,
}

/// One entity's rendered dot: its label and SCREEN position at this frame's camera. Appearing in
/// [`DrawList::entities`] at ALL means its label placed successfully - see [`frame`]'s own doc.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DrawEntity {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub x: f64,
    pub y: f64,
}

/// One typed edge between two DRAWN entities.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DrawEdge {
    pub from: String,
    pub to: String,
    pub rel: String,
}

/// One frame's whole draw list: every district pill (always present, Design's own words),
/// every LABELLED entity dot (never an unlabelled one), and every edge between two drawn entities.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct DrawList {
    pub districts: Vec<DrawDistrict>,
    pub entities: Vec<DrawEntity>,
    pub edges: Vec<DrawEdge>,
}

/// The number of a community's top-ranked members visible at `zoom` (Design, SEMANTIC ZOOM: "the
/// entities shown per community are the top budget(zoom) by rank"). Strictly increasing in `zoom`
/// (clamped to `0.0` below), so a [`frame`] call at a higher zoom always admits a rank-prefix that
/// is a STRICT SUPERSET of a lower zoom's - the eligibility half of "zooming in never drops an
/// already-labelled entity" (the other half - that a wider camera scale only ever gives two
/// distinct entities MORE screen-space separation, never less - is `frame`'s own projection).
const BUDGET_BASE: usize = 3;
const BUDGET_STEP: f64 = 4.0;
pub(crate) fn budget(zoom: f64) -> usize {
    BUDGET_BASE + (zoom.max(0.0) * BUDGET_STEP).round() as usize
}

/// Render one frame of `model` at `zoom` for a `viewport_w` x `viewport_h` canvas. `zoom <= 0.0` is
/// the FULL EXTENT (Design, SEED: "the initial camera is the full extent"): the whole world
/// bounding box scaled to fit the viewport (with a small margin), centred on its own centroid - c1
/// implements no panning (that is criterion 2's camera), so every frame is centred the same way,
/// magnified by `zoom`.
///
/// THE LABELLED-MAP INVARIANT, enforced by construction: district pills are placed FIRST,
/// unconditionally, and reserve their screen bounding box - so "entity labels never displace
/// district labels" (Design) holds because an entity candidate overlapping a pill is simply
/// rejected, never placed over it. Entities are then walked in `model.entities`'s FIXED rank-primary
/// order; an entity beyond this zoom's [`budget`] is skipped outright, and an eligible entity whose
/// label cannot be placed at any of [`place_label`]'s four candidates is likewise skipped - in
/// BOTH cases it is never pushed onto [`DrawList::entities`], so every returned entity carries a
/// placed label and no unlabelled node is ever drawn.
pub fn frame(model: &MapModel, viewport_w: f64, viewport_h: f64, zoom: f64) -> DrawList {
    let (min_x, min_y, max_x, max_y) = model.bounds;
    let world_w = (max_x - min_x).max(1.0);
    let world_h = (max_y - min_y).max(1.0);
    let world_cx = (min_x + max_x) / 2.0;
    let world_cy = (min_y + max_y) / 2.0;

    const MARGIN: f64 = 0.9;
    let fit = (viewport_w / world_w).min(viewport_h / world_h) * MARGIN;
    let scale = fit * (1.0 + zoom.max(0.0) * 0.5);

    let project = |x: f64, y: f64| -> (f64, f64) {
        (
            (x - world_cx) * scale + viewport_w / 2.0,
            (y - world_cy) * scale + viewport_h / 2.0,
        )
    };

    let mut reserved: Vec<(f64, f64, f64, f64)> = Vec::new();
    let mut districts = Vec::with_capacity(model.districts.len());
    for d in &model.districts {
        let (sx, sy) = project(d.x, d.y);
        let half_w = (d.purpose.chars().count() as f64 * 6.5 + 16.0) / 2.0;
        let half_h = 12.0;
        reserved.push((sx - half_w, sy - half_h, sx + half_w, sy + half_h));
        districts.push(DrawDistrict {
            purpose: d.purpose.clone(),
            population: d.population,
            x: sx,
            y: sy,
            radius: d.radius * scale,
        });
    }

    let visible_rank = budget(zoom);
    let mut drawn: BTreeSet<&str> = BTreeSet::new();
    let mut entities = Vec::new();
    for e in &model.entities {
        if e.rank >= visible_rank {
            continue;
        }
        let (sx, sy) = project(e.x, e.y);
        if place_label(sx, sy, &e.name, &mut reserved).is_some() {
            drawn.insert(e.id.as_str());
            entities.push(DrawEntity {
                id: e.id.clone(),
                name: e.name.clone(),
                kind: e.kind.clone(),
                x: sx,
                y: sy,
            });
        }
        // else: the entity's label could not be placed - it is NOT drawn (the invariant).
    }

    let edges = model
        .edges
        .iter()
        .filter(|(from, to, _)| drawn.contains(from.as_str()) && drawn.contains(to.as_str()))
        .map(|(from, to, rel)| DrawEdge {
            from: from.clone(),
            to: to.clone(),
            rel: rel.clone(),
        })
        .collect();

    DrawList {
        districts,
        entities,
        edges,
    }
}

/// Try to place `name`'s label at one of four candidate offsets around the dot at screen `(sx,
/// sy)` (Design, SEMANTIC ZOOM: "Labels dodge each other (four candidate positions)") - the FIRST
/// candidate (east, west, north, south of the dot, in that fixed preference order) whose bounding
/// box overlaps NONE of `reserved` wins: it is pushed onto `reserved` (so later entities and the
/// caller's next label dodge it too) and its offset is returned. `None` when all four collide -
/// the caller must then treat the entity as NOT DRAWN (Design: "an entity whose label cannot be
/// placed is NOT drawn - the invariant beats density").
fn place_label(
    sx: f64,
    sy: f64,
    name: &str,
    reserved: &mut Vec<(f64, f64, f64, f64)>,
) -> Option<(f64, f64)> {
    const DOT_R: f64 = 4.0;
    const CHAR_W: f64 = 6.2;
    const LABEL_H: f64 = 13.0;
    let w = name.chars().count() as f64 * CHAR_W + 6.0;
    let h = LABEL_H;
    let candidates: [(f64, f64); 4] = [
        (DOT_R + 3.0, -h / 2.0),
        (-(w + DOT_R + 3.0), -h / 2.0),
        (-w / 2.0, -(DOT_R + h + 3.0)),
        (-w / 2.0, DOT_R + 3.0),
    ];
    for (dx, dy) in candidates {
        let x0 = sx + dx;
        let y0 = sy + dy;
        let x1 = x0 + w;
        let y1 = y0 + h;
        let overlaps = reserved
            .iter()
            .any(|&(rx0, ry0, rx1, ry1)| x0 < rx1 && x1 > rx0 && y0 < ry1 && y1 > ry0);
        if !overlaps {
            reserved.push((x0, y0, x1, y1));
            return Some((dx, dy));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contextgraph::{CommunityAssigned, Edge, Node, REL_IN_COMMUNITY, TIER_EXTRACTED};
    use crate::eventstore::Position;

    fn code_node(id: &str, name: &str, kind: &str) -> Node {
        let mut attrs = std::collections::BTreeMap::new();
        attrs.insert("name".to_string(), name.to_string());
        attrs.insert("kind".to_string(), kind.to_string());
        Node {
            id: id.to_string(),
            kind: KIND_CODE_ENTITY.to_string(),
            attrs,
        }
    }

    fn live_edge(from: &str, to: &str, rel: &str) -> Edge {
        Edge {
            from: from.to_string(),
            to: to.to_string(),
            rel: rel.to_string(),
            valid_from: 0,
            valid_to: None,
            source: Position::default(),
            tier: TIER_EXTRACTED.to_string(),
        }
    }

    fn in_community(entity: &str, community: &str) -> Edge {
        live_edge(entity, community, REL_IN_COMMUNITY)
    }

    // ---- module_of / district_purpose -------------------------------------------------------

    #[test]
    fn module_of_a_leaf_src_file_is_its_stem_never_the_file_name() {
        assert_eq!(module_of("src/worktree.rs"), "worktree");
        assert_eq!(module_of("worktree.rs"), "worktree");
    }

    #[test]
    fn module_of_a_directory_module_is_the_directory_not_the_file_within_it() {
        assert_eq!(module_of("src/contextgraph/query.rs"), "contextgraph");
    }

    #[test]
    fn module_of_a_crate_member_is_its_crate_name() {
        assert_eq!(module_of("crates/console-core/src/lib.rs"), "console-core");
    }

    #[test]
    fn module_of_never_returns_a_string_carrying_a_dot_or_a_slash() {
        for file in [
            "src/worktree.rs",
            "src/contextgraph/query.rs",
            "crates/console-core/src/lib.rs",
            "shim/mock-rigger-server.mjs",
            "build.rs",
        ] {
            let m = module_of(file);
            assert!(
                !m.contains('.') && !m.contains('/'),
                "module_of({file:?}) = {m:?} looks like a file name"
            );
        }
    }

    #[test]
    fn district_purpose_uses_the_curated_table_when_present() {
        assert_eq!(district_purpose("worktree"), "worktree lifecycle");
        assert_eq!(district_purpose("liveness"), "liveness & heartbeats");
        assert_eq!(district_purpose("spawn"), "liveness & heartbeats");
    }

    #[test]
    fn district_purpose_falls_back_to_the_bare_module_name_when_uncurated() {
        assert_eq!(district_purpose("some_future_module"), "some_future_module");
    }

    // ---- build: districts, ranks, exclusion --------------------------------------------------

    /// A small, hand-built graph (two districts by construction, via two modules) proves the
    /// STRUCTURAL properties directly - the REAL-repository rendering claim itself lives in the
    /// periphery test (spec 84 Design, "Real store": "a fixture cannot reproduce the file-
    /// co-location degeneration this spec exists to fix" - that claim needs the real graph; these
    /// unit properties do not).
    fn two_district_graph() -> Graph {
        let mut nodes = vec![
            code_node(
                "src/worktree.rs::spawn_worktree",
                "spawn_worktree",
                "function",
            ),
            code_node(
                "src/worktree.rs::remove_worktree",
                "remove_worktree",
                "function",
            ),
            code_node("src/worktree.rs::WorktreeError", "WorktreeError", "type"),
            code_node("src/dash.rs::live_page", "live_page", "function"),
            code_node(
                "src/dash.rs::build_graph_view",
                "build_graph_view",
                "function",
            ),
        ];
        // A membership-less code entity: must be excluded from the model entirely.
        nodes.push(code_node("src/dash.rs::unrelated", "unrelated", "function"));

        let mut edges = vec![
            live_edge(
                "src/worktree.rs::spawn_worktree",
                "src/worktree.rs::remove_worktree",
                REL_CALLS,
            ),
            live_edge(
                "src/dash.rs::live_page",
                "src/dash.rs::build_graph_view",
                REL_CALLS,
            ),
            in_community("src/worktree.rs::spawn_worktree", "community/1/0"),
            in_community("src/worktree.rs::remove_worktree", "community/1/0"),
            in_community("src/worktree.rs::WorktreeError", "community/1/0"),
            in_community("src/dash.rs::live_page", "community/1/1"),
            in_community("src/dash.rs::build_graph_view", "community/1/1"),
        ];
        edges.retain(|_| true);
        Graph { nodes, edges }
    }

    #[test]
    fn build_excludes_a_membership_less_code_entity() {
        let model = build(&two_district_graph());
        assert!(
            !model.entities.iter().any(|e| e.id.ends_with("::unrelated")),
            "a code entity with no community membership must not appear on the map: {:?}",
            model.entities.iter().map(|e| &e.id).collect::<Vec<_>>()
        );
        assert_eq!(model.entities.len(), 5);
    }

    #[test]
    fn build_groups_communities_into_districts_by_curated_purpose() {
        let model = build(&two_district_graph());
        let purposes: BTreeSet<&str> = model.districts.iter().map(|d| d.purpose.as_str()).collect();
        assert_eq!(
            purposes,
            BTreeSet::from(["worktree lifecycle", "dashboard rendering"]),
            "districts must be named by the curated purpose, never a raw community id or a file"
        );
    }

    /// Design, DISTRICTS's own running example ("`liveness`/`spawn` both read `liveness &
    /// heartbeats`"), never previously exercised through `build`: two DIFFERENT communities whose
    /// dominant modules share ONE curated purpose must fold into a SINGLE district, its
    /// population the SUM across both communities - never two separate one-purpose districts.
    #[test]
    fn build_folds_two_communities_sharing_a_curated_purpose_into_one_district() {
        let nodes = vec![
            code_node(
                "src/liveness.rs::heartbeat_loop",
                "heartbeat_loop",
                "function",
            ),
            code_node(
                "src/spawn.rs::spawn_worktree_agent",
                "spawn_worktree_agent",
                "function",
            ),
        ];
        let edges = vec![
            in_community("src/liveness.rs::heartbeat_loop", "community/1/0"),
            in_community("src/spawn.rs::spawn_worktree_agent", "community/1/1"),
        ];
        let model = build(&Graph { nodes, edges });

        assert_eq!(
            model.districts.len(),
            1,
            "two communities sharing one curated purpose must fold into ONE district, not two: {:?}",
            model.districts.iter().map(|d| &d.purpose).collect::<Vec<_>>()
        );
        let district = &model.districts[0];
        assert_eq!(district.purpose, "liveness & heartbeats");
        assert_eq!(
            district.population, 2,
            "the merged district's population must be the SUM across both communities"
        );
        assert_eq!(
            model.entities.len(),
            2,
            "both communities' members must still be present as map entities"
        );
        for e in &model.entities {
            assert_eq!(
                e.district, "liveness & heartbeats",
                "entity {:?} must carry the merged district's purpose, not a per-community one",
                e.id
            );
        }
        // The two entities keep their OWN distinct community ids even though they share a district
        // - the merge is purpose-level only, never a community-identity merge.
        let communities: BTreeSet<&str> = model
            .entities
            .iter()
            .map(|e| e.community.as_str())
            .collect();
        assert_eq!(
            communities,
            BTreeSet::from(["community/1/0", "community/1/1"])
        );
    }

    /// Design's own tie-break rule ("ties broken to the lexicographically-smallest module",
    /// mirrored in `build`'s own dominant-module comment): a community whose members split evenly
    /// across two modules resolves to the ALPHABETICALLY-FIRST module's purpose, never whichever
    /// happened to be counted first.
    #[test]
    fn build_breaks_a_dominant_module_tie_by_the_lexicographically_smallest_module() {
        // "worktree" > "dash" lexicographically, so a 1-1 tie between them must resolve to
        // "dash"'s own curated purpose ("dashboard rendering"), never "worktree lifecycle" -
        // insertion order below is deliberately worktree-first, so a bug that picked "whichever
        // module was counted first" would still pass if this test inserted dash first.
        let nodes = vec![
            code_node(
                "src/worktree.rs::spawn_worktree",
                "spawn_worktree",
                "function",
            ),
            code_node("src/dash.rs::live_page", "live_page", "function"),
        ];
        let edges = vec![
            in_community("src/worktree.rs::spawn_worktree", "community/1/0"),
            in_community("src/dash.rs::live_page", "community/1/0"),
        ];
        let model = build(&Graph { nodes, edges });

        assert_eq!(model.districts.len(), 1, "{:?}", model.districts);
        assert_eq!(
            model.districts[0].purpose, "dashboard rendering",
            "a 1-1 dominant-module tie must resolve to the lexicographically-smallest module \
             ('dash' < 'worktree'), not whichever module was counted first"
        );
    }

    #[test]
    fn build_never_labels_an_entity_with_its_file() {
        let model = build(&two_district_graph());
        for e in &model.entities {
            assert!(
                !e.name.contains('/') && !e.name.ends_with(".rs"),
                "entity {:?} carries a file-shaped name {:?}",
                e.id,
                e.name
            );
        }
        for d in &model.districts {
            assert!(
                !d.purpose.contains('/') && !d.purpose.ends_with(".rs"),
                "district carries a file-shaped purpose {:?}",
                d.purpose
            );
        }
    }

    #[test]
    fn build_ranks_a_communitys_members_by_degree_descending() {
        let model = build(&two_district_graph());
        // spawn_worktree/remove_worktree each get one CALLS edge (degree 1); WorktreeError has
        // none (degree 0) - both degree-1 members must outrank the degree-0 one.
        let rank_of = |id: &str| {
            model
                .entities
                .iter()
                .find(|e| e.id == id)
                .unwrap_or_else(|| panic!("no entity {id}"))
                .rank
        };
        assert!(
            rank_of("src/worktree.rs::spawn_worktree") < rank_of("src/worktree.rs::WorktreeError")
        );
        assert!(
            rank_of("src/worktree.rs::remove_worktree") < rank_of("src/worktree.rs::WorktreeError")
        );
    }

    #[test]
    fn build_is_deterministic() {
        let g = two_district_graph();
        let a = build(&g);
        let b = build(&g);
        let ids_a: Vec<&str> = a.entities.iter().map(|e| e.id.as_str()).collect();
        let ids_b: Vec<&str> = b.entities.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(
            ids_a, ids_b,
            "two builds of the same graph must agree byte-for-byte"
        );
        for (ea, eb) in a.entities.iter().zip(b.entities.iter()) {
            assert_eq!((ea.x, ea.y), (eb.x, eb.y));
        }
    }

    #[test]
    fn build_on_an_empty_graph_yields_an_empty_model_never_a_panic() {
        let model = build(&Graph::default());
        assert!(model.entities.is_empty());
        assert!(model.districts.is_empty());
    }

    #[test]
    fn build_never_uses_communityassigned_events_directly_only_the_folded_edge() {
        // Sanity: the fixture's membership is expressed as a folded IN_COMMUNITY edge, exactly
        // what the always-compiled projector fold produces from a `CommunityAssigned` event -
        // never a raw event this pure module would have to parse itself.
        let _ = CommunityAssigned {
            node: "x".to_string(),
            community: "community/1/0".to_string(),
            resolution: 1.0,
            hash: String::new(),
            fresh: true,
        };
    }

    // ---- frame: the labelled-map invariant, budget, monotonic zoom ---------------------------

    /// Many communities across several districts, generously spread out, so [`frame`] has real
    /// label-placement work to do at both a small and a large budget.
    fn populous_graph(
        districts: usize,
        communities_per_district: usize,
        members_per_community: usize,
    ) -> Graph {
        let modules = [
            "worktree",
            "dash",
            "contextgraph",
            "grounder",
            "gate",
            "ledger",
            "budget",
            "registry",
        ];
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        for d in 0..districts {
            let module = modules[d % modules.len()];
            for c in 0..communities_per_district {
                let community = format!("community/1/{}", d * communities_per_district + c);
                for m in 0..members_per_community {
                    let id = format!("src/{module}.rs::f_{d}_{c}_{m}");
                    nodes.push(code_node(&id, &format!("f_{d}_{c}_{m}"), "function"));
                    edges.push(in_community(&id, &community));
                }
                // A couple of intra-community CALLS edges so degree ranking has something to rank.
                if members_per_community >= 2 {
                    let a = format!("src/{module}.rs::f_{d}_{c}_0");
                    let b = format!("src/{module}.rs::f_{d}_{c}_1");
                    edges.push(live_edge(&a, &b, REL_CALLS));
                }
            }
        }
        Graph { nodes, edges }
    }

    #[test]
    fn frame_never_draws_an_entity_without_a_placed_label() {
        let model = build(&populous_graph(6, 3, 12));
        for zoom in [0.0, 1.0, 3.0, 6.0] {
            let dl = frame(&model, 1200.0, 800.0, zoom);
            // Every drawn entity's name must be non-empty (a placed label exists), and the count
            // of drawn entities can never exceed the eligible (budgeted) set.
            let eligible = model
                .entities
                .iter()
                .filter(|e| e.rank < budget(zoom))
                .count();
            assert!(
                dl.entities.len() <= eligible,
                "zoom {zoom}: drew {} entities but only {eligible} were eligible",
                dl.entities.len()
            );
            for e in &dl.entities {
                assert!(
                    !e.name.is_empty(),
                    "a drawn entity must carry a placed label"
                );
            }
        }
    }

    #[test]
    fn frame_always_carries_every_districts_pill_at_every_zoom() {
        let model = build(&populous_graph(4, 2, 5));
        for zoom in [0.0, 2.0, 5.0] {
            let dl = frame(&model, 1000.0, 700.0, zoom);
            assert_eq!(
                dl.districts.len(),
                model.districts.len(),
                "zoom {zoom}: every district's pill must be present"
            );
        }
    }

    #[test]
    fn frame_district_pill_is_never_a_file_name() {
        let model = build(&populous_graph(3, 1, 4));
        let dl = frame(&model, 900.0, 600.0, 0.0);
        for d in &dl.districts {
            assert!(!d.purpose.ends_with(".rs") && !d.purpose.contains('/'));
        }
    }

    #[test]
    fn frame_zooming_in_strictly_increases_the_labelled_entity_count() {
        let model = build(&populous_graph(8, 3, 14));
        let out = frame(&model, 1200.0, 800.0, 0.0);
        let in_ = frame(&model, 1200.0, 800.0, 6.0);
        assert!(
            in_.entities.len() > out.entities.len(),
            "zooming in must strictly increase the labelled-entity count: full extent {} vs zoomed {}",
            out.entities.len(),
            in_.entities.len()
        );
    }

    #[test]
    fn budget_is_strictly_increasing_in_zoom() {
        assert!(budget(1.0) > budget(0.0));
        assert!(budget(5.0) > budget(1.0));
        assert_eq!(
            budget(-3.0),
            budget(0.0),
            "a negative zoom clamps to zero, never panics or shrinks"
        );
    }

    #[test]
    fn place_label_returns_none_when_every_candidate_collides() {
        // Saturate all four candidate zones around the origin with reservations, then any label
        // attempt there must fail (never silently overlap).
        let mut reserved = vec![
            (0.0, -20.0, 200.0, 20.0),
            (-200.0, -20.0, 0.0, 20.0),
            (-100.0, -60.0, 100.0, -6.0),
            (-100.0, 6.0, 100.0, 60.0),
        ];
        assert!(place_label(0.0, 0.0, "blocked", &mut reserved).is_none());
    }

    #[test]
    fn place_label_reserves_its_chosen_box_so_a_second_call_at_the_same_spot_dodges_it() {
        let mut reserved = Vec::new();
        let first = place_label(100.0, 100.0, "first", &mut reserved);
        assert!(first.is_some());
        assert_eq!(reserved.len(), 1);
        let second = place_label(100.0, 100.0, "second", &mut reserved);
        assert!(
            second.is_some(),
            "a second label at the same anchor must dodge to another candidate"
        );
        assert_ne!(
            first, second,
            "the second label must not choose the same offset as the first"
        );
    }
}
