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
         --derived` keeps the LATEST event per replay key of each derived index type, deletes the \
         superseded re-recordings, and compacts the file so events.db shrinks on disk. Every \
         other event survives byte-for-byte - lessons, decisions, findings, gate verdicts, and \
         the whole run history `rigger stats` and replay read. The live graph is unchanged: every \
         recording of one key folds to the same rows, and the prune carries a pruned key's \
         EARLIEST recorded valid-time onto the recording it keeps, so a design fact keeps the \
         date it first became true rather than being re-dated to whichever recording survived. \
         WHAT IT CANNOT RECLAIM, because this decides whether it is worth running at all: it only \
         ever sheds DUPLICATE recordings of one key, never the index itself. The last recording \
         of every key stays, so on a log that holds no key twice `rigger reset --derived` deletes \
         ZERO rows from it and reports so - that is the expected report on a clean log, not a \
         failure, and the derived index remains the bulk of the log by design because it is what \
         the graph is folded from. WHEN A DEDUPLICATED LOG STILL HAS SOMETHING TO SHED, because \
         a non-zero prune is otherwise read as a broken dedup: a log written since the dedup \
         existed holds one recording per distinct fact EXCEPT where a file's content has \
         RETURNED to a generation the log had already recorded - a revert, a branch switch, a \
         checkout back - which re-records that file's whole batch by design, since a dedup that \
         suppressed an already-recorded key would strand the graph on the version the file has \
         since moved past. A prune that sheds rows on such a log is shedding that duplication, \
         not covering for a defect; a log written BEFORE the dedup sheds the whole accumulated \
         pile instead. WHAT IT COSTS TO RUN: the compaction rewrites events.db in full and stages \
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
         ran could reissue a gap and reorder the log, so it refuses while a `rigger step` holds \
         its lock, a unit in the current run is not yet terminal, a spawn is in flight, or a \
         driver registration for this store is still live, naming what it found. `--force-live` \
         overrides the refusal for an operator certain no writer is using the store; it checks \
         nothing.\n"
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
concurrent actors, crash-resume, cold start (fresh process, empty memory). Write what must
happen into the spec. If a prescribed mechanism fails a corner under a constraint, the spec is
self-contradictory - fix it now; the panel will otherwise find it around attempt 5.

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
  one's").
Type shapes, tables, long detail: a non-criteria Notes section. Two behaviors joined by "and":
two checkboxes.

**6. Carry the house constraints.** Hyphens not em dashes (U+2014 fails the diff gate); both
feature lanes green; no new event type unless the spec's whole point is one; fallback stated
for any criterion that might be impossible; anything the gates cannot see flagged for the
adjudicator to demand evidence on.

**7. Preflight, then launch.** `rigger validate` is mandatory (it catches model-alias drift -
run `rigger canary --if-model-changed` on a warning); `rigger reset --runs` before a large run;
anchor `base=` on the ref the work must land on. Launch via the /rigger workflow only.

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
"#;

/// Render the `planning-a-spec` skill. `ctx` is accepted only to match the registry's
/// uniform `fn(&DocsContext) -> String` signature ([`SkillEntry`]); this body has nothing
/// in it to interpolate from `ctx`.
fn render_planning_a_spec_skill(_ctx: &DocsContext) -> String {
    PLANNING_A_SPEC_BODY.to_string()
}

/// Render the `rigger-reset-store` skill (spec 68, criterion 2): store hygiene for the
/// three files under `.rigger/`. `ctx` is accepted only to match the registry's uniform
/// signature; nothing here is drift-prone enough to interpolate from it.
fn render_reset_store_skill(_ctx: &DocsContext) -> String {
    let mut s = String::new();
    s.push_str("---\n");
    s.push_str("name: rigger-reset-store\n");
    s.push_str(
        "description: Store hygiene for rigger's own state - growing .rigger/ disk usage, \
         the bloat advisory from `rigger validate`, or `rigger step`/replay running slow. \
         Read this before running `rigger reset` or touching any store file by hand.\n",
    );
    s.push_str("---\n\n");
    s.push_str("# rigger-reset-store\n\n");
    let _ = writeln!(
        s,
        "rigger keeps three stores under `.rigger/`, and only one of them holds anything \
         durable:\n"
    );
    let _ = writeln!(
        s,
        "- `events.db` - the event log. This IS the truth: every decision, finding, gate \
         verdict, and run milestone rigger has ever recorded, in the order it happened. \
         Nothing else derives it; it derives everything else."
    );
    let _ = writeln!(
        s,
        "- `graph.db` - the context graph. A REBUILDABLE projection folded from the event \
         log: rigger-build-graph regenerates it from `events.db` alone, so losing it loses \
         time, never truth."
    );
    let _ = writeln!(
        s,
        "- `progress.db` - live per-agent progress telemetry. Never replayed into a run's \
         state; it is a side channel `rigger status` and the dashboard read to show what an \
         agent is doing right now, not a record anything else depends on.\n"
    );
    let _ = writeln!(s, "## Procedure\n");
    let _ = writeln!(
        s,
        "`rigger reset` with no flags is the MENU, not an error: it exits 0 and prints one \
         line per prunable accumulation, each with a measured count and the flag that acts \
         on it. It is read-only - safe to run any time just to look.\n"
    );
    let _ = writeln!(
        s,
        "- `rigger reset --runs` prunes dead-run rows and superseded edges out of \
         `graph.db`. It works over ANY event-store backend (the graph is always a local \
         file); rerun it any time, especially before a large run."
    );
    let _ = writeln!(
        s,
        "- `rigger reset --derived` compacts `events.db`: it keeps the LATEST event per \
         replay key of each derived project-ingest type, deletes the superseded \
         duplicates, and vacuums so the file shrinks on disk. Every other event - every \
         decision, finding, lesson, gate verdict, the whole run history - survives \
         byte-for-byte. Only the embedded sqlite backend can compact this way, and it \
         refuses (unless overridden with `--force-live`) while a run is live against the \
         store."
    );
    let _ = writeln!(
        s,
        "- The two flags compose: `rigger reset --runs --derived` sheds both \
         accumulations in one pass.\n"
    );
    let _ = writeln!(s, "## Anti-move\n");
    let _ = writeln!(
        s,
        "Never touch `events.db`, `graph.db`, or `progress.db` with raw SQL, `rm`, or any \
         tool outside `rigger reset`. The event log is append-only truth: a hand-edit or a \
         hand-deleted row can desync the graph from the log in ways `rigger reset \
         --derived`'s own key-preserving compaction is specifically built to avoid. A store \
         file that is genuinely corrupt is an incident to fix at its root, never a reason \
         to reach for a database client.\n"
    );
    let _ = writeln!(s, "## See also\n");
    let _ = writeln!(
        s,
        "rigger-build-graph if `graph.db` needs regenerating rather than pruning; \
         rigger-reindex if only the symbols index is stale.\n"
    );
    s
}

/// Render the `rigger-build-graph` skill (spec 68, criterion 2): the cold-build entry
/// point for the context graph. `ctx` is accepted only to match the registry's uniform
/// signature; nothing here is drift-prone enough to interpolate from it.
fn render_build_graph_skill(_ctx: &DocsContext) -> String {
    let mut s = String::new();
    s.push_str("---\n");
    s.push_str("name: rigger-build-graph\n");
    s.push_str(
        "description: Cold-build the context graph - empty `rigger graph --around`/`--show` \
         lookups on a repo that already has source, or a first setup before any run exists. \
         Read this before deleting a store file to force a re-ingest.\n",
    );
    s.push_str("---\n\n");
    s.push_str("# rigger-build-graph\n\n");
    let _ = writeln!(s, "## Procedure\n");
    let _ = writeln!(
        s,
        "`rigger graph build` folds the project's source straight into `.rigger/graph.db` - \
         no run, no `RunStarted`, nothing but the code-ingest events the fold already emits. \
         It CREATES the store when the checkout is cold (`.rigger/` does not exist yet) and \
         REFRESHES an existing store incrementally: an unchanged file re-ingests nothing, and \
         it reuses the exact same walk-and-content-key ingest authority a live run uses, so a \
         standalone build and a run can never fold the same file under two different keys.\n"
    );
    let _ = writeln!(
        s,
        "Rerun it any time it is convenient - on a schedule, after pulling a large set of \
         changes, or simply because a lookup came back empty and you want to check. It is \
         always safe: nothing is deleted, only appended and incrementally refreshed.\n"
    );
    let _ = writeln!(s, "## Anti-move\n");
    let _ = writeln!(
        s,
        "Never force a rebuild by deleting `.rigger/graph.db` (or `events.db`) and \
         re-running `rigger graph build` on the empty result. Deleting the log throws away \
         truth that no rebuild can get back, and deleting only the graph is unnecessary work \
         `rigger graph build` already does FOR you, incrementally, without erasing anything \
         first. If lookups are empty, just run `rigger graph build`; only reach for \
         rigger-reset-store if you specifically mean to prune, not rebuild.\n"
    );
    let _ = writeln!(s, "## See also\n");
    let _ = writeln!(
        s,
        "rigger-reindex for a narrower staleness problem - one that is really about the \
         symbols grounding index, not the whole structural graph; rigger-reset-store for \
         pruning `graph.db`'s dead-run accumulation rather than rebuilding it.\n"
    );
    s
}

/// Render the `rigger-reindex` skill (spec 68, criterion 2): the targeted refresh for the
/// symbols grounding index. `ctx` is accepted only to match the registry's uniform
/// signature; nothing here is drift-prone enough to interpolate from it.
fn render_reindex_skill(_ctx: &DocsContext) -> String {
    let mut s = String::new();
    s.push_str("---\n");
    s.push_str("name: rigger-reindex\n");
    s.push_str(
        "description: Refresh the symbols grounding index - a `rigger graph`/`rigger ground` \
         lookup that names an entity the current tree no longer holds, or the \
         index-staleness advisory from `rigger validate`. Read this before rebuilding the \
         whole graph over an index-freshness problem.\n",
    );
    s.push_str("---\n\n");
    s.push_str("# rigger-reindex\n\n");
    let _ = writeln!(s, "## Procedure\n");
    let _ = writeln!(
        s,
        "`rigger reindex <file>...` re-parses ONLY the named files and persists the delta to \
         the project's symbols grounding index at `.rigger/symbols/` - the fast, targeted fix \
         for an index that has drifted from files you just changed (a unit's own commit, a \
         rebase, a branch switch). It is scoped strictly to the symbols index, a DIFFERENT \
         store from the structural context graph, so it costs only the named files, never a \
         walk of the whole tree.\n"
    );
    let _ = writeln!(
        s,
        "Name every file whose content changed since the index was last built; an unnamed \
         file's stale entry is left exactly as it was.\n"
    );
    let _ = writeln!(s, "## Anti-move\n");
    let _ = writeln!(
        s,
        "Do not reach for a whole-graph rebuild (see rigger-build-graph) or a store wipe to \
         fix a lookup that is really an index-freshness problem: naming the stale files and \
         reindexing exactly them is both cheaper and more targeted than rebuilding the whole \
         structural graph over a handful of drifted entries. Reserve a whole-graph rebuild \
         for when the graph itself is missing or empty, not for a symbols lookup that just \
         needs the files it names re-parsed.\n"
    );
    let _ = writeln!(s, "## See also\n");
    let _ = writeln!(
        s,
        "rigger-build-graph for the whole-project structural graph; rigger-reset-store for \
         the stores' own hygiene.\n"
    );
    s
}

/// Render the `rigger-resume-a-run` skill (spec 68, criterion 2): continuing a run after
/// its driver died mid-flight. `ctx` is accepted only to match the registry's uniform
/// signature; nothing here is drift-prone enough to interpolate from it.
fn render_resume_a_run_skill(_ctx: &DocsContext) -> String {
    let mut s = String::new();
    s.push_str("---\n");
    s.push_str("name: rigger-resume-a-run\n");
    s.push_str(
        "description: Continue interrupted work after a dead driver (spent quota, a crash, a \
         laptop that slept mid-run) or `rigger status` showing an agent 'in flight' with a \
         stale heartbeat. Read this before relaunching a run or reaching for `--fresh`.\n",
    );
    s.push_str("---\n\n");
    s.push_str("# rigger-resume-a-run\n\n");
    let _ = writeln!(s, "## Procedure\n");
    let _ = writeln!(
        s,
        "Diagnose first: `rigger status` (or the dashboard) shows each in-flight agent's \
         last progress report and heartbeat age. A stale heartbeat with no recent store \
         event means the DRIVER died mid-run (quota ran out, the process crashed, the \
         machine slept) - it does not mean the run itself is broken; the event log already \
         holds every decision and gate verdict the run made before the driver stopped.\n"
    );
    let _ = writeln!(
        s,
        "Relaunch the same blessed driver on the same spec WITHOUT `--fresh` - `rigger run \
         <spec>`, `rigger serve <spec>` / `rigger workflow <spec>`, or the native `/rigger \
         <spec>` workflow with its `fresh` argument left unset. Because the run lives in the \
         event log, not in the dead process, the conductor's own run-starting step adopts \
         the existing run instead of minting a new one: it replays the log, rebuilds its \
         in-memory state, and continues exactly where the dead driver left off. No unit \
         restarts from zero and no work already recorded is lost.\n"
    );
    let _ = writeln!(
        s,
        "`--fresh` is for a DIFFERENT situation, not this one: a run wedged in a terminal \
         state (for example a plan-critique escalation) on a spec that is otherwise \
         UNCHANGED. It is a one-shot new-run boundary, never the default way to continue \
         interrupted work.\n"
    );
    let _ = writeln!(s, "## Anti-move\n");
    let _ = writeln!(
        s,
        "Never hand-drive `rigger step` yourself in a shell to \"help it along\" - the \
         driver owns stepping, and a hand step races it, which can double-spawn a unit or \
         wedge the frontier (see using-rigger). And do not reach for `--fresh` reflexively \
         just because a run looks stuck: on a merely-interrupted run it abandons the \
         adoptable state your relaunch would otherwise have continued from, in exchange for \
         nothing - reserve it for the genuinely wedged-terminal case above.\n"
    );
    let _ = writeln!(s, "## See also\n");
    let _ = writeln!(
        s,
        "rigger-handle-an-escalation for the run-level and unit-level terminal states \
         `--fresh` genuinely exists for.\n"
    );
    s
}

/// Render the `rigger-handle-an-escalation` skill (spec 68, criterion 2): acting on a
/// unit the loop handed back to a human. Unlike its four siblings, this one DOES
/// interpolate from `ctx`: the remediation bound it names is [`DocsContext::max_retries`],
/// the same code-derived fact `using-rigger` interpolates, so the two can never disagree
/// on what the bound actually is.
fn render_handle_an_escalation_skill(ctx: &DocsContext) -> String {
    let mut s = String::new();
    s.push_str("---\n");
    s.push_str("name: rigger-handle-an-escalation\n");
    s.push_str(
        "description: Act on a unit the loop handed back - `rigger status` (or the \
         dashboard) names it `escalated (awaiting a human)` after it exhausted its \
         remediation attempts. Read this before touching the unit's branch or relaunching \
         the run.\n",
    );
    s.push_str("---\n\n");
    s.push_str("# rigger-handle-an-escalation\n\n");
    let _ = writeln!(s, "## Procedure\n");
    let _ = writeln!(
        s,
        "An escalated unit gave up at the remediation bound (`defaults.max_retries`, {max} \
         by default) - the loop will not retry it on its own; it is waiting on a human \
         decision. Read the recorded lesson for the CONCRETE final failure - via `rigger \
         peers` scoped to the unit's files, or the dashboard - rather than guessing: the \
         escalation lesson carries the actual failing gate or review reason, not a \
         placeholder, and that reason is the bounded remedy you are about to apply.\n",
        max = ctx.max_retries
    );
    let _ = writeln!(
        s,
        "Apply EXACTLY that remedy on the unit's durable branch (`rigger/u/<unit-id>`, the \
         branch rigger itself created and kept for this unit's committed work across every \
         attempt) - nothing more, nothing less. Then relaunch the run fresh - `rigger run \
         --fresh <spec>` (or `rigger serve --fresh <spec>` / the native `/rigger <spec>` \
         workflow with `fresh` set) - against the same, otherwise-unchanged spec: the \
         conductor mints a new run boundary, and the loop picks the escalated unit back up \
         with a clean remediation budget.\n"
    );
    let _ = writeln!(s, "## Anti-move\n");
    let _ = writeln!(
        s,
        "Never hand-merge the unit's durable branch onto the run branch yourself - that \
         bypasses review and integration and forks the merged code away from what the event \
         log says happened. And never re-implement more than the remedy names: scope creep \
         here is work the next review has no record of and did not ask for. If the remedy \
         genuinely needs more than a bounded fix, that is a reason to amend the spec (see \
         planning-a-spec), not to freelance on the branch.\n"
    );
    let _ = writeln!(s, "## See also\n");
    let _ = writeln!(
        s,
        "rigger-resume-a-run for the DIFFERENT case of a merely-interrupted run, where \
         `--fresh` is the wrong move.\n"
    );
    s
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
/// name and the function that renders its BODY (before the operator-binary prohibition is
/// stamped on) from the code-derived [`DocsContext`]. An entry whose content carries no
/// drift-prone facts (`planning-a-spec`) simply ignores `ctx`.
///
/// [`skill_registry`] is the ONE enumeration every surface walks - `rigger docs` (renders
/// each entry to its committed source), `rigger setup` (installs each entry, overlay
/// honored), and the docs-drift gate (drift-checks each entry) - so adding an entry here is
/// the ONLY step required to make a skill render, install, and drift-check; no surface
/// needs its own edit.
pub struct SkillEntry {
    pub name: &'static str,
    render_body: fn(&DocsContext) -> String,
}

impl SkillEntry {
    /// This entry's FULL rendered content: its body plus the
    /// [`OPERATOR_BINARY_PROHIBITION`], stamped here - once, for every entry - rather than
    /// by each skill's own author.
    pub fn render(&self, ctx: &DocsContext) -> String {
        let mut s = (self.render_body)(ctx);
        s.push_str(OPERATOR_BINARY_PROHIBITION);
        s
    }
}

/// The skill registry (spec 68, criterion 1): `using-rigger` (the driving discipline),
/// `planning-a-spec` (the authoring discipline), and the five-member per-operation family
/// (spec 68, criterion 2) - one skill per operation, joining this same list by appending
/// entries, never by adding a second, independently-walked enumeration.
pub fn skill_registry() -> Vec<SkillEntry> {
    vec![
        SkillEntry {
            name: "using-rigger",
            render_body: render_using_rigger_skill,
        },
        SkillEntry {
            name: "planning-a-spec",
            render_body: render_planning_a_spec_skill,
        },
        SkillEntry {
            name: "rigger-reset-store",
            render_body: render_reset_store_skill,
        },
        SkillEntry {
            name: "rigger-build-graph",
            render_body: render_build_graph_skill,
        },
        SkillEntry {
            name: "rigger-reindex",
            render_body: render_reindex_skill,
        },
        SkillEntry {
            name: "rigger-resume-a-run",
            render_body: render_resume_a_run_skill,
        },
        SkillEntry {
            name: "rigger-handle-an-escalation",
            render_body: render_handle_an_escalation_skill,
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
        }
    }

    #[test]
    fn skill_render_is_parameterized_by_every_fact() {
        let ctx = sentinel_ctx();
        let out = render_using_rigger_skill(&ctx);
        assert!(out.contains("SENTINEL/base-ref"), "base_ref not rendered");
        assert!(out.contains("65531"), "dash_port not rendered");
        assert!(out.contains("999"), "max_retries not rendered");
        assert!(
            out.contains("sentinelverdict"),
            "verdict_approve not rendered"
        );
        assert!(
            out.contains("sentinel-rule-a"),
            "spec_shape_rule not rendered"
        );
        assert!(
            out.contains("sentinel recommendation text"),
            "spec_shape_recommendation not rendered"
        );
        assert!(out.contains("sentinelcmd-a"), "subcommand not rendered");
    }

    #[test]
    fn handbook_render_is_parameterized_by_every_fact() {
        let ctx = sentinel_ctx();
        let out = render_handbook_discipline(&ctx);
        assert!(out.contains("SENTINEL/base-ref"), "base_ref not rendered");
        assert!(out.contains("65531"), "dash_port not rendered");
        assert!(out.contains("999"), "max_retries not rendered");
        assert!(
            out.contains("sentinelverdict"),
            "verdict_approve not rendered"
        );
        assert!(
            out.contains("sentinel-rule-a"),
            "spec_shape_rule not rendered"
        );
        assert!(out.contains("sentinelcmd-a"), "subcommand not rendered");
    }

    #[test]
    fn both_outputs_render_from_the_one_context() {
        // A fact set only on the context appears in BOTH outputs, proving one context
        // feeds both renders (no second, drifting source).
        let ctx = sentinel_ctx();
        assert!(render_using_rigger_skill(&ctx).contains("SENTINEL/base-ref"));
        assert!(render_handbook_discipline(&ctx).contains("SENTINEL/base-ref"));
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
    /// an append-only log: WHAT IT KEEPS (the latest event per replay key of each derived index
    /// type), WHAT IT COSTS (nothing else - every other event survives byte-for-byte, so lessons,
    /// decisions, findings and the run history `stats` and replay read are untouched), that the
    /// FILE actually shrinks, and that the two flags COMPOSE rather than one superseding the
    /// other. Both shipped outputs render from `discipline_body`, so the skill and the handbook
    /// chapter cannot disagree and a drift here would drift for every consumer at once.
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
                out.contains("LATEST event per replay key"),
                "{label} must state what the derived prune KEEPS, so an operator can predict it"
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
            let raw_body = (entry.render_body)(&ctx);
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
        let out = render_planning_a_spec_skill(&sentinel_ctx());
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

    /// Spec 68, criterion 2: the five-member per-operation family is IN the registry,
    /// each name present exactly once, alongside (not instead of) `using-rigger` and
    /// `planning-a-spec`.
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
        ] {
            assert_eq!(
                names.iter().filter(|n| **n == expected).count(),
                1,
                "{expected:?} must appear exactly once in the registry; got {names:?}"
            );
        }
        assert_eq!(
            names.len(),
            7,
            "the registry must have exactly 7 entries; got {names:?}"
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
            let out = (entry.render_body)(&ctx);
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
            let out = (entry.render_body)(&ctx);
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
            let out = (entry.render_body)(&ctx);
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
                (*name, (entry.render_body)(&ctx))
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
}
