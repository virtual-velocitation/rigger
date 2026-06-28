# Workflow driver - the (B) Workflow-tool + MCP bridge

The in-Claude-Code agent driver. The Rust conductor stays the orchestrator; a thin
Claude Code **Workflow** shim drives the agents via `agent()`, and the two halves
talk over **MCP**. This keeps the Workflow tool's parallel/journal/resume while
the Rust core does the real work (the DAG, gates, ledger, graph).

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
   - `rigger_next` -> the next queued spawn `{id, prompt, model, tools, dir,
     blast_radius}` (an empty `id` means the run is done),
   - **tool-boundary injection (§5.3)** -> `rigger_peers` with the spawn's
     `blast_radius` as `files`, rendering the blast-radius-scoped peer decisions as
     a "peers context refresh" prepended to the prompt, so the agent starts aware of
     what concurrent agents decided about the files it is about to touch,
   - `agent(prompt, ...)` -> runs the agent **in-process** via the Workflow tool,
   - `rigger_result` -> reports the agent's output by spawn id, unblocking the
     conductor's `Spawn`.

4. While an agent works it records each decision by calling the `rigger_emit` MCP
   tool (`{type, data}`), which appends straight to the event log - **live**, so
   the side-car and concurrent agents see it immediately. No file, no subprocess.

5. **Continuous in-flight awareness.** The side-car keeps collecting peers'
   decisions for the whole run, so an agent should re-check `rigger_peers` (scoped
   to its `files`) *between its own actions*, not only at the start - the injection
   in step 3 is the tool boundary that guarantees it never starts blind, but the
   shared decision channel stays live the entire time it works.

## The bridge protocol (MCP tools)

| Tool | In | Out / effect |
|---|---|---|
| `rigger_next` | (none) | `{id, prompt, model, tools, dir, blast_radius}`; `id:""` when nothing is queued |
| `rigger_result` | `{id, output, error?}` | unblocks the conductor's `Spawn` for `id` |
| `rigger_emit` | `{type, data, meta?, valid_from?}` | appends the event to the log, live |
| `rigger_peers` | `{files?}` | the peers' decisions; scoped to `files` (the blast-radius) when given, else all |

## Validation boundary

The Rust halves - the conductor, the bridge (`driver/workflow`), and the MCP server
(`mcpserver`) - are covered by `cargo test` (including an in-process test that drives
`spawn` through `next`/`result`, and one that scopes `rigger_peers` by `files`). The
shim itself runs only inside the Claude Code Workflow runtime, so it is validated
there, not in `cargo test`.
