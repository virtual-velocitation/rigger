# Spec-conformance tracker

Closing every gap between `docs/architecture.md` (the reference architecture - the
spec) and the implementation. Audited 2026-06-27 by five independent reviewers
against the code. Phases are in dependency order; each lands CI-green.

## Phase 1 - Event store model (the foundation) - DONE (both backends green)
- [x] `Event` gains `stream`, `meta: BTreeMap`, `valid_from`, `revision` (§5.1, §9)
- [x] `recorded_at` is store-stamped on append, not caller-stamped (§5.1)
- [x] `Revision` type; `ExpectedRevision::Exact(Revision)` = last-revision semantics (§5.1)
- [x] SQLite: `revision` + `meta` + `valid_from` columns; `UNIQUE(stream, revision)` index (§5.1)
- [x] `read_stream(from: Revision)` not Position (§5.1)
- [x] `subscribe_stream(stream, from)` added to the trait + both adapters (§5.1)
- [x] `Error::Conflict { stream, expected, actual }` carries expected + actual revision (§5.1, §8)
- [x] `Subscription::err()` exposed (§5.1)
- [x] Namespace decorator strips the prefix from returned events (§5.1.1)
- [x] KurrentDB `open` fails fast on an unreachable server (§8)
- [x] Contract suite: revision assignment, `actual` in conflicts, meta/valid_from round-trip (§5.1)

## Phase 2 - Context graph
- [ ] `DECIDED` (agent->decision) edge from the event actor (meta) (§5.2, §7)
- [ ] `BLOCKS` (need->unit) edge (§5.2)
- [ ] `ASSIGNED_TO` (unit->agent) edge (§5.2)
- [ ] Edge `valid_from` = the event's caller-supplied valid_from (§5.2, §7)
- [ ] Supersession stamps `valid_to` on the SUPERSEDES edge too, not only GOVERNS (§7)
- [ ] `Resolve` + a populated alias table that collapses synonyms (§5.2)
- [ ] `alias_unresolved` event + node-on-miss, never silently dropped (§8)
- [ ] `GATED_BY` produced in the live run (conductor `GateVerdict` carries `artifact`) (§7)

## Phase 3 - Conductor, ledger, rails
- [ ] Resume-by-replay: `run()` folds existing state, skips integrated units (§4.2, §8)
- [ ] `Unit` gains `depends_on`, `worktree`, `branch`, `evidence`; full `Status` set (§4.2)
- [ ] Adjudicator verdict gates the stage (§3.2)
- [ ] `checkBudget` circuit-breaker wired into the run loop (§4.4)
- [ ] `flagSpecDefect` (halt + amend) (§4.4)
- [ ] `abortTask` (discard un-integrated worktrees, keep integrated) (§4.4)
- [ ] scope-creep guard: refuse a criterion-less proposed unit + emit `scope_creep` (§8)
- [ ] loop-ready gate: block when the spec has no enumerable criteria (§8)
- [ ] coverage proxy-gap guard: mechanical-gate-only criterion => not covered (§8)
- [ ] async manual-gate queue: `decide` -> pause; independent units advance (§4.3)
- [ ] autonomy promotion is proposed, not auto-applied (§4.3)
- [ ] `remediate` re-grounds between attempts (§4.4)
- [ ] mid-spawn crash routes to remediate, not abort-the-run (§8)
- [ ] `done()` = every criterion covered + every unit integrated + every gate green (§4.1, R6)
- [ ] coverage gate not silently disabled by a `produces` stage (§3.2)

## Phase 4 - Config + driver
- [ ] `isolation: none|worktree` honored per agent (§3.1, §6)
- [ ] `recurse: false` strips fan-out capability (§3.1, §6)
- [ ] `strategy: fan-out` drives fan-out (§3.2)
- [ ] `partition: by-blast-radius` + a real disjoint partitioner (§3.2, §8)
- [ ] stage `autonomy` override honored (§3.2)
- [ ] `on_pass: merge` honored (§3.2)
- [ ] `SpawnOpts` gains `isolation` + `parallel` (§6)
- [ ] bounded fan-out pool, default 4 (§6)
- [ ] gate compact summary = verdict + <=5 failing lines, not a byte-tail (§3.3)
- [ ] grounder selected by config (`defaults.grounder`), `nop` reachable (§3.2, §5.4, R4)
- [ ] `rigger_emit` sets meta/actor/valid_from (§6)

## Phase 5 - Side-car
- [ ] subscription filtered to the agent's blast-radius (§5.3)
- [ ] mid-run injection: surface peer decisions at the next tool boundary (§5.3)

## Phase 6 - CLI + composition
- [ ] `rigger run --driver <cli|workflow>` flag (§10)
- [ ] `rigger run --eventstore <sqlite|kurrentdb>` flag, KurrentDB wired (§10)
- [ ] namespace decorator wired (default to project identity) (§5.1.1, R9)
- [ ] living-DAG / `spawnUnit`: a `produces` stage extends the run DAG (§3.2, §8)
- [ ] `rigger init` scaffold shows the full DAG shape (§3.2)
- [ ] `examples/golden-apple/` worked example (§10, §11)

## Phase 7 - Docs
- [ ] Re-sweep `docs/architecture.md` UP to the implemented spec (not down to old code)
