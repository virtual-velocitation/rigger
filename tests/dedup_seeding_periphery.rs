//! Periphery (contract / API / integration) tests for spec 60 criterion 1: the ingest dedup keys
//! are seeded from the WHOLE stream, through one project-scoped suppression predicate, so an
//! unchanged tree appends nothing on every subsequent run forever.
//!
//! These run OUTSIDE the crate, over the library's PUBLIC surface and the SHIPPED binary, so they
//! guard the boundary the inside-out unit tests are structurally blind to. The unit layer proves
//! the predicate's rule against HAND-SPELLED keys (`ingest::dedup_tests`) and the run entry's
//! outcome over a real tree (`conductor::tests::a_second_run_over_an_unchanged_tree_...`). What
//! neither can see is the boundary:
//!
//! 1. The four new items (`META_REPLAY_KEY`, `DERIVED_INDEX_TYPES`, `is_derived_index_type`,
//!    `project_scoped_latest_generations`) are now PUBLIC API with an EXTERNAL consumer - `src/main.rs`
//!    reaches them as `rigger::ingest::...` across the crate boundary. Nothing drove them from
//!    outside the crate.
//! 2. The `<prefix>/<file>@<hash>#<i>` content key gained a READER (`derived_key_parts`) beside
//!    its long-standing WRITER (`key_batch`). A hand-spelled key cannot prove the reader accepts
//!    what the writer actually mints; if the two ever drift, the predicate degrades SILENTLY to
//!    "suppresses nothing" and the log resumes growing without bound. These tests round-trip REAL
//!    minted keys - including for paths that carry the key format's own `@` separator, a shape
//!    only a real walk over a real tree can prove the writer ever mints.
//! 3. `cmd_graph_build` - the cold build's sink - decides type-first, as that predicate does. Its
//!    type-first behaviour is only observable end to end,
//!    through the built binary and a store the test seeds.
//! 4. The replay-key METADATA NAME is now owned by `rigger::ingest` and RE-EXPORTED as
//!    `rigger::conductor::META_REPLAY_KEY`, which every existing caller names. A stamping half and
//!    a reading half that address the slot through two different constants is a boundary no
//!    single-module test can see, and it fails silently: nothing suppresses, and the log grows.
//! 5. The predicate is PROJECT-scoped, so a `RunStarted` in the stream it is handed must not change
//!    its answer. The run entry's own end-to-end proof of that needs a tree to walk and is
//!    therefore `symbols`-gated, while the predicate is compiled and called in BOTH lanes and is
//!    the surface a future consumer meets - so the light lane, and the API edge, are pinned here.
//!
//! Scope is strictly criterion 1. The run-scoping boundary for NON-ingest keys (criterion 2), the
//! run-level change/revert path (criterion 3) and the store-layer append guard (criterion 4) are
//! owned by sibling units and are deliberately not exercised here.

use rigger::contextgraph::{
    TYPE_ALIAS_DEFINED, TYPE_ALIAS_UNRESOLVED, TYPE_CODE_ENTITY_EXTRACTED, TYPE_COMMUNITY_ASSIGNED,
    TYPE_CONCEPT_DERIVED, TYPE_CONCEPT_REALIZED, TYPE_DECISION_MADE, TYPE_DOC_CONCEPT_EXTRACTED,
    TYPE_DOC_LINK_EXTRACTED, TYPE_EDGE_INFERRED, TYPE_FILE_TOUCHED, TYPE_GATE_VERDICT,
    TYPE_LESSON_LEARNED, TYPE_REVIEW_FINDING, TYPE_UNIT_INTEGRATED, TYPE_UNIT_STARTED,
};
use rigger::eventstore::Event;
use rigger::ingest::{is_derived_index_type, DERIVED_INDEX_TYPES};
use std::collections::BTreeSet;

/// An event of `type_` carrying `key` in the replay-key metadata slot - the slot `rigger graph
/// build` stamps on each keyed derived event it appends, and the run's sink on its one ledger
/// entry - built here through the crate's public `Event` API so the test pins the recorded form
/// rather than an in-crate helper. The slot is addressed through
/// `rigger::conductor::META_REPLAY_KEY`, the spelling every existing caller of the crate uses;
/// that it is the SAME name the owning `rigger::ingest` module publishes, and therefore the same
/// slot the predicate reads, is itself pinned below rather than assumed here.
fn keyed(type_: &str, key: &str) -> Event {
    Event::new(type_, Vec::new()).with_meta(rigger::conductor::META_REPLAY_KEY, key)
}

/// Every event type in the crate's public vocabulary that is NOT part of the derived index: the
/// domain and lifecycle facts whose recurrence is meaningful and which must therefore never be
/// eligible for content-key suppression, however their replay key happens to be spelled. The
/// conductor's own lifecycle types are named by their on-log strings (they are stamped as literals
/// by the run, not exported as constants), so the enumeration covers the log as it actually reads.
fn non_derived_event_types() -> Vec<&'static str> {
    vec![
        TYPE_DECISION_MADE,
        TYPE_FILE_TOUCHED,
        TYPE_GATE_VERDICT,
        TYPE_UNIT_STARTED,
        TYPE_UNIT_INTEGRATED,
        TYPE_LESSON_LEARNED,
        TYPE_ALIAS_DEFINED,
        TYPE_ALIAS_UNRESOLVED,
        TYPE_REVIEW_FINDING,
        TYPE_COMMUNITY_ASSIGNED,
        TYPE_CONCEPT_DERIVED,
        TYPE_CONCEPT_REALIZED,
        rigger::run::TYPE_RUN_STARTED,
        "SpawnRequested",
        "SpawnResult",
        "UnitProposed",
        "UnitStatus",
        "UnitFailed",
        "UnitEscalated",
        "BlastRadiusComputed",
    ]
}

/// CONTRACT, at the public API edge. `DERIVED_INDEX_TYPES` is the code-owned discriminator that
/// decides what may be suppressed at all, and it is now reachable from outside the crate - so what
/// it CONTAINS is a published contract, not an implementation detail. It must be exactly the four
/// graph index types, named from the graph's own constants rather than from the ingest module that
/// is under test, so a future graph type added to one list and not the other reddens here instead
/// of silently widening (a domain event becomes droppable) or narrowing (the log resumes growing)
/// the suppression set. `is_derived_index_type` must agree with the constant on every member -
/// they are two views of one decision and a consumer may use either.
#[test]
fn derived_index_types_are_exactly_the_four_graph_index_types_and_the_predicate_agrees() {
    let published: BTreeSet<&str> = DERIVED_INDEX_TYPES.iter().copied().collect();

    assert_eq!(
        published,
        BTreeSet::from([
            TYPE_CODE_ENTITY_EXTRACTED,
            TYPE_EDGE_INFERRED,
            TYPE_DOC_CONCEPT_EXTRACTED,
            TYPE_DOC_LINK_EXTRACTED,
        ]),
        "the published derived-index discriminator must be exactly the four graph index types"
    );
    assert_eq!(
        published.len(),
        DERIVED_INDEX_TYPES.len(),
        "the published discriminator must carry no duplicate entry"
    );
    for t in DERIVED_INDEX_TYPES {
        assert!(
            is_derived_index_type(t),
            "is_derived_index_type must agree with the constant it is derived from, for {t:?}"
        );
    }
}

/// CONTRACT: the fail-safe direction is TOTAL, not anecdotal. The whole partition rests on the type
/// test being asked FIRST, so that no domain or lifecycle event can ever be dropped by the dedup
/// path however its replay key reads. The unit layer pins that for ONE type (`ReviewFinding`); an
/// external consumer's guarantee is the universal one, so this drives the full public vocabulary
/// through the predicate with every event carrying a WELL-FORMED content key - the worst case, in
/// which only the type test stands between a domain fact and suppression. The predicate must return
/// the EMPTY set: nothing here is eligible, so nothing here can suppress a later emit.
#[test]
fn no_non_derived_event_is_eligible_however_its_replay_key_is_spelled() {
    let types = non_derived_event_types();
    for t in &types {
        assert!(
            !is_derived_index_type(t),
            "{t:?} is a domain/lifecycle fact and must never be eligible for content-key suppression"
        );
    }

    // Every one of them stamped with a key that is EXACTLY the content-key shape, including the
    // `gc`/`gd` prefixes the ingest sinks really use - so only the type test can save them.
    let stream: Vec<Event> = types
        .iter()
        .enumerate()
        .flat_map(|(i, t)| {
            [
                keyed(t, &format!("gc/src/file{i}.rs@abc123#0")),
                keyed(t, &format!("gd/docs/file{i}.md@abc123#0")),
            ]
        })
        .collect();

    assert!(
        reference_replay_keys(&stream).is_empty(),
        "no non-derived event may contribute a suppression key - the type test is asked first, so \
         a domain fact whose replay key is spelled like a content key is still passed over"
    );
}

/// CONTRACT: the predicate is a pure, order-stable function of the recorded stream, which is what
/// lets two independent processes reading the same log (the run's seeding and a cold `graph build`)
/// reach the SAME suppression set. Reading the same stream twice must yield the same set, and a
/// stream that carries no derived event at all must suppress nothing.
#[test]
fn the_predicate_is_a_pure_function_of_the_recorded_stream() {
    let stream = vec![
        keyed(TYPE_CODE_ENTITY_EXTRACTED, "gc/src/a.rs@h1#0"),
        keyed(TYPE_EDGE_INFERRED, "gc/src/a.rs@h1#1"),
        keyed(TYPE_DOC_LINK_EXTRACTED, "gd/docs/a.md@h9#0"),
    ];

    let first = reference_replay_keys(&stream);
    let second = reference_replay_keys(&stream);

    assert_eq!(
        first, second,
        "two reads of one stream must reach the same suppression set - the property that keeps a \
         run and a cold build from disagreeing about what a fresh emit is redundant against"
    );
    assert!(
        reference_replay_keys(&[]).is_empty(),
        "an empty stream suppresses nothing"
    );
    assert!(
        reference_replay_keys(&[Event::new(TYPE_CODE_ENTITY_EXTRACTED, Vec::new())]).is_empty(),
        "a derived event with no replay key at all names no generation, so it suppresses nothing"
    );
}

/// CONTRACT: a RUN BOUNDARY in the stream is not a boundary to this predicate - it is what
/// PROJECT-scoped means, stated at the public API every consumer reaches it through.
///
/// This reference is handed a WHOLE multi-run stream, and its answer - "what is already
/// recorded" - is what the group lookup each sink asks is held to. The word that makes that safe is project-scoped: a file's content hash does not
/// change because a new run started, so a `RunStarted` and the lifecycle facts that follow it must
/// leave the answer exactly as it was. The regression this guards is not hypothetical - it is the
/// PRIOR behaviour this criterion removes, in which the derived keys were scoped to the current
/// run's slice, so every new run saw an empty ingest-key set and re-appended the whole derived
/// index. A predicate that re-acquired any run awareness would restore precisely that defect.
///
/// Nothing else pins it HERE, at the predicate, and in the LIGHT lane nothing pins it at all: the
/// run entry's own end-to-end proof needs a tree to walk, so it is `symbols`-gated, while this
/// predicate is compiled and called in both lanes. It is also the layer a future consumer meets -
/// the predicate is the shared authority, so a caller that is neither of today's two sinks inherits
/// this contract and no test of either sink covers it.
///
/// Two directions, both killable. A run-scoped read returns the empty set for the first file (its
/// generation was recorded by the EARLIER run) and reddens the first assertion. A first-generation
/// -wins read keeps the second file's superseded generation live across the boundary and reddens
/// the second. The rule is one rule over the whole stream, and a run boundary is invisible to it.
#[test]
fn derived_keys_recorded_by_an_earlier_run_still_seed_the_next_run() {
    // What an earlier run recorded: one file's code batch, another file's design batch, and that
    // second file at a generation it will move past.
    let mut stream = vec![
        keyed(TYPE_CODE_ENTITY_EXTRACTED, "gc/src/steady.rs@h1#0"),
        keyed(TYPE_EDGE_INFERRED, "gc/src/steady.rs@h1#1"),
        keyed(TYPE_DOC_CONCEPT_EXTRACTED, "gd/docs/moving.md@h1#0"),
        keyed(TYPE_DOC_LINK_EXTRACTED, "gd/docs/moving.md@h1#1"),
    ];

    // A NEW run begins and records its own lifecycle facts, exactly as a real log carries them.
    stream.push(Event::new(rigger::run::TYPE_RUN_STARTED, Vec::new()));
    stream.push(keyed(TYPE_UNIT_STARTED, "u-alpha/started"));
    stream.push(keyed(TYPE_GATE_VERDICT, "u-alpha/fmt#0"));
    stream.push(keyed(TYPE_DECISION_MADE, "u-alpha/decided#0"));

    // ... and in that new run, ONE of the two files moves to a new content generation.
    stream.push(keyed(TYPE_DOC_CONCEPT_EXTRACTED, "gd/docs/moving.md@h2#0"));
    stream.push(keyed(TYPE_DOC_LINK_EXTRACTED, "gd/docs/moving.md@h2#1"));

    let seed: BTreeSet<String> = reference_replay_keys(&stream);

    // DIRECTION ONE - the untouched file's generation was recorded by the EARLIER run and must
    // still suppress. Scope the read to the current run and this set is missing it, so the file's
    // whole batch re-appends on this run and on every run after it: the unbounded growth this
    // criterion exists to end.
    let steady: BTreeSet<String> = ["gc/src/steady.rs@h1#0", "gc/src/steady.rs@h1#1"]
        .into_iter()
        .map(String::from)
        .collect();
    assert!(
        steady.is_subset(&seed),
        "a file untouched since an EARLIER run must still be suppressed by what that run recorded \
         - a run boundary is not a boundary to a project-scoped seed; missing {:?}",
        steady.difference(&seed).collect::<Vec<_>>()
    );

    // DIRECTION TWO - the file that moved in the LATER run keeps only its newest generation live,
    // so the retirement is computed over the WHOLE stream and not within one run's slice.
    assert_eq!(
        seed,
        BTreeSet::from([
            "gc/src/steady.rs@h1#0".to_string(),
            "gc/src/steady.rs@h1#1".to_string(),
            "gd/docs/moving.md@h2#0".to_string(),
            "gd/docs/moving.md@h2#1".to_string(),
        ]),
        "the seed must be each file's latest generation across the whole stream: the untouched \
         file's earlier-run keys live, the moved file's superseded generation retired, and none of \
         the new run's own lifecycle keys admitted"
    );
}

/// CONTRACT at the crate boundary: the replay-key METADATA NAME is ONE name, owned beside the key
/// authority, and the conductor's is that same name rather than a second spelling of it.
///
/// The name is a WIRE fact, and its two halves now live in different modules. `rigger::ingest`
/// OWNS the constant, because it both builds the `<prefix>/<file>@<hash>#<i>` key and parses it
/// back in the suppression predicate; `rigger::conductor::META_REPLAY_KEY` - the spelling every
/// existing caller, both ingest sinks, and every other test in this file name - is a RE-EXPORT of
/// it. A second constant declared in the conductor under a different literal would compile, and it
/// would leave every other test in this suite green: they all stamp AND read through the conductor
/// spelling, so writer and reader would still agree with each other. What would break is the seam
/// between them - the sinks would stamp under one name while the predicate read another, every
/// fresh emit would look unrecorded, nothing would ever be suppressed, and the log would resume
/// growing without bound, which is the exact defect this criterion exists to end. Nothing else
/// pins that, so it is pinned here: as the published equality, and as the behaviour the equality
/// buys - a key written under EITHER export is read back through the other, and by the predicate.
#[test]
fn the_replay_key_metadata_name_is_one_name_owned_beside_the_key_authority() {
    assert_eq!(
        rigger::ingest::META_REPLAY_KEY,
        rigger::conductor::META_REPLAY_KEY,
        "the conductor's replay-key name must BE the ingest module's, never a second spelling"
    );
    assert!(
        !rigger::ingest::META_REPLAY_KEY.is_empty(),
        "the replay-key metadata name must name a real slot"
    );

    // The slot itself: a key stamped through the caller-facing export must be readable through the
    // owning module's - one slot, one name, whichever path a consumer reaches it by.
    let key = "gc/src/a.rs@h1#0";
    let stamped_by_a_sink = keyed(TYPE_CODE_ENTITY_EXTRACTED, key);
    assert_eq!(
        stamped_by_a_sink
            .meta
            .get(rigger::ingest::META_REPLAY_KEY)
            .map(String::as_str),
        Some(key),
        "a key stamped through the conductor's export must be readable under the owning module's"
    );

    // ... and the predicate, which reads through the owning module's name, must recognise a key
    // stamped through EITHER export. That recognition is the whole point of there being one name.
    let expected = BTreeSet::from([key.to_string()]);
    for (via, event) in [
        ("the conductor's re-export", stamped_by_a_sink),
        (
            "the ingest module's own constant",
            Event::new(TYPE_CODE_ENTITY_EXTRACTED, Vec::new())
                .with_meta(rigger::ingest::META_REPLAY_KEY, key),
        ),
    ] {
        let seen: BTreeSet<String> = reference_replay_keys(&[event]);
        assert_eq!(
            seen, expected,
            "the suppression predicate must read a key stamped through {via}; a stamping half and \
             a reading half that address two different slots suppress nothing at all"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// The seam tests below need the extraction pass to have a tree to walk, so they are `symbols`-gated
// exactly as the walk is. In the light lane the walk is a documented no-op that mints no key, so
// there is no producer/consumer pair to round-trip and no batch for a build to suppress; the three
// contract tests above stay UNGATED and keep that lane's coverage of the predicate's own boundary.
// ---------------------------------------------------------------------------------------------

/// A throwaway project that is its own git repo (so the binary resolves a stable project identity
/// and folds from the git top-level), carrying a source file for the code half of the index and a
/// real design doc for the design half - both halves of the derived index have work to mint.
#[cfg(feature = "symbols")]
fn temp_ingestable_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let _ = common::git::run_git(root, &["init", "-q"]);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("specs")).unwrap();
    std::fs::create_dir_all(root.join(".rigger")).unwrap();
    std::fs::write(
        root.join("src/alpha.rs"),
        "pub fn alpha_helper() {}\npub fn alpha_caller() { alpha_helper(); }\n",
    )
    .unwrap();
    std::fs::write(
        root.join("specs/sample.md"),
        include_str!("../specs/29c-unified-traversal-tiers.md"),
    )
    .unwrap();
    dir
}

/// Every `<prefix>/<file>@<hash>#<i>` key the SHIPPED walk-and-key authority mints for the tree at
/// `root`, paired with the event type it was minted for. This is the WRITER half of the round-trip:
/// it is the same public entry `rigger graph build` and the run's ingest both drive, so these are
/// the real keys the log will carry - never a spelling this test invented.
#[cfg(feature = "symbols")]
fn minted(root: &std::path::Path) -> Vec<(String, String)> {
    minted_events(root)
        .into_iter()
        .map(|(key, event)| (key, event.type_))
        .collect()
}

/// The minted keys as a set, for comparison against what the predicate hands a sink back.
#[cfg(feature = "symbols")]
fn minted_keys(root: &std::path::Path) -> BTreeSet<String> {
    minted(root).into_iter().map(|(k, _)| k).collect()
}

/// The recorded events, replayed as a sink would see them: each minted key stamped onto an event of
/// the type it was minted for - the shape `rigger graph build` appends (`with_meta(replay_key)`).
#[cfg(feature = "symbols")]
fn as_recorded(minted: &[(String, String)]) -> Vec<Event> {
    minted.iter().map(|(k, t)| keyed(t, k)).collect()
}

/// ROUND-TRIP CONTRACT between the key WRITER and the new key READER, over REAL keys.
///
/// `key_batch` mints `<prefix>/<file>@<hash>#<i>`; `project_scoped_latest_generations` now parses that
/// same string back into (batch identity, content generation). The unit test proves the reader's
/// rule against keys a human typed - which cannot fail if the writer's format moves. This drives
/// the SHIPPED walk over a real tree and asserts the reader recognises EVERY key it mints: the
/// returned suppression set is exactly the minted set, with nothing passed over as malformed.
///
/// That equality is the load-bearing one, because the failure it guards is SILENT. A reader that
/// stopped recognising the writer's keys would suppress nothing, every run would re-append the
/// whole derived index, and no test that only checks "the rule is right" would notice - the log
/// would simply resume growing without bound, which is the defect this criterion exists to end.
#[cfg(feature = "symbols")]
#[test]
fn every_content_key_the_walk_actually_mints_round_trips_through_the_predicate() {
    let dir = temp_ingestable_project();
    let root = dir.path();

    let recorded = minted(root);
    let keys: BTreeSet<String> = recorded.iter().map(|(k, _)| k.clone()).collect();
    assert!(
        !keys.is_empty(),
        "sanity: the walk must mint content keys for a real tree"
    );
    assert!(
        keys.iter().any(|k| k.starts_with("gc/")),
        "sanity: the code half of the derived index must have minted keys; got {keys:?}"
    );
    assert!(
        keys.iter().any(|k| k.starts_with("gd/")),
        "sanity: the design half of the derived index must have minted keys; got {keys:?}"
    );

    let suppressed: BTreeSet<String> = reference_replay_keys(&as_recorded(&recorded));

    assert_eq!(
        suppressed, keys,
        "the reader must recognise EVERY key the writer mints - a key it passes over as malformed \
         silently stops suppressing, and the log resumes growing without bound"
    );
}

/// ROUND-TRIP CONTRACT, generation half: the identity and generation the reader recovers from REAL
/// minted keys must track the file's content, not merely parse. Edit one file and re-walk: the
/// previous generation's recorded keys must suppress NONE of the new batch (so the changed file
/// re-emits), while the untouched file's keys are still suppressed by the very same recorded
/// stream (so it does not). This is the reader's contract over real keys; the RUN-level proof that
/// an edit re-emits exactly that file's batch belongs to criterion 3 and is not duplicated here.
#[cfg(feature = "symbols")]
#[test]
fn a_real_batchs_generation_is_recovered_from_the_key_the_walk_minted() {
    let dir = temp_ingestable_project();
    let root = dir.path();

    let before = minted(root);
    let recorded_before = as_recorded(&before);
    let design_keys: BTreeSet<String> = before
        .iter()
        .map(|(k, _)| k.clone())
        .filter(|k| k.starts_with("gd/"))
        .collect();
    assert!(
        !design_keys.is_empty(),
        "sanity: the untouched design half must have recorded keys to stay suppressed"
    );

    // Change ONLY the source file. Its batch bytes change, so its whole batch hashes to a new
    // generation; the design doc is untouched and keeps its generation.
    std::fs::write(
        root.join("src/alpha.rs"),
        "pub fn alpha_helper() {}\npub fn alpha_caller() { alpha_helper(); }\npub fn added() {}\n",
    )
    .unwrap();

    let after: BTreeSet<String> = minted_keys(root);
    let new_code_keys: BTreeSet<String> = after
        .iter()
        .filter(|k| k.starts_with("gc/"))
        .cloned()
        .collect();
    assert!(
        !new_code_keys.is_empty(),
        "sanity: the edited source file must mint a fresh code batch"
    );

    let suppressed: BTreeSet<String> = reference_replay_keys(&recorded_before);

    assert!(
        new_code_keys.is_disjoint(&suppressed),
        "the edited file's NEW generation must be suppressed by nothing the prior generation \
         recorded; overlap was {:?}",
        new_code_keys.intersection(&suppressed).collect::<Vec<_>>()
    );
    assert!(
        design_keys.is_subset(&suppressed),
        "the untouched design half must stay suppressed by the same recorded stream - one file's \
         new generation must not disturb another file's identity"
    );
}

// The compiled `rigger` binary under test is located at RUNTIME by the shared authority in
// `tests/common`: a path baked in at compile time goes stale the moment the target dir moves,
// and every suite that spawns the product then dies with a bare NotFound.
use crate::common;

#[cfg(feature = "symbols")]
use common::fixtures::minted_events;
use common::fixtures::reference_replay_keys;
