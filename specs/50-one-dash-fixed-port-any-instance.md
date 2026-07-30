# 50 - One dash, one known address, any rigger instance

**Goal:** the dashboard stops being a per-run process on a drifting port and becomes ONE machine-level
dash at ONE known address - `http://localhost:7420` - that gives visibility into ANY rigger instance:
every local project's runs, and any configured shared store. Today the dash binds the first FREE port
from 7420 upward (so the address drifts to 7421, 7422, ... and the operator never knows which), it is
started per-run and scoped to that run's project, and it self-reaps with the run - so "where is my
dash?" has a different answer every run, and there is no way to see two projects, or a teammate's runs
on a shared store, in one place. The dash is a read-only PROJECTION over event stores; it should
therefore attach to STORES, not to processes - which makes "see any instance" a discovery problem, not
a coordination problem.

## Design

### Fixed address, singleton process

- `rigger dash` binds the DEFAULT PORT, period - no free-port search. If the port is already held by a
  rigger dash, the new invocation does not start a second one: it reports the existing dash's address
  and exits 0 (the singleton is the point). An explicit `--port` still overrides for a genuine
  conflict, but the default address is stable and bookmarkable.
- The dash process is machine-level, not per-run: it serves every registered instance (below) and
  outlives any single run. Its self-reap trigger changes from "my run went idle" to "NOTHING has been
  registered or alive for the idle window" - so it lingers across back-to-back runs and across
  projects, and still cleans itself up on a quiet machine.

### The instance registry (discovery)

- Every `rigger` invocation that starts or advances a run REGISTERS its instance in a machine-global
  registry under the user's state directory (`~/.local/state/rigger/instances/<id>` or the platform
  equivalent): the project root, the resolved event-store (the local `.rigger` path, or the configured
  shared-store identity WITHOUT credentials), and a heartbeat it refreshes while alive. Entries whose
  heartbeat goes stale are pruned by any reader. The registry is discovery metadata ONLY - never a
  source of truth, never credentials.
- The dash's LANDING view lists the registry: every live (and recently live) instance on the machine,
  plus any shared-store endpoints from the current project's store configuration - so a team member's
  dash shows the shared store's runs alongside their local projects.

### Attach to stores, not processes

- Selecting an instance attaches the dash to that instance's STORES, read-only, through the same lazy
  per-request providers the graph views already use: the run views project that instance's event log;
  the knowledge-graph views open that instance's local graph projection. A shared-store instance
  resolves its connection exactly as every command does (the store-resolution authority), so the same
  config that lets a worker report to the shared store lets the dash read it - and each user's dash
  projects the shared log LOCALLY (no hosted dashboard service, no new write path, no dash-to-dash
  protocol).
- Run-agnostic by construction: an instance with no active run still shows its event history and its
  knowledge graph (the graph views already read the projection directly); an empty store renders an
  empty state, never an error.

### Auto-ensure with a clean opt-out

- The step path keeps its always-on promise, retargeted at the singleton: a starting run ENSURES the
  machine dash is up (best-effort, warn-only on failure) and registers its instance - it never spawns
  a second dash and never changes the address. The existing session-detachment is preserved.
- The opt-out is first-class: a config key (`dash: off`) or the existing environment disable suppresses
  the ensure entirely - a headless or CI run proceeds with NO dash, NO port bind, and no degradation
  (a run never needs a dash, and a dash never needs a run).

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any external tool
  or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- The dash stays READ-ONLY over every store it attaches to; it adds no event type and no write path.
  The registry is discovery metadata only, holds no credentials, and its loss is harmless (it
  repopulates as instances heartbeat).
- The dash charter holds: one self-contained page, no external assets; wide content scrolls within its
  own cell.
- Secrets discipline: a registered shared-store identity and every rendered instance label carry NO
  credentials (the redaction rules of the store-resolution authority apply everywhere the dash prints
  a connection).
- Backward compatible: a project with no registry entries and no shared-store config sees exactly
  today's single-project dash content at the fixed address; the per-run ensure keeps working with the
  same opt-out env.

## Done when

- [ ] a test proves the FIXED ADDRESS + SINGLETON: `rigger dash` binds the default port with no
  free-port search; a second invocation while one is serving does not bind a second port - it reports
  the existing address and exits cleanly. This criterion OWNS the stable address.
- [ ] a test proves the REGISTRY lifecycle: starting/advancing a run registers the instance (project
  root + credential-free store identity) with a heartbeat in the machine-global state directory;
  a stale heartbeat is pruned by a reader; the registry never contains a credential. This criterion
  OWNS discovery.
- [ ] a test proves the LANDING + ATTACH flow: the dash's landing view lists registered instances,
  and selecting one serves THAT instance's run and graph views read-only through per-request store
  opens - including an instance with no active run (history + graph, empty-state degrade). This
  criterion OWNS multi-instance attach; it does NOT own the registry (criterion 2).
- [ ] a test proves AUTO-ENSURE + OPT-OUT: the step path ensures the singleton (never a second dash,
  never a changed address) and registers its instance; with the config or env opt-out set, NO dash is
  started and NO port is bound while the run proceeds normally. This criterion OWNS the ensure/opt-out
  behavior.
- [ ] a test proves the SELF-REAP retarget: the singleton reaps itself only when no registered
  instance has a live heartbeat for the idle window - it survives one run ending while another
  project's run is live. This criterion OWNS the singleton lifecycle; it does NOT own the ensure
  (criterion 4).
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
