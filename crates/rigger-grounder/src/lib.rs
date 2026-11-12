//! Rigger's grounding adapters, ring 3 of the workspace: the grounders behind the `Grounder`
//! interface (the structural `symbols` index, grep and nop), the one scoped project walk they
//! share, the extraction of a file's bytes into the facts the code store holds, and the combined
//! lookup. They know the filesystem, tree-sitter and the domain; the root `rigger` crate
//! re-exports them under their historical `rigger::grounder` and `rigger::ingest` paths.

/// Extraction to facts: a file's path and bytes read into the facts the code store holds, over the
/// symbol, design and workflow extractors.
#[cfg(all(feature = "symbols", any(feature = "store", not(feature = "core"))))]
pub mod facts;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod grounder;
/// The folding store: an event store whose every append also folds the appended events into the
/// records half of the graph.
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod ingest;
/// The combined lookup: one call answering a list of terms from the code graph, a live text scan
/// of the caller's working tree and the tree's ranked prose, in one bounded reply.
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod lookup;

// The modules the moved code names by their historical `crate::` paths.
#[cfg(all(feature = "symbols", any(feature = "store", not(feature = "core"))))]
use rigger_config_files::config_store;
#[cfg(all(feature = "symbols", any(feature = "store", not(feature = "core"))))]
use rigger_domain::config;
#[cfg(any(feature = "store", not(feature = "core")))]
use rigger_domain::eventstore;
#[cfg(any(feature = "store", not(feature = "core")))]
mod contextgraph {
    pub use rigger_domain::contextgraph::*;
}
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
use rigger_domain::test_cases;

/// The shared fixtures name the crate as `rigger::...`, which `extern crate self as rigger` makes
/// resolve to this crate.
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
extern crate self as rigger;

/// The graph fixtures the sink-check tests take the read half of their projection double from,
/// compiled here from the same file as the root crate's tests.
// The composed view the shared graph fixtures read a record half through.
#[cfg(all(
    test,
    feature = "symbols",
    any(feature = "store", not(feature = "core"))
))]
use rigger_domain::{code_facts, code_view, composed};
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
