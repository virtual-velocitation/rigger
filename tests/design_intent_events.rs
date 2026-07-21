//! Periphery (contract / API / integration) tests for spec 29b criteria 1-2: design intent
//! ingested AS EVENTS - the design-intent NODES (criterion 1) and the design-intent EDGES
//! (criterion 2). These run OUTSIDE the crate, over the library's public surface, so they guard the
//! boundary the inside-out fold / emit unit tests are structurally blind to:
//!
//! - the SERIALIZED-FORM back-compat contract of the new event type. A `DocConceptExtracted`
//!   payload carries `title` and `doc` behind `#[serde(default)]`, so a historical event recorded
//!   before those fields existed (or a minimal `{kind,id}` emit) must still fold. The inside-out
//!   fold test always supplies both, so it never exercises the default arms; this test drives the
//!   raw on-log JSON, the form a rebuild actually replays, deliberately bypassing the in-crate
//!   payload struct to pin the JSON contract rather than the Rust type.
//! - the fold arm's defensive kind guard. The arm matches the four design-intent kinds exactly and
//!   folds NOTHING for any other kind string (`_ => Ok(())`), so a stray / future kind never mints a
//!   spurious node and never wedges a rebuild. The inside-out fold test only ever feeds valid kinds.
//! - the one-graph promotion, for ALL FOUR design-intent kinds. A path first seen as a bare
//!   `artifact` (a decision GOVERNS it, a lesson is ABOUT it) must PROMOTE to the specific
//!   design-intent kind when the same path is ingested, or the design-intent query would miss it;
//!   and a later bare-artifact reference must never DEMOTE it. The inside-out promotion test proves
//!   this for `design-doc` only, so the promotion LIST being closed over `arch-decision`,
//!   `handbook-rule`, and `rationale` too is untested by it.
//! - the public emit-to-fold mapping. `ConceptKind::node_kind` is the single mapping the emit lowers
//!   through, and the fold matches those exact strings; this drives the real public API from outside
//!   the crate to pin that every concept kind lowers onto the fold arm that matches it (no silent
//!   drop) and that the full extract-emit-fold pipeline is a deterministic, reproducible rebuild.

use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{
    Graph, Projection, KIND_ARCH_DECISION, KIND_ARTIFACT, KIND_DESIGN_DOC, KIND_HANDBOOK_RULE,
    KIND_RATIONALE, REL_CONSTRAINS, REL_DOC_REFERENCES, REL_EXPLAINS, REL_GOVERNS, REL_SPECIFIES,
    TIER_EXTRACTED, TYPE_DECISION_MADE, TYPE_DOC_CONCEPT_EXTRACTED, TYPE_DOC_LINK_EXTRACTED,
};
use rigger::eventstore::Event;

/// Fold an event built from its raw on-log JSON bytes at `pos` - the SERIALIZED form a rebuild
/// replays - deliberately bypassing the in-crate payload structs so a test pins the JSON contract,
/// not the Rust type. `apply` returns `Err` on a deserialize failure, so a successful call is itself
/// evidence the payload satisfied the fold's contract.
fn apply_json(p: &Projector, pos: u64, type_: &str, json: serde_json::Value) {
    let mut e = Event::new(type_, serde_json::to_vec(&json).unwrap());
    e.position = pos;
    p.apply(&e).unwrap();
}

/// The kind of the node with `id` in `g`, if it folded at all.
fn kind_of<'g>(g: &'g Graph, id: &str) -> Option<&'g str> {
    g.nodes.iter().find(|n| n.id == id).map(|n| n.kind.as_str())
}

/// The four design-intent node kinds this criterion introduces - the closed set the fold arm
/// matches and the promotion list must cover.
const DESIGN_INTENT_KINDS: [&str; 4] = [
    KIND_DESIGN_DOC,
    KIND_ARCH_DECISION,
    KIND_HANDBOOK_RULE,
    KIND_RATIONALE,
];

#[test]
fn a_doc_concept_missing_the_optional_title_and_doc_still_folds_backcompat() {
    // Back-compat contract: a DocConceptExtracted as it would have been serialized before the
    // `title` / `doc` fields existed - only `kind` and `id` present, no `title` / `doc` key at all -
    // must still fold, because both are `#[serde(default)]`. The inside-out fold test always supplies
    // title and doc, so this default arm is untested by it; a rebuild that replays a pre-`title` log
    // must not error, and the missing attrs must default to empty rather than aborting the fold.
    for (i, kind) in DESIGN_INTENT_KINDS.iter().enumerate() {
        let p = Projector::open(":memory:", "test").unwrap();
        let id = format!("docs/legacy-{kind}.md");
        apply_json(
            &p,
            (i + 1) as u64,
            TYPE_DOC_CONCEPT_EXTRACTED,
            // Deliberately minimal: the on-log form a pre-`title`/`doc` emit would have written.
            serde_json::json!({ "kind": kind, "id": id }),
        );

        let g = p.subgraph(std::slice::from_ref(&id), 1).unwrap();
        // The fold ran to completion (the `apply` above would have returned Err on a deserialize
        // failure): the design-intent node landed at its declared kind.
        assert_eq!(
            kind_of(&g, &id),
            Some(*kind),
            "a title/doc-less DocConceptExtracted still folds into a {kind} node; got {:?}",
            g.nodes
        );
        let node = g.nodes.iter().find(|n| n.id == id).unwrap();
        assert_eq!(
            node.attrs.get("title").map(String::as_str),
            Some(""),
            "a missing title defaults to empty, never a fold error; got {:?}",
            node.attrs
        );
        assert_eq!(
            node.attrs.get("doc").map(String::as_str),
            Some(""),
            "a missing doc defaults to empty, never a fold error; got {:?}",
            node.attrs
        );
    }
}

#[test]
fn a_doc_concept_with_an_unrecognized_kind_folds_nothing_and_never_errors() {
    // Defensive kind guard: the fold arm matches the four design-intent kinds exactly and returns
    // Ok without touching the graph for any other kind string (`_ => Ok(())`). So a stray or
    // future-vocabulary kind on the log never mints a spurious node and never wedges a rebuild. The
    // inside-out fold test only ever feeds the four valid kinds, so this arm is untested by it.
    let p = Projector::open(":memory:", "test").unwrap();
    let id = "docs/some-doc.md";
    // A well-formed event whose `kind` is simply not one of the four - the `apply` must still
    // succeed (no Err), proving the arm returns Ok rather than failing the fold.
    apply_json(
        &p,
        1,
        TYPE_DOC_CONCEPT_EXTRACTED,
        serde_json::json!({ "kind": "not-a-design-intent-kind", "id": id, "title": "x", "doc": id }),
    );

    let g = p.subgraph(&[id.to_string()], 1).unwrap();
    assert!(
        kind_of(&g, id).is_none(),
        "an unrecognized kind folds NO node; got {:?}",
        g.nodes
    );
}

#[test]
fn a_governed_artifact_path_promotes_to_each_design_intent_kind_and_never_demotes() {
    // One-graph identity (spec 29b, addendum 6.1 single id space), for ALL FOUR design-intent kinds.
    // A path is folded as a bare `artifact` the moment a decision GOVERNS it - which happens in a
    // real run, where the decision stream cites docs by path. When that SAME path is later ingested
    // as design intent it must PROMOTE to the specific kind (or the design-intent query misses it),
    // and a later bare-artifact reference must never DEMOTE the established kind. The inside-out
    // promotion test proves this for `design-doc` alone; this pins that the promotion list is closed
    // over `arch-decision`, `handbook-rule`, and `rationale` too - dropping any one from the list
    // would leave that ingested kind stuck as a bare artifact, invisible to the design-doc query.
    for kind in DESIGN_INTENT_KINDS {
        let path = format!("docs/one-graph-{kind}.md");

        // Order A: governed-first (bare artifact), then ingested (promotes to the specific kind).
        let a = Projector::open(":memory:", "test").unwrap();
        apply_json(
            &a,
            1,
            TYPE_DECISION_MADE,
            serde_json::json!({
                "id": format!("d-{kind}"), "summary": "cites the doc by path",
                "governs": [path.clone()], "supersedes": "",
            }),
        );
        // The starting state is a BARE artifact - the generic role a governed path carries.
        let before = a.subgraph(std::slice::from_ref(&path), 1).unwrap();
        assert_eq!(
            kind_of(&before, &path),
            Some(KIND_ARTIFACT),
            "a governed path starts as a bare artifact; got {:?}",
            before.nodes
        );
        apply_json(
            &a,
            2,
            TYPE_DOC_CONCEPT_EXTRACTED,
            serde_json::json!({ "kind": kind, "id": path.clone(), "title": "design intent", "doc": path.clone() }),
        );
        let g = a.subgraph(std::slice::from_ref(&path), 1).unwrap();
        assert_eq!(
            kind_of(&g, &path),
            Some(kind),
            "a governed artifact PROMOTES to {kind} when ingested; got {:?}",
            g.nodes
        );
        let promoted = g.nodes.iter().find(|n| n.id == path).unwrap();
        assert_eq!(
            promoted.attrs.get("title").map(String::as_str),
            Some("design intent"),
            "the ingested title rides onto the promoted {kind} node; got {:?}",
            promoted.attrs
        );

        // Order B: ingested-first, then a later governing reference - stays the specific kind (a
        // bare-artifact reference never DEMOTES an established design-intent kind).
        let b = Projector::open(":memory:", "test").unwrap();
        apply_json(
            &b,
            1,
            TYPE_DOC_CONCEPT_EXTRACTED,
            serde_json::json!({ "kind": kind, "id": path.clone(), "title": "design intent", "doc": path.clone() }),
        );
        apply_json(
            &b,
            2,
            TYPE_DECISION_MADE,
            serde_json::json!({
                "id": format!("d-{kind}"), "summary": "cites the doc by path",
                "governs": [path.clone()], "supersedes": "",
            }),
        );
        let g = b.subgraph(std::slice::from_ref(&path), 1).unwrap();
        assert_eq!(
            kind_of(&g, &path),
            Some(kind),
            "a later governing reference never DEMOTES the {kind} node; got {:?}",
            g.nodes
        );
    }
}

#[cfg(feature = "symbols")]
#[test]
fn the_public_emit_lowers_every_concept_kind_onto_the_fold_arm_that_matches_it() {
    use rigger::grounder::design::events::concept_events;
    use rigger::grounder::design::model::{ConceptKind, DesignConcept};

    // Emit-to-fold mapping closure, driven through the REAL public API from outside the crate. The
    // emit lowers each concept through the single `ConceptKind::node_kind` mapping, and the fold arm
    // matches those exact strings; if the two ever drift, the emit would produce an event the fold
    // silently drops through its `_ => Ok(())` arm. So for EVERY `ConceptKind` variant, a concept
    // emitted through the public API must fold into a node whose kind is exactly `node_kind()`. This
    // also proves the emit / model / node-kind surface is genuinely reachable across the crate
    // boundary (a later criterion extends this same emit module), which an in-crate test cannot.
    let variants = [
        ConceptKind::DesignDoc,
        ConceptKind::ArchDecision,
        ConceptKind::HandbookRule,
        ConceptKind::Rationale,
    ];
    for variant in variants {
        let expected_kind = variant.node_kind();
        let concept = DesignConcept {
            kind: variant,
            id: format!("docs/{expected_kind}.md"),
            title: "a concept".to_string(),
            doc: format!("docs/{expected_kind}.md"),
        };
        let events = concept_events(std::slice::from_ref(&concept));
        assert_eq!(events.len(), 1, "one concept emits one event");
        assert_eq!(
            events[0].type_, TYPE_DOC_CONCEPT_EXTRACTED,
            "the emitted event is a DocConceptExtracted"
        );

        let p = Projector::open(":memory:", "test").unwrap();
        let mut e = events.into_iter().next().unwrap();
        e.position = 1;
        p.apply(&e).unwrap();

        let g = p.subgraph(std::slice::from_ref(&concept.id), 1).unwrap();
        assert_eq!(
            kind_of(&g, &concept.id),
            Some(expected_kind),
            "{variant:?} emits an event the fold turns into a {expected_kind} node (no silent \
             drop); got {:?}",
            g.nodes
        );
    }
}

#[cfg(feature = "symbols")]
#[test]
fn the_public_extraction_pipeline_is_a_deterministic_reproducible_rebuild() {
    use rigger::grounder::design::events::concept_events;
    use rigger::grounder::design::extract::extract_concepts;

    // Reproducible rebuild, end to end through the REAL public pipeline (extract -> emit -> fold)
    // from outside the crate. The design half of the graph is a rebuildable projection, so identical
    // sources must yield byte-identical events AND an identical folded graph however many times they
    // are replayed - a non-deterministic iteration order anywhere in extraction or emit (e.g. a
    // HashMap) would break the reproducible-rebuild guarantee spec 29b rests on. Feed one source of
    // each of the four design-intent shapes, run the whole pipeline twice, and prove both the emitted
    // event bytes and the resulting node kinds match.
    let sources: [(&str, &str); 4] = [
        (
            "docs/architecture.md",
            "# Reference architecture\n\nintro\n\n## Node taxonomy\n\n## Edge taxonomy\n",
        ),
        (
            "docs/adr/0001-code-as-events.md",
            "# Ingest code as events\n\n## Decision\n",
        ),
        (
            "docs/handbook-rules.md",
            "# Loop-discipline rule: one owner per criterion\n",
        ),
        (
            "src/combat.rs",
            "fn clamp() {}\n// WHY: damage must never go negative\n",
        ),
    ];
    let run = || {
        let mut concepts = Vec::new();
        for (path, contents) in sources {
            concepts.extend(extract_concepts(path, contents));
        }
        concept_events(&concepts)
    };

    let first = run();
    let second = run();
    let bytes = |es: &[Event]| {
        es.iter()
            .map(|e| (e.type_.clone(), e.data.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        bytes(&first),
        bytes(&second),
        "the extract-emit pipeline is byte-deterministic for identical sources"
    );
    assert!(
        !first.is_empty(),
        "the design-intent extraction emitted events for the four sources"
    );

    // Fold the emitted stream and confirm the four design-intent kinds all land - the reference
    // architecture becomes a set of queryable design-intent nodes in the very graph it specifies.
    let p = Projector::open(":memory:", "test").unwrap();
    let mut seeds = Vec::new();
    for (i, mut e) in first.into_iter().enumerate() {
        e.position = (i + 1) as u64;
        p.apply(&e).unwrap();
    }
    for path in ["docs/architecture.md", "docs/adr/0001-code-as-events.md"] {
        seeds.push(path.to_string());
    }
    // The whole-doc and rationale ids the extraction assigns, so the seed reaches every folded node.
    seeds.push("docs/handbook-rules.md".to_string());
    seeds.push("src/combat.rs#L2".to_string());
    let g = p.subgraph(&seeds, 1).unwrap();
    for kind in DESIGN_INTENT_KINDS {
        assert!(
            g.nodes.iter().any(|n| n.kind == kind),
            "the pipeline folded a {kind} node; got {:?}",
            g.nodes
        );
    }
}

/// Whether `g` holds a currently-live edge `from --rel--> to` at the EXTRACTED tier (every
/// design-intent link folds at that tier, addendum 6.2).
fn has_edge(g: &Graph, from: &str, rel: &str, to: &str) -> bool {
    g.edges
        .iter()
        .any(|e| e.from == from && e.rel == rel && e.to == to && e.tier == TIER_EXTRACTED)
}

#[test]
fn every_design_intent_link_relation_folds_into_its_typed_edge_at_the_extracted_tier() {
    // Criterion 2, from the raw on-log JSON (the DocLinkExtracted payload is crate-private, so a
    // rebuild replays exactly this serialized form - the in-crate fold test builds it through the
    // Rust type). The fold arm matches a CLOSED set of five relations and folds each into an edge
    // carrying that relation at the EXTRACTED tier: design-doc --SPECIFIES--> code, arch-decision
    // --CONSTRAINS--> code, handbook-rule --GOVERNS--> code (the REUSED GOVERNS relation), rationale
    // --explains--> code, and design-doc --references--> doc. Dropping any one relation from the
    // fold's match list would redden this, so the closed list is mutation-proven from outside the
    // crate - the boundary an inside-out test cannot guard.
    let cases = [
        (
            REL_SPECIFIES,
            "docs/architecture.md",
            "src/contextgraph/sqlite.rs",
        ),
        (REL_CONSTRAINS, "docs/adr/0001-x.md", "src/conductor.rs"),
        (REL_GOVERNS, "docs/handbook.md", "src/spawn.rs"),
        (REL_EXPLAINS, "src/combat.rs#L7", "src/combat.rs"),
        (
            REL_DOC_REFERENCES,
            "docs/architecture.md",
            "docs/addendum.md",
        ),
    ];
    for (i, (rel, from, to)) in cases.iter().enumerate() {
        let p = Projector::open(":memory:", "test").unwrap();
        apply_json(
            &p,
            (i + 1) as u64,
            TYPE_DOC_LINK_EXTRACTED,
            serde_json::json!({ "from": from, "to": to, "rel": rel }),
        );
        let g = p.subgraph(&[from.to_string()], 1).unwrap();
        assert!(
            has_edge(&g, from, rel, to),
            "a {rel} DocLinkExtracted folds into a {rel} edge at the extracted tier; got {:?}",
            g.edges
        );
    }
}

#[test]
fn a_doc_link_with_an_unrecognized_rel_folds_nothing_and_never_errors() {
    // Defensive relation guard: the fold arm matches the five design-intent relations exactly and
    // returns Ok without touching the graph for any other relation string (`_ => Ok(())`). So a
    // stray or future-vocabulary relation on the log never mints a spurious edge and never wedges a
    // rebuild. The inside-out fold test only ever feeds the five valid relations, so this arm is
    // untested by it.
    let p = Projector::open(":memory:", "test").unwrap();
    apply_json(
        &p,
        1,
        TYPE_DOC_LINK_EXTRACTED,
        serde_json::json!({ "from": "docs/x.md", "to": "src/y.rs", "rel": "not-a-design-relation" }),
    );
    let g = p
        .subgraph(&["docs/x.md".to_string(), "src/y.rs".to_string()], 1)
        .unwrap();
    assert!(
        g.edges.is_empty(),
        "an unrecognized relation folds NO edge; got {:?}",
        g.edges
    );
}

#[cfg(feature = "symbols")]
#[test]
fn the_public_link_emit_lowers_every_link_rel_onto_the_fold_arm_that_matches_it() {
    use rigger::grounder::design::events::link_events;
    use rigger::grounder::design::model::{DesignLink, LinkRel};

    // Emit-to-fold mapping closure for the EDGES, driven through the REAL public API from outside
    // the crate. The emit lowers each link through the single `LinkRel::rel` mapping, and the fold
    // arm matches those exact strings; if the two ever drift, the emit would produce an event the
    // fold silently drops through its `_ => Ok(())` arm. So for EVERY `LinkRel` variant, a link
    // emitted through the public API must fold into an edge whose relation is exactly `rel()`.
    let variants = [
        LinkRel::Specifies,
        LinkRel::Constrains,
        LinkRel::Governs,
        LinkRel::Explains,
        LinkRel::References,
    ];
    for variant in variants {
        let expected = variant.rel();
        let link = DesignLink {
            from: "docs/from.md".to_string(),
            rel: variant,
            to: "src/to.rs".to_string(),
        };
        let events = link_events(std::slice::from_ref(&link));
        assert_eq!(events.len(), 1, "one link emits one event");
        assert_eq!(
            events[0].type_, TYPE_DOC_LINK_EXTRACTED,
            "the emitted event is a DocLinkExtracted"
        );

        let p = Projector::open(":memory:", "test").unwrap();
        let mut e = events.into_iter().next().unwrap();
        e.position = 1;
        p.apply(&e).unwrap();

        let g = p.subgraph(std::slice::from_ref(&link.from), 1).unwrap();
        assert!(
            has_edge(&g, &link.from, expected, &link.to),
            "{variant:?} emits an event the fold turns into a {expected} edge (no silent drop); \
             got {:?}",
            g.edges
        );
    }
}

#[cfg(feature = "symbols")]
#[test]
fn the_public_link_pipeline_folds_edges_that_emanate_from_their_typed_design_intent_nodes() {
    use rigger::grounder::design::events::{concept_events, link_events};
    use rigger::grounder::design::extract::{extract_concepts, extract_links};

    // End to end through the REAL public pipeline (extract -> emit -> fold) from OUTSIDE the crate,
    // for the EDGE half (criterion 2). The in-crate fold tests hand-build payloads and assert only
    // that an edge EXISTS; this drives `extract_links` (the one public API item no periphery test
    // otherwise exercises) plus `extract_concepts` over real doc contents and additionally proves the
    // from-side IDENTITY the criterion names: each design->code edge emanates from a node of the
    // correct design-intent KIND - the concept fold's real node, never a coincidental bare artifact -
    // and the rationale `explains` edge's from-endpoint IS the same `<file>#L<line>` node the rationale
    // CONCEPT folded (the cross-module id the extract_concepts / extract_links seam must agree on, or
    // the edge silently dangles off a parallel node sharing only a literal string). The concepts fold
    // first, so each from-node already carries its design-intent kind when the link's `ensure_node`
    // (which never demotes) folds the edge onto it.
    let sources: [(&str, &str); 4] = [
        (
            "docs/architecture.md",
            "# Reference architecture\n\n\
             The projector `src/contextgraph/sqlite.rs` folds the log.\n\n\
             See the [addendum](docs/architecture-addendum-context-management.md).\n",
        ),
        (
            "docs/adr/0001-code-as-events.md",
            "# Ingest code as events\n\nThis decision binds `src/conductor.rs`.\n",
        ),
        (
            "docs/handbook.md",
            "# Loop-discipline handbook\n\nThe rule governs `src/spawn.rs` role tokens.\n",
        ),
        (
            "src/combat.rs",
            "fn clamp() {}\n// WHY: damage must never go negative\n",
        ),
    ];

    let p = Projector::open(":memory:", "test").unwrap();
    let mut pos = 0u64;
    // Concepts first (the from-side nodes and their design-intent kinds) ...
    for (path, contents) in sources {
        for mut e in concept_events(&extract_concepts(path, contents)) {
            pos += 1;
            e.position = pos;
            p.apply(&e).unwrap();
        }
    }
    // ... then the links (the edges), so each edge folds onto a node that already has its kind.
    for (path, contents) in sources {
        for mut e in link_events(&extract_links(path, contents)) {
            pos += 1;
            e.position = pos;
            p.apply(&e).unwrap();
        }
    }

    let g = p
        .subgraph(
            &[
                "docs/architecture.md".to_string(),
                "docs/adr/0001-code-as-events.md".to_string(),
                "docs/handbook.md".to_string(),
                "src/combat.rs#L2".to_string(),
            ],
            1,
        )
        .unwrap();

    // Each edge exists AND emanates from a node of the design-intent kind the criterion names.
    let check = |from: &str, rel: &str, to: &str, kind: &str| {
        assert!(
            has_edge(&g, from, rel, to),
            "the {rel} edge {from} -> {to} folded; got {:?}",
            g.edges
        );
        assert_eq!(
            kind_of(&g, from),
            Some(kind),
            "the {rel} edge emanates from a {kind} node (the concept fold's real node, not a bare \
             artifact); got {:?}",
            g.nodes
        );
    };
    check(
        "docs/architecture.md",
        REL_SPECIFIES,
        "src/contextgraph/sqlite.rs",
        KIND_DESIGN_DOC,
    );
    check(
        "docs/adr/0001-code-as-events.md",
        REL_CONSTRAINS,
        "src/conductor.rs",
        KIND_ARCH_DECISION,
    );
    check(
        "docs/handbook.md",
        REL_GOVERNS,
        "src/spawn.rs",
        KIND_HANDBOOK_RULE,
    );
    // The rationale `explains` edge lands on the SAME `<file>#L<line>` node the rationale CONCEPT
    // folded - the cross-module id coherence between extract_concepts and extract_links.
    check(
        "src/combat.rs#L2",
        REL_EXPLAINS,
        "src/combat.rs",
        KIND_RATIONALE,
    );
    // A design-doc `references` the doc it cites (doc->doc), from the design-doc node.
    check(
        "docs/architecture.md",
        REL_DOC_REFERENCES,
        "docs/architecture-addendum-context-management.md",
        KIND_DESIGN_DOC,
    );
}

#[cfg(feature = "symbols")]
#[test]
fn the_public_link_pipeline_is_an_order_independent_reproducible_edge_rebuild() {
    // Reproducible-rebuild for the EDGE half across DISCOVERY order (spec 29b: the design half of the
    // graph is a rebuildable projection). The in-crate emit-determinism test shuffles ONE file's
    // links; it cannot prove that pooling links from files WALKED in a different order folds to the
    // same edge projection - the property the reproducible rebuild rests on when a real run enumerates
    // the doc tree in whatever order the filesystem yields. Fold the same multi-file source SET twice,
    // once forward and once with the file order reversed, and prove the folded edge set (from, rel,
    // to, tier) is identical, so the design-intent edge layer is independent of the walk order.
    fn fold_edge_set(order: &[(&str, &str)]) -> Vec<(String, String, String, String)> {
        use rigger::grounder::design::events::link_events;
        use rigger::grounder::design::extract::extract_links;
        let p = Projector::open(":memory:", "test").unwrap();
        let mut pos = 0u64;
        for &(path, contents) in order {
            for mut e in link_events(&extract_links(path, contents)) {
                pos += 1;
                e.position = pos;
                p.apply(&e).unwrap();
            }
        }
        // Seed at every possible from-node (each source doc, and the rationale comment site) so the
        // depth-1 subgraph captures every folded design-intent edge.
        let mut seeds: Vec<String> = order.iter().map(|&(path, _)| path.to_string()).collect();
        seeds.push("src/e.rs#L2".to_string());
        let g = p.subgraph(&seeds, 1).unwrap();
        let mut tuples: Vec<(String, String, String, String)> = g
            .edges
            .iter()
            .map(|e| (e.from.clone(), e.rel.clone(), e.to.clone(), e.tier.clone()))
            .collect();
        tuples.sort();
        tuples
    }

    let sources: [(&str, &str); 3] = [
        (
            "docs/architecture.md",
            "# Reference architecture\n\nUses `src/a.rs`; see [b](docs/b.md).\n",
        ),
        (
            "docs/adr/0001-x.md",
            "# Decision\n\nBinds `src/c.rs` and `src/d.rs`.\n",
        ),
        ("src/e.rs", "fn f() {}\n// WHY: keep it total\n"),
    ];

    let forward = fold_edge_set(&sources);
    let mut reversed = sources;
    reversed.reverse();
    let backward = fold_edge_set(&reversed);

    assert!(
        !forward.is_empty(),
        "the pipeline folded design-intent edges to compare"
    );
    assert_eq!(
        forward, backward,
        "the folded design-intent edge set is independent of source-file discovery order"
    );
}

#[cfg(feature = "symbols")]
#[test]
fn the_scope_gate_ingests_a_design_doc_and_drops_a_usage_doc() {
    use rigger::grounder::design::events::{concept_events, link_events};
    use rigger::grounder::design::extract::{extract_concepts, extract_links};

    // Scope boundary (spec 29b criterion 3), end to end through the REAL public pipeline (extract ->
    // emit -> fold) from outside the crate. Only design/architecture knowledge is ingested: a design
    // doc (the reference architecture) folds into queryable design-intent nodes AND edges, while a
    // user-facing usage doc (how to DRIVE the tool) emits NO events and folds into NO nodes and NO
    // edges at all. The usage doc deliberately carries an inline CODE path and a markdown citation -
    // the exact shapes that WOULD emit a SPECIFIES / references link (and fold their bare-artifact
    // endpoints) were it not gated - so this proves the gate drops the LINK half too, not only the
    // concepts. Both docs feed the SAME pipeline, so the assertion is that the pipeline itself
    // discriminates: the usage doc is dropped on the emit side, never reaching the always-compiled
    // fold.
    let design_doc = "docs/architecture.md";
    let usage_doc = "docs/using-rigger.md";
    let usage_contents = "# Using Rigger\n\nRun `src/main.rs` to drive the tool; see the [readme](README.md).\n\n## Quick start\n";

    // The usage doc emits nothing - neither concepts nor links - so the scope gate drops it before
    // any event is produced.
    let usage_events: Vec<_> = concept_events(&extract_concepts(usage_doc, usage_contents))
        .into_iter()
        .chain(link_events(&extract_links(usage_doc, usage_contents)))
        .collect();
    assert!(
        usage_events.is_empty(),
        "a user-facing usage doc emits zero events (concepts AND links); got {usage_events:?}"
    );

    // The design doc, same shape, DOES emit - proving the empty usage result is the gate, not an
    // extractor that finds nothing.
    let design_contents = "# Reference architecture\n\nThe entry point is `src/main.rs`.\n\n## Node taxonomy\n\n## Edge taxonomy\n";
    let mut events = concept_events(&extract_concepts(design_doc, design_contents));
    events.extend(link_events(&extract_links(design_doc, design_contents)));
    assert!(
        !events.is_empty(),
        "a design doc is ingested and emits events"
    );
    events.extend(usage_events);

    let p = Projector::open(":memory:", "test").unwrap();
    for (i, mut e) in events.into_iter().enumerate() {
        e.position = (i + 1) as u64;
        p.apply(&e).unwrap();
    }

    // The design doc is present as a design-doc node; the usage doc is absent from the graph -
    // neither as a design-intent node nor as a bare-artifact edge endpoint.
    let g = p
        .subgraph(
            &[
                design_doc.to_string(),
                usage_doc.to_string(),
                "src/main.rs".to_string(),
            ],
            1,
        )
        .unwrap();
    assert_eq!(
        kind_of(&g, design_doc),
        Some(KIND_DESIGN_DOC),
        "the design doc folded into a design-doc node; got {:?}",
        g.nodes
    );
    assert!(
        kind_of(&g, usage_doc).is_none(),
        "the usage doc produced NO node - it never emitted an event to fold; got {:?}",
        g.nodes
    );
    assert!(
        g.nodes.iter().all(|n| !n.id.starts_with(usage_doc)),
        "no section node of the usage doc folded either; got {:?}",
        g.nodes
    );
    // No design-intent edge emanates from the dropped usage doc (the link half of the gate).
    assert!(
        g.edges.iter().all(|e| !e.from.starts_with(usage_doc)),
        "no edge emanates from the dropped usage doc; got {:?}",
        g.edges
    );
    // The design doc's own SPECIFIES edge DID fold - the code path is a node because the DESIGN doc
    // specifies it, not because the usage doc mentioned it. This keeps the usage-drop assertions
    // above honest: the edge machinery is live, and only the usage doc's contribution is gated.
    assert!(
        has_edge(&g, design_doc, REL_SPECIFIES, "src/main.rs"),
        "the design doc SPECIFIES the code path it names; got {:?}",
        g.edges
    );
}

#[cfg(feature = "symbols")]
#[test]
fn a_structural_design_doc_carrying_a_usage_word_is_never_dropped_and_folds_its_kind() {
    use rigger::grounder::design::events::concept_events;
    use rigger::grounder::design::extract::extract_concepts;

    // False-drop guard at the FOLD boundary (spec 29b criterion 3), from OUTSIDE the crate. The
    // scope gate's biggest risk is dropping a REAL design doc, so an unambiguous design-structural
    // doc - the reference architecture, an addendum, an ADR / decision, a spec - is design by
    // construction and must survive the gate EVEN when its path or heading carries an end-user usage
    // word, then fold into its own design-intent node kind. The inside-out unit test pins the scope
    // override on the returned Vec; this drives the whole extract -> emit -> fold pipeline and proves
    // the node actually lands in the graph at the right kind, the property a design query depends on.
    // A regression that let the usage signal win over the structural override would silently erase
    // the doc from the graph and redden this.
    let cases: &[(&str, &str, &str)] = &[
        // An ADR whose subject is the installation flow - "installation" is a usage word.
        (
            "docs/adr/0007-installation-flow.md",
            "# Installation flow decision\n\n## Context\n",
            KIND_ARCH_DECISION,
        ),
        // An addendum whose heading mentions usage metering.
        (
            "docs/architecture-addendum-usage-metering.md",
            "# Usage metering\n\n## Meters\n",
            KIND_DESIGN_DOC,
        ),
        // A spec that happens to describe a tutorial subsystem.
        (
            "specs/40-tutorial-engine.md",
            "# Tutorial engine\n\n## Steps\n",
            KIND_DESIGN_DOC,
        ),
        // A load-bearing decision doc titled like a how-to.
        (
            "docs/decisions/how-to-shard.md",
            "# How to shard the store\n",
            KIND_ARCH_DECISION,
        ),
    ];
    for &(path, contents, want_kind) in cases {
        let events = concept_events(&extract_concepts(path, contents));
        assert!(
            !events.is_empty(),
            "a structural design doc is never gated as usage; {path} emitted nothing"
        );
        let p = Projector::open(":memory:", "test").unwrap();
        for (i, mut e) in events.into_iter().enumerate() {
            e.position = (i + 1) as u64;
            p.apply(&e).unwrap();
        }
        let g = p.subgraph(&[path.to_string()], 1).unwrap();
        assert_eq!(
            kind_of(&g, path),
            Some(want_kind),
            "{path} survives the scope gate and folds its own design-intent kind; got {:?}",
            g.nodes
        );
    }
}

#[cfg(feature = "symbols")]
#[test]
fn every_recognized_end_user_usage_shape_is_dropped_before_the_fold() {
    use rigger::grounder::design::events::concept_events;
    use rigger::grounder::design::extract::extract_concepts;

    // Full breadth of the scope gate at the emit + fold boundary. The inside-out unit test iterates
    // the usage shapes over the returned Vec; this drives every DISTINCT usage-doc shape - a PATH
    // signal and a HEADING-only signal - through the real emit and proves each produces ZERO events,
    // so nothing ever reaches the always-compiled fold. A design doc folded alongside them still
    // lands, so the projection carries the design node and NOT ONE usage node. Dropping any single
    // usage signal from the gate would let that shape emit an event and fold a node, reddening both
    // the per-shape empty-events assertion and the no-usage-node assertion.
    let usage: &[(&str, &str)] = &[
        ("README.md", "# Rigger\n\ndrive it\n"),
        ("docs/getting-started.md", "# Getting started\n\ninstall\n"),
        ("docs/quickstart.md", "# Overview\n\nsteps\n"),
        ("docs/tutorial-first-run.md", "# First run\n\nsteps\n"),
        ("docs/how-to-configure.md", "# Configure\n\nsteps\n"),
        ("docs/user-guide.md", "# The guide\n\nsteps\n"),
        ("docs/faq.md", "# Questions\n\nanswers\n"),
        ("docs/troubleshooting.md", "# When it breaks\n\nfixes\n"),
        // Signal in the HEADING only, over an otherwise-neutral path.
        ("docs/overview.md", "# Installation\n\ninstall it\n"),
        ("docs/notes.md", "# Command reference\n\nflags\n"),
    ];

    // Each usage shape emits nothing on its own.
    let mut all_events = Vec::new();
    for &(path, contents) in usage {
        let events = concept_events(&extract_concepts(path, contents));
        assert!(
            events.is_empty(),
            "the usage shape {path} emits zero events; got {events:?}"
        );
        all_events.extend(events);
    }

    // A design doc folded alongside every usage shape is the only thing that lands in the graph.
    let design_doc = "docs/architecture.md";
    all_events.extend(concept_events(&extract_concepts(
        design_doc,
        "# Reference architecture\n\n## Nodes\n",
    )));
    let p = Projector::open(":memory:", "test").unwrap();
    for (i, mut e) in all_events.into_iter().enumerate() {
        e.position = (i + 1) as u64;
        p.apply(&e).unwrap();
    }
    let mut seeds: Vec<String> = usage.iter().map(|&(path, _)| path.to_string()).collect();
    seeds.push(design_doc.to_string());
    let g = p.subgraph(&seeds, 1).unwrap();
    assert_eq!(
        kind_of(&g, design_doc),
        Some(KIND_DESIGN_DOC),
        "the design doc folded alongside the dropped usage docs; got {:?}",
        g.nodes
    );
    for &(path, _) in usage {
        assert!(
            g.nodes.iter().all(|n| !n.id.starts_with(path)),
            "no node folded for the dropped usage doc {path}; got {:?}",
            g.nodes
        );
    }
}

#[cfg(feature = "symbols")]
#[test]
fn the_scope_gate_never_suppresses_source_file_rationale_even_under_a_usage_path() {
    use rigger::grounder::design::events::concept_events;
    use rigger::grounder::design::extract::extract_concepts;

    // The scope gate is a DOC gate: it only ever drops markdown usage docs. A SOURCE file is scanned
    // for inline `# WHY:` rationale regardless of a usage word in its path, because rationale is
    // design intent wherever the code lives (spec 29b). This drives the whole pipeline from outside
    // the crate and proves the rationale node actually folds - a regression that applied the gate to
    // source files (moving the usage check ahead of the markdown branch) would silently erase code
    // rationale from the graph and redden this.
    let path = "src/usage_metering.rs";
    let events = concept_events(&extract_concepts(
        path,
        "fn meter() {}\n// WHY: usage is billed per call\n",
    ));
    assert!(
        !events.is_empty(),
        "a source file's rationale is never gated by the doc scope gate; got nothing"
    );
    let p = Projector::open(":memory:", "test").unwrap();
    for (i, mut e) in events.into_iter().enumerate() {
        e.position = (i + 1) as u64;
        p.apply(&e).unwrap();
    }
    let rationale_id = format!("{path}#L2");
    let g = p.subgraph(std::slice::from_ref(&rationale_id), 1).unwrap();
    assert_eq!(
        kind_of(&g, &rationale_id),
        Some(KIND_RATIONALE),
        "the WHY rationale in a source file under a usage path still folds a rationale node; got {:?}",
        g.nodes
    );
}

#[cfg(feature = "symbols")]
#[test]
fn under_a_handbook_path_a_design_rule_doc_stays_but_a_pure_end_user_guide_is_dropped() {
    use rigger::grounder::design::events::concept_events;
    use rigger::grounder::design::extract::extract_concepts;

    // The handbook nuance (spec 29b criterion 3) at the fold boundary, from outside the crate. A
    // `handbook` path is DELIBERATELY not treated as an unambiguous design-structural doc, because a
    // handbook holds BOTH end-user guides (usage) AND loop-discipline / spec-shape rules (design).
    // So the discrimination is CONTENT-aware, NOT keyed on the filename: a rule doc under the
    // handbook path stays in and folds a handbook-rule node even when its path/heading carries a
    // usage word ("using-", "Using rigger"), while a PURE end-user guide under the same path (no
    // design-rule content) still gates out. Both directions are pinned here.
    //
    // The `rule` path is the REAL repo file `docs/handbook/using-rigger.md` with its real
    // operating-discipline shape (H1 "Using rigger: the operating discipline"; sections "Spec
    // shape", "Base anchoring", "The load-bearing decisions"). A regression that keyed the drop on
    // the "using-" prefix (or the "Using rigger" heading) would silently erase this governing-rules
    // doc from the graph - the exact false-drop spec 29b exists to prevent - and redden this test.
    let rule = "docs/handbook/using-rigger.md";
    let guide = "docs/handbook/quickstart.md";

    // A PURE end-user guide under a handbook path carries only usage signals and no design-rule
    // content, so it still gates out.
    let guide_events = concept_events(&extract_concepts(
        guide,
        "# Quick start\n\nDownload the binary and run the installer to get started.\n",
    ));
    assert!(
        guide_events.is_empty(),
        "a pure end-user guide under a handbook path is gated out; got {guide_events:?}"
    );

    // The real operating-discipline doc under the same path is a loop-discipline / spec-shape rule
    // doc: its content carries design-rule signals, so it stays in even though its path is
    // "using-rigger.md" and its heading opens with "Using rigger".
    let mut events = concept_events(&extract_concepts(
        rule,
        "# Using rigger: the operating discipline\n\nThis chapter is the operating discipline for a rigger run.\n\n## Spec shape\n\nOne observable behavior per criterion.\n\n## Base anchoring\n\nAnchor on the ref you want the work to land on.\n\n## The load-bearing decisions\n\nBlast-radius isolation and fail-closed review keep a run consistent.\n",
    ));
    assert!(
        !events.is_empty(),
        "a loop-discipline / spec-shape rule doc under a handbook path is ingested; got nothing"
    );
    events.extend(guide_events);
    let p = Projector::open(":memory:", "test").unwrap();
    for (i, mut e) in events.into_iter().enumerate() {
        e.position = (i + 1) as u64;
        p.apply(&e).unwrap();
    }
    let g = p
        .subgraph(&[rule.to_string(), guide.to_string()], 1)
        .unwrap();
    assert_eq!(
        kind_of(&g, rule),
        Some(KIND_HANDBOOK_RULE),
        "the real operating-discipline doc folded into a handbook-rule node; got {:?}",
        g.nodes
    );
    assert!(
        g.nodes.iter().all(|n| !n.id.starts_with(guide)),
        "the pure end-user guide under a handbook path produced no node; got {:?}",
        g.nodes
    );
}

#[cfg(feature = "symbols")]
#[test]
fn a_design_word_in_a_non_handbook_usage_doc_does_not_leak_the_handbook_content_keep() {
    use rigger::grounder::design::events::concept_events;
    use rigger::grounder::design::extract::extract_concepts;

    // Handbook SCOPING of the content-aware keep (spec 29b criterion 3), at the fold boundary from
    // outside the crate. The content-aware layer that keeps a rule doc IN is deliberately SCOPED to a
    // handbook path: a handbook is the one doc tree that mixes design rules with end-user guides, so
    // its scope decision is resolved by content there and NOWHERE ELSE. Outside a handbook path the
    // content signal must NOT apply, or a plain end-user usage doc that merely mentions a design word
    // in prose (a README that says "the loop discipline", an FAQ that mentions an "invariant") would
    // be silently KEPT and pollute the code-grounded graph with usage noise - the mirror-image
    // false-KEEP of the false-DROP the remediation fixed.
    //
    // This drives the whole extract -> emit -> fold pipeline and proves the keep is path-scoped: two
    // NON-handbook usage docs (a `readme` path, an `faq` path) whose BODY carries design-rule words
    // still emit ZERO events and fold NO node, while the REAL `docs/handbook/using-rigger.md` with the
    // SAME class of design words IS kept and folds a handbook-rule node - the ONLY difference being
    // the handbook path. A regression that dropped the `is_handbook_path` guard (applied the content
    // signal globally) would let the README and the FAQ emit and fold, reddening this. The other c3
    // tests do not catch that mutation: none of their usage fixtures carries a design-rule word.
    //
    // The two non-handbook docs are genuine usage docs by their path (a top-level README index, an
    // FAQ) whose drop is the CORRECT disposition; the design words in their bodies are contrived
    // purely to prove the content keep does not leak past the handbook boundary, not a claim that the
    // doc is design intent.
    let leaky_usage: &[(&str, &str)] = &[
        (
            "README.md",
            "# Rigger\n\nRun it to build a spec. This project keeps a loop discipline and every \
             run holds an invariant, but this file is a usage index.\n",
        ),
        (
            "docs/faq.md",
            "# Frequently asked questions\n\nQ: is review fail-closed? A: yes. Q: what is blast \
             radius? A: isolation. This is still just an end-user FAQ.\n",
        ),
    ];

    // Each non-handbook usage doc emits nothing: outside a handbook path the design word in its body
    // is inert, so layer 3's `readme` / `faq` path signal drops it.
    let mut all_events = Vec::new();
    for &(path, contents) in leaky_usage {
        let events = concept_events(&extract_concepts(path, contents));
        assert!(
            events.is_empty(),
            "the content keep is scoped to handbook paths, so the non-handbook usage doc {path} is \
             still dropped despite a design word in its body; got {events:?}"
        );
        all_events.extend(events);
    }

    // The SAME class of design words under a handbook path IS kept - the keep is path-scoped, not a
    // global content rule. This is the real repo file `docs/handbook/using-rigger.md`.
    let handbook_rule = "docs/handbook/using-rigger.md";
    all_events.extend(concept_events(&extract_concepts(
        handbook_rule,
        "# Using rigger: the operating discipline\n\nThe operating discipline for a run: every \
         criterion holds an invariant and review is fail-closed.\n",
    )));

    let p = Projector::open(":memory:", "test").unwrap();
    for (i, mut e) in all_events.into_iter().enumerate() {
        e.position = (i + 1) as u64;
        p.apply(&e).unwrap();
    }
    let mut seeds: Vec<String> = leaky_usage
        .iter()
        .map(|&(path, _)| path.to_string())
        .collect();
    seeds.push(handbook_rule.to_string());
    let g = p.subgraph(&seeds, 1).unwrap();

    // The handbook rule doc landed as a handbook-rule node.
    assert_eq!(
        kind_of(&g, handbook_rule),
        Some(KIND_HANDBOOK_RULE),
        "the handbook rule doc is kept by the content-aware layer and folds a handbook-rule node; \
         got {:?}",
        g.nodes
    );
    // Neither non-handbook usage doc folded any node - the content keep never left the handbook path.
    for &(path, _) in leaky_usage {
        assert!(
            g.nodes.iter().all(|n| !n.id.starts_with(path)),
            "no node folded for the non-handbook usage doc {path} - the content keep is handbook-scoped; \
             got {:?}",
            g.nodes
        );
    }
}

#[cfg(feature = "symbols")]
#[test]
fn project_batches_lowers_a_whole_tree_into_per_file_design_batches_the_fold_ingests() {
    // Spec 29c criterion 5's PUBLIC production entry for the DESIGN half: `project_batches` walks a
    // whole tree and returns each file's design-intent batch - a design doc's concepts + links, a
    // source file's `# WHY:` rationale - which the always-compiled fold ingests into design-intent
    // nodes. This is the pass a live run drives to POPULATE the graph (29b built the machinery, 29c
    // wires the caller); exercised at the crate boundary over a real walk of a real tree.
    use rigger::grounder::design::events::project_batches;

    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("docs")).unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(
        dir.path().join("docs/architecture.md"),
        "# Reference architecture\n\n## Node taxonomy\n\nThe `src/combat.rs` module folds nodes.\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("src/combat.rs"),
        "fn clamp() {}\n// WHY: damage must never go negative\n",
    )
    .unwrap();

    let batches = project_batches(dir.path().to_str().unwrap());
    let files: Vec<&str> = batches.iter().map(|(f, _)| f.as_str()).collect();
    // BOTH sources yield a batch (the design doc AND the source rationale), in sorted path order.
    assert_eq!(
        files,
        vec!["docs/architecture.md", "src/combat.rs"],
        "one sorted per-file design-intent batch for the design doc and the rationale source"
    );
    // Folding every batch yields the design-intent nodes a run would populate.
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
    // The design doc folds a design-doc node; the source's `# WHY:` folds a rationale node reachable
    // from the file it explains.
    let g = p.subgraph(&["src/combat.rs".to_string()], 3).unwrap();
    assert!(
        g.nodes.iter().any(|n| n.kind == KIND_RATIONALE),
        "the source's WHY rationale folds as a rationale node the traversal reaches; got {:?}",
        g.nodes
    );
    let gd = p
        .subgraph(&["docs/architecture.md".to_string()], 2)
        .unwrap();
    assert!(
        gd.nodes.iter().any(|n| n.kind == KIND_DESIGN_DOC),
        "the design doc folds a design-doc node; got {:?}",
        gd.nodes
    );
}
