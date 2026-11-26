//! THE CONDUCTOR RESOLVES EVERY SPAWN ONCE (spec 135), at the seam between the conductor and the
//! one driver that records no spawn request, the real `driver::cli::Driver`
//! (`tests/common/real_driver_spy.rs`). Under it a speculation group re-entered past a boundary
//! runs its lanes from attempt 0 again, so a record of the group keyed by the lane alone would
//! repeat the earlier group's key and never be written. Each row drives `conductor::run` through
//! that driver, a real agent process for every spawn and the real gate runner, over the project's
//! on-disk store reopened for every run, and pins the replay keys of `s`'s `UnitFailed` records in
//! log order: each group records its own lane once, keyed by the re-entry boundary it entered at.

use std::path::Path;

use crate::common;
use crate::real_driver_spy::RealDriverSpy;
use crate::real_gate_run::write_agent_running;
use common::cli::{run_rigger, seed_store, with_run_store};
use common::fixtures::{
    agent, branch_of, failed_keys_beside, gate_def, run_log, test_spawn_host, the_compensation,
    the_grant, TWO_LOST_GROUPS_SPLIT_BY_A_GRANT, TWO_WINNING_GROUPS_SPLIT_BY_A_COMPENSATION,
};
use common::git::temp_git_project_with_commit;
use rigger::conductor::{run, Deps};
use rigger::config::{Config, ReviewPanel, Stage};
use rigger::eventstore::Event;
use rigger::gate::ExecRunner;
use rigger::ledger::Status;

/// The verdict line under which every lane of `s` loses its review.
const REJECT: &str = r#"{"verdict":"reject","issues":["every lane of s is rejected"]}"#;
/// The verdict line under which every lane of `s` is approved.
const APPROVE: &str = r#"{"verdict":"approve"}"#;

/// The gate every stage runs, before its merge and again on the merged tree: red only for a lane
/// 0 of `s` whose base moved after it branched, which lane 0's `s-lane-0.txt` (the base's
/// `advance.txt` count it saw) and the tree's own count tell apart. So lane 0 passes in its own
/// worktree and breaks the tree its merge makes once lane 1 has advanced the base.
const GATE: &str = r#"[ ! -f s-lane-0.txt ] || [ "$(cat s-lane-0.txt)" = "$(cat advance.txt 2>/dev/null || echo 0)" ]"#;

/// `s`, a speculation group of two lanes, and `t`, which needs `s`: each lands on merge, runs
/// [`GATE`] and is judged by one adjudicator.
fn two_stage_workflow() -> Config {
    let mut cfg = Config::default();
    cfg.agents.insert("worker".into(), agent("worker"));
    cfg.agents.insert("judge".into(), agent("judge"));
    cfg.workflow.gates.insert("ok".into(), gate_def(GATE));
    for (name, speculation_width, needs) in [("s", 2, Vec::new()), ("t", 0, vec!["s".into()])] {
        cfg.workflow.stages.insert(
            name.into(),
            Stage {
                name: name.into(),
                agent: "worker".into(),
                needs,
                gates: vec!["ok".into()],
                on_pass: "merge".into(),
                speculation_width,
                review: ReviewPanel {
                    adjudicator: "judge".into(),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
    }
    cfg
}

/// The agent every spawn of the project runs, by the spawn id the driver hands it: it logs the
/// id to `spawned`; lane 0 of `s` writes the base count it branched at; lane 1 of `s` writes its
/// own file and advances `root`'s base with a commit, so lane 0's merge is no fast-forward whose
/// tree its own gate verdict answers; `t`'s implementer writes its file and `t`'s adjudicator
/// approves, naming `s` for a compensation; every other spawn, `s`'s adjudicator, gives `verdict`.
fn agent_body(root: &Path, spawned: &Path, verdict: &str) -> String {
    let root = root.display();
    format!(
        r#"echo "$RIGGER_SPAWN" >> '{spawned}'
case "$RIGGER_SPAWN" in
  s/implementer#0)
    {{ cat advance.txt 2>/dev/null || echo 0; }} > s-lane-0.txt
    ;;
  s/implementer#1)
    echo 1 > s-lane-1.txt
    n=$(cat '{root}/advance.txt' 2>/dev/null || echo 0)
    echo $((n + 1)) > '{root}/advance.txt'
    git -C '{root}' add advance.txt
    git -C '{root}' commit -q -m 'advance the base'
    ;;
  t/implementer#*)
    echo t > t.txt
    ;;
  t/*)
    echo '{{"verdict":"approve","compensate":"s"}}'
    ;;
  *)
    echo '{verdict}'
    ;;
esac"#,
        spawned = spawned.display(),
    )
}

/// One run of the project at `root` through the real driver spawning `agent`, over `root`'s
/// on-disk store opened afresh, as a new process opens it; returns the status `s` settles at.
fn run_the_project(root: &Path, cfg: &Config, agent: &Path) -> Status {
    let repo = root.to_str().unwrap().to_string();
    let driver = RealDriverSpy::new(agent);
    with_run_store(root, |store| {
        let deps = Deps {
            store,
            driver: &driver,
            gates: &ExecRunner,
            run_branch: branch_of(&repo),
            repo: repo.clone(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
            log: &|_| {},
            check_emit: &rigger::contextgraph::check_emit,
            spawn_host: test_spawn_host(),
            code_store: None,
            context: None,
        };
        let rs = run(cfg, &deps).unwrap_or_else(|e| panic!("the run must settle: {e}"));
        rs.units["s"].status
    })
}

/// The operator grants the escalated `s` one attempt through `rigger resume-unit`.
fn grant_s(root: &Path) {
    let (out, err, ok) = run_rigger(root, &["resume-unit", "s", "--attempts", "1"]);
    assert!(ok, "the grant must succeed; stdout: {out} stderr: {err}");
}

/// One row: how `s`'s lanes are judged, what the operator does between two runs, if anything, and
/// what the log then holds.
struct EachGroupOnce {
    /// The verdict line `s`'s adjudicator gives every lane.
    verdict: &'static str,
    /// The operator's act between a first and a second run, or `None` for one run.
    between: Option<fn(&Path)>,
    /// The status the last group of `s` settles `s` at.
    settled: Status,
    /// Picks the one re-entry boundary the log holds, whose position fills `{entry}`.
    boundary: fn(&Event) -> bool,
    /// The replay keys of `s`'s `UnitFailed` records, in log order, `""` for an unkeyed one.
    records: &'static [&'static str],
}

/// Given the two-stage project whose every spawn runs a real agent process through the real
/// driver, when it runs (twice, around the operator's act, when the row has one), then the driver
/// ran lanes 0 and 1 of `s` in each of its two groups, the last group settles `s` as the row says,
/// and `s`'s `UnitFailed` records carry exactly the row's replay keys: each group's record once,
/// keyed by the boundary it entered at.
fn assert_each_group_records_its_own_lane_once(row: EachGroupOnce) {
    let project = temp_git_project_with_commit();
    let root = project.path();
    seed_store(root);
    let host = tempfile::tempdir().unwrap();
    let spawned = host.path().join("spawned");
    let agent = write_agent_running(host.path(), &agent_body(root, &spawned, row.verdict));
    let cfg = two_stage_workflow();

    let mut settled = run_the_project(root, &cfg, &agent);
    if let Some(between) = row.between {
        between(root);
        settled = run_the_project(root, &cfg, &agent);
    }

    let log = std::fs::read_to_string(&spawned).unwrap();
    let lanes: Vec<&str> = log
        .lines()
        .filter(|id| id.starts_with("s/implementer#"))
        .collect();
    assert_eq!(
        (lanes, settled),
        (
            vec![
                "s/implementer#0",
                "s/implementer#1",
                "s/implementer#0",
                "s/implementer#1"
            ],
            row.settled
        ),
        "premise: the driver records no spawn request, so each group of s runs lanes 0 and 1, and \
         the last group settles s; every spawn: {log}"
    );
    let events = with_run_store(root, run_log);
    let (recorded, expected, entry) = failed_keys_beside(&events, Some(row.boundary), row.records);
    assert_eq!(
        recorded, expected,
        "s's UnitFailed records, by replay key, the boundary at {entry:?}"
    );
}

rigger::test_cases! {
    /// Two lost groups of `s`, every lane review-rejected, split by the operator's grant: each
    /// group's lost record is written, the second keyed by the grant.
    two_lost_groups_split_by_a_grant_each_record_once: assert_each_group_records_its_own_lane_once(
        EachGroupOnce {
            verdict: REJECT,
            between: Some(grant_s),
            settled: Status::Escalated,
            boundary: the_grant,
            records: TWO_LOST_GROUPS_SPLIT_BY_A_GRANT,
        },
    );
    /// Two winning groups of `s` in one run, split by the compensation `t`'s review queues, lane 0
    /// of each breaking the tree its merge makes: each group's broken lane is recorded, the
    /// second keyed by the compensation.
    two_winning_groups_split_by_a_compensation_each_record_their_broken_lane:
        assert_each_group_records_its_own_lane_once(EachGroupOnce {
            verdict: APPROVE,
            between: None,
            settled: Status::Integrated,
            boundary: the_compensation,
            records: TWO_WINNING_GROUPS_SPLIT_BY_A_COMPENSATION,
        });
}
