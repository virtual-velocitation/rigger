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
