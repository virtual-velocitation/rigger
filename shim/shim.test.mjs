// Automated test of the shim driver loop, run with `node --test` (no network).
//
// It proves, against a MOCK `rigger serve` (mock-rigger-server.mjs, spawned over a
// real StdioClientTransport exactly like production):
//   1. the MCP handshake against the mock succeeds and the loop pulls ONE spawn
//      then exits on the empty id;
//   2. rigger_next / rigger_peers / rigger_result payloads are correctly unwrapped
//      from `structuredContent` (the mock wraps them there like the Rust server);
//   3. the blast-radius reaches rigger_peers and the peer decision is injected;
//   4. an agent calling the PROXIED rigger_emit reaches the mock - i.e. the agent's
//      tool call goes through the in-process proxy server's real handler, out the
//      shim's single shared client connection, and lands at the mock;
//   5. the agent's output is reported back via rigger_result.

import { test } from 'node:test'
import assert from 'node:assert/strict'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'
import { mkdtempSync, readFileSync, existsSync } from 'node:fs'
import { tmpdir } from 'node:os'

import { Client } from '@modelcontextprotocol/sdk/client/index.js'
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js'
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js'

import {
  runWorkflow,
  unwrap,
  renderPeers,
  buildProxyServer,
  buildAgentOptions,
  isUsageLimitError,
  parseResetTime,
  computeRetryDelayMs,
  withLimitResume,
  DEFAULT_RETRY_INTERVAL_MS,
} from './shim.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const MOCK = join(here, 'mock-rigger-server.mjs')
const MOCK_SLOW = join(here, 'mock-rigger-slow-start.mjs')

function readRecords(path) {
  if (!existsSync(path)) return []
  return readFileSync(path, 'utf8')
    .split('\n')
    .filter((l) => l.trim())
    .map((l) => JSON.parse(l))
}

test('unwrap pulls structuredContent, falling back to the text block', () => {
  assert.deepEqual(unwrap({ structuredContent: { id: '7' }, content: [] }), { id: '7' })
  assert.deepEqual(unwrap({ content: [{ type: 'text', text: '{"id":"9"}' }] }), { id: '9' })
  assert.deepEqual(unwrap({ content: [{ type: 'text', text: 'not json' }] }), {})
})

test('renderPeers injects scoped peer decisions, empty when none', () => {
  assert.equal(renderPeers({ decisions: [] }), '')
  const out = renderPeers({ decisions: [{ id: 'p1', summary: 'use the buffer', governs: ['a.rs'] }] })
  assert.match(out, /PEERS CONTEXT REFRESH/)
  assert.match(out, /p1: use the buffer \[governs: a\.rs\]/)
})

test('the loop drives one spawn end-to-end and the proxied rigger_emit reaches the mock', async () => {
  const recordPath = join(mkdtempSync(join(tmpdir(), 'rigger-shim-test-')), 'record.jsonl')

  // Connect a real MCP client to the mock `rigger serve` over a real stdio
  // transport (the transport spawns the child), exactly as production does.
  const transport = new StdioClientTransport({
    command: process.execPath, // node
    args: [MOCK],
    env: { ...process.env, RIGGER_MOCK_RECORD: recordPath },
    stderr: 'inherit',
  })
  const client = new Client({ name: 'rigger-shim-test', version: '0.0.0' }, { capabilities: {} })
  await client.connect(transport)

  // What the stub agent saw, so we can assert the peers refresh was injected and
  // the spawn fields (including the persona/system prompt) arrived.
  const seen = { prompts: [], systemPrompts: [], models: [], tools: [] }

  // The stub agent simulates a real agent: it calls the PROXIED rigger_emit on the
  // in-process proxy server the shim built (proxyServer.instance, an McpServer),
  // through a real in-memory MCP client pair - the same machinery the Agent SDK's
  // CLI uses. That call must travel proxy handler -> shim's shared client -> mock.
  const stubAgent = async ({ prompt, systemPrompt, model, tools, proxyServer }) => {
    seen.prompts.push(prompt)
    seen.systemPrompts.push(systemPrompt)
    seen.models.push(model)
    seen.tools.push(tools)

    const [clientSide, serverSide] = InMemoryTransport.createLinkedPair()
    const agentClient = new Client({ name: 'stub-agent', version: '0.0.0' }, { capabilities: {} })
    await Promise.all([proxyServer.instance.connect(serverSide), agentClient.connect(clientSide)])

    // The agent emits a decision (the EMIT_PROTOCOL shape) through the proxy.
    const emitResult = await agentClient.callTool({
      name: 'rigger_emit',
      arguments: {
        type: 'DecisionMade',
        data: { id: 'agent-d1', summary: 'split the pipeline', governs: ['a.rs'] },
        meta: { actor: 'stub-agent' },
      },
    })
    assert.equal(emitResult.isError ?? false, false, 'the proxied rigger_emit must not error')

    // The agent also reads peers through the proxy (proves rigger_peers proxies too).
    const peersResult = await agentClient.callTool({ name: 'rigger_peers', arguments: { files: ['a.rs'] } })
    const peers = unwrap(peersResult)
    assert.equal(peers.decisions[0].id, 'peer-1', 'the proxied rigger_peers returns the mock peer')

    await agentClient.close()
    return 'unit implemented; final result line'
  }

  const drove = await runWorkflow(client, stubAgent)
  await client.close()
  await transport.close()

  // The loop drove exactly one spawn (mock hands out one, then empty id).
  assert.equal(drove, 1, 'the loop must drive exactly one spawn then exit on the empty id')

  // The spawn fields were unwrapped from structuredContent and passed to the agent.
  assert.equal(seen.models[0], 'sonnet', 'the spawn model reached the agent')
  assert.deepEqual(seen.tools[0], ['Read'], 'the spawn tools reached the agent')
  // The persona (the agent's role) reached the agent runner as the system prompt -
  // the workflow path threads the role exactly as the cli path does.
  assert.equal(
    seen.systemPrompts[0],
    'You are the rust engineer. Implement the unit.',
    'the persona/system_prompt from the spawn reached the agent runner',
  )
  // The persona is NOT spliced into the task prompt: it is a distinct system prompt.
  assert.ok(
    !seen.prompts[0].includes('You are the rust engineer'),
    'the persona must be the system prompt, not concatenated into the task prompt',
  )
  // The blast-radius peer decision was injected into the prompt (tool-boundary injection).
  assert.match(seen.prompts[0], /PEERS CONTEXT REFRESH/, 'peers refresh was prepended')
  assert.match(seen.prompts[0], /use the buffer authority/, 'the mock peer decision was injected')
  assert.match(seen.prompts[0], /do the one unit/, 'the spawn prompt was preserved')

  // The mock recorded BOTH the agent's proxied emit AND the loop's result.
  const records = readRecords(recordPath)
  const emit = records.find((r) => r.tool === 'rigger_emit')
  assert.ok(emit, 'the agent-proxied rigger_emit must have reached the mock')
  assert.equal(emit.args.type, 'DecisionMade')
  assert.equal(emit.args.data.id, 'agent-d1', 'the agent decision payload reached the mock intact')
  assert.equal(emit.args.meta.actor, 'stub-agent', 'the emit meta.actor reached the mock')

  const result = records.find((r) => r.tool === 'rigger_result')
  assert.ok(result, 'rigger_result must have reached the mock')
  assert.equal(result.args.id, '1', 'the result was reported for the spawn id')
  assert.equal(result.args.output, 'unit implemented; final result line', 'the agent output was reported')
  assert.ok(!result.args.error, 'a successful agent run reports no error')
})

test('a thrown agent error is reported through rigger_result.error', async () => {
  const recordPath = join(mkdtempSync(join(tmpdir(), 'rigger-shim-test-')), 'record.jsonl')
  const transport = new StdioClientTransport({
    command: process.execPath,
    args: [MOCK],
    env: { ...process.env, RIGGER_MOCK_RECORD: recordPath },
    stderr: 'inherit',
  })
  const client = new Client({ name: 'rigger-shim-test', version: '0.0.0' }, { capabilities: {} })
  await client.connect(transport)

  const failingAgent = async () => {
    throw new Error('agent run failed (error_max_turns)')
  }
  const drove = await runWorkflow(client, failingAgent)
  await client.close()
  await transport.close()

  assert.equal(drove, 1)
  const records = readRecords(recordPath)
  const result = records.find((r) => r.tool === 'rigger_result')
  assert.ok(result, 'even a failed agent reports a result (so the conductor is unblocked)')
  assert.equal(result.args.id, '1')
  assert.match(result.args.error, /agent run failed/, 'the agent error is reported in the error field')
})

test('the loop polls past empty done:false responses (slow conductor start)', async () => {
  // Regression test for the race the first real e2e run hit: the conductor enqueues
  // the planner spawn asynchronously, so the shim's first rigger_next poll returns
  // an empty id BEFORE any spawn exists. The shim must keep polling (done:false),
  // not exit. The slow-start mock returns two empty done:false polls, then the
  // spawn, then done:true.
  const recordPath = join(mkdtempSync(join(tmpdir(), 'rigger-shim-test-')), 'record.jsonl')
  const transport = new StdioClientTransport({
    command: process.execPath,
    args: [MOCK_SLOW],
    env: { ...process.env, RIGGER_MOCK_RECORD: recordPath },
    stderr: 'inherit',
  })
  const client = new Client({ name: 'rigger-shim-test', version: '0.0.0' }, { capabilities: {} })
  await client.connect(transport)

  let agentRuns = 0
  const stubAgent = async () => {
    agentRuns += 1
    return 'done after a slow start'
  }

  // Tiny idle-poll so the test is fast.
  const drove = await runWorkflow(client, stubAgent, { idlePollMs: 5 })
  await client.close()
  await transport.close()

  assert.equal(drove, 1, 'the loop drove the one spawn that appeared after the empty polls')
  assert.equal(agentRuns, 1, 'the agent ran exactly once, after the conductor caught up')
  const records = readRecords(recordPath)
  const result = records.find((r) => r.tool === 'rigger_result')
  assert.ok(result, 'the result reached the mock after the slow start')
  assert.equal(result.args.output, 'done after a slow start')
})

test('buildProxyServer exposes exactly rigger_emit and rigger_peers', async () => {
  // A no-op client; we only inspect the tool surface the proxy advertises.
  const fakeClient = { callTool: async () => ({ structuredContent: {} }) }
  const proxyServer = buildProxyServer(fakeClient)

  const [clientSide, serverSide] = InMemoryTransport.createLinkedPair()
  const inspector = new Client({ name: 'inspector', version: '0.0.0' }, { capabilities: {} })
  await Promise.all([proxyServer.instance.connect(serverSide), inspector.connect(clientSide)])
  const { tools } = await inspector.listTools()
  await inspector.close()

  const names = tools.map((t) => t.name).sort()
  assert.deepEqual(names, ['rigger_emit', 'rigger_peers'], 'the proxy exposes exactly the two rigger tools')
})

test('buildAgentOptions sets the Agent SDK cwd to the spawn worktree dir', () => {
  // The worktree-isolation invariant on the shim side: the agent's working directory
  // (Agent SDK `options.cwd`) MUST be the spawn's `dir` - its isolated worktree - so
  // the agent's relative-path tool calls resolve inside the worktree, never the shim's
  // own cwd (= the main repo checkout for `rigger workflow`). A spawn that carried its
  // worktree dir must NOT run the agent in the repo root.
  const dir = '/tmp/rigger-wt-unit-1-abcd1234'
  const proxyServer = { instance: {} }
  const options = buildAgentOptions({
    systemPrompt: 'You are the rust engineer.',
    model: 'sonnet',
    tools: ['Read', 'Edit'],
    dir,
    proxyServer,
  })
  assert.equal(options.cwd, dir, 'the agent runs IN its worktree (cwd === the spawn dir), not the main repo')
  assert.equal(options.permissionMode, 'bypassPermissions')
  // The rigger proxy tools are always granted on top of the spawn tools.
  assert.ok(options.allowedTools.includes('mcp__rigger__rigger_emit'))
  assert.ok(options.allowedTools.includes('mcp__rigger__rigger_peers'))
})

test('buildAgentOptions omits cwd only when no worktree dir is given (repo-less run)', () => {
  // An empty `dir` means a genuinely repo-less run - there is no main checkout to
  // protect, so the agent runs in the project cwd (cwd omitted => SDK default). The
  // conductor never hands a writing agent an empty dir when a repo is configured, so
  // this branch is reachable ONLY when there is nothing to corrupt.
  const optsNoDir = buildAgentOptions({ dir: '', proxyServer: { instance: {} } })
  assert.equal('cwd' in optsNoDir, false, 'no dir => no cwd override (repo-less run only)')
  const optsUndef = buildAgentOptions({ proxyServer: { instance: {} } })
  assert.equal('cwd' in optsUndef, false, 'an undefined dir likewise sets no cwd')
})

test('the loop threads the spawn worktree dir to the agent runner', async () => {
  // End-to-end through runWorkflow: the spawn the mock hands out carries a `dir`, and
  // the loop MUST pass it to the agent runner so the agent runs in that worktree. This
  // closes the path conductor.dir -> SpawnRequest.dir -> next.dir -> runAgent({dir}).
  const transport = new StdioClientTransport({
    command: process.execPath,
    args: [MOCK],
    env: { ...process.env, RIGGER_MOCK_DIR: '/tmp/rigger-wt-unit-1-deadbeef' },
    stderr: 'inherit',
  })
  const client = new Client({ name: 'rigger-shim-test', version: '0.0.0' }, { capabilities: {} })
  await client.connect(transport)

  const seenDirs = []
  const stubAgent = async ({ dir }) => {
    seenDirs.push(dir)
    return 'done'
  }
  const drove = await runWorkflow(client, stubAgent)
  await client.close()
  await transport.close()

  assert.equal(drove, 1, 'the loop drove the one spawn')
  assert.equal(
    seenDirs[0],
    '/tmp/rigger-wt-unit-1-deadbeef',
    'the spawn worktree dir reached the agent runner, so the agent runs in its worktree',
  )
})

// ---------------------------------------------------------------------------
// Usage-limit pause-and-resume.

test('isUsageLimitError recognizes the limit wordings, not real failures', () => {
  assert.equal(isUsageLimitError("You've hit your weekly limit · resets 5am (America/Chicago)"), true)
  assert.equal(isUsageLimitError('Claude AI usage limit reached'), true)
  assert.equal(isUsageLimitError("You've hit your 5-hour limit, resets at 11pm (PST)"), true)
  assert.equal(isUsageLimitError('rate limited; try again later'), true)
  // A genuine stage failure must NOT be treated as a limit (it should fail the spawn).
  assert.equal(isUsageLimitError('agent run failed (error_max_turns)'), false)
  assert.equal(isUsageLimitError('compile error: mismatched types'), false)
  assert.equal(isUsageLimitError(''), false)
  assert.equal(isUsageLimitError(undefined), false)
})

test('parseResetTime("5am (America/Chicago)") -> the next future 5am Chicago instant', () => {
  // Anchor "now" at a fixed instant: 2026-06-23 12:00:00 UTC (= 07:00 CDT, Chicago is
  // UTC-5 in June). The next 5am Chicago is therefore TOMORROW 5am CDT = 10:00 UTC the
  // following day, since 5am today already passed.
  const nowMs = Date.UTC(2026, 5, 23, 12, 0, 0)
  const reset = parseResetTime("You've hit your weekly limit · resets 5am (America/Chicago)", nowMs)
  assert.ok(reset instanceof Date, 'a parseable message yields a Date')
  assert.ok(reset.getTime() > nowMs, 'the reset is in the future')

  // The reset, read back IN Chicago, must be exactly 05:00 local.
  const fmt = new Intl.DateTimeFormat('en-US', {
    timeZone: 'America/Chicago',
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
  })
  const parts = {}
  for (const p of fmt.formatToParts(reset)) parts[p.type] = p.value
  const hour = parts.hour === '24' ? '00' : parts.hour
  assert.equal(hour, '05', 'the reset is at 5am Chicago local time')
  assert.equal(parts.minute, '00', 'on the hour')

  // June Chicago is CDT (UTC-5), so 5am CDT == 10:00 UTC, and it is tomorrow.
  assert.equal(reset.getTime(), Date.UTC(2026, 5, 24, 10, 0, 0), 'next 5am CDT is tomorrow 10:00 UTC')
})

test('parseResetTime handles minutes, pm, and an abbreviated zone; null when unparseable', () => {
  const nowMs = Date.UTC(2026, 5, 23, 12, 0, 0)
  const r = parseResetTime('resets at 11:30pm (PST)', nowMs)
  assert.ok(r instanceof Date && r.getTime() > nowMs)
  const fmt = new Intl.DateTimeFormat('en-US', {
    timeZone: 'America/Los_Angeles',
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
  })
  const parts = {}
  for (const p of fmt.formatToParts(r)) parts[p.type] = p.value
  assert.equal(parts.hour, '23', '11:30pm -> 23:xx local')
  assert.equal(parts.minute, '30')

  // No reset time present -> null (caller falls back to a fixed interval).
  assert.equal(parseResetTime('You have hit your weekly limit', nowMs), null)
  assert.equal(parseResetTime('totally unrelated message', nowMs), null)
})

test('computeRetryDelayMs: parsed reset -> wait to reset (+buffer); unparseable -> fixed interval', () => {
  const nowMs = Date.UTC(2026, 5, 23, 12, 0, 0)
  // Parseable: 5am Chicago tomorrow = 2026-06-24 10:00 UTC. Wait = that - now + 5s buffer.
  const parsed = computeRetryDelayMs("resets 5am (America/Chicago)", nowMs, { bufferMs: 5000 })
  const expected = Date.UTC(2026, 5, 24, 10, 0, 0) - nowMs + 5000
  assert.equal(parsed, expected, 'the wait spans now -> reset plus the buffer')

  // Unparseable: fall back to the fixed interval.
  const fallback = computeRetryDelayMs('weekly limit reached, no time given', nowMs, {
    intervalMs: DEFAULT_RETRY_INTERVAL_MS,
  })
  assert.equal(fallback, DEFAULT_RETRY_INTERVAL_MS, 'no reset time -> fixed interval')

  // A far-future parse is capped (never an absurd single sleep).
  const capped = computeRetryDelayMs("resets 5am (America/Chicago)", nowMs, { maxWaitMs: 60_000 })
  assert.equal(capped, 60_000, 'the wait is clamped to maxWaitMs')
})

test('withLimitResume: limit error once (parseable) -> parses, waits via the injected clock, retries to success', async () => {
  // The fake agent returns a usage-limit error on its FIRST call (a parseable reset
  // time) then succeeds on the SECOND - exactly a real run that died on a limit.
  let calls = 0
  const fakeAgent = async (spawn) => {
    calls += 1
    if (calls === 1) {
      throw new Error("agent run failed (error): Claude AI usage limit reached · resets 5am (America/Chicago)")
    }
    return `done for ${spawn.id}`
  }

  // Injected clock: starts at a fixed instant and JUMPS to the sleep target instantly,
  // so the test does not actually sleep. We record what we waited for to assert on it.
  let clockMs = Date.UTC(2026, 5, 23, 12, 0, 0)
  const sleeps = []
  const now = () => clockMs
  const sleepUntil = async (targetMs, currentMs) => {
    sleeps.push({ targetMs, currentMs })
    clockMs = targetMs // advance the virtual clock; no real wait
  }

  const resumable = withLimitResume(fakeAgent, { now, sleepUntil })
  const out = await resumable({ id: '1' })

  assert.equal(out, 'done for 1', 'the spawn eventually succeeds after the limit clears')
  assert.equal(calls, 2, 'the agent was retried exactly once after the limit hit')
  assert.equal(sleeps.length, 1, 'it paused exactly once')

  // It WAITED until the parsed reset: 5am Chicago tomorrow (10:00 UTC 2026-06-24),
  // plus the default 5s buffer - proving it parsed the reset time, not a fixed interval.
  const expectedTarget = Date.UTC(2026, 5, 24, 10, 0, 0) + 5000
  assert.equal(sleeps[0].targetMs, expectedTarget, 'it waited until the parsed reset time (+buffer)')
  assert.equal(sleeps[0].currentMs, Date.UTC(2026, 5, 23, 12, 0, 0), 'it measured the wait from the limit-hit instant')
})

test('withLimitResume: unparseable limit message -> fixed-interval wait, still retries to success', async () => {
  let calls = 0
  const fakeAgent = async () => {
    calls += 1
    if (calls === 1) throw new Error('agent run failed (error): weekly limit reached') // no reset time
    return 'recovered'
  }

  let clockMs = 1_000_000
  const sleeps = []
  const resumable = withLimitResume(fakeAgent, {
    now: () => clockMs,
    sleepUntil: async (targetMs) => {
      sleeps.push(targetMs)
      clockMs = targetMs
    },
    intervalMs: 900_000, // explicit fixed fallback
  })

  const out = await resumable({ id: '7' })
  assert.equal(out, 'recovered', 'an unparseable limit message still pauses-and-resumes to success')
  assert.equal(calls, 2)
  assert.equal(sleeps.length, 1)
  assert.equal(sleeps[0], 1_000_000 + 900_000, 'it waited the fixed interval (no parseable reset)')
})

test('withLimitResume: a non-limit error is re-thrown immediately (real failure, not paused)', async () => {
  let calls = 0
  let slept = false
  const fakeAgent = async () => {
    calls += 1
    throw new Error('agent run failed (error_max_turns): the agent gave up')
  }
  const resumable = withLimitResume(fakeAgent, {
    now: () => 0,
    sleepUntil: async () => {
      slept = true
    },
  })
  await assert.rejects(() => resumable({ id: '1' }), /error_max_turns/, 'a real failure surfaces unchanged')
  assert.equal(calls, 1, 'a non-limit error is NOT retried')
  assert.equal(slept, false, 'and the loop does not pause on a real failure')
})

test('withLimitResume: caps retries so a never-clearing limit cannot loop forever', async () => {
  let calls = 0
  const fakeAgent = async () => {
    calls += 1
    throw new Error('Claude AI usage limit reached') // never recovers
  }
  let clockMs = 0
  const resumable = withLimitResume(fakeAgent, {
    now: () => clockMs,
    sleepUntil: async (t) => {
      clockMs = t
    },
    maxRetries: 3,
    intervalMs: 1000,
  })
  await assert.rejects(() => resumable({ id: '1' }), /not cleared after 3 retries/, 'it gives up after the cap')
  // 1 initial attempt + 3 retries = 4 calls.
  assert.equal(calls, 4, 'it tried the initial run plus exactly maxRetries retries')
})

test('the loop drives a limit-then-success spawn end-to-end (withLimitResume around runWorkflow)', async () => {
  // Prove the wrapper composes with runWorkflow against the real mock conductor: the
  // agent hits the limit once, the wrapper waits (instant, injected clock) and retries,
  // and the loop reports the eventual SUCCESS via rigger_result - never an error.
  const recordPath = join(mkdtempSync(join(tmpdir(), 'rigger-shim-test-')), 'record.jsonl')
  const transport = new StdioClientTransport({
    command: process.execPath,
    args: [MOCK],
    env: { ...process.env, RIGGER_MOCK_RECORD: recordPath },
    stderr: 'inherit',
  })
  const client = new Client({ name: 'rigger-shim-test', version: '0.0.0' }, { capabilities: {} })
  await client.connect(transport)

  let calls = 0
  const flakyAgent = async () => {
    calls += 1
    if (calls === 1) throw new Error("Claude AI usage limit reached · resets 5am (America/Chicago)")
    return 'unit done after the limit cleared'
  }
  let clockMs = Date.UTC(2026, 5, 23, 12, 0, 0)
  const resumable = withLimitResume(flakyAgent, {
    now: () => clockMs,
    sleepUntil: async (t) => {
      clockMs = t
    },
  })

  const drove = await runWorkflow(client, resumable)
  await client.close()
  await transport.close()

  assert.equal(drove, 1, 'the loop drove the one spawn')
  assert.equal(calls, 2, 'the agent ran twice: limit hit, then success on resume')
  const records = readRecords(recordPath)
  const result = records.find((r) => r.tool === 'rigger_result')
  assert.ok(result, 'a result was reported')
  assert.equal(result.args.output, 'unit done after the limit cleared', 'the SUCCESS output was reported, post-resume')
  assert.ok(!result.args.error, 'the limit was NOT surfaced as a stage failure')
})
