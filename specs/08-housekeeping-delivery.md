# 08 - Housekeeping delivery: the deferred list, delivered

**Goal:** deliver every item the campaign's adjudications deferred - nothing on the housekeeping list survives this spec. Two units, consolidated by blast radius (handbook rule 6), each item's disposition decided here (rule 8).

## Problem

Six small items were explicitly dispatched to "later" by adjudicators across specs 05-07, all recorded in the store's decisions and the gaps doc's closed entries:

1. The `--agents` import helpers (`normalize_identity`, `existing_agents`) duplicate `config::split_frontmatter` and the loader loop at helper level.
2. The `SCAFFOLD_AGENTS` seed still carries the four generic personas, so a truly empty repo scaffolds a stray-prone fleet (this repo is immune; fresh repos are not).
3. Landed behavior lacks pins: the scaffold-skip filter, `cmd_setup`'s dispatch boundary, `rigger init`'s silent-no-op and positive-summary paths, and the import-defeats-silent-no-op weave decision.
4. `write_if_absent` swallows a failed write: it returns `false`, the summary omits the artifact, and setup exits 0 - an honest-reporting hole on an error path.
5. Under `--if-absent`, the orphan advisory says "recording an orphan result" even when the CAS then leaves an existing result untouched.
6. Store-opening binds the NEAREST store in the bounded scope, so a shadow `events.db` inside a worktree or scratch dir eclipses the repo root's real store (today: warned by `validate`, still mis-bound by couriers).

## Design

**Unit 1 - setup and scaffold housekeeping (items 1-4). OWNS the setup/scaffold/import path.**

- **Consolidate (1):** the import path parses frontmatter and enumerates existing agents through public `config` seams; the duplicated helper logic is deleted. `config::index_agents` remains the single identity authority - no behavior change, pinned by the existing import test suite passing unchanged.
- **Align the seed (2):** `SCAFFOLD_AGENTS` and `SCAFFOLD_WORKFLOW` reference the same canonical persona set - a fresh-repo `rigger init` yields a fleet where every scaffolded agent is referenced by the scaffolded workflow and nothing stray is seeded. The four generic personas (`implementer`, `devils-advocate`, `reviewer.architecture`, `reviewer.technical`) leave the seed.
- **Pin (3):** tests for the referenced-agent scaffold-skip filter, `cmd_setup`'s argument/dispatch boundary, `rigger init`'s silent no-op and positive summary, and `--agents` import reporting on an otherwise up-to-date repo.
- **Fail loudly (4), disposition decided:** a FAILED scaffold write is an error - setup/init exit nonzero naming the artifact that could not be written. Keeping-an-existing-file remains silent success; only genuine write failures escalate.

Exclusion: store-opening and result advisories are unit 2's.

**Unit 2 - store-opening housekeeping (items 5-6). OWNS store resolution and result advisories.**

- **Honest advisory under `--if-absent` (5), disposition decided:** the orphan note never claims a recording; under `--if-absent` it states the conditional ("no spawn request is recorded for <id>; --if-absent records only if the spawn is unanswered"). The plain path's wording is unchanged.
- **Outermost store wins (6), disposition decided:** within the bounded walk scope (start dir up to the main-repo root), store resolution prefers the OUTERMOST store - the repo root's - over any nearer shadow; when a nearer shadow is bypassed, a stderr warning names both paths. A shadow can therefore never eclipse the real run stream. `validate`'s existing shadow-store residue warning stays as-is.

Exclusion: the walk's boundary itself (main-repo root, no-git-no-walk) is landed unit-9 behavior - do not rework it; this unit changes only WHICH store within the scope is chosen.

## Global constraints

- Hyphens, not em dashes, in every file this spec touches.
- NO new event types.
- Idiomatic Rust; no placeholder/TODO-stub code; every unit leaves the workspace green on both feature lanes (fmt, clippy, build, test, style).
- Every landed behavior keeps its existing tests passing unchanged; consolidation and rewording must not alter any pinned contract.

## Done when

- [ ] the setup/scaffold/import path is consolidated and hardened as one unit: import helpers route through public `config` seams with the duplicates deleted, the scaffold seed and scaffold workflow reference the same canonical persona set (no generic strays on a fresh-repo init), the scaffold-skip filter / `cmd_setup` boundary / init no-op and positive-summary / import-reporting behaviors are test-pinned, and a failed scaffold write exits nonzero naming the artifact - with the whole existing setup and import test suite passing unchanged
- [ ] store resolution and result advisories are hardened as one unit: within the bounded walk scope the outermost store wins with a stderr warning naming any bypassed shadow (pinned by a test with a planted shadow), and the `--if-absent` orphan advisory states the conditional instead of claiming a recording (pinned), with unit-9's and unit-10's existing tests passing unchanged
