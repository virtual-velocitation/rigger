# 102 - A wave has a width and a config has no silent keys

**Goal:** the conductor bounds how many units run at once, and a config key rigger does not
read is an error, never silence. On 2026-09-15 `.rigger/workflow.yml` carried
`max_parallel_units: 2` under `defaults` (with a comment sizing the build caches to it) and
no code read the key: the spec-92 run had five units in flight, three per-unit build caches
of 54 GB each (162 GB), and every unit's four reviewers re-running both test lanes at once -
the baseline the day's memory overrun landed on. `rigger validate` reported nothing, because
`config::Defaults` and its siblings derive `Deserialize` without `deny_unknown_fields`
(`src/config.rs:357`) and unknown keys are dropped on load.

## Design

**THE WIDTH IS A CONFIG KEY THE CONDUCTOR ENFORCES.** `defaults.max_parallel_units`
(the key the config already carries) bounds the stages in flight across a wave: within each
batch `run_wave` (`src/conductor.rs:3785`) admits at most that many stages; a stage not
admitted is neither failed nor terminal - it waits, and starts when a slot frees in this
step's wave or a later one. `0` means unbounded and is the default, so an existing consumer's
behavior does not change until it writes the key; `rigger init` and `rigger setup` scaffold
`max_parallel_units: 2` with the sizing comment the operator's file already carries.

**SLOTS ARE READ FROM THE LOG, decided here so no unit has to.** Occupancy is the count of
units with a non-terminal in-flight spawn in the log at the start of the wave, re-derived
on every step; a process-local counter is NOT an implementation of this bound (a crashed
driver would resume with every slot free while spawns are still running).

**AN UNKNOWN KEY IS A LOAD ERROR.** Every config struct rejects unknown keys
(`deny_unknown_fields`), and the error names the dotted path of the offending key
(`defaults.max_parallel_unitz: unknown key`). `rigger validate` surfaces the same error, so
the mistake is found before a run spends a step on it.

**REVIEWER BUILD CONCURRENCY IS OUT OF SCOPE.** A spawn's own `cargo test` runs outside
`build.max_concurrent` (spec 65); bounding them is a separate spec.

## Global constraints

- Hyphens, never em dashes, in every added line.
- No new event type; no new dependency.
- Both feature lanes green (fmt, clippy, test on default and --no-default-features).

## Done when

- [ ] a test proves THE WIDTH IS ENFORCED: a workflow with `max_parallel_units: 1` and three
  ready stages runs them one at a time across successive waves, none failed and none
  terminal while waiting, with occupancy re-derived from the log so a resumed step after a
  driver crash counts the spawn still in flight, pinned at `run_wave`. This criterion OWNS
  the wave-width bound; the scaffolded default is criterion 2's, NOT this one's.
- [ ] a test proves THE SCAFFOLD WRITES THE KEY: `rigger init` writes
  `defaults.max_parallel_units: 2` with its sizing comment, and a config without the key
  loads as unbounded. This criterion OWNS the key's default and scaffold text.
- [ ] a test proves AN UNKNOWN KEY IS NAMED: loading a workflow.yml carrying
  `defaults.max_parallel_unitz` fails with an error naming that dotted path, and
  `rigger validate` on the same file fails with the same text. This criterion OWNS
  unknown-key rejection at every config level.
- [ ] both feature lanes green (fmt, clippy, test on default and --no-default-features).
