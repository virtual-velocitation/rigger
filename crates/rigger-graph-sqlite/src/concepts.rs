//! The concepts lens's tests: [`rigger_domain::concepts`] reached through the facade, beside the
//! sqlite projector its rebuild-from-the-log test folds into.

pub use rigger_domain::concepts::*;

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(any(feature = "store", not(feature = "core")))]
    use crate::contextgraph::sqlite::Projector;
    use crate::contextgraph::{
        Graph, KIND_DESIGN_DOC, KIND_HANDBOOK_RULE, KIND_RATIONALE, REL_EXPLAINS, REL_GOVERNS,
        REL_SPECIFIES,
    };
    #[cfg(any(feature = "store", not(feature = "core")))]
    use crate::eventstore::Event;
    use crate::test_support::edge;
    use crate::test_support::node_with_optional_attrs;
    use std::collections::BTreeMap;
    // `Projection` (the `.whole()` trait) and the two rel constants are needed only by the
    // sqlite-backed rebuild test above, gated the same way for the same reason.
    use crate::contextgraph::{KIND_DECISION, KIND_FILE, TIER_EXTRACTED};
    #[cfg(any(feature = "store", not(feature = "core")))]
    use crate::contextgraph::{REL_IN_COMMUNITY, REL_REALIZES};
    use crate::test_support::pair_map;

    /// The canonical spec-54 intent fixture: TWO documents governing DISJOINT code regions, each
    /// region spanning TWO directories (so a concept groups a doc WITH its code across directory
    /// lines), plus ONE shared rationale. A dev-loop `decision --GOVERNS--> file` is added as NOISE
    /// the intent-layer filter must reject.
    ///
    /// - Concept A ("The knowledge graph"): a `design-doc` SPECIFIES four files across `src/graph` and
    ///   `src/db`.
    /// - Concept B ("Review adjudication"): a `handbook-rule` GOVERNS four files across `src/review`
    ///   and `src/verdict`.
    /// - Concept C (the shared rationale): a `rationale` EXPLAINS `src/graph/index.rs` - a file in the
    ///   SAME directory as concept A's `src/graph/store.rs`, yet NOT part of concept A, because no
    ///   design doc governs it. Its whole point: concepts are grouped by INTENT, not by directory - a
    ///   file's folder never forces its concept.
    fn intent_fixture() -> Graph {
        let doc_a = "docs/kg.md";
        let a_files = [
            "src/graph/store.rs",
            "src/graph/fold.rs",
            "src/db/sqlite.rs",
            "src/db/schema.rs",
        ];

        let doc_b = "docs/review.md";
        let b_files = [
            "src/review/panel.rs",
            "src/review/lens.rs",
            "src/verdict/judge.rs",
            "src/verdict/tally.rs",
        ];

        let rat_file = "src/graph/index.rs";
        let rationale = "src/graph/index.rs#L5";

        let mut nodes = vec![
            node_with_optional_attrs(
                doc_a,
                KIND_DESIGN_DOC,
                &[("title", Some("The knowledge graph")), ("name", None)],
            ),
            node_with_optional_attrs(
                doc_b,
                KIND_HANDBOOK_RULE,
                &[("title", Some("Review adjudication")), ("name", None)],
            ),
            node_with_optional_attrs(
                rationale,
                KIND_RATIONALE,
                &[("title", Some("why index by name")), ("name", None)],
            ),
            node_with_optional_attrs(rat_file, KIND_FILE, &[("title", None), ("name", None)]),
            node_with_optional_attrs("d-noise", KIND_DECISION, &[("title", None), ("name", None)]),
        ];
        for f in a_files.iter().chain(b_files.iter()) {
            nodes.push(node_with_optional_attrs(
                f,
                KIND_FILE,
                &[("title", None), ("name", None)],
            ));
        }

        let mut edges = Vec::new();
        // Region A: the design-doc specifies every region-A file across src/graph + src/db.
        for f in &a_files {
            edges.push(edge(doc_a, f, REL_SPECIFIES, TIER_EXTRACTED));
        }
        // Region B: the handbook-rule governs its four files across src/review + src/verdict.
        for f in &b_files {
            edges.push(edge(doc_b, f, REL_GOVERNS, TIER_EXTRACTED));
        }
        // The shared rationale explains a src/graph file NO design doc governs.
        edges.push(edge(rationale, rat_file, REL_EXPLAINS, TIER_EXTRACTED));
        // NOISE: a dev-loop decision GOVERNS a region-A file. Same rel as an intent edge, but the
        // `decision` node is not an intent-doc, so the layer must EXCLUDE it.
        edges.push(edge("d-noise", a_files[0], REL_GOVERNS, TIER_EXTRACTED));

        Graph { nodes, edges }
    }

    #[test]
    fn derives_concepts_grouping_docs_with_the_code_they_govern_across_directories() {
        // The core criterion-1 claim: the pass derives ONE concept per connected intent region, each
        // grouping a document WITH the code it governs regardless of directory, and the dev-loop
        // GOVERNS noise never enters the layer.
        let g = intent_fixture();
        let layer = intent_layer(&g);
        let d = derive(&g, &layer, DEFAULT_RESOLUTION);
        let m = pair_map(&d.members);

        // Two design docs governing disjoint regions, plus a rationale on a doc-less file, yield three
        // connected intent regions - three concepts.
        assert_eq!(
            d.num_concepts, 3,
            "three connected intent regions, three concepts"
        );

        // Concept A groups the design-doc WITH ALL FOUR files it specifies, spanning src/graph AND
        // src/db - the doc grouped with its code across directory lines.
        let ca = &m["docs/kg.md"];
        for member in [
            "src/graph/store.rs",
            "src/graph/fold.rs",
            "src/db/sqlite.rs",
            "src/db/schema.rs",
        ] {
            assert_eq!(
                &m[member], ca,
                "{member} realizes concept A (a doc grouped with its code across directories)"
            );
        }
        // Concept B groups the handbook-rule WITH all four files it governs, across src/review AND
        // src/verdict.
        let cb = &m["docs/review.md"];
        for member in [
            "src/review/panel.rs",
            "src/review/lens.rs",
            "src/verdict/judge.rs",
            "src/verdict/tally.rs",
        ] {
            assert_eq!(
                &m[member], cb,
                "{member} realizes concept B across directories"
            );
        }
        assert_ne!(ca, cb, "the two doc regions are DISTINCT concepts");

        // Concept 0 holds the lexicographically-smallest member id (deterministic numbering); the
        // smallest id here is `docs/kg.md`, so concept A is `concept/1/0`.
        assert_eq!(
            ca, "concept/1/0",
            "concept/1/0 holds the smallest-id member"
        );

        // The shared rationale groups WITH the code it explains, and joins NEITHER doc region.
        let cc = &m["src/graph/index.rs#L5"];
        assert_eq!(
            &m["src/graph/index.rs"], cc,
            "the rationale groups with the code it explains"
        );
        assert_ne!(cc, ca, "the rationale's concept is not concept A");
        assert_ne!(cc, cb, "the rationale's concept is not concept B");

        // Grouping is by INTENT, not directory: `src/graph/index.rs` sits in the SAME directory as
        // concept A's `src/graph/store.rs`, yet lands in a DIFFERENT concept - no design doc governs
        // it, so its folder never drags it into concept A.
        assert_ne!(
            &m["src/graph/index.rs"], ca,
            "a src/graph file with no design-doc link is NOT forced into concept A by its directory"
        );

        // The dev-loop decision noise node is in NO concept - the intent-layer filter rejected its
        // GOVERNS edge because a `decision` node is not an intent-doc.
        assert!(
            !m.contains_key("d-noise"),
            "a dev-loop decision --GOVERNS--> file is NOT intent and joins no concept"
        );
    }

    #[test]
    fn every_concept_is_a_single_connected_intent_region_never_spanning_disjoint_regions() {
        // No concept mixes region A's nodes with region B's - the disjoint doc regions stay disjoint.
        let g = intent_fixture();
        let d = derive(&g, &intent_layer(&g), DEFAULT_RESOLUTION);
        let m = pair_map(&d.members);
        let region_a: BTreeMap<String, ()> = [
            "docs/kg.md",
            "src/graph/store.rs",
            "src/graph/fold.rs",
            "src/db/sqlite.rs",
            "src/db/schema.rs",
        ]
        .iter()
        .map(|s| (s.to_string(), ()))
        .collect();
        let region_b: BTreeMap<String, ()> = [
            "docs/review.md",
            "src/review/panel.rs",
            "src/review/lens.rs",
            "src/verdict/judge.rs",
            "src/verdict/tally.rs",
        ]
        .iter()
        .map(|s| (s.to_string(), ()))
        .collect();
        // Group members by concept; each concept must be all-A, all-B, or all-other, never mixed.
        let mut by_concept: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (node, concept) in &d.members {
            by_concept
                .entry(concept.clone())
                .or_default()
                .push(node.clone());
        }
        for (concept, members) in &by_concept {
            let has_a = members.iter().any(|n| region_a.contains_key(n));
            let has_b = members.iter().any(|n| region_b.contains_key(n));
            assert!(
                !(has_a && has_b),
                "concept {concept} spans BOTH disjoint regions: {members:?}"
            );
        }
        // And each doc's whole region is intact within its concept.
        assert_eq!(
            m["docs/kg.md"], m["src/graph/store.rs"],
            "region A is one concept"
        );
        assert_eq!(
            m["docs/review.md"], m["src/verdict/tally.rs"],
            "region B is one concept"
        );
    }

    #[test]
    fn derivation_is_byte_identical_across_runs() {
        // Determinism: two independent derivations of the same intent layer produce equal members,
        // equal concepts, an equal hash, and byte-identical events.
        let g = intent_fixture();
        let d1 = derive(&g, &intent_layer(&g), DEFAULT_RESOLUTION);
        let d2 = derive(&g, &intent_layer(&g), DEFAULT_RESOLUTION);
        assert_eq!(
            d1.members, d2.members,
            "members are byte-identical across runs"
        );
        assert_eq!(
            d1.concepts, d2.concepts,
            "concepts + labels are byte-identical"
        );
        assert_eq!(d1.hash, d2.hash, "the pass hash is byte-identical");

        let ev = |d: &Derivation| -> Vec<(String, Vec<u8>)> {
            events(d).into_iter().map(|e| (e.type_, e.data)).collect()
        };
        assert_eq!(ev(&d1), ev(&d2), "the recorded events are byte-identical");
    }

    #[test]
    fn labels_name_each_concept_by_its_most_central_document() {
        // Each concept is labelled by its most-central document member's title. (Precise label
        // semantics are u54c2's to own; this pins the derivation produces a sane, deterministic label
        // so the recorded ConceptDerived is not empty.)
        let g = intent_fixture();
        let d = derive(&g, &intent_layer(&g), DEFAULT_RESOLUTION);
        let labels: BTreeMap<String, String> = d.concepts.iter().cloned().collect();
        // The concept holding docs/kg.md is labelled from that doc; likewise for docs/review.md.
        let m = pair_map(&d.members);
        assert_eq!(labels[&m["docs/kg.md"]], "The knowledge graph");
        assert_eq!(labels[&m["docs/review.md"]], "Review adjudication");
    }

    // Needs a real sqlite-backed EventStore for its rebuild-from-log verification, so it moves
    // with the `Projector` import above under `#[cfg(any(feature = "store", not(feature =
    // "core")))]` - the derive/fold logic it exercises is otherwise fully covered by this
    // module's other (pure) tests.
    #[cfg(any(feature = "store", not(feature = "core")))]
    #[test]
    fn a_rebuild_from_the_recorded_events_reproduces_identical_membership() {
        // The event-sourced claim: folding the recorded events into a FRESH projection reproduces the
        // derivation's membership WITHOUT re-running the pass, and a second fresh rebuild reproduces
        // the SAME rows - so the grouping is a rebuildable projection of the log.
        let g = intent_fixture();
        let d = derive(&g, &intent_layer(&g), DEFAULT_RESOLUTION);
        let evs = events(&d);

        // The members a concept attaches to are nodes the graph holds (spec 101: an attachment on
        // a node the graph does not hold is not live); a decision naming them holds them.
        let held: Vec<&str> = d.members.iter().map(|(m, _)| m.as_str()).collect();
        let rebuild = |events: &[Event]| -> BTreeMap<String, String> {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("graph.db");
            let p = Projector::open(path.to_str().unwrap(), "proj").unwrap();
            crate::test_support::apply_decision(&p, 900_000, "d-hold", "members", &held, "");
            for (i, e) in events.iter().enumerate() {
                let mut e = e.clone();
                e.position = i as u64 + 1;
                assert_eq!(
                    crate::contextgraph::Fold::of_batch(Some(&p), std::slice::from_ref(&e)),
                    crate::contextgraph::Fold::Folded
                );
            }
            // Read the folded REALIZES membership: <member> --REALIZES--> <concept>.
            p.whole()
                .unwrap()
                .edges
                .into_iter()
                .filter(|e| e.rel == REL_REALIZES)
                .map(|e| (e.from, e.to))
                .collect()
        };

        let expected: BTreeMap<String, String> = d.members.iter().cloned().collect();
        let first = rebuild(&evs);
        assert_eq!(
            first, expected,
            "the folded REALIZES edges reproduce the derivation's membership"
        );
        let second = rebuild(&evs);
        assert_eq!(
            second, first,
            "a second rebuild reproduces identical membership"
        );

        // No IN_COMMUNITY edge is minted by the concepts fold (it records only its own relation).
        let no_community = {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("graph.db");
            let p = Projector::open(path.to_str().unwrap(), "proj").unwrap();
            for (i, e) in evs.iter().enumerate() {
                let mut e = e.clone();
                e.position = i as u64 + 1;
                assert_eq!(
                    crate::contextgraph::Fold::of_batch(Some(&p), std::slice::from_ref(&e)),
                    crate::contextgraph::Fold::Folded
                );
            }
            p.whole()
                .unwrap()
                .edges
                .iter()
                .all(|e| e.rel != REL_IN_COMMUNITY)
        };
        assert!(
            no_community,
            "the concepts fold records only REALIZES edges"
        );
    }
}
