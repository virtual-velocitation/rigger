# 80 - Criteria survive extraction: a checkbox's whole text reaches every consumer

**Goal:** `src/spec.rs::extract_criteria` (via `checkbox_text`, src/spec.rs:6-11,76-90) matches
ONLY a checkbox's first physical line, so every `Done when` criterion wrapped across lines -
which the planning discipline mandates, since OWNS/exclusion clauses and verify-wording rarely
fit one line - is silently truncated to its first ~88 columns in `self.deps.criteria`
(wired at src/conductor.rs:13272; the only other call site is src/main.rs:3610). Downstream,
`resolve_served_criterion` (src/conductor.rs:8320-8342) canonicalizes every proposal's
`st.coverage` to that truncated text BY DESIGN (anti-paraphrase), so the loss is structural: no
replan can restore it, unit titles / grounding queries / `UnitStarted.spec_criterion` /
`build_dag_critique_prompt` all serve OWNS-stripped criteria, and plan-critique correctly
rejects DAGs whose rule-7/rule-8 prose the storage layer itself discarded. Proven live in the
spec-62 fresh run (decisions `adj2-pc62-confirmed-conductor-coverage-refine-bug-root-cause` and
`plan62-replan3-corrected-root-cause-extractor-not-fold-branch`, verified at 09aec75 against
raw stored events; the same defect was previously flagged for specs 59-65 and 74). The fix
belongs in the extractor alone: join a checkbox item's continuation lines so the canonical
criterion text is the bullet's FULL text; zero changes in conductor.rs are then needed.

## Design

JOINING RULE, decided here so no unit has to: a checkbox item's text runs from its `- [ ]` (or
`- [x]`) marker to the start of the next checkbox item, a blank line, or a heading line,
whichever comes first. Continuation lines have their leading indentation stripped and are
joined with single spaces (the text is prose; internal line breaks are wrapping, not meaning).
Trailing whitespace trimmed. Nested sub-bullets under a checkbox (a line whose trimmed form
starts with `-` or `*` but no checkbox marker) are PART of that checkbox's text, joined the
same way - the spec-shape lint already discourages them, but the extractor must not silently
drop what an author wrote. Nothing else about extract_criteria changes: same ordering, same
stable ids, same call sites, no signature change.

BLAST RADIUS, decided: `src/spec.rs` only, plus tests. The conductor's canonicalization
(`resolve_served_criterion`, the ADD-path overwrite at conductor.rs:8235, the fold branch's
coverage no-op) is CORRECT once fed full text and must not be touched - decision
`plan62-replan3-corrected-root-cause-extractor-not-fold-branch` explicitly proved the
fold-branch "refresh coverage" idea would reintroduce paraphrase drift; it stays unbuilt.

THIS SPEC'S OWN CRITERIA are deliberately single physical lines (no wrapping), so they pass
through the CURRENT, unfixed extractor intact while this spec is the thing being built.

## Notes (non-criteria)

Consumers that start carrying fuller text after this fix: unit titles (conductor.rs:3624,
4202, 4703, 6130), the grounding query (3253, 9802), `UnitStarted.spec_criterion` (3333-3334),
`build_dag_critique_prompt` (6011-6032). No stored-event migration: already-recorded events
keep their truncated text; only newly extracted criteria improve. After this spec lands, the
operator rebuilds and reinstalls the binary, then relaunches spec 62 (whose run ef0497a5 was
parked on exactly this defect).

## Global constraints

- Hyphens, never em dashes. Both feature lanes green (fmt, clippy -D warnings, test, default
  and --no-default-features); the no-os-kill gate green on every unit's diff.
- No new event type, no new dependency, no signature change to extract_criteria's public API.
- The operator's installed rigger binary is never replaced or modified by any unit.

## Done when

- [ ] a test proves FULL-TEXT EXTRACTION: for a spec whose checkbox wraps across three-plus physical lines with an OWNS sentence on the third, `extract_criteria` returns the bullet's entire joined text (single-spaced, indentation stripped), pinned at the `src/spec.rs` seam; and for adjacent checkboxes, blank-line-then-prose, a following heading, and a nested sub-bullet, the boundaries and joining follow the Design's JOINING RULE exactly. This criterion OWNS `src/spec.rs` and its unit tests; end-to-end delivery is criterion 2's, NOT this one's.
- [ ] a test proves DELIVERY TO CONSUMERS: driving the real extraction path a multi-line criterion's full text (including its third-physical-line OWNS sentence) reaches `self.deps.criteria` and is what `UnitStarted.spec_criterion` carries for that criterion's baseline unit, proven at the periphery against specs/62-dash-marker-lifecycle.md's own c1 text. This criterion OWNS the periphery proof; the extractor itself is criterion 1's, NOT this one's.
- [ ] both feature lanes green (fmt, clippy, test on default and --no-default-features).
