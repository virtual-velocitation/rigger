use super::*;

/// Whether the tracked `.rigger/project.id` is present (and non-blank) for the project at
/// `root`, resolved relative to the git top-level (else `root`) - the same anchoring
/// [`project_identity_at`] uses. `false` means identity falls back to the volatile basename,
/// which `rigger validate` surfaces as a rename-orphans-history hazard.
fn has_tracked_project_id(root: &Path) -> bool {
    let toplevel = git_repo_at(root);
    let base: &Path = if toplevel.is_empty() {
        root
    } else {
        Path::new(&toplevel)
    };
    read_project_id(base).is_some()
}

/// The `rigger validate` GRAPH INDEX LAG sample (spec 107, THE LEDGER ANSWERS THE INDEX-LAG
/// ADVISORY): reads the log's side from `store` in ONE typed read of the perception types
/// ([`rigger::ingest::perceived_generations`]), never the whole stream, and hands it to
/// [`rigger::ingest::graph_index_lag_sample`], the one authority that draws the bounded candidate
/// list from it and compares each candidate's current bytes, read under `root`, against its
/// latest recording and against `graph`'s current generation. With no `graph` the log's side
/// alone is compared.
fn read_graph_index_lag(
    store: &dyn EventStore,
    graph: Option<&dyn contextgraph::Projection>,
    root: &Path,
) -> Result<Vec<String>, rigger::eventstore::Error> {
    let latest = rigger::ingest::perceived_generations(store, conductor::STREAM)?;
    Ok(rigger::ingest::graph_index_lag_sample(root, &latest, graph))
}

/// The graph the index-lag advisory compares against: the project's `graph.db` when it stands
/// ([`standing_graph`]) and owes no rebuild. An owed graph is not asked, nor one whose debt
/// cannot be read: the advisory then compares the log's side alone.
fn graph_to_compare(graph_db: &str, project: &str) -> Option<Projector> {
    let graph = standing_graph(graph_db, project)?;
    let owed = graph.rebuild_owed().ok()?;
    (!owed).then_some(graph)
}

/// The `rigger validate` model-drift advisory (spec 13b, unit 1): a stderr warning naming
/// each tier whose resolved model id re-pointed since the previous run and recommending the
/// drift-gated canary, or `None` when nothing drifted. Pure over the [`metrics::ModelDrift`]
/// so it is asserted without touching the filesystem (like [`format_stats`]); the caller
/// prints it without changing the exit status, exactly like the other validate advisories.
fn model_drift_advisory(drift: &metrics::ModelDrift) -> Option<String> {
    if !drift.changed() {
        return None;
    }
    // DRIFT SEVERITY (spec 61, c11): a drift where EVERY change is a same-base date-suffix
    // bump (the resolved id moved to a fresher snapshot, not a different model) is worded as
    // a low-urgency advisory rather than the mandate-style warning below - a real re-point
    // among the changes still gets the full warning, since one is enough to warrant it.
    if drift.snapshot_only() {
        let mut msg = String::from(
            "advisory: a tier's resolved model id moved to a newer snapshot since the previous \
             run (same model, a date-suffix bump):",
        );
        for c in &drift.changes {
            let alias = if c.alias.is_empty() {
                "(unnamed tier)"
            } else {
                c.alias.as_str()
            };
            msg.push_str(&format!("\n  - {alias}: {} -> {}", c.previous, c.current));
        }
        msg.push_str(
            "\nRun `rigger canary --if-model-changed` when convenient to re-measure - same \
             model, so there is no urgency.",
        );
        return Some(msg);
    }
    let mut msg = String::from(
        "warning: a tier's resolved model id changed since the previous run (a silent alias \
         re-point):",
    );
    for c in &drift.changes {
        let alias = if c.alias.is_empty() {
            "(unnamed tier)"
        } else {
            c.alias.as_str()
        };
        msg.push_str(&format!("\n  - {alias}: {} -> {}", c.previous, c.current));
    }
    msg.push_str(
        "\nRun `rigger canary --if-model-changed` to re-measure the review panel against the \
         seeded-defect corpus before trusting a run under the new model.",
    );
    Some(msg)
}

/// The `rigger validate` order-signature advisories (spec 71, VALIDATE DETECTS THE
/// SIGNATURE): one warning per affected stream naming the row count, the affected position
/// range, and [`watch::ORDER_SIGNATURE_REPAIR_DOC_REF`] - or an empty vec on a clean log. Pure
/// over already-detected signatures, mirroring [`model_drift_advisory`]'s split between
/// reading and formatting. Report-only like every validate advisory: printed to stderr, never
/// changing the exit status - repair stays a documented operator procedure, never a command
/// this binary performs (spec 71 Notes: fail-safe directions only).
fn order_signature_advisories(signatures: &[watch::OrderSignature]) -> Vec<String> {
    signatures
        .iter()
        .map(|s| {
            format!(
                "warning: stream {} has {} row(s) where position order and revision order \
                 disagree (positions {}..={}): a write likely landed in a revision hole a \
                 compaction opened. This is report-only - `rigger validate` never repairs it; \
                 see {} for the repair procedure.",
                s.stream,
                s.rows,
                s.first_position,
                s.last_position,
                watch::ORDER_SIGNATURE_REPAIR_DOC_REF
            )
        })
        .collect()
}

/// Read [`watch::OrderSignature`]s (spec 71) from the embedded `events.db` at `path`,
/// namespaced by `project`, scanning the FULL log in position order (mirrors
/// [`cmd_prime`]'s full-log read - the store defends its own order for every stream, not one
/// distinguished stream, so there is no narrower slice to scan). Returns an empty vec when
/// there is no store yet, like [`read_model_drift`]. Detection itself
/// ([`watch::order_signatures`]) is the SAME shared algorithm `rigger watch`'s own store-
/// integrity signal calls - one implementation, not two kept in sync by hand.
fn read_order_signatures(
    path: &str,
    project: &str,
) -> Result<Vec<watch::OrderSignature>, Box<dyn std::error::Error>> {
    let sel = store_selection(None, None)?;
    if sel.is_sqlite() && !Path::new(path).exists() {
        return Ok(Vec::new());
    }
    let backend = resolve_store(&sel, path)?;
    let store = Namespaced::new(backend.as_ref(), project);
    let events = store.read_all(0, Direction::Forward, &Filter::default())?;
    Ok(watch::order_signatures(&events))
}

pub(crate) fn cmd_validate(args: &[String]) -> Res {
    let root = Path::new(".");
    // Optional `<spec>` path (spec 18, Unit 4; spec 66, unit c3): emit heuristic spec-lint
    // advisories - shape (multi-behavior / sub-bullet-as-unit / over-long), ownership
    // (F1), open dispositions (F4), and hygiene (em dash) - each naming its criterion (when
    // tied to one) and field-guide class. These are ADVISORY - they never change the exit
    // status - so a badly-shaped or ownerless criterion is surfaced, not refused. Run
    // before config validation so a spec can be linted from a fresh checkout whose rigger
    // config is not yet valid; an unreadable spec path is still an input error (the lint is
    // heuristic, but "you named a spec that does not exist" is not). `spec_lint_advisories`
    // is the ONE combined lint surface (it internally reuses `spec_shape_advisories`),
    // never a second, parallel aggregation - and `spec_lint_warning_lines` above is the ONE
    // formatter this and the in-run call site (`load_criteria`, spec 66 criterion 4) share.
    if let Some(spec_path) = args.first() {
        let text = std::fs::read_to_string(spec_path)
            .map_err(|e| format!("read spec {spec_path}: {e}"))?;
        for line in spec_lint_warning_lines(spec_path, &text) {
            eprintln!("{line}");
        }
    }
    let config_store::LoadedConfig {
        config: cfg,
        gate_requirements,
    } = config_store::load_with_gate_requirements(".")?;
    // Static verdict-line lint (spec 18, unit 1): a gating adjudicator whose persona only
    // records its verdict via `rigger_emit` - never on its result output - is a guaranteed
    // stall, because the integration gate reads the result channel, not emitted events. This
    // is a HARD error (deterministic hang) that names the fix, so `rigger validate` refuses a
    // config that would silently ferment into an escalation loop.
    config::lint_gating_verdict_lines(&cfg)?;
    // Surface the running binary's version + build provenance (spec 18) so an agent driving
    // `rigger validate` can identify the exact binary - the same provenance the drift
    // advisory below uses to name which side is stale.
    println!("{}", version_line());
    println!(
        "config valid: {} agents, {} stages, {} gates",
        cfg.agents.len(),
        cfg.workflow.stages.len(),
        cfg.workflow.gates.len()
    );
    // NO UNGATED FAN-OUT TEMPLATE advisory (spec 103, criterion 2): warn when a fan-out
    // implement template declares no gates at all - the author-time half of the runtime
    // invariant `conductor::run` enforces once a spec actually decomposes against the
    // template (`conductor::assert_no_ungated_fanout_unit`). Non-fatal, like every other
    // advisory here: an author who deliberately wants an ungated fan-out stage still gets
    // one, just no longer by silent omission.
    for template in conductor::ungated_fan_out_templates(&cfg.workflow.stages) {
        eprintln!(
            "warning: fan-out template '{template}' declares no gates - every unit it \
             decomposes into will run ungated; add a `gates:` list to the template if this \
             is unintended"
        );
    }
    // Build-environment SURFACES report (spec 65 units 2 and 5, NO SILENT DEGRADE /
    // HONEST SURFACES): a named-but-absent `build.wrapper`, or a named wrapper whose cache
    // dir cannot be created, already failed above (`config::load`'s `Config::validate`
    // rejects both at run start, before `cfg` could exist), so by this point resolution
    // can only succeed - this SURFACES what it resolved to (wrapper, cache dir, budget) so
    // an `auto` probe that quietly found nothing (or found a wrapper whose cache dir turned
    // out unusable) is SEEN as "none" here rather than silently doing nothing invisibly.
    // Reads through the SAME `resolve_build_layer` authority `Config::validate` and the
    // conductor's build-environment authority use - never a second, independently
    // re-derived report; the formatting itself lives in the pure, unit-tested
    // `build_environment_report` below so this edge stays a thin resolve-then-print.
    let wrapper =
        match resolve_build_layer(&cfg.workflow.build.wrapper, &cfg.workflow.build.cache_dir) {
            Ok(w) => w,
            Err(e) => return Err(e.to_string().into()),
        };
    // Gate requirement SURFACE (spec 113): one line per declared gate, in gate-id order,
    // rendered from the resolution the load itself made - a missing requirement already
    // refused the load above, before any output, so every line names a resolved path and no
    // second lookup runs here.
    for line in build_environment_report(wrapper.as_deref(), &cfg.workflow.build)
        .into_iter()
        .chain(gate_requirement_lines(&gate_requirements))
    {
        println!("{line}");
    }
    // Non-fatal advisories (spec 05:55): surface config/install drift so it is seen,
    // not discovered by accident. Each is a stderr warning that never changes the exit
    // status - `rigger validate` still succeeds so long as the config itself is valid.
    for advisory in validate_advisories(root) {
        eprintln!("{advisory}");
    }
    // Unbounded wall-clock advisory (spec 19c, unit 3): warn when `defaults.max_wall_clock`
    // is unbounded and a gating role carries no per-agent bound, so a hung gating agent that
    // is never swept - a silent stall - is visible at author time. Non-fatal like the others;
    // reuses the single `config::gating_agent_ids` authority the verdict-line lint uses.
    if let Some(advisory) = config::unbounded_wall_clock_advisory(&cfg) {
        eprintln!("{advisory}");
    }
    // Residue surfacing (spec 06, unit 6 / Gap 14d): report leftover scratch worktrees,
    // orphaned build caches, shadow stores, and dead `rigger/u/*` branches - with sizes -
    // so residue is seen before a disk fills. Warnings only; validate NEVER fails or
    // deletes anything (cleanup stays with the step-start sweep).
    // A genuine store-SELECTION failure here (unreadable `.rigger/store.conn`, malformed
    // `workflow.yml`, invalid `store.backend`) SURFACES loudly - `?` fails validate - rather than
    // degrading to a wrong-store read that would misreport live worktrees/branches as residue
    // (d-u2rr-observer-selection-loud). The residue FINDINGS themselves stay warning-only below;
    // this only makes an inability to even resolve the run store loud, never silent.
    for advisory in residue_advisories(root, &cfg)? {
        eprintln!("{advisory}");
    }
    // Model-drift advisory (spec 13b, unit 1): warn when a tier's resolved model id
    // re-pointed since the previous run and recommend `rigger canary --if-model-changed`.
    // A store-read failure just skips the advisory (never fails validate), exactly like the
    // git-backed advisories above swallow a missing/erroring git.
    if let Ok(drift) = read_model_drift(&db_path("events.db"), &project_identity()) {
        if let Some(advisory) = model_drift_advisory(&drift) {
            eprintln!("{advisory}");
        }
    }
    // Order-signature advisory (spec 71, VALIDATE DETECTS THE SIGNATURE): warn when a
    // stream's position order and revision order disagree - the tail a write leaves when it
    // lands at a revision a compaction opened as a hole. Report-only, like every other
    // validate advisory: this NEVER repairs, reorders, or changes the exit status (fail-safe
    // direction only - the spec's repair stays a documented operator procedure, never a
    // command). A store-read failure just skips the advisory (never fails validate), exactly
    // like the model-drift advisory above.
    if let Ok(signatures) = read_order_signatures(&db_path("events.db"), &project_identity()) {
        for advisory in order_signature_advisories(&signatures) {
            eprintln!("{advisory}");
        }
    }
    // INDEX STALENESS advisory (spec 68, VALIDATE ADVISORIES): warn when the persisted
    // `symbols` grounding index has drifted from the tree and name `rigger reindex`. Cost-
    // bounded and ungated (Design) - see `grounder::symbols::staleness`'s own docs for the
    // measurement itself. `None` when there is no persisted index (nothing to compare
    // against) or no disagreement; this never fails validate.
    if let Some(drift) = rigger::grounder::symbols::staleness(root.to_str().unwrap_or(".")) {
        eprintln!("{}", index_staleness_message(&drift));
    }
    // GRAPH INDEX LAG advisory (spec 107, THE LEDGER ANSWERS THE INDEX-LAG ADVISORY): warn when
    // a bounded sample of files the log has recorded extracts, from the bytes the tree holds
    // now, to a generation the log's latest recording and `graph.db` do not both hold -
    // staleness the integration-time reindex is supposed to prevent, surfaced before it is felt
    // rather than discovered by a stale `graph --show` line. The tree is read under the ONE
    // ROOT, the top level of the repository holding the store's `.rigger/`, which the entries'
    // paths are relative to. An absent store has recorded nothing, and a store-read failure
    // just skips the advisory (never fails validate), exactly like the model-drift advisory
    // above.
    let project = project_identity();
    let graph = graph_to_compare(&db_path("graph.db"), &project);
    let lagging = store_selection(None, None).and_then(|sel| {
        with_project_store(&db_path("events.db"), &project, &sel, |store| {
            read_graph_index_lag(
                store,
                graph
                    .as_ref()
                    .map(|graph| graph as &dyn contextgraph::Projection),
                &tree_root(&cwd().join(RIGGER_DIR)),
            )
        })
    });
    if let Ok(Some(lagging)) = lagging {
        if let Some(advisory) = graph_index_lag_advisory(&lagging) {
            eprintln!("{advisory}");
        }
    }
    // LOG BLOAT advisory (spec 68, VALIDATE ADVISORIES): warn when the event log's derived
    // index is duplicated above threshold and name `rigger reset --derived`. Reuses the
    // store's OWN aggregate ([`rigger::eventstore::sqlite::Store::measure_derived_duplication`],
    // the same key/type/prefix authority the compaction itself uses - no shadow accounting).
    // `None` on a server-backed project (a sqlite-only mechanic, exactly like `reset --derived`
    // itself), on a project with no events.db yet, or on any read failure; this never fails
    // validate and never creates a store that does not already exist.
    if let Some(advisory) = bloat_advisory_for(&db_path("events.db"), &project_identity()) {
        eprintln!("{advisory}");
    }
    // FOOTPRINT ACCOUNTING (spec 77 criterion 6): rigger's total on-disk footprint by
    // category, ALWAYS printed (an honest surface, not conditional on anything crossing a
    // threshold), plus an advisory for any category whose dead share breaches the
    // threshold, naming the reclaiming command. A store-ACCESS-miss failure degrades to an
    // empty report (mirrors residue_advisories/read_run_units); only a genuine store-
    // SELECTION failure is worth surfacing, and it already will be by the residue block
    // above sharing the same read - so this stays a silent skip, never a second error path
    // for the identical cause.
    if let Ok((totals, advisories)) = footprint_report_for(&cfg) {
        for line in totals {
            println!("{line}");
        }
        for advisory in advisories {
            eprintln!("{advisory}");
        }
    }
    // RETIRED CODE-ENTITY advisory (spec 86 criterion 3, THE MIGRATION IS DELIBERATE): report,
    // once, how many code-entity nodes the graph's own supersession has retired since their last
    // extraction - what makes the test-exclusion migration's shrink VISIBLE to the operator
    // rather than a silent internal bookkeeping fact (`rigger::contextgraph::sqlite::Projector::
    // retired_code_entity_count` is the one counting authority; this never re-derives it). `None`
    // on every reason there is nothing honest to report (see `retired_entities_advisory_for`'s own
    // doc), so this never fails validate.
    if let Some(advisory) = retired_entities_advisory_for(&db_path("graph.db"), &project_identity())
    {
        eprintln!("{advisory}");
    }
    // REBUILD OWED advisory (spec 101): a `graph.db` that owes its rebuild - folded under an older
    // fold rule, or marked by a fold into it that failed - answers as it stands until `rigger setup` rebuilds it;
    // validate says so and writes nothing to it.
    if let Some(note) = graph_rebuild_owed_note(&db_path("graph.db"), &project_identity()) {
        eprintln!("{note}");
    }
    // Docs-drift GATE (spec 20, unit 2): the committed `using-rigger` skill and handbook
    // discipline chapter are generated by `rigger docs` from the same code facts this binary
    // runs on. When a source fact or a template changes, a fresh render diverges from the
    // committed copy - so re-render here and, UNLIKE the warning advisories above, FAIL
    // LOUDLY (a non-zero exit, surfaced by `main`) when the committed docs no longer match,
    // naming the drifted files and the `rigger docs` fix. This is what makes the discipline
    // STAY accurate rather than merely start accurate. Runs last so the config summary and the
    // soft advisories are still seen before the hard failure. Absent files are skipped, so an
    // operator project that never carries rigger's own committed docs still passes validate.
    if let Some(failure) = docs_drift_failure(root) {
        return Err(failure.into());
    }
    Ok(())
}

/// The build-environment lines `rigger validate` prints (spec 65 unit 5, HONEST SURFACES):
/// given the wrapper ALREADY resolved by [`resolve_build_layer`] (the same authority
/// `Config::validate`'s run-start check and the conductor's build-environment authority
/// read - this never re-derives it), render:
/// - the wrapper name, or `none` when the layer is inactive;
/// - the cache dir it resolved to (via [`resolved_cache_dir`], the SAME ternary
///   [`BuildEnv::resolve`] and the cache-dir probe use) - ONLY when a wrapper is actually
///   active, since an inactive layer touches no cache dir and claiming one would fabricate
///   a surface nothing backs;
/// - the machine-wide build budget, ALWAYS: `build.max_concurrent` gates every compiler
///   invocation this loop runs (spec 65 unit 3) regardless of whether a wrapper is
///   configured, so an operator sees it even with the wrapper off. `0` is the documented
///   unlimited convention (mirrors `defaults.budget`), reported in words rather than a
///   bare, easily-misread `0`.
///
/// Pure formatting over already-resolved values, so it is unit-tested without touching
/// PATH or the filesystem; the effectful wrapper resolution stays at the `cmd_validate`
/// edge that calls this.
fn build_environment_report(wrapper: Option<&str>, build: &config::BuildConfig) -> Vec<String> {
    let mut lines = Vec::new();
    match wrapper {
        Some(w) => {
            lines.push(format!("build wrapper: {w}"));
            lines.push(format!(
                "build cache dir: {}",
                resolved_cache_dir(&build.cache_dir)
            ));
        }
        None => lines.push("build wrapper: none".to_string()),
    }
    lines.push(format!(
        "build budget: {}",
        if build.max_concurrent == 0 {
            "unlimited".to_string()
        } else {
            build.max_concurrent.to_string()
        }
    ));
    lines
}

/// The `gate <id>:` lines `rigger validate` prints after its build budget line (spec 113):
/// one per declared gate, in the order the load's resolution holds them (gate-id order),
/// `gate <id>: requires nothing`, or each resolved entry as `<name> at <path>` joined by
/// `, `. Pure over that resolution, which holds resolved entries only, so no line can name
/// an unresolved requirement.
fn gate_requirement_lines(gates: &[GateRequirements]) -> Vec<String> {
    gates
        .iter()
        .map(|g| {
            let requires = if g.requires.is_empty() {
                "nothing".to_string()
            } else {
                g.requires
                    .iter()
                    .map(|r| format!("{} at {}", r.name, r.at.display()))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            format!("gate {}: requires {requires}", g.gate)
        })
        .collect()
}

/// The non-fatal `rigger validate` advisories (spec 05:55), in report order:
///   (a) the installed `/rigger` workflow has drifted from this binary's embedded copy;
///   (b) tracked `.rigger/` files carry uncommitted modifications;
///   (c) the checkout's `.claude/settings.json` lacks rigger's session hooks.
/// Both are warnings only - they are collected here and printed to stderr by the caller
/// without affecting the exit status. Rooted at `root` so the seam is testable against a
/// temp dir without mutating the process-wide current directory.
fn validate_advisories(root: &Path) -> Vec<String> {
    let mut advisories = Vec::new();
    // Identity durability (spec 09): without a tracked project.id, identity is the volatile
    // directory basename, so a rename away orphans this project's run history. Warn (like
    // the other drift advisories) so it is seen before a rename loses the log.
    if !has_tracked_project_id(root) {
        advisories.push(format!(
            "warning: no tracked {RIGGER_DIR}/{PROJECT_ID_FILE}; this project's identity falls \
             back to the directory basename, so renaming the checkout orphans its run history. \
             Run `rigger setup` (or `rigger init`) to mint a durable id, then commit it."
        ));
    }
    // Workflow-drift diagnostic (spec 18, criterion 9): when the installed workflow differs
    // from this binary's embedded copy, name WHICH side is stale (the installed workflow vs
    // the binary) using the embedded build provenance and give the directive fix, rather
    // than an ambiguous "they differ". The binary's provenance and the git ancestry oracle
    // are wired here at the edge; the decision itself is the pure [`drift_side`].
    if let Some(advisory) =
        workflow_drift_advisory(root, BUILD_PROVENANCE, |a, b| git_is_ancestor(root, a, b))
    {
        advisories.push(advisory);
    }
    if let Some(dirty) = uncommitted_rigger_advisory(root) {
        advisories.push(dirty);
    }
    if let Some(advisory) = session_hooks_advisory(root) {
        advisories.push(advisory);
    }
    // Spec 74, criterion 2: the missing-go-gitsemver-binary advisory (fires whenever
    // THIS binary's own embedded version carries the `+unversioned` marker, regardless
    // of cause) and the behind-the-tree advisory (fires when the checkout's freshly
    // re-derived version is genuinely ahead of it) are independent - both wired here,
    // never mutually exclusive in code even though in practice at most one condition
    // tends to hold at a time (an unversioned installed side already silences the
    // behind-the-tree comparison on its own, inside `behind_the_tree_message`).
    if let Some(advisory) = missing_gitsemver_binary_advisory(GITSEMVER_VERSION) {
        advisories.push(advisory);
    }
    if let Some(advisory) = behind_the_tree_advisory(root, GITSEMVER_VERSION, BUILD_PROVENANCE) {
        advisories.push(advisory);
    }
    advisories
}

/// The INDEX STALENESS advisory line (spec 68, VALIDATE ADVISORIES), rendered from an already-
/// computed [`rigger::grounder::symbols::IndexDrift`] (the pure formatting stays separate from
/// the gathering in [`rigger::grounder::symbols::staleness`], exactly like
/// [`build_environment_report`] above). Names counts per kind of disagreement - never just a
/// bare "it drifted" - and the fix, `rigger reindex`.
fn index_staleness_message(drift: &rigger::grounder::symbols::IndexDrift) -> String {
    let mut parts = Vec::new();
    if !drift.added.is_empty() {
        parts.push(format!(
            "{} file(s) on disk not yet in the index",
            drift.added.len()
        ));
    }
    if !drift.removed.is_empty() {
        parts.push(format!(
            "{} indexed file(s) no longer on disk",
            drift.removed.len()
        ));
    }
    if !drift.changed.is_empty() {
        parts.push(format!(
            "{} sampled file(s) whose content changed",
            drift.changed.len()
        ));
    }
    format!(
        "warning: the symbols grounding index ({}) has drifted from the tree ({}). Run \
         `rigger reindex <file>...` to refresh it.",
        rigger::grounder::symbols::store::index_path(".").display(),
        parts.join(", "),
    )
}

/// The GRAPH INDEX LAG advisory line (spec 92 criterion 1, FRESH ON EVERY INTEGRATION), rendered
/// from an already-sampled list of files [`rigger::ingest::graph_index_lag_sample`] found
/// disagreeing with `graph.db`'s own last recorded generation for them. `None` when the sample is
/// empty - nothing to warn about, not merely nothing measured (the pure formatting stays separate
/// from the gathering, exactly like [`index_staleness_message`] above). Names every lagging file
/// (never just a bare count) and the fix, `rigger reindex`, so the same fix that keeps the
/// `symbols` index fresh also closes the gap this advisory reports.
fn graph_index_lag_advisory(lagging: &[String]) -> Option<String> {
    if lagging.is_empty() {
        return None;
    }
    Some(format!(
        "warning: the context graph has fallen behind {} sampled file(s) it previously indexed \
         ({}). Run `rigger reindex <file>...` to refresh it.",
        lagging.len(),
        lagging.join(", "),
    ))
}

/// The derived-index duplication FACTOR (rows per row a compaction keeps) above which `rigger
/// validate` warns of log bloat (Design: "derived-type duplication factor above threshold"). `1.5`
/// means at least half again as many recordings as a compaction would keep sit in the log - a real
/// redundancy signal, not the occasional legitimate re-recording (a revert, a branch switch) a
/// small, healthy log can carry without ever being worth an operator's attention.
const BLOAT_DUPLICATION_THRESHOLD: f64 = 1.5;

/// The LOG BLOAT advisory line (spec 68, VALIDATE ADVISORIES), rendered from an already-measured
/// [`rigger::eventstore::sqlite::DerivedDuplication`] - pure formatting, separate from the
/// gathering in [`bloat_advisory_for`]. `None` when the measured factor does not clear
/// [`BLOAT_DUPLICATION_THRESHOLD`].
fn bloat_advisory(measured: &rigger::eventstore::sqlite::DerivedDuplication) -> Option<String> {
    let factor = measured.factor();
    if factor <= BLOAT_DUPLICATION_THRESHOLD {
        return None;
    }
    Some(format!(
        "warning: the event log's derived index is duplicated {factor:.1}x ({} row(s), of which a \
         compaction keeps only {}); run `rigger reset --derived` to compact it.",
        measured.rows, measured.kept
    ))
}

/// Gather + measure the LOG BLOAT advisory's input (spec 68): open the sqlite event log at
/// `path`, scoped to `project`'s stream prefix, and run
/// [`rigger::eventstore::sqlite::Store::measure_derived_duplication`] - the ONE read-only
/// aggregate the compaction's own `key_expr`/`type_list` authority backs (Design: "no shadow
/// accounting"). `None`, never an error, on every reason there is nothing honest to measure:
/// a server-backed project (this is a sqlite-only mechanic, exactly like `reset --derived`
/// itself refuses there - see [`cmd_reset`]), or a project with no `events.db` file YET - checked
/// BEFORE opening anything, because [`open_sqlite_store`] (like [`Store::open`] under it) creates
/// a missing file, and a read-only advisory must never have that side effect. Any read error
/// after that point (a malformed store, a lock) is likewise swallowed, exactly like the model-
/// drift and order-signature advisories above.
fn bloat_advisory_for(path: &str, project: &str) -> Option<String> {
    let sel = store_selection(None, None).ok()?;
    if !sel.is_sqlite() || !Path::new(path).exists() {
        return None;
    }
    let store = open_sqlite_store(path).ok()?;
    let prefix = Namespaced::prefix_for(project);
    let measured = store
        .measure_derived_duplication(&prefix, &rigger::ingest::derived_index_identity())
        .ok()?;
    bloat_advisory(&measured)
}

/// The RETIRED CODE-ENTITY advisory line (spec 86 criterion 3, THE MIGRATION IS DELIBERATE),
/// rendered from an already-measured count - pure formatting, separate from the gathering in
/// [`retired_entities_advisory_for`]. `None` when nothing has been retired: the steady-state
/// case, and the honest answer before any re-ingest has excluded or removed anything.
fn retired_entities_advisory(n: usize) -> Option<String> {
    if n == 0 {
        return None;
    }
    let noun = if n == 1 { "entity" } else { "entities" };
    Some(format!(
        "{n} code-{noun} retired (no longer reachable by any live edge; history stays in the \
         log, never a store wipe)"
    ))
}

/// Gather + measure the RETIRED CODE-ENTITY advisory's input (spec 86 criterion 3): open the graph
/// at `graph_db`, scoped to `project`, and read
/// [`rigger::contextgraph::sqlite::Projector::retired_code_entity_count`] - the ONE counting
/// authority the migration's supersession backs (never a second, shadow count). The context graph
/// is always a local sqlite file regardless of `--eventstore` (unlike the event log this mirrors
/// the shape of, `Projector` is the only [`Projection`] this binary ever opens), so this needs no
/// backend-selection guard. `None`, never an error, on every reason there is nothing honest to
/// report: no `graph.db` file YET, or one that does not open ([`standing_graph`]), or any read
/// error after that point, exactly like the log-bloat and index-staleness advisories above
/// swallow one.
fn retired_entities_advisory_for(graph_db: &str, project: &str) -> Option<String> {
    let graph = standing_graph(graph_db, project)?;
    retired_entities_advisory(graph.retired_code_entity_count().ok()?)
}

/// Whether the `/rigger` workflow installed at `<root>/.claude/workflows/rigger.js` has
/// DRIFTED from the embedded [`RIGGER_WORKFLOW`] this binary ships. `false` when the file
/// is absent (nothing installed, so nothing to drift) or byte-identical to the embedded
/// copy; `true` only when an installed file differs. This is the single source of truth
/// for the "installed vs embedded workflow" comparison - it reuses the same
/// [`workflow_path`] and [`RIGGER_WORKFLOW`] that [`install_workflow`] writes, so the
/// drift check and the install can never disagree on what "the workflow" is.
fn installed_workflow_drifted(root: &Path) -> bool {
    match std::fs::read(workflow_path(root)) {
        Ok(bytes) => bytes != RIGGER_WORKFLOW.as_bytes(),
        Err(_) => false, // absent or unreadable: no installed workflow to surface drift for
    }
}

/// Whether commit `ancestor` is an ancestor of commit `descendant` in the git repository
/// rooted at `root`: `Some(true)`/`Some(false)` when git can decide, `None` when it cannot
/// (git unavailable, not a repo, or either id unresolvable - e.g. an operator project that
/// does not carry rigger's history). Uses `git merge-base --is-ancestor`, whose exit status
/// is 0 for an ancestor and 1 otherwise; any other status is treated as undecidable. This
/// is the ordering oracle the [`drift_side`] decision injects, so the pure decision stays
/// testable in both directions without a live repo.
///
/// Spec 74, criterion 2 periphery finding: captures the child's stdout/stderr (`.output()`)
/// rather than inheriting the parent's (`.status()`), so an unresolvable `ancestor` - the
/// exact "does not carry rigger's history" case this doc comment already calls out as
/// normal and silently handled via `None` - never leaks git's own `fatal: Not a valid
/// object name ...` onto the CALLER's stderr. Before this fix `behind_the_tree_advisory`
/// (below) called this UNCONDITIONALLY on every `rigger validate` invocation, so the leak
/// fired on nearly every non-self-hosting target project, not merely the rare drifted-
/// workflow-with-recorded-provenance case [`workflow_drift_advisory`] alone would reach;
/// mirrors [`git_commit_distance`]'s already-correct `.output()` pattern immediately below.
fn git_is_ancestor(root: &Path, ancestor: &str, descendant: &str) -> Option<bool> {
    let out = subprocess::command("git")
        .args(["merge-base", "--is-ancestor", ancestor, descendant])
        .current_dir(root)
        .output()
        .ok()?;
    match out.status.code() {
        Some(0) => Some(true),
        Some(1) => Some(false),
        _ => None,
    }
}

/// Number of commits `descendant` carries beyond `ancestor` (`git rev-list --count
/// ancestor..descendant`), or `None` when git cannot decide (unavailable, not a repo, or
/// either id unresolvable - the same undecidable cases [`git_is_ancestor`] reports).
/// Meaningful only once the caller already knows `ancestor` truly is a (proper) ancestor
/// of `descendant`: this just counts, it never itself verifies order (spec 74, criterion
/// 2's commit-distance figure for the behind-the-tree advisory).
fn git_commit_distance(root: &Path, ancestor: &str, descendant: &str) -> Option<u64> {
    let out = subprocess::command("git")
        .args(["rev-list", "--count", &format!("{ancestor}..{descendant}")])
        .current_dir(root)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout).ok()?.trim().parse().ok()
}

/// The missing-`go-gitsemver`-binary advisory (spec 74, criterion 2): whenever THIS
/// installed binary's own embedded version ([`GITSEMVER_VERSION`]) carries the
/// [`gitsemver::UNVERSIONED_SUFFIX`] marker - REGARDLESS OF CAUSE (the tool missing from
/// PATH at build time, the build not run inside a git checkout, or any other reason
/// [`gitsemver::derive_version`] fell back). One uniform advisory for every cause,
/// mirroring `derive_version`'s own one-uniform-fallback contract (see
/// `build/gitsemver.rs`'s module doc: the embedded marker itself cannot distinguish its
/// cause, so every cause folds into one signal - here too, into one advisory). `None`
/// when the version was genuinely derived.
fn missing_gitsemver_binary_advisory(installed_version: &str) -> Option<String> {
    if !installed_version.ends_with(gitsemver::UNVERSIONED_SUFFIX) {
        return None;
    }
    Some(format!(
        "warning: this rigger binary's version ({installed_version}) could not be derived by \
         go-gitsemver at build time (the tool may be missing from PATH, or the build did not \
         run inside a git checkout - the embedded marker cannot distinguish which). Install \
         go-gitsemver (github.com/MyCarrier-DevOps/go-gitsemver) and rebuild so future builds \
         report a real, comparable version instead of the crate's bare semver."
    ))
}

/// The pure "behind-the-tree" decision (spec 74, criterion 2): given the installed
/// binary's version, the checkout's freshly re-derived version, and how many commits (if
/// decidable) the checkout is ahead of the installed build, decide the advisory text.
/// `None` whenever there is nothing actionable: the two version strings already agree
/// (nothing that would change the derived order actually changed), either side carries
/// the `+unversioned` marker (derivation unavailable is a DIFFERENT condition, reported
/// separately by [`missing_gitsemver_binary_advisory`] - a string comparison against a
/// fallback marker would be meaningless noise here), the git order was undecidable, or
/// the checkout is not (or no longer) ahead. `commit_distance` is injected so this stays
/// pure and testable without a live repo - mirrors [`drift_side`]'s injected-oracle shape.
fn behind_the_tree_message(
    installed_version: &str,
    checkout_version: &str,
    commit_distance: Option<u64>,
) -> Option<String> {
    if installed_version == checkout_version
        || installed_version.ends_with(gitsemver::UNVERSIONED_SUFFIX)
        || checkout_version.ends_with(gitsemver::UNVERSIONED_SUFFIX)
    {
        return None;
    }
    let distance = commit_distance?;
    if distance == 0 {
        return None;
    }
    Some(format!(
        "warning: this checkout's derived version ({checkout_version}) is {distance} commit(s) \
         ahead of the installed rigger binary's version ({installed_version}); rebuild rigger \
         so the binary matches the tree."
    ))
}

/// Gather the behind-the-tree advisory (spec 74, criterion 2) at `root`, given
/// `installed_version` and `installed_commit` (the composition root wires
/// [`GITSEMVER_VERSION`] and [`BUILD_PROVENANCE`] - the SAME commit-determined identity
/// [`workflow_drift_advisory`] already uses, per the spec's global constraint that
/// stored provenance keeps the build hash as its identity key). Re-derives the
/// CHECKOUT's current version through the SAME derivation seam criterion 1 embeds at
/// compile time ([`gitsemver::derive_version`], reused here rather than reimplemented -
/// a live, runtime invocation of `go-gitsemver`/git for VALIDATE's comparison
/// specifically, never for `rigger version`'s own compile-time-only self-report; see the
/// `mod gitsemver` doc comment above for why this is in-bounds). "Ahead" is decided by
/// real git ANCESTRY of `installed_commit` in `HEAD` (mirrors [`git_is_ancestor`]'s
/// existing role in [`drift_side`]) rather than a hand-rolled semver comparison: under
/// this project's Mainline config the derived order is monotonic with commit order along
/// one line of history, and ancestry is the more literal reading of the Problem
/// statement's own "behind the tree" framing, with no new dependency. Only when
/// `installed_commit` truly is a proper ancestor of `HEAD` is a distance even computed;
/// the actual decision is the pure [`behind_the_tree_message`].
fn behind_the_tree_advisory(
    root: &Path,
    installed_version: &str,
    installed_commit: &str,
) -> Option<String> {
    let checkout_version = gitsemver::derive_version("go-gitsemver", root);
    let distance = if git_is_ancestor(root, installed_commit, "HEAD") == Some(true) {
        git_commit_distance(root, installed_commit, "HEAD")
    } else {
        None
    };
    behind_the_tree_message(installed_version, &checkout_version, distance)
}

/// Which side of an installed-vs-embedded workflow drift is stale (spec 18, criterion 9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DriftSide {
    /// The installed workflow is from a NEWER build than this binary: the binary is stale
    /// (rebuild it).
    BinaryStale,
    /// The installed workflow is older than - or was hand-edited away from - this binary's
    /// embedded copy: the workflow is stale (`rigger setup` to refresh it).
    WorkflowStale,
}

/// Decide which side of a workflow drift is stale from the two builds' provenance, using an
/// injected ancestry oracle so the decision is pure and testable in both directions. Names
/// the BINARY as stale ONLY when the installed workflow's build is provably newer (this
/// binary's build is a proper ancestor of it). Every other case (the installed build equals
/// this binary from a local hand-edit, no recorded provenance, or an undecidable order)
/// resolves to the actionable refresh directive, so the diagnostic is never the ambiguous
/// "they differ".
fn drift_side(
    installed_provenance: Option<&str>,
    binary_provenance: &str,
    is_ancestor: impl Fn(&str, &str) -> Option<bool>,
) -> DriftSide {
    match installed_provenance {
        Some(installed)
            if installed != binary_provenance
                && is_ancestor(binary_provenance, installed) == Some(true) =>
        {
            DriftSide::BinaryStale
        }
        _ => DriftSide::WorkflowStale,
    }
}

/// The workflow-drift advisory (spec 18, criterion 9): when the installed `/rigger` workflow
/// differs from this binary's embedded copy, name WHICH side is stale using the build
/// provenance and give the directive fix (rebuild the binary vs `rigger setup`), never an
/// ambiguous "they differ". `None` when there is no drift. `binary_provenance` and the
/// ancestry oracle are injected so the message is testable for both drift directions without
/// a live git repo; the composition root wires [`BUILD_PROVENANCE`] and [`git_is_ancestor`].
fn workflow_drift_advisory(
    root: &Path,
    binary_provenance: &str,
    is_ancestor: impl Fn(&str, &str) -> Option<bool>,
) -> Option<String> {
    if !installed_workflow_drifted(root) {
        return None;
    }
    let path = workflow_path(root);
    let installed_provenance = installed_workflow_provenance(root);
    Some(
        match drift_side(
            installed_provenance.as_deref(),
            binary_provenance,
            is_ancestor,
        ) {
            DriftSide::BinaryStale => format!(
                "warning: the installed /rigger workflow ({}) is from a newer build ({}) than \
                 this rigger binary (build {}); the binary is stale. Rebuild rigger so the \
                 workflow and the binary that drives it are the same build.",
                path.display(),
                installed_provenance.as_deref().unwrap_or("a newer build"),
                binary_provenance,
            ),
            DriftSide::WorkflowStale => format!(
                "warning: the installed /rigger workflow ({}) has drifted from this rigger \
                 binary's embedded copy (build {}); the installed workflow is stale. Run \
                 `rigger setup` to refresh it so the workflow and the binary that drives it \
                 are the same build.",
                path.display(),
                binary_provenance,
            ),
        },
    )
}

/// Advisory naming the tracked `.rigger/` files that carry uncommitted modifications, or
/// `None` when the tracked `.rigger/` tree is clean (or the project is not a git repo, or
/// git is unavailable - in which case there is nothing to flag). Runs `git status
/// --porcelain -- .rigger` rooted at `root` and folds its output through the pure
/// [`dirty_tracked_paths`] seam.
/// Warn when the checkout's `.claude/settings.json` lacks any of the session settings
/// `rigger setup` installs (the prime hook, the grep-guard, the status line): a headless spawn
/// gets them from the host, but the operator's own interactive session reads them from here.
fn session_hooks_advisory(root: &Path) -> Option<String> {
    let settings = std::fs::read(root.join(".claude").join("settings.json")).unwrap_or_default();
    if rigger::hooks::carries_session_settings(&settings) {
        return None;
    }
    Some(
        "warning: .claude/settings.json does not carry rigger's session hooks (the SessionStart \
         prime hook, the PreToolUse grep-guard and the status line), so an interactive session \
         in this checkout starts unprimed and without the graph-first lookup guard. Run \
         `rigger setup` to install them."
            .to_string(),
    )
}

fn uncommitted_rigger_advisory(root: &Path) -> Option<String> {
    let out = subprocess::command("git")
        .args(["status", "--porcelain", "--", RIGGER_DIR])
        .current_dir(root)
        .output()
        .ok()?;
    if !out.status.success() {
        return None; // not a git repo / git absent: nothing to flag
    }
    let porcelain = String::from_utf8_lossy(&out.stdout);
    let dirty = dirty_tracked_paths(&porcelain);
    if dirty.is_empty() {
        return None;
    }
    let mut msg = String::from("warning: tracked .rigger/ files have uncommitted modifications:");
    for path in &dirty {
        msg.push_str("\n  - ");
        msg.push_str(path);
    }
    msg.push_str("\nCommit or discard them so a run starts from a clean, reproducible state.");
    Some(msg)
}

/// Given `git status --porcelain` output already scoped to `.rigger/`, return the paths
/// of TRACKED files with uncommitted modifications. Untracked (`??`) and ignored (`!!`)
/// entries are excluded - the criterion flags TRACKED files, and a machine-local
/// untracked/ignored file (e.g. `.rigger/events.db`, `.rigger/shim/`) is not a drift the
/// operator must commit. A porcelain line is `XY <path>` (two status columns, a space,
/// then the path); rename entries (`R  old -> new`) are reported verbatim.
fn dirty_tracked_paths(porcelain: &str) -> Vec<String> {
    porcelain
        .lines()
        .filter_map(|line| {
            // A well-formed porcelain line is at least "XY " followed by the path.
            if line.len() < 4 {
                return None;
            }
            let status = &line[..2];
            if status == "??" || status == "!!" {
                return None; // untracked or ignored: not a tracked modification
            }
            Some(line[3..].to_string())
        })
        .collect()
}

/// The stderr advisory (spec 06:60) naming the run's residue, or empty when nothing is
/// leftover. Reuses the two impure seams a courier uses - the run store (for the LIVE
/// unit set) and git (for local `rigger/u/*` branches) - then folds the pure
/// [`scan_residue`]. Anchored at `root`'s owning store so the scanned scratch root is the
/// SAME `<repo>/.rigger/tmp` the run uses; the path is resolved WITHOUT creating it, so
/// validate stays read-only.
fn residue_advisories(
    root: &Path,
    cfg: &config::Config,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| root.to_path_buf());
    // The repo whose `<repo>/.rigger/tmp` the run uses (spec 77 criterion 6:
    // [`owning_repo_root`] is the SAME authority [`footprint_report_for`] resolves its own
    // scratch root from). Keeps the scanned scratch root aligned with the run's actual one.
    let repo = owning_repo_root(&cwd);
    let scratch = PathBuf::from(rigger::worktree::scratch_root_path_from_env(
        &repo,
        &cfg.workflow.defaults.workdir,
    ));
    // A genuine store-SELECTION failure (unreadable secret file / malformed config / invalid
    // backend) SURFACES here rather than silently reading the wrong store - which, folding zero live
    // units, would misreport every LIVE `rigger/u/*` worktree/branch as removable residue
    // (d-u2rr-observer-selection-loud). The benign no-run/no-store and store-access-miss cases still
    // yield an empty live set inside `read_run_units`, so an unconfigured or never-run project scans
    // cleanly.
    let run_units = read_run_units(&cwd)?;
    let slugs = live_slugs(&run_units.live_branches);
    let local_branches = rigger::worktree::unit_branches(&cwd);
    let report = scan_residue(
        &scratch,
        &slugs,
        &run_units.dead_slugs,
        &local_branches,
        &run_units.live_branches,
    );
    let mut advisories = format_residue(&report);
    // Leaked-process advisory (spec 23, unit 2): any process still rooted under the SAME
    // resolved scratch root, warning-only like the residue block above. Reuses the `scratch`
    // path already resolved here and the shared scan authority - no second resolver, no second
    // scan - so a process left holding a now-deleted (or soon-to-be-removed) scratch dir is
    // visible even when no teardown is running.
    advisories.extend(leaked_process_advisories(&scratch));
    Ok(advisories)
}

/// The warning-only `rigger validate` advisories (spec 23, unit 2) naming every process still
/// rooted under the scratch root: a leak the teardown reap missed, or a process left running
/// while no teardown is active. ONE advisory per process, each naming its pid and command, so
/// an operator can see and reclaim it - surfaced only, like the residue block, never a hard
/// failure and never a kill (the teardown reap in `src/worktree.rs` / `cmd_step` is the only
/// kill). Empty when nothing is rooted there; and because the shared scan authority
/// ([`rigger::reap::processes_rooted_under`] - the SAME one the teardown reap consumes) returns
/// empty where the dir or `/proc` is absent, this is a graceful no-op (empty, never an error)
/// on a platform without `/proc` too.
fn leaked_process_advisories(scratch_root: &Path) -> Vec<String> {
    rigger::reap::processes_rooted_under(scratch_root)
        .into_iter()
        .map(|(pid, command)| {
            let named = if command.is_empty() {
                format!("pid {pid}")
            } else {
                format!("pid {pid} ({command})")
            };
            format!(
                "warning: process rooted under the scratch root (surfaced only - validate \
                 never reaps it): {named} - its cwd is under {}; it outlives a dir rigger owns \
                 until the next teardown or step reaps it.",
                scratch_root.display()
            )
        })
        .collect()
}

/// The CURRENT run's unit liveness, read from the run store the SAME way the couriers do
/// (walk UP to the owning store, scope by its identity). No store (a project that never
/// ran) means no live units, so every scratch worktree and `rigger/u/*` branch reads as
/// residue.
///
/// This reads the DURABLE real run stream, so it resolves WHICH backend through the one
/// authority ([`store_selection`]) exactly as every other real-run-stream read does
/// (`dash_read_run`, `canary_stats_lines`, `read_model_drift`): a project configured for the
/// server backend reads the SERVER's run (spec 48 criterion 1, "a command invoked in a project
/// configured for the server-backed store resolves that store"), never a stale local sqlite
/// file. It is NOT local-by-construction like the isolated replay store, so it must not pin
/// [`StoreSelection::Sqlite`].
///
/// A genuine selection FAILURE off a PRESENT source - an unreadable `.rigger/store.conn`, an
/// unreadable/malformed `workflow.yml`, an invalid `store.backend`, or the server selected with no
/// resolvable connection string - SURFACES as an `Err` here (propagated with `?`), never a silent
/// degrade to the local sqlite default: reading the wrong (empty local) store would fold zero live
/// units and misreport every LIVE `rigger/u/<slug>` worktree/branch as residue (via
/// [`residue_advisories`], spec 06 line 60) - the exact silent-wrong-store fracture spec 48's one
/// resolution authority and spec 19c's loud-failure-surfacing forbid (d-u2rr-observer-selection-loud).
/// Only the BENIGN "no run ever happened / nothing selected" cases degrade to `Ok(RunUnits::default())`
/// (no live units): a sqlite selection whose local store was never created, and a store that resolves
/// but cannot be opened or read (e.g. an unreachable configured server) - a store-ACCESS miss, distinct
/// from a selection FAILURE.
fn read_run_units(cwd: &Path) -> Result<RunUnits, Box<dyn std::error::Error>> {
    Ok(read_run_units_or_why(cwd)?.unwrap_or_default())
}

/// [`read_run_units`] without its best-effort degrade: a store-ACCESS miss comes back as the
/// inner `Err`, naming why, instead of as an empty live set. A caller that DELETES what it
/// judges dead (`rigger reset --build-cache`, gap 96) must fail closed on it - "cannot tell which
/// units are live" is never "no unit is live". A project whose store was never created still
/// reads as no live units: nothing ever ran there.
fn read_run_units_or_why(
    cwd: &Path,
) -> Result<Result<RunUnits, String>, Box<dyn std::error::Error>> {
    let sel = store_selection(None, None)?;
    // Resolve the store's OWNING root and identity. For sqlite the durable log is a LOCAL file,
    // so walk UP to it (as the couriers do); its absence means no run ever happened => no live
    // units. For the server backend there is no local `events.db` to walk to, so resolve through
    // the shared [`server_store_location`] (the SAME authority the store-opening couriers'
    // [`require_store_dir`] server branch uses), binding identity to the main repo root and
    // letting [`resolve_store`] reach the server.
    let loc = if sel.is_sqlite() {
        let Some(dir) = find_store_dir_from(cwd) else {
            return Ok(Ok(RunUnits::default()));
        };
        StoreLocation { dir }
    } else {
        server_store_location(cwd)
    };
    // A store-ACCESS miss (an unreachable configured server, a corrupt local log) degrades to no
    // live units - best-effort, distinct from the selection FAILURE surfaced above: the store WAS
    // resolved, it just cannot be reached, so the residue scan stays warning-only rather than
    // failing validate on a transient outage.
    let backend = match resolve_store(&sel, &loc.file("events.db")) {
        Ok(backend) => backend,
        Err(e) => return Ok(Err(format!("the run log cannot be opened: {e}"))),
    };
    let store = Namespaced::new(backend.as_ref(), &loc.identity());
    Ok(store
        .read_stream(conductor::STREAM, 0, Direction::Forward)
        .map(|events| current_run_units(&events))
        .map_err(|e| format!("the run log cannot be read: {e}")))
}

/// `rigger validate`'s unconditional FOOTPRINT report (spec 77 criterion 6): one line per
/// category naming its total size, ALWAYS printed - like [`build_environment_report`]'s
/// HONEST SURFACES - so an operator sees every category that exists even when none has
/// crossed the advisory threshold below. Pure formatting over already-measured categories.
fn footprint_report_lines(categories: &[FootprintCategory]) -> Vec<String> {
    categories
        .iter()
        .map(|c| format!("footprint: {} {}", c.name, human_size(c.total_bytes)))
        .collect()
}

/// `rigger validate`'s two FOOTPRINT ACCOUNTING output streams (spec 77 criterion 6): the
/// unconditional per-category totals (stdout) and the dead-share advisories (stderr).
/// Resolves the SAME repo/scratch-root/liveness inputs [`residue_advisories`] does, via the
/// SAME shared authorities ([`owning_repo_root`], [`rigger::worktree::scratch_root_path_from_env`],
/// [`read_run_units`]) - never a second, independently-derived notion of what is live. A
/// store-ACCESS miss (no run ever happened) degrades to an empty live set, exactly like
/// `residue_advisories`, so an unconfigured or never-run project still gets a footprint
/// report; a genuine store-SELECTION failure surfaces as `Err` (propagated with `?`),
/// never a silent wrong-store read.
fn footprint_report_for(
    cfg: &config::Config,
) -> Result<(Vec<String>, Vec<String>), Box<dyn std::error::Error>> {
    let (categories, _liveness_unknown) =
        measure_footprint(&cwd(), &cfg.workflow.defaults.workdir)?;
    Ok((
        footprint_report_lines(&categories),
        footprint_advisories(&categories),
    ))
}

/// Rigger's footprint by category, measured from `cwd` with the scratch root `workdir`
/// configures - the ONE measurement both `rigger validate`'s report and `rigger reset
/// --build-cache`'s reclaim read (gap 96), so the verb reclaims exactly what the advisory
/// calls dead. See [`footprint_report_for`] for how each input resolves. The second value is
/// `Some(why)` when the run log could not be read: the liveness-dependent classes were then
/// measured against an EMPTY live set, which an advisory may report but a reclaim must never act
/// on ([`read_run_units_or_why`]).
pub(super) fn measure_footprint(
    cwd: &Path,
    workdir: &str,
) -> Result<(Vec<FootprintCategory>, Option<String>), Box<dyn std::error::Error>> {
    let repo = owning_repo_root(cwd);
    let rigger_dir = Path::new(&repo).join(RIGGER_DIR);
    let scratch = PathBuf::from(rigger::worktree::scratch_root_path_from_env(&repo, workdir));
    let (run_units, liveness_unknown) = match read_run_units_or_why(cwd)? {
        Ok(units) => (units, None),
        Err(why) => (RunUnits::default(), Some(why)),
    };
    let slugs = live_slugs(&run_units.live_branches);
    let categories = footprint_report(
        &rigger_dir,
        &scratch,
        &slugs,
        &run_units.dead_slugs,
        run_units.current_run_scratch_leaf.as_deref(),
        &run_units.live_spawn_leaf_names,
    );
    Ok((categories, liveness_unknown))
}

/// `rigger docs` renders the operating discipline from the code the binary runs on into
/// its committed outputs - every registry skill plus every handbook page in
/// [`HANDBOOK_PAGES`] - so the discipline stays in lock-step with behavior instead of
/// drifting from it. Re-run it after changing a source fact or a template and commit the
/// result; `rigger validate` (spec 20, unit 2; spec 68, criterion 1; spec 66, criterion 2)
/// fails loudly if a committed copy drifts from a fresh render.
pub(crate) fn cmd_docs(_args: &[String]) -> Res {
    for path in write_docs(Path::new("."))? {
        println!("rendered {}", path.display());
    }
    Ok(())
}

/// The committed discipline outputs under `root` that have DRIFTED from a fresh render of
/// the current code-derived context (spec 20, unit 2; spec 68, criterion 1: the docs-drift
/// gate covers EVERY registry entry; spec 66, criterion 2: AND every [`HANDBOOK_PAGES`]
/// entry), in report order (registry order, then [`HANDBOOK_PAGES`] order). A path is
/// reported only when the committed file EXISTS and its BYTES differ from a fresh render;
/// an ABSENT (or unreadable) file is skipped - these are rigger's OWN committed docs, and
/// an operator project never carries them, so their absence is not drift (the same
/// "nothing installed, nothing to drift" rule [`installed_workflow_drifted`] applies to
/// the workflow). Reuses the SINGLE render authority ([`docs_context`] +
/// `rigger::docs::skill_registry`/[`HANDBOOK_PAGES`]) and the same [`skill_source_rel`] /
/// `HANDBOOK_PAGES` paths [`write_docs`] writes, so the drift check and the write can
/// never disagree on what "the docs" are. Rooted at `root` so the seam is testable against
/// a temp dir without touching the cwd.
fn docs_drift(root: &Path) -> Vec<std::path::PathBuf> {
    let ctx = docs_context();
    let mut checks: Vec<(std::path::PathBuf, String)> = rigger::docs::skill_registry()
        .into_iter()
        .map(|entry| (root.join(skill_source_rel(entry.name)), entry.render(&ctx)))
        .collect();
    for (rel, body) in HANDBOOK_PAGES {
        checks.push((root.join(rel), body.render(&ctx)));
    }
    let mut drifted = Vec::new();
    for (path, fresh) in checks {
        // Byte comparison (not `read_to_string`): a committed file corrupted to non-UTF-8 is
        // genuinely drifted, and comparing bytes catches it rather than silently skipping it.
        match std::fs::read(&path) {
            Ok(bytes) if bytes != fresh.as_bytes() => drifted.push(path),
            _ => {} // absent/unreadable (not our committed docs here), or byte-identical
        }
    }
    drifted
}

/// The `rigger validate` docs-drift FAILURE (spec 20, unit 2; spec 68, criterion 1): when a
/// committed discipline output has drifted from a fresh render, a single loud message
/// naming EVERY drifted file and the one-command fix, or `None` when the committed docs
/// are in sync (or absent). Unlike the warning advisories, the caller surfaces this as a
/// HARD, non-zero exit - a changed const/template/hand-edit is a definition drift that
/// must be regenerated, not a soft nudge - so the discipline STAYS in lock-step with the
/// code the binary runs on.
fn docs_drift_failure(root: &Path) -> Option<String> {
    let drifted = docs_drift(root);
    if drifted.is_empty() {
        return None;
    }
    let names = drifted
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!(
        "the committed rigger skill/discipline docs have drifted from a fresh render \
         ({names}): a source fact or template changed but the committed copy was not \
         regenerated, so the discipline no longer matches the code it describes. Run \
         `rigger docs` and commit the result so they are in lock-step again."
    ))
}

/// `rigger instructions` - print the composed instruction layers every spawned agent
/// receives: the built-in law and working discipline, then the operator files in
/// `.rigger/instructions/`.
pub(crate) fn cmd_instructions(_args: &[String]) -> Res {
    let ops = config_store::load_instructions(Path::new("."))?;
    print!("{}", instructions::render(&ops));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::git_ok_with_identity;
    use crate::test_support::git_out;
    use crate::test_support::tool_available;
    use rigger::gate::ResolvedRequirement;
    use std::process::Command;

    /// Spec 20, unit 2 (the drift seam, at the unit level); spec 68, criterion 1 (the gate
    /// covers EVERY registry entry): `docs_drift` flags a committed output whose bytes
    /// differ from a fresh render, is SILENT when the committed copies are in sync, and
    /// SKIPS an absent file (an operator project that never carries rigger's own committed
    /// docs must not be flagged). Proven against a temp root so it needs no cwd.
    #[test]
    fn docs_drift_flags_a_changed_file_and_skips_absent_or_in_sync_ones() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let skill_path = root.join(skill_source_rel("using-rigger"));
        let other_skill_path = root.join(skill_source_rel("planning-a-spec"));
        let handbook_path = root.join(HANDBOOK_DISCIPLINE_REL);

        // Absent (nothing rendered yet) -> no drift, no failure: these are rigger's OWN docs,
        // which an operator project never carries, so their absence must not fail validate.
        assert!(
            docs_drift(root).is_empty(),
            "absent committed docs are not drift"
        );
        assert!(docs_drift_failure(root).is_none());

        // Rendered from code -> in sync -> still no drift.
        write_docs(root).unwrap();
        assert!(
            docs_drift(root).is_empty(),
            "freshly rendered docs must be in sync with a fresh render"
        );
        assert!(docs_drift_failure(root).is_none());

        // Hand-edit the using-rigger skill the render would never produce -> ONLY that
        // skill drifts (planning-a-spec stays in sync), and the failure names it plus the
        // `rigger docs` fix.
        std::fs::write(&skill_path, "hand-edited, not a render\n").unwrap();
        assert_eq!(
            docs_drift(root),
            vec![skill_path.clone()],
            "only the changed committed file is flagged"
        );
        let failure = docs_drift_failure(root).expect("a drifted skill must produce a failure");
        assert!(
            failure.contains(skill_source_rel("using-rigger").as_str())
                && failure.contains("rigger docs"),
            "the drift failure must name the drifted file and the `rigger docs` fix; got: {failure}"
        );

        // Drift the OTHER registry skill too -> both are reported, in registry order.
        std::fs::write(&other_skill_path, "hand-edited, not a render\n").unwrap();
        assert_eq!(
            docs_drift(root),
            vec![skill_path.clone(), other_skill_path.clone()],
            "both drifted skills are flagged, in registry order"
        );

        // Drift the handbook too -> it reports next, after every registry skill.
        std::fs::write(&handbook_path, "hand-edited handbook, not a render\n").unwrap();
        assert_eq!(
            docs_drift(root),
            vec![
                skill_path.clone(),
                other_skill_path.clone(),
                handbook_path.clone()
            ]
        );

        // Drift the SECOND handbook page (spec 66, criterion 2) too -> it reports LAST,
        // after every registry skill and the first handbook page - proving the drift gate
        // holds over the planning field guide the same way it holds over using-rigger.md.
        let guide_path = root.join(PLANNING_FIELD_GUIDE_REL);
        std::fs::write(&guide_path, "hand-edited guide, not a render\n").unwrap();
        assert_eq!(
            docs_drift(root),
            vec![skill_path, other_skill_path, handbook_path, guide_path]
        );
    }

    /// Spec 66, criterion 2 (the handbook-page-renders criterion, stated exactly as
    /// written): `rigger docs` writes the planning field guide handbook page,
    /// `authoring-loops.md` links to it, and the docs-drift gate holds over both. The
    /// render/write half is proven against a temp root (mirroring
    /// [`write_docs_writes_every_registry_skill_plus_the_handbook`]); the link and the
    /// drift-gate-holds halves are proven against the REAL committed repo tree, so a
    /// dropped link or a hand-edit that skips `rigger docs` fails `cargo test`, not only a
    /// live `rigger validate`.
    #[test]
    fn planning_field_guide_page_renders_and_is_linked_from_authoring_loops() {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));

        // `rigger docs` writes the planning field guide handbook page.
        let dir = tempfile::tempdir().unwrap();
        let written = write_docs(dir.path()).unwrap();
        assert!(written.contains(&dir.path().join(PLANNING_FIELD_GUIDE_REL)));
        let rendered = std::fs::read_to_string(dir.path().join(PLANNING_FIELD_GUIDE_REL))
            .expect("write_docs must create the planning field guide page");
        assert_eq!(rendered, rigger::docs::PLANNING_FIELD_GUIDE_BODY);
        assert!(rendered.contains("F1 - Duplicated or ambiguously-owned units"));

        // `authoring-loops.md` links to it.
        let authoring_loops =
            std::fs::read_to_string(manifest.join("docs/handbook/authoring-loops.md"))
                .expect("authoring-loops.md must be readable from the committed tree");
        assert!(
            authoring_loops.contains("planning-field-guide.md"),
            "authoring-loops.md must link to the planning field guide handbook page"
        );

        // The docs-drift gate holds over both, against the REAL committed tree (not a
        // synthetic fixture): the committed guide page (and every other registry/handbook
        // output) is in sync with a fresh render right now.
        assert!(
            docs_drift_failure(manifest).is_none(),
            "the committed planning field guide must be in sync with a fresh render"
        );
    }

    // ---- `rigger validate` advisories (spec 05:55): pure seams + drift compare ----

    #[test]
    fn dirty_tracked_paths_keeps_tracked_modifications_and_drops_untracked_and_ignored() {
        // A mix of porcelain status codes scoped to `.rigger/`: modified-in-worktree,
        // staged, added, deleted (all TRACKED), plus untracked (`??`) and ignored (`!!`).
        let porcelain = " M .rigger/workflow.yml\n\
                         M  .rigger/agents/sdet.md\n\
                         A  .rigger/agents/new.md\n\
                         D  .rigger/agents/gone.md\n\
                         ?? .rigger/events.db\n\
                         !! .rigger/shim/node_modules\n";
        let dirty = dirty_tracked_paths(porcelain);
        assert_eq!(
            dirty,
            vec![
                ".rigger/workflow.yml".to_string(),
                ".rigger/agents/sdet.md".to_string(),
                ".rigger/agents/new.md".to_string(),
                ".rigger/agents/gone.md".to_string(),
            ],
            "only TRACKED+modified paths are flagged; untracked `??` and ignored `!!` \
             entries are excluded"
        );
    }

    #[test]
    fn dirty_tracked_paths_on_a_clean_tree_is_empty() {
        assert!(dirty_tracked_paths("").is_empty());
    }

    #[test]
    fn installed_workflow_drifted_is_false_when_absent_or_identical_and_true_on_drift() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Absent: nothing installed, so there is no drift to surface.
        assert!(
            !installed_workflow_drifted(root),
            "an absent installed workflow is not drift"
        );

        // Identical to the embedded copy: not drift.
        let path = workflow_path(root);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, RIGGER_WORKFLOW).unwrap();
        assert!(
            !installed_workflow_drifted(root),
            "an installed workflow byte-identical to the embedded copy is not drift"
        );

        // Differs from the embedded copy: drift.
        std::fs::write(&path, "// stale installed workflow\n").unwrap();
        assert!(
            installed_workflow_drifted(root),
            "an installed workflow differing from the embedded copy IS drift"
        );
    }

    // ---- spec 18, criterion 9: workflow-drift "which side is stale" diagnostic --------

    #[test]
    fn drift_side_names_the_binary_stale_only_when_the_installed_workflow_is_provably_newer() {
        // This binary's build is a PROPER ANCESTOR of the build that wrote the installed
        // workflow: the installed workflow is newer, so the BINARY is stale.
        let binary_is_ancestor = |ancestor: &str, descendant: &str| -> Option<bool> {
            Some(ancestor == "binary" && descendant == "installed")
        };
        assert_eq!(
            drift_side(Some("installed"), "binary", binary_is_ancestor),
            DriftSide::BinaryStale,
            "a provably-newer installed workflow makes the binary stale"
        );

        // The installed workflow's build is OLDER (this binary is not its ancestor): the
        // WORKFLOW is stale.
        assert_eq!(
            drift_side(Some("installed"), "binary", |_, _| Some(false)),
            DriftSide::WorkflowStale,
            "an older installed workflow makes the workflow stale"
        );

        // Undecidable order (git cannot resolve one of the ids, e.g. an operator project
        // that lacks rigger's history): fall back to the actionable refresh directive.
        assert_eq!(
            drift_side(Some("installed"), "binary", |_, _| None),
            DriftSide::WorkflowStale,
            "an undecidable order falls back to refreshing the workflow"
        );

        // No recorded provenance (an older install with no sidecar): refresh directive, and
        // the ancestry oracle is never consulted.
        assert_eq!(
            drift_side(None, "binary", |_, _| panic!(
                "ancestry must not be consulted without a recorded provenance"
            )),
            DriftSide::WorkflowStale,
            "a missing provenance falls back to refreshing the workflow"
        );

        // The installed build EQUALS this binary but the content drifted (a local
        // hand-edit): refresh directive, ancestry never consulted.
        assert_eq!(
            drift_side(Some("binary"), "binary", |_, _| panic!(
                "ancestry must not be consulted for a same-build hand-edit"
            )),
            DriftSide::WorkflowStale,
            "a same-build content edit falls back to refreshing the workflow"
        );
    }

    #[test]
    fn workflow_drift_advisory_names_which_side_is_stale_and_never_says_they_differ() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let path = workflow_path(root);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();

        // No drift (byte-identical to the embedded copy): no advisory at all.
        std::fs::write(&path, RIGGER_WORKFLOW).unwrap();
        assert!(
            workflow_drift_advisory(root, "binary", |_, _| None).is_none(),
            "no advisory when the installed workflow matches the embedded copy"
        );

        // Drift the installed workflow and record it as written by a NEWER build.
        std::fs::write(&path, "// drifted installed workflow\n").unwrap();
        std::fs::write(workflow_provenance_path(root), "installed-newer\n").unwrap();
        let binary_stale = workflow_drift_advisory(root, "binary-old", |anc, desc| {
            Some(anc == "binary-old" && desc == "installed-newer")
        })
        .expect("a drifted workflow yields an advisory");
        assert!(
            binary_stale.contains("the binary is stale")
                && binary_stale.to_lowercase().contains("rebuild")
                && binary_stale.contains("installed-newer")
                && binary_stale.contains("binary-old"),
            "the binary-stale advisory names the binary as stale, says rebuild, and cites \
             both provenances; got: {binary_stale}"
        );
        assert!(
            !binary_stale.contains("they differ"),
            "the advisory must never be the ambiguous 'they differ'; got: {binary_stale}"
        );

        // Same drifted file, but recorded as an OLDER build: the WORKFLOW is stale.
        std::fs::write(workflow_provenance_path(root), "installed-old\n").unwrap();
        let workflow_stale = workflow_drift_advisory(root, "binary-new", |_, _| Some(false))
            .expect("a drifted workflow yields an advisory");
        assert!(
            workflow_stale.contains("the installed workflow is stale")
                && workflow_stale.contains("rigger setup")
                && workflow_stale.contains(".claude/workflows/rigger.js"),
            "the workflow-stale advisory names the workflow as stale, says `rigger setup`, \
             and names the file; got: {workflow_stale}"
        );
        assert!(
            !workflow_stale.contains("they differ"),
            "the advisory must never be the ambiguous 'they differ'; got: {workflow_stale}"
        );
    }

    /// A fresh repo at `root` with a linear history of `commits` commits (each rewriting one
    /// file), returning their ids oldest first.
    fn linear_history(root: &Path, commits: usize) -> Vec<String> {
        let git = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .args(args)
                .current_dir(root)
                .env("GIT_AUTHOR_NAME", "t")
                .env("GIT_AUTHOR_EMAIL", "t@e")
                .env("GIT_COMMITTER_NAME", "t")
                .env("GIT_COMMITTER_EMAIL", "t@e")
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            String::from_utf8(out.stdout).unwrap().trim().to_string()
        };
        git(&["init", "-q"]);
        (1..=commits)
            .map(|n| {
                std::fs::write(root.join("a"), n.to_string()).unwrap();
                git(&["add", "."]);
                git(&["commit", "-q", "-m", &format!("commit {n}")]);
                git(&["rev-parse", "HEAD"])
            })
            .collect()
    }

    #[test]
    fn git_is_ancestor_decides_commit_order_in_a_real_repo() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let history = linear_history(root, 2);
        let (first, second) = (&history[0], &history[1]);

        assert_eq!(
            git_is_ancestor(root, first, second),
            Some(true),
            "the parent commit is an ancestor of the child"
        );
        assert_eq!(
            git_is_ancestor(root, second, first),
            Some(false),
            "the child commit is not an ancestor of the parent"
        );
        assert_eq!(
            git_is_ancestor(root, &"0".repeat(40), second),
            None,
            "an unresolvable id makes the order undecidable"
        );
    }

    #[test]
    fn git_commit_distance_counts_commits_ahead_in_a_real_repo() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let history = linear_history(root, 3);
        let (first, third) = (&history[0], &history[2]);

        assert_eq!(
            git_commit_distance(root, first, third),
            Some(2),
            "two commits separate first and third"
        );
        assert_eq!(
            git_commit_distance(root, first, first),
            Some(0),
            "a commit is zero commits ahead of itself"
        );
        assert_eq!(
            git_commit_distance(root, &"0".repeat(40), third),
            None,
            "an unresolvable id makes the distance undecidable"
        );
    }

    #[test]
    fn missing_gitsemver_binary_advisory_fires_only_on_the_unversioned_marker() {
        assert!(
            missing_gitsemver_binary_advisory("1.2.3+abcdef").is_none(),
            "a genuinely derived version names no missing-binary advisory"
        );
        for installed in ["0.3.0+unversioned", "9.9.9+unversioned"] {
            let advisory = missing_gitsemver_binary_advisory(installed).unwrap_or_else(|| {
                panic!("every +unversioned-marked version must yield an advisory (cause is irrelevant); got None for {installed}")
            });
            assert!(
                advisory.contains("go-gitsemver") && advisory.contains(installed),
                "the advisory must name go-gitsemver and the installed version; got: {advisory}"
            );
        }
    }

    /// Each `(installed, checkout, distance, why)` case carries nothing actionable: the
    /// behind-the-tree advisory stays silent.
    fn assert_behind_the_tree_silent(cases: &[(&str, &str, Option<u64>, &str)]) {
        for (installed, checkout, distance, why) in cases {
            assert!(
                behind_the_tree_message(installed, checkout, *distance).is_none(),
                "{why}"
            );
        }
    }

    rigger::test_cases! {
        behind_the_tree_message_is_silent_when_versions_already_match:
            assert_behind_the_tree_silent(&[(
            "1.2.3+abc",
            "1.2.3+abc",
            Some(5),
            "identical versions carry nothing actionable, regardless of a nonzero distance",
        )]);
        behind_the_tree_message_is_silent_when_either_side_is_unversioned:
            assert_behind_the_tree_silent(&[
            (
                "0.3.0+unversioned",
                "1.0.0+abc",
                Some(3),
                "an unversioned installed side is reported by the missing-binary advisory, not \
                 this one",
            ),
            (
                "1.0.0+abc",
                "0.3.0+unversioned",
                Some(3),
                "an unversioned checkout side has nothing comparable to report",
            ),
        ]);
        behind_the_tree_message_is_silent_on_an_undecidable_or_zero_distance:
            assert_behind_the_tree_silent(&[
            (
                "1.0.0+abc",
                "1.1.0+def",
                None,
                "an undecidable git order (diverged history, an unresolvable id) reports nothing",
            ),
            (
                "1.0.0+abc",
                "1.1.0+def",
                Some(0),
                "zero commits ahead is not behind the tree",
            ),
        ]);
    }

    #[test]
    fn behind_the_tree_message_names_both_versions_and_the_commit_distance() {
        let msg = behind_the_tree_message("1.0.0+abc123", "1.1.0+def456", Some(4))
            .expect("a genuine ahead-by-N case must yield an advisory");
        assert!(
            msg.contains("1.0.0+abc123") && msg.contains("1.1.0+def456") && msg.contains('4'),
            "the advisory must name both versions and the commit distance; got: {msg}"
        );
    }

    #[test]
    fn behind_the_tree_advisory_names_the_real_derived_version_ahead_of_the_installed_commit() {
        if !tool_available("go-gitsemver", "version") {
            eprintln!("skipping: go-gitsemver not on PATH");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        git_ok_with_identity(root, &["init", "-q"]);
        std::fs::write(
            root.join("go-gitsemver.yml"),
            "mode: Mainline\ntag-prefix: v\n",
        )
        .unwrap();
        git_ok_with_identity(root, &["add", "."]);
        git_ok_with_identity(root, &["commit", "-q", "-m", "chore: initial"]);
        git_ok_with_identity(root, &["tag", "v1.0.0"]);
        let installed_commit = git_out(root, &["rev-parse", "HEAD"]);
        let installed_version = gitsemver::derive_version("go-gitsemver", root);

        std::fs::write(root.join("file.txt"), "second\n").unwrap();
        git_ok_with_identity(root, &["add", "."]);
        git_ok_with_identity(root, &["commit", "-q", "-m", "feat: add a thing"]);

        let advisory = behind_the_tree_advisory(root, &installed_version, &installed_commit)
            .expect("a checkout genuinely ahead of the installed commit must yield an advisory");
        assert!(
            advisory.contains(&installed_version),
            "the advisory must name the installed version; got: {advisory}"
        );
        assert!(
            advisory.contains('1'),
            "the checkout is exactly one commit ahead; got: {advisory}"
        );
        assert!(
            advisory.contains("1.1.0"),
            "the feat: commit must bump the minor in the reported checkout version; got: \
             {advisory}"
        );
    }

    #[test]
    fn behind_the_tree_advisory_is_silent_when_the_checkout_has_not_moved() {
        if !tool_available("go-gitsemver", "version") {
            eprintln!("skipping: go-gitsemver not on PATH");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        git_ok_with_identity(root, &["init", "-q"]);
        std::fs::write(
            root.join("go-gitsemver.yml"),
            "mode: Mainline\ntag-prefix: v\n",
        )
        .unwrap();
        git_ok_with_identity(root, &["add", "."]);
        git_ok_with_identity(root, &["commit", "-q", "-m", "chore: initial"]);
        let installed_commit = git_out(root, &["rev-parse", "HEAD"]);
        let installed_version = gitsemver::derive_version("go-gitsemver", root);

        assert!(
            behind_the_tree_advisory(root, &installed_version, &installed_commit).is_none(),
            "the installed commit equals HEAD, so there is nothing to report"
        );
    }

    #[test]
    fn validate_advisories_warns_on_workflow_drift_naming_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let path = workflow_path(root);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "// drifted\n").unwrap();

        let advisories = validate_advisories(root);
        assert!(
            advisories
                .iter()
                .any(|a| a.contains("drifted") && a.contains(".claude/workflows/rigger.js")),
            "a drifted installed workflow yields a drift advisory naming the file; got: \
             {advisories:?}"
        );
    }

    #[test]
    fn footprint_report_lines_reports_every_categorys_total_unconditionally() {
        let categories = vec![
            FootprintCategory {
                name: "store",
                total_bytes: 100,
                dead_bytes: 0,
                reclaim_hint: None,
                reclaimable: Vec::new(),
            },
            FootprintCategory {
                name: "shared build cache",
                total_bytes: 0,
                dead_bytes: 0,
                reclaim_hint: Some("rigger reset --build-cache"),
                reclaimable: Vec::new(),
            },
        ];
        let lines = footprint_report_lines(&categories);
        assert_eq!(
            lines,
            vec![
                "footprint: store 100B".to_string(),
                "footprint: shared build cache 0B".to_string(),
            ],
            "one line per category, including an EMPTY one - never conditional"
        );
    }

    // ---- `rigger validate` leaked-process advisory (spec 23, unit 2) ----------------

    #[test]
    fn leaked_process_advisories_name_a_process_rooted_under_the_scratch_root() {
        // spec 23 unit 2: a process whose cwd is under the scratch root is surfaced as a
        // warning-only advisory naming its pid, so a leaked build/tool is visible even when no
        // teardown is running. Consumes the SAME scan authority the teardown reap uses.
        let dir = tempfile::tempdir().unwrap();
        let scratch = dir.path().join("tmp");
        let inside = scratch.join("probe");
        std::fs::create_dir_all(&inside).unwrap();

        let mut child = Command::new("sleep")
            .arg("300")
            .current_dir(&inside)
            .spawn()
            .expect("spawn probe child");

        // Wait until the kernel reports the child rooted under the scratch root, then capture.
        let mut advisories = Vec::new();
        for _ in 0..200 {
            let a = leaked_process_advisories(&scratch);
            if a.iter()
                .any(|line| line.contains(&format!("pid {}", child.id())))
            {
                advisories = a;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }

        // Reap the fixture before asserting so a failed assert never leaks the sleeper.
        let _ = child.kill();
        let _ = child.wait();

        assert!(
            advisories.iter().any(|line| line.starts_with("warning:")
                && line.contains("scratch root")
                && line.contains(&format!("pid {}", child.id()))),
            "a process rooted under the scratch root yields a warning-only advisory naming its \
             pid; got: {advisories:?}"
        );
    }

    #[test]
    fn leaked_process_advisories_is_empty_when_no_process_is_rooted_under_the_scratch_root() {
        // None rooted under the scratch root: the advisory list is empty, so validate stays
        // silent about leaked processes.
        let dir = tempfile::tempdir().unwrap();
        let scratch = dir.path().join("tmp");
        std::fs::create_dir_all(&scratch).unwrap();
        assert!(
            leaked_process_advisories(&scratch).is_empty(),
            "an empty scratch root yields no leaked-process advisory"
        );
    }

    #[test]
    fn leaked_process_advisories_is_a_graceful_no_op_when_the_scratch_root_is_absent() {
        // Platform tolerance: an absent scratch root - the stand-in for an absent `/proc`,
        // since the shared scanner short-circuits to empty in both cases - yields an empty
        // list and NEVER an error, so validate keeps working on any platform.
        let dir = tempfile::tempdir().unwrap();
        let absent = dir.path().join("never-created");
        assert!(leaked_process_advisories(&absent).is_empty());
    }

    #[test]
    fn project_identity_reads_the_tracked_id_file_then_falls_back_to_the_basename() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // No project.id: identity is the legacy basename (unchanged pre-spec-09 behavior).
        let basename = root.file_name().unwrap().to_str().unwrap().to_string();
        assert_eq!(project_identity_at(root), basename);
        assert_eq!(legacy_identity_at(root), basename);

        // A tracked project.id, when present (and trimmed), IS the identity - it survives a
        // directory rename because it does not track the basename.
        std::fs::create_dir_all(root.join(RIGGER_DIR)).unwrap();
        std::fs::write(
            root.join(RIGGER_DIR).join(PROJECT_ID_FILE),
            "  durable-id-42 \n",
        )
        .unwrap();
        assert_eq!(project_identity_at(root), "durable-id-42");
        // The legacy resolver ignores the file, so the migration can still name the "before".
        assert_eq!(legacy_identity_at(root), basename);
        assert!(has_tracked_project_id(root));

        // A blank id file is treated as absent (falls back), never an empty identity.
        std::fs::write(root.join(RIGGER_DIR).join(PROJECT_ID_FILE), "   \n").unwrap();
        assert_eq!(project_identity_at(root), basename);
        assert!(!has_tracked_project_id(root));
    }

    // --- Spec 65 unit 5: `rigger validate` SURFACES the resolved wrapper, cache dir, and
    // budget; `rigger setup` writes `build: { wrapper: auto }` for new projects only. ---

    /// With a wrapper active, `rigger validate` reports the wrapper name, the cache dir it
    /// resolved to, AND the budget - three lines, in that order, so an operator sees the
    /// whole resolved build environment at a glance (Design: "wrapper ... cache dir, slot
    /// budget"). The cache dir line reads through the SAME [`resolved_cache_dir`] the
    /// [`BuildEnv`] resolver and the cache-dir probe use - never a second independently
    /// re-derived path.
    #[test]
    fn build_environment_report_with_a_wrapper_lists_wrapper_cache_dir_and_budget() {
        let build = config::BuildConfig {
            wrapper: "sccache".to_string(),
            cache_dir: "/tmp/example-cache".to_string(),
            jobs: 0,
            max_concurrent: 4,
            mutation: String::new(),
        };
        let lines = build_environment_report(Some("sccache"), &build);
        assert_eq!(
            lines,
            vec![
                "build wrapper: sccache".to_string(),
                "build cache dir: /tmp/example-cache".to_string(),
                "build budget: 4".to_string(),
            ]
        );
    }

    /// With no wrapper resolved (`off`, or `auto` finding nothing), NO cache dir line is
    /// printed - an inactive layer touches no cache dir, so claiming one would be a fabricated
    /// surface - but the budget line still prints: `build.max_concurrent` gates every compiler
    /// invocation this loop runs regardless of whether a wrapper is configured.
    #[test]
    fn build_environment_report_with_no_wrapper_omits_cache_dir_but_keeps_budget() {
        let build = config::BuildConfig {
            wrapper: String::new(),
            cache_dir: String::new(),
            jobs: 0,
            max_concurrent: 8,
            mutation: String::new(),
        };
        let lines = build_environment_report(None, &build);
        assert_eq!(
            lines,
            vec![
                "build wrapper: none".to_string(),
                "build budget: 8".to_string(),
            ]
        );
    }

    /// `max_concurrent: 0` is the documented unlimited convention (matching
    /// `defaults.budget`'s own `0` = unlimited); the report says so in words, never a bare
    /// misleading `0`.
    #[test]
    fn build_environment_report_zero_max_concurrent_reports_unlimited() {
        let build = config::BuildConfig {
            max_concurrent: 0,
            ..Default::default()
        };
        let lines = build_environment_report(None, &build);
        assert!(
            lines.iter().any(|l| l == "build budget: unlimited"),
            "a zero max_concurrent must report as unlimited, got: {lines:?}"
        );
    }

    // --- Spec 113: `rigger validate` reports each declared gate's requirements from the
    // load's own resolution. ---

    /// A gate requiring nothing renders `requires nothing`.
    #[test]
    fn gate_requirement_lines_render_a_gate_requiring_nothing() {
        let gates = vec![GateRequirements {
            gate: "build".into(),
            requires: vec![],
        }];
        assert_eq!(
            gate_requirement_lines(&gates),
            vec!["gate build: requires nothing".to_string()]
        );
    }

    /// A gate requiring two executables renders each as `<name> at <path>`, joined by `, `,
    /// one line per gate in the gate-id order the resolution holds.
    #[test]
    fn gate_requirement_lines_render_each_resolved_entry_joined_in_gate_id_order() {
        let gates = vec![
            GateRequirements {
                gate: "build".into(),
                requires: vec![],
            },
            GateRequirements {
                gate: "sweep".into(),
                requires: vec![
                    ResolvedRequirement {
                        name: "sweeper".into(),
                        at: PathBuf::from("/home/u/.cargo/bin/sweeper"),
                    },
                    ResolvedRequirement {
                        name: "cargo-nextest".into(),
                        at: PathBuf::from("/usr/local/bin/cargo-nextest"),
                    },
                ],
            },
        ];
        assert_eq!(
            gate_requirement_lines(&gates),
            vec![
                "gate build: requires nothing".to_string(),
                "gate sweep: requires sweeper at /home/u/.cargo/bin/sweeper, \
                 cargo-nextest at /usr/local/bin/cargo-nextest"
                    .to_string(),
            ]
        );
    }

    /// A workflow declaring no gate prints no gate line.
    #[test]
    fn gate_requirement_lines_of_no_gates_is_empty() {
        assert_eq!(gate_requirement_lines(&[]), Vec::<String>::new());
    }

    // --- Spec 71, criterion 3: `rigger validate` detects a stream whose position order and
    // revision order disagree (the signature left by a write that lands in a compaction-
    // opened revision hole). ---

    // NOTE: `order_signatures` itself (the pure running-max-revision detector) and its own
    // boundary tests - including the duplicate-revision case - now live in
    // `src/watch.rs`'s own test module: it moved there (spec 69 u69c2 consolidation) to be the
    // ONE shared implementation `rigger watch`'s store-integrity signal also calls, rather
    // than a second parallel reimplementation. Only the advisory FORMATTING below (unique to
    // `rigger validate`) still belongs to this file.

    /// The advisory names the stream, the out-of-order row count, the affected position
    /// range, and the doc location the repair procedure lives at - an operator reading
    /// `rigger validate`'s stderr has everything needed to find and fix it, without validate
    /// performing any repair itself (report-only, like every other validate advisory).
    #[test]
    fn order_signature_advisories_names_the_stream_count_range_and_repair_doc() {
        let signatures = vec![watch::OrderSignature {
            stream: "run".to_string(),
            rows: 2,
            first_position: 5,
            last_position: 6,
        }];
        let advisories = order_signature_advisories(&signatures);
        assert_eq!(advisories.len(), 1);
        let a = &advisories[0];
        assert!(a.starts_with("warning:"), "advisory: {a}");
        assert!(
            a.contains("run") && a.contains('2') && a.contains('5') && a.contains('6'),
            "advisory names the stream, count, and position range: {a}"
        );
        assert!(
            a.contains(watch::ORDER_SIGNATURE_REPAIR_DOC_REF),
            "advisory names the repair doc location: {a}"
        );
    }

    /// A clean log's empty signature list draws no advisories at all.
    #[test]
    fn order_signature_advisories_is_empty_when_no_signatures_are_given() {
        assert!(order_signature_advisories(&[]).is_empty());
    }

    // --- Spec 61, DRIFT SEVERITY: `model_drift_advisory` words a snapshot-only drift (every
    // change a same-base date-suffix bump) as a non-urgent advisory, and a drift carrying at
    // least one real model-base change as today's mandate-style warning. The `--if-model-
    // changed` gate's matching skip-vs-run behavior is pinned end to end in
    // `tests/cli.rs`. ---

    fn drift_change(alias: &str, previous: &str, current: &str) -> metrics::ModelChange {
        metrics::ModelChange {
            alias: alias.to_string(),
            previous: previous.to_string(),
            current: current.to_string(),
        }
    }

    /// A snapshot-only drift (same base, a newer dated build) reads as a low-urgency
    /// advisory: no `warning:` prefix, no "re-point" language, and it still names the
    /// changed tier and both ids so an operator can see exactly what moved.
    #[test]
    fn model_drift_advisory_is_a_soft_note_for_snapshot_only_drift() {
        let drift = metrics::ModelDrift {
            previous_run: Some("r1".to_string()),
            current_run: Some("r2".to_string()),
            changes: vec![drift_change(
                "lens",
                "claude-sonnet-4-5-20250929",
                "claude-sonnet-4-5-20260210",
            )],
        };
        let msg = model_drift_advisory(&drift).expect("snapshot drift still draws an advisory");
        assert!(
            !msg.starts_with("warning:"),
            "a snapshot bump is not worded as a warning: {msg}"
        );
        assert!(
            !msg.to_lowercase().contains("re-point"),
            "a same-model snapshot bump is not a re-point: {msg}"
        );
        assert!(
            msg.contains("lens")
                && msg.contains("claude-sonnet-4-5-20250929")
                && msg.contains("claude-sonnet-4-5-20260210"),
            "the advisory still names the tier and both ids: {msg}"
        );
        assert!(
            msg.contains("rigger canary --if-model-changed"),
            "the advisory still points at the drift-gated canary: {msg}"
        );
    }

    /// A drift with at least one real model-base change keeps today's mandate-style
    /// `warning:` wording naming a re-point, even when it also happens to change dates.
    #[test]
    fn model_drift_advisory_stays_a_warning_when_any_change_is_a_real_model_repoint() {
        let drift = metrics::ModelDrift {
            previous_run: Some("r1".to_string()),
            current_run: Some("r2".to_string()),
            changes: vec![drift_change("opus", "claude-opus-4-1", "claude-opus-4-8")],
        };
        let msg = model_drift_advisory(&drift).expect("a repoint draws an advisory");
        assert!(
            msg.starts_with("warning:") && msg.to_lowercase().contains("re-point"),
            "a real model repoint keeps the mandate-style warning: {msg}"
        );
    }

    /// No drift at all draws no advisory, snapshot-only or otherwise.
    #[test]
    fn model_drift_advisory_is_none_when_nothing_changed() {
        assert!(model_drift_advisory(&metrics::ModelDrift::default()).is_none());
    }

    // --- Spec 68, VALIDATE ADVISORIES: the two pure formatters (INDEX STALENESS, LOG BLOAT) ---

    #[test]
    fn index_staleness_message_names_every_kind_of_disagreement_and_the_fix() {
        let drift = rigger::grounder::symbols::IndexDrift {
            added: vec!["new.rs".to_string()],
            removed: vec!["gone.rs".to_string()],
            changed: vec!["edited.rs".to_string()],
        };
        let msg = index_staleness_message(&drift);
        assert!(msg.starts_with("warning:"), "advisory: {msg}");
        assert!(
            msg.contains('1'),
            "the message must carry the per-kind counts: {msg}"
        );
        assert!(
            msg.contains("rigger reindex"),
            "the message must name the fix: {msg}"
        );
    }

    // --- Spec 92 criterion 1, FRESH ON EVERY INTEGRATION: the GRAPH INDEX LAG advisory's pure
    // formatter (the sample itself is `rigger::ingest::graph_index_lag_sample`, tested beside its
    // own implementation) ---

    #[test]
    fn graph_index_lag_advisory_names_every_lagging_file_and_the_fix() {
        let lagging = vec!["src/a.rs".to_string(), "src/b.rs".to_string()];
        let msg = graph_index_lag_advisory(&lagging).expect("a non-empty sample draws an advisory");
        assert!(msg.starts_with("warning:"), "advisory: {msg}");
        assert!(
            msg.contains("src/a.rs") && msg.contains("src/b.rs"),
            "the message must name every lagging file: {msg}"
        );
        assert!(
            msg.contains("rigger reindex"),
            "the message must name the fix: {msg}"
        );
    }

    #[test]
    fn graph_index_lag_advisory_is_none_when_the_sample_is_empty() {
        assert_eq!(graph_index_lag_advisory(&[]), None);
    }

    /// The advisory reads the log's side in ONE typed read of the perception types from the
    /// stream's start and no whole-stream read (spec 107): over a log holding a run's start, a
    /// stale ledger entry of a file the tree holds and an entry of a file it does not, it
    /// materializes the two entries alone, and with no graph to ask names the stale file where
    /// an extraction is compiled and nothing where none is.
    #[test]
    fn the_index_lag_advisory_reads_the_log_in_one_typed_read_of_the_perception_types() {
        use crate::test_support::{generation_ingested, CountedRead, ReadCountingStore};

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("kept.rs"), "fn kept() {}\n").unwrap();
        let inner = Store::open(":memory:").unwrap();
        inner
            .append(
                conductor::STREAM,
                ExpectedRevision::Any,
                &[
                    Event::new("RunStarted", b"{}".to_vec()),
                    generation_ingested("gc", "kept.rs", "stale", "", false).event(1),
                    generation_ingested("gc", "gone.rs", "stale", "", false).event(1),
                ],
            )
            .unwrap();
        let store = ReadCountingStore::new(&inner);

        let lagging = read_graph_index_lag(&store, None, dir.path()).unwrap();

        let sampled: &[&str] = if cfg!(feature = "symbols") {
            &["kept.rs"]
        } else {
            &[]
        };
        assert_eq!(lagging, sampled);
        assert_eq!(
            store.reads(),
            [CountedRead::Typed {
                stream: conductor::STREAM.to_string(),
                from: 0,
                only: true,
                types: rigger::retention::PERCEPTION_TYPES
                    .map(String::from)
                    .to_vec(),
                materialized: 2,
            }]
        );
    }

    #[test]
    fn bloat_advisory_is_none_at_or_below_the_threshold_and_named_above_it() {
        // Exactly at the threshold: not yet a warning-worthy signal.
        let at_threshold = rigger::eventstore::sqlite::DerivedDuplication {
            rows: 3,
            kept: 2, // factor 1.5 == BLOAT_DUPLICATION_THRESHOLD
        };
        assert_eq!(bloat_advisory(&at_threshold), None);

        // Clearly above: a named warning carrying the measured factor and the fix.
        let above_threshold = rigger::eventstore::sqlite::DerivedDuplication {
            rows: 6,
            kept: 1, // factor 6.0
        };
        let advisory = bloat_advisory(&above_threshold).expect("must warn above threshold");
        assert!(advisory.starts_with("warning:"), "advisory: {advisory}");
        assert!(
            advisory.contains("6.0"),
            "the message must carry the measured factor: {advisory}"
        );
        assert!(
            advisory.contains("rigger reset --derived"),
            "the message must name the fix: {advisory}"
        );
    }

    /// Shared assertion for every `<x>_advisory_for` read-only gatherer's forbidden-side-effect
    /// contract: with NO store file at `<tempdir>/.rigger/<filename>` at all, `advisory_for` must
    /// return `None` BEFORE opening anything (opening would create the file - a read-only
    /// advisory's forbidden side effect), and the file must genuinely stay absent afterwards.
    /// `bloat_advisory_for` and `retired_entities_advisory_for` (spec 86 criterion 3) share this
    /// exact contract and this exact test shape - one helper, never two near-identical bodies.
    fn assert_advisory_for_never_fabricates_a_missing_store(
        filename: &str,
        advisory_for: impl Fn(&str, &str) -> Option<String>,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".rigger").join(filename);
        assert_eq!(
            advisory_for(path.to_str().unwrap(), "proj"),
            None,
            "no store yet is not evidence of anything to report"
        );
        assert!(
            !path.exists(),
            "a read-only advisory must never create the store it found absent"
        );
    }

    #[test]
    fn bloat_advisory_for_never_fabricates_a_store_that_does_not_exist() {
        assert_advisory_for_never_fabricates_a_missing_store("events.db", bloat_advisory_for);
    }

    // --- Spec 86 criterion 3, THE MIGRATION IS DELIBERATE: the RETIRED CODE-ENTITY advisory ---

    #[test]
    fn retired_entities_advisory_is_none_at_zero_and_named_with_correct_pluralization() {
        assert_eq!(
            retired_entities_advisory(0),
            None,
            "nothing retired is not worth an operator's attention"
        );
        let one = retired_entities_advisory(1).expect("a count of one must still be reported");
        assert!(one.contains('1'), "advisory: {one}");
        assert!(
            one.contains("entity") && !one.contains("entities"),
            "singular wording for exactly one: {one}"
        );
        let many = retired_entities_advisory(3).expect("a count above one must be reported");
        assert!(many.contains('3'), "advisory: {many}");
        assert!(
            many.contains("entities"),
            "plural wording for more than one: {many}"
        );
    }

    #[test]
    fn retired_entities_advisory_for_never_fabricates_a_graph_that_does_not_exist() {
        assert_advisory_for_never_fabricates_a_missing_store(
            "graph.db",
            retired_entities_advisory_for,
        );
    }

    #[test]
    fn retired_entities_advisory_for_reads_the_projectors_own_counting_authority() {
        // An end-to-end check that the advisory's gathering half is wired to the SAME counting
        // authority the fold-level migration tests exercise directly
        // (`contextgraph::sqlite::migration_c3`), never a second, shadow count.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".rigger").join("graph.db");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        {
            let p = Projector::open(path.to_str().unwrap(), "proj").unwrap();
            // A legacy test-entity node, then the empty structural boundary that retires it -
            // exactly the migration's own fold-level shape.
            let def = serde_json::json!({
                "file": "tests/integration.rs", "name": "an_integration_test", "kind": "function",
                "line": 2, "lang": "rust", "fresh": true,
            });
            let mut e = Event::new(
                contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
                serde_json::to_vec(&def).unwrap(),
            );
            e.position = 1;
            crate::test_support::folds(&p, std::slice::from_ref(&e));
            let boundary = serde_json::json!({
                "file": "tests/integration.rs", "name": "", "lang": "rust", "fresh": true,
            });
            let mut e = Event::new(
                contextgraph::TYPE_EDGE_INFERRED,
                serde_json::to_vec(&boundary).unwrap(),
            );
            e.position = 2;
            crate::test_support::folds(&p, std::slice::from_ref(&e));
        }
        let advisory = retired_entities_advisory_for(path.to_str().unwrap(), "proj")
            .expect("the migration retired exactly one entity, so the advisory must fire");
        assert!(advisory.contains('1'), "advisory: {advisory}");
    }
}
