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
    // Live progress (spec 14): the worker reports one short line after each significant step,
    // so a long silent stretch of real work is visible (via `rigger status` / the dash) rather
    // than looking like a stall. `require_store_dir` walks up from the worktree to the real
    // store, so the CLI resolves it correctly from wherever the agent runs.
    const progressNote =
      `\n\nLIVE PROGRESS: after each significant step - a search, a read, a build, a commit, a decision - report ONE short line of what you just did by running (Bash):\n` +
      `  rigger progress '${next.id}' '<one line: what you just did>'\n` +
      `Keep it flowing WHILE you work; do not batch it at the end.`
    const prompt = (refresh ? `${refresh}\n${next.prompt}` : next.prompt) + progressNote

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
        // An error result (max turns, execution error, ...) surfaces as a thrown
        // Error so the loop reports it via rigger_result's `error` field.
        const errs = Array.isArray(message.errors) ? message.errors.join('; ') : message.subtype
        throw new Error(`agent run failed (${message.subtype}): ${errs}`)
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
    const drove = await runWorkflow(client, runAgentViaSdk)
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
