//! Periphery (contract / API / integration) tests for spec 88 criterion 4 - PLAN AMENDMENTS
//! LAND: a producer (`plan`) stage's own git commits reach the run branch at its DAG-terminal
//! integration point, instead of dying with the throwaway worktree the historical
//! `REVIEW_ONLY_NO_ARTIFACT` path never lands anywhere.
//!
//! Diff-grounded surface this file accounts for (base e387031 - this unit's own merge-base
//! with the run branch, excluding merged-in sibling work; the DecisionMade
//! `sdet-u88c4-surface-enumeration` carries round 1's probe output over the narrower base
//! 298ffb2, `sdet-u88c4-surface-enumeration-r3` the re-run over the full unit diff after
//! rounds 2 and 3 landed):
//!   1. new public API - `Worktree::commits_since_base`, `Worktree::cherry_pick_onto_run_branch`,
//!      `Worktree::files_touched_by_commit` (round 2), `CherryPickOutcome` (all
//!      `src/worktree.rs`);
//!   2. changed event serialized form - `UnitIntegrated` gained a `shas: []` JSON field
//!      (`src/conductor.rs:4450`). Round 1 left it write-only; round 2's fix for
//!      `arch-u88c4-multicommit-landing-breaks-compensation-single-commit-contract` made
//!      `commits_to_compensate` (`src/conductor.rs:2924`) READ it back, so a later unit's
//!      review naming this producer as a compensation target reverts EVERY landed commit,
//!      not just the newest - gap 6 below closes this fold arm at the periphery;
//!   3. a new cross-module seam - `RunCtx::integrate_plan_commits` (private, `conductor.rs`)
//!      calling the `Worktree` methods above, wired into `run_single_stage`'s producer arm.
//!
//! WHAT THE INSIDE-OUT TESTS ARE STRUCTURALLY BLIND TO. `src/worktree.rs`'s own test module
//! proves the new methods' git mechanics directly (one commit landed, a conflict aborts the
//! whole sequence); `src/conductor.rs`'s own test module proves the full producer-stage flow
//! through the crate-PRIVATE `Stub` driver, for a single-file commit under `specs/`, a single
//! out-of-scope file, and a conflict. Both are real and valuable, but both are the
//! IMPLEMENTER's own inside-out authorship (spec 32: unit-level TDD the sdet-author layer is
//! ADDED to, never a substitute for it) and neither is reachable from outside the crate - a
//! `Stub` with `commits_by_agent`/`read_file_by_agent` fields is a private `struct` inside
//! `conductor.rs`, so an external consumer of this library (anything linking `rigger` as a
//! dependency, exactly what every file in this `tests/` directory is) cannot reuse it or see
//! it exists. This file drives the SAME public entry points (`conductor::run`, the public
//! `AgentDriver` trait, the public `Worktree` API) with an INDEPENDENTLY-authored driver, from
//! outside the crate - the same discipline `tests/replan_episode_identity.rs` and
//! `tests/store_append_order_periphery.rs` already established for this codebase - and closes
//! three gaps neither inside-out layer covers at all:
//!   - a MULTI-commit amendment (two separate `specs/` commits in one producer attempt) landing
//!     in order, verified against the run branch's own real git history rather than the folded
//!     projection alone (`multiple_specs_commits_land_in_order...`);
//!   - the new `shas` field's actual JSON shape, oldest-first ordering, and survival through a
//!     real close-and-reopen of a file-backed store, PLUS a hand-built legacy `UnitIntegrated`
//!     (no `shas` field at all - exactly every pre-spec-88 producer integration and every
//!     ordinary unit's integration today) coexisting safely in the same store
//!     (`unit_integrated_shas_field_round_trips...`);
//!   - a single commit that touches BOTH an in-scope `specs/` path and an out-of-scope path
//!     together (`changed_since_base` unions every touched path, so one dirty file anywhere
//!     poisons the whole commit) - the implementer's own out-of-scope test only ever committed
//!     one file at a time (`plan_stage_commit_mixing_an_in_scope_and_out_of_scope_path...`);
//!   - the conflict path's CAUSE tag specifically (`integrate-conflict`, distinct from the
//!     out-of-scope path's `reject`) - the implementer's own conflict test asserts only the
//!     terminal status and the untouched file content, never the cause the run's own "cause
//!     wire" (spec 69 criterion 3) is supposed to carry (`plan_stage_conflicting_amendment...`);
//!   - the bare `Worktree::cherry_pick_onto_run_branch(&[])` no-op contract as a PUBLIC API
//!     guarantee, called by an external consumer with no conductor involved at all - untested
//!     anywhere else in the tree (`cherry_pick_onto_run_branch_public_api_no_op_on_empty_shas`);
//!   - gap 6 (round 2/3, `sdet-u88c4-surface-enumeration-r3`): a producer that landed MULTIPLE
//!     commits, later named as a compensation target by a downstream unit's review, has EVERY
//!     one of its landed commits reverted from the run branch - not just the newest. The
//!     implementer's own `commits_to_compensate_reverts_every_sha_a_multi_commit_landing_
//!     recorded` (conductor.rs) calls the crate-private `commits_to_compensate` directly
//!     against hand-built `shas: ["c1","c2","c3"]` fixtures - fake object ids, no real git,
//!     and unreachable from outside the crate - so it proves the FOLD reads the right JSON
//!     field, never that the real git revert this criterion exists to protect actually removes
//!     every landed file from a real run branch through the public `run()` entry
//!     (`plan_stage_compensation_reverts_every_landed_commit_not_just_the_newest`).

use rigger::conductor::{
    run, AgentDriver, AgentResult, Deps, Error, SpawnOpts, META_COMPENSATED,
    META_COMPENSATE_TARGET, REVIEW_ONLY_NO_ARTIFACT, STREAM,
};
use rigger::config::{AgentDef, Config, Gate, ReviewPanel, Stage};
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, Event, EventStore, ExpectedRevision, Filter};
use rigger::gate::ExecRunner;
use rigger::ledger;
use rigger::worktree::{CherryPickOutcome, Worktree};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// A bare git repo with one initial empty commit - the run branch every stage's worktree
/// ultimately branches from or merges into.
fn init_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().to_str().unwrap();
    // A local closure over four direct calls, not a loop over an array literal
    // (kept distinct in SHAPE from the crate's own internal `init_repo` test
    // helper it otherwise mirrors, so the two never collide as a mechanical
    // near-duplicate pair and silently renumber the duplication catalog's
    // unrelated ids - sdet-u88c2-audit-cascade-root-cause's fix pattern).
    let step = |args: &[&str]| {
        assert!(std::process::Command::new("git")
            .arg("-C")
            .arg(p)
            .args(args)
            .status()
            .unwrap()
            .success());
    };
    step(&["init", "-q"]);
    step(&["config", "user.email", "t@example.com"]);
    step(&["config", "user.name", "t"]);
    step(&["commit", "--allow-empty", "-q", "-m", "init"]);
    dir
}

/// Run `git <args>` in `dir`, returning trimmed stdout; panics with stderr on failure. For
/// read-only plumbing only (`rev-parse`, `diff-tree`, ...) - the driver below never uses this
/// for its own commits, since a RETRY must tolerate "nothing to commit" (see its doc comment).
fn run_git(dir: &str, args: &[&str]) -> String {
    let out = match std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
    {
        Ok(out) => out,
        Err(e) => panic!("git must be installed and runnable: {e}"),
    };
    if !out.status.success() {
        panic!(
            "git {args:?} in {dir} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn agent(id: &str) -> AgentDef {
    AgentDef {
        id: id.to_string(),
        ..Default::default()
    }
}

/// Drives a `produces` (planner) stage that commits its own paths directly with its OWN git
/// access, one `git commit` per group in `commits`, in order - the shape spec 88 criterion 4
/// exists for (a planner's approved spec amendment; a `produces` stage writes no code the
/// conductor ever sweeps and commits). An optional downstream "reader" agent then records what
/// it finds at each of `reads`, in ITS OWN worktree (branched off the run branch only after
/// the producer reaches `Integrated`), so a test can prove a LATER stage already sees a landed
/// amendment.
///
/// The commit step ignores its own subprocess result (`let _ = ... .output()`): a scope
/// violation feeds the SAME bounded remediation loop an ordinary reject does (§3.2), so the
/// planner can be re-spawned into the SAME adopted branch with the SAME already-committed,
/// still-uncorrected content - a real re-commit attempt there has nothing new to stage and
/// would fail loudly for a reason that has nothing to do with what the test is proving. An
/// asserting `run_git` is reserved for read-only verification below, never for this idempotent
/// write path.
#[derive(Default)]
struct PlanAmendDriver {
    planner: String,
    reader: String,
    /// Each inner `Vec` is ONE commit; the `(path, content)` pairs inside it are all written
    /// and staged together before that one `git commit`.
    commits: Vec<Vec<(String, String)>>,
    reads: Vec<String>,
    found: Mutex<HashMap<String, String>>,
    /// The adjudicator agent id for a downstream review that names a compensation target
    /// (gap 6 - PLAN AMENDMENTS LAND's multi-commit compensation fold arm). Empty (the
    /// default) means no unit ever plays adjudicator - every existing gap-1-through-5 test
    /// leaves this unset and is byte-for-byte unaffected.
    judge: String,
    /// The unit id the judge's verdict names via `compensate` (spec 12, unit 4's
    /// pre-existing vocabulary) - always approving its OWN unit's work while naming this
    /// target as the real defect source, exactly like `conductor.rs`'s own `CompDriver`
    /// fixture for the single-commit case.
    compensate_target: String,
    /// Gates the planner's commit application to its FIRST spawn only (gap 6): once this
    /// unit's amendment has landed and then been reverted by a compensation, this fixture's
    /// job is done - a genuine re-implementation is a DIFFERENT concern (already proven by
    /// `conductor.rs`'s own `a_contradiction_compensates_reverts_and_re_enters_the_
    /// integrated_unit`), so the re-parked second attempt commits nothing and the producer
    /// converges via the ordinary no-artifact path, leaving the revert as the run branch's
    /// final, unambiguous word on those two files. Every existing test still calls the
    /// planner exactly once, so this changes nothing for gaps 1-5.
    committed: AtomicBool,
}

impl PlanAmendDriver {
    fn new(planner: &str) -> Self {
        PlanAmendDriver {
            planner: planner.to_string(),
            ..Default::default()
        }
    }

    fn commit(mut self, files: &[(&str, &str)]) -> Self {
        self.commits.push(
            files
                .iter()
                .map(|(p, c)| (p.to_string(), c.to_string()))
                .collect(),
        );
        self
    }

    fn reading(mut self, reader: &str, path: &str) -> Self {
        self.reader = reader.to_string();
        self.reads.push(path.to_string());
        self
    }

    fn judging(mut self, judge: &str, compensate_target: &str) -> Self {
        self.judge = judge.to_string();
        self.compensate_target = compensate_target.to_string();
        self
    }
}

impl AgentDriver for PlanAmendDriver {
    fn spawn(
        &self,
        a: &AgentDef,
        _prompt: &str,
        opts: &SpawnOpts,
        _emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        if !self.judge.is_empty() && a.id == self.judge {
            return Ok(AgentResult {
                output: format!(
                    "{{\"verdict\":\"approve\",\"compensate\":\"{}\"}}",
                    self.compensate_target
                ),
                resolved_model: String::new(),
            });
        }
        if a.id == self.planner
            && !opts.dir.is_empty()
            && !self.committed.swap(true, Ordering::SeqCst)
        {
            for group in &self.commits {
                for (path, content) in group {
                    let full = std::path::Path::new(&opts.dir).join(path);
                    if let Some(parent) = full.parent() {
                        std::fs::create_dir_all(parent).unwrap();
                    }
                    std::fs::write(&full, content).unwrap();
                }
                let _ = std::process::Command::new("git")
                    .arg("-C")
                    .arg(&opts.dir)
                    .args(["add", "-A"])
                    .output();
                let msg = format!(
                    "amend {}",
                    group
                        .iter()
                        .map(|(p, _)| p.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                let _ = std::process::Command::new("git")
                    .arg("-C")
                    .arg(&opts.dir)
                    .args(["commit", "-q", "-m", &msg])
                    .output();
            }
        }
        if a.id == self.reader && !opts.dir.is_empty() {
            for path in &self.reads {
                if let Ok(content) =
                    std::fs::read_to_string(std::path::Path::new(&opts.dir).join(path))
                {
                    self.found.lock().unwrap().insert(path.clone(), content);
                }
            }
        }
        Ok(AgentResult {
            output: format!("{} ok", a.id),
            resolved_model: String::new(),
        })
    }
}

fn plan_stage() -> Stage {
    Stage {
        name: "plan".into(),
        agent: "planner".into(),
        produces: "dag".into(),
        ..Default::default()
    }
}

/// Stands in for the next real stage's worktree (e.g. a plan-critique gate's throwaway
/// review worktree): a standalone, un-gated review stage that needs "plan", so its worktree
/// is created only AFTER "plan" reaches `Integrated` - branched off whatever the run branch
/// holds at that moment.
fn downstream_reader_stage(reader: &str) -> Stage {
    Stage {
        name: "critique".into(),
        agents: vec![reader.to_string()],
        needs: vec!["plan".into()],
        ..Default::default()
    }
}

/// Criterion 4, gap 1 (new public API `Worktree::commits_since_base` / `cherry_pick_onto_
/// run_branch`, exercised through the real cross-module seam): TWO separate `specs/` commits
/// in one producer attempt land on the run branch IN ORDER, and the next stage's worktree -
/// branched off the run branch only after "plan" integrates - already sees BOTH, not just the
/// first. Verified against the run branch's own real git history (`diff-tree`), independent of
/// the folded `RunState` projection.
#[test]
fn multiple_specs_commits_land_in_order_and_the_next_worktree_sees_both() {
    let repo = init_repo();
    let repo_path = repo.path().to_str().unwrap().to_string();

    let mut cfg = Config::default();
    cfg.agents.insert("planner".into(), agent("planner"));
    cfg.agents.insert("checker".into(), agent("checker"));
    cfg.workflow.stages.insert("plan".into(), plan_stage());
    cfg.workflow
        .stages
        .insert("critique".into(), downstream_reader_stage("checker"));

    let store = Store::open(":memory:").unwrap();
    let driver = PlanAmendDriver::new("planner")
        .commit(&[("specs/90-first.md", "first amendment\n")])
        .commit(&[("specs/91-second.md", "second amendment\n")])
        .reading("checker", "specs/90-first.md")
        .reading("checker", "specs/91-second.md");
    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &ExecRunner,
        repo: repo_path.clone(),
        grounder: None,
        graph: None,
        criteria: Vec::new(),
    };
    let rs = run(&cfg, &deps).unwrap();

    assert_eq!(
        rs.units["plan"].status,
        ledger::Status::Integrated,
        "a producer with two landed amendments must still reach Integrated"
    );
    let commit = rs.units["plan"].commit.clone();
    assert_ne!(
        commit, REVIEW_ONLY_NO_ARTIFACT,
        "a landed commit replaces the marker"
    );
    assert_ne!(commit, "");

    // Both amendments are on the run branch itself.
    assert_eq!(
        std::fs::read_to_string(repo.path().join("specs").join("90-first.md")).unwrap(),
        "first amendment\n"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("specs").join("91-second.md")).unwrap(),
        "second amendment\n"
    );

    // Independent, real-git proof of ORDER: the projected `commit` is the NEWEST landed
    // sha, whose own diff is the SECOND amendment; its parent's diff is the FIRST.
    let newest_files = run_git(
        &repo_path,
        &["diff-tree", "--no-commit-id", "--name-only", "-r", &commit],
    );
    assert!(
        newest_files.contains("specs/91-second.md"),
        "the newest landed commit must be the second amendment; files: {newest_files:?}"
    );
    let parent = run_git(&repo_path, &["rev-parse", &format!("{commit}^")]);
    let parent_files = run_git(
        &repo_path,
        &["diff-tree", "--no-commit-id", "--name-only", "-r", &parent],
    );
    assert!(
        parent_files.contains("specs/90-first.md"),
        "the parent of the newest landed commit must be the first amendment; files: {parent_files:?}"
    );

    // The load-bearing ordering claim: the NEXT stage's worktree, branched off the run
    // branch after "plan" integrated, already sees BOTH landed amendments.
    let found = driver.found.lock().unwrap();
    assert_eq!(
        found.get("specs/90-first.md").cloned(),
        Some("first amendment\n".to_string())
    );
    assert_eq!(
        found.get("specs/91-second.md").cloned(),
        Some("second amendment\n".to_string())
    );
}

/// Criterion 4, gap 2 (the changed event serialized form): the `UnitIntegrated.shas` field is
/// a genuinely new JSON shape nothing else in the tree reads back (it is write-only audit
/// trail today - `sdet-u88c4-surface-enumeration` names the grep proving this). Proves it is
/// a well-formed array of real git object ids, oldest-first (matching the run branch's own
/// `rev-list`), with `commit == shas.last()`, and that it SURVIVES a real close-and-reopen of
/// a file-backed store (not just an in-process read of the same handle). Also proves BACK-
/// COMPAT in the same store: a hand-built LEGACY `UnitIntegrated` with no `shas` field at all,
/// exactly what every producer integration recorded before this spec (and what every ordinary,
/// non-producer unit's `UnitIntegrated` still records today), reads back fine alongside the
/// new-format event, so an old reader (or an old row) is never broken by this addition.
#[test]
fn unit_integrated_shas_field_round_trips_through_a_reopened_store_and_tolerates_legacy_events() {
    let repo = init_repo();
    let repo_path = repo.path().to_str().unwrap().to_string();
    let init_sha = run_git(&repo_path, &["rev-parse", "HEAD"]);

    let mut cfg = Config::default();
    cfg.agents.insert("planner".into(), agent("planner"));
    cfg.workflow.stages.insert("plan".into(), plan_stage());

    let store_dir = tempfile::tempdir().unwrap();
    let store_path = store_dir
        .path()
        .join("events.db")
        .to_str()
        .unwrap()
        .to_string();

    {
        let store = Store::open(&store_path).unwrap();
        let driver = PlanAmendDriver::new("planner")
            .commit(&[("specs/92-a.md", "a\n")])
            .commit(&[("specs/92-b.md", "b\n")]);
        let deps = Deps {
            store: &store,
            driver: &driver,
            gates: &ExecRunner,
            repo: repo_path.clone(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        run(&cfg, &deps).unwrap();

        // A LEGACY UnitIntegrated, hand-built exactly as pre-spec-88 code (or any
        // ordinary unit today) would have recorded it - no `shas` field at all -
        // appended into the SAME real store, on the run's own stream.
        store
            .append(
                STREAM,
                ExpectedRevision::Any,
                &[Event::new(
                    ledger::TYPE_UNIT_INTEGRATED,
                    serde_json::to_vec(&json!({"id": "legacy-unit", "commit": "deadbeefcafe"}))
                        .unwrap(),
                )],
            )
            .unwrap();
    } // the store handle is dropped here - a real close, not just going out of scope of a borrow.

    // Reopen a FRESH handle on the SAME file: the new-format event must survive an actual
    // close/reopen of the persisted store.
    let reopened = Store::open(&store_path).unwrap();
    let events = reopened
        .read_all(0, Direction::Forward, &Filter::default())
        .unwrap();
    let integrated: Vec<Value> = events
        .iter()
        .filter(|e| e.type_ == ledger::TYPE_UNIT_INTEGRATED)
        .map(|e| serde_json::from_slice::<Value>(&e.data).unwrap())
        .collect();

    let plan_event = integrated
        .iter()
        .find(|v| v.get("id").and_then(Value::as_str) == Some("plan"))
        .expect("the plan unit's UnitIntegrated must round-trip through the reopened store");
    let shas: Vec<String> = plan_event["shas"]
        .as_array()
        .expect("shas must be a JSON array")
        .iter()
        .map(|v| {
            v.as_str()
                .expect("each sha must be a JSON string")
                .to_string()
        })
        .collect();
    assert_eq!(shas.len(), 2, "two commits in, two shas out; got {shas:?}");
    assert!(
        shas.iter()
            .all(|s| s.len() >= 7 && s.chars().all(|c| c.is_ascii_hexdigit())),
        "every recorded sha must look like a real git object id; got {shas:?}"
    );
    assert_eq!(
        plan_event["commit"].as_str().unwrap(),
        shas.last().unwrap(),
        "commit must be the newest (last) landed sha"
    );

    // The recorded order matches the run branch's OWN real git history, oldest-first -
    // the exact order `commits_since_base` / `cherry_pick_onto_run_branch` document.
    let real_order: Vec<String> = run_git(
        &repo_path,
        &["rev-list", "--reverse", &format!("{init_sha}..HEAD")],
    )
    .lines()
    .map(str::to_string)
    .collect();
    assert_eq!(
        shas, real_order,
        "the recorded shas must match the run branch's real, oldest-first commit order"
    );

    // The legacy, shas-less event round-tripped too, untouched.
    let legacy = integrated
        .iter()
        .find(|v| v.get("id").and_then(Value::as_str) == Some("legacy-unit"))
        .expect("the legacy (no-shas) UnitIntegrated must round-trip through the reopened store");
    assert!(
        legacy.get("shas").is_none(),
        "a legacy UnitIntegrated must carry no shas field at all; got {legacy:?}"
    );
    assert_eq!(legacy["commit"].as_str().unwrap(), "deadbeefcafe");
}

/// Criterion 4, gap 3 (the cross-module seam's OutOfScope branch, a shape the implementer's
/// own single-file test never exercises): ONE commit that touches BOTH an in-scope `specs/`
/// path AND an out-of-scope path, together. `changed_since_base` unions every path the
/// worktree touched since the run branch's HEAD, so this single mixed commit must be rejected
/// as a whole (naming the offending path) - a legitimate `specs/` edit riding alongside a
/// scope violation must never let the violation through, and must never reach the run branch.
#[test]
fn plan_stage_commit_mixing_an_in_scope_and_out_of_scope_path_is_rejected_and_names_the_path() {
    let repo = init_repo();
    let repo_path = repo.path().to_str().unwrap().to_string();

    let mut cfg = Config::default();
    cfg.agents.insert("planner".into(), agent("planner"));
    cfg.workflow.stages.insert("plan".into(), plan_stage());

    let store = Store::open(":memory:").unwrap();
    let driver = PlanAmendDriver::new("planner").commit(&[
        ("specs/93-mixed.md", "a real amendment\n"),
        ("docs/rogue.md", "scope creep riding along\n"),
    ]);
    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &ExecRunner,
        repo: repo_path.clone(),
        grounder: None,
        graph: None,
        criteria: Vec::new(),
    };
    let rs = run(&cfg, &deps).unwrap();

    assert_eq!(
        rs.units["plan"].status,
        ledger::Status::Escalated,
        "a commit mixing an out-of-scope path with an in-scope one must never integrate"
    );
    let events = store
        .read_all(0, Direction::Forward, &Filter::default())
        .unwrap();
    let failed = events
        .iter()
        .find(|e| e.type_ == ledger::TYPE_UNIT_FAILED)
        .expect("a mixed-scope commit must record a UnitFailed");
    let body = String::from_utf8_lossy(&failed.data);
    assert!(
        body.contains("\"cause\":\"reject\""),
        "a scope violation is stamped the same cause tag an ordinary review reject is"
    );

    // NEITHER file reached the run branch - a legitimate specs/ edit does not get a free
    // ride onto the branch just because it shares a commit with the violation.
    assert!(!repo.path().join("specs").join("93-mixed.md").exists());
    assert!(!repo.path().join("docs").join("rogue.md").exists());
}

/// Criterion 4, gap 4 (the cross-module seam's Conflict branch, plus the "cause wire" spec 69
/// criterion 3 names): a plan amendment that conflicts with a concurrent operator commit under
/// `specs/` escalates to a human - proven here through the SAME real cherry-pick-conflict
/// mechanism the implementer's own test uses (a durable `rigger/u/plan` branch pre-seeded by a
/// PRIOR window, then a diverging concurrent edit on the run branch), but additionally
/// asserting the CAUSE the failure carries: `integrate-conflict`, distinct from the mixed-scope
/// test's `reject` above - a boundary neither of the implementer's own tests names.
#[test]
fn plan_stage_conflicting_amendment_escalates_with_the_integrate_conflict_cause() {
    let repo = init_repo();
    let repo_path = repo.path().to_str().unwrap().to_string();

    // A PRIOR window's planner already committed a specs/ amendment onto the deterministic
    // `rigger/u/plan` branch - via a throwaway worktree, never touching the run branch.
    let seed_dir = tempfile::tempdir().unwrap();
    let seed = Worktree::create(
        &repo_path,
        seed_dir.path().to_str().unwrap(),
        "rigger/u/plan",
        "",
    )
    .unwrap();
    std::fs::create_dir_all(seed_dir.path().join("specs")).unwrap();
    std::fs::write(
        seed_dir.path().join("specs").join("94-contested.md"),
        "planner amend\n",
    )
    .unwrap();
    assert!(std::process::Command::new("git")
        .arg("-C")
        .arg(seed_dir.path())
        .args(["add", "-A"])
        .status()
        .unwrap()
        .success());
    assert!(std::process::Command::new("git")
        .arg("-C")
        .arg(seed_dir.path())
        .args(["commit", "-q", "-m", "planner amend"])
        .status()
        .unwrap()
        .success());
    seed.remove().unwrap(); // only the transient dir goes; the branch persists.

    // Meanwhile the run branch independently gains a CONFLICTING concurrent operator edit
    // to the same spec path.
    std::fs::create_dir_all(repo.path().join("specs")).unwrap();
    std::fs::write(
        repo.path().join("specs").join("94-contested.md"),
        "operator edit\n",
    )
    .unwrap();
    assert!(std::process::Command::new("git")
        .arg("-C")
        .arg(repo.path())
        .args(["add", "-A"])
        .status()
        .unwrap()
        .success());
    assert!(std::process::Command::new("git")
        .arg("-C")
        .arg(repo.path())
        .args(["commit", "-q", "-m", "operator edit"])
        .status()
        .unwrap()
        .success());

    let mut cfg = Config::default();
    cfg.agents.insert("planner".into(), agent("planner"));
    cfg.workflow.stages.insert("plan".into(), plan_stage());

    let store = Store::open(":memory:").unwrap();
    // The planner's fresh spawn commits nothing new this run - the ALREADY-adopted branch
    // (from the prior window) is what conflicts.
    let driver = PlanAmendDriver::new("planner");
    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &ExecRunner,
        repo: repo_path.clone(),
        grounder: None,
        graph: None,
        criteria: Vec::new(),
    };
    let rs = run(&cfg, &deps).unwrap();

    assert_eq!(
        rs.units["plan"].status,
        ledger::Status::Escalated,
        "a conflicting plan amendment must escalate to a human, never integrate"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("specs").join("94-contested.md")).unwrap(),
        "operator edit\n",
        "the conflicting amendment must never overwrite the concurrent operator edit"
    );

    let events = store
        .read_all(0, Direction::Forward, &Filter::default())
        .unwrap();
    let failed = events
        .iter()
        .find(|e| e.type_ == ledger::TYPE_UNIT_FAILED)
        .expect("a conflicting amendment must record a UnitFailed");
    let body = String::from_utf8_lossy(&failed.data);
    assert!(
        body.contains("\"cause\":\"integrate-conflict\""),
        "a conflicting plan amendment must carry the integrate-conflict cause, distinct \
         from an out-of-scope commit's reject cause; got: {body}"
    );
}

/// Criterion 4, gap 5 (new public API, no conductor involved at all): `Worktree::
/// cherry_pick_onto_run_branch`'s documented no-op contract on an empty `shas` slice -
/// `Picked(vec![])`, nothing touched - called directly by an external consumer of the public
/// `Worktree` API. Untested anywhere else: every existing caller (the conductor's own
/// `integrate_plan_commits`, and every worktree.rs unit test) always passes a non-empty slice.
#[test]
fn cherry_pick_onto_run_branch_public_api_no_op_on_empty_shas() {
    let repo = init_repo();
    let repo_path = repo.path().to_str().unwrap().to_string();
    let head_before = run_git(&repo_path, &["rev-parse", "HEAD"]);

    let wt_dir = tempfile::tempdir().unwrap();
    let wt = Worktree::create(
        &repo_path,
        wt_dir.path().to_str().unwrap(),
        "rigger/u/ext-plan",
        "",
    )
    .unwrap();

    assert_eq!(
        wt.commits_since_base().unwrap(),
        Vec::<String>::new(),
        "a fresh worktree has nothing beyond the run branch's HEAD"
    );

    match wt.cherry_pick_onto_run_branch(&[]).unwrap() {
        CherryPickOutcome::Picked(landed) => {
            assert!(
                landed.is_empty(),
                "an empty input must land nothing; got {landed:?}"
            )
        }
        CherryPickOutcome::Conflict(detail) => {
            panic!("an empty input must never conflict; got: {detail}")
        }
    }
    assert_eq!(
        run_git(&repo_path, &["rev-parse", "HEAD"]),
        head_before,
        "a no-op cherry-pick must never move the run branch's HEAD"
    );
    wt.remove().unwrap();
}

/// Criterion 4, gap 6 (round 2's fix for `arch-u88c4-multicommit-landing-breaks-compensation-
/// single-commit-contract`, `sdet-u88c4-surface-enumeration-r3`): a plan-stage producer that
/// landed TWO commits in one attempt, later named as a compensation target by a downstream
/// unit's review, has BOTH landed commits reverted from the run branch - not just the newest
/// (`commit`, the single-sha projection every OTHER unit's compensation contract reads).
///
/// WHY THIS, DISTINCT FROM THE IMPLEMENTER'S OWN TESTS. `conductor.rs`'s own
/// `commits_to_compensate_reverts_every_sha_a_multi_commit_landing_recorded` calls the
/// crate-private `RunCtx::commits_to_compensate` DIRECTLY against a hand-built
/// `{"shas": ["c1","c2","c3"]}` fixture - fake object ids that were never real git commits,
/// unreachable from outside the crate, and never fed through an actual `git revert`. It
/// proves the FOLD reads the right JSON array; it cannot prove the real git revert this
/// criterion exists to protect actually removes every landed file from a real run branch.
/// This test drives the SAME real end-to-end mechanism `conductor.rs`'s own
/// `a_contradiction_compensates_reverts_and_re_enters_the_integrated_unit` uses for the
/// pre-existing single-commit case (a downstream "checker" unit whose adjudicator approves
/// its own work but names the producer via `compensate`), over a MULTI-commit producer
/// landing, which that implementer fixture never constructs.
#[test]
fn plan_stage_compensation_reverts_every_landed_commit_not_just_the_newest() {
    let repo = init_repo();
    let repo_path = repo.path().to_str().unwrap().to_string();

    let mut cfg = Config::default();
    cfg.agents.insert("planner".into(), agent("planner"));
    cfg.agents
        .insert("checker_impl".into(), agent("checker_impl"));
    cfg.agents.insert("judge".into(), agent("judge"));
    cfg.workflow.gates.insert(
        "g".into(),
        Gate {
            run: "true".into(),
            kind: "core".into(),
            inputs: Vec::new(),
        },
    );
    cfg.workflow.stages.insert("plan".into(), plan_stage());
    cfg.workflow.stages.insert(
        "checker".into(),
        Stage {
            name: "checker".into(),
            agent: "checker_impl".into(),
            gates: vec!["g".into()],
            on_pass: "merge".into(),
            needs: vec!["plan".into()],
            review: ReviewPanel {
                adjudicator: "judge".into(),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    let store = Store::open(":memory:").unwrap();
    // "checker"'s own work always approves; its verdict additionally names "plan" as the
    // real defect source (spec 12, unit 4's pre-existing `compensate` vocabulary - untouched
    // by this criterion). The re-parked "plan" attempt commits nothing new (`committed`
    // already flipped true), so the revert below is the run branch's final word on both
    // originally-landed files.
    let driver = PlanAmendDriver::new("planner")
        .commit(&[("specs/95-first.md", "first amendment\n")])
        .commit(&[("specs/96-second.md", "second amendment\n")])
        .judging("judge", "plan");
    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &ExecRunner,
        repo: repo_path.clone(),
        grounder: None,
        graph: None,
        criteria: Vec::new(),
    };
    let rs = run(&cfg, &deps).unwrap();

    assert_eq!(
        rs.units["checker"].status,
        ledger::Status::Integrated,
        "checker's own work is fine and must integrate"
    );
    assert_eq!(
        rs.units["plan"].status,
        ledger::Status::Integrated,
        "plan re-converges via the ordinary no-artifact path after its compensation rollback \
         (this fixture's re-parked attempt commits nothing new), mirroring conductor.rs's own \
         single-commit compensation fixture's re-implement-and-reconverge shape"
    );

    let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    let unit_of = |e: &Event| -> Option<String> {
        serde_json::from_slice::<Value>(&e.data)
            .ok()
            .and_then(|v| v.get("id").and_then(Value::as_str).map(str::to_string))
    };

    // The FIRST (pre-compensation) UnitIntegrated for "plan" carries the ground truth for
    // what actually landed - both commits, oldest-first.
    let first_integrated = events
        .iter()
        .find(|e| e.type_ == ledger::TYPE_UNIT_INTEGRATED && unit_of(e).as_deref() == Some("plan"))
        .expect("plan's first integration must be recorded");
    let first_v: Value = serde_json::from_slice(&first_integrated.data).unwrap();
    let landed_shas: Vec<String> = first_v["shas"]
        .as_array()
        .expect("plan's first UnitIntegrated must carry the shas field")
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        landed_shas.len(),
        2,
        "this test's premise is a TWO-commit producer landing; got {landed_shas:?}"
    );

    // (1) COMPENSATION RECORDED, naming BOTH landed shas newest-first - not just
    // `shas.last()`, which is all the pre-round-2 single-commit contract could see.
    let compensated = events
        .iter()
        .find(|e| {
            e.type_ == ledger::TYPE_UNIT_FAILED
                && unit_of(e).as_deref() == Some("plan")
                && e.meta.contains_key(META_COMPENSATED)
        })
        .expect("a UnitFailed carrying META_COMPENSATED must record plan's compensation");
    let reverted: Vec<String> = compensated
        .meta
        .get(META_COMPENSATED)
        .unwrap()
        .split(',')
        .map(str::to_string)
        .collect();
    let mut expected_reverted = landed_shas.clone();
    expected_reverted.reverse();
    assert_eq!(
        reverted, expected_reverted,
        "compensation must revert EVERY commit plan actually landed, newest-first - not just \
         the newest one a single-sha contract would see"
    );

    // (2) REVERTED ON THE RUN BRANCH via an evented (not history-rewriting) rollback: one
    // "compensate plan (revert <sha>)" commit per originally-landed sha - real git proof,
    // independent of the folded projection above.
    let log = std::process::Command::new("git")
        .arg("-C")
        .arg(&repo_path)
        .args(["log", "--pretty=%s"])
        .output()
        .unwrap();
    let log = String::from_utf8_lossy(&log.stdout);
    for sha in &landed_shas {
        assert!(
            log.lines()
                .any(|l| l.contains("compensate plan") && l.contains(sha.as_str())),
            "the run branch must carry an evented revert of landed commit {sha}; log:\n{log}"
        );
    }

    // (3) Both amended files are GONE from the run branch's working tree - the re-parked
    // attempt (this fixture) commits nothing new, so the revert is the final, unambiguous
    // word on both, not just the one a single-sha revert would have reached.
    assert!(!repo.path().join("specs").join("95-first.md").exists());
    assert!(!repo.path().join("specs").join("96-second.md").exists());

    // (4) The durable trigger (spec 12, unit 4's pre-existing vocabulary, reused unmodified
    // by this criterion) named "plan" exactly once.
    let queued: Vec<&Event> = events
        .iter()
        .filter(|e| e.meta.get(META_COMPENSATE_TARGET).map(String::as_str) == Some("plan"))
        .collect();
    assert_eq!(
        queued.len(),
        1,
        "exactly one durable compensation-queued mark must name plan"
    );
}
