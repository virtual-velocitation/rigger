//! Shared fixtures for the persisted symbol index's wire-format suites: building a one-file
//! index, saving it through the real `store` boundary and reading the bytes back, loading a
//! hand-authored legacy `index.json`, and projecting a file's definitions and its references with
//! their callers.
//! Included by each suite, and by the grounder crate for its unit tests, through `#[path]`; a suite uses the
//! subset it needs (hence the module-wide `dead_code` allowance, the same convention
//! `tests/common/mod.rs` keeps). Parser-free but for `extracted`, which only the `symbols` lane
//! compiles, so the rest compiles in every feature lane.

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

/// The symbols the grammar and query registered for the extension `ext` extract from `src`: the
/// one read of a source text through the registry, for a test that asserts what a registered
/// query yields. It is the one fixture here that reaches the parser, so only the `symbols` lane
/// holds it.
#[cfg(feature = "symbols")]
pub fn extracted(ext: &str, src: &str) -> FileSymbols {
    let entry = rigger::grounder::symbols::registry::for_extension(ext)
        .expect("the extension is registered");
    rigger::grounder::symbols::extract::extract(src, entry.lang, &entry.language, entry.tags_query)
        .expect("the registered query extracts the source")
}

/// Every definition of `fs` as `(line, name, kind)`, sorted: the one projection the extraction
/// tests of the registry compare whole.
pub fn defs_by_line(fs: &FileSymbols) -> Vec<(u32, &str, Kind)> {
    let mut defs: Vec<_> = fs
        .defs
        .iter()
        .map(|d| (d.line, d.name.as_str(), d.kind))
        .collect();
    defs.sort();
    defs
}

/// Every reference of `fs` as `(line, name, caller)`, sorted: the one projection the caller-rule
/// tests of the extractor and of the registry compare whole.
pub fn refs_with_callers(fs: &FileSymbols) -> Vec<(u32, &str, Option<&str>)> {
    let mut refs: Vec<_> = fs
        .refs
        .iter()
        .map(|r| (r.line, r.name.as_str(), r.enclosing.as_deref()))
        .collect();
    refs.sort();
    refs
}

/// Writes `body`, a hand-authored `index.json`, as the persisted index of `root`, stamped as
/// built under the extraction generation `grammar`.
pub fn persist_stamped(root: &str, grammar: &str, body: &str) {
    let path = store::index_path(root);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut stamped: serde_json::Value =
        serde_json::from_str(body).expect("the hand-authored index is JSON");
    stamped["grammar"] = serde_json::Value::String(grammar.to_string());
    std::fs::write(&path, serde_json::to_vec(&stamped).unwrap()).unwrap();
}

/// The `file` entry of a hand-authored legacy `index.json` (`legacy`) loaded through the real
/// `store::load`, which must succeed (`must_load` names the back-compat promise) and hold it.
/// The legacy form is stamped with the CURRENT extraction generation first, so what it proves is
/// the per-field default alone: an index from another generation loads as absent by design (it
/// is rebuilt), a separate contract pinned beside `store::load` itself.
pub fn legacy_file(legacy: &str, file: &str, must_load: &str) -> FileSymbols {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    persist_stamped(
        root,
        rigger::grounder::symbols::model::GRAMMAR_TAGS_VERSION,
        legacy,
    );
    store::load(root)
        .expect(must_load)
        .files()
        .get(file)
        .expect("the legacy file entry is present")
        .clone()
}
