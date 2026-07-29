//! Periphery (end-to-end) proof for spec 53's RESOLUTION KNOB (criterion 2): the `--resolution`
//! grain, driven through the REAL detection pass and the REAL fold over the library's PUBLIC surface
//! (`rigger::community` + `Projector`). This criterion OWNS the GRAIN and SUPERSESSION; it does NOT
//! own detection (criterion 1) - so it proves the three grain PROPERTIES emergent from
//! `detect(coupling, resolution)` + the `CommunityAssigned` fold, which no other criterion's test
//! owns:
//!
//!  1. GRAIN MONOTONICITY - a higher resolution yields AT LEAST AS MANY communities on the fixture,
//!     and a high enough resolution genuinely refines the grain (strictly more communities, up to
//!     one-per-node), so the knob is load-bearing, not decorative.
//!  2. COEXISTENCE - two resolutions detected from the SAME coupling graph fold into two DISTINCT
//!     live assignment sets (`community/<r>/*` namespaces that never collide), so the grain knob does
//!     not destroy other grains.
//!  3. SUPERSESSION SCOPING - re-running ONE resolution's pass supersedes ONLY that grain's prior
//!     memberships (the `fresh` pass boundary), leaving every OTHER grain's live assignment set
//!     byte-identical, and leaving the re-run grain with exactly one live membership per member (no
//!     stale duplicate).
//!
//! Sibling coverage this deliberately does NOT duplicate: criterion 1's determinism/connectedness
//! (`community_detection_pass.rs`), criterion 3's fold contract with HAND-AUTHORED events
//! (`community_fold_periphery.rs`), and the SDET binary boundary (`community_detection_cli.rs`, which
//! drives the shipped `rigger graph communities` subcommand). This file drives the LIBRARY seam
//! `detect -> events -> fold` directly, the layer at which the grain semantics actually emerge.
//!
//! Detection and the fold are ALWAYS compiled, so nothing here is feature-gated and it runs in BOTH
//! feature lanes.

use std::collections::{BTreeMap, BTreeSet};

use rigger::community::{self, Coupling};
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
    let mut events = community::events(&assignment);
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
        .filter(|e| e.rel == REL_IN_COMMUNITY)
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
        if e.rel == REL_IN_COMMUNITY && e.to.starts_with(&prefix) {
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
    let grain1: Vec<&String> = comms
        .iter()
        .filter(|c| c.starts_with("community/1/"))
        .collect();
    let grain4: Vec<&String> = comms
        .iter()
        .filter(|c| c.starts_with("community/4/"))
        .collect();
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
        let g1: Vec<&String> = memberships
            .iter()
            .filter(|(from, to)| from == *m && to.starts_with("community/1/"))
            .map(|(_, to)| to)
            .collect();
        let g4: Vec<&String> = memberships
            .iter()
            .filter(|(from, to)| from == *m && to.starts_with("community/4/"))
            .map(|(_, to)| to)
            .collect();
        assert_eq!(
            g1.len(),
            1,
            "member {m} has one live default-grain membership; got {g1:?}"
        );
        assert_eq!(
            g4.len(),
            1,
            "member {m} has one live fine-grain membership; got {g4:?}"
        );
    }

    // The two grains' community-id sets are DISJOINT: no community node is shared across grains.
    let set1: BTreeSet<&String> = grain1.iter().copied().collect();
    let set4: BTreeSet<&String> = grain4.iter().copied().collect();
    assert!(
        set1.is_disjoint(&set4),
        "the two grains occupy disjoint community namespaces; {set1:?} vs {set4:?}"
    );
}

#[test]
fn a_rerun_supersedes_only_its_own_resolution_grain() {
    // SUPERSESSION SCOPING (criterion 2's third clause): after two grains coexist, RE-RUNNING one
    // grain's pass supersedes ONLY that grain's prior memberships (its `fresh` pass boundary),
    // leaving the OTHER grain's live layer byte-identical and the re-run grain duplicate-free. This is
    // the sharp per-resolution claim: a GLOBAL supersession (retiring every grain on any re-run) would
    // wipe the other grain here and redden the byte-identical assertion.
    let p = Projector::open(":memory:", "test").unwrap();
    let next = seed(&p);
    let next = record_grain(&p, 1.0, next);
    let next = record_grain(&p, 4.0, next);

    // Snapshot the OTHER grain (r=4) before the re-run: its whole live layer, nodes + edges.
    let before = grain_snapshot(&p.whole().unwrap(), "4");
    assert!(
        !before.is_empty(),
        "the r=4 grain has a non-empty live layer before the re-run (else the guard is vacuous)"
    );

    // Re-run the default grain (r=1): a fresh pass boundary that must supersede ONLY `community/1/*`.
    record_grain(&p, 1.0, next);
    let g = p.whole().unwrap();

    // The OTHER grain (r=4) is untouched: its live layer is byte-identical across the r=1 re-run.
    let after = grain_snapshot(&g, "4");
    assert_eq!(
        before, after,
        "re-running the r=1 grain leaves the r=4 grain's live layer byte-identical (per-resolution \
         supersession, not global)"
    );

    // The re-run grain is duplicate-free: each of its members carries exactly ONE live `community/1/*`
    // membership - the prior pass's memberships were superseded, not left as stale duplicates.
    let memberships = live_memberships(&g);
    let mut per_member: BTreeMap<&String, usize> = BTreeMap::new();
    for (from, to) in &memberships {
        if to.starts_with("community/1/") {
            *per_member.entry(from).or_default() += 1;
        }
    }
    assert!(
        !per_member.is_empty(),
        "the re-run grain still has live memberships"
    );
    for (m, count) in &per_member {
        assert_eq!(
            *count, 1,
            "member {m} carries exactly one live default-grain membership after the re-run; got {count}"
        );
    }

    // The re-run grain's community set is still the default namespace only (the deterministic pass
    // reproduces `community/1/*`), and the fine grain's communities all survive alongside.
    let comms = community_nodes(&g);
    assert!(
        comms.iter().any(|c| c.starts_with("community/1/")),
        "the re-run default grain's communities are live; got {comms:?}"
    );
    assert!(
        comms.iter().any(|c| c.starts_with("community/4/")),
        "the fine grain's communities survive the default-grain re-run; got {comms:?}"
    );
}
