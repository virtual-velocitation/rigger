# 69 - Ship the watching discipline: monitoring skills and anomalies that push

**Goal:** a launched run must be WATCHED, and today nothing makes that natural: the
orchestrating session (human or agent) that starts a loop routinely stops monitoring it, and
the system's anomalies are pull-only whispers. The recorded costs: a dashboard died mid-run
and the OPERATOR discovered it, not the orchestrator; a driver killed by quota exhaustion sat
undetected until the next human message; `rigger status` printed a dashboard URL while nothing
was listening on it (the marker named a dead PID and status trusted it); and unit escalations
surface only as a `status` blocker line that nobody is forced to read until fixpoint. Two
halves, matching the family shipped by the skill registry: per-scenario skills that define the
watch protocol and its responses, and push-side guardrails so an anomaly lands in the
orchestrator's own surfaces the moment it happens instead of waiting to be polled.

## Design

- **Three new per-operation skills** (registry entries; committed sources at
  `skills/rigger-<operation>/SKILL.md`; symptom-carrying descriptions, one procedure each,
  named anti-moves, cross-links by name - the spec-68 contract):
  - `rigger-watch-a-run` - the monitoring protocol for any in-flight run. Tells: you just
    launched a run; a run has been driving unattended for a while. Body: the FIVE SIGNALS to
    check on every look - (1) escalated blockers, (2) heartbeat staleness against live agent
    processes (a stale heartbeat with no live worker means the driver or worker died), (3)
    dashboard liveness, (4) the reject-recurrence trend per unit, (5) FRONTIER PROGRESS: is
    the run consuming what it spawns? A spawn id that survives consecutive looks, a store
    whose last run event is hours old while agents "work", or the same wave repeating are a
    stall, not slowness - liveness signals all read healthy in a stalled run, which is
    exactly why progress is its own signal - each mapped BY NAME to its response skill
    (`rigger-handle-an-escalation`, `rigger-resume-a-run`, `rigger-restore-the-dash`,
    `rigger-diagnose-churn`, and for a stall: stop the driver and diagnose before another
    round spends). The skill's FIRST instruction: on launching any run, ARM `rigger watch`
    under your harness's background-monitor primitive so the signals interrupt you - the
    manual five-signal look is the fallback for harnesses without one, on every wave boundary
    you are present for and never less than once per remediation cycle.
    Anti-moves: polling git/ps as the primary view (`rigger status` is the surface), and
    hand-intervening in a run that is merely SLOW rather than stuck.
  - `rigger-restore-the-dash` - get the dashboard serving again. Tells: the printed URL does
    not answer, status reports the dash not serving, a browser spins forever on the port.
    Body: the singleton semantics (one dash per machine, step-path autostart), reading
    status's dash line (post-guardrail it verifies liveness), restarting via `rigger dash`,
    and the hung-holder case - a stopped process holding the port never accepts, so clients
    hang rather than being refused; resume or kill THAT pid (the bind diagnosis names it).
    Anti-moves: hand-editing the marker file, killing processes by port-adjacent guesswork.
  - `rigger-diagnose-churn` - act on a unit that keeps getting rejected. Tells:
    reject-recurrence climbing past ~3, attempts whose diffs oscillate instead of shrinking.
    Body: the finding audit - read the blocking findings against the diffs (they cite
    checkable facts); factually-correct findings clustering on one constraint means fix the
    SPEC (the amendment protocol in `planning-a-spec`); infra-caused attempts (worktree
    restoration, cache thrash, quota deaths) are separated out before judging anything; the
    churn-signature table in `planning-a-spec` maps the signature to its planning-defect
    class. Anti-moves: blaming the model or the panel without the audit; reflexively raising
    `max_retries`.
- **A driver-independent watchdog ships as `rigger watch`** (`src/main.rs`, new command): the
  five signals as a COMMAND every consumer's orchestrator can arm, prototyped as this
  project's own session watchdog during the stalled-frontier incident (an orchestrator that
  needed a local script is the proof every consumer needs the command). `rigger watch` polls
  the store and status authorities on an interval (default 180s, `--interval <s>`) and prints
  ONE LINE PER ANOMALY to stdout, silent otherwise, each line naming the signal, the subject,
  and the response skill - ONE PER SKILL SIGNAL, pinned so the command and the watch skill
  can never disagree about what is watched: a spawn id accumulating results without
  consumption (>= 3), an escalated unit, a unit's reject-recurrence reaching the diagnose
  threshold (>= 3, re-alerted on each further increment - churn is a trend, and the moment to
  audit it is BEFORE the next attempt spends), a dead driver, store shrink or the
  out-of-order-revision signature, and a status dash line nothing serves. The dead-driver signal is a CONJUNCTION, tuned by its
  first live false positive: store not advancing for a full hour AND no step process AND
  every agent heartbeat stale (>30 min) - an adjudicator re-running two gate lanes writes
  nothing to the store for half an hour while its heartbeat stays fresh, and an alert that
  fires on quiet-but-heartbeating work teaches operators to ignore the watchdog. Alerts DEDUPE until their condition clears, so a long watch stays
  quiet, not noisy. THE DEDUP STATE LIVES IN PROCESS MEMORY AND NOWHERE ELSE - a watch is an
  observer, and an observer that writes files into the project it watches becomes a thing the
  watched system must account for; the accepted consequence, decided here, is that a
  RESTARTED watch re-alerts every still-standing anomaly once, which is the correct behavior
  for a fresh observer (an operator restarting the watch wants the current picture, not a
  prior process's memory of it). `--once` runs a single poll and exits printing every
  currently-standing anomaly (cron/CI integration); the streaming default is what a harness
  monitor primitive consumes. CRUCIALLY it reads only the store, the process table, and
  status - never the driver - because the dead-driver signal is exactly the case where
  driver-relayed attention cannot exist.
- **`rigger status` never lies about the dash** (`src/main.rs::cmd_status`): before printing
  the dashboard URL, status VERIFIES something is serving it (the marker's liveness check the
  step path already owns); a dead marker prints
  `dashboard: not serving (marker names dead pid <N>) - run 'rigger dash' or the next step
  restarts it` instead of a URL that answers to nobody. The `--json` shape carries the same
  truth as a field.
- **The step wire carries attention; the driver relays it** (`src/main.rs::cmd_step` /
  `src/spawn.rs::Step`, then `workflows/rigger.js`): anomalies push through ONE additive,
  optional `attention` array on the step's JSON line, stamped by `rigger step` from the
  conductor's LIVE state exactly as the existing `halted` field is (the field the pure log
  fold cannot see is exactly the field the step must stamp) - the driver must never scrape or
  infer what the wire does not say. Each entry names the event and the unit: a unit ESCALATED,
  the run HALTED with its reason, a worker's death recorded for the Nth time on one unit, the
  spawn budget crossing into its final tenth - and a STALLED FRONTIER: a spawn id parked in
  the wave while the log already holds MULTIPLE results recorded against it (the conductor is
  re-driving work it cannot consume - whatever the cause, a spawn answered more than twice
  without the run advancing is burning full agent cost per round and must reach the operator
  as an attention event, not be discovered at round 37; the recorded incident this signal
  comes from cost thirteen million tokens). THRESHOLD EVENTS ARE STAMPED ONCE PER CROSSING,
  conductor-side (the breaker's idempotency discipline), so a long run never spams. The field
  is serde-defaulted and omitted when empty, keeping a clean run's wire byte-stable. The
  driver renders each entry as a `log()` narrator line naming the event and its response
  skill, so the orchestrating session sees it live in its own progress surface without polling
  anything. The driver's behavior is otherwise byte-for-byte unchanged: log lines only, no new
  stops, no retry-rule changes.

## Notes (non-criteria)

- Detection vs response: this spec ships detection (watching, pushing) and the diagnose/restore
  procedures; the response protocols for resume and escalation are spec 68's skills, referenced
  by name and not duplicated.
- The five signals are exactly the recorded failure modes of real sessions - the fifth
  (frontier progress) was added the day a run recorded it, per this spec's own rule that
  signals earn their place from incidents, never speculation.
- No new event type is introduced anywhere in this spec.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to prior
  harnesses or to projects unrelated to the mechanism.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- The docs-drift gate stays green over the registry; `rigger setup` remains non-destructive.
- Push is additive-only: the step JSON gains one optional serde-defaulted `attention` field
  (omitted when empty; an older driver ignores it, a newer driver on an older binary logs
  nothing - graceful in both mix directions), the driver gains narrator lines, never new
  failure modes; status gains truth, never a changed exit status; `node --check` passes on
  the template. The marker LIFECYCLE (write-after-bind, self-heal) is spec 62's - this spec
  only makes status VERIFY before it prints.

## Done when

- [ ] a test proves the WATCH SKILLS RENDER TRUE: `rigger-watch-a-run` names the four signals
  each mapped to its response skill, `rigger-restore-the-dash` carries the hung-holder
  diagnosis, and `rigger-diagnose-churn` carries the finding-audit procedure with the
  infra-separation step - all with symptom-carrying descriptions, one operation per skill,
  command references accuracy-pinned. This criterion OWNS the skill content; registry
  install mechanics are spec 68's, NOT this spec's.
- [ ] a test proves THE WATCHDOG: `rigger watch --once` on a store seeded with a
  multi-result spawn, an escalated unit, a unit at reject-recurrence three, and an
  out-of-order tail prints one line per anomaly naming signal, subject, and response skill;
  on a clean store it prints nothing; the streaming mode dedupes a persisting anomaly until
  it clears and re-alerts a churn count on each increment; and the command's signal set
  covers every signal the watch skill names, pinned so the two cannot drift apart. This
  criterion OWNS the watchdog command; the driver-relayed attention push is the wire
  criteria's, NOT this one's - the watchdog must work with the driver dead.
- [ ] a test proves STATUS NEVER LIES ABOUT THE DASH: with a marker naming a dead pid, status
  prints the not-serving line (naming the dead pid and the restart) and no URL; with a
  live serving dash, today's URL line is unchanged; `--json` carries the same truth. This
  criterion OWNS the status dash line.
- [ ] a test proves THE STEP STAMPS ATTENTION: a step during which a unit escalated, the run
  halted, a worker's death recurred, the budget crossed into its final tenth, or a parked
  spawn already carries more than two recorded results (the stalled frontier) prints the
  additive `attention` array naming each event and unit - stamped from live conductor state,
  once per threshold crossing, omitted entirely on a clean step so today's wire is
  byte-stable. This criterion OWNS the wire stamp.
- [ ] a test proves THE DRIVER RELAYS ATTENTION: each `attention` entry produces a narrator
  log line naming the event, the unit, and the response skill, at the wave it arrived - and
  the driver renders ONLY what the wire says (an entry-less step logs nothing). This
  criterion OWNS the relay; the wire stamp is the previous criterion's, and its log-only
  bound is pinned (no stop-path change).
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`),
  including `node --check` on the template.
