//! Rigger's context-graph persistence adapter, ring 3 of the workspace: the SQLite projector
//! behind the `Projection` port. It knows rusqlite, the filesystem and the domain; the root
//! `rigger` crate re-exports it under its historical `rigger::contextgraph::sqlite` path.

#[cfg(test)]
mod concepts;
pub mod contextgraph;

// The modules the moved code names by their historical `crate::` paths.
#[cfg(any(test, feature = "store", not(feature = "core")))]
use rigger_domain::eventstore;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
use rigger_domain::ingest;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
use rigger_domain::retention;
#[cfg(any(feature = "store", not(feature = "core")))]
use rigger_domain::spawn;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
use rigger_domain::test_cases;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
use rigger_root::{conductor, metrics};
#[cfg(any(feature = "store", not(feature = "core")))]
use rigger_store_sqlite::{lockfile, sqlite};

/// The fixtures an inline `#[cfg(test)]` module shares with the root crate's tests, compiled
/// here from the same files. They name the crate as `rigger::...`, which
/// `extern crate self as rigger` makes resolve to this crate, whose modules sit at the same
/// paths the root facade re-exports them under.
#[cfg(test)]
extern crate self as rigger;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/cursor.rs"]
mod cursor_fixtures;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/fold.rs"]
mod fold_fixtures;
// The scratch fixture the fold fixtures name through `super::`, compiled here from the same
// file as the root crate's tests.
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
#[path = "../../../tests/common/fixtures/scratch.rs"]
mod scratch_fixtures;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
use scratch_fixtures::temp_root;
#[cfg(test)]
#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/events.rs"]
mod event_fixtures;
#[cfg(test)]
#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/graph.rs"]
mod graph_fixtures;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/host.rs"]
mod host_fixtures;
// The reaper and the open-files reader the shared host fixtures name as `rigger::reap` and
// `rigger::holders`.
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
use rigger_process::{holders, reap};
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/passed_over.rs"]
mod passed_over_fixtures;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/write_lock.rs"]
mod write_lock_fixtures;
#[cfg(test)]
mod test_support {
    #[cfg(any(feature = "store", not(feature = "core")))]
    pub use crate::cursor_fixtures::*;
    #[cfg(any(feature = "store", not(feature = "core")))]
    pub use crate::event_fixtures::*;
    #[cfg(any(feature = "store", not(feature = "core")))]
    pub use crate::fold_fixtures::*;
    pub use crate::graph_fixtures::*;
    #[cfg(any(feature = "store", not(feature = "core")))]
    pub use crate::host_fixtures::*;
    #[cfg(any(feature = "store", not(feature = "core")))]
    pub use crate::passed_over_fixtures::*;
    #[cfg(any(feature = "store", not(feature = "core")))]
    pub use crate::write_lock_fixtures::*;
}
