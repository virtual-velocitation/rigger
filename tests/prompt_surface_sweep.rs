//! THE SOURCE SWEEP (spec 121, criterion 18): every production string literal of 40 or more
//! characters with a space in a prompt-bearing file is registry text or an allow-listed operator
//! message or tool output, and every such literal of a crate's `prompt_surface` module is carried
//! by a registry group.
//!
//! The swept files are every `.rs` file under `crates/` and `src/` the shared production-boundary
//! reader keeps, each read through its cut. A file is prompt-bearing when a line of that
//! production text whose first non-blank characters are not `//` spells `SpawnOpts`, `SpawnHost`
//! or `prompt_surface`, so a new file touching the spawn surface or the registry is swept with no
//! edit here. A literal (escaped, raw or byte, line continuations joined) qualifies by shape
//! alone, and it is registered when the text of a group `rigger::prompt_surface::all_groups()`
//! returns contains it. Every other qualifying literal outside a `prompt_surface` module is in
//! [`ALLOW_LIST`], one prefix entry per literal; a qualifying module literal no group carries fails
//! whatever the list holds.

use crate::common::fixtures::production_files;
use crate::common::repo::{repo_root, rs_files_under};
use crate::source_audit::{tokenize, RawKind};
use std::collections::BTreeMap;

/// Who reads an allow-listed literal, and so why it is no prompt text.
#[derive(Clone, Copy, Debug)]
enum Category {
    /// An error, a status line, a help text, a log line the operator reads, or a command line
    /// the host runs.
    OperatorMessage,
    /// A tool result or error an agent reads back as data, never as instruction.
    ToolOutput,
}

use Category::{OperatorMessage, ToolOutput};

/// One file's allow-listed literals of one category: `(file, category, prefixes)`, a prefix of
/// each literal, listed once per literal.
type Allowed = (&'static str, Category, &'static [&'static str]);

/// The qualifying literals of the prompt-bearing files that no registry group carries, keyed by
/// file: each is text no agent reads as instruction.
const ALLOW_LIST: &[Allowed] = &[
    (
        "crates/rigger-conductor/src/canary_store.rs",
        OperatorMessage,
        &[
            "canary item {:?}: expected_verdict must be \"{VERDICT_APPROVE",
            "canary: the review panel names no adjudicator - the canary m",
            "canary: panel {role} references unknown agent {agent_id:?}",
        ],
    ),
    (
        "crates/rigger-conductor/src/conductor.rs",
        OperatorMessage,
        &[
            "settled: a completed gate battery acted on {} verdict row(s)",
            "recovered a halted spawn's tree as commit {}",
            "{BUDGET_MARKER} stage {stage:?} {tier} {agent:?}: spawn budg",
            "{MISMATCH_MARKER}stage {stage:?} {tier} {agent:?} returned N",
            "{DEGENERATE_MARKER}stage {stage:?} {tier} {agent:?} returned",
            "stage {stage:?} {tier} {agent:?}: the reviewer exited with n",
            "{MISMATCH_MARKER}stage {stage:?} {tier} {agent:?}: the revie",
            "{PLAN_LANDING_MARKER}unit {unit:?}: landing its plan-stage c",
            "{INFRA_MARKER}infra: {evidence}\n(unit {unit:?} failed on inf",
            "a critique gate is detected only when a producer exists",
            "rigger: the gate request channel failed: {e}",
            "amend the spec and relaunch: plan-critique found a spec defe",
            "{} of {} keyed events of this run were not recorded on {STRE",
            "a running stage always reports before the wave ends",
            "stage {name:?} failed in its wave: {msg}",
            "{role} {agent_id:?} would run in the main repo checkout (emp",
            "{role} {agent_id:?} would run directly in the main repo chec",
            "stage {:?} references unknown {role} {agent_id:?}",
            "spawn {}: session {} to resume is gone; it ran as a fresh se",
            "For the operator: review of unit {unit:?} at attempt {attemp",
            "a review round left unit {unit:?}'s worktree as residue afte",
            "{} (guard_review_round_tree also failed to restore residue:",
            "stage {:?} references unknown agent {:?}",
            "a plan-stage commit touched paths outside specs/: {}",
            "the plan amendment conflicts with a concurrent specs/ change",
            "unit {:?} escalated after {attempts} attempts; {why}",
            "stage {:?} references unknown agent {:?}",
            "unit {:?} escalated: all {width} speculation candidates fail",
            "speculation winner {lane} implemented by {}",
            "candidate {lane} rejected by the adjudicator",
            "review stage {:?} escalated after {attempts} attempts; {why}",
            "stage {:?} references unknown {tier} {agent_id:?}",
            "plan-critique re-plan references unknown planner {:?}",
            "the adjudicator did not approve the decomposition",
            "plan-critique {gate_name:?} escalated after {attempts} attem",
            "plan-critique {gate:?} stopped the run: its {} reject at att",
            "rigger: branch-gc: removing branch {branch:?} - {}",
            "rigger: branch-gc: kept branch {branch:?} - {}",
            "deferred gate {gid:?} failed at the phase boundary: {}",
            "integrate {}: the run branch moved under every one of {pass}",
            "integrate conflict unresolved after {CONFLICT_RESOLVE_BOUND}",
            "plan-stage commit landing intent for {unit}: {} commit(s) qu",
            "plan-stage commit landing confirmed for {unit}: {}/{} intend",
            "rigger: regenerate conflicting artifacts for {unit}",
            "stage {:?} references unknown agent {:?}",
            "the code store was not advanced to the landed merge ({e}); t",
            "landing {unit:?} was refused: local content at {} would be o",
            "{LAND_REFUSED_MARKER}unit {unit:?}: landing refused for loca",
            "same-id refine resolves to a criterion its unit does not ser",
            "unmatched-proposal: proposed unit {} maps to no baseline cri",
            "conductor invariant violated: fan-out unit '{name}' has an e",
            "conductor invariant violated: fan-out unit '{name}' (an unma",
        ],
    ),
    (
        "crates/rigger-conductor/src/conductor.rs",
        ToolOutput,
        &[
            "flaky: gate {:?} failed then passed on rerun {}/{}; first fa",
            "gate: no gate {gate_id:?} is declared in the workflow; the d",
            "gate: {dir} has uncommitted changes; commit them, then run `",
            "{WORKTREE_CHANGED}, so its result addresses no commit; chang",
            "re-run: unverified recorded pass at log position {}",
            " (it gated commit {}, and the gate runs on {commit} now)",
            "gate: {} is not the worktree of a unit this run is driving;",
            "skipped: gate {gid:?} inputs {:?} do not intersect the unit'",
            "skipped: gate {gid:?} declares post_merge: false, so it judg",
            "{why_rerun}cache-hit: gate {gid:?} was proven green at log p",
        ],
    ),
    (
        "crates/rigger-dash/src/mcpserver.rs",
        OperatorMessage,
        &[
            "tool_emit_bound/tool_progress/tool_scratch are dispatched on",
            "an emit whose fold is not reported is a fold that can be sil",
        ],
    ),
    (
        "crates/rigger-dash/src/mcpserver.rs",
        ToolOutput,
        &[
            "invalid params: tools/call requires params.name",
            "rigger_emit: refusing to emit on behalf of spawn {named:?}:",
            "rigger_emit: arguments must be a JSON object",
            "rigger_progress: activity must be non-empty",
            "rigger_progress: no progress store is wired on this server",
            "rigger_scratch: no scratch root is wired on this server",
            "rigger_scratch: {bound:?} does not name a usable scratch pat",
            "rigger_ground: no grounder is wired on this server",
            "rigger_graph: no lookup is wired on this server",
            "rigger_graph: pass `lookup` <term>..., `show` <entity> or `a",
            "no context graph is wired on this server",
            "rigger_emit: refusing to emit {typ:?}: only agent context ev",
            "rigger_emit: meta value for {k:?} must be a string",
            "(+{} ranked below these left out to keep this reply under bu",
            "rigger_emit: valid_from must be unix-nanos (integer) or an R",
            "rigger_emit: invalid RFC3339 valid_from {s:?}",
            "{}\n[rigger: reply truncated - {} of {} bytes left out to sta",
        ],
    ),
    (
        "crates/rigger-domain/src/agent.rs",
        OperatorMessage,
        &[
            "{PARKED_MARKER} spawn {id:?} parked: nothing serves it in-pr",
            "refresh the cloud provider's credentials",
            "fix the agent's configured model, restart rigger run with --",
        ],
    ),
    (
        "crates/rigger-domain/src/instructions.rs",
        OperatorMessage,
        &["warning: built-in instruction {} is {by}"],
    ),
    (
        "crates/rigger-domain/src/review.rs",
        OperatorMessage,
        &[
            "the critic's output carries no verdict line",
            "the critic's verdict {verdict:?} does not approve, yet its o",
            "a result recorded on the critique stream is unreadable: {e}",
            "a spawn request recorded on the critique stream is unreadabl",
        ],
    ),
    (
        "crates/rigger-driver/src/driver/claude_code.rs",
        OperatorMessage,
        &[
            "claude_code driver: {:?}: launch carried no stdout pipe",
            "stream (silent after result, stopping the read)",
            "stream (read error after result, ignored): {e}",
            "stream (read error before system/init): {e}",
            "stream: launched as session {}, system/init named session {n",
            "stream: session {} to resume yielded no system/init, falling",
            "claude_code driver: {:?}: the agent stream ended with no res",
            "claude_code driver: {:?}: its {} MCP server reported status",
            "stream: a process outside the child's tree still holds the {",
            "waiting: {category} (attempt {attempt}, retrying in {delay_m",
            "spawn {spawn_id:?} stopped: its agent stream stayed silent f",
            "claude_code driver: {:?}: read agent stream: {e}",
            "claude_code driver: resolve the write root {}: {e}",
        ],
    ),
    (
        "crates/rigger-driver/src/driver/cli.rs",
        OperatorMessage,
        &["cli driver: agent {:?} exited unsuccessfully ({})"],
    ),
    (
        "crates/rigger-driver/src/driver/mod.rs",
        OperatorMessage,
        &[
            "driver: compose the spawn's allowed tools: {e}",
            "driver: write the spawn's system prompt under {}: {e}",
        ],
    ),
    (
        "crates/rigger-driver/src/hooks.rs",
        OperatorMessage,
        &["{bin} hook stop-failure --spawn {spawn} --class {class}"],
    ),
    (
        "src/cli/critique.rs",
        OperatorMessage,
        &[
            "critique: expected a spec path: rigger critique <spec> [--ev",
            "{COMMAND}: refusing to critique {}: it is outside the reposi",
            "{}answered from the critique recorded over {passes}",
            "no pass recorded: raise the ceiling or edit the text",
            "no open BLOCKING finding, the critique is clean at the ceili",
            "open BLOCKING findings: {}: amend the spec or record a Decis",
            "{}not exhausted after {attempts} attempts, {bound}: {close}",
            "{COMMAND}: the critic {:?} the workflow names has no definit",
            "{}critiquing with {} at attempt {attempt}",
        ],
    ),
    (
        "src/cli/graph.rs",
        OperatorMessage,
        &[
            "graph build: this build keeps no code store",
            "graph {verb}: {} over {members} node(s) at resolution {resol",
            "graph {verb}: --resolution expects a number, got {raw:?}",
            "graph {verb}: --resolution must be a positive finite number,",
            "graph {verb}: unknown argument {other:?}",
        ],
    ),
    (
        "src/cli/graph.rs",
        ToolOutput,
        &[
            "{verb}: --tests expects exclude, include or only",
            "graph: --lookup <term>..., --around <id> or --show <entity>",
            "subgraph around {around:?} (depth {depth}):",
            "  {} governing decision(s)/finding(s) (newest {shown} shown)",
            "  (+{rest} more decision(s)/finding(s) not shown - {})",
            "  (nothing found; a seed is an exact id, `<file>::<path>`, o",
            "the context graph could not be opened: {e}",
            "show {entity:?}: no such entity in the graph (has it been bu",
            "show {entity:?}: {} candidates - the name is ambiguous, re-r",
            "  (body clamped to {SHOW_MAX_BODY_LINES} lines; {omitted} mo",
            "no production definition {} of kind {kind} at line {line} of",
            "source unavailable at {file}:{start}; the recorded location",
            "source unavailable at {file}:{start}: {why}",
            "source unavailable at {file}:{start}; the recorded location",
            "source unavailable at {file}:{start}; the recorded location",
            "the body extent needs the code-extraction grammar; this buil",
            "ground: expected a query: rigger ground \"<query>\" [<k>]",
            "ground: <k> must be a non-negative integer, got {s:?}",
            "ground: expected at most a query and k, got {} arguments",
            "no entity matches strongly for {query:?}",
            "emit: --spawn expects a spawn id: rigger emit --spawn <id> <",
            "emit: expected a type: rigger emit [--spawn <id>] <type> '<j",
            "emit: expected a JSON object: rigger emit [--spawn <id>] <ty",
            "emit: expected a type and a single JSON object, got {} argum",
            "emit: <json-object> is not valid JSON: {e}",
            "emit: <json-object> must be a JSON object, got {}",
        ],
    ),
    (
        "src/cli/guard.rs",
        OperatorMessage,
        &[
            "hook: unknown subcommand {other:?}: rigger hook stop-failure",
            "hook: expected a subcommand: rigger hook stop-failure --spaw",
            "hook stop-failure: --spawn expects a value",
            "hook stop-failure: --class expects a value",
            "hook stop-failure: unknown argument {other:?}: rigger hook s",
            "hook stop-failure: --spawn must be non-empty",
            "hook stop-failure: unrecognized --class {class:?}; known cat",
            "stop-failure recorded for {spawn_id} (class {class}, positio",
        ],
    ),
    (
        "src/cli/mod.rs",
        OperatorMessage,
        &[
            "{verb}: --eventstore expects sqlite|kurrentdb, got {other:?}",
            "{verb}: unexpected second positional argument {arg:?}",
            "note: {} - until then the context graph answers as it stands",
            "note: graph.db passed over {} event(s) {by_cause}; {PASSED_O",
            "a rebuild under a later fold rule retries them",
            "warning: {} is readable by other users (mode {:o}); it holds",
            "the project config's store.backend is {other:?}; only \"sqlit",
            "the server event store is selected but no connection string",
            "rigger: instance registry refresh skipped ({e}); discovery i",
            "definition hash: read {RIGGER_DIR}/{WORKFLOW_FILE}: {e}",
            "ambiguous project identity: the event store holds history un",
            "migrated project history to the durable identity: renamed {n",
            "rigger: migrated project identity - renamed {n} stream(s) fr",
            "{command}: refusing to run from inside a linked worktree ({l",
            "no rigger store found: neither {} nor any parent directory h",
            "the working directory cannot be read: {why}",
            "store: warning: bypassing a nearer shadow store at {} in fav",
            "result: note: no spawn request is recorded for {id:?}; recor",
            "result: note: {id:?} already has a recorded result at positi",
            "spawn {id} already ended (result at position {})",
            "result: {refusal}; pass --supersede to replace it",
            "{STEP_BUSY_TOKEN} in this repo (lock {}). Refusing to run co",
            "rigger run: scratch sweep skipped (liveness unreadable): {e}",
            "{cmd}: no reachable base for the run branch {RUN_BRANCH:?}:",
            "{cmd}: the spec's criteria reference {n} path(s) - e.g. {fir",
            "{cmd}: {n} spec-referenced path(s) are absent from the base",
            "# Rigger: no runs recorded yet (run `rigger run` to start a",
            "{:.1}% of grounded units stay co-schedulable (wave-paralleli",
            "; {} of {} grounded units name no code (an empty radius runs",
            " - WARN: below the {:.1}% floor, the fleet is largely serial",
            "  first-pass yield   {:.1}% ({}/{} units clean on the first",
            "  escalation rate    {:.1}% ({}/{} units escalated to a huma",
            "  review             {} approved / {} rejected",
            "  gates              (no gate runs recorded)",
            "    {gate:<16} {} pass / {} fail / {} total",
            "  spawn timing       (no recorded spawns)",
            "  spawn timing per agent (mean / count / total):",
            "    {role:<20} {} avg / {} spawns / {} total",
            "    ({} unpaired spawn request(s) excluded above - no record",
            "no adjudicator verdict recorded on this run's driver - the u",
            "a verdict WAS recorded, but {} upheld finding(s) carry no at",
            "    (unfed upheld numerator: the folds below - survival, adv",
            "    flip-flop rate     {:.1}% ({}/{} rejects reversed on the",
            "    lens overlap       {:.1}% ({}/{} flagged files hit by 2+",
            "    adversary precision {:.1}% ({}/{} adversary-only finding",
            "    finding survival   (no review findings recorded)",
            "    finding survival per actor (upheld/raised):",
            "    (cause folded for {}/{} review rejects; the other {} car",
            "    tier cost          (no review spawns recorded)",
            "    cost per upheld finding per tier (spawns/upheld):",
            "      {tier:<12} {} spawns / {} upheld ({ratio})",
            "  resolved model by tier (never a configured alias):",
            "  items scored       {} ({} planted, {} defect class(es) cat",
            "  catch rate by tier (planted defects each tier caught):",
            "    {tier:<16} n/a ({} correctly-rejected item(s) with no me",
            "  control items      {}/{} approved ({} false positive(s): k",
            "  adjudicator        {}/{} correct ({:.1}%)",
            "  verdict stability  {}/{} stable ({:.1}%) under finding-ord",
            "  findings raised by tier (volume, informational):",
            "rigger replay: needs a git repo - the candidate config is re",
            "rigger replay: no runs recorded yet for this project (no {db",
            "rigger replay: no run {run_id:?} in this project's stream (u",
            "rigger replay: the candidate re-drive did not complete ({e})",
            "rigger replay: --against needs a git rev",
            "rigger replay: unexpected argument {extra:?}",
            "rigger replay: expected a run id (or `latest`) and `--agains",
            "rigger replay: missing --against <rev> (the candidate config",
            "rigger replay: could not check out --against {rev:?}: {}",
            "rigger replay: candidate config at {rev:?} is invalid: {e}",
            "rigger replay: candidate helpers at {rev:?}: {e}",
            "rigger replay: candidate definition hash at {rev:?}: {e}",
            "replay stats diff (baseline run {run_id} vs candidate config",
            "FAIL\ngate {}: not covered by the replayed trajectory (a cand",
            "watch: --interval expects an integer number of seconds, got",
            "watch: unknown argument {other:?} (usage: rigger watch [--in",
            "warning: residue found under the scratch root (surfaced only",
            "\n  worktree with no live unit: {name} ({})",
            "reclaimed automatically by the next `rigger run`, or remove",
            "reaped by the code store within its cap, or cleared by `rigg",
            "`rigger reset --build-cache` reclaims it now (the next `rigg",
            "`rigger reset --build-cache` reclaims it now (recording `rig",
            "no run or spawn owns this, so nothing reclaims it automatica",
            "warning: {} is {dead_pct}% dead ({} of {} reclaimable) - {hi",
            "--build-cache: reclaimed {} ({} byte(s)) from {} ({} dead en",
            "--build-cache: left {} dead entr{} of {} in place - a live p",
            "setup: {} is not a valid docs overlay: {e}",
            "# >>> BEGIN rigger docs pre-commit (managed - do not edit) >",
            "# <<< END rigger docs pre-commit (managed) <<<",
            "__BEGIN__\n# Check rigger's code-derived docs before THIS com",
            "next: `rigger validate {spec_path}` checks the spec's shape",
        ],
    ),
    (
        "src/main.rs",
        OperatorMessage,
        &[
            "rigger - a config-driven, event-sourced multi-agent dev-loop",
            "rigger: the usage text has no entry for {sub:?}",
        ],
    ),
];

/// The texts of the groups the registry holds.
fn group_texts() -> Vec<&'static str> {
    rigger::prompt_surface::all_groups()
        .iter()
        .map(|group| group.text)
        .collect()
}

/// The production text of every `.rs` file under the repository's `crates/` and `src/`.
fn production_tree() -> Vec<(String, String)> {
    let root = repo_root();
    production_files(&root, &rs_files_under(&root, &["crates", "src"]))
}

/// The real tree's qualifying literals are each registered or allow-listed, the allow-list
/// holds no entry beyond them, and every qualifying literal of each crate's `prompt_surface`
/// module is carried by a registry group.
#[test]
fn every_qualifying_literal_of_a_prompt_bearing_file_is_registered_or_allow_listed() {
    assert_eq!(
        sweep(&production_tree(), &group_texts(), ALLOW_LIST),
        Vec::<String>::new()
    );
}

/// Each registry crate's `prompt_surface` module is prompt-bearing by its own token, so the
/// module check sweeps all five.
#[test]
fn the_sweep_reads_every_registry_module() {
    let swept: Vec<String> = production_tree()
        .into_iter()
        .filter(|(file, text)| in_module(file) && prompt_bearing(text))
        .map(|(file, _)| file)
        .collect();
    assert_eq!(
        swept,
        [
            "crates/rigger-conductor/src/prompt_surface.rs",
            "crates/rigger-dash/src/prompt_surface.rs",
            "crates/rigger-domain/src/prompt_surface.rs",
            "crates/rigger-driver/src/prompt_surface.rs",
            "src/prompt_surface.rs",
        ]
    );
}

#[test]
fn a_string_literal_token_decodes_to_its_value() {
    let rows: &[(&str, &str, Option<&str>)] = &[
        ("plain", r#""plain text""#, Some("plain text")),
        ("line feed", r#""a\nb""#, Some("a\nb")),
        ("tab", r#""a\tb""#, Some("a\tb")),
        ("carriage return", r#""a\rb""#, Some("a\rb")),
        ("nul", r#""a\0b""#, Some("a\0b")),
        ("backslash", r#""a\\b""#, Some("a\\b")),
        ("double quote", r#""a\"b""#, Some("a\"b")),
        ("single quote", r#""a\'b""#, Some("a'b")),
        ("hex", r#""a\x41b""#, Some("aAb")),
        ("unicode", r#""a\u{e9}b""#, Some("a\u{e9}b")),
        ("continuation", "\"one \\\n      two\"", Some("one two")),
        (
            "crlf continuation",
            "\"one \\\r\n\r\n\ttwo\"",
            Some("one two"),
        ),
        ("a trailing backslash", "\"ab\\\"", Some("ab")),
        ("unterminated", "\"ab", None),
        ("raw", r#"r"a\nb""#, Some("a\\nb")),
        ("hashed raw", r##"r#"say "hi""#"##, Some("say \"hi\"")),
        ("byte", r#"b"a\tb""#, Some("a\tb")),
        ("byte raw", r##"br#"a\b"#"##, Some("a\\b")),
        ("char", "'a'", None),
        ("escaped char", r"'\n'", None),
        ("number", "40", None),
    ];
    for (case, token, value) in rows {
        assert_eq!(string_value(token).as_deref(), *value, "{case}");
    }
}

/// A text of exactly `chars` characters holding one space.
fn spaced(chars: usize) -> String {
    format!("a b{}", "x".repeat(chars - 3))
}

#[test]
fn a_literal_qualifies_at_forty_characters_with_a_space() {
    let rows: Vec<(&str, String, Vec<Literal>)> = vec![
        ("39 characters", format!("f(\"{}\");", spaced(39)), vec![]),
        (
            "40 characters",
            format!("f(\"{}\");", spaced(40)),
            vec![(1, spaced(40))],
        ),
        (
            "41 characters",
            format!("f(\"{}\");", spaced(41)),
            vec![(1, spaced(41))],
        ),
        (
            "40 with no space",
            format!("f(\"{}\");", "x".repeat(40)),
            vec![],
        ),
        (
            "counted decoded, an escape one character",
            format!("f(\"{}\\t\");", spaced(38)),
            vec![],
        ),
        (
            "an escape reaching 40 decoded",
            format!("f(\"{}\\t\");", spaced(39)),
            vec![(1, format!("{}\t", spaced(39)))],
        ),
        (
            "counted in characters, 39 of them in 40 bytes",
            format!("f(\"{}\\u{{e9}}\");", spaced(38)),
            vec![],
        ),
        (
            "an identifier is no literal",
            "let r = 1;".to_string(),
            vec![],
        ),
        (
            "a continuation joined",
            "f(\"the first half of it, \\\n     and the second half\");".to_string(),
            vec![(1, "the first half of it, and the second half".to_string())],
        ),
        (
            "a raw literal",
            format!("f(r#\"{}\"#);", spaced(40)),
            vec![(1, spaced(40))],
        ),
        (
            "a line comment",
            format!("// f(\"{}\");", spaced(40)),
            vec![],
        ),
        (
            "a block comment",
            format!("/* f(\"{}\"); */", spaced(40)),
            vec![],
        ),
        (
            "every literal, on its own line, in order",
            format!(
                "fn f() {{}}\nconst A: &str = \"{}\";\n\nconst B: &str = \"{}\";",
                spaced(40),
                spaced(45)
            ),
            vec![(2, spaced(40)), (4, spaced(45))],
        ),
    ];
    for (case, source, expected) in rows {
        assert_eq!(qualifying_literals(&source), expected, "{case}");
    }
}

#[test]
fn a_file_is_prompt_bearing_by_a_token_on_a_code_line() {
    let rows: &[(&str, bool)] = &[
        ("use crate::spawn::SpawnOpts;", true),
        ("fn host(h: &dyn SpawnHost) {}", true),
        ("use rigger::prompt_surface::all_groups;", true),
        ("let x = 1; // SpawnOpts", true),
        ("// SpawnOpts", false),
        ("    // SpawnHost", false),
        ("/// prompt_surface", false),
        ("//! SpawnOpts", false),
        ("/* SpawnOpts */", true),
        ("// SpawnOpts\nfn f() {}", false),
        ("// note\nuse crate::spawn::SpawnOpts;", true),
        ("fn f() {}", false),
        ("", false),
    ];
    for (text, expected) in rows {
        assert_eq!(prompt_bearing(text), *expected, "{text:?}");
    }
}

/// A registry group's text, as the sweep's synthetic registry.
const GROUP_TEXT: &str = "Answer with one verdict line: APPROVE or REJECT, then the reason.";
/// A qualifying literal [`GROUP_TEXT`] contains.
const CONTAINED: &str = "one verdict line: APPROVE or REJECT, then the reason";
/// A qualifying literal no group carries.
const OPERATOR_LINE: &str = "an operator reads this line and no agent ever does";
/// An allow-list prefix of [`OPERATOR_LINE`].
const PREFIX: &str = "an operator reads this line";
/// A shorter prefix of [`OPERATOR_LINE`], distinct from [`PREFIX`].
const SHORTER: &str = "an operator reads";
/// A slice of [`OPERATOR_LINE`] that is not its prefix.
const INSIDE: &str = "reads this line";
/// A swept file outside every `prompt_surface` module.
const OUTSIDE: &str = "src/cli/a.rs";
/// A second swept file outside every module.
const OTHER: &str = "src/cli/b.rs";
/// A crate's `prompt_surface` module.
const MODULE: &str = "crates/x/src/prompt_surface.rs";
/// A swept file whose name only ends like a module's.
const NAMED_LIKE: &str = "src/cli/my_prompt_surface.rs";

/// A prompt-bearing source holding `literals` one per line from line 2.
fn bearing(literals: &[&str]) -> String {
    literals.iter().fold(
        "use crate::spawn::SpawnOpts;\n".to_string(),
        |source, literal| format!("{source}const _: &str = \"{literal}\";\n"),
    )
}

#[test]
fn the_sweep_reports_each_unregistered_literal_and_each_miscounted_entry() {
    let unlisted = |file: &str, line: usize| {
        format!("{file}:{line}: {OPERATOR_LINE:?} is in no registry group and no allow-list entry")
    };
    let entry = |file: &str, category: &str, prefix: &str, listed: usize, prefixed: usize| {
        format!(
            "{file}: {category} entry {prefix:?} is listed {listed} time(s) for {prefixed} literal(s)"
        )
    };
    type Row = (
        &'static str,
        Vec<(&'static str, String)>,
        &'static [Allowed],
        Vec<String>,
    );
    let rows: Vec<Row> = vec![
        (
            "a group's exact text is registered",
            vec![(OUTSIDE, bearing(&[GROUP_TEXT]))],
            &[],
            vec![],
        ),
        (
            "a literal a group's text contains is registered",
            vec![(OUTSIDE, bearing(&[CONTAINED]))],
            &[],
            vec![],
        ),
        (
            "an unregistered literal no entry prefixes",
            vec![(OUTSIDE, bearing(&[OPERATOR_LINE]))],
            &[],
            vec![unlisted(OUTSIDE, 2)],
        ),
        (
            "an operator-message entry allows its literal",
            vec![(OUTSIDE, bearing(&[OPERATOR_LINE]))],
            &[(OUTSIDE, OperatorMessage, &[PREFIX])],
            vec![],
        ),
        (
            "a tool-output entry allows its literal",
            vec![(OUTSIDE, bearing(&[OPERATOR_LINE]))],
            &[(OUTSIDE, ToolOutput, &[PREFIX])],
            vec![],
        ),
        (
            "a file spelling no token is not swept",
            vec![(
                OUTSIDE,
                format!("fn f() {{}}\nconst _: &str = \"{OPERATOR_LINE}\";\n"),
            )],
            &[],
            vec![],
        ),
        (
            "a module literal a group carries",
            vec![(MODULE, bearing(&[GROUP_TEXT, CONTAINED]))],
            &[],
            vec![],
        ),
        (
            "a module literal no group carries",
            vec![(MODULE, bearing(&[OPERATOR_LINE]))],
            &[],
            vec![format!(
                "{MODULE}:2: {OPERATOR_LINE:?} is in no registry group"
            )],
        ),
        (
            "an entry never allows a module literal",
            vec![(MODULE, bearing(&[OPERATOR_LINE]))],
            &[(MODULE, OperatorMessage, &[PREFIX])],
            vec![
                entry(MODULE, "OperatorMessage", PREFIX, 1, 0),
                format!("{MODULE}:2: {OPERATOR_LINE:?} is in no registry group"),
            ],
        ),
        (
            "a second copy of an allowed literal",
            vec![(OUTSIDE, bearing(&[OPERATOR_LINE, OPERATOR_LINE]))],
            &[(OUTSIDE, OperatorMessage, &[PREFIX])],
            vec![entry(OUTSIDE, "OperatorMessage", PREFIX, 1, 2)],
        ),
        (
            "one entry per copy",
            vec![(OUTSIDE, bearing(&[OPERATOR_LINE, OPERATOR_LINE]))],
            &[(OUTSIDE, OperatorMessage, &[PREFIX, PREFIX])],
            vec![],
        ),
        (
            "one entry per copy across categories",
            vec![(OUTSIDE, bearing(&[OPERATOR_LINE, OPERATOR_LINE]))],
            &[
                (OUTSIDE, OperatorMessage, &[PREFIX]),
                (OUTSIDE, ToolOutput, &[PREFIX]),
            ],
            vec![],
        ),
        (
            "an entry listed twice for one literal",
            vec![(OUTSIDE, bearing(&[OPERATOR_LINE]))],
            &[(OUTSIDE, OperatorMessage, &[PREFIX, PREFIX])],
            vec![entry(OUTSIDE, "OperatorMessage", PREFIX, 2, 1)],
        ),
        (
            "a file only named like a module is outside it",
            vec![(NAMED_LIKE, bearing(&[OPERATOR_LINE]))],
            &[],
            vec![unlisted(NAMED_LIKE, 2)],
        ),
        (
            "a literal two distinct entries prefix",
            vec![(OUTSIDE, bearing(&[OPERATOR_LINE]))],
            &[(OUTSIDE, ToolOutput, &[PREFIX, SHORTER])],
            vec![
                entry(OUTSIDE, "ToolOutput", PREFIX, 1, 0),
                entry(OUTSIDE, "ToolOutput", SHORTER, 1, 0),
                format!("{OUTSIDE}:2: {OPERATOR_LINE:?} is prefixed by 2 allow-list entries"),
            ],
        ),
        (
            "an entry inside a literal but not its prefix",
            vec![(OUTSIDE, bearing(&[OPERATOR_LINE]))],
            &[(OUTSIDE, OperatorMessage, &[INSIDE])],
            vec![
                entry(OUTSIDE, "OperatorMessage", INSIDE, 1, 0),
                unlisted(OUTSIDE, 2),
            ],
        ),
        (
            "an entry prefixing no literal",
            vec![(OUTSIDE, bearing(&[]))],
            &[(OUTSIDE, OperatorMessage, &[PREFIX])],
            vec![entry(OUTSIDE, "OperatorMessage", PREFIX, 1, 0)],
        ),
        (
            "an entry for a file the sweep does not read",
            vec![],
            &[(OUTSIDE, OperatorMessage, &[PREFIX])],
            vec![entry(OUTSIDE, "OperatorMessage", PREFIX, 1, 0)],
        ),
        (
            "an entry whose literal is registered",
            vec![(OUTSIDE, bearing(&[GROUP_TEXT]))],
            &[(OUTSIDE, OperatorMessage, &["Answer with"])],
            vec![entry(OUTSIDE, "OperatorMessage", "Answer with", 1, 0)],
        ),
        (
            "another file's entry allows nothing here",
            vec![(OUTSIDE, bearing(&[OPERATOR_LINE])), (OTHER, bearing(&[]))],
            &[(OTHER, OperatorMessage, &[PREFIX])],
            vec![
                unlisted(OUTSIDE, 2),
                entry(OTHER, "OperatorMessage", PREFIX, 1, 0),
            ],
        ),
    ];
    for (case, sources, allow_list, expected) in rows {
        let sources: Vec<(String, String)> = sources
            .into_iter()
            .map(|(file, text)| (file.to_string(), text))
            .collect();
        assert_eq!(
            sweep(&sources, &[GROUP_TEXT], allow_list),
            expected,
            "{case}"
        );
    }
}

// --- the sweep ---

/// The tokens a prompt-bearing file's production text spells on a code line.
const TOKENS: [&str; 3] = ["SpawnOpts", "SpawnHost", "prompt_surface"];

/// The fewest characters a qualifying literal holds.
const MIN_CHARS: usize = 40;

/// A qualifying literal: the 1-based line it opens on and its decoded value.
type Literal = (usize, String);

/// Every violation of the sweep over the production `sources` against the registry's group
/// `texts` and `allow_list`, sorted: each qualifying literal of a `prompt_surface` module no
/// group text contains, each other qualifying literal no group text contains that no allow-list
/// entry of its file prefixes or that two distinct entry texts prefix, and each entry text listed
/// a different number of times than the literals it alone prefixes.
fn sweep(sources: &[(String, String)], texts: &[&str], allow_list: &[Allowed]) -> Vec<String> {
    let mut violations = Vec::new();
    let mut unregistered: BTreeMap<&str, Vec<Literal>> = BTreeMap::new();
    for (file, text) in sources.iter().filter(|(_, text)| prompt_bearing(text)) {
        for (line, value) in qualifying_literals(text) {
            if texts.iter().any(|group| group.contains(value.as_str())) {
                continue;
            }
            if in_module(file) {
                violations.push(format!("{file}:{line}: {value:?} is in no registry group"));
            } else {
                unregistered
                    .entry(file.as_str())
                    .or_default()
                    .push((line, value));
            }
        }
    }
    let mut files: Vec<&str> = unregistered
        .keys()
        .copied()
        .chain(allow_list.iter().map(|(file, ..)| *file))
        .collect();
    files.sort_unstable();
    files.dedup();
    for file in files {
        let entries: Vec<(Category, &str)> = allow_list
            .iter()
            .filter(|(listed, ..)| *listed == file)
            .flat_map(|(_, category, prefixes)| {
                prefixes.iter().map(move |prefix| (*category, *prefix))
            })
            .collect();
        let literals = unregistered.get(file).map_or(&[][..], Vec::as_slice);
        violations.extend(account(file, literals, &entries));
    }
    violations.sort();
    violations
}

/// One file's allow-list accounting: each of its unregistered `literals` that no entry prefixes
/// or that two distinct entry texts prefix, and each distinct entry text listed a different
/// number of times than the literals it alone prefixes.
fn account(file: &str, literals: &[Literal], entries: &[(Category, &str)]) -> Vec<String> {
    let mut violations = Vec::new();
    let mut prefixed: BTreeMap<&str, usize> = BTreeMap::new();
    for (line, value) in literals {
        let mut matches: Vec<&str> = entries
            .iter()
            .map(|(_, prefix)| *prefix)
            .filter(|prefix| value.starts_with(prefix))
            .collect();
        matches.sort_unstable();
        matches.dedup();
        match matches.as_slice() {
            [prefix] => *prefixed.entry(*prefix).or_default() += 1,
            [] => violations.push(format!(
                "{file}:{line}: {value:?} is in no registry group and no allow-list entry"
            )),
            several => violations.push(format!(
                "{file}:{line}: {value:?} is prefixed by {} allow-list entries",
                several.len()
            )),
        }
    }
    let mut seen: Vec<&str> = Vec::new();
    for (category, prefix) in entries {
        if seen.contains(prefix) {
            continue;
        }
        seen.push(*prefix);
        let listed = entries.iter().filter(|(_, other)| other == prefix).count();
        let count = prefixed.get(prefix).copied().unwrap_or(0);
        if listed != count {
            violations.push(format!(
                "{file}: {category:?} entry {prefix:?} is listed {listed} time(s) for {count} literal(s)"
            ));
        }
    }
    violations
}

/// Whether `file` is a crate's `prompt_surface` module.
fn in_module(file: &str) -> bool {
    file.rsplit('/').next() == Some("prompt_surface.rs")
}

/// Whether the production `text` spells one of [`TOKENS`] on a line whose first non-blank
/// characters are not `//`.
fn prompt_bearing(text: &str) -> bool {
    text.lines().any(|line| {
        !line.trim_start().starts_with("//") && TOKENS.iter().any(|token| line.contains(token))
    })
}

/// Each string literal of `text` of at least [`MIN_CHARS`] characters holding a space, as the
/// 1-based line it opens on and its decoded value, in source order. Comments hold none.
fn qualifying_literals(text: &str) -> Vec<Literal> {
    let chars: Vec<char> = text.chars().collect();
    tokenize(&chars)
        .into_iter()
        .filter(|token| token.kind == RawKind::Lit)
        .filter_map(|token| Some((token.line, string_value(&token.text)?)))
        .filter(|(_, value)| value.chars().count() >= MIN_CHARS && value.contains(' '))
        .collect()
}

/// The value of the string-literal `token` (escaped, raw, or either as bytes), or `None` for a
/// char or number literal: a raw literal's body as written, an escaped one's through
/// [`unescape`].
fn string_value(token: &str) -> Option<String> {
    let token = token.strip_prefix('b').unwrap_or(token);
    if let Some(raw) = token.strip_prefix('r') {
        let hashes = raw.len() - raw.trim_start_matches('#').len();
        return Some(raw[hashes + 1..raw.len() - 1 - hashes].to_string());
    }
    Some(unescape(token.strip_prefix('"')?.strip_suffix('"')?))
}

/// The value of an escaped literal's `body`: each escape decoded as Rust decodes it, and a line
/// continuation (a backslash ending the line) dropping the line break and the whitespace after it.
fn unescape(body: &str) -> String {
    let mut value = String::new();
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            value.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => value.push('\n'),
            Some('t') => value.push('\t'),
            Some('r') => value.push('\r'),
            Some('0') => value.push('\0'),
            Some('x') => {
                let hex: String = chars.by_ref().take(2).collect();
                value.push(char::from(
                    u8::from_str_radix(&hex, 16).expect("a \\x escape"),
                ));
            }
            Some('u') => {
                let hex: String = chars.by_ref().skip(1).take_while(|c| *c != '}').collect();
                let code = u32::from_str_radix(&hex, 16).expect("a \\u escape");
                value.push(char::from_u32(code).expect("a scalar value"));
            }
            Some('\n' | '\r') => {
                while chars
                    .next_if(|c| matches!(c, ' ' | '\t' | '\n' | '\r'))
                    .is_some()
                {}
            }
            Some(other) => value.push(other),
            None => {}
        }
    }
    value
}
