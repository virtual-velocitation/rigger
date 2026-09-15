# 95 - Theater, Agents and the dock

**Goal:** the operator's first three questions during a run - where is each unit, what is
each agent doing right now, what happens next and what needs me - have no surface. `rigger
status` prints the frontier and blockers as lines; the dashboard's run tree shows statuses.
Neither shows a unit moving through Plan, Build, Review and Integrate, an agent's own account
of its work (its prompt, its session turn by turn, the gate evidence it produced, its diff,
its result), or the next steps of the run. Mission Control's Theater, Agents and dock
(docs/architecture-addendum-mission-control.md, sections 6.1, 6.2 and 6.8) are those
surfaces, rendered from the console core's view models at the scrubber's position, over the
session records spec 99 puts in the log.

## Design

THE THEATER, decided: `view("theater")` returns one lane per unit (plus the plan lane and
the drive lane) with four phase cells - Plan, Build, Review, Integrate. A cell holds the
agent cards of the spawns whose stage and persona fall in that phase (plan and plan-critique
personas in Plan; implementer and SDET author in Build; lenses, adversary and adjudicator in
Review), each card showing the persona name, its activity, the age since its last heartbeat,
and a state: working (pulsing dot), stale (grey dot, age past the run's liveness bound), done
(faint). The activity is the latest progress line, or, when a transcript turn is newer, that
turn as an activity phrase: `running <command>` for a shell call, `editing <file>` for an
edit, `reading <file>` for a read, `searching <pattern>` for a search, the text's first
sentence otherwise. The Review cell carries the round pile - one chip per completed round
with its outcome (`r1 x`, `r2 ok`). The Integrate cell shows `merged . <commit>` for a landed
unit and `approved . integrating` between the ruling and the merge. The lane subtitle states
the unit's status: `waiting on <needs>`, `ready to build`, `building . round N`, `reviewing .
round N`, `landed <time> . <duration>`, or `escalated`. The drive lane lists every
`StepTaken` as a badge, the current one highlighted, with the last step's time. The active
cell is tinted. Clicking a card selects that agent and switches to Agents.

THE AGENTS VIEW, decided: `view("agent", id)` returns the transcript of one spawn in order:
the prompt turn (persona, task text, the criterion; the full prompt behind a disclosure,
identical to `rigger prompt <id>`); one turn per `TranscriptTurn` - text as paragraphs, a
tool call as the tool chip with its command or its tool name and target, a tool result as a
terminal block (test-runner lines coloured pass and fail, other output verbatim, a truncated
block naming its omitted bytes), an edit as a diff block under the file's header (old text
as removed lines, new text as added lines), a progress line as a highlighted turn at its
time; each `GateVerdict` of the unit recorded during the spawn as a terminal block at its
time; the diff reference (the unit's `worktree_sha` from the latest `UnitStatus` during the
spawn); the findings and rulings whose `meta.spawn` is this id at their times; and the result
turn (the `SpawnResult` output and its usage totals) - or, while working, a live turn that
grows as the stream's `turn` frames arrive. The page fetches a spawn's turns through the
transcript route when the agent is opened and follows the live ones with `follow=`. The list
on the left groups every spawn by unit, newest first, marking the working ones. The header
carries the spawn id, the persona, done with the duration or working with the age since the
last turn, and the tokens spent. A `Follow live` toggle keeps the newest turn in view.

THE CODE ENDPOINT, decided: `GET /api/code?diff=<unit>&sha=<worktree_sha>` returns the
unit's diff between its base and that sha, resolving the sha through the unit's lineage ref
(the durable `rigger/lineage/...` refs spec 88 records) when it is no longer on a live
branch; `GET /api/code?file=<path>&line=<n>` returns a window of that file at the run branch
with the line marked. Both are read-only and refuse paths outside the repository.

THE DOCK, decided: `view("dock")` returns three sections. Next: the run's next steps derived
from the fold - the adjudicator ruling on a unit in review, an implementer finishing a
round, a rejected unit re-spawning for its next round, a blocked unit unblocking when its
needs land, the capstone running, release-ready - the first one marked current. Needs you:
every ledger attention entry (escalated, halted, worker death recurred, budget final tenth,
stalled frontier), plus release-ready, reject recurrence (two or more rejects on an open unit),
stale heartbeats and a session the ingester could not find; each item with its exact command
chips, and when the list is empty the sentence "Nothing needs a human right now. Walk away -
you will be tapped." Run: spec, base, steps, agents, decisions, findings, the latest decision
id and time. The guarded buttons on items are spec 97's; this spec renders the commands as
copyable chips.

CONSTRAINTS WALK: a spawn with neither progress lines nor turns yet - the card reads
`grounding` from the spawn time, and the ingester's not-found record surfaces if the session
never appears. A spawn without a result at the head - working; at a replay position past a
recorded result - done. A unit escalated - its lane turns critical and its item appears in
Needs you with `rigger resume-unit`. A diff whose sha has left every live branch - resolved
through the lineage ref; a sha absent from lineage too is a store fault surfaced on the
health strip, never a blank pane.

## Notes (non-criteria)

Phase membership by persona is read from the definition's stage names and the spawn id's
role segment; a persona the mapping does not know renders in the stage's own column named by
the stage. The view models are proven on recorded stream and transcript fixtures at several
positions; the served page is asserted to render them with the mock's markup, and the SDET
lens records the manual walk as evidence, as in spec 94.

## Global constraints

- Hyphens, never em dashes. Both feature lanes and the core lane green (fmt, clippy -D
  warnings, test); no-os-kill and reap audits green.
- No new event type; no new crate dependency. Read-only over the repository and the store.
- The operator's installed rigger binary is never replaced or modified by any unit.

## Done when

- [ ] a test proves THE THEATER: for a recorded stream with transcripts, `view("theater")`
  at a mid-review position returns lanes whose Review cells hold the working lens cards with
  transcript-derived activity phrases and ages, at a position after a ruling returns the
  round pile with that outcome, after a merge returns `merged` with the commit, and the drive
  lane lists every `StepTaken`; the served page renders the lanes, cells, cards, pile and
  drive lane with the mock's markup. This criterion OWNS the theater model and markup; the
  agent transcript is criterion 2's, NOT this one's.
- [ ] a test proves THE TRANSCRIPT: `view("agent", id)` returns, in order, the prompt turn,
  every transcript turn rendered by block kind (text, tool chip, terminal block, diff block,
  progress), the gate evidence blocks at their times, the diff reference, the records the
  spawn wrote and the result turn with usage, with the live turn while the spawn is working;
  the served page fetches the turns through the transcript route, follows live turns, and
  renders them with the follow toggle. This criterion OWNS the transcript model and markup;
  the code endpoint is criterion 3's, NOT this one's.
- [ ] a test proves THE CODE ENDPOINT: the diff form returns the unit's diff for a recorded
  worktree sha and for a sha reachable only through its lineage ref, the file form returns a
  window with the marked line, and both refuse paths outside the repository. This criterion
  OWNS the endpoint only.
- [ ] a test proves THE DOCK: `view("dock")` returns the next steps in the stated order for
  a recorded stream at several positions, the needs-you items with their command chips
  including reject recurrence, stale heartbeats and a session not found, the empty sentence
  when nothing needs a human, and the run block; the served page renders the three sections
  with the mock's markup. This criterion OWNS the dock; the guarded buttons are spec 97's,
  NOT this one's.
- [ ] both feature lanes and the core lane green (fmt, clippy, test).
