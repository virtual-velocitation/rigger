# 83 - A live spawn's worktree is never reclaimed under it

**Goal:** unit worktrees vanish while their spawns are still live and unreported. Observed
across three runs: spec 63's u63c3 and u63c4 implementers each lost their worktree FOUR times
(mid-build, mid `cargo test --no-fail-fast`, ~10 min apart), u81c1's adversary lost its tree
mid-review ("worktree vanished, so my code verdict is unaffected; I could not personally rerun
the suite"), and an earlier seed-ingest run's sdet lens lost its tree before the review tier
ran ("it makes every reviewer's first act a repair"). Agents cope by recreating and committing
early, so runs converge, but every recurrence burns a gate rerun or forfeits an independent
re-verification. A telling correlate: every affected in-flight spawn shows `heartbeat -` in
`rigger status` - the per-spawn liveness marker the sweep would consult is absent even while
the agent is demonstrably alive and recording progress events.

## Design

FENCE, decided: a worktree may be removed only when its unit's LATEST spawn is terminal - a
recorded result exists for that spawn id, or the liveness sweep has affirmatively classified
it hung (marker stale beyond `max_wall_clock`, the spec-10 contract). ABSENCE of a liveness
marker for an in-flight spawn is never reapable-evidence on its own: an in-flight spawn whose
marker was never created (the observed state) is treated as live until its wall-clock bound
expires, because a missing marker is indistinguishable from a worker that has not yet written
one. The sweep's classification of each candidate worktree (kept: unit in flight / removed:
spawn terminal at <result position> / removed: hung past bound) is recorded so a vanish is
attributable from the log afterward, and the log record names the evidence, not just the verb.

ROOT CAUSE OF THE MISSING HEARTBEATS, owned here: diagnose why in-flight spawns show
`heartbeat -` (the marker path under the agent-live scratch dir moved, the writer stopped, or
status reads a different location than workers write) and restore the write/read agreement so
`rigger status` shows a live heartbeat age for a working spawn again. The fence above must not
depend on this fix landing first - it treats missing markers as live - but the heartbeat
restoration is what lets the hung-classification arm actually fire when a worker truly dies.

## Global constraints

- Hyphens, never em dashes. Both feature lanes green (fmt, clippy -D warnings, test, default
  and --no-default-features); the no-os-kill and reap-before-removal audits stay green.
- No new event type unless the sweep-classification record genuinely needs one; prefer an
  existing recordable form. No new dependency.
- The operator's installed rigger binary is never replaced or modified by any unit.

## Done when

- [ ] a test proves THE FENCE: with a unit's latest spawn in flight and no liveness marker
  present, the step's worktree sweep leaves that worktree in place, while a worktree whose
  spawn has a recorded result (and one whose marker is stale past `max_wall_clock`) is
  reclaimed, and each sweep decision is attributable from the log with its evidence. This
  criterion OWNS the sweep's keep/remove classification; the heartbeat writer is criterion
  2's, NOT this one's.
- [ ] a test proves HEARTBEATS ARE VISIBLE AGAIN: a working spawn's heartbeat age renders in
  `rigger status` (write and read agree on the marker location), pinned at the real
  writer/reader seam rather than a mock of both sides. This criterion OWNS the marker
  write/read agreement; the sweep's use of it is criterion 1's, NOT this one's.
- [ ] both feature lanes green (fmt, clippy, test on default and --no-default-features).
