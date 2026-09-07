# 86 - Tests are evidence, not knowledge: they leave the graph and land on the card

**Goal:** the knowledge graph ingests `tests/` and `#[cfg(test)]` code as code entities on equal
footing with the product. On this repository that is 153 test files and 2,804 `#[test]`s inside
the 9,211 code-entity nodes, and it shows: the code lens's largest districts are `tests: cli`,
`tests: graph`, `tests: dash`, and the dash's community overview is dominated by `tests/*.rs`
clusters. Operator decision (2026-09-06): tests do not constitute knowledge about the project;
they are PROOF of functionality. The graph should hold the product's entities and relations, and
each entity should carry, as metadata, the fact of how much proof exists for it. This also
re-applies a standing rule: the graph models the target project, not the machinery around it.

## Design

INGESTION, decided: the code-entity pass excludes test code from NODE creation - every file under
`tests/`, every `#[cfg(test)]` module, and every `#[test]` function. Excluded code is still
PARSED, because its references are the evidence: a call or type reference from a test into a
product entity records a `proven_by` count on that entity (and the referencing test's
`file:line`s as the evidence list), never a node and never an edge on the canvas. The
per-entity count is available on the card and in the `/api/graph` payload as metadata; the
files lens no longer lists test files as subjects; the concepts pass no longer draws evidence
from test bodies (a concept's evidence is product code, docs and decisions).

MIGRATION, decided: existing test-entity nodes are retired by the fold's supersession
mechanism on the next ingest, not by a store wipe; `rigger validate` reports the count of
retired test entities once so the operator sees the graph shrink deliberately. The derived
index dedup handles the re-record.

WHERE PROOF RENDERS, decided: the card gains a PROOF row - "proven by N tests" with the list on
expand - and an explicit `no test reaches this entity` state (amber, not silent), since absence
of proof is itself information the audit (spec 85) and reviewers want. The run-console
(Mission Control) card carries the same row; the mock names the design.

CONSTRAINTS WALK: a product function defined inside a `#[cfg(test)]` module - excluded, by the
rule; a test helper in `tests/common/` referenced only by tests - excluded (no product caller);
doc-tests in `///` examples - not entities today, unchanged; a `mod tests` inside `src/*.rs` -
excluded by the `cfg(test)` rule, and its references to siblings count as proof; the audit's
scanner (spec 85) reads tests directly from the tree and is unaffected by graph contents.

## Global constraints

- Hyphens, never em dashes. Both feature lanes green (fmt, clippy -D warnings, test, default
  and --no-default-features); no-os-kill and reap audits green.
- No new event type: `proven_by` rides the existing code-entity record as an additive,
  serde-defaulted field. No new dependency.
- The operator's installed rigger binary is never replaced or modified by any unit.

## Done when

- [ ] a test proves TESTS ARE NOT NODES: after ingesting a fixture with product code, a
  `tests/` file, a `#[cfg(test)]` module and `#[test]` functions, the graph contains a
  code-entity node for every product item and none for any test item, and the files lens lists
  no test file as a subject. This criterion OWNS the exclusion rule in the ingest pass; the
  proof metadata is criterion 2's, NOT this one's.
- [ ] a test proves PROOF LANDS ON THE CARD: a product entity referenced by two test functions
  carries `proven_by: 2` with both `file:line`s in the graph payload and renders "proven by 2
  tests" on its card, while an unreferenced entity renders the explicit no-test state. This
  criterion OWNS the `proven_by` field, its payload and card rendering; exclusion is criterion
  1's, NOT this one's.
- [ ] a test proves THE MIGRATION IS DELIBERATE: re-ingesting a store that already holds test
  entity nodes retires them through supersession (no store wipe), the fold yields the same
  product entities before and after, and `rigger validate` reports the retired count once.
  This criterion OWNS supersession and the validate advisory.
- [ ] both feature lanes green (fmt, clippy, test on default and --no-default-features).
