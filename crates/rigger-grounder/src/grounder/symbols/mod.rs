//! The symbol extraction: a projection of the code tree into definitions, references, and a
//! name-level, per-language cross-reference graph (architecture 5.5). Designed once for its
//! several consumers (the code store's extractor, the emit pass, and - in spec 16 - blast-radius).
//! The grounder reads the code store's view; nothing here persists an index.
//!
//! Dependency direction (principle 7): the `model` is PARSER-FREE and always compiled, in
//! both feature lanes, so a build without the `symbols` feature - which never links
//! tree-sitter - still compiles the model. That is the compile-time proof that no
//! `tree_sitter::` type crosses into the data-model API. tree-sitter lives ONLY behind the
//! `symbols` feature, confined to `extract` (and, in later units, the registry).

pub mod model;

/// Tags-based extraction over an INJECTED `(grammar, tag query)` pair - the ONE place
/// tree-sitter is touched. Feature-gated: the light lane drops it entirely.
#[cfg(feature = "symbols")]
pub mod extract;

/// The code-to-events emit pass (spec 29a): lowers an extracted index into
/// `CodeEntityExtracted` / `EdgeInferred` events the context-graph fold ingests, so code
/// structure becomes a rebuildable projection over the log. Feature-gated with the rest of the
/// extraction pass; the always-compiled fold arms in `contextgraph::sqlite` ingest what it emits.
#[cfg(feature = "symbols")]
pub mod events;

/// The `extension -> (grammar, tag query)` registry: maps a file to the grammar the
/// extractor injects, for the five shipped languages, with a `--language` override
/// (unit 2). Names `tree_sitter::Language` types, so it is confined to the `symbols`
/// feature exactly like `extract`.
#[cfg(feature = "symbols")]
pub mod registry;

/// The `symbols` grounder (unit 4): the `Grounder` port over the code store's view of the head,
/// ranking a definition-name match above a reference above an incidental prose mention. It is
/// confined to the `symbols` feature, as the code store's extractor is; `main::select_grounder`
/// wires it when the feature is built, and `grounder_for` returns a loud error when it is not.
#[cfg(feature = "symbols")]
pub mod grounder;

#[cfg(feature = "symbols")]
use crate::grounder::symbols::model::{Lang, SymbolIndex};
#[cfg(feature = "symbols")]
use crate::grounder::{relative_path, walk_guarded};
#[cfg(feature = "symbols")]
use std::ops::ControlFlow;
#[cfg(feature = "symbols")]
use std::path::Path;

/// A line-ending-normalized content hash: `"a\r\nb\r\n"` and `"a\nb\n"` hash identically, so the
/// SAME source keys the SAME entry whether it was checked out with CRLF (Windows) or LF (Unix).
/// Uses a fixed-seed FNV-1a - deliberately chosen over `DefaultHasher` (whose seed the stdlib does
/// NOT guarantee stable across builds) - so the value is a stable content key across processes,
/// builds, and machines.
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

/// Build the whole-project index over `root`: walk the tree with the SHARED scoped walk
/// (`walk_guarded`, the same walk grep and the ingests use, so they never diverge on
/// which files count), and for each file whose extension the registry resolves, read it and
/// extract its symbols under its normalized relative path. A file whose extension is unregistered
/// is skipped without being read; a file that cannot be read or fails to extract contributes no
/// entry; a parse that recovers to no symbols contributes an empty one. None of them crashes the
/// walk. `override_lang` forces one language for every file (the `--language` override); `None`
/// auto-detects per extension.
#[cfg(feature = "symbols")]
pub fn build_index(root: &str, override_lang: Option<Lang>) -> SymbolIndex {
    let mut idx = SymbolIndex::default();
    let _ = walk_guarded(Path::new(root), &mut |path| {
        let rel = relative_path(Path::new(root), path);
        index_one_file(root, &rel, &mut idx, override_lang);
        ControlFlow::Continue(())
    });
    idx
}

/// The symbols of the file at `path` under the grammar the registry resolves for that path and
/// `override_lang`, beside the text they were extracted from: none when no grammar resolves, an
/// inner none when `source` answers no text or the extraction fails. `source` is asked for the
/// text only once a grammar resolves, so a path under no grammar costs no read. This is the ONE place
/// a path's grammar is paired with the extraction its stored symbols come from, so every reader
/// of them - the index ([`index_one_file`]) and the batch re-extracted from bytes
/// ([`bytes_batch`](events::bytes_batch)) - reads the same symbols from the same text; a show
/// surface's body bound resolves its grammar itself and extracts through
/// [`extract::locate_extent`].
#[cfg(feature = "symbols")]
pub(crate) fn extracted<S: AsRef<str>>(
    path: &str,
    source: impl FnOnce() -> Option<S>,
    override_lang: Option<Lang>,
) -> Option<Option<(S, model::FileSymbols)>> {
    let grammar = registry::for_path(path, override_lang)?;
    Some(source().and_then(|text| {
        extract::extract(
            text.as_ref(),
            grammar.lang,
            &grammar.language,
            grammar.tags_query,
        )
        .ok()
        .map(|symbols| (text, symbols))
    }))
}

/// Extract the single file at relative path `rel` (under `root`) into `idx`, keyed by `rel`, if
/// the registry resolves a grammar for it. This is the ONE per-file indexing authority of the
/// whole-tree `build_index`. The file is read only once a grammar resolves, and its symbols are
/// [`extracted`] from that text.
///
/// The miss arms keep the index equal to what a fresh `build_index` over the current tree holds,
/// and none of them crashes the walk:
/// - an UNRESOLVED extension leaves `idx` untouched and the file unread, whether or not it could
///   be read - a fresh build never indexes such a file;
/// - under a grammar, a file that can no longer be READ (deleted, unreadable or not UTF-8) or that
///   fails to EXTRACT (a tags-query failure) has its entry REMOVED via
///   [`SymbolIndex::remove_file`];
/// - a parse that recovers to NO symbols still returns `Ok` and INSERTS an empty entry (replacing
///   any prior one).
#[cfg(feature = "symbols")]
pub fn index_one_file(root: &str, rel: &str, idx: &mut SymbolIndex, override_lang: Option<Lang>) {
    let read = || std::fs::read_to_string(Path::new(root).join(rel)).ok();
    match extracted(rel, read, override_lang) {
        None => {}
        Some(Some((_, fs))) => idx.insert_file(rel.to_string(), fs),
        Some(None) => idx.remove_file(rel),
    }
}

#[cfg(test)]
mod content_hash_tests {
    use super::content_hash;

    #[test]
    fn content_hash_is_line_ending_normalized() {
        assert_eq!(content_hash("a\r\nb\r\n"), content_hash("a\nb\n"));
    }

    #[test]
    fn content_hash_is_a_stable_fixed_seed_value() {
        // FNV-1a's offset basis is the hash of the empty string: pinning it catches a seed or
        // algorithm change, which would move every key derived from it across builds.
        assert_eq!(content_hash(""), "cbf29ce484222325");
        assert_ne!(content_hash("a"), content_hash("b"));
    }
}

#[cfg(test)]
#[cfg(feature = "symbols")]
mod tests {
    use super::*;
    use crate::grounder::symbols::model::{Def, Kind};

    /// Every definition named `name` across `idx` - the tests' observer of what an index holds.
    fn defs_named<'a>(idx: &'a SymbolIndex, name: &str) -> Vec<&'a Def> {
        idx.files()
            .values()
            .flat_map(|f| f.defs.iter())
            .filter(|d| d.name == name)
            .collect()
    }

    #[test]
    fn indexing_a_file_again_replaces_only_that_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        std::fs::write(dir.path().join("a.rs"), "fn one() {}\n").unwrap();
        std::fs::write(dir.path().join("b.rs"), "fn two() {}\n").unwrap();
        let mut idx = build_index(root, None);
        // Change only a.rs; index just that file again.
        std::fs::write(dir.path().join("a.rs"), "fn oneprime() {}\n").unwrap();
        index_one_file(root, "a.rs", &mut idx, None);
        assert_eq!(defs_named(&idx, "oneprime").len(), 1); // a.rs's new symbol is in
        assert!(defs_named(&idx, "one").is_empty()); // a.rs's old symbol is gone (entry replaced)
        assert_eq!(defs_named(&idx, "two").len(), 1); // b.rs untouched
    }

    #[test]
    fn indexing_a_deleted_file_removes_its_symbols_and_equals_a_fresh_build() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        std::fs::write(dir.path().join("a.rs"), "fn gone_symbol() {}\n").unwrap();
        std::fs::write(dir.path().join("b.rs"), "fn kept_symbol() {}\n").unwrap();
        let mut idx = build_index(root, None);
        assert_eq!(defs_named(&idx, "gone_symbol").len(), 1);

        // Delete a.rs, then index JUST it again. The gone file's stale symbols must be PURGED,
        // not left describing a file that no longer exists.
        std::fs::remove_file(dir.path().join("a.rs")).unwrap();
        index_one_file(root, "a.rs", &mut idx, None);

        // The deleted file's symbol is gone and its entry is dropped; the untouched file stands.
        assert!(defs_named(&idx, "gone_symbol").is_empty());
        assert!(!idx.files().contains_key("a.rs"));
        assert_eq!(defs_named(&idx, "kept_symbol").len(), 1);

        // The incremental index over the deletion EQUALS a fresh whole-tree build over the
        // surviving tree (which never visits the gone file either).
        let fresh = build_index(root, None);
        assert_eq!(idx, fresh);
    }

    /// A path no grammar resolves is left alone without its file being asked for: an entry the
    /// index holds for it stands whether the file reads or cannot be read at all.
    #[test]
    fn an_entry_under_no_grammar_stands_whether_or_not_its_file_can_be_read() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        std::fs::write(dir.path().join("notes.txt"), "fn in_prose() {}\n").unwrap();
        // Indexed under a forced language, as a `--language` override indexes it.
        let mut idx = SymbolIndex::default();
        index_one_file(root, "notes.txt", &mut idx, Some(Lang::Rust));
        let forced = idx.clone();
        assert_eq!(defs_named(&idx, "in_prose").len(), 1);

        index_one_file(root, "notes.txt", &mut idx, None);
        assert_eq!(idx, forced, "a file no grammar resolves is untouched");

        // A directory at the path cannot be read as text under any uid.
        std::fs::remove_file(dir.path().join("notes.txt")).unwrap();
        std::fs::create_dir(dir.path().join("notes.txt")).unwrap();
        index_one_file(root, "notes.txt", &mut idx, None);
        assert_eq!(
            idx, forced,
            "no grammar resolves, so the unreadable path is never asked for"
        );

        // The same unreadable path under a grammar has its entry removed.
        index_one_file(root, "notes.txt", &mut idx, Some(Lang::Rust));
        assert_eq!(
            idx,
            SymbolIndex::default(),
            "an unreadable file's entry goes once a grammar resolves"
        );
    }

    /// The text is asked for only once a grammar resolves: a path under no grammar answers the
    /// outer none with the text never asked, and a path under a grammar asks exactly once and
    /// hands back the very text it was given beside the symbols extracted from it.
    #[test]
    fn extracted_asks_for_the_text_only_once_a_grammar_resolves() {
        let asked = std::cell::Cell::new(0_u32);
        let source = || {
            asked.set(asked.get() + 1);
            Some(String::from("fn one() {}\n"))
        };

        assert_eq!(extracted("notes.txt", source, None), None);
        assert_eq!(asked.get(), 0, "no grammar resolves, so no text is asked");

        let (text, symbols) = extracted("a.rs", source, None)
            .expect("a grammar resolves for a.rs")
            .expect("the handed text extracts");
        assert_eq!(asked.get(), 1, "the text is asked exactly once");
        assert_eq!(text, "fn one() {}\n");
        assert_eq!(symbols.lang, Lang::Rust);
        let defs: Vec<(&str, Kind, u32)> = symbols
            .defs
            .iter()
            .map(|d| (d.name.as_str(), d.kind, d.line))
            .collect();
        assert_eq!(defs, [("one", Kind::Function, 1)]);
    }

    #[test]
    fn build_index_walks_the_tree_and_skips_unparseable_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "fn parse() {}\n").unwrap();
        // Unregistered extension -> skipped entirely.
        std::fs::write(dir.path().join("b.txt"), "not code\n").unwrap();
        // Malformed source -> tree-sitter recovers to partial/empty symbols, never a crash.
        std::fs::write(dir.path().join("c.rs"), "fn (((").unwrap();
        let idx = build_index(dir.path().to_str().unwrap(), None);
        // The parseable file's definition is indexed with its kind.
        assert!(defs_named(&idx, "parse")
            .iter()
            .any(|d| d.kind == Kind::Function));
        assert!(idx.files().contains_key("a.rs"));
        // The unregistered file is never indexed.
        assert!(!idx.files().contains_key("b.txt"));
    }
}
