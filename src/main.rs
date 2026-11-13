//! The harness CLI and the single composition root: it constructs the concrete
//! adapters (event store, agent driver, grounder, projector) and injects them into
//! the conductor, which depends only on ports. `rigger run` executes the configured
//! workflow, spawning each agent as its own host CLI process - the event store
//! (`--eventstore sqlite|kurrentdb`) is selected by flag; `rigger graph` inspects
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
// or tool invocation") reads narrowly, scoped to that self-report: a runtime tool/git call
// from `validate` for comparison purposes is in bounds.
#[path = "../build/gitsemver.rs"]
#[allow(dead_code)]
mod gitsemver;
#[cfg(test)]
#[allow(dead_code, unused_imports)]
#[path = "../tests/common/fixtures/prompt_surface.rs"]
mod prompt_surface_fixtures;
#[cfg(test)]
#[path = "../tests/common/fixtures/mod.rs"]
mod test_support;

/// The canonical command surface, in dispatch order. This is the SINGLE list of
/// subcommand names: the runtime reads it (the unknown-command help below names the
/// known commands from it) and `rigger docs` reads it so the generated discipline's
/// command list is code-derived, not hand-copied. Keep it in step with the `main`
/// dispatch match below. `docs_context` and the `commands_registry_agrees_with_dispatch` test guard
/// it against drift.
const SUBCOMMANDS: &[&str] = &[
    "run",
    "resume-unit",
    "hold",
    "release",
    "graph",
    "stats",
    "canary",
    "playbooks",
    "replay",
    "status",
    "dash",
    "watch",
    "ground",
    "emit",
    "gate",
    "progress",
    "result",
    "peers",
    "reset",
    "critique",
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

/// The subcommands whose arguments are not their own flags, so a `-h` or `--help` among them is
/// one of those arguments, handed over like any other, never a request for help:
/// - `emit`, `result` and `progress` take a payload recorded as given: an event's type and JSON
///   body, an agent's output or failure text, an activity line.
/// - `ground` takes a free-text query, and `--help` is a fair thing to look up.
/// - `mcp`, `grep-guard`, `guard-write` and `hook` are launched by the host, from the argv its
///   server or hook registration composes, and the host reads their answer. `guard-write` fails
///   closed, so an argument it does not know must deny with its blocking exit code, never exit 0.
const HELP_PASSES_THROUGH: &[&str] = &[
    "emit",
    "result",
    "progress",
    "ground",
    "mcp",
    "grep-guard",
    "guard-write",
    "hook",
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
    if let Some(code) = answer_help(&args[1], &args[2..]) {
        std::process::exit(code);
    }
    let result = match args[1].as_str() {
        "run" => cmd_run(&args[2..]),
        "resume-unit" => cmd_resume_unit(&args[2..]),
        "hold" => cmd_hold(&args[2..]),
        "release" => cmd_release(&args[2..]),
        "graph" => cmd_graph(&args[2..]),
        "stats" => cmd_stats(&args[2..]),
        "canary" => cmd_canary(&args[2..]),
        "playbooks" => cmd_playbooks(&args[2..]),
        "replay" => cmd_replay(&args[2..]),
        "status" => cmd_status(&args[2..]),
        "dash" => cmd_dash(&args[2..]),
        "watch" => cmd_watch(&args[2..]),
        "ground" => cmd_ground(&args[2..]),
        "emit" => cmd_emit(&args[2..]),
        "gate" => cmd_gate(&args[2..]),
        "progress" => cmd_progress(&args[2..]),
        "result" => cmd_result(&args[2..]),
        "peers" => cmd_peers(&args[2..]),
        "reset" => cmd_reset(&args[2..]),
        "critique" => cmd_critique(&args[2..]),
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
            help(&args[2..]);
            Ok(())
        }
        other => {
            rigger::whole_line::say(format_args!("rigger: unknown command {other:?}"));
            rigger::whole_line::say(format_args!("known commands: {}", SUBCOMMANDS.join(", ")));
            usage();
            std::process::exit(2);
        }
    };

    let code = match result {
        Ok(()) => 0,
        Err(e) => {
            rigger::whole_line::say(format_args!("rigger: {e}"));
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
rigger resume-unit <unit>   give an escalated unit of the current run more attempts\n            \
[--attempts <n>]  (default 1) on its same branch: the next `rigger run`\n                              \
re-parks its implementer there. Refuses, recording\n                              \
nothing, when the unit is not escalated or its branch\n                              \
is gone\n  \
rigger hold --reason <text> hold the current run for the operator, the text its\n                              \
reason: no probe ends this hold; `rigger release`, a\n                              \
new run, or `rigger run` reaching done does. Refuses,\n                              \
recording nothing, when no run is live\n  \
rigger release              end the current run's hold, whichever writer declared\n                              \
it; with no hold open it records nothing and says so\n  \
rigger graph --lookup <term>... answer each term from the graph, your tree and its prose\n  \
rigger graph --around <id>  print the context subgraph around a node\n  \
rigger graph --show <entity> print an entity's definition site + line-numbered body\n                              \
(the text half of lookup; resolves a full id, a bare name or a path)\n  \
rigger graph build          advance the project's code store to the head, from a\n                              \
cold checkout or a live one (no run required)\n  \
rigger graph communities    print the code store's coupling communities, read-only\n                              \
[--resolution <r>]          (stored at the default r=1.0; any other r is computed)\n  \
rigger graph concepts       print the code store's intent-layer concepts, read-only\n                              \
[--resolution <r>]          (stored at the default r=1.0; any other r is computed)\n  \
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
(the blackout). --json prints the dash machine shape;\n                              \
--line prints only the one-line statusline (the same line\n                              \
`rigger setup` registers as the editor's status bar)\n  \
rigger dash [--port <n>]    serve the read-only observability page on 127.0.0.1\n                              \
(default port 7420) with live past/present/future views;\n                              \
--export <path> writes the equivalent static snapshot\n  \
rigger watch [--interval <s>] the driver-independent watchdog: polls the store,\n            \
[--once]                  process table, and status for the five rigger-watch-a-run\n                              \
signals (escalated blockers, heartbeat staleness, dash\n                              \
liveness, reject-recurrence trend, frontier progress) plus\n                              \
store integrity and a red CI check on the run branch's tip\n                              \
(read with gh; a CI gh cannot read is noted on stderr),\n                              \
printing one line per anomaly naming signal, subject,\n                              \
and response. --once prints standing\n                              \
anomalies and exits (cron/CI); default streams (poll every\n                              \
180s, dedup'd) and never talks to the driver - it works\n                              \
with the driver dead\n  \
rigger ground <query> [k]   print up to k (default 8) repo references the project's\n                              \
configured grounder finds for <query>, as `file:line: text`\n  \
rigger emit <type> <json>   append {{type, data:<json>}} to the event store and fold\n                              \
it into the context graph (the CLI form of rigger_emit)\n  \
rigger gate <id>            ask the driver serving this run to run declared gate <id>\n                              \
on this unit worktree's committed tree; it signs and\n                              \
records the verdict and reuses a green one for that commit\n                              \
instead of running the gate again. With no driver serving,\n                              \
runs the gate for information only and records nothing.\n                              \
Refuses uncommitted changes\n  \
rigger progress <id> <act>  record one live progress line for spawn <id> to the\n                              \
separate .rigger/progress.db (never the run stream), so an\n                              \
observer can see what a working agent is doing between\n                              \
milestones - `rigger status` and the dash present it\n  \
rigger result <id> [out]    record a spawn's outcome to the run log, which the next\n                              \
`rigger run` answers that spawn from: <out> (or stdin) is\n                              \
the agent's output (with --error, its failure message). It\n                              \
refuses to replace the result of a spawn that already\n                              \
ended unless --supersede, the repair a reviewer halt names\n  \
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
alive and every spawn of the run has ended on a real\n                              \
result, it also closes the current run's units whose\n                              \
branch work is landed on rigger-run, appending the\n                              \
UnitIntegrated a hand landing never recorded; a dead run's\n                              \
hung spawn (answered only by its liveness fault)\n                              \
keeps them open and is named with its remedy,\n                              \
`rigger result <id>`. It removes a stale graph.db.pruned,\n                              \
the pruned copy a rebuild's stopped swap\n                              \
left, unless another `rigger setup` or `rigger reset` holds graph.db.lock\n  \
rigger reset --derived      delete every derived event of the EVENT LOG in one\n                              \
transaction (`rigger setup` runs the same delete once\n                              \
its rebuilt graph is in place), and vacuum so the file\n                              \
shrinks on disk.\n                              \
Once the delete has run it removes the code store's\n                              \
facts memo (a cache reads refill); a refused --derived\n                              \
removes nothing.\n                              \
Every other event survives;\n                              \
composes with --runs (each sheds its own accumulation).\n                              \
Refuses while the run is live (a held step lock, an\n                              \
in-flight spawn's marker inside its wall-clock bound,\n                              \
or a driver registration's heartbeat inside the idle\n                              \
window), naming what is live (a dead driver's run is\n                              \
not): the delete\n                              \
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
rigger critique <spec>      critique the spec before any run: the workflow's critic\n                              \
(the plan-critique gate's adversary, else\n                              \
an adversary step in defaults.review) reads the spec\n                              \
text pass after pass, each recorded keyed on the\n                              \
text's content hash, until a pass adds no finding or\n                              \
its attempts reach the ceiling (the effective\n                              \
max_retries of the stage it is read from); the union\n                              \
of the passes is printed, and a text exhausted, or at\n                              \
the ceiling with a pass recorded, is answered from\n                              \
the record with no spawn; takes --eventstore/--conn\n                              \
as run does\n  \
rigger validate             load and validate the workflow + agents\n  \
rigger init                 set up a project: scaffold .rigger/ (workflow.yml +\n                              \
an agents/ folder) and install the Claude Code\n                              \
SessionStart hook (it runs `rigger prime`)\n  \
rigger setup                full setup: everything `init` does, PLUS install the\n                              \
registry skills, the MCP server entry and the session\n                              \
settings. After it: `rigger run <spec> --base <ref>`\n  \
rigger docs                 render the code-derived docs (every registry skill and\n                              \
handbook page) into their committed paths\n  \
rigger docs --check         check every rendered output against a fresh render,\n                              \
writing nothing: prints `fresh <path>` or `drifted <path>`\n                              \
per output on disk; exits 1 when any drifted\n  \
rigger prime [<spec>]       print recent decisions (what the hook runs); given a spec\n                              \
path, also names `rigger validate <spec>` (the pre-launch\n                              \
spec lint) as a next step\n  \
rigger instructions         print the instructions every spawned agent is held to:\n                              \
each built-in with its state (in force, replaced,\n                              \
switched off), then each unshadowed operator file in\n                              \
.rigger/instructions/ (filename order)\n  \
rigger version              print the crate version and the build-provenance id\n                              \
(a git commit/describe embedded at build time) so an\n                              \
agent can identify the exact binary. Also `--version`\n  \
rigger help [<command>]     print this text, or given a command, only its entry: the\n                              \
one `rigger <command> --help` prints for each command\n                              \
whose arguments are its own flags\n\n\
run options:\n  \
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
    say_text(USAGE_TEXT);
}

/// Say `text`, lines carrying their own newlines, on stderr in one write.
fn say_text(text: &str) {
    rigger::whole_line::on_stderr(|err| {
        rigger::whole_line::write_whole(err, format_args!("{text}"))
    });
}

/// The block, or blocks, of [`USAGE_TEXT`] that document `name`: each line `  rigger <name>` with
/// the lines indented under it, up to the next entry, then each [`usage_options`] section naming
/// it. Empty when nothing documents it.
fn usage_entry(name: &str) -> String {
    let mut inside = false;
    let mut entry = String::new();
    for line in USAGE_TEXT.split_inclusive('\n') {
        if let Some(rest) = line.strip_prefix("  rigger ") {
            inside = rest.split_whitespace().next() == Some(name);
        } else if !line.starts_with("   ") {
            inside = false;
        }
        if inside {
            entry.push_str(line);
        }
    }
    entry + &usage_options(name)
}

/// The sections of [`USAGE_TEXT`] headed `<command>/<command> options:` that name `name`, each
/// from the blank line before its heading to its last option line: the options an entry that
/// says "opts below" points at.
fn usage_options(name: &str) -> String {
    USAGE_TEXT
        .split("\n\n")
        .filter(|section| {
            section
                .lines()
                .next()
                .and_then(|heading| heading.strip_suffix(" options:"))
                .is_some_and(|commands| commands.split('/').any(|command| command == name))
        })
        .map(|section| format!("\n{section}\n"))
        .collect()
}

/// Answer a subcommand asked for its help before it runs. When `sub` is a subcommand outside
/// [`HELP_PASSES_THROUGH`] and an argument after it, before any `--` separator, is exactly `-h` or
/// `--help`, print its [`usage_entry`] and give the exit code, having run nothing. A subcommand
/// with no entry is refused rather than run. `None` when the call does not ask for help.
fn answer_help(sub: &str, rest: &[String]) -> Option<i32> {
    let covered = SUBCOMMANDS.contains(&sub) && !HELP_PASSES_THROUGH.contains(&sub);
    let asks = rest
        .iter()
        .take_while(|arg| *arg != "--")
        .any(|arg| arg == "-h" || arg == "--help");
    if !(covered && asks) {
        return None;
    }
    let entry = usage_entry(sub);
    if entry.is_empty() {
        rigger::whole_line::say(format_args!(
            "rigger: the usage text has no entry for {sub:?}"
        ));
        return Some(1);
    }
    say_text(&entry);
    Some(0)
}

/// `rigger help [<command>]`: the usage text, or given a command with an entry, that entry alone.
fn help(rest: &[String]) {
    match rest.first().map(|name| usage_entry(name)) {
        Some(entry) if !entry.is_empty() => say_text(&entry),
        _ => usage(),
    }
}

/// The binary crate's registry test (spec 121, criterion 16), invoked here because this file
/// sits beside `src/prompt_surface.rs`, the module the test reads by `include_str!`.
#[cfg(test)]
mod prompt_surface_registry {
    use crate::prompt_surface_fixtures::registry_test;
    use rigger::prompt_surface::prompt_groups;

    /// THE CLASS TABLE's binary rows: each const and the class its doc comment opens with.
    const CLASS_TABLE: &[(&str, &str)] = &[
        ("GREP_GUARD_MESSAGE", "CONTRACT"),
        ("GUARD_WRITE_DENY", "FRAMING"),
        ("GUARD_WRITE_STDERR_PREFIX", "FRAMING"),
        ("GUARD_WRITE_ROOT_MISSING", "FRAMING"),
        ("GUARD_WRITE_ROOT_EMPTY", "FRAMING"),
        ("GUARD_WRITE_ROOT_RELATIVE", "FRAMING"),
        ("GUARD_WRITE_UNKNOWN_ARG", "FRAMING"),
        ("GUARD_WRITE_NO_ROOT", "FRAMING"),
        ("GUARD_WRITE_PAYLOAD_UNREADABLE", "FRAMING"),
        ("PRIME_IN_FORCE", "FRAMING"),
        ("PRIME_OPERATOR_FILES", "FRAMING"),
        ("PRIME_NO_STORE", "FRAMING"),
        ("PRIME_DECISIONS_HEADER", "FRAMING"),
        ("PRIME_NONE_YET", "FRAMING"),
    ];

    registry_test!(
        every_binary_prompt_text_is_a_group_of_its_class_table_class,
        CLASS_TABLE
    );
}
