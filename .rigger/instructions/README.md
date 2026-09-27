# Operator instructions

Every `*.md` file in this directory (except this README) is appended, in filename order,
to the system prompt of every agent rigger spawns: after the agent's persona and the
built-in engineering principles, before rigger's communication discipline. Name files with
a numeric prefix (`10-house.md`, `20-team.md`) to control their order.

Run `rigger instructions` to read exactly what your agents are held to.

These files are part of a run's definition pin: editing one mid-run halts the next step
as a definition drift. Edit them between runs, or continue with `--rebase-definition`.
