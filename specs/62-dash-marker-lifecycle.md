# 62 - Dash marker lifecycle: record only a live server, explain a held port

**Goal:** make `.rigger/dash.marker` trustworthy (issue #25). Today the marker can be written
by a process that then FAILS to bind (the port was still held), leaving a marker naming a PID
that no longer exists; a later successful start does not rewrite it. The marker is the first
place an operator looks when the dashboard is unreachable, and it actively misleads: "the dash
isn't running" reads as true while a hung predecessor holds the port. The observed sequence: an
earlier `rigger dash` was stopped by job control (process state `T`), which keeps its listening
socket but never accepts - connections stack in the kernel backlog (observed 19 deep) and the
browser spins forever instead of being refused - then a later `rigger dash` wrote the marker
with its own PID, failed to bind, and exited.

## Design

- **Marker follows bind** (`src/dash.rs` / `src/main.rs` dash startup): the marker is written
  (or overwritten) only AFTER the listener has successfully bound, so it always names a process
  that actually held the port at write time. A start that fails to bind leaves the prior marker
  byte-for-byte untouched and writes nothing.
- **Stale markers self-heal** (`src/main.rs` dash startup): a successful start reconciles
  whatever marker it finds: dead PID or wrong port, the new server's `port\npid` record simply
  replaces it. The existing still-serving short-circuit (a marker naming a live, serving dash
  exits 0 without binding a second) is unchanged.
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
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
