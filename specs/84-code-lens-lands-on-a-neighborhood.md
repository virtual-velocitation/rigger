# 84 - The code lens lands on a seeded neighborhood, never on the community overview

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

LANDING RULE, decided: opening the code lens renders a seeded NEIGHBORHOOD, never the overview.
Seed selection, in order: (1) if a run is live, the union blast radius of its in-flight units
(the entities their worktrees touch, from the run's BlastRadiusComputed events); (2) otherwise
the most recently changed entity in the graph (latest ingest generation); (3) otherwise the
highest-degree entity. The neighborhood is two hops of typed directed edges (`calls`,
`constructs`, `implements`, `reads`, whatever the graph carries) rendered with arrowheads and,
on hover or selection, the edge label. Nodes are entities: name, kind dot, community as a
dashed hull behind them. No file node, no per-type bucket, no storage schema name renders in
this lens (spec 63's rule stands).

RENDER BUDGET, decided: the neighborhood is capped at 120 nodes; when two hops exceed the cap the
second hop is truncated by degree and the truncated nodes are represented by one "+N more"
affordance per frontier node, which expands one hop on click. Expansion never re-lays the
whole graph; new nodes enter from their parent's position. A hub entity with hundreds of edges
therefore shows its top neighbors and an honest count, not a hairball and not nothing.

SEARCH SEEDS THE VIEW, decided: a search box (entity names, prefix and fuzzy match, kind shown
beside each hit) re-centers the neighborhood on the chosen entity; the URL carries the seed
(`#/code/<entity>`) so a view is shareable and the back button returns to the prior seed.

OVERVIEW IS ZOOM-OUT, decided: the community collapse remains as an explicit "zoom out" action
from a neighborhood and as a breadcrumb back; it is never the landing page. In the code lens a
community super-node is labelled by its dominant ENTITY (highest degree member) with the member
count, never by a file name; the file lives on the card. Clicking a super-node returns to a
neighborhood seeded on that dominant entity (the DRILL mode collapses into this one path).

THE CARD, decided: unchanged from spec 63 - title row (kind dot, name), provenance
(file:line, degree, community), chips FILE / CONCEPTS / MEMORY with handoff into the files and
concepts lenses and into the decision/finding trail. Files and concepts lenses are untouched by
this spec beyond the handoff target now being a seeded neighborhood.

CONSTRAINTS WALK: empty graph - the lens shows an empty-state sentence naming `rigger graph`
as the way to build one, never an empty canvas. No live run and no ingest yet - seed rule (3).
Seed entity deleted since - fall through to the next rule and say so in the card. Hub entity
(degree in the hundreds) - the cap and the "+N more" affordance, proven with `src/dash.rs`'s
own render function as the seed. Two lenses open in two tabs - each URL carries its own seed;
no shared mutable seed state on the server. Real store - every rendering criterion is proven
against this repository's actual graph, not a fixture: a fixture cannot reproduce the
file-co-location degeneration this spec exists to fix.

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

- [ ] a test proves THE LANDING IS A NEIGHBORHOOD: opening the code lens against this
  repository's real store renders a seeded two-hop neighborhood in which every visible node is
  a code entity with a name and kind, every node has at least one typed directed edge drawn,
  and no node is labelled with a file name - with the seed chosen by the Design's rule order
  (live-run blast radius, else most recent change, else highest degree), each rule proven.
  This criterion OWNS seed selection and the landing render; search and zoom-out are
  criteria 2 and 3's, NOT this one's.
- [ ] a test proves SEARCH AND EXPANSION: choosing an entity in the search box re-centers the
  neighborhood on it with the seed in the URL, and clicking a "+N more" affordance adds exactly
  that frontier node's next hop without re-laying existing nodes, honoring the 120-node cap with
  an honest count. This criterion OWNS the search box, URL seeding and expansion; the landing
  seed is criterion 1's, NOT this one's.
- [ ] a test proves OVERVIEW IS ZOOM-OUT: the community collapse is reached only by an explicit
  zoom-out action, its super-nodes are labelled by dominant entity name and member count
  (never a file name), and clicking one lands on a neighborhood seeded on that entity. This
  criterion OWNS the overview's labelling and both transitions; it introduces no new render
  mode.
- [ ] both feature lanes green (fmt, clippy, test on default and --no-default-features).
