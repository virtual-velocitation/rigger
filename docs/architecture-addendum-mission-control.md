# Architecture addendum: Mission Control

**Intent.** The console a person keeps open while rigger drives a project: what every agent is
doing right now, what happens next, what needs a human, why a unit was rejected, and the map of
the codebase being changed - for every project the machine is running, at any moment of a run,
not only its head. It is one browser page that rigger serves itself. Everything on it is folded
from the event log by the same Rust that drives the loop, compiled to WebAssembly and run in the
page, so the console can never disagree with the conductor or with `rigger status`. It replaces
the dashboard; the dashboard's knowledge-graph panel becomes the console's Knowledge tab.

The visual contract is the Rigger Mission Control mock, artifact
`54f8451e-ca21-45a5-bab7-575e57451b20` (approved 2026-09-06 as the goal). Where this addendum
and the mock differ, the mock's appearance wins and this addendum's data rules win: the console
looks like the mock and shows only what the log can prove.

## Problem

The loop already records everything: every spawn, every progress line, every gate verdict,
every finding, stance and ruling, every merge. What is missing is a surface that turns that
record into the four things a person actually asks while a run is live. Five gaps, each with a
concrete anchor in today's tree:

- **No surface answers the operator's questions.** `rigger status` prints the frontier and the
  blocker lines; the dashboard (`src/dash.rs`, 11,240 lines, and `src/dash.html`, 2,398 lines)
  renders a run tree, gate metrics and a graph panel. Both are snapshots of *state*. Neither
  says what an agent is doing at this second, what the next three steps of the run are, what
  needs a human and with which command, or why round 4 was rejected when round 3 fixed the
  named defect. On this project's own runs the operator ends up polling the event store with
  hand-written scripts to learn that a verdict landed or a sweep finished.
- **No time.** The log is append-only and complete, yet every surface shows only its head. A
  post-mortem of a six-attempt unit is done by re-reading event dumps by position. The
  question "what did the theater look like when round 2 was rejected" has an exact answer in
  the log and no way to see it.
- **Three folds of one log.** `rigger status` folds the log (`ledger::project`,
  `blocker::from_state`), the dashboard folds it again (`build_state`), and every operator
  script folds it a third time. The dashboard's release-ready line is kept equal to status's by
  a test *because they drifted once*. Every new surface written in another language is another
  fold that can drift.
- **The graph in the browser is a second implementation.** The page reimplements
  neighborhoods, layouts and label placement in JavaScript over payloads the server computes,
  while `contextgraph` already answers the same questions in Rust (neighborhood, card, path,
  communities). Two implementations of one query set drift, and the JavaScript one is the
  slower and the less proven of the two at this repository's real scale (21,394 entities before
  spec 86, about 9,000 after).
- **One machine, many projects, no attention routing.** The dashboard is already a
  fixed-address machine singleton with an instance registry (`/api/instances`), but an
  escalation, a release-ready run or a stalled frontier on any project is discovered by looking
  at it. A person should be able to walk away and be tapped.

## The console

### 1. One page, seven views

```
  +---------------------------------------------------------------------------------------+
  | * Rigger Mission Control   [project v]   run 0f85a1f5 . spec 90 - hermetic ...  [Theme] [^K] |
  +---------------------------------------------------------------------------------------+
  | Fleet  Theater  Agents  Courtroom  Knowledge  Plan  Briefing     o heartbeats 2 live      |
  |                                                                  o frontier step#12      |
  |                                                                  o no churn  o dash o store|
  +---------------------------------------------------------------------+-----------------+
  |                                                                     | NEXT            |
  |                              the view                               |  1 Adjudicator  |
  |                                                                     |    rules on C2  |
  |                                                                     |  2 C3 unblocks  |
  |                                                                     | NEEDS YOU       |
  |                                                                     |  (empty: walk   |
  |                                                                     |   away)         |
  |                                                                     | RUN             |
  |                                                                     |  spec 90 . base |
  +---------------------------------------------------------------------+-----------------+
  | (>)  |----+----+----+----o======================================|   LIVE 12:41 . 3146190 |
  |      09:00     10:00     11:00       ^reject  ^approve  |integrate                       |
  +---------------------------------------------------------------------------------------+
  | statusline > U90 . checkin review r1 . 3/3 units . healthy . step#12 4m ago      live |
  +---------------------------------------------------------------------------------------+
```

The shell is a fixed grid: header, tab bar with the health strip, the view beside the dock, the
time scrubber, the statusline. Each view answers one question; the dock answers "what next and
what needs me" on every view.

| View       | The question it answers                                  | Folded from                                   |
|------------|-----------------------------------------------------------|-----------------------------------------------|
| Fleet      | What is every project on this machine doing?              | the registry, one small fold per instance     |
| Theater    | Where is each unit in Plan / Build / Review / Integrate?  | units, spawns, progress, verdicts, merges     |
| Agents     | What is this agent doing, and what has it done?           | one spawn's prompt, progress, gates, result   |
| Courtroom  | Why was this round approved or rejected?                  | findings, stances, verdicts, rulings per round|
| Knowledge  | What does the code look like, and what is changing?       | the graph, plus the run's blast radius        |
| Plan       | How far along is the run, and how fast is it going?       | the unit DAG, gates, durations, rounds        |
| Briefing   | Tell me where things stand, in prose.                     | the whole fold, rendered as sentences         |

### 2. Everything is a fold of the log, at a position

The console has exactly one model: **the state of the run after the first N events**. The time
scrubber moves N. The live head is N = the store's head, and new events move it forward; a
replay is N moving forward on a timer; a scrub is any N. Every view renders from the same state
struct, so the theater, the courtroom and the briefing at position N are three renderings of
one fact, never three folds.

```
   event log (run stream)         progress store          liveness markers (disk)
   RunStarted .. UnitProposed ..  per-spawn "doing"       per-spawn heartbeat mtime
   SpawnRequested .. GateVerdict  lines with time         (live head only)
   ReviewFinding .. DecisionMade
   UnitIntegrated ..
          |                              |                        |
          v                              v                        v
   +----------------------------------------------------------------------+
   |  fold(events[..N], progress[..t(N)], ages)  ->  ConsoleState          |
   |     units (ledger::RunState)   agents (spawn + progress + result)     |
   |     court (findings, stances, verdicts by unit and round)             |
   |     plan (DAG, gates, durations, ETA)     attention (needs-you items) |
   |     brief (sentences)   health (signals)   next (steps)   statusline  |
   +----------------------------------------------------------------------+
          |            |            |            |            |
       Theater      Agents     Courtroom       Plan       Briefing  ... Dock, strip, statusline
```

Only **console events** count toward N: the run-lifecycle types (`RunStarted`,
`UnitProposed`, `UnitStarted`, `UnitStatus`, `UnitIntegrated`, `UnitFailed`, `UnitEscalated`,
`UnitResumed`, `SpawnRequested`, `SpawnResult`, `GateVerdict`, `ReviewFinding`,
`DecisionMade`, `LessonLearned`, `BlastRadiusComputed`, `FileTouched`,
`DefinitionSuperseded`, `BudgetExhausted`). The graph-extraction types that share the stream
(`CodeEntityExtracted`, `EdgeInferred`, `DocLinkExtracted`, `DocConceptExtracted`) are the
knowledge graph's and reach the console only as the graph payload; they never appear on the
scrubber. A run of this repository's size has a few thousand console events and two million
graph events, so this filter is what makes "the state at every position" cheap.

Times come from each event's `recorded_at`. At the live head, an agent's age is its heartbeat
marker's age; at a replay position, its age is the time since its last progress line before
that position, so a replay shows the same staleness the operator would have seen then.

### 3. The core runs in the browser as WebAssembly

**Why.** The fold above already exists in Rust and is the conductor's own: `ledger::project`,
`blocker::from_state`, `progress::consolidate`, `spawn::step_result`, `metrics::project`, the
run tree in `dash.rs`, the `contextgraph` queries, the community and concept derivations. None
of those modules touches a file, a socket or a process (each imports no `rusqlite`, `tokio`,
`std::fs`, `std::process` or `std::net`); they are pure functions over `Event` slices and the
`Graph`. Compiling them to WebAssembly and running them in the page makes the console's fold
*the same code* as the conductor's, not a translation of it. That is the only way the fourth
fold cannot drift: it is not a fourth fold.

**What runs in the core.** A `core` build of the library containing the pure modules and one
new module, `console`, that holds the view models (theater lanes, agent transcripts, courtroom
boards, plan instruments, briefing sentences, next steps, needs-you items, health signals, the
statusline) and the map engine spec 84 defines (districts, degree rank, semantic zoom budget,
label placement, hit testing). The same `console` module serves `rigger status` (the
statusline and the needs-you lines are printed by the CLI from the same functions), so the
terminal and the page are one authority too.

**What stays in JavaScript.** Only what a browser must do: the DOM (building the shell and the
view markup from the core's JSON), the canvas draw calls for the map (the core returns a draw
list: circles, labels, edges with screen coordinates), input (scroll, drag, click, keys), the
event stream connection, and the theme. The page holds no model of the run and no graph
algorithm; if a number is on screen, the core produced it.

```
   +---------------------------- the served page ------------------------------+
   |  JavaScript (thin)                     |  console core (.wasm, Rust)        |
   |  - shell, tabs, dock, scrubber DOM     |  fold_reset(events)  fold_push(e)  |
   |  - EventSource /api/console/stream     |  fold_at(N) -> ConsoleState json   |
   |  - canvas: draws the returned list     |  view(name, params) -> json        |
   |  - input -> core calls -> re-render    |  graph_load(payload)               |
   |  - theme, fonts, palette, toasts       |  graph_query(kind, params)         |
   |                                        |  map_build(w,h)  map_frame(cam,sel)|
   |                                        |  map_hit(x,y)    statusline()      |
   +----------------------------------------+------------------------------------+
                   ^ JSON strings across a two-function ABI (alloc/call), no glue crate
```

**The ABI.** Three exported functions - `console_alloc(len)`, `console_free(ptr, len)`,
`console_call(op_ptr, op_len, in_ptr, in_len) -> u64` (packed pointer and length of a JSON
reply) - and one global `Console` state inside the module (WebAssembly in a page is
single-threaded, so the state needs no lock). Every request and reply is JSON through
`serde_json`, which the crate already depends on. No binding-generator crate is added and no
JavaScript is generated: the whole page-side ABI is thirty lines of loader.

**The build.** A workspace member `crates/console-core` is a `cdylib` that depends on the
library with `default-features = false, features = ["core"]`. The library's I/O dependencies
(the SQLite and server event-store backends, the async runtime, process and filesystem
helpers) move behind a `store` feature that stays in `default`, so today's two lanes build
exactly what they build now and a third lane, `--no-default-features --features core`, builds
the pure subset for any target. The main crate's `build.rs` compiles the member for
`wasm32-unknown-unknown` in release mode into `OUT_DIR` (a nested cargo invocation with its
own target directory, so it never contends for the outer build's lock), and the binary embeds
the result with `include_bytes!`; the page loads it from `/console/core.wasm`. The nested
build reruns only when a core module changes. The target is an operator install
(`rustup target add wasm32-unknown-unknown`, named by the build error when it is missing) and
a workflow-level install in CI - never something a unit installs.

**Budget.** The module stays under 3 MB uncompressed; instantiation under 200 ms on the
operator's machine; a fold of 10,000 console events under 16 ms (one frame), so a scrub is
never perceptibly late. A frame of the map at full extent stays under 8 ms of core time.

### 4. The data plane

```
   browser page                              rigger (the dash singleton, one process)
   -------------                             --------------------------------------
   GET /                                --->  the console page (shell + JS + font links)
   GET /console/core.wasm, /console/fonts/* -> embedded assets from the binary
   GET /api/console/snapshot?instance=  --->  { run_id, spec, base, console events of the
                                               current run, progress lines, liveness ages,
                                               definition (stages, gates, liveness bound),
                                               head position }
   GET /api/console/stream?since=N      --->  text/event-stream: one frame per new console
                                               event (subscribe_all on the store), one
                                               `progress` frame per new progress line, one
                                               `liveness` frame every 5 s with marker ages,
                                               a `heartbeat` frame every 15 s
   GET /api/graph?payload=map           --->  the whole graph for the map (entities, kinds,
                                               files, typed edges, communities, concepts,
                                               proof counts), cached per index stamp
   GET /api/code?file=&line=            --->  a window of the file at the run branch
   GET /api/code?diff=<unit>&sha=       --->  git diff of the unit's worktree sha vs its base
   GET /api/instances                   --->  the registry (unchanged)
   POST /api/actions/resume-unit        --->  guarded action (section 6)
   POST /api/actions/ruling             --->  guarded action (section 6)
```

The server does no folding for the console. Its job is to hand the page the raw materials
(events, progress, ages, the graph payload, code windows) and to keep the stream open. The
existing `/api/state` and `/api/graph` routes stay for the transition and for external
readers; the console does not read `/api/state`.

Three sources feed the fold: the run stream (the event store), the progress store (the
separate store `rigger progress` appends to, never the run stream) and the liveness markers
(files whose modification time is the heartbeat). The snapshot carries all three at once so
the page renders before the stream connects; the stream then carries deltas only. A dropped
stream reconnects with `since=` the last position received and is shown on the health strip
(`dash` turns amber while disconnected); the page never fabricates events while offline.

### 5. The views

Every view has an **honest empty state**: a sentence that names what the log does not hold
and, where a command produces it, the command. No view fabricates a number, a transcript line
or an estimate it cannot derive.

#### 5.1 Theater

```
   UNIT                    PLAN          BUILD                REVIEW                INTEGRATE
   ------------------------------------------------------------------------------------------
   Plan & critique         [Plan  done]                                             [DAG approved]
   u90c1 . runner hermetic               [Implementer done]   [SDET done][Arch done] [merged . 5df3c3d]
                                          [SDET-Author done]   [Adversary done]
                                                               [Adjudicator done]    r1 ok
   u90c2 . line-free guard               [Implementer done]   [Lens:SDET o working]  r1 x  r2 x
                                          [SDET-Author done]   [Lens:Arch  o 2m]
   u90c3 . lanes green     waiting on u90c2
   Drive                                  step#1 .. step#12  (last step 4m ago)
```

One lane per unit plus the plan lane and the drive lane; four phase cells. An agent card shows
its persona, its latest progress line, its age since the last heartbeat and a pulsing dot while
working (grey when stale, faint when done). The Review cell carries the round pile (`r1 x`,
`r2 ok`). The active cell is tinted; a landed unit's Integrate cell shows the commit. The
frontier at position N is the set of spawns without a result at N. Clicking an agent card
opens it in Agents.

Fold inputs: `UnitStarted`/`UnitProposed` (lanes, needs), `SpawnRequested`/`SpawnResult`
(cards, phases from the stage and the persona), progress lines (the card text), `GateVerdict`
(the gate badges), `UnitFailed` with cause `reject` and the adjudicator's verdict ruling (the
round pile), `UnitIntegrated` (merged), `UnitEscalated` (the lane turns critical).

#### 5.2 Agents

Left: every spawn of the run grouped by unit, newest first, the working ones marked. Right:
the selected agent's transcript, built only from what the log holds, in order:

1. **Prompt** - the spawn's task text and persona name from `SpawnRequested` (the full prompt
   behind a disclosure, the same text `rigger prompt <id>` prints).
2. **Progress turns** - each progress line with its time.
3. **Gate evidence** - each `GateVerdict` of the unit during this spawn, rendered as a
   terminal block (pass in green, fail in red, the evidence text verbatim).
4. **The diff** - the unit's worktree diff at the spawn's last recorded `worktree_sha`
   (`UnitStatus` meta), fetched on demand from `/api/code?diff=`.
5. **Findings, stances and rulings this agent recorded** - `ReviewFinding` and `DecisionMade`
   whose `meta.spawn` is this id.
6. **Result** - the `SpawnResult` output, or a live "working" turn showing the latest progress
   line with a pulsing marker.

The tool-by-tool transcript of the agent's editor session is not held by the log and is not
shown; the header says so ("the log holds the agent's reports, gates and records, not its
keystrokes"). A `Follow live` toggle keeps the newest turn in view at the head.

#### 5.3 Courtroom

```
   [u90c1] [u90c2] [u90c3]     round 1 x   round 2 x   round 3 o          line-free audit guard
   +------------------------------------------+-----------------------------------------------+
   | Lens:SDET   sdet-u90c2-r3-...  file:line | Lens:Architecture  arch-u90c2-r2-...  file:line|
   | The report still cites live line numbers | render_section_1 reads MapEntry fields ...    |
   |  UPHOLD  Reproduced by a pin-bump probe  |  UPHOLD  Same defect from the code side ...   |
   +------------------------------------------+-----------------------------------------------+
   | REJECT   Adjudicator . round 2   00:31 . cause: genuine-defect                            |
   | Round 2 delivered section 2 only; sections 1, 4.3 and 6 still read live lines ...        |
   +--------------------------------------------------------------------------------------------+
   Decisions governing this unit:  op-u90c2-round-3-delivers-the-whole-line-free-clause ...
   [code pane: tests/simplification_audit.rs around line 1802]
   Finding audit: 2 rejects . same cause through 2 paths (report rendering) . 0 new causes
```

A unit tab row and a round row (each round's button carries its outcome dot). The board holds
one card per `ReviewFinding` of that unit and round (the lens as the colour stripe, the finding
id, the `about` location as a link that opens the code pane), threaded with the adversary's
stance where the adversary's record names that finding id (uphold or refute, with its text),
then the verdict card: the round's outcome from the log (`UnitFailed` cause `reject`, or the
unit reaching `reviewed`) with the adjudicator's ruling text and cause, or a "deliberating"
card with the adjudicator's latest progress line while the round is live. Below: the
`DecisionMade` records that govern the unit (operator rulings first), and the **finding
audit**: for a unit with two or more rejects, the core groups the rejecting rounds' findings
by the code location and the wording they share and reports whether the rejects are one cause
reached through different paths or genuinely new defects - the question an operator must
answer before spending another round.

Rounds are derived, not recorded: a spawn id's attempt number (`u90c2/lens:sdet#1` is round
2) keys findings, stances and verdicts to their round.

#### 5.4 Knowledge

The three lenses of the knowledge-graph inspector, unchanged in their taxonomy: Code (the
labelled map of spec 84: districts named by purpose, semantic zoom, always-labelled
entities, typed directed edges, the explore rail, the card with Called by / Calls / File /
Proof / Concepts / Memory chips), Files, Concepts. The console adds exactly one thing: the
**run-activity overlay**, which at the scrubber's position lights every entity inside an
in-flight unit's blast radius (`BlastRadiusComputed` precise and safe file lists, plus
`FileTouched` since the unit started) with the amber ring the legend names, and pulses the
files a building unit is editing. The Memory chips hand off to the Courtroom (a finding chip
opens its unit and round). The map engine runs in the core; the page draws.

#### 5.5 Plan

The unit DAG (needs edges, the critical path - the longest chain of unlanded units - drawn in
the accent colour, each box coloured by status and captioned with its status, reject count and
commit), the instruments (units landed, ETA to done, rounds and rejects, elapsed, tokens,
mean landed-unit duration), token burn per unit as sparklines, and the gates table (the latest
verdict of every gate for every unit). ETA is remaining units times the mean landed-unit
duration less the time each in-flight unit has already spent, recomputed at every position;
before any unit has landed it reads "no unit landed yet". Tokens render from `SpawnResult`
usage metadata when the driver recorded it and read "usage not recorded by this driver" when
it did not - a blank, never an estimate.

#### 5.6 Briefing

Prose generated from the fold at the cursor: a headline (release-ready, or where things stand
at the cursor's time), a metadata line, one summary sentence, then Landed (each unit with its
time, commit and reject count), Rejects and why (the first sentence of each rejecting ruling),
Decisions taken, What happens next (the same steps the dock shows), and, on a done run, the
release commands `ledger::ReleaseReady` already produces for `rigger status`. Every sentence is
assembled from recorded text; the page adds no adjectives.

#### 5.7 Fleet

One board for every registered instance on the machine: a strip of totals (projects, live
runs, agents working, needs-you count, tokens today), a card per project (spec, run, a unit
progress bar coloured by status, landed / agents / ETA / tokens, the five health signals, a
spawn-rate sparkline, and `Open console`), the needs-you inbox across projects, and a timeline
of today's runs as bars against the clock with a "now" line. A project's card is the same fold
rendered small: the singleton folds each registered instance's store through the same core
(it runs the core natively for this), so a number on a card equals the number on that
project's console. Opening a card routes the console to that instance (`?instance=<id>`, the
convention the API already uses). Attention entries from every project reach the inbox and,
when the person has allowed it once, a browser notification, so nobody watches a board to be
tapped.

#### 5.8 The dock, the health strip and the statusline

**Next** is a numbered list of the run's next steps derived from the fold (the adjudicator
rules on a unit in review; an implementer finishes a round; a rejected unit re-spawns for its
next round; a blocked unit unblocks when its needs land; the capstone runs; release-ready) -
the current one highlighted. **Needs you** is the attention list: every `AttentionEntry` the
ledger already computes (escalated, halted, worker death recurred, budget final tenth, stalled
frontier), plus release-ready, reject recurrence (two or more rejects on an open unit) and
stale heartbeats; each item carries the exact command chips (copyable) and, for the two
actions the console may perform, a guarded button. When the list is empty it says so:
"Nothing needs a human right now. Walk away - you will be tapped." **Run** is the key-value
block (spec, base, steps, agents, decisions, findings, the latest decision).

The **health strip** shows five signals as coloured dots: heartbeats (working spawns, amber
when any is past the run's liveness bound), frontier (amber when the last step is older than
the bound), churn (amber when any open unit has three or more rejects), dash (the stream
connection), store (the server's last successful read), plus the token chip.

The **statusline** is one line: unit focus, review round, units landed, health word, step age,
live or replay - the same text `rigger status` prints first, produced by the same core
function.

#### 5.9 The scrubber, replay and the palette

The scrubber's range is the run's console events; its marks are verdicts (red reject, green
approve), integrations (accent, taller) and the plan approval, each with a tooltip; its ticks
are wall-clock hours. Dragging scrubs; the play button replays from the cursor at five events
per second; space toggles play; the arrow keys step one event; `LIVE` shows at the head with
the head time and position, and a replay position shows how far before live it is. The URL
carries the view, the selection and the position (`#/court/u90c2/2?at=3146150`), so a moment
is shareable. The command palette (`Ctrl/Cmd-K`) jumps to a view, a unit's courtroom, an agent
or a round, to live, or to a replay from the start; the digits 0-6 switch views.

### 6. Guarded actions

The console is a reader that may perform two writes, both of which the CLI performs today
with the same library calls, both recorded on the log by the same code path:

- **Resume a unit** - `POST /api/actions/resume-unit` with the unit and an attempt count,
  equal to `rigger resume-unit <unit> --attempts N`; the log gains `UnitResumed` with
  `by: console`.
- **Record a ruling** - `POST /api/actions/ruling` with id, summary, governs and supersedes,
  equal to `rigger emit DecisionMade`; the log gains the `DecisionMade`.

Each action is bound to the loopback address, carries a per-serve token the page received in
its snapshot (a cross-site page cannot forge it), shows the exact command it is about to
perform in a confirmation, and reports the position of the event it wrote; the stream then
shows the effect like any other event. Every other item in the needs-you list is a copyable
command, never a button: the console does not push branches, open pull requests, start or
stop runs, or touch a process (there is no stop-agent action anywhere, by the no-OS-kills
rule). Launching runs belongs to the world authority described in its own addendum.

### 7. Look and feel

The mock's tokens are the console's tokens, verbatim, in both themes:

```
   ground     --bg #F2F5F7      surface #FFFFFF   surface2 #E9EEF2   line #D3DCE3 / #BFCAD3
   ink        --ink #17232C     ink2 #4A5A66      muted #7C8B97      faint #A9B6C0
   accent     --accent #B86F2E  (copper)          accent-soft rgba(184,111,46,.14)
   status     good #2E8B57  warn #B7791F  crit #C7423B  review #6D5DD3  stale #9AA7B1
   terminal   term-bg #0F171E   term-ink #D6E0E8
   dark       bg #0C141B  surface #131D26  ink #E4ECF1  accent #D08A48  (the mock's dark set)
```

Three faces: Sora for the interface (tabs, labels, headings), Source Sans 3 for reading text,
JetBrains Mono for identifiers, times and terminals, with `tabular-nums` wherever digits
align. The faces are embedded in the binary as Latin-subset woff2 files (all three are
published under the SIL Open Font License, whose text ships beside them) and served from
`/console/fonts/`, so the page loads nothing from the network; the fallback stacks are the
mock's. Themes follow the system, with a toggle that persists in the browser; the dark theme
is designed, not inverted. Motion is limited to the heartbeat pulse, the card hover lift and the
scrubber fill, and all of it stops under `prefers-reduced-motion`. Every interactive element
has a visible focus ring in the accent colour.

### 8. The dashboard charter, amended

The dashboard's charter was: no external assets, inline JavaScript in the served page, zero
new dependencies, read-only over existing projections. For the console it becomes:

- **No external assets** - unchanged; the page, its script, the core module and the fonts are
  all served from the binary.
- **Served from the binary** replaces "inline JavaScript": the page may load its own script,
  the core module and font files from the same origin, never from anywhere else.
- **Zero new crate dependencies** - unchanged; no binding generator, no framework, no chart
  library. A build *target* (`wasm32-unknown-unknown`) is added; it is an operator and CI
  install, never a crate.
- **Reads projections; writes only through the guarded action set** replaces "read-only": the
  two actions in section 6, each a CLI-equivalent library call recorded on the log.

### 9. Constraints walk

- **Empty store, no run** - every view shows its empty sentence ("no run recorded; start one
  with `rigger run <spec>`"), the scrubber is disabled, the health strip shows store and dash
  only.
- **A run recorded by an older binary** - events lacking a field the core reads (a
  `worktree_sha`, a `base_tip`) render the honest blank ("no worktree recorded"), never a
  guess; the fold never fails on a missing optional field.
- **A run with no progress lines** - agent cards show "no progress reported" and age from the
  spawn time.
- **The graph payload is large** - it is fetched once per index stamp, cached in the page, and
  the map draws only what the rank budget admits (spec 84); the core never returns the whole
  entity list to the page.
- **Two tabs, two positions** - each tab's URL carries its own position and selection; the
  server holds no per-viewer state.
- **The stream drops** - reconnect with `since=`, amber `dash` signal meanwhile, no
  fabricated events; a snapshot re-fetch reconciles if the gap exceeds the server's retained
  window.
- **An instance goes away** - its Fleet card shows its last fold with an "unreachable since"
  badge; opening it says why.
- **The singleton restarts** - the page reloads its snapshot; the position it was showing is
  in its URL, so a replay survives the restart.
- **A store anomaly** - a projection error is surfaced as a critical `store` signal with the
  error text, never swallowed into an empty view.
- **The core fails to load** - the page shows one sentence naming the module and the build
  step; it never falls back to a JavaScript fold (there is none to fall back to).

## Delivery

Six specs, in this order, plus one amendment. Spec 84 (the labelled map) is amended to build
its layout, ranking and label placement in the core rather than in page script, so it lands on
the core once and moves into the Knowledge tab unchanged; it therefore follows spec 93.

| Spec | Scope |
|------|-------|
| 93 | The console core compiles to WebAssembly: the `store`/`core` feature split of the library, the `crates/console-core` member, the build-time embedding, the three-function ABI, the fold exports proven equal to `rigger status`, the graph query exports |
| 84 (amended) | The labelled map, built on the core: districts, semantic zoom, label placement and hit testing as core functions returning draw lists; the page draws |
| 94 | The console shell and the live data plane: the served page (shell, tabs, dock frame, health strip, statusline, scrubber, palette, theme, embedded fonts), the snapshot and stream endpoints, the position-addressed fold in the page, replay and the shareable URL |
| 95 | Theater, Agents and the dock: lanes and phase cells, agent cards, the transcript view with prompt, progress, gate evidence, diff and result, Next and Needs-you and Run |
| 96 | Courtroom and Plan: the findings board with adversary threads and verdict cards, the code pane, governing decisions, the finding audit; the DAG with critical path, the instruments, token burn, the gates table |
| 97 | Briefing, Fleet and attention: the generated brief, the fleet board over the registry with per-instance folds, the cross-project inbox, notifications, the guarded action set |
| 98 | The Knowledge tab absorbs the inspector: the three lenses in the console with the run-activity overlay and the Memory hand-off, and the old dashboard page retired - `rigger dash` serves the console |

Operator prerequisites, named in spec 93: `rustup target add wasm32-unknown-unknown`; CI
installs the same target at workflow level.

## Invariants

1. **One fold.** The console's state is computed by the library's own projection code
   running in the page; no view derives a number from another view or from the DOM.
2. **Position-addressed.** Every view is a function of (events up to N, progress up to the
   time of N, ages); the live head is only the largest N.
3. **The page never computes what the core can.** JavaScript builds markup and draws lists;
   it holds no run model and no graph algorithm.
4. **Reads projections, writes through the guarded set only.** Two actions, each a CLI
   equivalent recorded on the log; no process control, no git writes, no run launches.
5. **No external assets.** Page, script, core and fonts come from the binary.
6. **Every visible mark is named.** Legends and labels for every class of mark; no unlabelled
   node on the map (spec 84).
7. **Honest blanks.** A value the log does not hold is shown as its absence with the command
   that would produce it, never estimated or fabricated.
8. **The mock is the contract.** Appearance, layout, interaction and wording follow the
   approved mock; data rules follow this addendum.
