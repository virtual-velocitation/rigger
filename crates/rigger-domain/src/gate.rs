//! The gate vocabulary the adapters share: names both the gate runner and the worktree
//! reclaimer resolve through, so the two can never spell them differently.

/// The scratch-dir naming suffix [`ExecRunner::run`] appends to `target_dir` to derive the
/// store fence's own sibling scratch dir (`{target_dir}{STORE_FENCE_SUFFIX}`). Shared with
/// `worktree::reclaim_cache_sibling` (the ONE reclaim authority for a unit's
/// `cargo-target-<slug>` cache sibling) so it reclaims the fence's sibling by the exact
/// same name it was created under, rather than a second, independently-spelled copy that
/// could drift from this one.
pub const STORE_FENCE_SUFFIX: &str = "-store-fence";
