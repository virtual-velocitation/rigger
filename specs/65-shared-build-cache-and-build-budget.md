# 65 - Shared compilation cache and a machine-wide build budget

**Goal:** stop concurrent unit builds from overrunning memory, and stop N worktrees compiling
the same dependency graph N times. A consuming project measured both: 10+ concurrent builds
(per-unit worktrees, each with its private target dir per the Gap 19 clobber fix) drove the
machine into memory exhaustion, and the fix that worked there - a rustc compilation-cache
wrapper with a shared cache, plus bounded build concurrency - was hand-instituted per project.
Per the ship-to-consumers rule, rigger institutes it directly: config-driven, detected rather
than bundled, applied uniformly to every build the loop runs.

## Design

- **One build-environment authority** (`src/gate.rs` env seam + the agent spawn env): a single
  resolver derives the build environment from config and applies it to EVERY build the loop
  executes - inline/deferred gates AND agent-run builds (the courier's spawn env) - so a gate
  build and an agent's `cargo test` hit the same cache under the same budget. The existing
  per-unit `CARGO_TARGET_DIR` isolation (Gap 19) is unchanged; the cache layer sits under it.
- **Compilation-cache wrapper** (`workflow.yml` `build.wrapper`): the mechanism is cargo's own
  `RUSTC_WRAPPER` - rigger hardcodes no tool. `build.wrapper: auto` (the setup default) probes
  for a known wrapper binary on PATH and uses it when present; a named wrapper is REQUIRED
  (absent on PATH = loud config error at run start - no silent degrade); `off` suppresses.
  When a wrapper is active the resolver also sets the shared cache location
  (`build.cache_dir`, default `<state home>/rigger/build-cache` so every project and worktree
  on the machine shares one cache) and `CARGO_INCREMENTAL=0` (incremental output defeats
  wrapper caching; the warm per-unit target dirs from spec 64 carry the incremental win
  instead).
- **Machine-wide build budget** (`build.max_concurrent`, default bounded): a flock-based slot
  directory (the machine-wide-flock precedent from the accelerator construct lock) caps how
  many builds run concurrently ACROSS every rigger process on the machine; the N+1th build
  waits for a slot rather than stacking another compiler fleet into memory. Slots are held
  for the build's duration and auto-release on crash (flock semantics). `build.jobs`
  optionally caps each build's internal parallelism (`CARGO_BUILD_JOBS`), so slots x jobs can
  be sized to the machine.
- **Honest surfaces**: `rigger validate` reports the RESOLVED build environment - wrapper
  (which binary, or "none found" under auto, or the hard error for a named-but-absent one),
  cache dir, slot budget - so an operator sees at a glance whether the cache is actually
  live. `rigger setup` writes the `build:` section with `wrapper: auto` for new projects and
  never clobbers an existing one.

## Notes (non-criteria)

- The known-wrapper probe list is config-extensible (`build.wrapper` accepts any binary
  name); `auto` exists so the pit-of-success default helps a machine that already has a
  wrapper installed without demanding config.
- Slots bound BUILDS, not agents: reviewers reading code or querying the graph are never
  budget-gated; only compiler invocations are.
- Spec 64 (worktrees and their caches survive parks) composes: warm per-unit dirs cut
  repeat-attempt cost; the shared wrapper cache cuts cross-unit and cold-start cost; the
  budget bounds peak memory regardless.
- No new event type is introduced anywhere in this spec.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to prior
  harnesses or to projects unrelated to the mechanism; the wrapper integration is via cargo's
  generic `RUSTC_WRAPPER`/env contract and names no specific tool in code.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- No silent degrade in either direction: a required wrapper that is absent errors loudly; an
  auto probe that finds nothing reports "none" and injects nothing.
- Gap 19 isolation holds: per-unit target dirs are never shared, budget or not.

## Done when

- [ ] a test proves the ONE AUTHORITY: with a wrapper configured, both a gate build and an
  agent spawn's environment carry the wrapper, the shared cache dir, and incremental-off; with
  `off`, neither does. This criterion OWNS the env resolver and both injection sites.
- [ ] a test proves NO SILENT DEGRADE: a named wrapper absent from PATH fails the run start
  with an error naming the binary and the config key; `auto` with nothing on PATH injects
  nothing and reports "none" through validate.
- [ ] a test proves the BUILD BUDGET: with `max_concurrent: N`, the N+1th concurrent build
  blocks until a slot frees, the slot releases when its holder exits (including abnormally),
  and non-build agent work is never gated.
- [ ] a test proves JOBS CAP: `build.jobs` reaches the build as its internal parallelism cap;
  unset leaves the ambient default untouched.
- [ ] a test proves the SURFACES: `rigger validate` reports resolved wrapper, cache dir, and
  budget; `rigger setup` writes the default `build:` section only when absent.
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
