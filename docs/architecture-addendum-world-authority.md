# Architecture addendum: the world authority

Status: PROPOSED - for operator review. This document (v3) merges and SUPERSEDES two prior
proposals (the resident conductor; the world reconciler) and integrates two rounds of
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
   concurrently.
2. **Filesystem resources have no owner after creation.** Worktrees, per-unit and
   shared build caches, agent scratch, mutation-testing tree copies, and backups are
   created at many call sites - most by DELEGATES (agents' own shells, cargo, git), not
   by rigger code - and cleaned by per-class heuristic reapers keyed on names and
   liveness guesses. Measured consequences on this project: the directory reached 403G
   (40G worktrees carrying embedded build trees, 47G leaked mutation scratch, an
   unreported 108G root cache); the store's volume hit 775KB free before any signal
   fired; a heuristic sweep deleted an escalated unit's worktree while the operator
   worked inside it, because the sweep could not know what the log knew.

The defect class is singular: NOTHING OWNS THE WORLD. Process truth needs an owner that
lives; resource truth needs an owner that derives; today both are inferred. And the two
regimes interact: the fix that made the loop re-entrant safe (short-lived processes,
crash-resumable from the log) is exactly what forbids any process from holding the
authoritative view of what it created.

## The principle

One architecture, two authorities, one resident owner - with one rule that the first
review round missed and this version makes structural: **the log's authority over the
world is only as trustworthy as the log's own writes.** Because agents hold
`Bash(rigger:*)` and can append events, a design that DERIVES delete-and-kill authority
from the log must separate what agents may ASSERT from what CONSTITUTES a resource's
existence or terminus.

- **Parentage governs processes.** A resident conductor daemon parents every subprocess
  the loop forks. A parent holds unforgeable identity over its children: exits are
  delivered to it, pids cannot recycle before it reaps them, ending a child is a typed
  act on a handle it minted. The loop signals only a process it parented and still
  holds, or one whose recorded `(pid, start-time, boot-id)` identity it has just
  re-verified. It never signals a guess. Processes the daemon did NOT fork - the
  harness-parented agent turns - are governed by CONTAINMENT and by their own live
  socket claim, never by a signal.
- **The log governs everything else, split into two surfaces.** A DECLARATION surface,
  appendable only by the daemon under exclusivity, constitutes which resources should
  exist and when their terminus fires. An OBSERVATION surface (agent `emit`, `result`,
  `progress`, review findings) can PROPOSE and inform, never CONSTITUTE. `desired_world`
  folds the declaration surface; a forged observation can no longer manufacture a
  desired resource or supersede a live owner.
- **The command line is a client.** Mutation flows through the daemon; CLI invocations
  query live state or compute an OBSERVE-ONLY diff, never converge. Destructive
  convergence happens in exactly one process, holding one lease, by construction.

```
             +--------------------------------------------------------------+
             |            RESIDENT CONDUCTOR DAEMON (one per project store)  |
             |  singleton by a held flock on .rigger/daemon.lock             |
             |  (the socket is a rendezvous, never the claim)                |
             |                                                              |
             |  +--------------------+      +--------------------------+    |
             |  |  CONDUCTOR LOOP    |      |  CHILD TABLE + LEDGER    |    |
             |  |  (the run; step =  |      |  every FORKED child:     |    |
             |  |  internal tick     |      |  handle, spawn id, role, |    |
             |  |  that appends      |      |  cgroup, (pid,start,     |    |
             |  |  SpawnResult)      |      |  boot-id) + capability    |    |
             |  +---------+----------+      +------------+-------------+    |
             |            v                              |                  |
             |  +--------------------+          spawns / signals / reaps    |
             |  |  WORLD RECONCILER  |          (only its own cgroups)      |
             |  |  desired = fold of |                   |                  |
             |  |  the DECLARATION   |            gates, builds,            |
             |  |  surface; 5 arms   |            the dashboard             |
             |  +--------------------+                                      |
             +---------------+----------------------------------------------+
                             | unix socket (0700 dir, 0600 sock, peer-cred verified)
        +--------------------+--------------------+
        |                    |                    |
   rigger CLI          workflow couriers      turn-boundary hook
   (queries + observe-  (agent runtime -      (reads judgment
   only diffs; writes   clients that hold a   anomalies as
   via socket or a      live socket CLAIM     UNTRUSTED data
   durable outbox)      on their spawn +      into the
                        heartbeat progress)   orchestrator)
```

## The runtime: the resident conductor

**Exclusivity is a held flock, never a socket.** The daemon holds `LOCK_EX` on
`.rigger/daemon.lock` for its entire life - the kernel releases it on any death, clean or
killed, which is the property the existing step lock already relies on. The socket
(`.rigger/daemon.sock`) is bound only after the lock is held, and is a rendezvous, not a
claim: a daemon finding the lock FREE but a socket present unlinks and rebinds; a daemon
finding the lock HELD exits naming the holder, whatever the socket looks like. A "failed
connect" is never treated as licence to replace a claim. The daemon re-`stat`s its own
socket inode on its cadence and halts destructive arms if it no longer owns it.

**The store is the deeper authority: a fencing epoch, on every backend.** Each daemon
start acquires an epoch by `append(epoch-stream, ExpectedRevision::Exact(n),
[EpochAcquired])` - the same compare-and-swap the store already implements and tests on
BOTH the sqlite and server backends. Every daemon write carries its epoch, and every
DESTRUCTIVE reconciler arm re-asserts it with a CAS append immediately before acting
(this IS the compare-and-act the first review found missing). A superseded daemon's next
write fails on its own conflict, so exclusivity holds even where a socket or a clock
lies. To make the CAS enforceable rather than advisory, the daemon's own writes are
confined to one epoch-fenced stream; per-unit and per-spawn streams become projections of
it, and no process other than the daemon (save the fenced-gate exception below) appends to
that stream.

**Singleton scope is the store, not the cwd.** The lock, socket, and epoch are resolved
through the SAME bounded-walk authority the store already uses (outermost `.rigger`,
`main_repo_root`, refuse-to-fabricate), keyed by the store's `(device, inode)` - so a
courier inside a worktree, a symlinked repo path, or a second mount namespace can never
mint a second daemon by resolving a nearer socket.

**The child table and ledger govern forked processes.** Every subprocess the daemon forks
is registered: in memory (the `Child` handle, spawn id, role, its own cgroup v2 subtree,
and a per-spawn CAPABILITY TOKEN handed to the child on an inherited fd - never argv); on
disk (a machine-local ledger row `(pid, start-time, boot-id, spawn id, role, cgroup)`,
HMAC'd under a key held only in daemon memory and re-minted each start, opened
`O_NOFOLLOW` inside the 0700 root). A spawn's process SET is its cgroup membership, which a
descendant cannot leave - so a double-forked or `setsid` process is still attributed, and
`PR_SET_CHILD_SUBREAPER` is a backstop, not the authority. Ending work signals the CGROUP,
not a pid from a file, through a typed signal PORT (see lanes below). At daemon start the
ledger is reconciled: for a row whose HMAC verifies AND whose `(pid, start-time, boot-id)`
matches a live process, the daemon adopts or ends per role AND appends a `SpawnResult`/
`SpawnAbandoned` terminus so the LOG reflects the outcome; every other row is retired
untouched, and a row that fails HMAC or names a foreign process is never a signal - only,
at most, an arm-5 anomaly. The `/proc` cwd scan is DEMOTED to a read-only VETO input and a
`validate` advisory; nothing in the binary signals a scanned process.

**Liveness has two channels, both facts.** A forked child heartbeats over its pipe and is
bounded by the daemon's timer. An AGENT TURN - run by a courier the daemon did not fork -
holds a live socket CLAIM on its spawn for the turn's duration and heartbeats via
`rigger progress` (already a client verb); its worktree's liveness is that claim plus the
timestamp of its last progress, never a marker mtime. Arm 1's live-worker veto consults
BOTH the child table (forked work) and open socket claims (agent turns), plus the
read-only `processes_rooted_under` scan as a third corroborating veto - so the process
most likely to be mid-edit in a worktree, the agent's own cargo/git, can never be reaped
under. A liveness FAULT records the fault but routes the resource consequence through arm
5 for one cycle before any reap, so a false-positive timeout never becomes an irreversible
kill.

**The dashboard** is daemon-owned; its liveness IS daemon state. Marker files, stale-marker
diagnosis, and the self-reap protocol are deleted, not fixed.

**The CLI as client.** `status`/`watch` are socket queries. `progress`/`result`/`emit` are
socket requests the daemon appends - but a client that finds the daemon unreachable writes
its request to a DURABLE OUTBOX inside its own already-declared per-spawn container (one
line, idempotency-keyed), which the daemon drains on connect and every tick; so a worker
finishing during a daemon outage never loses its result. Every socket request carries a
client-minted idempotency key and is applied through the store's existing
`record_result_if_absent` CAS, never a blind `Any` append. `run` starts or submits to the
daemon; `wave --pull` is the named verb couriers use to fetch parked work (the tick is
park-only and never spawns agents). `step --once` is retained for tests but takes the SAME
lease as the daemon - it IS the daemon for its duration, never a second converger. A FENCED
gate (`RIGGER_STORE_FENCE_DIR` set) does NOT connect to the daemon at all: it opens its
fenced store directly, so a unit's own test suite still cannot write the live run stream -
the one control the socket would otherwise dissolve. `reset` asks the live daemon to
reconcile to quiescence and hold convergence for the prune, refusing (via the held lock,
never a socket probe) only if the daemon declines or run state is non-quiescent;
`--force-live` survives. The daemon runs detached from login sessions only where a working,
lingering-enabled supervisor is VERIFIED present - at start the daemon probes for one and
reports its supervision posture (`systemd-user(lingering)` / `unsupervised` / `unknown`)
through `rigger status`, so the session-independence guarantee is a checked fact, never a
silent assumption.

## The resource model

A RESOURCE is anything rigger causes to exist outside the log. The model is a CLASS
REGISTRY IN CODE - one enum, one derivation per variant - and the table is generated from
it. The enforcement test keys on PATH AUTHORITIES, not syscalls: every variant names the
pure path function that derives it (the eight existing single-authority functions -
`unit_worktree_dir`, `unit_cache_sibling`, `spawn_scratch_path`, `mutation_scratch_path`,
`liveness::marker_path`, `review_fence_sibling`, `gate::default_cache_dir`,
`budget::default_slot_dir`, and their peers), and the test fails any production
path-deriving function no variant names, and any `Command::new`/`create_dir_all` whose path
does not originate in one. A regex over call sites could pass while the 403G recurred; a
path-authority key cannot.

**Identity.** A DERIVABLE class - path a pure function of `(project, run id, owner id,
attempt)` through the one shared injective encoding - carries NO declaration; the log
already names it, which is what lets the reconciler govern resources created before this
design shipped. Keying by `(run, owner, attempt)` - never a bare owner id, which recurs
across runs - is mandatory: 208 spawn ids recur across the live log's runs, and a bare-id
fold reopens the cross-run replay defect the project already closed. Only NON-DERIVABLE
identities (an allocated port, an operator-relocated root, a delegate-chosen path) are
declared, as metadata on the owner's existing lifecycle events - no new event type - and a
declaration records the ABSOLUTE RESOLVED path plus its `(device, inode)`, so the
observation set is the union of every root ever declared and a remounted or reused root is
detected rather than converged.

**Containers, not leaves.** Rigger declares container ROOTS; DELEGATES fill them. Every
spawn's environment pins `TMPDIR`, `CARGO_TARGET_DIR`, and cache homes INSIDE its
containers - and where the workflow `agent()` primitive cannot pass env (its known limit),
the agent obtains its container from `rigger scratch <spawn>` (shipped in spec 77) and the
SCRATCH POLICY directs all agent-created scratch and manual cargo there. Creation is
two-mode: DIRECT (rigger creates; declaration and creation are one act) and DELEGATED
(rigger declares and pins/instructs; the delegate materializes; declared-but-absent is the
normal steady state, and a convergent handler exists per class - recreate from a branch
where idempotent, mark the owner failed where not). A delegate writing OUTSIDE its
container is a detected anomaly, never silent residue.

**Scope.** Every class is project-, machine-, or user-scoped. The project world derives
from the project log; the machine world (dashboard singletons, build-budget slots, the
shared compilation cache, quarantine repos) derives from a machine-scoped substrate. That
substrate is NOT today's instance registry, whose contract is "never a source of truth,
loss is harmless" and which PRUNES rows as a read side effect: machine-scoped ASSIGNMENT
moves to a separate daemon-written store in the 0700 root, each assignment proven by a HELD
flock on the resource's own slot file (forgeable JSON can never grant it), carrying the
owning daemon's epoch and identity; no reader ever deletes another daemon's row. Two
projects on one machine resolve by scope and held lock, never by whichever reconciler ran
last. Machine paths move out of world-writable `/tmp` into per-user 0700 roots created with
an explicit mode and verified owner-owned, not merely `create_dir_all`'d.

**The derivation.** `desired_world(log, now, observations) -> ResourceSet` is deterministic
in its EXPLICIT inputs - `now` and measured sizes are arguments, so log-pure rows stay
replayable at any position and clock/size-governed rows are testable by injection. A
RUN-TERMINAL fact (appended when a run reaches fixpoint or is superseded) stops the last
run's resources staying desired forever; the terminus rule is "the LAST event for an
identity is a result", and the in-process driver MUST append `SpawnResult` before
delivering on its channel (today it does not - the channel is a wakeup, but the log must be
the record, or a daemon-tick run leaves every spawn desired forever). Reads are incremental
from a daemon-held checkpoint cursor over type-indexed streams - but the cursor is PROCESS
MEMORY only (a daemon start always full-derives), and arm 1 may act ONLY against a
`ResourceSet` produced by a full re-derivation performed at least once in the current
daemon lifetime; incremental folding advances the observe/notify arms alone. No prune may
remove a declaration-bearing or lifecycle-terminal event (declaration-bearing types and
compactable types are disjoint, pinned by test).

## The reconciler: five arms, five rails

Runs as the daemon's internal loop OFF the socket-serving path (a slow re-derivation never
blocks a courier's result), on its own timer; every CLI invocation computes the same diff
OBSERVE-ONLY and submits judgment items over the socket. Arms, in order:

1. **REAP** present-but-undesired, with three vetoes that are facts not heuristics: the
   child table, open socket claims, and the read-only cwd scan. Deletion follows the
   git-quarantine rule below, so every reap of unique content is reversible.
2. **REPAIR** present-but-divergent - each class carries an integrity predicate;
   registered-but-absent worktrees recreate from their branch, zero-length git admin
   entries heal, bare leftover dirs adopt-or-clear. Absorbs the shipped self-healing
   guarantee. A class is EITHER repairable (arm 2 owns it, arm 3 may not evict it) OR
   evictable (arm 3 owns it, arm 2 may not recreate it) - never both, so arm 2 cannot
   recreate what arm 3 just evicted (the envelope livelock).
3. **CONVERGE ENVELOPES** - size-governed classes carry an LRU-to-floor eviction terminus
   so the arm always has a convergent action; refusal engages only when eviction to the
   floor still breaches, is scoped to the over-budget class, and fires at two points: the
   creation authority (DIRECT) and pre-spawn ADMISSION (the only lever that reaches
   delegate-produced bytes). Per-class byte accounting is maintained incrementally at
   create and reclaim (reclaimed-facts carry sizes); a full non-symlink-following,
   depth-and-inode-bounded walk runs only when `statvfs` on the device crosses a floor,
   and "could not measure" is arm 5, never "under budget".
4. **CREATE** absent-but-desired positional resources (dashboard, socket structure).
5. **NOTIFY** the unconvergeable. Arm 5 IS the existing `watch::detect` over its closed
   `Signal` enum, extended (enum + generated skill body + pins together) with envelope
   and world-diff signals - detection stays STATELESS and is computed by a NON-DAEMON
   reader folding log + world + daemon-liveness, so the daemon's OWN death is a signal a
   dead daemon cannot suppress. The `anomalies.jsonl` file is a rebuildable projection of
   open anomalies, written temp-then-rename (atomic), stamped with the daemon's epoch and
   the position it derived at; readers cursor by the anomaly's stable identity, never a
   byte offset, so a rewrite never desyncs a tailer. Anomaly fields are a fixed enum of
   kinds with typed, length-capped operands; free text is escaped, never interpolated
   into a rendered sentence or a shell (`notify:` execs argv-only, JSON on stdin), because
   the turn-boundary hook feeds an orchestrator holding `Bash(rigger:*)`.

Rails, in priority over every arm:

- **Three-tier action rail.** DECLARED-or-DERIVABLE: converge. GRAMMAR-RECOGNIZED (a path
  matching a class's path-authority grammar under a governed root whose owner key names
  no desired owner): converge via git-quarantine. FOREIGN (nothing claims it): report
  only. This is what makes "log-derived" compatible with a world that predates the design
  and stops recognized residue becoming a permanent leak.
- **One path authority.** Every path an arm touches is a `ConfinedPath` NEWTYPE that only
  `confine()` can construct: lstat (a symlink leaf is refused), canonicalized
  component-wise containment under a validated root, same-device check. Every destructive
  method takes only `ConfinedPath`, so the containment invariant is a compile-time fact,
  not a review rule - the review rule is what produced 403G.
- **One safe action, and destructive arms hold the lease.** An arm exists only where the
  correct response is unique; arms 1-3 run only from the lease-holding daemon or a
  `--once` that took the lease, and re-assert the epoch by CAS immediately before acting.
- **Convergent from any partial state.** Multi-step physical acts specify order and land
  every interruption in a state arm 2 repairs; "atomic" is claimed only of single acts.
- **Bounded blast radius.** A pass whose REAP set exceeds an absolute or relative-to-
  desired threshold refuses and routes the batch to arm 5; a derivation yielding zero
  owners for a project with governed paths on disk is an anomaly, never a reap set.
  `reconcile --explain` (dry-run) is permanent; every convergence appends a reclaimed-fact
  so 40G never disappears silently. A crash-loop breaker enters degraded diagnose-only
  mode after N restarts in window W.

## Git-quarantine: the retention discipline

The operator-ratified rule: **git is the retention system; disk holds only what's live.**
Residue splits by recreatability. REBUILDABLE content (build caches, mutation scratch,
target trees - the bulk by bytes) is deleted outright; its recovery is the cold rebuild
that defines a cache. UNIQUE content (any tree holding uncommitted tracked or untracked
source) is COMMITTED before its tree is deleted - but with PLUMBING in a scrubbed
environment (`-c core.hooksPath=/dev/null -c core.fsmonitor=false`,
`GIT_CONFIG_GLOBAL=/dev/null`, `hash-object`/`mktree`/`commit-tree`), and into a DEDICATED
BARE quarantine repository outside the project's object store, so untrusted agent content
never runs a hook at the daemon's privilege, never enters the repo that gets pushed, and
never re-arms a gate on checkout (`.rigger/` and `.claude/` paths are ineligible and
reported instead). Quarantine refs are a size-governed class under arm 3 with a declared
retention window and LRU eviction (real deletion + `gc`), never "expiring under gc
conventions" that a named ref does not obey.

**Escalation holds no disk.** An escalated unit's worktree is purged at terminal like any
other - the purge is preceded by the unique-content snapshot every purge gets, and the unit
branch is the durable working base. Remedy work happens on operator-created checkouts,
which the rail classifies FOREIGN and never touches; an operator entering ANY governed
worktree announces presence with `rigger hold <path>` - scoped by `confine()` to a single
container (never a repo root), owned by its taker, carrying a required TTL after which it
degrades to arm 5 rather than remaining a veto, and appendable through the outbox when the
daemon is down (the one mutation permitted directly). Branches are NOTIFY-only; no arm
deletes one - the branch is what makes worktree deletion safe.

## What this deletes and what becomes impossible

Retired into arms (behavior preserved, existence gone): the terminal-state worktree sweep
and its ensure-on-park half, the orphan-scratch walk and per-spawn reclaims, worktree
self-healing, residue advisories (branch advisories excepted), dash ensure/self-reap and
every marker file, marker-staleness watchdogs, the `/proc` kill path (scan demoted to
advisory), stateless `watch` detection re-based off the retired inputs, and the generated
consumer surfaces they backed (`rigger-restore-the-dash` retires into daemon ownership;
`rigger-watch-a-run` and `rigger-reset-store` lose their interval/threshold text; residue
and bloat advisories move from `validate` to arm 5 - all generated from `src/docs.rs` with
content pins, so each migrates in the same change as its mechanism). Made impossible: a
second driver (flock + epoch), signalling a stranger (parentage + cgroup + verified
identity), disk exhaustion by rigger's writes (eviction + dual-point refusal), sweeping a
surface a human or agent holds live (three vetoes + FOREIGN tier), unowned scratch of any
future kind (path-authority registry + container pinning), a forged event manufacturing or
reaping a resource (declaration/observation split + capability tokens), and silent mass
deletion (blast-radius rail + reclaimed-facts).

## Delivery

Class-by-class and primitive-first, never arm-by-arm; each migration hoists every call site
to one authority function BEFORE replacing that one body, so no class ever has two owners or
none. A class spanning the conductor use case and the composition root migrates only once it
has a single entry point, and arms never call into the conductor nor the conductor into an
arm - both call the same class module. A store minimum-version marker plus the daemon lease
fence an old binary's heuristic reapers out of a governed project.

1. **Pure domain + ports.** The path-authority class registry and its enforcement test;
   `desired_world` as a tested fold keyed by `(run, owner, attempt)` with the run-terminal
   event; `diff -> Plan`; the `ConfinedPath` newtype and `confine()`; a signal PORT and an
   Fs/Proc/Git port set (time stays threaded as an argument - the crate's existing idiom -
   so NO new Clock port); the compaction-disjointness test. Ends with observe-only
   `validate --world-diff` (labeled with daemon-liveness so an offline diff never justifies
   a manual kill) and `--adopt` moved to a named write command.
2. **Process authority.** The typed signal wrapper as a port with a `libc`/`rustix` adapter
   AND a `kill(1)` adapter (so the `--no-default-features` lane keeps its documented parity,
   the signal taking a minted handle never a pid); `(pid, start-time, boot-id)` identity;
   cgroup-per-spawn; subreaper as backstop; the HMAC'd ledger with crash reconciliation that
   APPENDS termini. Consumer-safe before the daemon exists.
3. **The resident daemon.** Flock singleton, socket rendezvous (0700/0600, peer-cred),
   epoch acquisition and per-write CAS, conductor loop as a tick that appends `SpawnResult`,
   child table + capability tokens, owned dashboard, pipe + socket-claim liveness, the
   durable outbox, CLI/courier as clients with the fenced-gate direct-write exception, a
   persisted-nowhere fold cursor with full-derive-on-start, a crash-loop breaker, socket
   protocol versioning + `daemon stop --drain`, verified supervision posture.
4. **Reconciler arms in-daemon,** class by class, with the blast-radius rail and `--explain`
   from the first class; git-quarantine into a bare repo; envelopes with eviction +
   admission + incremental accounting; the machine-scope flock-proven assignment substrate;
   the anomalies projection + identity-cursored readers + hook + `notify:`.

## Acceptance (mechanical, each falsifies a core claim)

1. Replay determinism: the same log prefix yields the same `ResourceSet` whether folded
   incrementally or from scratch, in any process (the single most load-bearing property).
2. CLI observe-only: with a daemon live, every command issues zero unlink/rmdir/ref-delete
   from a non-daemon process.
3. No stranger signalled: every signal originates from the one wrapper taking a minted
   handle; a recycled-pid and a foreign-cgroup fixture are both refused.
4. Class registry exhaustive: the enforcement test runs and fails RED when a new
   path-deriving function is added without a variant.
5. Refusal scoping: an over-budget class refuses its own creations and admissions while an
   unrelated class still creates.
6. Wave survives a daemon outage: SIGKILL the daemon mid-wave (reached via a test-only
   synchronization seam, not a sleep), restart, and every worker that completed during the
   outage has its result in the LOG, every undesired resource is gone, every desired one and
   every held claim survive, and the ledger reconciles its survivors by appending termini.
7. Exclusivity: a second daemon exits naming the holder; a superseded daemon's next
   epoch-CAS write fails; unlinking the socket does not mint a second converger.
8. Escalation: a purged escalated worktree's unique content is recoverable by one checkout
   of its quarantine ref, and a `rigger hold` and a FOREIGN checkout survive every pass.
9. Fencing on the DEFAULT backend: acceptance 7's epoch conflict is asserted on sqlite, not
   only the server backend.
10. No prune removes a declaration-bearing event; the disjointness test is RED if one is
    ever shaped like a compactable key.

The bar that governs all of it: after a full campaign of rigger's OWN development,
`validate --world-diff` reports empty modulo FOREIGN (with tier assignment asserted per
path, so classifying everything FOREIGN cannot satisfy it), and the operator ran no
hand-rolled monitor, disk guard, or worktree reaper - the very things this session proved
necessary under the present design.
