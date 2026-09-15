# 98 - The Knowledge tab absorbs the inspector

**Goal:** the knowledge-graph inspector (three lenses: the labelled code map of spec 84, the
files lens, the concepts lens) lives in the old dashboard page, `src/dash.html`, beside a run
tree and metrics the console now renders better. With specs 93-97 landed, two pages fold the
same log and draw the same graph. Mission Control's Knowledge tab
(docs/architecture-addendum-mission-control.md, section 5.4) takes the inspector in, adds the
one thing the console can add - the run's activity on the map - and the old page retires.

## Design

THE LENSES IN THE CONSOLE, decided: the Knowledge tab renders the lens bar (Code, Files,
Concepts), the search box, `Fit whole map` and the `Run activity overlay` toggle exactly as
the mock does, and hosts the Code lens's map component from spec 84 (its layout, ranking,
label placement and hit testing in the core, its drawing in the page) and the files and
concepts lenses as the inspector defines them, all through the core's graph ops
(`graph_load`, `graph_query`, `map_build`, `map_frame`, `map_hit`). The card, the explore
rail and the legend are spec 84's, unchanged; the card's Memory chips open the Courtroom at
the finding's unit and round, and its File and Concepts chips switch lenses with the subject
selected.

THE RUN-ACTIVITY OVERLAY, decided: at the scrubber's position, the entities in every
in-flight unit's blast radius (`BlastRadiusComputed` precise and safe file lists, plus
`FileTouched` since the unit started) carry the amber ring the legend names, the files a
building unit is editing pulse, the lens bar's hint reads `in flight: <units> - blast radius
lit` or `nothing in flight at this moment`, and the explore rail's `Changing right now`
chips lead into that radius. The toggle removes the overlay without moving the camera. The
overlay is computed by the core from the fold, so scrubbing moves it with the run.

THE OLD PAGE RETIRES, decided: `rigger dash` serves the console at `/`; `src/dash.html` and
the page-side fold and layout code it carried are deleted; the graph payload route and
`/api/state` remain for external readers; the served page holds no fold and no graph
algorithm in script - an audit test asserts the console script contains only the loader,
the DOM builders, the canvas draw loop, input handling, the stream client and the theme.

CONSTRAINTS WALK: no run live - the overlay has nothing to light and the hint says so. A
blast radius naming a file the graph does not index - ignored for lighting, counted in the
hint. Two lenses open in two tabs - each URL carries its own lens and selection. The graph not
built yet - the tab shows the inspector's documented empty state naming `rigger graph build`.

## Notes (non-criteria)

Spec 84 must have landed on the core (its amended Design) before this spec runs; the map
component moves without rewrite. The current dashboard's run tree and metrics panels are
not carried over; their questions are answered by Theater and Plan.

## Global constraints

- Hyphens, never em dashes. Both feature lanes and the core lane green (fmt, clippy -D
  warnings, test); no-os-kill and reap audits green.
- No new event type; no new crate dependency. The served page references no external URL.
- The operator's installed rigger binary is never replaced or modified by any unit.

## Done when

- [ ] a test proves THE LENSES IN THE CONSOLE: the Knowledge tab renders the lens bar,
  search, fit and overlay controls with the mock's markup, the Code lens draws spec 84's map
  from the core's draw list against this repository's real store, the files and concepts
  lenses render through the core's graph ops, and the card's Memory, File and Concepts chips
  hand off as stated. This criterion OWNS the tab's hosting of the lenses and the hand-offs;
  the overlay is criterion 2's, NOT this one's.
- [ ] a test proves THE RUN-ACTIVITY OVERLAY: for a recorded stream with a unit in flight,
  `map_frame` at that position marks the blast-radius entities and the editing files, the
  hint names the units, the rail's `Changing right now` chips lead into the radius, the
  toggle clears the marks without moving the camera, and at a position with nothing in flight
  the hint says so. This criterion OWNS the overlay only.
- [ ] a test proves THE OLD PAGE RETIRES: `rigger dash` serves the console at `/`,
  `src/dash.html` no longer exists in the tree, the graph payload route and `/api/state`
  still answer, and the served script passes the no-fold-in-script audit. This criterion OWNS
  the retirement and the audit only.
- [ ] both feature lanes and the core lane green (fmt, clippy, test).
