# 62 - Dash marker lifecycle: record only a live server, explain a held port, survive live work

**Goal:** make `.rigger/dash.marker` trustworthy (issue #25) and the always-on singleton
actually always-on while work is live. Two recorded defects: (1) a process can write the
marker and then FAIL to bind (port held by a job-control-stopped predecessor whose socket
never accepts - clients hang in the backlog), leaving a marker naming a dead PID that a later
successful start never rewrites; (2) the step-path singleton SELF-REAPS mid-run
(`f-dash-selfreap-blind-to-agent-work`): only driver commands heartbeat the instance
registry, courier commands (`progress`, `emit`, `result`) do not, so any agent phase longer
than the 900s idle window empties the registry and the watcher exits the dash while agents
are in flight.

## Design

- **Marker follows bind** (`src/dash.rs` / `src/main.rs` dash startup): the marker is written
  only AFTER the listener has bound. A failed bind leaves the prior marker byte-for-byte
  untouched and writes nothing.
- **Stale markers self-heal** (`src/main.rs` dash startup): a successful start replaces
  whatever marker it finds (dead PID, wrong port) with its own `port\npid`. The still-serving
  short-circuit (live marker exits 0 without binding) is unchanged.
- **Couriers count as activity** (`src/main.rs` courier entry points, `src/registry.rs`):
  `progress`, `emit`, and `result` refresh the project's registry entry with a fresh
  heartbeat - one-shot re-stamp through the existing write path, no heartbeat thread,
  best-effort and warn-only like the driver registration, including its degrade: a homeless
  environment or unwritable registry skips silently and never fails, slows, or warns the
  courier's work. This honors the documented contract: every invocation that starts or
  advances a run keeps the instance discoverable.
- **The idle judgment sees agents** (`src/main.rs::watch_and_self_reap_on_idle` seam): reap
  requires BOTH an empty registry AND no live agent signal (the same liveness authority
  `rigger status` presents); a genuinely quiet machine reaps exactly as today.
- **A held port is explained, not silent** (`src/main.rs` dash startup): a bind failure on a
  held port names the holding PID and its process state when discoverable; a stopped-state
  holder gets the explicit diagnosis (a stopped listener keeps the port but never accepts -
  resume or kill that PID). An undiscoverable holder still gets the held-address report,
  never a bare exit.

## Notes (non-criteria)

- The marker format (`port\npid`) and its readers are unchanged; this spec changes WHEN it is
  written and what a failed bind reports.
- Removal-on-exit is out of scope: the detached singleton outlives its parent by design;
  write-after-bind plus self-heal make a leftover marker harmless.
- Platform bound, decided here: holder PID/state discovery reads the proc surface and is a
  Unix-path feature like the always-on dash itself; other platforms still get the
  held-address report. The always-serving criterion set is judged on the Unix path.
- No new event type is introduced anywhere in this spec.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any
  external tool or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- Loopback-only, zero-new-dependency dash charter holds: no async runtime, Cargo.toml gains
  nothing.

## Done when

- [ ] a test proves MARKER FOLLOWS BIND: a dash start whose bind fails writes no marker and
  leaves an existing marker untouched, while a successful bind writes the marker naming the
  bound port and the serving PID. This criterion OWNS the write ordering.
- [ ] a test proves SELF-HEAL: a successful start over a stale marker (dead PID, or live PID
  not serving the recorded port) replaces it with the new server's record.
- [ ] a test proves the HELD-PORT DIAGNOSIS: a bind failure on a held port reports the holder
  (PID and process state when discoverable, the held address always), and a stopped-state
  holder gets the explicit hung-listener explanation naming resume-or-kill.
- [ ] a test proves COURIERS KEEP THE INSTANCE LIVE: a courier invocation (`progress`, `emit`,
  or `result`) refreshes the project's registry entry, so an instance whose only activity is
  courier traffic within the idle window stays in `read_live` past where it ages out today.
  This criterion OWNS the courier registration.
- [ ] a test proves the SINGLETON SURVIVES LIVE WORK: with an aged-out registry but a fresh
  in-flight agent liveness signal, the self-reap watcher does not reap; with both quiet, it
  reaps exactly as today. This criterion OWNS the idle judgment.
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
