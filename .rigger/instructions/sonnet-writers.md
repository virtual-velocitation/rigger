# Sonnet writers

This binds the IMPLEMENTER of a unit: the persona writing code in a unit worktree. Every other
persona reads it only as how a diff may have been produced, and judges that diff as before.

## The division
You decide and specify; a writer types. Write these yourself: every interface (types, traits,
signatures, error shapes, event payloads); the failing tests that define the behaviour - red
first stays yours; and any body where the body IS the decision (concurrency, transaction
boundaries, anything the spec's Design decides).

A writer may be handed: a body behind a signature and tests you already wrote; re-expressing a
group of existing tests to a pattern you wrote once in full; a rename or signature change
carried through its call sites; fixtures; a doc or comment passage whose content you dictate.

## One writer per concern, never per file
A concern is one coherent change, named by the symbols (graph nodes) or the test group it
covers, with the files it may touch. Two writers never hold the same file at once. This is the
built-in rule that a fan-out is over nodes, never over files, applied to writing.

## The brief is the writer's whole world
Give it: the signatures verbatim, the pattern or example to follow, the files it may touch,
what done looks like, and the rules that bind code here - match the surrounding idiom; hyphens,
never em dashes; no attribution of any kind; no new dependency; no lint allowance; never weaken
or delete a test to pass. It runs NO build, NO test and NO git command. When the contract
cannot be met it stops and reports; it never changes the contract.

Dispatch: the Agent tool with `subagent_type: "general-purpose"` and `model: "sonnet"`, the
brief as `prompt`, `run_in_background: false`; independent concerns go in one message.

Builds and tests stay with `verify`, one at a time, after every writer has returned.

## You answer for every line
Read the whole diff before you hand the unit off; fix or re-brief what is wrong; check it for
duplication against the tree as you would your own typing. A reviewer holds you to the diff as
if you typed it.

Do not use a writer for a change small enough to type directly, or for anything you cannot
specify in a few sentences. The built-in cap on subagents in flight still holds.

## The measurement line
Your result carries one plain-text line, never inside a JSON line: `writers: <count>` followed
by one clause per writer saying what it wrote, or `writers: 0` when you used none.
