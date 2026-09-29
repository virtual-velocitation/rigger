use super::*;

/// Record that THIS run's own step path just attempted a dash ensure (spec 69, round-8 fix; see
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

/// Environment opt-out for the step path's always-on dashboard: when
/// [`DASH_DISABLE_ENV`] is set (to any value) the step does NOT auto-start a run
/// dashboard. Production leaves it unset (the dash is always-on, no opt-in flag - spec
/// 19b); a headless CI or the crate's own integration tests set it so a short-lived
/// `rigger step` never spawns a real dashboard process.
const DASH_DISABLE_ENV: &str = "RIGGER_NO_DASH";

/// Env override for the PORT the step-path always-on dashboard binds. Absent (the production
/// default) resolves to [`dash::DEFAULT_PORT`] with NO free-port search, so the singleton's
/// stable fixed-address contract (spec 50, criterion 4) is unchanged. It exists for the case the
/// fixed address otherwise makes untestable and unusable: a machine where a rigger dash already
/// holds the default (the self-hosting dev box always does) or a non-rigger process owns 7420 -
/// there the ensure path needs the same port seam the manual `rigger dash --port` already has.
/// The crate's own step-path dash integration tests set it to an ephemeral loopback port so they
/// exercise the ensure path WITHOUT fighting a real machine dash on the fixed default, exactly as
/// the direct-`rigger dash` singleton test injects `free_loopback_port`. A malformed value falls
/// back to the default (a bad knob never breaks a run's observability).
const DASH_PORT_ENV: &str = "RIGGER_DASH_PORT";

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
/// in-process (`rigger run`/`serve` - a single `conductor::run` call that can drive for hours, or a
/// `rigger step` whose one gate runs longer than the idle window) from aging out of discovery
/// MID-RUN: a one-shot register alone would let a reader prune the live entry after the window.
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
/// 2): the project root, the credential-free store identity, and a fresh heartbeat, keyed so a
/// re-registration refreshes one entry in place. This is the SINGLE registration writer - every run
/// entry point (`rigger step`, and the in-process `rigger run`/`serve`/`workflow` drivers) requests
/// through it, so the machine-global registry sees every invocation that starts or advances a run,
/// not just the stepwise loop path.
///
/// Returns a [`RunRegistration`] the CALLER MUST HOLD for the life of the run (`let _reg = ...;`):
/// the initial entry is written here, synchronously, and the returned guard's background thread
/// keeps its heartbeat fresh until the guard drops. BEST-EFFORT and warn-only throughout: the
/// registry is discovery metadata whose loss is harmless (live instances repopulate it), so a
/// homeless environment or a write error degrades to an inert guard and NEVER fails the run.
#[must_use = "hold the RunRegistration for the life of the run so its heartbeat stays fresh"]
fn register_run_instance(repo: &str, selection: &StoreSelection) -> RunRegistration {
    let Some(dir) = rigger::registry::default_dir() else {
        return RunRegistration::inert(); // homeless environment: degrade to no registration
    };
    let root = if repo.is_empty() {
        cwd()
    } else {
        PathBuf::from(repo)
    };
    let inst = rigger::registry::Instance {
        project: project_identity(),
        root: root.to_string_lossy().into_owned(),
        store: registry_store_identity(selection, &root),
        heartbeat_ms: rigger::registry::now_ms(),
    };
    // The initial, synchronous registration. On failure, degrade to an inert guard rather than
    // spawn a heartbeat thread that could only fail the same way.
    if let Err(e) = rigger::registry::write(&dir, &inst) {
        eprintln!("rigger: instance registry write skipped ({e}); discovery is unaffected");
        return RunRegistration::inert();
    }
    // The heartbeat thread re-stamps `inst` every interval until the guard drops. `recv_timeout`
    // makes the sleep interruptible: each `Timeout` refreshes the entry; a `Disconnected` (the guard
    // dropped) ends the `while let` at once, so teardown never waits out a full interval.
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let interval = registry_heartbeat_interval();
    let handle = std::thread::spawn(move || {
        while let Err(std::sync::mpsc::RecvTimeoutError::Timeout) = rx.recv_timeout(interval) {
            let refreshed = rigger::registry::Instance {
                heartbeat_ms: rigger::registry::now_ms(),
                ..inst.clone()
            };
            let _ = rigger::registry::write(&dir, &refreshed); // best-effort refresh
        }
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
/// returns a loud error, so `rigger step`/`run` HALTS naming the drift instead of driving a
/// campaign whose replay semantics silently changed. The conductor's own (unpinned)
/// `ensure_started` then simply ADOPTS the run this ensured.
///
/// `base` is the resolved run-branch base to persist on a freshly-minted RunStarted (spec 38,
/// criterion 3), so `rigger status`/`rigger dash` later read the run's actual base from the
/// log rather than re-resolving without the run's `--base` flag. On an ADOPTED (resumed) run
/// it is ignored - the base its original start stamped stands.
///
/// `base_tip` is the run branch's tip commit sha this run anchored at (spec 91), persisted on
/// a freshly-minted RunStarted alongside `base` so the checkin stage's `mutation` gate can
/// later diff the whole spec against it via `$RIGGER_RUN_BASE`, never a `git merge-base` with
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
            eprintln!(
                "rigger: --rebase-definition: recorded the definition supersession \
                 ({pinned} -> {current}) on run {run}; continuing on the new definition."
            );
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

/// EXACTLY ONE ROOT (spec 89, criterion 4), `rigger step` only: refuses BEFORE any terminal
/// sweep when the store this step is about to open, the repository `git` resolved for the
/// same `cwd`, and the scratch root this step is about to sweep disagree on their owning
/// root - a three-way check, not two.
///
/// LEG ONE (`cwd` vs `repo`): `RIGGER_DIR` is opened cwd-relative (never walked up), while
/// `repo` can walk PAST a `.git`-less `cwd` to an ENCLOSING repository - the two diverge
/// exactly when `cwd` has no `.git` of its own, e.g. a test fixture nested under a scratch
/// root (u87c3, 2026-09-11): the fixture's own store held none of the real run's events,
/// `git` resolved the REAL enclosing repository, and `sweep_terminal` went on to remove every
/// live worktree of the run actually using that scratch root.
///
/// LEG TWO (`scratch_root` vs `repo`, round 2 adjudication
/// `adv-u89c4-r2-one-root-check-is-two-of-three-scratch-root-never-compared`): leg one alone
/// still lets a scratch root aimed at an unrelated real repository through unrefused.
/// `RIGGER_TMPDIR` (read unconditionally by `worktree::scratch_root_from_env`, ahead of any
/// repo-derived default) can point `scratch_root` anywhere; run from the real repository root
/// (so leg one passes) with `RIGGER_TMPDIR` aimed at some OTHER real project's own scratch
/// tree, this step's sweep would act on THAT project's real worktrees using this run's
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
/// repo-gated branch in `cmd_step`.
fn refuse_unless_one_root(
    cwd: &Path,
    repo: &str,
    scratch_root: Option<&str>,
) -> Result<(), String> {
    if repo.is_empty() {
        return Ok(());
    }
    let cwd_canon = std::fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    let repo_canon = std::fs::canonicalize(Path::new(repo)).unwrap_or_else(|_| PathBuf::from(repo));
    if cwd_canon != repo_canon {
        return Err(format!(
            "rigger step: refusing - the store this step would open and the repository git \
             resolved for this directory disagree on their root: git toplevel (and the scratch \
             root it sweeps, {scratch}) is {repo}, but the store under {RIGGER_DIR} would be \
             opened relative to the current directory {cwd} instead - a DIFFERENT root. This \
             shape arises when the current directory has no `.git` of its own (e.g. a test \
             fixture nested under a scratch root): `git rev-parse` then walks UP past it to an \
             ENCLOSING repository while the store stays right here, so this step's sweep would \
             act on that enclosing repository's real worktrees using THIS directory's own \
             (unrelated) events. Re-run from the repository root.",
            scratch = scratch_root.unwrap_or("(none)"),
            cwd = cwd_canon.display(),
        ));
    }
    if let Some(scratch) = scratch_root.map(str::trim).filter(|s| !s.is_empty()) {
        let scratch_repo = git_repo_at(Path::new(scratch));
        if !scratch_repo.is_empty() {
            let scratch_repo_canon = std::fs::canonicalize(Path::new(&scratch_repo))
                .unwrap_or_else(|_| PathBuf::from(&scratch_repo));
            if scratch_repo_canon != repo_canon {
                return Err(format!(
                    "rigger step: refusing - the scratch root this step would sweep belongs to \
                     a DIFFERENT repository than the one this step resolved: git toplevel (and \
                     the store under {RIGGER_DIR}, opened relative to the current directory \
                     {cwd}) is {repo}, but the scratch root {scratch} resolves to the \
                     repository {scratch_repo} instead - a DIFFERENT root. This shape arises \
                     when `RIGGER_TMPDIR` (or `defaults.workdir`) is pointed at another \
                     project's own scratch tree: this step's sweep would then act on THAT \
                     project's real worktrees using this run's events. Point the scratch root \
                     back under {repo}, or re-run from the repository the scratch root belongs \
                     to.",
                    cwd = cwd_canon.display(),
                    scratch_repo = scratch_repo_canon.display(),
                ));
            }
        }
    }
    Ok(())
}

/// Load the config a RUN will drive, refusing to start when a gating persona guarantees an
/// integration-gate stall (spec 18, unit 2). This is the single load seam every run entry
/// (`cmd_step`, `run_cli`, `run_workflow`) shares, so the run-start refusal cannot be present
/// at one entry and silently missing at another.
///
/// The integration gate reads a gating agent's RESULT channel for a `{"verdict":...}` line and
/// never reads emitted events (a deliberate load-bearing decision); a gating persona (a review
/// adjudicator on any tier, or a plan-critique adjudicator) that records its verdict ONLY via
/// `rigger_emit` is therefore a guaranteed stall - the gate finds no verdict, folds it as a
/// non-approval, and the unit remediates until it escalates. Rather than begin that doomed run,
/// `rigger run`/`workflow`/`step` refuse up front with the SAME deterministic fix message
/// `rigger validate` gives. The check itself has ONE authority - `config::lint_gating_verdict_lines`
/// (spec 18, unit 1) - reused here, never re-derived; this seam only wires it onto the run path.
fn load_run_config(dir: &str) -> Result<config::Config, Box<dyn std::error::Error>> {
    let cfg = config_store::load(dir)?;
    config::lint_gating_verdict_lines(&cfg)?;
    Ok(cfg)
}

pub(crate) fn cmd_run(args: &[String]) -> Res {
    let parsed = parse_run_args(args)?;
    // `--driver workflow` is the equivalent of `rigger serve`: the in-Claude-Code
    // MCP-server path. `cli` (the default) keeps the standalone subprocess path.
    match parsed.driver {
        DriverKind::Workflow => run_workflow(&parsed, "rigger run --driver workflow"),
        DriverKind::Cli => run_cli(&parsed),
    }
}

/// `rigger resume-unit <unit> [--attempts N]` (default `N`: 1) - spec 88, criterion 3
/// (ESCALATION RESUMES). An escalated unit is otherwise final: this is the one operator
/// lever that gives it another chance without replanning the whole spec. Appends
/// [`ledger::TYPE_UNIT_RESUMED`] (`{unit, attempts_granted, by: "operator"}`) to the
/// CURRENT run's stream; the ledger folds it back to a mid-remediation `Failed` unit
/// with a widened per-unit bound ([`ledger::Unit::resume_bound`]), so the next `rigger
/// step` re-parks the implementer on the unit's SAME durable branch (resume-continuity
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

    let (loc, selection) = require_store_dir()?;
    let backend = resolve_store(&selection, &loc.file("events.db"))?;
    let store = Namespaced::new(backend.as_ref(), &loc.identity());
    let (run_events, run_id) = runscope::read::read_current_run(&store, conductor::STREAM)?;
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
    store.append(
        conductor::STREAM,
        ExpectedRevision::Any,
        std::slice::from_ref(&ev),
    )?;

    println!(
        "resumed unit {unit_id:?}: {attempts_granted} attempt(s) granted (by operator) - the \
         next `rigger step` re-parks its implementer on {branch:?}"
    );
    Ok(())
}

pub(crate) fn cmd_step(args: &[String]) -> Res {
    let args = parse_step_args(args)?;
    // DISCOVERABILITY (spec 66, criterion 5, round 3): `rigger step` is the PRIMARY,
    // most-used pre-launch surface - on the native `/rigger <spec>` path `cmd_step` IS the
    // driver (see this fn's own doc comment above), so it must name the spec lint too, not
    // only the one-shot `run_cli`/`cmd_workflow` entries. Printed FIRST, before any config
    // load or base check, so the reminder survives every downstream refusal or failure path -
    // mirroring `run_cli`'s and `cmd_workflow`'s placement. Routed to STDERR, never stdout:
    // `cmd_step` prints exactly ONE line of `{wave,done}` JSON on stdout that a driver parses
    // (see this fn's doc comment and `acquire_step_lock`'s), and a stdout `println!` here
    // (mirroring the other two callers verbatim) would corrupt that single-line contract.
    // Gated on `spec_lint_reminder_should_print` (spec 66, c5 REMINDER DEDUP disposition): a
    // `rigger step` genuinely nested under a parent that already printed (its real direct
    // parent pid names it in `SPEC_LINT_REMINDER_PID_ENV`) stays silent; anything else prints.
    if let Some(spec) = &args.spec {
        if spec_lint_reminder_should_print() {
            eprintln!("{}", spec_lint_next_step(spec));
        }
    }
    // STEP RESOLVES THE MAIN WORKTREE (spec 89, criterion 4): resolved and refused-or-not
    // FIRST, before any config load, store touch or worktree mutation - a linked worktree
    // gets a clear refusal naming both trees instead of wasting a config/criteria load only
    // to fail deep inside branch setup with git's own opaque error.
    let cwd = cwd();
    let repo = resolve_main_worktree_or_refuse(&cwd, "rigger step")?;
    // Refuse a doomed run up front: a gating persona that never puts its verdict on the result
    // channel would stall the integration gate (spec 18, unit 2). This reuses unit 1's lint at
    // the run's config-load seam, before any unit is parked.
    let cfg = load_run_config(".")?;
    let criteria = load_criteria(args.spec.as_deref())?;
    std::fs::create_dir_all(RIGGER_DIR)?;

    // Captured here (moved up from the pre-round-2 placement just before the terminal sweep) so
    // `refuse_unless_one_root` immediately below can name it: the value is pure (only `repo` and
    // `cfg.workflow.defaults.workdir`, both already resolved above), so hoisting the computation
    // changes no answer it was ever going to give, only how early that answer is available. Kept
    // alive for the rest of the function - the fixpoint/terminal teardown and the definition-pin
    // HALT's own reclaim both still need it (spec 34, criterion 3).
    let scratch_root = if repo.is_empty() {
        None
    } else {
        Some(rigger::worktree::scratch_root_from_env(
            &repo,
            &cfg.workflow.defaults.workdir,
        ))
    };

    // EXACTLY ONE ROOT (spec 89, criterion 4, round 2): refuse BEFORE any GIT/worktree
    // mutation - not merely before the terminal sweep - when the store this step is about to
    // open, the repository `git` resolved for this cwd, and the scratch root this step is
    // about to sweep disagree on their root (three-way as of round 2's own adjudication; see
    // `refuse_unless_one_root`'s doc comment for the added leg). "Any GIT/worktree mutation",
    // precisely: the `std::fs::create_dir_all(RIGGER_DIR)` two lines above this comment still
    // precedes this refusal, but it only ever creates an empty local `.rigger/` under THIS
    // process's own cwd - never the enclosing repository the hazard below is about - so it is
    // not the mutation this ordering guards against. Round 1 placed this call
    // just before the sweep, AFTER the run-branch anchor block below (`ensure_run_branch`
    // creates and checks out `RUN_BRANCH` in whatever `repo` resolved to - a real mutation of
    // that repository); a nested git-less fixture whose `repo` resolves to an ENCLOSING real
    // repository would have that repository's branch switched to `rigger-run` before this
    // refusal ever fired (u87c3-adjacent regression, confirmed live via `tests/
    // step_root_resolution_periphery.rs`'s
    // `step_refuses_the_one_root_mismatch_but_must_not_have_already_mutated_the_enclosing_repos_
    // checked_out_branch`). Moved here, before `acquire_step_lock` and the anchor block, so a
    // step that is going to refuse never mutates any repository first - see
    // `refuse_unless_one_root`'s own doc comment for the full u87c3 incident this closes.
    refuse_unless_one_root(&cwd, &repo, scratch_root.as_deref())?;

    // Serialize concurrent `rigger step` invocations so the run advances ONE step at a time
    // (spec 51 relies on that invariant). A step checks out the run branch and branches unit
    // worktrees off HEAD, then integrates units and appends events (see just below); two
    // steps at once would race that shared checkout/HEAD and interleave their integrations,
    // corrupting the run. The overlap arises when a driver re-couriers a step while the
    // first's minutes-long gate still runs. Held for the whole step and released when this
    // process exits (even on crash/kill), so a dead step never wedges the run. The guard
    // binds a name so it is not dropped early.
    let _step_lock = acquire_step_lock(Path::new(RIGGER_DIR))?;

    // Anchor + check out the run branch before the conductor branches any unit worktree
    // off HEAD. Guarded on a real repo so the repo-less unit-test path is untouched. A
    // failure here aborts the step (with a clear, actionable error) rather than driving
    // the conductor on the wrong branch - isolation is a precondition, not best-effort.
    // The run branch's tip commit sha AT THIS STEP'S ANCHOR (spec 91): resolved right after
    // `ensure_run_branch` below, BEFORE the conductor ever branches a unit worktree off it or
    // advances it - so a mint further down (a `--fresh` boundary, or a new campaign inside
    // `enforce_definition_pin`) persists the tip the run genuinely started at, never a value
    // some later step's own re-anchor could shift. `""` on the repo-less path: the checkin
    // stage never runs without a real repo to diff against anyway.
    let mut base_tip = String::new();
    if !repo.is_empty() {
        // Refuse an obviously-wrong base BEFORE the run branch is anchored (spec 18, criterion
        // 7). Gating on the PLANNED anchor (a side-effect-free peek) - not on the created branch
        // - means a refused first step leaves NO wrong-base run branch behind, so the corrected
        // `--base` retry re-runs this check and re-anchors fresh instead of reusing (and thus
        // self-disarming on) the wrong-base branch.
        let planned = Worktree::planned_run_branch_setup(&repo, RUN_BRANCH, &args.base);
        // Loop-readiness gate (spec 38, criterion 2): refuse a run with no reachable base (an
        // unresolvable base AND no HEAD to fall back to) loudly rather than minting a run branch
        // that branches from nowhere.
        refuse_when_base_unreachable(&repo, "rigger step", &args.base, planned)?;
        refuse_when_base_lacks_spec_paths(&repo, "rigger step", &args.base, planned, &criteria)?;
        let setup = Worktree::ensure_run_branch(&repo, RUN_BRANCH, &args.base).map_err(|e| {
            format!(
                "rigger step: could not prepare the run branch {RUN_BRANCH:?} (base {:?}): {e}. \
                 The step did not run; resolve the git state (e.g. commit or stash a dirty tree) and retry.",
                args.base
            )
        })?;
        warn_on_run_branch_divergence("rigger step", setup, &args.base, args.base_explicit);
        base_tip = rigger::worktree::branch_tip(&repo, RUN_BRANCH).unwrap_or_default();
    }

    // Migrate a pre-spec-09 store's legacy-namespace history to the minted identity once,
    // before opening the run backend (spec 09, Gap 20). A no-op unless `.rigger/project.id`
    // was minted with an id distinct from the basename and the legacy namespace still holds
    // the history; refuses loudly if both namespaces are populated.
    migrate_local_identity()?;

    let selection = store_selection(None, None)?;
    // Register this instance in the machine-global discovery registry (spec 50, criterion 2):
    // every step "advances a run". Held (`_registration`) for the whole step so its heartbeat
    // thread keeps the entry fresh even if THIS step's in-process gate runs longer than the idle
    // window; dropped when the step returns. Best-effort and warn-only - it never blocks the step.
    let _registration = register_run_instance(&repo, &selection);
    let backend = resolve_store(&selection, &db_path("events.db"))?;
    let store = Namespaced::new(backend.as_ref(), &project_identity());

    // The definition hash this step pins / re-checks (spec 13, unit 1): the digest of the
    // on-disk workflow.yml + agent-prompt set. Computed once and used for both the `--fresh`
    // pinned boundary and the drift check below.
    let definition = definition_hash(".")?;

    // `--fresh`: begin a NEW run BEFORE this step (and before the liveness sweep reads the
    // current run), so the conductor's own `ensure_started` adopts this just-minted
    // boundary instead of the latest (possibly wedged) run. A one-shot the DRIVER passes
    // on the first step of an explicit restart; plain steps after it adopt the boundary it
    // began. The notice goes to STDERR - stdout carries only the `{wave,done}` JSON the
    // driver parses. See `runscope_store::start_fresh`.
    if args.fresh {
        // Persist the resolved run-branch base, and the launching spec path (spec 82,
        // criterion 1), on the fresh boundary (spec 38, criterion 3): `args.base` is the base
        // this step anchored the run branch on, so `rigger status`/dash name the same base
        // and derive the per-run-unique PR head name in the ready-to-release handoff.
        let run = runscope_store::start_fresh(
            &store,
            &criteria,
            &definition,
            &args.base,
            &base_tip,
            args.spec.as_deref().unwrap_or(""),
        )?;
        eprintln!("rigger step: --fresh: began a new run {run} (the prior run stays in the log)");
    }

    // The currently loaded workflow's own declared unit branches (spec 89, criterion 1, round
    // 2 fix): config, never the event log, so it stays populated even at this project's very
    // first step, before a single event has ever been recorded. Both step-start worktree
    // sweeps below narrow their own "spare a dirty candidate" exception to a unit THIS run's
    // definition actually declares - see `sweep_terminal`'s and `reclaim_orphan_scratch`'s own
    // doc comments - so a genuinely dead, unrelated branch that happens to also be dirty is
    // still reclaimed exactly as before this criterion. (`scratch_root` itself is NOT
    // recomputed here - u89c4 round 2 already hoisted that single binding above, before
    // `refuse_unless_one_root`; see its doc comment there. Both step-start sweeps below use
    // that one binding.)
    let declared_units: std::collections::HashSet<String> = cfg
        .workflow
        .stages
        .keys()
        .map(|slug| conductor::unit_branch(slug))
        .collect();

    // The maintenance half of Gap 14, made liveness-aware (spec 64, criterion 4): every step
    // starts by sweeping the scratch root's terminal worktrees (integrated units, review
    // scaffolding), so leaks from crashed or superseded step processes are reclaimed by the
    // loop itself instead of accumulating until a human notices a full disk. Placed here (after
    // the store opens and any `--fresh` boundary is settled, but still BEFORE the
    // definition-pin HALT below) so it keeps running at step start on every step, including one
    // that goes on to halt on drift - exactly as before this criterion. The merged-only git
    // ancestry rule alone is not sufficient: a PARKED unit whose attempt produced an EMPTY diff
    // has a branch tip that IS an ancestor of the run branch (trivially - it never advanced past
    // it) while the unit is still live in review, so `sweep_terminal` is handed the CURRENT
    // run's live branches (the same `current_run_units` fold `reclaim_orphan_scratch` below
    // reads - one liveness authority, not a parallel notion) and spares any of them outright,
    // even one that would otherwise pass the ancestry test.
    //
    // `live_branches_for_sweep` fails CLOSED on an unreadable stream (`None`): liveness can only
    // be UNDER- not OVER-determined, so a `store.read_stream` error skips the sweep call
    // OUTRIGHT below, never runs it with a live set silently degraded to empty (which would
    // revert to the pre-c4 ancestry-only rule this criterion exists to close). Its own doc
    // comment carries the full rationale and is where its unit test lives; the `sweep_terminal`
    // call itself stays INLINE here (not pulled into that helper) because
    // `worktree_sweep_completes_before_any_add_within_one_step` (spec 51, criterion 5) pins its
    // presence and lock->sweep->add ordering directly in `cmd_step`'s own source text.
    //
    // Spec 83, criterion 1: THE FENCE. `sweep_terminal` additionally consults the unit's
    // LATEST requested spawn (`worktree::spawn_fence`) before removing an already-merged,
    // ledger-terminal branch - a second, INDEPENDENT read of the same stream, scoped the same
    // way `current_run_units` scopes its own fold, so a straggler spawn for a unit
    // `current_run_units` already read as terminal still fences off its worktree. An
    // unreadable second read degrades to an EMPTY events slice, under which `spawn_fence`
    // reads every candidate as `NoSpawn` - the pre-spec-83 rule alone, never a NEW way to
    // block a reclaim - so the degrade costs nothing beyond forgoing this step's extra
    // protection, exactly like `live_branches_for_sweep`'s own read one line above.
    if let Some(root) = &scratch_root {
        if let Some(live_branches) =
            live_branches_for_sweep(runscope::read::read_run(&store, conductor::STREAM))
        {
            let fence_events = runscope::read::read_current_run(&store, conductor::STREAM)
                .map(|(run, _)| run)
                .unwrap_or_default();
            match rigger::worktree::sweep_terminal(
                &repo,
                root,
                RUN_BRANCH,
                &live_branches,
                &declared_units,
                &fence_events,
            ) {
                Ok(0) => {}
                Ok(n) => eprintln!("rigger step: swept {n} terminal worktree(s) from {root}"),
                Err(e) => eprintln!("rigger step: scratch sweep skipped: {e}"),
            }
        }
    }

    // Definition pinning (spec 13, unit 1): pin this run's definition (a fresh run) or enforce
    // it (a live run). A drifted live-run definition WITHOUT `--rebase-definition` HALTS here,
    // loudly and before any worktree work, so a mid-campaign prompt edit can never silently
    // change replay semantics; `--rebase-definition` records the supersession and continues.
    if let Err(e) = enforce_definition_pin(
        &store,
        &criteria,
        &definition,
        args.rebase_definition,
        &args.base,
        &base_tip,
        args.spec.as_deref().unwrap_or(""),
    ) {
        // A definition-drift HALT is a terminal state for this run process (spec 34, criterion
        // 3): reclaim the run-level shared scratch before propagating the loud halt, so a halted
        // run leaves no shared build cache or agent scratch behind - the same run-teardown a
        // clean fixpoint gets. Gated on the SAME `terminal_and_no_live_worker` predicate the
        // terminal-fixpoint teardown below uses (ONE authority, so the two sites can never
        // diverge): the run must be at a terminal state with NO live worker - an empty pending
        // frontier, no hung-but-possibly-alive spawn, AND no still-pending manual-review pause. A
        // still-in-flight worker, OR a hung spawn whose liveness fault counts as "answered" yet
        // leaves a worker the operator may still resume with `--rebase-definition`, OR a unit
        // paused awaiting a human (its persisted `ManualReview` from an EARLIER step folds into the
        // inbox this predicate reads from the full stream), is STILL ADVANCING - so its scratch is
        // never pulled out from under it (the never-delete-live-owned rail). An unreadable/malformed
        // stream reads as NOT safe, so uncertainty never reclaims. Best-effort; the halt is
        // surfaced regardless.
        if let Some(root) = &scratch_root {
            if let Ok(events) = runscope::read::read_run(&store, conductor::STREAM) {
                if terminal_and_no_live_worker(&events).unwrap_or(false) {
                    reclaim_run_scratch(root);
                }
            }
        }
        return Err(e);
    }

    let graph = open_graph(&db_path("graph.db"), &project_identity(), "step")?;
    let grounder = select_grounder(&cfg.workflow.defaults.grounder)?;
    // The store state BEFORE this step's own liveness sweep runs (spec 69, criterion 5):
    // used below to scope the sweep's marker reads to THIS run (a slug-colliding re-run
    // never reads a prior run's leftover mtime) and, since `run_id` never changes within a
    // step, reused verbatim for the hung-attention cursor path too (see `pre_hung_ids`
    // below). Read ONCE here, before the sweep mutates the log.
    // The run id is empty before the first RunStarted (the first step, where nothing is
    // in-flight to sweep and nothing has ever been surfaced anyway).
    let (pre, run_id) = runscope::read::read_current_run(&store, conductor::STREAM)?;
    // The hung-attention CROSSING BOUNDARY (spec 69, criterion 5; review u69c5 round 3,
    // cause genuine-defect): the spawn ids already surfaced on the `attention` wire as of
    // the end of the PREVIOUS `rigger step` invocation, read from the persisted cursor
    // (see `liveness::hung_cursor_path`'s own doc comment for why a fresh read taken at
    // THIS process's own start - the prior approach - can never see a fault an out-of-band
    // driver call recorded strictly between two step invocations: that write already
    // predates every read this process could take). `conductor::compute_attention`'s own
    // doc comment covers why this specific signal cannot be computed from inside
    // `conductor::run` at all, in-process or otherwise. Absent scratch (the repo-less
    // unit-test path only - guarded the same way every other scratch-dependent read in this
    // function already is) has nowhere to persist a cursor, so this reads as empty - see the
    // `newly_hung` computation below (review u69c5 round 5, cause genuine-defect) for why an
    // always-empty read here is made SAFE (never a false "newly hung" every step) rather than
    // read at face value the way it is in the scratch-present path.
    let pre_hung_ids: std::collections::BTreeSet<String> = scratch_root
        .as_deref()
        .map(|root| rigger::liveness::read_hung_cursor(root, &run_id))
        .unwrap_or_default();

    // Liveness sweep (spec 10, unit 3): BEFORE the conductor replays the frontier, classify
    // any IN-FLIGHT spawn whose per-spawn heartbeat marker went stale beyond its
    // `max_wall_clock` as an infrastructure fault (a HUNG agent) and record it on the
    // spawn's id. The conductor then re-parks that fault (charging no remediation attempt -
    // the unit's code is not at fault), and it surfaces as a halt below. Best-effort and
    // scoped to the current run; a sweep failure never blocks the step.
    if let Some(root) = &scratch_root {
        match cfg.workflow.failure_taxonomy() {
            Ok(taxonomy) => {
                match rigger::liveness::sweep(
                    &store,
                    &pre,
                    root,
                    &run_id,
                    &taxonomy,
                    std::time::SystemTime::now(),
                ) {
                    Ok(stale) => {
                        // Reclaim EACH freshly-recorded hung spawn's registered scratch the
                        // moment the sweep answers it (spec 77, criterion 2) - `sweep` just
                        // recorded its liveness fault DIRECTLY via
                        // `spawn_store::record_result_if_absent`, never through `cmd_result`, so
                        // that courier's own reclaim never runs for it; this call site is the
                        // liveness-fault half of the two-call-site/one-authority shape
                        // [`reclaim_spawn_registered_scratch`]'s doc comment describes (review
                        // u77c2b round 2/3 reject: a hung spawn's mutation-scratch used to
                        // leak until its unit reached a terminal state).
                        for s in &stale {
                            reclaim_spawn_registered_scratch(root, &run_id, &s.id);
                        }
                        if !stale.is_empty() {
                            eprintln!(
                                "rigger step: liveness swept {} hung spawn(s) (classified infra, no attempt charged): {}",
                                stale.len(),
                                stale.iter().map(|s| s.id.clone()).collect::<Vec<_>>().join(", ")
                            );
                        }
                    }
                    Err(e) => eprintln!("rigger step: liveness sweep skipped: {e}"),
                }
            }
            Err(e) => eprintln!("rigger step: liveness sweep skipped (taxonomy: {e})"),
        }
    }

    // Orphan-sweep backstop (spec 34, criterion 2): reclaim any scratch under the root that no
    // LIVE unit of the CURRENT run owns - a prior run's stranded worktree/build cache, or a
    // `cargo-target-<slug>` an agent wrote outside its assigned path (the unbounded per-agent
    // leak). Keyed on liveness ownership (the SAME `worktree_belongs_to_live` predicate the
    // `rigger validate` residue report reads), so it can never remove a worktree an in-flight
    // reviewer is reading or a cache a live unit is building, and it deliberately spares the
    // shared `agent-scratch`/`agent-live`/bare-`cargo-target` areas a running spawn may still
    // be writing into. This runs AFTER `enforce_definition_pin` above so a `--fresh` restart's
    // just-superseded prior-run scratch reads as non-live and is reclaimed; it re-runs
    // idempotently each step (an already-clean root sweeps nothing). Broader than the git-only
    // `sweep_terminal` above, which reclaims only integrated worktrees. Best-effort - a sweep
    // failure only warns and never blocks the step.
    if let Some(root) = &scratch_root {
        match runscope::read::read_run(&store, conductor::STREAM) {
            Ok(events) => {
                let run_units = current_run_units(&events);
                let removed = reclaim_orphan_scratch(&repo, root, &run_units, &declared_units);
                if removed > 0 {
                    eprintln!(
                        "rigger step: reclaimed {removed} orphaned scratch entr{} under {root}",
                        if removed == 1 { "y" } else { "ies" }
                    );
                }
            }
            Err(e) => eprintln!("rigger step: orphan sweep skipped: {e}"),
        }
    }

    // Always-on dash on the native step path, retargeted at the machine SINGLETON (spec 39,
    // criterion 1; spec 50, criterion 4): ENSURE the one machine-level dash at the fixed default
    // address is up, so the loop the driver advances through many short-lived `rigger step`
    // invocations is never invisible. The first step of a run starts it at `dash::DEFAULT_PORT`
    // (never a drifting port); every later step finds it serving and starts none (never a second
    // dash). Started DETACHED so it survives across the run's many step processes, and best-effort
    // so a start failure only warns - the run proceeds headless. Suppressed entirely by the
    // opt-out (`RIGGER_NO_DASH` OR `dash: off`): a headless/CI run then binds no port at all. The
    // config opt-out is read off the already-loaded `cfg`, not re-loaded. Placed just before the
    // conductor advances the frontier so the dash is serving while this step's gates and spawns
    // are in flight.
    ensure_run_dashboard(cfg.workflow.dash_enabled(), &store);

    let driver = ReplayDriver::new(&store);
    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &ExecRunner,
        repo,
        grounder: Some(grounder.as_ref()),
        graph: Some(&graph),
        criteria,
    };
    let rs = conductor::run(&cfg, &deps)?;

    let (events, wave_run_id) = runscope::read::read_current_run(&store, conductor::STREAM)?;
    // The printed wave is the FULL pending frontier (every parked spawn without a
    // result), so a killed or re-run step process orphans nothing and a relaunched
    // driver resumes the in-flight wave (see spawn::step_result). Scoped to the CURRENT
    // run's slice (spec 06, unit 1): a prior run's unanswered spawns sit before this
    // run's RunStarted, so they never reappear in this run's wave (Gap 11).
    let mut step = spawn::step_result(&events).map_err(|e| e.to_string())?;
    // Stamp each bounded wave item with the RESOLVED absolute path of its liveness marker
    // (spec 10, unit 3, BLOCKER-1): the thin driver frames both the worker's heartbeat
    // `touch` and its staleness watchdog around THIS path, never re-deriving a scratch root
    // of its own. Derived from the SINGLE authority `liveness::marker_path` over the same
    // resolved scratch root (`RIGGER_TMPDIR` > `defaults.workdir` > repo default) the sweep
    // above reads and this run's id - so the worker-write path is byte-identical to the
    // sweep-read path under ANY scratch config. Only a bounded spawn carries a marker.
    if let Some(root) = &scratch_root {
        for item in step.wave.iter_mut() {
            if item.max_wall_clock.is_some() {
                // A degenerate id (never a real spawn id rigger itself mints) yields no
                // marker path at all rather than a fabricated placeholder - the item simply
                // carries no liveness marker, the same as any other unbounded spawn.
                item.marker_path = rigger::liveness::marker_path(root, &wave_run_id, &item.id)
                    .map(|p| p.to_string_lossy().into_owned());
            }
        }
    }
    // Surface a spawn-budget HALT (Gap 13) distinct from convergence: the conductor sets
    // `budget_halt` from its in-process breaker when a trip left ready work unscheduled, so
    // the printed `Step` carries a halt reason (`{"...","done":true,"halted":"..."}`) the
    // thin driver stops LOUDLY on - instead of reading a starved run as a clean completion.
    //
    // Surface a WEDGED terminus (spec 19c, unit 1) distinct from a clean completion, ALONGSIDE
    // the budget halt: the set of units that escalated (exhausted remediation and went
    // terminal without integrating), taken from the conductor's projected run state - the
    // single authority for the escalated set, reusing the folded `UnitEscalated` status.
    // Omitted from the wire when empty, so a clean run's `{"wave":[],"done":true}` shape is
    // unchanged; when non-empty the driver treats a `done` fixpoint carrying it as a LOUD stop
    // (exactly as for a budget halt), so a unit that can never pass review no longer
    // masquerades as a clean "run complete". Escalation-and-continue MID-run is untouched -
    // only the driver's read of the final terminus changes, and it gates on `step.done`.
    // Stamped BEFORE the `halted` move below (which consumes `rs.budget_halt`), as it borrows
    // `rs`.
    step.escalated = rs.escalated_units();
    // Surface the push-side ATTENTION array (spec 69, criterion 5): the conductor already
    // computed it as a before/after diff of this call's own transition (see
    // `conductor::compute_attention`), so this is a plain move of the live state onto the
    // wire - exactly like `escalated` and `halted`, and like them omitted when empty so a
    // clean step's `{"wave":[],"done":true}` shape stays byte-for-byte unchanged. Rendering
    // each entry as a narrator log line is a later criterion's job (spec 69, "the driver
    // relays it"); this step only stamps the wire.
    step.attention = rs.attention;
    step.halted = rs.budget_halt;
    // Hung agents (spec 10, unit 3): any spawn whose LATEST result is a liveness fault is a
    // hung, unrecovered agent whose worker may STILL be alive and writing under the shared
    // scratch. Surfaced as a loud halt so the driver stops on a named reason instead of reading
    // a stalled wave as a clean fixpoint. A budget halt already on the channel takes precedence
    // for the surfaced REASON (it is the harder global rail), so the hung reason is only stamped
    // when no budget halt is set. (The teardown's never-delete-live guard reads the same hung set
    // through `terminal_and_no_live_worker` below, so a hung-but-alive worker is spared under any
    // halt - not just when its reason is the one surfaced here.)
    let hung = rigger::liveness::hung_spawns(&events).map_err(|e| e.to_string())?;
    if step.halted.is_none() && !hung.is_empty() {
        // Recovery: record a real result on the named spawn (last-write-wins supersedes the
        // fault), then re-drive.
        step.halted = Some(rigger::liveness::halt_reason(&hung));
    }
    // The hung-liveness half of `attention`'s `halted` signal (spec 69, criterion 5; review
    // u69c5 round 3, cause genuine-defect): computed HERE, not inside `conductor::run`, because
    // its crossing boundary is `pre_hung_ids` (the boundary computed above - the PERSISTED
    // cursor whenever scratch is available, see `liveness::hung_cursor_path`'s own doc comment
    // for why - never a fresh read taken at this process's own start) - see
    // `conductor::compute_attention`'s own doc comment for why that boundary cannot live
    // inside `run()` itself either way. A spawn hung in `hung` that
    // was NOT already hung as of `pre_hung_ids` is a genuine NEW crossing this step; one
    // already hung as of `pre_hung_ids` is a still-true restamp and does not fire again.
    // `merge_hung_attention` (pulled out for unit-testability - see its own doc comment) owns
    // the precedence-and-ordering mechanics.
    //
    // Gated on `scratch_root.is_some()` (review u69c5 round 5, cause genuine-defect, findings
    // sdet-u69c5r4-repoless-cursor-restamps-every-step /
    // adv-u69c5r4-confirm-sdet-repoless-restamp-empirically-proven): absent scratch,
    // `pre_hung_ids` above has nowhere to persist a cursor and always reads empty, while
    // `hung` (unlike `pre_hung_ids`) is derived purely from the event log and is NOT itself
    // gated on `scratch_root` - so comparing the two directly would read every step as a fresh
    // crossing and restamp `attention` forever instead of once, the opposite failure direction
    // from the crash-window gap the persisted cursor exists to close. `step.halted` just above
    // is UNCHANGED by this gate - it stays sourced from the full, ungated `hung` set, so a
    // hung spawn still halts loudly on every step regardless of scratch (this only scopes the
    // separate crossing-tracked `attention` entry). Without a persisted cursor there is no
    // boundary available at all to tell a genuine new crossing from a still-true restamp in
    // this path (every repo-less-reachable fault is recorded by a wholly separate `rigger
    // result --error` process call, which by construction always predates this step's own
    // reads - there is no in-process moment that precedes it the way the scratch-present
    // sweep's pre-sweep read does), so gating BOTH halves of the comparison to the SAME
    // scratch-presence keeps them consistent in every path: `attention` simply carries no
    // hung-liveness entry when repo-less, rather than guessing wrong in either direction.
    let newly_hung = scratch_root.is_some() && hung.iter().any(|h| !pre_hung_ids.contains(&h.id));
    step.attention = merge_hung_attention(step.attention, newly_hung, || {
        rigger::liveness::halt_reason(&hung)
    });
    // `step` is now fully finalized - nothing below this point mutates it further.
    //
    // DELIVER the step BEFORE persisting the hung-attention cursor (spec 69, criterion 5;
    // review u69c5 round 5, cause genuine-defect, finding
    // adv-u69c5r4-cursor-write-outruns-its-own-delivery): the `println!` below is `attention`'s
    // SOLE delivery channel - it is never appended to the event log (see `Step`'s own
    // `attention` field: `skip_serializing_if = "Vec::is_empty"`, live-wire-only by design) -
    // so persisting the cursor first would let a process death strictly between the two calls
    // (SIGKILL, OOM, a broken pipe to the driver) durably mark a crossing "already surfaced"
    // even though no observer ever actually saw it: the next invocation's `pre_hung_ids` would
    // already contain the id, so `newly_hung` reads false forever after - silently swallowing a
    // genuine crossing, the exact failure mode this whole mechanism exists to close. Printing
    // FIRST means a crash in that window instead costs one harmless extra restamp next step
    // (the cursor never got updated, so the unchanged crossing is detected again) - the same
    // "fail toward one extra harmless re-notification, never toward silently swallowing a
    // genuine new crossing" direction `write_hung_cursor`'s own doc comment already commits to.
    println!("{}", serde_json::to_string(&step)?);
    // Persist the hung-attention cursor (spec 69, criterion 5; review u69c5 round 3, cause
    // genuine-defect): overwrite it with THIS step's own full `hung` set - the exact set
    // `pre_hung_ids` above just diffed against - so the NEXT `rigger step` invocation (this
    // process's own next step, OR the very next one after an out-of-band driver fault) reads
    // an up-to-date boundary rather than the one this process itself started with. Best-effort;
    // a write failure only risks one extra re-stamp later, never fails this step (see
    // `liveness::write_hung_cursor`'s own doc comment). Runs AFTER the print above, never
    // before it (see that comment for why).
    if let Some(root) = &scratch_root {
        let current_hung_ids: std::collections::BTreeSet<String> =
            hung.iter().map(|h| h.id.clone()).collect();
        if let Err(e) = rigger::liveness::write_hung_cursor(root, &run_id, &current_hung_ids) {
            eprintln!("rigger step: could not persist the hung-attention cursor: {e}");
        }
    }
    // RUN TEARDOWN at a terminal run state (spec 34, criterion 3): reclaim the run's run-level
    // shared scratch - `agent-scratch` (probe repos + verification builds a worker parks under
    // <scratch-root>/agent-scratch per the driver's scratch policy), `agent-live` (per-spawn
    // liveness markers, spec 10 unit 3), and the SHARED build cache (`cargo-target`/`target`
    // directly under the root, the driver's `CARGO_TARGET_DIR` - the unbounded multi-GB leak
    // spec 34 names). These exist only to serve in-flight spawns, so once the run is terminal
    // with no spawn live they are pure residue; leaving them is how a wedged/halted run leaks
    // gigabytes of build debris (Gap 14). The orphan-sweep backstop (criterion 2) deliberately
    // SPARES these shared areas while the run steps (a live spawn may still be building into
    // them), so their reclamation is exactly this run-level teardown - fired for EVERY terminal
    // state, not just a clean fixpoint: a wedge/escalation and a budget halt reclaim too.
    //
    // Gated on the SINGLE `terminal_and_no_live_worker` predicate (the never-delete-live-owned
    // rail): the pending frontier is empty, no liveness-fault spawn may still be alive, AND no
    // manual-review pause is still pending. The SAME predicate gates the definition-drift teardown
    // above, so EVERY still-advancing condition is inherited by both sites and none can drift
    // between them. It generalizes the former clean-fixpoint-only guard (`step.done &&
    // halted.is_none()`) to also fire on a budget halt / escalation while still sparing a liveness
    // halt or a manual-review pause. Best-effort - never fails the step. `?` here can never
    // actually err: all three sub-reads (`step_result`, `hung_spawns`, `ledger::project`) already
    // succeeded above on this same `events` (the last inside `conductor::run`, which produced
    // `rs`), so the predicate is pure recomputation over an in-memory slice.
    //
    // The frontier+hung core is NECESSARY but not SUFFICIENT for run terminality: a manual-review
    // PAUSE (`autonomy: manual` on a gated stage, §4.3) emits `ManualReview` and returns its unit
    // pending WITHOUT ever parking an implementer spawn, so it leaves an EMPTY frontier and no hung
    // spawn - the core reads terminal - yet the run is manual-review-pending, i.e. NOT converged
    // and STILL ADVANCING (a human will approve+integrate it on a later step). That is exactly a
    // run this rail must SPARE. The manual-review exclusion is FOLDED INTO the shared predicate
    // (it projects the `manual_review` inbox from the scoped events), so both this terminal site
    // and the drift early-return above spare a paused run without any per-caller guard to keep in
    // sync. (A budget halt / escalation IS terminal per criterion 3 and leaves the inbox empty, so
    // those still reclaim - only a non-terminal manual-review pause is excluded.)
    if terminal_and_no_live_worker(&events)? {
        if let Some(root) = &scratch_root {
            reclaim_run_scratch(root);
        }
    }
    Ok(())
}

/// Merge the hung-liveness half of signal 2 (`halted`) into `attention` (spec 69, criterion
/// 5; review u69c5 round 3, cause genuine-defect) and restore the canonical kind order -
/// pulled out of [`cmd_step`] (not reachable through the crate API - `main.rs` is a binary)
/// purely so the PRECEDENCE-AND-ORDERING mechanics are independently unit-testable, separate
/// from the CROSSING decision (`newly_hung`, computed at the call site from `pre_hung_ids` -
/// see `conductor::compute_attention`'s own doc comment for why that decision cannot live
/// inside `conductor::run` itself).
///
/// `attention` already carries whatever `compute_attention` built (escalated /
/// budget-halted / worker-death-recurred / budget-final-tenth / stalled-frontier, in THAT
/// canonical order). Pushes ONE run-scoped `halted` entry - built lazily via `reason` only
/// when actually needed, since `liveness::halt_reason` walks the whole hung set - when
/// `newly_hung` is true AND no `halted` entry is already present (a budget halt this same
/// call takes precedence, mirroring the SAME precedence the `halted` wire field itself
/// already gives the budget breaker over the hung fallback, just above this function's call
/// site). A STABLE sort by [`ledger::attention_kind_rank`] afterward only ever needs to
/// relocate the ONE entry just appended - `compute_attention`'s own entries are already in
/// canonical order, and a stable sort never disturbs their relative order (e.g. two
/// `stalled-frontier` units stay lexical) - so the merged array is byte-identical to what
/// `compute_attention` alone would have produced had it been able to see this crossing.
fn merge_hung_attention(
    mut attention: Vec<ledger::AttentionEntry>,
    newly_hung: bool,
    reason: impl FnOnce() -> String,
) -> Vec<ledger::AttentionEntry> {
    if newly_hung && attention.iter().all(|e| e.kind != ledger::ATTENTION_HALTED) {
        attention.push(ledger::AttentionEntry::run_scoped(
            ledger::ATTENTION_HALTED,
            reason(),
        ));
        attention.sort_by_key(|e| ledger::attention_kind_rank(e.kind));
    }
    attention
}

/// The NO-STILL-ADVANCING-WORK core of the never-delete-live-owned rail as ONE predicate (spec 34,
/// criterion 3): true when the current run has NO worker that may still be alive under the shared
/// scratch AND no unit still awaiting a human. Both run-teardown sites - the definition-drift
/// early-return in [`cmd_step`] and the terminal-fixpoint teardown after `conductor::run` - gate on
/// THIS function, so every still-advancing condition is inherited by both and none can drift into a
/// divergent per-caller copy (the divergence that once let the drift path reclaim on an empty
/// frontier ALONE - first omitting the hung check, then the manual-review check).
///
/// Three conditions, all required:
/// - the pending frontier is EMPTY (`spawn::step_result(...).done`): every recorded spawn has a
///   result, so no in-flight wave and no obviously-live worker; and
/// - NO spawn is HUNG (`liveness::hung_spawns(...)` is empty): a liveness-fault result counts as
///   "answered" (so it does NOT keep the frontier non-empty) yet leaves a worker that may still
///   be alive and writing under the shared scratch - and which the operator may yet recover - so
///   its presence must still block reclamation; and
/// - NO manual-review PAUSE is pending (`ledger::project(...).manual_review` is empty): a
///   `autonomy: manual` gate (§4.3) emits a PERSISTED `ManualReview` and returns its unit pending
///   WITHOUT parking any spawn, so it leaves an empty frontier and no hung spawn - the frontier+hung
///   core alone reads terminal - yet the run is manual-review-pending, i.e. NON-terminal and STILL
///   ADVANCING (a human will approve+integrate it on a later step). That persisted pause is a
///   property of the LOG, not of whether `conductor::run` ran this step, so it is folded in HERE
///   rather than at a caller: the drift early-return runs BEFORE `conductor::run`, but it reads the
///   full stream (which already carries a prior step's `ManualReview`), so it needs the exclusion
///   too. Folding it into this shared core keeps a single authority for "no still-advancing work"
///   and closes the never-delete-live breach a per-caller guard re-opened.
///
/// Scoped to the CURRENT run only (`runscope::current_run`), so a prior run's unanswered spawns or
/// paused units never gate this run's teardown. Errs only if a malformed stored event cannot be
/// replayed; callers treat an `Err` as "not safe to reclaim" (never delete on uncertainty).
fn terminal_and_no_live_worker(events: &[Event]) -> Result<bool, String> {
    let scoped = runscope::current_run(events);
    let frontier_empty = spawn::step_result(scoped).map_err(|e| e.to_string())?.done;
    let no_hung = rigger::liveness::hung_spawns(scoped)
        .map_err(|e| e.to_string())?
        .is_empty();
    // The manual-review inbox, projected from the SAME scoped slice - the single authority for
    // which units still await a human. A non-terminal manual-review PAUSE leaves an empty frontier
    // and no hung spawn (it parks no spawn), so the frontier+hung core alone reads terminal even
    // though the run is still advancing. Folding the exclusion HERE - not at each caller - means
    // both teardown sites inherit it structurally and the guard can never diverge between them.
    let no_manual_review = ledger::project(scoped)
        .map_err(|e| e.to_string())?
        .manual_review
        .is_empty();
    Ok(frontier_empty && no_hung && no_manual_review)
}

/// Reclaim the run's run-level shared scratch at a terminal run state (spec 34, criterion 3):
/// `agent-scratch` (probe repos + verify builds a worker parks there), `agent-live` (per-spawn
/// liveness markers), and the SHARED build cache - `cargo-target` and `target` directly under
/// the scratch root, the driver's `CARGO_TARGET_DIR`, the unbounded multi-GB leak. These are the
/// run-level areas the orphan-sweep backstop deliberately spares while the run is stepping (a
/// live spawn may still be building into them); once the run is terminal and no spawn is live
/// they are pure residue, so this teardown - and only this teardown - reclaims them. The two
/// build-cache names mirror exactly what `rigger validate`'s residue report flags as a shared
/// cache (`scan_residue`), so validate-reports and step-reclaims stay in lockstep.
///
/// Each area is reaped-then-removed (spec 23): any process still rooted in it is reaped BEFORE
/// the dir is removed so nothing outlives a dir holding a now-deleted cwd. Scoped to the EXACT
/// dir removed under the resolved scratch `root` (`RIGGER_TMPDIR` > `defaults.workdir` > repo
/// default), never a hardcoded `.rigger/tmp`, so a relocated scratch root stays safe and only
/// rigger's own scratch is ever reaped. Every half is best-effort - a missing area is a graceful
/// no-op (platform-tolerant, idempotent), never an error that fails the step. Per-unit worktrees
/// and their `cargo-target-<slug>` caches are NOT this function's concern - those are reclaimed
/// when their unit goes terminal (`Worktree::remove` / `sweep_terminal` / the orphan-sweep),
/// never while a later stage of the same unit still needs them.
///
/// The bare `cargo-target`/`target` removals are the first code to delete those names directly
/// under the root (the orphan-sweep and `rigger validate` only ever touch the per-unit
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
    reap_then_remove_dir(&base.join("agent-scratch"), base);
    reap_then_remove_dir(&base.join(rigger::liveness::MARKER_SUBDIR), base);
    let _ = reclaim_shared_build_cache(&base.join(rigger::worktree::SHARED_BUILD_CACHE_NAME), base);
    reap_then_remove_dir(&base.join("target"), base);
}

/// `rigger reported <id>` - exit 0 iff spawn `<id>` already has a recorded result in this
/// project's run stream, and non-zero (a clear error) when it does not.
///
/// A read-only "has this spawn reported yet?" query - it never writes. It was originally
/// the READ half of the driver's two-process check-then-record death-report guard
/// (decision `thin-driver-death-guard`): the courier ran `rigger reported <id> || rigger
/// result <id> --error <why>` so the `--error` landed ONLY when no result existed yet,
/// because recording UNCONDITIONALLY would clobber a self-report ([`spawn::result_of`] is
/// last-write-wins) and force-fail a genuinely successful/approved unit on the next replay.
/// That read-then-write pair left a TOCTOU window (a self-report landing between the check
/// and the record was still clobbered), so the death courier now records atomically via a
/// single `rigger result <id> --if-absent --error <why>` instead (spec 05; the write path
/// is [`spawn_store::record_result_if_absent`]). This command is retained as a standalone check -
/// e.g. an operator asking whether a spawn is answered - not as the courier's guard.
///
/// Composition mirrors [`cmd_result`]: the store is RESOLVED by walking up to the owning
/// root and scoped by that root's identity (via [`require_store_dir`]), the SAME per-project
/// namespaced sqlite run stream the write half lands in - so the guard and the self-report
/// can never disagree about which store/stream is authoritative (a cwd-relative or cwd-git-
/// worktree read could see "not reported" off-root and clobber a real self-report). The
/// stream is read forward from revision 0 and projected through [`spawn::result_of`] - the
/// exact boundary and projection the replay driver uses to decide answer-vs-park, so this
/// check agrees with the conductor by construction. The namespace-scoped read and its
/// absent/unreported edges live in the testable [`result_of_at`] seam.
pub(crate) fn cmd_reported(args: &[String]) -> Res {
    let id = match args {
        [id] => id.as_str(),
        _ => return Err("reported: expected exactly one spawn id: rigger reported <id>".into()),
    };
    // Resolve the store the SAME way `cmd_result` does - walk UP to the owning root and
    // bind identity to THAT root - so the death-report guard reads the exact namespaced
    // stream a self-report landed in. Reading a cwd-relative store (or the cwd's git-
    // worktree identity) could see "not reported" off-root and clobber a real self-report
    // with an `--error` (arch-reported-result-store-asym). When no store exists up-tree,
    // nothing could have been reported: treat it as unreported (the guard proceeds), the
    // same outcome as `result_of_at`'s absent-db edge, without fabricating a store.
    let reported = match require_store_dir() {
        Ok((loc, sel)) => result_of_at(&loc.file("events.db"), &loc.identity(), id, &sel)?,
        Err(_) => None,
    };
    match reported {
        // Already answered: print a one-line summary (for the courier's log) and exit 0, so
        // the guard's `|| rigger result <id> --error` is SKIPPED and the existing result -
        // the worker's own report - stands untouched.
        Some(res) => {
            println!(
                "{} {}",
                res.id,
                if res.is_error() { "failed" } else { "ok" }
            );
            Ok(())
        }
        // No result yet: exit non-zero (a clear error) so a caller can tell the spawn is still
        // unanswered.
        None => Err(format!("reported: spawn {id:?} has no recorded result yet").into()),
    }
}

/// `rigger prompt <spawn-id>` - print the parked spawn's full prompt (persona + task)
/// on stdout. The thin driver's waves are SLIM manifests (spawn-by-reference): a
/// review-round prompt can run to hundreds of kilobytes, which cannot survive a
/// model-relayed structured output verbatim, so the worker fetches its own prompt
/// straight from the log.
///
/// A store-opening COURIER, invoked BY THE WORKER from inside its unit worktree, so it
/// resolves the store the SAME way `cmd_reported`/`cmd_result` do - walk UP to the owning
/// root and scope by that root's identity (via [`require_store_dir`] /
/// [`StoreLocation::identity`]) - reading the `proj-<repo>-run` stream the conductor parked
/// the spawn in. A cwd-relative `Store::open(&db_path("events.db"))` would instead FABRICATE
/// a fresh empty `.rigger/events.db` inside the worktree (which carries the tracked
/// `.rigger/` but never the gitignored store) and then report "no spawn request recorded"
/// for every id, stranding the worker that ran it - the exact store-opening defect spec 05
/// closes, and the reason this sibling of `cmd_reported` must not stay a parallel un-hardened
/// store-opener.
pub(crate) fn cmd_prompt(args: &[String]) -> Res {
    let id = match args {
        [id] => id.as_str(),
        _ => return Err("prompt: expected exactly one spawn id: rigger prompt <id>".into()),
    };
    let (loc, selection) = require_store_dir()?;
    let backend = resolve_store(&selection, &loc.file("events.db"))?;
    let store = Namespaced::new(backend.as_ref(), &loc.identity());
    let (events, _) = runscope::read::read_current_run(&store, conductor::STREAM)?;
    match spawn::prompt_for(&events, id).map_err(|e| e.to_string())? {
        Some(p) => {
            println!("{p}");
            Ok(())
        }
        None => Err(format!("prompt: no spawn request recorded for {id:?}").into()),
    }
}

/// `rigger scratch <spawn-id>` - print spawn `<id>`'s own rigger-assigned scratch
/// container: the exact [`spawn_scratch_path`] the per-spawn reclaim (`cmd_result`'s
/// [`reclaim_spawn_scratch`]) already reaps at the spawn's terminus (spec 34, criterion 1).
///
/// This is the fix spec 77's AGENT SCRATCH IS SPAWN-OWNED design bullet names: the driver
/// cannot pass env to a workflow-driven agent (`agent()` takes only phase/model/schema/
/// label), so a worker's own manual scratch and `CARGO_TARGET_DIR` used to be directed by
/// PROSE at an unbucketed `agent-scratch/` literal and the shared build cache - producing
/// top-level ad-hoc dirs no run/spawn owns. `rigger scratch <spawn>` lets a worker fetch
/// its own container at runtime instead, exactly the pattern [`cmd_prompt`] already
/// established (a worker fetches its own slim data from the log by spawn id).
///
/// Resolves `scratch_root`/`run_id` through the IDENTICAL precedence
/// [`reclaim_spawn_scratch`] uses (the config's `workflow.defaults.workdir`, then the live
/// run's `RunStarted` via [`runscope::current_run_id`]), so the printed path is
/// byte-identical to the one path authority the reaper targets - never a second,
/// independently-derived scratch path that could drift from it.
///
/// A store-opening COURIER, invoked BY THE WORKER (mirrors [`cmd_prompt`]/[`cmd_reported`]):
/// resolves the store by walking UP to the owning root and scoping by that root's identity,
/// so a worker running this from inside its own unit worktree still reads the real run's
/// `RunStarted` rather than fabricating a fresh, run-less store.
pub(crate) fn cmd_scratch(args: &[String]) -> Res {
    let id = match args {
        [id] => id.as_str(),
        _ => return Err("scratch: expected exactly one spawn id: rigger scratch <id>".into()),
    };
    let (loc, selection) = require_store_dir()?;
    let backend = resolve_store(&selection, &loc.file("events.db"))?;
    let store = Namespaced::new(backend.as_ref(), &loc.identity());
    let (_, run_id) = runscope::read::read_current_run(&store, conductor::STREAM)?;
    let repo = loc
        .dir
        .parent()
        .and_then(|p| p.to_str())
        .ok_or("scratch: could not resolve the project root")?;
    // `workdir` via the SAME shared, validate-independent `scratch_defaults` resolver
    // `reclaim_spawn_scratch` uses (spec 83 criterion 2, round 3) - never `config::load`,
    // which additionally requires a fully loadable `.rigger/agents/` fleet AND a passing
    // `Config::validate` just to learn this one string field, and previously left this
    // function diverging from the reaper it must stay byte-identical to (see this
    // function's own doc comment) whenever `Config::validate` failed for an unrelated
    // reason (arch-u83c3-cmd-scratch-diverges-from-reclaim-after-asymmetric-fix).
    let (workdir, _max_retries) = scratch_defaults(&loc);
    let scratch_root = rigger::worktree::scratch_root_path_from_env(repo, &workdir);
    match spawn_scratch_path(&scratch_root, &run_id, id) {
        Some(path) => {
            println!("{}", path.display());
            Ok(())
        }
        None => Err(format!("scratch: {id:?} does not name a usable scratch path").into()),
    }
}

/// The parsed flags of a `rigger step` invocation.
struct StepArgs {
    /// The spec whose Done-when criteria drive the deterministic decomposition, or
    /// None for an unconstrained step (exactly as `rigger run` uses `--spec`).
    spec: Option<String>,
    /// The ref the run branch is anchored to (`--base`, default [`DEFAULT_BASE_REF`]).
    base: String,
    /// Whether `--base` was passed explicitly (vs. the default). Used to warn only when
    /// an operator's EXPLICIT base is ignored because the run branch already exists -
    /// the steady-state default reuse is silent, an explicit-but-ignored base is not.
    base_explicit: bool,
    /// `--fresh`: begin a NEW run for the spec's criteria before this step, even when the
    /// latest run matches (which the conductor's `ensure_started` would adopt). A ONE-SHOT
    /// the DRIVER passes on the first step of an explicit restart - the evented recovery
    /// from a run wedged in a terminal state whose spec is unchanged; see
    /// [`rigger::run_store::start_fresh`]. Plain steps after it adopt the boundary it began.
    fresh: bool,
    /// `--rebase-definition` (spec 13, unit 1): on a live-run step whose on-disk definition
    /// drifted from the hash pinned at start, record the supersession and continue on the new
    /// definition instead of HALTING loudly. The operator's explicit mid-campaign-edit escape.
    rebase_definition: bool,
}

/// Parse `rigger step`'s flags: an optional `--spec <path>` (the spec whose Done-when
/// criteria drive the deterministic decomposition, exactly as `rigger run` uses it) and
/// an optional `--base <ref>` (the run-branch base, default [`DEFAULT_BASE_REF`]). Each
/// flag requires its value, and an unknown flag or a bare positional is a clear error,
/// so a typo never silently runs an unconstrained step.
fn parse_step_args(args: &[String]) -> Result<StepArgs, Box<dyn std::error::Error>> {
    let mut spec = None;
    let mut base = None;
    let mut fresh = false;
    let mut rebase_definition = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--fresh" => fresh = true,
            "--rebase-definition" => rebase_definition = true,
            "--spec" => {
                i += 1;
                spec = match args.get(i) {
                    Some(p) => Some(p.clone()),
                    None => return Err("step: --spec expects a path".into()),
                };
            }
            "--base" => {
                i += 1;
                base = match args.get(i) {
                    Some(r) => Some(r.clone()),
                    None => return Err("step: --base expects a ref".into()),
                };
            }
            flag if flag.starts_with("--") => {
                return Err(format!("step: unknown flag {flag:?}").into());
            }
            positional => {
                return Err(format!(
                    "step: unexpected positional argument {positional:?}; pass the spec via --spec <path>"
                )
                .into());
            }
        }
        i += 1;
    }
    Ok(StepArgs {
        spec,
        base_explicit: base.is_some(),
        base: base.unwrap_or_else(|| DEFAULT_BASE_REF.to_string()),
        fresh,
        rebase_definition,
    })
}

/// Warn on stderr when the run branch was anchored somewhere OTHER than the base the
/// operator asked for, so a divergence is never silent (the old behavior silently
/// no-op'd an unresolvable base and silently ignored `--base` on every run after the
/// first). `cmd` names the invoking command (e.g. `"rigger step"`, `"rigger run"`) so the
/// advisory reads true for whichever run entry anchored. Any primary output (the step's
/// `{wave,done}` JSON) still goes to stdout untouched; these are stderr advisories, not
/// errors - isolation is intact in every case, only the anchor differs.
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
        RunBranchSetup::CreatedFromHead => eprintln!(
            "{cmd}: base {base:?} did not resolve, so the run branch {RUN_BRANCH:?} was anchored \
             on the current HEAD instead (unit isolation is intact, but not anchored on {base:?}). \
             Fetch the base or pass an existing ref as --base to anchor there."
        ),
        // The run branch already exists and was reused. Reusing the default base every
        // run is the expected steady state and stays silent; only an EXPLICIT --base
        // that got ignored (because re-anchoring would orphan integrated work) is worth a
        // word, so the operator is not left thinking their re-anchor took effect.
        RunBranchSetup::Reused if base_explicit => eprintln!(
            "{cmd}: the run branch {RUN_BRANCH:?} already exists and was reused (its \
             integrated work is preserved); --base {base:?} was NOT applied. Re-anchoring an existing \
             run branch would discard integrated units; to anchor a run on {base:?}, start it on a \
             repo without {RUN_BRANCH:?} (or delete that branch first)."
        ),
        RunBranchSetup::Reused | RunBranchSetup::CreatedFromBase => {}
    }
}

/// Anchor the run branch off `base` before a run entry drives the conductor, so every
/// unit worktree branches off [`RUN_BRANCH`] and integration merges never land on the
/// operator's own branch (spec 18, criterion 6 threads `--base` here). Creates and checks
/// out [`RUN_BRANCH`] off `base` (or off HEAD when `base` does not resolve - the same
/// fallback `cmd_step` uses), reusing an existing run branch untouched. `cmd` labels the
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

/// The standalone CLI path: ground, spawn agents as `claude` subprocesses, drive
/// the DAG to integration. The store is selected by flag and wrapped in the
/// per-project namespace decorator before it is injected (§5.1.1, R9).
fn run_cli(parsed: &RunArgs) -> Res {
    // DISCOVERABILITY (spec 66, criterion 5): `rigger run <spec>` is a REAL pre-launch
    // surface that genuinely holds the spec path in production (unlike the SessionStart-only
    // `rigger prime` hook, which the installed hook always invokes with zero args - see
    // `spec_lint_next_step`'s doc comment). Printed FIRST, before any config load or base
    // check, so the reminder survives every downstream refusal or failure path, not only a
    // successful run. Gated on `spec_lint_reminder_should_print` (REMINDER DEDUP): a genuinely
    // nested `rigger run` stays silent, ambient env pollution from an unrelated tree still
    // prints.
    if let Some(spec) = &parsed.spec {
        if spec_lint_reminder_should_print() {
            println!("{}", spec_lint_next_step(spec));
        }
    }
    // STEP RESOLVES THE MAIN WORKTREE (spec 89, criterion 4): resolved and refused-or-not
    // FIRST, mirroring `cmd_step`'s own placement - see `resolve_main_worktree_or_refuse`'s
    // doc comment.
    let cwd = cwd();
    let repo = resolve_main_worktree_or_refuse(&cwd, "rigger run")?;
    // Refuse before starting if a gating persona would stall the integration gate (spec 18,
    // unit 2); `load_run_config` reuses unit 1's lint at this run's config-load seam.
    let cfg = load_run_config(".")?;
    let criteria = load_criteria(parsed.spec.as_deref())?;
    std::fs::create_dir_all(RIGGER_DIR)?;
    // Anchor + check out the run branch off `--base` (spec 18, criterion 6) BEFORE the
    // conductor branches any unit worktree off HEAD, so machine-generated units never
    // branch/merge onto the operator's own branch. `--base` threads here exactly as it does
    // for `rigger step`; the effective base is the flag, then the `RIGGER_BASE` env override
    // (how `rigger workflow` threads its `--base` through the shim), then `origin/main`.
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
        refuse_when_base_unreachable(&repo, "rigger run", &base, planned)?;
        refuse_when_base_lacks_spec_paths(&repo, "rigger run", &base, planned, &criteria)?;
        anchor_run_branch(&repo, "rigger run", &base, base_explicit)?;
        base_tip = rigger::worktree::branch_tip(&repo, RUN_BRANCH).unwrap_or_default();
    }
    // The boxed backend and its namespaced wrapper both live here, in this stack
    // frame, for the whole run: the decorator borrows the concrete store, and both
    // outlive the `conductor::run` call below.
    // Migrate a pre-spec-09 store's legacy-namespace history to the minted identity once,
    // before opening the run backend (spec 09). Local-sqlite only - the migration renames
    // streams in the local `.rigger/events.db`; a shared KurrentDB backend is out of scope.
    let selection = store_selection(parsed.store, parsed.conn.as_deref())?;
    if selection.is_sqlite() {
        migrate_local_identity()?;
    }
    // Register this instance in the machine-global discovery registry (spec 50, criterion 2).
    // `rigger run` drives the WHOLE run in-process through a single `conductor::run` below, so the
    // held guard's heartbeat thread (not a one-shot register) is what keeps the entry from aging out
    // of discovery mid-run; it is dropped when `run_cli` returns. Best-effort - it never blocks the
    // run. Registered here, off the same resolved `selection` `rigger step` uses, so a native
    // `rigger run` is as discoverable as the stepwise loop path.
    let _registration = register_run_instance(&repo, &selection);
    let backend = resolve_store(&selection, &db_path("events.db"))?;
    let store = Namespaced::new(backend.as_ref(), &project_identity());
    // `--fresh`: begin a NEW run before driving, so the conductor's own `ensure_started`
    // adopts this just-minted boundary instead of the (possibly wedged) latest run. See
    // `runscope_store::start_fresh` - the evented restart for a terminal escalation on an
    // unchanged spec. `false`: this is the standalone CLI path, so stdout is the normal
    // human-facing channel and the `--fresh` notice belongs there, unchanged.
    fresh_run_if_requested(parsed, &store, &criteria, false, &base_tip)?;
    let graph = open_graph(&db_path("graph.db"), &project_identity(), "run")?;
    // NOT YET the agent host (spec 104 criterion 2 decision d-u104-stream-defer-composition-
    // swap): `driver::claude_code::Driver` now conforms to `AgentDriver` (this criterion),
    // but flipping THIS composition root breaks the argv/stdio CONTRACT `tests/cli.rs`'s own
    // fake-`claude`-on-PATH fixtures assume (`cli::Driver`'s `-p <prompt>` plus plain-text
    // stdout) across dozens of existing, real-subprocess, end-to-end tests - empirically
    // confirmed by running them against this swap. Migrating every such fixture to the
    // stream-json protocol is a real body of work of its own, out of this criterion's blast
    // radius ("OWNS the reader and the result mapping", nothing in `tests/cli.rs`), and this
    // host cannot usefully run unattended yet regardless (no failure-class relaunch or hold -
    // criterion 5 / spec 105 - so a bare `api_retry` would end the run instead of riding it
    // out). The swap stays `cli::Driver::default()` until that migration + spec 105 land.
    let driver = cli::Driver::default();
    let grounder = select_grounder(&cfg.workflow.defaults.grounder)?;
    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &ExecRunner,
        repo,
        grounder: Some(grounder.as_ref()),
        graph: Some(&graph),
        criteria,
    };
    // Always-on dash (spec 19b, unit 1): auto-start a `rigger dash` serving this run before
    // the loop begins, so an active harness is never invisible. Held for the whole run - the
    // guard reaps the dash when this scope ends (unit 3's reaping mechanism).
    let _dash = start_run_dashboard(&store);
    let rs = conductor::run(&cfg, &deps)?;
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
                eprintln!("rigger: {line}");
            }
        }
    }
    Ok(())
}

/// Begin (or adopt) and definition-PIN the run both `run` drivers drive (spec 13, unit 1).
/// When `--fresh` is set it appends a new pinned `RunStarted` for `criteria` so the run starts
/// a clean slice even if the latest run already matches (which `ensure_started` would adopt),
/// printing the new run id. It then enforces the definition pin ([`enforce_definition_pin`]):
/// a drifted live-run definition HALTS loudly unless `--rebase-definition` records the
/// supersession and continues. A fresh or unchanged run continues silently.
///
/// `fresh_notice_to_stderr` names which stream the `--fresh` notice is safe to land on for
/// THIS caller (spec 66, criterion 5 escalation remedy round 2: adj-u66c5-escalation-remedy-
/// verdict-reject-adjacent-fresh-leak upheld adv-u66c5-escalation-remedy-fresh-println-still-
/// leaks-stdout - this notice was a bare, unconditional `println!` that corrupted
/// `run_workflow`'s stdout, the live MCP JSON-RPC transport, even after that same round taught
/// the DISCOVERABILITY reminder three lines above it to respect that invariant). `run_cli`
/// passes `false`: stdout is the normal human-facing channel there and must keep printing.
/// `run_workflow` passes `true`, mirroring the reminder's own `eprintln!` three lines above its
/// call site - ONE shared implementation, not a second parallel copy per caller.
///
/// `base_tip` is the run branch's tip commit sha the caller anchored at (spec 91), resolved via
/// [`worktree::branch_tip`] right after its own `anchor_run_branch` call - BEFORE the conductor
/// (or anything else) advances that branch - so this always names the tip AT run start, never a
/// live re-resolution. Persisted only on a mint, exactly like `base`; `""` from a repo-less path
/// leaves the checkin stage (which never runs without a real repo anyway) nothing to diff.
fn fresh_run_if_requested(
    parsed: &RunArgs,
    store: &dyn EventStore,
    criteria: &[String],
    fresh_notice_to_stderr: bool,
    base_tip: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let definition = definition_hash(".")?;
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
        let notice =
            format!("rigger: --fresh: began a new run {run} (the prior run stays in the log)");
        if fresh_notice_to_stderr {
            eprintln!("{notice}");
        } else {
            println!("{notice}");
        }
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

/// The in-Claude-Code MCP-server path (`rigger serve` / `rigger run --driver
/// workflow`): the conductor orchestrates on a background thread and this thread
/// serves the MCP bridge over stdio. The store is selected by flag and wrapped in
/// the per-project namespace decorator before it is injected into BOTH the
/// conductor and the side-car (§5.1.1, R9).
///
/// `command` is the ACTUALLY-INVOKED command line (`"rigger serve"` from [`cmd_serve`],
/// `"rigger run --driver workflow"` from [`cmd_run`]) - threaded through rather than a
/// literal `"rigger serve"` here, so [`resolve_main_worktree_or_refuse`]'s refusal names
/// the command the operator actually typed instead of always naming `rigger serve` even
/// when reached via `rigger run --driver workflow` (spec 89 criterion 4, round 2
/// adjudication `adv-u89c4-r2-serve-command-name-hardcoded-for-both-entry-points`).
fn run_workflow(parsed: &RunArgs, command: &str) -> Res {
    // DISCOVERABILITY (spec 66, criterion 5): `rigger serve <spec>` / the shim-driven
    // workflow path is a REAL pre-launch surface holding the spec path - the one the
    // /rigger workflow itself runs through, and the omission this unit was rejected for
    // twice (adj-u66c5r6-reject-run-workflow-gap, adj-u66c5-rebuild-verdict-reject-dead-
    // consumer). Printed FIRST, before any config load or base check, so the reminder
    // survives every downstream refusal path; on stderr so the shim-captured stdout
    // protocol stream stays clean. Gated on `spec_lint_reminder_should_print` (REMINDER
    // DEDUP): the shim's validated pass-through suppresses a genuinely nested surface,
    // ambient pollution from an unrelated tree still prints.
    if let Some(spec) = &parsed.spec {
        if spec_lint_reminder_should_print() {
            eprintln!("{}", spec_lint_next_step(spec));
        }
    }
    // STEP RESOLVES THE MAIN WORKTREE (spec 89, criterion 4, round 2): resolved and
    // refused-or-not FIRST, before any config load, store touch or worktree mutation -
    // mirroring `run_cli`'s and `cmd_step`'s own placement. `run_workflow` is `run_cli`'s
    // sibling `DriverKind` dispatched from the same `cmd_run` (and is `rigger serve`'s sole
    // implementation, via `cmd_serve` below) - round 1 of this criterion guarded `run_cli`
    // and the Node-shim-launching `cmd_workflow` but missed THIS entry point, which every
    // one of its repo-reading call sites called bare `git_repo()` with no refusal wired in
    // at all: a `rigger serve` (the shape the shim's driver actually spawns on the
    // automated `/rigger` path) invoked from a linked worktree drove straight into
    // `anchor_run_branch` and failed with git's own opaque "already used by worktree"
    // error, never this refusal. `repo` is resolved ONCE here and reused for the rest of
    // this function (the branch anchor, instance registration, scratch root and the
    // conductor's `Deps`) instead of a `git_repo()` re-read at each site, so there is one
    // resolution authority for the whole call, never several that could in principle
    // disagree with each other or with this guard.
    let cwd = cwd();
    let repo = resolve_main_worktree_or_refuse(&cwd, command)?;
    // Refuse before starting if a gating persona would stall the integration gate (spec 18,
    // unit 2); `load_run_config` reuses unit 1's lint at this run's config-load seam.
    let cfg = load_run_config(".")?;
    let criteria = load_criteria(parsed.spec.as_deref())?;
    std::fs::create_dir_all(RIGGER_DIR)?;
    // Anchor + check out the run branch off `--base` (spec 18, criterion 6) before the
    // conductor branches any unit worktree off HEAD, mirroring `rigger step`. `rigger
    // workflow` threads its `--base` here through the shim via the inherited `RIGGER_BASE`
    // env (the shim spawns this `rigger serve` with the inherited environment); an explicit
    // `--base` on `rigger serve` / `rigger run --driver workflow` takes precedence. Guarded
    // on a real repo, so the repo-less path is untouched.
    // The run branch's tip commit sha AT THIS ANCHOR (spec 91): resolved right after
    // `anchor_run_branch` below, threaded to `fresh_run_if_requested` so a mint persists the
    // tip the run genuinely started at. `""` on the repo-less path.
    let mut base_tip = String::new();
    if !repo.is_empty() {
        let (base, base_explicit) = resolve_run_base(
            parsed.base.as_deref(),
            std::env::var("RIGGER_BASE").ok().as_deref(),
        );
        // Refuse an obviously-wrong base BEFORE anchoring (spec 18, criterion 7), gating on
        // the side-effect-free planned anchor so no wrong-base run branch is ever created and
        // the corrected `--base` retry re-anchors fresh.
        let planned = Worktree::planned_run_branch_setup(&repo, RUN_BRANCH, &base);
        // Loop-readiness gate (spec 38, criterion 2): refuse a run with no reachable base
        // (an unresolvable base AND no HEAD to fall back to) loudly rather than minting a run
        // branch that branches from nowhere.
        refuse_when_base_unreachable(&repo, "rigger workflow", &base, planned)?;
        refuse_when_base_lacks_spec_paths(&repo, "rigger workflow", &base, planned, &criteria)?;
        anchor_run_branch(&repo, "rigger workflow", &base, base_explicit)?;
        base_tip = rigger::worktree::branch_tip(&repo, RUN_BRANCH).unwrap_or_default();
    }
    // One-time spec-09 identity migration before opening the run backend (local-sqlite only).
    let selection = store_selection(parsed.store, parsed.conn.as_deref())?;
    if selection.is_sqlite() {
        migrate_local_identity()?;
    }
    // Register this instance in the machine-global discovery registry (spec 50, criterion 2). Like
    // `rigger run`, the served conductor drives the whole run in-process (on the background thread
    // in the scope below), so the held guard's heartbeat thread keeps the entry live for the whole
    // MCP session; it is dropped when `run_workflow` returns. Reuses the `repo` this function
    // resolved once above - see that resolution's own comment for why a second `git_repo()`
    // re-read is never taken here any more. Best-effort - it never blocks.
    let _registration = register_run_instance(&repo, &selection);
    let backend = resolve_store(&selection, &db_path("events.db"))?;
    let store = Namespaced::new(backend.as_ref(), &project_identity());
    // `--fresh`: begin a NEW run before the conductor thread starts, so its `ensure_started`
    // adopts this boundary rather than the latest (possibly wedged) run. `true`: this is the
    // MCP-serving path, so the notice must land on stderr, mirroring the reminder three lines
    // above (spec 66, criterion 5 escalation remedy round 2) - stdout stays the pure MCP wire.
    fresh_run_if_requested(parsed, &store, &criteria, true, &base_tip)?;
    let graph = open_graph(&db_path("graph.db"), &project_identity(), "run")?;
    let driver = rigger::driver::workflow::Driver::new();
    let grounder = select_grounder(&cfg.workflow.defaults.grounder)?;

    // Spec 14: the SEPARATE progress store + scratch root, so the MCP `rigger_activity` tool
    // presents the live per-agent view (this run's progress joined with the frontier and the
    // liveness-marker ages rigger reads in Rust) to the shim over its existing connection -
    // the shim never touches the filesystem. Progress is always the local sqlite sibling of
    // the run store, regardless of the run store's backend.
    let prog_backend = Store::open(&db_path("progress.db"))?;
    let prog_store = Namespaced::new(&prog_backend, &project_identity());
    // Reuses the `repo` resolved once at this function's entry (see its own comment) rather
    // than a second `git_repo()` re-read.
    let scratch_root = if repo.is_empty() {
        String::new()
    } else {
        rigger::worktree::scratch_root_from_env(&repo, &cfg.workflow.defaults.workdir)
    };

    // Always-on dash (spec 19b, unit 1): auto-start a `rigger dash` serving this run for the
    // whole MCP session, so an active harness is never invisible. Held here (not inside the
    // scope) so it is reaped when `run_workflow` returns - after the session ends - by unit
    // 3's guard.
    let _dash = start_run_dashboard(&store);

    // The conductor orchestrates in the background; this thread serves the MCP
    // bridge over stdio. The shim drains spawns via rigger_next/result; closing
    // stdin ends the session.
    std::thread::scope(|s| {
        s.spawn(|| {
            let deps = Deps {
                store: &store,
                driver: &driver,
                gates: &ExecRunner,
                // Reuses the `repo` this function resolved once at entry via
                // `resolve_main_worktree_or_refuse`, rather than a second `git_repo()` re-read.
                repo: repo.clone(),
                grounder: Some(grounder.as_ref()),
                graph: Some(&graph),
                criteria,
            };
            if let Err(e) = conductor::run(&cfg, &deps) {
                eprintln!("rigger: conductor: {e}");
            }
            // Signal the run is over so an empty rigger_next reports done:true and the
            // shim exits cleanly. Set on BOTH success and error: a conductor error
            // still ends the run, and the shim must not poll forever.
            driver.finish();
        });
        // Wire the graph into the MCP server too, so a ReviewFinding (or DecisionMade)
        // an agent emits via rigger_emit folds into the graph as it lands - the
        // adversary / adjudicator, which ground afterwards, then retrieve it through
        // `graph_context` (the cross-agent memory the review tiers communicate
        // through), not via the conductor hand-threading prompts.
        let server = rigger::mcpserver::Server::new(&driver, &store, conductor::STREAM)
            .with_graph(&graph)
            .with_progress(&prog_store, &scratch_root);
        let _ = server.run(std::io::stdin().lock(), std::io::stdout().lock());
    });
    Ok(())
}

pub(crate) fn cmd_serve(args: &[String]) -> Res {
    // `rigger serve` is the equivalent of `rigger run --driver workflow`, so it
    // shares the same flag surface (the event store and its connection string) and
    // the same composition path - it just forces the workflow driver.
    let mut parsed = parse_run_args(args)?;
    parsed.driver = DriverKind::Workflow;
    run_workflow(&parsed, "rigger serve")
}

/// Parse `rigger workflow`'s arguments: an optional positional spec path and an optional
/// `--base <ref>` (the run-branch base, spec 18 criterion 6). A second positional, an
/// unknown flag, and a valueless `--base` are clear errors, so a typo never silently
/// changes what runs or which base a run anchors on.
fn parse_workflow_args(
    args: &[String],
) -> Result<(Option<String>, Option<String>), Box<dyn std::error::Error>> {
    let mut spec = None;
    let mut base = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--base" => {
                i += 1;
                base = match args.get(i) {
                    Some(r) => Some(r.clone()),
                    None => return Err("workflow: --base expects a ref".into()),
                };
            }
            flag if flag.starts_with("--") => {
                return Err(format!("workflow: unknown flag {flag:?}").into());
            }
            positional => {
                if spec.is_some() {
                    return Err(format!(
                        "workflow: expected at most one spec path, got a second {positional:?}"
                    )
                    .into());
                }
                spec = Some(positional.to_string());
            }
        }
        i += 1;
    }
    Ok((spec, base))
}

/// `rigger workflow [spec] [--base <ref>]` is the turn-key one-command activation of the
/// workflow driver: it execs the Node shim (`shim/shim.mjs`), which spawns `rigger serve`
/// (this same binary, via `RIGGER_BIN`), connects an MCP client to it, and drives the agent
/// loop via the Claude Agent SDK. The user runs ONE command instead of hand-wiring `rigger
/// serve` into an MCP host. `--base` (spec 18, criterion 6) threads to the served run's
/// branch anchor through the inherited `RIGGER_BASE` environment.
pub(crate) fn cmd_workflow(args: &[String]) -> Res {
    // `rigger workflow [spec] [--base <ref>]`: an optional spec path and the run-branch base
    // (spec 18, criterion 6). A second positional or a valueless --base is a clear error.
    let (spec, base) = parse_workflow_args(args)?;
    // DISCOVERABILITY (spec 66, criterion 5): `rigger workflow <spec>` is a REAL pre-launch
    // surface that genuinely holds the spec path in production (unlike the SessionStart-only
    // `rigger prime` hook - see `spec_lint_next_step`'s doc comment). Printed before locating
    // or launching the JS driver, so the reminder survives an un-provisioned shim or any
    // other launch failure below. Gated on `spec_lint_reminder_should_print` (REMINDER DEDUP):
    // a genuinely nested `rigger workflow` stays silent, ambient env pollution still prints.
    if let Some(spec) = &spec {
        if spec_lint_reminder_should_print() {
            println!("{}", spec_lint_next_step(spec));
        }
    }
    // STEP RESOLVES THE MAIN WORKTREE (spec 89, criterion 4): resolved and refused-or-not
    // BEFORE locating or launching the Node shim - a linked worktree is refused up front
    // instead of (at best) failing to find a per-project shim only ever provisioned in the
    // main checkout, or (at worst) driving a stray one. See
    // `resolve_main_worktree_or_refuse`'s doc comment. Empty (repo-less) resolves to "." -
    // the existing behavior every prior caller of `locate_shim` already relied on.
    let cwd = cwd();
    let repo = resolve_main_worktree_or_refuse(&cwd, "rigger workflow")?;
    let shim_root = if repo.is_empty() {
        PathBuf::from(".")
    } else {
        PathBuf::from(&repo)
    };
    let shim = locate_shim(&shim_root)?;
    // The shim spawns `rigger serve` itself; point it at THIS binary so the driver
    // and the served conductor are always the same build (no PATH ambiguity).
    let rigger_bin = std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "rigger".to_string());

    let node = std::env::var("RIGGER_NODE").unwrap_or_else(|_| "node".to_string());
    let mut cmd = subprocess::command(&node);
    cmd.arg(&shim);
    if let Some(spec) = &spec {
        cmd.arg(spec);
    }
    cmd.env("RIGGER_BIN", &rigger_bin);
    // REMINDER DEDUP, the nesting-surface half of the pid-scoped contract: this process is
    // the shim's real direct OS parent, so stamp ITS OWN pid (never a value inherited from
    // further up) - the shim (an INTERMEDIARY, spec 66 c5) validates it against its own real
    // parent id before it may re-stamp and pass it on to `rigger serve`. Set unconditionally
    // (not only when the print above actually fired) so an already-suppressed `rigger
    // workflow` still hands a genuine, verifiable link to its own child rather than a stale
    // or absent one.
    cmd.env(SPEC_LINT_REMINDER_PID_ENV, std::process::id().to_string());
    // Thread --base to the served `rigger serve` the shim spawns: the shim inherits this
    // process's environment (the same channel it uses for RIGGER_BIN), so RIGGER_BASE reaches
    // `run_workflow`'s run-branch anchor, where `resolve_run_base` reads it. Set only when the
    // operator passed --base, so the no-flag default (origin/main) is unchanged.
    if let Some(base) = &base {
        cmd.env("RIGGER_BASE", base);
    }

    let status = cmd.status().map_err(|e| {
        format!(
            "workflow: failed to launch the Node driver ({node} {shim}): {e}. \
             Is Node installed and on your PATH? Run `rigger setup` if the JS driver's \
             dependencies are not yet installed."
        )
    })?;
    if !status.success() {
        return Err(format!("workflow: the Node driver exited unsuccessfully ({status})").into());
    }
    Ok(())
}

/// Extract the spec's acceptance criteria, enforcing the loop-ready gate (§8): a
/// spec with no enumerable Done-when criteria blocks until a human adds them; no
/// spec path means an unconstrained run (empty criteria).
///
/// The ONE in-run spec-lint call site (spec 66, criterion 4 - ONE LINT AUTHORITY): every
/// real live-run entry (`rigger run` via `run_cli`, `rigger step` via `cmd_step` - the ONE
/// command the documented primary native `/rigger <spec>` workflow ever invokes - and
/// `rigger serve`/`rigger workflow` via `run_workflow`) calls this SAME function to load a
/// spec's criteria, so wiring the lint here reaches all three without a bespoke call site
/// per entry. Prints via `spec_lint_warning_lines` - the identical formatter `cmd_validate`
/// (the pre-launch, standalone `rigger validate <spec>`) uses, itself built on
/// `spec::spec_lint_advisories` (criterion 3's untouched classification) - so an operator
/// who launches straight into a run without a separate `rigger validate` pass still sees
/// the same advisories. Advisory only, exactly like the pre-launch surface: never refuses
/// the run.
fn load_criteria(spec_path: Option<&str>) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let Some(spec_path) = spec_path else {
        return Ok(Vec::new());
    };
    let text =
        std::fs::read_to_string(spec_path).map_err(|e| format!("read spec {spec_path}: {e}"))?;
    for line in spec_lint_warning_lines(spec_path, &text) {
        eprintln!("{line}");
    }
    let criteria = spec::extract_criteria(&text);
    if criteria.is_empty() {
        return Err(format!(
            "loop-ready: spec {spec_path} has no enumerable Done-when criteria (checkbox items); add them before running"
        )
        .into());
    }
    Ok(criteria)
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
/// [`dash::DEFAULT_PORT`] or the next free loopback port (so two concurrent harnesses each
/// get their OWN); its URL is printed at run start and recorded in `.rigger/`[`DASH_URL_FILE`]
/// so `rigger status` can surface it.
///
/// Returns the [`dash::ReapedChild`] guard the DRIVER holds for the whole run: dropping it
/// (on a normal finish OR an unwinding panic) reaps the dash. That guard is unit 3's reaping
/// mechanism, reused here as the single reaper - THIS unit owns only start + discoverability,
/// never stopping. Best-effort: if the dash cannot be started the run still proceeds (the
/// dash is observability, not the deliverable), so a port-starved or spawn-refused
/// environment degrades to a headless run rather than aborting one.
///
/// `store` is read once here to stamp [`DASH_ATTEMPT_FILE`] with the current run's id via
/// [`record_dash_attempt`] (spec 69, round-8 fix), mirroring [`ensure_run_dashboard`]'s own
/// stamp on the step path - by the time this runs, `fresh_run_if_requested` has already
/// ensured/minted the run, so the id is always available for a real run.
fn start_run_dashboard(store: &dyn EventStore) -> Option<dash::ReapedChild> {
    if let Ok((_, run_id)) = runscope::read::read_current_run(store, conductor::STREAM) {
        record_dash_attempt(&run_id);
    }
    match spawn_run_dashboard() {
        Ok((guard, url)) => {
            // Stderr, not stdout: in the workflow driver (`rigger serve`) stdout is the MCP
            // transport, which the run-start pointer must never corrupt.
            eprintln!("rigger dash: serving this run at {url}");
            Some(guard)
        }
        Err(e) => {
            eprintln!(
                "rigger: could not auto-start the dashboard ({e}); the run continues headless"
            );
            None
        }
    }
}

/// Pick a free port, spawn `rigger dash --port <n>` as a child of the current executable,
/// and record its URL in `.rigger/`[`DASH_URL_FILE`] for `rigger status`. The child's stdout
/// is silenced (in the MCP `rigger serve` driver the parent's stdout is the protocol
/// transport, which the dash child must never write to) and its stdin is closed; the dash
/// logs only to its own stderr. Returns the guard plus the URL for the run-start pointer.
fn spawn_run_dashboard() -> Result<(dash::ReapedChild, String), Box<dyn std::error::Error>> {
    let port = dash::free_port_from(dash::DEFAULT_PORT)?;
    let exe = std::env::current_exe()?;
    let child = subprocess::command(exe)
        .arg("dash")
        .arg("--port")
        .arg(port.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()?;
    let url = format!("http://127.0.0.1:{port}/");
    // Discoverability breadcrumb for `rigger status`; best-effort and overwritten each run.
    // `.rigger/` already exists (the driver created it before reaching here).
    let _ = std::fs::write(db_path(DASH_URL_FILE), &url);
    Ok((dash::ReapedChild::new(child), url))
}

/// The outcome of the step path's idempotent dash-start attempt (spec 39, criterion 1).
#[derive(Debug, PartialEq, Eq)]
enum DashStart {
    /// No dash was serving this project, so this step STARTED one on the given port.
    Started(u16),
    /// A dash was already serving on the given port, so this step started NONE (the
    /// idempotent no-op that makes the second and every later `step` of a run a no-op).
    AlreadyServing(u16),
    /// The best-effort start failed; the run proceeds headless (a start failure never
    /// fails the step - the dash is observability, not the deliverable). Carries the
    /// failure's diagnosis text (spec 62 round 2 fix,
    /// adv-u62c3-diagnosis-unreachable-from-step-path-auto-start): the prior unit variant
    /// silently discarded [`spawn_run_dashboard_detached`]'s `io::Error`, so an operator whose
    /// step-path auto-start hit a genuinely held port (e.g. a job-control-stopped predecessor -
    /// spec 62's own motivating Goal scenario) saw only [`ensure_run_dashboard`]'s generic
    /// headless-degrade line, never the pid, process state, or resume-or-kill remedy the
    /// HELD-PORT DIAGNOSIS (criterion 3) exists to surface - reachable, before this fix, ONLY
    /// through the manual `rigger dash` CLI arm ([`cmd_dash`]), never the step path that is
    /// spec 62's actual Goal. This is the `io::Error`'s `Display` text verbatim.
    Failed(String),
}

/// The recorded-serving predicate BOTH the step path's idempotent-start decision
/// ([`ensure_run_dashboard_at`], via [`dash::dash_start_needed`]) and `rigger status`'s
/// truthful presentation ([`cmd_status`], via [`dash::dash_status`]) verify a marker's port
/// against - ONE named symbol, not two independently duplicated literal closures, so the two
/// surfaces provably share the same probe rather than merely claiming to (arch-u69c4-parity-
/// claim-rests-on-stale-unlinked-docs-not-a-shared-symbol). A REAL network probe of the port,
/// never a bare pid-liveness check: a marker left by a self-reaped or pid-recycled dash must
/// never masquerade as still serving just because its pid happens to be alive (possibly reused
/// by an unrelated process).
fn dash_marker_serving(m: dash::DashMarker) -> bool {
    dash::dash_serving_on(m.port)
}

/// Idempotently ensure a run dashboard serves the project whose marker lives at
/// `marker_path` (spec 39, criterion 1: idempotent start on step). Reads the per-project
/// [`dash::DashMarker`]; if it names a still-serving dash (per `still_serving`), returns
/// [`DashStart::AlreadyServing`] WITHOUT spawning a second one - the marker/pid short-circuit
/// that makes the second and every later `step` of a run a no-op, never a port fight.
/// Otherwise calls `start` to spawn one, records its marker, and returns [`DashStart::Started`].
///
/// `still_serving` and `start` are INJECTED so the start-once behavior is provable without a
/// real dashboard process; the production caller ([`ensure_run_dashboard`]) passes
/// [`dash_marker_serving`] (a real port probe, not a bare pid check - see its own doc) and
/// [`spawn_run_dashboard_detached`].
fn ensure_run_dashboard_at(
    marker_path: &Path,
    still_serving: impl Fn(dash::DashMarker) -> bool,
    start: impl FnOnce() -> std::io::Result<dash::DashMarker>,
) -> DashStart {
    let existing = dash::DashMarker::read(marker_path);
    if !dash::dash_start_needed(existing, &still_serving) {
        // `dash_start_needed` only returns false when `existing` is a still-serving marker,
        // so this port is that live dash's port; the `unwrap_or` is unreachable-but-safe.
        return DashStart::AlreadyServing(existing.map(|m| m.port).unwrap_or_default());
    }
    match start() {
        Ok(marker) => {
            // Record the marker so the NEXT step of this run discovers this dash and does not
            // start a second. Best-effort: a failed write only risks a later duplicate start,
            // never a broken step.
            let _ = marker.write(marker_path);
            DashStart::Started(marker.port)
        }
        // The diagnosis lives entirely in `e`'s `Display` text (spec 62 round 2 fix): the real
        // production `start` ([`spawn_run_dashboard_detached`]) already folds the HELD-PORT
        // DIAGNOSIS in when a genuine port conflict is what failed the bind, so carrying `e`
        // forward here - rather than discarding it as the prior `Err(_) => DashStart::Failed`
        // (no payload) did - is what lets that diagnosis reach the operator at all.
        Err(e) => DashStart::Failed(e.to_string()),
    }
}

/// Raw launch of `rigger dash --port <n> --reap-on-idle` as a session-detached child (spec 44).
/// NO [`dash::ReapedChild`] guard is held, and NO claim is made about whether it bound. Split
/// out of [`spawn_run_dashboard_detached`] so the process-group detachment is provable on its
/// own: under `cargo test`, [`std::env::current_exe`] names the TEST harness binary, which never
/// recognizes `dash` as a subcommand and exits immediately without binding anything, so a test
/// that must observe a REAL detachment (not an injected stand-in) can only assert on the spawn
/// itself, never on a confirmed bind - see `spawn_run_dashboard_detached_session_detaches_the_dash`,
/// which calls this directly.
///
/// ALL THREE of the child's standard streams are closed (`stdin`/`stdout`/`stderr` ->
/// [`Stdio::null`]). This matters precisely BECAUSE it is detached and outlives the `step`:
/// an inherited stderr (or stdout) would keep the step process's pipe write-end open after
/// the step exits, so any parent capturing the step's output (the thin driver, or a plain
/// `rigger step 2>&1 | ...`) would BLOCK on EOF until the long-lived dash died. A guard-bound
/// dash can inherit stderr safely because it is reaped when the driver exits; a detached one
/// must hold no inherited descriptor. The dash's own startup errors are therefore silent -
/// acceptable for a best-effort, self-contained observability process whose logs nothing reads.
fn spawn_dash_child_process(port: u16) -> std::io::Result<(std::process::Child, u32)> {
    let exe = std::env::current_exe()?;
    let mut cmd = subprocess::command(exe);
    cmd.arg("dash")
        .arg("--port")
        .arg(port.to_string())
        // Self-reap on run-idle (spec 39, criterion 3): a DETACHED dash holds no `ReapedChild`,
        // so it must watch the run's own liveness and exit once the run completes or its heartbeat
        // goes stale - the backstop the process-bound guard provides on the `rigger run` paths.
        .arg("--reap-on-idle")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // Session-detach BEFORE spawning: put the dash in its own process group so the teardown of
    // the foreground `rigger step` command's process group does not reap it (spec 44). Without
    // this the "detached" child still shares step's group and dies the instant the step returns.
    subprocess::detach_process_group(&mut cmd);
    let child = cmd.spawn()?;
    let pid = child.id();
    Ok((child, pid))
}

/// How long [`spawn_run_dashboard_detached`] polls [`dash::dash_serving_on`] for a confirmed
/// bind before giving up (spec 62, criterion 1: marker follows bind). Generous enough to absorb
/// a real machine's config-load-then-bind startup cost while keeping a genuinely stuck start's
/// headless-degrade responsive rather than hanging the step that ensures it.
const DASH_BIND_CONFIRM_WINDOW: std::time::Duration = std::time::Duration::from_secs(5);

/// The re-poll cadence inside [`DASH_BIND_CONFIRM_WINDOW`] - the same cadence the project's
/// other real-dash-readiness polls already use (see `bind_singleton_short_circuits_on_an_
/// already_serving_rigger_dash` in `dash.rs`).
const DASH_BIND_CONFIRM_POLL: std::time::Duration = std::time::Duration::from_millis(20);

/// Wait for the just-spawned `child` to confirm a bind on `port` (spec 62, criterion 1: marker
/// follows bind, confirmed ACROSS the process boundary - not merely ordered within the parent's
/// own call). Polls [`dash::dash_serving_on`] - the SAME probe the still-serving short-circuit
/// already wraps - so "bound" means exactly what every other caller already means by it.
///
/// Returns `true` the instant the probe answers. Returns `false` the instant `child` exits
/// before that: `cmd.spawn()` succeeding proves only that the OS launched a process, never that
/// it bound, so an early exit (e.g. a held port losing the `AddrInUse` race) is a failed start,
/// never awaited out the rest of `window`. Also returns `false` once `window` elapses with
/// neither. `window` and `poll` are parameters rather than always reading the module constants
/// so a test can bound this deterministically fast against a REAL held port, without waiting
/// out the production window.
fn wait_for_dash_bind(
    port: u16,
    child: &mut std::process::Child,
    window: std::time::Duration,
    poll: std::time::Duration,
) -> bool {
    let deadline = std::time::Instant::now() + window;
    loop {
        if dash::dash_serving_on(port) {
            return true;
        }
        match child.try_wait() {
            Ok(Some(_status)) => return false, // exited before confirming - a failed start
            Ok(None) => {}                     // still running; keep polling
            Err(_) => return false,            // liveness undeterminable - treat as failed
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(poll);
    }
}

/// Wait for `child`'s bind on `port` to confirm via [`wait_for_dash_bind`], and on failure
/// compose the HELD-PORT DIAGNOSIS `Err` message - the two ALWAYS travel together (spec 62
/// round 4 fix, adj-u62c3r3-verdict-reject-child-self-attribution). Split out of
/// [`spawn_run_dashboard_detached`] so the composed wait-then-diagnose behavior is directly
/// testable against a REAL deadline timeout without needing a real `rigger dash` binary:
/// `spawned_pid` and `child` are independent parameters (production always passes
/// `child.id()` for both, since `spawn_dash_child_process` returns them as a pair, but nothing
/// here requires that - a test can bind the target port itself to stand in for "the child has
/// bound it", pass its OWN pid as `spawned_pid`, and use an unrelated real process only to
/// satisfy `child`'s `try_wait` liveness check).
///
/// The critical fix this round makes: when [`wait_for_dash_bind`] gives up, the discovered
/// holder of `port` (via [`dash::held_port_holder`]) is checked against `spawned_pid` BEFORE
/// choosing the "already in use" framing. A match means the "holder" `/proc` just found is not
/// a competing process at all - it is THIS exact spawn attempt, merely slower than `window`
/// allows (e.g. a slow machine caught right between binding the socket and
/// [`dash::dash_serving_on`] first answering true). Telling an operator that pid is "already in
/// use" by itself is self-referential and actively misleading: it reads as a competing external
/// holder blocking the port when the named process is the very one about to start serving
/// correctly (round 3's defect, empirically reproduced against a real bound child in the
/// deadline sub-case - the only sub-case where this can happen, since the early-exit sub-case
/// means `child` already exited and cannot itself be the discovered holder going forward). A
/// held port genuinely occupied by SOME OTHER process still gets the full pid/state diagnosis
/// unchanged; nothing independently confirmed still falls back to the "could not be confirmed"
/// wording unchanged from round 3 - only the self-attributed case is new.
fn wait_for_dash_bind_or_diagnose(
    port: u16,
    child: &mut std::process::Child,
    spawned_pid: u32,
    window: std::time::Duration,
    poll: std::time::Duration,
) -> Result<(), String> {
    if wait_for_dash_bind(port, child, window, poll) {
        return Ok(());
    }
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let diagnosis = match dash::held_port_holder(addr) {
        // The discovered holder IS this exact spawn attempt (matched by pid) - never frame this
        // as a competing occupant; see the doc above for why this sub-case is reachable and why
        // it must read differently from a genuine conflict.
        Some((holder_pid, _)) if holder_pid == spawned_pid => format!(
            "this dash's own spawn (pid {spawned_pid}) has not yet confirmed serving {addr} \
             within the startup window, even though something is now bound there - almost \
             certainly this exact spawn attempt, merely slower than the window allows (e.g. a \
             slow machine), not a competing process; re-checking shortly should find it already \
             up"
        ),
        Some((_, msg)) => msg,
        None => format!(
            "address {addr} could not be confirmed bound by any other process - the failure \
             may be unrelated to a held port (e.g. a permission or configuration problem, or \
             a slow machine)"
        ),
    };
    Err(format!(
        "dash on port {port} (pid {spawned_pid}) did not confirm a bind within the startup \
         window: {diagnosis}"
    ))
}

/// Spawn `rigger dash --port <n>` as a DETACHED child - NO [`dash::ReapedChild`] guard is
/// held - returning its port + pid as a [`dash::DashMarker`] the caller records. Detached is
/// deliberate: the step path that starts it returns per frontier, so a guard-bound child would
/// be reaped on that return and the very next step would find no live dash and start another.
/// Not reaping here is what lets the dash keep serving across the run's many `step`
/// invocations (spec 39). Also records the dash-url breadcrumb `rigger status` reads.
///
/// `Ok` is returned ONLY once [`wait_for_dash_bind`] confirms SOMETHING is genuinely serving
/// `port` (spec 62, criterion 1: marker follows bind). `cmd.spawn()` succeeding proves only that
/// the child process launched, never that it bound - a job-control-stopped predecessor can hold
/// the port with a socket that accepts the TCP handshake but never calls `accept()`, or the
/// child can lose an `AddrInUse` race outright and exit. Either way, the caller
/// ([`ensure_run_dashboard_at`], via its injected `start` seam) must never learn of a dash that
/// never actually came up, so a bind that is not confirmed within [`DASH_BIND_CONFIRM_WINDOW`],
/// or a child that exits before confirming, returns `Err` here - closing the race at its source
/// rather than asking `ensure_run_dashboard_at`'s already-correct `Err` arm (which already
/// writes no marker) to somehow notice on its own.
///
/// That `Err` NAMES what blocked the bind WHEN a held port is what genuinely blocked it (spec
/// 62, criterion 3 - HELD-PORT DIAGNOSIS; round 2 fix,
/// adv-u62c3-diagnosis-unreachable-from-step-path-auto-start; round 3 fix,
/// adj-u62c3r2-verdict-reject-non-addrinuse-mislabel): it folds in
/// [`dash::held_port_holder`]'s report on `port` - the SAME pid/process-
/// state/resume-or-kill diagnosis [`cmd_dash`]'s own `AddrInUse` arm already surfaces for the
/// manual `rigger dash` CLI invocation, but computed from a GATED seam rather than
/// [`dash::describe_held_port`] itself. The difference matters here specifically: `cmd_dash`
/// only ever reaches `describe_held_port` AFTER `bind_singleton` has already confirmed a
/// genuine `AddrInUse` from ITS OWN bind attempt, so a `/proc` scan that fails to attribute a
/// holder there still means "held, holder undiscoverable". THIS caller has no such upstream
/// confirmation to lean on: the bind attempt runs in the detached CHILD, whose `io::Error`
/// never reaches the parent (`stdout`/`stderr`/`stdin` stay `Stdio::null()`, spec 44 -
/// unchanged), so all `wait_for_dash_bind` giving up proves is that SOMETHING blocked the bind
/// within the startup window - equally true of a genuine held port, a permission error, a slow
/// machine, or an unrelated config problem (measured indistinguishable by exit timing alone).
/// Folding `describe_held_port`'s unconditional "already in use" wording in on that weaker
/// signal was round 2's defect, reproduced live by the adjudicator with `RIGGER_DASH_PORT=1` (a
/// real `PermissionDenied`, not `AddrInUse`) still getting told a phantom pid held the port.
/// `held_port_holder` supplies the missing confirmation itself, from the SAME
/// live `/proc` discovery keyed only on the address (not on which process asks), and resolves
/// `None` rather than a false claim when nothing can be confirmed holding `port` - in which case
/// the `Err` below falls back to a message that names the timeout without asserting a holder.
///
/// Round 4 fix (adj-u62c3r3-verdict-reject-child-self-attribution) closes a NARROWER gap round 3
/// left open: an independently-confirmed holder is not automatically a COMPETING one. On
/// `wait_for_dash_bind`'s deadline sub-case (the window elapses while `child` is still alive,
/// never the early-exit sub-case), `child` can itself already have bound `port` - it just has
/// not yet reached the point where [`dash::dash_serving_on`] answers true (a slow machine caught
/// in that narrow window). `/proc` then genuinely confirms a holder, but that holder IS `pid` -
/// this exact spawn attempt, not some other process. The actual wait-then-diagnose logic now
/// lives in [`wait_for_dash_bind_or_diagnose`], which compares the discovered holder's pid
/// against the known `pid` before choosing the "already in use" framing; see its own doc for the
/// full self-attribution gate.
///
/// The recorded marker's pid is the port's OWN reported serving pid
/// ([`dash::dash_serving_pid_on`]), never assumed to be this call's locally-spawned `child`
/// (spec 62 round 2: adv-u62c1-marker-pid-not-the-serving-pid-on-singleton-race). `wait_for_dash_
/// bind` confirming `true` proves only that SOMETHING now answers `port` as a rigger dash - never
/// that it is THIS `child`: the machine singleton binds one FIXED address (spec 50, criterion 4),
/// so a concurrent run racing to start the same singleton can win it out from under this call -
/// `child`'s own `bind_singleton` then hits `AddrInUse`, recognizes the winner via
/// [`dash::DASH_HEADER`], and exits cleanly WITHOUT ever binding anything, while the port keeps
/// answering via the winner the whole time. Asking the port itself who is really serving (rather
/// than inferring it from `child`'s own liveness/exit timing, which cannot resolve this
/// deterministically - `child` does real filesystem/config work before it ever reaches its own
/// bind attempt, so "child has not yet exited" never proves "child is the one bound") is what
/// makes the returned marker correct regardless of which side of any such race this call landed
/// on.
///
/// A [`dash::dash_serving_pid_on`] `None` here means the port's real serving pid cannot be
/// attributed - either the confirmed server vanished in the instant since `wait_for_dash_bind`,
/// or (the steady-state case) it is a genuine dash that predates [`dash::DASH_HEADER_PID`], e.g.
/// a pre-round-2 or foreign build. This has been through two rejected shapes before this one.
/// Round 2 fell back to `child`'s own locally-spawned pid (`unwrap_or(pid)`, spec 62 round 2 fix
/// point, adj-u62c1r2-verdict-reject-version-skew-fallback) - wrong, because in a lost singleton
/// race `child` never bound anything at all, so asserting its pid reproduced the exact defect
/// this whole fix exists to prevent, and the steady-state case above means this is no narrow
/// timing race but a COMMON trigger. Round 3 then refused to record anything at all in this case
/// (adj-u62c1r3-verdict-reject-idempotency-regression) - also wrong: the port genuinely IS
/// serving (confirmed by `wait_for_dash_bind` above), so writing no marker left
/// `ensure_run_dashboard_at` nothing to short-circuit on, and every LATER `step` repeated this
/// entire spawn/wait/attribute cycle forever, never reaching spec 39 criterion 1's no-op
/// invariant. This fix records the documented [`dash::UNATTRIBUTED_PID`] sentinel instead of
/// either extreme: never a value this call cannot prove, but never nothing either - the marker's
/// PORT (which `dash_start_needed`/[`dash_marker_serving`] actually probe; neither ever reads
/// this pid) is enough for the next step to recognize this dash as already serving, and it
/// leaves a marker for spec 62's sibling self-heal (u62c2) to eventually correct if the real pid
/// ever becomes attributable - where recording nothing left it nothing to correct.
fn spawn_run_dashboard_detached() -> std::io::Result<dash::DashMarker> {
    // The machine singleton binds the FIXED default address (spec 50, criterion 4) - no free-port
    // search, so the address never drifts. If a dash is already serving it, the spawned `rigger
    // dash` recognizes the singleton and exits 0 without binding a second (criterion 1's
    // `bind_singleton`), so this is safe even when a concurrent run races to start one.
    //
    // The default is overridable via [`DASH_PORT_ENV`] for exactly the case the fixed address
    // otherwise makes untestable and unusable: a machine where a rigger dash already holds the
    // default (the self-hosting dev box always does), or where a non-rigger process owns 7420.
    // Unset (the production default) resolves to [`dash::DEFAULT_PORT`] with NO free-port search,
    // so the singleton's stable-address contract is unchanged; the ensure path just gains the same
    // port seam the manual `rigger dash --port` already has, which lets the step-path dash tests
    // inject an ephemeral port and never fight a real machine dash.
    let port = dash_ensure_port();
    let (mut child, pid) = spawn_dash_child_process(port)?;
    // The wait-then-diagnose composition (including the round 4 self-attribution gate - see
    // `wait_for_dash_bind_or_diagnose`'s own doc) lives in one function precisely so the two
    // parts can never be called out of step with each other again.
    wait_for_dash_bind_or_diagnose(
        port,
        &mut child,
        pid,
        DASH_BIND_CONFIRM_WINDOW,
        DASH_BIND_CONFIRM_POLL,
    )
    .map_err(std::io::Error::other)?;
    // Detach: `child` is dropped here. A std `Child`'s Drop neither waits nor kills, so the dash
    // process keeps running after this step returns. (Contrast `dash::ReapedChild`, whose Drop
    // reaps - deliberately NOT used on the step path.) Dropping BEFORE the pid-attribution probe
    // below is deliberate too: if `child` itself is the one serving, dropping it here changes
    // nothing (Drop neither waits nor kills), and if it already lost the race and exited, there
    // is nothing left to hold onto anyway.
    //
    // A `None` here means the port cannot be attributed to a real serving pid - either the
    // confirmed server vanished in the instant since `wait_for_dash_bind`, or (the steady-state
    // case) it is a genuine dash that predates `DASH_HEADER_PID`. Neither case may fall back to
    // `pid`: `pid` is THIS call's own locally-spawned child, and in a lost singleton race that
    // child never bound anything at all, so asserting its pid would repeat the exact defect this
    // probe exists to prevent. But the port IS confirmed serving (`wait_for_dash_bind` above), so
    // refusing to record ANYTHING is not safe either (spec 62 round 3's own regression,
    // adj-u62c1r3-verdict-reject-idempotency-regression): it would leave the NEXT step with no
    // marker to short-circuit on, repeating this entire spawn/probe cycle forever. Record the
    // documented sentinel instead - never a guessed real pid, but a real marker
    // `dash_start_needed` (which never reads this field, only the port) can recognize as
    // already serving.
    let serving_pid = match dash::dash_serving_pid_on(port) {
        Some(pid) => pid,
        None => dash::UNATTRIBUTED_PID,
    };
    let url = format!("http://127.0.0.1:{port}/");
    let _ = std::fs::write(db_path(DASH_URL_FILE), &url);
    Ok(dash::DashMarker {
        port,
        pid: serving_pid,
    })
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

/// The port the step-path always-on dashboard binds: [`DASH_PORT_ENV`] when set to a valid
/// `u16`, else [`dash::DEFAULT_PORT`]. The real caller resolves the raw value from the process
/// environment; the resolution itself is [`dash_ensure_port_from`], pure over that resolved input
/// so both the override and the fixed-default fallback are provable without mutating the process
/// environment (the same "pure over resolved inputs" discipline as [`dash_ensure_suppressed`]).
fn dash_ensure_port() -> u16 {
    dash_ensure_port_from(std::env::var(DASH_PORT_ENV).ok().as_deref())
}

/// Resolve the step-path ensure port from an already-read [`DASH_PORT_ENV`] value: a valid `u16`
/// overrides, anything else (absent, empty, or malformed) falls back to [`dash::DEFAULT_PORT`] -
/// so production (env unset) gets the fixed default with no free-port search, and a bad knob never
/// breaks a run's observability.
fn dash_ensure_port_from(raw: Option<&str>) -> u16 {
    raw.and_then(|v| v.trim().parse().ok())
        .unwrap_or(dash::DEFAULT_PORT)
}

/// Ensure the machine-level SINGLETON dashboard is up for the step drive path (spec 39,
/// criterion 1; spec 50, criterion 4), unless opted out. The always-on promise retargeted at the
/// singleton: the first `step` of a run starts the one dash at the FIXED [`dash::DEFAULT_PORT`]
/// (never a drifting port); every later step finds it serving and starts none (never a second
/// dash). `config_dash_enabled` is the workflow's resolved [`config::Workflow::dash_enabled`],
/// passed in from the already-loaded config so this never re-reads it.
///
/// OPT-OUT (criterion 4): the ensure is skipped entirely when EITHER the [`DASH_DISABLE_ENV`]
/// environment disable OR the config `dash: off` is set (resolved through
/// [`dash_ensure_suppressed`]) - a headless or CI run then binds NO port at all and proceeds
/// normally. Best-effort and headless-degrading otherwise: a failed start only warns. The started
/// dash is DETACHED so it survives across the run's many short-lived `step` processes.
///
/// `store` is read once here to stamp [`DASH_ATTEMPT_FILE`] with the current run's id via
/// [`record_dash_attempt`] (spec 69, round-8 fix) - by the time this runs, `enforce_definition_pin`
/// has already ensured/minted the run for this step, so the id is always available for a real
/// run. Skipped entirely on the opt-out path above: an opted-out step attempts no dash at all, so
/// there is nothing this run to vouch for.
fn ensure_run_dashboard(config_dash_enabled: bool, store: &dyn EventStore) {
    let env_disabled = std::env::var_os(DASH_DISABLE_ENV).is_some();
    if dash_ensure_suppressed(env_disabled, config_dash_enabled) {
        return;
    }
    if let Ok((_, run_id)) = runscope::read::read_current_run(store, conductor::STREAM) {
        record_dash_attempt(&run_id);
    }
    let marker_path = std::path::PathBuf::from(db_path(DASH_MARKER_FILE));
    match ensure_run_dashboard_at(
        &marker_path,
        dash_marker_serving,
        spawn_run_dashboard_detached,
    ) {
        DashStart::Started(port) => {
            // Stderr, not stdout: in the workflow driver (`rigger serve`) stdout is the MCP
            // transport; in the step driver stdout carries only the `{wave,done}` JSON.
            eprintln!("rigger dash: serving this run at http://127.0.0.1:{port}/");
        }
        // A dash is already serving the singleton address - the idempotent no-op, nothing to
        // announce (this run started it earlier, a prior run left it up, or another project did).
        DashStart::AlreadyServing(_) => {}
        // The diagnosis (spec 62, criterion 3 - HELD-PORT DIAGNOSIS, round 2 fix,
        // adv-u62c3-diagnosis-unreachable-from-step-path-auto-start) travels alongside the
        // generic headless-degrade line rather than replacing it: the generic line is the
        // stable, always-present sentence every start failure gets; `msg` is whatever detail
        // `spawn_run_dashboard_detached` could establish - a job-control-stopped predecessor's
        // pid/state/remedy for a genuine port conflict (the step-path scenario this fix closes),
        // or the bare timeout wording when nothing more specific could be determined.
        DashStart::Failed(msg) => {
            eprintln!(
                "rigger: could not auto-start the dashboard; the run continues headless: {msg}"
            );
        }
    }
}

/// A parsed `rigger result` invocation (see [`cmd_result`]): the spawn `id`, the
/// optional outcome `text` (`None` means "read it from stdin"), whether `--error`
/// marks it a failure, whether `--if-absent` makes the record conditional, and the
/// optional `--meta` courier bookkeeping.
struct ResultArgs {
    id: String,
    text: Option<String>,
    is_error: bool,
    if_absent: bool,
    meta: Option<serde_json::Value>,
}

/// Parse `rigger result <id> [<output>] [--error] [--if-absent] [--meta '<json>']`.
///
/// `<id>` is the required deterministic spawn id (`{unit}/{role}#{attempt}`). The
/// outcome payload is an OPTIONAL second positional; when omitted, [`cmd_result`]
/// reads it from stdin (spec 04: "record a spawn's outcome (stdin or arg)"). `--error`
/// is a bare flag that turns the payload into the failure message rather than the
/// agent's output. `--if-absent` is a bare flag that makes the record CONDITIONAL: the
/// outcome is written only when the spawn has no result yet, atomically and without
/// clobbering an existing one (the thin driver's death courier uses it - spec 05).
/// `--meta` takes a JSON OBJECT (mirroring `rigger emit`'s payload contract) carrying
/// courier bookkeeping (e.g. the resolved model id, spec 05). Unknown flags, a
/// missing/empty id, a third positional, and a non-object/invalid `--meta` are all
/// rejected with a clear message.
fn parse_result_args(args: &[String]) -> Result<ResultArgs, Box<dyn std::error::Error>> {
    let mut id: Option<String> = None;
    let mut text: Option<String> = None;
    let mut is_error = false;
    let mut if_absent = false;
    let mut meta: Option<serde_json::Value> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--error" => is_error = true,
            "--if-absent" => if_absent = true,
            "--meta" => {
                let raw = args.get(i + 1).ok_or(
                    "result: --meta needs a JSON object: rigger result <id> --meta '<json>'",
                )?;
                let value: serde_json::Value = serde_json::from_str(raw)
                    .map_err(|e| format!("result: --meta is not valid JSON: {e}"))?;
                if !value.is_object() {
                    return Err(format!(
                        "result: --meta must be a JSON object, got {}",
                        json_type_name(&value)
                    )
                    .into());
                }
                meta = Some(value);
                i += 1;
            }
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
                        "result: unexpected extra argument {positional:?}; usage: rigger result <id> [<output>] [--error] [--meta '<json>']"
                    )
                    .into());
                }
            }
        }
        i += 1;
    }
    let id = id.ok_or(
        "result: expected a spawn id: rigger result <id> [<output>] [--error] [--meta '<json>']",
    )?;
    if id.is_empty() {
        return Err("result: the spawn id must not be empty".into());
    }
    Ok(ResultArgs {
        id,
        text,
        is_error,
        if_absent,
        meta,
    })
}

/// Build the [`spawn::SpawnResult`] a `rigger result` invocation records, from its
/// parsed pieces and the already-resolved outcome `text` (positional arg or stdin).
///
/// Split from [`cmd_result`] (which does the stdin + store I/O) so the outcome-shaping
/// rules are a pure, unit-testable function. `--error` needs a NON-EMPTY message: a
/// blank error would leave [`spawn::SpawnResult::is_error`] false, so the replay driver
/// would answer the spawn AS a success and silently swallow the failure the courier
/// meant to record. A success may carry empty output (an agent that finished with no
/// final message is a valid outcome).
fn build_result(
    id: &str,
    text: &str,
    is_error: bool,
    meta: Option<serde_json::Value>,
) -> Result<spawn::SpawnResult, Box<dyn std::error::Error>> {
    let mut res = if is_error {
        if text.trim().is_empty() {
            return Err(format!(
                "result: --error for {id:?} needs a non-empty message (a blank error would replay as a success)"
            )
            .into());
        }
        spawn::SpawnResult::failed(id, text)
    } else {
        spawn::SpawnResult::ok(id, text)
    };
    if let Some(m) = meta {
        res = res.with_meta(m);
    }
    Ok(res)
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

/// `rigger result <id> [<output>] [--error] [--if-absent] [--meta '<json>']` - record a
/// parked spawn's OUTCOME to the run log, so the conductor's replay driver answers that
/// spawn from the log instead of re-parking it and the next `rigger step` / `rigger run`
/// advances past it (spec 04). The courier that ran the parked agent reports its final
/// message as `<output>` (or on stdin); a worker that died is reported with `--error
/// <message>`; `--meta` attaches optional bookkeeping (e.g. the resolved model id).
///
/// `--if-absent` makes the write CONDITIONAL and atomic: the outcome is recorded only
/// when the spawn has no result yet, and an already-recorded result is left UNTOUCHED
/// (still exit 0). The thin driver's death courier uses it to record a died-worker
/// failure without clobbering a self-report that landed first - one atomic operation
/// closing the TOCTOU window the old two-process `rigger reported <id> || rigger result
/// <id> --error` guard left open (spec 05). See [`spawn_store::record_result_if_absent`].
///
/// The [`spawn::SpawnResult`] is appended to the SAME per-project [`Namespaced`] `run`
/// stream the conductor drives, so the write lands exactly where the replay driver reads.
/// A recorded failure replays AS a failure - the conductor remediates it just as it would
/// a live one. The store is RESOLVED by walking up to the project's existing `.rigger`
/// (refusing to fabricate one in the wrong cwd, spec 05 - see [`require_store_dir`]); and
/// before recording, a single pre-write read of the stream prints stderr advisories for
/// an ORPHAN id (no matching spawn request) or for SUPERSEDING an existing result (see
/// [`result_advisories`]).
pub(crate) fn cmd_result(args: &[String]) -> Res {
    let parsed = parse_result_args(args)?;
    // The outcome text comes from the positional arg when given, else stdin. Resolving
    // it here keeps `build_result` a pure function of already-resolved pieces.
    let text = match parsed.text {
        Some(t) => t,
        None => read_outcome_from_stdin()?,
    };
    let res = build_result(&parsed.id, &text, parsed.is_error, parsed.meta)?;

    // Resolve the EXISTING store (walk up; refuse if none) rather than fabricating one
    // in the wrong cwd, scoped by the RESOLVED root's identity: a courier run from a unit
    // worktree would otherwise record into a fresh dead store (no store) or misfile under
    // the worktree's own namespace (walked-up store) while the real spawn stays parked
    // forever - both fixed here (see [`require_store_dir`] / [`StoreLocation::identity`]).
    let (loc, selection) = require_store_dir()?;
    // Spec 62 (couriers count as activity): re-stamp this project's registry heartbeat before
    // the real work below, so the instance stays discoverable even if this result is the only
    // traffic in the run for a while. Best-effort/warn-only; never fails the result.
    refresh_registry_entry(&loc, &selection);
    let backend = resolve_store(&selection, &loc.file("events.db"))?;
    let store = Namespaced::new(backend.as_ref(), &loc.identity());

    // One cheap pre-write read of the run stream, to advise (on stderr) about an orphan
    // id or about superseding an existing result BEFORE the append. Advisory only: the
    // record still lands, since pre-recording and deliberate re-recording are both
    // legitimate (see [`result_advisories`]). Weave with unit-10: under `--if-absent`
    // nothing can supersede (the CAS refuses), so the supersede note is suppressed -
    // the "left it untouched" line below reports that case honestly.
    let (prior, _) = runscope::read::read_current_run(&store, conductor::STREAM)?;
    for note in result_advisories(&prior, &res.id, !parsed.if_absent) {
        eprintln!("{note}");
    }

    let kind = if res.is_error() {
        "error result"
    } else {
        "result"
    };
    // The position an append actually landed at, or `None` when `--if-absent` was a no-op
    // (a result already stood, so a prior `rigger result` already folded it - see the fold
    // below). Only a real append is folded into the graph.
    let recorded = if parsed.if_absent {
        // Conditional atomic record: write only if the spawn is still unanswered, never
        // overwriting an existing result. A no-op (a result already stood) is a success,
        // so the courier's `|| ...`-free single command always exits 0.
        match spawn_store::record_result_if_absent(&store, &res)? {
            Some(pos) => {
                println!("recorded {kind} for {} (position {pos})", res.id);
                Some(pos)
            }
            None => {
                println!(
                    "{} already has a result; --if-absent left it untouched",
                    res.id
                );
                None
            }
        }
    } else {
        let pos = spawn_store::record_result(&store, &res)?;
        println!("recorded {kind} for {} (position {pos})", res.id);
        Some(pos)
    };

    // Disposition-expiry (spec 25, criterion 1): fold the just-recorded result into this run's
    // context graph through the same fold outcome `rigger emit` reports (see [`cmd_emit`] /
    // [`mcpserver::emit_event`]). The adjudicator's recorded `SpawnResult` is the ONLY place a
    // review's findings are disposed: the `TYPE_SPAWN_RESULT` fold arm reads its verdict line's
    // `discarded` ids (through the single [`spawn::SpawnResult::adjudication`] authority) and
    // invalidates those findings' graph edges, so grounding stops surfacing them. Without this
    // fold the arm is inert in production - the courier appends the verdict to `events.db` but
    // nothing ever folds a `SpawnResult` into the persistent `graph.db`, so a discarded finding
    // is never pruned. Only an adjudicator result disposes anything (`adjudication` self-gates
    // on the adjudicator role and returns `None` otherwise), so folding EVERY recorded result
    // is safe: a plain worker/courier result folds to nothing.
    //
    // AFTER the durable append, as `emit_event` does: the record already landed in the log, so a
    // fold that cannot happen (a graph that owes its rebuild, or one locked past its busy
    // timeout) never fails a result the log holds - but it is SAID, with the reason, because a
    // current graph that missed the fold is re-derived by no rebuild, and a silently lost
    // adjudicator verdict would keep its discarded findings in grounding. A `--if-absent` no-op
    // appended nothing, so there is nothing new to fold (the prior record already did).
    if let Some(pos) = recorded {
        if let contextgraph::Fold::NotFolded(why) = fold_recorded_result(&loc, &res, pos) {
            println!("{}", not_folded(&why));
        }
    }

    // Per-spawn scratch reclamation (spec 34, criterion 1): the moment this spawn's result is
    // recorded - for ANY outcome (a success, a reject verdict, an `--error`, or a
    // liveness/infra fault, all of which reach the store through THIS courier) - reclaim the
    // dedicated scratch dir rigger assigned it under `.rigger/tmp`. `cmd_result` only ever
    // runs for the spawn being reported, so a spawn with no recorded result is never touched
    // (the "keeps its scratch" half of the criterion, by construction). Reclaimed even on a
    // `--if-absent` no-op: a result already stood, so a prior `rigger result` already
    // reclaimed it and reclaiming an already-gone path is a graceful no-op. Best-effort - the
    // record already landed durably, so scratch reclamation may never fail a recorded result.
    reclaim_spawn_scratch(&loc, &prior, &res.id);
    Ok(())
}

/// Reclaim the per-spawn scratch dir [`spawn_scratch_path`] assigned spawn `spawn_id`,
/// resolving the scratch root and run id the SAME way the assignment (the replay driver's
/// park) did so the reclaim targets the exact path the run created (spec 34, criterion 1) -
/// PLUS every REGISTERED SCRATCH ROOT beyond `agent-scratch` (spec 77, criterion 2):
/// currently just the SPAWN-scoped mutation-testing scratch dir the seeded persona's `cargo
/// mutants` invocation points `TMPDIR` at ([`mutation_scratch_path`]). That root is keyed by
/// the FULL spawn id, never a bare unit or a unit+attempt composite (spec 77 Design "mutation
/// scratch is spawn-scoped, never unit-scoped"), so THIS reporting spawn's own result reclaims
/// ONLY its own leaf - exactly like the agent-scratch half two lines above, both deriving from
/// the identical `spawn_id` this function already has in hand, no extraction needed. A
/// reviewer spawn (lens/adversary/adjudicator/sdet-author) never populated a mutation-scratch
/// leaf of its own (only the implementer role ever runs `cargo mutants`), so its own reclaim
/// call here is a harmless no-op - it was never going to name anything real to begin with.
///
/// Full-spawn-id keying is what keeps `speculation_width > 1` safe: each candidate lane is a
/// DISTINCT spawn ([`crate::spawn::spawn_id`]`(unit, ROLE_IMPLEMENTER, lane)`), so concurrent
/// lanes of one unit never share a reclaim target - the round-7 review reject found a bare-unit
/// key let the first lane to report SIGKILL a sibling lane's still-running `cargo mutants`
/// subprocess out from under it (`sdet-u77c2r7-mutation-scratch-key-collides-across-
/// speculation-lanes`, `adv-u77c2r7-shared-lane-reap-sigkills-sibling-mutants-on-any-result`).
///
/// Entirely best-effort and platform-tolerant: the result already landed durably in
/// `events.db`, so neither resolving a root nor removing a dir may surface an error that fails
/// a recorded result, and an already-gone (or never-populated) path is a graceful no-op. A
/// DEGENERATE id (`spawn_scratch_path`/`mutation_scratch_path` returning `None` -
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

/// The reclaim ACTION itself: given an already-resolved `scratch_root`/`run_id`, reap spawn
/// `spawn_id`'s per-spawn `agent-scratch` dir (spec 34, criterion 1) plus its REGISTERED
/// mutation-testing scratch dir (spec 77, criterion 2), if the ambient environment resolves a
/// cache home to look under (a homeless environment has nothing there to reclaim either).
///
/// The ONE reap authority both production call sites that record a spawn's terminal outcome
/// converge on, so they can never diverge on what "reclaim a spawn's scratch" means:
/// [`reclaim_spawn_scratch`] (the `cmd_result` courier path - success/reject/`--error`, and any
/// outcome an operator or `workflows/rigger.js`'s death courier records through it) resolves
/// `scratch_root`/`run_id` from its own `StoreLocation`/prior-events context and delegates
/// here; `cmd_step`'s liveness-sweep call site delegates here directly for each spawn
/// [`rigger::liveness::sweep`] just recorded a fault for, using the `scratch_root`/`run_id` it
/// already resolved for the sweep call itself. Before this second call site existed, a hung
/// spawn's fault - recorded by the sweep via `spawn_store::record_result_if_absent` DIRECTLY,
/// in-process, never through `cmd_result` - left its registered mutation-scratch dir
/// unreclaimed forever unless its owning unit later reached a terminal state (round-2/3 review
/// reject, spec 77 criterion 2, `adv-u77c2b-liveness-sweep-bypasses-reclaim`): the mechanism
/// spec 10 exists for (a hung `cargo mutants` implementer) is exactly the workload spec 77's
/// own Problem statement targets, so this was not a hypothetical corner.
///
/// Keyed on the SAME raw `spawn_id` at both call sites - no unit/attempt extraction, so
/// neither can ever diverge from how `spawn::spawn_id` mints it. Entirely best-effort and
/// platform-tolerant (see [`reclaim_spawn_scratch`]'s doc comment for the full rationale): a
/// DEGENERATE id ([`spawn_scratch_path`]/[`mutation_scratch_path`] returning `None`) is a
/// no-op, never a fabricated path to reap.
fn reclaim_spawn_registered_scratch(scratch_root: &str, run_id: &str, spawn_id: &str) {
    if let Some(path) = spawn_scratch_path(scratch_root, run_id, spawn_id) {
        reap_then_remove_dir(&path, Path::new(scratch_root));
    }
    if let Some(cache_home) =
        cache_home_from(std::env::var_os("XDG_CACHE_HOME"), std::env::var_os("HOME"))
    {
        if let Some(path) = mutation_scratch_path(&cache_home, spawn_id) {
            // The registered mutation-scratch ROOT (`<cache_home>/rigger-mutants`), NEVER
            // `scratch_root` - by construction (spec 77 criterion 2) it lives in the user
            // cache, outside any one run's own scratch tree (spec 78 round 2, decision
            // `u78c2r2-authorized-root-caller-supplied`: this is the exact call that was an
            // unconditional no-op before this fix, since it could never canonicalize under
            // any `<repo>/.rigger/tmp`).
            reap_then_remove_dir(&path, &mutation_scratch_root(&cache_home));
        }
    }
}

/// Fold a just-recorded [`spawn::SpawnResult`] into the run's context graph at its recorded
/// `position`, so an adjudicator verdict that disposes a review's findings invalidates their
/// graph edges (the `contextgraph` `TYPE_SPAWN_RESULT` fold arm), and answer what became of the
/// fold. This is the result-channel counterpart of [`mcpserver::emit_event`]: rebuild the
/// appended event, stamp it with the position the append returned, and fold it through the one
/// [`contextgraph::Fold::of`] into the graph [`StoreLocation::graph`] opens, the same opener
/// [`cmd_emit`] hands the emit core.
///
/// The result is already on the log, so nothing here fails it: a graph that owes its rebuild,
/// one locked past its busy timeout, or a result that will not serialize (unreachable for one
/// that just serialized to append) is answered as [`contextgraph::Fold::NotFolded`], with the
/// reason, for the caller to report. A graph that owes its rebuild re-derives the result from
/// the log when `rigger setup` rebuilds it; a current graph that missed it never does.
fn fold_recorded_result(
    loc: &StoreLocation,
    res: &spawn::SpawnResult,
    pos: rigger::eventstore::Position,
) -> contextgraph::Fold {
    match res.to_event() {
        Ok(mut event) => {
            event.position = pos;
            contextgraph::Fold::of(loc.graph(), &event)
        }
        Err(e) => contextgraph::Fold::NotFolded(e.to_string()),
    }
}

/// The pid-scoped parent-to-child contract's env var name (spec 66, criterion 5 disposition,
/// closing the ad-hoc-mechanism class two earlier rounds tried and a reviewer rejected: a
/// bare presence sentinel leaks across unrelated process trees, an in-process static leaks
/// across in-process calls but not across a real process boundary). A nesting surface that
/// has already printed [`spec_lint_next_step`]'s reminder sets this to ITS OWN process id
/// before spawning a child; a genuinely nested child then suppresses its own reminder. An
/// INTERMEDIARY (a non-rigger wrapper such as `shim/shim.mjs` that spawns rigger as its own
/// OS child) passes the contract through in the same pass-through form: see `shim.mjs`'s
/// mirror of this name and [`spec_lint_reminder_suppressed`]'s doc comment.
const SPEC_LINT_REMINDER_PID_ENV: &str = "RIGGER_SPEC_LINT_REMINDER_PID";

/// Whether the spec-lint discoverability reminder is suppressed for THIS process, per the
/// [`SPEC_LINT_REMINDER_PID_ENV`] contract (spec 66, criterion 5): suppressed ONLY when
/// `env_value` parses as a `u32` AND equals `parent_pid` (the real direct OS parent, e.g.
/// `std::os::unix::process::parent_id()`) - an absent, foreign (parses but names some other
/// process), stale, or malformed value always means "print". Pure over its inputs (no env or
/// process reads inside) so both directions - nested invocation suppressed, ambient pollution
/// from an unrelated process tree still prints - are unit-testable without a real process
/// tree; [`spec_lint_reminder_should_print`] is the impure wrapper real call sites use.
fn spec_lint_reminder_suppressed(env_value: Option<&str>, parent_pid: u32) -> bool {
    env_value
        .and_then(|v| v.trim().parse::<u32>().ok())
        .is_some_and(|seen_pid| seen_pid == parent_pid)
}

/// The real-environment, real-process wrapper around [`spec_lint_reminder_suppressed`]: reads
/// [`SPEC_LINT_REMINDER_PID_ENV`] and this process's real direct parent id, so every
/// reminder-printing call site (`run_cli`, `run_workflow`, `cmd_workflow`, `cmd_step`)
/// shares ONE decision rather than per-site copies that could drift. `parent_id()` is Unix-only in `std`; a
/// non-Unix build has no parent-pid seam to check, so it always prints (never silently
/// suppresses on a platform this contract cannot verify).
fn spec_lint_reminder_should_print() -> bool {
    #[cfg(unix)]
    {
        !spec_lint_reminder_suppressed(
            std::env::var(SPEC_LINT_REMINDER_PID_ENV).ok().as_deref(),
            std::os::unix::process::parent_id(),
        )
    }
    #[cfg(not(unix))]
    {
        true
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::pgid_of;
    use std::process::Command;

    // --- Spec 66, criterion 5: REMINDER DEDUP - the pid-scoped parent-to-child contract ---

    /// The nested-invocation direction: a child whose env names EXACTLY its own real direct
    /// parent pid is suppressed (the nesting surface just printed and passed its own pid
    /// down).
    #[test]
    fn spec_lint_reminder_suppressed_when_env_names_the_real_direct_parent_pid() {
        assert!(spec_lint_reminder_suppressed(Some("4242"), 4242));
    }

    /// The ambient-pollution direction: absent, foreign (a pid that is not the real direct
    /// parent), stale, or malformed values must never suppress - state lives ONLY in the
    /// explicit parent-to-child contract, never ambient env presence.
    #[test]
    fn spec_lint_reminder_prints_on_absent_foreign_stale_or_malformed_env() {
        assert!(
            !spec_lint_reminder_suppressed(None, 4242),
            "an absent value must print"
        );
        assert!(
            !spec_lint_reminder_suppressed(Some("1"), 4242),
            "a foreign pid (not the real direct parent) must print"
        );
        assert!(
            !spec_lint_reminder_suppressed(Some("not-a-pid"), 4242),
            "a malformed value must print"
        );
        assert!(
            !spec_lint_reminder_suppressed(Some(""), 4242),
            "a stale/empty value must print"
        );
        assert!(
            !spec_lint_reminder_suppressed(Some(" 4242 "), 4243),
            "a value that parses but does not equal the real direct parent must print"
        );
    }

    // --- Spec 39, criterion 1: idempotent start of the run dashboard on the step path ---

    /// The flagship criterion-1 proof: a `step` with NO dash serving starts exactly one, and
    /// a later `step` while it is serving starts NONE - the marker/pid check short-circuits.
    /// `start` is injected (counting spawns and recording a marker owned by THIS process, a
    /// guaranteed-live pid), so the injected liveness predicate finds the recorded dash
    /// serving on the second call - proving idempotency without a real dashboard process.
    #[test]
    fn ensure_run_dashboard_at_starts_once_then_short_circuits_on_a_live_marker() {
        use std::cell::Cell;
        let dir = tempfile::tempdir().unwrap();
        let marker_path = dir.path().join(DASH_MARKER_FILE);
        let starts = Cell::new(0u32);
        let live = dash::DashMarker {
            port: 54321,
            pid: std::process::id(),
        };

        // First step of the run: no marker yet -> start one and record its marker.
        let first = ensure_run_dashboard_at(
            &marker_path,
            |m| m.pid == std::process::id(),
            || {
                starts.set(starts.get() + 1);
                Ok(live)
            },
        );
        assert_eq!(
            first,
            DashStart::Started(54321),
            "the first step starts a dash"
        );
        assert_eq!(starts.get(), 1, "exactly one dash was started");
        assert_eq!(
            dash::DashMarker::read(&marker_path),
            Some(live),
            "the started dash is recorded for later steps to discover"
        );

        // A later step WHILE it is serving: the marker names a live dash -> NO second start.
        let second = ensure_run_dashboard_at(
            &marker_path,
            |m| m.pid == std::process::id(),
            || {
                starts.set(starts.get() + 1);
                Ok(dash::DashMarker {
                    port: 60000,
                    pid: std::process::id(),
                })
            },
        );
        assert_eq!(
            second,
            DashStart::AlreadyServing(54321),
            "a later step is a no-op while a dash is serving"
        );
        assert_eq!(
            starts.get(),
            1,
            "no second dash was started - the start is idempotent across steps"
        );
    }

    /// [`dash_marker_serving`] is the ONE named predicate `ensure_run_dashboard`'s real
    /// production wiring passes to [`ensure_run_dashboard_at`] - pinned directly, not only
    /// through an injected test double, so a mutant that made it unconditionally trust any
    /// marker (round 3, part of closing adv-u69c4r2-mismatched-marker-still-trusts-a-dead-url's
    /// remediation) cannot slip through untested. A port nothing binds must read as NOT
    /// serving - the real network probe, never a bare presence check.
    #[test]
    fn dash_marker_serving_reports_false_when_nothing_answers_the_markers_port() {
        let port = dash::free_port_from(40000).expect("a free loopback port must be available");
        assert!(
            !dash_marker_serving(dash::DashMarker { port, pid: 1 }),
            "a marker naming a port nothing serves must read as not serving"
        );
    }

    /// A marker left by a crashed/reaped dash (recorded but NOT serving) does not suppress a
    /// fresh start: the step starts a new dash and overwrites the stale marker.
    #[test]
    fn ensure_run_dashboard_at_restarts_when_the_recorded_dash_is_gone() {
        let dir = tempfile::tempdir().unwrap();
        let marker_path = dir.path().join(DASH_MARKER_FILE);
        dash::DashMarker {
            port: 40000,
            pid: 123,
        }
        .write(&marker_path)
        .unwrap();
        let outcome = ensure_run_dashboard_at(
            &marker_path,
            |_| false, // the recorded dash is gone
            || {
                Ok(dash::DashMarker {
                    port: 40001,
                    pid: 456,
                })
            },
        );
        assert_eq!(
            outcome,
            DashStart::Started(40001),
            "a dead marker -> start a fresh dash"
        );
        assert_eq!(
            dash::DashMarker::read(&marker_path),
            Some(dash::DashMarker {
                port: 40001,
                pid: 456
            }),
            "the fresh dash replaces the stale marker"
        );
    }

    /// A best-effort start failure degrades to headless (a warning, `DashStart::Failed`) and
    /// records no marker - never a panic or a failed step. The failure's diagnosis text (spec 62
    /// round 2 fix, adv-u62c3-diagnosis-unreachable-from-step-path-auto-start) is carried
    /// through verbatim - the `io::Error`'s `Display` - rather than discarded, so this pins the
    /// carry-through at the injected-closure seam directly.
    #[test]
    fn ensure_run_dashboard_at_reports_failed_when_the_start_errors() {
        let dir = tempfile::tempdir().unwrap();
        let marker_path = dir.path().join(DASH_MARKER_FILE);
        let outcome = ensure_run_dashboard_at(
            &marker_path,
            |_| false,
            || Err(std::io::Error::other("no free port")),
        );
        assert_eq!(
            outcome,
            DashStart::Failed("no free port".to_string()),
            "a start failure degrades to headless, not a panic, carrying the failure's own \
             diagnosis text rather than discarding it"
        );
        assert_eq!(
            dash::DashMarker::read(&marker_path),
            None,
            "a failed start records no marker"
        );
    }

    // --- Spec 62, criterion 1: marker follows bind (confirmed ACROSS the process boundary) ---

    /// The success arm of [`wait_for_dash_bind`]: once a port genuinely answers as a rigger dash
    /// (carrying [`dash::DASH_HEADER`], exactly what `dash_serving_on` itself checks for), the
    /// wait confirms PROMPTLY - well inside its window - rather than waiting the window out.
    /// `child` here is a real, independently-spawned long-lived process standing in for "the
    /// detached child launched"; the REAL server thread plays the "its port is now answering"
    /// role, matching how production separates the two (a spawn succeeding is not a confirmed
    /// bind - only the probe answering is).
    #[test]
    fn wait_for_dash_bind_confirms_promptly_once_the_port_answers_as_a_dash() {
        use std::io::Write as _;
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for mut s in listener.incoming().flatten() {
                let _ = s.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\n{}: probe\r\nConnection: close\r\n\r\n",
                        dash::DASH_HEADER
                    )
                    .as_bytes(),
                );
            }
        });
        let mut child = Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn a real, long-lived child standing in for the detached dash process");

        let start = std::time::Instant::now();
        let confirmed = wait_for_dash_bind(
            port,
            &mut child,
            std::time::Duration::from_secs(5),
            std::time::Duration::from_millis(20),
        );

        assert!(
            confirmed,
            "a port genuinely answering as a rigger dash must be confirmed"
        );
        assert!(
            start.elapsed() < std::time::Duration::from_secs(2),
            "a confirmed bind must return promptly, not wait out the whole window; took {:?}",
            start.elapsed()
        );
        let _ = child.kill();
        let _ = child.wait();
    }

    /// The early-exit arm: `cmd.spawn()` succeeding proves only that the OS launched a process,
    /// never that it bound (spec 62, criterion 1) - so a child that exits BEFORE ever confirming
    /// a bind must fail FAST, never wait out the whole window. `sleep 0` is a real process that
    /// exits almost immediately without binding anything.
    #[test]
    fn wait_for_dash_bind_fails_fast_when_the_child_exits_before_confirming() {
        let port = dash::free_port_from(42000).expect("a free loopback port must be available");
        let mut child = Command::new("sleep")
            .arg("0")
            .spawn()
            .expect("spawn a real, near-instantly-exiting child");
        // Give the OS a moment to make the exit visible to `try_wait` before the wait begins, so
        // this proves the early-exit short-circuit rather than racing the child's own exit.
        std::thread::sleep(std::time::Duration::from_millis(100));

        let start = std::time::Instant::now();
        let confirmed = wait_for_dash_bind(
            port,
            &mut child,
            std::time::Duration::from_secs(5),
            std::time::Duration::from_millis(20),
        );

        assert!(
            !confirmed,
            "a child that exited before confirming a bind must never be confirmed"
        );
        assert!(
            start.elapsed() < std::time::Duration::from_secs(1),
            "an already-exited child must fail FAST, not wait out the whole window; took {:?}",
            start.elapsed()
        );
    }

    /// The window-bound arm, proven against a REAL held port - not only an injected stand-in -
    /// per the criterion's own instruction ("this criterion's test exercises this real path end
    /// to end - a real detached spawn against a real held port"). A plain listener HOLDS a real
    /// port but never calls `accept()`: the kernel completes the TCP handshake from its backlog,
    /// but nothing application-level ever reads or writes, so a probing client hangs until ITS
    /// OWN read timeout - exactly the "job-control-stopped predecessor whose socket never
    /// accepts" shape spec 62 names. The stand-in `child` stays alive the whole time too, so only
    /// the bounded window (never an early exit) can end this wait. Run on a background thread
    /// with a hard `recv_timeout` so a regression to an unbounded loop fails LOUD here instead of
    /// hanging the whole suite (the same discipline `dash_serving_on_is_bounded_against_a_byte_
    /// dribbling_holder` in `dash.rs` uses).
    #[test]
    fn wait_for_dash_bind_times_out_against_a_real_held_port() {
        use std::sync::mpsc;

        let held = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = held.local_addr().unwrap().port();
        let mut child = Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn a real, long-lived child standing in for the detached dash process");

        let (tx, rx) = mpsc::channel();
        let start = std::time::Instant::now();
        std::thread::spawn(move || {
            let confirmed = wait_for_dash_bind(
                port,
                &mut child,
                std::time::Duration::from_millis(900),
                std::time::Duration::from_millis(20),
            );
            let _ = child.kill();
            let _ = child.wait();
            let _ = tx.send(confirmed);
        });

        match rx.recv_timeout(std::time::Duration::from_secs(5)) {
            Ok(confirmed) => {
                assert!(
                    !confirmed,
                    "a port held by a listener that never accepts must never be confirmed as a bind"
                );
                assert!(
                    start.elapsed() < std::time::Duration::from_secs(5),
                    "wait_for_dash_bind must be bounded against a real held port; it took {:?}",
                    start.elapsed()
                );
            }
            Err(_) => panic!(
                "wait_for_dash_bind HUNG against a real held port - it never returned within 5s \
                 (a regression that would hang every caller, including ensure_run_dashboard)"
            ),
        }
        drop(held);
    }

    /// Spec 62 round 4 fix (adj-u62c3r3-verdict-reject-child-self-attribution): the DEADLINE
    /// sub-case of `wait_for_dash_bind` (window elapses while `child` is STILL ALIVE, never the
    /// early-exit sub-case) must never let the HELD-PORT DIAGNOSIS self-attribute the port to
    /// the very spawn attempt that is still starting. Mirrors `wait_for_dash_bind_times_out_
    /// against_a_real_held_port` above (a real `TcpListener` bound in THIS test process stands
    /// in for "the port is genuinely held" - a plain listener never calls `accept()`, so it
    /// never answers as a dash and the wait can only end via the window, never the probe) but
    /// drives the composed `wait_for_dash_bind_or_diagnose` instead of the bare probe, with
    /// `spawned_pid` set to THIS test process's own pid - the exact pid the bound listener makes
    /// independently discoverable via `/proc`, standing in for "the spawned child has bound the
    /// port itself" without needing an external interpreter to fabricate one. `child` (used only
    /// for the `try_wait` liveness check `wait_for_dash_bind` itself performs) stays a real,
    /// unrelated, long-lived `sleep` process, decoupled from `spawned_pid` exactly as production
    /// couples them (`child.id()`) only incidentally - the composed function's two parameters
    /// are independent by construction, which is what makes this scenario constructible at all.
    ///
    /// Before this fix, the "already in use by pid {spawned_pid}" framing fired here exactly as
    /// it would for a genuine competing holder (empirically reproduced by the round-3 adversary
    /// review with a real bound child); the fix must instead recognize the pid match and report
    /// a distinct, honest "still starting" message that never claims a competing occupant.
    #[test]
    fn wait_for_dash_bind_or_diagnose_never_self_attributes_its_own_still_starting_spawn() {
        if !Path::new("/proc").is_dir() {
            return;
        }

        let held = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = held.local_addr().unwrap().port();
        let spawned_pid = std::process::id();

        let mut child = Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn a real, long-lived child standing in for the detached dash process");

        let result = wait_for_dash_bind_or_diagnose(
            port,
            &mut child,
            spawned_pid,
            std::time::Duration::from_millis(200),
            std::time::Duration::from_millis(20),
        );

        let _ = child.kill();
        let _ = child.wait();
        drop(held);

        let msg = result
            .expect_err("a port held by a listener that never accepts must never confirm a bind");
        assert!(
            !msg.contains("already in use"),
            "the discovered holder IS spawned_pid itself - this must never be framed as a \
             competing external holder; got: {msg}"
        );
        assert!(
            msg.contains(&spawned_pid.to_string()),
            "the still-starting message must still name the pid so an operator can correlate \
             it; got: {msg}"
        );
        let lower = msg.to_lowercase();
        assert!(
            lower.contains("starting") || lower.contains("slow"),
            "the self-attribution case must get a distinct, honest message explaining this is \
             the spawn attempt itself, not a competing process; got: {msg}"
        );
    }

    /// The REAL production seam end to end - not only `ensure_run_dashboard_at`'s already-correct
    /// injected-closure ordering test (spec 62, criterion 1's own instruction). Under `cargo
    /// test`, `current_exe()` names THIS test binary, which never recognizes `dash` as a
    /// subcommand and exits immediately without binding anything - so `spawn_run_dashboard_
    /// detached` deterministically reaches the "child exited before confirming" arm of
    /// `wait_for_dash_bind`, proving the UN-injected real seam surfaces `Err` (never a premature
    /// `Ok` merely because `Command::spawn` succeeded) for a bind that never actually happened.
    /// Wired through `ensure_run_dashboard_at` end to end: NO marker is written, and a
    /// PRE-EXISTING marker is left completely untouched - the write ordering this criterion owns.
    ///
    /// `DASH_PORT_ENV` is pinned to a fresh ephemeral port (never the fixed default) so this can
    /// never collide with a real dash already serving this machine's singleton address; serialized
    /// with the project's other real-dash tests since it mutates process-wide environment state.
    #[test]
    #[serial_test::serial(dash_default_port)]
    fn ensure_run_dashboard_at_writes_no_marker_when_the_real_spawn_never_confirms_a_bind() {
        let port = dash::free_port_from(41000).expect("a free loopback port must be available");
        std::env::set_var(DASH_PORT_ENV, port.to_string());
        let outcome = std::panic::catch_unwind(|| {
            let dir = tempfile::tempdir().unwrap();
            let marker_path = dir.path().join(DASH_MARKER_FILE);
            let stale = dash::DashMarker {
                port: 40002,
                pid: 999_999,
            };
            stale.write(&marker_path).unwrap();

            let outcome =
                ensure_run_dashboard_at(&marker_path, |_| false, spawn_run_dashboard_detached);

            assert!(
                matches!(outcome, DashStart::Failed(_)),
                "a bind that is never confirmed degrades to headless, not a fabricated success; \
                 got: {outcome:?}"
            );
            assert_eq!(
                dash::DashMarker::read(&marker_path),
                Some(stale),
                "a bind that never confirmed must leave a pre-existing marker completely untouched"
            );
        });
        std::env::remove_var(DASH_PORT_ENV);
        outcome.unwrap();
    }

    /// Spec 62 round 2 fix (adv-u62c3-diagnosis-unreachable-from-step-path-auto-start): the
    /// step-path auto-start's OWN failure message names the holder of a genuinely held port -
    /// not only the manual `rigger dash` CLI arm. This is the real, un-injected
    /// `spawn_run_dashboard_detached` seam (same as the sibling test above), but this time the
    /// ensure port is pinned to a port THIS test process holds itself with a plain listener, so
    /// the spawned child's `bind_singleton` (were it ever reached) would see a genuine
    /// `AddrInUse` - the exact conflict shape spec 62's criterion 3 exists to explain. Before
    /// this fix, `DashStart::Failed` carried no payload at all, so this assertion could not even
    /// be expressed; the held port's pid must now appear in the returned diagnosis, closing the
    /// gap the adjudicator's reject upheld: the diagnosis was structurally unreachable from this
    /// exact path.
    #[test]
    #[serial_test::serial(dash_default_port)]
    fn ensure_run_dashboard_at_names_the_held_ports_holder_when_the_real_spawn_fails() {
        if !Path::new("/proc").is_dir() {
            // The `/proc`-surface discovery is Unix/Linux-only by design (spec 62 Notes); this
            // test's assertions are unavailable elsewhere.
            return;
        }
        let held = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = held.local_addr().unwrap().port();
        std::env::set_var(DASH_PORT_ENV, port.to_string());
        let outcome = std::panic::catch_unwind(|| {
            let dir = tempfile::tempdir().unwrap();
            let marker_path = dir.path().join(DASH_MARKER_FILE);

            let outcome =
                ensure_run_dashboard_at(&marker_path, |_| false, spawn_run_dashboard_detached);

            let my_pid = std::process::id().to_string();
            match outcome {
                DashStart::Failed(msg) => {
                    assert!(
                        msg.contains(&format!("by pid {my_pid}")),
                        "a bind that fails against a port THIS test process holds must name \
                         this process's own pid as the holder - the HELD-PORT DIAGNOSIS, not \
                         only the bare 'did not confirm a bind' wording; checked as the exact \
                         `by pid {{N}}` attribution phrase, not a raw pid-string substring test, \
                         since this project's mandatory pid-namespace test sandbox \
                         (.cargo/pidns-runner.sh, every test binary here runs AS PID 1 of its own \
                         fresh namespace) makes a raw substring check vacuous - \"1\" trivially \
                         matches inside the loopback address \"127.0.0.1\" regardless of what the \
                         message actually reports; got: {msg}"
                    );
                    assert!(
                        msg.contains(&port.to_string()),
                        "the diagnosis must always name the held address; got: {msg}"
                    );
                }
                other => panic!(
                    "expected DashStart::Failed(..) naming the held port's holder pid \
                     {my_pid}, got {other:?}"
                ),
            }
        });
        std::env::remove_var(DASH_PORT_ENV);
        drop(held);
        outcome.unwrap();
    }

    /// Spec 62 round 3 fix (adj-u62c3r2-verdict-reject-non-addrinuse-mislabel), mirroring
    /// `cmd_dash_leaves_a_non_addrinuse_bind_error_unenriched` (tests/cli.rs) for the step path:
    /// a bind that `wait_for_dash_bind` never confirms for a reason UNRELATED to a genuinely
    /// held port must never be framed as "already in use" - the false-positive the adjudicator
    /// reproduced (round 2 fired that framing unconditionally on every `wait_for_dash_bind`
    /// failure; `RIGGER_DASH_PORT=1` gave a real `PermissionDenied`, not `AddrInUse`).
    ///
    /// This test cannot reproduce that exact `PermissionDenied` shape through the real-spawn
    /// seam (under `cargo test`, the spawned child is the TEST BINARY itself via `current_exe`,
    /// which never reaches a real `bind_singleton` call at all - see the sibling test above's own
    /// doc), but it exercises the SAME gate this round's fix adds from the other side: an
    /// ephemeral port NOTHING holds. `wait_for_dash_bind` fails here for the harness's own
    /// early-exit reason, not a held port - the exact "unrelated reason" class this fix must
    /// never mislabel. `describe_held_port_if_confirmed` (crates/rigger-dash/src/dash.rs) is unit-tested directly
    /// against both a held and an unheld port; this test locks the PRODUCTION WIRING end to end,
    /// the gap round 2 shipped with (no step-path mirror of the CLI arm's regression test).
    #[test]
    #[serial_test::serial(dash_default_port)]
    fn ensure_run_dashboard_at_never_claims_a_phantom_holder_for_an_unheld_port() {
        let port = dash::free_port_from(41500).expect("a free loopback port must be available");
        std::env::set_var(DASH_PORT_ENV, port.to_string());
        let outcome = std::panic::catch_unwind(|| {
            let dir = tempfile::tempdir().unwrap();
            let marker_path = dir.path().join(DASH_MARKER_FILE);

            let outcome =
                ensure_run_dashboard_at(&marker_path, |_| false, spawn_run_dashboard_detached);

            match outcome {
                DashStart::Failed(msg) => {
                    assert!(
                        !msg.contains("already in use"),
                        "nothing holds this ephemeral port - the diagnosis must not claim a \
                         phantom holder just because wait_for_dash_bind failed for an unrelated \
                         reason; got: {msg}"
                    );
                }
                other => panic!("expected DashStart::Failed(..), got {other:?}"),
            }
        });
        std::env::remove_var(DASH_PORT_ENV);
        outcome.unwrap();
    }

    // --- Spec 62, criterion 2: self-heal (a successful start replaces a stale marker) ---

    /// A marker naming a DEAD pid whose recorded port nothing answers: wired through the REAL
    /// `dash_marker_serving` probe (not an injected fake, unlike
    /// `ensure_run_dashboard_at_restarts_when_the_recorded_dash_is_gone` above, which already
    /// covers this same overwrite mechanically but only through an injected `|_| false`), the
    /// port never actually answering as a dash makes the step start a fresh one, and the stale
    /// record is REPLACED with the new server's own port/pid - the first of the two forms spec
    /// 62 criterion 2's Done-when text names ("dead PID").
    #[test]
    fn ensure_run_dashboard_at_self_heals_a_marker_naming_a_dead_pid() {
        let dir = tempfile::tempdir().unwrap();
        let marker_path = dir.path().join(DASH_MARKER_FILE);
        let dead_port =
            dash::free_port_from(41100).expect("a free loopback port must be available");
        dash::DashMarker {
            port: dead_port,
            // No process on this machine is required to hold this exact pid; the REAL probe
            // below decides self-heal purely from the port, never this field.
            pid: 999_999,
        }
        .write(&marker_path)
        .unwrap();

        let fresh = dash::DashMarker {
            port: dead_port + 1,
            pid: std::process::id(),
        };
        let outcome = ensure_run_dashboard_at(&marker_path, dash_marker_serving, || Ok(fresh));

        assert_eq!(
            outcome,
            DashStart::Started(fresh.port),
            "the REAL still-serving probe finds nothing answering the dead marker's port, so a \
             fresh dash starts"
        );
        assert_eq!(
            dash::DashMarker::read(&marker_path),
            Some(fresh),
            "self-heal replaces the dead-pid marker with the new server's own record"
        );
    }

    /// A marker naming a LIVE pid (this very test process, unambiguously alive) whose recorded
    /// port nothing answers as a dash: `dash_marker_serving` never reads the pid field at all,
    /// only probes the port (see its own doc), so a genuinely-alive-but-unrelated pid must never
    /// suppress self-heal. The second of the two forms spec 62 criterion 2's Done-when text
    /// names ("live PID not serving the recorded port") - distinct from the dead-pid case above
    /// precisely because pid liveness itself is never load-bearing for this decision, only the
    /// port's own probe is.
    #[test]
    fn ensure_run_dashboard_at_self_heals_a_marker_naming_a_live_pid_whose_port_is_unserved() {
        let dir = tempfile::tempdir().unwrap();
        let marker_path = dir.path().join(DASH_MARKER_FILE);
        let unserved_port =
            dash::free_port_from(41200).expect("a free loopback port must be available");
        let live_pid = std::process::id();
        dash::DashMarker {
            port: unserved_port,
            pid: live_pid,
        }
        .write(&marker_path)
        .unwrap();

        let fresh = dash::DashMarker {
            port: unserved_port + 1,
            pid: live_pid,
        };
        let outcome = ensure_run_dashboard_at(&marker_path, dash_marker_serving, || Ok(fresh));

        assert_eq!(
            outcome,
            DashStart::Started(fresh.port),
            "a live-but-unrelated pid never suppresses self-heal - only the port's own probe \
             decides whether the recorded dash is still serving"
        );
        assert_eq!(
            dash::DashMarker::read(&marker_path),
            Some(fresh),
            "self-heal replaces the marker even though its old pid field was genuinely alive"
        );
    }

    /// The still-serving short-circuit is UNCHANGED by self-heal (the design bullet's own text)
    /// even for the one case self-heal deliberately does NOT correct: a marker carrying
    /// `dash::UNATTRIBUTED_PID` whose port genuinely keeps answering. `d-u62c1-unattributable-
    /// serving-disposition` scopes self-heal to DEAD or stale markers only, never a live-but-
    /// unattributable server - this proves that scope holds in the real wiring: with a REAL
    /// listener answering as a dash (the same DASH_HEADER response `wait_for_dash_bind`'s own
    /// test above uses), `ensure_run_dashboard_at` short-circuits to `AlreadyServing` WITHOUT
    /// ever calling `start` (a call here would panic), and the on-disk marker - sentinel pid and
    /// all - is left completely untouched. Regression-locks
    /// `adv-u62c1r5-eventually-correct-claim-unreachable-while-the-same-dash-serves`.
    #[test]
    fn ensure_run_dashboard_at_never_revisits_a_still_serving_unattributed_pid_marker() {
        use std::io::Write as _;
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for mut s in listener.incoming().flatten() {
                let _ = s.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\n{}: probe\r\nConnection: close\r\n\r\n",
                        dash::DASH_HEADER
                    )
                    .as_bytes(),
                );
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let marker_path = dir.path().join(DASH_MARKER_FILE);
        let sentinel = dash::DashMarker {
            port,
            pid: dash::UNATTRIBUTED_PID,
        };
        sentinel.write(&marker_path).unwrap();

        let outcome = ensure_run_dashboard_at(&marker_path, dash_marker_serving, || {
            panic!("self-heal must never call start() while the recorded port still serves")
        });

        assert_eq!(
            outcome,
            DashStart::AlreadyServing(port),
            "a still-serving sentinel marker short-circuits; self-heal never gets a chance to run"
        );
        assert_eq!(
            dash::DashMarker::read(&marker_path),
            Some(sentinel),
            "the still-serving marker, sentinel pid and all, is left completely untouched"
        );
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

    /// Spec 50, criterion 4 (stable fixed address): the step-path ensure port resolves to the
    /// FIXED [`dash::DEFAULT_PORT`] when the override env is unset - production's no-free-port-search
    /// singleton contract - and only a VALID `u16` override relocates it; an empty or malformed
    /// value degrades to the default so a bad knob never breaks a run's observability. Pure over the
    /// already-read env value so both branches are provable without mutating the process
    /// environment (the same discipline as `dash_ensure_suppressed`).
    #[test]
    fn dash_ensure_port_defaults_to_the_fixed_address_and_only_a_valid_override_relocates_it() {
        // Unset: the fixed default, with no free-port search - the production singleton address.
        assert_eq!(
            dash_ensure_port_from(None),
            dash::DEFAULT_PORT,
            "an unset override binds the fixed DEFAULT_PORT (the stable singleton address)"
        );
        // A valid u16 relocates it (the seam the step-path dash tests use to inject an ephemeral
        // port and never fight a real machine dash on the fixed default).
        assert_eq!(
            dash_ensure_port_from(Some("54321")),
            54321,
            "a valid u16 override relocates the ensure port"
        );
        assert_eq!(
            dash_ensure_port_from(Some("  8080  ")),
            8080,
            "surrounding whitespace is trimmed before parsing"
        );
        // Absent-shaped or malformed values degrade to the default, never a panic or a break.
        for bad in ["", "   ", "not-a-port", "70000", "-1", "80.5"] {
            assert_eq!(
                dash_ensure_port_from(Some(bad)),
                dash::DEFAULT_PORT,
                "a malformed override ({bad:?}) falls back to the fixed default"
            );
        }
    }

    // ---- `rigger result`: argument parsing and outcome shaping (the stepwise CLI) ----

    #[test]
    fn parse_result_takes_an_id_and_an_optional_output_arg() {
        let a = parse_result_args(&["u/implementer#0".into(), "the diff".into()]).unwrap();
        assert_eq!(a.id, "u/implementer#0");
        assert_eq!(a.text.as_deref(), Some("the diff"));
        assert!(!a.is_error);
        assert!(a.meta.is_none());
    }

    #[test]
    fn parse_result_with_no_output_defers_to_stdin() {
        // Just an id -> text is None, so cmd_result reads the outcome from stdin.
        let a = parse_result_args(&["u/implementer#0".into()]).unwrap();
        assert_eq!(a.id, "u/implementer#0");
        assert!(a.text.is_none());
    }

    #[test]
    fn reclaim_run_scratch_removes_the_run_level_areas_and_spares_per_unit_scratch() {
        // spec 34, criterion 3: the terminal-state run teardown reclaims EXACTLY the run-level
        // shared areas - `agent-scratch`, `agent-live`, and the SHARED build cache
        // (`cargo-target` + `target` directly under the root, the driver's `CARGO_TARGET_DIR`) -
        // and NOTHING else. Per-unit worktrees (`rigger-wt-<slug>`) and per-unit build caches
        // (`cargo-target-<slug>`) are owned by their unit's own terminal reclamation (Worktree::
        // remove / sweep_terminal / the orphan-sweep), never this run-level teardown, so they are
        // SPARED here even though `cargo-target-<slug>` shares the build-cache prefix.
        let root = tempfile::tempdir().unwrap();
        let base = root.path();

        // The four run-level areas the teardown OWNS.
        for area in ["agent-scratch", "agent-live", "cargo-target", "target"] {
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

        for area in ["agent-scratch", "agent-live", "cargo-target", "target"] {
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

    #[test]
    fn parse_result_error_flag_is_order_independent() {
        // `--error` is a bare flag, so it composes with the output positional in either
        // order: `<id> --error <msg>` and `<id> <msg> --error` both mean the same thing.
        for args in [
            vec![
                "u/adjudicator#1".to_string(),
                "--error".into(),
                "boom".into(),
            ],
            vec![
                "u/adjudicator#1".to_string(),
                "boom".into(),
                "--error".into(),
            ],
        ] {
            let a = parse_result_args(&args).unwrap();
            assert_eq!(a.id, "u/adjudicator#1");
            assert_eq!(a.text.as_deref(), Some("boom"));
            assert!(a.is_error);
        }
    }

    #[test]
    fn parse_result_if_absent_is_off_by_default_and_a_bare_order_independent_flag() {
        // Absent by default (the plain `rigger result` still records unconditionally).
        let plain = parse_result_args(&["u/implementer#0".into(), "done".into()]).unwrap();
        assert!(!plain.if_absent, "--if-absent defaults off");

        // `--if-absent` is a bare flag that composes with `--error` and the output
        // positional in any order (the death courier passes `<id> --if-absent --error <msg>`).
        for args in [
            vec![
                "u/adjudicator#1".to_string(),
                "--if-absent".into(),
                "--error".into(),
                "died".into(),
            ],
            vec![
                "u/adjudicator#1".to_string(),
                "died".into(),
                "--error".into(),
                "--if-absent".into(),
            ],
        ] {
            let a = parse_result_args(&args).unwrap();
            assert_eq!(a.id, "u/adjudicator#1");
            assert_eq!(a.text.as_deref(), Some("died"));
            assert!(a.is_error);
            assert!(a.if_absent, "--if-absent must parse regardless of position");
        }
    }

    #[test]
    fn parse_result_meta_must_be_a_json_object() {
        let a = parse_result_args(&[
            "u/implementer#0".into(),
            "out".into(),
            "--meta".into(),
            r#"{"resolved_model":"claude-x"}"#.into(),
        ])
        .unwrap();
        assert_eq!(a.meta.unwrap()["resolved_model"], "claude-x");

        // A non-object JSON --meta is rejected (mirrors `rigger emit`'s object contract).
        assert!(
            parse_result_args(&[
                "u/implementer#0".into(),
                "--meta".into(),
                "\"just-a-string\"".into(),
            ])
            .is_err(),
            "a non-object --meta is rejected"
        );
        // Invalid JSON is rejected.
        assert!(
            parse_result_args(&[
                "u/implementer#0".into(),
                "--meta".into(),
                "{not json".into()
            ])
            .is_err(),
            "malformed --meta json is rejected"
        );
        // --meta with no following value is rejected.
        assert!(
            parse_result_args(&["u/implementer#0".into(), "--meta".into()]).is_err(),
            "--meta needs a value"
        );
    }

    #[test]
    fn parse_result_rejects_missing_id_extra_args_and_unknown_flags() {
        assert!(parse_result_args(&[]).is_err(), "the id is required");
        assert!(
            parse_result_args(&["".into()]).is_err(),
            "an empty id is rejected"
        );
        assert!(
            parse_result_args(&["id".into(), "out".into(), "extra".into()]).is_err(),
            "a third positional is rejected"
        );
        assert!(
            parse_result_args(&["id".into(), "--bogus".into()]).is_err(),
            "an unknown flag is rejected"
        );
    }

    #[test]
    fn build_result_shapes_success_and_failure() {
        let ok = build_result("u/implementer#0", "the diff", false, None).unwrap();
        assert!(!ok.is_error());
        assert_eq!(ok.output, "the diff");

        let failed = build_result("u/adjudicator#1", "crashed", true, None).unwrap();
        assert!(failed.is_error());
        assert_eq!(failed.error, "crashed");

        // A success may legitimately carry empty output (an agent with no final message).
        assert!(build_result("u/implementer#0", "", false, None)
            .unwrap()
            .output
            .is_empty());
    }

    #[test]
    fn build_result_rejects_a_blank_error_message() {
        // A blank --error would leave is_error() false and replay AS a success, silently
        // swallowing the failure the courier meant to record - so it is rejected.
        assert!(build_result("u/adjudicator#1", "   ", true, None).is_err());
        assert!(build_result("u/adjudicator#1", "", true, None).is_err());
    }

    #[test]
    fn build_result_attaches_meta() {
        let res = build_result(
            "u/implementer#0",
            "out",
            false,
            Some(serde_json::json!({"resolved_model": "claude-x"})),
        )
        .unwrap();
        assert_eq!(res.meta["resolved_model"], "claude-x");
    }

    /// Spawn the implementer `id` of unit `u` through a [`ReplayDriver`] over `store`, the way
    /// the conductor's next step does.
    fn replay_spawn(
        store: &Namespaced<'_>,
        id: &str,
    ) -> Result<rigger::conductor::AgentResult, rigger::conductor::Error> {
        use rigger::conductor::{AgentDriver, Error, SpawnOpts};
        let opts = SpawnOpts {
            id: id.to_string(),
            unit: "u".into(),
            stage: "u".into(),
            ..Default::default()
        };
        let no_emit = |_: &str, _: serde_json::Value| -> Result<(), Error> { Ok(()) };
        ReplayDriver::new(store).spawn(
            &rigger::config::AgentDef::default(),
            "do it",
            &opts,
            &no_emit,
        )
    }

    #[test]
    fn a_recorded_result_lets_the_replay_driver_advance_past_the_spawn() {
        // The acceptance shape for this unit: a result recorded through the SAME seam
        // cmd_result uses (build_result -> spawn_store::record_result on the per-project
        // namespaced run stream) flips a PARKED spawn to one the replay driver answers -
        // i.e. the next step advances past it (spec 04, Done-when).
        let backend = Store::open(":memory:").unwrap();
        let store = Namespaced::new(&backend, "proj");
        let id = spawn::spawn_id("u", spawn::ROLE_IMPLEMENTER, 0);

        // Before any result is recorded, the frontier PARKS (it waits for the courier).
        let parked = replay_spawn(&store, &id).expect_err("an unrecorded spawn parks the frontier");
        assert!(rigger::conductor::is_parked(&parked));

        // `rigger result u/implementer#0 "the diff"` records the outcome through the seam.
        let res = build_result(&id, "the diff", false, None).unwrap();
        spawn_store::record_result(&store, &res).unwrap();

        // Now the next step ADVANCES PAST it: the same spawn is answered from the log.
        let answered =
            replay_spawn(&store, &id).expect("a recorded result replays instead of re-parking");
        assert_eq!(answered.output, "the diff");
    }

    #[test]
    fn a_recorded_error_result_replays_as_a_failure_not_a_fake_success() {
        // `rigger result <id> --error <msg>` must replay AS a failure so the conductor
        // remediates it exactly as a live failure, never a fabricated success.
        let backend = Store::open(":memory:").unwrap();
        let store = Namespaced::new(&backend, "proj");
        let id = spawn::spawn_id("u", spawn::ROLE_IMPLEMENTER, 0);

        let res = build_result(&id, "worker died: non-zero exit", true, None).unwrap();
        spawn_store::record_result(&store, &res).unwrap();

        let err = replay_spawn(&store, &id).expect_err("a recorded failure replays as an error");
        assert_eq!(err.0, "worker died: non-zero exit");
        assert!(
            !rigger::conductor::is_parked(&err),
            "a recorded failure is a real failure, not a park"
        );
    }

    /// `rigger workflow` accepts an optional spec AND `--base <ref>` (spec 18, criterion 6):
    /// `--base` is no longer rejected as "expected at most one spec path". Spec and flag
    /// compose in any order; a second positional and a valueless `--base` are hard errors.
    #[test]
    fn parse_workflow_args_reads_spec_and_base() {
        let w =
            |a: &[&str]| parse_workflow_args(&a.iter().map(|s| s.to_string()).collect::<Vec<_>>());

        // Bare: no spec, no base.
        let (spec, base) = w(&[]).unwrap();
        assert!(spec.is_none());
        assert!(base.is_none());

        // Just a spec (the pre-existing behavior).
        let (spec, base) = w(&["spec.md"]).unwrap();
        assert_eq!(spec.as_deref(), Some("spec.md"));
        assert!(base.is_none());

        // `rigger workflow <spec> --base <ref>` and the order-flipped form both parse.
        let (spec, base) = w(&["spec.md", "--base", "my-feature"]).unwrap();
        assert_eq!(spec.as_deref(), Some("spec.md"));
        assert_eq!(base.as_deref(), Some("my-feature"));
        let (spec, base) = w(&["--base", "my-feature", "spec.md"]).unwrap();
        assert_eq!(spec.as_deref(), Some("spec.md"));
        assert_eq!(base.as_deref(), Some("my-feature"));

        // `--base` with no spec is fine (the default spec-less workflow, re-anchored).
        let (spec, base) = w(&["--base", "my-feature"]).unwrap();
        assert!(spec.is_none());
        assert_eq!(base.as_deref(), Some("my-feature"));

        // A second spec path is still the same clear error; a valueless --base names the fix.
        let err = w(&["a.md", "b.md"]).unwrap_err().to_string();
        assert!(
            err.contains("expected at most one spec path"),
            "a second positional must be rejected; got: {err:?}"
        );
        let err = w(&["--base"]).unwrap_err().to_string();
        assert!(
            err.contains("--base expects a ref"),
            "the error must explain --base needs a ref; got: {err:?}"
        );
    }

    /// `rigger step` accepts `--spec` and `--base`: `--base` defaults to `origin/main`,
    /// both flags require a value, and an unknown flag or bare positional is rejected.
    #[test]
    fn parse_step_args_reads_spec_and_base_with_default() {
        let s = |a: &[&str]| parse_step_args(&a.iter().map(|s| s.to_string()).collect::<Vec<_>>());

        // Default base when --base is not given; no spec. The default is NOT flagged as
        // explicit, so steady-state reuse stays silent.
        let a = s(&[]).unwrap();
        assert_eq!(a.base, DEFAULT_BASE_REF);
        assert_eq!(a.base, "origin/main");
        assert!(a.spec.is_none());
        assert!(!a.base_explicit, "an unspecified --base is not explicit");

        // --base overrides the default and is flagged explicit; --spec is read
        // independently and order-free.
        let a = s(&["--base", "rigger-run-1"]).unwrap();
        assert_eq!(a.base, "rigger-run-1");
        assert!(a.spec.is_none());
        assert!(a.base_explicit, "a given --base is explicit");

        let a = s(&["--spec", "specs/04.md", "--base", "origin/next"]).unwrap();
        assert_eq!(a.spec.as_deref(), Some("specs/04.md"));
        assert_eq!(a.base, "origin/next");
        assert!(a.base_explicit);

        // An explicit --base equal to the default is still explicit (so an ignored
        // re-anchor to origin/main is reported, not swallowed as a default).
        let a = s(&["--base", "origin/main"]).unwrap();
        assert_eq!(a.base, "origin/main");
        assert!(a.base_explicit);

        // Each flag requires its value; typos and positionals are hard errors.
        assert!(s(&["--base"]).is_err(), "--base without a value must error");
        assert!(s(&["--spec"]).is_err(), "--spec without a value must error");
        assert!(s(&["--nope"]).is_err(), "an unknown flag must error");
        assert!(s(&["bare"]).is_err(), "a bare positional must error");

        // `--fresh` is a bare boolean flag (off by default), composing with the others.
        assert!(!s(&[]).unwrap().fresh, "--fresh is off unless asked");
        let a = s(&["--fresh", "--spec", "specs/12.md"]).unwrap();
        assert!(a.fresh, "--fresh sets the fresh-restart flag on a step");
        assert_eq!(a.spec.as_deref(), Some("specs/12.md"));

        // `--rebase-definition` (spec 13, unit 1) is likewise a bare boolean, off by default.
        assert!(
            !s(&[]).unwrap().rebase_definition,
            "--rebase-definition is off unless asked"
        );
        let a = s(&["--rebase-definition", "--base", "origin/next"]).unwrap();
        assert!(
            a.rebase_definition,
            "--rebase-definition sets the mid-campaign-edit escape on a step"
        );
        assert_eq!(a.base, "origin/next");
    }

    /// `merge_hung_attention` leaves `attention` untouched - and never evaluates the
    /// (potentially expensive) reason closure - given `newly_hung`, `why` naming the case.
    fn assert_merge_hung_attention_leaves_untouched(
        attention: Vec<ledger::AttentionEntry>,
        newly_hung: bool,
        why: &str,
    ) {
        let merged = merge_hung_attention(attention.clone(), newly_hung, || {
            panic!("the reason closure must not run: {why}")
        });
        assert_eq!(merged, attention, "attention must be untouched: {why}");
    }

    rigger::test_cases! {
        /// Spec 69, criterion 5, signal 2's hung-liveness half (review u69c5 round 3, cause
        /// genuine-defect): `merge_hung_attention` must not fire when there is nothing newly
        /// hung, proving the crossing gate, not just the merge mechanics, since a wrong-way bug
        /// here would restamp on every call exactly like the defect this round fixes.
        merge_hung_attention_does_nothing_when_not_newly_hung:
            assert_merge_hung_attention_leaves_untouched(
                vec![ledger::AttentionEntry::unit_scoped(
                    ledger::ATTENTION_ESCALATED,
                    "u",
                    "escalated after exhausting remediation",
                )],
                false,
                "nothing is newly hung",
            );
        /// A budget halt this same call takes precedence over a co-occurring hung-liveness halt
        /// (mirroring the SAME precedence the `halted` wire field already gives the budget
        /// breaker over its own hung fallback, just above this function's call site in
        /// `cmd_step`) - proving the merge does NOT stamp a second `halted` entry, and does not
        /// evaluate the reason closure, when one is already present.
        merge_hung_attention_defers_to_an_existing_budget_halt:
            assert_merge_hung_attention_leaves_untouched(
                vec![ledger::AttentionEntry::run_scoped(
                    ledger::ATTENTION_HALTED,
                    "budget exhausted: 1/1 spawns",
                )],
                true,
                "a halted entry already exists",
            );
    }

    /// The merge must land the hung-liveness `halted` entry in its CANONICAL position
    /// (escalated, halted, worker-death-recurred, budget-final-tenth, stalled-frontier) even
    /// when `compute_attention` already produced entries both BEFORE and AFTER that slot in
    /// the SAME step (a different unit independently escalating, and a third stalling) - a
    /// naive push-to-the-end would leave `halted` stuck last, violating the wire's
    /// documented deterministic order.
    #[test]
    fn merge_hung_attention_lands_in_canonical_position_alongside_other_signals() {
        let attention = vec![
            ledger::AttentionEntry::unit_scoped(
                ledger::ATTENTION_ESCALATED,
                "e",
                "escalated after exhausting remediation",
            ),
            ledger::AttentionEntry::unit_scoped(
                ledger::ATTENTION_STALLED_FRONTIER,
                "s",
                "3 recorded results, still parked",
            ),
        ];
        let merged =
            merge_hung_attention(attention, true, || "liveness: 1 spawn(s) hung".to_string());
        assert_eq!(
            merged,
            vec![
                ledger::AttentionEntry::unit_scoped(
                    ledger::ATTENTION_ESCALATED,
                    "e",
                    "escalated after exhausting remediation",
                ),
                ledger::AttentionEntry::run_scoped(
                    ledger::ATTENTION_HALTED,
                    "liveness: 1 spawn(s) hung",
                ),
                ledger::AttentionEntry::unit_scoped(
                    ledger::ATTENTION_STALLED_FRONTIER,
                    "s",
                    "3 recorded results, still parked",
                ),
            ],
            "the hung halted entry must be inserted BETWEEN escalated and stalled-frontier, \
             the canonical order, not appended after both"
        );
    }

    /// `cmd_reported` validates its arg count BEFORE any store I/O: exactly one spawn id is
    /// required, so a typo (zero args, or extra args) is a clear error rather than a silent
    /// read of the wrong thing. The single-id read path itself is covered by `result_of_at`
    /// (the testable seam), which `cmd_reported` wraps for I/O + identity + the exit decision.
    #[test]
    fn cmd_reported_requires_exactly_one_id() {
        let none = cmd_reported(&[]).expect_err("no id must be a clear error");
        assert!(
            none.to_string().contains("rigger reported <id>"),
            "the no-id error must show the usage; got: {none}"
        );
        let extra = cmd_reported(&["a".to_string(), "b".to_string()])
            .expect_err("extra args must be a clear error");
        assert!(
            extra.to_string().contains("rigger reported <id>"),
            "the extra-args error must show the usage; got: {extra}"
        );
    }

    /// End-to-end wiring: the dash actually spawned by [`spawn_dash_child_process`] (the raw
    /// launch [`spawn_run_dashboard_detached`] wraps) is placed in its own process group (PGID ==
    /// the spawned pid, a different group than this parent), so the production step path really
    /// does session-detach the always-on dash (spec 44 criterion 3), not merely the seam in
    /// isolation. This calls the raw launch directly, not `spawn_run_dashboard_detached` itself:
    /// under `cargo test`, `current_exe()` is the TEST harness binary, which can never confirm a
    /// bind (see the module doc on `spawn_dash_child_process`), so a test proving detachment must
    /// observe the spawn itself rather than a bind confirmation. The spawned child is deliberately
    /// un-reaped - being un-reaped across steps is the whole point of "detached" - and the OS
    /// reaps this transient child when the test process exits.
    #[cfg(target_os = "linux")]
    #[test]
    fn spawn_run_dashboard_detached_session_detaches_the_dash() {
        let parent_pgid = pgid_of(std::process::id());

        let port = dash::free_port_from(40000).expect("a free loopback port must be available");
        let (child, pid) = spawn_dash_child_process(port).expect("spawn the detached dash");
        let dash_pgid = pgid_of(pid);

        assert_eq!(
            dash_pgid, pid,
            "the spawned dash is its own process-group leader (PGID == its PID)"
        );
        assert_ne!(
            dash_pgid, parent_pgid,
            "the spawned dash is in a DIFFERENT process group than the step process that spawned \
             it - so tearing down the step command's process group does not reap the dash"
        );

        // Deliberately un-reaped: `Child`'s Drop neither waits nor kills, matching production -
        // being un-reaped across steps is the whole point of "detached".
        drop(child);
    }
}
