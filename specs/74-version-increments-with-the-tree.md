# Spec 74: the version increments with the tree

## Problem

The crate version is `0.1.0` and has never moved across 70+ landed specs, so every
operator install reports "Replacing rigger v0.1.0 with rigger v0.1.0" and nothing but the
embedded build hash distinguishes two binaries. A hash identifies a build but does not
ORDER two builds: an operator cannot see at a glance whether the installed binary is older
or newer than the tree, and the docs-drift advisory can only say "drifted", not "behind by
N commits". The operator noticed at install time (2026-08-20): version output must
increment as the tree advances.

## Design

- MONOTONIC BUILD VERSION, decided here: the build embeds, at compile time, the repo's
  commit count and short hash for the commit the build was made from, and `rigger version`
  reports `rigger <crate-semver>+<commit-count>.<short-hash> (build <build-id>)`. The
  commit count gives ORDER (newer tree = strictly larger), the hash gives identity, and
  the crate semver stays the hand-managed base. Computed in `build.rs` from git at compile
  time; a build outside a git checkout (e.g. a plain `cargo install` from a tarball) falls
  back to the bare crate semver with an explicit `+unversioned` marker - never a fabricated
  count.
- WHERE IT SURFACES: `rigger version` (the authority), and the version line `rigger
  validate` prints, so an operator comparing installed-vs-tree sees ordered versions in
  both places. The stored scorecard/build provenance fields that today carry the build
  hash gain nothing new - the hash stays their identity key; no event shape changes.
- BEHIND-THE-TREE ADVISORY, decided here: `rigger validate`, when run inside a git
  checkout whose HEAD commit count exceeds the installed binary's embedded count, adds an
  advisory naming both versions and the commit distance ("installed binary is N commits
  behind the tree - rebuild and reinstall"). Same advisory tone as the existing docs-drift
  warning; never a hard failure.
- OUT OF SCOPE, decided here: semver semantics (when 0.1 becomes 0.2 or 1.0) stay a human
  decision in Cargo.toml; this spec makes the automatic component monotonic, it does not
  invent meaning for the hand-managed part.

## Done when

- [ ] `rigger version` reports the crate semver plus a build-time-embedded monotonic
  component (commit count and short hash of the built commit), a build from a newer tree
  strictly orders above one from an older tree, and a build outside a git checkout reports
  the bare semver with an explicit unversioned marker - proven by a test pinned at the
  version-rendering seam. This criterion OWNS the version format and its derivation.
- [ ] `rigger validate` inside a checkout ahead of the installed binary emits an advisory
  naming the installed version, the tree's version, and the commit distance, and stays
  silent on this when the binary matches the tree - proven at the validate seam. This
  criterion OWNS the behind-the-tree advisory; the version format is criterion 1's, NOT
  this one's.
- [ ] Both feature lanes green: `cargo fmt --check`; `cargo clippy --all-targets -D
  warnings`; `cargo test` on default features AND `--no-default-features`.

## Global constraints

- Hyphens, not em dashes, anywhere the diff touches.
- No new event type; stored provenance keeps the build hash as its identity key.
- The embedded values are computed at COMPILE time only - no runtime git invocation in
  `rigger version` (it must answer correctly outside any checkout).

## Notes

- Constraints walk record: outside-a-checkout build - explicit `+unversioned`, never a
  fabricated count; shallow clone (commit count truncated) - the count is whatever git
  reports for the built checkout, and the advisory compares counts only when both sides
  carry one; crash-resume/concurrency - compile-time constants, n/a; repeated installs of
  the same commit - identical version string, correct.
- The operator-build worktree flow gains nothing new: checkout tip, build, `cargo install
  --path` - the version now shows the increment that flow was already producing invisibly.
