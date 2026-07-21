# Context management: prompt pruning, a unified knowledge graph, and sleep-phase consolidation, 2026-07-15

Three external concepts, evaluated against this tree, on how rigger assembles and stores the
context it feeds agents: (1) a safe prompt-pruning layer, (2) a full code+docs knowledge
graph, (3) a wake/sleep memory-consolidation paradigm. They are not competitors; they are the
three timescales of context management (between-runs, the retrieval substrate, per-prompt).
This document is a feed for candidate specs. Load-bearing LOC and byte figures were measured
against the code (`wc -l`, in-source measurements); token/percentage gains are ESTIMATES and
are marked as such - measure the live `graph.db` before trusting them.

**Meta-finding:** rigger already ships partial, deeper versions of all three. `contextgraph`
is an event-sourced knowledge-graph projection (proof that a full KG can be event-sourced,
which a mutable on-disk graph store is not). The two-view `precise`/`safe` blast radius is confidence
tiers discovered independently. `playbooks.rs` is a sleep phase for lessons. The roadmap is
therefore to FINISH patterns already started - in-tree, in Rust, on the event-sourced core -
not to adopt an external dependency.

---

## The three concepts

**1. Safe prompt pruning** (deterministic, stdlib-only, runs just before prompt
serialization). Three passes over structured messages: expired-context elimination (newest
occurrence of a tool-call key wins; older superseded results dropped); duplicate elimination
(normalized near-duplicate passages dropped, first kept); dependency restoration (never drop
a fact a later kept message still references). Reported: 27-34% token reduction on RAG /
tool-heavy agents, 100% required-fact preservation, sub-50ms overhead, idempotent. Stated
weakness: dependency detection is literal identifier matching, not understanding.

**2. A full knowledge graph** (tree-sitter code entities + design rationale from `# WHY:`/ADR
comments as first-class nodes + doc concepts from markdown/PDF, all in ONE traversable graph;
edges tagged `EXTRACTED`/`INFERRED`/`AMBIGUOUS`; Leiden community detection; markdown links ->
`references` edges). Explicitly anti-embedding ("a real graph you traverse, no vector store"),
deterministic code extraction (no LLM credits for code). Storage is a mutable `graph.json`
with a git union-merge driver; runtime is a separate service. Pre-1.0.

**3. Wake/sleep consolidation** (a research paper: offline distillation of transient
in-context knowledge into durable memory without catastrophic forgetting, via a
multi-frequency memory hierarchy plus self-generated "dreaming" curricula). Weight-bound - the
literal mechanism needs model-weight access rigger does not have. Only the PARADIGM transfers:
consolidate the memory population between runs so per-prompt assembly starts from less.

## The unifying frame: three timescales

```
event log (source of truth)
   │  between runs ("sleep", concept 3): consolidate - dedup and distill the
   │  finding/lesson population into per-file digests; raw kept, retrievable
   ▼
one confidence-tagged knowledge graph (concept 2): code + docs + rationale +
   decisions, EXTRACTED/INFERRED/AMBIGUOUS tiers = the precise/safe split formalized
   │  per spawn: ground → EXTRACTED tier seeds the prompt; wider tier feeds
   │  the safety consumers (partition/route/staleness)
   ▼
prompt assembly → safe pruning pass (concept 1): drop superseded/dispositioned,
   dedup near-duplicates, dependency-restore via graph edges (structural, not regex)
   ▼
agent prompt (smaller, provably fact-complete)
```

Concept 3 shrinks the population so concept 1 only fights intra-run redundancy; concept 2
upgrades concept 1's safety pass from string matching to graph reachability. None subsumes
another: consolidation is slow/offline/lossy; pruning is fast/per-prompt/lossless; the graph
is the substrate both run over.

## Where this lands in the code

Each spawn's prompt is assembled in `build_prompt_with_failure` (`src/conductor.rs:5994`):
prior-failure block + grounder refs (`gr.ground(query, 8)`, `:6002`, `GROUNDED_SEED_K=8`
`:78`) + `graph_context(seed)` (`:6014`/`:6023`), which runs `graph.subgraph(seed, 2)`
(`:6028`; recursive CTE `src/contextgraph/sqlite.rs:78`) and renders three budgeted sections
via `write_capped_section` (`:6690`) wrapped by `write_capped_decisions/lessons/findings`
(`:6785`/`:6805`/`:6830`). Hand-tuned budgets (`:6623-6652`): decisions 12 verbatim / 24KiB,
lessons 12 / 12KiB, findings 24 / 48KiB. Measured pre-cap blowups quoted in-source: findings
~95KiB about `conductor.rs`, ~187KiB about `main.rs` (`:6038`). The hard cap is 84KiB ~=
21k tokens per spawn on saturated files.

Measured LOC (`wc -l`): `contextgraph` 822 (mod 171 + sqlite 651); grounders 5,762 (turbovec
2,529; symbols 2,607; shared mod 626); eventstore 2,135; `playbooks.rs` 343.

## The full-KG reframing (the important correction)

The naive reading maps the external graph onto the `symbols` grounder's edges. That is the
small story. A FULL knowledge graph is a single unifying substrate, and rigger today splits
knowledge across four disconnected stores. Under the full-KG lens:

- **`symbols` grounder (code structure) - FULLY subsumed** (`-2,607 LOC`, minus ~200-400
  re-expressed as tier + community logic). This is exactly a code KG's core.
- **`turbovec` (code semantics) - KEPT, not deletable.** Criterion queries ground on spec
  prose (`ground_query`, `:5805`) that often names no symbol; graph traversal cannot answer a
  symbol-free query. Attach embeddings to nodes if desired, but the vector path stays.
- **`contextgraph` (decisions/findings/lessons) - MERGED, not deleted; it GROWS.** Its value
  is the bi-temporal event-sourced projection (`valid_to` supersession, idempotent `apply`,
  rebuildable - `sqlite.rs:62-131`). It becomes the substrate the code graph folds INTO, so a
  single `subgraph` traverses from a code node to BOTH its governing decisions AND its
  structural neighbors - replacing the two-store stitch at `:5994-6050`.
- **Docs prose (handbook, `architecture.md`) - the genuine gain.** Correction to an earlier
  claim: docs ARE embedded today, because turbovec's `collect_files` (`turbovec.rs:1319`)
  reads every UTF-8 file with no extension filter. But they are flat chunks, not nodes: no
  edge links a handbook rule to the code it governs, and doc prose never reaches the
  STRUCTURED injection (`graph_context`), only the anonymous top-k list. A full KG makes
  markdown into concept/rule nodes with `GOVERNS`/`references` edges to code.

**The new capability rigger lacks:** an agent whose blast radius touches file F could traverse
`F -> GOVERNED_BY -> handbook-rule` and inject "the rule that governs this file" by
deterministic traversal instead of embedding luck. This directly attacks the spec-authoring /
plan-critique failure class (a demanded mitigation owned by two units with no exclusion; a
criterion split into duplicate units) - those are "the agent did not know the governing rule
or intent" failures. Magnitude is UNMEASURED (needs a labeled corpus); the mechanism is sound.

**Event-sourcing is NOT at odds with a full KG - rigger already proves it.** `contextgraph` is
a full-ish KG built purely as an event-log projection. Extending it to code+docs means
ingesting structure AS EVENTS (`CodeEntityExtracted` / `EdgeInferred` / `DocConceptExtracted`,
folded by the same `apply`); a re-extraction supersedes old edges via `valid_to`, exactly like
`REL_SUPERSEDES`. That is strictly stronger than an overwrite-and-union-merge on a mutable graph file.

**Keep the data model, reject the mutable-file storage.** Keep: one typed node taxonomy (code + docs +
rationale + decisions); confidence-tagged edges (a principled form of the precise/safe split);
Leiden communities (replacing the hub-percentile `serialize` heuristic,
`grounder/symbols/grounder.rs:15-18`); rationale-as-nodes. Reject: `graph.json` as source of
truth; the git union-merge driver (a mutable second source of truth - violates the
rebuildable-projection invariant); the Python runtime / MCP / HTML viz (a non-Rust dep into a
`cargo install` crate; the turbovec port exists to avoid exactly this); LLM-credit doc
extraction (this project has hit weekly usage limits).

## Candidate specs (ordered by ROI, all in-tree, in Rust)

1. **Finding disposition-expiry + normalized dedup at injection** (concept 1; smallest, safest,
   ~1-2 days). Fold an adjudicator's disposition of a finding as an invalidation of its `ABOUT`
   edge, mirroring `REL_SUPERSEDES` for decisions; `subgraph`'s existing `valid_to IS NULL`
   filter then prunes resolved findings FOR FREE. Add normalized dedup to
   `write_capped_findings`. Attacks the measured pain (usage limits) at an estimated ~30% of
   the saturated graph slice. FIRST STEP: measure the real duplicate/dispositioned ratio from a
   live `graph.db` - do not ship on the estimate. Demotes the hand-tuned caps (`:6623-6652`)
   from load-bearing truncation to backstop assertions.

2. **A findings/decisions distiller, sibling of `playbooks.rs`** (concept 3; ~300-500 LOC). A
   between-runs consolidation that folds older-than-current-run findings into per-file digest
   nodes (raw kept, retrievable via `rigger peers`). This is the ONLY item that bends the
   cross-run growth curve; without it caps or pruners stay permanently load-bearing. Must carry
   run provenance (see risks).

3. **Confidence-tagged edges + one blast-radius object** (concept 2, data model only). Replace
   the `BlastRadius` two-view struct + default-impl contortions (`grounder/mod.rs:135-191`) and
   the seed-vs-precise divergence doc/workaround (`conductor.rs:5863-5969`, ~150-200 LOC) with
   `EXTRACTED`/`INFERRED`/`AMBIGUOUS` tiers filtered per consumer; replace the hub-percentile
   heuristic with community detection. Do this when next touching `src/grounder/symbols/`.

4. **The unified event-sourced KG substrate** (concepts 2+3 combined; the endgame). Fold the
   `symbols` code graph into the `contextgraph` projection; ingest docs/rationale as
   node-events; one `subgraph` traversal spans code + decisions + governing docs. Net estimate
   **-2,000 to -2,400 LOC** (nearly all from `symbols` folding in + the two-view contortions
   dissolving); `contextgraph` grows +400-600; `turbovec` stays. Plus deterministic
   design-intent grounding. Large; gated on the risks below.

## Risks and invariants (do not regress these)

- **The run-provenance gap is the most acute.** The graph has no run column (no `run_id` in
  `contextgraph/` or `eventstore/`). Decisions/findings are already run-scoped at read time via
  `runscope::current_run`; folding CODE-STRUCTURE edges cross-run into a run-scopeless graph
  reintroduces the cross-run id-collision class (a fresh run grounding on a prior run's stale
  structure or a superseded reject). A unified KG MUST carry run provenance on code/doc edges.
  Close this gap FIRST.
- **Safe-superset recall is a correctness invariant, not a token cost.** `partition_by_blast_radius`
  (`:6958`), `partition_wave` (`:2694`), `route_review_tier` (`:438`) co-schedule and route on
  the `safe` view, which is uncapped and grep-UNIONED (`grounder/mod.rs:139`). Any pruning or
  tier mapping must apply to PROMPT RENDERING ONLY, never to these consumers, and the
  `INFERRED`/`AMBIGUOUS` tier must remain a SUPERSET of the grep-unioned safe view. Heuristic
  tags that under-include are a correctness regression, not a saving.
- **Consolidation is lossy by design.** Keep the `rigger peers` recovery path; consolidate only
  material older than the current run.
- **The sleep paper is weight-bound.** Only the paradigm transfers; its benchmark numbers do
  not convert into a rigger token figure.

## Recommendation

Pursue 1 -> 2 -> 3, then 4 only after the run-provenance gap is closed. Every step is reachable
on the event-sourced core, in Rust, without a new dependency. The endgame - caps as assertions,
one blast-radius object, one traversal spanning code + decisions + governing docs, bounded
cross-run memory - is a consolidation of systems rigger already half-built, not a rewrite.
