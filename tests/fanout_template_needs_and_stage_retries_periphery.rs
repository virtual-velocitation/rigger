//! Periphery for spec 91, criterion 1: the two generic conductor rules. (1) A `needs` entry that
//! names the fan-out implement TEMPLATE is satisfied exactly when every live unit the template
//! expanded into has integrated, and stays unsatisfied while any member is open or reached a
//! terminal-but-not-integrated state. (2) A stage may set its own `max_retries`, overriding
//! `defaults.max_retries` for the units it governs.
//!
//! `src/conductor.rs`'s own `mod tests` proves both rules in one process, over a hand-built
//! config, a stub driver and an in-memory store:
//!   - `a_stage_needing_the_fan_out_template_becomes_ready_once_every_criterion_unit_integrates`
//!     drives the `run()` wiring for rule 1's happy path;
//!   - `a_downstream_stage_needing_the_fan_out_template_stays_unready_until_every_member_integrates`
//!     proves rule 1 over `ready_stages`/`wave_ready`, including a partially integrated group and
//!     an escalated member;
//!   - `a_planner_supersede_of_a_fan_out_member_still_satisfies_its_downstream_needs_edge` proves
//!     rule 1 survives a planner supersede of a fan-out member;
//!   - `a_real_split_pair_must_both_integrate_not_just_the_btreemap_key_first_sibling` proves
//!     `need_satisfied` requires every live sibling sharing a criterion id to integrate, in both
//!     directions (one sibling integrated while its partner is still open, and a partner that
//!     escalates);
//!   - `a_stages_own_max_retries_overrides_the_run_default_for_its_units` proves rule 2's
//!     precedence (`max_retries_for`), including the resume grant.
//!
//! What none of them reach, and this file drives:
//!   1. `max_retries:` as a stage-level key in `workflow.yml` text, parsed by the real
//!      deserializer, changing when a `rigger run` escalates through its remediation loop, in
//!      both directions (lowering and raising the bound).
//!   2. Rule 1 through the compiled binary: fan-out members that integrate through real git
//!      worktrees and merges onto the run branch, and a downstream `needs: [<template>]` stage
//!      the run starts only after every member's integration is on the log, or never starts at
//!      all once a member escalates.
//!   3. Rule 1 over the live owners a real planner leaves: a split proposed through `rigger emit
//!      --spawn <plan-spawn> UnitProposed` from the planner's own spawn (two and three
//!      same-episode siblings, and a sibling that escalates), and a same-id refine later
//!      superseded by a distinct id from a later episode.
//!   4. Rule 1 across a planner supersede through the crate's public `run`/`AgentDriver` API
//!      over a real git repo, so the superseding unit's worktree-create, commit and merge cycle
//!      is real.
//!
//! NOT OWNED HERE: the pure `ready_stages`/`wave_ready`/`max_retries_for`/`need_satisfied`
//! arithmetic itself (private to `conductor.rs`, covered by its own tests above), including the
//! window in which one split sibling has integrated while its partner is still open.

use crate::common;

use std::path::{Path, PathBuf};

use common::cli::{read_run_events, run_on_arms, temp_repoless_project};
use common::fixtures::test_spawn_host;
use common::fixtures::{stand_in_launches, RECORD_EACH_LAUNCH};
use common::fixtures::{unit_proposal, ProposingPlannerDriver};
use common::git::{git_out, temp_git_project_with_commit};
use rigger::conductor::{run, Deps};
use rigger::config::{AgentDef, Config, Gate, Stage};
use rigger::eventstore::sqlite::Store;
use rigger::gate::ExecRunner;
use rigger::ledger;

/// Write the `worker` agent file with `isolation` frontmatter lines (each ending in `\n`):
/// [`GIT_WORKER`] for a git-worktree-backed worker whose `on_pass: merge` reaches a real merge
/// onto the run branch, [`REPOLESS_WORKER`] for a repo-less one.
fn write_worker_agent(root: &Path, isolation: &str) {
    let agents = root.join(".rigger").join("agents");
    std::fs::create_dir_all(&agents).unwrap();
    std::fs::write(
        agents.join("worker.md"),
        format!(
            "---\nid: worker\nmodel: sonnet\ntools: [Read, Edit]\n{isolation}---\nDo the unit.\n"
        ),
    )
    .unwrap();
}

/// The git-worktree-backed worker's (empty) isolation frontmatter.
const GIT_WORKER: &str = "";

/// The repo-less worker's isolation frontmatter.
const REPOLESS_WORKER: &str = "isolation: none\n";

/// A canonical scratch root of its own, kept alive by the returned guard.
fn scratch_root() -> (tempfile::TempDir, PathBuf) {
    let held = tempfile::tempdir().unwrap();
    let scratch = std::fs::canonicalize(held.path()).unwrap();
    (held, scratch)
}

/// What one `rigger run` answered: its stdout, stderr and success, and the spawn of every launch
/// the fake host put on record, in spawn order.
struct Run {
    out: String,
    err: String,
    ok: bool,
    launched: Vec<String>,
}

impl Run {
    /// Whether the run state it printed shows `unit` at `state`.
    fn shows(&self, unit: &str, state: &str) -> bool {
        self.out
            .lines()
            .any(|line| line.split_whitespace().collect::<Vec<_>>() == [unit, state])
    }
}

/// `rigger <args>` over `root` under a scratch root of its own, the fake host answering from
/// `arms`.
fn run_over(root: &Path, args: &[&str], arms: &str) -> Run {
    let (_held, scratch) = scratch_root();
    let (out, err, ok, fake) = run_on_arms(root, args, &scratch, arms);
    let on_record = fake.path().join("bin/launches");
    let launched = if on_record.exists() {
        stand_in_launches(&on_record)
            .into_iter()
            .map(|l| l.spawn)
            .collect()
    } else {
        Vec::new()
    };
    Run {
        out,
        err,
        ok,
        launched,
    }
}

/// The file a member's implementer writes and its merge lands: its spawn id with every
/// character but a lowercase letter or digit made `_`, then `.rs`.
fn landed_file(spawn: &str) -> String {
    format!(
        "{}.rs",
        spawn.replace(
            |c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit()),
            "_"
        )
    )
}

// -----------------------------------------------------------------------------------------
// Rule 1: a `needs: [implement]` stage starts only once every live owner of the template's
// criteria has integrated, and never once one escalates.
// -----------------------------------------------------------------------------------------

/// The one-criterion spec text every planned split below decomposes: exactly one deterministic
/// baseline unit, so the planner's proposals are the only source of a second live owner for its
/// criterion id.
const SPLIT_CRITERION: &str = "the auth module lands";

/// Which planning episode a proposal comes from.
#[derive(Clone, Copy)]
enum Episode {
    /// The planner's own spawn.
    Own,
    /// A strictly later episode, `plan/replan#1`.
    Later,
}

/// One planner proposal for [`SPLIT_CRITERION`]: its episode, unit id and gate.
type Proposal = (Episode, &'static str, &'static str);

/// The fan-out workflow: when `planned`, a `plan` stage (`produces: dag`) the template's units
/// need; `implement`, the fan-out template, gated `template_gate`; and `checkin`, `needs:
/// [implement]`, the edge under test. One git-isolated agent plays every role; the run-wide
/// remediation bound of one escalates a member on its first failed gate.
fn write_fan_out_workflow(root: &Path, planned: bool, template_gate: &str) {
    write_worker_agent(root, GIT_WORKER);
    let plan = if planned {
        "  plan:\n    agent: worker\n    produces: dag\n"
    } else {
        ""
    };
    std::fs::write(
        root.join(".rigger").join("workflow.yml"),
        format!(
            r#"defaults:
  grounder: nop
  budget: 60
  max_retries: 1
gates:
  ok: {{ run: "true", kind: core }}
  bad: {{ run: "false", kind: core }}
stages:
{plan}  implement:
    agent: worker
    strategy: fan-out
    gates: [{template_gate}]
    on_pass: merge
  checkin:
    agent: worker
    needs: [implement]
    gates: [ok]
    on_pass: merge
"#
        ),
    )
    .unwrap();
}

/// The worker's arm: the planner's spawn records each of `proposals` through `rigger emit
/// --spawn` against the store in `root`, in order; every other spawn writes its
/// [`landed_file`] for its stage's merge to land.
fn fan_out_arm(root: &Path, proposals: &[Proposal]) -> String {
    let bin = common::rigger_bin();
    let emits: Vec<String> = proposals
        .iter()
        .map(|(episode, id, gate)| {
            let spawn = match episode {
                Episode::Own => r#""$RIGGER_SPAWN""#,
                Episode::Later => "'plan/replan#1'",
            };
            let body = serde_json::json!({
                "id": id, "agent": "worker", "criterion": SPLIT_CRITERION, "gates": [gate],
            });
            format!(
                "'{}' emit --spawn {spawn} UnitProposed '{body}'",
                bin.display()
            )
        })
        .collect();
    let plan = if emits.is_empty() {
        String::new()
    } else {
        format!(
            "      plan/*) (cd '{}' && {}) >&2 ;;\n",
            root.display(),
            emits.join(" && ")
        )
    };
    format!(
        r#"  *"Do the unit."*)
{RECORD_EACH_LAUNCH}    case "$RIGGER_SPAWN" in
{plan}      *) echo 'pub fn done() {{}}' > "$(printf '%s' "$RIGGER_SPAWN" | tr -c 'a-z0-9' _).rs" ;;
    esac
    echo done
    ;;
"#
    )
}

/// The log position of the first event of type `kind` whose `id` is `unit`.
fn position_of(events: &[rigger::eventstore::Event], kind: &str, unit: &str) -> Option<u64> {
    events
        .iter()
        .filter(|e| e.type_ == kind)
        .find(|e| {
            serde_json::from_slice::<serde_json::Value>(&e.data).unwrap()["id"].as_str()
                == Some(unit)
        })
        .map(|e| e.position)
}

/// Given a spec of `criteria`, the fan-out template gated `template_gate` and, when any, a
/// planner that records `proposals`, when `rigger run` drives the spec, then: each unit of
/// `integrated` launches once and integrates, its file landed on the run branch; each unit of
/// `escalated` launches once, escalates and lands nothing; no unit of `never` ever starts; and
/// `checkin` starts only after every unit of `integrated` has integrated, then integrates, when
/// nothing escalated, and never starts or launches at all when anything did.
fn a_run_over_a_fan_out(
    criteria: &[&str],
    template_gate: &str,
    proposals: &[Proposal],
    integrated: &[&str],
    escalated: &[&str],
    never: &[&str],
) {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    let planned = !proposals.is_empty();
    write_fan_out_workflow(root, planned, template_gate);
    let done_when: String = criteria.iter().map(|c| format!("- [ ] {c}\n")).collect();
    std::fs::write(
        root.join("spec.md"),
        format!("# Spec\n\n## Done when\n\n{done_when}"),
    )
    .unwrap();

    let run = run_over(root, &["run", "spec.md"], &fan_out_arm(root, proposals));
    let events = read_run_events(root);
    let dump = || {
        format!(
            "stdout:\n{}\nstderr:\n{}\nlaunched: {:?}",
            run.out, run.err, run.launched
        )
    };
    assert!(run.ok, "the run ends cleanly; {}", dump());

    let ready = escalated.is_empty();
    let mut expected: Vec<String> = integrated
        .iter()
        .chain(escalated)
        .map(|unit| format!("{unit}/implementer#0"))
        .collect();
    if planned {
        expected.push("plan/implementer#0".to_string());
    }
    if ready {
        expected.push("checkin/implementer#0".to_string());
    }
    expected.sort();
    assert_eq!(run.launched, expected, "the spawns the run launches");

    for unit in integrated {
        assert!(
            run.shows(unit, "integrated"),
            "{unit} integrates; {}",
            dump()
        );
        assert_eq!(
            git_out(
                root,
                &[
                    "show",
                    &format!(
                        "rigger-run:{}",
                        landed_file(&format!("{unit}/implementer#0"))
                    )
                ]
            ),
            "pub fn done() {}",
            "{unit}'s merge lands its file on the run branch"
        );
    }
    for unit in escalated {
        assert!(run.shows(unit, "escalated"), "{unit} escalates; {}", dump());
        assert!(
            position_of(&events, "UnitIntegrated", unit).is_none(),
            "{unit} escalated without ever integrating"
        );
    }
    for unit in never {
        assert_eq!(
            position_of(&events, "UnitStarted", unit),
            None,
            "{unit} was superseded before it was ever scheduled, so it never starts"
        );
    }

    let checkin = position_of(&events, "UnitStarted", "checkin");
    if ready {
        let last_member = integrated
            .iter()
            .map(|unit| position_of(&events, "UnitIntegrated", unit).unwrap())
            .max()
            .unwrap();
        assert!(
            checkin.is_some_and(|started| started > last_member),
            "checkin starts only after every live owner of the template's criteria has \
             integrated (the last at {last_member}); checkin started at {checkin:?}"
        );
        assert!(
            run.shows("checkin", "integrated"),
            "checkin integrates; {}",
            dump()
        );
    } else {
        assert_eq!(
            checkin,
            None,
            "a member escalated instead of integrating, so checkin never starts; {}",
            dump()
        );
    }
}

rigger::test_cases! {
    /// Two Done-when criteria decompose into two baseline units of the template, which both
    /// integrate through real merges before `checkin` starts.
    checkin_becomes_ready_only_once_both_real_fanout_baseline_units_have_integrated:
        a_run_over_a_fan_out(
            &["alpha lands cleanly", "beta lands too"],
            "ok",
            &[],
            &["unit-1-alpha-lands-cleanly", "unit-2-beta-lands-too"],
            &[],
            &[],
        );
    /// The template's only member escalates on its first failed gate, so `checkin` never
    /// starts and is not itself escalated.
    checkin_never_becomes_ready_when_its_only_fanout_member_escalates_instead_of_integrating:
        a_run_over_a_fan_out(
            &["gamma never lands"],
            "bad",
            &[],
            &[],
            &["unit-1-gamma-never-lands"],
            &[],
        );
    /// A same-episode two-way split: `checkin` waits for the second sibling, not only the one
    /// that sorts first.
    checkin_stays_unready_while_a_real_split_siblings_partner_has_not_integrated_yet:
        a_run_over_a_fan_out(
            &[SPLIT_CRITERION],
            "ok",
            &[(Episode::Own, "split-a-1", "ok"), (Episode::Own, "split-a-2", "ok")],
            &["split-a-1", "split-a-2"],
            &[],
            &[],
        );
    /// A same-episode split whose second sibling escalates keeps `checkin` from ever starting,
    /// though the first sibling integrated.
    checkin_never_becomes_ready_when_a_real_split_siblings_partner_escalates_instead:
        a_run_over_a_fan_out(
            &[SPLIT_CRITERION],
            "ok",
            &[(Episode::Own, "split-a-1", "ok"), (Episode::Own, "split-a-2", "bad")],
            &["split-a-1"],
            &["split-a-2"],
            &[],
        );
    /// A three-way split: the predicate holds over every live owner, not a pair.
    checkin_stays_unready_until_every_member_of_a_same_episode_three_way_split_has_integrated:
        a_run_over_a_fan_out(
            &[SPLIT_CRITERION],
            "ok",
            &[
                (Episode::Own, "split-a-1", "ok"),
                (Episode::Own, "split-a-2", "ok"),
                (Episode::Own, "split-a-3", "ok"),
            ],
            &["split-a-1", "split-a-2", "split-a-3"],
            &[],
            &[],
        );
    /// A same-id refine of `auth-v1` folds into one unit, a distinct `auth-v2` from a later
    /// episode supersedes it before it is scheduled, and `checkin` tracks only `auth-v2`.
    checkin_integrates_after_a_same_id_refine_is_later_superseded_by_a_distinct_proposal:
        a_run_over_a_fan_out(
            &[SPLIT_CRITERION],
            "ok",
            &[
                (Episode::Own, "auth-v1", "ok"),
                (Episode::Own, "auth-v1", "ok"),
                (Episode::Later, "auth-v2", "ok"),
            ],
            &["auth-v2"],
            &[],
            &["auth-v1"],
        );
}

/// A planner supersede of a fan-out baseline, through the crate's public `run`/`AgentDriver` API
/// over a real git repo: `checkin` (`needs: [implement]`) integrates once the superseding unit
/// for criterion A (an id the deterministic baseline decomposition never produced) and
/// criterion B's ordinary baseline both land through real, separate git merges, because a needs
/// edge resolves each criterion by its live owner, never a frozen unit-id snapshot.
#[test]
fn checkin_integrates_after_a_real_planner_supersede_of_a_fanout_baseline_lands_via_real_git_merges(
) {
    let repo = temp_git_project_with_commit();

    let mut cfg = Config::default();
    // The fan-out unit worktrees of a real repo stay under the test's own workdir, never the
    // ambient cache home.
    cfg.workflow.defaults.workdir = common::isolated_workdir(repo.path());
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
            requires: Vec::new(),
            post_merge: true,
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
    let driver = ProposingPlannerDriver {
        proposals: vec![unit_proposal(superseding_id, crit_a, &["ok"])],
        output: "proposed a refinement",
    };
    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &ExecRunner,
        repo: repo.path().to_str().unwrap().to_string(),
        grounder: None,
        graph: None,
        criteria: vec![crit_a.to_string(), crit_b.to_string()],
        log: &|_| {},
        hash_blob: &|_| Ok(String::new()),
        check_emit: &rigger::contextgraph::check_emit,
        spawn_host: test_spawn_host(),
        code_store: None,
        context: None,
    };
    let rs = run(&cfg, &deps).unwrap();

    // Exactly 4 units ran: "plan", "checkin", the superseding unit and criterion B's ordinary
    // baseline. A 5th would be criterion A's own baseline surviving beside the unit that
    // superseded it.
    assert_eq!(
        rs.units.len(),
        4,
        "plan + checkin + exactly 2 implement-derived units (the superseding unit for \
         criterion A, the ordinary baseline for criterion B); units: {:?}",
        rs.units.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        rs.units[superseding_id].status,
        ledger::Status::Integrated,
        "the planner-proposed unit superseding criterion A's baseline integrates through a real \
         git merge"
    );

    // Criterion B's ordinary baseline, found by elimination: its deterministic slug derives
    // from criterion text this test does not hand-compute.
    let crit_b_baseline = rs
        .units
        .keys()
        .find(|k| k.as_str() != "plan" && k.as_str() != "checkin" && k.as_str() != superseding_id)
        .cloned()
        .expect("criterion B's ordinary baseline exists as a 4th unit");
    assert_eq!(
        rs.units[&crit_b_baseline].status,
        ledger::Status::Integrated,
        "criterion B's ordinary baseline integrates through a real git merge"
    );

    // checkin needs the fan-out template, whose live owner for criterion A is the superseding
    // unit: a frozen unit-id snapshot could never see it integrate, and the run would converge
    // one wave early with no checkin at all.
    assert_eq!(
        rs.units.get("checkin").map(|u| u.status),
        Some(ledger::Status::Integrated),
        "checkin becomes ready and integrates once every criterion's live owner has integrated \
         through a real git merge; units: {:?}",
        rs.units.keys().collect::<Vec<_>>()
    );

    // The base repo's working tree carries every landed unit's committed file after the real
    // merges, not merely an in-memory status transition.
    for name in [superseding_id, crit_b_baseline.as_str(), "checkin"] {
        let file_name = format!(
            "{}.rs",
            name.replace(|c: char| !c.is_ascii_alphanumeric(), "_")
        );
        let landed = repo.path().join(&file_name);
        assert!(
            landed.exists(),
            "unit {name:?}'s committed file lands on the base after its merge: {}",
            landed.display()
        );
    }
}

// -----------------------------------------------------------------------------------------
// Rule 2: a stage's own `max_retries:` key, parsed from `workflow.yml` text, overrides
// `defaults.max_retries` for that stage's units through a run's remediation loop, in both
// directions.
// -----------------------------------------------------------------------------------------

/// Given a repo-less `solo` stage whose gate always fails, under `defaults.max_retries:
/// {default}` and the stage's own `max_retries: {stage}`, when `rigger run` drives it, then the
/// unit is launched and fails exactly `stage` attempts, each a fresh implementer, and escalates
/// after the last: the stage's key governs, never the run-wide default.
fn a_stages_own_max_retries_bounds_the_attempts(default: u32, stage: u32) {
    let dir = temp_repoless_project();
    let root = dir.path();
    write_worker_agent(root, REPOLESS_WORKER);
    std::fs::write(
        root.join(".rigger").join("workflow.yml"),
        format!(
            r#"defaults:
  grounder: nop
  budget: 60
  max_retries: {default}
gates:
  bad: {{ run: "false", kind: core }}
stages:
  solo:
    agent: worker
    gates: [bad]
    on_pass: none
    max_retries: {stage}
"#
        ),
    )
    .unwrap();

    let arm = format!("  *\"Do the unit.\"*)\n{RECORD_EACH_LAUNCH}    echo attempted\n    ;;\n");
    let run = run_over(root, &["run"], &arm);
    assert!(
        run.ok && run.shows("solo", "escalated"),
        "the run escalates the unit and ends cleanly; stdout:\n{}\nstderr:\n{}",
        run.out,
        run.err
    );
    let attempts: Vec<String> = (0..stage)
        .map(|attempt| format!("solo/implementer#{attempt}"))
        .collect();
    assert_eq!(
        run.launched, attempts,
        "with the stage's own max_retries: {stage} in force, every failure but the last \
         relaunches a fresh attempt and the last escalates; the run-wide default of {default} \
         does not govern this stage"
    );
    let failed = read_run_events(root)
        .iter()
        .filter(|e| e.type_ == "UnitFailed")
        .count();
    assert_eq!(
        failed, stage as usize,
        "the unit fails exactly {stage} attempts before it escalates"
    );
}

rigger::test_cases! {
    /// `defaults.max_retries: 5` but the stage sets `max_retries: 1`: the unit escalates on its
    /// first failed attempt, never surviving to a second.
    a_stages_own_max_retries_yaml_key_lowers_the_effective_bound_below_a_higher_default:
        a_stages_own_max_retries_bounds_the_attempts(5, 1);
    /// `defaults.max_retries: 1` (which alone escalates on the first failed attempt) but the
    /// stage sets `max_retries: 3`: the unit survives two failed attempts and escalates on the
    /// third.
    a_stages_own_max_retries_yaml_key_raises_the_effective_bound_above_a_lower_default:
        a_stages_own_max_retries_bounds_the_attempts(1, 3);
}
