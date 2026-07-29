//! Periphery (contract / API / integration) tests for spec 53's EVENT-SOURCED FOLD (criterion 3):
//! a `CommunityAssigned` event folds into a `KIND_COMMUNITY` super-node plus a live
//! `<member> --IN_COMMUNITY--> <community>` edge with a deterministic label. These run OUTSIDE the
//! crate, over the library's PUBLIC surface (`rigger::contextgraph::...` + `Projector::apply` /
//! `Projector::whole`), so they guard the boundaries the inside-out fold / rebuild / supersession
//! unit tests are structurally blind to:
//!
//!  - the SERIALIZED-FORM back-compat contract. `resolution`, `hash`, and `fresh` are all
//!    `#[serde(default)]` (and `fresh` is `skip_serializing_if`), so a historical event recorded
//!    before those fields existed - or a minimal `{node, community}` emit - must still fold, and an
//!    event with NO `fresh` key must fold as a NON-boundary that never supersedes. The inside-out
//!    helper always supplies all five fields, so it never exercises the default arms; these tests
//!    drive the raw on-log JSON, the form a rebuild actually replays, deliberately bypassing the
//!    in-crate payload struct to pin the JSON contract rather than the Rust type.
//!  - the PUBLIC-CONST + TIER contract of the fold. An external consumer reads the projected
//!    community layer as `KIND_COMMUNITY` nodes joined by `REL_IN_COMMUNITY` edges folded at
//!    `TIER_INFERRED` (a derived grouping one confidence step below its structural inputs); the
//!    inside-out test asserts the rel but never the tier.
//!  - the persisted LITERAL forms. The event type on the log and the node kind / edge rel in the
//!    graph db are replayed on every rebuild; a drift silently orphans historical events and rows.
//!  - PERMUTATION determinism. The inside-out rebuild folds the same log in the same order twice;
//!    this drives the SAME assignment SET in a DIFFERENT arrival order and proves the derived
//!    community node (id, kind, attrs incl the degree-derived label) and membership edges are
//!    byte-identical - the label pick is a pure function of the live member set, not arrival order.

use std::collections::BTreeSet;

use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{
    Graph, Projection, KIND_COMMUNITY, REL_IN_COMMUNITY, TIER_INFERRED, TYPE_CODE_ENTITY_EXTRACTED,
    TYPE_COMMUNITY_ASSIGNED, TYPE_EDGE_INFERRED,
};
use rigger::eventstore::Event;

/// Fold an event built from its raw on-log JSON bytes at `pos` - the SERIALIZED form a rebuild
/// replays - deliberately bypassing the in-crate payload structs so a test pins the JSON contract,
/// not the Rust type. `apply` returns `Err` on a deserialize failure, so a successful call is itself
/// evidence the payload satisfied the fold's contract.
fn apply_json(p: &Projector, pos: u64, type_: &str, json: serde_json::Value) {
    let mut e = Event::new(type_, serde_json::to_vec(&json).unwrap());
    e.position = pos;
    p.apply(&e).unwrap();
}

/// Fold one `CodeEntityExtracted` (spec 29a): a definition, folding its file node, the
/// `<file>::<name>` entity node (carrying a `name` attr - the label source), and their `CONTAINS`
/// edge. The coupling structure the community label's degree pick is computed over.
fn entity(p: &Projector, pos: u64, file: &str, name: &str) {
    apply_json(
        p,
        pos,
        TYPE_CODE_ENTITY_EXTRACTED,
        serde_json::json!({ "file": file, "name": name, "kind": "function", "line": pos, "lang": "rust" }),
    );
}

/// Fold one caller-attributed reference (spec 37): a `<file>::<caller> --CALLS--> <file>::<callee>`
/// edge, so a hub caller accrues real structural degree the deterministic label ranks by.
fn call(p: &Projector, pos: u64, file: &str, callee: &str, caller: &str) {
    apply_json(
        p,
        pos,
        TYPE_EDGE_INFERRED,
        serde_json::json!({ "file": file, "name": callee, "caller": caller, "lang": "rust" }),
    );
}

/// The live `IN_COMMUNITY` targets of `member` in `g` (whole() returns only live edges), as a set.
fn live_memberships(g: &Graph, member: &str) -> BTreeSet<String> {
    g.edges
        .iter()
        .filter(|e| e.rel == REL_IN_COMMUNITY && e.from == member)
        .map(|e| e.to.clone())
        .collect()
}

/// A deterministic snapshot of the whole community layer read over the PUBLIC surface: every
/// `KIND_COMMUNITY` node (id, kind, ordered attrs) and every LIVE `IN_COMMUNITY` edge (from, to),
/// sorted. Two derivations of the same assignment SET must produce byte-identical snapshots.
fn community_snapshot(g: &Graph) -> Vec<String> {
    let mut rows: Vec<String> = Vec::new();
    for n in &g.nodes {
        if n.kind == KIND_COMMUNITY {
            // `attrs` is a `BTreeMap`, so its Debug is key-ordered and byte-stable.
            rows.push(format!("node\t{}\t{}\t{:?}", n.id, n.kind, n.attrs));
        }
    }
    for e in &g.edges {
        if e.rel == REL_IN_COMMUNITY {
            rows.push(format!("edge\t{}\t{}", e.from, e.to));
        }
    }
    rows.sort();
    rows
}

/// The canonical spec-53 coupling graph: an `apply_damage` hub that CALLS three symbols (degree-4
/// highest-degree member), a `clamp` and a `send` in DIFFERENT directories (`src/combat`,
/// `src/net`), and a `util.rs` pair. Shared so the community-layer tests fold the SAME structure.
fn seed_coupling(p: &Projector) {
    entity(p, 1, "src/combat/hit.rs", "apply_damage");
    entity(p, 2, "src/combat/hit.rs", "clamp");
    entity(p, 3, "src/net/socket.rs", "send");
    entity(p, 4, "src/util.rs", "alpha");
    entity(p, 5, "src/util.rs", "zeta");
    // `apply_damage` calls three symbols -> the highest-degree hub (1 CONTAINS + 3 CALLS).
    call(p, 6, "src/combat/hit.rs", "clamp", "apply_damage");
    call(p, 7, "src/combat/hit.rs", "min", "apply_damage");
    call(p, 8, "src/combat/hit.rs", "max", "apply_damage");
}

#[test]
fn a_minimal_community_assigned_event_folds_with_defaulted_attrs_backcompat() {
    // Serialized-form back-compat: the on-log form a pre-`resolution`/`hash`/`fresh` emit would have
    // written - only `node` and `community`, no other key at all. All three later fields are
    // `#[serde(default)]`, so this must still fold; the inside-out helper always supplies all five,
    // so these default arms are untested by it. A rebuild that replays a pre-field log must not
    // error, and the attrs must default deterministically rather than aborting the fold.
    let p = Projector::open(":memory:", "test").unwrap();
    entity(&p, 1, "src/a.rs", "solo");
    apply_json(
        &p,
        2,
        TYPE_COMMUNITY_ASSIGNED,
        // Deliberately minimal: no resolution / hash / fresh key.
        serde_json::json!({ "node": "src/a.rs::solo", "community": "community/0/0" }),
    );

    let g = p.whole().unwrap();
    let c = g
        .nodes
        .iter()
        .find(|n| n.id == "community/0/0")
        .expect("a minimal CommunityAssigned still folds a community node");
    assert_eq!(
        c.kind, KIND_COMMUNITY,
        "the minimal event still folds a KIND_COMMUNITY super-node; got {:?}",
        c
    );
    assert_eq!(
        c.attrs.get("resolution").map(String::as_str),
        Some("0"),
        "a missing resolution defaults to the f64 default 0.0, stored as \"0\"; got {:?}",
        c.attrs
    );
    assert_eq!(
        c.attrs.get("hash").map(String::as_str),
        Some(""),
        "a missing hash defaults to the empty String, never a fold error; got {:?}",
        c.attrs
    );
    assert_eq!(
        c.attrs.get("label").map(String::as_str),
        Some("solo"),
        "the label is still derived (its one live member); got {:?}",
        c.attrs
    );
    assert!(
        g.edges.iter().any(|e| e.rel == REL_IN_COMMUNITY
            && e.from == "src/a.rs::solo"
            && e.to == "community/0/0"
            && e.valid_to.is_none()),
        "a live membership edge folds from the minimal event; got {:?}",
        g.edges
    );
}

#[test]
fn the_community_fold_exposes_kind_relation_and_inferred_tier_over_the_public_surface() {
    // The public-const + tier contract as an external consumer reads it: the community super-node is
    // a KIND_COMMUNITY node, its members are joined by REL_IN_COMMUNITY edges, and those edges fold
    // at TIER_INFERRED - a derived grouping one confidence step below the explicit structural edges
    // detection runs over. The inside-out test asserts the rel but never the tier.
    let p = Projector::open(":memory:", "test").unwrap();
    seed_coupling(&p);
    // A resolution-1.0 pass grouping members ACROSS directory lines into community/1/0. First event
    // carries the pass boundary.
    apply_json(
        &p,
        9,
        TYPE_COMMUNITY_ASSIGNED,
        serde_json::json!({ "node": "src/combat/hit.rs::apply_damage", "community": "community/1/0", "resolution": 1.0, "hash": "h", "fresh": true }),
    );
    apply_json(
        &p,
        10,
        TYPE_COMMUNITY_ASSIGNED,
        serde_json::json!({ "node": "src/combat/hit.rs::clamp", "community": "community/1/0", "resolution": 1.0, "hash": "h", "fresh": false }),
    );
    apply_json(
        &p,
        11,
        TYPE_COMMUNITY_ASSIGNED,
        serde_json::json!({ "node": "src/net/socket.rs::send", "community": "community/1/0", "resolution": 1.0, "hash": "h", "fresh": false }),
    );

    let g = p.whole().unwrap();
    let c = g
        .nodes
        .iter()
        .find(|n| n.id == "community/1/0")
        .expect("the community super-node folded");
    assert_eq!(
        c.kind, KIND_COMMUNITY,
        "the super-node folds at KIND_COMMUNITY; got {:?}",
        c
    );
    let mem = g
        .edges
        .iter()
        .find(|e| {
            e.rel == REL_IN_COMMUNITY
                && e.from == "src/combat/hit.rs::apply_damage"
                && e.to == "community/1/0"
        })
        .expect("a membership edge ties the hub member to its community");
    assert!(
        mem.valid_to.is_none(),
        "the membership edge is live; got {:?}",
        mem
    );
    assert_eq!(
        mem.tier, TIER_INFERRED,
        "the derived membership edge folds at TIER_INFERRED (one step below its structural inputs); got {:?}",
        mem
    );
}

#[test]
fn a_community_assigned_event_without_a_fresh_key_is_a_non_boundary_and_never_supersedes() {
    // Serialized-form back-compat: `fresh` is `#[serde(default)]` + `skip_serializing_if`, so an
    // on-log event with NO `fresh` key folds as a NON-boundary - it never supersedes prior
    // memberships, it accretes. The inside-out helper always writes a `fresh` value, so the
    // absent-key path is untested by it. A pre-`fresh` log must replay without retiring anything
    // (the pass-boundary supersession is opt-in, gated on the emitter setting `fresh`).
    let p = Projector::open(":memory:", "test").unwrap();
    entity(&p, 1, "a.rs", "x");
    // First membership, established WITHOUT a `fresh` key (the pre-field on-log form).
    apply_json(
        &p,
        2,
        TYPE_COMMUNITY_ASSIGNED,
        serde_json::json!({ "node": "a.rs::x", "community": "community/1/0", "resolution": 1.0, "hash": "h" }),
    );
    // A second resolution-1.0 membership, ALSO with no `fresh` key: a non-boundary fold, so it must
    // accrete rather than replace - BOTH stay live.
    apply_json(
        &p,
        3,
        TYPE_COMMUNITY_ASSIGNED,
        serde_json::json!({ "node": "a.rs::x", "community": "community/1/1", "resolution": 1.0, "hash": "h" }),
    );

    let g = p.whole().unwrap();
    let live = live_memberships(&g, "a.rs::x");
    let expected: BTreeSet<String> = ["community/1/0".to_string(), "community/1/1".to_string()]
        .into_iter()
        .collect();
    assert_eq!(
        live, expected,
        "with no `fresh` boundary a re-run accretes: both memberships stay live; got {live:?}"
    );
}

#[test]
fn the_community_layer_serialized_literals_are_stable_and_the_fold_matches_the_on_log_type() {
    // These strings are PERSISTED (the event type on the log; the node kind and edge rel in the
    // graph db) and REPLAYED on every rebuild, so a drift silently orphans historical events / rows.
    assert_eq!(TYPE_COMMUNITY_ASSIGNED, "CommunityAssigned");
    assert_eq!(KIND_COMMUNITY, "community");
    assert_eq!(REL_IN_COMMUNITY, "IN_COMMUNITY");

    // A historical event whose raw on-log type literal is "CommunityAssigned" (bypassing the const
    // entirely) must still hit the fold arm - the arm matches the persisted string, not a renamed
    // const - and must store the literal `community` kind / `IN_COMMUNITY` rel a later read expects.
    let p = Projector::open(":memory:", "test").unwrap();
    entity(&p, 1, "a.rs", "x");
    apply_json(
        &p,
        2,
        "CommunityAssigned",
        serde_json::json!({ "node": "a.rs::x", "community": "community/1/0", "resolution": 1.0, "hash": "h", "fresh": true }),
    );

    let g = p.whole().unwrap();
    assert!(
        g.nodes
            .iter()
            .any(|n| n.id == "community/1/0" && n.kind == "community"),
        "the literal-typed event folds a node stored with the literal `community` kind; got {:?}",
        g.nodes
    );
    assert!(
        g.edges
            .iter()
            .any(|e| e.rel == "IN_COMMUNITY" && e.from == "a.rs::x" && e.to == "community/1/0"),
        "the membership edge is stored with the literal `IN_COMMUNITY` rel; got {:?}",
        g.edges
    );
}

#[test]
fn the_community_derivation_is_a_pure_function_of_the_assignment_set_not_arrival_order() {
    // Strengthens the inside-out same-order rebuild: the SAME assignment SET folded in a DIFFERENT
    // arrival order yields byte-identical community nodes (id, kind, attrs including the
    // degree-derived label) and live membership edges. The label pick (`max`/`min` over the live
    // members' degree) is order-independent; this proves it from OUTSIDE the crate, so a rebuild
    // that replays the log in position order re-derives the identical grouping.
    let members = [
        "src/combat/hit.rs::apply_damage",
        "src/combat/hit.rs::clamp",
        "src/net/socket.rs::send",
    ];

    let build_with_order = |order: &[&str]| -> Vec<String> {
        let p = Projector::open(":memory:", "test").unwrap();
        seed_coupling(&p);
        for (i, m) in order.iter().enumerate() {
            apply_json(
                &p,
                9 + i as u64,
                TYPE_COMMUNITY_ASSIGNED,
                serde_json::json!({ "node": m, "community": "community/1/0", "resolution": 1.0, "hash": "h", "fresh": i == 0 }),
            );
        }
        community_snapshot(&p.whole().unwrap())
    };

    let forward = build_with_order(&members);
    let mut rev: Vec<&str> = members.to_vec();
    rev.reverse();
    let reversed = build_with_order(&rev);

    assert!(
        forward.iter().any(|r| r.starts_with("node\t"))
            && forward.iter().any(|r| r.starts_with("edge\t")),
        "precondition: the fixture folded a community node and membership edges; got {forward:?}"
    );
    // The label is the highest-degree hub, grouped ACROSS the src/combat and src/net directory lines.
    assert!(
        forward
            .iter()
            .any(|r| r.starts_with("node\t") && r.contains("apply_damage")),
        "the community label is its highest-degree member (`apply_damage`), across directories; got {forward:?}"
    );
    assert_eq!(
        forward, reversed,
        "the same assignment SET in a different arrival order re-derives byte-identical community rows"
    );
}
