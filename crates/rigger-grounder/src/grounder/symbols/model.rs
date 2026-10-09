//! The parser-free symbol data model (architecture 5.5.2). No `tree_sitter::` type appears
//! here: spans are plain integers, `kind` is a rigger-owned enum, and the whole module
//! compiles in a build that never links tree-sitter. That is the dependency-direction proof
//! the grounder, persistence, and (spec 16) blast-radius all rely on.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The languages the registry can extract. A rigger-owned enum so the model never names a
/// tree-sitter type; per-language scoping keys the cross-reference graph on it (a `parse` in
/// a `.rs` file never links one in a `.py` file, 5.5.2). `Hash` (spec 92 criterion 3
/// remediation round 5) so `(name, Lang)` can key a `HashSet`/`HashMap` - the entity-resolution
/// scoping `grounder.rs` needs to stop conflating same-named entities across languages.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Lang {
    Rust,
    CSharp,
    Js,
    Ts,
    Go,
    Python,
}

/// The kind of a definition. A rigger enum, not a tree-sitter syntax-type id. Unknown grammar
/// tag categories fold to `Other`, so the model stays grammar-agnostic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Kind {
    Function,
    Method,
    Type,
    Trait,
    Impl,
    Module,
    Constant,
    Other,
}

/// A definition site: kind, name, and 1-based line. The span is a plain integer, never a
/// `tree_sitter::Range` (dependency direction, 5.5.2).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Def {
    pub kind: Kind,
    pub name: String,
    pub line: u32,
    /// Spec 86 criterion 1: whether this definition is TEST code - directly annotated
    /// `#[test]`/`#[cfg(test)]` (or any `cfg(...)` predicate naming the `test` token, e.g.
    /// `#[cfg(all(test, feature = "x"))]`), or nested inside such a definition (a plain helper
    /// `fn` inside a `#[cfg(test)] mod tests { .. }` with no attribute of its own). Computed ONCE
    /// during extraction ([`crate::grounder::symbols::extract::extract`]) from the source text
    /// and byte ranges, which are NOT available downstream (a reused persisted index never
    /// re-reads the source), so it must be carried on the definition itself rather than
    /// re-derived later. The code-entity EMIT pass reads it to exclude test code from graph NODE
    /// creation; the parser-free model here still records it - "still PARSED" - so grounding and
    /// the reference-degree primitive stays UNCHANGED (this field adds information, it drops
    /// nothing). `#[serde(default)]` so a pre-86 persisted index loads with every definition
    /// `is_test: false` (the safe default - never manufacturing a false exclusion of old data),
    /// and the false (overwhelmingly common) case serializes with no key at all, byte-identical
    /// to the pre-86 wire form.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_test: bool,
    /// Round 6 (`op-u86c1-r5-close-every-remaining-test-shape` item 2): whether this `Module`-kind
    /// definition is an OUT-OF-LINE declaration - Rust's `mod name;` form, which has no body of its
    /// own and instead names a SEPARATE file the compiler resolves by its own file-per-module
    /// convention. `mod name { .. }` (an INLINE module WITH a body) is never this - any
    /// `#[cfg(test)]` on it already governs its own nested definitions directly, by containment
    /// (`is_test`/`extract::test_regions`), with no cross-file resolution needed. Always `false` for
    /// every non-`Module` kind, which never declares a file this way. Computed structurally in
    /// [`crate::grounder::symbols::extract::extract`] (the node's own `body` field is absent iff the
    /// declaration is out-of-line - a fact only the parsed tree, not the tags pass, can see), and
    /// consumed at the events/index layer ([`crate::grounder::symbols::events::extract_events`]'s
    /// module), the ONE place a `mod name;` declaration's OWN file and every OTHER file's path are
    /// both already known, to resolve which file `name` names and exclude it wholesale when the
    /// declaration is also `is_test` - the per-file extractor can never see that attribute itself,
    /// since it lives in a DIFFERENT file's tree entirely. `#[serde(default)]` for the same
    /// pre-86-index-compatibility reason as `is_test`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_out_of_line_module: bool,
    /// Round 7 (`op-u86c1-r7-out-of-line-module-resolution-follows-rust`): the string value of a
    /// `#[path = ".."]` attribute directly governing this `Module`-kind out-of-line declaration,
    /// when one is present - rustc's own escape hatch from the file-per-module convention, taking
    /// precedence over it entirely. `None` when no `#[path]` attribute governs this declaration
    /// (the overwhelmingly common case) or the definition is not an out-of-line module at all.
    /// Captured structurally in [`crate::grounder::symbols::extract::extract`] alongside
    /// `is_out_of_line_module`, for the SAME reason: only the declaring file's own parsed tree
    /// carries the attribute, so it must be carried on the definition since a reused persisted
    /// index never re-reads the source. Consumed at the events/index layer
    /// ([`crate::grounder::symbols::events`]'s `out_of_line_test_module_files`), which resolves it
    /// relative to the declaring file's own directory. `#[serde(default)]` for the same
    /// pre-86-index-compatibility reason as `is_test`/`is_out_of_line_module`; omitted from the
    /// wire form when `None` so the overwhelmingly common case stays byte-identical to before this
    /// field existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_override: Option<String>,
    /// Round 9 (`op-u86-c1-path-attribute-contract-is-rustc-s-and-unresolvable-never-excludes`):
    /// the `/`-joined chain of enclosing INLINE `mod name { .. }` names (outermost first) this
    /// out-of-line declaration sits nested inside, when any - the EXTRA directory-component chain
    /// a `#[path]` override on it resolves under, one level per enclosing inline module, on top of
    /// the declaring file's own module directory. `None` when the declaration sits directly at the
    /// file's own top level (the overwhelmingly common case), in which case the override stays
    /// directory-of-file-relative exactly as before this field existed. Verified against real
    /// rustc (throwaway probe crates, not part of this tree): `mod outer { #[path = "foo.rs"] mod
    /// inner; }` in `src/lib.rs` resolves `foo.rs` against `src/outer/foo.rs`, and the identical
    /// nesting inside a LEAF file `src/parent.rs` resolves against `src/parent/outer/foo.rs` - the
    /// file's own MODULE directory (what a plain, non-overridden out-of-line sibling of `parent.rs`
    /// would already use) with `outer/` appended, never the file's bare directory. Captured
    /// structurally in [`crate::grounder::symbols::extract::extract`] alongside
    /// `is_out_of_line_module`/`path_override`, for the same reason: only the declaring file's own
    /// parsed tree can see its ancestor `mod_item` nodes. Consumed at the events/index layer
    /// ([`crate::grounder::symbols::events::resolve_out_of_line_target`]'s `#[path]`-override
    /// branch). `#[serde(default)]` for the same pre-round-9-index-compatibility reason as the
    /// other out-of-line fields; omitted from the wire form when `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enclosing_inline_module_path: Option<String>,
}

/// A reference site: the referenced name, its 1-based line, and the ENCLOSING definition the
/// reference occurs inside (the caller). `None` for a top-level reference outside every
/// definition - an import or a module-level call; an impl header's trait and type attribute
/// to the impl block, a definition of its own. Set during extraction by attributing the reference
/// to the innermost definition whose body contains it (spec 37). Serde-defaulted and omitted when
/// `None` so a pre-37 persisted index folds as caller-less and a caller-less ref serializes
/// byte-identically to before.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymRef {
    pub name: String,
    pub line: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enclosing: Option<String>,
    /// Spec 86 criterion 1, the reference-side twin of [`Def::is_test`]: whether this reference
    /// occurs INSIDE a test region (a `#[test]` function, a `#[cfg(test)]` module, or anything
    /// nested inside either). Computed the same way, at the same time, over the same byte ranges.
    /// The code-entity emit pass reads it to exclude a test-scoped reference from becoming a
    /// structural edge "on the canvas"; serde-defaulted and omitted when false for the same
    /// byte-identical-wire-form reason as `Def::is_test`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_test: bool,
}

/// One file's extracted symbols, tagged with the language it was parsed as (the scope key for
/// the cross-reference graph).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileSymbols {
    pub lang: Lang,
    pub defs: Vec<Def>,
    pub refs: Vec<SymRef>,
    /// Spec 92 criterion 2 round 4 (review REJECT `adj-u2c2-r3-verdict-reject`, finding
    /// `adv-u2c2-partial-marker-unimplemented`): whether tree-sitter's parse of this file's
    /// source contained an ERROR node - `tree_sitter::Node::has_error()` on the parsed root -
    /// meaning the grammar could not fully parse it and `defs`/`refs` above only cover as far as
    /// the parse reached, never a guarantee of completeness. Computed once during extraction
    /// ([`crate::grounder::symbols::extract::extract`]) from the SAME parsed tree
    /// `test_regions` already walks there (never a second parse). `#[serde(default)]` so an
    /// index persisted before this field existed loads with every file `partial: false` - the
    /// safe default, never manufacturing a false degraded marker for old data - and the
    /// overwhelmingly common well-formed case serializes with no key at all, byte-identical to
    /// before this field existed.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub partial: bool,
}

/// The version of the grammar / tag-query set this build indexes with (architecture 5.5.3),
/// stamped into unit 3's `BlastRadiusComputed` audit event (via `Symbols::index_stamp`) so a
/// recorded radius names the tag-query generation that produced it. Bump it when a shipped
/// grammar or an authored tags query changes, so a radius computed under an older grammar set
/// is distinguishable on replay from one the current set would produce, and so a persisted
/// [`SymbolIndex`] built under an older set loads as absent and is rebuilt (gap 104). Parser-free,
/// so it lives in the model both feature lanes compile.
pub const GRAMMAR_TAGS_VERSION: &str = "ts-tags-v2";

/// The whole-project index. Deterministic containers only (`BTreeMap`): iterating it for
/// serialization is stable across processes, unlike a `HashMap` whose iteration order is
/// per-process randomized. That determinism-by-construction is what unit 3's persistence
/// relies on, so the choice is made here, in the shared model.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolIndex {
    /// The extraction generation ([`GRAMMAR_TAGS_VERSION`]) this index was built under. A new
    /// index carries the current one; an index persisted before the field existed deserializes
    /// with none, which is never current.
    #[serde(default)]
    grammar: String,
    /// rel-path -> that file's symbols.
    files: BTreeMap<String, FileSymbols>,
    /// rel-path -> the content hash (`symbols::store::content_hash`) of that file's source AT
    /// THE TIME it was last indexed. Populated by `index_one_file` alongside `files` (spec 68),
    /// never computed here - the model stays hash-algorithm-agnostic and never depends on
    /// `store` (dependency direction: `store` is a projection OF the model, never the reverse).
    /// `rigger validate`'s index-staleness advisory rehashes a small deterministic sample of
    /// the CURRENT tree and diffs it against these persisted values, so genuine content drift
    /// is detected without a full-tree rehash. `#[serde(default)]` so an index persisted before
    /// this field existed still loads - as empty, which [`SymbolIndex::hash_for`] reports as
    /// "unknown" rather than "unchanged" or "changed", so a pre-upgrade index never manufactures
    /// a false drift signal.
    #[serde(default)]
    hashes: BTreeMap<String, String>,
}

impl Default for SymbolIndex {
    /// An empty index of the current extraction generation.
    fn default() -> Self {
        SymbolIndex {
            grammar: GRAMMAR_TAGS_VERSION.to_string(),
            files: BTreeMap::new(),
            hashes: BTreeMap::new(),
        }
    }
}

impl SymbolIndex {
    /// Whether this index was built under the current extraction generation - false for one
    /// persisted by a build whose grammar or tags query has since changed, whose symbols the
    /// current extractor would not reproduce.
    pub fn is_current(&self) -> bool {
        self.grammar == GRAMMAR_TAGS_VERSION
    }

    /// Insert (or replace) a file's symbols under its normalized relative path.
    pub fn insert_file(&mut self, rel_path: String, fs: FileSymbols) {
        self.files.insert(rel_path, fs);
    }

    /// Drop a file's symbols from the index (a no-op when no entry is held for `rel_path`). The
    /// incremental reindex path calls this when a changed file can no longer be read or extracted
    /// (it was deleted or became unreadable), so the freshened index matches a fresh whole-tree
    /// `build_index`, which never visits the gone file: a stale definition or reference from a
    /// removed file must not keep grounding. Parser-free by construction, so it stays in the light
    /// lane exactly like the rest of the model. Drops that file's recorded content hash too
    /// (spec 68), so a removed file can never survive as a stale hash entry with no symbols
    /// behind it.
    pub fn remove_file(&mut self, rel_path: &str) {
        self.files.remove(rel_path);
        self.hashes.remove(rel_path);
    }

    /// Insert (or replace) a file's symbols together with its content hash (spec 68) - the
    /// caller computes the hash (via `symbols::store::content_hash`, the one hash primitive)
    /// and hands it in, so the model itself never depends on `store`.
    pub fn insert_hashed_file(&mut self, rel_path: String, fs: FileSymbols, hash: String) {
        self.hashes.insert(rel_path.clone(), hash);
        self.insert_file(rel_path, fs);
    }

    /// The content hash last recorded for `rel_path`, or `None` when never recorded (an index
    /// persisted before this field existed, or a file this index never indexed).
    pub fn hash_for(&self, rel_path: &str) -> Option<&str> {
        self.hashes.get(rel_path).map(String::as_str)
    }

    /// The whole file map (rel-path -> symbols), for consumers that iterate the index.
    pub fn files(&self) -> &BTreeMap<String, FileSymbols> {
        &self.files
    }

    /// How many references name `name` WITHIN `lang` - the per-language fan-out degree an entity
    /// row reports. SCOPED to `lang`: the cross-reference graph is per-language (5.5.2), so the
    /// degree over it is too. A name that over-links in another language never inflates this
    /// count (a Python `parse` leaves the Rust `parse` degree untouched).
    pub fn reference_degree(&self, name: &str, lang: Lang) -> usize {
        self.files
            .values()
            .filter(|f| f.lang == lang)
            .flat_map(|f| f.refs.iter())
            .filter(|r| r.name == name)
            .count()
    }
}

/// The nearest-rank percentile cutoff over a distribution of per-name counts (spec 92 criterion
/// 3): the cutoff the `symbols` grounder's tree-wide-ambiguity gate draws from. Sorts `counts` in
/// place and returns the value at 0-based rank `floor((counts.len() - 1) * percentile)` - always
/// in bounds for a non-empty slice, so no clamp is needed. Every caller already special-cases an
/// empty distribution (there IS no cutoff over zero names), so this panics on an empty `counts`
/// rather than silently returning a meaningless default.
pub fn percentile_cutoff(counts: &mut [usize], percentile: f64) -> usize {
    assert!(
        !counts.is_empty(),
        "percentile_cutoff requires a non-empty distribution; callers must special-case empty"
    );
    counts.sort_unstable();
    let idx = (((counts.len() - 1) as f64) * percentile).floor() as usize;
    counts[idx]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_degree_counts_every_file_s_references_to_a_name() {
        let mut idx = SymbolIndex::default();
        // `new` is referenced in many files; `apply_damage` in one.
        for i in 0..20 {
            idx.insert_file(
                format!("f{i}.rs"),
                FileSymbols {
                    lang: Lang::Rust,
                    defs: vec![],
                    refs: vec![SymRef {
                        name: "new".into(),
                        line: 1,
                        enclosing: None,
                        is_test: false,
                    }],
                    partial: false,
                },
            );
        }
        idx.insert_file(
            "combat.rs".into(),
            FileSymbols {
                lang: Lang::Rust,
                defs: vec![Def {
                    kind: Kind::Function,
                    name: "apply_damage".into(),
                    line: 1,
                    is_test: false,
                    is_out_of_line_module: false,
                    path_override: None,
                    enclosing_inline_module_path: None,
                }],
                refs: vec![SymRef {
                    name: "apply_damage".into(),
                    line: 2,
                    enclosing: None,
                    is_test: false,
                }],
                partial: false,
            },
        );
        assert_eq!(idx.reference_degree("new", Lang::Rust), 20);
        assert_eq!(idx.reference_degree("apply_damage", Lang::Rust), 1);
        assert_eq!(idx.reference_degree("absent", Lang::Rust), 0);
    }

    #[test]
    fn reference_degree_is_language_scoped_no_cross_language_inflation() {
        // The exact cross-language collision per-language scoping exists to prevent (5.5.2):
        // the name `parse` is referenced ONCE in Rust but TWENTY times in Python. A
        // language-BLIND degree would report 21 for the Rust `parse` purely from Python usage.
        // Per-language scoping must keep the two graphs disjoint.
        let mut idx = SymbolIndex::default();
        for i in 0..20 {
            idx.insert_file(
                format!("py/f{i}.py"),
                FileSymbols {
                    lang: Lang::Python,
                    defs: vec![],
                    refs: vec![SymRef {
                        name: "parse".into(),
                        line: 1,
                        enclosing: None,
                        is_test: false,
                    }],
                    partial: false,
                },
            );
        }
        // Rust references `parse` once and `new` ten times.
        idx.insert_file(
            "a.rs".into(),
            FileSymbols {
                lang: Lang::Rust,
                defs: vec![],
                refs: vec![SymRef {
                    name: "parse".into(),
                    line: 3,
                    enclosing: None,
                    is_test: false,
                }],
                partial: false,
            },
        );
        for i in 0..10 {
            idx.insert_file(
                format!("rs/f{i}.rs"),
                FileSymbols {
                    lang: Lang::Rust,
                    defs: vec![],
                    refs: vec![SymRef {
                        name: "new".into(),
                        line: 1,
                        enclosing: None,
                        is_test: false,
                    }],
                    partial: false,
                },
            );
        }
        // Degree is per-language: the 20 Python refs do NOT inflate the Rust `parse` degree,
        // and vice versa.
        assert_eq!(idx.reference_degree("parse", Lang::Rust), 1);
        assert_eq!(idx.reference_degree("parse", Lang::Python), 20);
        assert_eq!(idx.reference_degree("new", Lang::Rust), 10);
    }

    #[test]
    fn data_model_api_is_tree_sitter_free_and_serdes_by_plain_types() {
        // The whole model API is composed of serde-able rigger types (integer spans, the
        // `Kind`/`Lang` enums) - no `tree_sitter::Range` (which is not `Serialize`) could
        // appear here or this round-trip would not compile. Together with this module
        // compiling in the tree-sitter-free `--no-default-features` lane, that is the
        // criterion-1 assertion that no tree-sitter type crosses into the data-model API.
        let mut idx = SymbolIndex::default();
        idx.insert_file(
            "combat.rs".into(),
            FileSymbols {
                lang: Lang::Rust,
                defs: vec![Def {
                    kind: Kind::Method,
                    name: "apply_damage".into(),
                    line: 7,
                    is_test: false,
                    is_out_of_line_module: false,
                    path_override: None,
                    enclosing_inline_module_path: None,
                }],
                refs: vec![SymRef {
                    name: "clamp".into(),
                    line: 9,
                    enclosing: None,
                    is_test: false,
                }],
                partial: false,
            },
        );
        let json = serde_json::to_string(&idx).expect("model serializes with plain serde");
        let back: SymbolIndex = serde_json::from_str(&json).expect("model round-trips");
        assert_eq!(back, idx);
    }

    #[test]
    fn content_hash_is_recorded_and_dropped_with_its_file() {
        // spec 68: a hash recorded via `insert_hashed_file` is readable via `hash_for`; a path never
        // recorded is `None` (unknown - never misread as "unchanged").
        let mut idx = SymbolIndex::default();
        assert_eq!(idx.hash_for("a.rs"), None);
        idx.insert_hashed_file(
            "a.rs".into(),
            FileSymbols {
                lang: Lang::Rust,
                defs: vec![],
                refs: vec![],
                partial: false,
            },
            "deadbeef".into(),
        );
        assert_eq!(idx.hash_for("a.rs"), Some("deadbeef"));
        // `remove_file` drops the recorded hash along with the symbols, so a removed file can
        // never survive as a stale hash entry with no symbols behind it.
        idx.remove_file("a.rs");
        assert_eq!(idx.hash_for("a.rs"), None);
    }

    #[test]
    fn a_pre_upgrade_index_with_no_hashes_field_loads_with_every_hash_unknown() {
        // An index persisted before this field existed serializes with no `hashes` key at all;
        // `#[serde(default)]` must still load it, with every `hash_for` answering `None` -
        // never manufacturing a false "changed" or "unchanged" signal for pre-existing data.
        let mut idx = SymbolIndex::default();
        idx.insert_file(
            "a.rs".into(),
            FileSymbols {
                lang: Lang::Rust,
                defs: vec![],
                refs: vec![],
                partial: false,
            },
        );
        let mut json: serde_json::Value = serde_json::to_value(&idx).unwrap();
        json.as_object_mut().unwrap().remove("hashes");
        let loaded: SymbolIndex = serde_json::from_value(json).unwrap();
        assert_eq!(loaded.hash_for("a.rs"), None);
        assert!(loaded.files().contains_key("a.rs"));
    }
}
