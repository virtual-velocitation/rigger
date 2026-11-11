//! Periphery (contract / API) tests for spec 29a criterion 1: code structure lowered AS EVENTS.
//! These run OUTSIDE the crate, over the library's public surface, and pin the emit API's
//! determinism-and-ordering contract the doc comments promise: `project_batches` yields
//! byte-identical events for identical source, definitions precede references, and
//! `extract_events` emits one event per path and reference, each threading its file's label.
//! They run the real extraction pass, so they live in the `symbols` lane only.

#![cfg(feature = "symbols")]

use crate::common;

use rigger::contextgraph::{TYPE_CODE_ENTITY_EXTRACTED, TYPE_EDGE_INFERRED};

#[test]
fn the_emit_api_is_deterministic_and_emits_definitions_before_references() {
    // Drive the real extraction pass over a source file with two definitions and two same-file
    // references, then lower it through the public emit API.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("combat.rs"),
        "fn apply_damage() {}\nfn heal() {}\nfn caller() { apply_damage(); heal(); }\n",
    )
    .unwrap();
    // Determinism-by-construction (the doc contract "identical source yields byte-identical
    // events"): index the SAME source twice and emit each - the two event streams are byte-
    // identical in type and payload. This exercises the full source -> extraction -> emit
    // pipeline, so a non-deterministic iteration order anywhere in it (e.g. a HashMap) would break
    // this, and with it the reproducible-rebuild guarantee spec 29a rests on.
    let first = common::project_events(dir.path().to_str().unwrap());
    let second = common::project_events(dir.path().to_str().unwrap());
    let shape = |evs: &[rigger::eventstore::Event]| {
        evs.iter()
            .map(|e| (e.type_.clone(), e.data.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        shape(&first),
        shape(&second),
        "project_batches must be deterministic for identical source"
    );
    assert!(
        !first.is_empty(),
        "the extraction pass emitted events for a non-trivial source file"
    );

    // Ordering contract ("Definitions are emitted before references"): for a single file no
    // reference event precedes a definition event, so the fold can land a same-file reference on
    // its already-folded definition entity.
    let last_def = first
        .iter()
        .rposition(|e| e.type_ == TYPE_CODE_ENTITY_EXTRACTED);
    let first_ref = first.iter().position(|e| e.type_ == TYPE_EDGE_INFERRED);
    match (last_def, first_ref) {
        (Some(ld), Some(fr)) => assert!(
            ld < fr,
            "every definition precedes every reference; defs end at {ld}, refs start at {fr}"
        ),
        other => panic!(
            "expected both a definition and a reference event; got {other:?} over types {:?}",
            first.iter().map(|e| e.type_.clone()).collect::<Vec<_>>()
        ),
    }
}

#[test]
fn extract_events_emits_one_event_per_path_and_reference_threading_the_file_label() {
    use rigger::grounder::symbols::build_index;
    use rigger::grounder::symbols::events::extract_events;

    // Drive the per-file emit API DIRECTLY - `project_batches` only reaches `extract_events`
    // transitively, so its own contract is otherwise unpinned. The pass emits one
    // CodeEntityExtracted per path (spec 119, ONE PATH, ONE EVENT) and one EdgeInferred per
    // reference, besides a module row per out-of-line `mod` declaration (this source declares
    // none); that CARDINALITY is the core of the extract-as-events pass, yet no other periphery
    // test asserts it (the determinism test checks only non-empty + defs-before-refs). A regression
    // that dropped, doubled, or mis-typed a def/ref event would fold a wrong graph - and, once the
    // rebuild criterion replays the log, a wrong REBUILT graph - while every existing test stayed
    // green. This ties the emitted per-type counts to the extracted `FileSymbols` as ground truth
    // and pins that `extract_events` threads its `file` argument onto every payload.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("combat.rs"),
        "fn apply_damage() {}\nfn heal() {}\nfn caller() { apply_damage(); heal(); }\n",
    )
    .unwrap();
    let idx = build_index(dir.path().to_str().unwrap(), None);
    let (_, fs) = idx
        .files()
        .iter()
        .next()
        .expect("the extraction pass indexed the one source file");
    assert!(
        !fs.defs.is_empty() && !fs.refs.is_empty(),
        "the fixture must carry both definitions and references so the cardinality assertions are \
         non-vacuous; got {} defs, {} refs",
        fs.defs.len(),
        fs.refs.len()
    );

    // Pass a synthetic file label distinct from the temp path so the threading assertion pins the
    // `file` argument, not an incidental match against the path the index was built from.
    let label = "src/synthetic/combat.rs";
    let events = extract_events(label, fs);

    let defs = events
        .iter()
        .filter(|e| e.type_ == TYPE_CODE_ENTITY_EXTRACTED)
        .count();
    let refs = events
        .iter()
        .filter(|e| e.type_ == TYPE_EDGE_INFERRED)
        .count();
    let paths = fs
        .defs
        .iter()
        .filter(|d| !d.is_test)
        .map(|d| d.path.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    assert_eq!(
        defs, paths,
        "exactly one CodeEntityExtracted per extracted path; got {defs} for {paths} paths"
    );
    assert_eq!(
        refs,
        fs.refs.len(),
        "exactly one EdgeInferred per extracted reference; got {refs} for {} refs",
        fs.refs.len()
    );
    assert_eq!(
        events.len(),
        paths + fs.refs.len(),
        "extract_events emits exactly one event per path and per reference, nothing spurious; got \
         {} events",
        events.len()
    );

    // File-threading + serialized-form contract: every emitted payload carries the label under the
    // `file` key, read from the raw on-log JSON (not the in-crate struct) so the wire field name is
    // pinned too - a rebuild replays exactly these bytes.
    for e in &events {
        let v: serde_json::Value = serde_json::from_slice(&e.data).unwrap();
        assert_eq!(
            v.get("file").and_then(|f| f.as_str()),
            Some(label),
            "every emitted payload threads the file argument under the `file` key; got {v}"
        );
    }

    // Per-file ordering asserted directly on `extract_events` (the determinism test only exercises
    // `project_batches`): every definition event precedes every reference event, so a same-file
    // reference folds onto an already-folded definition entity.
    let last_def = events
        .iter()
        .rposition(|e| e.type_ == TYPE_CODE_ENTITY_EXTRACTED)
        .expect("a definition event was emitted");
    let first_ref = events
        .iter()
        .position(|e| e.type_ == TYPE_EDGE_INFERRED)
        .expect("a reference event was emitted");
    assert!(
        last_def < first_ref,
        "every definition precedes every reference; defs end at {last_def}, refs start at \
         {first_ref}"
    );
}

// ---- spec 29a criterion 2 periphery: the confidence tier at the persisted + integrated boundary ----
