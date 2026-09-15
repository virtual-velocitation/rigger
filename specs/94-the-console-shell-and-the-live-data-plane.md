# 94 - The console shell and the live data plane

**Goal:** the dashboard page polls `/api/state` every few seconds and re-renders a whole run
tree from a server-side projection; it has no notion of a position in the log, so it can show
only the head, and its 2,398 lines of page script fold and lay out on their own. Mission
Control (docs/architecture-addendum-mission-control.md) is one page whose every view is the
state of the run after N events, moved by a time scrubber, fed by a snapshot and a live stream,
computed by the console core of spec 93. This spec ships that page's shell and its data plane:
the frame every view of specs 95-98 renders into.

## Design

THE PAGE, decided: served at `/` by `rigger dash`, the shell is the mock's, region for region:
a header (brand dot and name, project selector, the run line `run <id> . spec <n> - <title> .
base <branch>`, theme toggle, palette button), a tab bar (Fleet, Theater, Agents, Courtroom,
Knowledge, Plan, Briefing) with the health strip on its right, the view beside a 300 px dock,
the scrubber footer (play button, track with ticks, marks, fill and knob, cursor readout), and
the statusline. Markup classes and CSS custom properties are the mock's verbatim (`--bg`,
`--surface`, `--ink`, `--accent` and the rest, light on `:root`, dark under both the media
query and `[data-theme="dark"]`). Each view region is empty in this spec except for the
sentence naming the spec that fills it.

THE ASSETS, decided: the page's script, the core module and three typefaces - Sora (400, 500,
600), Source Sans 3 (400, 600, 400 italic) and JetBrains Mono (400, 500) - are embedded in the
binary as Latin-subset woff2 files under `src/console/fonts/` with the SIL Open Font License
text beside each family, and served from `/console/`. The served page references no URL
outside its own origin.

THE SNAPSHOT, decided: `GET /api/console/snapshot` returns the current run's identity (run id,
spec path and title, base), the run's console events in position order, its progress lines
with times, the liveness ages by spawn id, the definition's stage and gate names and liveness
bound, the head position and a per-serve action token. Console events are the run-lifecycle
types listed in the addendum; graph-extraction types never appear.

THE STREAM, decided: `GET /api/console/stream?since=N` is `text/event-stream` carrying four
frame kinds: `event` (one console event as it is appended, from the store's subscription),
`progress` (one progress line), `liveness` (all marker ages, every 5 s while any spawn is
live), and `heartbeat` (every 15 s). A client that reconnects with `since=` receives every
event after N. A new event appended to the store reaches a connected client within one second.

THE POSITION MODEL, decided: the page loads the snapshot into the core (`fold_reset`), pushes
stream events (`fold_push`), and renders the state at the cursor (`fold_at`). The cursor is a
position among console events; its range is the run's first console event to the head. At the
head the cursor readout shows `LIVE`, the head time and `event N/N`; elsewhere it shows the
time, the position and how long before live. Dragging the track scrubs; the play button
replays from the cursor at five events per second and stops at the head; space toggles play;
the left and right arrows step one event; `End` jumps to live. `scrub_track` in the core
supplies the marks (verdicts red and green, integrations in the accent colour and taller, the
plan approval) and the hour ticks. The URL hash carries the view, the selection and the
position (`#/theater?at=N`, `#/court/<unit>/<round>?at=N`, `#/agents/<spawn>?at=N`) and is
restored on load, so a moment is a link.

THE PALETTE AND KEYS, decided: `Ctrl-K` and `Cmd-K` open the palette; its entries come from
`palette_commands` in the core (views, each unit's courtroom, each agent, jump to live, replay
from start), filtered as the person types, `Enter` runs the highlighted one, `Escape` closes;
the digits 0-6 switch views; the theme toggle persists in the browser's storage.

THE CHARTER, decided: the dashboard charter is the addendum's amended one - no external
assets, served from the binary, zero new crate dependencies, reads projections and writes only
through the guarded actions of spec 97.

CONSTRAINTS WALK: empty store - the shell renders with "no run recorded; start one with
`rigger run <spec>`", the scrubber disabled. Stream drop - reconnect with `since=`, the `dash`
signal amber meanwhile, no fabricated events; a gap beyond the server's retained window
triggers a snapshot re-fetch. Two tabs - independent cursors, no per-viewer server state. An
older run lacking a field - the fold renders the blank, never fails. Core fails to load - one
sentence naming the module and the build; there is no script fallback.

## Notes (non-criteria)

Browser behavior is outside the gate set. Each page-side criterion is proven at two levels
the gates can see: the logic lives in the core and is tested natively (`scrub_track`,
`palette_commands`, `fold_at`), and the served page's source is asserted to call those ops
and to carry the mock's markup and tokens. The SDET lens records a manual walk of the mock
parity checklist (header, tabs, dock, scrubber, statusline, palette, both themes) as a
DecisionMade the adjudicator reads as evidence.

## Global constraints

- Hyphens, never em dashes. Both feature lanes and the core lane green (fmt, clippy -D
  warnings, test); no-os-kill and reap audits green.
- No new event type; no new crate dependency. The served page references no external URL.
- The operator's installed rigger binary is never replaced or modified by any unit.

## Done when

- [ ] a test proves THE PAGE IS THE MOCK'S SHELL: the served page carries the header, tab
  bar, health strip, view, dock, scrubber and statusline regions with the mock's class names,
  the CSS custom properties verbatim for both themes, the three typefaces served from
  `/console/fonts/` with their license texts, and no reference to a URL outside its origin.
  This criterion OWNS the shell markup, the tokens and the embedded assets; the endpoints are
  criterion 2's and the position model criterion 3's, NOT this one's.
- [ ] a test proves THE SNAPSHOT AND THE STREAM: against a real store, the snapshot carries
  the run's console events, progress lines, liveness ages, definition names and the head
  position, the stream is `text/event-stream` with the four frame kinds, an event appended to
  the store reaches a connected client within one second, and `since=` resumes without a gap.
  This criterion OWNS the two endpoints and the console-event filter; the page is criterion
  1's and 3's, NOT this one's.
- [ ] a test proves THE POSITION MODEL: `scrub_track` returns the marks and ticks for a
  recorded stream, `fold_at` returns the state after N for every N, and the served page loads
  the snapshot through `fold_reset`, pushes stream frames through `fold_push`, renders
  through `fold_at`, replays at five events per second, and restores view, selection and
  position from the URL hash. This criterion OWNS the cursor, replay, keys and routing; the
  palette is criterion 4's, NOT this one's.
- [ ] a test proves THE PALETTE: `palette_commands` lists the views, every unit's courtroom,
  every agent, jump to live and replay from start for a recorded stream, and the served page
  opens it on `Ctrl-K` and `Cmd-K`, filters as typed, runs on `Enter` and closes on `Escape`.
  This criterion OWNS the palette only.
- [ ] both feature lanes and the core lane green (fmt, clippy, test).
