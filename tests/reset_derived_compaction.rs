//! Periphery (CLI / store / fold) tests for spec 60, criterion 5 - SUPPORTED COMPACTION.
//!
//! `rigger reset --derived` prunes the duplication an already-bloated log accumulated before the
//! project-scoped ingest dedup existed. For each of the four derived index types it keeps the
//! LATEST event per distinct replay key, deletes the rest, and vacuums, so the file shrinks on
//! disk. Every non-derived event survives byte-for-byte, the graph projection stays consistent,
//! and the compacted store is still a working store.
//!
//! These tests drive the COMPILED binary against a real `.rigger/events.db`, because the criterion
//! is an operator-facing command whose observable effects are on-disk: which rows survive, what the
//! file weighs, what the command printed, and whether the store still reads and appends afterwards.
//!
//! What this file OWNS (criterion 5) and what it deliberately does not:
//!
//!   - OWNS: the `--derived` prune's selection rule (latest per distinct replay key, per derived
//!     type), the non-derived preservation, the on-disk shrink, the printed report, the loud
//!     refusal on a non-embedded backend, the composition with `--runs`, and the usability of the
//!     compacted store (`rigger validate` reads it, and it still accepts appends).
//!   - NOT OWNED: the run-path ingest dedup (criterion 1), the run-scoping boundary (criterion 2),
//!     the revert proof (criterion 3), and the storage guard (criterion 4). Those are pinned by
//!     their own units' tests and are not re-litigated here.

use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, Event, EventStore, ExpectedRevision};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, UNIX_EPOCH};

// ---------------------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------------------

fn rigger_bin() -> &'static str {
    env!("CARGO_BIN_EXE_rigger")
}

/// A throwaway project: its own git repo, so `project_identity()` resolves to the directory's
/// basename exactly as it does for a real project, and a seed appended under that identity lands
/// in the same `proj-<id>-run` stream the compiled binary reads back.
fn temp_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create temp project");
    let _ = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status();
    std::fs::create_dir_all(dir.path().join(".rigger")).expect("create .rigger");
    dir
}

/// The project identity the binary resolves for `root`, mirrored for seeding: the tracked
/// `.rigger/project.id` at the git top-level when present, else that top-level's basename.
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
    run_rigger_envs(cwd, args, &[])
}

fn run_rigger_envs(cwd: &Path, args: &[&str], envs: &[(&str, &str)]) -> (String, String, bool) {
    let mut cmd = Command::new(rigger_bin());
    cmd.args(args).current_dir(cwd);
    cmd.env("RIGGER_NO_DASH", "1");
    let state = tempfile::tempdir().expect("create a temp XDG_STATE_HOME");
    cmd.env("XDG_STATE_HOME", state.path());
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("failed to spawn the rigger binary");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

// ---------------------------------------------------------------------------------------
// The seeded log: an ALREADY-BLOATED store, exactly the shape this command exists to prune
// ---------------------------------------------------------------------------------------

/// How many times each derived batch was re-recorded before the project-scoped dedup existed.
/// Large enough that the pruned rows span many database pages, so the VACUUM's shrink is a real
/// on-disk effect and not a rounding artifact.
const ROUNDS: usize = 500;

/// The replay keys the seed records, in the `<prefix>/<file>@<hash>#<i>` form `ingest::key_batch`
/// builds. `src/b.rs` is recorded at TWO content generations, so the prune is proven to keep the
/// latest recording of EVERY key rather than the latest key per file.
const KEY_A_DEF: &str = "gc/src/a.rs@h1#0";
const KEY_A_REF: &str = "gc/src/a.rs@h1#1";
const KEY_B_GEN1: &str = "gc/src/b.rs@h1#0";
const KEY_B_GEN2: &str = "gc/src/b.rs@h2#0";

/// One row of the event log, as the table holds it. Comparing these tuples across the compaction
/// is what "survives byte-for-byte" MEANS: same global position, stream, type, id, payload bytes,
/// metadata, valid-from, recorded-at, and per-stream revision.
type Row = (i64, String, String, String, Vec<u8>, String, i64, i64, i64);

fn rows(db: &Path) -> Vec<Row> {
    let conn = rusqlite::Connection::open(db).expect("open the event log");
    let mut stmt = conn
        .prepare(
            "SELECT position, stream, type, id, data, meta, valid_from, recorded_at, revision \
             FROM events ORDER BY position",
        )
        .unwrap();
    let out = stmt
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
            ))
        })
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    out
}

/// The replay key a row carries, if any - read out of the row's metadata JSON exactly as the store
/// reads it.
fn replay_key(row: &Row) -> Option<String> {
    let meta: serde_json::Value = serde_json::from_str(&row.5).ok()?;
    meta.get("replay_key")?.as_str().map(str::to_string)
}

fn derived(row: &Row) -> bool {
    rigger::ingest::is_derived_index_type(&row.2)
}

/// A `CodeEntityExtracted` payload in the on-log JSON form, built raw so the test pins the wire
/// contract rather than an in-crate struct it cannot name.
fn code_entity(file: &str, name: &str, line: u32, fresh: bool) -> Vec<u8> {
    let mut v = serde_json::json!({
        "file": file, "name": name, "kind": "function", "line": line, "lang": "rust",
    });
    if fresh {
        v["fresh"] = serde_json::Value::Bool(true);
    }
    serde_json::to_vec(&v).unwrap()
}

/// An `EdgeInferred` payload in the on-log JSON form.
fn edge_inferred(file: &str, name: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({ "file": file, "name": name, "lang": "rust" })).unwrap()
}

fn keyed(type_: &str, data: Vec<u8>, key: &str, secs: u64) -> Event {
    Event::new(type_, data)
        .with_meta(rigger::ingest::META_REPLAY_KEY, key)
        .with_valid_from(UNIX_EPOCH + Duration::from_secs(secs))
}

/// Seed `root` with a bloated event log: `ROUNDS` re-recordings of every derived batch (the
/// duplication the pre-dedup ingest accumulated), alongside the events the prune must never touch.
///
/// The events the prune MUST leave alone are seeded deliberately, each one closing a different way
/// the rule could be got wrong:
///
///   - two IDENTICAL `ReviewFinding`s: domain events legitimately repeat, and both must survive.
///   - two `DecisionMade`s carrying a DERIVED-LOOKING replay key: the partition is decided BY TYPE
///     FIRST, so a non-derived event is ineligible however its key is spelled.
///   - two derived events carrying NO replay key: a key is what names a content generation, so an
///     event without one is never provably redundant and appends through (the fail-safe direction).
///   - `src/b.rs` at two content generations: a superseded generation's key is still a DISTINCT
///     key, so its latest recording survives the prune.
fn seed_bloated_log(root: &Path) {
    let backend = Store::open(event_log(root).to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));

    let mut events: Vec<Event> = Vec::with_capacity(ROUNDS * 4 + 8);

    events.push(
        Event::new("RunStarted", br#"{"run":"r1","criteria":["c"]}"#.to_vec())
            .with_valid_from(UNIX_EPOCH + Duration::from_secs(10)),
    );
    for _ in 0..2 {
        events.push(
            Event::new(
                "ReviewFinding",
                br#"{"id":"f1","summary":"raised twice"}"#.to_vec(),
            )
            .with_valid_from(UNIX_EPOCH + Duration::from_secs(11)),
        );
    }
    for _ in 0..2 {
        events.push(
            Event::new(
                "DecisionMade",
                br#"{"id":"d1","summary":"s","governs":["src/a.rs"],"supersedes":""}"#.to_vec(),
            )
            // A NON-derived event whose replay key is spelled exactly like a derived one.
            .with_meta(rigger::ingest::META_REPLAY_KEY, KEY_A_DEF)
            .with_valid_from(UNIX_EPOCH + Duration::from_secs(12)),
        );
    }
    for _ in 0..2 {
        events.push(
            Event::new(
                rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
                code_entity("src/keyless.rs", "unkeyed", 1, false),
            )
            .with_valid_from(UNIX_EPOCH + Duration::from_secs(13)),
        );
    }

    for r in 0..ROUNDS {
        let secs = 1_000 + r as u64;
        events.push(keyed(
            rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
            code_entity("src/a.rs", "alpha", 1, true),
            KEY_A_DEF,
            secs,
        ));
        events.push(keyed(
            rigger::contextgraph::TYPE_EDGE_INFERRED,
            edge_inferred("src/a.rs", "beta"),
            KEY_A_REF,
            secs,
        ));
    }
    for r in 0..ROUNDS {
        events.push(keyed(
            rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
            code_entity("src/b.rs", "gen_one", 1, true),
            KEY_B_GEN1,
            2_000 + r as u64,
        ));
    }
    for r in 0..ROUNDS {
        events.push(keyed(
            rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
            code_entity("src/b.rs", "gen_two", 2, true),
            KEY_B_GEN2,
            3_000 + r as u64,
        ));
    }

    for chunk in events.chunks(500) {
        store
            .append(rigger::conductor::STREAM, ExpectedRevision::Any, chunk)
            .unwrap();
    }

    // Fold the WAL into the main file so a size measured right after seeding is the real one.
    let conn = rusqlite::Connection::open(event_log(root)).unwrap();
    conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
        .unwrap();
}

/// How many rows the prune must remove per derived type, given the seed above: every recording of
/// a key except its latest.
const REMOVED_CODE_ENTITIES: usize = 3 * (ROUNDS - 1);
const REMOVED_EDGES: usize = ROUNDS - 1;

// ---------------------------------------------------------------------------------------
// The selection rule
// ---------------------------------------------------------------------------------------

#[test]
fn reset_derived_keeps_the_latest_recording_of_every_replay_key_and_prunes_every_earlier_one() {
    let dir = temp_project();
    let root = dir.path();
    seed_bloated_log(root);

    let before = rows(&event_log(root));
    let (out, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert!(ok, "reset --derived must succeed; stderr: {err}\n{out}");
    let after = rows(&event_log(root));

    // For every derived replay key, exactly ONE row survives, and it is the row the log recorded
    // LAST - the file's current recording, never a superseded one.
    for key in [KEY_A_DEF, KEY_A_REF, KEY_B_GEN1, KEY_B_GEN2] {
        let kept: Vec<&Row> = after
            .iter()
            .filter(|r| derived(r) && replay_key(r).as_deref() == Some(key))
            .collect();
        assert_eq!(
            kept.len(),
            1,
            "exactly one derived event must survive for {key}; got {}",
            kept.len()
        );
        let latest = before
            .iter()
            .filter(|r| derived(r) && replay_key(r).as_deref() == Some(key))
            .map(|r| r.0)
            .max()
            .expect("the seed recorded this key");
        assert_eq!(
            kept[0].0, latest,
            "the surviving recording of {key} must be the LATEST one the log held"
        );
    }

    // A superseded content generation is still a DISTINCT key, so `src/b.rs` keeps one recording
    // of EACH generation: the prune is per key, never per file.
    let b_rows: Vec<String> = after
        .iter()
        .filter(|r| derived(r))
        .filter_map(replay_key)
        .filter(|k| k.starts_with("gc/src/b.rs@"))
        .collect();
    assert_eq!(
        b_rows.len(),
        2,
        "both of src/b.rs's recorded generations must keep their latest recording; got {b_rows:?}"
    );
}

// ---------------------------------------------------------------------------------------
// The fail-safe direction: nothing else may be dropped, reordered, or rewritten
// ---------------------------------------------------------------------------------------

#[test]
fn reset_derived_preserves_every_non_derived_event_and_every_keyless_derived_event_byte_for_byte() {
    let dir = temp_project();
    let root = dir.path();
    seed_bloated_log(root);

    let before = rows(&event_log(root));
    let (out, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert!(ok, "reset --derived must succeed; stderr: {err}\n{out}");
    let after = rows(&event_log(root));

    // Byte-for-byte, in order: every column of every non-derived row is unchanged, INCLUDING its
    // global position and its per-stream revision. The compaction moves the store's revision
    // CURSOR, it never rewrites a row.
    let survivors_expected: Vec<&Row> = before.iter().filter(|r| !derived(r)).collect();
    let survivors_actual: Vec<&Row> = after.iter().filter(|r| !derived(r)).collect();
    assert_eq!(
        survivors_actual, survivors_expected,
        "every non-derived event must survive the compaction byte-for-byte and in order"
    );

    // Two IDENTICAL domain events mean the fact was recorded twice: both rows stay.
    assert_eq!(
        after.iter().filter(|r| r.2 == "ReviewFinding").count(),
        2,
        "two identical ReviewFindings must both survive - only the derived index is content-keyed"
    );
    // TYPE FIRST: a non-derived event is ineligible however its replay key is spelled.
    assert_eq!(
        after.iter().filter(|r| r.2 == "DecisionMade").count(),
        2,
        "a non-derived event carrying a derived-LOOKING replay key must never be pruned"
    );
    // A derived event with NO replay key names no content generation, so it is never provably
    // redundant: it appends through, which is the fail-safe direction.
    assert_eq!(
        after
            .iter()
            .filter(|r| derived(r) && replay_key(r).is_none())
            .count(),
        2,
        "a derived event carrying no replay key must survive - it names no content generation"
    );
}

// ---------------------------------------------------------------------------------------
// The observable operator effects: the file shrinks, and the command says what it did
// ---------------------------------------------------------------------------------------

#[test]
fn reset_derived_shrinks_the_log_on_disk_and_reports_the_rows_per_type_and_the_bytes_reclaimed() {
    let dir = temp_project();
    let root = dir.path();
    seed_bloated_log(root);

    let db = event_log(root);
    let before_bytes = std::fs::metadata(&db).unwrap().len();

    let (out, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert!(ok, "reset --derived must succeed; stderr: {err}\n{out}");

    // ROWS PER TYPE. An operator has to be able to read what was removed, per derived type - a
    // prune that reports only a total cannot be checked against what was expected.
    assert!(
        out.contains(&format!("CodeEntityExtracted {REMOVED_CODE_ENTITIES}")),
        "reset --derived must report the CodeEntityExtracted rows it removed; got: {out:?}"
    );
    assert!(
        out.contains(&format!("EdgeInferred {REMOVED_EDGES}")),
        "reset --derived must report the EdgeInferred rows it removed; got: {out:?}"
    );
    assert!(
        out.contains("DocConceptExtracted 0") && out.contains("DocLinkExtracted 0"),
        "reset --derived must report every derived type, including the ones it removed nothing \
         from; got: {out:?}"
    );

    // BYTES RECLAIMED, and the shrink they describe. Without the VACUUM the DELETE only frees
    // pages inside the file and the log stays exactly as large on disk as before.
    let reclaimed = reported_reclaimed(&out).unwrap_or_else(|| {
        panic!("reset --derived must report a reclaimed-byte count; got {out:?}")
    });
    assert!(
        reclaimed > 0,
        "a real prune must report a non-zero reclaimed-byte count; got {reclaimed} from {out:?}"
    );
    let after_bytes = std::fs::metadata(&db).unwrap().len();
    assert!(
        after_bytes < before_bytes,
        "reset --derived must COMPACT the log on disk: {before_bytes} -> {after_bytes} bytes"
    );

    // A SECOND pass finds nothing to prune: the compaction is idempotent, and it says so honestly
    // rather than reporting a prune that did not happen.
    let (again, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert!(ok, "a second reset --derived must succeed; stderr: {err}");
    assert!(
        again.contains("CodeEntityExtracted 0") && again.contains("EdgeInferred 0"),
        "a second pass over an already-compacted log must report removing nothing; got: {again:?}"
    );
}

/// The reclaimed byte count out of the `reset --derived` report line (`reclaimed <N> byte(s)`).
fn reported_reclaimed(out: &str) -> Option<u64> {
    let needle = "reclaimed ";
    let at = out.rfind(needle)? + needle.len();
    let rest = &out[at..];
    let end = rest.find(|c: char| !c.is_ascii_digit())?;
    rest[..end].parse().ok()
}

// ---------------------------------------------------------------------------------------
// The compacted store is still a store: it reads clean, it folds the same graph, it appends
// ---------------------------------------------------------------------------------------

/// Fold `events` into a FRESH projection at `path` and return its live rows: the nodes, and the
/// live edges with the provenance the fold assigned them.
fn fold_snapshot(events: &[Event], project: &str, path: &Path) -> (Vec<String>, Vec<String>) {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::Projection;
    {
        let p = Projector::open(path.to_str().unwrap(), project).unwrap();
        p.apply_batch(events).unwrap();
    }
    let conn = rusqlite::Connection::open(path).unwrap();
    let mut nodes: Vec<String> = conn
        .prepare("SELECT id, kind, COALESCE(attrs,''), project FROM nodes")
        .unwrap()
        .query_map([], |r| {
            Ok(format!(
                "{}|{}|{}|{}",
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?
            ))
        })
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    nodes.sort();
    // `valid_from` is deliberately NOT compared: it records when the surviving assertion was made,
    // and pruning a fact's superseded re-recordings necessarily advances it to the recording that
    // remains. Which facts are LIVE, what they connect, and which recording is their provenance
    // (`source`) are what the projection's contract is about, and those must be identical.
    let mut edges: Vec<String> = conn
        .prepare(
            "SELECT from_id, to_id, rel, source, project, tier FROM edges WHERE valid_to IS NULL",
        )
        .unwrap()
        .query_map([], |r| {
            Ok(format!(
                "{}|{}|{}|{}|{}|{}",
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?
            ))
        })
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    edges.sort();
    (nodes, edges)
}

fn read_log(root: &Path) -> Vec<Event> {
    let backend = Store::open(event_log(root).to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    store
        .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
        .unwrap()
}

#[test]
fn a_compacted_log_folds_to_the_same_live_graph_reads_clean_and_still_accepts_appends() {
    let dir = temp_project();
    let root = dir.path();
    seed_bloated_log(root);
    let id = run_stream_identity(root);

    let scratch = tempfile::tempdir().unwrap();
    let before_graph = fold_snapshot(&read_log(root), &id, &scratch.path().join("before.db"));

    let (out, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert!(ok, "reset --derived must succeed; stderr: {err}\n{out}");

    // THE PROJECTION CONTRACT. The graph is an upsert projection, so every recording of a key
    // folds to the same rows: keeping the latest and dropping the rest leaves a log that folds to
    // the identical live graph.
    let after_graph = fold_snapshot(&read_log(root), &id, &scratch.path().join("after.db"));
    assert_eq!(
        after_graph.0, before_graph.0,
        "the compacted log must fold to the same nodes"
    );
    assert_eq!(
        after_graph.1, before_graph.1,
        "the compacted log must fold to the same live edges, with the same provenance"
    );
    assert!(
        !before_graph.0.is_empty() && !before_graph.1.is_empty(),
        "the seeded log must actually fold to something, or the equality above proves nothing"
    );

    // `rigger validate` reads the compacted store cleanly. Scaffold the workflow + agents first,
    // so the only thing left for validate to judge is the store this test compacted.
    let (_, ierr, iok) = run_rigger(root, &["init"]);
    assert!(iok, "rigger init must scaffold the project; stderr: {ierr}");
    let (_, verr, vok) = run_rigger(root, &["validate"]);
    assert!(
        vok,
        "rigger validate must read the compacted store cleanly; stderr: {verr}"
    );

    // AND IT IS STILL A STORE. The prune leaves gaps in the stream's revisions, so a store that
    // derived its revision cursor from the surviving ROW COUNT would reissue a revision the stream
    // still holds and fail the `UNIQUE(stream, revision)` index on the very next append.
    let backend = Store::open(event_log(root).to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &id);
    store
        .append(
            rigger::conductor::STREAM,
            ExpectedRevision::Any,
            &[Event::new(
                "RunStarted",
                br#"{"run":"r2","criteria":["c"]}"#.to_vec(),
            )],
        )
        .expect("a compacted store must still accept appends");
    let log = store
        .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
        .unwrap();
    assert_eq!(
        log.last().map(|e| e.type_.as_str()),
        Some("RunStarted"),
        "the appended event must be readable back off the compacted store"
    );
}

// ---------------------------------------------------------------------------------------
// The command surface: composition with --runs, and the loud refusal
// ---------------------------------------------------------------------------------------

#[test]
fn reset_composes_derived_with_runs_and_still_refuses_a_bare_or_unknown_reset() {
    let dir = temp_project();
    let root = dir.path();
    seed_bloated_log(root);

    // Each flag prunes its own accumulation, and they compose in one invocation.
    let (out, err, ok) = run_rigger(root, &["reset", "--runs", "--derived"]);
    assert!(
        ok,
        "reset --runs --derived must succeed; stderr: {err}\n{out}"
    );
    assert!(
        out.contains("reset --runs:") && out.contains("reset --derived:"),
        "a composed reset must report BOTH prunes; got: {out:?}"
    );
    assert!(
        out.contains(&format!("CodeEntityExtracted {REMOVED_CODE_ENTITIES}")),
        "the composed --derived prune must do the same work it does alone; got: {out:?}"
    );

    // A bare `rigger reset` still never prunes silently, and an unknown mode is refused rather
    // than ignored.
    let (_, err, ok) = run_rigger(root, &["reset"]);
    assert!(!ok, "a bare `rigger reset` must still refuse");
    assert!(
        err.contains("--derived") && err.contains("--runs"),
        "the refusal must name both modes; got: {err:?}"
    );
    let (_, err, ok) = run_rigger(root, &["reset", "--everything"]);
    assert!(!ok, "an unknown reset mode must be refused");
    assert!(
        err.contains("--everything"),
        "the refusal must name what it did not understand; got: {err:?}"
    );
}

#[test]
fn reset_derived_on_a_backend_that_cannot_compact_fails_loudly_naming_the_backend_it_needs() {
    let dir = temp_project();
    let root = dir.path();
    seed_bloated_log(root);
    let db_before = rows(&event_log(root)).len();

    // Deleting rows and reclaiming the file is a mechanic of the embedded log. Configured for the
    // server-backed store, `--derived` must FAIL - never silently report a prune that did not
    // happen, which is the one outcome an operator cannot detect.
    let (out, err, ok) = run_rigger_envs(
        root,
        &["reset", "--derived"],
        &[("KURRENTDB_CONN", "esdb://127.0.0.1:2113?tls=false")],
    );
    assert!(
        !ok,
        "reset --derived on a server-backed project must fail; stdout: {out}"
    );
    let said = format!("{err}{out}");
    assert!(
        said.contains("--derived") && said.contains("sqlite"),
        "the refusal must name the embedded sqlite backend the compaction needs; got: {said:?}"
    );

    // And it must be a REFUSAL, not a half-done prune: the local log is untouched.
    assert_eq!(
        rows(&event_log(root)).len(),
        db_before,
        "a refused compaction must leave the log exactly as it was"
    );

    // The refusal is decided BEFORE any pruning, so a composed invocation does not half-succeed.
    let (out, _, ok) = run_rigger_envs(
        root,
        &["reset", "--runs", "--derived"],
        &[("KURRENTDB_CONN", "esdb://127.0.0.1:2113?tls=false")],
    );
    assert!(
        !ok,
        "a composed reset naming an unsupported --derived must fail before pruning; stdout: {out}"
    );
    assert!(
        !out.contains("reset --runs:"),
        "the composed reset must refuse BEFORE running the --runs prune; got: {out:?}"
    );
}
