//! Project-source ingest into the context graph: the ONE walk-and-content-key authority both
//! the live run (`conductor::RunCtx::ingest_project_batches`) and the standalone
//! `rigger graph build` entry share, so the content key an event is deduped under can never
//! drift between the two ingest entries.
//!
//! Each caller supplies its OWN emit sink - the run's replay-keyed, concurrency-safe
//! `emit_keyed`; the cold build's log-seeded seen-set plus a direct append-and-fold - because
//! their mutation semantics legitimately differ. What must NOT fork is the drift-prone part:
//! the walk over the project's per-file extraction batches, the `<prefix>/<file>@<hash>#<i>`
//! content key, and the predicate that decides which recorded keys a fresh emit is redundant
//! against ([`project_scoped_replay_keys`]). Those are derived here, once, so the run and a cold
//! `graph build` agree on every key and never double-ingest one another's work.
//!
//! Symbols-gated: the walk lowers the tree through the `symbols` extraction pass, so the light
//! lane has nothing to ingest - a no-op that emits nothing, exactly as the run's ingest is a
//! no-op there.

use crate::contextgraph::Projection;
use crate::eventstore::{Event, EventStore, ExpectedRevision, Position};

/// Append a whole batch of events to `stream` in ONE store append and fold them into `graph` in ONE
/// transaction (via [`Projection::apply_batch`]) - the batched-fold cadence spec 49 needs: one store
/// transaction per file's batch, not one per event (the measured cold-build throughput was
/// transaction-cadence bound, not parse-bound). Each event is stamped with its global position
/// before it folds: a single append lands the batch at CONSECUTIVE positions ending at the returned
/// last position, so event `i` of an `n`-event batch sits at `last - (n - 1) + i`. The fold is
/// best-effort - a fold failure never fails the append, which already landed durably in the log,
/// exactly as the run's per-event `append_and_fold` folds best-effort. Returns the last appended
/// position (`0` for an empty batch, which appends nothing).
///
/// This is the ONE batched append-and-fold authority both ingest sinks share - the run's keyed emit
/// and a cold `rigger graph build` - so the batching can never diverge between them. It lives here
/// beside the walk-and-key authority rather than inside either sink, and it is deliberately NOT
/// `symbols`-gated: it only moves events through the store and graph ports, which both feature lanes
/// compile, so the run's single-event mutation path can route its one-event case through it in
/// either lane.
pub fn append_and_fold_batch(
    store: &dyn EventStore,
    graph: Option<&dyn Projection>,
    stream: &str,
    events: &[Event],
) -> Result<Position, crate::eventstore::Error> {
    if events.is_empty() {
        return Ok(0);
    }
    let last = store.append(stream, ExpectedRevision::Any, events)?;
    if let Some(g) = graph {
        let n = events.len() as Position;
        let base = last + 1 - n;
        let positioned: Vec<Event> = events
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let mut e = e.clone();
                e.position = base + i as Position;
                e
            })
            .collect();
        let _ = g.apply_batch(&positioned);
    }
    Ok(last)
}

/// What a walk did, reported back to the caller. `batches_emitted` counts the file batches the walk
/// handed to `emit` (code and design). `workers_engaged` is how many parse-worker threads actually
/// ran the code half: `> 1` proves the parse fanned across cores, and it is an HONEST count (the
/// distinct threads that ran), so a serial walk (width 1) reports exactly 1.
#[cfg(feature = "symbols")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IngestStats {
    pub batches_emitted: usize,
    pub workers_engaged: usize,
}

/// Walk the project tree at `root` and, for every extraction event the code (spec 29a) and
/// design (spec 29b) passes emit, call `emit(key, event)` with the event's deterministic content
/// key `<prefix>/<file>@<hash>#<i>` (`gc` for code, `gd` for design). The key is a pure function
/// of the batch's bytes ALONE, so the same content always yields the same keys and different
/// content always yields different ones. A key is therefore a CONTENT GENERATION of a file, not a
/// mark that the file has been seen: whether a given key is redundant is a question about the
/// file's LATEST recorded generation ([`project_scoped_replay_keys`] answers it), which is why a
/// file reverted to content it held earlier re-emits its whole batch even though every one of its
/// keys is already in the log. This function owns only the walk and the keying; the sink decides
/// what a key MEANS (append-and-fold, or skip a replay), so the mutation authority stays with the
/// caller.
///
/// The code half's per-file parse/lower fans across a default-sized worker pool (one worker per
/// logical core), but the EMIT stays in sorted file-path order - parallelism is observationally
/// invisible. The returned [`IngestStats`] is informational; existing callers discard it and are
/// unaffected.
#[cfg(feature = "symbols")]
pub fn ingest_project(root: &str, emit: impl FnMut(&str, &Event)) -> IngestStats {
    ingest_project_paced(root, crate::parallel::default_workers(), emit)
}

/// [`ingest_project`] at a chosen parse width, emitting one event at a time. The code half (spec
/// 29a) parses/lowers its files across up to `workers` threads yet EMITS them in the index's sorted
/// file-path order, so the event sequence a caller's sink observes is byte-identical to a serial
/// walk's regardless of scheduling (the rebuild-byte-identical discipline). `workers <= 1` runs the
/// lowering inline: it IS the serial walk a wider walk is proven byte-identical against - the same
/// code path, not a hand-rolled twin. The design half (spec 29b) stays serial; its walk lives in
/// `design/events.rs`, whose scoping a separate unit owns, so parallelizing it here would fork that
/// file.
///
/// This is the per-EVENT view of the one walk: it FLATTENS each file's keyed batch into one
/// `emit(key, event)` per event, in `#i` order, so it is a thin adapter over
/// [`ingest_project_batched_paced`] - not a second walk. A sink that appends and folds a file's
/// whole batch as a UNIT (the batched-fold cadence, spec 49) uses the batched entry instead.
#[cfg(feature = "symbols")]
pub fn ingest_project_paced(
    root: &str,
    workers: usize,
    mut emit: impl FnMut(&str, &Event),
) -> IngestStats {
    walk_batches(root, workers, |keyed| {
        for (key, ev) in keyed {
            emit(key, ev);
        }
    })
}

/// [`ingest_project`] handing a sink each file's WHOLE keyed batch at once (the whole file's events,
/// each paired with its `<prefix>/<file>@<hash>#<i>` content key, in `#i` order) rather than one
/// event at a time. A sink appends the file's batch in ONE store append and folds it in ONE graph
/// transaction (via [`append_and_fold_batch`]) - the batched-fold cadence spec 49 needs, since the
/// measured cold-build throughput was transaction-cadence bound. Same default parse width as
/// [`ingest_project`]; the per-event walk is this same core flattened.
#[cfg(feature = "symbols")]
pub fn ingest_project_batched(
    root: &str,
    on_batch: impl FnMut(&[(String, &Event)]),
) -> IngestStats {
    ingest_project_batched_paced(root, crate::parallel::default_workers(), on_batch)
}

/// [`ingest_project_batched`] at a chosen parse width - the batched analogue of
/// [`ingest_project_paced`]. Parse width changes only the code half's parallelism (criterion 1),
/// never the batching: the same files still emit as the same per-file batches, in sorted file-path
/// order.
#[cfg(feature = "symbols")]
pub fn ingest_project_batched_paced(
    root: &str,
    workers: usize,
    on_batch: impl FnMut(&[(String, &Event)]),
) -> IngestStats {
    walk_batches(root, workers, on_batch)
}

/// The ONE walk both public views share: parse/lower the project at `root` and hand each file's
/// WHOLE keyed batch to `on_batch`, in sorted file-path order (the code half first, then the design
/// half), each batch in `#i` order. The per-event [`ingest_project_paced`] and the per-batch
/// [`ingest_project_batched_paced`] are both thin views over this, so there is no forked walk to
/// drift - the emit order is defined once, here.
#[cfg(feature = "symbols")]
fn walk_batches(
    root: &str,
    workers: usize,
    mut on_batch: impl FnMut(&[(String, &Event)]),
) -> IngestStats {
    let mut batches_emitted = 0usize;
    // The code half (spec 29a): parallel parse feeds this ordered emit. Reuses the `symbols`
    // grounder's persisted index when present (no re-parse), so in a live run this is a cheap read of
    // what the grounder already built - not a second whole-tree parse.
    let (code_batches, workers_engaged) =
        crate::grounder::symbols::events::project_batches_paced(root, workers);
    for (file, batch) in &code_batches {
        key_batch("gc", file, batch, &mut on_batch);
        batches_emitted += 1;
    }
    // The design half (spec 29b): the project's design docs and inline source rationale, serial.
    let design_batches = crate::grounder::design::events::project_batches(root);
    for (file, batch) in &design_batches {
        key_batch("gd", file, batch, &mut on_batch);
        batches_emitted += 1;
    }
    IngestStats {
        batches_emitted,
        workers_engaged,
    }
}

/// Key one file's batch under `<prefix>/<file>@<hash>#<i>` and hand the WHOLE keyed batch to
/// `on_batch` at once. `hash` fingerprints the WHOLE batch's bytes with the SAME line-ending-
/// normalized content primitive the symbols reindex freshening keys on (reused, not a fresh copy, so
/// the change-detection key is one content-identity authority), so every event of a file shares one
/// `<hash>`. The batch bytes are JSON the emit pass just serialized, so they are valid UTF-8.
#[cfg(feature = "symbols")]
fn key_batch(
    prefix: &str,
    file: &str,
    batch: &[Event],
    on_batch: &mut impl FnMut(&[(String, &Event)]),
) {
    let concat: String = batch
        .iter()
        .filter_map(|e| std::str::from_utf8(&e.data).ok())
        .collect();
    let hash = crate::grounder::symbols::store::content_hash(&concat);
    let keyed: Vec<(String, &Event)> = batch
        .iter()
        .enumerate()
        .map(|(i, ev)| (format!("{prefix}/{file}@{hash}#{i}"), ev))
        .collect();
    on_batch(&keyed);
}

/// The metadata key under which an event carries its deterministic REPLAY KEY (spec 04, criterion
/// 4): the name a content key is STAMPED under and read back from, so this module owns the wire
/// form of its key as well as its format.
///
/// The key itself is a pure function of what it identifies - for a derived index event, the batch's
/// own bytes; for a run's lifecycle events, the run structure (unit id, phase or gate token,
/// remediation attempt) - never wall clock or randomness. An event stamped with one is appended AT
/// MOST ONCE against whatever key set its sink seeds from, so two processes computing the identical
/// key for the identical event let the second recognize the first's as a replay. Folds and
/// projections ignore it, like [`crate::contextgraph::META_ACTOR`].
///
/// It is DEFINED HERE, beside [`key_batch`] which builds the `<prefix>/<file>@<hash>#<i>` form and
/// [`project_scoped_replay_keys`] which parses it back, rather than in the orchestrator that also
/// stamps it. That predicate is the shared suppression authority BOTH a live run and a cold
/// `rigger graph build` call, so reading the name out of `crate::conductor` would point this module
/// UP at the orchestrator and couple every future caller of the predicate to it for a wire-format
/// fact the orchestrator does not own. `conductor::META_REPLAY_KEY` re-exports this constant, so
/// there is exactly one name and no second spelling to drift.
pub const META_REPLAY_KEY: &str = "replay_key";

/// The DERIVED INDEX event types: the re-derivable projection of the project's own sources that
/// [`key_batch`] above keys, and the ONLY types eligible for project-scoped suppression.
///
/// This is a code-owned discriminator, not a string convention, and it is what makes the
/// fail-safe direction a property of the code: an event of any OTHER type never reaches the key
/// comparison below, so no domain event can be dropped by that path however its replay key
/// happens to look. Domain events legitimately repeat (two identical review findings mean the
/// finding was raised twice); these four do not - a file's content hash does not change because a
/// new run started, so re-recording an unchanged file's batch records nothing new.
pub const DERIVED_INDEX_TYPES: [&str; 4] = [
    crate::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
    crate::contextgraph::TYPE_EDGE_INFERRED,
    crate::contextgraph::TYPE_DOC_CONCEPT_EXTRACTED,
    crate::contextgraph::TYPE_DOC_LINK_EXTRACTED,
];

/// Whether `type_` is one of the four [`DERIVED_INDEX_TYPES`] - the TYPE half of the partition,
/// asked FIRST, before any key is looked at.
pub fn is_derived_index_type(type_: &str) -> bool {
    DERIVED_INDEX_TYPES.contains(&type_)
}

/// Split a derived-index content key `<prefix>/<file>@<hash>#<i>` into
/// `(<prefix>/<file>, <hash>)`: the BATCH IDENTITY (which file's batch this is) and the CONTENT
/// GENERATION of that batch. `None` when the key is not that shape.
///
/// The identity deliberately carries the `<prefix>` segment, so one file's code (`gc`) and design
/// (`gd`) batches are two independent identities that never overwrite each other's generation -
/// and it is read as the WHOLE key's leading span, never by sniffing the prefix's VALUE.
/// [`key_batch`] takes its prefix from the CALLER, so a value sniff would rest on an unenforced
/// cross-module naming habit; the shape (a `/`, an `@`, and a `#<digits>` tail) is what the key
/// authority actually guarantees. `<file>` may itself contain `/`, `@` or `#`, so the tail and the
/// hash are split from the RIGHT.
fn derived_key_parts(key: &str) -> Option<(&str, &str)> {
    let (prefix, remainder) = key.split_once('/')?;
    if prefix.is_empty() {
        return None;
    }
    let (head, index) = remainder.rsplit_once('#')?;
    if index.is_empty() || !index.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let (file, hash) = head.rsplit_once('@')?;
    if file.is_empty() || hash.is_empty() {
        return None;
    }
    // `key` is `<prefix>` + '/' + `<file>` + '@' + `<hash>` + '#' + `<i>`, so the identity is the
    // key's leading `prefix.len() + 1 + file.len()` bytes - all three substrings borrow `key`.
    Some((&key[..prefix.len() + 1 + file.len()], hash))
}

/// The ONE project-scoped suppression predicate, expressed as the set of replay keys a derived-index
/// emit may be suppressed against, derived from the WHOLE prior stream.
///
/// Both ingest sinks - the run's keyed emit and a cold `rigger graph build` - seed from this and
/// neither copies it, because the `<prefix>/<file>@<hash>#<i>` format is built by [`key_batch`]
/// here and must not fork. The rule, in the order it is applied:
///
/// 1. **Type first.** Only the four [`DERIVED_INDEX_TYPES`] are eligible. Every other event is
///    passed over whatever its replay key looks like, so a unit or stage whose id happened to read
///    like an ingest prefix could never have its lifecycle key mistaken for a project fact.
/// 2. **Then the whole key.** A derived event's key is parsed for its batch identity and content
///    generation ([`derived_key_parts`]); a key that is not that shape names no generation and is
///    passed over (the fail-safe direction - it re-emits).
/// 3. **Latest per file, never ever-recorded.** Only the keys of each identity's LATEST recorded
///    generation are returned. A file's earlier generations are deliberately absent: content
///    REVERTED to a generation the file has since moved past differs from its latest recorded
///    batch, so it must re-emit and supersede the newer structural edges. An ever-recorded key set
///    would match the old records, re-emit nothing, and strand the graph on a superseded version of
///    that file forever.
///
/// This is project-scoped ON PURPOSE: derived index facts are facts about the project's files, not
/// about a run, so a NEW run inherits them and an unchanged file appends nothing on every
/// subsequent run forever. Run-scoped seeding stays exactly as it is for every other replay key
/// (unit lifecycle, gate verdicts, breaker trips), whose recurrence IS a property of one run.
pub fn project_scoped_replay_keys(prior: &[Event]) -> std::collections::HashSet<String> {
    // identity -> (that identity's latest recorded generation, the keys of that generation)
    let mut latest: std::collections::HashMap<String, (String, Vec<String>)> =
        std::collections::HashMap::new();
    for e in prior {
        // TYPE first: a non-derived event never reaches the key comparison at all.
        if !is_derived_index_type(&e.type_) {
            continue;
        }
        let Some(key) = e.meta.get(META_REPLAY_KEY) else {
            continue;
        };
        let Some((identity, hash)) = derived_key_parts(key) else {
            continue;
        };
        let slot = latest
            .entry(identity.to_string())
            .or_insert_with(|| (hash.to_string(), Vec::new()));
        // A later generation of the same file RETIRES the keys of every earlier one: the stream is
        // read in append order, so the last generation seen is the file's latest recorded batch.
        if slot.0 != hash {
            slot.0 = hash.to_string();
            slot.1.clear();
        }
        slot.1.push(key.clone());
    }
    latest.into_values().flat_map(|(_, keys)| keys).collect()
}

/// The light lane compiles no extraction pass, so there is nothing to walk - a no-op that emits
/// nothing. `graph build` still opens (creating) the store and degrades to an empty graph, never
/// an error, exactly as the run's ingest is a no-op here.
#[cfg(not(feature = "symbols"))]
pub fn ingest_project(_root: &str, _emit: impl FnMut(&str, &crate::eventstore::Event)) {}

/// The light lane's batched entry: no extraction pass, so nothing to walk - a no-op that hands the
/// sink no batches. Mirrors the light-lane [`ingest_project`], so a cold `graph build` degrades to
/// an empty graph in either lane (the batched append-and-fold kernel above stays compiled in both).
#[cfg(not(feature = "symbols"))]
pub fn ingest_project_batched(
    _root: &str,
    _on_batch: impl FnMut(&[(String, &crate::eventstore::Event)]),
) {
}

/// The suppression predicate's OWN contract, at the unit level: which recorded keys it hands a
/// sink, given a stream. Both lanes compile it, because the predicate is not `symbols`-gated - it
/// reads recorded events, it does not walk a tree. The SEAM-level proofs live with their criteria:
/// that a prior run's non-ingest key never suppresses this run's keyed emit, and that a reverted
/// file survives the full suppression stack, are pinned at the conductor's seeding seam and over a
/// cold rebuild respectively, not here.
#[cfg(test)]
mod dedup_tests {
    use super::{derived_key_parts, project_scoped_replay_keys, META_REPLAY_KEY};
    use crate::contextgraph::{
        TYPE_CODE_ENTITY_EXTRACTED, TYPE_DOC_CONCEPT_EXTRACTED, TYPE_EDGE_INFERRED,
        TYPE_REVIEW_FINDING,
    };
    use crate::eventstore::Event;
    use std::collections::HashSet;

    fn keyed(type_: &str, key: &str) -> Event {
        Event::new(type_, Vec::new()).with_meta(META_REPLAY_KEY, key)
    }

    #[test]
    fn the_suppression_predicate_is_type_first_whole_key_and_latest_generation_only() {
        let stream = vec![
            // A file's code batch, then the SAME file's design batch: two independent identities,
            // because the identity carries the `<prefix>` segment.
            keyed(TYPE_CODE_ENTITY_EXTRACTED, "gc/src/a.rs@h1#0"),
            keyed(TYPE_EDGE_INFERRED, "gc/src/a.rs@h1#1"),
            keyed(TYPE_DOC_CONCEPT_EXTRACTED, "gd/src/a.rs@h1#0"),
            // A DOMAIN event whose replay key is spelled exactly like a content key. Type first:
            // it is never eligible, so no domain event can be dropped by this path.
            keyed(TYPE_REVIEW_FINDING, "gc/src/b.rs@h1#0"),
            // A derived event whose key is NOT the content-key shape names no generation, so it
            // suppresses nothing (the fail-safe direction: it re-emits).
            keyed(TYPE_CODE_ENTITY_EXTRACTED, "gc/src/c.rs"),
            // A file path containing both `@` and `#`: the tail and the hash split from the RIGHT,
            // so the identity is still the whole `<prefix>/<file>` span.
            keyed(TYPE_CODE_ENTITY_EXTRACTED, "gc/we@ird#1/x.rs@h3#0"),
            // A LATER generation of the first file's code batch retires its earlier keys.
            keyed(TYPE_CODE_ENTITY_EXTRACTED, "gc/src/a.rs@h2#0"),
            // A derived event carrying no replay key at all is simply passed over.
            Event::new(TYPE_CODE_ENTITY_EXTRACTED, Vec::new()),
        ];

        let keys = project_scoped_replay_keys(&stream);

        assert_eq!(
            keys,
            HashSet::from([
                "gc/src/a.rs@h2#0".to_string(),
                "gd/src/a.rs@h1#0".to_string(),
                "gc/we@ird#1/x.rs@h3#0".to_string(),
            ]),
            "only the LATEST generation of each derived-type, well-formed identity is returned"
        );
        assert!(
            !keys.contains("gc/src/a.rs@h1#0") && !keys.contains("gc/src/a.rs@h1#1"),
            "a superseded generation's keys must NOT suppress - that is what makes a revert re-emit"
        );
        assert!(
            !keys.contains("gc/src/b.rs@h1#0"),
            "a domain event is ineligible however its replay key is spelled"
        );
        assert!(
            project_scoped_replay_keys(&[]).is_empty(),
            "an empty stream suppresses nothing"
        );
    }

    #[test]
    fn two_files_whose_paths_contain_an_at_sign_stay_two_batch_identities() {
        // The identity/generation split direction is load-bearing and rigger is project-agnostic:
        // `@` is an ordinary character in a real path (a vendored `pkg@1.2.3/` directory, a scoped
        // package folder), and only `key_batch`'s OWN trailing `@<hash>` separates identity from
        // generation. Splitting from the LEFT instead would cut both keys below at their FIRST `@`,
        // collapsing two different files onto the single identity `gc/vendor/pkg` - and since their
        // (mis-parsed) generations then differ, the later file would RETIRE the earlier file's keys
        // and strip them from the suppression set. Two unrelated files must never share one batch
        // identity, whatever their paths spell.
        let stream = vec![
            keyed(TYPE_CODE_ENTITY_EXTRACTED, "gc/vendor/pkg@1.2.3/a.rs@h1#0"),
            keyed(TYPE_EDGE_INFERRED, "gc/vendor/pkg@1.2.3/a.rs@h1#1"),
            keyed(TYPE_CODE_ENTITY_EXTRACTED, "gc/vendor/pkg@4.5.6/b.rs@h2#0"),
        ];

        assert_eq!(
            project_scoped_replay_keys(&stream),
            HashSet::from([
                "gc/vendor/pkg@1.2.3/a.rs@h1#0".to_string(),
                "gc/vendor/pkg@1.2.3/a.rs@h1#1".to_string(),
                "gc/vendor/pkg@4.5.6/b.rs@h2#0".to_string(),
            ]),
            "every file's own keys survive: no file's batch may retire another file's"
        );

        // The same rule at the parser: the identity is the WHOLE `<prefix>/<file>` span, so every
        // `@` and `#` inside the path belongs to the file, never to the generation or the index.
        assert_eq!(
            derived_key_parts("gc/vendor/pkg@1.2.3/a.rs@h1#0"),
            Some(("gc/vendor/pkg@1.2.3/a.rs", "h1")),
            "identity and generation split from the RIGHT, at the key authority's own separators"
        );
        assert_eq!(
            derived_key_parts("gd/a#1/b@2/c.md@deadbeef#12"),
            Some(("gd/a#1/b@2/c.md", "deadbeef")),
            "a path carrying both `@` and `#` still yields the whole path as the identity"
        );
    }

    #[test]
    fn a_key_that_is_not_the_content_key_shape_names_no_generation() {
        // Rule 2 of the predicate's contract, and the fail-safe direction it encodes: a derived
        // event whose replay key is not `<prefix>/<file>@<hash>#<i>` names no generation, so it is
        // passed over and its emit is NEVER suppressed. The rows below cover EVERY reject arm the
        // parser has, so deleting any ARM fails this test rather than surviving it. They are NOT
        // one row per arm: a guard testing two conditions needs a row per condition, and two
        // differently-shaped keys can land on the same arm.
        for key in [
            "gcsrca.rs@h1#0",    // no `/` anywhere: the prefix split itself finds no separator
            "gcsrc/a.rs@h1",     // `gcsrc` parses AS the prefix, so this rejects at the `#` tail
            "/src/a.rs@h1#0",    // empty prefix
            "gc/src/a.rs@h1",    // no `#<i>` tail
            "gc/src/a.rs@h1#",   // empty index
            "gc/src/a.rs@h1#0a", // non-digit in the index
            "gc/src/a.rs@h1#-1", // ditto: a sign is not a digit
            "gc/src/a.rs#0",     // no `@`: nothing separates the generation
            "gc/@h1#0",          // empty file
            "gc/src/a.rs@#0",    // empty generation
        ] {
            assert_eq!(
                derived_key_parts(key),
                None,
                "{key:?} is not the content-key shape, so it names no batch identity"
            );
            assert!(
                project_scoped_replay_keys(&[keyed(TYPE_CODE_ENTITY_EXTRACTED, key)]).is_empty(),
                "{key:?} must suppress nothing - a key we cannot parse re-emits (fail-safe)"
            );
        }

        // The positive control, so the table proves a REJECT rather than a parser that says no to
        // everything: the well-formed shape still yields its identity and generation.
        assert_eq!(
            derived_key_parts("gc/src/a.rs@h1#0"),
            Some(("gc/src/a.rs", "h1")),
            "the well-formed content key still parses"
        );
    }
}

#[cfg(all(test, feature = "symbols"))]
mod tests {
    use super::{ingest_project_paced, IngestStats};
    use crate::eventstore::Event;

    /// Drive a walk at `workers` width and capture the exact `(key, type, data)` triples the sink
    /// sees, in emit order - the observable the byte-identical contract is defined over.
    fn walk(root: &str, workers: usize) -> (Vec<(String, String, Vec<u8>)>, IngestStats) {
        let mut seq: Vec<(String, String, Vec<u8>)> = Vec::new();
        let stats = ingest_project_paced(root, workers, |key, ev: &Event| {
            seq.push((key.to_string(), ev.type_.clone(), ev.data.clone()));
        });
        (seq, stats)
    }

    #[test]
    fn parallel_parse_emits_the_byte_identical_sequence_a_serial_walk_would() {
        // Criterion 1: ingesting a multi-file fixture at width 8 (parallel parse) must engage more
        // than one parse worker AND emit the SAME event sequence - same keys, types, and bytes, in
        // the SAME sorted-file-path order - that a serial walk (width 1) emits. The serial walk is
        // the SAME production function at width 1 (`map_ordered` short-circuits to an inline map), so
        // determinism is proven against the one code path, not a hand-rolled twin.
        let dir = tempfile::tempdir().unwrap();
        for i in 0..6 {
            std::fs::write(
                dir.path().join(format!("m{i}.rs")),
                format!("fn f{i}() {{}}\nfn g{i}() {{ f{i}(); }}\n"),
            )
            .unwrap();
        }
        let root = dir.path().to_str().unwrap();

        let (serial, serial_stats) = walk(root, 1);
        let (parallel, parallel_stats) = walk(root, 8);

        assert!(
            !serial.is_empty(),
            "the multi-file fixture emits code-ingest events"
        );
        assert_eq!(
            serial, parallel,
            "parallel parse feeds an ordered emit: the width-8 sequence is byte-identical to serial"
        );

        // Sorted file-path order: the code (`gc/`) keys name files in ascending path order.
        let gc_files: Vec<&str> = parallel
            .iter()
            .filter_map(|(k, _, _)| k.strip_prefix("gc/"))
            .filter_map(|rest| rest.split('@').next())
            .collect();
        let mut sorted = gc_files.clone();
        sorted.sort_unstable();
        assert_eq!(
            gc_files, sorted,
            "code batches emit in sorted file-path order; got {gc_files:?}"
        );

        // Stable batch keys: the two walks agree on every key (the content key is a pure function of
        // the batch bytes, independent of walk width).
        let keys = |seq: &[(String, String, Vec<u8>)]| -> Vec<String> {
            seq.iter().map(|(k, _, _)| k.clone()).collect()
        };
        assert_eq!(
            keys(&serial),
            keys(&parallel),
            "the batch keys are stable across walk width"
        );

        // Engagement: width 1 is the serial oracle (one worker); width 8 over six files fans out.
        assert_eq!(
            serial_stats.workers_engaged, 1,
            "width 1 runs inline: exactly one engaged worker"
        );
        assert!(
            parallel_stats.workers_engaged > 1,
            "width 8 over a six-file fixture engages more than one parse worker; got {}",
            parallel_stats.workers_engaged
        );
        assert_eq!(
            serial_stats.batches_emitted, parallel_stats.batches_emitted,
            "both widths emit the same number of file batches"
        );
        assert!(
            parallel_stats.batches_emitted >= 6,
            "each of the six source files contributes a code batch; got {}",
            parallel_stats.batches_emitted
        );
    }

    /// Criterion 4 (the SCOPED WALK): the ingest walk is scoped to the project's own sources. A
    /// fixture tree with (a) an in-root source file the project keeps, (b) a directory the
    /// project's OWN `.gitignore` excludes, (c) the VCS metadata directory `.git`, (d) rigger's
    /// runtime directory `.rigger`, and (e) a symlink escaping the root, must ingest EVERY in-root
    /// source file and NONE of the excluded paths - so the graph grows no cluster for tooling,
    /// ignored, or out-of-root paths. This owns the walk scope and root confinement: the scoping
    /// rides the ONE shared `walk_guarded` authority, so proving it here proves it for every ingest
    /// half (code and design) that walk drives.
    #[test]
    fn ingest_scopes_the_walk_to_the_project_and_never_escapes_the_root() {
        use std::collections::BTreeSet;
        let root_dir = tempfile::tempdir().unwrap();
        let root = root_dir.path();
        // (a) an in-root source file the walk MUST ingest.
        std::fs::write(root.join("keep.rs"), "fn kept() {}\n").unwrap();
        // (b) a build directory the project declares not-source via its OWN version-control
        // ignore rules (a real `.gitignore` at the root naming `build/`).
        std::fs::write(root.join(".gitignore"), "build/\n").unwrap();
        std::fs::create_dir(root.join("build")).unwrap();
        std::fs::write(root.join("build").join("gen.rs"), "fn generated() {}\n").unwrap();
        // (c) the VCS metadata directory and (d) rigger's runtime directory - never source.
        std::fs::create_dir(root.join(".git")).unwrap();
        std::fs::write(root.join(".git").join("hook.rs"), "fn vcs_internal() {}\n").unwrap();
        std::fs::create_dir(root.join(".rigger")).unwrap();
        std::fs::write(
            root.join(".rigger").join("state.rs"),
            "fn runtime_state() {}\n",
        )
        .unwrap();
        // (e) a symlink escaping the root: a subdirectory link pointing at an OUTSIDE tree, whose
        // file must never be reached through the link.
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("escaped.rs"), "fn out_of_root() {}\n").unwrap();
        std::os::unix::fs::symlink(outside.path(), root.join("outsider")).unwrap();

        // Ingest at width 1 (scope is width-independent) and collect the FILE each emitted content
        // key names (`<prefix>/<file>@<hash>#<i>`).
        let mut files: BTreeSet<String> = BTreeSet::new();
        ingest_project_paced(root.to_str().unwrap(), 1, |key, _ev: &Event| {
            if let Some((_, rest)) = key.split_once('/') {
                if let Some(file) = rest.split('@').next() {
                    files.insert(file.to_string());
                }
            }
        });

        // Every in-root source file is ingested.
        assert!(
            files.contains("keep.rs"),
            "the in-root source file must be ingested; got {files:?}"
        );
        // NONE of the excluded paths leak in - not the gitignored build dir, the VCS metadata, the
        // rigger runtime dir, nor anything reached by escaping the root through the symlink.
        for f in &files {
            assert!(
                !f.starts_with("build/")
                    && !f.starts_with(".git")
                    && !f.starts_with(".rigger")
                    && !f.starts_with("outsider"),
                "an excluded path leaked into the ingest: {f:?} (all ingested: {files:?})"
            );
        }
        // Concretely: the walk ingests EXACTLY the one in-root source file, nothing else.
        assert_eq!(
            files,
            BTreeSet::from(["keep.rs".to_string()]),
            "the walk ingests only the project's own in-root source; got {files:?}"
        );
    }
}
