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
`src/worktree.rs` `expect_merged` and `is_dirty`; most of `src/blast_radius_eval.rs`). That pass
is itself approximate (its `#[cfg(test)]` stripping over-counts `src/dash.rs`), which is why the
precise instrument belongs in the audit's own generator. Operator rule: a dead-code sweep counts
PRODUCTION references only, over the whole crate, definitions excluded; a function referenced
only from tests is dead, and its tests are dead with it.

## Design

INSTRUMENT, decided: extend `tests/simplification_audit.rs`'s scanner (which already classifies
every fn frame as `is_test` via enclosing `#[cfg(test)]` mods and `#[test]` attributes) to
cover the whole `src/` tree, and add a reference pass that, for each production (`is_test:
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

This spec changes no production code; deletions happen in the wave as section 6's new item 0.
It lands after spec 86 only if 86 is already integrated when it runs; otherwise the graph
cross-check follows the "before 86" rule above.

## Global constraints

- Hyphens, never em dashes. Both feature lanes green (fmt, clippy -D warnings, test, default
  and --no-default-features); no-os-kill and reap audits green.
- No production code changes; no new dependency; no new event type.
- The operator's installed rigger binary is never replaced or modified by any unit.

## Done when

- [ ] a test proves THE SWEEP COUNTS PRODUCTION ONLY: the generator, over the whole `src/`
  tree, classifies every fn as production or test, counts references to each production fn from
  production code only (test spans and `tests/` excluded, definition span excluded, code-shaped
  references only), writes `docs/audit/dead-code.json` and asserts the committed file matches;
  a fixture with a fn referenced only by its own test lists that fn, and a fn referenced from a
  production caller does not appear. This criterion OWNS the instrument and the JSON; the
  report and dispositions are criterion 2's, NOT this one's.
- [ ] a test proves EVERY CANDIDATE IS DISPOSITIONED: section 4 of the report is regenerated
  from the JSON with the count, per-file distribution and full list, every entry carries exactly
  one of `delete` / `keep-public-surface` / `keep-pending` with its cited reason, the knowledge
  graph degree is reported beside each, and section 6 gains item 0 "Delete the dead-code set"
  with the deletion list and its delta. This criterion OWNS section 4's text, the dispositions
  and the section 6 item; the instrument is criterion 1's, NOT this one's.
- [ ] both feature lanes green (fmt, clippy, test on default and --no-default-features).
