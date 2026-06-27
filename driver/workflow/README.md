# Workflow driver - the (B) Workflow-tool + MCP bridge

The in-Claude-Code agent driver. The Go conductor stays the orchestrator; a thin
Claude Code **Workflow** shim drives the agents via `agent()`, and the two halves
talk over **MCP**. This keeps the Workflow tool's parallel/journal/resume while
the Go core does the real work (the DAG, gates, ledger, graph).

## How it fits together

1. `rigger serve` runs the conductor in the background and serves an MCP server
   over stdio (the [`mcpserver`](../../mcpserver) package). Register it with
   Claude Code:

   ```
   claude mcp add rigger -- rigger serve
   ```

2. The conductor walks the workflow DAG. Each `Spawn` enqueues a spawn request on
   the workflow bridge (this package).

3. The Workflow shim (`shim.mjs`) loops:
   - `rigger_next` -> the next queued spawn `{id, prompt, model, tools, dir}` (an
     empty `id` means the run is done),
   - `agent(prompt, ...)` -> runs the agent **in-process** via the Workflow tool,
   - `rigger_result` -> reports the agent's output by spawn id, unblocking the
     conductor's `Spawn`.

4. While an agent works it records each decision by calling the `rigger_emit` MCP
   tool (`{type, data}`), which appends straight to the event log - **live**, so
   the side-car and concurrent agents see it immediately. No file, no subprocess.

## The bridge protocol (MCP tools)

| Tool | In | Out / effect |
|---|---|---|
| `rigger_next` | (none) | `{id, prompt, model, tools, dir}`; `id:""` when nothing is queued |
| `rigger_result` | `{id, output, error?}` | unblocks the conductor's `Spawn` for `id` |
| `rigger_emit` | `{type, data}` | appends `{type, data}` to the event log, live |

## Validation boundary

The Go halves - the conductor, the bridge (`driver/workflow`), and the MCP server
(`mcpserver`) - are covered by `go test` (including an in-process test that drives
`Spawn` through `Next`/`Result`). The shim itself runs only inside the Claude Code
Workflow runtime, so it is validated there, not in `go test`.
