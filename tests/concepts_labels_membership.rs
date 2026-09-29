//! Criterion tests for spec 54's LABELS + NO-FORCED-MEMBERSHIP claim: each derived concept carries
//! its most-central DOCUMENT's title as its label (lexicographic tie-break, document-first over a
//! rationale, with a deterministic fallback when no document is present), and a code node reached by
//! no intent edge belongs to NO concept. They run OUTSIDE the crate, over the library's PUBLIC
//! derivation surface (`rigger::concepts::{intent_layer, derive, events}`), so they pin the label and
//! membership CONTRACT a downstream consumer relies on, not an internal detail:
//!
//!  - the LABEL is chosen by intent-degree, ties broken to the lexicographically-smallest document id
//!    - so the same layer names a concept identically on every machine.
//!  - a RATIONALE (`# WHY:` comment) is an intent-layer member but never a label source while any
//!    titled document is present, even when the rationale is more central - a document names an idea,
//!    a local comment does not.
//!  - a DOCUMENTLESS concept still gets a deterministic label: its most-central member's `name`, else
//!    its id - never empty, never first-seen-by-id.
//!  - the derivation is the LABEL AUTHORITY: the fold carries the pass-computed label verbatim onto
//!    the concept super-node rather than recomputing it, so the label survives the log round trip.
//!  - HONEST MEMBERSHIP: a code node reached only by a non-intent edge (a dev-loop
//!    `decision --GOVERNS--> file`, harness machinery) never enters the intent layer, so it joins no
//!    concept - a concept is an idea, not a bucket of leftovers.
//!
//! Fixture strategy: the community detection groups sparse intent structures into small connected
//! regions, so every fixture that must stay ONE concept is built DENSE - a K3 triangle for the
//! two-document cases (the densest single-concept structure the intent layer admits, since a
//! file-to-file edge never enters it) and a PURE STAR (one hub, degree-1 leaves) for the single-hub
//! cases. Every symbol used is always-compiled, so these run identically in BOTH feature lanes.

mod common;

use common::fixtures::edge;
use common::fixtures::node_with_optional_attrs;
use common::fixtures::pair_map;
use rigger::concepts::{derive, events, intent_layer, Derivation, DEFAULT_RESOLUTION};
use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{
    Graph, KIND_CONCEPT, KIND_DECISION, KIND_DESIGN_DOC, KIND_FILE, KIND_HANDBOOK_RULE,
    KIND_RATIONALE, REL_DOC_REFERENCES, REL_EXPLAINS, REL_GOVERNS, REL_SPECIFIES, TIER_EXTRACTED,
};
use rigger::eventstore::Event;

/// Derive concepts over `g`'s intent layer at the default grain - the public pass the criterion owns.
fn derive_default(g: &Graph) -> Derivation {
    derive(g, &intent_layer(g), DEFAULT_RESOLUTION)
}

/// The label of the concept `member` realizes: the derivation's answer to "what names this member's
/// idea". Panics if `member` joined no concept - a caller that expects membership asserts it first.
fn label_of(d: &Derivation, member: &str) -> String {
    let concept = &pair_map(&d.members)[member];
    pair_map(&d.concepts)[concept].clone()
}

/// Fold `events` into a FRESH in-memory projection (positions assigned in order, as a rebuild replays
/// them) and return the whole live graph - so a test can read what the fold actually persisted.
fn fold(events: &[Event]) -> Graph {
    let p = Projector::open(":memory:", "test").unwrap();
    for (i, e) in events.iter().enumerate() {
        let mut e = e.clone();
        e.position = i as u64 + 1;
        assert_eq!(
            rigger::contextgraph::Fold::of_batch(Some(&p), std::slice::from_ref(&e)),
            rigger::contextgraph::Fold::Folded
        );
    }
    p.whole().unwrap()
}

/// A concept holding a titled design-doc AND a MORE-CENTRAL rationale in ONE connected region. The
/// region is a PURE STAR - the intent structure the community detection reliably keeps as a single
/// concept - whose HUB is the rationale (it explains three files and references the doc, degree 4) and
/// whose leaves are those files plus the doc (each degree 1). So the rationale is strictly the most
/// central node, yet the lone design-doc must name the concept - shared by the rationale-precedence
/// and label-carry tests.
fn doc_with_more_central_rationale() -> Graph {
    let doc = "docs/kg.md";
    let rationale = "src/graph/store.rs#L7";
    Graph {
        nodes: vec![
            node_with_optional_attrs(
                doc,
                KIND_DESIGN_DOC,
                &[("title", Some("The knowledge graph")), ("name", None)],
            ),
            node_with_optional_attrs(
                rationale,
                KIND_RATIONALE,
                &[("title", Some("why we index by name")), ("name", None)],
            ),
            node_with_optional_attrs(
                "src/graph/store.rs",
                KIND_FILE,
                &[("title", None), ("name", None)],
            ),
            node_with_optional_attrs(
                "src/graph/a.rs",
                KIND_FILE,
                &[("title", None), ("name", None)],
            ),
            node_with_optional_attrs(
                "src/graph/b.rs",
                KIND_FILE,
                &[("title", None), ("name", None)],
            ),
        ],
        edges: vec![
            // The rationale is the star hub (degree 4): it explains three files and references the doc.
            edge(
                rationale,
                "src/graph/store.rs",
                REL_EXPLAINS,
                TIER_EXTRACTED,
            ),
            edge(rationale, "src/graph/a.rs", REL_EXPLAINS, TIER_EXTRACTED),
            edge(rationale, "src/graph/b.rs", REL_EXPLAINS, TIER_EXTRACTED),
            // The doc hangs off the hub as a degree-1 leaf - the only titled document in the concept.
            edge(doc, rationale, REL_DOC_REFERENCES, TIER_EXTRACTED),
        ],
    }
}

/// Deriving `g`'s concepts puts documents `winner` and `other` in one concept, labelled by
/// `winner`'s title `label` (`why`).
fn assert_documents_share_a_concept_labelled_by(
    g: &Graph,
    winner: &str,
    other: &str,
    label: &str,
    why: &str,
) {
    let d = derive_default(g);
    let m = pair_map(&d.members);
    assert_eq!(
        m[winner], m[other],
        "both documents share one concept; got {m:?}"
    );
    assert_eq!(label_of(&d, winner), label, "{why}");
}

rigger::test_cases! {
    /// A concept holding TWO document members is named by the one with the HIGHER intent-degree -
    /// the document most connected to the idea, the closest thing the graph has to the idea's human
    /// name. The higher-degree document here (`docs/zzz-kg.md`, a star hub of intent-degree 4) also
    /// carries the lexicographically-LARGER id, so a picker that ranked by id order instead of degree
    /// would choose the other document (`docs/aaa-review.md`, a degree-1 leaf): this pins that
    /// DEGREE, not id order, decides which document names the concept. A PURE STAR keeps both
    /// documents in ONE concept: the hub specifies three files and the leaf hangs off it via a
    /// doc-to-doc reference.
    label_is_the_most_central_document_by_intent_degree: assert_documents_share_a_concept_labelled_by(
        &Graph {
            nodes: vec![
                node_with_optional_attrs("docs/zzz-kg.md", KIND_DESIGN_DOC, &[("title", Some("The knowledge graph")), ("name", None)]),
                node_with_optional_attrs("docs/aaa-review.md", KIND_HANDBOOK_RULE, &[("title", Some("Review adjudication")), ("name", None)]),
                node_with_optional_attrs("src/core/f1.rs", KIND_FILE, &[("title", None), ("name", None)]),
                node_with_optional_attrs("src/core/f2.rs", KIND_FILE, &[("title", None), ("name", None)]),
                node_with_optional_attrs("src/core/f3.rs", KIND_FILE, &[("title", None), ("name", None)]),
            ],
            edges: vec![
                edge("docs/zzz-kg.md", "src/core/f1.rs", REL_SPECIFIES, TIER_EXTRACTED),
                edge("docs/zzz-kg.md", "src/core/f2.rs", REL_SPECIFIES, TIER_EXTRACTED),
                edge("docs/zzz-kg.md", "src/core/f3.rs", REL_SPECIFIES, TIER_EXTRACTED),
                edge("docs/zzz-kg.md", "docs/aaa-review.md", REL_DOC_REFERENCES, TIER_EXTRACTED),
            ],
        },
        "docs/zzz-kg.md",
        "docs/aaa-review.md",
        "The knowledge graph",
        "the higher-degree document names the concept, not the lower-degree (smaller-id) one",
    );
    /// When two documents in one concept have EQUAL intent-degree, the tie breaks to the
    /// lexicographically-smallest document id - a deterministic, machine-independent choice. Both
    /// docs sit in a symmetric K3 at degree 2 (each document and the shared file has intent-degree
    /// 2), so ONLY the tie-break decides the label: a picker that let the later-seen (larger-id)
    /// document win the tie would name it "Omega idea".
    label_ties_break_to_the_lexicographically_smallest_document:
        assert_documents_share_a_concept_labelled_by(
            &Graph {
                nodes: vec![
                    node_with_optional_attrs("docs/a-alpha.md", KIND_DESIGN_DOC, &[("title", Some("Alpha idea")), ("name", None)]),
                    node_with_optional_attrs("docs/z-omega.md", KIND_DESIGN_DOC, &[("title", Some("Omega idea")), ("name", None)]),
                    node_with_optional_attrs("src/shared.rs", KIND_FILE, &[("title", None), ("name", None)]),
                ],
                edges: vec![
                    edge("docs/a-alpha.md", "docs/z-omega.md", REL_DOC_REFERENCES, TIER_EXTRACTED),
                    edge("docs/a-alpha.md", "src/shared.rs", REL_SPECIFIES, TIER_EXTRACTED),
                    edge("docs/z-omega.md", "src/shared.rs", REL_SPECIFIES, TIER_EXTRACTED),
                ],
            },
            "docs/a-alpha.md",
            "docs/z-omega.md",
            "Alpha idea",
            "the tie breaks to the smaller document id, not the later-seen larger one",
        );
}

#[test]
fn a_rationale_is_not_a_label_source() {
    // A rationale (`# WHY:` comment) is a genuine intent-layer MEMBER but not a document that NAMES an
    // idea, so it never labels a concept while any titled document is present - even when the rationale
    // is MORE central. Here the rationale is the concept's highest-degree node (4), yet the lone
    // design-doc (degree 2) names the concept: a picker that ranked by degree ALONE, ignoring the
    // document preference, would wrongly title the concept from the rationale.
    let g = doc_with_more_central_rationale();
    let d = derive_default(&g);
    let m = pair_map(&d.members);
    assert_eq!(
        m["docs/kg.md"], m["src/graph/store.rs#L7"],
        "the doc and the rationale share one concept; got {m:?}"
    );
    assert_eq!(
        label_of(&d, "docs/kg.md"),
        "The knowledge graph",
        "the titled document names the concept, never the more-central rationale"
    );
}

#[test]
fn the_folded_concept_node_carries_the_pass_computed_document_label() {
    // The derivation is the LABEL AUTHORITY: it computes each concept's label once (the most-central
    // document's title) and the events carry it onto the folded concept super-node, which the fold
    // stores verbatim rather than recomputing from the graph. Folding a concept that holds a titled
    // doc plus a more-central rationale, the concept node's `label` attr is the DOC's title - so the
    // honest, document-first label survives the round trip through the event log into the graph.
    let g = doc_with_more_central_rationale();
    let d = derive_default(&g);
    let concept_id = pair_map(&d.members)["docs/kg.md"].clone();

    let folded = fold(&events(&d));
    let concept_node = folded
        .nodes
        .iter()
        .find(|n| n.id == concept_id && n.kind == KIND_CONCEPT)
        .expect("the concept super-node folded from the recorded events");
    assert_eq!(
        concept_node.attrs.get("label").map(String::as_str),
        Some("The knowledge graph"),
        "the fold carries the pass-computed document label onto the concept node, never a recomputed \
         (rationale) one; got {:?}",
        concept_node.attrs
    );
}

/// The hub file of the documentless fixture [`label_of_the_documentless_hub`] labels.
const HUB: &str = "src/zzz_hub.rs";

/// The label of the documentless concept around [`HUB`] (carrying the `name` attr `hub_name`, if
/// any) explained by three rationales: the hub is the most-central member (degree 3) while each
/// rationale is a degree-1 leaf whose id sorts BEFORE the hub's - so a label chosen by id order,
/// not centrality, would name a leaf.
fn label_of_the_documentless_hub(hub_name: Option<&str>) -> String {
    let g = Graph {
        nodes: vec![
            node_with_optional_attrs(HUB, KIND_FILE, &[("title", None), ("name", hub_name)]),
            node_with_optional_attrs(
                "src/aaa.rs#L1",
                KIND_RATIONALE,
                &[("title", Some("why a")), ("name", None)],
            ),
            node_with_optional_attrs(
                "src/bbb.rs#L1",
                KIND_RATIONALE,
                &[("title", Some("why b")), ("name", None)],
            ),
            node_with_optional_attrs(
                "src/ccc.rs#L1",
                KIND_RATIONALE,
                &[("title", Some("why c")), ("name", None)],
            ),
        ],
        edges: vec![
            edge("src/aaa.rs#L1", HUB, REL_EXPLAINS, TIER_EXTRACTED),
            edge("src/bbb.rs#L1", HUB, REL_EXPLAINS, TIER_EXTRACTED),
            edge("src/ccc.rs#L1", HUB, REL_EXPLAINS, TIER_EXTRACTED),
        ],
    };
    label_of(&derive_default(&g), HUB)
}

rigger::test_cases! {
    /// A concept with NO document member (a region of pure rationale over code, or a
    /// high-resolution split) still needs a deterministic label: it falls back to its MOST-CENTRAL
    /// member overall, by intent-degree, and prefers that member's `name` attr over its id. The hub,
    /// not the lexicographically-first leaf, names the concept - pinning that CENTRALITY (degree),
    /// not id order, selects the fallback member.
    a_documentless_concept_falls_back_to_its_most_central_members_name: assert_eq!(
        label_of_the_documentless_hub(Some("the hub file")),
        "the hub file",
        "a documentless concept labels by its most-central member's name attr, not a leaf's title"
    );
    /// The final rung of the fallback: a documentless concept whose most-central member carries no
    /// `name` attr either labels by that member's ID - never empty, always deterministic. The hub
    /// file has no title and no name, so its id names the concept; the lexicographically-first
    /// rationale leaf does NOT, pinning again that DEGREE, not id order, selects the fallback member.
    a_documentless_concept_with_no_named_member_falls_back_to_the_most_central_members_id: assert_eq!(
        label_of_the_documentless_hub(None),
        HUB,
        "a documentless, nameless concept labels by its most-central member's id, not a leaf's title"
    );
}

#[test]
fn a_code_node_with_no_intent_edge_belongs_to_no_concept() {
    // Honest membership: a concept is an IDEA, not a bucket of leftovers. A code node reached only by a
    // NON-intent edge - here a dev-loop `decision --GOVERNS--> file`, whose `decision` endpoint is
    // harness machinery, not a design document - never enters the intent layer, so it joins NO concept.
    // The intent-doc endpoint filter draws that line: drop it and the harness noise (which shares the
    // GOVERNS rel with a real intent edge) would be grouped as if it were an idea.
    let doc = "docs/kg.md";
    let orphan = "src/generated/noise.rs";
    let g = Graph {
        nodes: vec![
            node_with_optional_attrs(
                doc,
                KIND_DESIGN_DOC,
                &[("title", Some("The knowledge graph")), ("name", None)],
            ),
            node_with_optional_attrs(
                "src/graph/a.rs",
                KIND_FILE,
                &[("title", None), ("name", None)],
            ),
            node_with_optional_attrs(
                "src/graph/b.rs",
                KIND_FILE,
                &[("title", None), ("name", None)],
            ),
            node_with_optional_attrs("d-noise", KIND_DECISION, &[("title", None), ("name", None)]),
            node_with_optional_attrs(orphan, KIND_FILE, &[("title", None), ("name", None)]),
        ],
        edges: vec![
            // A real concept: the design-doc specifies two files (a pure star, one connected region).
            edge(doc, "src/graph/a.rs", REL_SPECIFIES, TIER_EXTRACTED),
            edge(doc, "src/graph/b.rs", REL_SPECIFIES, TIER_EXTRACTED),
            // Harness noise: a dev-loop decision GOVERNS a file. Same rel as an intent edge, but
            // NEITHER endpoint is an intent-doc, so the layer must reject it - the file stays orphaned.
            edge("d-noise", orphan, REL_GOVERNS, TIER_EXTRACTED),
        ],
    };
    let d = derive_default(&g);
    let m = pair_map(&d.members);
    // The real concept formed and grouped the document WITH the code it specifies.
    assert_eq!(
        m[doc], m["src/graph/a.rs"],
        "the design-doc concept grouped the code it specifies; got {m:?}"
    );
    assert_eq!(
        m[doc], m["src/graph/b.rs"],
        "the design-doc concept grouped the code it specifies; got {m:?}"
    );
    // The noise nodes joined no concept - the intent-doc endpoint filter rejected their edge.
    assert!(
        !m.contains_key(orphan),
        "a file reached only by a decision --GOVERNS--> file edge joins no concept; got {m:?}"
    );
    assert!(
        !m.contains_key("d-noise"),
        "a dev-loop decision node is harness machinery, not intent, and joins no concept; got {m:?}"
    );
}
