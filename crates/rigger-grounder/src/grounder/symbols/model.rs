//! The parser-free symbol data model (architecture 5.5.2). No `tree_sitter::` type appears
//! here: spans are plain integers, `kind` is a rigger-owned enum, and the whole module
//! compiles in a build that never links tree-sitter. That is the dependency-direction proof
//! the extractors, the code store's facts, and (spec 16) blast-radius all rely on.

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
/// tag categories fold to `Other`, so the model stays grammar-agnostic. `Function`, `Method`,
/// `Constructor` and `Macro` are the FUNCTION-LIKE kinds, a reference's first choice of caller;
/// a `Declaration` (a function, method or constructor with no body) is never a caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Kind {
    Function,
    Method,
    Type,
    Trait,
    Impl,
    Module,
    Constant,
    Constructor,
    Macro,
    Declaration,
    Static,
    Variant,
    Field,
    Variable,
    Property,
    Other,
}

/// A definition site: kind, name, 1-based line and 1-based end line. The span is plain integers,
/// never a `tree_sitter::Range` (dependency direction, 5.5.2).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Def {
    pub kind: Kind,
    pub name: String,
    pub line: u32,
    /// Spec 119 (ONE EXTENT): the 1-based line the definition's whole construct ends on, its body
    /// included - the row of the last byte of its byte range plus one, never below `line`, and
    /// `line` itself for a zero-length range ([`crate::grounder::symbols::extract::extract`] sets
    /// it on every definition).
    pub end_line: u32,
    /// Spec 119 (IDS FOLLOW THE LANGUAGE'S PATH): the definition's id path in its file - the path
    /// of the definitions enclosing it, `::`, its leaf (`.` before a field or property), `name`
    /// itself at file level - so its node is `<file>::<path>`
    /// ([`crate::grounder::symbols::extract::extract`] sets it on every definition).
    pub path: String,
    /// Spec 119 (CONTAINS IS STRUCTURE): the [`Def::path`] of the definition this one's
    /// `CONTAINS` comes from ([`crate::grounder::symbols::extract::extract`] sets it): its
    /// container's path - the innermost definition that alone strictly holds it, a span two
    /// definitions share holding nothing (in `var a, b = func() { type T struct{} }(), 1` the file
    /// holds `T`), or the C# file-scoped namespace a declaration after it sits in - but that a JS
    /// or TS named value takes its container's parent, being one definition with it, and a Go
    /// method the path of its receiver base type when a definition of the file sits there; `None`
    /// where that names no definition, the file containing it. Serde-defaulted and omitted when
    /// `None`; an index persisted before it existed is of an older [`GRAMMAR_TAGS_VERSION`], so it
    /// loads as absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
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
    /// consumed by the code store's resolver (the file's facts carry it as a module declaration),
    /// the ONE place a `mod name;` declaration's OWN file and every OTHER file's path are
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
    /// index never re-reads the source. Consumed by
    /// `symbols::paths::module_candidates`, which resolves it
    /// relative to the declaring file's own directory, or its module directory and inline chain
    /// when nested in inline mods ([`Self::enclosing_inline_module_path`], whose doc gives examples
    /// where that chain departs from rustc). `#[serde(default)]` for the same
    /// pre-86-index-compatibility reason as `is_test`/`is_out_of_line_module`; omitted from the
    /// wire form when `None` so the overwhelmingly common case stays byte-identical to before this
    /// field existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_override: Option<String>,
    /// Round 9 (`op-u86-c1-path-attribute-contract-is-rustc-s-and-unresolvable-never-excludes`):
    /// the `/`-joined chain of enclosing INLINE `mod name { .. }` names (outermost first) this
    /// out-of-line declaration sits nested inside, when any - the EXTRA directory-component chain
    /// its file resolves under, `#[path]` override or convention alike (spec 117 criterion 8), one
    /// level per enclosing inline module, on top of the declaring file's own module directory, so
    /// `mod outer { mod x; }` in `src/lib.rs` names `src/outer/x.rs`. Each level is the inline
    /// name as written, so the chain names rustc's directory only where that name is the
    /// directory. Among the departures: an inline parent with its own `#[path]` (under
    /// `#[path = "dir"] mod outer { .. }` rustc looks in `dir/`) and a raw identifier
    /// (`mod r#type { .. }`, where rustc reads `type/`). `None` when the declaration sits
    /// directly at the file's own top level (the overwhelmingly common case), in which case an
    /// override stays directory-of-file-relative exactly as before this field existed. Verified
    /// against real rustc for a plain inline parent
    /// (throwaway probe crates, not part of this tree): `mod outer { #[path = "foo.rs"] mod
    /// inner; }` in `src/lib.rs` resolves `foo.rs` against `src/outer/foo.rs`, and the identical
    /// nesting inside a LEAF file `src/parent.rs` resolves against `src/parent/outer/foo.rs` - the
    /// file's own MODULE directory (what a plain, non-overridden out-of-line sibling of
    /// `parent.rs` would already use) with `outer/`
    /// appended, never the file's bare directory. Captured
    /// structurally in [`crate::grounder::symbols::extract::extract`] alongside
    /// `is_out_of_line_module`/`path_override`, for the same reason: only the declaring file's own
    /// parsed tree can see its ancestor `mod_item` nodes. Consumed at the events/index layer
    /// ([`crate::grounder::symbols::paths::module_candidates`], both branches).
    /// `#[serde(default)]` for the same pre-round-9-index-compatibility reason as the other
    /// out-of-line fields; omitted from the wire form when `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enclosing_inline_module_path: Option<String>,
}

/// A reference site: the referenced name, its 1-based line, and the ENCLOSING definition the
/// reference occurs inside (the caller), named by its path ([`Def::path`], spec 119 criterion 8:
/// `T::new`, `outer::inner`). `None` for a top-level reference outside every
/// definition - an import or a module-level call; a file-level impl header's trait and type
/// attribute to the impl block, a definition of its own, and its trait's tagged `type` row to the
/// self type's definition when the file defines that once (spec 117). Set during extraction by
/// attributing the reference to the innermost function-like definition whose range contains it,
/// else the innermost definition of any kind but a declaration (spec 116). Serde-defaulted and
/// omitted when `None` so a pre-37 persisted index folds as caller-less and a caller-less ref
/// serializes byte-identically to before. `Default` is the bare reference a tagged one is pushed
/// from before attribution and the test-region pass fill its caller and test mark.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Spec 117 (REFERENCES): what kind of non-call reference this is - a const read, a type
    /// reference or an imported leaf of a `use`, the `EdgeInferred` payload's own spelling
    /// (`contextgraph::REF_KIND_CONST`, `REF_KIND_TYPE`, `REF_KIND_USE`) - extracted for Rust
    /// only; `None` for a call, every other language's reference included, which folds and counts
    /// as before the field existed. A calls-only consumer reads [`FileSymbols::calls`].
    /// Serde-defaulted and omitted when `None`, so a call serializes byte-identically to before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ref_kind: Option<String>,
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

impl FileSymbols {
    /// The file's CALL references, every reference with no [`SymRef::ref_kind`] (spec 117), in
    /// `refs` order: the one filter each calls-only consumer reads - the reference degree, the
    /// grounder's commonness, scoring and radius, and the test evidence - so no tagged reference
    /// moves a count, a rank or a radius.
    pub fn calls(&self) -> impl Iterator<Item = &SymRef> {
        self.refs.iter().filter(|r| r.ref_kind.is_none())
    }
}

/// The version of the grammar / tag-query set this build extracts with (architecture 5.5.3),
/// stamped into unit 3's `BlastRadiusComputed` audit event (via `Symbols::blast_radius_stamped`) so a
/// recorded radius names the tag-query generation that produced it, and part of the code store's
/// extractor fingerprint, so a snapshot written under another set is not trusted. Bump it when a
/// shipped grammar or an authored tags query changes, so a radius computed under an older grammar
/// set is distinguishable on replay from one the current set would produce (gap 104); so does a
/// change to what extraction records under an unchanged query, the caller a reference carries
/// (`SymRef::enclosing`) and a definition's parent (`Def::parent`) included: `ts-tags-v4` is
/// spec 119's one bump, and `ts-tags-v5` is spec 117's, at the tagged references
/// (`SymRef::ref_kind`: const reads, type references and the leaves a `use` imports) and the
/// test regions and impl-header callers they take. The registry's golden test
/// `an_extraction_change_ships_with_a_grammar_tags_version_bump` pins this tag beside the digest
/// of a fixed extraction, so a change to what extraction records fails it until the tag is bumped
/// and the pair re-pinned together. Parser-free, so it lives in the model both feature lanes
/// compile.
pub const GRAMMAR_TAGS_VERSION: &str = "ts-tags-v5";

/// The whole-project index, held in memory while an emit pass lowers it. Deterministic containers
/// only (`BTreeMap`): iterating it is stable across processes, unlike a `HashMap` whose
/// iteration order is per-process randomized.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SymbolIndex {
    /// rel-path -> that file's symbols.
    files: BTreeMap<String, FileSymbols>,
}

impl SymbolIndex {
    /// Insert (or replace) a file's symbols under its normalized relative path.
    pub fn insert_file(&mut self, rel_path: String, fs: FileSymbols) {
        self.files.insert(rel_path, fs);
    }

    /// Drop a file's symbols from the index (a no-op when no entry is held for `rel_path`). The
    /// per-file indexing path calls this when a file can no longer be read or extracted (it was
    /// deleted or became unreadable), so the index matches a fresh whole-tree `build_index`, which
    /// never visits the gone file. Parser-free by construction, so it stays in the light lane
    /// exactly like the rest of the model.
    pub fn remove_file(&mut self, rel_path: &str) {
        self.files.remove(rel_path);
    }

    /// The whole file map (rel-path -> symbols), for consumers that iterate the index.
    pub fn files(&self) -> &BTreeMap<String, FileSymbols> {
        &self.files
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
    fn data_model_api_is_tree_sitter_free_and_serdes_by_plain_types() {
        // The whole model API is composed of serde-able rigger types (integer spans, the
        // `Kind`/`Lang` enums) - no `tree_sitter::Range` (which is not `Serialize`) could
        // appear here or this round-trip would not compile. Together with this module
        // compiling in the tree-sitter-free `--no-default-features` lane, that is the
        // criterion-1 assertion that no tree-sitter type crosses into the data-model API.
        let file = FileSymbols {
            lang: Lang::Rust,
            defs: vec![Def {
                kind: Kind::Method,
                name: "apply_damage".into(),
                line: 7,
                end_line: 7,
                path: "apply_damage".into(),
                parent: None,
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
                ref_kind: None,
            }],
            partial: false,
        };
        let json = serde_json::to_string(&file).expect("model serializes with plain serde");
        let back: FileSymbols = serde_json::from_str(&json).expect("model round-trips");
        assert_eq!(back, file);
    }
}
