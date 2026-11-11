//! The code store: one immutable snapshot of the tree's facts at `.rigger/store/code/`, named by
//! a `CURRENT` pointer and advanced under a `LOCK`.

mod advance;
mod dir_source;
#[cfg(any(test, feature = "doubles"))]
pub mod doubles;
mod snapshot;

pub use advance::{open_current, AtomicPublish, Behind, Mode, Opened, Publish, Store};
pub use dir_source::{content_id, tree_id, DirTreeSource, DIRECTORY_HEAD};
pub use snapshot::{encode, Snapshot};
