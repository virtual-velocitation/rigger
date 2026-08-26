# Architecture addendum: the world reconciler

Status: PROPOSED - for operator review. Nothing in this document is built yet; every
section below describes the TARGET state except "Problem", which records the measured
present.

## Problem

Rigger is event-sourced: every decision, finding, verdict, and result lives in the event
log, and the log is authoritative - a run can be replayed, resumed, and audited from it
alone. But the log's authority stops at the edge of the PHYSICAL WORLD the run creates:

- filesystem: unit worktrees, per-unit build caches, the shared gate cache, per-spawn
  agent scratch, mutation-testing scratch, tombstones, liveness markers, lock files;
- processes: the driver, gate builds, spawned agents, the dashboard, test-spawned
  servers;
- exclusive positions: the step lock, the build-budget slots, the singleton dash port.

These resources are created ad-hoc at many call sites, owned by nobody after creation,
cleaned by per-class heuristic reapers (a worktree sweep keyed on directory names, an
orphan-scratch walk, a dash self-reap timer), and watched - when watched at all - by an
external orchestrator polling `rigger watch`. Measured consequences on this project:

- the project directory reached 403G: worktrees carrying embedded 19G build trees
  (agents building without the shared cache environment), per-unit caches surviving
  their units, 47G of mutation scratch leaked by killed runs, a 108G root build cache
  no surface ever reported;
- the volume holding the store reached 775KB free before any signal fired;
- two drivers stepped the same project concurrently (nothing declares how many drivers
  should exist, so nothing could refuse the second);
- a worktree sweep deleted an ESCALATED unit's worktree while the operator was applying
  the escalation remedy inside it - the sweep keyed on name and process liveness, and
  could not know what the log knew: that the unit was awaiting a human;
- dead test processes (idle dashboards, a stranded server) survived their worktrees by
  days, invisible to every surface.

Each incident got a fix, and each fix was one more mechanism in an inventory that now
includes: the terminal-state sweep, the orphan-scratch walk, the per-spawn reclaim, the
dash ensure/self-reap pair, the marker-staleness watchdog, the residue advisories, and
an operator-side polling loop. Every one of them re-derives, from heuristics, facts the
log already holds exactly. The defect class is singular: THE WORLD DRIFTS FROM THE LOG,
AND NOTHING OWNS RECONCILING THE WORLD TO THE LOG.

## The principle

Extend the log's authority over the physical world by construction:

1. every resource rigger creates is DECLARED in the log at creation, with an owner and
   a terminus;
2. the set of resources that SHOULD exist at any moment is a pure function of the log;
3. ONE reconciler continuously converges the actual world toward that derived state;
4. a divergence the reconciler can converge with exactly one safe action, it converges
   silently; a divergence needing judgment is - by that fact alone - a notification.

This is the reconciliation-loop architecture: declared desired state, observed actual
state, a controller that closes the difference. Rigger already has the hard half - a
durable, replayable declaration substrate - as its core. This addendum finishes the
loop.

```
                     +--------------------------------------+
                     |            EVENT LOG                  |
                     |  runs, units, spawns, results,        |
                     |  resource declarations (new)          |
                     +-------------------+------------------+
                                         |
                            pure derivation (no I/O)
                                         v
                     +--------------------------------------+
                     |         DESIRED WORLD                 |
                     |  worktrees, caches, scratch,          |
                     |  processes, leases - each with        |
                     |  owner + terminus + cardinality       |
                     +-------------------+------------------+
                                         |
              +--------------------------+---------------------------+
              |                     RECONCILER                       |
              |   diff(actual, desired) -> converge | notify         |
              +--+----------------+----------------+---------------+-+
                 |                |                |                |
          absent+desired   present+undesired   over-envelope   unconvergeable
                 |                |                |                |
              CREATE            REAP        RECLAIM-then-REFUSE   NOTIFY
           (dash, lease)   (every orphan       (disk budget)    (escalation,
                            class, uniformly)                    churn, store
                                                                 integrity)
```

Why a loop and not better guards: a guard protects one call site against one hazard and
must be re-invented per hazard (the measured history above). A reconciler is indifferent
to HOW the world diverged - crash, kill, operator error, a bug in rigger itself - because
it never reasons about causes, only about the difference between what is and what the
log says should be. Every future failure mode in the resource domain is covered the day
it first occurs, not the day it is first diagnosed.

## The resource model

A RESOURCE is anything rigger brings into existence outside the log. Every resource has:

- an IDENTITY: its class and its path/pid/port, injectively derived from its owner's id
  (the one shared injective encoding; no two owners can name one resource);
- an OWNER: the log entity whose existence justifies it (a spawn, a unit, a run, or the
  project itself);
- a TERMINUS: the owner's lifecycle event at which the resource stops being desired
  (spawn result recorded; unit terminal; run superseded; project-level resources have
  cardinality instead - see below);
- a CARDINALITY where identity is positional rather than owned: exactly-one driver
  lease per project, exactly-one dashboard per machine, at-most-N build slots.

### The creation authority

Resources enter the world through ONE creation authority, which performs the physical
creation and appends the declaration as a single logical act (declaration first, then
creation; a crash between the two leaves a declared-but-absent resource, which the
reconciler treats as absent-and-undesired-to-recreate for scratch classes and
absent-but-desired for positional classes - both convergent). No call site touches the
filesystem or spawns a process directly. The declaration rides EXISTING event surfaces
as metadata on the events that already mark the owner's lifecycle (a spawn's request
event carries its scratch identities; a unit's first stage event carries its worktree
and cache identities) - the resource ledger costs no new event type.

### The derived desired world

`desired_world(log) -> ResourceSet` is a pure fold, computable at any position, with no
filesystem or process I/O. The derivation per class:

| resource class            | desired while...                     | terminus                       |
|---------------------------|--------------------------------------|--------------------------------|
| unit worktree             | unit is live OR awaiting a human     | unit integrated / abandoned at |
|                           | (escalated counts as DESIRED)        | a superseding run boundary     |
| per-unit build cache      | same as its worktree                 | same as its worktree           |
| per-spawn agent scratch   | spawn requested, no result           | result recorded                |
| per-spawn mutation scratch| same as agent scratch                | same as agent scratch          |
| shared gate cache         | always desired; size-governed        | never (envelope-governed)      |
| tombstones                | never desired                        | immediate                      |
| liveness markers          | their spawn desired                  | spawn result recorded          |
| driver lease              | cardinality exactly-one per project  | lease renewal lapses           |
| dashboard                 | cardinality exactly-one per machine  | machine idle window            |
| gate/agent processes      | their spawn desired                  | spawn result recorded          |
| step lock                 | held only inside a step              | step exit                      |

The table is the WHOLE contract: adding a resource class means adding a row (a deriving
event, a terminus), never a new reaper, watcher, or guard.

Why derived and not stored: a stored desired-state can itself drift (it is one more
resource). A derivation from the log cannot - it is exactly as durable, replayable, and
crash-consistent as the log, which is the strongest guarantee the system has.

## The reconciler

One function, `reconcile(actual, desired)`, with four arms and three rails.

Arms, in fixed order (reclaim before create, so convergence never worsens pressure):

1. REAP present-but-undesired: worktrees, caches, scratch, markers, tombstones whose
   owner reached its terminus; processes whose owning spawn has a recorded result.
   Uniform, unconditional on the derivation - the reconciler never re-checks liveness
   by heuristic, because "desired" already encodes liveness from the log (this is
   precisely what makes the escalated-unit worktree safe: the log says awaiting-human,
   so it is desired, so no arm touches it).
2. CONVERGE size envelopes: when a size-governed class (the shared cache; total
   footprint) exceeds its configured envelope, reclaim undesired residue first; if
   still over, the envelope becomes a REFUSAL at the creation authority - new builds
   are declined with the reclaiming command named - never a mid-flight deletion.
3. CREATE absent-but-desired positional resources: the dashboard, the lease file
   structure. (Owned resources are created by their owner's own flow, not the
   reconciler; recreating a spawn's scratch makes no sense without the spawn.)
4. NOTIFY the unconvergeable: any diff with no single safe action. The set is small
   and closed: a unit awaiting a human, review churn past threshold, store integrity
   faults, an envelope still exceeded after full reclaim, a positional resource that
   cannot be created (port genuinely held by a foreign process).

Rails (safety, in priority over every arm):

- LOG-DERIVED ONLY: no arm may act on a resource the derivation does not name.
  Unrecognized paths under rigger's roots are reported as foreign residue, never
  deleted - fail-safe extends to things rigger does not remember creating.
- ONE SAFE ACTION: an arm exists only where the correct response is unique and
  independent of intent. Anything else is arm 4 by definition.
- IDEMPOTENT AND CONCURRENT-SAFE: reconcile passes may overlap (two commands at once);
  every action is either atomic (rename, unlink, flock) or guarded by the resource's
  own exclusion (the same guard-file discipline the shared cache uses).

### Cadence: reconcile-on-invocation

Every rigger entry point that touches the project - `step`, `serve`, `run`, `validate`,
`watch`, `reset` - runs one reconcile pass on entry (the auto-maintenance pattern:
using the tool maintains the tool). The dashboard singleton, which already exists on
every stepping machine, runs the same pass on its poll interval, giving a cadence floor
when no commands run. There is no new resident process and no consumer-side wiring:
operating rigger AT ALL is what monitors rigger. An orchestrator that dies, a quota
that lapses, a machine that reboots - the next invocation of anything converges the
whole world.

```
   any rigger command ----+
                          +--> reconcile(actual, desired_world(log)) --> converge
   dash poll tick --------+                                          \-> notify residue
```

### The driver lease (cardinality worked example)

The lease is a flock-held file plus a lease declaration in the log naming the holder
and a renewal horizon. A driver acquires it at launch and renews on every step; a
second driver's acquisition fails fast and loud - the double-drive becomes impossible
at the entry point rather than detectable afterward. A crashed driver's lease lapses at
its horizon; the reconciler observes lapsed-but-locked (or lapsed-and-free) and clears
it, so recovery needs no human. `rigger status` reports the leaseholder; a resume is
the same acquisition, so the resume-vs-fresh decision no longer risks racing a live
driver.

### The notification channel (judgment residue only)

Arm-4 diffs are appended to `anomalies.jsonl` under the project store - append-only,
one JSON object per line, with a stable identity per anomaly and an explicit
cleared-marker when a later pass observes the condition gone. Consumers read it three
zero-lift ways, all fed by the same file:

- `rigger watch --follow` tails it (the existing watch surface becomes a reader of
  reconciler output instead of a second detector);
- the dashboard renders open anomalies and serves them over its existing endpoint;
- the setup-installed harness hook injects new-since-last-turn anomalies into the
  orchestrator's context at turn boundaries, so an LLM orchestrator receives judgment
  work without polling anything.

A `notify:` config key (a command to exec per new anomaly) covers desktop or chat
delivery without rigger knowing any provider. Dedup and rate-limiting live in the
writer - state the stateless `watch --once` could never hold - so a standing condition
is one line and one clear, not a page per poll.

## What this deletes

The measure of the design is the mechanism inventory it retires. Become ARMS of the one
reconciler (their behavior preserved, their independent existence gone): the
terminal-state worktree sweep, the orphan-scratch walk, the per-spawn reclaim call
sites, tombstone sweeping, marker cleanup, dash ensure and self-reap, residue
advisories. Become IMPOSSIBLE rather than handled: double drivers (lease), disk
exhaustion by rigger's own writes (envelope refusal at creation), sweeping an
awaiting-human worktree (desired by derivation), unowned scratch of every future kind
(creation authority). Become CONSUMER-FREE: all operator-side polling loops, watcher
scripts, and disk guards - the reference orchestrator for this project runs none after
delivery, which is the acceptance bar: if operating rigger's own development still
needs a hand-rolled monitor, the addendum has failed.

## Delivery shape (for decomposition after review)

1. Resource declarations + derivation: the creation authority, ledger metadata on
   existing events, `desired_world` as a pure tested fold. No behavior change yet -
   validate gains a world-diff REPORT (observe-only) proving the derivation against
   real runs.
2. The reconciler arms 1-3 + rails, replacing the retired mechanisms arm by arm, each
   retirement pinned by the tests that guarded the old mechanism.
3. The driver lease and disk envelope (the two impossibility conversions).
4. The channel: anomalies.jsonl writer with dedup state, watch-as-reader, dash surface,
   the setup-installed turn-boundary hook, the `notify:` key.

Ordering rationale: observe-only first (the derivation must earn trust against live
history before any arm acts on it), impossibility conversions before channel polish
(they remove more risk than any notification can).
