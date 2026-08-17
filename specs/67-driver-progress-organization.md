# 67 - The driver's progress tree groups by lifecycle phase and speaks human

**Goal:** make the /workflows progress display legible to a human who has NOT read the
execution plan. Three failures, all in `workflows/rigger.js` (the embedded template every
consumer installs): (1) every step courier is pinned to `phase: 'Plan'` (rigger.js:531, the
global `phase('Plan')` at :484), so a mature run shows dozens of `step#N` rows under a phase
that finished hours ago; (2) workers group under runtime `<unit>:<stage>` slugs (`phaseOf`,
:179-181) meaningless without inner knowledge, while the declared meta phases sit empty;
(3) rows lead with the opaque spawn id even though the wave item carries a human sentence
(`SpawnRequest.title`, on the wire since spec 19; :321 buries it). Target: groups are the
lifecycle meta phases (Plan / Build / Review, plus a Drive lane for relays); each row is one
terse human sentence with role and attempt as metadata - never a slug.

## Design

- **Meta phases are the groups** (`workflows/rigger.js::phaseOf`): phase derives from ROLE
  (the deterministic spawn-id format `<stage>/<role>#<attempt>`, spec 18, plus stage
  fields): plan and plan-critique map to `Plan`; implementer and every authoring role to
  `Build`; lens / adversary / adjudicator to `Review`; an unrecognized role defaults to
  `Build` (fail-visible in the label, never a dropped row). Per-unit groups are retired.
- **A dedicated orchestration lane**: step couriers spawn under `Drive` (labels stay
  `step#N`); the global `phase('Plan')` marker is retired. Sidecar couriers (liveness
  probes, fault recorders) ride their worker's phase.
- **Rows lead with the persona, then its verb** (label builders): a worker's label is
  `<Persona> - <action phrase> #<attempt>: <subject>`, e.g.
  `Lens:SDET - evaluate testing effectiveness #2: seed ingest dedup keys project-wide`.
  Persona is title-cased from the id's role half (`Implementer`, `Lens:SDET`, `Adversary`,
  `Adjudicator`, `Plan-Critique`); the action phrase states the persona's MANDATE (the
  criterion sentence alone would render every tier of one unit identically). Role map:
  implementer "implement"; sdet-author "author the discriminating tests for"; sdet lens
  "evaluate testing effectiveness"; architecture lens "evaluate architectural integrity";
  adversary "challenge the findings, assumptions, and rigor of <roster>" (its mandate is to
  DISPROVE the other agents' work); adjudicator "weigh <roster> and rule"; plan "decompose
  the spec into a unit DAG"; plan-critique "critique the decomposition". Unknown roles keep
  their persona token with the generic "review:" verb - readable, never a slug.
- **Review tiers name their targets** (`src/conductor.rs` review-spawn seam + the driver):
  `<roster>` is the actual persona set that tier judges, stamped by the CONDUCTOR on the
  review-tier wave item as one additive, serde-defaulted field (e.g.
  `reviews: ["lens:sdet","lens:architecture-reviewer"]`; the adjudicator's roster also
  carries the adversary). The conductor is the only honest source (`review.tiers` varies
  the roster per unit; a driver guess would miss a replayed lens). A roster-less item
  renders the phrase without the parenthetical - graceful both directions, never a wrong
  roster. The subject is `SpawnRequest.title` reduced to its FIRST SENTENCE,
  whitespace-normalized, passed WHOLE: the driver never truncates and never appends an
  ellipsis - clipping is the display's job. Only an untitled spawn falls back to the id.
- **meta tells the truth**: `meta.phases` declares exactly `Plan`, `Build`, `Review`,
  `Drive`, each with a detail line. `Integrate` is dropped (conductor work, spawns no
  agent; its detail moves onto `Review`: an approved unit integrates automatically).

## Notes (non-criteria)

- Display-first with ONE additive exception: the optional `reviews` roster stamp
  (serde-defaulted, additive on the existing payload - not a new event type, no gate or
  stop-path change). Everything else consumes `SpawnRequest.title` as delivered.
- The unit stays visible inside the sentence; the grouping dimension changes from unit to
  lifecycle phase by explicit preference.
- The installed workflow refreshes through the drift-aware `rigger setup` path; the in-repo
  template is the one source (`include_str!` at src/main.rs:129).
- No new event type is introduced anywhere in this spec.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to prior
  harnesses or to projects unrelated to the mechanism.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- The driver stays gate-blind and behavior-identical: only `opts.phase` strings, labels, and
  the meta block change; every spawn, retry rule, and stop path is byte-for-byte today's.
  Conductor-side, the `reviews` stamp is the ONE change: additive, optional, read by nothing
  in the conductor itself.
- `node --check` passes on the changed template.

## Done when

- [ ] a test proves the PHASE MAPPING: plan and plan-critique items map to `Plan`, an
  implementer item to `Build`, and lens / adversary / adjudicator items to `Review`, with an
  unrecognized role defaulting to `Build` - pinned on the template's mapping function. This
  criterion OWNS phase derivation; courier placement is criterion 3's, NOT this one's.
- [ ] a test proves ROWS LEAD WITH THE PERSONA: a titled wave item's label renders
  `<Persona> - <action phrase> #<attempt>: <subject>` (persona title-cased from the id's
  role half; subject is the title's first sentence, whitespace-normalized, passed whole with
  no driver-side truncation or ellipsis), two different tiers of the SAME unit render
  distinct persona tokens AND distinct action phrases, an unmapped custom lens keeps its
  persona token with the generic "review:" verb, and an untitled item falls back to the
  spawn id. This criterion OWNS the label.
- [ ] a test proves COURIERS RIDE THE DRIVE LANE: every step-courier spawn site passes
  `phase: 'Drive'`, no global phase marker remains, and sidecar couriers pass their worker's
  phase. This criterion OWNS courier placement.
- [ ] a test proves REVIEW TIERS NAME THEIR TARGETS: the conductor stamps the adversary's
  wave item with the unit's routed lens roster and the adjudicator's with lenses plus
  adversary, the driver renders the roster inside the action phrase, and a roster-less item
  (an older conductor) renders the phrase without a parenthetical - never a fabricated or
  stale roster. This criterion OWNS the roster stamp and its render.
- [ ] a test proves META MATCHES REALITY: the meta block declares exactly `Plan`, `Build`,
  `Review`, `Drive`, and no `Integrate` phase or `unit:stage` group construction survives in
  the template.
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`),
  including `node --check` on the template.
