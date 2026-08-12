# 68 - Ship the operating discipline: a skill registry, per-operation skills, guardrail surfaces

**Goal:** make operating rigger correctly the natural path and operating it incorrectly the hard
one, for every consumer. The operational knowledge exists only as session lore today, and the
recorded costs of that gap are concrete: a consumer facing a bloated event store reached for RAW
SQL against `events.db` because no supported surface existed; the resume-vs-fresh distinction and
the escalation protocol (apply the bounded remedy on the unit branch, relaunch fresh) live in one
operator's memory; a bare `rigger reset` is an opaque error (`expected --runs`) that does not even
enumerate what is prunable; and nothing warns when the persisted symbols index has drifted from
the tree or when a pre-fix log carries massive derived-type duplication. Three deliverables: a
binary-owned SKILL REGISTRY so skills ship as a set (adding one is content, never plumbing), a
FAMILY OF PER-OPERATION SKILLS - one operation per skill, its symptoms in the description,
because the description is the routing layer and a monolithic manual routes poorly and loads
wastefully - and guardrails on the commands themselves so the wrong move meets a refusal or a
menu, not a mystery.

## Design

- **The skill registry** (`src/main.rs`, generalizing the `install_skill` seam at :7685;
  `src/docs.rs`): the binary owns ONE registry of embedded skills - `using-rigger`,
  `planning-a-spec`, and the per-operation family below - each entry carrying its rendered content, its committed
  source path (`skills/<name>/SKILL.md`), and its install path (`.claude/skills/<name>/`).
  `rigger docs` renders the SET, `rigger setup` installs the SET (drift-aware and
  non-destructive per entry, per-repo overlay honored per skill, exactly the existing
  `using-rigger` semantics), and the docs-drift gate covers every entry. Adding skill N+1 is
  adding a registry entry and its content - no new install function, no new gate wiring. This
  criterion's structure is what spec 66's planning-a-spec install rides.
- **Per-operation skills, not a manual** (registry content; committed sources at
  `skills/rigger-<operation>/SKILL.md`): each operation ships as its OWN small skill whose
  DESCRIPTION carries the tells (the symptoms that mean you need it) - descriptions are the
  routing layer the harness matches against the task, so the right procedure loads at the
  right moment and nothing else rides along. Each skill body is one procedure: the correct
  move, the named anti-move, and cross-links by name to its neighbors. The family:
  - `rigger-reset-store` - store hygiene. Tells: disk growth, validate's bloat advisory, a
    slow or memory-heavy replay. Body: what `events.db` / `graph.db` / `progress.db` each are
    (log = source of truth, graph = rebuildable projection, progress = non-replayed
    telemetry), `--runs` vs `--derived` and what each prunes, flagless reset as the menu.
    Anti-move: raw SQL against any store file (it bypasses the revision/cursor invariants the
    supported prunes preserve).
  - `rigger-build-graph` - cold-build the knowledge graph on a new or moved project. Tells:
    empty lookups on a repo that has code, first setup. Anti-move: re-ingesting by deleting
    store files.
  - `rigger-reindex` - refresh the persisted symbols index. Tells: lookups naming entities
    the tree no longer holds, validate's staleness advisory, batches describing stale
    content. Anti-move: rebuilding the whole graph to fix an index-freshness problem.
  - `rigger-resume-a-run` - continue interrupted work. Tells: a driver that died (quota,
    crash, sleep), "agents in flight" with stale heartbeats. Body: relaunch the driver
    WITHOUT fresh (the conductor adopts and replays); `--fresh` is ONLY for a run wedged
    terminal on its unchanged spec. Anti-moves: hand-driving `rigger step`, a reflexive
    `--fresh` that orphans a resumable run.
  - `rigger-handle-an-escalation` - act on a unit the loop handed back. Tells: "escalated
    (awaiting a human)" in status. Body: read the final adjudication's bounded remedy, apply
    EXACTLY it on the unit's durable branch, relaunch fresh so the panel verifies and the
    loop integrates. Anti-moves: hand-merging the unit branch, re-implementing beyond the
    remedy's bounds.
  Driving a run stays `using-rigger`'s; authoring stays `planning-a-spec`'s; each per-op
  skill names them rather than repeating them. The set is expected to GROW (the registry
  makes an added operation a content entry); a new operational surface lands with its skill.
- **Bare `rigger reset` is a menu, not an error** (`src/main.rs::cmd_reset`): with no flags it
  exits 0 and prints each prunable accumulation with its MEASURED reclaimable size and the flag
  that prunes it (`--runs`: dead-run graph rows and bytes; `--derived`: redundant derived-index
  rows and bytes), so the operator discovers the safe surface by running the obvious command.
  On a backend where a prune is unavailable (the server backend, where `--derived` refuses by
  design), the menu SAYS SO on that line rather than omitting it or faking a size - the menu
  is honest per backend. With a flag, behavior is unchanged.
- **`rigger validate` gains two operational advisories** (`src/main.rs::cmd_validate`,
  advisory-warn like the model-drift line, never failing validation): (a) INDEX STALENESS -
  when the persisted symbols index disagrees with the tree, warn and name `rigger reindex`.
  COST-BOUNDED by construction: the check compares the index's path set against the tree's
  (added/removed files, a walk without reads) plus the existing per-file content hashes of a
  small deterministic sample - never a full-tree rehash, because validate is a preflight an
  operator must not learn to skip for being slow; (b) LOG BLOAT -
  when the derived-index types' duplication factor in the log exceeds a threshold (distinct
  payload keys vs rows, one aggregate query), warn with the measured factor and name
  `rigger reset --derived`. Consumers upgrading with pre-fix bloated logs meet the fix at the
  surface they already run preflight.

## Notes (non-criteria)

- The skill surface partitions cleanly: using-rigger = driving a run, planning-a-spec =
  authoring the spec, and one small skill per operation around them; each cross-links its
  neighbors by name rather than repeating content, and no skill is a manual.
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
- [ ] a test proves the PER-OPERATION SKILLS RENDER TRUE: every skill in the operation family
  renders with a symptom-carrying description (its tells), exactly one operation's procedure,
  its named anti-move (raw SQL for reset-store, whole-rebuild for reindex, reflexive fresh
  for resume, hand-merge for escalation), and command references accuracy-pinned against the
  binary's real surface - and no registry skill's body exceeds one operation's scope. This
  criterion OWNS the skill content; the install path is criterion 1's, NOT this one's.
- [ ] a test proves the RESET MENU: flagless `rigger reset` exits 0 and prints each prunable
  accumulation with a measured size and its flag, on both an empty and a populated store;
  flagged behavior is byte-for-byte unchanged. This criterion OWNS the reset surface.
- [ ] a test proves the VALIDATE ADVISORIES: a tree drifted from its persisted index draws the
  staleness warning naming `rigger reindex`, a log seeded with duplicated derived events
  draws the bloat warning with the measured factor naming `rigger reset --derived`, a clean
  store draws neither, and validation's exit status is unchanged by both. This criterion OWNS
  the advisory surface.
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
