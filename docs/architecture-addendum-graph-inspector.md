# Architecture addendum: the knowledge-graph inspector

**Intent.** Turn the dash's knowledge-graph panel into an inspector a human uses to *understand a
codebase* - what it is about, how its code is wired, where it lives, and why it was built the way it
was - by pointing one parameterized view at a question, rather than staring at a fixed picture of the
whole graph. This addendum owns the human-facing inspector AND the two derivations it stands on (a
concept layer and coupling communities), because the inspector without them is one lens of three.

## Problem

The panel today clusters the whole graph by filesystem directory. The largest node is `src`, which is
both obvious and useless: it restates the folder tree the operator already knows. Four deeper gaps sit
under that surface one:

- **It cannot follow the code.** The graph holds caller-attributed `CALLS` edges, but there is no way
  to ask "what does this function call, transitively" (its execution path) or "who calls this
  function" (its call sites). Worse, a `CALLS` edge is stored scoped to the *caller's* file: a
  cross-file call points at a bare placeholder node in the caller's namespace, not at the real
  definition, so a naive forward walk stops silently at the first file boundary - and on the live
  graph 29% of cross-file calls resolve to a name with more than one definition (`new` has 11, `spawn`
  8, `run` 6), so following every candidate would drag whole unrelated call subtrees into the answer.
- **It has no notion of a concept.** A directory is not an idea, and the graph has no concept nodes at
  all today - clustering the graph's dominant code edges just re-derives the directory tree, because
  code structure *is* mostly the directory tree. A concept ("the grounding pipeline") spans a
  conductor function, several grounder modules, a context-graph fold, and the design docs that govern
  them; expressing it requires a concept layer that does not yet exist.
- **It cannot see coupling.** "Which functions actually work together" is a different grouping from
  both the folder tree and the concept layer, and requires community detection over the call/reference
  edges - which also does not exist yet.
- **It shows the harness, not the project.** When the tool that builds a project also records its own
  run into the same event log, the projection folds the builder's machinery - agent personas, work
  units, gates, agent-touched-file edges - into the graph as nodes. On the project this graph is
  *about*, that machinery is noise. (This is a self-hosting artifact: a foreign repo the tool has only
  indexed has no such nodes at all. It is a real papercut for developing the tool on itself, not a
  cross-project defect.) Separately, the panel seeds itself from *run* events, so it shows nothing on a
  repo the tool has only indexed, never built - a genuine project-agnosticism defect.

## The inspector

The inspector is one view with two orthogonal controls - a **subject** (what you are looking at) and a
**lens** (at what altitude) - plus a **rationale overlay** (why it is the way it is). Every capability
below is a point in that space, not a separate mode.

### 1. Three lenses: an abstraction ladder

A lens is a re-clustering of the same nodes at a chosen altitude. There are three, ordered from
abstract to concrete:

```
  CONCEPTS   what the project is about      "the knowledge graph", "review adjudication"
     |       (design-intent concept nodes, labelled)
     v
  CODE       how the code is coupled         a subsystem -> its functions -> their calls
     |       (call/reference community detection)
     v
  FILES      where it literally lives         a directory -> its files -> their entities
```

The lenses are the same node set bucketed three ways, so the same overview-and-drill machinery renders
all three - only the **bucket key** changes (directory path -> coupling community -> concept
membership). `FILES` reads a node's directory from its id and works on today's graph. `CODE` and
`CONCEPTS` each read a node's bucket from **membership edges produced by an offline derivation**
(community detection; concept extraction) - derivations this campaign builds (see Delivery). Both
derivations run as deterministic, event-sourced passes over the projection and are *consumed*, never
computed, at request time (request-time clustering would jitter the view every poll and break the
rebuildable-projection invariant). Until a derivation has run, its lens renders a documented
"not built yet - run the concept/community pass" state, never an error.

### 2. Subject x lens: re-projection, with a defined cell for every pair

The lens does not just set a global clustering. It re-grains **whatever is selected**, in place, and
the two controls compose freely. "Any-to-any" is only a real promise if every (subject, lens) pair is
defined - including the degenerate ones - so the inspector fixes each cell:

```
  subject \ lens   CONCEPTS                     CODE                          FILES
  -------------------------------------------------------------------------------------------------
  (none)           all concepts + relations     all coupling subsystems       directory overview (spec 42)
  a concept        it + neighbour concepts      its member entities+docs,      the distinct files its members
                                                 with call/ref structure        resolve to (bare nodes -> defining file)
  a function       the concept(s) it realizes:  its call neighbourhood;        its file (+ sibling entities)
                   0 -> "not in any concept",    the seat of the call queries
                   1 -> that concept, N -> all
                   as a graph (flagged, not
                   collapsed)
  a file           the concept(s) it realizes    its entities + their calls    the file / its directory
                   (0/1/N, same as above)
```

Two rules make the cells well-defined. **Wide re-grains truncate, they do not lie**: a concept whose
members span 200 files re-grained to `FILES` reuses the existing render budget and its "showing N of M"
caption rather than pretending to draw all of them. **Cross-grain projection resolves, it does not
trust the id**: projecting a concept or file "down/up" to another grain follows membership and
definition edges, and a bare cross-file placeholder node is resolved to its *defining* file, not the
referencing file its id encodes - otherwise a symbol is attributed to the wrong file exactly in the
cross-file cases. The whole-graph overview is the "nothing selected" case at the current altitude.

### 3. Directed call queries: resolve conservatively, never confidently wrong

When the subject is a function (the Code altitude), two directed questions run over the `CALLS` layer:
**execution path** (forward: what it calls, transitively) and **call sites** (reverse: who calls it).
Both traverse the same stored edges in opposite directions, as a new store-side traversal
`Projection::calls(seed, dir, depth, tier_floor)` - a directed recursive query, not the existing
undirected one flipped, because each hop must resolve a bare cross-file callee to its definition before
it can continue. Three rules keep the result honest:

- **Resolve conservatively.** A hop auto-continues only through edges that are unambiguous: a same-file
  `extracted` call, or a cross-file `inferred` call whose name has exactly one definition. A hop whose
  name resolves to **multiple** definitions is not followed automatically - it renders as a
  **"fans out to N candidates - pick one"** frontier the human expands deliberately. This is the
  difference between a real execution path and a DAG dominated by the eleven wrong `new`s; auto-
  following every candidate is the failure mode this design exists to avoid.
- **Cycles yield a DAG.** Recursion and mutual calls are normal, so the result is reached nodes plus
  the live edges among them, depth-bounded and deduped - never an expanded tree (infinite under
  recursion). Cycles render as marked back-edges.
- **Tier default is about resolvability, not triviality.** The view defaults to the resolvable tiers
  and hides the `ambiguous` tier - but that tier holds real external-crate and macro-generated calls,
  not just language built-ins, so it is a per-subject **opt-in** toggle ("include unresolved calls"),
  never a silent permanent exclusion.

Presentation is a **layered left-to-right DAG** (section 7), because a call graph's whole point is the
direction of execution, which the force layout destroys.

### 4. The rationale overlay ("why")

Decisions, findings, and lessons are not a lens - they are **metadata bound to nodes**, the "why" layer
the three lenses' "what" and "where" cannot express. Each hangs off the graph item it concerns (a
decision `ABOUT` a function, a rule that `GOVERNS` a file) as a leaf, revealed on demand at *any* lens:
focus a function and reveal the decisions that shaped it; focus a concept and reveal the design
decisions behind it. It reuses what the graph already records (the provenance edges and the node-
provenance query); only the decision's *content* is project design memory and stays, while the builder-
agent attribution is machinery and drops (section 5). It is the highest-value view for AI-authored code,
where "why is this here?" is the question a human most needs answered - and it is simply empty, never an
error, on a project with no decisions yet.

### 5. A graph of the target project, not the harness

The knowledge graph models the **target project**. The builder's machinery - agent personas, work
units, gates, and the agent-touched-file edges - is removed **at the fold**: the projection stops
folding those event arms into nodes and edges, so the graph itself (everywhere it is read - dash,
grounding, blast-radius) is the target project, not just the dash view. This is safe: the machinery's
functional consumers do not read these nodes (peer grounding reads the sidecar; run pruning and metrics
read the event log directly), and the run-tree - the proper home for "what the build is doing" -
projects units and stages straight from the event log, untouched. The change is a **fold-level scoping**
(what the projection includes), not a capability removal: every excluded fact remains in the event log
and the run-tree.

Two consequences this campaign handles rather than ignores: the decision/finding/lesson **content**
stays (only the agent attribution drops), so the rationale overlay is unaffected; and the run-tree's
click-to-seed, which today seeds the graph with a unit id, is re-pointed to seed with the files and
decisions that unit produced (which remain in the graph) so clicking a unit still lands somewhere real.

### 6. One route, two providers

The whole inspector is one route whose parameters are the two axes plus drill and overlay:

```
  /api/graph
     subject :  seed=<id>  view=neighborhood|calls  dir=down|up|both  depth=  tier=
     lens    :  lens=concepts|code|files  resolution=          (no seed => overview)
     drill   :  cluster=<key>   (lens-aware)
     why     :  explain=<id>    (the rationale overlay for a node)
```

`view` and `lens` absent reproduce the existing seeded-neighborhood and directory-overview behaviours
exactly, so the extension never regresses what exists. Crucially, the graph views **must not** ride the
dash's polled per-request provider, which pre-fetches a small run-seeded subgraph on every 1.5-second
state poll: reading the whole projection (or running a directed traversal) on that cadence would be a
quarter-million-edge read per poll on a large repo. Instead, `/api/graph` gets its **own lazy provider**
- a projection query handle opened only when a graph request arrives (panel load, a drill, a lens flip,
a call query), never on the state poll. The existing seeded-neighborhood view is moved onto this same
provider too, which also fixes its project-agnosticism dead-end (below).

### 7. Presentation: two real layouts behind one emitter

The force-directed layout is right for the overviews, the concept map, and the undirected neighbourhood
- a few dozen weighted super-nodes it already draws well. It is wrong for a call query, where the
direction of execution is the point. So the SVG emitter is refactored to take a **position map** rather
than compute one internally, and gains a second layout: a **layered left-to-right DAG** (layer by hop
distance, within-layer barycenter ordering, cycles as curved back-edges, high fan-out capped with a
"+K more" expander) plus **directional arrowheads** (SVG markers the renderer does not have today). This
is a real second layout module, comparable in size to the existing force layout and component packing -
not a position-map swap. The toolbar carries the lens selector (the concrete<->abstract toggle, with a
resolution control for the Code/community lens) and the rationale toggle.

### 8. Project-agnostic by construction

The inspector must work on a cold checkout of any repository, with no build history:

- Ingest gains a **cold-checkout entry** (`graph build`) so the graph can be populated from source
  alone, without a run - today ingest is only reachable from inside a run, which is why an
  indexed-but-never-built repo shows an empty panel. The seeded-neighbourhood, whole-graph, and call
  views all read the projection through the section-6 provider, so none of them dead-end on a repo with
  no run history.
- The call views use only `CALLS`/`REFERENCES` and the `<file>::<name>` id scheme the extractor produces
  for every supported grammar; name resolution is language-neutral, and object-oriented method-name
  collision is handled by the conservative traversal (section 3), not by pretending precision.
- `FILES` needs only code ingest; `CODE` needs the community pass; `CONCEPTS` needs the concept pass;
  the rationale overlay is simply empty on a project with no decisions. Each degrades independently.
- Directed traversal and name resolution stay sub-linear on large repositories via two additive indexes
  - a partial index on the live-edge relation, and an expression index on the entity-name attribute -
  added through the existing additive-migration pattern, with the resolution query phrased to use the
  exact name-suffix expression the index materialises (a mismatched expression silently misses it).

## Delivery

This ships as **one campaign - many work units, one feature/PR** - because a lens toggle with only one
lens, or a graph still full of harness nodes, is not the thing. It is honestly a large campaign: it
builds the two derivations the abstract lenses stand on, not only the dash surface. The units group as:

- **The derivations** (the biggest, previously-unbuilt part): cold-checkout `graph build` ingest; a
  deterministic community-detection pass emitting `IN_COMMUNITY` membership edges; a concept-extraction
  pass emitting concept nodes and `REALIZES` membership edges, labelled deterministically with a
  model-assisted refinement that has a deterministic fallback. These are event-sourced folds, seeded and
  ordered deterministically so a rebuild reproduces them.
- **The graph read path**: the dedicated lazy `/api/graph` provider (section 6), the store-side directed
  `Projection::calls` traversal with conservative resolution (section 3), and the two index migrations.
- **The graph content**: the fold-level de-noise (section 5) and the run-tree click-to-seed re-point.
- **The presentation**: the pluggable bucket key (three lenses over the existing overview/drill folds),
  the second layered-DAG layout and arrowheads, and the client lens/overlay toggles with the
  subject-by-lens re-projection matrix (section 2).

## Global constraints

- The event log stays the source of truth; the graph is a rebuildable projection. The concept and
  community layers are event-sourced derivations; nothing is computed at request time.
- Determinism by construction, at every new choice point: community detection is seeded over a
  deterministic edge ordering; concept labels break degree ties lexicographically; multi-candidate call
  resolutions are ordered by id so the rendered DAG does not jitter across polls; any serialized set uses
  ordered structures. A rebuild reproduces every derivation and every view byte-identically.
- The de-noise is a fold-level **scoping** change (what the projection includes), not a capability
  removal: the excluded machinery remains fully in the event log and the run-tree, and no functional
  consumer (grounding, pruning, metrics, blast-radius) reads the dropped nodes.
- Both feature lanes stay green (fmt, clippy, test on default and no-default-features). The dash and
  every route serve in both lanes; an unbuilt lens layer, an empty overlay, or an unresolved call hop
  degrades to a documented state, never an error.
- Hyphens, not em dashes. No references to any external tool or project in code, comments, docs, or
  commit messages.
- The inspector is read-only over the store and projection; it adds query and derivation, never a
  mutation of the log.
