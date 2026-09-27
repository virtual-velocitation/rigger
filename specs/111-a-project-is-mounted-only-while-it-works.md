# 111 - A project is mounted only while it works

**Goal:** each project's hive memory, the graph that holds its whole understanding and the log
that persists it, costs the machine only while the project works. Today a project's store is
open whenever any process wants it, and nothing measures or
bounds what that costs: the kernel's out-of-memory report of 2026-09-24 chose an 8.5 GB
`rigger step` while eight test children held 5.5 GB each, and no rigger surface had refused,
budgeted or reported any of it. Rigger runs many projects on one machine; today two projects'
costs are simply whichever process opens the larger store. This spec ships sections 6 and 7
of docs/architecture-addendum-the-owned-store.md: a project is MOUNTED only while it has
workloads, a cold read never mounts, and every mount is admitted against a measured
machine-wide budget or refused with a named remedy.

## Design

THE OWNER, decided: the process that holds `.rigger/store/OWNER` (exclusive advisory lock)
is the mount owner: the detached supervisor (spec 106) for a run, or `rigger mcp` / `rigger
dash` serving an interactive session. Only the owner does structural work (seal, archive,
compaction, base rewrite, manifest). A process that cannot take the lock is a client.

MOUNT, decided: taking the OWNER lock, opening the manifest, loading indexes and the resident
graph, taking the project's machine-scope slot `$XDG_STATE_HOME/rigger/machine/mounts/<project-id>.slot`
(an exclusive lock held for the mount's life; `$XDG_STATE_HOME` is the registry's state home,
`src/registry.rs:109`, `~/.local/state` by default) and writing into it, every 30 s alongside the
supervisor beat, the owner's identity `(pid, start-time, boot-id)`, the mount time and its
measured resident bytes (RSS now minus RSS at mount). A slot whose lock is free is a stale
row; its content is informational only.

WORKLOAD, decided: a live run (not terminal, not held with cause `stopped`), a live spawn, a
held step lock, or a client session heartbeat younger than 90 s. UNMOUNT happens when no
workload has existed for `idle_unmount_after` (default 600 s): the owner rewrites the graph
base, releases the slot, closes every file, releases OWNER and, if it is a supervisor whose
run is done, exits.

COLD READ, decided: every one-shot command and every hook reads the manifest, sealed segments,
base and deltas directly and never mounts; a write takes the JOURNAL lock for its batch. The
one-shot path is pinned by a test that runs `rigger status`, `rigger peers` and `rigger
emit` on an unmounted project and asserts no OWNER lock and no slot were taken.

THE BUDGET, decided: `$XDG_STATE_HOME/rigger/machine/budget.toml` with `resident_budget_bytes`
(default 10% of `MemTotal`), `per_project_resident_bytes` (default 2 GiB),
`max_mounted_projects` (default 4) and `idle_unmount_after_s` (default 600). Admission at
mount sums the resident bytes of every slot whose lock is held plus the candidate's expected
bytes (its base size plus footer sizes) and refuses when any limit would be exceeded. A
refused supervisor appends spec 105's `RunHeld {cause: "resources", detail}` naming the mounted
projects, their bytes, the budget and the remedy, and retries on the release cadence; idle
mounted projects are asked to unmount first through their slot (a `wanted` flag the owner
honors on its next beat). Nothing is ever unmounted while it has a workload.

MAINTENANCE, decided: `$XDG_STATE_HOME/rigger/machine/maintenance.lock` is held for the duration of
any archive, compaction or base rewrite, machine-wide, so two projects never do structural
work at once.

WHAT A PERSON SEES, decided: `rigger store status` lists every held slot (project id, root,
resident bytes, mounted since, owner identity), the budget and its headroom, and any
`wanted` flag; `rigger status` on a held-for-resources run opens with
`HELD <duration> - resources - <detail>`.

CONSTRAINTS WALK: owner crash - OWNER and slot locks drop with the process; the next mount
finds a stale slot row and overwrites it. Two supervisors on one project - the second cannot
take OWNER and refuses, naming the first's identity. Project root moved - the slot is keyed by
project id, so the next mount re-derives the path and the stale row is overwritten. Machine
reboot - every lock is free; the first mount recomputes. Budget lowered below current use -
no eviction; new mounts refuse until idle unmounts free room. Client heartbeat from a dead
session - expires at 90 s. Cold read during a base rewrite - the manifest names old files
until the rename.

## Notes (non-criteria)

The slot file is a small TOML the world-authority addendum's machine-scope substrate can adopt
unchanged; content is never project data. The instance registry (`src/registry.rs`) keeps
its discovery role and its 900 s heartbeat; the slot, not the registry, is what admission
reads, because a slot is proven by a held lock and a registry row is not. The dashboard,
serving many projects, is a client of every one of them and keeps at most four mapped bases. Expected bytes for admission are an estimate only
until the first beat publishes the measurement; the first beat corrects the slot.

## Global constraints

- Hyphens, never em dashes. Both feature lanes and the core lane green (fmt, clippy -D
  warnings, test); no-os-kill and reap audits green.
- No new event type (the hold reuses spec 105's `RunHeld`); no new crate dependency.
- Nothing is unmounted, closed or evicted while the project has a workload; the kernel is
  never left to choose.
- Machine-scope files carry no project content; every path derives from the project root.
- The operator's installed rigger binary is never replaced or modified by any unit.

## Done when

- [ ] a test proves THE MOUNT LIFECYCLE: a supervisor mounts on run start (OWNER held, slot
  held, resident bytes published within 30 s), stays mounted while any workload exists, and
  unmounts after the idle window releasing both locks and closing every file. This criterion
  OWNS mount, workload and unmount; admission is criterion 3's, NOT this one's.
- [ ] a test proves COLD READS NEVER MOUNT: `rigger status`, `rigger peers` and `rigger emit`
  on an unmounted project answer and append without taking OWNER or a slot, and a second
  supervisor on a mounted project refuses naming the owner. This criterion OWNS the
  client/owner distinction.
- [ ] a test proves ADMISSION: with a budget equal to two mounted projects' bytes, a third
  mount is refused, its run holds with cause `resources` and a detail naming projects, bytes,
  budget and remedy, an idle mounted project is asked to unmount and the retry then succeeds.
  This criterion OWNS `budget.toml`, the sum and the refusal.
- [ ] a test proves MAINTENANCE IS SERIAL: two projects' archive operations started together
  run one after the other under the machine lock. This criterion OWNS the lock.
- [ ] a test proves WHAT A PERSON SEES: `rigger store status` lists slots, bytes, budget and
  headroom, and `rigger status` on a resources-held run opens with the HELD line. This
  criterion OWNS the two surfaces.
- [ ] both feature lanes and the core lane green (fmt, clippy, test).
