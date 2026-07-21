# 23 - No process outlives the worktree/scratch dir it runs in

**Goal:** rigger owns the lifecycle of the per-unit worktrees and agent-scratch dirs it
creates under `<repo>/.rigger/tmp/`, but tears them down by removing the DIR only - it never
reaps a process whose working directory is INSIDE that dir. Such a process (a build the agent
left running, an LSP or tool the harness spawned for the agent inside the worktree, a stray
server) then outlives its dir: it holds a now-deleted cwd and leaks memory until the machine
is under pressure. Close it: no process rooted in a dir rigger removes may survive the
removal, and a leaked process rooted under the scratch root is surfaced. This extends the
no-orphaned-processes guarantee of spec 19b (which covered rigger's OWN children - `rigger
serve`, the peers sidecar, the dash) to cover ANY process left rooted in a dir rigger owns,
regardless of who spawned it.

The lifecycle rule is 1:1 and deterministic, NOT periodic: whatever creates the dir owns
every process rooted in it, and reaps them as part of THAT dir's teardown (when the unit /
scratch that created them is complete) - never a background sweep that hopes to catch them
later. The teardown of the creating context IS the reap point.

## Design

Builds on the worktree teardown (`Worktree::remove`, `src/worktree.rs:472`) and the per-step
scratch sweep (`cmd_step` / Gap 14, `src/main.rs:1306`), and the `rigger validate` residue
scan (`residue_advisories`, `src/main.rs:4356`) that already surfaces leftover worktrees and
caches as warning-only advisories.

**Unit 1 - reap processes rooted in a removed dir (touches `src/worktree.rs`,
`src/main.rs`).** Before rigger removes a worktree or scratch dir it owns, it finds every
process whose resolved cwd is INSIDE that dir and reaps it - SIGTERM, then SIGKILL after a
short grace - so the dir can be removed cleanly and nothing outlives it. Detection is
best-effort and Linux-first via `/proc/<pid>/cwd`; on a platform without `/proc` it is a
graceful no-op, NEVER a hard failure (rigger is a published cross-platform crate). The reap is
scoped STRICTLY to processes whose cwd resolves inside the exact dir being removed - see the
safety boundary in the constraints.

**Unit 2 - surface a leaked-process advisory (touches `src/main.rs`).** `rigger validate`'s
residue scan additionally reports, as a warning-only advisory (like the leftover-worktree and
orphaned-cache advisories, never a hard failure), any process whose cwd resolves under
`<repo>/.rigger/tmp/` - naming the pid and command - so a leak is visible even when no
teardown is running.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any
  external tool or project in code, comments, or commit messages.
- SAFETY BOUNDARY (load-bearing): the reap kills ONLY processes whose cwd resolves strictly
  INSIDE the dir being removed, which is always under `<repo>/.rigger/tmp/`. rigger must NEVER
  kill a process rooted at the repo root, in another project, or anywhere outside its own
  scratch - an editor LSP or a user process rooted at the repo root is off-limits. A test must
  prove the boundary holds (an outside-rooted process is never touched).
- Best-effort and platform-tolerant: `/proc`-based on Linux; a graceful no-op where `/proc` is
  absent; never a hard error, so teardown and validate keep working on any platform.
- Warnings only for `validate` (Unit 2); the teardown reap (Unit 1) is the only kill, and only
  within rigger's own removed dir.
- BOTH feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.

## Done when

- [ ] a test spawns a child process whose cwd is INSIDE a scratch dir under `.rigger/tmp`, has rigger tear that dir down, and proves the child is no longer alive (SIGTERM then SIGKILL after a grace); a second child rooted OUTSIDE `.rigger/tmp` (at the repo root) is proven STILL alive - the safety boundary holds
- [ ] a test proves `rigger validate` emits a warning-only advisory naming a process whose cwd is under `<repo>/.rigger/tmp/`, and emits none when no such process exists; on a platform without `/proc` both the reap and the scan are a graceful no-op, never an error
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`)
