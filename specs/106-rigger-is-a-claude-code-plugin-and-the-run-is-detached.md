# 106 - Rigger is a Claude Code plugin and the run is detached

**Goal:** `rigger setup` (`src/main.rs:12727`) writes loose project files - ten skills, two
hook entries in `.claude/settings.json`, an `.mcp.json` entry, a workflow script
(`main.rs:12146`) and a Node shim (`main.rs:12639`) - and the run lives inside the operator's
chat session: the script relays `rigger step` through an agent under a 10-minute tool cap, a
chat message can halt it, and the session learns of run events only by polling
(`crates/rigger-domain/src/watch.rs:150`, 180 s). Reviewer verdicts are parsed from prose
(`crates/rigger-conductor/src/conductor.rs:10513`). This spec ships the operator seam of
docs/architecture-addendum-claude-code-integration.md (section 6) on the host of spec 104 and
the hold of spec 105, and retires the script and the shim.

## Design

THE PLUGIN, decided: setup materializes, from assets embedded in the binary, a marketplace
directory `.rigger/plugin/` holding one plugin `rigger`: `.claude-plugin/plugin.json` (version
= the binary's), `skills/`, `hooks/hooks.json` (`SessionStart` runs `rigger prime`,
`PreToolUse` on `Grep|Bash` runs `rigger grep-guard`), `.mcp.json` (`rigger mcp`),
`monitors/monitors.json` and `bin/rigger` (a launcher for the installed binary). Setup then
runs `claude plugin marketplace add .rigger/plugin` and
`claude plugin install rigger@rigger-local --scope project`; when `claude` is absent or either
command fails, setup finishes its other steps, reports the plugin as not installed and prints
both commands. Setup removes the loose equivalents an earlier setup wrote - matched by the
exact skill names and command strings it owns, nothing else. The status line stays a settings
entry (`main.rs:12946`). Re-running setup after a binary refresh re-materializes the
directory and runs `claude plugin marketplace update rigger-local`.

THE ATTENTION FEED, decided: `rigger attention --follow` prints, at start, one line per
condition true now (held, escalated, halted), then one line per crossing as the log grows,
polling at 1 s: `held`, `released`, `escalated`, `halted`, `worker-death-recurred`,
`budget-final-tenth`, `stalled-frontier`, `completed`, and `supervisor-absent` (no
`SupervisorBeat` for 90 s on a run neither done nor held). A line names the kind, the subject
and the response skill. The command keeps no state; with no run it prints nothing and stays
alive. The plugin's monitor entry runs it with `when: "always"`.

THE OPERATOR TOOLS, decided: `rigger mcp` serves `rigger_status`, `rigger_hold`,
`rigger_release`, `rigger_resume_unit` and `rigger_run` beside the lookup tools
(`src/mcpserver.rs:567`). Each write tool calls the function its CLI verb calls, so tool and
verb append identical events.

THE DETACHED SUPERVISOR, decided: `rigger run --detach <spec>` re-executes the supervisor as
its own session leader, stdio to `<scratch root>/run-logs/<run>.log`, under the project's
existing single-run lock; a second detach is refused, naming the running run. It adopts a
recorded run exactly as `rigger run` does. `rigger run --attach` follows the log and the feed
until the run is done. `rigger run --stop` appends an operator `RunHeld` with cause `stopped`;
the supervisor ends live sessions through the host's clean stop and exits; a later detach
releases that hold and continues. Steps run in-process with no duration bound. Run logs are
reclaimed by `rigger reset --runs`.

VERDICTS ARE DATA, decided: lenses, the adversary, the adjudicator and the plan critic launch
with `--json-schema` for `{verdict: "approve" | "reject", cause, findings: [{id, summary,
about}]}`. The host stores `structured_output` as `SpawnResult.meta.verdict`; the conductor
reads that field. `verdict_approves` remains only to replay a result recorded without it.

THE BOLT-ON RETIRES, decided: setup no longer installs the workflow script or the shim;
`workflows/rigger.js`, `shim/`, `rigger workflow`, the bridge tools `rigger_next` and
`rigger_result`, and `crates/rigger-driver/src/driver/workflow.rs` are removed. `rigger step` stays as the
hand-step. The shipped skills name `rigger run --detach`, `--attach` and `--stop` as the way
to start, resume and stop a run.

CONSTRAINTS WALK: two sessions on one project - two feeds, both read-only. Supervisor death -
the `supervisor-absent` line; a detach adopts. Crash during setup - every step is idempotent
and re-run. A project carrying the old loose files - removed by ownership match; a
hand-edited entry is left and reported. Headless agent sessions start no monitor (monitors
are interactive-only), so an agent never receives the feed. A result recorded before this
spec - the prose parser.

## Global constraints

- Hyphens, never em dashes. Both feature lanes and the core lane green (fmt, clippy -D
  warnings, test); no-os-kill and reap audits green.
- No new event type; no new crate dependency. No API key.
- The operator's installed rigger binary is never replaced or modified by any unit.

## Done when

- [ ] a test proves THE PLUGIN IS ONE UNIT: setup materializes the marketplace directory with
  the manifest, skills, hooks, MCP entry, monitor entry and launcher, runs the two install
  commands, reports and prints them when they cannot run, and removes only the loose entries
  an earlier setup owned. This criterion OWNS the layout, the install and the migration; the
  feed's behavior is criterion 2's, NOT this one's.
- [ ] a test proves THE ATTENTION FEED: `rigger attention --follow` prints the conditions
  true at start, then one line per crossing - including `held`, `released`, `completed` and
  `supervisor-absent` - naming kind, subject and response skill, and prints nothing for an
  empty store. This criterion OWNS the command and the monitor entry's command line.
- [ ] a test proves THE OPERATOR TOOLS: `rigger mcp` serves the five operator tools, and each
  write tool appends the same events as its CLI verb. This criterion OWNS the operator
  surface only.
- [ ] a test proves THE RUN IS DETACHED: `rigger run --detach` starts a supervisor that
  outlives its parent, logs to the named path and refuses a second detach; `--attach` follows
  it; `--stop` appends the operator hold, ends live sessions cleanly and exits; a later
  detach continues the run. This criterion OWNS the three flags and the log's lifecycle.
- [ ] a test proves VERDICTS ARE DATA: the four verdict personas launch with the schema, the
  conductor decides from `meta.verdict`, and a result without it replays through the prose
  parser. This criterion OWNS the schema, its launch flag and the conductor's read.
- [ ] a test proves THE BOLT-ON RETIRES: setup installs neither the workflow script nor the
  shim, `rigger workflow` and the bridge tools are gone, and the shipped skills name the
  detached run as the way to start, resume and stop. This criterion OWNS the removals and
  the skill text only.
- [ ] both feature lanes and the core lane green (fmt, clippy, test).
