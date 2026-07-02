// meta MUST be a pure literal: the Workflow runtime extracts it statically (before
// the workflow body ever runs), so it cannot contain computed values or interpolation.
// That is why meta.phases stays the FIXED stage set below and does NOT enumerate the
// per-unit phases (u<id>:Build, u<id>:Review, u<id>:Integrate) - the unit ids come from
// the planner at runtime and are unknowable at static-extraction time. The per-unit
// distinction that makes the /workflows display match execution is carried entirely by
// the runtime `opts.phase` progress-group strings assigned inside buildUnit; meta.phases
// remains the stable up-front announcement of the stages a unit passes through. See the
// buildUnit `PH` helper below.
export const meta = {
  name: 'rigger',
  description:
    'The rigger dev-loop as a native workflow: decompose a spec into a unit DAG, then for each unit implement -> cargo gates -> three-tier adversarial review -> integrate, with bounded remediation. Agents are grounded via `rigger ground`; decisions and review findings persist in the shared context graph via `rigger emit` and are read back via `rigger peers`. Build/Review/Integrate run PER UNIT (a unit is fully integrated before the next starts); the /workflows progress groups are labelled per unit (u<id>:Build, ...) at runtime via opts.phase - meta.phases can only name the fixed stages because it must be a static literal.',
  phases: [
    { title: 'Plan', detail: 'decompose the spec into a unit DAG (one global pass)' },
    { title: 'Build', detail: 'per-unit implement + cargo gates; runs as opts.phase "u<id>:Build" per unit' },
    { title: 'Review', detail: 'per-unit three-tier adversarial review (lenses, adversary, adjudicator); runs as "u<id>:Review"' },
    { title: 'Integrate', detail: 'per-unit merge of the approved unit onto the run branch; runs as "u<id>:Integrate"' },
  ],
}

// args: a spec path string, or { repo, spec, maxRetries, base }.
// rigger's shared context store lives in <repo>/.rigger - every `rigger ...` command and the
// run-branch git run in REPO; code edits, cargo gates, and the per-unit commit run in the worktree.
// The grounding index is reindexed only AFTER a unit merges into REPO (in the Integrate step),
// never from the pre-merge worktree, so it never embeds stale (unmerged) code.
// `base` (default origin/main) is the ref the run branch is created FROM when it does not exist
// yet; if base is unresolvable the run branch is created off HEAD. An EXISTING run branch is
// REUSED, never reset, so a re-invoked/resumed run continues from (builds on) its accumulated
// work instead of orphaning already-integrated units. This mirrors `rigger step --base`
// (Worktree::ensure_run_branch): same reuse-else-create-off-base-else-create-off-HEAD contract.
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
const MAX = A.maxRetries || 6
const RUN = 'rigger-run'
const BASE = A.base || 'origin/main'
const LENSES = [
  'technical correctness: it compiles, the logic is right, errors are handled, the tests genuinely exercise the behavior, idiomatic Rust',
  'clean architecture: one mutation authority per domain, correct dependency direction, DRY (no duplicated literals or contracts), no new parallel abstraction where one already exists',
]

const PLAN = { type: 'object', additionalProperties: false, required: ['units'], properties: { units: { type: 'array', items: { type: 'object', additionalProperties: false, required: ['id', 'criterion'], properties: { id: { type: 'string' }, criterion: { type: 'string' }, files: { type: 'array', items: { type: 'string' } } } } } } }
const IMPL = { type: 'object', additionalProperties: false, required: ['summary', 'files'], properties: { summary: { type: 'string' }, files: { type: 'array', items: { type: 'string' } } } }
const GATE = { type: 'object', additionalProperties: false, required: ['pass', 'evidence'], properties: { pass: { type: 'boolean' }, evidence: { type: 'string' } } }
const VERDICT = { type: 'object', additionalProperties: false, required: ['approved', 'reason'], properties: { approved: { type: 'boolean' }, reason: { type: 'string' } } }

phase('Plan')
await agent(
  `Prepare the rigger run branch in the repo ${REPO} (use Bash). Run: \`git -C ${REPO} fetch origin 2>/dev/null; git -C ${REPO} worktree prune; rm -rf /tmp/rigger-wf-*; if git -C ${REPO} rev-parse --verify --quiet refs/heads/${RUN}; then git -C ${REPO} checkout ${RUN}; else git -C ${REPO} checkout -B ${RUN} ${BASE} 2>/dev/null || git -C ${REPO} checkout -B ${RUN}; fi\`. This REUSES an existing ${RUN} branch (never resetting it, so a resumed run keeps its already-integrated units), and only when ${RUN} does not exist yet creates it off ${BASE} - falling back to the current HEAD if ${BASE} is unresolvable. Mirrors \`rigger step --base\` (Worktree::ensure_run_branch). Confirm the branch is checked out and the working tree is clean.`,
  { phase: 'Plan', model: 'sonnet', label: 'setup run branch' },
)

const plan = await agent(
  `You are the rigger PLANNER for the repo ${REPO}. Read the spec at ${REPO}/${SPEC}. Ground yourself first: \`cd ${REPO} && rigger ground "$(head -1 ${REPO}/${SPEC})"\` and read the surfaced files to understand the existing code. Decompose the spec into a DAG of SMALL, independently-implementable units - ONE per acceptance criterion (the "- [ ]" Done-when lines) - in dependency order, each with a stable short id, the exact criterion text, and the files it touches. Record each unit in the shared store: \`cd ${REPO} && rigger emit UnitProposed '{"id":"<id>","summary":"<criterion>","governs":["<file>"]}'\`.`,
  { phase: 'Plan', model: 'opus', schema: PLAN, label: 'planner' },
)
log(`planner decomposed ${SPEC} into ${plan.units.length} units: ${plan.units.map((u) => u.id).join(', ')}`)

async function buildUnit(unit) {
  const WT = `/tmp/rigger-wf-${unit.id}`
  const BR = `rigger/u/${unit.id}`
  const files = (unit.files || []).join(' ')
  // Per-unit progress-group labels for the /workflows display. Each unit runs its whole
  // Build -> Review -> Integrate lifecycle to completion before the next unit starts (see
  // the sequential loop below), so labelling the progress groups per unit makes the display
  // match execution instead of implying a false global "all units Build, then all Review"
  // order. These are runtime strings (meta.phases, a static literal, cannot carry them) that
  // still map back to the fixed meta.phases stages via the `<stage>` suffix after the colon.
  const PH = (stage) => `u${unit.id}:${stage}`
  let prior = ''
  for (let a = 1; a <= MAX; a++) {
    const impl = await agent(
      `You are the rigger IMPLEMENTER (an expert Rust engineer) for repo ${REPO}. RULES: run every \`rigger ...\` command and the run-branch git from ${REPO} (the shared context store is ${REPO}/.rigger); do your code edits, cargo, and the unit commit inside the worktree ${WT}. Set up your worktree: if attempt ${a} is 1, \`git -C ${REPO} worktree add ${WT} -B ${BR} ${RUN}\`; otherwise reuse ${WT}. Ground: \`cd ${REPO} && rigger ground "${unit.criterion}" && rigger peers ${files}\` (do not silently contradict peers' decisions). ${a === 1 ? `Record the start: \`cd ${REPO} && rigger emit UnitStarted '{"id":"${unit.id}"}'\`.` : ''} Implement the unit FULLY, with tests, in ${WT}: "${unit.criterion}". ${prior} Record each significant design decision the moment you make it: \`cd ${REPO} && rigger emit DecisionMade '{"id":"<short>","summary":"<one line>","governs":["<file>"]}'\`. Then \`cd ${WT} && cargo fmt && git add -A && git commit -m "${unit.id} a${a}"\`. The change now lives on branch ${BR} in the worktree ${WT}, NOT yet in ${REPO} (which is still on ${RUN}), so do NOT reindex here - reindexing ${REPO} now would embed the pre-merge (stale) tree. The shared grounding index is refreshed AFTER the merge lands, in the Integrate step below. Idiomatic Rust, no placeholders, no TODO stubs. Return a one-line summary and the files you changed.`,
      { phase: PH('Build'), model: 'opus', schema: IMPL, label: `impl:${unit.id} a${a}` },
    )
    const gate = await agent(
      `Run the rigger gates in the worktree and report. \`cd ${WT} && export PATH="$HOME/.cargo/bin:$PATH" && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build && cargo test\`. Set pass=true ONLY if every command succeeds; otherwise pass=false with a compact evidence summary of the key failing lines (not the whole log).`,
      { phase: PH('Build'), model: 'sonnet', schema: GATE, label: `gate:${unit.id} a${a}` },
    )
    if (!gate.pass) {
      prior = `Your previous attempt FAILED the gates:\n${gate.evidence}\nFix exactly these and keep everything else green.`
      log(`${unit.id} a${a}: gates failed`)
      continue
    }
    // tier 1: parallel expert lenses - each grounds, reads peers from the shared store, emits findings to it
    await parallel(
      LENSES.map((L, i) => () =>
        agent(
          `You are a rigger review LENS - ${L}. Review ONLY the diff for unit ${unit.id}: \`cd ${WT} && git diff ${RUN}...HEAD\`. Criterion: "${unit.criterion}". Read peers from the shared store first so you do not duplicate: \`cd ${REPO} && rigger peers ${files}\`. Record each REAL finding (a genuine defect against your lens, not a style nitpick) to the shared store: \`cd ${REPO} && rigger emit ReviewFinding '{"id":"<short>","summary":"<one line>","about":["<file>"]}'\`. If it is clean through your lens, emit nothing.`,
          { phase: PH('Review'), model: 'opus', label: `lens${i + 1}:${unit.id}` },
        ),
      ),
    )
    // tier 2: adversary - reads the lenses' findings, refutes + finds what they missed
    await agent(
      `You are the rigger ADVERSARY for unit ${unit.id}. Read the lenses' findings from the shared store: \`cd ${REPO} && rigger peers ${files}\`. Inspect the diff: \`cd ${WT} && git diff ${RUN}...HEAD\` and the surrounding code. Refute any weak or overreaching lens finding, AND find the real defects the lenses MISSED. Record your findings: \`cd ${REPO} && rigger emit ReviewFinding '{"id":"adv-<short>","summary":"<one line>","about":["<file>"]}'\`.`,
      { phase: PH('Review'), model: 'opus', label: `adversary:${unit.id}` },
    )
    // tier 3: adjudicator - reads ALL findings, gates the verdict
    const verdict = await agent(
      `You are the rigger ADJUDICATOR - the neutral final judge for unit ${unit.id} (criterion: "${unit.criterion}"). The gates already passed (it builds and tests). Read every finding from the lenses and the adversary: \`cd ${REPO} && rigger peers ${files}\`, and inspect the diff: \`cd ${WT} && git diff ${RUN}...HEAD\`. Weigh them. APPROVE if and only if the code correctly and completely implements the criterion with NO real correctness or architecture defect remaining; a genuine blocker is a REJECT with the specific reason and what must change. Pure style nitpicks are NOT blockers. Record your verdict: \`cd ${REPO} && rigger emit ReviewVerdict '{"id":"adj-${unit.id}-${a}","summary":"<approve or reject>: <reason>","about":["${unit.id}"]}'\`. If you reject, also \`cd ${REPO} && rigger emit UnitFailed '{"id":"${unit.id}","attempt":${a}}'\`.`,
      { phase: PH('Review'), model: 'opus', schema: VERDICT, label: `adjudicator:${unit.id} a${a}` },
    )
    if (verdict.approved) {
      await agent(
        `Integrate unit ${unit.id}: \`git -C ${REPO} checkout ${RUN} && git -C ${REPO} merge --no-ff ${BR} -m "integrate ${unit.id}" && git -C ${REPO} worktree remove --force ${WT}\`. The unit's code is now MERGED into ${REPO} on ${RUN}, so pre-warm the shared grounding index against the just-integrated (not pre-merge) tree - this re-embeds only the changed files so the next unit and its review tier ground on the accepted code: \`cd ${REPO} && rigger reindex ${files || '<the repo-relative files this unit changed>'}\` (incremental; a no-op for the grep/nop grounders, and the next \`ground\` auto-freshens anything it misses). Then record it: \`cd ${REPO} && rigger emit UnitIntegrated '{"id":"${unit.id}"}'\`. Confirm ${RUN} now contains the unit and still builds.`,
        { phase: PH('Integrate'), model: 'sonnet', label: `integrate:${unit.id}` },
      )
      log(`${unit.id} INTEGRATED on attempt ${a}`)
      return { unit: unit.id, integrated: true, attempts: a }
    }
    prior = `Your previous attempt was REJECTED by review: ${verdict.reason}. Read the full findings with \`cd ${REPO} && rigger peers ${files}\` and address ALL of them in the same worktree ${WT}, then re-commit.`
    log(`${unit.id} a${a}: rejected - ${(verdict.reason || '').slice(0, 70)}`)
  }
  await agent(
    `Record the escalation - the implementer could not satisfy the strict review for unit ${unit.id} in ${MAX} attempts; its work is left on branch ${BR} for a human: \`cd ${REPO} && rigger emit UnitEscalated '{"id":"${unit.id}"}'\`.`,
    { phase: PH('Review'), model: 'haiku', label: `escalate:${unit.id}` },
  )
  log(`${unit.id} ESCALATED after ${MAX} attempts (left on ${BR})`)
  return { unit: unit.id, escalated: true }
}

// The planner returns units in dependency order; iterate sequentially so integrate never races.
// No global `phase('Build')` marker here: each unit drives its OWN Build/Review/Integrate progress
// groups via the per-unit opts.phase strings (u<id>:Build, ...) inside buildUnit, so the /workflows
// display shows each unit's lifecycle distinctly instead of one global Build group that would falsely
// imply every unit builds together before any reviews. meta.phases (a static literal) still names the
// fixed stages up front; the per-unit labels are the runtime refinement of those same stages.
const results = []
for (const unit of plan.units) {
  results.push(await buildUnit(unit))
}
return {
  integrated: results.filter((r) => r.integrated).map((r) => r.unit),
  escalated: results.filter((r) => r.escalated).map((r) => r.unit),
  results,
}
