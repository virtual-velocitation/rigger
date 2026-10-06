# 115 - Every run entry prints the halt reason

**Goal:** an operator learns why a run stopped from whichever entry drove it (issue #45). Only
`rigger step` says why: `cmd_step` prints the `halted` key (`src/cli/run.rs:931`), set from the
conductor's `RunState::budget_halt` by `spawn::step_of_pass` (`spawn.rs:997-1004`, in
`crates/rigger-domain/src/`), else from the slice's hung spawns (`src/cli/run.rs:875-880`). The
conductor's reason is the budget breaker's `budget exhausted: <n>/<budget> spawns`, else a
plan-critique stop's `amend the spec and relaunch: ...` (`RunCtx::halt_reason`,
`crates/rigger-conductor/src/conductor.rs:3886-3895`), both returned in `Ok` (`settle_run_state`,
`conductor.rs:2416-2436`). `run_cli` (`src/cli/run.rs:1371`) prints only `print_run_state`
(`src/cli/run.rs:2968-2987`), which names no reason, and `run_workflow` (`src/cli/run.rs:1723`)
drops the returned state (`src/cli/run.rs:1858`).

## Design

**UNIT ORDER AND BASE, decided here.** Criterion 1 needs nothing, criterion 2 needs criterion 1
for `halted_line` only (criterion 1 lands it with its caller `run_cli`) and criterion 3 needs
both; launched on rigger-run. Written against 65536eec, the run's base, which every citation names;
built by PR #62 (main e3a9125b), whose additions beyond this text - the dash opt-out fix in
`rigger run` and `rigger serve` with its tests and `McpSession::from_command`, the
`lines_starting_with` and `answer_every_spawn` helpers, the `rigger serve` budget test - are the
build's, owned by its units' reviews, not by this text.

**ONE RENDERER, decided here.** No run entry renders the reason as text today: the step's
renderer is the composition above, serialized as the `halted` key, and the only text form is the
Workflow driver's `the run halted: ${step.halted}` (`workflows/rigger.js:965-967`). Criterion 1
adds `halted_line(rs: &RunState) -> Option<String>` in `src/cli/run.rs`, `Some("halted:
<reason>")` when `rs.budget_halt` is `Some`, else `None`: the step's key in `print_run_state`'s
`done:` label form. Each entry is one `if let Some(line)` and only the stream differs: `run_cli`
prints it on stdout right after `print_run_state`; `run_workflow` on stderr, never stdout (the
MCP transport), from the conductor thread's `Ok(rs)` arm before `driver.finish()`, and its `Err`
arm keeps `rigger: conductor: <e>`. The halt line's stdout-clean half is pinned by the stdout lock
the server thread holds for the whole run (`server.run(stdin.lock(), stdout.lock())`,
`src/cli/run.rs:1874` at the base): a stdout write from the conductor thread blocks before
`driver.finish()`, so the budget run's second `next_spawn` never answers `None` and `fail`s at its
deadline, beside the exactly-once stderr assertion. `run_cli`'s `Err` path is unchanged:
`conductor::run(&cfg, &deps)?` (`src/cli/run.rs:1490` at the base) propagates before
`print_run_state`, printing neither the run state nor a halt line. The reason is printed verbatim
after `halted: `; a reason with a newline carries the prefix on its first line only, accepted.
The blocking entries
print the conductor's returned reason alone: the hung arm is the step's alone, because the hung
set is folded from parked spawn requests (`crates/rigger-driver/src/liveness.rs:614-631`), which
the blocking drivers never park (`conductor.rs:1882-1884`), so `cmd_step`'s composition at
`src/cli/run.rs:875-880` is unchanged.

**EVERY RUN ENTRY, enumerated** over the four production `conductor::run` call sites: `cmd_step`
(`rigger step`, `src/cli/run.rs:428`): the key, unchanged. `run_cli` (`rigger run`, `--driver cli`
by default, `src/cli/run.rs:317-325`): stdout. `run_workflow` (`rigger run --driver workflow`, and
`rigger serve` via `cmd_serve`, `src/cli/run.rs:1879`): stderr, which under an MCP host other than
the shim lands in that host's server log, accepted. `cmd_workflow` (`src/cli/run.rs:1920`) prints
nothing itself; its shim spawns `rigger serve` with stderr inherited (`shim/shim.mjs:413`).
`cmd_replay` (`rigger replay`, `src/cli/mod.rs:2468`, the call at `:2574`) is not a run entry: an
offline re-fold of a recorded trajectory over an isolated store for a metrics diff, reading its
state only for `err()` (`:2596`) and printing its own not-completed line (`:2615-2620`); untouched.
The other caller of `liveness::halt_reason`, `close_landed_units` of `rigger reset --runs`
(`src/cli/hygiene.rs:982`), is not a run entry and is untouched.

**CONSTRAINTS WALK.** Empty log, clean fixpoint: no line. A run that ends `incomplete` with no
budget or spec-defect reason (an escalated unit, a manual-review pause, a failed deferred gate,
`conductor.rs:2408-2410`) prints no `halted: ` line either, at parity with `rigger step`'s
`halted` key, which is `None` for the same stops; the unit status lines name them; accepted.
Repeated, crash-resume and cold start:
`cmd_step` re-trips on every step, its count seeded from the recorded spawn requests
(`tests/cli.rs:7436-7439`), and the stop refolds (`spawn.rs:893-894`). On a slice no step drove,
`run_cli` and `run_workflow` count only their own process's spawns (`conductor.rs:1882-1884`), so
a relaunch prints the budget line only when its own pass trips, and an earlier `BudgetExhausted`
under a raised budget prints none. The spec-defect line, the conductor's text, reprints on every
relaunch until the spec is amended and relaunched, pinned by the step's test
(`tests/plan_critique_spec_defect_stop_periphery.rs:254`); the blocking entries print whatever
reason the returned state carries, asserted with the budget reason. On a slice a step drove, they
print the conductor's returned reason whatever it counts and name no hung spawn; accepted.
The workflow entry owes the line only when its conductor returns (the `Ok(rs)` arm after
`conductor::run`, `src/cli/run.rs:1858` at the base); a spawn never answered keeps it in
`rx.recv()` and prints none, accepted under issue #59 by name.
Concurrent: `run_workflow`'s conductor thread writes stderr, its server thread stdout. Every input
(`--driver`, `--base`, `RIGGER_BASE`, `--fresh`, the store flags, the spec path) changes the
reason only through the conductor's state; `--driver` picks the stream. STATE PLACEMENT: the
returned `RunState::budget_halt` and the log's fold; folding `BudgetExhausted` to print a budget
halt is NOT an implementation (`ledger.rs:181-183`).

**TEST DISPOSITIONS.** Both new tests sit in `tests/cli.rs` beside the step's budget test
(`tests/cli.rs:7374`), using the budget-one fixture `BUDGET_ONE_TWO_STAGE_WORKFLOW`
(`tests/cli.rs:4158`), private to that file. The step's three arms stay pinned as they are: budget
(`tests/cli.rs:7374`), hung (`tests/cli.rs:7549`), spec defect
(`tests/plan_critique_spec_defect_stop_periphery.rs:254`). A halt line's absence is asserted per
line: no line starting with `halted: `, the form `halted_line` returns. The clean `rigger run` of
`tests/cli.rs:6134` gains criterion 1's such assertion on its stdout. Criterion 1's test is
`#[cfg(unix)]` like its neighbour (`tests/cli.rs:6132`) and gives the fake agent one arm matching
the worker persona's `Do the unit.` text (`tests/common/cli.rs:524-525`), echoing one line.
`McpSession`
(`tests/common/mcp.rs:9-96`) is the one MCP session authority. The auto-started-dash test
(`tests/cli.rs:15333`) stays a raw spawn, the one `rigger serve` session outside `McpSession`,
because it must run with the dash (`rigger_command` sets `RIGGER_NO_DASH`), speaks no MCP (it holds
stdin open and never writes it) and its `workflow.yml` declares no stages
(`write_gating_lint_project`, `tests/cli.rs:14728`; `stages` defaults empty,
`crates/rigger-domain/src/config.rs:936-937`), so no spawn is parked when its stdin closes and
its teardown is a clean exit; the suite's other raw child ends are
the standing idiom, issue #60, outside this spec. Criterion 2 makes `McpSession::start_with` build
through `rigger_command(root, args, &[], root)` (`tests/common/cli.rs:122`), so every session
carries `RIGGER_NO_DASH` and an isolated `XDG_STATE_HOME` (`root`, the periphery suite's existing
choice at `tests/workflow_driver_resolved_model_periphery.rs:136`; the registry directory in the
fixture root is accepted). `rigger mcp` reads neither variable (`cmd_mcp`,
`src/cli/dashboard.rs:814`, reaches neither the registry nor the dash), so every existing
`McpSession` caller is unchanged and the builder change is a consequence of criterion 2's test, not
a second mitigation. Criterion 2 adds four `McpSession` methods in `tests/common/mcp.rs`, nothing
moved: `tool_call(name, arguments)`, the one `tools/call` authority, which `peers` calls and whose
private copy in `tests/compaction_generations_periphery.rs:3530` is deleted, its callers calling
the method and its assertions unchanged; `initialize()`, once per session; `next_spawn(deadline:
Instant) -> Option<String>`, polling `rigger_next` 20 ms apart until it hands out a spawn id
(`Some`), answers `done: true` (`None`) or the deadline passes, when it calls `fail` itself; and
`fail(&mut self, why: &str) -> !`, which ends its own child by its handle (`Child::kill()`, the
no-os-kill gate's sanctioned form), drains its stderr, waits it and panics naming `why` and that
stderr. `finish(self)` alone consumes the session; the clean run shares one `Instant` across its
loop, the budget run one `Instant` taken before its first `next_spawn` and passed to both calls.
`initialize()` asserts the response carries `result` (the periphery's assert, moved there);
`tool_call` returns the whole JSON-RPC response as `call` does (the compaction callers'
`answer["error"]["message"]` assertions unchanged); `next_spawn` reads `result.structuredContent`
for `id` and `done`: an absent or empty `id` is no handout, `done` is read only then (`true` is
`None`, else it polls on, as the periphery does at `:193-196`), and a missing `structuredContent`
polls on to the deadline, where `fail` names stderr (the deleted `call_tool`'s panic ends there,
decided); `McpSession::call`'s parse panic stands in for the deleted `call`'s empty-line assert.
`finish` is the clean-run exit (stdin closed after the run reports done) and `fail` the deadline
exit, 15 s, because `run_workflow`'s scope (`src/cli/run.rs:1844-1875`) joins a conductor thread
blocked in `Driver::spawn`'s `rx.recv()` (`workflow.rs:198-202`) while a spawn is pending, so the
process never exits on stdin closing (issue #59). `fail`'s end and `finish` are the only session
ends this spec adds; a test panicking between `next_spawn`'s handout and its `rigger_result` leaves
the serve child running, which is issue #59's consequence and ends with its root fix, accepted here
by name. In the two new runs every check between a handout and its `rigger_result` goes through
`fail`, never a bare assert, so the accepted leak covers only unforeseen panics; the periphery's
own asserts stand as they are, accepted under issue #59 by name. The periphery keeps two 15 s
deadlines, one passed to `next_spawn`, one for its `events.db` poll.
`tests/workflow_driver_resolved_model_periphery.rs` is re-expressed over `McpSession` and
these methods, its `call`, `call_tool`, `drain_stderr` and piped spawn deleted, its `events.db`
poll miss calling `fail`, its assertions unchanged; it ends its session with `finish` after reading
the green event (its one spawn answered, nothing pending, so the conductor returns), and a `None`
from its `next_spawn` before a spawn is handed out calls `fail`. Their first caller is criterion
2's test, which uses `tool_call` for `rigger_result`, reads stderr from `McpSession::finish`'s
`Output` and also drives `TWO_STAGE_WORKFLOW` (`tests/cli.rs:4140`, budget 60) clean, looping
`next_spawn` and answering each id until `None`, asserting the per-line absence. The budget run,
after its one `rigger_result`, calls `next_spawn` again and requires `None` before `finish`, so the
15 s deadline covers the conductor's return and the assertion also pins that the refused second
unit's spawn is never handed out; `finish` then waits on a process whose run has reported done. A
session is for short fixtures whose stderr stays under the 64 KiB pipe buffer, as these two runs
do, and a hang inside `exchange`'s untimed `read_line` (`tests/common/mcp.rs:55`) is outside
`fail`'s reach and accepted for them; a longer run drains stderr on a thread first (not this
spec's).

**DOCUMENT EDITS.** In `tests/common/mcp.rs` only: criterion 2 rewords its module and method
rustdoc and `expect` strings (`:1-8, 21, 32, 65, 94`) from a `rigger mcp` invocation to a rigger
stdio session; `start`'s `rigger mcp` default keeps its name. No passage in `docs/`, the skills
directories and the README says what a run entry prints on a halt; the rustdoc naming
`rigger step` the stamper stays true.

**OUT OF SCOPE.** Exit status (a halt is a run outcome; `rigger step` exits 0 on one,
`tests/cli.rs:7379-7385`); the step line's other fields; the Workflow driver's stop text; the
shim's end-of-run text after a halted serve.

## Global constraints

- Hyphens, never em or en dashes, and ASCII only, in every added line.
- No new event type; no new crate dependency.
- Both feature lanes green (fmt, clippy -D warnings, test on default and --no-default-features).

## Done when

- [ ] a test proves THE BLOCKING RUN PRINTS THE HALT REASON: `rigger run` over the budget-one fixture exits 0 and prints `halted: budget exhausted: 1/1 spawns` on stdout after the run state,
  asserted in `tests/cli.rs` with the fake agent executable of `tests/cli.rs:6055` on `PATH`
  answering the worker, the clean run of `tests/cli.rs:6134` printing no line starting with
  `halted: `. This criterion OWNS `halted_line` and `run_cli`'s call; `run_workflow`'s call is
  criterion 2's, NOT this one's.
- [ ] a test proves THE WORKFLOW DRIVER PRINTS THE HALT REASON: `rigger run --driver workflow` over the budget-one fixture prints `halted: budget exhausted: 1/1 spawns` on stderr when its conductor returns,
  asserted in `tests/cli.rs` on a `McpSession` with `--base HEAD` through its NEW `initialize`
  and `next_spawn` methods in `tests/common/mcp.rs`, answered with one `rigger_result` for the
  `*/implementer#0` spawn `next_spawn` hands out, `next_spawn` then answering `None`, its stderr
  read from `McpSession::finish`, while a clean two-stage workflow run's stderr holds no line
  starting with `halted: `. This criterion OWNS `run_workflow`'s call, its stream,
  `McpSession::start_with`'s builder change, the `tool_call`, `initialize`, `next_spawn` and `fail`
  methods, the `tests/common/mcp.rs` rewording DOCUMENT EDITS gives it, and the
  re-expression of the periphery and compaction suites over them; `halted_line` is criterion 1's,
  NOT this one's.
- [ ] both feature lanes green (fmt, clippy, test on default and --no-default-features). This
  criterion OWNS only the lanes over the integrated result.
