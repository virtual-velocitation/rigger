//! Live-writer fixtures: the reasons the live-writer probe names when a command refuses beside a
//! live run.

/// The reason the live-writer probe names while a `rigger step` holds `.rigger/step.lock`.
pub const STEP_LOCK_REASON: &str =
    "a `rigger step` is running right now (it holds .rigger/step.lock)";
