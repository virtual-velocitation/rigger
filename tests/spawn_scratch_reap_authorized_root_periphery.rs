//! Periphery (cross-module, real-binary) test for spec 78 round 2's core production fix
//! (decision `u78c2r2-authorized-root-caller-supplied`) at its HIGHEST-TRAFFIC real entry
//! point: `main.rs::reclaim_spawn_registered_scratch`, the ONE reap authority both `rigger
//! result` and `cmd_step`'s liveness sweep converge on (its own doc comment says so).
//!
//! WHAT THE INSIDE-OUT TESTS ARE STRUCTURALLY BLIND TO.
//!
//! `tests/cli.rs` already drives THIS exact call chain through the real compiled binary
//! repeatedly (spec 77's `a_dotdot_spawn_id_never_escapes_the_registered_scratch_roots`,
//! `a_leading_slash_spawn_id_never_collapses_the_reclaim_to_its_registered_root`, and others) -
//! but every one of them plants only FILES and asserts an UNRELATED SIBLING's files survive.
//! None of them plants a live process and asserts the TARGET's own process actually dies. That is a structural blind spot, not an oversight: this
//! module's `reap_then_remove_dir` always calls `std::fs::remove_dir_all` unconditionally,
//! regardless of whether the reap that precedes it was a genuine kill or a silent no-op - so a
//! file-survival assertion passes IDENTICALLY either way. It cannot see the exact defect class
//! spec 78 round 1 shipped and round 2 fixed: `is_reapable_base` refusing a real, correctly
//! targeted base and turning `reap_processes_rooted_under` into an unconditional no-op
//! (`adj-u78c2-verdict-reject-reap-authority-conflict`) - the dir still gets removed either
//! way, only the LIVE PROCESS inside it tells the two cases apart.
//!
//! `worktree_remove_relocated_scratch_base_guard_periphery.rs` already closes this blind spot
//! for `Worktree::remove` (the worktree-teardown path) by calling the guarded function
//! DIRECTLY. This file closes it for `cmd_result`'s per-SPAWN reclaim of the spawn's
//! `agent-scratch` dir (spec 34 criterion 1), driven through the compiled binary exactly as a
//! real `rigger result` invocation would (an implementer, reviewer, or adjudicator spawn
//! reporting its outcome).

use std::path::Path;

mod common;

use common::cli::run_rigger_envs;
use common::cli::run_stream_identity;
use common::cli::seed_store;
use common::cli::temp_project;
use common::cli::temp_store_project;
use common::fixtures::cleanup;
use common::fixtures::sigterm_ignorer_in;
use common::wait_until;

use rigger::driver::replay::spawn_scratch_path;
use rigger::reap::processes_rooted_under;

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

/// A throwaway project with a seeded store and a started run `r1`, plus a dedicated, empty cache
/// home: the default agent-scratch root never lands in the operator's real ~/.cache, and the
/// fixture and the child `rigger result` process (handed the SAME `XDG_CACHE_HOME`) resolve the
/// identical scratch root.
struct ReapProject {
    dir: tempfile::TempDir,
    cache_home: tempfile::TempDir,
}

impl ReapProject {
    fn new() -> Self {
        let dir = temp_project();
        seed_store(dir.path());
        seed_run_started(dir.path(), "r1");
        ReapProject {
            dir,
            cache_home: tempfile::tempdir().unwrap(),
        }
    }

    /// The agent-scratch root the binary resolves with no workflow.yml and no `RIGGER_TMPDIR`:
    /// the documented cache-home default (spec 89, criterion 2: it no longer nests under
    /// `<repo>/.rigger/tmp`), keyed off this project's cache home.
    fn agent_scratch_root(&self) -> std::path::PathBuf {
        rigger::worktree::cache_scratch_root_from(
            self.dir.path().to_str().unwrap(),
            Some(self.cache_home.path().as_os_str().to_owned()),
            None,
        )
        .expect("a non-empty repo with an explicit cache home always resolves")
    }

    /// `rigger result <spawn_id> <text>` under this project's cache home.
    fn result(&self, spawn_id: &str, text: &str) -> (String, String, bool) {
        run_rigger_envs(
            self.dir.path(),
            &["result", spawn_id, text],
            &[("XDG_CACHE_HOME", self.cache_home.path().to_str().unwrap())],
        )
    }
}

/// A SIGTERM-ignoring child rooted in `leaf` (created first), asserted to really be rooted in the
/// spawn's registered `which` dir `before` the step under test.
fn live_child_in(leaf: &Path, which: &str, before: &str) -> std::process::Child {
    std::fs::create_dir_all(leaf).unwrap();
    let child = sigterm_ignorer_in(leaf);
    assert!(
        wait_until(|| processes_rooted_under(leaf)
            .iter()
            .any(|(pid, _)| *pid == child.id())),
        "precondition: the fixture process must actually be rooted in the spawn's registered \
         {which} dir {before}"
    );
    child
}

/// Assert `child` died - reaped by the call under test (`why`) - cleaning it up otherwise.
fn assert_reaped(child: &mut std::process::Child, why: &str) {
    let died = wait_until(|| matches!(child.try_wait(), Ok(Some(_))));
    if !died {
        cleanup(child);
    }
    assert!(died, "{why}");
}

#[test]
fn rigger_result_reaps_a_live_process_in_the_spawns_registered_agent_scratch_dir() {
    let project = ReapProject::new();
    let spawn_id = "u-periphery-cli-live-reap/implementer#0";
    let leaf = spawn_scratch_path(
        project.agent_scratch_root().to_str().unwrap(),
        "r1",
        spawn_id,
    )
    .expect("a well-formed spawn id must encode to a real path");
    let mut child = live_child_in(&leaf, "agent-scratch", "before `rigger result` runs");

    let (out, err, ok) = project.result(spawn_id, "done");
    assert!(
        ok,
        "recording the result must succeed; stdout: {out:?} stderr: {err}"
    );
    assert_reaped(
        &mut child,
        "`rigger result` must reap a live process still rooted in the spawn's own registered \
         agent-scratch dir (spec 34 criterion 1) before removing it, through the real \
         reclaim_spawn_registered_scratch call chain - a SIGTERM-ignoring process here must \
         still be SIGKILLed. Every pre-existing regression test for this call chain \
         (tests/cli.rs, spec 77's a_dotdot_spawn_id_never_escapes_... family and siblings) \
         only plants FILES and checks an unrelated sibling survives, since remove_dir_all runs \
         unconditionally either way - none of them could see a silent reap no-op here, which \
         is exactly the defect class spec 78 round 1 shipped \
         (adj-u78c2-verdict-reject-reap-authority-conflict) and round 2 \
         (u78c2r2-authorized-root-caller-supplied) fixed.",
    );
}

/// A DIFFERENT axis of `reclaim_spawn_scratch`'s own boundary than the rest of this file
/// (spec 83 criterion 2 round 2, not spec 78): WHICH ROOT it reaps under. `reclaim_spawn_
/// scratch`'s round-2 fix (the reject's own required follow-up to a half-applied round-1 fix)
/// makes it resolve `defaults.workdir` via the shared, validate-independent `scratch_defaults`,
/// never `config::load`, which additionally requires a fully loadable `.rigger/agents/` fleet
/// just to learn that one string field, and silently zeroed it via `.unwrap_or_default()`
/// whenever that fleet was absent (this project's OWN committed `.rigger/workflow.yml`, with
/// no agents fleet in THIS fixture, is the exact shape). This test reuses the live-process
/// fixture above for the SAME reason it does there: a misresolved root does not merely miss a
/// field, it reaps the WRONG (default) directory while a process quietly keeps running in the
/// CONFIGURED one forever, and only a live process (never a file-survival check) tells "reaped
/// the right root" apart from "silently reaped nothing relevant".
#[test]
fn rigger_result_reaps_a_live_process_from_the_owning_roots_configured_workdir_with_no_agents_fleet_present(
) {
    let dir = temp_store_project();
    let root = dir.path();
    seed_run_started(root, "r1");

    let relocated = tempfile::tempdir().expect("create relocated workdir");
    std::fs::write(
        root.join(".rigger").join("workflow.yml"),
        format!(
            "defaults:\n  workdir: \"{}\"\n",
            relocated.path().to_string_lossy()
        ),
    )
    .expect("write the owning root's workflow.yml with a configured workdir");
    // Fixture guard: no `.rigger/agents/` dir exists at the owning root either - confirms this
    // test genuinely exercises the validate-independent axis of the fix, not just field
    // plumbing.
    assert!(
        !root.join(".rigger").join("agents").exists(),
        "fixture bug: this test requires an agents-less owning root to exercise the \
         validate-independent axis of the fix"
    );

    let spawn_id = "u-periphery-cli-live-reap-configured-workdir/implementer#0";
    let scratch_root = rigger::worktree::scratch_root_path_from_env(
        root.to_str().unwrap(),
        relocated.path().to_str().unwrap(),
    );
    // Fixture guard: the configured scratch root genuinely differs from the crate's own
    // documented DEFAULT (spec 89, criterion 2: the cache-home root, or the pre-relocation
    // `<repo>/.rigger/tmp` degrade on a homeless host) - else this test cannot discriminate
    // the fix from a regression that silently fell back to the default because
    // `config::load` failed on this agents-less root.
    let default_scratch_root =
        rigger::worktree::scratch_root_path(root.to_str().unwrap(), "", None);
    assert_ne!(
        scratch_root, default_scratch_root,
        "fixture bug: the configured workdir must resolve a scratch root distinct from the \
         crate's own default, else this test cannot discriminate the fix"
    );

    let leaf = spawn_scratch_path(&scratch_root, "r1", spawn_id)
        .expect("a well-formed spawn id must encode to a real path");
    std::fs::create_dir_all(&leaf).unwrap();

    let mut child = sigterm_ignorer_in(&leaf);
    assert!(
        wait_until(|| processes_rooted_under(&leaf)
            .iter()
            .any(|(pid, _)| *pid == child.id())),
        "precondition: the fixture process must actually be rooted in the spawn's registered \
         agent-scratch dir (under the CONFIGURED workdir) before `rigger result` runs"
    );

    // A dedicated, empty cache home so the call never reads the operator's real ~/.cache, and
    // `RIGGER_TMPDIR` explicitly cleared so this test's outcome cannot depend on whatever
    // scratch relocation the surrounding gate/CI happens to be running under - the same
    // env-override-free precedence rung the fixture above assumes.
    let cache_home = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().expect("create a temp XDG_STATE_HOME for the rigger run");
    let mut cmd = common::rigger_courier();
    cmd.args(["result", spawn_id, "done"])
        .current_dir(root)
        .env("RIGGER_NO_DASH", "1")
        .env("XDG_STATE_HOME", state.path())
        .env("XDG_CACHE_HOME", cache_home.path())
        .env_remove("RIGGER_TMPDIR");
    let output = cmd.output().expect("failed to spawn the rigger binary");
    assert!(
        output.status.success(),
        "recording the result must succeed even when the owning root has no agents fleet at \
         all; stdout: {:?} stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let died = wait_until(|| matches!(child.try_wait(), Ok(Some(_))));
    if !died {
        cleanup(&mut child);
    }
    assert!(
        died,
        "`rigger result` must reap a live process rooted in the spawn's agent-scratch dir \
         under the OWNING ROOT'S CONFIGURED `defaults.workdir` - resolved without requiring a \
         loadable agents fleet there - not silently no-op onto the crate's default root \
         instead: got a still-alive fixture process, meaning the reap targeted the wrong \
         directory"
    );
}
