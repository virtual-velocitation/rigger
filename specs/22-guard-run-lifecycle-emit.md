# 22 - `rigger emit` must not mint run-lifecycle boundary events

**Goal:** a stray `rigger emit RunStarted` (from a verification agent, a scratch dir,
anywhere) must never be able to inject a run-lifecycle boundary event into the live run
stream and hijack the conductor's run. Restrict the emit surface to the context events
agents legitimately produce; refuse everything else. This closes a real store-corruption
footgun: an agent verifying the reset-runs unit ran `rigger emit RunStarted` from a
scratch worktree, the store-binding correctly walked up to the repo's real store (the
sanctioned courier behavior), and the stray `RunStarted` became the latest boundary - so
the next `rigger step` saw mismatched criteria, called `start_fresh`, and would have
orphaned the whole run.

## Design

The defect is NOT the store walk-up (bounded at the main-repo root; cross-project walk-up
is already closed - see `walk_stores_from` / `main_repo_root` in `src/main.rs`). The defect
is that the emit surface accepts an ARBITRARY event type.

`emit_event` (`src/mcpserver.rs`) is the SHARED CORE of both `rigger emit` (the CLI,
`cmd_emit` in `src/main.rs`) and `rigger_emit` (the MCP tool, `tool_emit`). It reads the
`type` field with no allowlist and appends it to the run stream, so `rigger emit
RunStarted '{...}'` lands a run boundary in the live store. Run boundaries (`RunStarted`,
`TYPE_RUN_STARTED` in `src/run.rs`) are minted ONLY by the conductor's run lifecycle
(`start_fresh` / `ensure_started`) - a path that does NOT go through `emit_event` and is
therefore unaffected by this change.

**Unit 1 - allowlist the emit surface (touches `src/mcpserver.rs`).** In the shared
`emit_event` core, ALLOWLIST exactly the agent-emittable context event types - the
context-graph `TYPE_*` set: `DecisionMade`, `ReviewFinding`, `LessonLearned` (referenced
from the `src/contextgraph` constants so the list stays in sync, never hand-copied). REFUSE
any other type - especially any run-lifecycle / orchestration event (`RunStarted`, and
peers like `RunEnded`/`SpawnRequested`/`SpawnResult`/`UnitEscalated`) - with a clear,
actionable error naming the offending type and directing the caller to the right tool:
that run boundaries are minted by the conductor and a new run is started with `rigger run
--fresh`, not `rigger emit`. Because the guard lives in the shared core, BOTH the CLI
(`rigger emit`) and the MCP tool (`rigger_emit`) inherit it. An allowlist (not a denylist)
is deliberate: a future conductor-owned event type is refused by default, never silently
injectable.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any
  external tool or project in code, comments, or commit messages.
- The verification MUST be a Rust unit test that calls `emit_event` DIRECTLY over a
  temp/in-memory `Store` - NEVER a CLI `rigger emit` run from a walk-up-able cwd. Testing
  the guard must not be able to reproduce the store corruption this spec fixes; a unit
  test over an isolated store never touches the real store and never walks up. Flag this
  to the adjudicator: reject any test that shells out to `rigger emit` against the repo.
- The conductor's own `RunStarted` minting (`src/run.rs` `start_fresh`/`ensure_started`)
  is UNCHANGED and must stay green - it does not go through `emit_event`.
- The three context emits agents already make (`DecisionMade`, `ReviewFinding`,
  `LessonLearned`) keep working byte-for-byte.
- BOTH feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D
  warnings`; `cargo test` - on default features AND `--no-default-features`.

## Done when

- [ ] a Rust unit test over a temp `Store` proves `emit_event` REFUSES a `RunStarted` (and at least one other conductor-owned type) with an actionable error that names the type and points to `rigger run --fresh`, and appends NOTHING to the store
- [ ] a Rust unit test over a temp `Store` proves `emit_event` still accepts `DecisionMade`, `ReviewFinding`, and `LessonLearned` and appends them, so both the CLI (`rigger emit`) and MCP (`rigger_emit`) paths that share this core keep working
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`)
