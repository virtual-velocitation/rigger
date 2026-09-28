//! Rigger's git worktree adapter, ring 3 of the workspace: the throwaway per-unit worktree,
//! its landing onto the run branch and every reclaim of the scratch it leaves. It knows git,
//! the filesystem, the process adapters and the domain; the root `rigger` crate re-exports it
//! under its historical `rigger::worktree` path.

#[cfg(any(feature = "store", not(feature = "core")))]
pub mod worktree;

// The modules the moved code names by their historical `crate::` paths.
#[cfg(any(feature = "store", not(feature = "core")))]
use rigger_domain::{eventstore, gate, spawn};
#[cfg(any(feature = "store", not(feature = "core")))]
use rigger_process::{reap, subprocess};
