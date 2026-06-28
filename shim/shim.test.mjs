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

import { runWorkflow, unwrap, renderPeers, buildProxyServer } from './shim.mjs'

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
