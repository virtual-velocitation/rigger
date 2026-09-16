# 100 - Nothing pushed carries an assistant attribution

**Goal:** the operator's rule is that no commit, pull request, release or document carries an
AI-assistant attribution of any kind - no `Co-Authored-By` trailer, no `Claude-Session:`
trailer, no "Generated with" footer, no session link, no mention. The editor harness injects
those trailers into every agent's commit template and a footer into its pull-request
template, and the loop's agents follow the template: on this repository 117 of the 825
commits on the run branch carry a trailer, and every squash commit on `main` since PR #20
inherited them into its body (81 lines in the one merged 2026-09-15). A rule that lives only
in a persona's prose loses to a template that is re-injected every session. It has to be a
mechanical refusal at the seams where text leaves the machine: the commit, and the merge.

## Design

THE COMMIT-MESSAGE HOOK, decided: `rigger setup` installs a `commit-msg` hook into the
repository's common git directory (beside the `pre-commit` hook it already installs) that
refuses any message matching an attribution pattern - a `Co-Authored-By` trailer naming an
assistant vendor, a `Claude-Session:` trailer, a "Generated with" footer, a `claude.ai/code`
link, or a bare mention of the assistant - printing the offending lines and exiting non-zero.
Hooks live in the common directory, so every unit worktree and every agent in it is held to
the same refusal as the operator. The hook is content rigger ships (its text lives in the
binary), re-installed idempotently by `setup`, and `rigger validate` reports its absence as a
warning naming `rigger setup`.

THE GATE AUDIT, decided: a `no-attribution` audit joins the existing style audits in this
repository's gate library and the shipped template: `git log <base>..HEAD --format=%B` of
the unit's branch must contain no attribution pattern, so a unit whose commits carry one
never reaches review; the same audit runs on the run branch at `release-ready`.

THE MERGE SEAM, decided: the release-ready handoff `rigger status` prints already names the
push and `gh pr create`; it now also names the merge as `gh pr merge --squash --subject
"<title>" --body-file <file>` with a body the operator writes, never the default squash body
(which concatenates every commit message and is how the trailers reached `main`), and the
shipped skill's release section states the rule for pull-request titles and bodies.

THE PERSONAS, decided: every shipped persona that commits carries one sentence under its
commit rules: no attribution trailer, footer, link or mention; the hook refuses them. A
definition test proves the sentence is present in each committing persona.

CONSTRAINTS WALK: a dependency whose package name contains the vendor's name (an SDK) in a
commit body - the hook matches the bare word too, so the message must name the package by
its role ("the agent SDK") rather than its scoped name; the refusal text says so. A hook
already present from another tool - `setup` chains it (the installed hook calls the prior
one first) rather than overwriting it. The operator's own commits - held to the same hook.
Existing history - out of scope for this spec (a rewrite invalidates every commit sha the
event log records); the operator decides separately whether to rewrite `main`'s squash
messages.

## Notes (non-criteria)

The hook text and the audit pattern are one shared constant so they cannot drift. The
operator installed the hook by hand on this repository on 2026-09-15; this spec makes it a
consumer's by default.

## Global constraints

- Hyphens, never em dashes. Both feature lanes green (fmt, clippy -D warnings, test, default
  and --no-default-features); no-os-kill and reap audits green.
- No new event type; no new crate dependency.
- The operator's installed rigger binary is never replaced or modified by any unit.

## Done when

- [ ] a test proves THE HOOK REFUSES: `rigger setup` installs the `commit-msg` hook into the
  common git directory idempotently, chaining a pre-existing hook, and a commit whose message
  carries any of the attribution patterns is refused with the offending lines printed while a
  clean message passes, in the main checkout and in a unit worktree alike; `rigger validate`
  warns when the hook is absent. This criterion OWNS the hook, its installation and the
  validate warning; the audit is criterion 2's, NOT this one's.
- [ ] a test proves THE GATE AUDIT: the `no-attribution` audit fails a fixture unit branch
  with one attributed commit and names it, passes a clean branch, runs in this repository's
  gate library and in the shipped template, and runs on the run branch at release-ready.
  This criterion OWNS the audit only.
- [ ] a test proves THE MERGE SEAM AND THE PERSONAS: the release-ready handoff prints the
  squash merge with an operator-written body, the shipped skill's release section states the
  rule, and every committing persona carries the no-attribution sentence. This criterion OWNS
  the handoff text, the skill text and the persona sentence only.
- [ ] both feature lanes green (fmt, clippy, test on default and --no-default-features).
