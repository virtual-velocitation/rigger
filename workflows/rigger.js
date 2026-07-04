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
    'The rigger dev-loop as a native workflow, driven THINLY: a courier agent advances the Rust conductor one frontier via `rigger step`, the script spawns the returned wave of agents natively in parallel (each grounded, personified, and worktree-isolated by the conductor), every worker self-reports via `rigger result`, a worker that dies without reporting has its failure recorded on its behalf, and the loop repeats until done. All decomposition, per-unit implement -> cargo gates -> three-tier adversarial review -> integrate, and bounded remediation live in the conductor; the /workflows progress groups are labelled per unit (`<unit>:<stage>`) at runtime via opts.phase, and meta.phases names only the fixed stages because it must be a static literal.',
  phases: [
    { title: 'Plan', detail: 'the conductor sets up the run branch and decomposes the spec into a unit DAG on the first `rigger step` (one global pass)' },
    { title: 'Build', detail: 'per-unit implement + cargo gates; the conductor parks the implementer, the driver spawns it under opts.phase "<unit>:<stage>"' },
    { title: 'Review', detail: 'per-unit three-tier adversarial review (lenses, adversary, adjudicator); the conductor parks each reviewer, the driver spawns it under "<unit>:<stage>"' },
    { title: 'Integrate', detail: 'per-unit merge of the approved unit onto the run branch; the conductor does the merge when a unit passes review' },
  ],
}

// args: a spec path string, or { repo, spec, base }.
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
    error: { type: 'string' },
  },
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
// markerPath is the per-spawn liveness marker (spec 10, unit 3). It MUST match Rust
// `liveness::marker_path`: <scratch-root>/agent-live/<sanitized id>, where the id's chars
// outside [A-Za-z0-9._-] each become '_'. The scratch root is the SAME default the sweep
// resolves (<repo>/.rigger/tmp), matching the SCRATCH POLICY path framed below.
function markerPath(id) {
  const safe = id.replace(/[^A-Za-z0-9._-]/g, '_')
  return `${REPO}/.rigger/tmp/agent-live/${safe}`
}

async function runWorker(req, fatal) {
  const ph = phaseOf(req)
  const workdir = req.dir
    ? `Do all your file edits, cargo, and any git commit inside your isolated worktree ${req.dir} (the conductor assigned it and owns its lifecycle; run \`rigger ...\` commands from ${REPO}).`
    : `Work in ${REPO}.`
  // The driver-framed liveness heartbeat (spec 10, unit 3), same mechanism family as the
  // SCRATCH POLICY: only when this spawn carries a wall-clock bound. The worker keeps a
  // per-spawn marker fresh so a HUNG agent (one that stops touching it) is caught by
  // `rigger step`'s liveness sweep as an infrastructure fault - never charging the unit.
  const heartbeat = req.max_wall_clock
    ? `LIVENESS HEARTBEAT (spec 10): your spawn carries a ${req.max_wall_clock}s wall-clock bound. Prove you are alive by TOUCHING your per-spawn marker at the START of your work and again after each significant step (a tool call, a build, a commit), using Bash:\n` +
      `  mkdir -p ${REPO}/.rigger/tmp/agent-live && touch ${markerPath(req.id)}\n` +
      `\`rigger step\` treats this marker going stale beyond your ${req.max_wall_clock}s bound as a HUNG agent - an infrastructure fault that charges you NO remediation attempt - so keep it fresh while you work. It stops mattering the instant you self-report your result.\n`
    : ''
  const prompt =
    `You are the rigger worker for spawn ${req.id} (unit ${req.unit}). ` +
    `Your persona and full task are recorded in the run log - FETCH THEM FIRST by running, from ${REPO}, using Bash:\n` +
    `  cd ${REPO} && rigger prompt '${req.id}'\n` +
    `Everything it prints (a persona above a \`---\` line when present, then the task) IS your assignment - follow it as if it were this message. Then:\n\n` +
    `--- rigger driver instructions ---\n` +
    `${workdir}\n` +
    `SCRATCH POLICY (hard rule): any scratch YOU create - probe repos, verification worktrees, test builds, setup rehearsals - lives under ${REPO}/.rigger/tmp/agent-scratch/, NEVER under /tmp or your own session scratchpad (those are on the operator's small OS partition, and a single cargo target or \`rigger setup\` shim install there fills the disk). For any cargo you run outside your assigned worktree, export CARGO_TARGET_DIR=${REPO}/.rigger/tmp/cargo-target first. agent-scratch is swept when the run completes - do not store anything durable there.\n` +
    heartbeat +
    `The rigger context tools your task refers to (rigger_emit, rigger_peers) are available here as the CLI commands \`rigger emit <Type> '<json>'\` and \`rigger peers <file>...\`, run from ${REPO}.\n` +
    `When you finish, SELF-REPORT your result by running, from ${REPO}:\n` +
    `  rigger result ${req.id} "<your result: a one-line summary, or your full verdict/findings>"\n` +
    `(pipe multi-line output via stdin instead, e.g. \`rigger result ${req.id}\` reading a heredoc). ` +
    `Also record the model that actually served you so the run's audit trail carries it (spec 05: every spawn's recorded events carry the resolved model id): add \`--meta '{"resolved_model":"<the concrete model id you ran as${req.model ? `, e.g. the resolved version of ${req.model}` : ''}>"}'\` to that success report. ` +
    `If you cannot complete the task, report the failure instead: \`rigger result ${req.id} --error "<why it failed>"\` (the message must be non-empty). ` +
    `--error means YOU were unable to perform your task (blocked, crashed, missing tools) - NEVER a negative conclusion: a reviewer whose verdict is REJECT, or a gate that found failures, COMPLETED its task and reports that verdict/finding as its NORMAL result (an --error replays as a dead worker and aborts the run, not as your verdict). ` +
    `Reporting your result is mandatory - the run cannot advance past this spawn until you do.`

  // Run the worker, but do not await it FOREVER when it carries a wall-clock bound: a HUNG
  // agent must not stall the whole wave (spec 10, unit 3). Map agent() to a never-rejecting
  // outcome so abandoning it can never surface as an unhandled rejection after we stop
  // awaiting it, then race it against an opt-in wall-clock watchdog.
  const ran = agent(prompt, { phase: ph, model: req.model || undefined, label: req.id }).then(
    () => ({ kind: 'done' }),
    (e) => ({ kind: 'error', e }),
  )
  const wallMs = req.max_wall_clock ? req.max_wall_clock * 1000 : 0
  let timer = null
  const watchdog =
    wallMs > 0 && typeof setTimeout === 'function'
      ? new Promise((resolve) => {
          timer = setTimeout(() => resolve({ kind: 'hung' }), wallMs)
        })
      : null
  const outcome = watchdog ? await Promise.race([ran, watchdog]) : await ran
  if (timer) clearTimeout(timer)

  if (outcome.kind === 'hung') {
    // The worker outran its max_wall_clock. Presume it HUNG and STOP awaiting it. Do NOT run
    // the death courier: that is the dead-worker-EXIT path and would CHARGE the unit, whereas
    // a hung agent is an INFRASTRUCTURE fault. We just return, so parallel() resolves and the
    // loop reaches the next `rigger step`, whose liveness sweep classifies the spawn's stale
    // marker as infra (no attempt charged) and surfaces it. A worker that was merely slow but
    // kept its marker fresh is found not-stale by the sweep, left in-flight, and re-run next
    // wave. The abandoned agent() promise (`ran`) is inert - it resolves to an ignored value.
    log(
      `worker ${req.id} exceeded its ${req.max_wall_clock}s max_wall_clock - abandoning it; the next \`rigger step\` liveness sweep classifies a stale marker as infra (no attempt charged)`,
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
    try {
      await agent(
        `You are a rigger COURIER. The worker for spawn ${req.id} died. Record its failure ON ITS BEHALF, but ONLY if it did not already self-report - a result the worker already recorded must NEVER be overwritten. Run EXACTLY this, from ${REPO}, using Bash (ONE command):\n` +
          `  cd ${REPO} && rigger result ${req.id} --if-absent --error "worker ${req.id} died without reporting: ${msg}"\n` +
          `\`--if-absent\` records the failure ATOMICALLY only when the spawn has no result yet; if the worker already reported, it writes nothing and still exits 0, so an existing result is never overwritten (the message is non-empty by construction). Confirm the command exited 0; report nothing else.`,
        { phase: ph, model: 'haiku', label: `report-death:${req.id}` },
      )
    } catch (ce) {
      // The death-report COURIER itself died (max turns / crash). We can no longer guarantee a
      // result was recorded for ${req.id}, so the conductor's replay could hang on resume. Do
      // NOT swallow it and do NOT re-throw (that would reject parallel() and abort sibling
      // workers mid-wave); record it in the shared `fatal` sink so the loop stops LOUDLY once
      // the wave has drained.
      const cmsg = (ce && ce.message ? ce.message : String(ce)).replace(/\s+/g, ' ').trim().slice(0, 200)
      log(`FATAL: the death-report courier for ${req.id} itself failed: ${cmsg} - the spawn may have no result`)
      fatal.push(`${req.id}: ${cmsg}`)
    }
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
  let step
  try {
    step = await agent(
      `You are a rigger COURIER. Advance the run one frontier and return the wave, verbatim. Run EXACTLY this, from ${REPO}, using Bash with the timeout parameter set to 600000 (a step runs cargo gates inline and can take many minutes; the default timeout kills it mid-work):\n` +
        `  cd ${REPO} && CARGO_TARGET_DIR=${REPO}/.rigger/tmp/cargo-target rigger step --spec ${SPEC}${BASEFLAG}\n` +
        `(the CARGO_TARGET_DIR prefix makes every gate share one build cache instead of cold-building per worktree - keep it exactly as written). ` +
        `It prints ONE line of JSON on stdout: {"wave":[...],"done":<bool>} (a halted run also carries a "halted":"<reason>" field). Return that JSON object EXACTLY as printed, INLINE and IN FULL, in your structured output - no matter how large it is. NEVER write it to a file, return a path, a reference, a summary, or a truncation: the driver can only read your returned JSON, so anything but the verbatim object (all wave items, all their fields) LOSES the wave and stalls the run. Do not drop fields or run anything else. ` +
        `If the Bash call TIMES OUT, re-run the exact same command - as many times as needed: the step's gate results are recorded durably as they complete, so every re-run resumes past the recorded ones and gets strictly further; return the JSON from the run that prints it. ` +
        `NEVER fabricate or guess the JSON: if you cannot obtain it after many re-runs, or the command prints no JSON / exits non-zero (not a timeout), return {"wave":[],"done":true,"error":"<the stderr / failure message, or 'step did not complete within my attempts'>"} so the loop stops cleanly and the error is visible.`,
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
