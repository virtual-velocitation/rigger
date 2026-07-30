# 56 - Rebuild the architecture document to describe the current system

**Goal:** `docs/architecture.md` is the front-door architecture document, and it has fallen behind
the system it describes: the event store is now pure configuration behind one resolution authority,
the knowledge graph carries a three-lens inspector with directed call queries and a rationale
overlay, the dashboard is a fixed-address machine singleton over an instance registry, and the
ingest is parallel, incremental, and project-scoped. Rebuild the document so a newcomer reading it
learns the system THAT EXISTS: every claim grounded in the current code and commands, every stale
claim gone. The document stays self-contained (it explains the problems each design solves; it
names no external tools or prior harnesses) and keeps its role as the top of the documentation tree
(the addenda remain the deep dives; this document orients and links).

## Design

Rewrite `docs/architecture.md` section by section against the current source:

- **The store**: the event log as the source of truth; the backend as configuration (the single
  store-resolution authority, the committed `store:` selection, the environment and per-machine
  secret-file credential channels, credential redaction); projections (graph, progress) as local
  rebuildable folds regardless of backend.
- **The knowledge graph**: target-project-only content (the de-noise principle); code + design +
  decision knowledge; the derivations (coupling communities, intent concepts) as event-sourced
  offline passes; the inspector - three lenses as an abstraction ladder, subject-by-lens
  re-projection, directed call queries with conservative resolution, the rationale overlay.
- **The loop**: spec -> unit DAG -> implement -> gates -> three-tier adversarial review ->
  integrate; bounded remediation and escalation; the recovery behaviors (reviewer re-park,
  self-healing worktrees); grounding as pull tools over the graph.
- **The dashboard**: the fixed-address singleton, the instance registry, attach-to-stores,
  run-agnostic reads, the auto-ensure and its opt-out.
- **The ingest**: parallel parse with ordered emit, batched fold, content-keyed embed skip, the
  project-scoped walk.

Accuracy is enforced by tests that pin the document to the code's own names: the document must
mention the real command and configuration surface it describes, and must not contain claims the
campaign retired.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any external
  tool or project in the document, code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- The document is SELF-CONTAINED: it explains the problem each design solves in its own words;
  a reader needs no other repository or history to follow it.
- Documentation-only where possible: the deliverable is `docs/architecture.md` plus its pinning
  tests; no behavior change to the system being described.

## Done when

- [ ] a test proves CURRENT-SURFACE ACCURACY: `docs/architecture.md` names the store-resolution
  and configuration surface that exists today (the `store:` selection, the environment variable,
  the per-machine secret file) and the graph inspector's real query surface (the three lens names
  and the directed call views). This criterion OWNS the present-tense accuracy pins.
- [ ] a test proves STALE CLAIMS ARE GONE: the document does not describe the retired shapes - a
  build-time feature flag for the server-backed store, a port-searching per-run dashboard, or a
  whole-project re-fold on every step. This criterion OWNS the staleness pins; it does NOT own
  present-tense accuracy (criterion 1).
- [ ] a test proves DOCUMENT INTEGRITY: the document is pure ASCII hyphens (no unicode dashes),
  every intra-repo link it carries resolves to a file that exists, and it links to each
  architecture addendum exactly once. This criterion OWNS the document's structural integrity.
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
