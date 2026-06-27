# Rigger: Reference Architecture & Blueprint

> **Status:** Reference architecture. **[AS-BUILT] (implemented in Rust).**
> **Subject:** `Rigger`, a standalone, general-purpose, multi-agent development-loop
> harness, published as a public Rust crate (`cargo install --git https://github.com/virtual-velocitation/rigger`).
> **Scope:** the complete blueprint to reproduce the harness from scratch: the
> orchestration core, the declarative config model (agent files + workflow YAML),
> the event-sourced + context-graph memory layer, and the two pluggable seams
> (event store, agent driver).
> **Grounded against (2026-06-27):** the tank_game dev-loop harness it generalizes,
> `scripts/dev-loop/*.mjs` (conductor, plan, ledger, gates, autonomy, safety, learn,
> coordinate, turbovec), `.claude/workflows/dev-loop-{fanout,review}.mjs`,
> `.claude/workflows/review-and-remediate.js`, `tools/semantic-retrieval/`,
> `scripts/bd-*.sh`, the review-lens agent definitions, and
> `docs/superpowers/specs/2026-06-23-development-loop-design.md`. Plus the
> context-graph research corpus (Zep/Graphiti temporal KG, the TrustGraph context-graph
> manifesto, GraphRAG-vs-vector findings) and KurrentDB's event-sourcing model.
>
> **This is Rigger's canonical architecture doc.** It was drafted while building the
> tank_game dev-loop it generalizes, then moved into this repo. The proposed records in
> §12 (ADR-0001 + glossary) stay PROPOSALS until ratified at roadmap Phase 0; they are
> not yet written into `docs/adr/`.

---

## How to read this

Sections tagged **[AS-BUILT]** describe the proven tank_game dev-loop harness, the
*prior art* Rigger generalizes (it already runs; it built the engine inversion this
session). Sections tagged **[TARGET]** describe Rigger: the standalone, config-driven,
language-agnostic product. The single sentence that relates them:

> **Rigger is the *machinery* of the tank_game dev-loop, inverted the same way the
> engine was: every project-specific thing (Rust, cargo, bd, e7, Golden Apple) becomes
> user-supplied *content* (agent files, a workflow YAML, gate commands), and Rigger
> itself ships knowing none of it.**

The reader who wants the 5-minute version: §1 (what it is) → §2 (the picture) → §3
(the declarative model) → §5 (the memory ∞). The reader reproducing it: read all of it.

---

## 1. What Rigger is, and what it is not

**Rigger turns a *spec* into *integrated code* by orchestrating a fleet of AI agents,
and it remembers every decision they make in a self-reinforcing context graph so the
next agent is never blind to what the last one decided.** It is the *producing* loop
(spec → code); an adversarial *review* loop is a stage inside it.

**It is:**
- A **single Rust binary** (cargo-installable) + a **public Rust crate** (`cargo install --git …` / a library dependency).
- **Language-/project-agnostic.** It knows nothing about your build tool, test runner,
  tracker, or domain. You bring those as config.
- **Declarative.** The agents are **definition files**; the flow is a **workflow YAML**
  shaped like a GitHub Actions DAG. Reconfiguring the loop is an *edit*, never a recompile.
- **Memory-first.** An embedded **event store** (the append-only truth) projects a
  **bi-temporal context graph** (the queryable map) that scopes each agent's context to
  *exactly* its blast-radius and makes concurrent agents aware of each other's decisions.

**It is NOT:**
- Tied to Claude Code. The default agent driver shells out to the `claude` CLI; running
  *inside* Claude Code (with the Workflow tool) is an *optional* driver, not a requirement.
- Tied to a database server. The default event store is embedded SQLite (zero-dependency,
  single file). KurrentDB is an *optional* backend behind the same trait, built and shipped
  behind the `kurrentdb` cargo feature.
- Opinionated about your gates. A gate is "a command that must exit 0" plus an autonomy
  level. `cargo test`, `go test`, `pytest`, `npm test`, a custom lint: all just YAML.

### The inversion (why "no current config exists")

```
        tank_game dev-loop (AS-BUILT)              Rigger (TARGET)
   ┌─────────────────────────────────┐     ┌──────────────────────────────┐
   │ MACHINERY  (general)            │     │ MACHINERY  →  the Rigger crate │
   │  conductor · ledger · DAG ·     │ ══▶ │  (Rust: conductor, eventstore, │
   │  gates · autonomy · fan-out ·   │     │   contextgraph, drivers, …)    │
   │  review · context-graph(new)    │     └──────────────────────────────┘
   ├─────────────────────────────────┤     ┌──────────────────────────────┐
   │ CONTENT   (Golden-Apple-specific)│ ══▶ │ CONTENT  →  YOUR repo's config │
   │  cargo/e7 gates · bd federation ·│     │  agents/*.md · .rigger/*.yml · │
   │  Rust turbovec corpus · review   │     │  gate commands · grounding src │
   │  lenses · the S1 spec            │     │  (tank_game becomes one EXAMPLE)│
   └─────────────────────────────────┘     └──────────────────────────────┘
```

The tank_game harness already proved the machinery (it drove the ADR-0008 engine
inversion). Rigger is that machinery with the content cut out and replaced by a config
surface.

---

## 2. Architecture at a glance  **[AS-BUILT]**

```mermaid
flowchart TB
  subgraph CFG["📄 CONFIG (your repo - the only thing you write)"]
    AG[".rigger/agents/*.md<br/>(id · model · tools · prompt)"]
    WF[".rigger/workflow.yml<br/>(DAG: stages · needs · gates · autonomy)"]
  end

  subgraph CORE["⚙️ RIGGER CORE (Rust - the published crate)"]
    direction TB
    LOADER["config loader<br/>(parse agents + workflow → runtime DAG)"]
    COND["conductor<br/>(execute the DAG · sole state writer)"]
    LEDGER["ledger / projector<br/>(durable run state)"]
    GATES["gate engine<br/>(run · ratchet · autonomy)"]
    SAFE["safety<br/>(budget · remediate · escalate)"]
    LEARN["learn<br/>(failure → memory)"]
    COND --- LEDGER & GATES & SAFE & LEARN
    LOADER --> COND
  end

  subgraph SEAMS["🔌 PLUGGABLE SEAMS (traits, 2 impls each)"]
    direction LR
    ES["EventStore<br/>■ sqlite (default)<br/>○ kurrentdb (feature)"]
    DR["AgentDriver<br/>■ cli  (claude, default)<br/>○ workflow (MCP shim)"]
    GR["Grounder<br/>■ grep (default)<br/>○ turbovec (feature)"]
  end

  subgraph MEM["🧠 MEMORY"]
    LOG[("event log<br/>append-only, bi-temporal")]
    CG[("context graph<br/>projection: nodes+edges, validity intervals")]
    LOG -->|project| CG
  end

  CFG --> LOADER
  COND <-->|spawn agents| DR
  COND -->|append events| ES
  ES --- LOG
  LOG -->|subscribe \$all| COND
  CG -->|scoped subgraph| COND
  GR -->|top-k chunks| COND
```

**Two hard seams, one philosophy:** *the core depends on traits; the impls are
swapped by config / cargo feature:*

| Seam | Trait | Default impl | Optional impl | Why pluggable |
|---|---|---|---|---|
| **EventStore** | `append` / `read_stream` / `read_all` / `subscribe_all` | `sqlite` (embedded, 1 file) | `kurrentdb` (gRPC server, `kurrentdb` feature) | local zero-dep dev vs. multi-machine / scale; KurrentDB-shaped so the *contract suite* of the embedded impl is a faithful proxy for the server |
| **AgentDriver** | `spawn(agent, prompt, opts, emit) → result` | `cli` (`claude` subprocess) | `workflow` (MCP shim) | self-contained `cargo install` vs. in-Claude-Code parallel/journal/resume |
| **Grounder** | `ground(query, k) → Vec<Ref>` | `grep` (default) / `Nop` | `turbovec` (native vector search, `turbovec` feature) | a project may want semantic grounding (turbovec) or none |

---

## 3. The declarative model: the heart of "reconfigure by editing, not coding"  **[TARGET]**

Two file kinds, both in the *consuming* repo. Rigger reads them; it ships neither.

### 3.1 Agent definition files: `.rigger/agents/<id>.md`

Markdown-with-YAML-frontmatter (the format the tank_game review lenses already use,
`.claude/agents/*.md`), so existing agent defs port verbatim.

```markdown
---
id: implementer
model: sonnet
tools: [Read, Edit, Write, Grep, Glob, Bash]
isolation: worktree          # run in an isolated git worktree
recurse: false               # no Agent tool ⇒ cannot fan out (runaway-proof)
---
You implement ONE fully-specified finding inside your worktree. Write the failing
test first, confirm RED, implement minimally, confirm GREEN, run the named gates,
commit, push. Report the final line as JSON: {"id","pass","evidence"}.
```

```markdown
---
id: reviewer.architecture
model: sonnet
tools: [Read, Grep, Glob, Bash, LSP]
isolation: none
---
You review a diff for architectural defects ONLY. Quote the rule/doc violated.
Output the REVIEW schema: {verdict, issues:[{title,file_line,reason}]}.
```

The agent file is a **pure capability + persona declaration**, with no flow logic. The flow
references it by `id`.

### 3.2 The workflow YAML: `.rigger/workflow.yml`

GitHub-Actions-shaped: a DAG of **stages**, each with `needs:` edges, each binding an
**agent**, optional **gates**, and an **autonomy** level. *This* is the loop: the thing
that is hardcoded as `ground→plan→red→green→verify→review→integrate` in the tank_game
conductor becomes data anyone can rewrite.

```yaml
# .rigger/workflow.yml - a GitHub-Actions-style DAG for the producing loop
name: produce-from-spec
on: { spec: { path: "specs/**.md" } }      # what kicks off a run

defaults:
  autonomy: manual                          # manual | auto_notify | silent
  grounder: turbovec                        # grep (default) | turbovec (needs the cargo feature)

gates:                                      # reusable gate library (commands)
  build:   { run: "cargo build",                    kind: core }
  test:    { run: "cargo test",                     kind: core }
  lint:    { run: "cargo clippy -- -D warnings",    kind: elevated }
  custom:  { run: "./scripts/my-invariant.sh",      kind: elevated }

stages:
  plan:
    agent: planner
    produces: dag                           # decomposes the spec into a unit DAG
    coverage: required                      # block if a spec criterion has no unit

  implement:
    needs: [plan]
    agent: implementer
    strategy: fan-out                       # one agent per ready unit, in worktrees
    partition: by-blast-radius              # disjoint batches → safe parallelism
    gates: [build, test]                    # red→green enforced around these

  review:
    needs: [implement]
    strategy: fan-out
    agents: [reviewer.architecture, reviewer.technical]   # the lenses
    adjudicator: devils-advocate            # adversarial pass; verdict gates the stage
    autonomy: manual

  integrate:
    needs: [review]
    gates: [build, test, lint, custom]
    on_pass: merge                          # land + reindex + record
```

**The YAML → runtime mapping** (loader, §4.1): each `stage` becomes a node in the run
DAG; `needs` are the edges; `strategy: fan-out` + `partition` triggers the partitioner +
the AgentDriver per unit; `gates` are looked up in the `gates:` library and run via the
gate engine; `autonomy` seeds that gate/stage's ratchet. A stage with `produces: dag`
runs an agent whose output *extends* the run DAG (the living-DAG / `spawnUnit` mechanic).

### 3.3 Gates are config, not code

A gate is `{ run: <command>, kind: core|elevated|deferred }`. Rigger runs it, captures a
**compact summary** (verdict + ≤5 failing lines, capped), never the raw log, and feeds
that to the autonomy ratchet. `cargo test` / the e7 lexical check / `pytest` are all just
entries in a project's `gates:` map. Rigger ships **zero** gates.

---

## 4. The execution model: the conductor  **[TARGET, generalizing AS-BUILT]**

### 4.1 The pipeline, now *declared*

[AS-BUILT] the tank_game conductor hardcodes `Intake → Loop-readiness → Ground → Plan →
Coverage → Partition → Fan-out → Verify+Review → Integrate → Converge`
(`conductor.mjs`, `runLoop`). [TARGET] Rigger executes whatever DAG the workflow YAML
declares; the canonical pipeline above is simply the *default* workflow shipped as an
example.

```mermaid
flowchart LR
  S["spec"] --> RDY{loop-ready?\n(enumerable\nDone-when criteria)}
  RDY -->|no| BLK1["block: ask for criteria"]
  RDY -->|yes| G["ground each unit (JIT)\nvector + context-graph subgraph"]
  G --> P["run the DAG stage-by-stage\n(needs = edges)"]
  P --> COV{coverage gate\nevery criterion has a unit?}
  COV -->|gap| BLK2["block: plan missed a requirement"]
  COV -->|ok| PAR["partition ready units\n(disjoint by blast-radius)"]
  PAR --> FAN["fan-out: AgentDriver per unit\n(red → green → gates)"]
  FAN --> VR["verify + review\n(lenses → adjudicator)"]
  VR --> INT["integrate\ncommit · land · emit events · reindex"]
  INT --> CONV{converged?\nall criteria covered +\nall units integrated +\nall gates green}
  CONV -->|no| G
  CONV -->|yes| DONE["done (machine-verified)"]
```

**"done" is a machine-verifiable predicate:** every spec criterion covered + every unit
integrated + every gate green. Never "looks done."

### 4.2 Durable state: the ledger *is* a projection of the event log

[AS-BUILT] tank_game keeps a JSON ledger written solely by the Conductor; executors
append to a `.buffer` and the Conductor `drain()`s it (one-mutation-authority §6.8).
[TARGET] Rigger keeps that one-writer discipline but makes the ledger a **projection of
the event log** (§5): the run's state (units, coverage, gate history, autonomy) is
*derived* by folding the events, so a crashed/compacted run resumes by replaying. The
Conductor is the sole writer of *projections*; agents only ever *append events*.

```rust
// the conductor owns the run; agents never mutate shared state directly. RunState
// is projected from the event log by folding the run events (see `ledger`).
pub struct RunState {
    pub units: BTreeMap<String, Unit>,
}
pub struct Unit {
    pub id: String,
    pub spec_criterion: String,   // every unit maps to a criterion (anti-fragmentation)
    pub status: Status,           // Pending | Running | Integrated | Failed | Escalated
    pub attempts: u32,
    pub commit: String,           // the integrating commit, once it lands
}
// The conductor folds run events (UnitStarted / UnitFailed / UnitEscalated /
// UnitIntegrated) into this state; gate-autonomy history lives in the gate engine.
```

### 4.3 The autonomy ratchet (bidirectional, self-correcting)

Per gate: `manual → auto_notify → silent` on N consecutive clean passes (proposed, never
auto-applied); any non-manual gate that **fails** auto-demotes to `manual`. Autonomy
tracks demonstrated reliability: a graduated gate can never become a silent hole that
auto-passes bad work. The async manual-gate queue lets *independent* units advance while
one waits on a human. (Direct port of `autonomy.mjs`.)

### 4.4 Safety rails

`checkBudget` (token/time circuit-breaker → pause), `remediate` (bounded retry with
re-grounding → escalate after N), `flagSpecDefect` (halt + amend the spec, don't
deviate), `abortTask` (discard un-integrated worktrees, keep integrated). Never silent,
never infinite. (Port of `safety.mjs`.)

---

## 5. The memory layer: event source + context graph  **[TARGET: the new heart]**

This is what the tank_game harness does *not* have and what makes Rigger more than a
port. The model, in one line: **agents append immutable events to a log; a projector
folds the log into a bi-temporal context graph; agents retrieve their connected subgraph
and subscribe for in-flight decisions.**

### 5.1 The event store: KurrentDB-shaped, embedded by default

The trait mirrors KurrentDB's primitives so the embedded SQLite impl is a faithful
*contract proxy* for the real server; swapping backends is a config flip, not an
architecture change.

```rust
// src/eventstore/mod.rs
// Mirrors KurrentDB: append-only streams, a global $all order, catch-up subscriptions.
pub trait EventStore: Send + Sync {
    /// Append events to a stream under an optimistic-concurrency expectation,
    /// returning the last global position written; a failed expectation yields
    /// `Error::Conflict`.
    fn append(&self, stream: &str, expected: ExpectedRevision, events: &[Event])
        -> Result<Position, Error>;

    /// Read one stream's events from a global position, in a direction.
    fn read_stream(&self, stream: &str, from: Position, dir: Direction)
        -> Result<Vec<Event>, Error>;

    /// Read the global $all log from a position, in a direction, filtered: the
    /// projector's input.
    fn read_all(&self, from: Position, dir: Direction, filter: &Filter)
        -> Result<Vec<Event>, Error>;

    /// Open a catch-up subscription over $all: replay matching events from `from`,
    /// then deliver new ones live. This is the (A) live-awareness mechanism: a
    /// running agent's side-car watches $all.
    fn subscribe_all(&self, from: Position, filter: &Filter) -> Result<Subscription, Error>;
}

pub type Position = u64; // global ($all-order) position, assigned by the store on append

pub struct Event {
    pub id: String,            // a fresh UUID per event
    pub type_: String,         // "DecisionMade", "FileTouched", "GateVerdict", "UnitIntegrated", …
    pub data: Vec<u8>,         // the opaque (usually JSON) payload (see §5.3)
    pub recorded_at: SystemTime, // when the event was created / ingested
    pub position: Position,    // global order (assigned on append)
}

// Optimistic-concurrency expectation: any version, no stream yet, or an exact count.
pub enum ExpectedRevision { Any, NoStream, Exact(u64) }

// A read/subscription filter over the global log (a stream-name prefix).
pub struct Filter { pub stream_prefix: Option<String> }
```

`Subscription` is a concrete catch-up handle (not a trait): adapters feed it from a
background thread, and callers drain it with `recv` / `recv_timeout` / `try_recv`;
dropping it stops the feed.

**Two impls, one trait:**
- **`sqlite` (default).** One table `events(position INTEGER PK AUTOINCREMENT, stream,
  type, data, recorded_at, …)`. `$all` = `ORDER BY position`. A per-stream uniqueness
  constraint gives optimistic concurrency. **Subscriptions** = a poll on `MAX(position)`
  fed onto an mpsc channel from a background thread; at Rigger's event volume (hundreds
  to thousands of events per run) this is trivial. Backed by bundled `rusqlite`; zero
  external service; the whole store is one file.
- **`kurrentdb` ([AS-BUILT], behind the `kurrentdb` cargo feature).** A thin adapter over
  the official KurrentDB Rust client, bridging its async gRPC API onto the (sync) port
  through a tokio runtime: `append`→`AppendToStream`, `read_all`→`$all` read,
  `subscribe_all`→a filtered catch-up subscription. Selected at the composition root when
  built with `-F kurrentdb`.

Because the trait *is* the KurrentDB model, the SQLite impl's contract suite
(`eventstore::contract::assert_contract`: append ordering, optimistic-concurrency
conflicts, catch-up replay-then-live) doubles as the contract test the KurrentDB adapter
must also pass — its `kurrentdb` CI job runs that same suite against a real KurrentDB via
testcontainers: the proxy fidelity you asked for.

### 5.1.1 Per-project segregation (one mechanism, every backend)  **[AS-BUILT]**

Event streams and the context graph are **scoped to one project by default**, never shared. `cargo install` puts the rigger *binary* on a shared `PATH`, but its *data* is always project-local, enforced by **one mechanism for every backend**, not a different trick per store: a **project namespace applied to stream names**, via a single scoping decorator over the `EventStore` port. This is implemented as `eventstore::namespace::Namespaced<'a>`, a wrapper struct that itself implements `EventStore`.

- The decorator (`Namespaced::new(inner, project)`) prefixes every stream a project writes with its `proj-<project>-` namespace, and scopes every read/subscribe filter to it, so callers use plain, unprefixed stream names and never see the namespace. It is written once and wraps *any* `&dyn EventStore`; the backends are namespace-unaware.
- **SQLite** realizes the filter on the `stream` column (a prefix match); its `.rigger/` directory is just the default storage path, not the isolation mechanism.
- **KurrentDB** realizes the same prefix as a server-side `$all` filter (it supports filtered catch-up subscriptions natively), so one server backs many projects, each seeing only its own events against its own checkpoint.
- The namespace **defaults to the project identity**, so isolation is the default. A hard boundary (security or multi-tenant) is just config: a dedicated SQLite file, or a dedicated KurrentDB instance.

This is dependency inversion (R8) paying off directly: because the decorator depends on the `EventStore` *trait*, segregation is one implementation for all backends — and it passes the same contract suite (`Namespaced` is contract-tested). The **context graph is always a local, per-project projection**, rebuilt into `.rigger/` from the namespaced stream whatever the log backend, so even a shared KurrentDB server never shares a graph.

### 5.2 The context graph: a bi-temporal projection

The graph is a **read model** the projector maintains by folding `$all`. Rigger ships the
projection as **SQLite tables** (one store, no extra engine; subgraph traversal in the
adapter); an embedded graph engine with Cypher would be a drop-in alternative behind the
same `Projection` trait if traversal richness ever demands it.

```rust
// src/contextgraph/mod.rs
pub struct Node {
    pub id: String,                       // stable id (entity-resolved)
    pub kind: String,                     // "decision" | "artifact" | "agent" | "gate" | "unit" | "lesson"
    pub attrs: BTreeMap<String, String>,
}
pub struct Edge {
    pub from: String,
    pub to: String,
    pub rel: String,                      // "SUPERSEDES" | "TOUCHES" | "GOVERNS" | "GATED_BY" | "ABOUT"
    pub valid_from: i64,                  // bi-temporal validity interval …
    pub valid_to: Option<i64>,            // … None = still valid; Some = invalidated (NOT deleted)
    pub source: Position,                 // the event that asserted this edge (provenance)
}
pub trait Projection: Send + Sync {
    fn apply(&self, e: &Event) -> Result<(), Error>;                  // fold one event (idempotent per position)
    fn subgraph(&self, seed: &[String], depth: i64) -> Result<Graph, Error>; // the FEED arc: connected blast-radius
    fn resolve(&self, mention: &str) -> Result<Option<String>, Error>;       // entity resolution (alias → node id)
}
```

**Three properties carried from the research:**
1. **Bi-temporal freshness (Zep/Graphiti).** Supersession sets `valid_to` on the old edge
   and appends a new one: the graph shows the *current* truth, the log keeps the
   *history*, and a stale fact never surfaces with false confidence. (e.g. the
   `collapse-decision`'s governing edge has its `valid_to` stamped when the `split-decision`
   supersedes it.)
2. **Entity resolution (Graphiti / the TDS alias-table bug).** `resolve` collapses
   `"the editor" ≡ "content-editor" ≡ "velocity-engine"` to one node on ingest, so
   retrieval joins instead of fragmenting.
3. **Scoped retrieval (GraphRAG).** `subgraph(seed, depth)` returns the *connected
   subgraph* of an agent's blast-radius (ALL & ONLY its context), not a chunk dump.

### 5.3 The ∞ loop: emit, project, retrieve

```mermaid
flowchart LR
  subgraph A["🤖 AGENT (one of N, isolated worktree)"]
    R["① RETRIEVE\nsubgraph(my files, depth) + grounder top-k"]
    W["② WORK"]
    E["③ EMIT events\nDecisionMade · FileTouched · GateVerdict"]
    R --> W --> E
  end
  E ==>|append| LOG[("event log\n\$all, bi-temporal")]
  LOG ==>|projector folds| CG[("context graph")]
  CG ==>|FEED: scoped subgraph| R
  LOG -. "SubscribeAll(filter=my blast-radius)\n(A) live: see CC2's in-flight decision" .-> W
```

**The (A) live awareness, concretely.** An agent's run is wrapped by a Rigger **side-car**
(`sidecar::Sidecar`) that holds a `subscribe_all` catch-up subscription filtered to the
agent's blast-radius, draining matching events in a background thread. When a *concurrent*
agent appends a `DecisionMade` touching a shared node, the side-car surfaces it to the
agent at its next tool-boundary (a context refresh injected before the next action). This
gives true in-flight awareness **without** touching the agent's files: isolation guards
the *files* (worktree), the event stream is the *separate shared decision channel*. The
two are orthogonal (the insight that makes (A) safe).

### 5.4 Grounding stays hybrid (vector + graph)

[AS-BUILT] `turbovec` is local code+memory vector search (`tools/semantic-retrieval`).
[AS-BUILT] Rigger keeps a pluggable **`Grounder`** trait (`ground(query, k) -> Vec<Ref>`)
for fuzzy "find things like this" and adds the **graph** for "what decisions govern these
files / who else touches these nodes": the multi-hop questions vector RAG structurally
can't answer. The research is unanimous the winner is *both*: vector for the fast first
pass, graph for relationships. The default impl is `Grep` (a self-contained literal
substring search, no index, no dependency); the semantic impl is `grounder::turbovec::Turbovec`,
built behind the `turbovec` cargo feature (native turbovec quantized vector search +
fastembed embeddings). The CLI selects turbovec when compiled with `-F turbovec` and
**falls back to grep** if its embedding model is unavailable, so the default build stays
light.

---

## 6. The agent driver: pluggable spawning  **[AS-BUILT]**

```rust
// src/conductor.rs
pub trait AgentDriver: Send + Sync {
    /// Spawn one agent to completion. The agent records events it emits during its
    /// run by calling `emit`, so its decisions reach the log live (the workflow
    /// driver wires `emit` to an in-process tool the agent calls; the cli driver,
    /// a subprocess, cannot call back and ignores it).
    fn spawn(
        &self,
        agent: &AgentDef,
        prompt: &str,
        opts: &SpawnOpts,
        emit: &dyn Fn(&str, serde_json::Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error>;
}
pub struct SpawnOpts { pub dir: String }   // the working dir (an isolated worktree, or "")
pub struct AgentResult { pub output: String }
```

- **`cli` (default, self-contained).** `driver::cli::Driver` spawns the `claude` CLI as a
  subprocess (`bin` defaults to `"claude"`) and reads its output. Worktree isolation is
  Rigger's own (`worktree::Worktree`): `git worktree add` before, harvest the branch +
  remove after. **No Claude-Code runtime assumption: works for any `cargo install` user
  with the `claude` CLI on PATH.** Fan-out = a bounded pool of scoped OS threads
  (`std::thread::scope`) over disjoint units. A subprocess agent cannot call the in-process
  `emit`, so this driver ignores it; the workflow driver is the one that delivers live emission.
- **`workflow` (optional).** `driver::workflow::Driver` runs inside Claude Code to keep the
  Workflow tool's built-in parallelism / journaling / resume. The Workflow sandbox cannot
  shell out to a binary, so the bridge is **MCP, not a subprocess**: `rigger serve` runs the
  conductor on a background thread and serves an MCP server (`mcpserver::Server`) over stdio
  exposing `rigger_next` / `rigger_result` / `rigger_emit` / `rigger_peers`. A thin Workflow
  shim loops - `rigger_next` for the next spawn, run it in-process, `rigger_result` to report
  it - while agents record decisions live via `rigger_emit`. The Rust core is identical; only
  the spawn seam changes.

**Runaway-proof by construction** (carried from the fan-out lesson): the implementer agent
def declares `recurse: false` (no Agent/spawn capability), and units are partitioned
disjoint, so parallel worktrees cannot conflict and an agent cannot fan out.

---

## 7. Worked example: the modifier saga through Rigger

The real episode from this session, replayed as Rigger would record it. A unit
"genericize the modifier pipeline" runs; here is the **event log** it appends and the
**graph** that results.

```jsonc
// stream "run-7", appended over the unit's life (Position grows globally)
{ "type":"UnitStarted",   "data":{"unit":"mod","criterion":"engine names no game concept"} }
{ "type":"Grounded",      "data":{"refs":["modifier.rs","FoldRule","GA_STAGES"]} }
{ "type":"DecisionMade",  "data":{"id":"mod-collapse","summary":"move whole modifier to ga-*"},
  "validFrom":"…T10:00Z" }
// … owner rejects; the split decision supersedes the collapse …
{ "type":"DecisionMade",  "data":{"id":"mod-split","summary":"generic FoldRule pipeline in engine, GA taxonomy on top",
  "supersedes":"mod-collapse"}, "validFrom":"…T11:30Z" }
{ "type":"FileTouched",   "data":{"path":"engine-schema/src/modifier.rs"} }
{ "type":"GateVerdict",   "data":{"gate":"e7","pass":true,"evidence":"TOTAL 0"} }
{ "type":"GateVerdict",   "data":{"gate":"test","pass":true,"evidence":"54 passed"} }
{ "type":"UnitIntegrated","data":{"unit":"mod","commit":"f848b97"} }
```

The projector folds these into the graph:

```
(decision mod-split)    --SUPERSEDES--> (decision mod-collapse)
(decision mod-collapse) --GOVERNS(valid_to=11:30)--> (artifact modifier.rs)   ← invalidated, not deleted
(decision mod-split)    --GOVERNS--> (artifact modifier.rs)
(artifact modifier.rs)  --GATED_BY--> (gate e7)
(agent impl-mod)        --TOUCHES--> (artifact modifier.rs)
```

**The payoff, concretely:** the *next* agent that touches `modifier.rs` calls
`subgraph(&["modifier.rs"], 2)` and is handed `mod-split` (current), **not** `mod-collapse`
(invalidated), plus the `e7` gate that governs the file, plus the no-named-bridge lesson
linked to the engine crate. It cannot re-litigate the collapse, re-invent a gate-dodge, or
work a stale base: the three failure classes this session hit, closed structurally.

---

## 8. Edge cases & failure modes

| Failure | Handling |
|---|---|
| Spec has no enumerable Done-when criteria | `loop-ready` gate blocks; ask the human to add them (never guess "done") |
| A discovered unit has no `spec_criterion` | `spawnUnit` refuses + emits a `scope_creep` event (anti-fragmentation) |
| A conceptual criterion covered only by a mechanical gate | `coverage` proxy-gap guard ⇒ NOT covered; demands a real (LLM-judge) verifier |
| Two concurrent units edit the same file | Partitioner makes batches disjoint by blast-radius; they never share a worktree |
| Agent crashes / hits usage limit mid-spawn | `cli` driver: non-zero exit → `remediate` (bounded retry, re-grounded) → escalate |
| Stale base (a peer landed while I ran) | Integrate does `pull --rebase` + re-runs gates; the graph's `TOUCHES` edges flag the overlap pre-merge |
| Event store append conflict (optimistic concurrency) | `expectedVersion` mismatch → re-read stream, re-project, retry the append |
| Projector falls behind / crashes | The graph is a *pure projection*: rebuild it from `$all` from position 0 (event sourcing's superpower); idempotent |
| Superseded decision still in the graph | Bi-temporal `ValidTo` set on supersession; `Subgraph` filters `ValidTo IS NULL` by default |
| Entity mention doesn't resolve | `Resolve` miss ⇒ create a new node + log an `alias_unresolved` event for later merge (never silently drop) |
| KurrentDB unreachable (optional backend) | Fail fast at startup with a clear error; the `sqlite` default never has this failure mode |
| Gate command itself errors (not just fails) | Gate engine wraps the run; a throwing command ⇒ `{pass:false, evidence:"gate errored: …"}`; never crashes the loop |
| Budget exhausted mid-run | `checkBudget` circuit-breaker pauses; resume by replaying the ledger projection |

---

## 9. Data model / schemas (consolidated)

- **Event:** §5.1 (`id, type_, data, recorded_at, position`; the stream is an `append`
  argument, not a field). `ExpectedRevision` (`Any | NoStream | Exact(u64)`) and `Filter`
  (`stream_prefix`) accompany it.
- **Graph Node / Edge:** §5.2 (Node `id, kind, attrs`; Edge carries the bi-temporal
  `valid_from: i64 / valid_to: Option<i64>` + `source: Position` provenance).
- **RunState / Unit:** §4.2 (the projected ledger; Unit = `id, spec_criterion, status, attempts, commit`).
- **AgentDef:** §3.1 frontmatter (`id, model, tools, isolation, recurse, prompt`).
- **Workflow:** §3.2 (`name, gates{}, stages{needs, agent(s), strategy, partition, gates, adjudicator, autonomy, on_pass, …}`).
- **Gate:** `{id, run, kind: Core|Elevated|Deferred, autonomy: Manual|AutoNotify|Silent, history:[{pass}]}`.

---

## 10. Repo layout & `cargo install` usage  **[AS-BUILT]**

A single Rust crate: a library (`src/lib.rs`) plus a binary (`src/main.rs`), with the ports
and adapters as modules under `src/`. Two opt-in cargo features keep the default build light.

```
github.com/virtual-velocitation/rigger
├── Cargo.toml                   crate "rigger"; features: turbovec, kurrentdb
├── src/
│   ├── lib.rs                   the library: re-exports every module
│   ├── main.rs                  the CLI binary (run/serve/graph/validate/init/setup/prime)
│   ├── conductor.rs             the DAG executor + run loop; the AgentDriver port
│   ├── eventstore/
│   │   ├── mod.rs               the EventStore trait + Event/Position/Filter/Subscription
│   │   ├── sqlite.rs            default adapter (embedded, bundled rusqlite)
│   │   ├── kurrentdb.rs         server adapter (behind the `kurrentdb` feature)
│   │   ├── namespace.rs         per-project segregation decorator (Namespaced)
│   │   └── contract.rs          the shared contract suite (assert_contract)
│   ├── contextgraph/
│   │   ├── mod.rs               the Projection trait + Node/Edge/Graph
│   │   └── sqlite.rs            the bi-temporal SQLite projector
│   ├── driver/
│   │   ├── mod.rs
│   │   ├── cli.rs               default driver (claude subprocess)
│   │   └── workflow.rs          optional driver (in-Claude-Code MCP shim)
│   ├── grounder/
│   │   ├── mod.rs               the Grounder trait + Grep (default) + Nop
│   │   └── turbovec.rs          semantic grounder (behind the `turbovec` feature)
│   ├── gate.rs                  gate engine + autonomy ratchet (Runner port, ExecRunner)
│   ├── safety.rs                budget breaker + bounded remediation
│   ├── ledger.rs                RunState projection (folded from the run events)
│   ├── config.rs                agent-file + workflow-YAML loader → runtime types
│   ├── spec.rs  worktree.rs  sidecar.rs  mcpserver.rs  hooks.rs
└── .github/workflows/rust.yml   CI: build-test, turbovec, kurrentdb jobs
```

```bash
cargo install --git https://github.com/virtual-velocitation/rigger
# opt into the features (each pulls heavier deps):
cargo install --git https://github.com/virtual-velocitation/rigger --features turbovec,kurrentdb

cd my-project
rigger init                         # scaffolds .rigger/workflow.yml + .rigger/agents/
rigger run specs/feature.md         # runs the producing loop on a spec
rigger serve                        # run as an MCP server for the in-Claude-Code workflow shim
rigger graph --around modifier.rs   # inspect the context graph (subgraph query)
rigger validate                     # load + validate the workflow + agents
```

The optional backends are selected at build time by cargo feature (`-F kurrentdb`,
`-F turbovec`), not by a runtime flag. Library use (embed the harness) imports the same
modules from the `rigger` crate directly.

---

## 11. What carries over vs. what's new

| tank_game module **[AS-BUILT]** | Rigger **[TARGET]** | Change |
|---|---|---|
| `ledger.mjs` | `conductor` + event projection | ledger becomes a projection of the log |
| `conductor.mjs` (hardcoded pipeline) | `conductor` executing the workflow DAG | pipeline becomes declared YAML |
| `plan.mjs` (DAG, coverage, partition) | `conductor` (same logic, Rust) | direct port |
| `gates.mjs` | `gate` + the YAML `gates:` library | gates become config |
| `autonomy.mjs`, `safety.mjs`, `learn.mjs` | `gate` (ratchet) + `safety`, Rust | direct ports |
| `turbovec.mjs` + `tools/semantic-retrieval` | `grounder::turbovec` (turbovec feature) | generalized + pluggable |
| `bd-*.sh` federation memory | the event log + context graph | replaced by event-sourced memory (the new core) |
| `dev-loop-fanout` / `dev-loop-review` (JS Workflows) | `driver/cli` (default) or `driver/workflow` | spawning becomes a pluggable seam |
| review lenses, e7 gate, S1 spec | `examples/golden-apple/` | demoted to a worked example |
| (none) | **event store + bi-temporal context graph + (A) subscriptions** | **net-new** |

---

## 12. Records to ratify during execution

> These are **Rigger's** future records (created in the *rigger* repo at Phase 0), embedded
> here as proposals. **Do not** write them into tank_game's `adr/` or `ubiquitous-language.md`.

### Proposed `rigger/docs/adr/0001-rigger-architecture.md`

```markdown
# ADR-0001: Rigger, a config-driven, event-sourced multi-agent dev-loop harness

- Status: Proposed
- Context: We need a standalone, publishable harness that turns a spec into integrated
  code via a fleet of AI agents, generalized from the tank_game dev-loop, owning none of
  any consumer's project specifics.
- Decision: Rigger is governed by:
  - R1 CONFIG-OVER-CODE: agents are definition files; the flow is a workflow YAML (a DAG);
    gates are commands. Reconfiguring the loop never recompiles the binary.
  - R2 EVENT-SOURCED MEMORY: an append-only event log is the single source of truth; all
    run state and the context graph are projections folded from it (rebuildable, resumable).
  - R3 BI-TEMPORAL CONTEXT GRAPH: decisions are first-class nodes with validity intervals;
    superseded facts are invalidated, never deleted; retrieval returns a connected subgraph,
    not a chunk dump.
  - R4 PLUGGABLE SEAMS: EventStore (sqlite default | kurrentdb), AgentDriver (cli default |
    workflow), Grounder (grep default | turbovec) are traits chosen by config / cargo feature;
    the core depends only on the traits.
  - R5 ORTHOGONAL ISOLATION: worktree isolation guards FILES; the event stream is the shared
    DECISION channel; live cross-agent awareness never crosses the file boundary.
  - R6 MACHINE-VERIFIABLE DONE: every spec criterion covered + every unit integrated + every
    gate green; failures escalate or bounded-retry, never silently drop, never infinite-spin.
  - R7 SELF-CONTAINED PUBLISH: `cargo install`-able; no runtime dependency on Claude Code or a
    database server in the default configuration (the server backend and semantic grounder are
    opt-in cargo features).
  - R8 CLEAN ARCHITECTURE + DI: ports (EventStore/Projection/AgentDriver/Grounder/gate::Runner) are
    traits; sqlite/kurrentdb/cli/workflow are adapters that depend inward; use cases depend
    only on ports; a single composition root (`src/main.rs`) constructs the concrete adapters and
    injects them. No globals, no module-level singletons, no type building its own dependencies.
    Idiomatic Rust throughout: small traits, accept `&dyn Trait` and return concrete types, errors
    as `Result` values, one responsibility per module, no premature abstraction.
  - R9 PROJECT-SCOPED DATA, ONE MECHANISM: event streams and the context graph are segregated per
    project by a single scoping decorator over the EventStore port, a project namespace applied to
    stream names, identical for every backend (SQLite filters the `stream` column; KurrentDB filters
    `$all` server-side). The shared `cargo install` binary never implies shared data; the graph is
    always a local, per-project projection.
- Consequences: a hardcoded flow, a project-specific concept baked into the core, a mutable
  (non-event-sourced) source of truth, a deleted-not-invalidated fact, a default that requires a
  server/IDE, a use case that depends on a concrete adapter instead of a port, or a second
  segregation mechanism are defects.
```

### Proposed Rigger glossary rows (`rigger/docs/glossary.md`, status `pending ADR-0001`)

| Term | Meaning |
|---|---|
| **Workflow** | the YAML DAG that declares the loop's stages, deps, gates, autonomy |
| **Agent def** | a markdown+frontmatter file declaring one agent's model/tools/prompt |
| **Gate** | a command + kind + autonomy; the unit of verification |
| **Event** | an immutable, bi-temporal fact appended to the log (the source of truth) |
| **Context graph** | the projected, bi-temporal node/edge read model of decisions+artifacts |
| **Driver** | the pluggable agent-spawning backend (cli \| workflow) |
| **Side-car** | the per-agent subscription that delivers in-flight cross-agent decisions |

---

## 13. Phased delivery roadmap

Each phase lands independently and is demoable. **Task 0 = ratify the records.**

- **Phase 0: Repo + records.** Create the public `github.com/virtual-velocitation/rigger`
  repo; `cargo init` the crate; move this blueprint to `docs/architecture.md`; **ratify
  ADR-0001 + the glossary** (Task 0). *Done when:* the crate builds + the ADR is committed.
- **Phase 1: Event store.** `EventStore` trait + `sqlite` adapter + a contract suite
  (append ordering, optimistic-concurrency conflict, catch-up replay-then-live). *Done when:*
  the contract suite passes against `sqlite`.
- **Phase 2: KurrentDB adapter.** `kurrentdb` adapter (behind the `kurrentdb` feature) passing
  the *same* contract suite. *Done when:* the proxy fidelity is proven (one suite, two backends
  green; the `kurrentdb` CI job runs it against a real KurrentDB via testcontainers).
- **Phase 3: Context graph.** `Projection` trait + sqlite projector: fold events → nodes/edges,
  bi-temporal supersession, entity resolution, `subgraph`. *Done when:* the modifier-saga
  fixture (§7) projects correctly and `subgraph` returns `mod-split`, not `mod-collapse`.
- **Phase 4: Config loader.** Parse agent files + workflow YAML → runtime types; validate.
  *Done when:* the `examples/golden-apple` config loads (or `rigger validate` passes).
- **Phase 5: Conductor + rails.** The DAG executor + ledger projection + gate engine +
  autonomy + safety (ports). *Done when:* a trivial 2-stage workflow runs end-to-end with a
  stub driver.
- **Phase 6: CLI driver + worktrees + side-car.** `driver::cli` (claude subprocess), git-worktree
  isolation, the live subscription side-car. *Done when:* a real spec produces an integrated
  commit, and a concurrent decision is observed in-flight.
- **Phase 7: Workflow driver + turbovec grounder + polish.** The optional MCP workflow shim, the
  turbovec semantic grounder (behind the `turbovec` feature), `rigger init/run/graph`,
  README/examples. *Done when:* `cargo install … && rigger run` works from a clean machine; both
  drivers + both event stores are switchable (driver by config, store by cargo feature).

---

## 14. Glossary & cross-references

See §12 for Rigger's own glossary. This blueprint inherits its discipline from the tank_game
dev-loop design (`docs/superpowers/specs/2026-06-23-development-loop-design.md`) and the
context-graph research (Zep/Graphiti, the TrustGraph context-graph manifesto, GraphRAG-vs-vector).
KurrentDB's model (`github.com/kurrent-io/KurrentDB`) is the trait blueprint for `eventstore`.

---

*End of reference architecture. This is a PROPOSAL: nothing here is ratified until Phase 0,
and the rigger repo does not yet exist. Review gate next; see the hand-off.*
