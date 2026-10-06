# Haiku lookups

This instruction is the other half of the measurement of the fan-out's value, and a
measurement with an exit measures the exit: so every rule below is absolute.

It binds every persona that reads the tree: the implementer, the planner, the reviewers, the
adversary and the SDET. It tightens the built-in fan-out rule: where that rule lets you consult
the graph yourself first, here you do not answer a question yourself at all.

## Every question is a lookup
Every question about the project is answered by dispatching the `lookup` subagent: where a
symbol is defined or used, what a decision or criterion says, which tests cover a node, what a
file's neighbourhood looks like. You call no `mcp__rigger__*` graph tool and no Read, Grep or
Glob to ANSWER a question.

Dispatch: the Agent tool with `subagent_type: "lookup"` and `model: "haiku"`, the node and the
question as `prompt`, `run_in_background: false`.

## One dispatch per graph node, never per file
Each dispatch carries ONE question about ONE graph node: an entity, a decision, a criterion or
a finding. Independent nodes are dispatched in ONE message so they run in parallel; the
built-in cap on subagents in flight holds. Never one dispatch per file, and never a sweep that
asks one lookup about many nodes.

The `lookup` agent's own contract agrees: it answers exactly one question about exactly one
node, through the graph first; it reads a file only to confirm a line the graph named; a
literal search is its last resort, used only after the graph returned nothing, and every such
use is reported in a line beginning `GAP:` naming what the graph could not answer.

## Verify, never replace
You may Read a file only to VERIFY a lookup's answer before you decide on it. A lookup whose
answer carries a `GAP:` line is a question the graph could not answer: record it as the
built-in rule says, and carry it verbatim, with its question, in your result. It is never
silently replaced by a search of your own.

## The measurement line
Your result carries one plain-text line, never inside a JSON line: `lookups: <count>`, beside
the `writers:` line where your result has one. When any lookup reported a gap, the same line
adds `lookup gaps: <n>` followed by each gap's question and its `GAP:` line verbatim.
