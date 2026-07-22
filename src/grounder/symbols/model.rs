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

/// A reference site: the referenced name, its 1-based line, and the ENCLOSING definition the
/// reference occurs inside (the caller). `None` for a top-level reference outside every
/// definition - an import or an `impl`-header trait bound that belongs to no function body. Set
/// during extraction by attributing the reference to the innermost definition whose body contains
/// it (spec 37). Serde-defaulted and omitted when `None` so a pre-37 persisted index folds as
/// caller-less and a caller-less ref serializes byte-identically to before.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymRef {
    pub name: String,
    pub line: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enclosing: Option<String>,
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

    /// Drop a file's symbols from the index (a no-op when no entry is held for `rel_path`). The
    /// incremental reindex path calls this when a changed file can no longer be read or extracted
    /// (it was deleted or became unreadable), so the freshened index matches a fresh whole-tree
    /// `build_index`, which never visits the gone file: a stale definition or reference from a
    /// removed file must not keep grounding. Parser-free by construction, so it stays in the light
    /// lane exactly like the rest of the model.
    pub fn remove_file(&mut self, rel_path: &str) {
        self.files.remove(rel_path);
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

    /// How many references name `name` WITHIN `lang` - the per-language fan-out degree, the raw
    /// fan-out the suppression threshold is computed against. SCOPED to `lang`, mirroring
    /// `references_named`: the cross-reference graph is per-language (5.5.2), so the degree over
    /// it is too. A name that over-links in another language never inflates this count (a Python
    /// `parse` leaves the Rust `parse` degree untouched).
    pub fn reference_degree(&self, name: &str, lang: Lang) -> usize {
        self.files
            .values()
            .filter(|f| f.lang == lang)
            .flat_map(|f| f.refs.iter())
            .filter(|r| r.name == name)
            .count()
    }

    /// Whether `name` is a HUB WITHIN `lang` - a STRICT high-degree OUTLIER against THAT
    /// language's OWN reference-degree distribution (5.5.2): its per-language reference degree is
    /// STRICTLY ABOVE the `percentile` cutoff of the distinct-name degrees. Scoped to `lang` on
    /// BOTH axes: the degree counted for `name` and the distribution the cutoff is drawn from are
    /// each restricted to `lang`, so a name that over-links in another language (a Python `parse`)
    /// never flags the same name in this one (a Rust `parse`) - the cross-language collision
    /// per-language scoping exists to prevent. A relative threshold, not an absolute magic number
    /// a monorepo would blow past: `new` / `parse` / `build` over-link every like-named definition,
    /// so the graph the grounder and persistence share must be able to down-weight them.
    ///
    /// The outlier definition is deliberate. A realistic reference graph is a long tail: most names
    /// are referenced once or twice (hapax), a few over-link. A cutoff drawn as the degree AT the
    /// percentile rank, matched with `>=`, collapses to 1 on such a tail (the rank lands inside the
    /// mass of degree-1 names) and then flags EVERY referenced name - so every unit would serialize
    /// and parallelism-retention would collapse to near zero. Requiring the degree to be STRICTLY
    /// ABOVE the cutoff makes a hub a genuine outlier: a flat or long-tail-only distribution with no
    /// meaningful spread yields NO hubs (nothing exceeds the bulk), and the fraction flagged stays
    /// near the top `(1 - percentile)` decile, never approaching all. This exposes the read-only
    /// fan-out primitive; wiring it into any partitioning or blast-radius decision is spec 16.
    pub fn is_hub(&self, name: &str, lang: Lang, percentile: f64) -> bool {
        // The degree of every referenced name IN `lang`, one entry per distinct name.
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        for r in self
            .files
            .values()
            .filter(|f| f.lang == lang)
            .flat_map(|f| f.refs.iter())
        {
            *counts.entry(r.name.as_str()).or_insert(0) += 1;
        }
        if counts.is_empty() {
            return false;
        }
        let mut degrees: Vec<usize> = counts.into_values().collect();
        degrees.sort_unstable();
        // Nearest-rank percentile over the distinct-name degree distribution, drawn as the degree
        // at 0-based rank `floor((N - 1) * percentile)` - always in bounds (the index never exceeds
        // `N - 1`), so no clamp is needed. This lands the cutoff on a TYPICAL degree the outliers
        // rise above rather than on the top element itself: with a two-name distribution [1, 20] at
        // the 90th percentile it picks 1, and the STRICT `>` below then flags only `20` as a hub,
        // never the degree-1 name. On a long tail dominated by degree-1 names the cutoff is 1 and
        // `>` flags only the names that genuinely rise above the tail; on a FLAT distribution the
        // cutoff equals the shared degree and `>` flags nothing (no meaningful spread, no hub). A
        // `floor(N * percentile)` index with `>=` would instead sit ON the tail value and flag
        // every referenced name - the degeneration this outlier definition exists to prevent.
        let cutoff_idx = (((degrees.len() - 1) as f64) * percentile).floor() as usize;
        let cutoff = degrees[cutoff_idx];
        // STRICTLY above the cutoff: the name must be a genuine high-degree outlier, not merely
        // reach the typical degree. Every counted degree is >= 1, so `cutoff >= 1` and a hub needs
        // degree >= 2 at minimum - a lone or flat reference set can never manufacture a hub.
        self.reference_degree(name, lang) > cutoff
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
                defs: vec![Def {
                    kind: Kind::Function,
                    name: "parse".into(),
                    line: 3,
                }],
                refs: vec![SymRef {
                    name: "parse".into(),
                    line: 9,
                    enclosing: None,
                }],
            },
        );
        idx.insert_file(
            "b.py".into(),
            FileSymbols {
                lang: Lang::Python,
                defs: vec![Def {
                    kind: Kind::Function,
                    name: "parse".into(),
                    line: 1,
                }],
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
                    refs: vec![SymRef {
                        name: "new".into(),
                        line: 1,
                        enclosing: None,
                    }],
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
                }],
                refs: vec![SymRef {
                    name: "apply_damage".into(),
                    line: 2,
                    enclosing: None,
                }],
            },
        );
        assert_eq!(idx.reference_degree("new", Lang::Rust), 20);
        assert_eq!(idx.reference_degree("apply_damage", Lang::Rust), 1);
        // At the 90th percentile of the degree distribution, `new` is a hub, `apply_damage`
        // is not. The threshold is repo-relative, not an absolute constant a monorepo blows
        // past (5.5.2).
        assert!(idx.is_hub("new", Lang::Rust, 0.90));
        assert!(!idx.is_hub("apply_damage", Lang::Rust, 0.90));
        // A name with no references is never a hub, and an empty index has no hubs.
        assert!(!idx.is_hub("absent", Lang::Rust, 0.90));
        assert!(!SymbolIndex::default().is_hub("anything", Lang::Rust, 0.90));
    }

    #[test]
    fn hub_detection_does_not_flag_nearly_all_on_a_degree_one_dominated_distribution() {
        // The realistic long-tail (hapax-heavy) distribution the robust cutoff must survive: a
        // large tail of single-reference (degree-1) names plus a couple of genuine high-degree
        // outliers. The old nearest-rank `floor(N * percentile)` cutoff collapses to 1 on this
        // shape and the `>=` comparison then flags EVERY referenced name as a hub - so every unit
        // serializes and parallelism-retention collapses to near zero. A hub must be a STRICT
        // high-degree outlier: only the genuine outliers clear the bar, and the flagged fraction
        // stays near the top decile, never approaching all.
        let mut idx = SymbolIndex::default();
        let mut refs: Vec<SymRef> = Vec::new();
        // 30 hapax names, each referenced exactly once (degree 1) - the long tail.
        for i in 0..30 {
            refs.push(SymRef {
                name: format!("hapax_{i}"),
                line: 1,
                enclosing: None,
            });
        }
        // Two genuine high-degree outliers (degree 15 each) - the real hubs.
        for _ in 0..15 {
            refs.push(SymRef {
                name: "hub_a".into(),
                line: 1,
                enclosing: None,
            });
            refs.push(SymRef {
                name: "hub_b".into(),
                line: 1,
                enclosing: None,
            });
        }
        idx.insert_file(
            "big.rs".into(),
            FileSymbols {
                lang: Lang::Rust,
                defs: vec![],
                refs,
            },
        );

        // The two genuine outliers ARE hubs; a degree-1 tail name is NOT.
        assert!(idx.is_hub("hub_a", Lang::Rust, 0.90));
        assert!(idx.is_hub("hub_b", Lang::Rust, 0.90));
        assert!(!idx.is_hub("hapax_0", Lang::Rust, 0.90));

        // The load-bearing property: across ALL 32 distinct referenced names, only the genuine
        // outliers are flagged - the fraction stays small, never approaching all (the exact
        // regression the old `floor(N * p)` + `>=` rule caused: it would flag all 32 here).
        let all_names: Vec<String> = (0..30)
            .map(|i| format!("hapax_{i}"))
            .chain(["hub_a".to_string(), "hub_b".to_string()])
            .collect();
        let flagged: Vec<&String> = all_names
            .iter()
            .filter(|n| idx.is_hub(n, Lang::Rust, 0.90))
            .collect();
        assert_eq!(
            flagged.len(),
            2,
            "only the two genuine outliers are hubs on a degree-one-dominated distribution, \
             not the 30 hapax names; got {flagged:?}"
        );
        assert!(
            (flagged.len() as f64) / (all_names.len() as f64) < 0.25,
            "the flagged fraction must stay near the top decile on a degree-one-dominated \
             distribution, never approaching all; got {}/{}",
            flagged.len(),
            all_names.len()
        );

        // A FLAT distribution (no meaningful spread) yields NO hubs: with every name at the same
        // degree there is no outlier to flag, even at a low percentile.
        let mut flat = SymbolIndex::default();
        let mut flat_refs: Vec<SymRef> = Vec::new();
        for name in ["alpha", "beta", "gamma", "delta"] {
            for _ in 0..5 {
                flat_refs.push(SymRef {
                    name: name.into(),
                    line: 1,
                    enclosing: None,
                });
            }
        }
        flat.insert_file(
            "flat.rs".into(),
            FileSymbols {
                lang: Lang::Rust,
                defs: vec![],
                refs: flat_refs,
            },
        );
        for name in ["alpha", "beta", "gamma", "delta"] {
            assert!(
                !flat.is_hub(name, Lang::Rust, 0.90),
                "a flat distribution has no meaningful spread, so no name is a hub; {name} was flagged"
            );
        }
    }

    #[test]
    fn fan_out_suppression_is_language_scoped_no_cross_language_inflation() {
        // The exact cross-language collision per-language scoping exists to prevent (5.5.2):
        // the name `parse` is referenced ONCE in Rust but TWENTY times in Python. A
        // language-BLIND degree would report 21 and flag the Rust `parse` as a hub purely from
        // Python usage. Per-language scoping must keep the two graphs disjoint.
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
                    }],
                },
            );
        }
        // Rust references `parse` once and `new` ten times, so Rust's OWN hub is `new`.
        idx.insert_file(
            "a.rs".into(),
            FileSymbols {
                lang: Lang::Rust,
                defs: vec![],
                refs: vec![SymRef {
                    name: "parse".into(),
                    line: 3,
                    enclosing: None,
                }],
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
                    }],
                },
            );
        }
        // Degree is per-language: the 20 Python refs do NOT inflate the Rust `parse` degree,
        // and vice versa.
        assert_eq!(idx.reference_degree("parse", Lang::Rust), 1);
        assert_eq!(idx.reference_degree("parse", Lang::Python), 20);
        assert_eq!(idx.reference_degree("new", Lang::Rust), 10);
        // The Rust `parse` is NOT a hub: measured against Rust's OWN distribution ([1, 10] for
        // {parse, new}) it does not rise above the cutoff (the typical degree 1), even at the 50th
        // percentile - even though the SAME name over-links in Python. Rust's real hub is `new`,
        // which is a genuine outlier above Rust's own tail.
        assert!(!idx.is_hub("parse", Lang::Rust, 0.50));
        assert!(idx.is_hub("new", Lang::Rust, 0.50));
        // Python's `parse` is NOT a hub either: its distribution is a SINGLE name ([20]), which has
        // no spread, so there is no outlier to flag - a hub is a STRICT high-degree outlier, and a
        // lone name cannot rise above itself. (This is the over-serialization a single-name
        // per-language distribution used to cause; a robust cutoff yields no hub without spread.)
        assert!(!idx.is_hub("parse", Lang::Python, 0.50));
        // And the Python usage of `parse` never leaks into Rust's verdict at any percentile.
        assert!(!idx.is_hub("parse", Lang::Rust, 0.90));
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
                }],
                refs: vec![SymRef {
                    name: "clamp".into(),
                    line: 9,
                    enclosing: None,
                }],
            },
        );
        let json = serde_json::to_string(&idx).expect("model serializes with plain serde");
        let back: SymbolIndex = serde_json::from_str(&json).expect("model round-trips");
        assert_eq!(back, idx);
    }
}
