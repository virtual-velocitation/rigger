//! Spec 57 - Retire turbovec: the STEP SHEDS THE FRESHEN (criterion 3), proven at the STEP's
//! lifecycle seam.
//!
//! The retired vector grounder persisted an EMBEDDING index under `.rigger/grounding/` and
//! freshened it on the step path - loading it when a grounder was constructed and re-embedding
//! the just-integrated files after a unit merged. That embedding surface is gone (criterion 2
//! deleted the engine and its dependencies); the knowledge/symbol index is the lookup surface
//! now. This file pins the runtime consequence: the step path performs NO embedding-index load
//! or freshen - it neither READS nor WRITES `.rigger/grounding/`.
//!
//! The step's lifecycle performs exactly TWO grounder operations, and this test reconstructs
//! both over a real temporary project - the outside-in periphery approach, over the library's
//! public surface, exactly as the step wires them:
//!
//!   1. PER-STEP GROUNDER CONSTRUCTION. `rigger step` builds the configured grounder via
//!      `main::select_grounder`, whose UNSET / empty default resolves to the structural
//!      `symbols` grounder - `Symbols::open(root, None)`. A freshen-on-open that touched the
//!      embedding index would read/rewrite `.rigger/grounding/`.
//!   2. THE POST-INTEGRATE FRESHEN. After a unit's merge lands, the conductor's integrate seam
//!      (`src/conductor.rs`, the `deps.grounder.reindex(&deps.repo, &files)` call) freshens the
//!      grounder over the just-changed files. With turbovec retired this is the symbols-
//!      structural reindex, which writes the SYMBOL index under `.rigger/symbols/` - never the
//!      embedding index under `.rigger/grounding/`.
//!
//! Each temp project is seeded with a deliberately-garbage sentinel under `.rigger/grounding/`
//! (a stale embedding index a pre-retirement run would have left). If any step-seam operation
//! still treated that directory as the embedding index, it would read the sentinel (and, being
//! garbage, either fail or rewrite it) or write a fresh index beside it - either way the
//! directory's byte-for-byte snapshot would change. It does not: the seam ignores
//! `.rigger/grounding/` entirely, and the freshen's real target is `.rigger/symbols/`.

use rigger::grounder::symbols::store;
use rigger::grounder::{grounder_for, Grounder};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// The retired persisted EMBEDDING index directory - the one the step path must never load or
/// freshen. Relative to a project root.
const EMBEDDING_INDEX_REL: &str = ".rigger/grounding";

/// Seed a stale embedding index under `<root>/.rigger/grounding/`: the artifact a pre-retirement
/// run would have persisted and freshened on every step. The bytes are deliberately NOT a valid
/// index of any kind, so a step-seam operation that still tried to LOAD this directory as the
/// embedding index would have to either fail or consume/rewrite it - an observable change the
/// snapshot below would catch. Returns the seeded directory's snapshot for the before/after
/// comparison.
fn seed_stale_embedding_index(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let dir = root.join(EMBEDDING_INDEX_REL);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("index.bin"),
        b"stale-embedding-index-not-a-real-index\x00\xff",
    )
    .unwrap();
    fs::write(dir.join("meta.json"), br#"{"model":"retired","dims":384}"#).unwrap();
    snapshot_subtree(&dir)
}

/// A byte-for-byte snapshot of every file under `dir` (recursively), keyed by path relative to
/// `dir`. An absent directory snapshots to the empty map. Comparing this before and after a
/// step-seam operation proves whether the operation added, removed, or modified ANY file there -
/// the read/write the criterion forbids.
fn snapshot_subtree(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(base: &Path, cur: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        let entries = match fs::read_dir(cur) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(base, &path, out);
            } else if let Ok(bytes) = fs::read(&path) {
                let rel = path
                    .strip_prefix(base)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .into_owned();
                out.insert(rel, bytes);
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

/// The persisted symbol-index path for `root`, used to prove the freshen's real write target is
/// `.rigger/symbols/`, not the retired `.rigger/grounding/`.
fn symbol_index_path(root: &str) -> PathBuf {
    store::index_path(root)
}

/// BOTH FEATURE LANES. The grounders a `--no-default-features` (feature-off) build still ships -
/// `grep` and `nop`, the only names `grounder_for` resolves without the `symbols` feature - have
/// NO embedding index, so their post-integrate freshen (`Grounder::reindex`, the exact call the
/// conductor's integrate seam makes) is a no-op that never touches `.rigger/grounding/`. This
/// pins the runtime removal in the light lane, where the substantive symbols-lane test below
/// does not compile. It also pins the surviving PERSISTED-INDEX AUTHORITY: `store::index_path`
/// (parser-free, both lanes) points at the SYMBOL index under `.rigger/symbols/`, never the
/// retired embedding index under `.rigger/grounding/`.
#[test]
fn the_surviving_freshen_never_touches_the_embedding_index() {
    // The persisted-index authority is the symbol index, not the retired embedding directory.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let index_path = symbol_index_path(root);
    assert!(
        index_path.ends_with(".rigger/symbols/index.json"),
        "the surviving persisted index is the SYMBOL index under .rigger/symbols/; got {}",
        index_path.display()
    );
    assert!(
        !index_path.starts_with(dir.path().join(EMBEDDING_INDEX_REL)),
        "the persisted index must not live under the retired embedding directory {}; got {}",
        EMBEDDING_INDEX_REL,
        index_path.display()
    );

    // Every grounder a feature-off build can select, freshened over a changed file, leaves the
    // seeded embedding index byte-for-byte untouched.
    for name in ["grep", "nop"] {
        let dir = tempfile::tempdir().unwrap();
        let root_path = dir.path();
        let root = root_path.to_str().unwrap();
        fs::write(root_path.join("lib.rs"), "fn changed_symbol() {}\n").unwrap();
        let before = seed_stale_embedding_index(root_path);

        let grounder = grounder_for(name, root)
            .unwrap_or_else(|e| panic!("the surviving grounder {name:?} must resolve: {e}"));
        // The post-integrate freshen the conductor's integrate seam runs, over the changed file.
        grounder.reindex(root, &["lib.rs".to_string()]);

        let after = snapshot_subtree(&root_path.join(EMBEDDING_INDEX_REL));
        assert_eq!(
            before, after,
            "the {name:?} grounder's post-integrate freshen must not read or write the retired \
             embedding index under {EMBEDDING_INDEX_REL} (it has no such index); the directory \
             changed"
        );
    }
}

/// SYMBOLS LANE (the default lane). The DEFAULT step grounder - what `main::select_grounder`
/// resolves the unset/empty `defaults.grounder` to, `Symbols::open(root, None)` - and the
/// conductor's post-integrate freshen (`Grounder::reindex`) together perform NO read or write of
/// the retired embedding index `.rigger/grounding/`. The freshen's real target is the SYMBOL
/// index under `.rigger/symbols/`, and grounding is served from it.
#[cfg(feature = "symbols")]
#[test]
fn the_default_step_grounder_seam_never_reads_or_writes_the_embedding_index() {
    use rigger::grounder::symbols::grounder::Symbols;

    let dir = tempfile::tempdir().unwrap();
    let root_path = dir.path();
    let root = root_path.to_str().unwrap();

    // A one-symbol project, plus a stale embedding index the step seam must ignore end-to-end.
    fs::write(root_path.join("lib.rs"), "pub fn alpha_symbol_one() {}\n").unwrap();
    let seeded = seed_stale_embedding_index(root_path);

    // SEAM 1 - per-step grounder construction. select_grounder resolves the unset default to this
    // exact call. On a cold start it builds the SYMBOL index and persists it under
    // .rigger/symbols/; a freshen-on-open that loaded the embedding index would touch
    // .rigger/grounding/.
    let grounder = Symbols::open(root, None);
    assert_eq!(
        seeded,
        snapshot_subtree(&root_path.join(EMBEDDING_INDEX_REL)),
        "constructing the default step grounder must not read or write the retired embedding \
         index under {EMBEDDING_INDEX_REL}"
    );
    assert!(
        symbol_index_path(root).exists(),
        "constructing the default grounder builds and persists the SYMBOL index under \
         .rigger/symbols/ - the freshen's real target"
    );
    // The construction actually indexed the project (so the seam is exercised, not vacuous), and
    // grounding is served from the symbol index, never the embedding directory.
    assert!(
        !grounder.ground("alpha_symbol_one", 8).is_empty(),
        "the default grounder grounds the project's symbol from the persisted SYMBOL index"
    );
    // The new symbol does not exist yet, so the freshen below has real work to do.
    assert!(
        grounder.ground("beta_symbol_two", 8).is_empty(),
        "the new symbol is absent before the post-integrate freshen"
    );

    // SEAM 2 - the post-integrate freshen. A unit merged a change to lib.rs; the conductor's
    // integrate seam calls grounder.reindex(repo, &files) over the changed file (conductor.rs).
    // With turbovec retired this freshens the SYMBOL index, never the embedding index.
    fs::write(
        root_path.join("lib.rs"),
        "pub fn alpha_symbol_one() {}\npub fn beta_symbol_two() {}\n",
    )
    .unwrap();
    grounder.reindex(root, &["lib.rs".to_string()]);

    // The freshen landed in the symbol index: the newly-added symbol now grounds.
    assert!(
        !grounder.ground("beta_symbol_two", 8).is_empty(),
        "the post-integrate freshen re-parsed the changed file into the SYMBOL index"
    );
    // ...and never touched the retired embedding index: it is byte-for-byte the seeded snapshot -
    // no file read-then-rewritten, none added, none removed.
    assert_eq!(
        seeded,
        snapshot_subtree(&root_path.join(EMBEDDING_INDEX_REL)),
        "the post-integrate freshen must not read or write the retired embedding index under \
         {EMBEDDING_INDEX_REL}; the step sheds that freshen"
    );
}
