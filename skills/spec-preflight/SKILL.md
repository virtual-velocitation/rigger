---
name: spec-preflight
description: Use before launching any rigger spec, after planning-a-spec - simulate landing order and walk every criterion's corners, then run rigger critique on the spec and close every BLOCKING finding before launch.
---

# Spec preflight

## Why

A checklist read in the author's head does not simulate the run. Two spec defects survive
planning-a-spec and each costs a whole loop round:

- Landing-order circularity (F10): two criteria assert the same measured property of one
  command, and each one's fixture needs the other's change. Whichever unit lands first fails its
  own text, and plan-critique finds it only at run time.
- Undecided removal (F11): a criterion claims an identity (byte-identical, equal) and no Design
  sentence decides what a later generation that DROPS a fact does to it. The corner walk never
  reaches it, the implementer narrows the fixture, and the round is lost.

This skill makes you EXECUTE two simulations and one adversarial pass before launch. It does not
repeat planning-a-spec (`../planning-a-spec/SKILL.md`); run that recipe first, then this.

## When

- After the draft passes planning-a-spec.
- Before `rigger validate`.
- Before any launch or relaunch.
- Again after any mid-run Design amendment.
- Never skipped for a small spec.

## Step 1: landing-order simulation

1. Table the criteria in the order their units land (follow the needs chain; independent units
   in any order).
2. List every measured surface each criterion asserts over: a command, a store read, a file, a
   counter, a test double.
3. For every ORDERED PAIR (A, B) sharing a surface, write one line:
   `If A lands first, on a tree WITHOUT B, does A's own text hold? yes/no - why.`
4. Every "no" is a defect. Fix it now, one of three ways:
   - move the assertion to the later unit;
   - split ownership at the seam in a Design block named `CRITERIA A AND B SPLIT AT <seam>`;
   - make the earlier criterion's assertion conditional on what exists at its landing.
5. Rule: a criterion is testable in isolation on the tree it lands on, never only on the
   finished tree.

Worked example (shared surface: the reads one command makes):

    C2 first, without C3: C2 asserts "the command reads no derived event", but excluding
      derived reads needs C3's query to serve them -> NO.
    C3 first, without C2: C3 asserts the same, but its seed relies on C2's exclusion -> NO.
    Fix: Design block "CRITERIA 2 AND 3 SPLIT AT the derived-read seam": C2 owns the
      exclusion, C3 owns the query; only the later-landing unit asserts "reads no derived event".

## Step 2: per-criterion corner walk

For EACH criterion's mechanism (not only per global constraint), write one sentence per corner,
or "out of scope - <reason>":

| Corner | Question |
|---|---|
| empty | No input, no rows, no prior generation. |
| repeated | The same input twice. |
| reverted | An earlier state re-asserted later. |
| DROPPED | A fact, row, file or link present in an earlier generation and absent in a later one. |
| concurrent | Two actors on the mechanism at once. |
| crash-resume | The process dies mid-mechanism; the next run resumes from the log. |
| cold start | A fresh process, empty memory. |
| existing data | A store or tree that predates the mechanism - the upgrade path. |

Rules:
- Every identity claim (identical, equal, the same, byte-identical) names its comparison
  surface: which bytes, which projection, which ordering.
- Its fixture includes the DROPPED corner explicitly. A fixture that only adds or moves facts
  does not test an identity claim.
- A corner no Design sentence decides is a defect; decide it in Design now.

Worked example (an identity claim over a compacted log):

    DROPPED: generation N links doc D -> E; generation N+1 re-ingests D without the link.
      The compacted log keeps only N+1; the original log holds N and N+1. Undecided: does
      N+1 supersede N's link in the fold? If not, the two rebuilds differ -> the claim is false.
    Fix: Design "a generation supersedes the whole prior generation; the comparison surface is
      the live projection, not raw bytes"; the fixture drops a link and drops an entity.

## Step 3: the adversary pass

Run `rigger critique <spec>`. It runs the workflow's critic (the plan-critique gate's adversary,
else `defaults.review.adversary`) against the spec text, records its findings and verdict keyed on
the text's content hash, and prints them; unchanged text is answered from the record, and any edit
is critiqued afresh. Under a workflow naming neither, `rigger critique` refuses and runs are not
gated, so skip Steps 3 and 4.2. Each finding is one line:

    <id> | BLOCKING or NON-BLOCKING | <criterion or Design block> | <exact reading that breaks> | <smallest Design change that closes it>

## Step 4: resolve, record, re-run

1. Close every BLOCKING finding in Design or Global constraints. Once a run has built code, never
   edit a criterion. Before any run, a criterion rewrite is allowed, and Steps 1-3 then run again
   from the top.
2. Run `rigger critique <spec>` on the amended text until it returns no BLOCKING finding. Close or
   record each NON-BLOCKING one.
3. Close a BLOCKING finding you judge wrong, or already decided, with a recorded resolution
   instead of a text change (`<ids>` are the quoted finding ids):

       rigger emit DecisionMade '{"id":"preflight-<short>","governs":["<spec>"],"resolves":[<ids>],"summary":"closed <ids>: <one line each>"}'

4. `rigger validate <spec>`.
5. Launch.

Refusal rule: do not launch, relaunch, or amend-and-continue with an open BLOCKING finding.

## Mid-run amendment

When a review or plan-critique reject names a defect in the spec itself:

1. Amend Design and Global constraints only; a criterion edit orphans the live run.
2. Land the amendment between steps, never while a step is mid-flight.
3. `rigger emit DecisionMade` with the spec path in `governs`, so in-flight agents see it through
   the graph.
4. Run Steps 1-3 on the amended spec before the next step spawns.

## Operator binary boundary

An agent never installs, replaces, or modifies the operator's installed `rigger` binary - that binary is operator-only. A tree checkout's own `rigger` build is invoked only by explicit path, and only to render (spec/docs output) - never to overwrite what is on PATH.
