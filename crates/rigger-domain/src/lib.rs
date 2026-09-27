//! Rigger's domain: the entities and use-case rules, ring 1 of the workspace. Nothing here
//! touches a file, a process, the network, the clock, a store or the agent host; the root
//! `rigger` crate re-exports every module under its historical path.

pub mod config;
pub mod failure;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod playbooks;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod review;
pub mod safety;
pub mod spawn;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod spec;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod wave;

/// Parameterised tests: one shared case helper, one generated `#[test]` per named case.
mod test_cases;
