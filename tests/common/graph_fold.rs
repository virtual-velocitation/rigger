//! Shared fold fixtures for the projection periphery suites (code ingest, design intent, the
//! community and concept layers): folding a hand-built JSON event straight onto a `Projector`,
//! the canonical spec-53 two-subsystem coupling seed, and the two read-backs those suites take of
//! a folded `Graph`. Included by each suite through `#[path]`; a suite uses the subset it needs
//! (hence the module-wide `dead_code` allowance, the same convention `tests/common/mod.rs` keeps).

#![allow(dead_code)]

use std::collections::BTreeSet;

use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{
    Graph, Projection, KIND_COMMUNITY, META_ACTOR, REL_IN_COMMUNITY, TYPE_CODE_ENTITY_EXTRACTED,
    TYPE_EDGE_INFERRED,
};
use rigger::eventstore::Event;

/// Fold one `type_` event carrying `json` onto `p` at log position `pos`.
pub fn apply_json(p: &Projector, pos: u64, type_: &str, json: serde_json::Value) {
    apply_json_as(p, pos, type_, json, None);
}

/// Fold one `type_` event from its raw on-log JSON at `pos`, optionally stamping the acting
/// persona in `META_ACTOR` (the metadata the conductor puts on every real emit). Folding the raw
/// JSON - not an in-crate payload struct - pins the contract the log actually carries. `apply`
/// returns `Err` on a fold failure, so a successful call is itself evidence the payload folded.
pub fn apply_json_as(
    p: &Projector,
    pos: u64,
    type_: &str,
    json: serde_json::Value,
    actor: Option<&str>,
) {
    let mut e = Event::new(type_, serde_json::to_vec(&json).unwrap());
    e.position = pos;
    if let Some(a) = actor {
        e.meta.insert(META_ACTOR.to_string(), a.to_string());
    }
    p.apply(&e).unwrap();
}

/// Fold one `type_` event carrying `json` onto `p` at the position after `*pos`, advancing
/// `*pos` to it - the public event API a real run folds through, one position per event.
pub fn apply_next_json(p: &Projector, pos: &mut u64, type_: &str, json: serde_json::Value) {
    *pos += 1;
    apply_json(p, *pos, type_, json);
}

/// Fold one definition (spec 29a): the file node, the `<file>::<name>` entity node (carrying the
/// `name` attr that marks a real definition), and their `CONTAINS` edge.
pub fn def(p: &Projector, pos: u64, file: &str, name: &str) {
    apply_json(
        p,
        pos,
        TYPE_CODE_ENTITY_EXTRACTED,
        serde_json::json!({ "file": file, "name": name, "kind": "function", "line": pos, "lang": "rust" }),
    );
}

/// Fold one caller-attributed call (spec 37): `<file>::<caller> --CALLS--> <name>`, landing on a
/// BARE same-file placeholder the detection pass resolves by unique name-suffix when `name` is
/// defined in ANOTHER file.
pub fn call(p: &Projector, pos: u64, file: &str, name: &str, caller: &str) {
    apply_json(
        p,
        pos,
        TYPE_EDGE_INFERRED,
        serde_json::json!({ "file": file, "name": name, "caller": caller, "lang": "rust" }),
    );
}

/// Seed the canonical spec-53 TWO-SUBSYSTEM coupling graph onto `p`, each subsystem spanning TWO
/// directories and joined to the other by a single weak bridge. Returns the next free position.
/// Replaying this onto a fresh projector reproduces the identical coupling graph.
///
/// Subsystem A spans `src/combat` and `src/net`; subsystem B spans `src/render` and `src/ui`.
/// Every cross-file call lands on a bare placeholder the pass resolves, so A's four entities
/// couple across the combat/net line and B's across the render/ui line. Its coupling graph has 12
/// nodes (eight entities + four files), so at a high enough resolution every node isolates into
/// its own community.
pub fn seed_two_subsystems(p: &Projector) -> u64 {
    // Definitions (each folds its file node + entity + CONTAINS).
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

    // A single weak bridge from A to B (one call): too thin to merge the two subsystems.
    call(p, 19, "src/combat/hit.rs", "paint", "strike");
    20
}

/// The live `rel` targets of `member` in `g` (whole() returns only live edges), as a set.
pub fn live_targets(g: &Graph, rel: &str, member: &str) -> BTreeSet<String> {
    g.edges
        .iter()
        .filter(|e| e.rel == rel && e.from == member)
        .map(|e| e.to.clone())
        .collect()
}

/// Every live node id of `kind` in `g`, sorted.
pub fn live_node_ids(g: &Graph, kind: &str) -> Vec<String> {
    let mut ids: Vec<String> = g
        .nodes
        .iter()
        .filter(|n| n.kind == kind)
        .map(|n| n.id.clone())
        .collect();
    ids.sort();
    ids
}

/// A deterministic snapshot of the whole community layer read over the PUBLIC surface: every
/// `KIND_COMMUNITY` node (id, kind, ordered attrs) and every LIVE `IN_COMMUNITY` edge (from, to),
/// sorted, one tab-separated row each. Two derivations of the same assignment set must produce
/// byte-identical snapshots.
pub fn community_snapshot(g: &Graph) -> Vec<String> {
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
