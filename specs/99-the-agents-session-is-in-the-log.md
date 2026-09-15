# 99 - The agent's session is in the log

**Goal:** a worker records progress lines (`rigger progress`, into the separate progress
store) and a result (`SpawnResult`, whose `meta` today carries only `resolved_model`); its
actual work - the tools it ran, what they printed, the edits it made, the tokens each turn
cost - is written by the editor harness into that harness's own session files, keyed by
nothing rigger knows. So no surface can show an agent's transcript or a token burn, a silent
agent reads as blank, and a post-mortem reads harness files by hand. Nor is a `rigger step`
recorded: the run's step count and the frontier's last movement are inferred from spawn
timestamps. Mission Control (docs/architecture-addendum-mission-control.md, section 3) needs
the session and the steps in the log. The join exists: every spawn's prompt begins with its
spawn id, and the harness writes each sub-agent session as a line-delimited file whose first
message is that prompt.

## Design

TRANSCRIPT SOURCES, decided: a `TranscriptSource` is the one place a spawn's session is
read from; two ship. The editor-session source derives the harness's project directory from
the repository root the way the harness does (the absolute path with separators replaced),
overridable by `RIGGER_TRANSCRIPT_ROOT`, scans its sessions' sub-agent files newest first, and
selects the file whose first message contains the spawn id; a spawn never matches two files
(the id is unique per run) and a file never matches two spawns. The driver source is the
headless driver (`rigger workflow`, `rigger run`), which receives the agent's message stream
from the agent SDK and records turns as they arrive. Each spawn's transcript records which
source produced it.

THE TURN RECORD, decided: the progress store gains one record kind, `TranscriptTurn`, keyed
by (run, spawn, turn index) and carrying the turn's time, role, and blocks: text; a tool call
(tool name and input, an edit's old and new text kept whole, a shell command kept whole); a
tool result (output capped at 32 KB per block with head and tail kept and the omitted byte
count recorded); and the message's usage (input, output, cache-creation, cache-read tokens).
Ingestion is idempotent by key. The run stream gains no type for this; at the spawn's
completion the totals are written into the existing `SpawnResult` event's `meta.usage`
(`input`, `output`, `cache_creation`, `cache_read`, `turns`), so `rigger stats` and the Plan
view read one number.

WHEN, decided: three moments, one code path. While a spawn is live, the dash singleton's
tailer polls each live spawn's session file every two seconds by length and records new
complete lines (a partial trailing line waits). At `rigger result <id>`, the recording call
ingests the file to its end and writes the usage totals. At every `rigger step`, any spawn
with a result and a transcript not marked complete is ingested to its end, so a courier that
died mid-session still leaves a whole record. A spawn whose session the source cannot find
gets one `TranscriptTurn` stating that and the paths searched, and the dock's needs-you list
carries it as a harness fault; nothing is silently absent.

THE STEP RECORD, decided: `StepTaken` is the one new run-stream event type in the Mission
Control set. `rigger step` appends it once per invocation, at the end, with the step's
ordinal, the wave it returned (spawn ids), the courier's identity when known, and whether the
run reached `done`. The frontier signal, the drive lane, the metadata lines and the
stalled-frontier attention read it.

THE STREAM, decided: `/api/console/stream` gains `follow=<spawn>` and two frame kinds: `turn`
(a new `TranscriptTurn` of a followed spawn) and `usage` (a spawn's running totals whenever
they grow); `/api/console/transcript?spawn=<id>&from=<index>` returns a spawn's turns paged by
index.

CONSTRAINTS WALK: the harness directory does not exist (a project driven only by the headless
driver) - the editor source reports no session and the driver source has recorded the turns.
Two sessions carry the same spawn id (a re-driven spawn under the same id) - the newest wins
and the older is recorded as superseded. A 40 MB tool result - capped as stated. A file
rewritten by the harness (compaction) - the tailer detects a shrink and re-ingests from the
recorded turn count by content, never duplicating a key. Crash resume - the recorded turn
counts and completeness marks live in the progress store, so the tailer and the step-time
sweep resume without re-reading whole runs.

## Notes (non-criteria)

The parser is proven against a session recorded from this project's own run (checked in as
a fixture). Token totals of spawns that predate this spec are filled at step time from the
sessions still on disk; usage on `SpawnResult` is `meta`, an existing field. The console
views that render turns are spec 95's.

## Global constraints

- Hyphens, never em dashes. Both feature lanes and the core lane green (fmt, clippy -D
  warnings, test); no-os-kill and reap audits green.
- `StepTaken` is the only new run-stream event type in the Mission Control set; the progress
  store's `TranscriptTurn` is a progress-store record. No new crate dependency.
- The operator's installed rigger binary is never replaced or modified by any unit.

## Done when

- [ ] a test proves THE SOURCES LOCATE A SESSION: with a fixture harness directory holding
  several sub-agent session files, the editor source returns the file whose first message
  carries the spawn id and none other, reports not-found with the searched paths for an
  unknown id, and the driver source records turns from an SDK message stream. This
  criterion OWNS the source abstraction and the two sources; the records are criterion 2's,
  NOT this one's.
- [ ] a test proves THE TURN RECORD: ingesting the fixture session yields one
  `TranscriptTurn` per message with text, tool call, tool result (capped with the omitted
  count) and edit blocks and per-message usage, ingesting twice records nothing new, and the
  spawn's `SpawnResult` carries `meta.usage` totals equal to the sum of its turns. This
  criterion OWNS the record shape, idempotence and the usage totals only.
- [ ] a test proves THE THREE MOMENTS: the tailer records new complete lines of a growing
  file within two seconds and waits on a partial line, `rigger result` ingests to the end
  and writes usage, a step ingests every resulted-but-incomplete spawn, and a missing
  session becomes a not-found turn and a needs-you item. This criterion OWNS the tailer, the
  result-time and step-time ingestion; the sources are criterion 1's, NOT this one's.
- [ ] a test proves THE STEP RECORD: every `rigger step` appends one `StepTaken` with its
  ordinal, wave and done flag, the ledger's stalled-frontier attention reads the last
  `StepTaken`, and the console's step count equals the number of records. This criterion
  OWNS the event and its readers only.
- [ ] a test proves THE STREAM CARRIES TURNS: with `follow=<spawn>`, a newly recorded turn
  reaches a connected client as a `turn` frame within two seconds, `usage` frames carry the
  running totals, and the transcript route pages a spawn's turns by index. This criterion
  OWNS the two frame kinds and the transcript route only.
- [ ] both feature lanes and the core lane green (fmt, clippy, test).
