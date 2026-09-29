use super::*;

/// `rigger stats` - print the operator metrics for the current project's run: the
/// implement -> review loop's first-pass yield, per-gate remediation (pass/fail)
/// counts, escalation rate, and review approve/reject counts.
///
/// Composition mirrors `run_cli` (decision `d-stats-namespace`): resolve this project's
/// identity and `.rigger/events.db` path, then delegate to [`stats_lines`], which opens
/// the db via [`Store`], wraps it in the per-project [`Namespaced`] decorator, reads the
/// conductor's run stream ([`conductor::STREAM`]) forward - the same stream and boundary
/// the conductor itself replays its run state from - and folds it through the pure
/// [`metrics::project`] read-model.
///
/// Both no-run edges (absent db, empty namespaced run stream) come back from
/// [`stats_lines`] as `None` and print the same clear "no runs yet" message instead of
/// an empty table or a panic (decision `d-stats-absent-guard`); see that function for
/// the per-edge rationale.
///
/// `rigger stats` takes no arguments; any extra argument is a clear error.
pub(crate) fn cmd_stats(args: &[String]) -> Res {
    // `rigger stats` reports the LATEST run; `rigger stats --all` reports the historical
    // aggregate over every run in the store (spec 06, unit 1); `rigger stats --canary`
    // reports the judge-the-judges scorecard of the latest canary run (spec 13, unit 5).
    // No other argument is accepted.
    if let [flag] = args {
        if flag == "--canary" {
            return cmd_stats_canary();
        }
    }
    let all = match args {
        [] => false,
        [flag] if flag == "--all" => true,
        _ => {
            return Err(format!(
                "stats: expected no arguments, --all, or --canary, got {}",
                args.join(" ")
            )
            .into())
        }
    };

    // Resolve the project identity and db path the same way every CLI command does,
    // then delegate the namespace-scoped read + no-runs decision to `stats_lines`. This
    // wrapper owns only the I/O boundary (which file, which project, and the printing);
    // the read-model edges live in the testable seam below.
    let selection = store_selection(None, None)?;
    match stats_lines(&db_path("events.db"), &project_identity(), all, &selection)? {
        Some(lines) => {
            for line in lines {
                println!("{line}");
            }
        }
        // No run to report on - absent db (never-run project) or an empty namespaced
        // run stream. One clear message for both edges.
        None => println!("{NO_RUNS_MESSAGE}"),
    }
    Ok(())
}

/// The message `rigger stats --canary` prints when no canary run has been recorded yet -
/// either the project has never run (no `events.db`) or its canary stream is empty.
const NO_CANARY_MESSAGE: &str =
    "# Rigger: no canary run recorded yet (run `rigger canary` to score the review panel \
     against the corpus, then `rigger stats --canary`).";

/// `rigger stats --canary` (spec 13, unit 5): report the judge-the-judges scorecard of the
/// LATEST canary run - per-tier catch rate, adjudicator correctness, and finding-order
/// stability - folded from the project's DISTINCT canary stream (never the run stream).
fn cmd_stats_canary() -> Res {
    match canary_stats_lines(&db_path("events.db"), &project_identity())? {
        Some(lines) => {
            for line in lines {
                println!("{line}");
            }
        }
        None => println!("{NO_CANARY_MESSAGE}"),
    }
    Ok(())
}

/// The pure read-model core of `rigger stats --canary`: open the embedded `events.db`,
/// read `project`'s namespaced `canary` stream, and fold it into the printable canary
/// scorecard - `None` for the two "no canary run yet" edges (absent db / empty stream),
/// so [`cmd_stats_canary`] prints one clear message for both. Split out for the same
/// reason [`stats_lines`] is: the namespace-scoped read is unit-testable off the process
/// cwd.
fn canary_stats_lines(
    path: &str,
    project: &str,
) -> Result<Option<Vec<String>>, Box<dyn std::error::Error>> {
    let sel = store_selection(None, None)?;
    let events = read_project_stream(path, project, canary::STREAM, &sel)?.unwrap_or_default();
    if events.is_empty() {
        return Ok(None);
    }
    // `project_canary` scopes internally to the latest canary run (its batch marker).
    Ok(Some(format_canary_stats(&metrics::project_canary(&events))))
}

/// Render `rigger status`'s dashboard line (spec 69, criterion 4: "`rigger status` never lies
/// about the dash"). `None` when nothing was ever recorded ([`dash::DashStatus::Absent`]) - the
/// unchanged silent case. A trusted URL prints EXACTLY as before
/// ([`dash::DashStatus::Serving`]). A probe PROVED the recorded URL dead prints the truthful
/// line instead - naming the matching marker's pid when one is known, or naming none when only
/// a mismatched marker (or none at all) was recorded (round 3: a mismatched marker's pid
/// belongs to an unrelated dash and must never be printed as though it were this URL's) - plus
/// both self-heal paths, so an operator is never sent chasing a URL nothing answers.
fn dash_status_line(status: &dash::DashStatus) -> Option<String> {
    match status {
        dash::DashStatus::Absent => None,
        dash::DashStatus::Serving(url) => Some(format!("dashboard: {url}")),
        dash::DashStatus::NotServing { pid: Some(pid) } => Some(format!(
            "dashboard: not serving (marker names dead pid {pid}) - run 'rigger dash' or the \
             next step restarts it"
        )),
        dash::DashStatus::NotServing { pid: None } => Some(
            "dashboard: not serving (recorded url is unreachable) - run 'rigger dash' or the \
             next step restarts it"
                .to_string(),
        ),
        dash::DashStatus::Unresponsive { url, pid } => {
            let pid = pid.map(|p| format!(" (pid {p})")).unwrap_or_default();
            Some(format!(
                "dashboard: {url}{pid} did not answer within {}ms - busy, not dead",
                dash::DASH_PROBE_WINDOW_MS
            ))
        }
    }
}

/// Render `rigger status --json`'s dashboard entry (spec 69, criterion 4's third clause:
/// "`--json` carries the same truth"), the JSON sibling of [`dash_status_line`]. `None` for
/// [`dash::DashStatus::Absent`] - nothing to append, mirroring the text render's silent case.
/// A trusted URL and a proven-dead marker each render as a small, self-describing object
/// under a `"dashboard"` key - never a bare string or a shape that could be mistaken for an
/// [`progress::AgentActivity`] entry - so the caller can tell it apart from an agent entry by
/// its keys alone. `pid` serializes as `null` when no matching marker named one (round 3).
fn dash_status_json(status: &dash::DashStatus) -> Option<serde_json::Value> {
    let dashboard = match status {
        dash::DashStatus::Absent => return None,
        dash::DashStatus::Serving(url) => serde_json::json!({"status": "serving", "url": url}),
        dash::DashStatus::NotServing { pid } => {
            serde_json::json!({"status": "not_serving", "pid": pid})
        }
        dash::DashStatus::Unresponsive { url, pid } => {
            serde_json::json!({"status": "unresponsive", "url": url, "pid": pid})
        }
    };
    Some(serde_json::json!({ "dashboard": dashboard }))
}

/// `rigger progress <id> "<activity>"` - record one live progress report for spawn `<id>`
/// to the SEPARATE progress store (`.rigger/progress.db`), stamped with the current run
/// (spec 14, Gap 27). `<activity>` is a short one-line description of what the agent just
/// did (a grep, a build, a commit, a decision). The report NEVER touches the run stream -
/// it lands in its own store, so replay stays byte-identical - and rigger reads it back
/// (the consolidator) to PRESENT a live per-agent view. A pure append: the run is resolved
/// read-only from the run store to scope the report, and only the progress store is written.
/// Routed through [`require_store_dir`] like the other courier commands, so a worker running
/// it from a nested worktree records into the project's real store, never a misfiled one.
pub(crate) fn cmd_progress(args: &[String]) -> Res {
    let id = args
        .first()
        .ok_or("progress: expected a spawn id: rigger progress <id> \"<activity>\"")?;
    let activity = args
        .get(1)
        .ok_or("progress: expected an activity: rigger progress <id> \"<activity>\"")?;
    if args.len() > 2 {
        return Err(format!(
            "progress: expected an id and a single activity string, got {} arguments",
            args.len()
        )
        .into());
    }
    if activity.trim().is_empty() {
        return Err("progress: <activity> must be non-empty".into());
    }

    let (loc, selection) = require_store_dir()?;
    // Spec 62 (couriers count as activity): re-stamp this project's registry heartbeat before
    // the real work below, so the instance stays discoverable even if this progress report is
    // the only traffic in the run for a while. Best-effort/warn-only; never fails the report.
    refresh_registry_entry(&loc, &selection);
    // Resolve the current run READ-ONLY from the run store, only to scope the report.
    let run_backend = resolve_store(&selection, &loc.file("events.db"))?;
    let run_store = Namespaced::new(run_backend.as_ref(), &loc.identity());
    let (_, run_id) = runscope::read::read_current_run(&run_store, conductor::STREAM)?;
    // Append to the SEPARATE progress store - never the run stream.
    let prog_backend = Store::open(&loc.file("progress.db"))?;
    let prog_store = Namespaced::new(&prog_backend, &loc.identity());
    let pos = rigger::progress_store::record(&prog_store, &run_id, id, activity)?;
    println!("progress recorded for {id} (position {pos})");
    Ok(())
}

/// Spec 94 CONSTRAINTS WALK's own empty-store text ("no run recorded; start one with `rigger
/// run <spec>`"): the ONE placeholder [`cmd_status`]'s `--line` path renders for the literal
/// state `rigger setup` itself leaves every project in (no `.rigger/events.db` anywhere yet,
/// until the first `rigger run` - or again after a store reset), so this line and the future
/// console page's identical empty-store render (specs 95-98) are never two independently-
/// worded copies of the same fact.
const NO_RUN_RECORDED_LINE: &str = "no run recorded; start one with `rigger run <spec>`";

/// `rigger status [--json|--line]` - present the live per-agent view of the current run (spec
/// 14, unit 2). Rigger CONSOLIDATES its three signals for every in-flight spawn - the
/// run-stream milestone, the latest progress report, and the liveness-marker age it reads in
/// Rust here (so no consumer stats a file) - into one view: what each agent is at, what it is
/// doing, how long since its last activity and heartbeat, and how long since its last store
/// event (the blackout this closes). `--json` prints the machine shape the shim and the dash
/// also consume; `--line` prints ONLY the core's one-line statusline (spec 94, criterion 5 -
/// the same command `rigger setup` registers as the editor's status bar); the default is a
/// readable table. Read-only over the run store, the separate progress store, and the
/// liveness markers.
pub(crate) fn cmd_status(args: &[String]) -> Res {
    let mut json = false;
    // Spec 94, criterion 5 (THE STATUSLINE COMMAND): `--line` prints ONLY the core's
    // `console::statusline` text - the same line the human-readable path below prints as its
    // own first line - for an editor's status bar (`rigger setup` registers this exact
    // command; see `install_status_line`). Mutually exclusive with `--json`: the two are
    // different output modes for the same command, never composed.
    let mut line = false;
    for a in args {
        match a.as_str() {
            "--json" => json = true,
            "--line" => line = true,
            other => {
                return Err(
                    format!("status: unknown argument {other:?} (only --json or --line)").into(),
                )
            }
        }
    }
    if json && line {
        return Err("status: --line and --json are mutually exclusive".into());
    }
    // Spec 94 criterion 5 round 2 (checkin adjudication reject on
    // `adv-u94c5-setup-wires-a-command-that-hard-fails-with-no-run-yet`): `--line` is the
    // exact command `rigger setup` registers as the editor's status line, so an editor polls
    // it automatically and unconditionally - with no human present to read a hard error. The
    // literal state `rigger setup` itself leaves every project in (no `.rigger/events.db`
    // anywhere yet, until the first `rigger run`, and again after any store reset) must
    // render [`NO_RUN_RECORDED_LINE`] with exit 0 instead of propagating
    // `require_store_dir`'s refusal - the SAME graceful degradation the console's own
    // empty-store shell takes (spec 94 CONSTRAINTS WALK), never a second wording. Matched by
    // DOWNCASTING on [`NoStoreFound`] specifically (never a string match on its message), so
    // every OTHER `--line` failure (a corrupt store, an unreadable stream) still propagates
    // exactly as before this round - only the no-store-at-all case degrades. The default and
    // `--json` paths below are human-invoked and keep the CLI's existing hard refusal
    // unchanged: this fast path is `--line`-only.
    let (loc, selection) = if line {
        match require_store_dir() {
            Ok(pair) => pair,
            Err(e) if e.downcast_ref::<NoStoreFound>().is_some() => {
                println!("{NO_RUN_RECORDED_LINE}");
                return Ok(());
            }
            Err(e) => return Err(e),
        }
    } else {
        require_store_dir()?
    };
    let now = std::time::SystemTime::now();

    // The current run's slice of the run stream, and its id.
    let run_backend = resolve_store(&selection, &loc.file("events.db"))?;
    let run_store = Namespaced::new(run_backend.as_ref(), &loc.identity());
    let (run_slice, run_id) = runscope::read::read_current_run(&run_store, conductor::STREAM)?;
    let run_events = run_slice.as_slice();

    // Spec 94, criterion 5: `--line` needs only the run's event slice and the configured
    // remediation bound - console::fold's own three inputs (unit statuses, current blockers,
    // the dock) - so it short-circuits HERE, before the progress-store read, the liveness
    // marker stat and the dash-server probe below, none of which the statusline text depends
    // on. An editor polling this command on every render stays cheap. `console::fold`, not a
    // second independently-composed copy, so a repeated poll can never drift from `rigger
    // status`'s own first line.
    if line {
        let (_, max_retries) = scratch_defaults(&loc);
        let console_state = console::fold(run_events, max_retries)?;
        println!("{}", console_state.statusline);
        return Ok(());
    }

    // This run's progress, from the SEPARATE store's per-run stream (absent/empty is fine - the
    // store is created lazily by the first `rigger progress`).
    let prog_events: Vec<Event> = match Store::open(&loc.file("progress.db")) {
        Ok(backend) => read_run_progress(&backend, &loc.identity(), &run_id),
        Err(_) => Vec::new(),
    };

    // Liveness ages: rigger stats each in-flight spawn's marker IN RUST here (this is what
    // the JS driver's haiku probe was reconstructing by proxy - unit 3 retires it). The
    // configured remediation bound is read from the SAME config (via the shared
    // `scratch_defaults`, spec 83 criterion 2 round 2) so the current-blocker classifier's
    // `#n/max` line matches the depth the run actually escalates at.
    let (workdir, max_retries) = scratch_defaults(&loc);
    let wave = spawn::step_result(run_events)?.wave;
    let liveness_ages: std::collections::HashMap<String, u64> =
        liveness_ages_for_wave(&loc.repo_root(), &workdir, &run_id, &wave, now)
            .into_iter()
            .collect();

    let view = progress::consolidate(run_events, &prog_events, &liveness_ages, now)?;

    // Spec 69, criterion 4: never printed (or, under `--json`, ever wired) on trust alone.
    // [`dash::dash_status`] probes with [`dash::dash_answer_on`] directly - the SAME
    // underlying probe [`dash_marker_serving`] wraps for the step path's own idempotent-start
    // decision, so this surface and the step path can never disagree about whether a recorded
    // dash is alive. A marker-less recorded URL (the guard-bound `rigger run` / `rigger
    // serve` dash writes none) is unverifiable, not dead, and stays trusted exactly as before
    // this criterion; a marker naming a DIFFERENT dash than the recorded URL (round 3,
    // adv-u69c4r2-mismatched-marker-still-trusts-a-dead-url) is NOT "nothing to check" either -
    // `dash_status` probes the URL's own port directly in that case, never the marker's
    // unrelated one. Computed HERE, before the `--json` early return below, so `--json`
    // carries the same truth as the human table (round 2: the prior placement after the
    // `--json` return left `--json` structurally unable to compute it at all).
    let dash_marker = dash::DashMarker::read(Path::new(&loc.file(DASH_MARKER_FILE)));
    let dash_status = dash::dash_status(recorded_dash_url(&loc), dash_marker, dash::dash_answer_on);

    if json {
        // The dashboard truth is APPENDED to the in-flight-agent array rather than wrapping
        // it in a new top-level shape: the liveness courier (workflows/rigger.js:218) does
        // `Array.isArray(arr) ? arr.find(a => a.id === id) : null` over this exact line, so
        // the top level MUST stay a bare array - wrapping it in an object would silently and
        // permanently blind spec 10/14's liveness watchdog (`find` never runs, every
        // liveness read becomes `null`, "conservatively" treated as never-stale forever).
        // Absent (nothing ever recorded) appends nothing, so a project with no dash history
        // gets byte-identical `--json` output to before this criterion; the appended object
        // carries no `id` field, so it can never collide with a real spawn id.
        let mut value = serde_json::to_value(&view)?;
        if let (Some(arr), Some(dashboard)) = (value.as_array_mut(), dash_status_json(&dash_status))
        {
            arr.push(dashboard);
        }
        println!("{}", serde_json::to_string(&value)?);
        return Ok(());
    }

    // ONE FOLD (spec 93, criterion 4): the console's own fold over the run's event slice -
    // unit statuses, the current-blocker lines (the SAME shared classifier the dashboard also
    // renders, spec 19a), the needs-you dock and the statusline - so `rigger status` prints
    // exactly what the future console page will render from the identical stream, never a
    // second, independently-composed copy of any of the four. Computed even when no agent is
    // parked, so an escalated unit or a budget halt (which have no live spawn) is still
    // surfaced.
    let console_state = console::fold(run_events, max_retries)?;

    // The ready-to-release handoff (spec 38, criterion 3): surfaced on this status surface
    // when the run is DONE (every unit integrated, no failed deferred gate), naming the run
    // branch, the release-target base, the integrated-unit count, and the PR command. Empty
    // for a run that is not done, so an unfinished run surfaces NO release-ready signal. The
    // base is the one PERSISTED on this run's RunStarted, so status names the SAME base the run
    // anchored on - `rigger status` runs without the run's `--base` flag on its argv and so
    // cannot re-resolve it. A run predating base persistence (or with no repo) carries none, so
    // fall back to the `RIGGER_BASE` env / load-bearing default. A done run has no live spawns,
    // so this must also print in the no-agents-in-flight branch below, never only in the
    // agents-in-flight path.
    let release_base = runscope::current_run_base(run_events)
        .unwrap_or_else(|| resolve_run_base(None, std::env::var("RIGGER_BASE").ok().as_deref()).0);
    let release_lines = release_ready_lines(run_events, RUN_BRANCH, &release_base);

    // The auto-started dash's URL for this run (spec 19b, unit 1 discoverability): shown
    // whenever a driver recorded one, even for an otherwise-quiet run, so an operator can
    // always find the live observability page. Printed before the run summary so it appears
    // in the "no agents in flight" case too.
    if let Some(line) = dash_status_line(&dash_status) {
        println!("{line}");
    }

    // The run-id header (spec 82, criterion 1's truncation authority): printed only when a
    // `RunStarted` is on the stream, so a legacy or run-less read stays exactly as bare as the
    // statusline below leaves it. Kept as ITS OWN line, separate from the statusline, so the
    // statusline's text - the SAME text a later criterion's `rigger status --line` and the
    // console page print - never carries a run id the page's own instance selector already
    // shows.
    if !run_id.is_empty() {
        println!("run {}", ledger::short_run_id(&run_id));
    }

    // The statusline (spec 93, criterion 4): the console's own one-line summary, THE first
    // line `rigger status` prints - the same text a later criterion's `rigger status --line`
    // prints alone for an editor status bar, so the terminal table and that one-liner are
    // never two derivations. Printed unconditionally, even for a run holding no units at all,
    // so the line is always there to anchor the table under it.
    println!("{}", console_state.statusline);

    // The per-agent "doing" detail, unchanged from before this criterion: one block per
    // in-flight spawn. The blackout is visible as `last store event` age >> activity age.
    if !view.is_empty() {
        let age = |s: Option<u64>| s.map(|s| format!("{s}s ago")).unwrap_or_else(|| "-".into());
        for a in &view {
            println!("  {} [{}]", a.id, a.stage);
            println!(
                "      doing: {} ({}) | heartbeat {} | last store event: {} ({})",
                a.latest_activity
                    .as_deref()
                    .unwrap_or("(none reported yet)"),
                age(a.activity_age_s),
                a.liveness_age_s
                    .map(|s| format!("{s}s ago"))
                    .unwrap_or_else(|| "-".into()),
                a.last_milestone.as_deref().unwrap_or("-"),
                age(a.milestone_age_s),
            );
        }
    }

    // The dock's needs-you section (spec 93, criterion 4): every CURRENT outstanding
    // condition an operator must act on, from `console::fold`'s own dock - printed even when
    // empty, with the design's own reassurance line (spec 93 Design, "6.8"), so a quiet run
    // never leaves an operator wondering whether the check ran at all.
    println!("needs you:");
    if console_state.dock.needs_you.is_empty() {
        println!("  Nothing needs a human right now. Walk away - you will be tapped.");
    } else {
        for line in console_state.dock.lines() {
            println!("  {line}");
        }
    }

    if !console_state.blockers.is_empty() {
        println!("current blockers:");
        for line in &console_state.blockers {
            println!("  {line}");
        }
    }
    // Non-empty only when the run is done.
    for line in &release_lines {
        println!("{line}");
    }
    Ok(())
}

/// The ready-to-release handoff lines `rigger status` prints (spec 38, criterion 3): empty
/// for any run that is NOT done, else the summary naming the run branch, the release-target
/// base, the integrated-unit count, and the exact PR command. Pure over the run's event
/// slice plus the resolved run branch/base, so it is unit-testable without a store and
/// renders identically wherever it is surfaced - the single authority is
/// [`ledger::RunState::release_ready`] + [`ledger::ReleaseReady::lines`], never a second
/// derivation. A projection hiccup yields no lines rather than failing the status read.
fn release_ready_lines(run_events: &[Event], run_branch: &str, base: &str) -> Vec<String> {
    ledger::project(run_events)
        .ok()
        .and_then(|rs| rs.release_ready(run_branch, base))
        .map(|rr| rr.lines())
        .unwrap_or_default()
}

/// `rigger watch [--interval <s>] [--once]` (spec 69, criterion 2): the
/// driver-independent watchdog. Gathers the store, process-table, and status truth
/// [`watch::detect`] needs and prints one line per anomaly - naming signal, subject,
/// and response - so an orchestrator armed on this command sees exactly what a
/// manual `rigger-watch-a-run` look would, without polling anything by hand.
///
/// `--once` prints the CURRENT standing anomalies and exits (the cron/CI shape): a poll
/// failure here (no store, a genuinely unreadable one, a bad flag upstream) propagates and
/// exits non-zero, exactly like every other one-shot courier command.
///
/// Without it, this STREAMS: poll, print only what is new or has worsened since the last poll
/// (in-process [`watch::Dedup`] - spec 69 Design: "dedup state lives in process memory
/// only"), sleep `--interval` seconds (default [`watch::DEFAULT_INTERVAL_SECS`]), and repeat
/// forever - the harness's background monitor is the intended host for this loop. A poll
/// failure while streaming is FAIL-SOFT, not fatal: reported on stderr and retried on the
/// next tick, never propagated out of the process. This is the whole point named by this
/// command's own Design text ("it must work with the driver dead" - the watchdog reads only
/// store, process table, and status, never the driver, exactly the process that may be dead)
/// carried one step further: a watchdog armed unattended must also outlive a TRANSIENT fault
/// in the very store it reads (a torn read racing a concurrent writer, a momentarily locked
/// file) rather than itself becoming the thing that silently stops monitoring. Matches every
/// other fallible read [`watch_poll`] already performs beyond the store (the shared
/// `scratch_defaults` config probe, the step-lock probe, the liveness-marker stat, the
/// dash-marker read) - all deliberately fail-soft; only the store reads used to be the
/// exception.
pub(crate) fn cmd_watch(args: &[String]) -> Res {
    let WatchArgs {
        once,
        interval_secs,
    } = parse_watch_args(args)?;

    let mut dedup = watch::Dedup::new();
    loop {
        let anomalies =
            match require_store_dir().and_then(|(loc, selection)| watch_poll(&loc, &selection)) {
                Ok(anomalies) => anomalies,
                Err(e) if once => return Err(e),
                Err(e) => {
                    eprintln!("rigger: watch: poll failed, will retry: {e}");
                    std::thread::sleep(std::time::Duration::from_secs(interval_secs.max(1)));
                    continue;
                }
            };
        let to_print = if once {
            anomalies
        } else {
            dedup.step(anomalies)
        };
        for a in &to_print {
            println!("{}", a.line());
        }
        if once {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_secs(interval_secs.max(1)));
    }
}

/// `rigger peers [<file> ...]` - print the peer decisions, lessons, and review findings
/// from the context graph scoped to the given files (or all if none), EXACTLY as the MCP
/// `rigger_peers` tool does (both render through [`mcpserver::peers_json`]). The store
/// is RESOLVED by walking up to the project's existing `.rigger` (refusing to fabricate
/// one, spec 05 - see [`require_store_dir`]); the side-car reads the `conductor::STREAM` run
/// from its boundary with the carried-over knowledge by type, and this command renders one
/// readable line per decision / lesson / finding. Rendering the lessons here is what makes the
/// capped prompt sections' "recover the full set with `rigger peers <file>`" note honest
/// for the lessons section, not just decisions and findings (adj-u1gap17).
pub(crate) fn cmd_peers(args: &[String]) -> Res {
    let files: Vec<String> = args.to_vec();

    let (loc, selection) = require_store_dir()?;
    let backend = resolve_store(&selection, &loc.file("events.db"))?;
    let store = Namespaced::new(backend.as_ref(), &loc.identity());

    // The side-car reads the run from its boundary and the carried-over decisions, lessons and
    // findings by type (spec 101) - every record committed before this call, never the whole
    // log.
    let peers = Sidecar::read(&store, conductor::STREAM)?;

    let result = mcpserver::peers_json(&peers, &files);
    let decisions = result["decisions"].as_array().cloned().unwrap_or_default();
    let lessons = result["lessons"].as_array().cloned().unwrap_or_default();
    let findings = result["findings"].as_array().cloned().unwrap_or_default();
    for d in &decisions {
        println!("{}", peer_decision_line(d));
    }
    for l in &lessons {
        let id = l["id"].as_str().unwrap_or_default();
        let summary = l["summary"].as_str().unwrap_or_default();
        let about = json_str_array(&l["about"]);
        println!("lesson {id} | {summary} | about: {about}");
    }
    for f in &findings {
        let id = f["id"].as_str().unwrap_or_default();
        let by = f["by"].as_str().unwrap_or_default();
        let summary = f["summary"].as_str().unwrap_or_default();
        let about = json_str_array(&f["about"]);
        println!("finding {id} | by {by} | {summary} | about: {about}");
    }
    Ok(())
}

/// Render one `rigger peers` decision line, labeling its provenance LIVE (from the
/// active run) or HISTORICAL (a superseded run, or pre-boundary) from the `live` flag the
/// side-car derived via the single c1 run attribution (spec 21, unit 3). The label makes
/// a prior run's decision legible instead of alarming; grounding still surfaces cross-run
/// decisions unchanged. A missing/false `live` flag renders HISTORICAL - the conservative
/// default that matches the side-car's own default.
fn peer_decision_line(d: &serde_json::Value) -> String {
    let id = d["id"].as_str().unwrap_or_default();
    let summary = d["summary"].as_str().unwrap_or_default();
    let governs = json_str_array(&d["governs"]);
    let provenance = if d["live"].as_bool().unwrap_or(false) {
        "LIVE"
    } else {
        "HISTORICAL"
    };
    format!("decision {id} | {provenance} | {summary} | governs: {governs}")
}

/// Join a JSON array of strings into a comma-separated list for a `rigger peers`
/// line (the `governs` / `about` files). A non-array or empty value renders as `-`.
fn json_str_array(v: &serde_json::Value) -> String {
    match v.as_array() {
        Some(a) if !a.is_empty() => a
            .iter()
            .filter_map(|x| x.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        _ => "-".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::CwdGuard;

    /// Spec 69, criterion 4: `dash_status_line` renders each [`dash::DashStatus`] outcome to
    /// the EXACT text `rigger status` prints - `Absent` prints nothing (today's silent
    /// no-recorded-dash case), `Serving` prints the unchanged `dashboard: <url>` line, a
    /// `NotServing` with a known pid (a MATCHING marker proven dead) prints the truthful line
    /// naming that pid and both self-heal paths, and a `NotServing` with no pid (round 3: a
    /// mismatched-or-absent marker, the url's own port directly proven dead) prints the
    /// truthful line WITHOUT fabricating a pid - never a stale URL either way.
    #[test]
    fn dash_status_line_renders_each_outcome_to_its_exact_text() {
        assert_eq!(
            dash_status_line(&dash::DashStatus::Absent),
            None,
            "no recorded dash prints nothing"
        );
        assert_eq!(
            dash_status_line(&dash::DashStatus::Serving("http://127.0.0.1:7420/".into())),
            Some("dashboard: http://127.0.0.1:7420/".to_string()),
            "a trusted URL prints exactly as before this criterion"
        );
        assert_eq!(
            dash_status_line(&dash::DashStatus::NotServing { pid: Some(4242) }),
            Some(
                "dashboard: not serving (marker names dead pid 4242) - run 'rigger dash' or \
                 the next step restarts it"
                    .to_string()
            ),
            "a matching marker proven dead names the pid and both self-heal paths, never a \
             stale URL"
        );
        assert_eq!(
            dash_status_line(&dash::DashStatus::NotServing { pid: None }),
            Some(
                "dashboard: not serving (recorded url is unreachable) - run 'rigger dash' or \
                 the next step restarts it"
                    .to_string()
            ),
            "a directly-proven-dead url with no matching marker names both self-heal paths \
             without fabricating a pid"
        );
        assert_eq!(
            dash_status_line(&dash::DashStatus::Unresponsive {
                url: "http://127.0.0.1:7420/".into(),
                pid: Some(4242),
            }),
            Some(
                "dashboard: http://127.0.0.1:7420/ (pid 4242) did not answer within 750ms - \
                 busy, not dead"
                    .to_string()
            ),
            "a held port that did not answer in the window is busy, never reported dead"
        );
    }

    /// Spec 69, criterion 4's third clause ("`--json` carries the same truth"): the JSON
    /// sibling of the text-render test above, over the same [`dash::DashStatus`] outcomes.
    /// `Absent` appends nothing (round 2: this is what closes the "vacuously true" gap - before
    /// round 2, `--json` never even reached a computed [`dash::DashStatus`], so this branch and
    /// the informative ones below were indistinguishable).
    #[test]
    fn dash_status_json_renders_each_outcome_to_its_exact_shape() {
        assert_eq!(
            dash_status_json(&dash::DashStatus::Absent),
            None,
            "no recorded dash appends nothing to the --json array"
        );
        assert_eq!(
            dash_status_json(&dash::DashStatus::Serving("http://127.0.0.1:7420/".into())),
            Some(serde_json::json!({
                "dashboard": {"status": "serving", "url": "http://127.0.0.1:7420/"}
            })),
            "a trusted URL carries its status and url"
        );
        assert_eq!(
            dash_status_json(&dash::DashStatus::NotServing { pid: Some(4242) }),
            Some(serde_json::json!({
                "dashboard": {"status": "not_serving", "pid": 4242}
            })),
            "a matching marker proven dead carries its status and the dead pid, never a URL"
        );
        assert_eq!(
            dash_status_json(&dash::DashStatus::NotServing { pid: None }),
            Some(serde_json::json!({
                "dashboard": {"status": "not_serving", "pid": null}
            })),
            "a directly-proven-dead url with no matching marker carries null, never a \
             fabricated pid"
        );
        assert_eq!(
            dash_status_json(&dash::DashStatus::Unresponsive {
                url: "http://127.0.0.1:7420/".into(),
                pid: None,
            }),
            Some(serde_json::json!({
                "dashboard": {"status": "unresponsive", "url": "http://127.0.0.1:7420/", "pid": null}
            })),
            "a held port that did not answer in the window carries its url, never not_serving"
        );
    }

    /// spec 21, unit 3: the exact line `rigger peers` PRINTS for a decision must label its
    /// provenance - LIVE for a decision from the active run, HISTORICAL for one from a
    /// superseded run - from the `live` flag the shared `peers_json` core threads through.
    /// Asserting on the rendered line (not just the JSON) closes the gap where the printed
    /// label was untested (sdet-u21peers-cmdpeers-render-label-untested).
    #[test]
    fn cmd_peers_prints_live_or_historical_per_decision_provenance() {
        let live = peer_decision_line(&serde_json::json!({
            "id": "d_new", "summary": "chose X", "governs": ["a.rs"], "live": true,
        }));
        assert_eq!(live, "decision d_new | LIVE | chose X | governs: a.rs");

        let historical = peer_decision_line(&serde_json::json!({
            "id": "d_old", "summary": "chose Y", "governs": ["b.rs"], "live": false,
        }));
        assert_eq!(
            historical,
            "decision d_old | HISTORICAL | chose Y | governs: b.rs"
        );

        // A missing `live` flag renders HISTORICAL - the conservative default.
        let defaulted = peer_decision_line(&serde_json::json!({
            "id": "d_bare", "summary": "z", "governs": [],
        }));
        assert_eq!(defaulted, "decision d_bare | HISTORICAL | z | governs: -");
    }

    /// Spec 38, criterion 3 (the ready-to-release handoff): the exact lines the `rigger
    /// status` surface prints are non-empty and name the run branch, the release-target base,
    /// the integrated-unit count, and the PR command ONLY when the run is done; a run that is
    /// NOT done surfaces no release-ready signal. Proven over the production render seam
    /// (`release_ready_lines`) `cmd_status` prints, so the surface cannot silently drift from
    /// the one authority.
    ///
    /// Spec 82, criterion 1 (the status handoff is unique): the PR command is the two-command
    /// unique-head flow - the head derived from the run's OWN RunStarted (spec stem +
    /// run-short-id), never the literal run branch as `--head`.
    #[test]
    fn release_ready_lines_surface_only_on_a_done_run() {
        // A done run: one integrated unit, no failed deferred gate.
        let done = [
            Event::new(
                runscope::TYPE_RUN_STARTED,
                br#"{"run":"7ad52031-01f1-4d37-aa19-ad48090f84a5","spec":"specs/82-unique-pr-heads.md"}"#
                    .to_vec(),
            ),
            Event::new(ledger::TYPE_UNIT_STARTED, br#"{"id":"u1"}"#.to_vec()),
            Event::new(
                ledger::TYPE_UNIT_INTEGRATED,
                br#"{"id":"u1","commit":"abc"}"#.to_vec(),
            ),
        ];
        let lines = release_ready_lines(&done, RUN_BRANCH, DEFAULT_BASE_REF);
        assert!(
            !lines.is_empty(),
            "a done run surfaces the release-ready handoff"
        );
        let text = lines.join("\n");
        assert!(text.contains(RUN_BRANCH), "names the run branch: {text}");
        assert!(
            text.contains("1 unit"),
            "names the integrated-unit count: {text}"
        );
        // `origin/main` is stripped to the release-target branch in the PR command, and the
        // head is the per-run-unique `pr/<spec-stem>-<run-short-id>` branch - never the run
        // branch itself.
        let head = "pr/82-unique-pr-heads-7ad52031-01f";
        assert!(
            text.contains(&format!("git push origin {RUN_BRANCH}:{head}")),
            "names the push command: {text}"
        );
        assert!(
            text.contains(&format!("gh pr create --base main --head {head}")),
            "names the PR command: {text}"
        );
        assert!(
            !text.contains("--head rigger-run"),
            "the literal `--head <run_branch>` form must never appear: {text}"
        );

        // A run with a still-un-integrated unit surfaces NO release-ready signal.
        let running = [
            Event::new(ledger::TYPE_UNIT_STARTED, br#"{"id":"u1"}"#.to_vec()),
            Event::new(
                ledger::TYPE_UNIT_INTEGRATED,
                br#"{"id":"u1","commit":"abc"}"#.to_vec(),
            ),
            Event::new(ledger::TYPE_UNIT_STARTED, br#"{"id":"u2"}"#.to_vec()),
        ];
        assert!(
            release_ready_lines(&running, RUN_BRANCH, DEFAULT_BASE_REF).is_empty(),
            "an unfinished run surfaces no release-ready signal"
        );
    }

    #[test]
    fn status_and_dash_read_the_runs_persisted_base_not_a_re_resolution() {
        // The base-asymmetry fix (spec 38, criterion 3): a run started with an explicit
        // `--base` and NO `RIGGER_BASE` in the environment persists that base as `META_BASE`
        // on its RunStarted. `rigger status` and `rigger dash` run WITHOUT the run's `--base`
        // flag on their argv, so before this fix they re-resolved via `resolve_run_base(None,
        // env)` and named the WRONG default base for a `rigger run --base X` run. Now every
        // surface reads the ONE persisted base, so all of status/dash/print_run_state name the
        // base the run actually anchored on.

        // `rigger run --base release/2.0` with no `RIGGER_BASE` resolves to the flag base...
        let (flag_base, explicit) = resolve_run_base(Some("release/2.0"), None);
        assert_eq!(flag_base, "release/2.0");
        assert!(explicit, "an explicit --base is flagged explicit");

        // ...and that resolved base is stamped as `META_BASE` on the run's RunStarted, exactly
        // as `start_fresh`/`ensure_started_pinned` now stamp it at mint. A done run follows.
        let events = [
            Event::new(
                runscope::TYPE_RUN_STARTED,
                br#"{"run":"r1","criteria":[]}"#.to_vec(),
            )
            .with_meta(runscope::META_RUN_ID, "r1")
            .with_meta(runscope::META_BASE, &flag_base),
            Event::new(ledger::TYPE_UNIT_STARTED, br#"{"id":"u1"}"#.to_vec()),
            Event::new(
                ledger::TYPE_UNIT_INTEGRATED,
                br#"{"id":"u1","commit":"abc"}"#.to_vec(),
            ),
        ];

        // The persisted base is read straight back from the log - the single authority.
        assert_eq!(
            runscope::current_run_base(&events).as_deref(),
            Some("release/2.0")
        );

        // The status/dash read pattern (persisted base, else the env/default fallback) names
        // the flag base even though the surface's own argv has no `--base` and the env is
        // empty here - the parity the fix restores...
        let status_base =
            runscope::current_run_base(&events).unwrap_or_else(|| resolve_run_base(None, None).0);
        assert_eq!(status_base, "release/2.0");

        // ...whereas the OLD asymmetric re-resolution (the defect) named the DEFAULT, not the
        // run's base - documenting exactly the wrong PR command the persisted base eliminates.
        assert_eq!(resolve_run_base(None, None).0, DEFAULT_BASE_REF);
        assert_ne!(status_base, DEFAULT_BASE_REF);

        // Every surface renders through `release_ready`, so the PR command names the run's
        // actual base - not `main` - while the head (unaffected by base) is this run's
        // per-run-unique `pr/<run-short-id>` branch (no spec was seeded, so the head degrades
        // to the run-short-id alone).
        let text = release_ready_lines(&events, RUN_BRANCH, &status_base).join("\n");
        assert!(
            text.contains("gh pr create --base release/2.0 --head pr/r1"),
            "the PR command targets the run's persisted base: {text}"
        );

        // A run started BEFORE base persistence carries no `META_BASE`, so a surface falls back
        // to the live env/default resolution - the legacy behavior is preserved untouched.
        let legacy = [Event::new(
            runscope::TYPE_RUN_STARTED,
            br#"{"run":"r0","criteria":[]}"#.to_vec(),
        )
        .with_meta(runscope::META_RUN_ID, "r0")];
        assert_eq!(runscope::current_run_base(&legacy), None);
    }

    /// `cmd_stats` rejects any positional argument with a clear error (it takes none),
    /// mirroring the strict-arity errors the other CLI commands raise.
    #[test]
    fn cmd_stats_rejects_extra_arguments() {
        let err = cmd_stats(&["unexpected".to_string()]).expect_err("stats takes no arguments");
        assert!(
            err.to_string().contains("stats: expected no arguments"),
            "the error must explain stats takes no arguments; got: {err}"
        );
    }

    /// On an absent `events.db` (a project that has never run) `cmd_stats` must print
    /// the clear "no runs yet" message and succeed, NOT create the db or panic. Run in
    /// a temp dir so the real project's `.rigger/` is untouched.
    #[test]
    // Mutates the process-global CWD (`set_current_dir` below). Shares the `cwd` serial
    // key with `shipped_workflows_carry_a_non_zero_spawn_budget` (which reads relative
    // paths) so the two never run concurrently: the restore guard prevents LEAKING a
    // changed CWD past this test, but only mutual exclusion prevents the other test from
    // OBSERVING the changed CWD mid-window.
    #[serial_test::serial(cwd)]
    fn cmd_stats_on_a_never_run_project_says_no_runs_and_creates_no_db() {
        let dir = tempfile::tempdir().unwrap();
        let prev = std::env::current_dir().unwrap();
        // current_dir is process-global; serialize against the other cwd-sensitive
        // path via a guard that always restores it even on a failed assertion.
        let _restore = CwdGuard(prev);
        std::env::set_current_dir(dir.path()).unwrap();

        cmd_stats(&[]).expect("stats on a never-run project must succeed");

        // The absent-db guard must run BEFORE Store::open, so no events.db is created.
        assert!(
            !dir.path().join(RIGGER_DIR).join("events.db").exists(),
            "stats on a never-run project must not create events.db"
        );
    }
}
