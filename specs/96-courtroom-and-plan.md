# 96 - Courtroom and Plan

**Goal:** the review record is the richest part of the log - `ReviewFinding` per lens,
adversary and adjudicator `DecisionMade` records, `UnitFailed` with cause `reject` - and no
surface reads it as a case: which findings a round raised, which the adversary upheld and
refuted, what the adjudicator ruled and why, and whether a unit's third reject is the same
cause as its first through a different path. The spec-89 check-in stage spent six attempts on
three seam defects an operator had to diagnose by reading raw events. The record's shape
also fights the reader: the adversary's stances and the adjudicator's rulings are free-text
`DecisionMade` summaries that name finding ids inconsistently. Likewise the run's shape - the
unit DAG, its critical path, how fast units land, what each unit costs, which gates are green
on the tree - is folded nowhere. Mission Control's Courtroom and Plan
(docs/architecture-addendum-mission-control.md, sections 6.3 and 6.5) are those two
surfaces.

## Design

THE REVIEW RECORD'S SHAPE, decided (definition content, not conductor code): a lens records
one `ReviewFinding` per finding with `about` the code location; the adversary records one
`ReviewFinding` per stance with `about` the finding id it weighs, `by` the adversary, and a
summary beginning `UPHOLD` or `REFUTE`; the adjudicator records its ruling as one
`DecisionMade` whose id is `adj-<unit>-r<N>-verdict-<approve|reject>` and whose summary opens
with the cause as its first clause. The three review personas carry these rules in their
text, and a definition test proves the shipped personas state them. Records written before
this spec render by the finding ids their summaries name.

ROUNDS ARE DERIVED, decided: a round is keyed by the attempt number in the spawn id
(`u90c2/lens:sdet#1` is round 2); findings, stances and rulings belong to the round of the
spawn that recorded them (`meta.spawn`); the round's outcome is the log's - `UnitFailed`
with cause `reject` closes a round rejected, the unit reaching `reviewed` closes it
approved - and the ruling text is the adjudicator's `DecisionMade`. No new event carries a
round number.

THE BOARD, decided: `view("court", unit, round)` returns the unit tabs, the round row (each
round's button carrying its outcome), one card per lens `ReviewFinding` of that round (the
lens as its colour stripe, the finding id, the `about` location as a code link, the summary),
threaded with the adversary's stance record on it (`UPHOLD` with its text, `REFUTE` with its
text, otherwise "adversary has not weighed in yet"), the verdict card (`APPROVE` in green,
`REJECT` in red, the round, the time, the cause, the ruling text) or, while the round is live,
a `DELIBERATING` card carrying the adjudicator's latest transcript activity, and the
`DecisionMade` records governing the unit with operator rulings first.

THE FINDING AUDIT, decided: for a unit with two or more rejects, `view("audit", unit)`
groups the rejecting rounds' findings by their code location (file, and function when the
graph resolves the line) and by shared wording (the normalized-token overlap the
simplification audit already computes), and reports each group as one cause reached through
N paths, listing the rounds and findings in it, plus the findings that match no earlier
group as new causes. The Courtroom renders the audit under the board as one line per group.
The audit is a console view only; no CLI command is added.

THE CODE PANE, decided: clicking a finding's location opens a pane under the board with the
file window spec 95's endpoint returns and the line highlighted; a location naming a spawn
turn opens that turn of the transcript instead; the pane header names the file and the line.

THE PLAN, decided: `view("plan")` returns the unit DAG (nodes with status, reject count and
commit; needs edges; the critical path - the longest chain of unlanded units by needs -
marked), the instruments (units landed of total, ETA to done, rounds and rejects, elapsed,
tokens, mean landed-unit duration), token burn per unit as point series, and the gates table
(the latest verdict of every gate for every unit). Tokens are the recorded usage of each
unit's spawns (spec 99), cumulative over the turns' times. ETA is remaining units times the
mean landed-unit duration less the time each in-flight unit has already spent, recomputed at
every position; the mean is this run's once a unit has landed, this project's across its
earlier runs before that, and the fleet's across the machine's registered instances before
that, and the instrument names which of the three it used. The page draws the DAG as SVG
with the mock's box, need-edge and critical-path treatments and the sparklines with the
mock's area and line.

CONSTRAINTS WALK: a round with findings and no ruling yet - deliberating card. A ruling
without findings - the verdict card alone with "no findings recorded in this round". A stance
on a finding id not in the round - shown under the board as an unattached stance, flagged. A
unit with one reject - no audit section. A DAG with a cycle in `needs` - the plan shows the
cycle as an error line, never hangs. A first project on a machine before any unit has ever
landed - ETA uses the fleet mean when another instance has one and the definition's liveness
bound as the per-unit estimate before that, named as such.

## Notes (non-criteria)

The location resolution for the audit uses the graph's line index when present and the raw
`file:line` otherwise. Proof is on recorded stream fixtures shaped like the spec-89 check-in
history (six attempts, three causes) with the review record in the shape above; the SDET
lens records the manual walk of both views as evidence, as in spec 94.

## Global constraints

- Hyphens, never em dashes. Both feature lanes and the core lane green (fmt, clippy -D
  warnings, test); no-os-kill and reap audits green.
- No new event type; no new crate dependency. Read-only over the repository and the store.
- The operator's installed rigger binary is never replaced or modified by any unit.

## Done when

- [ ] a test proves THE REVIEW RECORD'S SHAPE: the shipped adversary persona instructs one
  `ReviewFinding` per stance with `about` the finding id and an `UPHOLD` or `REFUTE` opening,
  the shipped adjudicator persona instructs the `adj-<unit>-r<N>-verdict-<outcome>` ruling id
  with the cause first, and a fixture round recorded in that shape joins every stance to its
  finding by id. This criterion OWNS the persona rules and the join; the board's rendering is
  criterion 2's, NOT this one's.
- [ ] a test proves THE BOARD: for a recorded stream, `view("court", unit, round)` keys
  findings, stances and the ruling to their rounds by spawn attempt, threads each stance to
  its finding, returns the verdict card with the log's outcome and the ruling text, returns
  the deliberating card while a round is live, and lists the governing decisions
  operator-first; the served page renders the tabs, round row, cards, threads and verdict
  with the mock's markup. This criterion OWNS the board model and markup; the audit is
  criterion 3's and the code pane criterion 4's, NOT this one's.
- [ ] a test proves THE FINDING AUDIT: for a fixture with three rejecting rounds where two
  share a cause, `view("audit", unit)` reports one cause through two paths and one new
  cause with their rounds and findings, returns nothing for a unit with fewer than two
  rejects, and the Courtroom renders one line per group. This criterion OWNS the audit only.
- [ ] a test proves THE CODE PANE: a finding's location link opens the pane with the file
  window and the highlighted line from the code endpoint, a spawn-turn location opens that
  transcript turn, and the pane names the file and line. This criterion OWNS the pane
  markup only; the endpoint is spec 95's, NOT this one's.
- [ ] a test proves THE PLAN: `view("plan")` returns the DAG with statuses, needs, the
  critical path and a cycle as an error line, the instruments with the ETA rule naming its
  mean's provenance at each of its three levels, the token series from recorded usage, and
  the gates table; the served page renders the DAG, instruments, sparklines and table with
  the mock's markup. This criterion OWNS the plan model and markup only.
- [ ] both feature lanes and the core lane green (fmt, clippy, test).
