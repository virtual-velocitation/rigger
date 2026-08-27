# Architecture addendum: the world authority

Status: PROPOSED - for operator review. This document (v5) merges and SUPERSEDES two prior
proposals (the resident conductor; the world reconciler) and integrates four rounds of
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

**The trust boundary is single-uid, so it is structural, never cryptographic.** On the
default sqlite backend the store is a plain file owned by the invoking user; every agent
turn runs as that same user, holds `Bash(rigger:*)`, and controls its own environment.
No OS credential (a socket peer-cred is the same uid for every courier) and no on-disk
secret (any key or token a file carries, a second same-uid process can read) can tell a
legitimate assertion from a forged one. The realistic adversary is therefore twofold and
BOTH are defended structurally: (1) an ACCIDENTALLY forged assertion - a confused or
buggy agent that emits the wrong `result`, double-reports, or names a sibling's spawn -
which must never constitute a destructive act; and (2) untrusted agent OUTPUT reaching a
privileged context - command injection, a hook run at daemon privilege - which must never
execute. The destructive-forgery defense is INTENT-AGNOSTIC: the terminus corroboration
below consults the daemon's own liveness facts, not the sender's intent, so a forged
terminus can neither manufacture nor reap a resource whether the forgery is accidental OR
deliberate - which is why "what becomes impossible" states that flatly. What remains
"bounded, not prevented" for a DELIBERATELY hostile same-uid process is a different class:
its ability to RETAIN or waste a resource (refresh a liveness veto on a resource it does
not own), to DoS (kill the daemon it could kill directly anyway), or to READ another
unit's same-uid data - none of which is a destructive-forgery act, each blast-radius
capped, and the OS user boundary, not rigger, is the security perimeter for that class
(documented, not assumed). Every "only the daemon may X" guarantee below is enforced by
WHICH code holds the lease and does the writing and by the daemon's own corroborating
observations, never by a secret an agent could hold or read.

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
  folds the declaration surface alone; a forged observation can no longer manufacture a
  desired resource or supersede a live owner.
- **A terminus is DECLARED by the daemon, never CONSTITUTED by an observation.** This is
  the round-1 principle made real where v3 still contradicted it. An agent's `result` is
  a PROPOSAL: the daemon records it, but the SpawnResult/SpawnAbandoned terminus that
  actually drops a spawn's resources is a DAEMON DECLARATION the daemon appends only after
  it has CORROBORATED the spawn is no longer live with its own facts. Corroboration is the
  EXACT NEGATION of the full liveness predicate - it fires only when ALL of these hold at
  once: the spawn's socket claim is released, its last `rigger progress` heartbeat is
  stale past the liveness TTL, AND no live process is rooted under its worktree (a forked
  child's cgroup empty, or an agent turn's `processes_rooted_under` scan empty). ANY single
  sign of life is a hard veto that blocks the terminus, and the process-presence sign is
  NON-DEGRADING: a live process under a worktree ALWAYS blocks its terminus, with no
  wall-clock override, because deleting a directory a live process writes is the founding
  defect. So a `result` for a spawn any of whose liveness signs still shows is inert: it
  queues as a proposal and never fires a terminus while the spawn lives. A gone spawn
  (all three signs negative) with no recorded result is DECLARED SpawnAbandoned from the
  same corroboration - closing the forged-reap and the crashed-worker-stranding holes
  together, without any unforgeable secret.
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
             |  |  that DECLARES a   |      |  cgroup, (pid,start,     |    |
             |  |  liveness-         |      |  boot-id); termini       |    |
             |  |  corroborated      |      |  DECLARED on corroborated|    |
             |  |  terminus)         |      |  non-liveness            |    |
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
**A wedged-but-alive daemon is always safe to kill.** Because every destructive act is
convergent-from-partial (a rail below), epoch-fenced, git-quarantine reversible, and
strictly ordered so a partial physical act lands in an arm-2-repairable state (including
the quarantine ordering spelled out below), an operator facing a daemon that holds the
lock but does not answer the socket may SIGKILL it without reasoning about mid-act state;
`daemon kill --wedged` (itself behind the operator capability) names the lock holder,
SIGKILLs it, and states this guarantee, so a hung lock is never a standoff and the CLI's
outbox fallback (below) covers the interval until a replacement starts.

**The store is the deeper authority: a fencing epoch, on every backend.** Each daemon
start acquires an epoch by `append(epoch-stream, ExpectedRevision::Exact(n),
[EpochAcquired])` - the same compare-and-swap the store already implements and tests on
BOTH the sqlite and server backends. Every daemon write carries its epoch, and every
DESTRUCTIVE reconciler arm re-asserts it with a CAS append immediately before acting. A
superseded daemon's next write fails on its own conflict, so exclusivity in the LOG holds
even where a socket or a clock lies. This fences the log, not the filesystem act itself:
the compare (a CAS append) and the act (an `unlinkat`) are separate operations on separate
resources, so a superseded daemon that has ALREADY passed its per-arm CAS is caught only at
its NEXT arm boundary - which is why the flock is the primary exclusivity and the epoch its
backstop, and why a residual one-arm-batch window exists only once the flock has ALREADY
failed (a degraded filesystem; on such a mount destructive arms are withheld entirely, see
the substrate preflight in Delivery). That residual is bounded by the blast-radius rail and
reversible by git-quarantine. To make the CAS enforceable rather than advisory, the daemon's
own writes are confined to one epoch-fenced stream; per-unit and per-spawn streams become
projections of it, and no process other than the daemon (save the fenced-gate exception
below) appends to that stream. The epoch stream's non-declaration markers (`EpochAcquired`,
per-arm CAS re-assertions) are COMPACTABLE - they are neither declaration-bearing nor
lifecycle-terminal - so the fence does not itself become an unbounded stream; they compact
under the same disjointness test that governs every prune.

**Singleton scope is the store, not the cwd.** The lock, socket, and epoch are resolved
through the SAME bounded-walk authority the store already uses (outermost `.rigger`,
`main_repo_root`, refuse-to-fabricate), keyed by the store's `(device, inode)` - so a
courier inside a worktree, a symlinked repo path, or a second mount namespace can never
mint a second daemon by resolving a nearer socket.

**The child table and ledger govern forked processes.** Every subprocess the daemon forks
is registered: in memory (the `Child` handle, spawn id, role, and its own cgroup v2
subtree); on disk (a machine-local ledger row `(pid, start-time, boot-id, spawn id, role,
cgroup path)`, opened `O_NOFOLLOW` inside the 0700 root, each row a versioned record so a
binary upgrade migrates the format rather than misparsing it). A spawn's process SET is its
cgroup membership, which a descendant cannot leave - so a double-forked or `setsid` process
is still attributed, and `PR_SET_CHILD_SUBREAPER` is a backstop, not the authority. Ending
work signals the CGROUP, not a pid from a file, through a typed signal PORT (see lanes
below). **Crash reconciliation is keyed on the cgroup subtree and process identity, never on
a secret.** At daemon start the ledger is reconciled against ground truth that survives a
restart - the persisted cgroup subtree and each row's `(pid, start-time, boot-id)`: a row
whose cgroup still holds live processes is ADOPTED (re-attached to the timer, or ended per
role); a row whose process identity is GONE and for which no `SpawnResult` proposal was
recorded is DECLARED `SpawnAbandoned`; a row that completed is DECLARED `SpawnResult`. Where
cgroup v2 delegation is ABSENT (cgroup v1, or a probed-undelegated host - the same posture
the signal port degrades for), there is no persisted subtree, so reconciliation falls back
to `(pid, start-time, boot-id)` identity ALONE, with the pid-recycle ambiguity documented as
a known reduced-lane limitation: a survivor row whose identity re-verifies unambiguously is
adopted or terminated as above, and any row whose identity is ambiguous is routed to arm 5
for operator diagnosis, never signalled or reaped on a guess. Every survivor thus reaches a
daemon-declared terminus or an explicit anomaly, so `desired_world` never strands a crashed
spawn's resources as desired-forever. The ledger MAY additionally be HMAC'd under a
per-lifetime in-memory key strictly as intra-lifetime tamper-evidence of the row set the
CURRENT daemon wrote; that HMAC never gates cross-restart adoption (a re-minted key cannot
verify a dead instance's rows, and a persisted key a same-uid agent could read would be a
forgery oracle - neither can be a reconciliation gate). Reconciliation trusts the kernel's
cgroup membership and `/proc` identity, which no same-uid agent can forge without the
privilege the OS boundary already denies. The `/proc` cwd scan is a read-only VETO input and
a `validate` advisory; nothing in the binary signals a scanned process.

**Liveness has two channels, both facts, and the corroboration is the exact negation of
them.** A forked child heartbeats over its pipe and is bounded by the daemon's timer. An
AGENT TURN - run by a courier the daemon did not fork - is represented by a courier RUNTIME
that holds a live socket CLAIM on its spawn CONTINUOUSLY for the spawn's whole life (not
released and re-taken per model turn, so there is no between-turns claim gap) and heartbeats
via `rigger progress` on its OWN wall-clock timer, decoupled from whatever tool call the
agent is blocked in - so a multi-minute `cargo build`/`cargo mutants` still heartbeats. A
worktree's liveness is the conjunction of three facts: an open socket claim, a fresh
progress timestamp (within TTL), and a live process rooted under the worktree. Arm 1's
live-worker veto consults ALL THREE - the child table plus open socket claims (both channels
above), the last-progress-freshness timestamp, and the read-only `processes_rooted_under`
scan - and ANY ONE of them showing life vetoes the reap. The process-presence veto is
NON-DEGRADING: it never expires on a wall clock while a process is actually rooted under the
worktree, so the process most likely to be mid-edit - the agent's own cargo/git - can never
be reaped under, even across a long op that outlasts every TTL. The orphan case that a naive
non-expiring veto would leak (a harness-parented process still running under a worktree whose
agent turn has ended, its claim released and progress stale) is NOT time-bombed into a reap
and NOT a silent leak: it is surfaced as an arm-5 ANOMALY naming the parentless-but-live
process and its worktree, for the operator (or the process's own exit) to resolve - and once
the process exits, the worktree reaps normally. A liveness FAULT (a stale claim or stale
progress while a process may still be present) records the fault but routes the resource
consequence through arm 5 for one cycle before any reap, so a false-positive timeout never
becomes an irreversible kill.

**The dashboard** is daemon-owned; its liveness IS daemon state. Marker files, stale-marker
diagnosis, and the self-reap protocol are deleted, not fixed.

**Signals reach a human without a poller.** Arm 5's open anomalies are not only a pull
surface: the daemon EMITS each new arm-5 signal through the same `notify:` port the
turn-boundary hook uses (argv-only, JSON on stdin) AND persists it to `anomalies.jsonl`, so a
signal raised while no loop is driving still reaches a consumer's configured notify hook and
survives durably for the next reader; and where a supervisor is present the crash-loop
breaker's trip is wired to the supervisor's own restart-limit and journal, so a daemon
crash-looping with nobody watching is not silent. Rigger owns the durable signal and the
notify port; delivering it to a pager is the consumer's `notify:` hook, by scope.

**The CLI as client.** `status`/`watch` are socket queries. `progress`/`result`/`emit` are
socket requests the daemon appends as PROPOSALS (a `result` proposes a terminus the daemon
declares only on liveness corroboration, per the principle) - but a client that finds the
daemon unreachable writes its request to a DURABLE OUTBOX inside its own already-declared
per-spawn container (one line per request, written temp-then-rename so a crash mid-append
never leaves a torn line), which the daemon drains on connect and every tick; so a worker
finishing during a daemon outage never loses its result. **Drain ordering is fixed, not
incidental:** on every connect and tick the daemon drains ALL outboxes first (each line
applied through the store's existing `record_result_if_absent` CAS, exactly-once by its
idempotency key, then the applied line removed), THEN reconciles the ledger, THEN re-derives
and re-spawns - so a worker that completed during an outage is never re-spawned into a
double-run, and its result is never reaped before it is drained. Every socket request carries
a client-minted idempotency key and is applied through the same CAS, never a blind `Any`
append. For a SINGLE-SHOT kind (`result`, one per spawn) the key is `(run, owner, attempt,
kind)`; for a REPEATABLE kind (`emit`, a reviewer's several findings) the key additionally
carries a per-instance discriminator (a client sequence or content id), so two distinct
observations in one attempt never collide into a silent dedup-drop - the CAS still makes a
retried identical request idempotent, but distinct requests stay distinct. A proposal names
the spawn it concerns; the daemon binds it to that spawn through the work-assignment token it
minted and handed out with the work via `wave --pull` (delivered in the agent's own
assignment channel, not a shared file), which scopes an ACCIDENTAL cross-spawn misreport -
the HARD backstop against a forged terminus remains the liveness-corroborated declaration,
which needs no unforgeable token. `run` starts or submits to the daemon; `wave --pull` is the
named verb couriers use to fetch parked work (the tick is park-only and never spawns agents).
DAEMON-AUTHORITY and DESTRUCTIVE-CONTROL verbs (`run`, `step --once`, `reset`,
`reset --force-live`, `daemon kill --wedged`) require an OPERATOR capability - and because the
single-uid model means an env var or same-uid file is forgeable, the capability is a marker a
harness-parented `Bash(rigger:*)` agent STRUCTURALLY lacks: a supervisor-inherited file
descriptor, a systemd credential, or a controlling-tty test. This stops a confused agent from
ACCIDENTALLY assuming daemon authority or triggering a reset; against a deliberate agent that
forges the marker the real bound is that even a forged `step --once` runs the REAL converger
(agents hold only `Bash(rigger:*)`, not rigger internals) under every rail - the corroborated
terminus, the non-degrading live-process veto, the blast-radius cap, git-quarantine - so no
destructive act escapes those bounds. `step --once` is retained for tests and takes the SAME
lease as the daemon, never a second converger. A FENCED gate (`RIGGER_STORE_FENCE_DIR` set)
does NOT connect to the daemon at all: it opens its fenced store directly, so a unit's own
test suite still cannot write the live run stream - and fence mode OPENS the resolved
`FENCE_DIR` first and `fstat`s the open handle, REFUSING loudly when that handle's
`(device, inode)` equals the live store's (no resolve-then-open TOCTOU, and a hardlink,
bind-mount, or symlink all resolve to the same identity and are refused), so the one
sanctioned direct-write can never be aimed at the live declaration stream. `reset` asks the
live daemon to reconcile to quiescence and hold convergence for the prune, refusing (via the
held lock, never a socket probe) only if the daemon declines or run state is non-quiescent.
The daemon runs detached from login sessions only where a working, lingering-enabled
supervisor is VERIFIED present - at start the daemon probes for one and reports its
supervision posture (`systemd-user(lingering)` / `unsupervised` / `unknown`), along with
whether a delegated writable cgroup v2 subtree is present, through `rigger status`, so both
the session-independence and the process-set guarantees are checked facts, never silent
assumptions, and `status` names the remedy for an `unsupervised` or cgroup-undelegated
posture rather than dead-ending on the honest signal.

## The resource model

A RESOURCE is anything rigger causes to exist outside the log. The model is a CLASS
REGISTRY IN CODE - one enum, one derivation per variant - and the table is generated from
it. The enforcement test keys on PATH AUTHORITIES, not syscalls: every variant names the
pure path function that derives it (the eight existing single-authority functions -
`unit_worktree_dir`, `unit_cache_sibling`, `spawn_scratch_path`, `mutation_scratch_path`,
`liveness::marker_path`, `review_fence_sibling`, `gate::default_cache_dir`,
`budget::default_slot_dir`, and their peers), PLUS the per-spawn cgroup v2 subtree, now a
first-class governed class with its own path authority (`spawn_cgroup_path`) so the kernel
resource the daemon creates is owned and reaped like any other - a cgroup left empty after
its process set exits is not auto-removed by the kernel, so its empty-subtree `rmdir` is an
arm action (REPAIR heals a stale-but-present subtree; REAP removes an undesired one),
closing the leak of the one resource class this design itself introduces. The test fails any
production path-deriving function no variant names, and any `Command::new`/`create_dir_all`
whose path does not originate in one. A regex over call sites could pass while the 403G
recurred; a path-authority key cannot. The provenance check is a compile-gated audit: raw
`std::fs`/`std::process` path constructors are a compile error outside the class modules
(a wrapper the workspace lints enforce), so "originates in an authority" is a mechanical
fact, not a reviewer's reading.

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
owning daemon's epoch and identity. Reclamation is deliberately conservative and asymmetric
by class. A reader never deletes a row whose slot flock is currently HELD (a live owner). A
row whose flock is FREE but whose owning project ROOT still resolves is a DORMANT real owner:
its row sits inert for that project's next daemon start to re-acquire, reclaimed by no one.
Reclamation of a REBUILDABLE/positional slot (a build-budget slot, cache assignment, a
dashboard singleton) happens only on POSITIVELY CONFIRMED abandonment - the project root's
parent is present AND the root entry is confirmed ABSENT (a real deletion) - never on mere
unreachability: a root that is unresolvable because its MOUNT is absent (a removable drive
unplugged, an NFS share down) is UNKNOWN, routed to arm 5, and never reclaimed, so a
briefly-offline project is never mistaken for a deleted one. CONTENT-BEARING quarantine repos
are governed NOT by slot-reclamation at all but by their own declared retention window and
LRU (below), independent of whether the owning project root currently resolves - so no
reclamation path can ever delete unique content because a mount blinked. Two projects on one
machine resolve by scope and held lock, never by whichever reconciler ran last. Machine paths
move out of world-writable `/tmp` into per-user 0700 roots created with an explicit mode and
verified owner-owned, not merely `create_dir_all`'d.

**The derivation.** `desired_world(log, clock, sizes) -> ResourceSet` is deterministic in
its EXPLICIT inputs - `clock` (the injected `now`) and measured `sizes` are arguments, so
log-pure rows stay replayable at any position and clock/size-governed rows are testable by
injection. Its inputs are the DECLARATION surface plus these injected measurements ONLY;
the agent-writable OBSERVATION surface is a distinct type that never enters this fold - the
name separation is enforced so an implementer cannot wire the observation stream into the
constitutive derivation and reopen the CONSTITUTE/PROPOSE hole. A RUN-TERMINAL fact
(appended when a run reaches fixpoint or is superseded) stops the last run's resources
staying desired forever. A spawn's terminus is the daemon-declared `SpawnResult`/
`SpawnAbandoned` described in the principle - declared on the daemon's own liveness
corroboration, never constituted by an agent's `result` observation. In steady state (no
crash, daemon up) the in-process driver's conductor tick DECLARES that terminus PROMPTLY when
a spawn completes (today it does not - the channel is a wakeup, but the log must be the
record, or a daemon-tick run leaves every spawn desired forever); the crash path declares it
too from reconciliation, so completion and abandonment converge through one authority whether
or not the daemon restarted. Reads are incremental from a daemon-held checkpoint cursor over
type-indexed streams - but the cursor is PROCESS MEMORY only (a daemon start always
full-derives), and arm 1 may act ONLY against a `ResourceSet` produced by a full
re-derivation performed at least once in the current daemon lifetime AND advanced to the
current log head - incremental folding advances the observe/notify arms alone, and no
destructive arm acts against a set stale relative to a declaration made since. Full
re-derivation cost grows with log length; the daemon periodically writes a materialized
ResourceSet SNAPSHOT (a rebuildable projection, never a declaration, a VERSIONED on-disk
format like the ledger and outbox so a binary upgrade migrates or discards it rather than
misparsing) that it folds forward from, and the once-per-lifetime full derive VERIFIES the
snapshot; on any mismatch the full derive WINS - the snapshot is discarded and rebuilt, never
trusted over the log - and until that verify completes, `status` and `validate --world-diff`
label their answer PROVISIONAL so a just-restarted daemon never presents an unverified
snapshot as authoritative at the moment an operator is diagnosing a crash. No prune may
remove a declaration-bearing or lifecycle-terminal event (declaration-bearing types and
compactable types are disjoint, pinned by test).

## The reconciler: five arms, five rails

Runs as the daemon's internal loop OFF the socket-serving path (a slow re-derivation never
blocks a courier's result), on its own timer; every CLI invocation computes the same diff
OBSERVE-ONLY and submits judgment items over the socket. On a substrate whose preflight
found weak locks (see Delivery), arms 1-3 are WITHHELD entirely - the daemon runs
observe+notify-only until the store is relocated or the operator explicitly overrides -
because a known-degraded mount makes the residual concurrent-reap window ordinary rather than
adversarial, and safety outranks liveness. Arms, in order:

1. **REAP** present-but-undesired, with vetoes that are facts not heuristics: the child
   table, open socket claims, last-progress freshness, and the read-only cwd scan, ANY of
   which blocks the reap, the process-presence one non-degrading. A spawn-owned resource is
   undesired only once the daemon has DECLARED that spawn's terminus - which the daemon does
   only on the full corroboration above - so a forged `result` cannot render a live spawn's
   worktree undesired in the first place. Deletion follows the git-quarantine rule below, so
   every reap of unique content is reversible.
2. **REPAIR** present-but-divergent - each class carries an integrity predicate;
   registered-but-absent worktrees recreate from their branch, zero-length git admin
   entries heal, bare leftover dirs adopt-or-clear, a stale empty cgroup subtree is
   `rmdir`'d. Absorbs the shipped self-healing guarantee. A class is EITHER repairable
   (arm 2 owns it, arm 3 may not evict it) OR evictable (arm 3 owns it, arm 2 may not
   recreate it) - never both, so arm 2 cannot recreate what arm 3 just evicted (the
   envelope livelock).
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
  only. The GRAMMAR-RECOGNIZED tier needs the INVERSE of a class's forward path authority
  (a discovered path parsed back to an owner key); that inverse is a NAMED, TESTED
  primitive per class that FAILS CLOSED to FOREIGN on any ambiguity or non-conforming
  path, so a legacy or malformed path is reported, never mistakenly quarantined. This is
  what makes "log-derived" compatible with a world that predates the design and stops
  recognized residue becoming a permanent leak.
- **One path authority, safe at USE time, not just construction.** Every path an arm
  touches is a `ConfinedPath` NEWTYPE that only `confine()` can construct: lstat (a symlink
  leaf is refused), canonicalized component-wise containment under a validated root,
  same-device check - AND, captured at that moment, an `openat2(RESOLVE_BENEATH)` directory
  file descriptor for the container. Every destructive method operates AT that captured fd
  (`unlinkat`/`rmdir` relative to it), never by re-resolving the path string later; a deep
  recursive delete descends by opening EACH child from its parent fd with
  `openat2(RESOLVE_BENEATH)` (and `RESOLVE_NO_XDEV`), never falling back to a path-string
  `remove_dir_all` that would re-resolve and re-open the race - so a symlink swapped into any
  intermediate component at any depth between validation and act cannot redirect the syscall
  outside the root, closing the construction-time-only TOCTOU that a compile-time type alone
  cannot. A `ConfinedPath` is never deserialized; it is re-derived through `confine()` on
  every read from a ledger row or `anomalies.jsonl`, so the invariant cannot silently regress
  through a `Deserialize` impl.
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
  mode after N restarts in window W (default N=5 in W=10 min, both configurable, surfaced
  in `rigger status` and wired to the supervisor restart-limit, its tripping an arm-5
  signal), so a daemon that dies on every start cannot reap-loop.

## Git-quarantine: the retention discipline

The operator-ratified rule: **git is the retention system; disk holds only what's live.**
Residue splits by recreatability. REBUILDABLE content (build caches, mutation scratch,
target trees - the bulk by bytes) is deleted outright; its recovery is the cold rebuild
that defines a cache. UNIQUE content (any tree holding uncommitted tracked or untracked
source) is COMMITTED before its tree is deleted - but with PLUMBING in a fully scrubbed
environment (`-c core.hooksPath=/dev/null -c core.fsmonitor=false -c
core.attributesFile=/dev/null`, `GIT_CONFIG_GLOBAL=/dev/null`, `GIT_CONFIG_NOSYSTEM=1`,
`hash-object --no-filters`/`mktree`/`commit-tree`), and into a DEDICATED BARE quarantine
repository outside the project's object store, so untrusted agent content never runs a hook,
a clean/smudge attribute FILTER, or a system-config command at the daemon's privilege, never
enters the repo that gets pushed, and never re-arms a gate on checkout (`.rigger/` and
`.claude/` paths are ineligible and reported instead). The purge is STRICTLY ORDERED so a
kill at any point is safe: the quarantine ref update (the final, atomic step) durably records
the content BEFORE the worktree delete begins, so a SIGKILL landing mid `commit-tree` - the
remedy `daemon kill --wedged` itself prescribes - simply never reaches the delete: the
worktree is left intact, the run is unchanged, and the orphaned loose objects the interrupted
`commit-tree` wrote are collected by the quarantine repo's own `gc`; the successor daemon
re-attempts the snapshot-then-purge from a clean state. A commit step that fails GRACEFULLY
(disk-full or a corrupt object mid `commit-tree`, observed and caught) likewise ABORTS the
purge - the tree is left in place and raised as an arm-5 anomaly, never deleted on a failed
snapshot - so a quarantine failure, killed or caught, can never become silent data loss.
Quarantine refs are keyed by `(run, owner, attempt)` - the same identity the resource model
mandates, never a bare owner id - so a unit escalated twice never repoints one ref and loses
the earlier attempt's content under the later attempt's eviction schedule. They are a
size-governed class under arm 3 with a declared retention window and LRU eviction (real
deletion + `gc`), governed by that window ALONE and never by machine-slot reclamation, so no
project-root-liveness check can ever delete a live-but-dormant owner's unique content.

**Escalation holds no disk.** An escalated unit's worktree is purged at terminal like any
other - the purge is preceded by the unique-content snapshot every purge gets, and the unit
branch is the durable working base. Remedy work happens on operator-created checkouts,
which the rail classifies FOREIGN and never touches; an operator entering ANY governed
worktree announces presence with `rigger hold <path>` - scoped by `confine()` to a single
container (never a repo root), owned by its taker, carrying a required TTL after which it
degrades to arm 5 rather than remaining a veto, and - like every client mutation when the
daemon is down - appendable through the durable outbox. Branches are NOTIFY-only; no arm
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
content pins, so each migrates in the same change as its mechanism). Made impossible
(regardless of whether a forgery is accidental or deliberate, because each guarantee rests on
a mechanism that does not consult sender intent): a second driver (flock + epoch), signalling
a stranger (parentage + cgroup + verified identity), disk exhaustion by rigger's writes
(eviction + dual-point refusal), sweeping a surface a human or agent holds live (the
four-fact liveness conjunction, the non-degrading live-process veto, and the FOREIGN tier),
unowned scratch of any future kind including the daemon's own cgroups (path-authority registry
+ container pinning + cgroup reap arm), a forged or confused observation manufacturing or
reaping a resource (declaration/observation split + the terminus DECLARED only on the daemon's
own liveness corroboration, so a `result` for a live spawn is inert and a crashed spawn is
abandoned, not stranded), and silent mass deletion (blast-radius rail + reclaimed-facts). What
is bounded rather than made impossible (the deliberate same-uid class named in the principle):
a hostile process wasting or retaining a resource it does not own, DoSing the daemon, or
reading another unit's same-uid data - blast-radius capped and OS-user-boundary scoped, never
a destructive-forgery act.

## Delivery

Class-by-class and primitive-first, never arm-by-arm; each migration hoists every call site
to one authority function BEFORE replacing that one body, so no class ever has two owners or
none. A class spanning the conductor use case and the composition root migrates only once it
has a single entry point, and arms never call into the conductor nor the conductor into an
arm - both call the same class module. A store minimum-version marker plus the daemon lease
fence an old binary's heuristic reapers out of a governed project; the same versioning
extends to every NEW on-disk format this design adds (ledger row, per-spawn outbox line,
`anomalies.jsonl`, machine-scope slot file, materialized ResourceSet snapshot), each carrying
a format tag so a binary upgrade migrates rather than misparses, and rollback is fenced by the
store minimum-version marker refusing an older binary against a newer store. The epoch CAS and
the flock singleton assume a store on a LOCAL filesystem with linearizable Exact-CAS and
honest advisory locks; a daemon start PREFLIGHTS the store's filesystem, and on a known-weak
mount (such as NFS) it does NOT run destructive arms - it starts in observe+notify-only mode,
warns loudly once, flags the posture in `rigger status`, and names the remediation (relocate
`.rigger` to a local disk, or override explicitly) - because the residual concurrent-reap
window widens from adversarial to ordinary operation where advisory locks are weak, and
withholding convergence is the safe posture, not warn-and-proceed.

1. **Pure domain + ports.** The path-authority class registry (including the cgroup class)
   and its enforcement test; `desired_world` as a tested fold keyed by `(run, owner,
   attempt)` over the declaration surface plus injected clock/sizes, with the run-terminal
   event; `diff -> Plan`; the `ConfinedPath` newtype with `confine()`, its captured
   `RESOLVE_BENEATH` fd, and the per-level descent for deep deletes; the inverse path-parse
   primitive that fails closed to FOREIGN; a signal PORT and an Fs/Proc/Git port set (time
   stays threaded as an argument - the crate's existing idiom - so NO new Clock port); the
   compaction-disjointness test. Ends with observe-only `validate --world-diff` (labeled with
   daemon-liveness - simply "offline" until stage 3 mints the daemon, so an offline diff never
   justifies a manual kill) and `--adopt` moved to a named write command.
2. **Process authority.** The typed signal wrapper as a port whose DEFAULT lane signals the
   whole process set recycle-proof by writing `cgroup.kill` (a plain sysfs write, needing no
   libc), with a `libc`/`rustix` adapter for hosts on cgroup v1 and a last-resort `kill(1)`
   adapter reserved for hosts lacking cgroup v2 - where the pid-recycle risk is documented as
   a known reduced-lane limitation, every signal there gated on a fresh identity re-verify -
   so the `--no-default-features` lane keeps a working, honestly-scoped signal rather than a
   falsely-claimed parity; `(pid, start-time, boot-id)` identity; cgroup-per-spawn with its
   reap arm; subreaper as backstop; the ledger with crash reconciliation keyed on the cgroup
   subtree and identity that DECLARES termini (`SpawnResult` for completed, `SpawnAbandoned`
   for gone-and-resultless), with the cgroup-undelegated identity-only fallback that routes an
   ambiguous survivor to arm 5, independent of any HMAC. Consumer-safe before the daemon
   exists.
3. **The resident daemon.** Flock singleton, socket rendezvous (0700/0600, peer-cred),
   epoch acquisition and per-write CAS, conductor loop as a tick that DECLARES a
   liveness-corroborated terminus, child table, owned dashboard, the four-fact liveness
   conjunction with a non-degrading live-process veto and courier heartbeats on an independent
   timer, the durable outbox with fixed drain-before-reconcile-before-respawn ordering and
   per-instance idempotency keys, CLI/courier as clients with the fenced-gate direct-write
   exception (fstat-the-handle live-store refusal), work-assignment tokens minted at
   `wave --pull`, the structurally-unforgeable operator capability gating daemon-authority and
   destructive-control verbs, the arm-5 notify-port emission + supervisor restart-limit wiring,
   a persisted-nowhere fold cursor with full-derive-on-start and a versioned materialized-
   snapshot fast path (full-derive wins on mismatch; provisional labeling pre-verify), a
   crash-loop breaker, socket protocol versioning + `daemon stop --drain` + `daemon kill
   --wedged`, verified supervision and cgroup-delegation posture.
4. **Reconciler arms in-daemon,** class by class, with the blast-radius rail and `--explain`
   from the first class, and the weak-mount observe-only withholding; git-quarantine into a
   bare repo with the fully scrubbed plumbing, the ordered ref-before-delete purge, and
   `(run, owner, attempt)` refs governed by their own window; envelopes with eviction +
   admission + incremental accounting; the machine-scope flock-proven assignment substrate
   with its confirmed-absent (never merely-unreachable) reclamation and quarantine exclusion;
   the anomalies projection + identity-cursored readers + hook + `notify:`.

## Acceptance (mechanical, each falsifies a core claim)

1. Replay determinism: the same log prefix yields the same `ResourceSet` whether folded
   incrementally or from scratch, in any process (the single most load-bearing property).
2. CLI observe-only: with a daemon live, every command issues zero unlink/rmdir/ref-delete
   from a non-daemon process.
3. No stranger signalled: every signal originates from the one wrapper taking a minted
   handle; a recycled-pid and a foreign-cgroup fixture are both refused.
4. Class registry exhaustive: the enforcement test runs and fails RED when a new
   path-deriving function (the cgroup class included) is added without a variant.
5. Refusal scoping: an over-budget class refuses its own creations and admissions while an
   unrelated class still creates; a REAP set past the blast-radius threshold refuses as a
   batch and routes to arm 5 rather than deleting.
6. Wave survives a daemon outage: SIGKILL the daemon mid-wave (reached via a test-only
   synchronization seam, not a sleep), restart, and every worker that COMPLETED during the
   outage has its result in the LOG (drained before any re-derive, so none is re-spawned into
   a double-run), every undesired resource is gone, every desired one and every held claim
   survive, and the ledger reconciles its survivors by appending termini.
7. Exclusivity: a second daemon exits naming the holder; a superseded daemon's next
   epoch-CAS write fails; unlinking the socket does not mint a second converger.
8. Escalation: a purged escalated worktree's unique content is recoverable by one checkout
   of its `(run, owner, attempt)` quarantine ref, a twice-escalated unit keeps both attempts'
   content, and a `rigger hold` and a FOREIGN checkout survive every pass.
9. Fencing on the DEFAULT backend: acceptance 7's epoch conflict is asserted on sqlite, not
   only the server backend.
10. No prune removes a declaration-bearing event; the disjointness test is RED if one is
    ever shaped like a compactable key.
11. Observation cannot constitute a terminus: a forged or duplicate `result` naming a spawn
    the daemon still sees LIVE (claim held, progress fresh, OR process present) declares NO
    terminus and its worktree survives every REAP pass; the terminus fires only after the
    daemon corroborates non-liveness on all three signs.
12. A crashed worker is abandoned, not stranded: a worker that DIES during a daemon outage
    (no result, no outbox line) is DECLARED `SpawnAbandoned` on restart from the cgroup +
    identity reconciliation, and its resources converge rather than staying desired forever.
13. No orphan cgroup: after a spawn's process set exits (and across a daemon SIGKILL +
    restart), its cgroup subtree is reaped, so `find` over the delegated cgroup root returns
    to baseline; the reap needs no HMAC to succeed.
14. Machine-scope reclamation is conservative: a rebuildable slot whose flock is FREE and
    whose project root is CONFIRMED ABSENT (parent present, entry gone) is reclaimed; a slot
    whose root is merely UNREACHABLE (mount absent) is routed to arm 5 and never reclaimed; a
    content-bearing quarantine ref is never reclaimed by this path at all; a HELD-flock or
    still-resolving-root slot is never deleted by another daemon.
15. Use-time path safety: a symlink swapped into an intermediate component of a
    `ConfinedPath` between `confine()` and the destructive act - at the leaf OR mid-descent
    of a deep tree - does not redirect the syscall outside the container (every act runs at a
    captured `RESOLVE_BENEATH` fd).
16. A live worker is never reaped under: a live, long-running, non-heartbeating op (a real
    `cargo` subprocess rooted under a worktree) survives every REAP pass past every TTL
    because the process-presence veto does not degrade; and an agent turn whose claim is held
    or whose progress is fresh within TTL is likewise never declared terminated, while a
    parentless-but-live process is raised as an arm-5 anomaly rather than reaped or leaked.
17. The crash-loop breaker trips: a daemon forced to fail N times in window W enters
    diagnose-only mode, emits the arm-5 signal, and runs no destructive arm thereafter.
18. A kill mid-quarantine-commit loses nothing: SIGKILL the daemon during an escalation
    purge's `commit-tree` (test-only seam), restart, and the worktree is intact, the run
    unchanged, and no unique content is lost.
19. The steady-state tick declares a terminus: with the daemon UP and no crash, a spawn that
    completes normally has its `SpawnResult` appended by the conductor tick promptly; a
    regression that drops the tick's declaration is RED here even while 6/11/12 stay green.
20. A weak-mount daemon withholds destruction: on a preflight-flagged weak-lock store the
    daemon serves `status`/`validate --world-diff` but issues zero unlink/rmdir/ref-delete
    from any arm until the store is relocated or the operator explicitly overrides.

The bar that governs all of it: after a full campaign of rigger's OWN development,
`validate --world-diff` reports empty modulo FOREIGN (with tier assignment asserted per
path, so classifying everything FOREIGN cannot satisfy it), and the operator ran no
hand-rolled monitor, disk guard, or worktree reaper - the very things this session proved
necessary under the present design.
