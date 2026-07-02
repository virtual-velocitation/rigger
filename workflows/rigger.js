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
//      its behalf by a courier agent - but GUARDED as check-then-record: the courier runs
//      `rigger reported <id> || rigger result <id> --error <why>`, so the `--error` is
//      written ONLY when the spawn has no result yet. A worker (or a reviewer that already
//      emitted an approve verdict) that self-reported and THEN ran on to max-turns must not
//      have its result clobbered - `rigger result` is last-write-wins - so the guard honors
//      the "dies WITHOUT reporting" clause while still guaranteeing every parked spawn ends
//      with a result and the run can never hang.
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
// `error` is the courier's own out-of-band channel: if `rigger step` itself fails, the
// courier reports the message here rather than fabricating a wave.
const STEP = {
  type: 'object',
  additionalProperties: true,
  required: ['wave', 'done'],
  properties: {
    wave: {
      type: 'array',
      items: {
        type: 'object',
        additionalProperties: true,
        required: ['id', 'unit', 'stage', 'prompt'],
        properties: {
          id: { type: 'string' },
          unit: { type: 'string' },
          stage: { type: 'string' },
          prompt: { type: 'string' },
          system_prompt: { type: 'string' },
          model: { type: 'string' },
          dir: { type: 'string' },
          tools: { type: 'array', items: { type: 'string' } },
          blast_radius: { type: 'array', items: { type: 'string' } },
        },
      },
    },
    done: { type: 'boolean' },
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
// GUARDED (check-then-record) so it never overwrites a result the worker already reported.
//
// `fatal` is a shared sink: if the death courier ITSELF dies, we can no longer guarantee a
// result was recorded for this spawn, so we push it here and the loop stops loudly after the
// wave drains rather than swallowing the failure (which would hang the run on resume).
async function runWorker(req, fatal) {
  const ph = phaseOf(req)
  const persona = req.system_prompt ? `${req.system_prompt}\n\n---\n\n` : ''
  const workdir = req.dir
    ? `Do all your file edits, cargo, and any git commit inside your isolated worktree ${req.dir} (the conductor assigned it and owns its lifecycle; run \`rigger ...\` commands from ${REPO}).`
    : `Work in ${REPO}.`
  const prompt =
    `${persona}${req.prompt}\n\n` +
    `--- rigger driver instructions ---\n` +
    `${workdir}\n` +
    `The rigger context tools your task refers to (rigger_emit, rigger_peers) are available here as the CLI commands \`rigger emit <Type> '<json>'\` and \`rigger peers <file>...\`, run from ${REPO}.\n` +
    `When you finish, SELF-REPORT your result by running, from ${REPO}:\n` +
    `  rigger result ${req.id} "<your result: a one-line summary, or your full verdict/findings>"\n` +
    `(pipe multi-line output via stdin instead, e.g. \`rigger result ${req.id}\` reading a heredoc). ` +
    `If you cannot complete the task, report the failure instead: \`rigger result ${req.id} --error "<why it failed>"\` (the message must be non-empty). ` +
    `--error means YOU were unable to perform your task (blocked, crashed, missing tools) - NEVER a negative conclusion: a reviewer whose verdict is REJECT, or a gate that found failures, COMPLETED its task and reports that verdict/finding as its NORMAL result (an --error replays as a dead worker and aborts the run, not as your verdict). ` +
    `Reporting your result is mandatory - the run cannot advance past this spawn until you do.`

  try {
    await agent(prompt, { phase: ph, model: req.model || undefined, label: req.id })
  } catch (e) {
    // The worker's agent() REJECTED (max turns, a crash, an execution error). That rejection
    // does NOT prove it died before reporting - a worker (or a reviewer that already emitted
    // an approve verdict) can self-report and THEN run on to max-turns. So record its failure
    // ON ITS BEHALF as CHECK-THEN-RECORD: `rigger reported <id>` exits 0 iff the spawn already
    // has a result, so `... || rigger result <id> --error <why>` writes the failure ONLY when
    // the worker truly died WITHOUT reporting. `rigger result` is last-write-wins, so an
    // unconditional --error would CLOBBER a self-reported success/approve and force-fail an
    // approved unit on the next replay - the guard prevents exactly that while still ensuring
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
        `You are a rigger COURIER. The worker for spawn ${req.id} died. Record its failure ON ITS BEHALF, but ONLY if it did not already self-report - a result the worker already recorded must NEVER be overwritten. Run EXACTLY this, from ${REPO}, using Bash (ONE command, keep the \`||\`):\n` +
          `  cd ${REPO} && rigger reported ${req.id} || rigger result ${req.id} --error "worker ${req.id} died without reporting: ${msg}"\n` +
          `\`rigger reported ${req.id}\` exits 0 when the spawn ALREADY has a result - then the \`||\` SKIPS the --error and nothing is overwritten. It exits non-zero only when there is no result yet, in which case the failure is recorded (the message is non-empty by construction). Confirm the whole command exited 0; report nothing else.`,
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
        `It prints ONE line of JSON on stdout: {"wave":[...],"done":<bool>}. Return that JSON object EXACTLY as printed, INLINE and IN FULL, in your structured output - no matter how large it is. NEVER write it to a file, return a path, a reference, a summary, or a truncation: the driver can only read your returned JSON, so anything but the verbatim object (all wave items, all their fields) LOSES the wave and stalls the run. Do not drop fields or run anything else. ` +
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

  // 3. Stop at the conductor's fixpoint (every parked spawn has a result and nothing new was
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
