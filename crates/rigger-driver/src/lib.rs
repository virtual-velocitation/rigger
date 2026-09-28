//! Rigger's agent-host adapters, ring 3 of the workspace: the drivers behind the `AgentDriver`
//! port (the headless Claude Code host, the CLI, the Workflow and the replay driver), the
//! per-spawn liveness markers, the peers side-car, the installed Claude Code hooks and the
//! supervised child guard. They know `std::process`, the filesystem, the stores and the domain;
//! the root `rigger` crate re-exports every module under its historical path.

#[cfg(any(feature = "store", not(feature = "core")))]
pub mod driver;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod hooks;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod liveness;
/// The supervised child-process guard: a dropped guard kills and reaps its child.
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod reaped_child;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod sidecar;

// The modules the moved code names by their historical `crate::` paths.
#[cfg(all(not(test), any(feature = "store", not(feature = "core"))))]
use rigger_domain::config;
#[cfg(any(feature = "store", not(feature = "core")))]
use rigger_domain::contextgraph;
#[cfg(any(feature = "store", not(feature = "core")))]
use rigger_domain::{agent, failure, progress, run};
#[cfg(any(feature = "store", not(feature = "core")))]
use rigger_process::{reap, subprocess};
#[cfg(any(feature = "store", not(feature = "core")))]
use rigger_store_sqlite::{progress_store, spawn_store};
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
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
use rigger_domain::{ledger, test_cases};
// The root-crate vocabulary the tests and the shared fixtures name (see the manifest's
// `rigger-root` dev-dependency).
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
use rigger_root::{conductor, config, gate};
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
use rigger_store_sqlite::run_store;

/// The fixtures an inline `#[cfg(test)]` module shares with the root crate's tests, compiled
/// here from the same files. They name the crate as `rigger::...`, which
/// `extern crate self as rigger` makes resolve to this crate, whose modules sit at the same
/// paths the root facade re-exports them under.
#[cfg(test)]
extern crate self as rigger;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/conductor.rs"]
mod conductor_fixtures;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/config.rs"]
mod config_fixtures;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/events.rs"]
mod event_fixtures;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/spawn.rs"]
mod spawn_fixtures;
// The config fixtures the conductor fixtures name through `super::`.
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
use config_fixtures::{agent, gate_def, gate_def_inputs};
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
mod test_support {
    pub use crate::conductor_fixtures::*;
    pub use crate::event_fixtures::*;
    pub use crate::spawn_fixtures::*;
}
