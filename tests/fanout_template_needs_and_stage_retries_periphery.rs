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
//!   - `a_planner_supersede_of_a_fan_out_member_still_satisfies_its_downstream_needs_edge`
//!     (round 2, closing `adj-u91c1-verdict-reject` /
//!     `arch-u91c1-fanout-members-orphans-a-superseded-baseline`) proves rule 1 survives a
//!     planner supersede of a fan-out member, end to end through the real `run()` wiring.
//!   - `a_real_split_pair_must_both_integrate_not_just_the_btreemap_key_first_sibling`
//!     (round 3, closing `adj-u91c1-r2-verdict-reject` /
//!     `arch-u91c1-r2-need-satisfied-ignores-real-split-siblings`) proves `need_satisfied`
//!     requires EVERY live sibling sharing a criterion id to integrate - not merely the
//!     first one a `BTreeMap` iterates to - over a hand-built `stages`/`integrated`/
//!     `terminal` map, both directions (early integration, permanent escalation).
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
//!   5. Nothing proves the round-2 supersede fix (gap 3's SAME class of concern, for the
//!      supersede path specifically) survives a REAL git-backed integrate: the round-2
//!      regression test passes `deps.repo: String::new()` - no real git at all, so its
//!      `on_pass: merge` stages never actually merge anything - and answers through a
//!      private, hand-emitting `Stub`, never the crate's own public `AgentDriver` trait a
//!      real embedder implements. Nothing proves the criterion-id-keyed live-owner
//!      resolution `need_satisfied` now does survives an ACTUAL worktree-create -> commit
//!      -> merge cycle for the unit that supersedes a fan-out baseline, as opposed to an
//!      in-memory `Stub` answering synchronously.
//!   6. Nothing proves the round-3 real-split-sibling fix (`need_satisfied` resolving
//!      EVERY live owner of a criterion id, not merely the first one `stages.iter()`
//!      reaches) survives a REAL SPLIT produced by a genuine multi-process planner
//!      courier and REAL git merges (or a REAL escalation) for each sibling - the
//!      round-3 regression test itself proves the fixed arithmetic in one process over a
//!      hand-built `stages`/`integrated`/`terminal` map, never through `rigger step`,
//!      `rigger emit`, or `rigger result` process boundaries, and never through an
//!      actual worktree-create -> commit -> merge (or escalate) cycle for either sibling.
//!
//! This file closes all six, through the compiled binary or the crate's public `run`/
//! `AgentDriver` API (gap 5, which needs a real git repo and a real superseding proposal),
//! with real git worktrees/merges wherever the scenario needs them, and real `rigger
//! step`/`rigger emit --spawn`/`rigger result` process boundaries for gaps 1-4 and 6 -
//! gap 6's real split is produced by two `rigger emit --spawn <plan-spawn-id>
//! UnitProposed` calls from the SAME parked spawn (`cmd_emit`, `src/main.rs`: the
//! established native-courier idiom a scripted, non-MCP-tooled planner uses to record a
//! decision, threading its own spawn id into `META_SPAWN` exactly as a live MCP-tooled
//! agent's stamped emit does), so both proposals share one episode identity - a real
//! multi-process planner idiom this file did not use for gap 5, which instead drives the
//! public `AgentDriver` trait directly.
//!
//! NOT OWNED HERE: the pure `ready_stages`/`wave_ready`/`max_retries_for`/`need_satisfied`
//! arithmetic itself (private to `conductor.rs`, exhaustively covered by its own colocated
//! tests), and the resume-grant-vs-stage-override interaction (pure arithmetic over
//! already-parsed structs, no new I/O boundary - covered by the implementer's own
//! `a_stages_own_max_retries_overrides_the_run_default_for_its_units`).

mod common;

use std::path::Path;
use std::process::Command;

use rigger::conductor::{
    run, AgentDriver, AgentResult, Deps, Error, SpawnOpts, TYPE_UNIT_PROPOSED,
};
use rigger::config::{AgentDef, Config, Gate, Stage};
use rigger::eventstore::sqlite::Store;
use rigger::gate::ExecRunner;
use rigger::ledger;
use serde_json::{json, Value};

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
// Rule 1, gap 5 (round-2 regression fix, `adj-u91c1-verdict-reject` /
// `arch-u91c1-fanout-members-orphans-a-superseded-baseline`): a real planner's `UnitProposed`
// superseding one fan-out baseline member must not orphan a downstream `needs: [<template>]`
// stage - proven through a REAL git-backed integrate (real worktrees, real commits, real
// merges), never the round-2 fix's own regression test
// (`a_planner_supersede_of_a_fan_out_member_still_satisfies_its_downstream_needs_edge`,
// src/conductor.rs `mod tests`), which passes `deps.repo: String::new()` - no real git at
// all, so its `on_pass: merge` stages never actually merge anything - and answers through a
// private, hand-emitting `Stub`, never the crate's own public `AgentDriver` trait a real
// embedder implements. This proves the criterion-id-keyed live-owner resolution
// `need_satisfied`'s round-2 fix added survives an ACTUAL worktree-create -> commit -> merge
// cycle for the SUPERSEDING unit, not merely an in-memory `Stub` answering synchronously.
// -----------------------------------------------------------------------------------------

/// A two-role `AgentDriver`, over the crate's PUBLIC trait (never `conductor.rs`'s private
/// `Stub`): `planner` emits ONE real `UnitProposed` through the SAME `emit` closure a live
/// planner agent's `rigger_emit` calls are wired to, proposing `proposed_id` for
/// `criterion` under a DIFFERENT id than the deterministic baseline `baseline_units` would
/// otherwise have synthesized for it - the designed spec-18/72 supersede path. Every other
/// (`worker`) spawn writes one real, unit-named file into its real, isolated worktree, so
/// its stage's `on_pass: merge` (or the equally-eligible unset default - `integrates`,
/// `src/conductor.rs`, treats empty `on_pass` as merge too) lands a genuine git commit
/// rather than a no-op.
struct RealGitSupersedingPlannerDriver {
    proposed_id: String,
    criterion: String,
}

impl AgentDriver for RealGitSupersedingPlannerDriver {
    fn spawn(
        &self,
        agent: &AgentDef,
        _prompt: &str,
        opts: &SpawnOpts,
        emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        if agent.id == "planner" {
            emit(
                TYPE_UNIT_PROPOSED,
                json!({
                    "id": self.proposed_id,
                    "agent": "worker",
                    "criterion": self.criterion,
                    "gates": ["ok"],
                }),
            )?;
            return Ok(AgentResult {
                output: "proposed a refinement".into(),
                resolved_model: String::new(),
            });
        }
        if !opts.dir.is_empty() {
            let file = format!(
                "{}/{}.rs",
                opts.dir,
                opts.unit.replace(|c: char| !c.is_ascii_alphanumeric(), "_")
            );
            std::fs::write(file, "pub fn done() {}\n").unwrap();
        }
        Ok(AgentResult {
            output: "ok".into(),
            resolved_model: String::new(),
        })
    }
}

/// Proves the round-2 fix (`fanout_criteria` resolving a needs edge by each criterion's
/// LIVE `stages` owner, never a frozen unit-id snapshot) end to end through a REAL git
/// repo: `checkin` (`needs: [implement]`) must still integrate once the SUPERSEDING unit
/// for criterion A (a planner-proposed id the deterministic baseline decomposition never
/// produced) and criterion B's ordinary baseline BOTH land through real, separate git
/// merges onto the base branch.
#[test]
fn checkin_integrates_after_a_real_planner_supersede_of_a_fanout_baseline_lands_via_real_git_merges(
) {
    let repo = temp_git_project_with_commit();

    let mut cfg = Config::default();
    for id in ["planner", "worker"] {
        cfg.agents.insert(
            id.into(),
            AgentDef {
                id: id.into(),
                ..Default::default()
            },
        );
    }
    cfg.workflow.gates.insert(
        "ok".into(),
        Gate {
            run: "true".into(),
            kind: "core".into(),
            inputs: Vec::new(),
        },
    );
    cfg.workflow.stages.insert(
        "plan".into(),
        Stage {
            name: "plan".into(),
            agent: "planner".into(),
            produces: "dag".into(),
            ..Default::default()
        },
    );
    cfg.workflow.stages.insert(
        "implement".into(),
        Stage {
            name: "implement".into(),
            agent: "worker".into(),
            strategy: "fan-out".into(),
            needs: vec!["plan".into()],
            gates: vec!["ok".into()],
            on_pass: "merge".into(),
            ..Default::default()
        },
    );
    cfg.workflow.stages.insert(
        "checkin".into(),
        Stage {
            name: "checkin".into(),
            agent: "worker".into(),
            needs: vec!["implement".into()],
            gates: vec!["ok".into()],
            on_pass: "merge".into(),
            ..Default::default()
        },
    );

    let crit_a = "the auth module lands";
    let crit_b = "the billing module lands";
    let superseding_id = "planner-refines-the-auth-module";

    let store = Store::open(":memory:").unwrap();
    let driver = RealGitSupersedingPlannerDriver {
        proposed_id: superseding_id.to_string(),
        criterion: crit_a.to_string(),
    };
    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &ExecRunner,
        repo: repo.path().to_str().unwrap().to_string(),
        grounder: None,
        graph: None,
        criteria: vec![crit_a.to_string(), crit_b.to_string()],
    };
    let rs = run(&cfg, &deps).unwrap();

    // Exactly 4 units ever ran: "plan", "checkin", the real superseding unit, and criterion
    // B's ordinary baseline - NEVER a 5th (criterion A's own deterministic baseline
    // surviving ALONGSIDE the unit that superseded it, the round-1 defect this proves
    // fixed).
    assert_eq!(
        rs.units.len(),
        4,
        "plan + checkin + exactly 2 implement-derived units (the real superseding unit for \
         criterion A, the ordinary baseline for criterion B) - a 5th would mean criterion \
         A's baseline survived alongside its superseding unit; units: {:?}",
        rs.units.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        rs.units[superseding_id].status,
        ledger::Status::Integrated,
        "the real planner-proposed unit superseding criterion A's baseline must integrate \
         through a real git merge"
    );

    // Find criterion B's ordinary baseline by elimination - its deterministic slug is
    // derived from criterion text this test does not hand-compute - and confirm it
    // integrated too.
    let crit_b_baseline = rs
        .units
        .keys()
        .find(|k| k.as_str() != "plan" && k.as_str() != "checkin" && k.as_str() != superseding_id)
        .cloned()
        .expect("criterion B's ordinary baseline must exist as a 4th unit");
    assert_eq!(
        rs.units[&crit_b_baseline].status,
        ledger::Status::Integrated,
        "criterion B's ordinary baseline must integrate through a real git merge"
    );

    // THE ASSERTION THAT WAS RED before the round-2 fix: checkin needs the fan-out
    // template, whose live owner for criterion A is now the REAL superseding unit - an id
    // the deterministic baseline decomposition never produced. A frozen unit-id snapshot
    // could never see it integrate; checkin would never even appear in `rs.units` (the run
    // silently converges one wave early instead, exactly as
    // arch-u91c1-fanout-members-orphans-a-superseded-baseline described).
    assert_eq!(
        rs.units.get("checkin").map(|u| u.status),
        Some(ledger::Status::Integrated),
        "checkin must become ready and integrate once every criterion's CURRENT live owner \
         has integrated through a REAL git merge, even when a real planner supersede \
         changed which unit id owns criterion A; got units: {:?}",
        rs.units.keys().collect::<Vec<_>>()
    );

    // Non-vacuity: the base repo's working tree really carries every landed unit's own
    // real committed file after the real merges, not merely an in-memory status
    // transition.
    for name in [superseding_id, crit_b_baseline.as_str(), "checkin"] {
        let file_name = format!(
            "{}.rs",
            name.replace(|c: char| !c.is_ascii_alphanumeric(), "_")
        );
        let landed = repo.path().join(&file_name);
        assert!(
            landed.exists(),
            "unit {name:?}'s real committed file must have landed on the base after its \
             real merge: {}",
            landed.display()
        );
    }
}

// -----------------------------------------------------------------------------------------
// Rule 1, gap 6 (round-3 fix, closing `adj-u91c1-r2-verdict-reject` /
// `arch-u91c1-r2-need-satisfied-ignores-real-split-siblings`): a REAL SPLIT - a same-episode
// planner proposal that adds a SECOND live unit sharing one criterion id alongside the
// first (spec 31/72's real-split guarantee: `harvest_proposed` never reaps a genuinely-new
// same-episode sibling, only a strictly-earlier-episode owner) - must require EVERY live
// sibling under that criterion id to integrate before a downstream `needs: [<template>]`
// stage becomes ready. Round 2's own fix (`need_satisfied`'s `stages.iter().find(..)`
// resolution) silently satisfied - or silently NEVER satisfied - the whole entry on
// whichever sibling `BTreeMap` iteration reaches first, ignoring every other live sibling
// entirely; round 3 replaced it with `.filter(..).all(..)`. The round-3 regression test
// proving this (`a_real_split_pair_must_both_integrate_not_just_the_btreemap_key_first_sibling`,
// `src/conductor.rs` `mod tests`) answers through a hand-built `stages`/`integrated`/
// `terminal` map in one process - never a real planner proposal, never a real gate, never a
// real git merge or a real escalation. The two tests below close that: a REAL split is
// produced by a native courier's `rigger emit --spawn <plan-spawn-id> UnitProposed` (see
// `propose_real_split` below) called TWICE from the SAME parked `plan` spawn so both
// proposals share one episode identity, and each split unit's own real gate outcome and
// real git merge (or real escalation) is driven through a genuine `rigger step`/`rigger
// result` process boundary - never the crate's private `Stub`/`AgentDriver` in-process seam
// gap 5 above already covers.
// -----------------------------------------------------------------------------------------

/// The one-criterion spec text every real-split test below decomposes: exactly one
/// deterministic baseline unit, so the planner's real split is the ONLY source of a second
/// live owner for its criterion id (never a second baseline from a second criterion).
const SPLIT_CRITERION: &str = "the auth module lands";

fn write_split_criterion_spec(root: &Path) {
    std::fs::write(
        root.join("spec.md"),
        format!("# Spec\n\n## Done when\n\n- [ ] {SPLIT_CRITERION}\n"),
    )
    .unwrap();
}

/// The fan-out workflow every real-split test below shares: `plan` (agent: worker, the
/// producer), `implement` (the fan-out template, needs the producer implicitly via
/// `baseline_units`), `checkin` (`needs: [implement]`, the edge under test). One agent id
/// ("worker", real git isolation) plays every role - the planner's own real actions here
/// are driven by the TEST's calls to `rigger emit`/`rigger result`, never by an LLM, so a
/// single agent identity suffices exactly as it does for `write_git_worker_agent`'s other
/// callers above.
fn write_split_fanout_workflow(root: &Path) {
    write_git_worker_agent(root);
    std::fs::write(
        root.join(".rigger").join("workflow.yml"),
        r#"name: fanoutrealsplittest
defaults:
  grounder: nop
  budget: 60
  max_retries: 1
gates:
  ok: { run: "true", kind: core }
  bad: { run: "false", kind: core }
stages:
  plan:
    agent: worker
    produces: dag
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
}

/// Post one `UnitProposed` decision for the currently-parked spawn `spawn`, through the
/// real `rigger emit --spawn` CLI courier (`cmd_emit`, `src/main.rs`) - the established
/// native-courier idiom for a scripted (non-MCP) agent to record a decision, threading its
/// OWN spawn id into `META_SPAWN` exactly as a live MCP-tooled planner's stamped emit does.
/// Never the crate's private in-process `AgentDriver::spawn` emit closure gap 5's own test
/// uses above - this drives the SAME real subprocess boundary `run_rigger` already does for
/// `rigger step`/`rigger result` in every other test in this file.
fn propose_unit_via_rigger_emit(root: &Path, spawn: &str, body: &Value) {
    let (_out, err, ok) = run_rigger(
        root,
        &["emit", "--spawn", spawn, "UnitProposed", &body.to_string()],
    );
    assert!(
        ok,
        "rigger emit --spawn {spawn} UnitProposed {body} must succeed; stderr:\n{err}"
    );
}

/// A same-episode REAL SPLIT of `SPLIT_CRITERION`'s deterministic baseline into two live
/// siblings, `split-a-1` (gate `gate_a1`) and `split-a-2` (gate `gate_a2`), proposed as two
/// separate `rigger emit --spawn` calls from the SAME parked `plan` spawn so both share one
/// episode identity - the shape `harvest_proposed` never reaps a genuinely-new same-episode
/// sibling for (spec 31/72), then records `plan`'s own result so its real (empty, since a
/// planner writes no files) worktree merges and integrates.
fn propose_real_split(root: &Path, gate_a1: &str, gate_a2: &str) {
    let plan_spawn = "plan/implementer#0";
    propose_unit_via_rigger_emit(
        root,
        plan_spawn,
        &json!({"id": "split-a-1", "agent": "worker", "criterion": SPLIT_CRITERION, "gates": [gate_a1]}),
    );
    propose_unit_via_rigger_emit(
        root,
        plan_spawn,
        &json!({"id": "split-a-2", "agent": "worker", "criterion": SPLIT_CRITERION, "gates": [gate_a2]}),
    );
    let (_o, err, ok) = run_rigger(root, &["result", plan_spawn, "proposed a real split"]);
    assert!(
        ok,
        "recording plan's own result must succeed; stderr: {err}"
    );
}

/// checkin's needs edge must NEVER become satisfied while ANY live real-split sibling has
/// not integrated - even after its `BTreeMap`-key-first sibling (`split-a-1`, which sorts
/// before `split-a-2`) has ALREADY integrated through a real git merge. Proves the round-3
/// fix's early-satisfaction direction (`sdet-u91c1-r2-confirms-split-sibling-orphan`'s exact
/// repro) through a real multi-step `rigger step` process and real git merges, not a
/// hand-built map in one call.
#[test]
fn checkin_stays_unready_while_a_real_split_siblings_partner_has_not_integrated_yet() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_split_fanout_workflow(root);
    write_split_criterion_spec(root);

    // Step 1: only "plan" is ready (the baseline needs the producer); no split proposed
    // yet.
    let (out, err, ok) = run_rigger(root, &["step", "--spec", "spec.md"]);
    assert!(ok, "step 1 must succeed; stderr:\n{err}");
    assert!(
        out.contains(r#""id":"plan/implementer#0""#),
        "plan must park first; got:\n{out}"
    );
    assert!(
        !out.contains("split-a") && !out.contains("checkin"),
        "neither split sibling nor checkin exists before the planner proposes; got:\n{out}"
    );

    // A REAL split: both siblings pass "ok", from the SAME plan episode.
    propose_real_split(root, "ok", "ok");

    // Step 2: plan's own real (empty) worktree merges trivially and integrates; BOTH real
    // split siblings are now live (superseding the deterministic baseline) and ready - both
    // park in this same step.
    let (out, err, ok) = run_rigger(root, &["step", "--spec", "spec.md"]);
    assert!(ok, "step 2 must succeed; stderr:\n{err}");
    for id in ["split-a-1/implementer#0", "split-a-2/implementer#0"] {
        assert!(
            out.contains(&format!(r#""id":"{id}""#)),
            "both real split siblings must park together in the step their planner \
             proposal lands; got:\n{out}"
        );
    }
    assert!(
        !out.contains("checkin"),
        "checkin must not appear before either split sibling has even attempted its gate; \
         got:\n{out}"
    );

    // Land ONLY split-a-1 for real - the BTreeMap-key-first sibling (alphabetically
    // first), the exact one round 2's `.find()` locked onto.
    let wt_a1 = root.join(".rigger").join("tmp").join("rigger-wt-split-a-1");
    assert!(
        wt_a1.exists(),
        "split-a-1 must already have its real worktree on disk: {}",
        wt_a1.display()
    );
    std::fs::write(wt_a1.join("a1.rs"), "pub fn a1() {}\n").unwrap();
    let (_o, err, ok) = run_rigger(
        root,
        &["result", "split-a-1/implementer#0", "landed split a1"],
    );
    assert!(
        ok,
        "recording split-a-1's result must succeed; stderr: {err}"
    );

    // Step 3: split-a-1's real merge lands (Integrated); split-a-2 has posted no result yet
    // and stays outstanding. THE ASSERTION THAT WAS RED before the round-3 fix: checkin
    // must NOT appear even though the BTreeMap-key-first sibling has genuinely integrated
    // through a real git merge - its real-split partner has not.
    let (out, err, ok) = run_rigger(root, &["step", "--spec", "spec.md"]);
    assert!(ok, "step 3 must succeed; stderr:\n{err}");
    assert!(
        root.join("a1.rs").exists(),
        "split-a-1's real merge must have actually landed its file on the base"
    );
    assert!(
        !out.contains("checkin"),
        "split-a-2 (a real split sibling under the SAME criterion id as split-a-1) has not \
         integrated yet - checkin's needs edge must stay unsatisfied even though split-a-1, \
         the BTreeMap-key-first sibling, has genuinely integrated through a real git merge; \
         got:\n{out}"
    );

    // Land split-a-2 too, the same way.
    let wt_a2 = root.join(".rigger").join("tmp").join("rigger-wt-split-a-2");
    assert!(
        wt_a2.exists(),
        "split-a-2 must already have its real worktree on disk: {}",
        wt_a2.display()
    );
    std::fs::write(wt_a2.join("a2.rs"), "pub fn a2() {}\n").unwrap();
    let (_o, err, ok) = run_rigger(
        root,
        &["result", "split-a-2/implementer#0", "landed split a2"],
    );
    assert!(
        ok,
        "recording split-a-2's result must succeed; stderr: {err}"
    );

    // Step 4: split-a-2's real merge lands too. NOW every live owner of the criterion has
    // integrated, so checkin's needs edge is satisfied and its own implementer parks.
    let (out, err, ok) = run_rigger(root, &["step", "--spec", "spec.md"]);
    assert!(ok, "step 4 must succeed; stderr:\n{err}");
    assert!(
        root.join("a2.rs").exists(),
        "split-a-2's real merge must have actually landed its file on the base"
    );
    assert!(
        out.contains(r#""id":"checkin/implementer#0""#),
        "once BOTH real split siblings have integrated through real git merges, checkin's \
         needs edge must be satisfied and its own implementer must park; got:\n{out}"
    );
}

/// The reverse direction (round-3's own reverse-direction regression test,
/// `adv-u91c1-r2-confirms-split-sibling-reverse-direction`): a real-split sibling that
/// permanently ESCALATES (terminal, never integrating) must keep checkin unready forever -
/// even when its `BTreeMap`-key-first partner has ALREADY integrated through a real git
/// merge - through a real multi-step `rigger step` process and a real remediation-bound
/// escalation, never a hand-built terminal set in one call.
#[test]
fn checkin_never_becomes_ready_when_a_real_split_siblings_partner_escalates_instead() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_split_fanout_workflow(root);
    write_split_criterion_spec(root);

    let (out, err, ok) = run_rigger(root, &["step", "--spec", "spec.md"]);
    assert!(ok, "step 1 must succeed; stderr:\n{err}");
    assert!(
        out.contains(r#""id":"plan/implementer#0""#),
        "plan must park first; got:\n{out}"
    );

    // A REAL split: split-a-1 passes "ok"; split-a-2's gate always fails ("bad").
    propose_real_split(root, "ok", "bad");

    let (out, err, ok) = run_rigger(root, &["step", "--spec", "spec.md"]);
    assert!(ok, "step 2 must succeed; stderr:\n{err}");
    for id in ["split-a-1/implementer#0", "split-a-2/implementer#0"] {
        assert!(
            out.contains(&format!(r#""id":"{id}""#)),
            "both real split siblings must park together; got:\n{out}"
        );
    }

    let wt_a1 = root.join(".rigger").join("tmp").join("rigger-wt-split-a-1");
    std::fs::write(wt_a1.join("a1.rs"), "pub fn a1() {}\n").unwrap();
    let (_o, err, ok) = run_rigger(
        root,
        &["result", "split-a-1/implementer#0", "landed split a1"],
    );
    assert!(
        ok,
        "recording split-a-1's result must succeed; stderr: {err}"
    );

    // Step 3: split-a-1's real merge lands (Integrated); split-a-2 has posted no result yet.
    let (out, err, ok) = run_rigger(root, &["step", "--spec", "spec.md"]);
    assert!(ok, "step 3 must succeed; stderr:\n{err}");
    assert!(
        root.join("a1.rs").exists(),
        "split-a-1's real merge must have actually landed its file on the base"
    );
    assert!(
        !out.contains("checkin"),
        "checkin must not appear while split-a-2 is still outstanding, even though \
         split-a-1 has genuinely integrated; got:\n{out}"
    );

    let (_o, err, ok) = run_rigger(
        root,
        &["result", "split-a-2/implementer#0", "attempted split a2"],
    );
    assert!(
        ok,
        "recording split-a-2's result must succeed; stderr: {err}"
    );

    // Step 4: split-a-2's "bad" gate fails and the run-wide max_retries: 1 means this FIRST
    // failure IS the escalation - it goes terminal WITHOUT ever integrating. THE ASSERTION
    // THAT WAS RED before the round-3 fix (in the OPPOSITE direction from the test above):
    // checkin must never appear at all - not spawned, and not itself in the escalated set
    // either - even though its BTreeMap-key-first sibling genuinely integrated through a
    // real git merge.
    let (out, err, ok) = run_rigger(root, &["step", "--spec", "spec.md"]);
    assert!(
        ok,
        "a step that reaches an escalated fixpoint still exits 0; stderr:\n{err}"
    );
    assert!(
        out.contains(r#""escalated":["split-a-2"]"#),
        "split-a-2 must reach the escalated fixpoint on its first failed attempt (the run's \
         own max_retries: 1); got:\n{out}"
    );
    assert!(
        !out.contains("checkin"),
        "split-a-2 (a real split sibling under the SAME criterion id as split-a-1) \
         escalated without ever integrating - checkin must never appear anywhere, even \
         though split-a-1, the BTreeMap-key-first sibling, genuinely integrated through a \
         real git merge; got:\n{out}"
    );
    assert!(
        !root.join("a2.rs").exists(),
        "split-a-2 escalated without ever merging - its file must never have landed"
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
