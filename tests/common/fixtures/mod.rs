//! Test fixtures shared by the integration suites AND the crate's own inline `#[cfg(test)]`
//! modules - the one definition of every fixture both sides need.
//!
//! This tree is compiled into three kinds of test crate: each integration suite (through
//! `tests/common/mod.rs`), the library's unit tests and the binary's unit tests (each through a
//! `#[cfg(test)] #[path = ...] mod test_support;`). It therefore names the library only as
//! `rigger::...`, which resolves in all three (the library aliases itself with
//! `extern crate self as rigger`). A submodule that builds `store`-gated types carries the same
//! gate as those types, so every feature lane compiles exactly the fixtures it can use.
//! Everything is re-exported flat, so a caller never spells the submodule path.

#![allow(dead_code, unused_imports)]

mod caller_sources;
mod config;
mod csharp_definition_sources;
mod events;
mod extent_sources;
mod extraction_tree;
mod git;
mod go_definition_sources;
mod graph;
mod host;
mod js_ts_definition_sources;
mod language_markers;
mod name_filter_sources;
mod page;
mod python_definition_sources;
mod read_fault;
mod rust_definition_sources;
mod scratch;
mod spawn;
pub use caller_sources::*;
pub use config::*;
pub use csharp_definition_sources::*;
pub use events::*;
pub use extent_sources::*;
pub use extraction_tree::*;
pub use git::*;
pub use go_definition_sources::*;
pub use graph::*;
pub use host::*;
pub use js_ts_definition_sources::*;
pub use language_markers::*;
pub use name_filter_sources::*;
pub use page::*;
pub use python_definition_sources::*;
pub use read_fault::*;
pub use rust_definition_sources::*;
pub use scratch::*;
pub use spawn::*;

#[cfg(any(feature = "store", not(feature = "core")))]
mod canary;
#[cfg(any(feature = "store", not(feature = "core")))]
mod conductor;
#[cfg(any(feature = "store", not(feature = "core")))]
mod fold;
#[cfg(any(feature = "store", not(feature = "core")))]
mod ingest;
#[cfg(any(feature = "store", not(feature = "core")))]
mod kurrentdb;
#[cfg(any(feature = "store", not(feature = "core")))]
mod ledger;
#[cfg(any(feature = "store", not(feature = "core")))]
mod plan_critique;
#[cfg(any(feature = "store", not(feature = "core")))]
mod sqlite;
#[cfg(any(feature = "store", not(feature = "core")))]
mod store;
#[cfg(any(feature = "store", not(feature = "core")))]
pub use canary::*;
#[cfg(any(feature = "store", not(feature = "core")))]
pub use conductor::*;
#[cfg(any(feature = "store", not(feature = "core")))]
pub use fold::*;
#[cfg(any(feature = "store", not(feature = "core")))]
pub use ingest::*;
#[cfg(any(feature = "store", not(feature = "core")))]
pub use kurrentdb::*;
#[cfg(any(feature = "store", not(feature = "core")))]
pub use ledger::*;
#[cfg(any(feature = "store", not(feature = "core")))]
pub use plan_critique::*;
#[cfg(any(feature = "store", not(feature = "core")))]
pub use sqlite::*;
#[cfg(any(feature = "store", not(feature = "core")))]
pub use store::*;
