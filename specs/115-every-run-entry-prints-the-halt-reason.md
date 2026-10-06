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
both; launched on rigger-run.

**ONE RENDERER, decided here.** No run entry renders the reason as text today: the step's
renderer is the composition above, serialized as the `halted` key, and the only text form is the
Workflow driver's `the run halted: ${step.halted}` (`workflows/rigger.js:965-967`). Criterion 1
adds `halted_line(reason: &str) -> String` in `src/cli/run.rs`, returning `halted: <reason>`, the
step's key in `print_run_state`'s `done:` label form. `run_cli` prints `halted_line` of
`rs.budget_halt`, when it is `Some`, on stdout right after `print_run_state`; `run_workflow`
prints it on stderr, never stdout (the MCP transport), from the conductor thread's `Ok(rs)` arm
before `driver.finish()`, and its `Err` arm keeps `rigger: conductor: <e>`. The blocking entries
print the conductor's returned reason alone: the hung arm is the step's alone, because the hung
set is folded from parked spawn requests (`crates/rigger-driver/src/liveness.rs:614-631`), which
the blocking drivers never park (`conductor.rs:1882-1884`), so `cmd_step`'s composition at
`src/cli/run.rs:875-880` is unchanged.

**EVERY RUN ENTRY, enumerated.** `cmd_step` (`rigger step`, `src/cli/run.rs:428`): the key,
unchanged. `run_cli` (`rigger run`, `--driver cli` by default, `src/cli/run.rs:317-325`): stdout.
`run_workflow` (`rigger run --driver workflow`, and `rigger serve` via `cmd_serve`,
`src/cli/run.rs:1879`): stderr, which under an MCP host other than the shim lands in that host's
server log, accepted. `cmd_workflow` (`src/cli/run.rs:1920`) prints nothing itself; its shim
spawns `rigger serve` with stderr inherited (`shim/shim.mjs:413`). The other caller of
`liveness::halt_reason`, `close_landed_units` of `rigger reset --runs` (`src/cli/hygiene.rs:982`),
is not a run entry and is untouched.

**CONSTRAINTS WALK.** Empty log, clean fixpoint: no line. Repeated, crash-resume and cold start:
`cmd_step` re-trips on every step, its count seeded from the recorded spawn requests
(`tests/cli.rs:7436-7439`), and the stop refolds (`spawn.rs:893-894`). On a slice no step drove,
`run_cli` and `run_workflow` count only their own process's spawns (`conductor.rs:1882-1884`), so
a relaunch prints the budget line only when its own pass trips, and an earlier `BudgetExhausted`
under a raised budget prints none. The spec-defect line, the conductor's text, reprints on every
relaunch until the spec is amended and relaunched, pinned by the step's test
(`tests/plan_critique_spec_defect_stop_periphery.rs:254`); the blocking entries print whatever
reason the returned state carries, asserted with the budget reason. On a slice a step drove, they
print the conductor's returned reason whatever it counts and name no hung spawn; accepted.
Concurrent: `run_workflow`'s conductor thread writes stderr, its server thread stdout. Every input
(`--driver`, `--base`, `RIGGER_BASE`, `--fresh`, the store flags, the spec path) changes the
reason only through the conductor's state; `--driver` picks the stream. STATE PLACEMENT: the
returned `RunState::budget_halt` and the log's fold; folding `BudgetExhausted` to print a budget
halt is NOT an implementation (`ledger.rs:181-183`).

**TEST DISPOSITIONS.** Both new tests sit in `tests/cli.rs` beside the step's budget test
(`tests/cli.rs:7374`), using the budget-one fixture `BUDGET_ONE_TWO_STAGE_WORKFLOW`
(`tests/cli.rs:4158`), private to that file. The step's three arms stay pinned as they are:
budget (`tests/cli.rs:7374`), hung (`tests/cli.rs:7549`), spec defect
(`tests/plan_critique_spec_defect_stop_periphery.rs:254`). A halt line's absence is asserted per
line: no line starting with `halted: `, the form `halted_line` returns. The clean `rigger run` of
`tests/cli.rs:6134` gains criterion 1's such assertion on its stdout. `McpSession`
(`tests/common/mcp.rs:9-96`) is the one stdio session authority; criterion 2 makes its
`start_with` build through `rigger_command(root, args, &[], root)` (`tests/common/cli.rs:122`), so
every session carries `RIGGER_NO_DASH` and an isolated `XDG_STATE_HOME`, and adds one NEW
handshake helper beside it (`initialize`, then `rigger_next` polled until it hands out a spawn,
answering that spawn's id); nothing is moved. `tests/workflow_driver_resolved_model_periphery.rs`
is re-expressed over both, its `call`, `call_tool`, `drain_stderr` and piped spawn deleted, its
assertions unchanged. The helper's first caller is criterion 2's test, which reads stderr from
`McpSession::finish`'s `Output` and also drives `TWO_STAGE_WORKFLOW` (`tests/cli.rs:4140`, budget
60) clean, answering each spawn `rigger_next` hands out, asserting the per-line absence.

**DOCUMENT EDITS.** None: no passage in `docs/`, the skills directories and the README says what
a run entry prints on a halt; the rustdoc naming `rigger step` the stamper stays true.

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
  asserted in `tests/cli.rs` on a `McpSession` with `--base HEAD` through the NEW handshake helper
  beside it in `tests/common/mcp.rs`, answered with one `rigger_result` for the `*/implementer#0`
  spawn `rigger_next` hands out, its stderr read from `McpSession::finish`, while a clean
  two-stage workflow run's stderr holds no line starting with `halted: `. This criterion OWNS
  `run_workflow`'s call, its stream, `McpSession::start_with`'s builder change, the helper and the
  periphery test's re-expression over them; `halted_line` is criterion 1's, NOT this one's.
- [ ] both feature lanes green (fmt, clippy, test on default and --no-default-features). This
  criterion OWNS only the lanes over the integrated result.
