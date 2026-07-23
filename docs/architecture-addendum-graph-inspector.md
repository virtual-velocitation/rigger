# Architecture addendum: the knowledge-graph inspector

**Intent.** Turn the dash's knowledge-graph panel into an inspector a human uses to *understand a
codebase* - what it is about, how its code is wired, where it lives, and why it was built the way it
was - by pointing one parameterized view at a question, rather than staring at a fixed picture of the
whole graph.

## Problem

The panel today clusters the whole graph by filesystem directory. The largest node is `src`, which is
both obvious and useless: it restates the folder tree the operator already knows. Three deeper gaps
sit under that surface one:

- **It cannot follow the code.** The graph holds caller-attributed `CALLS` edges, but there is no way
  to ask "what does this function call, transitively" (its execution path) or "who calls this
  function" (its call sites) - the two questions a person actually has about a function. Worse, a
  `CALLS` edge is stored scoped to the *caller's* file: a cross-file call points at a bare placeholder
  node in the caller's namespace, not at the real definition, so any naive forward walk stops silently
  at the first file boundary.
- **It has no notion of a concept.** A directory is not an idea. "The grounding pipeline" spans a
  conductor function, several grounder modules, a context-graph fold, and the design docs that govern
  them - a grouping the filesystem cannot express and directory clustering cannot recover (clustering
  the graph's dominant code edges just re-derives the directory tree).
- **It shows the harness, not the project.** Because the tool that builds the project also records its
  own run in the same event log, the graph surfaces the builder's machinery - agent personas, work
  units, gates, and the contents of the tool's own working directory - as top-level nodes. On the
  project this graph is *about*, that machinery is noise; on any other project it would be pure
  pollution. The panel also seeds itself from run events, so it presents nothing at all on a repo the
  builder has only ever indexed, never built.

## The inspector

The inspector is one view with two orthogonal controls - a **subject** (what you are looking at) and a
**lens** (at what altitude) - plus a **rationale overlay** (why it is the way it is). Every capability
below is a point in that space, not a separate mode.

### 1. Three lenses: an abstraction ladder

A lens is a re-clustering of the same nodes at a chosen altitude. There are three, ordered from
abstract to concrete:

```
  CONCEPTS   what the project is about      "the knowledge graph", "review adjudication"
     |       (design-intent communities, labelled)
     v
  CODE       how the code is wired          a subsystem -> its functions -> their calls
     |       (call/reference coupling)
     v
  FILES      where it literally lives        a directory -> its files -> their entities
```

The lenses are the same node set bucketed three ways, so the same overview-and-drill machinery renders
all three - only the bucket key changes (directory path -> structural community -> concept membership).
`FILES` is the concrete floor (a node's bucket is its file's directory). `CODE` groups code entities
by how tightly they call and reference each other, regardless of directory, so a subsystem can pull a
conductor function and a grounder module into one group because they *work together*, not because they
share a folder. `CONCEPTS` groups by design intent - the specs, docs, and handbook rules that govern
code, plus discovered concept nodes - so a concept is an idea that cuts across files and subsystems.

The concept and community layers are produced offline as deterministic, event-sourced derivations over
the projection (community detection over a chosen edge layer; a concept-extraction pass), per the
concept-graph addendum. The inspector *consumes* those layers; it does not compute them at request
time (request-time clustering would jitter the view on every poll and break the rebuildable-projection
invariant).

### 2. Subject x lens: re-projection, not navigation

The lens does not just set a global clustering. It re-grains **whatever is selected**, in place. The
two controls compose freely:

```
   subject  =  a concept  |  a function  |  a file  |  nothing (whole graph)
   lens     =  CONCEPTS   |  CODE        |  FILES

   select "the knowledge graph" concept, then flip the lens:
     CONCEPTS -> the KG concept and the concepts it relates to
     CODE     -> every function + design doc that realizes the KG, with their call/reference structure
     FILES    -> those same members collapsed to the literal files they live in
```

Any-to-any: the operator jumps concept -> files directly, without stepping through a drill. This works
because the links *between* altitudes exist in the data - a concept `REALIZES` its code entities and
docs, and every code-entity id carries its file - so the view can project a selection up or down to any
grain by following those links. The whole-graph overview is simply the "nothing selected" case, shown
at the current lens's altitude.

### 3. Directed call queries at the Code grain

When the subject is a function (the Code altitude), two directed questions become available over the
`CALLS` layer:

- **Execution path** - the forward transitive call graph (what it calls, and what those call).
- **Call sites** - the reverse (who calls it), one-hop and transitive.

Both traverse the *same* stored edges in opposite directions. Three realities shape the traversal:

- **Cross-file resolution.** A hop that lands on a bare callee node (a cross-file call) is resolved by
  matching its name to the definition node(s) that share it, and the walk continues from there. When a
  name resolves to more than one definition (`new`, `run`, `apply`), the view returns *all* candidates
  and marks the fan-out ("1 of N candidates") rather than inventing a single answer - the graph's
  extracted-vs-inferred trust split, surfaced honestly.
- **Cycles.** Recursion and mutual calls are normal, so the result is a **DAG** of reached nodes and
  the live edges among them, never an expanded tree (an expanded tree of a recursive function is
  infinite). Traversal dedups reached nodes and is depth-bounded.
- **Noise.** Most `CALLS` edges are calls to language built-ins (`Some`, `unwrap`). The call views
  default to the trusted tiers (extracted and inferred) so the closure's size is bounded by real
  project calls, not standard-library chatter.

### 4. The rationale overlay ("why")

Decisions, findings, and lessons are not a lens - they are **metadata bound to nodes**, the "why" layer
that the three lenses' "what" and "where" cannot express. Each hangs off the graph item it concerns (a
decision `ABOUT` a function, a rule that `GOVERNS` a file) as a leaf, revealed on demand at *any* lens:
focus a function and reveal the decisions that shaped it; focus a concept and reveal the design
decisions behind it. It is a per-node overlay toggled on when the operator wants the reasoning and out
of the way otherwise.

This is the highest-value view for AI-authored code, where "why is this here?" is the question a human
most needs answered - the decision trail *is* the answer. It reuses what the graph already records
(the provenance edges and the node-provenance query); only the decision's *content* is project design
memory and stays, while the builder-agent attribution ("which persona decided") is machinery and drops
(see section 5).

### 5. A graph of the target project, not the harness

The knowledge graph models the **target project**, full stop. The builder's machinery - agent personas,
work units, gates, the agent-touched-file edges, and everything under the tool's own working directory
- is excluded from the graph. It does not vanish from view: the dash's run-tree already projects the
run's mechanics straight from the event log, which is its proper home. The two surfaces separate
cleanly - the graph is *what the project is*, the run-tree is *what the build is doing*.

Concretely, the graph keeps only: code entities and their `CALLS`/`REFERENCES`; design intent (the
project's specs, docs, and handbook rules that govern the code); and the decisions/findings/lessons
about that code (content only). Ingest never walks the tool's working directory or build outputs. And
the whole-graph and call views read the projection directly rather than seeding from run events, so the
inspector works on any repo the tool has merely indexed. This is what makes the graph project-agnostic:
nothing in it, or in the queries over it, keys on the builder's vocabulary.

### 6. One route: the parameter space

The whole inspector is one route whose parameters are the two axes plus drill and overlay:

```
  /api/graph
     subject :  seed=<id>  view=neighborhood|calls  dir=down|up|both  depth=  tier=
     lens    :  lens=concepts|code|files  resolution=          (no seed => overview)
     drill   :  cluster=<key>   (lens-aware)
     why     :  explain=<id>    (the rationale overlay for a node)
```

`view` and `lens` absent reproduce the existing seeded-neighborhood and directory-overview behaviors
exactly, so the extension never regresses what exists. No seed selects a lens overview; a seed with
`view=calls` selects a call query; `dir=both` centers a function with callers left and callees right -
"the flow through this function" - answering both directed questions in one render.

### 7. Presentation: two layouts, one emitter

The force-directed layout is right for the overviews, the concept map, and the undirected neighborhood
- a few dozen weighted super-nodes it already draws well. It is wrong for a call query, where the
direction of execution is the entire point. So the inspector has **two layout functions behind one SVG
emitter**: the force layout, and a **layered left-to-right DAG** for the call views (seed at the root,
callees/callers in ranked tiers by hop distance, recursion drawn as labeled back-edges, high fan-out
capped with a "+K more" expander). The emitter that draws nodes and edges is shared; only the position
map differs, selected by the view. The toolbar carries the lens selector (the concrete<->abstract
toggle, plus a resolution control for the Code/community lens) and the rationale toggle.

### 8. Project-agnostic by construction

The inspector must work on a cold checkout of any repository, with no build history:

- The call views use only `CALLS`/`REFERENCES` and the `<file>::<name>` id scheme the extractor
  produces for every supported grammar; name resolution is language-neutral. Method-name collision in
  object-oriented languages is handled by the multi-candidate marking, not by pretending precision.
- `FILES` and `CODE` (structural community) need only pure code ingest; `CONCEPTS` needs only the
  design-doc pass; none require the builder to have ever run. The rationale overlay is simply empty on
  a project with no decisions yet - a graceful absence, never an error.
- Directed traversal and name resolution stay sub-linear on repositories an order of magnitude larger
  than the tool's own, via additive indexes on the live-edge relation and on the entity-name attribute.

## Delivery

This ships as **one campaign** - the de-noise, the three lenses, the directed call queries, and the
rationale overlay land together, because a lens toggle with only one lens, or a graph still full of
harness nodes, is not the thing. Internally the work is small, well-bounded seams on existing
structure: a pluggable bucket key over the existing overview-and-drill folds (the three lenses); one
new directed traversal beside the existing undirected one (the call queries), with query-time
cross-file resolution; the exclusion rules in ingest and the fold (the de-noise); the shared emitter
gaining a second, layered layout (the call presentation); and the route gaining the `view`/`dir`/`lens`
parameters. The offline concept and community derivations are the concept-graph addendum's workstreams,
consumed here.

## Global constraints

- The event log stays the source of truth; the graph is a rebuildable projection. The lenses' concept
  and community layers are event-sourced derivations; nothing is computed at request time.
- Determinism by construction: community detection is seeded over a deterministic edge ordering and
  reproduces byte-identically on rebuild; any serialized set uses ordered structures.
- The de-noise is a **scoping** change (what the projection includes), not a capability removal: the
  excluded machinery remains fully available through the event log and the run-tree.
- Both feature lanes stay green (fmt, clippy, test on default and no-default-features). The dash and
  every route serve in both lanes; an absent lens layer or an empty overlay degrades to a graceful
  message, never an error.
- Hyphens, not em dashes. No references to any external tool or project in code, comments, docs, or
  commit messages.
- The inspector is read-only over the existing store and projection; it adds query and derivation, never
  a mutation of the log.
