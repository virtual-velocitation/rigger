# 66 - Ship the planning discipline: skill installed by setup, guide in the handbook, lint in validate

**Goal:** the spec-planning discipline must reach every consumer of the binary, not live as
loose files in this repo. The content exists (`docs/handbook/planning-field-guide.md`,
`skills/planning-a-spec/SKILL.md`) and the distribution machinery exists (binary-embedded
renders, `rigger setup` install, drift gate). Three surfaces: a skill agents load, a handbook
page humans read, and the mechanical subset of the recipe as a pre-launch spec lint.

## Design

- **Lint-heuristic semantics, decided here** (round-5 disposition closing the defect class
  four review rounds found one corner at a time): every cue word the spec lint matches
  (`either`, `or`, `owns`, `owner`, denial phrases) matches as a STANDALONE WORD - one
  shared word-boundary matcher, never a bare `contains` - with hyphens word-forming.
  Within one checkbox an AFFIRMATIVE ownership match takes precedence over a denial
  phrase (a criterion that says "OWNS X; Y is criterion 2's, NOT this one's" carries
  ownership; the denial half never vetoes it). The acceptance property, made precise
  (round-6 sharpening - "zero false findings over all historical specs" is unjudgeable
  by machine, since old specs may carry TRUE smells): (1) SELF-CLEAN NARROW, proven by
  test: spec 66 itself raises zero lint findings at the unit's HEAD; (2) a LABELED
  FIXTURE CORPUS, proven by test: true positives fire and boundary-adjacent negatives
  (substring-inside-word, hyphenated forms, denial-beside-affirmative) stay silent.
  Historical `specs/*.md` are NOT a zero-findings corpus; a finding there is advisory
  output, not a test failure.
- **Quote-masking fails closed, decided here** (closing the stray-delimiter class rounds
  kept re-finding one shape at a time): the lint's one invariant is that quoted or named
  text can NEVER false-positive; advisory recall is expendable, the invariant is not. So
  per delimiter kind within a paragraph, the mask is ONE span with no nearest-closer
  logic at all (nearest-closer is where every recurrence lived - stray marks, embedded
  digit-adjacent marks, twin spans): an EVEN count of that kind masks from its FIRST
  mark through its LAST mark; an ODD count masks from its FIRST mark to the paragraph
  end. Any genuinely quoted text lies after some opener of its kind, so the invariant
  holds under any arrangement of strays, units marks, or multiple spans; unquoted prose
  between spans is over-masked, which is expendable recall. Tests pin each direction: a
  stray before a real quote still masks the quote, a digit-adjacent mark inside a span
  cannot close it early, two spans in one paragraph mask through both, and a balanced
  paragraph still lints its unquoted hedge outside the marks.
- **The skill ships as a registry entry** (`src/main.rs`, `src/docs.rs`): `planning-a-spec`
  becomes a binary-embedded render registered in the spec-68 SKILL REGISTRY (spec 68 runs
  FIRST). Drift, overlay, and non-destructive install are the registry's contract, owned by
  spec 68; this spec owns only the entry and its content.
- **The guide ships in the handbook** (`src/docs.rs`): the planning field guide becomes a
  rendered handbook page under the same drift gate, cross-linked from `authoring-loops.md`.
  Content as committed (failure classes F1-F8, the mid-run amendment protocol, measured
  outcomes), self-contained - a consumer needs no access to this repo's history.
- **The mechanical recipe becomes a lint** (`src/main.rs::cmd_validate`, reusing the loop's
  existing spec-shape lint as the ONE authority - never a second parser):
  `rigger validate <spec.md>` warns per criterion (advisory, exit 0):
  - shape: multi-behavior, over-long, or sub-bullet criteria (the existing lint's classes,
    surfaced pre-launch);
  - ownership: with three-plus criteria, checkboxes carrying no OWNS/owner sentence are
    listed as twin-risk;
  - open dispositions: draft-smell phrases ("worth considering", "either ... or", "could
    instead") outside Notes - scanned in prose only, never inside fenced code blocks or
    inline code spans, so quoted code cannot false-positive;
  - hygiene: U+2014 em dashes anywhere (the diff gate will fail them later; validate says so
    now).
  Each warning names the criterion and its field-guide class.
- **Discoverability** (`src/main.rs`, `rigger prime` / the workflow's launch path): the
  pre-launch surfaces that name next steps mention the spec lint when a spec path is in play.
  REMINDER DEDUP, decided here (closing the ad-hoc-mechanism class: a bare env sentinel
  leaks across unrelated process trees, a printed-once static leaks across in-process
  calls): a nesting surface that has already printed the reminder passes the suppression
  DOWN as an env variable whose VALUE is its own process id, and a child honors the
  suppression ONLY when that value parses and equals its direct parent process id
  (`std::os::unix::process::parent_id`); any absent, foreign, stale, or malformed value
  means the surface prints. State lives in the explicit parent-to-child contract, never
  ambient presence - a test pins both directions (nested invocation suppressed, ambient
  pollution from an unrelated tree still prints).

## Notes (non-criteria)

- The skill and guide texts are content, not code: units embed the committed texts
  (normalizing only what the render pipeline requires); wording changes are
  drift-gate-visible.
- The lint is advisory by design: the plan-critique gate remains the binding reviewer of
  decompositions; the lint catches the mechanical classes for free before tokens are spent.
- No new event type is introduced anywhere in this spec.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to prior
  harnesses or to projects unrelated to the mechanism.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- One lint authority: the pre-launch lint and the loop's in-run spec-shape lint are the same
  code.
- The docs-drift gate stays green over every rendered artifact; `rigger setup` remains
  non-destructive on rerun.

## Done when

- [ ] a test proves the SKILL IS A REGISTRY ENTRY: the `planning-a-spec` render is enumerated
  by the skill registry, so `rigger setup` installs it and `rigger docs` renders it through
  the registry's own paths with no planning-specific install code. This criterion OWNS the
  entry and its content; registry mechanics are spec 68's, NOT this spec's.
- [ ] a test proves the HANDBOOK PAGE RENDERS: `rigger docs` writes the planning field guide
  handbook page, `authoring-loops.md` links to it, and the docs-drift gate holds over both.
  This criterion OWNS the guide's render; the skill install is criterion 1's, NOT this one's.
- [ ] a test proves the SPEC LINT: `rigger validate <spec>` on a fixture containing a
  multi-behavior criterion, an ownerless criterion among three-plus, a disposition smell
  outside Notes, and an em dash reports each with its criterion and field-guide class, exits
  0, and reports a clean fixture clean. This criterion OWNS the pre-launch lint surface.
- [ ] a test proves ONE LINT AUTHORITY: the pre-launch lint and the loop's in-run spec-shape
  lint share the single implementation, pinned so a divergence cannot compile or cannot pass.
  This criterion OWNS the in-run call site and the shared-implementation pin; the lint's
  classification logic and the pre-launch `cmd_validate` surface are criterion 3's, NOT this
  one's.
- [ ] a test proves DISCOVERABILITY: the pre-launch surface that names next steps mentions the
  spec lint when given a spec path. This criterion OWNS that surface's wording; it claims no
  lint classification logic or call site of its own.
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`). This
  criterion OWNS the whole-diff gates-green audit; it claims no lint or documentation concept
  of its own.
