# 112 - The spec is critiqued before the run

**Goal:** a spec defect is found and closed before a run starts, never by the run. Today the
only adversarial read of a spec happens at run time: the plan-critique gate
(`run_plan_critique_gate`, `crates/rigger-conductor/src/conductor.rs:7207`) critiques the
planner's DAG, and when the defect is in the criteria themselves the plan cannot fix it. Two
defect shapes are findable from the spec text alone and still cost whole runs. Two criteria that
are circular under landing order - whichever unit lands first fails its own text - surface only
after a plan, a re-plan and plan-critique rejects, and the run is stopped and restarted. A
criterion that claims an identity (byte-identical, equal) which no Design sentence decides for a
fact a later generation drops surfaces only when an adjudicator rejects a built unit with cause
`spec-ambiguity`. The run-time signal is also mislabelled: a plan-critique reject whose defect is
in the criteria can carry cause `decomposition-conflict`, so a stop keyed only on the cause does
not fire.

## Design

**UNIT ORDER AND BASE, decided here.** Criterion 2 needs criterion 1 (the refusal reads the record
criterion 1 writes). Criteria 3, 4 and 5 need nothing. Criterion 6 needs all five. The spec is
launched on a tree carrying spec 101's typed read (`EventStore::read_stream_typed` with
`TypeSelection`), which criterion 2 uses to read `DecisionMade` without a whole-stream scan.

**THE CRITIC IS THE PLAN-CRITIQUE ADVERSARY, SPAWNED THROUGH THE HEADLESS HOST, decided here.**
`rigger critique <spec>` resolves the persona the workflow names as the `plan-critique` stage's
`adversary` (this repo: `.rigger/agents/adversary.md`, model `opus`), falling back to
`defaults.review.adversary`; a workflow naming neither refuses with a message naming both keys and
records nothing. The spawn runs through spec 104's host, `claude_code::Driver`
(`crates/rigger-driver/src/driver/claude_code.rs:81`), through its `AgentDriver::spawn` (`:1017`):
the child inherits the operator's ambient environment and login, and no credential variable is
read or set. The verb composes the host itself (`bin` and `rigger_bin` empty, so resolved on
`PATH`; `progress_store` the project's `.rigger/progress.db`; `scratch_root` the project scratch
root; `stop_grace` 30 s; the persona's own `max_wall_clock`). `SpawnOpts` carries an empty `dir`
and `isolation: false` (the project checkout, like the planner), `unit` `critique-<hash>`, `stage`
`critique`, `run_id` `critique-<hash>`, `title` the spec path, and `attempt` the number of
critique requests already recorded for the hash. The loop's own run composition (`run_cli`,
`src/cli/run.rs:1469`) is not changed by this spec.

**THE CRITIQUE PROMPT, decided here.** One pure domain function builds it from the spec path and
the spec text, in this order: (i) the stance - this is a critique of a SPEC, not code: default to
skepticism, assume the author missed something, prove the spec self-contradictory or undecided,
never soften to converge, cite the criterion number or Design block title for every finding, run
no build or test command and edit no file (the persona's gate-running duty does not apply); (ii)
the plan-critique rules text `review::PLAN_CRITIQUE_RULES` (below); (iii) twin criteria (two
checkboxes claiming one concern) and bundling (one checkbox carrying two mitigations) as ownership
defects; (iv) two hunts, each named: LANDING ORDER - for every ordered pair of criteria sharing a
command, store read, file or counter, "if A lands first on a tree without B, does A's own text
hold?"; UNDECIDED CORNER - for every criterion's mechanism, the corners empty, repeated, reverted,
DROPPED (a fact present in an earlier generation and absent in a later one), concurrent,
crash-resume, cold start and existing data, each a finding when no Design sentence decides it;
(v) the ban - a fix is a Design or Global-constraint change, never a criterion edit; (vi) the
output contract in Notes; (vii) the spec text verbatim. `PLAN_CRITIQUE_RULES` is the rules
paragraph `build_dag_critique_prompt` (`conductor.rs:7002`) carries today (rule 7 ownership, rule 8
open dispositions, shared blast radius is not a defect), moved verbatim into
`crates/rigger-domain/src/review.rs` as one `pub const` that both prompts read.

**THE CRITIQUE RECORD LIVES OFF THE RUN STREAM AND KEYS ON THE CONTENT HASH, decided here.**
- *The hash.* `<hash>` is `playbooks::fnv1a_64` (`crates/rigger-domain/src/playbooks.rs:13`) over
  the spec file's raw bytes, as 16 lowercase hex digits. Any byte change is new text; a
  whitespace-only edit costs a re-critique, accepted.
- *The spawn events.* The host records the `SpawnResult` on `run::STREAM` of the store it is
  handed (`spawn_store::record_result_if_absent`, `crates/rigger-store-sqlite/src/spawn_store.rs:101`),
  and the verb parks its `SpawnRequested` the same way (`spawn_store::park_in_run`, `:32`). The
  verb hands both a store namespaced to `<identity>-critique`
  (`Namespaced::new(backend, &format!("{identity}-critique"))`), so the request and the result
  land on the critique's own stream and never on the run stream a `rigger step` folds into its
  wave. The spawn id is `spawn_id("critique-<hash>", ROLE_ADVERSARY, attempt)`.
- *The authority.* The critique of a hash IS its latest `SpawnResult` (highest store position)
  with an empty `error` whose output ends in a verdict line. Its findings are the finding lines of
  that output, parsed by one pure domain function; a result with an `error`, or with no verdict
  line, is not a critique (the verb prints why and exits non-zero). The verdict line is the loop's
  (`review::verdict_approves`, `crates/rigger-domain/src/review.rs:172`); the refusal reads
  BLOCKING findings, never the verdict word, so an `approve` beside a BLOCKING line still counts as
  blocking.
- *The graph copy.* For each finding the verb appends one `ReviewFinding` to the project run
  stream (the stream every finding lives on, so `rigger peers <spec>` shows it), replay-keyed by
  the finding id so a repeat appends nothing. Finding ids are `sc-<hash>-<attempt>-<k>`, `k` the
  1-based order of the finding line. `by` is `spec-critic`; `about` is `[<spec path>]`. The
  `ReviewFinding` events are the graph's view; the refusal never reads them.
- *The spec path* is repo-relative with any leading `./` removed; an absolute path inside the
  repository is made repo-relative; a path outside it is refused.

**ANSWERED FROM THE STORE, decided here.** When the hash already has a critique, `rigger critique`
spawns nothing: it re-appends any missing `ReviewFinding` (the keyed append makes this a no-op
when they exist), prints the recorded findings and verdict, and exits 0. There is no flag to force
a re-critique of unchanged text. When the hash has none, the verb parks, spawns, parses, appends
and prints. Exit status is 0 whenever a critique was recorded or answered, whatever its findings.

**A NEW RUN IS REFUSED UNTIL ITS CRITIQUE IS CLEAN, decided here.**
- *A new run* is exactly what `ensure_started_pinned` (`crates/rigger-store-sqlite/src/run_store.rs:127`)
  would mint: `--fresh` was passed, or the latest `RunStarted`'s criteria differ from the spec's.
  A step that ADOPTS the latest run (criteria equal, with or without `--rebase-definition`) is a
  resumed run and is never refused by this rule. A run with no spec path (empty criteria) is out of
  scope and never refused.
- *Open findings* are the BLOCKING findings of the hash's critique minus every id named in the
  `resolves` array of a `DecisionMade` on the run stream whose `governs` contains the normalized
  spec path and whose store position is after the critique's `SpawnResult` (the global order
  `read_all` returns, on both backends). A `resolves` entry
  naming an id that is not in the critique is ignored. No critique for the hash is refused as
  "not critiqued".
- *The sites.* One CLI function decides it, called before any event is appended: in `cmd_step`
  after the store opens (`src/cli/run.rs:530`) and before the `--fresh` block (`:544`); and at the
  top of `fresh_run_if_requested` (`:1551`), which serves `rigger run`, `rigger serve` and
  `rigger workflow`. The `/rigger` Workflow driver couriers `rigger step` and already stops loudly
  on a failing step (`.claude/workflows/rigger.js:28`), so it needs no change.
- *The refusal* exits non-zero and prints the text in Notes on stderr (stdout stays the step's
  single JSON line). There is no override flag; the only ways past are a clean critique of the
  current text or a recorded resolution.
- *The fixtures.* Every existing test that begins a new run through a CLI entry records a clean
  critique for its fixture spec first, through one shared test helper under `tests/common/`;
  criterion 2's unit owns the helper and those edits. `rigger replay` and `rigger canary` drive the
  conductor in their own namespaces without these CLI sites and are unaffected.

**THE VALIDATE TELLS, decided here.** Three advisories join `spec::spec_lint_advisories`
(`crates/rigger-domain/src/spec.rs:713`), so `rigger validate <spec>` and the in-run
`load_criteria` print them through `spec_lint_warning_lines` (`src/cli/mod.rs:3013`). Each is a
`LintAdvisory` with `criterion: Some(n)`; advisory only, the exit status never changes. Text
matching is case-insensitive; a sentence is the text between `. ` boundaries of a criterion's
full block (`criterion_blocks`).
- *Twin measured surface* (class `F10 landing-order circularity`): criteria i < j each hold a
  sentence carrying the same backtick span and a measure word from `exactly`, `zero`, `at most`,
  `no more than`, `reads no`, `costs`, `materializes`, `appends`. One advisory per pair and span,
  on criterion j, detail naming criterion i and the span.
- *Identity claim without a comparison surface* (class `F11 undecided removal`): a criterion
  sentence carrying `identical`, `byte-identical`, `equal to`, `equals` or `the same as`, and none
  of `projection`, `wire form`, `compared on`, `comparison surface`, `bytes of`, `ordered by`.
- *Identity claim with no removal corner* (class `F11 undecided removal`): a criterion carrying one
  of those identity words while no line under a heading whose title starts with `Design` or
  `Notes` carries `drop`, `drops`, `dropped`, `removed`, `removal`, `absent`, `deleted` or
  `no longer`.

**THE SHIPPED SKILL, decided here.** `spec-preflight` is one more `SkillEntry` in
`skill_registry` (`crates/rigger-domain/src/docs.rs:1100`), its body a `const` beside
`PLANNING_A_SPEC_BODY` (`:367`) with a renderer that ignores `ctx`, exactly like planning-a-spec.
Adding the entry is the whole wiring: `install_skills` (`src/cli/mod.rs:4373`, called from
`cmd_setup` at `src/cli/setup.rs:646`) installs it to `.claude/skills/spec-preflight/SKILL.md`
and rewrites it only when absent or drifted, `rigger docs` renders it to
`skills/spec-preflight/SKILL.md`, and the docs-drift gate checks it. The body is the operator's
preflight procedure (steps: landing-order simulation, per-criterion corner walk, adversary pass,
resolve and record) with these changes: it names planning-a-spec as `../planning-a-spec/SKILL.md`;
its adversary pass is `rigger critique <spec>`; its record step is a `DecisionMade` carrying
`governs` and `resolves` (Notes); it carries no absolute path, no home directory and no reference
to this repository's source files or spec numbers (its worked examples describe the two failure
shapes generically); and it claims nothing about what `rigger validate` reports. The failure
catalog already holds F9 (unbounded claim surface), so the two new classes are F10 (landing-order
circularity) and F11 (undecided removal): one row each is added to planning-a-spec's churn table
in `PLANNING_A_SPEC_BODY`, and one `### F10` and one `### F11` section with its tell and
countermeasure to `PLANNING_FIELD_GUIDE_BODY` (`docs.rs:460`), whose rendered
`docs/handbook/planning-field-guide.md` and `skills/planning-a-spec/SKILL.md` are regenerated in
the same unit.

**A SPEC DEFECT STOPS THE RUN AT PLAN-CRITIQUE, decided here.**
- *The signal* is one field: the plan-critique adjudicator's `cause`, read through the single
  disposition parse `SpawnResult::adjudication` (`crates/rigger-domain/src/spawn.rs:502`), never a
  second parser. `spec-ambiguity` is a spec defect; every other cause is not. The verdict paragraph
  of `build_dag_critique_prompt` (`conductor.rs:7074-7079`) gains the adjudicator's cause contract
  and one rule: an upheld finding that names a criterion's own text as the defect (two criteria
  that contradict, a criterion no plan can satisfy) is `spec-ambiguity`, never
  `decomposition-conflict`.
- *Code built* means the current run slice holds a `SpawnResult` whose spawn role is
  `implementer` for a unit other than the plan stage.
- *The site* is the reject branch of `plan_critique_loop` (`conductor.rs:7250`), after the
  `UnitFailed` it records today (`:7384-7390`). When the cause is `spec-ambiguity`, no code was
  built, and the slice holds no re-plan `SpawnRequested` for this attempt, the gate records a
  `LessonLearned` whose `about` is `[<spec path>]` and whose summary names the upheld finding ids,
  then `UnitEscalated` for the gate, both replay-keyed on the attempt; it does not call `re_plan`
  (`:7412`). The spec path is the current run's `RunStarted.spec`.
- *The stop reason* rides the step's existing `halted` field: `RunCtx::halt_reason`
  (`conductor.rs:3540`) returns `amend the spec and relaunch: plan-critique found a spec defect in
  <spec> (<ids>)` for the step that stopped. Later steps see the escalated gate in `escalated`, as
  any plan-critique escalation today.
- *Relaunch* is the operator's: amend the spec, then `rigger critique <spec>`, then a new run
  (criteria edited mint one; a Design-only amendment needs `--fresh`), which criterion 2 gates.
- *After code was built*, and for every other cause, the branch behaves as today.

**CRITERIA 1 AND 5 SPLIT AT THE CRITIQUE PROMPT.** Criterion 1 owns moving the rules paragraph into
`PLAN_CRITIQUE_RULES` with `build_dag_critique_prompt` reading it; criterion 5 owns only the
verdict paragraph and the reject branch. Neither asserts the other's text.

**CRITERIA 1 AND 2 SPLIT AT THE CRITIQUE RECORD.** Criterion 1 owns the record, the hash, the id
scheme and the domain reader that returns a hash's critique and its findings; criterion 2 owns the
resolution filter and the refusal.

**CRITERIA 2 AND 5 SPLIT AT THE STEP.** Criterion 2 acts before a run is minted, at the CLI;
criterion 5 acts inside the conductor after a run is minted. Criterion 5's test drives
`conductor::run` with a test driver and never enters a CLI mint site.

**CRITERIA 3 AND 4 SPLIT AT THE CLASS LABEL.** Criterion 3 owns the advisory class strings;
criterion 4 owns the catalog prose. The shipped skill names `rigger critique`; that verb's
behavior is criterion 1's, and criterion 4's unit asserts only install, drift refresh and content.

**LANDING-ORDER SIMULATION** (ordered pairs sharing a surface; landing order 1, 2, then 3, 4, 5 in
any order, then 6):

| First | Without | Shared surface | Does the first one's own text hold? |
|---|---|---|---|
| 1 | 2 | critique record | yes - it records and answers; nothing refuses yet |
| 2 | 1 | critique record | not reachable - 2 needs 1 |
| 1 | 5 | DAG critique prompt | yes - rules text moves verbatim; the DAG prompt reads the const |
| 5 | 1 | DAG critique prompt | yes - 5 edits only the verdict paragraph and the reject branch |
| 2 | 5 | `rigger step` | yes - the refusal fires before any plan exists |
| 5 | 2 | `rigger step` | yes - 5's test is at the conductor seam; no CLI mint |
| 3 | 4 | F10/F11 class names | yes - 3 asserts warning lines only, not catalog text |
| 4 | 3 | F10/F11 class names | yes - the skill claims nothing about validate output |
| 4 | 1 | `rigger critique` named in the skill | yes - 4 asserts install, drift and content, not the verb |
| 4 | 2 | the refusal rule in the skill | yes - the skill states the author's rule, not the mechanism |

**CONSTRAINTS WALK.**
- *Criterion 1.* Empty: a spec with no Done-when criteria is refused by the verb with the
  loop-ready message and nothing is recorded; a critic that returns no finding lines and a verdict
  line is a clean critique. Repeated: answered from the store, zero spawns. Reverted: text back at
  an earlier hash is answered by that hash's critique. DROPPED: a later hash's critique does not
  inherit an earlier hash's findings or resolutions; each hash stands alone. Concurrent: two
  critiques of one hash both spawn and both record; the later result is the critique, the extra
  spawn is the accepted cost; a critique during a live run adds nothing to the run's wave.
  Crash-resume: a request with no result is ignored and the next call spawns the next attempt; a
  result whose `ReviewFinding` copies were not appended is completed on the next call, and the
  refusal never depended on them. Cold start: every answer is read from the store. Existing data:
  no critique exists; nothing is migrated.
- *Criterion 2.* Empty: no critique refuses as not critiqued. Repeated: each refused step refuses
  again until cleared. Reverted: an earlier hash's critique and its resolutions apply again.
  DROPPED: a `resolves` id absent from the current critique is ignored. Concurrent: the check reads
  committed rows; a critique landing a moment later is seen by the next step. Crash-resume: the
  check writes nothing; a resumed run is never refused. Cold start: log only. Existing data: every
  run already in the store is adopted and proceeds; only new runs need a critique.
- *Criterion 3.* Empty: no criteria, no tells. Repeated: a span shared by three criteria yields one
  advisory per pair. Reverted, DROPPED, concurrent, crash-resume: out of scope - a pure function of
  one text. Cold start: pure. Existing data: existing specs may warn; advisory only.
- *Criterion 4.* Empty: a repo with no `.claude/skills/` gets the directory. Repeated: a rerun
  writes nothing. Reverted: an edited installed copy is refreshed like any drifted registry skill.
  DROPPED: a hand-authored `.claude/skills/spec-preflight/SKILL.md` in a project is overwritten as
  drift; a copy outside the project is never touched. Concurrent, crash-resume: out of scope - one
  file write per skill, as today. Cold start: pure render. Existing data: projects gain the file
  on their next `rigger setup`.
- *Criterion 5.* Empty: a `spec-ambiguity` reject that upheld no finding still stops, the lesson
  saying none were upheld. Repeated: a replayed step reaches the escalated gate and appends nothing
  (keyed). Reverted, DROPPED: out of scope - one verdict per attempt. Concurrent: the step lock
  serializes steps. Crash-resume: a crash between `UnitFailed` and `UnitEscalated` replays the
  recorded adjudicator result into the same branch. Cold start: log only. Existing data: a
  recorded run whose gate already re-planned after a `spec-ambiguity` reject follows its recorded
  re-plan, so replay never diverges from history.

**OUT OF SCOPE.** Moving the loop's own spawns onto the headless host; a size limit on anything;
any `rigger reset` change for the critique stream (one request and one result per critiqued text).

## Notes (non-criteria)

The critic's output contract, one finding per line, then the verdict as the last line:

```
<critic id> | BLOCKING | <criterion n or Design block title> | <exact reading that breaks> | <smallest Design change that closes it>
<critic id> | NON-BLOCKING | ...
{"verdict":"reject"}
```

A finding line has five `|`-separated fields whose second is exactly `BLOCKING` or `NON-BLOCKING`;
any other line is prose. The verdict is `reject` when any line is BLOCKING, else `approve`.

The recorded finding (on the project run stream):

```
{"id":"sc-<hash>-<attempt>-<k>","by":"spec-critic","summary":"BLOCKING | <where> | <reading> | <fix>","about":["<spec path>"]}
```

A resolution the refusal honors (`governs` and `resolves` are JSON arrays):

```
rigger emit DecisionMade '{"id":"spec-112-preflight-<short>","governs":["specs/112-the-spec-is-critiqued-before-the-run.md"],"resolves":["sc-<hash>-0-2"],"summary":"sc-<hash>-0-2 closed: <one line>"}'
```

The refusal text:

```
rigger step: refusing to begin a new run on <spec>: <not critiqued | open BLOCKING findings: <ids>>
  amend the spec and critique it:   rigger critique <spec>
  or record a resolution:           rigger emit DecisionMade '{"id":"...","governs":["<spec>"],"resolves":[<ids>],"summary":"..."}'
```

## Global constraints

- Hyphens, never em or en dashes, in every added line.
- No new event type; no new crate dependency.
- Both feature lanes green (fmt, clippy -D warnings, test on default and --no-default-features).
- No API key and no third-party service: the critic runs on the operator's own agent CLI login.
- The operator's installed rigger binary is never replaced or modified by any unit.
- Fewest moving parts: one verb, one record, one refusal function, one gate branch.

## Done when

- [ ] a test proves A SPEC IS CRITIQUED BEFORE THE RUN: `rigger critique <spec>` spawns the plan-critique adversary on the spec text through the headless host and records its findings and verdict keyed on the content hash,
  and a second call on unchanged text answers from the store with zero spawns, asserted with a
  stub agent binary first on `PATH`. This criterion OWNS the verb, the critique record, the
  critique prompt and `PLAN_CRITIQUE_RULES`; the refusal is criterion 2's and the plan-critique
  stop is criterion 5's, NOT this one's.
- [ ] a test proves A NEW RUN NEEDS A CLEAN CRITIQUE: a `rigger step --spec <spec>` that would begin a new run refuses, printing each open BLOCKING finding id and the two clearing commands,
  when the spec's hash has no critique or holds a BLOCKING finding no later `DecisionMade`
  resolves, while a step adopting the spec's existing run proceeds. This criterion OWNS the
  refusal at every CLI run start and the shared critique fixture helper; the critique record is
  criterion 1's, NOT this one's.
- [ ] a test proves VALIDATE NAMES THE PREFLIGHT TELLS: `rigger validate <spec>` warns with the criterion number on a twin measured surface, an identity claim naming no comparison surface,
  and an identity claim whose Design and Notes never decide removal. This criterion OWNS the
  three text checks and their class labels; the catalog prose is criterion 4's, NOT this one's.
- [ ] a test proves SETUP SHIPS THE PREFLIGHT SKILL: `rigger setup` installs `spec-preflight` from the skill registry with no absolute or home path in it and refreshes it on drift like every registry skill, with F10 and F11 rows in the catalog.
  This criterion OWNS the shipped skill text, its registry entry and the F10 and F11 catalog
  rows; the validate tells are criterion 3's and the verb is criterion 1's, NOT this one's.
- [ ] a test proves A SPEC DEFECT STOPS THE RUN AT PLAN-CRITIQUE: a plan-critique reject with cause `spec-ambiguity` before any implement unit built code records the escalation naming the spec and its upheld findings
  and halts the step with "amend the spec and relaunch" without re-planning, while that reject
  after code was built re-plans as today. This criterion OWNS the gate's spec-defect branch and
  the verdict paragraph of the DAG critique prompt; the rules paragraph is criterion 1's, NOT
  this one's.
- [ ] both feature lanes green (fmt, clippy, test on default and --no-default-features). This
  criterion OWNS only the lanes over the integrated result.
