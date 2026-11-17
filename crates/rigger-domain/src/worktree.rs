//! The worktree adapter's error value (workspace split): plain data, which `rigger-worktree-git`
//! re-exports under its historical `worktree::Error` path. The agent interface's `Error` has no
//! conversion from it: a worktree result reaches a conductor error only through the conductor's
//! one git step, which marks it as a halt.

#[derive(Debug, thiserror::Error)]
#[error("worktree: {0}")]
pub struct Error(pub String);
