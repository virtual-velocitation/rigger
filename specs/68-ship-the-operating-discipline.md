# 68 - Ship the operating discipline: a skill registry, an operating-rigger skill, guardrail surfaces

**Goal:** make operating rigger correctly the natural path and operating it incorrectly the hard
one, for every consumer. The operational knowledge exists only as session lore today, and the
recorded costs of that gap are concrete: a consumer facing a bloated event store reached for RAW
SQL against `events.db` because no supported surface existed; the resume-vs-fresh distinction and
the escalation protocol (apply the bounded remedy on the unit branch, relaunch fresh) live in one
operator's memory; a bare `rigger reset` is an opaque error (`expected --runs`) that does not even
enumerate what is prunable; and nothing warns when the persisted symbols index has drifted from
the tree or when a pre-fix log carries massive derived-type duplication. Three deliverables: a
binary-owned SKILL REGISTRY so skills ship as a set (adding one is content, never plumbing), an
`operating-rigger` skill covering the three operational lifecycles, and guardrails on the
commands themselves so the wrong move meets a refusal or a menu, not a mystery.

## Design

- **The skill registry** (`src/main.rs`, generalizing the `install_skill` seam at :7685;
  `src/docs.rs`): the binary owns ONE registry of embedded skills - `using-rigger`,
  `planning-a-spec`, `operating-rigger` - each entry carrying its rendered content, its committed
  source path (`skills/<name>/SKILL.md`), and its install path (`.claude/skills/<name>/`).
  `rigger docs` renders the SET, `rigger setup` installs the SET (drift-aware and
  non-destructive per entry, per-repo overlay honored per skill, exactly the existing
  `using-rigger` semantics), and the docs-drift gate covers every entry. Adding skill N+1 is
  adding a registry entry and its content - no new install function, no new gate wiring. This
  criterion's structure is what spec 66's planning-a-spec install rides.
- **The `operating-rigger` skill** (registry content; committed source at
  `skills/operating-rigger/SKILL.md`): the operations companion to `using-rigger` (driving) and
  `planning-a-spec` (authoring), organized by lifecycle, each section stating the correct move,
  the tell that you need it, and the named anti-move:
  - STORE: what `events.db` / `graph.db` / `progress.db` each are (the log is the source of
    truth; the graph is a rebuildable projection; progress is non-replayed telemetry); hygiene
    via `rigger reset --runs` (graph dead-run accumulation) and `rigger reset --derived`
    (log-side derived-index duplication); raw SQL against any store file is the named
    anti-move (it bypasses revision/cursor invariants the supported prunes preserve).
  - GRAPH: `rigger graph build` for a cold build, `rigger reindex` when the persisted symbols
    index has drifted from the tree (the tell: lookups name entities the tree no longer holds,
    or validate's staleness warning below); the lookup verbs recap with a pointer to
    `using-rigger` as their owner.
  - RUNS: preflight is `rigger validate` (model drift, config, and the advisories below);
    launch through the one blessed driver; RESUME an interrupted run by relaunching the driver
    WITHOUT fresh (the conductor adopts and replays); `--fresh` is ONLY for a run wedged
    terminal on its spec; an ESCALATED unit hands you a bounded remedy - apply exactly it on
    the unit's durable branch, then relaunch fresh so the panel verifies and the loop
    integrates; hand-driving `rigger step`, hand-merging unit branches, and deleting loop-owned
    worktrees are the named anti-moves.
- **Bare `rigger reset` is a menu, not an error** (`src/main.rs::cmd_reset`): with no flags it
  exits 0 and prints each prunable accumulation with its MEASURED reclaimable size and the flag
  that prunes it (`--runs`: dead-run graph rows and bytes; `--derived`: redundant derived-index
  rows and bytes), so the operator discovers the safe surface by running the obvious command.
  With a flag, behavior is unchanged.
- **`rigger validate` gains two operational advisories** (`src/main.rs::cmd_validate`,
  advisory-warn like the model-drift line, never failing validation): (a) INDEX STALENESS -
  when the persisted symbols index disagrees with the tree beyond a small tolerance (measured
  by the existing per-file content hashes), warn and name `rigger reindex`; (b) LOG BLOAT -
  when the derived-index types' duplication factor in the log exceeds a threshold (distinct
  payload keys vs rows, one aggregate query), warn with the measured factor and name
  `rigger reset --derived`. Consumers upgrading with pre-fix bloated logs meet the fix at the
  surface they already run preflight.

## Notes (non-criteria)

- The three skills partition cleanly: using-rigger = driving a run, planning-a-spec = authoring
  the spec, operating-rigger = the store/graph/run lifecycles around them; each cross-links the
  others by name rather than repeating content.
- Guardrails are advisory-or-menu, never new failure modes: nothing here can fail a run or a
  validation that passes today.
- No new event type is introduced anywhere in this spec.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to prior
  harnesses or to projects unrelated to the mechanism.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- The docs-drift gate stays green over the whole registry: every rendered skill matches its
  committed source, and `rigger setup` remains non-destructive on rerun.
- One measurement authority per advisory: the staleness check reuses the symbols store's
  content hashes and the bloat check reuses the store's own aggregates - no second parser or
  shadow accounting.

## Done when

- [ ] a test proves the REGISTRY: `rigger setup` installs every registry skill (drift-aware,
  overlay-honoring, non-destructive on rerun), `rigger docs` renders the same set, and the
  registry is the single enumeration both consume - pinned so adding an entry cannot bypass
  either surface. This criterion OWNS the registry structure.
- [ ] a test proves the OPERATING SKILL RENDERS TRUE: the `operating-rigger` render covers the
  three lifecycles with their named anti-moves (raw SQL, hand-stepping, hand-merging,
  worktree deletion), states the resume-vs-fresh rule and the escalation bounded-remedy
  protocol, and its command references are accuracy-pinned against the binary's real surface.
  This criterion OWNS the skill content; the install path is criterion 1's, NOT this one's.
- [ ] a test proves the RESET MENU: flagless `rigger reset` exits 0 and prints each prunable
  accumulation with a measured size and its flag, on both an empty and a populated store;
  flagged behavior is byte-for-byte unchanged. This criterion OWNS the reset surface.
- [ ] a test proves the VALIDATE ADVISORIES: a tree drifted from its persisted index draws the
  staleness warning naming `rigger reindex`, a log seeded with duplicated derived events
  draws the bloat warning with the measured factor naming `rigger reset --derived`, a clean
  store draws neither, and validation's exit status is unchanged by both. This criterion OWNS
  the advisory surface.
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
