//! Fold fixtures over the graph projector: folding a hand-built JSON event straight onto a
//! `Projector` (the raw on-log JSON - never an in-crate payload struct - so a test pins the
//! contract the log actually carries), the on-log payloads the decision, alias and code readers
//! take, the canonical spec-53 two-subsystem coupling seed, the live edges, calls and module
//! `CONTAINS` edges a projector serves and the enum rows they are checked against, the
//! read-backs suites take of a folded `Graph`, the two read batches a rebuild source hands with a
//! writer between them, the texts a lost or refused fold answers, the payload no judge accepts
//! with the decision built on it, what a test's rebuild source answers when it stops, the owed
//! mark's path and the note a read-only surface prints while the file owes its rebuild. `apply`
//! returns `Err` on a fold failure, so every helper that folds unwraps it - a successful call is
//! itself evidence the payload folded - but [`fold_of`], which answers what became of a public
//! fold ([`rigger::contextgraph::Fold`]), [`rebuild_answer`], which answers what a rebuild did or
//! its error as text, and the rebuild sources [`one_read`] and [`two_read_batches`], which hand on
//! what their sink answers.

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, UNIX_EPOCH};

use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{
    around, entity_site, site_fields, Candidate, ContextGraph, EntitySite, Graph, Located, Node,
    RecordFold, CODE_KIND_CONSTANT, CODE_KIND_FIELD, CODE_KIND_FUNCTION, CODE_KIND_TYPE,
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

/// A payload no type's judge accepts ([`rigger::contextgraph::check_fold_payload`]): malformed
/// JSON.
pub const POISON: &[u8] = b"{ not valid json";

/// A decision at log position `position`, valid from `position` seconds past the epoch, whose
/// payload is [`POISON`].
pub fn poisoned_decision_at(position: u64) -> Event {
    let poisoned = Event::new(rigger::contextgraph::TYPE_DECISION_MADE, POISON.to_vec());
    placed_at(poisoned, position, position)
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

/// `secs` seconds as nanoseconds.
pub fn nanos(secs: u64) -> i64 {
    Duration::from_secs(secs).as_nanos() as i64
}

/// Everything `p` serves - every node and edge of its whole graph - as one comparable JSON text.
pub fn whole_json(p: &(impl rigger::contextgraph::Neighbors + ?Sized)) -> String {
    serde_json::to_string(&p.whole().unwrap()).unwrap()
}

/// Every live edge `p` serves as `(from, rel, to, valid_from, source)`, sorted.
pub fn live_edges(
    p: &(impl rigger::contextgraph::Neighbors + ?Sized),
) -> Vec<(String, String, String, i64, u64)> {
    let mut edges: Vec<_> = p
        .whole()
        .unwrap()
        .edges
        .into_iter()
        .map(|e| (e.from, e.rel.into(), e.to, e.valid_from, e.source))
        .collect();
    edges.sort();
    edges
}

/// Every live `rel` edge `p` serves as `(from, to)`, sorted.
pub fn live_of(
    p: &(impl rigger::contextgraph::Neighbors + ?Sized),
    rel: &str,
) -> Vec<(String, String)> {
    live_edges(p)
        .into_iter()
        .filter(|(_, edge_rel, ..)| edge_rel == rel)
        .map(|(from, _, to, ..)| (from, to))
        .collect()
}

/// Every live `CALLS` edge `p` serves as `(caller, callee)`, sorted.
pub fn live_calls(p: &(impl rigger::contextgraph::Neighbors + ?Sized)) -> Vec<(String, String)> {
    live_of(p, REL_CALLS)
}

/// Every live `rel` edge `p` serves ([`live_of`]) is exactly `edges`, `(from, to)` ids in that
/// sorted order, `why` naming the rule the edges follow.
pub fn assert_live(
    p: &(impl rigger::contextgraph::Neighbors + ?Sized),
    rel: &str,
    edges: &[(&str, &str)],
    why: &str,
) {
    let edges: Vec<(String, String)> = edges
        .iter()
        .map(|(from, to)| (from.to_string(), to.to_string()))
        .collect();
    assert_eq!(live_of(p, rel), edges, "{why}");
}

/// Every live `CALLS` edge `p` serves ([`live_calls`]) is exactly `calls`, `(caller, callee)`
/// ids in that sorted order, `why` naming the rule the edges follow.
pub fn assert_live_calls(
    p: &(impl rigger::contextgraph::Neighbors + ?Sized),
    calls: &[(&str, &str)],
    why: &str,
) {
    assert_live(p, REL_CALLS, calls, why);
}

/// Every live `REFERENCES` and `CALLS` edge `p` serves, as `(from, rel, to, tier)`, sorted:
/// where each reference landed and at what tier.
pub fn landed(
    p: &(impl rigger::contextgraph::Neighbors + ?Sized),
) -> Vec<(String, String, String, String)> {
    let mut edges: Vec<_> = p
        .whole()
        .unwrap()
        .edges
        .into_iter()
        .filter(|e| e.rel == REL_REFERENCES || e.rel == REL_CALLS)
        .map(|e| (e.from, e.rel.into(), e.to, e.tier.into()))
        .collect();
    edges.sort();
    edges
}

/// Every live module `CONTAINS` edge `p` serves - one onto a `file` node (spec 117, A MODULE ROW
/// REACHES ITS FILE) - as `(from, to, tier)`, sorted.
pub fn module_edges(
    p: &(impl rigger::contextgraph::Neighbors + ?Sized),
) -> Vec<(String, String, String)> {
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
        .map(|e| (e.from.clone(), e.to.clone(), e.tier.to_string()))
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

/// The text of a graph error whose reason is `reason`: the domain error's own display.
pub fn graph_error(reason: &str) -> String {
    rigger::contextgraph::Error(reason.to_string()).to_string()
}

/// What a fold on a surface wired with no context graph answers: the domain's own refusal
/// ([`rigger::contextgraph::wired`]) as the fold displays it.
pub fn unwired() -> String {
    let Err(refused) = rigger::contextgraph::wired(None) else {
        panic!("a surface wired with no graph refuses the fold");
    };
    refused.to_string()
}

/// The text of the graph error a fold meets on `path` when a directory stands where it writes a
/// file.
pub fn directory_refusal(path: impl std::fmt::Display) -> String {
    graph_error(&(path.to_string() + ": Is a directory (os error 21)"))
}

/// What SQLite answers when the file a connection opens holds no database.
pub const NOT_A_DATABASE: &str = "file is not a database";

/// The clause a report line spells for events the graph did not fold, for `why`.
pub fn not_folded(why: &str) -> String {
    ["not folded into the context graph: ", why].concat()
}

/// The suffix a report line ends with when the graph did not fold the event it reports, for
/// `why`: the [`not_folded`] clause after a `; `.
pub fn not_folded_suffix(why: &str) -> String {
    format!("; {}", not_folded(why))
}

/// The line a run surface prints when it recorded `events` run events whose fold the graph did
/// not make, for `reason` - a graph error's reason, its `graph: ` prefix added ([`graph_error`]).
pub fn recorded_not_folded(events: usize, reason: &str) -> String {
    let recorded = format!("rigger: recorded {events} run event(s); ");
    recorded + &not_folded(&graph_error(reason))
}

/// The tail a direct fold's not-folded error ends with, for events all of the project's run
/// stream (spec 124, CAUSE TEXTS): a fold into a file without the `caught_up` cursor, or through a
/// projector handed no log.
pub const SETUP_PAYS: &str =
    " - the next rigger setup finds the event missing from graph.db and folds it";

/// The tail a call-run fold's not-folded error ends with (spec 124, CAUSE TEXTS): a fold of events
/// all of the project's run stream into a file carrying the `caught_up` cursor, through a
/// projector handed the log.
pub const CATCH_UP_PAYS: &str = " - the next fold through the folding store catches it up";

/// The error text of a fold not made for `reason`: the reason, then `pays`, the tail of the
/// fold's path naming what folds the event later ([`SETUP_PAYS`] after a direct fold of the run
/// stream, [`CATCH_UP_PAYS`] after a call).
pub fn lost_fold(reason: &str, pays: &str) -> String {
    [reason, pays].concat()
}

/// The error text of a call-run fold not made for `reason` (spec 124, CAUSE TEXTS): the graph
/// error of `reason` ([`graph_error`]), then [`CATCH_UP_PAYS`] ([`lost_fold`]).
pub fn lost_by_calls(reason: &str) -> String {
    lost_fold(&graph_error(reason), CATCH_UP_PAYS)
}

/// The cause a fold answers past its ceiling `held` while another writer holds the write lock
/// (spec 124, CAUSE TEXTS), `held` as the ceiling reads.
pub fn lock_held_past(held: &str) -> String {
    format!("graph.db's write lock stayed held past {held}")
}

/// The cause a call-run fold answers past its ceiling `held` after a call that advanced short of
/// its position or met the log's head below it (spec 124, CAUSE TEXTS), `held` as the ceiling
/// reads.
pub fn catch_up_short_after(held: &str) -> String {
    format!("graph.db's catch-up had not reached this event after {held}")
}

/// The cause a call-run fold answers while the file holds a rebuild's state (spec 124, CAUSE
/// TEXTS).
pub const UNFINISHED_REBUILD: &str = "graph.db holds an unfinished rebuild";

/// What a test's rebuild source answers when it stops a rebuild mid-read.
pub const SOURCE_INTERRUPTED: &str = "interrupted";

/// A test's rebuild source, or the sink a source hands to, stopping a rebuild mid-read
/// ([`SOURCE_INTERRUPTED`]).
pub fn source_interrupted<T>() -> Result<T, rigger::contextgraph::Error> {
    Err(rigger::contextgraph::Error(SOURCE_INTERRUPTED.to_string()))
}

/// The owed mark of the graph file `graph.db` in the directory `dir`: the file an older binary's
/// lost fold left beside it, no cause of its rebuild (spec 124, THE MARK).
pub fn owed_mark_in(dir: &std::path::Path) -> std::path::PathBuf {
    dir.join("graph.db.owed")
}

/// Write the owed mark of the graph file `graph.db` in the directory `dir` ([`owed_mark_in`]),
/// empty, as a fold that lost its event once left it, and answer its path: no fold writes it now
/// (spec 124, AN APPENDER WAITS UNDER THE CEILING AND NEVER MARKS).
pub fn mark_owed_in(dir: &std::path::Path) -> std::path::PathBuf {
    let mark = owed_mark_in(dir);
    std::fs::write(&mark, b"").unwrap();
    mark
}

/// Stand a directory at the path of the owed mark of the graph file `graph_db` ([`owed_mark_in`]),
/// a mark no removal can remove - its removal is refused naming it ([`directory_refusal`]) - and
/// answer its path.
pub fn unremovable_mark_beside(graph_db: &std::path::Path) -> std::path::PathBuf {
    let mark = owed_mark_in(graph_db.parent().unwrap());
    std::fs::create_dir(&mark).unwrap();
    mark
}

/// Whether the owed mark of the graph file `graph_db` ([`owed_mark_in`]) stands beside it.
pub fn mark_stands_beside(graph_db: &std::path::Path) -> bool {
    owed_mark_in(graph_db.parent().unwrap()).exists()
}

/// The one note every read-only surface prints while `graph.db` owes its rebuild.
pub fn rebuild_owed_note() -> String {
    format!(
        "note: {} - until then the context graph answers as it stands",
        rigger::contextgraph::REBUILD_OWED
    )
}

/// What the public fold answers on a graph that owes its rebuild: refused, naming the debt.
pub fn owed_refusal() -> rigger::contextgraph::Fold {
    rigger::contextgraph::Fold::NotFolded(graph_error(rigger::contextgraph::REBUILD_OWED))
}

/// What the emit tool answers for the event it appended at `position`: folded, or - given the
/// `reason` its fold failed for - not folded, and why.
pub fn emit_answer(position: u64, reason: Option<&str>) -> serde_json::Value {
    match reason {
        None => serde_json::json!({ "position": position, "folded": true }),
        Some(reason) => {
            serde_json::json!({ "position": position, "folded": false, "reason": reason })
        }
    }
}

/// Fold `events` into `p` through the public fold, answering what became of them.
pub fn fold_of(p: &dyn RecordFold, events: &[Event]) -> rigger::contextgraph::Fold {
    rigger::contextgraph::Fold::of_batch(|| rigger::contextgraph::wired(Some(p)), events)
}

/// What a rebuild that ran answers ([`rebuild_answer`]) when it pruned nothing.
pub fn clean_rebuild() -> Result<Option<rigger::contextgraph::sqlite::Rebuilt>, String> {
    Ok(Some(rigger::contextgraph::sqlite::Rebuilt::default()))
}

/// Rebuild the graph file `graph_db` under the project `test` from `source`, owed when `owed` says
/// so, reporting no progress, and answer what the rebuild did, its error as text.
pub fn rebuild_answer(
    graph_db: &str,
    owed: bool,
    source: &mut rigger::contextgraph::sqlite::RebuildSource,
) -> Result<Option<rigger::contextgraph::sqlite::Rebuilt>, String> {
    Projector::open(graph_db, "test")
        .unwrap()
        .rebuild(
            &Projector::lock_rebuild(graph_db).unwrap(),
            owed,
            source,
            &mut |_| {},
        )
        .map_err(|e| e.0)
}

/// Fold `events` into `p` through the public fold ([`fold_of`]), and insist they landed: the one
/// spelling of seeding a graph a test then reads.
pub fn folds(p: &dyn RecordFold, events: &[Event]) {
    assert_eq!(fold_of(p, events), rigger::contextgraph::Fold::Folded);
}

/// Fold each of `events` into `p` alone, in order, each in a transaction of its own ([`folds`]).
pub fn folds_each(p: &dyn RecordFold, events: &[Event]) {
    for e in events {
        folds(p, std::slice::from_ref(e));
    }
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

/// The replay key of event `index` of generation `generation` of `file`'s code batch, under the
/// file's identity `gc/<file>`.
pub fn code_batch_key(file: &str, generation: &str, index: usize) -> String {
    format!("gc/{file}@{generation}#{index}")
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

/// The enum `E` of `a.rs` (spec 119, CONTAINS IS STRUCTURE).
pub const ENUM_E: &str = "a.rs::E";
/// The variant `A` of [`ENUM_E`].
pub const VARIANT_E_A: &str = "a.rs::E::A";
/// The variant `B` of [`ENUM_E`].
pub const VARIANT_E_B: &str = "a.rs::E::B";
/// The live `CONTAINS` edges, `(parent, child)` in sorted order, of `a.rs` defining [`ENUM_E`]
/// with its one variant [`VARIANT_E_A`].
pub const E_HOLDS_A: &[(&str, &str)] = &[("a.rs", ENUM_E), (ENUM_E, VARIANT_E_A)];

/// The candidates `(id, file)` a lookup of `a.rs`'s `new` lists over [`T_NEW`] and [`U_NEW`], in
/// id order.
pub const A_RS_NEWS: &[(&str, &str)] = &[("a.rs::T::new", "a.rs"), ("a.rs::U::new", "a.rs")];

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

/// The `CodeEntityExtracted` payload of one extraction batch's Rust function definition. `fresh`
/// marks the FIRST event of a file's batch: the fold supersedes the file's prior structural edges
/// before folding it, so a later batch replaces rather than accretes.
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

/// `payload` marked the head of its file's batch when `fresh`, and not when not: the flag on which
/// the fold supersedes what the file's prior batch recorded.
pub fn marked_fresh(mut payload: serde_json::Value, fresh: bool) -> serde_json::Value {
    payload["fresh"] = fresh.into();
    payload
}

/// `tests/t.rs`'s test reference to `name` at `line` heading its batch ([`marked_fresh`]), as
/// event 0 of its generation `k1` (replay-keyed `gc/tests/t.rs@k1#0`), valid from `secs`, at no
/// log position yet ([`t_rs_batch_reference`]).
pub fn fresh_test_reference(name: &str, line: u32, secs: u64) -> Event {
    t_rs_batch_reference(0, "k1", 0, name, line, secs)
}

/// `tests/t.rs`'s test reference to `name` at `line` at log position `pos`: event `index` of
/// generation `generation`'s batch ([`code_batch_key`]), its head ([`test_reference_json`]) when
/// `index` is 0, valid from `secs` ([`keyed_event`]).
pub fn t_rs_batch_reference(
    pos: u64,
    generation: &str,
    index: usize,
    name: &str,
    line: u32,
    secs: u64,
) -> Event {
    let payload = test_reference_json("tests/t.rs", name, line, index == 0);
    let key = code_batch_key("tests/t.rs", generation, index);
    keyed_event(pos, TYPE_EDGE_INFERRED, payload, &key, secs)
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

/// The test-origin `EdgeInferred` payload: `file` references `name` at line `line` from a test,
/// the evidence a test's reference to a definition records.
pub fn edge_inferred_evidence_json(file: &str, name: &str, line: u32) -> serde_json::Value {
    serde_json::json!({ "file": file, "name": name, "lang": "rust", "line": line, "is_test": true })
}

/// The test-origin reference of `file` to `name` at line `line` ([`edge_inferred_evidence_json`]),
/// marked the head of its file's batch when `fresh`, and not when not ([`marked_fresh`]).
pub fn test_reference_json(file: &str, name: &str, line: u32, fresh: bool) -> serde_json::Value {
    marked_fresh(edge_inferred_evidence_json(file, name, line), fresh)
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

/// The two-subsystem coupling graph written as Rust sources, `(path, text)`: each definition of
/// [`TWO_SUBSYSTEM_DEFS`] is a function in its file calling the callees [`TWO_SUBSYSTEM_CALLS`]
/// names for it.
pub fn two_subsystem_sources() -> Vec<(String, String)> {
    let mut files: BTreeMap<&str, String> = BTreeMap::new();
    for (file, name) in TWO_SUBSYSTEM_DEFS {
        let calls: String = TWO_SUBSYSTEM_CALLS
            .iter()
            .filter(|(at, _, caller)| *at == file && *caller == name)
            .map(|(_, callee, _)| format!(" {callee}();"))
            .collect();
        files
            .entry(file)
            .or_default()
            .push_str(&format!("pub fn {name}() {{{calls} }}\n"));
    }
    files
        .into_iter()
        .map(|(file, text)| (file.to_string(), text))
        .collect()
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
pub fn definition_ids(p: &(impl rigger::contextgraph::Neighbors + ?Sized)) -> Vec<String> {
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
pub fn node_of(p: &(impl rigger::contextgraph::Neighbors + ?Sized), id: &str) -> Node {
    let nodes = p.whole().unwrap().nodes;
    let node = nodes.into_iter().find(|n| n.id == id);
    node.unwrap_or_else(|| panic!("{id} is held"))
}

/// The attrs node `id` carries in `p`'s whole graph ([`node_of`]).
pub fn attrs_of(
    p: &(impl rigger::contextgraph::Neighbors + ?Sized),
    id: &str,
) -> BTreeMap<String, String> {
    node_of(p, id).attrs
}

/// The `proven_by` attr node `id` carries in `p`'s whole graph ([`attrs_of`]), `None` when it
/// carries no proof.
pub fn proven_by_attr(
    p: &(impl rigger::contextgraph::Neighbors + ?Sized),
    id: &str,
) -> Option<String> {
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
