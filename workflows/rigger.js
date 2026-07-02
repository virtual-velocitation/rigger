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
//      its behalf by a courier agent running `rigger result <id> --error <why>`, so the
//      conductor always sees a result for every parked spawn and the run can never hang.
//   4. LOOPS until a step reports `done`.
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
const BASE = A.base || 'origin/main'

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
// contract. If the agent REJECTS (died without reporting), a courier records the failure on
// its behalf so the conductor's replay still sees a result for this spawn.
async function runWorker(req) {
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
    `Reporting your result is mandatory - the run cannot advance past this spawn until you do.`

  try {
    await agent(prompt, { phase: ph, model: req.model || undefined, label: req.id })
  } catch (e) {
    // The worker died without self-reporting (max turns, a crash, an execution error). Record
    // its failure ON ITS BEHALF via a courier so the conductor's replay driver sees a result
    // for this parked spawn and the run advances (into remediation or escalation) instead of
    // hanging forever waiting for a result that will never come. The --error message must be
    // non-empty (a blank error would replay AS a success and silently swallow the failure).
    // Neutralize shell metacharacters (`"`, backtick, `$`) so the message can never break out
    // of - or trigger substitution inside - the double-quoted --error arg in the courier command.
    const why = (e && e.message ? e.message : String(e))
      .replace(/["`$\\]/g, "'")
      .replace(/\s+/g, ' ')
      .trim()
      .slice(0, 400)
    const msg = why || 'the worker agent exited without producing a result'
    log(`worker ${req.id} died without reporting: ${msg.slice(0, 80)} - recording its failure on its behalf`)
    await agent(
      `You are a rigger COURIER. The worker for spawn ${req.id} died without self-reporting its result, so record its failure on its behalf. Run EXACTLY this, from ${REPO}, using Bash:\n` +
        `  cd ${REPO} && rigger result ${req.id} --error "worker ${req.id} died without reporting: ${msg}"\n` +
        `The error message is non-empty by construction. Confirm the command exited 0; report nothing else.`,
      { phase: ph, model: 'haiku', label: `report-death:${req.id}` },
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
// (its spawn-budget breaker and per-unit retry bound), so this loop needs no cap of its own;
// the empty-wave-but-not-done guard below is a belt-and-braces stop for the anomalous case of
// a worker that resolved without self-reporting (nothing new to park, yet not a fixpoint).
let waves = 0
for (;;) {
  // 1. Courier: advance the conductor one frontier and return the wave verbatim. `rigger step`
  //    sets up/reuses the run branch (via --base) before parking anything, then prints one line
  //    of JSON. On the FIRST step the run branch is anchored and the spec is decomposed; on
  //    later steps the conductor replays past the results workers reported and parks the next
  //    frontier. If `rigger step` errors, the courier returns it in `error` (not a faked wave).
  const step = await agent(
    `You are a rigger COURIER. Advance the run one frontier and return the wave, verbatim. Run EXACTLY this, from ${REPO}, using Bash:\n` +
      `  cd ${REPO} && rigger step --spec ${SPEC} --base ${BASE}\n` +
      `It prints ONE line of JSON on stdout: {"wave":[...],"done":<bool>}. Return that JSON object EXACTLY as printed - do not summarize it, drop fields, or run anything else. ` +
      `If the command prints no JSON or exits non-zero, return {"wave":[],"done":true,"error":"<the stderr / failure message>"} so the loop stops cleanly and the error is visible.`,
    { phase: 'Plan', model: 'haiku', schema: STEP, label: `step#${waves + 1}` },
  )

  if (step.error) {
    log(`rigger step failed: ${step.error} - stopping the driver loop`)
    break
  }

  // 2. Spawn the wave natively in parallel; each worker in its own per-unit progress group.
  const wave = step.wave || []
  if (wave.length > 0) {
    waves += 1
    log(`wave ${waves}: spawning ${wave.length} agent(s) in parallel: ${wave.map((r) => r.id).join(', ')}`)
    await parallel(wave.map((req) => () => runWorker(req)))
  }

  // 3. Stop at the conductor's fixpoint (every parked spawn has a result and nothing new was
  //    parked). A non-empty wave always implies done === false, so we drain it first (above),
  //    then re-check on the next iteration.
  if (step.done) {
    log(`run complete: the conductor reached a fixpoint after ${waves} wave(s)`)
    break
  }
  // An empty wave that is NOT done means a prior worker resolved without self-reporting: the
  // conductor has an unanswered spawn but there is nothing new for us to run, so stepping again
  // would spin. Stop with a clear log rather than loop forever.
  if (wave.length === 0) {
    log('rigger step parked no new wave yet is not done (a worker likely resolved without self-reporting); stopping to avoid a spin')
    break
  }
}

return { waves }
