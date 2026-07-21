# Reference Architecture Addendum — Loop Execution at Scale

> The dev-loop builds ONE spec by fanning its independent units into isolated worktrees. A body of
> work larger than one spec — a CAMPAIGN of many specs — has no equivalent execution substrate: the
> specs run strictly one after another even when they share nothing, and the binary that drives them
> drifts from the code the loop has already integrated. This addendum specifies the substrate that
> runs a campaign as a dependency-scheduled, SELF-HEALING fleet of isolated runs that right-sizes its
> own parallelism to the host, and keeps the driver current with the source it lands. It is a blueprint
> for the required state, not a description of the present one.

---

## 1. Problem (measured)

Two gaps, both measured on a real multi-spec campaign.

**1a. Sequential execution wastes the critical path.** A campaign is a set of specs with
dependencies — a directed acyclic graph. The loop runs them strictly in series, so wall-clock is the
SUM of every run, even for specs that share no files and no dependency edge. Measured: individual
runs took 4.0–6.1 hours each; an 8-spec campaign's wall-clock was their sum (~40 hours), while the
DAG's true critical path was only 6 runs. A third of the specs were mutually independent and could
have overlapped — and did not.

**1b. Runs cannot overlap, because isolation stops at the run boundary.** The loop isolates UNITS
within a run — each in its own worktree — but nothing isolates one RUN from another. A run integrates
onto ONE run branch in the MAIN working directory, appends to ONE shared event store and context
graph, shares ONE cargo target directory across its gates, and its frontier advance is serialized so
two never proceed at once. Each of those is a single-writer choke, and each fails a specific way if
forced:

```
  shared resource            what a 2nd concurrent run does to it         failure class
  ------------------------   ------------------------------------------   ------------------------
  run branch + main worktree two writers stage/commit one index/HEAD      tree corruption
  event store (one stream)   two runs interleave their run boundaries      run-provenance scramble
  context graph (one db)     two runs fold into one projection            cross-run node bleed
  cargo target (one dir)     two gate suites clobber each other's build   FALSE red gate verdicts
  serialized frontier        the 2nd advance blocks on the 1st            no concurrency at all
```

So the loop cannot run two specs at once — independent or not.

**1c. The driver drifts from the code it integrates.** The loop is driven by an INSTALLED binary,
not the source tree it lands commits into. As a campaign integrates spec after spec, the driving
binary stays at whatever build was installed; it does not pick up the orchestration and grounding
changes it has itself merged. Measured: the driver ran a build many integrated commits behind its
source, and in one observed case drove a run on orchestration logic that a since-integrated change
had already superseded — so the loop failed to benefit from its own landed fix until the binary was
rebuilt by hand. A loop that does not run its own latest code cannot dogfood its improvements, and a
stale driver can re-exhibit a defect the source has already cured.

**1d. A fixed degree of parallelism is wrong in both directions.** Too low wastes headroom; too high
thrashes memory and starves agents. Measured: under memory pressure (swap fully exhausted), a run's
review agent hung past its wall-clock and was classified an infrastructure fault that required a
MANUAL re-drive to clear; separately, concurrent gate builds sharing one target directory produced
false-red verdicts. A static fleet cannot respond to conditions it can already measure — it neither
backs off before it thrashes nor heals after an infra fault.

### Non-goals / anti-fixes

- NOT distributed execution. A single host with a fleet bound sized to local resources — never a
  cluster, a scheduler daemon, or a network protocol.
- NOT parallelizing DEPENDENT specs. The dependency DAG is honored exactly; only mutually
  independent specs overlap. A blocked spec waits, it is never speculatively started.
- NOT changing within-unit or within-run behavior. The unit lifecycle (implement → gates → review →
  integrate) and the three-tier review are untouched. This adds a layer ABOVE the run, reusing the
  run unchanged.
- NOT a second build system. Driver currency reuses the existing release build and the existing
  build-provenance signal; it introduces no parallel toolchain and no new artifact format.

---

## 2. Load-bearing invariants — what the design must carry

### 2.1 Isolation is fractal — the run is to the campaign what the unit is to the run

A unit isolates from a concurrent unit by running in its own worktree; a run isolates from a
concurrent run the same way, one level up — its own worktree AND run branch AND target directory AND
event-store scope. It is the SAME isolation primitive at two levels. The invariant: nothing
concurrent shares mutable state; the instant two concurrent runs would touch one branch, one store,
or one target, the design has leaked. This is what makes the serialized-frontier choke (§1b)
unnecessary between runs — two runs on disjoint store scopes have nothing to serialize.

### 2.2 The dependency DAG is both the schedule and the merge order

A spec becomes READY only when every spec it `needs` has INTEGRATED. Ready specs launch up to the
fleet bound; finished runs merge back in a deterministic topological order. The same edges that gate
a start gate a merge — one DAG, two uses. A cycle in the manifest is a hard authoring error refused
before any run starts, exactly as an unwinnable unit DAG is.

### 2.3 Driver currency — the binary driving a run reflects the source that run builds on

A run is driven by a binary built from the base its worktree was cut from. When a dependency merges
and moves the integration tip, a dependent that builds on it is driven by a binary rebuilt from the
NEW tip. The loop never drives a run on orchestration older than the run's own base — the driver is
downstream of the integration branch, always.

### 2.4 Loud on conflict — a merge or build conflict escalates, never drops

A run-branch merge that conflicts, or a dependent that no longer builds on its updated base, is
surfaced as a campaign-level escalation carrying the offending spec — the same loud terminus a wedged
unit gets. It is never silently skipped, force-merged, or reported as a clean campaign completion.

### 2.5 Determinism is about the OUTCOME, not the timing

The topological merge order and the final integration tree are computed from the DAG with a stable
tie-break (the spec id), so the same manifest always yields the same integration history — regardless
of how many runs happened to overlap or which finished first. The concurrency LEVEL (§2.6) is a free
variable that changes only wall-clock, never the result: the same campaign run at fleet 1 and at
fleet N produces a byte-identical integration tree. Adaptivity may reschedule; it may never re-decide.

### 2.6 The fleet is adaptive and self-healing — right-sized to measured conditions

The degree of parallelism is a CONTROLLED variable, not a fixed constant. The orchestrator
continuously reconciles concurrency against measured resource headroom and loop-level fault signals:
it DRAINS (holds back new launches, letting in-flight runs finish) under pressure, and FILLS (raises
concurrency toward the DAG's available width) under sustained headroom. A failure classified
INFRASTRUCTURE — an agent hung past its wall-clock, a build OOM-killed, a gate reddened by target
contention — is a right-size-AND-RETRY signal, never a code escalation. The system converges toward
the largest parallelism the host can sustain and no larger: it heals rather than thrashes or stalls.
The orchestrator is INFORMED (the resource and fault signals are surfaced to it) and CAPABLE (it holds
the fleet lever and may override the default policy).

---

## 3. Workstream A — the campaign as a DAG of isolated runs

### 3.1 The campaign manifest — a spec-level DAG

A campaign is a typed manifest: a set of spec-nodes, each naming the specs it depends on. It is the
spec-level analogue of the unit DAG the conductor builds INSIDE a run.

```
NODE KIND    FIELD          ROLE IN THE CAMPAIGN
----------   ------------   --------------------------------------------------
spec-node    id             stable campaign id (the tie-break key, §2.5)
             spec           the spec path this node runs through the loop
             needs[]        the spec-nodes that must INTEGRATE before this starts
             status         pending | ready | running | integrated | escalated
             base           the integration-branch commit this run was cut from
             run-branch     the isolated branch this run integrates its units onto
```

The manifest is the ONLY place cross-spec order is declared. Everything else — which runs overlap,
in what order they merge — is DERIVED from `needs`, never hand-sequenced.

### 3.2 The per-run isolation context

Each concurrently-executing spec-node is given an isolation context at launch, and it is torn down
at merge. Every field exists to remove one shared-writer choke from §1b:

```
CONTEXT FIELD        ISOLATES                        removes the choke
------------------   -----------------------------   ---------------------------------
worktree             a dedicated git worktree cut     main-worktree index contention
                     from `base`
run-branch           a dedicated branch the run's     shared-run-branch tree corruption
                     units integrate onto
target-dir           a dedicated CARGO_TARGET_DIR     shared-target false-red gates
store-scope          a run-namespaced event stream    interleaved run boundaries +
                     + graph scope                    cross-run node bleed
```

The store-scope reuses the graph project-scoping mechanism (a run is scoped like a project is): two
runs' events and nodes are prefixed disjointly, so a fold, a traversal, or a prune in one run cannot
see the other. Because the scopes are disjoint, the frontier advances of two runs touch no common
row — which is precisely why they need no cross-run serialization (§2.1).

### 3.3 Fractal isolation — one primitive, two levels

```
        CAMPAIGN  (integration branch · the merge target)
        ┌───────────────────────────────────────────────────────────────┐
        │  run A (worktree_A · branch_A · target_A · scope_A)   [running] │
        │    ├─ unit a1 (worktree)   ┐                                    │
        │    └─ unit a2 (worktree)   ┘ conductor fans units (EXISTING)    │
        │  run B (worktree_B · branch_B · target_B · scope_B)   [running] │  <- NEW: fan RUNS
        │    ├─ unit b1 (worktree)                                        │
        │    └─ unit b2 (worktree)                                        │
        └───────────────────────────────────────────────────────────────┘
                 A and B share NO branch, store scope, or target
                 -> safe to run at once, exactly as units a1/a2 do
```

The whole design is this generalization: the loop's proven unit-isolation, lifted to the run. Nothing
below the run line changes; the campaign line is added above it.

---

## 4. Workstream B — the scheduler and the integrator

### 4.1 The scheduler — fill the fleet from the ready set

```
  loop until every spec-node is integrated or escalated:
    READY   = { node | status==pending AND every needs-node is INTEGRATED }
    FREE    = fleet_bound - count(status==running)
    launch  min(FREE, |READY|) nodes in ready order (id-sorted, §2.5),
            each in a fresh isolation context cut from the CURRENT integration tip
    await   any running run reaching a terminus, then re-compute
```

The fleet bound is sized to local resources (a run is ~one concurrent unit's build load times its
in-flight units); it is a single knob, not a scheduler policy. When the ready set is smaller than the
free fleet — the common tail of a deep DAG — the campaign narrows to its critical path automatically,
with no idle spinning.

### 4.2 The integrator — merge back in topological order, loudly

```
  on run R reaching a CLEAN fixpoint (all units integrated, zero escalations):
    merge R.run-branch  ->  integration branch      (topological order, §2.2)
      conflict?           -> ESCALATE (campaign terminus, §2.4); do NOT force
    fold R's LessonLearned into the campaign lesson pool   (§4.3)
    mark R integrated; its dependents may now become READY
  on run R reaching an ESCALATED terminus:
    R stays escalated; every node that needs R (transitively) is BLOCKED, never started
```

A run integrates onto its OWN branch first (the conductor's existing per-unit integration, unchanged),
and only a CLEAN run's branch is merged up to the campaign. So the campaign integration branch only
ever advances by whole, reviewed, green runs — never by a partial or wedged one.

```
   run branches            integration branch (advances only by whole CLEAN runs)
   ---------------         -----------------------------------------
   branch_A  ───────┐
   branch_B  ─────┐ ├───►  A ─► B ─► C ─► D ─► E ─► ...   (topological order)
   branch_D  ───┐ │ │             each merge gated on its needs having merged;
   ...          │ │ │             a conflict here is a loud campaign escalation
```

### 4.3 Lesson reconciliation — decisions stay isolated, lessons cross-pollinate

Independent runs deliberately do NOT share a decision stream (they are independent by construction —
disjoint blast radius). But LessonLearned is durable cross-run knowledge, not run-local. At merge, a
run's lessons fold into the campaign's shared lesson pool, so a later dependent run — cut from the
post-merge tip — grounds on the lessons its predecessors learned. Decisions isolate for safety;
lessons reconcile for learning.

### 4.4 The adaptive fleet controller — informed and capable

The fleet bound of §4.1 is not a constant set once — it is the output of a control loop that runs
between scheduling decisions. The loop is deliberately simple: measure, compare to a headroom target,
adjust with hysteresis.

```
  between scheduling decisions:
    signals (INFORMED) ── host:  available memory · swap pressure · load average
                       └─ loop:  infra-classed liveness faults · target-contention false-reds ·
                                 build OOM-kills   (fault rate over the last window)
                                   │
                                   ▼
    policy: pressure high OR a hard fault    ->  DRAIN  (fleet -= 1; launch none until it clears)
            sustained headroom for K cycles  ->  FILL   (fleet += 1; up to the DAG's width)
            otherwise                        ->  HOLD
                                   │
                                   ▼
    lever (CAPABLE): the next scheduler pass launches to the NEW fleet bound
```

Hysteresis is the discipline: DRAIN is immediate on a hard signal (never wait while the host
thrashes), but FILL requires K consecutive headroom cycles (never oscillate by ramping into the gap a
just-finished run briefly vacated). The default policy is deterministic; where the cause of pressure
is ambiguous, the SAME signals and the SAME lever are surfaced to the orchestrating AGENT, which can
right-size by judgment or override the policy — informed by the measurements, capable through the one
lever. Adjusting the fleet changes only WHICH runs overlap and WHEN; it never changes the merge order
or the tree (§2.5).

### 4.5 Heal, do not escalate — an infrastructure fault right-sizes and retries

A hung or starved run is not a code defect, and the fault classification separates the two: a spawn
that misses its wall-clock is an INFRASTRUCTURE fault, distinct from a code or plan escalation. The
controller CONSUMES that classification instead of leaving it for a human:

```
   a run reaches a fault terminus ──► what CLASS?
        │
        ├─ infrastructure (hang / OOM / contention false-red)
        │        └─► DRAIN the fleet one step, then RE-DRIVE the run at reduced concurrency  (heal)
        │
        └─ code / plan defect
                 └─► ESCALATE loudly (unchanged, §2.4)
```

This is the self-healing the design turns on: an infrastructure fault — the exact failure a
resource-starved host produces — automatically right-sizes and re-drives, where a static fleet would
strand the run awaiting a manual re-drive. A genuine defect still escalates; only the infra class
heals. The classes must stay distinguishable so a real defect is never masked as a resource blip, so
an infra fault is retried a BOUNDED number of times and then escalated: a run that infra-faults even at
fleet 1 is not a resource problem, and the design refuses to loop on it forever.

---

## 5. Workstream C — driver currency

### 5.1 The currency guard — the driver is downstream of the base

```
  before a run starts on base B:
    installed-driver.provenance == B ?
      yes -> start the run
      no  -> rebuild the driver from B and reinstall it, THEN start
             rebuild fails? -> refuse the run LOUDLY (never drive on a stale binary)
```

Build-provenance is a PRECONDITION on starting a run: the driver's provenance must equal the run's
base, or the run does not start until a rebuild makes it so. A run is never driven by orchestration
older than the code its own worktree is cut from.

### 5.2 On base change — dogfood the predecessor

```
   run P integrates ────► integration tip moves to T_P
                         │
   run Q is READY next   │ Q needs P, so its base = T_P (Q builds ON P's changes)
                         ▼
   currency guard: driver.provenance != T_P  ->  rebuild driver from T_P, reinstall
                         │
                         ▼
   run Q is driven by a binary that INCLUDES P's just-landed changes
```

This is the whole point of currency at the campaign level: each run drives on the accumulated result
of every dependency it builds upon, so the loop compounds its own improvements instead of freezing at
the build that happened to be installed when the campaign began.

---

## 6. Worked example — an 8-spec campaign

A campaign of eight specs with two independent roots and one deep chain:

```
   DAG (needs edges)                          schedule (fleet = 3)
   ---------------------------------------    ------------------------------------------
   A ─► C                                     wave 0:  A ∥ B ∥ D     (3 independent roots)
   D ─► E ─► F ─► G ─► H                      wave 1:  C (after A) ∥ E (after D)
   B (independent)                            wave 2:  F (after E)
                                              wave 3:  G ─► H  (the deep tail, serial)

   critical path = D ─► E ─► F ─► G ─► H  (5 runs)
   sequential wall-clock = A+B+C+D+E+F+G+H   (8 runs, the sum)
   parallel  wall-clock ≈ max(critical path, fleet-limited head)
                        ≈ 5 runs, not 8   ->  ~1.6x faster at fleet 3, bounded by the chain
```

The speedup is bounded BY THE DAG, not by the fleet: a campaign that is one deep chain gets no
parallel benefit and correctly runs serial; a campaign that is wide (many independent roots) gets
close to a fleet-fold speedup on its head and then narrows to its tail. The design never PROMISES a
speedup it cannot deliver — it delivers exactly the DAG's available concurrency, and logs the
critical path so the bound is visible, never hidden (§1a is a measurement, not a surprise).

---

## 7. Delivery

Decomposed into atomic loop-ready specs, in dependency order:

1. **Driver currency guard** (§5) — smallest and independently valuable; makes the loop dogfood its
   own source on every run, standalone. Reuses the existing build-provenance signal (an advisory
   the driver already reports), promoting it from advisory to a start-of-run precondition. Ships
   first because a stale driver undermines every run, parallel or not.
2. **Per-run isolation context** (§3.2) — the worktree + run branch + target dir + run-scoped store
   for a SINGLE run, proven by running one spec in an isolated context that never touches the main
   worktree. Reuses the graph project-scoping mechanism for the store scope.
3. **The scheduler** (§4.1) — the DAG ready-set + fleet-bounded launch over isolation contexts,
   proven on a manifest with independent roots overlapping and a chain serializing.
4. **The integrator** (§4.2, §4.3) — topological merge-back with loud conflict escalation and lesson
   reconciliation; gated on 2 and 3.
5. **The adaptive fleet controller** (§4.4, §4.5) — the resource + fault signal loop feeding the fleet
   lever with hysteresis, plus the infrastructure-fault right-size-and-retry. Gated on the scheduler
   (3); turns the fleet from a static knob into a self-healing control variable. Ships last because
   correctness (isolation + merge) precedes optimization (adaptivity), but it is what makes the runner
   survive a resource-constrained host unattended.

Each spec ends with both feature lanes green. Because §5 lands first, every subsequent spec's own run
is itself driven by a current binary — the campaign runner is validated by running under the currency
guarantee it defines.

## 8. Acceptance (measured targets)

- A campaign whose DAG has K independent roots and fleet F runs min(K, F) specs CONCURRENTLY, each in
  a worktree/branch/target/store-scope disjoint from the others — proven by asserting no two running
  runs share any of the four (a two-project-style isolation fixture).
- The main working tree is NEVER written by a concurrent run — the integration branch advances only by
  merging a CLEAN run branch, and a merge conflict ESCALATES rather than corrupting the tree.
- Campaign wall-clock tracks the DAG CRITICAL PATH, not the sum — measured against a fixture whose
  critical path is a known fraction of its spec count.
- Every run is driven by a binary whose build-provenance equals the run's base commit — a drifted
  driver is rebuilt-or-refused before the run starts, never silently used.
- A dependent run grounds on its predecessors' lessons — a lesson emitted in run X is retrievable to a
  run Y that needs X, proven by a fixture.
- The schedule and the final integration tree are DETERMINISTIC across replays of the same manifest,
  independent of which run finishes first.
- Under induced memory pressure the fleet DRAINS (fewer concurrent runs launch) and RECOVERS when the
  pressure clears — measured against a fault-injection fixture — and the controller converges without
  oscillation (the FILL hysteresis holds).
- An INFRASTRUCTURE-classed run fault (hang / OOM / contention false-red) is automatically right-sized
  and RE-DRIVEN at reduced concurrency with no human intervention, while a CODE/plan defect still
  escalates; an infra fault that recurs down to fleet 1 escalates after a bounded number of retries.
- Outcome invariance under adaptivity: the same campaign run at fleet 1 and at fleet N produces a
  byte-identical integration tree — adaptivity changes timing, never the result.
