//! Rigger's git worktree adapter, ring 3 of the workspace: the throwaway per-unit worktree,
//! its landing onto the run branch, the scratch root it lives under and every reclaim of the
//! scratch it leaves. It knows git, the filesystem, the process adapters, the agent host's
//! cache-home and liveness-marker encodings and the domain; the root `rigger` crate re-exports it
//! under its historical `rigger::worktree` path.

#[cfg(any(feature = "store", not(feature = "core")))]
pub mod worktree;

// The modules the moved code names by their historical `crate::` paths.
#[cfg(all(not(test), any(feature = "store", not(feature = "core"))))]
use rigger_domain::{config, eventstore, gate};
#[cfg(any(feature = "store", not(feature = "core")))]
mod spawn {
    /// The minimal request the tests build, defined once with the shared fixtures.
    #[cfg(test)]
    pub(crate) use crate::test_support::test_request;
    pub use rigger_domain::spawn::*;
}
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
use rigger_domain::{ledger, run, test_cases};
#[cfg(any(feature = "store", not(feature = "core")))]
use rigger_driver::{driver, liveness};
#[cfg(any(feature = "store", not(feature = "core")))]
use rigger_process::{reap, subprocess};
// The root-crate vocabulary the tests and the shared fixtures name (see the manifest's
// `rigger-root` dev-dependency).
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
use rigger_root::{conductor, config, eventstore, gate, ingest, registry};

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
#[path = "../../../tests/common/fixtures/git.rs"]
mod git_fixtures;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/host.rs"]
mod host_fixtures;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/spawn.rs"]
mod spawn_fixtures;
// `store.rs` gates one fixture on the root's `symbols` feature, which this crate does not carry.
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
#[allow(dead_code, unused_imports, unexpected_cfgs)]
#[path = "../../../tests/common/fixtures/store.rs"]
mod store_fixtures;
// The config fixtures the conductor fixtures name, and the git fixtures the store fixtures name,
// through `super::`.
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
use config_fixtures::{agent, gate_def, gate_def_inputs};
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
use git_fixtures::{install_refusing_hook, temp_git_project_with_commit};
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
mod test_support {
    pub use crate::conductor_fixtures::*;
    pub use crate::git_fixtures::*;
    pub use crate::host_fixtures::*;
    pub use crate::spawn_fixtures::*;
    pub use crate::store_fixtures::*;
}
