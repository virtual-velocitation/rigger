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
//! built via the current API) must load and stay silent; plus the GRAPH INDEX LAG sample's
//! candidate set as the operator sees it (spec 107, THE TREE IS READ BY ONE RULE) - a recorded
//! path outside the walk's scope, or one the read fault makes unreadable, is never named and
//! takes no slot of the bounded sample. NOT OWNED: the underlying measurement
//! primitives themselves (`rigger::grounder::symbols::staleness`/`compare_to_tree` and
//! `rigger::eventstore::sqlite::Store::measure_derived_duplication`), which carry their own unit
//! tests beside their implementations.

mod common;

use common::cli::rigger_file;
use common::cli::run_rigger;
use common::cli::run_stream_identity;
use common::cli::temp_rigger_project;
use common::cli::validate_after_init;
use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Event, EventStore, ExpectedRevision};
use rigger::grounder::symbols::model::{Def, FileSymbols, Kind, Lang, SymbolIndex};
use rigger::grounder::symbols::store as symstore;
use std::path::Path;

// ---------------------------------------------------------------------------------------
// Harness (mirrors tests/reset_derived_compaction.rs's conventions)
// ---------------------------------------------------------------------------------------

/// Persist a `symbols` index directly (no tree-sitter needed - the staleness check is ungated),
/// one entry per `(rel_path, content)`, its hash recorded from the CONTENT GIVEN (so a caller can
/// hand a hash that does not match what is on disk, to provoke drift deliberately).
fn persist_index(root: &Path, entries: &[(&str, &str)]) {
    let mut idx = SymbolIndex::default();
    for (path, content) in entries {
        idx.insert_hashed_file(
            (*path).to_string(),
            FileSymbols {
                lang: Lang::Rust,
                defs: vec![Def {
                    kind: Kind::Function,
                    name: "f".into(),
                    line: 1,
                    is_test: false,
                    is_out_of_line_module: false,
                    path_override: None,
                    enclosing_inline_module_path: None,
                }],
                refs: vec![],
                partial: false,
            },
            symstore::content_hash(content),
        );
    }
    symstore::save(&idx, root.to_str().unwrap()).unwrap();
}

/// Seed `root`'s event log with `rounds` recordings of ONE derived-index replay key, in the
/// project's own namespaced stream - the exact duplication `rigger reset --derived` prunes and
/// the bloat advisory measures.
fn seed_duplicated_key(root: &Path, rounds: usize) {
    seed_derived_keys(root, &vec!["gc/src/a.rs@h1#0"; rounds]);
}

/// Seed one `CodeEntityExtracted` per entry of `keys`, in order, each carrying that replay key.
fn seed_derived_keys(root: &Path, keys: &[&str]) {
    let backend = Store::open(rigger_file(root, "events.db").to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    let mut events: Vec<Event> = vec![Event::new("RunStarted", b"{}".to_vec())];
    for &key in keys {
        events.push(
            Event::new(
                rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
                b"{}".to_vec(),
            )
            .with_meta(rigger::ingest::META_REPLAY_KEY, key),
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
    let backend = Store::open(rigger_file(root, "events.db").to_str().unwrap()).unwrap();
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

/// `rigger validate`'s stderr over a fresh `rigger init` project that `prepare` then edited;
/// validate must still succeed - an advisory never fails its exit status.
fn validate_stderr_after(prepare: impl FnOnce(&Path)) -> String {
    let dir = temp_rigger_project();
    let root = dir.path();
    let (_out, err) = validate_after_init(root, prepare);
    err
}

/// Validate's stderr after `prepare` carries an advisory naming (case-insensitively) at least one
/// of `any_of` (`warning_why`), and every `(needle, why)` of `names` - the fix it points at among
/// them.
fn assert_validate_advises(
    prepare: impl FnOnce(&Path),
    any_of: &[&str],
    warning_why: &str,
    names: &[(&str, &str)],
) {
    let err = validate_stderr_after(prepare);
    let lower = err.to_lowercase();
    assert!(
        any_of.iter().any(|w| lower.contains(w)),
        "{warning_why}; stderr:\n{err}"
    );
    for (needle, why) in names {
        assert!(err.contains(needle), "{why}; stderr:\n{err}");
    }
}

rigger::test_cases! {
    /// Persist an index over `a.rs` at its ORIGINAL content, then edit the file on disk without
    /// reindexing - the drift a path-set-only check cannot see (same path, changed content).
    validate_warns_of_index_staleness_and_names_reindex: assert_validate_advises(
        |root| {
            std::fs::write(root.join("a.rs"), "fn one() {}\n").unwrap();
            persist_index(root, &[("a.rs", "fn one() {}\n")]);
            std::fs::write(root.join("a.rs"), "fn onemodified() {}\n").unwrap();
        },
        &["drift", "stale"],
        "validate must warn that the symbols index has drifted",
        &[(
            "rigger reindex",
            "the staleness warning must name `rigger reindex` as the fix",
        )],
    );
    /// The `symbols` feature is what compiles the extraction pass `ingest_files_batched` needs to
    /// find `fn original() {}`/`fn renamed() {}` as real definitions in the first place (mirrors
    /// [`locate_definition_extent`]'s own light-lane stub, main.rs): the light lane's
    /// `graph_index_lag_sample` is unconditionally a no-op stub, exactly like its INDEX STALENESS
    /// counterpart is NOT (that one is content-hash-only, ungated) - so this positive case is
    /// `symbols`-only; the two SILENT cases below hold in both lanes (nothing can ever disagree in
    /// the light lane, so "no warning" is trivially true there too).
    ///
    /// The graph recorded churn.rs's ORIGINAL content, then the file was edited on disk without an
    /// integration ever reindexing it into the graph - the exact drift the audit
    /// (docs/audit/2026-09-graph-vs-grep.md, findings 9/11/12) found: a `graph.db` generation the
    /// tree has since moved past.
    #[cfg(feature = "symbols")]
    validate_warns_of_graph_index_lag_and_names_reindex: assert_validate_advises(
        |root| {
            std::fs::write(root.join("churn.rs"), "fn original() {}\n").unwrap();
            seed_graph_generation(root, "churn.rs");
            std::fs::write(root.join("churn.rs"), "fn renamed() {}\n").unwrap();
        },
        &["context graph", "graph index lag", "fallen behind"],
        "validate must warn that the context graph has fallen behind",
        &[
            ("churn.rs", "the warning must name the lagging file"),
            (
                "rigger reindex",
                "the graph-lag warning must name `rigger reindex` as the fix",
            ),
        ],
    );
}

#[test]
fn validate_is_silent_on_index_staleness_when_the_index_matches_the_tree() {
    let dir = temp_rigger_project();
    let root = dir.path();
    let (_out, err) = validate_after_init(root, |root| {
        let content = "fn one() {}\n";
        std::fs::write(root.join("a.rs"), content).unwrap();
        persist_index(root, &[("a.rs", content)]);
    });
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
    // struct built via the current `insert_hashed_file`) through the compiled binary, proving an
    // operator's pre-upgrade index still loads without crashing and never manufactures a
    // false staleness warning from the field's mere absence - `#[serde(default)]` must let it
    // load, and every path's hash reads as "unknown", which is nothing to compare against.
    let dir = temp_rigger_project();
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
    let dir = temp_rigger_project();
    let root = dir.path();
    let (_out, err) = validate_after_init(root, |root| seed_duplicated_key(root, 6));
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

/// Spec 101, criterion 4: the advisory measures the ONE selection `rigger reset --derived` acts
/// on, so a log whose every key is recorded once but which holds five superseded generations of
/// one file (six generations, only the latest of which a compaction keeps) is 6.0x bloated.
#[test]
fn validate_warns_of_log_bloat_on_a_log_holding_only_superseded_generations() {
    let dir = temp_rigger_project();
    let root = dir.path();
    let (_out, err) = validate_after_init(root, |root| {
        seed_derived_keys(
            root,
            &[
                "gc/src/a.rs@h1#0",
                "gc/src/a.rs@h2#0",
                "gc/src/a.rs@h3#0",
                "gc/src/a.rs@h4#0",
                "gc/src/a.rs@h5#0",
                "gc/src/a.rs@h6#0",
            ],
        )
    });
    assert!(
        err.contains("6.0") && err.contains("rigger reset --derived"),
        "six generations of which a compaction keeps one must warn at the measured 6.0x and name \
         `rigger reset --derived`; stderr:\n{err}"
    );
}

/// `rigger validate` over an initialized project whose log `seed` shaped succeeds (`ok_why`
/// when it does not) and draws no bloat warning (`why` when it does).
fn assert_validate_draws_no_bloat_warning(seed: impl FnOnce(&Path), ok_why: &str, why: &str) {
    let dir = temp_rigger_project();
    let root = dir.path();
    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    seed(root);

    let (_out, err, ok) = run_rigger(root, &["validate"]);
    assert!(ok, "{ok_why}; stderr:\n{err}");
    assert!(
        !err.contains("rigger reset --derived"),
        "{why}; stderr:\n{err}"
    );
}

rigger::test_cases! {
    validate_is_silent_on_log_bloat_when_every_key_is_recorded_once: assert_validate_draws_no_bloat_warning(
        |root| seed_duplicated_key(root, 1),
        "validate must succeed",
        "a log with no duplication must draw no bloat warning",
    );
    /// spec 68 Global constraints: "one measurement authority per advisory ... no shadow
    /// accounting". The real compaction (`rigger reset --derived` / `prune_derived_index`)
    /// deletes duplicates PER COVERED TYPE - its own per-type loop only ever compares a key
    /// against OTHER ROWS OF THE SAME TYPE. The SAME replay key recorded once under two
    /// DIFFERENT covered types (here, a code-entity extraction and an inferred edge) is
    /// therefore two independent single-row groups to the real prune, which reclaims NOTHING
    /// for it - so the bloat advisory must draw no warning either. A measurement that merges
    /// duplicate-detection ACROSS types would read this as one key recorded twice (a false
    /// factor of 2.0) and warn of bloat a real `rigger reset --derived` could never reclaim -
    /// exactly the shadow, independently-re-derived definition of "duplicated" the design
    /// forbids.
    validate_is_silent_on_log_bloat_when_the_same_key_recurs_only_across_different_covered_types: assert_validate_draws_no_bloat_warning(
        |root| seed_key_under_two_covered_types(root, "gc/src/a.rs@h1#0"),
        "an advisory must never fail validate's exit status",
        "the same key recorded once under two different covered types is not duplication a \
         real prune can reclaim, and must draw no bloat warning",
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
    let dir = temp_rigger_project();
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
// (c) GRAPH INDEX LAG (spec 92 criterion 1, FRESH ON EVERY INTEGRATION)
// ---------------------------------------------------------------------------------------

/// Record `file`'s CURRENT extraction as `graph.db`'s "latest generation" for it, by appending
/// real `gc/<file>@<hash>#<i>`-keyed events into the project's own event stream - the keyed
/// derived shape a cold `rigger graph build` appends. Drives it through the SAME `rigger::ingest::ingest_files_batched` authority
/// `rigger::ingest::graph_index_lag_sample` re-extracts through at validate time, so a caller who
/// seeds a file's CURRENT content and never edits it afterward is recording a graph that agrees
/// with the tree; editing the file afterward (without re-seeding) is what provokes disagreement.
fn seed_graph_generation(root: &Path, file: &str) {
    let backend = Store::open(rigger_file(root, "events.db").to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    let mut events: Vec<Event> = vec![Event::new("RunStarted", b"{}".to_vec())];
    rigger::ingest::ingest_files_batched(
        root.to_str().unwrap(),
        &[file.to_string()],
        |keyed, _| {
            for (key, ev) in keyed {
                events.push(
                    (*ev)
                        .clone()
                        .with_meta(rigger::ingest::META_REPLAY_KEY, key.as_str()),
                );
            }
        },
    );
    store
        .append(rigger::conductor::STREAM, ExpectedRevision::Any, &events)
        .unwrap();
}

/// A project `prepare` left in a state with nothing to compare, or nothing that disagrees, draws
/// no graph index-lag warning from validate (`why`).
fn assert_validate_is_silent_on_graph_index_lag(prepare: impl FnOnce(&Path), why: &str) {
    let err = validate_stderr_after(prepare);
    assert!(
        !err.to_lowercase().contains("fallen behind"),
        "{why}; stderr:\n{err}"
    );
}

rigger::test_cases! {
    /// The graph recorded churn.rs's CURRENT content, and it is never edited afterward - a fresh
    /// graph, exactly what an integration that just reindexed it leaves behind.
    validate_is_silent_on_graph_index_lag_when_the_graph_matches_the_tree:
        assert_validate_is_silent_on_graph_index_lag(
            |root| {
                std::fs::write(root.join("churn.rs"), "fn stable() {}\n").unwrap();
                seed_graph_generation(root, "churn.rs");
            },
            "a graph that agrees with the tree must draw no index-lag warning",
        );
    /// No `gc/`-keyed event was ever recorded (no integration has run yet) - there is nothing to
    /// compare, so this must never manufacture a warning from the mere absence of a graph.
    validate_is_silent_on_graph_index_lag_when_the_graph_has_recorded_nothing:
        assert_validate_is_silent_on_graph_index_lag(
            |root| std::fs::write(root.join("untracked.rs"), "fn untracked() {}\n").unwrap(),
            "a project the graph has never indexed must draw no index-lag warning",
        );
}

/// Plant each of `files` under `root` as a small Rust source and record a STALE `gc` generation
/// of every one (`gc/<file>@stale#0`, a generation no extraction yields), so the advisory names
/// every one of them the sample takes as a candidate: a recorded path the advisory leaves out
/// was never sampled.
#[cfg(feature = "symbols")]
fn plant_stale_recordings(root: &Path, files: &[&str]) {
    for file in files {
        common::fixtures::write_file(&root.join(file), b"fn planted() {}\n");
    }
    let keys: Vec<String> = files
        .iter()
        .map(|file| format!("gc/{file}@stale#0"))
        .collect();
    seed_derived_keys(root, &keys.iter().map(String::as_str).collect::<Vec<_>>());
}

/// `stderr` carries exactly ONE graph index-lag advisory line, the one naming `files` in that
/// order (`why`).
#[cfg(feature = "symbols")]
fn assert_names_as_lagging(stderr: &str, files: &[&str], why: &str) {
    let advisories: Vec<&str> = stderr
        .lines()
        .filter(|line| line.contains("fallen behind"))
        .collect();
    let expected = format!(
        "warning: the context graph has fallen behind {} sampled file(s) it previously indexed \
         ({}). Run `rigger reindex <file>...` to refresh it.",
        files.len(),
        files.join(", "),
    );
    assert_eq!(
        advisories,
        vec![expected.as_str()],
        "{why}; stderr:\n{stderr}"
    );
}

/// Validate's stderr over a fresh `rigger init` project holding a stale recording of each of
/// `files` names exactly `lagging` (`why`); the project is handed back for a second act.
#[cfg(feature = "symbols")]
fn project_validated_with_stale_recordings(
    files: &[&str],
    lagging: &[&str],
    why: &str,
) -> tempfile::TempDir {
    let dir = temp_rigger_project();
    let (_out, err) = validate_after_init(dir.path(), |root| plant_stale_recordings(root, files));
    assert_names_as_lagging(&err, lagging, why);
    dir
}

/// A later `rigger validate` in `root` still exits 0 and names exactly `lagging` (`why`).
#[cfg(feature = "symbols")]
fn assert_validate_now_names_as_lagging(root: &Path, lagging: &[&str], why: &str) {
    let (_out, err, ok) = run_rigger(root, &["validate"]);
    assert!(
        ok,
        "validate must exit 0 (an advisory never fails it); stderr:\n{err}"
    );
    assert_names_as_lagging(&err, lagging, why);
}

/// Given a graph that recorded a path under a hidden directory and one a committed `.gitignore`
/// comes to name, when the operator validates, then the advisory names only the recorded paths
/// inside the walk's scope (spec 107, THE TREE IS READ BY ONE RULE): the ignored path is named
/// until the `.gitignore` names it and never after, and the hidden one never.
#[cfg(feature = "symbols")]
#[test]
fn validate_samples_no_recorded_path_outside_the_walk_scope() {
    let dir = project_validated_with_stale_recordings(
        &[".hidden/h.rs", "ignored.rs", "kept.rs"],
        &["ignored.rs", "kept.rs"],
        "the two recorded paths inside the scope are sampled and the hidden one is not",
    );
    let root = dir.path();

    let gitignore = root.join(".gitignore");
    let mut rules = std::fs::read_to_string(&gitignore).unwrap_or_default();
    rules.push_str("ignored.rs\n");
    std::fs::write(&gitignore, rules).unwrap();

    assert_validate_now_names_as_lagging(
        root,
        &["kept.rs"],
        "a recorded path the committed .gitignore names is sampled no more",
    );
}

/// Given a graph that recorded a file THE READ FAULT then makes unreadable, when the operator
/// validates, then validate still exits 0 and the advisory names only the readable recording:
/// the unreadable one is named until the fault is armed and never after.
#[cfg(feature = "symbols")]
#[test]
fn validate_samples_no_recorded_path_the_read_fault_makes_unreadable() {
    let dir = project_validated_with_stale_recordings(
        &["kept.rs", "locked.rs"],
        &["kept.rs", "locked.rs"],
        "both recorded paths are sampled while both can be read",
    );
    let root = dir.path();

    if !common::fixtures::arm_read_fault(&root.join("locked.rs")) {
        return;
    }

    assert_validate_now_names_as_lagging(
        root,
        &["kept.rs"],
        "the recorded path the read fault makes unreadable is sampled no more",
    );
}

rigger::test_cases! {
    /// A recorded path outside the walk's scope is left OUT of the candidates, never merely filtered
    /// from the answer, so it takes no slot of the bounded sample: with a hidden recording sorting
    /// ahead of nine in-scope ones, the advisory names the first eight in-scope paths - the eighth
    /// (`h.rs`) is the one a slot spent on the hidden path would cost, and the ninth (`i.rs`) is the
    /// one past the bound.
    #[cfg(feature = "symbols")]
    validate_spends_no_sample_slot_on_a_recorded_path_outside_the_walk_scope:
        project_validated_with_stale_recordings(
            &[
                ".hidden/first.rs", "a.rs", "b.rs", "c.rs", "d.rs", "e.rs", "f.rs", "g.rs", "h.rs",
                "i.rs",
            ],
            &["a.rs", "b.rs", "c.rs", "d.rs", "e.rs", "f.rs", "g.rs", "h.rs"],
            "the hidden recording takes no sample slot and the ninth in-scope path is past the \
             bound",
        );
}

// ---------------------------------------------------------------------------------------
// (d) NO UNGATED FAN-OUT TEMPLATE (spec 103, criterion 2)
// ---------------------------------------------------------------------------------------

/// Declare one gate in the REAL persisted `.rigger/workflow.yml` `rigger init` just wrote for a
/// project matching no gate template set, and list it on the `implement` fan-out template: the
/// fixture's own gate, since a scaffold matching no set declares none (`gates: {}`) and lists
/// `[]` on its unit stages, of which `implement` comes first.
fn declare_an_implement_gate(root: &Path) {
    let path = root.join(".rigger").join("workflow.yml");
    let raw = std::fs::read_to_string(&path).expect("read the scaffolded workflow");
    for needle in ["gates: {}", "gates: []"] {
        assert!(
            raw.contains(needle),
            "fixture bug: the no-set scaffold no longer carries {needle:?}; workflow.yml:\n{raw}"
        );
    }
    let gated = raw
        .replacen(
            "gates: {}",
            "gates:\n  unit-check: { run: \"make check\", kind: core }",
            1,
        )
        .replacen("gates: []", "gates: [unit-check]", 1);
    std::fs::write(&path, gated).expect("rewrite workflow.yml with a gated implement template");
}

#[test]
fn validate_warns_of_an_ungated_fanout_template_and_names_it() {
    let dir = temp_rigger_project();
    let root = dir.path();
    // A project matching no gate template set: its scaffolded `implement` template lists no gate.
    let (_out, err) = validate_after_init(root, |_| {});
    assert!(
        err.contains("fan-out template 'implement' declares no gates"),
        "validate must warn of the ungated fan-out template, naming it; stderr:\n{err}"
    );
    assert!(
        err.contains("gates:"),
        "the warning must name the fix (adding a `gates:` list); stderr:\n{err}"
    );
    // The scaffold's OTHER gate-less stages - `plan` (a producer: `produces: dag`) and
    // `plan-critique` (review-only: no `agent`) - are not fan-out templates at all and must
    // draw no warning of their own. Proven against the REAL, multi-stage scaffolded config
    // (never a synthetic single-stage fixture), so this is the only place `is_fan_out_template`'s
    // full predicate is exercised against real coexisting stage shapes that could plausibly be
    // confused for a fan-out template.
    assert_eq!(
        err.matches("declares no gates").count(),
        1,
        "only the one genuine ungated fan-out template may be named; stderr:\n{err}"
    );
    assert!(
        !err.contains("template 'plan'") && !err.contains("template 'plan-critique'"),
        "a non-fan-out stage with no gates must never be misidentified as an ungated fan-out \
         template; stderr:\n{err}"
    );
}

#[test]
fn validate_is_silent_on_a_gated_scaffolded_fanout_template() {
    let dir = temp_rigger_project();
    let root = dir.path();
    let (_out, err) = validate_after_init(root, declare_an_implement_gate);
    assert!(
        !err.contains("declares no gates"),
        "a scaffolded `implement` template that lists a gate must draw no warning; \
         stderr:\n{err}"
    );
}

// ---------------------------------------------------------------------------------------
// A CLEAN store draws neither advisory, and the exit status is unchanged either way
// ---------------------------------------------------------------------------------------

#[test]
fn a_clean_store_with_no_symbols_index_and_no_duplication_draws_neither_advisory() {
    let dir = temp_rigger_project();
    let root = dir.path();
    // No persisted symbols index at all, and no seeded event log - the state `rigger init`
    // itself leaves a fresh project in, with the fixture's own gate on its fan-out template.
    let (out, err) = validate_after_init(root, declare_an_implement_gate);
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
    assert!(
        !err.to_lowercase().contains("fallen behind"),
        "a project the graph has never indexed must draw no graph-index-lag warning; stderr:\n{err}"
    );
    assert!(
        !err.contains("declares no gates"),
        "a freshly-scaffolded `implement` template listing a gate must draw no \
         ungated-fan-out-template warning; stderr:\n{err}"
    );
}
