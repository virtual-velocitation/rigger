//! Periphery (integration) tests for spec 88, criterion 2 (ADOPTION KEYS ON THE CRITERION):
//! `RunCtx::adopt_prior_criterion_branch` (src/conductor.rs:8124) and `prior_criterion_unit`
//! (src/conductor.rs:9814) - a fresh run's planner-proposed unit, naming a DIFFERENT slug
//! than a prior run's, still continues that prior unit's un-integrated work because the two
//! units' `criterion_id`s (`criterion_stable_id`, src/conductor.rs:8735 - a position plus a
//! content hash of the criterion text) match.
//!
//! WHAT THE INSIDE-OUT TESTS ARE STRUCTURALLY BLIND TO.
//!
//! The implementer's own `mod tests` (src/conductor.rs) proves the DECISION logic
//! thoroughly: `prior_criterion_unit_finds_a_prior_un_integrated_units_id` (10607),
//! `..._never_returns_an_integrated_units_id...` (10618), `..._tie_break_prefers_the_most_
//! recent...` (10647), `..._excludes_this_unit_itself` (10663), and `..._ignores_a_different_
//! criterion_and_an_empty_one` (10671) all call the private `prior_criterion_unit` function
//! DIRECTLY with hand-built `Event`s. Two further tests, `a_fresh_units_own_branch_adopts_a_
//! prior_runs_un_integrated_unit_sharing_the_criterion` (10703) and `a_fresh_unit_never_
//! adopts_a_criterion_whose_prior_attempt_already_integrated` (10791), go one step further
//! and drive the public `run()` entry itself - but from INSIDE the crate, where `Stage`'s
//! `criterion_id` field (`#[serde(skip)]`, set ONLY by production code: `baseline_units`
//! (10270) or `resolve_served_criterion` (8650) inside `harvest_proposed`) can be poked
//! directly. Both of those tests hand-construct `cfg.workflow.stages` with a literal
//! `Stage { criterion_id: cid.into(), .. }`, using the SAME string literal `cid` for both
//! the prior run's event and the fresh run's stage, and pass `criteria: Vec::new()` in
//! `Deps` - so `baseline_units`/`resolve_served_criterion` never run at all.
//!
//! That proves the decision logic is correct GIVEN two already-equal ids. It proves nothing
//! about spec 88's actual, load-bearing claim - "regardless of the planner's slug" - which
//! depends on `criterion_stable_id` computing the IDENTICAL id for the SAME criterion text
//! from TWO INDEPENDENT PRODUCTION CALL SITES, run-apart and callsite-apart: the prior run's
//! deterministic BASELINE synthesis (`baseline_units`, called once per run before any agent
//! spawns) and the fresh run's PLANNER-PROPOSAL resolution (`resolve_served_criterion`,
//! called per `UnitProposed` inside `harvest_proposed`). A drift between those two call
//! sites - a normalization difference, a position off-by-one, anything that made them hash
//! the same text to different ids - would leave every hand-typed-`cid` test above green while
//! the real feature silently never matched a single real criterion in production. Neither
//! layer above can see that: both bypass `criterion_stable_id` entirely.
//!
//! So these two tests drive the SAME real production write path spec 72's periphery layer
//! established for this identical class of gap (`tests/replan_episode_identity.rs`):
//! TWO real `conductor::run` calls sharing one store and one real git repo, `deps.criteria`
//! populated with real criterion text (never a hand-set `Stage.criterion_id`), the "prior
//! run" reaching its unit via the deterministic baseline path and the "fresh run" reaching
//! its DIFFERENTLY-NAMED unit via a real planner `UnitProposed` citing the SAME criterion
//! text (matched through the whitespace-normalized prose fallback `resolve_served_criterion`
//! falls back to, exactly like a real planner that never echoes the id). The `criterion_id`
//! values asserted equal below are both read back off the real events `store.read_stream`
//! returns - never asserted by construction.
//!
//! Test 2 also folds in the "event type / serialized form" probe's back-compat half: a
//! hand-authored `UnitStarted` predating spec 88 (no `criterion_id` field at all, mirroring
//! a pre-upgrade binary's leftover history) is appended into the SAME log the fresh run's
//! whole-stream fold walks, proving the mixed-log real-upgrade shape - not just that the
//! isolated decode doesn't error - never derails the correct verdict.

use std::path::Path;
use std::process::Command;

use rigger::conductor::{
    run, AgentDriver, AgentResult, Deps, Error, SpawnOpts, STREAM, TYPE_UNIT_PROPOSED,
};
use rigger::config::{AgentDef, Config, Gate, Stage};
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, Event, EventStore};
use rigger::gate::ExecRunner;
use rigger::ledger;
use rigger::run::start_fresh;
use serde_json::{json, Value};

/// A bare git repo with one empty commit, so `HEAD` resolves for `Worktree::create`'s
/// branch-from-HEAD path. Mirrors `tests/worktree_liveness_fence_periphery.rs`'s identical
/// helper.
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
}

/// A single-role driver: the implementer writes `file_name` (with `content`) into its
/// worktree on every spawn. Used for the PRIOR run's deterministic-baseline unit - it
/// writes real, committable content regardless of how many remediation attempts the
/// always-failing gate below forces, so the branch carries real work even though the unit
/// never integrates.
struct WritesFileDriver {
    file_name: String,
    content: String,
}

impl AgentDriver for WritesFileDriver {
    fn spawn(
        &self,
        _agent: &AgentDef,
        _prompt: &str,
        opts: &SpawnOpts,
        _emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        if !opts.dir.is_empty() {
            std::fs::write(format!("{}/{}", opts.dir, self.file_name), &self.content).unwrap();
        }
        Ok(AgentResult {
            output: "ok".into(),
            resolved_model: String::new(),
        })
    }
}

/// A three-role driver for the FRESH run: `planner` proposes ONE unit under `proposed_id`
/// citing `criterion` (the PLAN_PROTOCOL `criterion` key - `UnitProposed::coverage`'s alias,
/// resolved against `deps.criteria` by `resolve_served_criterion`'s real production code,
/// never a hand-set `criterion_id`); `judge` (the plan-critique adjudicator) approves the
/// DAG immediately; the proposed unit's own implementer (`worker`) optionally writes
/// `worker_write` into its worktree.
struct ProposesSlugDriver {
    proposed_id: String,
    criterion: String,
    worker_write: Option<(String, String)>,
}

impl AgentDriver for ProposesSlugDriver {
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
                }),
            )?;
            return Ok(AgentResult {
                output: "proposed the DAG".into(),
                resolved_model: String::new(),
            });
        }
        if agent.id == "judge" {
            return Ok(AgentResult {
                output: r#"{"verdict":"approve"}"#.into(),
                resolved_model: String::new(),
            });
        }
        if let Some((name, content)) = &self.worker_write {
            if !opts.dir.is_empty() {
                std::fs::write(format!("{}/{}", opts.dir, name), content).unwrap();
            }
        }
        Ok(AgentResult {
            output: "ok".into(),
            resolved_model: String::new(),
        })
    }
}

/// A single-criterion, no-planner workflow whose lone fan-out `implement-template` stage
/// (`fan_out_template_name`'s match: non-empty `agent`, `strategy: "fan-out"`, empty
/// `produces`) gets replaced by ONE deterministic baseline unit for the criterion
/// `deps.criteria` carries (`baseline_units`, src/conductor.rs:10270) - the REAL production
/// path that stamps `criterion_id: criterion_stable_id(1, criterion)` on the synthesized
/// `Stage`, never a hand-set field.
fn baseline_only_cfg(gate_run: &str, max_retries: u32) -> Config {
    let mut cfg = Config::default();
    cfg.agents.insert(
        "worker".into(),
        AgentDef {
            id: "worker".into(),
            ..Default::default()
        },
    );
    cfg.workflow.gates.insert(
        "gate".into(),
        Gate {
            run: gate_run.into(),
            kind: "core".into(),
            inputs: Vec::new(),
        },
    );
    cfg.workflow.stages.insert(
        "implement-template".into(),
        Stage {
            name: "implement-template".into(),
            agent: "worker".into(),
            strategy: "fan-out".into(),
            gates: vec!["gate".into()],
            on_pass: "merge".into(),
            ..Default::default()
        },
    );
    cfg.workflow.defaults.max_retries = max_retries;
    cfg
}

/// A `plan -> plan-critique -> implement` workflow (mirrors `tests/replan_episode_identity.
/// rs`'s `two_episode_cfg`): the planner's `UnitProposed` is harvested by the REAL
/// `harvest_proposed` (src/conductor.rs), which resolves its criterion via
/// `resolve_served_criterion` (8650) and, on a match, supersedes the deterministic baseline
/// `deps.criteria` would otherwise have synthesized for the same criterion - so the
/// planner-proposed unit alone survives to run, carrying the SAME `criterion_stable_id` a
/// completely separate call site computed.
fn fresh_run_cfg(gate_run: &str) -> Config {
    let mut cfg = Config::default();
    for id in ["planner", "judge", "worker"] {
        cfg.agents.insert(
            id.into(),
            AgentDef {
                id: id.into(),
                ..Default::default()
            },
        );
    }
    cfg.workflow.gates.insert(
        "gate".into(),
        Gate {
            run: gate_run.into(),
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
        "plan-critique".into(),
        Stage {
            name: "plan-critique".into(),
            needs: vec!["plan".into()],
            adjudicator: "judge".into(),
            ..Default::default()
        },
    );
    cfg.workflow.stages.insert(
        "implement-template".into(),
        Stage {
            name: "implement-template".into(),
            agent: "worker".into(),
            strategy: "fan-out".into(),
            needs: vec!["plan-critique".into()],
            gates: vec!["gate".into()],
            on_pass: "merge".into(),
            ..Default::default()
        },
    );
    cfg
}

/// The `UnitStarted` event body for `id`, read back off the real store - mirrors the
/// implementer's own `find_unit_started` (src/conductor.rs:10703's file, an internal test
/// helper unreachable from here), rewritten locally since a periphery test can only ever
/// see this through the SAME public read path any other consumer (a resumed process,
/// `rigger status`) would use.
fn find_unit_started(events: &[Event], id: &str) -> Value {
    for e in events {
        if e.type_ != ledger::TYPE_UNIT_STARTED {
            continue;
        }
        let Ok(body) = serde_json::from_slice::<Value>(&e.data) else {
            continue;
        };
        if body.get("id").and_then(Value::as_str) == Some(id) {
            return body;
        }
    }
    panic!(
        "no UnitStarted recorded for unit {id:?} among {} events",
        events.len()
    );
}

/// Criterion 2's primary Done-when proof: "a fresh run whose planner proposes a new slug
/// for a criterion a prior run left un-integrated starts that unit at the prior tip with
/// `adopted_from` recorded" - through the REAL production id computation, not a hand-typed
/// match.
#[test]
fn a_fresh_runs_differently_named_planner_proposal_adopts_a_prior_runs_escalated_baseline_via_real_criterion_id_matching(
) {
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let store = Store::open(":memory:").unwrap();
    let criterion = "the widget survives a restart";

    // PRIOR RUN: the deterministic baseline unit for `criterion` spawns, commits a real
    // file (the pre-gate commit fires on every attempt, before the gate ever runs - src/
    // conductor.rs:4445), then its gate ALWAYS fails - one remediation attempt
    // (`max_retries: 1`) then escalate, so the loop terminates cheaply without ever
    // integrating.
    let driver1 = WritesFileDriver {
        file_name: "prior-work.txt".into(),
        content: "escalated attempt\n".into(),
    };
    let deps1 = Deps {
        store: &store,
        driver: &driver1,
        gates: &ExecRunner,
        repo: repo.path().to_str().unwrap().to_string(),
        grounder: None,
        graph: None,
        criteria: vec![criterion.to_string()],
    };
    let rs1 = run(&baseline_only_cfg("false", 1), &deps1).unwrap();
    assert_eq!(
        rs1.units.len(),
        1,
        "exactly one baseline unit for the one criterion; units: {:?}",
        rs1.units.keys().collect::<Vec<_>>()
    );
    let prior_slug = rs1.units.keys().next().unwrap().clone();
    assert_eq!(
        rs1.units[&prior_slug].status,
        ledger::Status::Escalated,
        "an always-failing gate must exhaust remediation and escalate, never integrate"
    );
    let prior_branch = format!("rigger/u/{prior_slug}");
    let prior_tip = git_out(repo.path(), &["rev-parse", &prior_branch])
        .expect("the escalated unit's durable branch must exist with a resolvable tip");
    assert_eq!(prior_tip.len(), 40, "a real git commit sha: {prior_tip}");
    let shown = git_out(
        repo.path(),
        &["show", &format!("{prior_branch}:prior-work.txt")],
    )
    .expect("the prior branch must carry the real committed file, despite never integrating");
    assert_eq!(shown.trim(), "escalated attempt");
    assert!(
        !repo.path().join("prior-work.txt").exists(),
        "the escalated unit never integrated, so its file must NOT be on the base yet"
    );

    // FRESH RUN: a new run boundary in the SAME log, the SAME criterion text, and a
    // planner that proposes a unit under a DELIBERATELY DIFFERENT id - never the
    // deterministic baseline slug (`prior_slug` above), so the ONLY way this unit can
    // start from the prior tip is the real `criterion_id` match, not ordinary same-name
    // branch continuity.
    start_fresh(&store, &[criterion.to_string()], "", "", "").unwrap();
    let fresh_slug = "totally-differently-named-unit";
    assert_ne!(
        fresh_slug, prior_slug,
        "the fresh run's unit must be a genuinely different slug from the prior run's"
    );
    let driver2 = ProposesSlugDriver {
        proposed_id: fresh_slug.to_string(),
        criterion: criterion.to_string(),
        worker_write: None,
    };
    let deps2 = Deps {
        store: &store,
        driver: &driver2,
        gates: &ExecRunner,
        repo: repo.path().to_str().unwrap().to_string(),
        grounder: None,
        graph: None,
        criteria: vec![criterion.to_string()],
    };
    let rs2 = run(&fresh_run_cfg("true"), &deps2).unwrap();

    assert_eq!(
        rs2.units[fresh_slug].status,
        ledger::Status::Integrated,
        "the adopted unit still runs its ordinary lifecycle through to integration; units: {:?}",
        rs2.units.keys().collect::<Vec<_>>()
    );
    assert!(
        repo.path().join("prior-work.txt").exists(),
        "the escalated prior run's committed work must ride the adoption all the way into the base"
    );

    let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    let prior_started = find_unit_started(&events, &prior_slug);
    let fresh_started = find_unit_started(&events, fresh_slug);

    // The core proof: TWO INDEPENDENT production call sites (`baseline_units`'s stamp for
    // the prior run; `resolve_served_criterion` inside `harvest_proposed` for the fresh
    // run) computed the SAME `criterion_stable_id` for the SAME criterion text. Neither
    // value was ever hand-typed by this test.
    let prior_cid = prior_started["criterion_id"]
        .as_str()
        .expect("the prior unit's own UnitStarted carries its criterion_id");
    let fresh_cid = fresh_started["criterion_id"]
        .as_str()
        .expect("the fresh unit's own UnitStarted carries its criterion_id");
    assert!(
        !fresh_cid.is_empty(),
        "the fresh unit's criterion_id must be resolved, not left empty"
    );
    assert_eq!(
        fresh_cid, prior_cid,
        "two independent production call sites must compute the identical criterion_stable_id \
         for the same criterion text - this is the real mechanism 'regardless of the planner's \
         slug' depends on, never a hand-typed match; prior UnitStarted: {prior_started}, fresh \
         UnitStarted: {fresh_started}"
    );

    assert_eq!(
        fresh_started["adopted_from"]["unit"], prior_slug,
        "UnitStarted must record which prior unit this one adopted: {fresh_started}"
    );
    let adopted_tip = fresh_started["adopted_from"]["tip"]
        .as_str()
        .expect("adopted_from.tip must be a string");
    assert_eq!(
        adopted_tip.len(),
        40,
        "adopted_from.tip must be a real 40-hex-char commit sha: {fresh_started}"
    );
    assert_eq!(
        adopted_tip, prior_tip,
        "adopted_from.tip must be the prior branch's ACTUAL tip sha, read back off real git \
         (never re-derived by this test)"
    );
}

/// Criterion 2's second Done-when clause: "never adopts an integrated unit." Also closes
/// the "event type / serialized form" back-compat probe: a hand-authored `UnitStarted`
/// predating spec 88 (no `criterion_id` field at all - the exact shape a pre-upgrade
/// binary's history carries) sits in the SAME log the fresh run's whole-stream fold walks,
/// proving the mixed-log real-upgrade shape never derails the correct verdict.
#[test]
fn a_fresh_runs_differently_named_planner_proposal_never_adopts_a_criterion_whose_baseline_already_integrated_and_ignores_legacy_history(
) {
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let store = Store::open(":memory:").unwrap();
    let criterion = "the gadget reports its own health";

    // PRIOR RUN: the deterministic baseline unit's gate passes immediately, so it
    // integrates - its work is already on the base.
    let driver1 = WritesFileDriver {
        file_name: "prior-integrated-work.txt".into(),
        content: "landed attempt\n".into(),
    };
    let deps1 = Deps {
        store: &store,
        driver: &driver1,
        gates: &ExecRunner,
        repo: repo.path().to_str().unwrap().to_string(),
        grounder: None,
        graph: None,
        criteria: vec![criterion.to_string()],
    };
    let rs1 = run(&baseline_only_cfg("true", 3), &deps1).unwrap();
    let prior_slug = rs1.units.keys().next().unwrap().clone();
    assert_eq!(rs1.units[&prior_slug].status, ledger::Status::Integrated);
    assert!(repo.path().join("prior-integrated-work.txt").exists());

    // A hand-authored UnitStarted predating spec 88: no `criterion_id` field at all - the
    // one shape the CURRENT write path can never produce (every real UnitStarted since
    // spec 88 stamps it, even as ""), simulating a pre-upgrade binary's leftover history
    // sitting in the same log a fresh process's whole-stream fold must walk.
    store
        .append(
            STREAM,
            rigger::eventstore::ExpectedRevision::Any,
            &[Event::new(
                ledger::TYPE_UNIT_STARTED,
                serde_json::to_vec(&json!({"id": "legacy-unit-predating-spec-88"})).unwrap(),
            )],
        )
        .unwrap();

    // FRESH RUN: same criterion, a differently-named planner proposal again.
    start_fresh(&store, &[criterion.to_string()], "", "", "").unwrap();
    let fresh_slug = "another-fresh-planner-proposal";
    let driver2 = ProposesSlugDriver {
        proposed_id: fresh_slug.to_string(),
        criterion: criterion.to_string(),
        worker_write: Some(("second-work.txt".into(), "genuinely fresh\n".into())),
    };
    let deps2 = Deps {
        store: &store,
        driver: &driver2,
        gates: &ExecRunner,
        repo: repo.path().to_str().unwrap().to_string(),
        grounder: None,
        graph: None,
        criteria: vec![criterion.to_string()],
    };
    let rs2 = run(&fresh_run_cfg("true"), &deps2).unwrap();

    assert_eq!(
        rs2.units[fresh_slug].status,
        ledger::Status::Integrated,
        "a criterion whose prior attempt already integrated still runs its unit normally, \
         genuinely fresh (never blocked by the presence of an integrated prior, or by the \
         unrelated legacy event); units: {:?}",
        rs2.units.keys().collect::<Vec<_>>()
    );
    assert!(
        repo.path().join("second-work.txt").exists(),
        "the fresh unit's own real work must land on the base"
    );

    let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    let fresh_started = find_unit_started(&events, fresh_slug);
    assert_eq!(
        fresh_started["adopted_from"],
        Value::Null,
        "a criterion whose prior owner already integrated must adopt nothing - even though \
         production computed the SAME criterion_id (the identical mechanism test 1 confirms), \
         the integrated-exclusion correctly refuses adoption: {fresh_started}"
    );
    assert_eq!(
        fresh_started["criterion_id"].as_str().map(str::is_empty),
        Some(false),
        "the fresh unit still gets a real, resolved criterion_id of its own even when nothing \
         is adopted: {fresh_started}"
    );
}
