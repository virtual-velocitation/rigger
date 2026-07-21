# 20 - One source of truth: generate the using-rigger skill and handbook from code

**Goal:** the operating discipline (when to use rigger, the one blessed driver, spec
shape, base anchoring, fix-the-loop, the verdict contract, the run-branch-to-PR flow, and
the load-bearing decisions) lives only in the handbook, and `rigger setup` installs no
loadable front-door for it. Any scheme that copies the discipline into a second artifact
just relocates the drift. Make the discipline render from ONE source whose facts are read
from the code the runtime uses, install it as a skill, and catch drift mechanically. This
spec implements Workstream C of `docs/architecture-addendum-pit-of-success.md`. It lands
LAST in the campaign so the generated subcommand list and CLI facts reflect the complete
surface (specs 18, 19, 21).

## Design

Builds on `cmd_setup` / `install_workflow` (`src/main.rs`), the drift-report pattern
already in `cmd_validate` / `validate_advisories` (`src/main.rs`), and the concrete code
facts the runtime uses: `DEFAULT_BASE_REF`, `dash::DEFAULT_PORT`, the remediation bound
(`MAX_RETRIES` in `src/safety.rs` / `max_retries` in `src/config.rs`), the verdict-line
literal read by `verdict_approves` (`src/conductor.rs`), the subcommand dispatch table
(`src/main.rs`), the gating-role set (`ReviewPanel` in `src/config.rs`), and the
spec-shape rules (spec 18, Unit 4).

**Unit 1 - code-fact context + templates + `rigger docs` (touches `src/main.rs`, adds a
template set).** The discipline has two kinds of content, handled separately so the whole
document stays accurate: PROSE (the discipline, the WHY, the load-bearing rationales)
lives in hand-authored templates; FACTS (every value that could drift) are read from the
same code definitions the binary runs on - not a parallel copy. Add a typed context struct
populated FROM those code definitions (default base ref, dash port, retry bound,
verdict-line literal, the subcommand list, the gating roles, the spec-shape rule names) and
a Rust-native, COMPILE-TIME-CHECKED template engine (the template is validated against the
context type at build time, so a template that references a fact the code no longer exposes
breaks the build, not a runtime check). A `rigger docs` command renders the templates
against that context into two outputs: the `using-rigger` skill file and the handbook's
discipline chapter. The engine choice is an implementation detail; the load-bearing
properties are (a) facts read from code and (b) the template checked against the code - and
explicitly NOT an external toolchain that would require re-exporting the facts (which would
reintroduce the drift surface).

**Unit 2 - drift check (touches `src/main.rs`).** `rigger validate` (and the CI lane)
re-renders and diffs the committed `using-rigger` skill and handbook discipline chapter
against a fresh render. Any mismatch - a changed const, a changed template, a hand-edited
skill - is a LOUD failure, exactly like the existing workflow-drift check. This is what
makes the document STAY accurate rather than merely start accurate.

**Unit 3 - setup install + project overlay (touches `src/main.rs`).** `rigger setup`
installs the rendered `using-rigger` skill (DISTINCT from the `/rigger` workflow: the
workflow runs the loop, the skill tells an agent when and how to drive it). A project
overlay lets a repo add its own specifics - base branch, where specs live - without editing
the shared discipline; the overlay is merged into the render, so repo specifics and the
shared discipline share one pipeline. The `using-rigger` content is the front-door: when to
reach for rigger vs not; the one blessed driver (native `/rigger`, visible in `/workflows`
and the dashboard) and the anti-patterns (polling git/ps by hand, hand-driving `rigger
step`, hand-implementing a unit); spec shape (one observable behavior per criterion, atomic
unit is one checkbox, type shapes in Notes); base anchoring on the working ref;
fix-the-loop-when-it-wedges; auto-integration on approve (the human PRs the run branch,
never cherry-picks approved units by hand); every gating agent ends its output with the
verdict line; how to self-serve the binary version and diagnose workflow drift (`rigger
version`); plus the load-bearing decisions so the discipline explains its own constraints.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any
  external tool or project in code, comments, or commit messages, or in the generated skill
  and handbook content.
- One source of truth: no discipline fact is hand-copied into the skill or handbook; every
  drift-prone value is read from the code definitions the runtime uses.
- Determinism by construction: the render is byte-stable across runs on unchanged inputs
  (sorted collections, no map iteration order in output), so the drift check has no false
  positives.
- BOTH feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D
  warnings`; `cargo test` - on default features AND on `--no-default-features`.
- The generated skill is self-contained: it explains the problem each rule solves and does
  not reference any prior harness, internal history, or external tool.

## Done when

- [ ] a test proves `rigger docs` renders both the `using-rigger` skill and the handbook discipline chapter from ONE typed, compile-time-checked template context whose facts are read from code: changing a source fact (e.g. the default base ref or the dash port const) changes the rendered output (facts not hand-copied), and a golden test asserts known code facts appear verbatim - this unit OWNS the render pipeline (typed context + templates + `rigger docs`)
- [ ] a fixture proves `rigger validate` FAILS when the committed `using-rigger` skill or handbook discipline chapter differs from a fresh render, and PASSES when they are in sync
- [ ] a fixture proves `rigger setup` installs the rendered `using-rigger` skill as a file distinct from the `/rigger` workflow, and that a project overlay adds repo specifics (base branch, specs location) into the render without editing the shared discipline source
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`)
