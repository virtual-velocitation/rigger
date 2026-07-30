//! Periphery (end-to-end) proof for spec 53's RESOLUTION KNOB (criterion 2): the `--resolution`
//! grain, driven through the REAL detection pass and the REAL fold over the library's PUBLIC surface
//! (`rigger::community` + `Projector`). This criterion OWNS the GRAIN and SUPERSESSION; it does NOT
//! own detection (criterion 1) - so it proves the grain PROPERTIES emergent from
//! `detect(coupling, resolution)` + the `CommunityAssigned` fold, which no other criterion's test
//! owns:
//!
//!  1. GRAIN MONOTONICITY - a higher resolution yields AT LEAST AS MANY communities on the fixture,
//!     and a high enough resolution genuinely refines the grain (strictly more communities, up to
//!     one-per-node), so the knob is load-bearing, not decorative. (REAL detect.)
//!  2. COEXISTENCE - two resolutions detected from the SAME coupling graph fold into two DISTINCT
//!     live assignment sets (`community/<r>/*` namespaces that never collide), so the grain knob does
//!     not destroy other grains. (REAL detect.)
//!  3. SUPERSESSION that FIRES - a re-run whose NEW assignment DIFFERS from the prior (a member MOVES
//!     community, or the community set SHRINKS) leaves a STALE membership - and a now-emptied
//!     community NODE - that ONLY the fold's `fresh`-guarded retire removes. An IDENTICAL re-run is
//!     deliberately NOT the proof here: identical assignments are deduped by `add_edge`'s upsert-live
//!     alone (spec 40), so an identical-re-run test survives deletion of the retire block and proves
//!     nothing about supersession. Each firing test names its MUTATION-VERIFICATION: which production
//!     block, deleted, reddens it.
//!  4. SUPERSESSION SCOPING - a FIRING re-run of ONE grain supersedes only that grain's layer,
//!     leaving every OTHER grain's live layer (edges AND community nodes) byte-identical - including
//!     an ADJACENT numeric grain (`community/1/` vs `community/10/`), proving the `fresh` retire's
//!     trailing-slash prefix boundary on both the edge retire and the orphan-node drop.
//!  5. EMPTY RE-RUN = KEEP-LAST-GOOD (the charter routed here by adj-u53c1-empty-rerun-carryforward):
//!     an empty coupling yields an empty `Assignment` -> `events()` emits ZERO events -> no `fresh`
//!     boundary fires -> the grain's last non-empty assignment stays LIVE. This locks the deliberate
//!     policy (decision `d-u53c2-empty-rerun-keep-last-good`): clearing on empty would require a new
//!     event type (spec 53 standing-b forbids it) or a projector-level clear outside the fold (breaks
//!     event-sourcing), and an empty coupling is the degenerate whole-layer-empty read; a real SHRINK
//!     is a smaller NON-empty assignment that DOES supersede (see property 3).
//!  6. CROSS-PROJECT SCOPING - the `fresh` supersession (the edge retire AND the orphan-node drop) is
//!     `project`-scoped, and community ids (`community/<r>/<n>`) are NOT project-qualified, so on a
//!     shared backend two projects hold the SAME community id as DISTINCT `(id, project)` rows. A
//!     FIRING re-run in ONE project must leave the OTHER project's identically-named grain
//!     byte-identical - the axis the single-project siblings are structurally blind to. (REAL fold
//!     over two projects on one backend.)
//!
//! Sibling coverage this deliberately does NOT duplicate: criterion 1's determinism/connectedness
//! (`community_detection_pass.rs`), criterion 3's fold contract with HAND-AUTHORED events
//! (`community_fold_periphery.rs`), and the SDET binary boundary (`community_detection_cli.rs`, which
//! drives the shipped `rigger graph communities` subcommand). This file drives the LIBRARY seam
//! `detect -> events -> fold` directly, the layer at which the grain semantics actually emerge; the
//! FIRING properties author their DIFFERING re-runs as explicit `Assignment`s (the grain criterion
//! owns supersession, a FOLD property, so a controlled pair of assignments proves the retire
//! deterministically - no dependency on coaxing the real pass into a particular move).
//!
//! Detection and the fold are ALWAYS compiled, so nothing here is feature-gated and it runs in BOTH
//! feature lanes.

use std::collections::{BTreeMap, BTreeSet};

use rigger::community::{self, Assignment, Coupling};
use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{
    Graph, Projection, KIND_COMMUNITY, REL_IN_COMMUNITY, TYPE_CODE_ENTITY_EXTRACTED,
    TYPE_EDGE_INFERRED,
};
use rigger::eventstore::Event;

/// Apply one event built from its raw on-log JSON at `pos` - the serialized form a rebuild replays.
fn apply_json(p: &Projector, pos: u64, type_: &str, json: serde_json::Value) {
    let mut e = Event::new(type_, serde_json::to_vec(&json).unwrap());
    e.position = pos;
    p.apply(&e).unwrap();
}

/// Fold one definition (spec 29a): the file node, the `<file>::<name>` entity (carrying the `name`
/// attr that marks a real definition), and their `CONTAINS` edge.
fn def(p: &Projector, pos: u64, file: &str, name: &str) {
    apply_json(
        p,
        pos,
        TYPE_CODE_ENTITY_EXTRACTED,
        serde_json::json!({ "file": file, "name": name, "kind": "function", "line": pos, "lang": "rust" }),
    );
}

/// Fold one caller-attributed reference (spec 37): a `<file>::<caller> --CALLS--> <name>` edge plus
/// the file-level `REFERENCES` edge; a cross-file `name` lands on a BARE placeholder the pass
/// resolves by unique name-suffix.
fn call(p: &Projector, pos: u64, file: &str, name: &str, caller: &str) {
    apply_json(
        p,
        pos,
        TYPE_EDGE_INFERRED,
        serde_json::json!({ "file": file, "name": name, "caller": caller, "lang": "rust" }),
    );
}

/// Seed the canonical spec-53 TWO-SUBSYSTEM coupling graph onto `p`, each subsystem spanning two
/// directories, joined by one weak bridge. Returns the next free position. Subsystem A spans
/// `src/combat` and `src/net`; subsystem B spans `src/render` and `src/ui`. This is the same shape
/// the criterion-1 periphery seeds; here it drives the GRAIN. Its coupling graph has 12 nodes (eight
/// entities + four files), so at a high enough resolution every node isolates into its own community.
fn seed(p: &Projector) -> u64 {
    def(p, 1, "src/combat/hit.rs", "strike");
    def(p, 2, "src/combat/hit.rs", "block");
    def(p, 3, "src/net/link.rs", "send");
    def(p, 4, "src/net/link.rs", "recv");
    def(p, 5, "src/render/draw.rs", "paint");
    def(p, 6, "src/render/draw.rs", "shade");
    def(p, 7, "src/ui/hud.rs", "layout");
    def(p, 8, "src/ui/hud.rs", "show");

    // Subsystem A: dense cross-file coupling between combat and net.
    call(p, 9, "src/combat/hit.rs", "send", "strike");
    call(p, 10, "src/combat/hit.rs", "recv", "strike");
    call(p, 11, "src/combat/hit.rs", "send", "block");
    call(p, 12, "src/net/link.rs", "strike", "send");
    call(p, 13, "src/net/link.rs", "block", "recv");

    // Subsystem B: dense cross-file coupling between render and ui.
    call(p, 14, "src/render/draw.rs", "layout", "paint");
    call(p, 15, "src/render/draw.rs", "show", "paint");
    call(p, 16, "src/render/draw.rs", "layout", "shade");
    call(p, 17, "src/ui/hud.rs", "paint", "layout");
    call(p, 18, "src/ui/hud.rs", "shade", "show");

    // A single weak bridge from A to B: too thin to merge the two subsystems.
    call(p, 19, "src/combat/hit.rs", "paint", "strike");
    20
}

/// The coupling graph of the seeded projection (over the public `whole()` read).
fn coupling(p: &Projector) -> Coupling {
    Coupling::from_graph(&p.whole().unwrap())
}

/// Detect at `resolution` over `p`'s coupling graph, record the `CommunityAssigned` events at
/// positions starting at `next_pos`, and fold them into `p`. Returns the next free position. This is
/// the real `read -> detect -> record -> fold` seam a pass runs, minus the store append.
fn record_grain(p: &Projector, resolution: f64, next_pos: u64) -> u64 {
    let assignment = community::detect(&coupling(p), resolution);
    fold_assignment(p, &assignment, next_pos)
}

/// Record a HAND-AUTHORED assignment: build the `Assignment` for `members` (each `(node,
/// community)`, the community an explicit `community/<res>/<n>` id) at `resolution`, then fold its
/// `CommunityAssigned` events at `next_pos`. This drives the fold's SUPERSESSION with a re-run whose
/// assignment DIFFERS from the prior by construction - a member moved, or a community emptied -
/// which the real pass only reaches on a coupling change; the grain criterion owns supersession (a
/// fold property), so authoring the differing pair is the deterministic way to prove the retire.
fn record_assignment(
    p: &Projector,
    resolution: f64,
    hash: &str,
    members: &[(&str, &str)],
    next_pos: u64,
) -> u64 {
    let assignment = Assignment {
        resolution,
        hash: hash.to_string(),
        members: members
            .iter()
            .map(|(n, c)| (n.to_string(), c.to_string()))
            .collect(),
        num_communities: members
            .iter()
            .map(|(_, c)| *c)
            .collect::<BTreeSet<_>>()
            .len(),
    };
    fold_assignment(p, &assignment, next_pos)
}

/// Fold an `Assignment`'s `CommunityAssigned` events (the real `community::events` encoding, so the
/// `fresh` boundary rides its FIRST event exactly as a real pass records it) into `p` starting at
/// `next_pos`, one event at a time (the rebuild-replay order). Returns the next free position.
fn fold_assignment(p: &Projector, assignment: &Assignment, next_pos: u64) -> u64 {
    let mut events = community::events(assignment);
    for (i, e) in events.iter_mut().enumerate() {
        e.position = next_pos + i as u64;
        p.apply(e).unwrap();
    }
    next_pos + events.len() as u64
}

/// Every LIVE `<member> --IN_COMMUNITY--> <community>` edge as a sorted `(member, community)` list.
fn live_memberships(g: &Graph) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = g
        .edges
        .iter()
        .filter(|e| e.rel == REL_IN_COMMUNITY && e.valid_to.is_none())
        .map(|e| (e.from.clone(), e.to.clone()))
        .collect();
    v.sort();
    v
}

/// The sorted set of live `KIND_COMMUNITY` node ids.
fn community_nodes(g: &Graph) -> Vec<String> {
    let mut v: Vec<String> = g
        .nodes
        .iter()
        .filter(|n| n.kind == KIND_COMMUNITY)
        .map(|n| n.id.clone())
        .collect();
    v.sort();
    v
}

/// The sorted live `KIND_COMMUNITY` node ids under the `community/<grain>/` prefix ONLY (the
/// trailing slash makes `1` NOT match `10`).
fn grain_community_nodes(g: &Graph, grain: &str) -> Vec<String> {
    let prefix = format!("community/{grain}/");
    community_nodes(g)
        .into_iter()
        .filter(|c| c.starts_with(&prefix))
        .collect()
}

/// The live targets of `member`'s `IN_COMMUNITY` edges, sorted - the member's live memberships.
fn live_memberships_of(g: &Graph, member: &str) -> Vec<String> {
    let mut v: Vec<String> = g
        .edges
        .iter()
        .filter(|e| e.rel == REL_IN_COMMUNITY && e.from == member && e.valid_to.is_none())
        .map(|e| e.to.clone())
        .collect();
    v.sort();
    v
}

/// A deterministic snapshot of ONE grain's whole live layer: every `KIND_COMMUNITY` node under the
/// `community/<grain>/` prefix (id + ordered attrs) and every live `IN_COMMUNITY` edge into it,
/// sorted. Two reads of an unchanged grain produce byte-identical snapshots.
fn grain_snapshot(g: &Graph, grain: &str) -> Vec<String> {
    let prefix = format!("community/{grain}/");
    let mut rows: Vec<String> = Vec::new();
    for n in &g.nodes {
        if n.kind == KIND_COMMUNITY && n.id.starts_with(&prefix) {
            let attrs: BTreeMap<&String, &String> = n.attrs.iter().collect();
            rows.push(format!("node {}|{attrs:?}", n.id));
        }
    }
    for e in &g.edges {
        if e.rel == REL_IN_COMMUNITY && e.to.starts_with(&prefix) && e.valid_to.is_none() {
            rows.push(format!("edge {} -> {}", e.from, e.to));
        }
    }
    rows.sort();
    rows
}

#[test]
fn a_higher_resolution_yields_at_least_as_many_communities() {
    // GRAIN MONOTONICITY (criterion 2's first clause), proven over the REAL detection pass on the
    // fixture's coupling graph: sweeping the resolution up never yields FEWER communities, and a high
    // enough resolution genuinely refines the grain into strictly more (down to one-per-node). If the
    // resolution argument were ignored, the count would be flat and the strict-increase and
    // full-refinement assertions below would redden - so this is non-vacuous by construction.
    let p = Projector::open(":memory:", "test").unwrap();
    seed(&p);
    let c = coupling(&p);
    let n = c.len();
    assert!(
        n >= 8,
        "the fixture folds a non-trivial coupling graph, got {n}"
    );

    // A fine ascending sweep. `detect` is a pure function of (coupling, resolution).
    let sweep = [0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 4.0, 6.0];
    let counts: Vec<usize> = sweep
        .iter()
        .map(|&r| community::detect(&c, r).num_communities)
        .collect();

    // "at least as many": the count is MONOTONE NON-DECREASING as the resolution rises.
    for w in counts.windows(2) {
        assert!(
            w[1] >= w[0],
            "a higher resolution never yields fewer communities; sweep {sweep:?} -> {counts:?}"
        );
    }

    // The knob is load-bearing: the coarsest grain groups the graph into a FEW communities, and a
    // finer grain splits it into STRICTLY more.
    let coarse = *counts.first().unwrap();
    let fine = *counts.last().unwrap();
    assert!(
        fine > coarse,
        "a higher resolution genuinely refines the grain into more communities; \
         coarse={coarse}, fine={fine} (sweep {sweep:?} -> {counts:?})"
    );

    // At the extreme, the grain fully refines: a high enough resolution isolates every node into its
    // own community (the maximum possible grain), a resolution-robust invariant.
    let maxed = community::detect(&c, 50.0);
    assert_eq!(
        maxed.num_communities, n,
        "a high enough resolution isolates every one of the {n} coupled nodes into its own community"
    );

    // Distinct resolutions are DISTINCT assignment sets, not one relabeled: the coarse and fine grains
    // partition the SAME nodes differently AND carry their resolution in the community id.
    let coarse_a = community::detect(&c, *sweep.first().unwrap());
    let fine_a = community::detect(&c, 50.0);
    let partition = |a: &community::Assignment| -> BTreeSet<BTreeSet<String>> {
        let mut by_comm: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (node, comm) in &a.members {
            by_comm
                .entry(comm.clone())
                .or_default()
                .insert(node.clone());
        }
        by_comm.into_values().collect()
    };
    assert_ne!(
        partition(&coarse_a),
        partition(&fine_a),
        "distinct resolutions produce distinct groupings of the same nodes"
    );
    assert!(
        coarse_a
            .members
            .iter()
            .all(|(_, c)| c.starts_with(&format!("community/{}/", sweep[0]))),
        "the coarse grain's community ids carry its resolution: {:?}",
        coarse_a.members
    );
    assert!(
        fine_a
            .members
            .iter()
            .all(|(_, c)| c.starts_with("community/50/")),
        "the fine grain's community ids carry its resolution: {:?}",
        fine_a.members
    );
}

#[test]
fn distinct_resolutions_coexist_as_distinct_live_assignment_sets() {
    // COEXISTENCE (criterion 2's second clause): two grains detected from the SAME coupling graph and
    // folded into the SAME projection live SIDE BY SIDE - distinct `community/<r>/*` namespaces that
    // never collide, each member carrying exactly one live membership PER grain. The grain knob does
    // not destroy other grains.
    let p = Projector::open(":memory:", "test").unwrap();
    let next = seed(&p);
    let next = record_grain(&p, 1.0, next); // the default coarse grain
    record_grain(&p, 4.0, next); // a finer grain that fully refines this fixture

    let g = p.whole().unwrap();
    let comms = community_nodes(&g);
    let grain1 = grain_community_nodes(&g, "1");
    let grain4 = grain_community_nodes(&g, "4");
    assert!(
        !grain1.is_empty() && !grain4.is_empty(),
        "both grains materialized live community nodes; got {comms:?}"
    );
    // The two grains are genuinely DIFFERENT assignment sets on this fixture (the fine grain refines
    // into more communities), so coexistence is not a trivial relabeling.
    assert!(
        grain4.len() > grain1.len(),
        "the finer grain holds more communities than the coarse one; {} vs {}",
        grain4.len(),
        grain1.len()
    );

    // Every entity member carries EXACTLY ONE live membership in EACH grain, and those targets sit in
    // the matching namespace - the two grains coexist without clobbering one another.
    let memberships = live_memberships(&g);
    let members: BTreeSet<&String> = memberships.iter().map(|(m, _)| m).collect();
    for m in &members {
        assert_eq!(
            live_memberships_of(&g, m)
                .iter()
                .filter(|c| c.starts_with("community/1/"))
                .count(),
            1,
            "member {m} has one live default-grain membership"
        );
        assert_eq!(
            live_memberships_of(&g, m)
                .iter()
                .filter(|c| c.starts_with("community/4/"))
                .count(),
            1,
            "member {m} has one live fine-grain membership"
        );
    }

    // The two grains' community-id sets are DISJOINT: no community node is shared across grains.
    let set1: BTreeSet<&String> = grain1.iter().collect();
    let set4: BTreeSet<&String> = grain4.iter().collect();
    assert!(
        set1.is_disjoint(&set4),
        "the two grains occupy disjoint community namespaces; {set1:?} vs {set4:?}"
    );
}

#[test]
fn a_rerun_whose_member_moved_retires_the_stale_membership() {
    // SUPERSESSION that FIRES on a MOVED member (criterion 2's supersession clause, made
    // NON-VACUOUS). Two hand-authored r=2 passes over the SAME grain whose assignments DIFFER: `b`
    // moves from `community/2/0` to `community/2/1`. After the re-run, `b`'s stale membership in
    // `community/2/0` must be GONE - a fact ONLY the `fresh`-guarded edge retire delivers, because
    // the new pass's events never touch `community/2/0` for `b`, so `add_edge`'s upsert-live dedup
    // (spec 40) cannot remove the old `b -> community/2/0` edge (it is a DIFFERENT edge from the new
    // `b -> community/2/1`).
    //
    // MUTATION-VERIFICATION: deleting the `if c.fresh { UPDATE edges SET valid_to ... }` retire block
    // (sqlite.rs, the community fold arm) leaves `b -> community/2/0` LIVE, so `b` carries TWO live
    // memberships and the single-membership assertion reddens. (An IDENTICAL re-run would NOT redden
    // under that deletion - that is the vacuity this test exists to avoid.)
    let p = Projector::open(":memory:", "test").unwrap();
    // Pass 1: community/2/0 = {a, b}, community/2/1 = {c}.
    let next = record_assignment(
        &p,
        2.0,
        "pass-1",
        &[
            ("n_a", "community/2/0"),
            ("n_b", "community/2/0"),
            ("n_c", "community/2/1"),
        ],
        1,
    );
    assert_eq!(
        live_memberships_of(&p.whole().unwrap(), "n_b"),
        vec!["community/2/0".to_string()],
        "precondition: b starts in community/2/0"
    );

    // Pass 2 (DIFFERS): b MOVES community/2/0 -> community/2/1, so community/2/0 = {a}, /1 = {b, c}.
    record_assignment(
        &p,
        2.0,
        "pass-2",
        &[
            ("n_a", "community/2/0"),
            ("n_b", "community/2/1"),
            ("n_c", "community/2/1"),
        ],
        next,
    );
    let g = p.whole().unwrap();

    // b now carries EXACTLY ONE live membership, and it is the NEW community - the stale
    // b -> community/2/0 edge was retired, not left as a duplicate.
    assert_eq!(
        live_memberships_of(&g, "n_b"),
        vec!["community/2/1".to_string()],
        "b moved to community/2/1 with exactly one live membership (the stale community/2/0 \
         membership is retired); got {:?}",
        live_memberships_of(&g, "n_b")
    );
    // The members that did NOT move keep exactly their new (== reasserted) membership.
    assert_eq!(
        live_memberships_of(&g, "n_a"),
        vec!["community/2/0".to_string()],
        "a stays in community/2/0"
    );
    assert_eq!(
        live_memberships_of(&g, "n_c"),
        vec!["community/2/1".to_string()],
        "c stays in community/2/1"
    );
}

#[test]
fn a_shrinking_rerun_retires_the_emptied_community_node() {
    // SUPERSESSION that FIRES on a SHRINK, proving NODE completeness: a re-run whose assignment holds
    // FEWER communities must leave NO ghost community super-node behind. The fold retires the grain's
    // membership EDGES on `fresh`, but a KIND_COMMUNITY node carries no `valid_to`; without the
    // node-side drop, an emptied community LINGERS in `whole()` (the read the `/api/graph` provider
    // consults) as a live node with ZERO live members and a STALE label - a ghost bucket. This proves
    // the emptied node is retired, closing adv-u53c2-orphan-community-node-on-shrink-rerun and
    // adv-u53c3-phantom-community-stale-label.
    //
    // MUTATION-VERIFICATION: deleting the community-node DELETE the fold runs right after the edge
    // retire (sqlite.rs, the `if c.fresh` block) leaves community/3/1 and community/3/2 as live ghost
    // nodes, reddening the community-node-set assertion.
    let p = Projector::open(":memory:", "test").unwrap();
    // Pass 1: three singleton communities, community/3/0..2.
    let next = record_assignment(
        &p,
        3.0,
        "pass-1",
        &[
            ("n_a", "community/3/0"),
            ("n_b", "community/3/1"),
            ("n_c", "community/3/2"),
        ],
        1,
    );
    assert_eq!(
        grain_community_nodes(&p.whole().unwrap(), "3"),
        vec![
            "community/3/0".to_string(),
            "community/3/1".to_string(),
            "community/3/2".to_string(),
        ],
        "precondition: three live community nodes before the shrink"
    );

    // Pass 2 (SHRINKS): all three members collapse into community/3/0; /1 and /2 are EMPTIED.
    record_assignment(
        &p,
        3.0,
        "pass-2",
        &[
            ("n_a", "community/3/0"),
            ("n_b", "community/3/0"),
            ("n_c", "community/3/0"),
        ],
        next,
    );
    let g = p.whole().unwrap();

    // ONLY community/3/0 survives: the emptied nodes are retired, not left as ghosts.
    assert_eq!(
        grain_community_nodes(&g, "3"),
        vec!["community/3/0".to_string()],
        "the shrink retired the emptied community/3/1 and community/3/2 nodes; got {:?}",
        grain_community_nodes(&g, "3")
    );
    // Belt and braces: no live membership into an emptied community, and the survivor holds all three.
    for emptied in ["community/3/1", "community/3/2"] {
        assert!(
            !g.edges
                .iter()
                .any(|e| e.rel == REL_IN_COMMUNITY && e.to == emptied && e.valid_to.is_none()),
            "no live membership survives into the emptied {emptied}"
        );
        assert!(
            !g.nodes.iter().any(|n| n.id == emptied),
            "the emptied community node {emptied} is retired, not a ghost; live nodes: {:?}",
            community_nodes(&g)
        );
    }
    let mut survivors: Vec<String> = g
        .edges
        .iter()
        .filter(|e| e.rel == REL_IN_COMMUNITY && e.to == "community/3/0" && e.valid_to.is_none())
        .map(|e| e.from.clone())
        .collect();
    survivors.sort();
    assert_eq!(
        survivors,
        vec!["n_a".to_string(), "n_b".to_string(), "n_c".to_string()],
        "the survivor community/3/0 holds all three members live"
    );
}

#[test]
fn a_firing_rerun_supersedes_only_its_own_resolution_grain() {
    // SUPERSESSION SCOPING under a FIRING re-run (criterion 2's per-resolution clause): after two
    // grains coexist, RE-RUNNING one grain with a DIFFERING assignment (a member moves AND a
    // community empties) supersedes ONLY that grain - the OTHER grain's whole live layer (its nodes
    // AND edges) stays byte-identical. A GLOBAL supersession would wipe the other grain here and
    // redden the byte-identical assertion; an edge-only retire that missed the node drop would leave
    // the re-run grain's emptied node behind. Uses ADJACENT numeric grains (`community/1/` vs
    // `community/10/`) so the assertion also proves the retire's trailing-slash prefix boundary: an
    // r=1 re-run must not bleed into the `community/10/` grain (closing the
    // sdet-u53c3-supersede-slash-boundary coverage gap for BOTH the edge retire and the node drop).
    let p = Projector::open(":memory:", "test").unwrap();
    // Grain r=1: community/1/0 = {a, b}, community/1/1 = {c}.
    let next = record_assignment(
        &p,
        1.0,
        "r1-pass-1",
        &[
            ("n_a", "community/1/0"),
            ("n_b", "community/1/0"),
            ("n_c", "community/1/1"),
        ],
        1,
    );
    // Grain r=10 (an ADJACENT numeric prefix): community/10/0 = {x}, community/10/1 = {y}.
    let next = record_assignment(
        &p,
        10.0,
        "r10-pass",
        &[("n_x", "community/10/0"), ("n_y", "community/10/1")],
        next,
    );

    // Snapshot the OTHER grain (r=10) before the r=1 re-run: its whole live layer, nodes + edges.
    let before10 = grain_snapshot(&p.whole().unwrap(), "10");
    assert!(
        !before10.is_empty(),
        "the r=10 grain has a non-empty live layer before the re-run (else the guard is vacuous)"
    );

    // Re-run r=1 with a DIFFERING assignment: c moves community/1/1 -> community/1/0 (so /1/1
    // empties) - a FIRING re-run that supersedes edges AND drops the emptied node.
    record_assignment(
        &p,
        1.0,
        "r1-pass-2",
        &[
            ("n_a", "community/1/0"),
            ("n_b", "community/1/0"),
            ("n_c", "community/1/0"),
        ],
        next,
    );
    let g = p.whole().unwrap();

    // The r=10 grain is UNTOUCHED - byte-identical across the r=1 re-run (per-resolution scoping AND
    // the trailing-slash prefix boundary: community/1/ never matched community/10/).
    assert_eq!(
        before10,
        grain_snapshot(&g, "10"),
        "re-running the r=1 grain leaves the ADJACENT r=10 grain's live layer byte-identical \
         (per-resolution supersession, not global; the community/1/ prefix has a trailing-slash \
         boundary that excludes community/10/)"
    );

    // The r=1 re-run FIRED: community/1/1 emptied and was retired, community/1/0 holds all three.
    assert_eq!(
        grain_community_nodes(&g, "1"),
        vec!["community/1/0".to_string()],
        "the FIRING r=1 re-run retired the emptied community/1/1 node; got {:?}",
        grain_community_nodes(&g, "1")
    );
    assert_eq!(
        live_memberships_of(&g, "n_c"),
        vec!["community/1/0".to_string()],
        "c moved into community/1/0 with exactly one live membership; got {:?}",
        live_memberships_of(&g, "n_c")
    );
}

#[test]
fn an_empty_rerun_keeps_the_last_good_assignment() {
    // EMPTY RE-RUN = KEEP-LAST-GOOD (decision d-u53c2-empty-rerun-keep-last-good; the charter routed
    // here by adj-u53c1-empty-rerun-carryforward-c2). Detecting over an EMPTY coupling yields an
    // empty `Assignment`, whose `events()` is EMPTY, so the fold sees no `fresh` boundary and the
    // grain's last non-empty assignment stays LIVE - the deliberate policy: an empty coupling is the
    // degenerate whole-layer-empty read, and clearing on it would need a new event type (spec 53
    // standing-b forbids one) or a projector-level clear outside the fold (breaks event-sourcing). A
    // real SHRINK (a smaller NON-empty assignment) DOES supersede - proven by
    // `a_shrinking_rerun_retires_the_emptied_community_node`; THIS test pins the empty case.
    let p = Projector::open(":memory:", "test").unwrap();
    let next = seed(&p);
    record_grain(&p, 1.0, next); // a real, non-empty r=1 assignment.
    let before = grain_snapshot(&p.whole().unwrap(), "1");
    assert!(
        !before.is_empty(),
        "the r=1 grain is non-empty before the empty re-run (else keep-last-good is vacuous)"
    );

    // The keep-last-good MECHANISM: an empty coupling -> an empty assignment -> ZERO events.
    let empty_coupling = Coupling::from_graph(&Graph {
        nodes: Vec::new(),
        edges: Vec::new(),
    });
    let empty_assignment = community::detect(&empty_coupling, 1.0);
    assert!(
        empty_assignment.members.is_empty(),
        "an empty coupling yields an empty assignment (the keep-last-good precondition)"
    );
    let empty_events = community::events(&empty_assignment);
    assert!(
        empty_events.is_empty(),
        "an empty assignment yields ZERO CommunityAssigned events, so no `fresh` boundary fires \
         (the keep-last-good mechanism); got {} event(s)",
        empty_events.len()
    );

    // Fold the empty re-run (a no-op: zero events) exactly as the real pass would.
    fold_assignment(&p, &empty_assignment, next + 100);

    // KEEP-LAST-GOOD: the r=1 grain's last-good live layer is byte-identical - the empty re-run did
    // NOT clear it.
    assert_eq!(
        before,
        grain_snapshot(&p.whole().unwrap(), "1"),
        "an empty re-run leaves the r=1 grain's last-good live layer byte-identical (keep-last-good, \
         not clear-on-empty)"
    );
}

#[test]
fn a_firing_rerun_supersedes_only_its_own_project() {
    // CROSS-PROJECT SCOPING of the `fresh` supersession (criterion 2's per-resolution supersession
    // extended to the OTHER axis the single-project siblings are structurally blind to). Community ids
    // are `community/<res>/<n>` - NOT project-qualified - and the `nodes` primary key is
    // `(id, project)`, so two projects on ONE shared backend BOTH hold a node `community/1/0` as
    // DISTINCT rows. The fold's `fresh` arm is the ONLY thing keeping a re-run in project A from
    // clobbering project B's identically-named grain: the edge retire is `... AND project = ?` and the
    // orphan-node DELETE is `DELETE ... AND project = ?`. Every OTHER community test (this file's
    // siblings, community_fold_periphery.rs, community_detection_cli.rs) runs a SINGLE project, so this
    // guard is otherwise unexercised. This drives TWO projects through the REAL fold over one backend.
    //
    // MUTATION-VERIFICATION: dropping `AND project = ?3` from the node DELETE (sqlite.rs community fold
    // arm) lets project A's shrink also delete project B's `community/1/0` and `community/1/1` nodes
    // (neither has a live member in project A) - reddening the "project B byte-identical" node rows;
    // dropping `AND project = ?4` from the edge retire retires project B's live memberships too -
    // reddening its edge rows. Neither mutation reddens any single-project sibling.
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("graph.db");
    let db = db_path.to_str().unwrap();

    // Two projects share ONE backend file; the `(id, project)` primary key keeps their
    // identically-named community nodes as DISTINCT rows, and `whole()` reads each in isolation.
    let a = Projector::open(db, "proj-a").unwrap();
    let b = Projector::open(db, "proj-b").unwrap();

    // Project B: a stable r=1 grain - community/1/0 = {b1, b2}, community/1/1 = {b3}. Its positions are
    // GLOBALLY disjoint from project A's because the `applied` ledger (position PRIMARY KEY) is shared
    // across the backend - a reused position would be IGNORED and fold nothing.
    record_assignment(
        &b,
        1.0,
        "b-pass",
        &[
            ("n_b1", "community/1/0"),
            ("n_b2", "community/1/0"),
            ("n_b3", "community/1/1"),
        ],
        1,
    );
    let before_b = grain_snapshot(&b.whole().unwrap(), "1");
    assert!(
        !before_b.is_empty(),
        "project B has a live r=1 layer before project A's re-run (else the guard is vacuous)"
    );

    // Project A: an r=1 grain with the SAME community ids, then a FIRING shrink re-run that EMPTIES
    // community/1/1 (its sole member moves into community/1/0) - firing project A's edge retire AND
    // orphan-node DELETE.
    let next = record_assignment(
        &a,
        1.0,
        "a-pass-1",
        &[("n_a1", "community/1/0"), ("n_a2", "community/1/1")],
        100,
    );
    record_assignment(
        &a,
        1.0,
        "a-pass-2",
        &[("n_a1", "community/1/0"), ("n_a2", "community/1/0")],
        next,
    );

    // Project A's re-run FIRED over its OWN grain: its emptied community/1/1 node is gone.
    assert_eq!(
        grain_community_nodes(&a.whole().unwrap(), "1"),
        vec!["community/1/0".to_string()],
        "project A's firing shrink retired its OWN community/1/1 node; got {:?}",
        grain_community_nodes(&a.whole().unwrap(), "1")
    );

    // Project B is UNTOUCHED - its whole live r=1 layer (the identically-named community/1/0 AND
    // community/1/1 nodes, and every membership edge) is byte-identical across project A's firing
    // re-run. A guard-less DELETE would have dropped project B's community nodes; a guard-less edge
    // retire would have retired project B's memberships.
    assert_eq!(
        before_b,
        grain_snapshot(&b.whole().unwrap(), "1"),
        "a firing re-run in project A leaves project B's identically-named r=1 grain byte-identical \
         (the fold's edge retire AND orphan-node DELETE are project-scoped)"
    );
}
