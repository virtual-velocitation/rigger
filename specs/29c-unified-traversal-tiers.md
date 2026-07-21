# 29c - Unified KG: one seeded traversal, confidence-tier blast radius

**Goal:** collapse grounding onto the ONE unified graph. Replace the separate structural-grounder-
call-then-stitch with a single seeded traversal that reaches code, decisions, and design intent
together, and replace the hand-rolled two-view `BlastRadius` struct with two confidence-tier
filters over one edge set. This lands the payoff of the unified KG (section 6): deterministic design-
intent grounding, and the retirement of the two-store stitch and the seed-vs-precise workaround.
Third and last of the unified-KG specs; depends on 29a (code nodes) and 29b (design-intent nodes).
It also WIRES those extraction passes to run in a live run (29a/29b built the machinery with no
production caller), so grounding traverses a graph the run itself populated - not one filled only by
test fixtures.

## Design

Today structural grounding is stitched in `build_prompt_with_failure` (`src/conductor.rs`): it
calls `gr.ground(&query, 8)`, builds a `seed` Vec from the returned refs' `.file` fields, then
calls `graph_context(seed)` which runs a single `graph.subgraph(seed, 2)`. Blast radius is a
separate `BlastRadius { precise: Vec<String>, safe: Vec<String>, serialize: bool }` struct
(`src/grounder/mod.rs`) with per-grounder overrides (`grounder/symbols/grounder.rs`,
`grounder/symbols/hybrid.rs`) and a `record_blast_radius` audit emit (`conductor.rs`).

With code and design intent now IN the graph (29a/29b), unify:

- **One traversal.** Structural grounding is a single seeded `subgraph` traversal over the unified
  graph - it returns the touched file's code neighborhood, the decisions/findings about it, the
  handbook rule that governs it, and the RA section that specifies it, in one pass. The separate
  structural grounder call and the stitch in `build_prompt_with_failure` go away (turbovec stays,
  for NL seeding - see below).
- **Tier filters replace the struct.** The `precise` seed for a prompt is the EXTRACTED subgraph;
  the `safe` superset the safety consumers need (section 2.4) is EXTRACTED u INFERRED u AMBIGUOUS. Two
  filters over one edge set replace `BlastRadius{precise,safe}`; the safe superset MUST remain a
  superset of the grep union (the correctness invariant). The `serialize` hub fail-safe becomes a
  property of the traversal (degree / community), dissolving the seed-vs-precise divergence and the
  `record_blast_radius` workaround.
- **Vectors stay complementary.** A symbol-free query ("where is retry backoff handled") has no
  node id to seed on; `turbovec` (`src/grounder/turbovec.rs`) answers it and its result SEEDS the
  traversal. Graph and vectors are layers, not competitors (section 2.5).
- **The run populates the graph; the traversal never assumes it.** Removing the `gr.ground()` call
  also removes the on-demand symbol build, so the traversal reads nothing unless the run has
  populated the graph. The grounding path therefore ensures the unified graph reflects the current
  project before it traverses: extraction (29a) and doc ingestion (29b) run IN THE LIVE RUN - at run
  start or first grounding - emitting the events that fold into nodes, with 29a's supersede-on-change
  keeping it current so unchanged files are not re-extracted. This is the wiring 29a/29b deliberately
  left out (their extraction entry points had no production caller); 29c is where the pass actually
  runs, closing the "green tests, empty prod graph" gap.

Expected consequence (not a done-when): the `symbols` blast-radius path folds into the projection
and the two-view struct + seed-vs-precise workaround collapse - est. ~2000-2400 LOC removed. This
spec is judged by the behavior below, not by a line count.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any external
  tool or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- Determinism by construction: traversal results are sorted; tier filters are stable set
  operations over `BTreeSet`.
- Load-bearing decisions preserved: the SAFE superset stays a grep-superset (section 2.4); cross-run
  grounding is NOT scoped to the active run; the gate reads only the result channel.

## Done when

- [ ] a test proves structural grounding is ONE seeded traversal over the unified graph: for a
  touched file, a single `subgraph` traversal returns its code neighborhood, the decisions/findings
  about it, and its design-intent nodes together - with no separate structural-grounder call
  stitched in `build_prompt_with_failure`. This criterion OWNS the unified traversal replacing the
  two-store stitch; it ASSUMES a populated graph and does NOT own population (criterion 5).
- [ ] a test proves the CONFIDENCE-TIER blast radius: the precise seed is the EXTRACTED subgraph;
  the safe superset is EXTRACTED u INFERRED u AMBIGUOUS; and the safe superset remains a superset of
  the grep union (section 2.4). This criterion OWNS the tier-filter blast radius replacing
  `BlastRadius{precise,safe,serialize}`; it does NOT own the traversal (criterion 1).
- [ ] a test proves DESIGN-INTENT grounding by traversal: an agent whose blast radius touches file
  F retrieves, by graph traversal (not vector similarity), the handbook rule that GOVERNS F and the
  RA section that SPECIFIES it. This criterion OWNS deterministic design-intent grounding (the
  highest-value new capability); it does NOT own the traversal or tier filter (criteria 1-2).
- [ ] a test proves `turbovec` NL retrieval is RETAINED and complementary: a symbol-free query
  still resolves via the vector index and its result can seed the unified traversal. This criterion
  OWNS vector-retention; it does NOT own the graph traversal, tiers, or design-intent grounding
  (criteria 1-3).
- [ ] a test proves the unified graph is POPULATED BY A LIVE RUN over the real project - not only by
  test fixtures: exercising the actual grounding path (`build_prompt_with_failure`) causes the run to
  extract the project's real source (29a) and design docs (29b) into `CodeEntityExtracted` /
  `EdgeInferred` / `DocConcept` / `DocLink` events that fold into the graph, so a seeded traversal
  returns REAL nodes the run itself ingested (asserted against actual project symbols, never
  test-injected rows). Re-extraction of a changed file SUPERSEDES via 29a's mechanism; unchanged files
  are not re-ingested. This criterion OWNS end-to-end production ingestion - the extraction pass
  actually RUNS in a live run and populates the graph; it does NOT own the traversal, tier filters,
  design-intent retrieval, or vector retention (criteria 1-4).
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
