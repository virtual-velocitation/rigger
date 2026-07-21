//! The parser-free design-intent model (spec 29b): the concept kinds a doc yields, the
//! `DesignConcept` the emit lowers into a `DocConceptExtracted` event, and the design-intent LINKS
//! (criterion 2) it lowers into `DocLinkExtracted` events. Framework-free and deterministic -
//! identical docs yield identical concepts and links - so a rebuild re-derives byte-identical nodes
//! and edges. The single mapping from each model enum to the always-compiled vocabulary lives here
//! ([`ConceptKind::node_kind`], [`LinkRel::rel`]), so the emit and the fold can never drift on which
//! concept becomes which node kind, nor which link becomes which edge relation.

use crate::contextgraph::{
    KIND_ARCH_DECISION, KIND_DESIGN_DOC, KIND_HANDBOOK_RULE, KIND_RATIONALE, REL_CONSTRAINS,
    REL_DOC_REFERENCES, REL_EXPLAINS, REL_GOVERNS, REL_SPECIFIES,
};

/// Which design-intent node kind a concept folds into (spec 29b criterion 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConceptKind {
    /// A reference-architecture / `architecture.md` / addendum doc, or one of its `##` sections.
    DesignDoc,
    /// A load-bearing decision / ADR / `design-intent-gaps` entry that constrains the code.
    ArchDecision,
    /// A spec-shape / loop-discipline rule that governs authoring.
    HandbookRule,
    /// A `# WHY:` / `# NOTE:` inline comment capturing the local intent behind a code entity.
    Rationale,
}

impl ConceptKind {
    /// The context-graph node kind string this concept folds into. The ONE mapping from the model
    /// enum to the always-compiled `KIND_*` vocabulary, so the emit and the fold share one story.
    pub fn node_kind(self) -> &'static str {
        match self {
            ConceptKind::DesignDoc => KIND_DESIGN_DOC,
            ConceptKind::ArchDecision => KIND_ARCH_DECISION,
            ConceptKind::HandbookRule => KIND_HANDBOOK_RULE,
            ConceptKind::Rationale => KIND_RATIONALE,
        }
    }
}

/// One design-intent concept extracted from a doc: the parser-free unit the emit lowers into a
/// `DocConceptExtracted` event. A pure function of the source, so identical docs yield identical
/// concepts and a rebuild is byte-stable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesignConcept {
    /// The node kind this concept folds into.
    pub kind: ConceptKind,
    /// The stable node id: a doc's relative path (a whole-doc `design-doc`), `<doc>#<section-slug>`
    /// (a section node), the source path of a decision / rule doc, or `<file>#L<line>` (a
    /// `rationale` comment site).
    pub id: String,
    /// The human-readable title / summary folded onto the node (the heading, the decision title,
    /// the rule text, or the rationale comment).
    pub title: String,
    /// The source doc / file this concept came from - its provenance, so a later criterion's
    /// design-intent edges can key their links off the concept's origin.
    pub doc: String,
}

/// Which design-intent edge relation a link folds into (spec 29b criterion 2). The variant is
/// picked on the emit side from the source node's kind and the link carrier, and lowered here
/// through the ONE mapping to the always-compiled `REL_*` vocabulary, so the emit and the fold can
/// never drift on which link becomes which relation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LinkRel {
    /// `design-doc --SPECIFIES--> code`: a reference-architecture / design doc designs a subsystem.
    Specifies,
    /// `arch-decision --CONSTRAINS--> code`: a load-bearing decision binds this code.
    Constrains,
    /// `handbook-rule --GOVERNS--> code`: a spec-shape / loop-discipline rule governs this file.
    /// Reuses the existing `GOVERNS` relation - no second governs relation is minted.
    Governs,
    /// `rationale --explains--> code`: a `# WHY:` / `# NOTE:` comment explains the code it annotates.
    Explains,
    /// `design-doc --references--> doc`: a markdown link / ADR citation (doc->doc or doc->code).
    References,
}

impl LinkRel {
    /// The context-graph edge relation string this link folds into. The ONE mapping from the model
    /// enum to the always-compiled `REL_*` vocabulary, so the emit and the fold share one story.
    pub fn rel(self) -> &'static str {
        match self {
            LinkRel::Specifies => REL_SPECIFIES,
            LinkRel::Constrains => REL_CONSTRAINS,
            LinkRel::Governs => REL_GOVERNS,
            LinkRel::Explains => REL_EXPLAINS,
            LinkRel::References => REL_DOC_REFERENCES,
        }
    }
}

/// One design-intent link extracted from a doc: the parser-free unit the emit lowers into a
/// `DocLinkExtracted` event. A pure function of the source, so identical docs yield identical links
/// and a rebuild is byte-stable.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DesignLink {
    /// The link's source node id: a doc's relative path (a whole-doc `design-doc` / `arch-decision`
    /// / `handbook-rule` node) or a `<file>#L<line>` rationale comment site.
    pub from: String,
    /// The edge relation this link folds into.
    pub rel: LinkRel,
    /// The link's target node id: the code file / entity path it designs / constrains / governs /
    /// explains, or the doc / code path it cites.
    pub to: String,
}
