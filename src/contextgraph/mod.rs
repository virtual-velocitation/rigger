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

// Edge relationships.
pub const REL_SUPERSEDES: &str = "SUPERSEDES";
pub const REL_TOUCHES: &str = "TOUCHES";
pub const REL_GOVERNS: &str = "GOVERNS";
pub const REL_GATED_BY: &str = "GATED_BY";
pub const REL_ABOUT: &str = "ABOUT";

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
pub const TYPE_UNIT_INTEGRATED: &str = "UnitIntegrated";
pub const TYPE_LESSON_LEARNED: &str = "LessonLearned";

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
struct UnitIntegrated {
    unit: String,
    #[serde(default)]
    commit: String,
}
#[derive(Deserialize)]
struct LessonLearned {
    id: String,
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
