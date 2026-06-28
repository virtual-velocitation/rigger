# demo: the worked example

The architecture (`docs/architecture.md` §10, §11) treats a project's whole
dev-loop - its review lenses, its build/test gates, its planner - as *content*,
not machinery. This directory is that content for a small, fictional project (a
tiny URL-shortener library): the config a real repo would commit so Rigger can
drive it.

```
examples/demo/
  README.md
  .rigger/
    workflow.yml                  the producing-loop DAG (plan -> implement)
    agents/
      planner.md                  decomposes the spec into a unit DAG
      implementer.md              implements one unit in an isolated worktree
      reviewer.architecture.md    tier-1 lens: module boundaries / layering
      reviewer.correctness.md     tier-1 lens: logic / error-handling / tests
      reviewer.api-design.md      tier-1 lens: public surface / ergonomics
      adversary.md                tier-2: refutes the lenses, surfaces what they missed
      adjudicator.md              tier-3: neutral final judge; verdict gates integration
```

Review and integration are PER UNIT (`defaults.review` + the `implement` stage),
not separate downstream stages: each implementer unit runs its own lifecycle -
ground -> implement (red/green TDD in a worktree) -> the unit's gates -> the
three-tier review of that unit (lenses -> adversary -> adjudicator) -> integrate -
and only an adjudicator-approve on a green workspace merges (`on_pass: merge`).

To run it against a spec from this directory:

```
cd examples/demo
rigger run <spec>
```

The `config::load("examples/demo")` path is covered by a test in
`src/config.rs` (`demo_example_loads`), so the example never rots.
