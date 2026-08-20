# Spec 74: the version increments with the tree

## Problem

The crate version is `0.1.0` and has never moved across 70+ landed specs, so every
operator install reports "Replacing rigger v0.1.0 with rigger v0.1.0" and nothing but the
embedded build hash - identity without order - tells two binaries apart. The operator's
direction (2026-08-20, verbatim intent): follow the standing GitVersion / git-semver
convention and auto-increment at least the patch level. The anchor exists: tag `v0.1.0`
sits on the root commit, and `git describe --tags --match 'v*'` already yields
`v0.1.0-338-g6ac5776` for today's tree.

## Design

- GIT-DERIVED SEMVER, the GitVersion mainline convention, decided here: the version is
  anchored on the highest reachable `vX.Y.Z` tag and the PATCH auto-increments per commit
  since it - `X.Y.(Z+N)` where N is the commit distance `git describe` reports, with the
  short hash as build metadata: `0.1.338+g6ac5776` today. Exactly at a tag the version is
  the tag's own `X.Y.Z`. Minor and major bumps happen by TAGGING (`v0.2.0` resets the
  patch count from that anchor), which is the convention's release act and stays a human
  decision - no commit-message bump hints in this cut.
- COMPUTED AT COMPILE TIME in `build.rs` from `git describe --tags --match 'v*'`; the
  binary embeds the finished string and never invokes git at runtime. A build outside a
  git checkout, or in a checkout with no reachable `v*` tag, reports the bare crate semver
  with an explicit `+unversioned` marker - never a fabricated distance.
- WHERE IT SURFACES: `rigger version` is the authority and prints the derived semver
  (with the existing build id retained); `rigger validate`'s version line uses the same
  string. The crate version in Cargo.toml stays the static base cargo requires; stored
  provenance keeps the build hash as its identity key unchanged.
- BEHIND-THE-TREE ADVISORY, decided here: `rigger validate`, run inside a checkout whose
  derived version orders above the installed binary's, adds an advisory naming both
  versions and the patch distance ("installed 0.1.331, tree 0.1.338 - 7 commits behind;
  rebuild and reinstall"). Advisory tone like the docs-drift warning; never a hard
  failure; silent when equal, and silent on the comparison when either side is
  `+unversioned`.

## Done when

- [ ] `rigger version` reports the tag-anchored auto-incrementing semver: with a reachable
  `vX.Y.Z` tag it reports `X.Y.(Z+N)` for N commits since the tag plus the short hash as
  build metadata, exactly at the tag it reports `X.Y.Z`, and a build with no reachable
  tag or outside a checkout reports the crate semver with an explicit unversioned marker -
  proven by tests at the derivation seam using fixture git repositories. This criterion
  OWNS the version derivation and format.
- [ ] `rigger validate` inside a checkout whose derived version orders above the installed
  binary's emits an advisory naming both versions and the commit distance, stays silent
  when they match, and skips the comparison when either side is unversioned - proven at
  the validate seam. This criterion OWNS the behind-the-tree advisory; the derivation is
  criterion 1's, NOT this one's.
- [ ] Both feature lanes green: `cargo fmt --check`; `cargo clippy --all-targets -D
  warnings`; `cargo test` on default features AND `--no-default-features`.

## Global constraints

- Hyphens, not em dashes, anywhere the diff touches.
- No new event type; stored provenance keeps the build hash as its identity key.
- The embedded values are computed at COMPILE time only - no runtime git invocation.

## Notes

- Constraints walk record: outside-a-checkout or tagless build - explicit `+unversioned`,
  never fabricated; shallow clone - `git describe` reports what the checkout can reach,
  and the advisory compares only when both sides carry a derived version; repeated builds
  of one commit - identical string; crash-resume/concurrency - compile-time constants,
  n/a; future `v0.2.0` tag - patch count resets from the new anchor, by convention.
- The operator seeded `v0.1.0` on the root commit and then declared the current release
  level by tagging `v0.3.0` on the version-bump commit (Cargo.toml 0.1.0 -> 0.3.0,
  2026-08-20) - the operator's judgment that the accumulated feature work warrants 0.3 -
  so the live anchor is `v0.3.0` and derived versions read `0.3.N`. Pushing tags to any
  remote rides the normal push flow, out of scope here.
- Semantic bump hints in commit messages (`+semver: minor` etc., GitVersion's richer
  mode) are OUT of scope, deferred deliberately - tagging is the bump mechanism in this
  cut.
