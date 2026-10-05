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
(it calls `halt_of_pass` and `halted_line`, which criterion 1 lands with their callers `cmd_step`
and `run_cli`) and criterion 3 needs both; launched on rigger-run.

**ONE PRECEDENCE, MOVED, decided here.** There is no Rust text renderer of the reason today: the
step's renderer is the composition above, serialized as the `halted` key, and the only text form
is the Workflow driver's `the run halted: ${step.halted}` (`workflows/rigger.js:965-967`).
Criterion 1 moves `src/cli/run.rs:875-880` into one private function of `src/cli/run.rs`,
`halt_of_pass(conductor_halt: Option<String>, events: &[Event]) -> Result<Option<String>,
String>`: the conductor's reason when present, else `liveness::halt_reason` over
`liveness::hung_spawns(events)` when that set is non-empty, else `None`. It computes the hung set
itself, so no caller can keep the call and drop the hung arm. `cmd_step` sets `step.halted` from
it and keeps its own `hung_spawns` read for the attention merge and the cursor
(`src/cli/run.rs:911-946`), unchanged. `liveness` compiles in both lanes
(`crates/rigger-driver/src/lib.rs:11-12`, re-exported at `src/lib.rs:74`).

**ONE LINE, decided here.** `halted_line(reason: &str) -> String` in `src/cli/run.rs` returns
`halted: <reason>`, the step's key in `print_run_state`'s `done:` label form. `run_cli` prints it
on stdout right after `print_run_state`; `run_workflow` prints it on stderr, never stdout (the
MCP transport), from the conductor thread's `Ok(rs)` arm before `driver.finish()`, and its `Err`
arm keeps `rigger: conductor: <e>`. No entry prints it when `halt_of_pass` answers `None`.

**EVERY RUN ENTRY, enumerated.** `cmd_step` (`rigger step`, `src/cli/run.rs:428`): the key,
unchanged. `run_cli` (`rigger run`, `--driver cli` by default, `src/cli/run.rs:317-325`): stdout.
`run_workflow` (`rigger run --driver workflow`, and `rigger serve` via `cmd_serve`,
`src/cli/run.rs:1879`): stderr. `cmd_workflow` (`src/cli/run.rs:1920`) prints nothing itself;
its shim spawns `rigger serve` with stderr inherited (`shim/shim.mjs:413`). The other caller of
`liveness::halt_reason`, `close_landed_units` of `rigger reset --runs` (`src/cli/hygiene.rs:982`),
is not a run entry and is untouched.

**THE SLICE, decided here.** Each entry hands `halt_of_pass` the current run slice it reads with
`runscope::read::read_current_run` after `conductor::run` returns. `cmd_step`'s read
(`src/cli/run.rs:835`) still fails the step; in `run_cli` and `run_workflow` an unreadable slice
reads as empty, as `run_cli`'s post-run reads already degrade (`src/cli/run.rs:1496-1520`), so
the conductor's reason still prints. `run_cli` reads that slice once after `print_run_state` and
hands it to `halt_of_pass` and to its existing metrics re-projection (`src/cli/run.rs:1513-1520`).

**CONSTRAINTS WALK.** Empty log, clean fixpoint: no line. Repeated: level triggered, so each
invocation of a halted run prints it again (the breaker re-trips, `tests/cli.rs:7436-7439`; the
stop refolds, `spawn.rs:893-894`). Crash-resume and cold start: a fresh process prints a budget
halt only when its own breaker trips, so an earlier `BudgetExhausted` under a raised budget prints
none. Concurrent: `run_workflow`'s conductor thread writes stderr, its server thread stdout.
Every input (`--driver`, `--base`, `RIGGER_BASE`, `--fresh`, the store flags, the spec path)
changes the reason only through the slice and the conductor's state; `--driver` picks the stream.
STATE PLACEMENT: the returned `RunState::budget_halt` and the log's fold; folding
`BudgetExhausted` to print a budget halt is NOT an implementation (`ledger.rs:181-183`).

**TEST DISPOSITIONS.** Both new tests sit in `tests/cli.rs` beside the step's budget test
(`tests/cli.rs:7374`), using the budget-one fixture `BUDGET_ONE_TWO_STAGE_WORKFLOW`
(`tests/cli.rs:4158`), private to that file. The step's three arms stay pinned, now through
`halt_of_pass`: budget (`tests/cli.rs:7374`), hung (`tests/cli.rs:7549`), spec defect
(`tests/plan_critique_spec_defect_stop_periphery.rs:254`). The one existing test that changes is
`run_end_to_end_restores_a_worktree_a_reviewer_agent_deletes_mid_review` (`tests/cli.rs:6134`),
a clean `rigger run`, which gains criterion 1's assertion that its stdout holds no `halted:`.

**DOCUMENT EDITS.** None: no passage in `docs/`, the skills directories and the README says what
a run entry prints on a halt; the rustdoc naming `rigger step` the stamper stays true.

**OUT OF SCOPE.** Exit status (a halt is a run outcome; `rigger step` exits 0 on one,
`tests/cli.rs:7379-7385`); the step line's other fields; the Workflow driver's stop text.

## Global constraints

- Hyphens, never em or en dashes, and ASCII only, in every added line.
- No new event type; no new crate dependency.
- Both feature lanes green (fmt, clippy -D warnings, test on default and --no-default-features).
- Flagged for the adjudicator (the per-entry tests pin only the budget arm): each entry hands
  `halt_of_pass` the slice its own post-pass read returned, never a literal empty slice, and
  `run_workflow` prints nothing when `halt_of_pass` answers `None`.

## Done when

- [ ] a test proves THE BLOCKING RUN PRINTS THE HALT REASON: `rigger run` over the budget-one fixture exits 0 and prints `halted: budget exhausted: 1/1 spawns` on stdout after the run state,
  asserted in `tests/cli.rs` with the fake agent executable of `tests/cli.rs:6055` on `PATH`
  answering the worker, the clean run of `tests/cli.rs:6134` printing no `halted:`. This
  criterion OWNS `halt_of_pass`, `halted_line`, `cmd_step`'s move onto `halt_of_pass` and
  `run_cli`'s call; `run_workflow`'s call is criterion 2's, NOT this one's.
- [ ] a test proves THE WORKFLOW DRIVER PRINTS THE HALT REASON: `rigger run --driver workflow` over the budget-one fixture prints `halted: budget exhausted: 1/1 spawns` on stderr when its conductor returns,
  asserted in `tests/cli.rs` on a process spawned through `rigger_command`
  (`tests/common/cli.rs:122`) with piped stdio and `--base HEAD`, answered with one
  `rigger_result` for the `*/implementer#0` spawn `rigger_next` hands out, its stderr read once
  its stdin closes. This criterion OWNS `run_workflow`'s call and its stream; `halt_of_pass` and
  `halted_line` are criterion 1's, NOT this one's.
- [ ] both feature lanes green (fmt, clippy, test on default and --no-default-features). This
  criterion OWNS only the lanes over the integrated result.
