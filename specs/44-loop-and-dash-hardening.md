# 44 - Loop and dash hardening: ship the courier, driver, and dash-detach fixes

**Goal:** three infrastructure defects surfaced while dogfooding the loop must ship IN THE BINARY (so
they reach every consumer and survive workflow regeneration), each pinned by a regression test.
Currently they exist only as edits to the INSTALLED `.claude/workflows/rigger.js` - a local unblock
that a `rigger setup` overwrites and a fresh install never receives. The source of truth is
`workflows/rigger.js` (embedded via `include_str!` at `src/main.rs`, written to a consumer's
`.claude/workflows/rigger.js` on setup) and `src/main.rs`; fix them there.

The three defects:

1. **The step courier backgrounds `rigger step` and fabricates a placeholder error.** The workflow
   courier is meant to run `rigger step` as one foreground, blocking call and return the JSON it
   prints. Intermittently it instead runs the step in the BACKGROUND, sets up a Monitor to watch the
   output, then returns `{"wave":[],"done":false,"error":"PLACEHOLDER_DO_NOT_USE"}` before the step
   has produced anything - a fabricated error that stops the run after zero waves. The loop lies about
   its own state.
2. **The driver crashes on a null step instead of stopping cleanly.** When the courier AGENT dies on a
   terminal error (an expired login, an exhausted quota), the workflow's `agent()` RESOLVES to null
   rather than rejecting, so the try/catch never fires and the driver crashes dereferencing
   `step.error` - an uncaught crash instead of a clean, resumable stop.
3. **The always-on dash dies when the step command completes.** `spawn_run_dashboard_detached`
   (`src/main.rs`) spawns the dash with only null stdio and relies on dropping the Rust `Child` (whose
   Drop neither waits nor kills) as its "detachment". It never puts the dash in its own session or
   process group, so the dash stays in `rigger step`'s process group. When the workflow courier runs
   `rigger step` as a foreground command, the harness tears down that command's process group on
   completion and reaps the dash with it - the spec-39 always-on dash dies the instant the step
   returns, every step.

## Design

### Courier and driver (`workflows/rigger.js`, the embedded source)

- **Foreground, no fabrication.** The step-courier prompt instructs the agent to run `rigger step` as
  ONE FOREGROUND, BLOCKING Bash call - explicitly NOT `run_in_background`, and NOT via a Monitor or
  poll loop - because a foreground Bash call blocks until the step prints its single JSON line, which
  is exactly the line to return. And the courier's `error` string, when it must report a failure, MUST
  be the command's actual stderr text or the literal phrase `step did not complete within my attempts`
  - NEVER an invented placeholder token. A placeholder in any field means the command was not run to
  completion.
- **Null-step guard.** After the `agent()` call, the driver guards `!step` before reading `step.error`:
  a null step (the courier agent died on a terminal API error, so `agent()` resolved to null rather
  than rejecting) stops the driver cleanly and loudly with a diagnostic that names the likely cause
  and that the run is resumable, instead of crashing uncaught on a null dereference.

### Dash detachment (`src/main.rs`)

`spawn_run_dashboard_detached` puts the dash in its OWN session / process group before spawning, so it
survives the teardown of the `rigger step` command's process group. Use the standard-library Unix
primitive (`std::os::unix::process::CommandExt::process_group(0)`, or a `pre_exec` calling
`libc::setsid`) so the child is a group/session leader, detached from the parent command's group. The
existing null-stdio and drop-without-kill stay; this adds the missing session-detachment that makes
"detached" actually mean detached. Non-Unix builds keep the current behavior (the always-on dash is a
Unix-path feature).

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any external tool
  or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- The fixes ship IN THE BINARY: the courier/driver fixes live in the `include_str!`-embedded
  `workflows/rigger.js`, and the dash fix in `src/main.rs`, so a `rigger setup` writes the corrected
  workflow and every `rigger` invocation carries the corrected dash detachment. No fix may live only
  in the installed `.claude/` copy.
- Determinism / read-only: these are process-lifecycle and prompt-text fixes; they add no event type
  and do not touch the event log or the graph projection.

## Done when

- [ ] a test proves the COURIER PROMPT is foreground-and-honest: the embedded workflow source (the
  `include_str!`-loaded `RIGGER_WORKFLOW` string) instructs the step courier to run `rigger step` as a
  foreground/blocking call (not `run_in_background`, not a Monitor) and forbids returning a fabricated
  placeholder token in `error` (the error must be real stderr or the fixed no-completion phrase). This
  criterion OWNS the courier-prompt guarantee.
- [ ] a test proves the DRIVER NULL-STEP GUARD: the embedded workflow source guards a null step before
  dereferencing `step.error`, stopping cleanly with a resumable diagnostic rather than crashing. This
  criterion OWNS the null-step guard; it does NOT own the courier prompt (criterion 1). (A structural
  assertion on the embedded source is sufficient and correct - the same style spec 39 used for the
  workflow string.)
- [ ] a test proves the DASH IS SESSION-DETACHED: the dash spawned by `spawn_run_dashboard_detached`
  is placed in its own process group / session (a different process group than its parent), so a
  teardown of the parent command's process group does not reap it. This criterion OWNS the dash
  detachment; it does NOT own the courier or driver fixes.
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
