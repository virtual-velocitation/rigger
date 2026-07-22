//! The bi-temporal context graph: the read model projected from the event log
//! that answers relationship questions vector search cannot ("what decisions
//! govern this file? what lessons apply?"). `Projection` is the port; `sqlite` is
//! the adapter. A superseded edge is invalidated (its valid_to set), never
//! deleted, so retrieval returns the current decision and never the stale one.

pub mod sqlite;

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
#[derive(Clone, Debug)]
pub struct Node {
    pub id: String,
    pub kind: String,
    pub attrs: BTreeMap<String, String>,
}

/// A typed, bi-temporal edge. `valid_to == None` means it currently holds; a set
/// value means it was invalidated (superseded) and is never deleted.
#[derive(Clone, Debug)]
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
#[derive(Clone, Debug, Default)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
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

#[derive(Deserialize)]
struct DecisionMade {
    id: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    governs: Vec<String>,
    #[serde(default)]
    supersedes: String,
}
#[derive(Deserialize)]
struct FileTouched {
    path: String,
    #[serde(default)]
    by: String,
}
#[derive(Deserialize)]
struct GateVerdict {
    gate: String,
    #[serde(default)]
    pass: bool,
    #[serde(default)]
    artifact: String,
}
#[derive(Deserialize)]
struct UnitStarted {
    unit: String,
    #[serde(default)]
    criterion: String,
    #[serde(default)]
    agent: String,
    #[serde(default)]
    needs: Vec<String>,
}
#[derive(Deserialize)]
struct UnitIntegrated {
    // The conductor emits UNIT_INTEGRATED with an `id` key (`{"id": <unit>, "commit": ...}`),
    // unlike UNIT_STARTED which redundantly carries both `id` and `unit`. Accept `id` as an
    // alias so this fold parses what production actually records; without it the fold fails to
    // deserialize every real event and its disposition-expiry effect is dead in production.
    #[serde(alias = "id")]
    unit: String,
    #[serde(default)]
    commit: String,
}
#[derive(Deserialize)]
struct AliasDefined {
    alias: String,
    canonical: String,
}
#[derive(Deserialize)]
struct AliasUnresolved {
    mention: String,
}
#[derive(Deserialize)]
struct LessonLearned {
    id: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    about: Vec<String>,
}
#[derive(Deserialize)]
struct ReviewFinding {
    id: String,
    #[serde(default)]
    by: String,
    #[serde(default)]
    unit: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    about: Vec<String>,
}
/// The `CodeEntityExtracted` payload (spec 29a): one definition the extraction pass emits. It
/// is the ONE serialization contract shared by both sides of the log - the feature-gated emit
/// pass (`grounder::symbols`) constructs and serializes it, and the always-compiled fold
/// deserializes it - so the field names can never drift between emitter and folder.
#[derive(Serialize, Deserialize)]
pub(crate) struct CodeEntityExtracted {
    /// The definition's file, as a normalized relative path (the file container node id).
    pub file: String,
    /// The defined symbol's name.
    pub name: String,
    /// The rigger `Kind` of the definition, lowercased (e.g. `function`, `type`, `module`).
    pub kind: String,
    /// The 1-based line of the definition site.
    pub line: u32,
    /// The language the file was parsed as, lowercased (e.g. `rust`).
    #[serde(default)]
    pub lang: String,
    /// Set `true` on the FIRST event of a file's extraction batch (spec 29a criterion 3). It marks
    /// the batch boundary: the fold SUPERSEDES (sets `valid_to` on, never deletes) the file's prior
    /// live structural edges before folding this batch, so re-extracting a changed file REPLACES
    /// its structural edges rather than accreting duplicates. On the initial extraction it
    /// supersedes nothing (the file has no prior edges); on a later re-extraction it retires the
    /// previous pass. Rides the existing event - the batch boundary is a property of the extraction
    /// pass, not a fact meriting its own event type - and defaults `false`, so a historical event
    /// recorded before the field existed folds as a non-boundary event.
    #[serde(default, skip_serializing_if = "is_false")]
    pub fresh: bool,
}
/// The `EdgeInferred` payload (spec 29a): one reference the extraction pass emits. Shares the
/// same one-contract discipline as [`CodeEntityExtracted`]: emitted by the feature-gated pass,
/// folded by the always-compiled arm.
#[derive(Serialize, Deserialize)]
pub(crate) struct EdgeInferred {
    /// The referencing file, as a normalized relative path (the edge's `from` node id).
    pub file: String,
    /// The referenced symbol's name.
    pub name: String,
    /// The language the file was parsed as, lowercased (e.g. `rust`).
    #[serde(default)]
    pub lang: String,
    /// The extraction-batch boundary marker; see [`CodeEntityExtracted::fresh`]. A refs-only file
    /// (no definitions) carries it on its first reference instead, so every re-extracted file
    /// supersedes its prior edges regardless of whether it defines anything.
    #[serde(default, skip_serializing_if = "is_false")]
    pub fresh: bool,
    /// The enclosing definition this reference was attributed to during extraction (spec 37): the
    /// caller's name, same-file. `None` for a top-level reference outside every definition (an
    /// import or an `impl`-header bound). The emit pass carries what extraction attributed onto the
    /// `SymRef`; the fold, when it is present, adds a `<file>::<caller> --CALLS--> <callee>` edge
    /// ALONGSIDE the existing file-level `REFERENCES` edge (a later criterion owns that fold).
    /// Serde-defaulted and omitted when `None`, so a pre-37 log folds as caller-less and a
    /// caller-less reference serializes byte-identically to before - the CALLS edge is purely
    /// additive to the code layer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caller: Option<String>,
}

/// Serde `skip_serializing_if` predicate: keep the `fresh` boundary marker off the wire for the
/// common non-boundary event, so only the FIRST event of each extraction batch serializes it and
/// every other code event's payload is byte-identical to a pre-criterion-3 log.
fn is_false(b: &bool) -> bool {
    !*b
}

/// The `DocConceptExtracted` payload (spec 29b): one design-intent concept the extraction pass
/// emits. Like the 29a code payloads it is the ONE serialization contract shared by both sides of
/// the log - the feature-gated design-intent emit pass constructs and serializes it, and the
/// always-compiled fold deserializes it - so the field names can never drift between emitter and
/// folder. The fold ingests it into a node whose kind is `kind` (one of the four design-intent
/// `KIND_*` above); a payload carrying any other kind string folds nothing.
#[derive(Serialize, Deserialize)]
pub(crate) struct DocConceptExtracted {
    /// The node kind, one of [`KIND_DESIGN_DOC`], [`KIND_ARCH_DECISION`], [`KIND_HANDBOOK_RULE`],
    /// [`KIND_RATIONALE`]. The emit only ever produces these four.
    pub kind: String,
    /// The stable node id: a doc's relative path (a `design-doc` whole-doc node), `<doc>#<slug>`
    /// (a section node), the source path of an ingested decision / rule doc, or `<file>#L<line>`
    /// (a `rationale` comment site).
    pub id: String,
    /// The concept's human-readable title / summary (the doc heading, the decision title, the rule
    /// text, or the rationale comment). Folded onto the node's `title` attr.
    #[serde(default)]
    pub title: String,
    /// The source doc / file this concept was extracted from. Folded onto the node's `doc` attr,
    /// so a later criterion (the design-intent EDGES) can key its links off the concept's origin.
    #[serde(default)]
    pub doc: String,
}

/// The `DocLinkExtracted` payload (spec 29b): one design-intent link the extraction pass emits.
/// Like the 29a code payloads and [`DocConceptExtracted`] it is the ONE serialization contract
/// shared by both sides of the log - the feature-gated design-intent emit pass constructs and
/// serializes it, and the always-compiled fold deserializes it - so the field names can never
/// drift between emitter and folder. The fold folds it into a typed edge whose relation is `rel`
/// (one of the five design-intent relations: [`REL_SPECIFIES`], [`REL_CONSTRAINS`],
/// [`REL_GOVERNS`], [`REL_EXPLAINS`], [`REL_DOC_REFERENCES`]); a payload carrying any other
/// relation folds nothing (defensive - the emit only ever produces these five).
#[derive(Serialize, Deserialize)]
pub(crate) struct DocLinkExtracted {
    /// The link's source node id (the design-intent node the edge emanates from): a doc's relative
    /// path (a `design-doc` / `arch-decision` / `handbook-rule` whole-doc node) or a `<file>#L<line>`
    /// rationale comment site.
    pub from: String,
    /// The link's target node id (the code / doc node the edge points at): a code file / entity
    /// path (a `SPECIFIES` / `CONSTRAINS` / `GOVERNS` / `explains` target) or a cited doc / code
    /// path (a `references` target).
    pub to: String,
    /// The design-intent relation, one of the five [`REL_SPECIFIES`] / [`REL_CONSTRAINS`] /
    /// [`REL_GOVERNS`] / [`REL_EXPLAINS`] / [`REL_DOC_REFERENCES`]. The emit only ever produces
    /// these five; a payload carrying any other relation folds nothing.
    pub rel: String,
}

#[derive(Debug, thiserror::Error)]
#[error("graph: {0}")]
pub struct Error(pub String);

/// Projection is the context-graph read model. `apply` folds one event; `subgraph`
/// and `resolve` query it, returning only currently valid edges.
pub trait Projection: Send + Sync {
    /// Fold a single event into the graph, idempotently per global position.
    fn apply(&self, e: &Event) -> Result<(), Error>;

    /// The connected subgraph reachable from any seed within depth hops,
    /// following only currently valid edges (the FEED arc / an agent's blast radius).
    fn subgraph(&self, seed: &[String], depth: i64) -> Result<Graph, Error>;

    /// Map a mention to a canonical node id, falling back to a direct id match.
    fn resolve(&self, mention: &str) -> Result<Option<String>, Error>;
}

/// Periphery layer (spec 37 criterion 2): the round-trip + back-compat CONTRACT of the
/// [`EdgeInferred`] wire form now that it carries `caller`. This is the emit->log->fold seam,
/// not the emit pass itself: the feature-gated `extract_events` (its own inside-out unit tests
/// live in `grounder::symbols::events`) serializes the event, and the always-compiled fold in
/// [`sqlite`] reads it straight back with `serde_json::from_slice::<EdgeInferred>` (see the
/// `TYPE_EDGE_INFERRED` arm). These tests pin the serialized form the two sides share, so they
/// are NOT feature-gated and run in BOTH lanes exactly like the fold they guard. They guard what
/// the emit unit test is structurally blind to: the deserialize direction, the byte-identical
/// omission of `caller` when it is `None`, and a pre-37 log's tolerance.
#[cfg(test)]
mod caller_wire_contract {
    use super::EdgeInferred;

    /// The full wire round-trip: an attributed reference serializes its `caller` and the fold's
    /// exact read path (`from_slice::<EdgeInferred>`) recovers it. This is the emit->fold contract
    /// the implementer's raw-JSON emit test does not close - that test never deserializes back into
    /// an `EdgeInferred`, so nothing else proves the caller survives the deserialize direction the
    /// fold depends on.
    #[test]
    fn a_caller_carrying_reference_event_round_trips_through_the_fold_deserialize_path() {
        let edge = EdgeInferred {
            file: "src/combat.rs".to_string(),
            name: "G".to_string(),
            lang: "rust".to_string(),
            fresh: false,
            caller: Some("F".to_string()),
        };
        let wire = serde_json::to_vec(&edge).unwrap();
        let back: EdgeInferred = serde_json::from_slice(&wire).unwrap();
        assert_eq!(
            back.caller,
            Some("F".to_string()),
            "a caller-carrying reference preserves its caller across the emit->fold serde round-trip"
        );
    }

    /// A top-level reference (no enclosing definition) carries `caller: None`, and
    /// `skip_serializing_if = Option::is_none` keeps the key OFF the wire, so the event is
    /// byte-identical to the pre-37 `EdgeInferred` form - the CALLS layer is purely additive.
    /// The implementer's emit test only asserts `caller_of() == None`, which a stray `"caller":
    /// null` would also satisfy; this pins the key's ABSENCE by comparing the exact bytes.
    #[test]
    fn a_caller_less_reference_event_serializes_byte_identically_to_the_pre37_wire_form() {
        let edge = EdgeInferred {
            file: "src/combat.rs".to_string(),
            name: "std_thing".to_string(),
            lang: "rust".to_string(),
            fresh: false,
            caller: None,
        };
        let wire = String::from_utf8(serde_json::to_vec(&edge).unwrap()).unwrap();
        assert_eq!(
            wire, r#"{"file":"src/combat.rs","name":"std_thing","lang":"rust"}"#,
            "a caller-less reference serializes byte-identically to the pre-37 EdgeInferred form (no caller key)"
        );
    }

    /// The fold reads historical logs with `from_slice::<EdgeInferred>`. A pre-37 event has NO
    /// `caller` key; it must still deserialize (serde tolerates the absent optional field) and fold
    /// as caller-less, so replaying an old log never errors on the new field. Guards the
    /// back-compat the fold at `sqlite.rs`'s `TYPE_EDGE_INFERRED` arm silently relies on.
    #[test]
    fn a_pre37_reference_event_without_a_caller_key_still_deserializes_folding_caller_less() {
        let pre37 = br#"{"file":"src/combat.rs","name":"G","lang":"rust"}"#;
        let edge: EdgeInferred = serde_json::from_slice(pre37).unwrap();
        assert_eq!(
            edge.caller, None,
            "a pre-37 reference event (no caller key) folds as caller-less"
        );
        // The pre-existing fields still deserialize unchanged (the new optional field is additive).
        assert_eq!(edge.name, "G");
        assert!(!edge.fresh);
    }
}
