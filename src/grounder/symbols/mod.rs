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

/// The `symbols` grounder (unit 4): the `Grounder` port over the persisted index, ranking a
/// definition-name match above a reference above an incidental prose mention. It consumes the
/// gated extraction path (`build_index`/`index_one_file`), so it is confined to the `symbols`
/// feature; `main::select_grounder` wires it when the feature is built, and `grounder_for`
/// returns a loud error when it is not.
#[cfg(feature = "symbols")]
pub mod grounder;

// UNGATED: [`staleness`]/[`compare_to_tree`] below (spec 68) need these too, and stay ungated
// themselves - see their own docs - so the imports they share with the gated `build_index` path
// live here, unconditionally, rather than duplicated under a second `#[cfg(feature = "symbols")]`
// block (which would conflict with this one under a `symbols` build).
use crate::grounder::symbols::model::SymbolIndex;
use crate::grounder::walk_guarded;
use std::collections::BTreeSet;
use std::ops::ControlFlow;
use std::path::Path;

#[cfg(feature = "symbols")]
use crate::grounder::symbols::model::Lang;

/// Build the whole-project index over `root`: walk the tree with the SHARED scoped walk
/// (`walk_guarded`, the same walk grep and the ingests use, so they never diverge on
/// which files count), and for each file whose extension the registry resolves, extract its symbols
/// under its
/// normalized relative path. A file whose extension is unregistered is skipped; a file that
/// cannot be read, or whose parse recovers to no symbols, contributes whatever the tags run
/// produced and NEVER crashes the walk. `override_lang` forces one language for every file
/// (the `--language` override); `None` auto-detects per extension.
#[cfg(feature = "symbols")]
pub fn build_index(root: &str, override_lang: Option<Lang>) -> SymbolIndex {
    let mut idx = SymbolIndex::default();
    let _ = walk_guarded(Path::new(root), &mut |path| {
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
///
/// A file that is successfully indexed also has its CONTENT HASH recorded (spec 68), via
/// `store::content_hash` - the one hash primitive, the SAME one the live grounder's reindex
/// freshening gate hashes with - so `rigger validate`'s staleness advisory can later rehash a
/// small sample of the tree and diff it against these persisted values without a full-tree
/// rehash. A removed entry drops its hash too ([`SymbolIndex::remove_file`]).
#[cfg(feature = "symbols")]
pub fn index_one_file(root: &str, rel: &str, idx: &mut SymbolIndex, override_lang: Option<Lang>) {
    let Some(entry) = registry::for_path(rel, override_lang) else {
        return;
    };
    let abs = Path::new(root).join(rel);
    let Ok(src) = std::fs::read_to_string(&abs) else {
        idx.remove_file(rel);
        return;
    };
    match extract::extract(&src, entry.lang, &entry.language, entry.tags_query) {
        Ok(fs) => {
            idx.set_hash(rel.to_string(), store::content_hash(&src));
            idx.insert_file(rel.to_string(), fs);
        }
        Err(_) => idx.remove_file(rel),
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

/// What comparing the persisted index against the current tree found - the evidence `rigger
/// validate`'s INDEX STALENESS advisory (spec 68, Design) warns from. An empty `IndexDrift`
/// (every field empty) means the index and the tree agree, as far as [`staleness`]'s
/// cost-bounded check can tell.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IndexDrift {
    /// Paths in the current tree (at an extension the index already covers) that the persisted
    /// index does not yet hold - files added since the index was last built.
    pub added: Vec<String>,
    /// Paths the persisted index still holds that no longer exist in the current tree - files
    /// deleted since the index was last built.
    pub removed: Vec<String>,
    /// Sampled paths present in BOTH sets whose CURRENT content hash disagrees with the hash
    /// the index recorded for them - the content drift a path-set comparison alone cannot see
    /// (an edit that neither adds nor removes a file).
    pub changed: Vec<String>,
}

impl IndexDrift {
    /// Whether this comparison found any disagreement at all.
    pub fn is_stale(&self) -> bool {
        !self.added.is_empty() || !self.removed.is_empty() || !self.changed.is_empty()
    }
}

/// The pure comparison core of the index-staleness check (spec 68): given the persisted
/// `index`, the CURRENT tree's paths at the extensions the index already covers (gathered by
/// the caller - [`staleness`] is the one production caller), and a small deterministic SAMPLE
/// of `(rel_path, current content_hash)` pairs for paths present in both sets, report the
/// disagreement. A sampled path the index never recorded a hash for (an index persisted before
/// [`SymbolIndex::set_hash`] existed) is never flagged as changed - [`SymbolIndex::hash_for`]
/// answers `None`, and there is nothing honest to compare that `None` against.
pub fn compare_to_tree(
    index: &SymbolIndex,
    tree_paths: &BTreeSet<String>,
    sample: &[(String, String)],
) -> IndexDrift {
    let indexed: BTreeSet<String> = index.files().keys().cloned().collect();
    let added: Vec<String> = tree_paths.difference(&indexed).cloned().collect();
    let removed: Vec<String> = indexed.difference(tree_paths).cloned().collect();
    let changed: Vec<String> = sample
        .iter()
        .filter(|(p, hash)| index.hash_for(p).is_some_and(|h| h != hash))
        .map(|(p, _)| p.clone())
        .collect();
    IndexDrift {
        added,
        removed,
        changed,
    }
}

/// How many files' current content [`staleness`] rehashes to check for drift, at most (spec 68,
/// Design: "path-set comparison plus existing content hashes of a SMALL DETERMINISTIC sample,
/// never a full-tree rehash"). Deterministic because the sample is drawn from the sorted
/// intersection of two `BTreeSet`s, so the SAME tree state yields the SAME sample on every run -
/// never a randomly-chosen subset that would make one `rigger validate` flag drift another run
/// over the identical tree does not.
const STALENESS_SAMPLE_SIZE: usize = 8;

/// `rigger validate`'s INDEX STALENESS check (spec 68, VALIDATE ADVISORIES): compare the
/// `symbols` index persisted at `root` against the CURRENT tree, cost-bounded exactly as the
/// Design specifies:
/// - a path-set comparison restricted to the extensions the persisted index ALREADY covers -
///   never a scan for every extension the tree-sitter registry could resolve, which is why this
///   function (and [`compare_to_tree`]) stay UNGATED: they need no grammar, so this check runs
///   identically with or without the `symbols` feature;
/// - a REHASH of only a small deterministic sample ([`STALENESS_SAMPLE_SIZE`]) of files already
///   present in both the index and the tree, via [`store::content_hash`] - the SAME primitive
///   the live grounder's reindex freshening gate hashes with (one measurement authority) -
///   never a full-tree rehash.
///
/// `None` when there is nothing to warn about: no persisted index (nothing ever built, or an
/// unreadable/corrupt file - there is nothing honest to compare against), or a comparison that
/// found no disagreement.
pub fn staleness(root: &str) -> Option<IndexDrift> {
    let idx = store::load(root)?;
    // The extensions the index already covers - derived from the index itself, never from the
    // tree-sitter registry, which is exactly what keeps this function ungated.
    let extensions: BTreeSet<String> = idx
        .files()
        .keys()
        .filter_map(|p| {
            Path::new(p)
                .extension()
                .map(|e| e.to_string_lossy().into_owned())
        })
        .collect();
    if extensions.is_empty() {
        // Nothing persisted to compare an extension against - an empty index is never "stale".
        return None;
    }
    let root_path = Path::new(root);
    let mut tree_paths: BTreeSet<String> = BTreeSet::new();
    let _ = walk_guarded(root_path, &mut |path| {
        let matches = path
            .extension()
            .map(|e| e.to_string_lossy().into_owned())
            .is_some_and(|ext| extensions.contains(&ext));
        if matches {
            let rel = path
                .strip_prefix(root_path)
                .unwrap_or(path)
                .to_string_lossy()
                .into_owned();
            tree_paths.insert(rel);
        }
        ControlFlow::Continue(())
    });
    let indexed: BTreeSet<String> = idx.files().keys().cloned().collect();
    // The sample: the sorted INTERSECTION's first `STALENESS_SAMPLE_SIZE` paths (deterministic -
    // see the constant's docs), each rehashed from its CURRENT content. A file that cannot be
    // read right now (a race with a delete) is simply left out of the sample rather than
    // manufacturing a spurious "changed" - the path-set comparison already covers a genuine
    // deletion via `removed`.
    let sample: Vec<(String, String)> = indexed
        .intersection(&tree_paths)
        .take(STALENESS_SAMPLE_SIZE)
        .filter_map(|rel| {
            let content = std::fs::read_to_string(root_path.join(rel)).ok()?;
            Some((rel.clone(), store::content_hash(&content)))
        })
        .collect();
    let drift = compare_to_tree(&idx, &tree_paths, &sample);
    drift.is_stale().then_some(drift)
}

#[cfg(test)]
mod staleness_tests {
    use super::*;
    use crate::grounder::symbols::model::{FileSymbols, Kind};

    /// Persist an index directly (no tree-sitter needed - this whole check is ungated), one
    /// entry per `(path, hash)`, each carrying a single trivial def so `files()` is non-empty.
    fn persist(root: &str, entries: &[(&str, &str)]) {
        let mut idx = SymbolIndex::default();
        for (path, hash) in entries {
            idx.insert_file(
                (*path).to_string(),
                FileSymbols {
                    lang: crate::grounder::symbols::model::Lang::Rust,
                    defs: vec![crate::grounder::symbols::model::Def {
                        kind: Kind::Function,
                        name: "f".into(),
                        line: 1,
                    }],
                    refs: vec![],
                },
            );
            idx.set_hash((*path).to_string(), (*hash).to_string());
        }
        store::save(&idx, root).unwrap();
    }

    #[test]
    fn no_persisted_index_is_never_stale() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        assert_eq!(staleness(root), None);
    }

    #[test]
    fn an_unchanged_tree_matching_the_index_is_not_stale() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let content = "fn one() {}\n";
        std::fs::write(dir.path().join("a.rs"), content).unwrap();
        persist(root, &[("a.rs", &store::content_hash(content))]);
        assert_eq!(staleness(root), None);
    }

    #[test]
    fn a_file_added_to_the_tree_since_the_index_was_built_is_flagged() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let content = "fn one() {}\n";
        std::fs::write(dir.path().join("a.rs"), content).unwrap();
        std::fs::write(dir.path().join("b.rs"), "fn two() {}\n").unwrap();
        persist(root, &[("a.rs", &store::content_hash(content))]);
        let drift = staleness(root).expect("a new file must be flagged as drift");
        assert_eq!(drift.added, vec!["b.rs".to_string()]);
        assert!(drift.removed.is_empty());
    }

    #[test]
    fn a_file_removed_from_the_tree_since_the_index_was_built_is_flagged() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let content = "fn one() {}\n";
        std::fs::write(dir.path().join("a.rs"), content).unwrap();
        persist(
            root,
            &[
                ("a.rs", &store::content_hash(content)),
                ("gone.rs", &store::content_hash("fn gone() {}\n")),
            ],
        );
        let drift = staleness(root).expect("a deleted file must be flagged as drift");
        assert_eq!(drift.removed, vec!["gone.rs".to_string()]);
        assert!(drift.added.is_empty());
    }

    #[test]
    fn edited_content_with_no_path_change_is_flagged_via_the_sample_rehash() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        // The index was built over the ORIGINAL content...
        persist(root, &[("a.rs", &store::content_hash("fn one() {}\n"))]);
        // ...but the file on disk has since changed - same path, different bytes.
        std::fs::write(dir.path().join("a.rs"), "fn onemodified() {}\n").unwrap();
        let drift = staleness(root).expect("edited content must be flagged as drift");
        assert_eq!(drift.changed, vec!["a.rs".to_string()]);
        assert!(drift.added.is_empty());
        assert!(drift.removed.is_empty());
    }

    #[test]
    fn a_path_the_index_never_recorded_a_hash_for_is_never_flagged_as_changed() {
        // Simulates an index persisted before the hash field existed (or a hash that was, for
        // any reason, never recorded): `hash_for` answers `None`, and `compare_to_tree` must
        // never treat a missing "before" value as evidence of change.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        std::fs::write(dir.path().join("a.rs"), "fn one() {}\n").unwrap();
        let mut idx = SymbolIndex::default();
        idx.insert_file(
            "a.rs".to_string(),
            FileSymbols {
                lang: crate::grounder::symbols::model::Lang::Rust,
                defs: vec![],
                refs: vec![],
            },
        );
        // Deliberately no `set_hash` call.
        store::save(&idx, root).unwrap();
        assert_eq!(staleness(root), None);
    }

    #[test]
    fn the_sample_is_capped_and_deterministic() {
        // Persist more files than STALENESS_SAMPLE_SIZE, all unchanged; staleness must still
        // report no drift - the sample is bounded, not a full-tree rehash, and every unchanged
        // file it happens to draw agrees.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let mut entries: Vec<(String, String)> = Vec::new();
        for i in 0..(STALENESS_SAMPLE_SIZE * 2) {
            let name = format!("f{i}.rs");
            let content = format!("fn f{i}() {{}}\n");
            std::fs::write(dir.path().join(&name), &content).unwrap();
            entries.push((name, store::content_hash(&content)));
        }
        let refs: Vec<(&str, &str)> = entries
            .iter()
            .map(|(p, h)| (p.as_str(), h.as_str()))
            .collect();
        persist(root, &refs);
        assert_eq!(staleness(root), None);
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
    fn index_one_file_records_the_content_hash_alongside_the_symbols() {
        // spec 68: a successfully indexed file's content hash is persisted alongside its
        // symbols, keyed by store::content_hash - the ONE hash primitive - so `rigger
        // validate`'s staleness advisory can rehash a small sample later and diff against it.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        std::fs::write(dir.path().join("a.rs"), "fn one() {}\n").unwrap();
        let idx = build_index(root, None);
        assert_eq!(
            idx.hash_for("a.rs"),
            Some(store::content_hash("fn one() {}\n").as_str())
        );

        // Reindexing after content changes updates the recorded hash to match.
        let mut idx = idx;
        std::fs::write(dir.path().join("a.rs"), "fn oneprime() {}\n").unwrap();
        reindex_files(root, &mut idx, &["a.rs".into()], None);
        assert_eq!(
            idx.hash_for("a.rs"),
            Some(store::content_hash("fn oneprime() {}\n").as_str())
        );

        // A deleted file's hash is dropped along with its symbols.
        std::fs::remove_file(dir.path().join("a.rs")).unwrap();
        reindex_files(root, &mut idx, &["a.rs".into()], None);
        assert_eq!(idx.hash_for("a.rs"), None);
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
