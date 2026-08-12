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
//!    `project_scoped_replay_keys`) are now PUBLIC API with an EXTERNAL consumer - `src/main.rs`
//!    reaches them as `rigger::ingest::...` across the crate boundary. Nothing drove them from
//!    outside the crate.
//! 2. The `<prefix>/<file>@<hash>#<i>` content key gained a READER (`derived_key_parts`) beside
//!    its long-standing WRITER (`key_batch`). A hand-spelled key cannot prove the reader accepts
//!    what the writer actually mints; if the two ever drift, the predicate degrades SILENTLY to
//!    "suppresses nothing" and the log resumes growing without bound. These tests round-trip REAL
//!    minted keys - including for paths that carry the key format's own `@` separator, a shape
//!    only a real walk over a real tree can prove the writer ever mints.
//! 3. `cmd_graph_build` - the SECOND dedup sink, which this criterion owns alongside the run's -
//!    now seeds from that same predicate. Its type-first behaviour is only observable end to end,
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
use rigger::ingest::{is_derived_index_type, project_scoped_replay_keys, DERIVED_INDEX_TYPES};
use std::collections::BTreeSet;

/// An event of `type_` carrying `key` in the replay-key metadata slot - the exact shape both ingest
/// sinks stamp on what they append, built here through the crate's public `Event` API so the test
/// pins the recorded form rather than an in-crate helper. The slot is addressed through
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
        project_scoped_replay_keys(&stream).is_empty(),
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

    let first = project_scoped_replay_keys(&stream);
    let second = project_scoped_replay_keys(&stream);

    assert_eq!(
        first, second,
        "two reads of one stream must reach the same suppression set - the property that keeps a \
         run and a cold build from disagreeing about what a fresh emit is redundant against"
    );
    assert!(
        project_scoped_replay_keys(&[]).is_empty(),
        "an empty stream suppresses nothing"
    );
    assert!(
        project_scoped_replay_keys(&[Event::new(TYPE_CODE_ENTITY_EXTRACTED, Vec::new())])
            .is_empty(),
        "a derived event with no replay key at all names no generation, so it suppresses nothing"
    );
}

/// CONTRACT: a RUN BOUNDARY in the stream is not a boundary to this predicate - it is what
/// PROJECT-scoped means, stated at the public API every consumer reaches it through.
///
/// Both sinks hand this function a WHOLE multi-run stream and take its answer as "what is already
/// recorded". The word that makes that safe is project-scoped: a file's content hash does not
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

    let seed: BTreeSet<String> = project_scoped_replay_keys(&stream).into_iter().collect();

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
        let seen: BTreeSet<String> = project_scoped_replay_keys(&[event]).into_iter().collect();
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
    let _ = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(root)
        .status();
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

/// A throwaway project whose sources sit under DIRECTORIES WHOSE NAMES CONTAIN `@` - the shape a
/// vendored, version-pinned or scoped dependency folder really takes on disk, and one rigger must
/// handle because it indexes whatever project it is pointed at. The two files share the whole span
/// to the LEFT of their first `@` and differ only to its right, so any left-to-right read of the
/// content key collapses them onto a single batch identity.
#[cfg(feature = "symbols")]
fn temp_project_with_at_signs_in_its_paths() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let _ = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(root)
        .status();
    std::fs::create_dir_all(root.join(".rigger")).unwrap();
    for (pkg, name) in [("pkg@1.2.3", "alpha"), ("pkg@4.5.6", "beta")] {
        let package = root.join("src").join(pkg);
        std::fs::create_dir_all(&package).unwrap();
        std::fs::write(
            package.join("lib.rs"),
            format!("pub fn {name}_helper() {{}}\npub fn {name}_caller() {{ {name}_helper(); }}\n"),
        )
        .unwrap();
    }
    dir
}

/// Every `<prefix>/<file>@<hash>#<i>` key the SHIPPED walk-and-key authority mints for the tree at
/// `root`, paired with the event type it was minted for. This is the WRITER half of the round-trip:
/// it is the same public entry `rigger graph build` and the run's ingest both drive, so these are
/// the real keys the log will carry - never a spelling this test invented.
#[cfg(feature = "symbols")]
fn minted(root: &std::path::Path) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    rigger::ingest::ingest_project_batched(root.to_str().unwrap(), |batch| {
        for (key, ev) in batch {
            out.push((key.clone(), ev.type_.clone()));
        }
    });
    out
}

/// The minted keys as a set, for comparison against what the predicate hands a sink back.
#[cfg(feature = "symbols")]
fn minted_keys(root: &std::path::Path) -> BTreeSet<String> {
    minted(root).into_iter().map(|(k, _)| k).collect()
}

/// The recorded events, replayed as a sink would see them: each minted key stamped onto an event of
/// the type it was minted for - byte-for-byte the shape both sinks append (`with_meta(replay_key)`).
#[cfg(feature = "symbols")]
fn as_recorded(minted: &[(String, String)]) -> Vec<Event> {
    minted.iter().map(|(k, t)| keyed(t, k)).collect()
}

/// ROUND-TRIP CONTRACT between the key WRITER and the new key READER, over REAL keys.
///
/// `key_batch` mints `<prefix>/<file>@<hash>#<i>`; `project_scoped_replay_keys` now parses that
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

    let suppressed: BTreeSet<String> = project_scoped_replay_keys(&as_recorded(&recorded))
        .into_iter()
        .collect();

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

    let suppressed: BTreeSet<String> = project_scoped_replay_keys(&recorded_before)
        .into_iter()
        .collect();

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

/// ROUND-TRIP CONTRACT for the one path shape the key format is AMBIGUOUS about, over keys the
/// shipped walk really mints, and then through the shipped binary.
///
/// `<prefix>/<file>@<hash>#<i>` reuses `@` and `#` as separators, and a project's own file paths
/// may contain both. The reader therefore splits from the RIGHT, at the writer's own trailing
/// separators, so the batch identity is the WHOLE `<prefix>/<file>` span. The unit layer pins that
/// rule against keys a human typed, which can only prove the reader is self-consistent: it cannot
/// prove the WRITER ever mints a key of that shape, so a hand-spelled case could be guarding a
/// shape that never occurs on any real tree. This drives the SHIPPED walk over a real tree whose
/// directory names carry `@`, so the keys under test are minted, not invented.
///
/// The failure it guards is a LOST FILE, not a noisy log. Read left to right, both files below cut
/// at their first `@`, collapsing onto the single identity `gc/src/pkg`; their (mis-parsed)
/// generations then differ, so the later file RETIRES the earlier file's keys, the earlier file's
/// batch stops being suppressed, and every subsequent build re-appends it forever. The two
/// assertions catch exactly that: the predicate must return every minted key, and a re-build over
/// the byte-identical tree must append nothing.
#[cfg(feature = "symbols")]
#[test]
fn two_real_files_under_at_sign_paths_keep_two_batch_identities_end_to_end() {
    let dir = temp_project_with_at_signs_in_its_paths();
    let root = dir.path();

    let recorded = minted(root);
    let keys: BTreeSet<String> = recorded.iter().map(|(k, _)| k.clone()).collect();
    assert!(
        keys.iter().any(|k| k.contains("pkg@1.2.3"))
            && keys.iter().any(|k| k.contains("pkg@4.5.6")),
        "sanity: the shipped walk must really mint content keys for paths containing `@` - the \
         whole premise of splitting the key from the right; got {keys:?}"
    );

    let suppressed: BTreeSet<String> = project_scoped_replay_keys(&as_recorded(&recorded))
        .into_iter()
        .collect();
    assert_eq!(
        suppressed, keys,
        "every file's own minted keys must survive: no file's batch may retire another's because \
         their paths happen to share a span to the left of an `@`"
    );

    // The consequence, through the SHIPPED binary: two builds over a byte-identical tree, and the
    // second appends nothing. Were the two files reading as one identity, the retired file's batch
    // would be unsuppressed and re-appended on this build and on every build after it.
    let (out, err, ok) = run_rigger(root, &["graph", "build"]);
    assert!(ok, "graph build must succeed; stderr: {err}; stdout: {out}");
    assert!(
        ingested_count(&out) > 0,
        "sanity: the first build must ingest these files; got:\n{out}"
    );
    let (out2, err2, ok2) = run_rigger(root, &["graph", "build"]);
    assert!(ok2, "a second graph build must succeed; stderr: {err2}");
    assert_eq!(
        ingested_count(&out2),
        0,
        "a re-build over a byte-identical tree whose paths contain `@` must append NOTHING; got:\n{out2}"
    );
}

/// The project identity the binary resolves for `root`, mirrored here so a seed lands in the exact
/// namespaced run stream the compiled binary reads back: the tracked `.rigger/project.id` at the
/// git top-level when present, else the git top-level basename, else `root`'s own basename.
#[cfg(feature = "symbols")]
fn run_stream_identity(root: &std::path::Path) -> String {
    let toplevel = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());
    let base = toplevel
        .as_deref()
        .map(std::path::Path::new)
        .unwrap_or(root);
    if let Ok(raw) = std::fs::read_to_string(base.join(".rigger").join("project.id")) {
        let id = raw.trim();
        if !id.is_empty() {
            return id.to_string();
        }
    }
    base.file_name()
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty())
        .map(String::from)
        .unwrap_or_else(|| "rigger".to_string())
}

/// Append events directly to the namespaced run stream under `root` - the store the compiled binary
/// opens - standing in for facts a prior run recorded.
#[cfg(feature = "symbols")]
fn seed_run_stream(root: &std::path::Path, events: &[Event]) {
    use rigger::eventstore::namespace::Namespaced;
    use rigger::eventstore::sqlite::Store;
    use rigger::eventstore::{EventStore, ExpectedRevision};

    let backend = Store::open(
        root.join(".rigger")
            .join("events.db")
            .to_str()
            .expect("a utf-8 store path"),
    )
    .unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    store
        .append(rigger::conductor::STREAM, ExpectedRevision::Any, events)
        .unwrap();
}

/// The whole namespaced run stream under `root`, read back the way the binary reads it.
#[cfg(feature = "symbols")]
fn read_run_stream(root: &std::path::Path) -> Vec<Event> {
    use rigger::eventstore::namespace::Namespaced;
    use rigger::eventstore::sqlite::Store;
    use rigger::eventstore::{Direction, EventStore};

    let backend = Store::open(
        root.join(".rigger")
            .join("events.db")
            .to_str()
            .expect("a utf-8 store path"),
    )
    .unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    store
        .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
        .unwrap()
}

/// Run `rigger <args...>` in `cwd` through the COMPILED binary, returning (stdout, stderr, success).
#[cfg(feature = "symbols")]
fn run_rigger(cwd: &std::path::Path, args: &[&str]) -> (String, String, bool) {
    let state = tempfile::tempdir().expect("a temp XDG_STATE_HOME for the rigger invocation");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_rigger"))
        .args(args)
        .current_dir(cwd)
        // Never let a short-lived integration invocation spawn a real dashboard, and never let it
        // register a phantom instance in the operator's real machine-global registry.
        .env("RIGGER_NO_DASH", "1")
        .env("XDG_STATE_HOME", state.path())
        .output()
        .expect("failed to spawn the rigger binary");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

/// How many events `rigger graph build` reported ingesting, parsed from the line it prints - the
/// shipped observable for "this build appended nothing".
#[cfg(feature = "symbols")]
fn ingested_count(stdout: &str) -> usize {
    stdout
        .split_once("ingested ")
        .and_then(|(_, rest)| rest.split_whitespace().next())
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("graph build must report its ingested count; got:\n{stdout}"))
}

/// INTEGRATION, at the crate boundary the binary crosses: the cold `graph build` sink and the run's
/// seeding are two processes that must agree, and this criterion makes them agree by sharing ONE
/// predicate. Prove the agreement without driving a run: after the SHIPPED build records the tree,
/// the predicate over the resulting log returns EXACTLY the key set the walk mints for that tree -
/// so a run seeding from that same log would find every one of its own emits already suppressed and
/// append nothing. A second build then appends nothing either, at both halves of the index.
///
/// The pre-existing cold-build test counts one event type across a re-build; this pins the WHOLE
/// derived index and the seeding equivalence that makes the two sinks interchangeable.
#[cfg(feature = "symbols")]
#[test]
fn a_cold_build_leaves_the_shared_seeding_with_nothing_left_to_ingest() {
    let dir = temp_ingestable_project();
    let root = dir.path();

    let (out, err, ok) = run_rigger(root, &["graph", "build"]);
    assert!(ok, "graph build must succeed; stderr: {err}; stdout: {out}");
    assert!(
        ingested_count(&out) > 0,
        "sanity: the first build over a fresh tree must ingest the derived index; got:\n{out}"
    );

    let log = read_run_stream(root);
    let suppressed: BTreeSet<String> = project_scoped_replay_keys(&log).into_iter().collect();
    assert_eq!(
        suppressed,
        minted_keys(root),
        "the shared predicate over the log a cold build wrote must return exactly the keys the walk \
         mints for that tree - that equality IS the build/run seeding equivalence, and it is what \
         lets a run inherit a cold build's work instead of re-appending the whole index"
    );

    let derived_before = log
        .iter()
        .filter(|e| is_derived_index_type(&e.type_))
        .count();
    let (out2, err2, ok2) = run_rigger(root, &["graph", "build"]);
    assert!(ok2, "a second graph build must succeed; stderr: {err2}");
    assert_eq!(
        ingested_count(&out2),
        0,
        "a re-build over a byte-identical tree must append NOTHING; got:\n{out2}"
    );
    assert_eq!(
        read_run_stream(root)
            .iter()
            .filter(|e| is_derived_index_type(&e.type_))
            .count(),
        derived_before,
        "the whole derived-index slice of the log must be unchanged by the second build"
    );
}

/// INTEGRATION through the SHIPPED binary: the cold build's seen-set is TYPE-FIRST, so a domain
/// event can never suppress a project fact.
///
/// This is the boundary the old seeding got wrong and no test could see: `cmd_graph_build` used to
/// collect EVERY recorded event's replay key with no type test, so a domain event whose key was
/// spelled like a content key would make the build skip that file's batch entirely - silently
/// losing a file from the graph, with a clean exit and a truthful-looking "ingested 0".
///
/// The proof pre-claims a not-yet-ingested file's REAL keys with domain events (the keys are taken
/// from the shipped walk, so they are exactly what the build is about to mint), then runs the build
/// and asserts the file was ingested anyway and its entities are readable back through the shipped
/// inspector. Under a type-blind seen-set this reddens: the build reports 0 and `beta.rs` is absent.
#[cfg(feature = "symbols")]
#[test]
fn a_domain_events_replay_key_never_suppresses_the_shipped_builds_ingest() {
    let dir = temp_ingestable_project();
    let root = dir.path();

    let (out, err, ok) = run_rigger(root, &["graph", "build"]);
    assert!(
        ok,
        "the first graph build must succeed; stderr: {err}; stdout: {out}"
    );

    // A file the build has NOT yet seen, whose keys we can therefore pre-claim.
    std::fs::write(
        root.join("src/beta.rs"),
        "pub fn beta_helper() {}\npub fn beta_caller() { beta_helper(); }\n",
    )
    .unwrap();
    let beta: Vec<(String, String)> = minted(root)
        .into_iter()
        .filter(|(k, _)| k.contains("beta.rs"))
        .collect();
    assert!(
        !beta.is_empty(),
        "sanity: the new source file must mint content keys of its own"
    );

    // Pre-claim every one of beta's keys with a DOMAIN event. Only the type test stands between
    // this residue and a lost file.
    let residue: Vec<Event> = beta
        .iter()
        .map(|(k, _)| keyed(TYPE_REVIEW_FINDING, k))
        .collect();
    seed_run_stream(root, &residue);

    let (out2, err2, ok2) = run_rigger(root, &["graph", "build"]);
    assert!(
        ok2,
        "the second graph build must succeed; stderr: {err2}; stdout: {out2}"
    );
    assert!(
        ingested_count(&out2) > 0,
        "the new file must be ingested despite domain events pre-claiming its content keys; got:\n{out2}"
    );

    let recorded: BTreeSet<String> = read_run_stream(root)
        .iter()
        .filter(|e| is_derived_index_type(&e.type_))
        .filter_map(|e| e.meta.get(rigger::conductor::META_REPLAY_KEY).cloned())
        .collect();
    let beta_keys: BTreeSet<String> = beta.iter().map(|(k, _)| k.clone()).collect();
    assert!(
        beta_keys.is_subset(&recorded),
        "every one of the new file's content keys must be recorded on a DERIVED event; missing {:?}",
        beta_keys.difference(&recorded).collect::<Vec<_>>()
    );

    // And it is readable back through the shipped inspector, so the file really reached the graph.
    let (g, gerr, gok) = run_rigger(root, &["graph", "--around", "src/beta.rs::beta_helper"]);
    assert!(
        gok,
        "graph --around must succeed after a build; stderr: {gerr}"
    );
    assert!(
        g.contains("beta_caller"),
        "the pre-claimed file's entities must be folded into the graph; got:\n{g}"
    );
}

/// Every `<prefix>/<file>@<hash>#<i>` key the log has EVER recorded on a derived-index event under
/// `root` - across generations, so a superseded generation is still in here. This is deliberately
/// NOT `project_scoped_replay_keys`: that returns the LIVE set (each file's latest generation
/// only), and the difference between the two is exactly what "only what changed is re-emitted"
/// and "the log holds the latest generation in full" are claims about.
#[cfg(feature = "symbols")]
fn recorded_derived_keys(root: &std::path::Path) -> BTreeSet<String> {
    read_run_stream(root)
        .iter()
        .filter(|e| is_derived_index_type(&e.type_))
        .filter_map(|e| e.meta.get(rigger::conductor::META_REPLAY_KEY).cloned())
        .collect()
}

/// How many derived-index EVENTS the log under `root` carries - counted, not deduplicated by key.
/// A re-append of a key the log already holds is invisible to the key SET and visible only here, so
/// "only what changed is re-emitted" has to be measured against this and not against the set.
#[cfg(feature = "symbols")]
fn recorded_derived_events(root: &std::path::Path) -> usize {
    read_run_stream(root)
        .iter()
        .filter(|e| is_derived_index_type(&e.type_))
        .count()
}

/// INTEGRATION through the SHIPPED binary, over a MIX of skipping and re-ingest - the one shape
/// this criterion's net contract is stated against and the one shape nothing drove.
///
/// The contract this diff writes at all three sites it owns (`docs/architecture.md` 5.5, the run
/// sink's comment in `src/conductor.rs`, and `cmd_graph_build`'s rustdoc in `src/main.rs`) is
/// LOG-relative and has two halves: after any mix of skipping and re-ingest, the log holds each
/// file's LATEST content generation AS THE WALK LOWERED IT in full, and only what changed is ever
/// re-emitted. The fixture below has no persisted symbols index, so the walk's view IS the tree's
/// here and the qualifier is satisfied trivially - it is carried because the three sites state it,
/// not because this test distinguishes the two.
/// Every other test here sees a single content generation per file - a fresh build, a re-build over
/// a byte-identical tree, or keys pre-claimed before their file was ever ingested - so the half of
/// the predicate that RETIRES a file's earlier generation when a later one is recorded is driven
/// only by hand-spelled keys in the unit layer. That is precisely the boundary this test crosses:
/// real keys the shipped walk minted, over a tree where one file moved generation and another did
/// not, read back through the shipped binary's own store.
///
/// Both halves fail SILENTLY without it. If the predicate stopped retiring an earlier generation
/// (it returned every key ever recorded rather than the latest per file), the live set would grow a
/// stale generation the tree no longer holds; the suite would stay green because no other test ever
/// records two generations of one file, and a file reverted to earlier content would then be
/// suppressed forever - the log would look complete while the graph stayed on the superseded
/// version. If suppression were all-or-nothing rather than per-file, the second build would append
/// nothing at all and the edited file's new generation would never reach the log.
#[cfg(feature = "symbols")]
#[test]
fn a_mixed_build_holds_every_files_latest_generation_and_re_emits_only_what_changed() {
    let dir = temp_ingestable_project();
    let root = dir.path();

    // A second source file, so a later edit to ONE of them is a genuine MIX: one file's batch is
    // skipped while the other's is re-emitted, in the same build.
    std::fs::write(
        root.join("src/beta.rs"),
        "pub fn beta_helper() {}\npub fn beta_caller() { beta_helper(); }\n",
    )
    .unwrap();

    let (out, err, ok) = run_rigger(root, &["graph", "build"]);
    assert!(
        ok,
        "the first graph build must succeed; stderr: {err}; stdout: {out}"
    );
    assert!(
        ingested_count(&out) > 0,
        "sanity: the first build over a fresh tree must ingest the derived index; got:\n{out}"
    );
    let recorded_before = recorded_derived_keys(root);
    let events_before = recorded_derived_events(root);
    assert_eq!(
        recorded_before,
        minted_keys(root),
        "sanity: before the edit the log holds exactly the tree's one generation"
    );

    // Move exactly ONE file's content generation. `beta.rs` and the design doc are untouched, so
    // their batches must be skipped while `alpha.rs`'s must be re-emitted whole.
    std::fs::write(
        root.join("src/alpha.rs"),
        "pub fn alpha_helper() {}\npub fn alpha_caller() { alpha_helper(); }\npub fn alpha_extra() { alpha_caller(); }\n",
    )
    .unwrap();
    let tree_now = minted_keys(root);
    assert_ne!(
        tree_now, recorded_before,
        "sanity: the edit must move a content generation, else there is no mix to drive"
    );

    let (out2, err2, ok2) = run_rigger(root, &["graph", "build"]);
    assert!(
        ok2,
        "the build over the edited tree must succeed; stderr: {err2}; stdout: {out2}"
    );
    assert!(
        ingested_count(&out2) > 0,
        "the edited file's batch must be re-emitted, so the build cannot report nothing; got:\n{out2}"
    );

    // HALF ONE - only what changed was re-emitted. Every key this build ADDED to the
    // log must belong to the file that moved; a key appended for an unchanged file would mean the
    // skip did not happen and the log grows on every build, which is the defect this criterion ends.
    let recorded_after = recorded_derived_keys(root);
    let appended: BTreeSet<&String> = recorded_after.difference(&recorded_before).collect();
    assert!(
        !appended.is_empty(),
        "the edited file's new generation must reach the log"
    );
    assert!(
        appended.iter().all(|k| k.contains("alpha.rs")),
        "only the file whose content changed may be re-emitted; these keys were appended for \
         unchanged files: {:?}",
        appended
            .iter()
            .filter(|k| !k.contains("alpha.rs"))
            .collect::<Vec<_>>()
    );
    // Measured by EVENT COUNT, because a re-append of a key the log already holds does not change
    // the key SET at all: an unchanged file whose whole batch is re-appended is invisible to the
    // assertion above and visible only here. The log may grow by exactly the events carrying keys
    // it did not already hold, and by nothing else.
    assert_eq!(
        recorded_derived_events(root) - events_before,
        appended.len(),
        "the log may grow ONLY by the keys it did not already hold; it grew by {} derived events \
         while only {} of them carry a key the log was missing, so a batch already wholly recorded \
         was re-appended",
        recorded_derived_events(root) - events_before,
        appended.len()
    );

    // HALF TWO - the log holds each file's LATEST generation in full, and holds no earlier one as
    // live. The predicate over the log is the shipped read of "what is already recorded", so this
    // equality is the net contract itself: the edited file's superseded generation is retired, the
    // skipped files' generations are still there whole, and nothing else is live.
    let live: BTreeSet<String> = project_scoped_replay_keys(&read_run_stream(root))
        .into_iter()
        .collect();
    assert_eq!(
        live, tree_now,
        "after a mix of skipping and re-ingest the live suppression set must be exactly the tree's \
         latest generation - no retired generation left live, no skipped file's keys dropped"
    );

    // And the mix settles: a further build over the now-unchanged tree appends nothing at all, so
    // the re-ingest left the log in the same steady state a cold build reaches.
    let (out3, err3, ok3) = run_rigger(root, &["graph", "build"]);
    assert!(
        ok3,
        "the settling build must succeed; stderr: {err3}; stdout: {out3}"
    );
    assert_eq!(
        ingested_count(&out3),
        0,
        "once the edited file's generation is recorded, a further build must append NOTHING; \
         got:\n{out3}"
    );
}
