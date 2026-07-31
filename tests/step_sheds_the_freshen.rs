//! Spec 57 - Retire turbovec: the STEP SHEDS THE FRESHEN (criterion 3), proven at the STEP's
//! lifecycle seam.
//!
//! The retired vector grounder persisted an EMBEDDING index under `.rigger/grounding/` and
//! freshened it on the step path - loading it when a grounder was constructed and re-embedding
//! the just-integrated files after a unit merged. That embedding surface is gone (criterion 2
//! deleted the engine and its dependencies); the knowledge/symbol index is the lookup surface
//! now. This file pins the runtime consequence: the step path performs NO embedding-index load
//! OR freshen - it neither READS nor WRITES `.rigger/grounding/`. Criterion 3 owns BOTH halves,
//! so this test pins BOTH, not just the write.
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
//! HOW THE POISON PINS BOTH HALVES. Each temp project is seeded with a booby-trapped embedding
//! index directory under `.rigger/grounding/`, arranged so ANY resurrected embedding read or
//! write there is caught, no matter how the resurrected code behaves:
//!
//!   * A readable-garbage WITNESS file (`meta.json`) gives a byte-for-byte no-WRITE snapshot: if
//!     any step-seam operation wrote a fresh index beside it (a cold-start rebuild) or rewrote
//!     it, the snapshot would change. It does not.
//!   * An UNREADABLE (mode 0o000) primary SHARD file (`index.bin`) is the "errors if opened"
//!     no-LOAD trap: a resurrected load that enumerated the directory and opened its shards would
//!     hit EACCES on this file and error. The surviving seam completes normally, so it opened
//!     nothing under `.rigger/grounding/`.
//!
//! Between the two, every resurrected load path is caught: a STRICT load errors on the
//! unreadable shard (the seam would fail); a TOLERANT load that treats the garbage as a corrupt
//! or absent index cold-start REBUILDS one, which WRITES into the (still writable) directory and
//! trips the byte snapshot. The garbage cannot be read as a VALID index, so there is no third
//! "loaded cleanly, no error, no write" escape. The surviving symbols seam does neither: its real
//! target is the SYMBOL index under `.rigger/symbols/`, and it never touches `.rigger/grounding/`.

use rigger::grounder::grounder_for;
use rigger::grounder::symbols::store;
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// The retired persisted EMBEDDING index directory - the one the step path must never load or
/// freshen. Relative to a project root.
const EMBEDDING_INDEX_REL: &str = ".rigger/grounding";
/// The primary index shard a resurrected load would open first. Seeded UNREADABLE (mode 0o000) so
/// that opening it errors - the no-LOAD trap.
const UNREADABLE_SHARD: &str = "index.bin";
/// A readable sidecar left beside the shard. Seeded with known garbage bytes so the byte-for-byte
/// snapshot below is the no-WRITE witness.
const READABLE_WITNESS: &str = "meta.json";

/// Seed a booby-trapped embedding index under `<root>/.rigger/grounding/`: the artifact a
/// pre-retirement run would have persisted and freshened on every step, arranged to catch any
/// resurrected read OR write of it. Writes a readable-garbage WITNESS (`meta.json`, the no-WRITE
/// byte snapshot) and an UNREADABLE (mode 0o000) primary SHARD (`index.bin`, the no-LOAD trap: a
/// load that opened it would EACCES-error). The directory itself stays WRITABLE, so a tolerant
/// cold-start rebuild that wrote a fresh index here would succeed and trip the snapshot rather
/// than being silently blocked. Returns the seeded directory's readable snapshot for the
/// before/after comparison, after asserting the trap is actually armed.
fn seed_embedding_index_poison(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let dir = root.join(EMBEDDING_INDEX_REL);
    fs::create_dir_all(&dir).unwrap();
    // The readable no-WRITE witness: known garbage, so any rewrite or fresh index beside it shows
    // up in the byte snapshot.
    fs::write(
        dir.join(READABLE_WITNESS),
        br#"{"model":"retired","dims":384,"stale":true}"#,
    )
    .unwrap();
    // The unreadable no-LOAD trap: write garbage, then strip ALL permissions so opening it errors.
    let shard = dir.join(UNREADABLE_SHARD);
    fs::write(&shard, b"stale-embedding-index-not-a-real-index\x00\xff").unwrap();
    fs::set_permissions(&shard, fs::Permissions::from_mode(0o000)).unwrap();
    // The trap must be armed: opening the shard must actually error for this process, or the
    // no-LOAD proof would be vacuous.
    assert!(
        fs::read(&shard).is_err(),
        "the no-LOAD trap must be unreadable so a resurrected load that opened it would error"
    );
    snapshot_subtree(&dir)
}

/// A byte-for-byte snapshot of every READABLE file under `dir` (recursively), keyed by path
/// relative to `dir`. An unreadable file (the 0o000 shard) reads as `Err` and is omitted, so the
/// snapshot is exactly the readable witness set; an absent directory snapshots to the empty map.
/// Comparing this before and after a step-seam operation proves whether the operation added,
/// removed, or modified any readable file there - the write the criterion forbids. A resurrected
/// write that REPLACED the unreadable shard with real (readable) data would newly appear here.
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

/// Assert the seeded embedding index is byte-for-byte untouched (no WRITE) and its unreadable
/// no-LOAD trap is still armed - the check run after each reconstructed step-seam operation. That
/// the calling seam REACHED this assertion (rather than erroring on the trap) is itself the
/// no-LOAD proof: a resurrected load that opened the shard would have EACCES-errored first.
fn assert_grounding_index_untouched(root: &Path, seeded: &BTreeMap<String, Vec<u8>>, ctx: &str) {
    let dir = root.join(EMBEDDING_INDEX_REL);
    assert_eq!(
        seeded,
        &snapshot_subtree(&dir),
        "{ctx}: must not read or write the retired embedding index under {EMBEDDING_INDEX_REL} \
         (no fresh index written, no witness rewritten, no shard replaced with readable data)"
    );
    assert!(
        fs::read(dir.join(UNREADABLE_SHARD)).is_err(),
        "{ctx}: the unreadable no-LOAD trap must still be in place - the seam reaching here \
         without erroring proves it never opened a shard under {EMBEDDING_INDEX_REL}"
    );
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
    // seeded embedding index byte-for-byte untouched and never opens its unreadable shard.
    for name in ["grep", "nop"] {
        let dir = tempfile::tempdir().unwrap();
        let root_path = dir.path();
        let root = root_path.to_str().unwrap();
        fs::write(root_path.join("lib.rs"), "fn changed_symbol() {}\n").unwrap();
        let seeded = seed_embedding_index_poison(root_path);

        let grounder = grounder_for(name, root)
            .unwrap_or_else(|e| panic!("the surviving grounder {name:?} must resolve: {e}"));
        // The post-integrate freshen the conductor's integrate seam runs, over the changed file.
        grounder.reindex(root, &["lib.rs".to_string()]);

        assert_grounding_index_untouched(
            root_path,
            &seeded,
            &format!("the {name:?} grounder's post-integrate freshen"),
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
    // `Grounder` is imported HERE (not at module scope) because only this feature-gated test calls
    // its trait methods (`ground`/`reindex`) on the CONCRETE `Symbols`, which needs the trait in
    // scope. The light-lane test above calls `reindex` through a `Box<dyn Grounder>`, which
    // resolves without importing the trait - so a module-scope import would be unused (and, under
    // `-D warnings`, a hard error) in the `--no-default-features` lane.
    use rigger::grounder::symbols::grounder::Symbols;
    use rigger::grounder::Grounder;

    let dir = tempfile::tempdir().unwrap();
    let root_path = dir.path();
    let root = root_path.to_str().unwrap();

    // A one-symbol project, plus a booby-trapped embedding index the step seam must ignore
    // end-to-end (neither reading its unreadable shard nor writing over its witness).
    fs::write(root_path.join("lib.rs"), "pub fn alpha_symbol_one() {}\n").unwrap();
    let seeded = seed_embedding_index_poison(root_path);

    // SEAM 1 - per-step grounder construction. select_grounder resolves the unset default to this
    // exact call. On a cold start it builds the SYMBOL index and persists it under
    // .rigger/symbols/; a freshen-on-open that loaded the embedding index would open the trap
    // shard (EACCES) or write a fresh index over the witness.
    let grounder = Symbols::open(root, None);
    assert_grounding_index_untouched(root_path, &seeded, "constructing the default step grounder");
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
    // ...and never touched the retired embedding index: byte-for-byte the seeded witness with its
    // no-LOAD trap still armed - no file read-then-rewritten, none added, none removed, no shard
    // opened. The step sheds that freshen.
    assert_grounding_index_untouched(root_path, &seeded, "the post-integrate freshen");
}
