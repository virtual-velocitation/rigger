//! The self-documenting discipline pipeline (spec 20, unit 1).
//!
//! The operating discipline - when to reach for the loop, the one blessed driver,
//! spec shape, base anchoring, the verdict contract, fix-the-loop-when-it-wedges,
//! and auto-integration on approve - is rendered from ONE typed context so the
//! document cannot silently disagree with the code the binary runs on. Two kinds of
//! content are kept apart so the whole document stays accurate:
//!
//! - PROSE (the WHY, the rationales) lives in the hand-authored template functions
//!   below, because prose cannot be inferred from code.
//! - FACTS (every value that could drift - the default base ref, the dashboard port,
//!   the remediation bound, the verdict-line literal, the spec-shape rules, the
//!   command surface) are carried on [`DocsContext`] and interpolated by typed field
//!   access. A template that references a fact the code no longer exposes fails to
//!   COMPILE (the template is checked against the context type at build time), so a
//!   fact cannot silently diverge from behavior - and there is no external toolchain
//!   that would require re-exporting the facts.
//!
//! The composition root (the binary) populates the context from the real code
//! definitions and calls [`render_using_rigger_skill`] and
//! [`render_handbook_discipline`]. Both outputs render from the SAME context, so a
//! project overlay that overrides a field (unit 3) flows into both through this one
//! pipeline. The render is byte-stable on unchanged inputs (no map iteration, fixed
//! collection order), so the drift check (unit 2) has no false positives.

use std::fmt::Write as _;

/// The typed, code-derived facts the discipline templates interpolate. Every field is
/// a value that could drift from behavior if hand-copied, so the composition root
/// populates each one FROM the code definition the runtime uses (see `docs_context` in
/// the binary). A project overlay merges by overriding fields here BEFORE rendering, so
/// repo specifics and the shared discipline share this one pipeline.
///
/// Removing or renaming a field breaks every template that interpolates it at COMPILE
/// time - that is the load-bearing property: the templates are validated against this
/// type by the build, not by a runtime check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocsContext {
    /// The default ref a run anchors its branch on (`DEFAULT_BASE_REF`).
    pub base_ref: String,
    /// The loopback port the always-on dashboard binds first (`dash::DEFAULT_PORT`).
    pub dash_port: u16,
    /// The bounded-remediation ceiling before a unit escalates (`safety::MAX_RETRIES`).
    pub max_retries: u32,
    /// The verdict value that approves a unit on the result channel, read from the same
    /// definition the integration gate uses (`conductor::VERDICT_APPROVE`).
    pub verdict_approve: String,
    /// The spec-shape lint rule names, in document order (`spec::ShapeRule`).
    pub spec_shape_rules: Vec<String>,
    /// The single recommendation every spec-shape advisory ends with
    /// (`spec::SHAPE_RECOMMENDATION`).
    pub spec_shape_recommendation: String,
    /// The command surface, in registry order (the `SUBCOMMANDS` dispatch registry).
    pub subcommands: Vec<String>,
    /// Where this repo keeps its specs. A project-overlay override point (unit 3);
    /// defaults to the shared convention.
    pub specs_location: String,
    /// The five `rigger watch` signals (spec 69), in Design order (escalated,
    /// dead-driver, dash-not-serving, reject-recurrence, frontier-stall) - each
    /// carrying its canonical name and response, read from `crate::watch::Signal`
    /// (`name()`/`response()`) by the composition root so `rigger-watch-a-run`'s
    /// render never imports `crate::watch` directly.
    pub watch_signals: [WatchSignalFact; 5],
    /// The default `rigger watch` poll interval in seconds
    /// (`crate::watch::DEFAULT_INTERVAL_SECS`).
    pub watch_poll_interval_secs: u64,
    /// The reject-recurrence diagnose threshold `rigger-diagnose-churn` pins its own
    /// procedure text against (`crate::watch::REJECT_RECURRENCE_DIAGNOSE_THRESHOLD`).
    pub reject_recurrence_diagnose_threshold: u32,
    /// The graph-first lookup hook's stated bounce message (spec 92, criterion 4: IN
    /// EVERY SESSION'S HAND), read from the SAME constant `rigger grep-guard` denies
    /// with, so the skill's lookup section can never describe a different rule than the
    /// installed hook actually enforces.
    pub grep_guard_message: String,
}

/// One `rigger watch` signal's canonical name and response (spec 69), carried on
/// [`DocsContext`] so a render function reads it through the one injected channel
/// like every other drift-prone fact in this file, instead of reaching into
/// `crate::watch` directly from production code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WatchSignalFact {
    pub name: String,
    pub response: String,
}

/// Render the `using-rigger` skill: a self-contained front-door that tells an agent
/// WHEN and HOW to drive the loop. Distinct from the `/rigger` workflow (which RUNS the
/// loop); this skill tells an agent when to reach for it and how to stay on the rails.
///
/// The skill opens with loadable frontmatter (a name and a description) so it installs
/// as a discoverable skill, then carries the shared discipline body. Every drift-prone
/// value comes from `ctx`.
pub fn render_using_rigger_skill(ctx: &DocsContext) -> String {
    let mut s = String::new();
    s.push_str("---\n");
    s.push_str("name: using-rigger\n");
    s.push_str(
        "description: When and how to drive rigger - the one blessed driver, spec shape, \
         base anchoring, the verdict contract, and fix-the-loop discipline. Read this before \
         starting or driving a rigger run.\n",
    );
    s.push_str("---\n\n");
    s.push_str("# Using rigger\n\n");
    s.push_str(
        "This is the front-door for driving a rigger run: when to reach for the loop, the \
         one blessed way to drive it, and the rails that keep a run honest. It is generated \
         from the code the binary runs on, so its facts cannot drift from behavior.\n\n",
    );
    s.push_str(&discipline_body(ctx));
    s
}

/// Render the handbook's discipline chapter from the SAME context as the skill, so the
/// two never disagree. The chapter is the operator-manual framing of the discipline.
pub fn render_handbook_discipline(ctx: &DocsContext) -> String {
    let mut s = String::new();
    s.push_str("# Using rigger: the operating discipline\n\n");
    s.push_str(
        "This chapter is the operating discipline for a rigger run: when the loop is the \
         right tool, the one blessed driver, and the rails that keep a run consistent. Its \
         facts are generated from the code the binary runs on, so the chapter cannot silently \
         disagree with how rigger actually behaves.\n\n",
    );
    s.push_str(&discipline_body(ctx));
    s
}

/// The shared discipline body both outputs carry, so the skill and the handbook chapter
/// render from ONE context and can never disagree. Every fact is interpolated from
/// `ctx` by typed field access, so a fact the code stops exposing breaks this at compile
/// time. Written pure-ASCII (hyphens, not unicode dashes) so the drift check has no
/// false positives, and self-contained (it explains the problem each rule solves and
/// names no tool outside rigger's own surface).
fn discipline_body(ctx: &DocsContext) -> String {
    let mut s = String::new();

    let _ = writeln!(s, "## When to reach for rigger\n");
    let _ = writeln!(
        s,
        "Reach for rigger when you have a written spec whose \"Done when\" section \
         enumerates machine-checkable criteria and you want it built, tested, reviewed, and \
         integrated without hand-holding each step. Do NOT reach for it for a one-line edit, \
         an exploratory spike, or work that has no spec to anchor acceptance on - the loop's \
         value is the disciplined lifecycle around a checkable spec, and without one there is \
         nothing for it to hold to.\n"
    );

    let _ = writeln!(s, "## The one blessed driver\n");
    let _ = writeln!(
        s,
        "Drive every run through the native /rigger workflow (visible in /workflows and on \
         the dashboard at 127.0.0.1:{port}). It launches the loop and keeps the event log, the \
         ledger, and the context graph consistent with one another. These anti-patterns split \
         the run's state away from that shared record and must be avoided:\n",
        port = ctx.dash_port
    );
    let _ = writeln!(
        s,
        "- Polling git or ps by hand to guess progress. Read the dashboard or `rigger \
         status`; the by-hand view misses the ledger and the graph."
    );
    let _ = writeln!(
        s,
        "- Hand-driving `rigger step` in a shell. The driver owns stepping; a hand step \
         races the driver and can double-spawn or wedge the frontier."
    );
    let _ = writeln!(
        s,
        "- Hand-implementing a unit the loop parked. That leaves the loop still stuck for \
         the next unit and forks the code from the log - fix the loop instead (see below).\n"
    );

    let _ = writeln!(s, "## Looking things up\n");
    let _ = writeln!(
        s,
        "The knowledge graph is the lookup surface - reach for it before grepping the project's \
         sources. Three verbs answer the three questions you have about the code: `rigger graph \
         --around <file|entity>` (structure: who calls X, and the caller/callee neighborhood), \
         `rigger graph --show <entity>` (text: an entity's definition site and its body), and \
         `rigger peers <file>...` (memory: the prior decisions, findings, and lessons about the \
         files). Grep over the project's sources is a fallback worth reporting, not a habit: if the \
         graph could not answer and you fall back to grep, record it with `rigger progress <id> \
         'grep-fallback: <what the graph did not answer>'` - one line before moving on - so the gap \
         lands in the event log where it can be measured and closed. Filtering your own build or \
         gate output is not a fallback and is not reported.\n"
    );
    let _ = writeln!(
        s,
        "`rigger setup` puts these same lookups directly in THIS session's hands too: it \
         registers a `rigger` MCP server (`.mcp.json`) exposing `rigger_peers`, `rigger_ground`, \
         and `rigger_graph` as tools here - the same three lookups a loop agent has, instead of a \
         shell - and installs a PreToolUse hook that bounces every `Grep` tool call and every \
         `grep`-invoking Bash command in this project, with no target it lets through, with the \
         message \"{message}\" - unless the Bash command ends with a `# --literal` comment (the \
         shell discards the comment, so the marker never reaches grep whichever PreToolUse hook \
         rewrites the command). The graph stays the path of least resistance in this very \
         session, and a literal-text grep is the deliberate act `--literal` names, not a habit.\n",
        message = ctx.grep_guard_message,
    );

    let _ = writeln!(s, "## Graph hygiene before a large run\n");
    let _ = writeln!(
        s,
        "The context graph the loop reasons over is a persistent projection rigger \
         maintains incrementally: each run's decisions and findings are folded in one event \
         at a time as they are emitted, and superseded rows are retired in place rather than \
         re-derived from scratch, so a step never re-folds the whole history. Across many \
         runs graph.db therefore ACCUMULATES the dead-run rows and retired edges that no \
         live query reads, so the file grows on disk without bound even though the live \
         graph the loop grounds on does not. Keep it lean before a large run with `rigger \
         reset --runs`, which prunes that dead-run accumulation and reclaims the disk it \
         held; a very stale graph should be pruned this way first. This is PRE-RUN hygiene \
         through a real command, NOT a hand-driven `rigger step`: hand-stepping races the \
         driver (see the one-blessed-driver anti-patterns above), whereas `rigger reset \
         --runs` is a one-shot prune you run BEFORE launching the loop.\n"
    );
    let _ = writeln!(s, "## Event log hygiene: the derived-index prune\n");
    let _ = writeln!(
        s,
        "The EVENT LOG accumulates separately from the graph, and has its own prune: `rigger \
         reset --derived`. Each run's project-ingest pass records the project's derived index - \
         the code entities, inferred edges, design links, and doc concepts folded from your \
         sources - and a log written before that pass deduplicated across runs holds the WHOLE \
         index once per run, which is re-derivable duplication rather than history. `rigger reset \
         --derived` keeps, for each file, only the recordings of its LATEST generation - the \
         content the log last recorded for it - and of those the LATEST event per replay key, \
         deletes every superseded generation and re-recording, and compacts the file so \
         events.db shrinks on disk. Every other event survives byte-for-byte - lessons, \
         decisions, findings, gate verdicts, and the whole run history `rigger stats` and replay \
         read. The live graph a rebuild folds is unchanged: a newer generation of a file \
         retires every fact the one before it asserted and it does not, so the whole log \
         already folds to each file's latest generation; nothing reads a shed recording again \
         (a file that returns to an earlier content re-emits its batch); and the prune carries \
         a design fact's EARLIEST valid-time within its unbroken run of generations onto the \
         recording it keeps, so a design fact keeps the date it first became true rather than \
         being re-dated to whichever recording survived. WHAT IT CANNOT RECLAIM, because \
         this decides whether it is worth running at all: it never sheds the index itself. The latest generation of every file stays, so on \
         a log that holds each file once, at one recording per key, `rigger reset --derived` \
         deletes ZERO rows from it and reports so - that is the expected report on a clean log, \
         not a failure, and the derived index remains the bulk of the log by design because it \
         is what the graph is folded from. WHEN A DEDUPLICATED LOG STILL HAS SOMETHING TO SHED, \
         because a non-zero prune is otherwise read as a broken dedup: every edit to a file \
         records a new generation of its batch and leaves the one before it superseded, and a \
         file whose content has RETURNED to a generation the log had already recorded - a \
         revert, a branch switch, a checkout back - re-records that file's whole batch by \
         design, since a dedup that suppressed an already-recorded key would strand the graph \
         on the version the file has since moved past. A prune that sheds rows on such a log is \
         shedding exactly that, not covering for a defect; a log written BEFORE the dedup sheds \
         the whole accumulated pile instead. WHAT IT COSTS TO RUN: the compaction rewrites events.db in full and stages \
         a COMPLETE COPY of the log in SQLite's temporary directory while it does, so the free \
         space it needs is on whichever filesystem that resolves to rather than on the partition \
         holding .rigger/ - SQLITE_TMPDIR if you set it, else TMPDIR, else the first of /var/tmp, \
         /usr/tmp, /tmp that exists and is writable, which on a Linux box with TMPDIR unset means \
         /var/tmp and NOT /tmp. Set TMPDIR yourself if the default lands somewhere too small for \
         a second copy of your log. It rewrites only when the FILE is holding reclaimable free \
         pages, which is not the same as this run having deleted something: a prune with nothing \
         to shed from an already-compact log leaves the file exactly as it found it and reports \
         reclaiming zero, while a prune that sheds nothing from a log still holding free pages \
         reclaims them. That is what makes the re-run a real remedy - if the rewrite fails after \
         the deletes have committed, the command still reports what it removed and names the \
         failure, and because the deletes are durable and the space they freed is still free in \
         the file, re-running it is both safe and the way to reclaim that space. The two flags \
         COMPOSE \
         and each prunes its own accumulation: `rigger reset --runs --derived` sheds the dead-run \
         graph rows and the duplicated index in one pass. Both are one-shot maintenance you run \
         BETWEEN runs, never against a live one - and `--derived` ENFORCES that itself: a \
         compaction leaves revision gaps by design, and a writer whose cursor was built before it \
         ran could reissue a gap and reorder the log, so it refuses while the run is live - a \
         `rigger step` holds its lock, an in-flight spawn (one with no recorded result, or only \
         the step's liveness fault) has a liveness marker younger than its wall-clock bound, or \
         a driver registration for this store has a heartbeat inside the idle window - naming \
         what it found. A run whose driver died is not live: units it left non-terminal never \
         block the compaction, a spawn with no marker never does, and an in-flight spawn stops \
         blocking once its marker outlives the spawn's bound or a real result is recorded for \
         it. An unbounded spawn's marker never outlives its bound, so record that spawn's \
         result to end it. Every `rigger step`, `run` and `serve` registers as the run's \
         driver, so the last step's stamp counts as a live driver for the idle window; a \
         courier's (`emit`, `result`, `progress`) discovery refresh of that registration never \
         does. `--force-live` overrides the refusal for an operator certain no writer is using \
         the store; it checks nothing.\n"
    );

    let _ = writeln!(s, "## Spec shape\n");
    let _ = writeln!(
        s,
        "One observable behavior per criterion; the atomic unit is one checkbox; put type \
         shapes and structural detail in a non-criteria Notes section. The loop's spec-shape \
         lint flags these shapes because a planner paraphrases or truncates them when told to \
         copy a criterion verbatim, which then fails the baseline match the conductor \
         reconciles proposals against: {rules}. Recommendation: {rec}.\n",
        rules = ctx.spec_shape_rules.join(", "),
        rec = ctx.spec_shape_recommendation
    );

    let _ = writeln!(s, "## Base anchoring\n");
    let _ = writeln!(
        s,
        "A run anchors its branch on the working ref (default {base}) and reuses that \
         branch once it exists. Anchor on the ref you actually want the work to land on, not \
         a stale default: the anchor is what every unit worktree branches from and every \
         approved unit merges back into, so an anchor on the wrong ref lands the run in the \
         wrong place.\n",
        base = ctx.base_ref
    );

    let _ = writeln!(s, "## When it wedges, fix the loop\n");
    let _ = writeln!(
        s,
        "If a unit will not pass, the fix belongs in the loop - the spec, the gate, the \
         agent, or the config - never a manual edit that sidesteps it. A by-hand fix leaves \
         the loop broken for the next unit and splits the code from the log, so the run can no \
         longer be trusted to replay. Correct the underlying cause and let the loop re-run \
         the unit.\n"
    );

    let _ = writeln!(s, "## Auto-integration on approve\n");
    let _ = writeln!(
        s,
        "An approved unit integrates itself onto the run branch. A human reviews the whole \
         run by opening a pull request FROM the run branch, never by cherry-picking approved \
         units by hand - cherry-picking drops the run's accumulated context and its ordering. \
         A failing unit is retried under a bounded budget (up to {max} attempts) and then \
         escalated to a human rather than spinning forever.\n",
        max = ctx.max_retries
    );

    let _ = writeln!(s, "## The verdict line\n");
    let _ = writeln!(
        s,
        "Every gating agent ends its output with its verdict line: a JSON line carrying \
         {{\"verdict\":\"{verdict}\"}} to approve (or the rejecting value to send the unit \
         back). The integration gate reads that result line, not events recorded through any \
         side channel, so an agent that records its verdict only out-of-band returns no \
         verdict the gate can see and stalls the run. Anyone authoring or porting a gating \
         persona must keep this line.\n",
        verdict = ctx.verdict_approve
    );

    let _ = writeln!(s, "## Self-serve\n");
    let _ = writeln!(
        s,
        "Run `rigger version` to see the exact binary and its build provenance and to \
         diagnose drift between the installed /rigger workflow and the binary that would run \
         it. This repo keeps its specs in {specs}. The full command surface is: {cmds}.\n",
        specs = ctx.specs_location,
        cmds = ctx.subcommands.join(", ")
    );

    let _ = writeln!(s, "## The load-bearing decisions\n");
    let _ = writeln!(s, "The discipline explains its own constraints:\n");
    let _ = writeln!(
        s,
        "- One source of truth: every drift-prone fact in this document is read from the \
         code the binary runs on, so the document cannot silently disagree with behavior. A \
         drift check re-renders and diffs it, so it stays accurate rather than merely starting \
         accurate."
    );
    let _ = writeln!(
        s,
        "- Blast-radius isolation: each unit does its work in its own worktree, so \
         concurrent units never clobber one another and every unit's change is reviewed on its \
         own diff."
    );
    let _ = writeln!(
        s,
        "- Fail-closed review: only an explicit approve verdict integrates a unit; a \
         missing, unparseable, or rejecting verdict routes the unit back to remediation rather \
         than passing it silently."
    );

    s
}

/// The `planning-a-spec` skill's body: the authoring procedure for writing, splitting, or
/// amending a spec before or during a rigger loop run (the authoring counterpart to
/// `using-rigger`'s driving discipline). Unlike `using-rigger`, it carries no code-derived
/// facts to interpolate, so it is a plain constant rather than a `DocsContext`-parameterized
/// template.
const PLANNING_A_SPEC_BODY: &str = r#"---
name: planning-a-spec
description: Use when writing, splitting, or amending a spec for a rigger loop run - before launching /rigger on new work, when a plan-critique gate rejects a decomposition, when a run churns in review and the spec is suspect, or when turning bug reports or design discussions into Done-when criteria.
---

# Planning a spec

## Overview

A loop run's outcome is mostly decided at spec time. This skill is the authoring procedure for
the failure catalog in `docs/handbook/planning-field-guide.md`; the shape rules live in
`docs/handbook/authoring-loops.md` (rules 1-8). Follow the recipe in order - each step exists
because skipping it has a recorded escalation attached.

## The recipe

**1. Ground the Goal in evidence.** State the problem with measured numbers and real anchors
(`file.rs:line`, event counts, durations) - look them up via `rigger graph --show/--around` and
`rigger peers`, not memory. A goal an implementer can re-verify is a goal an adjudicator can
hold the line on.

**2. Close every disposition.** Scan the draft for "or", "either", "could", "worth
considering". Each becomes a decision recorded in Design ("BACKEND SCOPE, decided here so no
unit has to: ...") or an explicit Notes deferral OUT of scope. A disposition left open is a
rejection loop: implementer picks one reading, reviewer picks the other.

**3. Run the constraints walk.** For every Global constraint x every criterion (and every
mechanism Design prescribes), walk the corner-case list: empty, repeated, REVERT/rollback,
DROPPED (a fact present in an earlier generation and absent in a later one), concurrent actors,
crash-resume, cold start (fresh process, empty memory), existing data (a store or tree that
predates the mechanism). Write what must happen into the spec. If a prescribed mechanism fails
a corner under a constraint, the spec is self-contradictory - fix it now; the panel will
otherwise find it around attempt 5.

**4. Place state explicitly.** Any criterion about dedup, persistence, recovery, budgets, or
caches names WHERE the authoritative state lives (the log, a file, a flock) and names the
inadequate stand-in ("an in-memory seen-set is NOT an implementation of this guard") so the
easy-but-wrong implementation is rejected by the text, not by attempt 4's adversary.

**5. Write criteria to the criterion contract.** Each checkbox is:
- ONE observable behavior, self-contained in one-to-two sentences, copyable verbatim as a
  unit's whole contract (the planner copies it; the conductor baseline-matches the copy);
- named verification ("a test proves X ... pinned at the Y seam"), not just a state;
- ownership INSIDE the checkbox ("This criterion OWNS the selection surface") with exclusions
  on every neighbor that could claim the concern ("the advisory is criterion 3's, NOT this
  one's");
- the code it touches named in backticks (`run_wave`, `crates/x/src/ingest.rs`). The unit's
  blast radius grounds only on those spans and identifier-shaped words, never on prose, so a
  criterion naming no code runs alone, and a multi-word span (`rigger validate`) matches as one
  phrase.
Type shapes, tables, long detail: a non-criteria Notes section. Two behaviors joined by "and":
two checkboxes.

**6. Carry the house constraints.** Hyphens not em dashes (U+2014 fails the diff gate); both
feature lanes green; no new event type unless the spec's whole point is one; fallback stated
for any criterion that might be impossible; anything the gates cannot see flagged for the
adjudicator to demand evidence on.

**7. Preflight, then launch.** Run the `spec-preflight` skill and, under a workflow with a
critic, `rigger critique <spec>` before launch. `rigger validate` is mandatory (it catches
model-alias drift - run `rigger canary --if-model-changed` on a warning); `rigger reset --runs`
before a large run; anchor `base=` on the ref the work must land on. Launch via the /rigger
workflow only.

## Amending mid-run

Design and Global constraints only - criteria checkboxes are the run's identity (editing one
orphans the live run). Commit when no step is mid-flight, then `rigger emit DecisionMade` naming
the spec file so in-flight reviewers see the change through the graph immediately. Still
escalates? Restart fresh: durable branches carry the work, the budget resets.

## Quick reference: churn signature -> planning defect

| Signature in the run | Catalog class | Fix at spec time |
|---|---|---|
| Plan-critique rejects twins with identical criteria | F1 ownership | OWNS inside checkbox + neighbor exclusions |
| Plan-critique names one criterion, two mitigations | F2 bundling | Split the checkbox |
| Panel rejects every mechanism variant; `cause: spec-ambiguity` | F3 contradiction | Constraints walk (esp. revert) |
| Implementer and reviewer disagree on a reading | F4 disposition | Decide it in Design |
| Guard/dedup rejected as process-local | F5 state placement | Name where state lives + the banned stand-in |
| Plan baseline-match fails, paraphrased units | F6 copyability | One-sentence criteria, detail to Notes |
| First run after a while churns everywhere | F7 environment | validate preflight + canary on drift |
| High attempt counts, findings about worktrees/caches/quota | F8 infra noise | Audit findings; fix infra separately |
| A ratified unit rejected again each round for a new prose claim | F9 claim surface | Bound the claim surface in the criterion; delete an unowed claim rather than qualify it |
| Plan-critique rejects two criteria that each need the other's change | F10 landing-order circularity | Landing-order simulation (spec-preflight step 1) |
| Identity claim rejected with `cause: spec-ambiguity` once a fact is dropped | F11 undecided removal | DROPPED corner + named comparison surface (spec-preflight step 2) |
"#;

/// The `spec-preflight` skill's body (spec 112, criterion 4): the pre-launch procedure that runs
/// after `planning-a-spec` - the landing-order simulation, the per-criterion corner walk and the
/// `rigger critique` adversary pass, then resolving and recording every BLOCKING finding. Spec
/// 112 ships it byte for byte as the fenced block closing its Notes section. Like
/// [`PLANNING_A_SPEC_BODY`], it carries no code-derived facts, so it is a plain constant.
const SPEC_PREFLIGHT_BODY: &str = r#"---
name: spec-preflight
description: Use before launching any rigger spec, after planning-a-spec - simulate landing order and walk every criterion's corners, then run rigger critique on the spec and close every BLOCKING finding before launch.
---

# Spec preflight

## Why

A checklist read in the author's head does not simulate the run. Two spec defects survive
planning-a-spec and each costs a whole loop round:

- Landing-order circularity (F10): two criteria assert the same measured property of one
  command, and each one's fixture needs the other's change. Whichever unit lands first fails its
  own text, and plan-critique finds it only at run time.
- Undecided removal (F11): a criterion claims an identity (byte-identical, equal) and no Design
  sentence decides what a later generation that DROPS a fact does to it. The corner walk never
  reaches it, the implementer narrows the fixture, and the round is lost.

This skill makes you EXECUTE two simulations and one adversarial pass before launch. It does not
repeat planning-a-spec (`../planning-a-spec/SKILL.md`); run that recipe first, then this.

## When

- After the draft passes planning-a-spec.
- Before `rigger validate`.
- Before any launch or relaunch.
- Again after any mid-run Design amendment.
- Never skipped for a small spec.

## Step 1: landing-order simulation

1. Table the criteria in the order their units land (follow the needs chain; independent units
   in any order).
2. List every measured surface each criterion asserts over: a command, a store read, a file, a
   counter, a test double.
3. For every ORDERED PAIR (A, B) sharing a surface, write one line:
   `If A lands first, on a tree WITHOUT B, does A's own text hold? yes/no - why.`
4. Every "no" is a defect. Fix it now, one of three ways:
   - move the assertion to the later unit;
   - split ownership at the seam in a Design block named `CRITERIA A AND B SPLIT AT <seam>`;
   - make the earlier criterion's assertion conditional on what exists at its landing.
5. Rule: a criterion is testable in isolation on the tree it lands on, never only on the
   finished tree.

Worked example (shared surface: the reads one command makes):

    C2 first, without C3: C2 asserts "the command reads no derived event", but excluding
      derived reads needs C3's query to serve them -> NO.
    C3 first, without C2: C3 asserts the same, but its seed relies on C2's exclusion -> NO.
    Fix: Design block "CRITERIA 2 AND 3 SPLIT AT the derived-read seam": C2 owns the
      exclusion, C3 owns the query; only the later-landing unit asserts "reads no derived event".

## Step 2: per-criterion corner walk

For EACH criterion's mechanism (not only per global constraint), write one sentence per corner,
or "out of scope - <reason>":

| Corner | Question |
|---|---|
| empty | No input, no rows, no prior generation. |
| repeated | The same input twice. |
| reverted | An earlier state re-asserted later. |
| DROPPED | A fact, row, file or link present in an earlier generation and absent in a later one. |
| concurrent | Two actors on the mechanism at once. |
| crash-resume | The process dies mid-mechanism; the next run resumes from the log. |
| cold start | A fresh process, empty memory. |
| existing data | A store or tree that predates the mechanism - the upgrade path. |

Rules:
- Every identity claim (identical, equal, the same, byte-identical) names its comparison
  surface: which bytes, which projection, which ordering.
- Its fixture includes the DROPPED corner explicitly. A fixture that only adds or moves facts
  does not test an identity claim.
- A corner no Design sentence decides is a defect; decide it in Design now.

Worked example (an identity claim over a compacted log):

    DROPPED: generation N links doc D -> E; generation N+1 re-ingests D without the link.
      The compacted log keeps only N+1; the original log holds N and N+1. Undecided: does
      N+1 supersede N's link in the fold? If not, the two rebuilds differ -> the claim is false.
    Fix: Design "a generation supersedes the whole prior generation; the comparison surface is
      the live projection, not raw bytes"; the fixture drops a link and drops an entity.

## Step 3: the adversary pass

Run `rigger critique <spec>`. It runs the workflow's critic (the plan-critique gate's adversary,
else `defaults.review.adversary`) against the spec text, records its findings and verdict keyed on
the text's content hash, and prints them; unchanged text is answered from the record, and any edit
is critiqued afresh. Under a workflow naming neither, `rigger critique` refuses and runs are not
gated, so skip Steps 3 and 4.2. Each finding is one line:

    <id> | BLOCKING or NON-BLOCKING | <criterion or Design block> | <exact reading that breaks> | <smallest Design change that closes it>

## Step 4: resolve, record, re-run

1. Close every BLOCKING finding in Design or Global constraints. Once a run has built code, never
   edit a criterion. Before any run, a criterion rewrite is allowed, and Steps 1-3 then run again
   from the top.
2. Run `rigger critique <spec>` on the amended text until it returns no BLOCKING finding. Close or
   record each NON-BLOCKING one.
3. Close a BLOCKING finding you judge wrong, or already decided, with a recorded resolution
   instead of a text change (`<ids>` are the quoted finding ids):

       rigger emit DecisionMade '{"id":"preflight-<short>","governs":["<spec>"],"resolves":[<ids>],"summary":"closed <ids>: <one line each>"}'

4. `rigger validate <spec>`.
5. Launch.

Refusal rule: do not launch, relaunch, or amend-and-continue with an open BLOCKING finding.

## Mid-run amendment

When a review or plan-critique reject names a defect in the spec itself:

1. Amend Design and Global constraints only; a criterion edit orphans the live run, and a
   plan-critique stop is closed by critiquing the amended spec (`rigger critique <spec>`), then
   relaunching on it, which begins a new run.
2. Land the amendment between steps, never while a step is mid-flight.
3. `rigger emit DecisionMade` with the spec path in `governs`, so in-flight agents see it through
   the graph.
4. Run Steps 1-3 on the amended spec before the next step spawns.
"#;

/// The planning field guide's body, committed as-is at
/// `docs/handbook/planning-field-guide.md` (spec 66, criterion 2): the failure catalog
/// (F1-F11) this repo's own event history recorded, the mid-run amendment protocol, and
/// measured outcomes. This is a HANDBOOK PAGE, not a skill (it carries no frontmatter and
/// is not in [`skill_registry`]) - it is the "how to produce a loop-ready spec" companion
/// the `planning-a-spec` skill and `authoring-loops.md`'s shape rules both point readers
/// at. Self-contained: a consumer needs no access to this repo's history to use it. Like
/// [`PLANNING_A_SPEC_BODY`], it carries no code-derived facts, so it is a plain constant
/// rather than a `DocsContext`-parameterized template, which the binary's handbook-page list
/// holds as a [`DocBody::Static`].
pub const PLANNING_FIELD_GUIDE_BODY: &str = r#"# Planning a loop run: the field guide

The handbook's [authoring-loops](authoring-loops.md) rules say what a loop-ready spec IS. This
guide is the other half: how to PRODUCE one, distilled from this repository's own event store -
every escalation, plan-critique rejection, and multi-attempt review churn it has recorded. The
one-sentence summary of all of it: **almost every expensive run failure was decided before the
run started, at planning time, and each failure class below has a mechanical countermeasure.**

## The failure catalog

Each class below appeared in the recorded history at least once; the recurring ones are marked.
Read this before writing a spec; the authoring recipe that follows exists to make each class
structurally impossible.

### F1 - Duplicated or ambiguously-owned units (the #1 recurring killer)

Six separate plan-critique escalations trace to one shape: the planner turns one criterion into
TWO units - byte-identical criterion text, identical blast radius, no exclusion naming which
twin owns what ("mechanism" and "proof" twins, "scaffold" and "seam" twins, or whole DAGs
duplicated as parallel chains). The reviewers then enforce each twin against the other and
neither converges.

**Countermeasure:** the OWNS sentence lives INSIDE the criterion checkbox ("This criterion OWNS
the selection surface"), and every neighbor that could plausibly claim the same concern carries
the exclusion by name ("the orphan-id advisory is unit-9's, NOT this unit's"). A criterion whose
ownership sentence sits outside the checkbox gets truncated away when the planner copies
criteria verbatim into units - that truncation is a recorded failure, not a hypothesis.
When the planner itself splits one criterion into parts, each part keeps the criterion's
verbatim text and adds its own OWNS sentence after it; the conductor keeps that sentence after
the exact criterion as the part's contract, so the parts never reach the plan critique as twins.

### F2 - Bundled criteria

One checkbox demanding two mitigations ("a drift monitor AND distilled playbooks") forces the
planner to either split it (creating F1 twins) or build an unreviewably wide unit. The
plan-critique gate correctly escalated a spec for exactly this.

**Countermeasure:** one observable behavior per checkbox. If the sentence contains "and" between
two verifiable outcomes, it is two criteria wearing one checkbox - split it yourself rather
than letting the planner guess.

### F3 - The self-contradictory spec (the most expensive single failure)

When a spec states requirements in more than one form - a prescribed mechanism, asserted
properties of that mechanism, and independent constraints on the outcome - the forms can
contradict each other in a corner case the author never walked, and the contradiction is
invisible until an implementation reaches it. No implementation can satisfy a contradiction:
every attempt violates one clause or another, each rejection is individually correct, and the
run churns until someone re-reads the SPEC instead of the diffs. The tell is rejections that
keep citing the same constraint against different, otherwise-reasonable implementations - or a
review verdict that names the cause as spec ambiguity outright. (Recorded cost: six attempts
rejected against one unwalked corner - a file reverting to earlier content - before the spec
was amended.)

**Countermeasure:** the constraints walk. Take every Global constraint and every criterion and
walk them against the standard corner-case list: empty input, repeated input, REVERT/rollback to
a prior state, DROPPED (a fact present in an earlier generation and absent in a later one),
concurrent actors, crash-resume, cold start (fresh process, empty caches), existing data (a store
or tree that predates the mechanism - the upgrade path). A constraint you have not walked
against a corner case is a rejection you have scheduled for attempt 5. When Design prescribes a
mechanism, the walk applies to the mechanism too - or drop the prescription and let the criteria
state observables the implementer must find a mechanism for.

### F4 - Open dispositions

"Removed" and "ignored" are different verdicts on the same files; if the spec has not picked
one, the implementer picks one and a reviewer picks the other (a recorded rejection loop). Any
question a reviewer could reasonably re-litigate - backend scope, what happens on the degraded
path, whether a doc updates - is a disposition the spec must close.

**Countermeasure:** grep your draft for every "or", "either", "could", and "worth considering" -
each is either a decision to make now or a Notes line explicitly deferring it OUT of scope.
Recent specs close these with explicit "BACKEND SCOPE, decided here so no unit has to" blocks;
that pattern generalizes: decide it where you noticed it.

### F5 - State that lives in the wrong place

A guard against CROSS-PROCESS duplication implemented as an in-process seen-set defends nothing:
every driver step and every cold rebuild is its own process, so the set starts empty exactly
when it matters. The class generalizes: any criterion about persistence, dedup, recovery, or
budgets must say WHERE the authoritative state lives (the log, a file, a lock) and the spec must
reject in-memory stand-ins by name, or an implementer will reach for the easy one and a reviewer
will (correctly) reject it late.

### F6 - Criteria that cannot survive verbatim copying

The planner copies criteria into units verbatim and the conductor reconciles proposals against a
baseline match. Over-long criteria, sub-bullets-as-units, and multi-sentence checkboxes get
paraphrased or truncated in that copy, and the mismatch fails the reconcile. The spec-shape lint
flags these; heed it before launch rather than after the plan escalates.

**Countermeasure:** a criterion is ONE self-contained sentence-or-two, copyable as a unit's
whole contract. Type shapes, tables, and long detail go in a non-criteria Notes section.

### F7 - Unpinned environment

Agent models resolve through aliases, and an alias can silently re-point between runs (a
recorded re-point preceded the churniest run in this repo's history and was flagged only by
`rigger validate`, which nobody ran). Gates, corpus, and binary are part of the same
environment.

**Countermeasure:** `rigger validate` is a MANDATORY preflight, not a linter you run when
curious. On a model-drift warning, run the canary (`rigger canary --if-model-changed`) before
trusting a big run to the new resolution.

### F8 - Infra noise misread as semantic failure

Attempt counts inflate from harness defects (assigned worktrees deleted between spawns, shared
build caches thrashed by concurrent lanes, agents killed by quota exhaustion). A run that "took
6 attempts" may have burned half of them on infrastructure. Reacting to the raw count - blaming
the spec, the model, or the panel - misdiagnoses it.

**Countermeasure:** before reacting to churn, audit the blocking findings against the diffs
(they cite checkable facts) and separate infra findings from semantic ones. Fix infra in the
binary via its own spec; never let it masquerade as review strictness.

### F9 - Unbounded claim surface: prose that says more than the artifact owes

Every statement in a reviewed artifact is a claim that can be falsified, and prose invites
stronger claims than anything else: universal quantifiers ("never", "only", "all"), exhaustive
enumerations, and reassuring guarantees nobody asked for. Under adversarial review each
unnecessary claim is an independent way to fail - and remediation makes it WORSE by default,
because the natural way to fix a falsified statement is to write a longer, more qualified one,
which adds new claims to falsify. The result is an artifact whose verified core is done while
its claim surface grows a fresh defect per round and never converges. (Recorded cost: four
consecutive rejections of a unit whose code was ratified and untouched throughout.)

**Countermeasure, at spec time:** bound the claim surface in the criterion itself - demand the
RULE stated short and pinned by an accuracy check, and say explicitly that no enumeration of
cases or guarantees beyond it is owed. **At remediation time:** prefer deletion to replacement -
a claim the artifact does not owe is removed, not repaired - and treat any fix that ADDS a
universal as the failure mode repeating.

### F10 - Landing-order circularity

An F3 shape, found by the landing-order simulation. Two criteria assert the same measured
property of one surface - a command, a store read, a file, a counter - and each one's fixture
needs the other's change, so whichever unit lands first fails its own text on a tree without the
other. The tell is a plan-critique reject naming two criteria that each need the other's change,
and a re-plan that draws the same reject.

**Countermeasure:** simulate landing order before launch. For every ordered pair of criteria
sharing a surface, ask "if A lands first, on a tree without B, does A's own text hold?" Each
"no" is a defect to fix before launch: move the assertion to the later unit, split ownership at
the seam in a named Design block, or make the earlier criterion's assertion conditional on what
exists at its landing. The `spec-preflight` skill runs this simulation as its first step.

### F11 - Undecided removal

An F3 shape, found by the DROPPED corner of the corner walk. A criterion claims an identity
(byte-identical, equal, the same as) and no Design sentence decides what a later generation that
DROPS a fact - a row, a link or a file present before and absent after - does to it. A corner
walk without the DROPPED corner does not reach it: the implementer narrows the fixture to
inputs that only add or move facts, and an adjudicator rejects the built unit with cause
`spec-ambiguity`. The tell is an identity claim that names no comparison surface, or a fixture
that never removes anything.

**Countermeasure:** every identity claim names its comparison surface (which bytes, which
projection, which ordering), Design decides its DROPPED corner, and its fixture drops a fact
explicitly. The `spec-preflight` skill walks this corner for every criterion as its second
step.

## Amending a spec mid-run

Sometimes the panel proves the spec wrong while the run is live (F3 was caught exactly this
way). The protocol, validated in production:

1. Amend Design and Global constraints ONLY. The criteria checkboxes are the RUN'S IDENTITY -
   the conductor adopts a run by matching criteria, so editing a checkbox mid-run orphans the
   live run. Criteria changes wait for a fresh run.
2. Commit the amendment to the run branch when no step is mid-flight.
3. Emit the clarification as a decision (`rigger emit DecisionMade ...` naming the spec file) -
   in-flight reviewers ground through the knowledge graph and see it IMMEDIATELY, ahead of
   their worktrees picking up the text.
4. If the run still escalates, restart FRESH under the amended spec: durable unit branches
   carry the work forward, the budget resets, and the graph's findings steer the new attempts.

## What good looks like, measured

Runs whose specs followed all of the above have recorded 85-100% first-pass yields, zero
escalations, and flawless 6-wave convergences. The disasters (40 rejections over three
criteria; six attempts against a contradiction; four plan-critique rounds) each map to a
catalog class above. The delta is not model quality or reviewer mood - it is whether the spec
closed these holes before `rigger run` ever started. Use the `planning-a-spec` skill to apply
this guide as a procedure.
"#;

/// A shipped document's body: rendered from the code-derived facts a [`DocsContext`]
/// carries, or a static body committed as-is that no context reaches. The skill registry and
/// the binary's handbook-page list hold one per entry, so a static document needs no adapter
/// fitting it to a render signature: [`DocBody::render`] is the one renderer that ignores
/// `ctx` (`d112-op-seam-items-from-c4`).
#[derive(Clone, Copy)]
pub enum DocBody {
    /// Rendered from the docs context.
    Rendered(fn(&DocsContext) -> String),
    /// Committed as-is (`planning-a-spec`, `spec-preflight`, the planning field guide).
    Static(&'static str),
}

impl DocBody {
    /// This body's text for `ctx`: a rendered body's render, a static body as written.
    pub fn render(&self, ctx: &DocsContext) -> String {
        match self {
            DocBody::Rendered(render) => render(ctx),
            DocBody::Static(body) => (*body).to_string(),
        }
    }
}

/// The fixed shape every per-operation skill renders in: frontmatter naming it, its title,
/// any `preamble` paragraphs, then its Procedure, Anti-move and See also sections - each
/// paragraph written as its author wrote it, followed by a newline. The one place that shape
/// lives, so each skill's own render carries only its content.
fn render_operation_skill(
    name: &str,
    description: &str,
    preamble: &[&str],
    procedure: &[&str],
    anti_move: &str,
    see_also: &str,
) -> String {
    let mut s = String::new();
    let _ = writeln!(
        s,
        "---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n"
    );
    for paragraph in preamble {
        let _ = writeln!(s, "{paragraph}");
    }
    let sections: [(&str, &[&str]); 3] = [
        ("Procedure", procedure),
        ("Anti-move", &[anti_move]),
        ("See also", &[see_also]),
    ];
    for (heading, paragraphs) in sections {
        let _ = writeln!(s, "## {heading}\n");
        for paragraph in paragraphs {
            let _ = writeln!(s, "{paragraph}");
        }
    }
    s
}

/// Render the `rigger-reset-store` skill (spec 68, criterion 2): store hygiene for the
/// three files under `.rigger/`. `ctx` is accepted only to match the registry's uniform
/// signature; nothing here is drift-prone enough to interpolate from it.
fn render_reset_store_skill(_ctx: &DocsContext) -> String {
    render_operation_skill(
        "rigger-reset-store",
        "Store hygiene for rigger's own state - growing .rigger/ disk usage, \
         the bloat advisory from `rigger validate`, or `rigger step`/replay running slow. \
         Read this before running `rigger reset` or touching any store file by hand.",
        &[
            "rigger keeps three stores under `.rigger/`, and only one of them holds anything \
             durable:\n",
            "- `events.db` - the event log. This IS the truth: every decision, finding, gate \
             verdict, and run milestone rigger has ever recorded, in the order it happened. \
             Nothing else derives it; it derives everything else.",
            "- `graph.db` - the context graph. A REBUILDABLE projection folded from the event \
             log: rigger-build-graph regenerates it from `events.db` alone, so losing it loses \
             time, never truth.",
            "- `progress.db` - live per-agent progress telemetry. Never replayed into a run's \
             state; it is a side channel `rigger status` and the dashboard read to show what an \
             agent is doing right now, not a record anything else depends on.\n",
        ],
        &[
            "`rigger reset` with no flags is the MENU, not an error: it exits 0 and prints one \
             line per prunable accumulation, each with a measured count and the flag that acts \
             on it. It is read-only - safe to run any time just to look.\n",
            "- `rigger reset --runs` prunes dead-run rows and superseded edges out of \
             `graph.db`. It works over ANY event-store backend (the graph is always a local \
             file); rerun it any time, especially before a large run. When no driver is alive \
             (no `rigger step` holds the lock, no in-flight spawn's liveness marker is younger \
             than its wall-clock bound, no driver registration for the store has a heartbeat \
             inside the idle window) and every spawn of the run has ended on a real result, it \
             also closes the current run's units whose branch work is already landed on \
             `rigger-run`: a unit landed by hand gets the `UnitIntegrated` only the conductor \
             mints, so `rigger status` stops reporting the finished run as working. It only \
             appends; a live run is left untouched. A spawn answered only by the step's liveness \
             fault has not ended - the step halts on it and a relaunched driver re-parks it - so \
             a dead run with a hung spawn stays open and the reset names it: record its real \
             result with `rigger result <id>`, then rerun `rigger reset --runs`. A `rigger \
             step` registers as the run's driver just as `run` and `serve` do, so a hand-landed \
             unit closes once the last step's stamp is older than the idle window; a courier's (`emit`, `result`, `progress`) discovery refresh never \
             counts as a driver, so your own courier just before the reset never holds it back.",
            "- `rigger reset --derived` compacts `events.db`: it keeps only each file's latest \
             generation of the derived index, at the latest event per replay key, deletes the \
             superseded generations and re-recordings, and vacuums so the file shrinks on disk. \
             Every other event - every decision, finding, lesson, gate verdict, the whole run \
             history - survives byte-for-byte. Only the embedded sqlite backend can compact \
             this way, and it refuses (unless overridden with `--force-live`) while a run is \
             live against the store.",
            "- `rigger reset --build-cache` reclaims the rebuildable scratch beside the \
             stores: every dead class `rigger validate`'s footprint names with this verb (dead \
             per-unit caches, dead spawns' registered scratch, unowned agent scratch) and the \
             shared gate build cache. It checks each entry for a live holder first - a process \
             whose working directory or open file is inside - and leaves a held entry where it \
             is. It needs no event-store backend.",
            "- The flags compose: `rigger reset --runs --derived` sheds both store \
             accumulations in one pass.\n",
        ],
        "Never touch `events.db`, `graph.db`, or `progress.db` with raw SQL, `rm`, or any \
         tool outside `rigger reset`. The event log is append-only truth: a hand-edit or a \
         hand-deleted row can desync the graph from the log in ways `rigger reset \
         --derived`'s own compaction is specifically built to avoid. A store \
         file that is genuinely corrupt is an incident to fix at its root, never a reason \
         to reach for a database client.\n",
        "rigger-build-graph if `graph.db` needs regenerating rather than pruning; \
         rigger-reindex if only the symbols index is stale.\n",
    )
}

/// Render the `rigger-build-graph` skill (spec 68, criterion 2): the cold-build entry
/// point for the context graph. `ctx` is accepted only to match the registry's uniform
/// signature; nothing here is drift-prone enough to interpolate from it.
fn render_build_graph_skill(_ctx: &DocsContext) -> String {
    render_operation_skill(
        "rigger-build-graph",
        "Cold-build the context graph - empty `rigger graph --around`/`--show` \
         lookups on a repo that already has source, or a first setup before any run exists. \
         Read this before deleting a store file to force a re-ingest.",
        &[],
        &[
            "`rigger graph build` folds the project's source straight into `.rigger/graph.db` - \
             no run, no `RunStarted`, nothing but the code-ingest events the fold already emits. \
             It CREATES the store when the checkout is cold (`.rigger/` does not exist yet) and \
             REFRESHES an existing store incrementally: an unchanged file re-ingests nothing, and \
             it reuses the exact same walk-and-content-key ingest authority a live run uses, so a \
             standalone build and a run can never fold the same file under two different keys.\n",
            "Rerun it any time it is convenient - on a schedule, after pulling a large set of \
             changes, or simply because a lookup came back empty and you want to check. It is \
             always safe: nothing is deleted, only appended and incrementally refreshed.\n",
        ],
        "Never force a rebuild by deleting `.rigger/graph.db` (or `events.db`) and \
         re-running `rigger graph build` on the empty result. Deleting the log throws away \
         truth that no rebuild can get back, and deleting only the graph is unnecessary work \
         `rigger graph build` already does FOR you, incrementally, without erasing anything \
         first. If lookups are empty, just run `rigger graph build`; only reach for \
         rigger-reset-store if you specifically mean to prune, not rebuild.\n",
        "rigger-reindex for a narrower staleness problem - one that is really about the \
         symbols grounding index, not the whole structural graph; rigger-reset-store for \
         pruning `graph.db`'s dead-run accumulation rather than rebuilding it.\n",
    )
}

/// Render the `rigger-reindex` skill (spec 68, criterion 2): the targeted refresh for the
/// symbols grounding index. `ctx` is accepted only to match the registry's uniform
/// signature; nothing here is drift-prone enough to interpolate from it.
fn render_reindex_skill(_ctx: &DocsContext) -> String {
    render_operation_skill(
        "rigger-reindex",
        "Refresh the symbols grounding index - a `rigger graph`/`rigger ground` \
         lookup that names an entity the current tree no longer holds, or the \
         index-staleness advisory from `rigger validate`. Read this before rebuilding the \
         whole graph over an index-freshness problem.",
        &[],
        &[
            "`rigger reindex <file>...` re-parses ONLY the named files and persists the delta to \
             the project's symbols grounding index at `.rigger/symbols/` - the fast, targeted fix \
             for an index that has drifted from files you just changed (a unit's own commit, a \
             rebase, a branch switch). It is scoped strictly to the symbols index, a DIFFERENT \
             store from the structural context graph, so it costs only the named files, never a \
             walk of the whole tree.\n",
            "Name every file whose content changed since the index was last built; an unnamed \
             file's stale entry is left exactly as it was.\n",
        ],
        "Do not reach for a whole-graph rebuild (see rigger-build-graph) or a store wipe to \
         fix a lookup that is really an index-freshness problem: naming the stale files and \
         reindexing exactly them is both cheaper and more targeted than rebuilding the whole \
         structural graph over a handful of drifted entries. Reserve a whole-graph rebuild \
         for when the graph itself is missing or empty, not for a symbols lookup that just \
         needs the files it names re-parsed.\n",
        "rigger-build-graph for the whole-project structural graph; rigger-reset-store for \
         the stores' own hygiene.\n",
    )
}

/// Render the `rigger-resume-a-run` skill (spec 68, criterion 2): continuing a run after
/// its driver died mid-flight. `ctx` is accepted only to match the registry's uniform
/// signature; nothing here is drift-prone enough to interpolate from it.
fn render_resume_a_run_skill(_ctx: &DocsContext) -> String {
    render_operation_skill(
        "rigger-resume-a-run",
        "Continue interrupted work after a dead driver (spent quota, a crash, a \
         laptop that slept mid-run) or `rigger status` showing an agent 'in flight' with a \
         stale heartbeat. Read this before relaunching a run or reaching for `--fresh`.",
        &[],
        &[
            "Diagnose first: `rigger status` (or the dashboard) shows each in-flight agent's \
             last progress report and heartbeat age. A stale heartbeat with no recent store \
             event means the DRIVER died mid-run (quota ran out, the process crashed, the \
             machine slept) - it does not mean the run itself is broken; the event log already \
             holds every decision and gate verdict the run made before the driver stopped.\n",
            "Relaunch the same blessed driver on the same spec WITHOUT `--fresh` - `rigger run \
             <spec>`, `rigger serve <spec>` / `rigger workflow <spec>`, or the native `/rigger \
             <spec>` workflow with its `fresh` argument left unset. Because the run lives in the \
             event log, not in the dead process, the conductor's own run-starting step adopts \
             the existing run instead of minting a new one: it replays the log, rebuilds its \
             in-memory state, and continues exactly where the dead driver left off. No unit \
             restarts from zero and no work already recorded is lost.\n",
            "`--fresh` is for a DIFFERENT situation, not this one: a run wedged in a terminal \
             state (for example a plan-critique escalation) on a spec that is otherwise \
             UNCHANGED. It is a one-shot new-run boundary, never the default way to continue \
             interrupted work.\n",
            "A run its plan-critique gate stopped on a spec defect (its halt opens `amend the \
             spec and relaunch`) is never adopted by a command naming its spec: amend the spec, \
             critique it with `rigger critique <spec>`, then relaunch on it, which begins a new \
             run with no `--fresh`.\n",
        ],
        "Never hand-drive `rigger step` yourself in a shell to \"help it along\" - the \
         driver owns stepping, and a hand step races it, which can double-spawn a unit or \
         wedge the frontier (see using-rigger). And do not reach for `--fresh` reflexively \
         just because a run looks stuck: on a merely-interrupted run it abandons the \
         adoptable state your relaunch would otherwise have continued from, in exchange for \
         nothing - reserve it for the genuinely wedged-terminal case above.\n",
        "rigger-handle-an-escalation for the run-level and unit-level terminal states \
         `--fresh` genuinely exists for.\n",
    )
}

/// Render the `rigger-handle-an-escalation` skill (spec 68, criterion 2): acting on a
/// unit the loop handed back to a human. Unlike its four siblings, this one DOES
/// interpolate from `ctx`: the remediation bound it names is [`DocsContext::max_retries`],
/// the same code-derived fact `using-rigger` interpolates, so the two can never disagree
/// on what the bound actually is.
fn render_handle_an_escalation_skill(ctx: &DocsContext) -> String {
    render_operation_skill(
        "rigger-handle-an-escalation",
        "Act on a unit the loop handed back - `rigger status` (or the \
         dashboard) names it `escalated (awaiting a human)` after it exhausted its \
         remediation attempts. Read this before touching the unit's branch or relaunching \
         the run.",
        &[],
        &[
            &format!(
                "An escalated unit gave up at the remediation bound (`defaults.max_retries`, {max} \
                 by default) - the loop will not retry it on its own; it is waiting on a human \
                 decision. Read the recorded lesson for the CONCRETE final failure - via `rigger \
                 peers` scoped to the unit's files, or the dashboard - rather than guessing: the \
                 escalation lesson carries the actual failing gate or review reason, not a \
                 placeholder, and that reason is the bounded remedy you are about to apply.\n",
                max = ctx.max_retries
            ),
            "Apply EXACTLY that remedy on the unit's durable branch (`rigger/u/<unit-id>`, the \
             branch rigger itself created and kept for this unit's committed work across every \
             attempt) - nothing more, nothing less. Then relaunch the run fresh - `rigger run \
             --fresh <spec>` (or `rigger serve --fresh <spec>` / the native `/rigger <spec>` \
             workflow with `fresh` set) - against the same, otherwise-unchanged spec: the \
             conductor mints a new run boundary, and the loop picks the escalated unit back up \
             with a clean remediation budget.\n",
        ],
        "Never hand-merge the unit's durable branch onto the run branch yourself - that \
         bypasses review and integration and forks the merged code away from what the event \
         log says happened. And never re-implement more than the remedy names: scope creep \
         here is work the next review has no record of and did not ask for. If the remedy \
         genuinely needs more than a bounded fix, that is a reason to amend the spec (see \
         planning-a-spec), not to freelance on the branch.\n",
        "rigger-resume-a-run for the DIFFERENT case of a merely-interrupted run, where \
         `--fresh` is the wrong move.\n",
    )
}

/// Render the `rigger-watch-a-run` skill (spec 69, criterion 1): the manual-look monitoring
/// protocol for a launched run. The FIVE SIGNAL NAMES and each one's RESPONSE text are
/// interpolated from `ctx.watch_signals`, and the poll interval from
/// `ctx.watch_poll_interval_secs` - both populated by the composition root straight from
/// `crate::watch::Signal` (`name()`/`response()`) and `crate::watch::DEFAULT_INTERVAL_SECS`,
/// the exact values `rigger watch` itself prints on an anomaly line (`src/watch.rs`) - so
/// this skill's own headline claim ("names the five signals each mapped to its response
/// skill") is pinned against the runtime, never hand-copied: a renamed signal or a changed
/// response breaks the render the same moment it would break `rigger watch`'s own output.
/// Reading these facts through `ctx`, like every other drift-prone value in this file,
/// rather than importing `crate::watch` directly keeps this render a pure function of its
/// one injected context - the same DI-provable shape `docs_context_reads_every_fact_from_code`
/// and the sentinel-context tests already hold every other fact in this file to.
fn render_watch_a_run_skill(ctx: &DocsContext) -> String {
    let [escalated, dead_driver, dash_not_serving, reject_recurrence, frontier_stall] =
        &ctx.watch_signals;
    render_operation_skill(
        "rigger-watch-a-run",
        "Monitor a run you just launched, or one that has driven unattended a \
         while, for the five signals a run can be failing on even while every other view \
         still looks healthy. Read this before walking away from a launched run.",
        &[],
        &[
            "On EVERY look, check all FIVE signals below, not just the one you already suspect - \
             a run can read healthy on any single signal while another one is quietly failing, \
             which is why liveness reads healthy in a stalled run (signal 5 exists for exactly \
             that case):\n",
            &format!(
                "1. **{}** - a unit `rigger status` (or the dashboard) marks `escalated (awaiting a \
                 human)`. Respond with `{}`.",
                escalated.name, escalated.response
            ),
            &format!(
                "2. **{}** VS LIVE AGENT PROCESSES - an in-flight agent's last heartbeat is stale but \
                 its worker process is actually gone, not merely slow (the driver quit, crashed, or \
                 the machine slept). Respond with `{}`.",
                dead_driver.name, dead_driver.response
            ),
            &format!(
                "3. **{}** - the dashboard URL does not answer, `rigger watch` reports it not \
                 serving, or a browser just spins. Respond with `{}`.",
                dash_not_serving.name, dash_not_serving.response
            ),
            &format!(
                "4. **{}** - a unit keeps failing the SAME finding rather than converging \
                 (reject-recurrence at or past the diagnose threshold). Respond with `{}`.",
                reject_recurrence.name, reject_recurrence.response
            ),
            &format!(
                "5. **{}** - is the run actually consuming what it spawns? A spawn id surviving \
                 consecutive looks, an hours-old last run event under \"working\" agents, or a \
                 repeating wave is a STALL even though every signal above reads clean - this is why \
                 progress is its own signal, not a restatement of liveness. Respond: {}.\n",
                frontier_stall.name, frontier_stall.response
            ),
            &format!(
                "FIRST instruction, every time: on launch, ARM `rigger watch` under the harness's \
                 background monitor - it polls store and status on its own (default every {}s, \
                 `--interval <s>` to change it) and folds these same five signals, plus a sixth \
                 store-integrity check of its own, into one printed line per anomaly. The manual look \
                 above is the FALLBACK for when nothing is armed, exercised at least once per \
                 remediation cycle even while `rigger watch` is running.\n",
                ctx.watch_poll_interval_secs
            ),
        ],
        "Do not make polling `git log` or `ps` by hand the PRIMARY view - a shell only shows \
         what a shell can see, and misses the signals the store and status already resolve \
         for you (escalation, reject-recurrence, frontier progress). And do not \
         hand-intervene in a run that is merely SLOW, not stuck: a long-running unit with \
         fresh heartbeats and advancing store events is working, not stalled, and \
         hand-driving it only races the loop (see rigger-resume-a-run's own anti-move).\n",
        &format!(
            "{}, {}, {}, and {} - the four response skills this protocol routes to by name; never \
             invent a response beyond them.\n",
            escalated.response,
        dead_driver.response,
        dash_not_serving.response,
        reject_recurrence.response
        ),
    )
}

/// Render the `rigger-restore-the-dash` skill (spec 69, criterion 1): getting the run
/// dashboard serving again. `ctx` is accepted only to match the registry's uniform
/// signature; nothing here is drift-prone enough to interpolate from it.
fn render_restore_the_dash_skill(_ctx: &DocsContext) -> String {
    render_operation_skill(
        "rigger-restore-the-dash",
        "Get the run dashboard serving again when its URL does not answer, \
         `rigger watch` reports it not serving, or a browser just spins. Read this before \
         restarting the dash or touching its marker file by hand.",
        &[],
        &[
            "The dash is a SINGLETON per project: at most one `rigger dash` serves a given \
             project's fixed address at a time. A second `rigger dash` against an address a real \
             rigger dash already answers on reports that address and exits 0 rather than binding \
             a second one - so a not-serving dash is never \"already running somewhere else\", it \
             is genuinely down.\n",
            "`rigger status`'s dashboard line is NOT yet a liveness check - it always prints \
             whatever URL was last recorded, even when nothing answers there, so do not trust \
             that line alone. `rigger watch --once` IS the accurate check: it verifies the \
             recorded marker by actually probing its port, and prints a `dash liveness` line \
             naming the dead PID when nothing answers there - trust that over a bare status \
             line. `rigger run` and `rigger serve` never write that marker at all (only \
             `rigger step` does) - for those two drivers `rigger watch` falls back to probing \
             the recorded dash URL's OWN port directly, and its `dash liveness` line still \
             fires, just without a pid to name.\n",
            "Restart with a plain `rigger dash` (no flags needed for the default address). The \
             singleton bind then does the right thing either way: if the address is genuinely \
             free it binds and serves; if a live rigger dash is already there after all, it \
             reports that address and exits cleanly instead of fighting it.\n",
            "The HUNG-HOLDER case is the one that actually hangs a client instead of failing \
             cleanly: the marker records a port whose process died, froze, or was suspended \
             without releasing it, so a fresh probe against that port neither serves nor cleanly \
             refuses - it just hangs, and so does anything waiting on it. The marker's own PID, \
             not a fresh diagnosis, is what names the culprit: `rigger watch` reads it and \
             prints that exact PID on its dash-liveness line. RESUME that \
             process if it is merely stopped (a suspended terminal, a paused container), or KILL \
             it if it is dead weight - THAT pid, the one the marker and `rigger watch`'s own \
             line actually name - then restart with `rigger dash`. When NO marker was ever \
             recorded (`rigger run` / `rigger serve`), `rigger watch`'s line names no pid at all \
             - skip straight to restarting with `rigger dash`; its singleton bind never fights a \
             genuinely-live dash, and a bind failure against a real non-dash holder is then a \
             manual, outside-`rigger` situation, never one to guess a pid for.\n",
        ],
        "Never hand-edit the dash marker file to \"fix\" it - it is a breadcrumb the step \
         path itself writes and overwrites, and a hand-edited value only makes the next real \
         dash's own self-heal harder to trust. And never kill a process by PORT-ADJACENT \
         GUESSWORK (\"kill whatever's near the dash port\") - resume or kill the EXACT pid \
         the marker and `rigger watch`'s own line name, never a guess, and never one you \
         found some other way when the line names none at all.\n",
        "rigger-watch-a-run names dash liveness as one of its five signals, routing here by \
         name; rigger-diagnose-churn for the DIFFERENT case of a unit that keeps failing \
         review, not a dead dashboard.\n",
    )
}

/// Render the `rigger-diagnose-churn` skill (spec 69, criterion 1): acting on a unit stuck
/// in reject-recurrence. The diagnose threshold is interpolated from
/// `ctx.reject_recurrence_diagnose_threshold`, populated by the composition root straight
/// from `crate::watch::REJECT_RECURRENCE_DIAGNOSE_THRESHOLD` - the same bound `rigger watch`
/// alerts on - so this skill can never quote a stale number, and reads it through the same
/// injected channel every other drift-prone fact in this file uses rather than importing
/// `crate::watch` directly.
fn render_diagnose_churn_skill(ctx: &DocsContext) -> String {
    render_operation_skill(
        "rigger-diagnose-churn",
        "Act on a unit whose blocker line shows `reject-recurrence #n/max \
         (remediating)` past roughly 3 attempts, or whose diffs are oscillating rather than \
         converging. Read this before blaming the model or the panel, or reaching for \
         `max_retries`.",
        &[],
        &[
            &format!(
                "By the time reject-recurrence reaches the diagnose threshold ({threshold}, the same \
                 bound `rigger watch`'s reject-recurrence-trend signal alerts on), do the FINDING \
                 AUDIT before reacting to the raw attempt count: read every blocking finding against \
                 the diffs it cites - each finding names a checkable fact, and the audit is comparing \
                 that fact against what the diff actually does, not trusting the finding's prose. A \
                 high attempt count on its own proves nothing about what actually went wrong.\n",
                threshold = ctx.reject_recurrence_diagnose_threshold
            ),
            "SEPARATE infra-caused attempts before judging the rest: a finding about a deleted \
             worktree, a thrashed shared build cache, or a quota-killed agent is an INFRA \
             failure, not a semantic one - it inflates the attempt count without saying anything \
             about whether the unit's actual approach is wrong. Fix infra in the binary via its \
             own spec; never let it count toward, or be blamed as, review strictness.\n",
            "Once the infra noise is set aside, look at what remains: if the SURVIVING, \
             factually-correct findings keep citing the SAME constraint against different, \
             otherwise-reasonable diffs, the spec itself is self-contradictory - no \
             implementation can satisfy a contradiction, so every attempt is individually correct \
             to reject and the run will churn forever without a spec change. Fix it with the \
             amendment protocol (`planning-a-spec`: amend Design and Global constraints only, \
             commit when no step is mid-flight, then `rigger emit DecisionMade` naming the spec \
             file so in-flight reviewers see it through the graph immediately).\n",
            "For any OTHER recurring pattern, match the SIGNATURE you found against \
             `planning-a-spec`'s own \"Quick reference: churn signature -> planning defect\" \
             table - it maps what a rejection loop looks like (twinned units, a bundled \
             checkbox, an unresolved either/or, findings blaming process-local state, a \
             paraphrased criterion) to the specific catalog class and its fix at spec time, so \
             the diagnosis names the actual defect class rather than just \"it keeps failing\".\n",
        ],
        "Never blame the model or the panel without having run the finding audit first - a \
         reviewer that is factually correct every single round is not the problem, even when \
         it rejects the same unit five times in a row. And do not reflexively raise \
         `defaults.max_retries` to buy another attempt: a bigger budget spent against the \
         SAME unaudited failure reproduces exactly the failure the audit exists to catch, \
         just more expensively.\n",
        "planning-a-spec owns the churn-signature table and the spec-amendment protocol this \
         procedure applies; rigger-watch-a-run names reject-recurrence as one of its five \
         signals, routing here by name; rigger-handle-an-escalation for when a unit exhausts \
         its remediation budget rather than merely churning.\n",
    )
}

/// The line stamped onto EVERY registry skill's rendered content (spec 68, Design): an
/// agent must never install, replace, or modify the operator's own installed `rigger`
/// binary. [`SkillEntry::render`] appends this ONCE, structurally, for every entry - it is
/// never authored into an individual skill's own body, so no entry (present or future) can
/// ship without it by construction.
pub const OPERATOR_BINARY_PROHIBITION: &str = "\n## Operator binary boundary\n\nAn agent \
     never installs, replaces, or modifies the operator's installed `rigger` binary - that \
     binary is operator-only. A tree checkout's own `rigger` build is invoked only by \
     explicit path, and only to render (spec/docs output) - never to overwrite what is on \
     PATH.\n";

/// One skill this binary owns end-to-end (spec 68, criterion 1: the skill registry): a
/// name and its [`DocBody`] (before the operator-binary prohibition is stamped on), rendered
/// from the code-derived [`DocsContext`]. An entry whose content carries no drift-prone facts
/// (`planning-a-spec`, `spec-preflight`) is a static body that ignores `ctx`.
///
/// [`skill_registry`] is the ONE enumeration every surface walks - `rigger docs` (renders
/// each entry to its committed source), `rigger setup` (installs each entry, overlay
/// honored), and the docs-drift gate (drift-checks each entry) - so adding an entry here is
/// the ONLY step required to make a skill render, install, and drift-check; no surface
/// needs its own edit.
pub struct SkillEntry {
    pub name: &'static str,
    body: DocBody,
}

impl SkillEntry {
    /// This entry's FULL rendered content: its body plus the
    /// [`OPERATOR_BINARY_PROHIBITION`], stamped here - once, for every entry - rather than
    /// by each skill's own author.
    pub fn render(&self, ctx: &DocsContext) -> String {
        let mut s = self.body.render(ctx);
        s.push_str(OPERATOR_BINARY_PROHIBITION);
        s
    }
}

/// The skill registry (spec 68, criterion 1): `using-rigger` (the driving discipline),
/// `planning-a-spec` (the authoring discipline), the five-member per-operation family
/// (spec 68, criterion 2), the three watch-discipline skills (spec 69, criterion 1) and
/// `spec-preflight` (spec 112, criterion 4) - one skill per operation, joining this same list
/// by appending entries, never by adding a second, independently-walked enumeration.
pub fn skill_registry() -> Vec<SkillEntry> {
    vec![
        SkillEntry {
            name: "using-rigger",
            body: DocBody::Rendered(render_using_rigger_skill),
        },
        SkillEntry {
            name: "planning-a-spec",
            body: DocBody::Static(PLANNING_A_SPEC_BODY),
        },
        SkillEntry {
            name: "rigger-reset-store",
            body: DocBody::Rendered(render_reset_store_skill),
        },
        SkillEntry {
            name: "rigger-build-graph",
            body: DocBody::Rendered(render_build_graph_skill),
        },
        SkillEntry {
            name: "rigger-reindex",
            body: DocBody::Rendered(render_reindex_skill),
        },
        SkillEntry {
            name: "rigger-resume-a-run",
            body: DocBody::Rendered(render_resume_a_run_skill),
        },
        SkillEntry {
            name: "rigger-handle-an-escalation",
            body: DocBody::Rendered(render_handle_an_escalation_skill),
        },
        SkillEntry {
            name: "rigger-watch-a-run",
            body: DocBody::Rendered(render_watch_a_run_skill),
        },
        SkillEntry {
            name: "rigger-restore-the-dash",
            body: DocBody::Rendered(render_restore_the_dash_skill),
        },
        SkillEntry {
            name: "rigger-diagnose-churn",
            body: DocBody::Rendered(render_diagnose_churn_skill),
        },
        SkillEntry {
            name: "spec-preflight",
            body: DocBody::Static(SPEC_PREFLIGHT_BODY),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic context with sentinel values so a rendered output that reflects them
    /// proves the render is PARAMETERIZED by the context, not hardcoded.
    fn sentinel_ctx() -> DocsContext {
        DocsContext {
            base_ref: "SENTINEL/base-ref".to_string(),
            dash_port: 65531,
            max_retries: 999,
            verdict_approve: "sentinelverdict".to_string(),
            spec_shape_rules: vec!["sentinel-rule-a".to_string(), "sentinel-rule-b".to_string()],
            spec_shape_recommendation: "sentinel recommendation text".to_string(),
            subcommands: vec!["sentinelcmd-a".to_string(), "sentinelcmd-b".to_string()],
            specs_location: "sentinel-specs/".to_string(),
            watch_signals: [
                WatchSignalFact {
                    name: "sentinel-signal-escalated".to_string(),
                    response: "sentinel-response-escalated".to_string(),
                },
                WatchSignalFact {
                    name: "sentinel-signal-dead-driver".to_string(),
                    response: "sentinel-response-dead-driver".to_string(),
                },
                WatchSignalFact {
                    name: "sentinel-signal-dash-not-serving".to_string(),
                    response: "sentinel-response-dash-not-serving".to_string(),
                },
                WatchSignalFact {
                    name: "sentinel-signal-reject-recurrence".to_string(),
                    response: "sentinel-response-reject-recurrence".to_string(),
                },
                WatchSignalFact {
                    name: "sentinel-signal-frontier-stall".to_string(),
                    response: "sentinel-response-frontier-stall".to_string(),
                },
            ],
            watch_poll_interval_secs: 424_242,
            reject_recurrence_diagnose_threshold: 909_090,
            grep_guard_message: "sentinel grep-guard message".to_string(),
        }
    }

    /// A [`DocsContext`] carrying `sentinel_ctx`'s values for every unrelated field, but the
    /// REAL `crate::watch` facts for the three watch fields - so a test built on it can prove
    /// `render_watch_a_run_skill`/`render_diagnose_churn_skill` show the runtime's actual
    /// signal names/responses/thresholds, the accuracy-pin `sentinel_ctx` alone cannot make
    /// (its watch fields are deliberately fake).
    fn real_watch_facts_ctx() -> DocsContext {
        DocsContext {
            watch_signals: [
                crate::watch::Signal::Escalated,
                crate::watch::Signal::DeadDriver,
                crate::watch::Signal::DashNotServing,
                crate::watch::Signal::RejectRecurrence,
                crate::watch::Signal::FrontierStall,
            ]
            .map(|signal| WatchSignalFact {
                name: signal.name().to_string(),
                response: signal.response().to_string(),
            }),
            watch_poll_interval_secs: crate::watch::DEFAULT_INTERVAL_SECS,
            reject_recurrence_diagnose_threshold:
                crate::watch::REJECT_RECURRENCE_DIAGNOSE_THRESHOLD,
            ..sentinel_ctx()
        }
    }

    /// Every sentinel fact both renders carry: `(sentinel value, the fact it stands for)`.
    const SHARED_SENTINEL_FACTS: &[(&str, &str)] = &[
        ("SENTINEL/base-ref", "base_ref"),
        ("65531", "dash_port"),
        ("999", "max_retries"),
        ("sentinelverdict", "verdict_approve"),
        ("sentinel-rule-a", "spec_shape_rule"),
        ("sentinelcmd-a", "subcommand"),
        ("sentinel grep-guard message", "grep_guard_message"),
    ];

    /// `render` over the sentinel context carries every shared sentinel fact plus `extra`.
    fn assert_renders_every_fact(render: fn(&DocsContext) -> String, extra: &[(&str, &str)]) {
        let out = render(&sentinel_ctx());
        for (sentinel, fact) in SHARED_SENTINEL_FACTS.iter().chain(extra) {
            assert!(out.contains(sentinel), "{fact} not rendered");
        }
    }

    crate::test_cases! {
        skill_render_is_parameterized_by_every_fact: assert_renders_every_fact(
            render_using_rigger_skill,
            &[("sentinel recommendation text", "spec_shape_recommendation")],
        );
        handbook_render_is_parameterized_by_every_fact:
            assert_renders_every_fact(render_handbook_discipline, &[]);
    }

    #[test]
    fn both_outputs_render_from_the_one_context() {
        // A fact set only on the context appears in BOTH outputs, proving one context
        // feeds both renders (no second, drifting source).
        let ctx = sentinel_ctx();
        assert!(render_using_rigger_skill(&ctx).contains("SENTINEL/base-ref"));
        assert!(render_handbook_discipline(&ctx).contains("SENTINEL/base-ref"));
    }

    /// Spec 92, criterion 4 (IN EVERY SESSION'S HAND): "the shipped skill's lookup section
    /// states the same rule for a human reader" - the "Looking things up" section (shared by
    /// both outputs) names the operator's own MCP tools, `.mcp.json`, and the PreToolUse hook,
    /// not just the loop-agent CLI verbs the section already covered.
    #[test]
    fn looking_things_up_section_states_the_operator_session_rule() {
        let ctx = sentinel_ctx();
        for out in [
            render_using_rigger_skill(&ctx),
            render_handbook_discipline(&ctx),
        ] {
            assert!(
                out.contains("## Looking things up"),
                "the lookup section must exist"
            );
            assert!(
                out.contains("rigger_peers")
                    && out.contains("rigger_ground")
                    && out.contains("rigger_graph"),
                "the operator's own MCP tool names must be stated; got:\n{out}"
            );
            assert!(
                out.contains(".mcp.json"),
                "the file the operator's MCP server is registered into must be named; got:\n{out}"
            );
            assert!(
                out.contains("PreToolUse"),
                "the hook event the lookup hook installs under must be named; got:\n{out}"
            );
            assert!(
                out.contains("sentinel grep-guard message"),
                "the hook's real bounce message must be interpolated, not hand-copied; \
                 got:\n{out}"
            );
            // d-spec92-hook-no-target-axis: the hook has no target axis, so the section
            // must state a bounce-everywhere rule, never a list of guarded trees.
            assert!(
                out.contains("every")
                    && out.contains("Grep")
                    && out.contains("grep")
                    && out.contains("--literal"),
                "the section must state the hook bounces EVERY Grep call and every \
                 grep-invoking Bash command unless --literal is carried; got:\n{out}"
            );
        }
    }

    #[test]
    fn skill_carries_claude_code_skill_frontmatter() {
        let out = render_using_rigger_skill(&sentinel_ctx());
        assert!(
            out.starts_with("---\nname: using-rigger\n"),
            "the skill must open with skill frontmatter naming it; got: {}",
            &out[..out.len().min(80)]
        );
        assert!(
            out.contains("\ndescription: "),
            "frontmatter needs a description"
        );
    }

    #[test]
    fn render_is_byte_stable_across_runs() {
        let ctx = sentinel_ctx();
        assert_eq!(
            render_using_rigger_skill(&ctx),
            render_using_rigger_skill(&ctx)
        );
        assert_eq!(
            render_handbook_discipline(&ctx),
            render_handbook_discipline(&ctx)
        );
    }

    /// Spec 46, criterion 2 (the shipped operator guidance): the shared discipline body
    /// carries a graph-hygiene section that names `rigger reset --runs` as the PRE-RUN
    /// hygiene step and explains WHY truthfully - graph.db is a PERSISTENT incremental
    /// projection (a step never re-folds the whole history), so across runs it accumulates
    /// dead-run rows and retired edges no live query reads, which `rigger reset --runs`
    /// prunes to reclaim the disk they held. Because BOTH outputs render from
    /// `discipline_body`, the skill and the handbook chapter cannot disagree, so the
    /// guidance ships identically through the skill and the handbook.
    #[test]
    fn discipline_carries_graph_hygiene_pre_run_reset() {
        let ctx = sentinel_ctx();
        for (label, out) in [
            ("skill", render_using_rigger_skill(&ctx)),
            ("handbook", render_handbook_discipline(&ctx)),
        ] {
            assert!(
                out.contains("## Graph hygiene"),
                "{label} must carry the graph-hygiene section"
            );
            assert!(
                out.contains("rigger reset --runs"),
                "{label} must name `rigger reset --runs` as the pre-run hygiene step"
            );
            assert!(
                out.contains("persistent projection"),
                "{label} must frame graph.db as a persistent incremental projection"
            );
            assert!(
                out.contains("reclaims the disk"),
                "{label} must explain reset --runs reclaims the disk dead-run rows held"
            );
            // NEGATIVE regression guard (spec 46 c2). The DISCREDITED fold-speed framing
            // that rejected this unit's first attempt (graph.db re-folded whole-history each
            // step, the fold slow in proportion to graph size, a prune speeding it up) must
            // never re-enter the shipped render: graph.db is a PERSISTENT incremental
            // projection and a prune reclaims DISK, it does not speed any fold. Pin those
            // phrases OUT (case-insensitively) so a future edit resurrecting the false
            // mechanism fails LOUDLY here instead of shipping silently.
            let lower = out.to_lowercase();
            for banned in [
                "re-folded each step",
                "fold stays slow",
                "proportional to graph size",
                "faster fold",
            ] {
                assert!(
                    !lower.contains(banned),
                    "{label} must NOT resurrect the discredited fold-speed framing \
                     (found {banned:?}); a prune reclaims disk, it does not speed a fold"
                );
            }
        }
    }

    /// Spec 60, criterion 5 (the shipped operator guidance for SUPPORTED COMPACTION): the same
    /// discipline body names `rigger reset --derived` as the EVENT LOG's own prune, so `--runs`
    /// is no longer rendered as THE prune command while a second one exists.
    ///
    /// It pins the four things an operator must know before running a command that deletes from
    /// an append-only log: WHAT IT KEEPS (each file's latest generation, at the latest event per
    /// replay key), WHAT IT COSTS (nothing else - every other event survives byte-for-byte, so lessons,
    /// decisions, findings and the run history `stats` and replay read are untouched), that the
    /// FILE actually shrinks, and that the two flags COMPOSE rather than one superseding the
    /// other. Both shipped outputs render from `discipline_body`, so the skill and the handbook
    /// chapter cannot disagree and a drift here would drift for every consumer at once. The
    /// `rigger-reset-store` skill renders its own `--derived` bullet, so it is held to the same
    /// keep rule here.
    #[test]
    fn discipline_names_reset_derived_as_the_event_logs_own_prune() {
        let ctx = sentinel_ctx();
        for (label, out) in [
            ("skill", render_using_rigger_skill(&ctx)),
            ("handbook", render_handbook_discipline(&ctx)),
        ] {
            assert!(
                out.contains("rigger reset --derived"),
                "{label} must name `rigger reset --derived` as the event log's own prune"
            );
            assert!(
                out.contains("EVENT LOG"),
                "{label} must say WHICH store the derived prune compacts - the event log, not \
                 the graph"
            );
            assert!(
                out.contains("only the recordings of its LATEST generation"),
                "{label} must state what the derived prune KEEPS, so an operator can predict it"
            );
            assert!(
                out.contains("LATEST event per replay key"),
                "{label} must state which recording of each replay key the derived prune KEEPS \
                 within that generation, so an operator can predict the exact-key dedup"
            );
            assert!(
                out.contains("byte-for-byte"),
                "{label} must state that every other event survives the derived prune untouched"
            );
            assert!(
                out.contains("shrinks on disk"),
                "{label} must state that the derived prune shrinks events.db on disk"
            );
            assert!(
                out.contains("rigger reset --runs --derived"),
                "{label} must show the two prunes COMPOSING, each shedding its own accumulation"
            );

            // WHEN A DEDUPLICATED LOG STILL HAS SOMETHING TO SHED. "A log written since the dedup
            // prunes to zero" is FALSE as a universal: a file whose content returns to a
            // generation the log already recorded re-records its whole batch by design (an
            // ever-recorded key test would strand the graph on the version the file moved past),
            // so a revert, a branch switch or a checkout back leaves duplication a modern log
            // sheds. That sentence is the one an operator uses to decide whether a non-zero prune
            // means the dedup is broken, so the exception ships with the rule.
            assert!(
                out.contains("RETURNED to a generation the log had already recorded"),
                "{label} must state the ONE case in which a log written since the dedup still \
                 prunes rows, or a correct non-zero prune reads as a broken dedup"
            );
            assert!(
                out.contains("revert"),
                "{label} must give that case its ordinary name, so an operator recognizes it"
            );

            // WHAT IT COSTS TO RUN, which is not on the partition the operator is watching: the
            // rewrite stages a complete copy of the log in the temporary directory, and it only
            // runs when the FILE has free space to reclaim.
            assert!(
                out.contains("temporary directory"),
                "{label} must say where the compaction stages its copy of the log, since the free \
                 space it needs is not on the partition holding the log"
            );
            // AND WHICH DIRECTORY THAT IS, resolved the way SQLite resolves it. This sentence
            // exists for exactly one job - telling an operator WHICH filesystem must hold a full
            // copy of their log - so naming the wrong one is worse than saying nothing. SQLite's
            // unix resolution is SQLITE_TMPDIR, then TMPDIR, then the first of /var/tmp, /usr/tmp
            // and /tmp that it can use, so with TMPDIR unset the answer is /var/tmp and /tmp is
            // never consulted.
            for needle in ["SQLITE_TMPDIR", "TMPDIR", "/var/tmp"] {
                assert!(
                    out.contains(needle),
                    "{label} must name how the temporary directory RESOLVES ({needle:?}): an \
                     operator reads this to decide which filesystem needs the free space, and a \
                     guess at the default sends them to the wrong one"
                );
            }
            assert!(
                out.contains("leaves the file exactly as it found it"),
                "{label} must say that a prune with nothing to shed does NOT rewrite the file, or \
                 the expected case reads as costing a full compaction"
            );
            // AND WHAT A RE-RUN AFTER A FAILED REWRITE ACTUALLY DOES. The command tells an
            // operator that re-running is safe; the document has to tell them it is also the way
            // to get the space back, which is only true because the rewrite is triggered by the
            // free space in the file rather than by the rows that run deleted.
            assert!(
                out.contains("reclaimable free pages"),
                "{label} must say what triggers the rewrite - the free pages in the file, not \
                 this run's deletes - or a re-run after a failed reclamation reads as pointless"
            );
        }
        assert!(
            render_reset_store_skill(&ctx)
                .contains("keeps only each file's latest generation of the derived index"),
            "the reset-store skill must state what the derived prune KEEPS, so an operator can \
             predict it"
        );
        assert!(
            render_reset_store_skill(&ctx).contains("at the latest event per replay key"),
            "the reset-store skill must state which recording of each replay key the derived \
             prune KEEPS within that generation, so an operator can predict the exact-key dedup"
        );
    }

    /// Spec 58, criterion 3 (the habit half): the shared discipline body carries the same
    /// three-verb lookup guidance the grounding pointer does - `rigger graph --around` (structure),
    /// `rigger graph --show` (text), `rigger peers` (memory) - states the rule plainly (the graph
    /// is the lookup surface; grep on the project's sources is a fallback worth reporting, not a
    /// habit), and names the fallback-reporting instruction (`grep-fallback:` via `rigger
    /// progress`). Because BOTH outputs render from `discipline_body`, the skill and the handbook
    /// chapter carry it identically.
    #[test]
    fn discipline_carries_three_verb_lookup_guidance() {
        let ctx = sentinel_ctx();
        for (label, out) in [
            ("skill", render_using_rigger_skill(&ctx)),
            ("handbook", render_handbook_discipline(&ctx)),
        ] {
            assert!(
                out.contains("rigger graph --around"),
                "{label} must name the STRUCTURE verb `rigger graph --around`"
            );
            assert!(
                out.contains("rigger graph --show"),
                "{label} must name the TEXT verb `rigger graph --show`"
            );
            assert!(
                out.contains("rigger peers"),
                "{label} must name the MEMORY verb `rigger peers`"
            );
            assert!(
                out.contains("structure") && out.contains("text") && out.contains("memory"),
                "{label} must name each lookup verb's job (structure/text/memory)"
            );
            assert!(
                out.contains("grep-fallback:") && out.contains("rigger progress"),
                "{label} must carry the grep-fallback reporting instruction"
            );
        }
    }

    #[test]
    fn render_carries_no_unicode_dashes() {
        // The drift check has no false positives only if the render is pure ASCII dashes
        // (the diff gate fails on U+2014 and the other unicode dashes).
        let ctx = sentinel_ctx();
        let mut outs = vec![
            render_using_rigger_skill(&ctx),
            render_handbook_discipline(&ctx),
        ];
        for entry in skill_registry() {
            outs.push(entry.render(&ctx));
        }
        for out in outs {
            for bad in ['\u{2012}', '\u{2013}', '\u{2014}', '\u{2015}', '\u{2212}'] {
                assert!(
                    !out.contains(bad),
                    "render must not contain unicode dash {bad:?}"
                );
            }
        }
    }

    /// Spec 68, criterion 1: the registry is the ONE enumeration - it names
    /// `using-rigger` and `planning-a-spec` (the two skills that exist today), each name
    /// non-blank and present exactly once.
    #[test]
    fn skill_registry_names_using_rigger_and_planning_a_spec_exactly_once_each() {
        let names: Vec<&str> = skill_registry().iter().map(|e| e.name).collect();
        for expected in ["using-rigger", "planning-a-spec"] {
            assert_eq!(
                names.iter().filter(|n| **n == expected).count(),
                1,
                "{expected:?} must appear exactly once in the registry; got {names:?}"
            );
        }
        for name in &names {
            assert!(
                !name.is_empty(),
                "a registry entry must not have a blank name"
            );
        }
    }

    /// Spec 68, criterion 1 (the prohibition is STRUCTURAL, not hand-authored per skill):
    /// every registry entry's `render()` carries the operator-binary prohibition exactly
    /// once, while the entry's own BODY function (called directly, bypassing the
    /// registry's stamp) does NOT - proving the line is appended by [`SkillEntry::render`]
    /// itself rather than baked into any individual skill's authored content.
    #[test]
    fn skill_entry_render_stamps_the_operator_binary_prohibition_structurally() {
        let ctx = sentinel_ctx();
        for entry in skill_registry() {
            let rendered = entry.render(&ctx);
            assert_eq!(
                rendered.matches(OPERATOR_BINARY_PROHIBITION).count(),
                1,
                "{}: render() must carry the prohibition exactly once",
                entry.name
            );
            let raw_body = entry.body.render(&ctx);
            assert!(
                !raw_body.contains(OPERATOR_BINARY_PROHIBITION),
                "{}: the skill's own body must NOT author the prohibition itself",
                entry.name
            );
        }
    }

    /// The `planning-a-spec` render carries its loadable frontmatter and the recipe's
    /// seven numbered steps, so the authoring discipline actually ships through the
    /// registry (not just a placeholder).
    #[test]
    fn planning_a_spec_render_carries_frontmatter_and_the_recipe() {
        let out = PLANNING_A_SPEC_BODY;
        assert!(
            out.starts_with("---\nname: planning-a-spec\n"),
            "must open with skill frontmatter naming it; got: {}",
            &out[..out.len().min(80)]
        );
        assert!(
            out.contains("\ndescription: "),
            "frontmatter needs a description"
        );
        for step in [
            "**1. Ground the Goal in evidence.**",
            "**7. Preflight, then launch.**",
        ] {
            assert!(out.contains(step), "recipe must carry {step:?}");
        }
    }

    /// Spec 68, criterion 2 (extended by spec 69, criterion 1 with the three watch-discipline
    /// skills, and by spec 112, criterion 4 with `spec-preflight`): the five-member
    /// per-operation family, the three watch skills AND the preflight skill are IN the
    /// registry, each name present exactly once, alongside (not instead of) `using-rigger`
    /// and `planning-a-spec`.
    #[test]
    fn registry_names_all_five_per_operation_skills_exactly_once_each() {
        let names: Vec<&str> = skill_registry().iter().map(|e| e.name).collect();
        for expected in [
            "using-rigger",
            "planning-a-spec",
            "rigger-reset-store",
            "rigger-build-graph",
            "rigger-reindex",
            "rigger-resume-a-run",
            "rigger-handle-an-escalation",
            "rigger-watch-a-run",
            "rigger-restore-the-dash",
            "rigger-diagnose-churn",
            "spec-preflight",
        ] {
            assert_eq!(
                names.iter().filter(|n| **n == expected).count(),
                1,
                "{expected:?} must appear exactly once in the registry; got {names:?}"
            );
        }
        assert_eq!(
            names.len(),
            11,
            "the registry must have exactly 11 entries; got {names:?}"
        );
    }

    /// Spec 68, criterion 2: every per-operation skill's frontmatter carries the
    /// operation's own symptom-bearing "tells" from the spec Design table, so an agent
    /// routes to the right skill from the description alone (this file IS the routing
    /// layer - see the registry doc comment).
    #[test]
    fn per_operation_descriptions_carry_their_symptoms() {
        let ctx = sentinel_ctx();
        let cases: &[(&str, &[&str])] = &[
            (
                "rigger-reset-store",
                &["disk usage", "bloat advisory", "rigger validate"],
            ),
            ("rigger-build-graph", &["empty", "first setup"]),
            (
                "rigger-reindex",
                &["no longer holds", "index-staleness advisory"],
            ),
            (
                "rigger-resume-a-run",
                &["dead driver", "in flight", "stale heartbeat"],
            ),
            (
                "rigger-handle-an-escalation",
                &["escalated (awaiting a human)"],
            ),
        ];
        let registry = skill_registry();
        for (name, tells) in cases {
            let entry = registry
                .iter()
                .find(|e| e.name == *name)
                .unwrap_or_else(|| panic!("{name} must be in the registry"));
            let out = entry.body.render(&ctx);
            let frontmatter_end = out.find("\n---\n\n").map(|i| i + 6).unwrap_or(out.len());
            let frontmatter = &out[..frontmatter_end];
            assert!(
                frontmatter.starts_with(&format!("---\nname: {name}\n")),
                "{name}: frontmatter must open naming itself; got: {}",
                &frontmatter[..frontmatter.len().min(80)]
            );
            for tell in *tells {
                assert!(
                    frontmatter.contains(tell),
                    "{name}: description must carry the symptom {tell:?}; got: {frontmatter}"
                );
            }
        }
    }

    /// Spec 68, criterion 2 (the escalation "tell" is genuinely pinned, not hand-copied):
    /// the exact phrase `rigger-handle-an-escalation` names in its description is the
    /// SAME string [`crate::blocker::Blocker::line`] renders for
    /// [`crate::blocker::Kind::Escalated`] - the literal text `rigger status`/the
    /// dashboard show an operator. A rename of that blocker line would break this test,
    /// not just silently stop matching what an operator actually sees.
    #[test]
    fn escalation_skill_names_the_real_blocker_line() {
        let blocker = crate::blocker::Blocker {
            subject: "some-unit".to_string(),
            kind: crate::blocker::Kind::Escalated,
        };
        let real_line = blocker.line();
        let out = render_handle_an_escalation_skill(&sentinel_ctx());
        assert!(
            out.contains(&real_line),
            "the skill must name the REAL blocker line {real_line:?} an operator actually \
             sees, not a hand-copied approximation"
        );
    }

    /// Spec 68, criterion 2: every per-operation skill's body carries exactly one
    /// "## Procedure" section and exactly one "## Anti-move" section (one operation, one
    /// named anti-move - never a second procedure bundled in, never a missing anti-move).
    #[test]
    fn per_operation_skills_carry_one_procedure_and_one_named_anti_move() {
        let ctx = sentinel_ctx();
        for name in [
            "rigger-reset-store",
            "rigger-build-graph",
            "rigger-reindex",
            "rigger-resume-a-run",
            "rigger-handle-an-escalation",
        ] {
            let registry = skill_registry();
            let entry = registry.iter().find(|e| e.name == name).unwrap();
            let out = entry.body.render(&ctx);
            assert_eq!(
                out.matches("## Procedure").count(),
                1,
                "{name}: must carry exactly one Procedure section"
            );
            assert_eq!(
                out.matches("## Anti-move").count(),
                1,
                "{name}: must carry exactly one named Anti-move section"
            );
            // The anti-move must actually follow the procedure (one operation described,
            // then the move that would defeat it) - not precede it.
            assert!(
                out.find("## Procedure").unwrap() < out.find("## Anti-move").unwrap(),
                "{name}: Procedure must come before Anti-move"
            );
        }
    }

    /// Spec 68, criterion 2 (the neighbor-linking half): every per-operation skill's body
    /// names at least one OTHER family member by name, so the family cross-links rather
    /// than each entry standing in isolation (spec Design: "cross-linking neighbors by
    /// name").
    #[test]
    fn per_operation_skills_cross_link_a_sibling_by_name() {
        let ctx = sentinel_ctx();
        let family = [
            "rigger-reset-store",
            "rigger-build-graph",
            "rigger-reindex",
            "rigger-resume-a-run",
            "rigger-handle-an-escalation",
        ];
        let registry = skill_registry();
        for name in family {
            let entry = registry.iter().find(|e| e.name == name).unwrap();
            let out = entry.body.render(&ctx);
            let mentions_a_sibling = family
                .iter()
                .filter(|other| **other != name)
                .any(|other| out.contains(other));
            assert!(
                mentions_a_sibling,
                "{name}: must cross-link at least one sibling skill by name"
            );
        }
    }

    /// Spec 112 (*Relaunch*): the skills route a run stopped on a spec defect through critique,
    /// then relaunch - `rigger-resume-a-run` says such a run is never adopted by a command naming
    /// its spec and keeps `--fresh` for a wedged run on an unchanged spec, and the preflight's
    /// amend step names the critique before the relaunch.
    #[test]
    fn the_skills_route_a_run_stopped_on_a_spec_defect_through_critique_then_relaunch() {
        let ctx = sentinel_ctx();
        let registry = skill_registry();
        let body = |name: &str| {
            let entry = registry.iter().find(|e| e.name == name).unwrap();
            crate::wave::normalize_ws(&entry.body.render(&ctx))
        };
        for (name, wanted) in [
            (
                "rigger-resume-a-run",
                "A run its plan-critique gate stopped on a spec defect (its halt opens `amend the \
                 spec and relaunch`) is never adopted by a command naming its spec: amend the \
                 spec, critique it with `rigger critique <spec>`, then relaunch on it, which \
                 begins a new run with no `--fresh`.",
            ),
            (
                "rigger-resume-a-run",
                "a run wedged in a terminal state (for example a plan-critique escalation) on a \
                 spec that is otherwise UNCHANGED",
            ),
            (
                "spec-preflight",
                "1. Amend Design and Global constraints only; a criterion edit orphans the live \
                 run, and a plan-critique stop is closed by critiquing the amended spec (`rigger \
                 critique <spec>`), then relaunching on it, which begins a new run.",
            ),
        ] {
            let wanted = crate::wave::normalize_ws(wanted);
            assert!(body(name).contains(&wanted), "{name} must say {wanted:?}");
        }
    }

    /// Spec 68, criterion 2 (the scope boundary): "no registry skill's body exceeds one
    /// operation's scope" - each per-operation skill's own primary command anchor appears
    /// ONLY in its own render, never reproduced as another skill's procedure. A neighbor
    /// may be named (the cross-link test above), but never re-documented as if it were
    /// this skill's own operation.
    #[test]
    fn per_operation_skills_stay_within_their_own_operations_scope() {
        let ctx = sentinel_ctx();
        let anchors: &[(&str, &str)] = &[
            ("rigger-reset-store", "rigger reset --derived"),
            ("rigger-build-graph", "rigger graph build"),
            ("rigger-reindex", "rigger reindex <file>"),
            ("rigger-resume-a-run", "adopts the existing run"),
            ("rigger-handle-an-escalation", "the unit's durable branch"),
        ];
        let registry = skill_registry();
        let rendered: Vec<(&str, String)> = anchors
            .iter()
            .map(|(name, _)| {
                let entry = registry.iter().find(|e| e.name == *name).unwrap();
                (*name, entry.body.render(&ctx))
            })
            .collect();
        for (name, out) in &rendered {
            let own_anchor = anchors.iter().find(|(n, _)| n == name).unwrap().1;
            assert!(
                out.contains(own_anchor),
                "{name}: must carry its own operation's anchor {own_anchor:?}"
            );
            for (other_name, other_anchor) in anchors {
                if other_name == name {
                    continue;
                }
                assert!(
                    !out.contains(other_anchor),
                    "{name}: must NOT reproduce {other_name}'s own anchor {other_anchor:?} - \
                     that would exceed this skill's one-operation scope"
                );
            }
        }
    }

    /// Spec 68, criterion 2: the escalation skill's remediation-bound sentence is
    /// PARAMETERIZED by `ctx.max_retries` (the same code-derived fact `using-rigger`
    /// interpolates), not a hand-copied number - proven with a sentinel value distinct
    /// from the real `MAX_RETRIES` default.
    #[test]
    fn escalation_skill_is_parameterized_by_max_retries() {
        let ctx = sentinel_ctx();
        let out = render_handle_an_escalation_skill(&ctx);
        assert!(
            out.contains(&ctx.max_retries.to_string()),
            "the escalation skill must interpolate ctx.max_retries, not hard-code a bound"
        );
    }

    /// Spec 69, criterion 1 (WATCH SKILLS RENDER TRUE, the headline claim): `rigger-watch-a-run`
    /// names the five signals `crate::watch::Signal` actually defines, each mapped BY THE SAME
    /// STRING to its real response - pinned against the runtime (`Signal::name()`/`response()`
    /// and `watch::SKILL_SIGNAL_NAMES`), the exact authority `src/watch.rs`'s own module doc
    /// names both the command and this skill as pinned against, so the two can never silently
    /// drift apart on what a signal is called or where it routes. Uses `real_watch_facts_ctx`
    /// (not `sentinel_ctx`) because the render now reads these facts from `ctx` - the
    /// composition root (`docs_context_reads_every_fact_from_code`, in the binary) is what
    /// proves `ctx` itself carries the real values; this test proves the render is faithful
    /// to whatever `ctx` says.
    #[test]
    fn watch_a_run_names_all_five_signals_each_mapped_to_its_response() {
        let out = render_watch_a_run_skill(&real_watch_facts_ctx());
        for signal in [
            crate::watch::Signal::Escalated,
            crate::watch::Signal::DeadDriver,
            crate::watch::Signal::DashNotServing,
            crate::watch::Signal::RejectRecurrence,
            crate::watch::Signal::FrontierStall,
        ] {
            assert!(
                out.contains(signal.name()),
                "must name signal {:?}",
                signal.name()
            );
            assert!(
                out.contains(signal.response()),
                "signal {:?} must map to its real response {:?}",
                signal.name(),
                signal.response()
            );
        }
        for name in crate::watch::SKILL_SIGNAL_NAMES {
            assert!(
                out.contains(name),
                "must carry the exact skill signal name {name:?} `rigger watch` pins against"
            );
        }
        assert!(
            out.contains(&crate::watch::DEFAULT_INTERVAL_SECS.to_string()),
            "the poll interval must be pinned against watch::DEFAULT_INTERVAL_SECS"
        );
        assert!(
            out.contains("ARM") && out.contains("rigger watch"),
            "the FIRST instruction must tell the reader to arm `rigger watch`"
        );
    }

    /// Spec 69, criterion 1 (the DI-provable property every other fact in this file already
    /// has): `render_watch_a_run_skill` and `render_diagnose_churn_skill` are pure functions
    /// of `ctx` for the watch facts - SENTINEL signal names/responses/threshold/interval
    /// (values `crate::watch` never produces) reach the rendered output, which a hardcoded
    /// `crate::watch::Signal::X.name()` call in the render body could never do. Mirrors
    /// `escalation_skill_is_parameterized_by_max_retries`'s shape for this file's newest facts.
    #[test]
    fn watch_and_diagnose_churn_skills_are_parameterized_by_watch_facts() {
        let ctx = sentinel_ctx();
        let watch_out = render_watch_a_run_skill(&ctx);
        for signal in &ctx.watch_signals {
            assert!(
                watch_out.contains(&signal.name),
                "rigger-watch-a-run must interpolate ctx.watch_signals, not hard-code a name; \
                 missing {:?}",
                signal.name
            );
            assert!(
                watch_out.contains(&signal.response),
                "rigger-watch-a-run must interpolate ctx.watch_signals, not hard-code a \
                 response; missing {:?}",
                signal.response
            );
        }
        assert!(
            watch_out.contains(&ctx.watch_poll_interval_secs.to_string()),
            "rigger-watch-a-run must interpolate ctx.watch_poll_interval_secs, not hard-code \
             the poll interval"
        );

        let churn_out = render_diagnose_churn_skill(&ctx);
        assert!(
            churn_out.contains(&ctx.reject_recurrence_diagnose_threshold.to_string()),
            "rigger-diagnose-churn must interpolate ctx.reject_recurrence_diagnose_threshold, \
             not hard-code the diagnose threshold"
        );
    }

    /// Spec 69, criterion 1: `rigger-restore-the-dash` carries the HUNG-HOLDER diagnosis - a
    /// stopped-but-still-bound process makes clients hang rather than fail cleanly, and the
    /// marker's own recorded PID (not a fresh bind attempt) is what names the culprit to
    /// resume or kill - plus the singleton semantics and the `rigger dash` restart path.
    #[test]
    fn restore_the_dash_carries_the_hung_holder_diagnosis() {
        let out = render_restore_the_dash_skill(&sentinel_ctx());
        assert!(
            out.contains("HUNG-HOLDER"),
            "must name the hung-holder case"
        );
        assert!(out.contains("SINGLETON"), "must state singleton semantics");
        assert!(
            out.contains("rigger dash"),
            "must name the real restart command"
        );
        assert!(out.contains("PID"), "must name the PID as the diagnosis");
        assert!(
            out.contains("RESUME") && out.contains("KILL"),
            "must name both remedies for the hung holder's process"
        );
        assert_eq!(out.matches("## Procedure").count(), 1);
        assert_eq!(out.matches("## Anti-move").count(), 1);
    }

    /// Spec 69, criterion 1: `rigger-diagnose-churn` carries the finding-audit procedure WITH
    /// the infra-separation step (F8), pinned against the real diagnose threshold rather than
    /// a hand-typed number, and cross-links `planning-a-spec`'s own churn-signature table and
    /// amendment protocol rather than duplicating either (one authority per concern).
    #[test]
    fn diagnose_churn_carries_the_finding_audit_and_infra_separation() {
        let out = render_diagnose_churn_skill(&real_watch_facts_ctx());
        assert!(
            out.contains(&crate::watch::REJECT_RECURRENCE_DIAGNOSE_THRESHOLD.to_string()),
            "must pin the real diagnose threshold, not a hand-typed number"
        );
        assert!(out.contains("FINDING AUDIT"), "must name the finding audit");
        assert!(
            out.contains("SEPARATE") && out.contains("INFRA"),
            "must carry the infra-separation step"
        );
        assert!(
            out.contains("planning-a-spec") && out.contains("churn signature"),
            "must cross-link the churn-signature table rather than duplicate it"
        );
        assert!(
            out.contains("amendment protocol"),
            "must name the spec-amendment protocol"
        );
        assert_eq!(out.matches("## Procedure").count(), 1);
        assert_eq!(out.matches("## Anti-move").count(), 1);
    }

    /// Spec 69, criterion 1 (accuracy-pinned reject-recurrence tag): `rigger-diagnose-churn`
    /// names the REAL reject-recurrence tag [`crate::blocker::Kind::RejectRecurrence`] actually
    /// renders (`crate::blocker::Blocker::line`), not a hand-typed approximation - normalized
    /// to its `#n/max` SHAPE (the real counts vary per unit; the words and punctuation around
    /// them do not) so a rename of the tag's wording breaks this test the same way
    /// `escalation_skill_names_the_real_blocker_line` catches a rename of the escalated tag.
    #[test]
    fn diagnose_churn_names_the_real_reject_recurrence_tag() {
        let out = render_diagnose_churn_skill(&sentinel_ctx());
        let real_line = crate::blocker::Blocker {
            subject: "some-unit".to_string(),
            kind: crate::blocker::Kind::RejectRecurrence {
                n: 2,
                max: 5,
                cause: "remediating".to_string(),
            },
        }
        .line();
        let normalized = real_line.replace("#2/5", "#n/max");
        assert!(
            out.contains(&normalized),
            "must name the real reject-recurrence tag shape {normalized:?} (from \
             blocker::Kind::RejectRecurrence::line, not a hand-typed one); got: {out}"
        );
    }

    /// Spec 69, criterion 1: each of the three watch-discipline skills' frontmatter carries
    /// its own symptom-bearing "tell" from the spec Design table (this file IS the routing
    /// layer), exactly one `## Procedure` and one `## Anti-move` section in that order, and
    /// cross-links at least one sibling by name - the same shape the spec-68 family proves,
    /// applied to this family.
    #[test]
    fn watch_skills_have_symptom_carrying_descriptions_and_cross_link() {
        /// One watch-discipline skill's name, render function, and the symptom "tells" its
        /// description must carry - factored out so the case table below stays a plain slice
        /// literal instead of tripping clippy's type-complexity lint on an inline tuple type.
        type WatchSkillCase = (
            &'static str,
            fn(&DocsContext) -> String,
            &'static [&'static str],
        );

        // `real_watch_facts_ctx`, not `sentinel_ctx`: rigger-watch-a-run's own cross-link
        // sentence is now built entirely from `ctx.watch_signals[..].response` (the DI fix
        // for arch-u69c1-docscontext-bypass), so it needs the REAL sibling-skill-name
        // responses to actually name a sibling; the other two skills' cross-links are
        // hardcoded prose, unaffected either way.
        let ctx = real_watch_facts_ctx();
        let cases: &[WatchSkillCase] = &[
            (
                "rigger-watch-a-run",
                render_watch_a_run_skill,
                &["just launched", "driven unattended"],
            ),
            (
                "rigger-restore-the-dash",
                render_restore_the_dash_skill,
                &["does not answer", "not serving", "spins"],
            ),
            (
                "rigger-diagnose-churn",
                render_diagnose_churn_skill,
                &["reject-recurrence", "oscillating"],
            ),
        ];
        let family = [
            "rigger-watch-a-run",
            "rigger-restore-the-dash",
            "rigger-diagnose-churn",
        ];
        for (name, render, tells) in cases {
            let out = render(&ctx);
            let frontmatter_end = out.find("\n---\n\n").map(|i| i + 6).unwrap_or(out.len());
            let frontmatter = &out[..frontmatter_end];
            assert!(
                frontmatter.starts_with(&format!("---\nname: {name}\n")),
                "{name}: frontmatter must open naming itself; got: {}",
                &frontmatter[..frontmatter.len().min(80)]
            );
            for tell in *tells {
                assert!(
                    frontmatter.contains(tell),
                    "{name}: description must carry the symptom {tell:?}; got: {frontmatter}"
                );
            }
            assert_eq!(
                out.matches("## Procedure").count(),
                1,
                "{name}: must carry exactly one Procedure section"
            );
            assert_eq!(
                out.matches("## Anti-move").count(),
                1,
                "{name}: must carry exactly one named Anti-move section"
            );
            assert!(
                out.find("## Procedure").unwrap() < out.find("## Anti-move").unwrap(),
                "{name}: Procedure must come before Anti-move"
            );
            let mentions_a_sibling = family
                .iter()
                .filter(|other| **other != *name)
                .any(|other| out.contains(other));
            assert!(
                mentions_a_sibling,
                "{name}: must cross-link at least one sibling skill by name"
            );
        }
    }

    /// The text of `text` from the first `start` up to (not including) the next `end` after
    /// it, so a test reads one section of a shipped document and nothing beyond it.
    fn section<'a>(text: &'a str, start: &str, end: &str) -> &'a str {
        let from = text
            .find(start)
            .unwrap_or_else(|| panic!("{start:?} must be in the document; got:\n{text}"));
        let len = text[from..]
            .find(end)
            .unwrap_or_else(|| panic!("{end:?} must follow {start:?}; got:\n{text}"));
        &text[from..from + len]
    }

    /// The `spec-preflight` registry entry's own body, before the operator-binary stamp.
    fn spec_preflight_body(ctx: &DocsContext) -> String {
        let registry = skill_registry();
        let entry = registry
            .iter()
            .find(|e| e.name == "spec-preflight")
            .expect("spec-preflight must be in the registry");
        entry.body.render(ctx)
    }

    /// Spec 112, criterion 4 (THE SHIPPED SKILL): `spec-preflight` is a registry entry whose
    /// body, like `planning-a-spec`'s, ignores `ctx` (two different contexts render it alike),
    /// opens with its loadable frontmatter and carries its sections in order: why, when, the
    /// four steps, and the mid-run amendment.
    #[test]
    fn spec_preflight_is_a_registry_skill_carrying_its_four_steps_in_order() {
        let body = spec_preflight_body(&sentinel_ctx());
        assert_eq!(
            body,
            spec_preflight_body(&real_watch_facts_ctx()),
            "the spec-preflight body carries no code-derived fact, so it ignores ctx"
        );
        assert!(
            body.starts_with(
                "---\nname: spec-preflight\ndescription: Use before launching any rigger spec, \
                 after planning-a-spec - "
            ),
            "must open with skill frontmatter naming it; got: {}",
            &body[..body.len().min(120)]
        );
        let mut from = 0;
        for heading in [
            "\n---\n\n# Spec preflight\n",
            "\n## Why\n",
            "\n## When\n",
            "\n## Step 1: landing-order simulation\n",
            "\n## Step 2: per-criterion corner walk\n",
            "\n## Step 3: the adversary pass\n",
            "\n## Step 4: resolve, record, re-run\n",
            "\n## Mid-run amendment\n",
        ] {
            let at = body[from..]
                .find(heading)
                .unwrap_or_else(|| panic!("{heading:?} must follow the section before it"));
            from += at + heading.len();
        }
    }

    /// Spec 112, criterion 4 (the F10 and F11 catalog rows): every class the field guide's
    /// failure catalog names, F1 through F11 in order, has exactly one row in
    /// `planning-a-spec`'s churn table, in the table's three-cell shape and the catalog's order,
    /// so the table stays complete; F9, F10 and F11 carry their class labels.
    #[test]
    fn churn_table_has_one_row_for_every_catalog_class() {
        let every_class: Vec<u32> = (1..=11).collect();
        let guide = PLANNING_FIELD_GUIDE_BODY;
        let catalog: Vec<u32> = guide
            .lines()
            .filter_map(|line| line.strip_prefix("### F"))
            .map(|rest| rest.split(" - ").next().unwrap().parse().unwrap())
            .collect();
        assert_eq!(
            catalog, every_class,
            "the field guide's catalog runs F1 to F11"
        );

        let skill = PLANNING_A_SPEC_BODY;
        let header = "| Signature in the run | Catalog class | Fix at spec time |\n|---|---|---|\n";
        let rows = &skill[skill.find(header).expect("the churn table") + header.len()..];
        let mut labels = Vec::new();
        for row in rows.lines().take_while(|line| line.starts_with('|')) {
            let cells: Vec<&str> = row.split('|').collect();
            assert_eq!(cells.len(), 5, "a churn-table row has three cells: {row:?}");
            labels.push(cells[2].trim());
        }
        let rowed: Vec<u32> = labels
            .iter()
            .map(|label| label[1..].split(' ').next().unwrap().parse().unwrap())
            .collect();
        assert_eq!(
            rowed, every_class,
            "one churn-table row per catalog class, in catalog order; got {labels:?}"
        );
        for label in [
            "F9 claim surface",
            "F10 landing-order circularity",
            "F11 undecided removal",
        ] {
            assert!(
                labels.contains(&label),
                "the churn table must carry the {label:?} row; got {labels:?}"
            );
        }
    }

    /// Spec 112, criterion 4: the field guide's F10 and F11 sections each open by naming
    /// itself an F3 shape and the simulation that finds it, then state its tell and a
    /// countermeasure naming the `spec-preflight` step that runs that simulation.
    #[test]
    fn field_guide_f10_and_f11_open_as_f3_shapes_found_by_their_simulation() {
        let guide = PLANNING_FIELD_GUIDE_BODY;
        for (heading, end, opening, step) in [
            (
                "### F10 - Landing-order circularity\n\n",
                "### F11",
                "An F3 shape, found by the landing-order simulation.",
                "as its first step",
            ),
            (
                "### F11 - Undecided removal\n\n",
                "## Amending a spec mid-run",
                "An F3 shape, found by the DROPPED corner of the corner walk.",
                "as its second step",
            ),
        ] {
            let body = &section(guide, heading, end)[heading.len()..];
            assert!(
                crate::wave::normalize_ws(body).starts_with(opening),
                "{heading:?} must open with {opening:?}; got:\n{body}"
            );
            let countermeasure = section(body, "**Countermeasure:**", "\n\n");
            assert!(
                body.contains("The tell is ")
                    && body.find("The tell is ") < body.find("**Countermeasure:**"),
                "{heading:?} must state its tell before its countermeasure; got:\n{body}"
            );
            let countermeasure = crate::wave::normalize_ws(countermeasure);
            assert!(
                countermeasure.contains("The `spec-preflight` skill")
                    && countermeasure.contains(step),
                "{heading:?}'s countermeasure must name the spec-preflight step that finds it; \
                 got:\n{countermeasure}"
            );
        }
    }

    /// Spec 112, criterion 4: the shipped docs carry ONE corner list of eight, walked in one
    /// order - `planning-a-spec`'s step 3, the field guide's F3 countermeasure,
    /// `spec-preflight`'s step 2 and the built-in working-discipline instruction every spawn
    /// receives (`d112-op-seam-items-from-c4`) each name empty, repeated, revert, DROPPED,
    /// concurrent, crash-resume, cold start and existing data.
    #[test]
    fn the_shipped_docs_carry_one_corner_list_of_eight() {
        let ctx = sentinel_ctx();
        let skill = PLANNING_A_SPEC_BODY;
        let guide = PLANNING_FIELD_GUIDE_BODY;
        let preflight = spec_preflight_body(&ctx);
        let (_, discipline) = crate::instructions::BUILTIN
            .iter()
            .find(|(name, _)| *name == "working-discipline")
            .expect("the working-discipline entry is built in");
        for (label, list) in [
            (
                "the working-discipline instruction",
                section(discipline, "leave no corner (", ") unwalked"),
            ),
            (
                "planning-a-spec step 3",
                section(skill, "**3. Run the constraints walk.**", "**4. "),
            ),
            (
                "the field guide's F3 countermeasure",
                section(guide, "**Countermeasure:** the constraints walk.", "### F4"),
            ),
            (
                "spec-preflight step 2",
                section(&preflight, "## Step 2: per-criterion corner walk", "Rules:"),
            ),
        ] {
            let lower = list.to_lowercase();
            let mut from = 0;
            for corner in [
                "empty",
                "repeated",
                "revert",
                "dropped",
                "concurrent",
                "crash-resume",
                "cold start",
                "existing data",
            ] {
                let at = lower[from..].find(corner).unwrap_or_else(|| {
                    panic!("{label} must walk the {corner:?} corner after the one before it; got:\n{list}")
                });
                from += at + corner.len();
            }
        }
    }

    /// Spec 112, criterion 4: `planning-a-spec`'s step 7 names the `spec-preflight` skill and,
    /// under a workflow with a critic, `rigger critique <spec>` before launch.
    #[test]
    fn planning_a_spec_step_7_runs_spec_preflight_and_the_critique_before_launch() {
        let skill = PLANNING_A_SPEC_BODY;
        let step = section(
            skill,
            "**7. Preflight, then launch.**",
            "## Amending mid-run",
        );
        assert!(
            crate::wave::normalize_ws(step).contains(
                "Run the `spec-preflight` skill and, under a workflow with a critic, `rigger \
                 critique <spec>` before launch."
            ),
            "step 7 must name spec-preflight and the critique before launch; got:\n{step}"
        );
    }
}
