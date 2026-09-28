//! The worktree adapter's error value (workspace split): plain data, held here so the agent
//! port's `Error` can convert from it; `rigger-worktree-git` re-exports it under its historical
//! `worktree::Error` path.

#[derive(Debug, thiserror::Error)]
#[error("worktree: {0}")]
pub struct Error(pub String);
