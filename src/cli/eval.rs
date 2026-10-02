use super::*;

/// Render one corpus item's completion for `rigger canary`'s live per-item progress (spec
/// 61, PROGRESS): the item id, whether the adjudicator's verdict matched the expectation,
/// which tier(s) actually caught it (`none` when nobody did), and how long scoring this ONE
/// item took. Pure so its exact content is unit-tested without a live run; the caller
/// ([`cmd_canary`], wired through [`canary_store::run_canary`]'s `on_item` hook) prints it AS EACH
/// ITEM COMPLETES, never batched into the `format_canary_stats` scorecard rendered after
/// every item is done. Reuses the crate's ONE duration-format authority ([`fmt_duration`])
/// rather than a second spelling.
fn format_progress_line(o: &canary::CanaryOutcome, elapsed: std::time::Duration) -> String {
    let verdict = if o.verdict_correct {
        "correct"
    } else {
        "WRONG"
    };
    let caught = if o.caught_by.is_empty() {
        "none".to_string()
    } else {
        o.caught_by.join(",")
    };
    format!(
        "canary: {} verdict {verdict} caught {caught} in {}",
        o.id,
        fmt_duration(elapsed),
    )
}

/// The parsed flags for `rigger canary`: which corpus to score, whether the run is
/// gated on model drift, and the spawn-concurrency budget (spec 61, ITEM SHARDING AND
/// THE JOBS CAP).
struct CanaryArgs {
    corpus_dir: String,
    if_model_changed: bool,
    /// `--jobs <n>`: the TOTAL concurrent-spawn budget [`canary_store::run_canary`] divides
    /// between its two concurrency dimensions - independent corpus items sharded across
    /// workers, and each item's tier-1 lens fan-out (the LENS FAN-OUT criterion's
    /// already-built inner concurrency) - so neither dimension can push the live review
    /// panel's total concurrent spawns past what the operator asked for. Defaults to
    /// [`canary_store::default_jobs`] (always greater than one).
    jobs: usize,
    /// `--model <tier>=<id>` pins (repeatable; tiers: `lens`, `adversary`, `adjudicator`,
    /// spec 61 MODEL PINNING criterion): forces the named tier's agent(s) to `<id>` for
    /// THIS run only. Empty when the operator names none.
    model_pins: canary_store::ModelPins,
}

/// Parse `rigger canary`'s flags: `--corpus <dir>`, `--if-model-changed`, `--jobs <n>`, and
/// `--model <tier>=<id>` (spec 61). Unknown flags are rejected, mirroring
/// [`parse_run_args`]'s discipline.
fn parse_canary_args(args: &[String]) -> Result<CanaryArgs, Box<dyn std::error::Error>> {
    let mut corpus_dir = "canaries".to_string();
    let mut if_model_changed = false;
    let mut jobs = canary_store::default_jobs();
    let mut model_pins: canary_store::ModelPins = canary_store::ModelPins::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--corpus" => {
                corpus_dir = args
                    .get(i + 1)
                    .ok_or("canary: --corpus needs a directory path")?
                    .clone();
                i += 2;
            }
            "--if-model-changed" => {
                if_model_changed = true;
                i += 1;
            }
            "--jobs" => {
                i += 1;
                let raw = args.get(i).cloned().unwrap_or_default();
                jobs = raw
                    .parse::<usize>()
                    .ok()
                    .filter(|n| *n > 0)
                    .ok_or_else(|| {
                        format!("canary: --jobs expects a positive integer, got {raw:?}")
                    })?;
                i += 1;
            }
            "--model" => {
                let raw = args.get(i + 1).ok_or(
                    "canary: --model needs <tier>=<id> (tiers: lens, adversary, adjudicator)",
                )?;
                let (tier, id) = raw.split_once('=').ok_or_else(|| {
                    format!(
                        "canary: --model {raw:?} is not <tier>=<id> (tiers: lens, adversary, \
                         adjudicator)"
                    )
                })?;
                if !matches!(
                    tier,
                    canary::TIER_LENS | canary::TIER_ADVERSARY | spawn::ROLE_ADJUDICATOR
                ) {
                    return Err(format!(
                        "canary: --model names unknown tier {tier:?} (tiers: lens, adversary, \
                         adjudicator)"
                    )
                    .into());
                }
                if id.is_empty() {
                    return Err(format!("canary: --model {raw:?} names an empty model id").into());
                }
                model_pins.insert(tier.to_string(), id.to_string());
                i += 2;
            }
            other => {
                return Err(format!(
                    "canary: unexpected argument {other:?} (usage: rigger canary [--corpus <dir>] \
                     [--if-model-changed] [--jobs <n>] [--model <tier>=<id> ...])"
                )
                .into())
            }
        }
    }
    Ok(CanaryArgs {
        corpus_dir,
        if_model_changed,
        jobs,
        model_pins,
    })
}

/// `rigger canary [--corpus <dir>] [--if-model-changed] [--jobs <n>]` (spec 13, unit 5;
/// drift trigger spec 13b, unit 1; concurrency spec 61): run the review panel against every
/// item in the seeded-defect corpus (default `./canaries`) and record the scored outcomes
/// to the project's canary stream, then print the scorecard. This is the loop's only RECALL
/// measurement - it judges the judges against known ground truth. The scores land in a
/// DISTINCT stream from the run's, so a canary run never perturbs the project's operator
/// metrics; `rigger stats --canary` re-reports them.
///
/// With `--if-model-changed` the run is GATED on model drift: the canary runs ONLY when a
/// tier's resolved model id re-pointed since the previous run (the same drift `rigger
/// validate` warns about), and an unchanged model runs no canary - the automatic monitor for
/// silent alias re-points. Without the flag the canary always runs.
///
/// `--jobs <n>` bounds the total number of review-panel spawns in flight at once, across
/// BOTH independent corpus items and each item's tier-1 lens fan-out together (spec 61,
/// ITEM SHARDING AND THE JOBS CAP); it defaults to [`canary_store::default_jobs`].
pub(crate) fn cmd_canary(args: &[String]) -> Res {
    let args = parse_canary_args(args)?;
    let corpus_dir = args.corpus_dir;
    let if_model_changed = args.if_model_changed;

    // The drift gate (spec 13b, unit 1): with `--if-model-changed`, run the canary ONLY when a
    // tier's resolved model re-pointed since the previous run; an unchanged model runs no
    // canary (and needs no corpus, so the gate precedes the corpus load). The detection reads
    // the SAME namespaced run stream `rigger validate`'s drift advisory folds, so the warning
    // and this trigger can never disagree on what "the model changed" means.
    if if_model_changed {
        let drift = read_model_drift(&db_path("events.db"), &project_identity())?;
        if !drift.changed() {
            println!(
                "canary: no resolved-model change since the previous run - skipping (run \
                 `rigger canary` to force a run)."
            );
            return Ok(());
        }
        // DRIFT SEVERITY (spec 61, c11): a same-base date-suffix bump is a snapshot refresh,
        // not a model re-point - report it and skip the multi-hour panel. A real base change
        // among the tiers still opens the gate below; an operator who wants the measurement
        // anyway just drops the flag (`rigger canary` with no `--if-model-changed`).
        if drift.snapshot_only() {
            for c in &drift.changes {
                let alias = if c.alias.is_empty() {
                    "(unnamed tier)"
                } else {
                    c.alias.as_str()
                };
                println!(
                    "canary: {alias} resolved to a newer snapshot ({} -> {}) since the previous \
                     run - same model, skipping the panel (run `rigger canary` without \
                     --if-model-changed to measure anyway).",
                    c.previous, c.current,
                );
            }
            return Ok(());
        }
        for c in &drift.changes {
            let alias = if c.alias.is_empty() {
                "(unnamed tier)"
            } else {
                c.alias.as_str()
            };
            println!(
                "canary: resolved model changed for {alias} ({} -> {}) since the previous run - \
                 running the panel.",
                c.previous, c.current,
            );
        }
    }

    let corpus = canary_store::load_corpus(Path::new(&corpus_dir))?;
    if corpus.is_empty() {
        return Err(format!(
            "canary: the corpus at {corpus_dir:?} has no items (add `*.md` canary files)"
        )
        .into());
    }

    let cfg = config_store::load(".")?;
    let panel = cfg.workflow.defaults.review.clone();
    if panel.is_empty() {
        return Err("canary: defaults.review declares no review panel to measure".into());
    }
    // MODEL PINNING criterion (spec 61 c7): resolve `--model <tier>=<id>` pins against a
    // CLONE of `cfg` - the loaded config (and the file it came from) is never mutated, and
    // an un-pinned tier's agent is byte-for-byte what the config declared.
    let cfg = canary_store::apply_model_pins(&cfg, &panel, &args.model_pins);

    std::fs::create_dir_all(RIGGER_DIR)?;
    // Sqlite is the canary's local measurement store; migrate a pre-spec-09 namespace once
    // so the canary stream lands under the same identity `stats --canary` reads.
    let selection = store_selection(None, None)?;
    if selection.is_sqlite() {
        migrate_local_identity()?;
    }
    let backend = resolve_store(&selection, &db_path("events.db"))?;
    let store = Namespaced::new(backend.as_ref(), &project_identity());
    let driver = cli::Driver::default();

    // Observable progress (spec 61, PROGRESS): stream one line per corpus item the
    // instant it finishes, rather than nothing on stdout until the whole batch is done -
    // wired to run_canary's on_item hook, which fires per item as it genuinely
    // completes (see canary_store::run_canary's doc comment for why that hook lives inside
    // the item-sharding closure, not the aggregation loop after it).
    let report = canary_store::run_canary(
        &store,
        &driver,
        &cfg,
        &panel,
        &corpus,
        args.jobs,
        &|o, e| {
            println!("{}", format_progress_line(o, e));
        },
    )?;
    println!(
        "canary run {}: scored {} corpus item(s) against the review panel",
        report.batch,
        report.outcomes.len(),
    );
    // MODEL PINNING criterion: record the run-level header (binary build, corpus hash,
    // every tier's actually-resolved model id) AFTER scoring, once every item's
    // resolved-model observations are known - never mutating the opening batch marker
    // `run_canary` already wrote.
    canary_store::record_header(
        &store,
        &report.batch,
        &canary::CanaryHeader {
            binary_build: version_line(),
            corpus_hash: canary_store::corpus_hash(&corpus),
            resolved_models: report.resolved_models,
        },
    )?;
    // Re-read and fold from the store so the printed scorecard is exactly what
    // `rigger stats --canary` will report from the same events.
    let events = store.read_stream(canary::STREAM, 0, Direction::Forward)?;
    for line in format_canary_stats(&metrics::project_canary(&events)) {
        println!("{line}");
    }
    Ok(())
}

/// `rigger playbooks --rebuild` (spec 13b, unit 2) - reconstruct the distilled playbook pool
/// under `.rigger/playbooks/` from this project's recorded `LessonLearned` stream. The pool is
/// a rebuildable PROJECTION of the log (never hand-edited state): [`playbooks::rebuild`] clears
/// the rigger-managed pool files and re-derives every deduplicated, trigger-scoped playbook, so
/// this command is the operator's way to regenerate the pool after new lessons land (or to
/// recover a hand-corrupted pool). It only READS the run stream (never writes it), scoped to
/// this project's namespace exactly as `rigger stats`/`rigger canary` read it; an absent store
/// (a never-run project) has no lessons, so the pool rebuilds empty rather than fabricating one.
pub(crate) fn cmd_playbooks(args: &[String]) -> Res {
    match args {
        [flag] if flag == "--rebuild" => {}
        _ => {
            return Err("playbooks: expected --rebuild (usage: rigger playbooks --rebuild)".into())
        }
    }

    // Migrate a pre-spec-09 namespace once so the lessons stream lands under the same
    // identity the conductor wrote, then READ (never fabricate) this project's run stream.
    let selection = store_selection(None, None)?;
    if selection.is_sqlite() {
        migrate_local_identity()?;
    }
    let db = db_path("events.db");
    let events = if !selection.is_sqlite() || Path::new(&db).exists() {
        let backend = resolve_store(&selection, &db)?;
        let store = Namespaced::new(backend.as_ref(), &project_identity());
        store.read_stream(conductor::STREAM, 0, Direction::Forward)?
    } else {
        Vec::new()
    };

    let pool_dir = Path::new(RIGGER_DIR).join(playbooks::POOL_SUBDIR);
    let pool = playbooks::rebuild(&events, &pool_dir)?;
    let lessons = events
        .iter()
        .filter(|e| e.type_ == contextgraph::TYPE_LESSON_LEARNED)
        .count();
    println!(
        "playbooks: rebuilt {} playbook(s) under {} from {} recorded lesson event(s)",
        pool.len(),
        pool_dir.display(),
        lessons,
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_canary_args_defaults_corpus_and_jobs() {
        let a = parse_canary_args(&[]).unwrap();
        assert_eq!(a.corpus_dir, "canaries");
        assert!(
            !a.if_model_changed,
            "--if-model-changed is off unless asked"
        );
        assert_eq!(
            a.jobs,
            canary_store::default_jobs(),
            "an unflagged run uses the production default jobs budget"
        );
    }

    #[test]
    fn parse_canary_args_reads_corpus_if_model_changed_and_jobs() {
        let args = [
            "--corpus".to_string(),
            "my-canaries".to_string(),
            "--if-model-changed".to_string(),
            "--jobs".to_string(),
            "4".to_string(),
        ];
        let a = parse_canary_args(&args).unwrap();
        assert_eq!(a.corpus_dir, "my-canaries");
        assert!(a.if_model_changed);
        assert_eq!(a.jobs, 4);
    }

    #[test]
    fn parse_canary_args_rejects_a_non_positive_jobs_value() {
        for bad in ["0", "-1", "not-a-number", ""] {
            let args = ["--jobs".to_string(), bad.to_string()];
            let err = match parse_canary_args(&args) {
                Ok(_) => panic!("--jobs {bad:?} must be rejected"),
                Err(e) => e.to_string(),
            };
            assert!(
                err.contains("--jobs"),
                "the error must name --jobs for input {bad:?}: {err:?}"
            );
        }
    }

    #[test]
    fn parse_canary_args_rejects_unknown_flags() {
        let args = ["--bogus".to_string()];
        let err = match parse_canary_args(&args) {
            Ok(_) => panic!("--bogus must be rejected"),
            Err(e) => e.to_string(),
        };
        assert!(
            err.contains("--bogus"),
            "the error names the bad flag: {err:?}"
        );
    }

    /// A minimal `CanaryOutcome` for [`format_progress_line`] tests - only the three
    /// fields that function reads (`id`, `verdict_correct`, `caught_by`) vary per case;
    /// every other field is a fixed, irrelevant placeholder.
    fn progress_outcome(
        id: &str,
        verdict_correct: bool,
        caught_by: &[&str],
    ) -> canary::CanaryOutcome {
        canary::CanaryOutcome {
            id: id.to_string(),
            defect_class: String::new(),
            planted: true,
            expected_reject: true,
            expected_tier: String::new(),
            caught_by: caught_by.iter().map(|s| s.to_string()).collect(),
            verdict_approved: false,
            verdict_correct,
            stable: true,
            findings_raised: std::collections::BTreeMap::new(),
        }
    }

    /// spec 61, PROGRESS criterion: the per-item stdout line names the item id, whether
    /// the adjudicator's verdict was correct, and which tier(s) caught it - `none` when
    /// nobody did, since an empty `caught_by` (a miss, or a control) must read as an
    /// explicit "nothing caught this", never a blank/missing field.
    #[test]
    fn format_progress_line_names_id_verdict_and_none_when_nothing_caught() {
        let o = progress_outcome("item-1", true, &[]);
        let line = format_progress_line(&o, std::time::Duration::from_millis(500));
        assert!(line.contains("item-1"), "the item id must appear:\n{line}");
        assert!(
            line.contains("correct"),
            "a correct verdict must render as such:\n{line}"
        );
        assert!(
            line.contains("none"),
            "an empty caught_by must render as an explicit none, not blank:\n{line}"
        );
    }

    /// A wrong verdict and multiple catching tiers both render honestly, and the elapsed
    /// duration reuses the crate's ONE duration-format authority (`fmt_duration`) rather
    /// than a second, possibly-diverging spelling.
    #[test]
    fn format_progress_line_reports_a_wrong_verdict_and_every_catching_tier() {
        let o = progress_outcome("item-2", false, &["lens", "adversary"]);
        let elapsed = std::time::Duration::from_secs_f64(12.3);
        let line = format_progress_line(&o, elapsed);
        assert!(
            line.contains("WRONG"),
            "an incorrect verdict must be visibly flagged, not read as success:\n{line}"
        );
        assert!(
            line.contains("lens") && line.contains("adversary"),
            "every catching tier must be named, not just the first:\n{line}"
        );
        assert!(
            line.contains(&fmt_duration(elapsed)),
            "elapsed must render through fmt_duration, the one duration-format authority, \
             not a second spelling:\n{line}"
        );
    }
}
