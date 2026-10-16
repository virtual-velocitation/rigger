//! Shared fixtures for the persisted symbol index's wire-format suites: building a one-file
//! index, saving it through the real `store` boundary and reading the bytes back, writing a
//! hand-authored `index.json` under the current extraction generation and loading a legacy one
//! through the real `store::load`, projecting a file's definitions, its test-marked
//! definitions and references and its references with their callers, checking a language's definition
//! types against a file's definitions, checking a language's definitions fixture whole and a source's callers whole,
//! projecting a file's definition extents, checking the variant names a file's kinds persist
//! under, and looking up a registered language.
//! Included by each suite, and by the grounder crate for its unit tests, through `#[path]`; a suite uses the
//! subset it needs (hence the module-wide `dead_code` allowance, the same convention
//! `tests/common/mod.rs` keeps). Every lane compiles it; `registered` and `extracted`, with
//! `assert_definitions_fixture` and `assert_callers` through it, alone reach a grammar and are held by the `symbols`
//! lane only.

#![allow(dead_code)]

use rigger::grounder::symbols::model::{Def, FileSymbols, Kind, Lang, SymRef, SymbolIndex};
use rigger::grounder::symbols::store;

/// A line-1 file-level definition of `kind` named `name` with every optional field unset,
/// `end_line` 0 and `path` its name, the end line and path [`legacy_file`] stamps on a legacy
/// definition - the shape a product-only definition persists as; a caller sets the one field its
/// case is about with struct-update syntax.
pub fn def(kind: Kind, name: &str) -> Def {
    Def {
        kind,
        name: name.into(),
        line: 1,
        end_line: 0,
        path: name.into(),
        parent: None,
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

    /// The kind each definition of `path` is written under in the persisted `index.json`, in
    /// the order written.
    pub fn written_kinds(&self, path: &str) -> Vec<String> {
        let persisted: serde_json::Value = serde_json::from_str(&self.bytes).unwrap();
        persisted["files"][path]["defs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["kind"].as_str().unwrap().to_string())
            .collect()
    }

    /// The saved index read back through the real `store::load`.
    pub fn reload(&self) -> SymbolIndex {
        store::load(self.dir.path().to_str().unwrap()).expect("the persisted index loads")
    }
}

/// The persisted index writes each definition of `kinds`, a (kind, name, variant name) triple,
/// under its variant name, in the order given, and reads the file saved under `path` back as
/// the kinds it was saved as.
pub fn assert_kinds_persist_under_their_variant_names(path: &str, kinds: &[(Kind, &str, &str)]) {
    let file = || {
        rust_file(
            kinds
                .iter()
                .map(|(kind, name, _)| def(*kind, name))
                .collect(),
            vec![],
        )
    };
    let variants: Vec<&str> = kinds.iter().map(|(_, _, variant)| *variant).collect();
    let saved = SavedIndex::save(path, file());
    assert_eq!(saved.written_kinds(path), variants);
    assert_eq!(saved.reload().files()[path], file());
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

/// The grammar and query entry registered for the extension `ext`: the one lookup of a registered
/// language, for a test that reads a source under it. It names the grammar, so only the
/// `symbols` lane holds it.
#[cfg(feature = "symbols")]
pub fn registered(ext: &str) -> rigger::grounder::symbols::registry::LanguageEntry {
    rigger::grounder::symbols::registry::for_extension(ext).expect("the extension is registered")
}

/// The symbols the grammar and query registered for the extension `ext` extract from `src`: the
/// one read of a source text through the registry, for a test that asserts what a registered
/// query yields. It is the one fixture here that reaches the parser, so only the `symbols` lane
/// holds it.
#[cfg(feature = "symbols")]
pub fn extracted(ext: &str, src: &str) -> FileSymbols {
    let entry = registered(ext);
    rigger::grounder::symbols::extract::extract(src, entry.lang, &entry.language, entry.tags_query)
        .expect("the registered query extracts the source")
}

/// Every definition of `fs` as `(line, end line, name)`, sorted: the one projection of each
/// `Def`'s extent the extent tests compare whole.
pub fn extents_by_line(fs: &FileSymbols) -> Vec<(u32, u32, &str)> {
    let mut extents: Vec<_> = fs
        .defs
        .iter()
        .map(|d| (d.line, d.end_line, d.name.as_str()))
        .collect();
    extents.sort();
    extents
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

/// Each row of a language's DEFINITION TYPES, a (node type, kind, fixture leaf) triple, names
/// exactly one definition of `fs`, the symbols extracted from that language's fixture: one of
/// the row's kind at the row's leaf.
pub fn assert_one_def_per_row(fs: &FileSymbols, rows: &[(&str, Kind, &str)]) {
    for (node_type, kind, leaf) in rows {
        let found = fs
            .defs
            .iter()
            .filter(|d| d.kind == *kind && d.name == *leaf)
            .count();
        assert_eq!(found, 1, "{node_type}: one {kind:?} definition at {leaf:?}");
    }
}

/// A language's definitions fixture `src`, read through the grammar and query registered for
/// `ext`, parses whole and holds exactly the definitions `defs` and the references `refs`, each
/// with its caller, and each row of `rows`, the language's DEFINITION TYPES, names exactly one
/// of those definitions.
#[cfg(feature = "symbols")]
pub fn assert_definitions_fixture(
    ext: &str,
    src: &str,
    rows: &[(&str, Kind, &str)],
    defs: &[(u32, &str, Kind)],
    refs: &[(u32, &str, Option<&str>)],
) {
    let fs = extracted(ext, src);
    assert_eq!(defs_by_line(&fs), defs);
    assert!(!fs.partial, "the fixture parses whole");
    assert_one_def_per_row(&fs, rows);
    assert_callers(ext, src, refs);
}

/// The references the registered query for `ext` extracts from `src`, each with its caller, are
/// exactly `refs` ([`refs_with_callers`]): the one check of a source text's callers whole.
#[cfg(feature = "symbols")]
pub fn assert_callers(ext: &str, src: &str, refs: &[(u32, &str, Option<&str>)]) {
    assert_eq!(refs_with_callers(&extracted(ext, src)), refs);
}

/// The `(line, name)` of each item marked test code, sorted: [`test_code_by_line`]'s halves.
pub type TestLines<'a> = Vec<(u32, &'a str)>;

/// Every definition and every reference of `fs` marked test code, each as `(line, name)`,
/// sorted: the one projection the test-attribute tests of the extractor compare whole, so an
/// item wrongly marked and one wrongly left production both fail it.
pub fn test_code_by_line(fs: &FileSymbols) -> (TestLines<'_>, TestLines<'_>) {
    (
        test_by_line(fs.defs.iter().map(|d| (d.is_test, d.line, d.name.as_str()))),
        test_by_line(fs.refs.iter().map(|r| (r.is_test, r.line, r.name.as_str()))),
    )
}

/// The `(line, name)` of each `(is_test, line, name)` in `marked` that is test code, sorted:
/// [`test_code_by_line`]'s projection of a file's definitions and of its references alike.
fn test_by_line<'a>(marked: impl Iterator<Item = (bool, u32, &'a str)>) -> TestLines<'a> {
    let mut lines: Vec<_> = marked
        .filter(|(is_test, _, _)| *is_test)
        .map(|(_, line, name)| (line, name))
        .collect();
    lines.sort();
    lines
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

/// A hand-authored `index.json` body holding `file` alone, a Rust file of the definitions `defs`
/// and the references `refs` (each a JSON array), for [`persist_stamped`] or [`persist_current`]
/// to write.
pub fn rust_index_json(file: &str, defs: serde_json::Value, refs: serde_json::Value) -> String {
    serde_json::json!({ "files": { file: { "lang": "Rust", "defs": defs, "refs": refs } } })
        .to_string()
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

/// Writes `body`, a hand-authored `index.json`, as the persisted index of `root`, stamped as
/// built under the current extraction generation, so only its own shape decides whether it loads.
pub fn persist_current(root: &str, body: &str) {
    persist_stamped(
        root,
        rigger::grounder::symbols::model::GRAMMAR_TAGS_VERSION,
        body,
    );
}

/// The `file` entry of a hand-authored legacy `index.json` (`legacy`) loaded through the real
/// `store::load`, which must succeed (`must_load` names the back-compat promise) and hold it.
/// The legacy form is stamped with the CURRENT extraction generation first, and each of its
/// definitions lacking `end_line` with 0 and lacking `path` with its `name`, the shape a current
/// index holds, so what it proves is the per-field default alone: an index from another
/// generation, or one lacking `end_line` or `path`, loads as absent by design (it is rebuilt), a
/// separate contract pinned beside `store::load` itself.
pub fn legacy_file(legacy: &str, file: &str, must_load: &str) -> FileSymbols {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let mut current: serde_json::Value =
        serde_json::from_str(legacy).expect("the hand-authored index is JSON");
    for symbols in current["files"]
        .as_object_mut()
        .expect("the legacy index holds files")
        .values_mut()
    {
        for def in symbols["defs"]
            .as_array_mut()
            .expect("a legacy file holds defs")
        {
            let def = def
                .as_object_mut()
                .expect("a legacy definition is an object");
            let name = def["name"].clone();
            def.entry("end_line").or_insert(0.into());
            def.entry("path").or_insert(name);
        }
    }
    persist_current(root, &current.to_string());
    store::load(root)
        .expect(must_load)
        .files()
        .get(file)
        .expect("the legacy file entry is present")
        .clone()
}
