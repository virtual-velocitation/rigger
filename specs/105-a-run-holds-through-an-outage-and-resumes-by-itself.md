# 105 - A run holds through an outage and resumes by itself

**Goal:** an API-side failure has no place in the run's model. `RunStarted` is the only
run-level event (`crates/rigger-domain/src/run.rs:39`); the failure taxonomy classifies gate
output only (`crates/rigger-domain/src/failure.rs:23`), so a spawn error of any class takes its
spawn site's error path, at the implementer an uncharged infra rerun that halts the run past the
taxonomy's limit (`retry_infra_failure`, `crates/rigger-conductor/src/conductor.rs:5586`, called
at `:6435`) and at the other sites their own error paths; liveness counts every elapsed second
(`liveness::sweep`, `crates/rigger-driver/src/liveness.rs:506`, and the host's stream-silence
stop, `crates/rigger-driver/src/driver/claude_code.rs:324`), so after a 17 h credential outage
the first step read a marker 61,963 s stale, recorded a liveness fault and halted; needs-you
knows three conditions (`crates/rigger-console/src/console/mod.rs:98`), none of them an outage,
so status read "working" throughout; and nothing keeps the machine awake while a run drives, so
an idle machine's sleep stops a live run. This spec ships the hold of
docs/architecture-addendum-claude-code-integration.md (section 5) on the agent host of spec 104,
and a keep-awake port.

## Design

DISPOSITIONS, decided, one table in code, `AgentFailure`'s disposition beside the enum
(`crates/rigger-domain/src/agent.rs`), read by the host and the conductor; each row gives its
disposition, its action text, whether it needs a person and whether it is a credential category:
`authentication_failed`, `oauth_org_not_allowed`, `cloud_credential_error`, `billing_error`,
`account_on_hold`, `model_not_found` - HOLD on the first `api_retry` or `StopFailure` that
carries the category; each needs a person, and `authentication_failed` and
`oauth_org_not_allowed` are the credential categories. `rate_limit`, `overloaded`,
`server_error` - WAIT while the session retries; HOLD when a session ends of it.
`invalid_request`, `max_output_tokens`, `unknown` - FAULT: the error takes its spawn site's
existing error path unchanged, so at the implementer it reruns uncharged under
`retry_infra_failure` and a spawn past the infra limit halts the run naming its class. The
action texts: the credential categories `run claude auth login`; `cloud_credential_error`
`refresh the cloud provider's credentials`; `billing_error` `fix the account's billing`;
`account_on_hold` `resolve the account's hold`; `model_not_found` `fix the agent's configured
model, restart rigger run with --rebase-definition, then run rigger release`; the WAIT
categories `none, the run retries by itself`; the FAULT categories none, since they never hold.
Two causes outside the categories have rows: `api-unstable`, action `none, the run retries by
itself`, needing no person, and `operator`, whose action is the hold's `--reason` text, needing
a person. A `RunHeld`'s `detail` is the error text of the line that sighted it, else empty. The
host records no `SpawnResult` for a `result` with `is_error: true` and closes the session's
input at it, as at any first `result` (`crates/rigger-driver/src/driver/claude_code.rs:570`), so
its launch takes its class after the child exits from the same sources in the same order as a
no-result launch (`classify_no_result`, `crates/rigger-driver/src/driver/claude_code.rs:652`),
which reads only a `StopFailure` that follows its launch's `SpawnLaunched` in the run's progress
stream, never by a comparison of seconds. The host closes a launch `completed` at its first
non-error `result`. A host appends its `fault` or `stopped` launch close first, then re-reads
the count of such closes across the run in the progress store since the later of the last
`completed` close and the last `RunReleased`, and a count of at least three HOLDs with cause
`api-unstable` before the host returns its error, which the decorator then waits on as an error
while held. The host appends every `RunHeld {by: host}`, naming the model its launch ran on. A
spawn error that carries no class keeps its spawn site's error path unchanged while the run is
unheld. Spec 104's halt-after-two-relaunches is replaced by these rules, bounded so: the
decorator of HELD relaunches a spawn id at most twice with no non-error outcome of that id
between, whatever the errors' classes, counted in its own memory over one supervisor's life, and
passes the next error through unchanged, even while held, recording nothing but the error result
RESUME decides, so the spawn site takes its existing error path; the count stays spent until a
non-error outcome of that id, so each later error of it passes through as well.

THE COMPOSITION ROOT, decided: this spec's first unit swaps `run_cli` (`src/cli/run.rs:1484`)
from `cli::Driver` to spec 104's `claude_code::Driver`, so `rigger run` hosts its agents as
headless sessions, and migrates the `tests/cli.rs` end-to-end fixtures (the fake `claude` on
PATH, `install_fake_claude`) from the `-p <prompt>` plain-text contract to the stream-json
contract the host speaks. The swap and the hold land in one spec because without the hold a bare
`api_retry` ends an unattended run. `run_cli` builds the host with `rigger_bin` set to the
running executable, the run's progress store, the scratch root, the stop grace and the store
selection's `handed_env` (`src/cli/mod.rs:429`), which the host sets after `opts.env` on every
launch, so the session's hooks and its `rigger mcp --spawn` child resolve the flag-selected
store as `rigger critique`'s do (`src/cli/critique.rs:123`), and hands it the supervisor's hold
sink and beat source; a host built without them, as `rigger critique` builds it
(`src/cli/critique.rs:207`), appends no hold, keeps spec 104's classed errors and counts silence
from the last line. The host merges spec 104's per-spawn hooks into each launch's `--settings`,
since every conductor spawn site passes an empty `settings_json`
(`crates/rigger-conductor/src/conductor.rs:4768`): the `PreToolUse` `rigger guard-write` rooted
at the spawn's `dir` and scratch container, an empty `dir` rooted at the session's cwd, and one
`StopFailure` entry per `AgentFailure::CATEGORIES` running `rigger hook stop-failure`; each
command of these two hooks runs `rigger_bin`, each path in it shell-quoted, while hooks the host
already merges keep their commands. The host pre-approves its spawn-bound `rigger_emit`,
`rigger_progress` and `rigger_scratch` beside the helper tools in `--allowed-tools`
(`allowed_tools_args`, `crates/rigger-driver/src/driver/mod.rs:45`). The host bridges no printed
record: a refused `rigger_emit` returns to its session as a tool error (`append_emit`,
`crates/rigger-dash/src/mcpserver.rs:857`), so on `rigger run` `rerun_once_on_refusal`
(`crates/rigger-conductor/src/conductor.rs:1516`) sees no refusal, and criterion 1 moves the
three bridged-record tests (`tests/cli.rs:7083`, `:7202`, `:7215`) to conductor tests over a
stub driver that calls the emit callback, beside `refusing_at`
(`crates/rigger-conductor/src/conductor.rs:48649`). The conductor reads the host's recorded
results as it reads the replay driver's (`review_spawn_errored`,
`reviewer_result_is_degenerate`, `crates/rigger-conductor/src/conductor.rs:7951`), so `rigger
run` gets the step path's review semantics, and the swap corrects the comments that say a live
driver records no result. When the conductor loop returns with `RunState::done()`
(`crates/rigger-domain/src/ledger.rs:685`) true, `run_cli` runs `reclaim_run_scratch`
(`src/cli/run.rs:1081`), which this spec extends to `AGENT_STREAM_SUBDIR` and whose
`agent-scratch` removal also clears an `interrupted` launch's kept scratch. `run_cli` resolves
its scratch root by `project_scratch_root` (`src/cli/run.rs:293`) and refuses through
`refuse_unless_one_root` (`:235`) at its start, before RESUME's step lock, as `rigger critique`
does (`src/cli/critique.rs:84`); a project with no scratch root skips that teardown and the
start-up reap and builds the host with an empty scratch root. The `rigger run` process is the
supervisor: the conductor loop, the host, the hold controller and the beat. Each wire in
`run_cli` lands with the criterion owning its part and is proved through the shipped `rigger
run` path: the hold sink with criterion 2, the decorator, with its `run_workflow` wire, with
criterion 3, the beat and its thread with criterion 4, the hold controller's thread with
criterion 5, the step lock and reconciliation with criterion 6, the keep-awake guard with
criterion 8. `rigger step` (`ReplayDriver`, `src/cli/run.rs:828`), `rigger serve`
(`workflow::Driver`, `src/cli/run.rs:1838`) and `rigger canary` (`cli::Driver`,
`src/cli/eval.rs:221`) keep their drivers.

THE EVENTS, decided: `RunHeld {cause, detail, action, needs_person, by, model}` and `RunReleased
{by, held_for_s}` are the two new run-stream event types, beside `RunStarted`; `ledger::project`
carries the open hold on `RunState`. `by` is `host`, `probe` or `operator`; `cause` is a
category, `api-unstable` or `operator`. Its writer sets `action` and `needs_person` from the
cause's row, so the fold and the console, which the core lane builds without `agent`, read them
without the table. The hold folds over the current run's slice, as `ledger::project` does, and a
mint over an open hold first appends `RunReleased {by: host}`, before its `RunStarted`, so a new
run starts unheld. A run is held when a `RunHeld` has no later `RunReleased`; a hold starts at
its earliest `RunHeld` since the last `RunReleased`, and its cause is its latest. A hold is an
operator hold when any of its `RunHeld` events is `by: operator`; no sighting or failed probe
appends on it. Every writer appends `RunHeld` or `RunReleased` only at the stream revision at
which it read the hold state (`ExpectedRevision::Exact`); on a conflict it reads `RunHeld` and
`RunReleased` by type from its pinned revision, re-decides and re-pins to the conflict's actual
revision, the `record_result_if_absent` idiom
(`crates/rigger-store-sqlite/src/spawn_store.rs:114`), so repeated and concurrent appends land
once: a second sighting appends nothing, except that a sighting or failed probe whose category
needs a person and differs from the hold's cause appends one `RunHeld` with the new cause, and
`rigger hold` on a held run appends one with cause `operator`. A probe's or operator's `RunHeld`
carries the prior `RunHeld`'s `model` forward, empty when none. When the conductor loop returns
with `RunState::done()` true and a hold open, an operator hold included, the supervisor appends
`RunReleased {by: host}`, which, like a mint's, releases an operator hold too; any other exit
leaves the hold to the next supervisor's probes. A sighting's append and the done run's and the
mint's releases are criterion 2's; a failed probe's and `rigger hold`'s appends are criterion
5's. Where a rule orders a progress-store record against a run-stream event, the record carries
the run stream's revision its writer last read, and the order is by revision, never by time.

HELD, decided: `rigger run` and `rigger serve` wrap their driver in one new `AgentDriver`
decorator in `rigger-conductor`, so no spawn site changes for the hold: before each launch it
waits while the run is held, and on an error of a HOLD or WAIT class, or any other error while
the run is held, it waits for the release and launches the same spawn id again, with no infra
mark, up to the bound of DISPOSITIONS. Two other errors pass to the conductor unchanged: one
carrying a conductor marker (`is_parked`, or `EMIT_REFUSED_MARKER`, which arises only under a
driver that calls the emit callback, `crates/rigger-conductor/src/conductor.rs:3413`), and one
for a spawn the current run's slice has ended (`spawn::ended_by`, read as RESUME reads it).
Every relaunch of a spawn id takes the ordinal RESUME decides, never `opts.launch`. `cmd_step`
reads the hold right after its step lock and `--fresh` (`src/cli/run.rs:584`), before
`sweep_terminal` (`:656`), `enforce_definition_pin` (`:675`) and `liveness::sweep` (`:748`), so
`rigger step` on a held run launches nothing and prints its step JSON with an empty `wave`,
`done` false and `halted` naming the hold, exiting 0, so the workflow driver drains its live
workers and stops; `rigger release` and a new `rigger workflow` resume it. `rigger serve` hosts
no prober, so no probe ends its hold, and `run_workflow` hands the decorator a session-closed
flag its serving thread sets when `server.run` returns, so a wait there ends with `parked_spawn`
(`crates/rigger-domain/src/agent.rs:241`) when the session closes and the conductor unwinds as
on any park. Live sessions are left alone. A host with a hold sink closes a launch `interrupted`
in place of `fault` when its class is HOLD or WAIT or the run is held at its end: no
`SpawnResult`, no attempt charged, the spawn's scratch kept. Gates and merges already running
continue.

LANDING ORDER, decided: criterion 2 lands `RunHeld`, `RunReleased` and the fold, and criteria 3
to 8 build on them; criterion 5 lands after criterion 3, whose wait keeps a held run alive to be
probed, criterion 6 after criteria 3 and 5, criterion 7 after criteria 4 and 5, whose beat its
suffix reads and whose `claude auth status --json` reader `rigger validate` reuses, and
criterion 8 after criterion 3, whose wait keeps a held run driving. Criterion 3's proofs seed
`RunReleased` from the test, since `rigger release` lands with criterion 5, and criterion 3
rewrites criterion 2's `rigger run` proofs that drive on past a hold. A criterion naming several
surfaces runs as one unit per concern, in order: criterion 1 as the swap with its fixtures, tool
grant, store hand-off and teardown, then the hooks, then the resume fallback; criterion 2 as the
events and the fold, then the table and its transitions; criterion 3 as the decorator with its
wires, then the host's `interrupted` close, then the step's exit; criterion 4 as the silent-time
function with `ApiRetry`, then the beat with its thread, then the stop's, the sweep's and the
status age's use of it; criterion 5 as the probes, then the two verbs; criterion 6 as the
start-up reconciliation, then the host's park, real-result write, the decorator's error write
and the conflict site's read, then the relaunch; criterion 7 as the hold surfaces, then the
`rigger validate` report; criterion 8 as the port with its acquisition points, then the Linux
adapter, then the macOS and Windows adapters.

LIVENESS, decided: the supervisor appends a progress-store `SupervisorBeat` every 30 s to the
current run's stream, re-reading the current run at each beat as the hold controller does at
each tick, and the host appends a progress-store `ApiRetry {spawn, delay_ms, category}` per
`api_retry` line. A spawn's silent time is the wall time since its last stream line outside the
union of every hold interval, every announced retry delay, every gap between consecutive beats
longer than 90 s, and the open span since the latest beat when longer than 90 s. Negative spans
clamp to zero. One pure function in `crates/rigger-driver/src/liveness.rs` computes it from
those records alone, and the host's stream-silence stop fires when it, not the time since the
last line, passes `max_wall_clock`. A run with no beats is unsupervised only for the span before
the adopting supervisor's first beat. `liveness::sweep` and `rigger status --json`'s
`liveness_age_s`, which the workflow's marker race reads (`workflows/rigger.js:564`), read a
marker's age through the same function over the run's hold intervals alone, so a step after a
released 17 h hold reads minutes; other marker-age readers keep wall-clock age.

RELEASE, decided: the hold controller never probes an operator hold or a `model_not_found` hold,
which no probe ends, since a supervisor launches from the configuration it read at start. For
any other hold it probes under the run's own environment: for a hold whose cause is a credential
category, `claude auth status --json`, read by the one reader `rigger validate` shares, must
report `loggedIn`; then a one-turn canary must return a `result` with `is_error: false` on the
model the hold's latest `RunHeld` names, a read of its own apart from the launch classing of
DISPOSITIONS; an empty `model` omits `--model`. The canary is a host entry apart from `spawn`:
no `rigger` server and no hooks, the project root as its cwd, bounded by the current delay; it
writes no launch record, close or `SpawnResult`. A failed canary's category is the last
`api_retry` category in its own stream, and upgrades the cause as THE EVENTS decides. Each probe
writes one progress-store `AgentProgress` whose id is the constant `hold-probe`, its probe line.
The delay starts at 60 s and doubles per probe to 900 s across the hold's chain: walking back
from the hold, the hold before it joins the chain when the later hold's first `RunHeld` falls
within 10 minutes after its `RunReleased`, and the walk ends at the first hold that does not
join or is an operator hold; the delay follows the count of probe lines whose run-stream
revision falls inside a hold of the chain, and the next probe falls one delay after the later of
the latest probe line and the hold's latest `RunHeld`, clamped to at most one delay from now, so
no other probe state is stored. The delay is one pure function over those records and the hold's
events, proved by a table; the `rigger run` proof seeds backdated records, with no knob. A
passing probe re-reads the hold and appends `RunReleased {by: probe}` only while the hold's
latest `RunHeld` is the one it probed; otherwise it appends nothing. `rigger hold --reason
<text>` appends an operator hold and refuses when no run is live, live meaning a current
`RunStarted` exists and its run's `RunState::done()` is false whatever its driver's state, so
any run not done accepts it, however it stopped; `rigger release` ends any hold, and on an
unheld run appends nothing and says so.

RESUME, decided: on start, before `anchor_run_branch`, `fresh_run_if_requested`
(`src/cli/run.rs:1367`, `:1566`) and the conductor loop's own mint, the supervisor takes the
project's step lock (`acquire_step_lock`, `src/cli/mod.rs:2011`) for its life, so a second
supervisor or a `rigger step` refuses with the lock's existing `STEP_BUSY_TOKEN` refusal
(`src/cli/mod.rs:1993`) and a courier backs off as it does on a busy step today; and still
before them it closes every open `SpawnLaunched` of every run in the progress store
`interrupted` and reaps what is still rooted in the launch's `dir`
(`reap_processes_rooted_under`, `crates/rigger-process/src/reap.rs:613`) with the scratch root
as its authorized root; criterion 6 builds this start-up reconciliation. `SpawnLaunched` gains
`dir`; an open record whose `dir` is absent, empty or outside the scratch root closes
`interrupted` and reaps nothing. The host owns the relaunch, inside `spawn`, and the conductor
changes at one site, the conflict resolver's (`spawn_conflict_resolution_implementer`,
`crates/rigger-conductor/src/conductor.rs:11033`), which reads an unmarked error of its spawn,
live or answered from the log, as a spent round, moving to the next `~retry` with no attempt
charged and past `CONFLICT_RESOLVE_BOUND` returning `Integration {blocked}` as an unresolved
conflict does: `spawn` answers a spawn id the current run's slice (`read_current_run`,
`crates/rigger-domain/src/run/read.rs:75`) has ended (`spawn::ended_by`,
`crates/rigger-domain/src/spawn.rs:1050`, the replay driver's read) with its result and launches
nothing, while a liveness fault, the stream-silence stop's, ends no spawn until the decorator's
error write below lands on its id, so a restart relaunches a stopped spawn whose error never
reached its spawn site, as the replay driver re-parks one; the host parks a spawn id through
`park_in_run` (`crates/rigger-store-sqlite/src/spawn_store.rs:38`) before its first launch
unless its `SpawnRequested` already stands, as `rigger critique` parks before it launches, so
`reserve_spawn` (`crates/rigger-conductor/src/conductor.rs:4050`) admits an answered spawn free
on a restart; the host records a launch's real result through a sibling of
`record_result_if_absent` (`crates/rigger-store-sqlite/src/spawn_store.rs:114`), the same
compare-and-append with `spawn::ended_by` as its answered test, while every liveness-fault write
keeps `record_result_if_absent`, so a real result supersedes a liveness fault and repeated or
concurrent results land once; on `rigger run` the decorator, before it hands its spawn site an
error for a parked id that `spawn::ended_by` does not end, a marked error aside, records that
error on the id as an error `SpawnResult` through the same write, so the id ends as the replay
driver's recorded failure does and `review_spawn_errored` reads it; when `opts.id`'s latest
launch ended `interrupted`, the host launches the next ordinal with `--resume` of that launch's
session (the `resumed_from` path, `SpawnOpts`, `crates/rigger-domain/src/agent.rs:159`) in the
same `dir`, its first user message stating the hold's cause and duration, or the supervisor's
restart, followed by the interrupted launch's `resume_task` when it carried one. Every relaunch
of a spawn id, by this path, the decorator of HELD or the fallback, takes the ordinal after the
highest the progress store records for that id, never `opts.launch`. When `read_stream` sees no
`system/init` from any `--resume` launch, the conductor's `continue_prior_session`
(`crates/rigger-conductor/src/conductor.rs:4784`) included, the host closes that launch
`interrupted` and launches the spawn fresh at the next ordinal with `resumed_from` empty,
logging the fallback as a progress line, except that a launch HELD closes `interrupted` for its
class or a held run takes no fallback and returns its error to the decorator; criterion 1 owns
the fallback and the ordinal rule, since its swap drops `cli::Driver`'s fallback, and criterion
3 that exception. Neither charges an attempt.

WHAT A PERSON SEES, decided: `rigger status` opens with `HELD <duration> - <cause> - <action>`,
ending `; the run resumes by itself` while the latest `SupervisorBeat` is no older than the 90 s
of LIVENESS and the hold is neither an operator hold nor a `model_not_found` hold, else `;
rigger release ends it`. The dock (`crates/rigger-console/src/console/mod.rs:98`) lists the kind
`held` for a hold whose latest `RunHeld` carries `needs_person`, as an operator hold's does,
first in `attention_kind_rank` (`crates/rigger-domain/src/ledger.rs:243`); a hold that needs no
person shows on the HELD line and the statusline only. The statusline (`console/mod.rs:139`)
reads `needs-you` for a dock entry other than `held`, else `held` while held, else `working`,
`done` or `healthy` as today. `CONSOLE_EVENT_TYPES` (`crates/rigger-dash/src/dash.rs:2781`)
gains `RunHeld` and `RunReleased`, so the page's fold sees the hold. Attention gains `held`,
ranked first, and `released`, ranked last (`crates/rigger-domain/src/ledger.rs:226`), folded one
each per hold from the run's `RunHeld` and `RunReleased` events and keyed by the hold's first
`RunHeld`, not diffed between endpoints in `compute_attention`
(`crates/rigger-conductor/src/conductor.rs:2779`), which carries neither; spec 106's `rigger
attention --follow` delivers them, so neither a step nor `rigger run` prints them. `rigger
validate` reports the auth method from `claude auth status --json` and warns when `authMethod`
is the value an interactive subscription login reports, pinned by a fixture recorded from the
installed `claude auth status --json`, naming the subscription's long-lived token; with `claude`
absent or its status failing it prints one advisory and exits as before.

KEEP-AWAKE, decided: an OS facility is a port with one adapter per OS family on that OS's
native API, never one OS's tool. The port is `SleepHold`, a new module in
`crates/rigger-domain/src/`: `acquire(reason)` returns a guard, and dropping the guard releases
the hold. The adapters are a new module in `crates/rigger-process/src/`, the ring crate that
owns the OS process facilities, which gains a normal `rigger-domain` dependency (ring 3 on ring
1); only the composition root names their constructor.
- Linux: a logind inhibitor lock, `org.freedesktop.login1.Manager.Inhibit("sleep:idle",
  "rigger", <reason>, "block")` over the system bus, holding the returned file descriptor for
  the guard's life; where logind answers but refuses `Inhibit` (its polkit default denies a
  caller with no local session), the adapter returns unavailable naming the refusal. The client
  is `zbus` with default features off, its blocking API on and its tokio integration on, reusing
  the tokio the store already links, so the adapter adds no executor and no direct `tokio` edge.
- macOS: `IOPMAssertionCreateWithName(kIOPMAssertPreventUserIdleSystemSleep, ...)` and
  `IOPMAssertionRelease`, through `extern` declarations linking IOKit; the assertion's name is a
  CoreFoundation string made through the same `extern` block linking CoreFoundation, so the
  adapter adds no crate.
- Windows: `PowerCreateRequest` plus `PowerSetRequest(PowerRequestSystemRequired)`, and
  `PowerClearRequest` on release, through `windows-sys` at a version the lock already resolves.
- Who holds it: the process that drives a run, for as long as it drives one: `rigger run`'s
  supervisor (`run_cli`, `src/cli/run.rs:1381`) and `rigger serve` (`run_workflow`,
  `src/cli/run.rs:1740`). Each takes it through a `&dyn SleepHold` the composition root passes.
  A held run keeps the guard, so its release resumes the run at once; `rigger serve` drops its
  guard when `server.run` returns. `rigger step` is one-shot and holds nothing.
- When the facility is missing (no system bus, no logind, any other OS, any acquisition error)
  the process prints one line to stderr,
  `rigger: keep-awake unavailable: <reason>; a sleep pauses the run and it resumes by itself`,
  and the run continues; it never fails. The hold and resume of this spec are the OS-neutral
  safety net.
- Verification: the macOS and Windows adapters are written to the vendor's documented API, and
  no gate, container or runner verifies them, by decision; a defect there surfaces as an issue
  report on the project. Only the port's lifecycle and the Linux adapter are tested here.

ADDENDUM DEPARTURES, decided: the Design departs from the Goal's addendum in seven places, each
kept as decided above: a `rate_limit` hold honors no known reset time (section 5.1), since the probe
backoff already bounds a rate-limit hold's canary spend; a `cloud_credential_error` hold runs no
`claude auth status` probe (5.1), since that status reports the login, not the cloud provider's
credentials; a `model_not_found` hold runs no canary (5.1), since a supervisor launches from the
configuration it read at start; the canary runs on the hold's model, not the cheapest configured
one (5.4), since that model is the one the held launches need; the page shows the hold in its
dock entry and statusline word, not its health strip (5.5), since that strip reports the dash's
and the store's connection; the write guard roots every spawn at its `dir` and scratch
container, not a reviewer persona at its scratch only (4.4), since personas and review's steps
are the consumer's to declare; and a probe releases a hold at worst 900 s after a login, not
within section 9's two minutes, since the probe delay caps at 900 s, and `rigger release` is the
instant path.

CONSTRAINTS WALK: supervisor crash while held - the fold says held; the new supervisor closes
the open launches `interrupted`, resumes probing and launches nothing. Flapping - the
carried-over delay. Clock set backwards - clamped spans. Empty store - not held. A run recorded
before this spec - no beats; it is adopted as unsupervised up to the adopting supervisor's first
beat, so no old gap becomes a fault. Operator releases too early - the next launch re-holds
within a second. The step-driven path writes no beats and holds no keep-awake guard, so its
sweep subtracts hold intervals only and an idle sleep pauses it. Until spec 106 retires the
workflow script, an outage on a step-driven run keeps today's behavior, by decision - it raises
no hold, `liveness::sweep` still faults it and `raceOuterWallClock` (`workflows/rigger.js:591`)
keeps wall time; `rigger hold` is the operator's cover on that path, and `rigger run` is the
blessed driver this spec fixes. A driver that dies holding the keep-awake guard - the OS ends
the inhibitor, assertion or request with the process. A sleep the guard does not prevent - the
beat gap and the resume carry the run.

## Global constraints

- Hyphens, never em dashes. Both feature lanes and the core lane green (fmt, clippy -D
  warnings, test); no-os-kill and reap audits green.
- `RunHeld` and `RunReleased` are the only new run-stream event types; `SupervisorBeat` and
  `ApiRetry` are progress-store records; all four join `retention::EPISODIC_TYPES`, which grows
  from 16 to 20. No new crate dependency except `zbus` for the Linux adapter and a
  direct `windows-sys` edge for the Windows adapter, each under its own
  `cfg(target_os = ...)`.
- No API key: probes run under the operator's `claude` login as inherited.
- The operator's installed rigger binary is never replaced or modified by any unit.

## Done when

- [ ] a test proves RIGGER RUN HOSTS ITS AGENTS: `rigger run` builds `claude_code::Driver` at
  its composition root, every launch's `--settings` carries the `PreToolUse` write guard and one
  `StopFailure` entry per category, an empty `dir` rooting the guard at the session's cwd, both
  hooks running `rigger_bin`, and `--allowed-tools` carries `rigger_emit`, `rigger_progress` and
  `rigger_scratch`; every launch carries the store selection's handed environment; a done run's
  scratch, its agent streams included, is reclaimed; a run whose directory and repository
  disagree refuses before it reclaims or reaps, and a project with no repository reclaims
  nothing; the end-to-end CLI fixtures drive a fake `claude` that speaks the stream-json
  contract, both lanes green; a `--resume` launch that yields no `system/init` falls back to a
  fresh launch at the next ordinal with `resumed_from` empty; a refused record charged to its
  attempt, a reviewer rerun once told why and a second refusal failing its stage hold in
  conductor tests over a stub driver that calls the emit callback. This criterion OWNS the
  composition-root swap, the hook injection, the tool grant, the store hand-off, the run
  teardown with its root check, the fixture migration with the bridged-record tests' move, the
  resume fallback and the ordinal rule; the hold rules are criterion 2's, NOT this one's.
- [ ] a test proves THE DISPOSITION TABLE: each category maps to hold, wait or fault as decided;
  through `rigger run`, the first hold-class sighting appends `RunHeld` with its cause, action
  and model, a repeated or concurrent one appends nothing, one whose person-needing category
  differs from the hold's cause appends one, and none appends on an operator hold; a done run or
  a mint with a hold open, an operator hold included, appends `RunReleased {by: host}`; a
  session ending of a wait-class category and a third consecutive fault or stream-silence stop,
  concurrent closes included, each append one; a `result` with `is_error: true` closes the
  session's input and is classed after the child exits as a no-result launch is, against a fake
  `claude` that exits only on that close, and a non-error one closes its launch `completed`; a
  `StopFailure` of an earlier launch never classes a later one; a classed fault at the
  implementer reruns uncharged under the infra limit; a host built with no hold sink appends
  nothing. This criterion OWNS the table, `RunHeld`, `RunReleased`, the hold fold, the revision
  stamp, the `completed` close, the sightings' transitions into the hold, the done run's and the
  mint's releases and the hold sink's `run_cli` wire; a failed probe's upgrade and `rigger
  hold`'s append are criterion 5's and behavior while held criterion 3's, NOT this one's.
- [ ] a test proves HELD MEANS NO LAUNCH AND NO CHARGE: through `rigger run`, while held the
  driver decorator launches nothing, and on a hold- or wait-class error it waits for the release
  and launches the same spawn id again at the next ordinal with no infra mark, while a marked
  error or one for a spawn the run has ended passes through unchanged; live sessions are left
  alone; a launch ending API-side closes `interrupted` with no `SpawnResult`, no attempt and its
  scratch kept, and a `--resume` launch with no `system/init` that ends API-side or on a held
  run launches nothing fresh; after two relaunches of a spawn id with no non-error outcome
  between, whatever the errors' classes, its next error and each later one pass through
  unchanged, held or not; running gates finish; a wait on `rigger serve` ends with a park error
  when its session closes; `rigger step` on a held run launches nothing and prints step JSON
  whose `halted` names the hold, exiting 0, before its scratch sweep, definition pin and
  liveness sweep run. This criterion OWNS the decorator with its bound and its `run_cli` and
  `run_workflow` wires, the step's exit, the host's `interrupted` close of an API-side ending
  and the fallback's exception for it.
- [ ] a test proves LIVENESS COUNTS SUPERVISED UNHELD SECONDS: from records alone, a spawn
  silent across a 17 h hold reads minutes, a beat gap over 90 s, an open span since the latest
  beat over 90 s and an announced retry delay are subtracted, negative spans clamp, and a run
  with no beats is unsupervised until the adopting supervisor's first beat; the host's
  stream-silence stop does not fire on a session silent across a hold, and a host with no beat
  source counts from the last line; `liveness::sweep` and `liveness_age_s` read a marker silent
  across a released 17 h hold as minutes. This criterion OWNS `SupervisorBeat`, the beat's
  `run_cli` thread, `ApiRetry`, the silent-time function, the stop's, the sweep's and
  `liveness_age_s`'s use of it.
- [ ] a test proves RELEASE BY PROBE: through `rigger run` against a fake `claude`, a logged-out
  status runs no canary and a `model_not_found` hold runs no probe; a logged-in status plus an
  `is_error: false` canary result on the hold's model appends `RunReleased`, and an error result
  does not; a failed canary's person-needing category upgrades the cause; the canary writes no
  launch record; a hold re-declared during a probe is not released by it; the delay, a pure
  function proved by a table and through `rigger run` on backdated records, doubles per probe
  from 60 s to 900 s and carries over a quick re-hold but not an operator hold; `rigger hold`
  refuses with no live run, an empty store included; and `rigger hold` appends an operator hold
  that no probe ends and `rigger release` ends. This criterion OWNS the probes, the failed-probe
  upgrade, the controller's `run_cli` thread and the two verbs.
- [ ] a test proves RESUME BY SESSION: a supervisor start takes the step lock before it anchors
  or mints a run, so a second refuses with the existing busy refusal first; the start closes an
  open launch of any run `interrupted` and reaps its `dir` under the scratch root, and a record
  whose `dir` is absent or outside the scratch root reaps nothing; a spawn the current run has
  ended is answered from its result with no launch, while one whose only result is a liveness
  fault and one only a prior run ended are launched; the host parks each spawn before its first
  launch, so a restart charges an answered spawn no budget, and a relaunch's real result
  supersedes a liveness fault, repeated or concurrent results landing once; on `rigger run` an
  error passed to its spawn site for a parked id records an error result, so a restart answers
  it from the log, and a conflict resolver's error, live or answered, spends a round with no
  attempt charged; after a release or that restart, an `interrupted` launch relaunches at the
  next ordinal with `--resume <session-id>` in the same cwd and a first message naming the hold
  or the restart, then its `resume_task` when it carried one, and charges no attempt. This
  criterion OWNS start-up reconciliation, the answer from the log, the host's park and
  real-result write, the decorator's error write, the conflict site's read of an error and the
  host's relaunch of an `interrupted` launch only.
- [ ] a test proves A PERSON SEES THE HOLD: `rigger status` opens with the HELD line, whose
  suffix names `rigger release` for an operator hold, a `model_not_found` hold or a hold with no
  beat in the last 90 s; the dock lists `held` first for a hold whose latest `RunHeld` carries
  `needs_person` and omits it otherwise; the statusline reads `held` when no other dock entry
  stands; the page's console stream carries the hold; the attention fold yields one `held` and
  one `released` per hold; and `rigger validate` reports the auth method, the expiring-login
  warning on the recorded subscription-login fixture, and one advisory when `claude` is absent.
  This criterion OWNS the surfaces only.
- [ ] a test proves THE MACHINE STAYS AWAKE WHILE A RUN DRIVES: with a fake adapter, `rigger
  run` and `rigger serve` acquire the hold when they start driving a run, keep it while the run
  is held and release it when they exit; an unavailable facility prints the one line and the run
  proceeds; where the system bus carries logind, the Linux adapter's inhibitor appears in
  `ListInhibitors` while held and is gone after release, and where it does not, the adapter
  returns unavailable and the test proves the bus truly lacks logind instead of passing
  silently; where logind refuses the inhibitor, the adapter returns unavailable naming the
  refusal and the test asserts logind's refusal. This criterion OWNS the port, the three
  adapters and the acquisition points.
- [ ] both feature lanes and the core lane green (fmt, clippy, test). This criterion OWNS the
  lanes' state only; every behavior above is its own criterion's.
