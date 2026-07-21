# 26 - Safe dedup + dependency-restore at prompt assembly

**Goal:** at prompt assembly, normalize-and-dedup the injected context slice so near-identical
items do not each consume the byte budget, and RESTORE any dependency a kept item references so
nothing fact-complete is dropped. This is Workstream B (section 4) of the context-management addendum: a
cheap complement to disposition-expiry (spec 25) and a safety net for consolidation (spec 27).
Measured value: ~21% duplication on lessons cross-run; lower on findings/decisions post-reset.

## Design

Prompt assembly caps three sections through one shared writer, `write_capped_section`
(`src/conductor.rs`), called by `write_capped_decisions`, `write_capped_lessons`, and
`write_capped_findings`. Lessons already rank `by_relevance` (blast-radius overlap); decisions and
findings do not. This spec adds two passes to the shared writer, BEFORE the recent-N / byte-budget
truncation:

1. **Normalize-and-dedup.** Collapse entries whose text is identical after normalization
   (whitespace-folded, mirroring the existing `normalize_ws` helper the grounding path uses).
   Keep the first occurrence's provenance; drop the byte-identical remainder. Deterministic:
   iterate a `BTreeMap`/sorted key so the kept entry is stable.
2. **Dependency-restore.** If a KEPT entry references another item it depends on (by a graph edge
   / id in the same `subgraph` result - e.g. a decision that `SUPERSEDES` another, or a finding
   whose context is a sibling node), restore that referenced item into the slice even if dedup or
   the cap would have dropped it, so the kept entry has no dangling reference. Restore is
   STRUCTURAL (follows graph edges from the `subgraph` result), not a text heuristic.

Both passes operate on the assembled slice only. They change the rendered prompt string; they
never emit an event and never mutate the graph or store (section 2.4 - recall is a safe superset, applied
at render).

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any external
  tool or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- Determinism by construction: dedup/restore iterate `BTreeMap`/`BTreeSet`/sorted `Vec`; identical
  input yields an identical rendered slice.
- Render-only: this spec adds NO event type and performs NO store/graph mutation. The projection is
  the source of truth and is untouched.
- Load-bearing decision preserved: recall stays a SAFE SUPERSET - dependency-restore may only ADD
  items to keep the slice fact-complete; it must never drop an item the un-deduped slice contained
  without a byte-budget reason.

## Done when

- [ ] a test proves the injected slice is DE-DUPLICATED by normalized text: two entries in the same
  section (two lessons, or two findings) whose text differs only by whitespace/normalization
  collapse to a single rendered entry, and the kept entry is deterministic across runs. This
  criterion OWNS the normalize-and-dedup pass in `write_capped_section`.
- [ ] a test proves DEPENDENCY-RESTORE: when a kept entry references another item (linked by an
  edge in the same `subgraph` result) that dedup or the byte cap would have dropped, the referenced
  item is restored into the rendered slice so the kept entry is fact-complete (no dangling
  reference). This criterion OWNS the structural dependency-restore pass; it does NOT own the dedup
  pass (criterion 1).
- [ ] a test proves this is RENDER-ONLY: exercising dedup and restore changes the assembled prompt
  string but emits NO event and leaves the graph/store projection byte-identical. This criterion
  OWNS the no-mutation guarantee; it does NOT own dedup or restore behavior (criteria 1-2).
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
