//! rigger's owned stores: the binary [`format`] every store file is written in, and the
//! [`code`] store, which holds one immutable snapshot of the tree's facts per stamp.
//!
//! The crate depends only on `rigger-domain` and std. It holds no language knowledge: extraction
//! and grouping are passed in through the domain's `Extract` and `Group` interfaces.

#[cfg(any(feature = "store", not(feature = "core")))]
pub mod code;
pub mod format;
pub mod sync_dir;
