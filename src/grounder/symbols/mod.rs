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
/// whole-tree `build_index` and unit 3's incremental `reindex_files` both freshen a file
/// through here, so a file is indexed identically whether the whole tree is built or one file
/// is re-parsed. An unresolved extension, an unreadable file, or a parse that recovers to no
/// symbols each leaves `idx` untouched for that file rather than crashing.
#[cfg(feature = "symbols")]
pub fn index_one_file(root: &str, rel: &str, idx: &mut SymbolIndex, override_lang: Option<Lang>) {
    let Some(entry) = registry::for_path(rel, override_lang) else {
        return;
    };
    let abs = Path::new(root).join(rel);
    if let Ok(src) = std::fs::read_to_string(&abs) {
        if let Ok(fs) = extract::extract(&src, entry.lang, &entry.language, entry.tags_query) {
            idx.insert_file(rel.to_string(), fs);
        }
    }
}

#[cfg(test)]
#[cfg(feature = "symbols")]
mod tests {
    use super::*;
    use crate::grounder::symbols::model::Kind;

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
