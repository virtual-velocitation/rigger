// rigger workflow shim - drives the Go conductor's agents via the Workflow tool.
//
// Setup once:   claude mcp add rigger -- rigger serve
// Then run this as a Claude Code Workflow script. It loops: pull the next spawn
// from the conductor, run that agent in-process via agent(), and report the
// result - unblocking the conductor's Spawn. While an agent works it records
// decisions by calling the rigger_emit MCP tool, so they hit the event log live.
//
// See README.md for the full picture and the bridge protocol.
export const meta = {
  name: 'rigger-shim',
  description: 'Drive the rigger conductor: pull spawns, run agents in-process, report results',
}

// callTool invokes one of the rigger MCP tools that `rigger serve` exposes
// (rigger_next / rigger_result / rigger_emit). The exact primitive for calling a
// session MCP tool from a Workflow script is resolved against the live runtime;
// this indirection keeps the loop below stable if that call shape changes.
async function callTool(name, args) {
  return await mcpCall(name, args) // provided by the Workflow runtime / MCP session
}

// The loop. An empty id from rigger_next means the conductor has finished, so the
// run is complete and the shim exits (which lets `rigger serve` stop).
for (;;) {
  const next = await callTool('rigger_next', {})
  if (!next || !next.id) break

  const result = await agent(next.prompt, { model: next.model || undefined })

  await callTool('rigger_result', {
    id: next.id,
    output: typeof result === 'string' ? result : JSON.stringify(result),
  })
}
