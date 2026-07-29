//! Deterministic intent-layer concept derivation (spec 54, the CONCEPTS lens): the OFFLINE pass that
//! groups the project's INTENT layer - design docs, handbook rules, specs, rationale, and the code
//! that layer governs / specifies / constrains / explains - into CONCEPTS: connected regions of
//! intent that name what the project is ABOUT ("the knowledge graph", "review adjudication"), each
//! grouping documents WITH the code they govern regardless of directory. Concepts cannot be read off
//! ids or directories; they live in the intent layer and in how it attaches to the code.
//!
//! It REUSES the Code lens's deterministic community detection verbatim ([`community::partition`]) -
//! sorted visit order, lexicographic tie-breaks, connected communities, the `--resolution` knob - so
//! the grouping, the numbering, and the determinism are ONE story across both lenses. What differs is
//! only the INPUT LAYER (intent edges, not call coupling; see [`intent_layer`]) and the membership
//! relation it records (`REALIZES`, not `IN_COMMUNITY`). The derivation is EVENT-SOURCED: one
//! [`crate::contextgraph::TYPE_CONCEPT_DERIVED`] event per concept and one
//! [`crate::contextgraph::TYPE_CONCEPT_REALIZED`] per member; the always-compiled fold turns them
//! into the concept super-node plus its `REALIZES` membership edges, so a rebuild reproduces the same
//! grouping from the log without re-running the pass.
//!
//! ALWAYS compiled, exactly like the fold it feeds. Detection reads only folded edges and needs no
//! grammar, so this carries no `symbols` gate: the pass, its determinism, and its connectedness are
//! proven identically in BOTH feature lanes.
//!
//! # Determinism (a hard requirement, not a wish)
//!
//! The same intent layer at the same resolution yields BYTE-IDENTICAL concepts on every run and every
//! machine, so a rebuild from the recorded events reproduces the same membership rows: the grouping
//! inherits the detection's determinism (sorted node order, ascending-representative numbering, no
//! hash-set iteration in the output), the pass content `hash` is FNV-1a over the canonical intent
//! edge set plus the resolution, and every label / member is derived by an order-independent choice.

use std::collections::BTreeMap;

use crate::community::{self, Coupling};
use crate::contextgraph::{
    ConceptDerived, ConceptRealized, Graph, Node, KIND_ARCH_DECISION, KIND_DESIGN_DOC,
    KIND_HANDBOOK_RULE, KIND_RATIONALE, REL_CONSTRAINS, REL_DOC_REFERENCES, REL_EXPLAINS,
    REL_GOVERNS, REL_SPECIFIES, TYPE_CONCEPT_DERIVED, TYPE_CONCEPT_REALIZED,
};
use crate::eventstore::Event;

/// The default resolution grain (spec 54): the same `1.0` the Code lens ships, so the two lenses
/// share one default grain. Re-exported from [`community`] rather than re-declared, so the grain
/// default can never drift between the lenses.
pub use community::DEFAULT_RESOLUTION;

/// The node kinds that make up the INTENT layer's document side (spec 29b): a design / RA doc, a
/// load-bearing decision, a handbook rule, or a `# WHY:` rationale comment. An edge only enters the
/// intent layer if one of its endpoints is one of these - which EXCLUDES a dev-loop
/// `decision --GOVERNS--> file` (a `decision` node is harness machinery, not design intent), so only
/// genuine intent forms a concept.
const INTENT_DOC_KINDS: [&str; 4] = [
    KIND_DESIGN_DOC,
    KIND_ARCH_DECISION,
    KIND_HANDBOOK_RULE,
    KIND_RATIONALE,
];

/// The node kinds that carry a human name for an IDEA, used to LABEL a concept: a design / RA doc, a
/// decision, or a handbook rule (each ingested with a title). A rationale is an intent-layer member
/// but NOT a preferred label source - it is a local `# WHY:` comment, not a document that names an
/// idea - so it labels a concept only through the no-document fallback.
const LABEL_DOC_KINDS: [&str; 3] = [KIND_DESIGN_DOC, KIND_ARCH_DECISION, KIND_HANDBOOK_RULE];

/// Whether `rel` is one of the design-intent relations (spec 29b) the concepts layer groups over:
/// `SPECIFIES`, `CONSTRAINS`, `GOVERNS`, the rationale `explains`, and the doc-to-doc `references`.
fn is_intent_rel(rel: &str) -> bool {
    matches!(
        rel,
        REL_SPECIFIES | REL_CONSTRAINS | REL_GOVERNS | REL_EXPLAINS | REL_DOC_REFERENCES
    )
}

/// Whether a node `kind` is an intent-layer document kind (design-doc / arch-decision / handbook-rule
/// / rationale).
fn is_intent_doc(kind: &str) -> bool {
    INTENT_DOC_KINDS.contains(&kind)
}

/// Whether a node `kind` names an idea and can LABEL a concept (design-doc / arch-decision /
/// handbook-rule).
fn is_label_doc(kind: &str) -> bool {
    LABEL_DOC_KINDS.contains(&kind)
}

/// Build the undirected INTENT layer from the whole projection: every live edge whose relation is an
/// intent relation ([`is_intent_rel`]) AND at least one endpoint is an intent-doc node
/// ([`is_intent_doc`]), collapsed undirected and weighted by multiplicity. Requiring an intent-doc
/// endpoint is what makes this the INTENT layer and not merely "every `GOVERNS` edge": a dev-loop
/// `decision --GOVERNS--> file` (harness machinery) has NO intent-doc endpoint and never enters the
/// layer. A code node with NO intent edge never enters either - so it joins no concept (honest
/// membership: a concept is an idea, not a bucket of leftovers). Reuses the Code lens's shared graph
/// assembly ([`Coupling::from_edges`]) so ONE deterministic detection runs over this layer exactly as
/// it runs over the coupling layer.
pub fn intent_layer(g: &Graph) -> Coupling {
    let kind_of: BTreeMap<&str, &str> = g
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.kind.as_str()))
        .collect();
    let endpoint_is_doc = |id: &str| kind_of.get(id).map(|k| is_intent_doc(k)).unwrap_or(false);
    let edges = g.edges.iter().filter_map(|e| {
        if !is_intent_rel(&e.rel) {
            return None;
        }
        if !(endpoint_is_doc(&e.from) || endpoint_is_doc(&e.to)) {
            return None;
        }
        Some((e.from.clone(), e.to.clone()))
    });
    Coupling::from_edges(edges)
}

/// A deterministic concept derivation produced by [`derive`]: the derived concepts (each an id +
/// label) and every member's `(node_id, concept_id)` mapping, plus the pass `resolution` and content
/// `hash`. `concepts` is in group-number order (`concept/<res>/0`, `/1`, ...); `members` is sorted by
/// node id, so the first entry is the lexicographically-smallest member. Byte-identical for the same
/// intent layer and resolution.
pub struct Derivation {
    /// The resolution grain this pass ran at.
    pub resolution: f64,
    /// The pass's content hash (FNV-1a of the canonical intent edges plus the resolution).
    pub hash: String,
    /// `(concept_id, label)` for every derived concept, in group-number order.
    pub concepts: Vec<(String, String)>,
    /// `(node_id, concept_id)` for every member, sorted by node id.
    pub members: Vec<(String, String)>,
    /// How many distinct concepts the derivation holds.
    pub num_concepts: usize,
}

/// Derive concepts over `layer` (the [`intent_layer`] of `g`) at `resolution`: run the shared
/// deterministic detection, format each group into a `concept/<resolution>/<n>` id, and pick each
/// concept's deterministic label. A pure function of `(g, layer, resolution)` - two calls return
/// equal derivations - and every concept is a connected intent region. An empty intent layer yields
/// an empty derivation.
///
/// LABELS, deterministically: a concept's label is the TITLE of its most-central DOCUMENT member (the
/// highest intent-degree design-doc / arch-decision / handbook-rule; ties broken to the
/// lexicographically-smallest node id, `layer.nodes()` being sorted). A concept with no such document
/// member (possible at a high resolution) falls back to its most-central member's label (its `name`
/// attr, else its id). Reading intent-degree from the layer and the title from `g` makes the label a
/// pure function of the log, so a rebuild re-derives it byte-identically.
pub fn derive(g: &Graph, layer: &Coupling, resolution: f64) -> Derivation {
    let grouping = community::partition(layer, resolution);
    let res_str = format!("{resolution}");
    let nodes = layer.nodes();
    let degs = layer.degrees();

    // Every member's (node_id, concept_id), aligned to the sorted node order.
    let members: Vec<(String, String)> = nodes
        .iter()
        .cloned()
        .zip(
            grouping
                .group_of
                .iter()
                .map(|n| format!("concept/{res_str}/{n}")),
        )
        .collect();

    // Group node INDICES by group number. `grouping.group_of` is walked in ascending index order and
    // the node vector is sorted, so each group's index list is ascending-id - the tie-break the label
    // picker relies on.
    let mut by_group: Vec<Vec<usize>> = vec![Vec::new(); grouping.num_groups];
    for (i, &gnum) in grouping.group_of.iter().enumerate() {
        by_group[gnum].push(i);
    }

    // Node metadata (kind + attrs) for the labels.
    let node_by_id: BTreeMap<&str, &Node> = g.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    let concepts: Vec<(String, String)> = by_group
        .iter()
        .enumerate()
        .map(|(gnum, idxs)| {
            let concept = format!("concept/{res_str}/{gnum}");
            // The most-central DOCUMENT member names the concept; fall back to the most-central
            // member overall when the concept has no document.
            let pick = most_central(idxs, degs, nodes, &node_by_id, true)
                .or_else(|| most_central(idxs, degs, nodes, &node_by_id, false));
            let label = pick
                .map(|i| node_label(&nodes[i], &node_by_id))
                .unwrap_or_default();
            (concept, label)
        })
        .collect();

    Derivation {
        resolution,
        hash: grouping.hash,
        concepts,
        members,
        num_concepts: grouping.num_groups,
    }
}

/// The most-central member of `idxs` (the group's node indices): the one of greatest intent-degree,
/// ties broken to the SMALLEST index (== lexicographically-smallest id, `idxs` being ascending). When
/// `want_doc` is set, only [`is_label_doc`] members are eligible (the document members that name an
/// idea); `None` if the group has none. A strict `>` on the degree keeps the first-seen (smallest
/// index) member on a tie, so the choice is byte-stable.
fn most_central(
    idxs: &[usize],
    degs: &[f64],
    nodes: &[String],
    node_by_id: &BTreeMap<&str, &Node>,
    want_doc: bool,
) -> Option<usize> {
    let mut best: Option<usize> = None;
    for &i in idxs {
        if want_doc {
            let is_doc = node_by_id
                .get(nodes[i].as_str())
                .map(|n| is_label_doc(&n.kind))
                .unwrap_or(false);
            if !is_doc {
                continue;
            }
        }
        best = Some(match best {
            None => i,
            Some(b) if degs[i] > degs[b] => i,
            Some(b) => b,
        });
    }
    best
}

/// A node's human label: its ingested `title` (a design-intent doc), else its `name` attr (a code
/// entity), else its id. Reads from the already-folded `g`, so a rebuild re-derives it identically.
fn node_label(id: &str, node_by_id: &BTreeMap<&str, &Node>) -> String {
    match node_by_id.get(id) {
        Some(n) => n
            .attrs
            .get("title")
            .or_else(|| n.attrs.get("name"))
            .cloned()
            .unwrap_or_else(|| n.id.clone()),
        None => id.to_string(),
    }
}

/// The events recording a [`Derivation`]: one [`TYPE_CONCEPT_DERIVED`] per concept (the FIRST event
/// carrying `fresh: true` - the pass boundary the fold supersedes the resolution grain's prior
/// membership on) followed by one [`TYPE_CONCEPT_REALIZED`] per member. Emitting all concept nodes
/// before any membership means the `fresh` boundary retires the grain's prior `REALIZES` edges and
/// drops its orphan concept nodes ONCE, at the head, before this pass's grouping folds. Appending and
/// folding these events materializes the concepts layer; a rebuild replays them to byte-identical
/// rows.
///
/// An EMPTY derivation yields NO events - the KEEP-LAST-GOOD policy, not an oversight: with no concept
/// there is no event to carry the `fresh` boundary, so the fold never fires the supersession and the
/// grain's last NON-empty grouping stays LIVE (an empty intent layer is the degenerate
/// derivation-run-before-ingest case, where clobbering a good grouping is worse than keeping it),
/// mirroring [`community::events`]. A real SHRINK is a smaller NON-empty derivation that DOES carry
/// `fresh` and DOES supersede.
pub fn events(d: &Derivation) -> Vec<Event> {
    if d.concepts.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(d.concepts.len() + d.members.len());
    for (i, (concept, label)) in d.concepts.iter().enumerate() {
        let payload = ConceptDerived {
            concept: concept.clone(),
            label: label.clone(),
            resolution: d.resolution,
            hash: d.hash.clone(),
            fresh: i == 0,
        };
        let data = serde_json::to_vec(&payload).expect("ConceptDerived always serializes");
        out.push(Event::new(TYPE_CONCEPT_DERIVED, data));
    }
    for (node, concept) in &d.members {
        let payload = ConceptRealized {
            node: node.clone(),
            concept: concept.clone(),
            resolution: d.resolution,
            hash: d.hash.clone(),
        };
        let data = serde_json::to_vec(&payload).expect("ConceptRealized always serializes");
        out.push(Event::new(TYPE_CONCEPT_REALIZED, data));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contextgraph::sqlite::Projector;
    use crate::contextgraph::{
        Edge, Projection, KIND_DECISION, KIND_FILE, REL_IN_COMMUNITY, REL_REALIZES, TIER_EXTRACTED,
    };
    use std::collections::BTreeMap as Map;

    /// One graph [`Node`] of a kind, with optional `title` / `name` attrs.
    fn node(id: &str, kind: &str, title: Option<&str>) -> Node {
        let mut attrs = Map::new();
        if let Some(t) = title {
            attrs.insert("title".to_string(), t.to_string());
        }
        Node {
            id: id.to_string(),
            kind: kind.to_string(),
            attrs,
        }
    }

    /// One live graph [`Edge`] of a rel at the extracted tier (every intent edge folds EXTRACTED).
    fn edge(from: &str, to: &str, rel: &str) -> Edge {
        Edge {
            from: from.to_string(),
            to: to.to_string(),
            rel: rel.to_string(),
            valid_from: 0,
            valid_to: None,
            source: 0,
            tier: TIER_EXTRACTED.to_string(),
        }
    }

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
            node(doc_a, KIND_DESIGN_DOC, Some("The knowledge graph")),
            node(doc_b, KIND_HANDBOOK_RULE, Some("Review adjudication")),
            node(rationale, KIND_RATIONALE, Some("why index by name")),
            node(rat_file, KIND_FILE, None),
            node("d-noise", KIND_DECISION, None),
        ];
        for f in a_files.iter().chain(b_files.iter()) {
            nodes.push(node(f, KIND_FILE, None));
        }

        let mut edges = Vec::new();
        // Region A: the design-doc specifies every region-A file across src/graph + src/db.
        for f in &a_files {
            edges.push(edge(doc_a, f, REL_SPECIFIES));
        }
        // Region B: the handbook-rule governs its four files across src/review + src/verdict.
        for f in &b_files {
            edges.push(edge(doc_b, f, REL_GOVERNS));
        }
        // The shared rationale explains a src/graph file NO design doc governs.
        edges.push(edge(rationale, rat_file, REL_EXPLAINS));
        // NOISE: a dev-loop decision GOVERNS a region-A file. Same rel as an intent edge, but the
        // `decision` node is not an intent-doc, so the layer must EXCLUDE it.
        edges.push(edge("d-noise", a_files[0], REL_GOVERNS));

        Graph { nodes, edges }
    }

    /// The member -> concept map, for clear asserts.
    fn membership(d: &Derivation) -> BTreeMap<String, String> {
        d.members.iter().cloned().collect()
    }

    #[test]
    fn derives_concepts_grouping_docs_with_the_code_they_govern_across_directories() {
        // The core criterion-1 claim: the pass derives ONE concept per connected intent region, each
        // grouping a document WITH the code it governs regardless of directory, and the dev-loop
        // GOVERNS noise never enters the layer.
        let g = intent_fixture();
        let layer = intent_layer(&g);
        let d = derive(&g, &layer, DEFAULT_RESOLUTION);
        let m = membership(&d);

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
        let m = membership(&d);
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
        let m = membership(&d);
        assert_eq!(labels[&m["docs/kg.md"]], "The knowledge graph");
        assert_eq!(labels[&m["docs/review.md"]], "Review adjudication");
    }

    #[test]
    fn a_rebuild_from_the_recorded_events_reproduces_identical_membership() {
        // The event-sourced claim: folding the recorded events into a FRESH projection reproduces the
        // derivation's membership WITHOUT re-running the pass, and a second fresh rebuild reproduces
        // the SAME rows - so the grouping is a rebuildable projection of the log.
        let g = intent_fixture();
        let d = derive(&g, &intent_layer(&g), DEFAULT_RESOLUTION);
        let evs = events(&d);

        let rebuild = |events: &[Event]| -> BTreeMap<String, String> {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("graph.db");
            let p = Projector::open(path.to_str().unwrap(), "proj").unwrap();
            for (i, e) in events.iter().enumerate() {
                let mut e = e.clone();
                e.position = i as u64 + 1;
                p.apply(&e).unwrap();
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
                p.apply(&e).unwrap();
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
