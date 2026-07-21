//! The design-intent-to-events emit pass (spec 29b): lowers extracted [`DesignConcept`]s into
//! `DocConceptExtracted` events (one per concept, criterion 1) and extracted [`DesignLink`]s into
//! `DocLinkExtracted` events (one per link, criterion 2), which the always-compiled context-graph
//! fold ingests into `design-doc` / `arch-decision` / `handbook-rule` / `rationale` nodes and the
//! five typed design-intent edges. Design intent thus becomes a rebuildable projection over the
//! event log - the design half of the one graph (spec 29a is the code half). This is the emit half;
//! the fold half lives in `contextgraph::sqlite` and stays compiled in both lanes.

use crate::contextgraph::{
    DocConceptExtracted, DocLinkExtracted, TYPE_DOC_CONCEPT_EXTRACTED, TYPE_DOC_LINK_EXTRACTED,
};
use crate::eventstore::Event;
use crate::grounder::design::model::{DesignConcept, DesignLink};

/// Emit design-intent concepts as events: one `DocConceptExtracted` per concept. Deterministic by
/// construction - the concepts are emitted in a sorted order (by folded node kind, then id) so
/// identical docs yield byte-identical events, and the always-compiled fold turns each into its
/// design-intent node. The concept's kind is lowered through the single [`DesignConcept`] ->
/// `KIND_*` mapping, so the emitted `kind` string is exactly what the fold matches.
pub fn concept_events(concepts: &[DesignConcept]) -> Vec<Event> {
    let mut sorted: Vec<&DesignConcept> = concepts.iter().collect();
    sorted.sort_by(|a, b| {
        a.kind
            .node_kind()
            .cmp(b.kind.node_kind())
            .then_with(|| a.id.cmp(&b.id))
    });
    sorted
        .iter()
        .map(|c| {
            let payload = DocConceptExtracted {
                kind: c.kind.node_kind().to_string(),
                id: c.id.clone(),
                title: c.title.clone(),
                doc: c.doc.clone(),
            };
            Event::new(
                TYPE_DOC_CONCEPT_EXTRACTED,
                serde_json::to_vec(&payload).expect("doc-concept payload serializes"),
            )
        })
        .collect()
}

/// Emit design-intent links as events: one `DocLinkExtracted` per link (spec 29b criterion 2).
/// Deterministic by construction - the links are emitted in a sorted order (by folded edge relation,
/// then from, then to) so identical docs yield byte-identical events, and the always-compiled fold
/// turns each into its typed design-intent edge. The link's relation is lowered through the single
/// [`LinkRel::rel`](crate::grounder::design::model::LinkRel::rel) mapping, so the emitted `rel`
/// string is exactly what the fold matches.
pub fn link_events(links: &[DesignLink]) -> Vec<Event> {
    let mut sorted: Vec<&DesignLink> = links.iter().collect();
    sorted.sort_by(|a, b| {
        a.rel
            .rel()
            .cmp(b.rel.rel())
            .then_with(|| a.from.cmp(&b.from))
            .then_with(|| a.to.cmp(&b.to))
    });
    sorted
        .iter()
        .map(|l| {
            let payload = DocLinkExtracted {
                from: l.from.clone(),
                to: l.to.clone(),
                rel: l.rel.rel().to_string(),
            };
            Event::new(
                TYPE_DOC_LINK_EXTRACTED,
                serde_json::to_vec(&payload).expect("doc-link payload serializes"),
            )
        })
        .collect()
}

/// Walk the WHOLE project tree at `root` and extract every file's design intent into per-file event
/// batches (spec 29c criterion 5): the production entry point that lowers the ACTUAL design docs
/// (and inline source rationale) into the design half of the unified graph, so a live run populates
/// the graph 29b built the machinery for but left with no caller. Walks with the SHARED
/// [`walk_guarded`](crate::grounder::walk_guarded) skeleton (the same skip-dirs / cycle guard the
/// grounders use, so the design ingest never diverges from the code walk), and lowers each readable
/// file through the shared [`extract_concepts`] / [`extract_links`] scope-gated authority: a design
/// doc yields concept + link events, a source file yields its `# WHY:` / `# NOTE:` rationale, and a
/// usage doc (or an unreadable / binary file, which `read_to_string` rejects) yields nothing.
/// Returns `(file, events)` per file in SORTED path order (`walk_guarded` visits in filesystem
/// order, so the collected batches are sorted for a deterministic emit order), skipping a file that
/// carries no design intent. The caller keys each batch on its content, so an unchanged file is not
/// re-ingested.
pub fn project_batches(root: &str) -> Vec<(String, Vec<Event>)> {
    use crate::grounder::design::extract::{extract_concepts, extract_links};
    use crate::grounder::walk_guarded;
    use std::collections::HashSet;
    use std::ops::ControlFlow;
    use std::path::Path;

    let mut batches: Vec<(String, Vec<Event>)> = Vec::new();
    let mut visited = HashSet::new();
    let _ = walk_guarded(Path::new(root), &mut visited, &mut |path| {
        let rel = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned();
        if let Ok(contents) = std::fs::read_to_string(path) {
            let mut events = concept_events(&extract_concepts(&rel, &contents));
            events.extend(link_events(&extract_links(&rel, &contents)));
            if !events.is_empty() {
                batches.push((rel, events));
            }
        }
        ControlFlow::Continue(())
    });
    batches.sort_by(|a, b| a.0.cmp(&b.0));
    batches
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contextgraph::sqlite::Projector;
    use crate::contextgraph::{
        Projection, KIND_ARCH_DECISION, KIND_DESIGN_DOC, KIND_HANDBOOK_RULE, KIND_RATIONALE,
        REL_CONSTRAINS, REL_DOC_REFERENCES, REL_EXPLAINS, REL_GOVERNS, REL_SPECIFIES,
        TIER_EXTRACTED, TYPE_DOC_CONCEPT_EXTRACTED, TYPE_DOC_LINK_EXTRACTED,
    };
    use crate::grounder::design::extract::{extract_concepts, extract_links};

    #[test]
    fn a_doc_extraction_pass_emits_events_the_fold_turns_into_design_intent_nodes() {
        // Criterion 1, end to end: run the real design-intent extraction over each of the four
        // design-intent sources the criterion names, emit the concepts AS events, fold them, and
        // confirm the design-intent layer lives in the projection - a design-doc node from a
        // reference-architecture doc, an arch-decision from a load-bearing decision, a handbook-rule
        // from a spec-shape rule, and a rationale from a `# WHY:` comment - with no mutable side
        // index in the middle.
        let mut concepts = Vec::new();
        concepts.extend(extract_concepts(
            "docs/architecture.md",
            "# Reference architecture\n\n## Node taxonomy\n",
        ));
        concepts.extend(extract_concepts(
            "docs/adr/0001-code-as-events.md",
            "# Code structure ingested as events\n",
        ));
        concepts.extend(extract_concepts(
            "docs/handbook-rules.md",
            "# Loop-discipline rule: one owner per criterion\n",
        ));
        concepts.extend(extract_concepts(
            "src/combat.rs",
            "fn clamp() {}\n// WHY: damage must never go negative\n",
        ));

        let events = concept_events(&concepts);
        assert!(
            events.iter().all(|e| e.type_ == TYPE_DOC_CONCEPT_EXTRACTED),
            "every emitted event is a DocConceptExtracted"
        );

        let p = Projector::open(":memory:", "test").unwrap();
        for (i, mut e) in events.into_iter().enumerate() {
            e.position = (i + 1) as u64;
            p.apply(&e).unwrap();
        }

        let g = p
            .subgraph(
                &[
                    "docs/architecture.md".to_string(),
                    "docs/adr/0001-code-as-events.md".to_string(),
                    "docs/handbook-rules.md".to_string(),
                    "src/combat.rs#L2".to_string(),
                ],
                1,
            )
            .unwrap();
        let has_kind = |kind: &str| g.nodes.iter().any(|n| n.kind == kind);
        assert!(
            has_kind(KIND_DESIGN_DOC),
            "the reference-architecture doc folded into a design-doc node; got {:?}",
            g.nodes
        );
        assert!(
            has_kind(KIND_ARCH_DECISION),
            "the load-bearing decision folded into an arch-decision node; got {:?}",
            g.nodes
        );
        assert!(
            has_kind(KIND_HANDBOOK_RULE),
            "the spec-shape rule folded into a handbook-rule node; got {:?}",
            g.nodes
        );
        assert!(
            has_kind(KIND_RATIONALE),
            "the WHY comment folded into a rationale node; got {:?}",
            g.nodes
        );
    }

    #[test]
    fn the_emit_is_deterministic_and_sorts_by_kind_then_id() {
        // Determinism by construction (spec 29b): identical concepts yield byte-identical events, in
        // a sorted order independent of the order the extractor discovered them.
        let concepts = extract_concepts("docs/architecture.md", "# Title\n\n## Zeta\n\n## Alpha\n");
        let a = concept_events(&concepts);
        let mut shuffled = concepts.clone();
        shuffled.reverse();
        let b = concept_events(&shuffled);
        let bytes = |es: &[Event]| es.iter().map(|e| e.data.clone()).collect::<Vec<_>>();
        assert_eq!(
            bytes(&a),
            bytes(&b),
            "emit order is independent of input order"
        );
    }

    #[test]
    fn a_doc_extraction_pass_emits_events_the_fold_turns_into_the_five_design_intent_edges() {
        // Criterion 2, end to end: run the real design-intent extraction over the sources the
        // criterion names, emit the concepts AND links AS events, fold them, and confirm the five
        // typed design-intent edges live in the projection - design-doc --SPECIFIES--> code,
        // arch-decision --CONSTRAINS--> code, handbook-rule --GOVERNS--> code (reusing REL_GOVERNS),
        // rationale --explains--> code, and design-doc --references--> doc - with no mutable side
        // index in the middle. The concepts fold first so each edge emanates from a real
        // design-intent node of the right kind (the from-side identity the criterion names).
        let sources: [(&str, &str); 4] = [
            (
                "docs/architecture.md",
                "# Reference architecture\n\n\
                 The projector `src/contextgraph/sqlite.rs` folds the log.\n\n\
                 See the [addendum](docs/architecture-addendum-context-management.md) for detail.\n",
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

        let mut events = Vec::new();
        for (path, contents) in sources {
            events.extend(concept_events(&extract_concepts(path, contents)));
            events.extend(link_events(&extract_links(path, contents)));
        }
        assert!(
            events.iter().all(
                |e| e.type_ == TYPE_DOC_CONCEPT_EXTRACTED || e.type_ == TYPE_DOC_LINK_EXTRACTED
            ),
            "the pass emits only DocConceptExtracted / DocLinkExtracted events"
        );

        let p = Projector::open(":memory:", "test").unwrap();
        for (i, mut e) in events.into_iter().enumerate() {
            e.position = (i + 1) as u64;
            p.apply(&e).unwrap();
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
        let has_edge = |from: &str, rel: &str, to: &str| {
            g.edges
                .iter()
                .any(|e| e.from == from && e.rel == rel && e.to == to && e.tier == TIER_EXTRACTED)
        };
        assert!(
            has_edge(
                "docs/architecture.md",
                REL_SPECIFIES,
                "src/contextgraph/sqlite.rs"
            ),
            "a design-doc SPECIFIES the code it designs; got {:?}",
            g.edges
        );
        assert!(
            has_edge(
                "docs/adr/0001-code-as-events.md",
                REL_CONSTRAINS,
                "src/conductor.rs"
            ),
            "an arch-decision CONSTRAINS the code it binds; got {:?}",
            g.edges
        );
        assert!(
            has_edge("docs/handbook.md", REL_GOVERNS, "src/spawn.rs"),
            "a handbook-rule GOVERNS the file it rules (reusing REL_GOVERNS); got {:?}",
            g.edges
        );
        assert!(
            has_edge("src/combat.rs#L2", REL_EXPLAINS, "src/combat.rs"),
            "a rationale explains the code it annotates; got {:?}",
            g.edges
        );
        assert!(
            has_edge(
                "docs/architecture.md",
                REL_DOC_REFERENCES,
                "docs/architecture-addendum-context-management.md"
            ),
            "a design-doc references the doc it cites; got {:?}",
            g.edges
        );
    }

    #[test]
    fn the_link_emit_is_deterministic_and_sorts_by_rel_then_from_then_to() {
        // Determinism by construction (spec 29b): identical links yield byte-identical events, in a
        // sorted order independent of the order the extractor discovered them.
        let links = extract_links(
            "docs/architecture.md",
            "# Title\n\nUses `src/z.rs`, `src/a.rs`; see [x](docs/x.md).\n",
        );
        let a = link_events(&links);
        let mut shuffled = links.clone();
        shuffled.reverse();
        let b = link_events(&shuffled);
        let bytes = |es: &[Event]| es.iter().map(|e| e.data.clone()).collect::<Vec<_>>();
        assert_eq!(
            bytes(&a),
            bytes(&b),
            "link emit order is independent of input order"
        );
    }
}
