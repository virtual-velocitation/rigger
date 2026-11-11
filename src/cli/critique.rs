//! `rigger critique <spec>` (spec 112, criterion 1): the workflow's plan-critique adversary
//! critiques a spec before any run, as a headless agent-host session, and each pass is recorded
//! keyed on the spec's content hash - on the critique's own stream, never the run stream `rigger
//! run`'s conductor folds. The critique passes to exhaustion (spec 123): the critic is spawned
//! again, the recorded findings in its prompt, until a pass carries no finding or the critic's
//! ceiling is reached; so unchanged text whose critique is exhausted, or at its ceiling with a pass
//! recorded, is answered from the store with no spawn, by the union of its passes and the verdict
//! that union derives, and any edit is critiqued afresh.

use super::*;

use std::collections::HashSet;

use rigger::blocker::RetriesSource;
use rigger::conductor::{AgentDriver, SpawnHost, SpawnOpts};
use rigger::driver::{claude_code, replay};
use rigger::eventstore::TypeSelection;
use rigger::liveness;
use rigger::review::{self, Critique};
use rigger::wave::Critic;

/// The command, as its refusals and notices name it.
const COMMAND: &str = "rigger critique";

/// The stage a critique spawn is recorded under.
const CRITIQUE_STAGE: &str = "critique";

/// The critique's own store: the project's namespace plus `-critique`, so a critique's spawn
/// request and result land on a stream of their own and never on the run stream `rigger run`'s
/// conductor folds. Every critique read and write goes through it - the one place that
/// spells the suffix.
pub(crate) fn critique_store<'a>(backend: &'a dyn EventStore, identity: &str) -> Namespaced<'a> {
    Namespaced::new(backend, &format!("{identity}-critique"))
}

/// `rigger critique`'s arguments: the spec path, and the `--eventstore` / `--conn` flags `rigger
/// run` takes.
struct CritiqueArgs {
    spec: String,
    store: Option<StoreKind>,
    conn: Option<String>,
}

fn parse_critique_args(args: &[String]) -> Result<CritiqueArgs, Box<dyn std::error::Error>> {
    let mut spec = None;
    let mut store = None;
    let mut conn = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--eventstore" => store = Some(eventstore_flag(it.next(), "critique")?),
            "--conn" => conn = Some(conn_flag(it.next(), "critique")?),
            other => spec_positional(other, &mut spec, "critique")?,
        }
    }
    let spec = spec.ok_or(
        "critique: expected a spec path: rigger critique <spec> [--eventstore \
         sqlite|kurrentdb] [--conn <url>]",
    )?;
    Ok(CritiqueArgs { spec, store, conn })
}

/// `rigger critique <spec> [--eventstore sqlite|kurrentdb] [--conn <url>]`.
///
/// Its refusals run in one order and only the first one reached is printed: a linked worktree,
/// a workflow with no critic (whether or not the text was critiqued before), a root mismatch,
/// a spec outside the repository, a spec with no Done-when criteria; then it selects, migrates
/// and opens the store and opens the graph (a graph that owes its rebuild refuses here, naming
/// `rigger setup`). Then it reads the hash's recorded union and attempt count:
///
/// - exhausted (a pass carries no finding), it spawns nothing and answers from the record of every
///   pass, saying over how many ([`answered_from`]);
/// - at the critic's ceiling ([`Critic::reached`]) with a pass recorded, it answers the same way,
///   then prints the CEILING NOTICE ([`Ceiling::notice`]);
/// - at the ceiling with no pass recorded, it is not answered: the notice is its error;
/// - otherwise it copies the recorded union's findings, then spawns the critic pass after pass,
///   the union's findings in each prompt, each spawn judged (one that records no pass fails the
///   call) and the union's findings copied at once, until the re-read union is exhausted or the
///   re-read attempt count is at the ceiling, when it prints the notice.
///
/// An answered call appends any missing `ReviewFinding` copy of the union's findings, prints the
/// union and the verdict it derives ([`answer`]), and exits 0 whatever the findings say.
pub(crate) fn cmd_critique(args: &[String]) -> Res {
    let parsed = parse_critique_args(args)?;
    let cwd = cwd();
    let repo = resolve_main_worktree_or_refuse(&cwd, COMMAND)?;
    let cfg = config_store::load(".")?;
    let critic = rigger::wave::critic_of(&cfg.workflow).ok_or_else(|| {
        format!(
            "{COMMAND}: refusing to critique {}: {}",
            parsed.spec,
            rigger::wave::NO_CRITIC_CLAUSE
        )
    })?;
    let scratch = project_scratch_root(&repo, &cfg);
    refuse_unless_one_root(&cwd, &repo, scratch.as_deref(), COMMAND)?;
    let scratch_root = scratch.unwrap_or_default();
    let root = review::spec_root(&cwd, &repo);
    let spec = review::normalize_spec_path(&root, &parsed.spec).ok_or_else(|| {
        format!(
            "{COMMAND}: refusing to critique {}: it is outside the repository {} - move the \
             spec into it",
            parsed.spec,
            root.display()
        )
    })?;
    let (_, text) = load_criteria(Some(&spec))?;
    let hash = review::critique_hash(&text);

    std::fs::create_dir_all(RIGGER_DIR)?;
    let selection = store_selection(parsed.store, parsed.conn.as_deref())?;
    if selection.is_sqlite() {
        migrate_local_identity(None)?;
    }
    let backend = resolve_store(&selection, &db_path("events.db"))?;
    let identity = project_identity();
    let store = Namespaced::new(backend.as_ref(), &identity);
    let graph = open_graph(&db_path("graph.db"), &identity, &selection, "critique")?;
    let critiques = critique_store(backend.as_ref(), &identity);
    let ceiling = Ceiling {
        spec: &spec,
        hash: &hash,
        critic: &critic,
        project: &store,
        root: &root,
    };

    let union = recorded_union(&critiques, &hash)?;
    let attempts = review::next_critique_attempt(&critiques, &hash)?;
    let critique = if !critic.settles(&union, attempts) {
        // Every recorded pass is copied before any spawn can fail; then pass after pass, each
        // judged and copied at once, until the re-read union and its re-read attempt count settle
        // the critique.
        copy_findings(&store, &graph, &union, &spec)?;
        let host = CriticHost {
            cfg: &cfg,
            critic: &critic,
            critiques: &critiques,
            store_env: selection.handed_env(),
            identity: &identity,
            scratch_root: &scratch_root,
            root: &root,
            spawn_host: rigger::hooks::resolve_spawn_host(&cfg.workflow.defaults, &root)?,
        };
        let mut union = union;
        loop {
            host.critique(&spec, &text, &hash, &union)?;
            union = recorded_union(&critiques, &hash)?;
            copy_findings(&store, &graph, &union, &spec)?;
            let attempts = review::next_critique_attempt(&critiques, &hash)?;
            if critic.settles(&union, attempts) {
                ceiling.say_unless_exhausted(&union, attempts)?;
                break union;
            }
        }
    } else if union.passes.is_empty() {
        // At the ceiling with no pass, which is never exhausted: not answered, and its notice is
        // the error.
        return Err(ceiling.notice(&union, attempts)?.into());
    } else {
        remove_critique_scratch(&scratch_root);
        answered_from(&spec, &hash, &union);
        ceiling.say_unless_exhausted(&union, attempts)?;
        copy_findings(&store, &graph, &union, &spec)?;
        union
    };
    answer(&critique);
    Ok(())
}

/// The head of every line `rigger critique` writes about `spec`'s text of hash `hash`.
fn head(spec: &str, hash: &str) -> String {
    format!("{COMMAND}: {spec} (hash {hash}): ")
}

/// The union of every recorded pass of `hash`'s critique, with no pass when none is recorded.
fn recorded_union(
    critiques: &dyn EventStore,
    hash: &str,
) -> Result<Critique, Box<dyn std::error::Error>> {
    Ok(review::read_critique(critiques, hash)?.unwrap_or(Critique { passes: Vec::new() }))
}

/// Say on stderr that `spec`'s critique is answered from the store, over how many passes of
/// `hash` it was recorded: `1 pass` or `<n> passes`.
fn answered_from(spec: &str, hash: &str, critique: &Critique) {
    let passes = match critique.passes.len() {
        1 => "1 pass".to_string(),
        n => format!("{n} passes"),
    };
    eprintln!(
        "{}answered from the critique recorded over {passes}",
        head(spec, hash)
    );
}

/// What one call's CEILING NOTICE (spec 123, THE CEILING) is composed from.
struct Ceiling<'a> {
    spec: &'a str,
    hash: &'a str,
    /// The critic, whose ceiling and its source bound the critique.
    critic: &'a Critic,
    /// The project store, whose run stream holds the `DecisionMade` events resolving a finding.
    project: &'a dyn EventStore,
    /// The root every `governs` entry is made relative to ([`review::spec_root`]).
    root: &'a Path,
}

impl Ceiling<'_> {
    /// Say the CEILING NOTICE ([`Ceiling::notice`]) on stderr for `union`, settled after
    /// `attempts` attempts, unless it is exhausted: a settled critique that is not exhausted
    /// settled at its ceiling.
    fn say_unless_exhausted(
        &self,
        union: &Critique,
        attempts: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if !union.exhausted() {
            eprintln!("{}", self.notice(union, attempts)?);
        }
        Ok(())
    }

    /// The CEILING NOTICE for `union` after `attempts` attempts: not exhausted, the bound the
    /// critic's ceiling came from, then the union's open BLOCKING findings
    /// ([`open_blocking_ids`]), or that none is open and the critique is clean at the ceiling, or
    /// that no pass is recorded.
    fn notice(
        &self,
        union: &Critique,
        attempts: u32,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let bound = match self.critic.source {
            RetriesSource::Stage => format!(
                "the ceiling of stages.{}.max_retries",
                self.critic.gate.as_deref().unwrap_or_default()
            ),
            RetriesSource::Defaults => "the ceiling of defaults.max_retries".to_string(),
            RetriesSource::BuiltIn => format!("the built-in bound of {}", self.critic.ceiling),
        };
        let close = if union.passes.is_empty() {
            "no pass recorded: raise the ceiling or edit the text".to_string()
        } else {
            let open = open_blocking_ids(self.project, union, self.spec, self.root)?;
            if open.is_empty() {
                "no open BLOCKING finding, the critique is clean at the ceiling".to_string()
            } else {
                format!(
                    "open BLOCKING findings: {}: amend the spec or record a DecisionMade \
                     resolving them",
                    open.join(", ")
                )
            }
        };
        Ok(format!(
            "{}not exhausted after {attempts} attempts, {bound}: {close}",
            head(self.spec, self.hash)
        ))
    }
}

/// The open BLOCKING findings of `critique`, the critique of `spec` (repo-relative to `root`):
/// its union's BLOCKING ids minus those a `DecisionMade` on the `project` run stream resolves
/// ([`review::open_findings`]).
pub(crate) fn open_blocking_ids(
    project: &dyn EventStore,
    critique: &Critique,
    spec: &str,
    root: &Path,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let decisions = project.read_stream_typed(
        conductor::STREAM,
        0,
        TypeSelection::Only(&[contextgraph::TYPE_DECISION_MADE]),
    )?;
    Ok(review::open_findings(critique, &decisions, spec, root))
}

/// Print the union of `critique`'s findings, in pass order, one `<id> | <summary>` line each,
/// then the verdict the union derives as one JSON line.
fn answer(critique: &Critique) {
    for finding in critique.findings() {
        println!("{} | {}", finding.id, finding.summary());
    }
    println!("{}", serde_json::json!({ "verdict": critique.verdict() }));
}

/// What a critique spawn is composed from.
struct CriticHost<'a> {
    cfg: &'a config::Config,
    /// The critic the workflow names ([`rigger::wave::critic_of`]): its persona is spawned.
    critic: &'a Critic,
    /// The critique's own store ([`critique_store`]).
    critiques: &'a dyn EventStore,
    /// The verb's store selection, handed to the critic so its bound `rigger mcp --spawn` server
    /// resolves the store the critique is recorded to ([`StoreSelection::handed_env`]).
    store_env: Vec<(String, String)>,
    identity: &'a str,
    /// The project scratch root; empty in a project with no git repository.
    scratch_root: &'a str,
    /// The root the spec path is relative to: the critic's working directory.
    root: &'a Path,
    /// The host values every pass's spawn carries, resolved once from the workflow's defaults
    /// and the helper copies at `root` when the critique starts
    /// ([`rigger::hooks::resolve_spawn_host`]).
    spawn_host: SpawnHost,
}

impl CriticHost<'_> {
    /// Park and spawn the critic at the hash's next attempt through the headless host, on the
    /// spec text with `union`'s findings recorded in the prompt
    /// ([`review::CRITIQUE_RECORDED_FINDINGS`]), then judge that spawn by its own result the store
    /// holds for the hash ([`review::read_spawned_critique`]) - never the session's own output nor
    /// another attempt's - and reap what the spawn left under the scratch root ([`reap_spawn`]),
    /// pass or not. A recorded result that is not a pass is an error saying why.
    fn critique(&self, spec: &str, text: &str, hash: &str, union: &Critique) -> Res {
        let persona = self.cfg.agents.get(&self.critic.persona).ok_or_else(|| {
            format!(
                "{COMMAND}: the critic {:?} the workflow names has no definition under \
                 .rigger/agents",
                self.critic.persona
            )
        })?;
        // The critic reads and looks things up; it can neither build nor record: its tools are
        // replaced, and every tool that builds, records or edits is denied whatever the
        // checkout's own settings allow.
        let critic = config::AgentDef {
            tools: review::CRITIC_TOOLS.map(String::from).to_vec(),
            ..persona.clone()
        };
        let attempt = review::next_critique_attempt(self.critiques, hash)?;
        let unit = review::critique_unit(hash);
        let prompt = review::spec_critique_prompt(spec, text, union.findings());
        let opts = SpawnOpts {
            id: review::critique_spawn_id(hash, attempt),
            unit: unit.clone(),
            stage: CRITIQUE_STAGE.to_string(),
            attempt,
            system_prompt: rigger::instructions::compose(&critic.prompt, &self.cfg.instructions),
            dir: self.root.to_string_lossy().into_owned(),
            run_id: unit,
            title: spec.to_string(),
            settings_json: serde_json::json!({
                "permissions": { "deny": review::CRITIC_DENIED_TOOLS }
            })
            .to_string(),
            ..SpawnOpts::new(self.spawn_host.clone())
        };
        spawn_store::park_in_run(
            self.critiques,
            &conductor::spawn_request(&critic, &prompt, &opts),
            &opts.run_id,
            self.scratch_root,
        )?;
        let progress_backend = open_progress_backend()?;
        let progress = Namespaced::new(&progress_backend, self.identity);
        let host = claude_code::Driver {
            bin: String::new(),
            rigger_bin: running_executable(),
            progress_store: &progress,
            run_store: self.critiques,
            scratch_root: self.scratch_root.to_string(),
            stop_grace: claude_code::STOP_GRACE,
            handed_env: self.store_env.clone(),
            beat_source: None,
            hold_sink: None,
        };
        eprintln!(
            "{}critiquing with {} at attempt {attempt}",
            head(spec, hash),
            self.critic.persona
        );
        let spawned = host.spawn(&critic, &prompt, &opts, &|_, _| Ok(()));
        let judged = review::read_spawned_critique(self.critiques, hash, attempt);
        reap_spawn(self.scratch_root, &opts.run_id, &opts.id);
        judged?.map(|_| ()).map_err(|why| {
            let why = why
                .or_else(|| spawned.err().map(|e| e.to_string()))
                .unwrap_or_else(|| "the critic's session recorded no result".to_string());
            format!("{}no critique was recorded - {why}", head(spec, hash)).into()
        })
    }
}

/// Remove what the critique spawn `spawn_id` of run `run_id` left under `scratch_root`: its
/// liveness marker ([`rigger::liveness::marker_path`]), each of its transcripts
/// ([`claude_code::remove_transcripts`], the host naming its own files) and its spawn scratch
/// ([`replay::reclaim_spawn_registered_scratch`]), then each of those directories' run directory
/// when it is left empty. Nothing else is touched: another spawn's entries, and a run
/// directory that still holds them, stay. An empty scratch root reaps nothing. Best-effort: an
/// entry that cannot be removed stays.
fn reap_spawn(scratch_root: &str, run_id: &str, spawn_id: &str) {
    let marker = liveness::marker_path(scratch_root, run_id, spawn_id);
    if let Some(marker) = &marker {
        let _ = std::fs::remove_file(marker);
    }
    claude_code::remove_transcripts(scratch_root, run_id, spawn_id);
    let leaf = liveness::scratch_subpath(
        scratch_root,
        claude_code::AGENT_STREAM_SUBDIR,
        run_id,
        spawn_id,
    );
    replay::reclaim_spawn_registered_scratch(scratch_root, run_id, spawn_id);
    let scratch = replay::spawn_scratch_path(scratch_root, run_id, spawn_id);
    for path in [marker, leaf, scratch].iter().flatten() {
        if let Some(run_dir) = path.parent() {
            let _ = std::fs::remove_dir(run_dir);
        }
    }
}

/// Remove the liveness, transcript and spawn-scratch directories of every critique run
/// (`critique-<hash>`, any hash) under the scratch root, on a call answered from the store, so
/// whatever a crashed critique left goes with it; the log holds the output. A spawning call never
/// runs it: it reaps only the spawns it judged ([`reap_spawn`]). A loop run's directories are
/// never touched. Each goes through the same entry-by-entry removal as a done run's teardown
/// ([`remove_agent_area_entries`]). Best-effort: a directory that cannot be read or removed stays
/// for the next call.
fn remove_critique_scratch(scratch_root: &str) {
    if scratch_root.is_empty() {
        return;
    }
    remove_agent_area_entries(Path::new(scratch_root), review::is_critique_run);
}

/// Append one `ReviewFinding` to the project run stream for each finding of `critique`'s union that
/// the stream does not already hold by id, folding them into the graph as they land, so `rigger
/// peers <spec>` shows them. Each copy passes the one emit guard first.
fn copy_findings(
    store: &dyn EventStore,
    graph: &dyn contextgraph::RecordFold,
    critique: &Critique,
    spec: &str,
) -> Res {
    let held: HashSet<String> = store
        .read_stream_typed(
            conductor::STREAM,
            0,
            TypeSelection::Only(&[contextgraph::TYPE_REVIEW_FINDING]),
        )?
        .iter()
        .filter_map(|event| event.decode::<serde_json::Value>())
        .filter_map(|finding| finding["id"].as_str().map(str::to_string))
        .collect();
    let mut copies = Vec::new();
    for finding in critique.findings().filter(|f| !held.contains(&f.id)) {
        let data = serde_json::to_vec(&review::finding_copy(finding, spec))?;
        let copy = Event::new(contextgraph::TYPE_REVIEW_FINDING, data);
        contextgraph::check_emit(store, conductor::STREAM, &copy)?;
        copies.push(copy);
    }
    if copies.is_empty() {
        return Ok(());
    }
    rigger::ingest::folding_into(store, Some(graph), &stderr_line).append(
        conductor::STREAM,
        ExpectedRevision::Any,
        &copies,
    )?;
    Ok(())
}
