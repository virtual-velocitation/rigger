# Reference Architecture Addendum — Project-Agnostic Concept Knowledge Graph

**Status:** design, approved for planning. Current-state facts are grounded in the code as of
2026-07-21 (`src/main.rs` `cmd_graph`, `src/conductor.rs` `ingest_project_into_graph`, the
`src/grounder/symbols` tree-sitter extractor); figures marked *(est.)* are not measured.
**Scope:** an addendum to `docs/architecture.md`, building on the unified event-sourced
knowledge graph established in the context-management addendum (its §6). It makes that graph a
**general, standalone capability**: point rigger at ANY repository — any language, any domain,
with or without dev-loop history — and get a queryable knowledge graph organized the way a human
inspects a project: **by concept**, with files and code as the detail *inside* a concept. It adds
one new node layer (concepts), one new derivation (community detection), and one parameterized
exploration surface. It changes no invariant of the event-sourced core.

---

## 1. Problem

The knowledge graph today is organized by **code structure and the dev-loop's own decision
stream**, and it only exists once rigger has run its loop on the project. Three facts about the
current surface:

- **There is no concept layer — grouping collapses to the filesystem.** Nodes are code entities
  (`<file>::<name>`), files, docs, and dev-loop artifacts (decisions, findings, units). A human
  exploring "what does this project *know*" thinks in **ideas** — "context pruning", "the
  knowledge graph", "bounded remediation" — not in files. Those ideas are **sub-document and
  cross-document**: one spec can define *context-pruning* AND *the knowledge graph* (two distinct
  concepts), and one concept can span several specs and dozens of files. The graph has no node for
  an idea, so any attempt to group it falls back to directories and files.

- **The dominant edge layer buries intent.** In a real project the graph is overwhelmingly
  `REFERENCES` (code-to-code): a measurement near one hub node found **8,650 of ~9,900** incident
  edges were `REFERENCES`. Any structural clustering is drowned by that layer and recovers
  *modules*, not *concepts*. The thin, high-value intent layer (`SPECIFIES`, `GOVERNS`, `ABOUT`,
  `CONSTRAINS`) — the edges that actually carry meaning — is invisible under it unless it is
  isolated deliberately.

- **The capability is bound to the target being this project.** `rigger graph` requires a seed
  (`--around <id>`), prints a text subgraph, and degrades to "has `rigger run` been run yet?" —
  it presumes the dev-loop populated the graph. Ingest is entered from a run
  (`ingest_project_into_graph`, `conductor.rs`), and tree-sitter coverage is ~6 languages
  (`rust`, `python`, `javascript`, `typescript`, `go`, `c#`). Pointed at an arbitrary repo — a
  language it does not yet parse, a project that has never seen the loop, a codebase whose
  vocabulary rigger knows nothing about — it cannot produce a concept graph. There is no
  whole-project overview and no community/concept summary at all.

### Non-goals / anti-fixes
- Do NOT organize the top level by files or modules — files are *detail within a concept*, never
  the entry point (§2.2). The filesystem is one lens, not the hierarchy.
- Do NOT bake any target-domain vocabulary into extraction — concepts come from the *target
  project's own* code and docs, never from rigger's spec/decision taxonomy (§2.1).
- Do NOT require the dev-loop to have run — the concept graph must build on a cold checkout
  (§2.1). Dev-loop artifacts are an *optional* source when present.
- Do NOT put the source code through an LLM — code structure stays deterministic and local; only
  the semantic layer is model-driven (§2.4).
- Do NOT introduce a second store — concepts and communities fold into the SAME event-sourced
  projection as everything else (§2.5).

---

## 2. Load-bearing decisions — invariants the design must carry

### 2.1 Project- and domain-agnostic by construction
Every node and edge is derived from the **target project's own artifacts** — its source, its
docs, its comments — with zero assumptions about the project's domain and zero dependence on
rigger's dev-loop vocabulary. The capability builds on a **cold checkout**: a repo that has never
run the loop still yields a full code + document + concept graph. Rigger's own decisions,
findings, and lessons are *one more optional source* folded in **when they exist**, never a
prerequisite. Acceptance is measured across N diverse repos and languages (§9); this project's
own tree is one row in that matrix, not the target.

### 2.2 Concepts are a first-class layer — below the document, above the code
A **concept** is a distinct idea the project is *about*. It is not a file, not a document, not a
node kind — it is its own layer, and it is **many-to-many** with everything else: a document
defines one *or several* concepts; a concept is realized across one *or many* files and referenced
by one *or many* decisions. Human inspection enters through this layer; files, entities, and
communities are the detail reached by drilling into a concept. "Show me every file the
knowledge-graph concept lives in" must be a one-hop query, not a manual cross-file hunt.

### 2.3 Two derivations, two grains, one graph — neither privileged
Concepts and structure are produced by two independent mechanisms, and both are **lenses over one
graph**, not competing hierarchies:
- **Fine, named concepts** come from a **model reading the semantic content** (docs, comments,
  rationale). Only a model reliably splits *two* concepts out of *one* document.
- **Coarse subsystems** come from **community detection** on the graph's connectivity —
  deterministic, unsupervised, structural.
A person picks which lens (and which grain) to view; the graph carries both. Community detection
never has to invent an idea, and the model never has to guess module boundaries.

### 2.4 Deterministic where it can be; model-driven only where it must be
Source-code structure is extracted **locally and deterministically** (tree-sitter AST, no
network, no model), and tagged `EXTRACTED`. Only the concept/semantic layer routes through the
configured model, and its output is tagged `INFERRED`. The confidence tier on every node and edge
records *which* produced it, so a reader (and every downstream consumer) can trust the
deterministic layer absolutely and treat the model layer as advisory. A re-run of the
deterministic layer on unchanged source produces an identical graph.

### 2.5 Event-sourced projection — inherit every core invariant
Concept nodes, concept edges, and community assignments are **folded from events by the same
idempotent `apply`** that folds a code entity or a decision (context-management addendum §2.1):
the event log is the source of truth, the graph is a rebuildable projection, supersession sets
`valid_to` and never deletes, and every node/edge carries the `proj-<identity>-` namespace so a
shared backend never mixes two projects (that addendum §2.2). Re-extracting after a file or doc
changes **supersedes** the old concept/edges rather than overwriting them, so the graph stays
queryable across time.

### 2.6 The view is parameterized — not one fixed hierarchy
Exploration is defined by four orthogonal parameters, not a hard-coded tree:

```
  lens        = concept (default) | file | kind        which axis partitions the top level
  edge-layer  = intent | structure | both              intent = SPECIFIES/GOVERNS/ABOUT/…
                                                        structure = REFERENCES (the code map)
  resolution  = coarse … fine                           community grain (few domains ↔ many)
  drill       = re-group one level finer, CROSS-axis    concept → files → entities
```

Files and concepts are both reachable; concepts are the default entry and files are a detail
reached by drilling. "Look at files if I want" is `lens=file`; "start at concepts and move into
files" is `lens=concept` then drill. The default is `lens=concept, edge-layer=intent`, because
`edge-layer=structure` alone reproduces the file view the `REFERENCES` layer already dominates
(§1).

---

## 3. Workstream A — Language-agnostic code ingest, standalone

**Target state:** a single command builds the structural graph for **any** repository, off a cold
checkout, with no run required.

```
  rigger graph build <path>            # cold checkout, no dev-loop, no seed
        │
        ▼
  walk the repo ──► per-file tree-sitter AST ──► emit events ──► fold into projection
        │              (local, deterministic)      CodeEntityExtracted { file, name, kind }
        │                                          EdgeInferred        { from, rel, to, tier }
        ▼
  entities: fn · type · class · module · import          edges, confidence-tiered:
                                                          calls / imports / inherits   EXTRACTED
                                                          depends_on (transitive)      INFERRED
                                                          dynamic / reflected ref      AMBIGUOUS
```

- **Any language, gracefully.** Extraction is driven by a per-language grammar + a tag query;
  adding a language is adding a grammar, not code. A file in a language with no grammar is
  ingested at **file granularity** (a `file` node + doc/text extraction in §4) rather than
  skipped — coverage degrades smoothly, never to an error. Current coverage (~6 languages) is the
  floor, not the ceiling.
- **Deterministic and offline (§2.4).** No model touches source. Re-running on unchanged files
  reproduces the graph byte-for-byte; a changed file **supersedes** its prior entities/edges
  (§2.5), so the structural layer is stable and auditable.
- **Standalone surface (§2.1).** `rigger graph build` no longer presumes a run: it opens (or
  creates) the per-project graph, ingests the tree, and is the entry the dash and the report read.
  The existing seeded `rigger graph --around` becomes one query over the result.

_Code:_ `src/grounder/symbols/` (the tree-sitter extractor, re-expressed as an
event-emitting ingest that folds into the projection), `src/main.rs` `cmd_graph` (a `build`
subcommand + cold-checkout entry), `src/contextgraph/` fold arms.

## 4. Workstream B — Concept extraction: the semantic layer

**Target state:** a model pass over the project's **prose** — documents, comments, and rationale —
distills the distinct ideas into first-class `concept` nodes and wires them, many-to-many, to the
code and documents that realize and reference them.

```
   design-doc "spec 27: disposition-expiry + consolidation"     ← ONE document …
        │  model reads the prose, splits it into distinct ideas
        ├──────────────► concept: context-pruning      { definition: "bounding retained context …" }
        └──────────────► concept: consolidation        { definition: "sleep-phase distilling …" }   ← … TWO concepts

   concept: context-pruning ──REALIZES──► expiry.rs, write_capped_findings, subgraph()   (many files)
   concept: knowledge-graph ──REALIZES──► contextgraph.rs, graph.rs, ingest, dash panel   ← spans specs 27,28,30
   decision "cap is a backstop" ──ABOUT──► concept: context-pruning
```

- **Fine and named (§2.2, §2.3).** The model returns, per concept: a stable id
  (`concept/<slug>`), a human name, a one-line definition, and the set of nodes it concerns. It is
  the only mechanism that reliably separates two concepts sharing a document — the split above is
  its whole reason to exist.
- **Project-agnostic prompting (§2.1).** The extractor is handed the project's own text and asked
  for *its* concepts; the prompt encodes no rigger-specific or domain-specific vocabulary. The same
  pass run on an unrelated repo yields *that* project's concepts.
- **Event-sourced + incremental (§2.5).** Extraction emits `ConceptExtracted` /
  `ConceptLinked` events folded by the shared `apply`. Re-running after docs change supersedes the
  affected concepts/edges (bi-temporal), so concept ids stay stable and history is preserved.
- **Confidence `INFERRED` (§2.4).** Concept edges are model-derived and tagged accordingly, so a
  reader can always separate the deterministic code graph from the interpreted concept graph.

_Code:_ a new `src/concepts/` extractor (prompt + parse + emit), new node kind `concept` and
edges `ABOUT` / `REALIZES` in `src/contextgraph/`, model backend via the existing agent-config
surface.

## 5. Workstream C — Leiden community lens: the coarse subsystem

**Target state:** deterministic **Leiden community detection** over the graph gives the *coarse*
lens — the subsystems that emerge from connectivity — auto-labeled, with a **resolution**
parameter that controls grain directly.

```
   run Leiden on the chosen edge-layer:
        edge-layer=structure  → communities ≈ code subsystems (module-ish)
        edge-layer=intent     → communities ≈ design groupings (cluster the SPECIFIES/GOVERNS layer)

   resolution knob = grain:
        coarse ──►  [ knowledge-graph ] [ review ] [ observability ] [ remediation ]      few big domains
        fine   ──►  [ ingest ][ traversal ][ dash ][ pruning ][ expiry ][ gates ] …       many communities

   each community auto-labeled by its most-central node (highest-degree doc/concept in the group)
```

- **Why Leiden.** It is the modern successor to earlier modularity methods and guarantees
  well-connected communities (it repairs the disconnected-community defect of Louvain), so a
  labeled community is always a genuinely cohesive group. The **resolution** parameter *is* the
  configurable grain of §2.6 — one knob spans "a handful of domains" to "one community per idea".
- **Deterministic (§2.4).** Seeded and run over the projection, community assignment is
  reproducible; it folds in as `CommunityAssigned` events so the labels are themselves
  rebuildable graph facts, not a transient render-time computation.
- **Complements, never replaces, concepts (§2.3).** Communities are *structural* — they recover
  subsystems, not sub-document ideas. The fine "two concepts in one doc" split stays Workstream
  B's job; C gives the coarse map you zoom out to.

_Code:_ a new `src/community/` module (Leiden over the projection's edge sets, parameterized by
edge-layer + resolution), `CommunityAssigned` fold arm in `src/contextgraph/`.

## 6. Workstream D — Parameterized exploration in the dash

**Target state:** the read-only dash KG panel becomes a **parameterized explorer** (§2.6):
concept-first by default, every axis selectable, drill crossing axes. It renders the projection
only — the conductor stays the sole mutation authority, the page stays self-contained (no CDN, no
build step, inline SVG), per the dash charter.

```
┌ knowledge graph ──── lens:[concept ▾]  layer:[intent ▾]  grain:[━━━●━━] fine  tier:[EXTRACTED ▾] ┐
│  overview — the whole graph as concepts (nodes sized by attached-knowledge, edges = shared code) │
│                                                                                                  │
│     ( knowledge-graph )══════( context-pruning )        ( review )────( remediation )            │
│            ▲  click a concept to drill                        ▲                                   │
│            │                                                                                      │
│  drill ▼  lens stays "concept", contents RE-GROUP by file:                                        │
│     overview › knowledge-graph                                                                    │
│        [ contextgraph.rs ] [ graph.rs ] [ dash.rs ] [ conductor.rs::ingest ]  ← every file it's in│
│            └ drill a file → its entities (leaves, seed a neighborhood)                            │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

- **Concept-first, files-inside (§2.2).** The default overview groups by concept; drilling a
  concept re-groups its members by file, and drilling a file yields entities. "Every file the
  knowledge-graph concept is in" is exactly the first drill — a one-hop `REALIZES` traversal.
- **Every parameter live (§2.6).** `lens` switches the top-level axis (concept / file / kind);
  `edge-layer` toggles intent vs structure (the switch that *makes concepts appear* vs the raw
  code map); the `grain` slider is Leiden's resolution; the `tier` filter toggles
  `EXTRACTED`/`INFERRED`/`AMBIGUOUS`.
- **Graph-shaped, not a tree.** Because a node serves many concepts (§2.2), the overview stays a
  graph — concepts as nodes, edges = machinery two concepts share — so shared code is visible as a
  concept-to-concept edge rather than hidden by a single-parent tree. A drilled node that also
  belongs to other concepts is flagged, not duplicated silently.
- **Bounded render.** Whole-graph node counts are large; the panel always renders an aggregated
  level (concepts, or a community, or a drilled group) and drills on demand — never every raw node
  at once.

_Code:_ `src/dash.rs` (a parameterized `/api/graph?lens=&layer=&resolution=&tier=&path=` route
over the projection) + `src/dash.html` (the lens/layer/grain controls, the concept overview, and
cross-axis drill in the inline SVG renderer).

## 7. Workstream E — Concept report + query surface

**Target state:** the graph is legible without the dash, on any project, from the command line.

- **`rigger graph report`** emits a concept report: the project's key concepts (name, definition,
  the files and documents that realize each), and the **surprising cross-concept / cross-file
  connections** — edges that bridge otherwise-distant communities, the "this touches that?"
  findings a reader would never grep for. Deterministic, regenerable, project-agnostic.
- **Concept-aware query.** The existing `query` / `path` / `explain` surface extends to concept
  nodes: `explain concept/<slug>` returns its definition + the events that folded it + its
  realizing nodes; `path A B` may route *through* a concept; a query may seed on a concept.
- **Provenance preserved (§2.4/2.5).** Every reported concept and edge carries its confidence
  tier and the event that produced it, so a `INFERRED` concept is never presented as ground truth.

_Code:_ `src/main.rs` (`graph report`, concept args on `graph query/path/explain`), a report
renderer over the projection.

---

## 8. Delivery

Decomposed into atomic, loop-ready specs (spec-shape rules), run through the loop, in dependency
order. Each spec ends with both feature lanes green, and — the standing invariant — is validated
on a **cold, non-rigger fixture repo**, not only this tree.

1. **A · Language-agnostic code ingest** (§3) — the standalone `rigger graph build` off a cold
   checkout; the foundation B/C/D read. Also lands the graceful file-granularity fallback for
   ungrammared languages.
2. **B · Concept extraction** (§4) — the `concept` node layer + `ABOUT`/`REALIZES`, the
   sub-document split; depends on A's entities to attach to.
3. **C · Leiden community lens** (§5) — deterministic communities + resolution + labels; depends
   on A's edges (and reads B's concept edges when `edge-layer=intent`).
4. **D · Parameterized exploration** (§6) — the dash lenses, grain slider, cross-axis drill; the
   visual layer, riding on A–C. Hand-verified visually (the loop is blind to rendered output).
5. **E · Concept report + query** (§7) — the CLI legibility surface over A–C.

The existing unified-KG substrate (context-management addendum §6) is a prerequisite: this
addendum adds the concept layer, the community lens, the generality, and the parameterized surface
on top of that projection, and changes none of its invariants.

## 9. Acceptance (targets)

- **Cold, foreign repo.** `rigger graph build` on a checkout in a supported language, with **no
  dev-loop history and no rigger vocabulary present**, produces a graph with code entities,
  document nodes, and named concept nodes — proven on ≥3 diverse repos across ≥3 languages.
- **The sub-document split.** A single document that defines two ideas yields **two distinct
  `concept` nodes**, each `REALIZES`-linked to different code — the "context-pruning vs
  knowledge-graph in one spec" case, verified on a fixture.
- **One-hop concept→files.** "Every file concept X is realized in" returns in a single traversal
  (§2.2), driving the first drill in the dash.
- **Deterministic layers.** The code layer and the Leiden communities reproduce exactly on a
  re-run of unchanged input (§2.4); only the model layer may vary, and it is tagged `INFERRED`.
- **Resolution controls grain.** Sweeping the resolution parameter monotonically changes the
  community count from a few domains to many fine communities (§5), each auto-labeled.
- **Event-sourced + scoped.** Concept/community nodes and edges are rebuildable from the log,
  supersede-not-delete on re-extraction, and carry the project namespace — a two-project fixture
  shows no leakage (§2.5).
- **Legible headless.** `rigger graph report` emits the key concepts + surprising cross-concept
  connections on any project, deterministically (§7).
