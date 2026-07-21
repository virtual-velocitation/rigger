# 19a - Observability surfaces: make a run legible without forensics

**Goal:** a run's decisive state - why a unit is stuck, where the blessed path and dashboard
are, what the loop is actually building - exists but is buried or requires grepping ps and
journals. Surface it. Nothing here changes control flow. This is the first of three specs
split from Workstream B of `docs/architecture-addendum-pit-of-success.md` (the monolithic
spec 19 over-refined in decomposition; each split spec is small and atomic).

## Design

Builds on `rigger status` (`cmd_status` in `src/main.rs`), the stats attribution
(`append_review_quality` in `src/main.rs`), the dashboard (`src/dash.rs`,
`dash::DEFAULT_PORT`), setup (`cmd_setup` in `src/main.rs`), the native workflow
(`workflows/rigger.js`: `meta`, `phaseOf`, the `log()` sites), the wire (`SpawnRequest` in
`src/spawn.rs`), and the unit's criterion text (`Stage.coverage` in `src/conductor.rs`).

**Unit 1 - current-blocker line (touches `src/main.rs`, `src/dash.rs`).** A pure classifier
over run state yields, for each in-flight unit, a one-line current blocker from a fixed set
of kinds: building (attempt n), reviewing, reject-recurrence (#n/max with the cause),
approved-not-integrated (verdict not on result channel), escalated, budget (spent/cap). It
reads the same signals `rigger stats` already computes; the verdict-channel diagnostic that
today lives only in `rigger stats` becomes one of the kinds. Surfaced identically in
`rigger status` and on the dashboard.

**Unit 2 - setup discoverability (touches `src/main.rs`).** `cmd_setup`'s output names only
the `/rigger` install line. Add an orientation block at the end: the blessed native path
(`/rigger <spec>`, visible in `/workflows`), the dashboard (`rigger dash`, its
`127.0.0.1:<DEFAULT_PORT>` URL), and `rigger workflow` / `rigger run` labelled explicitly as
the headless twins. This is the setup OUTPUT TEXT only; the dashboard's runtime behavior is
spec 19b's, not this spec's.

**Unit 3 - workflow tagline (touches `workflows/rigger.js`).** The static `meta.description`
reads as internal plumbing ("driven THINLY", courier, SpawnResult) and is the tagline shown
in BOTH the skills list and the `/workflows` header. Rewrite it to a jargon-free,
user-useful line that says what the workflow does and when to use it; the architecture
explanation moves to the file's header comment. `meta` stays a pure static literal.

**Unit 4 - live work-line (touches `src/spawn.rs`, `src/conductor.rs`, `workflows/rigger.js`).**
Add a human-readable `title` field to `SpawnRequest`, derived from the unit's `Stage.coverage`
(the criterion text, trimmed), threaded through `rigger step`. The thin driver renders it in
the `log()` narrator and the per-unit progress-group detail so the live display shows the
actual work (e.g. "Building u-domain - feature-off resolver test") rather than only
`u-domain:build`. Omitted from the wire when empty (back-compatible).

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any
  external tool or project in code, comments, or commit messages.
- No new event types; the classifier reads existing run state.
- Determinism by construction for anything serialized (`BTreeMap`/`BTreeSet`/sorted `Vec`).
- BOTH feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D
  warnings`; `cargo test` - on default features AND `--no-default-features`.

## Done when

- [ ] a production-render test proves `rigger status` and the dashboard each emit a one-line current-blocker per in-flight unit from one shared classifier, covering at least building, reject-recurrence (#n/max), approved-not-integrated, escalated, and budget
- [ ] a fixture proves `rigger setup` output names the blessed native `/rigger` path, the dashboard `127.0.0.1:<port>` URL, and labels `rigger workflow`/`rigger run` as the headless twins
- [ ] a fixture proves the static `meta.description` in `workflows/rigger.js` contains none of the plumbing terms ("driven THINLY", "courier", "SpawnResult") and reads as a user-useful tagline
- [ ] a test proves `SpawnRequest` carries a `title` derived from `Stage.coverage` and the thin driver renders it in the live per-unit progress
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`)
