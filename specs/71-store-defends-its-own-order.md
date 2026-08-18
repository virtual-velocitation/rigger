# 71 - The store defends its own order

**Goal:** make the event store's ordering invariant self-enforcing. Recorded incident: a
compaction (which leaves revision gaps by design) ran against a live store; a writer built
before the gap-tolerant cursor appended at reissued gap revisions; every later event sorted
BELOW the run boundary in revision order; the run re-drove an answered spawn for 37 rounds.
Three defenses, one per hole: append refuses disorder, compaction refuses live writers,
validate detects the signature after the fact.

## Design

- **Append asserts monotonicity** (`src/eventstore/sqlite.rs`, inside the same transaction as
  the cursor seek at :1081 and the insert): before committing, verify the revision to be
  written is STRICTLY GREATER than the stream's last row in position order (one indexed
  seek). A violation fails loudly, naming stream, both revisions, and the likely cause (a
  stale writer after a compaction - reinstall or restart the writer). Constraints walk:
  EMPTY stream - no tail row, vacuously passes. CONCURRENT writers - seek, assertion, and
  insert share one transaction; a racing sibling serializes and re-seeks; a race surfaces as
  retry-or-refusal through the NAMED error, never a bare `UNIQUE(stream, revision)` failure.
  CRASH-RESUME / COLD START - the tail is read from the log each append, never cached. The
  correct writer never trips it (its cursor IS the max); cost is one seek in a transaction
  already taken.
- **Compaction refuses live writers** (`src/main.rs`, `rigger reset --derived`): refuse while
  the run machinery is live (held step lock, in-flight spawns in the current run's slice, or
  a fresh driver registration), naming what is live and how to stop it, with an explicit
  override flag whose help text owns the risk.
- **Validate detects the signature** (`src/main.rs::cmd_validate`, advisory like its
  siblings): a stream whose position order and revision order disagree is reported with the
  out-of-order row count and affected range, naming the repair path.

## Notes (non-criteria)

- The append assertion is the load-bearing defense; the other two are belt and braces. All
  three fail LOUDLY; none may silently repair, reorder, or drop anything.
- Repair stays a documented operator procedure (renumber-by-position, two-phase), not a
  command; validate names the procedure's doc location.
- No new event type is introduced anywhere in this spec.
- Both feature lanes audited fresh at this spec's integrated tip (criteria 1-3 already merged,
  zero code change needed by this criterion): fmt --check clean; clippy --all-targets -D
  warnings clean on default features AND --no-default-features; cargo test exits 0 on both -
  default 1919 passed, 0 failed, 2 ignored, 99 suites; --no-default-features 1802 passed, 0
  failed, 2 ignored, 99 suites.
- One pre-existing gap surfaced by review during criterion 2, out of this spec's scope to fix
  (the shared helper and its other two callers predate this spec's diff, untouched by it):
  `current_run_units` (`src/main.rs`, around line 7625) folds the current run's units via
  `ledger::project(runscope::current_run(events)).unwrap_or_default()` - a malformed body on
  any known lifecycle event anywhere in the current run's slice silently degrades the live set
  to EMPTY rather than failing closed, so a genuinely live unit can read as nobody-is-live to
  every consumer of that fold (this spec's own new compaction guard included). This is the
  third live recurrence of a bug class already rejected once elsewhere; the sibling
  `terminal_and_no_live_worker` (`src/main.rs`, around line 2268) already shows the fix shape
  in this same file - propagate the fold error (`.map_err(|e| e.to_string())?`) instead of
  swallowing it into a default - so whichever future spec closes it has a proven pattern to
  follow, not a new one to invent.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to prior
  harnesses or to projects unrelated to the mechanism.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- Fail-safe directions only: the assertion may only refuse a write; the compaction guard may
  only refuse a prune; validate may only report. No path gains repair-by-side-effect.
- The port contract suite (`src/eventstore/contract.rs`) pins the assertion so every backend
  owes the same refusal.

## Done when

- [ ] a test proves APPEND REFUSES DISORDER: an append whose revision would sort at or below
  the stream's position-order tail fails loudly naming stream, revisions, and cause; a
  correct append is untouched; and two writers racing one stream serialize through the
  transaction - the loser retries-or-refuses through the SAME named error, never a bare
  unique-constraint failure - pinned in the backend-agnostic contract suite. This criterion
  OWNS the assertion, including its concurrency face.
- [ ] a test proves COMPACTION REFUSES LIVE WRITERS: `reset --derived` under a held step lock
  or in-flight spawns refuses naming what is live; with the machinery quiet it prunes exactly
  as today; the override flag's behavior is pinned. This criterion OWNS the guard.
- [ ] a test proves VALIDATE DETECTS THE SIGNATURE: a store seeded with out-of-order rows
  draws the advisory with count and range; a clean store draws nothing; exit status unchanged.
  This criterion OWNS the detector.
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
