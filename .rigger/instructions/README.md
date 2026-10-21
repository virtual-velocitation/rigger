# Operator instructions

Rigger composes the system prompt of every agent it spawns from the agent's persona, then the
built-in instructions rigger ships, then the operator files in this directory. The built-ins,
in the order they are injected, each named by the file that overrides it:

- `engineering-principles.md`
- `working-discipline.md`
- `communication.md`

A file in this directory named exactly as a built-in (case-sensitive, nothing trimmed) replaces
that built-in: its body is injected in the built-in's place. A same-named file whose body is
blank (empty or whitespace only, one leading byte-order mark ignored) switches the built-in
off: neither the built-in nor the file is injected.

Every other `*.md` file in this directory (except this README) is appended after the
built-ins, in filename order. Name files with a numeric prefix (`10-house.md`, `20-team.md`)
to control their order.

Run `rigger instructions` to read exactly what your agents are held to.

These files are part of a run's definition pin: editing one mid-run halts the next step
as a definition drift. Edit them between runs, or continue with `--rebase-definition`.
