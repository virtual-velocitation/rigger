# 49 - Ingest at pace: parallel, incremental, and scoped

**Goal:** the per-step ingest must stop being the loop's slowest, most fragile phase. Measured on this
repository (32 logical cores): a COLD `rigger graph build` - parse + emit + fold only, no embedding -
takes 8m44s for 36,022 code-ingest events (~69 events/s), running essentially single-threaded (a
99-thread process with one busy core); and the grounding index's embed freshen re-processes content
that has NOT changed after a binary reinstall, adding ~10-15 minutes more. A first step that exceeds
the driver's per-command time budget stalls the whole run, so today the operator must hand-warm the
graph before firing a loop - operator lore that no consumer has. Fix the phase itself, three ways that
compose: PARALLELIZE the work across cores, make the incremental skip HONEST (content-keyed, so
unchanged content is never re-processed, regardless of which binary runs), and SCOPE the walk to the
project's own sources (which also removes the tooling-directory noise from the knowledge graph). All
three ship in the binary; none may change what the ingest PRODUCES - the same tree yields a
byte-identical graph and index.

## Design

### 1. Parallel parse, ordered emit, batched fold (`src/ingest.rs`, the code-ingest fold)

The walk currently parses, emits, and folds one file at a time, one event at a time. Split the
pipeline:

- **Parse in parallel.** File parsing (the symbol extraction) is pure per-file work - fan it across a
  worker pool sized to the machine. Parsing has no ordering requirement.
- **Emit in deterministic order.** Results are emitted in SORTED FILE-PATH order regardless of which
  worker finished first, so the event log and the fold see exactly the sequence a serial walk would
  have produced - parallelism must be observationally invisible (rigger's rebuild-byte-identical
  discipline).
- **Batch the store work.** One append per FILE'S BATCH (the batch is already keyed
  `<prefix>/<file>@<hash>#<i>`), and the fold applies a file's batch in one transaction - not one
  transaction per event. The measured 69 events/s is transaction-cadence-bound, not parse-bound.

### 2. Honest incremental skip for the embed freshen (`src/grounder/turbovec.rs`)

The grounding index's freshen must re-embed a chunk ONLY when that chunk's CONTENT changed:

- The skip key is a pure function of chunk content (and the embedding model's identity), never of the
  binary's build id, install time, or index-file mtime - a reinstall with an unchanged tree embeds
  ZERO chunks.
- Chunks that genuinely drifted embed in BATCHES (one model invocation per batch, not one per chunk),
  so the accelerator is fed at its width instead of chunk-by-chunk.
- The existing behavior - `reindex` re-embeds exactly the named files; the freshen never double-embeds
  them - is preserved.

### 3. Scope the walk to the project (`src/ingest.rs` - and this is de-noise, not just speed)

The walk today has NO exclusions: it descends into VCS internals, rigger's own runtime directory, tool
caches, and build outputs, and it can escape the repository root through parent-relative paths - the
live graph shows clusters for the tooling directories and even paths above the root. Scope it:

- **Respect the project's ignore rules.** The walk skips everything the repository's own
  version-control ignore rules exclude (build outputs, caches, vendored artifacts - whatever the
  project already declared as not-source), plus the always-excluded set that is never source: the VCS
  metadata directory and rigger's own runtime directory.
- **Never escape the root.** Every walked path must resolve inside the canonicalized project root;
  symlinks or parent-relative entries that point outside are skipped, so the graph can never grow
  clusters for paths above the repository.
- Scoping shrinks the work set for BOTH phases above (fewer files to parse, fewer chunks to embed) and
  removes the tooling-path noise from the knowledge graph - the same target-project-only principle the
  node-kind de-noise applied at the fold, now applied at the walk.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any external tool
  or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features` (the ingest walk and its scoping
  serve in both lanes; the embed freshen rides the grounding feature as today).
- Determinism by construction: parallel parse feeds an ORDERED emit, so the same tree produces a
  byte-identical event sequence, graph, and index as a serial walk - parallelism is observationally
  invisible. Any worker-pool scheduling nondeterminism is confined to timing.
- The event log stays the source of truth; batching changes transaction CADENCE, never event content
  or order. The content-key replay-skip contract (`<prefix>/<file>@<hash>#<i>`) is unchanged.
- The ingest's OUTPUT is unchanged: same events, same graph rows, same index entries for the same
  tree - this spec changes cost and scope, not meaning. (Newly-excluded tooling paths stop being
  ingested; that scoping is the one intended output change, and a rebuild collapses their old rows.)

## Done when

- [ ] a test proves PARALLEL PARSE WITH ORDERED EMIT: ingesting a multi-file fixture engages more than
  one parse worker, and the emitted event sequence is byte-identical to the serial walk's (sorted
  file-path order, stable batch keys). This criterion OWNS the parallel pipeline and its determinism.
- [ ] a test proves BATCHED FOLD CADENCE: ingesting a K-file fixture performs one append per file
  batch and folds each batch in one transaction (not one per event). This criterion OWNS the batching;
  it does NOT own parallelism (criterion 1).
- [ ] a test proves the HONEST EMBED SKIP: a freshen over an unchanged tree embeds zero chunks even
  when the index was written by a different binary identity; a freshen after one file changes embeds
  only that file's chunks, in batched model invocations (counted via the test embedder). This
  criterion OWNS the content-keyed skip and embed batching.
- [ ] a test proves the SCOPED WALK: a fixture tree containing an ignored build directory, the VCS
  metadata directory, rigger's runtime directory, and a symlink escaping the root ingests NONE of
  them while ingesting every in-root source file; the resulting graph contains no cluster for the
  excluded paths. This criterion OWNS the walk scope and root confinement.
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
