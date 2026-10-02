# 112 - The spec is critiqued before the run

**Goal:** a spec defect is found and closed before a run starts, never by the run. Today the
only adversarial read of a spec happens at run time: the plan-critique gate
(`run_plan_critique_gate`, `crates/rigger-conductor/src/conductor.rs`) critiques the
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
criterion 1 writes). Criterion 4 needs criterion 1 (the skill it ships teaches `rigger critique`,
and its accuracy pin holds that command to `SUBCOMMANDS`). Criteria 3 and 5 need nothing.
Criterion 6 needs all five. The spec is launched on a tree carrying spec 101's typed read
(`EventStore::read_stream_typed` with `TypeSelection`), which criteria 1 and 2 use to read
`ReviewFinding` and `DecisionMade` across the run stream by type, materializing no other event.

**THE CRITIC IS THE PLAN-CRITIQUE ADVERSARY, SPAWNED THROUGH THE HEADLESS HOST, decided here.**
`rigger critique <spec>` resolves the persona the workflow names as the `adversary` of the
plan-critique gate, the stage `wave::critique_gate_name` returns (this repository:
`.rigger/agents/adversary.md`), falling back to `defaults.review.adversary`; that persona is the
critic, and this lookup is one pure function. A workflow naming neither has no critic, a new run
under it is not refused (A NEW RUN IS REFUSED), and there is no built-in critic. The critic lookup
runs on every call, before any store or graph is opened or migrated (*The store* orders it): under a
workflow with no critic the verb refuses, naming both keys, and records nothing, answered or not;
ANSWERED FROM THE STORE applies only under a workflow with a critic. The critic's system prompt is
`build_system_prompt(persona, instructions)` (the persona body, the configured instruction layers
and the communication discipline every spawn gets), and the verb runs the persona with its tools
replaced by Read, Glob, `mcp__rigger__rigger_graph`, `mcp__rigger__rigger_ground` and
`mcp__rigger__rigger_peers` - no Bash, Agent, Grep, or emit, progress or scratch tool - so the
critic can neither build nor record. The host defines its `lookup` and `verify` helpers on every
spawn, so the critic may still reach one through the fan-out tool; a helper's build or record is
denied like the critic's own, by the host's no-prompt permission rule (accepted). The verb hands the
critic its resolved store selection: under a server selection, whichever rung of `store_selection`
made it, the flags included, it puts `KURRENTDB_CONN=<conn>` into the critic's `SpawnOpts.env`,
which the host applies over the ambient environment, so the critic's bound MCP server
(`rigger mcp --spawn <id>`, whose `require_store_dir` ranks that variable above `.rigger/store.conn`
and the workflow's `store:` key) and its `rigger_peers` resolve the store the verb records the
critique to. Under a sqlite selection nothing is handed and that server resolves through
configuration alone: the local store the verb opened, unless `--eventstore sqlite` overrode a server
the configuration selects, which the critic then reads (accepted).
The spawn runs through spec 104's headless host and its `AgentDriver::spawn`: the child inherits the
operator's ambient environment and login, and no login credential variable is read or set. The verb
composes the host itself (`bin` and `rigger_bin` empty, so resolved on `PATH`; `progress_store` the
project's `.rigger/progress.db` namespaced to the project identity, as `run_workflow` composes it,
its critique rows sharing that file's lifecycle, which no command reclaims (accepted); `run_store`
the critique store below; `scratch_root` the project scratch root as `run_workflow` composes it,
empty in a project with no git repository, where no transcript or liveness marker is written and the
directory removal (*The spawn events*) is a no-op; `stop_grace` 30 s; the persona's own
`max_wall_clock`). `SpawnOpts` carries `dir` the repository root the spec path is made relative to
(*The spec path*), `isolation: false` (the project checkout, like the planner), `unit`
`critique-<hash>`, `stage` `critique`, `run_id` `critique-<hash>`, `title` the spec path, and
`attempt` (below). The loop's own driver choice in `run_cli` (`src/cli/run.rs`) is not changed by
this spec. Criterion 1's unit adds `critique` to `SUBCOMMANDS` (`src/main.rs`) and regenerates the
two rendered pages that list the command surface (`skills/using-rigger/SKILL.md`,
`docs/handbook/using-rigger.md`).

**THE CRITIQUE PROMPT, decided here.** One pure domain function builds it from the spec path and
the spec text, in this order: (i) the stance - this is a critique of a SPEC, not code: default to
skepticism, assume the author missed something, prove the spec self-contradictory or undecided,
never soften to converge, cite the criterion number or Design block title for every finding; in
the rules below a unit reads as a criterion and a reject as a BLOCKING finding; every persona or
discipline duty that conflicts with a spec critique is void - reviewing lenses and a diff, the rule
against rendering a verdict, running gates or any build or test command, editing a file, recording
through `rigger_emit`, `rigger_progress` or `rigger_scratch` - and the finding lines and the
verdict line of the final message are the only output; (ii) `review::PLAN_CRITIQUE_RULES` (below);
(iii) twin criteria (two checkboxes claiming one concern) and bundling (one checkbox carrying two
mitigations) as ownership defects; (iv) two hunts, each named: LANDING ORDER - for every ordered
pair of criteria sharing a command, store read, file or counter, "if A lands first on a tree
without B, does A's own text hold?"; UNDECIDED CORNER - for every criterion's mechanism, the
corners empty, repeated, reverted, DROPPED (a fact present in an earlier generation and absent in
a later one), concurrent, crash-resume, cold start and existing data, each a finding when no
Design sentence decides it; (v) the ban - a fix is a Design or Global-constraint change, never a
criterion edit; (vi) the output contract in Notes; (vii) the spec text verbatim.
`PLAN_CRITIQUE_RULES` holds exactly the Rule 7 and Rule 8 bullets and the NOTE on shared blast
radius that `build_dag_critique_prompt` pushes today, byte for byte, as one `pub const` in
`crates/rigger-domain/src/review.rs` that both prompts read. The DAG prompt keeps its DAG opener
and its unit-size line, the line now pushed after the const (the DAG prompt's byte order changes;
accepted); the spec critique prompt carries no unit-size line. Criterion 1's unit tests the pure
prompt function: sections (i) to (vii) appear in that order, the spec text is its verbatim tail,
and the bytes of `PLAN_CRITIQUE_RULES` appear in it and in the DAG critique prompt.

**THE CRITIQUE RECORD LIVES OFF THE RUN STREAM AND KEYS ON THE CONTENT HASH, decided here.**
- *The hash.* `<hash>` is `playbooks::fnv1a_64` (`crates/rigger-domain/src/playbooks.rs`) over
  the spec file's raw bytes, as 16 lowercase hex digits. Any byte change is new text; a
  whitespace-only edit costs a re-critique, accepted.
- *The store.* The verb's refusals run in one order - `resolve_main_worktree_or_refuse`, the critic
  lookup, `refuse_unless_one_root`, the spec path, the loop-ready check `load_criteria` makes, then
  selecting, migrating and opening the store and opening the graph - and only the first one reached
  is printed; it refuses a linked worktree and a root mismatch as `cmd_step` does
  (`refuse_unless_one_root` takes the invoking command for its message, as
  `resolve_main_worktree_or_refuse` does). It takes the `--eventstore` and `--conn` flags
  `rigger run` takes and resolves its backend through `store_selection` with them, as the run entry
  it precedes does (a flagless call resolves as `rigger step` does), and THE CRITIC hands that
  selection to the critic. On a sqlite selection it runs
  `migrate_local_identity` before opening its backend, as `run_cli` and `run_workflow` do, and it
  opens the project's store as `cmd_step` does, creating `.rigger/` and the store when absent.
  Criterion 1's unit words `refuse_unless_one_root`'s two messages for any invoking command and
  updates the one-call-site note in `tests/step_root_resolution_periphery.rs` in the same commit.
- *The spawn events.* The host records the `SpawnResult` on `run::STREAM` of the store it is handed
  (`spawn_store::record_result_if_absent`, which keeps the first result recorded for a spawn id),
  and the verb parks its `SpawnRequested` the same way (`spawn_store::park_in_run`). The verb hands
  both a store namespaced to `<identity>-critique`
  (`Namespaced::new(backend, &format!("{identity}-critique"))`), so the request and the result land
  on the critique's own stream and never on the run stream a `rigger step` folds into its wave.
  Project-wide prefix reads include that stream as `critique-run`; harmless, it holds only spawn
  events, and the graph's live selection reads the run stream alone. On one shared server backend, a
  project whose identity is another's plus `-critique` has that project's critique stream as its run
  stream; accepted, since a minted `project.id` is hex with no `-` and never ends in `-critique`.
  The spawn id is `spawn_id("critique-<hash>", ROLE_ADVERSARY, attempt)`, `attempt` the number of
  `SpawnRequested` events already recorded for the hash; the critique runs the persona's model rung
  for `attempt` (`AgentDef::model_for_attempt`). Once its spawn returns, and on every call answered
  from the store, the verb removes the `agent-live` and `agent-stream` directories of every critique
  run (`critique-<hash>`, any hash), so what a crash left goes with the next call; the log holds the
  output.
- *The authority.* A result for the hash is a critique when its `error` is empty, its output
  carries a verdict line (`review::has_verdict_line`, the last JSON line carrying `verdict`, prose
  may follow it), and a verdict that does not approve (`review::verdict_approves`) comes with at
  least one parsed BLOCKING finding; any other result is not a critique (the verb prints why and
  exits non-zero). The critique of a hash is its latest critique (highest store position). Its
  findings are the finding lines of that output, parsed by one pure domain function: once a
  leading and a trailing pipe are stripped and each field is trimmed, a line with at least five
  `|`-separated fields whose second is exactly `BLOCKING` or `NON-BLOCKING` is a finding, and
  fields past the fourth are joined back with `|` into the fix; any other line is prose. The
  refusal reads BLOCKING findings, never the verdict word, so an `approve` beside a BLOCKING line
  still counts as blocking.
- *The graph copy.* For each finding the verb appends one `ReviewFinding` to the project run stream
  (the stream every finding lives on, so `rigger peers <spec>` shows it), skipping an id that stream
  already holds (a typed `ReviewFinding` read of the whole stream), so a repeat appends nothing; two
  concurrent calls may each append a copy of one id, accepted, since the graph fold upserts a
  finding by id (`ensure_node`) and the finding totals count an id once. The copies go through
  `ingest::folding_into` over the project store and the project graph `open_graph` opened (the
  wiring `cmd_step` uses for its own appends), each payload first passing `check_fold_payload`. The
  verb opens that graph before it reads the critique, on every call no earlier refusal in
  *The store*'s order stops, so a graph.db that owes its rebuild refuses each such call, answered or
  not, naming `rigger setup`. Finding ids are `sc-<hash>-<attempt>-<k>`, `k` the 1-based order of
  the finding line. `by` is `spec-critic`; `about` is `[<spec path>]`. A copy is keyed by id alone:
  a renamed or duplicated spec with unchanged bytes gets no copy of an id already copied, so its
  findings stay about the paths the earlier copies name (accepted). The copies are run-attributed
  like every finding: pruned with the run current at their append and counted under `spec-critic` in
  that run's finding totals; accepted. The `ReviewFinding` events are the graph's view; the refusal
  never reads them.
- *The spec path* is repo-relative with any leading `./` removed; an absolute path inside the
  repository is made repo-relative; a path outside it is refused (*The store* orders the verb's
  refusals). In a project with no git repository the repository root is the project root (the
  directory holding `.rigger/`), the root the run entries' repo-less path already uses; `dir` is
  that root.

**ANSWERED FROM THE STORE, decided here.** When the hash already has a critique, `rigger critique`
spawns nothing: it appends any missing `ReviewFinding` copy, prints the recorded findings and
verdict, and exits 0. There is no flag to force a re-critique of unchanged text. When the hash has
none, the verb parks and spawns, then prints and copies the critique it reads back from the store
after its spawn returns, never its own session's output. Exit status is 0 whenever a critique was
recorded or answered, whatever its findings. Criterion 1's fixture then deletes one line of the
spec: the next call spawns and prints none of the earlier findings.

**A NEW RUN IS REFUSED UNTIL ITS CRITIQUE IS CLEAN, decided here.**
- *A new run* is what the mint decision says: one pure function over the run stream's
  `RunStarted` events, the criteria and `--fresh`, true when `--fresh` was passed, when the stream
  holds no `RunStarted`, or when the latest `RunStarted`'s criteria differ from the spec's;
  `ensure_started_pinned` (`crates/rigger-store-sqlite/src/run_store.rs`) and the refusal both
  call it, and criterion 2's unit extracts it. A step that ADOPTS the latest run (criteria equal,
  with or without `--rebase-definition`) is a resumed run and is never refused by this rule. A run
  with no spec path (empty criteria) is out of scope and never refused.
- *Open findings* are the BLOCKING findings of the hash's critique minus every id named in the
  `resolves` array of a `DecisionMade` on the run stream whose `governs` contains the spec path
  and whose store position is after the critique's `SpawnResult` (the global order `read_all`
  returns, on both backends). Each `governs` entry is normalized like the spec path before the
  comparison. A `resolves` entry naming an id that is not in the critique is ignored. Supersession
  is not read: a recorded resolution stands until the text changes. A renamed spec needs its
  resolutions recorded again under the new path. No critique for the hash is refused as "not
  critiqued". A spec outside the repository cannot be critiqued, so under a workflow with a critic a
  new run on it is refused as not critiqued, whatever its bytes, until the spec moves into the
  repository (accepted).
- *No critic.* Under a workflow with no critic (THE CRITIC's lookup) the refusal does not apply: a
  command that would begin a new run on a spec prints the one no-critic line in Notes on stderr
  and proceeds.
- *The sites.* One CLI function decides it, called before any event is appended: in `cmd_step`
  after the store opens and before the `--fresh` block; and at the top of
  `fresh_run_if_requested`, which serves `rigger run`, `rigger serve` and `rigger workflow`. It
  takes the backend, the identity, the loaded workflow, the spec path, the criteria, `--fresh`, the
  bytes `load_criteria` read and the invoking command, and reads the critique only through
  `Namespaced::new(backend, <identity>-critique)`; `fresh_run_if_requested` gains the backend, the
  loaded workflow, those bytes and the invoking command from its two callers (`run_cli` passes
  `rigger run`, `run_workflow` its own `command`). It hashes the exact bytes `load_criteria` read
  for this command, read once before the run-branch anchor, so `load_criteria` hands those bytes
  back beside the criteria. A refused command may leave the run branch anchored, the identity
  migrated and the instance registered; the next command reuses them. The `/rigger` driver
  (`workflows/rigger.js`, installed by `rigger setup`) couriers `rigger step` and already stops
  loudly on a failing step, so it needs no change.
- *The refusal* exits non-zero and prints the text in Notes on stderr, its prefix the invoking
  command; it writes nothing to stdout, so a refused `rigger step` prints no JSON line. There is
  no override flag; the only ways past are a clean critique of the current text or a recorded
  resolution. A resolution is read from the backend the run entry selected, and `rigger emit`
  selects through configuration alone (`KURRENTDB_CONN`, `.rigger/store.conn`, the workflow's
  `store:` key), so under a server selected by flags alone the printed resolution route needs that
  selection configured; accepted.
- *The fixtures.* Every existing test that begins a new run through a CLI entry under a workflow
  with a critic records a clean critique for its fixture spec first, through one shared test
  helper under `tests/common/`; a fixture whose workflow has no critic needs no adversary and no
  critique. Criterion 2's unit owns the helper and those edits. Criterion 1's unit puts its
  stream-json stub agent writer in `tests/common/`; the helper runs `rigger critique <spec>` with
  that stub first on `PATH` for that one call only, so a test's existing fake agent keeps its
  `PATH` slot for the run it drives; no helper writes critique events itself. `rigger replay` and
  `rigger canary` drive the conductor in their own namespaces without these CLI sites and are
  unaffected.

**THE VALIDATE TELLS, decided here.** Three advisories join `spec::spec_lint_advisories`
(`crates/rigger-domain/src/spec.rs`), so `rigger validate <spec>` and the in-run `load_criteria`
print them through `spec_lint_warning_lines` (`src/cli/mod.rs`). Each is a `LintAdvisory` with
`criterion: Some(n)`, one per criterion per tell unless the tell says otherwise; advisory only, the
exit status never changes. Words match case-insensitively and whole-word through the file's
`find_word`. A sentence is the text between `. ` boundaries of a criterion's full block
(`criterion_blocks`); the measure, identity and comparison-surface words match on the sentence
masked by the file's one span masker, `strip_inline_code` (*The one masker*), and the twin tell
reads its spans, the backtick spans that mask blanks, from the sentence before the mask. A section
runs from its heading to the next heading of the same or shallower level, as `notes_section_lines`
reads one, so a heading-shaped line inside a fence counts as a heading (accepted).
- *The one masker.* Criterion 3's unit corrects `strip_inline_code` in place and adds no second
  masker: it blanks backtick spans paired by backtick run, as CommonMark delimits a code span (a run
  of n consecutive backticks opens a span that closes at the next run of exactly n backticks; a run
  with no closing run of its length blanks to the end of the text masked - the sentence for these
  tells, the paragraph for F4), and keeps its double-quote rule unchanged (one span from the first
  quote mark through the last when their count is even, to the end of the text masked when it is
  odd). The twin tell's one span reader pairs runs the same way and drops an empty span, so the
  tell never names an empty surface. A code span is a paired Markdown construct, so a measure
  word between two code spans is prose the twin tell must see; a stray quote is common prose, so
  quoted text keeps failing closed. A stray backtick run pairs with the opener of the next real span
  of its length, so that span's text is linted; accepted, since a stray backtick is a Markdown
  defect the rendered spec shows. The correction reaches F4 (`disposition_advisories`, the masker's
  one other caller, which masks a paragraph), whose prose between two backtick spans is now linted.
  In the same commit criterion 3's unit rewrites to the pair rule the masker's doc comment, the doc
  comment on `disposition_advisories`, the doc comments of the `tests/spec_lint.rs` cases whose
  names carry `backtick` and the F4 narrative of `spec_lint_self_clean_over_the_committed_corpus`;
  adds to `strip_inline_code_direct_exact_output_pins_the_one_span_per_kind_rule` one assertion
  pinning the pair rule (two backtick spans with prose between them) and keeps every existing
  assertion of that test, each of which holds under the pair rule; changes no other existing test
  assertion but the snapshot's F4 fire set; and records a `DecisionMade` that narrows
  `d66-mask-one-span-per-kind`, the id the masker's doc comment cites, to the quote kind, with that
  reason: it restates the quote rule, carries no `supersedes`, and governs
  `specs/66-ship-the-planning-discipline.md`, `crates/rigger-domain/src/spec.rs` and
  `tests/spec_lint.rs`.
- *Twin measured surface* (class `F10 landing-order circularity`): criteria i < j each hold a
  sentence carrying the same backtick span and a measure word from `exactly`, `zero`, `at most`,
  `no more than`, `reads no`, `costs`, `materializes`, `appends`. One advisory per pair and span,
  on criterion j, detail naming criterion i and the span. Criterion 3's fixture includes a twin
  whose measure word sits between two backtick spans of its sentence; a measure word between two
  quoted spans is masked and not seen (accepted).
- *Identity claim without a comparison surface* (class `F11 undecided removal`): a criterion
  sentence carrying `identical`, `byte-identical`, `equal to`, `equals` or `the same as`, and none
  of `projection`, `wire form`, `compared on`, `comparison surface`, `bytes of`, `ordered by`.
- *Identity claim with no removal corner* (class `F11 undecided removal`): a criterion carrying one
  of those identity words while no line of a section whose heading title starts with `Design` or
  `Notes`, each line read unmasked and fenced lines included, carries `drop`, `drops`, `dropped`,
  `removed`, `removal`, `absent`, `deleted` or `no longer`.
- *The corpus snapshot.* Criterion 3's unit extends `spec_lint_self_clean_over_the_committed_corpus`
  (`tests/spec_lint.rs`) with F10 and F11 pinned at their observed totals, and re-pins in the same
  commit F4's fire set, which the backtick pairing moves (F4 is the one existing class that reads
  the masker), as a regression snapshot; no precision over historical specs is claimed or vetted
  (advisory output). A Design amendment to this spec during the run re-pins any total it moves, in
  the same commit.

**THE SHIPPED SKILL, decided here.** `spec-preflight` is one more `SkillEntry` in `skill_registry`
(`crates/rigger-domain/src/docs.rs`), its body a `const` beside `PLANNING_A_SPEC_BODY` with a
renderer that ignores `ctx`, exactly like planning-a-spec. The body is the fenced block that closes
the Notes block, byte for byte (the fence lines excluded); the rendered file is that body plus the
operator-binary section `SkillEntry::render` appends to every entry. Adding the entry wires install,
render and drift: `install_skills` (`src/cli/mod.rs`, called from `cmd_setup` in `src/cli/setup.rs`)
installs it to `.claude/skills/spec-preflight/SKILL.md` and rewrites it only when absent or drifted,
`rigger docs` renders it to `skills/spec-preflight/SKILL.md`, and the docs-drift gate checks it.
Three per-entry pins do not extend themselves, and criterion 4's unit extends each: the
registry-size pin (`registry_names_all_five_per_operation_skills_exactly_once_each` in `docs.rs`, to
eleven entries), the accuracy pin (`assert_skills_reference_only_real_subcommands` gains the case
`spec-preflight` with `critique`, `validate` and `emit`), and the field-guide class list of
`docs_renders_the_planning_field_guide_second_handbook_page` (`tests/cli.rs`, through F11). The
failure catalog already holds F9 (unbounded claim surface), so the two new classes are F10
(landing-order circularity) and F11 (undecided removal): planning-a-spec's churn table in
`PLANNING_A_SPEC_BODY` gains one row each for F9, F10 and F11 in its existing shape, so the table
stays complete; `PLANNING_FIELD_GUIDE_BODY` gains one `### F10` and one `### F11` section with its
tell and countermeasure, each opening by naming itself an F3 shape and the simulation that finds it
(landing order; the DROPPED corner); and DROPPED and existing data join F3's countermeasure and
planning-a-spec's step 3, so the shipped docs carry one corner list of eight; planning-a-spec's
step 7 gains one sentence naming spec-preflight and, under a workflow with a critic,
`rigger critique <spec>` before launch. The rendered `docs/handbook/planning-field-guide.md` and
`skills/planning-a-spec/SKILL.md` are regenerated in the same unit.

**A SPEC DEFECT STOPS THE RUN AT PLAN-CRITIQUE, decided here.** `plan_critique_loop` numbers its
rounds by attempt: round `k` critiques at attempt `k`; its reject records `UnitFailed` under replay
key `<gate>/failed#<k>` with `attempts` `k + 1` and the adjudicator's output as `review_reason`;
the re-plan that follows is `spawn_id(<plan>, ROLE_REPLAN, k + 1)`, after which round `k + 1`
runs. `<gate>` is the gate stage and `<plan>` its producer.
- *The signal* is the plan-critique adjudicator's `cause`, parsed from a recorded `review_reason`
  by `spawn::Adjudication::parse`, the one verdict-line parse; `spec-ambiguity` is a spec defect
  and every other cause is not. The verdict paragraph of `build_dag_critique_prompt` gains the
  cause contract, which governs the gate over the adjudicator persona's generic wording: its
  ONLY-list gains "or a defect in a criterion's own text that no decomposition can remove";
  `spec-ambiguity` only when the upheld defect is in a criterion's own text and no decomposition
  can remove it (two criteria that contradict under every landing order, a criterion no plan can
  satisfy, a demanded mitigation no criterion owns); `decomposition-conflict` for every defect a
  re-plan can fix (twin units, a missing exclusion between units, a unit over the size cap, a unit
  owning no criterion, a split the planner chose); when unsure, `decomposition-conflict`.
- *The stop predicate* is one pure function over the current run slice (`read_current_run`, the
  run's own events from its `RunStarted`), `s` the attempt the gate's next round would run. It
  holds when the `UnitFailed` events whose replay key (`META_REPLAY_KEY`) is `<gate>/failed#<s-2>`
  and `<gate>/failed#<s-1>` both hold a `review_reason` whose parsed cause is `spec-ambiguity`,
  the re-plan `spawn_id(<plan>, ROLE_REPLAN, s - 1)` is recorded, no re-plan
  `spawn_id(<plan>, ROLE_REPLAN, s)` is recorded, and no event carries the stop's completion key
  `<gate>/spec-defect-escalated#<s-1>` (*The stop*). A re-plan is recorded when the slice holds its
  `SpawnRequested` or `SpawnResult` (the stepwise driver records both) or an event the re-planner
  emitted, which carries that spawn id as `META_SPAWN` (a blocking driver records no spawn event).
  So a first `spec-ambiguity` reject re-plans as today, and the stop fires on a `spec-ambiguity`
  reject that follows a re-plan which did not clear the previous one.
- *The one place.* `plan_critique_loop` evaluates the predicate at the head of every round, before
  the round spawns. On entry `s` is the seeded attempt, so a step re-entering after a crash between
  the stopping reject's `UnitFailed` and the stop's `UnitEscalated` completes the stop without
  spawning. After a live reject `s` is the post-remediation attempt: the reject branch records
  `UnitFailed` and returns to the round head, which decides in this order - the stop; else, for a
  reject this call recorded, today's remediation decision (escalate, or `re_plan` at `s` and the
  harvest); then the critique. A round a later step enters re-plans nothing, as today, and a gate
  re-entered by `rigger resume-unit` after a completed stop runs its next critique round as any
  resumed gate does.
- *The stop* records three events in this order, each under its own replay key: a
  `LessonLearned` (key `<gate>/spec-defect-lesson#<s-1>`) whose `about` is `[<spec>]` and whose
  summary names the upheld finding ids of that attempt's adjudication (none upheld is said as
  such), through `emit_lesson`, which gains an optional replay key and an explicit about list
  (this branch passes both; every other caller is unchanged); the existing `SpecDefect` event
  (`TYPE_SPEC_DEFECT`, key `<gate>/spec-defect#<s-1>`), its reason the halt text, whose fold
  `RunState.spec_defect` is the stop's durable run-state form; then `UnitEscalated` for the gate
  (key `<gate>/spec-defect-escalated#<s-1>`, the completion key), its payload `{id}` as on every
  escalation. These three records are the escalation criterion 5 names: the lesson and
  `SpecDefect` name the spec and the upheld ids. It does not call `re_plan`, and the existing
  escalate branch does not run. `<spec>` is the current run's `RunStarted.spec` as recorded; a run
  launched with another spelling of the path names that spelling (accepted), and an empty one
  gives an empty about list and a halt text naming `the spec`. A workflow stage that does not wait
  on the gate may have built code before a reject; the stop fires regardless.
- *The stop reason.* The stop sets an in-process spec-defect reason; `RunCtx::halt_reason` returns
  the budget reason when both are set, else `amend the spec and relaunch: plan-critique found a
  spec defect in <spec> (<ids>)`, which rides the step's existing `halted` field; `cmd_step` fills
  that field from hung liveness only when it is empty, so its precedence is budget, then spec
  defect, then hung liveness. A step whose gate stopped in this process returns its halted state
  before `check_coverage_or_flag`. `compute_attention` stamps its `halted` entry for the budget
  reason only; the stop's attention entry is the gate's existing escalated entry, and a hung spawn
  in the stopping step still stamps its own `halted` entry through `merge_hung_attention`
  (accepted). Later steps find the gate terminal and behave as after any plan-critique escalation
  today (the gate in `escalated`, or the coverage error when the held DAG leaves a criterion
  uncovered); the lesson and `SpecDefect` carry the amend route.
- *Relaunch* is the operator's: amend the spec, then, under a workflow with a critic,
  `rigger critique <spec>`, then a new run (criteria edited mint one; a Design-only amendment
  needs `--fresh`), which criterion 2 gates.

**CRITERIA 1 AND 5 SPLIT AT THE CRITIQUE PROMPT.** Criterion 1 owns moving the Rule 7 and Rule 8
bullets and the NOTE into `PLAN_CRITIQUE_RULES` with `build_dag_critique_prompt` reading it;
criterion 5 owns only the verdict paragraph and the gate's spec-defect branch (the predicate, the
round head and the stop). Each asserts its own text by containment, never the DAG prompt's whole
bytes, and neither asserts the other's text.

**CRITERIA 1 AND 2 SPLIT AT THE CRITIQUE RECORD.** Criterion 1 owns the record, the hash, the id
scheme, the critic lookup and the domain reader that returns a hash's critique and its findings;
criterion 2 owns the resolution filter, the refusal and its no-critic exemption, which calls that
lookup.

**CRITERIA 2 AND 5 SPLIT AT THE STEP.** Criterion 2 acts before a run is minted, at the CLI;
criterion 5 acts inside the conductor after a run is minted. Criterion 5's test mints its run
through `start_fresh` with a spec path, which `conductor::run` then adopts, drives
`conductor::run` with a test driver, and never enters a CLI mint site.

**CRITERIA 3 AND 4 SPLIT AT THE CLASS LABEL.** Criterion 3 owns the advisory class strings;
criterion 4 owns the catalog prose. The shipped skill names `rigger critique`; that verb's behavior
is criterion 1's, and criterion 4's unit asserts install, drift refresh, content and the three
pins.

**LANDING-ORDER SIMULATION** (ordered pairs sharing a surface; 1 lands first, 2 and 4 after it, 3
and 5 at any point, 6 last):

| First | Without | Shared surface | Does the first one's own text hold? |
|---|---|---|---|
| 1 | 2 | critique record, critic lookup | yes - under a workflow with a critic it records and answers, under one with none the verb refuses; no run start refuses yet |
| 2 | 1 | critique record, critic lookup, stub writer | not reachable - 2 needs 1 |
| 1 | 4 | `SUBCOMMANDS` | yes - 1 adds `critique` and regenerates the command-surface pages; no skill is involved |
| 4 | 1 | `rigger critique` named in the skill | not reachable - 4 needs 1 |
| 1 | 5 | DAG critique prompt | yes - the three rule texts move byte for byte into the const, the unit-size line follows it, and 1 asserts the const by containment |
| 5 | 1 | DAG critique prompt | yes - 5 edits only the verdict paragraph and the gate branch and asserts its text by containment, so 1's later reorder leaves it true |
| 2 | 3 | `load_criteria` | yes - 2 has it hand back the bytes it read, leaves its lint lines alone and asserts the refusal and no-critic lines by containment |
| 3 | 2 | `load_criteria` | yes - 3 asserts `rigger validate` warning lines and the lint's own functions only |
| 2 | 4 | the skill's record step | yes - 2 reads no skill |
| 4 | 2 | the skill's record step | yes - the skill states the author's rule, not the refusal; its `resolves` field is read by nothing yet |
| 2 | 5 | `rigger step` | yes - the refusal, or the no-critic line, comes before any plan exists: it hashes the bytes `load_criteria` read and calls the mint decision |
| 5 | 2 | `rigger step` | yes - 5's test mints its run through `start_fresh` and drives `conductor::run`, whose adopt-or-mint answer the extracted mint decision leaves unchanged, and enters no CLI mint site |
| 3 | 4 | F10/F11 class names | yes - 3 asserts warning lines only, not catalog text |
| 4 | 3 | F10/F11 class names | yes - the skill claims nothing about validate output |

**CONSTRAINTS WALK.**
- *Criterion 1.* Empty: a spec with no Done-when criteria is refused by the verb with the loop-ready
  message (*The store* orders the verb's refusals) and nothing is recorded; a workflow with no
  critic refuses every call, answered or not, and nothing is recorded; a critic that returns no
  finding line and an approving verdict line is a clean critique; a verdict that does not approve
  with no parsed BLOCKING finding is not a critique.
  Repeated: under a workflow with a critic, which every corner from here on assumes, answered from
  the store, zero spawns; the copies are skipped by id. Reverted: text back at an earlier hash is
  answered by that hash's critique. DROPPED: a later hash's critique inherits no finding or
  resolution of an earlier hash (the fixture deletes a line and sees none of the earlier findings);
  a pipe inside a field or a table-formatted line loses no finding to the parse. Concurrent: two
  calls may mint one spawn id; the first recorded result is the critique and each call prints what
  it reads back; calls that mint different attempts both record and the latest critique answers; one
  call's directory removal may cut a concurrent critique's transcript, never its recorded result
  (accepted); two calls answering one critique may both append a copy of one id (accepted); a
  critique during a live run adds only its run-attributed copies to the run stream and nothing to
  the wave, and the critic records nothing. Crash-resume: a request with no result is ignored and
  the next call spawns the next attempt; a critique whose copies were not appended is completed on
  the next call, which also removes the scratch directories the crash left; the refusal never
  depended on them. Cold start: every answer is read from the store; a project with no store gets
  one; a server selected by flags alone and configured nowhere reaches the critic through the handed
  `KURRENTDB_CONN`, so its `rigger_peers` reads that server. Existing data: no critique exists and
  no critique data is migrated; a pre-spec-09 store has its identity migrated before the verb's
  first append; a graph.db that owes its rebuild refuses every call, answered or not; a project
  with no git repository resolves the spec path against its project root and runs with an empty
  scratch root.
- *Criterion 2.* Empty: no critique refuses as not critiqued, as does a new run on a spec outside
  the repository, whatever its bytes; a stream with no `RunStarted` mints, so its first command
  needs a critique; a run with no spec is never refused; a new run under a workflow with no critic
  is never refused and prints its one line. Repeated: each refused command
  refuses again until cleared, reusing the anchor, migration and registration the first one left;
  under a workflow with no critic each command that begins a new run prints the line again.
  Reverted: an earlier hash's critique and its resolutions apply again; under a workflow that
  names a critic again the refusal applies again, judged on any critique already recorded for the
  hash. DROPPED: a `resolves` id absent from the current critique is ignored; a resolution later
  superseded without `resolves` stands until the text changes; a renamed spec loses its
  resolutions; a criterion dropped from the spec mints a new run, which is refused until
  critiqued; a new run under a workflow that dropped both adversary keys is not refused, and every
  recorded critique stays in the store; a changed critic persona leaves every recorded critique
  standing, since the record keys on the spec bytes alone. Concurrent: the check reads committed
  rows; a critique or resolution landing a moment later is seen by the next command.
  Crash-resume: the check writes nothing; a resumed run is never refused. Cold start: log and
  workflow only. Existing data: every run already in the store is adopted and proceeds; under a
  workflow with a critic, a `--fresh` restart of a campaign that predates this spec is refused
  until its spec is critiqued; a consumer workflow naming no adversary begins new runs as before,
  with the one line; a run branch holding another copy of the spec cannot change the hash, taken
  from the bytes read before the anchor.
- *Criterion 3.* Empty: no criteria, no tells; an unclosed backtick run blanks from it to the
  sentence end, and an odd count of quote marks blanks from the first quote mark to the sentence
  end; a double-backtick span masks as one span; an empty span is not a surface. Repeated: a span
  shared by three criteria yields one advisory per pair; an F11 tell fires once per criterion
  however many of its sentences offend. Reverted, DROPPED, concurrent, crash-resume: out of scope -
  a pure function of one text. Cold start: pure. Existing data: committed specs may warn (advisory
  only), a real span after a stray backtick included; the corpus snapshot pins F10 and F11 at their
  observed totals and re-pins F4's fire set, which the backtick pairing moves; every other existing
  test assertion stands unchanged and one pair-rule assertion is added (*The one masker*); any
  amendment of this spec that moves a total re-pins it.
- *Criterion 4.* Empty: a project with no skills directory gets one. Repeated: a rerun
  writes nothing. Reverted: an edited installed copy is refreshed like any drifted registry skill.
  DROPPED: an installed copy with a line deleted is refreshed (drift compares the rendered bytes
  with the file's bytes); a hand-authored project copy is overwritten as drift; a copy outside the
  project is never touched (Notes). Concurrent, crash-resume: out of scope - one file write per
  skill, as today. Cold start: pure render. Existing data: projects gain the file on their next
  `rigger setup`.
- *Criterion 5.* Empty: a stopping reject that upheld no finding still stops, its lesson and halt
  text saying none were upheld; a slice holding fewer than two gate rejects never satisfies the
  predicate; a run whose `RunStarted.spec` is empty stops with an empty about list and a halt text
  naming the spec. Repeated: a third `spec-ambiguity` reject after the stop cannot occur, because
  the stopped gate is terminal and spawns nothing; a later step finds it terminal and appends
  nothing, and a replayed step re-reaching the stop appends nothing (each record under its own
  key). Reverted: a gate
  reopened by `rigger resume-unit` runs its next round, and its next `spec-ambiguity` reject is
  treated as a first one (the stop made no re-plan), so the remediation bound decides as today.
  DROPPED: a re-plan that emitted nothing under a blocking driver reads as absent, so the next
  `spec-ambiguity` reject re-plans again instead of stopping and the remediation bound decides
  (accepted). Concurrent: the step lock serializes steps and the gate runs
  synchronously in the producer prelude. Crash-resume: the stop is re-derived from the log - a step
  re-entering after a crash between the stopping reject's `UnitFailed` and the stop's
  `UnitEscalated` finds the predicate true at the round head with `s` the seeded attempt,
  completes the keyed records (those already recorded append nothing) and halts without spawning.
  Cold start: log only; the halt is in-process for the step that stops. Existing data: the
  predicate reads the current run slice only, so `spec-ambiguity` rejects of an earlier run in the
  store are never read across the run boundary; a recorded run that re-planned after its second
  `spec-ambiguity` reject holds that re-plan (`rigger replay` seeds its `SpawnResult`), so the
  predicate is false and replay follows history; an adopted run mid-gate follows the new rule from
  its next round head, and a gate `UnitFailed` with no `review_reason` parses no cause and counts
  as not `spec-ambiguity`, so such a run needs two rejects that carry one before the stop fires.

**OUT OF SCOPE.** Moving the loop's own spawns onto the headless host; a size limit on anything;
any `rigger reset` change for the critique stream (it holds only the critique spawns' requests and
results).

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

The refusal text, `<command>` the invoking command (`governs` and `resolves` are JSON arrays):

```
<command>: refusing to begin a new run on <spec>: <not critiqued | open BLOCKING findings: <ids>>
  amend the spec and critique it:   rigger critique <spec>
  or record a resolution:           rigger emit DecisionMade '{"id":"...","governs":["<spec>"],"resolves":[<ids>],"summary":"..."}'
```

In the not-critiqued case the last line opens `or, once critiqued, record a resolution:`.

The no-critic line, printed on stderr by a command that would begin a new run under a workflow with
no critic, which then proceeds:

```
<command>: no spec critique for <spec>: the workflow names neither the plan-critique gate's adversary nor defaults.review.adversary
```

A copy of a skill named `spec-preflight` outside the project is never touched by rigger; retiring
it is the operator's.

The shipped `spec-preflight` body, byte for byte between the fence lines:

```
---
name: spec-preflight
description: Use before launching any rigger spec, after planning-a-spec - simulate landing order and walk every criterion's corners, then run rigger critique on the spec and close every BLOCKING finding before launch.
---

# Spec preflight

## Why

A checklist read in the author's head does not simulate the run. Two spec defects survive
planning-a-spec and each costs a whole loop round:

- Landing-order circularity (F10): two criteria assert the same measured property of one
  command, and each one's fixture needs the other's change. Whichever unit lands first fails its
  own text, and plan-critique finds it only at run time.
- Undecided removal (F11): a criterion claims an identity (byte-identical, equal) and no Design
  sentence decides what a later generation that DROPS a fact does to it. The corner walk never
  reaches it, the implementer narrows the fixture, and the round is lost.

This skill makes you EXECUTE two simulations and one adversarial pass before launch. It does not
repeat planning-a-spec (`../planning-a-spec/SKILL.md`); run that recipe first, then this.

## When

- After the draft passes planning-a-spec.
- Before `rigger validate`.
- Before any launch or relaunch.
- Again after any mid-run Design amendment.
- Never skipped for a small spec.

## Step 1: landing-order simulation

1. Table the criteria in the order their units land (follow the needs chain; independent units
   in any order).
2. List every measured surface each criterion asserts over: a command, a store read, a file, a
   counter, a test double.
3. For every ORDERED PAIR (A, B) sharing a surface, write one line:
   `If A lands first, on a tree WITHOUT B, does A's own text hold? yes/no - why.`
4. Every "no" is a defect. Fix it now, one of three ways:
   - move the assertion to the later unit;
   - split ownership at the seam in a Design block named `CRITERIA A AND B SPLIT AT <seam>`;
   - make the earlier criterion's assertion conditional on what exists at its landing.
5. Rule: a criterion is testable in isolation on the tree it lands on, never only on the
   finished tree.

Worked example (shared surface: the reads one command makes):

    C2 first, without C3: C2 asserts "the command reads no derived event", but excluding
      derived reads needs C3's query to serve them -> NO.
    C3 first, without C2: C3 asserts the same, but its seed relies on C2's exclusion -> NO.
    Fix: Design block "CRITERIA 2 AND 3 SPLIT AT the derived-read seam": C2 owns the
      exclusion, C3 owns the query; only the later-landing unit asserts "reads no derived event".

## Step 2: per-criterion corner walk

For EACH criterion's mechanism (not only per global constraint), write one sentence per corner,
or "out of scope - <reason>":

| Corner | Question |
|---|---|
| empty | No input, no rows, no prior generation. |
| repeated | The same input twice. |
| reverted | An earlier state re-asserted later. |
| DROPPED | A fact, row, file or link present in an earlier generation and absent in a later one. |
| concurrent | Two actors on the mechanism at once. |
| crash-resume | The process dies mid-mechanism; the next run resumes from the log. |
| cold start | A fresh process, empty memory. |
| existing data | A store or tree that predates the mechanism - the upgrade path. |

Rules:
- Every identity claim (identical, equal, the same, byte-identical) names its comparison
  surface: which bytes, which projection, which ordering.
- Its fixture includes the DROPPED corner explicitly. A fixture that only adds or moves facts
  does not test an identity claim.
- A corner no Design sentence decides is a defect; decide it in Design now.

Worked example (an identity claim over a compacted log):

    DROPPED: generation N links doc D -> E; generation N+1 re-ingests D without the link.
      The compacted log keeps only N+1; the original log holds N and N+1. Undecided: does
      N+1 supersede N's link in the fold? If not, the two rebuilds differ -> the claim is false.
    Fix: Design "a generation supersedes the whole prior generation; the comparison surface is
      the live projection, not raw bytes"; the fixture drops a link and drops an entity.

## Step 3: the adversary pass

Run `rigger critique <spec>`. It runs the workflow's critic (the plan-critique gate's adversary,
else `defaults.review.adversary`) against the spec text, records its findings and verdict keyed on
the text's content hash, and prints them; unchanged text is answered from the record, and any edit
is critiqued afresh. Under a workflow naming neither, `rigger critique` refuses and runs are not
gated, so skip Steps 3 and 4.2. Each finding is one line:

    <id> | BLOCKING or NON-BLOCKING | <criterion or Design block> | <exact reading that breaks> | <smallest Design change that closes it>

## Step 4: resolve, record, re-run

1. Close every BLOCKING finding in Design or Global constraints. Once a run has built code, never
   edit a criterion. Before any run, a criterion rewrite is allowed, and Steps 1-3 then run again
   from the top.
2. Run `rigger critique <spec>` on the amended text until it returns no BLOCKING finding. Close or
   record each NON-BLOCKING one.
3. Close a BLOCKING finding you judge wrong, or already decided, with a recorded resolution
   instead of a text change (`<ids>` are the quoted finding ids):

       rigger emit DecisionMade '{"id":"preflight-<short>","governs":["<spec>"],"resolves":[<ids>],"summary":"closed <ids>: <one line each>"}'

4. `rigger validate <spec>`.
5. Launch.

Refusal rule: do not launch, relaunch, or amend-and-continue with an open BLOCKING finding.

## Mid-run amendment

When a review or plan-critique reject names a defect in the spec itself:

1. Amend Design and Global constraints only; a criterion edit orphans the live run.
2. Land the amendment between steps, never while a step is mid-flight.
3. `rigger emit DecisionMade` with the spec path in `governs`, so in-flight agents see it through
   the graph.
4. Run Steps 1-3 on the amended spec before the next step spawns.
```

## Global constraints

- Hyphens, never em or en dashes, in every added line.
- No new event type; no new crate dependency.
- Both feature lanes green (fmt, clippy -D warnings, test on default and --no-default-features).
- No API key and no third-party service: the critic runs on the operator's own agent CLI login.
- The operator's installed rigger binary is never replaced or modified by any unit.
- Fewest moving parts: one verb, one record, one refusal function, one stop predicate.

## Done when

- [ ] a test proves A SPEC IS CRITIQUED BEFORE THE RUN: `rigger critique <spec>` spawns the plan-critique adversary on the spec text through the headless host and records its findings and verdict keyed on the content hash,
  and a second call on unchanged text answers from the store with zero spawns, asserted with a
  stub agent binary first on `PATH`. This criterion OWNS the verb, the critique record, the
  critique prompt and `PLAN_CRITIQUE_RULES`; the refusal is criterion 2's and the plan-critique
  stop is criterion 5's, NOT this one's.
- [ ] a test proves A NEW RUN NEEDS A CLEAN CRITIQUE: under a workflow naming a critic, a `rigger step --spec <spec>` that would begin a new run refuses, printing each open BLOCKING finding id and the two clearing commands,
  when the spec's hash has no critique or holds a BLOCKING finding no later `DecisionMade`
  resolves, while a step adopting the spec's existing run proceeds. This criterion OWNS the
  refusal at every CLI run start, its no-critic exemption and the shared critique fixture helper;
  the critique record and the critic lookup are criterion 1's, NOT this one's.
- [ ] a test proves VALIDATE NAMES THE PREFLIGHT TELLS: `rigger validate <spec>` warns with the criterion number on a twin measured surface, an identity claim naming no comparison surface,
  and an identity claim whose Design and Notes never decide removal. This criterion OWNS the
  three text checks and their class labels; the catalog prose is criterion 4's, NOT this one's.
- [ ] a test proves SETUP SHIPS THE PREFLIGHT SKILL: `rigger setup` installs `spec-preflight` from the skill registry with no absolute or home path in it and refreshes it on drift like every registry skill, with F10 and F11 rows in the catalog.
  This criterion OWNS the shipped skill text, its registry entry and the F10 and F11 catalog
  rows; the validate tells are criterion 3's and the verb is criterion 1's, NOT this one's.
- [ ] a test proves A SPEC DEFECT STOPS THE RUN AT PLAN-CRITIQUE: a `spec-ambiguity` plan-critique reject after a re-plan that did not clear the previous `spec-ambiguity` reject records the escalation naming the spec and its upheld findings
  and halts the step with "amend the spec and relaunch" without re-planning, while a first
  `spec-ambiguity` reject re-plans as today. This criterion OWNS the gate's spec-defect branch and
  the verdict paragraph of the DAG critique prompt; the rules paragraph is criterion 1's, NOT
  this one's.
- [ ] both feature lanes green (fmt, clippy, test on default and --no-default-features). This
  criterion OWNS only the lanes over the integrated result.
