//! Rigger's application ring, ring 2 of the workspace: the conductor, the top-level use case
//! that walks the stage DAG, runs each stage's agent through the `AgentDriver` port and its gates
//! through the gate runner, advances units under the safety rails and emits the event stream the
//! ledger and the context graph project from. It still names several adapters directly (each
//! edge recorded in tests/boundary_audit.rs EDGE_ALLOWLIST); the root `rigger` crate re-exports
//! it under its historical `rigger::conductor` path.

/// Spec 16 unit 2 - the partitioning + routing SAFETY EVAL (architecture 5.5.8). A GATE, not a
/// runtime surface: it is compiled ONLY under `cfg(test)`, adds no API and no event, and its
/// quantified arms are feature-gated behind `symbols` internally. It authorizes unit 3 wiring
/// `blast_radius` into the conductor by proving the safe view is a grep superset and that the
/// safe-superset partitioning retains parallelism and a non-collapsed tier split.
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
mod blast_radius_eval;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod canary_store;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod conductor;
#[cfg(any(feature = "store", not(feature = "core")))]
mod replay_keys;

// The modules the moved code names by their historical `crate::` paths.
#[cfg(any(feature = "store", not(feature = "core")))]
use rigger_config_files::config;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
use rigger_config_files::config_store;
#[cfg(any(feature = "store", not(feature = "core")))]
use rigger_domain::{
    blocker, canary, failure, instructions, ledger, metrics, playbooks, run, safety,
};
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
use rigger_domain::{spec, test_cases};
#[cfg(any(feature = "store", not(feature = "core")))]
use rigger_driver::{driver, liveness};
#[cfg(any(feature = "store", not(feature = "core")))]
use rigger_gates_shell::gate;
#[cfg(any(feature = "store", not(feature = "core")))]
use rigger_grounder::{grounder, ingest};
#[cfg(any(feature = "store", not(feature = "core")))]
use rigger_process::{budget, parallel};
#[cfg(any(feature = "store", not(feature = "core")))]
use rigger_store_sqlite::run_store;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
use rigger_store_sqlite::spawn_store;
#[cfg(any(feature = "store", not(feature = "core")))]
use rigger_worktree_git::worktree;
#[cfg(any(feature = "store", not(feature = "core")))]
mod contextgraph {
    pub use rigger_domain::contextgraph::*;
    /// The SQLite projector the tests project the run's events through.
    #[cfg(test)]
    pub use rigger_graph_sqlite::contextgraph::sqlite;
}
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

/// The fixtures an inline `#[cfg(test)]` module shares with the root crate's tests, compiled
/// here from the same files. They name the crate as `rigger::...`, which
/// `extern crate self as rigger` makes resolve to this crate, whose modules sit at the same
/// paths the root facade re-exports them under.
#[cfg(test)]
extern crate self as rigger;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/canary.rs"]
mod canary_fixtures;
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
#[path = "../../../tests/common/fixtures/git.rs"]
mod git_fixtures;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/ingest.rs"]
mod ingest_fixtures;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/spawn.rs"]
mod spawn_fixtures;
// The config fixtures the conductor fixtures name through `super::`.
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
use config_fixtures::{agent, gate_def, gate_def_inputs};
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
mod test_support {
    pub use crate::canary_fixtures::*;
    pub use crate::conductor_fixtures::*;
    pub use crate::config_fixtures::*;
    pub use crate::event_fixtures::*;
    pub use crate::git_fixtures::*;
    pub use crate::ingest_fixtures::*;
    pub use crate::spawn_fixtures::*;
}
