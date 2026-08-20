# Spec 74: the version increments with the tree

## Problem

The crate version is `0.1.0` and has never moved across 70+ landed specs, so every
operator install reports "Replacing rigger v0.1.0 with rigger v0.1.0" and nothing but the
embedded build hash - identity without order - tells two binaries apart. The operator's
direction (2026-08-20, verbatim intent): LITERALLY use a GitVersion-class tool -
go-gitsemver was picked for its GitVersion compatibility without the dotnet ancestry -
with bumps handled by the tool from commit history, tags as artifacts rather than
definitions, and effectively zero human input.

## Design

- LITERALLY THE TOOL, decided here (operator direction: "when I'm saying use GitVersion I
  mean literally use GitVersion", tooling pick go-gitsemver -
  github.com/MyCarrier-DevOps/go-gitsemver, GitVersion-compatible, no dotnet ancestry):
  version derivation is DELEGATED to the `go-gitsemver` binary - rigger never reimplements
  the algorithm. Bumps are fully automatic from commit history: conventional-commit types
  (`feat:` minor, `fix:` patch, `feat!:`/BREAKING CHANGE major) and `+semver:` directives
  in commit messages; a tag is an ARTIFACT the process may emit, never the definition and
  never a required human act. Zero human input in the steady state.
- CONFIG COMMITTED, decided here so no unit has to: a `go-gitsemver.yml` at the repo root
  pins `mode: Mainline` (highest increment since the last version source, applied once,
  commit count in metadata - monotonic per commit on the run/main line), `tag-prefix: v`,
  and the default commit-message conventions. The committed config is the single
  authority; `--show-config` must reflect it.
- COMPUTED AT COMPILE TIME in `build.rs` by invoking
  `go-gitsemver --show-variable FullSemVer` (plus `ShortSha` for build metadata); the
  binary embeds the finished string and never invokes the tool or git at runtime. A build
  where `go-gitsemver` is absent or the tree is not a git checkout reports the bare crate
  semver with an explicit `+unversioned` marker - never a fabricated version and never a
  broken build; `rigger validate` names the missing binary in an advisory (the spec-65
  wrapper-precedent tone, adapted: version derivation degrades loudly, builds never
  fail on it).
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

- [ ] `rigger version` reports the semver `go-gitsemver` computes for the built commit
  under the committed config: in a fixture repository the patch increments per plain
  commit, a `feat:` (or `+semver: feature`) commit increments the minor, and a build with
  the tool absent or outside a checkout reports the crate semver with an explicit
  unversioned marker - proven by tests at the derivation seam using fixture git
  repositories and the real `go-gitsemver` binary. This criterion OWNS the build-time
  delegation and its fallback.
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

- Constraints walk record: outside-a-checkout, tool-absent, or shallow-history build -
  explicit `+unversioned`, never fabricated, build never fails; repeated builds of one
  commit - identical string (the tool is deterministic for a fixed history and config);
  crash-resume/concurrency - compile-time constants, n/a; a future release tag - a
  version SOURCE the tool folds in per its own semantics, requiring nothing of rigger.
- Environment as of authoring: `go-gitsemver` installed via
  `go install github.com/MyCarrier-DevOps/go-gitsemver@latest` (on PATH through the mise
  Go toolchain); against this repo it reports `FullSemVer: 0.3.0` at the `v0.3.0` version
  source (Cargo.toml bump commit). The historical tags `v0.1.0` (root) and `v0.3.0` are
  version sources the tool discovers - artifacts, not the mechanism.
- The repo's existing commit style (`fix(72): ...`, `spec: ...`, `sdet(68) ...`) maps
  onto the conventional-commit reader as patch-level by default with `fix:` recognized
  explicitly; agents' commit-message discipline gains `feat:`/`+semver:` vocabulary only
  when a change genuinely warrants a minor/major bump - nothing retroactive.
