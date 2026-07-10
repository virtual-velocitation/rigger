//! Persist and load the symbol index (architecture 5.5). This module is a PROJECTION of the
//! parser-free `model::SymbolIndex` onto disk: it names no `tree_sitter::` type and touches no
//! grammar, so - exactly like `model` - it compiles in BOTH feature lanes and its determinism
//! tests run in both. tree-sitter stays confined to `extract`/`registry` behind the `symbols`
//! feature; nothing here needs it.
//!
//! Cross-process determinism (the hash-seed hazard) is guaranteed BY CONSTRUCTION, not by a
//! sort at write time: `SymbolIndex` is `BTreeMap`-backed, so serde emits its keys in sorted
//! order and re-serializing the same logical index yields byte-identical output in every
//! process. We never iterate a `HashMap`/`HashSet` on the serialized path, so the seed
//! randomization that differs across processes cannot reach the bytes on disk. The in-process
//! `save_load_roundtrips_and_bytes_are_stable` test pins the byte-stability locally; the
//! `tests/cli.rs` two-process test pins it across separate `rigger` processes (the check the
//! in-process test structurally cannot make, since one process shares its own hash seed).

use crate::grounder::symbols::model::SymbolIndex;
use std::path::{Path, PathBuf};

/// The on-disk index file, under the project's per-machine grounding state
/// (`<dir>/.rigger/symbols/index.json`). `dir` is the project root the index was built over,
/// the same root the `symbols` grounder opens - so the grounder's load and this save agree on
/// one location.
pub fn index_path(dir: &str) -> PathBuf {
    Path::new(dir)
        .join(".rigger")
        .join("symbols")
        .join("index.json")
}

/// The advisory-lock file guarding a write of [`index_path`]: a sibling of the index under the
/// same (gitignored) `.rigger/symbols/` dir, carrying no data - it exists only to be the `flock`
/// target that serializes concurrent [`save`]s across processes.
fn lock_path(dir: &str) -> PathBuf {
    Path::new(dir)
        .join(".rigger")
        .join("symbols")
        .join("index.lock")
}

/// A line-ending-normalized content hash: `"a\r\nb\r\n"` and `"a\nb\n"` hash identically, so the
/// SAME source keys the SAME cache entry whether it was checked out with CRLF (Windows) or LF
/// (Unix). Uses a fixed-seed FNV-1a - the SAME stable-content-hash discipline the semantic
/// grounder's `hash_content` deliberately chose over `DefaultHasher` (whose seed the stdlib does
/// NOT guarantee stable across builds) - so the value is a stable content key across processes,
/// builds, and machines. (A single crate-level hash primitive shared by every open-coded copy is
/// the broader cross-cutting refactor already recorded as `arch-u2i-fnv1a-fourth-parallel-copy`;
/// the semantic grounder's copy is feature-gated and private, so this ungated module cannot call
/// it and matches its algorithm instead.) It is the content-identity primitive the `symbols`
/// grounder's reindex freshening gate keys on to decide a named file is unchanged and skip
/// re-parsing it.
pub fn content_hash(src: &str) -> String {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let normalized = src.replace("\r\n", "\n");
    let mut hash = FNV_OFFSET;
    for byte in normalized.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    format!("{hash:016x}")
}

/// Acquire the cross-process EXCLUSIVE write lock guarding [`index_path`], run `write` under it,
/// then release. This is the ONE cross-process write-lock authority both [`save`] (publish a whole
/// snapshot) and [`reindex_under_lock`] (reload-modify-publish) go through, so there is a single
/// `flock` discipline over the index, not two parallel ones. The lock is an `fs2` exclusive
/// advisory lock - the project's one cross-process lock authority, non-optional in both feature
/// lanes, the same `acquire_step_lock` uses (`libc::flock` is confined to the turbovec feature, so
/// this ungated module cannot reach it). The lock file is created if absent; the lock releases when
/// `lock` drops (or the process dies), so a crashed writer never wedges the next one. A write error
/// is reported over an unlock error.
fn with_write_lock<F>(dir: &str, write: F) -> Result<(), String>
where
    F: FnOnce() -> Result<(), String>,
{
    use fs2::FileExt;
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path(dir))
        .map_err(|e| format!("symbols: open lock: {e}"))?;
    lock.lock_exclusive()
        .map_err(|e| format!("symbols: lock index: {e}"))?;
    let result = write();
    // Release explicitly for clarity (drop would too).
    let _ = FileExt::unlock(&lock);
    result
}

/// Serialize the index deterministically to [`index_path`], creating the parent dir. The bytes are
/// byte-stable across processes because `SymbolIndex` is `BTreeMap`-backed (see the module docs);
/// we never iterate a `HashMap` here.
///
/// The write is ATOMIC and cross-process SERIALIZED so a concurrent reader (a `Symbols` grounder
/// opening the index in another process) never observes a torn file. We hold the exclusive
/// [`with_write_lock`] across the write and publish the bytes by writing a sibling temp file,
/// fsync-ing it, and `rename`-ing it over the index: `rename(2)` within one directory is atomic, so
/// a reader sees either the whole old file or the whole new one.
///
/// [`save`] publishes the WHOLE snapshot it is handed (the cold-start build in [`Symbols::open`],
/// which has nothing to reload). A grounder freshening a long-lived in-memory index must instead go
/// through [`reindex_under_lock`], which reloads the persisted base under the SAME held lock before
/// writing, so a concurrent writer is folded in rather than clobbered.
///
/// [`Symbols::open`]: crate::grounder::symbols::grounder::Symbols::open
pub fn save(idx: &SymbolIndex, dir: &str) -> Result<(), String> {
    let path = index_path(dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("symbols: mkdir: {e}"))?;
    }
    // Serialize BEFORE touching disk so a serialization failure aborts with no partial artifact.
    let bytes = serde_json::to_vec_pretty(idx).map_err(|e| format!("symbols: serialize: {e}"))?;
    with_write_lock(dir, || write_atomic(&path, &bytes))
}

/// Freshen the persisted index under ONE continuously-held write lock, closing the cross-process
/// LOST-UPDATE window a bare load-once-then-[`save`] leaves open. Under the exclusive
/// [`with_write_lock`] it (1) RELOADS the persisted index into `base` if one is on disk - folding
/// in any write a concurrent `rigger` process made since this grounder loaded its in-memory copy;
/// (2) hands that reloaded base to `mutate` to apply the caller's per-file delta; (3) publishes the
/// result atomically - all WITHOUT releasing the lock between the reload and the write.
///
/// This mirrors turbovec's `reload_persisted_locked` read-modify-write discipline. A long-lived
/// grounder (held for a whole `rigger run`) whose in-memory index has gone stale because a separate
/// `rigger reindex` process wrote since can no longer clobber that write with its stale snapshot: it
/// reloads the peer's bytes first, then layers ONLY its own changed files on top, so both survive.
///
/// When NOTHING is persisted yet, or the on-disk file is unreadable/torn, `base` is kept as-is
/// (there is nothing safe to adopt; the publish below heals the store) - the same "keep the
/// in-memory base" fallback turbovec's reload takes. On return `base` holds the freshened,
/// persisted state (reloaded base + the caller's delta), so the grounder's next `ground` serves it.
pub fn reindex_under_lock<F>(base: &mut SymbolIndex, dir: &str, mutate: F) -> Result<(), String>
where
    F: FnOnce(&mut SymbolIndex),
{
    let path = index_path(dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("symbols: mkdir: {e}"))?;
    }
    with_write_lock(dir, || {
        // Reload the persisted base under the held lock so the caller's delta applies to the LATEST
        // on-disk state, folding in a concurrent writer rather than overwriting it. Keep the
        // in-memory base when nothing is persisted / it is unreadable (a cold or torn store) - the
        // publish below writes a consistent file from what we hold.
        if let Some(disk) = load(dir) {
            *base = disk;
        }
        mutate(base);
        // Serialize the reloaded+mutated base under the lock (turbovec serializes under its lock
        // too); a serialization failure aborts with no partial artifact written.
        let bytes =
            serde_json::to_vec_pretty(base).map_err(|e| format!("symbols: serialize: {e}"))?;
        write_atomic(&path, &bytes)
    })
}

/// Publish `bytes` at `path` atomically: write to a sibling temp file, fsync it, then `rename` it
/// over `path`. `rename(2)` within one directory is atomic, so a concurrent reader observes either
/// the whole old file or the whole new one, never the truncating write in progress. The temp is
/// per-pid so two writers' temps never collide (though [`save`]'s lock already serializes them),
/// and is cleaned up on a rename failure so the dir is not littered with a stale `.tmp`.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let dir = path
        .parent()
        .ok_or_else(|| format!("symbols: {} has no parent dir", path.display()))?;
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("symbols: {} has no file name", path.display()))?;
    let tmp = dir.join(format!(".{file_name}.{}.tmp", std::process::id()));
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp)
            .map_err(|e| format!("symbols: create temp {}: {e}", tmp.display()))?;
        f.write_all(bytes)
            .map_err(|e| format!("symbols: write temp {}: {e}", tmp.display()))?;
        // fsync so the bytes hit disk before the rename publishes the file; otherwise a crash right
        // after the rename could leave the new name pointing at empty data.
        f.sync_all()
            .map_err(|e| format!("symbols: fsync temp {}: {e}", tmp.display()))?;
    }
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!(
            "symbols: rename {} -> {}: {e}",
            tmp.display(),
            path.display()
        )
    })
}

/// Load the persisted index, or `None` when it is absent or unreadable (a cold start - the
/// caller builds + persists it). A corrupt/partial file also yields `None`, so a stale artifact
/// never crashes grounding; it is transparently rebuilt.
pub fn load(dir: &str) -> Option<SymbolIndex> {
    let bytes = std::fs::read(index_path(dir)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grounder::symbols::model::{Def, FileSymbols, Kind, Lang, SymbolIndex};

    fn sample() -> SymbolIndex {
        let mut idx = SymbolIndex::default();
        idx.insert_file(
            "z.rs".into(),
            FileSymbols {
                lang: Lang::Rust,
                defs: vec![
                    Def {
                        kind: Kind::Function,
                        name: "b".into(),
                        line: 2,
                    },
                    Def {
                        kind: Kind::Function,
                        name: "a".into(),
                        line: 1,
                    },
                ],
                refs: vec![],
            },
        );
        idx
    }

    #[test]
    fn save_load_roundtrips_and_bytes_are_stable() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        save(&sample(), root).unwrap();
        let loaded = load(root).unwrap();
        assert_eq!(loaded, sample());
        // Re-serializing the same logical index yields byte-identical output (BTreeMap order,
        // not HashMap): write twice, compare.
        let p = index_path(root);
        let first = std::fs::read(&p).unwrap();
        save(&sample(), root).unwrap();
        let second = std::fs::read(&p).unwrap();
        assert_eq!(first, second, "serialization must be byte-stable");
    }

    #[test]
    fn load_is_none_on_a_cold_start() {
        // No index persisted yet -> None (the grounder then builds + persists one), never a panic.
        let dir = tempfile::tempdir().unwrap();
        assert!(load(dir.path().to_str().unwrap()).is_none());
    }

    #[test]
    fn content_hash_is_line_ending_normalized() {
        assert_eq!(content_hash("a\r\nb\r\n"), content_hash("a\nb\n"));
    }

    #[test]
    fn content_hash_is_a_stable_fixed_seed_value() {
        // Pin the FNV-1a lowering so an accidental swap back to a non-guaranteed-stable hasher
        // (the freshening gate keys on this value ACROSS processes) is caught. Empty input is the
        // bare FNV offset basis; a known string pins the mixing.
        assert_eq!(content_hash(""), "cbf29ce484222325");
        assert_eq!(content_hash("a\nb\n"), content_hash("a\r\nb\r\n"));
        assert_ne!(content_hash("a"), content_hash("b"));
    }

    #[test]
    fn save_publishes_atomically_and_leaves_no_temp_residue() {
        // The atomic write must leave the index in place and NO `.tmp` sibling behind, so a reader
        // that lists the dir never trips over a half-written scratch file.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        save(&sample(), root).unwrap();
        assert!(load(root).unwrap() == sample());
        let symbols_dir = index_path(root).parent().unwrap().to_path_buf();
        let leftover: Vec<_> = std::fs::read_dir(&symbols_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".tmp"))
            .collect();
        assert!(
            leftover.is_empty(),
            "no .tmp residue expected, found {leftover:?}"
        );
        // A second save over the same dir still round-trips (the lock file is reused, not doubled).
        save(&sample(), root).unwrap();
        assert!(load(root).unwrap() == sample());
    }
}
