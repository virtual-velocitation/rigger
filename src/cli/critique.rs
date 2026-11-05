//! `rigger critique <spec>` (spec 112, criterion 1): the workflow's plan-critique adversary
//! critiques a spec before any run, as a headless Claude Code session, and each pass is recorded
//! keyed on the spec's content hash - on the critique's own stream, never the run stream a step
//! folds into its wave - so unchanged text is answered from the store with no spawn, by the union
//! of its passes and the verdict that union derives (spec 123), and any edit is critiqued afresh.

use super::*;

use std::collections::HashSet;

use rigger::conductor::{AgentDriver, SpawnOpts};
use rigger::driver::claude_code;
use rigger::eventstore::TypeSelection;
use rigger::review::{self, Critique, CritiquePass};

/// The command, as its refusals and notices name it.
const COMMAND: &str = "rigger critique";

/// The stage a critique spawn is recorded under.
const CRITIQUE_STAGE: &str = "critique";

/// The critique's own store: the project's namespace plus `-critique`, so a critique's spawn
/// request and result land on a stream of their own and never on the run stream a `rigger step`
/// folds into its wave. Every critique read and write goes through it - the one place that
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
/// `rigger setup`). When the spec's hash already has a critique - one pass or more - it spawns
/// nothing and answers from the record of every pass, saying over how many ([`answered_from`]);
/// otherwise it parks and spawns the critic, then answers with the one pass it reads back from
/// the store. Either way it appends any missing `ReviewFinding` copy of the union's findings,
/// prints the union and the verdict it derives ([`answer`]), and exits 0 whatever the findings
/// say.
pub(crate) fn cmd_critique(args: &[String]) -> Res {
    let parsed = parse_critique_args(args)?;
    let cwd = cwd();
    let repo = resolve_main_worktree_or_refuse(&cwd, COMMAND)?;
    let cfg = config_store::load(".")?;
    let critic = rigger::wave::critic(&cfg.workflow).ok_or_else(|| {
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

    let critique = match review::read_critique(&critiques, &hash)? {
        Some(recorded) => {
            remove_critique_scratch(&scratch_root);
            answered_from(&spec, &hash, &recorded);
            recorded
        }
        None => {
            let host = CriticHost {
                cfg: &cfg,
                critic: &critic,
                critiques: &critiques,
                store_env: selection.handed_env(),
                identity: &identity,
                scratch_root: &scratch_root,
                root: &root,
            };
            Critique {
                passes: vec![host.critique(&spec, &text, &hash)?],
            }
        }
    };
    copy_findings(&store, &graph, &critique, &spec)?;
    answer(&critique);
    Ok(())
}

/// Say on stderr that `spec`'s critique is answered from the store, over how many passes of
/// `hash` it was recorded: `1 pass` or `<n> passes`.
fn answered_from(spec: &str, hash: &str, critique: &Critique) {
    let passes = match critique.passes.len() {
        1 => "1 pass".to_string(),
        n => format!("{n} passes"),
    };
    eprintln!("{COMMAND}: {spec} (hash {hash}): answered from the critique recorded over {passes}");
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
    /// The critic persona the workflow names.
    critic: &'a str,
    /// The critique's own store ([`critique_store`]).
    critiques: &'a dyn EventStore,
    /// The verb's store selection, handed to the critic so its bound rigger server resolves the
    /// store the critique is recorded to ([`StoreSelection::handed_env`]).
    store_env: Vec<(String, String)>,
    identity: &'a str,
    /// The project scratch root; empty in a project with no git repository.
    scratch_root: &'a str,
    /// The root the spec path is relative to: the critic's working directory.
    root: &'a Path,
}

impl CriticHost<'_> {
    /// Park and spawn the critic on the spec text through the headless host, then answer with the
    /// spawn's own pass the store holds for the hash ([`review::read_spawned_critique`]) - never
    /// the session's own output nor another attempt's pass. A recorded result that is not a pass
    /// is an error saying why.
    fn critique(
        &self,
        spec: &str,
        text: &str,
        hash: &str,
    ) -> Result<CritiquePass, Box<dyn std::error::Error>> {
        let persona = self.cfg.agents.get(self.critic).ok_or_else(|| {
            format!(
                "{COMMAND}: the critic {:?} the workflow names has no definition under \
                 .rigger/agents",
                self.critic
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
        let prompt = review::spec_critique_prompt(spec, text);
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
            ..SpawnOpts::default()
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
            "{COMMAND}: {spec} (hash {hash}): critiquing with {} at attempt {attempt}",
            self.critic
        );
        let spawned = host.spawn(&critic, &prompt, &opts, &|_, _| Ok(()));
        remove_critique_scratch(self.scratch_root);
        review::read_spawned_critique(self.critiques, hash, attempt)?.map_err(|why| {
            let why = why
                .or_else(|| spawned.err().map(|e| e.to_string()))
                .unwrap_or_else(|| "the critic's session recorded no result".to_string());
            format!("{COMMAND}: {spec} (hash {hash}): no critique was recorded - {why}").into()
        })
    }
}

/// Remove the liveness, transcript and spawn-scratch directories of every critique run (`critique-<hash>`, any
/// hash) under the scratch root, so whatever a crashed critique left goes with the next call; the
/// log holds the output. A loop run's directories are never touched. Each goes through the same
/// entry-by-entry removal as a done run's teardown ([`remove_agent_area_entries`]). Best-effort:
/// a directory that cannot be read or removed stays for the next call.
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
    graph: &dyn contextgraph::Projection,
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
