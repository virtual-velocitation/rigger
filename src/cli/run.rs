use super::*;

use rigger::driver::claude_code;
use rigger::eventstore::TypeSelection;
use rigger::held::HeldDriver;
use rigger::keep_awake::{OsSleepHold, KEEP_AWAKE_ENV};
use rigger::review;
use rigger::sleep_hold::{SleepGuard, SleepHold, Unavailable};

/// Record that THIS run just attempted a dash start (spec 69, round-8 fix; see
/// [`DASH_ATTEMPT_FILE`]'s doc for the full rationale). Best-effort like every other dash
/// breadcrumb write in this module - a failed write only risks a later false suppression of an
/// anomaly this run's own dash-liveness probe should catch anyway via the timestamp fallback,
/// never a broken run. `run_id` empty (no run started yet in this project) writes nothing: there
/// is no run for the breadcrumb to name, so a later watch poll correctly falls back to the
/// existing unknown-run handling rather than matching an empty string against another empty one.
fn record_dash_attempt(run_id: &str) {
    if !run_id.is_empty() {
        let _ = std::fs::write(db_path(DASH_ATTEMPT_FILE), run_id);
    }
}

/// Environment opt-out for every run entry's always-on dashboard: when
/// [`DASH_DISABLE_ENV`] is set (to any value) `rigger run` does
/// NOT auto-start a run dashboard. Production leaves it unset (the dash is always-on, no opt-in
/// flag - spec 19b); a headless CI or the crate's own integration tests set it so a short-lived
/// run never spawns a real dashboard process.
const DASH_DISABLE_ENV: &str = "RIGGER_NO_DASH";

/// How often a held [`RunRegistration`] refreshes its heartbeat: a THIRD of the registry's idle
/// window ([`rigger::registry::DEFAULT_IDLE_MS`]), so a live in-process run is re-stamped at least
/// three times before a reader would consider its entry stale - comfortably inside the window even
/// under scheduling jitter or one very long in-process gate.
fn registry_heartbeat_interval() -> std::time::Duration {
    std::time::Duration::from_millis(rigger::registry::DEFAULT_IDLE_MS / 3)
}

/// A LIVE registration in the machine-global discovery registry (spec 50), held for as long as the
/// run it represents is in flight. Its initial entry is written synchronously by
/// [`register_run_instance`]; while this guard is held a background thread REFRESHES the heartbeat
/// every [`registry_heartbeat_interval`], and dropping it (the run scope ends, on success OR error)
/// signals that thread to stop and joins it. The periodic refresh is what keeps a run driven WHOLLY
/// in-process (`rigger run` - a single `conductor::run` call that can drive for hours) from
/// aging out of discovery MID-RUN: a one-shot register alone would let a reader prune the live
/// entry after the window.
struct RunRegistration {
    /// Dropping this disconnects the channel, so the heartbeat thread's `recv_timeout` returns at
    /// once (never waiting out a full interval) and the join below completes promptly.
    tx: Option<std::sync::mpsc::Sender<()>>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl RunRegistration {
    /// An inert guard that holds no thread - the homeless/unwritable degrade path, so the caller
    /// binds one uniform type whether or not a registry entry was actually written.
    fn inert() -> Self {
        RunRegistration {
            tx: None,
            handle: None,
        }
    }
}

impl Drop for RunRegistration {
    fn drop(&mut self) {
        // Disconnect first so the sleeping heartbeat thread wakes immediately, THEN reap it. Both
        // are best-effort: a poisoned/panicked thread never blocks the run's teardown.
        drop(self.tx.take());
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// Register THIS invocation's instance in the machine-global discovery registry (spec 50, criterion
/// 2): the project `identity`, its root, the credential-free store identity, and a fresh heartbeat, keyed so a
/// re-registration refreshes one entry in place. This is the SINGLE registration writer - the
/// run entry point (the in-process `rigger run` driver) requests through it, so the
/// machine-global registry sees every invocation that starts or advances a run. That invocation
/// DRIVES the run while it lives, so the entry and each heartbeat re-stamp are written as
/// [`rigger::registry::Writer::Driver`]: the
/// driver-liveness stamp, which a courier's discovery refresh ([`refresh_registry_entry`]) never
/// is.
///
/// Returns a [`RunRegistration`] the CALLER MUST HOLD for the life of the run (`let _reg = ...;`):
/// the initial entry is written here, synchronously, and the returned guard's background thread
/// keeps its heartbeat fresh until the guard drops. BEST-EFFORT and warn-only throughout: the
/// registry is discovery metadata whose loss is harmless (live instances repopulate it), so a
/// homeless environment or a write error degrades to an inert guard and NEVER fails the run.
#[must_use = "hold the RunRegistration for the life of the run so its heartbeat stays fresh"]
fn register_run_instance(
    repo: &str,
    identity: &str,
    selection: &StoreSelection,
) -> RunRegistration {
    let Some(dir) = rigger::registry::default_dir() else {
        return RunRegistration::inert(); // homeless environment: degrade to no registration
    };
    let root = if repo.is_empty() {
        cwd()
    } else {
        PathBuf::from(repo)
    };
    let inst = rigger::registry::Instance {
        project: identity.to_string(),
        root: root.to_string_lossy().into_owned(),
        store: registry_store_identity(selection, &root),
        heartbeat_ms: rigger::registry::now_ms(),
        writer: rigger::registry::Writer::Driver,
    };
    // The initial, synchronous registration. On failure, degrade to an inert guard rather than
    // spawn a heartbeat thread that could only fail the same way.
    if let Err(e) = rigger::registry::write(&dir, &inst) {
        rigger::whole_line::say(format_args!(
            "rigger: instance registry write skipped ({e}); discovery is unaffected"
        ));
        return RunRegistration::inert();
    }
    // The heartbeat thread re-stamps `inst` every interval until the guard drops
    // ([`tick_until_stopped`]): the guard's drop ends the wait at once, so teardown never waits
    // out a full interval.
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let interval = registry_heartbeat_interval();
    let handle = std::thread::spawn(move || {
        tick_until_stopped(&rx, interval, || {
            let refreshed = rigger::registry::Instance {
                heartbeat_ms: rigger::registry::now_ms(),
                ..inst.clone()
            };
            let _ = rigger::registry::write(&dir, &refreshed); // best-effort refresh
        })
    });
    RunRegistration {
        tx: Some(tx),
        handle: Some(handle),
    }
}

/// Enforce the run's definition pin (spec 13, unit 1) at the CLI boundary, BEFORE the
/// conductor drives: adopt-or-mint the run for `criteria` with `definition` pinned, and act on
/// the outcome. A fresh or unchanged run continues silently; `--rebase-definition` on a drifted
/// run records the supersession and continues with a notice; a drifted run WITHOUT the flag
/// returns a loud error, so `rigger run` HALTS naming the drift instead of driving a
/// campaign whose replay semantics silently changed. The conductor's own (unpinned)
/// `ensure_started` then simply ADOPTS the run this ensured.
///
/// `base` is the resolved run-branch base to persist on a freshly-minted RunStarted (spec 38,
/// criterion 3), so `rigger status`/`rigger dash` later read the run's actual base from the
/// log rather than re-resolving without the run's `--base` flag. On an ADOPTED (resumed) run
/// it is ignored - the base its original start stamped stands.
///
/// `base_tip` is the run branch's tip commit sha this run anchored at, persisted on a
/// freshly-minted RunStarted alongside `base` so the check-in stage's gates can later diff
/// the whole spec's work against it via `$RIGGER_RUN_BASE`, never a `git merge-base` with
/// the run branch. Mint-only, exactly like `base`.
///
/// `spec_path` is the spec file this run was launched with (spec 82, criterion 1), persisted
/// on a freshly-minted RunStarted the same mint-only way as `base`, so the ready-to-release
/// handoff can later derive this run's per-run-unique PR head name.
fn enforce_definition_pin(
    store: &dyn EventStore,
    criteria: &[String],
    definition: &str,
    rebase: bool,
    base: &str,
    base_tip: &str,
    spec_path: &str,
) -> Res {
    match runscope_store::ensure_started_pinned(
        store, criteria, definition, rebase, base, base_tip, spec_path,
    )? {
        runscope::RunStart::Ready(_) => Ok(()),
        runscope::RunStart::Rebased {
            run,
            pinned,
            current,
        } => {
            rigger::whole_line::say(format_args!(
                "rigger: --rebase-definition: recorded the definition supersession \
                 ({pinned} -> {current}) on run {run}; continuing on the new definition."
            ));
            Ok(())
        }
        runscope::RunStart::Drifted {
            run,
            pinned,
            current,
        } => Err(format!(
            "definition drift - the on-disk workflow/agent definition (hash {current}) differs \
             from the hash run {run} pinned at start ({pinned}). A live run pins its definition so \
             replay semantics cannot silently change mid-campaign. Re-run with --rebase-definition \
             to record the supersession ({pinned} -> {current}) and continue, or restore the \
             definition to match the pin."
        )
        .into()),
    }
}

/// EXACTLY ONE ROOT (spec 89, criterion 4): refuses BEFORE any scratch reclaim or scratch write
/// when the store the invoking `command` is about to open, the repository `git` resolved for the
/// same `cwd`, and the scratch root it is about to use disagree on their owning root - a
/// three-way check, not two. `rigger critique`
/// (which writes a critique's transcript and liveness marker into it and removes them, spec 112)
/// and `rigger run` (whose teardown reclaims it) call it; `command` names the invoking command in
/// the refusal, as [`resolve_main_worktree_or_refuse`] takes it.
///
/// LEG ONE (`cwd` vs `repo`): `RIGGER_DIR` is opened cwd-relative (never walked up), while
/// `repo` can walk PAST a `.git`-less `cwd` to an ENCLOSING repository - the two diverge
/// exactly when `cwd` has no `.git` of its own, e.g. a test fixture nested under a scratch
/// root (u87c3, 2026-09-11): the fixture's own store held none of the real run's events,
/// `git` resolved the REAL enclosing repository, so a reclaim keyed on the fixture's events
/// would remove every live worktree of the run actually using that scratch root.
///
/// LEG TWO (`scratch_root` vs `repo`, round 2 adjudication
/// `adv-u89c4-r2-one-root-check-is-two-of-three-scratch-root-never-compared`): leg one alone
/// still lets a scratch root aimed at an unrelated real repository through unrefused.
/// `RIGGER_TMPDIR` (read unconditionally by `worktree::scratch_root_from_env`, ahead of any
/// repo-derived default) can point `scratch_root` anywhere; run from the real repository root
/// (so leg one passes) with `RIGGER_TMPDIR` aimed at some OTHER real project's own scratch
/// tree, this command would act on THAT project's real worktrees using this run's
/// events - the same hazard leg one guards against, reached from the opposite direction.
/// Resolved with the SAME "which repository does this path belong to" primitive leg one
/// already trusts (`git_repo_at`, not raw path containment): a scratch root with NO
/// enclosing git repository of its own - the common case for an arbitrary external tmp
/// directory, e.g. `tests/cli.rs`'s
/// `the_liveness_marker_path_follows_a_non_default_scratch_root` - is vacuously safe (there
/// is no OTHER repository's worktrees to endanger, and the default `<repo>/.rigger/tmp`
/// naturally resolves back to `repo` itself); only a scratch root whose OWN git toplevel
/// resolves to a DIFFERENT repository than the one leg one just verified trips the refusal.
///
/// A repo-less `cwd` (`repo` empty) has nothing to cross-check, matching every other
/// repo-gated branch in `run_cli`.
pub(crate) fn refuse_unless_one_root(
    cwd: &Path,
    repo: &str,
    scratch_root: Option<&str>,
    command: &str,
) -> Result<(), String> {
    if repo.is_empty() {
        return Ok(());
    }
    let cwd_canon = std::fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    let repo_canon = std::fs::canonicalize(Path::new(repo)).unwrap_or_else(|_| PathBuf::from(repo));
    if cwd_canon != repo_canon {
        return Err(format!(
            "{command}: refusing - the store this command would open and the repository git \
             resolved for this directory disagree on their root: git toplevel (and the scratch \
             root this command uses, {scratch}) is {repo}, but the store under {RIGGER_DIR} \
             would be opened relative to the current directory {cwd} instead - a DIFFERENT root. \
             This shape arises when the current directory has no `.git` of its own (e.g. a test \
             fixture nested under a scratch root): `git rev-parse` then walks UP past it to an \
             ENCLOSING repository while the store stays right here, so this command would act on \
             that enclosing repository's real worktrees and scratch using THIS directory's own \
             (unrelated) events. Re-run from the repository root.",
            scratch = scratch_root.unwrap_or("(none)"),
            cwd = cwd_canon.display(),
        ));
    }
    if let Some(scratch) = scratch_root.map(str::trim).filter(|s| !s.is_empty()) {
        let scratch_repo = git_repo_at(Path::new(scratch))?;
        if !scratch_repo.is_empty() {
            let scratch_repo_canon = std::fs::canonicalize(Path::new(&scratch_repo))
                .unwrap_or_else(|_| PathBuf::from(&scratch_repo));
            if scratch_repo_canon != repo_canon {
                return Err(format!(
                    "{command}: refusing - the scratch root this command would use belongs to \
                     a DIFFERENT repository than the one this command resolved: git toplevel \
                     (and the store under {RIGGER_DIR}, opened relative to the current directory \
                     {cwd}) is {repo}, but the scratch root {scratch} resolves to the \
                     repository {scratch_repo} instead - a DIFFERENT root. This shape arises \
                     when `RIGGER_TMPDIR` (or `defaults.workdir`) is pointed at another \
                     project's own scratch tree: this command would then act on THAT \
                     project's real worktrees and scratch using this project's events. Point the \
                     scratch root back under {repo}, or re-run from the repository the scratch \
                     root belongs to.",
                    cwd = cwd_canon.display(),
                    scratch_repo = scratch_repo_canon.display(),
                ));
            }
        }
    }
    Ok(())
}

/// The project scratch root `rigger run` and `rigger critique` work under: `None` in a project
/// with no git repository, where nothing keys a scratch root and no liveness marker or
/// transcript is written; else the root `scratch_root_from_env` resolves for the repository and
/// `defaults.workdir`. The one home of that rule, so the roots a hosted run writes under and
/// reclaims, a critique writes under, and [`refuse_unless_one_root`] checks, can never
/// disagree.
pub(crate) fn project_scratch_root(repo: &str, cfg: &config::Config) -> Option<String> {
    (!repo.is_empty())
        .then(|| rigger::worktree::scratch_root_from_env(repo, &cfg.workflow.defaults.workdir))
}

/// This project's progress store (`.rigger/progress.db`), the separate log a headless host
/// records a spawn's launch and live activity to, never the run stream.
pub(crate) fn open_progress_backend() -> Result<Store, Box<dyn std::error::Error>> {
    Ok(Store::open(&db_path("progress.db"))?)
}

/// This binary's own path, so a child rigger process is always the same build as its parent (no
/// PATH ambiguity); `rigger` on `$PATH` when the path cannot be read.
pub(crate) fn running_executable() -> String {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "rigger".to_string())
}

/// Load the config a RUN will drive, refusing to start when a gating persona guarantees an
/// integration-gate stall (spec 18, unit 2). This is the single load seam the run entry
/// (`run_cli`) and `rigger gate` share, so the run-start refusal cannot be present
/// at one entry and silently missing at another.
///
/// The integration gate reads a gating agent's RESULT channel for a `{"verdict":...}` line and
/// never reads emitted events (a deliberate load-bearing decision); a gating persona (a review
/// adjudicator on any tier, or a plan-critique adjudicator) that records its verdict ONLY via
/// `rigger_emit` is therefore a guaranteed stall - the gate finds no verdict, folds it as a
/// non-approval, and the unit remediates until it escalates. Rather than begin that doomed run,
/// `rigger run` refuses up front with the SAME deterministic fix message
/// `rigger validate` gives. The check itself has ONE authority - `config::lint_gating_verdict_lines`
/// (spec 18, unit 1) - reused here, never re-derived; this seam only wires it onto the run path.
pub(crate) fn load_run_config(dir: &str) -> Result<config::Config, Box<dyn std::error::Error>> {
    let cfg = config_store::load(dir)?;
    config::lint_gating_verdict_lines(&cfg)?;
    Ok(cfg)
}

/// The refusal a run started outside a rigger project gives: `cwd` holds no workflow file, so
/// there is nothing to drive.
#[derive(Debug, thiserror::Error)]
#[error(
    "{command}: not a rigger project - {cwd} has no {RIGGER_DIR}/{WORKFLOW_FILE}; run \
     `rigger setup` in the project root first, or run from the directory that holds it"
)]
struct NotAProject {
    command: &'static str,
    cwd: String,
}

/// Refuse, creating nothing, unless `cwd` holds a rigger project's workflow file.
fn refuse_outside_a_project(cwd: &Path, command: &'static str) -> Res {
    if cwd.join(RIGGER_DIR).join(WORKFLOW_FILE).is_file() {
        return Ok(());
    }
    Err(Box::new(NotAProject {
        command,
        cwd: cwd.display().to_string(),
    }))
}

pub(crate) fn cmd_run(args: &[String]) -> Res {
    let parsed = parse_run_args(args)?;
    run_cli(&parsed, &*sleep_hold())
}

/// `rigger resume-unit <unit> [--attempts N]` (default `N`: 1) - spec 88, criterion 3
/// (ESCALATION RESUMES). An escalated unit is otherwise final: this is the one operator
/// lever that gives it another chance without replanning the whole spec. Appends
/// [`ledger::TYPE_UNIT_RESUMED`] (`{unit, attempts_granted, by: "operator"}`) to the
/// CURRENT run's stream; the ledger folds it back to a mid-remediation `Failed` unit
/// with a widened per-unit bound ([`ledger::Unit::resume_bound`]), so the next `rigger
/// run` re-parks the implementer on the unit's SAME durable branch (resume-continuity
/// already treats `Failed` this way - no second resume path is introduced) and
/// `rigger status` names the grant until it is spent or superseded by a fresh resume.
///
/// Refuses loudly, appending nothing, when: the unit is unknown to the current run; its
/// status is not `Escalated` (only a unit that genuinely gave up may be resumed - a
/// mid-remediation or already-landed unit has nothing to resume FROM); or its recorded
/// durable branch no longer exists in the repo (deleted or reclaimed out of band) -
/// named alongside a `git reflog` hint, per the spec's own constraints walk, since a
/// resume with no branch to re-park the implementer on would strand the implementer on
/// a fresh, empty checkout instead of the unit's actual prior work.
pub(crate) fn cmd_resume_unit(args: &[String]) -> Res {
    let mut unit_id: Option<&str> = None;
    let mut attempts_granted: u32 = 1;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--attempts" => {
                let raw = it.next().ok_or(
                    "resume-unit: --attempts expects a number: \
                     rigger resume-unit <unit> [--attempts N]",
                )?;
                attempts_granted =
                    raw.parse::<u32>().ok().filter(|n| *n >= 1).ok_or_else(|| {
                        format!("resume-unit: --attempts value {raw:?} must be a positive integer")
                    })?;
            }
            other if unit_id.is_none() && !other.starts_with("--") => {
                unit_id = Some(other);
            }
            other => {
                return Err(format!(
                    "resume-unit: unknown argument {other:?} \
                     (usage: rigger resume-unit <unit> [--attempts N])"
                )
                .into());
            }
        }
    }
    let unit_id = unit_id
        .ok_or("resume-unit: expected a unit id: rigger resume-unit <unit> [--attempts N]")?;

    on_the_run(|loc, store| {
        let (run_events, run_id) = runscope::read::read_current_run(store, conductor::STREAM)?;
        let rs = ledger::project(&run_events)?;

        let unit = rs
            .units
            .get(unit_id)
            .ok_or_else(|| format!("resume-unit: no unit {unit_id:?} in the current run"))?;
        if unit.status != ledger::Status::Escalated {
            return Err(format!(
                "resume-unit: unit {unit_id:?} is not escalated (status: {:?}) - only a unit \
                 that has genuinely given up (\"escalated (awaiting a human)\") can be resumed",
                unit.status.as_str()
            )
            .into());
        }
        let branch = unit.branch.clone();
        if !branch.is_empty() && !rigger::worktree::branch_exists(&loc.repo_root(), &branch) {
            return Err(format!(
                "resume-unit: unit {unit_id:?}'s durable branch {branch:?} is gone (deleted or \
                 reclaimed) - refusing to resume with nothing to re-park the implementer on. \
                 Check `git reflog` for its last commit and recreate the branch at that sha \
                 before retrying."
            )
            .into());
        }

        let mut ev = Event::new(
            ledger::TYPE_UNIT_RESUMED,
            serde_json::to_vec(
                &serde_json::json!({"unit": unit_id, "attempts_granted": attempts_granted, "by": "operator"}),
            )?,
        );
        if !run_id.is_empty() {
            ev = ev.with_meta(runscope::META_RUN_ID, &run_id);
        }
        // The folding store puts the resume on the log first and opens `graph.db` only after (spec
        // 101), so a graph that cannot be had costs the operator's recovery verb only the fold,
        // which it says.
        store.append(
            conductor::STREAM,
            ExpectedRevision::Any,
            std::slice::from_ref(&ev),
        )?;

        println!(
            "resumed unit {unit_id:?}: {attempts_granted} attempt(s) granted (by operator) - the \
             next `rigger run` re-parks its implementer on {branch:?}"
        );
        Ok(())
    })
}

/// Reclaim the run's run-level shared scratch at a terminal run state (spec 34, criterion 3),
/// once `rigger run`'s conductor loop returns done:
/// `agent-scratch` (probe repos + verify builds a worker parks there), `agent-live` (per-spawn
/// liveness markers), `agent-stream` (a headless host's raw transcripts), and the SHARED build
/// cache - `cargo-target` and `target` directly under the scratch root, the driver's
/// `CARGO_TARGET_DIR`, the unbounded multi-GB leak. These are run-level areas a live spawn
/// may still be building into while the run drives; once the run is terminal and no spawn is
/// live they are pure residue, so this teardown - and only this teardown - reclaims them. The
/// two build-cache names mirror exactly what `rigger validate`'s residue report flags as a
/// shared cache (`scan_residue`), so validate-reports and run-reclaims stay in lockstep.
///
/// The three agent areas are reclaimed entry by entry ([`remove_agent_area_entries`]), each area
/// then removed once empty: a critique run's entries (`critique-<hash>`) share those areas and
/// are left to that critique's own next call, so a critique still running as the run ends keeps
/// its scratch and its processes. Each reclaimed entry and the bare `target` are
/// reaped-then-removed (spec 23): any process still rooted in one is reaped BEFORE the dir is
/// removed so nothing outlives a dir holding a now-deleted cwd. Scoped to the EXACT
/// dir removed under the resolved scratch `root` (`RIGGER_TMPDIR` > `defaults.workdir` > repo
/// default), never a hardcoded `.rigger/tmp`, so a relocated scratch root stays safe and only
/// rigger's own scratch is ever reaped. Every half is best-effort - a missing area is a graceful
/// no-op (platform-tolerant, idempotent), never an error that fails the run. Per-unit worktrees
/// and their `cargo-target-<slug>` caches are NOT this function's concern - those are reclaimed
/// by `Worktree::remove` at the end of their unit's stage, never while a later stage of the same
/// unit still needs them.
///
/// The bare `cargo-target`/`target` removals are the first code to delete those names directly
/// under the root (`Worktree::remove` and `rigger validate` only ever touch the per-unit
/// `cargo-target-<slug>` siblings), so this is safe ONLY because `root` is the RESOLVED rigger
/// scratch root (`RIGGER_TMPDIR` > `defaults.workdir` > the repo default `.rigger/tmp`), a
/// directory rigger owns end to end - never the repo root. An operator who misconfigures the
/// scratch root TO the repo root would have rigger park every worker's scratch there too, so the
/// misconfiguration is self-evident long before this teardown; this function does not re-derive
/// or second-guess `root`, it trusts the one resolution authority all scratch paths share.
///
/// The `cargo-target` reap (spec 77 criterion 5, BOUNDED SHARED CACHE) routes through
/// [`reclaim_shared_build_cache`] - the SAME guarded reclaim `rigger reset --build-cache`
/// calls - rather than a second, unguarded `remove_dir_all` over the identical resource:
/// spec 77's Global Constraint 3 (fail-safe deletion only, skip anything a liveness guard
/// claims) is spec-wide, and this run-teardown fires automatically and unconditionally
/// whenever a run reaches a terminal state with no live worker, which says nothing about
/// whether a rigger-launched build (an agent's own manual `cargo test` against this exact
/// shared cache, per the driver's own scratch-policy convention) is still mid-flight.
/// Contention here is a SILENT best-effort skip (never a loud refusal) - matching every
/// other area this function reaps: a still-building cache is simply left for a LATER
/// teardown, exactly like a missing area is a graceful no-op. The bare `target` dir (a
/// defensive sweep for a build that ran with no `CARGO_TARGET_DIR` override at all,
/// landing in the ambient cwd-relative default) is NOT this guard's concern - the naming
/// convention and guard protocol are specifically for the shared `cargo-target` name every
/// rigger-directed build actually targets; `target` stays a bare reap as before.
fn reclaim_run_scratch(root: &str) {
    let base = std::path::Path::new(root);
    remove_agent_area_entries(base, |name| !review::is_critique_run(name));
    for area in AGENT_AREAS {
        let _ = std::fs::remove_dir(base.join(area));
    }
    let _ = reclaim_shared_build_cache(&base.join(rigger::worktree::SHARED_BUILD_CACHE_NAME), base);
    reap_then_remove_dir(&base.join("target"), base);
}

/// The one teardown every ending of a `rigger run` passes through: dropped as `run_cli` returns,
/// done, halted, escalated or failed, it reclaims the run-level scratch ([`reclaim_run_scratch`])
/// unless the run ended with a unit awaiting a human, whose review resumes it. No spawn of the run
/// is live by then: the run reaps every earlier launch at its start and the driver reaps each
/// launch it makes before the conductor returns. A log that cannot be read reclaims nothing, and a
/// project with no scratch root has none to reclaim.
struct RunTeardown<'a> {
    scratch: Option<&'a str>,
    store: &'a dyn EventStore,
}

impl Drop for RunTeardown<'_> {
    fn drop(&mut self) {
        if let Some(root) = self.scratch.filter(|_| awaits_no_human(self.store)) {
            reclaim_run_scratch(root);
        }
    }
}

/// Whether the current run's log reads, and no unit of it awaits a human's review.
fn awaits_no_human(store: &dyn EventStore) -> bool {
    runscope::read::read_current_run(store, conductor::STREAM)
        .ok()
        .and_then(|(events, _)| ledger::project(&events).ok())
        .is_some_and(|rs| rs.manual_review.is_empty())
}

/// The agent areas under a scratch root, each holding one entry per run: the scratch a worker
/// parks probe repos and verify builds in, per-spawn liveness markers, and a headless host's raw
/// transcripts.
const AGENT_AREAS: [&str; 3] = [
    rigger::driver::replay::SPAWN_SCRATCH_SUBDIR,
    rigger::liveness::MARKER_SUBDIR,
    claude_code::AGENT_STREAM_SUBDIR,
];

/// Remove every entry of the [`AGENT_AREAS`] under `scratch_root` whose name `takes` accepts: a
/// directory through the reap-then-remove every scratch removal shares, scoped to
/// `scratch_root`, anything else as a file. Best-effort: an area or entry that cannot be read or
/// removed stays for a later call.
pub(crate) fn remove_agent_area_entries(scratch_root: &Path, takes: impl Fn(&str) -> bool) {
    for area in AGENT_AREAS {
        let Ok(entries) = std::fs::read_dir(scratch_root.join(area)) else {
            continue;
        };
        for entry in entries.flatten() {
            if !takes(&entry.file_name().to_string_lossy()) {
                continue;
            }
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                reap_then_remove_dir(&entry.path(), scratch_root);
            } else {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

/// Warn on stderr when the run branch was anchored somewhere OTHER than the base the
/// operator asked for, so a divergence is never silent (the old behavior silently
/// no-op'd an unresolvable base and silently ignored `--base` on every run after the
/// first). `cmd` names the invoking command (e.g. `"rigger run"`) so the advisory reads true
/// for whichever command anchored. Any primary output still goes to stdout untouched; these are
/// stderr advisories, not errors - isolation is intact in every case, only the anchor differs.
///
/// A [`RunBranchSetup::CreatedFromHead`] here is a HEAD fallback with a VALID HEAD: the
/// configured base did not resolve, but the current HEAD is a real commit, so the run branch
/// descends from a reachable base (the operator's own branch) that a PR still applies to - it
/// proceeds and is merely advised here. The genuinely baseless case (an unborn HEAD, nothing
/// to branch from) never reaches this function: it is refused loudly upstream by the spec 38
/// loop-readiness gate ([`refuse_when_base_unreachable`]).
fn warn_on_run_branch_divergence(
    cmd: &str,
    setup: RunBranchSetup,
    base: &str,
    base_explicit: bool,
) {
    match setup {
        RunBranchSetup::CreatedFromHead => rigger::whole_line::say(format_args!(
            "{cmd}: base {base:?} did not resolve, so the run branch {RUN_BRANCH:?} was anchored \
             on the current HEAD instead (unit isolation is intact, but not anchored on {base:?}). \
             Fetch the base or pass an existing ref as --base to anchor there."
        )),
        // The run branch already exists and was reused. Reusing the default base every
        // run is the expected steady state and stays silent; only an EXPLICIT --base
        // that got ignored (because re-anchoring would orphan integrated work) is worth a
        // word, so the operator is not left thinking their re-anchor took effect.
        RunBranchSetup::Reused if base_explicit => rigger::whole_line::say(format_args!(
            "{cmd}: the run branch {RUN_BRANCH:?} already exists and was reused (its \
             integrated work is preserved); --base {base:?} was NOT applied. Re-anchoring an existing \
             run branch would discard integrated units; to anchor a run on {base:?}, start it on a \
             repo without {RUN_BRANCH:?} (or delete that branch first)."
        )),
        RunBranchSetup::Reused | RunBranchSetup::CreatedFromBase => {}
    }
}

/// Anchor the run branch off `base` before a run entry drives the conductor, so every
/// unit worktree branches off [`RUN_BRANCH`] and integration merges never land on the
/// operator's own branch (spec 18, criterion 6 threads `--base` here). Creates and checks
/// out [`RUN_BRANCH`] off `base` (or off HEAD when `base` does not resolve), reusing an
/// existing run branch untouched. `cmd` labels the
/// command in the error and the divergence advisory. A failure aborts the run with an
/// actionable error rather than driving the conductor on the wrong branch - isolation is a
/// precondition, not best-effort. Callers guard this on a real repo (a repo-less invocation
/// skips run-branch setup entirely). The missing-files base check (criterion 7) runs BEFORE
/// this, gated on [`Worktree::planned_run_branch_setup`], so a wrong-base run is refused
/// without ever creating a branch to anchor here.
fn anchor_run_branch(repo: &str, cmd: &str, base: &str, base_explicit: bool) -> Res {
    let setup = Worktree::ensure_run_branch(repo, RUN_BRANCH, base).map_err(|e| {
        format!(
            "{cmd}: could not prepare the run branch {RUN_BRANCH:?} (base {base:?}): {e}. \
             The run did not start; resolve the git state (e.g. commit or stash a dirty tree) and retry."
        )
    })?;
    warn_on_run_branch_divergence(cmd, setup, base, base_explicit);
    Ok(())
}

/// One beat of the supervisor (spec 105 LIVENESS): a [`SupervisorBeat`] on the progress stream of
/// the run current at this beat, re-read here so the beat follows whichever run is current. Only
/// the current run's `RunStarted` is read, from its boundary, since a beat wants one id every
/// interval for as long as the run lives. A run log that cannot be read, or that names no run, beats nothing, and a write the
/// progress store refuses is ignored: the next tick is the retry either way.
///
/// [`SupervisorBeat`]: rigger::progress::SupervisorBeat
fn beat(run_store: &dyn EventStore, progress: &dyn EventStore) {
    if let Ok(Some(run)) = runscope::read::read_current_run_id(run_store, conductor::STREAM) {
        let _ = rigger::progress_store::append_stamped(
            progress,
            &run,
            &rigger::progress::SupervisorBeat::default(),
            "the supervisor beat",
        );
    }
}

/// Hand back what `drive` returns, with the supervisor beating beside it ([`ticking`] a [`beat`]
/// each `interval`): a beat as `drive` starts, so an adopting supervisor is on record at once, and
/// none once `drive` returns. A failed beat freezes silent time and no host state hangs on it, but
/// a beat still running as `drive` returns is waited out, and one that panics is re-raised then.
fn beating<T>(
    run_store: &dyn EventStore,
    progress: &dyn EventStore,
    interval: std::time::Duration,
    drive: impl FnOnce() -> T,
) -> T {
    ticking(interval, || beat(run_store, progress), drive)
}

/// Hand back what `drive` returns, with `tick` running beside it on a thread of its own: once as
/// `drive` starts, then once each `interval` after the last tick ends. The thread sleeps on a
/// channel whose one sender lives only as long as `drive`, so `drive`'s return wakes it at once and
/// it ticks no further; the scope joins it, so a tick still running when `drive` returns is waited
/// out, never cut short.
fn ticking<T>(
    interval: std::time::Duration,
    tick: impl Fn() + Send,
    drive: impl FnOnce() -> T,
) -> T {
    use std::sync::mpsc::channel;

    std::thread::scope(|scope| {
        let (_driving, driven) = channel::<()>();
        scope.spawn(move || {
            tick();
            tick_until_stopped(&driven, interval, &tick);
        });
        drive()
    })
}

/// Call `tick` once each `interval` until `stop` receives or its sender drops. The wait is a
/// `recv_timeout` on `stop`, so the sender's drop ends it at once, never after a full interval.
fn tick_until_stopped(
    stop: &std::sync::mpsc::Receiver<()>,
    interval: std::time::Duration,
    tick: impl Fn(),
) {
    while let Err(std::sync::mpsc::RecvTimeoutError::Timeout) = stop.recv_timeout(interval) {
        tick();
    }
}

/// The keep-awake adapter [`KEEP_AWAKE_ENV`] selects: unset, the host's [`OsSleepHold`];
/// `record:<path>`, a [`RecordingSleepHold`] appending to `<path>`; `unavailable:<reason>`, an
/// adapter answering `<reason>`; any other value, an adapter answering that the value names no
/// adapter, so a mistyped knob never takes the host's hold unasked.
fn sleep_hold() -> Box<dyn SleepHold> {
    let Some(knob) = std::env::var_os(KEEP_AWAKE_ENV) else {
        return Box::new(OsSleepHold);
    };
    let knob = knob.to_string_lossy();
    if let Some(path) = knob.strip_prefix("record:") {
        return Box::new(RecordingSleepHold(PathBuf::from(path)));
    }
    Box::new(UnavailableSleepHold(
        match knob.strip_prefix("unavailable:") {
            Some(why) => why.to_string(),
            None => format!("{KEEP_AWAKE_ENV}={knob} names no keep-awake adapter"),
        },
    ))
}

/// The keep-awake fake [`KEEP_AWAKE_ENV`]'s `record:<path>` selects: an acquire appends the line
/// `acquire <reason>` to the file at `<path>` and its guard's drop appends `release`, so a test
/// reads when a driver held the machine awake. An acquire it cannot record answers unavailable.
struct RecordingSleepHold(PathBuf);

impl SleepHold for RecordingSleepHold {
    fn acquire(&self, reason: &str) -> Result<SleepGuard, Unavailable> {
        append_line(&self.0, &format!("acquire {reason}"))
            .map_err(|e| Unavailable(format!("cannot record to {}: {e}", self.0.display())))?;
        let record = self.0.clone();
        // A release the record cannot take leaves its acquire standing alone, which a test reads
        // as a hold never released.
        Ok(SleepGuard::new(move || {
            let _ = append_line(&record, "release");
        }))
    }
}

/// Append `line` and a newline to the file at `path` in one write, creating the file when absent.
fn append_line(path: &Path, line: &str) -> std::io::Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    rigger::whole_line::write_line(&mut file, format_args!("{line}"))
}

/// The keep-awake fake whose every acquire answers unavailable with its reason: what
/// [`KEEP_AWAKE_ENV`]'s `unavailable:<reason>` selects, and what a value naming no adapter gets.
struct UnavailableSleepHold(String);

impl SleepHold for UnavailableSleepHold {
    fn acquire(&self, _reason: &str) -> Result<SleepGuard, Unavailable> {
        Err(Unavailable(self.0.clone()))
    }
}

/// The keep-awake hold `command` takes from `sleep` as it starts driving a run (spec 105), held
/// until the caller drops it as it stops driving; or, when the facility is unavailable, none, after
/// the one line on stderr saying so, since an unavailable facility never fails the run.
fn keep_awake(sleep: &dyn SleepHold, command: &str) -> Option<SleepGuard> {
    sleep
        .acquire(&format!("{command} is driving a run"))
        .map_err(|Unavailable(why)| {
            stderr_line(&format!(
                "rigger: keep-awake unavailable: {why}; a sleep pauses the run and it resumes by itself"
            ))
        })
        .ok()
}

/// How often a launch waiting on a held run reads the run again (spec 105), and how often the hold
/// controller polls for a probe its open hold is owed.
const HOLD_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

/// The pause between two reads of a standing hold, each run path's [`HeldDriver`] sleeping it.
fn pause_a_hold_poll() {
    std::thread::sleep(HOLD_POLL_INTERVAL);
}

/// A supervisor start's reconciliation (spec 105, RESUME BY SESSION): every launch the progress
/// `store` holds open, of any run ([`progress::open_launches`]), is closed
/// [`progress::ENDED_INTERRUPTED`] with no class on its own run's stream, so the close enters no
/// fault count. The start holds the step lock, so no other `rigger run` is driving them; a live
/// `rigger critique`'s launch is closed too, and that false close stands until its own close lands.
/// Each launch is reaped before its close, so a crash between the two re-reaps at the next start:
/// what is still rooted under its `dir`, when the project has a `scratch` root and the record names
/// a dir, through `reap` called with that dir and the root; the composition root passes
/// [`rigger::reap::reap_processes_rooted_under`], whose containment check refuses a dir outside
/// the scratch root by name and reaps nothing there. A read or an append that fails ends the start
/// with its error.
fn reconcile_open_launches(
    store: &dyn EventStore,
    scratch: Option<&str>,
    reap: &dyn Fn(&Path, &Path),
) -> Res {
    let every_run = Filter {
        stream_prefix: Some(progress::STREAM.to_string()),
    };
    let events = store.read_all(0, Direction::Forward, &every_run)?;
    for (run, open) in progress::open_launches(&events) {
        if let Some(root) = scratch.filter(|_| !open.dir.is_empty()) {
            reap(Path::new(&open.dir), Path::new(root));
        }
        let close = progress::SpawnLaunched::closed(
            open.spawn,
            open.launch,
            open.session_id,
            progress::ENDED_INTERRUPTED,
            "",
        );
        rigger::progress_store::record_launch(store, &run, &close)?;
    }
    Ok(())
}

/// The standalone CLI path: ground, host the agents as headless `claude` sessions
/// ([`claude_code::Driver`]) behind the [`HeldDriver`] that launches nothing while the run is
/// held, drive the DAG to integration with the supervisor [`beating`] and the hold controller
/// ([`rigger::hold_controller::probe_an_owed_hold`] each [`HOLD_POLL_INTERVAL`]) [`ticking`] beside it, and reclaim a done
/// run's scratch. The store is selected by flag and wrapped in the per-project namespace
/// decorator before it is injected (§5.1.1, R9). It holds the step lock from right after its root
/// check and spec read until it returns, and before its base gates closes and reaps every launch
/// the progress store holds open ([`reconcile_open_launches`]). The machine stays awake through
/// `sleep` from the moment the loop starts driving until this returns ([`keep_awake`]).
fn run_cli(parsed: &RunArgs, sleep: &dyn SleepHold) -> Res {
    let command = "rigger run";
    // DISCOVERABILITY (spec 66, criterion 5): `rigger run <spec>` is a REAL pre-launch
    // surface that genuinely holds the spec path in production (unlike the SessionStart-only
    // `rigger prime` hook, which the installed hook always invokes with zero args - see
    // `spec_lint_next_step`'s doc comment). Printed FIRST, before any config load or base
    // check, so the reminder survives every downstream refusal or failure path, not only a
    // successful run.
    if let Some(spec) = &parsed.spec {
        println!("{}", spec_lint_next_step(spec));
    }
    // THE RUN RESOLVES THE MAIN WORKTREE (spec 89, criterion 4): resolved and refused-or-not
    // FIRST - see `resolve_main_worktree_or_refuse`'s doc comment.
    let cwd = cwd();
    let repo = resolve_main_worktree_or_refuse(&cwd, command)?;
    refuse_outside_a_project(&cwd, command)?;
    // Refuse before starting if a gating persona would stall the integration gate (spec 18,
    // unit 2); `load_run_config` reuses unit 1's lint at this run's config-load seam.
    let cfg = load_run_config(".")?;
    // EXACTLY ONE ROOT: this run's teardown reclaims the scratch root and its hosted agents write
    // into it, so it is refused here, before anything is reclaimed, reaped or anchored.
    let scratch = project_scratch_root(&repo, &cfg);
    refuse_unless_one_root(&cwd, &repo, scratch.as_deref(), command)?;
    let (criteria, text) = load_criteria(parsed.spec.as_deref())?;
    std::fs::create_dir_all(RIGGER_DIR)?;
    // ONE DRIVER AT A TIME (spec 105, RESUME BY SESSION): the step lock is held from here until
    // this run returns, before any store is opened, any launch closed or any run branch anchored,
    // so a second `rigger run` beside this one refuses with the busy refusal
    // first, and a run that hangs keeps it, which keeps `rigger watch`'s dead-driver alert silent.
    let _step_lock = acquire_step_lock(Path::new(RIGGER_DIR))?;
    // Migrate a pre-spec-09 store's legacy-namespace history to the minted identity once,
    // before opening the run backend (spec 09). Local-sqlite only - the migration renames
    // streams in the local `.rigger/events.db`; a shared KurrentDB backend is out of scope.
    let selection = store_selection(parsed.store, parsed.conn.as_deref())?;
    if selection.is_sqlite() {
        migrate_local_identity(None)?;
    }
    let progress_backend = open_progress_backend()?;
    let identity = project_identity()?;
    let progress = Namespaced::new(&progress_backend, &identity);
    // No launch the progress store holds open has a live driver now that this run holds the
    // step lock: each is reaped and closed before the base gates, so a start they refuse has
    // already reconciled. The reconciliation reads the progress store alone, so the run store
    // still opens after the anchor, behind this instance's registration.
    reconcile_open_launches(
        &progress,
        scratch.as_deref(),
        &rigger::reap::reap_processes_rooted_under,
    )?;
    // Anchor + check out the run branch off `--base` (spec 18, criterion 6) BEFORE the
    // conductor branches any unit worktree off HEAD, so machine-generated units never
    // branch/merge onto the operator's own branch. The effective base is the `--base` flag,
    // then the `RIGGER_BASE` env override, then `origin/main`.
    // Guarded on a real repo, so the repo-less path is untouched.
    // The run branch's tip commit sha AT THIS ANCHOR (spec 91): resolved right after
    // `anchor_run_branch` below, before the conductor branches any unit worktree off it -
    // threaded to `fresh_run_if_requested` so a mint persists the tip the run genuinely
    // started at. `""` on the repo-less path.
    let mut base_tip = String::new();
    if !repo.is_empty() {
        let (base, base_explicit) = resolve_run_base(
            parsed.base.as_deref(),
            std::env::var("RIGGER_BASE").ok().as_deref(),
        );
        // Refuse an obviously-wrong base BEFORE anchoring (spec 18, criterion 7), gating on the
        // side-effect-free planned anchor so no wrong-base run branch is ever created and the
        // corrected `--base` retry re-anchors fresh.
        let planned = Worktree::planned_run_branch_setup(&repo, RUN_BRANCH, &base);
        // Loop-readiness gate (spec 38, criterion 2): refuse a run with no reachable base (an
        // unresolvable base AND no HEAD to fall back to) loudly rather than minting a run branch
        // that branches from nowhere.
        refuse_when_base_unreachable(&repo, command, &base, planned)?;
        refuse_when_base_lacks_spec_paths(&repo, command, &base, planned, &criteria)?;
        anchor_run_branch(&repo, command, &base, base_explicit)?;
        base_tip = rigger::worktree::branch_tip(&repo, RUN_BRANCH).unwrap_or_default();
    }
    // The host values every spawn of this run carries, resolved once, after the run-branch
    // checkout, so the helper copies are the tree's the run drives and its definition pin folds.
    let spawn_host = rigger::hooks::resolve_spawn_host(&cfg.workflow.defaults, Path::new(&repo))?;
    // Register this instance in the machine-global discovery registry (spec 50, criterion 2).
    // `rigger run` drives the WHOLE run in-process through a single `conductor::run` below, so the
    // held guard's heartbeat thread (not a one-shot register) is what keeps the entry from aging out
    // of discovery mid-run; it is dropped when `run_cli` returns. Best-effort - it never blocks the
    // run. Registered here, off the same resolved `selection` the run store opens on, so
    // discovery names the store this run drives.
    let _registration = register_run_instance(&repo, &identity, &selection);
    // The boxed backend and its namespaced wrapper both live here, in this stack
    // frame, for the whole run: the decorator borrows the concrete store, and both
    // outlive the `conductor::run` call below.
    let backend = resolve_store(&selection, &db_path("events.db"))?;
    let store = Namespaced::new(backend.as_ref(), &identity);
    let graph = open_graph(&db_path("graph.db"), &identity, &selection, "run")?;
    // Every appended event is folded into the graph as it lands, on `--fresh`'s mint and on the
    // results the host records alike.
    let folding = rigger::ingest::folding_into(&store, Some(&graph), &stderr_line);
    // Every way this run ends from here - done, a halt, an escalation, a refusal or an error -
    // passes through this one teardown, dropped after the driver, the dash and the gate socket.
    let _teardown = RunTeardown {
        scratch: scratch.as_deref(),
        store: &store,
    };
    // RUN-START SWEEPS: leftovers of earlier runs are reclaimed under the step lock, before the
    // conductor adds any unit worktree. The terminal sweep runs ahead of the definition pin, so a
    // run that halts on drift still reclaims its integrated worktrees.
    let sweep_root = scratch.as_deref().filter(|_| !repo.is_empty());
    if let Some(root) = sweep_root {
        sweep_terminal_at_start(&repo, root, &cfg.workflow, &store);
    }
    // `--fresh`: begin a NEW run before driving, so the conductor's own `ensure_started`
    // adopts this just-minted boundary instead of the (possibly wedged) latest run. See
    // `runscope_store::start_fresh` - the evented restart for a terminal escalation on an
    // unchanged spec.
    fresh_run_if_requested(
        parsed,
        &folding,
        &criteria,
        &base_tip,
        &spawn_host.helpers,
        &RunStartCritique {
            command,
            backend: backend.as_ref(),
            identity: &identity,
            workflow: &cfg.workflow,
            root: &review::spec_root(&cwd, &repo),
            text: &text,
        },
    )?;
    // The orphan sweep runs behind `--fresh`'s mint, so a superseded run's scratch reads as dead.
    if let Some(root) = sweep_root {
        reclaim_orphans_at_start(&repo, root, &cfg.workflow, &store);
    }
    let driver = claude_code::Driver {
        bin: String::new(),
        rigger_bin: running_executable(),
        progress_store: &progress,
        run_store: &folding,
        scratch_root: scratch.clone().unwrap_or_default(),
        stop_grace: claude_code::STOP_GRACE,
        handed_env: selection.handed_env(),
        beat_source: Some(&progress),
        hold_sink: Some(&folding),
    };
    // The decorator records the errors it hands the conductor on the folding store the host
    // records results on, so the graph folds them as it folds the host's.
    let held = HeldDriver::new(&driver, &folding, &pause_a_hold_poll);
    let grounder = select_grounder(&cfg.workflow.defaults.grounder)?;
    let root = repo.clone();
    // The run refuses a linked worktree, so `repo` is the operator checkout the store is rooted at.
    let code_store = run_code_store(&repo);
    // The grounding reads compare with the operator checkout the run lands its merges on, named
    // as every reader names its worktree; a unit's read compares with the unit's own worktree.
    let read_graph = ReadGraph::new(&graph, tree_of(Path::new(&repo))?);
    let deps = Deps {
        store: &store,
        driver: &held,
        gates: &ExecRunner,
        repo,
        grounder: Some(grounder.as_ref()),
        graph: Some(&graph),
        criteria,
        log: &stderr_line,
        check_emit: &rigger::contextgraph::check_emit,
        spawn_host,
        code_store: code_store.as_deref(),
        context: Some(&read_graph),
    };
    // Always-on dash (spec 19b, unit 1): auto-start a `rigger dash` serving this run before
    // the loop begins, so an active harness is never invisible. Held for the whole run - the
    // guard reaps the dash when this scope ends (unit 3's reaping mechanism).
    let _dash = start_run_dashboard(cfg.workflow.dash_enabled(), &store);
    let _awake = keep_awake(sleep, command);
    // The hold controller polls beside the drive, on the folding store the host holds the run on.
    let control_holds = || {
        let now = std::time::SystemTime::now();
        if let Err(e) =
            rigger::hold_controller::probe_an_owed_hold(&folding, &progress, &driver, &root, now)
        {
            stderr_line(&format!("rigger run: the hold probe: {e}"));
        }
    };
    // SIGNED VERDICTS: this driver process's verdict key lives only in its memory, and the
    // project's gate request socket ([`rigger::gate_socket::socket_path`]) carries each
    // `rigger gate <id>` to it for as long as the run drives; every spawn is handed its path as
    // `RIGGER_GATE_SOCKET`. A socket that cannot be bound leaves the verb information-only. A
    // project with no repository has no unit worktree a request could come from, so it binds none.
    let verdict_key = rigger::verdict_mac::VerdictKey::generate()?;
    let socket = (!root.is_empty())
        .then(|| {
            rigger::gate_socket::socket_path(Path::new(&root))
                .and_then(|path| rigger::gate_socket::GateSocket::bind(&path))
                .inspect_err(|e| {
                    stderr_line(&format!(
                        "rigger run: gate requests are not served ({e}); `rigger gate <id>` runs \
                         for information only and records nothing"
                    ))
                })
                .ok()
        })
        .flatten();
    let channel = socket
        .as_ref()
        .map(|s| s as &dyn rigger::gate_channel::GateChannel);
    let rs = beating(
        &store,
        &progress,
        rigger::progress::SUPERVISOR_BEAT_INTERVAL,
        || {
            ticking(HOLD_POLL_INTERVAL, control_holds, || {
                conductor::run_serving(&cfg, &deps, verdict_key, channel)
            })
        },
    )?;
    // A done run releases its open hold, an operator hold included (spec 105); any other exit
    // leaves it to the next supervisor's probes. A failed release never fails a run that already
    // succeeded: the hold stays open for `rigger release` or the next mint.
    if rs.done() {
        let now = std::time::SystemTime::now();
        if let Err(e) = runscope_store::release_hold(&folding, runscope::HOLD_BY_HOST, now) {
            stderr_line(&format!(
                "rigger run: the done run's hold was not released: {e}"
            ));
        }
    }
    // The release-target base the ready-to-release handoff names (spec 38, criterion 3): read
    // the base PERSISTED on this run's RunStarted, so the end-of-run summary, `rigger status`,
    // and `rigger dash` all name the ONE base the run anchored on. A run started before base
    // persistence existed (or without a repo) carries none, so fall back to the same
    // flag/env/default resolution the run branch was anchored with.
    let release_base = runscope::read::read_current_run(&store, conductor::STREAM)
        .ok()
        .and_then(|(events, _)| runscope::current_run_base(&events))
        .unwrap_or_else(|| {
            resolve_run_base(
                parsed.base.as_deref(),
                std::env::var("RIGGER_BASE").ok().as_deref(),
            )
            .0
        });
    print_run_state(&rs, &release_base);
    if let Some(line) = halted_line(&rs) {
        println!("{line}");
    }
    // spec 17 criterion 4c: a silently-serializing fleet must WARN during a run, not only when the
    // operator later runs `rigger stats`. Re-project this run's metrics from the log and, if the
    // parallelism-retention floor was breached under structural grounding, log the SAME line the
    // stats row shows (one authority) to stderr. Best-effort: a read hiccup never fails a run that
    // already succeeded, and on the shipped non-symbols default retention is unmeasured so nothing
    // prints and the default run output is unchanged.
    if let Ok((events, _)) = runscope::read::read_current_run(&store, conductor::STREAM) {
        let m = metrics::project(&events);
        if let Some(line) = parallelism_retention_line(&m) {
            if m.parallelism_retention_warns() {
                rigger::whole_line::say(format_args!("rigger: {line}"));
            }
        }
    }
    Ok(())
}

/// The unit branches `workflow` declares: config, never the event log, so it is populated even
/// before a project's first run records an event. Both run-start sweeps spare a dirty candidate
/// only when it is one of these.
fn declared_unit_branches(workflow: &config::Workflow) -> std::collections::HashSet<String> {
    workflow
        .stages
        .keys()
        .map(|slug| conductor::unit_branch(slug))
        .collect()
}

/// Remove the terminal worktrees under `root` (integrated units, review scaffolding) that an
/// earlier run left, sparing every unit still live in the current run and any branch whose latest
/// spawn is still in flight ([`rigger::worktree::sweep_terminal`]). An unreadable stream skips
/// the sweep ([`live_branches_for_sweep`]), since liveness may be under- but never over-determined.
/// Best-effort: a failure warns and never blocks the run.
fn sweep_terminal_at_start(
    repo: &str,
    root: &str,
    workflow: &config::Workflow,
    store: &dyn EventStore,
) {
    let Some(live_branches) =
        live_branches_for_sweep(runscope::read::read_run(store, conductor::STREAM))
    else {
        return;
    };
    let fence_events = runscope::read::read_current_run(store, conductor::STREAM)
        .map(|(run, _)| run)
        .unwrap_or_default();
    match rigger::worktree::sweep_terminal(
        repo,
        root,
        RUN_BRANCH,
        &live_branches,
        &declared_unit_branches(workflow),
        &fence_events,
    ) {
        Ok(0) => {}
        Ok(n) => stderr_line(&format!(
            "rigger run: swept {n} terminal worktree(s) from {root}"
        )),
        Err(e) => stderr_line(&format!("rigger run: scratch sweep skipped: {e}")),
    }
}

/// Reclaim the scratch under `root` no live unit of the current run owns: an earlier run's
/// stranded worktree or build cache, or a cache an agent wrote outside its assigned path
/// ([`reclaim_orphan_scratch`]). The shared areas a running spawn may write into are spared.
/// Best-effort: a failure warns and never blocks the run.
fn reclaim_orphans_at_start(
    repo: &str,
    root: &str,
    workflow: &config::Workflow,
    store: &dyn EventStore,
) {
    match runscope::read::read_run(store, conductor::STREAM) {
        Ok(events) => {
            let removed = reclaim_orphan_scratch(
                repo,
                root,
                &current_run_units(&events),
                &declared_unit_branches(workflow),
            );
            if removed > 0 {
                stderr_line(&format!(
                    "rigger run: reclaimed {removed} orphaned scratch entr{} under {root}",
                    if removed == 1 { "y" } else { "ies" }
                ));
            }
        }
        Err(e) => stderr_line(&format!("rigger run: orphan sweep skipped: {e}")),
    }
}

/// Begin (or adopt) and definition-PIN the run `rigger run` drives (spec 13, unit 1).
/// First, a run that is not `--fresh` reads its hold and names a standing one on stderr
/// ([`held_line`], this driver's own beat being the latest), failing on a hold it cannot read.
/// Then, before anything is appended, a command that would begin a new run on a spec whose
/// critique is not clean is refused (spec 112, [`RunStartCritique::refuse_unless_clean`]; the
/// caller hands `critique`, naming its own command).
/// When `--fresh` is set it appends a new pinned `RunStarted` for `criteria` so the run starts
/// a clean slice even if the latest run already matches (which `ensure_started` would adopt),
/// printing the new run id. It then enforces the definition pin ([`enforce_definition_pin`]):
/// a drifted live-run definition HALTS loudly unless `--rebase-definition` records the
/// supersession and continues. A fresh or unchanged run continues silently.
///
/// `base_tip` is the run branch's tip commit sha the caller anchored at (spec 91), resolved via
/// [`worktree::branch_tip`] right after its own `anchor_run_branch` call - BEFORE the conductor
/// (or anything else) advances that branch - so this always names the tip AT run start, never a
/// live re-resolution. Persisted only on a mint, exactly like `base`; `""` from a repo-less path
/// leaves the checkin stage (which never runs without a real repo anyway) nothing to diff.
///
/// `helpers` are the fan-out helpers the caller's spawn host resolved, so the definition the run
/// pins folds the same helper copies its spawns carry ([`definition_hash`]).
fn fresh_run_if_requested(
    parsed: &RunArgs,
    store: &dyn EventStore,
    criteria: &[String],
    base_tip: &str,
    helpers: &[conductor::HelperCopy],
    critique: &RunStartCritique,
) -> Result<(), Box<dyn std::error::Error>> {
    // A HELD RUN IS NAMED FIRST: a plain run reads its hold ahead of the critique refusal, so a
    // held run names its hold before anything refuses it and a hold it cannot read fails the run
    // before any refusal. `--fresh` decides its refusal first, since its mint ends the hold.
    if !parsed.fresh {
        if let Some(hold) = runscope::read::read_current_hold(store, conductor::STREAM)? {
            let now = std::time::SystemTime::now();
            stderr_line(&format!(
                "{}: {}",
                critique.command,
                held_line(&hold, Some(now), now)
            ));
        }
    }
    critique.refuse_unless_clean(parsed.spec.as_deref(), criteria, parsed.fresh)?;
    let definition = definition_hash(".", helpers)?;
    // The resolved run-branch base to persist on the RunStarted this mints (spec 38, criterion
    // 3), resolved from the SAME precedence the run branch is anchored with (the `--base` flag,
    // then the `RIGGER_BASE` env override, then the default) so the persisted base matches the
    // branch the run actually targets. Persisted only on a mint (a `--fresh` boundary or a new
    // campaign); an adopted resume keeps its original stamp.
    let (base, _) = resolve_run_base(
        parsed.base.as_deref(),
        std::env::var("RIGGER_BASE").ok().as_deref(),
    );
    if parsed.fresh {
        let run = runscope_store::start_fresh(
            store,
            criteria,
            &definition,
            &base,
            base_tip,
            parsed.spec.as_deref().unwrap_or(""),
        )?;
        println!("rigger: --fresh: began a new run {run} (the prior run stays in the log)");
    }
    enforce_definition_pin(
        store,
        criteria,
        &definition,
        parsed.rebase_definition,
        &base,
        base_tip,
        parsed.spec.as_deref().unwrap_or(""),
    )?;
    Ok(())
}

/// What the spec-critique refusal ([`RunStartCritique::refuse_unless_clean`]) reads beside the
/// run a command is about to begin or adopt (spec 112, A NEW RUN IS REFUSED UNTIL ITS CRITIQUE IS
/// CLEAN): every input but the spec, its criteria and `--fresh`, which the run entry holds.
struct RunStartCritique<'a> {
    /// The invoking command: the prefix of the refusal and of the no-critic line.
    command: &'a str,
    /// The selected backend, which the project store and the critique's own store
    /// ([`critique_store`]) are both namespaced over.
    backend: &'a dyn EventStore,
    /// The project identity both stores are namespaced by.
    identity: &'a str,
    /// The loaded workflow, whose critic ([`rigger::wave::critic_of`]) decides whether the refusal
    /// applies and holds the ceiling the critique's attempt count is held to.
    workflow: &'a config::Workflow,
    /// The root the spec path and every `governs` entry are made relative to
    /// ([`review::spec_root`]).
    root: &'a Path,
    /// The exact spec text `load_criteria` read for this command: the critique is keyed on its
    /// hash.
    text: &'a str,
}

impl RunStartCritique<'_> {
    /// Refuse a command that would begin a new run on `spec` unless the critique of its text is
    /// clean - the one decision every CLI run start makes before it appends anything. A command
    /// with no spec, and one that adopts the latest run ([`runscope::adopted_run`] over the run
    /// stream's [`runscope::MINT_DECISION_TYPES`] events, `criteria` and `fresh`, as a command
    /// naming its spec), proceeds. Under a workflow naming no
    /// critic a new run proceeds after one line on stderr saying so. Otherwise a spec outside the
    /// repository is refused as not critiqued, and one inside it proceeds only while its critique
    /// is clean ([`Self::refusal`]); else the new run is refused in the shape that names, with the
    /// text [`new_run_refusal`] renders. It writes nothing.
    fn refuse_unless_clean(&self, spec: Option<&str>, criteria: &[String], fresh: bool) -> Res {
        let Some(spec) = spec else {
            return Ok(());
        };
        let project = Namespaced::new(self.backend, self.identity);
        let started = project.read_stream_typed(
            conductor::STREAM,
            0,
            TypeSelection::Only(&runscope::MINT_DECISION_TYPES),
        )?;
        if runscope::adopted_run(&started, criteria, fresh, true).is_some() {
            return Ok(());
        }
        let path = review::normalize_spec_path(self.root, spec);
        let named = path.as_deref().unwrap_or(spec);
        let Some(critic) = rigger::wave::critic_of(self.workflow) else {
            rigger::whole_line::say(format_args!(
                "{}: no spec critique for {named}: {}",
                self.command,
                rigger::wave::NO_CRITIC_CLAUSE
            ));
            return Ok(());
        };
        let refusal = match &path {
            Some(path) => self.refusal(&project, path, &critic)?,
            None => Some(NewRunRefusal::Outside),
        };
        match refusal {
            None => Ok(()),
            Some(refusal) => Err(new_run_refusal(self.command, named, refusal).into()),
        }
    }

    /// Why the critique of this text refuses a new run, `spec` being its repo-relative path and
    /// `critic` the workflow's, whose ceiling the attempt count is held to (spec 123, CLEAN MEANS
    /// EXHAUSTED); `None` when it is CLEAN. The critique and its attempt count
    /// ([`review::next_critique_attempt`]) are read through the critique's own store, the
    /// resolutions from the project run stream's `DecisionMade` events ([`open_blocking_ids`]).
    /// The critique is settled when it is exhausted or its attempt count has reached the ceiling
    /// ([`rigger::wave::Critic::settles`]). An open BLOCKING
    /// id refuses on the ids, settled or not; with none open a settled critique is CLEAN, and an
    /// unsettled one, or a text with no pass, is not critiqued to exhaustion.
    fn refusal(
        &self,
        project: &dyn EventStore,
        spec: &str,
        critic: &rigger::wave::Critic,
    ) -> Result<Option<NewRunRefusal>, Box<dyn std::error::Error>> {
        let critiques = critique_store(self.backend, self.identity);
        let hash = review::critique_hash(self.text);
        let Some(critique) = review::read_critique(&critiques, &hash)? else {
            return Ok(Some(NewRunRefusal::Unexhausted));
        };
        let ids = open_blocking_ids(project, &critique, spec, self.root)?;
        let settled = critic.settles(&critique, review::next_critique_attempt(&critiques, &hash)?);
        Ok(match (ids.is_empty(), settled) {
            (false, settled) => Some(NewRunRefusal::Open { ids, settled }),
            (true, true) => None,
            (true, false) => Some(NewRunRefusal::Unexhausted),
        })
    }
}

/// Why a new run is refused (spec 123, CLEAN MEANS EXHAUSTED): each shape [`new_run_refusal`]
/// renders.
enum NewRunRefusal {
    /// The spec is outside the repository, so its text cannot be critiqued.
    Outside,
    /// The critique holds these open BLOCKING finding ids; `settled` when it is exhausted or at
    /// its ceiling, the only state in which their resolution is offered.
    Open { ids: Vec<String>, settled: bool },
    /// The critique holds no open BLOCKING id but has no pass, or is neither exhausted nor at its
    /// ceiling.
    Unexhausted,
}

/// The refusal of a new run on `spec` by `command` in the shape `refusal` (spec 112, Notes; spec
/// 123, CLEAN MEANS EXHAUSTED): why, then the command that critiques the spec, then - for a spec
/// outside the repository, or for open ids of a critique exhausted or at its ceiling - the
/// command recording a resolution, the labels padded to one column.
fn new_run_refusal(command: &str, spec: &str, refusal: NewRunRefusal) -> String {
    let (why, resolution) = match refusal {
        NewRunRefusal::Outside => (
            "not critiqued".to_string(),
            Some((
                "or, once critiqued, record a resolution:",
                "[<ids>]".to_string(),
            )),
        ),
        NewRunRefusal::Open { ids, settled } => (
            format!("open BLOCKING findings: {}", ids.join(", ")),
            settled.then(|| {
                (
                    "or record a resolution:",
                    serde_json::json!(ids).to_string(),
                )
            }),
        ),
        NewRunRefusal::Unexhausted => ("not critiqued to exhaustion".to_string(), None),
    };
    let mut refused = format!(
        "{command}: refusing to begin a new run on {spec}: {why}\n  {:<33} rigger critique {spec}",
        "amend the spec and critique it:"
    );
    if let Some((route, resolves)) = resolution {
        let governs = serde_json::json!([spec]);
        refused.push_str(&format!(
            "\n  {route:<33} rigger emit DecisionMade '{{\"id\":\"...\",\"governs\":{governs},\
             \"resolves\":{resolves},\"summary\":\"...\"}}'"
        ));
    }
    refused
}

/// Extract the spec's acceptance criteria, enforcing the loop-ready gate (§8): a
/// spec with no enumerable Done-when criteria blocks until a human adds them; no
/// spec path means an unconstrained run (empty criteria).
///
/// The ONE in-run spec-lint call site (spec 66, criterion 4 - ONE LINT AUTHORITY): the live-run
/// entry (`rigger run` via `run_cli`) and `rigger critique` call this SAME function to load a
/// spec's criteria, so wiring the lint here reaches both without a bespoke call site per entry.
/// Prints via `spec_lint_warning_lines` - the identical formatter `cmd_validate` (the pre-launch,
/// standalone `rigger validate <spec>`) uses, itself built on `spec::spec_lint_advisories`
/// (criterion 3's untouched classification) - so an operator who launches straight into a run
/// without a separate `rigger validate` pass still sees the same advisories. Advisory only, exactly
/// like the pre-launch surface: never refuses the run.
///
/// It hands back the criteria beside the exact text its one read read (an empty text, with no
/// read, when there is no spec path), so a caller that also hashes or prompts on the spec - the
/// `rigger critique` verb (spec 112) - never reads it a second time.
pub(crate) fn load_criteria(
    spec_path: Option<&str>,
) -> Result<(Vec<String>, String), Box<dyn std::error::Error>> {
    let Some(spec_path) = spec_path else {
        return Ok((Vec::new(), String::new()));
    };
    let text =
        std::fs::read_to_string(spec_path).map_err(|e| format!("read spec {spec_path}: {e}"))?;
    for line in spec_lint_warning_lines(spec_path, &text) {
        rigger::whole_line::say(format_args!("{line}"));
    }
    let criteria = spec::extract_criteria(&text);
    if criteria.is_empty() {
        return Err(format!(
            "loop-ready: spec {spec_path} has no enumerable Done-when criteria (checkbox items); add them before running"
        )
        .into());
    }
    Ok((criteria, text))
}

/// `rigger dash` - serve or export the embedded observability page (spec 11, unit 2).
///
/// A READ-ONLY window over the existing projections: the conductor stays the sole mutation
/// authority, so the dash has no write or control surface (enforced in [`dash::route`],
/// which answers only `GET`). `rigger dash` serves the live-polling single-file page on
/// loopback (`127.0.0.1`, default [`dash::DEFAULT_PORT`], override with `--port`);
/// `rigger dash --export <path>` writes the equivalent static, shareable snapshot.
///
/// Composition mirrors the sibling operator reads (`stats`, `graph`): it resolves this
/// project's `.rigger/events.db` + `.rigger/graph.db` by cwd (via [`db_path`] /
/// [`project_identity`]) and re-reads them on EACH request, so the page reflects the run
/// as it advances. An ABSENT `events.db` reads as an empty run (guarded BEFORE
/// [`Store::open`], which would otherwise create it), so an operator can launch the dash
/// first and watch the run populate it. The context graph is best-effort: a grep-only run
/// never builds one, and an absent or unreadable `graph.db` yields an empty graph rather
/// than failing the whole page.
/// Auto-start a read-only `rigger dash` for the run a driver is about to drive, so an active
/// harness is never invisible (spec 19b, unit 1: always-on, no opt-in flag). The dash binds
/// [`dash::DEFAULT_PORT`] or the next free loopback port itself (so two concurrent harnesses each
/// get their OWN); its URL is printed at run start and recorded in `.rigger/`[`DASH_URL_FILE`]
/// so `rigger status` can surface it.
///
/// Returns the [`RunDash`] guard the DRIVER holds for the whole run: dropping it (on a normal
/// finish OR an unwinding panic) reaps the dash and removes its marker. That guard is unit 3's reaping
/// mechanism, reused here as the single reaper - THIS unit owns only start + discoverability,
/// never stopping. Best-effort: if the dash cannot be started the run still proceeds (the
/// dash is observability, not the deliverable), so a port-starved or spawn-refused
/// environment degrades to a headless run rather than aborting one.
///
/// `store` is read once here to stamp [`DASH_ATTEMPT_FILE`] with the current run's id via
/// [`record_dash_attempt`] (spec 69, round-8 fix) - by the time this runs,
/// `fresh_run_if_requested` has already ensured/minted the run, so the id is always available
/// for a real run.
///
/// OPT-OUT: the one opt-out ([`dash_opted_out`] over `config_dash_enabled`, the workflow's
/// resolved [`config::Workflow::dash_enabled`]) starts no dash and stamps no attempt.
fn start_run_dashboard(config_dash_enabled: bool, store: &dyn EventStore) -> Option<RunDash> {
    if dash_opted_out(config_dash_enabled) {
        return None;
    }
    if let Ok((_, run_id)) = runscope::read::read_current_run(store, conductor::STREAM) {
        record_dash_attempt(&run_id);
    }
    match spawn_run_dashboard() {
        Ok((guard, url)) => {
            // Stderr: stdout is the run's own report, which the run-start pointer stays out of.
            // One write, since the dash just started says its own line on the same stream.
            rigger::whole_line::on_stderr(|err| rigger::dash::announce_run_serving(err, &url));
            Some(guard)
        }
        Err(e) => {
            rigger::whole_line::say(format_args!(
                "rigger: could not auto-start the dashboard ({e}); the run continues headless"
            ));
            None
        }
    }
}

/// How long the run waits for its dash to report the address it bound before it continues
/// headless: a dash that fails to start ends its stdout and is noticed at once, so the bound only
/// ever holds a run whose dash hangs before binding.
const DASH_REPORT_BOUND: std::time::Duration = std::time::Duration::from_secs(10);

/// Spawn `rigger dash --port-from <DEFAULT_PORT>` as a child of the current executable, read the
/// URL it reports once it has bound the first free port, and record that URL in
/// `.rigger/`[`DASH_URL_FILE`] for `rigger status` and its port and pid in
/// `.rigger/`[`DASH_MARKER_FILE`] for `rigger watch` and `rigger status`. The dash chooses its
/// own port by binding it, so two runs starting together never record one address. The child's
/// stdout is piped to this read alone, never the run's own report; the dash logs to its own
/// stderr. Its stdin is a pipe whose write end the guard's child handle holds for the whole run,
/// and the dash runs with `--exit-on-stdin-close`, so however the run ends, the closed pipe ends
/// the dash with it. A dash that reports no address within [`DASH_REPORT_BOUND`] is reaped and
/// nothing is recorded. Returns the guard plus the URL for the run-start pointer.
fn spawn_run_dashboard() -> Result<(RunDash, String), Box<dyn std::error::Error>> {
    let exe = std::env::current_exe()?;
    let mut child = dash::ReapedChild::new(
        subprocess::command(exe)
            .arg("dash")
            .arg("--port-from")
            .arg(dash::DEFAULT_PORT.to_string())
            .arg("--exit-on-stdin-close")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?,
    );
    let mut reported = None;
    child.read_lines_until(std::time::Instant::now() + DASH_REPORT_BOUND, |line| {
        reported = dash::url_port(line);
        if reported.is_some() {
            std::ops::ControlFlow::Break(())
        } else {
            std::ops::ControlFlow::Continue(())
        }
    });
    let port = reported.ok_or("the dashboard reported no address it serves at")?;
    let url = format!("http://127.0.0.1:{port}/");
    // Discoverability breadcrumb for `rigger status`; best-effort and overwritten each run.
    // `.rigger/` already exists (the driver created it before reaching here).
    let _ = std::fs::write(db_path(DASH_URL_FILE), &url);
    let marker = dash::DashMarker {
        port,
        pid: child.id(),
    };
    let marker_path = PathBuf::from(db_path(DASH_MARKER_FILE));
    let _ = marker.write(&marker_path);
    Ok((
        RunDash {
            child: Some(child),
            marker,
            marker_path,
        },
        url,
    ))
}

/// The dash `rigger run` started, held for the whole run: dropping it reaps the dash child
/// ([`dash::ReapedChild`]) and then removes the [`DASH_MARKER_FILE`] it recorded, unless the
/// file by then names another dash, so a marker on disk names a dash a run still holds or one
/// whose run never reached its teardown. A run that never reaches its teardown still takes its
/// dash with it: the child handle holds the dash's stdin pipe, which closes when the run ends.
struct RunDash {
    child: Option<dash::ReapedChild>,
    marker: dash::DashMarker,
    marker_path: PathBuf,
}

impl Drop for RunDash {
    fn drop(&mut self) {
        drop(self.child.take());
        if dash::DashMarker::read(&self.marker_path) == Some(self.marker) {
            let _ = std::fs::remove_file(&self.marker_path);
        }
    }
}

/// Whether the always-on dash auto-ensure is SUPPRESSED for this run (spec 50, criterion 4) -
/// the single opt-out authority. It is suppressed by EITHER opt-out: the environment disable
/// ([`DASH_DISABLE_ENV`] set to any value, `env_disabled`) OR the config opt-out (`dash: off`,
/// surfaced here as `config_dash_enabled == false`). Pure over its two resolved inputs so both
/// opt-out paths - and the fact that either alone suffices - are provable without mutating the
/// process environment or cwd; the real caller resolves `env_disabled` from the environment and
/// `config_dash_enabled` from the already-loaded workflow.
fn dash_ensure_suppressed(env_disabled: bool, config_dash_enabled: bool) -> bool {
    env_disabled || !config_dash_enabled
}

/// [`dash_ensure_suppressed`] with `env_disabled` read from the process environment: the opt-out
/// every run entry's always-on dash honors.
fn dash_opted_out(config_dash_enabled: bool) -> bool {
    dash_ensure_suppressed(
        std::env::var_os(DASH_DISABLE_ENV).is_some(),
        config_dash_enabled,
    )
}

/// A parsed `rigger result` invocation (see [`cmd_result`]): the spawn `id`, the
/// optional outcome `text` (`None` means "read it from stdin"), whether `--error`
/// marks it a failure, and whether `--supersede` replaces the result of a spawn that
/// already ended.
struct ResultArgs {
    id: String,
    text: Option<String>,
    is_error: bool,
    supersede: bool,
}

/// Parse `rigger result <id> [<output>] [--error] [--supersede]`.
///
/// `<id>` is the required deterministic spawn id (`{unit}/{role}#{attempt}`). The
/// outcome payload is an OPTIONAL second positional; when omitted, [`cmd_result`]
/// reads it from stdin. `--error` is a bare flag that turns the payload into the failure
/// message rather than the agent's output. `--supersede` is a bare flag: the explicit
/// operator repair that replaces the result of a spawn that already ended, which a plain
/// record refuses ([`result_refusal`]). Unknown flags, a missing/empty id and a third
/// positional are all rejected with a clear message.
fn parse_result_args(args: &[String]) -> Result<ResultArgs, Box<dyn std::error::Error>> {
    const USAGE: &str = "rigger result <id> [<output>] [--error] [--supersede]";
    let mut id: Option<String> = None;
    let mut text: Option<String> = None;
    let mut is_error = false;
    let mut supersede = false;
    for arg in args {
        match arg.as_str() {
            "--error" => is_error = true,
            "--supersede" => supersede = true,
            flag if flag.starts_with("--") => {
                return Err(format!("result: unknown flag {flag:?}").into());
            }
            positional => {
                if id.is_none() {
                    id = Some(positional.to_string());
                } else if text.is_none() {
                    text = Some(positional.to_string());
                } else {
                    return Err(format!(
                        "result: unexpected extra argument {positional:?}; usage: {USAGE}"
                    )
                    .into());
                }
            }
        }
    }
    let id = id.ok_or_else(|| format!("result: expected a spawn id: {USAGE}"))?;
    if id.is_empty() {
        return Err("result: the spawn id must not be empty".into());
    }
    Ok(ResultArgs {
        id,
        text,
        is_error,
        supersede,
    })
}

/// Build the [`spawn::SpawnResult`] a `rigger result` invocation records, from its
/// parsed pieces and the already-resolved outcome `text` (positional arg or stdin).
///
/// Split from [`cmd_result`] (which does the stdin + store I/O) so the outcome-shaping
/// rules are a pure, unit-testable function. `--error` needs a NON-EMPTY message: a
/// blank error would leave [`spawn::SpawnResult::is_error`] false, so the replay driver
/// would answer the spawn AS a success and silently swallow the failure the operator
/// meant to record. A success may carry empty output (an agent that finished with no
/// final message is a valid outcome).
fn build_result(
    id: &str,
    text: &str,
    is_error: bool,
) -> Result<spawn::SpawnResult, Box<dyn std::error::Error>> {
    if !is_error {
        return Ok(spawn::SpawnResult::ok(id, text));
    }
    if text.trim().is_empty() {
        return Err(format!(
            "result: --error for {id:?} needs a non-empty message (a blank error would replay as a success)"
        )
        .into());
    }
    Ok(spawn::SpawnResult::failed(id, text))
}

/// Read the outcome payload from stdin when it was not given as an argument. A pipe /
/// heredoc conventionally appends a trailing newline (e.g. `echo "$out" | rigger
/// result ...`), so a SINGLE trailing `\n` (and a preceding `\r`) is stripped, leaving
/// exactly the payload rather than the shell's line terminator. Reading from an
/// interactive terminal with no argument would block forever, so that is a clear error
/// instead.
fn read_outcome_from_stdin() -> Result<String, Box<dyn std::error::Error>> {
    use std::io::{IsTerminal, Read};
    if std::io::stdin().is_terminal() {
        return Err("result: no outcome given - pass it as an argument (rigger result <id> <output>) or pipe it on stdin".into());
    }
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf)?;
    if buf.ends_with('\n') {
        buf.pop();
        if buf.ends_with('\r') {
            buf.pop();
        }
    }
    Ok(buf)
}

/// `rigger result <id> [<output>] [--error] [--supersede]` - record a spawn's OUTCOME to
/// the run log, so the next `rigger run` answers that spawn from the log instead of
/// running it. It is the operator's repair for a halt that names it: a reviewer whose
/// recorded result carries no verdict, or only empty output, is answered by recording the
/// substantive result over the standing one with `--supersede`, then relaunching the run.
///
/// The [`spawn::SpawnResult`] is appended to the SAME per-project [`Namespaced`] `run`
/// stream the conductor drives, so the write lands exactly where the replay reads.
/// A recorded failure replays AS a failure - the conductor remediates it just as it would
/// a live one. The store is RESOLVED by walking up to the project's existing `.rigger`
/// (refusing to fabricate one in the wrong cwd - see [`require_store_dir`]); and before
/// recording, a single pre-write read of the stream refuses to replace the result of a
/// spawn that already ended unless `--supersede` (see [`result_refusal`]), then prints
/// stderr advisories for an ORPHAN id (no matching spawn request) or for SUPERSEDING an
/// existing result (see [`result_advisories`]).
pub(crate) fn cmd_result(args: &[String]) -> Res {
    let parsed = parse_result_args(args)?;
    // The outcome text comes from the positional arg when given, else stdin. Resolving
    // it here keeps `build_result` a pure function of already-resolved pieces.
    let text = match parsed.text {
        Some(t) => t,
        None => read_outcome_from_stdin()?,
    };
    let res = build_result(&parsed.id, &text, parsed.is_error)?;

    // Resolve the EXISTING store (walk up; refuse if none) rather than fabricating one
    // in the wrong cwd, scoped by the RESOLVED root's identity, so the record lands in the
    // stream the run reads (see [`require_store_dir`] / [`StoreLocation::identity`]).
    let (loc, selection) = require_store_dir()?;
    // A recorded result is project activity: re-stamp this project's registry heartbeat before
    // the work below, so the instance stays discoverable. Best-effort; never fails the result.
    refresh_registry_entry(&loc, &selection);
    over_the_run_store_at(&loc, &selection, |store| {
        // One cheap pre-write read of the run stream. A plain record over the result of a spawn
        // that already ended is refused unless `--supersede` names the repair; a record that
        // goes ahead is then advised (on stderr) about an orphan id or about superseding an
        // existing result (see [`result_advisories`]).
        let (prior, _) = runscope::read::read_current_run(store, conductor::STREAM)?;
        if let Some(refusal) = result_refusal(&prior, &res.id, parsed.supersede)? {
            return Err(refusal.into());
        }
        for note in result_advisories(&prior, &res.id) {
            rigger::whole_line::say(format_args!("{note}"));
        }
        let kind = if res.is_error() {
            "error result"
        } else {
            "result"
        };
        let pos = spawn_store::record_result(store, &res)?;
        println!("recorded {kind} for {} (position {pos})", res.id);

        // Fold the just-recorded result into this run's context graph through the same fold
        // outcome `rigger emit` reports (see [`cmd_emit`] / [`mcpserver::emit_event`]). An
        // adjudicator's recorded `SpawnResult` is where a review's findings are disposed: the
        // `TYPE_SPAWN_RESULT` fold arm reads its verdict line's `discarded` ids and invalidates
        // those findings' graph edges, so grounding stops surfacing them. Only an adjudicator
        // result disposes anything, so folding every recorded result is safe.
        //
        // AFTER the durable append, as `emit_event` does: the record already landed in the log, so
        // a fold that cannot happen never fails it - but it is SAID, with the reason, because a
        // graph that missed the fold keeps the result out of grounding until a later
        // `rigger setup` folds it.
        if let contextgraph::Fold::NotFolded(why) =
            fold_recorded_result(&loc, &selection, &res, pos)
        {
            println!("{}", not_folded(&why));
        }

        // Reclaim the dedicated scratch dir rigger assigned this spawn under `.rigger/tmp`, for
        // any outcome. Best-effort: the record already landed durably, so scratch reclamation
        // never fails a recorded result, and an already-gone path is a graceful no-op.
        reclaim_spawn_scratch(&loc, &prior, &res.id);
        Ok(())
    })
}

/// Reclaim the per-spawn scratch dir [`spawn_scratch_path`] assigned spawn `spawn_id`,
/// resolving the scratch root and run id the SAME way the assignment (the replay driver's
/// park) did so the reclaim targets the exact path the run created (spec 34, criterion 1).
/// Only the reporting spawn's own agent scratch is reclaimed: the path is keyed by the FULL
/// spawn id, so concurrent speculation lanes of one unit never share a reclaim target.
///
/// Entirely best-effort and platform-tolerant: the result already landed durably in
/// `events.db`, so neither resolving a root nor removing a dir may surface an error that fails
/// a recorded result, and an already-gone (or never-populated) path is a graceful no-op. A
/// DEGENERATE id (`spawn_scratch_path` returning `None` -
/// [`crate::liveness::marker_filename`]'s own doc comment) is likewise a no-op, never a
/// fabricated path to reap: no fixed placeholder drawn from that function's own reachable
/// alphabet can ever be proven disjoint from a real id's own mapped output (round-6 review
/// reject), so skipping is the only fail-safe answer.
/// [`reap_then_remove_dir`] reaps any process still rooted under the scratch (spec 23) before
/// removing it, so a build a hung worker left running never outlives its now-deleted cwd.
fn reclaim_spawn_scratch(loc: &StoreLocation, prior: &[Event], spawn_id: &str) {
    let repo = loc.repo_root();
    if repo.is_empty() {
        return;
    }
    // The run's scratch root by the SAME precedence the run assigned the path with
    // (`scratch_root_from_env`: RIGGER_TMPDIR > `defaults.workdir` > the `<repo>/.rigger/tmp`
    // default). The courier inherits the run's `RIGGER_TMPDIR`; `workdir` reads best-effort
    // via the shared `scratch_defaults` (spec 83 criterion 2 round 2), falling back to the
    // repo default when the config is momentarily unreadable (the overwhelming common
    // placement). The read-only `_path_` resolver never conjures a root.
    let (workdir, _max_retries) = scratch_defaults(loc);
    let scratch_root = rigger::worktree::scratch_root_path_from_env(&repo, &workdir);
    let run_id = runscope::current_run_id(prior).unwrap_or_default();
    reclaim_spawn_registered_scratch(&scratch_root, &run_id, spawn_id);
}

/// Fold a just-recorded [`spawn::SpawnResult`] into the run's context graph at its recorded
/// `position`, so an adjudicator verdict that disposes a review's findings invalidates their
/// graph edges (the `contextgraph` `TYPE_SPAWN_RESULT` fold arm), and answer what became of the
/// fold. This is the result-channel counterpart of [`mcpserver::emit_event`]: rebuild the
/// appended event, stamp it with the position the append returned and the run stream
/// ([`conductor::STREAM`]) the result was recorded on, and fold it through the one
/// [`contextgraph::Fold::of`] into the graph [`StoreLocation::graph`] opens under the store
/// `selection` the courier resolved, the same opener [`cmd_emit`] hands the emit core.
///
/// The result is already on the log, so nothing here fails it: a graph that owes its rebuild,
/// one whose write lock or catch-up held it past the appender's ceiling (spec 124), or a result
/// that will not serialize (unreachable for one that just serialized to append) is answered as
/// [`contextgraph::Fold::NotFolded`], with the reason, for the caller to report. A graph that
/// owes its rebuild re-derives the result from the
/// log when `rigger setup` rebuilds it, and a current graph that missed it is left with a hole in
/// its applied ledger at the result's position (a `SpawnResult` is in the live selection): setup's
/// seeding of a file not yet carrying the `caught_up` cursor reads that ledger against the log up
/// to the bound [`contextgraph::sqlite::Projector::seed_cursor`] names, and rebuilds the file for
/// a hole there it finds old (spec 124); a hole above the file's cursor, a young one the seeding
/// seeds the cursor below included, setup's catch-up folds forward from that cursor
/// ([`contextgraph::sqlite::Projector::catch_up`]).
fn fold_recorded_result(
    loc: &StoreLocation,
    selection: &StoreSelection,
    res: &spawn::SpawnResult,
    pos: rigger::eventstore::Position,
) -> contextgraph::Fold {
    match res.to_event() {
        Ok(event) => contextgraph::Fold::of(
            loc.graph(selection),
            &event.appended_at(conductor::STREAM, pos),
        ),
        Err(e) => contextgraph::Fold::NotFolded(e.to_string()),
    }
}

fn print_run_state(rs: &RunState, base: &str) {
    println!("run state:");
    for (name, u) in &rs.units {
        println!("  {:<20} {}", name, u.status.as_str());
    }
    if rs.done() {
        println!("done: every unit integrated");
    } else {
        println!("incomplete: not every unit integrated");
    }
    // The ready-to-release handoff (spec 38, criterion 3): on a DONE run, surface the run
    // branch, the release-target base, the integrated-unit count, and the exact PR command
    // the human runs to open the release PR - the same one-authority render `rigger status`
    // shows. The loop STOPS here: it surfaces the handoff, it never merges to the base.
    if let Some(rr) = rs.release_ready(RUN_BRANCH, base) {
        for line in rr.lines() {
            println!("{line}");
        }
    }
}

/// The halt line a run entry prints after its run state (spec 115): `halted: <reason>` when the
/// conductor returned a halt reason (the budget breaker's, else a plan-critique spec-defect
/// stop's), `None` on any other stop.
fn halted_line(rs: &RunState) -> Option<String> {
    rs.budget_halt
        .as_ref()
        .map(|reason| format!("halted: {reason}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What `rigger result <args>` parses to: `Ok((id, text, is_error, supersede))`, or the
    /// error it refuses with.
    fn assert_parsed_result(args: &[&str], want: Result<(&str, Option<&str>, bool, bool), &str>) {
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        let got = parse_result_args(&args)
            .map(|a| (a.id, a.text, a.is_error, a.supersede))
            .map_err(|e| e.to_string());
        let want = want
            .map(|(id, text, is_error, supersede)| {
                (
                    id.to_string(),
                    text.map(str::to_string),
                    is_error,
                    supersede,
                )
            })
            .map_err(str::to_string);
        assert_eq!(got, want, "rigger result {args:?}");
    }

    rigger::test_cases! {
        parse_result_takes_an_id_and_an_optional_output_arg: assert_parsed_result(
            &["u/implementer#0", "the diff"],
            Ok(("u/implementer#0", Some("the diff"), false, false)),
        );
        /// Just an id leaves the text unset, so `cmd_result` reads the outcome from stdin.
        parse_result_with_no_output_defers_to_stdin: assert_parsed_result(
            &["u/implementer#0"],
            Ok(("u/implementer#0", None, false, false)),
        );
        parse_result_error_flag_before_the_output: assert_parsed_result(
            &["u/adjudicator#1", "--error", "boom"],
            Ok(("u/adjudicator#1", Some("boom"), true, false)),
        );
        parse_result_error_flag_after_the_output: assert_parsed_result(
            &["u/adjudicator#1", "boom", "--error"],
            Ok(("u/adjudicator#1", Some("boom"), true, false)),
        );
        parse_result_supersede_is_a_bare_flag: assert_parsed_result(
            &["u/implementer#0", "--supersede", "corrected"],
            Ok(("u/implementer#0", Some("corrected"), false, true)),
        );
        parse_result_requires_an_id: assert_parsed_result(
            &[],
            Err("result: expected a spawn id: rigger result <id> [<output>] [--error] [--supersede]"),
        );
        parse_result_rejects_an_empty_id: assert_parsed_result(
            &[""],
            Err("result: the spawn id must not be empty"),
        );
        parse_result_rejects_a_third_positional: assert_parsed_result(
            &["id", "out", "extra"],
            Err("result: unexpected extra argument \"extra\"; usage: rigger result <id> [<output>] [--error] [--supersede]"),
        );
        parse_result_rejects_an_unknown_flag: assert_parsed_result(
            &["id", "--if-absent"],
            Err("result: unknown flag \"--if-absent\""),
        );
    }

    /// What [`build_result`] makes of `(text, is_error)`: `Ok((is_error, output, error))`, or
    /// a refusal. A blank `--error` would replay as a success, so it is refused.
    fn assert_built_result(text: &str, is_error: bool, want: Option<(bool, &str, &str)>) {
        let got = build_result("u/adjudicator#1", text, is_error)
            .ok()
            .map(|r| (r.is_error(), r.output, r.error));
        let want = want.map(|(e, o, err)| (e, o.to_string(), err.to_string()));
        assert_eq!(got, want, "build_result({text:?}, {is_error})");
    }

    rigger::test_cases! {
        build_result_records_output_as_a_success: assert_built_result("the diff", false, Some((false, "the diff", "")));
        /// An agent that finished with no final message is a valid outcome.
        build_result_keeps_an_empty_success: assert_built_result("", false, Some((false, "", "")));
        build_result_records_an_error_as_a_failure: assert_built_result("crashed", true, Some((true, "", "crashed")));
        build_result_refuses_a_blank_error: assert_built_result("   ", true, None);
        build_result_refuses_an_empty_error: assert_built_result("", true, None);
    }

    /// Spec 50, criterion 4 (opt-out): the always-on ensure is suppressed by EITHER opt-out - the
    /// environment disable OR the config `dash: off` (surfaced as `config_dash_enabled == false`) -
    /// and only proceeds when NEITHER is set. Pins that the two opt-out paths are independent (each
    /// alone suffices) and that a run with no opt-out still ensures the dash.
    #[test]
    fn dash_ensure_is_suppressed_by_either_opt_out_and_proceeds_when_neither_is_set() {
        // Neither opt-out: the ensure proceeds (the always-on default).
        assert!(
            !dash_ensure_suppressed(false, true),
            "with no env disable and the config dash ON, the ensure proceeds"
        );
        // The ENV opt-out alone suppresses it (even with the config dash ON).
        assert!(
            dash_ensure_suppressed(true, true),
            "RIGGER_NO_DASH alone suppresses the ensure"
        );
        // The CONFIG opt-out alone suppresses it (even with no env disable).
        assert!(
            dash_ensure_suppressed(false, false),
            "`dash: off` alone suppresses the ensure"
        );
        // Both set: still suppressed.
        assert!(
            dash_ensure_suppressed(true, false),
            "both opt-outs set stays suppressed"
        );
    }

    #[test]
    fn reclaim_run_scratch_removes_the_run_level_areas_and_spares_per_unit_scratch() {
        // spec 34, criterion 3: the terminal-state run teardown reclaims EXACTLY the run-level
        // shared areas - `agent-scratch`, `agent-live`, `agent-stream`, and the SHARED build cache
        // (`cargo-target` + `target` directly under the root, the driver's `CARGO_TARGET_DIR`) -
        // and NOTHING else. Per-unit worktrees (`rigger-wt-<slug>`) and per-unit build caches
        // (`cargo-target-<slug>`) are owned by their unit's own reclamation
        // (`Worktree::remove`), never this run-level teardown, so they are SPARED here even
        // though `cargo-target-<slug>` shares the build-cache prefix.
        const AREAS: [&str; 5] = [
            "agent-scratch",
            "agent-live",
            "agent-stream",
            "cargo-target",
            "target",
        ];
        let root = tempfile::tempdir().unwrap();
        let base = root.path();

        // The run-level areas the teardown OWNS.
        for area in AREAS {
            let dir = base.join(area);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("residue.bin"), [0u8; 32]).unwrap();
        }

        // Scratch the teardown must SPARE: a per-unit worktree, its per-unit build cache (whose
        // `cargo-target-` prefix must NOT be mistaken for the bare shared `cargo-target`), and an
        // unrelated file.
        let unit_wt = base.join("rigger-wt-some-unit");
        std::fs::create_dir_all(&unit_wt).unwrap();
        let unit_cache = base.join("cargo-target-some-unit");
        std::fs::create_dir_all(&unit_cache).unwrap();
        let unrelated = base.join("keep.txt");
        std::fs::write(&unrelated, b"durable").unwrap();

        reclaim_run_scratch(base.to_str().unwrap());

        for area in AREAS {
            assert!(
                !base.join(area).exists(),
                "the run teardown must reclaim the run-level {area}"
            );
        }
        assert!(
            unit_wt.exists(),
            "a per-unit worktree is owned by its unit's terminal reclamation, not the run teardown"
        );
        assert!(
            unit_cache.exists(),
            "a per-unit cargo-target-<slug> cache must be spared (prefix must not match the bare shared cache)"
        );
        assert!(unrelated.exists(), "an unrelated file must be spared");

        // Idempotent + platform-tolerant: a second call over the now-empty root is a graceful
        // no-op (the areas are already gone), never a panic or error.
        reclaim_run_scratch(base.to_str().unwrap());
    }

    #[test]
    fn reclaim_run_scratch_leaves_a_critique_runs_entries_in_every_agent_area() {
        // A critique run shares the scratch root's agent areas with the loop run and reclaims its
        // own entries on its next call, so a done run's teardown reclaims only the loop run's
        // entries: a critique still running as the run ends keeps its marker, transcript and
        // scratch, and no process rooted in them is reaped.
        const CRITIQUE: &str = "critique-0123456789abcdef";
        let root = tempfile::tempdir().unwrap();
        let base = root.path();
        let areas = [
            rigger::driver::replay::SPAWN_SCRATCH_SUBDIR,
            rigger::liveness::MARKER_SUBDIR,
            claude_code::AGENT_STREAM_SUBDIR,
        ];
        for area in areas {
            for run in ["run-1", CRITIQUE] {
                let dir = base.join(area).join(run);
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(dir.join("residue.bin"), [0u8; 32]).unwrap();
            }
        }

        reclaim_run_scratch(base.to_str().unwrap());

        for area in areas {
            let area_dir = base.join(area);
            assert_eq!(
                (
                    area_dir.join("run-1").exists(),
                    area_dir.join(CRITIQUE).join("residue.bin").exists()
                ),
                (false, true),
                "{area}: the run's entry is reclaimed and the critique's is left to its own next call"
            );
        }
    }

    #[test]
    fn reclaim_run_scratch_spares_the_shared_build_cache_while_a_build_holds_its_guard() {
        // spec 77 criterion 5 / Global Constraint 3: this run-teardown fires automatically
        // and unconditionally at every qualifying terminal state, with no operator action -
        // so it must route the shared `cargo-target` reap through the SAME guarded
        // primitive `rigger reset --build-cache` uses, never a second, unguarded
        // `remove_dir_all` that could corrupt a rigger-launched build still mid-flight.
        let root = tempfile::tempdir().unwrap();
        let base = root.path();
        let cache = base.join("cargo-target");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(cache.join("residue.bin"), [0u8; 32]).unwrap();
        let agent_scratch = base.join("agent-scratch");
        std::fs::create_dir_all(&agent_scratch).unwrap();

        let guard_path = rigger::worktree::shared_build_cache_guard_path(base.to_str().unwrap());
        let held = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&guard_path)
            .expect("open guard");
        fs2::FileExt::lock_shared(&held).expect("simulate a build's own shared hold");

        reclaim_run_scratch(base.to_str().unwrap());

        assert!(
            cache.exists() && cache.join("residue.bin").exists(),
            "a live build's shared cache must survive a run teardown that races it"
        );
        assert!(
            !agent_scratch.exists(),
            "contention on the shared cache alone must never abort the REST of the teardown"
        );
    }

    // --- Spec 105, LIVENESS: the supervisor beat ---

    /// Mint `run` on `run_log`'s run stream.
    fn mint_run(run_log: &Store, run: &str) {
        run_log
            .append(
                conductor::STREAM,
                rigger::eventstore::ExpectedRevision::Any,
                &[crate::test_support::run_started(run, &[])],
            )
            .unwrap();
    }

    /// The stream of every supervisor beat `progress` holds, oldest first.
    fn beat_streams(progress: &Store) -> Vec<String> {
        let everything = rigger::eventstore::Filter {
            stream_prefix: None,
        };
        progress
            .read_all(0, Direction::Forward, &everything)
            .unwrap()
            .into_iter()
            .filter(|event| event.type_ == rigger::progress::TYPE_SUPERVISOR_BEAT)
            .map(|event| event.stream)
            .collect()
    }

    /// Each beat re-reads the current run: it lands on the run current at that beat, again on a
    /// run that stands, and nowhere when no run has started. A row mints its run, then beats once;
    /// its list is every beat on record after that.
    #[test]
    fn a_beat_lands_on_the_run_current_at_that_beat() {
        let run_log = Store::open(":memory:").unwrap();
        let progress = Store::open(":memory:").unwrap();
        let (first, newer) = ("progress/run-a", "progress/run-b");
        for (case, minted, streams) in [
            ("no run yet", None, vec![]),
            ("a first run", Some("run-a"), vec![first]),
            ("the same run again", None, vec![first, first]),
            ("a newer run", Some("run-b"), vec![first, first, newer]),
        ] {
            if let Some(run) = minted {
                mint_run(&run_log, run);
            }
            beat(&run_log, &progress);
            assert_eq!(beat_streams(&progress), streams, "{case}");
        }
    }

    /// A run log that cannot be read names no run to beat on: the beat writes nothing and the
    /// supervisor stands.
    #[test]
    fn a_failed_read_of_the_current_run_writes_no_beat() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("events.db");
        let run_log = Store::open(db.to_str().unwrap()).unwrap();
        mint_run(&run_log, "run-a");
        crate::test_support::drop_the_log(&db);
        let progress = Store::open(":memory:").unwrap();
        beat(&run_log, &progress);
        assert_eq!(beat_streams(&progress), Vec::<String>::new());
    }

    /// A beat whose write the progress store refuses is ignored: the first tick meets another
    /// writer's lock past its busy timeout and fails, the thread stands, and the next tick lands
    /// the one beat on record. The interval outlasts what remains of the lock, and the driven work
    /// returns on that beat, so no third tick comes.
    #[test]
    fn a_failed_beat_write_is_ignored_and_the_next_tick_lands_its_beat() {
        use crate::test_support::{wait_until_for, with_graph_locked_for};

        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("progress.db");
        let progress = Store::open(db.to_str().unwrap()).unwrap();
        let run_log = Store::open(":memory:").unwrap();
        mint_run(&run_log, "run-a");
        let landed = with_graph_locked_for(&db, || {
            beating(
                &run_log,
                &progress,
                std::time::Duration::from_secs(3),
                || wait_until_for(800, || !beat_streams(&progress).is_empty()),
            )
        });
        assert!(landed, "the tick after the refused one lands its beat");
        assert_eq!(beat_streams(&progress), vec!["progress/run-a"]);
    }

    /// spec 105, RESUME BY SESSION: a start's reconciliation reaps the dir of each open launch that
    /// names one, under the project's scratch root and while that launch still stands open, so a
    /// crash ahead of the close re-reaps at the next start, then closes every open launch
    /// `interrupted` with no class. A project with no scratch root (a repo-less run) has no root to
    /// authorize a reap against, and a launch naming no dir has nothing to reap, so neither reaches
    /// the reaper.
    #[test]
    fn a_reconciliation_reaps_each_named_dir_under_the_scratch_root_ahead_of_its_close() {
        use rigger::progress::{SpawnLaunched, ENDED_INTERRUPTED};
        use std::cell::RefCell;
        const ROOT: &str = "/scratch";
        const DIR: &str = "/scratch/agent-scratch/run-a/u1";
        let launches = [
            ("run-a", "u1/implementer#0", DIR),
            ("run-b", "u2/implementer#0", ""),
        ];
        for (name, scratch, reaped) in [
            (
                "a scratch root",
                Some(ROOT),
                vec![(DIR.to_string(), ROOT.to_string(), true)],
            ),
            ("no scratch root", None, Vec::new()),
        ] {
            let progress = Store::open(":memory:").unwrap();
            let opened: Vec<SpawnLaunched> = launches
                .iter()
                .map(|&(run, spawn, dir)| {
                    let open = SpawnLaunched {
                        spawn: spawn.into(),
                        launch: 0,
                        session_id: format!("s-{run}"),
                        started: 1,
                        dir: dir.into(),
                        ..SpawnLaunched::default()
                    };
                    rigger::progress_store::record_launch(&progress, run, &open).unwrap();
                    open
                })
                .collect();
            // The fake reaper records each call with whether the launch naming its dir still
            // stands open in the store as it is called.
            let reaps = RefCell::new(Vec::new());
            let reap = |dir: &Path, root: &Path| {
                let every_run = Filter {
                    stream_prefix: Some(progress::STREAM.to_string()),
                };
                let events = progress
                    .read_all(0, Direction::Forward, &every_run)
                    .unwrap();
                let open = progress::open_launches(&events)
                    .iter()
                    .any(|(_, launch)| Path::new(&launch.dir) == dir);
                reaps.borrow_mut().push((
                    dir.display().to_string(),
                    root.display().to_string(),
                    open,
                ));
            };

            let reconciled =
                reconcile_open_launches(&progress, scratch, &reap).map_err(|e| e.to_string());

            let records = |run: &str| -> Vec<SpawnLaunched> {
                progress::read_run(&progress, run)
                    .unwrap()
                    .iter()
                    .map(|e| serde_json::from_slice(&e.data).unwrap())
                    .collect()
            };
            let ran = |open: &SpawnLaunched| {
                let closed = SpawnLaunched::closed(
                    open.spawn.clone(),
                    0,
                    open.session_id.clone(),
                    ENDED_INTERRUPTED,
                    "",
                );
                vec![open.clone(), closed]
            };
            assert_eq!(
                (
                    reconciled,
                    reaps.into_inner(),
                    records("run-a"),
                    records("run-b")
                ),
                (Ok(()), reaped, ran(&opened[0]), ran(&opened[1])),
                "{name}: each named dir is reaped while its launch stands open, then every launch \
                 closes interrupted"
            );
        }
    }
}
