//! Rigger's grounding adapters, ring 3 of the workspace: the grounders behind the `Grounder`
//! port (the structural `symbols` index, grep and nop), the one scoped project walk they share,
//! the code, design-intent and workflow-definition emit passes and the project-source ingest
//! walk that keys them into the context graph. They know the filesystem, tree-sitter and the
//! domain; the root `rigger` crate re-exports them under their historical `rigger::grounder` and
//! `rigger::ingest` paths.

#[cfg(any(feature = "store", not(feature = "core")))]
pub mod grounder;
/// Project-source ingest into the context graph (spec 45): the ONE walk-and-content-key
/// authority both the live run and the standalone `rigger graph build` entry share, so the
/// content key an event is deduped under can never drift between the two ingest entries.
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod ingest;

// The modules the moved code names by their historical `crate::` paths.
#[cfg(all(feature = "symbols", any(feature = "store", not(feature = "core"))))]
use rigger_config_files::config_store;
#[cfg(any(feature = "store", not(feature = "core")))]
use rigger_domain::config;
#[cfg(any(feature = "store", not(feature = "core")))]
use rigger_domain::eventstore;
#[cfg(all(feature = "symbols", any(feature = "store", not(feature = "core"))))]
use rigger_process::parallel;
#[cfg(any(feature = "store", not(feature = "core")))]
mod contextgraph {
    pub use rigger_domain::contextgraph::*;
    /// The SQLite projector the fold tests project the emitted events through.
    #[cfg(all(test, feature = "symbols"))]
    pub use rigger_graph_sqlite::contextgraph::sqlite;
}
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
use rigger_domain::test_cases;
