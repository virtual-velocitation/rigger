//! Fold fixtures over the graph projector: folding a hand-built JSON event straight onto a
//! `Projector` (the raw on-log JSON - never an in-crate payload struct - so a test pins the
//! contract the log actually carries), the on-log payloads the code-ingest, decision, alias and
//! reference folds read, the canonical spec-53 two-subsystem coupling seed, the spec-119
//! show-renderer seed with the oracles its `rigger_graph` `show` and `around` objects are checked
//! by, the live edges and calls a projector serves with the spec-119 enum's `CONTAINS` edges they
//! are checked against, the module declarations' `CONTAINS` edges it serves, the read-backs
//! suites take of a folded `Graph`, the two read batches a rebuild source hands with a writer
//! between them, and a rebuild whose extraction a writer meets. `apply` returns `Err` on a fold failure, so every helper
//! unwraps it - a successful call is itself evidence the payload folded - but
//! [`fold_entry_result`], which answers the error text a failing fold meets.

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, UNIX_EPOCH};

use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{
    around, entity_site, site_fields, Candidate, EntitySite, EntryFold, Graph, Located, Node,
    Projection, CODE_KIND_CONSTANT, CODE_KIND_FIELD, CODE_KIND_FUNCTION, CODE_KIND_TYPE,
    KIND_ARCH_DECISION, KIND_CODE_ENTITY, KIND_COMMUNITY, KIND_DESIGN_DOC, KIND_FILE, LIST_SEP,
    META_ACTOR, REF_KIND_CONST, REF_KIND_TYPE, REF_KIND_USE, REL_CALLS, REL_CONTAINS,
    REL_IN_COMMUNITY, REL_REFERENCES, TYPE_CODE_ENTITY_EXTRACTED, TYPE_DECISION_MADE,
    TYPE_DOC_CONCEPT_EXTRACTED, TYPE_EDGE_INFERRED, TYPE_REVIEW_FINDING,
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

/// The ledger entry of `<prefix>/<file>` at `generation`, its batch extracted from no blob, at log
/// position `pos` and valid from `secs` past the epoch, as the entry's one constructor builds it.
pub fn entry_at(prefix: &str, file: &str, generation: &str, pos: u64, secs: u64) -> Event {
    placed_at(
        generation_ingested(prefix, file, generation, "", false).event(1),
        pos,
        secs,
    )
}

/// The `gc` identity a code file `file` keys its batch under, as its ledger entry names it.
pub fn gc_identity(file: &str) -> String {
    generation_ingested("gc", file, "", "", false).identity()
}

/// `e` at log position `pos`, valid from `secs` past the epoch.
pub fn placed_at(e: Event, pos: u64, secs: u64) -> Event {
    let mut e = e.with_valid_from(UNIX_EPOCH + Duration::from_secs(secs));
    e.position = pos;
    e
}

/// A fresh directory ([`super::temp_root`]) and the path of the `graph.db` file a store opened on
/// it keeps.
pub fn temp_graph_db() -> (tempfile::TempDir, String) {
    let (dir, root) = super::temp_root();
    (dir, format!("{root}/graph.db"))
}

/// Two projects, `ours` and `theirs`, sharing one fresh `graph.db` ([`temp_graph_db`]), with the
/// directory that keeps it.
pub fn ours_and_theirs() -> (tempfile::TempDir, Projector, Projector) {
    let (dir, path) = temp_graph_db();
    let ours = Projector::open(&path, "ours").unwrap();
    let theirs = Projector::open(&path, "theirs").unwrap();
    (dir, ours, theirs)
}

/// `secs` seconds as nanoseconds.
pub fn nanos(secs: u64) -> i64 {
    Duration::from_secs(secs).as_nanos() as i64
}

/// Fold `entry` onto `p` with `batch` as its batch, through `Projection::apply_generation` - the
/// one fold that stages a tagged reference (spec 117) - answering which outcome it met.
pub fn fold_entry_with(p: &dyn Projection, entry: &Event, batch: Vec<Event>) -> EntryFold {
    fold_entry_result(p, entry, batch).unwrap()
}

/// [`fold_entry_with`]'s fold, answering the outcome it met or the text of the error it failed
/// with.
pub fn fold_entry_result(
    p: &dyn Projection,
    entry: &Event,
    batch: Vec<Event>,
) -> Result<EntryFold, String> {
    p.apply_generation(entry, Box::new(move || Ok(Some(batch))))
        .map_err(|e| e.0)
}

/// Fold `entry` onto `p` with `batch` as its batch ([`fold_entry_with`]) and insist the batch was
/// asked for.
pub fn entry_asked(p: &dyn Projection, entry: &Event, batch: Vec<Event>) {
    assert_eq!(fold_entry_with(p, entry, batch), EntryFold::BatchAsked);
}

/// Fold `batch` as generation `generation` of `gc/<file>`, its entry at log position `pos` valid
/// from `secs` ([`entry_at`]), and insist the batch was asked for ([`entry_asked`]).
pub fn fold_asked(
    p: &dyn Projection,
    file: &str,
    generation: &str,
    pos: u64,
    secs: u64,
    batch: Vec<Event>,
) {
    entry_asked(p, &entry_at("gc", file, generation, pos, secs), batch);
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

/// Every live `rel` edge `p` serves as `(from, to)`, sorted.
pub fn live_of(p: &Projector, rel: &str) -> Vec<(String, String)> {
    live_edges(p)
        .into_iter()
        .filter(|(_, edge_rel, ..)| edge_rel == rel)
        .map(|(from, _, to, ..)| (from, to))
        .collect()
}

/// Every live `CALLS` edge `p` serves as `(caller, callee)`, sorted.
pub fn live_calls(p: &Projector) -> Vec<(String, String)> {
    live_of(p, REL_CALLS)
}

/// Every live `rel` edge `p` serves ([`live_of`]) is exactly `edges`, `(from, to)` ids in that
/// sorted order, `why` naming the rule the edges follow.
pub fn assert_live(p: &Projector, rel: &str, edges: &[(&str, &str)], why: &str) {
    let edges: Vec<(String, String)> = edges
        .iter()
        .map(|(from, to)| (from.to_string(), to.to_string()))
        .collect();
    assert_eq!(live_of(p, rel), edges, "{why}");
}

/// Every live `CALLS` edge `p` serves ([`live_calls`]) is exactly `calls`, `(caller, callee)`
/// ids in that sorted order, `why` naming the rule the edges follow.
pub fn assert_live_calls(p: &Projector, calls: &[(&str, &str)], why: &str) {
    assert_live(p, REL_CALLS, calls, why);
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

/// Every live module `CONTAINS` edge `p` serves - one onto a `file` node (spec 117, A MODULE ROW
/// REACHES ITS FILE) - as `(from, to, tier)`, sorted.
pub fn module_edges(p: &Projector) -> Vec<(String, String, String)> {
    let g = p.whole().unwrap();
    let files: BTreeSet<&str> = g
        .nodes
        .iter()
        .filter(|n| n.kind == KIND_FILE)
        .map(|n| n.id.as_str())
        .collect();
    let mut edges: Vec<_> = g
        .edges
        .iter()
        .filter(|e| e.rel == REL_CONTAINS && files.contains(e.to.as_str()))
        .map(|e| (e.from.clone(), e.to.clone(), e.tier.clone()))
        .collect();
    edges.sort();
    edges
}

/// The `AliasDefined` payload: `alias` names the node `canonical`.
pub fn alias_json(alias: &str, canonical: &str) -> serde_json::Value {
    serde_json::json!({ "alias": alias, "canonical": canonical })
}

/// The `(from, rel, to, tier)` rows of `f.rs`'s reference and of `f.rs::main`'s call, both
/// landed on `to` at `tier`, in [`landed`]'s order ([`a_call_from`]).
pub fn a_call_from_main(to: &str, tier: &str) -> Vec<(String, String, String, String)> {
    a_call_from("f.rs", "main", to, tier)
}

/// The `(from, rel, to, tier)` rows of `file`'s reference and of `<file>::<caller>`'s call, both
/// landed on `to` at `tier`, in [`landed`]'s order.
pub fn a_call_from(
    file: &str,
    caller: &str,
    to: &str,
    tier: &str,
) -> Vec<(String, String, String, String)> {
    [
        (file.to_string(), REL_REFERENCES),
        (format!("{file}::{caller}"), REL_CALLS),
    ]
    .map(|(from, rel)| (from, rel.into(), to.into(), tier.into()))
    .to_vec()
}

/// One live `REFERENCES` edge from `from` onto `to` at `tier`, as [`landed`] answers it.
pub fn reference(from: &str, to: &str, tier: &str) -> (String, String, String, String) {
    (from.into(), REL_REFERENCES.into(), to.into(), tier.into())
}

/// Fold `events` into `p` through the public fold, answering what became of them.
pub fn fold_of(p: &dyn Projection, events: &[Event]) -> rigger::contextgraph::Fold {
    rigger::contextgraph::Fold::of_batch(|| rigger::contextgraph::wired(Some(p)), events)
}

/// A REBUILD MEETING A WRITER (spec 122, EXTRACTION OUTSIDE THE WRITE TRANSACTION): rebuild the
/// graph file `graph_db` under the project `test` from `source`, owed when `owed` says so, while
/// `writer` folds `events` through the public fold ([`fold_of`]) between two waits on a barrier
/// whose other party is the rebuild's first ask of `reextract`, which answers `batch` only once
/// the writer's fold has returned. It insists the rebuild ran, passing nothing over and pruning
/// nothing, and asked `reextract` once, and answers what became of the writer's fold.
pub fn rebuild_meeting_a_writer(
    graph_db: &str,
    owed: bool,
    source: &mut rigger::contextgraph::sqlite::RebuildSource,
    writer: &(dyn Projection + Sync),
    events: &[Event],
    batch: &[Event],
) -> rigger::contextgraph::Fold {
    let barrier = std::sync::Barrier::new(2);
    let mut asked = 0;
    let written = std::thread::scope(|s| {
        let folding = s.spawn(|| {
            barrier.wait();
            let written = fold_of(writer, events);
            barrier.wait();
            written
        });
        let rebuilt = Projector::rebuild(
            &Projector::lock_rebuild(graph_db).unwrap(),
            "test",
            owed,
            source,
            &mut |_| {
                asked += 1;
                if asked == 1 {
                    barrier.wait();
                    barrier.wait();
                }
                Ok(Some(batch.to_vec()))
            },
            &mut |_| {},
        );
        assert_eq!(
            rebuilt.map_err(|e| e.0),
            Ok(Some(rigger::contextgraph::sqlite::Rebuilt::default()))
        );
        folding.join().unwrap()
    });
    assert_eq!(asked, 1, "the rebuild asks the entry's batch once");
    written
}

/// Fold `events` into `p` through the public fold ([`fold_of`]), and insist they landed: the one
/// spelling of seeding a graph a test then reads.
pub fn folds(p: &dyn Projection, events: &[Event]) {
    assert_eq!(fold_of(p, events), rigger::contextgraph::Fold::Folded);
}

/// A rebuild source's one read past `after`: the events of `log` past it handed to `sink` as one
/// read batch.
pub fn one_read(
    log: &[Event],
    after: u64,
    sink: &mut rigger::contextgraph::sqlite::RebuildSink,
) -> Result<(), rigger::contextgraph::Error> {
    rigger::contextgraph::sqlite::stream_past(log, after, usize::MAX, sink)
}

/// A rebuild source's one read past `after`: the events of `log` handed to `sink` as two read
/// batches ([`one_read`] each), split before `log[split]`, with `between` run once the first is
/// handed and before the second - where a fold pass holds no transaction, so a writer's commit
/// lands there (spec 122, THE SEAM).
pub fn two_read_batches(
    log: &[Event],
    split: usize,
    after: u64,
    sink: &mut rigger::contextgraph::sqlite::RebuildSink,
    between: impl FnOnce(),
) -> Result<(), rigger::contextgraph::Error> {
    let (first, second) = log.split_at(split);
    one_read(first, after, sink)?;
    between();
    one_read(second, after, sink)
}

/// Fold `e` onto `p` at log position `pos`.
pub fn fold_at(p: &Projector, pos: u64, mut e: Event) {
    e.position = pos;
    folds(p, std::slice::from_ref(&e));
}

/// A fresh in-memory projector with `events` folded onto it one at a time, in order, their log
/// positions numbered from 1 - the shape a real ingest's append gives them.
pub fn fold_events(events: impl IntoIterator<Item = Event>) -> Projector {
    let p = Projector::open(":memory:", "test").unwrap();
    for (zero_based, event) in events.into_iter().enumerate() {
        fold_at(&p, zero_based as u64 + 1, event);
    }
    p
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
    let key = code_batch_key(file, generation, 0);
    apply_keyed(p, pos, type_, head, &key, secs);
}

/// The generation a re-extraction test folds a file's first batch at ([`fold_generation`]).
pub const GENERATION_BEFORE: &str = "before";
/// The generation a re-extraction test folds a file's second batch at, retracting what only
/// [`GENERATION_BEFORE`] asserted.
pub const GENERATION_AFTER: &str = "after";

/// The replay key of event `index` of generation `generation` of `file`'s code batch, under the
/// file's identity `gc/<file>`.
pub fn code_batch_key(file: &str, generation: &str, index: usize) -> String {
    format!("gc/{file}@{generation}#{index}")
}

/// Fold `batch`, the extraction of `file` at generation `generation`, onto `p` one event at a
/// time from the log position after `*pos`, event `index` keyed
/// [`code_batch_key`]`(file, generation, index)`, advancing `*pos` past it: the shape a ledger
/// entry's batch folds in, so a later generation retracts what only this one asserted.
pub fn fold_generation(
    p: &Projector,
    pos: &mut u64,
    file: &str,
    generation: &str,
    batch: Vec<Event>,
) {
    for (index, e) in batch.into_iter().enumerate() {
        *pos += 1;
        let key = code_batch_key(file, generation, index);
        fold_at(p, *pos, e.with_meta(rigger::ingest::META_REPLAY_KEY, key));
    }
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

/// [`code_entity_json`] carrying `path`, the definition's id path in its file (spec 119, IDS
/// FOLLOW THE LANGUAGE'S PATH), `name` staying its leaf.
pub fn path_entity_json(
    file: &str,
    name: &str,
    path: &str,
    kind: &str,
    line: u64,
    lang: &str,
) -> serde_json::Value {
    with_path(code_entity_json(file, name, kind, line, lang), Some(path))
}

/// [`path_entity_json`] carrying `parent`, the id path of the definition enclosing it (spec 119,
/// CONTAINS IS STRUCTURE), at line 1.
pub fn child_entity_json(
    file: &str,
    name: &str,
    path: &str,
    parent: &str,
    kind: &str,
) -> serde_json::Value {
    let mut json = path_entity_json(file, name, path, kind, 1, "rust");
    json["parent"] = parent.into();
    json
}

/// The enum `E` of `a.rs` (spec 119, CONTAINS IS STRUCTURE).
pub const ENUM_E: &str = "a.rs::E";
/// The variant `A` of [`ENUM_E`].
pub const VARIANT_E_A: &str = "a.rs::E::A";
/// The variant `B` of [`ENUM_E`].
pub const VARIANT_E_B: &str = "a.rs::E::B";
/// The live `CONTAINS` edges, `(parent, child)` in sorted order, of `a.rs` defining [`ENUM_E`]
/// with its one variant [`VARIANT_E_A`].
pub const E_HOLDS_A: &[(&str, &str)] = &[("a.rs", ENUM_E), (ENUM_E, VARIANT_E_A)];

/// A definition `(file, leaf, path, kind)`, folded carrying its id path (spec 119, IDS FOLLOW THE
/// LANGUAGE'S PATH).
pub type PathDef<'a> = (&'a str, &'a str, &'a str, &'a str);

/// The method `a.rs::T::new`.
pub const T_NEW: PathDef<'static> = ("a.rs", "new", "T::new", "method");

/// The method `a.rs::U::new`, sharing [`T_NEW`]'s file and leaf.
pub const U_NEW: PathDef<'static> = ("a.rs", "new", "U::new", "method");

/// The candidates `(id, file)` a lookup of `a.rs`'s `new` lists over [`T_NEW`] and [`U_NEW`], in
/// id order.
pub const A_RS_NEWS: &[(&str, &str)] = &[("a.rs::T::new", "a.rs"), ("a.rs::U::new", "a.rs")];

/// Fold each Rust definition of `defs` into `p` from position 1, each at its position's line, and
/// return the next position.
pub fn fold_path_def_rows(p: &Projector, defs: &[PathDef]) -> u64 {
    let mut pos = 0;
    for (file, leaf, path, kind) in defs {
        pos += 1;
        let json = path_entity_json(file, leaf, path, kind, pos, "rust");
        apply_json(p, pos, TYPE_CODE_ENTITY_EXTRACTED, json);
    }
    pos + 1
}

/// `json` carrying `path` under its `path` key when one is given, an id path (spec 119, IDS
/// FOLLOW THE LANGUAGE'S PATH), and unchanged when `None`.
pub fn with_path(mut json: serde_json::Value, path: Option<&str>) -> serde_json::Value {
    if let Some(path) = path {
        json["path"] = path.into();
    }
    json
}

/// One definition as `(name, kind, line)`.
pub type DefRow<'a> = (&'a str, &'a str, u64);

/// The id path the renamed definitions of `src/f.rs` share (spec 119, A RENAMED PATH RETIRES ITS
/// OLD LEAF).
pub const RENAMED_PATH: &str = "A.B";
/// The one node [`RENAMED_PATH`] gives `src/f.rs`'s definitions.
pub const RENAMED_NODE: &str = "src/f.rs::A.B";
/// The fn `B` at line 2, the old leaf of [`RENAMED_PATH`].
pub const OLD_LEAF_FN: DefRow<'static> = ("B", "function", 2);
/// The module `A.B` at line 1, the new leaf of [`RENAMED_PATH`].
pub const NEW_LEAF_MODULE: DefRow<'static> = ("A.B", "module", 1);

/// The `CodeEntityExtracted` event of the Rust `def` in `src/f.rs` at log position `pos`: event
/// `index` of generation `generation`'s batch, its head when `index` is 0, valid from `secs`, and
/// carrying the id path `path` when one is given ([`with_path`]).
pub fn f_rs_batch_def(
    pos: u64,
    generation: &str,
    index: usize,
    (name, kind, line): DefRow,
    path: Option<&str>,
    secs: u64,
) -> Event {
    keyed_event(
        pos,
        TYPE_CODE_ENTITY_EXTRACTED,
        with_path(
            kind_def_json("src/f.rs", name, kind, line, index == 0),
            path,
        ),
        &code_batch_key("src/f.rs", generation, index),
        secs,
    )
}

/// The `(type, payload)` of each event, in fold order, of `src/b.rs`'s fn `g`, the call it makes
/// to `leaf` across files and `tests/t.rs`'s test reference to `leaf` at line 3
/// ([`edge_inferred_evidence_json`]).
pub fn cross_file_call_and_proof(leaf: &str) -> [(&'static str, serde_json::Value); 3] {
    [
        (
            TYPE_CODE_ENTITY_EXTRACTED,
            def_json("src/b.rs", "g", 1, true),
        ),
        (TYPE_EDGE_INFERRED, call_json("src/b.rs", leaf, "g")),
        (
            TYPE_EDGE_INFERRED,
            edge_inferred_evidence_json("tests/t.rs", leaf, 3),
        ),
    ]
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

/// `head` marked `partial`, as a degraded parse's head is.
pub fn marked_partial(mut head: serde_json::Value) -> serde_json::Value {
    head["partial"] = true.into();
    head
}

/// The Rust definition of `name`, a `kind` at `line` in `file` ([`kind_def_json`]), as an unkeyed
/// batch event, its batch's head when `fresh`.
pub fn definition_event(file: &str, name: &str, kind: &str, line: u64, fresh: bool) -> Event {
    event_of(
        TYPE_CODE_ENTITY_EXTRACTED,
        kind_def_json(file, name, kind, line, fresh),
    )
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

/// A caller-attributed `EdgeInferred` call (spec 37) as an unkeyed event: `<file>::<caller>`
/// calls `name` ([`call_json`]).
pub fn call_event(file: &str, name: &str, caller: &str) -> Event {
    event_of(TYPE_EDGE_INFERRED, call_json(file, name, caller))
}

/// The TAGGED `EdgeInferred` payload (spec 117): `file` references `name` as a `ref_kind`
/// reference (`type`, `const` or `use`), from inside the enclosing definition `caller` when it
/// has one, else from the file.
pub fn tagged_ref_json(
    file: &str,
    name: &str,
    ref_kind: &str,
    caller: Option<&str>,
) -> serde_json::Value {
    let mut json = ref_json(file, name);
    json["ref_kind"] = ref_kind.into();
    if let Some(caller) = caller {
        json["caller"] = caller.into();
    }
    json
}

/// The `ref_kind` reference `file` makes to `name` from `caller`, else from the file
/// ([`tagged_ref_json`]), as an unkeyed batch event.
pub fn tagged_ref(file: &str, name: &str, ref_kind: &str, caller: Option<&str>) -> Event {
    event_of(
        TYPE_EDGE_INFERRED,
        tagged_ref_json(file, name, ref_kind, caller),
    )
}

/// The file whose rows [`seed_unresolved_references`] stages: a file-level `use`, and the fns
/// `scan` and `glance` reading what [`RESOLVING_FILE`] defines and what nothing does.
pub const UNRESOLVED_FILE: &str = "src/reader.rs";

/// The fn `scan` of [`UNRESOLVED_FILE`], a node reading unresolved leaves that is no file.
pub const UNRESOLVED_SCAN: &str = "src/reader.rs::scan";

/// The leaves of [`UNRESOLVED_FILE`]'s live rows that resolve to nothing, sorted and distinct
/// (spec 117, UNRESOLVED REFERENCES ARE A READ).
pub const UNRESOLVED_LEAVES: [&str; 4] = ["Elsewhere", "Hinted", "NOWHERE", "Unbuilt"];

/// The file [`seed_unresolved_references`] defines the constant `CEILING` and the types `Halved`
/// and `Hinted` in, staging no row of its own.
pub const RESOLVING_FILE: &str = "src/definer.rs";

/// The statement making every `refs` row's leaf a blob of its own bytes, which no read of it as
/// text takes.
pub const UNREADABLE_LEAVES: &str = "UPDATE refs SET leaf = CAST(leaf AS BLOB)";

/// The error a read of a `refs` row's leaf as text meets once [`UNREADABLE_LEAVES`] ran.
pub const UNREADABLE_LEAF: &str = "Invalid column type Blob at index: 0, name: leaf";

/// Fold, each through `Projection::apply_generation` (the one fold that stages a row; spec 117):
/// [`RESOLVING_FILE`] defining `CEILING`, `Halved` and `Hinted`; then [`UNRESOLVED_FILE`], its head
/// carrying `partial`, whose file-level `use` of `Elsewhere`, whose fn `scan`'s `type` `Unbuilt`,
/// `const` `CEILING`, `type` and `const` `Halved` and `type` `Hinted`, and whose fn `glance`'s
/// `type` `Unbuilt`, `const` `NOWHERE` and `const` `Hinted` leave [`UNRESOLVED_LEAVES`]
/// unresolved - `Unbuilt` read by two fns, `Halved` absent, its `type` row from `scan` resolving
/// where its `const` row does not, and `Hinted` listed, its one resolving row read from `scan` and
/// not from `glance`; then `src/bystander.rs`, whose file-level `type` of `Outlier` resolves to
/// nothing.
pub fn seed_unresolved_references(p: &dyn Projection) {
    let fold = |file: &str, generation: &str, pos: u64, batch: Vec<Event>| {
        fold_asked(p, file, generation, pos, 10 * pos, batch);
    };
    let defines = |name: &str, kind: &str, line: u64| {
        definition_event(RESOLVING_FILE, name, kind, line, line == 1)
    };
    fold(
        RESOLVING_FILE,
        "g-definer",
        1,
        vec![
            defines("CEILING", CODE_KIND_CONSTANT, 1),
            defines("Halved", CODE_KIND_TYPE, 2),
            defines("Hinted", CODE_KIND_TYPE, 3),
        ],
    );
    let head = marked_partial(kind_def_json(
        UNRESOLVED_FILE,
        "scan",
        CODE_KIND_FUNCTION,
        1,
        true,
    ));
    let from = |name: &str, ref_kind: &str, caller: Option<&str>| {
        tagged_ref(UNRESOLVED_FILE, name, ref_kind, caller)
    };
    let (scan, glance) = (Some("scan"), Some("glance"));
    fold(
        UNRESOLVED_FILE,
        "g-reader",
        2,
        vec![
            event_of(TYPE_CODE_ENTITY_EXTRACTED, head),
            definition_event(UNRESOLVED_FILE, "glance", CODE_KIND_FUNCTION, 2, false),
            from("Elsewhere", REF_KIND_USE, None),
            from("Unbuilt", REF_KIND_TYPE, scan),
            from("CEILING", REF_KIND_CONST, scan),
            from("Halved", REF_KIND_TYPE, scan),
            from("Halved", REF_KIND_CONST, scan),
            from("Hinted", REF_KIND_TYPE, scan),
            from("Unbuilt", REF_KIND_TYPE, glance),
            from("NOWHERE", REF_KIND_CONST, glance),
            from("Hinted", REF_KIND_CONST, glance),
        ],
    );
    let bystander = "src/bystander.rs";
    fold(
        bystander,
        "g-bystander",
        3,
        vec![
            definition_event(bystander, "observe", CODE_KIND_FUNCTION, 1, true),
            tagged_ref(bystander, "Outlier", REF_KIND_TYPE, None),
        ],
    );
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

/// The test-origin `EdgeInferred` payload: `file` references `name` at line `line` from a test,
/// the evidence a test's reference to a definition records.
pub fn edge_inferred_evidence_json(file: &str, name: &str, line: u32) -> serde_json::Value {
    serde_json::json!({ "file": file, "name": name, "lang": "rust", "line": line, "is_test": true })
}

/// Fold into `p`, at log position `pos`, one test-origin reference from `file` line `line` to
/// `name` ([`edge_inferred_evidence_json`]), as raw on-log JSON.
pub fn apply_edge_inferred_evidence(p: &Projector, pos: u64, file: &str, name: &str, line: u32) {
    apply_json(
        p,
        pos,
        TYPE_EDGE_INFERRED,
        edge_inferred_evidence_json(file, name, line),
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
    // Definitions (each folds its file node + entity + CONTAINS), then the calls.
    let mut pos = 0;
    for (file, name) in TWO_SUBSYSTEM_DEFS {
        pos += 1;
        def(p, pos, file, name);
    }
    for (file, name, caller) in TWO_SUBSYSTEM_CALLS {
        pos += 1;
        apply_call(p, pos, file, name, caller);
    }
    pos + 1
}

/// The definitions of the canonical spec-53 TWO-SUBSYSTEM coupling graph, `(file, name)`:
/// subsystem A spans `src/combat` and `src/net`, subsystem B `src/render` and `src/ui`.
pub const TWO_SUBSYSTEM_DEFS: [(&str, &str); 8] = [
    ("src/combat/hit.rs", "strike"),
    ("src/combat/hit.rs", "block"),
    ("src/net/link.rs", "send"),
    ("src/net/link.rs", "recv"),
    ("src/render/draw.rs", "paint"),
    ("src/render/draw.rs", "shade"),
    ("src/ui/hud.rs", "layout"),
    ("src/ui/hud.rs", "show"),
];

/// The calls of the canonical spec-53 TWO-SUBSYSTEM coupling graph, `(file, name, caller)`: dense
/// cross-file coupling between combat and net (A), then between render and ui (B), then a single
/// weak bridge from A to B (one call), too thin to merge the two subsystems.
pub const TWO_SUBSYSTEM_CALLS: [(&str, &str, &str); 11] = [
    ("src/combat/hit.rs", "send", "strike"),
    ("src/combat/hit.rs", "recv", "strike"),
    ("src/combat/hit.rs", "send", "block"),
    ("src/net/link.rs", "strike", "send"),
    ("src/net/link.rs", "block", "recv"),
    ("src/render/draw.rs", "layout", "paint"),
    ("src/render/draw.rs", "show", "paint"),
    ("src/render/draw.rs", "layout", "shade"),
    ("src/ui/hud.rs", "paint", "layout"),
    ("src/ui/hud.rs", "shade", "show"),
    ("src/combat/hit.rs", "paint", "strike"),
];

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

/// The [`Located::Many`] a lookup answers with the candidates `(id, file)`, in their order.
pub fn located_many(cands: &[(&str, &str)]) -> Located {
    Located::Many(
        cands
            .iter()
            .map(|(id, file)| Candidate {
                id: id.to_string(),
                file: file.to_string(),
            })
            .collect(),
    )
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
        proven_by_attr(graph, "src/limits.rs::LIMIT").as_deref(),
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
/// at depth 1 loads one of them, and its `summary`, which every `around` node omits.
pub fn assert_finding_shows_past_the_depth_bound(finding: &serde_json::Value) {
    assert_eq!(
        finding["degree"], 2,
        "show reads the finding's degree from the store, past the depth bound"
    );
    assert_eq!(
        finding["summary"], "the width overflows",
        "show keeps the finding's summary"
    );
}

/// Spec 117, UNRESOLVED REFERENCES ARE A READ: `reader`, the `rigger_graph` `show` site of
/// [`UNRESOLVED_FILE`] over [`seed_unresolved_references`], carries its `partial` and its
/// [`UNRESOLVED_LEAVES`] joined by [`LIST_SEP`] as one `unresolved` key, and `definer`, that of
/// [`RESOLVING_FILE`], which lists none, carries no `unresolved` key.
pub fn assert_show_lists_unresolved_leaves(
    reader: &serde_json::Value,
    definer: &serde_json::Value,
) {
    assert_eq!(
        [
            reader.get("partial"),
            reader.get("unresolved"),
            definer.get("unresolved")
        ],
        [
            Some(&serde_json::json!("true")),
            Some(&serde_json::json!(UNRESOLVED_LEAVES.join(LIST_SEP))),
            None
        ],
        "show carries the file's unresolved leaves beside partial, and no key for an empty list"
    );
}

/// Spec 117, UNRESOLVED REFERENCES ARE A READ: `answer`, the `rigger_graph` `around` of
/// [`UNRESOLVED_FILE`] at depth 1 over [`seed_unresolved_references`], loads the file and its two
/// fns, none of them carrying an `unresolved` key.
pub fn assert_around_lists_no_unresolved(answer: &serde_json::Value) {
    assert_eq!(
        around_nodes(answer)
            .iter()
            .map(|node| node.get("unresolved"))
            .collect::<Vec<_>>(),
        [None, None, None],
        "around carries no unresolved key on any node"
    );
}

/// Spec 119, SHOW IS ONE RENDERER: each of `nodes`, the `rigger_graph` `around` node objects of
/// [`SHOWN_FILE`] at depth 1, carries no `summary` key and is [`entity_site`] over the `Node`
/// [`around`] loads from `graph` at the degree the object carries less its `summary` entry,
/// compared without order.
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
            node.get("summary"),
            None,
            "{id}: an around node omits summary"
        );
        let mut fields = fields_of(&entity_site(held, degree));
        fields.remove("summary");
        assert_eq!(
            site_keys(node, id),
            fields,
            "{id}: an around node is entity_site over its loaded Node, less its summary"
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

/// The live `file` node ids of `g` and its live `code-entity` node ids ([`live_node_ids`]), in
/// that order: the node set a fold test compares whole.
pub fn live_file_and_entity_ids(g: &Graph) -> [Vec<String>; 2] {
    [KIND_FILE, KIND_CODE_ENTITY].map(|kind| live_node_ids(g, kind))
}

/// Each of `ids`, owned, in the order given.
pub fn owned_ids(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|id| id.to_string()).collect()
}

/// The `file` node ids `files` and the `code-entity` node ids `entities`, each in the order
/// given ([`owned_ids`]): the expected side of a [`live_file_and_entity_ids`] comparison.
pub fn file_and_entity_ids(files: &[&str], entities: &[&str]) -> [Vec<String>; 2] {
    [files, entities].map(owned_ids)
}

/// Every definition `p`'s whole graph holds - a code entity carrying a non-empty `name`, never a
/// placeholder or a bare node - as its id, sorted.
pub fn definition_ids(p: &Projector) -> Vec<String> {
    let mut ids: Vec<String> = p
        .whole()
        .unwrap()
        .nodes
        .into_iter()
        .filter(|n| {
            n.kind == KIND_CODE_ENTITY && n.attrs.get("name").is_some_and(|s| !s.is_empty())
        })
        .map(|n| n.id)
        .collect();
    ids.sort();
    ids
}

/// The node `id` of `p`'s whole graph, which must hold it.
pub fn node_of(p: &Projector, id: &str) -> Node {
    let nodes = p.whole().unwrap().nodes;
    let node = nodes.into_iter().find(|n| n.id == id);
    node.unwrap_or_else(|| panic!("{id} is held"))
}

/// The attrs node `id` carries in `p`'s whole graph ([`node_of`]).
pub fn attrs_of(p: &Projector, id: &str) -> BTreeMap<String, String> {
    node_of(p, id).attrs
}

/// The `proven_by` attr node `id` carries in `p`'s whole graph ([`attrs_of`]), `None` when it
/// carries no proof.
pub fn proven_by_attr(p: &Projector, id: &str) -> Option<String> {
    attrs_of(p, id).get("proven_by").cloned()
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
