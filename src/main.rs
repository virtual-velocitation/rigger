//! The harness CLI and the single composition root: it constructs the concrete
//! adapters (event store, agent driver, grounder, projector) and injects them into
//! the conductor, which depends only on ports. `rigger run` executes the configured
//! workflow - the agent driver (`--driver cli|workflow`) and the event store
//! (`--eventstore sqlite|kurrentdb`) are selected by flag; `rigger graph` inspects
//! the context graph; `rigger init`/`setup` scaffold a project.

use std::path::Path;
use std::process::Command;

use rigger::conductor::{self, Deps};
use rigger::config;
use rigger::contextgraph::{self, sqlite::Projector, Projection};
use rigger::driver::cli;
use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::{sqlite::Store, Direction, EventStore, Filter};
use rigger::gate::ExecRunner;
use rigger::grounder::Grounder;
use rigger::ledger::RunState;
use rigger::metrics::{self, Metrics};
use rigger::sidecar::{PeerDecision, Sidecar};
use rigger::{hooks, mcpserver, spec};

const RIGGER_DIR: &str = ".rigger";

/// The JS-driver RUNTIME files, embedded in the binary so `rigger setup` can
/// provision a per-project shim without the user cloning the repo. Only the three
/// runtime files ship: `shim.mjs` (the driver), `package.json`, and the
/// `package-lock.json` (so `npm ci` installs the exact locked tree). The dev-only
/// `mock-*`/`*.test.mjs` files are deliberately NOT embedded - they are for the
/// repo's own tests + CI, not the runtime a user runs.
const SHIM_MJS: &str = include_str!("../shim/shim.mjs");
const SHIM_PACKAGE_JSON: &str = include_str!("../shim/package.json");
const SHIM_PACKAGE_LOCK_JSON: &str = include_str!("../shim/package-lock.json");

/// The three embedded shim runtime files as (filename, contents) pairs, written
/// verbatim into `<project>/.rigger/shim/` by `provision_shim`.
const SHIM_FILES: &[(&str, &str)] = &[
    ("shim.mjs", SHIM_MJS),
    ("package.json", SHIM_PACKAGE_JSON),
    ("package-lock.json", SHIM_PACKAGE_LOCK_JSON),
];

/// The native Claude Code workflow, embedded in the binary so `rigger setup` can
/// install it into a project without the user cloning the repo. A saved Claude Code
/// workflow is a single self-contained `.js` file: Claude Code auto-discovers any
/// `.js` under `<project>/.claude/workflows/`, so writing this there makes the
/// `/rigger <spec>` workflow runnable immediately, with no registration step. The
/// workflow drives its agents through the Workflow tool and grounds / persists their
/// reasoning via `rigger ground`, `rigger emit`, and `rigger peers`.
const RIGGER_WORKFLOW: &str = include_str!("../workflows/rigger.js");

/// Where the native `/rigger` workflow is installed, relative to the project root:
/// `<root>/.claude/workflows/rigger.js`. Claude Code auto-discovers `.js` files in
/// this directory, so the workflow is runnable as `/rigger <spec>` the moment it is
/// written - no registration. Rooted at `root` so it is testable against a temp dir.
fn workflow_path(root: &Path) -> std::path::PathBuf {
    root.join(".claude").join("workflows").join("rigger.js")
}

type Res = Result<(), Box<dyn std::error::Error>>;

/// Which agent driver a `run` uses (§10): `cli` is the standalone `claude`
/// subprocess path; `workflow` is the in-Claude-Code MCP-server path.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DriverKind {
    Cli,
    Workflow,
}

/// Which event-store backend a run uses (§10): `sqlite` is the embedded default;
/// `kurrentdb` is the server backend (built only behind the `kurrentdb` feature).
#[derive(Clone, Copy, PartialEq, Eq)]
enum StoreKind {
    Sqlite,
    KurrentDb,
}

/// The parsed flags shared by `run` (and the `--driver workflow` path): which
/// driver, which event store, the connection string for the server backend, and
/// the positional spec path.
struct RunArgs {
    driver: DriverKind,
    store: StoreKind,
    conn: Option<String>,
    spec: Option<String>,
}

/// Parse `rigger run`'s flags: `--driver <cli|workflow>`, `--eventstore
/// <sqlite|kurrentdb>`, `--conn <url>`, and a single positional spec path. Unknown
/// flags and a second positional are rejected (§10).
fn parse_run_args(args: &[String]) -> Result<RunArgs, Box<dyn std::error::Error>> {
    let mut driver = DriverKind::Cli;
    let mut store = StoreKind::Sqlite;
    let mut conn = None;
    let mut spec = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--driver" => {
                i += 1;
                driver = match args.get(i).map(String::as_str) {
                    Some("cli") => DriverKind::Cli,
                    Some("workflow") => DriverKind::Workflow,
                    other => {
                        return Err(
                            format!("run: --driver expects cli|workflow, got {other:?}").into()
                        )
                    }
                };
            }
            "--eventstore" => {
                i += 1;
                store = match args.get(i).map(String::as_str) {
                    Some("sqlite") => StoreKind::Sqlite,
                    Some("kurrentdb") => StoreKind::KurrentDb,
                    other => {
                        return Err(format!(
                            "run: --eventstore expects sqlite|kurrentdb, got {other:?}"
                        )
                        .into())
                    }
                };
            }
            "--conn" => {
                i += 1;
                conn = match args.get(i) {
                    Some(c) => Some(c.clone()),
                    None => return Err("run: --conn expects a connection url".into()),
                };
            }
            flag if flag.starts_with("--") => {
                return Err(format!("run: unknown flag {flag:?}").into());
            }
            positional => {
                if spec.is_some() {
                    return Err(format!(
                        "run: unexpected second positional argument {positional:?}"
                    )
                    .into());
                }
                spec = Some(positional.to_string());
            }
        }
        i += 1;
    }
    Ok(RunArgs {
        driver,
        store,
        conn,
        spec,
    })
}

/// Construct the selected event-store backend as a boxed port (§10). `sqlite` (the
/// default) opens the embedded file under `.rigger/`; `kurrentdb` reads its
/// connection string from `--conn` or `KURRENTDB_CONN` and is built only behind the
/// `kurrentdb` feature - requesting it without the feature is a clear error, so the
/// default build stays green.
fn open_store(
    kind: StoreKind,
    conn: Option<&str>,
) -> Result<Box<dyn EventStore>, Box<dyn std::error::Error>> {
    match kind {
        StoreKind::Sqlite => Ok(Box::new(Store::open(&db_path("events.db"))?)),
        StoreKind::KurrentDb => open_kurrentdb(conn),
    }
}

#[cfg(feature = "kurrentdb")]
fn open_kurrentdb(conn: Option<&str>) -> Result<Box<dyn EventStore>, Box<dyn std::error::Error>> {
    let conn = conn
        .map(str::to_string)
        .or_else(|| std::env::var("KURRENTDB_CONN").ok())
        .ok_or(
            "run: --eventstore kurrentdb needs a connection string via --conn <url> or KURRENTDB_CONN",
        )?;
    Ok(Box::new(rigger::eventstore::kurrentdb::Store::open(&conn)?))
}

#[cfg(not(feature = "kurrentdb"))]
fn open_kurrentdb(_conn: Option<&str>) -> Result<Box<dyn EventStore>, Box<dyn std::error::Error>> {
    Err(
        "run: --eventstore kurrentdb requires the `kurrentdb` cargo feature (build with -F kurrentdb)"
            .into(),
    )
}

/// The project identity that scopes the event streams and context graph (§5.1.1,
/// R9): the basename of the git repo top-level, falling back to the current
/// directory's name, falling back to "rigger". Never empty.
fn project_identity() -> String {
    let toplevel = git_repo();
    let from_repo = Path::new(&toplevel)
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty());
    if let Some(name) = from_repo {
        return name.to_string();
    }
    std::env::current_dir()
        .ok()
        .and_then(|p| p.file_name().and_then(|n| n.to_str()).map(String::from))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "rigger".to_string())
}

fn main() {
    // Point `ort` at a CUDA-enabled ONNX Runtime `.so` to `dlopen` (it is built with
    // `load-dynamic`) BEFORE anything constructs a grounder, so the turbovec grounder
    // embeds on the GPU with no user-set env - for both the standalone binary and a
    // `cargo install`ed one. A no-op when the runtime is not found or the feature is
    // off; see `rigger::ort_runtime` for the discovery order.
    //
    // SAFETY: this is the first statement in `main`, before any thread is spawned, so
    // mutating the process environment here is sound (no concurrent env reader).
    #[cfg(feature = "turbovec")]
    unsafe {
        rigger::ort_runtime::ensure_dylib_path();
    }

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        usage();
        std::process::exit(2);
    }
    let result = match args[1].as_str() {
        "run" => cmd_run(&args[2..]),
        "serve" => cmd_serve(&args[2..]),
        "workflow" => cmd_workflow(&args[2..]),
        "graph" => cmd_graph(&args[2..]),
        "stats" => cmd_stats(&args[2..]),
        "ground" => cmd_ground(&args[2..]),
        "reindex" => cmd_reindex(&args[2..]),
        "emit" => cmd_emit(&args[2..]),
        "peers" => cmd_peers(&args[2..]),
        "validate" => cmd_validate(),
        "init" => cmd_init(),
        "setup" => cmd_setup(),
        "prime" => cmd_prime(),
        "help" | "-h" | "--help" => {
            usage();
            Ok(())
        }
        other => {
            eprintln!("rigger: unknown command {other:?}");
            usage();
            std::process::exit(2);
        }
    };
    if let Err(e) = result {
        eprintln!("rigger: {e}");
        std::process::exit(1);
    }

    // Exit WITHOUT running static/global destructors. `rigger` is a one-shot CLI: by the
    // time we get here the command has printed its output and persisted its files, so
    // there is nothing left for teardown to do that the kernel will not do for free when
    // the process dies (reclaim memory, close fds).
    //
    // Why this is not merely an optimization: with the `turbovec` feature the process has
    // built an ONNX Runtime + CUDA embedding session, and tearing that session down at
    // process exit intermittently corrupts the glibc heap ("corrupted double-linked list")
    // and aborts with exit 101. It surfaced under the full turbovec test suite - which
    // spawns many short-lived `rigger` CUDA subprocesses in sequence - at ~1-2 aborts per
    // 14 runs, only on the GPU path (CI's CPU-only build never hit it). The corruption is
    // in the CUDA/ORT teardown itself: it is absent when the session is never built,
    // reproduces only as the process exits after a successful GPU embed, and is not
    // reachable from any execution-provider or session option we can set through fastembed.
    // Skipping the teardown removes the buggy code path entirely: we flush our own buffered
    // output first (so nothing is lost), then `_exit(0)`, which terminates immediately
    // without invoking C/C++ static destructors or Rust drop glue. Verified to take the
    // full suite to zero aborts across 14x runs where the default return path did not.
    //
    // Gated to `turbovec` because that is the only build that loads the crashing runtime;
    // the default-features build always has it, and the CPU-only `--no-default-features`
    // build takes the ordinary return below (nothing to skip, and `libc` is not linked).
    #[cfg(feature = "turbovec")]
    {
        use std::io::Write;
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().flush();
        // SAFETY: `_exit` is async-signal-safe and simply terminates the process with the
        // given status. We have already flushed the only buffered writers we own (Rust's
        // line-buffered stdout/stderr); no other thread is producing output at this point
        // (the command has returned), so nothing observable is lost by not unwinding.
        unsafe { libc::_exit(0) };
    }
}

fn usage() {
    eprint!(
        "rigger - a config-driven, event-sourced multi-agent dev-loop harness\n\n\
usage:\n  \
rigger run [spec] [opts]    run the workflow (opts below)\n  \
rigger workflow [spec]      turn-key: launch the per-project Node driver, which\n                              \
spawns `rigger serve`, runs each agent via the Agent\n                              \
SDK, and drives the loop (one command; run `rigger\n                              \
setup` first - it provisions the driver in .rigger/shim/)\n  \
rigger serve [opts]         run as an MCP server the driver connects to\n  \
rigger graph --around <id>  print the context subgraph around a node\n  \
rigger stats                print the run's operator metrics: first-pass yield,\n                              \
per-gate remediation counts, escalation rate, and\n                              \
review approve/reject counts\n  \
rigger ground <query> [k]   print up to k (default 8) repo references the project's\n                              \
configured grounder finds for <query>, as `file:line: text`\n  \
rigger reindex <file>...    incrementally re-embed the named files in the project's\n                              \
persisted grounding index (the grounder's reindex), so a\n                              \
later `rigger ground` reflects just-landed changes\n  \
rigger emit <type> <json>   append {{type, data:<json>}} to the event store and fold\n                              \
it into the context graph (the CLI form of rigger_emit)\n  \
rigger peers [file ...]     print peer decisions and findings from the context\n                              \
graph, scoped to the given files (the CLI form of rigger_peers)\n  \
rigger validate             load and validate the workflow + agents\n  \
rigger init                 set up a project: scaffold .rigger/ (workflow.yml +\n                              \
an agents/ folder) and install the Claude Code\n                              \
SessionStart hook (it runs `rigger prime`)\n  \
rigger setup                full setup: everything `init` does, PLUS install the\n                              \
native /rigger Claude Code workflow (.claude/workflows/\n                              \
rigger.js) and provision the JS driver (.rigger/shim/ +\n                              \
npm install). After it: run `/rigger <spec>` in Claude\n                              \
Code (primary), or `rigger workflow` as a fallback\n  \
rigger prime                print recent decisions (what the hook runs)\n\n\
run/serve options:\n  \
--driver <cli|workflow>          cli (default): standalone claude subprocess;\n                                   \
workflow: in-Claude-Code MCP server\n  \
--eventstore <sqlite|kurrentdb>  sqlite (default): embedded file in .rigger/;\n                                   \
kurrentdb: server (needs the kurrentdb feature)\n  \
--conn <url>                     KurrentDB connection url (or set KURRENTDB_CONN)\n\n\
storage and graph live in ./.rigger/ (per project, like .git/), scoped to the\n\
project identity so one backend can hold many projects without their data mixing.\n"
    );
}

fn db_path(name: &str) -> String {
    Path::new(RIGGER_DIR)
        .join(name)
        .to_string_lossy()
        .into_owned()
}

fn cmd_run(args: &[String]) -> Res {
    let parsed = parse_run_args(args)?;
    // `--driver workflow` is the equivalent of `rigger serve`: the in-Claude-Code
    // MCP-server path. `cli` (the default) keeps the standalone subprocess path.
    match parsed.driver {
        DriverKind::Workflow => run_workflow(&parsed),
        DriverKind::Cli => run_cli(&parsed),
    }
}

/// The standalone CLI path: ground, spawn agents as `claude` subprocesses, drive
/// the DAG to integration. The store is selected by flag and wrapped in the
/// per-project namespace decorator before it is injected (§5.1.1, R9).
fn run_cli(parsed: &RunArgs) -> Res {
    let cfg = config::load(".")?;
    let criteria = load_criteria(parsed.spec.as_deref())?;
    std::fs::create_dir_all(RIGGER_DIR)?;
    // The boxed backend and its namespaced wrapper both live here, in this stack
    // frame, for the whole run: the decorator borrows the concrete store, and both
    // outlive the `conductor::run` call below.
    let backend = open_store(parsed.store, parsed.conn.as_deref())?;
    let store = Namespaced::new(backend.as_ref(), &project_identity());
    let graph = Projector::open(&db_path("graph.db"))?;
    let driver = cli::Driver::default();
    let grounder = select_grounder(&cfg.workflow.defaults.grounder)?;
    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &ExecRunner,
        repo: git_repo(),
        grounder: Some(grounder.as_ref()),
        graph: Some(&graph),
        criteria,
    };
    let rs = conductor::run(&cfg, &deps)?;
    print_run_state(&rs);
    Ok(())
}

/// The in-Claude-Code MCP-server path (`rigger serve` / `rigger run --driver
/// workflow`): the conductor orchestrates on a background thread and this thread
/// serves the MCP bridge over stdio. The store is selected by flag and wrapped in
/// the per-project namespace decorator before it is injected into BOTH the
/// conductor and the side-car (§5.1.1, R9).
fn run_workflow(parsed: &RunArgs) -> Res {
    let cfg = config::load(".")?;
    let criteria = load_criteria(parsed.spec.as_deref())?;
    std::fs::create_dir_all(RIGGER_DIR)?;
    let backend = open_store(parsed.store, parsed.conn.as_deref())?;
    let store = Namespaced::new(backend.as_ref(), &project_identity());
    let graph = Projector::open(&db_path("graph.db"))?;
    let driver = rigger::driver::workflow::Driver::new();
    let grounder = select_grounder(&cfg.workflow.defaults.grounder)?;
    let peers = rigger::sidecar::Sidecar::start(&store, 0, Filter::default())?;

    // The conductor orchestrates in the background; this thread serves the MCP
    // bridge over stdio. The shim drains spawns via rigger_next/result; closing
    // stdin ends the session.
    std::thread::scope(|s| {
        s.spawn(|| {
            let deps = Deps {
                store: &store,
                driver: &driver,
                gates: &ExecRunner,
                repo: git_repo(),
                grounder: Some(grounder.as_ref()),
                graph: Some(&graph),
                criteria,
            };
            if let Err(e) = conductor::run(&cfg, &deps) {
                eprintln!("rigger: conductor: {e}");
            }
            // Signal the run is over so an empty rigger_next reports done:true and the
            // shim exits cleanly. Set on BOTH success and error: a conductor error
            // still ends the run, and the shim must not poll forever.
            driver.finish();
        });
        // Wire the graph into the MCP server too, so a ReviewFinding (or DecisionMade)
        // an agent emits via rigger_emit folds into the graph as it lands - the
        // adversary / adjudicator, which ground afterwards, then retrieve it through
        // `graph_context` (the cross-agent memory the review tiers communicate
        // through), not via the conductor hand-threading prompts.
        let server = rigger::mcpserver::Server::new(&driver, &store, conductor::STREAM, &peers)
            .with_graph(&graph);
        let _ = server.run(std::io::stdin().lock(), std::io::stdout().lock());
    });
    Ok(())
}

fn cmd_serve(args: &[String]) -> Res {
    // `rigger serve` is the equivalent of `rigger run --driver workflow`, so it
    // shares the same flag surface (the event store and its connection string) and
    // the same composition path - it just forces the workflow driver.
    let mut parsed = parse_run_args(args)?;
    parsed.driver = DriverKind::Workflow;
    run_workflow(&parsed)
}

/// `rigger workflow [spec]` is the turn-key one-command activation of the workflow
/// driver: it execs the Node shim (`shim/shim.mjs`), which spawns `rigger serve`
/// (this same binary, via `RIGGER_BIN`), connects an MCP client to it, and drives
/// the agent loop via the Claude Agent SDK. The user runs ONE command instead of
/// hand-wiring `rigger serve` into an MCP host.
fn cmd_workflow(args: &[String]) -> Res {
    // The shim takes an optional spec path; reject extra positionals so a typo is a
    // clear error, not a silently-ignored argument.
    if args.len() > 1 {
        return Err(format!(
            "workflow: expected at most one spec path, got {} arguments",
            args.len()
        )
        .into());
    }
    let shim = locate_shim(Path::new("."))?;
    // The shim spawns `rigger serve` itself; point it at THIS binary so the driver
    // and the served conductor are always the same build (no PATH ambiguity).
    let rigger_bin = std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "rigger".to_string());

    let node = std::env::var("RIGGER_NODE").unwrap_or_else(|_| "node".to_string());
    let mut cmd = Command::new(&node);
    cmd.arg(&shim);
    if let Some(spec) = args.first() {
        cmd.arg(spec);
    }
    cmd.env("RIGGER_BIN", &rigger_bin);

    let status = cmd.status().map_err(|e| {
        format!(
            "workflow: failed to launch the Node driver ({node} {shim}): {e}. \
             Is Node installed and on your PATH? Run `rigger setup` if the JS driver's \
             dependencies are not yet installed."
        )
    })?;
    if !status.success() {
        return Err(format!("workflow: the Node driver exited unsuccessfully ({status})").into());
    }
    Ok(())
}

/// Locate the JS driver's `shim.mjs` to run, rooted at the project `root`.
///
/// `rigger workflow` runs the PER-PROJECT shim that `rigger setup` provisions
/// (`<root>/.rigger/shim/shim.mjs`), so the driver and its installed `node_modules`
/// travel with the project, not the binary. Search order:
///   1. the `RIGGER_SHIM` env override (an explicit path) - the escape hatch for a
///      custom or dev shim;
///   2. the provisioned per-project shim at `<root>/.rigger/shim/shim.mjs`.
///
/// When neither exists the error tells the user to run `rigger setup` (which
/// provisions `.rigger/shim/` and installs its deps), rather than leaving them to
/// hand-wire a shim. A `RIGGER_SHIM` override that points at a missing path is a
/// clear error, never a silent fallthrough.
fn locate_shim(root: &Path) -> Result<String, Box<dyn std::error::Error>> {
    if let Ok(explicit) = std::env::var("RIGGER_SHIM") {
        if Path::new(&explicit).exists() {
            return Ok(explicit);
        }
        return Err(format!("workflow: RIGGER_SHIM={explicit} does not exist").into());
    }
    let provisioned = shim_dir(root).join("shim.mjs");
    if provisioned.exists() {
        return Ok(provisioned.to_string_lossy().into_owned());
    }
    Err(format!(
        "workflow: the per-project JS driver is not provisioned (looked for {}). \
         Run `rigger setup` to write the shim into .rigger/shim/ and install its \
         dependencies, then re-run `rigger workflow`.",
        provisioned.display()
    )
    .into())
}

/// Extract the spec's acceptance criteria, enforcing the loop-ready gate (§8): a
/// spec with no enumerable Done-when criteria blocks until a human adds them; no
/// spec path means an unconstrained run (empty criteria).
fn load_criteria(spec_path: Option<&str>) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let Some(spec_path) = spec_path else {
        return Ok(Vec::new());
    };
    let text =
        std::fs::read_to_string(spec_path).map_err(|e| format!("read spec {spec_path}: {e}"))?;
    let criteria = spec::extract_criteria(&text);
    if criteria.is_empty() {
        return Err(format!(
            "loop-ready: spec {spec_path} has no enumerable Done-when criteria (checkbox items); add them before running"
        )
        .into());
    }
    Ok(criteria)
}

fn cmd_graph(args: &[String]) -> Res {
    let mut around = String::new();
    let mut depth: i64 = 2;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--around" => {
                i += 1;
                around = args.get(i).cloned().unwrap_or_default();
            }
            "--depth" => {
                i += 1;
                depth = args.get(i).and_then(|d| d.parse().ok()).unwrap_or(2);
            }
            _ => {}
        }
        i += 1;
    }
    if around.is_empty() {
        return Err("graph: --around <id> is required".into());
    }
    let gp = Projector::open(&db_path("graph.db"))?;
    let g = gp.subgraph(&[around.clone()], depth)?;
    println!("subgraph around {around:?} (depth {depth}):");
    for n in &g.nodes {
        println!("  node {:<24} {}", n.id, n.kind);
    }
    for e in &g.edges {
        println!("  edge {} -{}-> {}", e.from, e.rel, e.to);
    }
    if g.nodes.is_empty() {
        println!("  (nothing found; has `rigger run` been run yet?)");
    }
    Ok(())
}

/// `rigger stats` - print the operator metrics for the current project's run: the
/// implement -> review loop's first-pass yield, per-gate remediation (pass/fail)
/// counts, escalation rate, and review approve/reject counts.
///
/// Composition mirrors `run_cli` (decision `d-stats-namespace`): resolve this project's
/// identity and `.rigger/events.db` path, then delegate to [`stats_lines`], which opens
/// the db via [`Store`], wraps it in the per-project [`Namespaced`] decorator, reads the
/// conductor's run stream ([`conductor::STREAM`]) forward - the same stream and boundary
/// the conductor itself replays its run state from - and folds it through the pure
/// [`metrics::project`] read-model.
///
/// Both no-run edges (absent db, empty namespaced run stream) come back from
/// [`stats_lines`] as `None` and print the same clear "no runs yet" message instead of
/// an empty table or a panic (decision `d-stats-absent-guard`); see that function for
/// the per-edge rationale.
///
/// `rigger stats` takes no arguments; any extra argument is a clear error.
fn cmd_stats(args: &[String]) -> Res {
    if !args.is_empty() {
        return Err(format!("stats: expected no arguments, got {}", args.len()).into());
    }

    // Resolve the project identity and db path the same way every CLI command does,
    // then delegate the namespace-scoped read + no-runs decision to `stats_lines`. This
    // wrapper owns only the I/O boundary (which file, which project, and the printing);
    // the read-model edges live in the testable seam below.
    match stats_lines(&db_path("events.db"), &project_identity())? {
        Some(lines) => {
            for line in lines {
                println!("{line}");
            }
        }
        // No run to report on - absent db (never-run project) or an empty namespaced
        // run stream. One clear message for both edges.
        None => println!("{NO_RUNS_MESSAGE}"),
    }
    Ok(())
}

/// The pure read-model core of `rigger stats`: open the embedded `events.db` at `path`,
/// read `project`'s `run` stream through the per-project [`Namespaced`] decorator, and
/// fold it into the printable metric lines - returning `None` for the two "no runs yet"
/// edges so [`cmd_stats`] prints one clear message for both (decision `d-stats-read-seam`).
///
/// Split out from [`cmd_stats`] so the namespace-scoped read and its empty/absent edges
/// are unit-testable against any backing file and project name, without depending on the
/// process cwd or a real git repo for identity (which `project_identity` derives).
///
/// `None` is returned for two edges (decision `d-stats-absent-guard`):
///   1. **absent db** - a project that has never run has no `events.db`. We guard BEFORE
///      [`Store::open`], which (via `Connection::open`) would create the file and mask a
///      never-run project as an empty one. This mirrors [`cmd_prime`]'s absent-db guard.
///   2. **empty run stream** - the db exists (some other command, or another project
///      sharing the backend, created it) but *this* project's namespaced `run` stream
///      holds no events. The [`Namespaced`] read scopes to `proj-<project>-run`, so an
///      event another project wrote, or one this project wrote to a different stream,
///      does not leak into the count.
fn stats_lines(
    path: &str,
    project: &str,
) -> Result<Option<Vec<String>>, Box<dyn std::error::Error>> {
    if !Path::new(path).exists() {
        return Ok(None);
    }

    let backend = Store::open(path)?;
    let store = Namespaced::new(&backend, project);
    // The conductor projects its run state from STREAM read forward from revision 0
    // (inclusive); read the same stream the same way so the metrics fold sees exactly
    // the run the conductor drove, scoped to this project's namespace.
    let events = store.read_stream(conductor::STREAM, 0, Direction::Forward)?;
    if events.is_empty() {
        return Ok(None);
    }

    Ok(Some(format_stats(&metrics::project(&events))))
}

/// The message printed when there is no run to report on - either the project has
/// never run (no `events.db`) or its run stream is empty. Single-sourced so both
/// edges in [`cmd_stats`] stay in lock-step.
const NO_RUNS_MESSAGE: &str =
    "# Rigger: no runs recorded yet (run `rigger run` to start a run, then `rigger stats`).";

/// Render a [`Metrics`] value into the lines `rigger stats` prints, one metric group
/// per line. Split from [`cmd_stats`] (which does the I/O) so the formatting is a
/// pure function of the metrics and can be asserted in a unit test without touching
/// the filesystem.
///
/// The output reports the four required metrics:
///   - **first-pass yield** as a percentage with the clean/started fraction;
///   - **per-gate remediation counts** - one line per gate, `pass`/`fail`/`total`,
///     where `fail` is the remediation signal (sorted by gate id, stable);
///   - **escalation rate** as a percentage with the escalated/started fraction;
///   - **review approve/reject** counts.
fn format_stats(m: &Metrics) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push("run stats:".to_string());
    lines.push(format!(
        "  first-pass yield   {:.1}% ({}/{} units clean on the first pass)",
        m.first_pass_yield() * 100.0,
        m.first_pass_clean,
        m.units_started,
    ));
    lines.push(format!(
        "  escalation rate    {:.1}% ({}/{} units escalated to a human)",
        m.escalation_rate() * 100.0,
        m.units_escalated,
        m.units_started,
    ));
    lines.push(format!(
        "  review             {} approved / {} rejected",
        m.review_approve, m.review_reject,
    ));
    if m.gates.is_empty() {
        lines.push("  gates              (no gate runs recorded)".to_string());
    } else {
        lines.push("  per-gate runs (fail = remediation):".to_string());
        for (gate, counts) in &m.gates {
            lines.push(format!(
                "    {gate:<16} {} pass / {} fail / {} total",
                counts.pass,
                counts.fail,
                counts.total(),
            ));
        }
    }
    lines
}

/// `rigger ground "<query>" [<k>]` - run the project's configured grounder (the
/// same one the `run`/`serve` paths build from `defaults.grounder` via
/// [`select_grounder`]) over the repo and print up to `k` (default 8) relevant
/// references, one per line as `file:line: <text>`. Empty output when nothing is
/// relevant. This is the CLI surface a native-workflow agent (which has Bash, not
/// the MCP grounding tool) uses to ground.
fn cmd_ground(args: &[String]) -> Res {
    let query = args
        .first()
        .ok_or("ground: expected a query: rigger ground \"<query>\" [<k>]")?;
    let k: usize = match args.get(1) {
        Some(s) => s
            .parse()
            .map_err(|_| format!("ground: <k> must be a non-negative integer, got {s:?}"))?,
        None => 8,
    };
    if args.len() > 2 {
        return Err(format!(
            "ground: expected at most a query and k, got {} arguments",
            args.len()
        )
        .into());
    }
    // Honor the project's configured `defaults.grounder` when a config is present;
    // a project with no `.rigger/workflow.yml` yet falls back to the default grounder
    // (the empty name -> grep, the scaffold default), so an agent can ground before a
    // workflow is authored rather than hitting a config error.
    let name = config::load(".")
        .map(|cfg| cfg.workflow.defaults.grounder)
        .unwrap_or_default();
    let grounder = select_grounder(&name)?;
    for r in grounder.ground(query, k) {
        println!("{}:{}: {}", r.file, r.line, r.text);
    }
    Ok(())
}

/// `rigger reindex <file>...` - incrementally re-embed the named files in the
/// project's persisted grounding index. It resolves the grounder from
/// `defaults.grounder` via [`select_reindex_grounder`] (rooted at `.`) - which, unlike
/// [`select_grounder`], loads the turbovec store WITHOUT freshening the whole tree, so
/// the named files are re-embedded exactly ONCE here rather than once by a load-time
/// freshen and again by the reindex. It then calls [`Grounder::reindex`] on the changed
/// files, so the turbovec grounder drops each file's old chunks, re-embeds its current
/// content, and persists the delta to `.rigger/grounding/` - a later `rigger ground`
/// (and the review tier the workflow runs after a unit lands) then reflects the
/// just-integrated code WITHOUT re-embedding the whole repo. For the grep / nop
/// grounders `reindex` is a no-op (they re-read the tree each call), so this command is
/// harmless there. Files are repo-relative, matching how the grounder records and
/// grounds them. At least one file is required.
fn cmd_reindex(args: &[String]) -> Res {
    if args.is_empty() {
        return Err("reindex: expected at least one file: rigger reindex <file>...".into());
    }
    // Same selection path as `cmd_ground`: honor `defaults.grounder` when a config
    // is present, else the unset default (turbovec). The grounder is rooted at `.`,
    // so the persisted store it loads/updates is this project's `.rigger/grounding/`.
    let name = config::load(".")
        .map(|cfg| cfg.workflow.defaults.grounder)
        .unwrap_or_default();
    // Use the reindex-specific constructor: it loads the persisted store WITHOUT a
    // whole-tree freshen, so `reindex` re-embeds ONLY the named files - never those
    // files twice (once by a load-time freshen, once by the reindex below).
    let grounder = select_reindex_grounder(&name)?;
    grounder.reindex(".", args);
    println!(
        "reindexed {} file(s) in the grounding index: {}",
        args.len(),
        args.join(", ")
    );
    Ok(())
}

/// `rigger emit <type> '<json-object>'` - append an event `{type: <type>, data:
/// <parsed json>}` to the project's event store AND fold it into the context graph,
/// EXACTLY as the MCP `rigger_emit` tool does (both call [`mcpserver::emit_event`]).
/// The store and graph are opened the way `serve` opens them - the namespaced
/// per-project event store and the `graph.db` projector on the `conductor::STREAM`.
/// A bad / non-object JSON payload is a clear error to stderr with a non-zero exit.
fn cmd_emit(args: &[String]) -> Res {
    let typ = args
        .first()
        .ok_or("emit: expected a type: rigger emit <type> '<json-object>'")?;
    let json_arg = args
        .get(1)
        .ok_or("emit: expected a JSON object: rigger emit <type> '<json-object>'")?;
    if args.len() > 2 {
        return Err(format!(
            "emit: expected a type and a single JSON object, got {} arguments",
            args.len()
        )
        .into());
    }
    let data: serde_json::Value = serde_json::from_str(json_arg)
        .map_err(|e| format!("emit: <json-object> is not valid JSON: {e}"))?;
    if !data.is_object() {
        return Err(format!(
            "emit: <json-object> must be a JSON object, got {}",
            json_type_name(&data)
        )
        .into());
    }

    std::fs::create_dir_all(RIGGER_DIR)?;
    let backend = Store::open(&db_path("events.db"))?;
    let store = Namespaced::new(&backend, &project_identity());
    let graph = Projector::open(&db_path("graph.db"))?;

    // Same args shape the MCP tool receives, so emit_event - the shared core both
    // surfaces call - behaves identically here and over MCP.
    let tool_args = serde_json::json!({ "type": typ, "data": data });
    let pos = mcpserver::emit_event(&store, conductor::STREAM, Some(&graph), &tool_args)?;
    println!("emitted {typ} (position {pos}) and folded it into the context graph");
    Ok(())
}

/// `rigger peers [<file> ...]` - print the peer decisions and review findings from
/// the context graph scoped to the given files (or all if none), EXACTLY as the MCP
/// `rigger_peers` tool does (both render through [`mcpserver::peers_json`]). The
/// store is opened the way `serve` opens it; a side-car replays the `conductor::STREAM`
/// backlog and this command waits for it to catch up before rendering one readable
/// line per decision / finding.
fn cmd_peers(args: &[String]) -> Res {
    let files: Vec<String> = args.to_vec();

    std::fs::create_dir_all(RIGGER_DIR)?;
    let backend = Store::open(&db_path("events.db"))?;
    let store = Namespaced::new(&backend, &project_identity());

    // The side-car replays the whole backlog from position 0; wait until it has
    // drained every event currently in the store before reading, so a one-shot CLI
    // call sees the full picture (the long-running serve path catches up live).
    let peers = Sidecar::start(&store, 0, Filter::default())?;
    let total = store
        .read_all(0, Direction::Forward, &Filter::default())?
        .len();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while peers.len() < total && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    let result = mcpserver::peers_json(&peers, &files);
    let decisions = result["decisions"].as_array().cloned().unwrap_or_default();
    let findings = result["findings"].as_array().cloned().unwrap_or_default();
    for d in &decisions {
        let id = d["id"].as_str().unwrap_or_default();
        let summary = d["summary"].as_str().unwrap_or_default();
        let governs = json_str_array(&d["governs"]);
        println!("decision {id} | {summary} | governs: {governs}");
    }
    for f in &findings {
        let id = f["id"].as_str().unwrap_or_default();
        let by = f["by"].as_str().unwrap_or_default();
        let summary = f["summary"].as_str().unwrap_or_default();
        let about = json_str_array(&f["about"]);
        println!("finding {id} | by {by} | {summary} | about: {about}");
    }
    Ok(())
}

/// Join a JSON array of strings into a comma-separated list for a `rigger peers`
/// line (the `governs` / `about` files). A non-array or empty value renders as `-`.
fn json_str_array(v: &serde_json::Value) -> String {
    match v.as_array() {
        Some(a) if !a.is_empty() => a
            .iter()
            .filter_map(|x| x.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        _ => "-".to_string(),
    }
}

/// A human-readable name for a JSON value's type, for the `rigger emit` error that
/// rejects a non-object payload.
fn json_type_name(v: &serde_json::Value) -> &'static str {
    match v {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "a boolean",
        serde_json::Value::Number(_) => "a number",
        serde_json::Value::String(_) => "a string",
        serde_json::Value::Array(_) => "an array",
        serde_json::Value::Object(_) => "an object",
    }
}

fn cmd_validate() -> Res {
    let cfg = config::load(".")?;
    println!(
        "config valid: {} agents, {} stages, {} gates",
        cfg.agents.len(),
        cfg.workflow.stages.len(),
        cfg.workflow.gates.len()
    );
    Ok(())
}

/// The full project setup, rooted at `root` so it is testable against a temp dir
/// without touching the process-wide current directory (`set_current_dir` is not
/// test-safe). It does two things, both idempotent:
///   1. scaffolds `<root>/.rigger/` - `workflow.yml` plus the `agents/` folder -
///      from the scaffold constants, keeping any file that already exists;
///   2. installs the Claude Code SessionStart hook into `<root>/.claude/settings.json`,
///      merging into (never clobbering) whatever settings are already there.
fn init_project(root: &Path) -> Res {
    // 1. Scaffold .rigger/.
    let rigger_dir = root.join(RIGGER_DIR);
    let agents_dir = rigger_dir.join("agents");
    std::fs::create_dir_all(&agents_dir)?;
    write_if_absent(&rigger_dir.join("workflow.yml"), SCAFFOLD_WORKFLOW);
    for (file, content) in SCAFFOLD_AGENTS {
        write_if_absent(&agents_dir.join(file), content);
    }

    // 2. Install the SessionStart hook, merging into any existing settings.
    let claude_dir = root.join(".claude");
    std::fs::create_dir_all(&claude_dir)?;
    let settings_path = claude_dir.join("settings.json");
    let existing = std::fs::read(&settings_path).unwrap_or_default();
    let merged = hooks::install_session_start(&existing, "rigger prime")?;
    std::fs::write(&settings_path, merged)?;
    Ok(())
}

fn cmd_init() -> Res {
    init_project(Path::new("."))?;
    let names: Vec<&str> = SCAFFOLD_AGENTS.iter().map(|(f, _)| *f).collect();
    println!(
        "scaffolded .rigger/workflow.yml and .rigger/agents/{{{}}} and installed a Claude Code \
         SessionStart hook in .claude/settings.json (it runs `rigger prime`)",
        names.join(", ")
    );
    Ok(())
}

/// The directory the per-project JS driver is provisioned into, relative to the
/// project root: `<root>/.rigger/shim/`. `rigger setup` writes the embedded runtime
/// files here and installs their npm deps; `rigger workflow` runs `shim.mjs` from
/// here.
fn shim_dir(root: &Path) -> std::path::PathBuf {
    root.join(RIGGER_DIR).join("shim")
}

/// Install the native `/rigger` Claude Code workflow at
/// `<root>/.claude/workflows/rigger.js`, returning that path. Creates the
/// `.claude/workflows/` directory if absent and always (re)writes the file from the
/// embedded [`RIGGER_WORKFLOW`], so a `rigger setup` after a `rigger` upgrade
/// refreshes the workflow to match the binary - the workflow and the conductor /
/// CLI it drives stay the same build. Claude Code auto-discovers `.js` here, so the
/// user can run `/rigger <spec>` immediately, with no registration. Rooted at `root`
/// so it is testable against a temp dir.
fn install_workflow(root: &Path) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let path = workflow_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, RIGGER_WORKFLOW)?;
    Ok(path)
}

/// Provision the per-project JS driver under `<root>/.rigger/shim/`: write the three
/// embedded runtime files (`shim.mjs`, `package.json`, `package-lock.json`) and
/// install their npm dependencies so `node_modules` is ready and `rigger workflow`
/// is zero-setup. Rooted at `root` so it is testable against a temp dir.
///
/// The files are always (re)written from the embedded copies, so a `rigger setup`
/// after a `rigger` upgrade refreshes the driver to match the binary - the shim and
/// the conductor it drives stay the same build. npm install runs `npm ci` when the
/// lockfile is present (a reproducible, locked install) and falls back to `npm
/// install` otherwise. A missing `npm` is a CLEAR error (with the directory it would
/// have installed in), never a silent skip - the user must know the driver is not
/// ready.
fn provision_shim(root: &Path) -> Res {
    let dir = write_shim_files(root)?;
    run_npm_install(&dir)?;
    Ok(())
}

/// Write the three embedded shim runtime files into `<root>/.rigger/shim/`,
/// returning that directory. Split out from [`provision_shim`] (which also runs npm
/// install) so the file-provisioning step is testable without invoking npm.
fn write_shim_files(root: &Path) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let dir = shim_dir(root);
    std::fs::create_dir_all(&dir)?;
    for (name, contents) in SHIM_FILES {
        std::fs::write(dir.join(name), contents)?;
    }
    Ok(dir)
}

/// Install the shim's npm dependencies in `dir`. Uses `npm ci` when a
/// `package-lock.json` is present (a clean, lockfile-exact install) and `npm
/// install` otherwise. `npm` not being on PATH is a clear, actionable error naming
/// the directory - the JS driver is unusable without its deps, so this never
/// silently succeeds.
fn run_npm_install(dir: &Path) -> Res {
    let npm = std::env::var("RIGGER_NPM").unwrap_or_else(|_| "npm".to_string());
    let subcmd = if dir.join("package-lock.json").exists() {
        "ci"
    } else {
        "install"
    };
    let status = Command::new(&npm)
        .arg(subcmd)
        .current_dir(dir)
        .status()
        .map_err(|e| {
            format!(
                "setup: could not run `{npm} {subcmd}` in {}: {e}. \
                 Is Node's npm installed and on your PATH? The JS driver needs its \
                 dependencies before `rigger workflow` can run.",
                dir.display()
            )
        })?;
    if !status.success() {
        return Err(format!(
            "setup: `{npm} {subcmd}` failed in {} ({status}); the JS driver's \
             dependencies were not installed",
            dir.display()
        )
        .into());
    }
    Ok(())
}

/// `rigger setup` is the FULL project setup: it does everything `rigger init` does
/// (scaffold `.rigger/` + install the Claude Code hook), installs the native
/// `/rigger` Claude Code workflow at `.claude/workflows/rigger.js`, AND provisions
/// the JS driver (writes the embedded shim runtime into `.rigger/shim/` and runs `npm
/// install`). After it runs the user can drive the loop with the native workflow
/// (`/rigger <spec>`) with zero manual setup; the standalone `rigger workflow` shim
/// remains as a fallback.
fn cmd_setup() -> Res {
    let root = Path::new(".");
    init_project(root)?;
    install_workflow(root)?;
    provision_shim(root)?;
    let names: Vec<&str> = SCAFFOLD_AGENTS.iter().map(|(f, _)| *f).collect();
    println!(
        "scaffolded .rigger/workflow.yml and .rigger/agents/{{{}}}, installed a Claude Code \
         SessionStart hook in .claude/settings.json, and provisioned the JS driver in \
         .rigger/shim/ (wrote shim.mjs + package.json + package-lock.json and ran npm install)",
        names.join(", ")
    );
    println!(
        "installed the /rigger workflow (.claude/workflows/rigger.js) - run it with: /rigger \
         <spec-path>"
    );
    Ok(())
}

fn cmd_prime() -> Res {
    let path = db_path("events.db");
    if !Path::new(&path).exists() {
        println!("# Rigger: no decisions recorded yet (run `rigger run` to start).");
        return Ok(());
    }
    let store = Store::open(&path)?;
    let events = store.read_all(0, Direction::Backward, &Filter::default())?;
    println!("# Rigger: recent decisions");
    let mut shown = 0;
    for e in &events {
        if e.type_ != contextgraph::TYPE_DECISION_MADE {
            continue;
        }
        if let Ok(d) = serde_json::from_slice::<PeerDecision>(&e.data) {
            println!("- {}: {}", d.id, d.summary);
            shown += 1;
            if shown >= 10 {
                break;
            }
        }
    }
    if shown == 0 {
        println!("(none yet)");
    }
    Ok(())
}

/// Build the grounder named by `defaults.grounder` (§3.2, §5.4, R4). Turbovec is the
/// DEFAULT grounder: the turbovec names (`vector`/`turbovec`) AND an UNSET / empty
/// `defaults.grounder` resolve to the real semantic engine. `grep` and `nop` resolve
/// via `grounder::grounder_for` and are reachable ONLY when named explicitly.
///
/// When the binary is built WITHOUT the `turbovec` feature, resolving to turbovec is
/// a LOUD error (a clear message + non-zero exit), never a silent degrade to grep -
/// that silent degrade is exactly what hid turbovec being absent for a whole session.
/// Grep runs ONLY when the user writes `grounder: grep`.
#[cfg(feature = "turbovec")]
fn select_grounder(name: &str) -> Result<Box<dyn Grounder>, Box<dyn std::error::Error>> {
    if rigger::grounder::resolves_to_turbovec(name) {
        // Building the index can fail for a real, distinct reason (e.g. the embedding
        // model cannot be loaded); that is its OWN loud error, not a grep fallback.
        // `new` freshens any tree drift on load, which is what the grounding-read paths
        // (`ground`/`run`/`serve`) want.
        let tv = rigger::grounder::turbovec::Turbovec::new(".")
            .map_err(|e| format!("turbovec grounder unavailable: {e}"))?;
        return Ok(Box::new(tv));
    }
    Ok(rigger::grounder::grounder_for(name, ".")?)
}

#[cfg(not(feature = "turbovec"))]
fn select_grounder(name: &str) -> Result<Box<dyn Grounder>, Box<dyn std::error::Error>> {
    // No turbovec feature compiled in: `grounder_for` returns the loud
    // "built without the turbovec feature" error for the default / turbovec names,
    // and resolves grep / nop normally. We never silently degrade to grep.
    Ok(rigger::grounder::grounder_for(name, ".")?)
}

/// The grounder for `rigger reindex`, which differs from [`select_grounder`] ONLY for
/// turbovec: it constructs via `Turbovec::new_for_reindex`, which loads the persisted
/// store WITHOUT freshening tree drift. `reindex` then re-embeds exactly the named
/// files; using the freshening `new` here would re-embed every drifted file on load and
/// then the named files AGAIN - a double-embed. grep / nop have no index, so their
/// `reindex` is a no-op and this resolves identically to [`select_grounder`].
#[cfg(feature = "turbovec")]
fn select_reindex_grounder(name: &str) -> Result<Box<dyn Grounder>, Box<dyn std::error::Error>> {
    if rigger::grounder::resolves_to_turbovec(name) {
        let tv = rigger::grounder::turbovec::Turbovec::new_for_reindex(".")
            .map_err(|e| format!("turbovec grounder unavailable: {e}"))?;
        return Ok(Box::new(tv));
    }
    Ok(rigger::grounder::grounder_for(name, ".")?)
}

#[cfg(not(feature = "turbovec"))]
fn select_reindex_grounder(name: &str) -> Result<Box<dyn Grounder>, Box<dyn std::error::Error>> {
    Ok(rigger::grounder::grounder_for(name, ".")?)
}

fn git_repo() -> String {
    Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

fn print_run_state(rs: &RunState) {
    println!("run state:");
    for (name, u) in &rs.units {
        println!("  {:<20} {}", name, u.status.as_str());
    }
    if rs.done() {
        println!("done: every unit integrated");
    } else {
        println!("incomplete: not every unit integrated");
    }
}

fn write_if_absent(path: &Path, content: &str) {
    if path.exists() {
        println!("kept existing {}", path.display());
        return;
    }
    if let Err(e) = std::fs::write(path, content) {
        eprintln!("rigger: write {}: {e}", path.display());
    }
}

/// The scaffolded workflow (§3.2): a worked plan -> implement pipeline where the
/// review is PER UNIT. It demonstrates the documented shape - a `defaults:` block
/// (autonomy + grounder + the three-tier `review` panel), a reusable `gates:`
/// library, and an implement stage that runs each unit's complete lifecycle
/// (implement -> gates -> three-tier review of THIS unit -> integrate). It loads
/// through `config::load` against the agents scaffolded alongside it.
const SCAFFOLD_WORKFLOW: &str =
    "# Scaffolded by `rigger init`. A worked plan -> implement pipeline where the\n\
# review is PER UNIT: each unit implements, three-tier-reviews ITSELF (lenses ->\n\
# adversary -> adjudicator via defaults.review), and integrates in one lifecycle.\n\
# Replace the gate commands with your own.\n\
name: example\n\
\n\
defaults:\n  \
autonomy: auto_notify   # manual | auto_notify | silent\n  \
grounder: turbovec      # turbovec (default; the real semantic grounder) | grep | nop\n  \
# The spawn-budget circuit-breaker: the hard cap on agent spawns one unattended\n  \
# run may make. At the cap the breaker emits BudgetExhausted and aborts the run,\n  \
# so a runaway can never spawn unboundedly. NON-ZERO on purpose - 0 = unlimited.\n  \
budget: 60\n  \
# The remediation depth: how many attempts a failed unit gets before it escalates\n  \
# to a human. This is the REFINEMENT-depth knob, not a review-rigor one - raise it\n  \
# to give a subtle unit room to CONVERGE under the full strict review instead of\n  \
# escalating prematurely. It loosens the depth limit, never the review bar. Absent\n  \
# falls back to 3 (the historical default); bounded by `budget` above.\n  \
max_retries: 3\n  \
# The three-tier review panel applied to EVERY implementer unit. Declared once\n  \
# here, inherited by the implement stage and every planner-proposed unit.\n  \
review:\n    \
lenses: [reviewer.architecture, reviewer.technical]   # tier 1: the expert lenses\n    \
adversary: adversary           # tier 2: reviews the lenses and refutes them\n    \
adjudicator: devils-advocate   # tier 3: neutral judge; its verdict gates the unit\n\
\n\
gates:                    # a reusable library of commands, referenced by name\n  \
build: { run: \"echo build ok; true\", kind: core }\n  \
test:  { run: \"echo test ok; true\",  kind: core }\n  \
lint:  { run: \"echo lint ok; true\",  kind: elevated }\n\
\n\
stages:\n  \
# The conductor creates one baseline implement unit per acceptance criterion (the\n  \
# deterministic decomposition); this planner REFINES that baseline via UnitProposed.\n  \
# A produces stage decomposes the whole spec, so it has no single coverage criterion\n  \
# - it grounds on the spec's acceptance criteria, not a `coverage` label.\n  \
plan:\n    \
agent: planner\n    \
produces: dag           # refine the spec's unit DAG at runtime\n\
\n  \
# Each unit implements, three-tier-reviews ITSELF (via defaults.review), and\n  \
# integrates in one lifecycle. A reject or a gate failure feeds back into that\n  \
# same unit's remediation loop; it does NOT integrate until approved + green.\n  \
implement:\n    \
needs: [plan]\n    \
agent: implementer\n    \
strategy: fan-out       # one worker per ready unit, in isolated worktrees\n    \
partition: by-blast-radius\n    \
gates: [build, test, lint]  # red -> green enforced around the change\n    \
on_pass: merge          # land + reindex + record, per unit, once reviewed\n    \
coverage: \"each unit is implemented, reviews itself, and integrates green\"\n";

/// The agents the scaffolded workflow references, each a markdown-with-frontmatter
/// definition `config::load` parses. Filenames are arbitrary; the `id` is what the
/// workflow binds to.
const SCAFFOLD_AGENTS: &[(&str, &str)] = &[
    (
        "planner.md",
        "---\n\
id: planner\n\
model: sonnet\n\
tools: [Read, Grep, Glob]\n\
isolation: none\n\
---\n\
You decompose the spec into a DAG of small, independently-verifiable units, one\n\
per acceptance criterion. Emit each as a UnitProposed decision. Do not write code.\n",
    ),
    (
        "implementer.md",
        "---\n\
id: implementer\n\
model: sonnet\n\
tools: [Read, Edit, Write, Grep, Glob, Bash]\n\
isolation: worktree\n\
recurse: false\n\
---\n\
You implement ONE fully-specified unit inside your worktree. Write the failing\n\
test first, confirm RED, implement minimally, confirm GREEN, run the named gates,\n\
commit. Report the final line as JSON: {\"id\",\"pass\",\"evidence\"}.\n",
    ),
    (
        "reviewer.architecture.md",
        "---\n\
id: reviewer.architecture\n\
model: sonnet\n\
tools: [Read, Grep, Glob, Bash]\n\
isolation: none\n\
---\n\
You review a diff for architectural defects ONLY. Quote the rule or doc violated.\n\
Output the REVIEW schema: {verdict, issues:[{title,file_line,reason}]}.\n",
    ),
    (
        "reviewer.technical.md",
        "---\n\
id: reviewer.technical\n\
model: sonnet\n\
tools: [Read, Grep, Glob, Bash]\n\
isolation: none\n\
---\n\
You review a diff for correctness, error-handling, and idiomatic defects ONLY.\n\
Output the REVIEW schema: {verdict, issues:[{title,file_line,reason}]}.\n",
    ),
    (
        "adversary.md",
        "---\n\
id: adversary\n\
model: opus\n\
tools: [Read, Grep, Glob, Bash]\n\
isolation: none\n\
---\n\
You are the adversary (tier 2). You run AFTER the lenses and review THEIR findings\n\
AND the diff, trying to PROVE THE LENSES WRONG: hold them to a higher bar, surface\n\
the substantive issues they all missed, and refute lens overreach. You review the\n\
reviews - not a parallel lens - and you do NOT render the final verdict. Default to\n\
skepticism; cite file:line. Record findings with rigger_emit.\n",
    ),
    (
        "devils-advocate.md",
        "---\n\
id: devils-advocate\n\
model: opus\n\
tools: [Read, Grep, Glob, Bash]\n\
isolation: none\n\
---\n\
You are the adjudicator (tier 3), the neutral final judge. Weigh the expert lenses\n\
against the adversary and decide who wins. Be neutral in tone but EXTREMELY strict\n\
on design / architecture / ADR adherence: any deviation or cut corner is a reject,\n\
no matter which side flagged it. When you reject, say exactly what must change. End\n\
with a single JSON line {\"verdict\":\"approve\"} or {\"verdict\":\"reject\"} - reject\n\
blocks integration no matter what the static gates say.\n",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Write the scaffold constants into a temp `.rigger/` (the same bytes
    /// `rigger init` emits) and load them through `config::load`: the scaffold must
    /// be a valid, referentially-complete config demonstrating the full DAG shape.
    #[test]
    fn scaffold_parses_into_a_valid_config() {
        let dir = tempfile::tempdir().unwrap();
        let rigger = dir.path().join(RIGGER_DIR);
        let agents = rigger.join("agents");
        std::fs::create_dir_all(&agents).unwrap();
        std::fs::write(rigger.join("workflow.yml"), SCAFFOLD_WORKFLOW).unwrap();
        for (file, content) in SCAFFOLD_AGENTS {
            std::fs::write(agents.join(file), content).unwrap();
        }

        let cfg = config::load(dir.path().to_str().unwrap())
            .expect("the scaffolded config must load and validate");

        // Six agents: planner, implementer, two reviewer lenses, the adversary, the
        // adjudicator. Integration is folded into the unit lifecycle (no integrator).
        assert_eq!(cfg.agents.len(), 6, "scaffold agent count");
        // Two stages: plan -> implement (each unit reviews itself and integrates).
        assert_eq!(cfg.workflow.stages.len(), 2, "scaffold stage count");
        // Three gates in the reusable library.
        assert_eq!(cfg.workflow.gates.len(), 3, "scaffold gate count");

        // The scaffold exercises the per-unit shape: a producer, a fan-out implement
        // stage that integrates on_pass: merge, and a three-tier review panel
        // (lenses -> adversary -> adjudicator) declared once on defaults.review.
        let plan = &cfg.workflow.stages["plan"];
        assert_eq!(plan.produces, "dag");
        let implement = &cfg.workflow.stages["implement"];
        assert_eq!(implement.strategy, "fan-out");
        assert_eq!(implement.needs, ["plan"]);
        assert_eq!(implement.on_pass, "merge");
        let review = &cfg.workflow.defaults.review;
        assert_eq!(review.lenses.len(), 2, "tier 1: the expert lenses");
        assert_eq!(review.adversary, "adversary", "tier 2: refutes the lenses");
        assert_eq!(
            review.adjudicator, "devils-advocate",
            "tier 3: the neutral adjudicator gates"
        );
        // The scaffold sets turbovec EXPLICITLY (visible, not implicit) - it is the
        // default grounder and the default cargo feature.
        assert_eq!(cfg.workflow.defaults.grounder, "turbovec");
        // FIX 3: the scaffold ships a NON-ZERO spawn budget so an unattended `rigger
        // run` cannot spawn unboundedly - 0 would be unlimited.
        assert!(
            cfg.workflow.defaults.budget > 0,
            "the scaffold must ship a non-zero default spawn budget; was {}",
            cfg.workflow.defaults.budget
        );
        assert_eq!(cfg.workflow.defaults.budget, 60, "scaffold default budget");
    }

    /// The two checked-in workflows that ship with the repo - the self-hosted
    /// `.rigger/workflow.yml` and `examples/demo` - must each carry a NON-ZERO spawn
    /// budget (FIX 3): a shipped, unattended config must cap its own spawns. A 0
    /// (unlimited) budget here is what let a runaway loop churn for hours.
    #[test]
    // Reads relative paths (`.`, `..`) so it depends on the process CWD. Another test
    // (`cmd_stats_on_a_never_run_project...`) temporarily `set_current_dir`s to a temp
    // dir; if that runs concurrently, `config::load(".")` here resolves `.` to that
    // temp dir and fails ("read reviewer.architecture.md: No such file"). CWD is
    // process-global, so a restore guard in the other test does not close the window -
    // the two must be mutually exclusive. Both share the `cwd` serial key.
    #[serial_test::serial(cwd)]
    fn shipped_workflows_carry_a_non_zero_spawn_budget() {
        for root in ["..", "../examples/demo", ".", "examples/demo"] {
            // The test runs from the crate root in CI and from the workspace root
            // locally; probe both layouts and skip a path that does not resolve to a
            // loadable config rather than hard-failing on the working directory.
            let path = std::path::Path::new(root);
            if !path.join(RIGGER_DIR).join("workflow.yml").exists() {
                continue;
            }
            let cfg = config::load(root)
                .unwrap_or_else(|e| panic!("shipped workflow at {root:?} must load: {e}"));
            assert!(
                cfg.workflow.defaults.budget > 0,
                "shipped workflow at {root:?} must cap spawns with a non-zero budget; was {}",
                cfg.workflow.defaults.budget
            );
        }
    }

    #[test]
    fn parse_run_args_defaults_to_cli_sqlite() {
        let a = parse_run_args(&[]).unwrap();
        assert!(a.driver == DriverKind::Cli);
        assert!(a.store == StoreKind::Sqlite);
        assert!(a.conn.is_none());
        assert!(a.spec.is_none());
    }

    #[test]
    fn parse_run_args_reads_driver_eventstore_conn_and_spec() {
        let args = [
            "spec.md".to_string(),
            "--driver".to_string(),
            "workflow".to_string(),
            "--eventstore".to_string(),
            "kurrentdb".to_string(),
            "--conn".to_string(),
            "kurrentdb://localhost:2113".to_string(),
        ];
        let a = parse_run_args(&args).unwrap();
        assert!(a.driver == DriverKind::Workflow);
        assert!(a.store == StoreKind::KurrentDb);
        assert_eq!(a.conn.as_deref(), Some("kurrentdb://localhost:2113"));
        assert_eq!(a.spec.as_deref(), Some("spec.md"));
    }

    #[test]
    fn parse_run_args_rejects_unknown_flags_and_values() {
        assert!(parse_run_args(&["--driver".into(), "bogus".into()]).is_err());
        assert!(parse_run_args(&["--eventstore".into(), "bogus".into()]).is_err());
        assert!(parse_run_args(&["--nope".into()]).is_err());
        assert!(parse_run_args(&["a".into(), "b".into()]).is_err());
    }

    /// With the default build (no `kurrentdb` feature), requesting the server store
    /// is a clear error, never a silent fallback - the default build stays green.
    #[cfg(not(feature = "kurrentdb"))]
    #[test]
    fn kurrentdb_without_the_feature_is_a_clear_error() {
        match open_store(StoreKind::KurrentDb, Some("kurrentdb://x")) {
            Ok(_) => panic!("kurrentdb must not open without the feature"),
            Err(e) => assert!(
                e.to_string().contains("kurrentdb"),
                "the error must name the missing feature; got: {e}"
            ),
        }
    }

    /// With the turbovec feature compiled OUT, selecting the DEFAULT grounder (an
    /// unset name) or an explicit turbovec/vector name FAILS LOUDLY - a clear error
    /// naming turbovec, the missing feature, and the explicit grep opt-out - and never
    /// silently degrades to grep. This is the regression guard for the silent degrade
    /// that hid turbovec being absent for a whole session.
    #[cfg(not(feature = "turbovec"))]
    #[test]
    fn select_grounder_fails_loudly_without_the_turbovec_feature() {
        for name in ["", "turbovec", "vector"] {
            let err = select_grounder(name)
                .err()
                .unwrap_or_else(|| panic!("{name:?} must fail loudly without the feature"));
            let msg = err.to_string();
            assert!(
                msg.contains("turbovec") && msg.contains("feature") && msg.contains("grep"),
                "the loud error must name turbovec, the feature, and the grep opt-out; got: {msg}"
            );
        }
        // grep and nop are the explicit-only opt-outs and still resolve fine.
        assert!(select_grounder("grep").is_ok());
        assert!(select_grounder("nop").is_ok());
        // An unknown name is a hard error too, not a silent grep fallback.
        assert!(select_grounder("bogus").is_err());
    }

    /// With the turbovec feature compiled IN, grep is still reachable when named
    /// EXPLICITLY (the deliberate literal-grounder opt-out), and an unknown name is a
    /// hard error rather than a silent grep fallback. (The turbovec / default path is
    /// exercised by the grounder's own model-loading test, which downloads weights.)
    #[cfg(feature = "turbovec")]
    #[test]
    fn select_grounder_with_feature_resolves_grep_explicitly_and_rejects_unknown() {
        assert!(
            select_grounder("grep").is_ok(),
            "explicit grep must resolve even with the turbovec feature on"
        );
        assert!(select_grounder("nop").is_ok());
        assert!(
            select_grounder("bogus-grounder").is_err(),
            "an unknown grounder name must be a hard error, not a silent grep fallback"
        );
    }

    #[test]
    fn project_identity_is_never_empty() {
        assert!(!project_identity().is_empty());
    }

    /// `rigger setup` must provision the per-project JS driver: write the three
    /// embedded runtime files into `.rigger/shim/` with the embedded content. (The
    /// npm-install step is asserted separately so this test does not depend on npm.)
    #[test]
    fn setup_provisions_the_shim_runtime_files() {
        let dir = tempfile::tempdir().unwrap();
        let shim = write_shim_files(dir.path()).expect("provisioning writes the shim files");
        assert_eq!(shim, shim_dir(dir.path()));

        for (name, embedded) in SHIM_FILES {
            let path = shim.join(name);
            assert!(path.exists(), "{name} must be written into .rigger/shim/");
            let on_disk = std::fs::read_to_string(&path).unwrap();
            assert_eq!(
                &on_disk, embedded,
                "{name} on disk must be byte-identical to the embedded runtime"
            );
        }

        // The dev-only mock/test files must NOT ship - only the three runtime files.
        let names: Vec<String> = std::fs::read_dir(&shim)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            !names
                .iter()
                .any(|n| n.contains("mock") || n.contains(".test.")),
            "only runtime files ship; no mock-*/*.test.mjs. found: {names:?}"
        );

        // The embedded shim.mjs is the real driver (a sanity check it is not a stub).
        assert!(
            SHIM_MJS.contains("rigger") && SHIM_MJS.contains("query"),
            "the embedded shim.mjs must be the real JS driver"
        );
    }

    /// `rigger setup` must install the native `/rigger` Claude Code workflow at
    /// `.claude/workflows/rigger.js` with content byte-identical to the embedded
    /// `RIGGER_WORKFLOW`, and re-running setup must overwrite it cleanly (so a
    /// `rigger` upgrade refreshes the workflow to match the binary). The npm-install
    /// step is exercised separately, so this test does not depend on npm.
    #[test]
    fn setup_installs_the_native_rigger_workflow() {
        let dir = tempfile::tempdir().unwrap();
        let path = install_workflow(dir.path()).expect("installing writes the workflow file");
        assert_eq!(path, workflow_path(dir.path()));
        assert_eq!(
            path,
            dir.path()
                .join(".claude")
                .join("workflows")
                .join("rigger.js"),
            "the workflow must be installed at .claude/workflows/rigger.js"
        );

        let on_disk = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            on_disk, RIGGER_WORKFLOW,
            "the installed workflow must be byte-identical to the embedded RIGGER_WORKFLOW"
        );

        // The embedded workflow is the real driver, not a stub: it exports `meta` and
        // drives agents via the workflow runtime.
        assert!(
            RIGGER_WORKFLOW.contains("export const meta") && RIGGER_WORKFLOW.contains("agent("),
            "the embedded workflow must be the real native /rigger workflow"
        );

        // Re-running setup overwrites cleanly: pre-seed stale content, reinstall, and
        // confirm the embedded content wins (not appended, not left stale).
        std::fs::write(&path, "// stale - from an older rigger build\n").unwrap();
        let again = install_workflow(dir.path()).expect("re-install must succeed");
        assert_eq!(again, path);
        let after = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            after, RIGGER_WORKFLOW,
            "re-running setup must overwrite the workflow with the embedded content"
        );
    }

    /// `rigger setup` runs npm install against the provisioned shim so `node_modules`
    /// is ready. When npm is available we run it FOR REAL against a temp dir and
    /// confirm `node_modules` appears; when npm is unavailable we assert the clear
    /// error path instead (never a silent skip).
    #[test]
    fn setup_runs_npm_install_or_reports_a_clear_error() {
        let dir = tempfile::tempdir().unwrap();
        let shim = write_shim_files(dir.path()).unwrap();

        if npm_available() {
            // npm is on PATH: provisioning must run it for real and leave node_modules.
            provision_shim(dir.path()).expect("provision_shim must succeed when npm is available");
            assert!(
                shim.join("node_modules").is_dir(),
                "npm install must populate node_modules in the provisioned shim dir"
            );
        } else {
            // npm is NOT on PATH: the error must be clear and actionable, not a silent
            // skip. Point RIGGER_NPM at a binary that does not exist to exercise the
            // missing-npm path deterministically.
            std::env::set_var("RIGGER_NPM", "definitely-not-a-real-npm-binary-xyz");
            let err = run_npm_install(&shim).expect_err("a missing npm must be a clear error");
            std::env::remove_var("RIGGER_NPM");
            let msg = err.to_string();
            assert!(
                msg.contains("npm") && msg.to_lowercase().contains("path"),
                "the missing-npm error must mention npm and PATH; got: {msg}"
            );
        }
    }

    /// `rigger workflow` runs the PROVISIONED per-project shim when `.rigger/shim/`
    /// exists, and otherwise reports a clear "run `rigger setup`" error rather than
    /// failing obscurely.
    #[test]
    fn workflow_locates_the_provisioned_shim_or_tells_you_to_run_setup() {
        // Guard the RIGGER_SHIM override does not leak in from the environment.
        let prior = std::env::var("RIGGER_SHIM").ok();
        std::env::remove_var("RIGGER_SHIM");

        let dir = tempfile::tempdir().unwrap();
        // Absent: a clear, actionable error naming `rigger setup`.
        let err = locate_shim(dir.path()).expect_err("an unprovisioned project must error");
        assert!(
            err.to_string().contains("rigger setup"),
            "the unprovisioned error must tell the user to run `rigger setup`; got: {err}"
        );

        // After provisioning the files, locate_shim finds the per-project shim.mjs.
        let shim = write_shim_files(dir.path()).unwrap();
        let found = locate_shim(dir.path()).expect("a provisioned shim must be located");
        assert_eq!(
            Path::new(&found),
            shim.join("shim.mjs"),
            "locate_shim must return the provisioned .rigger/shim/shim.mjs"
        );

        if let Some(v) = prior {
            std::env::set_var("RIGGER_SHIM", v);
        }
    }

    fn npm_available() -> bool {
        Command::new("npm")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    use rigger::metrics::GateCounts;
    use std::collections::BTreeMap;

    /// `format_stats` must surface ALL FOUR required metrics - first-pass yield,
    /// per-gate remediation (pass/fail) counts, escalation rate, and review
    /// approve/reject - from a fully-populated `Metrics` value. This pins the CLI
    /// contract for `rigger stats` (the spec's "stats prints the four metrics")
    /// without touching the filesystem.
    #[test]
    fn format_stats_prints_all_four_metrics() {
        let mut gates = BTreeMap::new();
        gates.insert("build".to_string(), GateCounts { pass: 4, fail: 1 });
        gates.insert("clippy".to_string(), GateCounts { pass: 3, fail: 2 });
        let m = Metrics {
            units_started: 4,
            first_pass_clean: 3,
            gates,
            units_escalated: 1,
            review_approve: 5,
            review_reject: 2,
        };
        let out = format_stats(&m).join("\n");

        // 1. First-pass yield: 3/4 = 75.0%, with the fraction shown.
        assert!(
            out.contains("first-pass yield   75.0% (3/4 units clean on the first pass)"),
            "first-pass yield line missing/wrong:\n{out}"
        );
        // 2. Escalation rate: 1/4 = 25.0%, with the fraction shown.
        assert!(
            out.contains("escalation rate    25.0% (1/4 units escalated to a human)"),
            "escalation rate line missing/wrong:\n{out}"
        );
        // 3. Review approve/reject counts.
        assert!(
            out.contains("review             5 approved / 2 rejected"),
            "review approve/reject line missing/wrong:\n{out}"
        );
        // 4. Per-gate remediation counts: one line per gate (fail = remediation),
        // sorted by gate id (build before clippy).
        assert!(
            out.contains("build            4 pass / 1 fail / 5 total"),
            "build gate line missing/wrong:\n{out}"
        );
        assert!(
            out.contains("clippy           3 pass / 2 fail / 5 total"),
            "clippy gate line missing/wrong:\n{out}"
        );
        let build_at = out.find("build ").expect("build gate present");
        let clippy_at = out.find("clippy ").expect("clippy gate present");
        assert!(build_at < clippy_at, "gates must be sorted by id:\n{out}");
    }

    /// A zeroed `Metrics` (the shape `project(&[])` returns) must render guarded,
    /// NaN-free output: 0.0% rates and a "no gate runs" line, never `NaN%` from a
    /// divide-by-zero or an empty/blank gates section.
    #[test]
    fn format_stats_handles_zeroed_metrics_without_nan() {
        let out = format_stats(&Metrics::default()).join("\n");
        assert!(out.contains("first-pass yield   0.0%"), "{out}");
        assert!(out.contains("escalation rate    0.0%"), "{out}");
        assert!(
            out.contains("review             0 approved / 0 rejected"),
            "{out}"
        );
        assert!(
            out.contains("gates              (no gate runs recorded)"),
            "a run with no gate runs must say so, not print a blank section:\n{out}"
        );
        assert!(
            !out.to_lowercase().contains("nan"),
            "rates must be guarded, never NaN:\n{out}"
        );
    }

    /// `cmd_stats` rejects any positional argument with a clear error (it takes none),
    /// mirroring the strict-arity errors the other CLI commands raise.
    #[test]
    fn cmd_stats_rejects_extra_arguments() {
        let err = cmd_stats(&["unexpected".to_string()]).expect_err("stats takes no arguments");
        assert!(
            err.to_string().contains("stats: expected no arguments"),
            "the error must explain stats takes no arguments; got: {err}"
        );
    }

    /// On an absent `events.db` (a project that has never run) `cmd_stats` must print
    /// the clear "no runs yet" message and succeed, NOT create the db or panic. Run in
    /// a temp dir so the real project's `.rigger/` is untouched.
    #[test]
    // Mutates the process-global CWD (`set_current_dir` below). Shares the `cwd` serial
    // key with `shipped_workflows_carry_a_non_zero_spawn_budget` (which reads relative
    // paths) so the two never run concurrently: the restore guard prevents LEAKING a
    // changed CWD past this test, but only mutual exclusion prevents the other test from
    // OBSERVING the changed CWD mid-window.
    #[serial_test::serial(cwd)]
    fn cmd_stats_on_a_never_run_project_says_no_runs_and_creates_no_db() {
        let dir = tempfile::tempdir().unwrap();
        let prev = std::env::current_dir().unwrap();
        // current_dir is process-global; serialize against the other cwd-sensitive
        // path via a guard that always restores it even on a failed assertion.
        struct Restore(std::path::PathBuf);
        impl Drop for Restore {
            fn drop(&mut self) {
                let _ = std::env::set_current_dir(&self.0);
            }
        }
        let _restore = Restore(prev);
        std::env::set_current_dir(dir.path()).unwrap();

        cmd_stats(&[]).expect("stats on a never-run project must succeed");

        // The absent-db guard must run BEFORE Store::open, so no events.db is created.
        assert!(
            !dir.path().join(RIGGER_DIR).join("events.db").exists(),
            "stats on a never-run project must not create events.db"
        );
    }

    /// The no-runs message single-sourced for both the absent-db and empty-stream
    /// edges must actually point the user at `rigger run` - a pinned, greppable
    /// contract so the two edges can never drift apart or lose the next-step hint.
    #[test]
    fn no_runs_message_points_at_rigger_run() {
        assert!(NO_RUNS_MESSAGE.contains("rigger run"), "{NO_RUNS_MESSAGE}");
        assert!(NO_RUNS_MESSAGE.contains("no runs"), "{NO_RUNS_MESSAGE}");
    }

    /// Append `events` to `project`'s namespaced `run` stream in the sqlite db at
    /// `path` - the exact stream and namespace the conductor writes its run to, so a
    /// `stats_lines` read sees them exactly as it would a real run. Returns nothing;
    /// the db file now exists with the events committed.
    fn seed_run(path: &str, project: &str, events: &[rigger::eventstore::Event]) {
        use rigger::eventstore::ExpectedRevision;
        let backend = Store::open(path).expect("open sqlite backend");
        let store = Namespaced::new(&backend, project);
        store
            .append(conductor::STREAM, ExpectedRevision::Any, events)
            .expect("append run events");
    }

    /// `stats_lines` against an absent `events.db` returns `None` (the "no runs yet"
    /// signal) and - critically - does NOT create the file. Opening would create it
    /// and mask a never-run project as an empty one, so the guard must precede the open.
    #[test]
    fn stats_lines_absent_db_returns_none_and_creates_no_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.db");
        let path_str = path.to_str().unwrap();

        let out = stats_lines(path_str, "proj-x").expect("absent db is not an error");
        assert!(out.is_none(), "an absent db must read as no runs (None)");
        assert!(
            !path.exists(),
            "stats_lines must not create events.db when it is absent"
        );
    }

    /// `stats_lines` against an existing db whose namespaced `run` stream is empty
    /// returns `None`. This is the db-exists-but-no-run edge: another command (or
    /// another project sharing the backend) created the file, but this project has no
    /// run. It must read as "no runs yet", not a zeroed/empty table.
    #[test]
    fn stats_lines_existing_db_with_empty_run_stream_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.db");
        let path_str = path.to_str().unwrap();

        // Create the db file via the real store path, but leave "proj-me"'s run stream
        // empty (append zero events still opens/creates the backing file).
        seed_run(path_str, "proj-me", &[]);
        assert!(path.exists(), "the db file must exist for this edge");

        let out = stats_lines(path_str, "proj-me").expect("empty run stream is not an error");
        assert!(
            out.is_none(),
            "an existing db with an empty run stream must read as no runs (None)"
        );
    }

    /// The read is scoped to the per-project namespace: a run that ANOTHER project
    /// wrote to the SAME shared backend must not leak into this project's stats. With
    /// the backend holding `proj-other`'s run, `proj-me`'s `stats_lines` still reads
    /// `None` - proving the [`Namespaced`] decorator (`proj-<project>-run`) is on the
    /// read path, not just the write path.
    #[test]
    fn stats_lines_does_not_read_another_projects_namespaced_run() {
        use rigger::eventstore::Event;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.db");
        let path_str = path.to_str().unwrap();

        // proj-other has a real run in the shared backend.
        seed_run(
            path_str,
            "proj-other",
            &[Event::new("UnitStarted", b"{}".to_vec())],
        );

        // proj-me, reading the same file, sees its OWN (empty) namespace - no runs.
        let mine = stats_lines(path_str, "proj-me").expect("read is not an error");
        assert!(
            mine.is_none(),
            "stats must be namespace-scoped: another project's run must not leak in"
        );

        // Sanity: the other project's run IS visible to it, so the data really is there
        // and the None above is the namespace boundary, not a read failure.
        let theirs = stats_lines(path_str, "proj-other").expect("read is not an error");
        assert!(
            theirs.is_some(),
            "the project that owns the run must see its stats"
        );
    }

    /// A populated namespaced run reads back through `stats_lines` as the rendered
    /// metric lines - the positive case that pins the read-fold-format path end to end
    /// against a real on-disk db (not just the pure formatter), and that the events the
    /// fold sees came back through the namespace with their clean stream name.
    #[test]
    fn stats_lines_existing_run_renders_metric_lines() {
        use rigger::eventstore::Event;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.db");
        let path_str = path.to_str().unwrap();

        seed_run(
            path_str,
            "proj-me",
            &[
                Event::new("UnitStarted", br#"{"id":"u1"}"#.to_vec()),
                Event::new("UnitIntegrated", br#"{"id":"u1"}"#.to_vec()),
            ],
        );

        let lines = stats_lines(path_str, "proj-me")
            .expect("read is not an error")
            .expect("a populated run must render lines, not None");
        let out = lines.join("\n");
        assert!(
            out.contains("run stats:"),
            "a populated run must render the stats header:\n{out}"
        );
        assert!(
            out.contains("first-pass yield"),
            "a populated run must render the first-pass yield metric:\n{out}"
        );
        assert!(
            out != NO_RUNS_MESSAGE,
            "a populated run must not print the no-runs message"
        );
    }
}
