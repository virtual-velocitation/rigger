//! Periphery for spec 91, criterion 1 - the two generic conductor rules: (1) a `needs`
//! entry that names the fan-out implement TEMPLATE is satisfied exactly when every unit
//! the template expanded into has integrated, and stays unsatisfied while any member is
//! open or reached a terminal-but-not-integrated state; (2) a stage may set its own
//! `max_retries`, overriding `defaults.max_retries` for the units it governs.
//!
//! WHY THIS FILE, DISTINCT FROM THE IMPLEMENTER'S OWN TESTS. `src/conductor.rs`'s own
//! `mod tests` already proves both rules:
//!   - `a_stage_needing_the_fan_out_template_becomes_ready_once_every_criterion_unit_integrates`
//!     drives the real `run()` wiring end to end for rule 1's happy path.
//!   - `a_downstream_stage_needing_the_fan_out_template_stays_unready_until_every_member_integrates`
//!     proves rule 1 at the pure `ready_stages`/`wave_ready` level, including a partially-
//!     integrated group and an escalated (terminal, non-integrated) member.
//!   - `a_stages_own_max_retries_overrides_the_run_default_for_its_units` proves rule 2's
//!     precedence arithmetic (`max_retries_for`) directly, including the resume-grant-still-
//!     wins interaction.
//!
//! Every one of those runs in ONE process: a `cfg`/`Deps` built by hand, a `Stub` driver
//! that never parks, an in-memory `Store::open(":memory:")`. None of them closes these
//! periphery gaps:
//!   1. Nothing proves `max_retries:` is actually a STAGE-LEVEL YAML KEY a workflow author
//!      can write and have it parsed by the real deserializer - every implementer fixture
//!      constructs a `Stage { max_retries: N, .. }` literal in Rust, never parsing
//!      `max_retries: N` out of `workflow.yml` text.
//!   2. Nothing proves the override actually changes ESCALATION TIMING through the
//!      compiled binary's real remediation loop (spawn, gate, remediate, re-spawn or
//!      escalate) - the implementer's test calls the private `max_retries_for` directly and
//!      never drives `run_stage`'s loop at all.
//!   3. Nothing proves rule 1 holds across the REAL multi-process shape a fan-out unit
//!      actually takes: the template's baseline units park across separate `rigger step`
//!      invocations, integrate through REAL git worktrees and a REAL merge onto the run
//!      branch (never a `Stub` driver that answers synchronously in one call), and only
//!      then does a downstream `needs: [<template>]` stage become ready - in the SAME
//!      compiled binary a real operator runs.
//!   4. Nothing proves the escalated-member case (rule 1's other half) stops a downstream
//!      stage from ever appearing in a REAL `rigger step`'s printed wave, as opposed to a
//!      pure function returning the right boolean.
//!
//! This file closes all four, through the compiled binary, real git worktrees/merges
//! where the scenario needs them, and real `rigger step`/`rigger result` process
//! boundaries throughout.
//!
//! NOT OWNED HERE: the pure `ready_stages`/`wave_ready`/`max_retries_for` arithmetic
//! itself (private to `conductor.rs`, exhaustively covered by its own colocated tests),
//! and the resume-grant-vs-stage-override interaction (pure arithmetic over already-parsed
//! structs, no new I/O boundary - covered by the implementer's own
//! `a_stages_own_max_retries_overrides_the_run_default_for_its_units`).

mod common;

use std::path::Path;
use std::process::Command;

/// A throwaway project that is its own git repo with one commit, so a base ref like
/// `HEAD` resolves and a real per-unit worktree/branch/merge can land. Mirrors
/// `tests/cli.rs`'s identically-named helper.
fn temp_git_project_with_commit() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create temp project");
    for args in [
        &["init", "-q"][..],
        &["config", "user.email", "t@example.com"],
        &["config", "user.name", "t"],
        &["commit", "--allow-empty", "-q", "-m", "init"],
    ] {
        let ok = Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .status()
            .expect("git must be runnable")
            .success();
        assert!(ok, "git {args:?} must succeed while seeding the repo");
    }
    dir
}

/// A throwaway, repo-less project - enough for the offline (`isolation: none`) escalation
/// scenarios below, which never touch git. Mirrors `tests/cli.rs`'s `temp_repoless_project`.
fn temp_repoless_project() -> tempfile::TempDir {
    tempfile::tempdir().expect("create temp project")
}

/// Append `events` directly to `root`'s namespaced run stream through a real `Store::open`
/// round trip - standing in for the `rigger result` courier's own `SpawnResult` append, so
/// a retry/escalation loop can be driven one attempt at a time without a real agent
/// subprocess. Mirrors `tests/escalation_resume_periphery.rs`'s identically-named helper.
fn seed_run_events(root: &Path, events: &[(&str, &str)]) {
    use rigger::eventstore::namespace::Namespaced;
    use rigger::eventstore::sqlite::Store;
    use rigger::eventstore::{Event, EventStore, ExpectedRevision};

    let rigger_dir = root.join(".rigger");
    std::fs::create_dir_all(&rigger_dir).unwrap();
    let backend = Store::open(rigger_dir.join("events.db").to_str().unwrap()).unwrap();
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

/// The project identity the binary resolves for `root` - mirrors
/// `tests/escalation_resume_periphery.rs`'s identically-named helper, itself mirroring
/// `StoreLocation::identity`'s precedence.
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

/// Run `rigger <args...>` in `cwd` and return (stdout, stderr, success). Mirrors
/// `tests/cli.rs`'s identically-named helper: opts out of the auto-started dashboard and
/// scopes the machine-global instance registry to a throwaway `XDG_STATE_HOME`, so these
/// short-lived invocations never leak a live process or a phantom registry entry.
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

/// A real, isolated (git-worktree-backed) worker agent - no `isolation: none` - so its
/// stage's `on_pass: merge` reaches a genuine git merge onto the run branch. Mirrors
/// `tests/cli.rs`'s `write_reviewless_git_unit_workflow`'s own agent file.
fn write_git_worker_agent(root: &Path) {
    let agents = root.join(".rigger").join("agents");
    std::fs::create_dir_all(&agents).unwrap();
    std::fs::write(
        agents.join("worker.md"),
        "---\nid: worker\nmodel: sonnet\ntools: [Read, Edit]\n---\nDo the unit.\n",
    )
    .unwrap();
}

/// A repo-less (`isolation: none`) worker agent for the offline escalation scenarios,
/// which never need a real worktree. Mirrors `tests/cli.rs`'s
/// `write_failing_gate_escalating_workflow`'s own agent file.
fn write_repoless_worker_agent(root: &Path) {
    let agents = root.join(".rigger").join("agents");
    std::fs::create_dir_all(&agents).unwrap();
    std::fs::write(
        agents.join("worker.md"),
        "---\nid: worker\nmodel: sonnet\ntools: [Read, Edit]\nisolation: none\n---\nDo the unit.\n",
    )
    .unwrap();
}

// -----------------------------------------------------------------------------------------
// Rule 1, gap 3/4 (happy path): a `needs: [implement]` stage becomes ready, through the
// real compiled binary and real git merges, only once EVERY fan-out baseline unit has
// actually integrated - never on the first one alone.
// -----------------------------------------------------------------------------------------

/// Two Done-when criteria decompose into two real baseline units under the `implement`
/// fan-out template (`strategy: fan-out`, `on_pass: merge`, real git isolation). `checkin`
/// names the TEMPLATE ("implement") in its own `needs`. Drives three real `rigger step
/// --spec spec.md` invocations with real `rigger result` posts and real git merges in
/// between, proving: both baseline units park in the SAME first wave; `checkin` is absent
/// from every wave until the SECOND (final) baseline unit integrates; and `checkin`'s own
/// implementer parks in the very step that crosses that threshold.
#[test]
fn checkin_becomes_ready_only_once_both_real_fanout_baseline_units_have_integrated() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_git_worker_agent(root);
    std::fs::write(
        root.join(".rigger").join("workflow.yml"),
        r#"name: fanouttemplateneedstest
defaults:
  grounder: nop
  budget: 60
gates:
  ok: { run: "true", kind: core }
stages:
  implement:
    agent: worker
    strategy: fan-out
    gates: [ok]
    on_pass: merge
  checkin:
    agent: worker
    needs: [implement]
    gates: [ok]
    on_pass: merge
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("spec.md"),
        "# Spec\n\n## Done when\n\n- [ ] alpha lands cleanly\n- [ ] beta lands too\n",
    )
    .unwrap();

    let unit_a = "unit-1-alpha-lands-cleanly";
    let unit_b = "unit-2-beta-lands-too";

    // Step 1: both criteria decompose into baseline units from the SAME template in the
    // same wave (neither needs the other), so both implementers park together; `checkin`
    // needs the template and neither member has integrated yet, so it is absent.
    let (out, err, ok) = run_rigger(root, &["step", "--spec", "spec.md"]);
    assert!(
        ok,
        "the first spec-driven step must succeed; stderr:\n{err}"
    );
    for id in [unit_a, unit_b] {
        assert!(
            out.contains(&format!(r#""id":"{id}/implementer#0""#)),
            "both real fan-out baseline units must park in the first wave; got:\n{out}"
        );
    }
    assert!(
        !out.contains("checkin"),
        "checkin needs the template and NO member has integrated yet, so it must not \
         appear anywhere in the first wave; got:\n{out}"
    );

    // Land unit A for real: write its diff into the REAL worktree the park already
    // created, post its result, then step again so the real merge lands.
    let wt_a = root
        .join(".rigger")
        .join("tmp")
        .join(format!("rigger-wt-{unit_a}"));
    assert!(
        wt_a.exists(),
        "a parked implementer must already have its unit worktree on disk: {}",
        wt_a.display()
    );
    std::fs::write(wt_a.join("a.rs"), "pub fn a() {}\n").unwrap();
    let (_o, err, ok) = run_rigger(
        root,
        &["result", &format!("{unit_a}/implementer#0"), "landed a"],
    );
    assert!(ok, "recording unit A's result must succeed; stderr: {err}");

    // Step 2: unit A's implementer replays and its real merge lands (UnitIntegrated for
    // unit A). Unit B's implementer is still outstanding (parked, no result yet) so
    // nothing new happens for it. `checkin` still needs unit B, so it must still be
    // absent - the partial-fan-out half of rule 1, now proven at the real binary boundary.
    let (out, err, ok) = run_rigger(root, &["step", "--spec", "spec.md"]);
    assert!(ok, "the second step must succeed; stderr:\n{err}");
    assert!(
        !out.contains("checkin"),
        "checkin needs BOTH baseline units and only unit A has integrated so far, so it \
         must still be absent from every wave; got:\n{out}"
    );

    // Land unit B for real, the same way.
    let wt_b = root
        .join(".rigger")
        .join("tmp")
        .join(format!("rigger-wt-{unit_b}"));
    assert!(
        wt_b.exists(),
        "unit B's parked implementer must already have its unit worktree on disk: {}",
        wt_b.display()
    );
    std::fs::write(wt_b.join("b.rs"), "pub fn b() {}\n").unwrap();
    let (_o, err, ok) = run_rigger(
        root,
        &["result", &format!("{unit_b}/implementer#0"), "landed b"],
    );
    assert!(ok, "recording unit B's result must succeed; stderr: {err}");

    // Step 3: unit B's real merge lands (UnitIntegrated for unit B). Both members of the
    // template have now integrated, so `checkin`'s `needs: [implement]` is satisfied and
    // its own implementer must park in this SAME step - the threshold-crossing step.
    let (out, err, ok) = run_rigger(root, &["step", "--spec", "spec.md"]);
    assert!(ok, "the third step must succeed; stderr:\n{err}");
    assert!(
        out.contains(r#""id":"checkin/implementer#0""#),
        "once BOTH real fan-out baseline units have integrated, checkin's needs edge on \
         the template must be satisfied and its own implementer must park in this same \
         step; got:\n{out}"
    );
}

// -----------------------------------------------------------------------------------------
// Rule 1, gap 4: an escalated (terminal, never-integrated) fan-out member must permanently
// block a downstream `needs: [<template>]` stage, through a real `rigger step`'s printed
// wave - never merely a pure function returning the right boolean.
// -----------------------------------------------------------------------------------------

/// One Done-when criterion decomposes into exactly one baseline unit under the `implement`
/// template, whose only gate always fails with a remediation bound of one - it escalates on
/// its very first failed attempt and never integrates. `checkin` names the template in its
/// `needs`. Proves that the run's escalated fixpoint carries the baseline unit's id, never
/// `checkin`'s - `checkin`'s implementer must never appear in any real step's wave at all.
#[test]
fn checkin_never_becomes_ready_when_its_only_fanout_member_escalates_instead_of_integrating() {
    let dir = temp_repoless_project();
    let root = dir.path();
    write_repoless_worker_agent(root);
    std::fs::write(
        root.join(".rigger").join("workflow.yml"),
        r#"name: fanouttemplateescalationtest
defaults:
  grounder: nop
  budget: 60
  max_retries: 1
gates:
  bad: { run: "false", kind: core }
stages:
  implement:
    agent: worker
    strategy: fan-out
    gates: [bad]
    on_pass: none
  checkin:
    agent: worker
    needs: [implement]
    gates: [bad]
    on_pass: none
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("spec.md"),
        "# Spec\n\n## Done when\n\n- [ ] gamma never lands\n",
    )
    .unwrap();

    let unit = "unit-1-gamma-never-lands";

    // Step 1: the sole baseline unit's implementer parks; checkin needs the template and
    // the member has not even attempted a gate yet, so it is absent.
    let (out, err, ok) = run_rigger(root, &["step", "--spec", "spec.md"]);
    assert!(
        ok,
        "the first spec-driven step must succeed; stderr:\n{err}"
    );
    assert!(
        out.contains(&format!(r#""id":"{unit}/implementer#0""#)),
        "the sole fan-out baseline unit must park in the first wave; got:\n{out}"
    );
    assert!(
        !out.contains("checkin"),
        "checkin must not appear before its only member has even attempted a gate; \
         got:\n{out}"
    );

    seed_run_events(
        root,
        &[(
            "SpawnResult",
            &format!(r#"{{"id":"{unit}/implementer#0","output":"attempted"}}"#),
        )],
    );

    // Step 2: the gate fails, the remediation bound of one means this first failure IS
    // the escalation - the baseline unit goes terminal WITHOUT ever integrating.
    // `checkin`'s needs edge names the template and its only member never integrated, so
    // it must never appear - not in this step's wave, and not in the escalated set either.
    let (out, err, ok) = run_rigger(root, &["step", "--spec", "spec.md"]);
    assert!(
        ok,
        "a step that reaches an escalated fixpoint still exits 0; stderr:\n{err}"
    );
    assert!(
        out.contains(&format!(r#""escalated":["{unit}"]"#)),
        "the run must reach a fixpoint with the baseline unit escalated; got:\n{out}"
    );
    assert!(
        !out.contains("checkin"),
        "checkin's only fan-out member escalated instead of integrating, so checkin must \
         never appear anywhere - not spawned, and not itself in the escalated set; \
         got:\n{out}"
    );
}

// -----------------------------------------------------------------------------------------
// Rule 2: a stage's own `max_retries:` YAML key, actually parsed by the real deserializer,
// overrides `defaults.max_retries` for that stage's units through the real remediation
// loop - proven in both directions (lowering and raising the effective bound).
// -----------------------------------------------------------------------------------------

/// `defaults.max_retries: 5` (generous) but the stage itself sets `max_retries: 1` - the
/// unit must escalate on its FIRST failed attempt, exactly as if the run-wide default were
/// 1, never surviving to a second attempt the way a plain (unoverridden) default of 5
/// would allow. Proves the YAML-parsed stage-level key genuinely LOWERS the effective
/// bound below a higher run default, through the real remediation loop.
#[test]
fn a_stages_own_max_retries_yaml_key_lowers_the_effective_bound_below_a_higher_default() {
    let dir = temp_repoless_project();
    let root = dir.path();
    write_repoless_worker_agent(root);
    std::fs::write(
        root.join(".rigger").join("workflow.yml"),
        r#"name: stagemaxretrieslowertest
defaults:
  grounder: nop
  budget: 60
  max_retries: 5
gates:
  bad: { run: "false", kind: core }
stages:
  solo:
    agent: worker
    gates: [bad]
    on_pass: none
    max_retries: 1
"#,
    )
    .unwrap();

    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the first step must succeed; stderr:\n{err}");
    assert!(
        out.contains(r#""id":"solo/implementer#0""#) && !out.contains("escalated"),
        "the first step only parks the implementer; got:\n{out}"
    );

    seed_run_events(
        root,
        &[(
            "SpawnResult",
            r#"{"id":"solo/implementer#0","output":"attempted"}"#,
        )],
    );

    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(
        ok,
        "a step that reaches an escalated fixpoint still exits 0; stderr:\n{err}"
    );
    assert!(
        out.contains(r#""escalated":["solo"]"#),
        "the stage's own max_retries: 1, parsed from real workflow.yml text, must \
         override the run-wide defaults.max_retries: 5 and escalate on the FIRST failed \
         attempt - a plain default of 5 would instead retry here; got:\n{out}"
    );
}

/// `defaults.max_retries: 1` (which alone would escalate on the first failed attempt, per
/// `write_failing_gate_escalating_workflow`'s own established shape in `tests/cli.rs`) but
/// the stage itself sets `max_retries: 3` - the unit must survive TWO failed attempts,
/// escalating only on the third. Proves the YAML-parsed stage-level key genuinely RAISES
/// the effective bound above a lower run default, through the real remediation loop's
/// repeated spawn/gate/retry cycle (never merely the pure `max_retries_for` arithmetic).
#[test]
fn a_stages_own_max_retries_yaml_key_raises_the_effective_bound_above_a_lower_default() {
    let dir = temp_repoless_project();
    let root = dir.path();
    write_repoless_worker_agent(root);
    std::fs::write(
        root.join(".rigger").join("workflow.yml"),
        r#"name: stagemaxretriesraisetest
defaults:
  grounder: nop
  budget: 60
  max_retries: 1
gates:
  bad: { run: "false", kind: core }
stages:
  solo:
    agent: worker
    gates: [bad]
    on_pass: none
    max_retries: 3
"#,
    )
    .unwrap();

    // Attempt 0: park, fail, and - because the stage's max_retries: 3 overrides the
    // run-wide default of 1 - RETRY (attempt 1) rather than escalate.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "step 1 must succeed; stderr:\n{err}");
    assert!(
        out.contains(r#""id":"solo/implementer#0""#),
        "step 1 parks attempt 0; got:\n{out}"
    );
    seed_run_events(
        root,
        &[(
            "SpawnResult",
            r#"{"id":"solo/implementer#0","output":"attempted"}"#,
        )],
    );

    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "step 2 must succeed; stderr:\n{err}");
    assert!(
        out.contains(r#""id":"solo/implementer#1""#) && !out.contains("escalated"),
        "with the stage's own max_retries: 3 in force, the first failure must RETRY (a \
         fresh attempt 1 parks) rather than escalate - a plain default of 1 would have \
         already escalated here; got:\n{out}"
    );

    // Attempt 1: fail again, RETRY again (attempts so far: 2 < 3).
    seed_run_events(
        root,
        &[(
            "SpawnResult",
            r#"{"id":"solo/implementer#1","output":"attempted"}"#,
        )],
    );
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "step 3 must succeed; stderr:\n{err}");
    assert!(
        out.contains(r#""id":"solo/implementer#2""#) && !out.contains("escalated"),
        "the second failure must still retry (attempts so far: 2 < the stage's own bound \
         of 3) - a fresh attempt 2 parks, still no escalation; got:\n{out}"
    );

    // Attempt 2: fail a third time - attempts now reach 3, meeting the stage's own bound,
    // so this failure finally escalates.
    seed_run_events(
        root,
        &[(
            "SpawnResult",
            r#"{"id":"solo/implementer#2","output":"attempted"}"#,
        )],
    );
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(
        ok,
        "a step that reaches an escalated fixpoint still exits 0; stderr:\n{err}"
    );
    assert!(
        out.contains(r#""escalated":["solo"]"#),
        "the THIRD failed attempt must finally escalate, exactly matching the stage's own \
         max_retries: 3 (never the run-wide default of 1); got:\n{out}"
    );
}
