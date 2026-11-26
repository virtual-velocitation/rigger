//! Shared fixtures for the symbol extraction suites: building a one-file set of symbols, building
//! them from Rust sources, a reference of a tagged kind or a call, a reference's qualifier, a
//! mention the facts hold of a tagged kind or a call, the facts an extractor gives one file, a
//! tree whose inline mod declares a `#[cfg(test)]` mod out of line, the out-of-line `mod`
//! declaration sources, the one-fn source the module-row tests extract, the one-`use` source and
//! the source whose function reads one constant three times and calls one function three times,
//! projecting a file's definitions, its test-marked definitions and references and its call
//! references with their callers, checking a language's definition types against a file's
//! definitions, checking a language's definitions fixture whole and a source's callers whole,
//! projecting a file's definition extents, rendering and checking a file's tagged references, and
//! looking up a registered language. Mounted once by `tests/all.rs` (a suite reaches it with
//! `use crate::symbol_index;`) and included by the
//! grounder crate for its unit tests; a suite uses the subset it needs (hence the module-wide
//! `dead_code` allowance, the same convention `tests/common/mod.rs` keeps). Every lane compiles
//! it; `registered` and `extracted`, with `assert_definitions_fixture`, `assert_callers` and
//! `assert_tagged_refs` through it, alone reach a grammar and are held by the `symbols` lane only,
//! as is `extracted_rust_index`, which calls `extracted`.
//! The grounder crate's `tests/facts_*.rs` suites and its `tests/code_view_golden.rs` mount it
//! too, with the crate named `rigger`.

#![allow(dead_code)]

#[cfg(feature = "symbols")]
use rigger::grounder::symbols::model::SymbolIndex;
use rigger::grounder::symbols::model::{Def, FileSymbols, Kind, Lang, Qualifier, SymRef};
use rigger_domain::code_facts::{FileFacts, Mention, MentionKind};
use rigger_domain::code_view::{Entry, Extract};

/// A line-1 file-level definition of `kind` named `name` with every optional field unset, its doc
/// span included, `end_line` 0 and `path` its name - the shape a product-only definition takes;
/// a caller sets the one field its case is about with struct-update syntax.
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
        doc: None,
    }
}

/// A reference to `name` at `line`, attributed to `enclosing` when it has a caller.
pub fn sym_ref(name: &str, line: u32, enclosing: Option<&str>, is_test: bool) -> SymRef {
    SymRef {
        name: name.into(),
        line,
        enclosing: enclosing.map(Into::into),
        is_test,
        ref_kind: None,
        qualifier: None,
    }
}

/// A reference to `name` at `line`, attributed to `enclosing` when it has a caller, of the tagged
/// mention kind `ref_kind`, a call when it has none.
pub fn sym_ref_of_kind(
    name: &str,
    line: u32,
    enclosing: Option<&str>,
    ref_kind: Option<&str>,
    is_test: bool,
) -> SymRef {
    SymRef {
        ref_kind: ref_kind.map(str::to_string),
        ..sym_ref(name, line, enclosing, is_test)
    }
}

/// A reference's qualifier `name`, a path (`::`) when `path` and a member access (`.`) otherwise.
pub fn qualifier(name: &str, path: bool) -> Qualifier {
    Qualifier {
        name: name.into(),
        path,
    }
}

/// The mention written `name` of `leaf` on `line`, inside the definition at `enclosing`, of
/// the tagged mention kind `kind`, a call when it has none.
pub fn mention_in(
    enclosing: Option<u32>,
    name: &str,
    leaf: &str,
    kind: Option<&str>,
    line: u32,
    test: bool,
) -> Mention {
    Mention {
        enclosing,
        name: name.to_string(),
        leaf: leaf.to_string(),
        kind: match kind {
            None => MentionKind::Call,
            Some(kind) => MentionKind::Tagged(kind.to_string()),
        },
        line,
        test,
    }
}

/// The facts `extractor` gives the file at `path` holding `source`, its entry's blob empty.
pub fn facts_of(extractor: &dyn Extract, path: &str, source: &[u8]) -> FileFacts {
    let entry = Entry {
        path: path.to_string(),
        blob: String::new(),
    };
    extractor.extract(&entry, source)
}

/// A fully-parsed Rust file holding `defs` and `refs`, with no doc span.
pub fn rust_file(defs: Vec<Def>, refs: Vec<SymRef>) -> FileSymbols {
    FileSymbols {
        lang: Lang::Rust,
        defs,
        refs,
        partial: false,
        doc: None,
    }
}

/// A fully-parsed Rust file defining the function `f` and holding `refs`.
pub fn defining_f(refs: Vec<SymRef>) -> FileSymbols {
    rust_file(vec![def(Kind::Function, "f")], refs)
}

/// The call to `G` on line 2, inside the function `f` ([`defining_f`]).
pub fn g_called_in_f() -> SymRef {
    sym_ref("G", 2, Some("f"), false)
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
    let tags = entry.tags().expect("the registered query compiles");
    rigger::grounder::symbols::extract::extract(src, entry.lang, tags)
        .expect("the registered query extracts the source")
}

/// An index holding each `(path, src)` of `files` as the Rust symbols [`extracted`] reads off
/// `src`, under `path`.
#[cfg(feature = "symbols")]
pub fn extracted_rust_index(files: &[(&str, &str)]) -> SymbolIndex {
    let mut idx = SymbolIndex::default();
    for (path, src) in files {
        idx.insert_file(path.to_string(), extracted("rs", src));
    }
    idx
}

/// The file `mod outer { #[cfg(test)] mod x; }` in [`INLINE_TEST_MOD_TREE`]'s `src/lib.rs`
/// names: the first of its module candidates the tree holds.
pub const INLINE_TEST_MOD_TARGET: &str = "src/outer/x.rs";

/// The entity id of the `helper` [`INLINE_TEST_MOD_TREE`] defines in [`INLINE_TEST_MOD_TARGET`].
pub const INLINE_TEST_MOD_HELPER: &str = "src/outer/x.rs::helper";

/// A crate root declaring `x` out of line under `#[cfg(test)]` inside the inline `mod outer`,
/// beside [`INLINE_TEST_MOD_TARGET`] defining `helper`, and no `src/x.rs`.
pub const INLINE_TEST_MOD_TREE: [(&str, &str); 2] = [
    (
        "src/lib.rs",
        "mod outer {\n    #[cfg(test)]\n    mod x;\n}\n",
    ),
    (INLINE_TEST_MOD_TARGET, "pub fn helper() {}\n"),
];

/// A Rust source declaring the module `x` out of line.
pub const MOD_X: &str = "mod x;\n";
/// [`MOD_X`] under `#[cfg(test)]`.
pub const CFG_TEST_MOD_X: &str = "#[cfg(test)]\nmod x;\n";
/// [`MOD_X`] through a `#[path]` override naming `gen/real.rs`.
pub const PATH_MOD_X: &str = "#[path = \"gen/real.rs\"]\nmod x;\n";
/// [`MOD_X`] through an absolute `#[path]` override, which names no file.
pub const ABSOLUTE_PATH_MOD_X: &str = "#[path = \"/etc/x.rs\"]\nmod x;\n";
/// [`MOD_X`] inside the inline `mod outer`.
pub const INLINE_MOD_X: &str = "mod outer {\n    mod x;\n}\n";
/// A Rust source declaring the module `z` out of line.
pub const MOD_Z: &str = "mod z;\n";
/// A Rust source declaring the module `b` out of line inside the inline `mod a`, through a
/// `#[path]` override naming `b.rs`.
pub const INLINE_PATH_MOD_B: &str = "mod a {\n    #[path = \"b.rs\"]\n    mod b;\n}\n";
/// A Rust source defining the fn `y`.
pub const FN_Y: &str = "fn y() {}\n";
/// A Rust file holding one `use` and nothing else: its whole extraction is one tagged reference.
pub const USE_ONLY_SOURCE: &str = "use crate::store::Ledger;\n";
/// A Rust file defining `LIMIT`, `g` and `f`, in that order, whose `f` reads `LIMIT` on lines 4,
/// 6 and 8 and calls `g` on lines 5, 7 and 9.
pub const THREE_READS_AND_THREE_CALLS_SOURCE: &str = "\
const LIMIT: u32 = 3;
fn g() {}
fn f() -> u32 {
    let a = LIMIT;
    g();
    let b = LIMIT;
    g();
    let c = LIMIT;
    g();
    a + b + c
}
";

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
/// `ext`, parses whole and holds exactly the definitions `defs`, the call references `refs` and
/// the tagged references `tagged` ([`tagged_refs`]), each with its caller, and each row of `rows`,
/// the language's DEFINITION TYPES, names exactly one of those definitions.
#[cfg(feature = "symbols")]
pub fn assert_definitions_fixture(
    ext: &str,
    src: &str,
    rows: &[(&str, Kind, &str)],
    defs: &[(u32, &str, Kind)],
    refs: &[(u32, &str, Option<&str>)],
    tagged: &str,
) {
    let fs = extracted(ext, src);
    assert_eq!(defs_by_line(&fs), defs);
    assert!(!fs.partial, "the fixture parses whole");
    assert_one_def_per_row(&fs, rows);
    assert_callers(ext, src, refs);
    assert_tagged_refs(ext, src, tagged, "the fixture's tagged references");
}

/// The call references the registered query for `ext` extracts from `src`, each with its caller,
/// are exactly `refs` ([`refs_with_callers`]): the one check of a source text's callers whole.
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

/// Every call reference of `fs` ([`FileSymbols::calls`], a reference with no `ref_kind`) as
/// `(line, name, caller)`, sorted: the one projection the caller-rule tests of the extractor and
/// of the registry compare whole.
pub fn refs_with_callers(fs: &FileSymbols) -> Vec<(u32, &str, Option<&str>)> {
    let mut refs: Vec<_> = fs
        .calls()
        .map(|r| (r.line, r.name.as_str(), r.enclosing.as_deref()))
        .collect();
    refs.sort();
    refs
}

/// Every tagged reference of `fs` (spec 117: one carrying a `ref_kind`), one line each,
/// `<line> <ref kind> <name> <caller>`, the caller `-` for a top-level one, sorted by line, then
/// name, then kind, then caller and joined by `\n`: the one rendering the reference-kind tests
/// of the extractor and of the registry compare whole.
pub fn tagged_refs(fs: &FileSymbols) -> String {
    let mut tagged: Vec<_> = fs
        .refs
        .iter()
        .filter_map(|r| {
            let kind = r.ref_kind.as_deref()?;
            Some((
                r.line,
                r.name.as_str(),
                kind,
                r.enclosing.as_deref().unwrap_or("-"),
            ))
        })
        .collect();
    tagged.sort();
    tagged
        .iter()
        .map(|(line, name, kind, caller)| format!("{line} {kind} {name} {caller}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The tagged references the registered query for `ext` extracts from `src`, rendered by
/// [`tagged_refs`], are exactly `expected`, `case` naming the rule they follow: the one check of
/// a source text's tagged references whole.
#[cfg(feature = "symbols")]
pub fn assert_tagged_refs(ext: &str, src: &str, expected: &str, case: &str) {
    assert_eq!(tagged_refs(&extracted(ext, src)), expected, "{case}");
}
