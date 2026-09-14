//! Periphery (cross-module, real-binary) tests for spec 83, criterion 1 (THE FENCE):
//! `src/worktree.rs::spawn_fence`/`SpawnFence`, and `sweep_terminal`'s new `events` parameter
//! that consults them before reclaiming an already-merged, ledger-terminal unit's worktree.
//!
//! WHAT THE INSIDE-OUT TESTS ARE STRUCTURALLY BLIND TO.
//!
//! The implementer's own `mod tests` (`src/worktree.rs`) proves `spawn_fence`'s full decision
//! matrix (NoSpawn/InFlight/Terminal/hung, latest-across-roles, run-scoping, a same-id decoy
//! event) and drives `sweep_terminal`/`sweep_terminal_logged` directly with real git worktrees
//! and a DI-injected log sink - all from INSIDE the crate, in ONE Rust test process, never
//! crossing an actual OS process boundary and never through the crate's own public surface as
//! an external consumer would reach it. `src/main.rs`'s own `mod tests` separately proves
//! `current_run_units` folds the identical fence into `live_branches`/`dead_slugs` - again by
//! calling a crate-private function directly, never through `cmd_step` as a real subprocess.
//!
//! Neither layer can see: (1) whether `sweep_terminal`'s PRODUCTION instance - the one wired to
//! a real `eprintln!`, not the `sweep_terminal_logged` DI seam the implementer's own evidence
//! tests observe through an injected closure - actually reaches real stderr across a real
//! process boundary the way Done-when criterion 1 promises ("attributable from the log"); (2)
//! whether `main.rs`'s two fence call sites (`current_run_units`'s own `live_branches` fold and
//! `sweep_terminal`'s independent internal `spawn_fence` check - deliberately a "second,
//! INDEPENDENT read of the same stream" per that call site's own doc comment) actually compose
//! correctly through the real `cmd_step` wiring rather than merely each working in isolation;
//! and (3) whether `sweep_terminal`'s documented contract - "`events` should already be scoped
//! to the run the caller cares about" - is actually LOAD-BEARING: that an unscoped stream would
//! let a DEAD, prior run's abandoned (never-answered) spawn request for a reused unit slug
//! fence a current-run sweep from reclaiming a worktree that has nothing to do with any live
//! unit, forever. The implementer's own scoping test (`spawn_fence_scoped_out_of_a_prior_run_
//! never_sees_its_resolved_spawn`) uses a prior run's ALREADY-RESOLVED spawn, which reads
//! Terminal either way (scoped or not) and so cannot show scoping changing the actual sweep
//! outcome - only the evidence text's attribution.
//!
//! `step_worktree_sweep_discriminates_in_flight_hung_and_terminal_spawns_across_real_process_
//! boundaries` below is exactly the "crosses a real process boundary" test (1) describes. It was
//! ORIGINALLY RED (round 1) - not from a test defect, but because it found a genuine gap the
//! crate-internal layers above were structurally unable to see: `sweep_terminal` and
//! `current_run_units` both correctly consulted THE FENCE, but `conductor.rs::gc_integrated_
//! branches` - a THIRD worktree-reclaim call site, run unconditionally at the top of every
//! `conductor::run` - looped every unit whose ledger read `Integrated` and reclaimed its worktree
//! and branch with no `spawn_fence` check at all. A unit this test's OWN two fence-respecting
//! layers correctly protected from `sweep_terminal`'s crash-recovery sweep still lost its
//! worktree moments later, in the SAME `rigger step`, to this unpatched third path - the exact
//! `u81c1` shape spec 83 exists to close, surviving through a seam the implementer's round-1 fix
//! never touched. Recorded as decision `sdet-u83c1-gc-integrated-branches-bypasses-fence` (which
//! also flagged `run_stage`'s own fresh-half `w.remove()` teardown as a structurally identical,
//! separately-tracked, unconfirmed sibling gap - out of THIS diff's scope; see that decision for
//! its own status).
//!
//! Round 2 closed the gap: `gc_integrated_branches` now consults `worktree::spawn_fence` over
//! the same current-run-scoped `events` slice `run()` already resolves, mirroring `sweep_
//! terminal`'s own consultation, and (per spec 83's Design text, "each sweep decision is
//! attributable from the log with its evidence") gained its own DI-split logging seam
//! (`gc_integrated_branches_logged`) exactly like `sweep_terminal`/`sweep_terminal_logged`'s
//! precedent. The test below went GREEN, and its assertions were extended (not merely left
//! passing) to close the SAME class of gap (1) describes for this new production instance too:
//! the crate-internal `gc_integrated_branches_logged_prints_kept_evidence_for_an_in_flight_
//! straggler_spawn` / `..._prints_removing_evidence_for_a_terminal_spawns_decision` tests drive
//! `gc_integrated_branches_logged` directly through its DI-injected closure, bypassing the
//! production `eprintln!` wrapper (`gc_integrated_branches`) and the real `run()` call site
//! entirely - they cannot see whether that PRODUCTION wrapper is actually the one wired into
//! `run()`, or whether its evidence reaches real stderr across a real process boundary, the way
//! Done-when criterion 1 promises. The `branch-gc`-prefixed assertion added to this test's step
//! 2 kept arm (for `fenced`) closes that: `branch-gc` names no string anywhere in the pre-
//! round-2 tree (`git show 49cd8a3:src/conductor.rs | grep -c branch-gc` returns 0), so it could
//! only pass against the round-2 fix, and its distinct prefix (vs. `sweep_terminal`'s "worktree
//! sweep") means it can only be satisfied by `gc_integrated_branches_logged`'s own production
//! call, never by `sweep_terminal`'s coincidentally-overlapping evidence text for the same unit.
//!
//! Round 2 shipped its OWN, second defect in that new evidence-logging text (never in THE FENCE
//! mechanism itself, which every round confirmed correct): `gc_integrated_branches_logged`
//! gated the "removing" line purely on ledger status plus the fence's (purely event-derived)
//! liveness state, never on the branch/worktree's actual PHYSICAL presence - unlike `sweep_
//! terminal_logged`, which only ever iterates `git worktree list --porcelain`'s live output.
//! Because `rs.units` is a ledger-projected, monotonic, never-shrinking `Integrated` set folded
//! fresh on every `conductor::run`, this re-printed a false "removing branch" claim for every
//! already-integrated unit with any recorded spawn on every future step, forever - and, sharper
//! still, double-logged a single real vanish within the very FIRST triggering step too: this
//! file's own `hung` arm required (in round 2) a SECOND "branch-gc: removing" line duplicating
//! `sweep_terminal`'s own line moments earlier in the SAME step (`main.rs::cmd_step` runs `sweep_
//! terminal` BEFORE `conductor::run`). Round 3 gates the "removing" log on `worktree::
//! registered_worktree_for` still finding the worktree registered (mirroring `sweep_terminal_
//! logged`'s own live-git-state candidate set) - so the `hung` arm below now asserts the
//! ABSENCE of that duplicate line instead of requiring it, while still confirming the orphaned
//! branch ref is silently reclaimed underneath (only the duplicate LOG line is suppressed, never
//! the underlying best-effort cleanup).

mod common;

use std::path::Path;
use std::process::Command;

/// A throwaway git project with a real commit, so `rigger step`'s run-branch anchoring (a base
/// ref like `HEAD` must resolve) works. Mirrors `tests/cli.rs`'s identical helper.
fn temp_git_project_with_commit() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let _ = Command::new("git")
        .args(["init", "-q"])
        .current_dir(root)
        .status();
    for args in [
        &["config", "user.email", "t@example.com"][..],
        &["config", "user.name", "t"],
        &["commit", "--allow-empty", "-q", "-m", "init"],
    ] {
        let ok = Command::new("git")
            .args(args)
            .current_dir(root)
            .status()
            .expect("git must be runnable")
            .success();
        assert!(ok, "git {args:?} must succeed while seeding the repo");
    }
    dir
}

/// A bare (no commit-required) git repo for the pure library-boundary test: `git init` plus
/// identity config and one empty commit, so `HEAD` resolves for `Worktree::create`'s
/// branch-from-HEAD path.
fn init_repo(path: &Path) {
    for args in [
        &["init", "-q"][..],
        &["config", "user.email", "t@example.com"],
        &["config", "user.name", "t"],
        &["commit", "--allow-empty", "-q", "-m", "init"],
    ] {
        assert!(Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .status()
            .unwrap()
            .success());
    }
}

/// Run `git <args...>` in `cwd` and assert it succeeds.
fn git_ok(cwd: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .status()
        .expect("git must be runnable")
        .success();
    assert!(ok, "git {args:?} must succeed");
}

/// Run a read-only `git <args...>` in `cwd`, returning its trimmed stdout on success.
fn git_out(cwd: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("git must be runnable");
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

/// The project identity the binary resolves for `root` - mirrors `tests/cli.rs`'s identical
/// `run_stream_identity` helper (a repo with no `.rigger/project.id` falls through to the git
/// toplevel's own basename).
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

/// Append raw events built through the crate's PUBLIC `rigger::spawn`/`rigger::eventstore` API
/// (never a hand-typed JSON guess at the wire shape) directly into the same real on-disk store
/// `rigger step` itself reads - standing in for a review-tier dispatch a real multi-role panel
/// would have recorded, without needing to drive one through real (LLM) agents to construct the
/// spec-83 (`u81c1`) shape: a unit whose ledger already reads terminal, with a straggler spawn
/// for the SAME unit still unanswered.
fn seed_events(root: &Path, events: Vec<rigger::eventstore::Event>) {
    use rigger::eventstore::namespace::Namespaced;
    use rigger::eventstore::sqlite::Store;
    use rigger::eventstore::{EventStore, ExpectedRevision};

    let rigger_dir = root.join(".rigger");
    std::fs::create_dir_all(&rigger_dir).unwrap();
    let backend = Store::open(rigger_dir.join("events.db").to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    for event in events {
        store
            .append(rigger::conductor::STREAM, ExpectedRevision::Any, &[event])
            .unwrap();
    }
}

/// Run `rigger <args...>` in `cwd`, returning (stdout, stderr, success) - mirrors
/// `tests/cli.rs`'s identical `run_rigger` helper (opts out of the auto-started dashboard and
/// the machine-global instance registry, exactly as every other periphery suite that spawns the
/// product does).
fn run_rigger(cwd: &Path, args: &[&str]) -> (String, String, bool) {
    let mut cmd = common::rigger_courier();
    cmd.args(args).current_dir(cwd);
    cmd.env("RIGGER_NO_DASH", "1");
    let state = tempfile::tempdir().expect("create a temp XDG_STATE_HOME for the rigger run");
    cmd.env("XDG_STATE_HOME", state.path());
    let out = cmd.output().expect("failed to spawn the rigger binary");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

/// A single reviewless git-backed unit stage - mirrors `tests/cli.rs`'s identical
/// `write_reviewless_git_unit_workflow`. Its only purpose here is to give `rigger step` a
/// real workflow to bootstrap a run (and the `rigger-run` branch) against; the units this file
/// actually tests (`fenced`, `hung`) are manufactured directly as foreign worktrees/events,
/// exactly as `tests/cli.rs`'s `step_start_sweep_spares_a_live_units_empty_diff_worktree_but_
/// reclaims_a_dead_ancestor_leftover` already does for its own "leftover-orphan" branch.
fn write_reviewless_git_unit_workflow(root: &Path) {
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(rigger.join("agents")).unwrap();
    std::fs::write(
        rigger.join("agents").join("worker.md"),
        "---\nid: worker\nmodel: sonnet\ntools: [Read, Edit]\n---\nDo the unit.\n",
    )
    .unwrap();
    std::fs::write(
        rigger.join("workflow.yml"),
        r#"name: fencetest
defaults:
  grounder: nop
  budget: 60
gates:
  ok: { run: "true", kind: core }
stages:
  solo:
    agent: worker
    gates: [ok]
    on_pass: merge
"#,
    )
    .unwrap();
}

/// Spec 83, criterion 1 (THE FENCE), driven at the real binary boundary across THREE separate
/// `rigger step` processes against one persisted on-disk store and real git worktrees - the
/// exact `u81c1` shape: a unit whose LEDGER already reads terminal (a real `UnitStarted` then
/// `UnitIntegrated`, exactly as a genuinely-merged unit's history reads) while a straggler
/// spawn for that SAME unit is still outstanding. Both `fenced` and `hung` below carry that
/// full, realistic ledger history - deliberately NOT the "foreign, untracked branch" shape
/// `tests/cli.rs`'s own "leftover-orphan" convention uses, because an untracked branch is
/// `reclaim_orphan_scratch`'s (spec 34) territory, a SEPARATE backstop with no knowledge of
/// `spawn_fence` at all: a unit absent from the ledger entirely is never `live_branches`-
/// protected by anything spec 83 added, and an early version of this test that omitted the
/// ledger events confirmed exactly that (the orphan backstop deleted the fenced worktree a
/// step after `sweep_terminal` had correctly spared it). A LEDGER-TRACKED unit is what spec
/// 83's Design section actually targets, and it is also what makes the fence's benefit reach
/// BOTH downstream consumers for free: `current_run_units`'s own fence keeps a fenced unit's
/// branch OUT of `dead_slugs` and IN `live_branches`, and `reclaim_orphan_scratch` inherits
/// that protection by construction (it only ever reads `live_branches`/`dead_slugs`, never
/// `spawn_fence` directly) - proving the fence's reach past `sweep_terminal` alone, which no
/// existing test (implementer's or this file's own library-boundary test below) exercises.
///
/// Two such units co-occur in the SAME sweep decision, so one discriminating pass must both
/// spare and reclaim correctly rather than merely "do the right thing when it's the only
/// candidate":
/// - `fenced`: integrated, then a straggler spawn requested and never answered - the in-flight
///   arm. Because `current_run_units`'s OWN fence already keeps its branch in `live_branches`,
///   `sweep_terminal`'s pre-existing `live_branches` check spares it BEFORE its own internal
///   `spawn_fence` re-check is ever reached - the two independent layers agreeing (by
///   construction, since both fold the identical scoped stream) rather than the second layer
///   visibly firing; the library-boundary test below is where the second layer's OWN "kept"
///   evidence line is driven directly, the one shape (an untracked branch) that can reach it.
/// - `hung`: integrated, then a straggler spawn answered with a liveness-fault result
///   (`rigger step`'s own marker-staleness classification shape, spec 10 unit 3) - the "hung
///   past max_wall_clock" terminal arm, reported through the REAL `rigger result --meta`
///   courier command, never a raw seeded result, so the CLI's own meta-JSON parsing is
///   exercised too. Once terminal, it drops OUT of `live_branches` into `dead_slugs`, so it
///   DOES reach `sweep_terminal`'s own ancestor-merge + internal-fence check, and that
///   REMOVAL evidence line is exactly what this test can - and does - observe on real stderr.
#[test]
fn step_worktree_sweep_discriminates_in_flight_hung_and_terminal_spawns_across_real_process_boundaries(
) {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_reviewless_git_unit_workflow(root);

    // Step 1: bootstraps the store and the `rigger-run` branch, and parks the workflow's own
    // "solo" implementer (unrelated to the two foreign units this test actually probes).
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "step 1 must succeed; stderr: {err}");
    assert!(
        out.contains(r#""id":"solo/implementer#0""#),
        "premise: step 1 must park the workflow's own implementer; got: {out:?}"
    );

    let scratch = root.join(".rigger").join("tmp");
    assert!(
        scratch.join("rigger-wt-solo").exists(),
        "premise: the default scratch root is `.rigger/tmp` (load-bearing for the worktree \
         paths built below)"
    );

    // Two foreign worktrees, both trivially merged into `rigger-run` (branched directly off
    // its current tip, exactly like the "leftover-orphan" precedent) - by the PRE-spec-83
    // ancestry-only rule, BOTH would already be reclaim candidates.
    let fenced_dir = scratch.join("rigger-wt-fenced");
    git_ok(
        root,
        &[
            "worktree",
            "add",
            fenced_dir.to_str().unwrap(),
            "-b",
            "rigger/u/fenced",
            "rigger-run",
        ],
    );
    let fenced_canary = fenced_dir.join("canary.txt");
    std::fs::write(&fenced_canary, "spared\n").unwrap();

    let hung_dir = scratch.join("rigger-wt-hung");
    git_ok(
        root,
        &[
            "worktree",
            "add",
            hung_dir.to_str().unwrap(),
            "-b",
            "rigger/u/hung",
            "rigger-run",
        ],
    );

    // Seed each unit's real ledger history (started, then integrated - genuinely terminal,
    // exactly as a merged unit's log reads) followed by a STRAGGLER spawn requested AFTER that
    // integration - the u81c1 shape - standing in for a review-tier dispatch a real multi-role
    // panel would have recorded. The `hung` straggler is then ANSWERED through the real
    // `rigger result` courier below, before the sweep that must discriminate between them ever
    // runs; the `fenced` one is left unanswered.
    seed_events(
        root,
        vec![
            rigger::eventstore::Event::new(
                rigger::ledger::TYPE_UNIT_STARTED,
                br#"{"id":"fenced","branch":"rigger/u/fenced"}"#.to_vec(),
            ),
            rigger::eventstore::Event::new(
                rigger::ledger::TYPE_UNIT_INTEGRATED,
                br#"{"id":"fenced","commit":"deadbeef"}"#.to_vec(),
            ),
            rigger::spawn::SpawnRequest::new("fenced", "fenced", "adversary", 0, "verify")
                .to_event()
                .unwrap(),
            rigger::eventstore::Event::new(
                rigger::ledger::TYPE_UNIT_STARTED,
                br#"{"id":"hung","branch":"rigger/u/hung"}"#.to_vec(),
            ),
            rigger::eventstore::Event::new(
                rigger::ledger::TYPE_UNIT_INTEGRATED,
                br#"{"id":"hung","commit":"deadbeef"}"#.to_vec(),
            ),
            rigger::spawn::SpawnRequest::new("hung", "hung", "implementer", 0, "task")
                .to_event()
                .unwrap(),
        ],
    );
    let (_o, err, ok) = run_rigger(
        root,
        &[
            "result",
            "hung/implementer#0",
            "stale marker past max_wall_clock",
            "--error",
            "--meta",
            r#"{"liveness_class":"infra"}"#,
        ],
    );
    assert!(
        ok,
        "recording the hung spawn's liveness-fault result must succeed; stderr: {err}"
    );

    // Step 2: the step-start sweep is the ONLY thing under test here. `fenced` has no result at
    // all; `hung` has a liveness-fault result. Both are candidates by ancestry alone.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "step 2 must succeed; stderr: {err}");
    // step 2 still finds the same outstanding "solo" implementer, so the run has not advanced -
    // this step's own sweep decision, not a side effect of the roster changing, is what the
    // rest of this test measures.
    assert!(
        out.contains(r#""id":"solo/implementer#0""#),
        "premise: step 2 must still be waiting on the same outstanding roster spawn, or this \
         test measures nothing about its own sweep; got: {out:?}"
    );

    assert!(
        fenced_canary.exists(),
        "an in-flight latest spawn must fence `fenced`'s worktree off the reclaim entirely - \
         the canary is gone, so it was removed (or removed-then-silently-rebuilt) despite the \
         straggler spawn still being unanswered; stderr:\n{err}\n\n\
         DIAGNOSIS (recorded as decision `sdet-u83c1-gc-integrated-branches-bypasses-fence`): \
         `sweep_terminal` correctly spares `fenced` here (it never even appears in EITHER \
         removal-evidence line above), but `conductor.rs::gc_integrated_branches` - called \
         unconditionally at the top of every `conductor::run` (src/conductor.rs:1657) - loops \
         `for u in rs.units.values()` and reclaims ANY unit whose ledger `status == Integrated` \
         with `worktree::reclaim_worktree_on_branch` + `Worktree::delete_branch`, with NO \
         `spawn_fence` check at all and no log evidence of its own. THE FENCE (spec 83, \
         criterion 1) closed this gap in `sweep_terminal` and `current_run_units`, but this \
         THIRD, equally-reachable worktree-reclaim call site was never updated to consult it - \
         the exact `u81c1` shape (a unit reading terminal while a straggler spawn for the same \
         unit is still outstanding) survives through this seam instead."
    );
    // `fenced` is spared via `current_run_units`'s OWN fence (its branch lands in
    // `live_branches`, which `sweep_terminal`'s pre-existing check short-circuits on before its
    // internal `spawn_fence` re-check is ever reached, and which `reclaim_orphan_scratch` reads
    // directly too) - silently, exactly like every other still-live unit's worktree, never
    // spec 83, criterion 1's specific "removed" evidence. Assert the NEGATIVE directly: no
    // "removing" line on real stderr may name `fenced`'s branch.
    assert!(
        !err.lines()
            .any(|l| l.contains("removing") && l.contains("rigger/u/fenced")),
        "`fenced` must not appear in any REMOVAL evidence on real stderr - it is still live; \
         stderr:\n{err}"
    );
    // Round 2 (`gc_integrated_branches` consulting THE FENCE): this reclaim authority is a
    // SEPARATE loop from `sweep_terminal` above, reached only via the real `conductor::run`
    // this step's `cmd_step` drives (never `sweep_terminal`'s own call site) - it is the ONLY
    // authority `fenced` actually exercises here, since `sweep_terminal` never even considers
    // it (short-circuited on `live_branches` before its own internal fence re-check). The
    // crate-internal `gc_integrated_branches_logged_prints_kept_evidence_for_an_in_flight_
    // straggler_spawn` test proves this text through the DI-injected closure, bypassing the
    // production `eprintln!` wrapper entirely; this is the real-process confirmation that the
    // PRODUCTION instance - the one actually wired into `run()` - reaches real stderr with its
    // OWN "branch-gc" evidence line (distinct from `sweep_terminal`'s "worktree sweep" prefix,
    // so this can only be satisfied by `gc_integrated_branches_logged`'s own log call, never by
    // `sweep_terminal`'s coincidentally-overlapping text for a different unit).
    assert!(
        err.contains("branch-gc")
            && err.contains("kept branch")
            && err.contains(r#""rigger/u/fenced""#)
            && err.contains("in flight"),
        "gc_integrated_branches's OWN kept decision for `fenced` must be attributable from \
         real stderr, not only from the DI-injected closure the crate-internal test observes: \
         {err}"
    );

    assert!(
        !hung_dir.exists(),
        "a latest spawn already classified hung (past max_wall_clock) must not block the \
         reclaim; stderr:\n{err}"
    );
    assert!(
        err.contains("removing") && err.contains(r#""hung""#) && err.contains("hung past"),
        "the REMOVED decision for `hung` must name it as hung, not merely terminal, on real \
         stderr: {err}"
    );
    // Round 3 fix for `sdet-u83c1r2-removing-evidence-repeats-forever-after-real-removal`
    // (UPHELD in ADJUDICATION u83c1 round 2, cause genuine-defect): `hung` is ALSO
    // ledger-`Integrated`, so `gc_integrated_branches` reaches it too - but `sweep_
    // terminal` has ALREADY reclaimed its worktree by this point (both run in the SAME
    // step, `sweep_terminal` first, per `main.rs::cmd_step`). The round-2 diff this
    // replaces required a SECOND "branch-gc: removing" line here, duplicating `sweep_
    // terminal`'s own evidence above for the IDENTICAL vanish - exactly the double-
    // attribution the adjudication rejected. The fix gates `gc_integrated_branches`'s own
    // "removing" evidence on the worktree still being PHYSICALLY REGISTERED (mirroring
    // `sweep_terminal_logged`'s own `git worktree list --porcelain`-driven candidate
    // set), so it now stays silent here instead of re-reporting a vanish another
    // authority already logged moments earlier in the same step.
    assert!(
        !err.lines().any(|l| l.contains("branch-gc")
            && l.contains("removing branch")
            && l.contains(r#""rigger/u/hung""#)),
        "gc_integrated_branches must NOT re-report a vanish `sweep_terminal` already logged \
         in the SAME step, on real stderr: {err}"
    );
    // The branch REF itself - never touched by `sweep_terminal`, which only ever
    // deregisters the worktree - is still silently reclaimed by `gc_integrated_branches`'s
    // own best-effort `Worktree::delete_branch` call underneath; only the duplicate LOG
    // line is suppressed, never the underlying cleanup.
    assert!(
        git_out(
            root,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                "refs/heads/rigger/u/hung"
            ]
        )
        .is_none(),
        "the hung unit's now-orphaned branch ref must still be reclaimed (silently), even \
         though its removal is no longer independently logged"
    );
    let list = git_out(root, &["worktree", "list", "--porcelain"]).unwrap_or_default();
    assert!(
        !list.contains("rigger/u/hung"),
        "the reclaimed `hung` worktree must be fully DEREGISTERED from git: {list}"
    );
    assert!(
        list.contains("rigger/u/fenced"),
        "the spared `fenced` worktree must still be registered with git: {list}"
    );

    // The straggler for `fenced` now answers, through the real courier, exactly as a review
    // lens finishing late would.
    let (_o, err, ok) = run_rigger(root, &["result", "fenced/adversary#0", "approve"]);
    assert!(
        ok,
        "recording the fenced spawn's real result must succeed; stderr: {err}"
    );

    // Step 3: with its latest spawn now terminal, the fence's own internal check no longer
    // blocks the pre-existing removal - `fenced` must be reclaimed exactly as `hung` was.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "step 3 must succeed; stderr: {err}");
    assert!(
        !fenced_dir.exists(),
        "`fenced`'s worktree must be reclaimed once its latest spawn has a real result; got \
         stdout: {out:?}\nstderr:\n{err}"
    );
    assert!(
        err.contains("removing") && err.contains(r#""fenced""#) && err.contains("terminal"),
        "the REMOVED decision for the now-terminal `fenced` must also be attributable from \
         real stderr: {err}"
    );
}

/// Spec 83, criterion 1 (THE FENCE), round 3's OWN positive real-boundary proof.
///
/// Round 3's fix (gating `gc_integrated_branches_logged`'s "removing" evidence on `worktree::
/// registered_worktree_for` still finding the worktree registered) necessarily REPLACED the
/// only real-process assertion that had ever proven that evidence line reaches real stderr at
/// all: round 2's `hung` arm above asserted `branch-gc` + `removing branch` + `hung past` on
/// real stderr, and round 3 changed that SAME assertion to its negation (the line must be
/// ABSENT), because `sweep_terminal` - which always runs first, per `main.rs::cmd_step` - beats
/// `gc_integrated_branches` to every worktree still registered under the scratch root once its
/// branch is git-ancestry-merged into `rigger-run` (both consult the identical `spawn_fence`).
/// The crate-internal `gc_integrated_branches_logged_prints_removing_evidence_for_a_terminal_
/// spawns_decision` test proves the evidence text through the DI-injected closure, but - the
/// same class of gap this file's own module doc names for `sweep_terminal`'s identical
/// wrapper/DI split - it cannot see whether the PRODUCTION `eprintln!` wrapper actually wired
/// into `run()` reaches real stderr across a real process boundary the way Done-when criterion
/// 1 promises. After round 3's edit, no periphery test proved that positive arm at all.
///
/// `settled`'s worktree is deliberately created OUTSIDE the scratch root (`.rigger/tmp`) that
/// `cmd_step`'s OTHER two backstops - `sweep_terminal` (this file's module doc, `fenced`/`hung`
/// above) and the spec-34 orphan sweep (`reclaim_orphan_scratch`, which independently scans
/// `.rigger/tmp` for any `rigger-wt-<slug>` directory no live unit owns, entirely UNGATED by
/// git ancestry) - both scan before `conductor::run` ever starts. Placing `fenced`/`hung`'s own
/// convention (a worktree directly under the scratch root) here would let `reclaim_orphan_
/// scratch` reclaim `settled`'s directory first (confirmed empirically: an earlier draft of
/// this test, with `settled` under the scratch root and a divergent, never-merged commit to
/// dodge ONLY `sweep_terminal`'s ancestry check, still failed - real stderr read `rigger step:
/// reclaimed 1 orphaned scratch entry`, `reclaim_orphan_scratch`'s own evidence text, not `gc_
/// integrated_branches`'s), silently defeating this test's premise the way `sweep_terminal`'s
/// own ancestry-only rule would. `registered_worktree_for` (`gc_integrated_branches`'s own
/// presence check) parses `git worktree list --porcelain` GLOBALLY, with no location or naming
/// restriction at all - unlike both other backstops, it does not care where the worktree lives.
/// This isolates `gc_integrated_branches`'s own ledger-driven reclaim from either backstop, so
/// what this test proves is attributable to round 3's fix alone, not an accident of which
/// backstop happened to run first.
#[test]
fn gc_integrated_branches_removing_evidence_reaches_real_stderr_for_a_still_registered_worktree_it_alone_reclaims(
) {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_reviewless_git_unit_workflow(root);

    // Step 1: bootstraps the store and the `rigger-run` branch; unrelated to `settled`, which
    // this test manufactures directly, exactly like `fenced`/`hung` above.
    let (_out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "step 1 must succeed; stderr: {err}");

    let settled_dir = root.join("settled-outside-the-scratch-root");
    git_ok(
        root,
        &[
            "worktree",
            "add",
            settled_dir.to_str().unwrap(),
            "-b",
            "rigger/u/settled",
            "rigger-run",
        ],
    );

    // `settled`'s real ledger history: started, then integrated - as the resume-backstop
    // scenario reads it (the ledger already says `Integrated`, e.g. a crash before the
    // graceful teardown recorded its removal) - with its one spawn answered ORDINARILY (never
    // a liveness fault), so its evidence reads plain "terminal", distinct from `hung`'s "hung
    // past" wording above.
    seed_events(
        root,
        vec![
            rigger::eventstore::Event::new(
                rigger::ledger::TYPE_UNIT_STARTED,
                br#"{"id":"settled","branch":"rigger/u/settled"}"#.to_vec(),
            ),
            rigger::eventstore::Event::new(
                rigger::ledger::TYPE_UNIT_INTEGRATED,
                br#"{"id":"settled","commit":"deadbeef"}"#.to_vec(),
            ),
            rigger::spawn::SpawnRequest::new("settled", "settled", "adversary", 0, "verify")
                .to_event()
                .unwrap(),
            rigger::spawn::SpawnResult::ok("settled/adversary#0", "approve")
                .to_event()
                .unwrap(),
        ],
    );

    // Step 2: neither `sweep_terminal` nor `reclaim_orphan_scratch` ever considers a worktree
    // outside the scratch root; `gc_integrated_branches` then finds `settled` via the LEDGER
    // alone (`registered_worktree_for` scans git's own global worktree list) and is the ONLY
    // authority that ever touches it.
    let (_out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "step 2 must succeed; stderr: {err}");

    assert!(
        !err.lines()
            .any(|l| l.contains("worktree sweep") && l.contains("settled")),
        "premise: `sweep_terminal` must never even consider `settled` (outside the scratch \
         root) - or this test proves nothing about `gc_integrated_branches` acting alone: {err}"
    );
    assert!(
        !err.contains("orphaned scratch"),
        "premise: `reclaim_orphan_scratch` must never touch `settled` (outside the scratch \
         root) either - or this test proves nothing about `gc_integrated_branches` acting \
         alone: {err}"
    );
    assert!(
        !settled_dir.exists(),
        "`gc_integrated_branches` must reclaim `settled`'s still-registered worktree by itself; \
         stderr:\n{err}"
    );
    assert!(
        err.contains("branch-gc")
            && err.contains("removing branch")
            && err.contains(r#""rigger/u/settled""#)
            && err.contains("terminal"),
        "gc_integrated_branches's OWN removing decision, for a worktree it alone reclaims, must \
         be attributable from real stderr - the one positive proof round 3's negated `hung` \
         assertion above no longer gives: {err}"
    );
    assert!(
        git_out(
            root,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                "refs/heads/rigger/u/settled"
            ]
        )
        .is_none(),
        "the branch ref itself must also be reclaimed, not only the worktree"
    );
}

/// Spec 83, criterion 1 (THE FENCE), the run-scoping half at the crate's PUBLIC library
/// boundary - never a subprocess here, but never a crate-internal privilege either
/// (`rigger::conductor`/`rigger::eventstore`/`rigger::run`/`rigger::spawn`/`rigger::worktree`,
/// exactly as an external consumer following `sweep_terminal`'s own documented contract would
/// call it: "`events` should already be scoped to the run the caller cares about").
///
/// The implementer's own `spawn_fence_scoped_out_of_a_prior_run_never_sees_its_resolved_spawn`
/// proves scoping on raw events alone, with no worktree in sight, for a prior run whose spawn
/// is ALREADY RESOLVED - in that shape, an unscoped read still happens to permit the identical
/// reclaim (a resolved spawn reads Terminal either way), so it cannot show scoping changing the
/// sweep's actual OUTCOME, only its evidence text's attribution. This proves the sharper,
/// functionally different failure `sweep_terminal`'s own doc comment warns against but that no
/// existing test drives with a real worktree: a prior run KILLED before any courier ever
/// answered its spawn for a since-abandoned unit slug. Handed the raw, unscoped stream (the
/// exact mistake that doc comment warns a caller against), that dead run's forever-unanswered
/// request reads as the current sweep's "latest" for that slug and PERMANENTLY fences a
/// worktree the current run has nothing to do with - a real leaked-disk-space failure mode,
/// not merely a cosmetic evidence-text difference.
#[test]
fn sweep_terminal_scoped_to_the_current_run_reclaims_a_dead_runs_abandoned_slug_but_an_unscoped_read_fences_it_forever(
) {
    use rigger::conductor::STREAM;
    use rigger::eventstore::sqlite::Store;
    use rigger::eventstore::{Direction, Event, EventStore, ExpectedRevision};
    use rigger::run::{current_run, TYPE_RUN_STARTED};
    use rigger::spawn::SpawnRequest;
    use rigger::worktree::{scratch_root, sweep_terminal, Worktree};

    let repo = tempfile::tempdir().unwrap();
    let repo_path = repo.path().to_str().unwrap().to_string();
    init_repo(repo.path());
    git_ok(repo.path(), &["checkout", "-b", "rigger-run"]);
    let root = scratch_root(&repo_path, "", None);

    let store = Store::open(":memory:").unwrap();
    // Run r1: requests a spawn for "orphan-slug" and then dies - no courier ever reports back
    // (a killed run, never a resolved one).
    store
        .append(
            STREAM,
            ExpectedRevision::Any,
            &[Event::new(
                TYPE_RUN_STARTED,
                br#"{"run":"r1","criteria":["c"]}"#.to_vec(),
            )],
        )
        .unwrap();
    let dead_req = SpawnRequest::new("orphan-slug", "orphan-slug", "implementer", 0, "task");
    store
        .append(
            STREAM,
            ExpectedRevision::Any,
            &[dead_req.to_event().unwrap()],
        )
        .unwrap();
    // Run r2 begins - the CURRENT run - and never mentions "orphan-slug" again; that slug
    // belongs entirely to the dead run r1.
    store
        .append(
            STREAM,
            ExpectedRevision::Any,
            &[Event::new(
                TYPE_RUN_STARTED,
                br#"{"run":"r2","criteria":["c2"]}"#.to_vec(),
            )],
        )
        .unwrap();
    let all_events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    let scoped: Vec<Event> = current_run(&all_events).to_vec();

    let dir = format!("{root}/rigger-wt-orphan");
    Worktree::create(&repo_path, &dir, "rigger/u/orphan-slug", "").unwrap();

    // Handed the DOCUMENTED, correctly-scoped slice: r1's abandoned request must not be
    // mistaken for a current-run concern, so the pre-existing ancestry-only rule reclaims it
    // exactly as it would a unit with no recorded spawn at all.
    let removed = sweep_terminal(
        &repo_path,
        &root,
        "rigger-run",
        &std::collections::HashSet::new(),
        &std::collections::HashSet::new(),
        &scoped,
    )
    .unwrap();
    assert_eq!(
        removed, 1,
        "scoped to the current run, a dead run's abandoned spawn for a foreign slug must not \
         fence the sweep from reclaiming an otherwise-ordinary leftover worktree"
    );
    assert!(!Path::new(&dir).exists());

    // The control: recreate the identical worktree, then hand `sweep_terminal` the RAW,
    // UNSCOPED stream instead - the exact mistake its own doc comment warns a caller against.
    Worktree::create(&repo_path, &dir, "rigger/u/orphan-slug", "").unwrap();
    let removed = sweep_terminal(
        &repo_path,
        &root,
        "rigger-run",
        &std::collections::HashSet::new(),
        &std::collections::HashSet::new(),
        &all_events,
    )
    .unwrap();
    assert_eq!(
        removed, 0,
        "CONTROL: an unscoped read lets the dead run's forever-unanswered request read as the \
         'latest' for this slug and wrongly fence a foreign worktree - proving scoping is \
         load-bearing to the sweep's actual outcome, not merely to its evidence text"
    );
    assert!(
        Path::new(&dir).exists(),
        "CONTROL: the worktree must survive the unscoped call, demonstrating the exact leak \
         `sweep_terminal`'s documented contract exists to prevent"
    );
}
