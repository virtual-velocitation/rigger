//! Rigger's domain: the entities and use-case rules, ring 1 of the workspace. Nothing here
//! touches a file, a process, the network, the clock, a store or the agent host; the root
//! `rigger` crate re-exports every module under its historical path.

pub mod blocker;
/// Deterministic coupling-community detection (spec 53, the CODE lens): the offline pass that
/// groups code entities and files by how densely they call and reference one another, regardless of
/// directory, and records the result as `CommunityAssigned` events the always-compiled fold turns
/// into `IN_COMMUNITY` membership edges. Always compiled and proven in both feature lanes.
pub mod community;
pub mod concepts;
pub mod config;
pub mod contextgraph;
pub mod eventstore;
pub mod failure;
/// The ingest fold rules (spec 45): the replay-key vocabulary and the project-scoped suppression
/// predicate the live run and the standalone `rigger graph build` both seed from.
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod ingest;
pub mod ledger;
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
pub mod wave;

/// Parameterised tests: one shared case helper, one generated `#[test]` per named case.
mod test_cases;

/// The fixtures an inline `#[cfg(test)]` module shares with the root crate's tests: the event and
/// graph fixtures of `tests/common/fixtures/`, compiled here from the same files. They name the
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
#[cfg(test)]
mod test_support {
    pub use crate::event_fixtures::*;
    pub use crate::graph_fixtures::*;
}
