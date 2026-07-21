# Reference Architecture Addendum — Context Management

**Status:** design, approved for planning. Quantitative claims are grounded in a measurement
of the live `.rigger/graph.db` taken 2026-07-15; figures marked *(est.)* are not measured.
**Scope:** an addendum to `docs/architecture.md`. It makes the context rigger injects into
each agent cheaper, sharper, and bounded — across the three timescales of context management
(per-prompt, between-runs, and the retrieval substrate) — without weakening the event-sourced
core or the multi-project isolation the store already guarantees.

---

## 1. Problem (measured)

Each spawned agent's prompt is assembled in `build_prompt_with_failure` (`src/conductor.rs`):
grounder seed refs + `graph_context(seed)` (decisions, lessons, findings pulled from the
context graph, scoped to the unit's files) + protocols. Three measured facts about the graph
that feeds that injection:

- **The injectable pool is ~1.8x the cap, so truncation is load-bearing.** The raw
  decision+finding+lesson text is **149.2 KiB** (decisions 84.6 + lessons 36.1 + findings
  28.4), but `graph_context` caps at **~84 KiB/spawn** (budgets: decisions 24, lessons 12,
  findings 48 KiB). So **~65 KiB is truncated every spawn**, and a recency/relevance ranking
  under pressure — not relevance to the unit — decides what an agent never sees.
- **The bi-temporal invalidation machinery is built but entirely unused: 0 of 1582 edges are
  invalidated.** Every finding and decision, once emitted, stays `valid_to IS NULL` (live)
  forever. A finding an adjudicator already dispositioned (addressed/refuted) keeps
  re-injecting into every later prompt about those files, across runs, permanently.
- **Cross-run accumulation is the real bloat, not within-run duplication.** `reset --runs`
  pruned **2931 of ~3362 nodes (87%)** as dead-run cruft. Within the clean remainder,
  exact-duplicate text is low (findings 0%, decisions 0%, lessons 21%). So duplication is a
  *cross-run* phenomenon; `reset --runs` sheds it manually, nothing bounds it automatically.
- **Grounding does not reach design intent.** The graph already carries `GOVERNS` (204),
  `ABOUT` (88), `GATED_BY`, `TOUCHES`, `BLOCKS` edges — it is a real knowledge graph of the
  dev-loop — but the code structure (the `symbols` grounder) and the docs (handbook /
  `architecture.md`) live in *separate* stores, so an agent cannot traverse from a file it
  touches to the handbook rule that governs it. The rule-7 / spec-authoring failures this
  project hit are "the agent did not know the governing rule/intent" failures.

### Non-goals / anti-fixes
- Do NOT prune the event log — it is the source of truth (§2.1).
- Do NOT scope grounding to the active run by default — cross-run memory is deliberate;
  the fix is expiry + consolidation + provenance, not amnesia.
- Do NOT delete `turbovec` — fuzzy natural-language criterion queries need vectors (§2.5).
- Do NOT take on a third-party graph engine/service as a dependency — build the data model natively (§2.5).

---

## 2. Load-bearing decisions — invariants the design must carry

### 2.1 The event log is the source of truth; the graph is a rebuildable projection
Every context-management operation — expiry, dedup, consolidation, KG folding — mutates the
**graph/projection only**, never the event log. `reset --runs` already states this ("the
event log is untouched"). The graph must stay rebuildable from the log by an idempotent
`apply`, and supersession invalidates (sets `valid_to`), never deletes. This is what lets a
corrupted or pruned graph be rebuilt, and it is why a mutable `graph.json`-on-disk +
git-union-merge storage model is rejected.

### 2.2 Project-scoping is an invariant — enforced, not incidental
The event store is namespaced by project identity (`proj-<identity>-`, `eventstore/namespace.rs`):
one backend (embedded sqlite OR a shared KurrentDB) holds many projects without their streams
mixing. `reset --runs` reads only its own namespaced stream and prunes a **per-project-local**
`graph.db`, so it is project-scoped by two independent mechanisms today. **The moment the
unified KG (§6) puts graph nodes/edges into a shared backend, "isolated by construction"
disappears:** every node and edge must then carry the `proj-<identity>-` namespace, and every
operation (reset, consolidation, expiry, traversal) must filter by it — or one project's
operation mixes or clobbers another's. Project-scoping is load-bearing on the KG, exactly as
it is on the event store.

### 2.3 Run-scoping is an invariant — consolidation and expiry must carry run provenance
Run membership is derived today from `RunStarted` event-position boundaries; the graph has no
`run_id` column. Any operation that merges or expires nodes across runs (consolidation §5,
disposition-expiry §3) must carry run provenance, so a finding dispositioned in run A does not
silently suppress a legitimately re-raised finding in run B, and a fresh run never grounds on
a superseded prior run's state. This is the same gap that produced the run-boundary corruption
class; closing it is a prerequisite for §5 and §6.

### 2.4 Safe-superset recall is a correctness invariant, not a token cost
Pruning, dedup, and tier filtering apply to **prompt rendering only** — never to the safety
consumers `partition_by_blast_radius`, `partition_wave`, `route_review_tier`,
`stale_downstream_units`, which require over-inclusion. The `safe` view stays an uncapped
grep-superset; any confidence-tier mapping (§6) must keep the wide tier a superset of grep.
Dropping a reference a safety consumer needs is a correctness regression, not a saving.

### 2.5 Keep vectors for NL retrieval; build the KG data model natively
Criterion queries ground on spec prose that often names no symbol; graph traversal cannot
answer them, so `turbovec` stays (or embeddings attach to KG nodes). rigger builds the KG
*data model* natively — one typed node taxonomy (code + docs + rationale + decisions),
confidence-tagged edges (`EXTRACTED`/`INFERRED`/`AMBIGUOUS`), community detection — and rejects
the storage patterns that would compromise it: a `graph.json` source-of-truth, git-union-merge,
and any non-Rust service dependency (a `cargo install` crate takes none).

---

## 3. Workstream A — Disposition-expiry: activate the unused invalidation machinery

**Highest *measured* ROI.** The bi-temporal `valid_to` machinery exists and is used by 0% of
edges. When an adjudicator dispositions a finding (upheld-and-addressed, or discarded) or a
decision supersedes a prior one, fold that as an **invalidation** — set `valid_to` on the
finding's `ABOUT` edge (mirroring the decision `REL_SUPERSEDES` path) — so `subgraph`'s
existing `valid_to IS NULL` filter prunes resolved findings for free. Run-scoped per §2.3: a
disposition invalidates within its run's provenance, never suppressing a re-raise in a later
run.

**Impact:** shrinks the 149 KiB pool below the 84 KiB cap → truncation stops being
load-bearing (no relevant item silently dropped), and agents ground on *live* findings.
Demotes the hand-tuned cap constants to backstop assertions. Directly relieves the token /
usage-limit pressure this project measured.

_Code:_ `contextgraph/sqlite.rs` `fold`/`apply` (add the disposition-invalidation arm),
`graph_context`/`write_capped_findings` (`src/conductor.rs`), the adjudicator verdict
(`upheld`/`discarded`) already recorded in the log.

## 4. Workstream B — Safe dedup + dependency-restore at injection

At prompt assembly, normalize-and-dedup the injected slice (measured value: ~21% on lessons,
low on findings/decisions post-reset, higher cross-run before consolidation lands), and
**restore any dependency** a kept item references so nothing fact-complete is dropped. Applies
to prompt rendering only (§2.4). Modest alone; a cheap complement to §3 and a safety net for §5.

_Code:_ the `write_capped_*` path (`src/conductor.rs`), a normalized-text dedup with a
graph-edge-based dependency-restore pass (structural, not the external article's regex).

## 5. Workstream C — Sleep-phase consolidation: a findings/decisions distiller

The only capability that **bounds cross-run growth automatically** (measured: 87% of the graph
was dead-run cruft; `reset --runs` is the manual shed). A between-runs distiller — a sibling of
`playbooks.rs`, which already consolidates `LessonLearned` into a rebuildable projection — folds
**older-than-current-run** findings/decisions into per-file digest nodes, raw kept and
retrievable via `rigger peers`. It runs as a projection over the log (§2.1), carries run
provenance (§2.3), and preserves `LessonLearned`. This is the automatic form of what
`reset --runs` does by hand and keeps grounding lean over months, not just after a manual prune.

_Code:_ a new distiller module modeled on `src/playbooks.rs`; folds into the graph projection;
shares the `RunStarted`-boundary attribution with `reset --runs`.

## 6. Workstream D — Unified event-sourced knowledge graph

**The end state is a SINGLE knowledge graph that encapsulates both (a) the dev-loop's decision
stream — decisions, findings, lessons, folded from the event log — and (b) everything
structurally known about the codebase and project — code entities and their structure, the
docs, and design rationale.** It is one queryable, rebuildable, event-sourced projection you
traverse across; the event log stays the source of truth and the `turbovec` vector index stays
the complementary semantic layer for symbol-free NL queries (§2.5), attached to nodes or
alongside — not folded into them.

### 6.1 The node taxonomy — one typed vocabulary for three domains

Every thing rigger knows becomes a typed node in ONE graph. The vocabulary spans three domains,
and sharing it is what lets a single query cross from code to decision to design intent:

```
DOMAIN                    NODE KIND      SOURCE                          ROLE IN THE GRAPH
────────────────────────  ─────────────  ──────────────────────────────  ─────────────────────────────
codebase structure        code-entity    tree-sitter (fn/type/mod)       the WHAT - the code itself
                          file           the module tree                 container for code-entities
design & project intent   design-doc     RA / architecture.md / addenda  the DESIGN-INTENT layer
                          arch-decision  load-bearing decision / ADR     constrains the code
                          handbook-rule  spec-shape / loop discipline    governs authoring
                          rationale      `# WHY:` / `# NOTE:` inline      local intent, on a code node
dev-loop decision stream  decision       DecisionMade event              the WHY - chosen, w/ alternatives
                          finding        ReviewFinding event             a defect/observation about a node
                          lesson         LessonLearned event             durable cross-run knowledge
orchestration             unit           UnitProposed event              a planned slice of work
                          gate           gate lifecycle                  a quality checkpoint
                          agent          agent lifecycle                 who did the work
                          artifact       produced output                 what a unit produced
```

The graph is ONE event-sourced projection over all three domains: code structure, design intent,
and the decision stream share a single id space, one query surface, and one lifecycle. Structure
is ingested AS EVENTS alongside decisions (§6.3), and §2.1 fixes why the graph is a projection
rather than a fourth store.

**The design-intent layer is first-class and deliberately in scope; user-facing docs are not.**
The `design-doc` / `arch-decision` nodes are the reference architecture (this document and its
siblings), `architecture.md`, the addenda, `design-intent-gaps`, and every load-bearing
decision — *the why the code is the way it is*. They are the single highest-value doc knowledge
to graph: grounding an agent on the DESIGN INTENT of a subsystem — not just its code and
operating rules — is what an implementer or reviewer most needs and today most lacks (the
rule-7 / plan-critique failures were design-intent-blind). Docs intended for END USERS (how to
*drive* rigger) are deliberately OUT of scope — they describe usage, not design, and would add
noise to code-grounded traversal. Note the intended recursion: **a reference architecture
becomes a set of nodes in the very graph it specifies**, `SPECIFIES`-linked to the code it
designs — so the RA is itself queryable, and an agent editing a subsystem reaches the RA section
that designed it and the load-bearing decision that constrains it.

### 6.2 The edge taxonomy — typed, confidence-tagged, bi-temporal

Edges are typed by relation and tagged by extraction confidence
(`EXTRACTED`/`INFERRED`/`AMBIGUOUS` — rigger's `precise`/`safe` split made first-class).
Every edge carries `valid_from`/`valid_to` (bi-temporal, already in the schema).

```
FROM ──rel──► TO                         CONFIDENCE     meaning
──────────────────────────────────       ───────────    ─────────────────────────────────
code  ──calls / imports / inherits──► code   EXTRACTED   explicit in source (tree-sitter)
code  ──depends_on──────────────────► code   INFERRED    derived (transitive / re-export)
code  ──(grep-visible ref)──────────► code   AMBIGUOUS   macro body / reflection string / dynamic
design-doc    ──SPECIFIES / DESIGNS──► code   EXTRACTED   an RA / architecture section designs this subsystem
arch-decision ──CONSTRAINS───────────► code   EXTRACTED   a load-bearing decision constrains this code
handbook-rule ──GOVERNS──────────────► code   EXTRACTED   a spec-shape / loop rule governs this file/entity
rationale     ──explains─────────────► code   EXTRACTED   a WHY-comment attached to an entity
design-doc    ──references────────────► doc    EXTRACTED   markdown link / ADR citation (doc→doc, doc→code)
decision    ──ABOUT─────────────────► code   EXTRACTED   a decision concerns this entity
finding     ──RAISED / ABOUT────────► code   EXTRACTED   a review finding about this entity
decision    ──SUPERSEDES─────────────► decision          bi-temporal invalidation (sets valid_to)
unit ──needs──► unit,  unit ──GATED_BY──► gate,  unit ──ASSIGNED_TO──► agent   (orchestration)
```

The confidence tier IS the two-view blast radius, unified: the **precise seed** for a prompt is
the `EXTRACTED` sub-graph; the **safe superset** the safety consumers need (§2.4) is
`EXTRACTED ∪ INFERRED ∪ AMBIGUOUS`, which must remain a superset of the grep union. One edge
set, two filters — replacing the hand-rolled `BlastRadius{precise,safe,serialize}` struct and
the documented seed-vs-precise divergence.

### 6.3 How it is built — structure ingested AS EVENTS, folded like decisions

The code and doc knowledge is made event-sourced: an extraction pass emits events, and the
same idempotent `apply` that folds a `DecisionMade` folds them into the graph. Nothing is a
mutable side artifact (§2.1).

```
   EVENT LOG  (source of truth · per-project namespaced `proj-<id>-` · append-only)
   ┌─────────────────────────────────────────────────────────────────────────────┐
   │ dev-loop stream:   DecisionMade  ReviewFinding  LessonLearned                 │
   │ codebase ingest:   CodeEntityExtracted  EdgeInferred   (per tree-sitter pass) │
   │ docs ingest:       DocConceptExtracted  DocLinkExtracted                      │
   └───────────────────────────────┬─────────────────────────────────────────────┘
                                    │  apply()  — idempotent per position,
                                    │            supersede-not-delete (sets valid_to)
                                    ▼
        ╔══════════════════ UNIFIED KNOWLEDGE GRAPH (projection) ══════════════════╗
        ║  nodes {code-entity, doc-concept, rationale, decision, finding, lesson}  ║
        ║  edges {calls, GOVERNS, ABOUT, references, needs, …}  bi-temporal+tiered ║
        ╚═══════════════════════════════════┬═════════════════════════════════════╝
                                            │  subgraph(seed, depth)
             seed = the unit's blast radius │  traversal, tier-filtered
                                            ▼
        confidence tier:  EXTRACTED → prompt seed   |   ∪INFERRED∪AMBIGUOUS → safety consumers
                                            │
                                            ▼
                          AGENT PROMPT  (bounded, fact-complete, design-intent-aware)
```

Because it is a projection, the whole graph — code structure included — is **rebuildable from
the log**, and re-extraction after a file changes SUPERSEDES the old entity's edges rather than
overwriting them. That is strictly stronger than an overwrite-and-git-merge `graph.json` on
disk, and it is why rigger builds the data model natively rather than adopting that storage (§2.5).

### 6.4 Why bi-temporal matters — the graph knows what it knew, when

Because supersession sets `valid_to` instead of deleting, the graph is queryable at any point
in the log — grounding reads the live slice; an audit reads a historical slice.

```
run A:   CodeEntityExtracted foo()   ──►  edge (valid_from=posA, valid_to=NULL)     ← live
run B:   foo() refactored →
         CodeEntityExtracted foo'    ──►  fold sets prior edge  valid_to = posB     ← superseded
                                          new edge (valid_from=posB, valid_to=NULL) ← live
   grounding at run B      → filter `valid_to IS NULL`            → sees foo', never foo
   "what did run A see?"   → filter `valid_from ≤ posA < valid_to`→ sees foo
```

This is the SAME mechanism §3 (disposition-expiry) uses for findings and §2.3 (run provenance)
needs — one bi-temporal model serves grounding, audit, expiry, and consolidation.

### 6.5 How grounding uses it — one traversal, not a two-store stitch

Today `graph_context` runs `graph.subgraph(seed, 2)` for decisions and a SEPARATE grounder
call for code, then stitches them. Unified, a single seeded traversal reaches everything about
a touched file. Example — an agent whose blast radius includes `harvest_proposed`:

```
                    ┌ doc-concept: handbook rule 3 "one criterion,
                    │  one observable behavior" ┐
                    │            GOVERNS         │
                    ▼                            ▼
   decision ──ABOUT──►  conductor.rs::harvest_proposed  ◄──RAISED── finding
   "match on stable       (code-entity, seed)                       "rule-7 dup pair"
    criterion id"          │ calls        │ depends_on
                           ▼              ▼
                   baseline_units()   normalize_ws() ◄─explains─ rationale
                                                     "# WHY: verbatim so a paraphrase
                                                      runs as an EXTRA unit"
```

One `subgraph(harvest_proposed, depth)` returns: the decision governing it, the finding about
it, **the handbook rule that governs it, the design rationale of its dependency, and — via
`SPECIFIES`/`CONSTRAINS` — the reference-architecture section and load-bearing decision that
DESIGNED this subsystem** — the last three impossible in the structured slice today. The agent
is grounded on the *design intent*, not just the code and prior decisions.

### 6.6 Why it works — the rationale, tied to measured failures

- **One graph removes the store-stitch and the two-view contortions.** `graph_context` stops
  merging a grounder result with a graph result; `BlastRadius{precise,safe}` becomes two tier
  filters over one edge set; the seed-vs-precise divergence (~150–200 LOC of documentation and
  a `record_blast_radius` workaround) dissolves.
- **Deterministic design-intent grounding attacks the failure class we measured.** The rule-7 /
  plan-critique escalations were "the agent did not know the governing rule." A `GOVERNS`
  traversal delivers that rule by structure, not by hoping it is vector-similar to the criterion
  and survives the 84 KiB truncation. This is the single highest-value NEW capability.
- **Traversal-bounded context beats cap-and-truncate.** Today a 149 KiB flat pool is ranked and
  truncated to 84 KiB. A depth-bounded traversal from the seed returns *only what is structurally
  connected to the work* — smaller AND sharper, and it composes with §3 expiry and §5
  consolidation so the pool that reaches the cap is already the relevant slice.
- **Event-sourced keeps every invariant.** Rebuildable, superseded-not-deleted, project- and
  run-scoped (§2.1–2.3), auditable across time (§6.4). A mutable `graph.json` on disk would
  forfeit all of that; rigger takes the data model, not that storage engine.
- **Vectors stay for what graphs cannot do.** A symbol-free criterion query ("where is retry
  backoff handled") has no node id to seed on; `turbovec` answers it and can seed the traversal.
  Graph and vectors are complementary layers, not competitors (§2.5).

**Impact:** ~2000–2400 LOC removed *(est.)* — `symbols` (2,607) folds into the projection, and
the two-view `BlastRadius` struct + the seed-vs-precise divergence workaround collapse into
confidence-tier filters (`EXTRACTED` = precise seed; `EXTRACTED∪INFERRED∪AMBIGUOUS` = the safe
superset, which must stay a grep-superset per §2.4). The hub-percentile heuristic gives way to
community detection. And the genuinely new capability: **deterministic design-intent grounding**
— an agent whose blast radius touches file F traverses `F → GOVERNED_BY → handbook-rule` and
injects the governing rule by traversal, not embedding luck, attacking the rule-7 /
spec-authoring failure class. `turbovec` stays for NL retrieval (§2.5).

**Gated on** §2.2 (project-scoping enforced on shared-backend nodes/edges) and §2.3 (run
provenance) — the two invariants that make a shared, cross-run KG safe.

_Code:_ `src/contextgraph/` (new node kinds + fold arms), `src/grounder/symbols/` (re-expressed
as an event-emitting extractor that folds into the projection), `graph_context`
(`src/conductor.rs`) unified traversal, docs ingestion.

---

## 7. Workstream E — the knowledge graph in the dash

The unified KG (§6) is only as useful as it is inspectable. Extend the always-on `rigger dash`
(spec 19b) with an interactive KG panel that gives the full graph-inspection capabilities — *see* the
graph, *trace* query paths, *find* the god nodes — over rigger's OWN event-sourced graph,
read-only. Each capability — `query` / `path` / `explain` / `get_neighbors` / `shortest_path` /
force-graph / community-detection — maps onto rigger's grounding provenance.

- **Graph view.** A force-directed, seeded-neighborhood view. Nodes colored by KIND (code / doc
  / decision / finding / rationale); edges colored by CONFIDENCE TIER (`EXTRACTED` solid,
  `INFERRED` dashed, `AMBIGUOUS` dotted) and dimmed when superseded. Scoped and capped — a
  seeded neighborhood or the current run's subgraph, with a node cap — never the whole graph at
  once.
- **Query path / shortest_path.** Pick two nodes (or a grounding query) and highlight the PATH
  between them. The killer use is grounding provenance: *"why did this decision/rule land in
  `u-domain`'s prompt?"* → the path from the unit's seed file to the injected node, so what an
  agent grounds on stops being a black box.
- **God nodes / hubs.** Surface the highest-degree and community-bridging nodes — a file
  everything depends on, a rule that governs everything — via degree + Leiden community
  detection (the same signal §6's tier logic uses, made visible). Click a hub to see what it
  anchors; these are exactly the nodes whose over-inclusion the old hub-percentile heuristic
  guessed at.
- **Neighbor exploration (`get_neighbors`).** Click a node to expand one hop at a time, so a
  large graph is explored incrementally instead of rendered whole.
- **`explain(node)`.** For a node, show its provenance: the event(s) that folded it, its
  `valid_from`/`valid_to`, the run that raised it, and its confidence tier — closing the loop
  from a graph node back to the log fact that created it.
- **Filters: tier · time · run · project.** Toggle `EXTRACTED`/`INFERRED`/`AMBIGUOUS`; show or
  hide superseded edges to TIME-TRAVEL (§6.4 — "what did the graph know at run A"); filter by
  run (live vs historical, spec 21 u3); and — the invariant — always one project (§2.2).
- **Grounding overlay.** For an in-flight unit, overlay the subgraph that actually seeded its
  prompt beside the current-blocker line (spec 19a) — so an operator sees WHAT an agent was
  grounded on next to WHY it is stuck.

Mockup — a new panel in the existing dash:

```
┌ knowledge graph ───────────────────────────── [project: rigger ▾] [run: live ▾] ┐
│ seed:[conductor.rs::harvest_proposed        ]  depth[2]  tier[EXTRACTED ▾]        │
│                                                                                   │
│   (rule-3)══GOVERNS══►[harvest_proposed]◄──RAISED══(finding: rule-7 dup)          │
│                            │ calls    │ depends_on                                │
│                     [baseline_units]  [normalize_ws]···explains···(# WHY note)    │
│                                                                                   │
│ god nodes (degree):  conductor.rs(128)  main.rs(96)  rule-6(41)  turbovec.rs(33)  │
│ path (harvest_proposed→rule-3):  harvest_proposed ─GOVERNED_BY→ rule-3            │
│ explain[normalize_ws]  kind=code  CodeEntityExtracted@8102  valid: live  EXTRACTED│
└───────────────────────────────────────────────────────────────────────────────────┘
```

**Constraints (inherit the dash charter):**
- READ-ONLY: the dash never mutates the store; the conductor stays the sole mutation authority.
  The KG panel reads the projection only.
- SELF-CONTAINED: no CDN / JS toolchain — the dash serves one `include_str!` HTML page (spec 01);
  the force-graph is a small hand-rolled inline SVG/canvas renderer, no external dependency.
- SCOPED + CAPPED: a seeded neighborhood plus a node cap; the whole graph is never rendered (it
  can be large — the pre-reset graph held 3362 nodes).
- PROJECT + RUN SCOPED (§2.2/§2.3): the panel shows exactly one project's graph with a run
  filter; a shared-backend deployment never leaks another project's nodes.

_Code:_ `src/dash.rs` + `src/dash.html` (a new KG panel + a read-only
`/api/graph?seed=&depth=&tier=` endpoint over the projection), reusing the `subgraph` traversal
and confidence tiers from §6 and the live/historical labels from spec 21.

---

## 8. Delivery

Decomposed into atomic loop-ready specs (spec-shape rules), run through the loop, in ROI /
dependency order:

1. **Disposition-expiry** (§3) — smallest, highest measured ROI, reuses `valid_to`. Ships the
   run-provenance carrier (§2.3) it and §5 need.
2. **Dedup + dependency-restore** (§4) — cheap complement.
3. **Consolidation distiller** (§5) — bounds cross-run growth; depends on §3's provenance.
4. **Project-scoping enforcement on the graph** (§2.2) — the prerequisite that makes a shared
   backend safe; must land before §6 moves the graph off per-project-local storage.
5. **Unified KG** (§6) — the substrate; largest; gated on 4 + §2.3.
6. **KG in the dash** (§7) — the visualization + grounding-provenance panel; depends on the
   unified KG (§6); read-only, self-contained, project/run-scoped.

Each spec ends with both feature lanes green. The docs self-document via the mechanism spec 20
lands (the drift check + commit-time regen), so this addendum's own facts stay in sync.

## 9. Acceptance (measured targets)

- Injected `graph_context` slice drops **below the 84 KiB cap** on a hot unit (from the
  measured 149 KiB pool), so no truncation of relevant context — verified against a real
  `graph.db`.
- Edge invalidation is **non-zero** and tracks dispositions (from the measured 0%).
- Cross-run node count stays bounded across N runs without a manual `reset --runs` (§5).
- A `reset --runs` / consolidation / traversal run against a backend holding two projects
  touches **only** the current project's nodes — proven by a two-project fixture (§2.2).
- An agent can retrieve the handbook rule governing a touched file by traversal (§6).
- The dash renders a seeded KG neighborhood with query-path, god-node, `explain`, and
  grounding-provenance views, self-contained and scoped to one project (§7).
