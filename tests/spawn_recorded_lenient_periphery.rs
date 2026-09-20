//! Periphery (public-API / contract) tests for `rigger::spawn::recorded_lenient` (spec 94
//! criterion 4, adj-u94c4-r3-verdict-reject-recorded-spawn-duplication): the degrade-tolerant
//! sibling of `rigger::spawn::recorded` extracted out of `console::palette_commands`'s own
//! inline parsing loop so both share the ONE `spawn_requested_events` prefilter instead of
//! duplicating it.
//!
//! `recorded_lenient` has exactly one production caller today (`console::palette_commands`),
//! and that caller's own use of it - one malformed entry loses only its own row, a valid
//! entry alongside it survives - is already proven crossing the real exported ABI in
//! `crates/console-core/tests/exported_abi_periphery.rs`
//! (`console_call_palette_commands_degrades_on_a_malformed_recorded_spawn_through_the_public_abi`,
//! `console_call_palette_commands_keeps_a_valid_spawn_alongside_a_malformed_one_through_the_public_abi`).
//! Those tests observe `recorded_lenient` only through `.into_keys()` - `palette_commands`
//! reads ids, never the `SpawnRequest` VALUES the map carries - so the value-carrying half of
//! `recorded_lenient`'s own contract (a re-parked duplicate id collapses to the LAST recorded
//! request, exactly like `recorded`'s own documented fold) has no proof anywhere, at any
//! layer: it is now a standalone `pub fn` on the crate's public surface, callable directly by
//! any future consumer, not merely an implementation detail of one caller's id-listing. This
//! file drives that public surface directly, as an external caller of the `rigger` library
//! would (never through `console::palette_commands`), closing that gap.

use rigger::eventstore::Event;
use rigger::spawn::{self, SpawnRequest, ROLE_IMPLEMENTER};

fn spawn_requested_at(req: &SpawnRequest, position: u64) -> Event {
    let mut e = req
        .to_event()
        .expect("a well-formed SpawnRequest must serialize");
    e.position = position;
    e
}

fn malformed_spawn_requested_at(position: u64) -> Event {
    let mut e = Event::new(spawn::TYPE_SPAWN_REQUESTED, b"not json".to_vec());
    e.position = position;
    e
}

/// `recorded_lenient` skips a malformed `SpawnRequested` body per-event rather than failing
/// the whole fold - the exact contrast with `recorded`'s own `?`, which propagates the same
/// malformed body as an error. Both are proven over the SAME mixed log here, so the divergence
/// is a real, comparable assertion rather than two separate claims.
#[test]
fn recorded_lenient_skips_a_malformed_spawn_and_keeps_a_well_formed_one() {
    let good = SpawnRequest::new("u1", "implement", ROLE_IMPLEMENTER, 0, "do it");
    let events = vec![
        spawn_requested_at(&good, 1),
        malformed_spawn_requested_at(2),
    ];

    assert!(
        spawn::recorded(&events).is_err(),
        "recorded must still propagate a malformed spawn body as an error - the contract \
         recorded_lenient is the deliberate exception to, not a relaxation of it"
    );

    let lenient = spawn::recorded_lenient(&events);
    assert_eq!(
        lenient.len(),
        1,
        "a malformed entry must lose only its own row: {lenient:?}"
    );
    assert_eq!(
        lenient.get("u1/implementer#0"),
        Some(&good),
        "the well-formed entry must survive alongside the dropped malformed one: {lenient:?}"
    );
}

/// On a log with NO malformed entries, `recorded_lenient` agrees with `recorded` exactly -
/// same ids, same values - proving the extraction (spec 94 c4: replacing the inlined loop
/// with a function sharing `recorded`'s own `spawn_requested_events` prefilter) preserves
/// `recorded`'s fold rather than silently diverging from it on the happy path.
#[test]
fn recorded_lenient_agrees_with_recorded_on_a_well_formed_log() {
    let a = SpawnRequest::new("u1", "implement", ROLE_IMPLEMENTER, 0, "do it");
    let b = SpawnRequest::new("u2", "implement", ROLE_IMPLEMENTER, 0, "do it too");
    let events = vec![spawn_requested_at(&a, 1), spawn_requested_at(&b, 2)];

    let strict = spawn::recorded(&events).expect("a well-formed log must not error");
    let lenient = spawn::recorded_lenient(&events);
    assert_eq!(
        lenient, strict,
        "recorded_lenient must answer the identical map to recorded when nothing is \
         malformed - a real divergence here would mean the extraction changed the fold, not \
         merely its error handling"
    );
}

/// `recorded_lenient` collapses a re-parked duplicate id to the LAST recorded request - the
/// SAME invariant `recorded`'s own doc states ("a duplicate id ... collapses to the
/// last-written request") - proven directly against the VALUE the map carries, not merely
/// that exactly one entry survives (which a first-write-wins fold would satisfy just as
/// well). `console::palette_commands`, the sole current caller, never observes this half of
/// the contract at all: it reads only `.into_keys()`, so this is the one place it is pinned.
#[test]
fn recorded_lenient_collapses_a_re_parked_duplicate_id_to_the_last_recorded_request() {
    let first = SpawnRequest::new("u1", "implement", ROLE_IMPLEMENTER, 0, "first prompt");
    let last = SpawnRequest::new("u1", "implement", ROLE_IMPLEMENTER, 0, "last prompt");
    assert_eq!(
        first.id, last.id,
        "test setup must actually re-park the SAME deterministic id"
    );
    assert_ne!(
        first.prompt, last.prompt,
        "test setup must make the two re-parked requests distinguishable by value"
    );
    let events = vec![spawn_requested_at(&first, 1), spawn_requested_at(&last, 2)];

    let lenient = spawn::recorded_lenient(&events);
    assert_eq!(
        lenient.len(),
        1,
        "a re-parked id must collapse to one entry: {lenient:?}"
    );
    assert_eq!(
        lenient.get("u1/implementer#0"),
        Some(&last),
        "the surviving entry must be the LAST recorded request, not the first: {lenient:?}"
    );
}
