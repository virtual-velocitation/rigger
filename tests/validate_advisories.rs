//! CLI periphery for spec 68, criterion 4 - VALIDATE ADVISORIES.
//!
//! `rigger validate` gains two advisory-only checks (Design):
//!   (a) INDEX STALENESS - the persisted `symbols` grounding index has drifted from the tree,
//!       warn and name `rigger reindex`.
//!   (b) LOG BLOAT - the event log still holds a derived event (spec 107), warn naming the
//!       events and file identities left and `rigger reset --derived`, the migration.
//!
//! Both are stderr-only warnings that NEVER change `validate`'s exit status (Design: "advisory-
//! warn, never failing"). These tests drive the compiled binary so the observable surface -
//! stdout/stderr/exit code - is pinned exactly as an operator sees it.
//!
//! What this file OWNS (criterion 4): the two advisories' trigger conditions, their wording (the
//! staleness kind, the derived events and file identities left, the named fix each names), that
//! a clean store draws neither and the exit status is unaffected either way, AND boundary-only
//! properties neither advisory's own unit tests can see from inside their module: the LOG BLOAT
//! advisory's sqlite-only boundary - a server-selected store must draw no warning from a local
//! events.db sitting beside it, regardless of what that local file holds ("one resolution
//! authority": `bloat_advisory_for` gates on the resolved `StoreSelection`, exactly like `reset
//! --derived` itself) - and its COUNT, every derived event of the project's run stream however
//! its key or type repeats, since `rigger reset --derived` sheds every one of them; plus the
//! INDEX STALENESS advisory's real on-disk BACK-COMPAT boundary - a genuine
//! pre-spec-68 `index.json` (missing the `hashes` key entirely, not merely an in-memory struct
//! built via the current API) must load and stay silent; plus the GRAPH INDEX LAG sample's
//! candidate set as the operator sees it (spec 107, THE TREE IS READ BY ONE RULE) - a recorded
//! path outside the walk's scope, or one the read fault makes unreadable, is never named and
//! takes no slot of the bounded sample. NOT OWNED: the underlying measurement
//! primitives themselves (`rigger::grounder::symbols::staleness`/`compare_to_tree` and
//! `rigger::eventstore::sqlite::Store::count_derived`), which carry their own unit tests beside
//! their implementations.
//!
//! It also owns spec 105's CLAUDE LOGIN report (A PERSON SEES THE HOLD): the auth method
//! `rigger validate` reads from `claude auth status --json` through the one login reader, the
//! expiring-login warning pinned by a report recorded from the installed `claude` and cut to the
//! key the pin reads, and the one warning printed when `claude` is absent or gives no report, the
//! exit status unchanged on every row: 0 on a sound project, and a failure still on a project whose
//! run store cannot be selected. The reader's `loggedIn` key and its deadline are tested beside it
//! in `crates/rigger-driver/src/claude_auth.rs`; its `authMethod` key is tested only here, through
//! the binary.

mod common;

use common::cli::rigger_file;
use common::cli::run_rigger;
use common::cli::temp_rigger_project;
use common::cli::validate_after_init;
use rigger::eventstore::{Event, ExpectedRevision};
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
                    end_line: 1,
                    path: "f".into(),
                    parent: None,
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
/// project's own namespaced stream - `rounds` derived events of one file identity, each of which
/// `rigger reset --derived` sheds and the bloat advisory counts.
fn seed_duplicated_key(root: &Path, rounds: usize) {
    seed_derived_keys(root, &vec!["gc/src/a.rs@h1#0"; rounds]);
}

/// Seed one `CodeEntityExtracted` per entry of `keys`, in order, each carrying that replay key.
fn seed_derived_keys(root: &Path, keys: &[&str]) {
    seed_derived_events(root, &keys.iter().copied().map(Some).collect::<Vec<_>>());
}

/// Seed one `CodeEntityExtracted` per entry of `keys`, in order, each carrying that replay key
/// or, for `None`, no replay key at all: a derived event naming no file identity.
fn seed_derived_events(root: &Path, keys: &[Option<&str>]) {
    let mut events: Vec<Event> = vec![Event::new("RunStarted", b"{}".to_vec())];
    for &key in keys {
        let event = Event::new(
            rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
            b"{}".to_vec(),
        );
        events.push(match key {
            Some(key) => event.with_meta(rigger::ingest::META_REPLAY_KEY, key),
            None => event,
        });
    }
    common::cli::seed_pre_ledger_events(root, &events);
}

/// Seed `root`'s event log with ONE recording of `key` under EACH of two DIFFERENT derived-index
/// types (`TYPE_CODE_ENTITY_EXTRACTED` and `TYPE_EDGE_INFERRED`): two derived events of the one
/// file identity `key` names, each of which `rigger reset --derived` sheds.
fn seed_key_under_two_covered_types(root: &Path, key: &str) {
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
    common::cli::seed_pre_ledger_events(root, &events);
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
    /// The `symbols` feature is what compiles the extraction pass the advisory needs to find
    /// `fn original() {}`/`fn renamed() {}` as real definitions in the first place: the light
    /// lane's `graph_index_lag_sample` is unconditionally a no-op stub, exactly like its INDEX
    /// STALENESS counterpart is NOT (that one is content-hash-only, ungated) - so this positive
    /// case is `symbols`-only; the two SILENT cases below hold in both lanes (the light lane
    /// samples nothing, whatever the log records).
    ///
    /// The log's latest entry records churn.rs's ORIGINAL content, then the file was edited on disk
    /// without an integration ever reindexing it - the exact drift the audit
    /// (docs/audit/2026-09-graph-vs-grep.md, findings 9/11/12) found: a recorded generation the
    /// tree has since moved past.
    #[cfg(feature = "symbols")]
    validate_warns_of_graph_index_lag_and_names_graph_build: assert_validate_advises(
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
                "Run `rigger graph build` to refresh it.",
                "the graph-lag warning must name `rigger graph build`, which records and folds \
                 the lagging entry, as the fix",
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

/// Validate over an initialized project whose log `seed` shaped exits 0 and warns of log bloat
/// exactly once, on the line naming `events` derived events of `identities` file identities
/// left and `rigger reset --derived` (`why`).
fn assert_validate_warns_of_log_bloat(
    seed: impl FnOnce(&Path),
    events: usize,
    identities: usize,
    why: &str,
) {
    let err = validate_stderr_after(seed);
    assert_eq!(
        common::cli::bloat_lines(&err),
        [common::cli::bloat_advisory_naming(events, identities)],
        "{why}; stderr:\n{err}"
    );
}

rigger::test_cases! {
    /// Any derived event is one the migration sheds, so a single recording already warns: no
    /// threshold stands between a store holding a derived event and its advisory.
    validate_warns_of_log_bloat_on_a_log_holding_one_derived_event: assert_validate_warns_of_log_bloat(
        |root| seed_duplicated_key(root, 1),
        1,
        1,
        "one derived event of one file identity is named",
    );
    validate_warns_of_log_bloat_counting_every_recording_of_one_key: assert_validate_warns_of_log_bloat(
        |root| seed_duplicated_key(root, 6),
        6,
        1,
        "six recordings of one key are six derived events of one file identity",
    );
    /// A log whose every key is recorded once but which holds six generations of one file is
    /// six derived events of that one identity: the migration sheds all six, the latest too.
    validate_warns_of_log_bloat_on_a_log_holding_only_superseded_generations: assert_validate_warns_of_log_bloat(
        |root| seed_derived_keys(
            root,
            &[
                "gc/src/a.rs@h1#0",
                "gc/src/a.rs@h2#0",
                "gc/src/a.rs@h3#0",
                "gc/src/a.rs@h4#0",
                "gc/src/a.rs@h5#0",
                "gc/src/a.rs@h6#0",
            ],
        ),
        6,
        1,
        "six generations of one file are six derived events of one file identity",
    );
    /// The same replay key recorded once under two different derived types is two derived
    /// events, both shed by `rigger reset --derived`, of the one identity the key names.
    validate_warns_of_log_bloat_counting_a_key_recorded_under_two_derived_types: assert_validate_warns_of_log_bloat(
        |root| seed_key_under_two_covered_types(root, "gc/src/a.rs@h1#0"),
        2,
        1,
        "one key under two derived types is two derived events of one file identity",
    );
    /// A file identity is its prefix and file, so a `gc` and a `gd` batch of one file are two
    /// and a second file a third.
    validate_warns_of_log_bloat_naming_each_file_identity_once: assert_validate_warns_of_log_bloat(
        |root| seed_derived_keys(root, &["gc/src/a.rs@h1#0", "gc/src/b.rs@h1#0", "gd/src/a.rs@d1#0"]),
        3,
        3,
        "three recordings of three identities, a gc and a gd batch of one file being two",
    );
    /// A derived event is left whether or not it names a file identity: a log whose derived
    /// events all carry no replay key holds two events of no identity, and is warned of.
    validate_warns_of_log_bloat_on_a_log_whose_derived_events_all_name_no_identity: assert_validate_warns_of_log_bloat(
        |root| seed_derived_events(root, &[None, None]),
        2,
        0,
        "two derived events naming no identity are two derived events of no file identity",
    );
}

/// A log whose perception is recorded as ledger entries alone holds no derived event: the state
/// `rigger reset --derived` leaves a store in, which draws no bloat warning.
#[test]
fn validate_is_silent_on_log_bloat_when_the_log_holds_ledger_entries_and_no_derived_event() {
    let err = validate_stderr_after(|root| {
        common::cli::seed_run_events(root, &[("RunStarted", "{}")]);
        common::cli::with_run_store(root, |store| {
            store
                .append(
                    rigger::conductor::STREAM,
                    ExpectedRevision::Any,
                    &[
                        common::fixtures::generation_ingested("gc", "src/a.rs", "h1", "", false)
                            .event(1),
                    ],
                )
                .unwrap();
        });
    });
    assert_eq!(
        common::cli::bloat_lines(&err),
        [""; 0],
        "a ledger entry is no derived event; stderr:\n{err}"
    );
}

#[test]
fn validate_is_silent_on_log_bloat_when_the_store_is_server_selected() {
    // The bloat advisory is a SQLITE-ONLY mechanic (`bloat_advisory_for`'s own doc: "None on a
    // server-backed project ... a sqlite-only mechanic, exactly like `reset --derived` itself").
    // A server-selected store must never fabricate this warning from a LOCAL events.db that
    // happens to sit beside it - this proves the guard gates on the resolved `StoreSelection`
    // itself, not merely on whether a local file holding derived events happens to exist (the sibling
    // tests above already cover THAT half with no `KURRENTDB_CONN` set at all).
    let dir = temp_rigger_project();
    let root = dir.path();
    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    // Seed the SAME derived events the sqlite-selected warn test above proves draw a warning - so
    // any warning here would be unambiguous evidence the sqlite-only guard leaked.
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
    assert_eq!(
        common::cli::bloat_lines(&err),
        [""; 0],
        "a server-selected store must draw no bloat warning from a local events.db sitting \
         beside it, whatever derived events it holds; stderr:\n{err}"
    );
}

// ---------------------------------------------------------------------------------------
// (c) GRAPH INDEX LAG (spec 92 criterion 1, FRESH ON EVERY INTEGRATION)
// ---------------------------------------------------------------------------------------

/// Record the ledger entry of `file`'s `gc` batch at the generation its CURRENT bytes extract
/// to in the project's own run stream, by a plain append that folds nothing and builds no
/// `graph.db`, so the advisory compares the log's side alone (spec 107). A caller who seeds a
/// file's current content and never edits it afterward records a log that agrees with the tree;
/// editing the file afterward (without re-seeding) is what provokes disagreement. Where no
/// extraction is compiled the entry names a generation nothing extracts to, and the advisory
/// samples nothing.
fn seed_graph_generation(root: &Path, file: &str) {
    #[cfg(feature = "symbols")]
    let generation = common::fixtures::code_generation_now(root, file);
    #[cfg(not(feature = "symbols"))]
    let generation = "unextracted".to_string();
    common::cli::seed_run_events(root, &[("RunStarted", "{}")]);
    common::cli::with_run_store(root, |store| {
        common::fixtures::record_unfolded_entry(store, file, &generation)
    });
    assert!(
        !rigger_file(root, "graph.db").exists(),
        "premise: no graph.db stands, so the log's side alone is compared"
    );
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
    /// The log's latest entry records churn.rs's CURRENT content, and it is never edited
    /// afterward - exactly what an integration that just reindexed it leaves behind.
    validate_is_silent_on_graph_index_lag_when_the_graph_matches_the_tree:
        assert_validate_is_silent_on_graph_index_lag(
            |root| {
                std::fs::write(root.join("churn.rs"), "fn stable() {}\n").unwrap();
                seed_graph_generation(root, "churn.rs");
            },
            "a graph that agrees with the tree must draw no index-lag warning",
        );
    /// No recording of a `gc` identity was ever made (no integration has run yet) - there is
    /// nothing to compare, so this must never manufacture a warning from the mere absence of one.
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
    assert_eq!(
        common::cli::index_lag_lines(stderr),
        vec![common::cli::index_lag_advisory(files).as_str()],
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
fn a_clean_store_with_no_symbols_index_and_no_derived_event_draws_neither_advisory() {
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
    assert_eq!(
        common::cli::bloat_lines(&err),
        [""; 0],
        "a project holding no derived event must draw no bloat warning; stderr:\n{err}"
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

/// Given a project `rigger init` scaffolded that has no event log yet, when the operator runs
/// `rigger validate`, then it exits 0 and creates no event log: the bloat advisory reads a store
/// that exists and never makes one in order to count it.
#[test]
fn validate_creates_no_event_log_in_order_to_count_its_derived_events() {
    let dir = temp_rigger_project();
    let root = dir.path();
    validate_after_init(root, |root| {
        assert!(
            !rigger_file(root, "events.db").exists(),
            "premise: init leaves the project with no event log"
        );
    });
    assert!(
        !rigger_file(root, "events.db").exists(),
        "validate must leave a project that has no event log without one"
    );
}

// ---------------------------------------------------------------------------------------
// (e) THE CLAUDE LOGIN (spec 105, A PERSON SEES THE HOLD)
// ---------------------------------------------------------------------------------------

/// The report the installed `claude auth status --json` printed for an interactive subscription
/// login, cut to the one key the warning's pin reads, so no account identity is committed.
const SUBSCRIPTION_LOGIN_REPORT: &str =
    include_str!("fixtures/claude-auth-status-subscription.json");

/// The line `rigger validate` warns on when the session rides a subscription login.
#[cfg(unix)]
const EXPIRING_LOGIN_WARNING: &str = "warning: claude login: claude.ai is an interactive \
     subscription login, which can expire under a long run; run `claude setup-token` and set the \
     long-lived token it prints as CLAUDE_CODE_OAUTH_TOKEN where rigger runs.";

/// The one line `rigger validate` warns on when it read no login report.
#[cfg(unix)]
const NO_LOGIN_REPORT_WARNING: &str = "warning: claude login: `claude auth status --json` gave \
     no report within 5 s (claude is not on PATH, its status failed or gave no answer by the \
     bound, or its report is not one JSON object naming an authMethod string); a run launches \
     `claude` from PATH, logged in.";

/// `rigger validate` in `root` with a stand-in `claude` first on PATH whose `auth status --json`
/// runs the [`probe_arms`](common::fixtures::probe_arms) body `auth`, or for none with a PATH of
/// only the tools validate runs, which holds no `claude`: the stand-in's directory, holding what it
/// recorded, then validate's stdout, stderr and success, and how long it ran.
#[cfg(unix)]
fn validate_with_claude(
    root: &Path,
    auth: Option<&str>,
) -> (tempfile::TempDir, String, String, bool, std::time::Duration) {
    use common::cli::{ambient_tools_bin, run_rigger_envs};
    use common::fixtures::{arms_claude, probe_arms};

    let fake = tempfile::tempdir().unwrap();
    let path = match auth {
        Some(auth) => {
            arms_claude(fake.path(), &probe_arms(auth, ""));
            format!(
                "{}:{}",
                fake.path().display(),
                std::env::var("PATH").unwrap_or_default()
            )
        }
        None => ambient_tools_bin(fake.path(), &["git"])
            .display()
            .to_string(),
    };
    let started = std::time::Instant::now();
    let (out, err, ok) = run_rigger_envs(root, &["validate"], &[("PATH", &path)]);
    (fake, out, err, ok, started.elapsed())
}

/// Validate's stdout `out` carries exactly the login lines `lines` and its stderr `err` exactly
/// the login warnings `warnings` (`name`).
#[cfg(unix)]
fn assert_reports_the_claude_login(
    name: &str,
    out: &str,
    err: &str,
    lines: Vec<String>,
    warnings: Vec<&str>,
) {
    use common::cli::lines_where;

    assert_eq!(
        lines_where(out, |line| line.starts_with("claude login: ")),
        lines,
        "{name}: stdout:\n{out}"
    );
    assert_eq!(
        lines_where(err, |line| line.starts_with("warning: claude login: ")),
        warnings,
        "{name}: stderr:\n{err}"
    );
}

#[test]
fn the_recorded_subscription_login_keeps_only_the_key_the_pin_reads() {
    let report: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(SUBSCRIPTION_LOGIN_REPORT).unwrap();
    assert_eq!(report.keys().collect::<Vec<_>>(), ["authMethod"]);
}

/// Given a project, when the operator runs `rigger validate` with a stand-in `claude` first on
/// PATH, or none on it, then validate reads `claude auth status --json` once, reports on stdout
/// the auth method it names, warns of a subscription login naming its long-lived token and of no
/// other auth method, one only sharing its prefix or its suffix included, prints one warning in
/// place of the report when none came by its 5 s bound, leaves no process of the stand-in behind
/// and exits 0 on every row.
#[cfg(unix)]
#[test]
fn validate_reports_the_claude_login_and_warns_when_it_expires_or_gives_no_report() {
    use common::fixtures::{
        assert_no_sleeper_left, printing, recorded_argv, sleeping, LOGGED_OUT_STATUS,
    };
    use std::time::Duration;

    // Each window times the whole `rigger validate` process. `quick` stays below the 5 s bound,
    // since it proves no wait; `at_the_bound` leaves the rest of validate room above the bound
    // and stays below the stand-in's 30 s sleep, which it proves validate does not wait out.
    let quick = (Duration::ZERO, Duration::from_secs(3));
    let at_the_bound = (Duration::from_secs(5), Duration::from_secs(20));
    let reported = |method: &str| vec![format!("claude login: {method}")];
    let exits_1 = "    exit 1\n";
    // (name, the stand-in's `auth` arm, none for no `claude` on PATH, the login lines on stdout,
    // the login warnings on stderr, elapsed from and below, the sleeps the stand-in starts)
    let rows = [
        (
            "the recorded subscription login",
            Some(printing(SUBSCRIPTION_LOGIN_REPORT)),
            reported("claude.ai"),
            vec![EXPIRING_LOGIN_WARNING],
            quick,
            0,
        ),
        (
            "a long-lived token",
            Some(printing(r#"{"authMethod": "oauth_token"}"#)),
            reported("oauth_token"),
            vec![],
            quick,
            0,
        ),
        (
            "an API key",
            Some(printing(r#"{"authMethod": "api_key"}"#)),
            reported("api_key"),
            vec![],
            quick,
            0,
        ),
        (
            "an auth method the subscription login's is only the prefix of",
            Some(printing(r#"{"authMethod": "claude.ai-enterprise"}"#)),
            reported("claude.ai-enterprise"),
            vec![],
            quick,
            0,
        ),
        (
            "an auth method the subscription login's is only the suffix of",
            Some(printing(r#"{"authMethod": "enterprise.claude.ai"}"#)),
            reported("enterprise.claude.ai"),
            vec![],
            quick,
            0,
        ),
        (
            "a logged-out session, its status exiting 1",
            Some(printing(LOGGED_OUT_STATUS) + exits_1),
            reported("none"),
            vec![],
            quick,
            0,
        ),
        (
            "a status failing with no report",
            Some(exits_1.to_string()),
            vec![],
            vec![NO_LOGIN_REPORT_WARNING],
            quick,
            0,
        ),
        (
            "a report that is no JSON",
            Some(printing("Logged in as operator@example.com")),
            vec![],
            vec![NO_LOGIN_REPORT_WARNING],
            quick,
            0,
        ),
        (
            "a report naming no authMethod",
            Some(printing(r#"{"loggedIn": true}"#)),
            vec![],
            vec![NO_LOGIN_REPORT_WARNING],
            quick,
            0,
        ),
        (
            "an authMethod that is no string",
            Some(printing(r#"{"authMethod": 7}"#)),
            vec![],
            vec![NO_LOGIN_REPORT_WARNING],
            quick,
            0,
        ),
        (
            "no report by the bound",
            Some(sleeping(30)),
            vec![],
            vec![NO_LOGIN_REPORT_WARNING],
            at_the_bound,
            1,
        ),
        (
            "no claude on PATH",
            None,
            vec![],
            vec![NO_LOGIN_REPORT_WARNING],
            quick,
            0,
        ),
    ];
    let dir = temp_rigger_project();
    let root = dir.path();
    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    for (name, auth, lines, warnings, (from, below), sleepers) in rows {
        let (fake, out, err, ok, elapsed) = validate_with_claude(root, auth.as_deref());
        assert!(
            ok,
            "{name}: the login read never fails validate; stderr:\n{err}"
        );
        assert_reports_the_claude_login(name, &out, &err, lines, warnings);
        assert!(
            from <= elapsed && elapsed < below,
            "{name}: validate took {elapsed:?}, outside {from:?}..{below:?}"
        );
        if auth.is_some() {
            assert_eq!(
                recorded_argv(&fake.path().join("auth-argv")),
                ["auth", "status", "--json"],
                "{name}: the read's argv"
            );
        }
        assert_no_sleeper_left(fake.path(), sleepers, name);
    }
}

/// Given a project whose run store cannot be selected (a directory stands where
/// `.rigger/store.conn` goes), when the operator runs `rigger validate` with a subscription login,
/// a long-lived token or no `claude` on PATH, then validate reports the login as it does on a sound
/// project and still fails on the store, the store error the last line it prints: the login read
/// changes no exit status, a failing one included.
#[cfg(unix)]
#[test]
fn validate_still_fails_on_a_store_it_cannot_select_whatever_the_claude_login() {
    use common::cli::make_store_conn_unreadable;
    use common::fixtures::printing;

    let reported = |method: &str| vec![format!("claude login: {method}")];
    // (name, the stand-in's `auth` arm, none for no `claude` on PATH, the login lines on stdout,
    // the login warnings on stderr): one row for each way the login report can end
    let rows = [
        (
            "the recorded subscription login",
            Some(printing(SUBSCRIPTION_LOGIN_REPORT)),
            reported("claude.ai"),
            vec![EXPIRING_LOGIN_WARNING],
        ),
        (
            "a long-lived token",
            Some(printing(r#"{"authMethod": "oauth_token"}"#)),
            reported("oauth_token"),
            vec![],
        ),
        (
            "no claude on PATH",
            None,
            vec![],
            vec![NO_LOGIN_REPORT_WARNING],
        ),
    ];
    let dir = temp_rigger_project();
    let root = dir.path();
    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    make_store_conn_unreadable(root);
    let unreadable = std::fs::read_to_string(rigger_file(root, "store.conn")).unwrap_err();
    let store_error = format!("rigger: read store connection file: {unreadable}");
    for (name, auth, lines, warnings) in rows {
        let (_fake, out, err, ok, _elapsed) = validate_with_claude(root, auth.as_deref());
        assert!(
            !ok,
            "{name}: validate fails on a store it cannot select, whatever the login; \
             stdout:\n{out}\nstderr:\n{err}"
        );
        assert_eq!(
            err.lines().last(),
            Some(store_error.as_str()),
            "{name}: the store error ends what validate prints; stderr:\n{err}"
        );
        assert_reports_the_claude_login(name, &out, &err, lines, warnings);
    }
}
