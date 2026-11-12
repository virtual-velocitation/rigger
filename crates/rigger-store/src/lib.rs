//! rigger's owned stores: the binary [`format`] every store file is written in, the [`code`]
//! store, which holds one immutable snapshot of the tree's facts per stamp, the [`published`]
//! read of a store file and the one scoped [`walk`] of a project's files.
//!
//! The crate depends only on `rigger-domain`, std, the `ignore` gitignore engine and `memmap2`,
//! which maps a published file on the OSes that map. It holds no language knowledge: extraction
//! and grouping are passed in through the domain's `Extract` and `Group` interfaces.

#[cfg(any(feature = "store", not(feature = "core")))]
pub mod code;
pub mod format;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod published;
pub mod sync_dir;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod walk;
