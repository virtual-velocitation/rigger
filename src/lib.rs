//! Rigger: a config-driven, event-sourced multi-agent dev-loop harness.
//!
//! Clean Architecture: the ports are traits (EventStore, ...), the adapters live
//! beside them, and the conductor is the top-level use case depending only on
//! ports. This is the Rust port of the proven Go design.

pub mod config;
pub mod contextgraph;
pub mod eventstore;
pub mod gate;
pub mod ledger;
pub mod safety;
