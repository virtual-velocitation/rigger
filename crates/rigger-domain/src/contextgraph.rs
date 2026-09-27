//! The context graph's domain model and its port: the node, edge and relation vocabulary, the
//! graph and call-graph values, the event types and payloads the offline passes record, and the
//! `Projection` trait. The sqlite projector and the query engine live in the root crate's
//! `contextgraph` module.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::eventstore::{Event, Position};

// Node kinds. Rigger's own vocabulary, never a consuming project's domain.
pub const KIND_DECISION: &str = "decision";
pub const KIND_ARTIFACT: &str = "artifact";
pub const KIND_AGENT: &str = "agent";
pub const KIND_GATE: &str = "gate";
pub const KIND_UNIT: &str = "unit";
pub const KIND_LESSON: &str = "lesson";
/// A workflow STAGE node (spec 92 criterion 2, THE WHOLE PRODUCT IS COVERED): one entry of the
/// project's own `.rigger/workflow.yml` `stages:` map (`plan`, `implement`, `checkin`, ...) - the
/// workflow DEFINITION itself, never a live run's per-unit instance (spec 43 de-noised those; a
/// `stage` node is definitional, folded from a `DocConceptExtracted` event exactly like a
/// `design-doc`, never from a run's `UnitStarted`/`UnitIntegrated`). Its id is `stage:<name>`,
/// matching the Design text's own `stage:implement` example. Distinct from [`KIND_UNIT`] (a
/// de-noised run-instance kind no fold projects any more) and reuses [`KIND_GATE`] / [`KIND_AGENT`]
/// for the sibling `gate:<name>` / `agent:<name>` definition nodes this same pass folds.
pub const KIND_STAGE: &str = "stage";
/// A review finding a lens / adversary raised about a unit's files. It is the
/// cross-agent memory the three review tiers communicate THROUGH: a reviewer emits
/// a ReviewFinding, the projector folds it ABOUT the files it concerns, and the
/// later tiers (and concurrent lenses) RETRIEVE it via grounding, never via the
/// conductor hand-threading one agent's stdout into another's prompt.
pub const KIND_FINDING: &str = "finding";
/// A definition site extracted from source (a function, type, module, and so on): the code
/// half of the one graph (spec 29a). Folded from a `CodeEntityExtracted` event, so code
/// structure is a rebuildable projection over the log, not a mutable side index. Its id is
/// `<file>::<name>`; its attrs carry the name, the rigger `Kind` string, the 1-based line, and
/// the language it was parsed as.
pub const KIND_CODE_ENTITY: &str = "code-entity";
/// A source file container node (spec 29a): the `<rel-path>` node that a file's extracted
/// code entities hang off. Folded alongside the entities, so a query can reach a file's
/// structure the same way it reaches the decisions that govern the file.
pub const KIND_FILE: &str = "file";
/// A design-intent doc / doc-section node (spec 29b): a reference-architecture doc,
/// `architecture.md`, or an addendum (and its `##` sections) ingested as first-class design
/// knowledge. Folded from a `DocConceptExtracted` event, so the reference architecture becomes a
/// set of queryable nodes in the very graph it specifies. Its id is the doc's relative path (a
/// whole-doc node) or `<doc>#<section-slug>` (a section node); its attrs carry the title and the
/// source doc. This is the design half of the one graph (the RA / addenda / `architecture.md`).
pub const KIND_DESIGN_DOC: &str = "design-doc";
/// A load-bearing architecture-decision node (spec 29b): an ADR, a `design-intent-gaps` entry, or
/// any recorded decision that CONSTRAINS the code. Folded from a `DocConceptExtracted` event, so
/// an agent editing a subsystem reaches the load-bearing decision that binds it. Distinct from the
/// dev-loop `decision` kind (a `DecisionMade` from the run's own event stream): an `arch-decision`
/// is design knowledge ingested from a doc, keyed by its source path.
pub const KIND_ARCH_DECISION: &str = "arch-decision";
/// A handbook-rule node (spec 29b): a spec-shape or loop-discipline rule that GOVERNS authoring.
/// Folded from a `DocConceptExtracted` event, so a reviewer reaches the rule that governs a file.
pub const KIND_HANDBOOK_RULE: &str = "handbook-rule";
/// A rationale node (spec 29b): a `# WHY:` / `# NOTE:` inline comment attached to a code entity,
/// capturing the LOCAL design intent behind that code. Folded from a `DocConceptExtracted` event;
/// its id is `<file>#L<line>` (the comment's source site), so a later criterion can link it to the
/// entity it explains.
pub const KIND_RATIONALE: &str = "rationale";
/// A coupling-community node (spec 53): a derived subsystem - a set of code entities and files that
/// call and reference each other densely, regardless of directory - grouped by the offline,
/// deterministic community-detection pass. Its id is `community/<resolution>/<n>` (so distinct
/// resolution grains coexist as distinct communities); its attrs carry the `resolution`, the pass's
/// content `hash`, and a deterministic `label` (its highest-degree member's label). Folded from a
/// `CommunityAssigned` event, never computed at request time, so the `lens=code` view is a pure read
/// over an already-projected grouping. Always compiled, so the light lane folds it with the
/// detection pass absent, mirroring the 29a `KIND_CODE_ENTITY` split.
pub const KIND_COMMUNITY: &str = "community";
/// A concept node (spec 54, the CONCEPTS lens): a derived IDEA the project is about - "the knowledge
/// graph", "review adjudication" - a connected region of the INTENT layer (design docs, handbook
/// rules, specs, rationale) plus the code that layer governs/specifies/constrains/explains, grouped
/// by the offline, deterministic derivation. Concepts cannot be read off ids or directories; they
/// span both. Its id is `concept/<resolution>/<n>` (so distinct resolution grains coexist as distinct
/// concepts); its attrs carry the `resolution`, the pass's content `hash`, and a deterministic
/// `label` (the title of its most-central document member). Folded from a `ConceptDerived` event,
/// never computed at request time, so the `lens=concepts` view is a pure read over an
/// already-projected grouping. Always compiled, so the light lane folds it with the derivation pass
/// absent, mirroring the 53 `KIND_COMMUNITY` split.
pub const KIND_CONCEPT: &str = "concept";

// Edge relationships.
pub const REL_DECIDED: &str = "DECIDED";
pub const REL_SUPERSEDES: &str = "SUPERSEDES";
pub const REL_TOUCHES: &str = "TOUCHES";
pub const REL_GOVERNS: &str = "GOVERNS";
pub const REL_GATED_BY: &str = "GATED_BY";
pub const REL_ABOUT: &str = "ABOUT";
pub const REL_BLOCKS: &str = "BLOCKS";
pub const REL_ASSIGNED_TO: &str = "ASSIGNED_TO";
/// The acting reviewer raised this finding (a DECIDED-style provenance link from
/// the `by` agent to the finding node).
pub const REL_RAISED: &str = "RAISED";
/// A file container node CONTAINS a code entity extracted from it (spec 29a): the structural
/// edge from a `file` node to each `code-entity` node folded from that file's definitions.
pub const REL_CONTAINS: &str = "CONTAINS";
/// A file REFERENCES a code symbol (spec 29a): the structural edge folded from an
/// `EdgeInferred` event, from the referencing `file` node to the referenced symbol's
/// file-scoped `code-entity` id. A confidence tier is layered onto this edge by a later
/// criterion; this criterion only makes the structural edge exist.
pub const REL_REFERENCES: &str = "REFERENCES";
/// A caller-attributed call edge (spec 37): `<file>::<caller> --CALLS--> <callee>`, folded from an
/// `EdgeInferred` whose `caller` is set - the enclosing definition the reference was attributed to.
/// It is added ALONGSIDE the file-level [`REL_REFERENCES`] edge (purely additive; the same callee
/// resolution the REFERENCES edge uses), so one `subgraph` around a symbol answers "who calls it" by
/// function, not merely "referenced from which file". A reference outside every definition
/// (a top-level `use`/import) carries no caller and folds no CALLS edge. Re-extraction supersedes a
/// file's CALLS edges under the same `fresh` batch boundary as its other structural edges (spec 29a).
pub const REL_CALLS: &str = "CALLS";
/// A `design-doc` SPECIFIES (designs) a code node (spec 29b): the design-intent edge from a
/// reference-architecture / `architecture.md` / addendum node to the subsystem it designs, so a
/// `subgraph` traversal from a touched file reaches the RA section that designed it. Folded from a
/// `DocLinkExtracted` event at [`TIER_EXTRACTED`] (an explicit design fact recorded on the log).
pub const REL_SPECIFIES: &str = "SPECIFIES";
/// An `arch-decision` CONSTRAINS a code node (spec 29b): the design-intent edge from a
/// load-bearing decision / ADR / `design-intent-gaps` entry to the code it binds, so an agent
/// editing a subsystem reaches the decision that constrains it. Folded from a `DocLinkExtracted`
/// event at [`TIER_EXTRACTED`]. A `handbook-rule` reuses [`REL_GOVERNS`] for its rule-governs-code
/// edge (no second governs relation is minted).
pub const REL_CONSTRAINS: &str = "CONSTRAINS";
/// A `rationale` EXPLAINS a code node (spec 29b): the design-intent edge from a `# WHY:` / `# NOTE:`
/// comment site to the code it explains (its file), so a traversal reaches the local intent behind
/// an entity. Folded from a `DocLinkExtracted` event at [`TIER_EXTRACTED`]. Lower-case by spec, to
/// read as a design-intent relation distinct from the upper-case dev-loop / code rels.
pub const REL_EXPLAINS: &str = "explains";
/// A `design-doc` REFERENCES another doc / code node (spec 29b): the design-intent edge folded from
/// a markdown link / ADR citation (doc->doc or doc->code), so a cited addendum or subsystem is
/// reachable from the doc that cites it. Folded from a `DocLinkExtracted` event at
/// [`TIER_EXTRACTED`]. Lower-case `references` (a doc citation) is deliberately distinct from the
/// upper-case [`REL_REFERENCES`] code-symbol structural edge (spec 29a) - two relations, two id
/// spaces, never conflated.
pub const REL_DOC_REFERENCES: &str = "references";
/// A code node is `IN_COMMUNITY` a derived coupling community (spec 53): the membership edge from a
/// code entity / file node to its [`KIND_COMMUNITY`] super-node, folded from a `CommunityAssigned`
/// event. Every node carries at most ONE live membership per resolution grain (a re-run at a
/// resolution supersedes that grain's prior memberships; other grains stay live), so the
/// `lens=code` view buckets each member by its one live community. Folded at [`TIER_INFERRED`] - a
/// derived grouping, one confidence step below the explicit structural edges it is detected over.
pub const REL_IN_COMMUNITY: &str = "IN_COMMUNITY";
/// A node REALIZES a derived concept (spec 54): the membership edge from a member node (a design-doc,
/// handbook-rule, spec, rationale, or the code entity / file the intent layer attaches to) to its
/// [`KIND_CONCEPT`] super-node, folded from a `ConceptRealized` event. Direction: the concept is
/// realized BY its members (`<member> --REALIZES--> <concept>`), recorded in ONE direction
/// consistently so the projection queries it uniformly. Every node carries at most ONE live
/// membership per resolution grain (a re-run at a resolution supersedes that grain's prior
/// memberships; other grains stay live), so the `lens=concepts` view buckets each member by its one
/// live concept. Folded at [`TIER_INFERRED`] - a derived grouping, one confidence step below the
/// explicit intent edges it is detected over, mirroring the 53 `IN_COMMUNITY` split.
pub const REL_REALIZES: &str = "REALIZES";
/// A workflow `stage` NEEDS another stage (spec 92 criterion 2): the dependency edge folded from a
/// stage's `needs:` list in `.rigger/workflow.yml` (`stage:implement --NEEDS--> stage:plan-critique`),
/// so "how is a stage's needs satisfied" has both this definition edge and the conductor's
/// `need_satisfied` function on one page. Folded from a `DocLinkExtracted` event at
/// [`TIER_EXTRACTED`], mirroring the design-intent relations - a definitional fact, not a derived
/// grouping.
pub const REL_NEEDS: &str = "NEEDS";
/// A workflow `stage` RUNS a gate or its assigned agent (spec 92 criterion 2): the edge folded from
/// a stage's `gates:` list (`stage:checkin --RUNS--> gate:mutation`, matching the Design text's own
/// example pair) and from its `agent:` / `agents:` field (`stage:implement --RUNS--> agent:rust-
/// engineer`) - both read as "this stage's execution runs X". Folded from a `DocLinkExtracted` event
/// at [`TIER_EXTRACTED`].
pub const REL_RUNS: &str = "RUNS";
/// An `agent` REVIEWS a workflow stage (spec 92 criterion 2): the edge from a reviewer role to the
/// stage its verdict gates - a standalone stage's own `adversary:` / `adjudicator:` fields (e.g.
/// `plan-critique`), or a per-unit stage's effective review panel's OWN top-level roster
/// (`defaults.review`, or its own `review:` override) - lenses, adversary, and adjudicator alike.
/// Deliberately EXCLUDES a panel's opt-in `tiers.light` reduced roster (see
/// [`REL_REVIEWS_LIGHT`]): a real run routes each unit to the light OR the full panel exclusively
/// by observable risk (`config::Workflow::tiers` doc), never both, so this edge must name only the
/// roster a unit at this edge's stage actually gets when it is NOT routed light. Folded from a
/// `DocLinkExtracted` event at [`TIER_EXTRACTED`].
pub const REL_REVIEWS: &str = "REVIEWS";
/// An `agent` REVIEWS a workflow stage under its `tiers.light` REDUCED roster only (spec 92
/// criterion 2, spec 03 "adaptive review depth"): the edge from a light-tier-only reviewer role to
/// the stage a LOW-risk unit routes it to. Kept as a DISTINCT relation from [`REL_REVIEWS`] on
/// purpose - `config::ReviewPanel::agent_ids()` unions this roster with the full panel's for
/// REFERENTIAL VALIDATION (a different question: "does this id resolve to a real agent"), but
/// folding both into one undistinguished edge would assert that a light-only agent reviews a
/// HIGH-risk unit at that stage (and the reverse for a full-panel-only agent under a LOW-risk
/// routing) - a real accuracy defect this relation exists to avoid. Folded from a
/// `DocLinkExtracted` event at [`TIER_EXTRACTED`].
pub const REL_REVIEWS_LIGHT: &str = "REVIEWS_LIGHT";

// Edge confidence tiers (spec 29a, addendum 6.2). Every folded edge carries one, the
// `precise`/`safe` split of the two-view blast radius made a first-class edge attribute. The
// three tiers partition the reference set so their UNION stays a superset of the grep union
// (addendum 2.4): a later traversal reads the EXTRACTED sub-graph as the precise prompt seed and
// EXTRACTED u INFERRED u AMBIGUOUS as the safe superset the safety consumers need.
/// An explicit-in-source structural fact: a definition's containment, or a reference resolved to a
/// definition in the SAME file (a call / import / inherit of a known local symbol). The highest
/// confidence tier - the precise seed. Every non-code dev-loop edge (DECIDED / GOVERNS / ABOUT /
/// SUPERSEDES / ...) also folds EXTRACTED: they are explicit facts recorded on the log.
pub const TIER_EXTRACTED: &str = "extracted";
/// A derived / transitive link: a reference whose name is NOT defined in the referencing file but
/// IS defined in ANOTHER file the graph knows. The reference is inferred to reach that definition
/// across files - real, but one confidence step below an explicit same-file reference.
pub const TIER_INFERRED: &str = "inferred";
/// A grep-visible-only occurrence: a reference whose name is defined NOWHERE the graph knows - a
/// macro body, a reflection string, a dynamic name, an external symbol. It is kept (never dropped)
/// so the safe superset stays a grep-superset, but tiered lowest: the structural pass cannot
/// confirm it resolves to any definition.
pub const TIER_AMBIGUOUS: &str = "ambiguous";

/// The metadata key carrying the acting agent on an event (the DECIDED source).
pub const META_ACTOR: &str = "actor";

/// A node in the graph: a decision, artifact, agent, gate, unit, or lesson.
///
/// `Serialize`/`Deserialize` (spec 93 criterion 5): the wire form [`query::graph_load`] parses a
/// [`Graph`] payload from and the future console page would send - additive, since nothing
/// previously depended on these types NOT round-tripping through JSON.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub kind: String,
    pub attrs: BTreeMap<String, String>,
}

/// A typed, bi-temporal edge. `valid_to == None` means it currently holds; a set
/// value means it was invalidated (superseded) and is never deleted.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub rel: String,
    pub valid_from: i64,
    pub valid_to: Option<i64>,
    pub source: Position,
    /// The confidence tier this edge was folded at: one of [`TIER_EXTRACTED`], [`TIER_INFERRED`],
    /// [`TIER_AMBIGUOUS`] (spec 29a, addendum 6.2). The `precise`/`safe` blast-radius split made a
    /// first-class edge attribute; a later traversal filters on it.
    pub tier: String,
}

/// A set of nodes and the edges among them (e.g. a Subgraph result).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

/// The direction of a directed `CALLS` traversal (spec 52). `Down` follows CALLEES - the
/// EXECUTION PATH, "what does this call, transitively"; `Up` follows CALLERS - the CALL SITES,
/// "who calls this". A [`Projection::calls`] walk is depth-bounded and layered in exactly one
/// direction, over the caller-attributed `CALLS` edges (spec 37) the undirected
/// [`subgraph`](Projection::subgraph) can only reach neighbor-wise.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Callees: the transitive execution path out of the seed.
    Down,
    /// Callers: the transitive call sites into the seed.
    Up,
}

/// One node in a [`CallGraph`] (spec 52): the underlying graph [`Node`] plus its traversal
/// metadata. `layer` is the node's hop distance from the seed (the seed is layer 0), which the
/// left-to-right renderer maps to an x position. `frontier`, when `Some`, marks a multi-candidate
/// cross-file hop the walk did NOT descend, carrying its SORTED candidate definition ids - the
/// human re-seeds on one to continue, so the view stays honest (it may be INCOMPLETE but is never
/// confidently wrong). `None` is a fully-resolved node the walk reached normally.
#[derive(Clone, Debug)]
pub struct CallNode {
    pub node: Node,
    pub layer: i64,
    pub frontier: Option<Vec<String>>,
}

/// One edge in a [`CallGraph`] (spec 52): the underlying graph [`Edge`] plus `back`, set when the
/// edge points at a node whose layer is NOT deeper than the edge's source - a recursion / mutual
/// call the walk marks rather than following a second time (reached nodes dedup into a DAG, so a
/// cycle terminates instead of duplicating).
#[derive(Clone, Debug)]
pub struct CallEdge {
    pub edge: Edge,
    pub back: bool,
}

/// The result of a directed `CALLS` traversal (spec 52): a layered DAG of code entities reachable
/// from the seed by following `CALLS` edges in one [`Direction`], deduped under cycles. A `Down`
/// walk is the execution path; an `Up` walk is the call sites. Shaped like a [`Graph`] but with
/// per-node `layer`/`frontier` and per-edge `back` metadata the neighborhood view does not carry.
///
/// `referenced_not_called` is the UP direction's flat, NON-traversed sidecar (empty for `Down`): the
/// FILE nodes that reference the seed's name at file level (a `REFERENCES` edge to the name) but
/// carry NO caller-attributed `CALLS` edge to it from within the file - the imports / uses a
/// "who-uses-this" reader cares about, which the layered caller DAG (functions that CALL the seed)
/// deliberately does not include. Sorted by id, deterministic across polls.
#[derive(Clone, Debug, Default)]
pub struct CallGraph {
    pub nodes: Vec<CallNode>,
    pub edges: Vec<CallEdge>,
    pub referenced_not_called: Vec<Node>,
}

/// The resolution of a `rigger graph --show <entity>` query (spec 58, the TEXT half of lookup).
/// A PORT-owned type (spec 92's fix round moved it here from the sqlite adapter, so
/// [`Projection::locate`] returns the same kind of type every sibling `Projection` method does,
/// never a concrete adapter's own type): [`sqlite::Projector::locate`] resolves the query exactly
/// the way the graph's other surfaces do - a full `<file>::<name>` node id, or a bare name matched
/// by the pinned name-suffix expression - and returns one of three honest outcomes, never a guess
/// among candidates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Located {
    /// Exactly one entity resolved: its definition site and graph facts, from which the CLI reads
    /// the line-numbered body out of the working tree.
    One(EntitySite),
    /// An ambiguous bare name (several definitions share it): the SORTED candidate sites the caller
    /// picks from. The show surface prints these and NO body (the call-views honesty rule).
    Many(Vec<Candidate>),
    /// Nothing in the graph matched the query.
    None,
}

/// A single located code entity (spec 58): where its definition lives and how it sits in the graph,
/// so the show surface can print the site header and bound the body it reads from the working tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntitySite {
    /// The full `<file>::<name>` node id.
    pub id: String,
    /// The definition kind (`function`, `type`, ...), from the node's `kind` attr; falls back to
    /// the node's graph kind when a bare placeholder carries no definition attr.
    pub kind: String,
    /// The definition's file (the id prefix before `::`) - the working-tree path the body reads.
    pub file: String,
    /// The 1-based line of the definition site, from the node's `line` attr (`0` when unknown).
    pub line: u32,
    /// The entity's one-hop degree: the count of currently-live edges incident to it, so the reader
    /// knows how connected the entity is.
    pub degree: usize,
}

/// One disambiguation candidate for an ambiguous bare name (spec 58): the full node id and its
/// file. [`sqlite::Projector::locate`] returns these SORTED by id, so the listing is deterministic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    /// The candidate's full `<file>::<name>` node id.
    pub id: String,
    /// The candidate's file (the id prefix before `::`).
    pub file: String,
}

// Event type discriminators carried in Event.type_.
pub const TYPE_DECISION_MADE: &str = "DecisionMade";
pub const TYPE_FILE_TOUCHED: &str = "FileTouched";
pub const TYPE_GATE_VERDICT: &str = "GateVerdict";
pub const TYPE_UNIT_STARTED: &str = "UnitStarted";
pub const TYPE_UNIT_INTEGRATED: &str = "UnitIntegrated";
pub const TYPE_LESSON_LEARNED: &str = "LessonLearned";
pub const TYPE_ALIAS_DEFINED: &str = "AliasDefined";
pub const TYPE_ALIAS_UNRESOLVED: &str = "AliasUnresolved";
/// A review finding a lens / adversary raised about a unit's files. Folded into a
/// KIND_FINDING node ABOUT each file, plus a RAISED edge from the acting reviewer.
pub const TYPE_REVIEW_FINDING: &str = "ReviewFinding";
/// One definition extracted from a source file (spec 29a): the extraction pass emits one per
/// definition, and the always-compiled fold turns it into a `code-entity` node plus a
/// `CONTAINS` edge from the file container node. Always compiled, so the light lane folds it
/// with the extraction pass absent.
pub const TYPE_CODE_ENTITY_EXTRACTED: &str = "CodeEntityExtracted";
/// One reference extracted from a source file (spec 29a): the extraction pass emits one per
/// reference, and the always-compiled fold turns it into a `REFERENCES` structural edge from
/// the file container node to the referenced symbol's code-entity id.
pub const TYPE_EDGE_INFERRED: &str = "EdgeInferred";
/// One design-intent concept extracted from a doc (spec 29b): the design-intent extraction pass
/// emits one per concept, and the always-compiled fold turns it into a `design-doc` /
/// `arch-decision` / `handbook-rule` / `rationale` node. Always compiled, so the light lane folds
/// a design-intent log with the extraction pass absent - the fold arm and the node kinds live
/// outside the feature that gates the extraction, mirroring the 29a `CodeEntityExtracted` split.
pub const TYPE_DOC_CONCEPT_EXTRACTED: &str = "DocConceptExtracted";
/// One design-intent link extracted from a doc (spec 29b): the design-intent extraction pass emits
/// one per link, and the always-compiled fold turns it into a typed design-intent edge -
/// `design-doc --SPECIFIES--> code`, `arch-decision --CONSTRAINS--> code`,
/// `handbook-rule --GOVERNS--> code` (reusing `REL_GOVERNS`), `rationale --explains--> code`, and
/// `design-doc --references--> doc`. Always compiled, so the light lane folds a design-intent log
/// with the extraction pass absent - the fold arm and the edge relations live outside the feature
/// that gates the extraction, mirroring the 29a `EdgeInferred` split.
pub const TYPE_DOC_LINK_EXTRACTED: &str = "DocLinkExtracted";

/// Whether folding a batch of `type_` SUPERSEDES the subject's prior assertions (retiring them and
/// re-asserting at the new batch's valid-time) rather than RE-ASSERTING them in place.
///
/// The two halves of the derived index fold differently, and this is the ONE statement of which is
/// which. The CODE half (`CodeEntityExtracted` / `EdgeInferred`) carries a `fresh` batch head that
/// retires the file's prior structural edges before the batch folds its own (spec 29a criterion
/// 3), so a re-extraction REPLACES: the live edge's valid-time is the LATEST batch's. The DESIGN
/// half (`DocLinkExtracted` / `DocConceptExtracted`) sets no `fresh` head at all, so a
/// re-recording re-asserts an existing edge through the upsert-live path, which keeps the
/// EARLIEST valid-time (spec 40: the fact has held since it first became true).
///
/// It lives HERE, beside the type constants and the fold that implements it, because it is a FACT
/// ABOUT THE FOLD. It is published because log MAINTENANCE has to respect it: a compaction that
/// deletes a key's earlier recordings must carry the earliest valid-time onto the survivor for the
/// re-asserting half (or silently re-date the fact) and must NOT for the superseding half (whose
/// live valid-time is the surviving recording's own). One predicate, so the fold and the
/// maintenance can never come to disagree about which half a type is in.
pub fn refold_supersedes_prior_edges(type_: &str) -> bool {
    matches!(type_, TYPE_CODE_ENTITY_EXTRACTED | TYPE_EDGE_INFERRED)
}
/// One community membership the offline detection pass emits (spec 53): the pass records one
/// `CommunityAssigned` per member node, and the always-compiled fold turns it into a `KIND_COMMUNITY`
/// super-node plus a live `IN_COMMUNITY` membership edge. Always compiled, so the light lane folds a
/// community log with the detection pass absent, mirroring the 29a `CodeEntityExtracted` split.
pub const TYPE_COMMUNITY_ASSIGNED: &str = "CommunityAssigned";
/// One concept the offline intent-derivation pass emits (spec 54): the pass records one
/// `ConceptDerived` per derived concept, and the always-compiled fold turns it into a
/// [`KIND_CONCEPT`] super-node carrying the pass-computed `label`. Always compiled, so the light lane
/// folds a concept log with the derivation pass absent - the node kind and this arm live outside the
/// feature that gates derivation, mirroring the 53 `CommunityAssigned` split. The FIRST event of a
/// pass carries `fresh`, the supersession boundary the fold retires the grain's prior membership on.
pub const TYPE_CONCEPT_DERIVED: &str = "ConceptDerived";
/// One concept membership the offline intent-derivation pass emits (spec 54): the pass records one
/// `ConceptRealized` per member, and the always-compiled fold turns it into a live
/// `<member> --REALIZES--> <concept>` edge. Always compiled, so the light lane folds a concept log
/// with the derivation pass absent - the relation and this arm live outside the feature that gates
/// derivation, mirroring the 53 `CommunityAssigned` split.
pub const TYPE_CONCEPT_REALIZED: &str = "ConceptRealized";

/// The `CommunityAssigned` payload (spec 53): one membership the offline community-detection pass
/// emits. Like the 29a/29b payloads it is the ONE serialization contract shared by both sides of the
/// log - the feature-gated detection pass (a later unit) constructs and serializes it, and this
/// always-compiled fold deserializes it - so the field names can never drift between emitter and
/// folder. The fold projects it into a [`KIND_COMMUNITY`] node plus a live
/// `<node> --IN_COMMUNITY--> <community>` edge; the community node's deterministic `label` is
/// computed BY the fold (the highest-degree live member), so nothing waits on a model and a rebuild
/// re-derives it byte-identically.
#[derive(Serialize, Deserialize)]
pub struct CommunityAssigned {
    /// The member node id being assigned: an existing code-entity (`<file>::<name>`) or file node.
    pub node: String,
    /// The community id this member joins: `community/<resolution>/<n>`. Encodes the resolution
    /// grain, so distinct grains coexist as distinct communities and the `fresh` reset can scope
    /// itself to one grain by the `community/<resolution>/` prefix.
    pub community: String,
    /// The resolution grain this pass ran at (default 1.0). Folded onto the community node's
    /// `resolution` attr so the `lens=code` view can select a grain.
    #[serde(default)]
    pub resolution: f64,
    /// The pass's content hash (a deterministic digest of the input coupling graph + resolution).
    /// Folded onto the community node's `hash` attr, so a consumer can tell which pass produced a
    /// grouping without re-running detection.
    #[serde(default)]
    pub hash: String,
    /// Set `true` on the FIRST event of a pass (mirrors [`CodeEntityExtracted::fresh`]). It marks
    /// the pass boundary: the fold SUPERSEDES (sets `valid_to` on, never deletes) every live
    /// `IN_COMMUNITY` edge of THIS resolution grain before folding this pass's memberships, so a
    /// re-run at a resolution REPLACES that grain's assignment set rather than accreting - and
    /// leaves every OTHER resolution grain's memberships live. On the first-ever pass it supersedes
    /// nothing. Rides the existing event and defaults `false`, so a pre-field log folds as
    /// non-boundary.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fresh: bool,
}

/// The `ConceptDerived` payload (spec 54): one concept the offline intent-derivation pass emits. Like
/// the 53 `CommunityAssigned` payload it is the ONE serialization contract shared by both sides of
/// the log - the feature-gated derivation pass (`concepts`) constructs and serializes it, and this
/// always-compiled fold deserializes it - so the field names can never drift between emitter and
/// folder. The fold projects it into a [`KIND_CONCEPT`] super-node carrying the pass-computed
/// `label`. Unlike the community fold (which computes a label from the folded members), a concept's
/// label rides on the event because it comes from the INTENT layer (a document title), which the pass
/// reads once and pins here - so nothing waits on a model and a rebuild re-derives it byte-identically
/// from the recorded events.
#[derive(Serialize, Deserialize)]
pub struct ConceptDerived {
    /// The concept id: `concept/<resolution>/<n>`. Encodes the resolution grain, so distinct grains
    /// coexist as distinct concepts and the `fresh` reset scopes itself to one grain by the
    /// `concept/<resolution>/` prefix.
    pub concept: String,
    /// The concept's deterministic label: the title of its most-central document member (highest
    /// intent-degree, ties lexicographic), or its most-central member's label if it has no document
    /// member. Folded onto the concept node's `label` attr.
    #[serde(default)]
    pub label: String,
    /// The resolution grain this pass ran at (default 1.0). Folded onto the concept node's
    /// `resolution` attr so the `lens=concepts` view can select a grain.
    #[serde(default)]
    pub resolution: f64,
    /// The pass's content hash (a deterministic digest of the input intent layer + resolution).
    /// Folded onto the concept node's `hash` attr, so a consumer can tell which pass produced a
    /// grouping without re-running the derivation.
    #[serde(default)]
    pub hash: String,
    /// Set `true` on the FIRST event of a pass (mirrors [`CommunityAssigned::fresh`]). It marks the
    /// pass boundary: the fold SUPERSEDES (sets `valid_to` on, never deletes) every live `REALIZES`
    /// edge of THIS resolution grain and drops the grain's now-orphan concept nodes before folding
    /// this pass's concepts, so a re-run at a resolution REPLACES that grain's grouping rather than
    /// accreting - and leaves every OTHER resolution grain's memberships live. On the first-ever pass
    /// it supersedes nothing. Rides the event and defaults `false`, so a pre-field log folds as
    /// non-boundary.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fresh: bool,
}

/// The `ConceptRealized` payload (spec 54): one concept membership the offline intent-derivation pass
/// emits. The ONE serialization contract shared by both sides of the log - the pass constructs it,
/// this always-compiled fold deserializes it - so the field names can never drift. The fold projects
/// it into a live `<node> --REALIZES--> <concept>` edge at [`TIER_INFERRED`].
#[derive(Serialize, Deserialize)]
pub struct ConceptRealized {
    /// The member node id joining the concept: a design-doc / handbook-rule / spec / rationale node,
    /// or the code entity (`<file>::<name>`) / file the intent layer attaches to.
    pub node: String,
    /// The concept id this member realizes: `concept/<resolution>/<n>`.
    pub concept: String,
    /// The resolution grain this pass ran at (default 1.0), carried so a consumer reading the event
    /// stream can attribute the membership to its grain without joining to the concept node.
    #[serde(default)]
    pub resolution: f64,
    /// The pass's content hash, carried for provenance symmetry with [`ConceptDerived`].
    #[serde(default)]
    pub hash: String,
}

#[derive(Debug, thiserror::Error)]
#[error("graph: {0}")]
pub struct Error(pub String);

/// Projection is the context-graph read model. `apply` folds one event; `subgraph`
/// and `resolve` query it, returning only currently valid edges.
pub trait Projection: Send + Sync {
    /// Fold a single event into the graph, idempotently per global position.
    fn apply(&self, e: &Event) -> Result<(), Error>;

    /// Fold a whole batch of events into the graph in ONE transaction, idempotently per global
    /// position - the batched analogue of [`apply`](Projection::apply). The result is exactly what
    /// applying each event in order would produce; the only difference is transaction CADENCE.
    ///
    /// The default folds each event through [`apply`](Projection::apply) (one transaction per
    /// event), so an implementation with no cheaper batch path is unaffected and every `apply`
    /// still runs. The sqlite [`Projector`](crate::contextgraph::sqlite::Projector) OVERRIDES it to
    /// open ONE transaction for the whole batch: that is what lets an ingest sink fold a file's
    /// whole batch at the store's transaction cost of ONE commit instead of one per event (spec 49
    /// - the measured cold-build throughput was transaction-cadence bound, not parse-bound).
    fn apply_batch(&self, events: &[Event]) -> Result<(), Error> {
        for e in events {
            self.apply(e)?;
        }
        Ok(())
    }

    /// The connected subgraph reachable from any seed within depth hops,
    /// following only currently valid edges (the FEED arc / an agent's blast radius).
    fn subgraph(&self, seed: &[String], depth: i64) -> Result<Graph, Error>;

    /// A directed, depth-bounded traversal over the caller-attributed `CALLS` edges (spec 52),
    /// beside the undirected [`subgraph`](Projection::subgraph). `direction` picks callees
    /// ([`Direction::Down`] - the execution path) or callers ([`Direction::Up`] - the call sites);
    /// `depth` clamps the hop distance; `tier_floor` is the LOWEST edge confidence tier the walk
    /// follows - [`TIER_INFERRED`] (the default the route passes) excludes the unresolved
    /// [`TIER_AMBIGUOUS`] tier, and passing [`TIER_AMBIGUOUS`] opts it in. A cross-file hop that
    /// lands on a BARE placeholder node resolves by the shared name-suffix to its definition(s):
    /// a hop with EXACTLY ONE definition is auto-followed, a multi-definition hop becomes a marked
    /// frontier the walk does NOT descend (honest by construction). Reached nodes dedup into a
    /// layered DAG; a recursion edge is marked ([`CallEdge::back`]) rather than duplicated.
    ///
    /// The default returns an empty [`CallGraph`], so a projection with no directed-walk support (a
    /// test double, or a not-yet-overriding adapter) degrades to an empty view rather than erroring.
    /// The sqlite [`Projector`](crate::contextgraph::sqlite::Projector) OVERRIDES it with the real
    /// walk. Read-only over the projection; an empty graph or a seed with no calls yields an empty
    /// [`CallGraph`], never an error.
    fn calls(
        &self,
        _seed: &[String],
        _direction: Direction,
        _depth: i64,
        _tier_floor: &str,
    ) -> Result<CallGraph, Error> {
        Ok(CallGraph::default())
    }

    /// Map a mention to a canonical node id, falling back to a direct id match.
    fn resolve(&self, mention: &str) -> Result<Option<String>, Error>;

    /// Resolve `entity` by name/id in the CURRENT tree (spec 92, criterion 4's fix round): the
    /// `graph --show <entity>` / `rigger_graph`'s `show` selector lookup, put on the trait so a
    /// caller holding only `&dyn Projection` (`mcpserver::Server`, wired via
    /// [`with_graph`](crate::mcpserver::Server::with_graph) exactly like [`subgraph`] already is)
    /// can serve it without reaching for the concrete [`sqlite::Projector`] across the crate
    /// boundary - the DI extension that retired the operator MCP surface's second read loop
    /// rather than adding one. Mirrors [`sqlite::Projector::locate`]: a full `<file>::<name>` id
    /// resolves directly, a bare name matches by the pinned name-suffix expression, and the three
    /// outcomes ([`Located`], a port-owned type - see its doc) are never a guess among candidates.
    /// The default returns `Located::None`, so a projection with no locate support (a test double)
    /// degrades honestly rather than erroring - the sqlite `Projector` OVERRIDES it with the real
    /// lookup.
    fn locate(&self, _entity: &str) -> Result<Located, Error> {
        Ok(Located::None)
    }
}

/// spec 92, criterion 4's fix round: [`Projection::locate`]'s DEFAULT (the sqlite `Projector`
/// is the only implementor that overrides it) - proven against a minimal double that
/// implements only the trait's REQUIRED methods, so it inherits the default rather than
/// re-declaring it.
#[cfg(test)]
mod locate_default {
    use super::{Located, Projection};
    use crate::test_support::MinimalProjection;

    /// A projection with no locate support degrades HONESTLY to `Located::None` rather than
    /// erroring or panicking - the same honesty the [`crate::contextgraph::sqlite::Projector`]
    /// override promises for a genuinely unmatched query, just for every query on a projection
    /// that never indexed definitions at all.
    #[test]
    fn a_projection_with_no_override_reports_none_never_errors() {
        assert_eq!(MinimalProjection.locate("anything").unwrap(), Located::None);
    }
}
