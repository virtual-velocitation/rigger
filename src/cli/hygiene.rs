use super::*;
use runscope::{superseded_edge_boundary, superseded_graph_nodes};

/// `rigger reset` - the supported prunes, one flag per accumulation.
///
/// Each mode sheds a DIFFERENT pile, so they name themselves explicitly and compose in one
/// invocation.
///
///   - `--runs` (spec 21, unit 2) prunes the CONTEXT GRAPH: see [`reset_runs`].
///   - `--derived` (spec 107, criterion 16) migrates the EVENT LOG: see [`reset_derived`].
///
/// A BARE `rigger reset` (no flags at all) is a MENU, not an error (spec 68, "the reset
/// surface"): see [`reset_menu`]. Only a TRULY empty `args` takes that path - any non-empty
/// args that select no mode (an unknown flag, or `--force-live` alone) fall through to
/// [`reset_modes`]'s existing refusal exactly as before this menu existed, so that refusal and
/// its tests are untouched.
///
/// PRECHECKS FIRST, and exactly what they promise. The flags are parsed, and the scratch root,
/// registry and instant the modes read are resolved once ([`ResetEnv`], fail-closed) BEFORE the
/// first prune runs when a selected mode reads them ([`ResetModes::reads_env`]), so a composed
/// invocation never starts work an environment it cannot resolve would stop. A mode that reads none
/// of that environment (`--scratch-orphans`, and `--derived --force-live`, whose override skips the
/// live-writer guard entirely) never resolves it, so selected without a mode that reads it, it is
/// never failed by what it does not read. The `--derived` backend refusal is the one precheck that
/// follows its sibling modes, by design: it is settled inside `--derived`'s own block, after
/// `--runs`, `--build-cache` and `--scratch-orphans` have run, because a sibling mode with no
/// backend dependency of its own is never dropped by it (spec 77, criterion 4) - so `reset --runs
/// --derived` on a server-backed project prunes the graph, reports it, and then refuses the
/// compaction. Each mode's own mutation is atomic (each is one transaction over one file), and the
/// modes run in order: if a prune fails on a genuine IO or lock fault after an earlier one
/// committed, the earlier prune HAS happened and is reported on stdout above the error. That is the
/// honest statement of the composition, and it is deliberately not called all-or-nothing: two files
/// cannot be committed together, and claiming otherwise would tell an operator not to look.
pub(crate) fn cmd_reset(args: &[String]) -> Res {
    if args.is_empty() {
        let (loc, selection) = require_store_dir()?;
        if selection.is_sqlite() {
            migrate_identity_at(&loc)?;
        }
        return reset_menu(&loc, &selection);
    }

    let modes = reset_modes(args)?;

    let (loc, selection) = require_store_dir()?;
    // Resolved here, before the identity migration and the first prune, when a selected mode
    // reads it; every reading mode below then takes this one resolution.
    let mut env = None;
    if modes.reads_env() {
        ResetEnv::resolved(&mut env, &loc)?;
    }
    // Before ANY prune reads a stream name: run the one-time spec-09 identity migration, exactly
    // as `run` / `step` / `workflow` / `playbooks` do before they open their store. Both prunes
    // address this project's history BY ITS CURRENT IDENTITY, and a store bloated enough to need
    // compacting is by construction an OLD store whose history was written under the pre-identity
    // basename namespace. Without this, `reset` would match no stream at all on exactly the log it
    // exists for and report a perfectly successful prune of zero rows - the silent no-op this
    // command's whole design refuses. Anchored at the RESOLVED store root, not the process cwd, so
    // a reset run from a nested worktree migrates the store it is about to prune.
    if selection.is_sqlite() {
        migrate_identity_at(&loc)?;
    }
    if modes.runs {
        reset_runs(&loc, &selection, ResetEnv::resolved(&mut env, &loc)?)?;
    }
    if modes.build_cache {
        // A pure filesystem reclaim over the scratch root, orthogonal to the event log and
        // graph `--runs`/`--derived` prune, and carrying NO backend requirement at all
        // (spec 77 Design) - the one config value it needs (`defaults.workdir`) comes from
        // [`ResetEnv`]'s lightweight `config_store::read_scratch_workdir` probe, never the full
        // `config::load` (which would additionally require a loadable agent fleet just to
        // reclaim disk space). Dispatched BEFORE `--derived` below (not after, as its
        // Design-bullet order might suggest) so a composed `--build-cache --derived` on a
        // server-backed project still reclaims the cache and reports it - `--derived`'s own
        // backend refusal below must never silently drop a sibling mode with no backend
        // dependency of its own (spec 77 c4 review history: the identical composition defect an
        // earlier attempt at this feature was caught for, reproduced independently before this
        // fix landed).
        reset_build_cache(ResetEnv::resolved(&mut env, &loc)?)?;
    }
    if modes.scratch_orphans {
        reset_scratch_orphans()?;
    }
    if modes.derived {
        // Decided up front, before migrating: rewriting and deleting rows and reclaiming the
        // file are mechanics of the embedded log, not port operations, so `--derived` names the
        // backend it needs rather than quietly doing nothing on one the migration does not run on.
        // Checked HERE (inside this mode's own block), not as an early top-level return
        // before `--runs`/`--build-cache` even run - both those modes complete regardless
        // of what `--derived` decides, matching the SAME "each mode sheds only its own
        // accumulation, and an earlier prune's completion is reported before a later
        // refusal" honesty the live-writer guard just below already commits to.
        if !selection.is_sqlite() {
            return Err(format!(
                "reset --derived: the migration rewrites and deletes rows of the event log and \
                 vacuums the file, which is a mechanic of the embedded {RIGGER_DIR}/events.db \
                 store; this project is configured for the server-backed store, where the \
                 migration does not run. Re-run it against a project on the sqlite backend. \
                 Refusing rather than reporting a migration that did not happen."
            )
            .into());
        }
        // THE MIGRATION REFUSES LIVE WRITERS (spec 71, criterion 2): `--derived` leaves revision
        // gaps by design, and a writer built before this migration ran can reissue one of those
        // gaps and reorder the log (the incident spec 71 records) if the log changes under it.
        // `--force-live` is the explicit, named escape hatch that skips this check entirely (it
        // verifies nothing - the operator owns that risk once they pass it).
        //
        // The step lock the guard took is held until the migration returns, so its verdict that
        // no `rigger step` is running stays true while the log is rewritten: a step started
        // meanwhile refuses on the held lock, and its courier retries once the reset is done.
        let _step_lock = if modes.force_live {
            None
        } else {
            refuse_derived_reset_if_live(&loc, &selection, ResetEnv::resolved(&mut env, &loc)?)?
        };
        reset_derived(&loc, &|root, bytes| {
            Ok(rigger::worktree::hash_blob(root, bytes)?)
        })?;
    }
    Ok(())
}

/// What `rigger reset`'s modes read from outside the store: resolved ONCE by [`cmd_reset`], the
/// composition root, when a selected mode reads it ([`ResetModes::reads_env`]), and handed to the
/// live-writer probe (`--derived` without `--force-live`, and `--runs`) and to `--build-cache`'s
/// shared-cache reclaim, so the probe reads no configuration, environment or clock of its own and
/// judges every signal at one instant under one scratch root.
struct ResetEnv {
    /// The store's configured `defaults.workdir` (empty when unset).
    workdir: String,
    /// The scratch root this store's runs write under ([`marker_root`]), resolved from this
    /// process's environment: the build caches `--build-cache` reclaims, and the root the guard
    /// reads a spawn's liveness marker under only when the spawn's request recorded none of its
    /// own ([`rigger::liveness::MarkerRoots`] - where a marker lives is log-carried, spec 101).
    scratch_root: String,
    /// The machine-global instance registry (spec 50); `None` in a homeless environment.
    registry_dir: Option<PathBuf>,
    /// The one instant, in Unix-epoch milliseconds, every liveness judgment of this command is
    /// made at: registry heartbeats and spawn markers alike.
    now_ms: u64,
}

impl ResetEnv {
    /// Resolves the scratch root FAIL-CLOSED (ruling adj-u101gl-marker-root-fail-closed): a
    /// `defaults` block [`config_store::read_scratch_workdir`] cannot parse fails the command with
    /// the config's own error, and a store whose owning root has no UTF-8 path - which resolves
    /// no scratch root at all - is refused, because the live-writer guard may only refuse: a root
    /// it cannot resolve must never read as "no spawn marker, so nothing is live".
    fn resolve(loc: &StoreLocation) -> Result<Self, Box<dyn std::error::Error>> {
        let workdir = config_store::read_scratch_workdir(&loc.dir)?;
        let scratch_root = marker_root(&loc.repo_root(), &workdir);
        if scratch_root.is_empty() {
            return Err(format!(
                "reset: the store at {} has no UTF-8 owning root, so the scratch root its runs \
                 write their liveness markers and build caches under cannot be resolved; \
                 refusing rather than reading that as no live spawn",
                loc.dir.display()
            )
            .into());
        }
        Ok(Self {
            workdir,
            scratch_root,
            registry_dir: rigger::registry::default_dir(),
            now_ms: rigger::registry::now_ms(),
        })
    }

    /// The env in `slot`, [`Self::resolve`]d into it by the first read and returned as stored by
    /// every later one, so one command resolves it at most once and only once a mode reads it.
    fn resolved<'s>(
        slot: &'s mut Option<Self>,
        loc: &StoreLocation,
    ) -> Result<&'s Self, Box<dyn std::error::Error>> {
        let env = match slot.take() {
            Some(env) => env,
            None => Self::resolve(loc)?,
        };
        Ok(slot.insert(env))
    }
}

/// Bare `rigger reset` (spec 68, "the reset surface"): a MENU, not an error. Prints one line per
/// prunable accumulation `--runs` / `--derived` would act on, each with a MEASURED count and the
/// flag that acts on it, then exits 0. Read-only by construction - every number here comes from a
/// `SELECT`, never from running a prune, so invoking the bare command is always safe to do "just
/// to look". The `--derived` line reads [`Store::count_derived`] over the project's run stream, the
/// read the migration itself acts on, so what it names never drifts from what `--derived` sheds.
///
/// WHY A COUNT, NOT A DISK-BYTE FORECAST. The flagged reports name bytes RECLAIMED
/// ([`reclamation_lines`], `reset_runs`'s own line) because they measure a real before/after
/// across the mutation that just ran - `Reclamation::reclaimed_bytes`'s own docs are explicit
/// that this is "MEASURED, NOT DERIVED" over the actual rewrite, and `Projector::compact`'s docs
/// say the same of `VACUUM`: a page-count delta is only meaningful once the rewrite has happened.
/// There is no honest byte figure to preview BEFORE that rewrite runs - printing one here would
/// be exactly the fabricated number this whole command's design otherwise refuses to print. A
/// COUNT of what would be removed is the real, read-only measurement the preview CAN make
/// ([`contextgraph::sqlite::Projector::count_prunable`] / [`Store::count_derived`], each the
/// read-only twin of what its flag removes), so that is what this menu reports.
fn reset_menu(loc: &StoreLocation, selection: &StoreSelection) -> Res {
    // --runs: works over ANY backend, exactly like a real `--runs` does (the context graph is
    // always a local file; only the EVENT log may be server-backed) - so this reads the whole run
    // stream through the resolved backend precisely as `reset_runs` does.
    let backend = resolve_store(selection, &loc.file("events.db"))?;
    let store = Namespaced::new(backend.as_ref(), &loc.identity());
    let events = store.read_stream(conductor::STREAM, 0, Direction::Forward)?;
    let drop = superseded_graph_nodes(&events);
    let boundary = superseded_edge_boundary(&events);
    let graph = Projector::open(&loc.file("graph.db"), &loc.identity())?;
    // A graph that owes its rebuild is read as it stands, which may predate the tables the prune
    // reads: nothing is prunable from it until `rigger setup` pays that, exactly as `--runs`
    // refuses until then.
    if graph.rebuild_owed()? {
        println!("--runs: {}", contextgraph::REBUILD_OWED);
    } else {
        println!(
            "{}",
            runs_menu_line(&graph.count_prunable(&drop, boundary)?)
        );
    }

    // --derived: the migration is a mechanic of the embedded sqlite store (see `reset_derived`'s
    // own doc) - honest per backend rather than a count of a store it does not run on.
    if selection.is_sqlite() {
        let es = open_sqlite_store(&loc.file("events.db"))?;
        let counted = es.count_derived(&loc.run_stream())?;
        println!("{}", derived_menu_line(selection, Some(&counted)));
    } else {
        println!("{}", derived_menu_line(selection, None));
    }
    Ok(())
}

/// The `--runs` line of [`reset_menu`], pure over the already-measured [`PruneStats`] so the
/// wording is unit-testable without a store.
fn runs_menu_line(stats: &PruneStats) -> String {
    format!(
        "--runs: {} dead-run node(s) and {} superseded edge(s) prunable from the context graph; \
         rerun `rigger reset --runs` to reclaim them",
        stats.nodes, stats.superseded_edges
    )
}

/// What a store holding no derived event is told of it, by the menu's `--derived` line and by
/// `rigger reset --derived` itself.
const NO_DERIVED_EVENT_TO_SHED: &str = "no derived event to shed";

/// How the operator is told what the derived index of a log holds (spec 107): `events` derived
/// events of `identities` file identities, the two numbers [`Store::count_derived`] answers as
/// its count of events shed and the size of its set of identities.
pub(crate) fn derived_count_phrase(events: usize, identities: usize) -> String {
    format!("{events} derived events of {identities} file identities")
}

/// The `--derived` line of [`reset_menu`], pure over the already-read count (or its absence, on
/// a backend the migration does not run on) so every branch is unit-testable without a store or
/// a live server: `counted` is `Some` on the sqlite backend (`selection.is_sqlite()`) and `None`
/// on any other, and this reads `selection` only to name the backend it is honest about.
///
/// A count holding no derived event says there is none to shed; any other names the events, the
/// file identities holding them ([`derived_count_phrase`]) and the flag - decided by the events
/// alone, so a store whose derived rows all name no identity is still told what `--derived`
/// sheds.
fn derived_menu_line(selection: &StoreSelection, counted: Option<&DerivedCount>) -> String {
    match counted.map(|counted| (counted.shed, counted.identities.len())) {
        Some((0, _)) => format!("--derived: {NO_DERIVED_EVENT_TO_SHED}"),
        Some((events, identities)) => format!(
            "--derived: {} to shed from the event log; rerun `rigger reset --derived` to \
             migrate them",
            derived_count_phrase(events, identities)
        ),
        None => {
            debug_assert!(
                !selection.is_sqlite(),
                "derived_menu_line: a `None` count on the sqlite backend would hide a real \
                 measurement the caller could have taken"
            );
            "--derived: unavailable on this backend - the migration rewrites and deletes rows of \
             the event log and vacuums the file, a mechanic of the embedded sqlite events.db \
             store; this project is configured for the server-backed store, where the migration \
             does not run"
                .to_string()
        }
    }
}

/// Which prunes one `rigger reset` invocation was asked for.
struct ResetModes {
    runs: bool,
    derived: bool,
    /// spec 77 criterion 5 (BOUNDED SHARED CACHE): reclaim the shared gate build cache -
    /// a pure cache (always safe to cold-rebuild), so this mode carries no store-mutation
    /// implication at all and composes freely with `runs`/`derived`.
    build_cache: bool,
    /// Reclaim every cache-home scratch root whose repo no longer exists - the on-demand
    /// form of the sweep [`rigger::worktree::scratch_root_with`] runs whenever it creates a
    /// default-placed root. Touches no store and composes with every other mode.
    scratch_orphans: bool,
    /// The override for `--derived`'s live-writer guard (spec 71, criterion 2): skips
    /// [`refuse_derived_reset_if_live`] entirely rather than acting on what it would have found -
    /// the operator asked to compact WHATEVER the run machinery looks like, and this flag owns
    /// that risk (see its help text and [`live_writer_refusal`]). Meaningless on its own; only
    /// `--derived` ever reads it. Not itself a mode - a bare `--force-live` with no
    /// `--runs`/`--derived` still falls through the "at least one mode" refusal below exactly as
    /// before this flag existed.
    force_live: bool,
}

impl ResetModes {
    /// Whether a selected mode reads [`ResetEnv`]: `--runs` and `--build-cache` always, and
    /// `--derived` unless `--force-live` skips its live-writer guard. `--scratch-orphans` reads
    /// none of it.
    fn reads_env(&self) -> bool {
        self.runs || self.build_cache || (self.derived && !self.force_live)
    }
}

/// Parse `rigger reset`'s flags: any combination of the named modes, in any order, each at most
/// once, and at least one of them; `--force-live` composes with either and is at most once too.
///
/// Every mode is explicit and an unrecognized argument is REFUSED rather than ignored, because
/// both failure modes here are silent: a bare `reset` that guessed a mode would prune something
/// the operator did not ask for, and a tolerated typo would report success for work it never did.
fn reset_modes(args: &[String]) -> Result<ResetModes, Box<dyn std::error::Error>> {
    let mut modes = ResetModes {
        runs: false,
        derived: false,
        build_cache: false,
        scratch_orphans: false,
        force_live: false,
    };
    for arg in args {
        let slot = match arg.as_str() {
            "--runs" => &mut modes.runs,
            "--derived" => &mut modes.derived,
            "--build-cache" => &mut modes.build_cache,
            "--scratch-orphans" => &mut modes.scratch_orphans,
            "--force-live" => &mut modes.force_live,
            other => {
                return Err(format!(
                    "reset: expected --runs and/or --derived and/or --build-cache and/or \
                     --scratch-orphans (with an optional --force-live), got {other}: rigger \
                     reset --runs | rigger reset --derived [--force-live] | rigger reset \
                     --build-cache | rigger reset --scratch-orphans"
                )
                .into())
            }
        };
        if *slot {
            return Err(format!("reset: {arg} was given more than once").into());
        }
        *slot = true;
    }
    if !modes.runs && !modes.derived && !modes.build_cache && !modes.scratch_orphans {
        return Err(
            "reset: expected at least one mode: rigger reset --runs (prune the context \
                    graph), rigger reset --derived (migrate the event log's derived events into ledger \
                    entries), rigger reset \
                    --build-cache (reclaim the shared gate build cache), and/or rigger reset \
                    --scratch-orphans (reclaim cache-home scratch roots whose repo is gone)"
                .into(),
        );
    }
    Ok(modes)
}

/// `rigger reset --scratch-orphans`: reclaim every root under `<cache-home>/rigger` whose
/// repo no longer exists ([`rigger::worktree::sweep_orphan_scratch_roots`]) and report how
/// many went. The directory is resolved from the ambient `XDG_CACHE_HOME`/`HOME` exactly as
/// the default scratch-root rung resolves it, so the sweep and the placement can never name
/// different directories.
fn reset_scratch_orphans() -> Res {
    let Some(cache_home) = rigger::driver::replay::cache_home_from(
        std::env::var_os("XDG_CACHE_HOME"),
        std::env::var_os("HOME"),
    ) else {
        return Err(
            "reset --scratch-orphans: neither XDG_CACHE_HOME nor HOME is set, so \
                    there is no cache-home scratch directory to sweep"
                .into(),
        );
    };
    let dir = cache_home.join("rigger");
    let reclaimed = rigger::worktree::sweep_orphan_scratch_roots(&dir);
    println!(
        "--scratch-orphans: reclaimed {reclaimed} scratch root(s) whose repo no longer exists \
         under {}",
        dir.display()
    );
    Ok(())
}

/// `rigger reset --build-cache` (spec 77 criterion 5, BOUNDED SHARED CACHE; gap 96): reclaims
/// every dead footprint class `rigger validate` names with this verb - dead per-unit caches,
/// dead spawns' registered scratch, unowned agent scratch ([`reclaim_dead_footprint`] over the
/// one [`super::validate::measure_footprint`] accounting, skipping any entry a live process
/// holds) - and the shared gate build cache under the resolved scratch root. All of it is
/// rebuildable scratch, so this is the one reset mode with no store-mutation implication at
/// all and no backend requirement (unlike `--derived`).
///
/// The scratch root and `workdir` are [`ResetEnv`]'s, resolved once and fail-closed by
/// [`cmd_reset`] from the ALREADY-RESOLVED store dir through [`marker_root`], the read-only
/// `scratch_root_path_from_env(repo, workdir)` authority `rigger validate`'s residue scan and
/// the live-writer guard also resolve through - so `reset --build-cache` can never disagree with
/// either about which directory is "the shared cache".
///
/// Delegates the actual reclaim to [`reclaim_shared_build_cache`], the ONE mutation
/// authority over this resource, and reports what happened via
/// [`build_cache_reclaim_report`]: bytes reclaimed on success, or a loud, non-zero-exit
/// refusal when a rigger-launched shared-cache build holds the guard.
fn reset_build_cache(env: &ResetEnv) -> Res {
    let scratch = PathBuf::from(&env.scratch_root);
    // Every dead class the footprint accounting names with this verb first (gap 96), so a
    // busy shared cache below never keeps the rest from being reclaimed - unless the run log
    // cannot be read: then nothing says which units and spawns are live, and every
    // liveness-dependent class is left alone (FAIL CLOSED), reported after the shared cache,
    // whose own guard lock decides it independently.
    let (categories, liveness_unknown) = super::validate::measure_footprint(&cwd(), &env.workdir)?;
    if liveness_unknown.is_none() {
        for line in footprint_reclaim_lines(&reclaim_dead_footprint(&categories)) {
            println!("{line}");
        }
    }
    let cache = scratch.join(rigger::worktree::SHARED_BUILD_CACHE_NAME);
    let outcome = reclaim_shared_build_cache(&cache, &scratch)?;
    println!("{}", build_cache_reclaim_report(outcome)?);
    match liveness_unknown {
        None => Ok(()),
        Some(why) => Err(format!(
            "reset --build-cache: reclaimed nothing from per-unit caches, registered scratch \
             roots or unowned agent scratch - {why}, so which units and spawns are live cannot \
             be read, and an unknown is never treated as dead; repair the run log and rerun"
        )
        .into()),
    }
}

/// The line (success) or refusal (busy) `rigger reset --build-cache` reports, rendered from
/// the already-computed [`BuildCacheReclaim`] - pure, mirroring [`reclamation_lines`]'s own
/// "render from the real outcome, never re-derive it" convention.
fn build_cache_reclaim_report(outcome: BuildCacheReclaim) -> Result<String, String> {
    match outcome {
        BuildCacheReclaim::Reclaimed(bytes) => Ok(format!(
            "--build-cache: reclaimed {} ({bytes} byte(s)) from the shared gate build cache",
            human_size(bytes)
        )),
        BuildCacheReclaim::Busy => Err(
            "reset --build-cache: the shared gate build cache is in use by a rigger-launched \
             build (its guard lock is held); refusing rather than waiting - retry once it is \
             idle"
                .to_string(),
        ),
    }
}

/// `rigger reset --derived` (spec 107) - THE ONE-TIME MIGRATION of an event log recorded before
/// the ledger: it converts each file's latest derived batch into a ledger entry, in place, and
/// leaves no derived event behind.
///
/// It is orchestration over ONE store transaction
/// ([`rigger::eventstore::sqlite::Store::shed_derived`]) on the project's run stream, the one
/// stream a sink ever appended a derived event to, named through the SAME stream-prefix spelling
/// every namespaced read and write of this project uses (`StoreLocation::run_stream`). Before that
/// transaction opens it reads what the stream holds ([`Store::count_derived`]) and answers, for
/// every identity holding a derived event, the blob and the flag its entry records
/// ([`tree_entries`]) under THE ONE ROOT ([`tree_root`]); `hash` is the one hash function, handed
/// in by the composition root, and a hash that fails fails the command before any row changes.
/// A store holding no derived event has nothing to shed and says so.
///
/// Either way it then reclaims the space the log's file holds free
/// ([`Store::reclaim_space`]) against the size the log had on disk before the transaction, and
/// prints what that did ([`reclamation_lines`]), so a rerun after a reclamation that failed
/// reclaims what it left.
///
/// The sqlite store is constructed through [`open_sqlite_store`], the one sqlite event-log
/// constructor (§48), exactly as the local identity migration does when it needs the concrete
/// store for a maintenance operation the port does not carry.
///
/// It refuses, in this order and before it changes anything: a rebuild lock another holds
/// ([`Projector::lock_rebuild`]), which it takes first and holds across the migration; a rebuild
/// left unfinished ([`Projector::rebuild_unfinished`]), naming `rigger setup`; and a standing
/// `graph.db` that owes its rebuild. It appends nothing, folds nothing and writes no projection
/// or ledger row to `graph.db`.
fn reset_derived(loc: &StoreLocation, hash: &HashBlob) -> Res {
    // The rebuild lock first, before anything more is read, and held until this returns (spec
    // 107): no rebuild starts under the migration, and another already holding it - a `rigger
    // setup` or a `rigger reset` - refuses this at once.
    let graph_db = loc.file("graph.db");
    let held = Projector::lock_rebuild(&graph_db)?;
    // A rebuild left unfinished is `rigger setup`'s to finish. The check opens no graph file that
    // does not stand; with no shadow beside it, it opens a standing `graph.db` to read its cursor.
    if Projector::rebuild_unfinished(&held)? {
        return Err(
            "reset --derived: a rebuild of graph.db was left unfinished - run `rigger setup` to \
             finish it"
                .into(),
        );
    }
    // A `graph.db` that owes its rebuild is refused here as every command that depends on the
    // fold refuses it (spec 101): the migration runs once `rigger setup` has paid the rebuild.
    if Path::new(&graph_db).exists() {
        open_graph(&graph_db, &loc.identity(), "reset --derived")?;
    }
    let store = open_sqlite_store(&loc.file("events.db"))?;
    // THE OPERATOR'S BEFORE, taken before the migration's transaction opens: what the
    // reclamation reports is the space the log lost on disk across the whole command.
    let on_disk_before = store.bytes_on_disk();
    let stream = loc.run_stream();
    let counted = store.count_derived(&stream)?;
    if counted.shed == 0 {
        println!("reset --derived: {NO_DERIVED_EVENT_TO_SHED}");
    } else {
        let entries = tree_entries(&tree_root(&loc.dir), &counted.identities, hash)?;
        // A TOTAL function: an identity the read above did not count - one recorded since - gets
        // no blob and a clear flag.
        let shed = store.shed_derived(&stream, &|identity| {
            entries.get(identity).cloned().unwrap_or_default()
        })?;
        println!(
            "reset --derived: converted {} latest batch(es) into ledger entries and shed {} \
             derived event(s) from the event log",
            shed.converted, shed.shed
        );
        println!(
            "reset --derived: {} of the derived event(s) shed named no file identity (no replay \
             key, or one that does not parse)",
            shed.unkeyed
        );
    }
    for line in reclamation_lines(&store.reclaim_space(on_disk_before)) {
        println!("{line}");
    }
    Ok(())
}

/// The one hash function as [`reset_derived`] takes it: the object id of `bytes` under a root.
type HashBlob<'h> = dyn Fn(&Path, &[u8]) -> Result<String, Box<dyn std::error::Error>> + 'h;

/// What the ledger entry of each of `identities` records of the tree at `root` (spec 107): the
/// blob `hash` gives the bytes the tree's one read rule ([`rigger::grounder::tree_bytes`]) hands
/// for the identity's path - none for a path it hands no bytes for - and whether the walk
/// excludes the identity as an out-of-line test module's ([`rigger::ingest::walk_exclusions`]).
/// A hash that fails fails the whole answer.
fn tree_entries(
    root: &Path,
    identities: &std::collections::BTreeSet<String>,
    hash: &HashBlob,
) -> Result<std::collections::BTreeMap<String, (String, bool)>, Box<dyn std::error::Error>> {
    let (_, excluded) = rigger::ingest::walk_exclusions(&root.to_string_lossy());
    identities
        .iter()
        .map(|identity| {
            let bytes = rigger::retention::GenerationIngested::identity_parts(identity)
                .and_then(|(prefix, file)| rigger::grounder::tree_bytes(root, prefix, file));
            let blob = match bytes {
                Some(bytes) => hash(root, &bytes)?,
                None => String::new(),
            };
            Ok((identity.clone(), (blob, excluded.contains(identity))))
        })
        .collect()
}

/// The lines `rigger reset --derived` prints for what reclaiming the log's space did, a pure
/// function of the [`Reclamation`](rigger::eventstore::sqlite::Reclamation): one rendering per
/// state the store tells apart, because four of the five are unreachable from a happy-path run of
/// the binary and each exists to be READ correctly by an operator.
///
/// The command's own change has committed before any of this is decided, so none of them is a
/// failure of the command:
///   - the rewrite failed: the failure by name, and the two facts that follow from the ordering
///     (what the run changed is durable; a re-run is safe, and it retries the reclamation because
///     what triggers the rewrite is the free space still in the file).
///   - the file had no free space to reclaim: it is deliberately not rewritten. Read from
///     `compaction_ran`, never inferred from a zero count.
///   - the reclamation was measured: the bytes the log lost on disk.
///   - the truncating checkpoint was declined by a concurrent reader: the freed pages stay in
///     the write-ahead log, so it is reported as unmeasured rather than as a byte count the
///     operator's own `ls` would contradict.
///   - the database has no file behind it at all: there was never a before-measurement to take.
///     Read from `on_disk_measured`, never folded into the declined-checkpoint arm, which would
///     name a concurrent reader this function was not handed.
fn reclamation_lines(reclamation: &rigger::eventstore::sqlite::Reclamation) -> Vec<String> {
    match (
        &reclamation.compaction_error,
        reclamation.compaction_ran,
        reclamation.reclaimed_bytes,
        reclamation.on_disk_measured,
    ) {
        (Some(err), _, _, _) => vec![
            format!("reset --derived: the log file could NOT be compacted: {err}"),
            "reset --derived: what this run changed is committed and durable, so re-running the \
             command is safe - and it retries the reclamation, because the space this run could \
             not reclaim is still free in the file"
                .to_string(),
        ],
        (None, false, _, _) => vec![
            "reset --derived: the log file holds no reclaimable free page, so it was left as it \
             stands rather than rewritten to reclaim nothing"
                .to_string(),
        ],
        (None, true, Some(bytes), _) => vec![format!(
            "reset --derived: compacted the log file and reclaimed {bytes} byte(s) on disk"
        )],
        (None, true, None, true) => vec![
            "reset --derived: compacted the log file, but the freed pages could not be folded \
             back into it: a concurrent reader held the write-ahead log, so they land at the next \
             checkpoint and this run reclaimed an unmeasured amount"
                .to_string(),
        ],
        (None, true, None, false) => vec![
            "reset --derived: compacted the log, which has no file behind it (an in-memory or \
             temporary database): there are no bytes on disk to have been reclaimed, so the \
             reclamation is unmeasured rather than zero"
                .to_string(),
        ],
    }
}

/// The reasons `rigger reset --derived` must refuse, from the three already-gathered facts that
/// make a run LIVE (spec 101, THE LIVE-WRITER GUARD READS LIVENESS, owns this definition; the
/// refusal itself is spec 71, criterion 2's). The recorded incident this guard exists to prevent
/// is a compaction that ran WHILE a writer was still appending, and each fact is a writer proving
/// it is alive right now:
///   - `step_lock_held`: a `rigger step` is running right now, possibly mid-wave before it has
///     even parked a spawn (see [`acquire_step_lock`]).
///   - `live_spawns`: an in-flight spawn of the current run - no recorded result, or only the
///     step's liveness fault, which a resumed worker outlives - whose liveness marker is younger
///     than its own wall-clock bound ([`rigger::liveness::live_spawns`]): a worker still running
///     its own `rigger emit`/`rigger result` couriers even with no `step`/`run`/`serve` process
///     alive.
///   - `driver_registrations`: entries in the machine-global instance registry (spec 50) for THIS
///     project's exact store whose DRIVER heartbeat is younger than
///     [`rigger::registry::DEFAULT_IDLE_MS`] ([`live_driver_registrations`]): an in-process
///     `rigger run`/`serve` (which never touches `step.lock`), or a driver between two of its
///     `rigger step`s (every step registers as the run's driver, so its last stamp counts for the
///     idle window), elsewhere on this machine. A courier's discovery refresh is not one.
///
/// Unit terminality is NOT a liveness signal, and neither is an in-flight spawn on its own,
/// without a marker inside its bound: a run whose driver died leaves both behind forever, and
/// reading them as live made the run whose bloat most needs the compaction the one that refused
/// it.
///
/// Pure (no IO) so the composition - list EVERY applicable reason, never just the first, so an
/// operator sees the whole picture in one refusal instead of clearing one and retrying into the
/// next - is unit-tested without any of the three. Empty means quiet: safe to compact.
fn live_writer_reasons(
    step_lock_held: bool,
    live_spawns: &[rigger::liveness::LiveSpawn],
    driver_registrations: usize,
) -> Vec<String> {
    let mut reasons = Vec::new();
    if step_lock_held {
        reasons
            .push("a `rigger step` is running right now (it holds .rigger/step.lock)".to_string());
    }
    if !live_spawns.is_empty() {
        let named: Vec<String> = live_spawns.iter().map(live_spawn_named).collect();
        reasons.push(format!(
            "{} in-flight spawn(s) in the current run touched their liveness marker within their \
             wall-clock bound: {}",
            live_spawns.len(),
            named.join(", "),
        ));
    }
    if driver_registrations > 0 {
        reasons.push(format!(
            "{driver_registrations} driver registration(s) for this project's store in the \
             machine-global instance registry (spec 50) heartbeat within the last {}s - a \
             `run`/`serve`/`step` may be advancing this run elsewhere on this machine",
            rigger::registry::DEFAULT_IDLE_MS / 1000,
        ));
    }
    reasons
}

/// One live spawn as a refusal names it: its id, how long ago its marker was touched, and the
/// bound that keeps it live. An unbounded spawn's marker never goes stale (the step's own sweep
/// never calls it hung either), so the only thing that ends it is its recorded result - named
/// here, since that is the operator's way past it once its worker is gone.
fn live_spawn_named(s: &rigger::liveness::LiveSpawn) -> String {
    let ago = s.silent_for.as_secs();
    if s.bound.is_zero() {
        format!(
            "{id} (marker touched {ago}s ago; unbounded, so it stays live until its result is \
             recorded - `rigger result {id} --error <why>` once its worker is gone)",
            id = s.id,
        )
    } else {
        format!(
            "{} (marker touched {ago}s ago, bound {}s)",
            s.id,
            s.bound.as_secs()
        )
    }
}

/// The loud refusal `rigger reset --derived` prints for a non-empty [`live_writer_reasons`]:
/// names every reason, the concrete risk, and the override. `--force-live` is named here and in
/// its own help text (spec 71: "an explicit override flag whose help text owns the risk") so an
/// operator reads the same risk-owning sentence wherever they meet the flag.
fn live_writer_refusal(reasons: &[String]) -> String {
    format!(
        "reset --derived: refusing to migrate the event log while the run is live - {}. The \
         migration rewrites each file's latest derived batch into a ledger entry and deletes \
         every other derived event, which leaves REVISION GAPS by design; a writer whose append \
         cursor was built before this migration ran can reissue one of those gap revisions, and \
         every later event then sorts BELOW it in revision order - the incident this guard \
         exists to prevent, and the corruption forcing past a genuinely live writer would risk. \
         Stop the run machinery named above and retry, or pass \
         --force-live to migrate anyway if you are certain no writer is using this store \
         (--force-live checks nothing; it trusts you with that risk).",
        reasons.join("; "),
    )
}

/// Gather [`live_writer_reasons`]'s three facts and refuse `rigger reset --derived` (spec 71,
/// criterion 2) when any applies. IMPURE (a lock probe, a store read, marker reads, an optional
/// registry read) so the decision composition itself stays pure and unit-tested without any of
/// the three. When nothing is live it returns the step lock its probe took, still held, for the
/// caller to hold until the compaction returns ([`LiveWriterProbe`]).
///
/// The registry directory, the marker root and the instant are INJECTED through `env` (mirrors
/// [`dash_resolve_attach`]'s existing DI shape) rather than read ambiently in here: the
/// composition root (`cmd_reset`) resolves them once ([`ResetEnv::resolve`], the marker root
/// fail-closed), exactly as every other ambient-environment read in this crate is pushed to a
/// caller rather than repeated inside a callee. A `None` registry directory (a homeless
/// environment) degrades to zero registrations - the same degrade `register_run_instance` itself
/// takes for the identical reason: the registry's loss is harmless discovery metadata, never a
/// signal this guard can invent.
///
/// This probe is read-only in effect, not just in name: it reads via
/// [`rigger::registry::read_live_no_prune`], never [`rigger::registry::read_live`], so checking
/// for live writers here never deletes a stale registry entry as a side effect - `reset
/// --derived` performs no registry hygiene of its own; deletion stays reserved exclusively to
/// the dashboard's self-reap watcher tick (see `read_live_no_prune`'s doc for why a prune here
/// would be unsafe).
///
/// FAIL-SAFE in two different ways for two different faults:
///   - a run-stream read failure (a malformed spawn request, or a malformed result where a
///     marker inside its bound needs it read) propagates as a command error rather than folding
///     into "no in-flight spawns" - this guard may only REFUSE a prune, never approve one it
///     could not actually verify was safe (mirrors
///     [`rigger::liveness::terminal_and_no_live_worker`]'s convention on the opposite rail: an
///     unreadable stream is never read as "nobody is here").
///   - a step-lock probe error that is NOT the lock actually being held (e.g. a permission
///     fault) also propagates as a command error rather than being misread as "a step is
///     running": only [`STEP_BUSY_TOKEN`] in the probe's own error names a genuinely held lock,
///     so an operator troubleshooting an unrelated IO fault gets that fault's own message
///     instead of a misdiagnosis pointing them at a `rigger step` that is not actually running.
fn refuse_derived_reset_if_live(
    loc: &StoreLocation,
    selection: &StoreSelection,
    env: &ResetEnv,
) -> Result<Option<HeldLock>, Box<dyn std::error::Error>> {
    let backend = resolve_store(selection, &loc.file("events.db"))?;
    let store = Namespaced::new(backend.as_ref(), &loc.identity());
    let events = store.read_stream(conductor::STREAM, 0, Direction::Forward)?;
    let probe = live_writer_facts(loc, selection, env, &events)?;
    let reasons = probe.facts.reasons();
    if reasons.is_empty() {
        return Ok(probe.step_lock);
    }
    Err(live_writer_refusal(&reasons).into())
}

/// The three facts [`live_writer_reasons`] composes, gathered once so both consumers read the
/// same liveness: `reset --derived`'s refusal ([`refuse_derived_reset_if_live`]) and `reset
/// --runs`'s dead-driver test ([`close_landed_units`]).
struct LiveWriterFacts {
    step_lock_held: bool,
    live_spawns: Vec<rigger::liveness::LiveSpawn>,
    driver_registrations: usize,
}

impl LiveWriterFacts {
    fn reasons(&self) -> Vec<String> {
        live_writer_reasons(
            self.step_lock_held,
            &self.live_spawns,
            self.driver_registrations,
        )
    }

    /// Nothing is driving the run: none of the facts that make it live holds - the one
    /// definition `reset --derived` refuses on. A non-terminal unit, a spawn with no marker, or a
    /// spawn whose marker has outlived its bound (whether it has no result or only the step's
    /// liveness fault) is not a driver - it is exactly what a dead driver leaves behind.
    fn driver_dead(&self) -> bool {
        self.reasons().is_empty()
    }
}

/// [`LiveWriterFacts`] as [`live_writer_facts`] probed them, with the step lock the probe took.
/// The lock is HELD for as long as the probe lives, so the probe's verdict that no `rigger step`
/// is running stays true while the caller acts on it: a step started meanwhile refuses with
/// [`STEP_BUSY_TOKEN`] and its courier retries, never running against the close or the compaction
/// the caller makes under the probe.
struct LiveWriterProbe {
    facts: LiveWriterFacts,
    /// The step lock this probe holds; `None` exactly when a `rigger step` already held it
    /// (`facts.step_lock_held`).
    step_lock: Option<HeldLock>,
}

/// Probe [`LiveWriterFacts`] over `events` (the whole run stream), reading each of the current
/// run's spawn markers under the root its request recorded (`env`'s scratch root only for one that
/// recorded none) and judging every signal at `env`'s one instant, and keep the step lock the
/// probe took ([`LiveWriterProbe`]). IMPURE (a lock probe, marker reads and an optional registry
/// read) so the decisions built on it stay pure and unit-tested.
fn live_writer_facts(
    loc: &StoreLocation,
    selection: &StoreSelection,
    env: &ResetEnv,
    events: &[Event],
) -> Result<LiveWriterProbe, Box<dyn std::error::Error>> {
    // A non-blocking probe of the SAME advisory lock `rigger step` holds for its whole duration,
    // resolved at THIS STORE's own directory (never the process cwd) - `reset --derived` is run
    // from a nested worktree just as every other courier is (see `require_store_dir`), and a
    // cwd-relative probe would open a `.rigger/step.lock` under the WRONG (or nonexistent)
    // directory there. Acquiring it proves nobody else holds it, and the probe keeps holding it
    // so that stays true while the reset acts. A failure whose message names the busy token
    // proves a step IS running; any OTHER failure (a permission fault, a read-only filesystem) is
    // a real fault this command cannot silently misdiagnose as "held", so it propagates instead.
    let step_lock = match acquire_step_lock(&loc.dir) {
        Ok(lock) => Some(lock),
        Err(e) if e.to_string().contains(STEP_BUSY_TOKEN) => None,
        Err(e) => return Err(e),
    };

    // The current run's spawns under the one live-spawn rule `rigger step`'s halted-spawn
    // checkpoint also applies: no real result, and a marker inside the spawn's own bound.
    let live_spawns = rigger::liveness::live_spawns(
        runscope::current_run(events),
        &env.scratch_root,
        &runscope::current_run_id(events).unwrap_or_default(),
        std::time::UNIX_EPOCH + std::time::Duration::from_millis(env.now_ms),
    )?;

    let driver_registrations = env
        .registry_dir
        .as_deref()
        .map(|dir| {
            let root = loc
                .dir
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."));
            let expected = registry_store_identity(selection, &root);
            live_driver_registrations(dir, &expected, env.now_ms)
        })
        .unwrap_or(0);

    Ok(LiveWriterProbe {
        facts: LiveWriterFacts {
            step_lock_held: step_lock.is_none(),
            live_spawns,
            driver_registrations,
        },
        step_lock,
    })
}

/// How many entries in the machine-global registry at `dir` record a live DRIVER of the store
/// `expected` as of `now_ms` ([`rigger::registry::Instance::driver_live`]). A courier's discovery
/// re-stamp (`emit`, `result`, `progress`) is not one: it drives nothing, and counting it would
/// read an operator's own courier, seconds before a reset, as the live driver that keeps the
/// reset from closing a hand-landed unit. A courier entry still counts while it carries a fresh
/// driver stamp, so a live driver's own agent never hides that driver.
fn live_driver_registrations(
    dir: &Path,
    expected: &rigger::registry::StoreIdentity,
    now_ms: u64,
) -> usize {
    // `read_live_no_prune`, not `read_live`: this probe must never delete a foreign project's
    // registry entry as a side effect of checking THIS store for live writers (spec 62
    // criterion 5 round 4 - see `read_live_no_prune`'s doc).
    rigger::registry::read_live_no_prune(dir, now_ms, rigger::registry::DEFAULT_IDLE_MS)
        .into_iter()
        .filter(|inst| {
            inst.store == *expected && inst.driver_live(now_ms, rigger::registry::DEFAULT_IDLE_MS)
        })
        .count()
}

/// `rigger reset --runs` (spec 21, unit 2) - drop the decisions and findings of every
/// SUPERSEDED / dead run from the context graph, PRESERVING every `LessonLearned` and the
/// active run's decisions and findings. It is the supported way to shed dead-run noise
/// without deleting the whole store: this prune DELETES NO EVENT, so `rigger stats`, replay,
/// and cross-run history stay intact - only the graph the grounder reads is pruned (there is
/// no way to shed the noise today short of wiping `graph.db` wholesale).
///
/// WHAT THE COMMAND AROUND IT DOES WRITE TO THE LOG, stated here because this function's own
/// report used to promise an untouched log and no longer can: [`cmd_reset`] runs the one-time
/// spec-09 identity migration before EITHER mode, so on a store still filed under the legacy
/// basename namespace that migration renames its streams to the minted identity and appends one
/// `DecisionMade` recording the rename. No event is dropped, reordered, or altered in content by
/// it, and it prints its own line when it fires - but "the event log is untouched" is not true of
/// a `rigger reset --runs` on the one class of store the migration exists for, so the printed
/// report says what IS true instead. The migration is deliberately not gated on `--derived`:
/// `reset_runs` reads through `Namespaced::new(backend, &loc.identity())`, so skipping it on an
/// unmigrated store would read an empty stream and report a confident prune of zero dead-run
/// nodes - the silent no-op the migration is there to prevent, moved from one mode to the other.
///
/// This is pure orchestration over two single authorities: the disposition comes from the
/// run-attribution primitive ([`superseded_graph_nodes`] over `run::run_attribution` +
/// `run::current_run_id`), and the deletion is the graph-mutation primitive
/// ([`Projector::prune`]). ONE whole-stream forward read feeds the attribution AND the
/// node-id lookup (the index-keying contract `run_attribution` documents - a filtered slice
/// would misattribute); the derived node ids are then handed to the prune.
fn reset_runs(loc: &StoreLocation, selection: &StoreSelection, env: &ResetEnv) -> Res {
    // The private pruned copy a rebuild's stopped swap left is a graph file with no other reaper
    // but the next rebuild: removed first, whether or not the graph goes on to refuse the prune,
    // and kept, naming the lock's holder, while another holds the rebuild lock (spec 101).
    let graph_db = loc.file("graph.db");
    let copy = contextgraph::sqlite::pruned_copy("graph.db");
    match Projector::forget_stale_copy(&graph_db)? {
        contextgraph::sqlite::StaleCopy::Absent => {}
        contextgraph::sqlite::StaleCopy::Removed => println!(
            "reset --runs: removed {copy}, the pruned copy a rebuild's stopped swap left beside graph.db"
        ),
        contextgraph::sqlite::StaleCopy::InUse => println!(
            "reset --runs: kept {copy}: {}",
            contextgraph::sqlite::REBUILD_IN_PROGRESS
        ),
    }
    let backend = resolve_store(selection, &loc.file("events.db"))?;
    let store = Namespaced::new(backend.as_ref(), &loc.identity());
    // ONE whole-stream forward read: it feeds BOTH the attribution and the per-index node-id
    // lookup inside `superseded_graph_nodes`, honoring run_attribution's whole-stream contract.
    let events = store.read_stream(conductor::STREAM, 0, Direction::Forward)?;
    let drop = superseded_graph_nodes(&events);
    // Spec 41: the retention cutoff for the superseded-edge reclamation - the active run's start,
    // the SAME run boundary the node drop-set is derived against. `None` (a legacy store with no
    // run) reclaims no edge, so LIVE and recent history are both untouched.
    let boundary = superseded_edge_boundary(&events);

    let graph = open_graph(&graph_db, &loc.identity(), "reset --runs")?;
    // The probe's step lock is held until the close returns, so no `rigger step` starts against a
    // run this reset is closing; the graph prune below takes no part in it.
    let probe = live_writer_facts(loc, selection, env, &events)?;
    close_landed_units(loc, &store, &graph, &events, &probe.facts)?;
    std::mem::drop(probe);
    let removed = graph.prune(&drop, boundary)?;
    // Compact the projection file so the prune reclaims DISK, not just rows (spec 46, criterion 3):
    // the deletes free pages inside graph.db that SQLite retains on a freelist, so without a VACUUM
    // the file stays as LARGE on disk as before even though the dead rows are gone. VACUUM reclaims
    // disk ONLY - it changes no query result and gives no query or fold speedup; it rebuilds only
    // the rebuildable projection and the event log is untouched.
    let reclaimed_bytes = graph.compact()?;
    println!(
        "reset --runs: {} from the context graph, then compacted the graph file (reclaimed {} \
         byte(s) on disk) - every \
         lesson, the active run, and every live edge are preserved; this prune deletes no event \
         from the log. The one thing `rigger reset` writes there is the one-time identity \
         migration it runs first: on a store still under the legacy basename namespace that \
         renames its streams and records one DecisionMade, and it prints its own line when it \
         does",
        pruned_line(&removed),
        reclaimed_bytes
    );
    Ok(())
}

/// What a run-closure prune removed, in the one set of words both of its reports use: `rigger
/// reset --runs` for its prune of the live graph, and `rigger setup` for the prune its rebuild
/// makes of the rebuilt one (spec 101).
pub(crate) fn pruned_line(stats: &PruneStats) -> String {
    format!(
        "pruned {} dead-run node(s) and reclaimed {} superseded edge(s)",
        stats.nodes, stats.superseded_edges
    )
}

/// Close the current run's hand-landed units: when nothing drives the run
/// ([`LiveWriterFacts::driver_dead`]) and every spawn of the run has ended on a real result
/// ([`rigger::liveness::unended_spawns`], the one authority the run teardown reads too), record
/// the `UnitIntegrated` the conductor never minted for every unit whose branch work is landed on
/// the run branch ([`rigger::worktree::landed_branch_tip`]). A run the operator finished by hand
/// otherwise stays "working" forever, because only the conductor mints that event and `rigger
/// emit` refuses it. Appends only - no event is deleted or rewritten - and a live run is never
/// touched.
///
/// A spawn that has not ended is resumed whatever its marker says: one with no result stays in
/// the step's wave, and one answered only by the step's liveness fault is hung - the step halts on
/// it and the replay driver re-parks it - so closing its unit would hand a relaunched driver a
/// spawn of an integrated unit. A pending spawn leaves the landed units open silently; a hung one
/// leaves them open naming each with the hung spawns and their remedy, a real result recorded
/// with `rigger result`. The spawns are read before the liveness test, so a spawn event the log
/// cannot decode fails the command rather than reading as "nothing awaits".
fn close_landed_units(
    loc: &StoreLocation,
    store: &dyn EventStore,
    graph: &Projector,
    events: &[Event],
    facts: &LiveWriterFacts,
) -> Res {
    let current = runscope::current_run(events);
    let unended = rigger::liveness::unended_spawns(current)?;
    if !unended.pending.is_empty() || !facts.driver_dead() {
        return Ok(());
    }
    let run = ledger::project(current)?;
    let repo = loc.repo_root();
    let landed = landed_units(&run, |branch| {
        rigger::worktree::landed_branch_tip(&repo, branch, RUN_BRANCH)
    });
    let run_id = runscope::current_run_id(events).unwrap_or_default();
    if !unended.hung.is_empty() {
        let hung = rigger::liveness::halt_reason(&unended.hung);
        for (unit, tip) in &landed {
            println!(
                "reset --runs: left unit {unit:?} of run {run_id} open although its branch tip \
                 {tip} is landed on {RUN_BRANCH}: {hung}"
            );
        }
        return Ok(());
    }
    let closing = landed
        .iter()
        .map(|(unit, tip)| {
            let body = serde_json::json!({"id": unit, "commit": tip, "by": "operator"});
            let ev = Event::new(ledger::TYPE_UNIT_INTEGRATED, serde_json::to_vec(&body)?);
            Ok(ev.with_meta(runscope::META_RUN_ID, &run_id))
        })
        .collect::<Result<Vec<Event>, serde_json::Error>>()?;
    let done = rigger::ingest::folding_into(store, Some(graph), &stderr_line).append_and_fold(
        conductor::STREAM,
        ExpectedRevision::Any,
        &closing,
    )?;
    let lost = fold_loss_clause(&done.fold);
    for (unit, tip) in &landed {
        println!(
            "reset --runs: closed unit {unit:?} of run {run_id}: no driver is alive and its \
             branch tip {tip} is landed on {RUN_BRANCH}, so its UnitIntegrated is recorded \
             (by operator){lost}"
        );
    }
    Ok(())
}

/// The units of `run` that have not integrated but whose branch `landed_tip` reports landed,
/// as `(unit, tip)` in the run's unit order. Pure over the injected landing oracle.
fn landed_units(
    run: &RunState,
    landed_tip: impl Fn(&str) -> Option<String>,
) -> Vec<(String, String)> {
    run.units
        .values()
        .filter(|u| u.status != ledger::Status::Integrated && !u.branch.is_empty())
        .filter_map(|u| landed_tip(&u.branch).map(|tip| (u.id.clone(), tip)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ev;
    use rigger::registry::{Instance, StoreIdentity, Writer, DEFAULT_IDLE_MS};

    /// Spec 21, unit 2: the drop-set derivation `rigger reset --runs` hands to the prune. It
    /// reuses the SINGLE run-attribution authority (`run_attribution` + `current_run_id`), so a
    /// SUPERSEDED run's decision/finding AND a PRE-BOUNDARY one (recorded before the first
    /// `RunStarted`) are both dropped, while every `LessonLearned` (even a pre-boundary one) and
    /// the ACTIVE run's decisions/findings are preserved. Index-keyed off the whole forward
    /// stream, mapping each dropped index back to its event body's id; sorted + de-duplicated.
    ///
    /// Two hazards are pinned here that the naive skip-live-index derivation gets wrong:
    /// - KEEP INVARIANT under cross-run id reuse: `shared-d` is recorded in BOTH the dead run
    ///   r1 AND the active run r2, so its dead-run index is a drop candidate while its
    ///   active-run index is live. The one graph node must be PRESERVED - the drop set is the
    ///   candidates MINUS the active run's node ids, not merely the non-live indices.
    /// - SENTINEL arm: a dead-run decision with an EMPTY id and one with a MALFORMED (non-JSON)
    ///   body must be SKIPPED (`graph_node_id` returns `None`), never panicking and never
    ///   contributing a bogus id to the drop set.
    #[test]
    fn superseded_graph_nodes_drops_dead_runs_and_preboundary_keeping_lessons_active_and_reused_ids(
    ) {
        fn run_started(run: &str) -> Event {
            ev(
                runscope::TYPE_RUN_STARTED,
                &format!(r#"{{"run":"{run}","criteria":["crit"]}}"#),
            )
        }
        let events = vec![
            // Pre-boundary (before any RunStarted): decision + finding DROP, lesson KEEPS.
            ev(contextgraph::TYPE_DECISION_MADE, r#"{"id":"pre-d"}"#),
            ev(contextgraph::TYPE_REVIEW_FINDING, r#"{"id":"pre-f"}"#),
            ev(contextgraph::TYPE_LESSON_LEARNED, r#"{"id":"pre-lesson"}"#),
            run_started("r1"),
            // Superseded run r1: decision + finding DROP, lesson KEEPS.
            ev(contextgraph::TYPE_DECISION_MADE, r#"{"id":"r1-d"}"#),
            ev(contextgraph::TYPE_REVIEW_FINDING, r#"{"id":"r1-f"}"#),
            ev(contextgraph::TYPE_LESSON_LEARNED, r#"{"id":"r1-lesson"}"#),
            // A decision id reused across runs, recorded here in the DEAD run r1 first.
            ev(contextgraph::TYPE_DECISION_MADE, r#"{"id":"shared-d"}"#),
            // Sentinel arms in the dead run: an empty id and a malformed (non-JSON) body must
            // be skipped, never dropped and never panicking.
            ev(contextgraph::TYPE_DECISION_MADE, r#"{"id":""}"#),
            ev(contextgraph::TYPE_REVIEW_FINDING, "not json at all"),
            run_started("r2"),
            // Active run r2: decision + finding KEEP, lesson KEEPS.
            ev(contextgraph::TYPE_DECISION_MADE, r#"{"id":"r2-d"}"#),
            ev(contextgraph::TYPE_REVIEW_FINDING, r#"{"id":"r2-f"}"#),
            ev(contextgraph::TYPE_LESSON_LEARNED, r#"{"id":"r2-lesson"}"#),
            // The SAME reused id recorded again in the ACTIVE run r2: the node must survive.
            ev(contextgraph::TYPE_DECISION_MADE, r#"{"id":"shared-d"}"#),
        ];

        let drop = superseded_graph_nodes(&events);
        assert_eq!(
            drop,
            vec!["pre-d", "pre-f", "r1-d", "r1-f"],
            "exactly the dead-run + pre-boundary decisions/findings, sorted; lessons, the active \
             run (r2), a cross-run-reused id, and malformed/empty-id events are all preserved"
        );
    }

    /// Spec 41: the retention cutoff `rigger reset --runs` hands to the extended prune for the
    /// superseded-edge reclamation. It is the ACTIVE run's `RunStarted` `valid_from` in the graph's
    /// nanosecond-since-epoch time base (an edge's stored `valid_to`), derived from the SAME
    /// `run::current_run` boundary the node drop-set uses - so a superseded edge retired before the
    /// active run is reclaimed and one retired during it is kept. With NO run started (a legacy
    /// store) it is `None`, so nothing is reclaimed and LIVE plus recent history are both untouched.
    #[test]
    fn superseded_edge_boundary_is_the_active_runs_start_or_none_without_a_run() {
        use std::time::{Duration, UNIX_EPOCH};
        fn event_at(type_: &str, data: &str, secs: u64) -> Event {
            Event::new(type_, data.as_bytes().to_vec())
                .with_valid_from(UNIX_EPOCH + Duration::from_secs(secs))
        }
        let run_started = runscope::TYPE_RUN_STARTED;
        let decision = contextgraph::TYPE_DECISION_MADE;

        // No RunStarted at all (a legacy store): no boundary, so the reclamation is skipped entirely.
        let legacy = vec![event_at(decision, r#"{"id":"d0"}"#, 50)];
        assert_eq!(
            superseded_edge_boundary(&legacy),
            None,
            "with no run started there is no boundary - nothing is reclaimed"
        );

        // Two runs: the cutoff is the LATEST (active) run's start (300s), NOT the prior run's (100s),
        // so an edge superseded during run r1 (before 300s) is reclaimable and r2's own is retained.
        let events = vec![
            event_at(run_started, r#"{"run":"r1","criteria":["crit"]}"#, 100),
            event_at(decision, r#"{"id":"r1-d"}"#, 150),
            event_at(run_started, r#"{"run":"r2","criteria":["crit"]}"#, 300),
            event_at(decision, r#"{"id":"r2-d"}"#, 350),
        ];
        assert_eq!(
            superseded_edge_boundary(&events),
            Some(Duration::from_secs(300).as_nanos() as i64),
            "the boundary is the active run's RunStarted valid_from in the edge time base (nanos)"
        );
    }

    /// Spec 43, criterion 4 (CONSUMERS ARE UNAFFECTED). The graph fold de-noises to the target
    /// project: it stopped projecting the loop's own run machinery (the agent / unit / gate NODES,
    /// the `agent --TOUCHES--> file` edge, and the agent-attribution edges). This test proves the
    /// three named functional consumers produce the SAME result before and after that de-noise,
    /// because NONE of them reads a dropped node - each reads a substrate the fold change never
    /// touches:
    ///
    ///   * `metrics::project` folds the EVENT LOG. The de-noise removed the graph NODES for
    ///     `GateVerdict` / `UnitStarted` / `UnitIntegrated`, but those EVENTS still stand in the
    ///     log, and metrics reads them from there (it never receives a `Projection`), so its counts
    ///     are unmoved.
    ///   * run pruning (`superseded_graph_nodes` -> `Projector::prune`) derives its drop set from
    ///     the EVENT LOG through the one `run::run_attribution` authority, which attributes ONLY
    ///     decisions / findings / lessons - never a `UnitStarted` / `FileTouched` / `GateVerdict`.
    ///     So no machinery id can enter the drop set, the derivation is byte-identical whether or
    ///     not the machinery events are present, and pruning the de-noised graph (which has no
    ///     machinery nodes) still drops exactly the dead-run content and keeps the active run's.
    ///   * blast radius grounds over the CODE cross-reference (here the always-available `Grep`
    ///     default over the source tree; the `symbols` grounder likewise reads its symbol index) -
    ///     never the context graph - so a graph-only change cannot alter a radius.
    ///
    /// This owns the safe-consumer guarantee; it deliberately does NOT re-assert content survival
    /// (criterion 2 owns that). The event stream carries the machinery in its RAW production shape
    /// (a `UnitStarted` with both `id` and `unit`, a `GateVerdict` with `pass`, a `FileTouched`
    /// with `by`) - the exact payloads the log records and the de-noise now ignores.
    #[test]
    fn the_denoise_leaves_metrics_run_pruning_and_blast_radius_unaffected() {
        use rigger::grounder::{Grep, Grounder};

        // One positioned event in raw on-log JSON. Distinct positions are required: the graph fold
        // dedups on position (`INSERT OR IGNORE INTO applied`), and metrics / attribution key by
        // index, so a monotonic position per event models the real append order.
        use crate::test_support::ev_at;

        // A whole run stream spanning a DEAD run r1 and the ACTIVE run r2, each interleaving the
        // machinery the de-noise dropped (FileTouched / UnitStarted / GateVerdict / UnitIntegrated)
        // with the content (DecisionMade / ReviewFinding) and the unit lifecycle metrics folds.
        let stream = vec![
            // --- Dead run r1 ---
            ev_at(
                1,
                runscope::TYPE_RUN_STARTED,
                serde_json::json!({ "run": "r1", "criteria": ["c"] }),
            ),
            ev_at(
                2,
                contextgraph::TYPE_FILE_TOUCHED,
                serde_json::json!({ "path": "src/combat.rs", "by": "rust-engineer" }),
            ),
            ev_at(
                3,
                ledger::TYPE_UNIT_STARTED,
                serde_json::json!({ "id": "u_r1", "unit": "u_r1", "criterion": "c1", "agent": "rust-engineer", "needs": [] }),
            ),
            ev_at(
                4,
                contextgraph::TYPE_GATE_VERDICT,
                serde_json::json!({ "gate": "build", "pass": true }),
            ),
            ev_at(
                5,
                contextgraph::TYPE_DECISION_MADE,
                serde_json::json!({ "id": "d_r1", "summary": "dead-run decision", "governs": ["src/combat.rs"], "supersedes": "" }),
            ),
            ev_at(
                6,
                contextgraph::TYPE_REVIEW_FINDING,
                serde_json::json!({ "id": "f_r1", "by": "tech-lens", "unit": "u_r1", "summary": "dead-run finding", "about": ["src/combat.rs"] }),
            ),
            ev_at(
                7,
                ledger::TYPE_UNIT_STATUS,
                serde_json::json!({ "id": "u_r1", "status": "verified" }),
            ),
            ev_at(
                8,
                ledger::TYPE_UNIT_STATUS,
                serde_json::json!({ "id": "u_r1", "status": "reviewed" }),
            ),
            ev_at(
                9,
                ledger::TYPE_UNIT_INTEGRATED,
                serde_json::json!({ "id": "u_r1", "commit": "abc1" }),
            ),
            // --- Active run r2 ---
            ev_at(
                10,
                runscope::TYPE_RUN_STARTED,
                serde_json::json!({ "run": "r2", "criteria": ["c"] }),
            ),
            ev_at(
                11,
                contextgraph::TYPE_FILE_TOUCHED,
                serde_json::json!({ "path": "src/combat.rs", "by": "rust-engineer" }),
            ),
            ev_at(
                12,
                ledger::TYPE_UNIT_STARTED,
                serde_json::json!({ "id": "u_r2", "unit": "u_r2", "criterion": "c1", "agent": "rust-engineer", "needs": [] }),
            ),
            ev_at(
                13,
                contextgraph::TYPE_GATE_VERDICT,
                serde_json::json!({ "gate": "clippy", "pass": true }),
            ),
            ev_at(
                14,
                contextgraph::TYPE_DECISION_MADE,
                serde_json::json!({ "id": "d_r2", "summary": "active-run decision", "governs": ["src/combat.rs"], "supersedes": "" }),
            ),
            ev_at(
                15,
                ledger::TYPE_UNIT_STATUS,
                serde_json::json!({ "id": "u_r2", "status": "verified" }),
            ),
            ev_at(
                16,
                ledger::TYPE_UNIT_STATUS,
                serde_json::json!({ "id": "u_r2", "status": "reviewed" }),
            ),
            ev_at(
                17,
                ledger::TYPE_UNIT_INTEGRATED,
                serde_json::json!({ "id": "u_r2", "commit": "abc2" }),
            ),
        ];

        // ===================================================================================
        // CONSUMER 1 - metrics::project folds the EVENT LOG, machinery events and all.
        // ===================================================================================
        // The de-noise stopped projecting `GateVerdict` / `UnitStarted` / `UnitIntegrated` as graph
        // nodes, but metrics reads those events from the log - so it still tallies two started
        // units, two clean first passes, both gates, and two review approvals. Asserting the
        // headline fields (not the whole struct) keeps the pin focused on the de-noised event types
        // without coupling to the unrelated review-quality fold.
        let m = metrics::project(&stream);
        assert_eq!(
            m.units_started, 2,
            "both UnitStarted events fold from the log"
        );
        assert_eq!(
            m.first_pass_clean, 2,
            "both units integrated with no failure - metrics reads the lifecycle from the log, not the graph"
        );
        assert_eq!(
            m.gates.get("build").map(|g| (g.pass, g.fail)),
            Some((1, 0)),
            "the build GateVerdict is still folded from the log though it is no longer a graph node"
        );
        assert_eq!(
            m.gates.get("clippy").map(|g| (g.pass, g.fail)),
            Some((1, 0)),
            "the clippy GateVerdict is still folded from the log"
        );
        assert_eq!(m.units_escalated, 0, "no unit escalated");
        assert_eq!(
            m.review_approve, 2,
            "both `reviewed` statuses count as approvals"
        );
        assert_eq!(m.review_reject, 0, "no review rejected");

        // ===================================================================================
        // CONSUMER 2 - run pruning derives its drop set from the EVENT LOG.
        // ===================================================================================
        // `superseded_graph_nodes` reuses `run::run_attribution`, which attributes ONLY
        // decision / finding / lesson events - so the machinery events (a `UnitStarted` carrying an
        // `id`, a `FileTouched`, a `GateVerdict`, a `UnitIntegrated`) contribute NOTHING to the drop
        // set, and it is exactly the dead run's decision and finding.
        let drop = superseded_graph_nodes(&stream);
        assert_eq!(
            drop,
            vec!["d_r1", "f_r1"],
            "the drop set is precisely the dead run's content - no machinery id (u_r1, u_r2, build, clippy, src/combat.rs) leaks in"
        );

        // The derivation is UNAFFECTED by the machinery events' presence: stripping every
        // FileTouched / UnitStarted / GateVerdict / UnitStatus / UnitIntegrated from the stream (the
        // events the de-noise stopped projecting) leaves the drop set byte-identical. This is the
        // "same result before and after the de-noise" guarantee for the pruning consumer.
        let content_only: Vec<Event> = stream
            .iter()
            .filter(|e| {
                e.type_ == runscope::TYPE_RUN_STARTED
                    || e.type_ == contextgraph::TYPE_DECISION_MADE
                    || e.type_ == contextgraph::TYPE_REVIEW_FINDING
                    || e.type_ == contextgraph::TYPE_LESSON_LEARNED
            })
            .cloned()
            .collect();
        assert_eq!(
            superseded_graph_nodes(&content_only),
            drop,
            "removing the machinery events does not change the pruning drop set - it reads only the run windows and the content ids"
        );

        // End-to-end: fold the whole run into the DE-NOISED graph (which projects no machinery
        // node), then run the real prune. It still drops exactly the dead-run content and keeps the
        // active run's decision plus the code it governs.
        let graph = Projector::open(":memory:", "test").unwrap();
        for e in &stream {
            crate::test_support::folds(&graph, std::slice::from_ref(e));
        }
        let boundary = superseded_edge_boundary(&stream);
        graph.prune(&drop, boundary).unwrap();

        let g = graph
            .subgraph(
                &[
                    "d_r1".to_string(),
                    "f_r1".to_string(),
                    "d_r2".to_string(),
                    "src/combat.rs".to_string(),
                ],
                2,
            )
            .unwrap();
        assert!(
            g.nodes
                .iter()
                .any(|n| n.id == "d_r2" && n.kind == contextgraph::KIND_DECISION),
            "the active run's decision survives the prune; got {:?}",
            g.nodes.iter().map(|n| (&n.id, &n.kind)).collect::<Vec<_>>()
        );
        assert!(
            g.edges.iter().any(|e| e.rel == contextgraph::REL_GOVERNS
                && e.from == "d_r2"
                && e.to == "src/combat.rs"),
            "and its GOVERNS edge to the code it concerns survives"
        );
        assert!(
            !g.nodes.iter().any(|n| n.id == "d_r1"),
            "the dead run's decision is pruned"
        );
        assert!(
            !g.nodes.iter().any(|n| n.id == "f_r1"),
            "the dead run's finding is pruned"
        );

        // ===================================================================================
        // CONSUMER 3 - blast radius grounds over the CODE cross-reference, never the graph.
        // ===================================================================================
        // A blast radius is a function of the source tree (via the `Grep` default here, or the
        // `symbols` grounder's cross-reference index), never of the context graph - so removing
        // machinery graph nodes cannot move it. The radius of `apply_damage` covers the file that
        // defines it and the file that references it, and excludes an unrelated file.
        let repo = tempfile::tempdir().unwrap();
        std::fs::write(
            repo.path().join("combat.rs"),
            "pub fn apply_damage(target: &mut Enemy) { target.hp -= 1; }\n",
        )
        .unwrap();
        std::fs::write(
            repo.path().join("enemy.rs"),
            "fn hit(e: &mut Enemy) { apply_damage(e); }\n",
        )
        .unwrap();
        std::fs::write(repo.path().join("audio.rs"), "pub fn play_sound() {}\n").unwrap();
        let grep = Grep {
            root: repo.path().to_string_lossy().into_owned(),
        };
        let br = grep.blast_radius("apply_damage", 8);
        assert!(
            br.safe.iter().any(|f| f == "combat.rs"),
            "the safe radius covers the file that DEFINES apply_damage; got {:?}",
            br.safe
        );
        assert!(
            br.safe.iter().any(|f| f == "enemy.rs"),
            "and the file that REFERENCES it; got {:?}",
            br.safe
        );
        assert!(
            !br.safe.iter().any(|f| f == "audio.rs"),
            "an unrelated file is not in the radius; got {:?}",
            br.safe
        );
        assert!(
            !br.serialize,
            "apply_damage is not a hub, so the radius does not serialize"
        );
    }

    // --- Spec 107, criterion 16: what `rigger reset --derived` SAYS about the reclamation ---

    /// The lines [`reclamation_lines`] renders for a hand-built reclamation in the given state.
    ///
    /// `compaction_ran` and `on_disk_measured` are parameters of their own rather than inferred
    /// from `reclaimed`, as the lines read them rather than inferring them: a file that was not
    /// rewritten and a rewrite that reclaimed nothing are both `Some(0)`, and a rewrite a reader
    /// declined to let land and one over a database with no file are both a rewritten file with
    /// no number.
    fn lines_of(
        compaction_ran: bool,
        reclaimed: Option<u64>,
        on_disk_measured: bool,
        failure: Option<&str>,
    ) -> Vec<String> {
        reclamation_lines(&rigger::eventstore::sqlite::Reclamation {
            reclaimed_bytes: reclaimed,
            compaction_ran,
            on_disk_measured,
            compaction_error: failure.map(str::to_string),
        })
    }

    /// A compaction that failed AFTER the migration committed is reported, not swallowed into an
    /// error: the failure by name on one line, and on the next the two things that follow from
    /// the ordering - what the run changed is durable and a re-run is safe and retries.
    #[test]
    fn a_compaction_that_failed_after_the_deletes_is_reported_beside_the_counts() {
        assert_eq!(
            lines_of(true, None, true, Some("database or disk is full")),
            [
                "reset --derived: the log file could NOT be compacted: database or disk is full",
                "reset --derived: what this run changed is committed and durable, so re-running \
                 the command is safe - and it retries the reclamation, because the space this \
                 run could not reclaim is still free in the file",
            ]
        );
    }

    /// A file holding no free page is said to have been left as it stands: zero bytes is the
    /// measurement there, never a rewrite that reclaimed nothing.
    #[test]
    fn a_prune_that_shed_nothing_is_justified_by_this_log_not_by_when_it_was_written() {
        assert_eq!(
            lines_of(false, Some(0), true, None),
            ["reset --derived: the log file holds no reclaimable free page, so it was left as it \
              stands rather than rewritten to reclaim nothing"]
        );
    }

    /// "The file was left alone" is a statement about THE REWRITE, not about the row count: a
    /// pass that rewrote the file reports the bytes the log lost on disk, and never says the
    /// file was left as it stands.
    #[test]
    fn a_pass_that_deleted_nothing_but_reclaimed_space_reports_the_reclamation() {
        assert_eq!(
            lines_of(true, Some(8192), true, None),
            ["reset --derived: compacted the log file and reclaimed 8192 byte(s) on disk"]
        );
    }

    /// An unmeasured reclamation has TWO causes, and the lines name only the one they were told
    /// about: a checkpoint a concurrent reader declined, whose pages land later, or a database
    /// with no file behind it, where there were never bytes on disk to measure.
    #[test]
    fn an_unmeasurable_database_is_not_reported_as_a_checkpoint_a_reader_declined() {
        assert_eq!(
            (
                lines_of(true, None, false, None),
                lines_of(true, None, true, None)
            ),
            (
                vec![
                    "reset --derived: compacted the log, which has no file behind it (an \
                     in-memory or temporary database): there are no bytes on disk to have been \
                     reclaimed, so the reclamation is unmeasured rather than zero"
                        .to_string()
                ],
                vec![
                    "reset --derived: compacted the log file, but the freed pages could not be \
                     folded back into it: a concurrent reader held the write-ahead log, so they \
                     land at the next checkpoint and this run reclaimed an unmeasured amount"
                        .to_string()
                ],
            )
        );
    }

    /// Given a store whose run stream holds a derived row naming a regular file of the tree, when
    /// the migration is handed a hash function that fails, then it fails with that function's own
    /// error, having handed it the tree's root and the file's bytes, before any row changes.
    #[test]
    fn reset_derived_handed_a_failing_hash_function_fails_before_any_row_changes() {
        let dir = tempfile::tempdir().unwrap();
        crate::test_support::git_init_quiet(dir.path());
        let rigger_dir = dir.path().join(RIGGER_DIR);
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::create_dir_all(&rigger_dir).unwrap();
        std::fs::write(dir.path().join("src/a.rs"), "fn a() {}\n").unwrap();
        let loc = StoreLocation {
            dir: rigger_dir.clone(),
        };
        let db = loc.file("events.db");
        let rows = || {
            let backend = rigger::eventstore::sqlite::Store::open(&db).unwrap();
            Namespaced::new(&backend, &loc.identity())
                .read_stream(conductor::STREAM, 0, Direction::Forward)
                .unwrap()
                .into_iter()
                .map(|event| (event.position, event.type_, event.data, event.meta))
                .collect::<Vec<_>>()
        };
        {
            let backend = rigger::eventstore::sqlite::Store::open(&db).unwrap();
            Namespaced::new(&backend, &loc.identity())
                .append(
                    conductor::STREAM,
                    ExpectedRevision::Any,
                    &[
                        Event::new(contextgraph::TYPE_CODE_ENTITY_EXTRACTED, b"{}".to_vec())
                            .with_meta(rigger::ingest::META_REPLAY_KEY, "gc/src/a.rs@h1#0"),
                    ],
                )
                .unwrap();
        }
        let before = rows();
        let asked = std::cell::RefCell::new(Vec::new());

        let failed = reset_derived(&loc, &|root, bytes| {
            asked
                .borrow_mut()
                .push((root.canonicalize().unwrap(), bytes.to_vec()));
            Err("no hash today".into())
        })
        .expect_err("a failed hash fails the migration");

        assert_eq!(
            (
                failed.to_string(),
                asked.into_inner(),
                rows() == before,
                before.len(),
            ),
            (
                "no hash today".to_string(),
                vec![(dir.path().canonicalize().unwrap(), b"fn a() {}\n".to_vec())],
                true,
                1,
            )
        );
    }

    // --- Spec 71, criterion 2: COMPACTION REFUSES LIVE WRITERS (spec 101: it reads liveness) ---

    /// A live spawn `id`, its marker touched `ago` seconds back, bounded at `bound` seconds.
    fn live_spawn(id: &str, ago: u64, bound: u64) -> rigger::liveness::LiveSpawn {
        rigger::liveness::LiveSpawn {
            id: id.to_string(),
            silent_for: std::time::Duration::from_secs(ago),
            bound: std::time::Duration::from_secs(bound),
        }
    }

    const STEP_LOCK_REASON: &str =
        "a `rigger step` is running right now (it holds .rigger/step.lock)";

    /// The pure core: all three facts quiet is the only quiet state, and each fact alone draws
    /// exactly its own reason.
    #[test]
    fn live_writer_reasons_is_empty_only_when_all_three_facts_are_quiet() {
        assert_eq!(live_writer_reasons(false, &[], 0), Vec::<String>::new());
        assert_eq!(live_writer_reasons(true, &[], 0), vec![STEP_LOCK_REASON]);
        assert_eq!(
            live_writer_reasons(false, &[live_spawn("a/implementer#0", 12, 300)], 0).len(),
            1
        );
        assert_eq!(live_writer_reasons(false, &[], 1).len(), 1);
    }

    /// A held step lock is named by exactly what it is, and the refusal points at the override -
    /// with a phrase that OWNS the risk, not merely the bare flag token (spec 71: "an explicit
    /// override flag whose help text owns the risk").
    #[test]
    fn refusal_names_a_held_step_lock_and_the_force_live_override_owning_the_risk() {
        let reasons = live_writer_reasons(true, &[], 0);
        let out = live_writer_refusal(&reasons);
        assert!(
            out.contains("step.lock"),
            "must name the held lock; got {out:?}"
        );
        assert!(
            out.contains("--force-live"),
            "must name the override; got {out:?}"
        );
        assert!(
            out.contains("reset --derived"),
            "must name the refused command; got {out:?}"
        );
        assert!(
            out.contains("corruption"),
            "must OWN the risk, not just name the flag; got {out:?}"
        );
    }

    /// Every live spawn is named by id with its marker's age and the bound keeping it live, in
    /// the order given, and the count is stated.
    #[test]
    fn refusal_names_every_live_spawn_with_its_marker_age_and_bound() {
        let spawns = [
            live_spawn("a/implementer#0", 12, 300),
            live_spawn("b/reviewer#1", 299, 3600),
        ];
        assert_eq!(
            live_writer_reasons(false, &spawns, 0),
            vec![
                "2 in-flight spawn(s) in the current run touched their liveness marker within \
                 their wall-clock bound: a/implementer#0 (marker touched 12s ago, bound 300s), \
                 b/reviewer#1 (marker touched 299s ago, bound 3600s)"
            ]
        );
    }

    /// An unbounded spawn never goes stale, so its refusal names the one way it ends: recording
    /// its result.
    #[test]
    fn refusal_names_an_unbounded_live_spawn_and_the_result_that_ends_it() {
        assert_eq!(
            live_writer_reasons(false, &[live_spawn("a/implementer#0", 86_400, 0)], 0),
            vec![
                "1 in-flight spawn(s) in the current run touched their liveness marker within \
                 their wall-clock bound: a/implementer#0 (marker touched 86400s ago; unbounded, \
                 so it stays live until its result is recorded - `rigger result a/implementer#0 \
                 --error <why>` once its worker is gone)"
            ]
        );
    }

    /// Live driver registrations are named by their count, the mechanism (spec 50) they come from
    /// and the idle window their heartbeat is inside.
    #[test]
    fn refusal_names_the_driver_registration_count_and_the_idle_window() {
        assert_eq!(
            live_writer_reasons(false, &[], 3),
            vec![
                "3 driver registration(s) for this project's store in the machine-global \
                 instance registry (spec 50) heartbeat within the last 900s - a \
                 `run`/`serve`/`step` may be advancing this run elsewhere on this machine"
            ]
        );
    }

    /// ALL applicable reasons are named together, not just the first found - so an operator sees
    /// the whole picture in one refusal instead of clearing one and retrying into the next.
    #[test]
    fn refusal_names_every_applicable_reason_together_not_just_the_first() {
        let spawns = [live_spawn("b/reviewer#1", 5, 300)];
        let reasons = live_writer_reasons(true, &spawns, 1);
        assert_eq!(reasons.len(), 3, "one reason per live fact: {reasons:?}");
        assert_eq!(reasons[0], STEP_LOCK_REASON);
        assert!(reasons[1].contains("b/reviewer#1"), "{reasons:?}");
        assert!(
            reasons[2].contains("1 driver registration(s)"),
            "{reasons:?}"
        );
    }

    /// The one liveness definition decides `reset --runs`'s dead-driver test too: dead exactly
    /// when no fact holds, live on any one of them.
    #[test]
    fn a_driver_is_dead_exactly_when_no_liveness_fact_holds() {
        let facts = |step_lock_held, live_spawns: Vec<_>, driver_registrations| LiveWriterFacts {
            step_lock_held,
            live_spawns,
            driver_registrations,
        };
        assert!(facts(false, vec![], 0).driver_dead());
        assert!(!facts(true, vec![], 0).driver_dead());
        assert!(!facts(false, vec![live_spawn("a/implementer#0", 1, 300)], 0).driver_dead());
        assert!(!facts(false, vec![], 1).driver_dead());
    }

    // --- Driver liveness: only a driver's registration keeps the driver alive ---

    /// Quiet [`LiveWriterFacts`] but for the driver registrations counted, as of `now_ms`, from a
    /// registry holding one entry for this store written by `writer` at `heartbeat_ms`.
    fn facts_over_one_registration(
        writer: Writer,
        heartbeat_ms: u64,
        now_ms: u64,
    ) -> LiveWriterFacts {
        let state = tempfile::tempdir().unwrap();
        let dir = rigger::registry::instances_dir(state.path());
        let store = StoreIdentity::Local {
            path: "/home/dev/proj/.rigger/events.db".to_string(),
        };
        let entry = Instance {
            project: "proj".to_string(),
            root: "/home/dev/proj".to_string(),
            store: store.clone(),
            heartbeat_ms,
            writer,
        };
        rigger::registry::write(&dir, &entry).unwrap();
        LiveWriterFacts {
            step_lock_held: false,
            live_spawns: Vec::new(),
            driver_registrations: live_driver_registrations(&dir, &store, now_ms),
        }
    }

    /// The run's driver reads dead exactly when `dead` over one registration by `writer` at
    /// `heartbeat_ms`, judged at `now_ms` (`why`).
    fn assert_driver_dead(writer: Writer, heartbeat_ms: u64, now_ms: u64, dead: bool, why: &str) {
        let facts = facts_over_one_registration(writer, heartbeat_ms, now_ms);
        assert_eq!(facts.driver_dead(), dead, "{why}");
    }

    rigger::test_cases! {
        /// A courier's discovery refresh drives nothing: seconds old, it leaves the driver dead.
        a_courier_only_registration_leaves_the_driver_dead: assert_driver_dead(
            Writer::Courier { driver_heartbeat_ms: None },
            1_000,
            1_500,
            true,
            "a fresh courier-only entry is no live driver",
        );
        a_fresh_driver_registration_keeps_the_driver_alive: assert_driver_dead(
            Writer::Driver,
            1_000,
            1_500,
            false,
            "a fresh driver entry is a live driver",
        );
        a_stale_driver_registration_leaves_the_driver_dead: assert_driver_dead(
            Writer::Driver,
            0,
            DEFAULT_IDLE_MS + 1,
            true,
            "a driver entry past the idle window is no live driver",
        );
        /// A courier run by a live driver's own agent re-stamps the entry but carries the
        /// driver's stamp, so the driver stays alive.
        a_courier_carrying_a_fresh_driver_stamp_keeps_the_driver_alive: assert_driver_dead(
            Writer::Courier { driver_heartbeat_ms: Some(1_000) },
            1_200,
            1_500,
            false,
            "a courier entry carrying a fresh driver stamp is a live driver",
        );
        /// The recorded shape: a dead driver's stamp stays dead under a fresh courier re-stamp.
        a_courier_carrying_a_stale_driver_stamp_leaves_the_driver_dead: assert_driver_dead(
            Writer::Courier { driver_heartbeat_ms: Some(0) },
            DEFAULT_IDLE_MS + 1,
            DEFAULT_IDLE_MS + 1,
            true,
            "a fresh courier entry carrying a stale driver stamp is no live driver",
        );
    }

    /// The step lock the live-writer probe takes is held for as long as the probe lives - through
    /// the close and the compaction its caller runs under it - so a `rigger step` started
    /// meanwhile refuses with the busy token its courier retries on, and never runs against the
    /// prune; once the probe drops, the next step acquires the lock.
    #[test]
    fn the_live_writer_probe_holds_the_step_lock_until_it_drops() {
        let dir = tempfile::tempdir().unwrap();
        let rigger_dir = dir.path().join(RIGGER_DIR);
        std::fs::create_dir_all(&rigger_dir).unwrap();
        let loc = StoreLocation { dir: rigger_dir };
        let env = ResetEnv {
            workdir: String::new(),
            scratch_root: dir.path().join("scratch").to_string_lossy().into_owned(),
            registry_dir: None,
            now_ms: rigger::registry::now_ms(),
        };

        let probe = live_writer_facts(&loc, &StoreSelection::Sqlite, &env, &[]).unwrap();
        let racing = acquire_step_lock(&loc.dir).map(drop);
        drop(probe);
        let err = racing.expect_err("a step started under the probe must refuse");
        assert!(
            err.to_string().contains(STEP_BUSY_TOKEN),
            "the racing step's refusal carries the busy token its courier retries on: {err}"
        );
        acquire_step_lock(&loc.dir).expect("the next step acquires once the probe drops");
    }

    /// A malformed event in the current run's slice (the `Err(_)` sentinel of the in-flight-spawn
    /// read) makes the guard REFUSE rather than silently read as quiet - the fail-safe direction
    /// spec 71 requires: an unreadable signal is never treated as "nobody is here".
    #[test]
    fn refuse_derived_reset_if_live_fails_safe_on_a_malformed_spawn_event() {
        let dir = tempfile::tempdir().unwrap();
        let rigger_dir = dir.path().join(RIGGER_DIR);
        std::fs::create_dir_all(&rigger_dir).unwrap();
        let loc = StoreLocation {
            dir: rigger_dir.clone(),
        };
        let identity = loc.identity();
        let db = rigger_dir.join("events.db").to_string_lossy().into_owned();
        let backend = rigger::eventstore::sqlite::Store::open(&db).unwrap();
        let store = Namespaced::new(&backend, &identity);
        // A malformed SpawnRequested body: valid JSON but missing the fields `spawn::recorded`
        // needs, so decoding it fails and `spawn::recorded` returns `Err`.
        store
            .append(
                conductor::STREAM,
                ExpectedRevision::Any,
                &[Event::new(spawn::TYPE_SPAWN_REQUESTED, b"{}".to_vec())],
            )
            .unwrap();
        drop(store);
        drop(backend);

        let env = ResetEnv {
            workdir: String::new(),
            scratch_root: dir.path().join("scratch").to_string_lossy().into_owned(),
            registry_dir: None,
            now_ms: rigger::registry::now_ms(),
        };
        let err = refuse_derived_reset_if_live(&loc, &StoreSelection::Sqlite, &env)
            .expect_err("a malformed spawn event must refuse, never read as quiet");
        assert!(
            !err.to_string().is_empty(),
            "the refusal must carry a message an operator can act on"
        );
    }

    /// A store whose owning root has no UTF-8 path resolves no scratch root, and `reset` refuses
    /// rather than reading that as "no spawn marker, so nothing is live" (the guard may only
    /// refuse) or reclaiming a build cache under a root it never resolved.
    #[cfg(unix)]
    #[test]
    fn reset_env_refuses_a_store_whose_owning_root_has_no_utf8_path() {
        use std::os::unix::ffi::OsStrExt;
        let dir =
            Path::new(std::ffi::OsStr::from_bytes(b"/nonexistent-\xff-root")).join(RIGGER_DIR);
        let loc = StoreLocation { dir: dir.clone() };
        let Err(err) = ResetEnv::resolve(&loc) else {
            panic!("an owning root with no UTF-8 path must refuse, never resolve a scratch root");
        };
        assert_eq!(
            err.to_string(),
            format!(
                "reset: the store at {} has no UTF-8 owning root, so the scratch root its runs \
                 write their liveness markers and build caches under cannot be resolved; \
                 refusing rather than reading that as no live spawn",
                dir.display()
            )
        );
    }

    /// The reset modes `args` parse to (`what` names the accepted form).
    fn reset_modes_of(args: &[&str], what: &str) -> ResetModes {
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        reset_modes(&args).unwrap_or_else(|e| panic!("{what}: {e}"))
    }

    /// The refusal `args` meet (`what` names why they must be refused).
    fn reset_refusal(args: &[&str], what: &str) -> String {
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        match reset_modes(&args) {
            Err(e) => e.to_string(),
            Ok(_) => panic!("{what}"),
        }
    }

    /// `reset_modes` accepts `--force-live` alongside `--derived`, at most once, and it never
    /// implies a mode on its own - a bare `--force-live` still falls through the existing "at
    /// least one mode" refusal exactly as before this flag existed.
    #[test]
    fn reset_modes_parses_force_live_alongside_derived_rejects_duplicates_and_never_implies_a_mode()
    {
        let modes = reset_modes_of(
            &["--derived", "--force-live"],
            "--derived --force-live must parse",
        );
        assert!(modes.derived && modes.force_live && !modes.runs);

        let err = reset_refusal(
            &["--force-live", "--force-live"],
            "a duplicate --force-live must be refused",
        );
        assert!(err.contains("more than once"), "got {err}");

        let err = reset_refusal(&["--force-live"], "--force-live alone names no mode");
        assert!(
            err.contains("at least one mode"),
            "a bare --force-live must fall through the same 'at least one mode' refusal as a \
             bare reset; got {err}"
        );
    }

    /// Exactly the selections with a mode that reads [`ResetEnv`] resolve it: `--runs` and
    /// `--build-cache` always, `--derived` unless `--force-live` skips its live-writer guard, and
    /// never `--scratch-orphans`, alone or beside a `--force-live` that has nothing to skip.
    #[test]
    fn reset_modes_read_env_exactly_for_runs_build_cache_and_a_guarded_derived() {
        let reads = |args: &[&str]| reset_modes_of(args, &args.join(" ")).reads_env();
        assert_eq!(
            [
                reads(&["--runs"]),
                reads(&["--build-cache"]),
                reads(&["--derived"]),
                reads(&["--runs", "--force-live"]),
                reads(&["--scratch-orphans", "--derived"]),
                reads(&["--build-cache", "--derived", "--force-live"]),
                reads(&["--derived", "--force-live"]),
                reads(&["--scratch-orphans"]),
                reads(&["--scratch-orphans", "--force-live"]),
            ],
            [true, true, true, true, true, true, false, false, false]
        );
    }

    /// spec 77 criterion 5 (BOUNDED SHARED CACHE): `--build-cache` is a mode exactly like
    /// `--runs`/`--derived` - parses alone, composes with either sibling, is rejected on
    /// a duplicate, and (matching every other mode) never implied on its own from a bare
    /// `reset` with no flags at all.
    #[test]
    fn reset_modes_parses_scratch_orphans_alone_and_composed_and_rejects_duplicates() {
        let modes = reset_modes(&["--scratch-orphans".to_string()]).expect("alone");
        assert!(modes.scratch_orphans && !modes.runs && !modes.derived && !modes.build_cache);
        let modes = reset_modes(&["--build-cache".to_string(), "--scratch-orphans".to_string()])
            .expect("composed with another mode");
        assert!(modes.scratch_orphans && modes.build_cache);
        let err = reset_modes(&[
            "--scratch-orphans".to_string(),
            "--scratch-orphans".to_string(),
        ])
        .err()
        .expect("a duplicate is refused")
        .to_string();
        assert!(err.contains("more than once"), "{err}");
        let err = reset_modes(&["--bogus".to_string()])
            .err()
            .expect("unknown")
            .to_string();
        assert!(
            err.contains("--scratch-orphans"),
            "the usage names the new mode: {err}"
        );
    }

    #[test]
    fn reset_modes_parses_build_cache_alone_and_composed_and_rejects_duplicates() {
        let modes = reset_modes_of(&["--build-cache"], "--build-cache alone");
        assert!(modes.build_cache && !modes.runs && !modes.derived);

        let modes = reset_modes_of(
            &["--runs", "--build-cache"],
            "--runs --build-cache must compose",
        );
        assert!(modes.runs && modes.build_cache && !modes.derived);

        let modes = reset_modes_of(
            &["--derived", "--build-cache"],
            "--derived --build-cache must compose",
        );
        assert!(modes.derived && modes.build_cache);

        let err = reset_refusal(
            &["--build-cache", "--build-cache"],
            "a duplicate --build-cache must be refused",
        );
        assert!(err.contains("more than once"), "got {err}");
    }

    // --- Spec 68, criterion 3: the bare-menu report lines (pure, no store, no live server) ---

    #[test]
    fn runs_menu_line_names_the_measured_counts_and_the_flag() {
        let line = runs_menu_line(&PruneStats {
            nodes: 4,
            superseded_edges: 2,
        });
        assert!(
            line.contains("--runs:"),
            "must name its own flag; got {line:?}"
        );
        assert!(
            line.contains("4 dead-run node(s)") && line.contains("2 superseded edge(s)"),
            "must name the measured counts; got {line:?}"
        );
        assert!(
            line.contains("--runs"),
            "must tell the operator which flag reclaims it; got {line:?}"
        );

        let zero = runs_menu_line(&PruneStats::default());
        assert!(
            zero.contains("0 dead-run node(s)") && zero.contains("0 superseded edge(s)"),
            "an empty store must report zero, not omit the line; got {zero:?}"
        );
    }

    /// A count of `events` derived events, `unkeyed` of them naming no file identity, held by
    /// `identities`.
    fn derived_count(events: usize, unkeyed: usize, identities: &[&str]) -> DerivedCount {
        DerivedCount {
            shed: events,
            unkeyed,
            identities: identities.iter().map(|i| i.to_string()).collect(),
        }
    }

    #[test]
    fn derived_count_phrase_names_the_events_first_and_the_file_identities_second() {
        assert_eq!(
            derived_count_phrase(7, 2),
            "7 derived events of 2 file identities"
        );
        assert_eq!(
            derived_count_phrase(1, 0),
            "1 derived events of 0 file identities"
        );
    }

    /// A count holding no derived event has no phrase; any other is phrased by its events and
    /// the size of its identity set, decided by the events alone, so rows that all name no
    /// identity are still held.
    #[test]
    fn derived_events_held_phrases_a_count_holding_a_derived_event_and_answers_none_for_zero() {
        assert_eq!(derived_events_held(&DerivedCount::default()), None);
        assert_eq!(
            derived_events_held(&derived_count(0, 0, &["gc/src/a.rs"])),
            None,
            "the events decide, never the identities"
        );
        assert_eq!(
            derived_events_held(&derived_count(1, 0, &["gc/src/a.rs"])).as_deref(),
            Some("1 derived events of 1 file identities")
        );
        assert_eq!(
            derived_events_held(&derived_count(7, 1, &["gc/src/a.rs", "gd/src/a.rs"])).as_deref(),
            Some("7 derived events of 2 file identities")
        );
        assert_eq!(
            derived_events_held(&derived_count(2, 2, &[])).as_deref(),
            Some("2 derived events of 0 file identities")
        );
    }

    #[test]
    fn derived_menu_line_names_the_events_the_file_identities_holding_them_and_the_flag() {
        assert_eq!(
            derived_menu_line(
                &StoreSelection::Sqlite,
                Some(&derived_count(7, 1, &["gc/src/a.rs", "gd/src/a.rs"]))
            ),
            "--derived: 7 derived events of 2 file identities to shed from the event log; rerun \
             `rigger reset --derived` to migrate them",
            "the events are every derived event counted, the unkeyed one among them, and the \
             identities the size of the set"
        );
    }

    /// A store whose derived rows all name no file identity still holds events `--derived`
    /// sheds: the line is decided by the events, never by the identities.
    #[test]
    fn derived_menu_line_names_the_events_of_a_store_whose_rows_are_all_unkeyed() {
        assert_eq!(
            derived_menu_line(&StoreSelection::Sqlite, Some(&derived_count(2, 2, &[]))),
            "--derived: 2 derived events of 0 file identities to shed from the event log; rerun \
             `rigger reset --derived` to migrate them"
        );
    }

    #[test]
    fn derived_menu_line_says_a_store_holding_no_derived_event_has_none_to_shed() {
        assert_eq!(
            derived_menu_line(&StoreSelection::Sqlite, Some(&DerivedCount::default())),
            "--derived: no derived event to shed"
        );
    }

    /// The per-backend honesty branch (spec 68 Design: "a backend where a prune is unavailable
    /// says so on that line"). `StoreSelection` is private to this module, so this is the ONE
    /// place able to construct `Server(..)` directly and prove the wording without a live
    /// server - `derived_menu_line` never opens a connection either way.
    #[test]
    fn derived_menu_line_on_a_server_backend_says_the_migration_does_not_run_there() {
        let server = StoreSelection::Server("esdb://127.0.0.1:2113?tls=false".to_string());
        assert_eq!(
            derived_menu_line(&server, None),
            "--derived: unavailable on this backend - the migration rewrites and deletes rows of \
             the event log and vacuums the file, a mechanic of the embedded sqlite events.db \
             store; this project is configured for the server-backed store, where the migration \
             does not run"
        );
    }
}
