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

mod config;
mod events;
mod git;
mod graph;
mod host;
mod page;
pub use config::*;
pub use events::*;
pub use git::*;
pub use graph::*;
pub use host::*;
pub use page::*;

#[cfg(any(feature = "store", not(feature = "core")))]
mod canary;
#[cfg(any(feature = "store", not(feature = "core")))]
mod conductor;
#[cfg(any(feature = "store", not(feature = "core")))]
mod fold;
#[cfg(any(feature = "store", not(feature = "core")))]
mod store;
#[cfg(any(feature = "store", not(feature = "core")))]
pub use canary::*;
#[cfg(any(feature = "store", not(feature = "core")))]
pub use conductor::*;
#[cfg(any(feature = "store", not(feature = "core")))]
pub use fold::*;
#[cfg(any(feature = "store", not(feature = "core")))]
pub use store::*;
