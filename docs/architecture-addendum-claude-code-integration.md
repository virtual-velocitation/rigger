# Architecture addendum: Native Claude Code integration

> Rigger runs its agents on Claude Code, under the operator's Claude Code login. This
> addendum specifies the integration: rigger owns the run as a supervised process, launches
> every agent as a headless Claude Code session it can see into, holds the run through any
> outage and resumes it without a person, and meets the operator's interactive session
> through a plugin. Every seam is a documented Claude Code surface. The body describes the
> target state; the present state appears only in Problem and Delivery.

---

## 1. Problem (measured)

The run is hosted inside a chat session's workflow script. The script sees an agent as a
promise that resolves to text or to null, so rigger infers what it needs to know.

| Rigger needs to know | What it has today | Observed cost |
|---|---|---|
| why an agent died | `null` from the script API; a generic non-zero exit in `src/driver/cli.rs:88`; no Rust path maps 401, 429 or 529 | a login expiry stopped a run; status read "working, nothing needs a human" for 17 h |
| whether an agent is alive | a marker file the agent is asked to touch | the first step after the outage read the marker 61,963 s stale, recorded a liveness fault and halted; manual re-drive |
| where an agent works | a sentence in its prompt; its tool cwd is the operator checkout | 20+ edits landed in the operator checkout; the next landing was refused |
| which session is the agent's | a search of harness files for the spawn id | no transcript or token totals in the log |
| what an agent concluded | prose parsed for a verdict line (`src/conductor.rs:10820`); self-reported results by shell-out | an empty-stdin report overwrote a real result; one spawn recorded two |
| when the run needs a person | polling by the operator session | about 60 unanswerable polls in one night |
| how long a step may run | a 10-minute tool cap on the agent that relays `rigger step` | every longer gate or integration step is detached by hand |

Spawn errors bypass the failure taxonomy (`src/failure.rs` classifies gate output only), so
an API outage charges a unit's remediation attempts. `RunStarted` is the only run-level
event; no held state exists. A chat message typed during a run reaches the relay agent and
halts the driver. Three drivers exist and none is whole: the CLI driver blocks on
`output()` and parses stdout afterwards, the workflow script is the primary path, a Node
shim is a fallback.

A probe of `claude -p --output-format stream-json` with an invalid credential shows what
the documented surface already offers: within one second the stream carries
`{"type":"system","subtype":"api_retry","attempt":1,"max_retries":10,"retry_delay_ms":618,
"error_status":401,"error":"authentication_failed"}`, repeated with doubling delays for
over two minutes while the session stays alive.

### Non-goals

- No API key. The credential is the operator's Claude Code login (or that subscription's
  long-lived token); rigger reads auth state and never stores or sets a credential.
- No second agent runtime. Agents stay Claude Code sessions with its tools, permissions,
  hooks, MCP and skills; rigger does not reimplement the agent loop on the Messages API.
- No dependence on undocumented internals: no harness file layouts, no prompt-text joins.

---

## 2. Invariants

1. **Rigger owns the run.** One supervisor process per run, outside any chat session.
   Closing, compacting or logging out of a session never stops a run; chat never reaches
   an agent.
2. **Know, never infer.** Each fact about an agent comes from a typed surface: process
   cwd, a session id rigger minted, the message stream, Claude Code's error category, hook
   input, structured output.
3. **An outage is a state, not a failure.** An API-side failure never charges a unit and
   never reads as a hung agent.
4. **The log is the state.** Holds, releases, launches and failure classes are records; a
   fresh process derives the whole situation from them.
5. **Liveness counts supervised, unheld seconds.**
6. **One host.** Every persona launches through the same path.

---

## 3. Topology

```
  OPERATOR SEAM (plugin)                THE LOG                    AGENT SEAM (host)
  Claude Code interactive session    events + progress          rigger run (supervisor)
 +------------------------------+   +---------------+   +--------------------------------+
 | skills      /rigger:run ...  |   | RunHeld       |   | conductor loop (in-process)    |
 | MCP tools   status hold      |-->| RunReleased   |<--| hold controller + probes       |
 |             release resume   |   | SpawnLaunched |   | agent host                     |
 | monitor     attention feed   |<--| SpawnResult   |   +---------------+----------------+
 | statusline  one line         |<--| turns, usage  |                   | one child per spawn
 +------------------------------+   +-------+-------+                   v
                                            |             claude -p, stream-json both ways
                                            v             cwd = the spawn's worktree
                                   dash / Mission Control session id minted by rigger
```

The two seams never talk to each other. Both read and write the log; the supervisor
follows it, so an operator's `hold` is an event the supervisor obeys, and the monitor's
feed is a fold of the same events.

---

## 4. The agent host

### 4.1 Launch

One child process per spawn launch. Every parameter carries a fact that is otherwise
inferred.

| Parameter | Value | Replaces |
|---|---|---|
| process cwd | the spawn's worktree | a prompt sentence naming a directory |
| `--session-id` | a UUID rigger mints and records as `SpawnLaunched` | searching harness files |
| `--output-format stream-json --input-format stream-json --verbose` | the typed message stream, both directions | blocking on exit, parsing stdout afterwards |
| `--system-prompt` | the persona | unchanged |
| `--model`, `--fallback-model` | the attempt's rung, the configured fallback | a self-reported model |
| `--tools`, `--allowed-tools` | the spawn's tool list | unchanged |
| `--permission-mode` with `--permission-prompts none` | whatever would prompt is denied and reported in the stream | a silent wait on a prompt nobody sees |
| `--mcp-config` with `--strict-mcp-config` | one server, `rigger mcp --spawn <id>` | shell-outs that must carry `--spawn` |
| `--settings` | the per-spawn hooks of 4.4 | nothing |
| `--json-schema` | verdict personas only (4.5) | prose verdict parsing |
| environment | build-cache variables, `CARGO_TARGET_DIR`, the scratch path; the credential is inherited | export instructions in the prompt |

The task and any later operator note travel as user messages on the input stream.

### 4.2 The stream is the record

```
 claude -p  ==stream==>  host reader (one thread per spawn)  ==>  log + progress store
```

| Stream message | The host |
|---|---|
| `system/init` | records the resolved model, tools and MCP status; a `rigger` server that failed to connect fails the launch as infra |
| any line | touches the spawn's liveness marker - the host proves life, the agent is never asked to |
| `assistant`, `user` | appends transcript turns and running usage |
| `system/api_retry` | records a waiting line (category, attempt, delay); hands the category to the hold controller (5) |
| `system/permission_denied` | records the denied call; the count rides on the result |
| `result` | records `SpawnResult`: output or `structured_output`, usage, cost, turns, session id |
| exit without `result` | ends the launch as `interrupted` or `fault` by the last category seen (5.1); `unknown` when none |

The raw stream is written beside the spawn's scratch and deleted once its turns are
recorded. No agent self-reports a result, a model or a heartbeat.

### 4.3 The spawn's MCP server

`rigger mcp --spawn <id>` serves `emit`, `peers`, `ground`, `graph`, `progress` and
`scratch` over stdio. It is bound to one spawn when it starts, so every record it writes
is attributed by construction. It has no result tool: the session's final message is the
result.

### 4.4 Hooks

Per-spawn, through `--settings`:

- `PreToolUse` on `Edit|Write|NotebookEdit` runs `rigger guard-write --spawn <id>`. A
  target outside the spawn's worktree and scratch is denied with a reason naming the
  right root. A reviewer persona's roots are its scratch only.
- `StopFailure`, one entry per error category, runs `rigger hook stop-failure --spawn <id>
  --class <category>`: the category that ended a turn is recorded even when the stream's
  last line is lost.

### 4.5 Verdicts are data

Lenses, the adversary, the adjudicator and the plan critic launch with a JSON schema
(`verdict`, `cause`, `findings[]`). The conductor reads `structured_output`; the prose is
for people.

### 4.6 Stop

A host-initiated stop (wall clock, operator) closes the session's input stream, waits a
grace period, then ends the child through the sanctioned lifecycle helper on the child's
own handle. Never a pid, a group or a shell-out.

---

## 5. The hold

### 5.1 Classes and dispositions

The classes are Claude Code's own error categories; rigger adds two causes it observes
itself.

| Category | Disposition | Released when |
|---|---|---|
| `authentication_failed`, `oauth_org_not_allowed`, `cloud_credential_error` | HOLD on first sight | `claude auth status` reports logged in, then a canary turn succeeds |
| `billing_error`, `account_on_hold` | HOLD on first sight | a canary turn succeeds |
| `model_not_found` | HOLD on first sight, naming the model | a canary turn on that model succeeds |
| `rate_limit`, `overloaded`, `server_error` | WAIT while Claude Code retries; HOLD when a session dies of it | a canary turn succeeds; a known reset time is honored |
| `invalid_request`, `max_output_tokens`, `unknown` | FAULT: the spawn is relaunched, at most twice, no attempt charged; then the unit fails as infra | - |
| three consecutive FAULTs across the run | HOLD `api-unstable` | a canary turn succeeds |
| host suspended, supervisor absent | an implicit hold interval (5.3) | the supervisor is alive |
| operator | `rigger hold --reason <text>` | `rigger release` |

### 5.2 State machine

```
             first hold-class category | session death by a wait-class | 3 FAULTs | rigger hold
  RUNNING ----------------------------------------------------------------------------> HELD
     ^                          RunHeld {cause, detail, action, by}                      |
     |                                                                                   |
     +--------------- RunReleased {by: probe | operator, held_for} <-- probe ladder -----+

  HELD:  no launch is requested
         live sessions are left alone: each finishes, or ends by itself
         a launch that ends with an API-side category is `interrupted`: no result, no charge
         gates and merges already running continue - they need no API
```

`RunHeld` and `RunReleased` are run-stream events; held is the fold "a `RunHeld` with no
later `RunReleased`". `SpawnLaunched {spawn, launch, session_id, resumed_from, ended}` and
`SupervisorBeat` are progress-store records.

### 5.3 Liveness counts supervised, unheld seconds

The supervisor writes a beat every 30 s. A spawn's silent time is the wall time since its
last stream line, minus every overlap with a hold interval, an announced retry delay, or a
gap between beats longer than 90 s (suspend, crash, reboot).

```
  wall      17:57 ---- 18:00 ============================== 11:12 ---- 11:15
  state     running    HELD authentication_failed           released
  counted   |-- 3 m --|               0                     |-- 3 m --|     6 m silent, not 17 h
```

### 5.4 Release and resume

Probe ladder, under the run's own credential and settings: `claude auth status --json`
(`loggedIn`), then a one-turn headless canary on the cheapest configured model. Backoff
starts at 60 s and doubles to 15 min. On release every `interrupted` launch continues its
own session - `claude -p --resume <session-id>` in the same cwd, with one user message
stating the cause and duration of the hold - so the agent keeps its context. A session
that cannot resume gets a fresh launch of the same spawn. Neither charges an attempt.

### 5.5 What a person sees

- `rigger status`, first line: `HELD 17h02m - authentication_failed - log in to Claude
  Code (/login); the run resumes by itself`.
- needs-you ranks `held` first when the cause needs a person (credential, billing,
  account, model); the statusline health word is `held`; the dash health strip shows it.
- Attention gains two crossings, `held` and `released`; the plugin monitor delivers each
  to the operator's session within seconds.
- `rigger validate` reports the auth method and warns when a long run rides a login that
  can expire, naming the subscription's long-lived token as the remedy.

---

## 6. The operator seam

`rigger setup` installs one plugin, `rigger`, from a marketplace directory it materializes
from the binary (`claude plugin marketplace add`, `claude plugin install --scope project`).

| Component | Content | Purpose |
|---|---|---|
| skills | the shipped skills, plus `run` | `/rigger:run <spec>` starts a detached supervisor and returns |
| MCP server | `rigger mcp`: `status`, `hold`, `release`, `resume_unit`, `peers`, `ground`, `graph` | the session steers through typed tools |
| monitor | `rigger attention --follow` | one stdout line per attention crossing - held, released, escalated, stalled, completed - delivered to the session as a notification; replaces polling |
| hooks | `SessionStart` runs `rigger prime`; `PreToolUse` runs the lookup guard | unchanged behavior, one install unit |
| bin | `rigger` | the bare command in the session's shell tool |

The status line stays a settings entry (`rigger status --line`); a plugin cannot carry one.

`rigger run --detach` starts the supervisor in its own session under a per-project
single-instance lock, logging under the scratch root; `rigger run --attach` follows it. A
step runs in-process for as long as it needs.

---

## 7. Worked example - the login expires during a review round

```
 18:00:00  adversary session: api_retry 401 authentication_failed, attempt 1
 18:00:01  RunHeld {cause: authentication_failed, action: "log in to Claude Code", by: host}
           status: HELD; monitor line reaches the operator session; no launch is requested
 18:03     the adversary session exhausts its retries and exits: launch ended `interrupted`
 18:00 ->  probe every 60 s, doubling to 15 min: auth status says logged out
 11:12     the operator logs in
 11:13     probe: logged in; canary turn succeeds; RunReleased {by: probe, held_for: 17h13m}
 11:13     the adversary launch resumes its session with "held 17h13m for authentication_failed"
 11:31     result recorded; the adjudicator launches
```

Zero liveness faults, zero attempts charged, zero commands typed beyond the login.

---

## 8. Delivery

| Spec | Scope |
|---|---|
| 104 | the agent host: launch, stream, spawn MCP server, write guard, typed failure class, stop |
| 105 | the hold: dispositions, `RunHeld`/`RunReleased`, supervised-unheld liveness, probes, resume by session, status and attention |
| 106 | the operator plugin, the detached supervisor, structured verdicts; the workflow script and the Node shim retire |

Spec 99's driver transcript source is the host's stream. The write-guard item planned for
spec 103 moves to 104.

## 9. Acceptance (measured)

- A credential expiry mid-run: 0 liveness faults, 0 attempts charged, status reads HELD
  within 5 s, work continues within 2 min of the login with no other command.
- 0 writes outside a spawn's worktree; a denied write names the right root.
- Every `SpawnResult` carries session id, usage and turns; 0 self-reported results.
- A step of any duration runs without a hand-detach.
- The operator's session learns of a hold within 5 s, without polling.
