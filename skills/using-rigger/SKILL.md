---
name: using-rigger
description: When and how to drive rigger - the one blessed driver, spec shape, base anchoring, the verdict contract, and fix-the-loop discipline. Read this before starting or driving a rigger run.
---

# Using rigger

This is the front-door for driving a rigger run: when to reach for the loop, the one blessed way to drive it, and the rails that keep a run honest. It is generated from the code the binary runs on, so its facts cannot drift from behavior.

## When to reach for rigger

Reach for rigger when you have a written spec whose "Done when" section enumerates machine-checkable criteria and you want it built, tested, reviewed, and integrated without hand-holding each step. Do NOT reach for it for a one-line edit, an exploratory spike, or work that has no spec to anchor acceptance on - the loop's value is the disciplined lifecycle around a checkable spec, and without one there is nothing for it to hold to.

## The one blessed driver

Drive every run through the native /rigger workflow (visible in /workflows and on the dashboard at 127.0.0.1:7420). It launches the loop and keeps the event log, the ledger, and the context graph consistent with one another. These anti-patterns split the run's state away from that shared record and must be avoided:

- Polling git or ps by hand to guess progress. Read the dashboard or `rigger status`; the by-hand view misses the ledger and the graph.
- Hand-driving `rigger step` in a shell. The driver owns stepping; a hand step races the driver and can double-spawn or wedge the frontier.
- Hand-implementing a unit the loop parked. That leaves the loop still stuck for the next unit and forks the code from the log - fix the loop instead (see below).

## Spec shape

One observable behavior per criterion; the atomic unit is one checkbox; put type shapes and structural detail in a non-criteria Notes section. The loop's spec-shape lint flags these shapes because a planner paraphrases or truncates them when told to copy a criterion verbatim, which then fails the baseline match the conductor reconciles proposals against: multi-behavior, sub-bullet-as-unit, over-long. Recommendation: one observable behavior per criterion; put type shapes and detail in a non-criteria Notes section.

## Base anchoring

A run anchors its branch on the working ref (default origin/main) and reuses that branch once it exists. Anchor on the ref you actually want the work to land on, not a stale default: the anchor is what every unit worktree branches from and every approved unit merges back into, so an anchor on the wrong ref lands the run in the wrong place.

## When it wedges, fix the loop

If a unit will not pass, the fix belongs in the loop - the spec, the gate, the agent, or the config - never a manual edit that sidesteps it. A by-hand fix leaves the loop broken for the next unit and splits the code from the log, so the run can no longer be trusted to replay. Correct the underlying cause and let the loop re-run the unit.

## Auto-integration on approve

An approved unit integrates itself onto the run branch. A human reviews the whole run by opening a pull request FROM the run branch, never by cherry-picking approved units by hand - cherry-picking drops the run's accumulated context and its ordering. A failing unit is retried under a bounded budget (up to 3 attempts) and then escalated to a human rather than spinning forever.

## The verdict line

Every gating agent ends its output with its verdict line: a JSON line carrying {"verdict":"approve"} to approve (or the rejecting value to send the unit back). The integration gate reads that result line, not events recorded through any side channel, so an agent that records its verdict only out-of-band returns no verdict the gate can see and stalls the run. Anyone authoring or porting a gating persona must keep this line.

## Self-serve

Run `rigger version` to see the exact binary and its build provenance and to diagnose drift between the installed /rigger workflow and the binary that would run it. This repo keeps its specs in specs/. The full command surface is: run, step, reported, prompt, serve, workflow, graph, stats, canary, playbooks, replay, status, dash, ground, reindex, symbols-index, emit, progress, result, peers, reset, validate, init, setup, docs, prime, version, help.

## The load-bearing decisions

The discipline explains its own constraints:

- One source of truth: every drift-prone fact in this document is read from the code the binary runs on, so the document cannot silently disagree with behavior. A drift check re-renders and diffs it, so it stays accurate rather than merely starting accurate.
- Blast-radius isolation: each unit does its work in its own worktree, so concurrent units never clobber one another and every unit's change is reviewed on its own diff.
- Fail-closed review: only an explicit approve verdict integrates a unit; a missing, unparseable, or rejecting verdict routes the unit back to remediation rather than passing it silently.
