//! The harness CLI. `rigger run` executes the configured workflow with the default
//! CLI agent driver and the embedded SQLite event store; `rigger graph` inspects
//! the context graph; `rigger init`/`setup` scaffold a project.

use std::path::Path;
use std::process::Command;

use rigger::conductor::{self, Deps};
use rigger::config;
use rigger::contextgraph::{self, sqlite::Projector, Projection};
use rigger::driver::cli;
use rigger::eventstore::{sqlite::Store, Direction, EventStore, Filter};
use rigger::gate::ExecRunner;
use rigger::grounder::{Grep, Grounder};
use rigger::ledger::RunState;
use rigger::sidecar::PeerDecision;
use rigger::{hooks, spec};

const RIGGER_DIR: &str = ".rigger";

type Res = Result<(), Box<dyn std::error::Error>>;

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
rigger run [spec]           run the workflow with the standalone CLI driver\n  \
rigger serve                run as an MCP server for the Claude Code workflow shim\n  \
rigger graph --around <id>  print the context subgraph around a node\n  \
rigger validate             load and validate the workflow + agents\n  \
rigger init                 scaffold .rigger/ (workflow.yml + an agents/ folder)\n  \
rigger setup                init, then install a Claude Code SessionStart hook\n  \
rigger prime                print recent decisions (what the hook runs)\n\n\
storage and graph live in ./.rigger/ (per project, like .git/).\n"
    );
}

fn db_path(name: &str) -> String {
    Path::new(RIGGER_DIR)
        .join(name)
        .to_string_lossy()
        .into_owned()
}

fn cmd_run(args: &[String]) -> Res {
    let cfg = config::load(".")?;
    let mut criteria = Vec::new();
    if let Some(spec_path) = args.first() {
        let text = std::fs::read_to_string(spec_path)
            .map_err(|e| format!("read spec {spec_path}: {e}"))?;
        criteria = spec::extract_criteria(&text);
    }
    std::fs::create_dir_all(RIGGER_DIR)?;
    let store = Store::open(&db_path("events.db"))?;
    let graph = Projector::open(&db_path("graph.db"))?;
    let driver = cli::Driver::default();
    let grounder = select_grounder();
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

fn cmd_serve(_args: &[String]) -> Res {
    let cfg = config::load(".")?;
    std::fs::create_dir_all(RIGGER_DIR)?;
    let store = Store::open(&db_path("events.db"))?;
    let graph = Projector::open(&db_path("graph.db"))?;
    let driver = rigger::driver::workflow::Driver::new();
    let grounder = select_grounder();
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
                criteria: Vec::new(),
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

fn cmd_init() -> Res {
    std::fs::create_dir_all(Path::new(RIGGER_DIR).join("agents"))?;
    write_if_absent(
        &Path::new(RIGGER_DIR).join("workflow.yml"),
        SCAFFOLD_WORKFLOW,
    );
    write_if_absent(
        &Path::new(RIGGER_DIR).join("agents").join("builder.md"),
        SCAFFOLD_AGENT,
    );
    println!("scaffolded .rigger/workflow.yml and .rigger/agents/builder.md");
    Ok(())
}

fn cmd_setup() -> Res {
    cmd_init()?;
    std::fs::create_dir_all(".claude")?;
    let settings_path = Path::new(".claude").join("settings.json");
    let existing = std::fs::read(&settings_path).unwrap_or_default();
    let merged = hooks::install_session_start(&existing, "rigger prime")?;
    std::fs::write(&settings_path, merged)?;
    println!("installed a SessionStart hook in .claude/settings.json (it runs `rigger prime`)");
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

/// Build the grounder: the real turbovec engine when compiled with `-F turbovec`
/// (falling back to grep if its model is unavailable), grep otherwise.
#[cfg(feature = "turbovec")]
fn select_grounder() -> Box<dyn Grounder> {
    match rigger::grounder::turbovec::Turbovec::new(".") {
        Ok(tv) => Box::new(tv),
        Err(e) => {
            eprintln!("rigger: turbovec unavailable ({e}); falling back to grep");
            Box::new(Grep { root: ".".into() })
        }
    }
}

#[cfg(not(feature = "turbovec"))]
fn select_grounder() -> Box<dyn Grounder> {
    Box::new(Grep { root: ".".into() })
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

const SCAFFOLD_WORKFLOW: &str = "name: example\n\
gates:\n  \
smoke: { run: \"echo 'no gate configured yet'; true\", kind: core }\n\
stages:\n  \
build:\n    \
agent: builder\n    \
gates: [smoke]\n";

const SCAFFOLD_AGENT: &str = "---\n\
id: builder\n\
model: sonnet\n\
tools: [Read, Edit, Bash]\n\
---\n\
You are a builder agent. Implement the task, run the gates, and report concisely.\n";
