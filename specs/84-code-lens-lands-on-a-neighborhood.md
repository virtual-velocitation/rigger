# 84 - The code lens is a labelled map: districts, semantic zoom, and exploration without vocabulary

**Goal:** the dash's code lens has three render modes in `src/dash.rs` - a clustered
OVERVIEW of community super-nodes (spec 42 c3's render-budget answer to a 21k-node graph, spec
63 c6's collapse), a DRILL into one cluster, and a seeded NEIGHBORHOOD / directed-call view -
and it LANDS on the overview. At this repo's real scale (21,394 nodes, ~150 communities) the
communities form by file co-location, so their dominant-member labels are file names
(`src/dash.rs (256)`, `tests/cli.rs (253)`), inter-community edges are sparse, and the first
screen reads as a worse files lens: disjointed blobs named after files. The view the operator
actually wants - functions and their typed call relations around something they care about -
is the neighborhood mode, unreachable from the landing page. Spec 63's purity criteria all hold
(every node IS a set of code entities); what was never specified is what the first screen shows.
The visual contract for this spec is the Mission Control mock's Knowledge tab, Code lens
(artifact 54f8451e-ca21-45a5-bab7-575e57451b20, approved by the operator 2026-09-06): entity
nodes with kind dots and names, typed directed edges with arrowheads, a card with File /
Concepts / Memory chips that hand off to the other lenses.

## Design

THE MAP, decided (supersedes the earlier seeded-neighborhood-with-overview design; the operator
reviewed the mock's map on 2026-09-06 and called it excellent): the code lens is ONE zoomable,
pannable map with semantic zoom - not a neighborhood mode plus an overview mode. Its invariant:
EVERY ENTITY ON SCREEN IS LABELLED; zoom controls how many entities are on screen, never
whether they have names. Exploring means wandering a labelled map; search is a shortcut, not
the entry.

DISTRICTS, decided: communities are grouped into districts named by PURPOSE (a curated mapping
from module path to purpose, e.g. `worktree` -> "worktree lifecycle", `reap` -> "reaping
authority", `liveness`/`spawn` -> "liveness & heartbeats", `dash*` -> "dashboard rendering";
unmapped modules fall back to the module name, never to a file name), drawn as per-community
dashed hulls with ONE district label at the centroid - letter-spaced small caps in a pill whose
size grows with the district's population, with the entity count beneath - collision-avoided
against other labels. District labels are always present at every zoom.

SEMANTIC ZOOM, decided: each entity has a rank inside its community by degree; at a given zoom
the entities shown per community are the top `budget(zoom)` by rank, so zoomed out only the
landmarks appear (always labelled) and zooming in reveals more, each with its label. Labels
dodge each other (four candidate positions); an entity whose label cannot be placed is NOT
drawn - the invariant beats density. Landmark names (rank < 3) take their kind's colour; the
selected entity is underlined; edges among visible entities are drawn with arrowheads, and a lit
edge shows its relation type (`calls`, `reads`, `constructs`, `implements`) at its midpoint.
Interaction: scroll zooms about the cursor, drag pans, double-click a district fits it, a
"fit whole map" control returns to the full extent, clicking an entity lights its callers and
callees and lists them BY NAME on the card (exploration continues by names). The camera never
resets on its own.

EXPLORE RAIL, decided: beside the card, four always-available starting points that need no
vocabulary: Landmarks (busiest entities), Changing right now (the live run's blast radius),
Argued about in review (entities with findings pinned), Bridges between districts (entities
with the most cross-district edges). Each chip flies the camera to that entity and selects it.
A search box (prefix and substring, kind and degree beside each hit) remains as a shortcut;
the URL carries the selected entity (`#/code/<entity>`) so a view is shareable.

LEGEND, decided: a persistent legend on the canvas names every visual class - district pill,
entity dot and the four kind colours, the typed directed edge, "lit" for the selection and its
neighbours, the amber ring for a live unit's blast radius - so a reader never has to infer what
a mark belongs to. Tests are not on the map at all (spec 86): a card carries "proven by N
tests" or an explicit no-test state.

SEED, decided: the initial camera is the full extent (fit whole map); if a run is live, the
Explore rail's "Changing right now" chips lead into its blast radius. No entity is
auto-selected on open.

RENDER BUDGET, decided: the map holds the whole graph client-side (adjacency lists; 9k entities
after spec 86, 21k before), but per frame draws only the entities passing the rank budget inside
the viewport plus the selection's neighbours; label placement bounds the visible count. A hub
with hundreds of callers shows its top neighbours at the current zoom and an honest degree on
the card, never a hairball.

THE CARD, decided: spec 63's card, extended: title row (kind dot, name), provenance
(file:line, degree, district), CALLED BY and CALLS rows listing neighbours by name with the
relation type (each a chip that flies to and selects that entity), the PROOF row from spec 86,
and the FILE / CONCEPTS / MEMORY chips with handoff into the files and concepts lenses and the
decision/finding trail. Files and concepts lenses are untouched by this spec beyond their
handoff target now being a selected entity on the map.

CONSTRAINTS WALK: empty graph - the lens shows an empty-state sentence naming `rigger graph`
as the way to build one, never an empty canvas. A district with one community - the hull and
the pill coincide, still labelled. A district label that cannot be placed without collision at
the current zoom - it yields to larger districts and returns as the user zooms in; entity
labels never displace district labels. A selected entity whose neighbours are outside the rank
budget - they are drawn anyway (the selection's neighbours are always visible). Hub entity
(degree in the hundreds) - the card shows the honest degree and the top neighbours by degree;
the map draws those within the budget. Two lenses open in two tabs - each URL carries its own
selection; no shared mutable camera state on the server. Real store - every rendering
criterion is proven against this repository's actual graph, not a fixture: a fixture cannot
reproduce the file-co-location degeneration this spec exists to fix.

## Notes (non-criteria)

Mission Control (the operator-approved target console) will absorb this lens as its Knowledge
tab; build it as a self-contained component (payload + render) so it moves without rewrite. The
dash header string "the unified graph (code, design, decisions)" predates the lens design and is
replaced by the lens's own title. `rigger graph --around <file>` returning `.cargo/*` artifacts
as depth-2 neighbours is graph noise worth its own look, out of scope here.

## Global constraints

- Hyphens, never em dashes. Both feature lanes green (fmt, clippy -D warnings, test, default
  and --no-default-features); the no-os-kill and reap audits stay green.
- Dash charter holds: no external assets, inline JS in the served page, zero new dependencies,
  read-only over existing projections. No new event type.
- The operator's installed rigger binary is never replaced or modified by any unit.

## Done when

- [ ] a test proves THE MAP LANDS LABELLED: opening the code lens against this
  repository's real store renders the map at full extent in which every visible node is
  a code entity with a placed label and kind dot, every district carries its purpose label,
  and no node or district is labelled with a file name - and zooming in strictly increases
  the labelled-entity count without ever drawing an unlabelled node. This criterion OWNS the
  districts, semantic zoom and label placement; the rail, search and selection are
  criterion 2's, and the legend is criterion 3's, NOT this one's.
- [ ] a test proves EXPLORATION NEEDS NO VOCABULARY: the Explore rail offers Landmarks,
  Changing right now (empty-state when no run is live), Argued about in review and Bridges
  between districts, each chip flying the camera to and selecting that entity; clicking an
  entity lights its callers and callees and lists them by name with relation types on the
  card; the search box re-centers on a chosen entity with the selection in the URL; and the
  camera never resets except through fit-whole-map or a district double-click. This criterion
  OWNS the rail, search, selection and camera; districts and semantic zoom are criterion 1's,
  NOT this one's.
- [ ] a test proves THE LEGEND AND TEXT CLASSES: the served page carries a persistent legend
  naming the district pill, the entity dot and its four kind colours, the typed directed edge,
  the lit selection and the blast-radius ring, and the rendered map uses exactly those
  treatments (small-caps pill for districts, kind-coloured landmark names, underlined
  selection, italic relation type on lit edges). This criterion OWNS the legend and the text
  treatments; it introduces no new render data.
- [ ] both feature lanes green (fmt, clippy, test on default and --no-default-features).
