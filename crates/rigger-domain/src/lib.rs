//! Rigger's domain: the entities and use-case rules, ring 1 of the workspace. Nothing here
//! touches a file, a process, the network, the clock, a store or the agent host; the root
//! `rigger` crate re-exports every module under its historical path.

/// The agent-host port: the `AgentDriver` trait, its options, result and error, and the park and
/// failure-class sentinels that cross it.
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod agent;
pub mod blocker;
pub mod canary;
/// Deterministic coupling-community detection (spec 53, the CODE lens): the offline pass that
/// groups code entities and files by how densely they call and reference one another, regardless of
/// directory, and records the result as `CommunityAssigned` events the always-compiled fold turns
/// into `IN_COMMUNITY` membership edges. Always compiled and proven in both feature lanes.
pub mod community;
pub mod concepts;
pub mod config;
pub mod contextgraph;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod docs;
pub mod eventstore;
pub mod failure;
/// The gate vocabulary the gate runner and the worktree reclaimer share.
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod gate;
/// The grounding port: the `Grounder` trait and the plain values it returns.
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod grounder;
/// The ingest fold rules (spec 45): the replay-key vocabulary and the project-scoped suppression
/// predicate the live run and the standalone `rigger graph build` both seed from.
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod ingest;
/// Instruction injection: the built-in engineering law and working discipline and the operator's
/// `.rigger/instructions/*.md` layered into every spawned agent's system prompt.
pub mod instructions;
pub mod ledger;
pub mod metrics;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod playbooks;
pub mod progress;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod review;
pub mod run;
pub mod safety;
pub mod spawn;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod spec;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod watch;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod wave;
/// The worktree adapter's error value the agent port's `Error` converts from.
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod worktree;

/// Parameterised tests: one shared case helper, one generated `#[test]` per named case.
mod test_cases;

/// The fixtures an inline `#[cfg(test)]` module shares with the root crate's tests: the event,
/// graph and spawn fixtures of `tests/common/fixtures/`, compiled here from the same files. They name the
/// crate as `rigger::...`, which `extern crate self as rigger` makes resolve to this crate, whose
/// modules sit at the same paths the root facade re-exports them under.
#[cfg(test)]
extern crate self as rigger;
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
#[path = "../../../tests/common/fixtures/ingest.rs"]
mod ingest_fixtures;
#[cfg(test)]
#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/spawn.rs"]
mod spawn_fixtures;
#[cfg(test)]
mod test_support {
    pub use crate::event_fixtures::*;
    pub use crate::graph_fixtures::*;
    #[cfg(any(feature = "store", not(feature = "core")))]
    pub use crate::ingest_fixtures::*;
    pub use crate::spawn_fixtures::*;
}
