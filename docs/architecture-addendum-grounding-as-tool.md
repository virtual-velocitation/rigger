# Reference Architecture Addendum — Grounding as a Queryable Tool

**Status:** design, approved for planning. Quantitative claims are grounded in a measurement of the
live `.rigger/graph.db` taken 2026-07-21 and a reading of the grounding path
(`build_prompt_with_failure` / `graph_context` / `write_design_intent`, `src/conductor.rs`) and the
MCP tool surface (`src/mcpserver.rs`); figures marked *(est.)* are not measured.
**Scope:** an addendum to `docs/architecture.md`. It changes **how an agent consumes the knowledge
graph during a run** — from a capped blob PUSHED into every prompt to a HYBRID: push only the small
deterministic layer the agent must be *guaranteed* to see, and let the agent PULL everything else on
demand through a real graph query tool. It removes a measured 85%-truncation defect, makes retrieval
precise instead of recency-ranked, and is the point at which concept/community structure earns its
keep for agents. It changes no invariant of the event-sourced core and preserves the deterministic
design-intent grounding thesis exactly where that thesis is load-bearing.

---

## 1. Problem (measured)

Every agent prompt is assembled by `build_prompt_with_failure`, which does
`b.push_str(&self.graph_context(&seed))`: a depth-2 traversal from the unit's blast-radius files,
rendered as a code neighborhood + the design-intent layer + capped decisions/lessons/findings, all
PUSHED into the prompt string under a ~84 KiB cap. Four facts about that push:

- **It over-reaches, then discards most of what it reaches.** On the hottest file
  (`src/conductor.rs`) the depth-2 context pool is **529 nodes / 553.5 KiB** against the ~84 KiB cap
  — **~85% is truncated every spawn**, by a recency ranking, not by relevance to the agent's actual
  sub-problem. The pattern holds across hot files (`main.rs` 323 KiB, `contextgraph/sqlite.rs`
  332 KiB — all far over the cap).
- **The truncated mass is the reference bulk, not the governing intent.** Of that 553 KiB,
  **decisions are 442 KiB (80%) and findings 108 KiB**; the entire design-intent layer the push
  guarantees — handbook rules, design-docs, arch-decisions, rationale bound to the touched files —
  is **~2.8 KiB and sits mostly at depth-1** (410 of 529 context nodes are one hop from the seed).
  The layer that must be deterministic is tiny and always fits; the layer being truncated is
  reference material.
- **The pull tool already exists and is redundant with the push.** `rigger_peers` (an MCP tool
  agents already call live) returns "the decisions, lessons, AND review findings … scoped to
  \[blast-radius] files", **uncapped**. So the agent already receives the same content **twice** —
  once as a truncated push, once as an uncapped pull — and only the push is lossy.
- **There is no real query surface.** The MCP tools are `rigger_next`, `rigger_result`,
  `rigger_emit`, `rigger_peers`, `rigger_activity`. `rigger_peers` is file-scoped; there is no
  "explain this node", no NL/symbol query, no path, no seeded traversal, no concept query. The agent
  cannot ask a precise question — it gets a fixed, pre-rendered, truncated blob regardless of what it
  actually needs.

The consequence: reach is oversupplied, selection is lossy and blind to the sub-problem, and the
graph the run spent effort building is stuffed in rather than queried.

### Non-goals / anti-fixes
- Do NOT drop the deterministic design-intent grounding — the governing rule for a touched file must
  still be *guaranteed* in the prompt (§2.1); you cannot rely on an agent asking about a rule it does
  not know exists.
- Do NOT raise the cap — a bigger blob is a bigger truncation, still recency-ranked, still blind to
  the sub-problem. The fix is to stop pushing the bulk, not to push more of it.
- Do NOT make the graph a *pure* tool — a pure pull model reintroduces the rule-7 failure class
  (the agent never queries the governing rule it does not know about). The design is a HYBRID.
- Do NOT build a second query engine — the agent tools and the human dash read the SAME projection
  and traversal (§2.3).

---

## 2. Load-bearing decisions — invariants the design must carry

### 2.1 Push the deterministic minimum; tool the rest
The prompt PUSHES only the layer the agent must be guaranteed to see without knowing to ask for it:
the **design-intent bound to the touched files** — the handbook rule that `GOVERNS` a file, the RA
section that `SPECIFIES` it, the decision that `CONSTRAINS` it, the rationale that `explains` it
(exactly what `write_design_intent` already renders, ~2.8 KiB). Everything large and consulted-on-
demand — decisions, review findings, the wider code neighborhood, concepts — becomes a **PULL** the
agent issues against a query tool when its work reaches the point of needing it.

### 2.2 Determinism is preserved precisely where it was load-bearing
The deterministic design-intent grounding thesis (deliver the governing rule by structure, not by
hope) applies to the **intent layer**, and that layer stays pushed and guaranteed. The reference
bulk was **never** deterministic — it was truncated 85% by recency — so moving it to pull forfeits
no guarantee; it removes an arbitrary one and replaces it with precise, agent-directed retrieval.

### 2.3 One query engine, two consumers
The agent tool surface and the human dash read the **same projection and the same traversal**
(`subgraph` / neighborhood / `explain` / path / the `ground` NL-seeding pass). There is one query
implementation; the MCP tools and the dash `/api/graph` route are two thin callers of it. A query
that works for the operator works identically for the agent.

### 2.4 Tool results are bounded per-query, tier-tagged, provenance-carrying, read-only
A pulled result is bounded by the **query** (a node's neighbors, a path, a file's peers, a concept's
members), not by a global prompt cap — so nothing is silently truncated; a large result is narrowed
by asking a narrower question. Every returned node/edge carries its confidence tier
(`EXTRACTED`/`INFERRED`/`AMBIGUOUS`) and the event position that folded it, so the agent can weigh a
model-inferred fact against a deterministic one. The tools are **read-only** over the projection; the
conductor stays the sole mutation authority (only `rigger_emit` writes, and it appends events, it
does not mutate the graph directly).

### 2.5 Event-sourced + project-scoped — inherit the core invariants
The projection the tools read is rebuildable from the log, supersede-not-delete, and namespaced by
`proj-<identity>-` (context-management addendum §2.1/§2.2). A pull never crosses project boundaries;
a superseded fact is not returned to a live query unless the caller asks for history.

### 2.6 Concepts are query affordances, not pushed content
If the concept layer exists, it is exposed as **queries** (`what realizes <concept>`,
`peers-by-concept`), never as pushed prompt content — which is the only role in which concepts
improve LLM effectiveness (the push-a-concept-blob path was measured a no-go). The **structural and
community** queries need no model and ship first; the model-driven concept layer is justified only if
its queries prove useful in practice, and it rides on this tool surface either way.

### 2.7 Pull must not be left to chance — the agent is told, and review stays guaranteed
Two safeguards keep pull from degrading into "the agent forgot to look":
- **The protocol names the tools and their triggers** (§Workstream C): the prompt tells the agent
  the graph is queryable and *when* to query (an unknown symbol, prior decisions about a file,
  existing findings before re-raising). The pushed intent layer orients; the tools retrieve.
- **Review determinism is not weakened.** A reviewer's correctness depends on seeing the lenses'
  findings; today it gets them through the push. Those findings must remain **guaranteed** for the
  review stages — via a required retrieval or a findings-only push at review time — so moving the
  bulk to pull never lets an adjudicator adjudicate blind to a finding.

---

## 3. Workstream A — Trim the push to the deterministic minimum

**Target state:** `graph_context` pushes only what §2.1 guarantees — the design-intent layer
(`write_design_intent`) plus a compact code-neighborhood orientation for the touched files — and
appends a short pointer that the decisions/findings/wider graph are queryable via the tools. The
capped `write_capped_decisions` / `write_capped_lessons` / `write_capped_findings` blob is **removed
from the implement-stage prompt**.

```
   BEFORE (push, capped)                        AFTER (hybrid)
   ┌───────────────────────────────┐            ┌───────────────────────────────┐
   │ prior-failure block           │            │ prior-failure block           │
   │ code neighborhood (depth-2)   │            │ code neighborhood (compact)   │
   │ DESIGN INTENT     ~2.8 KiB  ◄──┼ guaranteed │ DESIGN INTENT     ~2.8 KiB  ◄──┼ guaranteed
   │ decisions   ┐                 │            │ » query the graph for prior   │
   │ lessons     ├─ ~80 KiB, 85%   │            │   decisions/findings/peers    │
   │ findings    ┘   truncated  ◄──┼ LOSSY      │   via rigger_graph_* / peers  │
   │ emit + plan protocol          │            │ emit + plan protocol          │
   └───────────────────────────────┘            └───────────────────────────────┘
     ~84 KiB pushed, most discarded               ~few KiB pushed, nothing discarded
```

- **Reclaims ~80 KiB of prompt per spawn and eliminates the 85% arbitrary truncation.** The
  reference bulk is no longer rendered-then-thrown-away; it is retrieved on demand, in full, scoped
  to the sub-problem.
- **The guaranteed layer is unchanged.** `write_design_intent` already renders exactly the bound
  governing intent, deterministically ordered; it stays. This is the measured, immediate-ROI step.

_Code:_ `graph_context` (`src/conductor.rs`) — keep `write_design_intent` + a compact neighborhood
render, drop the capped decisions/lessons/findings sections from the implement prompt, append the
tool pointer. The `write_capped_*` writers remain for the review-stage guarantee (§Workstream C).

## 4. Workstream B — A real graph query tool surface

**Target state:** the agent can ask the graph precise questions. The MCP surface (with CLI parity)
grows from file-scoped peers to a real query set, every tool a thin caller of the §2.3 engine:

```
  rigger_graph_query { query }        NL or symbol → the ground() pass seeds a bounded neighborhood
  rigger_graph_explain { node }       a node's provenance: incident edges, tiers, the event that folded it
  rigger_graph_path { from, to }      the path between two nodes (why is X connected to Y)
  rigger_graph_peers { files, kind? } the existing peers pull, extended with a kind/scope filter
  rigger_graph_concept { concept }    (contingent §2.6) the code/docs/decisions realizing a concept
```

- **Bounded, tier-tagged, provenance-carrying, read-only (§2.4).** Each returns a small, typed
  result the agent composes, not a blob it must skim.
- **Reuses what exists.** `rigger ground` already resolves NL/symbol → seed files (the vector pass);
  `subgraph`/`explain`/path already exist for the dash. This workstream exposes them over MCP, it
  does not reinvent retrieval.

_Code:_ `src/mcpserver.rs` (new `tools/list` entries + `call_tool` arms), `src/main.rs` (CLI parity
subcommands), all delegating to the existing `contextgraph` traversal + `ground` pass.

## 5. Workstream C — Teach the agent, and keep review deterministic

**Target state:** pull is reliable because the agent is told how and when, and the review stages keep
their finding guarantee.

- **Protocol.** The emit/plan protocol gains a short "the graph is queryable" section: the tools, and
  the triggers to use them (unknown symbol → `graph_query`; touching a file → `graph_peers` for prior
  decisions/findings; before re-raising → check existing findings). Truncation-recovery footnotes
  become tool pointers.
- **Review guarantee (§2.7).** The adversary/adjudicator, whose verdict depends on the lenses'
  findings, either receive a **required** `graph_peers`/findings retrieval as the first step of the
  review protocol or keep a **findings-only** push at review time — so review is never blind. This is
  the one place the push is retained deliberately, scoped to findings, not the whole bulk.

_Code:_ `EMIT_PROTOCOL` / `plan_protocol` / the review protocols (`src/conductor.rs`), the
review-stage branch of `graph_context`.

## 6. Workstream D — Concepts as query affordances (contingent)

**Target state:** *if* the concept layer is built (the concept-graph capability), it surfaces only as
tools — `rigger_graph_concept`, concept-scoped peers — never as pushed content (§2.6). The structural
and **Leiden community** queries need no model and can ship on this surface first; the model-driven
concept extraction is gated on demonstrated query value. This workstream is optional to the grounding
win and is the bridge to the human-observability concept explorer, which reads the same tools.

_Code:_ concept node/edge layer (separate campaign) + a `rigger_graph_concept` arm here.

---

## 7. Delivery

Decomposed into atomic, loop-ready specs, in ROI / dependency order. Each ends with both feature
lanes green.

1. **A · Trim the push** (§3) — smallest, highest measured ROI (reclaims ~80 KiB/spawn, kills the
   85% truncation), gated only on keeping the intent layer pushed. Ships first.
2. **B · Query tool surface** (§4) — the pull substrate; exposes the existing traversal + `ground`
   pass over MCP + CLI.
3. **C · Protocol + review guarantee** (§5) — makes pull reliable and preserves review determinism;
   depends on B.
4. **D · Concept query affordances** (§6) — contingent on the concept layer; structural/community
   queries first, model concepts only if their queries earn it.

This reprioritizes the earlier concept-graph campaign: the **grounding-as-tool** change (A–C) is the
part with a measured LLM-effectiveness justification and ships on its own; the concept layer becomes
a query affordance (D) and a human-observability bonus, not a grounding prerequisite. The
context-management work (disposition-expiry, consolidation) remains complementary — it keeps what a
pull *returns* lean, now that the pull is the path.

## 8. Acceptance (measured targets)

- **Pushed grounding shrinks to the deterministic minimum.** On the same hot files, the pushed
  `graph_context` drops from ~84 KiB (capped, 85% truncated) to the intent layer + compact
  neighborhood + tool pointer (single-digit KiB), with **zero** silent truncation — measured against
  the real `graph.db`.
- **No governing-rule regression.** For a unit touching file F, the handbook rule / design-doc /
  decision that governs F is still present in the prompt deterministically (§2.1) — proven by a test,
  not by retrieval luck.
- **The agent retrieves on demand.** An agent obtains prior decisions, findings, an `explain`, and a
  path via the query tools, each bounded and tier-tagged (§2.4) — proven end-to-end.
- **Review stays blind to nothing.** An adversary/adjudicator always obtains the lenses' findings
  (required retrieval or findings-only push), proven by a review-stage fixture (§2.7).
- **Read-only + project-scoped.** The query tools never mutate the store and never return another
  project's nodes — proven by a two-project fixture (§2.5).
- **End-to-end: equal-or-better at lower cost.** Across a set of real units, first-pass yield holds
  or improves while pushed-prompt tokens drop — the effectiveness-and-efficiency claim, measured, not
  assumed.
