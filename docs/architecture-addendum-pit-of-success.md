# Reference Architecture Addendum — Pit of Success

**Status:** design, approved for planning
**Scope:** an addendum to `docs/architecture.md`. It does not restate the core
architecture; it specifies a coherent set of changes that make rigger's *existing*
guarantees reachable, visible, validated, and self-documenting.

---

## 1. Problem

rigger already ships the machinery that makes a dev-loop safe: a fail-closed
integration gate, bounded remediation that escalates to a human rather than looping
forever, a blessed driver whose progress is visible in `/workflows`, a read-only
observability page, and documented rules for writing loop-ready specs.

The machinery works. What fails is **reachability**: a capable operator (human or
agent) driving rigger on a real project for the first time cannot discover any of it
from the tool itself, and rigger does not validate the one thing an operator most
easily gets wrong — a gating persona that never returns its verdict on the channel the
gate reads. The result is a chain of silent footguns whose symptoms all point *away*
from their causes:

- A gating agent whose persona emits its verdict only as an event (never on its result
  output) produces a guaranteed integration stall. The gate reads the result channel,
  finds no verdict, treats it as non-approval, and the unit remediates and re-runs
  until it escalates. Bounded and eventually loud — but from the outside it *looks*
  like an unbounded loop re-running an already-finished unit, and nothing detects the
  misconfiguration statically or explains it at runtime.
- A run anchored on the wrong base ref (the default `origin/main` while the real work
  lives on a feature branch far ahead of it) silently produces doomed units against a
  tree that lacks the files the spec names. The command an operator naturally reaches
  for (`rigger workflow`) rejects `--base` outright.
- The observability page never starts on its own and is named in neither the setup
  output nor the operator handbook, so a first-time operator drives blind and polls
  git/ps by hand — which is exactly the vantage point from which every other symptom
  above gets misdiagnosed.
- A spec whose criteria pack multiple behaviors into one checkbox, or whose planner
  paraphrases a criterion it was told to copy verbatim, festers as an opaque
  plan-critique reject loop rather than a named lint at author time.
- Decisions and findings from dead or superseded runs stay in the cross-run context
  graph with no way to tell them apart from live ones, so stale noise surfaces in a
  healthy run's grounding.

None of these is a missing safety mechanism. Each is a place where an existing
guarantee is invisible, an existing contract is unvalidated, or an existing design
decision is undocumented and therefore looks like a bug to the next person who trips
over it. This addendum closes them at the tool level.

### Non-goals / anti-fixes

This addendum explicitly does **not**:

- Add redundant guards to paths that are already bounded (remediation already escalates
  after `max_retries`; a degenerate gating result already halts loudly).
- Change the `origin/main` base default (see §2).
- Scope grounding to the active run (see §2).
- Move integration-gate authority onto the event stream (see §2).

These are called out because each is a plausible-looking "fix" that would regress a
load-bearing decision. §2 records why.

---

## 2. Load-bearing decisions — do not naively "fix"

Each decision below is stated with the concrete failure it prevents, so that a future
change does not undo it by mistake. These rationales are mirrored as doc-comments at
each code site and are part of the canonical discipline source (§6), so the skill and
handbook carry them too.

### 2.1 Base defaults to `origin/main`

The loop lands integrated commits on a dedicated **run branch** created off the base
ref; a human turns that run branch into `main` through normal PR review. Anchoring the
run branch off `origin/main` is what keeps "what the loop produced" a clean, reviewable
delta against the trunk everyone shares.

Defaulting the base to `HEAD` instead would anchor every run on whatever branch happens
to be checked out, making the run branch's history a function of local state rather than
the shared trunk, and defeating the run-branch → PR model. The correct fix for the
feature-branch case is not to change the default but to make an explicit base
**reachable** on every driver and to **refuse** a base that cannot contain the work
(§3.4).

_Code:_ `DEFAULT_BASE_REF` (`src/main.rs`), `parse_step_args`, `Worktree::ensure_run_branch`
(`src/worktree.rs`).

### 2.2 The context graph spans runs

A unit's grounding inherits the decisions, findings, and lessons recorded by earlier
work on the same files. That cross-run memory is what stops each run from starting
amnesiac — re-deriving settled architecture and, worse, *violating* constraints an
earlier run established. Example from this codebase: the decision that
`select_grounder` intercepts the `hybrid` grounder only under `cfg(feature = symbols)`
must remain visible to a later run editing the same file, or that later run silently
breaks the symbols-off contract.

Scoping grounding to the active run by default would throw this away to solve a
narrower problem — stale findings from a *dead* run surfacing in a healthy one. That
problem is real, but the cause is that live and superseded decisions are
indistinguishable, not that cross-run memory is wrong. The fix is provenance and
pruning (§7), not amnesia.

_Code:_ `graph_context`, `Projector` (`src/contextgraph/`), `src/conductor.rs`.

### 2.3 The gate reads the result-channel verdict; events are for diagnosis, not authority

Integration is gated on the adjudicator spawn's **result output** — a single JSON
`{"verdict":"approve"}` line — never on emitted `ReviewFinding`/`DecisionMade` events.
This is deliberate and load-bearing:

- **Attempt/run correlation.** A `SpawnResult` is bound to exactly one spawn — this
  adjudicator, this attempt, this diff. Events are an append-only stream that persists
  across attempts and runs. If the gate consulted events, a stale `approve` event from
  a superseded attempt (or a prior run) could gate a *different* diff. That is the same
  pollution class §2.2 guards against, reintroduced into the one place that most needs
  to be pollution-free.
- **Fail-closed determinism.** The gate is mechanical and binary: a missing or
  unparseable verdict is a non-approval, never a silent pass. A qualitative weighing of
  "result plus events" at the gate is a heuristic over a stream; delegating that
  weighing to another agent just makes *that* agent the real gate, which then needs its
  own authoritative verdict channel — infinite regress.
- **Plane separation.** Events are the context/memory plane (they feed grounding and
  metrics); results are the control plane. Any role holding `rigger_emit` can append a
  verdict-shaped event; only the agent the conductor spawned in the gating role writes
  that spawn's result. Letting the memory plane drive control couples them and widens
  blast radius.

The adjudicator **should** consume events — the lens findings and the adversary's
emitted attempts are its *input*, and its verdict line is that qualitative judgment's
distilled, authoritative output. So the system does look at both; the result channel
*decides* and the event stream *explains*. The productive use of events at the gate is
**diagnosis**: when an adjudicator emitted an `approve` event but returned no verdict
line, the conductor reads that mismatch and hard-errors with the exact fix (§3.1)
instead of silently treating it as a reject.

_Code:_ `verdict_approves`, `run_adjudicator`, `IntegrationApproval` (`src/conductor.rs`).

### 2.4 The loop is the blessed build path; TDD is intrinsic to it

The per-unit lifecycle — implement → cargo gates → three-tier adversarial review →
integrate — *is* the test-driven discipline. Reaching for a manual or headless path to
"just build it" is the anti-pattern this addendum exists to remove; if a task seems to
need hand-building, that is a gap in the loop to fix, not a reason to bypass it.

---

## 3. Workstream A — Fail fast, name the fix

### 3.1 Gating-persona verdict-line lint + runtime mismatch detection

**Problem.** A gating agent (a review adjudicator on any tier, or a plan-critique
adjudicator) whose persona instructs it to record its verdict only via `rigger_emit`,
never to *end its output* with the verdict line, is a guaranteed stall: the gate
(§2.3) finds no verdict on the result, treats it as non-approval, and the unit
remediates until it escalates. Today, config load validates only that an adjudicator is
*named*; the persona body is stored but never inspected.

**Static lint.** `config::load` / `rigger validate` inspects every gating agent's
persona prompt and fails when it does not instruct the agent to end its output with a
`{"verdict":…}` line — i.e. when the only verdict path is `rigger_emit`. The check is
deterministic (a guaranteed hang), so it is a **hard error**, both in `rigger validate`
and at run start (the run refuses to begin). The error names the exact fix:

> `agent "adjudicator" is a gating role but its prompt never instructs it to end its
> output with a verdict line (e.g. `{"verdict":"approve"}`). The integration gate reads
> the result channel, not emitted events; a verdict emitted only via rigger_emit will
> never gate. Add the verdict line to the agent's output.`

Detection heuristic: the prompt must contain a `{"verdict"` literal that is presented as
*output/result* (associated with "end", "output", "result", "final line", or an
un-qualified JSON example), not exclusively adjacent to a `rigger_emit` instruction. The
exact matcher is specified in the implementation plan; false negatives are acceptable
(a weird-but-correct prompt), false positives are not (never fail a prompt that does put
the verdict on the result).

**Runtime mismatch detection.** Independently, when a gating spawn returns a result with
no parseable verdict line **and** emitted an approve-shaped event during that spawn, the
conductor hard-errors for that unit with the same fix message, rather than folding it as
a reject and remediating. This is the diagnostic use of events from §2.3. It backstops
a persona that passed the static lint but still failed to return a verdict (e.g. the
agent ignored its instructions).

_Code:_ `config::load`, `ReviewPanel::validate_depth` (`src/config.rs`); `cmd_validate`
(`src/main.rs`); the gate site in `run_adjudicator` (`src/conductor.rs`).

### 3.2 Spec-shape lint

**Problem.** A checkbox that packs multiple observable behaviors, or sub-bullets under a
checkbox that read as separate units, or a criterion too long to copy verbatim reliably,
each ferments into an opaque reject loop instead of a named problem at author time. The
`rigger validate` command today accepts no spec argument and never inspects a spec.

**Fix.** `rigger validate [spec]` accepts an optional spec path and lints criterion
shape, emitting **advisory warnings** (heuristic, so never a hard failure) that name the
rule and recommend the fix:

- a checkbox containing multiple observable behaviors (multiple independent assertions /
  imperative clauses joined by "and");
- indented sub-bullets under a checkbox that read as separate criteria;
- a criterion long enough that a verbatim copy by the planner is unreliable (a length
  threshold).

Each warning recommends: *one observable behavior per criterion; put type shapes and
implementation detail in a non-criteria Notes section.* The distinction from §3.1 is
deliberate: §3.1 catches a deterministic hang and hard-errors; §3.2 catches a
probabilistic smell and advises.

_Code:_ `extract_criteria` (`src/spec.rs`); `cmd_validate` (`src/main.rs`).

### 3.3 Planner ↔ baseline robustness

**Problem.** The conductor reconciles a planner's proposed unit against the baseline
unit derived from a spec criterion by comparing the criterion text with only whitespace
normalization — no stable id. A planner that paraphrases or truncates a long criterion
it was told to copy verbatim produces a proposal that does not match the baseline, so
both run: two units claim the same files, and plan-critique rejects in a loop.

**Fix.** Each baseline criterion carries a **stable id** (its position plus a normalized
content hash). The planner echoes that id on each proposal, and the conductor matches on
the id rather than on re-normalized prose, so a paraphrase or truncation can no longer
silently spawn a duplicate. A proposal that maps to **no** baseline id still runs as a
genuinely-new sub-unit (the existing, intended behavior for real splits), but the
conductor emits a visible **`unmatched-proposal`** signal — surfaced in the current-
blocker line (§4) — so the extra unit is legible instead of silent. A proposal mapping
to the same baseline id as another is merged, never double-run.

This is the highest-risk unit in the addendum: it touches both the conductor's harvest
path and the planner persona. It is specified with its own tests proving (a) a verbatim
copy still supersedes its baseline, (b) a paraphrase now matches by id instead of
duplicating, and (c) an intentional split still runs and emits the `unmatched-proposal`
signal.

_Code:_ `harvest_proposed`, `normalize_ws`, `baseline_units`, `PLAN_PROTOCOL`
(`src/conductor.rs`).

### 3.4 `--base` reachability + missing-files refusal

**Problem.** The default base `origin/main` is correct (§2.1) but unreachable-to-override
on the commands an operator naturally uses: `rigger workflow <spec> --base <ref>` errors
"expected at most one spec path", and `rigger run --base <ref>` errors "unknown flag".
And nothing checks that the base actually contains the files a spec's criteria name, so
a run anchored on the wrong ref produces doomed units instead of a refusal.

**Fix.**

1. `rigger workflow` and `rigger run` accept `--base <ref>` and thread it to
   `rigger step --base` (the native workflow already threads a `{base}` arg). The
   default is unchanged.
2. Before a run parks its first unit, the conductor extracts path-like tokens from the
   spec's criteria (e.g. `crates/foo/src/bar.rs`, `src/…`) and checks them against the
   base ref. If **none** of the referenced paths resolve in the base, it **refuses**
   with a message naming a missing path and suggesting `--base`:

   > `base origin/main does not contain crates/foo/src/bar.rs referenced by the spec's
   > criteria; pass --base <your-branch> to anchor the run on a ref that has it.`

   The refusal fires only on *total* absence (a strong signal of the wrong base) to
   avoid false positives on partial or paraphrased path references; a partial match
   warns.

_Code:_ `cmd_workflow`, `parse_run_args`, `load_criteria` (`src/main.rs`);
`Worktree::ensure_run_branch` / `ref_resolves` (`src/worktree.rs`).

### 3.5 Version + build provenance an agent can self-serve

**Problem.** An agent cannot determine whether the installed `rigger` binary matches the
source, so the workflow-drift warning is ambiguous about which side is stale, and
resolving it (rebuild the binary vs `rigger setup`) falls back to a human. Having to ask a
human "is the binary current?" is itself the pit-of-success failure this addendum exists
to remove.

**Fix.** `rigger version` / `rigger --version` reports the crate version and a build
provenance identifier (a git commit/describe embedded at build time). The workflow-drift
diagnostic uses that provenance to name which side is stale and the directive fix, never
an ambiguous "they differ". Version is surfaced on the agent-visible paths (`rigger setup`
output, `rigger validate`), and the `using-rigger` skill (§5) tells an agent to check it.

_Code:_ a build script embedding the git commit; a `version` path (`src/main.rs`); the
drift diagnostic in `cmd_validate` (`src/main.rs`).

---

## 4. Workstream B — Make it visible

### 4.1 Current-blocker line

**Problem.** Diagnosing a stall means grepping stats/peers/ps across many polls; the
decisive signals (approved-but-not-integrated, Nth reject recurrence, budget nearly
spent) exist but are buried. The most telling one lives only in `rigger stats`.

**Fix.** A **blocker classifier** computes, from the existing event log and ledger, a
one-line current blocker per in-flight unit, surfaced identically in `rigger status`
and on the dashboard:

```
u-domain   APPROVED 10:14, not integrated (verdict not on result channel)
u-plugin   reject #3/3 -> escalated (genuine-defect: stale-doc)
u-resolver building (attempt 1)                       budget 112/120
```

Classification is a pure function of run state, with a fixed set of blocker kinds
(building, reviewing, reject-recurrence, approved-not-integrated, escalated,
unmatched-proposal, budget-exhausted). It reads the same signals `rigger stats` already
computes; the verdict-channel diagnostic that today only appears in stats becomes one of
the blocker kinds.

_Code:_ `cmd_status` (`src/main.rs`); the stats attribution in `append_review_quality`
(`src/main.rs`); `src/dash.rs`.

### 4.2 Setup + run discoverability + always-on dash

**Problem.** `rigger setup`'s output names only the `/rigger` workflow install line; it
never mentions `/workflows` visibility, the dashboard, or the headless twins. The
dashboard is absent from the operator handbook. And a run never starts the dashboard —
so the state of a running harness is opaque, answerable only by grepping `ps`, tailing
journals, and reading `rigger status`. That forensic dig is the exact opacity this
addendum exists to remove: **an active harness must always have a dash.**

**Fix.**

- `rigger setup` output ends with an orientation block: the blessed native path
  (`/rigger <spec>`, visible in `/workflows`), the dashboard (`rigger dash`, its URL),
  and `rigger workflow` / `rigger run` labelled explicitly as the headless twins.
- **The dashboard is always-on for an active harness.** Whenever any driver (the native
  workflow, `rigger serve`, `rigger run`, `rigger workflow`) has a run in flight, it
  ensures a `rigger dash` is serving that run — auto-started if none is up, on
  `DEFAULT_PORT` or the next free port (so concurrent harnesses each get their own), its
  URL printed at start and shown in `rigger status`, and reaped when the run ends (the
  §4.5 supervised lifecycle). There is no opt-in flag. The earlier instinct to "not spawn
  a server unasked" had the priority inverted: the orphan risk is handled by §4.5, not by
  hiding the dash.

_Code:_ `cmd_setup` (`src/main.rs`); the run entry points `run_workflow` / `run_cli`;
`dash::DEFAULT_PORT`, the dash lifecycle (`src/dash.rs`).

### 4.3 Workflow tagline + live work-line

**Problem.** The workflow's static description reads as internal plumbing ("driven
THINLY", courier, SpawnResult), which is the tagline shown both in the skills list and
the `/workflows` header — useless to an operator. And the display never names the actual
unit being worked.

**Fix.**

- Rewrite the static `meta.description` in `workflows/rigger.js` to a jargon-free,
  user-useful tagline that says what the workflow does and when to use it. The
  architecture explanation moves to (stays in) the file header comment, for maintainers.
  `meta` remains a pure static literal (the runtime extracts it before the body runs),
  so the tagline cannot be per-run.
- Add a human-readable `title` field to `SpawnRequest`, derived from the unit's
  `Stage.coverage` (the criterion text, trimmed to a short line). The thin driver
  renders it in the `log()` narrator and the per-unit progress-group detail, so the live
  display shows the actual work — e.g. *"Building u-domain — feature-off resolver
  test"*. The `title` is the only per-unit display string possible; the tagline is the
  fixed one.

_Code:_ `meta`, `phaseOf`, the `log()` sites (`workflows/rigger.js`); `SpawnRequest`
(`src/spawn.rs`); the `Stage.coverage` source and the wire in `rigger step`
(`src/conductor.rs`, `src/spawn.rs`).

### 4.4 Dashboard responsive redesign

**Problem.** The decision history renders inside an `overflow-x:auto` container with no
wrapping, so long decision text scrolls far to the right and the page body scrolls
horizontally.

**Fix.** A responsive pass on `src/dash.html`:

- Decision/finding text wraps (`overflow-wrap`/`white-space: normal`); decisions render
  as wrapped rows or cards, not a wide non-wrapping block.
- The page **body never scrolls horizontally**; only content that is intentionally wide
  (if any remains) scrolls within its own `overflow-x:auto` container.
- General responsive layout: relative units, the existing `cols2` grid collapses
  gracefully on narrow viewports, content stays readable on small screens.

_Code:_ `src/dash.html`.

### 4.5 No orphaned processes

**Problem.** A run starts long-lived `rigger` children — the MCP `rigger serve` the shim
spawns, the peers sidecar (`Sidecar::start` in `run_workflow`), the dashboard. At least
one is not reaped when the driving agent finishes, leaving an orphaned `rigger` process
that never ends; across a multi-unit campaign these accumulate.

**Fix.** Give every long-lived rigger child a supervised lifecycle — a process-group or
kill-on-drop / kill-on-parent-exit guard — so a normally-finishing or crashing agent
leaves no orphan.

_Code:_ `run_workflow` / `cmd_serve` / `Sidecar::start` (`src/main.rs`), the shim
(`shim/shim.mjs`), the dash lifecycle (`src/dash.rs`).

### 4.6 A wedged run surfaces as a loud error

**Problem.** A unit that exhausts remediation escalates and goes terminal; the run reaches
a clean `done` fixpoint and the native driver resolves *successfully*, the wedge recorded
only as a `UnitEscalated` event. A wedged terminus is indistinguishable from a clean one
unless you inspect escalations — so a unit that can never pass review looks like success.

**Fix.** The conductor's terminal result carries the escalated/unintegrated set; the driver
treats a fixpoint reached with any such unit as a LOUD failure (throws, non-zero, names
them) — exactly as it already does for a `halted` budget stop. Escalation-and-continue
mid-run is unchanged; only the final terminus must not masquerade as success.

_Code:_ the done/step result (`src/conductor.rs`), the driver's done handling
(`workflows/rigger.js`).

### 4.7 No silent hang

**Problem.** `defaults.max_wall_clock` defaults to `0` = unbounded, so a config that does
not set it leaves a hung spawn in-flight indefinitely — a silent hang the liveness sweep
never reaches. (This repo's `workflow.yml` sets 3600; the gap is the default that bites
anyone who does not.)

**Fix.** Without regressing the legitimate long-running unbounded case: the native driver
enforces an outer per-agent wall-clock so an unbounded-config spawn is
abandoned-and-surfaced rather than awaited forever, and `rigger validate` warns when the
default is unbounded and no per-agent bound covers the gating roles. A hung agent surfaces
within a bounded time.

_Code:_ the driver heartbeat framing (`workflows/rigger.js`), the wall-clock default +
validate (`src/config.rs`, `src/main.rs`).

---

## 5. Workstream C — One source of truth (self-documenting discipline)

**Problem.** The operating discipline (when to reach for rigger, the one blessed driver,
spec shape, base anchoring, "fix the loop don't sidestep it", the verdict contract, the
run-branch → PR flow, and the load-bearing decisions of §2) lives only in the handbook.
`rigger setup` installs config, shim, and the `/rigger` workflow, but not this
discipline, so a fresh agent has no loadable front-door. Any scheme that copies the
discipline into a second artifact (a hand-written skill, or markdown fragments embedded
verbatim) just relocates the drift: the copy stays confidently wrong when the code
changes underneath it.

**Design — templated prose, code-derived facts, drift-checked.** The discipline has two
kinds of content, handled differently so the whole document stays accurate to the code:

- **Prose** (the WHY, the discipline, the §2 rationales) lives in **templates**,
  hand-authored, because prose cannot be inferred from code.
- **Facts** (every value that could drift) are pulled from the **real code the runtime
  uses**, not a parallel copy: the default base ref, the dashboard port, the retry
  bound, the verdict-line literal, the subcommand registry, the gating-role list, the
  spec-shape rules. The generator links the crate and reads the same definitions the
  binary runs on, so a fact cannot silently diverge from behavior.

A `rigger docs` command renders the templates against that code-derived context into two
outputs: the **`using-rigger` skill** and the handbook's discipline chapter. The engine
is Rust-native and **compile-time-checked** (the template is validated against its
context type at build time), so a template that references a fact the code no longer
exposes breaks the *build*, not a runtime check. The specific engine is an
implementation-plan choice; the load-bearing properties are (a) facts read from code and
(b) the template checked against the code — not any particular library, and explicitly
not an external toolchain that would require re-exporting the facts (which would
reintroduce the drift surface).

**Drift check.** `rigger validate` (and CI) re-renders and diffs the committed skill and
handbook chapter against a fresh render. Any mismatch — a changed const, a changed
template, an edited-by-hand skill — is a loud failure, exactly like the existing
workflow-drift check. This is what makes the document *stay* accurate rather than merely
start accurate.

**Installation and overlay.** `rigger setup` installs the rendered `using-rigger` skill
(distinct from the `/rigger` workflow skill: the workflow *runs* the loop, the skill
tells an agent *when and how* to drive it). A project-overlay hook lets a repo add its
own specifics — base branch, where specs live — without rewriting the general discipline;
the overlay is merged into the render, so repo specifics and the shared discipline share
one pipeline.

**`using-rigger` content (the front-door).** When to reach for rigger vs not; the one
blessed driver (native `/rigger`, visible in `/workflows` and the dashboard) and the
anti-patterns (polling git/ps by hand, hand-driving `rigger step`, hand-implementing a
unit); spec shape (one observable behavior per criterion, atomic unit = one checkbox,
type shapes in Notes); base anchoring on the working ref; "when it wedges, fix the loop
— never sidestep by hand"; auto-integration on approve (the human PRs the run branch,
never cherry-picks approved units by hand); and — for anyone authoring or porting a
persona — every gating agent ends its output with the verdict line. Plus the §2
load-bearing decisions, so the discipline explains its own constraints.

_Code:_ a new `rigger docs` path (`src/main.rs`), a discipline template set + the
code-fact context, the drift check in `cmd_validate`; `install_skill` in `cmd_setup`.

---

## 6. Workstream D — Provenance & pruning

### 6.1 `rigger reset --runs`

**Problem.** There is no way to shed dead-run noise from the context graph short of
deleting the whole store. Decisions and findings from wedged or superseded runs persist
and surface in a new run's grounding.

**Fix.** `rigger reset --runs` drops decisions and findings belonging to
superseded/dead runs while **preserving `LessonLearned`** (the durable cross-run value
of §2.2). Because the graph has no run column today, node-to-run attribution is derived
from `RunStarted` event-position boundaries: a decision/finding belongs to the run whose
`[RunStarted, next RunStarted)` window contains the event that produced it. The exact
attribution mechanism is confirmed in the implementation plan; this is the addendum's
highest-uncertainty unit and is specified with tests that prove lessons survive and
active-run decisions survive while superseded-run decisions are dropped.

_Code:_ a new reset path (`src/main.rs`); `Projector` and the graph store
(`src/contextgraph/`); the run-boundary source in `src/run.rs`.

### 6.2 `rigger peers` provenance

**Problem.** `rigger peers` presents live and historical decisions identically, so a
stale finding from a superseded run reads as authoritative.

**Fix.** Each decision in `rigger peers` output is labelled **live** (from the active
run) or **historical** (from a superseded run), using the same `RunStarted`-boundary
attribution as §6.1. Grounding still includes cross-run decisions by default (§2.2); the
label makes their provenance legible instead of alarming.

_Code:_ the peers path (`src/main.rs`); run-boundary attribution shared with §6.1.

---

## 7. Delivery

This addendum is decomposed into a comprehensive set of atomic rigger specs under
`specs/`, each authored to the spec-shape rules §3.2 lints for (one observable behavior
per criterion; type shapes in Notes), plus a campaign ordering that respects the
dependencies below. The specs are authored fully up front, then run through the loop —
the loop is the build mechanism (§2.4), and TDD is intrinsic to it.

**Ordering / dependencies:**

- The current-blocker line (§4.1) consumes the `unmatched-proposal` signal (§3.3) and
  the verdict-channel diagnostic (§3.1), so those precede it.
- The `SpawnRequest.title` wire (§4.3) is independent and can land early.
- The self-documenting pipeline (§5) consumes the code facts the other workstreams
  define (the refusal messages, the new flags), so it lands after the CLI surface it
  documents is stable; the drift check lands with it.
- Provenance attribution (§6.1) and its consumer (§6.2) share one attribution mechanism;
  §6.1 precedes §6.2.

## 8. Acceptance — the pit-of-success test

On a fresh clone of a feature-branch project, `rigger setup` leaves a `using-rigger`
skill in place and prints the blessed path plus the dashboard URL. One command to run a
one-criterion spec anchors the run on an explicitly-passable working ref, drives
plan → implement → review → integrate visibly, and auto-integrates on approve. If any
persona or spec is misconfigured in one of the ways above, the tool **fails fast with a
message that names the fix** — never a silent loop:

- a gating persona with no result-channel verdict → hard error at `rigger validate` and
  at run start, naming the fix (§3.1);
- a base ref missing every file the spec's criteria name → refusal suggesting `--base`
  (§3.4);
- a multi-behavior criterion → an advisory lint at author time (§3.2);
- a paraphrased planner proposal → matched by id, or run-and-flagged, never a silent
  duplicate (§3.3);
- a stale decision from a dead run → labelled historical, prunable with
  `rigger reset --runs` (§6).

A canary spec + persona set that deliberately trips each guard proves the tool errors
loudly (or steers the operator away) instead of hanging.
