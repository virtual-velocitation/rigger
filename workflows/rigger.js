// The native `/rigger` Claude Code workflow - a THIN client over the Rust conductor.
//
// All of the dev-loop's intelligence (decomposing the spec into a unit DAG, ordering
// the units, the per-unit implement -> cargo gates -> three-tier adversarial review ->
// integrate lifecycle, and bounded remediation) lives in the Rust conductor and is
// delivered one frontier at a time by `rigger step`. This script does NOT re-implement
// any of it. It only:
//   1. COURIERS a step: an agent runs `cd <repo> && rigger step` and returns the one
//      line of JSON it prints - `{"wave":[<SpawnRequest>...],"done":<bool>}` - the wave
//      the conductor newly parked plus whether the run has reached a fixpoint.
//   2. SPAWNS the wave natively in parallel: one `agent()` per SpawnRequest, each in its
//      own per-unit `opts.phase` progress group so the /workflows display groups a unit's
//      agents together. Two ready units with disjoint blast radii share a wave, so
//      fan-out falls straight out of the conductor's partition - the driver just runs it.
//   3. Lets each worker SELF-REPORT via `rigger result <id> ...`, which is exactly what
//      the next `rigger step` replays past to advance the run. A worker that DIES without
//      reporting (its `agent()` rejects: max turns, a crash) has its failure recorded on
//      its behalf by a courier agent - but CONDITIONALLY and ATOMICALLY: the courier runs a
//      single `rigger result <id> --if-absent --error <why>`, which writes the `--error`
//      ONLY when the spawn has no result yet and leaves an existing result untouched. A
//      worker (or a reviewer that already emitted an approve verdict) that self-reported and
//      THEN ran on to max-turns must not have its result clobbered - `rigger result` is
//      last-write-wins - so `--if-absent` honors the "dies WITHOUT reporting" clause in ONE
//      atomic step (closing the read-then-write TOCTOU window a two-process `rigger reported
//      <id> || rigger result <id> --error` guard would leave open) while still guaranteeing
//      every parked spawn ends with a result and the run can never hang.
//   4. LOOPS until a step reports `done`. Every anomalous exit - a courier agent that itself
//      dies, `rigger step` failing, a failure that could not be recorded, or a stall - stops
//      the loop LOUDLY (throws with a clear message) rather than aborting mid-agent or being
//      reported as a clean completion; only a real fixpoint resolves the workflow.
//
// rigger's shared context store lives in <repo>/.rigger; every `rigger ...` command runs
// in REPO. Each worker does its code edits, cargo, and commit inside the isolated worktree
// the conductor assigned it (SpawnRequest.dir); the conductor owns that worktree's
// lifecycle and the run-branch anchoring (`rigger step` sets up the run branch before it
// parks anything). `base` (default origin/main) is threaded to `rigger step --base`: it is
// the ref the run branch is created FROM the first time it does not exist (falling back to
// HEAD if unresolvable); an existing run branch is reused, never reset.

// meta MUST be a pure literal: the Workflow runtime extracts it statically (before the
// workflow body ever runs), so it cannot contain computed values or interpolation. Unit
// ids come from the conductor at RUNTIME and are unknowable at static-extraction time, so
// meta.phases names only the FIXED lifecycle stages a unit passes through; the per-unit
// distinction that makes the /workflows display match execution is carried entirely by the
// runtime `opts.phase` strings the driver builds from each wave item (see `phaseOf` below).
export const meta = {
  name: 'rigger',
  description:
    'Turn a spec into working, reviewed code: it splits the spec into small units, implements and tests each one, reviews every change before merging it, and stops loudly if it gets stuck. Use it when you want a spec built out automatically instead of by hand.',
  phases: [
    { title: 'Plan', detail: 'the conductor sets up the run branch and decomposes the spec into a unit DAG on the first `rigger step` (one global pass)' },
    { title: 'Build', detail: 'per-unit implement + cargo gates; the conductor parks the implementer, the driver spawns it under opts.phase "<unit>:<stage>"' },
    { title: 'Review', detail: 'per-unit three-tier adversarial review (lenses, adversary, adjudicator); the conductor parks each reviewer, the driver spawns it under "<unit>:<stage>"' },
    { title: 'Integrate', detail: 'per-unit merge of the approved unit onto the run branch; the conductor does the merge when a unit passes review' },
  ],
}

// args: a spec path string, or { repo, spec, base, fresh }.
let A = args
if (typeof A === 'string') {
  try {
    A = JSON.parse(A)
  } catch (e) {
    A = { spec: A }
  }
}
A = A || {}
const REPO = A.repo || '.'
const SPEC = A.spec || 'spec.md'
// Pass --base ONLY when the caller explicitly provided one: `rigger step` applies its
// own default (origin/main) for a run branch it must create, and an existing run branch
// is reused with its own history as the anchor - an explicit --base that cannot be
// applied to an existing branch draws a stderr advisory, so the steady state (no base
// given, branch reused) stays silent instead of alarming the courier every step.
const BASEFLAG = A.base ? ` --base ${A.base}` : ''
// `fresh`: begin a NEW run for this spec even if the latest run already matches it (which
// `rigger step` would otherwise adopt/resume). The evented restart for a run wedged in a
// terminal state whose spec is unchanged - e.g. an escalated plan-critique from a
// since-fixed defect. Passed ONLY on the FIRST step (see the loop): that step just parks
// the planner (no gates, so it is fast and will not time out into a courier re-run), and
// every step after it adopts the boundary it began. The prior run stays in the log.
const FRESH = !!A.fresh

// OUTER_WALL_CLOCK_SEC is the driver's OUTER per-agent wall-clock (spec 19c, unit 2): the
// TOTAL-RUNTIME ceiling past which even an UNBOUNDED-config spawn is abandoned-and-surfaced,
// so a hung agent surfaces within a bounded time rather than being awaited forever. It applies
// ONLY to a spawn that carries NO per-spawn `max_wall_clock` (an unbounded config: defaults.
// max_wall_clock is 0 and the agent set none). Such a spawn is told NO liveness heartbeat and
// `rigger step`'s sweep - which times out only a POSITIVE bound - never reaches it, so this
// coarse total-runtime cap is the only backstop that keeps it from stalling the run silently.
// A BOUNDED spawn is left to the precise, marker-staleness watchdog (raceMarkerStaleness) that
// deliberately leaves a slow-but-ALIVE, marker-fresh worker in-flight; the outer cap does not
// touch it. The coarse default is intentionally generous (a legitimately long unbounded spawn
// should finish well inside it); an operator who wants a tighter, precise bound sets
// `defaults.max_wall_clock` - exactly what `rigger validate`'s unbounded-wall-clock advisory
// nudges. An optional `outer_wall_clock` arg field overrides the default (positive seconds),
// for tuning or a shorter cap in a test harness.
const OUTER_WALL_CLOCK_SEC = Number(A.outer_wall_clock) > 0 ? Number(A.outer_wall_clock) : 4 * 60 * 60

// The JSON shape `rigger step` prints (see spawn::Step / spawn::SpawnRequest): the wave it
// newly parked and a `done` fixpoint flag. The wave items carry everything the driver needs
// to spawn each agent. Optional SpawnRequest fields are omitted from the wire when empty, so
// only id/unit/stage/prompt are required; extra fields are tolerated (additionalProperties).
// `halted` is the spawn-budget HALT reason (Gap 13): present (distinct from a clean `done`)
// when the breaker stopped the run with work undone, so the driver stops LOUDLY on it.
// `error` is the courier's own out-of-band channel: if `rigger step` itself fails, the
// courier reports the message here rather than fabricating a wave.
// Top level rejects unknown fields (additionalProperties: false): a courier that
// invents a side-channel (a file reference, a summary field) fails validation and is
// retried, instead of smuggling an empty wave past the driver. Wave ITEMS stay open
// (additionalProperties: true) for forward-compat with new SpawnRequest fields.
// Wave items are SLIM MANIFESTS (spawn-by-reference): identity, placement, and model
// only - never the prompt. A review-round prompt can run to hundreds of kilobytes and
// a wave to megabytes, which cannot survive a model-relayed structured output
// verbatim; each worker fetches its own prompt from the log with `rigger prompt <id>`.
const STEP = {
  type: 'object',
  additionalProperties: false,
  required: ['wave', 'done'],
  properties: {
    wave: {
      type: 'array',
      items: {
        type: 'object',
        additionalProperties: true,
        required: ['id', 'unit', 'stage'],
        properties: {
          id: { type: 'string' },
          unit: { type: 'string' },
          stage: { type: 'string' },
          // The live work-line (spec 19a, c4): the unit's criterion, carried on the wave item
          // so the driver narrates the actual WORK, not just `${unit}:${stage}`. Omitted from
          // the wire for an untitled spawn; wave items stay open (additionalProperties: true).
          title: { type: 'string' },
          model: { type: 'string' },
          dir: { type: 'string' },
          tools: { type: 'array', items: { type: 'string' } },
          blast_radius: { type: 'array', items: { type: 'string' } },
        },
      },
    },
    done: { type: 'boolean' },
    // A spawn-budget HALT (Gap 13): `rigger step` sets this to the halt reason (e.g.
    // "budget exhausted: 200/200 spawns") when the breaker stopped the run with work
    // undone, distinct from a clean `done` convergence. Omitted on a converged run. The
    // top level rejects unknown properties, so this MUST be declared or a halted step's
    // JSON would fail validation and the halt would be lost.
    halted: { type: 'string' },
    // The WEDGED-terminus set (spec 19c, unit 1): `rigger step` lists the units that
    // ESCALATED - each exhausted remediation and went terminal WITHOUT integrating - so a
    // `done` fixpoint reached with any of them is NOT a clean completion. Omitted on a clean
    // run (preserving the historical `{wave,done}` shape); when present the driver stops
    // LOUDLY on it, exactly as it does for `halted`. Like `halted` this MUST be declared or
    // the top level (additionalProperties:false) would reject a wedged step's JSON and the
    // wedge would be lost.
    escalated: { type: 'array', items: { type: 'string' } },
    error: { type: 'string' },
  },
}

// The shape the LIVENESS READER courier returns (spec 14): the one line `rigger status --json`
// prints, verbatim - a JSON array of the in-flight agents. The watchdog couriers `rigger
// status` (not a raw `stat`) because the Workflow SCRIPT sandbox has no filesystem access -
// only `agent()` - so it consumes rigger's PRESENTED liveness, which rigger read from the
// marker in Rust, rather than reconstructing one file's mtime by proxy. rigger.js JSON-parses
// `stdout` itself (JSON is a sandbox built-in).
const STATUS = {
  type: 'object',
  additionalProperties: false,
  required: ['stdout'],
  properties: { stdout: { type: 'string' } },
}

// phaseOf builds a worker's per-unit `opts.phase` progress-group label from the wave item,
// exactly per the documented `unit + stage` contract on spawn::SpawnRequest. The conductor
// currently sets both to the unit id, so a unit's whole wave (implementer + reviewers)
// shares one group - which is precisely the grouping we want; if the conductor later
// distinguishes the stage half, this label refines automatically with no driver change.
function phaseOf(req) {
  return `${req.unit}:${req.stage}`
}

// runWorker spawns one wave item natively and lets it self-report. The Workflow `agent()`
// primitive accepts only { phase, model, schema, label }, so everything the cli/serve
// drivers pass out-of-band (the persona as --system-prompt, the worktree as cwd) must ride
// in the prompt here. The conductor already ground the task and folded peer decisions into
// req.prompt; the driver only frames it with the persona, the worktree, the rigger-CLI note
// (the native Workflow path has no MCP proxy, so the rigger_emit/rigger_peers the prompt
// references are used as `rigger emit`/`rigger peers` CLI commands), and the self-report
// contract. If the agent REJECTS, a death courier records the failure on its behalf - but
// ATOMICALLY via `rigger result --if-absent`, so it never overwrites a result the worker
// already reported.
//
// `fatal` is a shared sink: if the death courier ITSELF dies, we can no longer guarantee a
// result was recorded for this spawn, so we push it here and the loop stops loudly after the
// wave drains rather than swallowing the failure (which would hang the run on resume).
// runWorker and its liveness helpers are defined below (the helpers first, so runWorker can
// reference them).

// livenessAgeSeconds asks RIGGER for a spawn's liveness age - the whole seconds since it last
// touched its marker - by COURIERING `rigger status --json` (spec 14) and reading this spawn's
// `liveness_age_s` from the presented view. rigger stats the marker in Rust and PRESENTS the
// age, so the driver consumes rigger's consolidated view instead of reconstructing one file's
// mtime with a raw `stat` (the RETIRED haiku probe). Returns the integer age, or null when the
// spawn is absent from the view (no marker / not in flight), the JSON does not parse, or the
// courier itself failed - all treated CONSERVATIVELY as not-stale (never abandon a worker on a
// missing or flaky signal). The whole in-flight wave rides in ONE presented view, so a shared
// poll could serve every worker; kept per-worker here to match the existing watchdog shape.
async function livenessAgeSeconds(ph, id) {
  try {
    const out = await agent(
      `You are a rigger LIVENESS READER. Run EXACTLY this from ${REPO} using Bash and return its single line of stdout VERBATIM in \`stdout\`:\n` +
        `  cd ${REPO} && rigger status --json\n` +
        `It prints ONE line: a JSON array of the in-flight agents. Return that line EXACTLY as printed - do not summarize, reformat, or truncate it.`,
      { phase: ph, model: 'haiku', schema: STATUS, label: `liveness:${id}` },
    )
    const arr = JSON.parse(out.stdout)
    const me = Array.isArray(arr) ? arr.find((a) => a && a.id === id) : null
    return me && typeof me.liveness_age_s === 'number' ? me.liveness_age_s : null
  } catch {
    // The courier died, the JSON did not parse, or the spawn is absent: unknown, so treat as
    // not-stale. A flaky reader must never manufacture a false liveness halt.
    return null
  }
}

// raceMarkerStaleness is the per-worker MARKER-STALENESS watchdog (spec 10, unit 3; decision
// d-u3r2-js-watchdog-marker-staleness, which SUPERSEDES d-u3-liveness-design(5)). It races the
// worker's own outcome against the marker going stale, and returns whichever happens first:
// the worker's {kind:'done'|'error'} if it finishes, or {kind:'hung'} once the marker has been
// IDLE (untouched) longer than boundSec.
//
// It is NOT a total-runtime cap. It polls the marker's IDLE time - now minus its last touch -
// which is the SAME staleness the Rust sweep judges (`liveness::is_stale`), so the JS and the
// sweep share ONE definition of hung. A slow-but-ALIVE worker that keeps its marker fresh is
// therefore LEFT IN-FLIGHT indefinitely, never abandoned-and-re-run (the exact dup-exec the
// old wall-clock cap caused). Only a genuinely stale marker - which a hung worker leaves stale -
// makes it abandon, and because the marker IS stale at that moment, the very next `rigger step`
// sweep records the infra fault and halts LOUDLY (and the answered spawn is not re-run).
//
// A worker that finishes within its bound triggers ZERO probes (the common case: the first
// window has not even elapsed). A MISSING/unreadable marker is conservatively not-stale: a
// worker that never heartbeats is dead-worker-EXIT territory (its own agent timeout / the death
// courier), unchanged here per this unit's exclusion - so the loop keeps waiting on it.
async function raceMarkerStaleness(ran, boundSec, ph, id) {
  for (;;) {
    // Wait one bound-length window, but wake immediately if the worker resolves first.
    let timer = null
    const window = new Promise((resolve) => {
      timer = setTimeout(() => resolve({ kind: 'tick' }), boundSec * 1000)
    })
    const tick = await Promise.race([ran, window])
    if (timer) clearTimeout(timer) // never leave a bound-long timer dangling once the race is decided
    if (tick.kind !== 'tick') return tick // the worker finished/errored inside the window
    // A full window elapsed with the worker still running: ask rigger for its liveness age.
    const idle = await livenessAgeSeconds(ph, id)
    if (idle !== null && idle > boundSec) return { kind: 'hung' }
    // Fresh (or unknown/missing): slow-but-alive, leave in-flight and read again next window.
  }
}

// raceOuterWallClock is the OUTER, TOTAL-RUNTIME wall-clock (spec 19c, unit 2): it races an
// UNBOUNDED-config spawn's outcome against a single boundSec-long deadline and returns whichever
// resolves first - the worker's {kind:'done'|'error'} if it finishes, or {kind:'outer'} once
// boundSec of WALL TIME has elapsed with the worker still running. Unlike raceMarkerStaleness
// (which polls marker IDLE time and so leaves a marker-fresh worker in-flight forever), this is a
// hard total-runtime CEILING: it is the backstop for a spawn that carries no per-spawn bound and
// heartbeats nothing, so no liveness signal exists to poll and only elapsed wall time can bound
// it. Its caller abandons-and-SURFACES the {kind:'outer'} spawn (records a liveness fault on its
// behalf, below), so a hung agent under an unbounded config surfaces within a bounded time
// instead of being awaited forever.
async function raceOuterWallClock(ran, boundSec) {
  let timer = null
  const deadline = new Promise((resolve) => {
    timer = setTimeout(() => resolve({ kind: 'outer' }), boundSec * 1000)
  })
  const outcome = await Promise.race([ran, deadline])
  if (timer) clearTimeout(timer) // never leave the ceiling timer dangling once the race is decided
  return outcome
}

// recordFaultCourier records a fault on a spawn's behalf, atomically and conditionally, via a
// short courier - the SINGLE authority BOTH fault paths share, never a second parallel courier:
// a dead worker whose agent() rejected, and an UNBOUNDED worker that blew the outer wall-clock.
// It runs `rigger result <id> --if-absent --error "<why>"` (plus an optional `--meta`), so the
// fault lands ONLY when the spawn has no result yet and a worker that self-reported at the last
// moment is never clobbered (`rigger result` is otherwise last-write-wins). If the courier ITSELF
// dies we can no longer guarantee the fault was recorded (the conductor's replay could hang on
// resume), so - exactly as the worker path must - we neither swallow it nor re-throw mid-wave
// (which would reject parallel() and abort sibling workers): we push it into the shared `fatal`
// sink so the loop stops LOUDLY once the wave has drained. `why` MUST be shell-safe (the caller
// neutralizes any untrusted text before passing it); `meta`, when given, is a single-quote-safe
// JSON string appended as `--meta '<meta>'` (e.g. the `liveness_class` an infra fault carries).
async function recordFaultCourier(req, ph, why, meta, label, fatal) {
  const metaFlag = meta ? ` --meta '${meta}'` : ''
  try {
    await agent(
      `You are a rigger COURIER. Record a fault for spawn ${req.id} ON ITS BEHALF, but ONLY if it did not already self-report - a result the worker already recorded must NEVER be overwritten. Run EXACTLY this, from ${REPO}, using Bash (ONE command):\n` +
        `  cd ${REPO} && rigger result ${req.id} --if-absent --error "${why}"${metaFlag}\n` +
        `\`--if-absent\` records the fault ATOMICALLY only when the spawn has no result yet; if the worker already reported, it writes nothing and still exits 0, so an existing result is never overwritten (the message is non-empty by construction). Confirm the command exited 0; report nothing else.`,
      { phase: ph, model: 'haiku', label },
    )
  } catch (ce) {
    // The courier itself died (max turns / crash). Do NOT swallow it and do NOT re-throw (that
    // would reject parallel() and abort sibling workers mid-wave); record it in the shared `fatal`
    // sink so the loop stops LOUDLY once the wave has drained.
    const cmsg = (ce && ce.message ? ce.message : String(ce)).replace(/\s+/g, ' ').trim().slice(0, 200)
    log(`FATAL: the fault courier for ${req.id} itself failed: ${cmsg} - the fault may be unrecorded`)
    fatal.push(`${req.id}: ${cmsg}`)
  }
}

async function runWorker(req, fatal) {
  const ph = phaseOf(req)
  // The live work-line (spec 19a, c4): the unit's criterion the conductor threaded onto the
  // wave item. It rides the RENDER surfaces here - the log() narrator and the per-worker
  // progress-group label - so an observer sees the actual WORK a spawn is doing, not just its
  // `${req.unit}:${req.stage}` group. Collapsed to a single line so a multi-line criterion
  // never breaks the one-line narration; empty for an untitled (plan/canary) spawn, which then
  // renders exactly as before. The group label (phaseOf) is UNCHANGED - the title is additive.
  const work = (req.title || '').replace(/\s+/g, ' ').trim()
  const workLabel = work ? `${req.id} · ${work}` : req.id
  // Narrate the start of this worker's run so a long silent stretch is a visible line, not a
  // gap; the title is what turns `${req.unit}:${req.stage}` into the actual criterion.
  log(`starting ${req.id}${work ? `: ${work}` : ''}`)
  const workdir = req.dir
    ? `Do all your file edits, cargo, and any git commit inside your isolated worktree ${req.dir} (the conductor assigned it and owns its lifecycle; run \`rigger ...\` commands from ${REPO}).`
    : `Work in ${REPO}.`
  // The driver-framed liveness heartbeat (spec 10, unit 3), same mechanism family as the
  // SCRATCH POLICY: only when this spawn carries a wall-clock bound AND `rigger step` resolved
  // a marker path for it. The worker keeps THAT EXACT per-spawn marker fresh - the path the
  // step stamped on the wire from the single `liveness::marker_path` authority, so the
  // worker-write path is identical to the sweep-read path under any scratch config (never a
  // re-hardcoded root). A HUNG agent (one that stops touching it) is then caught by `rigger
  // step`'s liveness sweep as an infrastructure fault - never charging the unit.
  const marker = req.marker_path
  const heartbeat =
    req.max_wall_clock && marker
      ? `LIVENESS HEARTBEAT (spec 10): your spawn carries a ${req.max_wall_clock}s wall-clock bound. Prove you are alive by TOUCHING your per-spawn marker at the START of your work and again after each significant step (a tool call, a build, a commit), using Bash:\n` +
        `  mkdir -p "$(dirname "${marker}")" && touch "${marker}"\n` +
        `\`rigger step\` treats this marker going stale (left untouched) beyond your ${req.max_wall_clock}s bound as a HUNG agent - an infrastructure fault that charges you NO remediation attempt - so keep it fresh while you work. It stops mattering the instant you self-report your result.\n`
      : ''
  // Live progress (spec 14): every worker reports one short line after each significant step,
  // additive to the marker heartbeat above. This is what turns a 26-minute silent stretch of
  // real work into a visible stream an observer (and `rigger status` / the dash) can follow.
  const progressNote =
    `LIVE PROGRESS (spec 14): after each significant step - a search, a file read, a build, a commit, a decision - report ONE short line of what you just did, from ${REPO}, using Bash:\n` +
    `  rigger progress '${req.id}' '<one line: what you just did>'\n` +
    `This is how an observer sees you working between the milestones you record, so a long silent stretch is never mistaken for a stall. Keep it flowing WHILE you work; do not batch it at the end.\n`
  const prompt =
    `You are the rigger worker for spawn ${req.id} (unit ${req.unit}). ` +
    `Your persona and full task are recorded in the run log - FETCH THEM FIRST by running, from ${REPO}, using Bash:\n` +
    `  cd ${REPO} && rigger prompt '${req.id}'\n` +
    `Everything it prints (a persona above a \`---\` line when present, then the task) IS your assignment - follow it as if it were this message. Then:\n\n` +
    `--- rigger driver instructions ---\n` +
    `${workdir}\n` +
    `SCRATCH POLICY (hard rule): any scratch YOU create - probe repos, verification worktrees, test builds, setup rehearsals - lives under ${REPO}/.rigger/tmp/agent-scratch/, NEVER under /tmp or your own session scratchpad (those are on the operator's small OS partition, and a single cargo target or \`rigger setup\` shim install there fills the disk). For any cargo you run outside your assigned worktree, export CARGO_TARGET_DIR=${REPO}/.rigger/tmp/cargo-target first. agent-scratch is swept when the run completes - do not store anything durable there.\n` +
    heartbeat +
    progressNote +
    `The rigger context tools your task refers to (rigger_emit, rigger_peers) are available here as the CLI commands \`rigger emit --spawn '${req.id}' <Type> '<json>'\` and \`rigger peers <file>...\`, run from ${REPO}. The \`--spawn '${req.id}'\` stamps the emit with YOUR spawn id so the conductor attributes it to you exactly (spec 18) - always include it on every \`rigger emit\`.\n` +
    `When you finish, SELF-REPORT your result by running, from ${REPO}:\n` +
    `  rigger result ${req.id} "<your result: a one-line summary, or your full verdict/findings>"\n` +
    `(pipe multi-line output via stdin instead, e.g. \`rigger result ${req.id}\` reading a heredoc). ` +
    `Also record the model that actually served you so the run's audit trail carries it (spec 05: every spawn's recorded events carry the resolved model id): add \`--meta '{"resolved_model":"<the concrete model id you ran as${req.model ? `, e.g. the resolved version of ${req.model}` : ''}>"}'\` to that success report. ` +
    `If you cannot complete the task, report the failure instead: \`rigger result ${req.id} --error "<why it failed>"\` (the message must be non-empty). ` +
    `--error means YOU were unable to perform your task (blocked, crashed, missing tools) - NEVER a negative conclusion: a reviewer whose verdict is REJECT, or a gate that found failures, COMPLETED its task and reports that verdict/finding as its NORMAL result (an --error replays as a dead worker and aborts the run, not as your verdict). ` +
    `Reporting your result is mandatory - the run cannot advance past this spawn until you do.`

  // Run the worker, but do not await it FOREVER: a HUNG agent must not stall the whole wave
  // (spec 10, unit 3; spec 19c, unit 2). Map agent() to a never-rejecting outcome so abandoning
  // it can never surface as an unhandled rejection after we stop awaiting it, then race it
  // against the appropriate wall-clock below (marker-staleness for a bounded spawn, the outer
  // total-runtime ceiling for an unbounded one).
  const ran = agent(prompt, { phase: ph, model: req.model || undefined, label: workLabel }).then(
    () => ({ kind: 'done' }),
    (e) => ({ kind: 'error', e }),
  )
  // TWO wall-clocks bound this await so a hung agent never stalls the whole wave forever:
  //  - a BOUNDED spawn (its own per-spawn max_wall_clock AND a step-resolved marker) rides the
  //    precise MARKER-STALENESS watchdog: it abandons only a genuinely stale (idle-since-last-
  //    touch) marker and deliberately leaves a slow-but-ALIVE, marker-fresh worker in-flight
  //    (spec 10, unit 3) - NOT a total-runtime cap.
  //  - an UNBOUNDED-config spawn (no per-spawn bound, so it is told no heartbeat and carries no
  //    marker the sweep could ever time out) rides the OUTER total-runtime wall-clock (spec 19c,
  //    unit 2): a coarse ceiling that abandons-and-surfaces it after OUTER_WALL_CLOCK_SEC so it
  //    is never awaited forever, the only backstop available when no liveness signal exists.
  // Both are opt-in on setTimeout existing; without it we await plainly (unchanged).
  let outcome
  if (typeof setTimeout !== 'function') {
    outcome = await ran
  } else if (req.max_wall_clock && marker) {
    outcome = await raceMarkerStaleness(ran, req.max_wall_clock, ph, req.id)
  } else {
    outcome = await raceOuterWallClock(ran, OUTER_WALL_CLOCK_SEC)
  }

  if (outcome.kind === 'outer') {
    // An UNBOUNDED-config spawn blew the OUTER total-runtime ceiling (spec 19c, unit 2). The
    // `rigger step` liveness sweep can NEVER surface this one - it times out only a POSITIVE
    // per-spawn bound and this spawn has none - so unlike the marker-staleness `hung` path we
    // cannot lean on the next sweep. Surface it OURSELVES as an INFRASTRUCTURE fault through the
    // shared recordFaultCourier authority, stamping the `liveness_class:infra` meta the sweep
    // itself uses; the next `rigger step` then reads that fault through `hung_spawns` and halts
    // the wave LOUDLY. Because a liveness fault charges NO remediation attempt, the unit's code is
    // never blamed for its agent hanging. We then return so parallel() resolves; the abandoned
    // agent() promise (`ran`) is inert. `no per-spawn max_wall_clock` is what distinguishes this
    // from the marker-staleness `hung` path below - the sweep can time out a bounded spawn but not
    // this one, so the driver is its only backstop.
    log(
      `worker ${req.id}: no per-spawn max_wall_clock and running past the ${OUTER_WALL_CLOCK_SEC}s OUTER wall-clock - presuming HUNG and abandoning it; recording an infra liveness fault so the next \`rigger step\` halts loudly (no attempt charged). Set defaults.max_wall_clock for a precise per-spawn bound.`,
    )
    await recordFaultCourier(
      req,
      ph,
      `worker ${req.id} hung: ran past the ${OUTER_WALL_CLOCK_SEC}s outer wall-clock with no per-spawn max_wall_clock`,
      '{"liveness_class":"infra"}',
      `report-hung:${req.id}`,
      fatal,
    )
    return
  }

  if (outcome.kind === 'hung') {
    // The worker's marker went STALE beyond its bound (idle-since-last-touch): presume it HUNG
    // and STOP awaiting it. Do NOT run the death courier - that is the dead-worker-EXIT path and
    // would CHARGE the unit, whereas a hung agent is an INFRASTRUCTURE fault. We just return, so
    // parallel() resolves and the loop reaches the next `rigger step`, whose liveness sweep sees
    // the SAME stale marker (a hung worker leaves it stale), records an infra fault (no attempt
    // charged), and halts the wave LOUDLY. Because that fault ANSWERS the spawn, it is not
    // re-run - no dup-exec. A worker that was merely slow but kept its marker fresh never
    // reaches here (raceMarkerStaleness left it in-flight). The abandoned agent() promise
    // (`ran`) is inert - it resolves to an ignored value.
    log(
      `worker ${req.id}: liveness marker idle past its ${req.max_wall_clock}s bound - presuming HUNG and abandoning it; the next \`rigger step\` sweep records an infra fault (no attempt charged) and halts loudly`,
    )
    return
  }
  if (outcome.kind === 'done') {
    return
  }
  {
    const e = outcome.e
    // The worker's agent() REJECTED (max turns, a crash, an execution error). That rejection
    // does NOT prove it died before reporting - a worker (or a reviewer that already emitted
    // an approve verdict) can self-report and THEN run on to max-turns. So record its failure
    // ON ITS BEHALF via a single ATOMIC `rigger result <id> --if-absent --error <why>`: the
    // `--error` lands ONLY when the spawn has no result yet, leaving an existing result
    // untouched. `rigger result` is last-write-wins, so an unconditional --error would CLOBBER
    // a self-reported success/approve and force-fail an approved unit on the next replay;
    // `--if-absent` prevents exactly that in ONE step - closing the read-then-write TOCTOU
    // window a two-process `rigger reported <id> || rigger result <id> --error` guard leaves
    // open (a self-report landing between the check and the record) - while still ensuring
    // every parked spawn ends with a result (the run can never hang).
    // The --error message must be non-empty (a blank error would replay AS a success). Neutralize
    // shell metacharacters (`"`, backtick, `$`, `\`) so it can never break out of - or trigger
    // substitution inside - the double-quoted --error arg in the courier command.
    const why = (e && e.message ? e.message : String(e))
      .replace(/["`$\\]/g, "'")
      .replace(/\s+/g, ' ')
      .trim()
      .slice(0, 400)
    const msg = why || 'the worker agent exited without producing a result'
    log(`worker ${req.id} agent rejected: ${msg.slice(0, 80)} - recording its failure on its behalf IF it has not already reported`)
    // Record the death through the SAME single fault-courier authority the outer-wall-clock path
    // uses (no `--meta`: a dead worker is a plain failure, not a liveness fault). One concern -
    // atomic no-clobber fault record + fatal-on-courier-death - implemented once, never twice.
    await recordFaultCourier(
      req,
      ph,
      `worker ${req.id} died without reporting: ${msg}`,
      undefined,
      `report-death:${req.id}`,
      fatal,
    )
  }
}

// The single global phase marker: everything up front (and the courier steps, which have no
// unit of their own) is the run's Plan/orchestration pass. The per-unit progress groups are
// the runtime opts.phase strings on the workers, NOT a global phase('Build') marker - a
// global build marker would falsely imply every unit builds together before any review, when
// in fact each unit runs its whole Build -> Review -> Integrate lifecycle (inside the
// conductor) before the next unit's spawns are parked.
phase('Plan')

// The thin driver loop. Each iteration: courier one `rigger step`, spawn the wave it parked,
// and stop when the conductor reports a fixpoint. Termination is guaranteed by the conductor
// (its spawn-budget breaker and per-unit retry bound), so this loop needs no cap of its own.
// Every non-fixpoint exit is an ANOMALY and stops the loop LOUDLY (`stop(...)` throws): a
// stuck/failed run must never be reported as a clean completion, and a courier that itself
// dies must be a controlled, visible stop - not an uncaught rejection that aborts the driver.
let waves = 0
// `--fresh` is a ONE-SHOT: it begins a new run, so it rides the FIRST step only; every step
// after it must ADOPT that boundary, not mint another. Flipped false the moment it is used.
let firstStep = true

// stop the driver LOUDLY: throw a clear, single Error so the anomalous exit surfaces as a
// workflow failure with an actionable message (decision `thin-driver-loud-stops`), instead of
// resolving as success (which would mask a hung/failed run) or aborting mid-agent uncaught.
function stop(reason) {
  log(`stopping the driver loop: ${reason}`)
  throw new Error(`rigger driver stopped after ${waves} wave(s): ${reason}`)
}

for (;;) {
  // 1. Courier: advance the conductor one frontier and return the wave verbatim. `rigger step`
  //    sets up/reuses the run branch (via --base) before parking anything, then prints one line
  //    of JSON. On the FIRST step the run branch is anchored and the spec is decomposed; on
  //    later steps the conductor replays past the results workers reported and parks the next
  //    frontier. If `rigger step` errors, the courier returns it in `error` (not a faked wave);
  //    if the COURIER AGENT itself dies (max turns / crash), agent() rejects and the try/catch
  //    turns that into the same clean, loud stop instead of aborting the whole driver uncaught.
  // `--fresh` rides the FIRST step only (a one-shot new-run boundary); adopt it thereafter.
  const FRESHFLAG = firstStep && FRESH ? ' --fresh' : ''
  firstStep = false
  let step
  try {
    step = await agent(
      `You are a rigger COURIER. Advance the run one frontier and return the wave, verbatim. Run EXACTLY this, from ${REPO}, using Bash with the timeout parameter set to 1800000 (a step integrates ready units by running cargo gates on BOTH feature lanes inline and can take 20+ minutes when several units are ready at once; too short a timeout kills it mid-gate and the courier exhausts its re-run attempts before the gates ever finish):\n` +
        `  cd ${REPO} && CARGO_TARGET_DIR=${REPO}/.rigger/tmp/cargo-target rigger step --spec ${SPEC}${BASEFLAG}${FRESHFLAG}\n` +
        `(the CARGO_TARGET_DIR prefix makes every gate share one build cache instead of cold-building per worktree - keep it exactly as written). ` +
        `It prints ONE line of JSON on stdout: {"wave":[...],"done":<bool>} (a halted run also carries a "halted":"<reason>" field). Return that JSON object EXACTLY as printed, INLINE and IN FULL, in your structured output - no matter how large it is. NEVER write it to a file, return a path, a reference, a summary, or a truncation: the driver can only read your returned JSON, so anything but the verbatim object (all wave items, all their fields) LOSES the wave and stalls the run. Do not drop fields or run anything else. ` +
        `If the Bash call TIMES OUT, re-run the exact same command - as many times as needed: the step's gate results are recorded durably as they complete, so every re-run resumes past the recorded ones and gets strictly further; return the JSON from the run that prints it. ` +
        `If it exits non-zero and stderr says "another \`rigger step\` is already running" (a TRANSIENT concurrent step - e.g. an earlier step orphaned by a Bash timeout is still finishing its gate, and steps are serialized so two never run at once), WAIT ~60 seconds and re-run the exact same command; repeat until it returns the JSON. This is normal back-off, NOT a failure. ` +
        `NEVER fabricate or guess the JSON: if you cannot obtain it after many re-runs, or the command prints no JSON / exits non-zero for a DIFFERENT reason (not a timeout and not the "already running" back-off), return {"wave":[],"done":true,"error":"<the stderr / failure message, or 'step did not complete within my attempts'>"} so the loop stops cleanly and the error is visible.`,
      // sonnet, not haiku: the courier's one job is a verbatim relay of a possibly
      // large JSON object, and haiku demonstrably "helps" by externalizing big waves
      // to a file reference - which loses the wave (the driver reads only the
      // returned JSON) and stalls the run.
      { phase: 'Plan', model: 'sonnet', schema: STEP, label: `step#${waves + 1}` },
    )
  } catch (e) {
    // The `rigger step` courier AGENT itself rejected (its own max turns / crash) - distinct
    // from `rigger step` failing, which the courier reports in `error`. Without this catch the
    // rejection would abort the whole driver uncaught; instead stop cleanly and loudly.
    stop(`the \`rigger step\` courier agent itself failed: ${e && e.message ? e.message : String(e)}`)
  }

  if (step.error) {
    stop(`\`rigger step\` failed: ${step.error}`)
  }

  // 2. Spawn the wave natively in parallel; each worker in its own per-unit progress group. A
  //    worker that dies has its failure recorded on its behalf inside runWorker; if that death
  //    courier ITSELF dies, runWorker records it in `fatal` (it never re-throws, so parallel()
  //    is not aborted mid-wave) and we stop loudly below.
  const fatal = []
  const wave = step.wave || []
  if (wave.length > 0) {
    waves += 1
    log(`wave ${waves}: spawning ${wave.length} agent(s) in parallel: ${wave.map((r) => r.id).join(', ')}`)
    await parallel(wave.map((req) => () => runWorker(req, fatal)))
  }

  // A death-report courier died, so a spawn may have no result and the conductor's replay could
  // hang on resume. Stop LOUDLY rather than looping into an unrecoverable hang.
  if (fatal.length > 0) {
    stop(`the failure of ${fatal.length} worker(s) could not be recorded (their death-report couriers also died): ${fatal.join(' | ')}`)
  }

  // 3. A budget (or other rail) HALT is a LOUD stop, never a clean completion (Gap 13).
  //    `rigger step` reports it as a `halted` reason distinct from `done` convergence: the
  //    breaker stopped the run with ready work unscheduled (a resume needs a raised budget).
  //    We drain any wave the halting step already parked (above), then surface the halt as a
  //    workflow FAILURE carrying the reason - rather than letting the `done` fixpoint below
  //    read a starved run as success (the exact Gap-13 defect: a breaker halt printed as a
  //    clean completion and the driver reporting a starved run as done).
  if (step.halted) {
    stop(`the run halted: ${step.halted}`)
  }

  // 4. Stop at the conductor's fixpoint (every parked spawn has a result and nothing new was
  //    parked). A non-empty wave always implies done === false, so we drain it first (above),
  //    then re-check on the next iteration.
  if (step.done) {
    // A fixpoint reached with an ESCALATED unit is NOT a clean completion (spec 19c, unit 1):
    // the unit exhausted remediation and went terminal WITHOUT integrating, yet the run
    // converged AROUND it, so a bare `done` would report a wedged terminus as success. Surface
    // it LOUDLY, naming the units - exactly as a budget halt is surfaced - so a unit that can
    // never pass review is never mistaken for a landed one. Escalation-and-continue mid-run is
    // untouched: this gates only the FINAL terminus (`done`), never a mid-run wave.
    const escalated = step.escalated || []
    if (escalated.length > 0) {
      stop(`the run reached a fixpoint but ${escalated.length} unit(s) never integrated (escalated after exhausting remediation): ${escalated.join(', ')}`)
    }
    log(`run complete: the conductor reached a fixpoint after ${waves} wave(s)`)
    break
  }
  // An empty wave that is NOT done means a prior worker resolved WITHOUT self-reporting (its
  // agent() neither errored nor recorded a result): the conductor has an unanswered spawn but
  // there is nothing new for us to run, so stepping again would spin. This is an anomaly, not a
  // completion - stop loudly rather than resolve as done or loop forever.
  if (wave.length === 0) {
    stop('`rigger step` parked no new wave yet is not done (a worker likely resolved without self-reporting)')
  }
}

return { waves }
