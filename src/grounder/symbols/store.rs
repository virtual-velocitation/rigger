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

/// A line-ending-normalized content hash: `"a\r\nb\r\n"` and `"a\nb\n"` hash identically, so
/// the SAME source keys the SAME cache entry whether it was checked out with CRLF (Windows) or
/// LF (Unix). Deterministic across processes: `DefaultHasher::new()` seeds from fixed keys (it
/// is the per-process randomization of `HashMap`'s `RandomState`, not of `DefaultHasher`
/// itself, that varies), so this is a stable content key. It is the content-identity primitive
/// the incremental freshening path keys on to decide a file is unchanged.
pub fn content_hash(src: &str) -> String {
    use std::hash::{Hash, Hasher};
    let normalized = src.replace("\r\n", "\n");
    let mut h = std::collections::hash_map::DefaultHasher::new();
    normalized.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// Serialize the index deterministically to [`index_path`], creating the parent dir. Byte-stable
/// across processes because `SymbolIndex` is `BTreeMap`-backed (see the module docs); we never
/// iterate a `HashMap` here.
pub fn save(idx: &SymbolIndex, dir: &str) -> Result<(), String> {
    let path = index_path(dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("symbols: mkdir: {e}"))?;
    }
    let bytes = serde_json::to_vec_pretty(idx).map_err(|e| format!("symbols: serialize: {e}"))?;
    std::fs::write(&path, bytes).map_err(|e| format!("symbols: write {}: {e}", path.display()))
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
}
