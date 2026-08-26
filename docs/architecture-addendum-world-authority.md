# Architecture addendum: the world authority

Status: PROPOSED - for operator review. This document merges and SUPERSEDES two prior
proposals (the resident conductor; the world reconciler), integrating the findings of a
five-lens adversarial design review. Everything below describes the TARGET state except
"Problem", which records the measured present.

## Problem

Rigger's event log is authoritative for decisions - runs replay, resume, and audit from
it alone - but its authority stops at the edge of the physical world, which is governed
by two weaker regimes at once:

1. **Process facts are smuggled through the filesystem.** The loop is re-entrant:
   hundreds of short-lived processes each reconstruct the world, act once, and exit.
   Nothing lives long enough to KNOW anything, so inter-invocation facts ride side
   channels - a lock file, a dashboard marker recording a port and pid, worktree
   existence as ownership, liveness inferred from file mtimes, and a `/proc` scan that
   guesses process ownership from working directories. Every channel is an independent
   failure class and every one has fired: a recycled pid pointed a kill at an innocent
   process (observed as far as a whole-session kill); a stale marker advertised a dead
   dashboard; mtime liveness reaped a live worker; two drivers stepped one project
   concurrently because nothing could refuse the second.
2. **Filesystem resources have no owner after creation.** Worktrees, per-unit and
   shared build caches, agent scratch, mutation-testing tree copies, tombstones, and
   backups are created at many call sites - most by DELEGATES (agents' own shells,
   cargo, git), not by rigger code - and cleaned by per-class heuristic reapers keyed
   on names and liveness guesses. Measured consequences: the project directory reached
   403G (40G worktrees carrying embedded build trees, 47G of leaked mutation scratch,
   an unreported 108G root cache); the store's volume hit 775KB free before any signal
   fired; a heuristic sweep deleted an escalated unit's worktree while the operator
   worked inside it, because the sweep could not know what the log knew.

Each incident got a mechanism; the inventory now includes a terminal-state sweep, an
orphan-scratch walk, per-spawn reclaims, a dash ensure/self-reap pair, marker-staleness
watchdogs, residue advisories, and operator-side polling loops - every one re-deriving,
by inference, facts that either the log or a parent process could hold exactly. The
defect class is singular: NOTHING OWNS THE WORLD. Process truth needs an owner that
lives; resource truth needs an owner that derives; today both are inferred.

## The principle

One architecture, two authorities, one resident owner:

- **Parentage governs processes.** A resident conductor daemon parents every subprocess
  the loop needs. A parent holds unforgeable identity over its children: exits are
  delivered to it, pids cannot recycle out from under it before it reaps them, ending a
  child is a typed act on a handle it minted. The loop may only signal a process it
  parented and still holds, or one whose recorded `(pid, start-time)` identity it has
  just re-verified. It never signals a guess.
- **The log governs everything else.** The set of non-process resources that should
  exist is DERIVED from the event log by a pure fold - never stored, never guessed from
  names or mtimes - and one reconciler, running as the daemon's internal control loop,
  continuously converges the actual world toward it.
- **The command line is a client.** Mutation flows through the daemon; CLI invocations
  query live state over a socket or degrade to read-only store access when the daemon
  is down. Destructive convergence happens in exactly one process, by construction.

```
             +--------------------------------------------------------------+
             |            RESIDENT CONDUCTOR DAEMON (one per project)        |
             |  singleton by atomic socket bind (.rigger/daemon.sock)        |
             |                                                              |
             |  +--------------------+      +--------------------------+    |
             |  |  CONDUCTOR LOOP    |      |  CHILD TABLE + LEDGER    |    |
             |  |  (the run: event-  |      |  every subprocess is a   |    |
             |  |  sourced fold,     |      |  registered child:       |    |
             |  |  step = internal   |      |  (handle, spawn id,      |    |
             |  |  tick)             |      |  role, group,            |    |
             |  +---------+----------+      |  pid + start-time)       |    |
             |            |                 +------------+-------------+    |
             |            v                              |                  |
             |  +--------------------+          spawns / signals / reaps    |
             |  |  WORLD RECONCILER  |                   |                  |
             |  |  desired = fold of |            gates, builds,            |
             |  |  the log; diff ->  |            agents' heavy work,       |
             |  |  converge | notify |            the dashboard             |
             |  +--------------------+                                      |
             +---------------+----------------------------------------------+
                             | unix socket (line-delimited JSON)
        +--------------------+--------------------+
        |                    |                    |
   rigger CLI          workflow couriers      turn-boundary hook
   (status, watch,     (agent runtime,        (reads judgment
   result, emit -      unchanged - clients    anomalies into the
   queries and         that fetch parked      orchestrator's
   requests; offline   work and record        context)
   reads degrade to    results through
   the store)          the socket)
```

Why this shape: a reconciler is indifferent to HOW the world diverged - crash, kill,
operator error, a bug in rigger itself - because it reasons only about the difference
between what is and what should be; every future failure mode in the resource domain is
covered the day it occurs, not the day it is diagnosed. And a resident parent is the
only sound substrate for the reconciler's hardest arms: it gives process identity
without inference, serializes destructive convergence in one process without a new
lock, holds the incremental fold cursor and notification dedup state that stateless
invocations structurally cannot, and provides the cadence floor that polling designs
borrow circularly from the resources they manage.

## The runtime: the resident conductor

**Singleton and lease.** Binding `.rigger/daemon.sock` is atomic: a second daemon fails
to bind and exits naming the live one - driver exclusivity is a bind, not a lease
protocol. A stale socket (crashed daemon) is a failed connect, safely replaced. No lock
file is ever broken: a held OS lock is treated as a live holder everywhere in this
design, and "lapsed-but-held" is a notification, never a clear. For the shared-server
store backend (multi-machine), exclusivity additionally carries a FENCING EPOCH: each
daemon start appends an epoch acquisition via compare-and-swap on the stream head, and
every subsequent daemon write carries its epoch; a superseded daemon's next write fails
on its own, whatever its socket or clock believes.

**The child table and process ledger.** Every subprocess is registered at fork: in
memory, the `Child` handle, spawn id, role, and the process group minted at spawn; on
disk (machine-local ledger), `(pid, start-time, spawn id, role, group)` written before
the child runs and retired at reap. `(pid, start-time)` - start time from
`/proc/<pid>/stat` - is recycling-proof identity. The daemon sets
`PR_SET_CHILD_SUBREAPER` so double-forked children reparent to it and stay attributed.
Ending work is: look up the handle, signal the group through a typed syscall wrapper
(TERM, bounded grace, KILL), reap, retire the row. No shell `kill`, no argv, no string
in the signal path. At daemon start, the ledger is reconciled: rows whose
`(pid, start-time)` matches a live process are adopted or ended per role; mismatched or
gone rows are retired untouched. The `/proc` cwd scan is DEMOTED permanently to a
`validate` advisory - nothing in the binary signals a scanned process.

**Liveness.** Children heartbeat over pipes to the parent; the daemon enforces each
spawn's wall-clock bound on its own timer. A liveness fault is recorded as today - but
the fault's resource consequences are gated by the child table: a spawn whose process
the daemon still holds live is NEVER treated as resource-terminal, whatever the log's
last recorded result says. Parentage is the live-worker veto, replacing every mtime and
heuristic liveness check.

**The dashboard** is daemon-owned (an internal task or a registered child). Its
liveness IS daemon state; `rigger status` asks the owner. Marker files, stale-marker
diagnosis, and the self-reap protocol are deleted, not fixed.

**The CLI as client.** `status`/`watch` are socket queries answered from live state;
`progress`/`result`/`emit` are socket requests the daemon appends (couriers unchanged
as agent runtime - they gain a live authority and lose every filesystem side channel);
`run` starts or submits to the daemon; `step` survives as an internal tick plus a
foreground `--once` for tests; `validate` is read-only and works offline; `reset` is
offline-only and refused (naming the daemon) while one is live. The daemon should run
detached from login sessions (setup offers a user-service unit): a session teardown
never kills a run, and the daemon never signals outside its own tree - both directions
of the historical blast radius closed by one boundary.

## The resource model

A RESOURCE is anything rigger causes to exist outside the log. The model is a CLASS
REGISTRY IN CODE - one enum, one derivation per variant; the table below is generated
from it, and an enforcement test fails any `create_dir_all`/`File::create`/spawn site
whose path no class claims. Adding a class means adding a variant; the table cannot
drift from the code because the code is the table.

**Identity.** Derivable classes - those whose path is a pure function of
`(project, run id, owner id)` through the one shared injective encoding - carry NO
declaration: the log already names them, which is also what lets the reconciler govern
resources created before this design ships. Only non-derivable identities (an allocated
port, an operator-relocated root, a delegate-chosen path) are declared, as metadata on
the owner's existing lifecycle events - no new event type. Declarations record ABSOLUTE
RESOLVED paths, never recipes: the observation set is the union of every root ever
declared, so relocating `RIGGER_TMPDIR` makes the old root a drain-then-remove case,
not an invisible leak.

**Containers, not leaves.** Rigger declares container roots - a worktree, a per-unit
cache, a per-spawn scratch dir, a per-spawn mutation-scratch dir - and DELEGATES fill
them: agents' shells, cargo, git. Every spawn's environment pins `TMPDIR`,
`CARGO_TARGET_DIR`, and cache homes INSIDE its containers, so delegate output is
governed by containment rather than enumeration, and the reap acts subtree-wide.
Creation is therefore two-mode: DIRECT (rigger creates; declaration and creation are
one logical act) and DELEGATED (rigger declares and pins the environment; the delegate
materializes; declared-but-absent is the normal steady state, not a crash artifact). A
delegate writing OUTSIDE its container is a detected anomaly, not silent residue.

**Scope.** Every class is project-, machine-, or user-scoped. The project world derives
from the project log. The machine world (dashboard singletons, build-budget slots, the
shared compilation cache, the instance registry itself) derives from the machine-scoped
instance registry, which is promoted to a first-class declaration substrate; a project
daemon OBSERVES machine-scoped resources and may converge only those the machine
substrate assigns it. Cross-project collisions (two projects, one machine) are resolved
by scope, never by whichever reconciler runs last. Machine-scoped paths move out of
world-writable `/tmp` into per-user 0700 roots.

**The derivation.** `desired_world(log, now, observations) -> ResourceSet` is a
deterministic function of its explicit inputs - `now` and measured sizes are ARGUMENTS,
so log-pure rows stay replayable at any position and clock-or-size-governed rows are
testable by injection. Folding is keyed by `(run id, owner id, attempt)` - never bare
owner ids, which recur across runs - and reads incrementally from a daemon-held
checkpoint cursor (a cache, discarded whenever its position is absent, never itself an
input to deletion without full re-derivation) over type-indexed reads. Two structural
events complete the derivation: a RUN-TERMINAL fact (appended when a run reaches
fixpoint or is superseded) so the last run's resources do not stay desired forever, and
the terminus rule "the LAST event for an identity is a result" (results are
last-write-wins; a bare "a result exists" is not well-defined). Spawns that can never
self-report (died pre-marker, unbounded wall clocks) get a declared deadline the daemon
enforces, so no terminus can simply never arrive.

**Compaction invariant.** No store prune may remove a declaration-bearing or
lifecycle-terminal event: declaration-bearing types and compactable types are DISJOINT,
pinned by a test. `reset` runs reconcile-to-quiescence before pruning, never a pass on
entry.

## The reconciler: five arms, five rails

Runs as the daemon's internal loop on its own cadence; every CLI invocation computes
the same diff OBSERVE-ONLY (reporting, never converging) and submits judgment items
over the socket. Arms, in order (reclaim before create):

1. **REAP** present-but-undesired. Uniform on the derivation, with two vetoes that are
   facts rather than heuristics: the child table (a live parented process in the
   subtree blocks the reap) and presence holds (below). Deletion follows the
   GIT-QUARANTINE rule - see below - so every reap of unique content is reversible.
2. **REPAIR** present-but-divergent. Each class carries an integrity predicate beside
   its identity: a registered-but-absent worktree is recreated from its branch; a
   zero-length git admin entry is healed; a bare leftover dir where a worktree should
   be is adopted or cleared. This absorbs the shipped self-healing-worktree guarantee
   as an arm instead of losing it.
3. **CONVERGE ENVELOPES.** Size-governed classes carry an eviction terminus (LRU
   within the class to a declared floor) so the arm always has a convergent action;
   only when eviction to the floor still leaves the envelope breached does refusal
   engage - and refusal has TWO enforcement points: the creation authority declines
   new DIRECT creations, and ADMISSION declines parking new spawns (the only lever
   that reaches delegate-produced bytes), both naming the reclaiming command. Refusal
   is scoped to the over-budget class, never the whole project.
4. **CREATE** absent-but-desired positional resources (the dashboard; the socket
   structure). Owned resources are created by their owners' flows.
5. **NOTIFY** the unconvergeable. Arm 5 IS the existing watch detector extended with
   envelope and world-diff signals - not a second detector. Dedup state lives in the
   daemon's memory (preserving the settled in-process-dedup decision, now with a
   process that actually persists across observations); the file surface
   (`anomalies.jsonl`) is a REBUILDABLE PROJECTION of open anomalies -
   truncate-and-rewrite, bounded, itself a registered size-governed resource - read by
   the dash, `watch --follow`, the setup-installed turn-boundary hook (injecting
   judgment diffs into an orchestrator's context, delimited as untrusted data), and an
   exec-without-shell `notify:` command fed JSON on stdin.

Rails, in priority over every arm:

- **Three-tier action rail.** DECLARED-or-DERIVABLE: converge. GRAMMAR-RECOGNIZED (a
  path matching a class's grammar under a governed root whose owner key names no
  desired owner): converge via git-quarantine. FOREIGN (nothing claims it): report
  only. The rail is what makes "log-derived" compatible with governing a world that
  predates the design.
- **One path authority.** Every path an arm touches is constructed by a single
  `confine()` function: lstat (a symlink leaf is refused, arm-5), canonicalized
  component-wise containment under a validated root, same-device check. Roots are
  resolved once per pass and validated (absolute, existing, never `/`, `$HOME`, the
  repo root, or an ancestor of any). No arm can name a path that did not pass.
- **One safe action.** An arm exists only where the correct response is unique;
  anything else is arm 5 by definition. Deleting is only "one safe action" because
  quarantine makes it reversible.
- **Convergent from any partial state.** Multi-step physical acts (worktree = dir +
  git admin entry + branch) specify their order and every interruption lands in a
  state arm 2 repairs. "Atomic" is claimed only of single acts.
- **Bounded blast radius.** A pass whose REAP set exceeds an absolute or
  relative-to-desired threshold refuses to converge and routes the batch to arm 5.
  `reconcile --explain` (the dry-run diff) is a permanent surface, not migration
  scaffolding. Every convergence appends a compact reclaimed-fact so 40G never
  disappears silently.

**Git-quarantine (the deletion discipline).** Residue splits by recreatability, and
each half has a native lifecycle: REBUILDABLE content (build caches, mutation scratch,
target trees - the bulk by bytes) is deleted outright, its recovery path being the cold
rebuild that defines a cache; UNIQUE content (any tree holding uncommitted tracked or
untracked source) is COMMITTED to a `rigger/quarantine/<owner>-<date>` ref before its
tree is deleted - content-addressed, compressed, cache-free (ignored paths are not
committed), recoverable by one checkout, expiring under git's own gc and reflog
conventions rather than a bespoke window. The rule everywhere: delete what can be
rebuilt; commit what cannot; never hold raw trees in limbo.

**Escalation holds no disk.** An escalated unit's worktree is purged at terminal like
any other - the purge is preceded by the same unique-content snapshot every purge gets,
and the unit branch remains the durable working base. Remedy work happens in checkouts
the operator creates, which the rail classifies as FOREIGN and never touches; an
operator entering a LIVE rigger worktree announces presence with `rigger hold <path>`
(a log-visible presence marker arm 1 respects), the same primitive agents' liveness
already uses - a log fact, not a process heuristic.

**Branches are NOTIFY-only.** The branch is what makes every worktree deletion safe;
no arm deletes one. Dead-branch accumulation is a reported anomaly carrying the
delete command.

## What this deletes and what becomes impossible

Retired into arms (behavior preserved, independent existence gone): the terminal-state
sweep and its ensure-on-park half (arms 1+2), the orphan-scratch walk and per-spawn
reclaim call sites (arm 1, three-tier), worktree self-healing (arm 2), residue
advisories (arm 5, partly - branch advisories stay), dash ensure/self-reap and every
marker file (daemon ownership), marker-staleness watchdogs (pipe liveness), the
`/proc` kill path (child table; scan demoted to advisory), stateless `watch` detection
(arm 5 reader). Made impossible rather than handled: a second driver (socket bind +
fencing epoch), signaling a stranger (parentage + verified identity), disk exhaustion
by rigger's writes (eviction + dual-point refusal), sweeping a surface a human holds
(FOREIGN tier + presence holds), unowned scratch of any future kind (class registry +
container pinning), silent mass deletion (blast-radius rail + reclaimed-facts).
Consumer-side monitoring reduces to reading judgment anomalies; the acceptance
criteria below make "no hand-rolled monitor" mechanical.

## Delivery

Class-by-class, never arm-by-arm - each class migrates in one change (implement the
handler, replace the old mechanism's BODY with delegation, re-point its tests, delete
the symbol), so no class ever has two owners or none. Mixed-version safety: a store
minimum-version marker makes an old binary refuse its heuristic reapers against a
governed project.

1. **Pure domain + ports.** The class registry; `desired_world` as a tested fold;
   `diff -> Plan`; `confine()`; a `Clock` port joining the existing seams; the
   `(run, owner, attempt)` keying; the run-terminal event; the compaction disjointness
   test. Ends with `validate --world-diff` (observe-only) proving the derivation
   against real history, plus `--adopt` for legacy shapes.
2. **Process authority.** Typed signal wrapper, `(pid, start-time)` identity
   everywhere, groups at spawn, subreaper, the ledger with crash reconciliation -
   consumer-safe before the daemon exists, removing signal-a-stranger while the step
   model still runs.
3. **The resident daemon.** Socket singleton, conductor loop as tick, child table,
   owned dashboard, pipe liveness, CLI/courier as clients, single-writer store,
   fencing epoch on the server backend.
4. **Reconciler arms in-daemon,** class by class per the registry, with the
   blast-radius rail and `--explain` from the first class onward; git-quarantine;
   envelopes with eviction + admission; anomalies projection + hook + `notify:`.

## Acceptance (mechanical, each testable)

1. The shipped driver, skills, and setup surfaces contain no polling loop and no disk
   guard (pinned in the architecture-integrity test family).
2. Chaos: SIGKILL the daemon mid-wave; one invocation of any command later, every
   undesired resource is gone, every desired one survives, and the ledger reconciles
   its survivors - asserted for worktrees, caches, scratch, processes, dashboard.
3. A second daemon exits naming the live one; a superseded daemon's next store write
   fails on its epoch.
4. At the envelope: eviction to the floor happens first; admission refuses new spawns
   naming the reclaim command; no mid-flight deletion of a live build's inputs.
5. After a full campaign, `validate --world-diff` reports empty modulo FOREIGN.
6. A `rigger hold` path and a FOREIGN-named checkout survive every pass; a purged
   escalated worktree's unique content is recoverable by one checkout of its ref.
7. Every convergence that removed bytes has a reclaimed-fact in the log naming class,
   owner, and size.
