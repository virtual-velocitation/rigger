# 69 - Ship the watching discipline: monitoring skills and anomalies that push

**Goal:** a launched run must be WATCHED; today the orchestrating session routinely stops
monitoring, and anomalies are pull-only whispers. Recorded costs: a dead dash the OPERATOR
found first; a quota-killed driver undetected until the next human message; status printing a
URL nothing served; escalations nobody is forced to read. Two halves: per-scenario watch
skills, and push-side guardrails that land anomalies in the orchestrator's own surfaces.

## Design

- **Three new per-operation skills** (registry entries; sources at
  `skills/rigger-<operation>/SKILL.md`; symptom-carrying descriptions, one procedure each,
  named anti-moves, cross-links by name - the spec-68 contract):
  - `rigger-watch-a-run` - the monitoring protocol. Tells: you just launched a run; a run has
    driven unattended a while. Body: the FIVE SIGNALS per look - (1) escalated blockers,
    (2) heartbeat staleness vs live agent processes, (3) dash liveness, (4) per-unit
    reject-recurrence trend, (5) FRONTIER PROGRESS: is the run consuming what it spawns? A
    spawn id surviving consecutive looks, an hours-old last run event under "working" agents,
    or a repeating wave is a stall - liveness reads healthy in a stalled run, which is why
    progress is its own signal. Each signal maps BY NAME to its response skill
    (`rigger-handle-an-escalation`, `rigger-resume-a-run`, `rigger-restore-the-dash`,
    `rigger-diagnose-churn`; stall: stop the driver and diagnose before another round
    spends). FIRST instruction: on launch, ARM `rigger watch` under the harness's background
    monitor; the manual look is the fallback, at least once per remediation cycle.
    Anti-moves: polling git/ps as the primary view; hand-intervening in a slow-not-stuck run.
  - `rigger-restore-the-dash` - get the dash serving. Tells: URL does not answer, status says
    not serving, browser spins. Body: singleton semantics, status's dash line, restart via
    `rigger dash`, and the hung-holder case - a stopped process holding the port never
    accepts, so clients hang; resume or kill THAT pid (the bind diagnosis names it).
    Anti-moves: hand-editing the marker, killing by port-adjacent guesswork.
  - `rigger-diagnose-churn` - act on a repeatedly rejected unit. Tells: reject-recurrence
    past ~3, oscillating diffs. Body: the finding audit - read blocking findings against
    diffs; factually-correct findings clustering on one constraint means fix the SPEC (the
    amendment protocol in `planning-a-spec`); separate infra-caused attempts before judging;
    the churn-signature table maps signature to planning-defect class. Anti-moves: blaming
    the model or panel without the audit; reflexively raising `max_retries`.
- **A driver-independent watchdog ships as `rigger watch`** (`src/main.rs`, new command): the
  five signals as a command any orchestrator arms (prototyped as this project's session
  watchdog; an orchestrator that needed a local script is proof every consumer needs the
  command). Polls store and status (default 180s, `--interval <s>`), prints ONE LINE PER
  ANOMALY naming signal, subject, and response skill - one per skill signal, pinned so
  command and skill cannot disagree: a spawn id with >= 3 unconsumed results, an escalated
  unit, reject-recurrence at the diagnose threshold (>= 3, counted and re-alerted PER
  FAILURE CAUSE - see the cause wire), a dead driver, store shrink or out-of-order
  revisions, a dash line nothing serves. Dead-driver is a CONJUNCTION (store quiet a full
  hour AND no step process AND every heartbeat stale >30 min) - tuned by its first live
  false positive; an alert firing on quiet-but-heartbeating work teaches operators to
  ignore the watchdog. Alerts dedupe until cleared; DEDUP STATE LIVES IN PROCESS MEMORY
  ONLY (an observer that writes into the watched project becomes something the watched
  system must account for; accepted consequence: a restarted watch re-alerts standing
  anomalies once - correct for a fresh observer). `--once` prints standing anomalies and
  exits (cron/CI); streaming is the harness-monitor default. Reads only store, process
  table, and status - never the driver, which is exactly the process that may be dead.
- **A failed unit names its cause; recurrence counts per cause** (`src/conductor.rs`
  `TYPE_UNIT_FAILED` emit sites, the ledger fold, `cmd_status`, watch/attention): the
  conductor is in a distinct branch for each failure mode but emits a bare `{id, attempts}`.
  Stamp an additive serde-defaulted `cause` at each emit site from that branch (`reject`,
  `gate:<name>`, `integrate-conflict`, `infra:<kind>`) - closed values, never inferred
  downstream. Status appends it (`reject-recurrence #4/6 (integrate-conflict)`); watch and
  attention count recurrence per cause and name it: same-cause repeats are churn to audit, a
  changed cause is progress. (Recorded cost of the bare count: one alert conflated two
  infra rounds with a legitimate merge-conflict round.)
- **`rigger status` never lies about the dash** (`src/main.rs::cmd_status`): before printing
  the URL, VERIFY something serves it (the step path's liveness check); a dead marker prints
  `dashboard: not serving (marker names dead pid <N>) - run 'rigger dash' or the next step
  restarts it`. `--json` carries the same truth.
- **The step wire carries attention; the driver relays it** (`src/main.rs::cmd_step` /
  `src/spawn.rs::Step`, then `workflows/rigger.js`): ONE additive, serde-defaulted
  `attention` array on the step's JSON line, stamped by `rigger step` from live conductor
  state exactly as `halted` is - the driver never scrapes or infers. Entries: unit ESCALATED,
  run HALTED with reason, Nth worker death on one unit, budget crossing its final tenth, and
  STALLED FRONTIER - a parked spawn already holding multiple recorded results (a spawn
  answered more than twice without the run advancing burns full agent cost per round; the
  recorded incident cost thirteen million tokens). Threshold events stamp ONCE PER CROSSING,
  conductor-side. Omitted when empty - a clean run's wire is byte-stable. The driver renders
  each entry as a `log()` narrator line naming event and response skill; otherwise
  byte-for-byte unchanged (log lines only, no new stops, no retry-rule changes).

## Notes (non-criteria)

- This spec ships detection and the diagnose/restore procedures; resume and escalation
  response protocols are spec 68's skills, referenced by name.
- The five signals are exactly the recorded failure modes of real sessions; signals earn
  their place from incidents, never speculation.
- No new event type is introduced anywhere in this spec.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to prior
  harnesses or to projects unrelated to the mechanism.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- The docs-drift gate stays green over the registry; `rigger setup` remains non-destructive.
- Push is additive-only: the step JSON gains one optional serde-defaulted `attention` field
  (omitted when empty; graceful in both version-mix directions), the driver gains narrator
  lines, never new failure modes; status gains truth, never a changed exit status;
  `node --check` passes on the template. The marker LIFECYCLE (write-after-bind, self-heal)
  is spec 62's - this spec only makes status VERIFY before it prints.

## Done when

- [ ] a test proves the WATCH SKILLS RENDER TRUE: `rigger-watch-a-run` names the five signals
  each mapped to its response skill, `rigger-restore-the-dash` carries the hung-holder
  diagnosis, and `rigger-diagnose-churn` carries the finding-audit procedure with the
  infra-separation step - symptom-carrying descriptions, one operation per skill, command
  references accuracy-pinned. This criterion OWNS the skill content; registry install
  mechanics are spec 68's, NOT this spec's.
- [ ] a test proves THE WATCHDOG: `rigger watch --once` on a store seeded with a multi-result
  spawn, an escalated unit, a unit at reject-recurrence three, and an out-of-order tail
  prints one line per anomaly naming signal, subject, and response skill; on a clean store
  it prints nothing; streaming mode dedupes a persisting anomaly until it clears and
  re-alerts a churn count on each increment; and the command's signal set covers every
  signal the watch skill names, pinned so the two cannot drift. This criterion OWNS the
  watchdog command; it must work with the driver dead.
- [ ] a test proves A FAILED UNIT NAMES ITS CAUSE: each conductor failure branch stamps its
  distinct `cause` on the `UnitFailed` it emits (additive, serde-defaulted; a cause-less
  prior event reads as unknown); status blocker lines carry it; the watch churn line counts
  per cause, so a changed cause never reports as same-cause churn. This criterion OWNS the
  cause wire.
- [ ] a test proves STATUS NEVER LIES ABOUT THE DASH: with a marker naming a dead pid, status
  prints the not-serving line (naming the dead pid and the restart) and no URL; with a live
  serving dash, today's URL line is unchanged; `--json` carries the same truth. This
  criterion OWNS the status dash line.
- [ ] a test proves THE STEP STAMPS ATTENTION: a step during which a unit escalated, the run
  halted, a worker's death recurred, the budget crossed into its final tenth, or a parked
  spawn already carries more than two recorded results prints the additive `attention` array
  naming each event and unit - stamped from live conductor state, once per threshold
  crossing, omitted entirely on a clean step. This criterion OWNS the wire stamp.
- [ ] a test proves THE DRIVER RELAYS ATTENTION: each `attention` entry produces a narrator
  log line naming the event, the unit, and the response skill, at the wave it arrived - and
  the driver renders ONLY what the wire says (an entry-less step logs nothing). This
  criterion OWNS the relay; the wire stamp is the previous criterion's, and its log-only
  bound is pinned (no stop-path change).
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`),
  including `node --check` on the template.
