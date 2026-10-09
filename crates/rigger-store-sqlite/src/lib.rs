//! Rigger's persistence adapters, ring 3 of the workspace: the event-store backends behind the
//! `EventStore` port, the one SQLite opener every store connection goes through, the write
//! halves of the run, spawn and progress vocabularies, and the machine-global instance
//! registry. They know rusqlite, the filesystem and the domain; the root `rigger` crate
//! re-exports every module under its historical path.

pub mod eventstore;
/// The one advisory lock-file guard every rigger lock is held through: see that module's own doc.
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod lockfile;
/// The write half of `progress` (spec 93, criterion 1): see that module's own doc.
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod progress_store;
/// Machine-global instance registry (spec 50): credential-free discovery metadata so a single
/// machine-level dash can find every local project's runs (and any configured shared store)
/// without a coordination protocol. Discovery only - never a source of truth, never a credential.
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod registry;
/// The write half of `run` (spec 93, criterion 1): see that module's own doc.
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod run_store;
/// The write half of `spawn` (spec 93, criterion 1): see that module's own doc.
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod spawn_store;
/// The one opener every SQLite store connection goes through: see that module's own doc.
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod sqlite;

// The domain modules the moved code names by their historical `crate::` paths.
#[cfg(any(feature = "store", not(feature = "core")))]
use rigger_domain::{contextgraph, ingest, progress, retention, run};
#[cfg(any(feature = "store", not(feature = "core")))]
mod spawn {
    /// The minimal request the spawn store's tests build, defined once with the shared fixtures.
    #[cfg(all(test, any(feature = "store", not(feature = "core"))))]
    pub(crate) use crate::test_support::test_request;
    pub use rigger_domain::spawn::*;
}
#[cfg(test)]
use rigger_domain::test_cases;

/// The fixtures an inline `#[cfg(test)]` module shares with the root crate's tests, compiled
/// here from the same files. They name the crate as `rigger::...`, which
/// `extern crate self as rigger` makes resolve to this crate, whose modules sit at the same
/// paths the root facade re-exports them under. Only the store-gated modules' tests use them, so
/// they compile where those modules do.
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
extern crate self as rigger;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/events.rs"]
mod event_fixtures;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/kurrentdb.rs"]
mod kurrentdb_fixtures;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/spawn.rs"]
mod spawn_fixtures;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/sqlite.rs"]
mod sqlite_fixtures;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
mod test_support {
    pub use crate::event_fixtures::*;
    pub use crate::kurrentdb_fixtures::*;
    pub use crate::spawn_fixtures::*;
    pub use crate::sqlite_fixtures::*;
}
