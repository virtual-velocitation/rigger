#!/usr/bin/env node
// A tiny mock of `rigger serve`'s MCP stdio server, used ONLY by shim.test.mjs.
//
// It speaks the same newline-delimited JSON-RPC the real Rust server speaks
// (src/mcpserver.rs): initialize, tools/list, and tools/call for the four rigger
// tools, with every tools/call payload returned under `structuredContent` (so the
// test exercises the shim's unwrap, exactly like the real server).
//
// Scripted behaviour:
//   - rigger_next: returns ONE spawn the first time, then {id:""} forever after
//     (so the shim loop pulls one spawn then exits).
//   - rigger_peers: returns one canned peer decision (proves blast-radius injection).
//   - rigger_emit: records the call, replies {}.
//   - rigger_result: records the call, replies {}.
//
// Recorded rigger_emit and rigger_result calls are appended (as JSON lines) to the
// file named by RIGGER_MOCK_RECORD, so the test can read back exactly what the
// shim - and the agent, via the proxy - sent through the one shared connection.

import { appendFileSync } from 'node:fs'
import { createInterface } from 'node:readline'

const recordPath = process.env.RIGGER_MOCK_RECORD
function record(entry) {
  if (recordPath) appendFileSync(recordPath, JSON.stringify(entry) + '\n')
}

// The single spawn the mock hands out, then never again. system_prompt is the
// agent's persona (its role), which the shim must pass to the agent runner as the
// system prompt (mirrors the Rust SpawnRequest carrying it from the conductor).
const SPAWN = {
  id: '1',
  prompt: 'do the one unit',
  system_prompt: 'You are the rust engineer. Implement the unit.',
  model: 'sonnet',
  tools: ['Read'],
  dir: '',
  blast_radius: ['a.rs'],
}
let spawnHandedOut = false

function structured(payload) {
  return {
    content: [{ type: 'text', text: JSON.stringify(payload) }],
    structuredContent: payload,
  }
}

function callTool(name, args) {
  switch (name) {
    case 'rigger_next': {
      if (!spawnHandedOut) {
        spawnHandedOut = true
        return structured(SPAWN)
      }
      // After the one spawn, report the run is finished (mirrors the real server's
      // tool_next, which sets done:true once the conductor's `finish()` has run) so
      // the shim loop exits instead of polling forever.
      return structured({ id: '', done: true })
    }
    case 'rigger_peers': {
      // One canned peer decision touching a.rs, so renderPeers has something to
      // inject and the test can assert the blast-radius made it across.
      return structured({
        decisions: [{ id: 'peer-1', summary: 'use the buffer authority', governs: args?.files || [] }],
        findings: [],
      })
    }
    case 'rigger_emit': {
      record({ tool: 'rigger_emit', args })
      return structured({})
    }
    case 'rigger_result': {
      record({ tool: 'rigger_result', args })
      return structured({})
    }
    default:
      return null
  }
}

function handle(msg) {
  const id = msg.id
  switch (msg.method) {
    case 'initialize':
      return id === undefined
        ? null
        : reply(id, {
            protocolVersion: '2024-11-05',
            capabilities: { tools: {} },
            serverInfo: { name: 'mock-rigger', version: '0.1.0' },
          })
    case 'notifications/initialized':
      return null
    case 'tools/list':
      return id === undefined ? null : reply(id, { tools: toolList() })
    case 'tools/call': {
      if (id === undefined) return null
      const name = msg.params?.name
      const args = msg.params?.arguments || {}
      const result = callTool(name, args)
      if (result === null) return errReply(id, -32602, `unknown tool ${name}`)
      return reply(id, result)
    }
    default:
      return id === undefined ? null : errReply(id, -32601, `method not found: ${msg.method}`)
  }
}

function toolList() {
  return [
    { name: 'rigger_next', description: 'next spawn', inputSchema: { type: 'object', properties: {} } },
    { name: 'rigger_result', description: 'report result', inputSchema: { type: 'object', properties: {} } },
    { name: 'rigger_emit', description: 'record decision', inputSchema: { type: 'object', properties: {} } },
    { name: 'rigger_peers', description: 'list peers', inputSchema: { type: 'object', properties: {} } },
  ]
}

function reply(id, result) {
  return JSON.stringify({ jsonrpc: '2.0', id, result })
}
function errReply(id, code, message) {
  return JSON.stringify({ jsonrpc: '2.0', id, error: { code, message } })
}

const rl = createInterface({ input: process.stdin })
rl.on('line', (line) => {
  const trimmed = line.trim()
  if (!trimmed) return
  let msg
  try {
    msg = JSON.parse(trimmed)
  } catch {
    process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id: null, error: { code: -32700, message: 'parse error' } }) + '\n')
    return
  }
  const out = handle(msg)
  if (out !== null && out !== undefined) {
    process.stdout.write(out + '\n')
  }
})
