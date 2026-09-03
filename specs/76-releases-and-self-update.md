# Spec 76: published releases and self-update

## Problem

Consumers have no way to learn a newer rigger exists or to install it: versions are
derived (spec 74) but never published - tags live only locally, no pipeline turns a tag
into a GitHub Release with binaries - and `validate`'s behind-the-tree advisory is
ancestry-guarded to the source checkout, structurally silent for every consumer.

## Design

- RELEASES, the standing convention (cargo-dist): a pushed `v*` tag builds per-platform
  binaries with checksums and publishes a GitHub Release. Config committed
  (`dist-workspace.toml` + generated CI workflow); the generated workflow is committed
  as cargo-dist emits it, regenerated not hand-edited.
- UPSTREAM CHECK (`rigger validate`): when the tree-ancestry check stands down (the
  installed build commit is NOT an ancestor of HEAD - every consumer repo), validate
  compares the installed semver against the latest published release and prints an
  advisory naming both versions and `rigger self-update`. Polite by convention: the
  lookup is cached under the project store with a daily TTL, never blocks (short
  timeout, any network failure is silent), and is skipped entirely with `--local`, with
  config `update_check: off`, with `RIGGER_NO_UPDATE_CHECK` set, or when stdout is not
  a TTY (CI). The two guards are mutually exclusive by the same ancestry test: source
  checkouts compare to the tree, consumers to releases, never both.
- SELF-UPDATE (`rigger self-update`): explicit command, never automatic - downloads the
  matching platform asset from the latest release, verifies its checksum, and replaces
  the current executable (the `self_update` crate, the standing implementation).
  `--version <tag>` pins; exits loudly and changes nothing on any verification failure.
  The advisory only ever recommends it; no surface replaces the binary unasked.

## Done when

- [ ] The release pipeline is committed and verified: cargo-dist config plus its
  generated workflow build, checksum, and publish per-platform binaries on a `v*` tag,
  proven by the pipeline's own CI plan check (`dist plan` clean in a test) rather than a
  live tag push. This criterion OWNS the pipeline config; the update surfaces are
  criteria 2-3's, NOT this one's.
- [ ] A test proves the UPSTREAM CHECK at the validate seam with an injected release
  source: a consumer-shaped repo (installed build commit not an ancestor of HEAD) with a
  newer latest release prints the advisory naming both versions; `--local`, the config
  key, the env var, a non-TTY stdout, a fresh cache stamp, and a failing source each
  independently silence it; a source checkout (ancestor) never consults the source at
  all. This criterion OWNS the check, its cache, and every opt-out.
- [ ] A test proves SELF-UPDATE at the command seam with an injected release source: the
  matching asset is checksum-verified and swapped in place, `--version` pins a named
  tag, and a checksum mismatch or missing platform asset exits non-zero leaving the
  installed binary untouched. This criterion OWNS the command; the advisory wording is
  criterion 2's, NOT this one's.
- [ ] Both feature lanes green: `cargo fmt --check`; `cargo clippy --all-targets -D
  warnings`; `cargo test` on default features AND `--no-default-features`. This criterion
  OWNS the whole-diff gates-green audit and claims no release or update concept of its
  own.

## Global constraints

- Hyphens, not em dashes, anywhere the diff touches.
- No new event type; the check's cache stamp is a plain file, not an event.
- No network in tests: release lookups and downloads go through an injected source
  (trait port), with the GitHub implementation exercised only by the pipeline itself.
- `rigger validate` stays advisory-only and exit-0 on every update-related path.

## Notes

- Constraints walk: offline/timeout -> silent skip, stamp untouched; repeated validate
  within the TTL -> no lookup; crash mid-download -> temp file + atomic rename, the
  running binary is replaced only by a verified whole; concurrent validates -> stamp
  write is best-effort, duplicate lookups harmless; repo with no git at all -> ancestry
  undecidable reads as consumer-shaped, check applies.
- The PR to main after spec 67 tags the first release through this pipeline - the
  pipeline's first live exercise is the campaign's own publication.
- Prior related surfaces unchanged: spec 74's derivation and tree-ancestry advisory,
  spec 75's hook candidates.
