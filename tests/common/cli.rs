//! Fixtures for suites that drive the compiled `rigger` binary against a throwaway project:
//! running it, seeding the stores it reads, and reading back what it wrote.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rigger::config::RIGGER_DIR;
use rigger::contextgraph::sqlite::Projector;
use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, Event, EventStore, ExpectedRevision};

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

/// Run `rigger <args...>` in `cwd` and return (stdout, stderr, success).
pub fn run_rigger(cwd: &Path, args: &[&str]) -> (String, String, bool) {
    run_rigger_envs(cwd, args, &[])
}

/// Run `rigger <args...>` in `cwd`, asserting it exits 0 (naming its stderr when it does not),
/// and return its stdout.
pub fn run_rigger_ok(cwd: &Path, args: &[&str]) -> String {
    let (out, err, ok) = run_rigger(cwd, args);
    assert!(ok, "rigger {} must exit 0; stderr:\n{err}", args.join(" "));
    out
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

/// Every event in `root`'s namespaced run stream, oldest first.
pub fn read_run_events(root: &Path) -> Vec<Event> {
    with_run_store(root, |store| {
        store
            .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
            .unwrap()
    })
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

/// The graph projection of `root`'s own `.rigger/graph.db`, under its run-stream identity.
pub fn open_graph(root: &Path) -> Projector {
    let id = run_stream_identity(root);
    Projector::open(rigger_file(root, "graph.db").to_str().unwrap(), &id).unwrap()
}

/// The number of numbered source lines (`<n> | <text>`) in a `rigger graph --show` body.
pub fn body_line_count(out: &str) -> usize {
    out.lines()
        .filter(|l| {
            l.trim_start()
                .split_once(" | ")
                .map(|(pre, _)| pre.trim().parse::<u32>().is_ok())
                .unwrap_or(false)
        })
        .count()
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

/// `secs` seconds as nanoseconds.
pub fn nanos(secs: u64) -> i64 {
    Duration::from_secs(secs).as_nanos() as i64
}

/// A `type_` event carrying `data`, replay-keyed `key` and valid from `secs` after the epoch.
pub fn keyed(type_: &str, data: Vec<u8>, key: &str, secs: u64) -> Event {
    Event::new(type_, data)
        .with_meta(rigger::ingest::META_REPLAY_KEY, key)
        .with_valid_from(UNIX_EPOCH + Duration::from_secs(secs))
}

/// The payload of one extracted code entity, `alpha` in `src/a.rs`.
pub fn code_entity() -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "file": "src/a.rs", "name": "alpha", "kind": "function", "line": 1, "lang": "rust",
    }))
    .unwrap()
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
    with_run_store(root, |store| {
        store
            .append(rigger::conductor::STREAM, ExpectedRevision::Any, &events)
            .unwrap();
    });
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
