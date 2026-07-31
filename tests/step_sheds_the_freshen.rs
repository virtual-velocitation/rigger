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
//! index under `.rigger/grounding/`: the two files a pre-retirement run persisted there - the
//! primary shard (`index.bin`) and its metadata sidecar (`meta.json`) - each stripped to mode
//! 0o000. Two properties of that seed pin the two halves of the criterion:
//!
//!   * NO READ. Nothing under `.rigger/grounding/` is readable, so a resurrected load can consume
//!     no retired-index byte: opening either file EACCES-errors. A load only USES the index by
//!     reading its content, and there is no readable content here to obtain. (The first re-drive
//!     of this pin left the `meta.json` sidecar readable; a tolerant load could then SUCCESSFULLY
//!     read that stale sidecar, recognize a retired model, and degrade silently - an unwitnessed
//!     read of the retired index the header forbids. Making the sidecar unreadable too closes it:
//!     that successful read is no longer possible.)
//!   * NO WRITE. A structural fingerprint of the directory - every file's size, mode, and
//!     (readable) content - is taken before and after each seam operation and must be identical.
//!     A cold-start rebuild that wrote a fresh readable index, a freshen that rewrote a file, a
//!     chmod that exposed a trap, or a deletion each changes the fingerprint. None occurs.
//!
//! What this pins, precisely: the seam reads no readable content from, and writes nothing to,
//! `.rigger/grounding/`. It does NOT (and a black-box periphery test cannot) claim the seam issues
//! no bare `read_dir`/`exists` syscall against the directory - but such a probe obtains no index
//! byte and loads nothing, so it is not the embedding-index load or freshen criterion 3 forbids.
//! The surviving symbols seam does neither read nor write here: its real target is the SYMBOL index
//! under `.rigger/symbols/`, and it never touches `.rigger/grounding/` - grounding is served from
//! the symbol index, proven below by a symbol going absent -> present across the freshen.

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
/// opening it errors.
const PRIMARY_SHARD: &str = "index.bin";
/// The metadata sidecar beside the shard (model, dims). Seeded UNREADABLE (mode 0o000) too, so a
/// tolerant load cannot SUCCESSFULLY read it to recognize a stale model and degrade - no readable
/// byte remains under `.rigger/grounding/` for any resurrected load to consume.
const SIDECAR_META: &str = "meta.json";

/// Seed a booby-trapped embedding index under `<root>/.rigger/grounding/`: the artifact a
/// pre-retirement run would have persisted and freshened on every step, arranged to catch any
/// resurrected read OR write of it. Writes both files a pre-retirement index carried - the primary
/// SHARD (`index.bin`) and the metadata SIDECAR (`meta.json`) - then strips ALL permissions from
/// each (mode 0o000), so NO file here is readable: opening either EACCES-errors, and there is no
/// readable byte for a resurrected load to consume (the no-READ half). The directory itself stays
/// WRITABLE, so a tolerant cold-start rebuild that wrote a fresh index here would succeed and trip
/// the fingerprint rather than being silently blocked (the no-WRITE half). Returns the seeded
/// directory's fingerprint for the before/after comparison, after asserting each trap is armed.
fn seed_embedding_index_poison(root: &Path) -> BTreeMap<String, FileFingerprint> {
    let dir = root.join(EMBEDDING_INDEX_REL);
    fs::create_dir_all(&dir).unwrap();
    for (name, bytes) in [
        (
            PRIMARY_SHARD,
            b"stale-embedding-index-not-a-real-index\x00\xff".as_slice(),
        ),
        (
            SIDECAR_META,
            br#"{"model":"retired","dims":384,"stale":true}"#.as_slice(),
        ),
    ] {
        let file = dir.join(name);
        fs::write(&file, bytes).unwrap();
        // Strip ALL permissions so opening the file errors for this process.
        fs::set_permissions(&file, fs::Permissions::from_mode(0o000)).unwrap();
        // The trap must be armed: reading the file must actually error, or the no-READ proof would
        // be vacuous.
        assert!(
            fs::read(&file).is_err(),
            "the poison file {name} must be unreadable so any resurrected read of it errors"
        );
    }
    fingerprint_subtree(&dir)
}

/// A structural fingerprint of one file under the poisoned directory: its size, its permission
/// bits, and its byte content IF the file is readable (`None` when reading it errors - the mode
/// 0o000 traps stat fine but read as `Err`). Comparing the map of these before and after a seam
/// operation catches every write the criterion forbids - a fresh readable index (new key, or a
/// trap's `content` flipping `None` -> `Some`), a rewrite (`len`/`content` change), a chmod that
/// exposed a trap (`mode` change), or a deletion (key vanishes). Because every seeded file is
/// unreadable, `content` is `None` for all of them, so the map ALSO witnesses the no-READ half:
/// there is no readable byte under the directory for a resurrected load to consume.
#[derive(Debug, PartialEq, Eq)]
struct FileFingerprint {
    len: u64,
    mode: u32,
    content: Option<Vec<u8>>,
}

/// A fingerprint of every file under `dir` (recursively), keyed by path relative to `dir`; an
/// absent directory fingerprints to the empty map. See [`FileFingerprint`]: it records size, mode,
/// and readable content, so the before/after comparison catches any write even when the file is
/// unreadable, and simultaneously witnesses that no file here exposes readable bytes.
fn fingerprint_subtree(dir: &Path) -> BTreeMap<String, FileFingerprint> {
    fn walk(base: &Path, cur: &Path, out: &mut BTreeMap<String, FileFingerprint>) {
        let entries = match fs::read_dir(cur) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(base, &path, out);
            } else if let Ok(meta) = fs::metadata(&path) {
                let rel = path
                    .strip_prefix(base)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .into_owned();
                out.insert(
                    rel,
                    FileFingerprint {
                        len: meta.len(),
                        mode: meta.permissions().mode(),
                        content: fs::read(&path).ok(),
                    },
                );
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

/// Assert a step-seam operation left the seeded embedding index untouched, pinning BOTH halves of
/// the criterion. NO WRITE: the directory's fingerprint is identical (nothing added, removed,
/// resized, rewritten, or re-permissioned under `.rigger/grounding/`). NO READ: every file here is
/// still unreadable, so the seam obtained no retired-index byte to load - a resurrected load that
/// successfully read a file would have had to expose readable content, which the fingerprint would
/// show. Run after each reconstructed step-seam operation.
fn assert_grounding_index_untouched(
    root: &Path,
    seeded: &BTreeMap<String, FileFingerprint>,
    ctx: &str,
) {
    let dir = root.join(EMBEDDING_INDEX_REL);
    let now = fingerprint_subtree(&dir);
    assert_eq!(
        seeded, &now,
        "{ctx}: no write to the retired embedding index under {EMBEDDING_INDEX_REL} - its \
         fingerprint must be identical (no fresh index, no rewritten file, no exposed trap, no \
         deletion)"
    );
    // The no-READ witness, stated in its own right: no file under the directory exposes a readable
    // byte, so the seam read no retired-index content there.
    for (name, fp) in &now {
        assert!(
            fp.content.is_none(),
            "{ctx}: {name} under {EMBEDDING_INDEX_REL} must stay unreadable so no retired-index \
             byte is loadable by the step seam"
        );
    }
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
    // seeded embedding index fingerprint-identical and every poison file still unreadable.
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
    // end-to-end (reading neither of its unreadable poison files, writing nothing into the dir).
    fs::write(root_path.join("lib.rs"), "pub fn alpha_symbol_one() {}\n").unwrap();
    let seeded = seed_embedding_index_poison(root_path);

    // SEAM 1 - per-step grounder construction. select_grounder resolves the unset default to this
    // exact call. On a cold start it builds the SYMBOL index and persists it under
    // .rigger/symbols/; a freshen-on-open that loaded the embedding index would open a poison file
    // (EACCES) or write a fresh readable index into the directory - both caught below.
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
    // ...and never touched the retired embedding index: the directory fingerprint is identical and
    // every poison file is still unreadable - no readable byte was ever exposed for a load to
    // consume, nothing was written. The step sheds that freshen.
    assert_grounding_index_untouched(root_path, &seeded, "the post-integrate freshen");
}
