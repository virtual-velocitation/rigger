//! Rigger's delivery ring, ring 4 of the workspace: the HTTP dashboard (the run view, the
//! knowledge-graph explorer and the Mission Control console page, with the console core's
//! WebAssembly module the build script embeds) and the MCP server the agents talk to. It reaches
//! the conductor's use cases and the adapters it reads through; the root `rigger` crate
//! re-exports both modules under their historical `rigger::dash` and `rigger::mcpserver` paths.

#[cfg(any(feature = "store", not(feature = "core")))]
pub mod dash;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod mcpserver;

// The modules the moved code names by their historical `crate::` paths.
#[cfg(any(feature = "store", not(feature = "core")))]
use rigger_conductor::conductor;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
use rigger_config_files::config;
#[cfg(any(feature = "store", not(feature = "core")))]
use rigger_console::console;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
use rigger_domain::test_cases;
#[cfg(any(feature = "store", not(feature = "core")))]
use rigger_domain::{blocker, ledger, metrics, progress, run};
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
use rigger_domain::{ingest, retention};
#[cfg(any(feature = "store", not(feature = "core")))]
use rigger_driver::{driver, liveness, sidecar};
#[cfg(any(feature = "store", not(feature = "core")))]
use rigger_grounder::grounder;
#[cfg(any(feature = "store", not(feature = "core")))]
use rigger_process::reap;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
use rigger_store_sqlite::run_store;
#[cfg(any(feature = "store", not(feature = "core")))]
use rigger_store_sqlite::{progress_store, registry};
#[cfg(any(feature = "store", not(feature = "core")))]
mod contextgraph {
    pub use rigger_domain::contextgraph::*;
    /// The SQLite projector the tests fold events through.
    #[cfg(test)]
    pub use rigger_graph_sqlite::contextgraph::sqlite;
}
#[cfg(any(feature = "store", not(feature = "core")))]
mod eventstore {
    /// The append-only double the seam tests share, defined once with the fixtures.
    #[cfg(test)]
    pub(crate) use crate::test_support::SilentStore;
    pub use rigger_store_sqlite::eventstore::*;
}
#[cfg(any(feature = "store", not(feature = "core")))]
mod spawn {
    /// The minimal request the tests build, defined once with the shared fixtures.
    #[cfg(test)]
    pub(crate) use crate::test_support::test_request;
    pub use rigger_domain::spawn::*;
}

/// The fixtures an inline `#[cfg(test)]` module shares with the root crate's tests, compiled
/// here from the same files. They name the crate as `rigger::...`, which
/// `extern crate self as rigger` makes resolve to this crate, whose modules sit at the same
/// paths the root facade re-exports them under.
#[cfg(test)]
extern crate self as rigger;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/events.rs"]
mod event_fixtures;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/fold.rs"]
mod fold_fixtures;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/graph.rs"]
mod graph_fixtures;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/page.rs"]
mod page_fixtures;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/spawn.rs"]
mod spawn_fixtures;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
mod test_support {
    pub use crate::event_fixtures::*;
    pub use crate::fold_fixtures::*;
    pub use crate::graph_fixtures::*;
    pub use crate::page_fixtures::*;
    pub use crate::spawn_fixtures::*;
}
