//! Fixtures for suites that drive the compiled `rigger` binary against a throwaway project:
//! running it, seeding the stores and operator instruction files it reads, and reading back what
//! it wrote.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rigger::config::RIGGER_DIR;
use rigger::contextgraph::sqlite::Projector;
use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, Event, EventStore, ExpectedRevision};

pub use super::fixtures::keyed;

/// A throwaway project directory that is its own (commit-less) git repo, so the project
/// identity the binary resolves for it is its basename, stable across every call a test makes.
pub fn temp_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create temp project");
    let _ = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status();
    dir
}

/// A [`temp_project`] holding a root `Cargo.toml`: the marker the Rust gate template set detects.
pub fn cargo_project() -> tempfile::TempDir {
    let dir = temp_project();
    std::fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\n",
    )
    .expect("write the fixture Cargo.toml");
    dir
}

/// `root`'s workflow loaded through the production parser, which validates it.
pub fn loaded_config(root: &Path) -> rigger::config::Config {
    rigger::config_store::load(root.to_str().unwrap())
        .unwrap_or_else(|e| panic!("the scaffolded workflow must load: {e}"))
}

/// A [`temp_project`] the compiled binary accepts as a courier target: its own git repo (so the
/// store's project identity resolves normally) and an INITIALIZED event log - a courier refuses
/// to fabricate one from a cwd with no existing store (spec 05).
pub fn courier_project() -> tempfile::TempDir {
    let dir = temp_project();
    init_event_log(dir.path());
    dir
}

/// Open `root`'s `.rigger/events.db`, creating the directory, so the schema the binary appends
/// to exists.
pub fn init_event_log(root: &Path) {
    seed_rigger_dir(root);
    Store::open(
        rigger_file(root, "events.db")
            .to_str()
            .expect("a utf-8 store path"),
    )
    .expect("the event log initializes");
}

/// A [`temp_project`] with a commit identity configured and an empty `.rigger/` directory: its
/// own git repo keeps the project identity deterministic (the top-level is the fixture).
pub fn identified_git_project() -> tempfile::TempDir {
    let dir = temp_project();
    for args in [
        ["config", "user.email", "t@t"],
        ["config", "user.name", "t"],
    ] {
        let _ = Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .status();
    }
    seed_rigger_dir(dir.path());
    dir
}

/// A throwaway project directory that is deliberately NOT a git repo, so the conductor drives
/// a repo-less run (no worktrees, no run branch).
pub fn temp_repoless_project() -> tempfile::TempDir {
    tempfile::tempdir().expect("create temp project")
}

/// Where the file `name` (the `events.db` event log, the `graph.db` projection, ...) lives under
/// the state directory of a project rooted at `root`.
pub fn rigger_file(root: &Path, name: &str) -> PathBuf {
    root.join(RIGGER_DIR).join(name)
}

/// A raw connection of its own to the store file `name` under `root`'s rigger dir, for reading or
/// planting what the store's API never writes.
pub fn rigger_db(root: &Path, name: &str) -> rusqlite::Connection {
    rusqlite::Connection::open(rigger_file(root, name)).unwrap()
}

/// Run `rigger <args...>` in `cwd` and return (stdout, stderr, success).
pub fn run_rigger(cwd: &Path, args: &[&str]) -> (String, String, bool) {
    run_rigger_envs(cwd, args, &[])
}

/// Run `rigger <args...>` in `cwd`, asserting it exits 0 (naming its stderr when it does not),
/// and return its stdout.
pub fn run_rigger_ok(cwd: &Path, args: &[&str]) -> String {
    run_rigger_envs_ok(cwd, args, &[])
}

/// Run `rigger <args...>` in `cwd` with `envs` set ([`run_rigger_envs`]), asserting it exits 0
/// (naming its stderr when it does not), and return its stdout.
pub fn run_rigger_envs_ok(cwd: &Path, args: &[&str], envs: &[(&str, &str)]) -> String {
    let (out, err, ok) = run_rigger_envs(cwd, args, envs);
    assert!(ok, "rigger {} must exit 0; stderr:\n{err}", args.join(" "));
    out
}

/// Write each file of the operator layer `layer` into `root`'s `.rigger/instructions/` as
/// `<name>.md` holding its body.
pub fn write_operator_layer(root: &Path, layer: &[rigger::instructions::Instruction]) {
    let dir = super::fixtures::instructions_dir(root);
    for ins in layer {
        std::fs::write(dir.join(format!("{}.md", ins.name)), &ins.body).unwrap();
    }
}

/// Run `rigger validate` in `root` and assert it succeeds and that the lines of its stderr warning
/// of a built-in instruction an operator file shadows are `advisories`, in order.
pub fn assert_validate_names_the_shadowed_built_ins(root: &Path, advisories: &[&str]) {
    let (_out, err, ok) = run_rigger(root, &["validate"]);
    assert!(
        ok,
        "validate succeeds beside its shadow advisories; stderr:\n{err}"
    );
    assert_eq!(
        lines_where(&err, |line| line.contains("built-in instruction")),
        advisories,
        "validate warns once per shadowed built-in and of no other; stderr:\n{err}"
    );
}

/// Assert that over `root`, whose operator layer replaces the communication built-in, switches the
/// working-discipline built-in off and adds one file that shadows nothing, `rigger instructions`
/// prints each built-in with its state, `rigger validate` warns once per shadowed built-in on a
/// success exit with the same lines on the repeat, and `rigger prime` opens with the built-ins in
/// force and the unshadowed file count.
pub fn assert_reporters_name_the_shadowing_layer_states(root: &Path) {
    use super::fixtures::SHADOWING_LAYER_ADVISORIES;
    assert_instructions_render_the_shadowing_layer(root, &[]);
    for _ in 0..2 {
        assert_validate_names_the_shadowed_built_ins(root, &SHADOWING_LAYER_ADVISORIES);
    }
    assert_prime_names_the_shadowing_layer_states(root, &[]);
}

/// The first line `rigger prime` prints run in `cwd` ([`run_rigger_ok`]): the instructions in
/// force.
pub fn prime_first_line(cwd: &Path, envs: &[(&str, &str)]) -> Option<String> {
    run_rigger_envs_ok(cwd, &["prime"], envs)
        .lines()
        .next()
        .map(str::to_string)
}

/// A reporter assertion: it runs one instruction reporter (`rigger prime`, `rigger instructions`)
/// in a directory with the given environment set, and asserts what it printed.
pub type Reporter = fn(&Path, &[(&str, &str)]);

/// Both instruction reporters' assertions over the named-entry fixture: `rigger instructions`,
/// then `rigger prime`.
pub const INSTRUCTION_REPORTERS: [Reporter; 2] = [
    assert_instructions_render_the_shadowing_layer,
    assert_prime_names_the_shadowing_layer_states,
];

/// Run each of [`INSTRUCTION_REPORTERS`] in `cwd` with `envs` set.
pub fn assert_reporters_in(cwd: &Path, envs: &[(&str, &str)]) {
    for report in INSTRUCTION_REPORTERS {
        report(cwd, envs);
    }
}

/// Run the placement assertion `placed` - a reporter run from one place in the project - for each
/// of [`INSTRUCTION_REPORTERS`], in a committed git project with a store and in one before any
/// store exists.
pub fn assert_each_reporter_with_and_without_a_store(placed: fn(&Path, Reporter)) {
    for store in [true, false] {
        for report in INSTRUCTION_REPORTERS {
            let proj = super::fixtures::temp_git_project_with_commit();
            if store {
                seed_store(proj.path());
            }
            placed(proj.path(), report);
        }
    }
}

/// Write the named-entry fixture (`shadowing_operator_layer`) into the project at `root`, then run
/// the reporter assertion `report` at `root` and in its subdirectory: the reporter reads the
/// project's instruction layer from either, never the subdirectory's own.
pub fn assert_reported_from_below(root: &Path, report: Reporter) {
    write_operator_layer(root, &super::fixtures::shadowing_operator_layer());
    report(root, &[]);
    report(&super::fixtures::subdirectory_of(root), &[]);
}

/// Write the named-entry fixture (`shadowing_operator_layer`) into the committed git project at
/// `root`, uncommitted, then run the reporter assertion `report` in a worktree nested under its
/// `.rigger/`, which therefore checks none of that layer out: the reporter reads the main
/// checkout's layer, never the worktree's own.
pub fn assert_reported_from_a_nested_worktree(root: &Path, report: Reporter) {
    write_operator_layer(root, &super::fixtures::shadowing_operator_layer());
    report(
        &super::git::nested_worktree(root, "rigger-wt-reporter-layer"),
        &[],
    );
}

/// Write the named-entry fixture (`shadowing_operator_layer`) into the project at `root`, then run
/// the reporter assertion `report` there with the gate store fence
/// (`rigger::gate::STORE_FENCE_ENV`) pinned to a scratch store outside the project, as a gate's
/// own process runs a reporter: it reports the project's layer, the same lines as an unfenced run,
/// since the fence isolates the store and never names the project's root.
pub fn assert_reported_under_the_store_fence(root: &Path, report: Reporter) {
    write_operator_layer(root, &super::fixtures::shadowing_operator_layer());
    let scratch = tempfile::tempdir().expect("create the fence's scratch dir");
    let fence = scratch.path().join("store-fence");
    report(
        root,
        &[(rigger::gate::STORE_FENCE_ENV, fence.to_str().unwrap())],
    );
}

/// `rigger instructions` run in `cwd` prints the layers of the named-entry fixture
/// (`shadowing_operator_layer`) its project holds: each built-in with its state, then the
/// unshadowed file.
pub fn assert_instructions_render_the_shadowing_layer(cwd: &Path, envs: &[(&str, &str)]) {
    assert_eq!(
        run_rigger_envs_ok(cwd, &["instructions"], envs),
        super::fixtures::rendered_shadowing_layer(),
        "`rigger instructions` prints each built-in with its state, then the unshadowed file"
    );
}

/// `rigger prime` run in `cwd` opens with the in-force line of the named-entry fixture
/// (`shadowing_operator_layer`) its project holds.
pub fn assert_prime_names_the_shadowing_layer_states(cwd: &Path, envs: &[(&str, &str)]) {
    assert_eq!(
        prime_first_line(cwd, envs).as_deref(),
        Some(super::fixtures::SHADOWING_LAYER_IN_FORCE_LINE),
        "`rigger prime` opens with the built-ins in force and the unshadowed file count"
    );
}

/// Run `rigger graph --around <seed> --depth <depth>` in `cwd` ([`run_rigger`]) and return
/// (stdout, stderr, success).
pub fn run_around(cwd: &Path, seed: &str, depth: i64) -> (String, String, bool) {
    run_rigger(
        cwd,
        &["graph", "--around", seed, "--depth", &depth.to_string()],
    )
}

/// Every printed `edge <from> -<rel>-> <to>` line of a `rigger graph --around` transcript, parsed
/// into its (from, rel, to) triple - never substring-matched, so an assertion about which edges
/// printed is a claim about the parsed fields, not a text search a coincidental substring could
/// satisfy.
pub fn edge_lines(out: &str) -> Vec<(String, String, String)> {
    out.lines()
        .filter_map(|l| {
            let rest = l.trim_start().strip_prefix("edge ")?;
            let (from, rest) = rest.split_once(" -")?;
            let (rel, to) = rest.split_once("-> ")?;
            Some((
                from.trim().to_string(),
                rel.trim().to_string(),
                to.trim().to_string(),
            ))
        })
        .collect()
}

/// Run `rigger <args...>` in `cwd` with extra environment `envs` and return
/// (stdout, stderr, success).
pub fn run_rigger_envs(cwd: &Path, args: &[&str], envs: &[(&str, &str)]) -> (String, String, bool) {
    // Bound to `state` so the dir lives until after the command runs.
    let state = tempfile::tempdir().expect("create a temp XDG_STATE_HOME for the rigger run");
    let out = rigger_command(cwd, args, envs, state.path())
        .output()
        .expect("failed to spawn the rigger binary");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

/// Run `rigger <args>` in `cwd` with the checked-in prompt-capture agent first on PATH as
/// `claude`, assert the command succeeds, and return the system prompt the last agent it spawned
/// was handed: the text of the file its `--system-prompt-file` named.
pub fn run_capturing_system_prompt(cwd: &Path, args: &[&str]) -> String {
    let work = tempfile::tempdir().unwrap();
    let path = super::repo::stub_path(work.path(), "claude", Some("prompt-capture-agent.sh"));
    let (out, err, ok) = run_rigger_envs(
        cwd,
        args,
        &[
            ("PATH", &path),
            (
                super::fixtures::PROMPT_CAPTURE_DIR,
                work.path().to_str().unwrap(),
            ),
        ],
    );
    assert!(
        ok,
        "rigger {args:?} must succeed; stderr: {err}\nstdout: {out}"
    );
    super::fixtures::captured_system_prompt(work.path())
}

/// `rigger setup` in `cwd` with extra environment `envs`, which must succeed: its stdout.
pub fn setup_ok(cwd: &Path, envs: &[(&str, &str)]) -> String {
    let (out, err, ok) = run_rigger_envs(cwd, &["setup"], envs);
    assert!(ok, "setup must succeed; stdout: {out} stderr: {err}");
    out
}

/// `rigger <args...>` in `cwd` with extra environment `envs`, as [`run_rigger_envs`] runs it, for
/// a caller that spawns it and acts while it runs; `state` must outlive the process.
pub fn rigger_command(cwd: &Path, args: &[&str], envs: &[(&str, &str)], state: &Path) -> Command {
    let mut cmd = super::rigger_courier();
    cmd.args(args).current_dir(cwd);
    // The step path auto-starts a persistent, detached run dashboard (spec 39, criterion 1);
    // opt out so these short-lived integration invocations never spawn a real dashboard
    // process that would outlive the test. Set before the caller's envs so a test could still
    // override it.
    cmd.env("RIGGER_NO_DASH", "1");
    // The step/run/serve paths register this instance in the machine-global registry under
    // XDG_STATE_HOME (spec 50, criterion 2). Default it to a per-invocation temp dir so the
    // many tests that drive those paths never seed a phantom into the operator's real
    // ~/.local/state/rigger/instances - a live discovery entry, rooted at a since-deleted test
    // tempdir, that a running dash would otherwise pick up. Set before the caller's envs so the
    // registry tests that pass an explicit XDG_STATE_HOME (to read the registry back) still
    // override it.
    cmd.env("XDG_STATE_HOME", state);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd
}

/// A [`temp_project`] that already carries an empty `.rigger/` dir.
pub fn temp_rigger_project() -> tempfile::TempDir {
    let dir = temp_project();
    seed_rigger_dir(dir.path());
    dir
}

/// A [`temp_project`] carrying an empty `.rigger/events.db` ([`seed_store`]) - the store a prior
/// run would have created, which every store-opening courier requires before it appends.
pub fn temp_store_project() -> tempfile::TempDir {
    let dir = temp_project();
    seed_store(dir.path());
    dir
}

/// Run `rigger <args...>` in `cwd` with the machine-global instance registry redirected into
/// the CALLER-OWNED `state_home`, so a sequence of calls reads back and re-writes the same
/// registry directory.
pub fn run_rigger_in_state_home(cwd: &Path, state_home: &Path, args: &[&str]) -> Output {
    rigger_command(cwd, args, &[], state_home)
        .output()
        .expect("the rigger binary runs")
}

/// `rigger emit <typ> <json>` in `root`, asserting it succeeds.
pub fn emit(root: &Path, typ: &str, json: &str) {
    let (_o, err, ok) = run_rigger(root, &["emit", typ, json]);
    assert!(ok, "emit {typ} must succeed; stderr: {err}");
}

/// Record the critic's `answer` as the critique of `spec`'s current text in the project at `root`
/// (spec 112): `rigger critique <spec>` with the checked-in critique stub, answering `answer`,
/// first on `PATH` for that one call only, so a fixture's own fake agent keeps its `PATH` slot for
/// the run it drives. Asserts the verb succeeded and returns its stdout.
pub fn record_critique(root: &Path, spec: &str, answer: &str) -> String {
    let work = tempfile::tempdir().expect("a work directory for the critique stub");
    let path = super::repo::write_critique_stub(work.path(), answer);
    let (out, err, ok) = run_rigger_envs(root, &["critique", spec], &[("PATH", path.as_str())]);
    assert!(
        ok,
        "rigger critique {spec} must record a critique; stderr:\n{err}"
    );
    out
}

/// The id `rigger critique` records the `k`-th finding line of a critique of `text` under, at
/// `attempt` (spec 112): `sc-<hash>-<attempt>-<k>`, `<hash>` the content hash of `text`.
pub fn critique_finding_id(text: &str, attempt: u32, k: u32) -> String {
    format!("sc-{}-{attempt}-{k}", rigger::review::critique_hash(text))
}

/// Record a clean critique of `spec`'s current text in the project at `root` ([`record_critique`]:
/// prose, no finding line, an approve), so a new run on it under a workflow naming a critic is not
/// refused (spec 112).
pub fn record_clean_critique(root: &Path, spec: &str) {
    let out = record_critique(root, spec, "No defects.\n{\"verdict\":\"approve\"}");
    assert_eq!(
        out, "{\"verdict\":\"approve\"}\n",
        "a clean critique of {spec}: no finding and an approve"
    );
}

/// The `DecisionMade` a refusal of a new run on `spec` prints for the operator to record (spec
/// 112, Notes), resolving `resolves`.
pub fn printed_decision(spec: &str, resolves: &str) -> String {
    format!(
        "{{\"id\":\"...\",\"governs\":{},\"resolves\":{resolves},\"summary\":\"...\"}}",
        serde_json::json!([spec])
    )
}

/// The route label the refusal of a new run prints before `rigger critique <spec>` (spec 112,
/// Notes).
pub const CRITIQUE_ROUTE: &str = "amend the spec and critique it:";

/// The route label the refusal of a new run on a critiqued spec prints before the resolution it
/// offers (spec 112, Notes).
pub const RESOLUTION_ROUTE: &str = "or record a resolution:";

/// The text the refusal of a new run on `spec` by `command` ends stderr with (spec 112, Notes):
/// `open` the open BLOCKING finding ids, `None` when the spec's text has no critique. The route
/// labels are padded to one column.
pub fn refused_new_run(command: &str, spec: &str, open: Option<&[String]>) -> String {
    let (why, route, resolves) = match open {
        None => (
            "not critiqued".to_string(),
            "or, once critiqued, record a resolution:".to_string(),
            "[<ids>]".to_string(),
        ),
        Some(ids) => (
            format!("open BLOCKING findings: {}", ids.join(", ")),
            format!("{RESOLUTION_ROUTE:<33}"),
            serde_json::json!(ids).to_string(),
        ),
    };
    format!(
        "rigger: {command}: refusing to begin a new run on {spec}: {why}\n  {CRITIQUE_ROUTE}   \
         rigger critique {spec}\n  {route} rigger emit DecisionMade '{}'\n",
        printed_decision(spec, &resolves)
    )
}

/// Create an empty `.rigger/` under `root`.
pub fn seed_rigger_dir(root: &Path) {
    std::fs::create_dir_all(root.join(".rigger")).unwrap();
}

/// Seed an initialized `.rigger/events.db` under `root`, standing in for the store a
/// prior `rigger run`/`step` would have created. The store-opening couriers
/// (`emit`/`result`/`peers`) REFUSE to fabricate a fresh store from the wrong cwd
/// (spec 05), so a round-trip test must first establish one, exactly as a real run does
/// before any courier appends to it. An empty file is a valid empty SQLite database;
/// `Store::open` adds the schema on first open - so this models "the run created the
/// store" without needing a full workflow.
pub fn seed_store(root: &Path) {
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(&rigger).unwrap();
    std::fs::File::create(rigger.join("events.db")).unwrap();
}

/// The project identity the binary resolves for `root`, mirrored here for seeding: the
/// tracked `.rigger/project.id` at the git top-level when present, else the git top-level
/// basename, else `root`'s own basename (never empty) - the precedence
/// `project_identity_at` uses. A seed appended under this identity lands in the exact
/// `proj-<id>-run` stream the compiled binary reads back.
pub fn run_stream_identity(root: &Path) -> String {
    let toplevel = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());
    let base = toplevel.as_deref().map(Path::new).unwrap_or(root);
    if let Ok(raw) = std::fs::read_to_string(base.join(".rigger").join("project.id")) {
        let id = raw.trim();
        if !id.is_empty() {
            return id.to_string();
        }
    }
    base.file_name()
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty())
        .map(String::from)
        .unwrap_or_else(|| "rigger".to_string())
}

/// The fold rule a `graph.db` records (its `user_version`).
pub fn user_version(db: &Path) -> i64 {
    super::fixtures::pragma_i64(db, "user_version")
}

/// Whether an open of the `graph.db` at `db` for `project` owes a rebuild.
pub fn rebuild_owed(db: &Path, project: &str) -> bool {
    use rigger::contextgraph::Projection;
    Projector::open(db.to_str().unwrap(), project)
        .unwrap()
        .rebuild_owed()
        .unwrap()
}

/// The fold rule a `graph.db` records after an open of it for `project`, and whether that open
/// owes a rebuild.
pub fn version_and_owed(db: &Path, project: &str) -> (i64, bool) {
    let owed = rebuild_owed(db, project);
    (user_version(db), owed)
}

/// Seed `(type, json body)` run events directly into `root`'s namespaced run stream, standing
/// in for the conductor minting them - the `rigger emit` surface refuses conductor-owned
/// boundary types (spec 22), so a test that must seed them appends through the store.
pub fn seed_run_events(root: &Path, events: &[(&str, &str)]) {
    let rigger_dir = root.join(".rigger");
    std::fs::create_dir_all(&rigger_dir).unwrap();
    with_run_store(root, |store| {
        for &(ty, body) in events {
            store
                .append(
                    rigger::conductor::STREAM,
                    ExpectedRevision::Any,
                    &[Event::new(ty, body.as_bytes().to_vec())],
                )
                .unwrap();
        }
    });
}

/// `f` over `root`'s own project namespace of its on-disk `.rigger/events.db` - the store the
/// binary writes, opened through the same composition the binary opens it through.
pub fn with_run_store<R>(root: &Path, f: impl FnOnce(&dyn EventStore) -> R) -> R {
    let backend = Store::open(rigger_file(root, "events.db").to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    f(&store)
}

/// `f` over `root`'s own project namespace of its `.rigger/events.db` as a binary before the
/// ledger wrote it: every append `f` makes lands as pre-ledger rows
/// ([`super::fixtures::PreLedgerStore`]), so `f` seeds the derived events a store now refuses.
pub fn with_pre_ledger_run_store<R>(root: &Path, f: impl FnOnce(&dyn EventStore) -> R) -> R {
    let db = rigger_file(root, "events.db");
    let backend = Store::open(db.to_str().unwrap()).unwrap();
    let pre_ledger = super::fixtures::PreLedgerStore {
        db: &db,
        inner: &backend,
    };
    let store = Namespaced::new(&pre_ledger, &run_stream_identity(root));
    f(&store)
}

/// Append `events` to `root`'s own run stream as a binary before the ledger recorded them
/// ([`with_pre_ledger_run_store`]), so a test seeds the derived events a store now refuses.
pub fn seed_pre_ledger_events(root: &Path, events: &[Event]) {
    with_pre_ledger_run_store(root, |store| {
        store
            .append(rigger::conductor::STREAM, ExpectedRevision::Any, events)
            .unwrap();
    });
}

/// Every event in `root`'s namespaced run stream, oldest first.
pub fn read_run_events(root: &Path) -> Vec<Event> {
    with_run_store(root, |store| {
        store
            .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
            .unwrap()
    })
}

/// The id of the run [`rigger::run::current_run_id`] resolves over `root`'s own run stream
/// ([`read_run_events`]), `None` before any run started.
pub fn current_run_id_of(root: &Path) -> Option<String> {
    rigger::run::current_run_id(&read_run_events(root))
}

/// `root`'s run stream as `(position, revision, type)`, oldest first.
pub fn stream_shape(root: &Path) -> Vec<(u64, i64, String)> {
    read_run_events(root)
        .into_iter()
        .map(|event| (event.position, event.revision, event.type_))
        .collect()
}

/// `(position, type)` of every event on `root`'s run stream, oldest first.
pub fn run_log(root: &Path) -> Vec<(u64, String)> {
    read_run_events(root)
        .into_iter()
        .map(|e| (e.position, e.type_))
        .collect()
}

/// The decoded payloads of the events of type `type_` in `root`'s namespaced run stream, oldest
/// first.
pub fn run_payloads(root: &Path, type_: &str) -> Vec<serde_json::Value> {
    read_run_events(root)
        .iter()
        .filter(|e| e.type_ == type_)
        .map(|e| serde_json::from_slice(&e.data).unwrap())
        .collect()
}

/// Write the `nodes` row `id` of graph kind `kind` under `project` straight into the graph store
/// `conn` holds open, its `attrs` the JSON object text given, else NULL: a row no fold writes
/// (a nameless node at a line, an older store's node).
pub fn insert_node(
    conn: &rusqlite::Connection,
    project: &str,
    id: &str,
    kind: &str,
    attrs: Option<&str>,
) {
    conn.execute(
        "INSERT INTO nodes (id, kind, attrs, project) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![id, kind, attrs, project],
    )
    .unwrap();
}

/// Seed `root` as a project whose own `.rigger/graph.db` holds `graph`'s nodes and edges alone,
/// each written straight into the store under `root`'s run-stream identity - a node by
/// [`insert_node`], its `attrs` the JSON object of the node's own, an edge as its `edges` row - and
/// hand back that projection: a graph no fold writes, read by the binary and the library alike.
pub fn planted_graph(root: &Path, graph: &rigger::contextgraph::Graph) -> Projector {
    seed_rigger_dir(root);
    let planted = open_graph(root);
    let conn = rigger_db(root, "graph.db");
    let project = run_stream_identity(root);
    for n in &graph.nodes {
        let attrs = serde_json::to_string(&n.attrs).unwrap();
        insert_node(&conn, &project, &n.id, &n.kind, Some(&attrs));
    }
    for e in &graph.edges {
        conn.execute(
            "INSERT INTO edges (from_id, to_id, rel, valid_from, valid_to, source, project, tier)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                e.from,
                e.to,
                e.rel,
                e.valid_from,
                e.valid_to,
                e.source,
                project,
                e.tier
            ],
        )
        .unwrap();
    }
    planted
}

/// An alias the CLI's alias tests define of
/// [`UNRESOLVED_FILE`](super::fixtures::UNRESOLVED_FILE) (spec 117).
pub const UNRESOLVED_ALIAS: &str = "src/aka.rs";

/// A [`temp_project`] whose own graph holds the rows
/// [`seed_unresolved_references`](super::fixtures::seed_unresolved_references) stages (spec 117,
/// UNRESOLVED REFERENCES ARE A READ).
pub fn unresolved_project() -> tempfile::TempDir {
    let dir = temp_project();
    seed_rigger_dir(dir.path());
    super::fixtures::seed_unresolved_references(&open_graph(dir.path()));
    dir
}

/// The error `rigger graph --around <seed> --depth 1` fails with over an [`unresolved_project`]
/// once `sql` ran on its graph, the command printing nothing on stdout and its stderr that error
/// alone, after the binary's `rigger: graph: ` prefix.
pub fn around_unresolved_fails_after(sql: &str, seed: &str) -> String {
    let dir = unresolved_project();
    rigger_db(dir.path(), "graph.db")
        .execute_batch(sql)
        .unwrap();
    let (out, err, ok) = run_around(dir.path(), seed, 1);
    assert!(
        out.is_empty() && !ok,
        "the around fails printing nothing; stderr:\n{err}"
    );
    err.strip_prefix("rigger: graph: ")
        .map(str::trim_end)
        .unwrap_or_else(|| panic!("a graph error; stderr:\n{err}"))
        .to_string()
}

/// The stdout `rigger graph --show <query>` prints when several definitions answer `query`: the
/// candidate count, then each `(id, file)` of `cands` on its own line, in their order, and no body.
pub fn ambiguous_show(query: &str, cands: &[(&str, &str)]) -> String {
    let mut out = format!(
        "show \"{query}\": {} candidates - the name is ambiguous, re-run --show on one id:\n",
        cands.len()
    );
    for (id, file) in cands {
        out.push_str(&format!("  {id}   ({file})\n"));
    }
    out
}

/// Every line `rigger graph --show <query>` prints over `root`, in order.
pub fn show_lines(root: &Path, query: &str) -> Vec<String> {
    run_rigger_ok(root, &["graph", "--show", query])
        .lines()
        .map(str::to_string)
        .collect()
}

/// The first two lines `rigger graph --show <query>` prints over `root` ([`show_lines`]): the
/// header naming the id it shows and that definition's site line.
pub fn show_head(root: &Path, query: &str) -> Vec<String> {
    let mut lines = show_lines(root, query);
    lines.truncate(2);
    lines
}

/// `rigger graph --show <file>::<path>`'s stdout over `root` seeded as a project whose working
/// tree holds `source` at `file` and whose graph records the definition at the id path `path`
/// (spec 119, IDS FOLLOW THE LANGUAGE'S PATH), named by the text after the path's last `::`, of
/// kind `kind` at `line` in `lang`.
pub fn show_recorded(
    root: &Path,
    file: &str,
    source: &str,
    path: &str,
    kind: &str,
    line: u64,
    lang: &str,
) -> String {
    seed_rigger_dir(root);
    super::fixtures::write_text(root, file, source);
    let name = super::last_path_segment(path);
    super::fixtures::apply_json(
        &open_graph(root),
        1,
        rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
        super::fixtures::path_entity_json(file, name, path, kind, line, lang),
    );
    run_rigger_ok(
        root,
        &[
            "graph",
            "--show",
            &rigger::contextgraph::query::entity_id(file, path),
        ],
    )
}

/// Hold `graph_db` under another writer's write lock while `run` runs, past every busy timeout
/// the binary waits on, and hand back what `run` returned once the lock is released.
pub fn with_graph_locked<T>(graph_db: &Path, run: impl FnOnce() -> T) -> T {
    let holder = rusqlite::Connection::open(graph_db).unwrap();
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();
    let out = run();
    holder.execute_batch("ROLLBACK").unwrap();
    out
}

/// The fold state of the graph at `graph_db` that decides how FUTURE events fold (spec 101, "the
/// identity"): every recorded test reference and where it resolved (pending ones included), the restorable attrs a retired node keeps, each
/// identity's current generation, the node and edge assertions of live generations (edges by their
/// columns, never their row ids), and the detached attachments a returning node revives. Everything
/// else in the file is history a compacted log no longer replays.
pub fn fold_state(graph_db: &Path) -> Vec<String> {
    let conn = rusqlite::Connection::open(graph_db).unwrap();
    [
        "SELECT project, name, file, evidence, source, target FROM proofs
          ORDER BY project, name, file, source, evidence",
        "SELECT project, id, attrs FROM retired_nodes WHERE attrs IS NOT NULL ORDER BY project, id",
        "SELECT project, identity, generation FROM generations ORDER BY project, identity",
        "SELECT project, identity, generation, node_id, kind, attrs FROM live_node_assertions
          ORDER BY project, identity, node_id",
        "SELECT a.project, a.identity, a.generation, e.from_id, e.to_id, e.rel, e.tier,
                e.valid_from, e.valid_to, e.source
           FROM live_edge_assertions a JOIN edges e ON e.id = a.edge_id
          ORDER BY 1, 2, 3, 4, 5, 6, 7",
        "SELECT d.project, d.node_id, e.to_id, e.rel, e.tier, e.valid_from, e.source
           FROM detached_attachments d JOIN edges e ON e.id = d.edge_id
          ORDER BY 1, 2, 3, 4",
    ]
    .into_iter()
    .flat_map(|sql| {
        let mut stmt = conn.prepare(sql).unwrap();
        let width = stmt.column_count();
        stmt.query_map([], |r| {
            Ok((0..width)
                .map(|i| match r.get_ref(i).unwrap() {
                    rusqlite::types::ValueRef::Text(t) => String::from_utf8_lossy(t).into_owned(),
                    other => format!("{other:?}"),
                })
                .collect::<Vec<_>>()
                .join(" | "))
        })
        .unwrap()
        .map(Result::unwrap)
        .map(|row| format!("{sql}: {row}"))
        .collect::<Vec<_>>()
    })
    .collect()
}

/// The whole live projection of the graph at `graph_db`, read under `project`, and its
/// [`fold_state`], as one comparable value: spec 101's comparison surface.
pub fn graph_identity(graph_db: &Path, project: &str) -> (String, Vec<String>) {
    let whole = Projector::open(graph_db.to_str().unwrap(), project)
        .unwrap()
        .whole()
        .unwrap();
    (serde_json::to_string(&whole).unwrap(), fold_state(graph_db))
}

/// Whether `db`'s applied ledger records `position`.
pub fn applied(db: &Path, position: u64) -> bool {
    exists(
        db,
        "SELECT EXISTS (SELECT 1 FROM applied WHERE position = ?1)",
        position,
    )
}

/// Every position `db`'s applied ledger records, ascending.
pub fn applied_positions(db: &Path) -> Vec<u64> {
    let conn = rusqlite::Connection::open(db).unwrap();
    let mut rows = conn
        .prepare("SELECT position FROM applied ORDER BY position")
        .unwrap();
    let positions = rows.query_map([], |r| r.get(0)).unwrap();
    positions.map(Result::unwrap).collect()
}

/// What the `SELECT EXISTS` query `sql` answers over `db` with `param` bound to `?1`.
pub fn exists(db: &Path, sql: &str, param: impl rusqlite::ToSql) -> bool {
    rusqlite::Connection::open(db)
        .unwrap()
        .query_row(sql, [param], |r| r.get(0))
        .unwrap()
}

/// What a rebuild that reports nothing hands its progress callback.
pub fn no_progress(_: rigger::contextgraph::sqlite::RebuildProgress) {}

/// The graph projection of `root`'s own `.rigger/graph.db`, under its run-stream identity.
pub fn open_graph(root: &Path) -> Projector {
    let id = run_stream_identity(root);
    Projector::open(rigger_file(root, "graph.db").to_str().unwrap(), &id).unwrap()
}

/// The graph `rigger graph build` folds over a fresh [`temp_project`] holding each of `files`, a
/// `(root-relative path, source text)` pair; the project lives as long as the returned directory.
pub fn built_graph(files: &[(&str, &str)]) -> (tempfile::TempDir, Projector) {
    let dir = temp_project();
    let graph = rebuilt_graph(dir.path(), files);
    (dir, graph)
}

/// The graph `rigger graph build` folds over the project at `root` once each of `files`, a
/// `(root-relative path, source text)` pair, is written there: a first build, or a rebuild over
/// the files' next generation.
pub fn rebuilt_graph(root: &Path, files: &[(&str, &str)]) -> Projector {
    super::fixtures::write_files(root, files);
    run_rigger_ok(root, &["graph", "build"]);
    open_graph(root)
}

/// GIVEN a project whose one file `path`, in the language `lang`, holds `source`, WHEN the
/// operator runs `rigger graph build`, THEN the built graph's definitions and placeholders are
/// exactly one node `<path>::<id path>` per `definitions` row, an (id path, leaf, kind, line)
/// quadruple (spec 119, IDS FOLLOW THE LANGUAGE'S PATH), each carrying exactly that leaf as its
/// name, kind and line, and one placeholder `<path>::%<leaf>` ([`placeholder_id`]) per
/// `placeholders` leaf, a name a reference reaches that no one same-file call target gives
/// (`entities_why` says what makes no node), and its `CALLS` edges are exactly `calls`. Any other
/// code entity is the nameless node a call folds from, named in `calls`.
///
/// [`placeholder_id`]: rigger::contextgraph::query::placeholder_id
pub fn assert_built_graph_holds(
    (path, lang, source): (&str, &str, &str),
    definitions: &[(&str, &str, &str, u64)],
    placeholders: &[&str],
    calls: &[(&str, &str)],
    entities_why: &str,
) {
    use super::fixtures::{
        assert_live_calls, attrs_of, definition_ids, definition_in, live_node_ids,
    };
    use rigger::contextgraph::query::{entity_id, leaf_of, placeholder_id};
    use rigger::contextgraph::KIND_CODE_ENTITY;

    let (_project, graph) = built_graph(&[(path, source)]);
    let held: std::collections::BTreeSet<String> = definition_ids(&graph)
        .into_iter()
        .chain(
            live_node_ids(&graph.whole().unwrap(), KIND_CODE_ENTITY)
                .into_iter()
                .filter(|node| leaf_of(node).is_some()),
        )
        .collect();
    let ids: Vec<String> = definitions
        .iter()
        .map(|(id_path, _, _, _)| entity_id(path, id_path))
        .collect();
    let entities: std::collections::BTreeSet<String> = ids
        .iter()
        .cloned()
        .chain(placeholders.iter().map(|leaf| placeholder_id(path, leaf)))
        .collect();
    assert_eq!(held, entities, "{entities_why}");
    let carried: Vec<_> = ids.iter().map(|id| attrs_of(&graph, id)).collect();
    let expected: Vec<_> = definitions
        .iter()
        .map(|(_, name, kind, line)| definition_in(lang, name, kind, *line))
        .collect();
    assert_eq!(
        carried, expected,
        "each definition node carries its kind and line, and nothing else"
    );
    assert_live_calls(
        &graph,
        calls,
        "every CALLS edge of the built graph, each from the caller the fixture gives it",
    );
}

/// Every `graph.db*` entry under `.rigger/` of `root` ([`super::fixtures::dir_snapshot`]).
pub fn graph_files(root: &Path) -> Vec<(String, Option<Vec<u8>>)> {
    super::fixtures::dir_snapshot(&rigger_file(root, ""), "graph.db")
}

/// The event log's files and every `graph.db*` file under `.rigger/` of `root`, byte for byte: what
/// a refused `rigger reset --derived` must leave as it found.
pub type LogAndGraphFiles = (
    Vec<(String, Option<Vec<u8>>)>,
    Vec<(String, Option<Vec<u8>>)>,
);

/// The [`LogAndGraphFiles`] of `root` as they stand now.
pub fn log_and_graph_files(root: &Path) -> LogAndGraphFiles {
    (
        super::fixtures::dir_snapshot(&rigger_file(root, ""), "events.db"),
        graph_files(root),
    )
}

/// `rigger reset --derived` run in `root`, as a test that expects it refused reads it: whether it
/// succeeded, its stdout, its stderr, and whether the log and the graph files stand as `found`.
pub fn refused_derived_reset(
    root: &Path,
    found: &LogAndGraphFiles,
) -> (bool, String, String, bool) {
    let (out, err, ok) = run_rigger(root, &["reset", "--derived"]);
    (ok, out, err, log_and_graph_files(root) == *found)
}

/// The two lines `rigger reset --derived` prints, each ended, for a migration that converted
/// `converted` latest batches into ledger entries and shed `shed` derived events, `unkeyed` of
/// them naming no file identity.
pub fn migrated_lines(converted: usize, shed: usize, unkeyed: usize) -> String {
    format!(
        "reset --derived: converted {converted} latest batch(es) into ledger entries and shed \
         {shed} derived event(s) from the event log\n\
         reset --derived: {unkeyed} of the derived event(s) shed named no file identity (no \
         replay key, or one that does not parse)\n"
    )
}

/// The line `rigger reset --derived` prints, ended, over a store holding no derived event.
pub const NOTHING_TO_SHED_LINE: &str = "reset --derived: no derived event to shed\n";

/// The `--derived` line bare `rigger reset` prints over a store holding `events` derived events
/// of `identities` file identities.
pub fn derived_menu_line_naming(events: usize, identities: usize) -> String {
    format!(
        "--derived: {events} derived events of {identities} file identities to shed from the \
         event log; rerun `rigger reset --derived` to migrate them"
    )
}

/// The `--derived` line bare `rigger reset` prints over a store holding no derived event.
pub const NOTHING_TO_SHED_MENU_LINE: &str = "--derived: no derived event to shed";

/// The `--derived` lines of what bare `rigger reset` printed, in order.
pub fn derived_menu_lines(stdout: &str) -> Vec<&str> {
    lines_where(stdout, |line| line.starts_with("--derived:"))
}

/// The line `rigger reset --derived` prints, ended, when the log's file held no free page.
pub const LOG_LEFT_AS_IT_STANDS_LINE: &str =
    "reset --derived: the log file holds no reclaimable free page, so it was left as it stands \
     rather than rewritten to reclaim nothing\n";

/// The line `rigger reset --derived` prints, ended, for a rewrite that reclaimed `bytes`.
pub fn reclaimed_line(bytes: u64) -> String {
    format!("reset --derived: compacted the log file and reclaimed {bytes} byte(s) on disk\n")
}

/// The line a command refused at `graph.db.lock` ends its stderr with, whoever holds the lock.
pub const REBUILD_LOCK_REFUSAL: &str =
    "rigger: graph: graph.db.lock is held by another `rigger setup` or `rigger reset`";

/// The whole stderr of `rigger reset --derived` on a project configured for the server-backed
/// store, where the migration does not run.
pub const SERVER_BACKED_DERIVED_REFUSAL: &str =
    "rigger: reset --derived: the migration rewrites and deletes rows of the event log and \
     vacuums the file, which is a mechanic of the embedded .rigger/events.db store; this project \
     is configured for the server-backed store, where the migration does not run. Re-run it \
     against a project on the sqlite backend. Refusing rather than reporting a migration that did \
     not happen.\n";

/// The line `rigger reset --derived` ends its stderr with over a rebuild left unfinished.
pub const UNFINISHED_REBUILD_REFUSAL: &str =
    "rigger: reset --derived: a rebuild of graph.db was left unfinished - run `rigger setup` to \
     finish it";

/// The rebuild lock of the `graph.db` of `root`, held as a `rigger setup` or a `rigger reset`
/// holds `graph.db.lock`, until it is dropped.
pub fn hold_the_rebuild(root: &Path) -> rigger::contextgraph::sqlite::RebuildLock {
    Projector::lock_rebuild(rigger_file(root, "graph.db").to_str().unwrap()).unwrap()
}

/// Whether `line` is a numbered source line (`<n> | <text>`) of a `rigger graph --show` body.
pub fn is_body_line(line: &str) -> bool {
    line.trim_start()
        .split_once(" | ")
        .map(|(pre, _)| pre.trim().parse::<u32>().is_ok())
        .unwrap_or(false)
}

/// The number of numbered source lines (`<n> | <text>`) in a `rigger graph --show` body.
pub fn body_line_count(out: &str) -> usize {
    out.lines().filter(|l| is_body_line(l)).count()
}

/// The entry and value pairs a `rigger graph --show` output prints, in order (spec 119, SHOW IS
/// ONE RENDERER): the header's `file`, `line`, `kind` and `degree` (no `site:` part for a node with
/// no file), then one `  <entry> <value>` line each until the body or the note.
pub fn printed_site_entries(out: &str) -> Vec<(String, String)> {
    let mut lines = out.lines().skip(1);
    let header = lines.next().expect("a header line");
    let mut pairs: Vec<(&str, &str)> = Vec::new();
    let rest = match header.strip_prefix("  site: ") {
        Some(site) => {
            let (at, rest) = site.split_once("   ").expect("a site part");
            let (file, line) = at.rsplit_once(':').expect("a file:line site");
            pairs.extend([("file", file), ("line", line)]);
            rest
        }
        None => header.trim_start(),
    };
    pairs.extend(
        rest.split("   ")
            .take_while(|part| !part.starts_with('('))
            .map(|part| part.split_once(' ').expect("an entry")),
    );
    pairs.extend(
        lines
            .take_while(|line| !line.starts_with("  (") && !is_body_line(line))
            .map(|line| line.trim_start().split_once(' ').expect("an entry line")),
    );
    pairs
        .into_iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

/// The [`site_fields`](rigger::contextgraph::site_fields) entries of `site` after the CLI's two
/// print transforms (spec 119, SHOW IS ONE RENDERER): `line` the live line the body came from,
/// and each string value's newlines spaces - the pairs [`printed_site_entries`] reads back.
pub fn site_entries_as_printed(
    site: &rigger::contextgraph::EntitySite,
    live_line: u32,
) -> Vec<(String, String)> {
    rigger::contextgraph::site_fields(site)
        .into_iter()
        .map(|(key, value)| {
            let text = match value {
                _ if key == "line" => live_line.to_string(),
                serde_json::Value::String(text) => text.replace('\n', " "),
                number => number.to_string(),
            };
            (key.to_string(), text)
        })
        .collect()
}

/// The line `rigger graph build` prints for a build that counted `events` batch events, with
/// `lost`, its fold-loss clause: empty when the build lost no fold.
pub fn graph_build_line(events: usize, lost: &str) -> String {
    format!("graph build: ingested {events} code-ingest event(s) into .rigger/graph.db{lost}\n")
}

/// The entity count a `rigger graph build` reports (`... ingested <n> ...`).
pub fn ingested_count(stdout: &str) -> usize {
    stdout
        .split_once("ingested ")
        .and_then(|(_, rest)| rest.split_whitespace().next())
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("graph build must report its ingested count; got:\n{stdout}"))
}

/// Write a heartbeat marker at `marker` whose mtime is an hour old - a stale liveness mark.
pub fn plant_stale_marker(marker: &Path) {
    plant_marker(marker, 3600);
}

/// Write a heartbeat marker at `marker` last touched `secs_ago` seconds ago, so a liveness
/// reader judges a known age against its bound (0 is a marker touched right now).
pub fn plant_marker(marker: &Path, secs_ago: u64) {
    std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
    std::fs::write(marker, b"heartbeat").unwrap();
    let touched = SystemTime::now() - Duration::from_secs(secs_ago);
    std::fs::File::options()
        .write(true)
        .open(marker)
        .unwrap()
        .set_modified(touched)
        .unwrap();
}

/// The `SpawnResult` body `rigger step`'s sweep records on `spawn_id` once its liveness marker
/// went stale - the step's liveness fault, built by the product's own constructor: a diagnosis
/// of a silent worker, not the spawn's end.
pub fn liveness_fault_body(spawn_id: &str) -> String {
    serde_json::to_string(&rigger::spawn::SpawnResult::liveness_fault(
        spawn_id, "hung", "infra",
    ))
    .unwrap()
}

/// A real `SpawnResult` body on `spawn_id` - its worker's (or the operator's) `rigger result` -
/// which ends the spawn.
pub fn real_result_body(spawn_id: &str) -> String {
    serde_json::to_string(&rigger::spawn::SpawnResult::ok(spawn_id, "done")).unwrap()
}

/// Open, exclusively lock (non-blocking), and return `.rigger/step.lock` under `root` - standing
/// in for a `rigger step` holding it for its whole duration. The lock lasts until the returned
/// file is dropped.
pub fn hold_step_lock(root: &Path) -> std::fs::File {
    use fs2::FileExt;
    let lock_file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(rigger_file(root, "step.lock"))
        .unwrap();
    lock_file
        .try_lock_exclusive()
        .expect("the test must be able to take the lock first");
    lock_file
}

/// Configures `defaults.workdir` in the `workflow.yml` of the store under `root` - a fresh
/// directory, followed by `extra` (further `defaults` keys, two-space indented, or empty) - and
/// returns that directory with the scratch root a run of that store stamps its spawns' liveness
/// markers under once the workdir moves it off the default cache root.
pub fn configure_workdir(root: &Path, extra: &str) -> (tempfile::TempDir, String) {
    let relocated = tempfile::tempdir().expect("create the configured workdir");
    let workdir = relocated.path().to_str().unwrap().to_string();
    std::fs::write(
        rigger_file(root, "workflow.yml"),
        format!("defaults:\n  workdir: \"{workdir}\"\n{extra}"),
    )
    .expect("configure defaults.workdir");
    let scratch_root = rigger::worktree::scratch_root(root.to_str().unwrap(), &workdir, None);
    assert_ne!(
        Path::new(&scratch_root),
        super::default_scratch_root(root),
        "fixture bug: the configured workdir must move the scratch root off the default"
    );
    (relocated, scratch_root)
}

/// A machine-global instance registry under a fresh `XDG_STATE_HOME` holding one entry for
/// `project` at `root` (its local store under `root/.rigger/events.db`) last heard from at
/// `heartbeat_ms`; returns the state home and the entry's file.
pub fn seed_registry(project: &str, root: &str, heartbeat_ms: u64) -> (tempfile::TempDir, PathBuf) {
    let state_home = tempfile::tempdir().expect("create XDG_STATE_HOME");
    let inst = rigger::registry::Instance {
        project: project.to_string(),
        root: root.to_string(),
        store: rigger::registry::StoreIdentity::Local {
            path: format!("{root}/{RIGGER_DIR}/events.db"),
        },
        heartbeat_ms,
        writer: rigger::registry::Writer::Driver,
    };
    let entry = rigger::registry::write(&rigger::registry::instances_dir(state_home.path()), &inst)
        .expect("seed a registry entry");
    (state_home, entry)
}

/// The current time as nanoseconds since the Unix epoch.
pub fn now_nanos() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as i64
}

/// The payload of one extracted code entity, `alpha` in `src/a.rs`.
pub fn code_entity() -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "file": "src/a.rs", "name": "alpha", "kind": "function", "line": 1, "lang": "rust",
    }))
    .unwrap()
}

/// One pre-ledger batch of `src/a.rs` at the generation `h1`, valid from `secs`: the entity
/// `name` under `gc/src/a.rs@h1#0`, then an edge of that name under `gc/src/a.rs@h1#1`. The
/// derived reset rewrites the first row of the latest recording of this batch into its ledger
/// entry and deletes the rest.
pub fn pre_ledger_batch(name: &str, secs: u64) -> [Event; 2] {
    let entity = serde_json::json!({
        "file": "src/a.rs", "name": name, "kind": "function", "line": 1, "lang": "rust",
        "fresh": true,
    });
    let edge = serde_json::json!({ "file": "src/a.rs", "name": name, "lang": "rust" });
    [
        keyed(
            rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
            serde_json::to_vec(&entity).unwrap(),
            "gc/src/a.rs@h1#0",
            secs,
        ),
        keyed(
            rigger::contextgraph::TYPE_EDGE_INFERRED,
            serde_json::to_vec(&edge).unwrap(),
            "gc/src/a.rs@h1#1",
            secs,
        ),
    ]
}

/// How many duplicate re-extractions [`seed_derived_duplicates`] appends.
pub const DUP_ROUNDS: usize = 3;
/// The one replay key every duplicate [`seed_derived_duplicates`] appends shares.
pub const DUP_KEY: &str = "gc/src/a.rs@h1#0";

/// Append [`DUP_ROUNDS`] re-extractions of the same [`code_entity`] under [`DUP_KEY`] to
/// `root`'s run stream - derived duplicates a reset is expected to compact.
pub fn seed_derived_duplicates(root: &Path) {
    let mut events = Vec::with_capacity(DUP_ROUNDS);
    for r in 0..DUP_ROUNDS {
        events.push(
            Event::new(
                rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
                code_entity(),
            )
            .with_meta(rigger::ingest::META_REPLAY_KEY, DUP_KEY)
            .with_valid_from(UNIX_EPOCH + Duration::from_secs(1_000 + r as u64)),
        );
    }
    seed_pre_ledger_events(root, &events);
}

/// The `worker` agent definition (sonnet, Read/Edit) that runs without a worktree
/// (`isolation: none`).
pub const UNISOLATED_WORKER: &str =
    "---\nid: worker\nmodel: sonnet\ntools: [Read, Edit]\nisolation: none\n---\nDo the unit.\n";

/// The `worker` agent definition on the default, git-backed isolation.
pub const ISOLATED_WORKER: &str =
    "---\nid: worker\nmodel: sonnet\ntools: [Read, Edit]\n---\nDo the unit.\n";

/// A one-agent workflow fixture: the `worker` agent's definition and the `workflow.yml` body
/// that drives it. Each test file names its fixtures as constants of this type and writes them
/// with [`write_workflow_fixture`].
pub struct WorkflowFixture {
    pub worker: &'static str,
    pub body: &'static str,
}

/// Scaffold `root/.rigger` from `fixture`: its worker as `agents/worker.md` and its body as
/// `workflow.yml`.
pub fn write_workflow_fixture(root: &Path, fixture: &WorkflowFixture) {
    write_scaffold(root, &[("worker", fixture.worker)], fixture.body);
}

/// One real `rigger step` over `root`, which must exit 0 (`what` names the step in the failure);
/// returns its trimmed stdout - the step's one JSON line.
pub fn step_line(root: &Path, what: &str) -> String {
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "{what}; stderr: {err}");
    out.trim().to_string()
}

/// `rigger validate` over `root` once `rigger init` has scaffolded it and `seed` has planted what
/// the test examines; asserts both exit 0 (an advisory never fails validate) and returns
/// validate's (stdout, stderr).
pub fn validate_after_init(root: &Path, seed: impl FnOnce(&Path)) -> (String, String) {
    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    seed(root);
    let (out, err, ok) = run_rigger(root, &["validate"]);
    assert!(
        ok,
        "validate must exit 0 (an advisory never fails it); stderr:\n{err}"
    );
    (out, err)
}

/// Scaffold `root/.rigger`: each `(id, definition)` of `agents` as `agents/<id>.md`, and
/// `workflow` as its `workflow.yml`.
pub fn write_scaffold(root: &Path, agents: &[(&str, &str)], workflow: &str) {
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(rigger.join("agents")).expect("create .rigger/agents");
    for (id, definition) in agents {
        std::fs::write(rigger.join("agents").join(format!("{id}.md")), definition)
            .expect("write an agent definition");
    }
    std::fs::write(rigger.join("workflow.yml"), workflow).expect("write workflow.yml");
}

/// A project at `root` holding `workflow` and its `agents` ([`write_scaffold`]) and `text` as
/// the spec at the root-relative path `spec`.
pub fn write_spec_project(
    root: &Path,
    agents: &[(&str, &str)],
    workflow: &str,
    spec: &str,
    text: &str,
) {
    write_scaffold(root, agents, workflow);
    let spec = root.join(spec);
    std::fs::create_dir_all(spec.parent().expect("a spec path under the root"))
        .expect("create the spec's directory");
    std::fs::write(spec, text).expect("write the spec");
}

/// The grounder name [`stopping_at_the_grounder`] plants and [`assert_stopped_at_the_grounder`]
/// recognizes: one the binary's grounder registry rejects.
const REJECTED_GROUNDER: &str = "no-such-grounder";

/// `workflow`, whose grounder is `nop`, with a grounder the binary rejects instead: a run entry
/// (`rigger step`, `rigger run` on either driver, `rigger serve`) that gets past its run start -
/// its run minted or adopted - then stops selecting the grounder, before it drives an agent or
/// serves stdin, so a run start that should have refused fails its test instead of hanging it.
pub fn stopping_at_the_grounder(workflow: &str) -> String {
    let stopping = workflow.replace("grounder: nop", &format!("grounder: {REJECTED_GROUNDER}"));
    assert_ne!(
        stopping, workflow,
        "fixture bug: the workflow must name the nop grounder for the stop to replace"
    );
    stopping
}

/// The run entry whose `(stdout, stderr, success)` is `output` stopped at the grounder
/// [`stopping_at_the_grounder`] planted: it failed, its last stderr line opening with the
/// unknown-grounder clause that names the rejected grounder - never the registry's list of valid
/// names, which a new grounder extends. `what` names the entry in the failure.
pub fn assert_stopped_at_the_grounder(output: &(String, String, bool), what: &str) {
    let (out, err, ok) = output;
    let clause = format!("rigger: unknown grounder \"{REJECTED_GROUNDER}\"");
    assert!(
        !ok && err
            .lines()
            .last()
            .is_some_and(|last| last.starts_with(&clause)),
        "{what} gets past its run start and stops at the grounder; stdout:\n{out}\nstderr:\n{err}"
    );
}

/// Write a one-stage `workflow.yml` (plus its `worker` agent) under `root`, with `block`
/// appended verbatim after the stage.
pub fn write_workflow(root: &Path, block: &str) {
    let workflow = format!(
        "defaults:\n  grounder: nop\n  budget: 60\n\
         stages:\n  a:\n    agent: worker\n    on_pass: none\n\
         {block}"
    );
    write_scaffold(root, &[("worker", UNISOLATED_WORKER)], &workflow);
}

/// `rigger progress` from the courier project at `root` (under a throwaway `XDG_STATE_HOME`)
/// while THIS test process carries a well-formed but UNREACHABLE `KURRENTDB_CONN`, restored
/// afterwards. The shared `rigger_courier()` strips it from every child, so the courier must
/// resolve the fixture's local sqlite store and succeed; returns its output.
pub fn progress_under_an_ambient_kurrentdb_conn(root: &Path) -> Output {
    let state = tempfile::tempdir().expect("a temp XDG_STATE_HOME");
    let _restore = super::RestoreEnvVars::capture(&["KURRENTDB_CONN"]);
    std::env::set_var("KURRENTDB_CONN", "kurrentdb://127.0.0.1:1/");
    let out = run_rigger_in_state_home(
        root,
        state.path(),
        &["progress", "u1/impl#0", "did a thing"],
    );
    assert!(
        out.status.success(),
        "a courier spawned through the shared rigger_courier() helper must resolve the \
         fixture's local sqlite store, not attempt a real gRPC connection to whatever \
         KURRENTDB_CONN this test process's own environment carries; stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

/// Assert the courier (or run) resolved the SERVER backend: it failed inside the kurrentdb
/// adapter (the eager connect to the unreachable address) and fabricated no local sqlite event
/// log. The ABSENCE of the sqlite walk-up's `no rigger store found` is what distinguishes a
/// genuine server selection from a silent drop to the local default.
pub fn assert_selected_server(out: &Output, root: &Path, why: &str) {
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "{why}: an unreachable server must fail, never silently succeed against a local fallback; \
         stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("kurrentdb"),
        "{why}: the courier must fail INSIDE the server backend, proving it resolved the server; \
         stderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("no rigger store found"),
        "{why}: a server selection must not fall back to the local sqlite walk-up; stderr:\n{stderr}"
    );
    assert!(
        !rigger_file(root, "events.db").exists(),
        "{why}: a server selection must NOT fabricate a local .rigger/events.db"
    );
}

/// Assert the courier resolved the SQLITE backend: it took the local walk-up, which on a
/// never-initialized project refuses without reaching for any server.
pub fn assert_selected_sqlite(out: &Output, root: &Path, why: &str) {
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "{why}: a courier with no initialized local store must fail, not fabricate one; \
         stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("no rigger store found") && !stderr.contains("kurrentdb"),
        "{why}: a sqlite selection must resolve the LOCAL log (surfacing as a local-store error), \
         never a server connect; stderr:\n{stderr}"
    );
    assert!(
        !rigger_file(root, "events.db").exists(),
        "{why}: the refuse-to-fabricate guard must leave no local events.db behind"
    );
}

/// Seed `root` with a reviewless single-stage workflow: one `worker` agent, one always-passing
/// core gate, and `on_pass: merge` - the smallest workflow whose unit reaches the git integration
/// path.
pub const REVIEWLESS_GIT_UNIT_WORKFLOW: WorkflowFixture = WorkflowFixture {
    worker: "---\nid: worker\nmodel: sonnet\ntools: [Read, Edit]\n---\nDo the unit.\n",
    body: r#"defaults:
  grounder: nop
  budget: 60
gates:
  ok: { run: "true", kind: core }
stages:
  solo:
    agent: worker
    gates: [ok]
    on_pass: merge
"#,
};

/// The escalating twin of [`REVIEWLESS_GIT_UNIT_WORKFLOW`]: the same shape, but its one gate
/// always fails and `defaults.max_retries: 1` means `safety::remediate(0, 1)` escalates on the
/// FIRST failed attempt (`bounded_then_escalates` in `src/safety.rs` pins that arithmetic), so a
/// single implementer result whose gate goes red - never a park, and never a crash, which is
/// infrastructure and charges no attempt - is enough to drive the unit terminal without ever
/// integrating.
pub const REVIEWLESS_GIT_ESCALATING_UNIT_WORKFLOW: WorkflowFixture = WorkflowFixture {
    worker: ISOLATED_WORKER,
    body: r#"defaults:
  grounder: nop
  budget: 60
  max_retries: 1
gates:
  red: { run: "false", kind: core }
stages:
  solo:
    agent: worker
    gates: [red]
    on_pass: merge
"#,
};

/// Spec 88, criterion 3 (ESCALATION RESUMES) shared setup: drive `solo` to a genuine
/// terminal escalation through the REAL two-process replay lifecycle (park, then an
/// out-of-process result its gate fails), and assert the fixpoint before returning - every
/// `resume-unit` test builds on this SAME real, git-backed escalated unit.
pub fn escalate_solo_unit(root: &Path) {
    write_workflow_fixture(root, &REVIEWLESS_GIT_ESCALATING_UNIT_WORKFLOW);
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the first step must succeed; stderr: {err}");
    assert!(
        out.contains(r#""id":"solo/implementer#0""#) && out.contains(r#""done":false"#),
        "step 1 parks the implementer; got: {out:?}"
    );
    let (_o, err, ok) = run_rigger(root, &["result", "solo/implementer#0", "implemented"]);
    assert!(ok, "recording the result must succeed; stderr: {err}");
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(
        ok,
        "an escalation-fixpoint step still exits 0; stderr: {err}"
    );
    assert!(
        out.contains(r#""done":true"#) && out.contains(r#""escalated":["solo"]"#),
        "the red gate must exhaust remediation into an escalated fixpoint; got: {out:?}"
    );
}

/// Seed `<root>/.rigger/events.db` with rows in `project`'s run stream whose position order and
/// revision order DISAGREE (spec 71's corruption signature) by inserting directly - bypassing the
/// store's own always-increasing revision assignment, the only way to reach this shape. Three
/// rows land in this insertion (position) order: revision 5, then 1, then 2 - distinct values
/// (satisfying `UNIQUE(stream, revision)`, the on-disk shape a write into a compaction-opened
/// revision hole leaves) where positions 2 and 3 both carry a revision at or below the running
/// maximum (5). Each row is stamped `recorded_at` (and `valid_from`).
pub fn seed_order_signature(root: &Path, project: &str, recorded_at: i64) {
    let rigger_dir = root.join(".rigger");
    std::fs::create_dir_all(&rigger_dir).unwrap();
    let db = rigger_dir.join("events.db");
    // Open through the real store first, so the schema is laid down exactly as the binary
    // itself would lay it down.
    Store::open(db.to_str().unwrap()).unwrap();
    let stream = format!(
        "{}{}",
        Namespaced::prefix_for(project),
        rigger::conductor::STREAM
    );
    let conn = rusqlite::Connection::open(&db).unwrap();
    for revision in [5i64, 1, 2] {
        conn.execute(
            "INSERT INTO events (stream, type, id, data, meta, valid_from, recorded_at, revision)
             VALUES (?1, 'Seed', ?2, X'7b7d', '{}', ?3, ?3, ?4)",
            rusqlite::params![stream, format!("seed-{revision}"), recorded_at, revision],
        )
        .unwrap();
    }
}

/// The reclaimed byte count a `rigger reset` report states right after `marker` (`... <marker>N
/// byte(s) ...`), or `None` when the report carries no such clause.
pub fn reported_reclaimed_bytes(report: &str, marker: &str) -> Option<u64> {
    let start = report.find(marker)? + marker.len();
    let rest = &report[start..];
    let end = rest.find(" byte(s)")?;
    rest[..end].trim().parse().ok()
}

/// In a build WITHOUT the `symbols` feature (the light `--no-default-features` lane), `graph
/// --show` cannot derive a located entity's body extent (no extraction grammar is linked), so it
/// must degrade to the site header plus an explicit extent-unavailable note and NO line-numbered
/// body - never a hand-rolled lexer that would mis-read the grammars the graph ingests.
pub fn assert_light_lane_extent_note(out: &str) {
    assert!(
        out.contains("code-extraction grammar") || out.contains("`symbols` feature"),
        "the light lane names the missing extraction grammar in the extent note; got:\n{out}"
    );
    assert!(
        !out.contains(" | ") && body_line_count(out) == 0,
        "the light lane prints NO line-numbered body (extent unavailable); got:\n{out}"
    );
}

/// The line `rigger validate` warns of graph index lag on, naming `files` in sample order.
pub fn index_lag_advisory(files: &[&str]) -> String {
    format!(
        "warning: the context graph has fallen behind {} sampled file(s) it previously indexed \
         ({}). Run `rigger graph build` to refresh it.",
        files.len(),
        files.join(", ")
    )
}

/// Every graph index-lag advisory line among `stderr`, what `rigger validate` wrote there, in
/// order.
pub fn index_lag_lines(stderr: &str) -> Vec<&str> {
    lines_where(stderr, |line| line.contains("fallen behind"))
}

/// The line `rigger validate` warns of log bloat on, over a store still holding `events` derived
/// events of `identities` file identities.
pub fn bloat_advisory_naming(events: usize, identities: usize) -> String {
    format!(
        "warning: the event log still holds {events} derived events of {identities} file \
         identities; run `rigger reset --derived` to migrate them into the ledger."
    )
}

/// Every line naming `rigger reset --derived` among `stderr`, what `rigger validate` wrote there,
/// in order: the log-bloat advisory, and nothing else of validate's.
pub fn bloat_lines(stderr: &str) -> Vec<&str> {
    lines_where(stderr, |line| line.contains("rigger reset --derived"))
}

/// The lines of `text` that `keep` admits, in order: the one picker of the lines a command
/// printed that a test compares.
pub fn lines_where(text: &str, keep: impl Fn(&str) -> bool) -> Vec<&str> {
    text.lines().filter(|line| keep(line)).collect()
}
