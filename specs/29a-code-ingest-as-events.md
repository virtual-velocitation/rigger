# 29a - Unified KG: code structure ingested AS EVENTS

**Goal:** make the codebase structure part of the event-sourced context graph. Re-express the
tree-sitter extractor as an event-EMITTING pass whose events the SAME idempotent fold folds into
the graph as `code-entity` and `file` nodes with confidence-tiered structural edges - so code
structure becomes a rebuildable, bi-temporal projection over the log, not a mutable side index.
First of the three unified-KG specs (section 6): it lands the code half of the one graph.

## Design

The tree-sitter touch point today is `extract::extract(source, lang, ts_language, tags_query) ->
FileSymbols` (`src/grounder/symbols/extract.rs`), driven by `build_index` -> `index_one_file`
(`src/grounder/symbols/mod.rs`) into the parser-free model (`Lang`/`Kind`/`Def`/`SymRef`/
`FileSymbols`/`SymbolIndex`, `src/grounder/symbols/model.rs`). This spec keeps that extraction but
routes its output through the event log:

- **Emit.** The per-file extraction emits `CodeEntityExtracted` (one per definition) and
  `EdgeInferred` (one per reference) events. Extraction stays in the `symbols` feature; the emit +
  fold is always compiled.
- **Fold.** New fold arms in `Projection::apply` -> `fold` (`src/contextgraph/sqlite.rs`) turn
  those events into `code-entity` nodes (kind added alongside the existing `KIND_*` consts in
  `src/contextgraph/mod.rs`), a `file` container node, and structural edges.
- **Tier.** Each structural edge carries a confidence tier: explicit-in-source
  (calls / imports / inherits) folds as EXTRACTED; derived (transitive / re-export) as INFERRED;
  grep-visible-only (macro body / reflection string / dynamic) as AMBIGUOUS. The tier is a first-
  class edge attribute, the same `precise`/`safe` split made durable.
- **Supersede on re-extract.** Re-extracting a changed file SUPERSEDES rather than overwrites:
  the old entity's edges get `valid_to` set (reusing the decision-supersession `UPDATE ... SET
  valid_to` mechanism in `sqlite.rs`) and new edges are inserted live. This is strictly stronger
  than an overwrite-in-place index and is why the graph stays bi-temporal (section 6.4).

Nodes and edges carry the project scope from spec 28 (this spec depends on 28). This spec does NOT
change `graph_context` or retire the `BlastRadius` struct - that is spec 29c. It only makes the
code graph exist in the projection.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any external
  tool or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`. The always-compiled fold arms
  must build with the `symbols` feature OFF (extraction gated, fold not).
- Determinism by construction: extraction emits events in a sorted order; folded node/edge sets
  use `BTreeMap`/`BTreeSet`; identical source yields byte-identical nodes/edges.
- The event log stays the source of truth; the code graph is a rebuildable projection. Supersede-
  not-delete: re-extraction sets `valid_to`, never deletes.

## Done when

- [ ] a test proves a tree-sitter extraction pass over a source file EMITS `CodeEntityExtracted` /
  `EdgeInferred` events that the fold turns into `code-entity` nodes, a `file` container node, and
  structural edges - so code structure lives in the event-sourced projection, not a mutable side
  index. This criterion OWNS the extract-as-events pass and the code/file node fold arms.
- [ ] a test proves every structural edge carries a CONFIDENCE TIER: explicit source references
  fold as EXTRACTED, derived/transitive as INFERRED, and grep-visible-only as AMBIGUOUS. This
  criterion OWNS the tier attribute on folded edges; it does NOT own the extract pass (criterion 1).
- [ ] a test proves re-extraction after a file changes SUPERSEDES rather than overwrites: the
  changed entity's old edges get `valid_to` set and new edges are inserted live, so `subgraph` at
  the new position sees the new entity while a historical query still sees the old. This criterion
  OWNS supersede-on-re-extract; it does NOT own tiering or the initial fold (criteria 1-2).
- [ ] a test proves the code graph is REBUILDABLE from the log: rebuilding re-derives the same
  code-entity/file nodes and tiered edges from the emitted events, with no mutable side artifact.
  This criterion OWNS rebuild; it does NOT own the prior criteria.
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
