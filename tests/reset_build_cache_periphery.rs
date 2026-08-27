//! Periphery (CLI) tests for spec 77, criterion 5 - the BOUNDED SHARED CACHE:
//! `rigger reset --build-cache` reclaims the shared gate build cache under the scratch root.
//!
//! What this file OWNS (criterion 4) and what it deliberately does not:
//!
//!   - OWNS: `rigger reset --build-cache` deletes a real, populated shared cache and reports
//!     the exact bytes reclaimed; it is idempotent (a second call, or a call against a project
//!     that never built anything, reports zero rather than erroring); it composes with
//!     `--runs`/`--derived` in either order; it REFUSES (non-zero exit, never blocks) while a
//!     rigger-launched shared-cache build holds the guard, and leaves the cache byte-for-byte
//!     untouched when it does; and the flag is registered (parses, rejects a duplicate).
//!   - NOT OWNED: the exclusion PRIMITIVE's own unit-level contract
//!     (`reclaim_shared_build_cache`'s rename/idempotent-zero/busy/prompt-release behavior) -
//!     pinned in-crate beside its definition in `src/main.rs`; the PRODUCER half (a gate build
//!     actually holding the guard SHARED for its cargo invocation) - pinned in `src/gate.rs`
//!     and `src/conductor.rs`; and `rigger validate`'s FOOTPRINT ACCOUNTING category for this
//!     cache - spec 77 criterion 5's own surface.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

fn temp_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create temp project");
    let _ = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status();
    dir
}

fn event_log(root: &Path) -> PathBuf {
    root.join(".rigger").join("events.db")
}

/// Seed an initialized, otherwise-empty `.rigger/events.db`, standing in for the store a prior
/// `rigger run`/`step` would have created (an empty file is a valid empty SQLite database;
/// `Store::open` adds the schema on first open). `reset --build-cache` needs a resolvable store
/// only to anchor the scratch root at the SAME repo root every other scratch-touching command
/// uses - it never reads or writes a single event.
fn seed_store(root: &Path) {
    std::fs::create_dir_all(root.join(".rigger")).unwrap();
    std::fs::File::create(event_log(root)).unwrap();
}

/// The shared gate build cache's resolved path for a `temp_project()` with no `defaults.workdir`
/// override: the repo-default `<repo>/.rigger/tmp/cargo-target`.
fn shared_cache_dir(root: &Path) -> PathBuf {
    root.join(".rigger").join("tmp").join("cargo-target")
}

fn guard_path(root: &Path) -> PathBuf {
    root.join(".rigger").join("tmp").join("cargo-target.lock")
}

fn write_file(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

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

/// Total bytes of every regular file under `path`, recursively (a missing path sizes to 0) -
/// a minimal local mirror of `main.rs::dir_size_bytes` so this black-box suite verifies the
/// printed byte count against an INDEPENDENT measurement, not the same function under test.
fn dir_bytes(path: &Path) -> u64 {
    let mut total = 0;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_dir() {
                stack.push(entry.path());
            } else if let Ok(md) = entry.metadata() {
                total += md.len();
            }
        }
    }
    total
}

#[test]
fn reset_build_cache_deletes_a_real_populated_cache_and_reports_its_bytes() {
    let project = temp_project();
    let root = project.path();
    seed_store(root);
    let cache = shared_cache_dir(root);
    write_file(&cache.join("debug").join("a.rlib"), &[0u8; 5_000]);
    write_file(&cache.join("debug").join("b.rlib"), &[0u8; 2_500]);
    let bytes = dir_bytes(&cache);
    assert_eq!(
        bytes, 7_500,
        "sanity: the fixture itself sums to 7500 bytes"
    );

    let (out, err, ok) = run_rigger(root, &["reset", "--build-cache"]);
    assert!(
        ok,
        "reset --build-cache must succeed on a real populated cache; stderr: {err}"
    );
    assert!(
        out.contains("--build-cache:") && out.contains("7500 byte(s)"),
        "stdout must report the exact bytes reclaimed: {out:?}"
    );
    assert!(
        !cache.exists(),
        "the cache must be gone (no restore path - cargo recreates it on demand): {cache:?}"
    );
}

#[test]
fn reset_build_cache_is_idempotent_zero_report_on_a_project_that_never_built_anything() {
    let project = temp_project();
    let root = project.path();
    seed_store(root);
    // No cache ever created at all.

    let (out, err, ok) = run_rigger(root, &["reset", "--build-cache"]);
    assert!(
        ok,
        "an absent cache must succeed with a zero report, not an error; stderr: {err}"
    );
    assert!(
        out.contains("0 byte(s)"),
        "a project that never built anything must report zero, not omit the line: {out:?}"
    );

    // Repeated call: still a clean zero, not an error - the exclusion protocol is not a
    // single-shot mechanism.
    let (out2, err2, ok2) = run_rigger(root, &["reset", "--build-cache"]);
    assert!(
        ok2,
        "repeated reset --build-cache must stay idempotent; stderr: {err2}"
    );
    assert!(
        out2.contains("0 byte(s)"),
        "the repeat must also report zero: {out2:?}"
    );
}

#[test]
fn reset_build_cache_composes_with_runs_and_derived_in_either_order() {
    let project = temp_project();
    let root = project.path();
    seed_store(root);
    write_file(&shared_cache_dir(root).join("x.rlib"), &[0u8; 10]);
    let (out, err, ok) = run_rigger(root, &["reset", "--runs", "--build-cache", "--derived"]);
    assert!(
        ok,
        "--build-cache must compose freely with BOTH siblings in one invocation; stderr: {err}"
    );
    assert!(
        out.contains("--runs:") && out.contains("--build-cache:") && out.contains("--derived:"),
        "each mode's own report line must appear: {out:?}"
    );
    assert!(
        !shared_cache_dir(root).exists(),
        "the composed call must still reclaim the cache"
    );

    // The reverse order, against a freshly re-seeded cache, must succeed identically -
    // composition is order-independent.
    write_file(&shared_cache_dir(root).join("y.rlib"), &[0u8; 10]);
    let (_out, err, ok) = run_rigger(root, &["reset", "--build-cache", "--runs"]);
    assert!(
        ok,
        "the reverse flag order must compose identically; stderr: {err}"
    );
}

#[test]
fn reset_build_cache_flag_is_registered_and_rejects_a_duplicate() {
    let project = temp_project();
    let root = project.path();
    seed_store(root);

    let (_out, err, ok) = run_rigger(root, &["reset", "--build-cache", "--build-cache"]);
    assert!(!ok, "a duplicate --build-cache must be refused");
    assert!(
        err.contains("more than once"),
        "the refusal must name the duplicate, not a generic parse failure: {err:?}"
    );
}

#[test]
fn reset_build_cache_is_not_dropped_when_composed_with_derived_on_a_server_backend() {
    // spec 77 Design: --build-cache carries "no backend requirement" (unlike --derived,
    // which is a sqlite-only mechanic). `--derived`'s static backend-mismatch refusal must
    // therefore never silently swallow a co-requested `--build-cache` - each mode sheds
    // only its own accumulation. Selects the server backend the same way an operator would
    // (KURRENTDB_CONN alone, rung 2 of the store-selection precedence) - unreachable is
    // fine, since `--build-cache` never opens the store at all.
    let project = temp_project();
    let root = project.path();
    seed_store(root);
    let cache = shared_cache_dir(root);
    write_file(&cache.join("x.rlib"), &[0u8; 16]);

    let mut cmd = common::rigger_courier();
    cmd.args(["reset", "--build-cache", "--derived"])
        .current_dir(root)
        .env("RIGGER_NO_DASH", "1")
        .env(
            "KURRENTDB_CONN",
            "http://127.0.0.1:1/unreachable-on-purpose",
        );
    let state = tempfile::tempdir().expect("create a temp XDG_STATE_HOME");
    cmd.env("XDG_STATE_HOME", state.path());
    let out = cmd.output().expect("failed to spawn the rigger binary");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    // The overall command still fails (--derived is genuinely unsupported on this backend),
    // but that must not come at the cost of silently skipping the co-requested, backend-
    // independent --build-cache: its own report line must still appear, and the cache it
    // names must actually be gone.
    assert!(
        !out.status.success(),
        "the --derived half must still refuse on a server backend"
    );
    assert!(
        stdout.contains("--build-cache:"),
        "the --build-cache report must NOT be dropped by --derived's own backend refusal: \
         stdout {stdout:?} stderr {stderr:?}"
    );
    assert!(
        !cache.exists(),
        "the shared cache must actually be reclaimed, not merely reported: {cache:?}"
    );
}

#[test]
fn reset_build_cache_refuses_rather_than_waits_while_a_build_holds_the_guard() {
    // spec 77 Design: EXCLUSIVE and NON-BLOCKING - never waiting, so no build can queue
    // behind the delete. Simulate a rigger-launched shared-cache build (`gate::
    // hold_shared_build_cache_lock`'s own shape) with a real external `flock -s`, then run
    // the real compiled binary against the identical guard path it independently derives.
    let project = temp_project();
    let root = project.path();
    seed_store(root);
    let cache = shared_cache_dir(root);
    write_file(&cache.join("debug").join("a.rlib"), &[0u8; 64]);
    let guard = guard_path(root);
    std::fs::create_dir_all(guard.parent().unwrap()).unwrap();
    std::fs::write(&guard, b"").unwrap();

    // `flock -s <path> sleep 30` forks; the locked fd is inherited by the sleep child across
    // that fork, holding a real shared flock on the guard path for the wrapper's lifetime.
    let mut holder = Command::new("flock")
        .arg("-s")
        .arg(&guard)
        .arg("sleep")
        .arg("30")
        .spawn()
        .expect("spawn an external flock holder for the fixture");

    // Wait until the external holder has actually taken the shared lock (an exclusive probe
    // must fail) before racing the real reset against it.
    let mut held = false;
    for _ in 0..200 {
        let probed = Command::new("flock")
            .arg("-n")
            .arg("-x")
            .arg(&guard)
            .arg("true")
            .status()
            .map(|s| !s.success())
            .unwrap_or(false);
        if probed {
            held = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    assert!(
        held,
        "the external fixture holder must take the guard shared before this test proceeds"
    );

    let (out, err, ok) = run_rigger(root, &["reset", "--build-cache"]);
    let _ = holder.kill();
    let _ = holder.wait();

    assert!(
        !ok,
        "reset --build-cache must refuse while the guard is held, never wait for it"
    );
    assert!(
        err.contains("in use") || err.contains("guard"),
        "the refusal must explain WHY, not just fail silently: stdout {out:?} stderr {err:?}"
    );
    assert!(
        cache.join("debug").join("a.rlib").exists(),
        "a refused reclaim must leave the cache completely untouched"
    );
}
