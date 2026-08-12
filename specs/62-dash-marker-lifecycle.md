# 62 - Dash marker lifecycle: record only a live server, explain a held port, survive live work

**Goal:** make `.rigger/dash.marker` trustworthy (issue #25) and make the always-on singleton
actually always-on while work is live. Today the marker can be written by a process that then
FAILS to bind (the port was still held), leaving a marker naming a PID that no longer exists; a
later successful start does not rewrite it. The marker is the first place an operator looks when
the dashboard is unreachable, and it actively misleads: "the dash isn't running" reads as true
while a hung predecessor holds the port. The observed sequence: an earlier `rigger dash` was
stopped by job control (process state `T`), which keeps its listening socket but never accepts -
connections stack in the kernel backlog (observed 19 deep) and the browser spins forever instead
of being refused - then a later `rigger dash` wrote the marker with its own PID, failed to bind,
and exited.

A second defect was then observed live (finding `f-dash-selfreap-blind-to-agent-work`): the
step-path singleton SELF-REAPS IN THE MIDDLE OF A RUN. Its idle watcher reads only the
machine-global instance registry, and only DRIVER processes (`rigger step`/`run`/`serve`) hold a
registry heartbeat - the guard drops when the step exits, and the entry ages out after the 900s
idle window. The courier commands that agents call throughout real work (`rigger progress`,
`rigger emit`, `rigger result`) never touch the registry, even though `register_run_instance`'s
own contract says every invocation that starts or advances a run registers. So any agent phase
longer than the window (a long adversarial review, a long implementation) empties the registry
and the watcher exits the dash while agents are demonstrably in flight; every next step respawns
it and rewrites the marker, and the dashboard is only ever reachable within one window of the
last step.

## Design

- **Marker follows bind** (`src/dash.rs` / `src/main.rs` dash startup): the marker is written
  (or overwritten) only AFTER the listener has successfully bound, so it always names a process
  that actually held the port at write time. A start that fails to bind leaves the prior marker
  byte-for-byte untouched and writes nothing.
- **Stale markers self-heal** (`src/main.rs` dash startup): a successful start reconciles
  whatever marker it finds: dead PID or wrong port, the new server's `port\npid` record simply
  replaces it. The existing still-serving short-circuit (a marker naming a live, serving dash
  exits 0 without binding a second) is unchanged.
- **Couriers count as activity** (`src/main.rs` courier entry points, `src/registry.rs`): the
  courier commands that carry agent work (`rigger progress`, `rigger emit`, `rigger result`)
  refresh this project's machine-global registry entry with a fresh heartbeat - a one-shot
  re-stamp through the existing write path, no heartbeat thread, best-effort and warn-only
  exactly like the driver registration, including its degrade: a homeless environment (no
  state home) or an unwritable registry skips the re-stamp silently and never fails, slows,
  or warns the courier's actual work. This makes the code honor the documented registration
  contract: every invocation that starts or advances a run keeps the instance discoverable, so
  a run whose agents are working keeps its entry alive across an agent phase of any length.
- **The idle judgment sees agents** (`src/main.rs::watch_and_self_reap_on_idle` seam): before
  reaping, the watcher also consults in-flight agent liveness for the registered projects - the
  same liveness-marker/progress authority `rigger status` already presents - so live agent work
  is machine activity even across a courier-silent gap. Reap requires BOTH an empty registry
  AND no live agent signal; a genuinely quiet machine still reaps exactly as today.
- **A held port is explained, not silent** (`src/main.rs` dash startup): when the bind fails
  because the address is in use, the error names the holding PID and its process state when
  they are discoverable (via the same proc inspection the platform allows), and a holder in a
  stopped state gets the explicit diagnosis: a stopped listener keeps the port but never
  accepts, so clients hang rather than being refused - resume or kill that PID. When the
  holder is not discoverable, the error still says the port is held and by what address, never
  a bare exit.

## Notes (non-criteria)

- The marker format (`port\npid`) and its readers (`DashMarker::parse` / `read`, the
  step-path singleton logic) are unchanged; this spec changes WHEN it is written and what a
  failed bind reports.
- Removal-on-exit is intentionally out of scope: the detached singleton outlives its parent by
  design, and the self-heal-on-start plus write-after-bind together make a leftover marker
  harmless (it is reconciled by the next start and never names a process that failed to bind).
- Platform bound, decided here: holder PID/state discovery reads the proc surface and is a
  Unix-path feature exactly like the always-on dash itself; a platform without it still gets
  the held-address report, never a silent exit. The always-serving criterion set is judged on
  the Unix path.
- No new event type is introduced anywhere in this spec.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any
  external tool or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- Loopback-only, zero-new-dependency dash charter holds: the hand-rolled synchronous HTTP
  layer gains no async runtime and Cargo.toml gains nothing.

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
