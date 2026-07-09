# 09 - Durable project identity: close Gap 20

**Goal:** project identity survives directory renames, machine moves, and shared backends - a tracked id file replaces the volatile directory basename, with deterministic minting and a one-time legacy migration.

## Problem

`project_identity()` is the git top-level's basename ([design-intent-gaps.md](../docs/design-intent-gaps.md) Gap 20). One `mv` of the checkout orphans the project's own history: every stream is namespaced `proj-<basename>-run`, so the renamed project reads an empty log. On a shared server backend, unrelated repos with the same directory name collide.

## Design (dispositions decided)

**One unit - identity resolution, minting, and migration share one blast radius** (`project_identity*` and every `Namespaced` composition point, `rigger init`/`setup`, `rigger validate`).

- **The identity is a tracked file:** `.rigger/project.id`, one trimmed line. When present, it IS the identity - everywhere `project_identity()`/`project_identity_at()` resolve today. Clones and checkouts inherit it through git, so the same logical project shares one namespace across machines and paths.
- **Minting:** `rigger init`/`setup` write the file when absent: deterministically derived from the normalized origin URL when a remote exists (normalize so ssh/https/`.git`-suffix forms of the same repo agree: lowercase host, strip scheme/user/suffix; then a short stable hash), else a random id. The minted value is reported in the setup summary (per-artifact reporting discipline, spec 05/08 lineage).
- **Legacy fallback:** with no `project.id`, resolution falls back to today's basename identity unchanged - existing checkouts keep working before setup runs.
- **One-time migration:** when opening the project store with a minted identity whose stream namespace is EMPTY while the legacy basename namespace holds events, the store's streams are renamed to the new namespace once (a store-level operation, no new event types), and a `DecisionMade` records the migration (old identity, new identity, stream count). After migration the legacy namespace is empty; re-opening is a no-op. A store where BOTH namespaces hold events is ambiguous: refuse loudly with the two identities named (operator resolves; never guess).
- **Validate nudges:** `rigger validate` warns when `.rigger/project.id` is absent (a rename away from orphaned history), consistent with its existing drift warnings.
- **Scope exclusion:** worktree-anchored identity resolution (the walk-up couriers resolving identity at the store's owning root) keeps its landed behavior - it now reads THAT root's `project.id` first, same precedence.

## Global constraints

- Hyphens, not em dashes, in every file this spec touches.
- NO new event types; the migration is recorded with the existing `DecisionMade`.
- Idiomatic Rust; both feature lanes green (fmt, clippy, build, test, style); every existing identity-dependent test keeps passing (updated only where it must now mint/read the id file).
- Backward compatibility is a hard bar: a pre-spec-09 checkout with no `project.id` behaves exactly as today until setup mints.

## Done when

- [ ] project identity resolves, in precedence order, from the tracked `.rigger/project.id` file, else the legacy basename (unchanged behavior when the file is absent); `rigger init`/`setup` mint the file when absent (normalized-origin-URL hash when a remote exists, random otherwise, reported in the summary; ssh/https/`.git` forms of one repo mint identical ids - pinned); a store holding events only under the legacy namespace migrates once to the minted identity (streams renamed, recorded via `DecisionMade`, idempotent on re-open) while a store with events under BOTH namespaces refuses loudly naming them; `rigger validate` warns when the id file is absent; and a rename-the-directory scenario test proves history survives end-to-end
