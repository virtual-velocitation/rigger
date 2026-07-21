# 19c - A wedge or a hang always surfaces as a loud error

**Goal:** rigger surfaces some wedges loudly (budget halt, step failure) but not all: a unit
that exhausts remediation escalates and the run still reaches a clean `done` fixpoint (so the
driver reports success with the wedge only in an event), and a hung agent with no wall-clock
bound hangs indefinitely with no error. Close both. Third of three specs split from
Workstream B of `docs/architecture-addendum-pit-of-success.md`.

## Design

Builds on the native driver's done/halt handling (`workflows/rigger.js`), the conductor's
step/done result and escalation (`UnitEscalated`, `src/conductor.rs`), the per-spawn
wall-clock (`max_wall_clock` in `src/config.rs`), and `rigger validate` (`cmd_validate` in
`src/main.rs`).

**Unit 1 - a wedged run surfaces as a loud error (touches `src/conductor.rs`,
`workflows/rigger.js`).** A unit that exhausts remediation ESCALATES: it goes terminal, the
run continues and reaches a clean `done` fixpoint, so the driver logs "run complete" and
resolves SUCCESSFULLY with the wedge recorded only as a `UnitEscalated` event - a wedged
terminus is indistinguishable from a clean one unless you inspect escalations. Make the
terminal signal honest: the conductor's step/done result carries the set of
escalated/unintegrated units, and the driver treats a fixpoint reached with ANY such unit as
a LOUD failure (throws, non-zero, names them), exactly as it already does for a `halted`
budget stop. Escalation-and-continue mid-run stays correct; only the FINAL terminus must not
masquerade as success.

**Unit 2 - the native driver bounds a hung spawn (touches `workflows/rigger.js`).** The
default `defaults.max_wall_clock` is `0` = unbounded ("no spawn is ever timed out"), so a
config that does not set it leaves a hung spawn in-flight indefinitely - a silent hang the
liveness sweep never reaches. Without regressing the back-compatible unbounded default for a
spawn that legitimately runs long, the native driver enforces an OUTER per-agent wall-clock
so even an unbounded-config spawn is abandoned-and-surfaced after a bound rather than awaited
forever.

**Unit 3 - `rigger validate` warns on an unbounded default (touches `src/main.rs`,
`src/config.rs`).** `rigger validate` warns when `defaults.max_wall_clock` is unbounded and
no per-agent bound covers the gating roles ("a hung agent will never be swept; set
defaults.max_wall_clock"), so the risk is visible at author time.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any
  external tool or project in code, comments, or commit messages.
- The escalate-and-continue behavior mid-run is UNCHANGED; only the final terminal signal
  and the driver's exit change.
- Determinism by construction for anything serialized (`BTreeMap`/`BTreeSet`/sorted `Vec`).
- BOTH feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D
  warnings`; `cargo test` - on default features AND `--no-default-features`.

## Done when

- [ ] a test proves a run that reaches a fixpoint with an escalated/unintegrated unit surfaces as a LOUD driver error (non-zero, naming the escalated units), distinct from a clean convergence which succeeds - a wedge never masquerades as "run complete"
- [ ] a test proves the native driver abandons-and-surfaces an unbounded-config spawn after an outer per-agent wall-clock, rather than awaiting it forever
- [ ] a fixture proves `rigger validate` warns when `defaults.max_wall_clock` is unbounded and no per-agent bound covers the gating roles
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`)
