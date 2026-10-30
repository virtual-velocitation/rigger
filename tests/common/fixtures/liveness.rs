//! Live-writer fixtures: the reasons the live-writer probe names when a command refuses beside a
//! live run, and the refusal of a store whose live runs it cannot judge.

/// The reason the live-writer probe names while a `rigger step` holds `.rigger/step.lock`.
pub const STEP_LOCK_REASON: &str =
    "a `rigger step` is running right now (it holds .rigger/step.lock)";

/// What a command refuses the store at `store` with when its owning root has no UTF-8 path, so the
/// scratch root the live-writer probe reads its spawns' markers under resolves to nothing - naming
/// no command, since `rigger reset`'s modes and `rigger setup`'s new rebuild both reach it.
pub fn unresolved_scratch_root_refusal(store: &std::path::Path) -> String {
    format!(
        "the store at {} has no UTF-8 owning root, so the scratch root its runs write their \
         liveness markers and build caches under cannot be resolved; refusing rather than reading \
         that as no live spawn",
        store.display()
    )
}
