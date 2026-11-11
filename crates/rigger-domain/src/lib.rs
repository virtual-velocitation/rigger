//! Rigger's domain: the entities and use-case rules, ring 1 of the workspace. Nothing here
//! touches a file, a process, the network, the clock, a store or the agent host; the root
//! `rigger` crate re-exports the modules it names under their historical path.

/// The agent-host port: the `AgentDriver` trait, its options, result and error, and the park and
/// failure-class sentinels that cross it.
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod agent;
pub mod blocker;
pub mod canary;
pub mod code_facts;
pub mod code_graph;
/// Deterministic coupling-community detection (spec 53, the CODE lens): the offline pass that
/// groups code entities and files by how densely they call and reference one another, regardless of
/// directory, and records the result as `CommunityAssigned` events the always-compiled fold turns
/// into `IN_COMMUNITY` membership edges. Always compiled and proven in both feature lanes.
pub mod community;
pub mod composed;
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
/// The gate request interface: how `rigger gate` asks the run's driver to gate a unit's commit.
pub mod gate_channel;
/// The grounding port: the `Grounder` trait and the plain values it returns.
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod grounder;
pub mod groupings;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod hold_probe;
/// The ingest fold rules (spec 45): the replay-key vocabulary and the project-scoped suppression
/// predicate the live run and the standalone `rigger graph build` both seed from.
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod ingest;
/// Instruction injection: the built-in engineering law, working discipline and communication
/// discipline, each unless a same-named operator file replaces or switches it off, and every
/// other `.rigger/instructions/*.md` file, layered into every spawned agent's system prompt; a
/// same-named file is never layered itself.
pub mod instructions;
pub mod ledger;
/// The combined lookup interface: the `Lookup` trait one call answers graph, text and prose through.
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod lookup;
pub mod metrics;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod playbooks;
pub mod progress;
pub mod quoting;
pub mod resolve;
/// The classes of event (spec 107): the perception and episodic type lists and the ledger entry
/// of perception. Under `ingest`'s gate, whose derived index it cites.
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod retention;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod review;
pub mod run;
pub mod safety;
/// The keep-awake port (spec 105): the hold a run's driver takes so the machine does not sleep.
pub mod sleep_hold;
pub mod spawn;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod spec;
pub mod test_view;
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
/// graph, critique and spawn fixtures of `tests/common/fixtures/`, compiled here from the same files. They name the
/// crate as `rigger::...`, which `extern crate self as rigger` makes resolve to this crate, whose
/// modules sit at the same paths the root facade re-exports them under.
#[cfg(test)]
extern crate self as rigger;
/// The spawn fixtures name the domain's own types as `rigger_domain::...`, the dependency's name
/// in every other crate that compiles them, which this alias makes resolve to this crate too.
#[cfg(test)]
extern crate self as rigger_domain;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/critique.rs"]
mod critique_fixtures;
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
#[path = "../../../tests/common/fixtures/hold_probe.rs"]
mod hold_probe_fixtures;
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/ingest.rs"]
mod ingest_fixtures;
#[cfg(test)]
#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/instructions.rs"]
mod instructions_fixtures;
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
    pub use crate::instructions_fixtures::*;
    pub use crate::spawn_fixtures::*;
}
