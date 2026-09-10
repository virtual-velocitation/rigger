# 87 - Dead code counts production references only: the audit's section 4, redone

**Goal:** spec 85's section 4 ("Dead and Vestigial Code") reported zero removable functions. Its
instrument counted an identifier's remaining WHOLE-TREE occurrences - `tests/` and every
`#[cfg(test)]` body included - so a function whose only caller is its own test reads as alive:
the false negative the operator predicted on 2026-09-08. It also scanned only the 596
production functions of the three god files, and counted name occurrences (doc mentions,
strings) rather than references from code. A corrected quick pass over all 1,484 production
functions in `src/`, counting references from production code only, found 89 with zero
production references, every one of them referenced from tests alone (e.g.
`src/grounder/symbols/events.rs:17 index_events`, 21 test references; the `SpawnRequest`
builders `with_title`/`with_reviews`/`with_model`/`with_blast_radius` in `src/spawn.rs`;
`src/worktree.rs` `expect_merged` and `is_dirty`). That pass is itself approximate in both
directions: its `#[cfg(test)]` stripping over-counts `src/dash.rs`, and it misclassified
`src/blast_radius_eval.rs` and `src/eventstore/contract.rs` as production when both are
OUT-OF-LINE test modules (`src/lib.rs` declares `#[cfg(test)] mod blast_radius_eval;`,
`src/eventstore/mod.rs` declares `#[cfg(test)] mod contract;`) - the module file carries no
attribute of its own, so a per-file scan cannot see it. That is why the precise instrument
belongs in the audit's own generator. Operator rule: a dead-code sweep counts
PRODUCTION references only, over the whole crate, definitions excluded; a function referenced
only from tests is dead, and its tests are dead with it.

## Design

TWO STAGES, decided (operator direction 2026-09-08: "cargo minify and then the dead_code lint
should reduce the noise and confusion"). STAGE 1, compiler-driven, runs first and lands first:
`cargo minify` (tweedegolf's tool, operator-installed: `cargo install cargo-minify`; CI gets it
via `taiki-e/install-action` when its manifest carries it, else a cached `cargo install`)
applied in the unit's worktree on both feature
lanes, followed by a build on both lanes with `RUSTFLAGS="-D dead_code -D unused_imports
-D unused_variables -D unreachable_pub"` - whatever the compiler proves unreachable is removed
in this stage, with the removed items listed in the report (name, file:line) as
`delete (compiler)`. Stage 1's known blind spot is the reason stage 2 exists: in a crate that
is both a library and a binary, the `dead_code` lint never fires on a `pub` item, so every
`pub fn` with zero real callers survives stage 1 looking clean. STAGE 2 is the reference sweep
below, run on the tree stage 1 leaves behind. ORDER IS DECLARED, not coincidental: the plan
chains the units with explicit `needs` edges - criterion 2's unit needs criterion 1's, criterion
3's needs criterion 2's - so the sweep never runs before the compiler pass has integrated and
the dispositions never run before the JSON exists.

INSTRUMENT (stage 2), decided: extend `tests/simplification_audit.rs`'s scanner (which already classifies
every fn frame as `is_test` via enclosing `#[cfg(test)]` mods and `#[test]` attributes) to
cover the whole `src/` tree, with `is_test` made FILE-AWARE: a file declared by a parent's
`#[cfg(test)] mod name;` (resolving `name.rs`, `name/mod.rs`, or a `#[path = ".."]` target) is
test code in full, and so is everything nested under it; `tests/` files likewise. The same rule
governs spec 86's graph exclusion (op-u86c1-r5-close-every-remaining-test-shape), so the two
instruments agree on what a test is. Then add a reference pass that, for each production (`is_test:
false`) fn, counts references to its name from PRODUCTION code only: every `src/` file with its
`is_test` spans removed, and no `tests/` file at all. A reference is the identifier used as
code - followed by `(`, `::`, `.`, `<`, or used as a path segment - not a bare word inside a
comment or string literal. The fn's own definition span (doc comment, attributes, signature) is
excluded. Methods are resolved by name plus receiver-agnostic call shape (`.name(`); a name
shared by several fns is reported as ambiguous rather than counted as alive.

OUTPUT, decided: `docs/audit/dead-code.json` - one entry per production fn with zero
production references: name, file:line, visibility, the test-only references that kept it
looking alive (file:line each), and a DISPOSITION. The generator writes it under
`RIGGER_AUDIT_WRITE=1` and asserts the committed file matches otherwise (the same drift-guard
shape as the catalog). Section 4 of the report is REWRITTEN from this file: the count, the
per-file distribution, the full list, and for each entry its disposition.

DISPOSITIONS, decided, exactly three: `delete` (the fn and the tests that reference only it),
`keep-public-surface` (a `pub` item that is part of the library's intended external surface -
must cite the consumer: the MCP server, the workflow template, a documented CLI contract - a
consumer that does not exist is not a reason), or `keep-pending` (referenced only by a test
that PROVES a contract the product is expected to gain - must cite the spec that will call it).
Every entry gets one; an entry without a cited reason is a defect. The audit's section 6 gains
a Tier 1 item 0: "Delete the dead-code set" with the deletion list, expected line delta
(negative: the fns plus their orphaned tests) and risk, so the wave carries it.

THE KNOWLEDGE GRAPH CROSS-CHECK, decided: for each candidate, `rigger graph --show <entity>`
degree is reported beside the reference count; after spec 86 lands, a candidate's degree
counts product edges only and must agree (zero); before 86 the report notes the test-edge
inflation rather than trusting the degree.

CONSTRAINTS WALK: a fn called only through a trait object (dynamic dispatch) - the reference
pass counts trait method NAMES at call sites, so an impl's method referenced via `.name(` on any
receiver is alive; an unreferenced trait method is reported with `keep-public-surface` or
`delete` per its trait's use. A fn referenced only by a macro expansion - macros are scanned
as text; a name inside a macro body is a reference. A `#[cfg(feature)]`-gated fn - counted
within its lane; both lanes are scanned and the union is the reference set. A fn used only in
`build.rs` - `build.rs` and `build/` are production references. An entry point (`main`,
`#[no_mangle]`, clap handlers reached by attribute) - the scanner exempts `fn main` and any fn
named by a `#[...]` attribute that registers it, listed explicitly in the report.

## Notes (non-criteria)

Stage 1 is the only production change in this spec, and it is deletion only; the reference
sweep's deletions happen in the wave as section 6's new item 0. Spec 86 is integrated, so the
graph cross-check uses product-only degrees.

## Global constraints

- Hyphens, never em dashes. Both feature lanes green (fmt, clippy -D warnings, test, default
  and --no-default-features); no-os-kill and reap audits green.
- Production code changes ONLY in stage 1 and ONLY as deletions the compiler proves (what
  `cargo minify` removes and what the promoted lints name); stages 2 and 3 change no production
  code - the reference sweep's deletions happen in the wave as section 6 item 0. No new
  dependency; no new event type.
- The operator's installed rigger binary is never replaced or modified by any unit.

## Done when

- [ ] a test proves THE COMPILER PASS LANDED FIRST: `cargo minify` has been applied on both
  feature lanes and a build on both lanes with `dead_code`, `unused_imports`,
  `unused_variables` and `unreachable_pub` promoted to errors is clean, with every item the
  compiler removed listed in the report as `delete (compiler)` with file:line, and the
  no-os-kill and reap audits still green on the reduced tree. This criterion OWNS stage 1; the
  reference sweep is criterion 2's, NOT this one's.
- [ ] a test proves THE SWEEP COUNTS PRODUCTION ONLY: the generator, over the whole `src/`
  tree, classifies every fn as production or test, counts references to each production fn from
  production code only (test spans and `tests/` excluded, definition span excluded, code-shaped
  references only), writes `docs/audit/dead-code.json` and asserts the committed file matches;
  a fixture with a fn referenced only by its own test lists that fn, and a fn referenced from a
  production caller does not appear. This criterion OWNS the stage-2 instrument and the JSON; the
  report and dispositions are criterion 3's, NOT this one's.
- [ ] a test proves EVERY CANDIDATE IS DISPOSITIONED: section 4 of the report is regenerated
  from the JSON with the count, per-file distribution and full list, every entry carries exactly
  one of `delete` / `keep-public-surface` / `keep-pending` with its cited reason, the knowledge
  graph degree is reported beside each, and section 6 gains item 0 "Delete the dead-code set"
  with the deletion list and its delta. This criterion OWNS section 4's text, the dispositions
  and the section 6 item; the instruments are criteria 1 and 2's, NOT this one's.
- [ ] both feature lanes green (fmt, clippy, test on default and --no-default-features).
