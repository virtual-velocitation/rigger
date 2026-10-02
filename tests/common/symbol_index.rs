//! Shared fixtures for the persisted symbol index's wire-format suites: building a one-file
//! index, saving it through the real `store` boundary and reading the bytes back, and loading a
//! hand-authored legacy `index.json`. Included by each suite through `#[path]`; a suite uses the
//! subset it needs (hence the module-wide `dead_code` allowance, the same convention
//! `tests/common/mod.rs` keeps). Parser-free, so it compiles in every feature lane.

#![allow(dead_code)]

use rigger::grounder::symbols::model::{Def, FileSymbols, Kind, Lang, SymRef, SymbolIndex};
use rigger::grounder::symbols::store;

/// A line-1 definition of `kind` named `name` with every optional field unset - the shape a
/// product-only definition persists as; a caller sets the one field its case is about with
/// struct-update syntax.
pub fn def(kind: Kind, name: &str) -> Def {
    Def {
        kind,
        name: name.into(),
        line: 1,
        is_test: false,
        is_out_of_line_module: false,
        path_override: None,
        enclosing_inline_module_path: None,
    }
}

/// A reference to `name` at `line`, attributed to `enclosing` when it has a caller.
pub fn sym_ref(name: &str, line: u32, enclosing: Option<&str>, is_test: bool) -> SymRef {
    SymRef {
        name: name.into(),
        line,
        enclosing: enclosing.map(Into::into),
        is_test,
    }
}

/// A fully-parsed Rust file holding `defs` and `refs`.
pub fn rust_file(defs: Vec<Def>, refs: Vec<SymRef>) -> FileSymbols {
    FileSymbols {
        lang: Lang::Rust,
        defs,
        refs,
        partial: false,
    }
}

/// An index holding `symbols` under `path`, saved through the real `store::save` under a fresh
/// temp root; the root lives as long as this value does.
pub struct SavedIndex {
    dir: tempfile::TempDir,
    /// The persisted `index.json`, byte for byte.
    pub bytes: String,
}

impl SavedIndex {
    pub fn save(path: &str, symbols: FileSymbols) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let mut idx = SymbolIndex::default();
        idx.insert_file(path.into(), symbols);
        store::save(&idx, root).unwrap();
        let bytes = std::fs::read_to_string(store::index_path(root)).unwrap();
        Self { dir, bytes }
    }

    /// The saved index read back through the real `store::load`.
    pub fn reload(&self) -> SymbolIndex {
        store::load(self.dir.path().to_str().unwrap()).expect("the persisted index loads")
    }
}

/// Saving `symbols` under `path` writes no `key` at all - a field at its default serializes
/// byte-identically to the form written before the field existed; `why` states the contract.
pub fn assert_saved_without_key(path: &str, symbols: FileSymbols, key: &str, why: &str) {
    let bytes = SavedIndex::save(path, symbols).bytes;
    assert!(!bytes.contains(key), "{why}; got:\n{bytes}");
}

/// Saving `symbols` under `path` writes `key` (`why` states the contract); returns the saved
/// index read back, for the caller to check the value survived the round-trip.
pub fn saved_with_key(path: &str, symbols: FileSymbols, key: &str, why: &str) -> SymbolIndex {
    let saved = SavedIndex::save(path, symbols);
    let bytes = &saved.bytes;
    assert!(bytes.contains(key), "{why}; got:\n{bytes}");
    saved.reload()
}

/// The `file` entry of a hand-authored legacy `index.json` (`legacy`) loaded through the real
/// `store::load`, which must succeed (`must_load` names the back-compat promise) and hold it.
/// The legacy form is stamped with the CURRENT extraction generation first, so what it proves is
/// the per-field default alone: an index from another generation loads as absent by design (it
/// is rebuilt), a separate contract pinned beside `store::load` itself.
pub fn legacy_file(legacy: &str, file: &str, must_load: &str) -> FileSymbols {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let path = store::index_path(root);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut stamped: serde_json::Value =
        serde_json::from_str(legacy).expect("the legacy index is JSON");
    stamped["grammar"] = serde_json::Value::String(
        rigger::grounder::symbols::model::GRAMMAR_TAGS_VERSION.to_string(),
    );
    std::fs::write(&path, serde_json::to_vec(&stamped).unwrap()).unwrap();
    store::load(root)
        .expect(must_load)
        .files()
        .get(file)
        .expect("the legacy file entry is present")
        .clone()
}
