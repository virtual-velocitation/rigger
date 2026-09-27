# 105 - A run holds through an outage and resumes by itself

**Goal:** an API-side failure has no place in the run's model. `RunStarted` is the only
run-level event (`crates/rigger-domain/src/run.rs:33`); the failure taxonomy classifies gate output only
(`crates/rigger-domain/src/failure.rs:23`), so a spawn error charges the unit; the liveness sweep measures wall
time (`src/liveness.rs:342`), so after a 17 h credential outage the first step read a marker
61,963 s stale, recorded a liveness fault and halted; needs-you knows three conditions
(`src/console/mod.rs:91`), none of them an outage, so status read "working" throughout. This
spec ships the hold of docs/architecture-addendum-claude-code-integration.md (section 5) on
the agent host of spec 104.

## Design

DISPOSITIONS, decided, one table in code:
`authentication_failed`, `oauth_org_not_allowed`, `cloud_credential_error`, `billing_error`,
`account_on_hold`, `model_not_found` - HOLD on the first `api_retry` or `StopFailure` that
carries the category. `rate_limit`, `overloaded`, `server_error` - WAIT while the session
retries; HOLD when a session ends of it. `invalid_request`, `max_output_tokens`, `unknown` -
FAULT under spec 104's bound; three consecutive FAULTs across the run - HOLD with cause
`api-unstable`. Spec 104's halt-after-two-relaunches is replaced by these rules.

THE COMPOSITION ROOT, decided: this spec's first unit swaps `src/main.rs:3667` from
`cli::Driver` to spec 104's `claude_code::Driver`, so `rigger run` hosts its agents as headless
sessions, and migrates the `tests/cli.rs` end-to-end fixtures (the fake `claude` on PATH) from
the `-p <prompt>` plain-text contract to the stream-json contract the host speaks. Spec 104
deliberately left the root on `cli::Driver` because without the hold below a bare `api_retry`
ends an unattended run; the swap and the hold land in the same spec for that reason. The
`rigger step` path (`src/main.rs:5726`) keeps `cli::Driver` until spec 106 retires it.

THE EVENTS, decided: `RunHeld {cause, detail, action, by}` and `RunReleased {by, held_for_s}`
are the two new run-stream event types; `by` is `host`, `probe` or `operator`. A run is held
when a `RunHeld` has no later `RunReleased`. While held the conductor requests no launch; a
second hold-class sighting appends nothing. Live sessions are left alone. A launch that ends
with an API-side class while held, or that causes the hold, ends `interrupted`: no
`SpawnResult`, no attempt charged. Gates and merges already running continue.

LIVENESS, decided: the supervisor appends a progress-store `SupervisorBeat` every 30 s. A
spawn's silent time is the wall time since its last stream line minus every overlap with a
hold interval, a retry delay announced by an `api_retry` line, or a gap between consecutive
beats longer than 90 s. Negative spans clamp to zero. `liveness::sweep` computes it from the
records alone.

RELEASE, decided: the hold controller probes under the run's own environment: for a credential
cause, `claude auth status --json` must report `loggedIn`; then, for every cause, a one-turn
canary launch through the host on the cheapest configured model (for `model_not_found`, on the
named model) must return a `result`. The canary is a progress line, not a spawn. Delay starts
at 60 s and doubles to 900 s; a hold declared within 10 minutes of a release resumes the delay
where it ended. Success appends `RunReleased {by: probe}`. `rigger hold --reason <text>`
appends an operator hold, which only `rigger release` ends; `rigger release` ends any hold.

RESUME, decided: on release each `interrupted` launch is relaunched with
`--resume <session-id>` in the same cwd, its first user message stating the hold's cause and
duration. A resume that yields no `system/init` falls back to a fresh launch of the same
spawn. Neither charges an attempt.

WHAT A PERSON SEES, decided: `rigger status` opens with
`HELD <duration> - <cause> - <action>; the run resumes by itself`. The dock gains the kind
`held`, ranked first when the cause needs a person (credential, billing, account, model);
the statusline health word `held` outranks `working`; attention gains the crossings `held`
and `released` (`crates/rigger-domain/src/ledger.rs:169`). `rigger validate` reports the auth method from
`claude auth status --json` and warns when a run rides a login that can expire, naming the
subscription's long-lived token.

CONSTRAINTS WALK: supervisor crash while held - the fold says held; the new supervisor
resumes probing and launches nothing. Flapping - the carried-over delay. Clock set backwards -
clamped spans. Empty store - not held. A run recorded before this spec - no beats; it is
adopted as unsupervised up to the adopting supervisor's first beat, so no old gap becomes a
fault. Operator releases too early - the next launch re-holds within a second. The
step-driven path writes no beats and keeps wall-clock liveness until spec 106 retires it.

## Global constraints

- Hyphens, never em dashes. Both feature lanes and the core lane green (fmt, clippy -D
  warnings, test); no-os-kill and reap audits green.
- `RunHeld` and `RunReleased` are the only new run-stream event types; `SupervisorBeat` is a
  progress-store record. No new crate dependency.
- No API key: probes run under the operator's Claude Code login as inherited.
- The operator's installed rigger binary is never replaced or modified by any unit.

## Done when

- [ ] a test proves RIGGER RUN HOSTS ITS AGENTS: `rigger run` builds `claude_code::Driver` at
  its composition root, and the end-to-end CLI fixtures drive a fake `claude` that speaks the
  stream-json contract, both lanes green. This criterion OWNS the composition-root swap and the
  fixture migration; the hold rules are criterion 2's, NOT this one's.
- [ ] a test proves THE DISPOSITION TABLE: each category maps to hold, wait or fault as
  decided; the first hold-class sighting appends `RunHeld` with its cause and action; a
  session ending of a wait-class category and the third consecutive fault each append one.
  This criterion OWNS the table and the transitions into the hold; behavior while held is
  criterion 2's, NOT this one's.
- [ ] a test proves HELD MEANS NO LAUNCH AND NO CHARGE: while held the conductor requests no
  launch, leaves live sessions alone, records a launch ending API-side as `interrupted` with
  no `SpawnResult` and no attempt, and lets running gates finish; a repeated sighting appends
  nothing. This criterion OWNS the conductor's behavior under a hold.
- [ ] a test proves LIVENESS COUNTS SUPERVISED UNHELD SECONDS: from records alone, a spawn
  silent across a 17 h hold reads minutes, a beat gap over 90 s and an announced retry delay
  are subtracted, negative spans clamp, and a run with no beats is unsupervised until the
  adopting supervisor's first beat. This criterion OWNS `SupervisorBeat` and the sweep's
  arithmetic.
- [ ] a test proves RELEASE BY PROBE: against a fake `claude`, a logged-out status runs no
  canary, a logged-in status plus a canary result appends `RunReleased`, the delay doubles
  from 60 s to 900 s and carries over a quick re-hold, and `rigger hold` / `rigger release`
  append operator events only the operator's release ends. This criterion OWNS the probes and
  the two verbs.
- [ ] a test proves RESUME BY SESSION: after a release an `interrupted` launch relaunches
  with `--resume <session-id>` in the same cwd and the hold message, falls back to a fresh
  launch when the resume yields no init, and charges no attempt. This criterion OWNS the
  relaunch after a release only.
- [ ] a test proves A PERSON SEES THE HOLD: `rigger status` opens with the HELD line, the
  dock ranks `held` first for a cause that needs a person, the statusline reads `held`,
  attention carries `held` and `released` once each, and `rigger validate` reports the auth
  method and the expiring-login warning. This criterion OWNS the surfaces only.
- [ ] both feature lanes and the core lane green (fmt, clippy, test).
