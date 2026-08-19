//! CLI periphery for spec 68, criterion 4 - VALIDATE ADVISORIES.
//!
//! `rigger validate` gains two advisory-only checks (Design):
//!   (a) INDEX STALENESS - the persisted `symbols` grounding index has drifted from the tree,
//!       warn and name `rigger reindex`.
//!   (b) LOG BLOAT - the event log's derived index is duplicated above threshold, warn with the
//!       measured factor and name `rigger reset --derived`.
//!
//! Both are stderr-only warnings that NEVER change `validate`'s exit status (Design: "advisory-
//! warn, never failing"). These tests drive the compiled binary so the observable surface -
//! stdout/stderr/exit code - is pinned exactly as an operator sees it.
//!
//! What this file OWNS (criterion 4): the two advisories' trigger conditions, their wording (the
//! staleness kind, the measured bloat factor, the named fix each names), that a clean store
//! draws neither and the exit status is unaffected either way, AND two boundary-only properties
//! neither advisory's own unit tests can see from inside their module: the LOG BLOAT advisory's
//! sqlite-only boundary - a server-selected store must draw no warning from a local events.db
//! sitting beside it, regardless of what that local file holds (§48, "one resolution authority":
//! `bloat_advisory_for` gates on the resolved `StoreSelection`, exactly like `reset --derived`
//! itself) - and its CROSS-TYPE trigger condition - the same replay key recorded once under two
//! different covered derived-index types must draw no warning, since the real
//! `rigger reset --derived` reclaims nothing for it (its own compaction partitions duplicates
//! PER TYPE); plus the INDEX STALENESS advisory's real on-disk BACK-COMPAT boundary - a genuine
//! pre-spec-68 `index.json` (missing the `hashes` key entirely, not merely an in-memory struct
//! built via the current API) must load and stay silent. NOT OWNED: the underlying measurement
//! primitives themselves (`rigger::grounder::symbols::staleness`/`compare_to_tree` and
//! `rigger::eventstore::sqlite::Store::measure_derived_duplication`), which carry their own unit
//! tests beside their implementations.

mod common;

use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Event, EventStore, ExpectedRevision};
use rigger::grounder::symbols::model::{Def, FileSymbols, Kind, Lang, SymbolIndex};
use rigger::grounder::symbols::store as symstore;
use std::path::{Path, PathBuf};
use std::process::Command;

// ---------------------------------------------------------------------------------------
// Harness (mirrors tests/reset_derived_compaction.rs's conventions)
// ---------------------------------------------------------------------------------------

/// A throwaway project: its own git repo, so `project_identity()` resolves to the directory's
/// basename exactly as it does for a real project, and a seed appended under that identity lands
/// in the stream the compiled binary reads back.
fn temp_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create temp project");
    let _ = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status();
    std::fs::create_dir_all(dir.path().join(".rigger")).expect("create .rigger");
    dir
}

fn run_stream_identity(root: &Path) -> String {
    let toplevel = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());
    let base = toplevel.as_deref().map(Path::new).unwrap_or(root);
    if let Ok(raw) = std::fs::read_to_string(base.join(".rigger").join("project.id")) {
        let id = raw.trim();
        if !id.is_empty() {
            return id.to_string();
        }
    }
    base.file_name()
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty())
        .map(String::from)
        .unwrap_or_else(|| "rigger".to_string())
}

fn event_log(root: &Path) -> PathBuf {
    root.join(".rigger").join("events.db")
}

/// Run `rigger <args...>` in `cwd`, returning (stdout, stderr, success). The dashboard and the
/// machine-global instance registry are stubbed out so a short-lived invocation never leaves a
/// live process or a phantom registry entry behind.
fn run_rigger(cwd: &Path, args: &[&str]) -> (String, String, bool) {
    let mut cmd = common::rigger_courier();
    cmd.args(args).current_dir(cwd);
    cmd.env("RIGGER_NO_DASH", "1");
    let state = tempfile::tempdir().expect("create a temp XDG_STATE_HOME");
    cmd.env("XDG_STATE_HOME", state.path());
    let out = cmd.output().expect("failed to spawn the rigger binary");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

/// Persist a `symbols` index directly (no tree-sitter needed - the staleness check is ungated),
/// one entry per `(rel_path, content)`, its hash recorded from the CONTENT GIVEN (so a caller can
/// hand a hash that does not match what is on disk, to provoke drift deliberately).
fn persist_index(root: &Path, entries: &[(&str, &str)]) {
    let mut idx = SymbolIndex::default();
    for (path, content) in entries {
        idx.insert_file(
            (*path).to_string(),
            FileSymbols {
                lang: Lang::Rust,
                defs: vec![Def {
                    kind: Kind::Function,
                    name: "f".into(),
                    line: 1,
                }],
                refs: vec![],
            },
        );
        idx.set_hash((*path).to_string(), symstore::content_hash(content));
    }
    symstore::save(&idx, root.to_str().unwrap()).unwrap();
}

/// Seed `root`'s event log with `rounds` recordings of ONE derived-index replay key, in the
/// project's own namespaced stream - the exact duplication `rigger reset --derived` prunes and
/// the bloat advisory measures.
fn seed_duplicated_key(root: &Path, rounds: usize) {
    let backend = Store::open(event_log(root).to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    let mut events: Vec<Event> = vec![Event::new("RunStarted", b"{}".to_vec())];
    for _ in 0..rounds {
        events.push(
            Event::new(
                rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
                b"{}".to_vec(),
            )
            .with_meta(rigger::ingest::META_REPLAY_KEY, "gc/src/a.rs@h1#0"),
        );
    }
    store
        .append(rigger::conductor::STREAM, ExpectedRevision::Any, &events)
        .unwrap();
}

/// Seed `root`'s event log with ONE recording of `key` under EACH of two DIFFERENT covered
/// derived-index types (`TYPE_CODE_ENTITY_EXTRACTED` and `TYPE_EDGE_INFERRED`) - the cross-type
/// scenario `rigger reset --derived`'s real per-type compaction reclaims NOTHING for (each
/// type's own delete only ever sees its own one row), so the bloat measurement's per-type
/// scoping must never merge these into a false duplicate pair.
fn seed_key_under_two_covered_types(root: &Path, key: &str) {
    let backend = Store::open(event_log(root).to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    let events = vec![
        Event::new("RunStarted", b"{}".to_vec()),
        Event::new(
            rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
            b"{}".to_vec(),
        )
        .with_meta(rigger::ingest::META_REPLAY_KEY, key),
        Event::new(rigger::contextgraph::TYPE_EDGE_INFERRED, b"{}".to_vec())
            .with_meta(rigger::ingest::META_REPLAY_KEY, key),
    ];
    store
        .append(rigger::conductor::STREAM, ExpectedRevision::Any, &events)
        .unwrap();
}

// ---------------------------------------------------------------------------------------
// (a) INDEX STALENESS
// ---------------------------------------------------------------------------------------

#[test]
fn validate_warns_of_index_staleness_and_names_reindex() {
    let dir = temp_project();
    let root = dir.path();
    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    // Persist an index over `a.rs` at its ORIGINAL content, then edit the file on disk without
    // reindexing - the drift a path-set-only check cannot see (same path, changed content).
    std::fs::write(root.join("a.rs"), "fn one() {}\n").unwrap();
    persist_index(root, &[("a.rs", "fn one() {}\n")]);
    std::fs::write(root.join("a.rs"), "fn onemodified() {}\n").unwrap();

    let (_out, err, ok) = run_rigger(root, &["validate"]);
    assert!(
        ok,
        "an advisory must never fail validate's exit status; stderr:\n{err}"
    );
    assert!(
        err.to_lowercase().contains("drift") || err.to_lowercase().contains("stale"),
        "validate must warn that the symbols index has drifted; stderr:\n{err}"
    );
    assert!(
        err.contains("rigger reindex"),
        "the staleness warning must name `rigger reindex` as the fix; stderr:\n{err}"
    );
}

#[test]
fn validate_is_silent_on_index_staleness_when_the_index_matches_the_tree() {
    let dir = temp_project();
    let root = dir.path();
    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    let content = "fn one() {}\n";
    std::fs::write(root.join("a.rs"), content).unwrap();
    persist_index(root, &[("a.rs", content)]);

    let (_out, err, ok) = run_rigger(root, &["validate"]);
    assert!(ok, "validate must succeed; stderr:\n{err}");
    assert!(
        !err.contains("rigger reindex"),
        "an index that matches the tree must draw no staleness warning; stderr:\n{err}"
    );
}

#[test]
fn validate_tolerates_a_real_pre_spec68_index_file_with_no_hashes_field() {
    // `SymbolIndex` gained a persisted `hashes` field (spec 68) alongside its pre-existing
    // `files` field. An index written by a binary from BEFORE this field existed has no
    // "hashes" key at all on disk. This drives the REAL persisted file (not an in-memory
    // struct built via the current `set_hash`) through the compiled binary, proving an
    // operator's pre-upgrade index still loads without crashing and never manufactures a
    // false staleness warning from the field's mere absence - `#[serde(default)]` must let it
    // load, and every path's hash reads as "unknown", which is nothing to compare against.
    let dir = temp_project();
    let root = dir.path();
    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let content = "fn one() {}\n";
    std::fs::write(root.join("a.rs"), content).unwrap();
    persist_index(root, &[("a.rs", content)]);

    // Strip the "hashes" key from the REAL persisted file, simulating exactly what a
    // pre-spec-68 binary would have written - every other field of `SymbolIndex` predates
    // this spec, so the rest of the file is unchanged.
    let index_path = root.join(".rigger").join("symbols").join("index.json");
    let raw = std::fs::read_to_string(&index_path).expect("read the persisted index");
    let mut value: serde_json::Value = serde_json::from_str(&raw).expect("index is valid JSON");
    value
        .as_object_mut()
        .expect("a symbols index serializes as a JSON object")
        .remove("hashes");
    std::fs::write(&index_path, serde_json::to_string(&value).unwrap())
        .expect("rewrite the index without its hashes field");

    let (_out, err, ok) = run_rigger(root, &["validate"]);
    assert!(
        ok,
        "validate must succeed against a pre-upgrade index; stderr:\n{err}"
    );
    assert!(
        !err.contains("rigger reindex"),
        "a pre-upgrade index missing the hashes field must never manufacture a false \
         staleness warning from the absence alone; stderr:\n{err}"
    );
}

// ---------------------------------------------------------------------------------------
// (b) LOG BLOAT
// ---------------------------------------------------------------------------------------

#[test]
fn validate_warns_of_log_bloat_with_the_measured_factor_and_names_reset_derived() {
    let dir = temp_project();
    let root = dir.path();
    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    seed_duplicated_key(root, 6);

    let (_out, err, ok) = run_rigger(root, &["validate"]);
    assert!(
        ok,
        "an advisory must never fail validate's exit status; stderr:\n{err}"
    );
    assert!(
        err.to_lowercase().contains("duplicat") || err.to_lowercase().contains("bloat"),
        "validate must warn of derived-index duplication; stderr:\n{err}"
    );
    assert!(
        err.contains("6.0"),
        "the warning must carry the MEASURED factor (6 rows / 1 distinct key = 6.0x); \
         stderr:\n{err}"
    );
    assert!(
        err.contains("rigger reset --derived"),
        "the bloat warning must name `rigger reset --derived` as the fix; stderr:\n{err}"
    );
}

#[test]
fn validate_is_silent_on_log_bloat_when_every_key_is_recorded_once() {
    let dir = temp_project();
    let root = dir.path();
    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    seed_duplicated_key(root, 1);

    let (_out, err, ok) = run_rigger(root, &["validate"]);
    assert!(ok, "validate must succeed; stderr:\n{err}");
    assert!(
        !err.contains("rigger reset --derived"),
        "a log with no duplication must draw no bloat warning; stderr:\n{err}"
    );
}

#[test]
fn validate_is_silent_on_log_bloat_when_the_same_key_recurs_only_across_different_covered_types() {
    // spec 68 Global constraints: "one measurement authority per advisory ... no shadow
    // accounting". The real compaction (`rigger reset --derived` / `prune_derived_index`)
    // deletes duplicates PER COVERED TYPE - its own per-type loop only ever compares a key
    // against OTHER ROWS OF THE SAME TYPE. The SAME replay key recorded once under two
    // DIFFERENT covered types (here, a code-entity extraction and an inferred edge) is
    // therefore two independent single-row groups to the real prune, which reclaims NOTHING
    // for it - so the bloat advisory must draw no warning either. A measurement that merges
    // duplicate-detection ACROSS types would read this as one key recorded twice (a false
    // factor of 2.0) and warn of bloat a real `rigger reset --derived` could never reclaim -
    // exactly the shadow, independently-re-derived definition of "duplicated" the design
    // forbids.
    let dir = temp_project();
    let root = dir.path();
    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    seed_key_under_two_covered_types(root, "gc/src/a.rs@h1#0");

    let (_out, err, ok) = run_rigger(root, &["validate"]);
    assert!(
        ok,
        "an advisory must never fail validate's exit status; stderr:\n{err}"
    );
    assert!(
        !err.contains("rigger reset --derived"),
        "the same key recorded once under two different covered types is not duplication a \
         real prune can reclaim, and must draw no bloat warning; stderr:\n{err}"
    );
}

#[test]
fn validate_is_silent_on_log_bloat_when_the_store_is_server_selected() {
    // The bloat advisory is a SQLITE-ONLY mechanic (`bloat_advisory_for`'s own doc: "None on a
    // server-backed project ... a sqlite-only mechanic, exactly like `reset --derived` itself").
    // A server-selected store must never fabricate this warning from a LOCAL events.db that
    // happens to sit beside it - this proves the guard gates on the resolved `StoreSelection`
    // itself, not merely on whether a local file with duplication happens to exist (the sibling
    // tests above already cover THAT half with no `KURRENTDB_CONN` set at all).
    let dir = temp_project();
    let root = dir.path();
    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    // Seed the SAME heavy local duplication the sqlite-selected warn test above proves draws a
    // warning - so any warning here would be unambiguous evidence the sqlite-only guard leaked.
    seed_duplicated_key(root, 6);

    let mut cmd = common::rigger_courier();
    cmd.arg("validate")
        .current_dir(root)
        .env("RIGGER_NO_DASH", "1")
        .env(
            // A well-formed but unreachable address - nothing listens here. `bloat_advisory_for`
            // must skip on the selection alone, BEFORE any connect attempt, so an unreachable
            // server can never turn into a hang or a spurious failure (mirrors the unreachable-
            // server convention `tests/store_resolution_cli.rs` establishes for this exact class
            // of guard).
            "KURRENTDB_CONN",
            "kurrentdb://127.0.0.1:65533?tls=false",
        );
    let state = tempfile::tempdir().expect("create a temp XDG_STATE_HOME");
    cmd.env("XDG_STATE_HOME", state.path());
    let out = cmd.output().expect("failed to spawn the rigger binary");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "an advisory must never fail validate's exit status, even server-selected; stderr:\n{err}"
    );
    assert!(
        !err.contains("rigger reset --derived"),
        "a server-selected store must draw no bloat warning from a local events.db sitting \
         beside it, regardless of its duplication; stderr:\n{err}"
    );
}

// ---------------------------------------------------------------------------------------
// A CLEAN store draws neither advisory, and the exit status is unchanged either way
// ---------------------------------------------------------------------------------------

#[test]
fn a_clean_store_with_no_symbols_index_and_no_duplication_draws_neither_advisory() {
    let dir = temp_project();
    let root = dir.path();
    // No persisted symbols index at all, and no seeded event log - the state `rigger init`
    // itself leaves a fresh project in.
    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let (out, err, ok) = run_rigger(root, &["validate"]);
    assert!(
        ok,
        "validate must succeed on a clean project; stderr:\n{err}"
    );
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );
    assert!(
        !err.contains("rigger reindex"),
        "a project with no persisted symbols index must draw no staleness warning; stderr:\n{err}"
    );
    assert!(
        !err.contains("rigger reset --derived"),
        "a project with no duplication must draw no bloat warning; stderr:\n{err}"
    );
}
