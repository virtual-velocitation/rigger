# Sonnet writers

This instruction is the measurement of the fan-out's value, and a measurement with an exit
measures the exit: so every rule below is absolute, with no size, kind or urgency of change
below which it lapses.

It binds the IMPLEMENTER of a unit: the persona producing a change in a unit worktree. Every
reviewer holds the implementer to it through the measurement line (see the last section) and
otherwise judges the diff exactly as before.

## The division
You decide and specify; a writer types. Every line that enters the unit's tree is typed by a
writer: production code, tests (the failing test that goes red first among them), fixtures,
docs and comments. Regenerating a generated file by running its generator is not typing; that
is a command, and it stays with `verify`.

You type NOTHING into the tree yourself: no Edit, no Write, no NotebookEdit, no shell
redirection or heredoc into a file under the worktree. You DECIDE and SPECIFY: every interface
(types, traits, signatures, error shapes, event payloads), the failing test's assertions, and
any body where the body IS the decision (concurrency, transaction boundaries, anything the
spec's Design decides) go VERBATIM into the writer's brief, and the writer types them.

Red first stays your decision and the writer's typing: the failing test is the first writer
dispatch, `verify` runs it red, then the writers for the fix are dispatched.

There is no size below which you type. A change you cannot specify in a brief is a change you
have not yet decided: decide it, then brief it.

## One writer per concern, never per file
A concern is one coherent change, named by the symbols (graph nodes) or the test group it
covers, with the files it may touch. Two writers never hold the same file at once. This is the
built-in rule that a fan-out is over nodes, never over files, applied to writing. Independent
concerns are dispatched in ONE message so they run in parallel; the built-in cap on subagents
in flight holds.

## The brief is the writer's whole world
Give it: the signatures, assertions and decided bodies verbatim, the pattern or example to
follow, the files it may touch, what done looks like, and the rules that bind code here -
match the surrounding idiom; hyphens, never em dashes; no attribution of any kind; no new
dependency; no lint allowance; never weaken or delete a test to pass. It runs NO build, NO test
and NO git command. When the contract cannot be met it stops and reports; it never changes the
contract.

Dispatch: the Agent tool with `subagent_type: "general-purpose"` and `model: "sonnet"`, the
brief as `prompt`, `run_in_background: false`.

Builds and tests stay with `verify`, one at a time, after every writer of the step has
returned; you never run them yourself and a writer never runs them.

## You answer for every line
Read the whole diff before you hand the unit off; re-brief a writer for whatever is wrong,
never type the correction yourself; check the diff for duplication against the tree as you
would your own work. A reviewer holds you to the diff as if you typed it.

## The measurement line
Your result carries one plain-text line, never inside a JSON line: `writers: <count>` followed
by one clause per writer saying what it wrote. A unit whose diff is empty (a verification-only
criterion) reports `writers: 0`, and that is correct.

A reviewer enforces it. A unit whose diff is non-empty and whose result reports `writers: 0`,
or carries no `writers:` line, broke this instruction: the reviewer rejects it with the item
"implementer typed into the tree; the writers instruction binds" (cause: process).
