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

mod common;

use common::cli::run_rigger_envs;
use common::cli::run_stream_identity;
use common::cli::seed_store;
use common::cli::temp_project;
use common::cli::temp_store_project;
use common::fixtures::cleanup;
use common::fixtures::sigterm_ignorer_in;
use common::wait_until;

use rigger::driver::replay::{mutation_scratch_path, spawn_scratch_path};
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
/// home: the reclaim's mutation-scratch half never touches the operator's real ~/.cache, and the
/// fixture and the child `rigger result` process (handed the SAME `XDG_CACHE_HOME`) resolve the
/// identical scratch roots.
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

#[test]
fn rigger_result_reaps_a_live_process_in_the_spawns_registered_mutation_scratch_dir() {
    let project = ReapProject::new();
    let spawn_id = "u-periphery-cli-live-reap-mutation/implementer#0";
    let leaf = mutation_scratch_path(project.cache_home.path(), spawn_id)
        .expect("a well-formed spawn id must encode to a real path");
    let mut child = live_child_in(&leaf, "mutation-scratch", "before `rigger result` runs");

    let (out, err, ok) = project.result(spawn_id, "done");
    assert!(
        ok,
        "recording the result must succeed; stdout: {out:?} stderr: {err}"
    );
    assert_reaped(
        &mut child,
        "`rigger result` must reap a live process still rooted in the spawn's own registered \
         mutation-scratch dir (spec 77 criterion 2 - the EXACT root spec 78 round 1's reject \
         named, adj-u78c2-verdict-reject-reap-authority-conflict) before removing it, through \
         the real reclaim_spawn_registered_scratch call chain (spec 78 round 2 fix, decision \
         u78c2r2-authorized-root-caller-supplied) - a SIGTERM-ignoring process here must still \
         be SIGKILLed. This is a DIFFERENT call chain than reclaim_unit_mutation_scratch \
         (already proven directly in mutation_scratch_reap_base_guard_periphery.rs): this one \
         is keyed on ONE reporting spawn's own id via cmd_result, not a unit-terminal \
         enumeration, and every pre-existing regression test for it (tests/cli.rs) only plants \
         files, never a live process.",
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
    let project = ReapProject::new();
    let spawn_id = "u-periphery-cli-gone-mutation-scratch/implementer#0";
    // The agent-scratch ROOT exists (the everyday shape), so only the mutation-scratch half of
    // the reclaim meets a gone base.
    std::fs::create_dir_all(project.agent_scratch_root()).unwrap();
    let leaf = mutation_scratch_path(project.cache_home.path(), spawn_id)
        .expect("a well-formed spawn id must encode to a real path");
    let mut child = live_child_in(
        &leaf,
        "mutation-scratch",
        "before it is removed out from under it",
    );

    // `cargo-mutants`' own cleanup racing the courier that reports the spawn's outcome.
    std::fs::remove_dir_all(&leaf).expect("remove the leaf out from under the live process");

    let (out, err, ok) = project.result(spawn_id, "done");
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
    assert_reaped(
        &mut child,
        "spec 89 criterion 3 / spec 80's 8-day-hang incident, reproduced through the real \
         per-spawn `cmd_result` reclaim chain: a process still rooted in a registered \
         mutation-scratch dir that was REMOVED out from under it before `rigger result` ran \
         must still be found (via the kernel's \" (deleted)\" cwd suffix) and SIGKILLed, not \
         silently left running forever because the now-gone base was refused as \"not \
         strictly under\" its root.",
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
    let project = ReapProject::new();
    // The registered mutation-scratch ROOT already exists (some other spawn's leaf populated
    // it earlier in the run - the everyday shape), but THIS reviewer spawn's own leaf never
    // was and never will be: reviewers never run `cargo mutants`. The agent-scratch ROOT also
    // already exists.
    std::fs::create_dir_all(project.cache_home.path().join("rigger-mutants")).unwrap();
    std::fs::create_dir_all(project.agent_scratch_root()).unwrap();

    let spawn_id = "u-periphery-cli-reviewer-never-created-mutation-scratch/adversary#0";
    let leaf = mutation_scratch_path(project.cache_home.path(), spawn_id)
        .expect("a well-formed spawn id must encode to a real path");
    assert!(
        !leaf.exists(),
        "fixture bug: this test requires the reviewer's own mutation-scratch leaf to never \
         have been created"
    );

    let (out, err, ok) = project.result(spawn_id, "no blocking findings");
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
