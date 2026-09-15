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
//!     (`plan_stage_compensation_reverts_every_landed_commit_not_just_the_newest`);
//!   - gap 7 (round 4, `sdet-u88c4-r4-per-commit-scope-check-not-periphery-tested`): the
//!     PER-COMMIT scope check (`Worktree::files_touched_by_commit`, round 2's fix for
//!     `adv-u88c4-scope-check-nets-the-diff-not-each-commit`) that catches a LATER commit
//!     reverting an EARLIER commit's own out-of-scope touch - which the AGGREGATE
//!     `changed_since_base` diff alone would wrongly clear - had only the implementer's own
//!     inside-out regression (`plan_stage_commit_reverting_its_own_out_of_scope_touch_still_
//!     fails_the_stage`, conductor.rs), never a periphery-level proof through the public
//!     `run()` entry of what the adversary named "the ONLY safety boundary a producer commit
//!     crosses" (`plan_stage_commit_reverting_its_own_out_of_scope_touch_still_fails_the_
//!     stage_at_the_periphery`).
//!   - gap 8 (round 5, `adv-u88c4-r4-resumed-none-landing-is-permanently-uncompensable`): a
//!     new public API, `Worktree::already_landed_commits` (`src/worktree.rs`), recovers the
//!     real run-branch commit a crashed-before-emit prior attempt already landed - the
//!     implementer's own inside-out regression
//!     (`integrate_plan_commits_is_idempotent_on_a_resumed_already_landed_worktree`,
//!     conductor.rs) proves the private seam calls it and gets `Landed` back, but never that
//!     the recovered sha survives all the way through the public `run()` entry into a REAL,
//!     revertible compensation - the exact end-to-end guarantee the bug broke (a unit's own
//!     already-landed content becoming silently, permanently uncompensable). Proven here by
//!     pre-landing an amendment directly through the public `Worktree::cherry_pick_onto_run_
//!     branch` API (standing in for the crashed prior attempt), then driving the SAME branch
//!     through a fresh `run()` and a real compensating unit
//!     (`plan_stage_resumed_after_a_crash_recovers_the_real_sha_and_stays_compensable`).
//!   - gap 9 (round 5, sdet re-enumeration `sdet-u88c4-r5-surface-enumeration`; REWRITTEN round
//!     7 for operator ruling `op-u88c4-next-round-plan-commit-landing-is-log-carried-and-
//!     idempotent`): an intervening, unrelated commit landing on the run branch between a
//!     crashed prior attempt's real landing and the resume - the shape that defeated rounds
//!     4-6's tree-POSITION recovery (`Worktree::already_landed_commits`, REMOVED, rejected three
//!     review rounds running) and forced a safe-but-wrong fallback to the historical `REVIEW_
//!     ONLY_NO_ARTIFACT` marker even though the amendment genuinely landed. Round 7 replaces the
//!     position walk with `Worktree::find_landed_by_patch_id` (git patch-id, content identity,
//!     position-independent): `plan_stage_resumed_amendment_with_an_intervening_operator_
//!     commit_still_confirms_by_patch_id` proves the SAME setup now resolves CORRECTLY - the
//!     real landed sha, never the no-artifact marker - through the public `run()` entry;
//!   - gap 10 (round 7, new public API `Worktree::patch_id` / `Worktree::find_landed_by_
//!     patch_id`, ruling item (2)): the same "public API, no conductor involved" boundary gap 5
//!     closed for `cherry_pick_onto_run_branch(&[])`, applied to the two functions that replaced
//!     `already_landed_commits`. The implementer's own `worktree.rs` unit tests prove the
//!     identical mechanics from INSIDE the crate's private test module - unreachable by an
//!     external consumer of this library. `patch_id_and_find_landed_by_patch_id_are_a_public_
//!     content_identity_api` drives both directly through the public `Worktree` API, with no
//!     `run()` involved at all: stability across a cherry-pick, difference for different
//!     content, confirmation across an intervening unrelated commit, and refusal to confirm
//!     content that was never landed;
//!   - gap 11 (round 7, new serialized form `plan-intent:<unit>`, ruling item (1) "INTENT IS LOG
//!     STATE FIRST"): `RunCtx::record_plan_intent` writes a `DecisionMade`-shaped record naming
//!     every commit the producer intends to land, BEFORE any git mutation - write-only audit
//!     trail today, the same shape of gap this file's gap 2 closed for `UnitIntegrated.shas`
//!     when IT was write-only. `plan_intent_record_is_log_carried_before_any_git_mutation_and_
//!     names_the_original_shas` proves the record's shape (the correct, ordered ORIGINAL - never
//!     the cherry-pick-minted landed - shas) and its position: strictly before the `UnitIntegrated`
//!     the same call eventually produces;
//!   - gap 12 (round 7, new fold arm `RunCtx::read_plan_landed` / `record_plan_landed`, ruling
//!     item (2) "reachable... by patch-id OR BY THE RECORDED LANDED SHA" - the second of the
//!     ruling's two named mechanisms, gap 9 above being the first): a crash strictly AFTER a
//!     prior call both landed an amendment for real and recorded its own `plan-landed:<unit>`
//!     confirmation, but BEFORE `UnitIntegrated` - hand-seeded onto the store with the EXACT
//!     shape `record_plan_landed` itself writes (gap 2's legacy-event technique), then adopted by
//!     a fresh `run()` AFTER filler commits push the landed sha beyond the patch-id search's own
//!     window, isolating the log record as the ONLY mechanism that can recover it.
//!     `plan_stage_resumed_with_a_pre_existing_plan_landed_record_recovers_without_any_new_git_
//!     mutation` proves the resumed call trusts the log record directly - no new cherry-pick, no
//!     successful patch-id search - and still reaches `Integrated` with the real landed sha and
//!     content a downstream stage can read.
//!   - gap 13 (round 8, sdet re-enumeration `sdet-u88c4-r8-surface-enumeration`; fix for
//!     `arch-u88c4-r7-classification-skip-is-single-shot-not-a-loop` reopened through a narrower
//!     trigger): the leftover-`CHERRY_PICK_HEAD` classification in `Worktree::cherry_pick_onto_
//!     run_branch` issued exactly ONE `--skip` before giving up, so a leftover marker sitting
//!     ahead of TWO OR MORE chained empty commits - an ordinary shape for a multi-commit plan
//!     amendment resumed after a crash - re-paused on the second empty commit and hard-errored.
//!     Round 8 replaces it with a bounded `skips_left` loop. This changes no public SIGNATURE
//!     (probe 1 found nothing new since round 7), only the BODY of an already-public method, so
//!     it is invisible to a bare grep - caught only by reading the diff. The implementer's own
//!     `worktree.rs` test module proves the mechanics from inside the crate's private test
//!     module; `cherry_pick_onto_run_branch_self_heals_a_leftover_marker_ahead_of_two_chained_
//!     empty_commits_at_the_periphery` drives the SAME scenario through nothing but the PUBLIC
//!     `Worktree` API, no conductor or `run()` involved - the same "public API, no conductor"
//!     boundary gaps 5 and 10 above already established for this file's other `Worktree`
//!     methods.
//!   - gap 14 (round 8, new cross-module seam): `RunCtx::integrate_plan_commits` is now a thin
//!     wrapper over `integrate_plan_commits_inner` that tags EVERY hard Err with a new, private
//!     `PLAN_LANDING_MARKER` sentinel (the fifth alongside the pre-existing PARKED/BUDGET/
//!     DEGENERATE/MISMATCH markers), and `RunCtx::run_wave` gained a matching `Err(e) if
//!     is_plan_landing_failed(&e)` arm that propagates the halt loudly but records NO per-unit
//!     lesson and charges NO attempt - fixing `adv-u88c4-r7-plan-commit-errors-still-carry-no-
//!     infra-fault-marker`, the same structural gap named at round 2 and round 4 and never
//!     closed by three successive git-level-only fixes to the trigger. The implementer's own
//!     `integrate_plan_commits_wraps_any_hard_error_with_the_plan_landing_marker` and `a_plan_
//!     landing_infra_fault_halts_the_run_loudly_with_no_per_unit_lesson_or_attempt` force the
//!     identical failure (a `record_plan_intent` store-append error) through a crate-PRIVATE
//!     `FailingStore` double and a crate-PRIVATE `Stub` driver, both defined inside
//!     `conductor.rs`'s own `#[cfg(test)] mod tests` - unreachable from outside the crate,
//!     exactly the class of gap gap 1's own header names for `Stub`. `a_plan_landing_store_
//!     failure_halts_the_run_loudly_with_no_per_unit_lesson_or_charged_attempt_at_the_periphery`
//!     forces the SAME failure from OUTSIDE the crate, through nothing but the PUBLIC
//!     `EventStore` trait every real `Deps::store` caller already implements against
//!     (`FailingExternalStore`, an independently-authored double built only on that public
//!     trait) plus the public `run()` entry and an independently-authored `AgentDriver` -
//!     proving the new cross-module wiring is reachable by, and behaves correctly for, a
//!     genuine outside caller of this library.
//!   - gap 15 (round 9, sdet re-enumeration `sdet-u88c4-r9-surface-enumeration`; fix for
//!     `adj-u88c4-r8-verdict-reject` upholding both `sdet-u88c4-r8-single-commit-leftover-
//!     marker-hard-errors-on-missing-sequencer-todo` and `adv-u88c4-r8-single-commit-trigger-
//!     is-any-still-pending-len-1-not-just-single-commit-units`): the private
//!     `Worktree::sequencer_todo_remaining` helper - which `cherry_pick_onto_run_branch`'s
//!     leftover-marker classification (gap 13) calls to bound its own skip loop - hard-errored
//!     on `io::ErrorKind::NotFound` reading `.git/sequencer/todo`, but git NEVER materializes
//!     that file for a plain single-sha `git cherry-pick`, which is exactly the shape
//!     `cherry_pick_onto_run_branch` issues whenever its caller's `shas` has exactly one
//!     entry, the routine steady state of an iterative multi-commit plan amendment (a
//!     conductor resume whose `still_pending` has shrunk to one confirmed-pending sha, per
//!     `integrate_plan_commits_inner`'s own `prior_landed`/`find_landed_by_patch_id`
//!     recomputation), not merely a literal one-commit-total unit. Round 9 reads a missing
//!     todo file as exactly ONE remaining entry instead of propagating the io error. All five
//!     probes over the round's diff (`2540768^..2540768`) return EMPTY for new/changed public
//!     signatures, trait impls, CLI registrations, and event/serialized forms - this changes no
//!     public SIGNATURE (probe 1 found nothing new since round 8), only the BODY of a PRIVATE
//!     helper behind an already-public method, the same "invisible to a bare grep, caught only
//!     by reading the diff" shape gap 13's own header names. The implementer's own two
//!     `worktree.rs` unit tests (`cherry_pick_onto_run_branch_self_heals_a_leftover_marker_
//!     with_no_sequencer_todo_file`, a literal one-commit-total unit; `..._self_heals_when_a_
//!     multi_commit_amendment_shrinks_to_one_still_pending`, the conductor's real resume shape)
//!     prove the mechanics from INSIDE the crate's private test module - unreachable from
//!     outside the crate. Both angles collapse to the IDENTICAL public-API call shape at the
//!     `Worktree` boundary (a single-element `shas` slice against a leftover marker with no
//!     `sequencer/todo` file), so one periphery test proves the full externally-observable
//!     contract: `cherry_pick_onto_run_branch_self_heals_a_leftover_marker_with_a_missing_
//!     sequencer_todo_file_at_the_periphery` drives nothing but the PUBLIC `Worktree` API, no
//!     conductor or `run()` involved - the same "public API, no conductor" boundary gaps 5, 10
//!     and 13 above already established for this file's other `Worktree` methods.

mod common;

use rigger::conductor::{
    run, AgentDriver, AgentResult, Deps, Error, SpawnOpts, META_COMPENSATED,
    META_COMPENSATE_TARGET, REVIEW_ONLY_NO_ARTIFACT, STREAM,
};
use rigger::config::{AgentDef, Config, Gate, ReviewPanel, Stage};
use rigger::contextgraph;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{
    Appended, Direction, Event, EventStore, ExpectedRevision, Filter, Position, Revision,
    Subscription,
};
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
    /// The worktree's own HEAD sha immediately after this driver's commit loop finishes (gap
    /// 11) - the ORIGINAL, pre-cherry-pick identity a caller has no other way to read back
    /// once `opts.dir` is torn down with the stage. A same-parent, same-committer-second
    /// cherry-pick can legitimately mint a BYTE-IDENTICAL object (git's content addressing),
    /// so comparing this against the eventual landed sha for inequality would be racy; reading
    /// it back directly and comparing for EQUALITY against what the intent record names is the
    /// sound proof. `None` (the default) for every existing gap-1-through-10 test, which never
    /// reads this field.
    committed_sha: Mutex<Option<String>>,
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
            if !self.commits.is_empty() {
                if let Ok(out) = std::process::Command::new("git")
                    .arg("-C")
                    .arg(&opts.dir)
                    .args(["rev-parse", "HEAD"])
                    .output()
                {
                    if out.status.success() {
                        *self.committed_sha.lock().unwrap() =
                            Some(String::from_utf8_lossy(&out.stdout).trim().to_string());
                    }
                }
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
    // Spec 89 criterion 2 ruling item 2: nest the scratch/worktree default back inside
    // this fixture's own repo tempdir so this real, worktree-creating conductor::run()
    // never reaches the real ambient XDG_CACHE_HOME/HOME cache-home default.
    cfg.workflow.defaults.workdir = common::isolated_workdir(repo.path());
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
    // Spec 89 criterion 2 ruling item 2: nest the scratch/worktree default back inside
    // this fixture's own repo tempdir so this real, worktree-creating conductor::run()
    // never reaches the real ambient XDG_CACHE_HOME/HOME cache-home default.
    cfg.workflow.defaults.workdir = common::isolated_workdir(repo.path());
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
    // Spec 89 criterion 2 ruling item 2: nest the scratch/worktree default back inside
    // this fixture's own repo tempdir so this real, worktree-creating conductor::run()
    // never reaches the real ambient XDG_CACHE_HOME/HOME cache-home default.
    cfg.workflow.defaults.workdir = common::isolated_workdir(repo.path());
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
    // Spec 89 criterion 2 ruling item 2: nest the scratch/worktree default back inside
    // this fixture's own repo tempdir so this real, worktree-creating conductor::run()
    // never reaches the real ambient XDG_CACHE_HOME/HOME cache-home default.
    cfg.workflow.defaults.workdir = common::isolated_workdir(repo.path());
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
    // Spec 89 criterion 2 ruling item 2: nest the scratch/worktree default back inside
    // this fixture's own repo tempdir so this real, worktree-creating conductor::run()
    // never reaches the real ambient XDG_CACHE_HOME/HOME cache-home default.
    cfg.workflow.defaults.workdir = common::isolated_workdir(repo.path());
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

/// Criterion 4, gap 7 (round 4, `sdet-u88c4-r4-per-commit-scope-check-not-periphery-tested`):
/// the PER-COMMIT scope check (`Worktree::files_touched_by_commit`, this file's own header
/// names it new public API) is the fix for `adv-u88c4-scope-check-nets-the-diff-not-each-
/// commit` - CONFIRMED BY LIVE GIT REPRO that the AGGREGATE `changed_since_base` diff alone
/// nets a path to NOTHING when a LATER commit in the same producer attempt reverts an
/// EARLIER commit's own non-`specs/` touch to it. The implementer's own regression for this
/// exact shape (`plan_stage_commit_reverting_its_own_out_of_scope_touch_still_fails_the_
/// stage`, `src/conductor.rs`) is INSIDE-OUT: crate-private `Stub` driver, `mod tests`,
/// unreachable from outside the crate - so, per this file's own opening claim ("neither
/// inside-out layer covers... at all"), this specific gap was left uncovered at the
/// periphery despite being, in the adversary's own words, "the ONLY safety boundary a
/// producer commit crosses" before landing permanently on the shared run branch
/// (`PlanCommitOutcome::Landed` is never `review_unit`'d). Proven here through the public
/// `run()` entry, an independently-authored driver, and real git - never the crate's own
/// private fixture.
#[test]
fn plan_stage_commit_reverting_its_own_out_of_scope_touch_still_fails_the_stage_at_the_periphery() {
    let touched_path = "docs/existing.md";
    let repo = init_repo();
    let repo_path = repo.path().to_str().unwrap().to_string();
    // Pre-seed the path on the run branch BEFORE the producer worktree branches off, so
    // reverting it back is a real, in-history no-op relative to base - not merely deleting a
    // path base never had (mirrors the implementer's own inside-out fixture for this shape).
    std::fs::create_dir_all(repo.path().join("docs")).unwrap();
    std::fs::write(repo.path().join(touched_path), "seed\n").unwrap();
    run_git(&repo_path, &["add", "-A"]);
    run_git(&repo_path, &["commit", "-q", "-m", "seed docs/existing.md"]);

    let mut cfg = Config::default();
    // Spec 89 criterion 2 ruling item 2: nest the scratch/worktree default back inside
    // this fixture's own repo tempdir so this real, worktree-creating conductor::run()
    // never reaches the real ambient XDG_CACHE_HOME/HOME cache-home default.
    cfg.workflow.defaults.workdir = common::isolated_workdir(repo.path());
    cfg.agents.insert("planner".into(), agent("planner"));
    cfg.workflow.stages.insert("plan".into(), plan_stage());

    let store = Store::open(":memory:").unwrap();
    // Two commits touching the SAME non-specs path: the first plants "scope creep", the
    // second reverts it back to "seed" - the AGGREGATE base...HEAD diff for this path is
    // empty even though each commit individually touched it, so only the PER-COMMIT walk
    // this criterion added can catch it.
    let driver = PlanAmendDriver::new("planner")
        .commit(&[(touched_path, "scope creep\n")])
        .commit(&[(touched_path, "seed\n")]);
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
        "a commit sequence that touches a non-specs path and later reverts it must still \
         fail - the per-commit scope check must catch what the aggregate diff alone would \
         wrongly clear"
    );

    let events = store
        .read_all(0, Direction::Forward, &Filter::default())
        .unwrap();
    let failed = events
        .iter()
        .find(|e| e.type_ == ledger::TYPE_UNIT_FAILED)
        .expect("the reverted-touch sequence must record a UnitFailed");
    let body = String::from_utf8_lossy(&failed.data);
    assert!(
        body.contains("\"cause\":\"reject\""),
        "a per-commit scope violation is stamped the same cause tag an ordinary review \
         reject is; got: {body}"
    );

    // The pre-seeded path is UNCHANGED on the run branch - neither the transient scope
    // creep nor the producer's own revert-back ever landed, since the whole sequence was
    // refused before any of it reached the run branch.
    assert_eq!(
        std::fs::read_to_string(repo.path().join(touched_path)).unwrap(),
        "seed\n",
        "the pre-existing path must be untouched by a refused plan-stage commit sequence"
    );
}

/// Criterion 4, gap 8 (round 5, `adv-u88c4-r4-resumed-none-landing-is-permanently-
/// uncompensable`): a crash between a producer's cherry-pick REALLY landing an amendment and
/// the `UnitIntegrated` that would have recorded it must not make that real, permanent commit
/// silently, permanently uncompensable - indistinguishable from a producer that never touched
/// the run branch at all. Simulated here entirely through PUBLIC API a crashed-and-resumed
/// process would itself use: the amendment is pre-landed for real via `Worktree::cherry_pick_
/// onto_run_branch` directly (standing in for the crashed prior attempt's own successful git
/// mutation), then a fresh `run()` adopts the SAME producer branch - its own worktree still
/// carries only the ORIGINAL (pre-landing) commit identity, so the conductor's `integrate_plan_
/// commits` must recover what already landed rather than discarding it as a bare no-artifact
/// marker. Proven both by the event's own shape AND by a REAL downstream compensation actually
/// reverting the recovered commit from the run branch - the concrete, business-relevant
/// consequence permanently uncompensable content would otherwise have.
#[test]
fn plan_stage_resumed_after_a_crash_recovers_the_real_sha_and_stays_compensable() {
    let repo = init_repo();
    let repo_path = repo.path().to_str().unwrap().to_string();

    // A PRIOR window's planner committed its amendment onto the deterministic `rigger/u/plan`
    // branch, via its own throwaway worktree - never touching the run branch directly.
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
        seed_dir.path().join("specs").join("97-resumed.md"),
        "amend\n",
    )
    .unwrap();
    assert!(std::process::Command::new("git")
        .arg("-C")
        .arg(seed_dir.path())
        .args(["add", "-A"])
        .status()
        .unwrap()
        .success());
    // A FIXED, deliberately old author/committer date - never the wall-clock "now" a bare
    // `git commit` would use - so the cherry-pick just below (which stamps its OWN committer
    // time as real "now") cannot coincidentally reproduce a byte-identical commit object (the
    // same-committer-second case `CherryPickOutcome::Picked`'s own doc comment names). A real
    // crash-and-later-resume always spans wall-clock seconds, so the two dates would never
    // coincide in production; this only guards the test against a same-second fluke.
    assert!(std::process::Command::new("git")
        .arg("-C")
        .arg(seed_dir.path())
        .args(["commit", "-q", "-m", "amend"])
        .env("GIT_AUTHOR_DATE", "2000-01-01T00:00:00")
        .env("GIT_COMMITTER_DATE", "2000-01-01T00:00:00")
        .status()
        .unwrap()
        .success());
    let original_shas = seed.commits_since_base().unwrap();
    assert_eq!(original_shas.len(), 1);

    // The CRASHED PRIOR ATTEMPT's own successful git mutation: land it for real, through the
    // same public API `integrate_plan_commits` itself calls - simulating a process death
    // strictly AFTER this succeeds but BEFORE the caller ever records it.
    let prior_landed = match seed.cherry_pick_onto_run_branch(&original_shas).unwrap() {
        CherryPickOutcome::Picked(landed) => landed,
        CherryPickOutcome::Conflict(detail) => {
            panic!("a clean specs/-only cherry-pick must not conflict: {detail}")
        }
    };
    assert_eq!(prior_landed.len(), 1, "one commit in, one commit landed");
    let prior_landed_sha = prior_landed[0].clone();
    assert_eq!(
        std::fs::read_to_string(repo.path().join("specs").join("97-resumed.md")).unwrap(),
        "amend\n",
        "precondition: the amendment's content is already on the run branch before run() starts"
    );
    seed.remove().unwrap(); // only the transient dir goes; the branch persists.

    // A FRESH run() adopts the SAME producer branch. Its own worktree still carries only the
    // ORIGINAL (pre-landing) commit identity - `commits_since_base` is identity-based, so it
    // recomputes the exact same non-empty `original_shas` even though the content already
    // landed under a different, cherry-pick-minted object. The planner's own spawn commits
    // NOTHING new (no `.commit(...)` calls) - there is nothing left for it to do; a genuine
    // resume never re-does work a crashed attempt already finished at the git level.
    let mut cfg = Config::default();
    // Spec 89 criterion 2 ruling item 2: nest the scratch/worktree default back inside
    // this fixture's own repo tempdir so this real, worktree-creating conductor::run()
    // never reaches the real ambient XDG_CACHE_HOME/HOME cache-home default.
    cfg.workflow.defaults.workdir = common::isolated_workdir(repo.path());
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
    // checker's own work always approves; its verdict names "plan" as the real defect
    // source, exactly like the sibling multi-commit compensation test - the concrete,
    // business-relevant proof that the recovered commit is genuinely revertible, not merely
    // recorded.
    let driver = PlanAmendDriver::new("planner").judging("judge", "plan");
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
         (the planner's fresh spawn commits nothing new)"
    );

    // (1) THE RECOVERED EVENT SHAPE: the FIRST (pre-compensation) UnitIntegrated for "plan"
    // must name the REAL landed sha - never the bare REVIEW_ONLY_NO_ARTIFACT marker a genuine
    // no-commit producer would carry, which is exactly what made the pre-fix behavior
    // permanently uncompensable.
    let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    let unit_of = |e: &Event| -> Option<String> {
        serde_json::from_slice::<Value>(&e.data)
            .ok()
            .and_then(|v| v.get("id").and_then(Value::as_str).map(str::to_string))
    };
    let first_integrated = events
        .iter()
        .find(|e| e.type_ == ledger::TYPE_UNIT_INTEGRATED && unit_of(e).as_deref() == Some("plan"))
        .expect("plan's first integration must be recorded");
    let first_v: Value = serde_json::from_slice(&first_integrated.data).unwrap();
    assert_ne!(
        first_v["commit"].as_str(),
        Some(REVIEW_ONLY_NO_ARTIFACT),
        "a resumed, already-landed amendment must never be recorded as a bare no-artifact \
         marker - that is exactly what made it permanently uncompensable"
    );
    assert_eq!(
        first_v["commit"].as_str(),
        Some(prior_landed_sha.as_str()),
        "the recovered commit must be the REAL sha the crashed prior attempt actually landed"
    );

    // (2) REAL COMPENSABILITY: an evented (not history-rewriting) revert of the recovered
    // commit actually reaches the run branch - the concrete consequence the bug's silent,
    // permanent uncompensability would otherwise have prevented forever.
    let log = std::process::Command::new("git")
        .arg("-C")
        .arg(&repo_path)
        .args(["log", "--pretty=%s"])
        .output()
        .unwrap();
    let log = String::from_utf8_lossy(&log.stdout);
    assert!(
        log.lines()
            .any(|l| l.contains("compensate plan") && l.contains(prior_landed_sha.as_str())),
        "the run branch must carry an evented revert of the recovered commit {prior_landed_sha}; \
         log:\n{log}"
    );
    assert!(
        !repo.path().join("specs").join("97-resumed.md").exists(),
        "the recovered-then-compensated amendment must be gone from the run branch"
    );
}

/// Round 7, closing operator ruling `op-u88c4-next-round-plan-commit-landing-is-log-
/// carried-and-idempotent` items (1)/(2): rounds 4-6's recovery (`already_landed_commits`,
/// REMOVED) confirmed an already-landed commit by tree POSITION - the run branch's most
/// recent N commits had to match, position for position, the originals' own trees. That
/// heuristic was rejected three review rounds running (arch-u88c4-r6-operator-ruling-
/// unimplemented-still-a-heuristic, sdet-u88c4-r6-fallback-still-violates-ruling-item2-
/// proven-by-its-own-test) precisely because ANY unrelated commit landing on the run branch
/// in between - a concurrent sibling unit, an operator edit - shifts every position, forcing
/// a safe-but-wrong fallback to the historical `REVIEW_ONLY_NO_ARTIFACT` marker even though
/// the amendment genuinely, permanently landed (making it silently uncompensable forever,
/// the exact defect class the ruling exists to close).
///
/// This test proves the fix: the SAME "operator race on the run branch meanwhile" setup now
/// resolves CORRECTLY. Between the crashed prior attempt's real cherry-pick landing and this
/// resume, the run branch gains an UNRELATED commit of its own. `commits_since_base` still
/// recomputes the same single original (pre-landing) sha, and the resumed process has no
/// durable `plan-landed` record yet (the crash happened before ANY log write - crash point 1
/// of ruling item 4), so the resume must recover it via `Worktree::find_landed_by_patch_id`
/// (CONTENT identity - a patch-id never shifts when something unrelated lands nearby) rather
/// than fall back to the no-artifact marker.
#[test]
fn plan_stage_resumed_amendment_with_an_intervening_operator_commit_still_confirms_by_patch_id() {
    let repo = init_repo();
    let repo_path = repo.path().to_str().unwrap().to_string();

    // A PRIOR window's planner committed its amendment onto the deterministic `rigger/u/plan`
    // branch, via its own throwaway worktree - identical setup to the sibling recovery test.
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
        seed_dir.path().join("specs").join("98-diverged.md"),
        "amend\n",
    )
    .unwrap();
    assert!(std::process::Command::new("git")
        .arg("-C")
        .arg(seed_dir.path())
        .args(["add", "-A"])
        .status()
        .unwrap()
        .success());
    // A FIXED, deliberately old author/committer date - never the wall-clock "now" a bare
    // `git commit` would use - so the cherry-pick just below (which stamps its OWN
    // committer time as real "now") cannot coincidentally reproduce a byte-identical
    // commit object (the same-committer-second case `CherryPickOutcome::Picked`'s own doc
    // comment names - see the sibling recovery test's identical guard). Without this, a
    // fast test run risks the landed pick being the SAME object as `original_shas[0]`,
    // which would make it a (misleading) ancestor of itself once the operator commit
    // lands on top - never exercising the tree-mismatch this test exists to prove.
    assert!(std::process::Command::new("git")
        .arg("-C")
        .arg(seed_dir.path())
        .args(["commit", "-q", "-m", "amend"])
        .env("GIT_AUTHOR_DATE", "2000-01-01T00:00:00")
        .env("GIT_COMMITTER_DATE", "2000-01-01T00:00:00")
        .status()
        .unwrap()
        .success());
    let original_shas = seed.commits_since_base().unwrap();
    assert_eq!(original_shas.len(), 1);

    // The CRASHED PRIOR ATTEMPT's own successful git mutation: land it for real.
    let prior_landed = match seed.cherry_pick_onto_run_branch(&original_shas).unwrap() {
        CherryPickOutcome::Picked(landed) => landed,
        CherryPickOutcome::Conflict(detail) => {
            panic!("a clean specs/-only cherry-pick must not conflict: {detail}")
        }
    };
    assert_eq!(prior_landed.len(), 1, "one commit in, one commit landed");
    assert_ne!(
        prior_landed[0], original_shas[0],
        "the landed pick must be a genuinely different commit object from the original \
         (guaranteed by the fixed old commit date above), so it can become a real ancestor \
         of the operator's later commit rather than colliding with it by identity"
    );
    seed.remove().unwrap(); // only the transient dir goes; the branch persists.

    // MEANWHILE: the run branch independently gains an unrelated commit of its own - the
    // named "operator race" the recovery's doc comment defends against. This is the ONLY
    // difference from the sibling recovery test's setup.
    std::fs::write(
        repo.path().join("specs").join("99-unrelated.md"),
        "unrelated\n",
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
        .args(["commit", "-q", "-m", "unrelated meanwhile"])
        .status()
        .unwrap()
        .success());
    let head_before_run = run_git(&repo_path, &["rev-parse", "HEAD"]);

    // A FRESH run() adopts the SAME producer branch, exactly like the sibling recovery
    // test - the planner's fresh spawn commits nothing new.
    let mut cfg = Config::default();
    // Spec 89 criterion 2 ruling item 2: nest the scratch/worktree default back inside
    // this fixture's own repo tempdir so this real, worktree-creating conductor::run()
    // never reaches the real ambient XDG_CACHE_HOME/HOME cache-home default.
    cfg.workflow.defaults.workdir = common::isolated_workdir(repo.path());
    cfg.agents.insert("planner".into(), agent("planner"));
    cfg.workflow.stages.insert("plan".into(), plan_stage());

    let store = Store::open(":memory:").unwrap();
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
        ledger::Status::Integrated,
        "a resume that confirms by content must still converge safely"
    );

    // THE FIX: the real, permanently-landed commit is now correctly recognized DESPITE
    // the intervening unrelated commit - never the no-artifact marker a position-based
    // heuristic would have wrongly fallen back to.
    assert_ne!(
        rs.units["plan"].commit, REVIEW_ONLY_NO_ARTIFACT,
        "an intervening unrelated commit must never defeat content-based recovery"
    );
    assert_eq!(
        rs.units["plan"].commit, prior_landed[0],
        "the recovered commit must be the REAL sha the crashed prior attempt actually landed"
    );
    let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    let unit_of = |e: &Event| -> Option<String> {
        serde_json::from_slice::<Value>(&e.data)
            .ok()
            .and_then(|v| v.get("id").and_then(Value::as_str).map(str::to_string))
    };
    let integrated = events
        .iter()
        .find(|e| e.type_ == ledger::TYPE_UNIT_INTEGRATED && unit_of(e).as_deref() == Some("plan"))
        .expect("plan's integration must be recorded");
    let v: Value = serde_json::from_slice(&integrated.data).unwrap();
    assert_eq!(
        v.get("shas").and_then(Value::as_array).map(|a| a.len()),
        Some(1),
        "a genuinely confirmed recovery must carry the real sha in `shas`; got: {v}"
    );

    // The run branch's git state is UNTOUCHED by this resolution - confirmation is a
    // read-only patch-id search, never a new commit; neither the operator's unrelated
    // commit nor the earlier landed content moves.
    assert_eq!(
        run_git(&repo_path, &["rev-parse", "HEAD"]),
        head_before_run,
        "content-based confirmation must not mutate the run branch at all"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("specs").join("98-diverged.md")).unwrap(),
        "amend\n"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("specs").join("99-unrelated.md")).unwrap(),
        "unrelated\n"
    );
}

/// Criterion 4, gap 10 (round 7, new public API `Worktree::patch_id` / `Worktree::
/// find_landed_by_patch_id`, operator ruling `op-u88c4-next-round-plan-commit-landing-is-log-
/// carried-and-idempotent` item (2)): the same "public API, no conductor involved at all"
/// boundary gap 5 closed for `cherry_pick_onto_run_branch(&[])`, applied to the two functions
/// that replaced `Worktree::already_landed_commits` (removed, round 7, rejected three review
/// rounds running as a tree-POSITION heuristic). The implementer's own `worktree.rs` unit tests
/// (`patch_id_is_stable_across_a_cherry_pick_but_differs_for_different_content`,
/// `find_landed_by_patch_id_recovers_by_content_never_by_position`) prove the identical git
/// mechanics from INSIDE the crate's private test module - unreachable by an external consumer
/// of this library, exactly like every other "new public API" gap this file exists to close.
/// This test drives the same contract through the PUBLIC `Worktree` API only, with no `run()`
/// or conductor involvement whatsoever: `patch_id` is stable across a cherry-pick (content
/// identity, never object identity) and differs for genuinely different content;
/// `find_landed_by_patch_id` confirms a landed commit by content DESPITE an intervening,
/// unrelated commit shifting every tree position - the exact defect class the ruling exists to
/// close - and refuses to confirm content that was never landed at all, never a guess.
#[test]
fn patch_id_and_find_landed_by_patch_id_are_a_public_content_identity_api() {
    let repo = init_repo();
    let repo_path = repo.path().to_str().unwrap().to_string();
    let wt_dir = tempfile::tempdir().unwrap();
    let wt = Worktree::create(
        &repo_path,
        wt_dir.path().to_str().unwrap(),
        "rigger/u/ext-patch-id",
        "",
    )
    .unwrap();

    std::fs::create_dir_all(wt_dir.path().join("specs")).unwrap();
    std::fs::write(wt_dir.path().join("specs").join("95-a.md"), "amend a\n").unwrap();
    assert!(std::process::Command::new("git")
        .arg("-C")
        .arg(wt_dir.path())
        .args(["add", "-A"])
        .status()
        .unwrap()
        .success());
    // A FIXED, deliberately old author/committer date - never the wall-clock "now" a bare
    // `git commit` would use - so the cherry-pick below (which stamps its own committer time as
    // real "now") cannot coincidentally reproduce a byte-identical commit object in the rare
    // same-committer-second case, which would defeat this test's own `assert_ne!` below (the
    // implementer's own sibling unit test guards the identical risk the identical way).
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(wt_dir.path())
        .args(["commit", "-q", "-m", "amend a"])
        .env("GIT_AUTHOR_DATE", "2000-01-01T00:00:00")
        .env("GIT_COMMITTER_DATE", "2000-01-01T00:00:00")
        .output()
        .unwrap();
    assert!(out.status.success(), "fixed-date commit failed");
    let original = run_git(wt_dir.path().to_str().unwrap(), &["rev-parse", "HEAD"]);

    let landed = match wt
        .cherry_pick_onto_run_branch(std::slice::from_ref(&original))
        .unwrap()
    {
        CherryPickOutcome::Picked(landed) => landed,
        CherryPickOutcome::Conflict(detail) => {
            panic!("a clean specs/-only cherry-pick must not conflict: {detail}")
        }
    };
    assert_eq!(landed.len(), 1);
    assert_ne!(
        landed[0], original,
        "a cherry-pick mints a genuinely different commit object"
    );

    // (1) STABLE ACROSS A CHERRY-PICK: the same content re-committed under a different object
    // carries the SAME `patch_id` - called directly, bare, no conductor involved.
    assert_eq!(
        wt.patch_id(&original).unwrap(),
        wt.patch_id(&landed[0]).unwrap(),
        "the same content re-committed by a cherry-pick must carry the SAME patch_id"
    );

    // (2) DIFFERENT CONTENT DIFFERS: proves this is a real content hash, not a constant.
    std::fs::write(wt_dir.path().join("specs").join("95-b.md"), "amend b\n").unwrap();
    assert!(std::process::Command::new("git")
        .arg("-C")
        .arg(wt_dir.path())
        .args(["add", "-A"])
        .status()
        .unwrap()
        .success());
    assert!(std::process::Command::new("git")
        .arg("-C")
        .arg(wt_dir.path())
        .args(["commit", "-q", "-m", "amend b"])
        .status()
        .unwrap()
        .success());
    let other = run_git(wt_dir.path().to_str().unwrap(), &["rev-parse", "HEAD"]);
    assert_ne!(
        wt.patch_id(&original).unwrap(),
        wt.patch_id(&other).unwrap(),
        "different content must carry a different patch_id"
    );

    // (3) CONFIRMS BY CONTENT DESPITE AN INTERVENING, UNRELATED COMMIT - the exact position
    // shift that defeated the removed tree-position walk.
    std::fs::write(repo.path().join("specs").join("95-unrelated.md"), "x\n").unwrap();
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
        .args(["commit", "-q", "-m", "unrelated meanwhile"])
        .status()
        .unwrap()
        .success());
    let recovered = wt
        .find_landed_by_patch_id(&original, 50)
        .unwrap()
        .expect("content-based recovery must succeed despite the intervening commit");
    assert_eq!(
        recovered, landed[0],
        "the recovered sha must be the real, reachable run-branch commit"
    );

    // (4) NEVER A GUESS: content that was never landed at all must never be confirmed.
    assert_eq!(
        wt.find_landed_by_patch_id(&other, 50).unwrap(),
        None,
        "content that was never landed must never be confirmed - never a guess"
    );

    wt.remove().unwrap();
}

/// Criterion 4, gap 11 (round 7, new serialized form `plan-intent:<unit>`, operator ruling item
/// (1) "INTENT IS LOG STATE FIRST: before any git mutation, the step records the ordered list of
/// plan-stage shas it intends to land"): `RunCtx::record_plan_intent` writes a `DecisionMade`-
/// shaped record BEFORE any git mutation - write-only audit trail today (`RunCtx::
/// read_plan_landed`, the only outcome-driving read, looks for a DIFFERENT id, `plan-landed:
/// <unit>`) - the same shape of gap this file's gap 2 closed for `UnitIntegrated.shas` when IT
/// was write-only. Proves the record's shape (the correct, ordered ORIGINAL shas - never the
/// cherry-pick-minted landed sha) and its position in the stream: strictly BEFORE the
/// `UnitIntegrated` the same call eventually produces, the literal periphery-observable fact
/// "before any git mutation" requires.
#[test]
fn plan_intent_record_is_log_carried_before_any_git_mutation_and_names_the_original_shas() {
    let repo = init_repo();
    let repo_path = repo.path().to_str().unwrap().to_string();

    let mut cfg = Config::default();
    // Spec 89 criterion 2 ruling item 2: nest the scratch/worktree default back inside
    // this fixture's own repo tempdir so this real, worktree-creating conductor::run()
    // never reaches the real ambient XDG_CACHE_HOME/HOME cache-home default.
    cfg.workflow.defaults.workdir = common::isolated_workdir(repo.path());
    cfg.agents.insert("planner".into(), agent("planner"));
    cfg.workflow.stages.insert("plan".into(), plan_stage());

    let store = Store::open(":memory:").unwrap();
    let driver = PlanAmendDriver::new("planner").commit(&[("specs/96-intent.md", "amend\n")]);
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
    assert_eq!(rs.units["plan"].status, ledger::Status::Integrated);

    let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    let id_of = |e: &Event| -> Option<String> {
        serde_json::from_slice::<Value>(&e.data)
            .ok()
            .and_then(|v| v.get("id").and_then(Value::as_str).map(str::to_string))
    };
    let intent_pos = events
        .iter()
        .position(|e| {
            e.type_ == contextgraph::TYPE_DECISION_MADE
                && id_of(e).as_deref() == Some("plan-intent:plan")
        })
        .expect("the plan-intent:plan record must be recorded");
    let integrated_pos = events
        .iter()
        .position(|e| {
            e.type_ == ledger::TYPE_UNIT_INTEGRATED && id_of(e).as_deref() == Some("plan")
        })
        .expect("plan's integration must be recorded");
    assert!(
        intent_pos < integrated_pos,
        "the intent record (position {intent_pos}) must be recorded strictly BEFORE the \
         integration it precedes (position {integrated_pos}) - log state first, before any \
         git mutation"
    );

    let intent: Value = serde_json::from_slice(&events[intent_pos].data).unwrap();
    assert_eq!(intent["unit"].as_str(), Some("plan"));
    let intent_shas: Vec<String> = intent["shas"]
        .as_array()
        .expect("shas must be a JSON array")
        .iter()
        .map(|v| {
            v.as_str()
                .expect("each sha must be a JSON string")
                .to_string()
        })
        .collect();
    assert_eq!(
        intent_shas.len(),
        1,
        "one commit intended; got {intent_shas:?}"
    );
    assert!(
        intent_shas[0].len() >= 7 && intent_shas[0].chars().all(|c| c.is_ascii_hexdigit()),
        "the intended sha must look like a real git object id; got {intent_shas:?}"
    );

    // The intent record must name the ORIGINAL commit identity the planner's own worktree
    // actually produced - read back directly via `PlanAmendDriver`'s own recording, since
    // comparing against the eventual landed sha for INEQUALITY would be unsound: a same-
    // parent, same-committer-second cherry-pick can legitimately mint a byte-identical git
    // object (content addressing), which is exactly what this fixture's single, unopposed
    // commit onto a fresh run branch produces in a fast test run.
    let committed_sha = driver
        .committed_sha
        .lock()
        .unwrap()
        .clone()
        .expect("the planner's own commit sha must have been recorded");
    assert_eq!(
        intent_shas[0], committed_sha,
        "the intent record must name the real, original sha the worktree committed"
    );

    let integrated: Value = serde_json::from_slice(&events[integrated_pos].data).unwrap();
    let landed_sha = integrated["commit"].as_str().unwrap().to_string();
    assert!(
        !landed_sha.is_empty() && landed_sha.chars().all(|c| c.is_ascii_hexdigit()),
        "the eventual integration must carry a real landed sha; got {landed_sha}"
    );
}

/// Criterion 4, gap 12 (round 7, new fold arm `RunCtx::read_plan_landed` / `record_plan_landed`,
/// operator ruling item (2): "decide landed (an equivalent commit is reachable from the run
/// branch by patch-id OR BY THE RECORDED LANDED SHA)" - the ruling names TWO distinct
/// confirmation mechanisms; gap 9 above proves the patch-id one for a resume with NO prior
/// confirmation record, this test proves the SECOND, the durable log record itself): simulated
/// as a crash strictly AFTER a prior call both landed the amendment for real AND recorded its
/// own `plan-landed:<unit>` confirmation, but BEFORE `UnitIntegrated` - by hand-seeding the log
/// with the EXACT `DecisionMade` shape `RunCtx::record_plan_landed` itself writes (the technique
/// gap 2's legacy-event test established), landing the amendment for real through the same
/// public `Worktree::cherry_pick_onto_run_branch` a crashed prior attempt would itself have
/// used, then adopting the SAME run (`rigger::run::ensure_started`, matching criteria) with a
/// fresh `run()`. Filler commits deliberately push the landed sha beyond `find_landed_by_
/// patch_id`'s own search window BEFORE the resumed `run()` starts, so a patch-id search alone
/// could no longer recover it - isolating the log record as the ONLY mechanism that can produce
/// the correct outcome, rather than merely a scenario where either mechanism happens to work.
/// Proves the resumed call trusts the log record directly - no NEW cherry-pick, no successful
/// patch-id search - and still reaches `Integrated` with the real landed sha and content a
/// downstream stage can read.
#[test]
fn plan_stage_resumed_with_a_pre_existing_plan_landed_record_recovers_without_any_new_git_mutation()
{
    let repo = init_repo();
    let repo_path = repo.path().to_str().unwrap().to_string();

    // A PRIOR window's planner committed its amendment onto the deterministic `rigger/u/plan`
    // branch, via its own throwaway worktree - the shape every sibling crash-resume test in this
    // file uses.
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
        seed_dir.path().join("specs").join("97-preconfirmed.md"),
        "amend\n",
    )
    .unwrap();
    assert!(std::process::Command::new("git")
        .arg("-C")
        .arg(seed_dir.path())
        .args(["add", "-A"])
        .status()
        .unwrap()
        .success());
    // A FIXED, deliberately old date - the same guard every sibling crash-resume test in this
    // file uses, so this cherry-pick cannot coincidentally reproduce a byte-identical object.
    assert!(std::process::Command::new("git")
        .arg("-C")
        .arg(seed_dir.path())
        .args(["commit", "-q", "-m", "amend"])
        .env("GIT_AUTHOR_DATE", "2000-01-01T00:00:00")
        .env("GIT_COMMITTER_DATE", "2000-01-01T00:00:00")
        .status()
        .unwrap()
        .success());
    let original_shas = seed.commits_since_base().unwrap();
    assert_eq!(original_shas.len(), 1);
    let original_sha = original_shas[0].clone();

    // The CRASHED PRIOR ATTEMPT's own successful git mutation: landed for real, through the
    // same public API `integrate_plan_commits` itself calls.
    let prior_landed = match seed.cherry_pick_onto_run_branch(&original_shas).unwrap() {
        CherryPickOutcome::Picked(landed) => landed,
        CherryPickOutcome::Conflict(detail) => {
            panic!("a clean specs/-only cherry-pick must not conflict: {detail}")
        }
    };
    assert_eq!(prior_landed.len(), 1);
    let landed_sha = prior_landed[0].clone();
    seed.remove().unwrap();

    // Push the landed commit beyond `find_landed_by_patch_id`'s own search window (a private
    // constant, 256, in `integrate_plan_commits`) with filler commits on the run branch - so
    // THIS test genuinely isolates the log-record mechanism: a patch-id search alone could not
    // recover this sha any more, only the durable `plan-landed` record can. Without this, the
    // scenario below would ALSO resolve correctly via patch-id search alone (gap 9's mechanism),
    // which would prove nothing distinct about the NEW fold arm this test exists to close.
    for i in 0..260 {
        assert!(std::process::Command::new("git")
            .arg("-C")
            .arg(&repo_path)
            .args([
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                &format!("filler {i}")
            ])
            .status()
            .unwrap()
            .success());
    }

    let head_after_prior_landing = run_git(&repo_path, &["rev-parse", "HEAD"]);

    // The CRASHED PRIOR ATTEMPT'S OWN CONFIRMATION WRITE: the EXACT `DecisionMade` shape
    // `RunCtx::record_plan_landed` itself produces, hand-seeded directly onto the store before
    // any fresh `run()` ever starts - proving the record's OWN shape is what a resumed call
    // actually consults, not merely that the private method which writes it also happens to
    // read it back correctly in-process (already proven by the implementer's own conductor.rs
    // unit tests).
    let store = Store::open(":memory:").unwrap();
    rigger::run::ensure_started(&store, &[]).unwrap();
    store
        .append(
            STREAM,
            ExpectedRevision::Any,
            &[Event::new(
                contextgraph::TYPE_DECISION_MADE,
                serde_json::to_vec(&json!({
                    "id": "plan-landed:plan",
                    "summary": "plan-stage commit landing confirmed for plan: 1/1 intended \
                                commit(s) landed",
                    "governs": [],
                    "unit": "plan",
                    "landed": [{"sha": original_sha, "landed_sha": landed_sha}],
                }))
                .unwrap(),
            )],
        )
        .unwrap();

    // A FRESH run() adopts the SAME producer branch and the SAME run (matching, empty
    // criteria). Its own worktree still carries only the ORIGINAL (pre-landing) commit
    // identity; the planner's fresh spawn commits NOTHING new, mirroring every sibling resume
    // test - a genuine resume never re-does work a crashed attempt already finished.
    let mut cfg = Config::default();
    // Spec 89 criterion 2 ruling item 2: nest the scratch/worktree default back inside
    // this fixture's own repo tempdir so this real, worktree-creating conductor::run()
    // never reaches the real ambient XDG_CACHE_HOME/HOME cache-home default.
    cfg.workflow.defaults.workdir = common::isolated_workdir(repo.path());
    cfg.agents.insert("planner".into(), agent("planner"));
    cfg.agents.insert("reader".into(), agent("reader"));
    cfg.workflow.stages.insert("plan".into(), plan_stage());
    cfg.workflow
        .stages
        .insert("critique".into(), downstream_reader_stage("reader"));

    let driver = PlanAmendDriver::new("planner").reading("reader", "specs/97-preconfirmed.md");
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
        "a resume with a pre-existing plan-landed record must still converge"
    );
    assert_eq!(
        rs.units["plan"].commit, landed_sha,
        "the recovered commit must be exactly the log's own recorded landed sha"
    );

    // NO NEW GIT MUTATION: the run branch is byte-for-byte unchanged from the state the prior
    // (crashed) attempt's real cherry-pick already left it in - the durable record alone
    // answers this call; no fresh cherry-pick and no patch-id search are needed to reach it.
    assert_eq!(
        run_git(&repo_path, &["rev-parse", "HEAD"]),
        head_after_prior_landing,
        "a resume driven purely by the log record must not mutate the run branch at all"
    );

    // The downstream stage - branched off the run branch only after "plan" integrates - already
    // sees the amendment, proving the recovered commit is the SAME real content, not merely a
    // recorded label.
    assert_eq!(
        driver.found.lock().unwrap().get("specs/97-preconfirmed.md"),
        Some(&"amend\n".to_string()),
        "a downstream stage must see the real, recovered amendment content"
    );
}

/// Criterion 4, gap 13 (round 8, `sdet-u88c4-r8-surface-enumeration`; fix for `arch-u88c4-r7-
/// classification-skip-is-single-shot-not-a-loop` reopened through a narrower trigger): the
/// leftover-`CHERRY_PICK_HEAD` classification in `Worktree::cherry_pick_onto_run_branch` used to
/// issue exactly ONE `--skip` before giving up. Git's own `--skip` only ever advances the
/// sequencer past the CURRENT paused commit, and the very next one can ALSO be empty - an
/// ordinary shape for a multi-commit plan amendment resumed after a crash - so a single attempt
/// left `CHERRY_PICK_HEAD` still set and hard-errored on a state that was actually still
/// resolvable. Round 8 replaces the single shot with a bounded `skips_left` loop.
///
/// WHY THIS, DISTINCT FROM THE IMPLEMENTER'S OWN TEST. `src/worktree.rs`'s own
/// `cherry_pick_onto_run_branch_self_heals_a_leftover_marker_ahead_of_two_chained_empty_commits`
/// proves the identical mechanics from INSIDE the crate's own private test module. This method's
/// PUBLIC signature never changed (probe 1 - a grep for added `pub fn` lines - found nothing new
/// since round 7), only its BODY did, so the fix is invisible to a bare surface scan; it earns a
/// periphery-level proof anyway because `cherry_pick_onto_run_branch` is itself a public API an
/// external consumer of this library calls directly - the same "public API, no conductor
/// involved" boundary gaps 5 and 10 above already established for this file's other `Worktree`
/// methods. Drives the exact same crash shape through nothing but the public `Worktree` /
/// `CherryPickOutcome` API, independently authored, with no conductor or `run()` in the loop at
/// all.
#[test]
fn cherry_pick_onto_run_branch_self_heals_a_leftover_marker_ahead_of_two_chained_empty_commits_at_the_periphery(
) {
    let repo = init_repo();
    let repo_path = repo.path().to_str().unwrap().to_string();
    let wt_dir = tempfile::tempdir().unwrap();
    let wt_path = wt_dir.path().to_str().unwrap().to_string();
    let wt = Worktree::create(&repo_path, &wt_path, "rigger/u/ext-plan-skip", "").unwrap();

    // Three commits in the producer's own worktree, each touching its OWN path - no real
    // conflicts among them.
    let mut shas = Vec::new();
    for name in ["98-a.md", "98-b.md", "98-c.md"] {
        std::fs::create_dir_all(wt_dir.path().join("specs")).unwrap();
        std::fs::write(
            wt_dir.path().join("specs").join(name),
            format!("amend {name}\n"),
        )
        .unwrap();
        assert!(std::process::Command::new("git")
            .arg("-C")
            .arg(&wt_path)
            .args(["add", "-A"])
            .status()
            .unwrap()
            .success());
        assert!(std::process::Command::new("git")
            .arg("-C")
            .arg(&wt_path)
            .args(["commit", "-q", "-m", &format!("amend {name}")])
            .status()
            .unwrap()
            .success());
        shas.push(run_git(&wt_path, &["rev-parse", "HEAD"]));
    }
    assert_eq!(shas.len(), 3);

    // Pre-land the FIRST and SECOND commits' content directly on the run branch, independent of
    // the interrupted sequence below - so replaying the full sequence pauses on the first (now
    // empty) commit, and a single skip lands on the second, which is ALSO empty: exactly the
    // "2+ chained empty commits ahead of the marker" shape.
    assert!(std::process::Command::new("git")
        .arg("-C")
        .arg(&repo_path)
        .args(["cherry-pick", &shas[0], &shas[1]])
        .status()
        .unwrap()
        .success());
    for name in ["98-a.md", "98-b.md"] {
        assert!(
            repo.path().join("specs").join(name).exists(),
            "precondition: {name}'s content is already present before the interrupted \
             sequence starts"
        );
    }

    // Simulate the crash: run the RAW multi-sha cherry-pick directly against the run branch
    // (bypassing the public API entirely, via a bare git subprocess), so it naturally pauses on
    // the first, now-empty commit - exactly the state a process death right after the pause
    // (before even one skip ran) leaves, never a synthetic one.
    let raw = std::process::Command::new("git")
        .arg("-C")
        .arg(&repo_path)
        .args(["cherry-pick", &shas[0], &shas[1], &shas[2]])
        .status()
        .unwrap();
    assert!(
        !raw.success(),
        "the raw sequence must pause on the empty first commit, not succeed outright"
    );
    assert!(
        repo.path().join(".git").join("CHERRY_PICK_HEAD").exists(),
        "precondition: a leftover cherry-pick sequencer marker is left in progress"
    );
    assert!(
        run_git(&repo_path, &["ls-files", "--unmerged"]).is_empty(),
        "precondition: the pause carries ZERO unmerged files - it is not a conflict"
    );

    // A FRESH call through the PUBLIC API, with the ORIGINAL (identity) shas exactly as a
    // resumed process recomputing `commits_since_base` would - must self-heal the leftover
    // marker THROUGH BOTH chained empty commits and complete, never hard-error after only one
    // skip.
    match wt.cherry_pick_onto_run_branch(&shas) {
        Ok(CherryPickOutcome::Picked(_)) => {}
        Ok(CherryPickOutcome::Conflict(detail)) => {
            panic!("a self-healed, non-conflicting sequence must not read as a conflict: {detail}")
        }
        Err(e) => panic!(
            "a leftover marker ahead of two chained empty commits must self-heal through the \
             public API, never hard-error: {:?}",
            e.0
        ),
    }
    assert!(
        !repo.path().join(".git").join("CHERRY_PICK_HEAD").exists(),
        "no cherry-pick is left in progress after the self-healed retry"
    );
    for name in ["98-a.md", "98-b.md", "98-c.md"] {
        assert!(
            repo.path().join("specs").join(name).exists(),
            "every commit's content must be present on the run branch after the self-healed \
             retry completes the interrupted sequence: missing {name}"
        );
    }
    wt.remove().unwrap();
}

/// A public [`EventStore`] wrapper that fails ONE specific append - a batch containing a
/// `DecisionMade` whose `id` field is EXACTLY `fail_decision_id` - and delegates every other
/// call straight to `inner`. Authored independently from `conductor.rs`'s own crate-private
/// `FailingStore` test double (spec 32: the periphery layer never reuses an implementer's
/// crate-internal test doubles, since an external consumer of this library could never reach
/// them) - the SAME technique, built entirely on the PUBLIC `EventStore` trait this crate
/// exports for exactly this purpose (`Deps::store: &'a dyn EventStore`).
///
/// Matches on the parsed `id` field, NOT a raw substring of the serialized bytes: the failure
/// this forces (`record_plan_intent`'s own store-append error) gets QUOTED, verbatim, inside
/// the very error message the conductor's own fallback path would embed in a LATER
/// `LessonLearned` summary if the regression this test exists to catch ever reappeared - a raw
/// substring match on `"plan-intent:"` would ALSO poison that later, unrelated append (since
/// the quoted error text itself contains the substring), silently producing zero `LessonLearned`
/// events for the WRONG reason (the double eating its own error message) and masking the exact
/// regression under test. The precise `id`-field match fails only the one real intent-record
/// append and never anything downstream that merely mentions it.
struct FailingExternalStore<'a> {
    inner: &'a dyn EventStore,
    fail_decision_id: &'static str,
}

impl EventStore for FailingExternalStore<'_> {
    fn append(
        &self,
        stream: &str,
        expected: ExpectedRevision,
        events: &[Event],
    ) -> Result<Appended, rigger::eventstore::Error> {
        let hits = events.iter().any(|e| {
            e.type_ == contextgraph::TYPE_DECISION_MADE
                && serde_json::from_slice::<Value>(&e.data)
                    .ok()
                    .and_then(|v| v.get("id").and_then(Value::as_str).map(str::to_string))
                    .as_deref()
                    == Some(self.fail_decision_id)
        });
        if hits {
            return Err(rigger::eventstore::Error::Backend(format!(
                "simulated store failure appending the DecisionMade id {:?}",
                self.fail_decision_id
            )));
        }
        self.inner.append(stream, expected, events)
    }
    fn read_stream(
        &self,
        stream: &str,
        from: Revision,
        dir: Direction,
    ) -> Result<Vec<Event>, rigger::eventstore::Error> {
        self.inner.read_stream(stream, from, dir)
    }
    fn read_all(
        &self,
        from: Position,
        dir: Direction,
        filter: &Filter,
    ) -> Result<Vec<Event>, rigger::eventstore::Error> {
        self.inner.read_all(from, dir, filter)
    }
    fn subscribe_all(
        &self,
        from: Position,
        filter: &Filter,
    ) -> Result<Subscription, rigger::eventstore::Error> {
        self.inner.subscribe_all(from, filter)
    }
    fn subscribe_stream(
        &self,
        stream: &str,
        from: Revision,
    ) -> Result<Subscription, rigger::eventstore::Error> {
        self.inner.subscribe_stream(stream, from)
    }
}

/// Criterion 4, gap 14 (round 8, new cross-module seam `is_plan_landing_failed` / `run_wave`):
/// `RunCtx::integrate_plan_commits` is now a thin wrapper over `integrate_plan_commits_inner`
/// that tags EVERY hard Err with a new, private `PLAN_LANDING_MARKER` sentinel (the fifth
/// alongside the pre-existing PARKED/BUDGET/DEGENERATE/MISMATCH markers), and `RunCtx::run_wave`
/// gained a matching arm that propagates the halt loudly but records NO per-unit lesson and
/// charges NO attempt - fixing `adv-u88c4-r7-plan-commit-errors-still-carry-no-infra-fault-
/// marker`, the same structural gap named at round 2 and round 4 and never closed by three
/// successive git-level-only fixes to the trigger while the missing marker itself went
/// unaddressed.
///
/// WHY THIS, DISTINCT FROM THE IMPLEMENTER'S OWN TESTS. `conductor.rs`'s own
/// `integrate_plan_commits_wraps_any_hard_error_with_the_plan_landing_marker` and `a_plan_
/// landing_infra_fault_halts_the_run_loudly_with_no_per_unit_lesson_or_attempt` force the
/// identical failure (a `record_plan_intent` store-append error) through a crate-PRIVATE
/// `FailingStore` double and a crate-PRIVATE `Stub` driver, both defined inside `conductor.rs`'s
/// own `#[cfg(test)] mod tests` - unreachable from outside the crate, exactly the class of gap
/// this file's own gap 1 names for `Stub`. This test forces the SAME failure from OUTSIDE the
/// crate, through nothing but the PUBLIC `EventStore` trait every real `Deps::store` caller
/// already implements against (`FailingExternalStore`, above - independently authored, built
/// only on that public trait) plus the public `run()` entry and an independently-authored
/// `AgentDriver` - proving the new cross-module wiring is reachable by, and behaves correctly
/// for, a genuine outside caller of this library, not merely from within the crate's own
/// private test module. The externally-observable contract is proven via nothing but public
/// reads of the real store: no `UnitFailed`, no `UnitEscalated`, no `LessonLearned`.
#[test]
fn a_plan_landing_store_failure_halts_the_run_loudly_with_no_per_unit_lesson_or_charged_attempt_at_the_periphery(
) {
    let repo = init_repo();
    let repo_path = repo.path().to_str().unwrap().to_string();

    let mut cfg = Config::default();
    // Spec 89 criterion 2 ruling item 2: nest the scratch/worktree default back inside
    // this fixture's own repo tempdir so this real, worktree-creating conductor::run()
    // never reaches the real ambient XDG_CACHE_HOME/HOME cache-home default.
    cfg.workflow.defaults.workdir = common::isolated_workdir(repo.path());
    cfg.agents.insert("planner".into(), agent("planner"));
    cfg.workflow.stages.insert("plan".into(), plan_stage());

    let real_store = Store::open(":memory:").unwrap();
    let store = FailingExternalStore {
        inner: &real_store,
        fail_decision_id: "plan-intent:plan",
    };
    let driver = PlanAmendDriver::new("planner").commit(&[("specs/98-halt.md", "amend\n")]);
    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &ExecRunner,
        repo: repo_path.clone(),
        grounder: None,
        graph: None,
        criteria: Vec::new(),
    };

    let err = match run(&cfg, &deps) {
        Ok(rs) => panic!(
            "a plan-landing store failure must halt the run, not succeed: {:?}",
            rs.units.get("plan").map(|u| u.status)
        ),
        Err(e) => e,
    };
    assert!(
        err.0.contains("\"plan\""),
        "the operator-facing halt must name the producer unit: {:?}",
        err.0
    );

    // The externally-observable contract, proven via nothing but public reads of the real
    // store: a plan-landing infra-fault halt must charge the unit NEITHER a lesson NOR an
    // attempt - the same treatment the pre-existing degenerate-reviewer and verdict-channel-
    // mismatch halts already get.
    let events = real_store
        .read_all(0, Direction::Forward, &Filter::default())
        .unwrap();
    assert!(
        !events.iter().any(|e| e.type_ == ledger::TYPE_UNIT_FAILED),
        "a plan-landing infra-fault halt must not charge the unit an attempt (no UnitFailed)"
    );
    assert!(
        !events
            .iter()
            .any(|e| e.type_ == ledger::TYPE_UNIT_ESCALATED),
        "a plan-landing infra-fault halt must not escalate the unit either"
    );
    assert!(
        !events
            .iter()
            .any(|e| e.type_ == contextgraph::TYPE_LESSON_LEARNED),
        "a plan-landing infra-fault halt must record NO per-unit lesson - it would \
         misattribute a conductor/git-plumbing fault to the producer unit"
    );
}

/// Criterion 4, gap 15 (round 9, `Worktree::sequencer_todo_remaining` body fix): a MISSING
/// `.git/sequencer/todo` file is the NORMAL shape for a leftover cherry-pick marker whose
/// remaining set is exactly ONE commit, not an anomaly - git's sequencer machinery is never
/// engaged by a plain single-sha `git cherry-pick`, so it never creates `.git/sequencer/` at
/// all. `cherry_pick_onto_run_branch`'s leftover-marker classification (gap 13) must still
/// resolve this the same way it resolves every other empty-commit pause - one `--skip` - never
/// hard-error on the missing file.
///
/// WHY THIS, DISTINCT FROM THE IMPLEMENTER'S OWN TESTS. `worktree.rs`'s own private test
/// module proves this from two angles - a literal one-commit-total unit, and a multi-commit
/// amendment whose `still_pending` has shrunk to one sha across two separate calls (the
/// conductor's real resume shape via `integrate_plan_commits_inner`'s `prior_landed` map) -
/// but both are the implementer's own inside-out authorship, invisible to an external
/// consumer of this library. At the `Worktree` public-API boundary the two angles are
/// indistinguishable: both reduce to a single-element `shas` slice against a leftover marker
/// with no `sequencer/todo` file, since `Worktree` never sees how many total commits an
/// amendment originally had - only the slice it is handed. This test drives that one call
/// shape through nothing but the PUBLIC `Worktree` API, independently constructed from outside
/// the crate, no conductor or `run()` involved - the same "public API, no conductor" boundary
/// gaps 5, 10 and 13 above already established for this file's other `Worktree` methods.
#[test]
fn cherry_pick_onto_run_branch_self_heals_a_leftover_marker_with_a_missing_sequencer_todo_file_at_the_periphery(
) {
    let repo = init_repo();
    let repo_path = repo.path().to_str().unwrap().to_string();
    let wt_dir = tempfile::tempdir().unwrap();
    let wt_path = wt_dir.path().to_str().unwrap().to_string();
    let wt = Worktree::create(&repo_path, &wt_path, "rigger/u/ext-plan-missing-todo", "").unwrap();

    // One commit, touching its own path.
    std::fs::create_dir_all(wt_dir.path().join("specs")).unwrap();
    std::fs::write(
        wt_dir.path().join("specs").join("99-solo.md"),
        "amend 99-solo.md\n",
    )
    .unwrap();
    assert!(std::process::Command::new("git")
        .arg("-C")
        .arg(&wt_path)
        .args(["add", "-A"])
        .status()
        .unwrap()
        .success());
    assert!(std::process::Command::new("git")
        .arg("-C")
        .arg(&wt_path)
        .args(["commit", "-q", "-m", "amend 99-solo.md"])
        .status()
        .unwrap()
        .success());
    let sha = run_git(&wt_path, &["rev-parse", "HEAD"]);
    let shas = vec![sha.clone()];

    // Pre-land the sole commit's content directly on the run branch, independent of the
    // interrupted attempt below, so replaying it becomes an EMPTY re-pick.
    assert!(std::process::Command::new("git")
        .arg("-C")
        .arg(&repo_path)
        .args(["cherry-pick", &sha])
        .status()
        .unwrap()
        .success());
    assert!(
        repo.path().join("specs").join("99-solo.md").exists(),
        "precondition: the sole commit's content is already present"
    );

    // Simulate the crash: run the RAW single-sha cherry-pick directly against the run branch
    // (bypassing the public API entirely, via a bare git subprocess) - the exact same
    // invocation shape a call with `shas.len() == 1` makes - so it naturally pauses empty with
    // NO sequencer directory ever created.
    let raw = std::process::Command::new("git")
        .arg("-C")
        .arg(&repo_path)
        .args(["cherry-pick", &sha])
        .status()
        .unwrap();
    assert!(
        !raw.success(),
        "the raw single-sha pick must pause on the empty commit, not succeed outright"
    );
    assert!(
        repo.path().join(".git").join("CHERRY_PICK_HEAD").exists(),
        "precondition: a leftover cherry-pick sequencer marker is left in progress"
    );
    assert!(
        run_git(&repo_path, &["ls-files", "--unmerged"]).is_empty(),
        "precondition: the pause carries ZERO unmerged files - it is not a conflict"
    );
    let todo_path = run_git(&repo_path, &["rev-parse", "--git-path", "sequencer/todo"]);
    let todo_path = std::path::Path::new(&todo_path);
    let todo_path = if todo_path.is_absolute() {
        todo_path.to_path_buf()
    } else {
        std::path::Path::new(&repo_path).join(todo_path)
    };
    assert!(
        !todo_path.exists(),
        "precondition: git never materializes sequencer/todo for a genuinely single-sha \
         cherry-pick - {} must be ABSENT",
        todo_path.display()
    );

    // A FRESH call through the PUBLIC API, with the ORIGINAL (identity) single-element shas
    // exactly as a resumed caller recomputing a shrunk-to-one pending set would - must
    // self-heal the leftover marker despite the missing sequencer/todo file, never hard-error.
    match wt.cherry_pick_onto_run_branch(&shas) {
        Ok(CherryPickOutcome::Picked(_)) => {}
        Ok(CherryPickOutcome::Conflict(detail)) => {
            panic!("a self-healed, non-conflicting pause must not read as a conflict: {detail}")
        }
        Err(e) => panic!(
            "a leftover marker with no sequencer/todo file must self-heal through the public \
             API, never hard-error: {:?}",
            e.0
        ),
    }
    assert!(
        !repo.path().join(".git").join("CHERRY_PICK_HEAD").exists(),
        "no cherry-pick is left in progress after the self-healed retry"
    );
    assert!(
        repo.path().join("specs").join("99-solo.md").exists(),
        "the sole commit's content must still be present on the run branch"
    );
    wt.remove().unwrap();
}
