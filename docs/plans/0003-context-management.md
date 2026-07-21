# Context Management - Implementation Plan (Campaign)

> **For agentic workers:** this plan is executed by the rigger loop itself, not by hand and
> not by ad-hoc subagents. Each spec below is run through the native `/rigger` workflow
> (visible in `/workflows` and the dashboard); the conductor decomposes each spec into units
> and takes every unit through implement -> cargo gates -> three-tier adversarial review ->
> integrate, with bounded remediation. TDD is intrinsic to that lifecycle; do not add a
> separate TDD or execution harness. If a spec cannot be built by the loop, that is a gap in
> the loop to fix, not a reason to hand-build.

**Goal:** keep the context graph correct and bounded as it grows, and unify code, design
intent, and the decision stream into one event-sourced knowledge graph, per
`docs/architecture-addendum-context-management.md`.

**Architecture:** eight loop-ready specs across five workstreams, authored to the spec-shape
rules (one observable behavior per criterion; every mitigation owned by exactly one criterion
with named exclusions; type/anchor shapes in each spec's Design). The largest workstream (the
unified KG, addendum §6) is split into three atomic specs (29a/29b/29c) for the same reason
spec 19 was split during the pit-of-success campaign - a single 6-facet spec drives the planner
into a rule-7 loop it cannot win. Each spec is independently runnable and reviewable; the
ordering respects the cross-spec dependencies below.

**Tech stack:** Rust (the rigger crate), the native `/rigger` Claude Code workflow, the
`.rigger/workflow.yml` self-hosted gate library (cargo fmt / clippy / test on both feature
lanes), the existing event store (`eventstore/`), the context-graph projection
(`contextgraph/`), the grounders (`grounder/` - symbols + turbovec), and the always-on dash
(`dash.rs` / `dash.html`).

## Global constraints (inherited by every spec)

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any
  external tool or project in code, comments, or commit messages.
- Both CI lanes stay green on every unit: `cargo fmt --check`; `cargo clippy --all-targets -D
  warnings`; `cargo test` - on default features AND on `--no-default-features`.
- Determinism by construction: folded/serialized data uses `BTreeMap`/`BTreeSet`/sorted `Vec`.
- The event log stays the source of truth; the graph is a REBUILDABLE projection, never a
  mutable side artifact (addendum §2.1). Supersede-not-delete: invalidation sets `valid_to`;
  deletion stays the `reset --runs` prune authority (`Projector::prune`).
- The addendum's load-bearing invariants are preserved throughout: cross-run grounding is NOT
  scoped to the active run (a still-open prior-run finding stays live); recall stays a SAFE
  SUPERSET of the grep union (§2.4); project- and run-scoping are enforced, not incidental
  (§2.2/§2.3); vectors (`turbovec`) are kept for NL retrieval (§2.5).

## The specs

| Spec | Workstream | Owns (primary files) |
|---|---|---|
| `specs/25-disposition-expiry.md` | A - expiry (§3) | `contextgraph/sqlite.rs` (finding-invalidation fold arm), `conductor.rs` (`graph_context`, `write_capped_findings`), `metrics.rs` (disposition read) |
| `specs/26-dedup-dependency-restore.md` | B - dedup (§4) | `conductor.rs` (`write_capped_section` + the three `write_capped_*` callers) |
| `specs/27-consolidation-distiller.md` | C - consolidation (§5) | a new distiller module (modeled on `playbooks.rs`), the contextgraph projection, the `RunStarted`-boundary attribution |
| `specs/28-graph-project-scoping.md` | scoping (§2.2) | `contextgraph/sqlite.rs` (schema + fold + `subgraph` + `Projector::prune`), `eventstore/namespace.rs` (identity), `conductor.rs` (`graph_context`) |
| `specs/29a-code-ingest-as-events.md` | D - unified KG (§6) | `grounder/symbols/{extract,mod,model}.rs`, `contextgraph/{mod,sqlite}.rs` (new events / node kinds / fold arms) |
| `specs/29b-design-intent-ingest.md` | D - unified KG (§6) | `contextgraph/{mod,sqlite}.rs` (design-intent node kinds + `SPECIFIES`/`CONSTRAINS`/`references`/`explains` edges), a docs extractor |
| `specs/29c-unified-traversal-tiers.md` | D - unified KG (§6) | `conductor.rs` (`graph_context` / `build_prompt_with_failure`), `grounder/mod.rs` (`BlastRadius` retire), `grounder/symbols/*` (fold in), `grounder/turbovec.rs` (retain) |
| `specs/30-knowledge-graph-dash.md` | E - viz (§7) | `dash.rs` (`route` + `/api/graph`), `dash.html` (KG panel) |

## Ordering and dependencies

Run in this order (ROI-first, then dependency-forced):

1. **Spec 25 - disposition-expiry (§3).** First: smallest change, highest MEASURED ROI (reuses
   the `valid_to` column that 0% of edges use today), and it directly relieves the injected-
   findings truncation this project measured. Independent of the rest.
2. **Spec 26 - dedup + dependency-restore (§4).** Cheap render-only complement to 25.
   Independent; sequenced here because it is small and de-risks the grounding slice before the
   graph changes.
3. **Spec 27 - consolidation distiller (§5).** Bounds cross-run growth automatically. Depends
   on 25's run-scoped invalidation (it consolidates the old, still-live remainder).
4. **Spec 28 - project-scoping on the graph (§2.2).** The prerequisite that makes a SHARED
   graph backend safe. Today project isolation is only physical (separate `graph.db` files);
   this adds the in-graph project tag. It MUST land before 29a-c move the graph onto a shared
   backend. Independent of 25-27.
5. **Spec 29a - code ingest (§6).** Re-express the tree-sitter extractor as an event-emitting
   pass; lands `code-entity`/`file` nodes and tiered edges. Depends on 28 (nodes carry project
   scope).
6. **Spec 29b - design-intent ingest (§6).** Lands `design-doc`/`arch-decision`/`handbook-rule`/
   `rationale` nodes and their edges. Depends on 28; independent of 29a but sequenced after it.
7. **Spec 29c - unified traversal + tiers (§6).** One seeded traversal replaces the two-store
   stitch; confidence-tier filters replace the `BlastRadius` struct; retires the seed-vs-precise
   workaround. Depends on 29a AND 29b (the nodes must exist to traverse).
8. **Spec 30 - KG in the dash (§7) - LAST.** The read-only visualization + grounding-provenance
   panel. Depends on 29c (unified graph + tiers), spec 21 (LIVE/HISTORICAL run labels), and
   spec 19b (the always-on dash).

Rationale for splitting §6 into 29a/29b/29c: the unified KG is as large as the monolithic spec
19, which wedged repeatedly when its many criteria drove the planner to over-refine a criterion
into two units with byte-identical criteria (a rule-7 loop). Three atomic specs - code nodes,
design-intent nodes, then unified traversal - each keep the planner whole.

## How to run the campaign

This campaign runs AFTER the pit-of-success campaign (specs 18-24) reaches its fixpoint and
AFTER the user approves the addendum. For each spec, in the order above, drive it through the
native workflow:

```
/rigger specs/25-disposition-expiry.md
```

Watch progress in `/workflows` and the dashboard (`rigger dash`, then the printed `127.0.0.1`
URL). The run lands integrated commits on the `rigger-run` branch (reused, not re-anchored off
`origin/main`). Advance to the next spec only after the prior spec reaches a clean fixpoint (all
its units integrated, zero escalations). A wedge is fixed at the loop level (spec, persona, or
config) - never by hand-implementing or hand-banking a unit. Never commit to `rigger-run` while
a loop runs (the conductor integrates there in the main working dir; a concurrent human commit
corrupts the tree).

After all eight specs are green, a human turns `rigger-run` into `main` through normal PR review
(the loop lands on the run branch; humans land on main).

## Acceptance (the addendum's measured targets, §9)

The campaign is complete when the §9 targets hold, verified against a real `graph.db` and a
two-project fixture:

- Injected `graph_context` slice drops BELOW the summed 84 KiB budget
  (`DECISIONS_BUDGET_BYTES` 24K + `LESSONS_BUDGET_BYTES` 12K + `FINDINGS_BUDGET_BYTES` 48K) on a
  hot unit, from the measured 149 KiB pool - so no truncation of relevant context (specs 25-27).
- Edge invalidation is NON-ZERO and tracks dispositions, from the measured 0% (spec 25).
- Cross-run node count stays bounded across N runs without a manual `reset --runs` (spec 27).
- A `reset --runs` / consolidation / traversal against a backend holding TWO projects touches
  ONLY the current project's nodes - proven by a two-project fixture (spec 28).
- An agent retrieves the handbook rule GOVERNING a touched file, and the RA section that
  SPECIFIES it, by traversal (specs 29a/29b/29c).
- The dash renders a seeded KG neighborhood with query-path, god-node, `explain`, and
  grounding-provenance views, self-contained and scoped to one project (spec 30).

## Self-review (spec coverage vs the addendum)

- Addendum §2 (load-bearing invariants) -> preserved as global constraints in every spec.
- Addendum §3 (Workstream A - disposition-expiry) -> spec 25.
- Addendum §4 (Workstream B - dedup + dependency-restore) -> spec 26.
- Addendum §5 (Workstream C - consolidation distiller) -> spec 27.
- Addendum §2.2 (project-scoping enforced on the graph) -> spec 28.
- Addendum §6 (Workstream D - unified event-sourced KG) -> specs 29a (code ingest), 29b (design-
  intent ingest), 29c (unified traversal + confidence tiers).
- Addendum §7 (Workstream E - KG in the dash) -> spec 30.
- Addendum §9 (measured acceptance targets) -> the acceptance section above.
