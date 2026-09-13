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
//!
//! ROUND 4 (tests 7-8): operator ruling
//! `op-u88c2-round-4-adoption-keys-corrected-escalated-baselines-are-always-candidates`
//! item 3 closes the crash window between `adopt_prior_criterion_branch` deciding to
//! adopt and the caller's `UnitStarted` recording that decision - the ONLY other place
//! `adopted_from` was ever written (upheld three review rounds running as
//! `sdet-u88c2-adopted-from-lost-on-crash-between-branch-create-and-unitstarted`, then as
//! the round-3 PRIMARY BLOCKER left entirely unimplemented). The fix writes the decision
//! `{unit, tip, spec}` as durable `UnitStatus` log state the MOMENT it is made - BEFORE
//! the git side effect (`Worktree::create_branch_at`) that seeds the adopting unit's own
//! branch, which itself lands BEFORE `UnitStarted`. No public API can interrupt
//! `adopt_prior_criterion_branch` mid-call to inject a real crash, so - exactly like
//! tests 5 and 6 above - these two tests reproduce the precise durable SHAPE the fix
//! writes (a `UnitStatus` carrying `status: "adoption-recorded"` and the decided
//! `adopted_from` triple) directly, paired with whichever of the two OTHER writes (the
//! git branch, the `UnitStarted`) a crash at that exact point would or would not have
//! reached, then drive a REAL second `run()` call against the same store and repo - a
//! genuine resumed process - and prove the decision survives onto the unit's real
//! `UnitStarted` either way. Both tests deliberately record a `spec` value a FRESH
//! re-derivation could never produce (the fresh run below carries no launched spec, so
//! `current_run_spec` would fold to the empty string) - the surviving value must be the
//! recorded one, proving these tests exercise the READ-BACK path, not merely a
//! coincidental re-computation.
//!
//! Test 7 reproduces BOTH the git branch and the provenance mark already landed - the
//! window `sdet-u88c2-adopted-from-lost-on-crash-between-branch-create-and-unitstarted`
//! originally named, where the pre-fix code's `branch_exists` check alone short-circuited
//! straight to `None` the moment the unit's own branch already existed, discarding the
//! decision permanently. Test 8 reproduces only the provenance mark, proving a resumed
//! call creates the still-missing branch at the recorded tip rather than failing or
//! silently starting fresh.
//!
//! Test 9 (round 4, new-public-API probe): `Worktree::branch_tip` (src/worktree.rs) is a
//! brand-new public function this round - `adopt_prior_criterion_branch` now calls it to
//! read a prior candidate's tip BEFORE writing durable provenance, in place of the old
//! `branch_exists` guard the pre-round-4 code checked first. Tests 1-8 above all exercise
//! its SUCCESS arm implicitly (any fresh adoption decision calls it), but none exercise its
//! error arm: a prior candidate whose own branch has since been deleted (an operator's
//! manual cleanup, or any process that pruned the ref) must still resolve to "nothing to
//! adopt, start fresh" - the exact pre-existing contract `adopt_prior_criterion_branch`'s
//! own doc comment names - never a propagated error and never a spurious
//! `STATUS_ADOPTION_RECORDED` mark (which the fix's own ordering only ever writes AFTER a
//! successful `branch_tip`).

use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

use rigger::conductor::{
    run, AgentDriver, AgentResult, Deps, Error, SpawnOpts, META_REPLAY_KEY, STREAM,
    TYPE_UNIT_PROPOSED,
};
use rigger::conductor::{META_COMPENSATED, META_CONTRADICTION};
use rigger::config::{AgentDef, Config, Gate, Stage};
use rigger::contextgraph;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{
    Direction, Error as StoreError, Event, EventStore, ExpectedRevision, Filter, Position,
    Revision, Subscription,
};
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
    start_fresh(&store, &[criterion.to_string()], "", "", "", "").unwrap();
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
    start_fresh(&store, &[criterion.to_string()], "", "", "", "").unwrap();
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
    start_fresh(&store, &[criterion_a.to_string()], "", "", "", "").unwrap();
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
    start_fresh(&store, &[criterion_a.to_string()], "", "", "", "").unwrap();
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
    start_fresh(&store, &[criterion.to_string()], "", "", "", spec_a).unwrap();
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
    start_fresh(&store, &[criterion.to_string()], "", "", "", spec_b).unwrap();
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
    start_fresh(
        &store,
        &[criterion.to_string()],
        "",
        "",
        "",
        spec_a_reshelved,
    )
    .unwrap();
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
    start_fresh(&store, &[criterion.to_string()], "", "", "", "").unwrap();
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

    start_fresh(&store, &[criterion.to_string()], "", "", "", "").unwrap();
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

/// Test 7 (round 4): the PRIMARY BLOCKER's own named scenario -
/// `sdet-u88c2-adopted-from-lost-on-crash-between-branch-create-and-unitstarted` - a
/// crash AFTER `Worktree::create_branch_at` lands the adopting unit's branch but BEFORE
/// its `UnitStarted` append. Pre-fix, `adopt_prior_criterion_branch`'s FIRST check
/// (`branch_exists`) alone short-circuited straight to `None` the instant the branch
/// existed, so the resumed `UnitStarted` recorded no adoption at all despite the unit's
/// branch carrying real adopted content - a genuine unit lifecycle continuing on
/// silently-unrecorded provenance.
#[test]
fn a_crash_after_the_branch_exists_but_before_unitstarted_lands_recovers_the_recorded_adoption() {
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let store = Store::open(":memory:").unwrap();
    let criterion = "the pump reports its own pressure";

    // PRIOR RUN: an escalated baseline unit with real committed work, exactly like test
    // 1's setup.
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
    let prior_slug = rs1.units.keys().next().unwrap().clone();
    assert_eq!(
        rs1.units[&prior_slug].status,
        ledger::Status::Escalated,
        "an always-failing gate must exhaust remediation and escalate, never integrate"
    );
    let prior_tip = git_out(
        repo.path(),
        &["rev-parse", &format!("rigger/u/{prior_slug}")],
    )
    .expect("the escalated unit's durable branch must exist with a resolvable tip");
    // The fresh unit below serves the SAME criterion text at the same position, so a
    // real production call site computes the IDENTICAL `criterion_stable_id` - read back
    // off the prior unit's own `UnitStarted` rather than hand-typed, so the hand-crafted
    // provenance mark below carries the value `recorded_adoption` (round 5, keyed on the
    // full `(unit, criterion_id, spec)` triple) actually requires to match.
    let events_after_run1 = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    let fresh_criterion_id = find_unit_started(&events_after_run1, &prior_slug)["criterion_id"]
        .as_str()
        .expect("the prior unit's own UnitStarted carries its criterion_id")
        .to_string();

    // FRESH RUN boundary, a differently-named unit for the SAME criterion.
    start_fresh(&store, &[criterion.to_string()], "", "", "", "").unwrap();
    let fresh_slug = "crash-after-branch-unit";
    let fresh_branch = format!("rigger/u/{fresh_slug}");

    // Reproduce the EXACT crash state: the git side effect already landed - a real
    // branch at the prior tip, via the SAME production `Worktree::create_branch_at`
    // call - and, per the fix, the durable provenance mark that is always written
    // BEFORE it is therefore ALSO already on the log; only the eventual `UnitStarted`
    // never landed. `criterion_id` and `spec` are the fresh unit's OWN identity (round
    // 5) - `spec` is "" since this fresh run carries no launched spec path
    // (`current_run_spec` folds to the empty string), matching what a real crash-then-
    // resume would have recorded at the moment of decision.
    Worktree::create_branch_at(repo.path().to_str().unwrap(), &fresh_branch, &prior_tip).unwrap();
    store
        .append(
            STREAM,
            ExpectedRevision::Any,
            &[Event::new(
                ledger::TYPE_UNIT_STATUS,
                serde_json::to_vec(&json!({
                    "id": fresh_slug,
                    "status": "adoption-recorded",
                    "criterion_id": fresh_criterion_id,
                    "spec": "",
                    "adopted_from": {
                        "unit": prior_slug,
                        "tip": prior_tip,
                        "spec": "specs/88-a-unit-lineage-is-durable",
                    },
                }))
                .unwrap(),
            )],
        )
        .unwrap();
    let events_before_resume = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    assert!(
        !events_before_resume
            .iter()
            .any(|e| e.type_ == ledger::TYPE_UNIT_STARTED
                && serde_json::from_slice::<Value>(&e.data)
                    .ok()
                    .and_then(|v| v.get("id").and_then(Value::as_str).map(str::to_string))
                    == Some(fresh_slug.to_string())),
        "the simulated crash must leave NO UnitStarted for the fresh unit yet"
    );

    // RESUME: a real second `run()` call against the SAME store and repo - exactly what
    // a fresh `rigger step` process does after the crash.
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
        "the recovered unit still runs its ordinary lifecycle through to integration"
    );
    assert!(
        repo.path().join("prior-work.txt").exists(),
        "the adopted branch's real prior content must still ride into the base"
    );
    assert!(repo.path().join("run2-own-work.txt").exists());

    let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    let fresh_started = find_unit_started(&events, fresh_slug);
    assert_eq!(
        fresh_started["adopted_from"],
        json!({"unit": prior_slug, "tip": prior_tip, "spec": "specs/88-a-unit-lineage-is-durable"}),
        "the crash-resumed unit must recover the EXACT recorded decision, spec field \
         included - never None (the pre-fix defect: `branch_exists` alone returned None \
         the instant the unit's own branch already existed) and never a freshly \
         re-derived value (this run's own empty spec would fold to \"\", not the \
         recorded value, if `current_run_spec` ran again here): {fresh_started}"
    );
}

/// Test 8 (round 4): the OTHER half of the same crash window - a crash AFTER the durable
/// provenance mark is written but BEFORE `Worktree::create_branch_at` ever runs, so the
/// adopting unit's own branch does not exist yet at all. A resumed call must create it
/// at the RECORDED tip and complete the adoption exactly as an uninterrupted run would.
#[test]
fn a_crash_after_the_provenance_record_but_before_the_branch_is_created_still_completes_the_adoption_on_resume(
) {
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let store = Store::open(":memory:").unwrap();
    let criterion = "the valve reports its own position";

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
    let prior_slug = rs1.units.keys().next().unwrap().clone();
    assert_eq!(
        rs1.units[&prior_slug].status,
        ledger::Status::Escalated,
        "an always-failing gate must exhaust remediation and escalate, never integrate"
    );
    let prior_tip = git_out(
        repo.path(),
        &["rev-parse", &format!("rigger/u/{prior_slug}")],
    )
    .expect("the escalated unit's durable branch must exist with a resolvable tip");
    // The fresh unit below serves the SAME criterion text at the same position, so a
    // real production call site computes the IDENTICAL `criterion_stable_id` - read back
    // off the prior unit's own `UnitStarted` rather than hand-typed, so the hand-crafted
    // provenance mark below carries the value `recorded_adoption` (round 5, keyed on the
    // full `(unit, criterion_id, spec)` triple) actually requires to match.
    let events_after_run1 = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    let fresh_criterion_id = find_unit_started(&events_after_run1, &prior_slug)["criterion_id"]
        .as_str()
        .expect("the prior unit's own UnitStarted carries its criterion_id")
        .to_string();

    start_fresh(&store, &[criterion.to_string()], "", "", "", "").unwrap();
    let fresh_slug = "crash-before-branch-unit";
    let fresh_branch = format!("rigger/u/{fresh_slug}");

    // Reproduce ONLY the provenance write - the crash happens before the git side
    // effect ever runs, so the fresh unit's own branch must NOT exist yet. `criterion_id`
    // and `spec` are the fresh unit's OWN identity (round 5) - `spec` is "" since this
    // fresh run carries no launched spec path (`current_run_spec` folds to the empty
    // string), matching what a real crash-then-resume would have recorded at the moment
    // of decision.
    store
        .append(
            STREAM,
            ExpectedRevision::Any,
            &[Event::new(
                ledger::TYPE_UNIT_STATUS,
                serde_json::to_vec(&json!({
                    "id": fresh_slug,
                    "status": "adoption-recorded",
                    "criterion_id": fresh_criterion_id,
                    "spec": "",
                    "adopted_from": {
                        "unit": prior_slug,
                        "tip": prior_tip,
                        "spec": "specs/88-a-unit-lineage-is-durable",
                    },
                }))
                .unwrap(),
            )],
        )
        .unwrap();
    assert!(
        !worktree::branch_exists(repo.path().to_str().unwrap(), &fresh_branch),
        "the simulated crash must leave the fresh unit's branch NOT YET created"
    );

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
        "the resumed unit still runs its ordinary lifecycle through to integration"
    );
    // The adopted content (`prior-work.txt`) AND the unit's own fresh work
    // (`run2-own-work.txt`) both riding into the base is only possible if the resumed
    // call actually created `fresh_branch` at the recorded tip and continued the
    // ordinary lifecycle on it - a genuinely fresh (non-adopting) start would never
    // produce `prior-work.txt` at all (exactly as test 1 establishes). `fresh_branch`
    // itself is NOT re-checked here: `gc_integrated_branches` reclaims an integrated
    // unit's durable branch in the SAME call that integrates it (test 3's own doc
    // comment), so by the time `run` returns, a genuinely correct resume has ALREADY
    // deleted the very branch it created - asserting its continued existence here would
    // be asserting a bug, not the fix.
    assert!(repo.path().join("prior-work.txt").exists());
    assert!(repo.path().join("run2-own-work.txt").exists());

    let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    let fresh_started = find_unit_started(&events, fresh_slug);
    assert_eq!(
        fresh_started["adopted_from"],
        json!({"unit": prior_slug, "tip": prior_tip, "spec": "specs/88-a-unit-lineage-is-durable"}),
        "the resumed adoption must record the SAME recorded decision, spec field \
         included - a fresh re-derivation here would fold to an empty spec, not the \
         recorded value: {fresh_started}"
    );
}

/// Test 9 (round 4, new-public-API probe): `Worktree::branch_tip`'s error arm - a prior
/// candidate found via `prior_criterion_unit` (right criterion, right spec, never
/// integrated) whose own durable branch has since been deleted. Pre-round-4, the
/// equivalent guard was `!worktree::branch_exists(&prior_branch)`; round 4 replaced it
/// with a single `branch_tip` call whose `Err` arm must reach the identical outcome -
/// nothing adopted, no error propagated, no provenance mark written - never a regression
/// introduced by collapsing the two-step check into one.
#[test]
fn a_prior_candidates_deleted_branch_starts_the_fresh_unit_genuinely_unadopted() {
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let store = Store::open(":memory:").unwrap();
    let criterion = "the sensor reports its own calibration";

    // PRIOR RUN: an escalated baseline unit with real committed work, exactly like test
    // 1's setup - a genuine adoption candidate by every other measure.
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
    let prior_slug = rs1.units.keys().next().unwrap().clone();
    assert_eq!(
        rs1.units[&prior_slug].status,
        ledger::Status::Escalated,
        "an always-failing gate must exhaust remediation and escalate, never integrate"
    );
    let prior_branch = format!("rigger/u/{prior_slug}");
    assert!(
        worktree::branch_exists(repo.path().to_str().unwrap(), &prior_branch),
        "the escalated unit's durable branch must exist before this test deletes it"
    );

    // Delete the prior candidate's own durable branch - the exact "already gone" case
    // `adopt_prior_criterion_branch`'s doc comment has always named, reached this round
    // through `branch_tip`'s error arm instead of a `branch_exists` guard.
    assert!(Command::new("git")
        .arg("-C")
        .arg(repo.path())
        .args(["branch", "-D", &prior_branch])
        .status()
        .unwrap()
        .success());
    assert!(
        !worktree::branch_exists(repo.path().to_str().unwrap(), &prior_branch),
        "the prior candidate's branch must be genuinely gone before the fresh run starts"
    );

    // FRESH RUN: a differently-named unit re-serves the SAME criterion. `prior_criterion_
    // unit` still finds `prior_slug` as a candidate (never integrated, same criterion,
    // same spec) - only the git side effect that would seed the new branch is now
    // impossible.
    start_fresh(&store, &[criterion.to_string()], "", "", "", "").unwrap();
    let fresh_slug = "deleted-prior-branch-unit";
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
        "a candidate whose branch is gone must never block the fresh unit's own ordinary \
         lifecycle; units: {:?}",
        rs2.units.keys().collect::<Vec<_>>()
    );
    assert!(
        !repo.path().join("prior-work.txt").exists(),
        "nothing was adopted, so the prior candidate's content must NOT ride into the base"
    );
    assert!(repo.path().join("run2-own-work.txt").exists());

    let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    let fresh_started = find_unit_started(&events, fresh_slug);
    assert_eq!(
        fresh_started["adopted_from"],
        Value::Null,
        "branch_tip's error arm must resolve to no adoption, exactly like the pre-round-4 \
         branch_exists guard it replaced - never a propagated error, never a fabricated \
         provenance triple: {fresh_started}"
    );
    assert!(
        !events.iter().any(|e| {
            e.type_ == ledger::TYPE_UNIT_STATUS
                && serde_json::from_slice::<Value>(&e.data)
                    .ok()
                    .is_some_and(|v| {
                        v.get("id").and_then(Value::as_str) == Some(fresh_slug)
                            && v.get("status").and_then(Value::as_str) == Some("adoption-recorded")
                    })
        }),
        "no durable adoption-recorded mark may exist for a unit that never actually \
         adopted anything - the fix only writes it AFTER a successful branch_tip"
    );
}

/// Test 10 (round 5, PRIMARY BLOCKER fix): `recorded_adoption`/`adoption_provenance_key`
/// keyed the durable [`STATUS_ADOPTION_RECORDED`] fast path on the BARE unit id alone
/// (round 4) - unlike `prior_criterion_unit`, which that same round's own doc comment
/// requires be keyed on `(id, criterion_id, spec)` because a planner slug carries no
/// cross-run, cross-spec uniqueness guarantee. `adopt_prior_criterion_branch` consults
/// `recorded_adoption` FIRST, unconditionally - so once ANY unit id ever legitimately
/// adopts once, that decision replayed FOREVER for any later, wholly UNRELATED unit that
/// merely happens to reuse the same literal id, regardless of criterion or spec: real
/// cross-spec content contamination via the git side effect (`Worktree::create_branch_at`),
/// not merely a missed exclusion (arch-u88c2-r4-recorded-adoption-bare-id-crosses-specs,
/// sdet-u88c2-r4-confirms-recorded-adoption-bare-id-crosses-criteria,
/// adv-u88c2-r4-independently-live-reproduced-bare-id-collision).
///
/// Mirrors test 4's spec-scoping shape but drives the ONE case test 4's own kept fixture
/// never covers (adv-u88c2-r4-bug-breaches-rulings-own-fixture-1-not-merely-a-di-nit): test
/// 4 reuses the SAME spec across two DIFFERENTLY-named units; this test reuses the SAME
/// literal unit id across two UNRELATED specs/criteria. Three real `conductor::run` calls,
/// one store, one real git repo:
///
/// RUN 1 (spec A, criterion X): a baseline unit escalates with real committed work on an
/// abandoned durable branch (test 1's setup).
/// RUN 2 (spec A, SAME criterion X): a differently-named planner proposal `reused-id`
/// legitimately adopts run 1's baseline and integrates for real - `gc_integrated_branches`
/// (test 3) reclaims its durable branch once it does, so by run 3 `rigger/u/reused-id`
/// genuinely does not exist any more, exactly as the real crash-window tests (7, 8) assume.
/// RUN 3 (spec B, an UNRELATED criterion Y): a planner independently reuses the literal
/// slug `reused-id` for its own unrelated criterion. `adopted_from` must come back `Null`
/// - the pre-fix defect returned run 1's baseline verbatim instead.
#[test]
fn a_reused_planner_slug_never_replays_an_unrelated_specs_recorded_adoption_decision() {
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let store = Store::open(":memory:").unwrap();

    let criterion_x = "the pump reports its own pressure at a stable interval";
    let spec_a = "specs/88-adoption-keys-on-criterion.md";

    // RUN 1 (spec A, criterion X): the deterministic baseline escalates with real
    // committed work, exactly like test 1's setup.
    start_fresh(&store, &[criterion_x.to_string()], "", "", "", spec_a).unwrap();
    let driver1 = WritesFileDriver {
        file_name: "spec-a-baseline-work.txt".into(),
        content: "spec A's abandoned baseline attempt\n".into(),
    };
    let deps1 = Deps {
        store: &store,
        driver: &driver1,
        gates: &ExecRunner,
        repo: repo.path().to_str().unwrap().to_string(),
        grounder: None,
        graph: None,
        criteria: vec![criterion_x.to_string()],
    };
    let rs1 = run(&baseline_only_cfg("false", 1), &deps1).unwrap();
    let prior_slug = rs1.units.keys().next().unwrap().clone();
    assert_eq!(
        rs1.units[&prior_slug].status,
        ledger::Status::Escalated,
        "an always-failing gate must exhaust remediation and escalate, never integrate"
    );
    let prior_tip = git_out(
        repo.path(),
        &["rev-parse", &format!("rigger/u/{prior_slug}")],
    )
    .expect("the escalated baseline's durable branch must exist with a resolvable tip");

    // RUN 2 (spec A, SAME criterion X): a differently-named planner proposal
    // legitimately adopts run 1's baseline - a SANCTIONED adoption - and integrates for
    // real, so its durable `STATUS_ADOPTION_RECORDED` mark is keyed on the bare id
    // `reused-id` (pre-fix) exactly like a genuine real-world adoption would produce.
    start_fresh(&store, &[criterion_x.to_string()], "", "", "", spec_a).unwrap();
    let reused_id = "reused-id";
    let reused_branch = format!("rigger/u/{reused_id}");
    let driver2 = ProposesSlugDriver {
        proposed_id: reused_id.to_string(),
        criterion: criterion_x.to_string(),
        worker_write: Some((
            "run2-own-work.txt".into(),
            "reused-id's own real work\n".into(),
        )),
        gates: Vec::new(),
    };
    let deps2 = Deps {
        store: &store,
        driver: &driver2,
        gates: &ExecRunner,
        repo: repo.path().to_str().unwrap().to_string(),
        grounder: None,
        graph: None,
        criteria: vec![criterion_x.to_string()],
    };
    let rs2 = run(&fresh_run_cfg("true"), &deps2).unwrap();
    assert_eq!(
        rs2.units[reused_id].status,
        ledger::Status::Integrated,
        "the sanctioned adoption still runs its ordinary lifecycle through to integration"
    );
    let events_after_run2 = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    let run2_started = find_unit_started(&events_after_run2, reused_id);
    assert_eq!(
        run2_started["adopted_from"]["unit"], prior_slug,
        "run 2's reused-id must genuinely adopt spec A's baseline: {run2_started}"
    );
    assert_eq!(
        run2_started["adopted_from"]["tip"].as_str(),
        Some(prior_tip.as_str()),
        "run 2's adoption must name spec A's baseline's real tip: {run2_started}"
    );
    assert!(
        !worktree::branch_exists(repo.path().to_str().unwrap(), &reused_branch),
        "the integrated reused-id branch must already be reclaimed by \
         gc_integrated_branches before run 3 reuses the same literal slug, exactly like \
         the real crash-window tests (7, 8) assume"
    );

    // RUN 3 (spec B, an UNRELATED criterion Y): a planner independently reuses the SAME
    // literal slug `reused-id` for a wholly unrelated criterion under an unrelated spec.
    let criterion_y = "the valve independently reports its own position on every poll";
    let spec_b = "specs/90-hermetic-test-git-and-merge-friendly-audit-artifacts.md";
    start_fresh(&store, &[criterion_y.to_string()], "", "", "", spec_b).unwrap();
    let driver3 = ProposesSlugDriver {
        proposed_id: reused_id.to_string(),
        criterion: criterion_y.to_string(),
        worker_write: Some((
            "run3-own-work.txt".into(),
            "run 3's own unrelated work\n".into(),
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
        criteria: vec![criterion_y.to_string()],
    };
    let rs3 = run(&fresh_run_cfg("true"), &deps3).unwrap();
    assert_eq!(
        rs3.units[reused_id].status,
        ledger::Status::Integrated,
        "the unrelated reuse still runs its ordinary lifecycle through to integration"
    );
    // NOTE: `spec-a-baseline-work.txt` is NOT asserted absent here - run 2 already
    // legitimately merged it onto the ONE shared repo's trunk HEAD when it integrated
    // above, so it is present on every subsequent unit's tree (including a genuinely
    // fresh, unadopted one) regardless of this fix - that is ordinary, correct trunk
    // history, not the defect under test. The decisive, non-confounded signal is
    // `adopted_from` on run 3's OWN `UnitStarted`: a real (buggy) adoption pins run 3's
    // branch to run 1's OLD escalated tip (a stale, disconnected commit, never the
    // current trunk HEAD run 3's own fresh branch would otherwise start from) and
    // records that pin as provenance - `Null` proves no such pin, real or recorded, ever
    // happened.
    assert!(repo.path().join("run3-own-work.txt").exists());

    let events_after_run3 = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    // `reused_id` now has TWO `UnitStarted` events (run 2 and run 3) - `find_last_unit_
    // started` (test 3's helper) reads the CURRENT, still-live incarnation, never the
    // stale run-2 one `find_unit_started` would return.
    let run3_started = find_last_unit_started(&events_after_run3, reused_id);
    assert_eq!(
        run3_started["adopted_from"],
        Value::Null,
        "reusing a literal planner slug across two wholly unrelated specs/criteria must \
         never replay the FIRST reuse's recorded adoption decision - the pre-fix bare-id \
         fast path answered for ANY later unit sharing the id, regardless of criterion or \
         spec: {run3_started}"
    );
}

/// Test 11 (round 5, "event type / serialized form" probe - the back-compat half test 10
/// does not cover): round 5 widened the durable [`STATUS_ADOPTION_RECORDED`] `UnitStatus`
/// mark's payload with two NEW top-level fields (`criterion_id`, `spec`) so
/// `recorded_adoption` can match on the full triple instead of the bare id alone (test 10).
/// Every mark test 10's own real `run()` calls produce is written by THIS SAME round-5
/// binary, so it always carries both new fields - it cannot exercise what happens when the
/// reader meets a mark durably written by the PRE-round-5 binary (rounds 1-4's shape: only
/// `id`, `status`, `adopted_from`, exactly what `adopt_prior_criterion_branch`'s round-4
/// `emit_keyed_meta` call constructed, before this round's touch-up added the two fields).
/// That shape can already sit on a real, persistent event store the moment a running
/// process upgrades mid-campaign - the self-hosting exposure this project's own `.rigger/`
/// store carries on every "refresh the binary after a landed spec" cycle (unlike test 10's
/// three runs, which share one ephemeral `:memory:` store the same binary writes end to
/// end). Mirrors test 2's own "event type / serialized form" back-compat precedent (a
/// pre-spec-88 `UnitStarted` missing `criterion_id` entirely) for this round's new fields
/// instead.
///
/// The property under test is NOT "recognize the legacy mark with full fidelity" - round 5
/// exists PRECISELY because bare-id recognition alone is unsafe (test 10's whole point). It
/// is that a legacy-shaped record, missing the new fields altogether, is safely treated as
/// UNRECOGNIZED rather than resurrecting the exact cross-identity contamination test 10
/// closes through a different route: `recorded_adoption` (conductor.rs) defaults
/// `own_criterion_id`/`own_spec` to `""` via `unwrap_or_default()` when the keys are absent,
/// and this proves that default can never coincide with a real `criterion_stable_id` (never
/// empty) at the one real boundary that matters - a real `conductor::run()` call - rather
/// than only inside the private function's own unit tests.
#[test]
fn a_legacy_adoption_mark_missing_criterion_id_and_spec_never_matches_a_reused_id_for_an_unrelated_criterion(
) {
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let store = Store::open(":memory:").unwrap();

    let legacy_id = "legacy-shape-unit";
    let legacy_branch = format!("rigger/u/{legacy_id}");
    let legacy_tip = git_out(repo.path(), &["rev-parse", "HEAD"])
        .expect("the bare init commit must resolve a tip to point the legacy branch at");

    // Reproduce the EXACT pre-round-5 durable shape: `adopt_prior_criterion_branch`'s
    // round-4 `emit_keyed_meta` call wrote only `id`, `status` and `adopted_from` - no
    // `criterion_id`, no `spec`. The git side effect a real completed round-4 adoption
    // would also have finished by this point is reproduced too, exactly like tests 7-9's
    // "reproduce the exact state" technique, so `branch_exists` sees the same
    // unit-already-has-a-branch condition a real completed legacy adoption left behind.
    Worktree::create_branch_at(repo.path().to_str().unwrap(), &legacy_branch, &legacy_tip).unwrap();
    store
        .append(
            STREAM,
            ExpectedRevision::Any,
            &[Event::new(
                ledger::TYPE_UNIT_STATUS,
                serde_json::to_vec(&json!({
                    "id": legacy_id,
                    "status": "adoption-recorded",
                    "adopted_from": {
                        "unit": "some-other-run-s-unrelated-baseline",
                        "tip": legacy_tip,
                        "spec": "specs/some-unrelated-old-spec.md",
                    },
                }))
                .unwrap(),
            )],
        )
        .unwrap();

    // A LATER, wholly unrelated run reuses the SAME literal id for a criterion nothing
    // above has ever served - the exact reused-slug shape test 10 drives, but against a
    // legacy-shaped mark rather than an explicit, well-formed mismatch.
    let criterion = "the gauge independently reports its own reading on every cycle";
    let spec_path = "specs/91-an-unrelated-later-spec.md";
    start_fresh(&store, &[criterion.to_string()], "", "", "", spec_path).unwrap();
    let driver = ProposesSlugDriver {
        proposed_id: legacy_id.to_string(),
        criterion: criterion.to_string(),
        worker_write: Some((
            "own-work.txt".into(),
            "genuinely new, unrelated work\n".into(),
        )),
        gates: Vec::new(),
    };
    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &ExecRunner,
        repo: repo.path().to_str().unwrap().to_string(),
        grounder: None,
        graph: None,
        criteria: vec![criterion.to_string()],
    };
    let rs = run(&fresh_run_cfg("true"), &deps).unwrap();
    assert_eq!(
        rs.units[legacy_id].status,
        ledger::Status::Integrated,
        "reusing a literal id over a legacy mark must still run its ordinary lifecycle \
         through to integration"
    );
    assert!(repo.path().join("own-work.txt").exists());

    let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    let started = find_unit_started(&events, legacy_id);
    assert_eq!(
        started["adopted_from"],
        Value::Null,
        "a durable mark missing criterion_id/spec entirely (the pre-round-5 shape) must \
         never be read as a match for an unrelated criterion just because its `id` and \
         `status` fields happen to coincide - defaulting the missing fields to \"\" must \
         never accidentally equal a real criterion_stable_id: {started}"
    );
}

/// Test 12 (round 6, PRIMARY BLOCKER fix): round 5 closed the METADATA layer
/// (`recorded_adoption`/`adoption_provenance_key` triple-keying, test 10/11 above) but left
/// the GIT-CONTENT-REUSE mechanism completely untouched
/// (sdet-u88c2-r5-stage-worktree-branch-reuse-crosses-specs,
/// adv-u88c2-r5-independently-confirms-branch-reuse-crosses-specs,
/// arch-u88c2-r5-branch-exists-fallback-still-bare-id-crosses-specs, upheld ADJUDICATOR
/// VERDICT round 5): `stage_worktree` (src/conductor.rs) calls `Worktree::create`
/// UNCONDITIONALLY on every unit with the bare `unit_branch(&st.name)` name, regardless of
/// what `adopt_prior_criterion_branch` decided - and `Worktree::create`'s own
/// branch-exists fallback (src/worktree.rs) checks out and REUSES whatever real content
/// already sits on that literal branch name, with NO criterion/spec check of its own.
///
/// This drives the EXACT shape the round-5 reject named as still missing
/// (sdet-u88c2-r5-ruling-fixture-b-not-driven): test 10 above only ever reuses a slug
/// AFTER an intermediate legitimate-adoption-then-integrate-then-GC step, so
/// `rigger/u/reused-id` genuinely does not exist by the time it is reused and the bug
/// never fires. Here the FIRST run's unit ESCALATES (never integrates, so its branch is
/// NEVER reclaimed by `gc_integrated_branches` - test 3's own proof of that reclaim only
/// ever fires for `Integrated` units) and the SECOND run's planner reuses that exact
/// literal slug directly, with no adoption ever legitimately decided for it (a wholly
/// different criterion, under a wholly different spec) - the precise "escalated,
/// unreclaimed branch coinciding with an unrelated later reuse" construction the round-5
/// reject's own probe used.
///
/// `adopted_from` reading `Null` is NOT sufficient proof by itself (the pre-fix code
/// already read `Null` here too - `adopt_prior_criterion_branch`'s `branch_exists` guard
/// short-circuits to `Ok(None)` before ever deciding an adoption, exactly as its own doc
/// comment always described the "common repeat case"). The decisive assertion is that
/// spec A's escalated, unrelated content must never ride into spec B's unit's own tree:
/// spec B's unit never adopted anything, so its own tree must be built from HEAD alone,
/// not from the coincidentally-named branch's stale committed content.
#[test]
fn an_escalated_units_unreclaimed_branch_is_never_reused_by_an_unrelated_specs_slug_collision() {
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let store = Store::open(":memory:").unwrap();

    let criterion_x = "the turbine reports its own rotation speed continuously";
    let spec_a = "specs/88-a-unit-lineage-is-durable.md";

    // RUN 1 (spec A, criterion X): a planner proposes a literal slug directly (never the
    // deterministic baseline path), so THIS test controls the exact string RUN 2 reuses,
    // with no intermediate adoption step. Its gate always fails, so it exhausts
    // remediation and escalates - real committed work, never integrated, never GC'd.
    start_fresh(&store, &[criterion_x.to_string()], "", "", "", spec_a).unwrap();
    let shared_slug = "escalated-then-collided-slug";
    let shared_branch = format!("rigger/u/{shared_slug}");
    let driver1 = ProposesSlugDriver {
        proposed_id: shared_slug.to_string(),
        criterion: criterion_x.to_string(),
        worker_write: Some((
            "spec-a-secret.txt".into(),
            "spec A's escalated, unrelated secret\n".into(),
        )),
        gates: vec!["gate".to_string()],
    };
    let deps1 = Deps {
        store: &store,
        driver: &driver1,
        gates: &ExecRunner,
        repo: repo.path().to_str().unwrap().to_string(),
        grounder: None,
        graph: None,
        criteria: vec![criterion_x.to_string()],
    };
    let mut cfg1 = fresh_run_cfg("false");
    cfg1.workflow.defaults.max_retries = 1;
    let rs1 = run(&cfg1, &deps1).unwrap();
    assert_eq!(
        rs1.units[shared_slug].status,
        ledger::Status::Escalated,
        "an always-failing gate must exhaust remediation and escalate, never integrate; \
         units: {:?}",
        rs1.units.keys().collect::<Vec<_>>()
    );
    assert!(
        worktree::branch_exists(repo.path().to_str().unwrap(), &shared_branch),
        "the escalated unit's durable branch must exist, unreclaimed, before RUN 2 - \
         gc_integrated_branches only ever reclaims an Integrated unit's branch"
    );
    let shown = git_out(
        repo.path(),
        &["show", &format!("{shared_branch}:spec-a-secret.txt")],
    )
    .expect("the escalated branch must carry spec A's real committed secret");
    assert_eq!(shown.trim(), "spec A's escalated, unrelated secret");
    assert!(
        !repo.path().join("spec-a-secret.txt").exists(),
        "the escalated unit never integrated, so its secret must not be on the base yet"
    );

    // RUN 2 (spec B, an UNRELATED criterion Y): a planner independently reuses the exact
    // same literal slug for a wholly unrelated criterion under a wholly unrelated spec -
    // no adoption is ever legitimately decided for this pair (differing criterion AND
    // spec), yet the literal branch name collides.
    let criterion_y = "the compressor independently reports its own duty cycle on every poll";
    let spec_b = "specs/90-hermetic-test-git-and-merge-friendly-audit-artifacts.md";
    start_fresh(&store, &[criterion_y.to_string()], "", "", "", spec_b).unwrap();
    let driver2 = ProposesSlugDriver {
        proposed_id: shared_slug.to_string(),
        criterion: criterion_y.to_string(),
        worker_write: Some((
            "run2-own-work.txt".into(),
            "spec B's own genuinely new work\n".into(),
        )),
        gates: Vec::new(),
    };
    let deps2 = Deps {
        store: &store,
        driver: &driver2,
        gates: &ExecRunner,
        repo: repo.path().to_str().unwrap().to_string(),
        grounder: None,
        graph: None,
        criteria: vec![criterion_y.to_string()],
    };
    let rs2 = run(&fresh_run_cfg("true"), &deps2).unwrap();
    assert_eq!(
        rs2.units[shared_slug].status,
        ledger::Status::Integrated,
        "spec B's own unit must run its ordinary lifecycle through to integration; units: {:?}",
        rs2.units.keys().collect::<Vec<_>>()
    );

    // THE DECISIVE ASSERTION: spec A's escalated, unrelated content must NEVER ride into
    // spec B's unit's own tree merely because the two planners happened to reuse the same
    // literal slug - regardless of what `adopted_from` reads.
    assert!(
        !repo.path().join("spec-a-secret.txt").exists(),
        "spec B's unit must never inherit spec A's escalated, unrelated content just \
         because its planner reused the same literal slug - this is the primary blocker's \
         exact contamination: the git-content-reuse mechanism (stage_worktree's \
         unconditional Worktree::create call) bypassing the criterion/spec check entirely, \
         with adopted_from reading Null throughout"
    );
    assert!(
        repo.path().join("run2-own-work.txt").exists(),
        "spec B's own genuinely new work must still land on the base"
    );

    let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    let run2_started = find_last_unit_started(&events, shared_slug);
    assert_eq!(
        run2_started["adopted_from"],
        Value::Null,
        "spec B's unit legitimately adopted nothing (differing criterion AND spec) - this \
         alone was already true before the fix and is not sufficient proof on its own; \
         paired with the content assertion above it confirms the fix acts on the git side, \
         not the metadata side: {run2_started}"
    );
}

/// Test 13 (round 7, closing
/// `adv-u88c2-r6-quarantine-orphans-the-criterions-own-future-adoption` per operator ruling
/// `op-u88c2-round-7-definition-of-done-after-resume` item 1): round 6's quarantine
/// (`branch_is_foreign` -> `create_branch_at` the orphaned ref -> `delete_branch` the
/// canonical name) correctly stopped cross-spec CONTENT CONTAMINATION (test 12) but left the
/// move entirely UNRECORDED - `prior_criterion_unit` still (correctly) names the same bare
/// unit id for a LATER, genuine retry of the exact criterion/spec that content was itself
/// started under, but `unit_branch(prior)` now names the DELETED canonical ref, and the old
/// `let Ok(tip) = branch_tip(prior_branch) else { return Ok(None) }` swallowed that Not
/// Found as "nothing to adopt", silently discarding real reviewed history the harness
/// itself had just moved aside - defeating spec 88's own Operator rule ("a unit's reviewed
/// history is never discarded by the harness") for the exact criterion the quarantine had
/// just fired against.
///
/// THREE real `conductor::run` calls sharing one store and one real git repo:
/// - RUN 1 (spec A, criterion X): escalates with real committed work, never integrated,
///   never GC'd - mirrors test 12's own setup exactly.
/// - RUN 2 (spec B, an unrelated criterion Y): reuses RUN 1's exact literal slug, triggering
///   the round-6 quarantine with ZERO contamination (test 12's own guarantee, reproven here
///   as a precondition before the real assertion below).
/// - RUN 3 (spec A again, criterion X again, a NEW planner slug): the realistic retry spec
///   88 exists for - `prior_criterion_unit` names RUN 1's unit id as the candidate again,
///   its canonical branch is gone, so this round's fix must resolve the durable quarantine
///   record instead of silently starting fresh. Must ADOPT: RUN 1's real file lands on the
///   base, and `adopted_from` names RUN 1's real unit and tip.
#[test]
fn a_genuine_retry_of_a_quarantined_criterion_adopts_from_the_quarantine_ref() {
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let store = Store::open(":memory:").unwrap();

    let criterion_x = "the boiler reports its own internal temperature continuously";
    let spec_a = "specs/88-a-unit-lineage-is-durable.md";

    // RUN 1 (spec A, criterion X): escalates with real committed work, never integrated,
    // never GC'd - mirrors test 12's own setup exactly.
    start_fresh(&store, &[criterion_x.to_string()], "", "", "", spec_a).unwrap();
    let shared_slug = "quarantine-retry-original-slug";
    let shared_branch = format!("rigger/u/{shared_slug}");
    let driver1 = ProposesSlugDriver {
        proposed_id: shared_slug.to_string(),
        criterion: criterion_x.to_string(),
        worker_write: Some((
            "criterion-x-work.txt".into(),
            "criterion X's real, reviewed, still-abandoned work\n".into(),
        )),
        gates: vec!["gate".to_string()],
    };
    let deps1 = Deps {
        store: &store,
        driver: &driver1,
        gates: &ExecRunner,
        repo: repo.path().to_str().unwrap().to_string(),
        grounder: None,
        graph: None,
        criteria: vec![criterion_x.to_string()],
    };
    let mut cfg1 = fresh_run_cfg("false");
    cfg1.workflow.defaults.max_retries = 1;
    let rs1 = run(&cfg1, &deps1).unwrap();
    assert_eq!(
        rs1.units[shared_slug].status,
        ledger::Status::Escalated,
        "an always-failing gate must exhaust remediation and escalate, never integrate"
    );
    let prior_tip = git_out(repo.path(), &["rev-parse", &shared_branch])
        .expect("the escalated unit's durable branch must exist with a resolvable tip");

    // RUN 2 (spec B, an UNRELATED criterion Y): reuses the exact same literal slug, with
    // no adoption ever legitimately decided for it - triggers the round-6 quarantine.
    let criterion_y = "the injector independently reports its own duty cycle on every poll";
    let spec_b = "specs/90-hermetic-test-git-and-merge-friendly-audit-artifacts.md";
    start_fresh(&store, &[criterion_y.to_string()], "", "", "", spec_b).unwrap();
    let driver2 = ProposesSlugDriver {
        proposed_id: shared_slug.to_string(),
        criterion: criterion_y.to_string(),
        worker_write: Some((
            "run2-own-work.txt".into(),
            "spec B's own genuinely new work\n".into(),
        )),
        gates: Vec::new(),
    };
    let deps2 = Deps {
        store: &store,
        driver: &driver2,
        gates: &ExecRunner,
        repo: repo.path().to_str().unwrap().to_string(),
        grounder: None,
        graph: None,
        criteria: vec![criterion_y.to_string()],
    };
    let rs2 = run(&fresh_run_cfg("true"), &deps2).unwrap();
    assert_eq!(
        rs2.units[shared_slug].status,
        ledger::Status::Integrated,
        "spec B's own unit must run its ordinary lifecycle through to integration"
    );
    assert!(
        !worktree::branch_exists(repo.path().to_str().unwrap(), &shared_branch),
        "the round-6 quarantine must have deleted the canonical name once its foreign \
         content was moved aside"
    );
    assert!(
        !repo.path().join("criterion-x-work.txt").exists(),
        "spec B's unit must never inherit spec A's escalated, unrelated content - test 12's \
         own contamination guarantee, reproven here as a precondition of this test's real \
         assertion below"
    );

    // RUN 3 (spec A AGAIN, criterion X AGAIN, a NEW planner slug): the realistic retry -
    // `prior_criterion_unit` still names `shared_slug` for this exact (criterion, spec),
    // but its canonical branch is gone (quarantined above). This round's fix must resolve
    // the durable quarantine record instead of reading the deleted name as "never existed".
    start_fresh(&store, &[criterion_x.to_string()], "", "", "", spec_a).unwrap();
    let retry_slug = "quarantine-retry-new-slug";
    let driver3 = ProposesSlugDriver {
        proposed_id: retry_slug.to_string(),
        criterion: criterion_x.to_string(),
        worker_write: Some((
            "run3-own-work.txt".into(),
            "the retry's own genuinely new work\n".into(),
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
        criteria: vec![criterion_x.to_string()],
    };
    let rs3 = run(&fresh_run_cfg("true"), &deps3).unwrap();
    assert_eq!(
        rs3.units[retry_slug].status,
        ledger::Status::Integrated,
        "the genuine retry must run its ordinary lifecycle through to integration; units: {:?}",
        rs3.units.keys().collect::<Vec<_>>()
    );

    assert!(
        repo.path().join("criterion-x-work.txt").exists(),
        "the genuine retry of criterion X's own (criterion, spec) must recover its real, \
         reviewed work from the quarantine ref rather than silently starting fresh - spec \
         88's own Operator rule (\"a unit's reviewed history is never discarded by the \
         harness\") applied to the harness's OWN quarantine move, not just an external \
         operator's manual pruning"
    );
    assert!(repo.path().join("run3-own-work.txt").exists());

    let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    let retry_started = find_unit_started(&events, retry_slug);
    let adopted_from = &retry_started["adopted_from"];
    assert_eq!(
        adopted_from["unit"].as_str(),
        Some(shared_slug),
        "the retry must record its adoption as coming from RUN 1's real unit id: {retry_started}"
    );
    assert_eq!(
        adopted_from["tip"].as_str(),
        Some(prior_tip.as_str()),
        "the retry must adopt at RUN 1's own real tip - resolved off the quarantine ref, \
         never a different or stale sha: {retry_started}"
    );
}

/// Test 14 (round 7, closing `sdet-u88c2-r6-quarantine-crash-window-permanent-wedge` per
/// operator ruling `op-u88c2-round-7-definition-of-done-after-resume` item 2): the round-6
/// quarantine sequence (`create_branch_at(quarantine, tip)` then `delete_branch(branch)`)
/// had no `branch_exists(quarantine)` guard before the first call - unlike the sibling
/// adoption call site's own `!branch_exists` guard a few lines above it in the SAME
/// function - so a real crash between the two git calls left a resumed retry recomputing
/// the IDENTICAL deterministic `(unit_id, tip)` pair and hard-erroring on git's own "branch
/// already exists" refusal: a PERMANENT wedge, since every subsequent retry recomputes the
/// same inputs and fails identically, until a human resolved the git state by hand.
///
/// Reproduces the exact crash state directly (no public API can interrupt
/// `adopt_prior_criterion_branch` mid-call - mirrors tests 7/8's own technique): the
/// quarantine ref is pre-created via the SAME production `Worktree::create_branch_at` the
/// fix itself uses, at the SAME deterministic name (`quarantine_branch_name`'s own
/// `rigger/orphaned/<id>-<12-char-tip>` grammar), while the foreign canonical branch is
/// left in place (the delete never ran) - then a real second `run()` drives the identical
/// collision that would have triggered the original (pre-crash) quarantine attempt. Pre-fix
/// this hard-errors the whole run; post-fix the guard lets the resumed retry complete the
/// deferred rename instead of wedging.
#[test]
fn a_crash_between_the_quarantine_rename_and_the_canonical_delete_completes_on_a_resumed_retry() {
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let store = Store::open(":memory:").unwrap();

    let criterion_x = "the condenser reports its own coolant flow rate continuously";
    let spec_a = "specs/88-a-unit-lineage-is-durable.md";

    start_fresh(&store, &[criterion_x.to_string()], "", "", "", spec_a).unwrap();
    let shared_slug = "crash-window-shared-slug";
    let shared_branch = format!("rigger/u/{shared_slug}");
    let driver1 = ProposesSlugDriver {
        proposed_id: shared_slug.to_string(),
        criterion: criterion_x.to_string(),
        worker_write: Some((
            "criterion-x-work.txt".into(),
            "criterion X's real, reviewed, still-abandoned work\n".into(),
        )),
        gates: vec!["gate".to_string()],
    };
    let deps1 = Deps {
        store: &store,
        driver: &driver1,
        gates: &ExecRunner,
        repo: repo.path().to_str().unwrap().to_string(),
        grounder: None,
        graph: None,
        criteria: vec![criterion_x.to_string()],
    };
    let mut cfg1 = fresh_run_cfg("false");
    cfg1.workflow.defaults.max_retries = 1;
    let rs1 = run(&cfg1, &deps1).unwrap();
    assert_eq!(
        rs1.units[shared_slug].status,
        ledger::Status::Escalated,
        "an always-failing gate must exhaust remediation and escalate, never integrate"
    );
    let prior_tip = git_out(repo.path(), &["rev-parse", &shared_branch])
        .expect("the escalated unit's durable branch must exist with a resolvable tip");

    // Reproduce the CRASH STATE directly: the quarantine ref already exists at the
    // foreign branch's own tip (the first git call succeeded), but the canonical branch
    // is still there too (the second git call - the delete - never ran).
    let quarantine_branch = format!("rigger/orphaned/{shared_slug}-{}", &prior_tip[..12]);
    Worktree::create_branch_at(
        repo.path().to_str().unwrap(),
        &quarantine_branch,
        &prior_tip,
    )
    .unwrap();
    assert!(
        worktree::branch_exists(repo.path().to_str().unwrap(), &shared_branch),
        "the simulated crash must leave the canonical branch NOT YET deleted"
    );

    // RESUME: a real second `run()` call reuses the identical literal slug for an
    // unrelated criterion/spec - the SAME collision that would have driven the original
    // (pre-crash) quarantine attempt - exactly what a fresh `rigger step` process
    // recomputes after the crash.
    let criterion_y = "the fan independently reports its own duty cycle on every poll";
    let spec_b = "specs/90-hermetic-test-git-and-merge-friendly-audit-artifacts.md";
    start_fresh(&store, &[criterion_y.to_string()], "", "", "", spec_b).unwrap();
    let driver2 = ProposesSlugDriver {
        proposed_id: shared_slug.to_string(),
        criterion: criterion_y.to_string(),
        worker_write: Some((
            "run2-own-work.txt".into(),
            "spec B's own genuinely new work\n".into(),
        )),
        gates: Vec::new(),
    };
    let deps2 = Deps {
        store: &store,
        driver: &driver2,
        gates: &ExecRunner,
        repo: repo.path().to_str().unwrap().to_string(),
        grounder: None,
        graph: None,
        criteria: vec![criterion_y.to_string()],
    };
    let rs2 = run(&fresh_run_cfg("true"), &deps2).expect(
        "a resumed retry recomputing the identical (unit_id, tip) quarantine ref must \
         complete the deferred rename, never hard-error on git's own \"branch already \
         exists\" refusal - the permanent wedge \
         sdet-u88c2-r6-quarantine-crash-window-permanent-wedge reproduced",
    );
    assert_eq!(
        rs2.units[shared_slug].status,
        ledger::Status::Integrated,
        "the resumed unit must still run its ordinary lifecycle through to integration"
    );
    assert!(
        !worktree::branch_exists(repo.path().to_str().unwrap(), &shared_branch),
        "the resumed retry must complete the deferred delete of the canonical name"
    );
    assert!(
        worktree::branch_exists(repo.path().to_str().unwrap(), &quarantine_branch),
        "the pre-existing quarantine ref must survive untouched, never re-created or lost"
    );
    assert!(repo.path().join("run2-own-work.txt").exists());
    assert!(
        !repo.path().join("criterion-x-work.txt").exists(),
        "the foreign content must never ride into spec B's unit's own tree"
    );
}

/// Test 15 (round 7, sdet-author re-enumeration: new-public-API probe on
/// `adopt_prior_criterion_branch`'s SECOND `branch_tip` call site, the one round 7 itself
/// introduced): round 7's own commit message names the contract precisely - "a `branch_tip`
/// failure on a ref this call positively knows should exist is now a real Error ..., never
/// another silent `Ok(None)`. Only the genuinely-never-existed case (no canonical branch and
/// no quarantine record) still returns `Ok(None)`." Test 9 (round 4) already proves the
/// FIRST call site's precondition (`branch_exists(prior_branch) == false`, no quarantine
/// record either) still degrades to the old, unchanged `Ok(None)` contract. Tests 13 and 14
/// both drive the quarantine-ref arm's SUCCESS path only (the ref they resolve always
/// actually exists). Neither exercises the one arm round 7 changed the contract for: the
/// durable `STATUS_BRANCH_QUARANTINED` record survives (so `quarantined_branch` resolves
/// `Some(ref)`, correctly - `prior_criterion_unit` still names this criterion's real unit),
/// but the ref itself is gone (an operator's manual `git branch -D` of the orphaned ref,
/// unaware the durable record still names it, or any other process that pruned it). This is
/// the load-bearing case for spec 88's own Operator rule ("a unit's reviewed history is
/// never discarded by the harness"): a silent `Ok(None)` here would read identically to
/// "this criterion was simply never attempted," discarding the harness's own record that
/// real reviewed work exists - never distinguishable from data loss by the unit's own
/// operator. A real, durably-logged `Error` is the only outcome that keeps that promise.
///
/// THREE real `conductor::run` calls, identical RUN 1/RUN 2 setup to test 13's own (an
/// escalated baseline with real committed work, then an unrelated criterion/spec collision
/// that fires the round-6 quarantine) - then, before RUN 3, the quarantine ref RUN 2 just
/// created is deleted directly via `git branch -D`, leaving the durable `UnitStatus` mark
/// pointing at a ref that no longer resolves. RUN 3 drives the identical genuine retry test
/// 13 proves ADOPTS when the ref is intact; here it must hard-error instead: `run()` itself
/// returns `Err` (proven at the real boundary, never by calling the private function
/// directly), the retry's own `UnitStarted` must never be written (the `?` fires inside
/// `start_and_run_stage`, strictly BEFORE its `UnitStarted` emit - so a crash-resumed retry
/// never sees a half-recorded unit either), and the wave's ordinary generic-error arm must
/// still leave its usual durable lesson naming the failed stage - the failure stays legible
/// in the log exactly like any other stage error, never a swallowed failure invisible to the
/// operator.
#[test]
fn a_quarantine_record_whose_ref_was_since_deleted_hard_errors_instead_of_silently_starting_fresh()
{
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let store = Store::open(":memory:").unwrap();

    let criterion_x = "the reactor reports its own core pressure continuously";
    let spec_a = "specs/88-a-unit-lineage-is-durable.md";

    // RUN 1 (spec A, criterion X): escalates with real committed work, never integrated,
    // never GC'd - identical shape to test 13's own RUN 1.
    start_fresh(&store, &[criterion_x.to_string()], "", "", "", spec_a).unwrap();
    let shared_slug = "deleted-quarantine-ref-original-slug";
    let shared_branch = format!("rigger/u/{shared_slug}");
    let driver1 = ProposesSlugDriver {
        proposed_id: shared_slug.to_string(),
        criterion: criterion_x.to_string(),
        worker_write: Some((
            "criterion-x-work.txt".into(),
            "criterion X's real, reviewed, still-abandoned work\n".into(),
        )),
        gates: vec!["gate".to_string()],
    };
    let deps1 = Deps {
        store: &store,
        driver: &driver1,
        gates: &ExecRunner,
        repo: repo.path().to_str().unwrap().to_string(),
        grounder: None,
        graph: None,
        criteria: vec![criterion_x.to_string()],
    };
    let mut cfg1 = fresh_run_cfg("false");
    cfg1.workflow.defaults.max_retries = 1;
    let rs1 = run(&cfg1, &deps1).unwrap();
    assert_eq!(
        rs1.units[shared_slug].status,
        ledger::Status::Escalated,
        "an always-failing gate must exhaust remediation and escalate, never integrate"
    );
    let prior_tip = git_out(repo.path(), &["rev-parse", &shared_branch])
        .expect("the escalated unit's durable branch must exist with a resolvable tip");

    // RUN 2 (spec B, an UNRELATED criterion Y): reuses the exact same literal slug, firing
    // the round-6 quarantine - moves the foreign content aside and (round 7) durably
    // records the quarantine ref's identity.
    let criterion_y = "the pump independently reports its own duty cycle on every poll";
    let spec_b = "specs/90-hermetic-test-git-and-merge-friendly-audit-artifacts.md";
    start_fresh(&store, &[criterion_y.to_string()], "", "", "", spec_b).unwrap();
    let driver2 = ProposesSlugDriver {
        proposed_id: shared_slug.to_string(),
        criterion: criterion_y.to_string(),
        worker_write: Some((
            "run2-own-work.txt".into(),
            "spec B's own genuinely new work\n".into(),
        )),
        gates: Vec::new(),
    };
    let deps2 = Deps {
        store: &store,
        driver: &driver2,
        gates: &ExecRunner,
        repo: repo.path().to_str().unwrap().to_string(),
        grounder: None,
        graph: None,
        criteria: vec![criterion_y.to_string()],
    };
    let rs2 = run(&fresh_run_cfg("true"), &deps2).unwrap();
    assert_eq!(
        rs2.units[shared_slug].status,
        ledger::Status::Integrated,
        "spec B's own unit must run its ordinary lifecycle through to integration"
    );
    let quarantine_branch = format!("rigger/orphaned/{shared_slug}-{}", &prior_tip[..12]);
    assert!(
        worktree::branch_exists(repo.path().to_str().unwrap(), &quarantine_branch),
        "the round-6 quarantine must have created the orphaned ref before this test deletes \
         it"
    );

    // Delete the quarantine ref itself - the durable STATUS_BRANCH_QUARANTINED record
    // still names it, but the git ref it points to is now gone (an operator's manual
    // cleanup, unaware of the record; or any other process that pruned it).
    assert!(Command::new("git")
        .arg("-C")
        .arg(repo.path())
        .args(["branch", "-D", &quarantine_branch])
        .status()
        .unwrap()
        .success());
    assert!(
        !worktree::branch_exists(repo.path().to_str().unwrap(), &quarantine_branch),
        "the quarantine ref must be genuinely gone before RUN 3 - the record now names a ref \
         that no longer resolves"
    );

    // RUN 3: the identical genuine retry test 13 proves ADOPTS when the quarantine ref is
    // intact. Here `quarantined_branch` still resolves `Some(quarantine_branch)` (the
    // durable record was never touched), but `branch_tip` on that ref must now fail - and
    // that failure must propagate as a real `Error`, never silently degrade to "nothing to
    // adopt, start fresh".
    start_fresh(&store, &[criterion_x.to_string()], "", "", "", spec_a).unwrap();
    let retry_slug = "deleted-quarantine-ref-retry-slug";
    let driver3 = ProposesSlugDriver {
        proposed_id: retry_slug.to_string(),
        criterion: criterion_x.to_string(),
        worker_write: Some((
            "run3-own-work.txt".into(),
            "the retry's own genuinely new work\n".into(),
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
        criteria: vec![criterion_x.to_string()],
    };
    // `RunState` (the `Ok` type) does not implement `Debug`, so `expect_err` cannot be
    // used here - match explicitly instead (mirrors tests/build_env_authority_periphery.rs).
    match run(&fresh_run_cfg("true"), &deps3) {
        Err(_) => {}
        Ok(_) => panic!(
            "a durably-recorded quarantine ref that no longer resolves must hard-error, \
             never silently degrade to Ok(None) and start the retry fresh - discarding the \
             harness's own record that real reviewed history exists"
        ),
    }

    let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    assert!(
        !events.iter().any(|e| {
            if e.type_ != ledger::TYPE_UNIT_STARTED {
                return false;
            }
            let Ok(body) = serde_json::from_slice::<Value>(&e.data) else {
                return false;
            };
            body.get("id").and_then(Value::as_str) == Some(retry_slug)
        }),
        "the hard error fires before start_and_run_stage's own UnitStarted emit - the retry \
         unit must never be half-recorded, so a crash-resumed process never mistakes it for \
         an already-started unit"
    );
    // The whole-stream fold also carries RUN 1's own unrelated escalation lesson - match
    // on the retry stage's own name, never just "some LessonLearned exists somewhere in
    // the log", so this proves the failure was attributed to the right unit.
    assert!(
        events.iter().any(|e| {
            if e.type_ != contextgraph::TYPE_LESSON_LEARNED {
                return false;
            }
            let Ok(body) = serde_json::from_slice::<Value>(&e.data) else {
                return false;
            };
            body["summary"]
                .as_str()
                .unwrap_or_default()
                .contains(retry_slug)
        }),
        "the wave's ordinary generic-error arm must still leave its usual durable lesson \
         naming the failing stage - this failure must stay legible in the log exactly like \
         any other stage error, never a swallowed failure invisible to the operator"
    );
    assert!(
        !repo.path().join("run3-own-work.txt").exists(),
        "the retry's own implementer must never even spawn once its adoption decision \
         hard-errors"
    );
}

/// A wrapping `EventStore` double that fails, with a real backend `StoreError`, the FIRST
/// append batch matching `matches` - then delegates that same call, and every other call
/// (every other append included), straight through to the real store underneath. This
/// reproduces an in-process `emit_keyed_meta` failure (a transient store write error is
/// exactly as real a cause as an OS-level crash - both leave the SAME half-done state
/// behind) at the EXACT statement production code calls it, without touching any other
/// write in the same `run()` call - the technique test 16 below uses to prove WHERE in the
/// real call sequence the quarantine record lands relative to the canonical branch's
/// deletion, which no amount of hand-constructing before/after states (tests 13-15's own
/// technique) can observe: those tests can only ever probe states this file chooses to
/// construct, never the actual order two real, sequential statements execute in.
struct FailsOnceOn<'a> {
    inner: &'a Store,
    matches: fn(&Event) -> bool,
    fired: AtomicBool,
}

impl EventStore for FailsOnceOn<'_> {
    fn append(
        &self,
        stream: &str,
        expected: ExpectedRevision,
        events: &[Event],
    ) -> Result<rigger::eventstore::Appended, StoreError> {
        if events.iter().any(|e| (self.matches)(e))
            && self
                .fired
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
        {
            return Err(StoreError::Backend(
                "simulated write failure (test double FailsOnceOn, fires once)".into(),
            ));
        }
        self.inner.append(stream, expected, events)
    }

    fn read_stream(
        &self,
        stream: &str,
        from: Revision,
        dir: Direction,
    ) -> Result<Vec<Event>, StoreError> {
        self.inner.read_stream(stream, from, dir)
    }

    fn read_all(
        &self,
        from: Position,
        dir: Direction,
        filter: &Filter,
    ) -> Result<Vec<Event>, StoreError> {
        self.inner.read_all(from, dir, filter)
    }

    fn subscribe_all(&self, from: Position, filter: &Filter) -> Result<Subscription, StoreError> {
        self.inner.subscribe_all(from, filter)
    }

    fn subscribe_stream(&self, stream: &str, from: Revision) -> Result<Subscription, StoreError> {
        self.inner.subscribe_stream(stream, from)
    }
}

/// Whether `e` is the durable `STATUS_BRANCH_QUARANTINED` mark
/// `adopt_prior_criterion_branch`'s quarantine path writes - matched on the wire shape
/// (`UnitStatus` carrying `"status": "branch-quarantined"`), a literal mirroring the
/// private `STATUS_BRANCH_QUARANTINED` constant exactly the way test 8's own hand-built
/// event above matches `"adoption-recorded"` literally - never imported, since this is a
/// black-box periphery test asserting the WIRE contract, not the implementation's own
/// naming.
fn is_quarantine_record_write(e: &Event) -> bool {
    if e.type_ != ledger::TYPE_UNIT_STATUS {
        return false;
    }
    serde_json::from_slice::<Value>(&e.data)
        .ok()
        .and_then(|v| v.get("status").and_then(Value::as_str).map(str::to_string))
        == Some("branch-quarantined".to_string())
}

/// Test 16 (round 8, closing `arch-u88c2-r7-quarantine-record-write-ordered-after-git-not-
/// before` / `sdet-u88c2-r6-record-emit-crash-window-orphans-quarantine`, per operator
/// ruling `op-u88c2-round-8-definition-of-done-record-between-create-and-delete`): round 7
/// wrote the durable `STATUS_BRANCH_QUARANTINED` mark LAST - AFTER `delete_branch` had
/// already cleared this whole block's own re-entry gate (`branch_exists(canonical)`). A
/// crash, or any `emit_keyed_meta` failure (a transient store write error is just as real a
/// cause as an OS-level crash), landing between the delete succeeding and the emit
/// completing left the canonical branch gone, the orphaned ref real, and NO record ever
/// naming it - and because the gate that re-enters this whole block was already cleared by
/// the delete, a resumed retry of the SAME collision never even looks at this code again,
/// so nothing ever retries the dropped emit: a PERMANENT, silent loss of the quarantined
/// content's own future adoptability.
///
/// No public API can interrupt `adopt_prior_criterion_branch` mid-call to inject a real
/// crash at that exact statement boundary (tests 7/8/14/15's shared limitation, noted in
/// each of their own doc comments) - hand-constructing a before/after STATE, this file's
/// usual technique for that class of gap, cannot observe this specific defect either: it
/// tests what a resumed call does GIVEN a state, never which of two possible orderings a
/// single uninterrupted call actually executes writes in. So this test drives the ACTUAL
/// production statement sequence via `FailsOnceOn` above: a real second `run()` call, over
/// the identical round-6/7 slug-collision setup tests 13-15 already share, whose `Deps`
/// store is a thin wrapper that fails - with a real backend `StoreError`, not a panic - the
/// FIRST append matching the quarantine record's own wire shape, and delegates every other
/// append (including the git-independent bookkeeping the rest of the run performs) straight
/// through to a real, otherwise fully functional in-memory store.
///
/// Fixed (record before delete): the failing emit now runs BEFORE `delete_branch`, so the
/// run must fail with the canonical branch STILL PRESENT - no git mutation of it has
/// happened yet - and the quarantine ref, if the guarded create already ran, is harmless
/// and reusable. Pre-fix (record after delete): the delete already ran by the time the emit
/// fails, so the canonical branch would already be gone with no record surviving it - this
/// test's own primary assertion (`shared_branch` must still exist after the failed run)
/// fails against that ordering, proving it RED against round 7's own committed code before
/// this round's fix, GREEN after it.
///
/// A THIRD, ordinary `run()` (a real, non-failing store) then re-drives the identical
/// collision from that exact recovered state, proving the failure above cost nothing: the
/// collision completes cleanly to integration, the canonical branch is (now) actually
/// deleted, and a FOURTH `run()` - the genuine later retry of criterion X's own original
/// (criterion, spec), mirroring test 13's own final assertions - recovers its real, still
/// abandoned, reviewed work from the quarantine ref rather than silently starting fresh.
#[test]
fn a_store_failure_writing_the_quarantine_record_never_lets_the_canonical_branch_be_deleted_first()
{
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let store = Store::open(":memory:").unwrap();

    let criterion_x = "the turbine reports its own rotational speed continuously";
    let spec_a = "specs/88-a-unit-lineage-is-durable.md";

    // RUN 1 (spec A, criterion X): escalates with real committed work, never integrated,
    // never GC'd - identical shape to tests 13-15's own RUN 1.
    start_fresh(&store, &[criterion_x.to_string()], "", "", "", spec_a).unwrap();
    let shared_slug = "quarantine-write-failure-shared-slug";
    let shared_branch = format!("rigger/u/{shared_slug}");
    let driver1 = ProposesSlugDriver {
        proposed_id: shared_slug.to_string(),
        criterion: criterion_x.to_string(),
        worker_write: Some((
            "criterion-x-work.txt".into(),
            "criterion X's real, reviewed, still-abandoned work\n".into(),
        )),
        gates: vec!["gate".to_string()],
    };
    let deps1 = Deps {
        store: &store,
        driver: &driver1,
        gates: &ExecRunner,
        repo: repo.path().to_str().unwrap().to_string(),
        grounder: None,
        graph: None,
        criteria: vec![criterion_x.to_string()],
    };
    let mut cfg1 = fresh_run_cfg("false");
    cfg1.workflow.defaults.max_retries = 1;
    let rs1 = run(&cfg1, &deps1).unwrap();
    assert_eq!(
        rs1.units[shared_slug].status,
        ledger::Status::Escalated,
        "an always-failing gate must exhaust remediation and escalate, never integrate"
    );

    // RUN 2 (spec B, an UNRELATED criterion Y): reuses the exact same literal slug, firing
    // the round-6/7 quarantine - but this run's own store fails the quarantine record's
    // OWN append, once, with a real backend error.
    let criterion_y = "the compressor independently reports its own duty cycle on every poll";
    let spec_b = "specs/90-hermetic-test-git-and-merge-friendly-audit-artifacts.md";
    start_fresh(&store, &[criterion_y.to_string()], "", "", "", spec_b).unwrap();
    let failing_store = FailsOnceOn {
        inner: &store,
        matches: is_quarantine_record_write,
        fired: AtomicBool::new(false),
    };
    let driver2 = ProposesSlugDriver {
        proposed_id: shared_slug.to_string(),
        criterion: criterion_y.to_string(),
        worker_write: Some((
            "run2-own-work.txt".into(),
            "spec B's own genuinely new work\n".into(),
        )),
        gates: Vec::new(),
    };
    let deps2 = Deps {
        store: &failing_store,
        driver: &driver2,
        gates: &ExecRunner,
        repo: repo.path().to_str().unwrap().to_string(),
        grounder: None,
        graph: None,
        criteria: vec![criterion_y.to_string()],
    };
    // `RunState` (the `Ok` type) does not implement `Debug`, so `expect_err` cannot be
    // used here - match explicitly, exactly like test 15 above.
    match run(&fresh_run_cfg("true"), &deps2) {
        Err(_) => {}
        Ok(_) => panic!(
            "a store failure on the quarantine record's own append must propagate as a \
             real Error, never be silently absorbed"
        ),
    }

    // THE LOAD-BEARING ASSERTION: the canonical branch must still exist. Pre-fix (record
    // written AFTER delete_branch), the delete already ran by the time the emit above
    // failed, so this branch would already be gone with no record ever surviving it -
    // exactly the primary blocker's data-loss window. Post-fix (record written BEFORE
    // delete_branch), the failing emit runs before any git mutation of this branch, so it
    // must still be here, completely untouched.
    assert!(
        worktree::branch_exists(repo.path().to_str().unwrap(), &shared_branch),
        "the canonical branch must never be deleted before its own quarantine record has \
         durably landed - a store failure on the record write must leave the git side \
         effect entirely undone, not half-done"
    );
    let events_after_failure = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    assert!(
        !events_after_failure.iter().any(is_quarantine_record_write),
        "the failed append must not have landed a record either - this store failure must \
         leave EITHER both the record and the delete undone, or neither; never the delete \
         alone"
    );

    // RUN 3: retry the identical collision with an ORDINARY, non-failing store - the
    // recovered state must complete cleanly to integration, exactly as an uninterrupted
    // run would have.
    let driver3 = ProposesSlugDriver {
        proposed_id: shared_slug.to_string(),
        criterion: criterion_y.to_string(),
        worker_write: Some((
            "run2-own-work.txt".into(),
            "spec B's own genuinely new work\n".into(),
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
        criteria: vec![criterion_y.to_string()],
    };
    let rs3 = run(&fresh_run_cfg("true"), &deps3).unwrap();
    assert_eq!(
        rs3.units[shared_slug].status,
        ledger::Status::Integrated,
        "the retried collision must still run its ordinary lifecycle through to integration"
    );
    assert!(
        !worktree::branch_exists(repo.path().to_str().unwrap(), &shared_branch),
        "the retry must complete the quarantine's delete of the canonical name"
    );
    assert!(
        !repo.path().join("criterion-x-work.txt").exists(),
        "spec B's unit must never inherit spec A's escalated, unrelated content"
    );

    // RUN 4 (spec A AGAIN, criterion X AGAIN): the genuine later retry of the ORIGINAL
    // criterion/spec, mirroring test 13's own final assertions - proves the store failure
    // above cost nothing: the quarantined content is still fully recoverable.
    start_fresh(&store, &[criterion_x.to_string()], "", "", "", spec_a).unwrap();
    let retry_slug = "quarantine-write-failure-retry-slug";
    let driver4 = ProposesSlugDriver {
        proposed_id: retry_slug.to_string(),
        criterion: criterion_x.to_string(),
        worker_write: Some((
            "run4-own-work.txt".into(),
            "the retry's own genuinely new work\n".into(),
        )),
        gates: Vec::new(),
    };
    let deps4 = Deps {
        store: &store,
        driver: &driver4,
        gates: &ExecRunner,
        repo: repo.path().to_str().unwrap().to_string(),
        grounder: None,
        graph: None,
        criteria: vec![criterion_x.to_string()],
    };
    let rs4 = run(&fresh_run_cfg("true"), &deps4).unwrap();
    assert_eq!(
        rs4.units[retry_slug].status,
        ledger::Status::Integrated,
        "the genuine retry must run its ordinary lifecycle through to integration; units: {:?}",
        rs4.units.keys().collect::<Vec<_>>()
    );
    assert!(
        repo.path().join("criterion-x-work.txt").exists(),
        "the genuine retry of criterion X's own (criterion, spec) must still recover its \
         real, reviewed work from the quarantine ref - the earlier store failure while \
         writing that same record must never have cost the record its eventual durability"
    );
    assert!(repo.path().join("run4-own-work.txt").exists());
}

/// Test 17 (round 8, item 2 of operator ruling
/// `op-u88c2-round-8-definition-of-done-record-between-create-and-delete`): "ONE
/// crash-window fixture ... driving a crash after the record and before the delete
/// (resume completes the delete and adopts from the quarantine)". Test 14 above already
/// reproduces the OTHER half of this same crash window (a crash after the guarded
/// `create_branch_at` but before ANYTHING durable lands) and stays green unchanged under
/// round 8's reorder - this test reproduces the NEW half the reorder itself opens: the
/// durable `STATUS_BRANCH_QUARANTINED` record has already landed (round 8's own fix
/// writes it BEFORE `delete_branch`), but the canonical branch has not yet been deleted.
///
/// Mirrors test 14's own technique exactly, swapping which of the two writes is
/// pre-built: the quarantine ref is pre-created via the SAME production
/// `Worktree::create_branch_at` call the fix itself uses, at the SAME deterministic name,
/// and the durable mark is appended directly - carrying the SAME `META_REPLAY_KEY` meta a
/// real `emit_keyed_meta` call stamps (read back off the real `criterion_id` a real
/// `UnitStarted` recorded, never hand-typed, exactly like tests 7/8's own technique) - so
/// a resumed retry's own idempotent re-emit of the identical key is recognized as a
/// replay, never appends a duplicate record. The canonical branch is left in place (the
/// delete never ran): the simulated crash landed strictly between round 8's two
/// statements.
#[test]
fn a_crash_after_the_quarantine_record_but_before_the_canonical_delete_completes_the_delete_on_resume(
) {
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let store = Store::open(":memory:").unwrap();

    let criterion_x = "the generator reports its own output frequency continuously";
    let spec_a = "specs/88-a-unit-lineage-is-durable.md";

    // RUN 1 (spec A, criterion X): escalates with real committed work, never integrated,
    // never GC'd - identical shape to tests 13-16's own RUN 1.
    start_fresh(&store, &[criterion_x.to_string()], "", "", "", spec_a).unwrap();
    let shared_slug = "record-before-delete-shared-slug";
    let shared_branch = format!("rigger/u/{shared_slug}");
    let driver1 = ProposesSlugDriver {
        proposed_id: shared_slug.to_string(),
        criterion: criterion_x.to_string(),
        worker_write: Some((
            "criterion-x-work.txt".into(),
            "criterion X's real, reviewed, still-abandoned work\n".into(),
        )),
        gates: vec!["gate".to_string()],
    };
    let deps1 = Deps {
        store: &store,
        driver: &driver1,
        gates: &ExecRunner,
        repo: repo.path().to_str().unwrap().to_string(),
        grounder: None,
        graph: None,
        criteria: vec![criterion_x.to_string()],
    };
    let mut cfg1 = fresh_run_cfg("false");
    cfg1.workflow.defaults.max_retries = 1;
    let rs1 = run(&cfg1, &deps1).unwrap();
    assert_eq!(
        rs1.units[shared_slug].status,
        ledger::Status::Escalated,
        "an always-failing gate must exhaust remediation and escalate, never integrate"
    );
    let prior_tip = git_out(repo.path(), &["rev-parse", &shared_branch])
        .expect("the escalated unit's durable branch must exist with a resolvable tip");
    let events_after_run1 = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    let owner_criterion_id = find_unit_started(&events_after_run1, shared_slug)["criterion_id"]
        .as_str()
        .expect("the escalated unit's own UnitStarted carries its real criterion_id")
        .to_string();

    // RUN 2's own boundary (spec B, an UNRELATED criterion Y, the SAME literal slug)
    // opens FIRST, so the hand-built crash state below lands INSIDE it: `emit_keyed_meta`
    // seeds its in-process replay-key dedup from `crate::run::current_run`, scoped to the
    // CURRENT run's own boundary (never the whole historical stream) - unlike the
    // whole-stream READS `branch_owner`/`quarantined_branch` do - so a hand-built event
    // meant to simulate "this run already wrote this record before crashing" must be
    // appended AFTER this run's own `start_fresh`, exactly where a real crash mid-run
    // would have left it, for the resumed call's own idempotent re-emit to recognize it.
    let criterion_y = "the alternator independently reports its own duty cycle on every poll";
    let spec_b = "specs/90-hermetic-test-git-and-merge-friendly-audit-artifacts.md";
    start_fresh(&store, &[criterion_y.to_string()], "", "", "", spec_b).unwrap();

    // Reproduce the CRASH STATE directly: the durable quarantine RECORD has already
    // landed (round 8's own new ordering writes it BEFORE delete_branch) but the
    // canonical branch has NOT yet been deleted.
    let quarantine_branch = format!("rigger/orphaned/{shared_slug}-{}", &prior_tip[..12]);
    Worktree::create_branch_at(
        repo.path().to_str().unwrap(),
        &quarantine_branch,
        &prior_tip,
    )
    .unwrap();
    // `branch_owner` (src/conductor.rs) independently recomputes `owner_spec` off the
    // real `RunStarted` this test's own FIRST `start_fresh` call wrote for `spec_a` -
    // through `ledger::spec_stem` (`pub(crate)`, so unreachable from this black-box
    // periphery test) - never off this hand-built event's own "spec" field, so the value
    // here must match that STEMMED form exactly (Rust's own `Path::file_stem`: directory
    // and extension both dropped, no further sanitizing needed for this
    // already-alphanumeric name), the same way test 15's own comment mirrors the private
    // `STATUS_BRANCH_QUARANTINED` constant literally rather than importing it.
    let owner_spec_stem = "88-a-unit-lineage-is-durable";
    let replay_key = format!("{shared_slug}/{owner_criterion_id}/{owner_spec_stem}/quarantined");
    store
        .append(
            STREAM,
            ExpectedRevision::Any,
            &[Event::new(
                ledger::TYPE_UNIT_STATUS,
                serde_json::to_vec(&json!({
                    "id": shared_slug,
                    "status": "branch-quarantined",
                    "criterion_id": owner_criterion_id,
                    "spec": owner_spec_stem,
                    "quarantined_to": {"branch": quarantine_branch, "tip": prior_tip},
                }))
                .unwrap(),
            )
            .with_meta(META_REPLAY_KEY, replay_key.as_str())],
        )
        .unwrap();
    assert!(
        worktree::branch_exists(repo.path().to_str().unwrap(), &shared_branch),
        "the simulated crash must leave the canonical branch NOT YET deleted"
    );

    // RESUME: a real second `run()` call, same run boundary, reuses the identical
    // literal slug for the unrelated criterion Y - the SAME collision that would have
    // driven the original (pre-crash) quarantine attempt.
    let driver2 = ProposesSlugDriver {
        proposed_id: shared_slug.to_string(),
        criterion: criterion_y.to_string(),
        worker_write: Some((
            "run2-own-work.txt".into(),
            "spec B's own genuinely new work\n".into(),
        )),
        gates: Vec::new(),
    };
    let deps2 = Deps {
        store: &store,
        driver: &driver2,
        gates: &ExecRunner,
        repo: repo.path().to_str().unwrap().to_string(),
        grounder: None,
        graph: None,
        criteria: vec![criterion_y.to_string()],
    };
    let rs2 = run(&fresh_run_cfg("true"), &deps2).expect(
        "a resumed retry recomputing the identical (unit_id, tip) quarantine ref, with its \
         record already durably landed, must complete only the still-pending delete, \
         never hard-error and never duplicate the record",
    );
    assert_eq!(
        rs2.units[shared_slug].status,
        ledger::Status::Integrated,
        "the resumed unit must still run its ordinary lifecycle through to integration"
    );
    assert!(
        !worktree::branch_exists(repo.path().to_str().unwrap(), &shared_branch),
        "the resumed retry must complete the deferred delete of the canonical name"
    );
    assert!(
        worktree::branch_exists(repo.path().to_str().unwrap(), &quarantine_branch),
        "the pre-existing quarantine ref must survive untouched, never re-created or lost"
    );
    let events_after_run2 = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    assert_eq!(
        events_after_run2
            .iter()
            .filter(|e| is_quarantine_record_write(e))
            .count(),
        1,
        "the resumed retry's own idempotent re-emit must recognize the pre-landed record \
         as a replay under its shared META_REPLAY_KEY, never append a second, duplicate \
         quarantine record for the identical identity"
    );
    assert!(repo.path().join("run2-own-work.txt").exists());
    assert!(
        !repo.path().join("criterion-x-work.txt").exists(),
        "the foreign content must never ride into spec B's unit's own tree"
    );

    // RUN 3 (spec A AGAIN, criterion X AGAIN): the genuine later retry of the ORIGINAL
    // criterion/spec must still recover its real, reviewed work from the quarantine ref -
    // proving the crash between the record and the delete cost nothing.
    start_fresh(&store, &[criterion_x.to_string()], "", "", "", spec_a).unwrap();
    let retry_slug = "record-before-delete-retry-slug";
    let driver3 = ProposesSlugDriver {
        proposed_id: retry_slug.to_string(),
        criterion: criterion_x.to_string(),
        worker_write: Some((
            "run3-own-work.txt".into(),
            "the retry's own genuinely new work\n".into(),
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
        criteria: vec![criterion_x.to_string()],
    };
    let rs3 = run(&fresh_run_cfg("true"), &deps3).unwrap();
    assert_eq!(
        rs3.units[retry_slug].status,
        ledger::Status::Integrated,
        "the genuine retry must run its ordinary lifecycle through to integration; units: {:?}",
        rs3.units.keys().collect::<Vec<_>>()
    );
    assert!(
        repo.path().join("criterion-x-work.txt").exists(),
        "the genuine retry of criterion X's own (criterion, spec) must recover its real, \
         reviewed work from the quarantine ref even though a crash landed between the \
         record and the delete on the earlier round"
    );
    assert!(repo.path().join("run3-own-work.txt").exists());
}
