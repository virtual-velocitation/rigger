//! The bi-temporal context graph: the read model projected from the event log
//! that answers relationship questions vector search cannot ("what decisions
//! govern this file? what lessons apply?"). `Projection` is the port; `sqlite` is
//! the adapter. A superseded edge is invalidated (its valid_to set), never
//! deleted, so retrieval returns the current decision and never the stale one.

// THE QUERY ENGINE MOVES WITH THE OPS (spec 93): this module (model + queries) is
// `core`; `sqlite` is the concrete projector adapter Design explicitly names as
// `store`-gated, so it is excluded from the `core` lane (same predicate as every other
// store-gated module - see `lib.rs`'s own doc).
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod sqlite;

// THE QUERY ENGINE MOVES WITH THE OPS (spec 93 criterion 5): the pure graph query functions
// (`neighborhood`, `card`, `path`, `clustered_overview`, `cluster_detail`, `communities`'s
// dispatch, `search`) and the `graph_load`/`graph_query` ops, relocated from `dash.rs` - `core`,
// like this module itself, never gated.
pub mod query;

use serde::{Deserialize, Serialize};

pub use rigger_domain::contextgraph::*;

#[derive(Deserialize)]
#[cfg_attr(all(feature = "core", not(feature = "store")), allow(dead_code))] // consumed only by contextgraph::sqlite's fold, gated out under core-only
struct DecisionMade {
    id: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    governs: Vec<String>,
    #[serde(default)]
    supersedes: String,
}
// De-noise (spec 43): the FileTouched / GateVerdict / UnitStarted fold DTOs are gone. Their fold
// arms project only harness machinery (agent/unit/gate nodes and TOUCHES/ASSIGNED_TO/BLOCKS/
// GATED_BY edges), which the graph no longer models - so those arms are now graph no-ops that
// deserialize nothing. The events themselves stay in the log, read by metrics and the run-tree.
#[derive(Deserialize)]
#[cfg_attr(all(feature = "core", not(feature = "store")), allow(dead_code))] // consumed only by contextgraph::sqlite's fold, gated out under core-only
struct UnitIntegrated {
    // The conductor emits UNIT_INTEGRATED with an `id` key (`{"id": <unit>, "commit": ...}`),
    // unlike UNIT_STARTED which redundantly carries both `id` and `unit`. Accept `id` as an
    // alias so this fold parses what production actually records; without it the fold fails to
    // deserialize every real event and its disposition-expiry effect is dead in production. Only
    // the unit id is read (to drive disposition-expiry); the commit is not projected (de-noise).
    #[serde(alias = "id")]
    unit: String,
}
#[derive(Deserialize)]
#[cfg_attr(all(feature = "core", not(feature = "store")), allow(dead_code))] // consumed only by contextgraph::sqlite's fold, gated out under core-only
struct AliasDefined {
    alias: String,
    canonical: String,
}
#[derive(Deserialize)]
#[cfg_attr(all(feature = "core", not(feature = "store")), allow(dead_code))] // consumed only by contextgraph::sqlite's fold, gated out under core-only
struct AliasUnresolved {
    mention: String,
}
#[derive(Deserialize)]
#[cfg_attr(all(feature = "core", not(feature = "store")), allow(dead_code))] // consumed only by contextgraph::sqlite's fold, gated out under core-only
struct LessonLearned {
    id: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    about: Vec<String>,
}
#[derive(Deserialize)]
#[cfg_attr(all(feature = "core", not(feature = "store")), allow(dead_code))] // consumed only by contextgraph::sqlite's fold, gated out under core-only
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
#[cfg_attr(all(feature = "core", not(feature = "store")), allow(dead_code))] // consumed only by contextgraph::sqlite's fold, gated out under core-only
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
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fresh: bool,
    /// Spec 92 criterion 2 round 4 (review REJECT `adj-u2c2-r3-verdict-reject`, finding
    /// `adv-u2c2-partial-marker-unimplemented`): mirrors
    /// [`crate::grounder::symbols::model::FileSymbols::partial`] - whether this file's parse hit a
    /// tree-sitter ERROR node, so its extracted structure only covers as far as the parse reached.
    /// The fold stamps this onto the `KIND_FILE` node's own `partial` attrs key (through the SAME
    /// `ensure_node` attrs authority it already uses for `lang`/`title`, never a second
    /// attrs-writing path) whenever ANY event of the file's batch carries it, so a degraded parse is
    /// visible on the node rather than silently presented as complete. Serde-defaulted and omitted
    /// when `false`, so the overwhelmingly common well-formed file's wire form is byte-identical to
    /// before this field existed.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub partial: bool,
}
/// The `EdgeInferred` payload (spec 29a): one reference the extraction pass emits. Shares the
/// same one-contract discipline as [`CodeEntityExtracted`]: emitted by the feature-gated pass,
/// folded by the always-compiled arm.
#[derive(Serialize, Deserialize)]
#[cfg_attr(all(feature = "core", not(feature = "store")), allow(dead_code))] // consumed only by contextgraph::sqlite's fold, gated out under core-only
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
    ///
    /// Spec 86 criterion 2 (round 2) double duty: on a TEST-ORIGIN event (`is_test`), this same
    /// flag instead marks the boundary of the referencing file's own EVIDENCE batch (stamped by
    /// [`crate::grounder::symbols::events::proof_events`], never `extract_events`), and the fold
    /// reads it as `supersede_file_proof`'s trigger rather than `supersede_file_edges`'s - the two
    /// concerns share the field because they share the same "first event of this file's re-emitted
    /// batch" shape, never because one is defined in terms of the other.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
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
    /// The reference's 1-based source line (spec 86 criterion 2). Unused by a STRUCTURAL reference
    /// (the REFERENCES/CALLS edges it folds carry no line today) so the production emit for an
    /// ordinary reference leaves this `0`; [`crate::grounder::symbols::events::proof_events`] is the
    /// one emitter that populates it for real, since a `proven_by` evidence entry needs the exact
    /// call site. Serde-defaulted and omitted when `0` (never a real 1-based line), so an ordinary
    /// reference's wire form stays byte-identical to before this criterion.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub line: u32,
    /// Spec 86 criterion 2: marks this event as TEST-ORIGIN EVIDENCE rather than a structural fact -
    /// emitted only by [`crate::grounder::symbols::events::proof_events`], never by the ordinary
    /// `extract_events` structural pass. The fold reads it FIRST in the `TYPE_EDGE_INFERRED` arm and,
    /// when set, never creates a `file` node or a `REFERENCES`/`CALLS` edge (criterion 1's "never a
    /// node and never an edge on the canvas" promise extends to evidence too) - it instead folds the
    /// reference onto the referenced entity's `proven_by` count / evidence list. Serde-defaulted and
    /// omitted when `false`, so an ordinary reference's wire form is unaffected.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_test: bool,
    /// The `EdgeInferred` twin of [`CodeEntityExtracted::partial`] - see that field's own doc. A
    /// refs-only file (no definitions) carries its parse-degraded marker here instead, mirroring how
    /// `fresh` already rides whichever event happens to be the file's batch boundary.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub partial: bool,
}

/// Serde `skip_serializing_if` predicate: an `EdgeInferred::line` of `0` is never a real 1-based
/// source line, so an ordinary (non-evidence) reference - which leaves `line` at its default -
/// serializes byte-identically to before this field existed.
#[cfg_attr(all(feature = "core", not(feature = "store")), allow(dead_code))] // consumed only by contextgraph::sqlite's fold, gated out under core-only
fn is_zero_u32(n: &u32) -> bool {
    *n == 0
}

/// The `DocConceptExtracted` payload: one entity a DEFINITION-extraction pass emits, either the
/// design-intent pass (spec 29b: a reference-architecture doc, an ADR, a handbook rule, or an
/// inline rationale comment) or the workflow-definition pass (spec 92 criterion 2: a
/// `.rigger/workflow.yml` stage, gate, or agent role). Both are the SAME shape - a document parsed
/// into typed, identified concepts - so they share this ONE serialization contract rather than a
/// second entity-extraction event type (spec 92's no-new-event-type constraint): the feature-gated
/// emit pass constructs and serializes it, and the always-compiled fold deserializes it, so the
/// field names can never drift between emitter and folder. The fold ingests it into a node whose
/// kind is `kind` (one of the seven `KIND_*` the two passes produce, below); a payload carrying any
/// other kind string folds nothing.
#[derive(Serialize, Deserialize)]
#[cfg_attr(all(feature = "core", not(feature = "store")), allow(dead_code))] // consumed only by contextgraph::sqlite's fold, gated out under core-only
pub(crate) struct DocConceptExtracted {
    /// The node kind: [`KIND_DESIGN_DOC`], [`KIND_ARCH_DECISION`], [`KIND_HANDBOOK_RULE`], or
    /// [`KIND_RATIONALE`] from the design-intent pass; [`KIND_STAGE`], [`KIND_GATE`], or
    /// [`KIND_AGENT`] from the workflow-definition pass. The two passes together only ever produce
    /// these seven.
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

/// The `DocLinkExtracted` payload: one typed link a DEFINITION-extraction pass emits, mirroring
/// [`DocConceptExtracted`]'s two producers - the design-intent pass (spec 29b) or the
/// workflow-definition pass (spec 92 criterion 2: a stage's `needs:` / `gates:` / `agent:` /
/// review roster, read straight off `.rigger/workflow.yml`). One serialization contract, never a
/// second edge-extraction event type: the feature-gated emit pass constructs and serializes it,
/// and the always-compiled fold deserializes it, so the field names can never drift between
/// emitter and folder. The fold folds it into a typed edge whose relation is `rel` (one of the
/// nine relations the two passes produce, below); a payload carrying any other relation folds
/// nothing (defensive).
#[derive(Serialize, Deserialize)]
#[cfg_attr(all(feature = "core", not(feature = "store")), allow(dead_code))] // consumed only by contextgraph::sqlite's fold, gated out under core-only
pub(crate) struct DocLinkExtracted {
    /// The link's source node id (the node the edge emanates from): a doc's relative path (a
    /// `design-doc` / `arch-decision` / `handbook-rule` whole-doc node), a `<file>#L<line>`
    /// rationale comment site, or a `stage:<name>` / `agent:<name>` workflow-definition node.
    pub from: String,
    /// The link's target node id (the node the edge points at): a code file / entity path (a
    /// `SPECIFIES` / `CONSTRAINS` / `GOVERNS` / `explains` target), a cited doc / code path (a
    /// `references` target), or a `stage:<name>` / `gate:<name>` / `agent:<name>`
    /// workflow-definition node.
    pub to: String,
    /// The relation: from the design-intent pass, one of [`REL_SPECIFIES`] / [`REL_CONSTRAINS`] /
    /// [`REL_GOVERNS`] / [`REL_EXPLAINS`] / [`REL_DOC_REFERENCES`]; from the workflow-definition
    /// pass, one of [`REL_NEEDS`] / [`REL_RUNS`] / [`REL_REVIEWS`] / [`REL_REVIEWS_LIGHT`]. The
    /// two passes together only ever produce these nine; a payload carrying any other relation
    /// folds nothing.
    pub rel: String,
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
            line: 0,
            is_test: false,
            partial: false,
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
            line: 0,
            is_test: false,
            partial: false,
        };
        let wire = String::from_utf8(serde_json::to_vec(&edge).unwrap()).unwrap();
        assert_eq!(
            wire, r#"{"file":"src/combat.rs","name":"std_thing","lang":"rust"}"#,
            "a caller-less reference at line 0 (unused by an ordinary structural reference) and \
             is_test false serializes byte-identically to the pre-37/pre-86 EdgeInferred form (no \
             caller/line/is_test key)"
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
        // Spec 86 criterion 2's two new fields are ALSO additive: a pre-86 log (no `line`/`is_test`
        // key either) folds as a non-evidence reference at line 0, never erroring or panicking.
        assert_eq!(edge.line, 0);
        assert!(!edge.is_test);
    }
}
