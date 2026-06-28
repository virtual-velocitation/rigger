# golden-apple: the worked example

The architecture (`docs/architecture.md` §10, §11) demotes the tank_game dev-loop -
its review lenses, its cargo/e7 gates, its planner - from machinery to *content*.
This directory is that content: a realistic Rigger setup expressed entirely as
config, with Rigger itself knowing none of it.

```
examples/golden-apple/
└── .rigger/
    ├── workflow.yml        the plan -> implement -> review -> integrate DAG
    └── agents/
        ├── planner.md              decomposes the spec into a unit DAG
        ├── implementer.md          isolation: worktree, recurse: false
        ├── reviewer.architecture.md
        ├── reviewer.technical.md   the review lenses
        ├── reviewer.game-design.md
        ├── devils-advocate.md      the adversarial adjudicator
        └── integrator.md           rebase, re-gate, land
```

## What it demonstrates

- A **`defaults:`** block (autonomy + grounder).
- A reusable **`gates:`** library running real commands: `cargo build`,
  `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check`.
- A **producer** stage (`produces: dag`) that decomposes the spec at runtime - the
  living-DAG / spawnUnit mechanic.
- A **fan-out** implement stage (`strategy: fan-out`, `partition: by-blast-radius`)
  with an `isolation: worktree`, `recurse: false` implementer - safe parallelism,
  runaway-proof by construction.
- A **fan-out review** stage with three lenses plus a **`devils-advocate`**
  adjudicator whose verdict gates the stage, under `autonomy: manual`.
- An **integrate** stage with `on_pass: merge`.

## Run it

```bash
cd examples/golden-apple
rigger validate              # load + validate the workflow + agents
rigger run path/to/spec.md   # run the producing loop on a spec
```

The `config::load("examples/golden-apple")` path is covered by a test in
`src/config.rs` (`golden_apple_example_loads`), so the example never rots.
