//! Rigger's operator-tree adapter, ring 3 of the workspace: the loaded harness configuration and
//! the readers of the operator's `.rigger/` tree (agent definitions, the workflow, the instruction
//! layer and the lightweight workflow probes). It knows the filesystem, `PATH` (through the gate
//! adapter's build-environment probes) and the domain; the root `rigger` crate re-exports every
//! module under its historical path.

pub mod config;
/// The write half of [`config`] (spec 93, criterion 1): see that module's own doc.
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod config_store;

// The modules the moved code names by their historical `crate::` paths.
use rigger_domain::instructions;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
use rigger_domain::{failure, spawn, test_cases};
#[cfg(any(feature = "store", not(feature = "core")))]
use rigger_gates_shell::gate;

/// The fixtures an inline `#[cfg(test)]` module shares with the root crate's tests, compiled
/// here from the same file. They name the crate as `rigger::...`, which
/// `extern crate self as rigger` makes resolve to this crate.
#[cfg(test)]
extern crate self as rigger;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/config.rs"]
mod config_fixtures;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
mod test_support {
    pub use crate::config_fixtures::*;
}
