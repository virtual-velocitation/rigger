// rigger workflow shim - drives the Rust conductor's agents via the Workflow tool.
//
// Setup once:   claude mcp add rigger -- rigger serve
// Then run this as a Claude Code Workflow script. It loops: pull the next spawn
// from the conductor, run that agent in-process via agent(), and report the
// result - unblocking the conductor's spawn. While an agent works it records
// decisions by calling the rigger_emit MCP tool, so they hit the event log live.
//
// The tool-boundary injection (§5.3). MCP is request/response, so the side-car
// cannot push into a running agent; the shim performs the injection instead. Right
// before each agent runs - its tool boundary - the shim fetches the peer decisions
// scoped to that spawn's blast-radius (rigger_peers with the spawn's `files`) and
// prepends them as a "peers context refresh", so the agent starts already aware of
// what concurrent agents decided about the files it is about to touch. An agent
// should ALSO re-check rigger_peers between its own actions for continuous in-flight
// awareness (the side-car keeps collecting peers' decisions for the whole run).
//
// See README.md for the full picture and the bridge protocol.
export const meta = {
  name: 'rigger-shim',
  description: 'Drive the rigger conductor: pull spawns, run agents in-process, report results',
}

// callTool invokes one of the rigger MCP tools that `rigger serve` exposes
// (rigger_next / rigger_result / rigger_emit / rigger_peers). The exact primitive
// for calling a session MCP tool from a Workflow script is resolved against the live
// runtime; this indirection keeps the loop below stable if that call shape changes.
async function callTool(name, args) {
  return await mcpCall(name, args) // provided by the Workflow runtime / MCP session
}

// peersRefresh fetches the peer decisions scoped to a spawn's blast-radius and
// renders them as a context-refresh block to prepend to the agent's prompt. This is
// the tool-boundary injection: the agent reads it before its first action. An empty
// blast-radius means "no scope" and returns every decision; no decisions yields "".
async function peersRefresh(blastRadius) {
  // Pass the spawn's blast-radius as `files` so rigger_peers returns only decisions
  // that touch files this agent is about to work on (§5.3).
  const res = await callTool('rigger_peers', { files: blastRadius || [] })
  const decisions = (res && res.decisions) || []
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

// The loop. An empty id from rigger_next means the conductor has finished, so the
// run is complete and the shim exits (which lets `rigger serve` stop).
for (;;) {
  const next = await callTool('rigger_next', {})
  if (!next || !next.id) break

  // Tool-boundary injection: fetch the blast-radius-scoped peer decisions and
  // prepend them so the agent starts aware of its peers (§5.3).
  const refresh = await peersRefresh(next.blast_radius)
  const prompt = refresh ? `${refresh}\n${next.prompt}` : next.prompt

  const result = await agent(prompt, { model: next.model || undefined })

  await callTool('rigger_result', {
    id: next.id,
    output: typeof result === 'string' ? result : JSON.stringify(result),
  })
}
