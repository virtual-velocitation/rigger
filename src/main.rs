//! The harness CLI and the single composition root: it constructs the concrete
//! adapters (event store, agent driver, grounder, projector) and injects them into
//! the conductor, which depends only on ports. `rigger run` executes the configured
//! workflow - the agent driver (`--driver cli|workflow`) and the event store
//! (`--eventstore sqlite|kurrentdb`) are selected by flag; `rigger graph` inspects
//! the context graph; `rigger init`/`setup` scaffold a project.

mod cli;

use cli::*;

// Spec 74, criterion 2: the SAME derivation seam `build.rs` embeds at compile time
// (`build/gitsemver.rs`, already `#[path]`-included into `build.rs` and the two
// derivation-seam test binaries - see that file's own module doc comment: "that is the
// ONE derivation seam ... never a parallel reimplementation of it") is included here a
// third time so `cmd_validate` can re-derive the CHECKOUT's current version for the
// behind-the-tree comparison. This is a live, runtime invocation of `go-gitsemver` (and
// git) - but only from `validate`'s comparison path, never from `rigger version`'s own
// self-report, which stays the compile-time-only `GITSEMVER_VERSION` const below. The
// Global constraint ("version values are computed at COMPILE time only - no runtime git
// or tool invocation") reads narrowly, scoped to that self-report: `git_is_ancestor`
// (below) already shells to git live inside this exact command for the analogous
// spec-18-criterion-9 workflow-drift ordering diagnostic, an established precedent for a
// runtime tool/git call from `validate` for comparison purposes.
#[path = "../build/gitsemver.rs"]
#[allow(dead_code)]
mod gitsemver;
#[cfg(test)]
#[path = "../tests/common/fixtures/mod.rs"]
mod test_support;

/// The canonical command surface, in dispatch order. This is the SINGLE list of
/// subcommand names: the runtime reads it (the unknown-command help below names the
/// known commands from it) and `rigger docs` reads it so the generated discipline's
/// command list is code-derived, not hand-copied. Keep it in step with the `main`
/// dispatch match below - the same must-agree discipline [`RUN_BRANCH`] keeps with the
/// JS driver. `docs_context` and the `commands_registry_agrees_with_dispatch` test guard
/// it against drift.
const SUBCOMMANDS: &[&str] = &[
    "run",
    "step",
    "resume-unit",
    "reported",
    "prompt",
    "scratch",
    "serve",
    "workflow",
    "graph",
    "stats",
    "canary",
    "playbooks",
    "replay",
    "status",
    "dash",
    "watch",
    "ground",
    "reindex",
    "symbols-index",
    "emit",
    "progress",
    "result",
    "peers",
    "reset",
    "validate",
    "init",
    "setup",
    "docs",
    "prime",
    "instructions",
    "mcp",
    "grep-guard",
    "guard-write",
    "hook",
    "version",
    "help",
];

/// A reader that goes away early (`rigger prime | head -1`) closes stdout under the command
/// still writing to it, and `println!` panics on the broken pipe. That is not a failure of the
/// command: the reader has everything it asked for. So, for every command at once, a panic that
/// is std's report of a failed stdout write on a broken pipe ends the process quietly with
/// status 0; every other panic keeps the default report.
fn exit_quietly_when_stdout_closes() {
    let report = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = info
            .payload()
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| info.payload().downcast_ref::<&str>().copied())
            .unwrap_or_default();
        if is_closed_stdout_report(payload) {
            std::process::exit(0);
        }
        report(info);
    }));
}

/// Whether a panic `message` is std's report of a stdout write that failed on a broken pipe
/// (`failed printing to stdout: Broken pipe (os error 32)`).
fn is_closed_stdout_report(message: &str) -> bool {
    message.starts_with("failed printing to stdout")
        && message
            .to_ascii_lowercase()
            .contains(&std::io::ErrorKind::BrokenPipe.to_string())
}

fn main() {
    exit_quietly_when_stdout_closes();
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        usage();
        std::process::exit(2);
    }
    let result = match args[1].as_str() {
        "run" => cmd_run(&args[2..]),
        "step" => cmd_step(&args[2..]),
        "resume-unit" => cmd_resume_unit(&args[2..]),
        "reported" => cmd_reported(&args[2..]),
        "prompt" => cmd_prompt(&args[2..]),
        "scratch" => cmd_scratch(&args[2..]),
        "serve" => cmd_serve(&args[2..]),
        "workflow" => cmd_workflow(&args[2..]),
        "graph" => cmd_graph(&args[2..]),
        "stats" => cmd_stats(&args[2..]),
        "canary" => cmd_canary(&args[2..]),
        "playbooks" => cmd_playbooks(&args[2..]),
        "replay" => cmd_replay(&args[2..]),
        "status" => cmd_status(&args[2..]),
        "dash" => cmd_dash(&args[2..]),
        "watch" => cmd_watch(&args[2..]),
        "ground" => cmd_ground(&args[2..]),
        "reindex" => cmd_reindex(&args[2..]),
        "symbols-index" => cmd_symbols_index(&args[2..]),
        "emit" => cmd_emit(&args[2..]),
        "progress" => cmd_progress(&args[2..]),
        "result" => cmd_result(&args[2..]),
        "peers" => cmd_peers(&args[2..]),
        "reset" => cmd_reset(&args[2..]),
        "validate" => cmd_validate(&args[2..]),
        "init" => cmd_init(),
        "setup" => cmd_setup(&args[2..]),
        "docs" => cmd_docs(&args[2..]),
        "prime" => cmd_prime(&args[2..]),
        "instructions" => cmd_instructions(&args[2..]),
        "mcp" => cmd_mcp(&args[2..]),
        "grep-guard" => cmd_grep_guard(&args[2..]),
        "guard-write" => cmd_guard_write(&args[2..]),
        "hook" => cmd_hook(&args[2..]),
        "version" | "--version" | "-V" => cmd_version(),
        "help" | "-h" | "--help" => {
            usage();
            Ok(())
        }
        other => {
            eprintln!("rigger: unknown command {other:?}");
            eprintln!("known commands: {}", SUBCOMMANDS.join(", "));
            usage();
            std::process::exit(2);
        }
    };

    let code = match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("rigger: {e}");
            1
        }
    };

    std::process::exit(code);
}

/// The full `rigger --help` / bare-`rigger` text, single-sourced so a test can pin its
/// shape (spec 61, ITEM SHARDING AND THE JOBS CAP: a flag tag spliced into the middle of
/// an unrelated sentence is exactly the regression class
/// `usage_text_gives_the_jobs_flag_its_own_description_line` guards against).
const USAGE_TEXT: &str =
    "rigger - a config-driven, event-sourced multi-agent dev-loop harness\n\n\
usage:\n  \
rigger run [spec] [opts]    run the workflow (opts below)\n  \
rigger step [--spec <path>]      advance the run one frontier via the replay driver\n            \
[--base <ref>]        and print the newly parked spawn wave + a done flag\n                              \
as JSON. --base (default origin/main) anchors a NEW run\n                              \
branch; if it is unresolvable the branch is created off\n                              \
HEAD. An existing run branch is reused, never reset.\n                              \
--fresh begins a NEW run for the spec even if the latest\n                              \
matches (pass on the first step to restart a wedged run);\n                              \
--rebase-definition accepts a drifted definition and\n                              \
continues, else a live-run step HALTS on definition drift\n  \
rigger reported <id>        exit 0 iff spawn <id> already has a recorded result in\n                              \
this project's run stream (else non-zero). A read-only check\n                              \
of whether a spawn reported yet; the death courier records\n                              \
atomically instead via `rigger result --if-absent`\n  \
rigger prompt <id>          print the parked spawn's full prompt (persona + task).\n                              \
The step wave is a slim manifest; each worker fetches its\n                              \
own prompt from the log by spawn id (spawn-by-reference)\n  \
rigger scratch <id>         print spawn <id>'s own rigger-assigned scratch container\n                              \
(the exact dir the per-spawn reclaim reaps at its terminus);\n                              \
a worker points agent-created scratch and manual\n                              \
CARGO_TARGET_DIR there instead of an unbucketed literal\n  \
rigger workflow [spec]      turn-key: launch the per-project Node driver, which\n                              \
spawns `rigger serve`, runs each agent via the Agent\n                              \
SDK, and drives the loop (one command; run `rigger\n                              \
setup` first - it provisions the driver in .rigger/shim/)\n  \
rigger serve [opts]         run as an MCP server the driver connects to\n  \
rigger graph --around <id>  print the context subgraph around a node\n  \
rigger graph --show <entity> print an entity's definition site + line-numbered body\n                              \
(the text half of lookup; resolves a full id or a bare name)\n  \
rigger graph build          fold the project's source into the graph from a cold\n                              \
checkout (no run required)\n  \
rigger graph communities    derive the code lens's coupling communities offline and\n                              \
[--resolution <r>]          record them as events (deterministic; default r=1.0)\n  \
rigger graph concepts       derive the concepts lens's intent-layer grouping offline\n                              \
[--resolution <r>]          and record them as events (deterministic; default r=1.0)\n  \
rigger stats                print the run's operator metrics: first-pass yield,\n                              \
per-gate remediation counts, escalation rate, and\n                              \
review approve/reject counts. --canary reports the\n                              \
latest canary run's judge-the-judges recall scorecard\n  \
rigger canary               run the review panel against the seeded-defect corpus\n            \
[--corpus <dir>]         (default ./canaries) and score per-tier catch rate,\n                              \
adjudicator correctness, and verdict stability under\n                              \
finding-order shuffle, into the project's canary stream\n                              \
(read back with `rigger stats --canary`)\n            \
[--jobs <n>]              caps the total concurrent review-panel spawns\n                              \
across sharded items and each item's lens fan-out\n                              \
together (default: the crate's default worker\n                              \
width, floored at 2)\n            \
[--model <tier>=<id>]    pins a tier's (lens/adversary/adjudicator)\n                              \
model for this run only, repeatable; the scorecard\n                              \
header prints its resolved id\n  \
rigger playbooks --rebuild  reconstruct the distilled playbook pool under\n                              \
.rigger/playbooks/ from the recorded LessonLearned\n                              \
stream: deduplicated, trigger-scoped agent-files the\n                              \
lessons injector ranks by blast-radius relevance (a\n                              \
rebuildable projection of the log, never hand-edited)\n  \
rigger replay <run|latest>  re-drive a completed run's recorded trajectory under a\n            \
--against <rev>          candidate config (workflow + prompts at git <rev>) in an\n                              \
isolated scratch namespace, and print the stats diff\n                              \
vs the recorded baseline. Never writes the real run\n                              \
stream - past runs become a regression corpus for a\n                              \
config edit (\"did that change regress first-pass yield?\")\n  \
rigger status [--json|--line] present the live per-agent view of the current run: for\n                              \
each in-flight agent, what it is doing (latest progress),\n                              \
its heartbeat age, and how long since its last store event\n                              \
(the blackout). --json prints the shim/dash machine shape;\n                              \
--line prints only the one-line statusline (the same line\n                              \
`rigger setup` registers as the editor's status bar)\n  \
rigger dash [--port <n>]    serve the read-only observability page on 127.0.0.1\n                              \
(default port 7420) with live past/present/future views;\n                              \
--export <path> writes the equivalent static snapshot\n  \
rigger watch [--interval <s>] the driver-independent watchdog: polls the store,\n            \
[--once]                  process table, and status for the five rigger-watch-a-run\n                              \
signals (escalated blockers, heartbeat staleness, dash\n                              \
liveness, reject-recurrence trend, frontier progress) plus\n                              \
store integrity, printing one line per anomaly naming\n                              \
signal, subject, and response skill. --once prints standing\n                              \
anomalies and exits (cron/CI); default streams (poll every\n                              \
180s, dedup'd) and never talks to the driver - it works\n                              \
with the driver dead\n  \
rigger ground <query> [k]   print up to k (default 8) repo references the project's\n                              \
configured grounder finds for <query>, as `file:line: text`\n  \
rigger reindex <file>...    incrementally re-index the named files in the project's\n                              \
persisted grounding index (the grounder's reindex), so a\n                              \
later `rigger ground` reflects just-landed changes\n  \
rigger symbols-index [dir]  build + persist the structural symbol index over [dir]\n                              \
(default .) and print its path + file count - the fresh-\n                              \
process determinism harness for the symbols grounder (spec 15)\n  \
rigger emit <type> <json>   append {{type, data:<json>}} to the event store and fold\n                              \
it into the context graph (the CLI form of rigger_emit)\n  \
rigger progress <id> <act>  record one live progress line for spawn <id> to the\n                              \
separate .rigger/progress.db (never the run stream), so an\n                              \
observer can see what a working agent is doing between\n                              \
milestones - `rigger status` and the dash present it\n  \
rigger result <id> [out]    record a parked spawn's outcome to the run log so the next\n                              \
step advances past it: <out> (or stdin) is the agent's output\n                              \
(with --error, its failure message); --if-absent records only\n                              \
if the id has no result; --meta <json> adds bookkeeping\n  \
rigger peers [file ...]     print peer decisions, lessons, and findings from the\n                              \
context graph, scoped to the given files (the CLI form of\n                              \
rigger_peers)\n  \
rigger reset --runs         drop every superseded / dead run's decisions and\n                              \
findings from the context graph, keeping every lesson and\n                              \
the active run's own decisions/findings. Sheds dead-run\n                              \
grounding noise without wiping the store: it deletes no\n                              \
event. reset itself does write the log once, on a store\n                              \
still under the legacy basename namespace: the one-time\n                              \
identity migration renames those streams and records one\n                              \
DecisionMade before either mode prunes. When no driver is\n                              \
alive, it also closes the current run's units whose branch\n                              \
work is landed on rigger-run, appending the UnitIntegrated\n                              \
a hand landing never recorded\n  \
rigger reset --derived      compact the EVENT LOG: keep the latest event per\n                              \
replay key of each derived index type, delete the\n                              \
superseded re-recordings, and vacuum so the file shrinks\n                              \
on disk. Every other event survives. Sheds the\n                              \
duplication a log accreted before the ingest dedup;\n                              \
composes with --runs (each prunes its own accumulation).\n                              \
Refuses while run machinery looks live (a held step\n                              \
lock, a non-terminal unit, an in-flight spawn, or a live\n                              \
driver registration), naming what is live: compaction\n                              \
leaves revision gaps by design, and a stale writer can\n                              \
reissue one and reorder the log - the corruption this\n                              \
guard exists to prevent. --force-live skips the check\n                              \
entirely and checks nothing: pass it only once you are\n                              \
certain no writer is using this store, since forcing\n                              \
past a genuinely live writer is exactly that corruption\n  \
rigger reset --build-cache  reclaim the SHARED gate build cache (a pure cache,\n                              \
always safe to cold-rebuild) under the scratch root.\n                              \
Composes freely with --runs/--derived (each sheds its\n                              \
own accumulation); reports the exact bytes reclaimed,\n                              \
or 0 when there was nothing to reclaim. Refuses loudly\n                              \
- never waiting - while a rigger-launched build still\n                              \
holds the cache's guard lock; retry once it is idle\n  \
rigger reset --scratch-orphans\n                              \
reclaim every cache-home scratch root (under\n                              \
$XDG_CACHE_HOME/rigger, else ~/.cache/rigger) whose repo\n                              \
no longer exists: a root keyed on a deleted checkout or a\n                              \
test fixture's tempdir has no owner left to reclaim it.\n                              \
Rigger does this itself whenever it creates a default-\n                              \
placed root; this is the explicit on-demand form. Reports\n                              \
the number of roots reclaimed; composes with the others\n  \
rigger validate             load and validate the workflow + agents\n  \
rigger init                 set up a project: scaffold .rigger/ (workflow.yml +\n                              \
an agents/ folder) and install the Claude Code\n                              \
SessionStart hook (it runs `rigger prime`)\n  \
rigger setup                full setup: everything `init` does, PLUS install the\n                              \
native /rigger Claude Code workflow (.claude/workflows/\n                              \
rigger.js) and provision the JS driver (.rigger/shim/ +\n                              \
npm install). After it: run `/rigger <spec>` in Claude\n                              \
Code (primary), or `rigger workflow` as a fallback\n  \
rigger docs                 render the code-derived docs (every registry skill and\n                              \
handbook page) into their committed paths\n  \
rigger prime [<spec>]       print recent decisions (what the hook runs); given a spec\n                              \
path, also names `rigger validate <spec>` (the pre-launch\n                              \
spec lint) as a next step\n  \
rigger instructions         print the instructions every spawned agent is held to:\n                              \
the built-in law and working discipline, then each\n                              \
operator file in .rigger/instructions/ (filename\n                              \
order)\n  \
rigger version              print the crate version and the build-provenance id\n                              \
(a git commit/describe embedded at build time) so an\n                              \
agent can identify the exact binary. Also `--version`\n\n\
run/serve options:\n  \
--driver <cli|workflow>          cli (default): standalone claude subprocess;\n                                   \
workflow: in-Claude-Code MCP server\n  \
--eventstore <sqlite|kurrentdb>  sqlite (default): embedded file in .rigger/;\n                                   \
kurrentdb: shared server backend, always available\n  \
--conn <url>                     KurrentDB connection url (or set KURRENTDB_CONN)\n  \
--fresh                          begin a NEW run even if the latest run matches this\n                                   \
spec (which is otherwise adopted/resumed). The evented\n                                   \
restart for a run wedged in a terminal state (e.g. an\n                                   \
escalated plan-critique) whose spec is unchanged; the\n                                   \
prior run stays in the log as history and context\n  \
--rebase-definition              accept an on-disk definition (workflow.yml + agent\n                                   \
prompts) that drifted from what this live run pinned at\n                                   \
start: record the supersession and continue instead of\n                                   \
halting. The explicit mid-campaign-edit escape (a live\n                                   \
run otherwise HALTS loudly on definition drift)\n\n\
storage and graph live in ./.rigger/ (per project, like .git/), scoped to the\n\
project identity so one backend can hold many projects without their data mixing.\n";

fn usage() {
    eprint!("{USAGE_TEXT}");
}
