# 36 - Grounding: trim the pushed prompt to the deterministic intent layer

**Goal:** stop pushing the large, capped decisions/lessons/findings blob into the IMPLEMENT prompt.
Push only the layer an agent must be guaranteed to see without knowing to ask for it - the design
intent bound to the touched files (`write_design_intent`) plus a compact code-neighborhood
orientation - and append a short pointer that the rest is retrievable on demand. This reclaims the
~80 KiB the implement prompt spends rendering a pool that is then ~85% truncated by recency (measured
on a hot file: the decisions+lessons+findings sections total ~550 KiB against the ~84 KiB cap), and
makes retrieval precise - the agent pulls what its sub-problem needs - instead of a fixed, truncated
blob. The reference bulk is already pullable uncapped through the existing `rigger_peers` tool, and
since spec 37 the code graph ANSWERS navigation - `rigger graph --around <file|entity>` returns
who-calls-X and the caller/callee neighborhood - so pointing the implementer at both replaces the
truncated push with precise, on-demand pulls. This is Workstream A of the grounding-as-tool addendum
(section 3): push the deterministic minimum, tool the rest. The trim is IMPLEMENT-ONLY: the adversary and adjudicator retrieve the lenses' findings
THROUGH `graph_context`, so review-stage grounding is deliberately left intact here (the
review-determinism guarantee is a later workstream).

## Design

`build_prompt_with_failure` (`src/conductor.rs`) assembles every spawn's prompt and pushes
`graph_context(seed)` verbatim. `graph_context` renders, from one `subgraph(seed, 2)`:
`write_code_neighborhood` + `write_design_intent` + `write_capped_decisions` +
`write_capped_lessons` + `write_capped_findings`. The last three are the truncated bulk; the design
intent (the handbook rule that `GOVERNS` a seed file, the design-doc that `SPECIFIES` it, the
decision that `CONSTRAINS` it, the rationale that `explains` it) is small (~2.8 KiB, mostly depth-1)
and must stay.

Split grounding by stage at the assembly seam:

- an **IMPLEMENT** spawn (the implementer role) gets a TRIMMED slice: `write_code_neighborhood` +
  `write_design_intent` + a one-line pointer to the pull tools - `rigger_peers` for prior
  decisions/lessons/findings scoped to the blast-radius files, uncapped (MCP + CLI, `src/mcpserver.rs`
  / `cmd_peers` in `src/main.rs`), and `rigger graph --around <file|entity>` for code navigation,
  which since spec 37 answers who-calls-X and the caller/callee neighborhood (`cmd_graph` in
  `src/main.rs`). It OMITS `write_capped_decisions` / `write_capped_lessons` / `write_capped_findings`.
- a **REVIEW** spawn (lens / adversary / adjudicator) keeps the FULL `graph_context` UNCHANGED. The
  adversary grounds after the lenses and the adjudicator after both, and they retrieve the lenses'
  findings through `graph_context`'s findings section (`src/conductor.rs`: the adversary/adjudicator
  grounding comments, and the existing regression asserting a lens finding arrives under the
  `graph_context` findings header). Trimming findings here would blind review - explicitly out of
  scope.

The `write_capped_*` writers STAY in the tree (the review path uses them); only the implement
assembly stops calling them. The stage discrimination reuses the role/stage signal
`build_prompt_with_failure` already receives (`Stage`), so an implement spawn and a review spawn
render different grounding slices from the same seam. This spec changes prompt ASSEMBLY only - no
event type, no projection change, no store write.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any external
  tool or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- Determinism by construction: any folded/serialized set uses `BTreeMap`/`BTreeSet`/sorted `Vec`;
  the trimmed slice renders deterministically for a given seed.
- The event log stays the source of truth; the graph is a rebuildable projection. This spec touches
  prompt ASSEMBLY only - it never mutates the store or the projection.
- The design-intent deterministic-delivery guarantee is PRESERVED: the trim removes ONLY the capped
  dev-loop bulk (decisions/lessons/findings) from the implement prompt; the intent layer
  (`write_design_intent`) and the code neighborhood stay, delivered by traversal, not by retrieval
  luck. The governing-rule guarantee is not weakened.
- The review path is NOT weakened: findings still reach the adversary and adjudicator via
  `graph_context`. The trim is implement-only; blinding review is a correctness regression.

## Done when

- [ ] a test proves the IMPLEMENT prompt is TRIMMED: for an implement-stage spawn whose seed has
  decisions/lessons/findings in its depth-2 neighborhood, the assembled prompt contains the
  design-intent and code-neighborhood sections and a pointer naming the pull tools (`rigger_peers`
  for decisions/findings and `rigger graph --around` for code navigation), and does NOT contain the
  capped decisions / lessons / findings sections. This criterion OWNS the implement-stage trim and
  the tool pointer.
- [ ] a test proves the DESIGN-INTENT guarantee survives the trim: for an implement-stage spawn
  touching a file bound by a design-intent node (a handbook-rule `GOVERNS` / design-doc `SPECIFIES` /
  decision `CONSTRAINS` / rationale `explains` edge to a seed file), the assembled prompt STILL
  contains that intent binding deterministically. This criterion OWNS the intent-delivery
  preservation; it does NOT own the trim (criterion 1).
- [ ] a test proves the trim is IMPLEMENT-ONLY: a REVIEW-stage spawn (adversary or adjudicator)
  grounded on a seed carrying a lens finding STILL contains that finding under `graph_context`'s
  findings section, so review is not blinded. This criterion OWNS the review-path preservation (the
  guardrail that this trim does not weaken review); it does NOT own the review-determinism GUARANTEE
  (a later grounding-as-tool workstream).
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
