//! Periphery (contract / API / integration) tests for the PUBLISHED content-key split:
//! `rigger::ingest::derived_key_spans`, the one parser of the `<prefix>/<file>@<hash>#<i>` form
//! `rigger::ingest::key_batch` builds.
//!
//! These run OUTSIDE the crate, over the library's PUBLIC surface (`rigger::...`), so a consumer
//! reads the key authority's own parse instead of re-spelling the format. What the inside-out tests
//! are structurally blind to:
//!
//! 1. The published item is new PUBLIC API and nothing outside the crate drives it. Its contract is
//!    not "some split is found" but WHERE the two parts lie: the batch identity STARTS the key and
//!    ends BEFORE the `@` that begins the generation. That one byte is not cosmetic - a second
//!    spelling of the format had already drifted at exactly that boundary while an `is_some()`
//!    assertion stayed green over both - so the offsets are what an external test has to pin.
//! 2. `derived_key_spans` and the private slice view the suppression predicate uses are claimed to
//!    be ONE parse with two views. From outside, that claim is only observable as an agreement:
//!    the generations `project_scoped_latest_generations` retires and keeps must be the generations the
//!    published spans cut. A second parser hiding behind the predicate would satisfy every
//!    hand-spelled unit test and disagree here.
//!
//! Scope. What is proved here is the PUBLISHED SPLIT. The whole-stream seeding rule and its two
//! sinks are covered by `tests/dedup_seeding_periphery.rs` and are not duplicated.
//!
//! Everything but the real-tree walk is compiled unconditionally, so this suite runs in BOTH
//! feature lanes; the two tests that need a tree to walk are `symbols`-gated exactly as the walk is.

mod common;

#[cfg(feature = "symbols")]
use common::fixtures::minted_events as minted;
use common::fixtures::reference_replay_keys;
use std::collections::{BTreeMap, BTreeSet};

use rigger::contextgraph::{TYPE_CODE_ENTITY_EXTRACTED, TYPE_EDGE_INFERRED};
use rigger::eventstore::Event;
use rigger::ingest::{derived_key_spans, META_REPLAY_KEY};

/// The published spans as the two slices they name, for readable assertions. It cuts what
/// `derived_key_spans` returns and computes nothing of its own, so a disagreement between this and
/// the key it was handed is the parser's, never this helper's.
fn split(key: &str) -> Option<(&str, &str)> {
    let (identity, generation) = derived_key_spans(key)?;
    Some((&key[identity], &key[generation]))
}

/// An event of `type_` carrying `key` in the replay-key metadata slot: the exact recorded shape
/// both ingest sinks append, built through the crate's public `Event` API.
fn keyed(type_: &str, key: &str) -> Event {
    Event::new(type_, b"payload".to_vec()).with_meta(META_REPLAY_KEY, key)
}

/// The replay key an event carries, as the predicate reads it.
fn replay_key(event: &Event) -> &str {
    event
        .meta
        .get(META_REPLAY_KEY)
        .map(String::as_str)
        .expect("every event this suite records carries a replay key")
}

// ---------------------------------------------------------------------------
// The API edge: WHERE the published split says the two parts lie
// ---------------------------------------------------------------------------

/// THE ONE-BYTE BOUNDARY, pinned from outside the crate.
///
/// The batch identity STARTS the key and ends BEFORE the `@` that begins the generation. Both
/// halves are load-bearing and neither is provable by a test that only asks whether a split was
/// found:
///
/// - `identity.start == 0`: every key naming this batch begins with its identity;
/// - ending BEFORE the `@` is where a hand-rolled second spelling of this format had already
///   drifted. The two spellings produce identities differing by exactly one byte, both well formed,
///   and they disagree about which keys belong to a batch.
///
/// The cases below include the shapes the format is ambiguous about on purpose - a path carrying
/// the format's own `@` and `#` separators, which a real tree really produces (a version-pinned
/// vendored directory) - and a multi-byte path, because the ranges are BYTE offsets and slicing one
/// off a character boundary panics rather than answering wrongly.
#[test]
fn the_batch_identity_starts_the_key_and_ends_before_the_at_sign() {
    let cases = [
        ("gc/src/alpha.rs@h1#0", "gc/src/alpha.rs", "h1"),
        (
            "gd/specs/sample.md@deadbeef#12",
            "gd/specs/sample.md",
            "deadbeef",
        ),
        // The `<file>` span carries the format's own separators: split from the RIGHT.
        (
            "gc/src/pkg@1.2.3/lib.rs@h9#3",
            "gc/src/pkg@1.2.3/lib.rs",
            "h9",
        ),
        ("gd/a#1/b@2/c.md@cafe#7", "gd/a#1/b@2/c.md", "cafe"),
        // A multi-byte path: the returned offsets must land on character boundaries.
        (
            "gc/src/caf\u{e9}-\u{2192}.rs@h1#0",
            "gc/src/caf\u{e9}-\u{2192}.rs",
            "h1",
        ),
    ];

    for (key, identity, generation) in cases {
        let (spans_identity, spans_generation) = derived_key_spans(key)
            .unwrap_or_else(|| panic!("the key authority's own form must parse: {key}"));

        assert_eq!(
            spans_identity.start, 0,
            "the batch identity must START the key ({key}); a prefix range that does not begin at \
             0 describes no prefix, and a store cannot seek this batch's history with it"
        );
        assert_eq!(
            &key[spans_identity.clone()],
            identity,
            "the batch identity of {key} must be the whole <prefix>/<file> span"
        );
        assert!(
            key.starts_with(&key[spans_identity.clone()]),
            "every key naming this batch must BEGIN with the identity ({key}); that is the property \
             the store's range seek rests on"
        );
        assert!(
            !key[spans_identity.clone()].ends_with('@'),
            "the identity of {key} must end BEFORE the `@` that begins the generation. Ending \
             THROUGH it is the exact one-byte drift a second spelling of this format introduced"
        );
        assert_eq!(
            key.as_bytes().get(spans_identity.end).copied(),
            Some(b'@'),
            "the byte at the identity's end must be the `@` separator itself ({key})"
        );
        assert_eq!(
            spans_generation.start,
            spans_identity.end + 1,
            "the generation must begin immediately after that `@` ({key})"
        );
        assert_eq!(
            &key[spans_generation.clone()],
            generation,
            "the generation of {key} must be the content hash between the `@` and the `#<i>` tail"
        );
        assert_eq!(
            key.as_bytes().get(spans_generation.end).copied(),
            Some(b'#'),
            "the byte at the generation's end must be the `#` that starts the index tail ({key})"
        );
    }
}

/// A key that is not the form the key authority mints names NO generation, and says so as `None`.
///
/// This is the fail-safe direction of the published split, stated at its edge: a key the parser
/// does not recognise carries no content identity, so nothing downstream may suppress an append of
/// it. Every shape below breaks exactly one clause of the form, so a parser that quietly relaxed
/// one clause cannot pass by accident.
#[test]
fn a_key_that_is_not_the_minted_form_names_no_generation() {
    let not_the_form = [
        ("", "empty"),
        ("gc", "no `/` at all"),
        ("no-slash@h1#0", "no `/` before the file span"),
        ("/src/alpha.rs@h1#0", "empty prefix"),
        ("gc/src/alpha.rs@h1", "no `#<i>` tail"),
        ("gc/src/alpha.rs@h1#", "empty index"),
        ("gc/src/alpha.rs@h1#x", "non-digit index"),
        ("gc/src/alpha.rs@h1#0a", "index is not all digits"),
        ("gc/src/alpha.rs#0", "no `@` separating the generation"),
        ("gc/@h1#0", "empty file span"),
        ("gc/src/alpha.rs@#0", "empty generation"),
    ];

    for (key, why) in not_the_form {
        assert_eq!(
            derived_key_spans(key),
            None,
            "a key that is not the minted form must name no generation ({why}): {key:?}"
        );
    }
}

/// ONE PARSE, TWO VIEWS, observed from outside as an AGREEMENT.
///
/// The suppression predicate is documented as cutting its identity and generation from the
/// published spans rather than parsing the key again. A consumer cannot see the private slice view,
/// but it can see the consequence: the keys the predicate keeps must be exactly the keys whose
/// generation, AS THE PUBLISHED SPLIT CUTS IT, is the latest recorded for their identity. A second
/// parser hiding behind the predicate would satisfy every hand-spelled unit test and disagree here
/// the moment the two spellings differed by a byte.
///
/// The expectation is derived from `derived_key_spans` so the test states the agreement rather than
/// re-spelling the format, and it is ALSO pinned against a literal set, so a published split that
/// degraded to `None` everywhere cannot make both sides trivially empty and pass.
#[test]
fn the_suppression_predicate_groups_by_the_published_spans() {
    // One file recorded at two generations (the later one supersedes), a second file at one, and a
    // third whose key carries the format's own separators inside its path.
    let recorded = vec![
        keyed(TYPE_CODE_ENTITY_EXTRACTED, "gc/src/alpha.rs@old#0"),
        keyed(TYPE_EDGE_INFERRED, "gc/src/alpha.rs@old#1"),
        keyed(TYPE_CODE_ENTITY_EXTRACTED, "gd/specs/sample.md@sha#0"),
        keyed(TYPE_CODE_ENTITY_EXTRACTED, "gc/src/pkg@1.2.3/lib.rs@vend#0"),
        keyed(TYPE_CODE_ENTITY_EXTRACTED, "gc/src/alpha.rs@new#0"),
        keyed(TYPE_EDGE_INFERRED, "gc/src/alpha.rs@new#1"),
    ];

    // The latest generation per batch identity, both cut by the published split.
    let mut latest: BTreeMap<&str, &str> = BTreeMap::new();
    for event in &recorded {
        let key = replay_key(event);
        let (identity, generation) = split(key).expect("every key here is the minted form");
        latest.insert(identity, generation);
    }
    let expected_from_the_published_spans: BTreeSet<&str> = recorded
        .iter()
        .map(replay_key)
        .filter(|key| {
            let (identity, generation) = split(key).expect("every key here is the minted form");
            latest.get(identity) == Some(&generation)
        })
        .collect();

    let literal: BTreeSet<&str> = BTreeSet::from([
        "gc/src/alpha.rs@new#0",
        "gc/src/alpha.rs@new#1",
        "gd/specs/sample.md@sha#0",
        "gc/src/pkg@1.2.3/lib.rs@vend#0",
    ]);
    assert_eq!(
        expected_from_the_published_spans, literal,
        "sanity: the spans-derived expectation must be the set a reader can name by eye, so this \
         test cannot pass by making both sides empty"
    );

    let kept: BTreeSet<String> = reference_replay_keys(&recorded);
    let kept: BTreeSet<&str> = kept.iter().map(String::as_str).collect();

    assert_eq!(
        kept, expected_from_the_published_spans,
        "the predicate must group by the SAME identity and generation the published split cuts. \
         One parse, two views: a second parser behind the predicate would disagree here"
    );
    assert!(
        !kept.contains("gc/src/alpha.rs@old#0") && !kept.contains("gc/src/alpha.rs@old#1"),
        "a generation the file has moved past must not be kept - the revert case depends on it"
    );
}

// ---------------------------------------------------------------------------
// Over keys a real walk mints
// ---------------------------------------------------------------------------

/// A throwaway project the walk can lower, including a directory whose name carries the content
/// key's own `@` separator - the shape a version-pinned vendored folder really takes on disk, and
/// the one a hand-spelled key can never prove the WRITER actually mints.
#[cfg(feature = "symbols")]
fn at_sign_source_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join(".rigger")).unwrap();
    std::fs::create_dir_all(root.join("src/pkg@1.2.3")).unwrap();
    std::fs::create_dir_all(root.join("specs")).unwrap();
    std::fs::write(
        root.join("src/alpha.rs"),
        "pub fn alpha_helper() {}\npub fn alpha_caller() { alpha_helper(); }\n",
    )
    .unwrap();
    std::fs::write(
        root.join("src/pkg@1.2.3/lib.rs"),
        "pub fn vendored_helper() {}\npub fn vendored_caller() { vendored_helper(); }\n",
    )
    .unwrap();
    std::fs::write(
        root.join("specs/sample.md"),
        "# sample\n\nA design note the design half of the index can lower.\n",
    )
    .unwrap();
    dir
}

/// THE ROUND TRIP the published split exists to survive, over keys the WRITER really mints.
///
/// A hand-spelled key can only prove the parser is self-consistent. This drives the shipped walk
/// over a real tree - including a path carrying the format's own `@` - and requires, for EVERY key
/// it mints, that the published split recognises it, that the identity ends before the `@`, and
/// that every key of one file's batch shares one identity and one generation.
///
/// The failure it guards is silent in both directions: a parser that stopped recognising minted
/// keys suppresses nothing and the log resumes growing without bound, and one that collapsed two
/// files onto a single identity retires a live file's batch forever.
#[cfg(feature = "symbols")]
#[test]
fn the_published_split_recognises_every_key_a_real_walk_mints() {
    let dir = at_sign_source_project();
    let minted = minted(dir.path());
    assert!(
        !minted.is_empty(),
        "sanity: the walk must mint content keys for a real tree"
    );

    let mut generations: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for (key, _) in &minted {
        let published = derived_key_spans(key);
        assert!(
            published.is_some(),
            "the published split must recognise EVERY key the walk mints; it passed over {key} as \
             malformed, which silently stops all suppression"
        );

        let (identity, generation) = split(key).expect("just asserted it parses");
        assert!(
            !identity.ends_with('@') && key.as_bytes().get(identity.len()).copied() == Some(b'@'),
            "the identity of the minted key {key} must end BEFORE the `@`"
        );
        assert!(
            key.starts_with(identity),
            "every minted key must begin with its own batch identity ({key})"
        );
        generations
            .entry(identity.to_string())
            .or_default()
            .insert(generation.to_string());
    }

    for (identity, seen) in &generations {
        assert_eq!(
            seen.len(),
            1,
            "one walk records one generation per batch identity; {identity} was cut with {seen:?}, \
             so two files have collapsed onto one identity and each retires the other's batch"
        );
    }
    assert!(
        generations.keys().any(|i| i.contains('@')),
        "sanity: the tree under test must really mint a key whose file span carries the format's \
         own `@`; got {:?}",
        generations.keys().collect::<Vec<_>>()
    );
    assert!(
        generations.keys().any(|i| i.starts_with("gc/"))
            && generations.keys().any(|i| i.starts_with("gd/")),
        "sanity: both halves of the derived index must have minted keys; got {:?}",
        generations.keys().collect::<Vec<_>>()
    );
}
