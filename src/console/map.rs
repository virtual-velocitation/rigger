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
//! the degree rank, and label placement are criterion 1's; the explore rail, search, selection
//! and the interactive camera are THIS criterion's (spec 84 criterion 2, "EXPLORATION NEEDS NO
//! VOCABULARY" - [`landmarks`], [`bridges_between_districts`], [`changing_right_now`] and
//! [`argued_about_in_review`] are the rail's four candidate lists; [`search`] the search box;
//! [`hit`] hit-testing a click; [`frame`]'s own `camera`/`selection` params the pan/zoom/lit-
//! selection [`fit_whole_map`] and [`fit_district`] are the only two ways a camera resets); the
//! legend and the text TREATMENTS (underline/italic styling) are criterion 3's ([`legend`] and
//! [`kind_colour`] - the entity-dot palette and the named row for every other visual class),
//! rendered by the page from this module's plain draw list.

use std::collections::{BTreeMap, BTreeSet};

use crate::contextgraph::query::{file_of, name_suffix, Buckets, Lens};
use crate::contextgraph::{
    Graph, Node, KIND_CODE_ENTITY, KIND_FINDING, REL_ABOUT, REL_CALLS, REL_REFERENCES,
};

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
/// `lit` (spec 84 criterion 2, SEMANTIC ZOOM: "the selected entity is underlined") is true for
/// the selected entity itself AND its drawn callers/callees - criterion 3's own text-treatment
/// styling reads this flag; this module computes the FACT, never the styling.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DrawEntity {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub x: f64,
    pub y: f64,
    pub lit: bool,
}

/// One typed edge between two DRAWN entities. `lit` (Design, SEMANTIC ZOOM: "a lit edge shows
/// its relation type ... at its midpoint") is true when the edge touches the current selection.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DrawEdge {
    pub from: String,
    pub to: String,
    pub rel: String,
    pub lit: bool,
}

/// One neighbour row on the selection card (Design, THE CARD: "CALLED BY and CALLS rows listing
/// neighbours by name with the relation type") - the OTHER endpoint's id/name plus the edge's own
/// relation, so a page never resolves an id to a name itself.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct NeighborRef {
    pub id: String,
    pub name: String,
    pub rel: String,
}

/// The selected entity's card (spec 84 criterion 2; Design's SEMANTIC ZOOM: "clicking an entity
/// lights its callers and callees and lists them BY NAME on the card"). `degree` is the entity's
/// HONEST whole-map degree (Design, RENDER BUDGET: "a hub ... shows ... an honest degree on the
/// card, never a hairball") even when `called_by`/`calls` are themselves capped to
/// [`CARD_NEIGHBOR_CAP`] top neighbours by degree (Design, CONSTRAINTS WALK's hub carve-out: "the
/// card shows the honest degree and the top neighbours by degree").
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SelectedCard {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub district: String,
    pub degree: usize,
    pub called_by: Vec<NeighborRef>,
    pub calls: Vec<NeighborRef>,
}

/// One frame's whole draw list: every district pill (always present, Design's own words),
/// every LABELLED entity dot (never an unlabelled one), every edge between two drawn entities,
/// and the current selection's own card (`None` when nothing is selected).
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct DrawList {
    pub districts: Vec<DrawDistrict>,
    pub entities: Vec<DrawEntity>,
    pub edges: Vec<DrawEdge>,
    pub selected: Option<SelectedCard>,
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

/// The interactive camera (spec 84 criterion 2; Design, CAMERA: "scroll zooms about the cursor,
/// drag pans"). `zoom <= 0.0` is the FULL EXTENT sentinel c1 already defined (Design, SEED) and,
/// in that case ALONE, `cx`/`cy` are ignored in favour of the model's own bounds centroid - so a
/// default `Camera` (`zoom: 0.0`) renders EXACTLY what c1's zoom-only `frame` always rendered.
/// At `zoom > 0.0`, `(cx, cy)` is the WORLD-SPACE point the viewport centres on (the pan this
/// criterion adds), and `zoom` is a DIRECT multiplier of the model's own world-bounds fit-to-
/// viewport scale (see [`base_fit_scale`]) - `zoom: 1.0` renders at exactly that base fit
/// scale (panned, never bigger or smaller than the full extent), `zoom` above `1.0` zooms in
/// past it, and `zoom` between `0.0` (exclusive) and `1.0` zooms OUT below it, so a positive
/// zoom can represent ANY scale, not only ones at or above the full-extent fit (round 2: a
/// district whose own hull already spans the whole map still needs a scale BELOW the full-
/// extent fit to leave [`DISTRICT_FIT_FRACTION`]'s own margin around it - see [`fit_district`]).
/// Panning alone (holding `zoom` fixed) never changes scale, only the visible window - ordinary
/// map-camera semantics. A page never hand-assembles a `Camera` other than forwarding its own
/// scroll/drag deltas - [`fit_whole_map`], [`fit_district`] and [`fit_entity`] are the only
/// ways this module RESETS one (Design, CAMERA: "the camera never resets except through
/// fit-whole-map or a district double-click").
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Camera {
    pub cx: f64,
    pub cy: f64,
    pub zoom: f64,
}

/// spec 84 criterion 2's own NEIGHBOR CAP for a selected entity: how many of its own callers/
/// callees the MAP draws beyond the zoom budget, and how many the CARD lists - ONE fixed number
/// for both (Design, CONSTRAINTS WALK, two paragraphs folded together: "a selected entity whose
/// neighbours are outside the rank budget - they are drawn anyway" for an ordinary entity, and
/// the hub carve-out "the card shows the honest degree and the top neighbours by degree; the map
/// draws those within the budget" for one with hundreds). An ordinary entity's neighbours all fit
/// under this cap, so "drawn anyway" holds in full; a hub's are capped to its own top-by-degree,
/// never literally hundreds of dots.
const CARD_NEIGHBOR_CAP: usize = 20;

/// Which side of an edge a neighbour sits on relative to the selection - Design's own CALLED BY
/// (`to == selection`) vs CALLS (`from == selection`) card rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dir {
    CalledBy,
    Calls,
}

/// The selection's own neighbours on one [`Dir`] side, ranked by neighbour degree descending (id
/// ascending on a tie) and capped to [`CARD_NEIGHBOR_CAP`] - the ONE computation both [`frame`]'s
/// always-visible-neighbour eligibility and the [`SelectedCard`]'s row lists read, so the two can
/// never drift apart (a neighbour drawn on the map is always a neighbour the card lists, and vice
/// versa).
fn capped_neighbors<'a>(
    model: &'a MapModel,
    by_id: &BTreeMap<&str, &'a MapEntity>,
    sel: &str,
    dir: Dir,
) -> Vec<(&'a MapEntity, &'a str)> {
    let mut v: Vec<(&MapEntity, &str)> = model
        .edges
        .iter()
        .filter_map(|(from, to, rel)| match dir {
            Dir::CalledBy if to == sel => Some((from.as_str(), rel.as_str())),
            Dir::Calls if from == sel => Some((to.as_str(), rel.as_str())),
            _ => None,
        })
        .filter_map(|(id, rel)| by_id.get(id).map(|e| (*e, rel)))
        .collect();
    v.sort_by(|a, b| {
        b.0.degree
            .cmp(&a.0.degree)
            .then_with(|| a.0.id.cmp(&b.0.id))
    });
    v.truncate(CARD_NEIGHBOR_CAP);
    v
}

/// The selected entity's [`SelectedCard`] plus the id set of every neighbour [`frame`] must draw
/// regardless of the zoom budget - `None` when `sel` names no entity this model carries (an
/// unknown/stale selection is a graceful no-op, never a panic).
fn selection_card_and_neighbors<'a>(
    model: &'a MapModel,
    by_id: &BTreeMap<&str, &'a MapEntity>,
    sel: &str,
) -> Option<(SelectedCard, BTreeSet<&'a str>)> {
    let entity = *by_id.get(sel)?;
    let called_by = capped_neighbors(model, by_id, sel, Dir::CalledBy);
    let calls = capped_neighbors(model, by_id, sel, Dir::Calls);
    let neighbor_ids: BTreeSet<&str> = called_by
        .iter()
        .chain(calls.iter())
        .map(|(e, _)| e.id.as_str())
        .collect();
    let to_refs = |ns: &[(&MapEntity, &str)]| -> Vec<NeighborRef> {
        ns.iter()
            .map(|(e, rel)| NeighborRef {
                id: e.id.clone(),
                name: e.name.clone(),
                rel: (*rel).to_string(),
            })
            .collect()
    };
    let card = SelectedCard {
        id: entity.id.clone(),
        name: entity.name.clone(),
        kind: entity.kind.clone(),
        district: entity.district.clone(),
        degree: entity.degree,
        called_by: to_refs(&called_by),
        calls: to_refs(&calls),
    };
    Some((card, neighbor_ids))
}

/// How much margin the world bounding box leaves around itself when scaled to fill a viewport -
/// shared by every "fit the whole world (or a piece of it) to the viewport" computation
/// ([`frame`]'s own full-extent/base scale and [`fit_district`]'s target scale) so the two can
/// never independently drift (round 2 fix: this used to be a second, hand-copied `0.9` constant
/// inside `fit_district` alone).
const FIT_MARGIN: f64 = 0.9;

/// THE ONE fit-to-viewport BASE SCALE authority (round 2 fix for
/// `arch-u84c2-fit-district-duplicates-frame-scale`): given `bounds`, how much a `viewport_w` x
/// `viewport_h` canvas must scale the world to make the whole bounding box fit inside it (with
/// [`FIT_MARGIN`]'s own margin) - [`frame`]'s own full-extent scale AND the ONE formula
/// [`fit_district`] solves for its own target zoom, never two independently-maintained copies.
fn base_fit_scale(bounds: (f64, f64, f64, f64), viewport_w: f64, viewport_h: f64) -> f64 {
    let (min_x, min_y, max_x, max_y) = bounds;
    let world_w = (max_x - min_x).max(1.0);
    let world_h = (max_y - min_y).max(1.0);
    (viewport_w / world_w).min(viewport_h / world_h) * FIT_MARGIN
}

/// Render one frame of `model` at `camera` for a `viewport_w` x `viewport_h` canvas, highlighting
/// `selection` (an entity id, or `None`). `camera.zoom <= 0.0` is the FULL EXTENT (Design, SEED:
/// "the initial camera is the full extent"): the whole world bounding box scaled to fit the
/// viewport (with a small margin), centred on its own centroid; at a positive zoom the viewport
/// centres on `camera.cx`/`camera.cy` instead (see [`Camera`]'s own doc).
///
/// THE LABELLED-MAP INVARIANT, enforced by construction: district pills are placed FIRST,
/// unconditionally, and reserve their screen bounding box - so "entity labels never displace
/// district labels" (Design) holds because an entity candidate overlapping a pill is simply
/// rejected, never placed over it. Entities are then walked in `model.entities`'s FIXED rank-primary
/// order; an entity is ELIGIBLE when its rank is under this zoom's [`budget`] OR it is the
/// selection itself or one of its [`capped_neighbors`] (Design, CONSTRAINTS WALK: "a selected
/// entity whose neighbours are outside the rank budget - they are drawn anyway"); an eligible
/// entity whose label cannot be placed at any of [`place_label`]'s four candidates is skipped
/// regardless - in every skip case it is never pushed onto [`DrawList::entities`], so every
/// returned entity carries a placed label and no unlabelled node is ever drawn.
pub fn frame(
    model: &MapModel,
    viewport_w: f64,
    viewport_h: f64,
    camera: &Camera,
    selection: Option<&str>,
) -> DrawList {
    let (min_x, min_y, max_x, max_y) = model.bounds;
    let zoom = camera.zoom;
    let (world_cx, world_cy) = if zoom > 0.0 {
        (camera.cx, camera.cy)
    } else {
        ((min_x + max_x) / 2.0, (min_y + max_y) / 2.0)
    };

    let fit = base_fit_scale(model.bounds, viewport_w, viewport_h);
    // `zoom` is a DIRECT multiplier of `fit` (see [`Camera`]'s own doc) - `zoom <= 0.0` (the
    // full-extent sentinel branch above) always renders at exactly `fit`; a positive `zoom`
    // scales `fit` up OR down, so this - unlike a `zoom.max(0.0) * step` offset - can represent
    // a scale BELOW `fit` too (needed by [`fit_district`] for a district whose hull already
    // spans the whole map).
    let scale = if zoom > 0.0 { fit * zoom } else { fit };

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

    let by_id: BTreeMap<&str, &MapEntity> =
        model.entities.iter().map(|e| (e.id.as_str(), e)).collect();
    let sel_card = selection.and_then(|sel| selection_card_and_neighbors(model, &by_id, sel));
    let sel_neighbor_ids: BTreeSet<&str> = sel_card
        .as_ref()
        .map(|(_, ids)| ids.clone())
        .unwrap_or_default();

    let visible_rank = budget(zoom);
    let mut drawn: BTreeSet<&str> = BTreeSet::new();
    let mut entities = Vec::new();
    for e in &model.entities {
        let is_selected = selection == Some(e.id.as_str());
        let is_neighbor = sel_neighbor_ids.contains(e.id.as_str());
        if e.rank >= visible_rank && !is_selected && !is_neighbor {
            continue;
        }
        let (sx, sy) = project(e.x, e.y);
        if place_label(sx, sy, &e.name, &mut reserved).is_some() {
            drawn.insert(e.id.as_str());
            let lit = is_selected || is_neighbor;
            entities.push(DrawEntity {
                id: e.id.clone(),
                name: e.name.clone(),
                kind: e.kind.clone(),
                x: sx,
                y: sy,
                lit,
            });
        }
        // else: the entity's label could not be placed - it is NOT drawn (the invariant), even
        // when it is the selection or one of its neighbours - the labelled-map invariant beats
        // "always visible" exactly as it beats zoom-budget density.
    }

    let edges = model
        .edges
        .iter()
        .filter(|(from, to, _)| drawn.contains(from.as_str()) && drawn.contains(to.as_str()))
        .map(|(from, to, rel)| DrawEdge {
            from: from.clone(),
            to: to.clone(),
            rel: rel.clone(),
            // A lit edge is one touching the SELECTION itself (Design, SEMANTIC ZOOM: "a lit
            // edge shows its relation type ... at its midpoint") - never merely an edge between
            // two OTHER lit (neighbour) entities, which would light edges the click never
            // actually explored.
            lit: selection
                .map(|sel| from.as_str() == sel || to.as_str() == sel)
                .unwrap_or(false),
        })
        .collect();

    DrawList {
        districts,
        entities,
        edges,
        selected: sel_card.map(|(card, _)| card),
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

// ---- criterion 2: hit-testing, the explore rail, search, and the reset-only camera -----------

/// What a click on the rendered map landed on (spec 84 criterion 2's own `map_hit`): a code
/// entity's dot, or a district's hull - "double-click a district fits it" needs to tell the two
/// apart from a click on one of its own members.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hit {
    Entity(String),
    District(String),
}

/// How close (in SCREEN pixels) a click must land to an entity's dot to hit it - generous enough
/// for an imprecise pointer, small enough that two adjacent dots stay individually clickable
/// (dots are placed at least [`RING_STEP`]-derived world spacing apart, which even at the
/// highest zoom this crate's own tests reach never collapses two dots inside this radius of one
/// another - see `hit_prefers_the_nearest_entity_when_two_are_within_radius` for the tie-break
/// this constant's generosity makes reachable).
const HIT_RADIUS_PX: f64 = 10.0;

/// Hit-test a SCREEN-space click `(x, y)` against the frame [`frame`] itself would render for
/// this exact `camera`/`selection` - reusing `frame`'s OWN projection (by calling it directly)
/// so hit-testing can never drift from what is actually drawn on screen; a dot the labelled-map
/// invariant kept off-screen (an unplaceable label, or outside the zoom budget) is exactly as
/// un-hittable as it is invisible. Entities win over districts on any overlap (Design: clicking
/// a dot must always select that entity, never the district hull beneath it) - only when NO
/// entity is within [`HIT_RADIUS_PX`] does a district hull get checked, and only then does a
/// district hit register. `None` when the click lands on neither.
pub fn hit(
    model: &MapModel,
    viewport_w: f64,
    viewport_h: f64,
    camera: &Camera,
    selection: Option<&str>,
    x: f64,
    y: f64,
) -> Option<Hit> {
    let draw = frame(model, viewport_w, viewport_h, camera, selection);

    let mut nearest_entity: Option<(f64, &str)> = None;
    for e in &draw.entities {
        let d = ((e.x - x).powi(2) + (e.y - y).powi(2)).sqrt();
        if d <= HIT_RADIUS_PX && nearest_entity.is_none_or(|(bd, _)| d < bd) {
            nearest_entity = Some((d, e.id.as_str()));
        }
    }
    if let Some((_, id)) = nearest_entity {
        return Some(Hit::Entity(id.to_string()));
    }

    let mut nearest_district: Option<(f64, &str)> = None;
    for d in &draw.districts {
        let dist = ((d.x - x).powi(2) + (d.y - y).powi(2)).sqrt();
        if dist <= d.radius && nearest_district.is_none_or(|(bd, _)| dist < bd) {
            nearest_district = Some((dist, d.purpose.as_str()));
        }
    }
    nearest_district.map(|(_, purpose)| Hit::District(purpose.to_string()))
}

/// One always-available Explore rail chip's target (Design, EXPLORE RAIL): an entity id/name/kind
/// the page can fly the camera to and select, with no vocabulary of its own required to find it.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct RailCandidate {
    pub id: String,
    pub name: String,
    pub kind: String,
}

fn as_candidate(e: &MapEntity) -> RailCandidate {
    RailCandidate {
        id: e.id.clone(),
        name: e.name.clone(),
        kind: e.kind.clone(),
    }
}

/// Landmarks (Design, EXPLORE RAIL: "busiest entities") - the whole map's top `limit` entities by
/// WHOLE-MAP degree, ties broken by id. Never by [`MapEntity::rank`], which is only a
/// per-community rank c1's zoom budget reads - a landmark is busy ACROSS the whole map, not
/// merely dominant within its own small community.
pub fn landmarks(model: &MapModel, limit: usize) -> Vec<RailCandidate> {
    let mut v: Vec<&MapEntity> = model.entities.iter().collect();
    v.sort_by(|a, b| b.degree.cmp(&a.degree).then_with(|| a.id.cmp(&b.id)));
    v.into_iter().take(limit).map(as_candidate).collect()
}

/// Bridges between districts (Design, EXPLORE RAIL: "entities with the most cross-district
/// edges") - ranked by how many of an entity's OWN edges cross into a DIFFERENT district, never
/// by whole-map degree (a busy entity wholly inside one district is a landmark, not a bridge).
/// An entity with zero cross-district edges never appears - a "bridge" chip must bridge something.
pub fn bridges_between_districts(model: &MapModel, limit: usize) -> Vec<RailCandidate> {
    let by_id: BTreeMap<&str, &MapEntity> =
        model.entities.iter().map(|e| (e.id.as_str(), e)).collect();
    let mut crossings: BTreeMap<&str, usize> = BTreeMap::new();
    for (from, to, _) in &model.edges {
        let (Some(a), Some(b)) = (by_id.get(from.as_str()), by_id.get(to.as_str())) else {
            continue;
        };
        if a.district != b.district {
            *crossings.entry(from.as_str()).or_default() += 1;
            *crossings.entry(to.as_str()).or_default() += 1;
        }
    }
    let mut v: Vec<(&MapEntity, usize)> = crossings
        .into_iter()
        .filter_map(|(id, n)| by_id.get(id).map(|e| (*e, n)))
        .collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.id.cmp(&b.0.id)));
    v.into_iter()
        .take(limit)
        .map(|(e, _)| as_candidate(e))
        .collect()
}

/// Changing right now (Design, EXPLORE RAIL: "the live run's blast radius") - the map's own
/// entities among `touched`, ranked by whole-map degree. This module has no way to know whether
/// a run is live or what it is touching (that is the RUN's own event log, outside a pure function
/// over a `Graph`), so the CALLER (the console-core session, which already folds that log) hands
/// the touched-entity-id set in; an empty `touched` (no run is live, or a live run has not yet
/// computed a blast radius) answers an empty list - the EMPTY-STATE Design names, never a
/// fabricated placeholder.
pub fn changing_right_now(
    model: &MapModel,
    touched: &BTreeSet<String>,
    limit: usize,
) -> Vec<RailCandidate> {
    let mut v: Vec<&MapEntity> = model
        .entities
        .iter()
        .filter(|e| touched.contains(&e.id))
        .collect();
    v.sort_by(|a, b| b.degree.cmp(&a.degree).then_with(|| a.id.cmp(&b.id)));
    v.into_iter().take(limit).map(as_candidate).collect()
}

/// Argued about in review (Design, EXPLORE RAIL: "entities with findings pinned") - map entities
/// with at least one live [`KIND_FINDING`] node reachable by a live [`REL_ABOUT`] edge (a
/// finding's own `from`, per this graph's convention - see `mcpserver`'s own ReviewFinding fold),
/// ranked by how MANY findings are pinned, ties by id. Purely a graph read (spec 29b's own
/// findings-as-graph-nodes ingest already puts these on `graph`) - unlike [`changing_right_now`],
/// which needs data this module has no way to derive on its own.
pub fn argued_about_in_review(model: &MapModel, graph: &Graph, limit: usize) -> Vec<RailCandidate> {
    let by_id: BTreeMap<&str, &MapEntity> =
        model.entities.iter().map(|e| (e.id.as_str(), e)).collect();
    let finding_ids: BTreeSet<&str> = graph
        .nodes
        .iter()
        .filter(|n| n.kind == KIND_FINDING)
        .map(|n| n.id.as_str())
        .collect();
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for e in &graph.edges {
        if e.valid_to.is_some() || e.rel != REL_ABOUT || !finding_ids.contains(e.from.as_str()) {
            continue;
        }
        if by_id.contains_key(e.to.as_str()) {
            *counts.entry(e.to.as_str()).or_default() += 1;
        }
    }
    let mut v: Vec<(&MapEntity, usize)> = counts
        .into_iter()
        .filter_map(|(id, n)| by_id.get(id).map(|e| (*e, n)))
        .collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.id.cmp(&b.0.id)));
    v.into_iter()
        .take(limit)
        .map(|(e, _)| as_candidate(e))
        .collect()
}

/// One search hit (Design, EXPLORE RAIL: "a search box (prefix and substring, kind and degree
/// beside each hit)").
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SearchHit {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub degree: usize,
}

/// Search (Design, EXPLORE RAIL: "prefix and substring, kind and degree beside each hit") - a
/// case-insensitive match over every map entity's OWN display name (never its raw id, which
/// would leak the file - the SAME name-not-id discipline [`build`] already keeps for the map's
/// own labels). Prefix hits rank above substring-only hits; within each tier, higher degree
/// first, then id for a deterministic tie-break. An empty `query` answers an empty list - a
/// blank search box is not "match everything".
pub fn search(model: &MapModel, query: &str, limit: usize) -> Vec<SearchHit> {
    if query.is_empty() {
        return Vec::new();
    }
    let q = query.to_lowercase();
    let mut prefix: Vec<&MapEntity> = Vec::new();
    let mut substring: Vec<&MapEntity> = Vec::new();
    for e in &model.entities {
        let name = e.name.to_lowercase();
        if name.starts_with(&q) {
            prefix.push(e);
        } else if name.contains(&q) {
            substring.push(e);
        }
    }
    let by_rank =
        |a: &&MapEntity, b: &&MapEntity| b.degree.cmp(&a.degree).then_with(|| a.id.cmp(&b.id));
    prefix.sort_by(by_rank);
    substring.sort_by(by_rank);
    prefix
        .into_iter()
        .chain(substring)
        .take(limit)
        .map(|e| SearchHit {
            id: e.id.clone(),
            name: e.name.clone(),
            kind: e.kind.clone(),
            degree: e.degree,
        })
        .collect()
}

/// Fit whole map (Design, EXPLORE RAIL and CAMERA: "a 'fit whole map' control returns to the full
/// extent") - the one camera [`frame`]'s own FULL EXTENT sentinel already renders
/// (`camera.zoom <= 0.0`), returned as an explicit [`Camera`] value so a page never has to know
/// that sentinel exists on its own; it only ever calls this fn or [`fit_district`] to RESET the
/// camera (Design, CAMERA: "the camera never resets except through fit-whole-map or a district
/// double-click" - these two functions are the only way this module resets one).
pub fn fit_whole_map(model: &MapModel) -> Camera {
    let (min_x, min_y, max_x, max_y) = model.bounds;
    Camera {
        cx: (min_x + max_x) / 2.0,
        cy: (min_y + max_y) / 2.0,
        zoom: 0.0,
    }
}

/// How much of the smaller viewport dimension a double-clicked district's own hull should fill
/// after [`fit_district`] - generous enough to read as "fit", short of the whole viewport so a
/// district's own label and its neighbours' context stay visible around it.
const DISTRICT_FIT_FRACTION: f64 = 0.35;

/// Fit district (Design, CAMERA: "double-click a district fits it") - a camera centred on the
/// named district whose zoom makes its own hull occupy [`DISTRICT_FIT_FRACTION`] of the smaller
/// viewport dimension, using the SAME fit-to-viewport scale formula [`frame`] itself computes
/// ([`base_fit_scale`], never a second one), solved for the zoom that hits the target radius -
/// ABOVE `base_fit_scale`'s own scale for a district small relative to the whole map, but just as
/// validly BELOW it for a district whose own hull already spans (or exceeds) the whole map (round
/// 2 fix: `Camera::zoom` being a direct scale multiplier, per that type's own doc, is what makes a
/// sub-base-fit target representable at all - the old `1 + zoom*0.5` encoding could only ever grow
/// past `base_fit_scale`, never shrink below it, so this case used to silently clamp at the
/// full-extent floor instead). `None` for an unknown purpose - the caller's own double-click
/// already named a real district (via [`hit`]'s own `Hit::District`), so this is a defensive
/// contract, not a documented UI path.
pub fn fit_district(
    model: &MapModel,
    viewport_w: f64,
    viewport_h: f64,
    purpose: &str,
) -> Option<Camera> {
    let d = model.districts.iter().find(|d| d.purpose == purpose)?;
    let base_fit = base_fit_scale(model.bounds, viewport_w, viewport_h);
    let target_radius_px = DISTRICT_FIT_FRACTION * viewport_w.min(viewport_h) / 2.0;
    let scale_needed = if d.radius > 0.0 {
        target_radius_px / d.radius
    } else {
        base_fit
    };
    // `frame`'s own `scale = fit * zoom` for a positive zoom (see `Camera`'s own doc), solved
    // directly for zoom; floored just above zero so this NEVER answers the full-extent sentinel
    // `frame` would otherwise reinterpret as "ignore cx/cy" - a district fit must always pan, at
    // whatever scale (above OR below `base_fit`) actually hits the target radius.
    let zoom = (scale_needed / base_fit).max(0.0001);
    Some(Camera {
        cx: d.x,
        cy: d.y,
        zoom,
    })
}

/// Fit entity (Design, EXPLORE RAIL: "each chip flies the camera to that entity and selects it";
/// round 2 fix for `sdet-u84c2-rail-candidates-carry-no-camera-target`) - a camera centred on the
/// named entity's own world coordinates, at the SAME zoom [`fit_district`] would compute for that
/// entity's own district (never a second zoom formula): flying to an entity means flying into its
/// neighbourhood at reading distance, exactly the scale a district double-click already gives
/// that neighbourhood, just re-centred on the entity's own point rather than its district's
/// centroid. `None` for an unknown id (a stale rail chip or search hit id, never a documented UI
/// path once a page only ever forwards ids [`landmarks`]/[`bridges_between_districts`]/
/// [`changing_right_now`]/[`argued_about_in_review`]/[`search`] themselves just returned) or - a
/// purely defensive contract, unreachable from [`build`]'s own output, which always assigns every
/// entity a district that exists - one whose own `district` names no district in
/// [`MapModel::districts`].
pub fn fit_entity(model: &MapModel, viewport_w: f64, viewport_h: f64, id: &str) -> Option<Camera> {
    let e = model.entities.iter().find(|e| e.id == id)?;
    let district_cam = fit_district(model, viewport_w, viewport_h, &e.district)?;
    Some(Camera {
        cx: e.x,
        cy: e.y,
        zoom: district_cam.zoom,
    })
}

// ---- legend / text treatments (spec 84 criterion 3) --------------------------------------------
//
// "OWNS the legend and the text treatments; introduces no new render data" (Done-when c3): every
// row below reads a fact `build`/`frame` already compute (`DrawEntity::kind`, `DrawDistrict`,
// `DrawEdge::rel`/`lit`) or a constant declared right here - no new field on any `Draw*` wire
// type. `kind_colour` is the ONE palette both the legend's own kind rows AND, once a page exists,
// its canvas paint read (Design, SEMANTIC ZOOM: "Landmark names ... take their kind's colour"),
// so the two can never drift into two different palettes.

/// THE MAP's four KIND COLOURS (Design, LEGEND: "the entity dot and its four kind colours") - the
/// only definition kinds a reader reasons about by sight: a function, a type (struct/enum/class/
/// interface all fold to `type` - see `src/grounder/symbols/extract.rs::kind_of`), a trait, a
/// constant. `(kind, label, colour)` triples, in the legend's own display order.
pub const KIND_COLOURS: &[(&str, &str, &str)] = &[
    ("function", "Function", "#6ea8fe"),
    ("type", "Type", "#4ade80"),
    ("trait", "Trait", "#a78bfa"),
    ("constant", "Constant", "#f472b6"),
];

/// The neutral colour a [`DrawEntity::kind`] outside [`KIND_COLOURS`] paints with (`method`,
/// `impl`, `module`, `other` - see `src/grounder/symbols/events.rs::kind_str` - or an empty/
/// unknown kind): these are structural definition kinds, not vocabulary Design's own "four kind
/// colours" distinguishes on sight, so they share ONE default rather than growing a fifth
/// hand-picked colour.
pub const KIND_COLOUR_DEFAULT: &str = "#94a3b8";

/// The amber blast-radius ring's own colour (Design, LEGEND: "the amber ring for a live unit's
/// blast radius"). The run-activity overlay that lights an in-flight unit's touched entities
/// (docs/architecture-addendum-mission-control.md section 6.4) is a later spec's own render - this
/// module computes no per-entity blast-radius flag ([`changing_right_now`] already answers WHICH
/// entities a caller says are touched) - but the legend names the ring's colour here, the one
/// place every other swatch is named too, so a reader checks a single legend, never two.
pub const BLAST_RADIUS_COLOUR: &str = "#fbbf24";

/// The colour a [`DrawEntity`]'s dot, and its name label once a page paints one, take - keyed by
/// the entity's own `kind` string. The single authority [`legend`]'s own kind rows read; a page's
/// canvas paint reads the SAME function, never a second lookup table.
pub fn kind_colour(kind: &str) -> &'static str {
    KIND_COLOURS
        .iter()
        .find(|(k, _, _)| *k == kind)
        .map(|(_, _, c)| *c)
        .unwrap_or(KIND_COLOUR_DEFAULT)
}

/// One row of THE LEGEND (Design, LEGEND: "a persistent legend on the canvas names every visual
/// class ... so a reader never has to infer what a mark belongs to"). `id` is the row's stable
/// key (a page's CSS class name); `colour` is `None` for a row whose treatment carries no fixed
/// swatch of its own (a district pill's ink is not kind-specific; the lit selection is a stroke
/// treatment, not a colour).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct LegendEntry {
    pub id: String,
    pub label: String,
    pub colour: Option<String>,
    pub treatment: String,
}

/// THE LEGEND itself: one row per visual class Design's own LEGEND paragraph names, in that
/// paragraph's order - the district pill, the entity dot for each of [`KIND_COLOURS`]' four
/// kinds, the typed directed edge, the lit selection (and its neighbours), and the blast-radius
/// ring. A page's CSS class names key off `id`/`treatment` here - one source for both the legend
/// text and the canvas paint, never a second hand-copied list.
pub fn legend() -> Vec<LegendEntry> {
    let mut entries = vec![LegendEntry {
        id: "district-pill".to_string(),
        label: "District (purpose, population)".to_string(),
        colour: None,
        treatment: "small-caps-pill".to_string(),
    }];
    for (kind, label, _) in KIND_COLOURS {
        entries.push(LegendEntry {
            id: format!("entity-{kind}"),
            label: (*label).to_string(),
            // Read through kind_colour (never the KIND_COLOURS tuple's own third field
            // directly) so this row is provably the SAME colour a page's canvas paint would
            // get for this kind - one authority, never two lookups that could drift.
            colour: Some(kind_colour(kind).to_string()),
            treatment: "kind-colour-dot".to_string(),
        });
    }
    entries.push(LegendEntry {
        id: "edge".to_string(),
        label: format!(
            "Typed directed edge ({}, {})",
            REL_CALLS.to_lowercase(),
            REL_REFERENCES.to_lowercase()
        ),
        colour: None,
        treatment: "arrowhead".to_string(),
    });
    entries.push(LegendEntry {
        id: "selection".to_string(),
        label: "Selected entity and its lit neighbours".to_string(),
        colour: None,
        treatment: "underline-name-italic-relation".to_string(),
    });
    entries.push(LegendEntry {
        id: "blast-radius".to_string(),
        label: "Live unit's blast radius".to_string(),
        colour: Some(BLAST_RADIUS_COLOUR.to_string()),
        treatment: "amber-ring".to_string(),
    });
    entries
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
            let dl = frame(
                &model,
                1200.0,
                800.0,
                &Camera {
                    zoom,
                    ..Camera::default()
                },
                None,
            );
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
            let dl = frame(
                &model,
                1000.0,
                700.0,
                &Camera {
                    zoom,
                    ..Camera::default()
                },
                None,
            );
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
        let dl = frame(&model, 900.0, 600.0, &Camera::default(), None);
        for d in &dl.districts {
            assert!(!d.purpose.ends_with(".rs") && !d.purpose.contains('/'));
        }
    }

    #[test]
    fn frame_zooming_in_strictly_increases_the_labelled_entity_count() {
        let model = build(&populous_graph(8, 3, 14));
        let out = frame(&model, 1200.0, 800.0, &Camera::default(), None);
        let in_ = frame(
            &model,
            1200.0,
            800.0,
            &Camera {
                zoom: 6.0,
                ..Camera::default()
            },
            None,
        );
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

    // ---- criterion 2: camera pan, selection lighting, hit-testing, the rail, search ----------

    /// A->B, C->A (C calls A, A calls B), all one community/district - the minimum fixture with
    /// both a CALLED BY neighbour (C) and a CALLS neighbour (B) of the middle entity A.
    fn caller_callee_graph() -> Graph {
        let nodes = vec![
            code_node("src/a.rs::a_fn", "a_fn", "function"),
            code_node("src/a.rs::b_fn", "b_fn", "function"),
            code_node("src/a.rs::c_fn", "c_fn", "function"),
        ];
        let edges = vec![
            in_community("src/a.rs::a_fn", "community/1/0"),
            in_community("src/a.rs::b_fn", "community/1/0"),
            in_community("src/a.rs::c_fn", "community/1/0"),
            live_edge("src/a.rs::a_fn", "src/a.rs::b_fn", REL_CALLS),
            live_edge("src/a.rs::c_fn", "src/a.rs::a_fn", REL_CALLS),
        ];
        Graph { nodes, edges }
    }

    #[test]
    fn frame_at_full_extent_ignores_camera_pan() {
        let model = build(&populous_graph(3, 1, 5));
        let default_cam = frame(&model, 900.0, 700.0, &Camera::default(), None);
        let panned_but_zoom_zero = frame(
            &model,
            900.0,
            700.0,
            &Camera {
                cx: 9999.0,
                cy: -9999.0,
                zoom: 0.0,
            },
            None,
        );
        let positions = |dl: &DrawList| -> Vec<(String, f64, f64)> {
            dl.entities
                .iter()
                .map(|e| (e.id.clone(), e.x, e.y))
                .collect()
        };
        assert_eq!(
            positions(&default_cam),
            positions(&panned_but_zoom_zero),
            "zoom <= 0.0 must ignore camera cx/cy entirely - it is the full-extent sentinel"
        );
    }

    #[test]
    fn frame_at_a_positive_zoom_pans_with_the_camera() {
        let model = build(&populous_graph(2, 1, 6));
        let centered = frame(
            &model,
            1000.0,
            800.0,
            &Camera {
                zoom: 2.0,
                ..Camera::default()
            },
            None,
        );
        let panned = frame(
            &model,
            1000.0,
            800.0,
            &Camera {
                cx: 500.0,
                cy: 0.0,
                zoom: 2.0,
            },
            None,
        );
        fn x_by_id(dl: &DrawList) -> BTreeMap<&str, f64> {
            dl.entities.iter().map(|e| (e.id.as_str(), e.x)).collect()
        }
        let a = x_by_id(&centered);
        let b = x_by_id(&panned);
        assert!(
            !a.is_empty() && !b.is_empty(),
            "the fixture must actually draw entities at zoom 2"
        );
        // The SAME model+viewport+zoom but a different camera cx must move at least one shared
        // entity's screen x - proving pan actually panning, never a no-op.
        let panned_entity_ids: BTreeSet<&str> = b.keys().copied().collect();
        assert!(
            panned_entity_ids.iter().any(|id| {
                a.get(id)
                    .map(|&ax| (ax - b[id]).abs() > 1e-6)
                    .unwrap_or(false)
            }),
            "panning the camera must move at least one shared entity's projected position"
        );
    }

    #[test]
    fn frame_with_no_selection_lights_nothing() {
        let model = build(&caller_callee_graph());
        let dl = frame(&model, 800.0, 600.0, &Camera::default(), None);
        assert!(dl.selected.is_none(), "{dl:?}");
        assert!(
            dl.entities.iter().all(|e| !e.lit),
            "no entity may be lit with no selection: {dl:?}"
        );
        assert!(
            dl.edges.iter().all(|e| !e.lit),
            "no edge may be lit with no selection: {dl:?}"
        );
    }

    #[test]
    fn frame_selection_lights_the_entity_and_its_callers_and_callees() {
        let model = build(&caller_callee_graph());
        let dl = frame(
            &model,
            800.0,
            600.0,
            &Camera::default(),
            Some("src/a.rs::a_fn"),
        );
        let lit_ids: BTreeSet<&str> = dl
            .entities
            .iter()
            .filter(|e| e.lit)
            .map(|e| e.id.as_str())
            .collect();
        assert_eq!(
            lit_ids,
            BTreeSet::from(["src/a.rs::a_fn", "src/a.rs::b_fn", "src/a.rs::c_fn"]),
            "the selection AND both its caller (c_fn) and callee (b_fn) must be lit: {dl:?}"
        );
    }

    #[test]
    fn frame_selection_lights_only_edges_touching_the_selection() {
        let model = build(&caller_callee_graph());
        let dl = frame(
            &model,
            800.0,
            600.0,
            &Camera::default(),
            Some("src/a.rs::a_fn"),
        );
        for e in &dl.edges {
            let touches_selection = e.from == "src/a.rs::a_fn" || e.to == "src/a.rs::a_fn";
            assert_eq!(
                e.lit, touches_selection,
                "edge {:?}->{:?} lit must equal whether it touches the selection: {dl:?}",
                e.from, e.to
            );
        }
        assert!(dl.edges.iter().any(|e| e.lit), "{dl:?}");
    }

    #[test]
    fn frame_selected_card_lists_called_by_and_calls_by_name_with_relation_type() {
        let model = build(&caller_callee_graph());
        let dl = frame(
            &model,
            800.0,
            600.0,
            &Camera::default(),
            Some("src/a.rs::a_fn"),
        );
        let card = dl
            .selected
            .expect("a_fn is a real entity, must have a card");
        assert_eq!(card.id, "src/a.rs::a_fn");
        assert_eq!(card.name, "a_fn");
        assert_eq!(
            card.called_by,
            vec![NeighborRef {
                id: "src/a.rs::c_fn".to_string(),
                name: "c_fn".to_string(),
                rel: REL_CALLS.to_string(),
            }]
        );
        assert_eq!(
            card.calls,
            vec![NeighborRef {
                id: "src/a.rs::b_fn".to_string(),
                name: "b_fn".to_string(),
                rel: REL_CALLS.to_string(),
            }]
        );
    }

    #[test]
    fn frame_selection_neighbors_are_drawn_even_beyond_the_zoom_budget() {
        // One community with 10 members (budget(0.0) == 3, so most ranks are normally
        // ineligible at full extent) - a hub with an edge to every other member, and one LOW-
        // degree leaf whose only edge is to the hub. Selecting the hub must still draw the
        // leaf even though its rank alone would never clear the zoom-0 budget.
        let mut nodes = vec![code_node("src/a.rs::hub", "hub", "function")];
        let mut edges = vec![in_community("src/a.rs::hub", "community/1/0")];
        for i in 0..9 {
            let id = format!("src/a.rs::leaf_{i}");
            nodes.push(code_node(&id, &format!("leaf_{i}"), "function"));
            edges.push(in_community(&id, "community/1/0"));
            edges.push(live_edge("src/a.rs::hub", &id, REL_CALLS));
        }
        let model = build(&Graph { nodes, edges });
        let leaf_rank = model
            .entities
            .iter()
            .find(|e| e.id == "src/a.rs::leaf_8")
            .unwrap()
            .rank;
        assert!(
            leaf_rank >= budget(0.0),
            "the fixture must actually put leaf_8 outside the zoom-0 budget for this test to \
             prove anything: rank {leaf_rank} vs budget {}",
            budget(0.0)
        );
        let without_selection = frame(&model, 800.0, 600.0, &Camera::default(), None);
        assert!(
            !without_selection
                .entities
                .iter()
                .any(|e| e.id == "src/a.rs::leaf_8"),
            "sanity: leaf_8 must NOT be drawn with no selection"
        );
        let with_selection = frame(
            &model,
            800.0,
            600.0,
            &Camera::default(),
            Some("src/a.rs::hub"),
        );
        assert!(
            with_selection
                .entities
                .iter()
                .any(|e| e.id == "src/a.rs::leaf_8"),
            "leaf_8 is a neighbour of the selected hub - it must be drawn anyway: {with_selection:?}"
        );
    }

    #[test]
    fn frame_selected_card_reports_the_honest_degree_but_caps_the_neighbor_rows() {
        // A hub with 30 callees (CARD_NEIGHBOR_CAP is 20) - the card's own `degree` must be the
        // TRUE count (30), while `calls` is capped to the top 20 by degree.
        let mut nodes = vec![code_node("src/a.rs::hub", "hub", "function")];
        let mut edges = vec![in_community("src/a.rs::hub", "community/1/0")];
        for i in 0..30 {
            let id = format!("src/a.rs::callee_{i:02}");
            nodes.push(code_node(&id, &format!("callee_{i:02}"), "function"));
            edges.push(in_community(&id, "community/1/0"));
            edges.push(live_edge("src/a.rs::hub", &id, REL_CALLS));
        }
        let model = build(&Graph { nodes, edges });
        let dl = frame(
            &model,
            1200.0,
            900.0,
            &Camera::default(),
            Some("src/a.rs::hub"),
        );
        let card = dl.selected.expect("hub must have a card");
        // 30 CALLS edges plus the hub's own IN_COMMUNITY membership edge (degree_map counts
        // every live edge touching a node, not only CALLS - the SAME whole-map degree every
        // other entity on the map carries).
        assert_eq!(
            card.degree, 31,
            "the card's degree must be the HONEST whole-map count"
        );
        assert_eq!(
            card.calls.len(),
            CARD_NEIGHBOR_CAP,
            "the card's own neighbour rows must be capped, never literally all 30"
        );
    }

    #[test]
    fn frame_an_unknown_selection_is_a_graceful_no_op() {
        let model = build(&caller_callee_graph());
        let dl = frame(
            &model,
            800.0,
            600.0,
            &Camera::default(),
            Some("src/a.rs::does_not_exist"),
        );
        assert!(dl.selected.is_none(), "{dl:?}");
        assert!(dl.entities.iter().all(|e| !e.lit), "{dl:?}");
        assert!(dl.edges.iter().all(|e| !e.lit), "{dl:?}");
    }

    // ---- hit ----------------------------------------------------------------------------------

    #[test]
    fn hit_finds_the_nearest_entity_within_radius() {
        let model = build(&caller_callee_graph());
        let dl = frame(&model, 800.0, 600.0, &Camera::default(), None);
        let target = dl.entities.first().expect("a fixture entity must be drawn");
        let got = hit(
            &model,
            800.0,
            600.0,
            &Camera::default(),
            None,
            target.x,
            target.y,
        );
        assert_eq!(got, Some(Hit::Entity(target.id.clone())), "{dl:?}");
    }

    #[test]
    fn hit_returns_none_far_from_everything() {
        let model = build(&caller_callee_graph());
        let got = hit(
            &model,
            800.0,
            600.0,
            &Camera::default(),
            None,
            -1.0e9,
            -1.0e9,
        );
        assert_eq!(got, None);
    }

    #[test]
    fn hit_falls_back_to_a_district_when_no_entity_is_near_but_the_click_is_inside_its_hull() {
        let model = build(&populous_graph(1, 1, 6));
        let dl = frame(&model, 900.0, 700.0, &Camera::default(), None);
        let d = dl.districts.first().expect("one district in this fixture");
        // Just inside the hull's radius, but nowhere near HIT_RADIUS_PX of any entity dot.
        let x = d.x + d.radius * 0.99;
        let y = d.y;
        let too_far_from_any_dot = dl
            .entities
            .iter()
            .all(|e| ((e.x - x).powi(2) + (e.y - y).powi(2)).sqrt() > HIT_RADIUS_PX);
        assert!(
            too_far_from_any_dot,
            "the probe point must not accidentally land on an entity dot for this test to prove \
             the district fallback"
        );
        let got = hit(&model, 900.0, 700.0, &Camera::default(), None, x, y);
        assert_eq!(got, Some(Hit::District(d.purpose.clone())));
    }

    #[test]
    fn hit_prefers_an_entity_dot_over_the_district_hull_beneath_it() {
        let model = build(&caller_callee_graph());
        let dl = frame(&model, 800.0, 600.0, &Camera::default(), None);
        let target = dl.entities.first().expect("a fixture entity must be drawn");
        // This entity necessarily sits inside its own district's hull too - an exact hit on the
        // dot must still resolve to the ENTITY, never the district.
        let got = hit(
            &model,
            800.0,
            600.0,
            &Camera::default(),
            None,
            target.x,
            target.y,
        );
        assert_eq!(got, Some(Hit::Entity(target.id.clone())));
    }

    // ---- explore rail: landmarks, bridges, changing, argued-about -----------------------------

    fn cross_district_graph() -> Graph {
        let nodes = vec![
            code_node("src/worktree.rs::hub", "hub", "function"),
            code_node("src/worktree.rs::leaf", "leaf", "function"),
            code_node("src/dash.rs::bridge", "bridge", "function"),
            code_node("src/dash.rs::quiet", "quiet", "function"),
        ];
        let edges = vec![
            in_community("src/worktree.rs::hub", "community/1/0"),
            in_community("src/worktree.rs::leaf", "community/1/0"),
            in_community("src/dash.rs::bridge", "community/1/1"),
            in_community("src/dash.rs::quiet", "community/1/1"),
            live_edge("src/worktree.rs::hub", "src/worktree.rs::leaf", REL_CALLS),
            // The only cross-district edge: dash::bridge calls worktree::hub.
            live_edge("src/dash.rs::bridge", "src/worktree.rs::hub", REL_CALLS),
        ];
        Graph { nodes, edges }
    }

    #[test]
    fn landmarks_ranks_by_whole_map_degree_descending() {
        let model = build(&cross_district_graph());
        let top = landmarks(&model, 1);
        // hub has degree 3 (its own IN_COMMUNITY edge plus CALLS to leaf and from bridge), the
        // highest in the fixture.
        assert_eq!(top.len(), 1);
        assert_eq!(top[0].id, "src/worktree.rs::hub");
    }

    #[test]
    fn landmarks_respects_the_limit() {
        let model = build(&cross_district_graph());
        assert_eq!(landmarks(&model, 2).len(), 2);
        assert_eq!(landmarks(&model, 100).len(), model.entities.len());
    }

    #[test]
    fn bridges_between_districts_only_includes_entities_with_a_cross_district_edge() {
        let model = build(&cross_district_graph());
        let bridges = bridges_between_districts(&model, 10);
        let ids: BTreeSet<&str> = bridges.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(
            ids,
            BTreeSet::from(["src/dash.rs::bridge", "src/worktree.rs::hub"]),
            "only the two endpoints of the one cross-district edge may appear: {bridges:?}"
        );
        assert!(
            !ids.contains("src/worktree.rs::leaf") && !ids.contains("src/dash.rs::quiet"),
            "an entity with only intra-district edges is not a bridge: {bridges:?}"
        );
    }

    #[test]
    fn changing_right_now_filters_to_the_touched_set() {
        let model = build(&cross_district_graph());
        let touched: BTreeSet<String> = ["src/worktree.rs::leaf".to_string()].into_iter().collect();
        let v = changing_right_now(&model, &touched, 10);
        assert_eq!(v.len(), 1, "{v:?}");
        assert_eq!(v[0].id, "src/worktree.rs::leaf");
    }

    #[test]
    fn changing_right_now_is_empty_when_nothing_is_touched() {
        let model = build(&cross_district_graph());
        let v = changing_right_now(&model, &BTreeSet::new(), 10);
        assert!(
            v.is_empty(),
            "no run live (an empty touched set) must answer an empty list, the EMPTY-STATE \
             Design names: {v:?}"
        );
    }

    #[test]
    fn argued_about_in_review_ranks_entities_by_pinned_finding_count() {
        use crate::contextgraph::{Edge, KIND_FINDING};
        use crate::eventstore::Position;
        let mut g = cross_district_graph();
        g.nodes.push(Node {
            id: "f1".to_string(),
            kind: KIND_FINDING.to_string(),
            attrs: Default::default(),
        });
        g.nodes.push(Node {
            id: "f2".to_string(),
            kind: KIND_FINDING.to_string(),
            attrs: Default::default(),
        });
        let about = |finding: &str, subject: &str| Edge {
            from: finding.to_string(),
            to: subject.to_string(),
            rel: REL_ABOUT.to_string(),
            valid_from: 0,
            valid_to: None,
            source: Position::default(),
            tier: TIER_EXTRACTED.to_string(),
        };
        g.edges.push(about("f1", "src/worktree.rs::hub"));
        g.edges.push(about("f2", "src/worktree.rs::hub"));
        let model = build(&g);
        let v = argued_about_in_review(&model, &g, 10);
        assert_eq!(v.len(), 1, "{v:?}");
        assert_eq!(v[0].id, "src/worktree.rs::hub");
    }

    #[test]
    fn argued_about_in_review_is_empty_when_no_findings_are_pinned() {
        let g = cross_district_graph();
        let model = build(&g);
        assert!(argued_about_in_review(&model, &g, 10).is_empty());
    }

    // ---- search ---------------------------------------------------------------------------

    #[test]
    fn search_ranks_prefix_hits_above_substring_only_hits() {
        let model = build(&cross_district_graph());
        // "bridge" is a substring of nothing else here, but "b" prefix-matches "bridge" only,
        // and substring-matches nothing else - add a name that substring-matches "hub" to prove
        // ordering: search for "leaf" prefix-matches "leaf" and nothing substring-matches it.
        let hits = search(&model, "hu", 10);
        assert_eq!(hits[0].id, "src/worktree.rs::hub", "{hits:?}");
    }

    #[test]
    fn search_is_case_insensitive() {
        let model = build(&cross_district_graph());
        let hits = search(&model, "HUB", 10);
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert_eq!(hits[0].id, "src/worktree.rs::hub");
    }

    #[test]
    fn search_of_an_empty_query_answers_no_hits() {
        let model = build(&cross_district_graph());
        assert!(search(&model, "", 10).is_empty());
    }

    #[test]
    fn search_hit_carries_kind_and_degree_beside_the_name() {
        let model = build(&cross_district_graph());
        let hits = search(&model, "hub", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, "function");
        // hub's own IN_COMMUNITY edge (+1) plus its two CALLS edges (leaf, bridge) - the SAME
        // whole-map degree_map every entity on the map carries.
        assert_eq!(hits[0].degree, 3);
    }

    // ---- fit_whole_map / fit_district -------------------------------------------------------

    #[test]
    fn fit_whole_map_is_the_full_extent_sentinel_centered_on_the_bounds() {
        let model = build(&populous_graph(3, 1, 5));
        let cam = fit_whole_map(&model);
        assert_eq!(cam.zoom, 0.0);
        let (min_x, min_y, max_x, max_y) = model.bounds;
        assert_eq!(cam.cx, (min_x + max_x) / 2.0);
        assert_eq!(cam.cy, (min_y + max_y) / 2.0);
    }

    #[test]
    fn fit_district_centers_on_the_named_district_at_a_positive_zoom() {
        let model = build(&populous_graph(3, 1, 5));
        let purpose = model.districts[0].purpose.clone();
        let cam = fit_district(&model, 1000.0, 800.0, &purpose).expect("a real district");
        assert_eq!(cam.cx, model.districts[0].x);
        assert_eq!(cam.cy, model.districts[0].y);
        assert!(
            cam.zoom > 0.0,
            "a district fit must always pan, never fall back to the full-extent sentinel"
        );
    }

    #[test]
    fn fit_district_of_an_unknown_purpose_is_none() {
        let model = build(&populous_graph(2, 1, 4));
        assert_eq!(fit_district(&model, 900.0, 700.0, "no-such-district"), None);
    }

    /// adv-u84c2-fit-district-cannot-zoom-out-past-full-extent's own repro, now a permanent
    /// regression test: a LONE district's own world hull already spans the whole map
    /// (`world_w == world_h == 2*d.radius`), so `fit_district`'s target scale is BELOW the
    /// full-extent fit scale - the camera must actually shrink to it, never clamp at the
    /// full-extent floor and silently over-fill the viewport.
    #[test]
    fn fit_district_of_a_lone_district_shrinks_to_the_district_fit_fraction() {
        let model = build(&populous_graph(1, 1, 6));
        assert_eq!(
            model.districts.len(),
            1,
            "the fixture must build exactly one district"
        );
        let purpose = model.districts[0].purpose.clone();
        let (viewport_w, viewport_h) = (1000.0, 800.0);

        let (min_x, min_y, max_x, max_y) = model.bounds;
        assert_eq!(
            max_x - min_x,
            2.0 * model.districts[0].radius,
            "the lone district's hull must equal the whole world bounds"
        );
        assert_eq!(max_y - min_y, 2.0 * model.districts[0].radius);

        let cam = fit_district(&model, viewport_w, viewport_h, &purpose).expect("a real district");
        let dl = frame(&model, viewport_w, viewport_h, &cam, None);
        let drawn = dl
            .districts
            .iter()
            .find(|d| d.purpose == purpose)
            .expect("the fit district's own pill must still be drawn");

        let target_radius_px = DISTRICT_FIT_FRACTION * viewport_w.min(viewport_h) / 2.0;
        assert!(
            (drawn.radius - target_radius_px).abs() < 1e-6,
            "a lone district's fit must shrink to the DISTRICT_FIT_FRACTION target ({target_radius_px}px), \
             not render at full-extent scale: got {}px",
            drawn.radius
        );
    }

    // ---- fit_entity ---------------------------------------------------------------------------

    #[test]
    fn fit_entity_centers_on_the_named_entitys_own_coordinates_at_its_districts_fit_zoom() {
        let model = build(&populous_graph(3, 1, 5));
        let e = model.entities[0].clone();
        let cam = fit_entity(&model, 1000.0, 800.0, &e.id).expect("a real entity");
        assert_eq!(
            cam.cx, e.x,
            "fit_entity must center on the entity's OWN x, not its district's"
        );
        assert_eq!(
            cam.cy, e.y,
            "fit_entity must center on the entity's OWN y, not its district's"
        );
        let district_zoom = fit_district(&model, 1000.0, 800.0, &e.district)
            .expect("the entity's own district")
            .zoom;
        assert_eq!(
            cam.zoom, district_zoom,
            "fit_entity must reuse fit_district's own zoom formula for the entity's district - \
             never a second one"
        );
    }

    #[test]
    fn fit_entity_of_an_unknown_id_is_none() {
        let model = build(&populous_graph(2, 1, 4));
        assert_eq!(fit_entity(&model, 900.0, 700.0, "no-such-entity"), None);
    }

    // ---- legend / kind_colour (criterion 3) ---------------------------------------------------

    #[test]
    fn kind_colour_of_each_named_kind_is_distinct_and_stable() {
        let colours: Vec<&str> = ["function", "type", "trait", "constant"]
            .iter()
            .map(|k| kind_colour(k))
            .collect();
        let unique: BTreeSet<&str> = colours.iter().copied().collect();
        assert_eq!(
            unique.len(),
            4,
            "Design's own LEGEND text names FOUR kind colours - they must be pairwise distinct: {colours:?}"
        );
        // Stable across calls (a pure lookup, never derived from input order or any mutable state).
        assert_eq!(kind_colour("function"), kind_colour("function"));
    }

    #[test]
    fn kind_colour_of_a_kind_outside_the_four_is_the_neutral_default() {
        for outside in ["method", "impl", "module", "other", "", "bogus-kind"] {
            assert_eq!(
                kind_colour(outside),
                KIND_COLOUR_DEFAULT,
                "kind {outside:?} is not one of Design's four named kinds and must share the \
                 one neutral default, never a fifth hand-picked colour"
            );
        }
    }

    #[test]
    fn legend_has_exactly_the_documented_rows_in_design_order() {
        let entries = legend();
        let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "district-pill",
                "entity-function",
                "entity-type",
                "entity-trait",
                "entity-constant",
                "edge",
                "selection",
                "blast-radius",
            ],
            "the legend must name exactly the visual classes Design's own LEGEND paragraph lists, \
             in that paragraph's own order: district pill, entity dot (per kind), typed directed \
             edge, lit selection, blast-radius ring"
        );
    }

    #[test]
    fn legend_kind_rows_carry_the_same_colour_kind_colour_answers() {
        for entry in legend() {
            if let Some(kind) = entry.id.strip_prefix("entity-") {
                assert_eq!(
                    entry.colour.as_deref(),
                    Some(kind_colour(kind)),
                    "the legend's own {kind} swatch must be the SAME colour kind_colour(...) \
                     answers - one authority, never two palettes that could drift"
                );
            }
        }
    }

    #[test]
    fn legend_blast_radius_row_is_amber_and_named() {
        let entries = legend();
        let row = entries
            .iter()
            .find(|e| e.id == "blast-radius")
            .expect("the legend must name the blast-radius ring");
        assert_eq!(row.colour.as_deref(), Some(BLAST_RADIUS_COLOUR));
        assert_eq!(row.treatment, "amber-ring");
    }

    #[test]
    fn legend_district_pill_row_is_small_caps_with_no_fixed_colour() {
        let entries = legend();
        let row = entries
            .iter()
            .find(|e| e.id == "district-pill")
            .expect("the legend must name the district pill");
        assert_eq!(row.treatment, "small-caps-pill");
        assert_eq!(
            row.colour, None,
            "a district pill's colour is not kind-specific - it carries no fixed swatch"
        );
    }

    #[test]
    fn legend_selection_row_names_underline_and_italic_relation_treatment() {
        let entries = legend();
        let row = entries
            .iter()
            .find(|e| e.id == "selection")
            .expect("the legend must name the lit selection");
        assert_eq!(row.treatment, "underline-name-italic-relation");
    }

    #[test]
    fn legend_edge_row_names_the_real_relation_types_frame_actually_draws() {
        let entries = legend();
        let row = entries
            .iter()
            .find(|e| e.id == "edge")
            .expect("the legend must name the typed directed edge");
        // The ONLY two `rel` values `build` ever keeps (see this file's own edge filter, "e.rel !=
        // REL_CALLS && e.rel != REL_REFERENCES") - read straight off the same constants, never a
        // second hand-typed list that could silently drift from what the engine actually draws.
        assert!(row.label.contains(&REL_CALLS.to_lowercase()));
        assert!(row.label.contains(&REL_REFERENCES.to_lowercase()));
        assert_eq!(row.treatment, "arrowhead");
    }

    // `LegendEntry`'s own JSON wire shape (field names, exactly) is proven externally by
    // tests/console_map_legend_wire_shape_periphery.rs, never here - this module's own tests read
    // fields directly, matching every other Draw*/RailCandidate/SearchHit type's convention (see
    // that file's own doc for why).
}
