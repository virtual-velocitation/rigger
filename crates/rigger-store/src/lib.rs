//! rigger's owned stores: the binary [`format`] every store file is written in, the [`code`]
//! store, which holds one immutable snapshot of the tree's facts per stamp, and the one scoped
//! [`walk`] of a project's files.
//!
//! The crate depends only on `rigger-domain`, std and the `ignore` gitignore engine. It holds no language knowledge: extraction
//! and grouping are passed in through the domain's `Extract` and `Group` interfaces.

#[cfg(any(feature = "store", not(feature = "core")))]
pub mod code;
pub mod format;
pub mod sync_dir;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod walk;
