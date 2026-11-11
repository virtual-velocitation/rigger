//! The code store: one immutable snapshot of the tree's facts at `.rigger/store/code/`, named by

mod dir_source;
mod snapshot;

pub use dir_source::{content_id, diff, tree_id, DirTreeSource, DIRECTORY_HEAD};
pub use snapshot::{encode, Snapshot};
