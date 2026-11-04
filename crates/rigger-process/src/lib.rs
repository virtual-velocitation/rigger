//! Rigger's process adapters, ring 3 of the workspace: the one process-spawn port every child
//! is built through, the reaper that ends processes rooted inside a directory, the read-only
//! check for a process still holding a directory, the machine-wide
//! build-slot budget, the ordered parallel map and the keep-awake adapter behind the domain's
//! `SleepHold` port. They know `std::process`, the filesystem, threads and the host's OS
//! facilities; the root `rigger` crate re-exports every module under its historical path.

#[cfg(any(feature = "store", not(feature = "core")))]
pub mod budget;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod holders;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod keep_awake;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod parallel;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod reap;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod subprocess;

// The parameterised-test macro the moved tests name by its historical `crate::` path. Every
// module whose tests expand it is store-gated, so it is imported where those modules compile.
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
use rigger_domain::test_cases;

/// The fixtures an inline `#[cfg(test)]` module shares with the root crate's tests, compiled
/// here from the same file. They name the crate as `rigger::...`, which
/// `extern crate self as rigger` makes resolve to this crate, whose modules sit at the same
/// paths the root facade re-exports them under. Only the store-gated modules' tests use them, so
/// they compile where those modules do.
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
extern crate self as rigger;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/host.rs"]
mod host_fixtures;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
mod test_support {
    pub use crate::host_fixtures::*;
}
