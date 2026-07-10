//! The symbol index: a projection of the code tree into definitions, references, and a
//! name-level, per-language cross-reference graph (architecture 5.5). Designed once for its
//! several consumers (the grounder, persistence, and - in spec 16 - blast-radius).
//!
//! Dependency direction (principle 7): the `model` is PARSER-FREE and always compiled, in
//! both feature lanes, so a build without the `symbols` feature - which never links
//! tree-sitter - still compiles the model. That is the compile-time proof that no
//! `tree_sitter::` type crosses into the data-model API. tree-sitter lives ONLY behind the
//! `symbols` feature, confined to `extract` (and, in later units, the registry).

pub mod model;

/// Persist + load the index (unit 3). UNGATED like `model`: it is a projection of the
/// parser-free model onto disk, names no `tree_sitter::` type, and its cross-process
/// determinism-by-construction tests therefore run in BOTH feature lanes.
pub mod store;

/// Tags-based extraction over an INJECTED `(grammar, tag query)` pair - the ONE place
/// tree-sitter is touched. Feature-gated: the light lane drops it entirely.
#[cfg(feature = "symbols")]
pub mod extract;

/// The `extension -> (grammar, tag query)` registry: maps a file to the grammar the
/// extractor injects, for the five shipped languages, with a `--language` override
/// (unit 2). Names `tree_sitter::Language` types, so it is confined to the `symbols`
/// feature exactly like `extract`.
#[cfg(feature = "symbols")]
pub mod registry;

/// The `symbols` grounder (unit 4): the `Grounder` port over the persisted index, ranking a
/// definition-name match above a reference above an incidental prose mention. It consumes the
/// gated extraction path (`build_index`/`index_one_file`), so it is confined to the `symbols`
/// feature; `main::select_grounder` wires it when the feature is built, and `grounder_for`
/// returns a loud error when it is not.
#[cfg(feature = "symbols")]
pub mod grounder;

/// The `hybrid` grounder (unit 5): composes the structural [`grounder::Symbols`] with turbovec's
/// semantic engine - structural matches first, the vector pass fills the recall a name match
/// misses. Confined to the `symbols` feature (it needs the index); the turbovec half is gated
/// WITHIN it, so absent turbovec it is exactly the `symbols` mode.
#[cfg(feature = "symbols")]
pub mod hybrid;

#[cfg(feature = "symbols")]
use crate::grounder::symbols::model::{Lang, SymbolIndex};
#[cfg(feature = "symbols")]
use crate::grounder::walk_guarded;
#[cfg(feature = "symbols")]
use std::collections::HashSet;
#[cfg(feature = "symbols")]
use std::ops::ControlFlow;
#[cfg(feature = "symbols")]
use std::path::Path;

/// Build the whole-project index over `root`: walk the tree with the SHARED skip-dirs + cycle
/// guard (`walk_guarded`, the same walk grep and turbovec use, so the three never diverge), and
/// for each file whose extension the registry resolves, extract its symbols under its
/// normalized relative path. A file whose extension is unregistered is skipped; a file that
/// cannot be read, or whose parse recovers to no symbols, contributes whatever the tags run
/// produced and NEVER crashes the walk. `override_lang` forces one language for every file
/// (the `--language` override); `None` auto-detects per extension.
#[cfg(feature = "symbols")]
pub fn build_index(root: &str, override_lang: Option<Lang>) -> SymbolIndex {
    let mut idx = SymbolIndex::default();
    let mut visited = HashSet::new();
    let _ = walk_guarded(Path::new(root), &mut visited, &mut |path| {
        let rel = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned();
        index_one_file(root, &rel, &mut idx, override_lang);
        ControlFlow::Continue(())
    });
    idx
}

/// Extract the single file at relative path `rel` (under `root`) into `idx`, keyed by `rel`, if
/// the registry resolves a grammar for it. This is the ONE per-file extraction authority: the
/// whole-tree `build_index` and the incremental `reindex_files` both freshen a file through here,
/// so a file is indexed identically whether the whole tree is built or one file is re-parsed.
///
/// The miss arms keep the incremental index equal to a fresh `build_index` over the current tree,
/// and none of them crashes the walk:
/// - an UNRESOLVED extension leaves `idx` untouched - a fresh build never indexes such a file, so
///   there is no entry to hold or drop;
/// - a file that can no longer be READ (deleted or unreadable) or that fails to EXTRACT (a
///   tags-query failure) has its entry REMOVED via [`SymbolIndex::remove_file`], so reindexing a
///   deleted file purges its stale symbols rather than grounding a file a fresh build never visits;
/// - a parse that recovers to NO symbols still returns `Ok` and INSERTS an empty entry (replacing
///   any prior one), so a file edited down to its last symbol overwrites to empty rather than
///   keeping stale defs.
#[cfg(feature = "symbols")]
pub fn index_one_file(root: &str, rel: &str, idx: &mut SymbolIndex, override_lang: Option<Lang>) {
    let Some(entry) = registry::for_path(rel, override_lang) else {
        return;
    };
    let abs = Path::new(root).join(rel);
    match std::fs::read_to_string(&abs)
        .ok()
        .and_then(|src| extract::extract(&src, entry.lang, &entry.language, entry.tags_query).ok())
    {
        Some(fs) => idx.insert_file(rel.to_string(), fs),
        None => idx.remove_file(rel),
    }
}

/// Re-extract ONLY `files` into `idx`, replacing each named file's entry and leaving every
/// other file's symbols intact - the incremental freshening `reindex` runs after a unit
/// integrates (re-parse the just-changed files, not the whole tree). Each file is freshened
/// through the shared [`index_one_file`] authority, NOT a second extract loop, so a file is
/// indexed IDENTICALLY whether the whole tree is built (`build_index`) or one file is
/// re-parsed here (one mutation authority; the two paths cannot drift). A named file that can no
/// longer be read (deleted or unreadable) or that fails to extract has its entry REMOVED, so
/// reindexing a deletion leaves the index equal to a fresh `build_index` over the surviving tree;
/// a file with an unregistered extension leaves `idx` untouched, exactly as `index_one_file` does
/// on the whole-tree walk.
#[cfg(feature = "symbols")]
pub fn reindex_files(
    root: &str,
    idx: &mut SymbolIndex,
    files: &[String],
    override_lang: Option<Lang>,
) {
    for rel in files {
        index_one_file(root, rel, idx, override_lang);
    }
}

#[cfg(test)]
#[cfg(feature = "symbols")]
mod tests {
    use super::*;
    use crate::grounder::symbols::model::Kind;

    #[test]
    fn reindex_replaces_only_the_named_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        std::fs::write(dir.path().join("a.rs"), "fn one() {}\n").unwrap();
        std::fs::write(dir.path().join("b.rs"), "fn two() {}\n").unwrap();
        let mut idx = build_index(root, None);
        // Change only a.rs; reindex just that file.
        std::fs::write(dir.path().join("a.rs"), "fn oneprime() {}\n").unwrap();
        reindex_files(root, &mut idx, &["a.rs".into()], None);
        assert_eq!(idx.definitions_named("oneprime").len(), 1); // a.rs's new symbol is in
        assert!(idx.definitions_named("one").is_empty()); // a.rs's old symbol is gone (entry replaced)
        assert_eq!(idx.definitions_named("two").len(), 1); // b.rs untouched
    }

    #[test]
    fn reindex_over_a_deleted_file_removes_its_symbols_and_equals_a_fresh_build() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        std::fs::write(dir.path().join("a.rs"), "fn gone_symbol() {}\n").unwrap();
        std::fs::write(dir.path().join("b.rs"), "fn kept_symbol() {}\n").unwrap();
        let mut idx = build_index(root, None);
        assert_eq!(idx.definitions_named("gone_symbol").len(), 1);

        // Delete a.rs, then reindex JUST it - the incremental freshening a post-integrate reindex
        // runs. The gone file's stale symbols must be PURGED, not left grounding a file that no
        // longer exists.
        std::fs::remove_file(dir.path().join("a.rs")).unwrap();
        reindex_files(root, &mut idx, &["a.rs".into()], None);

        // The deleted file's symbol is gone and its entry is dropped; the untouched file stands.
        assert!(idx.definitions_named("gone_symbol").is_empty());
        assert!(!idx.files().contains_key("a.rs"));
        assert_eq!(idx.definitions_named("kept_symbol").len(), 1);

        // The incremental index over the deletion EQUALS a fresh whole-tree build over the
        // surviving tree - the invariant reindex must hold (it never visits the gone file either).
        let fresh = build_index(root, None);
        assert_eq!(idx, fresh);
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
        assert!(idx
            .definitions_named("parse")
            .iter()
            .any(|d| d.kind == Kind::Function));
        assert!(idx.files().contains_key("a.rs"));
        // The unregistered file is never indexed.
        assert!(!idx.files().contains_key("b.txt"));
    }
}
