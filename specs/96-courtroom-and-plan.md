# 96 - Courtroom and Plan

**Goal:** the review record is the richest part of the log - `ReviewFinding` per lens,
adversary and adjudicator `DecisionMade` records, `UnitFailed` with cause `reject` - and no
surface reads it as a case: which findings a round raised, which the adversary upheld and
refuted, what the adjudicator ruled and why, and whether a unit's third reject is the same
cause as its first through a different path. The spec-89 check-in stage spent six attempts on
three seam defects an operator had to diagnose by reading raw events. Likewise the run's
shape - the unit DAG, its critical path, how fast units land, which gates are green on the
tree - is folded nowhere. Mission Control's Courtroom and Plan
(docs/architecture-addendum-mission-control.md, sections 5.3 and 5.5) are those two
surfaces.

## Design

ROUNDS ARE DERIVED, decided: a round is keyed by the attempt number in the spawn id
(`u90c2/lens:sdet#1` is round 2); findings, stances and rulings belong to the round of the
spawn that recorded them (`meta.spawn`); the round's outcome is the log's - `UnitFailed`
with cause `reject` closes a round rejected, the unit reaching `reviewed` closes it
approved - and the adjudicator's ruling text is the `DecisionMade` that spawn wrote. No new
event carries a round number.

THE BOARD, decided: `view("court", unit, round)` returns the unit tabs, the round row (each
round's button carrying its outcome), one card per `ReviewFinding` of that round (the lens
as its colour stripe, the finding id, the `about` location as a code link, the summary),
each threaded with the adversary's stance where the adversary's record names that finding
id (`UPHOLD` with its text, `REFUTE` with its text, otherwise "adversary has not weighed
in yet"), the verdict card (`APPROVE` in green, `REJECT` in red, the round, the time, the
cause, the ruling text) or, while the round is live, a `DELIBERATING` card carrying the
adjudicator's latest progress line, and the `DecisionMade` records governing the unit with
operator rulings first.

THE FINDING AUDIT, decided: for a unit with two or more rejects, `view("audit", unit)`
groups the rejecting rounds' findings by their code location (file, and function when the
graph resolves the line) and by shared wording (the normalized-token overlap the
simplification audit already computes), and reports each group as one cause reached through
N paths, listing the rounds and findings in it, plus the findings that match no earlier
group as new causes. The Courtroom renders the audit under the board as one line per group.
The audit is a console view only; no CLI command is added.

THE CODE PANE, decided: clicking a finding's location opens a pane under the board with the
file window spec 95's endpoint returns and the line highlighted; the pane header names the
file and the line.

THE PLAN, decided: `view("plan")` returns the unit DAG (nodes with status, reject count and
commit; needs edges; the critical path - the longest chain of unlanded units by needs - marked),
the instruments (units landed of total, ETA to done, rounds and rejects, elapsed, tokens, mean
landed-unit duration), token burn per unit as point series, and the gates table (the latest
verdict of every gate for every unit, blank where none). ETA is the number of remaining
units times the mean duration of landed units less the time each in-flight unit has already
spent, recomputed at every position, and reads "no unit landed yet" before the first landing.
Tokens come from `SpawnResult` usage metadata when the driver recorded it and read "usage not
recorded by this driver" otherwise; the console never estimates them. The page draws the DAG
as SVG with the mock's box, need-edge and critical-path treatments and the sparklines with
the mock's area and line.

CONSTRAINTS WALK: a round with findings and no ruling yet - deliberating card. A ruling
without findings - the verdict card alone with "no findings recorded in this round". An
adversary text naming no finding id - shown under the board as the round's weighing, threads
empty. A unit with one reject - no audit section. A DAG with a cycle in `needs` - the plan
shows the cycle as an error line, never hangs. A unit whose landing predates the run's
first spawn - duration blank, excluded from the mean.

## Notes (non-criteria)

The location resolution for the audit uses the graph's line index when present and the raw
`file:line` otherwise. Proof is on recorded stream fixtures shaped like the spec-89 check-in
history (six attempts, three causes); the SDET lens records the manual walk of both views as
evidence, as in spec 94.

## Global constraints

- Hyphens, never em dashes. Both feature lanes and the core lane green (fmt, clippy -D
  warnings, test); no-os-kill and reap audits green.
- No new event type; no new crate dependency. Read-only over the repository and the store.
- The operator's installed rigger binary is never replaced or modified by any unit.

## Done when

- [ ] a test proves THE BOARD: for a recorded stream, `view("court", unit, round)` keys
  findings, stances and the ruling to their rounds by spawn attempt, threads a stance to the
  finding it names, returns the verdict card with the log's outcome and the ruling text,
  returns the deliberating card while a round is live, and lists the governing decisions
  operator-first; the served page renders the tabs, round row, cards, threads and verdict
  with the mock's markup. This criterion OWNS the board model and markup; the audit is
  criterion 2's and the code pane criterion 3's, NOT this one's.
- [ ] a test proves THE FINDING AUDIT: for a fixture with three rejecting rounds where two
  share a cause, `view("audit", unit)` reports one cause through two paths and one new
  cause with their rounds and findings, returns nothing for a unit with fewer than two
  rejects, and the Courtroom renders one line per group. This criterion OWNS the audit only.
- [ ] a test proves THE CODE PANE: a finding's location link opens the pane with the file
  window and the highlighted line from the code endpoint, and the pane names the file and
  line. This criterion OWNS the pane markup only; the endpoint is spec 95's, NOT this one's.
- [ ] a test proves THE PLAN: `view("plan")` returns the DAG with statuses, needs, the
  critical path and a cycle as an error line, the instruments with the stated ETA rule and
  its pre-landing sentence, the token series with the not-recorded sentence when usage is
  absent, and the gates table; the served page renders the DAG, instruments, sparklines and
  table with the mock's markup. This criterion OWNS the plan model and markup only.
- [ ] both feature lanes and the core lane green (fmt, clippy, test).
