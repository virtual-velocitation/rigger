# 108 - The hive consolidates upward: history by evidence, digests by altitude

**Goal:** the hive's graph holds the project's whole understanding and serves each persona the
slice it needs; the hive never loses understanding, it moves understanding to history when
the evidence says so and to a higher altitude when a run is over. Today it forgets by age. The
log holds 11,791 `DecisionMade` events; the live graph holds 403 decision nodes, because
`rigger reset --runs` (`src/main.rs:9154`, `superseded_graph_nodes` at `main.rs:9231`,
`Projector::prune` at `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:173`) drops every dead run's decisions and
findings from the graph on the run boundary alone, keeping only `LessonLearned`. A decision whose code is
still in the tree is forgotten with its run; a decision whose code is long gone stays live
until its run dies. Spec 25 already moves RESOLVED findings into history by disposition
(`invalidate_finding_edges`, `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:1775`, from the adjudication arm at
`:1497` and the integration arm at `:1138`), the right shape, for one kind only; a file's own
structural edges are already retired when a fresh generation lands (`supersede_file_edges`,
`sqlite.rs:1814`) and a superseding decision retires `GOVERNS` (`sqlite.rs:1055`). Nothing consolidates: no code folds decisions or findings into a
digest at any altitude. Grounding knows two altitudes,
`GroundingSlice::Implement` and `GroundingSlice::Full` (`crates/rigger-conductor/src/conductor.rs:12788`), and both
serve raw file-level items. `DecisionMade` carries two kinds of content under one type: durable
claims about the code and process records (a constraints recheck, a verdict, an accounting),
and nothing tells them apart: 11,280 of the 11,791 carry `governs`, and 6,441 (55%) carry a
review-tier id prefix (`sdet-` 2,205, `adj-` 1,794, `adv-` 1,383, `arch-` 996, `pc-` 63).

## Design

**FORGETTING BY AGE ENDS.** `rigger reset --runs` keeps its archive role (spec 107) and stops
pruning decisions and findings from the graph. What leaves the live view leaves by one of the
two operations below, each citing its evidence.

**SUPERSESSION INTO HISTORY, ON EVIDENCE.** A decision, lesson or finding whose EVERY governed
or about file has a blank-blob `GenerationIngested` (spec 107: the file was deleted) and no
rename successor (the ingest pass records `renamed_from` on the successor's first generation
when git detects the rename) gets `valid_to` on its `GOVERNS`, `ABOUT` and `RAISED` edges at
the position of the ingest that observed the last deletion, in the fold arm that already
retires the file's own structural edges (`supersede_file_edges`, `sqlite.rs:1814`), through
the same edge-invalidation authority spec 25 uses (`invalidate_finding_edges`, generalized to
the three node kinds). A node with one surviving file stays live. `rigger peers
--historical` returns superseded items with the evidence that superseded them (the deleting
generation, the superseding decision, or the disposing adjudication). Deletion of a node or
edge stays impossible outside `reset --derived`'s rebuild.

**PROCESS RECORDS ARE EPISODES.** A `DecisionMade` is a process record when its `id` carries a
review-tier prefix the personas already stamp (`adj-`, `adv-`, `arch-`, `sdet-`, `pc-`); it is
a durable decision otherwise (implementer, planner and operator decisions included: `governs`
does not discriminate, 96% of all decisions carry it). The prefixes are constants the personas
and the classifier share, so a renamed tier cannot silently reclassify. The classifier is one function beside
the event-class constants of spec 107 and is the single authority the fold, the archiver and
`rigger peers` consult. Process records fold into the graph for the run they serve and are
archived with the run's episodes by spec 107; durable decisions are knowledge and stay live
until superseded on evidence.

**THE CONSOLIDATOR WRITES UPWARD.** The workflow template gains a `consolidate` stage that the
conductor runs once at `RunStarted`, after spec 107's archive and before the first implement
wave, with a `consolidator` persona in `.rigger/agents/`. Its input is the durable decisions,
lessons and upheld findings that are live and older than the current run, grouped by the
concept each governed file realizes (`REL_REALIZES`, spec 54) and, for files outside every
concept, by community; plus the architecture documents and the open specs as the stated goals.
For each group it emits at most one `DecisionMade` at concept altitude whose `meta.distills`
lists the source ids, and at most one `LessonLearned` with `meta.generalizes: true` when a
group's lessons hold beyond this project. The fold links a digest to each source with a
`DISTILLS` edge; sources stay live and unranked-down for the implement slice. A group with one
source, or a group the consolidator judges not worth an altitude, emits nothing. The stage is
skipped, with a progress line saying so, when no live knowledge is older than the current run.
Its spawn is bounded like every other and a failure leaves the graph as it was.

**THE SLICE IS SERVED BY ALTITUDE.** `GroundingSlice::Full` renders digests first, under the
existing decisions budget, and lists the raw sources a digest distills only by id with the
`rigger peers` pointer; `GroundingSlice::Implement` is unchanged and file-level. Ranking inside
a slice is spec 92's. A persona therefore reads the project at the altitude of its task: the
planner and the review tiers at concepts, the implementer at files.

**PROCEDURAL MEMORY IS THE PLAYBOOK POOL.** `playbooks::rebuild` renders a `LessonLearned`
carrying `meta.generalizes: true` with a `ships: candidate` frontmatter line, and
`rigger playbooks --ship-candidates` lists them; moving one into the binary, a skill or the
workflow template remains the operator's act. No lesson leaves the graph by any other path.

**CONSTRAINTS WALK, decided.** Cold start: no live knowledge older than the run, the stage is
skipped. Repeat: a source already linked by a live `DISTILLS` edge is not re-digested; a digest
whose every source was later superseded is superseded with them. Crash-resume: the stage is a
spawn with a `SpawnResult`; a resumed run re-runs it only if no result was recorded.
Concurrent actors: the consolidator runs alone before the wave. Revert: a file restored after
deletion records a fresh generation; its superseded decisions stay in history and the
consolidator may re-raise them as a digest, never un-supersede them. Empty group: nothing is
emitted.

**STATE PLACEMENT.** Supersession lives in the graph's `valid_to` derived from log events; a
digest and its `DISTILLS` links are log events folded by `apply`; the process-record
classifier is a pure function of the event. A cached list of digested sources, a per-run file,
or a persona's own memory of what it consolidated is NOT an implementation of the repeat rule.

## Notes (non-criteria)

No new event type: digests are `DecisionMade` and `LessonLearned` with `meta.distills` and
`meta.generalizes`; `DISTILLS` and `renamed_from` are a relation constant and a ledger field.
The consolidator's judgment is qualitative and bounded to abstraction: it may create a digest
or decline to; it may never mark a source superseded, delete, or rewrite one. The persona's
prompt names the stated goals it judges against: `docs/architecture.md`, its addenda, and the
specs whose criteria are unchecked.

## Global constraints

- Hyphens, never em dashes, in every added line.
- No new event type; one new relation constant (`DISTILLS`); no new dependency.
- Both feature lanes green (fmt, clippy, test on default and --no-default-features).
- Nothing leaves the live view without a recorded reason a reader can follow to its evidence.

## Done when

- [ ] a test proves A DECISION WHOSE CODE IS GONE MOVES INTO HISTORY: after the ingest records
  blank-blob generations for every file a decision governs, its edges carry `valid_to` at that
  ingest's position, a decision with one surviving governed file stays live, a rename
  successor keeps the decision live, and `rigger peers --historical` names the deleting
  generation as the evidence, pinned at the fold arm. This criterion OWNS supersession on
  structural evidence; the age prune's removal is criterion 2's, NOT this one's.
- [ ] a test proves FORGETTING BY AGE ENDS: `rigger reset --runs` on a store with a dead run
  leaves that run's durable decisions and upheld findings in the graph and archives its
  episodes, and a `DecisionMade` is classified process record or durable decision by the one
  classifier from its id prefix and `governs`, pinned at that function. This criterion OWNS
  the prune's removal and the classifier.
- [ ] a test proves THE CONSOLIDATOR WRITES UPWARD: a run starting over live decisions older
  than itself, grouped under two concepts, runs the `consolidate` stage once, which records at
  most one concept-altitude `DecisionMade` per group with `meta.distills` naming its sources,
  the fold links each source by `DISTILLS`, sources stay live, an already-distilled source is
  not re-digested, and a run with nothing older than itself skips the stage with a progress
  line. This criterion OWNS the stage, the persona and the digest fold; how a slice renders
  digests is criterion 4's, NOT this one's.
- [ ] a test proves THE SLICE IS SERVED BY ALTITUDE: `GroundingSlice::Full` renders a digest
  before its sources and lists the sources by id under the peers pointer, within the existing
  decisions budget, while `GroundingSlice::Implement` renders exactly what it renders today.
  This criterion OWNS the slice rendering.
- [ ] a test proves GENERALIZABLE LESSONS ARE SHIP CANDIDATES: a `LessonLearned` with
  `meta.generalizes: true` rebuilds into a playbook carrying `ships: candidate`, and
  `rigger playbooks --ship-candidates` lists it and nothing else. This criterion OWNS the
  procedural surface.
- [ ] both feature lanes green (fmt, clippy, test on default and --no-default-features).
