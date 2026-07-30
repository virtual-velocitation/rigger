# 45 - Graph inspector read-path foundation: direct-projection provider, cold-checkout build, indexes

**Goal:** lay the read-path foundation the inspector's lenses and call queries stand on. Today the
dash's graph views read a RUN-SEEDED depth-2 subgraph pre-fetched on every 1.5-second state poll
(`dash_read_graph` -> `graph_seeds` -> `subgraph(seeds, 2)`), which has two disqualifying limits for an
inspector: it only ever shows the neighborhood of the current run's units/decisions, so on a repo the
tool has merely INDEXED (never built) `graph_seeds` is empty and the panel shows nothing at all; and
re-reading a subgraph on every poll is the wrong cadence for whole-graph and (future) traversal reads.
Give `/api/graph` its OWN provider that opens the projection lazily - only when a graph request arrives,
never on the state poll - and reads the whole projection directly, so the overview, the seeded
neighborhood, and the coming call/lens views all work on any repo with a graph. Add a `graph build`
entry so the graph can be populated from source alone, and the two additive indexes that keep
whole-graph and directed reads sub-linear on large repositories. This spec adds query and ingest
surface only; it does not change the fold or add an event type.

## Design

### A dedicated, lazy graph provider

`serve` today receives one `provider` closure (`src/main.rs`) that `dash_read_graph` fills with a
run-seeded `subgraph(graph_seeds(events), 2)`, and `route` (`src/dash.rs`) serves every `/api/*` path -
including the state poll and the graph views - purely over that one pre-fetched `DashInputs` tuple. The
state poll (`/api/state`, `/api/events`) runs every 1.5s and does NOT need the graph; the graph views
run only on panel load, a drill, a lens flip, or a call query.

Split the two so a whole-graph read never rides the poll:

- The polled `provider` keeps producing the cheap run-scoped inputs for `/api/state` and `/api/events`
  (unchanged cadence, unchanged cost).
- `/api/graph` gets a SEPARATE provider - a closure that opens the `Projector` and reads the projection
  only when a graph request actually arrives. It reads the graph DIRECTLY (the whole live projection,
  or the seeded/overview slice the request asks for), NOT through `graph_seeds`. The existing
  read-only, per-request-open discipline is preserved (the dash still starts before the store exists;
  an absent graph degrades to an empty result, never an error).
- Every existing graph view moves onto this provider unchanged in behavior: the whole-graph clustered
  overview, `cluster_detail` drill, and the seeded `neighborhood`/`graph_json` all read through it. The
  seeded-neighborhood view in particular STOPS depending on `graph_seeds`, so a seed that names any
  node in the projection resolves - fixing the never-built-repo dead-end where `graph_seeds` is empty
  and `dash_read_graph` returns `Graph::default()`.

### Cold-checkout `graph build`

Ingest today is reachable only from inside a run (the conductor's `ingest_project_into_graph`). Add a
`rigger graph build` subcommand that folds the project's source into `.rigger/graph.db` from a cold
checkout - no run, no event beyond the code-ingest events the fold already emits - so the graph exists
on any repo the tool has only cloned. It reuses the existing symbol extraction and code-ingest fold
(the same path the run uses); it only adds the standalone ENTRY. On a repo with no `.rigger` yet it
creates the store; on an existing one it refreshes incrementally.

### Additive indexes

Add two indexes through the existing additive-migration pattern (the `column_exists`-style guarded
`CREATE INDEX IF NOT EXISTS` in `src/contextgraph/sqlite.rs`), so whole-graph reads and the directed
call traversal (spec 46) stay sub-linear as a repository grows:

- a PARTIAL index on the live-edge relation - `edges(rel, from_id) WHERE valid_to IS NULL` - for the
  relationship-scoped forward scan;
- an EXPRESSION index on the entity-name suffix - the exact `substr(id, instr(id, '::') + 2)`
  expression the fold and the coming cross-file resolution use - so name resolution hits the index
  rather than a full scan. The resolution query must be phrased with the identical expression or it
  silently misses the index; this spec adds the index and pins that expression.

Both are pure additions (no column, no data migration); a fresh database and an existing one both gain
them idempotently.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any external tool
  or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`. The graph provider and `graph build`
  serve/run in BOTH lanes; with the KG feature off or an empty graph they degrade to an empty result,
  never an error.
- Read-only over the projection for the provider; `graph build` writes only through the existing
  code-ingest fold (no new event type, no new mutation path). The event log stays the source of truth;
  the graph is a rebuildable projection.
- Determinism by construction: the direct-projection reads sort deterministically (the overview/drill
  already fold over `BTreeMap`/`BTreeSet`); `graph build` is deterministic given a tree; the indexes do
  not change results, only speed.
- The state poll's cost is NOT increased: the whole-graph read happens only on a graph request, never
  on `/api/state` or `/api/events`.
- Spec 30/42 graph views are NOT regressed: the overview, drill, and seeded neighborhood return the
  same shape they do today; only their data source (direct projection, not run-seeded pre-fetch) and
  reach (any node in the projection) change.

## Done when

- [ ] a test proves the LAZY GRAPH PROVIDER: a `/api/graph` request opens the projection and returns a
  graph-derived body, while a `/api/state` (or `/api/events`) request does NOT trigger a whole-graph
  read - the graph provider is consulted only on graph requests. This criterion OWNS the provider
  split.
- [ ] a test proves DIRECT-PROJECTION REACH: the seeded-neighborhood and whole-graph overview read the
  projection directly, so on a graph whose `graph_seeds(events)` is EMPTY (an indexed-but-never-built
  repo) a seed naming a real node still returns its neighborhood and the overview still returns its
  clusters - not `Graph::default()`. This criterion OWNS the never-built-repo fix; it does NOT own the
  provider split (criterion 1).
- [ ] a test proves COLD-CHECKOUT BUILD: `rigger graph build` folds a source tree into `.rigger/graph.db`
  with no run - producing the code-entity nodes and `CALLS`/`REFERENCES` edges the fold emits - so the
  graph is populated from source alone. This criterion OWNS the standalone build entry.
- [ ] a test proves the INDEXES EXIST AND ARE USED: after migration the partial live-edge index and the
  entity-name expression index are present, and a name-resolution query phrased with the pinned
  `substr(id, instr(id,'::')+2)` expression uses the expression index (verified via the query plan or
  an equivalent index-presence assertion), in BOTH feature lanes. This criterion OWNS the index
  migration and the pinned expression.
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
