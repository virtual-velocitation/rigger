# Architecture addendum: the resident conductor

One rigger process per project owns the run for the run's whole lifetime. Every
subprocess the loop needs is a registered, supervised child of that process. Ending work
is a direct act by its parent, never an inference. The command line is a client.

## Problem

The loop today is re-entrant: a run is driven by hundreds of short-lived processes, each
of which reconstructs the world from the event store, acts once, and exits. No process
lives long enough to KNOW anything, so every fact that must survive between two
invocations is smuggled through the filesystem - a lock file to serialize steppers, a
marker file recording the dashboard's port and pid, worktree directories whose mere
existence encodes ownership, liveness inferred from the modification time of a touched
file, and a `/proc` scan that guesses which processes belong to the loop by testing
whether their working directory sits under a scratch path.

Each smuggling channel is an independent failure class, and every one of them has fired:

- **A recorded pid is not a process.** Pids are recycled integers. A marker that stores
  one can point at a dead process (the dashboard advertised by `rigger status` while
  nothing listens) or - strictly worse - at an INNOCENT process that later inherited the
  number. Anything that signals a recorded pid without verifying identity can kill an
  unrelated process on the operator's machine.
- **Signaling through a shell command is parsed, not typed.** Killing a process group by
  invoking the system `kill` with a negative-pid argument silently retargets when the
  argument is misparsed (a negative operand without an option terminator reads as an
  option). A signal built as a string can strike a target the code never named - observed
  as far as a whole-session kill: every process the operator owns, including their
  display server, dead in one stroke. A consumer running the loop on their workstation
  inherits exactly this blast radius.
- **Ownership by inference over `/proc` is a heuristic with a kill switch attached.** The
  teardown scan signals processes whose cwd lies under a directory. Its safety depends
  entirely on the base path never resolving too broadly - a property enforced nowhere in
  the type system and re-derived at every call site.
- **Concurrent short-lived writers corrupt shared state.** Two binaries of different
  versions stepping the same store produced interleaved-cursor corruption; the lock file
  that should serialize them is itself just another file.
- **Process lifetime is coupled to the operator's login session.** The driver, its
  couriers, and their builds are parented under whatever terminal launched them. A
  session teardown - logout, crash, machine sleep - SIGKILLs the whole tree mid-write,
  and every resume starts with archaeology.

These are not five bugs; they are one bug with five faces: **the loop has no resident
authority over its own processes and state.**

## Target state

```
                    rigger daemon            (one per project, started once)
                    ===================================================
                    | singleton: bound unix socket .rigger/daemon.sock |
                    | THE only writer of the event store               |
                    ===================================================
                      |          |            |             |
        +-------------+          |            |             +--------------+
        |                        |            |                            |
   conductor loop            dashboard   spawn supervisor             liveness
   (the same event-         (owned: a    (parent of every             (heartbeats
   sourced fold, now         thread or    subprocess RIGGER           arrive over
   resident; "step" is       owned child; RUNS; registers each        pipes from
   an internal tick;         alive iff    at fork; reaps each         children, not
   parking is a state        the daemon   by handle)                  file mtimes)
   transition, still         says so)         |
   journaled to the                    +------+---------+
   store)                              |                |
                                  gate/build        worktree
                                  commands          helpers

   rigger CLI    --unix socket-->  daemon   (status, progress, result, emit, watch:
   workflow                                  answered from live state - the truth
   couriers      --unix socket-->            is a query, not a forensic inference;
   (the agent                                read-only store access remains for
   runtime,                                  offline inspection)
   unchanged)
```

### Why this shape (the load-bearing decisions)

**One resident owner.** A process that stays alive can hold what the re-entrant model
had to smuggle: the child table, the dashboard's existence, the liveness channel, the
single store cursor. Every filesystem side-channel above stops being load-bearing.

**Parentage is the only foolproof ownership.** A parent holds unforgeable identity over
its children: their exits are delivered to it (SIGCHLD), their pids cannot be recycled
out from under it before it reaps them, and ending them is a direct, typed act on a
handle it minted. Everything else - pid files, scans, name matching - is inference, and
inference over processes eventually signals a stranger. The design rule: **the loop may
only signal a process it parented and still holds, or one whose recorded identity
(pid + start time) it has just re-verified. It never signals a guess.**

**The socket is the singleton.** Binding `.rigger/daemon.sock` is atomic: a second
daemon fails to bind and exits with a message naming the live one. A stale socket file
(daemon crashed) is detected by a failed connect and safely replaced. This retires the
marker-file pattern entirely - there is nothing to go stale, because liveness and
discoverability are the same fact.

**The daemon is the only store writer.** All mutation flows through one process running
one binary version, ordered by one cursor. The mixed-version interleaving class is
structurally gone. (The store keeps its own append-order defenses - the daemon can
crash, and a crashed daemon's successor must still find a coherent store.)

## Design

### Supervision: the child table and the ledger

Every subprocess is spawned by the daemon and registered at fork in two places:

- **The child table (in memory):** the `Child` handle, the spawn id it serves, its role
  (agent, gate, build, worktree helper, dashboard), and the process group minted for it
  at spawn (`process_group(0)` - the group id is known-good by construction, never looked
  up after the fact).
- **The process ledger (on disk, machine-local):** `(pid, start-time, spawn id, role,
  group)` per row, written before the child runs, retired when the child is reaped. Start
  time is read from `/proc/<pid>/stat` field 22 - the (pid, start-time) pair is a
  recycling-proof identity: a reused pid never reproduces the dead process's start time.

The daemon sets `PR_SET_CHILD_SUBREAPER`, so a child that double-forks cannot escape:
its orphans reparent to the daemon, appear in SIGCHLD accounting, and are attributed to
their originating spawn via the group recorded at fork.

**Ending work:** look up the child in the table, signal its group directly through a
typed syscall wrapper (TERM, bounded grace, KILL), reap, retire the ledger row. No shell
`kill`, no argv, no string parsing anywhere in the signal path. The syscall wrapper is
pure-Rust raw-syscall on Linux so BOTH feature lanes (with and without the C runtime)
share one implementation.

**Crash residue:** at start, the daemon reconciles the ledger: for each row, a process
that exists AND matches the recorded start time is a survivor of a crashed predecessor -
adopted or ended per its role; a row whose pid is gone or whose start time mismatches is
retired untouched. The identity check is what makes reconciliation safe on a machine
that has been recycling pids since the crash.

**The `/proc` cwd scan is demoted, permanently.** It remains only as an ADVISORY in
`rigger validate` - "processes rooted in scratch that no ledger row owns" is a reported
finding for a human. Nothing in the binary signals a process found by scanning.

### The dashboard

The dashboard is owned by the daemon - an internal server task, or an owned child in the
child table like any other. Its liveness IS daemon state: `rigger status` reports the
dashboard by asking the process that owns it. The marker file, the stale-marker
diagnosis, the marker-follows-bind dance, and the self-reap protocol are all deleted,
not fixed - the failure they managed cannot be expressed in this shape.

### Liveness

Children hold a pipe to the daemon and write heartbeats; the daemon enforces each
spawn's wall-clock bound on its own timer. Classification of a hung spawn (which
failure class, what the taxonomy says) is unchanged - only the transport changes, from
marker-file mtimes polled by the next stepper to events observed by the resident parent
the moment they stop arriving.

### The command line becomes a client

| Command                          | In the target state                                |
| -------------------------------- | -------------------------------------------------- |
| `status`, `watch`                | socket query - answered from live state            |
| `progress`, `result`, `emit`     | socket request - the daemon appends to the store   |
| `run`                            | starts the daemon (or submits to the live one)     |
| `step`                           | internal tick; retained as a foreground one-shot   |
|                                  | for tests and CI (`--once` semantics)              |
| `dash`                           | asks the daemon (starts it if absent)              |
| `graph`, `peers`, `stats`, ...   | unchanged - read-only store/graph access, offline  |
| `validate`, `reset`              | offline when the daemon is down; refused (with the |
|                                  | daemon named) when it is up - one writer, always   |

A client that finds no live daemon and needs one says so and how to start it; read-only
commands degrade gracefully to direct store reads.

### Agent execution: the workflow driver stays the agent runtime

Agent turns are executed by the workflow driver's couriers, exactly as today, and that
is a design decision, not a transition state. The agent runtime is where the model,
its tools, its skills, and its permission surface live; rigger's value is the
disciplined lifecycle AROUND agent turns, not reimplementing their execution. So the
boundary is permanent and clean:

- **The daemon owns the run**: the conductor loop, the dashboard, gates, builds,
  worktree lifecycle, liveness, and the store cursor.
- **The couriers own agent turns** and are CLIENTS of the daemon: a courier asks the
  live daemon for parked work over the socket, runs the agent turn in its own runtime,
  and records the result through the socket (`rigger result` and `rigger progress` as
  client calls). Parking remains a real hand-off boundary - but it is now a state
  transition inside a resident process (journaled to the store as ever), not a process
  death. Nothing is reconstructed between turns, because the conductor never went away.
- **Agent-spawned work is still supervised.** When an agent turn needs the loop to run
  something heavy (a gate, a build), that subprocess is requested from and parented by
  the daemon, so it lands in the child table like everything else - the courier's own
  session dying never strands a build the daemon owns.

The event store remains the source of truth: a daemon restart resumes a run exactly as
a fresh stepper resumes one today, so nothing already built on the log - replay, run
scoping, the ledger of units, the knowledge graph - changes meaning.

### Surviving the operator's session

The daemon is started once per project and SHOULD be run detached from any login
session (the setup surface offers a user-service unit with restart-on-failure and
lingering where the host supports it). The invariant this buys the consumer: **a run's
processes are never coupled to a terminal or a GUI session, and a session teardown
never kills a run** - the daemon reconnects clients when they return. Conversely, the
daemon never reaches OUT of its tree: the only processes it can signal are the ones in
its child table or ledger-verified survivors of its own predecessor. Both directions of
the blast radius - the session killing the run, the run killing the session - are
closed by the same boundary.

### Socket hygiene

`.rigger/daemon.sock` is created `0700`-scoped inside the project's `.rigger`
directory; the protocol is line-delimited JSON requests answered in kind - the same
serialization the store already speaks. There is no network listener; the dashboard's
HTTP port remains loopback-only as today.

## Delivery

Staged so every stage lands green through the loop and none blocks the queue:

1. **Process authority first** (the primitives, usable before the daemon exists): the
   typed signal wrapper on raw syscalls, (pid, start-time) identity everywhere a pid is
   recorded, groups minted at spawn, subreaper, the ledger with crash reconciliation,
   and demotion of every kill-by-inference path to ledger-driven teardown. This is the
   consumer-safety payload - it removes the ability to signal a stranger even while the
   step model still runs.
2. **The resident daemon**: socket singleton, resident conductor loop, owned dashboard
   (marker deleted), pipe liveness, CLI-and-courier-as-client, single-writer store,
   step retained as an internal tick and a test-mode one-shot. The workflow driver and
   its couriers keep executing agent turns unchanged - they gain a live authority to
   talk to and lose every filesystem side-channel.

Interaction with the existing queue: the store's own append-order defenses stay (a
crashed daemon must never leave an incoherent store behind); the queued dashboard
marker-lifecycle work is subsumed by stage 2 and should be re-scoped rather than built;
watching-discipline surfaces simplify to socket queries. Worktree lifecycle and build
budget/cache work are orthogonal and unaffected.
