# Spec 74: the version increments with the tree

## Problem

The reported version never moves (`rigger version` shows the static crate semver); only
the build hash - identity without order - tells two binaries apart, so an operator cannot
see whether the installed binary is behind the tree.

## Design

- Version derivation is DELEGATED to the `go-gitsemver` binary
  (github.com/MyCarrier-DevOps/go-gitsemver, GitVersion-compatible); rigger never
  reimplements the algorithm. Bumps are automatic from commit history (conventional
  commits: `fix:` patch, `feat:` minor, `feat!:`/BREAKING major; `+semver:` directives).
  A tag is an artifact the tool folds in as a version source, never a required human act.
- A committed `go-gitsemver.yml` at the repo root is the single config authority:
  `mode: Mainline`, `tag-prefix: v`, default commit-message conventions.
- `build.rs` invokes `go-gitsemver --show-variable FullSemVer` (plus `ShortSha` for build
  metadata) at COMPILE time; the binary embeds the string and never invokes the tool or
  git at runtime. Tool absent or not a git checkout: the bare crate semver with an
  explicit `+unversioned` marker - never fabricated, never a failed build - and
  `rigger validate` names the missing binary in an advisory.
- Behind-the-tree advisory: `rigger validate`, in a checkout whose derived version orders
  above the installed binary's, names both versions and the commit distance; silent when
  equal or when either side is `+unversioned`. Advisory tone; never a hard failure.

## Done when

- [ ] `rigger version` reports the semver `go-gitsemver` computes for the built commit
  under the committed config: in a fixture repository the patch increments per plain
  commit, a `feat:` commit increments the minor, and a build with the tool absent or
  outside a checkout reports the crate semver with an explicit unversioned marker -
  proven at the derivation seam with fixture git repositories and the real binary. This
  criterion OWNS the build-time delegation and its fallback; the missing-go-gitsemver-
  binary advisory that `rigger validate` emits is criterion 2's, NOT this one's.
- [ ] `rigger validate` inside a checkout whose derived version orders above the installed
  binary's emits an advisory naming both versions and the commit distance, stays silent
  on that comparison when they match or when either side is unversioned, and separately
  emits an advisory naming the missing `go-gitsemver` binary whenever the built binary
  carries the `+unversioned` marker, REGARDLESS OF CAUSE (tool absent at build time, the
  build not run inside a git checkout, or any other reason the marker appears) - a single
  binary-embedded marker cannot distinguish its cause, so every cause folds into this one
  advisory - proven at the validate seam. This criterion OWNS both the behind-the-tree
  advisory and the missing-binary advisory for every `+unversioned` cause; derivation is
  criterion 1's, NOT this one's.
- [ ] Both feature lanes green: `cargo fmt --check`; `cargo clippy --all-targets -D
  warnings`; `cargo test` on default features AND `--no-default-features`.

## Global constraints

- Hyphens, not em dashes, anywhere the diff touches.
- No new event type; stored provenance keeps the build hash as its identity key.
- Version values are computed at COMPILE time only - no runtime git or tool invocation.

## Notes

- Constraints walk: tool-absent / non-checkout / shallow build -> `+unversioned`, build
  never fails; repeated builds of one commit -> identical string; concurrency/resume ->
  compile-time constants, n/a; a future release tag -> a version source the tool folds in.
- Environment: `go-gitsemver` is installed (via `go install`, on PATH through the mise Go
  toolchain) and reports `FullSemVer: 0.3.0` against this repo's `v0.3.0` source. Crate
  semver in Cargo.toml stays the static base cargo requires.
- u74c3 (this criterion) verified at rigger-run tip 33edcc4, where c1/c2 are already
  merged: `cargo fmt --check` clean, `cargo clippy --all-targets -- -D warnings` clean on
  BOTH feature lanes, `cargo test` 2033 passed/2 ignored (default features), 1915
  passed/2 ignored (`--no-default-features`) - zero code changes needed to close this
  criterion. `build.mutation` is "on" in this run's `.rigger/workflow.yml`; the diff
  against the rigger-run merge-base (33edcc4, identical to this unit's own starting
  HEAD) touches zero `.rs` files, so the mutation-efficacy accounting is provably empty
  by construction - not a skipped step, recorded as DecisionMade
  d-u74c3-mutation-accounting.
- Three non-blocking fast-follow threads on `src/main.rs`, carried forward from the c2
  review rounds so they are not lost now that c1-c3 close:
  - `behind_the_tree_advisory` hardcodes calls to `git_is_ancestor`, `git_commit_distance`,
    and `gitsemver::derive_version` instead of accepting them as injected params, unlike
    its file-sibling `workflow_drift_advisory` (main.rs:7711) which injects its ancestry
    oracle as a closure for exactly this reason - a testability-consistency improvement,
    not a stated-rule violation
    (arch-u74c2-behind-the-tree-advisory-hardcoded-oracle-not-injected).
  - `missing_gitsemver_binary_advisory`'s fire arm (the `Some(...)` case) has no
    CLI-level periphery proof, only its silent case does; proving the fire case needs a
    genuinely new technique this codebase has no precedent for yet - a cold,
    PATH-controlled second `rigger` binary build with `go-gitsemver` absent, then driving
    THAT binary's `validate`
    (sdet-u74c2-missing-binary-advisory-fire-path-untested-at-cli).
  - If `go-gitsemver` is present at build time (a genuinely derived version, no
    `+unversioned`) but absent from `PATH` at a LATER `rigger validate` invocation, both
    new advisories go fully silent - zero signal that validate could not even check
    whether the tree has moved (adv-u74c2-validate-time-tool-loss-fully-silent).
  Whichever future unit next touches `behind_the_tree_advisory` or
  `missing_gitsemver_binary_advisory` for an unrelated reason should also weigh these
  three.
