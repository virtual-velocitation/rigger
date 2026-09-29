//! Fold fixtures over the graph projector: folding a hand-built JSON event straight onto a
//! `Projector` (the raw on-log JSON - never an in-crate payload struct - so a test pins the
//! contract the log actually carries), the on-log payloads the code-ingest, decision and
//! reference folds read, the canonical spec-53 two-subsystem coupling seed, and the two read-backs
//! suites take of a folded `Graph`. `apply` returns `Err` on a fold failure, so every helper
//! unwraps it: a successful call is itself evidence the payload folded.

use std::collections::BTreeSet;
use std::time::{Duration, UNIX_EPOCH};

use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{
    Graph, Projection, KIND_COMMUNITY, META_ACTOR, REL_IN_COMMUNITY, TYPE_CODE_ENTITY_EXTRACTED,
    TYPE_DECISION_MADE, TYPE_EDGE_INFERRED,
};
use rigger::eventstore::Event;

/// A `type_` event carrying `json` as its payload.
pub fn event_of(type_: &str, json: serde_json::Value) -> Event {
    Event::new(type_, serde_json::to_vec(&json).unwrap())
}

/// Fold `events` into `p` through the public fold, and insist they landed: the one spelling of
/// seeding a graph a test then reads.
pub fn folds(p: &dyn Projection, events: &[Event]) {
    assert_eq!(
        rigger::contextgraph::Fold::of_batch(Some(p), events),
        rigger::contextgraph::Fold::Folded
    );
}

/// Fold `e` onto `p` at log position `pos`.
fn fold_at(p: &Projector, pos: u64, mut e: Event) {
    e.position = pos;
    folds(p, std::slice::from_ref(&e));
}

/// Fold one `type_` event carrying `json` onto `p` at log position `pos`.
pub fn apply_json(p: &Projector, pos: u64, type_: &str, json: serde_json::Value) {
    apply_json_as(p, pos, type_, json, None);
}

/// Fold one `type_` event from its raw on-log JSON at `pos`, optionally stamping the acting
/// persona in `META_ACTOR` (the metadata the conductor puts on every real emit).
pub fn apply_json_as(
    p: &Projector,
    pos: u64,
    type_: &str,
    json: serde_json::Value,
    actor: Option<&str>,
) {
    let mut e = event_of(type_, json);
    if let Some(a) = actor {
        e.meta.insert(META_ACTOR.to_string(), a.to_string());
    }
    fold_at(p, pos, e);
}

/// Fold one `type_` event from its raw on-log JSON at `pos`, its valid-from stamped `secs` past
/// the epoch, so a test can place each assertion in a distinct run.
pub fn apply_json_at(p: &Projector, pos: u64, type_: &str, json: serde_json::Value, secs: u64) {
    let e = event_of(type_, json).with_valid_from(UNIX_EPOCH + Duration::from_secs(secs));
    fold_at(p, pos, e);
}

/// Fold one `type_` event carrying `json` onto `p` at the position after `*pos`, advancing
/// `*pos` to it - the public event API a real run folds through, one position per event.
pub fn apply_next_json(p: &Projector, pos: &mut u64, type_: &str, json: serde_json::Value) {
    *pos += 1;
    apply_json(p, *pos, type_, json);
}

/// The `CodeEntityExtracted` payload: `file` defines `name`, a `kind` at `line`, in `lang`.
pub fn code_entity_json(
    file: &str,
    name: &str,
    kind: &str,
    line: u64,
    lang: &str,
) -> serde_json::Value {
    serde_json::json!({ "file": file, "name": name, "kind": kind, "line": line, "lang": lang })
}

/// The `CodeEntityExtracted` payload of one extraction batch's Rust function definition. `fresh`
/// marks the FIRST event of a file's batch: the fold supersedes the file's prior structural edges
/// before folding it, so a re-extraction replaces rather than accretes.
pub fn def_json(file: &str, name: &str, line: u64, fresh: bool) -> serde_json::Value {
    serde_json::json!({
        "file": file, "name": name, "kind": "function", "line": line, "lang": "rust",
        "fresh": fresh,
    })
}

/// The caller-less `EdgeInferred` payload: `file` references `name`.
pub fn ref_json(file: &str, name: &str) -> serde_json::Value {
    serde_json::json!({ "file": file, "name": name, "lang": "rust" })
}

/// The caller-less `EdgeInferred` payload as the bytes a log append carries.
pub fn edge_inferred(file: &str, name: &str) -> Vec<u8> {
    serde_json::to_vec(&ref_json(file, name)).unwrap()
}

/// The CALLER-ATTRIBUTED `EdgeInferred` payload (spec 37): `file` references `name` from inside
/// the enclosing definition `caller`, exactly what the emit pass records for a call in a function
/// body, which the fold turns into `<file>::<caller> --CALLS--> <target>`.
pub fn call_json(file: &str, name: &str, caller: &str) -> serde_json::Value {
    serde_json::json!({ "file": file, "name": name, "lang": "rust", "caller": caller })
}

/// The `DecisionMade` payload. Each entry in `governs` folds a `decision --GOVERNS--> file` edge; a
/// non-empty `supersedes` folds `decision --SUPERSEDES--> <id>` and INVALIDATES (stamps `valid_to`
/// on, never deletes) the superseded decision's live `GOVERNS` edges.
pub fn decision_json(
    id: &str,
    summary: &str,
    governs: &[&str],
    supersedes: &str,
) -> serde_json::Value {
    serde_json::json!({
        "id": id, "summary": summary, "governs": governs, "supersedes": supersedes,
    })
}

/// Fold one code entity (see [`code_entity_json`]) at `pos`.
pub fn apply_code_entity(
    p: &Projector,
    pos: u64,
    file: &str,
    name: &str,
    kind: &str,
    line: u64,
    lang: &str,
) {
    apply_json(
        p,
        pos,
        TYPE_CODE_ENTITY_EXTRACTED,
        code_entity_json(file, name, kind, line, lang),
    );
}

/// Fold one definition (spec 29a) at `pos`, its line the position: the file node, the
/// `<file>::<name>` entity node (carrying the `name` attr that marks a real definition), and their
/// `CONTAINS` edge.
pub fn def(p: &Projector, pos: u64, file: &str, name: &str) {
    apply_code_entity(p, pos, file, name, "function", pos, "rust");
}

/// Fold one batch definition (see [`def_json`]) at `pos`.
pub fn apply_def(p: &Projector, pos: u64, file: &str, name: &str, line: u64, fresh: bool) {
    apply_json(
        p,
        pos,
        TYPE_CODE_ENTITY_EXTRACTED,
        def_json(file, name, line, fresh),
    );
}

/// Like [`apply_def`] but stamps the event's valid-from `secs` past the epoch, so a re-extraction
/// batch's supersession stamps a predictable `valid_to` - the retention boundary spec 41 keys on.
#[allow(clippy::too_many_arguments)]
pub fn apply_def_at(
    p: &Projector,
    pos: u64,
    file: &str,
    name: &str,
    line: u64,
    fresh: bool,
    secs: u64,
) {
    apply_json_at(
        p,
        pos,
        TYPE_CODE_ENTITY_EXTRACTED,
        def_json(file, name, line, fresh),
        secs,
    );
}

/// Fold into `p`, at log position `pos`, a caller-less reference from `file` to `name`.
pub fn apply_ref(p: &Projector, pos: u64, file: &str, name: &str) {
    apply_json(p, pos, TYPE_EDGE_INFERRED, ref_json(file, name));
}

/// Fold one caller-attributed call (see [`call_json`]) at `pos`. When `name` is defined in
/// ANOTHER file it lands on a BARE same-file placeholder the detection pass resolves by unique
/// name-suffix.
pub fn apply_call(p: &Projector, pos: u64, file: &str, name: &str, caller: &str) {
    apply_json(p, pos, TYPE_EDGE_INFERRED, call_json(file, name, caller));
}

/// Fold one decision (see [`decision_json`]) at `pos`.
pub fn apply_decision(
    p: &Projector,
    pos: u64,
    id: &str,
    summary: &str,
    governs: &[&str],
    supersedes: &str,
) {
    apply_json(
        p,
        pos,
        TYPE_DECISION_MADE,
        decision_json(id, summary, governs, supersedes),
    );
}

/// Fold a decision `id` GOVERNS `path` at `pos`, valid from `secs` past the epoch. GOVERNS is the
/// surviving content edge the spec-40 upsert-live dedup is demonstrated over: `secs` lets a test
/// assert the collapsed edge keeps the EARLIEST assertion time, and `pos` becomes the edge's
/// Hold `nodes` in `p`'s graph at `pos`: a decision naming them, so a graph-derived attachment (a
/// community or concept membership) folded onto them is live (spec 101: an attachment on a node the
/// graph does not hold is not).
pub fn hold(p: &Projector, pos: u64, nodes: &[&str]) {
    apply_decision(p, pos, &format!("hold@{pos}"), "held", nodes, "");
}

/// `source`, so the LATEST assertion wins.
pub fn apply_governs_at(p: &Projector, pos: u64, id: &str, path: &str, secs: u64) {
    apply_json_at(
        p,
        pos,
        TYPE_DECISION_MADE,
        decision_json(id, "x", &[path], ""),
        secs,
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
    apply_call(p, 9, "src/combat/hit.rs", "send", "strike");
    apply_call(p, 10, "src/combat/hit.rs", "recv", "strike");
    apply_call(p, 11, "src/combat/hit.rs", "send", "block");
    apply_call(p, 12, "src/net/link.rs", "strike", "send");
    apply_call(p, 13, "src/net/link.rs", "block", "recv");

    // Subsystem B: dense cross-file coupling between render and ui.
    apply_call(p, 14, "src/render/draw.rs", "layout", "paint");
    apply_call(p, 15, "src/render/draw.rs", "show", "paint");
    apply_call(p, 16, "src/render/draw.rs", "layout", "shade");
    apply_call(p, 17, "src/ui/hud.rs", "paint", "layout");
    apply_call(p, 18, "src/ui/hud.rs", "shade", "show");

    // A single weak bridge from A to B (one call): too thin to merge the two subsystems.
    apply_call(p, 19, "src/combat/hit.rs", "paint", "strike");
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
