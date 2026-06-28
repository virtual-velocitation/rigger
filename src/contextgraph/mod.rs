//! The bi-temporal context graph: the read model projected from the event log
//! that answers relationship questions vector search cannot ("what decisions
//! govern this file? what lessons apply?"). `Projection` is the port; `sqlite` is
//! the adapter. A superseded edge is invalidated (its valid_to set), never
//! deleted, so retrieval returns the current decision and never the stale one.

pub mod sqlite;

use std::collections::BTreeMap;

use serde::Deserialize;

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
