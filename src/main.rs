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
#[cfg(feature = "turbovec")]
use rigger::grounder::Grep;
use rigger::grounder::Grounder;
use rigger::ledger::RunState;
use rigger::sidecar::PeerDecision;
use rigger::{hooks, spec};

const RIGGER_DIR: &str = ".rigger";

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
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        usage();
        std::process::exit(2);
    }
    let result = match args[1].as_str() {
        "run" => cmd_run(&args[2..]),
        "serve" => cmd_serve(&args[2..]),
        "graph" => cmd_graph(&args[2..]),
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
}

fn usage() {
    eprint!(
        "rigger - a config-driven, event-sourced multi-agent dev-loop harness\n\n\
usage:\n  \
rigger run [spec] [opts]    run the workflow (opts below)\n  \
rigger serve [opts]         run as an MCP server (= run --driver workflow)\n  \
rigger graph --around <id>  print the context subgraph around a node\n  \
rigger validate             load and validate the workflow + agents\n  \
rigger init                 set up a project: scaffold .rigger/ (workflow.yml +\n                              \
an agents/ folder) and install the Claude Code\n                              \
SessionStart hook (it runs `rigger prime`)\n  \
rigger setup                alias for `rigger init` (kept for compatibility)\n  \
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
    let grounder = select_grounder(&cfg.workflow.defaults.grounder);
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
    let grounder = select_grounder(&cfg.workflow.defaults.grounder);
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
        });
        let server = rigger::mcpserver::Server::new(&driver, &store, conductor::STREAM, &peers);
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

/// `rigger setup` is now a thin alias for `rigger init` - kept so existing muscle
/// memory and scripts keep working - since `init` already does the full setup
/// (scaffold + hook).
fn cmd_setup() -> Res {
    cmd_init()
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

/// Build the grounder named by `defaults.grounder` (§3.2, §5.4, R4): `nop` and
/// `grep` (and the empty default) resolve via `grounder::grounder_for`; the
/// semantic names (`vector`/`turbovec`) resolve to the real turbovec engine only
/// when compiled with `-F turbovec`, falling back to grep with a note if its model
/// is unavailable. Anything else falls back to grep via `grounder_for`.
#[cfg(feature = "turbovec")]
fn select_grounder(name: &str) -> Box<dyn Grounder> {
    match name.to_lowercase().as_str() {
        "vector" | "turbovec" => match rigger::grounder::turbovec::Turbovec::new(".") {
            Ok(tv) => Box::new(tv),
            Err(e) => {
                eprintln!("rigger: turbovec unavailable ({e}); falling back to grep");
                Box::new(Grep { root: ".".into() })
            }
        },
        other => rigger::grounder::grounder_for(other, "."),
    }
}

#[cfg(not(feature = "turbovec"))]
fn select_grounder(name: &str) -> Box<dyn Grounder> {
    rigger::grounder::grounder_for(name, ".")
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

/// The scaffolded workflow (§3.2): a worked plan -> implement -> review ->
/// integrate pipeline that demonstrates the documented shape - a `defaults:` block
/// (autonomy + grounder), a reusable `gates:` library, and stages exercising
/// `needs`, `agent`/`agents`, `strategy`, `gates`, `adversary`, `adjudicator`,
/// `autonomy`, `produces`, `coverage`, and `on_pass`. It loads through
/// `config::load` against the agents scaffolded alongside it.
const SCAFFOLD_WORKFLOW: &str =
    "# Scaffolded by `rigger init`. A worked plan -> implement -> review -> integrate\n\
# pipeline showing the full DAG shape. Replace the gate commands with your own.\n\
name: example\n\
\n\
defaults:\n  \
autonomy: auto_notify   # manual | auto_notify | silent\n  \
grounder: grep          # grep (default) | turbovec (needs the cargo feature)\n\
\n\
gates:                    # a reusable library of commands, referenced by name\n  \
build: { run: \"echo build ok; true\", kind: core }\n  \
test:  { run: \"echo test ok; true\",  kind: core }\n  \
lint:  { run: \"echo lint ok; true\",  kind: elevated }\n\
\n\
stages:\n  \
plan:\n    \
agent: planner\n    \
produces: dag           # decompose the spec into a unit DAG at runtime\n    \
coverage: \"the spec is decomposed into units\"\n\
\n  \
implement:\n    \
needs: [plan]\n    \
agent: implementer\n    \
strategy: fan-out       # one worker per ready unit, in isolated worktrees\n    \
partition: by-blast-radius\n    \
gates: [build, test]    # red -> green enforced around the change\n    \
coverage: \"each unit is implemented and its gates pass\"\n\
\n  \
review:                   # three-tier: lenses -> adversary -> adjudicator\n    \
needs: [implement]\n    \
strategy: fan-out\n    \
agents: [reviewer.architecture, reviewer.technical]   # tier 1: the expert lenses\n    \
adversary: adversary           # tier 2: reviews the lenses and refutes them\n    \
adjudicator: devils-advocate   # tier 3: neutral judge; its verdict gates the stage\n    \
autonomy: manual               # pause for a human before integrating\n    \
coverage: \"the change passes three-tier adversarial review\"\n\
\n  \
integrate:\n    \
needs: [review]\n    \
agent: integrator\n    \
gates: [build, test, lint]\n    \
on_pass: merge          # land + reindex + record\n    \
coverage: \"the change is integrated on a green build\"\n";

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
    (
        "integrator.md",
        "---\n\
id: integrator\n\
model: sonnet\n\
tools: [Read, Bash]\n\
isolation: worktree\n\
recurse: false\n\
---\n\
You land the reviewed change: rebase on the latest base, re-run the gates, and\n\
merge only on a fully green build. Report the integrating commit.\n",
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

        // Seven agents: planner, implementer, two reviewer lenses, the adversary,
        // the adjudicator, the integrator.
        assert_eq!(cfg.agents.len(), 7, "scaffold agent count");
        // Four stages: plan -> implement -> review -> integrate.
        assert_eq!(cfg.workflow.stages.len(), 4, "scaffold stage count");
        // Three gates in the reusable library.
        assert_eq!(cfg.workflow.gates.len(), 3, "scaffold gate count");

        // The scaffold exercises the full shape: a producer, a fan-out lens set, the
        // three-tier review (lenses -> adversary -> adjudicator), a manual-autonomy
        // stage, and an on_pass: merge.
        let plan = &cfg.workflow.stages["plan"];
        assert_eq!(plan.produces, "dag");
        let implement = &cfg.workflow.stages["implement"];
        assert_eq!(implement.strategy, "fan-out");
        assert_eq!(implement.needs, ["plan"]);
        let review = &cfg.workflow.stages["review"];
        assert_eq!(review.agents.len(), 2, "tier 1: the expert lenses");
        assert_eq!(review.adversary, "adversary", "tier 2: refutes the lenses");
        assert_eq!(
            review.adjudicator, "devils-advocate",
            "tier 3: the neutral adjudicator gates"
        );
        assert_eq!(review.autonomy, "manual");
        let integrate = &cfg.workflow.stages["integrate"];
        assert_eq!(integrate.on_pass, "merge");
        assert_eq!(cfg.workflow.defaults.grounder, "grep");
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

    #[test]
    fn project_identity_is_never_empty() {
        assert!(!project_identity().is_empty());
    }
}
