//! Rigger's process adapters, ring 3 of the workspace: the one process-spawn port every child
//! is built through, the reaper that ends processes rooted inside a directory, the machine-wide
//! build-slot budget and the ordered parallel map. They know `std::process`, the filesystem and
//! threads; the root `rigger` crate re-exports every module under its historical path.

#[cfg(any(feature = "store", not(feature = "core")))]
pub mod budget;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod parallel;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod reap;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod subprocess;

// The parameterised-test macro the moved tests name by its historical `crate::` path.
#[cfg(test)]
use rigger_domain::test_cases;

/// The fixtures an inline `#[cfg(test)]` module shares with the root crate's tests, compiled
/// here from the same file. They name the crate as `rigger::...`, which
/// `extern crate self as rigger` makes resolve to this crate, whose modules sit at the same
/// paths the root facade re-exports them under.
#[cfg(test)]
extern crate self as rigger;
#[cfg(test)]
#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/host.rs"]
mod host_fixtures;
#[cfg(test)]
mod test_support {
    pub use crate::host_fixtures::*;
}
