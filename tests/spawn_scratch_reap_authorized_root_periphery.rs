//! Periphery (cross-module, real-binary) test for spec 78 round 2's core production fix
//! (decision `u78c2r2-authorized-root-caller-supplied`) at its HIGHEST-TRAFFIC real entry
//! point: `main.rs::reclaim_spawn_registered_scratch`, the ONE reap authority both `rigger
//! result` and `cmd_step`'s liveness sweep converge on (its own doc comment says so).
//!
//! WHAT THE INSIDE-OUT TESTS ARE STRUCTURALLY BLIND TO.
//!
//! `tests/cli.rs` already drives THIS exact call chain through the real compiled binary
//! repeatedly (spec 77's `a_dotdot_spawn_id_never_escapes_the_registered_scratch_roots`,
//! `..._the_pre_existing_agent_scratch_root_either`, `a_leading_slash_spawn_id_never_collapses_
//! the_reclaim_to_its_registered_root`, `two_speculation_lanes_of_the_same_unit_get_distinct_
//! mutation_scratch_dirs`, and others) - but every one of them plants only FILES and asserts an
//! UNRELATED SIBLING's files survive. None of them plants a live process and asserts the
//! TARGET's own process actually dies. That is a structural blind spot, not an oversight: this
//! module's `reap_then_remove_dir` always calls `std::fs::remove_dir_all` unconditionally,
//! regardless of whether the reap that precedes it was a genuine kill or a silent no-op - so a
//! file-survival assertion passes IDENTICALLY either way. It cannot see the exact defect class
//! spec 78 round 1 shipped and round 2 fixed: `is_reapable_base` refusing a real, correctly
//! targeted base and turning `reap_processes_rooted_under` into an unconditional no-op
//! (`adj-u78c2-verdict-reject-reap-authority-conflict`) - the dir still gets removed either
//! way, only the LIVE PROCESS inside it tells the two cases apart.
//!
//! `mutation_scratch_reap_base_guard_periphery.rs` and
//! `worktree_remove_relocated_scratch_base_guard_periphery.rs` already close this blind spot
//! for `reclaim_unit_mutation_scratch` (spec 77 criterion 3, the unit-terminal backstop) and
//! `Worktree::remove` (the worktree-teardown path) respectively - both by calling the guarded
//! function DIRECTLY. This file closes it for the THIRD, busiest call chain neither of those
//! reaches: `cmd_result`'s per-SPAWN reclaim, driven through the compiled binary exactly as a
//! real `rigger result` invocation would (an implementer, reviewer, or adjudicator spawn
//! reporting its outcome), covering BOTH scratch roots `reclaim_spawn_registered_scratch`
//! reaps - the per-spawn `agent-scratch` dir (spec 34 criterion 1) and the registered
//! mutation-scratch dir (spec 77 criterion 2, the exact root round 1's reject was about).

use std::path::Path;
use std::process::{Child, Command};

mod common;

use rigger::driver::replay::{mutation_scratch_path, spawn_scratch_path};
use rigger::reap::processes_rooted_under;

/// A throwaway project dir that is its own git repo, mirroring `tests/cli.rs::temp_project` -
/// `project_identity()` (which scopes the namespaced streams `rigger result` reads/writes)
/// resolves deterministically off a real repo.
fn temp_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let _ = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status();
    dir
}

/// Seed an initialized `.rigger/events.db` under `root`, mirroring `tests/cli.rs::seed_store` -
/// `rigger result` refuses to fabricate a fresh store from the wrong cwd (spec 05).
fn seed_store(root: &Path) {
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(&rigger).unwrap();
    std::fs::File::create(rigger.join("events.db")).unwrap();
}

/// The project identity the binary resolves for `root`, mirroring
/// `tests/cli.rs::run_stream_identity` exactly: the tracked `.rigger/project.id` at the git
/// top-level when present, else the git top-level basename, else `root`'s own basename.
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

/// Seed a `RunStarted` event into the namespaced run stream, mirroring
/// `tests/cli.rs::seed_run_events` - `reclaim_spawn_scratch` reads the run id back out of it
/// (`runscope::current_run_id`) to resolve the SAME per-spawn `agent-scratch` path this test
/// independently computes below.
fn seed_run_started(root: &Path, run_id: &str) {
    use rigger::eventstore::namespace::Namespaced;
    use rigger::eventstore::sqlite::Store;
    use rigger::eventstore::{Event, EventStore, ExpectedRevision};

    let rigger_dir = root.join(".rigger");
    std::fs::create_dir_all(&rigger_dir).unwrap();
    let backend = Store::open(rigger_dir.join("events.db").to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    store
        .append(
            rigger::conductor::STREAM,
            ExpectedRevision::Any,
            &[Event::new(
                "RunStarted",
                format!(r#"{{"run":"{run_id}","criteria":["c"]}}"#).into_bytes(),
            )],
        )
        .unwrap();
}

/// Run `rigger <args...>` in `cwd` with extra environment `envs`, mirroring
/// `tests/cli.rs::run_rigger_envs` - opts out of the auto-started dashboard and isolates the
/// machine-global instance registry, exactly as every other CLI-driven suite in this tree
/// does, so this test never leaks a dashboard process or a phantom registry entry.
fn run_rigger_envs(cwd: &Path, args: &[&str], envs: &[(&str, &str)]) -> (String, String, bool) {
    let mut cmd = common::rigger_courier();
    cmd.args(args).current_dir(cwd);
    cmd.env("RIGGER_NO_DASH", "1");
    let state = tempfile::tempdir().expect("create a temp XDG_STATE_HOME for the rigger run");
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

/// Spawn a long-lived process rooted at `dir` that IGNORES SIGTERM, so only a SIGKILL
/// escalation can end it - mirrors the identical fixture in `src/reap.rs`, `src/worktree.rs`,
/// and this crate's sibling `*_base_guard_periphery.rs` files.
fn sigterm_ignorer_in(dir: &Path) -> Child {
    Command::new("sh")
        .arg("-c")
        .arg("trap '' TERM; while :; do sleep 1; done")
        .current_dir(dir)
        .spawn()
        .expect("spawn a SIGTERM-ignoring fixture process")
}

/// Poll up to 5s for `pred`, matching the scan/escalation latency tolerance every sibling reap
/// test in this tree already uses.
fn wait_until(mut pred: impl FnMut() -> bool) -> bool {
    for _ in 0..200 {
        if pred() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    false
}

/// Kill-and-wait a fixture child unconditionally, ignoring errors - test cleanup only, via the
/// `Child` handle it was spawned with (never a computed pid).
fn cleanup(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn rigger_result_reaps_a_live_process_in_the_spawns_registered_agent_scratch_dir() {
    let dir = temp_project();
    let root = dir.path();
    seed_store(root);
    seed_run_started(root, "r1");

    let spawn_id = "u-periphery-cli-live-reap/implementer#0";
    // No workflow.yml and no RIGGER_TMPDIR override in this fixture, so
    // `scratch_root_path_from_env` resolves the documented default:
    // `<repo>/.rigger/tmp` (`src/worktree.rs::scratch_root_path`).
    let scratch_root = root.join(".rigger").join("tmp");
    let leaf = spawn_scratch_path(scratch_root.to_str().unwrap(), "r1", spawn_id)
        .expect("a well-formed spawn id must encode to a real path");
    std::fs::create_dir_all(&leaf).unwrap();

    let mut child = sigterm_ignorer_in(&leaf);
    assert!(
        wait_until(|| processes_rooted_under(&leaf)
            .iter()
            .any(|(pid, _)| *pid == child.id())),
        "precondition: the fixture process must actually be rooted in the spawn's registered \
         agent-scratch dir before `rigger result` runs"
    );

    // A dedicated, empty cache home so the SAME call's mutation-scratch half (which
    // `reclaim_spawn_registered_scratch` always runs alongside the agent-scratch half) never
    // touches the operator's real ~/.cache.
    let cache_home = tempfile::tempdir().unwrap();
    let (out, err, ok) = run_rigger_envs(
        root,
        &["result", spawn_id, "done"],
        &[("XDG_CACHE_HOME", cache_home.path().to_str().unwrap())],
    );
    assert!(
        ok,
        "recording the result must succeed; stdout: {out:?} stderr: {err}"
    );

    let died = wait_until(|| matches!(child.try_wait(), Ok(Some(_))));
    if !died {
        cleanup(&mut child);
    }
    assert!(
        died,
        "`rigger result` must reap a live process still rooted in the spawn's own registered \
         agent-scratch dir (spec 34 criterion 1) before removing it, through the real \
         reclaim_spawn_registered_scratch call chain - a SIGTERM-ignoring process here must \
         still be SIGKILLed. Every pre-existing regression test for this call chain \
         (tests/cli.rs, spec 77's a_dotdot_spawn_id_never_escapes_... family and siblings) \
         only plants FILES and checks an unrelated sibling survives, since remove_dir_all runs \
         unconditionally either way - none of them could see a silent reap no-op here, which \
         is exactly the defect class spec 78 round 1 shipped \
         (adj-u78c2-verdict-reject-reap-authority-conflict) and round 2 \
         (u78c2r2-authorized-root-caller-supplied) fixed."
    );
}

#[test]
fn rigger_result_reaps_a_live_process_in_the_spawns_registered_mutation_scratch_dir() {
    let dir = temp_project();
    let root = dir.path();
    seed_store(root);
    seed_run_started(root, "r1");

    let spawn_id = "u-periphery-cli-live-reap-mutation/implementer#0";
    let cache_home = tempfile::tempdir().unwrap();
    let leaf = mutation_scratch_path(cache_home.path(), spawn_id)
        .expect("a well-formed spawn id must encode to a real path");
    std::fs::create_dir_all(&leaf).unwrap();

    let mut child = sigterm_ignorer_in(&leaf);
    assert!(
        wait_until(|| processes_rooted_under(&leaf)
            .iter()
            .any(|(pid, _)| *pid == child.id())),
        "precondition: the fixture process must actually be rooted in the spawn's registered \
         mutation-scratch dir before `rigger result` runs"
    );

    let (out, err, ok) = run_rigger_envs(
        root,
        &["result", spawn_id, "done"],
        &[("XDG_CACHE_HOME", cache_home.path().to_str().unwrap())],
    );
    assert!(
        ok,
        "recording the result must succeed; stdout: {out:?} stderr: {err}"
    );

    let died = wait_until(|| matches!(child.try_wait(), Ok(Some(_))));
    if !died {
        cleanup(&mut child);
    }
    assert!(
        died,
        "`rigger result` must reap a live process still rooted in the spawn's own registered \
         mutation-scratch dir (spec 77 criterion 2 - the EXACT root spec 78 round 1's reject \
         named, adj-u78c2-verdict-reject-reap-authority-conflict) before removing it, through \
         the real reclaim_spawn_registered_scratch call chain (spec 78 round 2 fix, decision \
         u78c2r2-authorized-root-caller-supplied) - a SIGTERM-ignoring process here must still \
         be SIGKILLed. This is a DIFFERENT call chain than reclaim_unit_mutation_scratch \
         (already proven directly in mutation_scratch_reap_base_guard_periphery.rs): this one \
         is keyed on ONE reporting spawn's own id via cmd_result, not a unit-terminal \
         enumeration, and every pre-existing regression test for it (tests/cli.rs) only plants \
         files, never a live process."
    );
}
