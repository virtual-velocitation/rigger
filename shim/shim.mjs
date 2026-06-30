#!/usr/bin/env node
// rigger workflow driver - a standalone Node program that drives the Rust
// conductor's agents.
//
// A Claude Code Workflow script CANNOT call MCP tools, so this is NOT a Workflow
// script: it is a normal Node process. It uses two SDKs:
//
//   - @modelcontextprotocol/sdk  -> an MCP *client* that connects to `rigger serve`
//     (the Rust conductor's MCP server) over stdio. This is the one shared
//     connection through which every spawn is pulled (rigger_next), every result
//     is reported (rigger_result), and every agent's decisions/findings are
//     recorded (rigger_emit) or read (rigger_peers).
//
//   - @anthropic-ai/claude-agent-sdk -> runs each agent via query(). Each agent is
//     given an *in-process* SDK MCP server (createSdkMcpServer + tool) that exposes
//     rigger_emit and rigger_peers; those tool handlers PROXY to the shim's single
//     MCP client connection above. So when an agent emits a decision it goes LIVE
//     through the one shared connection to `rigger serve` - there is never a second
//     connection to the single-stdio server.
//
// The loop:  rigger_next -> (empty id ? exit) -> rigger_peers blast-radius
// injection -> run the agent via query() with the proxy server -> rigger_result.
//
// Every rigger tool reply is a tools/call result whose payload is under
// `structuredContent` (the Rust server wraps it there, see src/mcpserver.rs); this
// driver unwraps it on every call.
//
// This file is split into a pure driver (runWorkflow / the proxy + loop), wired
// against an injected MCP client and an injected runAgent, and a main() that
// constructs the real client (stdio transport spawning `rigger serve`) and the
// real runAgent (the Agent SDK). The seam is what makes the loop testable against
// a mock MCP server with a stubbed agent (see shim.test.mjs).

import { Client } from '@modelcontextprotocol/sdk/client/index.js'
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js'
import { query, tool, createSdkMcpServer } from '@anthropic-ai/claude-agent-sdk'
import { z } from 'zod'

export const meta = {
  name: 'rigger-shim',
  description:
    'Drive the rigger conductor: connect an MCP client to `rigger serve`, pull spawns, run agents via the Agent SDK with an in-process proxy server, report results',
}

// sleep resolves after `ms` milliseconds - used to back off between rigger_next
// polls while the conductor is still running but has nothing queued yet.
function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

// debug logs to stderr when RIGGER_SHIM_DEBUG is set, so a run can be traced
// (the spawn fields, the assembled prompt, the agent's output) without polluting
// the stdout JSON-RPC channel the MCP client speaks on.
function debug(msg) {
  if (process.env.RIGGER_SHIM_DEBUG) {
    process.stderr.write(`rigger-shim[debug]: ${msg}\n`)
  }
}

// ---------------------------------------------------------------------------
// Usage-limit pause-and-resume.
//
// When a spawn hits Claude's usage limit, query() does NOT die from a bug - the
// account is simply rate-limited until a reset time. Treating that as a stage
// failure (rigger_result.error) would mark a perfectly good unit as broken. So
// instead the shim recognizes the limit error, WAITS until the named reset time,
// and RETRIES the spawn transparently - the conductor's spawn just takes longer
// and eventually succeeds.
//
// This mirrors tank_game's review-and-remediate.js `resilientAgent`, which
// sleep-and-retries in a capped loop on a limit hit. tank_game detects the limit
// by a `null` return and only ever does a FIXED-interval sleep (its sandbox can't
// read the wall clock). Here we have a real clock, so we go one step further:
// PARSE the reset time out of the message and wait until exactly then, falling
// back to tank_game's fixed-interval behavior when the message can't be parsed.
//
// The parse and the wait are factored into small pure functions so they are unit-
// testable, and the clock/sleep is an injectable seam (`now` / `sleepUntil`) so
// the tests run instantly instead of actually sleeping.

// Default fixed retry interval (15 min) and retry cap (24 attempts ~= 6h), matching
// tank_game's `resilientAgent` defaults. Used as the fallback when a reset time can't
// be parsed, and as the bound on how many limit hits we ride out before giving up.
export const DEFAULT_RETRY_INTERVAL_MS = 15 * 60 * 1000
export const DEFAULT_MAX_LIMIT_RETRIES = 24

// Phrases that mark a usage/rate-limit error (as opposed to a real failure). Claude
// surfaces several wordings - the weekly limit, the rolling 5-hour limit, and the
// generic "usage limit reached" - so match any of them, case-insensitively.
const USAGE_LIMIT_PATTERNS = [
  /usage limit reached/i,
  /hit your (?:weekly|5-?hour|five-?hour) limit/i,
  /weekly limit/i,
  /\b5-?hour limit\b/i,
  /rate.?limit(?:ed|ing)?/i,
  /you've (?:reached|hit) (?:your|the) limit/i,
]

// isUsageLimitError reports whether an error message is a usage/rate-limit notice
// (so the loop should PAUSE-and-RESUME) rather than a genuine stage failure. Pure.
export function isUsageLimitError(message) {
  if (typeof message !== 'string' || message.length === 0) return false
  return USAGE_LIMIT_PATTERNS.some((re) => re.test(message))
}

// Map a few common timezone abbreviations to IANA zone ids so a message that says
// "PST"/"CT"/etc. instead of "(America/Chicago)" still resolves. IANA zone names in
// the message are used directly (see parseResetTime); this only covers abbreviations.
const TZ_ABBREV = {
  ET: 'America/New_York',
  EST: 'America/New_York',
  EDT: 'America/New_York',
  CT: 'America/Chicago',
  CST: 'America/Chicago',
  CDT: 'America/Chicago',
  MT: 'America/Denver',
  MST: 'America/Denver',
  MDT: 'America/Denver',
  PT: 'America/Los_Angeles',
  PST: 'America/Los_Angeles',
  PDT: 'America/Los_Angeles',
  UTC: 'UTC',
  GMT: 'UTC',
}

// tzOffsetMs returns the offset (in ms) of an IANA `timeZone` from UTC at the instant
// `atUtcMs`, i.e. (localWallClock - UTC). Positive east of UTC. Uses Intl (built into
// Node) so no tz database dependency is needed. Throws on an unknown zone.
function tzOffsetMs(timeZone, atUtcMs) {
  const dtf = new Intl.DateTimeFormat('en-US', {
    timeZone,
    hour12: false,
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  })
  const parts = dtf.formatToParts(new Date(atUtcMs))
  const f = {}
  for (const p of parts) f[p.type] = p.value
  // Intl renders midnight as "24" in some engines; normalize to "00".
  const hour = f.hour === '24' ? '00' : f.hour
  // The wall-clock time in `timeZone`, read back as if it were UTC, minus the real
  // UTC instant, IS the zone's offset at that instant (DST-correct, since we sampled
  // the zone AT that instant).
  const asUtc = Date.UTC(
    Number(f.year),
    Number(f.month) - 1,
    Number(f.day),
    Number(hour),
    Number(f.minute),
    Number(f.second),
  )
  return asUtc - atUtcMs
}

// parseResetTime extracts the reset time from a usage-limit message and returns the
// NEXT future occurrence of that wall-clock time, in the named timezone, as a Date -
// or null when the message has no parseable reset time (the caller then falls back to
// a fixed interval). Pure: `nowMs` is the injected current instant.
//
// Handles the canonical Claude wording, e.g.
//   "You've hit your weekly limit · resets 5am (America/Chicago)"
//   "... resets at 11:30pm (PST)"
// The hour:minute and am/pm are parsed; the zone may be an IANA id in parens or a
// known abbreviation. If no zone is named, the local zone is used.
export function parseResetTime(message, nowMs) {
  if (typeof message !== 'string') return null
  // resets [at] H[:MM][am|pm]  -- the time-of-day the limit clears.
  const timeMatch = message.match(/resets?\s+(?:at\s+)?(\d{1,2})(?::(\d{2}))?\s*([ap]\.?m\.?)?/i)
  if (!timeMatch) return null

  let hour = Number(timeMatch[1])
  const minute = timeMatch[2] !== undefined ? Number(timeMatch[2]) : 0
  const ampm = timeMatch[3] ? timeMatch[3].toLowerCase().replace(/\./g, '') : ''
  if (hour < 0 || hour > 23 || minute < 0 || minute > 59) return null
  if (ampm === 'pm' && hour < 12) hour += 12
  if (ampm === 'am' && hour === 12) hour = 0

  // Timezone: an IANA id "(America/Chicago)" or a known abbreviation "(PST)" / "PST".
  let timeZone = null
  const ianaMatch = message.match(/\(([A-Za-z]+(?:\/[A-Za-z_]+)+)\)/)
  if (ianaMatch) {
    timeZone = ianaMatch[1]
  } else {
    const abbrevMatch = message.match(/\(([A-Za-z]{2,4})\)|\b([A-Z]{2,4})\b/)
    const abbrev = abbrevMatch && (abbrevMatch[1] || abbrevMatch[2])
    if (abbrev && TZ_ABBREV[abbrev.toUpperCase()]) {
      timeZone = TZ_ABBREV[abbrev.toUpperCase()]
    }
  }

  const now = new Date(nowMs)
  // Build the candidate reset instant for "today" in the target zone, then roll
  // forward a day until it is strictly in the future.
  const buildCandidate = (dayOffset) => {
    if (!timeZone) {
      // No zone named: interpret the wall-clock time in the host's local zone.
      const d = new Date(nowMs)
      d.setDate(d.getDate() + dayOffset)
      d.setHours(hour, minute, 0, 0)
      return d.getTime()
    }
    // With a zone: figure out today's date AS SEEN in that zone, build the wall-clock
    // instant, and convert back to a UTC instant using the zone's offset at that time.
    const zoneParts = new Intl.DateTimeFormat('en-CA', {
      timeZone,
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
    }).formatToParts(now)
    const z = {}
    for (const p of zoneParts) z[p.type] = p.value
    // A first guess at the UTC instant, assuming the offset near `now` (good enough to
    // then re-sample the offset AT the candidate for DST correctness).
    let utcGuess = Date.UTC(Number(z.year), Number(z.month) - 1, Number(z.day) + dayOffset, hour, minute, 0)
    const off = tzOffsetMs(timeZone, utcGuess)
    return utcGuess - off
  }

  let candidate
  for (let dayOffset = 0; dayOffset <= 8; dayOffset++) {
    candidate = buildCandidate(dayOffset)
    if (Number.isFinite(candidate) && candidate > nowMs) break
  }
  if (!Number.isFinite(candidate) || candidate <= nowMs) return null
  return new Date(candidate)
}

// computeRetryDelayMs decides how long to wait after a usage-limit hit before retrying.
// It tries to parse a concrete reset time from the message; on success the wait is the
// gap from now to that reset (plus a small buffer so we don't retry a hair too early);
// when the message can't be parsed it falls back to a fixed interval (tank_game's
// behavior). Capped so a far-future / mis-parsed reset can't produce an absurd sleep.
// Pure: takes `nowMs` and the options. Returns a non-negative ms duration.
export function computeRetryDelayMs(message, nowMs, opts = {}) {
  const fallbackMs = opts.intervalMs ?? DEFAULT_RETRY_INTERVAL_MS
  // Buffer added past the named reset so we retry just AFTER the window clears.
  const bufferMs = opts.bufferMs ?? 5000
  // Never sleep longer than this in one go - a guard against a mis-parse landing far in
  // the future. The default (8 days + a day of slack) comfortably covers a real weekly
  // reset (parseResetTime only ever rolls up to 8 days ahead), so the common case waits
  // the whole way in ONE sleep; only an absurd parse is clamped, after which we wake,
  // find the limit still set, and wait again.
  const maxWaitMs = opts.maxWaitMs ?? 9 * 24 * 60 * 60 * 1000
  const reset = parseResetTime(message, nowMs)
  let waitMs
  if (reset) {
    waitMs = reset.getTime() - nowMs + bufferMs
  } else {
    waitMs = fallbackMs
  }
  if (!Number.isFinite(waitMs) || waitMs < 0) waitMs = fallbackMs
  return Math.min(waitMs, maxWaitMs)
}

// withLimitResume wraps a runAgent so a usage-limit error PAUSES-and-RESUMES instead of
// failing the spawn. On each call it runs the inner agent; if that throws a usage-limit
// error it computes the wait (parsed reset time, else a fixed interval), sleeps until
// then via the injected `sleepUntil`, and retries - looping until the agent succeeds or
// the retry cap is reached. A non-limit error is re-thrown immediately (a real failure).
//
// Injectable seam (for instant tests): `opts.now()` returns the current ms, and
// `opts.sleepUntil(targetMs, currentMs)` resolves at the target. Both default to the
// real clock + setTimeout. `opts.maxRetries` caps the number of limit pauses.
export function withLimitResume(runAgent, opts = {}) {
  const now = opts.now ?? Date.now
  const sleepUntil =
    opts.sleepUntil ??
    ((targetMs, currentMs) => sleep(Math.max(0, targetMs - currentMs)))
  const maxRetries = opts.maxRetries ?? DEFAULT_MAX_LIMIT_RETRIES
  const log = opts.log ?? debug

  return async function limitAwareAgent(spawn) {
    let attempt = 0
    for (;;) {
      try {
        return await runAgent(spawn)
      } catch (e) {
        const message = e?.message || String(e)
        if (!isUsageLimitError(message)) throw e
        if (attempt >= maxRetries) {
          throw new Error(
            `usage limit not cleared after ${maxRetries} retries; giving up. last: ${message}`,
          )
        }
        attempt += 1
        const currentMs = now()
        const waitMs = computeRetryDelayMs(message, currentMs, opts)
        const targetMs = currentMs + waitMs
        log(
          `usage limit hit (${message.slice(0, 80)}); pausing ~${Math.round(waitMs / 1000)}s ` +
            `then retry ${attempt}/${maxRetries}`,
        )
        await sleepUntil(targetMs, currentMs)
      }
    }
  }
}

// unwrap returns the structured payload of a tools/call result. The Rust MCP
// server replies with both a human-readable `content[].text` and the machine
// payload under `structuredContent`; we want the latter. If a server ever omits
// it (it shouldn't), fall back to parsing the text block so a missing wrapper is
// a soft failure, not a crash.
export function unwrap(result) {
  if (result && typeof result === 'object' && result.structuredContent !== undefined) {
    return result.structuredContent
  }
  const text = result?.content?.find?.((c) => c?.type === 'text')?.text
  if (typeof text === 'string') {
    try {
      return JSON.parse(text)
    } catch {
      return {}
    }
  }
  return {}
}

// call invokes a rigger MCP tool over the shared client connection and unwraps
// the structuredContent payload. A tool that returns an MCP error (isError) is
// surfaced as a thrown Error so the loop fails loudly instead of silently acting
// on an empty payload.
async function call(client, name, args) {
  const result = await client.callTool({ name, arguments: args || {} })
  if (result?.isError) {
    const text = result?.content?.find?.((c) => c?.type === 'text')?.text
    throw new Error(`rigger tool ${name} failed: ${text || 'unknown error'}`)
  }
  return unwrap(result)
}

// peersRefresh fetches the peer decisions scoped to a spawn's blast-radius and
// renders them as a context-refresh block to prepend to the agent's prompt. This
// is the tool-boundary injection (§5.3): the agent reads it before its first
// action. An empty blast-radius means "no scope" and returns every decision; no
// decisions yields "".
export function renderPeers(peers) {
  const decisions = (peers && peers.decisions) || []
  if (decisions.length === 0) return ''
  const lines = decisions.map((d) => {
    const governs = (d.governs || []).join(', ')
    const scope = governs ? ` [governs: ${governs}]` : ''
    return `- ${d.id}: ${d.summary}${scope}`
  })
  return [
    'PEERS CONTEXT REFRESH - concurrent agents have already decided the following',
    'about files in your blast-radius. Do not contradict them; supersede explicitly',
    'if you must. Re-check the rigger_peers tool between your own actions to stay',
    'aware of decisions made while you work.',
    ...lines,
    '',
  ].join('\n')
}

// buildProxyServer returns an in-process SDK MCP server exposing rigger_emit and
// rigger_peers, whose handlers proxy to the shared MCP client `call`. This is how
// each agent emits/peers LIVE through the one shared connection to `rigger serve`
// rather than opening its own (the server speaks a single stdio pair, so a second
// connection is impossible). Returned as an mcpServers entry for query().
export function buildProxyServer(client) {
  const emitTool = tool(
    'rigger_emit',
    'Record a decision or review finding on the shared event log, live, so other ' +
      'agents see it immediately. type is "DecisionMade", "ReviewFinding", or ' +
      '"UnitProposed"; data is the decision/finding object.',
    {
      type: z.string().describe('DecisionMade | ReviewFinding | UnitProposed'),
      data: z.record(z.string(), z.unknown()).describe('the decision/finding payload'),
      meta: z
        .record(z.string(), z.string())
        .optional()
        .describe('string->string metadata, e.g. {"actor":"<agent-id>"}'),
      valid_from: z
        .union([z.number(), z.string()])
        .optional()
        .describe('when the fact became true: unix nanoseconds or RFC3339'),
    },
    async (args) => {
      const payload = { type: args.type, data: args.data }
      if (args.meta) payload.meta = args.meta
      if (args.valid_from !== undefined) payload.valid_from = args.valid_from
      await call(client, 'rigger_emit', payload)
      return { content: [{ type: 'text', text: `recorded ${args.type}` }] }
    },
  )

  const peersTool = tool(
    'rigger_peers',
    'List the decisions and review findings other agents have raised so far this ' +
      'run. Pass `files` (your blast-radius) to scope the result to the files you ' +
      'are touching; omit it to see every one.',
    {
      files: z
        .array(z.string())
        .optional()
        .describe('the blast-radius: scope decisions/findings to these files'),
    },
    async (args) => {
      const peers = await call(client, 'rigger_peers', { files: args.files || [] })
      return { content: [{ type: 'text', text: JSON.stringify(peers) }] }
    },
  )

  return createSdkMcpServer({
    name: 'rigger',
    version: '0.1.0',
    tools: [emitTool, peersTool],
  })
}

// runWorkflow is the pure driver loop. It is given:
//   - client:   a connected MCP client to `rigger serve` (already initialized),
//   - runAgent: async ({ prompt, model, tools, dir, proxyServer }) => string,
//               which runs one agent and returns its final output text.
// It loops rigger_next -> peers injection -> runAgent -> rigger_result until the
// conductor reports an empty id, then returns the number of spawns it drove.
export async function runWorkflow(client, runAgent, opts = {}) {
  // How long to wait before re-polling rigger_next when the conductor is still
  // running but has nothing queued yet (it grounds/spawns asynchronously, so an
  // empty queue early in the run is transient, not terminal).
  const idlePollMs = opts.idlePollMs ?? 50
  const proxyServer = buildProxyServer(client)
  let drove = 0
  for (;;) {
    const next = await call(client, 'rigger_next', {})
    if (!next || !next.id) {
      // An empty id with done:true means the conductor has finished - exit. An
      // empty id with done:false (or absent, for older servers that only signal
      // done by closing) means nothing is queued YET: wait briefly and poll again
      // rather than exiting before the first spawn is enqueued.
      if (next && next.done) break
      await sleep(idlePollMs)
      continue
    }

    // Tool-boundary injection: fetch the blast-radius-scoped peer decisions and
    // prepend them so the agent starts aware of its peers (§5.3).
    const peers = await call(client, 'rigger_peers', { files: next.blast_radius || [] })
    const refresh = renderPeers(peers)
    const prompt = refresh ? `${refresh}\n${next.prompt}` : next.prompt

    debug(`spawn ${next.id}: model=${next.model || '(default)'} tools=${JSON.stringify(next.tools || [])} dir=${next.dir || '(cwd)'}`)
    debug(`spawn ${next.id} prompt:\n${prompt}`)

    let output
    let error = ''
    try {
      output = await runAgent({
        prompt,
        // The agent's persona (its role) - the conductor's single persona source,
        // threaded through SpawnRequest.system_prompt - is passed to query() as the
        // system prompt, so a workflow agent gets its role exactly as the cli path does.
        systemPrompt: next.system_prompt || undefined,
        model: next.model || undefined,
        tools: next.tools || [],
        dir: next.dir || undefined,
        proxyServer,
      })
    } catch (e) {
      error = e?.message || String(e)
      output = ''
    }
    debug(`spawn ${next.id} output: ${JSON.stringify(output)}${error ? ` error=${error}` : ''}`)

    const resultArgs = { id: next.id, output: typeof output === 'string' ? output : JSON.stringify(output) }
    if (error) resultArgs.error = error
    await call(client, 'rigger_result', resultArgs)
    drove += 1
  }
  return drove
}

// buildAgentOptions builds the Agent SDK query() options for one spawn. It is a
// PURE function (no I/O) so the worktree-isolation invariant is machine-verifiable:
// the agent's WORKING DIRECTORY is the spawn's `dir` (its isolated worktree), set as
// `options.cwd`. The SDK's cwd "Defaults to process.cwd()" (sdk.d.ts) - which, for
// `rigger workflow` run from the repo root, IS the live main checkout - so a spawn
// that failed to carry its worktree dir would run the agent in the main repo and let
// the implementer's edits (or a reviewer's stray Bash/Edit) corrupt it. The conductor
// guarantees every lifecycle spawn carries a worktree `dir`; this function threads it
// to `cwd` so relative-path tool calls resolve INSIDE the worktree, never the repo
// root. (`dir` is empty only on a genuinely repo-less run, where there is no main
// checkout to protect and the project cwd is the intended workspace.)
export function buildAgentOptions({ systemPrompt, model, tools, dir, proxyServer }) {
  // The agent always gets the two rigger tools (namespaced mcp__rigger__*) on top
  // of whatever the spawn allows, so it can always emit/peer through the proxy.
  const allowedTools = [
    ...(tools || []),
    'mcp__rigger__rigger_emit',
    'mcp__rigger__rigger_peers',
  ]
  const options = {
    mcpServers: { rigger: proxyServer },
    allowedTools,
    // The agent must not fan out (runaway-proof by construction, §3.1/§6); the
    // spawn's tool list already excludes Agent/Task, and we never add them.
    permissionMode: 'bypassPermissions',
  }
  // The persona is the agent's ROLE: pass it as query()'s custom systemPrompt (a
  // plain string => "use a custom system prompt"), the same role the cli path passes
  // via `--system-prompt`. Omitted when empty so the agent keeps the default prompt.
  if (systemPrompt) options.systemPrompt = systemPrompt
  if (model) options.model = model
  // The agent's working directory is its isolated worktree. Set it explicitly so the
  // SDK does NOT fall back to process.cwd() (the main repo checkout). Left unset only
  // when `dir` is empty (a repo-less run with no checkout to corrupt).
  if (dir) options.cwd = dir
  return options
}

// runAgentViaSdk is the real runAgent: it runs one agent via the Agent SDK's
// query(), giving it the in-process proxy server (so its rigger_emit/rigger_peers
// reach the shared connection) plus the spawn's persona (its role, as the system
// prompt), model, allowed tools, and dir (its worktree, as cwd). It returns the
// agent's final result text.
export async function runAgentViaSdk({ prompt, systemPrompt, model, tools, dir, proxyServer }) {
  const options = buildAgentOptions({ systemPrompt, model, tools, dir, proxyServer })

  let result = ''
  for await (const message of query({ prompt, options })) {
    if (message.type === 'result') {
      if (message.subtype === 'success') {
        result = message.result
      } else {
        // An error result (max turns, execution error, usage limit, ...) surfaces as
        // a thrown Error. We include any error detail AND the result text so a usage-
        // limit notice (which arrives as the result body, e.g. "...usage limit
        // reached · resets 5am (America/Chicago)") survives into the message - that is
        // what withLimitResume matches on to pause-and-resume instead of failing.
        const parts = []
        if (Array.isArray(message.errors) && message.errors.length) parts.push(message.errors.join('; '))
        if (typeof message.result === 'string' && message.result) parts.push(message.result)
        const detail = parts.length ? parts.join('; ') : message.subtype
        throw new Error(`agent run failed (${message.subtype}): ${detail}`)
      }
    }
  }
  return result
}

// connect spawns `rigger serve <spec>` and connects an MCP client to it over the
// stdio transport (the transport spawns the child itself). The rigger binary is
// taken from RIGGER_BIN (set by `rigger workflow`) or defaults to `rigger` on
// PATH. Spawn/connect failures throw with a clear, actionable message.
export async function connect(specPath) {
  const bin = process.env.RIGGER_BIN || 'rigger'
  const args = ['serve']
  if (specPath) args.push(specPath)

  const transport = new StdioClientTransport({
    command: bin,
    args,
    // Inherit the parent env so the child rigger finds the repo, PATH, etc. The
    // child's stderr is forwarded to ours so a conductor error is visible.
    env: process.env,
    stderr: 'inherit',
  })
  const client = new Client({ name: 'rigger-shim', version: '0.1.0' }, { capabilities: {} })
  try {
    await client.connect(transport)
  } catch (e) {
    throw new Error(
      `failed to start/connect to "${bin} ${args.join(' ')}": ${e?.message || e}. ` +
        `Is rigger on PATH (or RIGGER_BIN set)? Is the spec path correct?`,
    )
  }
  return { client, transport }
}

// main wires the real transport + real agent runner and runs the loop. Run as:
//   node shim.mjs <spec-path>      (normally via `rigger workflow <spec>`)
async function main() {
  const specPath = process.argv[2]
  let client
  let transport
  try {
    ;({ client, transport } = await connect(specPath))
  } catch (e) {
    console.error(`rigger-shim: ${e.message}`)
    process.exit(1)
    return
  }
  try {
    // Wrap the real agent runner so a usage-limit hit PAUSES until the reset time and
    // RESUMES (retries) transparently, rather than failing the spawn. Uses the real
    // clock + setTimeout; the seam is stubbed in the tests to make the wait instant.
    const resumableAgent = withLimitResume(runAgentViaSdk)
    const drove = await runWorkflow(client, resumableAgent)
    console.error(`rigger-shim: drove ${drove} spawn(s); conductor finished`)
  } catch (e) {
    console.error(`rigger-shim: ${e?.message || e}`)
    process.exitCode = 1
  } finally {
    await client.close().catch(() => {})
    await transport.close().catch(() => {})
  }
}

// Only run main() when executed directly (not when imported by the tests).
const isMain = process.argv[1] && import.meta.url === new URL(`file://${process.argv[1]}`).href
if (isMain) {
  main()
}
