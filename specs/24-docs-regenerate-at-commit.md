# 24 - Docs self-regenerate at commit time (git pre-commit hook)

**Goal:** make the self-documentation ACTIVE at the moment code changes, not merely enforced
later by a drift check. rigger's code-derived docs (the `using-rigger` skill + the handbook
discipline chapter rendered by `rigger docs`, spec 20) must regenerate as part of `git
commit`, so a commit that changes a documented code fact automatically carries its freshly
rendered docs - no manual `rigger docs` run, and the `rigger validate` / CI drift check
becomes a backstop rather than the only trigger. The generator already exists (spec 20); this
wires it to the commit.

## Design

Builds on `rigger docs` (the render command from spec 20) and `cmd_setup` / the project
scaffolding it provisions (`src/main.rs`). Depends on spec 20 having landed `rigger docs`.

**Unit 1 - setup installs a doc-regenerating pre-commit hook (touches `src/main.rs`).**
`rigger setup` installs a git pre-commit hook that runs `rigger docs` and `git add`s any
changed rendered outputs (the `using-rigger` skill + the handbook discipline chapter) so the
regenerated docs ride the SAME commit. Requirements that make it safe to live in everyone's
`.git/hooks`:
- IDEMPOTENT: re-running `rigger setup` never duplicates the hook.
- NON-DESTRUCTIVE: it chains with any pre-existing pre-commit hook (via a hooks directory or a
  clearly-marked appended block), NEVER clobbers one.
- SCOPED: it stages ONLY the rendered doc outputs, never other working-tree files.
- SELF-HOSTING-SCOPED: the rendered outputs are rigger's OWN committed docs. The hook
  regenerates and stages them ONLY in a repository that ALREADY TRACKS them - rigger's own
  self-hosting repo - which it detects at commit time by asking git whether either rendered
  output is a tracked file. This is deliberately consistent with the operator model spec 20's
  drift check already documents: an operator project never carries these committed docs, so
  their absence is "nothing to drift." In a project that does not track them (an operator
  installing rigger to drive their OWN code) the hook stays INERT - it does not run `rigger
  docs`, creates nothing, and stages nothing - so an ordinary operator commit is NEVER forced
  to carry rigger's internal discipline docs and never inherits the drift gate on files it did
  not ask for. `rigger setup` installs the same hook everywhere (it is machine-local per clone
  and cannot know at install time whether the repo will track the docs); the tracked check is
  what makes that single hook correct in both a self-hosting and an operator repo.
- GRACEFUL: if `rigger docs` is unavailable or fails, the hook WARNS and lets the commit
  proceed - it must never block a commit. The spec-20 `rigger validate` / CI drift check is
  the hard backstop, so a transient hook failure degrades to "caught later," never "cannot
  commit."

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any
  external tool or project in code, comments, or commit messages.
- The hook NEVER blocks a commit and NEVER stages anything but the rendered doc outputs.
- Non-destructive by construction: a pre-existing pre-commit hook keeps working.
- BOTH feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D
  warnings`; `cargo test` - on default features AND `--no-default-features`.

## Done when

- [ ] a fixture proves `rigger setup` installs a git pre-commit hook that, on `git commit` in a repo that ALREADY TRACKS the rendered docs (rigger's self-hosting repo), runs `rigger docs` and stages any changed `using-rigger` skill / handbook outputs into that same commit - so a commit changing a documented code fact ends up carrying the regenerated docs; AND a fixture proves that in an operator repo that does NOT track these docs the hook stays inert - an ordinary operator commit carries none of them and the operator's worktree is not polluted with them. This criterion OWNS hook installation, the regenerate-then-stage-into-the-commit behavior (that the fresh docs ride the commit), and its scoping to a repo that already tracks the docs.
- [ ] a fixture proves the hook is SAFE to live in everyone's `.git/hooks`: it is idempotent (re-running setup does not duplicate it), chains with a pre-existing pre-commit hook without clobbering it, stages ONLY the rendered doc outputs (never any other working-tree file), and warns-and-proceeds (never blocks the commit) when `rigger docs` is unavailable or errors. This criterion OWNS the hook's safety properties (idempotency, non-clobbering, staging-scope, graceful-degrade); it does NOT own hook installation or the stage-into-the-commit behavior itself (criterion 1).
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`)
