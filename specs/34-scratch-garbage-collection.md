# 34 - rigger reclaims its own scratch: core-enforced garbage collection

**Goal:** make "clean up after yourself" an INVARIANT of the rigger core, not a behavior asked of
agents or drivers. rigger creates all of its transient development scratch under `.rigger/tmp`
(worktrees, per-spawn build/verify cargo targets, review dirs, agent scratch); today none of it is
reliably reclaimed on a non-clean exit, so it accumulates without bound (observed: hundreds of GB of
orphaned per-agent `cargo-target-*`). The fix belongs in the Rust conductor, enforced by OWNERSHIP:
because rigger owns `.rigger/tmp`, rigger reclaims it - automatically, with no operator configuration,
no special agent, and no dependence on any driver or subagent cooperating. A real operator, or any
driver (native workflow, CLI, turnkey), inherits the guarantee for free.

## Design

The enforcement is ownership-based, not cooperation-based: rigger tracks which scratch belongs to a
LIVE spawn or run, and anything under `.rigger/tmp` that is NOT live-owned is reclaimable at any time.
That single rule makes the guarantee robust to every failure mode - a killed process, a wedged run, a
non-cooperative agent that wrote a target to an ad-hoc path - because none of those produce live-owned
scratch.

- **Per-spawn scratch is rigger-assigned and rigger-reclaimed.** rigger gives each spawn a dedicated
  scratch path under `.rigger/tmp` for its build/verify output (so a verify-by-running spawn's cargo
  target lands in a rigger-owned per-spawn location, not an ad-hoc top-level `cargo-target-*`), and
  DELETES that path the moment the spawn's result is recorded - for ANY outcome (success, a REJECT
  verdict, an --error, or a liveness/infra fault). This is the "reclaim the instant the agent is done"
  behavior, done by rigger, not the agent.
- **Orphan-sweep is the ownership backstop.** On run start (and idempotently on each `rigger step`),
  rigger reclaims any entry under `.rigger/tmp` that is not owned by a currently-live spawn or run -
  including a prior run's killed-process leftovers and any scratch an agent wrote outside its assigned
  path. This is what makes the guarantee not rest on agent goodwill: uncontrolled scratch cannot
  survive into the next run.
- **Run teardown reclaims run-level scratch for EVERY terminal state.** rigger reclaims a run's
  worktrees (per unit) and shared build cache when the run reaches ANY terminal state - clean fixpoint,
  wedge/escalation, definition-drift halt, budget halt - not only on clean integration (the gap that
  let wedged/halted runs leak). A unit's worktree is reclaimed when the unit reaches its own terminal
  state (integrated or terminal), never while later stages of the same unit still need it.
- **Never delete live-owned scratch.** The reclaimer keys off liveness (a spawn with no recorded
  result yet; a run still advancing), so it can never remove a worktree an in-flight reviewer is
  reading or a target a running build is writing.

`.rigger/tmp` is transient by definition; nothing durable is ever stored there, so reclamation is
always safe. This is code only (`src/conductor.rs` and the scratch/worktree lifecycle it owns); it
changes no agent persona, no `workflow.yml`, and no `.rigger/agents/` - so it introduces no definition
drift and needs no operator action.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any external tool
  or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- Determinism/safety by construction: reclamation is keyed on liveness ownership; it MUST NOT delete
  scratch owned by a live spawn or run (proven by a test that keeps an in-flight spawn's scratch).
- Core-enforced, zero-config: the guarantee lives in the Rust conductor and requires NO agent
  cooperation, NO driver change, and NO operator setup. It works for every driver.
- No new event type; no change to `.rigger/agents/` or `workflow.yml` (code only).
- Platform-tolerant: on a platform where a scratch path is already gone, reclamation is a graceful
  no-op, never an error that fails a run.

## Done when

- [ ] a test proves a spawn's rigger-assigned scratch is DELETED the moment its result is recorded,
  for every outcome (a success, a reject verdict, an `--error`, and a liveness/infra fault each leave
  the spawn's scratch gone) - and that a spawn with NO recorded result yet keeps its scratch. This
  criterion OWNS per-spawn reclamation on completion.
- [ ] a test proves the ORPHAN-SWEEP reclaims non-live-owned scratch: an entry under `.rigger/tmp`
  with no live owner (a prior run's leftover, or scratch written outside a spawn's assigned path) is
  gone after a run starts, while a live spawn's/run's scratch is untouched. This criterion OWNS the
  ownership backstop; it does NOT own per-spawn reclamation (the criterion above).
- [ ] a test proves RUN TEARDOWN reclaims run-level scratch for ANY terminal state: a run that ends by
  wedge/escalation/definition-drift/budget-halt - not just a clean fixpoint - leaves no worktrees or
  build cache behind, and a unit's worktree is reclaimed only once the unit is terminal (never while a
  later stage still needs it). This criterion OWNS terminal-state run/unit reclamation; it does NOT own
  the per-spawn or orphan-sweep behavior (the criteria above).
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
