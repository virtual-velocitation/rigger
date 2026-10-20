# 120 - Rigger is language and process agnostic

**Goal:** rigger provides tools to agents (the context graph, the event store courier, worktrees,
the blast radius, the dash, hooks, MCP, the verdict and gate contracts) and executes the workflow
definition; it prescribes nothing about how work is done except by enforcing that definition. Two
places at HEAD prescribe instead. The built-in instruction layer has no override: `compose`
(`crates/rigger-domain/src/instructions.rs:50-62`) injects every `BUILTIN` entry (:32-41) under
`HEADING` with the suffix "(non-negotiable)" (:54), `render` prints "Built-in (every agent rigger
spawns, not removable)" (:68), and the module doc says "no configuration removes it" (:11). And the
review ceremony is one fixed shape: `ReviewPanel` (`crates/rigger-domain/src/config.rs:355`) holds
one lens list, one `adversary` and one `adjudicator`, and `review_unit`
(`crates/rigger-conductor/src/conductor.rs:4773`) runs them as three fixed tiers (:4829, :4856,
:4885), so a consumer cannot add a second adversary, a lens after the adversary, or a second judge.
This spec makes the built-in layer overridable by name and the review panel an ordered sequence of
steps of the three official kinds, with the existing map form meaning the canonical sequence.

## Design

**THE RULE, decided here.** The handbook states it in two sentences, pinned by criterion 1's test
by their words (whitespace-normalized):

- S1: "Rigger is language and process agnostic: the binary provides tools to agents and executes
  the workflow definition, and it prescribes nothing about how work is done except by enforcing
  that definition."
- S2: "Prompts are guidance and logic is execution: the built-in instruction layer is best practice
  a consumer may replace or switch off, never a procedure standing in for a gate and never specific
  to a language."

Around them `docs/handbook/authoring-loops.md` gains a `## What rigger prescribes` section (after
the page's opening paragraph): the tools the binary provides, that a consumer may add any number of
stages, gates and review steps and rigger runs them, that gate template sets are the consumer's
choice at `rigger init`, and the official review vocabulary (lenses, adversary, adjudicator). No
enumeration of cases or guarantees beyond these is owed.

**BUILT-IN OVERRIDE BY NAME SHADOWING, decided here so no unit has to.** One pure function,
`instructions::resolve_layers(operator: &[Instruction])`, decides every built-in's state and the
remaining operator files; `compose`, `render` and `shadow_advisories` all read it, none re-derives
it. For each `BUILTIN` entry in order: an operator `Instruction` whose `name` equals the built-in's
name byte for byte (case-sensitive, never trimmed) with a body that is not empty after trimming
whitespace REPLACES it (state `Replaced`); one with an empty or whitespace-only body SWITCHES IT OFF
(state `SwitchedOff`); otherwise it stays `BuiltIn`. A shadowing file never also appears in the
operator layer. Every other operator file appends after the built-ins in filename order, as today.
A near name (`engineering-principles .md`, whose stem carries a trailing space, or
`Engineering-Principles.md`) shadows nothing and appends; `render` lists it under the operator
heading, which is how an operator sees it did not shadow.

- `compose` prints `HEADING` bare (the " (non-negotiable)" suffix goes), then each built-in section
  under `## <name>`: the built-in body (`BuiltIn`), the operator body (`Replaced`), or nothing at all,
  heading included (`SwitchedOff`); then the unshadowed operator files. `HEADING` is printed even
  when every built-in is switched off and no operator file remains (`rigger prime` and the tests key
  on it).
- `render` prints `HEADING`, then `Built-in:`, then each built-in as `## <name> (built-in)`,
  `## <name> (replaced by .rigger/instructions/<name>.md)` followed by the operator body, or
  `## <name> (switched off by .rigger/instructions/<name>.md)` with no body; then the operator
  heading and the unshadowed operator files, `(none)` when none remain. The "not removable" label
  goes.
- `shadow_advisories(operator) -> Vec<String>` returns one line per non-`BuiltIn` built-in:
  `warning: built-in instruction <name> is replaced by .rigger/instructions/<name>.md` or `... is
  switched off by .rigger/instructions/<name>.md` (the `warning:` prefix `cmd_validate`'s other
  advisories carry). `cmd_validate` (`src/cli/validate.rs:140`) prints
  each to stderr beside its other advisories (:229) and never changes its exit status. It reads the
  operator layer through `config_store::load_instructions`, the one loader.
- `BUILTIN` SURFACE, decided here: criterion 5 owns the only new assertion that iterates every
  `BUILTIN` entry (the README names each); the HEAD test
  `an_empty_persona_still_receives_every_built_in_layer` (`instructions.rs:131-147`) is
  pre-existing and exempt. Criteria 2 and 3 assert over named entries and read only `compose`'s
  output, and criterion 4 reads only `render`'s, `shadow_advisories`' and
  `instructions_in_force_line`'s output over a fixture of named entries ("each built-in" there means
  each entry of that fixture).
- Before criterion 3 lands, an empty same-named file leaves the built-in `BuiltIn` and appends as an
  operator file, as at HEAD.
- The module doc's sentence "no configuration removes it" (:11) becomes the override rule in one
  sentence.
- `SCAFFOLD_INSTRUCTIONS_README` (`src/cli/setup.rs:1499`) documents the mechanism and names every
  built-in by file name; this repository's own `.rigger/instructions/README.md`, today byte-equal
  to the scaffold text, is rewritten to the new text. Another project's README stays as written
  (`write_if_absent`, `src/cli/setup.rs:157-162`, never rewrites one); its `render` and `validate`
  report the override regardless.
- `instructions_in_force_line` (`src/cli/setup.rs:1410-1426`), the first line `rigger prime` prints,
  reads `resolve_layers`: it lists only the built-ins in force, in `BUILTIN` order, a replaced one as
  `<name> (replaced)`, a switched-off one omitted (`none` when no built-in is in force), and counts
  only the unshadowed operator files in its `(+N operator files)` suffix.
- Unchanged, decided: `BUILTIN`'s text and the tests
  reading it (`instructions.rs:150`, `src/cli/setup.rs:3484`, `crates/rigger-domain/src/docs.rs:2525`);
  `RIGGER_COMMUNICATION`'s own "(non-negotiable)" (`conductor.rs:12446`), which is rigger's courier
  protocol, not the instruction layer; `load_instructions` (`crates/rigger-config-files/src/config_store.rs:92-123`).

**INSTRUCTION STATE PLACEMENT, decided here.** The authoritative state is the files under
`.rigger/instructions/` on disk. `config_store::load_with_gate_requirements` reads them once per
driver process into `Config::instructions` (`config_store.rs:75`); `compose` resolves shadowing on
every call from that slice. No composed prompt and no shadow resolution is cached anywhere: a
memoized resolution kept across spawns is NOT an implementation of this design. The definition pin
already covers the files (`definition_hash` folds every loaded instruction, `src/cli/mod.rs:1006-1011`),
so an operator file added, emptied or deleted mid-run halts the next step as a definition drift;
under `--rebase-definition` the next process composes from the files as they then are.

**THE REVIEW PANEL IS A SEQUENCE, decided here.** A panel is an ordered list of steps, each exactly
one official kind:

- `lenses: [a, b, ..]` - the agents run concurrently, each recording findings through
  `review_protocol` (`conductor.rs:12506`);
- `adversary: x` - one agent that reads every finding recorded so far in the round and records its
  own;
- `adjudicator: z` - one agent whose verdict line (`REQUIRED_PROTOCOL`, `conductor.rs:12535`) the
  conductor reads.

Type shape: `ReviewStep` is `enum { Lenses(Vec<String>), Adversary(String), Adjudicator(String) }`.
`ReviewPanel` keeps `lenses`, `adversary`, `adjudicator` and `tiers`, and gains
`steps: Option<Vec<ReviewStepDecl>>`, where `ReviewStepDecl` is a `deny_unknown_fields` struct of
three optional keys (`lenses`, `adversary`, `adjudicator`) that `Config::validate` requires to set
exactly one; no serde enum representation is relied on. `ReviewPanel::steps() -> Vec<ReviewStep>`
is the ONE authority every reader goes through: the declared list when `steps` is `Some`, else the
CANONICAL SEQUENCE of the map form - `Lenses(lenses)` when `lenses` is non-empty, then
`Adversary(adversary)` when set, then `Adjudicator(adjudicator)` when set. `is_empty`,
`agent_ids`, `full_roster` and `gating_agent_ids` (`config.rs:381`, :396, :407, :419) derive from
`steps()` (criterion 6's, today's shape over the canonical sequence). Criterion 7 makes the last
three return each agent id once, in first-step order, however many steps name it (a map form naming
one agent as lens and adjudicator included); under `tiers`, `agent_ids` and `gating_agent_ids` list
the full panel's ids first, then the light panel's ids not already listed, each once. So the
workflow-definition grounder (`reviewers_of`,
`crates/rigger-grounder/src/grounder/workflowdef.rs:144`), the verdict-line lint (`lint_gating_verdict_lines`) and the wall-clock advisory
(`unbounded_wall_clock_advisory`, `crates/rigger-config-files/src/config.rs:107`), both reading the
`gating_agent_ids` set (:21), read every adjudicator step with no change of their own, each naming an agent once. The advisory
sums nothing: it names each adjudicator step's agent that carries no per-agent bound under an
unbounded default, as it names the one adjudicator today. Criterion 7 owns the dedup and these
three readers' naming-once behaviour through the accessors, with its tests; none of it is criterion
6's or 10's. `steps: []` is an empty panel, exactly as `review: {}` is: no review, and on a stage it
inherits `defaults.review` through `effective_review_panel` (`config.rs:958`) as an empty map does.

**CANONICAL IDENTITY SURFACE, decided here.** Criterion 6's identity claim compares, for each
fixture, the ordered list of (spawn id, `SpawnOpts.reviews`, review task prompt, verdict) a
recording driver receives against the same list on the tree before criterion 6, captured as a
golden fixture in the test. The task prompt is `build_review_prompt`'s (`conductor.rs:11029`) and
`run_adjudicator`'s, NEVER the system prompt, which criterion 2 changes (the suffix) and which is
not this surface. The fixtures: this repository's `.rigger/workflow.yml` panel, a lenses-only panel,
a lenses-plus-adjudicator panel (the DROPPED case: the adversary is absent, so the adjudicator's
roster names no `adversary`), and a `tiers` panel routed both light and full. The existing roster
pins (`conductor.rs:32877-32885`) and the replay tests stay green unchanged. Criterion 6's loop
applies `split_reject` to every `Adjudicator` step's verdict exactly as at HEAD (:4911-4915,
:5033-5034), so the identity holds on a converged reject too.

**PARSE AND REFUSALS, decided here.** `Config::validate` (`config_store.rs:286`) refuses, naming
the scope (`defaults.review`, `defaults.review.tiers.light`, `stage "<name>" review`, `stage
"<name>" review.tiers.light`) and, for an entry, its 0-based index: a panel that sets `steps` and
any of `lenses`, `adversary`, `adjudicator`; an entry setting zero or two or more keys (an unknown
key already fails the YAML parse); a `lenses` entry with no agent; an `adversary` or `adjudicator`
entry with an empty id; an agent named twice in one `lenses` step (in either form: two spawns of
one step would mint one spawn id, so no working workflow carries one); a review agent id
containing `@` (in either form: the step-token separator below). `validate_depth` (`config.rs:446`)
becomes: a panel with a `tiers` policy, and its `light` panel, each have at least one
`Adjudicator` step in `steps()`; its two messages keep their wording with "name an adjudicator"
reading as "name an adjudicator step". `tiers.light` takes either form. Until criterion 8 lands,
criterion 7 also refuses a list naming two steps of one kind (on that tree both would mint one
spawn id) and a list whose `Adjudicator` step is not last (on that tree no step after it has an
early-stop rule); criterion 8 lifts both refusals with the step tokens and the early stop. The same agent may appear in
several steps (two adversary steps naming one agent are two steps).

**STEP ROLE TOKENS, decided here.** A spawn id is `{unit}/{role}#{attempt}` and unique per role
(`crates/rigger-domain/src/spawn.rs:140`), so every step needs its own role token, and a map-form
panel must keep today's. `spawn::review_step_role(kind, agent, k)` mints, for the k-th step of its
kind in `steps()` (1-based, counted among steps of that kind only): the kind's existing token when
`k == 1` (`lens_role(agent)` for each lens, `ROLE_ADVERSARY`, `ROLE_ADJUDICATOR`; `spawn.rs:115`,
:95, :97), and that token followed by `@<k>` when `k >= 2` (`lens:c@2`, `adversary@2`). `@` is
neither `/`, `#` nor `~`, so `spawn_role` (`spawn.rs:222`) and `unit_of` stay correct. The ONE
parser `spawn::review_step_of(role) -> Option<(ReviewKind, u32)>` reads a token back (a bare token
is step 1; a suffix other than `@` and an integer >= 2 is no review token); criterion 8 migrates
`SpawnResult::is_adjudicator` (`spawn.rs:653`) to it, and the other production classifiers are
criterion 9's and 10's by name (READERS lists every site). The token is the step's label
everywhere: the spawn id, the `tier` argument `run_reviewer` (`conductor.rs:7321`) names in its
errors, the finding's `by` field (`review_protocol(actor)`), the session a later round resumes
(`continue_prior_session`, `conductor.rs:4569`, keyed by role). The tokens are a pure function of
`steps()` and the step's index: an in-memory counter of steps run so far is NOT an implementation
of the ordinal, since a resumed process starts it at zero. The plan-critique gate's and the
critique run's tokens (`review::critique_spawn_id`, `crates/rigger-domain/src/review.rs:392`) are
unchanged.

**EXECUTION, decided here.** `review_unit` routes the panel exactly as today (`select_review_panel`,
`conductor.rs:4396`; `log_review_tier`, :4419), approves an empty panel at once (:4805-4807), then
runs `steps()` in order, one step at a time: a `Lenses` step through
`run_review_agents_concurrently` (:7172), an `Adversary` step through `run_adversary` (:7681), an
`Adjudicator` step through `run_adjudicator` (:7739), each under its step role token. On a later round, a step whose role token has no recorded spawn
for the unit in the round that sent it back (it never ran, because an earlier adjudicator step
rejected) runs as on a first round: the whole-unit prompt with no round block and a fresh session;
every other step gets the round's block (`round_delta`, :4818-4825) as today. The round that sent the unit back is the attempt whose `STATUS_REVIEW_ROUND_START` mark
`round_delta_base` (:13791) read; `round_delta` (:4680) returns that attempt beside `base`, and a
step's "recorded spawn" is a spawn id `{unit}/{token}#{that attempt}` in the log, at any infra-retry
ordinal. When `round_delta` is `None`, every step runs as on a first round. A step that runs as
on a first round is passed `round = None` everywhere: its prompt, `continue_prior_session`, the
degenerate-verdict check (`ReviewerOutput::Verdict`, never `RoundVerdict`) and, for an adjudicator,
`split_reject`. An `Adjudicator` step whose verdict is a reject ends the round: no later step
spawns; the verdict this early stop reads is the post-split one criterion 6's loop computes (THE
ROUND'S VERDICT). The tree
guard runs as today: `guard_review_round_tree` after the last step that ran, and
`guard_review_round_tree_on_tier_err` when any step's spawn errors.

**ROSTERS, decided here.** "A step carries the ids of every earlier step's agents" means its
`SpawnOpts.reviews` roster (`crates/rigger-domain/src/agent.rs:142`), the channel the thin driver
renders today; no prompt text changes. `review::step_roster(steps, index)` returns the role tokens
of every spawn of every step before `index`, in step order and, within a lens step, in declared
order. A first `Lenses` step's roster is empty, as `run_lens`'s is today (`conductor.rs:7244`); a
later lens step's roster names the steps before it. `review_roster` and `adjudicator_roster`
(`review.rs:257`, :265) become wrappers over `step_roster` on the canonical sequence, so the
standalone review stage and the plan-critique gate (`conductor.rs:6991-7008`, :8262) keep their
rosters through the one rule.

**THE ROUND'S VERDICT, decided here.** Each `Adjudicator` step's verdict is read after its output
goes through `split_reject` (`conductor.rs:4996`): a reject the split keeps ends the round (the
early stop is EXECUTION's) with the kept REQUIRED list the next round holds, and a
converged reject (`(true, CONVERGED, [])`, :5033-5034) counts as an approve, so the round
continues. An approving step's output is recorded by its spawn and carried nowhere else. When the
last step has run and no adjudicator rejected, the round approves; its reason (the last adjudicator
step's post-split reason), compensation target (`verdict_compensates`, :4910) and resolved model are
the LAST adjudicator step's, and the one `reviewed#{attempt}` stamp (:4958) carries that step's model
alias. A rejected round's reason and compensation target are the rejecting step's, post-split; a
compensation target named by an approving step that is not the last adjudicator step is dropped by
design. `verdict_compensates` reads each adjudicator step's raw output before `split_reject`, as at
HEAD (:4910); only the reason is post-split. A sequence with no adjudicator step approves when its last step ends, with no `reviewed`
stamp, as an adjudicator-less map panel does today (:4879-4884). The speculation winner's deferred
`reviewed#{lane}` stamp (:6758-6759) keys on "the panel has an adjudicator step" and the last
adjudicator step's alias. On a later round every adjudicator step that ran in the prior round ends its
prompt with the round's block, and a reject naming no REQUIRED item is degenerate for each such step
(`ReviewerOutput::RoundVerdict`).

**READERS, decided here.**

Every production site at 1185c8c6 that mints, compares or classifies `ROLE_ADVERSARY`,
`ROLE_ADJUDICATOR`, `lens_role`, the `lens:` prefix, `is_adjudicator` or `spawn_role`, and its owner:

| Site | What it does | Owner |
| --- | --- | --- |
| `spawn.rs:654` `is_adjudicator` (read by `adjudication`, :663, the review-quality and finding-expiry folds, and the console, `console/mod.rs:262`) | classifies | 8 |
| `conductor.rs:7258`, :7265, :7696, :7703, :7774 (`run_lens`, `run_adversary`, `run_adjudicator`) | mints | 8, through `review_step_role` |
| `review.rs:258`, :268 (`review_roster`, `adjudicator_roster`) | mints the roster | 8 |
| `metrics.rs:240-245` `review_tier`, :257 `actor_tier`, :1060 and :1065 the adversary-only fold | classifies | 9 |
| `dash.rs:2537-2539` `stage_of_role` (called at :2357), :2554-2559 `role_and_agent` | classifies | 9 |
| `workflows/rigger.js:325-343` `roleOf`/`phaseOf`, :365 `PERSONA_VERB`, :384 `ROSTER_VERB` | classifies | 9 |
| `canary_store.rs:175`, :443, :460, :479, :490, :549, :551 | mints and keys tiers | 10 |
| `conductor.rs:13777` `recorded_session` | compares a whole token for equality | unchanged: one session per step token is the design |
| `metrics.rs:1024` spawn timing, :1289 tier seeds | keys by whole token; seeds kind names | unchanged: `adversary@2` is its own timing row |
| `conductor.rs:8228`, :8235, :8250 (plan-critique gate), `review.rs:393` `critique_spawn_id` | mints | unchanged: out of scope |
| `src/cli/eval.rs:94` | parses `--model` tier names | unchanged |


- `metrics::review_tier` and `actor_tier` (`crates/rigger-domain/src/metrics.rs:239`, :256) key cost
  and precision on the kind (`lens`, `adversary`, `adjudicator`); the adversary-only fold (:1060)
  counts a finding by any adversary-kind actor about files no lens-kind actor flagged;
  `finding_survival` stays keyed by the full token, so `adversary@2` is its own row.
- `dash::stage_of_role` and `role_and_agent` (`crates/rigger-dash/src/dash.rs:2534`, :2553) put every
  review-kind token in `Review` and group it under its kind, the agent label gaining ` step <k>`
  for `k >= 2`.
- `workflows/rigger.js`: `phaseOf` (:340), `PERSONA_VERB` (:365) and `ROSTER_VERB` (:384) look up
  the token with a trailing `@<digits>` stripped, mirroring `review_step_of`; the persona label
  keeps the full token. A lens step after another step now carries a roster; `workerLabel` (:463)
  renders a lens's base phrase and ignores its roster, as `ROSTER_VERB` names no lens.
- The console reads verdict marks through `SpawnResult::is_adjudicator`
  (`crates/rigger-console/src/console/mod.rs:262`), so it marks every adjudicator step's verdict
  with no change of its own.
- `wave::critic` (`crates/rigger-domain/src/wave.rs:204`) takes `defaults.review`'s first
  `Adversary` step; `NO_CRITIC_CLAUSE` (`wave.rs:198`) and the `rigger critique` help
  (`src/main.rs:341`) say "an adversary step in `defaults.review`" in place of
  `defaults.review.adversary`, so the advice holds in both forms; the rendered skill text
  (`docs.rs:553`) keeps its wording.
- The canary: `canary_store::apply_model_pins` (`crates/rigger-conductor/src/canary_store.rs:168`)
  pins every agent of every step of the pinned kind; `run_canary` (:245) refuses a panel with no
  adjudicator step, runs the steps in order under their step tokens, scores the lens tier over
  every lens step and the adversary tier over every adversary step, judges each adjudicator step
  in both finding orders (`adjudicate`, :541), and approves an item only when every adjudicator step
  approves, stopping the item at its first rejecting adjudicator step, as THE ROUND'S VERDICT
  stops a round; the steps after that adjudicator are excluded from that item's tier scores. `--model <tier>=<id>` keeps its three tier names (`src/cli/eval.rs:94`).
- `cmd_validate` prints, after the gate lines, one `review <stage>: <steps>` line per stage that
  `reviews_through_panel` (`wave.rs:174`) holds for and whose own `adversary` and `adjudicator`
  fields are empty (the fallback rule `reviewers_of` reads), in stage-name order, plus a
  `review <stage> light: <steps>` line under `tiers`. `<steps>` joins `lenses(a, b)`,
  `adversary(x)`, `adjudicator(z)` with ` -> `, rendering `effective_review_panel(stage).steps()` and
  its `tiers.light`, so a stage with `review: {}` or `steps: []` prints the defaults' steps and
  `none` prints only when the effective panel is empty.
- The replay driver (`crates/rigger-driver/src/driver/replay.rs`) re-runs `review_unit` and answers
  each spawn from the log by id; it reads no role of its own, so the deterministic step tokens are
  what make it correct.
- No production code of the MCP server reads a review role (`crates/rigger-dash/src/mcpserver.rs`;
  its hits at :2264 and :2298 sit in tests, past `mod tests` at :1056): nothing changes there.

**OUT OF SCOPE, decided here.** A stage's own `adversary` and `adjudicator` fields (`config.rs:634`)
and everything reading them - the standalone fan-out review stage (`conductor.rs:6980-7070`), the
plan-critique gate (:8219-8262), `critique_gate_name` and `has_llm_verifier` (`wave.rs:186`, :305),
`plan_critique_gate_name` (`crates/rigger-config-files/src/config.rs:50`) - take no sequence and are unchanged.
The scaffolded workflow (`SCAFFOLD_WORKFLOW`, `src/cli/setup.rs`) and this repository's
`.rigger/workflow.yml:83-86` keep the map form. The built-in instruction text, `REVIEWER_DISCIPLINE`
(`conductor.rs:12529`) and the per-unit build-cache naming are not this spec's; the core-names-no-
toolchain constraint binds the lines this spec adds. The rendered handbook pages (`using-rigger.md`,
`planning-field-guide.md`) and every shipped skill carry no fixed-tier or unremovable-layer
wording and are untouched. The pages this spec edits are hand-written: `authoring-loops.md` (its
opening, the `review:` comment at :54, the lifecycle line at :100, the new sections),
`agentic-sdlc.md:62-68` (the review section, retitled as a sequence whose default is the three
steps) and `docs/handbook/README.md:27` (the glossary's review-panel line). `authoring-loops.md`'s
existing workflow example keeps the map form, so `tests/handbook_grounder_accuracy.rs`'s
`grounder:` check (:124-152) reads it unchanged; the five-step example sits in a fenced `yaml`
block under a `### The review ceremony` heading and carries no `grounder:` line.

**CONSTRAINTS WALK, decided here.**

| Mechanism | Corner | Decision |
| --- | --- | --- |
| shadowing | empty | no operator files: every built-in `BuiltIn`, output equal to HEAD's apart from the suffix and label |
| shadowing | repeated | two files cannot share a stem in one directory; `x.md` and `x .md` are two names, the second appends |
| shadowing | near name | trailing space or case change appends (no trimming) |
| shadowing | reverted | a file emptied then refilled: each load reads the body as it is; definition drift between |
| shadowing | DROPPED | a shadowing file deleted between spawns: drift halts the next step; rebased, the built-in returns |
| shadowing | concurrent | two units compose from the one `Config` slice; resolution is pure |
| shadowing | crash-resume, cold start | nothing cached; the next process loads the files and resolves again |
| shadowing | existing data | an older README stays; render and validate report the override |
| sequence | empty | `steps: []` is an empty panel (approve at once; a stage inherits `defaults.review`) |
| sequence | one-agent lens step | a concurrent run of one; token `lens:a` or `lens:a@k` |
| sequence | repeated | one agent in two adversary steps: tokens `adversary`, `adversary@2`, two sessions; one agent twice in one lens step: refused |
| sequence | repeated round after a mid-sequence reject | a step with no recorded spawn in the prior round reviews the whole unit with no round block; a step that ran gets the delta |
| sequence | a gate-failed or infra-rerun attempt between two rounds | the round that sent the unit back is the attempt `round_delta_base` read, never `attempt - 1`; a step's prior spawn is looked up at that attempt |
| sequence | two adjudicators | first rejecting ends the round; both approving: the last stamps `reviewed` |
| sequence | concurrent | lens agents within a step only; never two steps at once |
| sequence | crash-resume | tokens recomputed from `steps()`; every recorded step answers from the log |
| sequence | rebased map to list | definition drift halts; rebased, a list spelling the canonical sequence mints the same ids and replays; an added step parks fresh |
| sequence | DROPPED step on rebase | its recorded spawns are never asked for again; its findings stay in the graph as facts about files |
| sequence | existing store | old ids carry bare tokens, which parse as step 1 of their kind; metrics fold them as today |
| sequence | `tiers.light` in list form | routed exactly as today; must have an adjudicator step |

## Notes (non-criteria)

UNIT ORDER (the needs chain): 2, 3, 4, 5, then 6, 7, 8, 9, 10, then 1, then 11.
Criteria 2-5 (instructions) and 6-10 (review) are independent chains; 1 needs 3, 7 and 8; 11
needs all.

LANDING-ORDER SIMULATION (every ordered pair sharing a surface; "no" lines were fixed in the text):

- 2 then 3 (surface `compose`): 2 without 3 asserts only the replace arm and a near name -> yes. 3
  without 2 would need `resolve_layers` -> 3 lands after 2 by order; 3's text names only the off arm.
- 4 without 3 (surface `render`): 4 asserts the switched-off label, which needs 3 -> fixed by order
  (4 after 3); 4 owns no `compose` assertion.
- 5 without 3: the README describes the off arm -> fixed by order; 5's test reads text only.
- 6 without 7 (surface `steps()`, `review_unit`): 6 asserts the canonical sequence only -> yes.
- 6 after 2 (surface the spawns' prompts): 2 removes the `HEADING` suffix from every system prompt
  before 6 lands, so an identity over system prompts would fail 6's own text -> fixed by the
  CANONICAL IDENTITY SURFACE block: 6 compares task prompts against the tree before it, never HEAD.
- 7 without 8 (surface `config::load`): 7 asserts parse, refusal and the accessors' dedup only,
  never a run, and refuses a list naming two steps of one kind (one spawn id on that tree) or an
  `Adjudicator` step that is not last (no early stop on that tree) -> yes. 8 lifts both refusals.
- 8 after 7 (surface the round): 8 lifts both refusals, and its fixtures run under its own tokens,
  rosters and verdict rule in one landing, so no interim tree lies between them -> yes.
- 9 without 8 (surface the token grammar): 9's fixtures carry `@k` tokens 8 mints and parses ->
  9 after 8; 8 owns `is_adjudicator`, so the console needs no clause in 9.
- 10 without 7: 10 parses list-form fixtures -> 10 after 7.
- 1 without 7: the five-step example must parse -> 1 after 7; 1 without 2/3 would state S2 about an
  override that does not exist -> 1 after 3.
- 6 and 7 both name `config.rs`'s `ReviewPanel`; 6 lands the type change, 7 the field. A shared
  blast radius serializes them; ownership is split by the OWNS sentences.

PER-CRITERION CORNERS (beyond the CONSTRAINTS WALK table, which covers 2, 3, 6, 7 and 8):

- 1: the example block DROPPED from the page fails the parse clause; a rewrap never moves S1 or S2
  (read by words). Concurrent, crash-resume, cold start: out of scope - static text.
- 4: an empty operator layer renders `(none)`; an all-shadowing layer renders `(none)` too; no
  shadow prints no advisory; repeated `validate` prints the same lines.
- 5: a new `BUILTIN` entry fails the test until the README names it; an older project README is
  existing data, left as written.
- 8 (rosters): a sequence of one step: an empty roster; a lens agent in two lens steps appears twice, once per
  token; a DROPPED adversary (lenses plus adjudicator) leaves `adversary` out of the roster, as today.
- 9: a token with `@1` or `@x` is no review token (falls to the reader's non-review arm); a store
  mixing bare and `@k` tokens folds each by its kind.
- 10: a panel with no adversary step has no critic (as today); a stage reviewed through no panel
  prints no `review` line; `tiers` absent prints no `light` line.
- 11: out of scope for every corner - it runs the lanes once on the finished tree.

THREE RISKIEST CONTRADICTIONS, and how the text resolves them:

1. Byte-identical map form versus distinct ids for repeated kinds. A step token keyed on the global
   step ordinal would rename every spawn of every existing panel and orphan every recorded result.
   Resolved by STEP ROLE TOKENS: the ordinal counts within a kind and the first step of each kind
   keeps today's bare token, so the canonical sequence mints today's ids and an existing store's
   tokens parse as step 1.
2. New refusals versus "every existing workflow unchanged". Refusing a lens named twice in one step
   or an `@` in an id applies to the map form too, which could reject a workflow HEAD loads.
   Resolved by criterion 7 owning a load-unchanged check over this repository's
   `.rigger/workflow.yml` and every scaffold gate template set's rendered workflow, none of which
   carries either, and by recording why no working workflow can carry a duplicate (its two spawns
   share one id).
3. "Each step's prompt carries earlier agents" versus byte-identical prompts. Adding the roster to
   prompt text would change every map-form prompt. Resolved by ROSTERS: the carrier is the existing
   `SpawnOpts.reviews` roster, whose canonical value equals today's `review_roster` and
   `adjudicator_roster`; no prompt text changes.

TEST PLACEMENT (criterion -> test homes; the fixtures each test uses):

| Criterion | Test homes and fixtures |
| --- | --- |
| 1 | `tests/handbook_grounder_accuracy.rs` over the committed pages: S1 and S2 by words; the fenced `yaml` block under `### The review ceremony` parsed; `docs/handbook/agentic-sdlc.md`'s review section naming the sequence and the three kinds |
| 2 | `crates/rigger-domain/src/instructions.rs` tests through `resolve_layers`: one named built-in replaced; a trailing-space and a changed-case name appended |
| 3 | `instructions.rs` tests: one named built-in off, with a second named built-in off as a fixed two-file fixture (never iterating `BUILTIN`), leaving `HEADING` and the operator layer alone |
| 4 | `instructions.rs` tests for `render` and `shadow_advisories`; `src/cli/setup.rs` tests for `instructions_in_force_line`; a periphery test running the built binary's `instructions`, `validate` and `prime` over a temp project with one replacing and one empty file |
| 5 | `src/cli/setup.rs` tests iterating `BUILTIN` |
| 6 | `crates/rigger-domain/src/config.rs` tests for `steps` and the accessors' canonical shape; `conductor.rs` tests over a recording driver and the golden fixtures |
| 7 | `crates/rigger-config-files/src/config_store.rs` tests (every refusal, the interim repeated-kind and adjudicator-not-last ones included; the repo workflow and every scaffold template set's rendering); `crates/rigger-domain/src/config.rs` tests of the dedup and `workflowdef.rs` and `crates/rigger-config-files/src/config.rs` tests of `reviewers_of`, the lint and the wall-clock advisory, each over a map form naming one agent as lens and adjudicator, a list form naming one agent in two steps and a `tiers` panel (named once, full panel first); a periphery test loading a list-form `.rigger/workflow.yml` |
| 8 | `spawn.rs` tests (bare tokens parse as step 1, a `~retry2` suffix ignored); `conductor.rs` tests over `lenses [a, b]`, `adversary x`, `lenses [c]`, `adversary x`, `adjudicator z` and over `adjudicator a`, `lenses [l]`, `adjudicator b` with `a` rejecting in round 1; `replay.rs` tests resuming mid-round from the log; `review.rs` tests for `step_roster` (the five-step sequence: the second adversary's roster `lens:a`, `lens:b`, `adversary`, `lens:c@2`, the adjudicator's adding `adversary@2`); `conductor.rs` tests over a recording driver; `conductor.rs` tests over `adjudicator a`, `lenses [l]`, `adjudicator b` under each verdict pair and over `lenses [l]`, `adversary x`; the speculation path's tests for `reviewed#{lane}` |
| 9 | `metrics.rs` and `dash.rs` tests (a bare-token store folding to the prior `ReviewQuality`); `tests/phase_of_role_mapping_periphery.rs`; `tests/review_tier_roster_periphery.rs` |
| 10 | `wave.rs`, `canary_store.rs` and `src/cli/validate.rs` tests |
| 11 | the lanes script on the integrated tree |

ARTIFACT OWNERSHIP (owner = the criterion of the first production caller):

| Artifact | Owner |
| --- | --- |
| `resolve_layers` and its `Replaced` state, `HEADING` printed bare, the module-doc sentence | 2 |
| the `SwitchedOff` state | 3 |
| `render`'s labels, `shadow_advisories`, its `cmd_validate` call, `instructions_in_force_line` reading `resolve_layers` | 4 |
| `SCAFFOLD_INSTRUCTIONS_README`, `.rigger/instructions/README.md` | 5 |
| `ReviewStep`, `ReviewPanel::steps`, the derived `is_empty`/`agent_ids`/`full_roster`/`gating_agent_ids` in today's shape over the canonical sequence, the one step loop in `review_unit` | 6 |
| `ReviewStepDecl`, `ReviewPanel.steps`, every refusal, the interim repeated-kind and adjudicator-not-last refusals, the new `validate_depth` rule, the load-unchanged check over the repo's workflow and every scaffold template set, the dedup of `agent_ids`/`full_roster`/`gating_agent_ids` (first-step order, full panel then light) and the naming-once behaviour of `reviewers_of`, `lint_gating_verdict_lines` and `unbounded_wall_clock_advisory` through them | 7 |
| `review_step_role`, `review_step_of`, `is_adjudicator` through them; per-step token minting, the early stop on a rejecting adjudicator and the fresh-step rule (`round = None`, the attempt `round_delta_base` read) inside the one loop criterion 6 owns; `step_roster` and the `review_roster`/`adjudicator_roster` wrappers; the verdict read after `split_reject` (a converged reject continues), the REQUIRED list, the round's reason and compensation choice and the `reviewed` stamps across several adjudicator steps; lifting criterion 7's two interim refusals | 8 |
| metrics, dash, `workflows/rigger.js` classification | 9 |
| `wave::critic` with `NO_CRITIC_CLAUSE` and the `rigger critique` help wording, the canary, `cmd_validate`'s `review` lines | 10 |
| the handbook text | 1 |

## Global constraints

- Hyphens, never em or en dashes, ASCII only, in every added line.
- A line citation names the symbol at the tree it was read on (1185c8c6, on `rigger-run`); the
  symbol name governs, the line being a pointer only.
- No new event type; no new dependency.
- Every lane `.rigger/gates/lanes.sh` runs green (fmt, clippy -D warnings, test), and the default
  lane's test suite.
- The planner's batch order is UNIT ORDER's sequence, never checkbox position.
- `.rigger/workflow.yml`, every scaffold gate template set's rendered workflow and every
  `ReviewPanel` fixture in the tree load unchanged and behave as at HEAD.
- No line this spec adds to the core names a language or a toolchain.
- Every operator-facing behaviour (the `instructions` and `validate` output, the `steps:` key, the
  thin driver's labels) is pinned by a periphery test as well as a unit test.

## Done when

- [ ] a test proves THE HANDBOOK STATES THE RULE: `docs/handbook/authoring-loops.md` carries S1 and S2 by their words and a `### The review ceremony` section whose five-step `steps:` example parses through `config_store::parse_workflow` to five `steps()`. This criterion OWNS the handbook text only; the README is criterion 5's and the parse criterion 7's, NOT this one's.
- [ ] a test proves A SAME-NAMED OPERATOR FILE REPLACES A BUILT-IN: `instructions::compose` puts the body of an operator file whose name equals a `BUILTIN` name byte for byte into that built-in's `## <name>` section (the composed string's section bytes), prints `HEADING` bare, and appends a near name. This criterion OWNS `resolve_layers`' replace arm and `compose`; switching off is criterion 3's and `render` criterion 4's, NOT this one's.
- [ ] a test proves AN EMPTY SAME-NAMED OPERATOR FILE SWITCHES A BUILT-IN OFF: `instructions::compose` given an operator file named as a `BUILTIN` entry with an empty or whitespace body injects no section for either, every other section unchanged. This criterion OWNS `resolve_layers`' switched-off arm; replacing is criterion 2's, NOT this one's.
- [ ] a test proves EVERY BUILT-IN'S STATE IS REPORTED: `instructions::render`, `shadow_advisories` (printed by `cmd_validate` with a success exit) and `instructions_in_force_line` name each built-in of a named-entry fixture as in force, replaced or switched off through `resolve_layers`. This criterion OWNS `render`, `shadow_advisories`, `instructions_in_force_line` and their call sites; `compose` is criteria 2 and 3's, NOT this one's.
- [ ] a test proves THE SCAFFOLDED README DOCUMENTS THE OVERRIDE: `SCAFFOLD_INSTRUCTIONS_README` names every `BUILTIN` entry as `<name>.md` with the replace, switch-off and append rules, and the committed `.rigger/instructions/README.md` equals it (whitespace-normalized text). This criterion OWNS the two README texts and the `BUILTIN`-iterating assertion; the mechanism is criteria 2 and 3's, NOT this one's.
- [ ] a test proves THE MAP FORM IS THE CANONICAL SEQUENCE: `ReviewPanel::steps` returns lenses, adversary, adjudicator for the map form, and `review_unit` over `steps()` gives each CANONICAL IDENTITY SURFACE fixture the list the tree before it gave. This criterion OWNS `ReviewStep`, `ReviewPanel::steps`, its accessors in today's shape and the one step loop; `steps:` and the dedup are criterion 7's, `@k` tokens 8's, NOT this one's.
- [ ] a test proves THE LIST FORM DECLARES ANY SEQUENCE: `Config::validate` accepts `review: { steps: [..] }` under every scope, refuses each PARSE AND REFUSALS panel naming its scope, and loads the repo's and every scaffold template set's workflow unchanged. This criterion OWNS `ReviewStepDecl`, `steps`, every refusal, that load check and the accessors' dedup (each id once) with its readers; running a sequence is criterion 8's, NOT this one's.
- [ ] a test proves A LIST-FORM SEQUENCE RUNS: each step spawns under its `review_step_role` token, parsed back by `review_step_of`, carrying in `SpawnOpts.reviews` every earlier step's tokens; a step absent from the prior round gets `round = None`; each adjudicator verdict is read after `split_reject`, a kept reject ending the round with its REQUIRED list, else one `reviewed` stamp from the last adjudicator. This criterion OWNS the token, roster, early-stop and verdict rules inside criterion 6's loop and lifts criterion 7's interim refusals; readers are criteria 9 and 10's, NOT this one's.
- [ ] a test proves TOKEN READERS CLASSIFY EVERY STEP BY ITS KIND: the readers READERS assigns to criterion 9 (`metrics::review_tier`, `actor_tier`, the adversary-only fold, `dash::stage_of_role`, `role_and_agent`, the `workflows/rigger.js` lookups) read an `@k` token as its kind, and bare tokens fold as today. This criterion OWNS those readers; the token grammar is criterion 8's, NOT this one's.
- [ ] a test proves PANEL READERS READ EVERY STEP: `wave::critic` (with `NO_CRITIC_CLAUSE` and the critique help naming an adversary step in `defaults.review`), the canary (`apply_model_pins`, `run_canary`) and `cmd_validate`'s `review <stage>:` lines read every step as READERS decides. This criterion OWNS those readers' list-form clauses; `steps()` and its accessors are criteria 6 and 7's, NOT this one's.
- [ ] `.rigger/gates/lanes.sh` runs green for both feature lanes on the finished tree, verified by `sh .rigger/gates/lanes.sh no-default` and `sh .rigger/gates/lanes.sh core` each exiting 0. This criterion OWNS only the lanes; every behaviour is criteria 1 to 10's, NOT this one's.
