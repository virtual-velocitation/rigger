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
    // A dedicated, empty cache home so the SAME call's mutation-scratch half (which
    // `reclaim_spawn_registered_scratch` always runs alongside the agent-scratch half) never
    // touches the operator's real ~/.cache - and so the fixture below and the CHILD process
    // (given the SAME override further down) resolve the identical agent-scratch root too
    // (spec 89, criterion 2: the default no longer nests under `<repo>/.rigger/tmp`).
    let cache_home = tempfile::tempdir().unwrap();
    // No workflow.yml and no RIGGER_TMPDIR override in this fixture, so
    // `scratch_root_path_from_env` resolves the documented default: the cache-home root
    // (`src/worktree.rs::scratch_root_path`/`cache_scratch_root_from`), keyed off THIS SAME
    // `cache_home` the child process below is also handed via `XDG_CACHE_HOME`.
    let scratch_root = rigger::worktree::cache_scratch_root_from(
        root.to_str().unwrap(),
        Some(cache_home.path().as_os_str().to_owned()),
        None,
    )
    .expect("a non-empty repo with an explicit cache home always resolves");
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
    let dir = temp_project();
    let root = dir.path();
    seed_store(root);
    seed_run_started(root, "r1");

    let relocated = tempfile::tempdir().expect("create relocated workdir");
    std::fs::write(
        root.join(".rigger").join("workflow.yml"),
        format!(
            "name: w\ndefaults:\n  workdir: \"{}\"\n",
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

    // A dedicated, empty cache home for the mutation-scratch half of the same call, and
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

/// The literal refusal text `is_reapable_base` prints to stderr (`src/reap.rs`) when it
/// refuses a base - the ONE string every assertion below checks is ABSENT, since spec 89
/// criterion 3's whole point is that a gone-but-under-root target is authorized silently, not
/// refused loudly.
const REAP_REFUSED_TEXT: &str = "not strictly under";

/// Spec 89 criterion 3 (THE RECLAIM GUARD COMPARES PATHS), extending this file's own real
/// per-spawn `cmd_result` call chain to the exact production incident the criterion closes.
///
/// WHAT THE INSIDE-OUT TESTS ARE STRUCTURALLY BLIND TO.
///
/// `src/reap.rs`'s own unit tests (`is_reapable_base_authorizes_a_gone_target_...`,
/// `reap_kills_a_process_whose_base_dir_was_already_removed_before_the_reap_call`) prove the
/// fix entirely through a bare `FakeRepo` fixture calling the private `is_reapable_base` and
/// the pub `reap_processes_rooted_under` directly - never through `rigger result`, so they
/// cannot see whether the fix actually reaches the ONE real caller that ever hands a
/// POSSIBLY-NONEXISTENT path to the reap without first checking: `main.rs::
/// reclaim_spawn_registered_scratch` (this file's own header doc comment already names it the
/// "HIGHEST-TRAFFIC real entry point"). `reclaim_unit_mutation_scratch` (closed by
/// `mutation_scratch_reap_base_guard_periphery.rs`) cannot reach this case either - it only
/// ever reaps entries its own `read_dir` enumeration found, which by construction exist at
/// reap time. This test reproduces spec 89's own cited incident (spec 80: a mutant test
/// binary looped for eight days after `cargo-mutants` removed its tree out from under it)
/// through the REAL per-spawn reclaim: the registered mutation-scratch dir is deleted out
/// from under a still-running process BEFORE `rigger result` ever runs, mirroring `cargo-
/// mutants`' own cleanup racing the courier that reports the spawn's outcome.
#[test]
fn rigger_result_reaps_a_live_process_whose_registered_mutation_scratch_dir_was_already_removed_before_the_call(
) {
    let dir = temp_project();
    let root = dir.path();
    seed_store(root);
    seed_run_started(root, "r1");

    let spawn_id = "u-periphery-cli-gone-mutation-scratch/implementer#0";
    let cache_home = tempfile::tempdir().unwrap();
    // The run's own agent-scratch ROOT already exists (as it would by the time any real spawn
    // reports - earlier steps have already populated it), so neither half of the same
    // `reclaim_spawn_registered_scratch` call can refuse on an absent AUTHORIZED ROOT of its
    // own (`is_reapable_base` still requires that half to exist, unchanged by this diff) -
    // the only thing missing below is the mutation-scratch LEAF itself, spec 89 criterion 3's
    // own scope. The agent-scratch root is the cache-home-relocated default (spec 89 criterion
    // 2), never the pre-relocation `<repo>/.rigger/tmp` - mirroring this file's own
    // `rigger_result_reaps_a_live_process_in_the_spawns_registered_agent_scratch_dir` above.
    let agent_scratch_root = rigger::worktree::cache_scratch_root_from(
        root.to_str().unwrap(),
        Some(cache_home.path().as_os_str().to_owned()),
        None,
    )
    .expect("a non-empty repo with an explicit cache home always resolves");
    std::fs::create_dir_all(&agent_scratch_root).unwrap();
    let leaf = mutation_scratch_path(cache_home.path(), spawn_id)
        .expect("a well-formed spawn id must encode to a real path");
    std::fs::create_dir_all(&leaf).unwrap();

    let mut child = sigterm_ignorer_in(&leaf);
    assert!(
        wait_until(|| processes_rooted_under(&leaf)
            .iter()
            .any(|(pid, _)| *pid == child.id())),
        "precondition: the fixture process must actually be rooted in the spawn's registered \
         mutation-scratch dir before it is removed out from under it"
    );

    // `cargo-mutants`' own cleanup (or any other reason the dir might already be gone) removes
    // the LEAF itself, but not the registered ROOT (`cache_home/rigger-mutants`) other spawns'
    // leaves still live under - the child process keeps running, now holding a deleted cwd.
    std::fs::remove_dir_all(&leaf).expect("remove the leaf out from under the live process");

    let (out, err, ok) = run_rigger_envs(
        root,
        &["result", spawn_id, "done"],
        &[("XDG_CACHE_HOME", cache_home.path().to_str().unwrap())],
    );
    assert!(
        ok,
        "recording the result must succeed even though its own mutation-scratch dir is \
         already gone; stdout: {out:?} stderr: {err}"
    );
    assert!(
        !err.contains(REAP_REFUSED_TEXT),
        "spec 89 criterion 3: a base that resolves strictly under the registered mutation-\
         scratch root but no longer exists is ALREADY RECLAIMED, never a logged refusal - got \
         a refusal on stderr: {err}"
    );

    let died = wait_until(|| matches!(child.try_wait(), Ok(Some(_))));
    if !died {
        cleanup(&mut child);
    }
    assert!(
        died,
        "spec 89 criterion 3 / spec 80's 8-day-hang incident, reproduced through the real \
         per-spawn `cmd_result` reclaim chain: a process still rooted in a registered \
         mutation-scratch dir that was REMOVED out from under it before `rigger result` ran \
         must still be found (via the kernel's \" (deleted)\" cwd suffix) and SIGKILLed, not \
         silently left running forever because the now-gone base was refused as \"not \
         strictly under\" its root."
    );
}

/// Sibling of the test above, proving spec 89 criterion 3's OTHER named production instance:
/// a role that never runs `cargo mutants` at all (any reviewer - lens, adversary,
/// adjudicator, sdet-author) reports through the identical `cmd_result` reclaim chain on
/// EVERY round, and its own mutation-scratch leaf was never created in the first place, not
/// merely removed after the fact. Before this fix `is_reapable_base` required `base_dir.
/// canonicalize()` to succeed, so a role that never populated its leaf refused - LOGGED - on
/// every single `rigger result` (`adj-u91c4-reclaim-refusal-corroborates-orphan-finding`,
/// spec 89's own Problem statement: "every reviewer re-reproduces and rules that out every
/// round"). `tests/cli.rs::a_reviewers_result_never_reclaims_the_implementers_mutation_
/// scratch` already proves the SIBLING implementer leaf survives untouched, but asserts
/// nothing about the reporting reviewer's OWN (never-created) leaf or about stderr - it
/// cannot see the noise this fix silences.
#[test]
fn rigger_result_logs_no_false_refusal_for_a_reviewers_own_never_created_mutation_scratch_dir() {
    let dir = temp_project();
    let root = dir.path();
    seed_store(root);
    seed_run_started(root, "r1");

    // The registered mutation-scratch ROOT already exists (some other spawn's leaf populated
    // it earlier in the run - the everyday shape), but THIS reviewer spawn's own leaf never
    // was and never will be: reviewers never run `cargo mutants`.
    let cache_home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(cache_home.path().join("rigger-mutants")).unwrap();
    // The agent-scratch ROOT also already exists, the cache-home-relocated default (spec 89
    // criterion 2), never the pre-relocation `<repo>/.rigger/tmp` - see the sibling test above
    // for the identical rationale.
    let agent_scratch_root = rigger::worktree::cache_scratch_root_from(
        root.to_str().unwrap(),
        Some(cache_home.path().as_os_str().to_owned()),
        None,
    )
    .expect("a non-empty repo with an explicit cache home always resolves");
    std::fs::create_dir_all(&agent_scratch_root).unwrap();

    let spawn_id = "u-periphery-cli-reviewer-never-created-mutation-scratch/adversary#0";
    let leaf = mutation_scratch_path(cache_home.path(), spawn_id)
        .expect("a well-formed spawn id must encode to a real path");
    assert!(
        !leaf.exists(),
        "fixture bug: this test requires the reviewer's own mutation-scratch leaf to never \
         have been created"
    );

    let (out, err, ok) = run_rigger_envs(
        root,
        &["result", spawn_id, "no blocking findings"],
        &[("XDG_CACHE_HOME", cache_home.path().to_str().unwrap())],
    );
    assert!(
        ok,
        "recording a reviewer's result must succeed; stdout: {out:?} stderr: {err}"
    );
    assert!(
        !err.contains(REAP_REFUSED_TEXT),
        "spec 89 criterion 3: a reviewer role's own mutation-scratch leaf, never created \
         because reviewers never run `cargo mutants`, resolves strictly under the registered \
         root and must be treated as ALREADY RECLAIMED - never a logged refusal on every \
         single `rigger result`; got: {err}"
    );
}
