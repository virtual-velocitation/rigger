//! Spec 62, criterion "COURIERS KEEP THE INSTANCE LIVE" (unit u62c4), driven through the BUILT
//! `rigger` binary.
//!
//! Before this unit, only the run driver (`run`/`step`/`serve`, via `register_run_instance`)
//! ever touched the machine-global instance registry (spec 50): a one-shot registration plus a
//! background heartbeat thread held for the life of the run. A courier (`progress`, `emit`,
//! `result`) is a separate, short-lived invocation with no such thread and no registration call
//! at all - so a run whose only activity for a stretch is courier traffic (an agent phase longer
//! than the registry's 900s idle window with no in-process driver heartbeating) silently ages
//! out of the registry mid-run, and any dash reading it sees the run as gone (`f-dash-
//! selfreap-blind-to-agent-work`).
//!
//! This suite proves the fix genuinely reaches the shipped binary: each courier command
//! refreshes the SAME registry entry a driver's heartbeat would (never accumulating a second
//! entry per call), and - the headline claim - a single courier call revives an entry the
//! registry's own idle-window judgment (`is_stale`) already considers aged out, with no driver
//! and no heartbeat thread involved at all.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;

use rigger::eventstore::sqlite::Store;
use rigger::registry::{self, Instance, DEFAULT_IDLE_MS};

// The compiled `rigger` binary under test is located at RUNTIME by the shared authority in
// `tests/common`: a path baked in at compile time goes stale the moment the target dir moves,
// and every suite that spawns the product then dies with a bare NotFound.
mod common;

/// A throwaway project the compiled binary accepts as a courier target: its own git repo (so the
/// store's project identity resolves normally) and an INITIALIZED event log - a courier refuses
/// to fabricate one from a cwd with no existing store (spec 05).
fn courier_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a temp project");
    let root = dir.path();
    let _ = Command::new("git")
        .args(["init", "-q"])
        .current_dir(root)
        .status();
    let rigger_dir = root.join(".rigger");
    std::fs::create_dir_all(&rigger_dir).expect("create .rigger");
    Store::open(
        rigger_dir
            .join("events.db")
            .to_str()
            .expect("a utf-8 store path"),
    )
    .expect("the event log initializes");
    dir
}

/// Run `rigger <args...>` in `root` through the COMPILED binary, with the machine-global
/// registry redirected into the CALLER-OWNED `state_home` - shared across several calls within
/// one test (unlike a fresh-per-call temp dir), so the SAME registry directory is read back and
/// re-written across a sequence of courier invocations.
fn run_rigger(root: &Path, state_home: &Path, args: &[&str]) -> Output {
    common::rigger_courier()
        .args(args)
        .current_dir(root)
        // Never let a short-lived courier spawn a real dashboard under test.
        .env("RIGGER_NO_DASH", "1")
        .env("XDG_STATE_HOME", state_home)
        .output()
        .expect("the rigger binary runs")
}

fn assert_ok(out: &Output, args: &[&str]) {
    assert!(
        out.status.success(),
        "rigger {args:?} failed: {}\n{}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
}

/// Every registry entry under `state_home`, decoded through `Instance`'s own (de)serialization -
/// a raw directory read, so a test can see exactly what a courier wrote without depending on
/// `read_live`'s pruning (which mutates the directory as a side effect of reading it).
fn registry_entries(state_home: &Path) -> Vec<(PathBuf, Instance)> {
    let dir = registry::instances_dir(state_home);
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if let Ok(body) = std::fs::read(&path) {
            if let Ok(inst) = serde_json::from_slice::<Instance>(&body) {
                out.push((path, inst));
            }
        }
    }
    out
}

/// EACH of the three courier commands - `progress`, `emit`, `result` - refreshes the project's
/// registry entry, and repeated calls refresh the SAME entry in place rather than accumulating a
/// duplicate per call (mirroring the driver's own re-registration-updates-one-entry contract,
/// `registry::write`'s own doc comment). A short sleep between calls makes each refreshed
/// heartbeat provably LATER than the last, not merely unchanged-and-coincidentally-equal.
#[test]
fn progress_emit_and_result_each_refresh_one_registry_entry_in_place() {
    let project = courier_project();
    let root = project.path();
    let state = tempfile::tempdir().expect("a temp XDG_STATE_HOME");

    let before = registry::now_ms();
    let out1 = run_rigger(root, state.path(), &["progress", "u1/impl#0", "step one"]);
    assert_ok(&out1, &["progress", "u1/impl#0", "step one"]);
    let after1 = registry::now_ms();

    let entries1 = registry_entries(state.path());
    assert_eq!(
        entries1.len(),
        1,
        "one courier call (progress) writes exactly one registry entry: {entries1:?}"
    );
    let id = entries1[0].0.file_name().unwrap().to_owned();
    let hb1 = entries1[0].1.heartbeat_ms;
    assert!(
        hb1 >= before && hb1 <= after1,
        "the entry's heartbeat is stamped at courier-invocation time: {hb1} not in [{before}, {after1}]"
    );

    std::thread::sleep(Duration::from_millis(5));
    let out2 = run_rigger(
        root,
        state.path(),
        &["emit", "DecisionMade", r#"{"id":"d1","summary":"s"}"#],
    );
    assert_ok(
        &out2,
        &["emit", "DecisionMade", r#"{"id":"d1","summary":"s"}"#],
    );
    let entries2 = registry_entries(state.path());
    assert_eq!(
        entries2.len(),
        1,
        "a second courier call (emit) refreshes the SAME entry, never a second one: {entries2:?}"
    );
    assert_eq!(
        entries2[0].0.file_name().unwrap(),
        id,
        "emit refreshes the identical entry file progress just wrote"
    );
    assert!(
        entries2[0].1.heartbeat_ms > hb1,
        "emit bumps the heartbeat forward: {} then {}",
        hb1,
        entries2[0].1.heartbeat_ms
    );

    std::thread::sleep(Duration::from_millis(5));
    let out3 = run_rigger(root, state.path(), &["result", "u1/impl#0", "done"]);
    assert_ok(&out3, &["result", "u1/impl#0", "done"]);
    let entries3 = registry_entries(state.path());
    assert_eq!(
        entries3.len(),
        1,
        "a third courier call (result) still refreshes the SAME entry: {entries3:?}"
    );
    assert_eq!(
        entries3[0].0.file_name().unwrap(),
        id,
        "result refreshes the identical entry file, never a duplicate"
    );
    assert!(
        entries3[0].1.heartbeat_ms > entries2[0].1.heartbeat_ms,
        "result bumps the heartbeat forward again: {} then {}",
        entries2[0].1.heartbeat_ms,
        entries3[0].1.heartbeat_ms
    );
}

/// THE HEADLINE CLAIM: an instance whose only activity is courier traffic stays in `read_live`
/// past where it ages out TODAY. This seeds a genuine registry entry (learning the exact id/root/
/// store the binary computes for this project from a real courier write, rather than replicating
/// that resolution logic here), ages its heartbeat past the registry's own idle window - the
/// state a run left with no driver heartbeating for that long is in RIGHT NOW, before this unit's
/// fix - and proves a single courier call alone (no driver, no heartbeat thread) revives it.
#[test]
fn a_courier_call_revives_an_entry_the_registrys_own_idle_window_already_calls_stale() {
    let project = courier_project();
    let root = project.path();
    let state = tempfile::tempdir().expect("a temp XDG_STATE_HOME");

    // Learn the real id/root/store this project resolves to, from one genuine courier write.
    let seed = run_rigger(
        root,
        state.path(),
        &["progress", "u1/impl#0", "first activity"],
    );
    assert_ok(&seed, &["progress", "u1/impl#0", "first activity"]);
    let mut entries = registry_entries(state.path());
    assert_eq!(
        entries.len(),
        1,
        "the seeding call writes one entry: {entries:?}"
    );
    let (path, mut inst) = entries.remove(0);

    // Age it out: stamp a heartbeat well past the registry's idle window relative to NOW, and
    // write it back to the SAME file (same id) - exactly the state a run left with no driver
    // heartbeating for that stretch, before couriers counted as activity.
    let now = registry::now_ms();
    let stale_heartbeat = now.saturating_sub(DEFAULT_IDLE_MS + 5_000);
    inst.heartbeat_ms = stale_heartbeat;
    std::fs::write(&path, serde_json::to_vec_pretty(&inst).unwrap()).expect("age the entry");
    assert!(
        registry::is_stale(stale_heartbeat, registry::now_ms(), DEFAULT_IDLE_MS),
        "the seeded heartbeat must already be judged stale under the registry's own idle-window rule"
    );
    assert!(
        path.exists(),
        "the aged entry file exists on disk before any further courier call"
    );

    // ONE courier call - the exact traffic this criterion covers - and nothing else: no driver,
    // no heartbeat thread.
    let revive = run_rigger(
        root,
        state.path(),
        &["progress", "u1/impl#0", "second activity"],
    );
    assert_ok(&revive, &["progress", "u1/impl#0", "second activity"]);

    // The SAME entry (never a duplicate) is live again under the registry's own reader.
    let dir = registry::instances_dir(state.path());
    let live = registry::read_live(&dir, registry::now_ms(), DEFAULT_IDLE_MS);
    assert_eq!(
        live.len(),
        1,
        "the courier call revives exactly the one aged-out entry: {live:?}"
    );
    assert!(
        live[0].heartbeat_ms > stale_heartbeat,
        "the revived heartbeat is fresher than the aged one: {} vs {}",
        live[0].heartbeat_ms,
        stale_heartbeat
    );

    let after_files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("json"))
        .collect();
    assert_eq!(
        after_files.len(),
        1,
        "no duplicate entry was created; the aged entry's own id was refreshed in place"
    );
}

/// The degrade path: a HOMELESS environment (no `XDG_STATE_HOME`, no `HOME`) must never fail,
/// slow, or warn away a courier's real work - the registry's loss is harmless (spec 50), and this
/// unit's refresh call inherits that degrade exactly like `register_run_instance` already does.
#[test]
fn a_homeless_environment_never_fails_a_courier_command() {
    let project = courier_project();
    let root = project.path();

    let out = common::rigger_courier()
        .args(["progress", "u1/impl#0", "did a thing"])
        .current_dir(root)
        .env("RIGGER_NO_DASH", "1")
        .env_remove("XDG_STATE_HOME")
        .env_remove("HOME")
        .output()
        .expect("the rigger binary runs");
    assert!(
        out.status.success(),
        "a homeless environment must not fail the courier's real work; stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("progress recorded for u1/impl#0"),
        "the courier's own output is unaffected by the registry's degrade: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}
