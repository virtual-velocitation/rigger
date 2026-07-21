# 31 - Idempotent plan refinement: a same-id re-emit updates a proposed unit in place

**Goal:** close a conductor defect that makes a plan-critique-requested REFINEMENT unwinnable. When
the plan-critique approves a decomposition but asks the planner to re-emit its units with refined
`needs` edges, the planner today has no clean way to comply: re-emitting the SAME unit-ids is
silently skipped (so the refinement never applies), and re-emitting with NEW ids adds a second unit
per criterion that no planner action can remove - a rule-7 duplicate-ownership escalation the loop
cannot clear. This makes the refine path deterministically fatal for any spec whose plan-critique
asks for one (observed: spec 26 escalated at `plan-critique` with two byte-identical unit chains).

The fix gives the re-emit path one clean semantic: **a re-emit under the SAME unit-id UPDATES that
unit in place (a refinement); a NEW unit-id is a genuinely new unit (a split sibling).**

## Design

`harvest_proposed` (`src/conductor.rs`) folds each run-scoped `UnitProposed` into the DAG. Today it
short-circuits an already-known unit: `if u.id.is_empty() || proposed.contains(&u.id) { continue }`
and `if stages.contains_key(&u.id) { continue }`. That skip is why a same-id refinement is a no-op.
Separately, a planner unit serving criterion C SUPERSEDES C's synthesized BASELINE stage (matched by
the echoed stable `criterion_id` via `normalize_criterion_id` / `criterion_stable_id`, with a prose
fallback) - but ONLY the baseline. Once the baseline is consumed by the first planner unit, a later
unit serving C (with a new id) finds no baseline to supersede and is ADDED, producing the duplicate.

Two coupled changes:

1. **Update-in-place on a same-id re-emit.** In `harvest_proposed`, a `UnitProposed` whose id
   already names a PROPOSED, not-yet-started, non-terminal stage no longer skips: it FOLDS the
   re-emitted refinable fields - the `needs` edges (and `coverage`/`criterion_id` if changed) - into
   the existing `Stage`, leaving one unit. Started/integrated/terminal units are still never mutated
   (the existing `integrated`/`terminal` guards hold). This makes a refinement idempotent and
   effective, so the planner never needs a new id to make a re-emit "take".

2. **Refine instruction reuses ids.** The plan-critique / re-emit directive the conductor gives the
   planner (the reviewer/refine prompt built around the plan-critique gate, `src/conductor.rs`)
   states the rule explicitly: to REFINE an existing unit, re-emit it under its EXACT existing id
   (the refinement updates it in place); use a NEW id ONLY for a genuinely new/split unit. This
   closes the id-change that triggered the duplication.

Because same-id now means refine and a new id means a new unit, a genuine SPLIT (two distinct new
ids intentionally serving one criterion) is unaffected: the first supersedes the baseline, the
sibling finds none and is added - exactly as today.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any external
  tool or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- Determinism by construction: any stage/needs collection stays `BTreeMap`/`BTreeSet`/sorted `Vec`;
  an identical re-emit sequence yields an identical DAG.
- No new event type: this refines how existing `UnitProposed` events fold; it adds no event kind.
- Run-scoped and replay-safe: the update-in-place operates only on the CURRENT run's proposed units
  (the existing `run::current_run` scoping), and a full replay of the same event log rebuilds the
  same refined DAG.

## Done when

- [ ] a test proves a SAME-ID re-emit UPDATES a proposed unit in place: after a unit is proposed and
  then re-emitted under its exact id with added `needs` edges, `harvest_proposed` yields exactly ONE
  stage for that id carrying the refined `needs` (not a skip, not a duplicate), and re-emitting every
  criterion's unit this way leaves exactly one unit per criterion so the plan-critique sees no rule-7
  duplication. This criterion OWNS the update-in-place refine and its one-unit-per-criterion outcome.
- [ ] a test proves a genuine SPLIT is preserved: two DISTINCT new ids intentionally serving one
  criterion both survive `harvest_proposed` (the first supersedes the baseline, the sibling is added)
  - the same-id update path does not collapse a real split. This criterion OWNS split-preservation;
  it does NOT own the update-in-place mechanism (criterion 1).
- [ ] a test proves a STARTED/integrated/terminal unit is never mutated by a re-emit: a same-id
  re-emit arriving after the unit has started/integrated is ignored (the existing guards hold), so a
  late refinement can never disturb work already in flight. This criterion OWNS the started-unit
  immutability guard; it does NOT own the refine or split behavior (criteria 1-2).
- [ ] a test proves the refine directive instructs id REUSE: the plan-critique / re-emit prompt the
  conductor generates tells the planner to re-emit a refinement under the EXACT existing unit-id and
  to use a new id only for a genuinely new unit. This criterion OWNS the refine-instruction text; it
  does NOT own the fold behavior (criteria 1-3).
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
