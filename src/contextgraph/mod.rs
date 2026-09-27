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
