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

    /// The two-view blast radius (architecture 5.5.1, spec 16 unit 1) forwards to the inner
    /// [`Symbols`] index. `Hybrid` is symbols-ACTIVE - it composes a real `Symbols` - so its safe
    /// view MUST be the structural-union-grep superset with a hub serializing, exactly the
    /// [`Symbols::blast_radius`] contract, NOT the grep-only trait default a non-symbols grounder
    /// inherits (which would cap the safe view at `k`, miss the grep-only references, and never
    /// serialize). The semantic (turbovec) axis is deliberately NOT unioned in: the safe contract
    /// is defined as `structural ∪ grep` (5.5.9), and `blast_radius` grounds partitioning safety,
    /// not the prompt recall the vector pass fills. So the composite delegates to the structural
    /// axis that owns the two-view query - it re-implements neither engine, exactly as `ground` and
    /// `reindex` do.
    fn blast_radius(&self, query: &str, k: usize) -> crate::grounder::BlastRadius {
        self.symbols.blast_radius(query, k)
    }
}

// The two-view `blast_radius` invariant over the COMPOSITE grounder (spec 16 unit 1). `Hybrid`
// composes an inner `Symbols` index, so it is symbols-ACTIVE and MUST serve the structural
// two-view contract - NOT the grep-only trait default a non-symbols grounder inherits. This
// module is gated ONLY on `test` (hybrid.rs itself compiles only under the `symbols` feature),
// so it runs in BOTH symbols-on lanes: the default `turbovec` lane (where `Hybrid::open` builds a
// real vector engine, hence `#[file_serial(turbovec_model)]`) AND the `--no-default-features
// --features symbols` degrade lane (no model built). blast_radius forwards to the inner `Symbols`
// and never touches the semantic axis, so the invariant is identical in both.
#[cfg(test)]
mod blast_radius_tests {
    use super::*;
    use crate::grounder::symbols::grounder::Symbols;
    use crate::grounder::Grounder;

    /// The composite `Hybrid` grounder's `blast_radius` serves the safe-superset + serialize-on-hub
    /// contract, NOT the grep-only trait default. This is the exact invariant the shipped
    /// `defaults.grounder: hybrid` broke by inheriting the default: its safe view was `ground(q,k)`
    /// capped at `k`, missing the grep-only references and never serializing a hub. We pin all three
    /// facets over a hub fixture:
    /// - `serialize == true` for a hub symbol (never truncated),
    /// - the safe view RECOVERS a comment-only grep reference the structural graph misses (a grep
    ///   superset), and is UNCAPPED (carries every hub file past the small `k`),
    /// - a degree-1 symbol in the same repo does NOT serialize.
    ///
    /// And, decisively, that `Hybrid::blast_radius` EQUALS its inner `Symbols::blast_radius` - the
    /// forward is exact, so the whole well-tested `Symbols` two-view contract holds for `Hybrid`.
    /// `#[cfg_attr(feature = "turbovec", ...)]` applies `#[file_serial]` ONLY in the lane where
    /// `Hybrid::open` builds an ort/model session (it must never race another test's build); in the
    /// no-turbovec degrade lane it builds no model and needs no serialization.
    #[test]
    #[cfg_attr(feature = "turbovec", serial_test::file_serial(turbovec_model))]
    fn hybrid_blast_radius_forwards_the_two_view_contract_not_the_grep_default() {
        let dir = tempfile::tempdir().unwrap();
        // `spawn` is referenced across many files (a hub); `def.rs` defines it.
        std::fs::write(dir.path().join("def.rs"), "fn spawn() {}\n").unwrap();
        let mut expected: Vec<String> = vec!["def.rs".to_string()];
        for i in 0..12 {
            let name = format!("f{i}.rs");
            std::fs::write(dir.path().join(&name), "fn c() { spawn(); }\n").unwrap();
            expected.push(name);
        }
        // A comment-only mention of `spawn`: NOT a symbol, so the structural graph misses it, but a
        // literal grep recovers it - the recall the safe superset exists to backstop.
        std::fs::write(
            dir.path().join("notes.rs"),
            "// spawn is discussed but never called here\n",
        )
        .unwrap();
        // A degree-1 symbol so the per-language distribution has a genuine low-degree name.
        std::fs::write(dir.path().join("rare.rs"), "fn r() { rare_call(); }\n").unwrap();
        let root = dir.path().to_str().unwrap();
        let hybrid = Hybrid::open(root, None).expect("hybrid opens");

        // A SMALL cap proves the safe view is uncapped: there are 13 `spawn` files plus the comment.
        let br = hybrid.blast_radius("spawn", 8);
        assert!(
            br.serialize,
            "the composite hybrid must serialize a hub, not inherit the never-serialize grep default; got {br:?}"
        );
        assert!(
            br.safe.contains(&"notes.rs".to_string()),
            "the hybrid safe view must recover the grep-only reference the structural graph misses (a grep superset), not the k-capped grep default; got {br:?}"
        );
        for f in &expected {
            assert!(
                br.safe.contains(f),
                "the hybrid safe view is uncapped and must not truncate {f}; got {br:?}"
            );
        }
        assert!(
            br.safe.len() > 8,
            "the hybrid safe view must exceed the {k}-cap the grep default would impose; got {n} in {br:?}",
            k = 8,
            n = br.safe.len()
        );

        // A degree-1 symbol is NOT a hub and must not serialize.
        let rare = hybrid.blast_radius("rare_call", 8);
        assert!(
            !rare.serialize,
            "a degree-1 symbol is not a hub; got {rare:?}"
        );

        // Decisive: the forward is EXACT - hybrid's blast_radius equals the inner symbols'. Building
        // a bare `Symbols` over the same root (no model) is cheap and proves the composition adds
        // nothing to and drops nothing from the two-view contract.
        let symbols = Symbols::open(root, None);
        assert_eq!(
            hybrid.blast_radius("spawn", 8),
            symbols.blast_radius("spawn", 8),
            "Hybrid::blast_radius must forward exactly to the inner Symbols::blast_radius"
        );
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
