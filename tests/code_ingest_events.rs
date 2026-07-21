//! Periphery (contract / API / integration) tests for spec 29a criterion 1: code structure
//! ingested AS EVENTS. These run OUTSIDE the crate, over the library's public surface, so they
//! guard the boundary the inside-out fold / emit unit tests are structurally blind to:
//!
//! - the SERIALIZED-FORM back-compat contract of the two new event types. A `CodeEntityExtracted`
//!   / `EdgeInferred` payload carries `lang` behind `#[serde(default)]`, so a historical event
//!   recorded BEFORE that field existed must still fold. The inside-out fold test always supplies
//!   `lang`, so it never exercises the default arm; this test drives the raw on-log JSON, the form
//!   a rebuild actually replays, deliberately bypassing the in-crate payload structs to pin the
//!   JSON contract rather than the Rust type.
//! - the fold's replay-idempotency for the code arms. The projection is rebuilt by replaying the
//!   whole log (spec 29a's later rebuild criterion), and the code arms' `add_edge` does NOT dedup
//!   at the row level, so replay-safety rests entirely on the applied-position ledger; this pins
//!   that the code entity node and its structural edges honor it.
//! - the emit API's determinism-and-ordering contract the doc comments promise: `index_events`
//!   yields byte-identical events for identical source, and definitions precede references.
//!   Exercised through the real extraction pass, so it lives in the `symbols` lane only.

use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{
    Projection, KIND_ARTIFACT, KIND_CODE_ENTITY, KIND_FILE, REL_CONTAINS, REL_REFERENCES,
    TYPE_CODE_ENTITY_EXTRACTED, TYPE_EDGE_INFERRED, TYPE_FILE_TOUCHED,
};
use rigger::eventstore::Event;

/// Fold an event built from its raw on-log JSON bytes at `pos` - the SERIALIZED form a rebuild
/// replays - deliberately bypassing the in-crate payload structs so a test pins the JSON contract,
/// not the Rust type. `apply` returns `Err` on a deserialize failure, so a successful call is
/// itself evidence the payload satisfied the fold's contract.
fn apply_json(p: &Projector, pos: u64, type_: &str, json: serde_json::Value) {
    let mut e = Event::new(type_, serde_json::to_vec(&json).unwrap());
    e.position = pos;
    p.apply(&e).unwrap();
}

#[test]
fn a_code_event_missing_the_optional_lang_field_still_folds_backcompat() {
    // Back-compat contract: a CodeEntityExtracted / EdgeInferred as it would have been serialized
    // BEFORE the `lang` field existed - no `lang` key at all - must still fold, because `lang` is
    // `#[serde(default)]`. The inside-out fold test always supplies `lang`, so this default arm is
    // untested by it; a rebuild that replays a pre-`lang` log must not error.
    let p = Projector::open(":memory:", "test").unwrap();
    apply_json(
        &p,
        1,
        TYPE_CODE_ENTITY_EXTRACTED,
        serde_json::json!({
            "file": "src/combat.rs", "name": "apply_damage", "kind": "function", "line": 7,
        }),
    );
    apply_json(
        &p,
        2,
        TYPE_EDGE_INFERRED,
        serde_json::json!({ "file": "src/combat.rs", "name": "clamp" }),
    );

    let g = p.subgraph(&["src/combat.rs".to_string()], 2).unwrap();

    // The fold ran to completion (the `apply` calls above would have returned Err on a deserialize
    // failure): the file container node landed, and its absent `lang` defaulted to an empty string
    // rather than aborting the fold.
    let file = g
        .nodes
        .iter()
        .find(|n| n.id == "src/combat.rs")
        .expect("file container node folded from the lang-less events");
    assert_eq!(file.kind, KIND_FILE);
    assert_eq!(
        file.attrs.get("lang").map(String::as_str),
        Some(""),
        "a missing lang defaults to empty, never a fold error; got {:?}",
        file.attrs
    );

    let ent = g
        .nodes
        .iter()
        .find(|n| n.id == "src/combat.rs::apply_damage")
        .expect("code-entity node folded from the lang-less CodeEntityExtracted");
    assert_eq!(ent.kind, KIND_CODE_ENTITY);
    assert_eq!(
        ent.attrs.get("name").map(String::as_str),
        Some("apply_damage")
    );
    assert_eq!(ent.attrs.get("kind").map(String::as_str), Some("function"));
    assert_eq!(ent.attrs.get("line").map(String::as_str), Some("7"));

    assert!(
        g.edges.iter().any(|e| e.rel == REL_CONTAINS
            && e.from == "src/combat.rs"
            && e.to == "src/combat.rs::apply_damage"),
        "a CONTAINS edge folded from the lang-less definition event; got {:?}",
        g.edges
    );
    assert!(
        g.edges.iter().any(|e| e.rel == REL_REFERENCES
            && e.from == "src/combat.rs"
            && e.to == "src/combat.rs::clamp"),
        "a REFERENCES edge folded from the lang-less reference event; got {:?}",
        g.edges
    );
}

#[test]
fn replaying_the_code_events_does_not_double_the_entity_node_or_its_structural_edges() {
    // Replay-idempotency for the code arms: a from-log rebuild replays EVERY recorded event, and
    // the code arms' `add_edge` does not dedup at the row level - replay-safety rests entirely on
    // the applied-position ledger. Fold two code events, then replay the identical events at their
    // same positions and confirm nothing doubled.
    let p = Projector::open(":memory:", "test").unwrap();
    let def = serde_json::json!({
        "file": "src/combat.rs", "name": "apply_damage", "kind": "function", "line": 7,
        "lang": "rust",
    });
    let reference = serde_json::json!({ "file": "src/combat.rs", "name": "clamp", "lang": "rust" });

    apply_json(&p, 1, TYPE_CODE_ENTITY_EXTRACTED, def.clone());
    apply_json(&p, 2, TYPE_EDGE_INFERRED, reference.clone());
    // Replay the same events (same positions) - a rebuild pumps the whole log.
    apply_json(&p, 1, TYPE_CODE_ENTITY_EXTRACTED, def);
    apply_json(&p, 2, TYPE_EDGE_INFERRED, reference);

    let g = p.subgraph(&["src/combat.rs".to_string()], 2).unwrap();

    assert_eq!(
        g.nodes
            .iter()
            .filter(|n| n.id == "src/combat.rs::apply_damage")
            .count(),
        1,
        "replay must not double the code-entity node; got {:?}",
        g.nodes
    );
    assert_eq!(
        g.edges
            .iter()
            .filter(|e| e.rel == REL_CONTAINS && e.to == "src/combat.rs::apply_damage")
            .count(),
        1,
        "replay must not double the CONTAINS edge; got {:?}",
        g.edges
    );
    assert_eq!(
        g.edges
            .iter()
            .filter(|e| e.rel == REL_REFERENCES && e.to == "src/combat.rs::clamp")
            .count(),
        1,
        "replay must not double the REFERENCES edge; got {:?}",
        g.edges
    );
}

#[test]
fn the_file_container_holds_kind_file_in_the_integrated_graph_either_fold_order() {
    // The gating scenario the isolation-only fold tests never exercise (they fold code events into
    // an empty graph with no FileTouched): in a REAL run a source file is ALSO touched, governed,
    // and cited, so the SAME rel-path node is folded as KIND_ARTIFACT by TYPE_FILE_TOUCHED and as
    // KIND_FILE by the code events. Spec 29a's "one graph" makes that ONE node, so its kind must
    // land on KIND_FILE - the code half's deliverable - no matter which event folds first. This
    // pins the integrated identity in BOTH interleavings, so the file container node's kind is a
    // pure function of the source, not of log interleaving.
    let touch = |path: &str| serde_json::json!({ "path": path, "by": "impl" });
    let def = |path: &str| {
        serde_json::json!({
            "file": path, "name": "apply_damage", "kind": "function", "line": 7, "lang": "rust",
        })
    };
    let reference =
        |path: &str| serde_json::json!({ "file": path, "name": "clamp", "lang": "rust" });

    // Artifact-first: the file is TOUCHED (folds a KIND_ARTIFACT node) BEFORE its code is
    // extracted - the steady state of every real run, where FileTouched precedes extraction. The
    // code events must PROMOTE that shared node to KIND_FILE, not leave it KIND_ARTIFACT.
    let a = Projector::open(":memory:", "test").unwrap();
    apply_json(&a, 1, TYPE_FILE_TOUCHED, touch("src/combat.rs"));
    apply_json(&a, 2, TYPE_CODE_ENTITY_EXTRACTED, def("src/combat.rs"));
    apply_json(&a, 3, TYPE_EDGE_INFERRED, reference("src/combat.rs"));
    let ga = a.subgraph(&["src/combat.rs".to_string()], 3).unwrap();
    let fa = ga
        .nodes
        .iter()
        .find(|n| n.id == "src/combat.rs")
        .expect("the touched-then-extracted file is one node in the integrated graph");
    assert_eq!(
        fa.kind, KIND_FILE,
        "a file touched before its code is extracted must be PROMOTED to the file container \
         kind, not left as the generic artifact kind; got {:?}",
        fa
    );
    // The one node carries BOTH roles: its code entity hangs off it via a CONTAINS edge.
    assert!(
        ga.edges.iter().any(|e| e.rel == REL_CONTAINS
            && e.from == "src/combat.rs"
            && e.to == "src/combat.rs::apply_damage"),
        "the promoted node still holds its extracted entity; got {:?}",
        ga.edges
    );

    // Code-first: the code is extracted (folds a KIND_FILE node) BEFORE the file is touched. The
    // later FileTouched must NOT DEMOTE the established file container back to KIND_ARTIFACT.
    let b = Projector::open(":memory:", "test").unwrap();
    apply_json(&b, 1, TYPE_CODE_ENTITY_EXTRACTED, def("src/render.rs"));
    apply_json(&b, 2, TYPE_EDGE_INFERRED, reference("src/render.rs"));
    apply_json(&b, 3, TYPE_FILE_TOUCHED, touch("src/render.rs"));
    let gb = b.subgraph(&["src/render.rs".to_string()], 3).unwrap();
    let fb = gb
        .nodes
        .iter()
        .find(|n| n.id == "src/render.rs")
        .expect("the extracted-then-touched file is one node in the integrated graph");
    assert_eq!(
        fb.kind, KIND_FILE,
        "a later touch of an already-extracted file must not demote the file container kind; \
         got {:?}",
        fb
    );

    // A path that is ONLY touched, never code-extracted, stays the generic artifact kind - the
    // promotion is targeted to files code was actually extracted from, not a blanket relabel.
    let c = Projector::open(":memory:", "test").unwrap();
    apply_json(&c, 1, TYPE_FILE_TOUCHED, touch("docs/notes.md"));
    let gc = c.subgraph(&["docs/notes.md".to_string()], 1).unwrap();
    let fc = gc
        .nodes
        .iter()
        .find(|n| n.id == "docs/notes.md")
        .expect("the touched-only file node exists");
    assert_eq!(
        fc.kind, KIND_ARTIFACT,
        "a path with no code extracted from it stays the generic artifact kind; got {:?}",
        fc
    );
}

#[cfg(feature = "symbols")]
#[test]
fn real_extraction_tiers_every_structural_edge_through_the_emit_fold_pipeline() {
    use rigger::contextgraph::{TIER_AMBIGUOUS, TIER_EXTRACTED, TIER_INFERRED};
    use rigger::grounder::symbols::build_index;
    use rigger::grounder::symbols::events::index_events;

    // Spec 29a criterion 2, through the REAL extraction pass (not hand-built events): a source tree
    // is extracted, emitted, and folded, and every structural edge lands at its confidence tier.
    // `util.rs` defines `shared`; `combat.rs` defines `apply_damage` and, in a call site, references
    // `apply_damage` (same-file -> EXTRACTED), `shared` (defined in another file -> INFERRED), and
    // `undefined_thing` (defined nowhere -> AMBIGUOUS). Because `build_index` emits files in sorted
    // path order (combat.rs before util.rs), combat.rs's reference to `shared` folds BEFORE util.rs's
    // definition of it, so this ALSO exercises the definition arm's convergent AMBIGUOUS -> INFERRED
    // upgrade over real, sorted-order extraction - the reverse fold order a hand-built test can only
    // simulate.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("util.rs"), "fn shared() {}\n").unwrap();
    std::fs::write(
        dir.path().join("combat.rs"),
        "fn apply_damage() {}\nfn caller() { apply_damage(); shared(); undefined_thing(); }\n",
    )
    .unwrap();

    let idx = build_index(dir.path().to_str().unwrap(), None);
    let p = Projector::open(":memory:", "test").unwrap();
    for (i, mut e) in index_events(&idx).into_iter().enumerate() {
        e.position = (i + 1) as u64;
        p.apply(&e).unwrap();
    }

    let g = p.subgraph(&["combat.rs".to_string()], 3).unwrap();
    let tier_of = |to: &str| {
        g.edges
            .iter()
            .find(|e| e.rel == REL_REFERENCES && e.to == to)
            .unwrap_or_else(|| panic!("a REFERENCES edge to {to} folded; got {:?}", g.edges))
            .tier
            .clone()
    };

    assert_eq!(
        tier_of("combat.rs::apply_damage"),
        TIER_EXTRACTED,
        "a real same-file reference folds EXTRACTED"
    );
    assert_eq!(
        tier_of("combat.rs::shared"),
        TIER_INFERRED,
        "a real cross-file reference folds INFERRED (through the convergent upgrade, since the \
         definition folds after the reference in sorted file order)"
    );
    assert_eq!(
        tier_of("combat.rs::undefined_thing"),
        TIER_AMBIGUOUS,
        "a real reference to a name defined nowhere folds AMBIGUOUS"
    );
    // The definition's containment is EXTRACTED, and tiering drops no reference (safe superset).
    assert!(
        g.edges.iter().any(|e| e.rel == REL_CONTAINS
            && e.to == "combat.rs::apply_damage"
            && e.tier == TIER_EXTRACTED),
        "the CONTAINS edge folds EXTRACTED; got {:?}",
        g.edges
    );
}

#[cfg(feature = "symbols")]
#[test]
fn real_extraction_folds_caller_attributed_calls_edges_at_every_tier() {
    use rigger::contextgraph::{REL_CALLS, TIER_AMBIGUOUS, TIER_EXTRACTED, TIER_INFERRED};
    use rigger::grounder::symbols::build_index;
    use rigger::grounder::symbols::events::index_events;

    // Spec 37 criterion 3, through the WHOLE real chain (extractor attribution -> emit -> fold), not
    // hand-built events: a call inside `fn caller` folds a `combat.rs::caller --CALLS--> <callee>`
    // edge keyed by the REAL enclosing function the extractor attributed - the boundary the in-crate
    // fold test (which hand-sets `caller`) is structurally blind to. The three callees exercise every
    // tier the CALLS edge inherits from its REFERENCES twin: `apply_damage` (same file -> EXTRACTED);
    // `shared` (defined in another file, and because `build_index` emits files in sorted path order
    // combat.rs's call folds BEFORE util.rs's definition -> AMBIGUOUS, then promoted INFERRED by the
    // definition arm's convergent upgrade over `rel IN (REFERENCES, CALLS)`); `undefined_thing`
    // (defined nowhere -> AMBIGUOUS). A regression that dropped the emit caller, folded the wrong
    // caller, or forgot to promote CALLS with its twin reds here while every hand-built unit stays
    // green.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("util.rs"), "fn shared() {}\n").unwrap();
    std::fs::write(
        dir.path().join("combat.rs"),
        "fn apply_damage() {}\nfn caller() { apply_damage(); shared(); undefined_thing(); }\n",
    )
    .unwrap();

    let idx = build_index(dir.path().to_str().unwrap(), None);
    let p = Projector::open(":memory:", "test").unwrap();
    for (i, mut e) in index_events(&idx).into_iter().enumerate() {
        e.position = (i + 1) as u64;
        p.apply(&e).unwrap();
    }

    let g = p.subgraph(&["combat.rs".to_string()], 3).unwrap();
    let calls_tier = |callee: &str| {
        g.edges
            .iter()
            .find(|e| e.rel == REL_CALLS && e.from == "combat.rs::caller" && e.to == callee)
            .unwrap_or_else(|| {
                panic!(
                    "a CALLS edge from combat.rs::caller to {callee} folded; got {:?}",
                    g.edges
                )
            })
            .tier
            .clone()
    };
    assert_eq!(
        calls_tier("combat.rs::apply_damage"),
        TIER_EXTRACTED,
        "a real same-file call folds EXTRACTED, mirroring its REFERENCES twin"
    );
    assert_eq!(
        calls_tier("combat.rs::shared"),
        TIER_INFERRED,
        "a real cross-file call upgrades AMBIGUOUS -> INFERRED with its REFERENCES twin (the \
         definition folds after the call in sorted file order)"
    );
    assert_eq!(
        calls_tier("combat.rs::undefined_thing"),
        TIER_AMBIGUOUS,
        "a real call to a name defined nowhere folds AMBIGUOUS"
    );
    // Every CALLS edge is keyed by the ENCLOSING function, never the bare file container: the
    // extractor attributed each call to `caller`, so no CALLS edge hangs off the `combat.rs` node.
    assert!(
        !g.edges
            .iter()
            .any(|e| e.rel == REL_CALLS && e.from == "combat.rs"),
        "no CALLS edge hangs off the bare file node; the caller is the enclosing fn; got {:?}",
        g.edges
    );
}

#[cfg(feature = "symbols")]
#[test]
fn re_extracting_a_file_that_drops_a_call_supersedes_its_calls_edge_end_to_end() {
    use rigger::contextgraph::REL_CALLS;
    use rigger::grounder::symbols::build_index;
    use rigger::grounder::symbols::events::index_events;

    // Spec 37 + spec 29a criterion 3, through the REAL pipeline: a CALLS edge hangs off
    // `<file>::<caller>` (the enclosing definition), NOT the bare file node, so the supersede must
    // match it by an exact `<file>::` id prefix. Re-extract a file whose caller drops a call and the
    // stale CALLS edge must leave the live subgraph - the hand-built unit test proves the prefix
    // match on a fabricated from_id; this proves it against the REAL `<file>::caller` id the extractor
    // mints, composed with the emit-side `fresh` batch stamping.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("combat.rs");
    std::fs::write(
        &path,
        "fn apply_damage() {}\nfn heal() {}\nfn caller() { apply_damage(); heal(); }\n",
    )
    .unwrap();
    let first = index_events(&build_index(dir.path().to_str().unwrap(), None));
    let p = Projector::open(":memory:", "test").unwrap();
    let mut pos = 0u64;
    for mut e in first {
        pos += 1;
        e.position = pos;
        p.apply(&e).unwrap();
    }

    // Precondition: caller calls heal - a live CALLS edge before the change.
    let g0 = p.subgraph(&["combat.rs".to_string()], 3).unwrap();
    assert!(
        g0.edges.iter().any(|e| e.rel == REL_CALLS
            && e.from == "combat.rs::caller"
            && e.to == "combat.rs::heal"),
        "precondition: combat.rs::caller --CALLS--> combat.rs::heal is live before re-extraction; \
         got {:?}",
        g0.edges
    );

    // The file CHANGES: the call to `heal` is removed. Re-extract, emit, fold the second batch onto
    // the same projection at fresh positions.
    std::fs::write(
        &path,
        "fn apply_damage() {}\nfn caller() { apply_damage(); }\n",
    )
    .unwrap();
    let second = index_events(&build_index(dir.path().to_str().unwrap(), None));
    for mut e in second {
        pos += 1;
        e.position = pos;
        p.apply(&e).unwrap();
    }

    let g1 = p.subgraph(&["combat.rs".to_string()], 3).unwrap();
    // The removed call's CALLS edge is superseded - gone from the live subgraph, not accreted.
    assert!(
        !g1.edges
            .iter()
            .any(|e| e.rel == REL_CALLS && e.to == "combat.rs::heal"),
        "the removed call's CALLS edge is superseded on re-extraction; got {:?}",
        g1.edges
    );
    // The surviving call is still folded fresh: caller --CALLS--> apply_damage stays live.
    assert!(
        g1.edges.iter().any(|e| e.rel == REL_CALLS
            && e.from == "combat.rs::caller"
            && e.to == "combat.rs::apply_damage"),
        "the surviving call's CALLS edge remains live after re-extraction; got {:?}",
        g1.edges
    );
}

#[test]
fn the_public_calls_edge_folds_only_for_a_caller_carrying_reference_event() {
    use rigger::contextgraph::{REL_CALLS, TIER_EXTRACTED};

    // Wire + public-API contract under BOTH feature lanes (no real extractor needed): the fold reads
    // the `caller` key off the raw EdgeInferred JSON - the exact wire the emit pass writes - and adds
    // a `<file>::<caller> --CALLS--> <callee>` edge identified by the PUBLIC `REL_CALLS` const an
    // external consumer imports. A pre-37 / top-level reference event carries NO `caller` key and
    // folds NO CALLS edge (purely additive, back-compatible). This pins the emit->fold serialization
    // KEY and the additive boundary at the library surface, driving the raw on-log JSON rather than
    // the in-crate payload struct.
    let p = Projector::open(":memory:", "test").unwrap();
    let file = "src/a.rs";

    apply_def_json(&p, 1, file, "worker"); // the enclosing caller definition
    apply_def_json(&p, 2, file, "helper"); // the callee the caller calls
    apply_def_json(&p, 3, file, "util"); // a second callee, referenced caller-lessly
                                         // A caller-carrying reference (a call inside `worker`): the raw wire the emit pass writes.
    apply_json(
        &p,
        4,
        TYPE_EDGE_INFERRED,
        serde_json::json!({ "file": file, "name": "helper", "caller": "worker" }),
    );
    // A caller-less reference (a top-level / pre-37 event): no `caller` key at all.
    apply_ref_json(&p, 5, file, "util");

    let g = p.subgraph(&[file.to_string()], 3).unwrap();
    // The caller-carrying event folds the public CALLS edge, keyed by the enclosing definition, at
    // the same EXTRACTED tier as its same-file REFERENCES twin.
    assert!(
        g.edges.iter().any(|e| e.rel == REL_CALLS
            && e.from == "src/a.rs::worker"
            && e.to == "src/a.rs::helper"
            && e.tier == TIER_EXTRACTED),
        "the caller-carrying event folds src/a.rs::worker --CALLS--> src/a.rs::helper at EXTRACTED; \
         got {:?}",
        g.edges
    );
    // The caller-less event folds NO CALLS edge (additive / back-compat): the ONLY CALLS edge is the
    // caller-carrying one, so a stray edge to `util` or off the bare file node would red here.
    let calls: Vec<_> = g.edges.iter().filter(|e| e.rel == REL_CALLS).collect();
    assert_eq!(
        calls.len(),
        1,
        "exactly one CALLS edge folds - the caller-carrying one; the caller-less reference adds \
         none; got {calls:?}"
    );
}

#[cfg(feature = "symbols")]
#[test]
fn the_emit_api_is_deterministic_and_emits_definitions_before_references() {
    use rigger::grounder::symbols::build_index;
    use rigger::grounder::symbols::events::index_events;

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
    let first = index_events(&build_index(dir.path().to_str().unwrap(), None));
    let second = index_events(&build_index(dir.path().to_str().unwrap(), None));
    let shape = |evs: &[Event]| {
        evs.iter()
            .map(|e| (e.type_.clone(), e.data.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        shape(&first),
        shape(&second),
        "index_events must be deterministic for identical source"
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

#[cfg(feature = "symbols")]
#[test]
fn re_extracting_a_changed_file_supersedes_its_removed_symbols_end_to_end() {
    use rigger::grounder::symbols::build_index;
    use rigger::grounder::symbols::events::index_events;

    // Criterion 3, end to end through the REAL pipeline: extract a file, emit its events, fold; then
    // CHANGE the file (delete a symbol), re-extract, emit, and fold the second batch onto the SAME
    // projection. Because the emit pass stamps the batch boundary (`fresh`) on the first event of
    // each file, the fold supersedes the file's prior structural edges before folding the new batch,
    // so the live `subgraph` at the new position shows the surviving symbol and NOT the removed one -
    // a re-extraction REPLACES rather than accretes. This exercises extraction -> emit -> fold with
    // no hand-built events, so it pins that the emit-side `fresh` stamping and the fold's supersede
    // actually compose in production shape (unlike the in-crate fold test's hand-built batches).
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("combat.rs");

    // Initial extraction: two definitions, `apply_damage` and `heal`, and a call to each.
    std::fs::write(
        &path,
        "fn apply_damage() {}\nfn heal() {}\nfn caller() { apply_damage(); heal(); }\n",
    )
    .unwrap();
    let first = index_events(&build_index(dir.path().to_str().unwrap(), None));

    let p = Projector::open(":memory:", "test").unwrap();
    let mut pos = 0u64;
    for mut e in first {
        pos += 1;
        e.position = pos;
        p.apply(&e).unwrap();
    }

    // Precondition: both definitions are live in the projection before the change.
    let g0 = p.subgraph(&["combat.rs".to_string()], 3).unwrap();
    assert!(
        g0.nodes
            .iter()
            .any(|n| n.kind == KIND_CODE_ENTITY && n.id == "combat.rs::heal"),
        "precondition: heal is a live entity before re-extraction; got {:?}",
        g0.nodes
    );

    // The file CHANGES: `heal` is deleted (and its call removed). Re-extract, emit, fold the second
    // batch onto the same projection at fresh positions.
    std::fs::write(
        &path,
        "fn apply_damage() {}\nfn caller() { apply_damage(); }\n",
    )
    .unwrap();
    let second = index_events(&build_index(dir.path().to_str().unwrap(), None));
    for mut e in second {
        pos += 1;
        e.position = pos;
        p.apply(&e).unwrap();
    }

    // The live view at the new position REPLACED the old: apply_damage survives, the deleted heal is
    // gone from the live subgraph (its CONTAINS edge was superseded, not deleted).
    let g1 = p.subgraph(&["combat.rs".to_string()], 3).unwrap();
    assert!(
        g1.nodes
            .iter()
            .any(|n| n.kind == KIND_CODE_ENTITY && n.id == "combat.rs::apply_damage"),
        "apply_damage survives the re-extraction; got {:?}",
        g1.nodes
    );
    assert!(
        g1.edges
            .iter()
            .any(|e| e.rel == REL_CONTAINS && e.to == "combat.rs::apply_damage"),
        "apply_damage is still CONTAINed after re-extraction; got {:?}",
        g1.edges
    );
    assert!(
        !g1.edges.iter().any(|e| e.to == "combat.rs::heal"),
        "the deleted heal has no live edge after re-extraction; got {:?}",
        g1.edges
    );
    assert!(
        !g1.nodes.iter().any(|n| n.id == "combat.rs::heal"),
        "the deleted heal is not reachable in the live subgraph; got {:?}",
        g1.nodes
    );
}

#[cfg(feature = "symbols")]
#[test]
fn extract_events_emits_one_event_per_definition_and_reference_threading_the_file_label() {
    use rigger::grounder::symbols::build_index;
    use rigger::grounder::symbols::events::extract_events;

    // Drive the per-file emit API DIRECTLY - `index_events` only reaches `extract_events`
    // transitively, so its own contract is otherwise unpinned. Criterion 1 says the pass emits
    // "one CodeEntityExtracted per definition" and "one EdgeInferred per reference"; that
    // CARDINALITY is the core of the extract-as-events pass, yet no other periphery test asserts
    // it (the determinism test checks only non-empty + defs-before-refs). A regression that
    // dropped, doubled, or mis-typed a def/ref event would fold a wrong graph - and, once the
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
    assert_eq!(
        defs,
        fs.defs.len(),
        "exactly one CodeEntityExtracted per extracted definition; got {defs} for {} defs",
        fs.defs.len()
    );
    assert_eq!(
        refs,
        fs.refs.len(),
        "exactly one EdgeInferred per extracted reference; got {refs} for {} refs",
        fs.refs.len()
    );
    assert_eq!(
        events.len(),
        fs.defs.len() + fs.refs.len(),
        "extract_events emits exactly the definition and reference events, nothing spurious; got \
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
    // `index_events`): every definition event precedes every reference event, so a same-file
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

/// Fold a code definition (`file` defines `name`) from its raw on-log JSON at `pos`.
fn apply_def_json(p: &Projector, pos: u64, file: &str, name: &str) {
    apply_json(
        p,
        pos,
        TYPE_CODE_ENTITY_EXTRACTED,
        serde_json::json!({ "file": file, "name": name, "kind": "function", "line": 1 }),
    );
}

/// Fold a code reference (`file` references `name`) from its raw on-log JSON at `pos`.
fn apply_ref_json(p: &Projector, pos: u64, file: &str, name: &str) {
    apply_json(
        p,
        pos,
        TYPE_EDGE_INFERRED,
        serde_json::json!({ "file": file, "name": name }),
    );
}

/// The stored tier of the single `rel` edge landing on `to` in `g` (panics unless exactly one).
fn one_edge_tier(g: &rigger::contextgraph::Graph, rel: &str, to: &str) -> String {
    let hits: Vec<_> = g
        .edges
        .iter()
        .filter(|e| e.rel == rel && e.to == to)
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "expected exactly one {rel} edge to {to}; got {hits:?}"
    );
    hits[0].tier.clone()
}

#[test]
fn the_confidence_tier_persists_across_a_reopen_of_an_on_disk_graph() {
    use rigger::contextgraph::{TIER_AMBIGUOUS, TIER_EXTRACTED, TIER_INFERRED};

    // Persistence + reopen boundary the inside-out tier tests are structurally blind to: every
    // in-crate tier test folds into a fresh `:memory:` connection (never re-opened, never
    // re-migrated), and the one on-disk unit test only proves the EXTRACTED *backfill* of a
    // pre-tier legacy edge. NONE proves that a NON-default tier - INFERRED or AMBIGUOUS - written
    // by the current projector to a real `graph.db` survives being closed and re-opened by a FRESH
    // projector (which re-runs the additive tier migration as an idempotent no-op) and reads back
    // through `row_to_edge`. This also pins the DURABLE serialized value: an edge's tier is stored
    // as its literal string, the cross-version on-disk contract a later traversal filters on, so we
    // assert the reopened tiers against the literal wire strings AND cross-check the public consts
    // still carry those exact literals - the public API name and the persisted value cannot drift.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("graph.db");
    let path = path.to_str().unwrap();

    {
        let p = Projector::open(path, "test").unwrap();
        // util.rs defines `shared`; combat.rs defines `apply_damage` and references three names:
        // `apply_damage` (same-file -> EXTRACTED), `shared` (another file -> INFERRED), and `magic`
        // (defined nowhere -> AMBIGUOUS). All three non-default tiers must survive the reopen.
        apply_def_json(&p, 1, "util.rs", "shared");
        apply_def_json(&p, 2, "combat.rs", "apply_damage");
        apply_ref_json(&p, 3, "combat.rs", "apply_damage");
        apply_ref_json(&p, 4, "combat.rs", "shared");
        apply_ref_json(&p, 5, "combat.rs", "magic");
    } // projector dropped: the connection is closed and every tier is now only on disk.

    // A FRESH projector re-opens the same file (re-running the tier migration idempotently).
    let reopened = Projector::open(path, "test").unwrap();
    let g = reopened.subgraph(&["combat.rs".to_string()], 3).unwrap();

    // The literal on-disk wire values survive unchanged - this is the durable cross-version
    // contract, asserted against the raw strings, not just the consts.
    assert_eq!(
        one_edge_tier(&g, REL_CONTAINS, "combat.rs::apply_damage"),
        "extracted",
        "a definition's CONTAINS edge persists at the extracted tier"
    );
    assert_eq!(
        one_edge_tier(&g, REL_REFERENCES, "combat.rs::apply_damage"),
        "extracted",
        "a same-file reference persists at the extracted tier"
    );
    assert_eq!(
        one_edge_tier(&g, REL_REFERENCES, "combat.rs::shared"),
        "inferred",
        "a cross-file reference persists at the inferred tier across the reopen"
    );
    assert_eq!(
        one_edge_tier(&g, REL_REFERENCES, "combat.rs::magic"),
        "ambiguous",
        "a define-nowhere reference persists at the ambiguous tier across the reopen"
    );

    // The public consts still carry those exact literals: the API name and the persisted value
    // cannot silently diverge (a rename of a const would redden this before it corrupts a db).
    assert_eq!(TIER_EXTRACTED, "extracted");
    assert_eq!(TIER_INFERRED, "inferred");
    assert_eq!(TIER_AMBIGUOUS, "ambiguous");

    // Safe-superset (addendum 2.4) holds after the reopen too: tiering drops NO reference - all
    // three folded references read back, each carrying exactly one of the three tiers.
    let ref_tiers: Vec<&str> = {
        let mut t: Vec<&str> = g
            .edges
            .iter()
            .filter(|e| e.rel == REL_REFERENCES)
            .map(|e| e.tier.as_str())
            .collect();
        t.sort_unstable();
        t
    };
    assert_eq!(
        ref_tiers,
        vec!["ambiguous", "extracted", "inferred"],
        "every folded reference survives the reopen, one per tier; got {ref_tiers:?}"
    );
}

#[test]
fn a_definition_upgrades_only_the_exact_name_cross_file_reference_never_a_substring() {
    use rigger::contextgraph::{TIER_AMBIGUOUS, TIER_INFERRED};

    // The convergent AMBIGUOUS -> INFERRED upgrade in the definition arm matches the reference's
    // target name by EXACT equality on the `::`-suffix, never as a wildcard or substring (the code
    // comment promises "never by a wildcard"). No in-crate test exercises that precision: defining
    // a short name that is a SUBSTRING of a still-unresolved reference's name must NOT drag that
    // reference up a tier. A regression to a `LIKE`/prefix match would silently over-promote.
    let p = Projector::open(":memory:", "test").unwrap();

    // combat.rs references `apply_damage`; no file defines it yet, so it folds AMBIGUOUS.
    apply_ref_json(&p, 1, "combat.rs", "apply_damage");

    // util.rs defines `damage` - a proper substring of `apply_damage`. The upgrade fires for the
    // name `damage`; an exact-match must leave the `apply_damage` reference untouched.
    apply_def_json(&p, 2, "util.rs", "damage");
    let g = p.subgraph(&["combat.rs".to_string()], 3).unwrap();
    assert_eq!(
        one_edge_tier(&g, REL_REFERENCES, "combat.rs::apply_damage"),
        TIER_AMBIGUOUS,
        "defining a substring name must NOT upgrade an unrelated reference (no wildcard match)"
    );

    // Positive control: defining the EXACT name promotes the same reference AMBIGUOUS -> INFERRED,
    // proving the negative above is precision, not a dead upgrade path.
    apply_def_json(&p, 3, "util.rs", "apply_damage");
    let g = p.subgraph(&["combat.rs".to_string()], 3).unwrap();
    assert_eq!(
        one_edge_tier(&g, REL_REFERENCES, "combat.rs::apply_damage"),
        TIER_INFERRED,
        "defining the exact name DOES promote the cross-file reference to inferred"
    );
}

/// Fold a code DEFINITION carrying the extraction-batch boundary marker `fresh` from its raw on-log
/// JSON at `pos` (spec 29a criterion 3). When `fresh` is true this is the FIRST event of a file's
/// (re-)extraction batch, so the fold supersedes the file's prior live structural edges before
/// folding this one - a re-extraction REPLACES rather than accretes. Driven as raw JSON (the wire
/// form a rebuild replays), so it exercises the ALWAYS-compiled fold in BOTH feature lanes.
fn apply_def_fresh(p: &Projector, pos: u64, file: &str, name: &str, line: u32, fresh: bool) {
    apply_json(
        p,
        pos,
        TYPE_CODE_ENTITY_EXTRACTED,
        serde_json::json!({
            "file": file, "name": name, "kind": "function", "line": line, "fresh": fresh,
        }),
    );
}

/// Fold a code REFERENCE carrying the extraction-batch boundary marker `fresh`, exactly as
/// [`apply_def_fresh`]. A refs-only file (one that defines nothing) carries the boundary HERE, on
/// its first reference, so the reference fold arm is the one that supersedes for such a file.
fn apply_ref_fresh(p: &Projector, pos: u64, file: &str, name: &str, fresh: bool) {
    apply_json(
        p,
        pos,
        TYPE_EDGE_INFERRED,
        serde_json::json!({ "file": file, "name": name, "fresh": fresh }),
    );
}

/// True when a LIVE structural edge of relation `rel` runs from `from` to `to` in `g`. The
/// `subgraph` port returns only live edges (`valid_to IS NULL`), so a superseded edge is absent -
/// which is exactly how a test observes supersede-on-re-extract through the public surface.
fn has_live_edge(g: &rigger::contextgraph::Graph, rel: &str, from: &str, to: &str) -> bool {
    g.edges
        .iter()
        .any(|e| e.rel == rel && e.from == from && e.to == to)
}

#[test]
fn a_pre_criterion_3_log_without_the_fresh_key_never_supersedes_backcompat() {
    // Serialized-form back-compat (criterion 3): the `fresh` batch-boundary marker is
    // `#[serde(default, skip_serializing_if = "is_false")]`, so an event recorded BEFORE the field
    // existed - carrying no `fresh` key at all - folds as `fresh = false`, a NON-boundary event. A
    // rebuild that replays such a pre-criterion-3 log must therefore fold EXACTLY as it did before:
    // re-recording a file must NOT retroactively supersede anything (supersede is opt-in, gated on
    // `fresh`). The inside-out fold unit test always supplies `fresh` explicitly, so it never
    // exercises this default-false arm; this drives the raw wire form a historical log replays.
    let p = Projector::open(":memory:", "test").unwrap();
    let file = "src/legacy.rs";

    // A pre-criterion-3 log: the same file appears in two extraction passes, and NO event carries a
    // `fresh` key (the field did not exist when the log was written). `apply_def_json` omits it.
    apply_def_json(&p, 1, file, "foo");
    apply_def_json(&p, 2, file, "bar");

    // Precondition: both definitions are live before the second pass.
    let g0 = p.subgraph(&[file.to_string()], 2).unwrap();
    assert!(
        has_live_edge(&g0, REL_CONTAINS, file, "src/legacy.rs::bar"),
        "precondition: bar is CONTAINed after the first pass; got {:?}",
        g0.edges
    );

    // The file is re-recorded (a second pre-criterion-3 pass): `foo` again, but NOT `bar`. With no
    // `fresh` marker on any event, the fold supersedes nothing - so the earlier `bar` edge stays
    // live and the graph accretes, byte-for-byte the pre-criterion-3 behavior.
    apply_def_json(&p, 3, file, "foo");

    let g1 = p.subgraph(&[file.to_string()], 2).unwrap();
    assert!(
        has_live_edge(&g1, REL_CONTAINS, file, "src/legacy.rs::bar"),
        "a fresh-less (pre-criterion-3) log NEVER supersedes: bar stays live even though the second \
         pass omitted it; got {:?}",
        g1.edges
    );
    assert!(
        has_live_edge(&g1, REL_CONTAINS, file, "src/legacy.rs::foo"),
        "foo is CONTAINed after the second pass; got {:?}",
        g1.edges
    );
}

#[test]
fn a_refs_only_files_re_extraction_supersedes_via_the_reference_batch_boundary() {
    // Criterion 3, the REFERENCE fold arm's supersede branch: a file that defines nothing but
    // references symbols still re-extracts, so it still needs the batch boundary - and with no
    // definitions the FIRST emitted event is a reference, so `fresh` rides that reference and the
    // `EdgeInferred` fold arm (not the definition arm) does the superseding. Every other supersede
    // test - the in-crate fold unit test and the symbols-lane end-to-end test - re-extracts a file
    // that DEFINES something, so its boundary lands on the definition arm; this un-gated raw-JSON
    // test is the only one that drives the reference arm's `if r.fresh` supersede, in BOTH lanes.
    let p = Projector::open(":memory:", "test").unwrap();
    let file = "src/only_refs.rs";

    // Initial extraction of a refs-only file: it references `alpha` (the first event: the boundary)
    // then `beta`.
    apply_ref_fresh(&p, 1, file, "alpha", true);
    apply_ref_fresh(&p, 2, file, "beta", false);

    // Precondition: both references are live before the file changes.
    let g0 = p.subgraph(&[file.to_string()], 2).unwrap();
    assert!(
        has_live_edge(&g0, REL_REFERENCES, file, "src/only_refs.rs::beta"),
        "precondition: beta is REFERENCEd before re-extraction; got {:?}",
        g0.edges
    );

    // The file CHANGES: it now references `alpha` only (the `beta` reference is gone). Re-extract:
    // the first reference carries `fresh`, so the reference fold arm supersedes the file's prior
    // structural edges before folding this batch.
    apply_ref_fresh(&p, 3, file, "alpha", true);

    let g1 = p.subgraph(&[file.to_string()], 2).unwrap();
    assert!(
        has_live_edge(&g1, REL_REFERENCES, file, "src/only_refs.rs::alpha"),
        "alpha is still REFERENCEd after re-extraction; got {:?}",
        g1.edges
    );
    assert!(
        !has_live_edge(&g1, REL_REFERENCES, file, "src/only_refs.rs::beta"),
        "the removed beta reference is superseded (gone from the live view) via the reference \
         batch boundary; got {:?}",
        g1.edges
    );
}

#[test]
fn a_re_extraction_supersedes_only_its_own_files_edges_not_another_files_reference() {
    // Criterion 3 scope boundary (cross-module seam): `supersede_file_edges` is scoped to
    // `from_id = the re-extracted file`, so re-extracting file A retires only A's OWN structural
    // edges - a REFERENCES edge OUT of a DIFFERENT file B (whose `from_id` is B) is left live even
    // when it targets a name A once defined. This is the deliberate scope decision, not a defect:
    // cross-file re-resolution is out of criterion 3. Two file containers make this a seam the
    // single-file unit tests are structurally blind to; if supersede were global (its `from_id`
    // clause dropped) B's reference would be wrongly retired - this pins that it is not.
    let p = Projector::open(":memory:", "test").unwrap();
    let a = "src/a.rs";
    let b = "src/b.rs";

    // File A defines `shared`; file B references `shared` (a separate, B-scoped REFERENCES edge).
    apply_def_fresh(&p, 1, a, "shared", 3, true);
    apply_ref_fresh(&p, 2, b, "shared", true);

    // Precondition: A CONTAINS its definition and B has its own live reference edge.
    let g0 = p.subgraph(&[a.to_string(), b.to_string()], 3).unwrap();
    assert!(
        has_live_edge(&g0, REL_CONTAINS, a, "src/a.rs::shared"),
        "precondition: A CONTAINS shared before re-extraction; got {:?}",
        g0.edges
    );
    assert!(
        has_live_edge(&g0, REL_REFERENCES, b, "src/b.rs::shared"),
        "precondition: B REFERENCEs its own shared target before A re-extracts; got {:?}",
        g0.edges
    );

    // File A CHANGES: it now defines `renamed` instead of `shared`. Re-extract A only - its batch
    // boundary supersedes A's own prior structural edges (CONTAINS shared) and nothing else.
    apply_def_fresh(&p, 3, a, "renamed", 5, true);

    let g1 = p.subgraph(&[a.to_string(), b.to_string()], 3).unwrap();
    // A's own re-extraction took effect: the renamed definition is live and the removed `shared`
    // definition is superseded out of the live view.
    assert!(
        has_live_edge(&g1, REL_CONTAINS, a, "src/a.rs::renamed"),
        "A's renamed definition is live after re-extraction; got {:?}",
        g1.edges
    );
    assert!(
        !has_live_edge(&g1, REL_CONTAINS, a, "src/a.rs::shared"),
        "A's removed `shared` definition is superseded out of the live view; got {:?}",
        g1.edges
    );
    // The scope boundary: B's reference edge (from_id = B) was NOT touched by supersede(A), so it
    // stays live. A supersede that ignored `from_id` would have wrongly retired it.
    assert!(
        has_live_edge(&g1, REL_REFERENCES, b, "src/b.rs::shared"),
        "B's cross-file reference survives A's re-extraction (supersede is scoped to A's own \
         from-edges); got {:?}",
        g1.edges
    );
}

#[cfg(feature = "symbols")]
#[test]
fn project_batches_lowers_a_whole_tree_into_per_file_code_batches_the_fold_ingests() {
    // Spec 29c criterion 5's PUBLIC production entry for the CODE half: `project_batches` walks a
    // whole tree and returns each file's extraction batch (the ACTUAL definitions/references) in
    // sorted path order, which the always-compiled fold ingests into code-entity nodes. This is the
    // pass a live run drives to POPULATE the graph (29a built the machinery, 29c wires the caller);
    // exercised here at the crate boundary over a real extraction of real source, not hand-built rows.
    use rigger::grounder::symbols::events::project_batches;

    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("src/a.rs"), "pub fn alpha() {}\n").unwrap();
    std::fs::write(dir.path().join("src/b.rs"), "pub fn beta() { alpha(); }\n").unwrap();

    let batches = project_batches(dir.path().to_str().unwrap());
    // One batch per file that extracts to something, keyed by its file, in sorted path order.
    let files: Vec<&str> = batches.iter().map(|(f, _)| f.as_str()).collect();
    assert_eq!(
        files,
        vec!["src/a.rs", "src/b.rs"],
        "one sorted per-file batch for each source file the tree walk visits"
    );
    // Each file's batch is non-empty and its FIRST event carries the `fresh` re-extraction boundary
    // (29a's supersede head), exactly as a live run's ingest emits - so a re-ingest supersedes that
    // file's prior edges rather than accreting duplicates.
    for (file, events) in &batches {
        assert!(!events.is_empty(), "{file}'s batch carries events");
        let head: serde_json::Value = serde_json::from_slice(&events[0].data).unwrap();
        assert_eq!(
            head.get("fresh").and_then(serde_json::Value::as_bool),
            Some(true),
            "{file}'s batch head carries the fresh supersede boundary"
        );
    }
    // Folding every batch yields the tree's REAL code-entity nodes - the graph a run would populate.
    let p = Projector::open(":memory:", "test").unwrap();
    let mut pos = 0u64;
    for (_, events) in &batches {
        for e in events {
            pos += 1;
            let mut ev = e.clone();
            ev.position = pos;
            p.apply(&ev).unwrap();
        }
    }
    let g = p
        .subgraph(&["src/a.rs".to_string(), "src/b.rs".to_string()], 3)
        .unwrap();
    let has_entity = |name: &str| {
        g.nodes.iter().any(|n| {
            n.kind == KIND_CODE_ENTITY && n.attrs.get("name").map(String::as_str) == Some(name)
        })
    };
    assert!(
        has_entity("alpha") && has_entity("beta"),
        "the tree's real definitions fold as code-entity nodes the traversal reaches; got {:?}",
        g.nodes
    );
}
