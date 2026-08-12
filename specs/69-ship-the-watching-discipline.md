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
    launched a run; a run has been driving unattended for a while. Body: the FOUR SIGNALS to
    check on every look - (1) escalated blockers, (2) heartbeat staleness against live agent
    processes (a stale heartbeat with no live worker means the driver or worker died), (3)
    dashboard liveness, (4) the reject-recurrence trend per unit - each mapped BY NAME to its
    response skill (`rigger-handle-an-escalation`, `rigger-resume-a-run`,
    `rigger-restore-the-dash`, `rigger-diagnose-churn`); and the cadence rule: look on every
    wave boundary you are present for, and never less than once per remediation cycle.
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
- **`rigger status` never lies about the dash** (`src/main.rs::cmd_status`): before printing
  the dashboard URL, status VERIFIES something is serving it (the marker's liveness check the
  step path already owns); a dead marker prints
  `dashboard: not serving (marker names dead pid <N>) - run 'rigger dash' or the next step
  restarts it` instead of a URL that answers to nobody. The `--json` shape carries the same
  truth as a field.
- **The driver pushes anomalies as they happen** (`workflows/rigger.js`): the moment a step's
  returned wave or status carries an attention event - a unit ESCALATED, the run HALTED with a
  reason, a worker's death recorded for the Nth time on one unit, the spawn budget entering
  its final tenth - the driver emits a `log()` narrator line naming the event and the response
  skill, so the orchestrating session sees it live in its own progress surface (and in the
  transcript) without polling anything. The driver's behavior is otherwise byte-for-byte
  unchanged: log lines only, no new stops, no retry-rule changes.

## Notes (non-criteria)

- Detection vs response: this spec ships detection (watching, pushing) and the diagnose/restore
  procedures; the response protocols for resume and escalation are spec 68's skills, referenced
  by name and not duplicated.
- The four signals are exactly the recorded failure modes of real sessions; a fifth signal
  earns its place in the skill when a run records it, not speculatively.
- No new event type is introduced anywhere in this spec.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to prior
  harnesses or to projects unrelated to the mechanism.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- The docs-drift gate stays green over the registry; `rigger setup` remains non-destructive.
- Push is additive-only: the driver gains narrator lines, never new failure modes; status
  gains truth, never a changed exit status; `node --check` passes on the template.

## Done when

- [ ] a test proves the WATCH SKILLS RENDER TRUE: `rigger-watch-a-run` names the four signals
  each mapped to its response skill, `rigger-restore-the-dash` carries the hung-holder
  diagnosis, and `rigger-diagnose-churn` carries the finding-audit procedure with the
  infra-separation step - all with symptom-carrying descriptions, one operation per skill,
  command references accuracy-pinned. This criterion OWNS the skill content; registry
  install mechanics are spec 68's, NOT this spec's.
- [ ] a test proves STATUS NEVER LIES ABOUT THE DASH: with a marker naming a dead pid, status
  prints the not-serving line (naming the dead pid and the restart) and no URL; with a
  live serving dash, today's URL line is unchanged; `--json` carries the same truth. This
  criterion OWNS the status dash line.
- [ ] a test proves THE DRIVER PUSHES ESCALATIONS: a wave whose step reports a unit escalated
  (or a halted run) produces a narrator log line naming the unit, the event, and the
  response skill, at the wave it happened - not only in the fixpoint stop message. This
  criterion OWNS the driver push; its log-only bound is pinned (no stop-path change).
- [ ] a test proves THE DRIVER PUSHES ATTRITION: a worker death recorded repeatedly for one
  unit and a spawn budget entering its final tenth each produce their narrator line once
  per threshold crossing, never per wave. This criterion OWNS the attrition push.
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`),
  including `node --check` on the template.
