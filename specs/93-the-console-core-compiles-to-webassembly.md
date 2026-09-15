# 93 - The console core compiles to WebAssembly

**Goal:** the projection code the conductor runs is already pure Rust - `src/ledger.rs`,
`src/metrics.rs`, `src/progress.rs`, `src/blocker.rs`, `src/run.rs`, `src/community.rs`,
`src/concepts.rs`, `src/contextgraph/mod.rs` and the fold half of `src/spawn.rs` import no
`rusqlite`, `tokio`, `std::fs`, `std::process` or `std::net` - yet the library cannot be built
without its I/O dependencies, because every one of them (`rusqlite`, `kurrentdb`, `tokio`,
`rustix`, `fs2`, `ignore`, `uuid`) is unconditional in `Cargo.toml`. So the dashboard page
reimplements the fold and the graph queries in JavaScript (`src/dash.html`, 2,398 lines) over
payloads `src/dash.rs` (11,240 lines) computes, and a fourth copy of the fold is written every
time an operator scripts against the store. The Mission Control console
(docs/architecture-addendum-mission-control.md) requires the library's own fold and graph
queries to run inside the page, which means the pure subset must build for
`wasm32-unknown-unknown` and expose a small, dependency-free calling surface.

## Design

THE FEATURE SPLIT, decided: a `store` feature, carried in `default`, gates every module that
touches a file, a socket, a process or the clock - the event-store backends
(`eventstore::sqlite`, `eventstore::kurrentdb`, `contextgraph::sqlite`), the async runtime,
`reap`, `worktree`, `liveness` (marker files), the process half of `spawn`, `gate` execution,
`dash` serving, `watch`, `registry`, `hooks`, `sidecar`, the binary. A `core` feature names the
pure subset: `ledger`, `metrics`, `progress` (the fold), `blocker`, `spawn` (the model and
`step_result`), `run` (base and release-ready), `config` (parsing), `contextgraph` (model and
queries), `community`, `concepts`, and a new `console` module holding the view models the
console renders (theater, agent transcript, courtroom, plan, brief, dock, health, next steps,
statusline, palette commands, scrub track) and the map engine spec 84 defines. A module is
either wholly in `core` or wholly behind `store`: where a file holds both (today `spawn.rs`),
it is split into two modules at file level, never gated function by function. Today's two
lanes build exactly what they build now; a third lane, `--no-default-features --features
core`, builds the pure subset natively and for the wasm target.

THE PURITY RULE, decided: a `core` module imports none of `std::fs`, `std::process`,
`std::net`, `std::time::SystemTime::now`, `rusqlite`, `tokio`, `rustix`, `fs2`, `ignore` or
`uuid`; every time and age the core needs is an input. An audit test enforces the rule by
reading the sources, and the core lane's build enforces it by compiling.

THE MEMBER CRATE, decided: `crates/console-core`, a `cdylib` in a new workspace whose members
are the root crate and this one, depending on the library with `default-features = false,
features = ["core"]`. It exports exactly three functions: `console_alloc(len) -> ptr`,
`console_free(ptr, len)`, and `console_call(op_ptr, op_len, in_ptr, in_len) -> u64` returning
the packed pointer and length of a JSON reply. The ops are `fold_reset(events)`,
`fold_push(event)`, `fold_at(position)`, `view(name, params)`, `graph_load(payload)`,
`graph_query(kind, params)`, `map_build(w, h)`, `map_frame(camera, selection)`, `map_hit(x,
y)`, `scrub_track()`, `statusline()`, `palette_commands()`. Requests and replies are JSON via
`serde_json`; module state is one thread-local `Console` value (a page's module is
single-threaded). An unknown op, malformed input or an unsatisfiable query returns an error
reply (`{"error": ...}`); a panic inside the core is a defect, never a documented outcome. The
same three functions compile natively as ordinary crate functions, so the ABI contract is
tested with plain Rust tests and needs no WebAssembly runtime as a dependency.

THE BUILD, decided: the root crate's `build.rs` compiles the member crate for
`wasm32-unknown-unknown` in release mode with `--locked` into a target directory of its own
under `OUT_DIR` (its own directory so it never contends for the outer build's lock, with the
outer build's `RUSTFLAGS` and wrapper cleared), and the library embeds the artifact with
`include_bytes!`; `rigger dash` serves it at `/console/core.wasm` as `application/wasm`. The
script reruns only when a `core` module or the member crate changes. When the target is not
installed the build fails with a message that names the exact command, `rustup target add
wasm32-unknown-unknown`; it never skips the module silently. The target is an operator install
locally and a workflow-level install in `.github/workflows/rust.yml`; no unit installs it.

ONE FOLD, decided: `rigger status` prints its first line and its needs-you lines from
`console::statusline` and `console::dock`, so the terminal and the page are one authority; a
parity test folds a recorded stream through `console::fold` and through the status
projection and asserts identical unit statuses, blockers, attention entries and statusline.

BUDGETS, decided: the module is under 3 MB; a fold of 10,000 console events completes in
under 16 ms natively in release mode (the page-side bound follows from the same code).

CONSTRAINTS WALK: target missing - the named error, no silent skip. `--no-default-features`
alone - unchanged (light lane, no core). Core lane natively - builds and its tests run on the
host; the wasm artifact is only ever produced by the nested build. Mutation sweeps - the
nested build reruns only on core-file mutants; other mutants never pay it. Clock - the core
has no `now()`, every age arrives as an input. Crash resume - the module holds no persistent
state; a page re-sends the snapshot.

## Notes (non-criteria)

The `console` view-model functions are consumed by specs 94-98; this spec ships them with
their types and the fold, proven by the parity and ABI tests, and no page. The CLI keeps
`rigger graph`'s text renderers; only the query functions move into the core surface. The
JavaScript loader that calls the ABI is spec 94's.

## Global constraints

- Hyphens, never em dashes. Both feature lanes green (fmt, clippy -D warnings, test, default
  and --no-default-features) plus the core lane (`--no-default-features --features core`,
  native build and test); no-os-kill and reap audits green.
- No new event type; no new crate dependency (a build target is not a crate). No binding
  generator.
- The operator's installed rigger binary is never replaced or modified by any unit.

## Done when

- [ ] a test proves THE CORE LANE IS PURE: `--no-default-features --features core` builds the
  library natively and for `wasm32-unknown-unknown`, an audit over the sources finds no `core`
  module importing the banned set, and `default` still carries `store` so today's two lanes are
  byte-for-byte unchanged in behavior. This criterion OWNS the feature split, the module split
  of `spawn`, and the purity audit; the member crate is criterion 2's, NOT this one's.
- [ ] a test proves THE MODULE AND ITS ABI: the member crate exports exactly `console_alloc`,
  `console_free` and `console_call` (proven by parsing the artifact's export section), the
  twelve ops answer with JSON replies, an unknown op and malformed input answer with an error
  reply, and the ABI contract passes the same tests compiled natively. This criterion OWNS the
  member crate and the op set; the embedding is criterion 3's, NOT this one's.
- [ ] a test proves THE BUILD EMBEDS IT: the bytes served at `/console/core.wasm` equal the
  artifact the nested build produced for this tree, the artifact is under 3 MB, the build
  script's missing-target path produces the message naming `rustup target add
  wasm32-unknown-unknown`, and the CI workflow installs the target at workflow level. This
  criterion OWNS the build script, the embedding, the route and CI; the ops are criterion 2's,
  NOT this one's.
- [ ] a test proves ONE FOLD: for a recorded stream, `console::fold` and the status projection
  agree on unit statuses, blockers, attention entries and the statusline, and `rigger status`
  prints its first line and its needs-you lines through the `console` functions. This criterion
  OWNS the parity and the CLI's use of the core; the graph ops are criterion 5's, NOT this one's.
- [ ] a test proves THE GRAPH QUERIES ARE EXPORTED: `graph_load` accepts the map payload and
  `graph_query` answers neighborhood, card, path, communities and search with the same results
  the library's query functions return for the same graph. This criterion OWNS the graph ops
  only; the map engine is spec 84's, NOT this one's.
- [ ] both feature lanes and the core lane green (fmt, clippy, test on default,
  --no-default-features, and --no-default-features --features core).
