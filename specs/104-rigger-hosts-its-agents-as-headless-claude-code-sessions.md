# 104 - Rigger hosts its agents as headless Claude Code sessions

**Goal:** the CLI driver launches `claude -p`, blocks on `output()` (`src/driver/cli.rs:71`),
learns nothing until exit, maps every non-zero exit to one generic error (`cli.rs:88`), leaves
`resolved_model` empty (`src/conductor.rs:732`) and bridges emits by parsing stdout afterwards
(`cli.rs:121`). Elsewhere an agent's heartbeat, model and result are self-reported by
shell-out, its session is unknown, and an API failure has no class. Claude Code's headless
surface already carries all of it: a probe with an invalid credential emitted
`{"type":"system","subtype":"api_retry","error_status":401,"error":"authentication_failed"}`
within one second. This spec ships the agent host of
docs/architecture-addendum-claude-code-integration.md (section 4) behind the `AgentDriver`
port (`src/conductor.rs:1479`) as the driver `rigger run` uses (`src/main.rs:3667`). The hold
that consumes its failure classes is spec 105's.

## Design

THE LAUNCH, decided: one child per launch, started in the spawn's `dir`, with
`-p --output-format stream-json --input-format stream-json --verbose`, `--session-id <uuid>`
minted by the host, `--system-prompt` (persona), `--model`, `--fallback-model` when configured,
`--tools`/`--allowed-tools` from the spawn, `--permission-mode` with
`--permission-prompts none`, `--mcp-config` with `--strict-mcp-config` naming one server
(`rigger mcp --spawn <id>`), and `--settings` carrying the per-spawn hooks. MCP config and
settings are passed as JSON strings, never files. The environment is the operator's plus
`opts.env`; the host sets and reads no credential variable. `build_args` (`cli.rs:164`) stays
the one argv authority. Before the child starts the host appends a progress-store record
`SpawnLaunched {run, spawn, launch, session_id, resumed_from, started}`; at exit it closes the
record with `ended: completed | interrupted | fault | stopped` and the class. The task is the
first user message on the input stream; the host closes the input after the first `result`.

THE STREAM, decided: one reader per child, line by line. `system/init` yields the resolved
model and the MCP server status (a `rigger` server that did not connect fails the launch as a
fault); every line touches the spawn's liveness marker (`liveness::marker_path`); a
`system/api_retry` becomes a waiting progress line carrying category, attempt and delay; a
`system/permission_denied` is counted; a line that is not JSON is recorded as a progress line
and skipped. The raw stream is written to `<scratch root>/agent-stream/<run>/<spawn>.<launch>.jsonl`,
part of the run's scratch and reclaimed with it. The `result` message becomes the
`SpawnResult`: `output`, and `meta` with `resolved_model`, `session_id`, `usage` (`input`,
`output`, `cache_creation`, `cache_read`), `turns`, `cost_usd`, `permission_denials`. A
host-launched prompt carries no instruction to report a result, a model or a heartbeat.

THE SPAWN MCP SERVER, decided: `rigger mcp --spawn <id>` extends the stdio server
(`src/mcpserver.rs:567`) with a spawn-bound surface: `rigger_emit`, `rigger_peers`,
`rigger_ground`, `rigger_graph`, `rigger_progress`, `rigger_scratch`. Every write is stamped
with the bound spawn; a write naming another spawn is refused. There is no result tool.

THE WRITE GUARD, decided: `rigger guard-write --root <dir>...` is a `PreToolUse` command hook
for `Edit|Write|NotebookEdit`. It reads the hook input, resolves the target (relative paths
against the hook's `cwd`, `..`, symlinks), allows a target under a root and otherwise prints
`hookSpecificOutput.permissionDecision: "deny"` with a reason naming the first root. It reads
no store. The host passes the spawn's `dir` and scratch container as roots.

THE HOOKS, decided here so no unit has to: the per-spawn settings JSON carries exactly two
hook families and the host assembles it from their two owners. The `PreToolUse` write guard
for `Edit|Write|NotebookEdit` is criterion 4's, command and injection both. The `StopFailure`
entries, one per error category, each invoking `rigger hook stop-failure --spawn <id>
--class <category>`, are criterion 5's, command, record and injection both. Criterion 1 passes
the assembled `--settings` string on the argv and owns none of the hooks' content.

FAILURE CLASS, decided: `AgentFailure` is Claude Code's error category: `rate_limit`,
`overloaded`, `server_error`, `authentication_failed`, `oauth_org_not_allowed`,
`cloud_credential_error`, `billing_error`, `account_on_hold`, `model_not_found`,
`invalid_request`, `max_output_tokens`, `unknown`. A session that ends without a `result`
takes its class from, in order: the record written by the `StopFailure` hook (one hook entry
per category, `rigger hook stop-failure --spawn <id> --class <category>`), the last
`api_retry.error`, else `unknown`. The port returns the class as data. Every class is
API-side: the failure charges no remediation attempt, the spawn is relaunched at most twice,
then the run halts naming the class and the spawn.

STOP, decided: the host enforces `max_wall_clock` against stream silence. Expiry closes the
session's input, waits 30 s, then ends the child through the sanctioned lifecycle helper
(`src/reap.rs`) on the child's own handle; the launch ends `stopped` and the existing
liveness-fault result is recorded. On start the supervisor closes any `SpawnLaunched` left
open as `interrupted` and reaps processes still rooted in that spawn's worktree before it
relaunches.

CONSTRAINTS WALK: the child exits before `system/init` (binary missing, unknown flag) - a
fault of class `unknown` carrying the stderr tail, never a hang. Relaunch - a new session id
and the next launch ordinal; the log holds one result. Concurrent spawns - one child, one
reader, one MCP server process each; no shared file. Supervisor crash - the open launch
record is the evidence; reconciliation as in STOP. Cold start - the log and the progress
store are the only state. An older Claude Code - `rigger validate` probes `claude --help` for
every flag the host passes and names a missing one.

## Notes (non-criteria)

Tests run against a fake `claude` executable selected through the driver's `bin` field; it
records argv, cwd and environment and replays a checked-in stream. Two streams are recorded
from the real CLI: a successful one-turn session and an invalid-credential session. The SDET
lens records one real end-to-end launch as a DecisionMade the adjudicator reads as evidence.
The workflow script, the shim and `rigger step` are untouched; spec 106 retires them.

## Global constraints

- Hyphens, never em dashes. Both feature lanes and the core lane green (fmt, clippy -D
  warnings, test); no-os-kill and reap audits green.
- No API key: the host neither sets nor reads a credential variable.
- No new run-stream event type (`SpawnLaunched` is a progress-store record); no new crate
  dependency.
- The operator's installed rigger binary is never replaced or modified by any unit.

## Done when

- [ ] a test proves THE LAUNCH IS TYPED: the host starts the child in the spawn's dir with a
  minted session id, stream-json both ways, the persona, model, tools, unattended permission
  flags, a strict MCP config naming `rigger mcp --spawn <id>` and the per-spawn settings,
  records `SpawnLaunched` before the child starts, and sets no credential variable. This
  criterion OWNS argv, cwd, environment and the launch record; the reader is criterion 2's,
  NOT this one's.
- [ ] a test proves THE STREAM IS THE RECORD: from a recorded stream the host takes the
  resolved model from `system/init`, touches the liveness marker on every line, turns
  `api_retry` lines into waiting progress lines, persists the raw stream at the named path,
  and records the `result` message as the `SpawnResult` with usage, turns, cost, session id
  and denial count, under a prompt that carries no self-report instruction. This criterion
  OWNS the reader and the result mapping; failure classes are criterion 5's, NOT this one's.
- [ ] a test proves THE SPAWN MCP SERVER: `rigger mcp --spawn <id>` serves the six tools over
  stdio, stamps every write with its spawn and refuses a write naming another spawn. This
  criterion OWNS the spawn-bound surface only.
- [ ] a test proves THE WRITE GUARD: `rigger guard-write` allows a target under a root and
  denies every other - absolute, relative, `..`, symlink-escaping - with the deny decision
  naming the root, and the host injects it as the `PreToolUse` hook for
  `Edit|Write|NotebookEdit`. This criterion OWNS the guard command and its injection.
- [ ] a test proves A FAILURE HAS A CLASS: a session ending without a result yields the class
  from the `StopFailure` record, else the last `api_retry` category, else `unknown`; the
  failure charges no remediation attempt, relaunches the spawn at most twice, then halts the
  run naming the class. This criterion OWNS the class, its sources and the bound; the hold is
  spec 105's, NOT this one's.
- [ ] a test proves STOP IS CLEAN: a wall-clock expiry closes the session's input, waits the
  grace, then ends the child through the sanctioned lifecycle helper, and a supervisor start
  closes an open launch record and reaps the spawn's worktree before relaunching. This
  criterion OWNS stop and start-up reconciliation only.
- [ ] both feature lanes and the core lane green (fmt, clippy, test).
