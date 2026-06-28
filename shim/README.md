# Workflow driver - the standalone Node MCP-client + Agent-SDK driver

The agent driver for the `workflow` path. The Rust conductor stays the
orchestrator; a standalone **Node program** (`shim.mjs`) connects to it over
**MCP**, pulls each queued agent spawn, runs that agent via the **Claude Agent
SDK**, and reports the result. The two halves talk over MCP-on-stdio.

## Why this is a standalone Node program, not a Claude Code Workflow script

A Claude Code **Workflow** script cannot call MCP tools, so the driver cannot be
a Workflow script. It is a normal Node process built on two SDKs:

- **`@modelcontextprotocol/sdk`** - an MCP **client** (`Client` +
  `StdioClientTransport`). The shim spawns `rigger serve` and connects this client
  to it. This is the **one shared connection** through which every spawn is pulled
  (`rigger_next`), every result reported (`rigger_result`), and every agent's
  decisions/findings recorded (`rigger_emit`) or read (`rigger_peers`). The
  `rigger serve` MCP server speaks a single stdio pair, so there is exactly one
  connection.

- **`@anthropic-ai/claude-agent-sdk`** - runs each agent via `query()`. Each agent
  is given an **in-process SDK MCP server** (`createSdkMcpServer` + `tool`) that
  exposes `rigger_emit` and `rigger_peers`; those tool handlers **proxy** to the
  shim's single MCP client above. So when an agent emits a decision it travels
  proxy-handler -> shared client -> `rigger serve` and lands in the event log
  **live** - there is never a second connection to the single-stdio server.

## One-command activation (turn-key)

```
rigger workflow [spec]
```

That is the whole thing. `rigger workflow` execs `node shim.mjs <spec>`, sets
`RIGGER_BIN` to the running `rigger` binary so the shim spawns the *same* build,
and the shim does the rest (spawn `rigger serve`, MCP handshake, drive the loop).

One-time prerequisite: install the shim's npm deps.

```
cd shim && npm install
```

Requires `node` on `PATH` (override with `RIGGER_NODE`). The shim is located
next to the binary or, in a dev checkout, at `shim/shim.mjs` relative to the
repo root; override with `RIGGER_SHIM=<path>`.

`rigger serve` still works on its own (the shim spawns it); registering it as an
MCP server in an external host is no longer required for the driver.

## The loop

1. `rigger_next` -> the next queued spawn `{id, prompt, model, tools, dir,
   blast_radius}`. An empty `id` carries a `done` flag: `done:true` means the
   conductor has finished (exit); `done:false` means it is still running and
   nothing is queued **yet** (the conductor enqueues spawns asynchronously) - so
   the shim polls again rather than exiting. This `done` signal is what stops the
   shim exiting before the conductor's first spawn is even enqueued.
2. **Tool-boundary injection (§5.3)** -> `rigger_peers` with the spawn's
   `blast_radius` as `files`; the blast-radius-scoped peer decisions are rendered
   as a "peers context refresh" prepended to the prompt, so the agent starts aware
   of what concurrent agents decided about the files it is about to touch.
3. `query(...)` -> runs the agent via the Agent SDK with the in-process proxy
   server, the spawn's model, and the spawn's allowed tools (plus the two proxied
   `mcp__rigger__*` tools so it can always emit/peer through the shared
   connection). Runs in `permissionMode: 'bypassPermissions'`.
4. `rigger_result {id, output, error?}` -> reports the agent's final output by
   spawn id, unblocking the conductor's `Spawn`.

Every tool reply is a `tools/call` result whose payload is under
`structuredContent` (the Rust server wraps it there - see
[`src/mcpserver.rs`](../src/mcpserver.rs)); the shim unwraps it on every call.

While an agent works it records each decision by calling the proxied `rigger_emit`
tool (`{type, data}`), which the proxy forwards over the shared client so it
appends straight to the event log - **live**, so the side-car and concurrent
agents see it immediately.

Set `RIGGER_SHIM_DEBUG=1` to trace each spawn (fields, assembled prompt, and the
agent's output) on stderr.

## The bridge protocol (MCP tools)

| Tool | In | Out / effect |
|---|---|---|
| `rigger_next` | (none) | `{id, prompt, model, tools, dir, blast_radius}`; `{id:"", done}` when nothing is queued (`done` distinguishes finished from not-yet) |
| `rigger_result` | `{id, output, error?}` | unblocks the conductor's `Spawn` for `id` |
| `rigger_emit` | `{type, data, meta?, valid_from?}` | appends the event to the log, live |
| `rigger_peers` | `{files?}` | the peers' decisions/findings; scoped to `files` (the blast-radius) when given, else all |

## Validation - now actually validated

The shim is validated two ways, both runnable without network:

1. **Automated tests** (`npm test`, `node --test`) drive the real loop against a
   **mock `rigger serve`** (`mock-rigger-server.mjs`, spawned over the real stdio
   transport) with a stubbed agent. They prove the MCP handshake, that the loop
   pulls one spawn and reports its result, that `structuredContent` is unwrapped,
   that the blast-radius peer injection works, that an **agent calling the proxied
   `rigger_emit` reaches the mock** (driven through the real in-process proxy
   server via an in-memory MCP client pair), that a thrown agent error is reported
   via `rigger_result.error`, and (against `mock-rigger-slow-start.mjs`) that the
   loop **polls past an early empty `done:false` response** instead of exiting -
   the slow-conductor-start race.

2. **A real end-to-end smoke run.** `rigger workflow <spec>` against a one-criterion
   spec drives a real `rigger serve`, completes the MCP handshake, pulls a
   `rigger_next` spawn, runs the planner agent via the real Agent SDK `query()`,
   and the agent's proxied `rigger_emit` calls land as `DecisionMade` events in the
   real event log - verified by reading `.rigger/events.db` after the run.

The Rust halves - the conductor, the bridge (`driver/workflow`), and the MCP
server (`mcpserver`) - are covered by `cargo test`, including the `done`-signal
contract (`tool_next` reports `done:true` only after the conductor finishes and
all spawns drain) and the cli driver's emit-bridge (parsing emit-protocol lines
from a subprocess agent's stdout and replaying them through `emit`).
