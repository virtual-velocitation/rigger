use super::*;

/// Env override (milliseconds) for the self-reap watcher's poll interval (spec 39, criterion 3),
/// and its production default. The crate's own integration test sets the env small so the
/// detached dash's self-reap is observable within the test, without changing the shipped cadence.
const DASH_REAP_POLL_ENV: &str = "RIGGER_DASH_REAP_POLL_MS";
const DASH_REAP_POLL_DEFAULT_MS: u64 = 5_000;

/// Env override (seconds) for the self-reap watcher's IDLE WINDOW (spec 50, criterion 5): a
/// registered instance whose heartbeat is older than this counts as no longer live. Absent, the
/// window defaults to the registry's own idle bound ([`rigger::registry::DEFAULT_IDLE_MS`]) so the
/// reader and the reaper share one staleness authority; the crate's own integration test sets it
/// small so the singleton's self-reap is observable within the test rather than on the shipped
/// multi-minute cadence.
const DASH_REAP_STALE_ENV: &str = "RIGGER_DASH_REAP_STALE_SECS";

pub(crate) fn cmd_dash(args: &[String]) -> Res {
    // `--export <path>` and/or `--port <n>`; loopback only (no host flag by design).
    // `--reap-on-idle` makes this dash SELF-REAP when the run it serves goes idle/complete
    // (spec 39, criterion 3) - passed only by the DETACHED step-path spawn, never the
    // guard-bound `rigger run` / `run_workflow` dash (which keeps its `ReapedChild`).
    let mut export: Option<String> = None;
    let mut port: u16 = dash::DEFAULT_PORT;
    let mut reap_on_idle = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--export" => {
                i += 1;
                export = Some(
                    args.get(i)
                        .cloned()
                        .ok_or("dash: --export expects a path")?,
                );
            }
            "--port" => {
                i += 1;
                port = args
                    .get(i)
                    .and_then(|p| p.parse().ok())
                    .ok_or("dash: --port expects a port number (1-65535)")?;
            }
            "--reap-on-idle" => reap_on_idle = true,
            other => return Err(format!("dash: unknown argument {other:?}").into()),
        }
        i += 1;
    }

    let events_db = db_path("events.db");
    let graph_db = db_path("graph.db");
    let progress_db = db_path("progress.db");
    let identity = project_identity();
    // The dashboard only reads the graph: one that owes its rebuild is served as it stands, and
    // the dash says so once at start (spec 101).
    if let Some(note) = graph_rebuild_owed_note(&graph_db, &identity) {
        eprintln!("{note}");
    }
    // The scratch root whose markers rigger stats to present each agent's liveness age (spec
    // 14). Resolved once; a repo-less invocation with no `RIGGER_TMPDIR`/configured `workdir`
    // either leaves it empty and the view omits ages (see the resolution below for why an
    // explicit override still resolves without a repo).
    // The configured remediation bound (same config) sets the `#n/max` on a current-blocker
    // `reject-recurrence` line so the dashboard and `rigger status` agree. Read via the SAME
    // shared, validate-independent `scratch_defaults` resolver `cmd_status`/`watch_poll`/
    // `reclaim_spawn_scratch`/`cmd_scratch`/`cmd_replay` all use (spec 83 criterion 2, round 3)
    // - never `config::load`, which additionally requires a fully loadable `.rigger/agents/`
    // fleet AND a passing `Config::validate` just to learn these two fields; `arch-u83c3-dash-
    // scratch-defaults-not-migrated` found `cmd_dash` was left on the old pattern, silently
    // losing a configured `defaults.workdir`/`defaults.max_retries` (and every agent's
    // liveness age on the dashboard with it) whenever `Config::validate` failed for an
    // unrelated reason. Anchored at the process's raw cwd (`RIGGER_DIR` alone, exactly what
    // `config_store::load(".")` resolved before), matching this function's own pre-existing
    // `git_repo()`-based `scratch_root` resolution below and its documented repo-less degrade
    // (`cmd_dash`, unlike the courier commands, is deliberately never routed through
    // `require_store_dir`'s owning-root walk) - only the VALIDATE-INDEPENDENCE axis changes
    // here, not the anchor.
    let (workdir, max_retries) = scratch_defaults(&StoreLocation {
        dir: PathBuf::from(RIGGER_DIR),
    });
    let scratch_root = {
        let repo = git_repo();
        // An empty `repo` alone must NOT force an empty scratch root: `RIGGER_TMPDIR` (or a
        // configured `workdir`) is an EXPLICIT override that `worktree::scratch_root_path`
        // already answers correctly with no git repo at all (its own first match arm checks the
        // override before ever consulting `repo`) - a `rigger dash` launched from a directory
        // with no git repository above it is not exotic: a whole-tree copy that excludes `.git`
        // (this project's own `cargo mutants --in-diff` scratch-tree build, spec 78's
        // mutation-efficacy step) produces exactly that cwd, and silently dropping an explicit
        // `RIGGER_TMPDIR` there blinds the self-reap watcher (spec 62, criterion 5) to this
        // project's own agent-liveness marker. Only when NEITHER signal is present (no repo, no
        // env override, no configured `workdir`) does this stay empty, preserving the original
        // repo-less degrade below (no scratch to probe, no directory fabricated from a bare cwd).
        let has_explicit_root = std::env::var("RIGGER_TMPDIR").is_ok_and(|v| !v.trim().is_empty())
            || !workdir.trim().is_empty();
        if repo.is_empty() && !has_explicit_root {
            String::new()
        } else {
            // The dash only READS this root (marker probes); it never places work under it,
            // so it resolves without creating - creation is the step's, and creation is
            // where the cache home's orphan-root reclaim runs.
            rigger::worktree::scratch_root_path_from_env(&repo, &workdir)
        }
    };
    // A clone taken BEFORE the `provider` closure below moves the original: the self-reap
    // watcher (spec 62, criterion 5) needs this project's own scratch root to check its agent
    // liveness markers directly, but `provider` is a `move` closure that would otherwise
    // consume `scratch_root` entirely, leaving nothing for the watcher spawned much later in
    // this function. Cheap regardless of `reap_on_idle` (an empty string when repo-less).
    let reap_scratch_root = scratch_root.clone();
    // The FALLBACK release-target base the ready-to-release handoff (spec 38, criterion 3) names
    // on the dash. `build_state` reads the base PERSISTED on this run's RunStarted from the
    // events it projects each request, so the live dash names the base the run actually anchored
    // on even though it inherits only the environment (never the run's `--base` flag). This
    // env/default resolution is used ONLY when the run predates base persistence (or ran with no
    // repo) and carries no persisted base, keeping the dash and `rigger status` on one handoff.
    let (release_base, _) = resolve_run_base(None, std::env::var("RIGGER_BASE").ok().as_deref());

    // The machine-global instance registry the singleton's self-reap watcher polls (spec 50,
    // criterion 5) - resolved ONLY when this is the detached, self-reaping singleton
    // (`--reap-on-idle`) and the environment has a state home (a homeless environment holds no
    // registry, so the watcher never starts and the dash simply serves). `None` for the guard-bound
    // `rigger run` / `run_workflow` dash, which never starts a watcher (it keeps its `ReapedChild`).
    // Retargets spec 39's per-run liveness snapshot at the singleton: the reap trigger is now the
    // registry (every instance on the machine), not this one project's run, so no per-RUN inputs
    // are captured here. Spec 62 criterion 5 adds a second, per-PROJECT (never per-run) signal
    // alongside it: `reap_scratch_root` above, so the watcher can also see THIS project's own
    // agent-liveness markers directly - and, as of round 2, every OTHER registered project's own
    // too, derived from the registry it already polls - see `watch_and_self_reap_on_idle`.
    let reap_registry_dir = reap_on_idle.then(rigger::registry::default_dir).flatten();

    // The SEPARATE, lazy graph provider for `/api/graph` (spec 45, criteria 1+2). It opens the
    // projection and reads the graph ONLY when a graph request arrives - never on the 1.5s state
    // poll - so a whole-graph read never rides the poll. It reads the WHOLE projection DIRECTLY
    // (`dash_read_whole_graph`), NOT the run-seeded `subgraph(graph_seeds(events), 2)` the polled
    // provider uses, so the overview and the seeded neighborhood reach any node the projection
    // holds - fixing the never-built-repo dead-end where an empty run-seed set collapsed the graph
    // to `Graph::default()`. Read-only and per-request-open like the polled provider: the dash
    // still starts before the store exists, and an absent/empty graph degrades to an empty result,
    // never an error. Its own clones of the db path + identity, captured before the polled
    // `provider` below moves the originals.
    // The machine-global instance registry (spec 50), for the landing list AND the ATTACH
    // resolver (criterion 3). `None` in a homeless environment: the landing is then empty and no
    // `?instance=` selector can resolve, but the dash still serves its own local project.
    let registry_dir = rigger::registry::default_dir();

    let graph_provider = {
        let graph_db = graph_db.clone();
        let identity = identity.clone();
        let registry_dir = registry_dir.clone();
        // The selected instance (spec 50, criterion 3) chooses WHICH store's whole graph is read:
        // the dash's own project by default, an attached instance's LOCAL graph projection when
        // one is selected, or an empty graph for a since-gone selector - never an error.
        move |instance: Option<&str>| -> contextgraph::Graph {
            match dash_resolve_attach(instance, registry_dir.as_deref()) {
                DashAttach::Local => dash_read_whole_graph(&graph_db, &identity),
                DashAttach::Instance(inst) => dash_attach_graph(&inst),
                DashAttach::Empty => contextgraph::Graph::default(),
            }
        }
    };

    // The DIRECTED-CALL provider for `/api/graph?view=calls` (spec 52, criterion 4): the SAME lazy
    // direct-projection provider the whole-graph views use, but running the store-side directed
    // traversal `Projection::calls` (the execution path / call sites of a seed) instead of reading
    // the whole graph. Opened only on a call request, never on the state poll; per-request-open and
    // best-effort like `graph_provider`, so an absent / empty graph or a seed with no calls degrades
    // to an empty `CallGraph`, never an error. Chooses WHICH store to walk from the same spec-50
    // attach selector.
    let calls_provider = {
        let graph_db = graph_db.clone();
        let identity = identity.clone();
        let registry_dir = registry_dir.clone();
        move |instance: Option<&str>,
              seed: &[String],
              direction: contextgraph::Direction,
              depth: i64,
              tier_floor: &str|
              -> contextgraph::CallGraph {
            match dash_resolve_attach(instance, registry_dir.as_deref()) {
                DashAttach::Local => {
                    dash_read_calls(&graph_db, &identity, seed, direction, depth, tier_floor)
                }
                DashAttach::Instance(inst) => {
                    dash_attach_calls(&inst, seed, direction, depth, tier_floor)
                }
                DashAttach::Empty => contextgraph::CallGraph::default(),
            }
        }
    };

    // Fresh projection inputs on every request. Reading (not holding an open handle) is
    // what lets the dash start before the store exists and pick the run up once it does. The
    // selected instance (spec 50, criterion 3) chooses WHICH store is opened per request: the
    // dash's own project by default, an attached instance's stores when one is selected, or an
    // empty state for a since-gone selector (an empty store renders empty, never an error).
    let provider = {
        let registry_dir = registry_dir.clone();
        move |instance: Option<&str>| -> Result<dash::DashInputs, String> {
            match dash_resolve_attach(instance, registry_dir.as_deref()) {
                DashAttach::Local => {
                    let (events, run_id) =
                        dash_read_run(&events_db, &identity).map_err(|e| e.to_string())?;
                    let graph = dash_read_graph(&graph_db, &identity, &events);
                    let progress = dash_read_progress(&progress_db, &identity, &run_id);
                    let liveness = dash_read_liveness(&events, &scratch_root, &run_id);
                    Ok((events, graph, progress, liveness))
                }
                DashAttach::Instance(inst) => Ok(dash_attach_inputs(&inst)),
                DashAttach::Empty => Ok((
                    Vec::new(),
                    contextgraph::Graph::default(),
                    Vec::new(),
                    std::collections::HashMap::new(),
                )),
            }
        }
    };

    // The LANDING provider (spec 50, criterion 3): the machine-global registry projected into the
    // credential-free instance list, freshly read (and stale-FILTERED, never pruned) on each
    // `/api/instances` poll. `read_live_no_prune`, not `read_live`: an open dash tab polls this on
    // its own schedule, unsynchronized with the self-reap watcher's tick, so a delete here could
    // win a race against the watcher's own `read_all` and permanently erase a foreign project's
    // only route into its `known_roots` set (spec 62 criterion 5 round 4 -
    // `adv-u62c5r4-known-roots-prune-race-with-instances-provider`; see `read_live_no_prune`'s doc).
    let instances_provider = {
        let registry_dir = registry_dir.clone();
        move || -> Vec<dash::InstanceView> {
            match registry_dir.as_deref() {
                Some(dir) => {
                    let now = rigger::registry::now_ms();
                    let live = rigger::registry::read_live_no_prune(
                        dir,
                        now,
                        rigger::registry::DEFAULT_IDLE_MS,
                    );
                    dash::instance_views(&live, now)
                }
                None => Vec::new(),
            }
        }
    };

    match export {
        Some(path) => {
            // An exported snapshot is always of the dash's own local project (no attach selector).
            let (events, graph, progress, liveness) =
                provider(None).map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
            let html = dash::render_export(
                &events,
                &graph,
                &progress,
                &liveness,
                max_retries,
                RUN_BRANCH,
                &release_base,
            )?;
            std::fs::write(&path, html)?;
            println!("wrote dash snapshot to {path}");
            Ok(())
        }
        None => {
            // Fixed address + singleton (spec 50, criterion 1): bind the RESOLVED address
            // DIRECTLY - no free-port search. If a rigger dash is already serving that address,
            // report it and exit 0 (never a second dash, never a drifted port); a non-dash
            // holder is a genuine conflict the bind error surfaces (resolve it with `--port`).
            let addr = SocketAddr::from(([127, 0, 0, 1], port));
            match dash::bind_singleton(addr) {
                Ok(dash::SingletonBind::AlreadyServing(existing)) => {
                    // The singleton is the point: a second invocation reports the ONE known
                    // address and exits cleanly instead of starting a rival dash.
                    println!("rigger dash: already serving on http://{existing}/");
                    Ok(())
                }
                // A held port is explained, not silent (spec 62, criterion 3 - HELD-PORT
                // DIAGNOSIS): `bind_singleton` only ever returns `Err` for a genuine, non-dash
                // conflict (a real rigger dash already on this address resolves to
                // `AlreadyServing` above instead), so an `AddrInUse` here always means SOME
                // other process holds the port - name it, and its process state when the
                // `/proc` surface can discover it, instead of the bare `io::Error` a plain `?`
                // used to propagate. Any OTHER bind error (e.g. a privileged port with no
                // permission) is unrelated to a held port and passes through unchanged.
                Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                    Err(dash::describe_held_port(addr).into())
                }
                Err(e) => Err(e.into()),
                Ok(dash::SingletonBind::Bound(listener)) => {
                    // Self-reap on machine idle (spec 50, criterion 5; spec 62, criterion 5): the
                    // detached step-path SINGLETON is started with `--reap-on-idle`, so a
                    // background thread polls the machine-global instance registry AND the
                    // agent-liveness markers of THIS project plus EVERY OTHER registered project
                    // (round 2's cross-project widening - `watch_and_self_reap_on_idle`'s own doc
                    // comment), exiting this process only once NONE of that shows anything live -
                    // leaving no orphaned dash on a quiet machine, while surviving one project's
                    // run ending as long as another's is still live, AND surviving a live agent
                    // on ANY registered project whose courier cadence has lapsed even as THAT
                    // project's own registry entry ages out. This retargets spec 39's per-run
                    // liveness trigger at the machine-level singleton; it is driven by the
                    // registry plus every known project's own agent-liveness check, NOT by any
                    // single `step` process exiting. Read-only: the watcher only reads the
                    // registry and stats marker files. The guard-bound `rigger run` /
                    // `run_workflow` dash omits the flag and keeps its `ReapedChild` reaping
                    // instead. Started ONLY on the branch that actually binds and serves - a
                    // short-circuited singleton invocation serves nothing and starts no watcher.
                    if let Some(registry_dir) = reap_registry_dir {
                        let poll = dash_reap_poll();
                        let idle_window = dash_reap_idle_window();
                        std::thread::spawn(move || {
                            watch_and_self_reap_on_idle(
                                registry_dir,
                                reap_scratch_root,
                                idle_window,
                                poll,
                            )
                        });
                    }
                    dash::serve_on(
                        listener,
                        provider,
                        graph_provider,
                        calls_provider,
                        instances_provider,
                        max_retries,
                        RUN_BRANCH,
                        &release_base,
                    )?;
                    Ok(())
                }
            }
        }
    }
}

/// The dash self-reap watcher's poll interval (spec 50, criterion 5): how often the detached
/// singleton dash re-checks the machine-global instance registry for a live instance. Env-tunable
/// via [`DASH_REAP_POLL_ENV`] (milliseconds) - the crate's own integration test sets it small so
/// the self-reap is observable quickly - defaulting to [`DASH_REAP_POLL_DEFAULT_MS`]. Clamped to at
/// least 1ms so a `0` override never spins the watcher into a busy loop.
fn dash_reap_poll() -> std::time::Duration {
    let ms = std::env::var(DASH_REAP_POLL_ENV)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(DASH_REAP_POLL_DEFAULT_MS)
        .max(1);
    std::time::Duration::from_millis(ms)
}

/// The dash self-reap watcher's IDLE WINDOW (spec 50, criterion 5): a registered instance whose
/// heartbeat is older than this counts as no longer live, so once EVERY registered instance has
/// aged past it (and none was refreshed within it) the singleton reaps. Defaults to the registry's
/// own idle window ([`rigger::registry::DEFAULT_IDLE_MS`]) so the reader and the reaper share ONE
/// staleness bound; env-tunable via [`DASH_REAP_STALE_ENV`] (seconds) so the crate's own
/// integration test can make the reap observable on a short window without the shipped multi-minute
/// cadence.
fn dash_reap_idle_window() -> std::time::Duration {
    match std::env::var(DASH_REAP_STALE_ENV)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
    {
        Some(secs) => std::time::Duration::from_secs(secs),
        None => std::time::Duration::from_millis(rigger::registry::DEFAULT_IDLE_MS),
    }
}

/// The scratch root a REGISTERED project at `root` would resolve for ITSELF (spec 62 criterion
/// 5 round 2: cross-project agent liveness) - that project's OWN `.rigger/workflow.yml`
/// configured workdir, read through the LIGHTWEIGHT [`config_store::read_scratch_workdir`] probe
/// (never the full [`config::load`], which would additionally require a loadable agent fleet
/// and a passing [`config::Config::validate`] just to learn one string field - a foreign
/// project this singleton never launched has no business failing this scan over its own
/// unrelated config shape), applied through the SAME read-only resolver every other
/// scratch-touching command in this binary shares ([`rigger::worktree::scratch_root_path`], the
/// authority `reset --build-cache` and `validate`'s residue scan both resolve through) -
/// deliberately WITHOUT this process's own `RIGGER_TMPDIR` override (`env_override: None`):
/// that variable states where THIS invocation places its OWN scratch and carries no meaning for
/// a DIFFERENT project's placement (which, if it set `RIGGER_TMPDIR` at all, did so under its
/// own separate invocation's environment - a value this long-lived singleton was never handed
/// and cannot reconstruct; honoring it here would also collapse EVERY registered project onto
/// the identical directory whenever the operator sets it, defeating the per-project distinction
/// this check exists to draw). Never creates a directory, so scanning a registered project's
/// markers never conjures a `.rigger/tmp` under a project that has none.
fn foreign_instance_scratch_root(root: &str) -> String {
    let rigger_dir = Path::new(root).join(RIGGER_DIR);
    let workdir = config_store::read_scratch_workdir(&rigger_dir).unwrap_or_default();
    rigger::worktree::scratch_root_path(root, &workdir, None)
}

/// The dash self-reap watcher loop (spec 50, criterion 5; spec 62, criterion 5), run on a
/// background thread inside the detached step-path SINGLETON dash. On each `poll` tick it reads
/// the machine-global instance registry read-only ([`rigger::registry::read_live`], which prunes
/// entries whose heartbeat has aged past `idle_window`) AND scans for a fresh in-flight AGENT
/// liveness marker ([`rigger::liveness::any_marker_fresh`], the SAME marker mechanism `rigger
/// status` reads, checked against the SAME `idle_window` bound) under `scratch_root` (this
/// LAUNCHING project's own) AND under every OTHER registered project's own derived scratch root
/// (spec 62 criterion 5 round 2 - [`foreign_instance_scratch_root`]); when
/// [`dash::should_reap_singleton`] says the machine is quiet - no registered instance is live,
/// at least one has been seen, and no agent liveness marker anywhere is fresh - it exits the
/// process, terminating the blocked [`dash::serve_on`] accept loop. This RETARGETS spec 39's
/// per-run liveness watch at the machine-level singleton: the dash serves every registered
/// instance and outlives any single run, so it SURVIVES one project's run ending while
/// another's is still live (that instance keeps `read_live` non-empty), SURVIVES a live agent on
/// ANY registered project whose courier cadence has lapsed even as THAT project's own registry
/// entry ages out (spec 62 criterion 5's headline, now closed for every registered project, not
/// only the launching one), and reaps only on a genuinely idle machine - driven by the registry
/// plus every known project's own agent-liveness check, NOT by any single `step` process
/// exiting. The `ever_seen_live` latch is the startup-race guard (a just-ensured singleton must
/// not reap before its ensuring run writes its entry); it stays scoped to the registry,
/// unchanged by the agent-liveness addition.
///
/// `known_roots` accumulates every OTHER project's root this watcher has EVER seen registered
/// ([`rigger::registry::read_all`], which - unlike `read_live` - applies no freshness filter and
/// prunes nothing), and is never forgotten: a project once seen keeps its own scratch root
/// checked for the rest of THIS singleton's lifetime, even past the poll where its registry
/// entry itself goes stale and `read_live` prunes it from disk. Without this durability, checking
/// only the CURRENT tick's registry snapshot would re-open almost the exact gap this criterion
/// closes for the launching project: the instant a project's own registry entry ages out and gets
/// pruned, a snapshot-only check would lose the only place it ever learned that project's root
/// from, and go blind to that project's still-fresh agent marker one poll later - mirroring the
/// exact registry-dependence gap criterion 5 round 2 exists to close for every OTHER project.
/// Bounded by how many distinct projects register on this one machine during the singleton's
/// lifetime - small and stable in practice, and each check is a cheap, mostly-empty directory
/// scan. Never returns (it either loops or exits the process).
fn watch_and_self_reap_on_idle(
    registry_dir: PathBuf,
    scratch_root: String,
    idle_window: std::time::Duration,
    poll: std::time::Duration,
) -> ! {
    let ttl_ms = u64::try_from(idle_window.as_millis()).unwrap_or(u64::MAX);
    let mut ever_seen_live = false;
    let mut known_roots: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    loop {
        std::thread::sleep(poll);
        // Grow the known-roots set from EVERY currently-registered instance BEFORE the
        // `read_live` call below can prune any of them - so a project is remembered the moment
        // it is first seen, even one whose registry entry goes stale and is pruned this very
        // tick. `read_all` applies no freshness filter and deletes nothing.
        for inst in rigger::registry::read_all(&registry_dir) {
            known_roots.insert(inst.root);
        }
        // Read-only scan of the machine-global registry: every instance whose heartbeat is fresher
        // than the idle window. `read_live` also prunes the entries that have aged out, so a dead
        // run's stale entry cannot keep the singleton alive.
        let live = rigger::registry::read_live(&registry_dir, rigger::registry::now_ms(), ttl_ms);
        if !live.is_empty() {
            // Latch: once any live instance has been seen, a later return to zero is genuine machine
            // idle (not the startup gap before the ensuring run's entry has landed).
            ever_seen_live = true;
        }
        // Read-only scan of this LOCAL project's own agent-liveness markers (spec 62, criterion
        // 5), OR any OTHER registered project's own (round 2): the same `idle_window` bound the
        // registry check above uses, so a fresh touch (spec 10's heartbeat) counts as live for
        // exactly as long as a fresh registry heartbeat would. An empty `scratch_root` (repo-less
        // launching project) degrades to no signal, matching every other reader.
        let now = std::time::SystemTime::now();
        let agent_live = rigger::liveness::any_marker_fresh(&scratch_root, now, idle_window)
            || known_roots.iter().any(|root| {
                rigger::liveness::any_marker_fresh(
                    &foreign_instance_scratch_root(root),
                    now,
                    idle_window,
                )
            });
        if dash::should_reap_singleton(live.len(), ever_seen_live, agent_live) {
            // Self-reap: exit the whole process so the detached singleton leaves no orphan on a
            // quiet machine. The stale `.rigger/dash.marker` this leaves behind is deliberately NOT
            // removed - a next run's first `step` already tolerates it (`dash_start_needed` probes
            // the recorded pid, sees it dead, and starts a fresh dash), and removing it here would
            // race a successor dash that may already have rewritten the marker with its own live pid.
            std::process::exit(0);
        }
    }
}

/// Read this project's CURRENT-run events and run id from `events_db` under `identity` through
/// the one-shot read of the run ([`runscope::read::read_current_run`], spec 101). An absent db is an empty run
/// and NO file is created (the guard precedes [`Store::open`], which would otherwise fabricate
/// one).
fn dash_read_run(
    events_db: &str,
    identity: &str,
) -> Result<(Vec<Event>, String), Box<dyn std::error::Error>> {
    // Resolve WHICH backend through the one authority, and SURFACE a genuine selection failure
    // (an unreadable `.rigger/store.conn`, an unreadable/malformed `workflow.yml`, an invalid
    // `store.backend`) with `?` - matching every other real-run-stream read (`canary_stats_lines`,
    // `read_model_drift`, `read_run_units`). Swallowing it into a silent local-sqlite default would
    // read the WRONG store on a server-pinned box whose secret file this user cannot read (the
    // different-user / permission edge §48 contemplates), so the dashboard read reports an empty run
    // against a live server (d-u2rr-observer-selection-loud, spec-19c loud-failure-surfacing).
    let sel = store_selection(None, None)?;
    Ok(with_project_store(events_db, identity, &sel, |store| {
        runscope::read::read_current_run(store, conductor::STREAM)
    })?
    .unwrap_or_default())
}

/// Build the context subgraph around the run's own units/decisions/findings from
/// `graph_db` (seeds via [`dash::graph_seeds`]). Best-effort: an absent graph (a grep-only
/// run never builds one) or any query error yields an empty graph, so the rest of the dash
/// still serves.
fn dash_read_graph(graph_db: &str, identity: &str, events: &[Event]) -> contextgraph::Graph {
    if !Path::new(graph_db).exists() {
        return contextgraph::Graph::default();
    }
    let seeds = dash::graph_seeds(events);
    if seeds.is_empty() {
        return contextgraph::Graph::default();
    }
    match Projector::open(graph_db, identity) {
        Ok(p) => p.subgraph(&seeds, 2).unwrap_or_default(),
        Err(_) => contextgraph::Graph::default(),
    }
}

/// The WHOLE projection for the `/api/graph` provider (spec 45, criterion 2): the direct-projection
/// read the dedicated, lazy graph provider consults on a graph request. Unlike [`dash_read_graph`]
/// it does NOT go through `graph_seeds` - it reads the entire live projection ([`Projector::whole`]),
/// so the seeded neighborhood and whole-graph overview reach ANY node the projection holds. That
/// fixes the never-built-repo dead-end: on a graph populated by code ingest with no run
/// decisions/findings, `graph_seeds` is empty and the run-seeded read collapses to
/// `Graph::default()`, whereas this read still returns the code nodes and their edges. Best-effort
/// like the run-seeded read: an absent graph (a grep-only run never builds one), an open error, or a
/// query error all degrade to an empty graph, never an error, so the rest of the dash still serves.
fn dash_read_whole_graph(graph_db: &str, identity: &str) -> contextgraph::Graph {
    if !Path::new(graph_db).exists() {
        return contextgraph::Graph::default();
    }
    match Projector::open(graph_db, identity) {
        Ok(p) => p.whole().unwrap_or_default(),
        Err(_) => contextgraph::Graph::default(),
    }
}

/// The DIRECTED-CALL walk for the `/api/graph?view=calls` provider (spec 52, criterion 4): the
/// store-side `Projection::calls` traversal (the seed's execution path or call sites) read through
/// the SAME lazy direct-projection open as [`dash_read_whole_graph`], never the polled read. Opens
/// the projection per request and runs the walk `direction`/`depth`/`tier_floor` select. Best-effort
/// exactly like the whole-graph read: an absent graph (a grep-only run never builds one), an open
/// error, or a walk error all degrade to an empty [`contextgraph::CallGraph`], never an error, so a
/// call request over a never-built or empty graph renders an empty view instead of failing.
fn dash_read_calls(
    graph_db: &str,
    identity: &str,
    seed: &[String],
    direction: contextgraph::Direction,
    depth: i64,
    tier_floor: &str,
) -> contextgraph::CallGraph {
    if !Path::new(graph_db).exists() {
        return contextgraph::CallGraph::default();
    }
    match Projector::open(graph_db, identity) {
        Ok(p) => p
            .calls(seed, direction, depth, tier_floor)
            .unwrap_or_default(),
        Err(_) => contextgraph::CallGraph::default(),
    }
}

/// The directed-call walk over an ATTACHED instance's graph store (spec 52 c4 + spec 50 c3): the
/// [`dash_read_calls`] analogue of [`dash_attach_graph`], opening the selected instance's `graph.db`
/// read-only. Best-effort - a since-gone or never-built instance graph degrades to an empty
/// [`contextgraph::CallGraph`], never an error.
fn dash_attach_calls(
    inst: &rigger::registry::Instance,
    seed: &[String],
    direction: contextgraph::Direction,
    depth: i64,
    tier_floor: &str,
) -> contextgraph::CallGraph {
    let graph_db = instance_rigger_dir(inst)
        .join("graph.db")
        .to_string_lossy()
        .into_owned();
    dash_read_calls(&graph_db, &inst.project, seed, direction, depth, tier_floor)
}

/// This run's progress from the SEPARATE progress store (spec 14), for the dash's live
/// per-agent view. Absent/empty is fine (the store is created lazily by the first
/// `rigger progress`), and only the current run's reports (its own stream) are read.
fn dash_read_progress(progress_db: &str, identity: &str, run_id: &str) -> Vec<Event> {
    if !Path::new(progress_db).exists() {
        return Vec::new();
    }
    let Ok(backend) = Store::open(progress_db) else {
        return Vec::new();
    };
    read_run_progress(&backend, identity, run_id)
}

/// The liveness-marker age (whole seconds since last touch) for each in-flight spawn in
/// `events` (the current run's slice), read HERE in Rust so the dash PRESENTS it (spec 14) -
/// the same stat the retired probe did, done by rigger rather than a spawned agent. Each marker
/// is read under the root its request recorded, `scratch_root` only for one that recorded none
/// (spec 101); a spawn found under neither has no age.
fn dash_read_liveness(
    events: &[Event],
    scratch_root: &str,
    run_id: &str,
) -> std::collections::HashMap<String, u64> {
    let Ok(step) = spawn::step_result(events) else {
        return std::collections::HashMap::new();
    };
    rigger::liveness::marker_ages(
        events,
        scratch_root,
        run_id,
        &step.wave,
        std::time::SystemTime::now(),
    )
    .into_iter()
    .collect()
}

/// Which registered instance a dash request ATTACHES to (spec 50, criterion 3), resolved from the
/// request's `?instance=<id>` selector against the machine-global registry.
enum DashAttach {
    /// No instance selected: serve the dash's OWN local project (backward compatible - today's
    /// single-project dash).
    Local,
    /// Serve this registered instance's stores, read-only.
    Instance(rigger::registry::Instance),
    /// An instance was requested but is unknown or has aged out of the registry: serve an EMPTY
    /// state - never the local default (a since-gone selection must not silently show the local
    /// run) and never an error (an empty store renders an empty state, spec 50).
    Empty,
}

/// Resolve which instance a dash request attaches to (spec 50, criterion 3). An absent/empty
/// selector keeps the dash on its own local project; a selector names a registry entry by its
/// stable id, resolved through a fresh [`rigger::registry::read_live_no_prune`] (which filters out
/// stale entries WITHOUT deleting them - see that function's doc for why an attach resolve must
/// never prune, spec 62 criterion 5 round 4) so a per-request open always lands on a
/// currently-live instance. An unresolvable selector (homeless environment, unknown id, or a
/// stale entry) degrades to [`DashAttach::Empty`].
fn dash_resolve_attach(instance: Option<&str>, dir: Option<&Path>) -> DashAttach {
    let Some(id) = instance.filter(|s| !s.is_empty()) else {
        return DashAttach::Local;
    };
    let Some(dir) = dir else {
        return DashAttach::Empty;
    };
    let live = rigger::registry::read_live_no_prune(
        dir,
        rigger::registry::now_ms(),
        rigger::registry::DEFAULT_IDLE_MS,
    );
    match live.into_iter().find(|i| i.id() == id) {
        Some(inst) => DashAttach::Instance(inst),
        None => DashAttach::Empty,
    }
}

/// A registered instance's `.rigger` directory, where its LOCAL knowledge-graph and progress
/// projections live regardless of whether its EVENT store is local sqlite or a shared server (the
/// KG is built locally per project - spec 50: "the knowledge-graph views open that instance's
/// local graph projection").
fn instance_rigger_dir(inst: &rigger::registry::Instance) -> PathBuf {
    Path::new(&inst.root).join(RIGGER_DIR)
}

/// Read a registered instance's current-run events, read-only (spec 50, criterion 3). A Local
/// instance is opened directly as sqlite at its registered log path; a Shared instance resolves
/// its connection through the store-resolution authority at the instance's OWN `.rigger` (the same
/// config that lets a worker report to that shared store lets the dash read it), read under the
/// instance's namespace identity. Best-effort: an absent, unreachable, or unreadable store degrades
/// to an empty run - "an empty store renders an empty state, never an error" - because the selected
/// instance is discovery metadata, not a source of truth.
///
/// Read an instance's `run` stream from an embedded sqlite event log at `path`, READ-ONLY: an
/// ABSENT file degrades to an empty run (`Ok(Vec::new())`) rather than opening it, because
/// [`open_sqlite_store`] -> [`Store::open`] CREATES the file AND its schema, and a dash attach is a
/// read-only projection that MUST NEVER write a store under a foreign project (spec 50, the
/// read-only global constraint). This is the ONE read-only sqlite attach reader: the Local arm and
/// the Shared arm's Sqlite-degrade BOTH route through it, so the existence guard lives in exactly
/// one place and no attach path can open-create a phantom `events.db`.
fn dash_read_sqlite_stream_readonly(
    path: &str,
    project: &str,
) -> Result<Vec<Event>, Box<dyn std::error::Error>> {
    Ok(
        with_project_store(path, project, &StoreSelection::Sqlite, |store| {
            Ok(runscope::read::read_current_run(store, conductor::STREAM)?.0)
        })?
        .unwrap_or_default(),
    )
}

fn dash_attach_run(inst: &rigger::registry::Instance) -> Vec<Event> {
    let read = || -> Result<Vec<Event>, Box<dyn std::error::Error>> {
        let all = match &inst.store {
            rigger::registry::StoreIdentity::Local { path } => {
                dash_read_sqlite_stream_readonly(path, &inst.project)?
            }
            rigger::registry::StoreIdentity::Shared { .. } => {
                let rigger_dir = instance_rigger_dir(inst);
                // Resolve through the ATTACHED instance's OWN `.rigger` with NO ambient environment
                // (`None`, never `env_conn()`): the dash process's own `KURRENTDB_CONN` addresses a
                // DIFFERENT project's store, so letting it win (§48 rung 2) would attach the wrong
                // store. The instance's own secret file / committed choice (rungs 3-4) is the
                // authority for reading THAT instance (adv-u50c3-uphold-sdet-env-precedence).
                let sel = store_selection_at(None, None, None, &rigger_dir)?;
                let events_db = rigger_dir.join("events.db");
                let events_db_path = events_db.to_string_lossy();
                match sel {
                    // The instance registered as Shared but its own config no longer resolves a
                    // server (its secret file / config is gone): a Sqlite DEGRADE. Guard existence
                    // EXACTLY like the Local arm - a read-only attach must NEVER open-create the
                    // store, so an absent `events.db` renders an empty run, never a phantom store
                    // file written under a foreign project (adv-u50c3-shared-attach-creates-phantom-store).
                    StoreSelection::Sqlite => {
                        dash_read_sqlite_stream_readonly(&events_db_path, &inst.project)?
                    }
                    StoreSelection::Server(_) => {
                        let backend = resolve_store(&sel, &events_db_path)?;
                        let store = Namespaced::new(backend.as_ref(), &inst.project);
                        runscope::read::read_current_run(&store, conductor::STREAM)?.0
                    }
                }
            }
        };
        Ok(all)
    };
    read().unwrap_or_default()
}

/// The cheap per-request inputs for an ATTACHED instance (spec 50, criterion 3): its run events,
/// the run-seeded context subgraph, and this-run progress - all from that instance's stores, read
/// read-only. The graph and progress are ALWAYS the instance's LOCAL projections under its
/// `.rigger`; only the event store may be remote. Liveness ages are the LOCAL run's scratch, which
/// a possibly-remote instance has none of reachable here, so its per-agent ages are simply absent.
fn dash_attach_inputs(inst: &rigger::registry::Instance) -> dash::DashInputs {
    let events = dash_attach_run(inst);
    let rigger_dir = instance_rigger_dir(inst);
    let graph_db = rigger_dir.join("graph.db").to_string_lossy().into_owned();
    let progress_db = rigger_dir
        .join("progress.db")
        .to_string_lossy()
        .into_owned();
    let graph = dash_read_graph(&graph_db, &inst.project, &events);
    let run_id = runscope::current_run_id(&events).unwrap_or_default();
    let progress = dash_read_progress(&progress_db, &inst.project, &run_id);
    (events, graph, progress, std::collections::HashMap::new())
}

/// The WHOLE knowledge-graph projection for an ATTACHED instance (spec 50, criterion 3), the
/// `/api/graph` view's lazy read: that instance's LOCAL `graph.db` under its `.rigger`, read
/// directly ([`dash_read_whole_graph`]) so it reaches any node the projection holds even when the
/// run seeded none. Best-effort/empty-degrade like the local read.
fn dash_attach_graph(inst: &rigger::registry::Instance) -> contextgraph::Graph {
    let graph_db = instance_rigger_dir(inst)
        .join("graph.db")
        .to_string_lossy()
        .into_owned();
    dash_read_whole_graph(&graph_db, &inst.project)
}

/// `rigger mcp` (spec 92, criterion 4: IN EVERY SESSION'S HAND): the operator's own
/// read-only MCP surface, the one `rigger setup` registers into `.mcp.json` for the
/// interactive Claude Code session. Unlike `rigger serve`/`cmd_serve` (the
/// workflow-driver bridge a loop run spawns, which anchors a run branch and creates unit
/// worktrees the moment it starts), this command has no run to drive and touches
/// NOTHING on disk beyond opening the existing event store, side-car, grounder, and
/// context graph read-only: it answers `rigger_peers`, `rigger_ground`, and
/// `rigger_graph` over stdio, the same three lookups a loop agent has (`rigger_peers`
/// through the shim's proxy to `rigger serve`; `ground`/`graph --show`/`graph --around`
/// as the CLI commands its persona names) - given to an interactive session as tools
/// instead of a shell.
///
/// Served through the SAME [`mcpserver::Server`] the workflow-driver bridge (`cmd_serve`
/// above) uses, wired with [`Server::with_grounder`](mcpserver::Server::with_grounder) and
/// [`Server::with_graph`](mcpserver::Server::with_graph) exactly like that call site already
/// wires `with_graph`/`with_progress` - ONE read loop, JSON-RPC dispatch, and ok/err envelope
/// answering both tool surfaces, rather than a second small stdio loop reaching for the
/// concrete grounder/`Projector` across the crate boundary. `rigger_next`/`rigger_result`
/// need a live `Driver`, but this surface never calls them (wiring a grounder is what marks a
/// `Server` as the lookup-only surface - see [`mcpserver::Server`]'s own doc comment) - a
/// freshly constructed, never-`spawn`ed one satisfies the constructor with no side effects.
///
/// Grounder resolution is NEVER propagated with `?`: a misconfigured/unavailable grounder
/// (no `defaults.grounder` pinned on a `--no-default-features` build with no `symbols`
/// feature, spec 57's own loud-refusal contract) must not take the WHOLE server down -
/// `rigger_peers`/`rigger_graph` have nothing to do with grounding and must keep answering.
/// A resolution failure is instead recorded via `with_grounder`'s `Err` arm, so only
/// `rigger_ground` itself reports it, lazily, exactly as the pre-fix operator surface did.
/// Parse `rigger mcp`'s one optional flag: `--spawn <id>` (spec 104, criterion 3) binds the
/// server to one spawn - see [`mcpserver::Server::with_spawn`]'s doc comment for what that
/// switches this instance to serve. No positional arguments; an unknown flag, a second
/// argument, or a valueless `--spawn` is a clear error rather than silently ignored (the
/// pre-spec-104 `cmd_mcp` accepted and discarded every argument).
fn parse_mcp_spawn_flag(args: &[String]) -> Result<Option<String>, Box<dyn std::error::Error>> {
    match args {
        [] => Ok(None),
        [flag, id] if flag == "--spawn" => {
            if id.is_empty() {
                return Err("mcp: --spawn expects a non-empty spawn id".into());
            }
            Ok(Some(id.clone()))
        }
        [flag] if flag == "--spawn" => {
            Err("mcp: --spawn expects a spawn id: rigger mcp --spawn <id>".into())
        }
        _ => Err(format!("mcp: unexpected arguments {args:?}: rigger mcp [--spawn <id>]").into()),
    }
}

/// `rigger mcp [--spawn <id>]`: serve MCP over stdio. With no `--spawn`, the operator's
/// read-only lookup surface (`rigger_peers`/`rigger_ground`/`rigger_graph`) `rigger setup`
/// registers into `.mcp.json`. With `--spawn <id>` (spec 104, THE SPAWN MCP SERVER,
/// criterion 3), the launched agent's own spawn-bound surface instead - every write it
/// serves (`rigger_emit`, `rigger_progress`) attributed to `id` BY CONSTRUCTION, over the
/// SAME progress-store + scratch-root resolution `cmd_progress`/`cmd_scratch` already use,
/// never a second parallel one.
pub(crate) fn cmd_mcp(args: &[String]) -> Res {
    let spawn = parse_mcp_spawn_flag(args)?;
    let (loc, selection) = require_store_dir()?;
    let backend = resolve_store(&selection, &loc.file("events.db"))?;
    let store = Namespaced::new(backend.as_ref(), &loc.identity());
    let grounder_name = config_store::load(".")
        .map(|cfg| cfg.workflow.defaults.grounder)
        .unwrap_or_default();
    let grounder = select_grounder(&grounder_name);
    // The resolved store's own graph, so a server started in a linked worktree serves and folds
    // into the owning repository's graph, never one opened in the worktree. Opened as it stands:
    // an emit this server serves appends whether or not the graph owes its rebuild (the fold
    // refuses while it does), and the graph and grounding tools, which depend on the fold,
    // refuse naming `rigger setup` until it is paid (spec 101).
    let graph = loc.graph()?;
    let driver = rigger::driver::workflow::Driver::new();

    // Only opened/resolved when `--spawn` is given - a plain `rigger mcp` (the operator's
    // lookup surface) must not conjure `.rigger/progress.db` or a scratch root it never uses.
    let prog_backend: Option<Store> = spawn
        .is_some()
        .then(|| Store::open(&loc.file("progress.db")))
        .transpose()?;
    let prog_store = prog_backend
        .as_ref()
        .map(|b| Namespaced::new(b, &loc.identity()));
    let scratch_root = if spawn.is_some() {
        let repo = loc
            .dir
            .parent()
            .and_then(|p| p.to_str())
            .ok_or("mcp: could not resolve the project root")?;
        let (workdir, _max_retries) = scratch_defaults(&loc);
        rigger::worktree::scratch_root_path_from_env(repo, &workdir)
    } else {
        String::new()
    };

    let mut server =
        mcpserver::Server::new(&driver, &store, conductor::STREAM).with_graph(graph.as_ref());
    server = server.with_grounder(match &grounder {
        Ok(g) => Ok(g.as_ref()),
        Err(e) => Err(e.to_string()),
    });
    if let Some(ps) = &prog_store {
        server = server.with_progress(ps, &scratch_root);
    }
    if let Some(id) = spawn {
        server = server.with_spawn(id);
    }
    server.run(std::io::stdin().lock(), std::io::stdout().lock())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Spec 45, criterion 2 (DIRECT-PROJECTION REACH): the `/api/graph` provider reads the WHOLE
    /// projection directly, not the run-seeded `subgraph(graph_seeds(events), 2)`. So on an
    /// indexed-but-never-built repo - a graph populated by code ingest with NO run
    /// decisions/findings, hence an EMPTY `graph_seeds` - a seed naming a real node still returns
    /// its neighborhood and the whole-graph overview still returns its clusters, instead of the
    /// `Graph::default()` dead-end the run-seeded read produced.
    #[test]
    fn dash_graph_provider_reaches_the_whole_projection_when_run_seeds_are_empty() {
        use std::collections::HashMap;

        let dir = tempfile::tempdir().unwrap();
        let graph_path = dir.path().join("graph.db");
        let graph_path = graph_path.to_str().unwrap();
        let identity = "reachtest";

        // A projection built from CODE INGEST alone (spec 29a: CodeEntityExtracted + EdgeInferred),
        // exactly what a cold-checkout `graph build` folds. No decision, no finding - so nothing a
        // run would seed a subgraph from.
        {
            let p = Projector::open(graph_path, identity).unwrap();
            let def = serde_json::json!({
                "file": "src/combat.rs", "name": "apply_damage",
                "kind": "function", "line": 7, "lang": "rust",
            });
            let mut e = Event::new(
                contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
                serde_json::to_vec(&def).unwrap(),
            );
            e.position = 1;
            crate::test_support::folds(&p, std::slice::from_ref(&e));
            let refr =
                serde_json::json!({ "file": "src/combat.rs", "name": "clamp", "lang": "rust" });
            let mut e2 = Event::new(
                contextgraph::TYPE_EDGE_INFERRED,
                serde_json::to_vec(&refr).unwrap(),
            );
            e2.position = 2;
            crate::test_support::folds(&p, std::slice::from_ref(&e2));
        }

        // Precondition = the never-built dead-end. The run log carries no content events, so
        // `graph_seeds` is EMPTY and the OLD run-seeded read collapses to `Graph::default()`.
        let no_run_events: Vec<Event> = Vec::new();
        assert!(
            dash::graph_seeds(&no_run_events).is_empty(),
            "the never-built repo has no run seeds"
        );
        let run_seeded = dash_read_graph(graph_path, identity, &no_run_events);
        assert!(
            run_seeded.nodes.is_empty(),
            "the run-seeded read is the empty dead-end this criterion removes, got {run_seeded:?}"
        );

        // The fix: the graph provider reads the whole projection directly, so a real code node is
        // reachable with no run seeds at all.
        let whole = dash_read_whole_graph(graph_path, identity);
        assert!(
            whole
                .nodes
                .iter()
                .any(|n| n.id == "src/combat.rs::apply_damage"),
            "the whole-projection read reaches a code node with no run seeds, got {whole:?}"
        );

        // Seeded-neighborhood reach: `/api/graph?seed=<real node>` over the whole graph returns the
        // node's neighborhood - not the empty default.
        let seed = "src/combat.rs::apply_damage";
        let resp = dash::route(
            "GET",
            &format!("/api/graph?seed={seed}"),
            &no_run_events,
            &whole,
            &[],
            &HashMap::new(),
            3,
            "rigger-run",
            "main",
            &[],
        );
        assert_eq!(resp.status, 200);
        let body = String::from_utf8(resp.body).unwrap();
        let nb: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(
            nb["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|n| n["id"] == seed),
            "the seeded neighborhood over the whole projection contains the seed node, got {body}"
        );

        // Whole-graph overview reach: `/api/graph` (no seed) returns clusters over the whole graph,
        // never an empty default.
        let resp2 = dash::route(
            "GET",
            "/api/graph",
            &no_run_events,
            &whole,
            &[],
            &HashMap::new(),
            3,
            "rigger-run",
            "main",
            &[],
        );
        assert_eq!(resp2.status, 200);
        let body2 = String::from_utf8(resp2.body).unwrap();
        let ov: serde_json::Value = serde_json::from_str(&body2).unwrap();
        assert!(
            !ov["clusters"].as_array().unwrap().is_empty() && ov["total"].as_u64().unwrap() > 0,
            "the whole-graph overview returns clusters, not an empty default, got {body2}"
        );
    }

    /// Spec 45 GLOBAL CONSTRAINT (read-only provider, L33-34 / L72-73): the direct-projection
    /// provider is READ-ONLY, and "the dash still starts before the store exists; an absent graph
    /// degrades to an empty result, never an error". `dash_read_whole_graph` over an ABSENT graph db
    /// (the grep-only / never-built repo that has no `.rigger/graph.db`) must return an EMPTY graph
    /// and MUST NOT materialize the db.
    ///
    /// This pins the load-bearing `if !Path::new(graph_db).exists()` guard: `Projector::open` opens
    /// with the default `OPEN_READWRITE | OPEN_CREATE` and runs `execute_batch(SCHEMA)`, so WITHOUT
    /// the guard a read would spuriously CREATE the file+schema on a repo that never built one - a
    /// real, user-facing WRITE that breaks the read-only contract. The file-not-created assertion is
    /// the guard's teeth: delete the guard and this test reddens.
    #[test]
    fn dash_read_whole_graph_on_an_absent_db_is_empty_and_creates_nothing() {
        let dir = tempfile::tempdir().unwrap();
        // A path that does NOT exist: the never-built repo has no graph projection at all.
        let graph_path = dir.path().join("graph.db");
        let graph_db = graph_path.to_str().unwrap();
        let identity = "reachtest";
        assert!(
            !Path::new(graph_db).exists(),
            "precondition: the never-built repo has no graph db yet"
        );

        // (a) The read degrades to an EMPTY graph, never an error.
        let whole = dash_read_whole_graph(graph_db, identity);
        assert!(
            whole.nodes.is_empty() && whole.edges.is_empty(),
            "an absent projection reads as an empty graph, got {whole:?}"
        );

        // (b) The read is READ-ONLY: it must NOT have materialized the db. Removing the
        // `if !Path::new(graph_db).exists()` guard makes `Projector::open` CREATE + SCHEMA-write the
        // file here, reddening this assertion - these are the guard's teeth.
        assert!(
            !Path::new(graph_db).exists(),
            "a read over an absent projection must NOT create {graph_db} (read-only provider)"
        );

        // The composed provider closure the dash consults on /api/graph (main.rs, the
        // `move || dash_read_whole_graph(..)` provider) honors the same read-only contract over the
        // absent path.
        let provider = || -> contextgraph::Graph { dash_read_whole_graph(graph_db, identity) };
        let via_provider = provider();
        assert!(
            via_provider.nodes.is_empty() && via_provider.edges.is_empty(),
            "the composed provider over an absent projection is empty, got {via_provider:?}"
        );
        assert!(
            !Path::new(graph_db).exists(),
            "the composed provider must NOT create the db either (read-only provider)"
        );
    }
}
