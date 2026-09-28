//! Rigger's grounding adapters, ring 3 of the workspace: the grounders behind the `Grounder`
//! port (the structural `symbols` index, grep and nop), the one scoped project walk they share
//! and the code and design-intent emit passes. They know the filesystem, tree-sitter and the
//! domain; the root `rigger` crate re-exports them under their historical `rigger::grounder`
//! path.

#[cfg(any(feature = "store", not(feature = "core")))]
pub mod grounder;

// The modules the moved code names by their historical `crate::` paths.
#[cfg(any(feature = "store", not(feature = "core")))]
use rigger_domain::config;
#[cfg(all(feature = "symbols", any(feature = "store", not(feature = "core"))))]
use rigger_domain::eventstore;
#[cfg(all(feature = "symbols", any(feature = "store", not(feature = "core"))))]
use rigger_process::parallel;
#[cfg(all(feature = "symbols", any(feature = "store", not(feature = "core"))))]
mod contextgraph {
    pub use rigger_domain::contextgraph::*;
    /// The SQLite projector the fold tests project the emitted events through.
    #[cfg(test)]
    pub use rigger_graph_sqlite::contextgraph::sqlite;
}
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
use rigger_domain::test_cases;
