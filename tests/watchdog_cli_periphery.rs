//! CLI periphery for spec 69, criterion 2 - `rigger watch`, the driver-independent watchdog
//! (`cmd_watch`, `watch_poll`, `parse_watch_args` in `src/main.rs`; the pure core they compose,
//! `src/watch.rs`, is a separate library module with its own owner below).
//!
//! WHY THIS FILE, DISTINCT FROM THE IMPLEMENTER'S OWN `mod tests`. `cmd_watch`, `watch_poll`,
//! and `parse_watch_args` are PRIVATE, non-`pub` free functions inside the `rigger` BINARY crate
//! (`src/main.rs`), not the `rigger` LIBRARY crate - so an integration-test binary under `tests/`
//! cannot call any of them directly (mirrors `tests/validate_behind_the_tree_periphery.rs`'s
//! identical situation for `cmd_validate`'s advisory helpers). The implementer's own colocated
//! tests already prove `watch_poll`'s composition and `parse_watch_args`'s parsing exhaustively -
//! but every one of those calls the function DIRECTLY, in the SAME compilation unit, against an
//! INJECTED `StoreLocation` it builds by hand. None of them ever calls `cmd_watch` itself (grep
//! confirms exactly one call site in the whole crate: the `"watch" => cmd_watch(...)` dispatch
//! arm) - so nothing proves: real `argv` dispatch through `main()`'s match and its `Result` ->
//! exit-code wiring; `require_store_dir()`'s real cwd walk (a store an operator actually has,
//! not a location a test built by hand); or `cmd_watch`'s OWN control flow - the `--once` branch
//! that must actually terminate the process, and the streaming branch's loop/`sleep`/re-poll
//! against a store that keeps changing while the process runs, which no synchronous single-call
//! unit test can exercise by construction. That is the API-and-integration edge this file owns:
//! the compiled `rigger watch` binary, driven for real, over real time, against a real store.
//!
//! NOT OWNED here: `watch::detect`/`Signal`/`Anomaly`/`WatchInputs`/`Dedup`/the threshold
//! constants (`src/watch.rs`) - the pure domain core, exhaustively covered by the implementer's
//! own inside-out tests there (every signal, every boundary - e.g. the exact 30-minute
//! dead-driver bound, the reject-recurrence cause-reset rule - already pinned, including a
//! completed mutation-testing pass per the run's own decision log). Those items are `pub` only
//! so `main.rs` (a different crate root) can reach them; their real external boundary is
//! reachability from the compiled CLI, which the tests below prove by driving `cmd_watch` end to
//! end rather than re-asserting the same pure-function outputs a second time.

mod common;

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Event, EventStore, ExpectedRevision};

/// A throwaway project: its own git repo (so `project_identity()` resolves deterministically),
/// with NO `.rigger` dir yet - mirrors `tests/cli.rs`'s `temp_project`.
fn temp_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create temp project");
    let _ = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status();
    dir
}

/// Seed an initialized, EMPTY `.rigger/events.db` under `root` - stands in for the store a prior
/// `rigger run`/`step` would have created, so `require_store_dir`'s walk finds a real store
/// instead of refusing to fabricate one. Mirrors `tests/cli.rs`'s `seed_store`.
fn seed_store(root: &Path) {
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(&rigger).unwrap();
    std::fs::File::create(rigger.join("events.db")).unwrap();
}

/// The project identity the binary resolves for `root` - mirrors `tests/cli.rs`'s
/// `run_stream_identity`, which mirrors `StoreLocation::identity`'s own precedence: the tracked
/// `.rigger/project.id` at the git top-level when present, else the git top-level basename, else
/// `root`'s own basename.
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

/// Append `events` directly to `root`'s namespaced run stream, standing in for the conductor
/// minting them - mirrors `tests/cli.rs`'s `seed_run_events`. Requires `seed_store(root)` (or an
/// equivalent prior append) to have run first.
fn seed_run_events(root: &Path, events: &[(&str, &str)]) {
    let db = root.join(".rigger").join("events.db");
    let backend = Store::open(db.to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    for &(ty, body) in events {
        store
            .append(
                rigger::conductor::STREAM,
                ExpectedRevision::Any,
                &[Event::new(ty, body.as_bytes().to_vec())],
            )
            .unwrap();
    }
}

/// Run `rigger <args...>` in `cwd` and return (stdout, stderr, success) - mirrors
/// `tests/cli.rs`'s `run_rigger`.
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

// --- `rigger watch --once`: the composition root, driven through the real binary ---

/// The headline boundary proof: a project seeded (via a real store, not an injected
/// `StoreLocation`) with an escalated unit and a stalled frontier, watched through the REAL
/// compiled `rigger watch --once` - proving `main()`'s dispatch, `require_store_dir()`'s cwd
/// walk, and `watch_poll`'s I/O all compose to print exactly what an operator would see,
/// naming signal, subject, and response for each anomaly, in Design order, and exiting 0.
#[test]
fn watch_once_reports_anomalies_through_the_real_compiled_binary_naming_signal_subject_and_response(
) {
    let proj = temp_project();
    let root = proj.path();
    seed_store(root);
    seed_run_events(
        root,
        &[
            ("UnitStarted", r#"{"id":"u-esc"}"#),
            ("UnitEscalated", r#"{"id":"u-esc"}"#),
            (
                "SpawnResult",
                r#"{"id":"u-stall/implementer#0","output":"a"}"#,
            ),
            (
                "SpawnResult",
                r#"{"id":"u-stall/implementer#0","output":"b"}"#,
            ),
            (
                "SpawnResult",
                r#"{"id":"u-stall/implementer#0","output":"c"}"#,
            ),
        ],
    );

    let (out, err, ok) = run_rigger(root, &["watch", "--once"]);
    assert!(
        ok,
        "rigger watch --once must exit 0 on a healthy store: {err}"
    );
    let lines: Vec<&str> = out.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(
        lines.len(),
        2,
        "expected one line per anomaly, in Design order (escalated before frontier-stall): {out}"
    );
    assert!(
        lines[0].contains("escalated blockers")
            && lines[0].contains("u-esc")
            && lines[0].contains("rigger-handle-an-escalation"),
        "line 1 must name signal, subject, and response: {}",
        lines[0]
    );
    assert!(
        lines[1].contains("frontier progress")
            && lines[1].contains("u-stall/implementer#0")
            && lines[1].contains("stop the driver and diagnose"),
        "line 2 must name signal, subject, and response: {}",
        lines[1]
    );
}

/// The clean-store counterpart: an initialized but otherwise empty store (the shape right
/// after `rigger run` first creates `.rigger/events.db`, before any unit has moved) reports NO
/// anomalies and exits 0 through the real binary - proving the no-anomaly path is not merely a
/// property of the pure `detect()` fold but of the whole composed command.
#[test]
fn watch_once_on_a_freshly_initialized_store_reports_nothing_and_exits_cleanly() {
    let proj = temp_project();
    let root = proj.path();
    seed_store(root);

    let (out, err, ok) = run_rigger(root, &["watch", "--once"]);
    assert!(ok, "watch --once must exit 0 on a clean store: {err}");
    assert!(
        out.trim().is_empty(),
        "a clean store must print nothing: {out:?}"
    );
}

/// Nanosecond wall-clock `recorded_at`/`valid_from`, matching exactly what a real
/// [`rigger::eventstore::sqlite::Store::append`] stamps - unlike `tests/cli.rs`'s own
/// `seed_order_signature` (which stamps `0`, harmless for `rigger validate`'s report), a
/// STALE `recorded_at` here would spuriously also satisfy `watch_poll`'s dead-driver "store
/// quiet an hour" clause, contaminating the store-integrity assertion below with a SECOND,
/// unrelated anomaly line.
fn now_nanos() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as i64
}

/// Seed `<root>/.rigger/events.db`'s run stream with rows whose position order and revision
/// order DISAGREE (spec 71's corruption signature `watch::order_signatures` detects) by
/// inserting directly - bypassing the store's own always-increasing revision assignment, the
/// only way to reach this shape (mirrors `tests/cli.rs`'s own `seed_order_signature`, which
/// proves the SAME shared detector reachable from `rigger validate`'s DIFFERENT composition
/// root). Three rows land in the run stream, in this insertion (position) order: revision 5,
/// then revision 1, then revision 2 - distinct values (satisfying `UNIQUE(stream, revision)`,
/// the actual on-disk shape a write into a compaction-opened revision hole leaves) where
/// positions 2 and 3 both carry a revision at or below the running maximum (5).
fn seed_order_signature(root: &Path) {
    let rigger_dir = root.join(".rigger");
    std::fs::create_dir_all(&rigger_dir).unwrap();
    let db = rigger_dir.join("events.db");
    // Open through the real store first, so the schema is laid down exactly as the binary
    // itself would lay it down (mirrors `seed_run_events`'s own precondition).
    rigger::eventstore::sqlite::Store::open(db.to_str().unwrap()).unwrap();
    let stream = format!(
        "{}{}",
        rigger::eventstore::namespace::Namespaced::prefix_for(&run_stream_identity(root)),
        rigger::conductor::STREAM
    );
    let conn = rusqlite::Connection::open(&db).unwrap();
    let ts = now_nanos();
    for revision in [5i64, 1, 2] {
        conn.execute(
            "INSERT INTO events (stream, type, id, data, meta, valid_from, recorded_at, revision)
             VALUES (?1, 'Seed', ?2, X'7b7d', '{}', ?3, ?3, ?4)",
            rusqlite::params![stream, format!("seed-{revision}"), ts, revision],
        )
        .unwrap();
    }
}

/// Seed an out-of-order TAIL directly on a stream DISTINCT from the run stream
/// (`"other"`, still namespaced to this project), rather than the run stream
/// [`seed_order_signature`] itself uses - a store-wide corruption shape `watch_poll`'s
/// `full_events` read picks up (it reads every stream under this project's namespace, spec
/// 71's own scope: "a disordered stream is a store-wide fault, not a per-run one") without
/// touching `run_events` (scoped to `conductor::STREAM` = `"run"` only). That separation is
/// what lets this seed compose cleanly, in the SAME store, alongside [`seed_run_events`]'s
/// legitimate run-scoped anomalies for the criterion's own combined scenario below - putting
/// the tail on `"run"` too would work for `order_signatures` itself, but every revision 1..N
/// there is already claimed by a real appended event, leaving no unused, still-disordering
/// value the `UNIQUE(stream, revision)` constraint would accept. Mirrors
/// [`seed_order_signature`]'s exact technique (three distinct revisions landing out of
/// position order: 5, then 1, then 2 - two rows disagree with the running max of 5),
/// parameterized onto a stream the four run-scoped signals never read.
fn seed_out_of_order_tail(root: &Path, stream_suffix: &str) {
    let db = root.join(".rigger").join("events.db");
    // The schema is already laid down by the `seed_run_events`/`seed_store` call this
    // combined scenario always makes first; opening again here is a no-op, kept for the same
    // self-contained-precondition reason `seed_order_signature` opens it.
    Store::open(db.to_str().unwrap()).unwrap();
    let stream = format!(
        "{}{stream_suffix}",
        Namespaced::prefix_for(&run_stream_identity(root))
    );
    let conn = rusqlite::Connection::open(&db).unwrap();
    let ts = now_nanos();
    for revision in [5i64, 1, 2] {
        conn.execute(
            "INSERT INTO events (stream, type, id, data, meta, valid_from, recorded_at, revision)
             VALUES (?1, 'Seed', ?2, X'7b7d', '{}', ?3, ?3, ?4)",
            rusqlite::params![stream, format!("tail-{revision}"), ts, revision],
        )
        .unwrap();
    }
}

/// The shared consolidation's own periphery proof (spec 69 c2 round 2,
/// `d-u69c2r2-consolidate-order-signatures`): `watch::order_signatures` is now the ONE
/// detector both `rigger validate`
/// (`tests/cli.rs::validate_detects_a_stream_whose_position_order_and_revision_order_disagree`)
/// and this command's own store-integrity signal call - reachable from TWO DIFFERENT
/// composition roots. Proving the algorithm through `rigger validate` says nothing about
/// whether `main.rs::watch_poll`'s OWN wiring (the whole-log `full_events` read, `detect`'s
/// signal-6 fold, `out_of_order_streams`' delegation) still reaches it correctly through THIS
/// command - a regression here (e.g. the consolidation quietly narrowing `watch_poll`'s read
/// to the run-scoped stream instead of the whole log, or `detect` dropping the signal-6 arm)
/// would leave every pure `watch::` unit test green while `rigger watch` itself silently
/// stopped reporting store corruption. Drives the REAL compiled binary against a REAL sqlite
/// store carrying a genuine out-of-order revision (not an injected `WatchInputs`), pinned to
/// the exact reported values like the validate counterpart, not a loose digit match.
#[test]
fn watch_once_reports_a_store_integrity_anomaly_through_the_real_compiled_binary() {
    let proj = temp_project();
    let root = proj.path();
    seed_order_signature(root);

    let (out, err, ok) = run_rigger(root, &["watch", "--once"]);
    assert!(
        ok,
        "watch --once must exit 0 even on a store-integrity anomaly (report-only, like every \
         other signal): {err}"
    );
    let lines: Vec<&str> = out.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(
        lines.len(),
        1,
        "a store with exactly one disordered stream must report exactly one anomaly, no \
         spurious extras from an unrealistic recorded_at: {out}"
    );
    assert!(
        lines[0].contains("store integrity")
            && lines[0].contains("run")
            && lines[0].contains("2 row(s) where position order and revision order disagree")
            && lines[0].contains("docs/architecture.md, section 5.1.3"),
        "the store-integrity line must name signal, subject, exact row count, and the repair \
         doc together: {}",
        lines[0]
    );
}

/// `require_store_dir()`'s real "no store anywhere" refusal, reached only through `cmd_watch`'s
/// own composition (the implementer's tests always hand `watch_poll` a `StoreLocation` they
/// built by hand, so this refusal path is never exercised there). A project with no `.rigger` at
/// all must exit non-zero and NAME the reason on stderr, never panic or silently fabricate a
/// store (spec 05's own store-opening discipline, which every courier - and now `watch` - must
/// honor identically).
#[test]
fn watch_refuses_a_project_with_no_rigger_store_at_all_through_the_real_binary() {
    let proj = temp_project();
    let root = proj.path();
    // Deliberately no `seed_store`: no `.rigger` dir exists anywhere under `root`.

    let (out, err, ok) = run_rigger(root, &["watch", "--once"]);
    assert!(
        !ok,
        "watch must refuse a project with no rigger store, not silently succeed: stdout={out:?}"
    );
    assert!(
        err.contains("no rigger store found"),
        "the refusal must name the reason: {err}"
    );
}

/// `parse_watch_args`'s error path, surfaced through `main()`'s real `Result` -> exit-code wire
/// (`Err(e) => { eprintln!("rigger: {{e}}"); 1 }`) rather than asserted as a bare `Result::Err` in
/// isolation - proving an operator who fat-fingers a flag gets a clean non-zero exit and a
/// message naming the bad argument, never a panic.
#[test]
fn watch_rejects_an_unknown_flag_through_the_real_binary_with_a_nonzero_exit() {
    let proj = temp_project();
    let root = proj.path();
    seed_store(root);

    let (out, err, ok) = run_rigger(root, &["watch", "--bogus"]);
    assert!(
        !ok,
        "an unknown watch flag must exit non-zero: stdout={out}"
    );
    assert!(
        err.contains("watch: unknown argument"),
        "stderr must name the bad flag: {err}"
    );
}

// --- `rigger watch` (streaming, no `--once`): the loop, live, over real time ---

/// The one thing no synchronous, single-call unit test can prove: that STREAMING mode (no
/// `--once`) is not one-shot - it stays alive, re-polls a store that keeps changing while the
/// process runs, prints a NEWLY appearing anomaly on a later poll, and (in-process `Dedup`,
/// exercised across REAL loop iterations, not one hand-fed `Vec` at a time) suppresses that same
/// anomaly again once it has already been printed once. `cmd_watch`'s loop, its `--interval`
/// wiring, and `watch::Dedup`'s composition into it are called from exactly one production site
/// and zero prior tests; this drives all three together, live.
#[test]
fn watch_without_once_streams_and_re_polls_a_live_mutating_store_until_killed() {
    let proj = temp_project();
    let root = proj.path();
    seed_store(root);

    let mut child = common::rigger_courier()
        .args(["watch", "--interval", "1"])
        .current_dir(root)
        .env("RIGGER_NO_DASH", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn `rigger watch`");
    let stdout = child.stdout.take().expect("watch stdout is piped");

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });

    // Phase 1: the store is clean. The first poll (immediate, before any sleep) must print
    // nothing, and the process must still be running afterward - streaming mode does not exit
    // just because one poll found nothing to report.
    assert!(
        rx.recv_timeout(Duration::from_millis(700)).is_err(),
        "a clean first poll must print nothing"
    );
    assert!(
        child.try_wait().expect("try_wait").is_none(),
        "streaming `rigger watch` must not exit on its own after a clean poll"
    );

    // Phase 2: mutate the store WHILE the child is alive - the shape no injected, one-shot
    // `WatchInputs` can stand in for.
    seed_run_events(
        root,
        &[
            ("UnitStarted", r#"{"id":"u-live"}"#),
            ("UnitEscalated", r#"{"id":"u-live"}"#),
        ],
    );

    // Phase 3: within a few 1s poll cycles, the new anomaly must appear on stdout.
    let line = rx
        .recv_timeout(Duration::from_secs(8))
        .expect("streaming watch never re-polled the live store and printed the new anomaly");
    assert!(
        line.contains("escalated blockers")
            && line.contains("u-live")
            && line.contains("rigger-handle-an-escalation"),
        "the re-polled line must name signal, subject, and response: {line}"
    );

    // Phase 4: the SAME anomaly, still present at the same magnitude on the next poll(s), must
    // be suppressed - `Dedup` composed into the real loop, not asserted as a bare struct.
    assert!(
        rx.recv_timeout(Duration::from_millis(1500)).is_err(),
        "a persisting anomaly at the same magnitude must be deduped on the next real poll"
    );

    let _ = child.kill();
    let _ = child.wait();
}

/// The fault-tolerance boundary streaming mode exists for (spec 69 Design: "it must work with
/// the driver dead" - the watchdog reads only store, process table, and status, NEVER the
/// driver, exactly the process that may be dead). A transient store-read failure - a torn or
/// corrupted read racing a concurrent writer - must be reported and RETRIED on the next poll,
/// never treated as fatal: a watchdog armed unattended under a background monitor that dies on
/// the very first store hiccup recreates, one level up, the exact "a monitor that quietly
/// stops monitoring" failure this whole command exists to catch. No synchronous single-call
/// unit test can prove this (it needs a real process surviving a real fault over real time),
/// mirroring `watch_without_once_streams_and_re_polls_a_live_mutating_store_until_killed`'s own
/// live-process shape.
#[test]
fn watch_streaming_survives_a_transient_store_read_failure_and_recovers() {
    let proj = temp_project();
    let root = proj.path();
    seed_store(root);
    let db_path = root.join(".rigger").join("events.db");
    let good_bytes = std::fs::read(&db_path).expect("read the seeded store");

    let mut child = common::rigger_courier()
        .args(["watch", "--interval", "1"])
        .current_dir(root)
        .env("RIGGER_NO_DASH", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn `rigger watch`");
    let stdout = child.stdout.take().expect("watch stdout is piped");
    let stderr = child.stderr.take().expect("watch stderr is piped");

    let (out_tx, out_rx) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            if out_tx.send(line).is_err() {
                break;
            }
        }
    });
    let (err_tx, err_rx) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            if err_tx.send(line).is_err() {
                break;
            }
        }
    });

    // Phase 1: a clean first poll prints nothing and the process stays alive.
    assert!(
        out_rx.recv_timeout(Duration::from_millis(700)).is_err(),
        "a clean first poll must print nothing"
    );
    assert!(child.try_wait().expect("try_wait").is_none());

    // Phase 2: corrupt the store mid-stream (the exact fault a torn or racing write can leave
    // behind) - a store-read failure squarely inside `watch_poll`'s own fallible operations.
    std::fs::write(&db_path, b"not a database, deliberately corrupted mid-poll")
        .expect("corrupt the store");

    // Phase 3: the process must survive the failing poll(s), reporting the failure on stderr
    // rather than dying silently or propagating it out of the process.
    let err_line = err_rx.recv_timeout(Duration::from_secs(5)).expect(
        "a transient store-read failure must be reported on stderr, not silently swallowed",
    );
    assert!(
        err_line.contains("watch") && err_line.contains("poll"),
        "stderr must name the poll failure: {err_line}"
    );
    assert!(
        child.try_wait().expect("try_wait").is_none(),
        "a transient store-read failure must not kill the streaming watchdog"
    );

    // Phase 4: repair the store - the watchdog must resume reporting on its own, on a LATER
    // poll, never requiring a restart.
    std::fs::write(&db_path, &good_bytes).expect("restore the store");
    seed_run_events(
        root,
        &[
            ("UnitStarted", r#"{"id":"u-recovered"}"#),
            ("UnitEscalated", r#"{"id":"u-recovered"}"#),
        ],
    );
    let line = out_rx
        .recv_timeout(Duration::from_secs(8))
        .expect("streaming watch never recovered and resumed reporting after the store healed");
    assert!(
        line.contains("escalated blockers") && line.contains("u-recovered"),
        "the recovered poll must report the new anomaly: {line}"
    );

    let _ = child.kill();
    let _ = child.wait();
}

// --- The criterion's own literal combination, and streaming's other dedup half ---
//
// The two tests below close the only sub-clauses of spec 69's own Done-when text for THE
// WATCHDOG left proven exclusively inside-out (`src/watch.rs::a_store_seeded_with_a_multi_
// result_spawn_an_escalated_unit_reject_recurrence_three_and_an_out_of_order_tail_prints_
// one_line_each` calls `detect()` directly with a hand-built `WatchInputs`; `src/watch.rs::
// dedup_re_alerts_when_the_magnitude_increments` calls `Dedup::step` directly with bare
// `Anomaly` values) - neither has ever been driven through the compiled `rigger watch`
// binary, the composition this file exists to own (module doc above). The pure core is
// already correct and mutation-tested (peer decision `d-u69c3-mutation-accounting`:
// `watch.rs:296 reject_recurrence_streak` fully caught); what was missing is proof that
// `main()`'s dispatch, `watch_poll`'s real store read, and the streaming loop's real
// `Dedup` actually WIRE that correct core to an operator's terminal for these two shapes.

/// The criterion's own combined scenario, verbatim (spec 69 Done-when): "a store seeded with
/// a multi-result spawn, an escalated unit, a unit at reject-recurrence three, and an
/// out-of-order tail prints one line per anomaly naming signal, subject, and response skill",
/// all FOUR anomalies in ONE store, through the real compiled `rigger watch --once`, in
/// Design order. Every other test in this file drives at most two signals in one store (the
/// headline test above: escalated + frontier-stall; the store-integrity test: that signal
/// alone); this is the one place the whole combination is proven end to end rather than
/// piecewise, closing the gap the module doc's own reasoning implies - a regression that
/// narrowed `watch_poll`'s real read (e.g. dropping a signal arm, or scoping `full_events`
/// down to the run stream) could leave every piecewise CLI test above green while the
/// combined shape a real operator's store actually presents silently lost a line.
#[test]
fn watch_once_reports_the_criterions_own_multi_anomaly_scenario_through_the_real_compiled_binary() {
    let proj = temp_project();
    let root = proj.path();
    seed_store(root);
    seed_run_events(
        root,
        &[
            ("UnitStarted", r#"{"id":"u-esc"}"#),
            ("UnitEscalated", r#"{"id":"u-esc"}"#),
            ("UnitStarted", r#"{"id":"u-fail"}"#),
            (
                "UnitFailed",
                r#"{"id":"u-fail","attempts":1,"cause":"gate:fmt"}"#,
            ),
            ("UnitStarted", r#"{"id":"u-fail"}"#),
            (
                "UnitFailed",
                r#"{"id":"u-fail","attempts":2,"cause":"gate:fmt"}"#,
            ),
            ("UnitStarted", r#"{"id":"u-fail"}"#),
            (
                "UnitFailed",
                r#"{"id":"u-fail","attempts":3,"cause":"gate:fmt"}"#,
            ),
            (
                "SpawnResult",
                r#"{"id":"u-stall/implementer#0","output":"a"}"#,
            ),
            (
                "SpawnResult",
                r#"{"id":"u-stall/implementer#0","output":"b"}"#,
            ),
            (
                "SpawnResult",
                r#"{"id":"u-stall/implementer#0","output":"c"}"#,
            ),
        ],
    );
    seed_out_of_order_tail(root, "other");

    let (out, err, ok) = run_rigger(root, &["watch", "--once"]);
    assert!(
        ok,
        "rigger watch --once must exit 0 even with every anomaly firing at once: {err}"
    );
    let lines: Vec<&str> = out.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(
        lines.len(),
        4,
        "one line per anomaly, four anomalies seeded together, in Design order: {out}"
    );
    assert!(
        lines[0].contains("escalated blockers")
            && lines[0].contains("u-esc")
            && lines[0].contains("rigger-handle-an-escalation"),
        "line 1 (Design order: escalated blockers first): {}",
        lines[0]
    );
    assert!(
        lines[1].contains("reject-recurrence trend")
            && lines[1].contains("u-fail")
            && lines[1].contains("reject-recurrence #3")
            && lines[1].contains("gate:fmt")
            && lines[1].contains("rigger-diagnose-churn"),
        "line 2 (the unit at reject-recurrence three, its cause named): {}",
        lines[1]
    );
    assert!(
        lines[2].contains("frontier progress")
            && lines[2].contains("u-stall/implementer#0")
            && lines[2].contains("stop the driver and diagnose"),
        "line 3 (the multi-result spawn): {}",
        lines[2]
    );
    assert!(
        lines[3].contains("store integrity")
            && lines[3].contains("other")
            && lines[3].contains("2 row(s) where position order and revision order disagree"),
        "line 4 (the out-of-order tail, store integrity sorts last): {}",
        lines[3]
    );
}

/// The criterion's other still-inside-out-only half, verbatim (spec 69 Done-when):
/// "streaming mode dedupes a persisting anomaly until it clears and re-alerts a churn count
/// on each increment". `watch_without_once_streams_and_re_polls_a_live_mutating_store_
/// until_killed` above already proves live dedup SUPPRESSION through the real binary, but
/// only for `Escalated`, a magnitude-0 signal that can never climb - it cannot exercise
/// re-alert-on-increment by construction. This test drives the one signal that DOES carry a
/// climbing magnitude (`RejectRecurrence`, spec 69 Design: "counted and re-alerted PER
/// FAILURE CAUSE") through a real streaming process, live: below threshold (silent) -> the
/// streak crosses 3 (first alert) -> holds at 3 (deduped) -> climbs to 4 (RE-alerts under the
/// same signal+subject key, not suppressed as a repeat of the magnitude-3 line already
/// printed) - the one behavior no synchronous single-call unit test can exercise, since it
/// needs a real process re-polling a store that keeps changing while it runs.
#[test]
fn watch_streaming_re_alerts_a_reject_recurrence_churn_count_on_each_increment() {
    let proj = temp_project();
    let root = proj.path();
    seed_store(root);
    // Two same-cause failures: below the diagnose threshold of three, must stay silent.
    seed_run_events(
        root,
        &[
            ("UnitStarted", r#"{"id":"u-churn"}"#),
            (
                "UnitFailed",
                r#"{"id":"u-churn","attempts":1,"cause":"gate:fmt"}"#,
            ),
            ("UnitStarted", r#"{"id":"u-churn"}"#),
            (
                "UnitFailed",
                r#"{"id":"u-churn","attempts":2,"cause":"gate:fmt"}"#,
            ),
        ],
    );

    let mut child = common::rigger_courier()
        .args(["watch", "--interval", "1"])
        .current_dir(root)
        .env("RIGGER_NO_DASH", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn `rigger watch`");
    let stdout = child.stdout.take().expect("watch stdout is piped");

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });

    // Phase 1: two same-cause failures is below threshold - the first (immediate) poll must
    // print nothing, and the process must still be running.
    assert!(
        rx.recv_timeout(Duration::from_millis(700)).is_err(),
        "two same-cause failures is below the reject-recurrence threshold: must be silent"
    );
    assert!(child.try_wait().expect("try_wait").is_none());

    // Phase 2: a third same-cause failure crosses the threshold, live - the churn count's
    // FIRST alert, magnitude 3.
    seed_run_events(
        root,
        &[
            ("UnitStarted", r#"{"id":"u-churn"}"#),
            (
                "UnitFailed",
                r#"{"id":"u-churn","attempts":3,"cause":"gate:fmt"}"#,
            ),
        ],
    );
    let first = rx
        .recv_timeout(Duration::from_secs(8))
        .expect("crossing the reject-recurrence threshold live must alert");
    assert!(
        first.contains("reject-recurrence trend")
            && first.contains("u-churn")
            && first.contains("reject-recurrence #3"),
        "the first churn alert must name the streak at 3: {first}"
    );

    // Phase 3: the SAME streak, unchanged, on the next real poll(s): deduped.
    assert!(
        rx.recv_timeout(Duration::from_millis(1500)).is_err(),
        "a persisting churn count at the SAME magnitude must stay deduped"
    );

    // Phase 4: the churn count CLIMBS to 4, live - re-alerts under the same signal+subject
    // key (Done-when: "re-alerts a churn count on each increment"), never suppressed as a
    // repeat of the magnitude-3 alert already printed.
    seed_run_events(
        root,
        &[
            ("UnitStarted", r#"{"id":"u-churn"}"#),
            (
                "UnitFailed",
                r#"{"id":"u-churn","attempts":4,"cause":"gate:fmt"}"#,
            ),
        ],
    );
    let second = rx
        .recv_timeout(Duration::from_secs(8))
        .expect("the churn count climbing from 3 to 4 must re-alert live, not stay deduped");
    assert!(
        second.contains("reject-recurrence trend")
            && second.contains("u-churn")
            && second.contains("reject-recurrence #4"),
        "the second churn alert must name the climbed streak at 4: {second}"
    );

    let _ = child.kill();
    let _ = child.wait();
}
