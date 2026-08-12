# 67 - The driver's progress tree tells the run's true story

**Goal:** make the /workflows progress display of a rigger run reflect what is actually
happening. Today it misleads on three counts, all in `workflows/rigger.js` (the embedded
template `rigger setup` installs, so every consumer sees this): (1) every `rigger step` courier
is pinned to `phase: 'Plan'` (rigger.js:531 and the global `phase('Plan')` at :484), so a
mature run shows dozens of "step#N" relay agents accumulating under Plan while the actual
planning finished hours earlier; (2) `meta.phases` declares Build / Review / Integrate
(rigger.js:50-55) that no agent can ever populate - workers spawn under runtime
`"<unit>:<stage>"` phase strings instead - so three declared phases render as permanently empty
rows; (3) `phaseOf` (rigger.js:179-181) builds the group title as `unit:stage` while the
conductor sets both halves to the unit id, so every unit group renders with its name duplicated
(`plan:plan`, `u1-dedup-seeding:u1-dedup-seeding`).

## Design

- **A dedicated orchestration lane** (`workflows/rigger.js`): the step couriers - the relay
  agents whose only job is to run `rigger step` and return the wave - spawn under a `Drive`
  phase (labels stay `step#N`), never under `Plan`. The global `phase('Plan')` marker is
  retired in favor of explicit per-spawn phases, since every spawn site already passes one.
- **Plan holds the planners** (`workflows/rigger.js::phaseOf`): wave workers whose unit is the
  plan or plan-critique stage group under the declared `Plan` title, so the Plan row counts
  actual planning agents (planner + critique tiers), not relays.
- **Unit groups render the unit** (`workflows/rigger.js::phaseOf`): when the wave item's unit
  and stage are equal (the conductor's current contract), the group title is the BARE unit id;
  when the conductor later distinguishes the stage half, the `unit:stage` form returns
  automatically. No group title ever repeats its own text.
- **meta tells the truth** (`workflows/rigger.js` meta block): `meta.phases` declares exactly
  the two statically-known groups - `Plan` (spec decomposition and its critique) and `Drive`
  (the courier relay lane) - each with a detail line, and the detail on `Drive` documents that
  per-unit groups appear dynamically as units start, each holding that unit's whole
  build-review lifecycle. No declared phase can be permanently empty.
- **Sidecar couriers stay with their unit** (decided here so no unit has to): the liveness
  probes and fault couriers that already ride `phase: ph` keep the worker's group - they are
  about that unit's spawn, and moving them to `Drive` would hide which unit they serve.

## Notes (non-criteria)

- Display-only: no conductor, event, wire-shape, or gate change of any kind. The
  `unit + stage` contract on the wave item is consumed as documented, not altered.
- The installed workflow refreshes through the existing drift-aware `rigger setup` path; the
  in-repo template is the one source of truth (`include_str!` at src/main.rs:129).
- No new event type is introduced anywhere in this spec.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to prior
  harnesses or to projects unrelated to the mechanism.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- The driver stays gate-blind and behavior-identical: only `opts.phase` strings, labels, and
  the meta block change; every spawn, courier retry rule, and stop path is byte-for-byte
  today's.
- `node --check` passes on the changed template (the existing JS syntax gate).

## Done when

- [ ] a test proves COURIERS RIDE THE DRIVE LANE: every step-courier spawn site in the
  template passes `phase: 'Drive'` and no spawn site passes the global-marker Plan pin,
  pinned on the template text the binary embeds. This criterion OWNS the courier lane.
- [ ] a test proves PLANNERS OWN PLAN: `phaseOf` maps a plan or plan-critique wave item to the
  declared `Plan` title. This criterion OWNS the planner mapping; the courier lane is
  criterion 1's, NOT this one's.
- [ ] a test proves UNIT GROUPS RENDER CLEAN: `phaseOf` yields the bare unit id when unit and
  stage are equal and `unit:stage` when they differ. This criterion OWNS the group title.
- [ ] a test proves META MATCHES REALITY: the meta block declares exactly the phases the
  driver populates statically (`Plan`, `Drive`), and the retired Build/Review/Integrate
  titles appear nowhere in it.
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`),
  including `node --check` on the template.
