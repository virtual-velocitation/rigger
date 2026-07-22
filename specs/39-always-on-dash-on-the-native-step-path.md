# 39 - Always-on dash on the native step path: a persistent, self-reaping run dashboard

**Goal:** correct the observability drift. Spec 19b's promise is that "an active harness is never
invisible" - a run auto-starts a read-only dashboard. But auto-start is wired into only two drive
paths: `rigger run` (the monolithic loop) and the MCP-session `run_workflow`. The native `/rigger`
workflow - the path we actually drive with - advances the conductor through many short-lived
`rigger step` (`cmd_step`) invocations, and `cmd_step` never calls `start_run_dashboard`. Worse, even
if it did, `start_run_dashboard` hands back a `ReapedChild` guard bound to the calling process, so a
per-frontier `step` would flicker a dash up and immediately reap it on return. The result: the drive
path we use runs the loop INVISIBLE (observed live - no dash was serving while a run was in flight).
This spec makes the `step` path start a PERSISTENT, SELF-REAPING run dashboard: the first step of a
run starts it if none is already serving for the project, later steps do not restart it, it survives
each short-lived `step` process, and it self-reaps when the run goes idle/complete - not when a step
returns.

## Design

`cmd_step` (`src/main.rs`) advances one frontier and returns; `start_run_dashboard` /
`spawn_run_dashboard` (`src/main.rs`) pick a free port, spawn `rigger dash --port <n>` as a child,
and return a `dash::ReapedChild` whose Drop reaps the child (spec 19b unit 3's reaping). The dash is
read-only and self-contained (`src/dash.rs`), and `rigger status` already reads a liveness marker the
run maintains (`src/main.rs` notes the auto-started dashboard + the liveness marker `rigger status`
consults). Three changes, scoped to the step drive path and the dash lifecycle:

- **Idempotent start on step.** `cmd_step` starts the run dashboard when NONE is already serving for
  this project. "Already serving" is a discoverable fact - a per-project dash marker (the port /
  pid record the dash writes, alongside the liveness marker it already reads), checked before spawn -
  so the second and every later `step` of a run is a no-op, never a second dash or a port fight.
- **Detached, not process-bound.** The step-started dash is NOT held by a `ReapedChild` in the
  `cmd_step` process (which returns per frontier); it is started detached so it survives across the
  run's many `step` invocations. The `rigger run` / `run_workflow` paths keep their existing
  guard-bound dash (those hold one long-lived process); this spec adds the persistent variant only
  for the step path.
- **Self-reaping on run-idle.** The persistent dash reaps ITSELF when the run it serves is complete
  or its liveness goes stale (the same liveness/heartbeat signal spec 19b already maintains and
  `rigger status` reads), so it never leaks after the run ends. Reaping is by the run's OWN liveness
  going stale, decoupled from any single `step` process exiting.

No new event type; the dash stays a read-only projection over the store. The behavior is identical
in both feature lanes (the dash serves in both).

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any external
  tool or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- Read-only + best-effort: the dash never mutates the store (it is observability, not the
  deliverable); if the dash cannot start (port-starved, spawn refused) the run proceeds headless with
  a clear warning, exactly as `start_run_dashboard` already degrades - never a run that fails because
  its dashboard could not start.
- Single-instance per project: the idempotency check guarantees at most one run dashboard per project
  at a time, so repeated steps never spawn a pile of dashes or fight over ports.
- Determinism / no leak: a completed or crashed run leaves no orphaned dash - the self-reap on stale
  liveness is the backstop that the process-bound guard used to provide for the other paths.

## Done when

- [ ] a test proves IDEMPOTENT START: a `step` with no run dashboard serving for the project starts
  one, and a subsequent `step` while it is serving starts NO second dash (the marker/port check
  short-circuits). This criterion OWNS the start-once-per-run behavior on the step path.
- [ ] a test proves the dash is DETACHED (persists across steps): the dash started by one `step` is
  still serving after that `step` process returns (it is not bound to a `ReapedChild` in the step
  process). This criterion OWNS persistence across the run's many step invocations; it does NOT own
  the idempotent start (criterion 1).
- [ ] a test proves SELF-REAP on run-idle: when the run's liveness goes stale / the run completes,
  the persistent dash reaps itself (no orphaned dash), driven by the liveness signal and NOT by a
  step process exiting. This criterion OWNS the self-reaping lifecycle; it does NOT own start or
  persistence (criteria 1-2).
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
