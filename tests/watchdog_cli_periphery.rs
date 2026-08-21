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
use std::time::Duration;

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
