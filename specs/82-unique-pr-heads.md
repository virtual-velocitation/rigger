# 82 - Release-ready hands off a unique PR head, never the run branch itself

**Goal:** the release-ready handoff teaches every consumer a dangerous habit: `rigger status`
(src/ledger.rs:~219) and the dash's release-ready view (src/dash.rs, `pr_command`, pinned by
tests/dash_release_ready.rs and src/dash.rs's own tests to the literal string) both recommend
`gh pr create --base <working-ref> --head <run-branch>` - reusing the LIVE run branch as the
PR head. A merged PR's head gets deleted; when the head IS the run branch, successive PRs
conflate review units, a remote delete appears to threaten the loop's anchor, and a force-push
to "fix" the shared head can destroy it. Operator decision 2026-09-03 (Byran): every PR gets a
UNIQUE remote branch name; the run branch never leaves the local repo as a PR head. Rigger's
advisory surfaces must hand consumers that flow.

## Design

THE HANDOFF FLOW, decided here: both surfaces emit a two-command handoff with a deterministic,
per-run-unique head name -
`git push origin <run_branch>:pr/<spec-stem>-<run-short-id>` then
`gh pr create --base <working-ref> --head pr/<spec-stem>-<run-short-id>` -
where `<spec-stem>` is the run's spec file stem (e.g. `80-criteria-survive-extraction`,
sanitized to git-ref-safe characters) and `<run-short-id>` is the same shortened run id the
status header already prints. Unique per run by construction (two runs of one spec differ in
run id); no network action by rigger itself - it PRINTS the flow, the operator executes it
(rigger holds no git credentials and gains none). The dash's `pr_command` field carries both
commands newline-joined; the status line prints them on two indented lines. No new event
type; the ledger derives everything it needs from state it already has (run branch, working
ref, spec path, run id).

## Notes (non-criteria)

Existing pinned strings in src/dash.rs tests and tests/dash_release_ready.rs update to the new
flow as part of the change, not as separate criteria. Spec authored while the installed binary
still truncates wrapped criteria (spec 80 in flight), so the criteria below are single
physical lines by design; tests/spec_lint.rs corpus pins are bumped for this file in the same
authoring commit under each pin's own documented protocol.

## Global constraints

- Hyphens, never em dashes. Both feature lanes green; the no-os-kill gate green on the diff.
- No new event type, no new dependency, no network or credential use added to rigger.

## Done when

- [ ] a test proves THE STATUS HANDOFF IS UNIQUE: on a fully-done run, `rigger status` emits the two-command flow pushing `<run_branch>` to `pr/<spec-stem>-<run-short-id>` and creating the PR from that head, with the head name derived from the run's spec stem and short run id (git-ref-safe), and the literal `--head <run_branch>` form nowhere in the output. This criterion OWNS src/ledger.rs's release-ready text; the dash surface is criterion 2's, NOT this one's.
- [ ] a test proves THE DASH HANDOFF MATCHES: the dash release-ready view's `pr_command` carries the same two-command unique-head flow (newline-joined) for the same run, byte-consistent with criterion 1's derivation, and its page body no longer contains a `--head <run_branch>` form. This criterion OWNS src/dash.rs's release-ready surface and updates its pinned test strings; the derivation helper is shared, defined once, and owned by criterion 1.
- [ ] both feature lanes green (fmt, clippy, test on default and --no-default-features).
