//! Rigger's gate adapter, ring 3 of the workspace: the shell runner that executes a gate
//! command under the resolved build environment, the build slot and the store fence, and
//! reduces its output to compact evidence. It knows `std::process`, the filesystem, `PATH` and
//! the domain's gate model; the root `rigger` crate re-exports it under its historical
//! `rigger::gate` path.

#[cfg(any(feature = "store", not(feature = "core")))]
pub mod gate;

// The modules the moved code names by their historical `crate::` paths.
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
use rigger_domain::test_cases;
#[cfg(any(feature = "store", not(feature = "core")))]
use rigger_process::{budget, subprocess};
#[cfg(any(feature = "store", not(feature = "core")))]
use rigger_store_sqlite::registry;
// The reaper the shared host fixtures name as `rigger::reap`.
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
use rigger_process::reap;

/// The fixtures an inline `#[cfg(test)]` module shares with the root crate's tests, compiled
/// here from the same file. They name the crate as `rigger::...`, which
/// `extern crate self as rigger` makes resolve to this crate.
#[cfg(test)]
extern crate self as rigger;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/host.rs"]
mod host_fixtures;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
mod test_support {
    pub use crate::host_fixtures::*;
}
