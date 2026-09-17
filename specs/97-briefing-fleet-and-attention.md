# 97 - Briefing, Fleet and attention

**Goal:** three things a person needs are missing even once the run views exist: a written
account of where a run stands ("tell me, in prose"), a board for every project this machine
is driving (the dashboard is already a machine singleton with an instance registry at
`/api/instances`, but it shows one project at a time), and attention that arrives rather
than being looked for - an escalation, a release-ready run or a stalled frontier on any
project should tap the person, and the two remedies the CLI performs (`rigger resume-unit`,
`rigger emit DecisionMade`) should be one confirmed click away. Mission Control's Briefing,
Fleet, inbox, notifications and guarded actions
(docs/architecture-addendum-mission-control.md, sections 5.6, 5.7 and 6) are those.

## Design

THE BRIEF, decided: `view("brief")` returns prose assembled only from recorded text: a
headline ("<spec> is release-ready" or "<spec> - where things stand at <time>"), a metadata
line (run, elapsed, steps from `StepTaken`, tokens from recorded usage, units landed of
total), one summary
sentence built from unit statuses and the working-agent count, Landed (each unit with time,
commit and reject count), Rejects and why (the first sentence of each rejecting ruling),
Decisions taken (operator rulings and governing decisions), What happens next (the dock's
steps), and on a done run the release commands `ledger::ReleaseReady` produces for `rigger
status`. The page renders it with the mock's typographic scale; the closing line says the
brief is generated from the log at the scrubber's position.

THE FLEET, decided: the singleton folds every registered instance's store through the
console core natively and `GET /api/console/fleet` returns one summary per instance (project,
spec, run, units by status, agents working, ETA, tokens from recorded usage, steps, last
event age, the five health signals, its attention entries, and a spawn-rate series);
`view("fleet")`
renders the strip (projects, live runs, agents working, needs-you count, tokens today), one
card per project (the unit progress bar coloured by status, the metadata, the signals, the
sparkline, `Open console`), the needs-you inbox across projects, and today's timeline of runs
as bars with the now line. `Open console` routes the console to that instance with
`?instance=<id>`; the project selector in the header does the same.

THE INBOX AND NOTIFICATIONS, decided: the stream gains an `attention` frame carrying an
attention entry (escalated, halted, worker death recurred, budget final tenth, stalled
frontier, release-ready, reject recurrence, stale heartbeat) with its instance whenever one
first appears; the page offers to enable browser notifications once, from the inbox, and
after consent shows one notification per new entry naming the project, the unit and the
remedy. Without consent the inbox alone carries them.

THE GUARDED ACTIONS, decided: exactly two `POST` routes, bound to the loopback address, each
requiring the per-serve token the snapshot carried and refusing any request without it:
`/api/actions/resume-unit` (unit, attempts) performs the same library call as `rigger
resume-unit` and records `UnitResumed` with `by: console`; `/api/actions/ruling` (id, summary,
governs, supersedes) performs the same call as `rigger emit DecisionMade`. The page shows the
exact command in a confirmation before sending and toasts the written position after; the
stream then shows the effect. No other action route exists: no pushing, no pull requests, no
run launch or stop, no process signal; every other remedy stays a copyable command chip.

CONSTRAINTS WALK: an instance whose store is unreadable - its card shows the last summary
with "unreachable since <time>" and the reason. A forged action request from another origin
- refused (no token). A resume on a unit that is not escalated - the library's own refusal
text is returned and toasted. A run with no ruling text - Rejects and why lists the round
with "no ruling text recorded". Notifications denied - the inbox is the only channel and says
so once.

## Notes (non-criteria)

The registry, `/api/instances` and the instance routing convention are unchanged. Token
totals are spec 99's recorded usage. Proof is on recorded stream
fixtures and a two-instance registry fixture; the SDET lens records the manual walk
(briefing prose, fleet board, a real notification, both actions) as evidence, as in spec 94.

## Global constraints

- Hyphens, never em dashes. Both feature lanes and the core lane green (fmt, clippy -D
  warnings, test); no-os-kill and reap audits green.
- No new event type; no new crate dependency. The two action routes are the only writes; no
  process control anywhere.
- The operator's installed rigger binary is never replaced or modified by any unit.

## Done when

- [ ] a test proves THE BRIEF: for a recorded stream, `view("brief")` returns the headline,
  metadata, summary sentence and sections assembled from recorded text with the release
  commands on a done run and no sentence the log does not support, and the served page
  renders it with the mock's markup. This criterion OWNS the brief only.
- [ ] a test proves THE FLEET: with two registered instances, `/api/console/fleet` returns a
  summary per instance equal to that instance's own fold, `view("fleet")` renders the strip,
  cards, inbox and timeline with the mock's markup, and `Open console` and the selector route
  to the instance. This criterion OWNS the fleet endpoint and view; the inbox frames are
  criterion 3's, NOT this one's.
- [ ] a test proves THE INBOX AND NOTIFICATIONS: the stream emits an `attention` frame when
  an entry first appears on any instance, the inbox lists it with its remedy, and the page
  requests notification consent once and notifies per new entry after consent. This
  criterion OWNS the attention frames and notifications only.
- [ ] a test proves THE GUARDED ACTIONS: the two routes perform the CLI-equivalent library
  calls and record the same events, refuse requests without the token, refuse a resume the
  library refuses with its text, the action route table has exactly two entries, and the
  page shows the exact command before sending. This criterion OWNS the actions only.
- [ ] both feature lanes and the core lane green (fmt, clippy, test).
