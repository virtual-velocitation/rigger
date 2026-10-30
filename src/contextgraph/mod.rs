//! The bi-temporal context graph: the read model projected from the event log
//! that answers relationship questions vector search cannot ("what decisions
//! govern this file? what lessons apply?"). `Projection` is the port; `sqlite` is
//! the adapter. A superseded edge is invalidated (its valid_to set), never
//! deleted, so retrieval returns the current decision and never the stale one.
//!
//! The model, the port and the query engine live in `rigger-domain`; the sqlite projector lives
//! in `rigger-graph-sqlite`. This module re-exports both under the historical
//! `rigger::contextgraph` path.

pub use rigger_domain::contextgraph::*;

// `sqlite` is the concrete projector adapter Design explicitly names as `store`-gated, so it is
// excluded from the `core` lane (same predicate as every other store-gated module - see
// `lib.rs`'s own doc).
#[cfg(any(feature = "store", not(feature = "core")))]
pub use rigger_graph_sqlite::contextgraph::sqlite;

/// The one emit guard, for an emit surface outside the MCP emit path: `rigger critique`'s finding
/// copies (spec 112) and the conductor's in-process emit, which the composition root hands the
/// guard through `Deps::check_emit`.
#[cfg(any(feature = "store", not(feature = "core")))]
pub use rigger_graph_sqlite::contextgraph::check_emit;

/// The fold's own payload judge, for a reader outside the adapter that spells the text a judged
/// payload carries: the root tests' passed-over fixtures, which build a fold pass's expected
/// cause from it.
#[cfg(any(feature = "store", not(feature = "core")))]
pub use rigger_graph_sqlite::contextgraph::check_fold_payload;
