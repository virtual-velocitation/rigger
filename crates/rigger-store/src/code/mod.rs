//! The code store: an immutable snapshot of the tree's facts at `.rigger/store/code/` for each of
//! the two most recently advanced fingerprints, each named by its `CURRENT-<fingerprint>` pointer
//! and advanced under a `LOCK`.

mod advance;
mod dir_source;
mod disk_memo;
#[cfg(any(test, feature = "doubles"))]
pub mod doubles;
mod overlay;
mod snapshot;

pub use advance::{
    fingerprint, open_compared, open_current, AtomicPublish, Behind, Mode, Opened, Publish, Store,
};
pub use dir_source::{content_id, tree_id, DirTreeSource, DIRECTORY_HEAD};
pub use disk_memo::MemoCap;
pub use overlay::{open_head, Memo, Overlay};
pub use snapshot::{encode, Snapshot};
