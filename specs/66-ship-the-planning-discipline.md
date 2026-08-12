# 66 - Ship the planning discipline: skill installed by setup, guide in the handbook, lint in validate

**Goal:** the spec-planning discipline distilled from this repository's failure record must reach
every consumer of the binary, not live as loose files in this repo. The content exists
(`docs/handbook/planning-field-guide.md`, and the committed skill source at
`skills/planning-a-spec/SKILL.md` - the same committed-source location the `using-rigger` render
uses, with `.claude/skills/` remaining the gitignored install target) and the
distribution machinery exists (the `using-rigger` skill is rendered from binary-embedded content,
installed by `rigger setup` under `.claude/skills/`, and drift-gated so the committed copy cannot
disagree with the code). This spec makes the planning discipline a first-class citizen of that
same machinery, on three surfaces: a skill agents load, a handbook page humans read, and the
mechanical subset of the recipe as a spec lint every operator can run before launching.

## Design

- **The skill ships via setup** (`src/main.rs`, the `install_skill` seam; `src/docs.rs`): the
  `planning-a-spec` skill becomes a binary-embedded render exactly like `using-rigger` - one
  source of truth in the code, written by `rigger docs`, installed (or drift-refreshed) by
  `rigger setup` at `.claude/skills/planning-a-spec/SKILL.md`, honoring the same per-repo
  overlay mechanism so a consumer can append project specifics without forking the render. The
  committed copy in THIS repo becomes the rendered artifact and joins the existing docs-drift
  gate, so it can never silently disagree with what consumers receive.
- **The guide ships in the handbook** (`src/docs.rs`): `docs/handbook/planning-field-guide.md`
  becomes a rendered handbook page under the same drift gate, cross-linked from
  `authoring-loops.md` (whose rules it operationalizes). Content is the failure catalog as
  committed - evidence-based classes F1-F8, the mid-run amendment protocol, and the measured
  outcomes - with wording kept self-contained (a consumer reading it needs no access to this
  repo's history).
- **The mechanical recipe becomes a lint** (`src/main.rs::cmd_validate`, reusing the loop's
  existing spec-shape lint as the ONE authority - never a second parser): `rigger validate
  <spec.md>` reports, per criterion, the machine-checkable planning defects BEFORE a run is
  launched, as warnings (advisory, exit 0 - judgment stays with the author):
  - shape: multi-behavior, over-long, or sub-bullet criteria (the existing lint's classes,
    surfaced pre-launch instead of only inside the loop);
  - ownership: in a spec with three or more criteria, criteria that carry no ownership
    sentence (no OWNS/owner language inside the checkbox) are listed as twin-risk;
  - open dispositions: draft-smell phrases ("worth considering", "either ... or", "could
    instead") OUTSIDE a Notes section are listed as re-litigation risk;
  - hygiene: U+2014 em dashes anywhere in the file (the diff gate will fail them later;
    validate says so now).
  Each warning names the criterion and the field-guide class it maps to, so the fix is one
  lookup away.
- **Discoverability** (`src/main.rs`, `rigger prime` / the workflow's launch path): the
  pre-launch surfaces that already tell an operator what to do next mention the spec lint when
  a spec path is in play, so the tool is found at the moment it is useful, not from
  documentation alone.

## Notes (non-criteria)

- The skill and guide texts are content, not code: units embed the committed texts (normalizing
  only what the render pipeline requires) rather than rewriting them; wording changes are
  drift-gate-visible.
- The lint is advisory by design: a spec is judgment plus mechanics, and the plan-critique gate
  remains the binding reviewer of decompositions. The lint exists so the mechanical classes
  (F1-shape, F2, F4-smells, F6) are caught for free before any tokens are spent.
- No new event type is introduced anywhere in this spec.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to prior
  harnesses or to projects unrelated to the mechanism.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- One lint authority: the pre-launch spec lint and the loop's in-run spec-shape lint are the
  same code, so they can never disagree about what a well-shaped criterion is.
- The docs-drift gate stays green: every rendered artifact (skill, handbook page) matches its
  committed copy after the change, and `rigger setup` remains non-destructive on rerun.

## Done when

- [ ] a test proves the SKILL INSTALLS: `rigger setup` in a fresh repo writes
  `.claude/skills/planning-a-spec/SKILL.md` from the binary-embedded render, honors the
  project overlay, and a rerun is drift-aware (refresh reported, never a destructive clobber).
  This criterion OWNS the skill's render-and-install path.
- [ ] a test proves the HANDBOOK PAGE RENDERS: `rigger docs` writes the planning field guide
  handbook page, `authoring-loops.md` links to it, and the docs-drift gate holds over both.
  This criterion OWNS the guide's render; the skill install is criterion 1's, NOT this one's.
- [ ] a test proves the SPEC LINT: `rigger validate <spec>` on a fixture spec containing a
  multi-behavior criterion, an ownerless criterion among three-plus, a disposition smell
  outside Notes, and an em dash reports each with its criterion and field-guide class, exits 0,
  and reports a clean fixture clean. This criterion OWNS the pre-launch lint surface.
- [ ] a test proves ONE LINT AUTHORITY: the pre-launch lint and the loop's in-run spec-shape
  lint share the single implementation, pinned so a divergence cannot compile or cannot pass.
- [ ] a test proves DISCOVERABILITY: the pre-launch surface that names next steps mentions the
  spec lint when given a spec path.
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
