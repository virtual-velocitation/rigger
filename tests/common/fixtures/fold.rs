//! Fold fixtures over the graph projector: folding a hand-built JSON event straight onto a
//! `Projector` (the raw on-log JSON - never an in-crate payload struct - so a test pins the
//! contract the log actually carries), the on-log payloads the code-ingest, decision and
//! reference folds read, the canonical spec-53 two-subsystem coupling seed, the spec-119
//! show-renderer seed with the oracles its `rigger_graph` `show` and `around` objects are checked
//! by, the live edges and calls a projector serves, and the read-backs suites take of a folded
//! `Graph`. `apply` returns `Err`
//! on a fold failure, so every helper unwraps it: a successful call is itself evidence the
//! payload folded.

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, UNIX_EPOCH};

use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{
    around, entity_site, site_fields, EntitySite, Graph, Located, Projection, CODE_KIND_CONSTANT,
    CODE_KIND_FIELD, CODE_KIND_FUNCTION, KIND_ARCH_DECISION, KIND_COMMUNITY, KIND_DESIGN_DOC,
    META_ACTOR, REL_CALLS, REL_IN_COMMUNITY, REL_REFERENCES, TYPE_CODE_ENTITY_EXTRACTED,
    TYPE_DECISION_MADE, TYPE_DOC_CONCEPT_EXTRACTED, TYPE_EDGE_INFERRED, TYPE_REVIEW_FINDING,
};
use rigger::eventstore::Event;
use rigger::retention::GenerationIngested;

/// A `type_` event carrying `json` as its payload.
pub fn event_of(type_: &str, json: serde_json::Value) -> Event {
    Event::new(type_, serde_json::to_vec(&json).unwrap())
}

/// Each event of `events` as `(type, payload text)`, in order: the form a test compares a batch
/// by, an event having no equality of its own.
pub fn wire(events: &[Event]) -> Vec<(&str, &str)> {
    events
        .iter()
        .map(|e| {
            (
                e.type_.as_str(),
                std::str::from_utf8(&e.data).expect("an extraction payload is UTF-8"),
            )
        })
        .collect()
}

/// [`wire`] with each pair owned: the form a test compares by when the batch does not outlive
/// the comparison.
pub fn wire_owned(events: &[Event]) -> Vec<(String, String)> {
    wire(events)
        .into_iter()
        .map(|(type_, payload)| (type_.to_string(), payload.to_string()))
        .collect()
}

/// `events` - `(type, payload text)` pairs, the form [`wire`] answers - as unkeyed batch events.
pub fn events_of(events: &[(&str, &str)]) -> Vec<Event> {
    events
        .iter()
        .map(|(type_, payload)| Event::new(*type_, payload.as_bytes().to_vec()))
        .collect()
}

/// The `gc` batch of `path` for no bytes: the one boundary event of a path that holds no file an
/// ingest reads.
pub fn no_bytes_batch(path: &str) -> Vec<Event> {
    events_of(&[(
        "EdgeInferred",
        &format!(r#"{{"file":"{path}","name":"","lang":"unknown","fresh":true}}"#),
    )])
}

/// What a ledger entry of `<prefix>/<file>` at `generation` records, its batch extracted from
/// `blob` under the walk's flag `excluded`.
pub fn generation_ingested(
    prefix: &str,
    file: &str,
    generation: &str,
    blob: &str,
    excluded: bool,
) -> GenerationIngested {
    GenerationIngested {
        prefix: prefix.to_string(),
        file: file.to_string(),
        generation: generation.to_string(),
        blob: blob.to_string(),
        excluded,
    }
}

/// Every live edge `p` serves as `(from, rel, to, valid_from, source)`, sorted.
pub fn live_edges(p: &Projector) -> Vec<(String, String, String, i64, u64)> {
    let mut edges: Vec<_> = p
        .whole()
        .unwrap()
        .edges
        .into_iter()
        .map(|e| (e.from, e.rel, e.to, e.valid_from, e.source))
        .collect();
    edges.sort();
    edges
}

/// Every live `CALLS` edge `p` serves as `(caller, callee)`, sorted.
pub fn live_calls(p: &Projector) -> Vec<(String, String)> {
    live_edges(p)
        .into_iter()
        .filter(|(_, rel, ..)| rel == REL_CALLS)
        .map(|(from, _, to, ..)| (from, to))
        .collect()
}

/// Every live `REFERENCES` and `CALLS` edge `p` serves, as `(from, rel, to, tier)`, sorted:
/// where each reference landed and at what tier.
pub fn landed(p: &Projector) -> Vec<(String, String, String, String)> {
    let mut edges: Vec<_> = p
        .whole()
        .unwrap()
        .edges
        .into_iter()
        .filter(|e| e.rel == REL_REFERENCES || e.rel == REL_CALLS)
        .map(|e| (e.from, e.rel, e.to, e.tier))
        .collect();
    edges.sort();
    edges
}

/// The `(from, rel, to, tier)` rows of `f.rs`'s reference and of `f.rs::main`'s call, both
/// landed on `to` at `tier`, in [`landed`]'s order.
pub fn a_call_from_main(to: &str, tier: &str) -> Vec<(String, String, String, String)> {
    [("f.rs", REL_REFERENCES), ("f.rs::main", REL_CALLS)]
        .map(|(from, rel)| (from.into(), rel.into(), to.into(), tier.into()))
        .to_vec()
}

/// Fold `events` into `p` through the public fold, and insist they landed: the one spelling of
/// seeding a graph a test then reads.
pub fn folds(p: &dyn Projection, events: &[Event]) {
    assert_eq!(
        rigger::contextgraph::Fold::of_batch(|| rigger::contextgraph::wired(Some(p)), events),
        rigger::contextgraph::Fold::Folded
    );
}

/// Fold `e` onto `p` at log position `pos`.
fn fold_at(p: &Projector, pos: u64, mut e: Event) {
    e.position = pos;
    folds(p, std::slice::from_ref(&e));
}

/// A `type_` event carrying `data`, replay-keyed `key` and valid from `secs` after the epoch.
pub fn keyed(type_: &str, data: Vec<u8>, key: &str, secs: u64) -> Event {
    Event::new(type_, data)
        .with_meta(rigger::ingest::META_REPLAY_KEY, key)
        .with_valid_from(UNIX_EPOCH + Duration::from_secs(secs))
}

/// One derived event stamped with replay key `key`, valid from `secs`, at `pos`.
pub fn keyed_event(pos: u64, type_: &str, json: serde_json::Value, key: &str, secs: u64) -> Event {
    let mut e = keyed(type_, serde_json::to_vec(&json).unwrap(), key, secs);
    e.position = pos;
    e
}

/// Fold one [`keyed_event`] onto `p`.
pub fn apply_keyed(
    p: &Projector,
    pos: u64,
    type_: &str,
    json: serde_json::Value,
    key: &str,
    secs: u64,
) {
    folds(p, &[keyed_event(pos, type_, json, key, secs)]);
}

/// Fold onto `p`, at log position `pos`, the first event of generation `generation` of `file`: a
/// definition of the function `name` at line 1 ([`apply_generation_head_event`]) - the batch head
/// a re-extraction of `file` appends, retiring what the generation before it asserted and this
/// one does not.
pub fn apply_generation_head(
    p: &Projector,
    pos: u64,
    file: &str,
    generation: &str,
    name: &str,
    secs: u64,
) {
    let head = def_json(file, name, 1, true);
    apply_generation_head_event(
        p,
        pos,
        file,
        generation,
        TYPE_CODE_ENTITY_EXTRACTED,
        head,
        secs,
    );
}

/// Fold onto `p`, at log position `pos`, the `type_` event carrying `head` as the first event of
/// generation `generation` of `file`: replay-keyed `gc/<file>@<generation>#0` and valid from
/// `secs`.
pub fn apply_generation_head_event(
    p: &Projector,
    pos: u64,
    file: &str,
    generation: &str,
    type_: &str,
    head: serde_json::Value,
    secs: u64,
) {
    let key = format!("gc/{file}@{generation}#0");
    apply_keyed(p, pos, type_, head, &key, secs);
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
    kind_def_json(file, name, "function", line, fresh)
}

/// [`def_json`] for a Rust definition of any `kind`.
pub fn kind_def_json(
    file: &str,
    name: &str,
    kind: &str,
    line: u64,
    fresh: bool,
) -> serde_json::Value {
    serde_json::json!({
        "file": file, "name": name, "kind": kind, "line": line, "lang": "rust",
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

/// Fold into `p`, at log position `pos`, one test-origin reference from `file` line `line` to
/// `name`: the evidence event a test's reference to a definition records, as raw on-log JSON.
pub fn apply_edge_inferred_evidence(p: &Projector, pos: u64, file: &str, name: &str, line: u32) {
    apply_json(
        p,
        pos,
        TYPE_EDGE_INFERRED,
        serde_json::json!({ "file": file, "name": name, "lang": "rust", "line": line, "is_test": true }),
    );
}

/// Fold one caller-attributed call (see [`call_json`]) at `pos`. When `name` is defined in
/// ANOTHER file it lands on the BARE same-file placeholder `<file>::%<name>` the detection
/// pass resolves by its unique leaf.
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

/// The file the show-renderer seed's three definitions live in ([`seed_shown_nodes`]).
pub const SHOWN_FILE: &str = "src/limits.rs";

/// The ids [`seed_shown_nodes`] folds, each shown by exact id (spec 119, SHOW IS ONE RENDERER): a
/// const, a field, a definition whose `kind` attr is empty, the extensionless file's definition
/// `Makefile::all`, a finding, an architecture decision and a doc section.
pub const SHOWN_IDS: [&str; 7] = [
    "src/limits.rs::LIMIT",
    "src/limits.rs::width",
    "src/limits.rs::blank",
    "Makefile::all",
    "f-shown",
    "docs/decisions/one-renderer.md",
    "docs/x.md#sec",
];

/// Fold the nodes of [`SHOWN_IDS`] into `p`: in [`SHOWN_FILE`] a field `width` whose `lang` is
/// empty (folded first, so the file keeps the `lang` its later definitions give it), the const
/// `LIMIT` a same-file test reference proves (so it carries `proven_by`, an attr outside the
/// printed list) and `blank`, whose `kind` attr is empty; `Makefile::all`; the finding `f-shown`
/// ABOUT [`SHOWN_FILE`] and `Makefile`; an architecture decision whose `doc` holds a newline; and
/// the section `docs/x.md#sec`.
pub fn seed_shown_nodes(p: &Projector) {
    apply_code_entity(p, 1, SHOWN_FILE, "width", CODE_KIND_FIELD, 5, "");
    apply_code_entity(p, 2, SHOWN_FILE, "LIMIT", CODE_KIND_CONSTANT, 3, "rust");
    apply_code_entity(p, 3, SHOWN_FILE, "blank", "", 7, "rust");
    apply_edge_inferred_evidence(p, 4, SHOWN_FILE, "LIMIT", 9);
    apply_code_entity(p, 5, "Makefile", "all", CODE_KIND_FUNCTION, 1, "make");
    apply_json(
        p,
        6,
        TYPE_REVIEW_FINDING,
        serde_json::json!({
            "id": "f-shown", "by": "lens:sdet", "unit": "u7",
            "summary": "the width overflows", "about": [SHOWN_FILE, "Makefile"],
        }),
    );
    apply_json(
        p,
        7,
        TYPE_DOC_CONCEPT_EXTRACTED,
        serde_json::json!({
            "kind": KIND_ARCH_DECISION, "id": "docs/decisions/one-renderer.md",
            "title": "One renderer", "doc": "the decision log\nits second line",
        }),
    );
    apply_json(
        p,
        8,
        TYPE_DOC_CONCEPT_EXTRACTED,
        serde_json::json!({
            "kind": KIND_DESIGN_DOC, "id": "docs/x.md#sec", "title": "Sec", "doc": "docs/x.md",
        }),
    );
}

/// A rendered node object's keys less its `id`, which must be `id`: the entries the object
/// carries, compared without order against [`site_fields`].
fn site_keys(object: &serde_json::Value, id: &str) -> BTreeMap<String, serde_json::Value> {
    let mut keys = object.as_object().expect("a node object").clone();
    assert_eq!(
        keys.remove("id"),
        Some(serde_json::json!(id)),
        "the object names its node"
    );
    keys.into_iter().collect()
}

/// The entries [`site_fields`] gives `site`, keyed for an unordered comparison.
fn fields_of(site: &EntitySite) -> BTreeMap<String, serde_json::Value> {
    site_fields(site)
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .collect()
}

/// The one site `graph` locates for `id`.
pub fn located_site(graph: &Projector, id: &str) -> EntitySite {
    let Located::One(site) = graph.locate(id).unwrap() else {
        panic!("{id} locates one node")
    };
    site
}

/// Each of [`SHOWN_IDS`] beside the one site `graph` locates for it, in their order.
pub fn shown_sites(graph: &Projector) -> Vec<(&'static str, EntitySite)> {
    SHOWN_IDS
        .into_iter()
        .map(|id| (id, located_site(graph, id)))
        .collect()
}

/// Spec 119, SHOW IS ONE RENDERER: the seed's const holds `proven_by`, so a surface printing no
/// `proven_by` excludes an attr the node carries, never one it lacks.
pub fn assert_the_shown_const_carries_proven_by(graph: &Projector) {
    assert_eq!(
        attrs_of(graph, "src/limits.rs::LIMIT")
            .get("proven_by")
            .map(String::as_str),
        Some("1"),
        "the const carries proven_by, an attr outside the printed list"
    );
}

/// Spec 119, SHOW IS ONE RENDERER: each of `sites`, the `rigger_graph` `show` site objects of
/// [`SHOWN_IDS`] in their order, is its node's `id` beside the [`site_fields`] entries of the site
/// `graph` locates for it, compared without order.
pub fn assert_shows_are_site_fields(graph: &Projector, sites: &[serde_json::Value]) {
    for ((id, located), site) in shown_sites(graph).iter().zip(sites) {
        assert_eq!(
            site_keys(site, id),
            fields_of(located),
            "{id}: show is site_fields"
        );
    }
}

/// The node objects of a `rigger_graph` `around` answer, sorted by id.
pub fn around_nodes(answer: &serde_json::Value) -> Vec<serde_json::Value> {
    let mut nodes = answer["nodes"].as_array().expect("a node list").clone();
    nodes.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    nodes
}

/// Spec 119, SHOW IS ONE RENDERER: `finding`, the `rigger_graph` `show` site of the seed's
/// finding, carries the two `ABOUT` edges the store holds, where an `around` of [`SHOWN_FILE`]
/// at depth 1 loads one of them.
pub fn assert_finding_shows_past_the_depth_bound(finding: &serde_json::Value) {
    assert_eq!(
        finding["degree"], 2,
        "show reads the finding's degree from the store, past the depth bound"
    );
}

/// Spec 119, SHOW IS ONE RENDERER: each of `nodes`, the `rigger_graph` `around` node objects of
/// [`SHOWN_FILE`] at depth 1, is [`entity_site`] over the `Node` [`around`] loads from `graph`
/// at the degree the object carries, compared without order.
pub fn assert_around_is_entity_sites(graph: &Projector, nodes: &[serde_json::Value]) {
    let loaded = around(graph, &[SHOWN_FILE.to_string()], 1).unwrap();
    for node in nodes {
        let id = node["id"].as_str().unwrap();
        let degree = node["degree"].as_u64().unwrap() as usize;
        let held = loaded
            .nodes
            .iter()
            .find(|n| n.id == id)
            .expect("a loaded node");
        assert_eq!(
            site_keys(node, id),
            fields_of(&entity_site(held, degree)),
            "{id}: an around node is entity_site over its loaded Node"
        );
    }
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

/// The attrs node `id` carries in `p`'s whole graph.
pub fn attrs_of(p: &Projector, id: &str) -> BTreeMap<String, String> {
    let nodes = p.whole().unwrap().nodes;
    let node = nodes.into_iter().find(|n| n.id == id);
    node.unwrap_or_else(|| panic!("{id} is held")).attrs
}

/// The attrs of a Rust definition `name`, a `kind` at `line`, and of nothing else: a node
/// carrying a proof is not equal to them.
pub fn definition(name: &str, kind: &str, line: u64) -> BTreeMap<String, String> {
    definition_in("rust", name, kind, line)
}

/// The attrs of a definition `name` in the language `lang`, a `kind` at `line`, and of nothing
/// else: a node carrying a proof is not equal to them.
pub fn definition_in(lang: &str, name: &str, kind: &str, line: u64) -> BTreeMap<String, String> {
    [
        ("name", name),
        ("kind", kind),
        ("line", &line.to_string()),
        ("lang", lang),
    ]
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .into()
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
