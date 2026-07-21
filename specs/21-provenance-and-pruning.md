# 21 - Provenance and pruning: tell live decisions from dead-run noise

**Goal:** the context graph spans runs by design (institutional memory - a unit inherits
prior decisions so runs are not amnesiac). The cost is that decisions and findings from a
wedged or superseded run are indistinguishable from live ones, so stale noise surfaces in a
healthy run's grounding. The fix is provenance and pruning, NOT scoping grounding to the
active run (which would throw away the memory). This spec implements Workstream D of
`docs/architecture-addendum-pit-of-success.md`.

## Design

Builds on the whole-stream context graph (`Projector`, `src/contextgraph/`), the run
boundary (`RunStarted`, `current_run`, `start_fresh` in `src/run.rs`), and the read paths
`rigger peers` and grounding (`graph_context` in `src/conductor.rs`). The graph has no
run column today; attribution is derived from the event stream.

**Unit 1 - RunStarted-boundary attribution (touches `src/contextgraph/`, `src/run.rs`).**
Derive, for each decision/finding node, the run it belongs to: the run whose
`[RunStarted, next RunStarted)` event-position window contains the event that produced the
node. `LessonLearned` is exempt - it is durable cross-run value and is never attributed
away or pruned. This is a pure function of the ordered event log plus the node-to-event
mapping; the exact concretization (replay-and-tag on demand vs. a stored source-run) is an
implementation choice, but attribution must be deterministic and must place every
decision/finding in exactly one run (or mark it pre-boundary) while leaving lessons
untouched.

**Unit 2 - `rigger reset --runs` (touches `src/main.rs`, `src/contextgraph/`).** A new
command that drops decisions and findings belonging to SUPERSEDED/dead runs (every run
except the active one) from the graph, while PRESERVING `LessonLearned` and the active
run's decisions and findings. It is the supported way to shed dead-run noise without
deleting the whole store; there is no way to do this today short of wiping `graph.db`.

**Unit 3 - `rigger peers` provenance labels (touches `src/main.rs`, `src/conductor.rs`).**
`rigger peers` presents live and historical decisions identically. Label each decision as
LIVE (from the active run) or HISTORICAL (from a superseded run), using the same
attribution as Unit 1. Grounding still INCLUDES cross-run decisions by default (the
load-bearing decision is preserved); the label only makes provenance legible instead of
alarming.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any
  external tool or project in code, comments, or commit messages.
- The cross-run grounding default is PRESERVED: grounding still surfaces decisions from
  prior runs; this spec adds provenance and an opt-in prune, never scopes grounding to the
  active run.
- `LessonLearned` is never pruned and never attributed away by `reset --runs`.
- Determinism by construction: attribution and pruning are deterministic over the ordered
  event log; anything serialized uses `BTreeMap`/`BTreeSet`/sorted `Vec`.
- BOTH feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D
  warnings`; `cargo test` - on default features AND on `--no-default-features`.

## Done when

- [ ] a test proves a decision/finding is attributed to the run whose `[RunStarted, next RunStarted)` window contains its producing event, and that `LessonLearned` is never attributed away
- [ ] a fixture with two runs in one store proves `rigger reset --runs` (using the run attribution owned by the first criterion) drops the superseded run's decisions and findings while PRESERVING every `LessonLearned` and the active run's decisions and findings
- [ ] a fixture proves `rigger peers` (reusing the first criterion's run attribution) labels each decision live (active run) vs historical (superseded), and that grounding still includes cross-run decisions by default (unchanged)
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`)
