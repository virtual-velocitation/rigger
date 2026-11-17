//! The worktree adapter's error values (workspace split): plain data, which `rigger-worktree-git`
//! re-exports under its historical `worktree::Error` and `worktree::StepError` paths. The agent
//! interface's `Error` has no conversion from either: a worktree result reaches a conductor error
//! only through the conductor's one git step, which marks it as a halt.

#[derive(Debug, thiserror::Error)]
#[error("worktree: {0}")]
pub struct Error(pub String);

/// How a CONTENT step failed: a git step whose refusal the unit's own work can cause - its
/// checkpoint commit, the merge of the run branch into its worktree and the acceptance of the run
/// branch's side of a conflicted path, a plan-stage landing's cherry-pick. Every other step is a
/// LIFECYCLE step, whose failure is a plain [`Error`], which converts into
/// [`StepError::Unanswered`].
#[derive(Debug, thiserror::Error)]
pub enum StepError {
    /// Git ran and refused the step (an exit status from 1 to 127: a hook's refusal, a merge
    /// refused over local changes, any error git returns rather than dies on), or rigger's own
    /// guard refused it before git ran (conflict-marker text a commit will not stage, a
    /// cherry-pick an earlier attempt left in progress): the stage's failure, as the unit's.
    #[error("worktree: {0}")]
    Refused(String),
    /// Git did not answer: it could not start, a signal ended it, or it died (an exit status of
    /// 128 or more: a lock, a corrupt object, a missing identity).
    #[error(transparent)]
    Unanswered(#[from] Error),
}
