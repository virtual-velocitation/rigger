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
/// generation a batch is recorded under can never drift between the two ingest entries.
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
#[cfg(all(
    test,
    feature = "symbols",
    any(feature = "store", not(feature = "core"))
))]
use rigger_domain::retention;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
use rigger_domain::test_cases;

/// The fold fixtures the inline fold tests share with the root crate's tests, compiled here from
/// the same file. It names the crate as `rigger::...`, which `extern crate self as rigger` makes
/// resolve to this crate.
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
extern crate self as rigger;
#[cfg(all(
    test,
    feature = "symbols",
    any(feature = "store", not(feature = "core"))
))]
#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/fold.rs"]
mod test_support;

/// The event fixtures the fold tests stamp a batch's log positions with, compiled here from the
/// same file as the root crate's tests.
#[cfg(all(
    test,
    feature = "symbols",
    any(feature = "store", not(feature = "core"))
))]
#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/events.rs"]
mod event_fixtures;

/// The graph fixtures the sink-check tests take the read half of their projection double from,
/// compiled here from the same file as the root crate's tests.
#[cfg(all(
    test,
    feature = "symbols",
    any(feature = "store", not(feature = "core"))
))]
#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/graph.rs"]
mod graph_fixtures;

/// THE CALLER SOURCES the caller-rule tests extract, compiled here from the same file as the root
/// crate's tests. It names no crate; only the default lane's tests extract them.
#[cfg(all(
    test,
    feature = "symbols",
    any(feature = "store", not(feature = "core"))
))]
#[path = "../../../tests/common/fixtures/caller_sources.rs"]
mod caller_sources;

/// THE NAME FILTER SOURCES the name-filter tests extract, compiled here from the same file as the
/// root crate's tests. It names no crate; only the default lane's tests extract them.
#[cfg(all(
    test,
    feature = "symbols",
    any(feature = "store", not(feature = "core"))
))]
#[path = "../../../tests/common/fixtures/name_filter_sources.rs"]
mod name_filter_sources;

/// THE RUST DEFINITIONS FIXTURE the Rust definition tests extract, and the enum `E` sources the
/// CONTAINS IS STRUCTURE tests and the twin-method sources the ONE PATH IS ONE EVENT tests extract,
/// compiled here from the same file as the root crate's tests. It names no crate; only the
/// default lane's tests extract it.
#[cfg(all(
    test,
    feature = "symbols",
    any(feature = "store", not(feature = "core"))
))]
#[path = "../../../tests/common/fixtures/rust_definition_sources.rs"]
mod rust_definition_sources;

/// THE GO DEFINITIONS FIXTURE the Go definition tests extract, compiled here from the file the
/// shared fixtures live in. It names no crate; only the default lane's tests extract it.
#[cfg(all(
    test,
    feature = "symbols",
    any(feature = "store", not(feature = "core"))
))]
#[path = "../../../tests/common/fixtures/go_definition_sources.rs"]
mod go_definition_sources;

/// THE C# DEFINITIONS FIXTURE the C# definition tests extract, compiled here from the file the
/// shared fixtures live in. It names no crate; only the default lane's tests extract it.
#[cfg(all(
    test,
    feature = "symbols",
    any(feature = "store", not(feature = "core"))
))]
#[path = "../../../tests/common/fixtures/csharp_definition_sources.rs"]
mod csharp_definition_sources;

/// THE JS AND TS DEFINITIONS FIXTURES the JS and TS definition tests extract, and the namespace
/// source the ONE PATH IS ONE EVENT tests extract, compiled here from the file the shared fixtures
/// live in. They name no crate; only the default lane's tests extract them.
#[cfg(all(
    test,
    feature = "symbols",
    any(feature = "store", not(feature = "core"))
))]
#[path = "../../../tests/common/fixtures/js_ts_definition_sources.rs"]
mod js_ts_definition_sources;

/// THE PYTHON DEFINITIONS FIXTURE the Python definition tests extract, compiled here from the
/// file the shared fixtures live in. It names no crate; only the default lane's tests extract it.
#[cfg(all(
    test,
    feature = "symbols",
    any(feature = "store", not(feature = "core"))
))]
#[path = "../../../tests/common/fixtures/python_definition_sources.rs"]
mod python_definition_sources;

/// THE EXTENT SOURCES the extent tests locate in, compiled here from the file the shared fixtures
/// live in. They name no crate; only the default lane's tests extract them.
#[cfg(all(
    test,
    feature = "symbols",
    any(feature = "store", not(feature = "core"))
))]
#[path = "../../../tests/common/fixtures/extent_sources.rs"]
mod extent_sources;

/// The symbol index fixtures the extractor's and the registry's tests share with the root crate's
/// tests (the stamped `index.json` writer, a source read through the registry, the definition
/// and reference projections), compiled here from the same file. It names the crate as
/// `rigger::...`; only the default lane's tests read it.
#[cfg(all(
    test,
    feature = "symbols",
    any(feature = "store", not(feature = "core"))
))]
#[path = "../../../tests/common/symbol_index.rs"]
mod symbol_index;

/// THE READ FAULT fixture the tree's read-rule tests share with the root crate's tests, compiled
/// here from the same file. It names no crate, so both lanes compile it.
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
#[path = "../../../tests/common/fixtures/read_fault.rs"]
mod read_fault_fixtures;

/// THE EXTRACTION TREE the bytes-form tests extract, compiled here from the same file as the root
/// crate's tests. It names no workspace crate and needs `tempfile` from this crate's
/// dev-dependencies; only the default lane's tests plant it.
#[cfg(all(
    test,
    feature = "symbols",
    any(feature = "store", not(feature = "core"))
))]
#[allow(dead_code)]
#[path = "../../../tests/common/fixtures/extraction_tree.rs"]
mod extraction_tree;

/// The host fixtures the tree's read-rule tests write their files through, compiled here from the
/// same file as the root crate's tests. It names the reaper and the open-files reader as
/// `rigger::reap` and `rigger::holders`, which the import below makes resolve in this crate.
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/host.rs"]
mod host_fixtures;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
use rigger_process::{holders, reap};
