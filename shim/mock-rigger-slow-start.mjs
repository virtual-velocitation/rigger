#!/usr/bin/env node
// A mock of `rigger serve` that reproduces the conductor's SLOW START: the first
// rigger_next poll arrives before any spawn is queued, so it returns an empty id
// with done:false (the conductor is still grounding). Only on a later poll does the
// spawn appear; then done:true. Used by shim.test.mjs to prove the shim loop polls
// past an early empty response instead of exiting prematurely (the race the real
// e2e run hit). Same wire protocol as mock-rigger-server.mjs.

import { appendFileSync } from 'node:fs'
import { createInterface } from 'node:readline'

const recordPath = process.env.RIGGER_MOCK_RECORD
function record(entry) {
  if (recordPath) appendFileSync(recordPath, JSON.stringify(entry) + '\n')
}

const SPAWN = {
  id: '1',
  prompt: 'do the one unit',
  model: 'sonnet',
  tools: ['Read'],
  dir: '',
  blast_radius: ['a.rs'],
}

// How many empty (done:false) polls to return before the spawn appears.
let emptyPollsRemaining = 2
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
      if (emptyPollsRemaining > 0) {
        emptyPollsRemaining -= 1
        // Conductor still running, nothing queued yet.
        return structured({ id: '', done: false })
      }
      if (!spawnHandedOut) {
        spawnHandedOut = true
        return structured(SPAWN)
      }
      return structured({ id: '', done: true })
    }
    case 'rigger_peers':
      return structured({ decisions: [], findings: [] })
    case 'rigger_emit':
      record({ tool: 'rigger_emit', args })
      return structured({})
    case 'rigger_result':
      record({ tool: 'rigger_result', args })
      return structured({})
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
            serverInfo: { name: 'mock-rigger-slow-start', version: '0.1.0' },
          })
    case 'notifications/initialized':
      return null
    case 'tools/list':
      return id === undefined ? null : reply(id, { tools: [] })
    case 'tools/call': {
      if (id === undefined) return null
      const result = callTool(msg.params?.name, msg.params?.arguments || {})
      if (result === null) return errReply(id, -32602, `unknown tool ${msg.params?.name}`)
      return reply(id, result)
    }
    default:
      return id === undefined ? null : errReply(id, -32601, `method not found: ${msg.method}`)
  }
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
