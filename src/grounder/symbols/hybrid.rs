//! The `hybrid` grounder (spec 15, unit 5): structure THEN semantics. Structural symbol
//! matches from the [`Symbols`] grounder rank FIRST; turbovec's semantic hits then fill the
//! recall a bare name match misses (a related file that shares no identifier). It composes the
//! two existing grounders - it does NOT re-implement either engine.
//!
//! Feature shape (the degrade lives in ONE authority, here, not split across `select_grounder`):
//! - with the `turbovec` feature ON, `Hybrid` holds both a `Symbols` and a `Turbovec` and
//!   composes them;
//! - with `turbovec` OFF, the `vector` field is `#[cfg]`'d away and `Hybrid` IS exactly the
//!   `symbols` mode - `ground`/`reindex` delegate straight to `Symbols`.
//!
//! Because the degrade is intrinsic to `Hybrid`, BOTH `select_grounder` cfg lanes call
//! `Hybrid::open` identically; there is no per-lane selector branch to drift (the parallel
//! -selector hazard spec 15 unit 4 was rejected for). `Hybrid::open` is fallible so a turbovec
//! construction failure surfaces LOUDLY (matching `select_grounder`'s turbovec arm), never a
//! silent degrade to symbols.

use crate::grounder::symbols::grounder::Symbols;
use crate::grounder::symbols::model::Lang;
use crate::grounder::{Grounder, Ref};

/// `hybrid`: [`Symbols`] structural matches first, then (when built) turbovec's semantic hits
/// fill the remaining `k` budget. See the module docs for the feature shape.
pub struct Hybrid {
    /// The structural axis: the symbol index over `root`. Always present.
    symbols: Symbols,
    /// The semantic axis: the turbovec vector engine. Present ONLY under the `turbovec`
    /// feature; absent, `Hybrid` degrades to exactly the `symbols` mode.
    #[cfg(feature = "turbovec")]
    vector: crate::grounder::turbovec::Turbovec,
}

impl Hybrid {
    /// Open the hybrid grounder over `root` for a grounding READ (`ground`/`run`/`serve`):
    /// [`Symbols::open`] loads (or cold-builds) the symbol index, and `Turbovec::new` loads the
    /// vector store, freshening any tree drift - what the read paths want. Fallible because
    /// building the vector engine is (e.g. the embedding model cannot load); the caller surfaces
    /// that loudly rather than silently dropping to symbols-only.
    #[cfg(feature = "turbovec")]
    pub fn open(root: &str, override_lang: Option<Lang>) -> Result<Hybrid, String> {
        Ok(Hybrid {
            symbols: Symbols::open(root, override_lang),
            vector: crate::grounder::turbovec::Turbovec::new(root)?,
        })
    }

    /// Open for `rigger reindex`: the vector engine loads the persisted store WITHOUT a
    /// whole-tree freshen (`Turbovec::new_for_reindex`), because `reindex` re-embeds exactly the
    /// named files and a preceding full freshen would double-embed them - identical to the
    /// turbovec grounder's own `select_reindex_grounder` path. The symbols side loads-only either
    /// way (its `open` never freshens the whole tree), so it is opened the same as in [`open`].
    #[cfg(feature = "turbovec")]
    pub fn open_for_reindex(root: &str, override_lang: Option<Lang>) -> Result<Hybrid, String> {
        Ok(Hybrid {
            symbols: Symbols::open(root, override_lang),
            vector: crate::grounder::turbovec::Turbovec::new_for_reindex(root)?,
        })
    }

    /// Feature-off `open`: with turbovec absent there is no vector engine to build, so `Hybrid`
    /// is exactly the `symbols` mode. Returns `Result` for a signature identical to the
    /// turbovec-on lane (always `Ok` here) so `select_grounder` calls it the same way in both.
    #[cfg(not(feature = "turbovec"))]
    pub fn open(root: &str, override_lang: Option<Lang>) -> Result<Hybrid, String> {
        Ok(Hybrid {
            symbols: Symbols::open(root, override_lang),
        })
    }

    /// Feature-off `open_for_reindex`: without turbovec, reindex opens IDENTICALLY to [`open`]
    /// (symbols loads-only; there is no vector store to load-without-freshening), so this
    /// delegates - one degrade authority, no hand-synced second body.
    #[cfg(not(feature = "turbovec"))]
    pub fn open_for_reindex(root: &str, override_lang: Option<Lang>) -> Result<Hybrid, String> {
        Self::open(root, override_lang)
    }
}

impl Grounder for Hybrid {
    fn ground(&self, query: &str, k: usize) -> Vec<Ref> {
        // Structural matches ALWAYS come first: the symbols grounder's own ranking
        // (definition > reference > absent prose) is preserved as the head of the result.
        let structural = self.symbols.ground(query, k);
        #[cfg(feature = "turbovec")]
        {
            let mut out = structural;
            // Fill the remaining budget with turbovec's semantic hits - the recall a bare name
            // match misses (a related file that shares no identifier). De-dup on the exact
            // (file, line) so a location the symbol index already surfaced is not repeated; a
            // semantic hit elsewhere in an already-listed file is still additive context.
            if out.len() < k {
                for r in self.vector.ground(query, k) {
                    if !out.iter().any(|o| o.file == r.file && o.line == r.line) {
                        out.push(r);
                        if out.len() >= k {
                            break;
                        }
                    }
                }
            }
            out
        }
        #[cfg(not(feature = "turbovec"))]
        structural
    }

    fn reindex(&self, src_dir: &str, files: &[String]) {
        // Freshen BOTH axes over the changed files: the symbol index and (when built) the vector
        // store. Each delegate owns its own incremental freshening + persistence authority; hybrid
        // only fans the delta out to both, it re-implements neither.
        self.symbols.reindex(src_dir, files);
        #[cfg(feature = "turbovec")]
        self.vector.reindex(src_dir, files);
    }
}

#[cfg(all(test, feature = "turbovec"))]
mod tests {
    use super::*;
    use crate::grounder::Grounder;
    use serial_test::file_serial;

    /// With turbovec PRESENT, hybrid COMPOSES the two axes: the STRUCTURAL definition leads, and
    /// the semantic pass fills a file the name match misses. `combat.rs` DEFINES `apply_damage` (a
    /// structural hit); `enemy.rs` is semantically about dealing damage but defines NO such symbol
    /// (a semantic-only hit). Hybrid must (a) rank `combat.rs` FIRST - structure ahead of semantics
    /// - and (b) still surface `enemy.rs`, which the symbols grounder ALONE never returns, proving
    /// the vector pass fills the recall a name match misses. A single-file corpus could not tell
    /// structure-first from semantic-first (both would return the one file), so two files are
    /// required. `#[file_serial(turbovec_model)]` because constructing the vector engine builds an
    /// ort/model session that must never race another test's build.
    #[test]
    #[file_serial(turbovec_model)]
    fn hybrid_ranks_structure_first_then_fills_semantic_recall() {
        let dir = tempfile::tempdir().unwrap();
        // Structural match: DEFINES the queried symbol.
        std::fs::write(
            dir.path().join("combat.rs"),
            "fn apply_damage(target: &mut Entity, amount: f32) {\n    target.health -= amount;\n}\n",
        )
        .unwrap();
        // Semantic-only: about reducing an enemy's hitpoints, but with NO `apply_damage` symbol.
        std::fs::write(
            dir.path().join("enemy.rs"),
            "fn reduce_hitpoints(enemy: &mut Enemy, blow: f32) {\n    enemy.hp -= blow;\n}\n",
        )
        .unwrap();
        let root = dir.path().to_str().unwrap();
        let g = Hybrid::open(root, None).expect("hybrid opens");
        let refs = g.ground("apply_damage", 5);
        assert!(!refs.is_empty(), "the structural definition must ground");
        // (a) The structural definition ranks FIRST, ahead of any purely-semantic hit.
        assert_eq!(
            refs[0].file, "combat.rs",
            "the structural definition must lead; got {refs:?}"
        );
        // (b) The symbols grounder ALONE never surfaces enemy.rs (it defines no matching symbol),
        // so its presence in the hybrid result proves the semantic pass filled the recall that the
        // structural axis missed - the composition, not just symbols with extra steps.
        let symbols_only = Symbols::open(root, None).ground("apply_damage", 5);
        assert!(
            !symbols_only.iter().any(|r| r.file == "enemy.rs"),
            "precondition: the symbols axis alone must NOT return the semantic-only file; got {symbols_only:?}"
        );
        assert!(
            refs.iter().any(|r| r.file == "enemy.rs"),
            "hybrid must fill the semantic recall the name match misses; got {refs:?}"
        );
    }
}

// Feature-off control (spec 15 unit 5 done-when: "a feature-off control"): with turbovec
// ABSENT, `Hybrid` must be EXACTLY the `symbols` mode. This module compiles/runs only in the
// symbols-on / turbovec-off lane (`cargo test --no-default-features --features symbols`); it
// builds no model, so it needs no serialization.
#[cfg(all(test, not(feature = "turbovec")))]
mod degrade_tests {
    use super::*;
    use crate::grounder::Grounder;

    #[test]
    fn without_turbovec_hybrid_is_exactly_the_symbols_mode() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("combat.rs"),
            "fn apply_damage(x: u8) -> u8 { x }\n",
        )
        .unwrap();
        let root = dir.path().to_str().unwrap();
        let hybrid = Hybrid::open(root, None).expect("hybrid opens without turbovec");
        let symbols = Symbols::open(root, None);
        // Same query, same result: hybrid degrades to precisely the symbols ranking - there is
        // no vector pass to add anything.
        let hy = hybrid.ground("apply_damage", 5);
        let sy = symbols.ground("apply_damage", 5);
        assert!(
            !hy.is_empty(),
            "the definition must ground in the degrade mode"
        );
        assert_eq!(hy[0].file, "combat.rs");
        assert_eq!(
            hy.iter().map(|r| (&r.file, r.line)).collect::<Vec<_>>(),
            sy.iter().map(|r| (&r.file, r.line)).collect::<Vec<_>>(),
            "without turbovec, hybrid must return exactly what the symbols grounder returns"
        );
    }
}
