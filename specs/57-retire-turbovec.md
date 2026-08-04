# 57 - Retire turbovec: the knowledge graph is the lookup surface

**Goal:** remove the vector-embedding grounder and everything that exists only to serve it. The
retention question was answered by measurement, twice: on a retrieval benchmark the vector index's
hit set was IDENTICAL to the knowledge graph's own structural retrieval (zero marginal recall); and
in a real A/B workload (the same documentation rebuild run with and without vectors) the vector
surface was invoked ZERO times by any agent in either run, while its index freshening taxed every
step and its dependency tree taxed every build and install. The knowledge graph is the lookup
surface; the vector sidecar is dead weight. Retire it: the `symbols` (structural) grounder becomes
the DEFAULT, the vector engine and its dependencies leave the tree entirely, and the README records
the measured rationale so the decision is legible to every consumer.

## Design

- **Feature and dependencies** (`Cargo.toml`): the `turbovec` cargo feature is deleted (building
  with it is rejected as unknown, like the retired store flag before it), and with it every
  dependency that exists only for embedding - the embedding engine, the ONNX runtime stack, and any
  optional dependency (e.g. the raw-flock shim) whose sole activator was the feature. The `symbols`
  feature remains and joins the default set; the default build gets LIGHTER, not different.
- **Code** (`src/`): the vector grounder module, the runtime-loader and teardown modules that exist
  only to host the ONNX/accelerator lifecycle, and the per-step index freshening leave the tree.
  The persisted embedding index under `.rigger/grounding/` is no longer read or written; `reindex`
  either retires or re-points to the symbol index (whichever the surviving command surface makes
  honest). No dead stubs remain: a removed thing is removed, not gated.
- **Grounder selection** (`src/grounder/`, `src/main.rs`): the accepted names become
  `symbols` (default) | `grep` | `nop`. `turbovec` and `hybrid` (whose whole point was composing
  vectors onto symbols) are REJECTED with a clear migration error naming the retirement and the
  default. The no-silent-degrade rule holds: an explicit request for a retired engine errors; it
  never quietly becomes something else. The committed workflow config's default and comment update
  to match.
- **Documentation and pins**: the freshly rebuilt `docs/architecture.md` (and its accuracy-pin
  tests) currently assert the vector grounder as the default - update both to the new truth. The
  README gains the retirement rationale WITH the measured numbers: identical retrieval hit-sets,
  zero invocations across both A/B workload runs, and the per-step and per-install costs removed.
  The `using-rigger` skill/handbook render (if it names grounders) re-renders consistently, keeping
  the docs-drift gate green.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any external
  tool or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- No silent degrade: retired grounder names error loudly with migration guidance; nothing falls
  back to grep implicitly (grep remains explicit opt-in only).
- The event log and the knowledge graph are untouched: this removes a retrieval sidecar, not
  knowledge. The graph remains the lookup surface for every agent.
- The removal is total: no orphaned feature gates, dead modules, unused dependencies, or stale
  documentation claims survive (the docs-drift and accuracy-pin gates enforce the last).

## Done when

- [ ] a test proves the DEFAULT GROUNDER: a default build with no configuration selects the
  `symbols` grounder, and the accepted-name set is exactly `symbols`/`grep`/`nop` - a retired name
  (`turbovec`, `hybrid`) errors with the migration message, never silently degrades. This criterion
  OWNS the selection surface.
- [ ] a test proves the DEPENDENCY DIET: the `turbovec` cargo feature no longer exists (building
  with `-F turbovec` is rejected by cargo), and the embedding/ONNX dependencies are absent from the
  dependency tree of both lanes. This criterion OWNS the packaging removal.
- [ ] a test proves the STEP SHEDS THE FRESHEN: the step path performs no embedding-index load or
  freshen (no grounding-index read/write), pinned at the step's lifecycle seam. This criterion OWNS
  the runtime removal.
- [ ] a test proves the DOCS TELL THE NEW TRUTH: the architecture document's accuracy pins assert
  the symbols default (not the retired engine), and the README carries the retirement rationale
  with the measured numbers. This criterion OWNS the documentation update.
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
