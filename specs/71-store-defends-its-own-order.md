# 71 - The store defends its own order

**Goal:** make the event store's ordering invariant self-enforcing, so the corruption class a
live incident just demonstrated can never again proceed silently. The recorded chain: a
compaction (which leaves revision gaps by design) ran against a live store; a WRITER BUILT
BEFORE the gap-tolerant cursor then appended at reissued gap revisions (count-derived cursor);
every subsequent event sorted BELOW the run boundary in the conductor's revision-ordered read;
and the run re-drove an already-answered spawn for 37 rounds because its results were
invisible. Three defenses, one per hole: the append path refuses to write disorder, the
compaction refuses to run under live writers, and validate detects the signature in an
already-damaged store.

## Design

- **Append asserts monotonicity** (`src/eventstore/sqlite.rs`, the append seam): before
  committing, the append verifies the revision it is about to write is STRICTLY GREATER than
  the revision of the stream's LAST ROW IN POSITION ORDER (one indexed seek). A violation
  fails the append loudly, naming the stream, both revisions, and the likely cause (a stale
  writer after a compaction - reinstall or restart the writer). This converts silent
  disordering into an immediate, attributable error at the FIRST bad write - the correct
  writer never trips it (its cursor IS the max), so the assertion costs one seek and fires
  only when the invariant is actually being broken.
- **Compaction refuses live writers** (`src/main.rs`, `rigger reset --derived`): the prune
  refuses to run while the run machinery is live - a held step lock, in-flight spawns in the
  current run's slice, or a fresh driver registration - naming what is live and instructing
  the operator to stop it first (or pass an explicit override flag whose help text owns the
  risk). Compaction is operator maintenance; mid-run mutation of the log is how the incident
  started.
- **Validate detects the signature** (`src/main.rs::cmd_validate`, advisory like its
  siblings): a stream whose position order and revision order DISAGREE is reported with the
  count of out-of-order rows and the affected range, naming the repair path. This is the
  after-the-fact detector for a store damaged before this spec landed, and the regression
  alarm if any future writer evades the append assertion.

## Notes (non-criteria)

- The append assertion is the load-bearing defense; the other two are belt and braces. All
  three fail LOUDLY in their own name - none may silently repair, reorder, or drop anything.
- Repair itself stays a documented operator procedure (renumber-by-position, two-phase), not
  a command, until a second incident justifies automating it; validate names the procedure's
  doc location.
- No new event type is introduced anywhere in this spec.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to prior
  harnesses or to projects unrelated to the mechanism.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- Fail-safe directions only: the assertion may only ever REFUSE a write; the compaction guard
  may only ever refuse a prune; validate may only ever report. No path gains repair-by-side-effect.
- The port contract suite (`src/eventstore/contract.rs`) pins the assertion so every backend
  owes the same refusal.

## Done when

- [ ] a test proves APPEND REFUSES DISORDER: an append whose revision would sort at or below
  the stream's position-order tail fails loudly naming stream, revisions, and cause, and a
  correct append (revision from the true tail) is untouched - pinned in the backend-agnostic
  contract suite. This criterion OWNS the assertion.
- [ ] a test proves COMPACTION REFUSES LIVE WRITERS: `reset --derived` under a held step lock
  or in-flight spawns refuses naming what is live; with the machinery quiet it prunes exactly
  as today; the override flag's behavior is pinned. This criterion OWNS the guard.
- [ ] a test proves VALIDATE DETECTS THE SIGNATURE: a store seeded with out-of-order rows
  draws the advisory with count and range; a clean store draws nothing; exit status unchanged.
  This criterion OWNS the detector.
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
