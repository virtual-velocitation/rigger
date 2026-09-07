//! Periphery for spec 83, criterion 2 - HEARTBEATS ARE VISIBLE AGAIN: the write/read
//! agreement between wherever a spawn's liveness marker is actually written (the run's
//! resolved scratch root, anchored at the store's OWNING repo root) and wherever `rigger
//! status` reads it back from, pinned at the REAL writer/reader seam through the compiled
//! binary - never a mock of either side.
//!
//! WHY THIS FILE, DISTINCT FROM THE IMPLEMENTER'S OWN TEST. `main.rs`'s own `mod tests`
//! (`store_location_repo_root_resolves_the_owning_root_not_the_process_cwd_so_a_real_marker_
//! is_found`) proves `StoreLocation::repo_root` and the private `liveness_ages_for_wave`
//! helper agree IN PROCESS, against a `StoreLocation` it fabricates by hand and a `WaveItem`
//! it builds directly. It never drives `cmd_status` itself, never reproduces a REAL
//! git-linked worktree (the actual shape the fix's own doc comment names - "a courier
//! invoked from a nested unit worktree"), and never proves the criterion's own literal
//! claim: "a working spawn's heartbeat age renders in `rigger status`". `cmd_status` and
//! `require_store_dir` are private to the `rigger` BINARY crate, unreachable from an
//! integration-test crate under `tests/` by any means other than spawning the compiled
//! binary (mirrors `tests/cause_wire_periphery.rs`'s identical situation for the SAME
//! command, and `tests/watchdog_cli_periphery.rs`'s for `cmd_watch`).
//!
//! This file drives the compiled `rigger status` binary, from a cwd inside a REAL git-linked
//! worktree nested under a REAL owning repo (the deterministic shape a run's unit worktrees
//! take), against a marker written at the exact path the real writer (`rigger step`'s own
//! `scratch_root`/`marker_path` composition - the wire `WaveItem::marker_path` the thin
//! driver frames its `touch` instruction around) would have stamped it. Two regressions this
//! guards against, both reproducing spec 83's own Problem statement ("the per-spawn liveness
//! marker the sweep would consult is absent even while the agent is demonstrably alive"):
//! resolving the scratch root from the process's raw cwd (`git_repo()`) instead of the
//! store's owning root, and `cmd_status`/`watch_poll` independently re-deriving that root
//! and drifting out of step with each other.
//!
//! NOT OWNED HERE: the fence deciding whether a worktree with NO marker is reapable (spec
//! 83, criterion 1's own periphery test); the `Step`/`WaveItem` fold itself (owned by
//! `spawn.rs`'s own tests); `progress::consolidate`'s age arithmetic (owned by
//! `progress.rs`'s own tests). This file only proves that whichever heartbeat age those
//! already-tested pieces would compute actually gets READ from where the REAL write landed,
//! through the real binary, from the real nested-worktree cwd the bug's own doc comment
//! names.

mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Event, EventStore, ExpectedRevision};

const RUN_ID: &str = "r-heartbeat-seam";
const SPAWN_ID: &str = "seam-unit/implementer#0";

/// Run `git <args>` in `dir`, asserting success - a fixture-setup failure here is a bug in
/// this file, not the behavior under test.
fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    assert!(ok, "git {args:?} must succeed in {dir:?}");
}

/// The git top-level directory containing `dir`, resolved with `git -C <dir>` - mirrors
/// `src/main.rs`'s own `git_repo_at`, the exact raw-cwd computation the fix replaces. Used
/// here only to PROVE the nested worktree's own top-level genuinely diverges from the owning
/// root, the precondition that gives this test its discriminating power.
fn git_toplevel(dir: &Path) -> String {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// A throwaway MAIN repo with one commit (so `git worktree add` has a base to branch from) -
/// the OWNING root a run's driver (`rigger step`, always invoked from the repo root) stamps
/// a spawn's liveness marker under.
fn main_repo_with_commit() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create temp project");
    git(dir.path(), &["init", "-q"]);
    git(
        dir.path(),
        &[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "base",
        ],
    );
    dir
}

/// A git-LINKED worktree nested under `root` - the deterministic scratch-root shape a run
/// spawns its units into, and the exact shape `require_store_dir`'s own doc comment names
/// ("most plausibly a unit worktree"). Its own `git rev-parse --show-toplevel` is the
/// WORKTREE path, distinct from `root` - the divergence a cwd-based resolution mistakes for
/// the owning root.
fn nested_worktree(root: &Path, name: &str) -> PathBuf {
    let nested = root.join(".rigger").join("tmp").join(name);
    std::fs::create_dir_all(nested.parent().unwrap()).unwrap();
    git(root, &["worktree", "add", "-q", nested.to_str().unwrap()]);
    nested
}

/// Seed an initialized, empty `.rigger/events.db` under `root` - stands in for the store a
/// prior `rigger run`/`step` would have created. Mirrors `tests/cause_wire_periphery.rs`'s
/// `seed_store`.
fn seed_store(root: &Path) {
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(&rigger).unwrap();
    std::fs::File::create(rigger.join("events.db")).unwrap();
}

/// The project identity the binary resolves for `root` - mirrors
/// `tests/cause_wire_periphery.rs`'s `run_stream_identity`, itself mirroring
/// `StoreLocation::identity`'s precedence: the tracked `.rigger/project.id` at the git
/// top-level when present, else the git top-level basename, else `root`'s own basename.
fn run_stream_identity(root: &Path) -> String {
    let toplevel = git_toplevel(root);
    let base = if toplevel.is_empty() {
        root
    } else {
        Path::new(&toplevel)
    };
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

/// Append `events` directly to `root`'s namespaced run stream through a REAL `Store::open` /
/// SQLite round trip - standing in for the conductor minting them. Mirrors
/// `tests/cause_wire_periphery.rs`'s `seed_run_events`.
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

/// Seed a run with exactly one in-flight spawn (`SPAWN_ID`, no recorded result) at `root`.
fn seed_in_flight_spawn(root: &Path) {
    seed_store(root);
    seed_run_events(
        root,
        &[
            (
                "RunStarted",
                &format!(r#"{{"run":"{RUN_ID}","criteria":["seam"]}}"#),
            ),
            (
                "SpawnRequested",
                &format!(
                    r#"{{"id":"{SPAWN_ID}","unit":"seam-unit","stage":"implementer","prompt":"do the seam work"}}"#
                ),
            ),
        ],
    );
}

/// Run `rigger <args>` with cwd `dir`. `RIGGER_TMPDIR` is explicitly cleared (never merely
/// left unset in the ambient environment) so this test's outcome cannot depend on whatever
/// scratch relocation the surrounding gate/CI happens to be running under - the fixture pins
/// the SAME assumption (`env_override: None`) [`real_scratch_root`] below computes for the
/// write side, so both sides are guaranteed to agree on the precedence rung in play,
/// regardless of ambient configuration. Mirrors `tests/cause_wire_periphery.rs`'s
/// `run_rigger`.
fn run_rigger(dir: &Path, args: &[&str]) -> Output {
    let state = tempfile::tempdir().expect("create a temp XDG_STATE_HOME");
    common::rigger_courier()
        .args(args)
        .current_dir(dir)
        .env("RIGGER_NO_DASH", "1")
        .env("XDG_STATE_HOME", state.path())
        .env_remove("RIGGER_TMPDIR")
        .output()
        .expect("failed to spawn the rigger binary")
}

/// The scratch root the REAL writer (`rigger step`) resolves for a spawn placed at
/// `owning_root` under a `configured` `defaults.workdir` (pass `""` for the unconfigured,
/// default-rung case), using the env-override-free precedence rung (`env_override: None`) -
/// the SAME rung [`run_rigger`] guarantees for the spawned reader by clearing
/// `RIGGER_TMPDIR` on its `Command`. Bypassing `scratch_root_from_env`'s own environment
/// read here (rather than relying on this test PROCESS having no ambient `RIGGER_TMPDIR`,
/// which a concurrently-running gate cannot guarantee) is what keeps this fixture
/// deterministic regardless of what the surrounding process environment carries.
fn real_scratch_root(owning_root: &Path, configured: &str) -> String {
    rigger::worktree::scratch_root(owning_root.to_str().unwrap(), configured, None)
}

/// Write a liveness marker at the path the REAL writer computes for `owning_root` with no
/// configured `defaults.workdir` (the default-rung case) - the same `scratch_root` +
/// `marker_path` composition `rigger step` stamps a wave item's `marker_path` with (the wire
/// path the thin driver frames the worker's `touch` around).
fn write_real_marker(owning_root: &Path) -> PathBuf {
    let scratch_root = real_scratch_root(owning_root, "");
    let marker = rigger::liveness::marker_path(&scratch_root, RUN_ID, SPAWN_ID)
        .expect("a spawn id must always resolve a marker path");
    std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
    std::fs::write(&marker, b"heartbeat").unwrap();
    marker
}

/// The headline boundary proof: `rigger status --json`, invoked from INSIDE a nested unit
/// worktree with a DIFFERENT git top-level than the owning repo the marker was written
/// under, still reports the spawn's real heartbeat age - because both the write and the read
/// resolve the scratch root from the store's OWNING root, never the process's raw cwd.
#[test]
fn status_from_a_nested_worktree_reports_the_heartbeat_written_at_the_owning_root() {
    let project = main_repo_with_commit();
    let root = project.path();
    seed_in_flight_spawn(root);

    let nested = nested_worktree(root, "rigger-wt-seam-json");

    // Precondition guard: the nested worktree's own git top-level must genuinely differ from
    // the owning root, else this test cannot discriminate the owning-root binding from a raw
    // cwd read (mirrors `tests/store_resolution.rs`'s identical guard on its nested-worktree
    // case).
    let root_toplevel = git_toplevel(root);
    let nested_toplevel = git_toplevel(&nested);
    assert!(
        !root_toplevel.is_empty() && root_toplevel != nested_toplevel,
        "fixture bug: the nested worktree must resolve a DISTINCT git top-level ({nested_toplevel:?}) \
         from the owning root ({root_toplevel:?}), else this test cannot discriminate a \
         cwd-based resolution from the owning-root one"
    );

    write_real_marker(root);

    // The WRONG (cwd-based) scratch root a `git_repo()` regression would resolve from inside
    // the nested worktree - proven to differ from the real one, and to hold NO marker, so a
    // regression back to cwd-based resolution would make this test fail loudly (heartbeat
    // absent) rather than pass vacuously.
    let wrong_scratch_root = real_scratch_root(&nested, "");
    assert_ne!(
        wrong_scratch_root,
        real_scratch_root(root, ""),
        "fixture bug: the nested worktree's own scratch root must differ from the owning \
         root's, else this test cannot discriminate the fix from the cwd-based regression it \
         guards against"
    );
    let wrong_marker = rigger::liveness::marker_path(&wrong_scratch_root, RUN_ID, SPAWN_ID)
        .expect("marker path must resolve");
    assert!(
        !wrong_marker.exists(),
        "fixture bug: a marker exists at the WRONG (cwd-based) location {wrong_marker:?}"
    );

    // `rigger status --json`, run FROM the nested worktree - the exact surface a courier
    // invoked from inside a unit worktree uses.
    let out = run_rigger(&nested, &["status", "--json"]);
    assert!(
        out.status.success(),
        "rigger status --json from a nested worktree must succeed; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout.lines().next().unwrap_or_default();
    let value: serde_json::Value = serde_json::from_str(line)
        .unwrap_or_else(|e| panic!("status --json must print one JSON line: {e}; got {line:?}"));
    let agents = value.as_array().expect("status --json prints a bare array");
    let agent = agents
        .iter()
        .find(|a| a.get("id").and_then(|v| v.as_str()) == Some(SPAWN_ID))
        .unwrap_or_else(|| {
            panic!("the in-flight spawn {SPAWN_ID:?} must appear in status --json; got {agents:?}")
        });
    let age = agent.get("liveness_age_s").and_then(|v| v.as_u64());
    assert!(
        age.is_some(),
        "the spawn's heartbeat age must be present (write and read must agree on the marker \
         location, not resolve it from the process's raw cwd): got {agent:?}"
    );
    // Generous upper bound: only this test's own wall-clock overhead separates the touch from
    // the read, never the scratch-root divergence this test guards against (which reports NO
    // age at all, not a large one).
    assert!(
        age.unwrap() < 120,
        "the reported heartbeat age must reflect the marker just touched, not a mis-scoped \
         read: got {age:?}"
    );
}

/// Same seam, the HUMAN-readable surface: `rigger status` (no `--json`) must render an
/// actual heartbeat age for the in-flight spawn, never the `heartbeat -` spec 83's own
/// Problem statement names as the observed symptom of the write/read disagreement.
#[test]
fn status_human_output_from_a_nested_worktree_never_prints_a_dash_heartbeat_for_a_live_spawn() {
    let project = main_repo_with_commit();
    let root = project.path();
    seed_in_flight_spawn(root);
    let nested = nested_worktree(root, "rigger-wt-seam-human");
    write_real_marker(root);

    let out = run_rigger(&nested, &["status"]);
    assert!(
        out.status.success(),
        "rigger status from a nested worktree must succeed; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(SPAWN_ID),
        "the in-flight spawn must appear in the human status table; got: {stdout:?}"
    );
    assert!(
        !stdout.contains("heartbeat -"),
        "a working spawn's heartbeat must render an age, never the absent-marker dash that is \
         spec 83's own Problem-statement symptom of write/read disagreement; got: {stdout:?}"
    );
    assert!(
        stdout.contains("heartbeat ") && stdout.contains("s ago"),
        "expected a rendered heartbeat age line (\"heartbeat <n>s ago\"); got: {stdout:?}"
    );
}

/// ROUND 2 of this same seam (spec 83 criterion 2's own reject: a half-applied round-1
/// fix). The round-1 fixture above (`status_from_a_nested_worktree_reports_the_heartbeat_
/// written_at_the_owning_root`) never configures `defaults.workdir` at all, so both the
/// buggy cwd-based read AND the fixed owning-root-based read land on the SAME default-rung
/// scratch root (`<repo>/.rigger/tmp`) - it cannot discriminate the fix from the
/// regression it means to guard against. This test closes that hole on TWO axes at once,
/// exactly as sdet/the adversary found them: (1) a REAL, non-default `defaults.workdir`
/// configured at the OWNING root, invisible from the nested worktree's own cwd (`git
/// worktree add` only ever checks out TRACKED content, and this workflow.yml is
/// deliberately never committed); (2) the owning root has NO `.rigger/agents/` fleet at
/// all, the exact shape `config::load` refuses outright - proving the read does not
/// silently fall back to the empty default merely because a full config load would have
/// failed.
#[test]
fn status_resolves_a_configured_workdir_from_the_owning_root_with_no_agents_fleet_present() {
    let project = main_repo_with_commit();
    let root = project.path();
    seed_in_flight_spawn(root);
    let nested = nested_worktree(root, "rigger-wt-workdir-seam");

    let relocated = tempfile::tempdir().expect("create relocated workdir");
    std::fs::write(
        root.join(".rigger").join("workflow.yml"),
        format!(
            "name: w\ndefaults:\n  workdir: \"{}\"\n",
            relocated.path().to_string_lossy()
        ),
    )
    .expect("write the owning root's workflow.yml with a configured workdir");
    // Fixture guard: no `.rigger/agents/` dir exists at the owning root either - confirms
    // this test genuinely exercises the validate-independent axis of the fix, not just the
    // cwd-vs-owning-root one.
    assert!(
        !root.join(".rigger").join("agents").exists(),
        "fixture bug: this test requires an agents-less owning root to exercise the \
         validate-independent axis of the fix"
    );

    // The REAL writer's path composition, using the configured workdir directly (mirrors
    // `write_real_marker`, generalized to a non-default workdir).
    let scratch_root = real_scratch_root(root, relocated.path().to_str().unwrap());
    let marker = rigger::liveness::marker_path(&scratch_root, RUN_ID, SPAWN_ID)
        .expect("a spawn id must always resolve a marker path");
    std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
    std::fs::write(&marker, b"heartbeat").unwrap();

    // The WRONG path a `config::load(".")`-from-nested-cwd regression would resolve: the
    // nested worktree's own cwd has no workflow.yml at all (never committed), so it falls
    // through to the empty-workdir default rung - a DIFFERENT scratch root than the
    // configured one above, so this test cannot pass vacuously.
    let wrong_scratch_root = real_scratch_root(&nested, "");
    assert_ne!(
        wrong_scratch_root, scratch_root,
        "fixture bug: the cwd-based (nested, default-workdir) resolution must differ from \
         the owning-root-configured one, else this test cannot discriminate the fix"
    );

    let out = run_rigger(&nested, &["status", "--json"]);
    assert!(
        out.status.success(),
        "rigger status --json from a nested worktree must succeed even when the owning \
         root has no agents fleet at all; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout.lines().next().unwrap_or_default();
    let value: serde_json::Value = serde_json::from_str(line)
        .unwrap_or_else(|e| panic!("status --json must print one JSON line: {e}; got {line:?}"));
    let agents = value.as_array().expect("status --json prints a bare array");
    let agent = agents
        .iter()
        .find(|a| a.get("id").and_then(|v| v.as_str()) == Some(SPAWN_ID))
        .unwrap_or_else(|| {
            panic!("the in-flight spawn {SPAWN_ID:?} must appear in status --json; got {agents:?}")
        });
    let age = agent.get("liveness_age_s").and_then(|v| v.as_u64());
    assert!(
        age.is_some(),
        "the spawn's heartbeat age must be present: the configured defaults.workdir must be \
         read from the OWNING root (never the process's raw cwd), and that read must not \
         require a loadable agents fleet at that root either: got {agent:?}"
    );
    assert!(
        age.unwrap() < 120,
        "the reported heartbeat age must reflect the marker just touched under the \
         CONFIGURED workdir, not a mis-scoped read: got {age:?}"
    );
}
