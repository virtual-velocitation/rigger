# 67 - The driver's progress tree groups by lifecycle phase and speaks human

**Goal:** make the /workflows progress display of a rigger run legible to a human who has NOT
read the execution plan. Today it fails that reader three ways, all in `workflows/rigger.js`
(the embedded template `rigger setup` installs, so every consumer sees this): (1) every
`rigger step` courier is pinned to `phase: 'Plan'` (rigger.js:531 and the global
`phase('Plan')` at :484), so a mature run shows dozens of "step#N" relay agents under a phase
that finished hours earlier; (2) workers group under runtime `"<unit>:<stage>"` strings
(`phaseOf`, rigger.js:179-181) - slugs that are meaningless without inner knowledge of this
specific plan, forcing the reader all the way into the prompt to learn what an agent is doing -
while the declared Build / Review / Integrate meta phases sit permanently empty; (3) each row's
label leads with the same opaque spawn id (`u1-dedup-seeding/lens:sdet#2`) even though the wave
item already carries a human work sentence (`SpawnRequest.title`, the unit's criterion, on the
wire since spec 19 - rigger.js:134 accepts it and :321 buries it behind the id).

The clarified target: the GROUPS are the lifecycle meta phases (Plan / Build / Review, plus a
Drive lane for orchestration relays), and within a phase each row is one terse human sentence
describing the work, with the role and attempt as suffix - never a slug a human cannot read.

## Design

- **Meta phases are the groups** (`workflows/rigger.js::phaseOf`): a wave item's phase derives
  from its ROLE (the deterministic spawn-id format `<stage>/<role>#<attempt>`, spec 18, plus
  the item's stage fields): plan and plan-critique stages map to `Plan`; the implementer and
  every authoring role map to `Build`; the three review tiers (lens, adversary, adjudicator)
  map to `Review`; a role the mapping does not recognize defaults to `Build` (fail-visible in
  the label, never a dropped row). Per-unit groups are retired.
- **A dedicated orchestration lane**: the step couriers spawn under `Drive` (labels stay
  `step#N`); the global `phase('Plan')` marker is retired in favor of explicit per-spawn
  phases. Sidecar couriers (liveness probes, fault recorders) ride their worker's phase, so
  they appear beside the work they serve.
- **Rows speak human** (`workflows/rigger.js`, the label builders): a worker's label is the
  work sentence first, role and attempt after: `<terse title> · <role>#<attempt>`, where the
  title is the wave item's `SpawnRequest.title` compressed to one terse line (first sentence,
  whitespace-normalized, hard length cap with an ASCII ellipsis). Only an untitled spawn falls
  back to the spawn id. The role token is the human word (`implementer`, `lens:sdet`,
  `adversary`, `adjudicator`), parsed from the id the conductor already formats.
- **meta tells the truth** (`workflows/rigger.js` meta block): `meta.phases` declares exactly
  the four populatable groups - `Plan`, `Build`, `Review`, `Drive` - each with a detail line.
  `Integrate` is dropped: integration is conductor work that spawns no agent, so it can never
  populate a group (its detail line moves onto `Review`: an approved unit integrates
  automatically).

## Notes (non-criteria)

- Display-only: no conductor, event, wire-shape, or gate change. `SpawnRequest.title` is
  consumed as already delivered; nothing new rides the wire.
- The unit a row belongs to remains visible inside its sentence (a criterion sentence names
  its subject); the grouping dimension changes from unit to lifecycle phase by explicit
  preference.
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

- [ ] a test proves the PHASE MAPPING: plan and plan-critique items map to `Plan`, an
  implementer item to `Build`, and lens / adversary / adjudicator items to `Review`, with an
  unrecognized role defaulting to `Build` - pinned on the template's mapping function. This
  criterion OWNS phase derivation; courier placement is criterion 3's, NOT this one's.
- [ ] a test proves ROWS SPEAK HUMAN: a titled wave item's label renders
  `<terse title> · <role>#<attempt>` (first sentence, normalized, capped with an ASCII
  ellipsis) and an untitled item falls back to the spawn id. This criterion OWNS the label.
- [ ] a test proves COURIERS RIDE THE DRIVE LANE: every step-courier spawn site passes
  `phase: 'Drive'`, no global phase marker remains, and sidecar couriers pass their worker's
  phase. This criterion OWNS courier placement.
- [ ] a test proves META MATCHES REALITY: the meta block declares exactly `Plan`, `Build`,
  `Review`, `Drive`, and no `Integrate` phase or `unit:stage` group construction survives in
  the template.
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`),
  including `node --check` on the template.
