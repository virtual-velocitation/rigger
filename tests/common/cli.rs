//! Fixtures for suites that drive the compiled `rigger` binary against a throwaway project:
//! running it, seeding the stores and operator instruction files it reads, and reading back what
//! it wrote.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rigger::config::RIGGER_DIR;
use rigger::contextgraph::sqlite::Projector;
#[cfg(feature = "symbols")]
use rigger::contextgraph::{Edge, Error as GraphError, Graph, Node};
use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, Event, EventStore, ExpectedRevision};
use rigger::run::{RunHeld, RunReleased, TYPE_RUN_HELD, TYPE_RUN_RELEASED};

pub use super::fixtures::keyed;
use super::fixtures::{dated_ago, run_held_event, CLEAN_CRITIQUE};

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

/// Place a DIRECTORY where `<root>/.rigger/store.conn` should be a file, so reading it as the secret
/// file yields an IO error DISTINCT from NotFound - the present-but-unreadable class. This mirrors
/// the config rung's parity test exactly and is more portable than `chmod 000`, which a test running
/// as root would bypass (root reads any file), silently defeating the guard under test.
pub fn make_store_conn_unreadable(root: &Path) {
    std::fs::create_dir(root.join(".rigger").join("store.conn"))
        .expect("place a directory where store.conn goes");
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

/// Post one `UnitProposed` decision for the currently-parked spawn `spawn`, through the
/// real `rigger emit --spawn` CLI courier (`cmd_emit`, `src/main.rs`) - the established
/// native-courier idiom for a scripted (non-MCP) agent to record a decision, threading its
/// OWN spawn id into `META_SPAWN` exactly as a live MCP-tooled planner's stamped emit does.
pub fn propose_unit_via_rigger_emit(root: &Path, spawn: &str, body: &serde_json::Value) {
    let (_out, err, ok) = run_rigger(
        root,
        &["emit", "--spawn", spawn, "UnitProposed", &body.to_string()],
    );
    assert!(
        ok,
        "rigger emit --spawn {spawn} UnitProposed {body} must succeed; stderr:\n{err}"
    );
}

/// The planner's proposal of the one [`WIDGET_UNIT`](super::fixtures::WIDGET_UNIT), covering
/// [`WIDGET_CRITERION`](super::fixtures::WIDGET_CRITERION), handed to `agent`.
pub fn widget_unit(agent: &str) -> serde_json::Value {
    serde_json::json!({
        "id": super::fixtures::WIDGET_UNIT,
        "agent": agent,
        "criterion": super::fixtures::WIDGET_CRITERION,
        "needs": [],
    })
}

/// The shell an arm of [`install_fake_claude`] runs to record `unit` as the running spawn's
/// `UnitProposed`, as a planner's bound record tool records it: the built binary's `rigger emit
/// --spawn "$RIGGER_SPAWN"`, its output kept out of the session's answer, and a refused record
/// failing the launch.
pub fn emit_unit_proposed(unit: &serde_json::Value) -> String {
    format!(
        "    '{bin}' emit --spawn \"$RIGGER_SPAWN\" UnitProposed '{unit}' >&2 || exit 1\n",
        bin = super::rigger_bin().display()
    )
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
/// `root`, uncommitted, add the linked worktree `worktree` places - nested under the project
/// ([`super::git::nested_worktree`]) or relocated outside it ([`super::git::relocated_worktree`]) -
/// which therefore checks none of that layer out, write another layer
/// (`house_and_team_operator_layer`) into the worktree's own `.rigger/instructions/`, then run the
/// reporter assertion `report` in the worktree: the reporter reads the main checkout's layer, never
/// the worktree's own.
pub fn assert_reported_from_a_worktree(
    root: &Path,
    report: Reporter,
    worktree: fn(&Path, &str) -> PathBuf,
) {
    write_operator_layer(root, &super::fixtures::shadowing_operator_layer());
    let wt = worktree(root, "rigger-wt-reporter-layer");
    write_operator_layer(&wt, &super::fixtures::house_and_team_operator_layer());
    report(&wt, &[]);
}

/// Write the named-entry fixture (`shadowing_operator_layer`) into the project at `root`, then run
/// the reporter assertion `report` there with the gate store fence
/// (`rigger::gate::STORE_FENCE_ENV`) pinned to a scratch store outside the project, as a gate's
/// own process runs a reporter: it reports the project's layer, the same lines as an unfenced run,
/// since the fence isolates the store and never names the project's root.
pub fn assert_reported_under_the_store_fence(root: &Path, report: Reporter) {
    write_operator_layer(root, &super::fixtures::shadowing_operator_layer());
    let (_scratch, fence) = scratch_store_fence();
    report(root, &[(rigger::gate::STORE_FENCE_ENV, fence.as_str())]);
}

/// A scratch directory, and the path inside it a test pins the gate store fence
/// (`rigger::gate::STORE_FENCE_ENV`) to: a store outside every project, which the binary creates.
/// The directory must outlive the process it fences.
pub fn scratch_store_fence() -> (tempfile::TempDir, String) {
    let scratch = tempfile::tempdir().expect("create the fence's scratch dir");
    let fence = scratch.path().join("store-fence");
    let fence = fence.to_str().unwrap().to_string();
    (scratch, fence)
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
/// (stdout past the honesty label, stderr, success).
pub fn run_around(cwd: &Path, seed: &str, depth: i64) -> (String, String, bool) {
    let (out, err, ok) = run_rigger(
        cwd,
        &["graph", "--around", seed, "--depth", &depth.to_string()],
    );
    (if ok { past_label(&out) } else { out }, err, ok)
}

/// `rigger <args>` at `root`, a graph read asserted to succeed, as its answer past the honesty
/// label that heads it.
pub fn graph_ok(root: &Path, args: &[&str]) -> String {
    past_label(&run_rigger_ok(root, args))
}

/// A graph answer's stdout past the honesty label that heads it (6.1), which
/// `around_prints_the_label_first` pins.
pub fn past_label(out: &str) -> String {
    let (label, answer) = out.split_once('\n').unwrap_or((out, ""));
    assert!(
        label.starts_with("describes "),
        "a graph answer opens with its honesty label; stdout:\n{out}"
    );
    answer.to_string()
}

/// Every printed `node <id> <kind>` line of a `rigger graph --around` transcript, parsed into its
/// id, so an assertion is exact (never a substring match against an edge line).
pub fn graph_node_ids(out: &str) -> Vec<String> {
    out.lines()
        .filter_map(|l| {
            l.trim_start()
                .strip_prefix("node ")
                .and_then(|rest| rest.split_whitespace().next())
                .map(str::to_string)
        })
        .collect()
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
    // Bound to `_state` so the dir lives until after the command runs.
    let (_state, child) = spawn_rigger(cwd, args, envs);
    rigger_outcome(child)
}

/// The first executable named `tool` on the ambient `PATH`, which the test needs there.
pub fn ambient_tool(tool: &str) -> PathBuf {
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .map(|d| d.join(tool))
        .find(|p| p.is_file())
        .unwrap_or_else(|| panic!("`{tool}` must be on PATH for this test"))
}

/// `dir` made a `PATH` directory holding each of `tools` as a symlink to its [`ambient_tool`], so a
/// `PATH` of `dir` finds exactly those tools whatever else the machine has installed, and no test
/// writes a file it then executes.
#[cfg(unix)]
pub fn ambient_tools_bin(dir: &Path, tools: &[&str]) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    for tool in tools {
        std::os::unix::fs::symlink(ambient_tool(tool), dir.join(tool)).unwrap();
    }
    dir.to_path_buf()
}

/// The directory holding the stand-in agent binary [`rigger_command`] puts on every `PATH`: a
/// `claude` linked to the ambient `false`, so whatever it is asked it prints nothing and fails. It
/// lives in the test target's scratch directory, made by whichever test process reaches it first
/// and shared by the rest.
#[cfg(unix)]
fn no_report_agent_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("no-login-report");
    std::fs::create_dir_all(&dir).unwrap();
    match std::os::unix::fs::symlink(ambient_tool("false"), dir.join("claude")) {
        Err(e) if e.kind() != std::io::ErrorKind::AlreadyExists => {
            panic!(
                "the stand-in agent binary links into {}: {e}",
                dir.display()
            )
        }
        _ => dir,
    }
}

/// `path` with [`no_report_agent_dir`] ahead of its first entry the ambient `PATH` holds, so the
/// rigger a test runs finds the stand-in agent binary before the machine's own, and a stand-in the
/// test put ahead of those entries before both; a `path` holding no ambient entry, one a test built
/// to hold only the tools it names, comes back whole.
#[cfg(unix)]
fn behind_the_no_report_agent(path: &str) -> std::ffi::OsString {
    let ambient: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    let mut entries: Vec<PathBuf> = std::env::split_paths(path).collect();
    let first_ambient = entries
        .iter()
        .position(|entry| !entry.as_os_str().is_empty() && ambient.contains(entry));
    if let Some(first_ambient) = first_ambient {
        entries.insert(first_ambient, no_report_agent_dir());
    }
    std::env::join_paths(entries).expect("a PATH's own entries join back into one")
}

/// The ambient `PATH` with every directory holding a `tool` removed, so a lookup of `tool` fails
/// whatever the machine running the test has installed; `git` and the coreutils stay found only
/// where no directory holding them also holds `tool`.
pub fn path_without(tool: &str) -> String {
    std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .filter(|dir| !dir.is_empty() && !Path::new(dir).join(tool).exists())
        .collect::<Vec<_>>()
        .join(":")
}

/// A spawned child the test owns: dropping the owner, on any path out of its scope and a panic's
/// unwind included, ends and reaps it through its handle ([`super::fixtures::cleanup`]), since a
/// `Child` dropped bare leaves its process running. [`OwnedChild::into_child`] hands the handle
/// back to a caller that reaps the child itself.
pub struct OwnedChild(Option<Child>);

impl OwnedChild {
    /// Own `child`.
    pub fn new(child: Child) -> Self {
        Self(Some(child))
    }

    /// The owned handle.
    pub fn child(&mut self) -> &mut Child {
        self.0
            .as_mut()
            .expect("an owned child is held until it is handed back")
    }

    /// The handle, handed back: its caller reaps the child.
    pub fn into_child(mut self) -> Child {
        self.0
            .take()
            .expect("an owned child is held until it is handed back")
    }
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        if let Some(child) = &mut self.0 {
            super::fixtures::cleanup(child);
        }
    }
}

/// [`run_rigger_envs`] bounded in time: `rigger <args...>` in `cwd` with extra environment `envs`,
/// spawned and polled up to `tries` times 25 ms apart ([`super::fixtures::wait_until_for`]),
/// `meanwhile` handed its pid at each poll while it runs. Its (stdout, stderr, success) once it
/// ends within them, or `None` once they are spent. The process is owned ([`OwnedChild`]) until it
/// ends, so once the tries are spent, or a panic in `meanwhile` unwinds out of this, it is ended
/// and reaped through its handle. Its output is read once it has ended, so it suits a command
/// whose output fits a pipe.
pub fn run_rigger_within(
    cwd: &Path,
    args: &[&str],
    envs: &[(&str, &str)],
    tries: u32,
    mut meanwhile: impl FnMut(u32),
) -> Option<(String, String, bool)> {
    let (_state, child) = spawn_rigger(cwd, args, envs);
    let pid = child.id();
    let mut child = OwnedChild::new(child);
    let ended = super::fixtures::wait_until_for(tries, || {
        meanwhile(pid);
        !matches!(child.child().try_wait(), Ok(None))
    });
    if !ended {
        return None;
    }
    Some(rigger_outcome(child.into_child()))
}

/// `rigger <args...>` in `cwd` with extra environment `envs` ([`rigger_command`]) spawned under a
/// fresh `XDG_STATE_HOME`, its stdin closed and its stdout and stderr piped as `Command::output`
/// runs a command; the state dir is returned beside the child, which it must outlive.
fn spawn_rigger(cwd: &Path, args: &[&str], envs: &[(&str, &str)]) -> (tempfile::TempDir, Child) {
    let state = tempfile::tempdir().expect("create a temp XDG_STATE_HOME for the rigger run");
    let child = rigger_command(cwd, args, envs, state.path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn the rigger binary");
    (state, child)
}

/// The (stdout, stderr, success) of the rigger run `child` ([`spawn_rigger`]), read once it ends.
fn rigger_outcome(child: Child) -> (String, String, bool) {
    let out = child
        .wait_with_output()
        .expect("wait for the rigger run to end");
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
    run_capturing_system_prompt_and_stdin(cwd, args).0
}

/// [`run_capturing_system_prompt`], returning beside that system prompt the stdin the same agent
/// was handed: the headless host's first stream-json line, or everything the cli host wrote.
pub fn run_capturing_system_prompt_and_stdin(cwd: &Path, args: &[&str]) -> (String, String) {
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
    (
        super::fixtures::captured(work.path(), "system-prompt"),
        super::fixtures::captured(work.path(), "stdin"),
    )
}

/// `rigger setup` in `cwd` with extra environment `envs`, which must succeed: its stdout.
pub fn setup_ok(cwd: &Path, envs: &[(&str, &str)]) -> String {
    let (out, err, ok) = run_rigger_envs(cwd, &["setup"], envs);
    assert!(ok, "setup must succeed; stdout: {out} stderr: {err}");
    out
}

/// `rigger <args...>` in `cwd` with extra environment `envs`, as [`run_rigger_envs`] runs it, for
/// a caller that spawns it and acts while it runs; `state` must outlive the process. Its `PATH`
/// finds the stand-in agent binary that gives no report ahead of the machine's own
/// ([`behind_the_no_report_agent`]).
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
    // No rigger a test runs reaches the machine's own agent binary: the caller's `PATH`, else the
    // ambient one, goes behind the stand-in that gives no report.
    #[cfg(unix)]
    {
        let path = envs.iter().rev().find(|(k, _)| *k == "PATH").map_or_else(
            || std::env::var("PATH").unwrap_or_default(),
            |(_, path)| path.to_string(),
        );
        cmd.env("PATH", behind_the_no_report_agent(&path));
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

/// Run `rigger <args>` with extra environment `envs` from the empty directory `dir` with `dir`
/// deleted: a spawn cannot enter a directory already gone, so a shell enters it, removes it and
/// execs the binary, with the environment [`rigger_command`] sets, and the process starts with no
/// working directory to read.
pub fn run_rigger_from_deleted(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> Output {
    let state = tempfile::tempdir().expect("create a temp XDG_STATE_HOME for the rigger run");
    let rigger = rigger_command(dir, args, envs, state.path());
    let mut shell = Command::new("sh");
    shell
        .args(["-c", r#"cd "$1" && rmdir "$1" && shift && exec "$@""#, "sh"])
        .arg(dir)
        .arg(rigger.get_program())
        .args(rigger.get_args());
    for (name, value) in rigger.get_envs() {
        match value {
            Some(value) => shell.env(name, value),
            None => shell.env_remove(name),
        };
    }
    shell.output().expect("the shell runs the rigger binary")
}

/// `rigger emit <typ> <json>` in `root`, asserting it succeeds.
pub fn emit(root: &Path, typ: &str, json: &str) {
    let (_o, err, ok) = run_rigger(root, &["emit", typ, json]);
    assert!(ok, "emit {typ} must succeed; stderr: {err}");
}

/// What the emit surface answers the type `typ`, which no agent may emit: the sentence of its
/// refusal that names the type and the types an agent may emit.
pub fn emit_refused(typ: &str) -> String {
    format!(
        "refusing to emit {typ:?}: only agent context events (DecisionMade, ReviewFinding, \
         LessonLearned, UnitProposed) may be emitted this way."
    )
}

/// The error the headless host hands back for spawn `id` when its session ended with no result
/// under `class`, `tail` the stderr tail it kept, the class marked as the conductor reads it.
pub fn no_result_failure(id: &str, class: &str, tail: &str) -> String {
    format!(
        "\u{2}{class}\u{2}claude_code driver: {id:?}: the agent stream ended with no result \
         (class {class}; stderr tail: {tail})"
    )
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

/// Record a clean critique of `spec`'s current text in the project at `root` ([`record_critique`]
/// answering [`CLEAN_CRITIQUE`]), so a new run on it under a workflow naming a critic is not
/// refused (spec 112).
pub fn record_clean_critique(root: &Path, spec: &str) {
    let out = record_critique(root, spec, CLEAN_CRITIQUE);
    assert_eq!(
        out, "{\"verdict\":\"approve\"}\n",
        "a clean critique of {spec}: no finding and an approve"
    );
}

/// Append `(type, payload)` events to the critique's own stream of the project at `root`, in the
/// wire form `rigger critique` records them in - standing in for a critique an earlier call
/// recorded.
pub fn seed_critique(root: &Path, events: &[(&str, serde_json::Value)]) {
    std::fs::create_dir_all(root.join(".rigger")).unwrap();
    let backend = Store::open(rigger_file(root, "events.db").to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &format!("{}-critique", run_stream_identity(root)));
    for (type_, payload) in events {
        store
            .append(
                rigger::conductor::STREAM,
                ExpectedRevision::Any,
                &[Event::new(*type_, serde_json::to_vec(payload).unwrap())],
            )
            .unwrap();
    }
}

/// The request of the critic's spawn at `attempt` on the text hashed `hash`, as the critique's
/// own stream records it.
fn critique_request(hash: &str, attempt: u32) -> (&'static str, serde_json::Value) {
    (
        rigger::spawn::TYPE_SPAWN_REQUESTED,
        serde_json::json!({
            "id": rigger::review::critique_spawn_id(hash, attempt),
            "unit": rigger::review::critique_unit(hash),
            "stage": "critique",
            "prompt": "p",
        }),
    )
}

/// Append the critic's spawn at `attempt` on the text hashed `hash` to the critique's own stream
/// of the project at `root` ([`seed_critique`]): its request, then its result answering `output`,
/// which is a pass of the critique when `output` is a critique (spec 123).
pub fn seed_critique_spawn(root: &Path, hash: &str, attempt: u32, output: &str) {
    seed_critique(
        root,
        &[
            critique_request(hash, attempt),
            (
                rigger::spawn::TYPE_SPAWN_RESULT,
                serde_json::json!({
                    "id": rigger::review::critique_spawn_id(hash, attempt),
                    "output": output,
                }),
            ),
        ],
    );
}

/// Append the request of the critic's spawn at `attempt` on the text hashed `hash` to the
/// critique's own stream of the project at `root` ([`seed_critique`]), with no result: a spawn
/// still in flight, which the attempt count counts (spec 123).
pub fn seed_critique_request(root: &Path, hash: &str, attempt: u32) {
    seed_critique(root, &[critique_request(hash, attempt)]);
}

/// The error the attempt count of a critique fails on when the critique's own stream holds a
/// spawn request it cannot read, ahead of the decode error that ends it: `rigger critique` and a
/// new run's refusal both read the count (spec 123).
pub const UNREADABLE_CRITIQUE_REQUEST: &str =
    "event store: a spawn request recorded on the critique stream is unreadable: ";

/// A `DecisionMade` resolving `ids` and governing `governs`, through `rigger emit`.
pub fn resolve(root: &Path, id: &str, governs: &str, ids: &[String]) {
    emit(
        root,
        "DecisionMade",
        &serde_json::json!({"id": id, "summary": "closed", "governs": [governs], "resolves": ids})
            .to_string(),
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

/// The shape of the refusal of a new run (spec 123, CLEAN MEANS EXHAUSTED): the why it gives and
/// the routes it prints after the critique route.
#[derive(Clone, Copy, Debug)]
pub enum Refusal<'a> {
    /// The spec is outside the repository: not critiqued, then the route recording a resolution
    /// once critiqued.
    Outside,
    /// The critique, exhausted or at its ceiling, holds these open BLOCKING finding ids: the ids,
    /// then the route resolving them.
    Settled(&'a [String]),
    /// The critique, neither exhausted nor at its ceiling, holds these open BLOCKING finding ids:
    /// the ids, and no other route.
    BelowCeiling(&'a [String]),
    /// The critique holds no open BLOCKING finding, and either no pass or neither is exhausted nor
    /// at its ceiling: not critiqued to exhaustion, and no other route.
    Unexhausted,
}

/// The text the refusal of a new run on `spec` by `command` ends stderr with (spec 112, Notes;
/// spec 123, CLEAN MEANS EXHAUSTED), in the shape `refusal` names: the why, the route that
/// critiques the spec, then the route that records a resolution when the shape offers one. The
/// route labels are padded to one column.
pub fn refused_new_run(command: &str, spec: &str, refusal: Refusal) -> String {
    let open = |ids: &[String]| format!("open BLOCKING findings: {}", ids.join(", "));
    let (why, resolution) = match refusal {
        Refusal::Outside => (
            "not critiqued".to_string(),
            Some((
                "or, once critiqued, record a resolution:".to_string(),
                "[<ids>]".to_string(),
            )),
        ),
        Refusal::Settled(ids) => (
            open(ids),
            Some((
                format!("{RESOLUTION_ROUTE:<33}"),
                serde_json::json!(ids).to_string(),
            )),
        ),
        Refusal::BelowCeiling(ids) => (open(ids), None),
        Refusal::Unexhausted => ("not critiqued to exhaustion".to_string(), None),
    };
    let refused = refusal_line(&format!(
        "{command}: refusing to begin a new run on {spec}: {why}"
    ));
    let mut text = format!("{refused}\n  {CRITIQUE_ROUTE}   rigger critique {spec}\n");
    if let Some((route, resolves)) = resolution {
        text.push_str(&format!(
            "  {route} rigger emit DecisionMade '{}'\n",
            printed_decision(spec, &resolves)
        ));
    }
    text
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

/// A store seeded under an owning root that has no UTF-8 path ([`seed_store`]), and that root,
/// beside the temporary directory it was made in, which removes both when dropped.
#[cfg(unix)]
pub fn store_at_a_non_utf8_root() -> (tempfile::TempDir, PathBuf) {
    use std::os::unix::ffi::OsStrExt;
    let parent = tempfile::tempdir().expect("create the parent of the non-UTF-8 root");
    let root = parent
        .path()
        .join(std::ffi::OsStr::from_bytes(b"owning-\xff-root"));
    seed_store(&root);
    (parent, root)
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

/// Pins the identity of the project rooted at `root` to `identity`: its `project.id` under the
/// state directory written as `identity` and a newline, which [`run_stream_identity`] reads back.
pub fn pin_identity(root: &Path, identity: &str) {
    std::fs::write(rigger_file(root, "project.id"), format!("{identity}\n")).unwrap();
}

/// The fold rule a `graph.db` records (its `user_version`).
pub fn user_version(db: &Path) -> i64 {
    super::fixtures::pragma_i64(db, "user_version")
}

/// Whether an open of the `graph.db` at `db` for `project` owes a rebuild.
pub fn rebuild_owed(db: &Path, project: &str) -> bool {
    use rigger::contextgraph::RecordFold;
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
    with_run_store(root, |store| append_run_events(store, events));
}

/// Append `(type, json body)` run events to `store`'s run stream, in order, one append each.
pub fn append_run_events<B: AsRef<str>>(store: &dyn EventStore, events: &[(&str, B)]) {
    for (ty, body) in events {
        store
            .append(
                rigger::conductor::STREAM,
                ExpectedRevision::Any,
                &[Event::new(*ty, body.as_ref().as_bytes().to_vec())],
            )
            .unwrap();
    }
}

/// Seed one released hold into `root`'s namespaced run stream, raised `held_ago` seconds ago and
/// released `released_ago` seconds ago, dated by `valid_from` as a run's hold intervals read it.
pub fn seed_released_hold(root: &Path, held_ago: u64, released_ago: u64) {
    let released = Event::new(
        TYPE_RUN_RELEASED,
        serde_json::to_vec(&RunReleased::default()).unwrap(),
    );
    with_run_store(root, |store| {
        store
            .append(
                rigger::conductor::STREAM,
                ExpectedRevision::Any,
                &[
                    dated_ago(run_held_event(&RunHeld::default()), held_ago),
                    dated_ago(released, released_ago),
                ],
            )
            .unwrap();
    });
}

/// The spawn [`one_unanswered_spawn`] requests and never answers.
pub const SPAWN: &str = "a/implementer#0";

/// Run `run`, whose one unit `a` is mid-spawn: [`SPAWN`] requested with the wall-clock bound
/// `bound` (`None`: unbounded) and never answered - what a driver that died mid-spawn leaves
/// behind.
pub fn one_unanswered_spawn(run: &str, bound: Option<u64>) -> Vec<(&'static str, String)> {
    let bound = bound.map_or(String::new(), |secs| format!(r#","max_wall_clock":{secs}"#));
    vec![
        (
            "RunStarted",
            format!(r#"{{"run":"{run}","criteria":["crit"]}}"#),
        ),
        (
            "UnitStarted",
            r#"{"id":"a","branch":"rigger/u/a"}"#.to_string(),
        ),
        (
            "SpawnRequested",
            format!(r#"{{"id":"{SPAWN}","unit":"a","stage":"implement","prompt":"go"{bound}}}"#),
        ),
    ]
}

/// Seeds `events` into `root`'s run stream, in order.
pub fn seed_owned(root: &Path, events: &[(&str, String)]) {
    let borrowed: Vec<(&str, &str)> = events.iter().map(|(t, b)| (*t, b.as_str())).collect();
    seed_run_events(root, &borrowed);
}

/// The traces a run's driver leaves where a liveness reader looks for them: [`SPAWN`]'s liveness
/// marker for each `(run id, secs ago)` planted under a fresh `RIGGER_TMPDIR` scratch root, and,
/// given a heartbeat age, the driver registration of the store at a root last heard from that many
/// ms back, in a fresh `XDG_STATE_HOME`'s registry.
pub struct DriverTraces {
    scratch: tempfile::TempDir,
    state_home: Option<tempfile::TempDir>,
}

impl DriverTraces {
    /// The traces `markers` and `heartbeat_ms_ago` (`None`: no registration) leave for the store
    /// at `root`.
    pub fn left(root: &Path, markers: &[(&str, u64)], heartbeat_ms_ago: Option<u64>) -> Self {
        let scratch = tempfile::tempdir().expect("create the scratch root the markers live under");
        let scratch_root = scratch.path().to_str().unwrap();
        for (run, secs_ago) in markers {
            plant_marker(
                &rigger::liveness::marker_path(scratch_root, run, SPAWN).unwrap(),
                *secs_ago,
            );
        }
        let toplevel = super::git::git_out(root, &["rev-parse", "--show-toplevel"]);
        let state_home = heartbeat_ms_ago.map(|ago| {
            super::fixtures::seed_registry(
                "proj",
                &toplevel,
                rigger::registry::now_ms() - ago,
                rigger::registry::Writer::Driver,
            )
            .0
        });
        Self {
            scratch,
            state_home,
        }
    }

    /// The env a `rigger` run reads these traces through, followed by `extra`.
    pub fn envs<'a>(&'a self, extra: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
        let mut envs = vec![("RIGGER_TMPDIR", self.scratch.path().to_str().unwrap())];
        // With no entry to seed, the binary's own fresh state home holds no registration at all.
        if let Some(home) = &self.state_home {
            envs.push(("XDG_STATE_HOME", home.path().to_str().unwrap()));
        }
        envs.extend_from_slice(extra);
        envs
    }
}

/// The age, in whole seconds, a refusal names for [`SPAWN`]'s liveness marker: the digits that
/// follow `<SPAWN> (marker touched `.
pub fn named_marker_age(err: &str) -> u64 {
    let lead = format!("{SPAWN} (marker touched ");
    let at = err
        .find(&lead)
        .unwrap_or_else(|| panic!("the refusal must name {SPAWN}'s marker age; stderr: {err:?}"))
        + lead.len();
    let digits: String = err[at..].chars().take_while(char::is_ascii_digit).collect();
    digits
        .parse()
        .unwrap_or_else(|e| panic!("{SPAWN}'s marker age {digits:?} must be whole seconds: {e}"))
}

/// The idle window, in whole seconds, as the refusal states it.
pub fn idle_window_secs() -> u64 {
    rigger::registry::DEFAULT_IDLE_MS / 1000
}

/// `f` over `root`'s own project namespace of its on-disk `.rigger/events.db` - the store the
/// binary writes, opened through the same composition the binary opens it through.
pub fn with_run_store<R>(root: &Path, f: impl FnOnce(&dyn EventStore) -> R) -> R {
    with_store_file(root, "events.db", f)
}

/// `f` over `root`'s own project namespace of its on-disk `.rigger/<file>` (`progress.db` the
/// progress log), opened as [`with_run_store`] opens the event log.
pub fn with_store_file<R>(root: &Path, file: &str, f: impl FnOnce(&dyn EventStore) -> R) -> R {
    let backend = Store::open(rigger_file(root, file).to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    f(&store)
}

/// Land one supervisor beat, now, on the progress stream of `root`'s current run: the beat
/// `rigger status` reads to tell whether a supervisor still watches a held run.
pub fn seed_a_supervisor_beat(root: &Path) {
    let run_id = current_run_id_of(root).expect("a started run to beat on");
    with_store_file(root, "progress.db", |store| {
        rigger::progress_store::append_stamped(
            store,
            &run_id,
            &rigger::progress::SupervisorBeat::default(),
            "a supervisor beat",
        )
        .unwrap();
    });
}

/// The duration text and the rest of `line` when it is `rigger status`'s HELD line, `HELD
/// <duration>s - <reason>; <what ends it>`, else `None`: how a binary test reads the one part of
/// the line the wall clock decides apart from the part it pins.
pub fn held_line_parts(line: &str) -> Option<(&str, &str)> {
    line.strip_prefix("HELD ")?.split_once("s - ")
}

/// Every `SpawnLaunched` record among `events`, each with the run it is stamped with ("" for
/// none), in their order.
pub fn spawn_launches(events: &[Event]) -> Vec<(String, rigger::progress::SpawnLaunched)> {
    events
        .iter()
        .filter(|e| e.type_ == rigger::progress::TYPE_SPAWN_LAUNCHED)
        .map(|e| {
            let run = e.meta.get(rigger::run::META_RUN_ID).cloned();
            (
                run.unwrap_or_default(),
                serde_json::from_slice(&e.data).unwrap(),
            )
        })
        .collect()
}

/// Append `event` to `root`'s run stream through a bare store, a hand beside the run.
pub fn seed_on_the_run(root: &Path, event: Event) {
    with_run_store(root, |store| {
        store
            .append(
                rigger::conductor::STREAM,
                rigger::eventstore::ExpectedRevision::Any,
                &[event],
            )
            .unwrap();
    });
}

/// Append a `RunHeld` to `root`'s run that no fold applies, so no read proves the run unheld.
pub fn seed_a_hold_no_fold_applies(root: &Path) {
    seed_on_the_run(root, Event::new(TYPE_RUN_HELD, b"not a hold".to_vec()));
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

/// The position of the newest event in `root`'s namespaced run stream ([`read_run_events`]).
pub fn run_log_head(root: &Path) -> u64 {
    read_run_events(root).last().unwrap().position
}

/// The id of the run [`rigger::run::current_run_id`] resolves over `root`'s own run stream
/// ([`read_run_events`]), `None` before any run started.
pub fn current_run_id_of(root: &Path) -> Option<String> {
    rigger::run::current_run_id(&read_run_events(root))
}

/// Seeds a REAL run in `root`'s own `.rigger/events.db`, through the SAME production
/// authority `RunCtx` itself calls before threading the result onto every spawn's
/// `opts.run_id` (`conductor.rs`'s `crate::run_store::ensure_started`) - returns the id it
/// mints. Namespaced identically to [`current_run_progress_events`]'s own
/// [`run_stream_identity`], so this writes into the exact stream `rigger hook stop-failure`'s own
/// cold `runscope::current_run_id` read (`cli/guard.rs::cmd_hook_stop_failure`) will independently
/// see.
pub fn seed_a_run(root: &Path) -> String {
    let backend = Store::open(
        root.join(".rigger/events.db")
            .to_str()
            .expect("a utf-8 store path"),
    )
    .expect("the event log opens");
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    rigger::run_store::ensure_started(&store, &["a criterion".to_string()])
        .expect("a fresh run mints")
}

/// Every event of `type_` of the current run in `root`'s own `.rigger/progress.db`, read directly
/// (never through a command under test) so an assertion is independent of it: the run's id
/// cold-read from `events.db` (empty before any run), then that run's progress stream, under the
/// same [`Namespaced`] scoping the binary writes through ([`with_store_file`]).
pub fn current_run_progress_events(root: &Path, type_: &str) -> Vec<Event> {
    let run_id = current_run_id_of(root).unwrap_or_default();
    with_store_file(root, "progress.db", |store| {
        rigger::progress::read_run(store, &run_id)
    })
    .expect("the progress stream reads")
    .into_iter()
    .filter(|e| e.type_ == type_)
    .collect()
}

/// Every record of type `T` of the current run in `root`'s own `.rigger/progress.db`
/// ([`current_run_progress_events`]), in append order.
pub fn recorded_progress<T>(root: &Path) -> Vec<T>
where
    T: rigger::progress::RunStamped + serde::de::DeserializeOwned,
{
    current_run_progress_events(root, T::EVENT_TYPE)
        .iter()
        .map(|e| serde_json::from_slice(&e.data).expect("a well-formed progress record"))
        .collect()
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

/// The file the one decision a [`governed_project`] holds governs.
pub const GOVERNED_FILE: &str = "src/reader.rs";

/// An alias the CLI's failing-read tests define of [`GOVERNED_FILE`].
pub const GOVERNED_ALIAS: &str = "src/aka.rs";

/// A [`temp_project`] whose own graph holds one decision governing [`GOVERNED_FILE`].
pub fn governed_project() -> tempfile::TempDir {
    let dir = temp_project();
    seed_rigger_dir(dir.path());
    super::fixtures::apply_decision(
        &open_graph(dir.path()),
        1,
        "d-reader",
        "governs the reader",
        &[GOVERNED_FILE],
        "",
    );
    dir
}

/// The error `rigger graph --around <seed> --depth 1` fails with over a [`governed_project`]
/// once `sql` ran on its graph, the command printing nothing on stdout and its stderr that error
/// alone, behind the opening of a graph error's [`refusal_line`] ([`super::fixtures::graph_error`]).
pub fn around_fails_after(sql: &str, seed: &str) -> String {
    let dir = governed_project();
    rigger_db(dir.path(), "graph.db")
        .execute_batch(sql)
        .unwrap();
    let (out, err, ok) = run_around(dir.path(), seed, 1);
    assert!(
        out.is_empty() && !ok,
        "the around fails printing nothing; stderr:\n{err}"
    );
    err.strip_prefix(&refusal_line(&super::fixtures::graph_error("")))
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

/// Every line `rigger graph --show <query>` prints over `root` past the honesty label, in order.
pub fn show_lines(root: &Path, query: &str) -> Vec<String> {
    past_label(&run_rigger_ok(root, &["graph", "--show", query]))
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

/// `rigger graph --show <file>::<path>`'s stdout past the honesty label over `root` seeded as a project whose working
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
    super::fixtures::commit_tree(root);
    show_planted(root, file, path, kind, line, lang)
}

/// `rigger graph --show <file>::<path>`'s stdout past the honesty label once the graph records
/// the definition at the id path `path` of `file`, named by the text after the path's last `::`,
/// of kind `kind` at `line` in `lang`.
fn show_planted(root: &Path, file: &str, path: &str, kind: &str, line: u64, lang: &str) -> String {
    let name = super::last_path_segment(path);
    super::fixtures::apply_json(
        &open_graph(root),
        1,
        rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
        super::fixtures::path_entity_json(file, name, path, kind, line, lang),
    );
    past_label(&run_rigger_ok(
        root,
        &[
            "graph",
            "--show",
            &rigger::contextgraph::query::entity_id(file, path),
        ],
    ))
}

/// The fold state of the graph at `graph_db` that decides how FUTURE events fold (spec 101, "the
/// identity"): the node assertions each record holds. Everything else in the file is history a
/// compacted log no longer replays.
pub fn fold_state(graph_db: &Path) -> Vec<String> {
    let conn = rusqlite::Connection::open(graph_db).unwrap();
    [
        "SELECT project, identity, generation, node_id, kind, attrs FROM node_assertions
       ORDER BY project, identity, node_id",
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
    let graph = Projector::open(graph_db.to_str().unwrap(), project).unwrap();
    (super::fixtures::whole_json(&graph), fold_state(graph_db))
}

/// Whether `db`'s applied ledger records `position`.
pub fn applied(db: &Path, position: u64) -> bool {
    exists(
        db,
        "SELECT EXISTS (SELECT 1 FROM applied WHERE position = ?1)",
        position,
    )
}

/// Whether the graph file at `db` records each of `positions` applied ([`applied`]), in order.
pub fn applied_each(db: &Path, positions: &[u64]) -> Vec<bool> {
    positions.iter().map(|&at| applied(db, at)).collect()
}

/// The position and type of each event of `root`'s run log ([`run_log`]) that its `graph.db`'s
/// applied ledger does not record ([`applied`]), in log order: empty once every event is folded.
pub fn unfolded_run_events(root: &Path) -> Vec<(u64, String)> {
    let graph_db = rigger_file(root, "graph.db");
    run_log(root)
        .into_iter()
        .filter(|(position, _)| !applied(&graph_db, *position))
        .collect()
}

/// What `rigger setup` prints before it rebuilds a `graph.db` that owes its rebuild only because
/// it misses an event the log holds.
pub const LOST_FOLD_REBUILD_LINE: &str = "rebuilding graph.db from the event log: it misses an \
     event the log holds, so the log's live selection is refolded once";

/// What `rigger setup` prints before it rebuilds a `graph.db` that owes its rebuild only because
/// it was folded under an older fold rule.
pub const OLDER_RULE_REBUILD_LINE: &str = "rebuilding graph.db from the event log: it was folded \
     under an older fold rule, so the log's live selection is refolded once";

/// What `rigger setup` prints before it rebuilds a `graph.db` that owes its rebuild for both
/// causes: it was folded under an older fold rule, and it misses an event the log holds.
pub const BOTH_CAUSES_REBUILD_LINE: &str = "rebuilding graph.db from the event log: it was folded \
     under an older fold rule, and it misses an event the log holds, so the log's live selection \
     is refolded once";

/// What a command or tool whose answer depends on the fold answers after its name when it refuses
/// because `graph.db` owes its rebuild
/// ([`rebuild_owed_refusal`](rigger::contextgraph::rebuild_owed_refusal)): spec 124's MARK LINES
/// text, its one cause the older fold rule the file records - the lost fold is the seeding's
/// verdict, never named here.
pub const REBUILD_OWED_TEXT: &str = "graph.db owes one rebuild from the event log (it was folded \
     under an older fold rule) - run `rigger setup` to rebuild it";

/// The arguments of `rigger dash --export` writing its snapshot to `snapshot.html` in the
/// project root.
pub const DASH_SNAPSHOT_ARGS: [&str; 3] = ["dash", "--export", "snapshot.html"];

/// What `rigger setup` prints once its rebuild is in place.
pub const REBUILT_LINE: &str = "rebuilt graph.db from the event log";

/// The progress line `rigger setup` prints when its rebuild has folded `events` events and reached
/// the log's head at position `head`.
pub fn rebuilt_through_the_head(events: usize, head: u64) -> String {
    format!("rebuilt {events} events, through position {head} of {head} (100%)")
}

/// What `rigger setup` prints once it has pruned `nodes` dead-run nodes and reclaimed `edges`
/// superseded edges from the graph it rebuilt.
pub fn pruned_from_the_rebuilt_graph(nodes: usize, edges: usize) -> String {
    format!(
        "pruned {nodes} dead-run node(s) and reclaimed {edges} superseded edge(s) from the rebuilt \
         graph"
    )
}

/// How the stdout of a `rigger emit` that appended its event and folded it into `graph.db` ends.
pub const EMIT_FOLDED_ENDING: &str = " and folded it into the context graph\n";

/// The stdout of a `rigger emit` that appended its `DecisionMade` at `position` and folded it into
/// `graph.db` ([`EMIT_FOLDED_ENDING`]).
pub fn emit_folded_line(position: u64) -> String {
    format!("emitted DecisionMade (position {position}){EMIT_FOLDED_ENDING}")
}

/// What `rigger setup` prints, exiting zero, when the transaction that seeds the `graph.db` cursor
/// meets another writer's lock past the busy timeout (spec 124, CATCH-UP LINES).
pub const SEEDING_SKIPPED_LINE: &str = "graph.db catch-up was not started: another writer holds \
     the write lock - the next rigger setup starts it";

/// Backdate every event `root`'s `.rigger/events.db` holds past the catch-up horizon
/// ([`super::fixtures::past_the_horizon`]), through [`backdate_the_log_by`] (spec 124, HORIZON IN
/// TESTS): what a binary test whose `rigger setup` must rebuild over a hole below an applied
/// position calls before that setup.
pub fn backdate_the_log(root: &Path) {
    backdate_the_log_by(root, super::fixtures::past_the_horizon());
}

/// Record every event `root`'s `.rigger/events.db` holds `age` earlier than it was, through a
/// connection of its own (spec 124, HORIZON IN TESTS): how a binary test ages the events whose
/// store stamps `rigger setup`'s seeding reads.
pub fn backdate_the_log_by(root: &Path, age: Duration) {
    backdate_store_file_by(root, "events.db", age);
}

/// Record every event `root`'s `.rigger/<file>` holds `age` earlier than it was, through a
/// connection of its own: how a binary test ages the store stamps a command reads, the event log's
/// ([`backdate_the_log_by`]) or the progress log's supervisor beats.
pub fn backdate_store_file_by(root: &Path, file: &str, age: Duration) {
    rigger_db(root, file)
        .execute(
            "UPDATE events SET recorded_at = recorded_at - ?1",
            [age.as_nanos() as i64],
        )
        .unwrap();
}

/// Remove the store file `name` under `root`'s `.rigger` and the `-wal` and `-shm` files sqlite
/// keeps beside it, each where it exists: a store file an operator deletes before the next `rigger
/// setup`.
pub fn remove_the_store_file(root: &Path, name: &str) {
    for file in [
        name.to_string(),
        format!("{name}-wal"),
        format!("{name}-shm"),
    ] {
        let path = rigger_file(root, &file);
        if path.exists() {
            std::fs::remove_file(path).unwrap();
        }
    }
}

/// The lines of `rigger setup`'s stdout `out` that report how it brought `graph.db` up to the log:
/// the cause it rebuilds for, that it rebuilt, how its catch-up ended or that it was not started,
/// and what the file passed over, in the order printed.
pub fn rebuild_lines(out: &str) -> Vec<String> {
    out.lines()
        .filter(|l| {
            l.starts_with("rebuilding graph.db")
                || l.starts_with("rebuilt graph.db")
                || l.starts_with("caught graph.db up")
                || l.starts_with("graph.db catch-up")
                || l.starts_with("passed over ")
        })
        .map(str::to_string)
        .collect()
}

/// The [`rebuild_lines`] of a `rigger setup` that printed `cause` before it rebuilt a `graph.db`
/// and passed nothing over: that line, then that it rebuilt.
pub fn rebuilt_for(cause: &str) -> Vec<String> {
    vec![cause.to_string(), REBUILT_LINE.to_string()]
}

/// The [`rebuilt_for`] lines of a `rigger setup` that rebuilt a `graph.db` missing an event the
/// log holds: the lost-fold cause, then that it rebuilt.
pub fn rebuilt_for_a_lost_fold() -> Vec<String> {
    rebuilt_for(LOST_FOLD_REBUILD_LINE)
}

/// The [`rebuild_lines`] of a `rigger setup` that rebuilt a `graph.db` missing an event the log
/// holds ([`rebuilt_for_a_lost_fold`]) and that then counts `rejected` rejected payloads passed
/// over ([`passed_over_line`]).
pub fn rebuilt_passing_over(rejected: usize) -> Vec<String> {
    let mut lines = rebuilt_for_a_lost_fold();
    lines.push(passed_over_line(rejected));
    lines
}

/// The line `rigger setup` prints once its graph step ends while `graph.db` counts events passed
/// over, by cause (spec 124, CATCH-UP LINES): `rejected` rejected payloads, counted by cause and in
/// total ([`super::fixtures::passed_over_counts`]).
pub fn passed_over_line(rejected: usize) -> String {
    let (n, by_cause) = super::fixtures::passed_over_counts(rejected);
    format!(
        "passed over {n} event(s) the fold could not fold {by_cause}, recorded as folded - {}",
        super::fixtures::RETRIED_BY_A_LATER_RULE
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

/// The graph projection of `root`'s own `.rigger/graph.db`, under its run-stream identity, folding
/// the log the binary's open of it folds there ([`local_log_identity`]).
pub fn open_graph(root: &Path) -> Projector {
    let id = run_stream_identity(root);
    Projector::open(rigger_file(root, "graph.db").to_str().unwrap(), &id)
        .unwrap()
        .with_store_identity(local_log_identity(root))
}

/// The identity of `root`'s own `.rigger/events.db` as a binary run in `root` records it beside a
/// graph's cursor: the path under the directory the binary's working directory resolves `root` to.
pub fn local_log_identity(root: &Path) -> String {
    let resolved = root.canonicalize().unwrap();
    rigger::registry::StoreIdentity::Local {
        path: rigger_file(&resolved, "events.db")
            .to_string_lossy()
            .into_owned(),
    }
    .log_identity()
}

/// [`open_graph`] folding forward from `root`'s own project namespace of its `.rigger/events.db`
/// ([`Projector::with_log`]), as every open of the binary that folds is wired.
pub fn open_graph_with_its_log(root: &Path) -> Projector {
    let (events_db, id) = (rigger_file(root, "events.db"), run_stream_identity(root));
    open_graph(root).with_log(move || {
        let backend = Store::open(events_db.to_str().unwrap())
            .map_err(|e| rigger::contextgraph::Error(e.to_string()))?;
        Ok(std::sync::Arc::new(Namespaced::owning(
            std::sync::Arc::new(backend),
            &id,
        )))
    })
}

/// What `rigger graph build` leaves a project, read as the binary's readers read it: the record
/// half its `.rigger/graph.db` holds, composed with the view of the head the build advanced the
/// code store to.
#[cfg(feature = "symbols")]
pub struct BuiltGraph {
    pub records: Projector,
    pub view: rigger_store::code::Snapshot,
}

#[cfg(feature = "symbols")]
impl BuiltGraph {
    /// The project at `root` as its last `graph build` left it: the code store is read, never
    /// advanced, so a build that left it behind the head fails here.
    pub fn of(root: &Path) -> Self {
        let opened = rigger::code_store::CodeStore::in_repository(root)
            .open(rigger_store::code::Mode::Read)
            .expect("the code store opens");
        assert!(
            opened.behind.is_none(),
            "graph build advanced the code store to the head: {:?}",
            opened.behind.map(|b| b.to_string())
        );
        Self {
            records: open_graph(root),
            view: opened.view,
        }
    }

    /// Every node and edge the composed graph serves ([`Self::composed`]).
    pub fn whole(&self) -> Result<Graph, GraphError> {
        rigger::contextgraph::Neighbors::whole(&self.composed())
    }

    /// The record half composed with the code store's view.
    pub fn composed(&self) -> rigger::composed::Composed<'_> {
        super::fixtures::view_graph(&self.records, &self.view)
    }
}

#[cfg(feature = "symbols")]
impl rigger::contextgraph::Neighbors for BuiltGraph {
    fn node(&self, id: &str) -> Result<Option<Node>, GraphError> {
        self.composed().node(id)
    }
    fn out(&self, id: &str) -> Result<Vec<Edge>, GraphError> {
        self.composed().out(id)
    }
    fn incoming(&self, id: &str) -> Result<Vec<Edge>, GraphError> {
        self.composed().incoming(id)
    }
    fn members(&self, file: &str) -> Result<Vec<Node>, GraphError> {
        self.composed().members(file)
    }
    fn leaf_nodes(&self, leaf: &str) -> Result<Vec<Node>, GraphError> {
        self.composed().leaf_nodes(leaf)
    }
    fn of_kind(&self, kind: &str) -> Result<Vec<Node>, GraphError> {
        self.composed().of_kind(kind)
    }
    fn unresolved(&self, node: &Node) -> Result<Vec<String>, GraphError> {
        self.composed().unresolved(node)
    }
    fn aliases(&self) -> Result<Vec<(String, String)>, GraphError> {
        self.composed().aliases()
    }
    fn whole(&self) -> Result<Graph, GraphError> {
        rigger::contextgraph::Neighbors::whole(&self.composed())
    }
}

/// The graph `rigger graph build` leaves a fresh [`temp_project`] holding each of `files`, a
/// `(root-relative path, source text)` pair ([`BuiltGraph`]); the project lives as long as the
/// returned directory.
#[cfg(feature = "symbols")]
pub fn built_graph(files: &[(&str, &str)]) -> (tempfile::TempDir, BuiltGraph) {
    let dir = temp_project();
    let graph = rebuilt_graph(dir.path(), files);
    (dir, graph)
}

/// The graph `rigger graph build` leaves the project at `root` once each of `files`, a
/// `(root-relative path, source text)` pair, is written and committed there ([`BuiltGraph`]): a
/// first build, or a rebuild over the files' next generation.
#[cfg(feature = "symbols")]
pub fn rebuilt_graph(root: &Path, files: &[(&str, &str)]) -> BuiltGraph {
    super::fixtures::write_files(root, files);
    build_head(root);
    BuiltGraph::of(root)
}

/// `rigger graph build` over the project at `root` once its worktree is committed, so the code
/// store's head holds the source the build reads.
pub fn build_head(root: &Path) -> String {
    super::fixtures::commit_tree(root);
    run_rigger_ok(root, &["graph", "build"])
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
#[cfg(feature = "symbols")]
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

/// The line `rigger reset --derived` prints, ended, for a delete of `deleted` derived events.
pub fn deleted_line(deleted: usize) -> String {
    format!("reset --derived: deleted {deleted} derived event(s) from the event log\n")
}

/// The line `rigger reset --derived` prints, ended, over a store holding no derived event.
pub const NOTHING_TO_DELETE_LINE: &str = "reset --derived: no derived event to delete\n";

/// The `--derived` line bare `rigger reset` prints over a store holding `events` derived events.
pub fn derived_menu_line_naming(events: usize) -> String {
    format!(
        "--derived: {events} derived event(s) to delete from the event log; rerun `rigger reset \
         --derived` to delete them"
    )
}

/// The `--derived` line bare `rigger reset` prints over a store holding no derived event.
pub const NOTHING_TO_DELETE_MENU_LINE: &str = "--derived: no derived event to delete";

/// The `--derived` lines of what bare `rigger reset` printed, in order.
pub fn derived_menu_lines(stdout: &str) -> Vec<&str> {
    lines_where(stdout, |line| line.starts_with("--derived:"))
}

/// The decision rows of what `rigger peers` printed, in order.
pub fn decision_lines(stdout: &str) -> Vec<&str> {
    lines_where(stdout, |line| line.starts_with("decision "))
}

/// The line `rigger reset --derived` prints, ended, when the log's file held no free page.
pub const LOG_LEFT_AS_IT_STANDS_LINE: &str =
    "reset --derived: the log file holds no reclaimable free page, so it was left as it stands \
     rather than rewritten to reclaim nothing\n";

/// The line `rigger reset --derived` prints, ended, for a rewrite that reclaimed `bytes`.
pub fn reclaimed_line(bytes: u64) -> String {
    format!("reset --derived: compacted the log file and reclaimed {bytes} byte(s) on disk\n")
}

/// The line the binary ends a failed command's stderr with: `rigger: ` before the `error` the
/// command failed on.
pub fn refusal_line(error: &str) -> String {
    let mut line = String::from("rigger: ");
    line.push_str(error);
    line
}

/// The line a rigger command that exited successfully when `ok` says so ended its stderr `err`
/// with, when it failed - a [`refusal_line`] for an error it refused on - and `None` when it
/// succeeded.
pub fn failure_line(ok: bool, err: &str) -> Option<String> {
    (!ok).then(|| err.lines().last().unwrap_or_default().to_owned())
}

/// The line a command that resolves the fan-out helpers fails on when the operator's copy `file`
/// under the checkout `root`, as the command reaches that root, is the
/// [`NAMELESS_HELPER_COPY`](super::fixtures::NAMELESS_HELPER_COPY).
pub fn nameless_copy_refusal(root: &Path, file: &str) -> String {
    let copy = root.join(".claude/agents").join(file);
    format!("rigger: hooks: helper agent {}: no name", copy.display())
}

/// The [`refusal_line`] `rigger setup` ends its stderr with when its failure is the owed mark
/// beside `graph.db` it cannot remove - a directory stands at its path
/// ([`unremovable_mark_beside`](super::fixtures::unremovable_mark_beside)) - naming the mark as
/// setup reaches it, under the project root's rigger dir; never once its catch-up stopped at a
/// failure, whose stop line ends its stderr instead.
pub fn unremovable_mark_refusal() -> String {
    refusal_line(&super::fixtures::directory_refusal(
        super::fixtures::owed_mark_in(Path::new(RIGGER_DIR)).display(),
    ))
}

/// The [`refusal_line`] a command ends its stderr with when it refuses the store at `root` for
/// the scratch root it cannot resolve, the store's owning root having no UTF-8 path
/// ([`super::fixtures::unresolved_scratch_root_refusal`]).
pub fn unresolved_scratch_root_refusal_line(root: &Path) -> String {
    refusal_line(&super::fixtures::unresolved_scratch_root_refusal(
        &root.join(RIGGER_DIR),
    ))
}

/// The [`refusal_line`] a command refused at `graph.db.lock` ends its stderr with, whoever holds
/// the lock: the graph error [`rigger::contextgraph::sqlite::REBUILD_IN_PROGRESS`].
pub fn rebuild_in_progress_line() -> String {
    let refused = super::fixtures::graph_error(rigger::contextgraph::sqlite::REBUILD_IN_PROGRESS);
    refusal_line(&refused)
}

/// The whole stderr of `rigger reset --derived` on a project configured for the server-backed
/// store, where the migration does not run: one [`refusal_line`], ended.
pub fn server_backed_derived_refusal() -> String {
    refusal_line(
        "reset --derived: the delete removes rows of the event log and vacuums the file, which is \
         a mechanic of the embedded .rigger/events.db store; this project is configured for the \
         server-backed store, where the delete does not run. Re-run it against a project on the \
         sqlite backend. Refusing rather than reporting a delete that did not happen.",
    ) + "\n"
}

/// The [`refusal_line`] `rigger reset --derived` ends its stderr with over a rebuild left
/// unfinished.
pub fn unfinished_rebuild_refusal() -> String {
    refusal_line(
        "reset --derived: a rebuild of graph.db was left unfinished - run `rigger setup` to finish \
         it",
    )
}

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

/// The [`site_fields`](rigger::contextgraph::site_fields) entries of `site` after the CLI's
/// print transform (spec 119, SHOW IS ONE RENDERER), each string value's newlines spaces - the
/// pairs [`printed_site_entries`] reads back.
pub fn site_entries_as_printed(site: &rigger::contextgraph::EntitySite) -> Vec<(String, String)> {
    rigger::contextgraph::site_fields(site)
        .into_iter()
        .map(|(key, value)| {
            let text = match value {
                serde_json::Value::String(text) => text.replace('\n', " "),
                number => number.to_string(),
            };
            (key.to_string(), text)
        })
        .collect()
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

/// The `SpawnResult` body the host records on `spawn_id` once its liveness marker went stale -
/// its liveness fault, built by the product's own constructor: a diagnosis of a silent worker,
/// not the spawn's end.
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
/// in for a `rigger run` holding it for its whole duration. The lock lasts until the returned
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

/// Declare `entry`, one `key: value` line, under the `defaults:` block of the workflow a scaffold
/// writer left under `root`, which opens on a line of its own.
pub fn declare_default(root: &Path, entry: &str) {
    let path = rigger_file(root, "workflow.yml");
    let workflow = std::fs::read_to_string(&path).expect("read workflow.yml");
    let declared = workflow.replacen("defaults:\n", &format!("defaults:\n  {entry}\n"), 1);
    assert_ne!(declared, workflow, "the workflow opens a `defaults:` block");
    std::fs::write(&path, declared).expect("write workflow.yml");
}

/// Declare `defaults.grep_guard: true` in the workflow a scaffold writer left under `root`: the
/// one test-side spelling of a project that declares the session's grep guard.
pub fn declare_grep_guard(root: &Path) {
    declare_default(root, "grep_guard: true");
}

/// Declare `defaults.test_author: sdet-author` in the workflow a scaffold writer left under `root`:
/// a project whose build seam spawns its `sdet-author` agent.
pub fn declare_sdet_author(root: &Path) {
    declare_default(root, "test_author: sdet-author");
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

/// `workflow`, whose grounder is `nop`, with a grounder the binary rejects instead: a `rigger run`
/// that gets past its run start - its run minted or adopted - then
/// stops selecting the grounder, before it drives an agent, so a run start that should have refused fails its test instead of hanging it.
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
    let clause = refusal_line(&format!("unknown grounder \"{REJECTED_GROUNDER}\""));
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
/// single implementer attempt whose gate goes red - never a crash, which is infrastructure and
/// charges no attempt - is enough to drive the unit terminal without ever integrating.
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

/// Spec 88, criterion 3 (ESCALATION RESUMES) shared setup: drive `solo` to a genuine terminal
/// escalation through `rigger run` on the default scratch root - its implementer writes a change
/// in the unit worktree and the red gate fails it - and assert the escalated end state from the
/// run's output and its log before returning. Every `resume-unit` test builds on this SAME real,
/// git-backed escalated unit.
pub fn escalate_solo_unit(root: &Path) {
    write_workflow_fixture(root, &REVIEWLESS_GIT_ESCALATING_UNIT_WORKFLOW);
    let arm = "  *\"Do the unit.\"*)\n    echo \"pub fn work() {}\" >> work.rs\n    ;;\n";
    let (out, err, ok, _fake) =
        run_on_arms(root, &["run"], &super::default_scratch_root(root), arm);
    assert!(
        ok,
        "a run that ends in an escalation still exits 0; stderr: {err}"
    );
    let solo = out.lines().find(|l| l.trim_start().starts_with("solo "));
    assert!(
        solo.is_some_and(|l| l.contains("escalated"))
            && out.contains("incomplete: not every unit integrated"),
        "the red gate must exhaust remediation into an escalated end state; got: {out:?}"
    );
    assert!(
        read_run_events(root)
            .iter()
            .any(|e| e.type_ == rigger::ledger::TYPE_UNIT_ESCALATED),
        "the escalation is on the log"
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

/// The line `rigger validate` warns of log bloat on, over a store still holding `events` derived
/// events.
pub fn bloat_advisory_naming(events: usize) -> String {
    format!(
        "warning: the event log still holds {events} derived event(s); run `rigger reset \
         --derived` to delete them."
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

/// Install a fake `claude` on a fresh directory: the checked-in stream-json stand-in
/// `claude-code-arms-agent.sh`, answering each spawn from the shell `case` `arms` over its system
/// prompt (the content of the file its `--system-prompt-file` names; each arm is
/// `*MARKER*) ... ;;`, and what it prints is the session's `result`), failing loudly on any
/// prompt no arm names. Only the arms are written here ([`super::fixtures::write_claude_arms`]),
/// beside the stand-in's symlink in the directory's `bin`. Returns the directory (keep it alive for the run) and a `PATH` value that
/// resolves the stand-in first.
pub fn install_fake_claude(arms: &str) -> (tempfile::TempDir, String) {
    let fakebin = tempfile::tempdir().unwrap();
    let path_env =
        super::repo::stub_path(fakebin.path(), "claude", Some("claude-code-arms-agent.sh"));
    super::fixtures::write_claude_arms(&fakebin.path().join("bin"), arms);
    (fakebin, path_env)
}

/// An arm of [`install_fake_claude`] for the `Do the unit.` workers: the launch records, in the
/// directory of the fake, its argv, its working directory, its first user message, the
/// `KURRENTDB_CONN` it was started with (`unset` when none) and the agent streams its scratch root
/// holds while it runs, then writes the change a merging stage lands.
pub const RECORDING_WORKER_ARM: &str = r#"  *"Do the unit."*)
    printf '%s\0' "$@" > "$here/argv"
    pwd -P > "$here/cwd"
    printf '%s\n' "$first" > "$here/first-message"
    printf '%s' "${KURRENTDB_CONN-unset}" > "$here/conn"
    ls "$RIGGER_TMPDIR"/agent-stream/* > "$here/streams" || true
    echo "pub fn work() {}" > work.rs
    ;;
"#;

/// `rigger <args>` in `cwd` under the scratch root `scratch`, with [`RECORDING_WORKER_ARM`]'s fake
/// `claude` first on PATH: [`run_on_arms`]'s four values, the directory holding what the arm
/// recorded last.
pub fn run_recording(
    cwd: &Path,
    args: &[&str],
    scratch: &Path,
) -> (String, String, bool, tempfile::TempDir) {
    run_on_arms(cwd, args, scratch, RECORDING_WORKER_ARM)
}

/// `rigger <args>` in `cwd` under the scratch root `scratch`, with a fake `claude` answering from
/// `arms` ([`install_fake_claude`]) first on PATH: its stdout, stderr and success, then the
/// directory holding the fake, which keeps it alive.
pub fn run_on_arms(
    cwd: &Path,
    args: &[&str],
    scratch: &Path,
    arms: &str,
) -> (String, String, bool, tempfile::TempDir) {
    let (fakebin, path_env) = install_fake_claude(arms);
    let (out, err, ok) = run_rigger_envs(cwd, args, &fake_claude_env(&path_env, scratch));
    (out, err, ok, fakebin)
}

/// The environment a run of [`install_fake_claude`]'s fake takes: `path_env` and the scratch root
/// `scratch`.
pub fn fake_claude_env<'a>(path_env: &'a str, scratch: &'a Path) -> [(&'a str, &'a str); 2] {
    [
        ("PATH", path_env),
        ("RIGGER_TMPDIR", scratch.to_str().unwrap()),
    ]
}

/// How many 25 ms polls [`run_on_arms_beside`] gives a run: two minutes.
pub const POLLS_BESIDE_A_RUN: u32 = 4800;

/// [`run_on_arms`] with `beside` called over the fake's directory at each poll of the running
/// `rigger`, so a test acts on a live run from no thread of its own: a run still waiting past
/// [`POLLS_BESIDE_A_RUN`] is ended through its handle and panics here, never hanging the suite.
pub fn run_on_arms_beside(
    cwd: &Path,
    args: &[&str],
    scratch: &Path,
    arms: &str,
    beside: impl FnMut(&Path),
) -> (String, String, bool, tempfile::TempDir) {
    run_on_arms_beside_with(cwd, args, &[], scratch, arms, beside)
}

/// [`run_on_arms_beside`] with the extra environment `envs` set after the fake's own.
pub fn run_on_arms_beside_with(
    cwd: &Path,
    args: &[&str],
    envs: &[(&str, &str)],
    scratch: &Path,
    arms: &str,
    mut beside: impl FnMut(&Path),
) -> (String, String, bool, tempfile::TempDir) {
    let (fakebin, path_env) = install_fake_claude(arms);
    let mut all = fake_claude_env(&path_env, scratch).to_vec();
    all.extend_from_slice(envs);
    let (out, err, ok) = run_rigger_within(cwd, args, &all, POLLS_BESIDE_A_RUN, |_| {
        beside(fakebin.path())
    })
    .expect("the run ends once its holds are released");
    (out, err, ok, fakebin)
}

/// Plant a stale agent stream under `scratch`, for a run that must leave it: its path.
pub fn plant_stale_stream(scratch: &Path) -> std::path::PathBuf {
    let stale = scratch.join("agent-stream/old-run/old-spawn.0.jsonl");
    std::fs::create_dir_all(stale.parent().unwrap()).unwrap();
    std::fs::write(&stale, "stale").unwrap();
    stale
}
