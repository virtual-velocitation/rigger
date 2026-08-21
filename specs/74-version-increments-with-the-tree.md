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
  carries the `+unversioned` marker because the tool was absent at build time - proven at
  the validate seam. This criterion OWNS both the behind-the-tree advisory and the
  missing-binary advisory; derivation is criterion 1's, NOT this one's.
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
