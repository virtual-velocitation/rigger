//! The code store: one immutable snapshot of the tree's facts at `.rigger/store/code/`, named by
//! a `CURRENT` pointer and advanced under a `LOCK`.
mod snapshot;
pub use snapshot::{encode, Snapshot};
