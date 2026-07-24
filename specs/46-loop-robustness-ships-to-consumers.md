# 46 - Loop robustness ships to consumers: graph hygiene, dash-artifact ignore, compacting prune

**Goal:** three loop-robustness lessons this project learned by running the loop must reach every
consumer through the binary and `rigger setup`, not live as local repo edits or operator lore. Each
generalizes beyond this repo's own agents - any project driving rigger hits the same wall - so each
belongs in the SHIPPED product:

1. **The dash runtime artifacts must be gitignored in the CONSUMER's repo.** The always-on dash writes
   `.rigger/dash.url` and `.rigger/dash.marker` (its discoverability breadcrumbs). `rigger init`/`setup`
   already appends gitignore patterns for the machine-local installs it writes (`.claude/`,
   `.rigger/shim/`), but NOT these. Left untracked-and-not-ignored they get swept into a unit worktree's
   commit by `git add`, then collide with the live dash's rewrites when the conductor merges the unit -
   `git merge` aborts with "untracked working tree files would be overwritten". This repo fixed its OWN
   `.gitignore`, which does nothing for a consumer; the fix must be in the setup-written patterns.
2. **The operator needs graph-hygiene guidance before a run.** The context graph is re-folded at the
   start of every step; on a large or long-unpruned graph the first fold is slow enough to blow the
   driver's per-command time budget and stall the first step. A consumer has no way to know this. The
   `using-rigger` skill (rendered from code, installed by setup) should tell them: keep the graph lean
   with `rigger reset --runs` before a large run, and that a very stale graph should be pruned first.
3. **`rigger reset --runs` must actually reclaim disk.** Today it deletes superseded rows but does not
   compact the database file, so the on-disk graph stays large and the fold stays slow even after a
   prune - the documented hygiene command does not fully deliver. It must compact (VACUUM) after
   reclaiming, so a pruned graph is both fewer rows AND a smaller, faster file.

## Design

### Dash artifacts in the setup-written gitignore (`src/main.rs`)

The setup scaffold that appends machine-local gitignore patterns (the `gitignore_added` path, ~line
6073) adds `.rigger/dash.url` and `.rigger/dash.marker` to the patterns it writes into the consumer's
`.gitignore` (idempotently, alongside the existing `.claude/` / `.rigger/shim/` entries, and reported in
the setup summary exactly as the other appended patterns are). A repo that already ignores them (or the
whole `.rigger/` runtime) gets no duplicate. This makes a fresh consumer's dash breadcrumbs
ignored-by-default, so `git add` never sweeps them and the merge-abort never happens.

### Graph-hygiene guidance in the rendered skill (`src/docs.rs`)

`discipline_body` (the shared body both the `using-rigger` skill and the handbook chapter render from)
gains a graph-hygiene section. Every fact stays code-derived (interpolated from `DocsContext` where it
names a value), pure-ASCII (hyphens, the drift check forbids unicode dashes), and self-contained (it
names only rigger's own surface). It states: the graph is re-folded each step; a large or long-unpruned
graph slows the first fold; run `rigger reset --runs` to keep it lean before a large run; this is
pre-run hygiene through a real command, NOT a hand-driven `rigger step` (which the same skill already
warns races the driver). Because both docs render from this one body, the skill and handbook cannot
disagree, and the docs-drift gate re-renders and diffs them so the guidance stays accurate.

### Compacting prune (`src/main.rs` `cmd_reset`)

`cmd_reset` (the `reset --runs` path) runs a VACUUM after the prune reclaims rows, so the file shrinks
to match. The reclaim stays deterministic and log-safe (it only drops superseded projection rows; the
event log is untouched and a rebuild re-derives the graph). The result message reports the compaction
alongside the existing reclaimed-count line.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any external tool
  or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- The fixes SHIP: the gitignore patterns go through the `rigger init`/`setup` scaffold, the guidance
  through the code-rendered skill + handbook (`rigger docs`/`setup` install them), and the compaction
  into the `rigger reset --runs` command - so a consumer receives all three from the binary, none from a
  local-only edit. The docs-drift gate stays green (the rendered docs match their committed source).
- The event log stays the source of truth; the graph is a rebuildable projection. The compacting prune
  drops only projection rows and compacts the file; it never mutates the log.
- Determinism / idempotence: the gitignore append is idempotent (no duplicate patterns); the rendered
  docs stay byte-stable for a given context; the VACUUM does not change query results, only file size.

## Done when

- [ ] a test proves the CONSUMER GITIGNORE: after `rigger init`/`setup` scaffolds a repo, the written
  `.gitignore` contains `.rigger/dash.url` and `.rigger/dash.marker` (and the append is idempotent - a
  second setup adds no duplicate). This criterion OWNS the dash-artifact ignore for consumers.
- [ ] a test proves the SKILL GUIDANCE: the rendered `using-rigger` skill (and the handbook chapter
  rendered from the same body) contain the graph-hygiene section naming `rigger reset --runs` as the
  pre-run hygiene step, and the rendered output stays byte-stable and unicode-dash-free (the existing
  render tests still pass). This criterion OWNS the shipped operator guidance.
- [ ] a test proves the COMPACTING PRUNE: `rigger reset --runs` reduces the on-disk graph file size
  after reclaiming superseded rows (a VACUUM ran), while every live edge and the event log are
  preserved. This criterion OWNS the reset compaction; it does NOT own the guidance (criterion 2).
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
