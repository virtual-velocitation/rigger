# 85 - Simplification audit: the report that decides the refactors

**Goal:** rigger has grown by accretion. `src/` is 128,533 lines in 56 files, three of which are
programs rather than modules - `src/conductor.rs` 34,677 lines, `src/main.rs` 24,024,
`src/dash.rs` 11,120 (plus `src/contextgraph/sqlite.rs` 7,320) - holding most of the 3,367
functions. `tests/` is 97,633 lines in 153 files, roughly one periphery file per spec criterion,
with `tests/cli.rs` alone at 26,930 lines and 2,804 `#[test]`s overall. There are 76 separate
`Command::new` sites. The loop adds and reviews per unit and never consolidates, so duplication
accumulates without any single bad commit; the review panel has already caught instances left
standing (`src/dash.rs` reimplementing `src/reap.rs`'s `/proc` pid scan, upheld at spec 62's
capstone). Operator rule (2026-09-06): DRY is STRICT - any logic present in more than one place
anywhere in the codebase is a violation, with no "small enough to duplicate" exemption. This
spec produces the audit REPORT that the refactoring specs will be derived from. It changes no
production code.

## Design

DELIVERABLES, decided: (1) `docs/audit/2026-09-simplification-audit.md`, the human report, with
the six fixed sections below; (2) `docs/audit/duplication-catalog.json` and
`docs/audit/responsibility-map.json`, machine-readable so follow-up specs can pin counts and
prove reductions; (3) `tests/simplification_audit.rs`, the deterministic generator and drift
guard for (2): run with `RIGGER_AUDIT_WRITE=1` it rewrites the JSON files; run plainly it
regenerates in memory and asserts the committed files match byte-for-byte, so the catalog can
never silently drift from the tree. Zero new dependencies: function extraction is a
brace-matching scanner over `.rs` files (fn item start to its closing brace), and similarity is
normalized-token shingling (identifiers canonicalized to their kind, literals to a placeholder,
whitespace and comments dropped) with Jaccard over 8-token shingles.

THE SIX SECTIONS, decided, each claim citing `file:line`:
1. RESPONSIBILITY MAP - every function in `conductor.rs`, `main.rs` and `dash.rs` assigned to a
   proposed module in a proposed module tree (ports-and-adapters as the rust-engineer persona
   already mandates: ports as traits, adapters inward, one composition root), with the current
   line span and the reason for the assignment; unassignable functions are named as such, never
   omitted.
2. DUPLICATION CATALOG - under the strict definition: every cluster of two or more sites whose
   normalized similarity is at or above 0.72, plus every semantic duplicate the auditors find by
   reading (same job, different shape - the `/proc` scan class), each cluster listing ALL sites,
   a classification (exact / near / semantic), and the ONE proposed home. The 76 `Command::new`
   sites, the `/proc` readers, sqlite open/connect paths, path composition for `.rigger/*`, and
   error-shaping helpers are named as mandatory sweeps the catalog must cover.
3. BOUNDARY VIOLATIONS - domain code reaching a concrete adapter, use cases importing
   infrastructure, a second mutation authority for one domain; each with the port that should
   have been used.
4. DEAD AND VESTIGIAL CODE - functions with zero callers in the knowledge graph AND no test
   reference, retired-feature remnants (turbovec, kurrentdb feature flag), stale doc claims.
5. TEST-SUITE SHAPE - the 153 files grouped by subsystem with a consolidation map, the shared
   fixtures to extract into `tests/common`, the split plan for `tests/cli.rs`, and every
   duplicated helper across test files (the strict rule applies to tests).
6. PRIORITIZED PLAN - an ordered list of refactoring specs (stubs: title, scope, files, expected
   line delta, risk, what it unblocks), largest-risk-reduction first; the god-file splits and
   the duplication removals are separate entries so each can be its own run.

THOROUGHNESS, decided: completeness is proven, not asserted. The responsibility map's coverage
is asserted by the generator (count of scanned fns in the three files == count of mapped fns).
The catalog's recall is checked by ADVERSARIAL SAMPLING: the adversary draws 30 functions by
seeded random index across `src/` and `tests/`, and for each finds by reading whether a
duplicate exists anywhere; any duplicate the catalog missed is a blocking finding that widens
the sweep, not a note. The report states the sample seed so the check is reproducible.

WHAT THIS SPEC DOES NOT DO: no production refactoring, no test consolidation, no deletion. It
may add the audit test and the docs only. The refactors are the follow-up specs in section 6.

CONSTRAINTS WALK: empty section (a category with zero findings) - the section states so with
the search that established it, never omitted. Two candidates for one home - the report picks
one and says why (rule 8). Generator on a tree with a new fn - the drift guard fails loudly
naming the file, which is the intended reminder to regenerate. Similarity threshold gaming -
the threshold is a floor for the mechanical pass; the reading pass owns semantic duplicates and
is what the adversarial sample measures.

## Notes (non-criteria)

The mechanical scanner is deliberately simple; `syn`-grade parsing is out of scope (no new
dependency). The knowledge graph (`rigger graph --around`, community structure) is the second
instrument for section 4's zero-caller check and section 2's cross-checks; the report cites
which instrument found what.

## Global constraints

- Hyphens, never em dashes. Both feature lanes green (fmt, clippy -D warnings, test, default
  and --no-default-features); no-os-kill and reap audits green.
- No production code changes; no new dependency; no new event type.
- The operator's installed rigger binary is never replaced or modified by any unit.

## Done when

- [ ] a test proves THE RESPONSIBILITY MAP IS COMPLETE: `tests/simplification_audit.rs` scans
  every function in `src/conductor.rs`, `src/main.rs` and `src/dash.rs` and asserts the
  committed `docs/audit/responsibility-map.json` assigns each one a proposed module with its
  line span, and the report's section 1 renders that map as a proposed module tree with a
  reason per assignment. This criterion OWNS the scanner and the responsibility map; the
  duplication catalog is criterion 2's, NOT this one's.
- [ ] a test proves THE DUPLICATION CATALOG IS EXHAUSTIVE AND STABLE: the generator regenerates
  `docs/audit/duplication-catalog.json` from the tree and it matches the committed file
  byte-for-byte; every cluster has two or more sites with file:line, a classification and one
  proposed home; the mandatory sweeps (Command::new sites, /proc readers, sqlite open paths,
  .rigger path composition, error shaping) each appear; and the adversary's seeded 30-function
  sample finds no duplicate the catalog lacks. This criterion OWNS the similarity pass, the
  catalog and its drift guard; the responsibility map is criterion 1's, NOT this one's.
- [ ] the report's sections 3-5 (BOUNDARY VIOLATIONS, DEAD AND VESTIGIAL CODE, TEST-SUITE
  SHAPE) each cite file:line for every claim, name the instrument that established it, and
  state explicitly when a category is empty; the zero-caller list is cross-checked against the
  knowledge graph. This criterion OWNS sections 3-5 and introduces no generator code.
- [ ] section 6 PRIORITIZED PLAN lists the follow-up refactoring specs as stubs - title,
  scope, files, expected line delta, risk, what it unblocks - ordered largest risk-reduction
  first, with the god-file splits and the duplication removals as separate entries. This
  criterion OWNS the plan; it cites sections 1-5 and adds no new findings.
- [ ] both feature lanes green (fmt, clippy, test on default and --no-default-features).
