# 90 - Hermetic test git and merge-friendly audit artifacts

**Goal:** two hygiene defects tax every review round. (1) The operator's global git has
`commit.gpgsign=true`, and the unit tests in `src/worktree.rs` and `src/conductor.rs` (about 43
`git commit` sites) never disable it, so every test commit runs gpg against the user's keyring:
with several agents running the suite at once the keyring lock contends and `git worktree add`
races, ten failures per lane during u86 c2/c3 round 3, and every reviewer spends a cold rerun
proving "the documented flake" is not the unit's. (2) Spec 85's `docs/audit/duplication-catalog.json`
and the report are byte-pinned by `tests/simplification_audit.rs` and keyed by line numbers, so
every pair of parallel units that adds a test conflicts on them at the second integration (the
u86 c2 x c3 dry-run conflicted ONLY there; all source merged clean), every spec-lint pin bump
drifts them, and the conflict cost u86-c3 a remediation attempt and its lineage.

## Design

THE TEST RUNNER MAKES GIT HERMETIC, decided: ONE place, not 43 - `.cargo/pidns-runner.sh`, which
already wraps every test binary, exports `GIT_CONFIG_GLOBAL=/dev/null`, `GIT_CONFIG_NOSYSTEM=1`,
`GIT_AUTHOR_NAME`/`GIT_AUTHOR_EMAIL`/`GIT_COMMITTER_NAME`/`GIT_COMMITTER_EMAIL` fixed to
`rigger-test <rigger-test@localhost>`, and `GIT_TERMINAL_PROMPT=0`; CI (runner opted out) sets
the same block at workflow level in `.github/workflows/rust.yml`. Tests that set their own
identity today keep working (env is a default, `-c` still wins); tests that ASSERTED on a signed
commit, if any, are corrected to assert on the commit's content. Production code paths are
untouched: a real `rigger` run still commits with the operator's own config, signing included.
An audit test proves no test file or `#[cfg(test)]` body sets `commit.gpgsign` or
`GIT_CONFIG_GLOBAL` itself - the runner is the single authority.

THE DRIFT GUARD IS LINE-FREE, decided: the byte-guarded artifact is the catalog in a canonical
line-free form - each site is `{file, fn, content_hash}` (the normalized-token hash spec 85
already computes), clusters ordered by (file, fn) - so two sibling units that add tests to
different places produce disjoint additions that merge cleanly, and a pin bump above a site
changes nothing. Line numbers move to a sibling `docs/audit/duplication-catalog.lines.json`
that the generator rewrites in write mode and the guard NEVER compares; the report keeps its
`file:line` citations (spec 85's rule) rendered from that file, and the report's guard checks
structure only (sections present, counts equal to the catalog) rather than bytes. The
responsibility map and spec 87's dead-code JSON follow the same split.

CONSTRAINTS WALK: a test that needs gpg on purpose - none exists; if one appears it sets its own
`GIT_CONFIG_GLOBAL` and the audit test lists it as the exception by name. Two units adding the
SAME fn name in different files - distinct `file` keys, no conflict. A refactor that moves a fn
(same content hash, new file) - one deletion and one addition, still conflict-free. CI runner
opt-out - the workflow env block is the runner's exact export list, checked by the same audit
test against both files so they cannot drift apart.

## Global constraints

- Hyphens, never em dashes. Both feature lanes green (fmt, clippy -D warnings, test, default
  and --no-default-features); no-os-kill and reap audits green.
- No new event type; no new dependency. No production git behaviour changes.
- The operator's installed rigger binary is never replaced or modified by any unit.

## Done when

- [ ] a test proves TEST GIT IS HERMETIC: under the runner every test commit succeeds with the
  fixed identity and no gpg invocation regardless of the operator's global config, CI carries the
  identical env block, and no test source sets the hermetic variables itself. This criterion OWNS
  the runner and CI env and the audit test; the artifacts are criterion 2's, NOT this one's.
- [ ] a test proves THE DRIFT GUARD IS LINE-FREE: the guarded catalog carries no line numbers,
  a pin bump that shifts every site in a file leaves it byte-identical, two branches adding tests
  in different files merge it without conflict, and the report still cites `file:line` from the
  unguarded lines file. This criterion OWNS the artifact split in the generator; the runner is
  criterion 1's, NOT this one's.
- [ ] both feature lanes green (fmt, clippy, test on default and --no-default-features).
