//! Fixtures for suites that drive the compiled `rigger` binary against a throwaway project:
//! running it, seeding the stores it reads, and reading back what it wrote.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

/// A [`temp_project`] that already carries an empty `.rigger/` and no event log yet.
pub fn temp_project_with_rigger_dir() -> tempfile::TempDir {
    let dir = temp_project();
    seed_rigger_dir(dir.path());
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
    Store::open(event_log(root).to_str().expect("a utf-8 store path"))
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

/// Where the embedded sqlite event log lives for a project rooted at `root`.
pub fn event_log(root: &Path) -> PathBuf {
    root.join(".rigger").join("events.db")
}

/// Where the graph projection lives for a project rooted at `root`.
pub fn graph_db(root: &Path) -> PathBuf {
    root.join(".rigger").join("graph.db")
}

/// Run `rigger <args...>` in `cwd` and return (stdout, stderr, success).
pub fn run_rigger(cwd: &Path, args: &[&str]) -> (String, String, bool) {
    run_rigger_envs(cwd, args, &[])
}

/// Run `rigger <args...>` in `cwd` with extra environment `envs` and return
/// (stdout, stderr, success).
pub fn run_rigger_envs(cwd: &Path, args: &[&str], envs: &[(&str, &str)]) -> (String, String, bool) {
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
    // tempdir, that a running dash would otherwise pick up. Bound to `state` so the dir lives
    // until after the command runs; set before the caller's envs so the registry tests that pass
    // an explicit XDG_STATE_HOME (to read the registry back) still override it.
    let state = tempfile::tempdir().expect("create a temp XDG_STATE_HOME for the rigger run");
    cmd.env("XDG_STATE_HOME", state.path());
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("failed to spawn the rigger binary");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

/// Run `rigger <args...>` in `cwd` with the machine-global instance registry redirected into
/// the CALLER-OWNED `state_home`, so a sequence of calls reads back and re-writes the same
/// registry directory.
pub fn run_rigger_in_state_home(cwd: &Path, state_home: &Path, args: &[&str]) -> Output {
    super::rigger_courier()
        .args(args)
        .current_dir(cwd)
        // Never let a short-lived courier or driver step spawn a real dashboard under test.
        .env("RIGGER_NO_DASH", "1")
        .env("XDG_STATE_HOME", state_home)
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
    let backend = Store::open(event_log(root).to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    for &(ty, body) in events {
        store
            .append(
                rigger::conductor::STREAM,
                ExpectedRevision::Any,
                &[Event::new(ty, body.as_bytes().to_vec())],
            )
            .unwrap();
    }
}

/// Every event in `root`'s namespaced run stream, oldest first.
pub fn read_run_events(root: &Path) -> Vec<Event> {
    let backend = Store::open(event_log(root).to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    store
        .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
        .unwrap()
}

/// The graph projection of `root`'s own `.rigger/graph.db`, under its run-stream identity.
pub fn open_graph(root: &Path) -> Projector {
    let id = run_stream_identity(root);
    Projector::open(graph_db(root).to_str().unwrap(), &id).unwrap()
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
    std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
    std::fs::write(marker, b"heartbeat").unwrap();
    let stale = SystemTime::now() - Duration::from_secs(3600);
    std::fs::File::options()
        .write(true)
        .open(marker)
        .unwrap()
        .set_modified(stale)
        .unwrap();
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
    let backend = Store::open(event_log(root).to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
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
    store
        .append(rigger::conductor::STREAM, ExpectedRevision::Any, &events)
        .unwrap();
}

/// The `worker` agent definition (sonnet, Read/Edit) that runs without a worktree
/// (`isolation: none`).
pub const UNISOLATED_WORKER: &str =
    "---\nid: worker\nmodel: sonnet\ntools: [Read, Edit]\nisolation: none\n---\nDo the unit.\n";

/// The `worker` agent definition on the default, git-backed isolation.
pub const ISOLATED_WORKER: &str =
    "---\nid: worker\nmodel: sonnet\ntools: [Read, Edit]\n---\nDo the unit.\n";

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

/// Write a one-stage `workflow.yml` named `name` (plus its `worker` agent) under `root`, with
/// `block` appended verbatim after the stage.
pub fn write_workflow(root: &Path, name: &str, block: &str) {
    let workflow = format!(
        "name: {name}\n\
         defaults:\n  grounder: nop\n  budget: 60\n\
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
