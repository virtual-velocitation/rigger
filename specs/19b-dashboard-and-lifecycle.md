# 19b - Dashboard always-on, responsive, and self-reaping

**Goal:** an active rigger harness must always have a dashboard serving it (no opt-in), the
dashboard must be readable (responsive, no runaway horizontal scroll), and every long-lived
`rigger` child must be reaped when its run ends (no orphans). Second of three specs split
from Workstream B of `docs/architecture-addendum-pit-of-success.md`.

## Design

Builds on the dashboard (`src/dash.rs`, `src/dash.html`, `dash::DEFAULT_PORT`), the run
entry points (`run_workflow` / `run_cli` / `cmd_serve` in `src/main.rs`), the peers sidecar
(`Sidecar::start`), and the shim (`shim/shim.mjs`).

**Unit 1 - always-on dash (touches `src/main.rs`, `src/dash.rs`).** Whenever any driver
(the native workflow, `rigger serve`, `rigger run`, `rigger workflow`) has a run in flight,
it ensures a `rigger dash` is serving that run - auto-started if none is up, on
`DEFAULT_PORT` or the next free port so concurrent harnesses each get their own, its URL
printed at run start and shown in `rigger status`. There is NO opt-in flag: a running
harness you cannot see is the opacity this campaign removes. This unit OWNS dash START and
URL discoverability; STOPPING/reaping the dash is unit 3's, not this unit's.

**Unit 2 - responsive redesign (touches `src/dash.html`).** The decision history renders in
an `overflow-x:auto` container with no wrapping, so long text scrolls far right and the page
body scrolls horizontally. Make decision/finding text wrap (`overflow-wrap`/`white-space:
normal`) and render as wrapped rows or cards; guarantee the page BODY never scrolls
horizontally (only intentionally-wide content scrolls in its own container); collapse the
`cols2` grid gracefully on narrow viewports. Visual responsiveness is outside the gate set
(rule 4): the adjudicator must demand explicit evidence (the changed CSS/markup and a
description of behavior at narrow and wide widths), not accept a green build as proof.

**Unit 3 - no orphaned processes (touches `src/main.rs`, `shim/shim.mjs`, `src/dash.rs`).**
A run starts long-lived `rigger` children - the MCP server (`rigger serve`, spawned by the
shim), the peers `Sidecar`, and the auto-started dash - and at least one is not reaped when
the driving agent finishes, leaving an orphaned `rigger` process that never ends. Give every
long-lived rigger child a supervised lifecycle - a process-group or kill-on-drop /
kill-on-parent-exit guard that kills AND waits (reaps) - so a normally-finishing OR crashing
agent leaves no orphan. This unit OWNS reaping every long-lived child, including the dash
unit 1 starts.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any
  external tool or project in code, comments, or commit messages.
- Determinism by construction for anything serialized (`BTreeMap`/`BTreeSet`/sorted `Vec`).
- BOTH feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D
  warnings`; `cargo test` - on default features AND `--no-default-features`.
- Two concurrent harnesses (e.g. two repos) each get their own dash on distinct ports;
  neither's dash nor reaping touches the other's store or processes.

## Done when

- [ ] a test proves that whenever a driver has a run in flight a `rigger dash` is serving it (auto-started if none is up, on `DEFAULT_PORT` or the next free port), with the URL printed at run start and shown in `rigger status`, and no opt-in flag - this unit owns dash start + discoverability, NOT reaping
- [ ] the dashboard decision history wraps long text and the page body does not scroll horizontally at narrow and wide widths (verified by the changed CSS/markup; visual proof is outside the gate set and the adjudicator must demand it explicitly)
- [ ] a test spawns a supervised long-lived rigger child (serve, sidecar, or dash), drops its guard / exits the driver, and asserts the child process is no longer alive - a finishing or crashing agent leaves no orphaned `rigger` process
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`)
