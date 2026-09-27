//! Periphery (real-on-disk-config, real-git, cross-module) tests for spec 103, criterion 3
//! (RE-GATE WHAT LANDED): `RunCtx::integrate_and_emit`'s entry-level fast path used to treat
//! "row 4 (landing) is fully closed and nothing is owed" as proof there was nothing left to
//! GATE - but `pending_landing_for` stops seeing a landing the instant `record_landed` closes
//! it, so a crash between that append and the post-merge re-gate that must follow it left a
//! real merge's tree ungated forever: the resumed call took the true no-op short circuit and
//! reached `UnitIntegrated` (`commit: ""`) without ever re-running the merged tree's gates.
//!
//! WHAT THE IMPLEMENTER'S OWN TEST ALREADY COVERS (not re-proven here).
//! `src/conductor.rs`'s own `mod tests` proves the NEW fold and lookup
//! (`landed_from_log`/`RunCtx::landed`/`RunCtx::landed_sha_for`) drive the fixed entry-level
//! fast path for ONE shape: a single `run()` call against a store HAND-SEEDED with the exact
//! four events a real prior window would have left (`UnitStarted`, `verified`, `reviewed`,
//! `integrate-landed` carrying the new `pre_merge` evidence field) - proving that IF the log
//! holds exactly that shape, the resumed call resolves `commit`/`pre_merge` from it, runs the
//! post-merge gate for real, and reports `UnitIntegrated` with the real landed sha.
//!
//! WHAT THIS FILE OWNS - two gaps the hand-seeded, inside-out coverage above is structurally
//! blind to (`sdet-u103c3-boundary-accounting`):
//!
//! GAP 1 (event type / serialized form - round-trip), `a_crash_right_after_landing_before_the_
//! postmerge_regate_still_gates_for_real_on_resume`. A hand-typed event proves the READ side
//! parses a shape the author BELIEVES the write side produces; it cannot prove the two agree.
//! This drives a REAL two-call crash-then-resume (mirroring
//! `tests/integrate_conflict_merge_periphery.rs`'s own established GAP 6/9/10 technique,
//! `FailAfterContaining` below): call 1 lands a genuine single-unit merge for real (implementer
//! -> gates -> review -> `wt.land()`, no shortcuts) and its `record_landed` append durably
//! succeeds - proving the PRODUCTION write path actually puts `pre_merge` on the wire, not just
//! the test author's own hand-built JSON - then a simulated crash refuses the very next append
//! (the post-merge gate's own verdict), before it can complete. Call 2 resumes fresh against the
//! SAME store+repo and must re-derive the real landed sha and pre_merge purely from that durable
//! row, run the post-merge gate for real, and reach `UnitIntegrated` with the actual merged sha
//! - never re-spawning the unit's implementer/lens/judge lifecycle at all.
//!
//! GAP 2 (event type / serialized form - back-compat, the OTHER half of the same probe hit),
//! `a_pre_fix_landed_row_missing_pre_merge_keeps_the_old_true_no_op_resume_behavior`. Every
//! `integrate-landed` row a binary built BEFORE this fix ever recorded carries no `pre_merge`
//! field at all (the field is new); `landed_from_log`'s own doc comment asserts such a row "is
//! skipped rather than guessed" so "a unit whose ONLY landed row predates the fix keeps taking
//! the true no-op short circuit it always did, no regression" - a claim the diff states but
//! never proves anywhere. This hand-seeds exactly that legacy shape (the one shape GAP 1 never
//! produces, since the current binary always writes the new field) and proves `run()` neither
//! panics nor misresolves a commit from it: the resumed call takes the identical pre-fix path,
//! recording no post-merge-gate verdict and reporting `UnitIntegrated` with the same empty
//! `commit` sentinel a pre-fix binary always reported for this shape.

mod common;

use common::fixtures::agent;
use common::fixtures::count_status_marker;
use common::fixtures::gate_def;
use common::fixtures::has_status_marker;
use common::fixtures::mk_stage;
use common::fixtures::review_or_adjudicate;
use common::git::git_commit_all;
use common::git::git_stdout;
use common::git::temp_git_project_with_commit;
use rigger::conductor::{run, AgentDriver, AgentResult, Deps, Error, SpawnOpts, STREAM};
use rigger::config::{AgentDef, Config};
use rigger::contextgraph;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{
    Appended, Direction, Error as EsError, Event, EventStore, ExpectedRevision,
};
use rigger::ledger;
use serde_json::{json, Value};
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

/// `rigger::conductor::unit_branch`'s exact convention (`rigger/u/<unit-id>`, the crate's own
/// public authority) reproduced by name rather than imported, so this file states plainly which
/// branch it seeds without depending on the crate leaking its own worktree-dir layout too.
fn unit_branch(unit_id: &str) -> String {
    format!("rigger/u/{unit_id}")
}

/// An ordinary, conflict-free single-unit implementer: writes `a.rs` and nothing else. Mirrors
/// `tests/integrate_conflict_merge_periphery.rs::SimpleWorkDriver`.
struct SimpleWorkDriver;

impl AgentDriver for SimpleWorkDriver {
    fn spawn(
        &self,
        _a: &AgentDef,
        _prompt: &str,
        opts: &SpawnOpts,
        _emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        if opts.id.contains("/implementer#") {
            std::fs::write(Path::new(&opts.dir).join("a.rs"), "A_WORK\n").unwrap();
            return Ok(AgentResult::default());
        }
        Ok(review_or_adjudicate(opts))
    }
}

/// A driver that fails the test the instant ANY role is spawned - GAP 1's own proof that a
/// resumed, already-landed unit re-gates with ZERO lifecycle spawns (matching the implementer's
/// own hand-seeded test's identical assertion, now over a REAL two-call resume instead of a
/// single hand-seeded call).
struct PanicOnAnySpawn;

impl AgentDriver for PanicOnAnySpawn {
    fn spawn(
        &self,
        _a: &AgentDef,
        _prompt: &str,
        opts: &SpawnOpts,
        _emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        panic!(
            "a resumed, already-landed unit must re-gate with NO lifecycle spawn at all \
             (RE-GATE WHAT LANDED consults the durable log, never re-runs the unit); got a \
             spawn for {}",
            opts.id
        );
    }
}

/// Refuses every append made AFTER an earlier append already durably recorded `trigger` -
/// simulating a process crash strictly BETWEEN one already-landed mutation's own after-record
/// and whatever the next store write would have been (spec 103, criterion 3's exact crash
/// window: "a crash between that append and the post-merge re-gate which must follow it").
/// Unlike `tests/integrate_conflict_merge_periphery.rs`'s own `FailAppendContaining` (which
/// refuses the triggering append ITSELF, proving a record survives crash-before-its-own-write),
/// this proves the OPPOSITE half: the trigger succeeds for real, on disk and in the log, and
/// everything durable that was SUPPOSED to follow it is exactly what never arrives - the one
/// window this criterion's fix closes. Never a production hook: `EventStore` is the crate's own
/// injected port ([`Deps::store`]), the same seam a genuine backend failure would surface
/// through.
struct FailAfterContaining<'a> {
    inner: &'a dyn EventStore,
    trigger: &'static str,
    armed: AtomicBool,
}

impl EventStore for FailAfterContaining<'_> {
    fn append(
        &self,
        stream: &str,
        expected: ExpectedRevision,
        events: &[Event],
    ) -> Result<Appended, EsError> {
        if self.armed.load(Ordering::SeqCst) {
            return Err(EsError::Backend(format!(
                "simulated crash: append after {:?} refused",
                self.trigger
            )));
        }
        let out = self.inner.append(stream, expected, events)?;
        if events
            .iter()
            .any(|e| String::from_utf8_lossy(&e.data).contains(self.trigger))
        {
            self.armed.store(true, Ordering::SeqCst);
        }
        Ok(out)
    }
    crate::delegate_event_store_reads!();
}

/// Whether `events` carries a real, non-cached-hit-or-otherwise `GateVerdict` for the
/// POST-MERGE re-gate specifically. The post-merge replay key
/// (`src/conductor.rs::postmerge_gate_verdict_key`, private to that module) is
/// `{unit}/postmerge-gate:{gate}#{attempt}` per its own doc comment - reproduced here by
/// literal format string rather than imported, exactly as `unit_branch` is above.
fn has_postmerge_gate_verdict(events: &[Event], unit: &str, attempt: u32, gate: &str) -> bool {
    let key = format!("{unit}/postmerge-gate:{gate}#{attempt}");
    events.iter().any(|e| {
        e.type_ == contextgraph::TYPE_GATE_VERDICT && e.meta.get("replay_key") == Some(&key)
    })
}

fn base_cfg(repo_path: &str) -> Config {
    let mut cfg = Config::default();
    // Spec 89, criterion 2 relocated the scratch/worktree DEFAULT off the fixture's own repo
    // tree onto a machine-wide `<cache-home>/rigger/<encoded repo>` root, so a fixture that
    // leaves `defaults.workdir` unconfigured shares that ONE real location with every other
    // concurrently-running fixture and agent on the machine. Nesting the workdir back inside
    // THIS fixture's own unique repo tempdir restores per-test isolation, mirroring every other
    // periphery suite's identical fix (e.g. `tests/integrate_conflict_merge_periphery.rs`).
    cfg.workflow.defaults.workdir = format!("{repo_path}/.rigger-test-scratch");
    cfg.agents.insert("worker".into(), agent("worker"));
    cfg.agents.insert("lens".into(), agent("lens"));
    cfg.agents.insert("judge".into(), agent("judge"));
    cfg.workflow.gates.insert("g".into(), gate_def("exit 0"));
    cfg.workflow
        .stages
        .insert("unit-a".into(), mk_stage("unit-a", "g"));
    cfg
}

// ============================================================================================
// GAP 1: a real two-call crash-then-resume proves the PRODUCTION write path actually puts
// `pre_merge` on the wire and the resumed read path actually re-gates the real merge for real.
// ============================================================================================

#[test]
fn a_crash_right_after_landing_before_the_postmerge_regate_still_gates_for_real_on_resume() {
    let repo = temp_git_project_with_commit();
    let repo_path = repo.path().to_str().unwrap().to_string();
    let cfg = base_cfg(&repo_path);

    let store = Store::open(":memory:").unwrap();
    let driver = SimpleWorkDriver;
    {
        let failing_store = FailAfterContaining {
            inner: &store,
            trigger: "integrate-landed",
            armed: AtomicBool::new(false),
        };
        let deps = Deps {
            store: &failing_store,
            driver: &driver,
            gates: &rigger::gate::ExecRunner,
            repo: repo_path.clone(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let err = match run(&cfg, &deps) {
            Ok(_) => panic!(
                "call 1 must fail - the post-merge gate's own append is refused right after \
                 landing succeeds"
            ),
            Err(e) => e,
        };
        assert!(
            err.0.contains("simulated crash"),
            "call 1 must fail for the SIMULATED reason, not some other defect; got: {}",
            err.0
        );
    }

    // The landing itself is real and durable: a genuine merge commit is on the run branch, and
    // row 4's after-record (`integrate-landed`, now carrying `pre_merge`) made it into the log
    // before the simulated crash - exactly the state a genuine process death in that window
    // leaves behind.
    let unit_sha = git_stdout(&repo_path, &["rev-parse", "HEAD"]);
    let events_after_call_1 = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    assert!(
        has_status_marker(&events_after_call_1, "integrate-landed"),
        "row 4's after-record must already be durable before the crash this fixture \
         simulates; events: {events_after_call_1:?}"
    );
    assert!(
        !has_postmerge_gate_verdict(&events_after_call_1, "unit-a", 0, "g"),
        "the post-merge re-gate must NOT have completed in call 1 - that is the exact crash \
         window this fixture simulates; events: {events_after_call_1:?}"
    );

    // Resume: a fresh `run()` against the SAME store+repo. A driver that panics on any
    // lifecycle spawn proves the resumed call recognizes the real, already-landed merge from
    // the durable log alone.
    let driver2 = PanicOnAnySpawn;
    let deps2 = Deps {
        store: &store,
        driver: &driver2,
        gates: &rigger::gate::ExecRunner,
        repo: repo_path.clone(),
        grounder: None,
        graph: None,
        criteria: Vec::new(),
    };
    let rs = run(&cfg, &deps2).expect("call 2 must resume and re-gate the already-landed tree");
    assert_eq!(rs.units["unit-a"].status, ledger::Status::Integrated);
    assert_eq!(
        rs.units["unit-a"].attempts, 0,
        "the resumed re-gate is infrastructure recovery, never a charged remediation attempt"
    );

    let events_after_call_2 = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    assert_eq!(
        count_status_marker(&events_after_call_2, "integrate-landed"),
        1,
        "row 4's after-record must never be re-recorded by the resumed call - it was already \
         completed once, by call 1, before the crash; events: {events_after_call_2:?}"
    );
    assert!(
        has_postmerge_gate_verdict(&events_after_call_2, "unit-a", 0, "g"),
        "THE defect this criterion fixes: a real merge whose post-merge re-gate never \
         completed before a crash must still run it for real on resume, never take the true \
         no-op short circuit that used to skip straight to UnitIntegrated; \
         events: {events_after_call_2:?}"
    );
    let integrated = events_after_call_2
        .iter()
        .find(|e| e.type_ == ledger::TYPE_UNIT_INTEGRATED)
        .expect("the resumed already-landed unit must emit UnitIntegrated");
    let v: Value = serde_json::from_slice(&integrated.data).unwrap();
    assert_eq!(
        v["commit"],
        json!(unit_sha),
        "UnitIntegrated must carry the ACTUAL landed sha, never empty/stale: {v:?}"
    );
    drop(repo);
}

// ============================================================================================
// GAP 2: a landed row recorded by a binary BEFORE this fix shipped (no `pre_merge` in its
// evidence) must not crash or misresolve a resumed call - it must keep the exact pre-fix
// true-no-op behavior, the back-compat contract `landed_from_log`'s own doc comment asserts
// but never proves.
// ============================================================================================

#[test]
fn a_pre_fix_landed_row_missing_pre_merge_keeps_the_old_true_no_op_resume_behavior() {
    let repo = temp_git_project_with_commit();
    let repo_path = repo.path().to_str().unwrap().to_string();
    let cfg = base_cfg(&repo_path);
    let branch = unit_branch("unit-a");

    // A prior window's real committed work, on the unit's own durable branch - mirrors
    // `src/conductor.rs::tests::commit_on_unit_branch` via plain git (no crate-internal
    // `Worktree` needed): `git worktree add` a throwaway checkout of a fresh branch, commit,
    // then remove the checkout - the branch ref survives as the durable checkpoint.
    let seed_wt = tempfile::tempdir().unwrap();
    let seed_dir = seed_wt.path().to_str().unwrap().to_string();
    let out = Command::new("git")
        .arg("-C")
        .arg(&repo_path)
        .args(["worktree", "add", "-b", &branch, &seed_dir, "HEAD"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "test setup: git worktree add must succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::write(Path::new(&seed_dir).join("feature.rs"), "fn feature() {}\n").unwrap();
    git_commit_all(&seed_dir, "rigger: prior window work");
    let unit_sha = git_stdout(&seed_dir, &["rev-parse", "HEAD"]);
    let out = Command::new("git")
        .arg("-C")
        .arg(&repo_path)
        .args(["worktree", "remove", "--force", &seed_dir])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "test setup: git worktree remove must succeed"
    );

    // Simulate `Worktree::land`'s fast-forward already having happened: the run branch (the
    // checked-out repo) is ALREADY at the unit's own landed tip.
    let out = Command::new("git")
        .arg("-C")
        .arg(&repo_path)
        .args(["merge", "--ff-only", &branch])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "test setup: the fast-forward must succeed: {out:?}"
    );
    assert_eq!(
        git_stdout(&repo_path, &["rev-parse", "HEAD"]),
        unit_sha,
        "test setup premise: the run branch must already carry the unit's landed tip"
    );

    let store = Store::open(":memory:").unwrap();
    // A leading `RunStarted` (spec 06, unit 1: run scoping) is required for the conductor to
    // ADOPT this run and fold everything below as ITS OWN prior-window resume state, rather
    // than minting a fresh run that leaves a bare hand-seeded log behind the run-scope
    // boundary - mirrors `src/conductor.rs::tests::seed_events_in_run` (private to that
    // module), the exact technique the implementer's own sibling test relies on.
    for ev in [
        Event::new(
            rigger::run::TYPE_RUN_STARTED,
            serde_json::to_vec(&json!({"run": "test-run", "criteria": Vec::<&str>::new()}))
                .unwrap(),
        ),
        Event::new(
            ledger::TYPE_UNIT_STARTED,
            serde_json::to_vec(&json!({"id": "unit-a", "agent": "worker", "branch": branch}))
                .unwrap(),
        ),
        Event::new(
            ledger::TYPE_UNIT_STATUS,
            serde_json::to_vec(&json!({"id": "unit-a", "status": "verified"})).unwrap(),
        ),
        Event::new(
            ledger::TYPE_UNIT_STATUS,
            serde_json::to_vec(&json!({"id": "unit-a", "status": "reviewed"})).unwrap(),
        ),
        // THE LEGACY SHAPE: a real `record_landed` write from a binary built BEFORE this
        // criterion - `evidence` carries only `sha`, never `pre_merge` (the field did not
        // exist yet). `landed_from_log` must skip this row rather than guess, and the caller
        // must fall through to the SAME true no-op short circuit it always took.
        Event::new(
            ledger::TYPE_UNIT_STATUS,
            serde_json::to_vec(&json!({
                "id": "unit-a",
                "status": "integrate-landed",
                "attempt": 0,
                "evidence": {"sha": unit_sha},
            }))
            .unwrap(),
        )
        .with_meta("replay_key", "unit-a/landed#0~0"),
    ] {
        store
            .append(STREAM, ExpectedRevision::Any, std::slice::from_ref(&ev))
            .unwrap();
    }

    let driver = PanicOnAnySpawn;
    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &rigger::gate::ExecRunner,
        repo: repo_path.clone(),
        grounder: None,
        graph: None,
        criteria: Vec::new(),
    };
    let rs = run(&cfg, &deps).expect(
        "a legacy landed row missing pre_merge must never crash or error out a resumed run",
    );
    assert_eq!(rs.units["unit-a"].status, ledger::Status::Integrated);

    let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    assert!(
        !has_postmerge_gate_verdict(&events, "unit-a", 0, "g"),
        "a legacy row this fold cannot resolve `pre_merge` from must take the SAME true no-op \
         short circuit a pre-fix binary always took for this shape - it must never guess a \
         tree to re-gate; events: {events:?}"
    );
    let integrated = events
        .iter()
        .find(|e| e.type_ == ledger::TYPE_UNIT_INTEGRATED)
        .expect("the unit must still reach UnitIntegrated - unchanged from before this fix");
    let v: Value = serde_json::from_slice(&integrated.data).unwrap();
    assert_eq!(
        v["commit"],
        json!(""),
        "no regression: the true no-op short circuit's own empty-commit sentinel, exactly as \
         a pre-fix binary reported for this legacy shape - never silently substituting the \
         unit's landed sha (which this fold structurally cannot vouch for without pre_merge): \
         {v:?}"
    );
    drop(repo);
}
