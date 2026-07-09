//! The parser-free symbol data model (architecture 5.5.2). No `tree_sitter::` type appears
//! here: spans are plain integers, `kind` is a rigger-owned enum, and the whole module
//! compiles in a build that never links tree-sitter. That is the dependency-direction proof
//! the grounder, persistence, and (spec 16) blast-radius all rely on.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The languages the registry can extract. A rigger-owned enum so the model never names a
/// tree-sitter type; per-language scoping keys the cross-reference graph on it (a `parse` in
/// a `.rs` file never links one in a `.py` file, 5.5.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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
}

/// A reference site: the referenced name and its 1-based line.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymRef {
    pub name: String,
    pub line: u32,
}

/// One file's extracted symbols, tagged with the language it was parsed as (the scope key for
/// the cross-reference graph).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileSymbols {
    pub lang: Lang,
    pub defs: Vec<Def>,
    pub refs: Vec<SymRef>,
}

/// The whole-project index. Deterministic containers only (`BTreeMap`): iterating it for
/// serialization is stable across processes, unlike a `HashMap` whose iteration order is
/// per-process randomized. That determinism-by-construction is what unit 3's persistence
/// relies on, so the choice is made here, in the shared model.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolIndex {
    /// rel-path -> that file's symbols.
    files: BTreeMap<String, FileSymbols>,
}

impl SymbolIndex {
    /// Insert (or replace) a file's symbols under its normalized relative path.
    pub fn insert_file(&mut self, rel_path: String, fs: FileSymbols) {
        self.files.insert(rel_path, fs);
    }

    /// Every definition whose name equals `name`, across languages. Grounding wants precision,
    /// so the cross-language reach is fine here; references are the language-scoped view.
    pub fn definitions_named(&self, name: &str) -> Vec<&Def> {
        self.files
            .values()
            .flat_map(|f| f.defs.iter())
            .filter(|d| d.name == name)
            .collect()
    }

    /// `(file, reference)` pairs referencing `name`, SCOPED to `lang`: a Rust reference never
    /// links a Python definition (the cross-language-collision fix, 5.5.2).
    pub fn references_named(&self, name: &str, lang: Lang) -> Vec<(&str, &SymRef)> {
        self.files
            .iter()
            .filter(|(_, f)| f.lang == lang)
            .flat_map(|(p, f)| f.refs.iter().map(move |r| (p.as_str(), r)))
            .filter(|(_, r)| r.name == name)
            .collect()
    }

    /// The whole file map (rel-path -> symbols), for consumers that iterate the index.
    pub fn files(&self) -> &BTreeMap<String, FileSymbols> {
        &self.files
    }

    /// How many references (across all files and languages) name `name`. This is the raw
    /// fan-out the suppression threshold is computed against.
    pub fn reference_degree(&self, name: &str) -> usize {
        self.files
            .values()
            .flat_map(|f| f.refs.iter())
            .filter(|r| r.name == name)
            .count()
    }

    /// Whether `name` is a HUB - its reference degree is at or above the `percentile` of the
    /// repo's OWN reference-degree distribution (5.5.2). A relative threshold, not an absolute
    /// magic number a monorepo would blow past: `new` / `parse` / `build` over-link every
    /// like-named definition, so the graph the grounder and persistence share must be able to
    /// down-weight them. This exposes the read-only fan-out primitive; wiring it into any
    /// partitioning or blast-radius decision is spec 16, deliberately out of scope here.
    pub fn is_hub(&self, name: &str, percentile: f64) -> bool {
        // The degree of every referenced name, one entry per distinct name.
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        for r in self.files.values().flat_map(|f| f.refs.iter()) {
            *counts.entry(r.name.as_str()).or_insert(0) += 1;
        }
        if counts.is_empty() {
            return false;
        }
        let mut degrees: Vec<usize> = counts.into_values().collect();
        degrees.sort_unstable();
        // Nearest-rank percentile over the distinct-name degree distribution: the cutoff is
        // the degree at rank `floor(N * percentile)` (0-based, clamped to the top entry). With
        // a two-name distribution [1, 20] and the 90th percentile this picks 20 - so only the
        // genuine hub clears the bar, not the degree-1 name. (A `(N - 1) * percentile` index
        // would pick 1 here and wrongly flag every referenced name.)
        let cutoff_idx = ((degrees.len() as f64) * percentile).floor() as usize;
        let cutoff = degrees[cutoff_idx.min(degrees.len() - 1)];
        // `cutoff > 0` keeps a repo whose entire distribution is zero-degree from flagging
        // everything; the name must actually reach the cutoff to count as a hub.
        cutoff > 0 && self.reference_degree(name) >= cutoff
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definitions_and_references_are_name_indexed_and_language_scoped() {
        let mut idx = SymbolIndex::default();
        idx.insert_file(
            "a.rs".into(),
            FileSymbols {
                lang: Lang::Rust,
                defs: vec![Def { kind: Kind::Function, name: "parse".into(), line: 3 }],
                refs: vec![SymRef { name: "parse".into(), line: 9 }],
            },
        );
        idx.insert_file(
            "b.py".into(),
            FileSymbols {
                lang: Lang::Python,
                defs: vec![Def { kind: Kind::Function, name: "parse".into(), line: 1 }],
                refs: vec![],
            },
        );
        // Name lookup finds both definitions of `parse`, across languages.
        assert_eq!(idx.definitions_named("parse").len(), 2);
        // References are LANGUAGE-SCOPED: a Rust `parse` reference never links the Python def.
        let rs_refs = idx.references_named("parse", Lang::Rust);
        assert_eq!(rs_refs.len(), 1);
        assert_eq!(rs_refs[0].0, "a.rs");
        assert_eq!(idx.references_named("parse", Lang::Python).len(), 0);
    }

    #[test]
    fn hub_symbols_are_flagged_by_repo_relative_degree() {
        let mut idx = SymbolIndex::default();
        // `new` is referenced in many files (a hub); `apply_damage` in one.
        for i in 0..20 {
            idx.insert_file(
                format!("f{i}.rs"),
                FileSymbols {
                    lang: Lang::Rust,
                    defs: vec![],
                    refs: vec![SymRef { name: "new".into(), line: 1 }],
                },
            );
        }
        idx.insert_file(
            "combat.rs".into(),
            FileSymbols {
                lang: Lang::Rust,
                defs: vec![Def { kind: Kind::Function, name: "apply_damage".into(), line: 1 }],
                refs: vec![SymRef { name: "apply_damage".into(), line: 2 }],
            },
        );
        assert_eq!(idx.reference_degree("new"), 20);
        assert_eq!(idx.reference_degree("apply_damage"), 1);
        // At the 90th percentile of the degree distribution, `new` is a hub, `apply_damage`
        // is not. The threshold is repo-relative, not an absolute constant a monorepo blows
        // past (5.5.2).
        assert!(idx.is_hub("new", 0.90));
        assert!(!idx.is_hub("apply_damage", 0.90));
        // A name with no references is never a hub, and an empty index has no hubs.
        assert!(!idx.is_hub("absent", 0.90));
        assert!(!SymbolIndex::default().is_hub("anything", 0.90));
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
                defs: vec![Def { kind: Kind::Method, name: "apply_damage".into(), line: 7 }],
                refs: vec![SymRef { name: "clamp".into(), line: 9 }],
            },
        );
        let json = serde_json::to_string(&idx).expect("model serializes with plain serde");
        let back: SymbolIndex = serde_json::from_str(&json).expect("model round-trips");
        assert_eq!(back, idx);
    }
}
