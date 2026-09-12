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
//!
//! Test 3 covers the "cross-module seam / fold arm" probe on `prior_criterion_unit`'s round-2
//! fix (adv-u88c2-integrated-set-keyed-by-id-not-criterion-masks-unrelated-prior): the
//! integrated-exclusion is keyed on `(id, criterion_id)`, never bare id, because a planner
//! slug carries no cross-run uniqueness guarantee (`harvest_proposed`'s `stages.contains_key`
//! check is scoped to the CURRENT run's own DAG only). The fix's own `mod tests` entry
//! (`prior_criterion_unit_integration_of_one_criterion_never_masks_an_abandoned_sibling_
//! criterion_sharing_the_same_id`, src/conductor.rs) proves the fold arm correct against
//! hand-built events, exactly the class of test tests 1 and 2's header above explains is
//! structurally blind to a drift between the two independent `criterion_stable_id` call
//! sites. Test 3 drives the identical scenario through THREE real `conductor::run` calls
//! sharing one store and one real git repo - a literal id reused across two runs for two
//! different real criterion texts - so the fix is proven at the boundary the bug actually
//! threatens, not only inside the private function's own unit test.
//!
//! ROUND 3 (tests 4-6): `prior_criterion_unit` grew two more production behaviors, each
//! closing an upheld round-2 review finding:
//!
//! - SPEC-SCOPED (`adv-u88c2-r2-criterion-id-unscoped-crosses-specs`): `criterion_id`
//!   (`criterion_stable_id`) is position plus a content hash of the criterion TEXT alone -
//!   no spec identity folded in. Two UNRELATED specs whose criterion at the same position
//!   is byte-for-byte identical text mint the SAME id (this repo's own corpus proves it
//!   happens: several specs share boilerplate Done-when text). Fixed: a candidate only
//!   counts when its own `RunStarted.spec`, stemmed through `ledger::spec_stem`, matches
//!   the current unit's owning spec (the LAST `RunStarted` in the whole stream).
//! - TEMPORAL (`adv-u88c2-r2-exclusion-set-permanent-blocks-a-genuine-redo-after-
//!   integration`): the integrated-exclusion set only ever grew - once a unit integrated
//!   for a criterion it was barred from adoption FOREVER, even after a later compensation
//!   (spec 12, unit 4 - a `UnitFailed` carrying `META_COMPENSATED`) genuinely reverts that
//!   integration. Fixed: a compensation walked for an id clears its exclusion triple.
//!
//! Test 4 proves spec-scoping at the real boundary AND proves STEMMED-identity matching
//! rather than raw path equality: its two same-spec runs spell that spec's path two
//! different ways (`ledger::spec_stem` is now `pub(crate)` specifically so
//! `prior_criterion_unit` and `pr_head_branch` share ONE canonical derivation - a test
//! that reused one literal path string twice could pass even if the two callers silently
//! used different derivations, exactly the drift class this file's own header explains
//! the inside-out tests are structurally blind to).
//!
//! Tests 5 and 6 prove the temporal fix against a REAL git branch, not a hand-built
//! `Event` list. `drain_compensations` (src/conductor.rs) - the mechanism that actually
//! PRODUCES a `META_COMPENSATED` `UnitFailed`, via a later unit's review naming an
//! earlier integrated unit as a rollback target - is spec 12's own, UNCHANGED-by-this-
//! diff machinery; driving its full adjudicator-names-a-target trigger is out of this
//! unit's blast radius (only `prior_criterion_unit`'s READING of its output is new this
//! round). So the shared setup below reproduces the exact SHAPE `drain_compensations`
//! appends directly, while building the reverted unit's real post-compensation branch
//! through the SAME real `Worktree::create` production call a re-entered remediation
//! round would use once its branch is reclaimed - proving the full real chain
//! (`prior_criterion_unit` unblocking the candidate AND `Worktree::create_branch_at`
//! actually finding a real branch to adopt), which the implementer's own round-3 `mod
//! tests` (hand-built events, no real git) cannot see: an integrated unit's durable
//! branch is reclaimed by `gc_integrated_branches` (test 3 above proves this), so
//! whether a real branch ever again exists for the fix to adopt is exactly the kind of
//! boundary question a private function operating on a bare `Vec<Event>` never touches.

use std::path::Path;
use std::process::Command;

use rigger::conductor::{
    run, AgentDriver, AgentResult, Deps, Error, SpawnOpts, STREAM, TYPE_UNIT_PROPOSED,
};
use rigger::conductor::{META_COMPENSATED, META_CONTRADICTION};
use rigger::config::{AgentDef, Config, Gate, Stage};
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, Event, EventStore, ExpectedRevision};
use rigger::gate::ExecRunner;
use rigger::ledger;
use rigger::run::start_fresh;
use rigger::worktree::{self, Worktree};
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
/// never a hand-set `criterion_id`) and `gates` (`UnitProposed::gates` - a planner-proposed
/// unit's own gate list is read from THIS field, never inherited from the fan-out
/// template's `Stage.gates`, so a test that needs a real gate verdict must echo it here
/// exactly as a real planner echoes the workflow's gate names); `judge` (the plan-critique
/// adjudicator) approves the DAG immediately; the proposed unit's own implementer
/// (`worker`) optionally writes `worker_write` into its worktree.
struct ProposesSlugDriver {
    proposed_id: String,
    criterion: String,
    worker_write: Option<(String, String)>,
    gates: Vec<String>,
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
                    "gates": self.gates,
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
        gates: Vec::new(),
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
        gates: Vec::new(),
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

/// The LAST `UnitStarted` event body recorded for `id` - mirrors [`find_unit_started`],
/// which returns the FIRST. `shared_slug` below is deliberately reused across two runs and
/// so carries TWO `UnitStarted` events; this reads the one that matters for the property
/// under test (the id's CURRENT, still-live incarnation), never the stale first one.
fn find_last_unit_started(events: &[Event], id: &str) -> Value {
    let mut found: Option<Value> = None;
    for e in events {
        if e.type_ != ledger::TYPE_UNIT_STARTED {
            continue;
        }
        let Ok(body) = serde_json::from_slice::<Value>(&e.data) else {
            continue;
        };
        if body.get("id").and_then(Value::as_str) == Some(id) {
            found = Some(body);
        }
    }
    found.unwrap_or_else(|| panic!("no UnitStarted recorded for unit {id:?}"))
}

/// Regression test for the round-2 fix (adv-u88c2-integrated-set-keyed-by-id-not-criterion-
/// masks-unrelated-prior): a unit id reused across two SEPARATE runs for two DIFFERENT
/// criteria must have its EARLIER integration for one criterion never mask its LATER,
/// still-abandoned attempt at a DIFFERENT criterion sharing that id.
///
/// RUN 1: a planner proposes unit `shared_slug` for criterion B; its gate always passes, so
/// it integrates for real - and [`Self::gc_integrated_branches`] (src/conductor.rs:7041)
/// reclaims its now-merged durable branch in the SAME call, exactly as it reclaims every
/// integrated unit's branch. Asserted explicitly below: this is why RUN 2 reusing the same
/// id starts from a genuinely CLEAN branch, not a confound to route around.
///
/// RUN 2: a fresh run's planner reuses the SAME literal id `shared_slug` - deliberately,
/// mirroring a planner LLM's slug collision across independent runs, since nothing in
/// `harvest_proposed` enforces cross-run uniqueness - but for a DIFFERENT criterion A; its
/// gate always fails, so it escalates, abandoned, with real committed work on a FRESH
/// `rigger/u/shared-slug...` branch (the old ref is gone, so git creates a new one off
/// HEAD, same as a genuinely-fresh unit).
///
/// RUN 3: a THIRD, independently-named unit re-serves criterion A - the SAME text run 2
/// served, never integrated. Under the pre-fix bare-id keying, `shared_slug` having reached
/// `UnitIntegrated` in run 1 (for the unrelated criterion B) would wrongly exclude it from
/// candidacy, masking run 2's still-abandoned criterion-A work. Fixed: run 3 must still
/// adopt it, via real `criterion_stable_id` matching between run 2's and run 3's
/// independent production call sites - never a hand-typed id.
#[test]
fn a_units_integration_for_one_criterion_never_masks_a_later_runs_still_abandoned_attempt_at_a_different_criterion_sharing_the_same_id(
) {
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let store = Store::open(":memory:").unwrap();
    let criterion_a = "criterion A: the widget survives a restart";
    let criterion_b = "criterion B: the gadget reports its own health";
    let shared_slug = "shared-slug-reused-across-runs";
    let shared_branch = format!("rigger/u/{shared_slug}");

    // RUN 1: `shared_slug` serves criterion B; its gate always passes, so it integrates.
    let driver1 = ProposesSlugDriver {
        proposed_id: shared_slug.to_string(),
        criterion: criterion_b.to_string(),
        worker_write: Some(("run1-criterion-b-work.txt".into(), "run1 work\n".into())),
        gates: vec!["gate".to_string()],
    };
    let deps1 = Deps {
        store: &store,
        driver: &driver1,
        gates: &ExecRunner,
        repo: repo.path().to_str().unwrap().to_string(),
        grounder: None,
        graph: None,
        criteria: vec![criterion_b.to_string()],
    };
    let rs1 = run(&fresh_run_cfg("true"), &deps1).unwrap();
    assert_eq!(
        rs1.units[shared_slug].status,
        ledger::Status::Integrated,
        "shared_slug's first, criterion-B attempt must integrate normally"
    );
    assert!(repo.path().join("run1-criterion-b-work.txt").exists());
    // Asserted explicitly (see doc comment above): integration reclaims the branch in the
    // SAME call, so nothing survives here for run 2 to confound with.
    assert!(
        git_out(repo.path(), &["rev-parse", &shared_branch]).is_none(),
        "an integrated unit's durable branch is reclaimed by gc_integrated_branches - \
         shared_slug's run-1 branch must be gone before run 2 ever starts"
    );

    // RUN 2: a fresh run boundary, the SAME literal id reused for a DIFFERENT criterion;
    // its gate always fails, one remediation attempt (max_retries: 1) then escalate, so it
    // never integrates - a genuinely fresh branch (the old ref is gone) carries real work.
    start_fresh(&store, &[criterion_a.to_string()], "", "", "").unwrap();
    let driver2 = ProposesSlugDriver {
        proposed_id: shared_slug.to_string(),
        criterion: criterion_a.to_string(),
        worker_write: Some(("run2-criterion-a-work.txt".into(), "run2 work\n".into())),
        gates: vec!["gate".to_string()],
    };
    let deps2 = Deps {
        store: &store,
        driver: &driver2,
        gates: &ExecRunner,
        repo: repo.path().to_str().unwrap().to_string(),
        grounder: None,
        graph: None,
        criteria: vec![criterion_a.to_string()],
    };
    let mut cfg2 = fresh_run_cfg("false");
    cfg2.workflow.defaults.max_retries = 1;
    let rs2 = run(&cfg2, &deps2).unwrap();
    assert_eq!(
        rs2.units[shared_slug].status,
        ledger::Status::Escalated,
        "an always-failing gate must exhaust remediation and escalate, never integrate"
    );
    let shared_tip_after_run2 = git_out(repo.path(), &["rev-parse", &shared_branch])
        .expect("the escalated unit's (fresh) durable branch must exist with a resolvable tip");
    assert_eq!(shared_tip_after_run2.len(), 40, "a real git commit sha");
    assert!(
        !repo.path().join("run2-criterion-a-work.txt").exists(),
        "the escalated unit never integrated, so its file must NOT be on the base yet"
    );

    // RUN 3: a THIRD, independently-named unit re-serves criterion A.
    start_fresh(&store, &[criterion_a.to_string()], "", "", "").unwrap();
    let fresh_slug = "run3-independently-named-unit";
    let driver3 = ProposesSlugDriver {
        proposed_id: fresh_slug.to_string(),
        criterion: criterion_a.to_string(),
        worker_write: None,
        gates: vec!["gate".to_string()],
    };
    let deps3 = Deps {
        store: &store,
        driver: &driver3,
        gates: &ExecRunner,
        repo: repo.path().to_str().unwrap().to_string(),
        grounder: None,
        graph: None,
        criteria: vec![criterion_a.to_string()],
    };
    let rs3 = run(&fresh_run_cfg("true"), &deps3).unwrap();
    assert_eq!(
        rs3.units[fresh_slug].status,
        ledger::Status::Integrated,
        "run 3's own unit still runs its ordinary lifecycle through to integration; units: {:?}",
        rs3.units.keys().collect::<Vec<_>>()
    );
    assert!(
        repo.path().join("run2-criterion-a-work.txt").exists(),
        "run 2's abandoned work must ride the adoption all the way into the base"
    );

    let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    let run2_started = find_last_unit_started(&events, shared_slug);
    let run3_started = find_unit_started(&events, fresh_slug);

    // Two independent production call sites - run 2's and run 3's planner-proposal
    // resolution, run-apart - computed the SAME criterion_stable_id for the same
    // criterion-A text. Neither value is hand-typed by this test.
    let cid_a_from_run2 = run2_started["criterion_id"]
        .as_str()
        .expect("run 2's UnitStarted carries its criterion_id");
    let cid_a_from_run3 = run3_started["criterion_id"]
        .as_str()
        .expect("run 3's UnitStarted carries its criterion_id");
    assert_eq!(
        cid_a_from_run3, cid_a_from_run2,
        "two independent production call sites must compute the identical criterion_stable_id \
         for the same criterion-A text"
    );

    // The core proof: shared_slug's EARLIER, UNRELATED criterion-B integration (run 1) must
    // never mask its LATER, still-abandoned criterion-A attempt (run 2) from run 3's
    // adoption.
    assert_eq!(
        run3_started["adopted_from"]["unit"], shared_slug,
        "shared_slug's abandoned criterion-A attempt must still be adoptable by run 3 - its \
         earlier, unrelated criterion-B integration in run 1 must never mask it: {run3_started}"
    );
    let adopted_tip = run3_started["adopted_from"]["tip"]
        .as_str()
        .expect("adopted_from.tip must be a string");
    assert_eq!(
        adopted_tip, shared_tip_after_run2,
        "adopted_from.tip must be shared_slug's ACTUAL run-2 tip sha, read back off real git \
         (never re-derived by this test): {run3_started}"
    );
}

/// The `commit` field of `id`'s `UnitIntegrated` event, read back off the real store -
/// used to stamp a realistic `META_COMPENSATED` value (a real reverted commit sha, never
/// a placeholder string) on the hand-authored compensation marker tests 5 and 6 append.
fn find_unit_integrated_commit(events: &[Event], id: &str) -> String {
    for e in events {
        if e.type_ != ledger::TYPE_UNIT_INTEGRATED {
            continue;
        }
        let Ok(body) = serde_json::from_slice::<Value>(&e.data) else {
            continue;
        };
        if body.get("id").and_then(Value::as_str) == Some(id) {
            return body["commit"]
                .as_str()
                .expect("UnitIntegrated must carry a commit sha")
                .to_string();
        }
    }
    panic!(
        "no UnitIntegrated recorded for unit {id:?} among {} events",
        events.len()
    );
}

/// Test 4's Done-when proof (round 3, SPEC-SCOPED): "regardless of the planner's slug" is
/// scoped to the OWNING SPEC - a fresh unit for a DIFFERENT spec must never adopt an
/// abandoned attempt at a textually-identical criterion, and a fresh unit for its OWN
/// spec must still adopt across a run boundary even when that spec's path is spelled two
/// different ways (stemmed-identity matching, never raw path equality - see the module
/// doc comment's rationale for why this matters).
#[test]
fn spec_scoping_blocks_adoption_across_specs_sharing_a_criterion_id_but_not_across_two_runs_of_the_same_spec(
) {
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let store = Store::open(":memory:").unwrap();
    // Deliberately the SAME criterion text throughout: two UNRELATED specs whose Nth
    // Done-when item happens to be byte-for-byte identical text mint the IDENTICAL
    // criterion_stable_id (position + content hash, no spec identity folded in) - the
    // exact real-world collision adv-u88c2-r2-criterion-id-unscoped-crosses-specs names.
    let criterion = "both feature lanes stay green (fmt, clippy -D warnings, cargo test)";

    let spec_a = "specs/88-adoption-keys-on-criterion.md";
    // The SAME spec, spelled with a DIFFERENT raw path (a different directory) - proves
    // `ledger::spec_stem` identity matching, never raw path string equality.
    let spec_a_reshelved = "docs/completed-specs/88-adoption-keys-on-criterion.md";
    let spec_b = "specs/90-hermetic-test-git-and-merge-friendly-audit-artifacts.md";

    // RUN 1 (spec A): the baseline unit's gate always fails - one remediation attempt
    // then escalate, so it never integrates and its real committed work sits on an
    // abandoned durable branch, exactly like test 1's prior-run setup. `start_fresh` is
    // called explicitly (spec_path non-empty) rather than left to `run`'s own internal
    // `ensure_started` (which never threads a spec through), mirroring how a real `rigger
    // run --spec ...` CLI invocation mints the run before the conductor ever touches it.
    start_fresh(&store, &[criterion.to_string()], "", "", spec_a).unwrap();
    let driver1 = WritesFileDriver {
        file_name: "spec-a-prior-work.txt".into(),
        content: "spec A's abandoned attempt\n".into(),
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
    let prior_slug = rs1.units.keys().next().unwrap().clone();
    assert_eq!(
        rs1.units[&prior_slug].status,
        ledger::Status::Escalated,
        "an always-failing gate must exhaust remediation and escalate, never integrate"
    );
    let prior_branch = format!("rigger/u/{prior_slug}");
    let prior_tip = git_out(repo.path(), &["rev-parse", &prior_branch])
        .expect("spec A's escalated unit must carry a real, resolvable durable branch");

    // RUN 2 (spec B): a DIFFERENT spec, the SAME criterion text (so a second, independent
    // production call site computes the SAME criterion_stable_id) - a differently-named
    // unit must NOT adopt spec A's abandoned attempt.
    start_fresh(&store, &[criterion.to_string()], "", "", spec_b).unwrap();
    let fresh_slug_b = "spec-b-cross-spec-unit";
    let driver2 = ProposesSlugDriver {
        proposed_id: fresh_slug_b.to_string(),
        criterion: criterion.to_string(),
        worker_write: Some(("spec-b-own-work.txt".into(), "spec B's own work\n".into())),
        gates: Vec::new(),
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
        rs2.units[fresh_slug_b].status,
        ledger::Status::Integrated,
        "a cross-spec unit still runs its ordinary lifecycle through to integration"
    );
    assert!(repo.path().join("spec-b-own-work.txt").exists());
    assert!(
        !repo.path().join("spec-a-prior-work.txt").exists(),
        "spec A's abandoned attempt must NEVER be adopted by spec B's unit, despite both \
         computing the identical criterion_stable_id for the identical criterion text"
    );
    let events_after_run2 = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    assert_eq!(
        find_unit_started(&events_after_run2, fresh_slug_b)["adopted_from"],
        Value::Null,
        "cross-spec adoption must be refused even though the criterion_id matches"
    );

    // RUN 3 (spec A again, re-shelved under a DIFFERENT raw path): the SAME spec
    // identity, spelled differently - a differently-named unit MUST still adopt spec A's
    // still-abandoned attempt, proving spec-scoping matches on STEMMED identity, never on
    // the raw path string.
    start_fresh(&store, &[criterion.to_string()], "", "", spec_a_reshelved).unwrap();
    let fresh_slug_a2 = "spec-a-same-spec-different-path-unit";
    let driver3 = ProposesSlugDriver {
        proposed_id: fresh_slug_a2.to_string(),
        criterion: criterion.to_string(),
        worker_write: Some((
            "spec-a-run3-own-work.txt".into(),
            "spec A run 3's own work\n".into(),
        )),
        gates: Vec::new(),
    };
    let deps3 = Deps {
        store: &store,
        driver: &driver3,
        gates: &ExecRunner,
        repo: repo.path().to_str().unwrap().to_string(),
        grounder: None,
        graph: None,
        criteria: vec![criterion.to_string()],
    };
    let rs3 = run(&fresh_run_cfg("true"), &deps3).unwrap();
    assert_eq!(
        rs3.units[fresh_slug_a2].status,
        ledger::Status::Integrated,
        "the adopted unit still runs its ordinary lifecycle through to integration"
    );
    assert!(
        repo.path().join("spec-a-prior-work.txt").exists(),
        "spec A's abandoned attempt must ride the SAME-spec adoption into the base once a \
         differently-pathed rerun of its own spec proposes a unit for the same criterion"
    );
    assert!(repo.path().join("spec-a-run3-own-work.txt").exists());

    let events_after_run3 = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    let run3_started = find_unit_started(&events_after_run3, fresh_slug_a2);
    assert_eq!(
        run3_started["adopted_from"]["unit"], prior_slug,
        "same-spec adoption (raw path differs, stem matches) must still name the prior \
         unit: {run3_started}"
    );
    assert_eq!(
        run3_started["adopted_from"]["tip"].as_str(),
        Some(prior_tip.as_str()),
        "adopted_from.tip must be spec A's original escalated tip, read back off real git: \
         {run3_started}"
    );
}

/// Shared setup for tests 5 and 6 (round 3, TEMPORAL): a baseline unit integrates for
/// real - its durable branch reclaimed by `gc_integrated_branches`, exactly as test 3
/// above proves - then a REAL post-compensation "second life" branch is built for it
/// through the SAME production `Worktree::create` call a re-entered remediation round
/// takes when its branch is absent (see the module doc comment for why this is built
/// directly rather than through spec 12's own full compensation-trigger flow). Returns
/// `(repo, store, prior_slug, prior_branch, integrated_commit, second_life_tip)`.
fn integrate_then_build_a_real_second_life_branch(
    criterion: &str,
    first_life_file: &str,
    second_life_file: &str,
) -> (tempfile::TempDir, Store, String, String, String, String) {
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let repo_path = repo.path().to_str().unwrap().to_string();
    let store = Store::open(":memory:").unwrap();

    let driver1 = WritesFileDriver {
        file_name: first_life_file.into(),
        content: "first life, later compensated\n".into(),
    };
    let deps1 = Deps {
        store: &store,
        driver: &driver1,
        gates: &ExecRunner,
        repo: repo_path.clone(),
        grounder: None,
        graph: None,
        criteria: vec![criterion.to_string()],
    };
    let rs1 = run(&baseline_only_cfg("true", 3), &deps1).unwrap();
    let prior_slug = rs1.units.keys().next().unwrap().clone();
    assert_eq!(
        rs1.units[&prior_slug].status,
        ledger::Status::Integrated,
        "the unit later compensated must genuinely integrate first"
    );
    let prior_branch = format!("rigger/u/{prior_slug}");
    assert!(
        !worktree::branch_exists(&repo_path, &prior_branch),
        "an integrated unit's durable branch must be reclaimed before any compensation - \
         the second-life branch built below must be a genuinely FRESH ref, mirroring \
         production, never a leftover from the first life"
    );

    let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    let integrated_commit = find_unit_integrated_commit(&events, &prior_slug);

    // The REAL production path a re-entered remediation round takes when its branch is
    // absent (`Worktree::create`'s no-branch-exists arm: `git worktree add -b <branch>
    // <dir> HEAD`).
    let scratch = tempfile::tempdir().unwrap();
    let second_life_dir = scratch.path().join("second-life-wt");
    let wt = Worktree::create(
        &repo_path,
        second_life_dir.to_str().unwrap(),
        &prior_branch,
        "",
    )
    .unwrap();
    std::fs::write(second_life_dir.join(second_life_file), "second life work\n").unwrap();
    let second_life_tip = wt
        .commit("attempt-1 re-implements after a compensation revert")
        .unwrap();
    assert_eq!(second_life_tip.len(), 40, "a real git commit sha");
    assert!(
        worktree::branch_exists(&repo_path, &prior_branch),
        "the second-life branch must exist before either test's fresh-run adoption attempt"
    );

    (
        repo,
        store,
        prior_slug,
        prior_branch,
        integrated_commit,
        second_life_tip,
    )
}

/// Test 5's Done-when proof (round 3, TEMPORAL): a later compensation (spec 12, unit 4)
/// that reverts an integrated unit's work must REOPEN that unit's candidacy, so a
/// genuinely fresh redo can adopt its real, still-existing (second-life) branch instead
/// of starting from base and losing reviewed progress.
#[test]
fn a_compensation_reverted_integration_reopens_adoption_of_its_real_still_existing_branch() {
    let criterion = "the turbine reports its own vibration";
    let (repo, store, prior_slug, prior_branch, integrated_commit, second_life_tip) =
        integrate_then_build_a_real_second_life_branch(
            criterion,
            "first-life-work.txt",
            "second-life-work.txt",
        );

    // A REAL compensation revert's `UnitFailed` - the EXACT shape `drain_compensations`
    // (src/conductor.rs) appends: the existing `UnitFailed` vocabulary (no new event
    // type) carrying `META_COMPENSATED` with the reverted commit(s) and
    // `META_CONTRADICTION` with the reviewer's reason.
    store
        .append(
            STREAM,
            ExpectedRevision::Any,
            &[Event::new(
                ledger::TYPE_UNIT_FAILED,
                serde_json::to_vec(&json!({
                    "id": prior_slug,
                    "attempts": 2,
                    "cause": "reject",
                }))
                .unwrap(),
            )
            .with_meta(META_COMPENSATED, &integrated_commit)
            .with_meta(
                META_CONTRADICTION,
                "a later unit's review proved this attempt's approach wrong",
            )],
        )
        .unwrap();

    // FRESH RUN: a differently-named unit re-serves the SAME criterion.
    start_fresh(&store, &[criterion.to_string()], "", "", "").unwrap();
    let fresh_slug = "temporal-redo-unit";
    let driver2 = ProposesSlugDriver {
        proposed_id: fresh_slug.to_string(),
        criterion: criterion.to_string(),
        worker_write: Some(("run2-own-work.txt".into(), "genuinely new\n".into())),
        gates: Vec::new(),
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
        "the reopened-adoption unit still runs its ordinary lifecycle through to \
         integration"
    );

    assert!(
        repo.path().join("second-life-work.txt").exists(),
        "the compensation-reopened unit's REAL second-life branch must ride the \
         adoption into the base"
    );
    assert!(repo.path().join("run2-own-work.txt").exists());

    let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    let fresh_started = find_unit_started(&events, fresh_slug);
    assert_eq!(
        fresh_started["adopted_from"]["unit"], prior_slug,
        "a compensation-reverted integration must reopen adoption: {fresh_started}"
    );
    assert_eq!(
        fresh_started["adopted_from"]["tip"].as_str(),
        Some(second_life_tip.as_str()),
        "adopted_from.tip must be the REAL second-life branch's actual tip, read back \
         off real git (never re-derived by this test): {fresh_started}"
    );
    assert!(
        worktree::branch_exists(repo.path().to_str().unwrap(), &prior_branch),
        "adoption creates a NEW ref at the prior branch's tip, never a rename - the \
         prior branch name must stay resolvable afterward"
    );
}

/// Test 6's Done-when proof (round 3, TEMPORAL control): an ORDINARY remediation
/// `UnitFailed` - no `META_COMPENSATED` at all - must NEVER be mistaken for a
/// compensation revert, even when a real, adoptable second-life branch happens to exist
/// under the same unit id. The exclusion stays permanent for a genuine, non-reverted
/// integration.
#[test]
fn a_plain_remediation_failure_after_integration_never_reopens_adoption_even_though_a_same_named_branch_exists(
) {
    let criterion = "the compressor logs every restart";
    let (repo, store, prior_slug, prior_branch, _integrated_commit, _second_life_tip) =
        integrate_then_build_a_real_second_life_branch(
            criterion,
            "first-life-work-2.txt",
            "second-life-work-2.txt",
        );
    assert!(worktree::branch_exists(
        repo.path().to_str().unwrap(),
        &prior_branch
    ));

    // An ORDINARY remediation `UnitFailed` - no `META_COMPENSATED` metadata at all.
    store
        .append(
            STREAM,
            ExpectedRevision::Any,
            &[Event::new(
                ledger::TYPE_UNIT_FAILED,
                serde_json::to_vec(&json!({
                    "id": prior_slug,
                    "attempts": 1,
                    "cause": "reject",
                }))
                .unwrap(),
            )],
        )
        .unwrap();

    start_fresh(&store, &[criterion.to_string()], "", "", "").unwrap();
    let fresh_slug = "plain-failure-control-unit";
    let driver2 = ProposesSlugDriver {
        proposed_id: fresh_slug.to_string(),
        criterion: criterion.to_string(),
        worker_write: Some(("run2-own-work-2.txt".into(), "genuinely fresh\n".into())),
        gates: Vec::new(),
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
        "the control unit still runs its ordinary lifecycle through to integration"
    );

    assert!(
        !repo.path().join("second-life-work-2.txt").exists(),
        "a plain UnitFailed (no META_COMPENSATED) must never reopen adoption, even \
         though a same-named real branch exists ready to be adopted"
    );
    assert!(repo.path().join("run2-own-work-2.txt").exists());

    let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    let fresh_started = find_unit_started(&events, fresh_slug);
    assert_eq!(
        fresh_started["adopted_from"],
        Value::Null,
        "an ordinary remediation failure must never reopen an integrated criterion's \
         exclusion: {fresh_started}"
    );
}
