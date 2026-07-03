# 05 - Review hardening, model stamping, and setup hygiene

**Goal:** close design-intent Gaps 4, 5, 6, and 9: spec global constraints get a mechanical gate and a named adjudicator check, the event log records which model actually ran, and `rigger setup` stops drifting and stops dirtying `git status`.

This spec is intended to run on the stepwise driver from spec 04, based on its run branch. Its units are mutually disjoint on purpose: they are the first live exercise of the new fan-out.

## Problem

Four residual gaps after spec 04:

- **Gap 4:** u1 of PR #7 shipped em dashes despite the spec's explicit global constraint; the implementer violated it and all three review tiers missed it. Constraints the gates cannot see rely on ambient attention.
- **Gap 6:** model tiers are aliases by design, so the event log is the only place the resolved model could be recorded - and it is not recorded anywhere. Cross-run quality comparisons across model upgrades are unanswerable.
- **Gaps 5 + 9:** the installed `.claude/workflows/rigger.js` silently drifts from the embedded source until someone re-runs setup; and setup scaffolds generic default agents next to the repo's customized ones and leaves `.claude/` and `.rigger/shim/` untracked, so every setup permanently dirties `git status`.

## Design

**Style gate (Gap 4a).** Mechanically checkable style constraints become a gate. Add a `style` gate to `.rigger/workflow.yml` that fails when the unit's diff against the run base introduces an em dash (U+2014) in any text - code, comments, docs, prompts. Wire it into the implement stage's gate list. The gate command lives with the other gates and runs in the unit worktree.

**Adjudicator constraint recheck (Gap 4b).** The adjudicator's persona gains an explicit, named step: re-read the spec's "Global constraints" section and verify each constraint against the diff before issuing a verdict. Non-mechanical constraints get a named check instead of ambient attention.

**Model stamping (Gap 6).** Every spawn's recorded events carry both the model alias requested and the resolved model that actually ran. The alias is known at request time (stamp it on the spawn-request event); the resolved model is known only inside the running agent (the harness resolves aliases), so the thin driver instructs each worker to include its resolved model id in its `rigger result --meta`, and the conductor copies both onto the unit events it emits for that spawn. No new event types.

**Setup hygiene (Gaps 5 + 9).** `rigger setup` becomes safely re-runnable and drift-aware:

- Compares the installed `.claude/workflows/rigger.js` against the embedded copy; refreshes on mismatch and says so. A no-op run on an up-to-date repo prints nothing surprising and changes nothing.
- Does not scaffold a default agent when the workflow's referenced agents already exist; scaffolding is for empty repos.
- Writes `.gitignore` entries for the machine-local installs it creates (`.claude/`, `.rigger/shim/`) when they are not already ignored or tracked.
- `rigger validate` warns when the installed workflow differs from the embedded one, and flags tracked `.rigger/` files carrying uncommitted modifications - config drift is surfaced, not discovered by accident.
- This repo's stray scaffolded duplicates (`implementer.md`, `devils-advocate.md`, `reviewer.architecture.md`, `reviewer.technical.md` under `.rigger/agents/`) are removed; the committed, customized agents are the fleet.

**Agent import (setup offers a starting fleet).** Writing a fleet from scratch is the highest-friction step of adopting the loop, and ready-made collections exist in the same Markdown-with-YAML-frontmatter shape - notably [agency-agents](https://github.com/msitarzewski/agency-agents) (MIT, 200+ agents). Two pieces, both offline:

- When `rigger setup` scaffolds default agents (the empty-repo path only), it prints a pointer to the agency-agents collection and the handbook's authoring-agents chapter as the way to grow past the scaffold.
- `rigger setup --agents <dir>` imports agent definition files from a local directory (a checkout of any collection): copies each `.md` into `.rigger/agents/`, normalizes the identity frontmatter field to Rigger's `id:`, refuses to overwrite an existing agent file, and runs the same validation `rigger validate` applies. No network access in setup; the user clones the collection themselves.

**Store-open and courier hardening (from result-cmd's post-run review).** The review of the remediated result-cmd unit produced four non-blocking findings the adjudicator dispatched here (see ReviewVerdict `adj-result-cmd-remediation`):

- The shared store-open seam (`create_dir_all(RIGGER_DIR)` + cwd-relative `db_path`, mirrored by `cmd_result`, `cmd_emit`, and other commands) silently fabricates a fresh `.rigger/events.db` when run from the wrong cwd - most plausibly a unit worktree - printing success while the real spawn stays parked. Harden it once for all commands: when the cwd has no existing `.rigger` store, refuse (or walk up to the repo root) instead of fabricating.
- `rigger result` does one cheap pre-write read of the stream and prints a stderr advisory when the id matches no recorded spawn request (orphan result) or supersedes an existing result at position N. Advisory only - pre-recording stays legitimate.
- `rigger result --if-absent` records atomically only when no result exists for the id (one transaction), and the thin driver's death courier uses it in place of the two-process `rigger reported <id> ||` guard, closing the TOCTOU window that could clobber a self-report landing in the gap.

## Global constraints

- Hyphens, not em dashes, in every file this spec touches (the style gate this spec adds must itself pass on this spec's own units).
- No new event types; stamp metadata onto events already emitted.
- Idiomatic Rust; every unit leaves the workspace green (fmt, clippy, build, test, style).

## Done when

- [ ] a `style` gate in `.rigger/workflow.yml` fails a unit whose diff introduces an em dash, runs in the unit worktree as part of the implement stage's gate list, and passes on a clean diff
- [ ] the adjudicator persona contains a named step that re-reads the spec's Global constraints section and verifies each constraint against the diff before the verdict
- [ ] every spawn's recorded events carry the requested model alias and the resolved model id that ran, the latter reported by the worker via `rigger result --meta` and copied onto the spawn's unit events
- [ ] `rigger setup` is re-runnable: it detects and refreshes a drifted installed workflow, reports the refresh, and is a silent no-op when nothing drifted
- [ ] `rigger setup` skips scaffolding default agents when the workflow's referenced agents exist, and writes `.gitignore` entries for `.claude/` and `.rigger/shim/` when they are neither ignored nor tracked
- [ ] `rigger validate` warns on installed-vs-embedded workflow drift and flags tracked `.rigger/` files with uncommitted modifications
- [ ] the four stray scaffolded agent duplicates under `.rigger/agents/` are removed from the working tree and cannot be re-scaffolded by a rerun of setup on this repo
- [ ] `rigger setup --agents <dir>` imports agent `.md` files from a local directory into `.rigger/agents/` (normalizing the identity field to `id:`, never overwriting an existing agent, validating the result), and the empty-repo scaffold path prints a pointer to the agency-agents collection and the authoring-agents handbook chapter
- [ ] store-opening commands refuse (or walk up) instead of fabricating a fresh `.rigger/events.db` when run from a cwd with no existing store, and `rigger result` prints stderr advisories for an orphan id and for superseding an existing result
- [ ] `rigger result --if-absent` records atomically only when the id has no result, and the thin driver's death courier uses it instead of the two-process `rigger reported ||` guard
