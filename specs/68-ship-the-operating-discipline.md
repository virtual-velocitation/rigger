# 68 - Ship the operating discipline: a skill registry, per-operation skills, guardrail surfaces

**Goal:** make operating rigger correctly the natural path and operating it incorrectly the
hard one. Today the operational knowledge is session lore; recorded costs: a consumer ran raw
SQL against `events.db` for lack of a supported surface; resume-vs-fresh and the escalation
protocol live in one operator's memory; bare `rigger reset` is an opaque error; nothing warns
of a drifted symbols index or a pre-fix bloated log. Three deliverables: a binary-owned skill
REGISTRY (adding a skill is content, never plumbing), a family of PER-OPERATION skills (the
description carries the symptoms - it is the routing layer; a monolithic manual routes
poorly), and guardrails so the wrong move meets a refusal or a menu, not a mystery.

## Design

- **The skill registry** (`src/main.rs`, generalizing the `install_skill` seam at :7685;
  `src/docs.rs`): ONE registry of embedded skills - `using-rigger`, `planning-a-spec`, and
  the family below - each entry carrying rendered content, committed source path
  (`skills/<name>/SKILL.md`), and install path (`.claude/skills/<name>/`). `rigger docs`
  renders the set, `rigger setup` installs the set (drift-aware, non-destructive, overlay
  honored per entry - the existing `using-rigger` semantics), the docs-drift gate covers
  every entry. Spec 66's planning-a-spec install rides this structure.
- **Per-operation skills, not a manual** (registry content; sources at
  `skills/rigger-<operation>/SKILL.md`): one operation per skill; the description carries the
  tells; the body is one procedure plus its named anti-move, cross-linking neighbors by name:
  - `rigger-reset-store` - store hygiene. Tells: disk growth, the bloat advisory, slow
    replay. Body: what events.db/graph.db/progress.db each are (log = truth, graph =
    rebuildable projection, progress = non-replayed telemetry), `--runs` vs `--derived`,
    flagless reset as the menu. Anti-move: raw SQL against any store file.
  - `rigger-build-graph` - cold-build the graph. Tells: empty lookups on a repo with code,
    first setup. Anti-move: re-ingesting by deleting store files.
  - `rigger-reindex` - refresh the symbols index. Tells: lookups naming entities the tree no
    longer holds, the staleness advisory. Anti-move: whole-graph rebuild for an
    index-freshness problem.
  - `rigger-resume-a-run` - continue interrupted work. Tells: dead driver (quota, crash,
    sleep), "agents in flight" with stale heartbeats. Body: relaunch WITHOUT fresh (the
    conductor adopts and replays); `--fresh` only for a run wedged terminal on its unchanged
    spec. Anti-moves: hand-driving `rigger step`, reflexive `--fresh`.
  - `rigger-handle-an-escalation` - act on a unit the loop handed back. Tells: "escalated
    (awaiting a human)". Body: read the final adjudication's bounded remedy, apply EXACTLY
    it on the unit's durable branch, relaunch fresh. Anti-moves: hand-merging the unit
    branch, re-implementing beyond the remedy.
  Driving stays `using-rigger`'s; authoring stays `planning-a-spec`'s. The set grows by
  registry entry. ONE PROHIBITION renders into EVERY skill, verbatim: an agent never
  installs, replaces, or modifies the operator's installed rigger binary - operator-only.
  (Cost of the imitation: an agent cargo-installed its worktree build over the operator
  binary mid-run; cursor semantics changed silently; two false fixpoints followed. Tree
  behavior is invoked by explicit path, and only to render.)
- **Bare `rigger reset` is a menu, not an error** (`src/main.rs::cmd_reset`): flagless, it
  exits 0 and prints each prunable accumulation with its MEASURED reclaimable size and its
  flag. A backend where a prune is unavailable says so on that line - honest per backend.
  With a flag, behavior unchanged.
- **`rigger validate` gains two advisories** (`src/main.rs::cmd_validate`, advisory-warn,
  never failing): (a) INDEX STALENESS - index vs tree disagreement warns and names
  `rigger reindex`; cost-bounded: path-set comparison plus existing content hashes of a
  small deterministic sample, never a full-tree rehash. (b) LOG BLOAT - derived-type
  duplication factor above threshold (distinct payload keys vs rows, one aggregate query)
  warns with the measured factor and names `rigger reset --derived`.

## Notes (non-criteria)

- Partition: using-rigger = driving, planning-a-spec = authoring, one small skill per
  operation around them; no skill is a manual.
- Guardrails are advisory-or-menu, never new failure modes.
- No new event type is introduced anywhere in this spec.
- Both feature lanes audited fresh at this spec's integrated tip (criteria 1-4 already merged,
  zero code change needed by this criterion): fmt --check clean; clippy --all-targets -D
  warnings clean on default features AND `--no-default-features`; cargo test exits 0 on both -
  default 1994 passed, 0 failed, 2 ignored, 104 suites; `--no-default-features` 1876 passed,
  0 failed, 2 ignored, 104 suites.
- One pre-existing gap surfaced by review during criterion 4, out of this criterion's scope to
  fix (a follow-up for a future hardening unit, not a defect of any landed diff):
  `grounder::symbols::mod.rs`'s staleness sample selects changed-content candidates via
  `indexed.intersection(&tree_paths).take(STALENESS_SAMPLE_SIZE)` over two `BTreeSet`s, which
  `BTreeSet::intersection` returns in ascending order - so the sample is permanently the
  lexicographically-first `STALENESS_SAMPLE_SIZE` (8) paths, every run, forever, on a stable
  file list. On any tree bigger than 8 files (this repo included), a content-only edit outside
  the alphabetic head is invisible to the staleness advisory no matter how many times
  `rigger validate` runs. This satisfies the Design text and this spec's own Done-when clause
  as literally written (a small deterministic sample, never a full-tree rehash; a drifted
  single-file fixture does draw the warning), so it is not a defect of criterion 4's diff - but
  it undermines the advisory's real-world usefulness and is named here so it is not lost once
  this spec closes. Suggested remedy: key sample selection off something that varies run-to-run
  while staying reproducible within one run (a rotating window seeded by index generation, or a
  stable hash of path plus a slowly-changing salt) so successive `validate` invocations
  eventually cover the whole tree instead of the same head forever.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to prior
  harnesses or to projects unrelated to the mechanism.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- The docs-drift gate stays green over the whole registry; `rigger setup` remains
  non-destructive on rerun.
- One measurement authority per advisory: staleness reuses the symbols store's content
  hashes; bloat reuses the store's own aggregates - no shadow accounting.
- Disposition for criteria 1-4 (registry, per-operation skills, reset menu, validate
  advisories): each may be satisfied EITHER by fresh implementation OR by independently
  re-verifying already-integrated code at the run's base commit - the evidence bar for the
  re-verify path is rerunning that criterion's own pinned tests plus both feature lanes
  (fmt, clippy, test on default and `--no-default-features`); a zero-diff confirm-only unit
  is acceptable evidence for that path, mirroring criterion 5's audit-only precedent.

## Done when

- [ ] a test proves the REGISTRY: `rigger setup` installs every registry skill (drift-aware, overlay-honoring, non-destructive on rerun), `rigger docs` renders the same set, and the registry is the single enumeration both consume - pinned so adding an entry cannot bypass either surface; a test also proves the ONE PROHIBITION (an agent never installs, replaces, or modifies the operator's rigger binary) renders verbatim into all seven registry entries - the five-member per-operation family plus the two pre-existing `using-rigger` and `planning-a-spec` skills. This criterion OWNS the registry structure AND the prohibition's presence in every entry. Disposition: satisfied either by fresh implementation or by independently re-verifying already-integrated code at the run's base commit, evidence bar = this criterion's own pinned tests plus both feature lanes (fmt, clippy, test on default and `--no-default-features`); a zero-diff confirm-only unit is acceptable evidence for the re-verify path, mirroring criterion 5's audit-only precedent.
- [ ] a test proves the PER-OPERATION SKILLS RENDER TRUE: every skill in the family renders with a symptom-carrying description, exactly one operation's procedure, its named anti-move, and command references accuracy-pinned against the binary's real surface - and no registry skill's body exceeds one operation's scope. This criterion OWNS the skill content; the install path is criterion 1's, NOT this one's, and the operator-binary prohibition text is criterion 1's, NOT this one's. Disposition: satisfied either by fresh implementation or by independently re-verifying already-integrated code at the run's base commit, evidence bar = this criterion's own pinned tests plus both feature lanes (fmt, clippy, test on default and `--no-default-features`); a zero-diff confirm-only unit is acceptable evidence for the re-verify path, mirroring criterion 5's audit-only precedent.
- [ ] a test proves the RESET MENU: flagless `rigger reset` (`src/main.rs::cmd_reset`) exits 0 and prints each prunable accumulation with a measured size and its flag, on both an empty and a populated store, proven in `tests/cli.rs` and `tests/reset_menu.rs`; flagged behavior is byte-for-byte unchanged. This criterion OWNS the reset surface. Disposition: satisfied either by fresh implementation or by independently re-verifying already-integrated code at the run's base commit, evidence bar = this criterion's own pinned tests plus both feature lanes (fmt, clippy, test on default and `--no-default-features`); a zero-diff confirm-only unit is acceptable evidence for the re-verify path, mirroring criterion 5's audit-only precedent.
- [ ] a test proves the VALIDATE ADVISORIES: a drifted index draws the staleness warning naming `rigger reindex`, a log seeded with duplicated derived events draws the bloat warning with the measured factor naming `rigger reset --derived`, a clean store draws neither, and validation's exit status is unchanged by both. This criterion OWNS the advisory surface. Disposition: satisfied either by fresh implementation or by independently re-verifying already-integrated code at the run's base commit, evidence bar = this criterion's own pinned tests plus both feature lanes (fmt, clippy, test on default and `--no-default-features`); a zero-diff confirm-only unit is acceptable evidence for the re-verify path, mirroring criterion 5's audit-only precedent.
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
