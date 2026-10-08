//! The ingest fold rules: the replay-key vocabulary of the derived index, the group stamp every
//! keyed derived event carries, and the latest-generation lookup both ingest sinks seed from. The
//! walk that builds the keys lives in the root crate's `ingest` module.

use crate::eventstore::{Error, Event, EventStore, META_GROUP};

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
/// [`project_scoped_latest_generations`] which parses it back, rather than in the orchestrator that also
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
/// fail-safe direction a property of the code: a reader's key comparison is reached only by an
/// event of a type on the list that reader names - these four, or
/// [`PERCEPTION_TYPES`](crate::retention::PERCEPTION_TYPES), which adds the ledger entry that
/// stands for a batch of them - so no domain event can be dropped by that path however its replay
/// key happens to look. Domain events legitimately repeat (two identical review findings mean the
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

/// WHERE a derived-index content key `<prefix>/<file>@<hash>#<i>` splits: `(the byte range of the
/// BATCH IDENTITY - which file's batch this is - , the byte range of that batch's CONTENT
/// GENERATION)`, both indexing `key`. `None` when the key is not that shape.
///
/// This is THE parser of the form [`key_batch`] builds: the suppression predicate below cuts its
/// identity and generation from it. A reader that re-spells the format instead is not a style
/// problem: a hand-rolled copy of this split had drifted by one byte at the identity boundary while
/// the assertion written to catch that drift stayed green over both spellings, because it only
/// asked whether SOME split was found.
///
/// The identity deliberately carries the `<prefix>` segment, so one file's code (`gc`) and design
/// (`gd`) batches are two independent identities that never overwrite each other's generation -
/// and it is read as the WHOLE key's leading span, never by sniffing the prefix's VALUE.
/// [`key_batch`] takes its prefix from the CALLER, so a value sniff would rest on an unenforced
/// cross-module naming habit; the shape (a `/`, an `@`, and a `#<digits>` tail) is what the key
/// authority actually guarantees. `<file>` may itself contain `/`, `@` or `#`, so the tail and the
/// hash are split from the RIGHT.
///
/// The identity range ends BEFORE the `@` that separates it from the generation: the identity
/// STARTS the key and every key naming this batch begins with it.
pub fn derived_key_spans(key: &str) -> Option<(std::ops::Range<usize>, std::ops::Range<usize>)> {
    let (prefix, remainder) = crate::retention::GenerationIngested::identity_parts(key)?;
    let (head, index) = remainder.rsplit_once('#')?;
    if index.is_empty() || !index.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let (file, hash) = head.rsplit_once('@')?;
    if file.is_empty() || hash.is_empty() {
        return None;
    }
    // `key` is `<prefix>` + '/' + `<file>` + '@' + `<hash>` + '#' + `<i>`, so the identity is the
    // key's leading `prefix.len() + 1 + file.len()` bytes and the generation is the `hash.len()`
    // bytes that follow the `@` terminating it.
    let identity = prefix.len() + 1 + file.len();
    Some((0..identity, identity + 1..identity + 1 + hash.len()))
}

/// [`derived_key_spans`] as the two slices themselves - `(<prefix>/<file>, <hash>)` - for the
/// callers that want the text rather than the offsets. ONE parse, two views: it cuts the ranges
/// that function returns and computes nothing of its own.
pub fn derived_key_parts(key: &str) -> Option<(&str, &str)> {
    let (identity, generation) = derived_key_spans(key)?;
    Some((&key[identity], &key[generation]))
}

/// The `(<prefix>/<file> identity, content generation)` a derived-index event records, cut from
/// its replay key by the one key parser ([`derived_key_parts`]); `None` for an
/// event of any other type and for one whose key is absent or not that shape.
///
/// This is the fold's GENERATION RULE (spec 101): the graph models the project's CURRENT state, so
/// when a newer generation of an identity folds, every fact the prior generation asserted for that
/// file and the newer one does not re-assert is retired, and a node no live generation asserts,
/// no other event asserted and no live edge touches is retired with it. An event this answers
/// `None` for is outside the rule and folds as it always has (the fail-safe direction: nothing is
/// retired on a generation nobody can name).
pub fn derived_generation(e: &Event) -> Option<(&str, &str)> {
    if !is_derived_index_type(&e.type_) {
        return None;
    }
    derived_key_parts(e.meta.get(META_REPLAY_KEY)?)
}

/// A KEYED DERIVED EVENT (spec 101): `event` stamped with its replay `key` and, when the key is the
/// content-key shape, with the batch identity [`derived_key_parts`] cuts from it as its
/// [`META_GROUP`]. The one builder both ingest sinks record a derived event through, so every
/// recording carries the group [`latest_generation`] is answered from. A key that is not the
/// content-key shape names no identity, so its event carries no group and is never answered.
pub fn keyed_derived_event(event: Event, key: &str) -> Event {
    let event = event.with_meta(META_REPLAY_KEY, key);
    match derived_key_parts(key) {
        Some((identity, _)) => event.with_meta(META_GROUP, identity),
        None => event,
    }
}

/// THE LATEST RECORDED GENERATION of the batch identity `identity` on `stream` (spec 101), answered
/// by the store's group lookup ([`EventStore::latest_in_group`]) - never by reading the stream.
/// A recording is a keyed derived row or the ledger entry that stands for a batch (spec 107), and
/// either names its generation in its replay key. TYPE FIRST: a newest match outside
/// [`PERCEPTION_TYPES`](crate::retention::PERCEPTION_TYPES) answers no generation, as does one
/// whose replay key does not parse - the fail-safe direction, since a batch with no recorded
/// generation re-emits.
pub fn latest_generation(
    store: &dyn EventStore,
    stream: &str,
    identity: &str,
) -> Result<Option<String>, Error> {
    let Some(head) = store.latest_in_group(stream, identity)? else {
        return Ok(None);
    };
    if !crate::retention::PERCEPTION_TYPES.contains(&head.type_.as_str()) {
        return Ok(None);
    }
    Ok(head
        .meta
        .get(META_REPLAY_KEY)
        .and_then(|key| derived_key_parts(key))
        .map(|(_, generation)| generation.to_string()))
}

/// FIRST-SIGHT SEEDING (spec 101): whether the keyed batch `keyed` - one file's whole batch, every
/// key sharing one identity and one generation - is already its identity's latest recorded
/// generation on `stream`. Both ingest sinks ask this the first time they meet an identity in a
/// process: `true` means the batch IS its identity's latest recorded generation, recorded by its
/// own keyed rows or by the ledger entry that stands for it, so the sink installs its keys either
/// way and the batch appends nothing; `false` - a changed
/// file, a reverted one, a never-recorded one, or a batch whose key does not parse - means it
/// appends.
pub fn batch_is_latest_recorded(
    store: &dyn EventStore,
    stream: &str,
    keyed: &[(String, &Event)],
) -> Result<bool, Error> {
    let Some((identity, generation)) = keyed.first().and_then(|(key, _)| derived_key_parts(key))
    else {
        return Ok(false);
    };
    Ok(latest_generation(store, stream, identity)?.as_deref() == Some(generation))
}

/// The graph's side of [`batch_is_current`] (spec 107): what `graph.db` answers for an identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphSide<'a> {
    /// The graph owes its rebuild, so its answer is not asked.
    Owed,
    /// The identity's current generation in the graph, none when it holds none.
    Holds(Option<&'a str>),
}

/// Whether a batch is CURRENT (spec 107): a pure predicate over the log's latest generation of
/// the batch's identity (`logged`), the graph's side (`graph`) and the batch's own `generation`,
/// true only when both sides hold that generation. A graph that owes its rebuild is answered
/// from the log side alone. A batch that is current records no ledger entry; this alone decides
/// the ledger write.
pub fn batch_is_current(logged: Option<&str>, graph: GraphSide, generation: &str) -> bool {
    let holds = |side: Option<&str>| side == Some(generation);
    holds(logged)
        && match graph {
            GraphSide::Owed => true,
            GraphSide::Holds(current) => holds(current),
        }
}

/// WHERE A WALK HANDS ITS BATCHES (spec 101): a sink taking one file's WHOLE keyed batch at a
/// time, with the batch's flag (spec 107) - whether the walk excluded the batch's identity as an
/// out-of-line test module's. Every walk entry takes one, and [`sink_walked_batches`] hands one
/// to the walk it drives, so the shape is spelled once. Any closure over a batch and its flag is
/// one. The walk answers the flag; a sink never computes it.
pub trait BatchSink: FnMut(&[(String, &Event)], bool) {}

impl<F: FnMut(&[(String, &Event)], bool) + ?Sized> BatchSink for F {}

/// A WALK INTO A FALLIBLE SINK (spec 101): drive `walk`, handing each batch it produces to `sink`
/// with the flag the walk handed it, and answer the FIRST error the sink returned. A failed batch
/// never stops the walk - every batch after it still reaches the sink - and its error is never
/// swallowed. The one policy both ingest
/// sinks walk under, the run's keyed emit and a cold `rigger graph build`, so a batch whose lookup
/// or append failed is answered the same way by both: the walk fails.
pub fn sink_walked_batches<E>(
    walk: impl FnOnce(&mut dyn BatchSink),
    mut sink: impl FnMut(&[(String, &Event)], bool) -> Result<(), E>,
) -> Result<(), E> {
    let mut first = None;
    walk(&mut |keyed, excluded| {
        if let Err(e) = sink(keyed, excluded) {
            first.get_or_insert(e);
        }
    });
    first.map_or(Ok(()), Err)
}

/// The derived index's CONTENT-IDENTITY POLICY as one value: the metadata key a derived event
/// carries its content key under, the four types that carry content identity, and WHICH of those
/// types re-assert a fact in place rather than superseding the subject's prior recording.
///
/// It exists so no consumer has to re-spell the policy as loose parameters. Every field of it is a
/// per-type or per-key rule that a caller would otherwise pass positionally: two strings can be
/// handed over in the wrong order, a list can drift apart one call site at a time, and neither
/// says anything about which recording's date the projection holds. One value, built HERE beside the key authority that builds the key form and
/// beside the fold facts the partition is derived from, is what keeps every consumer on one story.
///
/// Its consumers are the compacting prune (`rigger reset --derived`) and the derived-duplication
/// measurement.
pub fn derived_index_identity() -> crate::eventstore::ContentIdentity {
    crate::eventstore::ContentIdentity::new(META_REPLAY_KEY, DERIVED_INDEX_TYPES)
        .with_reasserting_types(reasserted_derived_types())
        .with_key_parts(derived_key_parts)
        .with_facts(crate::eventstore::FactIdentity {
            alias_type: crate::contextgraph::TYPE_ALIAS_DEFINED,
            alias: crate::contextgraph::alias_definition,
            fact: crate::contextgraph::asserted_fact,
        })
}

/// The derived index types whose recordings RE-ASSERT a fact that was already true, rather than
/// SUPERSEDING the subject's prior recording - the ones whose EARLIEST recorded valid-time is the
/// one the graph holds, and which a compaction must therefore carry onto the recording it keeps.
///
/// Derived, never listed: the partition comes from the single fold fact
/// [`crate::contextgraph::refold_supersedes_prior_edges`], filtered over
/// [`DERIVED_INDEX_TYPES`] - so a FIFTH derived type added above is placed by the fold that
/// projects it, and no second hand-written list can drift from it.
pub fn reasserted_derived_types() -> Vec<&'static str> {
    DERIVED_INDEX_TYPES
        .into_iter()
        .filter(|t| !crate::contextgraph::refold_supersedes_prior_edges(t))
        .collect()
}

/// The project-scoped suppression predicate as a PURE REFERENCE over a slice of the log:
/// `identity -> (that identity's latest recorded generation hash, the keys of that generation)`,
/// derived from the events handed in.
///
/// Neither ingest sink reads a slice to seed itself (spec 101): both ask
/// [`batch_is_latest_recorded`], answered by the store's group lookup. This is the reference that
/// lookup is held to - the lookup's contract test asserts it answers what this answers on the same
/// log - and the reader `rigger validate`'s index-lag sample uses. The rule, in the order it is
/// applied:
///
/// 1. **Type first.** Only the `types` the caller hands in are eligible: the four
///    [`DERIVED_INDEX_TYPES`], or [`PERCEPTION_TYPES`](crate::retention::PERCEPTION_TYPES) to read
///    a ledger entry as a recording too. Every other event is passed over whatever its replay key
///    looks like, so a unit or stage whose id happened to read like an ingest prefix could never
///    have its lifecycle key mistaken for a project fact.
/// 2. **Then the whole key.** A derived event's key is parsed for its batch identity and content
///    generation ([`derived_key_parts`]); a key that is not that shape names no generation and is
///    passed over (the fail-safe direction - it re-emits).
/// 3. **Latest per file, never ever-recorded.** Only the keys of each identity's LATEST recorded
///    generation are kept. A file's earlier generations are deliberately absent: content REVERTED
///    to a generation the file has since moved past differs from its latest recorded batch, so it
///    must re-emit. An ever-recorded key set would match the old records, re-emit nothing, and
///    strand the graph on a superseded version of that file forever. Whether the
///    re-emitted batch then RETIRES the newer generation's facts is the FOLD's business, not this
///    predicate's ([`derived_generation`]).
///
/// This is project-scoped ON PURPOSE: derived index facts are facts about the project's files, not
/// about a run, so a NEW run inherits them and an unchanged file appends nothing on every
/// subsequent run forever.
pub fn project_scoped_latest_generations(
    prior: &[Event],
    types: &[&str],
) -> std::collections::HashMap<String, (String, Vec<String>)> {
    // identity -> (that identity's latest recorded generation, the keys of that generation)
    let mut latest: std::collections::HashMap<String, (String, Vec<String>)> =
        std::collections::HashMap::new();
    for e in prior {
        // TYPE first: an event of no handed type never reaches the key comparison at all.
        if !types.contains(&e.type_.as_str()) {
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
    latest
}

/// THE LOG SIDE OF PERCEPTION (spec 107): each identity's latest recording on `stream`, a ledger
/// entry or a keyed derived row alike, as [`project_scoped_latest_generations`] answers it over
/// ONE typed read of [`PERCEPTION_TYPES`](crate::retention::PERCEPTION_TYPES) from the stream's
/// start. The generation is cut from each row's replay key, never looked up by group, so a keyed
/// derived row recorded with no group is seen. On a store whose derived rows are not yet shed the
/// read holds every one of them at once.
pub fn perceived_generations(
    store: &dyn EventStore,
    stream: &str,
) -> Result<std::collections::HashMap<String, (String, Vec<String>)>, Error> {
    let types = crate::retention::PERCEPTION_TYPES;
    let recorded =
        store.read_stream_typed(stream, 0, crate::eventstore::TypeSelection::Only(&types))?;
    Ok(project_scoped_latest_generations(&recorded, &types))
}

/// [`batch_is_current`]'s whole truth table: the log's latest generation and the graph's side
/// against one batch generation.
#[cfg(test)]
mod current_tests {
    use super::{batch_is_current, GraphSide};

    #[test]
    fn a_batch_is_current_only_when_the_log_and_the_graph_both_hold_its_generation() {
        let cases = [
            (Some("g1"), GraphSide::Holds(Some("g1")), true),
            (Some("g0"), GraphSide::Holds(Some("g1")), false),
            (Some("g1"), GraphSide::Holds(Some("g0")), false),
            (Some("g0"), GraphSide::Holds(Some("g0")), false),
            (None, GraphSide::Holds(Some("g1")), false),
            (Some("g1"), GraphSide::Holds(None), false),
            (None, GraphSide::Holds(None), false),
        ];
        assert_eq!(
            cases.map(|(logged, graph, _)| batch_is_current(logged, graph, "g1")),
            cases.map(|(_, _, current)| current)
        );
    }

    #[test]
    fn an_owed_graph_is_answered_from_the_log_side_alone() {
        assert_eq!(
            [Some("g1"), Some("g0"), None].map(|logged| batch_is_current(
                logged,
                GraphSide::Owed,
                "g1"
            )),
            [true, false, false]
        );
    }
}

/// The suppression predicate's OWN contract, at the unit level: which recorded keys it hands a
/// sink, given a stream. Both lanes compile it, because the predicate is not `symbols`-gated - it
/// reads recorded events, it does not walk a tree. The two SEAM-level proofs BELONG to their own
/// criteria and are not in this tree yet: that a prior run's non-ingest key never suppresses this
/// run's keyed emit is criterion 2's, and that a reverted file survives the full suppression stack
/// is criterion 3's.
#[cfg(test)]
mod dedup_tests {
    use super::{derived_key_parts, META_REPLAY_KEY};
    use crate::contextgraph::{
        TYPE_CODE_ENTITY_EXTRACTED, TYPE_DOC_CONCEPT_EXTRACTED, TYPE_EDGE_INFERRED,
        TYPE_REVIEW_FINDING,
    };
    use crate::eventstore::Event;
    use crate::test_support::reference_replay_keys;
    use std::collections::BTreeSet;

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

        let keys = reference_replay_keys(&stream);

        assert_eq!(
            keys,
            BTreeSet::from([
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
            reference_replay_keys(&[]).is_empty(),
            "an empty stream suppresses nothing"
        );
    }

    /// The type list decides which rows the reference reads (spec 107): under the perception types
    /// a ledger entry is a recording like a derived row, so an identity's latest recording is its
    /// entry; under the derived types the entry is passed over and the derived row before it
    /// stands; and under no type nothing is a recording.
    #[test]
    fn the_reference_reads_only_the_types_it_is_handed() {
        use crate::ingest::{project_scoped_latest_generations, DERIVED_INDEX_TYPES};
        use crate::retention::{PERCEPTION_TYPES, TYPE_GENERATION_INGESTED};
        use std::collections::HashMap;

        let stream = vec![
            keyed(TYPE_CODE_ENTITY_EXTRACTED, "gc/src/a.rs@h1#0"),
            keyed(TYPE_GENERATION_INGESTED, "gc/src/a.rs@h9#3"),
            keyed(TYPE_GENERATION_INGESTED, "gd/docs/a.md@h4#1"),
            keyed(TYPE_REVIEW_FINDING, "gc/src/b.rs@h1#0"),
        ];
        let answer = |identity: &str, generation: &str, key: &str| {
            (
                identity.to_string(),
                (generation.to_string(), vec![key.to_string()]),
            )
        };

        assert_eq!(
            project_scoped_latest_generations(&stream, &PERCEPTION_TYPES),
            HashMap::from([
                answer("gc/src/a.rs", "h9", "gc/src/a.rs@h9#3"),
                answer("gd/docs/a.md", "h4", "gd/docs/a.md@h4#1"),
            ]),
            "a ledger entry is the latest recording of its identity"
        );
        assert_eq!(
            project_scoped_latest_generations(&stream, &DERIVED_INDEX_TYPES),
            HashMap::from([answer("gc/src/a.rs", "h1", "gc/src/a.rs@h1#0")]),
            "the derived types pass a ledger entry over"
        );
        assert_eq!(
            project_scoped_latest_generations(&stream, &[]),
            HashMap::new(),
            "no type, no recording"
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
            reference_replay_keys(&stream),
            BTreeSet::from([
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
                reference_replay_keys(&[keyed(TYPE_CODE_ENTITY_EXTRACTED, key)]).is_empty(),
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

/// THE GROUP STAMP AND THE LATEST-GENERATION READER (spec 101), at the unit level: what a keyed
/// derived event carries, and which generation the one domain reader cuts from the store's group
/// answer, a derived row's or a ledger entry's (spec 107). The store's own answer is pinned per backend by the contract suite; here the store is a
/// double answering one fixed head, so every arm of the reader is driven directly.
#[cfg(test)]
mod group_lookup_tests {
    use super::{
        batch_is_latest_recorded, keyed_derived_event, latest_generation, sink_walked_batches,
        BatchSink, META_REPLAY_KEY,
    };
    use crate::contextgraph::{TYPE_CODE_ENTITY_EXTRACTED, TYPE_REVIEW_FINDING};
    use crate::eventstore::{Event, GroupHead, META_GROUP};
    use crate::test_support::GroupLookupOnly;
    use std::collections::BTreeMap;

    /// A lookup-only store answering `head` for every group.
    fn answering(head: Option<GroupHead>) -> GroupLookupOnly {
        GroupLookupOnly::new(Ok(head))
    }

    fn head(type_: &str, key: Option<&str>) -> GroupHead {
        let mut meta = BTreeMap::new();
        if let Some(key) = key {
            meta.insert(META_REPLAY_KEY.to_string(), key.to_string());
        }
        GroupHead {
            position: 7,
            type_: type_.to_string(),
            meta,
        }
    }

    #[test]
    fn a_keyed_derived_event_carries_its_replay_key_and_its_batch_identity_as_its_group() {
        let event = keyed_derived_event(
            Event::new(TYPE_CODE_ENTITY_EXTRACTED, b"{}".to_vec()),
            "gc/vendor/pkg@1.2.3/a.rs@h1#4",
        );
        assert_eq!(
            event.meta,
            BTreeMap::from([
                (
                    META_GROUP.to_string(),
                    "gc/vendor/pkg@1.2.3/a.rs".to_string()
                ),
                (
                    META_REPLAY_KEY.to_string(),
                    "gc/vendor/pkg@1.2.3/a.rs@h1#4".to_string()
                ),
            ]),
            "the group is the whole `<prefix>/<file>` span the key parser cuts"
        );
        assert_eq!(event.type_, TYPE_CODE_ENTITY_EXTRACTED);
        assert_eq!(event.data, b"{}".to_vec(), "the payload is untouched");
    }

    #[test]
    fn a_key_that_is_not_the_content_key_shape_stamps_no_group() {
        let event = keyed_derived_event(Event::new(TYPE_CODE_ENTITY_EXTRACTED, vec![]), "gc/a.rs");
        assert_eq!(
            event.meta,
            BTreeMap::from([(META_REPLAY_KEY.to_string(), "gc/a.rs".to_string())]),
            "no identity, so no group: the event is never answered by a group lookup"
        );
    }

    #[test]
    fn the_latest_generation_is_cut_from_the_newest_group_members_replay_key() {
        let store = answering(Some(head(TYPE_CODE_ENTITY_EXTRACTED, Some("gc/a.rs@h2#3"))));
        assert_eq!(
            latest_generation(&store, "rigger", "gc/a.rs").unwrap(),
            Some("h2".to_string())
        );
        assert_eq!(
            store.asked(),
            [("rigger".to_string(), "gc/a.rs".to_string())],
            "one group lookup of that identity on that stream, and nothing else"
        );
    }

    /// A ledger entry is a recording (spec 107): the newest member being a hand-built
    /// `GenerationIngested` answers the generation its replay key names, with the one lookup a
    /// derived row costs.
    #[test]
    fn the_latest_generation_of_an_identity_whose_newest_member_is_a_ledger_entry_is_the_entrys() {
        let store = answering(Some(head(
            crate::retention::TYPE_GENERATION_INGESTED,
            Some("gc/a.rs@h9#3"),
        )));
        assert_eq!(
            latest_generation(&store, "rigger", "gc/a.rs").unwrap(),
            Some("h9".to_string())
        );
        assert_eq!(
            store.asked(),
            [("rigger".to_string(), "gc/a.rs".to_string())],
            "one group lookup of that identity on that stream, and nothing else"
        );
    }

    #[test]
    fn no_recorded_member_a_non_perception_member_or_an_unparseable_key_answers_no_generation() {
        for (store, why) in [
            (answering(None), "a never-recorded identity"),
            (
                answering(Some(head(TYPE_REVIEW_FINDING, Some("gc/a.rs@h2#0")))),
                "a newest member outside the perception types (type first)",
            ),
            (
                answering(Some(head(
                    crate::retention::TYPE_GENERATION_INGESTED,
                    Some("gc/a.rs"),
                ))),
                "a newest ledger entry whose key does not parse",
            ),
            (
                answering(Some(head(TYPE_CODE_ENTITY_EXTRACTED, Some("gc/a.rs")))),
                "a newest member whose key does not parse",
            ),
            (
                answering(Some(head(TYPE_CODE_ENTITY_EXTRACTED, None))),
                "a newest member with no replay key",
            ),
        ] {
            assert_eq!(
                latest_generation(&store, "rigger", "gc/a.rs").unwrap(),
                None,
                "{why} answers no generation, so its batch re-emits"
            );
        }
    }

    #[test]
    fn a_batch_is_the_latest_recorded_only_when_its_generation_is_the_recorded_one() {
        let ev = Event::new(TYPE_CODE_ENTITY_EXTRACTED, vec![]);
        let batch = |generation: &str| -> Vec<(String, &Event)> {
            (0..2)
                .map(|i| (format!("gc/a.rs@{generation}#{i}"), &ev))
                .collect()
        };
        let store = answering(Some(head(TYPE_CODE_ENTITY_EXTRACTED, Some("gc/a.rs@h2#1"))));
        assert!(
            batch_is_latest_recorded(&store, "rigger", &batch("h2")).unwrap(),
            "the recorded generation: its keys are the recorded ones, it appends nothing"
        );
        assert!(
            !batch_is_latest_recorded(&store, "rigger", &batch("h1")).unwrap(),
            "another generation (a change or a revert) appends"
        );
        assert_eq!(
            store.asked(),
            [
                ("rigger".to_string(), "gc/a.rs".to_string()),
                ("rigger".to_string(), "gc/a.rs".to_string())
            ],
            "each question is one lookup of the batch's identity"
        );
        let empty = answering(None);
        assert!(
            !batch_is_latest_recorded(&empty, "rigger", &batch("h2")).unwrap(),
            "a never-recorded identity appends"
        );
        assert!(
            !batch_is_latest_recorded(&store, "rigger", &[("gc/a.rs".to_string(), &ev)]).unwrap()
                && !batch_is_latest_recorded(&store, "rigger", &[]).unwrap(),
            "an unparseable or empty batch appends"
        );
        assert_eq!(
            store.asked().len(),
            2,
            "a batch that names no identity asks the store nothing"
        );
    }

    #[test]
    fn a_walk_reaches_every_batch_past_a_failed_one_and_answers_the_first_error() {
        let ev = Event::new(TYPE_CODE_ENTITY_EXTRACTED, vec![]);
        let batches: Vec<(Vec<(String, &Event)>, bool)> =
            [("a", true), ("b", false), ("c", false), ("d", true)]
                .iter()
                .map(|(file, excluded)| (vec![(format!("gc/{file}.rs@h#0"), &ev)], *excluded))
                .collect();
        let walk = |sink: &mut dyn BatchSink| {
            for (batch, excluded) in &batches {
                sink(batch, *excluded);
            }
        };

        let mut sunk = Vec::new();
        let answer = sink_walked_batches(walk, |keyed, excluded| {
            let key = keyed[0].0.clone();
            sunk.push((key.clone(), excluded));
            if key.starts_with("gc/b") || key.starts_with("gc/d") {
                Err(key)
            } else {
                Ok(())
            }
        });
        assert_eq!(
            answer,
            Err("gc/b.rs@h#0".to_string()),
            "the walk answers its FIRST failed batch's error, never a later one's"
        );
        assert_eq!(
            sunk,
            [
                ("gc/a.rs@h#0".to_string(), true),
                ("gc/b.rs@h#0".to_string(), false),
                ("gc/c.rs@h#0".to_string(), false),
                ("gc/d.rs@h#0".to_string(), true),
            ],
            "a failed batch never stops the walk: every batch reaches the sink, in walk order, \
             with the flag the walk handed it"
        );

        assert_eq!(
            sink_walked_batches(walk, |_, _| Ok::<(), String>(())),
            Ok(()),
            "a walk whose every batch lands answers Ok"
        );
    }
}
