# 135 - a spawn runs at the capability its task needs

## Goal

Every persona pins one model. Every session of that role therefore runs at the same capability
whatever the unit asks of it. A mechanical rename and an invariant shared by six modules get
the same model, and no session states an effort at all.

- `AgentDef` carries `model`, `model_ladder` and `fallback_model`, and no effort
  (`crates/rigger-domain/src/config.rs:170`).
- `AgentDef::model_for_attempt` (`config.rs:1103`) is the one selection authority: attempt `n`
  takes ladder rung `n`, clamped, and a ladder-less persona takes `model`.
- The drivers call it themselves, so the launch flags are picked inside the driver:
  - `build_args` at `crates/rigger-driver/src/driver/claude_code.rs:2204` (`--model` at :2215,
    `--fallback-model` at :2216-2218);
  - `launch_as` at :331, which sets `Launch.model` at :410;
  - `build_args` at `crates/rigger-driver/src/driver/cli.rs:273` (:283);
  - `spawn_request` at `crates/rigger-domain/src/agent.rs:294` (:301), the parked request.
- The conductor runs the same resolution a second time to stamp `META_MODEL_ALIAS`
  (`agent_model`, `crates/rigger-conductor/src/conductor.rs:4814`).
- No launch passes `--effort`, so every session runs at the host's default effort.
- Model names live in persona files: all nine personas under `.rigger/agents/` declare the same
  `model:`. `SCAFFOLD_AGENTS` (`src/cli/setup.rs:2403`) seeds seven personas with that
  `model:`. `scaffold/rust/set.yml:20` and the two helpers under `.claude/agents/`
  (`lookup.md:4`, `verify.md:4`) name models too.
- A new model generation is therefore an edit to every persona of every project.
- The review comment at `conductor.rs:6120` states a fixed rule: "a reviewer keeps a FIXED
  tier - judgment is not laddered".

This spec moves those to:

- a persona states the kind of work it does;
- the planner judges each unit's complexity and says why;
- one table in the workflow maps kind and complexity to the host's model and effort;
- the conductor resolves every spawn from that table, once;
- a charged failure raises the unit one step;
- each spawn's record says what it ran at and why.

## Design

**EXISTING MECHANISMS JOINED, decided here so no unit adds a parallel one.**

| Existing mechanism | Where | What this spec does with it |
|---|---|---|
| Per-attempt model cascade (`model_ladder`, `model_for_attempt`) | `config.rs:1103` | Becomes the PIN branch of the one resolution (`capability::resolve`); its per-attempt behaviour is unchanged for a persona that declares a ladder. |
| Charged failure records (`UnitFailed`) and the bound (`charge_failed_attempt`, `safety::remediate`) | `conductor.rs:8279`, `crates/rigger-domain/src/safety.rs:24` | The raise is the count of a unit's `UnitFailed` records (RESOLUTION). No new counter and no new limit; the existing bound still ends the unit. |
| Model stamp on unit events (`META_MODEL_ALIAS`, `META_MODEL_RESOLVED`) | `metrics.rs:147`, stamped through `agent_model` `conductor.rs:4814` | Keeps its key and meaning (the requested model). Two sibling meta keys join it (THE RECORD). |
| Session continuation (`STATUS_SPAWN_SESSION`, `record_session`, `recorded_session`, `continue_prior_session`) | `conductor.rs:333`, `:6177`, `:17048`, `:6152` | The mark gains the capability stamps, and the continuation compares against them (RESOLUTION). |
| Parked spawn request (`spawn_request`, `spawn_store::park_in_run`) | `agent.rs:294`; parked by `Driver::before_launch` `claude_code.rs:475` through `replay.rs::read_or_park` | A record of every spawn the headless host launches, read by the dash and `metrics::project` (`metrics.rs:911-924`). It gains the effort and the why as record fields. Nothing launches from it. |
| Model drift monitor (`metrics::model_drift`) | `metrics.rs:1432` | Unchanged. It keys on the requested alias, so a model that varies per unit is never read as drift. |
| Canary model pins (`apply_model_pins`) | `canary_store.rs:177` | Unchanged. It writes `model` and clears `model_ladder`, which is a model pin under PINS below. The pin reaches the launch through `canary_opts`' `Capability`, so the echo driver in `tests/model_pinning_periphery.rs` reports `opts.capability.model` and its module doc names that capability as the launch's source. |
| Canary header (`CanaryHeader`) | `crates/rigger-domain/src/canary.rs:178` | Gains each panel persona's capability, the canary's record (THE RECORD). |
| Run hold (`record_hold_sighting`, `HoldProbe::probe`) | `crates/rigger-store-sqlite/src/run_store.rs:297`, `claude_code.rs:1824` | Unchanged and run-wide. A hold probes the sighting's model (`owed.held.model`), so a cap on one cell's model holds every spawn of the run until that model's probe passes. |
| Definition pin (`definition_hash`) | `crates/rigger-config-files/src/config_store.rs:165-215` | The table lives in `.rigger/workflow.yml`, which the hash already folds. A table edit from outside the run is a definition change: drift halt, then `--rebase-definition`. A definition change the run lands itself re-pins (`checkout_definition`, `conductor.rs:4557`). Nothing new. |
| Planner proposal (`UnitProposed`, `harvest_proposed`, `PLAN_UNIT_PROPOSED`, `DAG_UNIT_ROW`) | `conductor.rs:3495`, `:14265`; `crates/rigger-conductor/src/prompt_surface.rs:52-55`, `:428` | Carries the complexity and its reason, and shows both to the plan-critique panel. No new event type. |
| Built-in instruction layer (`instructions::BUILTIN`, `resolve_layers`) | `crates/rigger-domain/src/instructions.rs:45`, `:115` | Holds the complexity considerations as a fourth entry, replaceable or switched off by `.rigger/instructions/complexity.md`. |
| Helper subagents (`HELPER_AGENTS`, `helper_agents_json`) | `crates/rigger-driver/src/hooks.rs:63`, `:373` | Takes its model and effort from the table's reading row. |
| Workflow parse refusing unknown keys (`parse_yaml_naming_unknown_keys`) | `config.rs:87` | Refuses a mistyped table key with no new check. |
| Risk-tiered review depth (`ReviewPanel::tiers`) | `config.rs:475` | Not joined. It picks which reviewers run, not what capability they run at; the two compose. |

**THE WORK KEY, decided here.**

- A persona's frontmatter may declare `work:` with one of three values:
  - `judging` - the persona decides whether work is right: planner, reviewer, adversary,
    adjudicator, evaluator;
  - `writing` - the persona changes the tree: implementer, test author, scribe;
  - `reading` - the persona gathers and reports without deciding.
- The value is parsed into `AgentDef::work` as the enum `Work`. Any other value fails the load
  and names the persona and the three values.
- That refusal holds for existing files on purpose. `AgentDef` does not deny unknown keys
  (`config.rs:170`), so a persona or an imported collection that used `work:` with a meaning of
  its own loaded until now and is refused from this spec on; the refusal names the persona and
  the three values as the fix.
- `work` is optional. A persona without it resolves through its pins only (PINS below).

**THE COMPLEXITY JUDGMENT, decided here.**

- A unit's complexity is one of `routine`, `involved` or `hard`. The domain enum `Complexity`
  is ordered by those steps, and `involved` is the middle step.
- The planner assigns it when it decomposes the spec. It is a judgment, never a measure. The
  planner weighs five considerations:
  - how much design or judgment the unit needs, from a specified mechanical edit up to a design
    choice with real alternatives;
  - how cross-cutting it is, from one concern in one place up to an invariant several modules
    share;
  - how ambiguous the criterion leaves the answer;
  - how novel it is against what the codebase already does;
  - what a wrong answer costs.
- The planner writes one sentence, `complexity_reason`, naming the consideration that decided
  the step. It may cite the graph as evidence: what the change touches and who depends on it.
  No line count, file count, node count or size estimate decides a step. Resolution reads only
  the step, never the reason or the graph.
- The considerations live in the built-in layer as the entry `complexity`
  (`crates/rigger-domain/src/instructions/complexity.md`). The entry names no language. Like
  every built-in, `.rigger/instructions/complexity.md` replaces it or, when blank, switches it
  off.
- The entry reaches every spawn, as every built-in does: the five considerations are about ten
  lines, and a reviewer who meets a unit's step and reason needs the same definitions the
  planner used.
- Whether a reason is sound is judged by the plan-critique panel like any other plan content:
  its DAG rows carry each unit's step and reason. The binary stores the reason verbatim and
  never parses it.
- What the planner is asked for:
  - `PLAN_UNIT_PROPOSED` (CONTRACT) asks for one `UnitProposed` per criterion, one the planner
    keeps unchanged included, each under the planner's own id and echoing its `criterion_id`,
    with `"complexity":"routine|involved|hard"` and `"complexity_reason":"<one sentence>"`.
    That proposal takes `harvest_proposed`'s add path, which supersedes the criterion's
    baseline by the echoed id (`PLAN_SPLIT_ID_ECHO`, `PLAN_PAIRING`).
  - The contract says never to reuse a baseline's id: a baseline is never in `proposed`, so
    `harvest_proposed` skips a proposal naming one.
  - `PLAN_ROLE` (FRAMING, `prompt_surface.rs:21-23`) loses "Your job is to REFINE that
    baseline, not to re-decompose it" for: keep the baseline's decomposition, refine it where
    it must change, and propose every unit with its step and reason.
  - The contract names the three steps and asks for the reason by itself, so it reads whole
    whether or not the `complexity` built-in is in force.
  - The planner texts that call every proposal a refinement change with it, so the planner
    gets one instruction:
    - the policy sentence "Propose each refinement the moment you decide it." in
      `.rigger/agents/planner.md:17` and in the `SCAFFOLD_AGENTS` planner seed
      (`src/cli/setup.rs:2427-2428`) becomes: propose every unit, one kept unchanged included,
      with its step and reason, the moment you decide it. "Only omit the id when you truly
      intend a new sub-unit." stays;
    - `PLANNING_POLICY[3]` (`tests/common/fixtures/plan_critique.rs:26`) takes the new
      sentence. Its four readers then count it:
      `the_planner_persona_and_its_seed_state_the_planning_policy` (`src/cli/setup.rs:6318`,
      once in each), `the_planner_protocol_carries_no_planning_policy` (`conductor.rs:59054`,
      never in the plan protocol), `planning_counts`
      (`tests/critique_method_persona_periphery.rs:234`) and
      `the_handbook_upgrade_note_quotes_each_moved_sentence_once` (`src/cli/setup.rs:7088`,
      quoted once in the upgrade note). So the sentence is worded apart from the contract's own
      text;
    - the upgrade note's planner bullet (`docs/handbook/authoring-loops.md:123`) quotes the
      new sentence in place of "Propose each refinement the moment you decide it." and tells a
      project whose planner persona holds the old sentence to replace it with the new one;
    - `plan_protocol_text` (`tests/common/fixtures/plan_critique.rs:39`), the copy of the plan
      protocol that `the_planner_protocol_carries_no_planning_policy` and the periphery's
      served-task test (`tests/critique_method_persona_periphery.rs:256`) compare a planner's
      task to, restates `PLAN_ROLE`, `PLAN_UNIT_PROPOSED`, `PLAN_UNMATCHED` and
      `PLAN_CRITERIA_LEAD` as they become;
    - `PLAN_UNMATCHED` (FRAMING, `prompt_surface.rs:59-62`) names a unit kept unchanged beside
      "a refinement, a split, or a new sub-unit" among the proposals that run the fan-out
      template's gates;
    - `PLAN_CRITERIA_LEAD` (CONTRACT, `prompt_surface.rs:66-68`) opens with "The acceptance
      criteria to propose against" in place of "The acceptance criteria to refine against";
    - the `render_plan_protocol` doc (`conductor.rs:14967-14971`) says the planner proposes
      every unit with its step and reason, refining the baseline where it must change, in place
      of "which the planner REFINES; each refinement is a `UnitProposed`";
    - `REPLAN_LEAD` (`prompt_surface.rs:512-514`) asks to re-emit every unit with its step and
      reason, not "your UnitProposed refinements";
    - the `SCAFFOLD_WORKFLOW` comment (`src/cli/setup.rs:2069-2070`) says the planner proposes
      every baseline unit with its step and reason, refining it where it must change, and its
      `produces: dag` comment (`:2073`) says the planner proposes the spec's unit DAG at
      runtime. This repository's `.rigger/workflow.yml` takes both in the hand-off's
      definition change (Hand-off step 2).
- Where it is carried:
  - the conductor's private `UnitProposed` carries `complexity` and `complexity_reason` as
    `Option<serde_json::Value>` with `serde(default)`. Neither field can fail the proposal's
    parse, which `harvest_proposed` skips whole on an error (`conductor.rs:14318-14321`,
    `:14340-14343`). A `null` reads as absent;
  - `harvest_proposed` takes a VALID PAIR: a step that is one of the three words as a JSON
    string, and a reason that is a non-blank JSON string;
  - it copies the pair onto `Stage::complexity` and `Stage::complexity_reason`
    (`crates/rigger-domain/src/config.rs:975`), both `#[serde(skip)]`, so a workflow file
    cannot declare them;
  - `build_dag_critique_prompt` (`conductor.rs:15019`) renders each unit's step and reason
    through `DAG_UNIT_ROW`, and `unassessed` when the stage has none.
- What the stage gets:
  - **The pair freezes at the unit's first `UnitStarted`.** For a unit in `proposed`, the
    pair is the last valid pair among that id's `UnitProposed` events that precede the
    unit's first `UnitStarted` in the run slice `harvest_proposed` reads, or all of them when
    it has not started. This fold runs apart from the refine guard (`conductor.rs:14418-14422`),
    whose `integrated` and `terminal` sets a resume seeds from the log before it harvests. So
    the live harvest and a resume over the same log give the unit the same pair.
  - **Add path:** a valid pair is taken.
  - **Refine path** (a same-id re-proposal before the unit starts): a valid pair replaces the
    stage's pair, and an absent or refused pair keeps the earlier one, the way `episode` is
    kept.
  - **After the first start** a proposal's pair is not read. Every spawn and relaunch of a
    started unit reads the one frozen step, and only the raise moves it.
  - **Absent pair** (neither field present and non-null): a baseline no planner proposal
    superseded, a proposal with neither field, and every `UnitProposed` the log held before
    this spec leave the stage UNASSESSED and record nothing. Unassessed resolves at
    `involved`, and a spawn of it with `from_table` true stamps `unassessed` (THE RECORD).
  - **Refused pair** (at least one field present and non-null, but not a valid pair: an
    unknown word, a non-string, a step with no reason or a reason with no step): on the add
    path the stage is unassessed, and on a refine it keeps its earlier pair. Either way, for
    a proposal the freeze reads, `harvest_proposed` records one `DecisionMade` with id
    `unassessed-complexity:<unit>#<position>` naming what was wrong, `<position>` being the
    proposing event's log position, so two refused proposals of one unit are two decisions. It
    is emitted through `emit_keyed` on `{unit}/unassessed-complexity#{position}`, as the
    refine-retarget `ScopeCreep` is keyed (`{unit}/refine-retarget#{position}`,
    `conductor.rs:14434-14441`). So a later harvest or a resume over the same log records it
    once. A step with no reason is not a judgment a panel can check, so it is not taken.
- A stage no planner proposes is unassessed too: the plan stage itself, a declared stage, a
  conflict-resolution respawn of an unproposed stage.
- On resume, `harvest_proposed` refolds `UnitProposed` from the log under the same freeze, so
  a resumed unit carries the same step it had.

**THE TABLE, decided here.**

- `.rigger/workflow.yml` `defaults.capability` maps work, then step, to one cell:

      defaults:
        capability:
          judging:  {routine: {model: M, effort: E}, involved: {...}, hard: {...}}
          writing:  {routine: ..., involved: ..., hard: ...}
          reading:  {routine: ..., involved: ..., hard: ...}

- It parses into `CapabilityTable`, which has exactly three rows and each row exactly three
  cells. Its `Deserialize` refuses shape only: a missing row, a missing cell, an unknown key or
  an empty `model` fails the parse and names its path.
- `Config::validate` (`config_store.rs:406`) calls `CapabilityTable::validate` (THE FLOOR).
  The `Deserialize` does not, so `read_scratch_defaults` (`config_store.rs:392`), which parses
  `Defaults` with no passing `Config::validate`, fails only on a malformed table.
- A cell's `model` is a non-empty opaque host string. Its `effort` is an opaque host string;
  an absent or empty `effort` means no effort flag, so the session runs at the host default.
- The binary never interprets either string. It knows no model ordering and no effort
  vocabulary. A cell the host does not accept fails that launch the way an unknown `model:`
  does today, through the same launch path. This spec adds no check of host vocabularies.
- `--effort` needs no host-version check of its own. `rigger run` refuses a session host below
  `MIN_HOST_VERSION` (`crates/rigger-driver/src/hooks.rs:301`, through
  `claude_version::too_old`) and `rigger validate` warns on one. SCAFFOLD raises that minimum
  to 2.1.170, and the host's changelog lists `--effort` by 2.1.72 and the `xhigh` effort by
  2.1.111, both below it. `rigger critique` and `rigger canary` check no version, as today.
- The table is optional. With no `capability` key, the workflow resolves exactly as it does
  today (A CONSUMER THAT CHANGES NOTHING).
- It is the one place in a project's configuration where a model name is required. A pin is a
  project's own choice (PINS).
- The binary carries model names only in scaffold text:
  - `SCAFFOLD_WORKFLOW`, which writes the default host's table;
  - test fixtures.
- A new model generation is an edit to this one table.

**THE FLOOR, decided here as a validation rule on the table.**

- `CapabilityTable::validate` refuses a table whose `judging.routine` cell names a different
  `model` than its `judging.involved` cell. The two cells' `effort` may differ.
- So a judging spawn planned `routine` runs on the same model as one planned `involved`. The
  `judging.hard` cell is the table author's choice.
- The rule compares two `model` strings for equality, so it needs no model ordering and names
  no model. Effort is left free because the binary knows no effort vocabulary to order.

**RESOLUTION, decided here: one domain function, run once per spawn by the conductor.**

- `capability::resolve(agent, table, complexity, raise, attempt) -> Capability` lives in
  `crates/rigger-domain/src/capability.rs`. It returns
  `Capability { model, effort, helper_model, helper_effort, why, from_table }`. `from_table`
  is true exactly when `resolve` takes the model or the effort from a table cell, whether or
  not the value taken is empty.
- **The step** is the planned step (`involved` when unassessed) raised by `raise` steps and
  capped at `hard`.
- **`raise`** is `charged_failures(events, unit) -> u32`, a new pure function over the run
  slice (`read_current_run`): the number of the unit's `UnitFailed` records. It is one count
  for every unit, whatever its speculation width, over the slice a speculation group anchors
  (below), and never reads a spawn's attempt number.
  - Every `UnitFailed` writer is a charged cause, one record each:
    - a charge (`charge_failed_attempt`): a reviewer reject, a red gate, an integrate
      conflict, a refused git operation;
    - a fan-out review reject (`run_fan_out_review_loop`);
    - a compensation re-entry (`drain_compensations`, carrying `META_COMPENSATED`);
    - a lost speculation group, one record per group whatever its width
      (`{unit}/speculation-lost#{base}` in `run_speculation`);
    - a winning group's broken earlier lane merge (`record_lane_breaks`), an integrate
      conflict like any other. It is written through `emit_keyed_meta` on
      `{unit}/speculation-break#{entry}#{attempt}`, `{entry}` being the log position of the
      re-entry boundary `speculation_lane_base` finds (`none` when there is none) and
      `{attempt}` the broken lane's attempt. A crash-resume of the same group adds no boundary,
      so it writes no second record, and a later winning group of the unit records its own.
      - The boundary is in the key because the attempt alone repeats under the cli driver.
        After a winner, only a compensation (`drain_compensations`) or an operator's grant
        (`UnitResumed`) re-enters `run_speculation`, since `speculates` asks for a `Fresh`
        resume phase, and each adds a boundary. Past it the headless host numbers the lanes
        past every recorded attempt, but the cli driver, which records no spawn request,
        numbers them from 0 again.
      - The boundary search moves out of `speculation_lane_base` (`conductor.rs:1955`) into
        one function the base and the key both read;
    - a plan-critique reject, recorded on the gate unit (`plan_critique_loop`).
  - An infrastructure rerun is a `UnitStatus` mark (`InfraRetries`), a lane's reject is a
    fold-neutral `UnitStatus` (`record_speculation_reject`), and a group whose every lane
    crashed or lost to infrastructure writes no record. None of them raises.
  - So the lane index never raises: a lane is a parallel first attempt, not a failure. No
    site passes `base + lane`, `effective_attempts` or another local counter as the raise.
  - A speculation group's raise is `charged_failures` over the run slice before the group's
    first lane `SpawnRequested` (an id `spawn_id(unit, implementer, a)`, `a` from `base` to
    `base + width - 1`), and over the whole slice when the log holds none. Every spawn of the
    group resolves at that raise: the lane implementers, their sdet-authors and reviewers, and
    their adjudicators.
    - Live, the group reads it when it opens: no lane is requested yet, and no `UnitFailed` is
      written between the open and the first lane request.
    - A crash after `record_lane_breaks` and before the winner's `green` write
      (`emit_speculation_winner_status`, `conductor.rs:9091`) leaves the unit below `green`, so
      the resume re-enters `run_speculation` at the same `base`
      (`speculation_lane_base` adds no boundary for a non-compensating `UnitFailed`). Each lane
      keeps its spawn id, and `read_or_park` answers it from the log. The earlier lane's
      post-merge re-gate replays its recorded red (`recorded_verdict` acts on a recorded red
      whatever the tree), so the lane rejoins `LostLanes::breaks` and `record_lane_breaks` runs
      again; the keyed write records it once. The anchor leaves the lane-break records out, so
      the resumed group resolves and stamps the raise its first open read.
    - A crash after that write folds the unit to `green` or later, so `resume_phase` returns
      Implemented or Reviewed, `speculates` is false (`:8483`) and the resume takes the
      single-lane path. Its spawns come after the group, so they read the unit's
      `charged_failures`, the lane-break records counted, as the single-lane rows in Notes do.
    - The anchor is the first lane request, not lane 0's, since a lane Phase A refuses
      (`assert_isolated_cwd`) parks no request.
    - Under the cli driver, which records no spawn request (`conductor.rs:4196`), the group
      reads the whole slice, so a group re-entered after its lane-break records resolves one
      step higher per record, capped at `hard`. Its lanes relaunch rather than answer from the
      log, so each runs at, and stamps, that raise.
  - `re_plan` and the plan-critique critics read the plan-critique gate unit's count, so a
    plan-critique reject raises the re-plan and the next round's critics.
  - A fresh run resets the raise, since it opens a new run slice. `rigger resume-unit` does
    not: the `UnitResumed` fold writes no `UnitFailed`, so a resumed unit re-enters at its
    counted raise.
  - The rule is normative: a site reads its raise by the same rule in the process that charged
    the failure and after a resume, and under the cli driver a re-entered group's read may
    differ from its first (above). The per-site column in Notes names where each site reads
    it. A builder verifies each row against the code, and a row that disagrees
    is fixed to the rule.
- **The cell** is `table[agent.work][step]`.
- **The helper fields** are `table[reading][step]`, the cell the spawn's helpers run at.
- **Pins** apply per field (PINS).
- **The raise counts charged failures, not reviewer rejects alone.** "A rejected attempt" is
  read as "a charged failed attempt". Every charged cause is evidence the unit was harder than
  judged. Counting rejects alone would need a second classification of `UnitFailed` causes;
  both reads above are counts the log already holds.
- `AgentDef::model_for_attempt` stops being a public authority. Its body becomes the
  ladder-pin branch of `resolve`, and `resolve` is its one caller. Its three pure tests in
  `crates/rigger-config-files/src/config_store.rs` (`:1472`, `:1496`, `:1521`) move into
  `resolve`'s table as ladder rows, and it narrows to `pub(crate)` once no driver calls it.
- The conductor resolves once per spawn and carries the result on `SpawnOpts::capability`.
  - `SpawnOpts::new(host, capability)` takes it as a required argument, so the compiler lists
    every construction site, tests included: the conductor's, `spawn_conflict_resolution_implementer`
    (`conductor.rs:13055`), `src/cli/critique.rs:440` and `canary_opts` (`canary_store.rs:605`).
  - `agent_model` is deleted. The stamps read the same `Capability` the opts carry, so the
    stamped model and the launched one agree by construction.
  - A stamp written after its spawn returns reads the `Capability` that spawn launched at,
    never a fresh resolve. `SpecCandidate` (`conductor.rs:1788`) carries the winning lane's
    `Capability` beside `resolved_model`, and `ReviewOutcome` (`:1717`) its adjudicator's
    beside `adj_resolved`. `emit_speculation_winner_status` stamps `green`, `verified` and
    `reviewed` from those, since `record_lane_breaks` has already written records a fresh
    count would read.
  - `run_reviewer` returns the `Capability` its `SpawnOpts` carried beside its `AgentResult`,
    and `run_adjudicator` returns it to its callers. The single-lane and fan-out `reviewed`
    stamps (`:6710`, `:9469`) read the adjudicator's, the plan-critique `reviewed` stamp
    (`:10659`) reads the adjudicator's from the `run_reviewer` call at `:10618`, and
    `ReviewOutcome` copies the adjudicator's.
- The drivers read `opts.capability` and resolve nothing:
  - `claude_code::build_args` and `cli::build_args` push `--model` when the model is non-empty
    and `--effort` when the effort is non-empty.
  - Each host keeps its other flags. `claude_code::build_args` pushes `--fallback-model`, a
    persona field, when it is non-empty and differs from the launched model, compared on the
    two strings, since a fallback that matches the model names nothing to fall back to.
    `cli::build_args` pushes no `--fallback-model`, as today.
  - `launch_as` sets `Launch.model` from `opts.capability`.
  - `spawn_request` copies the model, the effort and the why onto the parked `SpawnRequest`,
    whose new `effort` and `capability` fields take `serde(default)`. They are record fields
    (THE RECORD). An older parked request with neither reads them as empty.
- **A changed capability starts cold.**
  - `continue_prior_session` (`conductor.rs:6152`) points a later attempt at the session
    `recorded_session` (`:17048`) finds on the `STATUS_SPAWN_SESSION` mark. `recorded_session`
    returns that mark's session together with its stamps.
  - A spawn whose `Capability` has `from_table` false (no table, a pin-only persona, or a
    ladder persona with no table field) continues as today, with no comparison.
  - A spawn with `from_table` true continues only when the mark carries `META_CAPABILITY`, and
    its `META_MODEL_ALIAS` and `META_EFFORT` are equal to the new model and effort, compared on
    the two strings. An absent key reads as empty, since `emit_keyed_meta` (`conductor.rs:4756`)
    omits an empty value.
  - This holds for every spawn that writes a mark: implementers, lenses, the adversary,
    sdet-authors, the plan-critique critics and the adjudicators. A critic or adjudicator
    whose raise moves its cell starts that round cold.
  - Otherwise the spawn starts a fresh session on its full prompt with the failure block. A
    mark with no `META_CAPABILITY` (written before this spec) starts a table spawn cold. A
    pinned spawn's mark carries the stamp (`pinned model`), and its `Capability` has
    `from_table` false.
  - Effort is compared as well as model because the binary cannot tell whether a host applies
    a new effort to a resumed session.
  - `resume_missed` (`conductor.rs:17384`) re-points the opts its caller built at the same
    attempt's session, so it inherits its caller's capability and is outside the comparison.
  - A relaunch (`LaunchSession::relaunch`, `crates/rigger-driver/src/driver/mod.rs:234`) forks
    the interrupted session at the relaunching spawn's own `opts.capability`. The two differ
    only after a definition change, never after a raise. So a raise never depends on whether a
    host switches model or effort inside a resumed session.
- **Reviewers rise with their unit.** A review spawn passes the unit's step and raise like the
  implementer does. The comment at `conductor.rs:6120` is replaced by this rule.
- **Spawns with no unit of their own** (`rigger critique` `src/cli/critique.rs:440`, `rigger
  canary` `canary_opts`) resolve unassessed with a raise of 0. A critique pass is not a reject.
  A canary measures each persona at its standing cell. A canary `--model` pin is a model pin
  (`apply_model_pins`), so the pinned persona keeps its standing cell's effort.

**PINS, decided here.**

- A persona's own `model:`, `model_ladder:` or new `effort:` is a pin. It wins over the table
  for its field. `fallback_model` remains a persona field that no table cell carries.
- Per-field results:
  - `model` pinned, `effort` not: the effort comes from the table cell when a table and `work`
    exist, else no effort.
  - `model_ladder` pinned: the model is rung `attempt` of the ladder, the spawn's own attempt
    exactly as today, so a ladder persona's lanes and retries resolve as before.
- A pin below the judging floor is allowed. The floor binds the table, not a project's explicit
  choice.
- `rigger validate` prints one note per pinned field of a persona (`model`, `model_ladder`,
  `effort`, `fallback_model`) naming the persona, the field and its value
  (`capability::pin_notes`). The note is advisory and never a refusal.
- `Config::validate` refuses only incoherent declarations:
  - a persona declaring `work` when the workflow declares no `capability` table, naming the
    persona and the `defaults.capability` key;
  - a persona with neither `work` nor a model pin (`model` or `model_ladder`) when a table is
    declared, since such a persona would otherwise run silently at the host default. The
    refusal names the persona and the two ways to fix it: declare `work`, or pin `model` or
    `model_ladder`.
- The second refusal covers every persona file on disk on purpose, one no stage, review step or
  panel names included: a later stage or planner proposal can name any of them, and the refusal
  names the persona and the two fixes.
- The two refusals are one function, `config::coherence(id, agent, table)` in
  `crates/rigger-domain/src/config.rs` beside `index_agents`, which `Config::validate`,
  `import_agents` and `parse_template_set` each call.
- `parse_template_set` runs it on the set's `implementer` persona against the set's own
  `scaffold_workflow` rendering, after its `parse_workflow` check and so before any write.
  Every set of a template root is parsed before `rigger init` or `rigger setup` writes
  (`parse_template_sets`, `src/cli/setup.rs:165`), so a set whose persona breaks either rule
  refuses the command naming the set key, as its other refusals do, and leaves no file to edit
  by hand.
- A persona with neither `work` nor a pin, and no table, runs at the host default as it does
  today.
- An import runs both refusals in `import_agents`' pre-write check (`src/cli/setup.rs:1862`,
  the check at :1894-1900), against the workflow `scaffold_project` left in place, over the
  prospective fleet: the agents already on disk chained with the staged ones, the collection
  `config::index_agents` checks at :1900. A persona already on disk that breaks either rule
  therefore refuses the import before any file is written. So a fresh
  `rigger setup --agents <dir>` is checked against the scaffolded table, and a refused
  collection writes no file. `stage_import` (`setup.rs:1792`) reads no workflow and is
  unchanged.
- Under the scaffolded table, an import therefore refuses a collection holding a persona that
  declares neither `work` nor a model pin, naming its key and the two ways to fix it. The two `src/cli/setup.rs` import tests
  whose collection personas declare neither
  (`import_agents_refuses_to_overwrite_an_existing_agent`,
  `import_agents_runs_full_validation_and_rejects_a_broken_project`) give them
  `work: writing` in THE SCAFFOLD DECLARES WORK, the criterion that makes that table live.

**THE RECORD, decided here: two meta keys beside `META_MODEL_ALIAS`, no new event type.**

- Four surfaces are stamped from the spawn's `Capability`:
  - (a) every unit event that carries `META_MODEL_ALIAS` today, the events `agent_model`
    stamps (`conductor.rs:5818`, `:6710`, `:7523`, `:9084`, `:9155`, `:9469`, `:10509`,
    `:10659`), and the approve `reuse_approve` (`conductor.rs:6294`) copies, which copies
    `META_EFFORT` and `META_CAPABILITY` beside the model keys. Two have no spawn:
    `UnitStarted` takes `resolve` at raise 0 and attempt 0, and the `skip_implement` resumed
    `green` takes the `Capability` that attempt's implementer would launch at. An agentless
    stage's `UnitStarted` resolves nothing: its three keys are empty and omitted, as its alias
    is today (`agent_model`, `conductor.rs:4814`);
  - (b) the `STATUS_SPAWN_SESSION` mark `record_session` (`conductor.rs:6177`) writes for
    `run_single_stage`, `run_reviewer` and `spawn_sdet_author`. `run_reviewer` covers the
    lenses, the adversary, the plan-critique critics and the adjudicators `run_adjudicator`
    spawns through it. It is the cold-start comparison's source. A result naming no session
    writes no mark, as today;
  - (c) the parked `SpawnRequest`, through `spawn_request`. The headless host parks one for
    every spawn it launches, lane implementers, the re-plan, the conflict resolver and
    critique included;
  - (d) the canary header. `rigger canary` spawns through the cli driver
    (`src/cli/eval.rs:229`), so it has no (c). `CanaryHeader` gains `capabilities`, mapping
    each panel persona's id to the model, effort and why `canary_opts` resolved for it.
    `run_canary` carries those `Capability` values on `CanaryReport` beside
    `resolved_models`, and `cmd_canary` (`src/cli/eval.rs:257`) writes the header from them
    after scoring, never from a second resolve. Every item resolves a persona from one config,
    unassessed at raise 0, so that entry is every spawn's record.
    `CanaryHeader::from_event` reads a header written before this spec as one with no entries.
- The cli driver records no spawn event (`conductor.rs:4196`). Under it, a unit spawn's record
  is (a) and (b) where they exist.
- The keys:
  - `META_MODEL_ALIAS` - the requested model `resolve` chose (empty for a persona resolving to
    the host default). `META_MODEL_RESOLVED` stays the model the host reports it ran;
  - `META_EFFORT` (`"effort"`) - the effort `resolve` chose, empty for the host default;
  - `META_CAPABILITY` (`"capability"`) - the why, in one fixed format.
- The why joins its segments with single spaces and no leading space:
  - when `from_table` is true: `<work>/<step>`, `<step>` being the step after the raise and
    the cap; then `raised <n>` when `raise > 0`, `<n>` being the raise as counted, so it may
    exceed the steps the cap left; then `unassessed` when no step was planned;
  - then `pinned <field>[,<field>]` when a pin supplied a resolved field, naming each such
    field's source (`model` or `model_ladder`, then `effort`).
- No other segment exists. `fallback_model` and the helper fields never appear, so a spawn with
  `from_table` false reads its pins alone (`pinned model`, `pinned model_ladder,effort`),
  whatever its step or raise. A host-default spawn (no table field, no pin) has an empty why,
  which `emit_keyed_meta` omits.
- The constants live in `metrics.rs` beside `META_MODEL_ALIAS`. The `META_MODEL_ALIAS` doc
  (`metrics.rs:139-146`, which names `AgentDef::model` as the source) and the `ModelChange`
  doc name the requested model `resolve` chose.
- These stamps are steering data for the run scorecard. No gate reads them; the cold-start
  comparison reads (b). This spec adds no scorecard column.
- The progress store's `SpawnLaunched` is not used: no run-stream fold may read the progress
  store (`crates/rigger-domain/src/progress.rs:1-21`).

**HELPERS, decided here.**

- `spawn_config_args` (`crates/rigger-driver/src/driver/mod.rs:396`) takes the spawn's
  `Capability` beside the host, so the helper cell reaches `helper_agents_json`. Its callers
  pass it: both `build_args`, and the tests at `driver/mod.rs:586` and `:622` and
  `crates/rigger-driver/src/driver/cli.rs:1083`. The other `helper_agents_json` callers are
  tests too: `handed_helpers_agents_json` (`tests/common/repo.rs:397`) and the one at
  `hooks.rs:1771`.
- `helper_agents_json` applies PINS' per-field rule to a helper whose frontmatter declares
  `work: reading`: its own `model:` or `effort:` wins for that field, and
  `capability.helper_model` and `capability.helper_effort` fill each field it leaves unset.
- A helper declaring any other `work`, or none, gets no cell field.
- The shipped helper text (`.claude/agents/lookup.md`, `.claude/agents/verify.md`, embedded by
  `HELPER_AGENTS`) declares `work: reading` and drops its `model:` line. The `HELPER_AGENTS`
  doc (`hooks.rs:58-62`) names their work, not a model.
- The built-in working-discipline lines naming each helper's model
  (`crates/rigger-domain/src/instructions/working-discipline.md:131`, `:147`) name its work
  instead: reading, at the table's reading row. Their pins (`instructions.rs:541-542`,
  `src/cli/setup.rs:6056`) move with them.
- `parse_helper` passes every frontmatter key into the `--agents` object, so
  `helper_agents_json` removes `work` from it. The host never sees a key it does not define.
- An operator's own copy keeps whatever it declares. `rigger setup` writes a helper only when
  absent (`write_if_absent`, `src/cli/setup.rs:1978`), so an existing copy with a `model:` line
  stays a pin.
- Rigger notes no helper pin. A helper copy is the operator's file in the host's own format,
  which the host also reads directly, and it is outside rigger's own definition (persona files
  and the table).
- The host runs a subagent definition with no `model` field at its parent session's model,
  whether the definition comes from rigger's `--agents` object or from the file an operator's
  own interactive session reads.
- With no table, a helper declaring `work: reading` and no `model` gets neither field, so it
  runs at its parent session's model. `scaffold_project` records in `ScaffoldReport` each helper it
  writes while the workflow it leaves in place declares no `defaults.capability`, and
  `scaffold_summary_lines`, which `cmd_init` and `cmd_setup` both print, names each one.
  - Its test scaffolds over a workflow and personas already on disk that declare no
    `capability` and no `work`, never over a fresh `SCAFFOLD_WORKFLOW`: THE SCAFFOLD DECLARES
    WORK, which lands first (Landing order), gives that a table.
- One reading cell serves both helpers. `lookup` (graph reads) and `verify` (builds and test
  runs, reported exactly) therefore run at the same model and effort for a given step.
- Rigger's `--agents` object is the only reader of `work:`. The host reads the helper file
  directly in an operator's own interactive session, so there a helper with no `model:` runs
  at that session's model.
- The host's `--agents` object takes `effort`: the installed host (2.1.332) declares an
  optional `effort` in the schema it parses that object with and copies it onto the subagent it
  builds, and the host's changelog extends agent `effort` frontmatter to plugin-shipped agents
  at 2.1.78, below `MIN_HOST_VERSION`. So a `work: reading` helper's object carries both
  fields, as the criterion's test asserts.

**SCAFFOLD, decided here.**

- `SCAFFOLD_AGENTS` seeds and `scaffold/rust/set.yml` declare `work:` and no `model:`:
  - planner, reviewers, adversary and adjudicator are `judging`;
  - implementer and test author are `writing`.
- `SCAFFOLD_WORKFLOW` writes a live (uncommented) `defaults.capability` for the default host,
  with a comment above it stating the calibration rule. The seeds declare `work`, so the table
  must be live for a scaffolded project to pass `Config::validate`.
- The `SCAFFOLD_AGENTS` doc comment ("seeds run on" one model) says the seeds declare work and
  the table decides.
- A template set is checked before any write and validated after the scaffold applies it.
  `parse_template_set` (`src/cli/setup.rs:2259`) keeps its `parse_workflow` check and runs the
  PINS refusals on the set's persona, and the freshly scaffolded project THE SCAFFOLD DECLARES
  WORK proves passes `Config::validate` includes one built with `scaffold/rust/set.yml`.
- Under the live table a set whose persona declares neither `work` nor a model pin is refused
  (PINS). A paragraph closing the `docs/handbook/authoring-loops.md` upgrade note (after
  `:123`) tells a template-set author to give the set's `implementer` persona `work:` or a
  `model:` pin. It is a paragraph, not a bullet, because that note's bullet list holds the
  persona sentences `the_handbook_upgrade_note_quotes_each_moved_sentence_once` reads.
- `every_persona_and_scaffold_seed_runs_on_opus_and_may_fan_out` (`src/cli/setup.rs:6215`)
  asserts every shipped persona pins `model: opus`, so it is rewritten:
  - seed and set rows assert `work` declared and `model` empty;
  - committed-fleet rows (`committed_personas` reads this checkout's `.rigger/agents/`) assert
    no model ladder and leave `model` unasserted, since they keep `model: opus` until the
    hand-off replaces it with `work:`;
  - the fan-out assertions stay whole.
- The every-persona-on-opus rule that test pinned is retired: the table decides each
  persona's model.
- A live table reaches every test that scaffolds a project and then loads it or reads a seed:
  a fixture persona declaring neither `work` nor a model pin is refused (PINS), and a seed's
  `model` is empty. THE SCAFFOLD DECLARES WORK re-pins each such test: a fixture persona gains
  `work: writing`, and an asserted seed or set persona text loses `model: opus` and gains its
  `work`. At this base they are:
  - in `src/cli/setup.rs`: the two import tests PINS names, the `with_implementer!` fixture
    (`:3476`), whose `maker` persona every synthetic-set test there shares and which
    `parse_template_set` now refuses before any write,
    `init_project_seeds_a_test_author_and_keeps_a_projects_own`,
    `init_with_the_rust_set_writes_and_names_its_implementer` (the set persona text it
    asserts), `init_with_no_set_writes_and_names_the_language_neutral_implementer` (through
    `LANGUAGE_NEUTRAL_IMPLEMENTER`) and `init_with_a_set_writes_and_names_the_id_its_persona_carries`
    (its `maker` persona);
  - in `tests/init_template_sets_periphery.rs`:
    `a_fresh_scaffold_seeds_the_test_author_it_declares_and_keeps_a_projects_own`,
    `a_rerun_writes_back_the_seed_only_for_the_test_author_the_workflow_declares`,
    `an_agent_file_that_does_not_parse_stops_every_seed_until_it_is_fixed`,
    `setup_agents_imports_a_collections_agent_in_the_place_of_the_seed_of_its_id`,
    `a_rerun_keeps_an_earlier_scaffold_whose_unit_stages_run_rust_engineer`, and
    `demo_implementer`, the persona `init_and_setup_apply_a_set_from_a_template_root_the_operator_owns`
    applies;
  - the fixtures `sdet_author_seed` (`tests/common/fixtures/instructions.rs:205`) and
    `LANGUAGE_NEUTRAL_IMPLEMENTER` (`tests/common/fixtures/config.rs:11`), which the periphery
    file also reads;
  - THE DECLARATIONS ARE COHERENT's `parse_template_set` row, which lands before the table:
    once every rendering carries it, the refused set's persona declares neither `work` nor a
    pin, the case THE SCAFFOLD DECLARES WORK's test names.
- Unaffected: `assert_init_keeps_an_earlier_rust_engineer_scaffold` (`src/cli/setup.rs`) loads
  no config; `init_project_errors_loudly_on_an_unknown_key_and_does_not_scaffold_agents` fails
  its parse before any table is read; the `assert_import_refused` rows assert only an error and
  no write.
- The scaffold's calibration rule: the host's mid model at high effort stands roughly where the
  previous generation's large model stood at medium effort. The scaffold's cells:

  | work | routine | involved | hard |
  |---|---|---|---|
  | judging | opus / high | opus / xhigh | fable / high |
  | writing | sonnet / high | opus / medium | opus / high |
  | reading | haiku / low | haiku / medium | sonnet / medium |

- `MIN_HOST_VERSION` rises from 2.1.139 to 2.1.170, the version under whose heading the host's
  changelog first offers the `fable` model, so every host `rigger run` accepts takes every
  cell value. Each value, with the host version the changelog shows it exists by, or the one
  it was verified on:
  - `opus` and `sonnet`: 1.0.88, whose entry names both aliases;
  - `haiku`: 2.0.41, the first entry naming `--model haiku`;
  - `low`, `medium` and `high`: 2.1.72, where the changelog sets the effort levels to those
    three and lists `--effort`;
  - `xhigh`: 2.1.111, where the changelog adds it, a model without it falling back to `high`;
  - `fable`: 2.1.170;
  - `haiku` with an effort (`low`, `medium`): the changelog names no version; verified on
    2.1.332, where both run and answer.
- An account with no access to a cell's model fails that cell's launches visibly, as any model
  the host refuses does today, and the operator edits the one table; this spec adds no
  fallback.
- The raised minimum's text and pins move with it:
  - the `MIN_HOST_VERSION` doc (`hooks.rs:295-300`) names the scaffold table as the reason, the
    hook `args` field's 2.1.139 being below it; the `install_spawn_hooks` doc line naming the
    minimum (`:319`) stays true;
  - `too_old`'s line (`crates/rigger-driver/src/claude_version.rs:68-70`) says the minimum is
    the version the scaffolded capability table needs, naming no model (Global constraints);
  - the tests and fixtures pinning 2.1.139 or 2.1.138 move to 2.1.170 and 2.1.169: the
    version rows and their test's doc (`claude_version.rs:91`, `:98-110`), the too-old line's
    test (`:122-136`), whose name and doc name the new reason,
    `a_session_host_is_the_one_binary_its_reads_and_starts_run`
    (`crates/rigger-driver/src/claude_auth.rs:352`, `:368`), the stand-in default every
    `rigger run` and `rigger validate` fixture answers `--version` with
    (`tests/fixtures/launch-args.sh:9`, `:16`), and the rows of `tests/cli.rs:3768-3774` and
    `tests/validate_advisories.rs:679-693`;
  - `claude_version.rs:50` keeps its format example, and the callers (`src/cli/run.rs:980`,
    `src/cli/validate.rs:47`) are unchanged.

**A CONSUMER THAT CHANGES NOTHING, decided here.**

- A project with no `capability` table and personas that pin `model:` loads and runs as
  before:
  - each spawn takes its pinned model, ladder rung by attempt as today;
  - no `--effort` is passed;
  - `META_MODEL_ALIAS` holds the same value;
  - `META_CAPABILITY` reads `pinned model` or `pinned model_ladder`, whatever the step or
    raise;
  - every session continues as today, since no spawn takes a table field.
- `rigger validate` adds one note per pin and refuses nothing it accepted before.
- Its session host must be 2.1.170 or later: `rigger run` refuses, and `rigger validate` warns
  of, an older one whatever the project declares (SCAFFOLD).
- Its helper copies keep their `model:` lines, which are now pins. Re-running `rigger setup`
  keeps them, because setup writes a helper only when absent.
- Only a helper setup writes fresh (a project that had none) declares `work: reading`. Without
  a table it runs at its parent session's model (HELPERS), and the `rigger init` or `rigger
  setup` output names it.
- The one case that needs an edit: `rigger setup` healing a workflow-referenced persona missing
  from disk (`write_seed_agents`, `src/cli/setup.rs:306`) writes a seed that declares `work`.
  In a project with no table, the next load fails through THE DECLARATIONS ARE COHERENT's
  refusal, naming the persona and `defaults.capability`. The operator adds the table or pins
  the healed persona. Setup adds no check of its own.

**INVENTORY of text that says a persona declares a model, decided here so the docs unit
misses none.**

| Text | Becomes |
|---|---|
| `docs/handbook/authoring-agents.md` :10, :25, :33-43, :49-50, :105 | `work:` in the example and field table; the "Model tiering" section rewritten as the capability table, pins, floor and raise; the two helpers run at the table's reading row. |
| `docs/handbook/best-practices.md` :35 (#12), :37 (#13) | Tier by work and complexity through the table; a pin is a project's explicit choice. |
| `docs/handbook/agentic-sdlc.md` :41-43 | The heading and :43 say the planner proposes every baseline unit with its step and reason, and splits, adds or wires units where the baseline must change; :43 names the judging row, not a model. |
| `examples/demo/README.md` | Says the demo pins each persona's `model:` on purpose to show PINS. Its `.rigger/agents/*.md` keep `model:` and its workflow declares no table, so `demo_example_loads` (`config_store.rs:2392`) and `shipped_workflows_load_and_validate` (`src/cli/mod.rs:9793`) load it unchanged. |
| `docs/handbook/README.md:12` | "capability table" for "model tiering". |
| `README.md:39` | A persona declares its kind of work, its tools and its prompt. |
| `docs/architecture.md` :144, :205, :898, :1217 | `id . work . tools . prompt`; the agent-driver section says the conductor resolves and the driver launches; glossary updated. |
| `docs/architecture-addendum-claude-code-integration.md:103` (§4.1) | The launch row adds `--effort`, both read from the resolved capability, and the fallback pushed only when it differs from the model. |

Text owned where the code changes, outside that table:

- THE CONSIDERATIONS ARE BUILT IN: the built-in list in `docs/handbook/authoring-agents.md`
  (:86 "three entries", the numbered list at :88-90 gaining `4. complexity` with its
  `Source:` path, :96 "the three built-in entries"); `SCAFFOLD_INSTRUCTIONS_README`
  (`src/cli/setup.rs:1991`); `.rigger/instructions/README.md`; the `instructions.rs:32-44` doc
  comment; the tests pinning the built-in list (`tests/common/fixtures/instructions.rs`,
  `the_handbook_lists_every_built_in_entry_in_order_with_its_source` in `tests/cli.rs`,
  `the_scaffolded_instructions_readme_names_every_built_in_and_the_override_rules` in
  `src/cli/setup.rs`, `src/cli/prime.rs`).
- HELPERS RUN AT THE READING CELL: the `HELPER_AGENTS` doc and the working-discipline helper
  lines with their pins (HELPERS); the `helper_agents_json` doc (`hooks.rs:369-372`), which
  says each field is the one the helper's frontmatter gives and is to say the cell fills each
  field a `work: reading` helper leaves unset; and
  `init_scaffolds_the_fan_out_helpers_byte_identical_to_the_committed_files`
  (`src/cli/setup.rs:7117`), whose `model: haiku` and `model: sonnet` asserts become
  `work: reading` and no `model:` line.
- THE DRIVERS LAUNCH WHAT THEY ARE GIVEN:
  - the `SpawnOpts::attempt` doc (`agent.rs:122-127`) and the `spawn_request` doc
    (`agent.rs:284-293`);
  - the `SpawnRequest::model` doc (`crates/rigger-domain/src/spawn.rs:578`), which names a
    model as its example;
  - the `AgentDef::model_ladder` and `AgentDef::fallback_model` docs (`config.rs:180-193`) and
    the `cli::build_args` doc (`driver/cli.rs:263-264`), which name `model_for_attempt` as the
    authority or the fallback as pushed on every launch;
  - the `canary_store.rs:171` doc;
  - the `ModelNotFound` action text (`agent.rs:668-671`), which names the table cell, or the
    persona's pin when one is set, as the thing to fix.
- EACH SPAWN STATES WHAT IT RAN AT: the `META_MODEL_ALIAS` and `ModelChange` docs (THE
  RECORD), and the plan-critique `UnitStarted` comment (`conductor.rs:10504-10506`), which is
  to say every `UnitStarted` stamps the unit's unraised cell at attempt 0.
- THE PLANNER RECORDS COMPLEXITY: the planner texts, the `PLANNING_POLICY` pin, the
  `plan_protocol_text` fixture and the `authoring-loops.md:123` upgrade bullet ("What the
  planner is asked for").
- THE DECLARATIONS ARE COHERENT: the `import_agents` doc (`src/cli/setup.rs:1855-1861`) and its
  pre-write comment (`:1894-1896`), which name the id rule alone and are to name the two
  coherence refusals beside it, and the `parse_template_set` doc (`:2250-2258`), which lists
  that function's refusals and is to name them too.
- THE SCAFFOLD DECLARES WORK: the `SCAFFOLD_AGENTS` doc comment, the opus test, the raised
  minimum's doc, line and pins, and the upgrade note's template-set paragraph (SCAFFOLD).

## Global constraints

- Hyphens, not em dashes (U+2014 fails the diff gate).
- Both feature lanes green (default features and `--no-default-features`).
- No new event type. Complexity rides on `UnitProposed`, and the record rides on existing
  unit-event meta, the spawn-session mark, the parked request and the canary header.
- No quantitative proxy decides a step. Resolution reads only `Complexity`, the raise and pins,
  never a size.
- No new counter and no new limit. The raise reads counts the log already holds, is bounded by
  the existing retry bound and is capped at `hard`.
- The binary names a model only in `SCAFFOLD_WORKFLOW` text and in test fixtures.
- Language-agnostic: the `complexity` built-in and every prompt string name no language, which
  the existing no-language tests enforce
  (`the_built_in_layer_and_the_reviewer_discipline_name_no_language` and
  `the_scaffold_a_project_gets_with_no_template_root_names_no_language`, `src/cli/setup.rs`).
- Nothing silent in rigger's own definition (persona files and the table): an unassessed step
  that selects a spawn's own cell, a pin and a refused step each show in the stamp, a note or a
  `DecisionMade`.

## Done when

- [ ] THE WORK KEY PARSES: a test proves `parse_agent` reads `work: judging|writing|reading`
  into `AgentDef::work` and an `effort:` pin into `AgentDef::effort`, and refuses any other
  `work` value naming the persona and the three values. This criterion OWNS `Work`, the two
  `AgentDef` fields and their parse in `crates/rigger-domain/src/config.rs`; validation against
  a table is criterion THE DECLARATIONS ARE COHERENT's, NOT this one's.
- [ ] THE TABLE PARSES AND HOLDS THE FLOOR: a table-driven test proves `Workflow` parses
  `defaults.capability` into `CapabilityTable` refusing a missing row, a missing cell, an empty
  `model` or an unknown key by its path, and that `Config::validate` refuses through
  `CapabilityTable::validate` a table whose `judging.routine` cell differs from
  `judging.involved` in `model`, accepts one whose two cells differ only in `effort`, while
  `read_scratch_defaults` still parses a table that breaks the floor. This criterion OWNS
  `CapabilityTable`, `Complexity` and the floor rule in `crates/rigger-domain/src/config.rs`,
  and the `CapabilityTable::validate` call in `Config::validate`; the persona refusals there
  are criterion THE DECLARATIONS ARE COHERENT's, NOT this one's.
- [ ] THE DECLARATIONS ARE COHERENT: a table-driven test proves `Config::validate` refuses a
  persona declaring `work` under a workflow with no `capability` table, refuses a persona with
  neither `work` nor `model`/`model_ladder` when a table is declared, accepts `model:`-pinned
  personas with no table, that `import_agents` refuses a collection in either case before it
  writes any file, and that `parse_template_set` refuses a set whose persona declares `work`
  under its table-less rendering. This criterion OWNS `config::coherence` and its calls in
  `Config::validate`, in `import_agents`' pre-write check and in `parse_template_set`, and the
  text the INVENTORY assigns to it; the `CapabilityTable::validate` call is criterion
  THE TABLE PARSES AND HOLDS THE FLOOR's and pin notes are criterion PINS ARE NOTED's, NOT this
  one's.
- [ ] PINS ARE NOTED: a test proves `capability::pin_notes` yields one line per pinned `model`,
  `model_ladder`, `effort` or `fallback_model` field of a persona naming the persona, field and
  value, and that `src/cli/validate.rs` prints them as notes beside `model_drift_advisory`
  without failing. This criterion OWNS `capability::pin_notes` and its print.
- [ ] ONE RESOLUTION: a table-driven test proves `capability::resolve` returns the table cell at
  the planned step raised by `raise` and capped at `hard`, `involved` when unassessed, the
  reading cell as the helper fields, a pinned `model`/`effort` winning per field, a ladder rung
  chosen by `attempt`, the host default with no table and no pin, `from_table` set only when a
  cell supplied a field, and the `why` string in THE RECORD's format. Its ladder rows are the
  three `model_for_attempt` tests moved from `crates/rigger-config-files/src/config_store.rs`.
  This criterion OWNS `capability::resolve`, `Capability` and the module
  `crates/rigger-domain/src/capability.rs`; `capability::pin_notes` is criterion PINS ARE
  NOTED's and narrowing `model_for_attempt` is criterion THE DRIVERS LAUNCH WHAT THEY ARE
  GIVEN's, NOT this one's.
- [ ] THE PLANNER RECORDS COMPLEXITY: a table-driven test proves `harvest_proposed` gives a
  proposed unit's `Stage::complexity` and `Stage::complexity_reason` the valid pair of its
  `UnitProposed`, keeps the earlier pair on a refine whose pair is absent or refused, takes no
  pair from a proposal after the unit's first `UnitStarted` (live or on a resume that seeds
  the unit integrated), leaves an absent pair unassessed with no record, and for a refused
  pair (an unknown word, a `null` or non-string step, a step with no reason) keeps the
  proposal as a stage and records one `DecisionMade` `unassessed-complexity:<unit>#<position>`
  that a second harvest of the same log does not repeat. A second test proves
  `build_dag_critique_prompt` renders each unit's step and reason through `DAG_UNIT_ROW`, and
  `unassessed` for a stage with none. This criterion OWNS
  `UnitProposed`'s two fields, the two `Stage` fields, the `PLAN_ROLE`, `PLAN_UNIT_PROPOSED`,
  `PLAN_UNMATCHED`, `PLAN_CRITERIA_LEAD`, `DAG_UNIT_ROW` and `REPLAN_LEAD` texts in
  `crates/rigger-conductor/src/prompt_surface.rs` with the tests pinning them, the
  `render_plan_protocol` doc, the planner policy sentence in `.rigger/agents/planner.md` and
  in the `SCAFFOLD_AGENTS` planner seed with `PLANNING_POLICY` and `plan_protocol_text`, the
  planner bullet of the `docs/handbook/authoring-loops.md` upgrade note, and the
  `SCAFFOLD_WORKFLOW` planner and `produces: dag` comments; the rest of `SCAFFOLD_AGENTS` and `SCAFFOLD_WORKFLOW` is criterion THE
  SCAFFOLD DECLARES WORK's, NOT this one's.
- [ ] THE CONSIDERATIONS ARE BUILT IN: a test proves `instructions::BUILTIN` carries a fourth
  entry `complexity` whose body names the five considerations and the three steps, that
  `resolve_layers` replaces it from `.rigger/instructions/complexity.md` and switches it off
  when that file is blank, and that the existing no-language test covers its body. This
  criterion OWNS `crates/rigger-domain/src/instructions/complexity.md` and the built-in-list
  text and tests the INVENTORY assigns to it, `SCAFFOLD_INSTRUCTIONS_README` among them.
- [ ] THE CONDUCTOR RESOLVES EVERY SPAWN ONCE: a table-driven test proves every spawn carries on
  `SpawnOpts::capability` the cell its unit's step raised by `charged_failures` selects. Its
  rows are a reviewer from `reviewer_spawn_opts` after one charged failure, a lane implementer
  and a lane reviewer of `run_speculation` at lane 2 with no failure, a re-entered group after
  one lost group of width 3 resolving `raised 1`, a width-1 re-entry after that lost group
  resolving `raised 1`, `re_plan` after one plan-critique reject, the same unit resumed from
  the log after one charged failure, a headless group resumed after its winner's lane-break
  records resolving the raise its first open read, a resumed winner path over a log that
  already holds those records leaving each recorded once, and two winning groups of one unit
  under the cli driver, a compensation between them, each recording its own broken lane. This
  criterion OWNS
  `SpawnOpts::capability`, the `SpawnOpts::new(host, capability)` signature, every
  construction site, `charged_failures` with the group's anchor, the keyed
  `record_lane_breaks` write, and the `EchoModelDriver` of `tests/model_pinning_periphery.rs` reporting
  `opts.capability.model` with that file's module doc; launch flags are criterion THE DRIVERS
  LAUNCH WHAT THEY ARE GIVEN's and the stamps criterion EACH SPAWN STATES WHAT IT RAN AT's,
  NOT this one's.
- [ ] A CHANGED CAPABILITY STARTS COLD: a table-driven test proves `continue_prior_session`
  continues the session `recorded_session` finds for a spawn whose `Capability` has
  `from_table` false, continues a table spawn only when the mark's `META_MODEL_ALIAS` and
  `META_EFFORT` are equal to its model and effort compared on the two strings, and starts a
  table spawn cold on a mark with no `META_CAPABILITY`. This criterion OWNS that comparison and
  `recorded_session` returning the mark's stamps in `crates/rigger-conductor/src/conductor.rs`;
  writing the stamps is criterion EACH SPAWN STATES WHAT IT RAN AT's, NOT this one's.
- [ ] THE DRIVERS LAUNCH WHAT THEY ARE GIVEN: a test proves `claude_code::build_args` and
  `cli::build_args` emit `--model` and `--effort` from `opts.capability` (each omitted when
  empty), `claude_code::build_args` emits `--fallback-model` only when it differs from that
  model, `launch_as` sets
  `Launch.model` from `opts.capability`, and `spawn_request` copies its model, effort and why onto
  `SpawnRequest`, none of them calling a model resolution. This criterion OWNS the
  driver reads in `crates/rigger-driver/src/driver/`, `SpawnRequest::effort`,
  `SpawnRequest::capability`, narrowing `AgentDef::model_for_attempt` to `pub(crate)` and the
  code docs the INVENTORY assigns to it; `spawn_config_args` is criterion HELPERS RUN AT THE
  READING CELL's, NOT this one's.
- [ ] HELPERS RUN AT THE READING CELL: a test proves `helper_agents_json` fills each of `model`
  and `effort` a `work: reading` helper leaves unset from the spawn's helper cell, keeps each
  field the helper sets, never carries a `work` key into the object, and that `rigger init`
  and `rigger setup` name each helper they write while the project's workflow declares no
  `defaults.capability`. This
  criterion OWNS `helper_agents_json`, `spawn_config_args` and its callers, that
  `ScaffoldReport` record and its line in `scaffold_summary_lines`, the shipped
  `.claude/agents/lookup.md` and `.claude/agents/verify.md` frontmatter, and the helper text
  the INVENTORY assigns to it with its pins.
- [ ] EACH SPAWN STATES WHAT IT RAN AT: a test proves a unit spawn's stamped events and its
  `STATUS_SPAWN_SESSION` mark carry `META_MODEL_ALIAS`, `META_EFFORT` and `META_CAPABILITY`
  equal to the `Capability` on that spawn's `SpawnOpts`, compared on the three strings,
  including `raised 1` after one charged failure, `unassessed` for an unproposed stage under a
  table, and a
  speculation winner whose earlier lane's merge broke. This criterion OWNS `META_EFFORT`,
  `META_CAPABILITY`, stamping all three on those events, in `reuse_approve` and in
  `record_session` from the `Capability`, the `Capability` that `run_reviewer` and
  `run_adjudicator` return and `SpecCandidate` and `ReviewOutcome` carry, the
  `META_MODEL_ALIAS` and `ModelChange` docs, and deleting
  `agent_model` in `crates/rigger-conductor/src/conductor.rs`.
- [ ] A CANARY STATES WHAT EACH PERSONA RAN AT: a test proves `rigger canary`'s header event
  carries, for each panel persona, the model, effort and why on the `Capability` its
  `canary_opts` resolved, and that `CanaryHeader::from_event` reads a header written without
  them as one with no entries. This criterion OWNS `CanaryHeader::capabilities`, its
  `to_event` and `from_event` fields in `crates/rigger-domain/src/canary.rs`, the
  `CanaryReport` field that carries them in `crates/rigger-conductor/src/canary_store.rs`, and
  the header write in `src/cli/eval.rs`.
- [ ] THE SCAFFOLD DECLARES WORK: a test proves every `SCAFFOLD_AGENTS` seed and
  `scaffold/rust/set.yml` declare `work:` and no `model:`, `SCAFFOLD_WORKFLOW` declares a
  `defaults.capability` that passes `CapabilityTable::validate`, a freshly scaffolded
  project, with no set and with the rust set, passes `Config::validate`, `parse_template_set`
  refuses a set whose persona declares neither `work` nor a model pin, and `too_old` names
  2.1.169 too old and 2.1.170 not. This criterion OWNS `SCAFFOLD_AGENTS`, its doc comment,
  `SCAFFOLD_WORKFLOW`, `scaffold/rust/set.yml`, `MIN_HOST_VERSION` with its doc, the `too_old`
  line, the rewrite of `every_persona_and_scaffold_seed_runs_on_opus_and_may_fan_out` in
  `src/cli/setup.rs`, the test and fixture re-pins SCAFFOLD assigns to it and the upgrade
  note's template-set paragraph; the planner texts in
  `SCAFFOLD_AGENTS` and `SCAFFOLD_WORKFLOW` are criterion THE PLANNER RECORDS COMPLEXITY's,
  NOT this one's.
- [ ] THE DOCS DESCRIBE THE TABLE: each INVENTORY table row's file and line reads as its
  "Becomes" column, which the adjudicator checks row by row against the cited line's text. No
  page under `docs/handbook/`, `docs/architecture.md`,
  `docs/architecture-addendum-claude-code-integration.md` or `README.md` says a persona
  declares its model. This criterion OWNS those doc files and `examples/demo/README.md`, except
  the `authoring-agents.md` built-in list lines, which are criterion THE CONSIDERATIONS ARE
  BUILT IN's, the `authoring-loops.md` upgrade note's planner bullet, which is criterion THE
  PLANNER RECORDS COMPLEXITY's, and its template-set paragraph, which is criterion THE SCAFFOLD
  DECLARES WORK's.

## Notes

**Where each spawn site's inputs come from** (the table THE CONDUCTOR RESOLVES EVERY SPAWN ONCE
builds against). The rule in RESOLUTION governs, and the builder verifies each row against the
code. Record names THE RECORD's surfaces; (c) is every headless-host spawn.

| Site | Persona | Step | Raise | Record |
|---|---|---|---|---|
| `run_single_stage` `conductor.rs:7274` (implementer) | stage agent | `Stage::complexity` | the unit's `charged_failures` | (a) `UnitStarted` (`start_and_run_stage`, `:5818`), `:7523`; (b); (c) |
| `run_single_stage` (plan stage) | planner | unassessed | the plan unit's `charged_failures` | (a); (b); (c) |
| `re_plan` `:10307` | planner | unassessed | the plan-critique gate unit's `charged_failures` | (c) |
| `plan_critique_loop` `:10477` | critic | unassessed | the plan-critique gate unit's `charged_failures` | (a) `:10509`, `:10659`; (b) through `run_reviewer`; (c) |
| `reviewer_spawn_opts` `:6076` outside a speculation lane (lenses, adversary, adjudicator, sdet-author, the fan-out loop's included) | step agent | the unit's | the unit's `charged_failures` | (a) `:6710`; (b) through `run_reviewer`, `spawn_sdet_author`; (c) |
| `reviewer_spawn_opts` from a speculation lane (`review_unit` `:6471` passes the lane's `attempt`) | step agent | the unit's | its group's raise (RESOLUTION), NOT the lane's `attempt` | (a) `:9155` from the adjudicator's `Capability` on `ReviewOutcome`; (b); (c) |
| `run_fan_out_review_loop` `:9331`: lenses (`run_review_agents_concurrently` `:9368`), adversary (`run_finding_reviewer` `:9385`), adjudicator | step agent | the unit's | the unit's `charged_failures` | (a) `:9469`; (b) through `run_reviewer` and `run_adjudicator` (`:10187`); (c) |
| `run_speculation` `:8544` lanes, and `emit_speculation_winner_status` `:9075` | implementer | the unit's | its group's raise (RESOLUTION), never `base + lane` | (a) `:9084` from the winning lane's `Capability` on `SpecCandidate`; (c) |
| `spawn_conflict_resolution_implementer` `:13055` | implementer | the unit's | the unit's `charged_failures` | (c); it records no session, so a later attempt never continues it |
| `resume_missed` `:17384` | its caller's | its caller's | its caller's | its caller's; outside the cold-start comparison |
| `src/cli/critique.rs:440` | critic | unassessed | 0 | (c) |
| `canary_opts` `canary_store.rs:605` | panel persona | unassessed | 0 | (d) |

**Corners decided above, collected:**

- **Empty** (no table, no `work`): resolution is today's, and the why names the pins alone,
  empty when there are none.
- **Repeated**: the same proposal twice refines to the same pair; a refused pair records once
  per proposing event.
- **Reverted** (a re-plan lowers a step after a reject): the new step applies to a unit that
  has not started. A started unit keeps its frozen step, and the raise still counts.
- **DROPPED**:
  - a refine that omits the pair, or whose pair is refused, keeps the earlier pair;
  - a re-plan that drops a unit is the existing scope path;
  - a table edit that drops a row fails the load;
  - a mark that lacks the stamps starts a table spawn cold.
- **Concurrent**: resolution is a pure function of the loaded config and the log fold. Every
  spawn of a group resolves at the group's one raise (RESOLUTION). A hold stays run-wide.
- **Crash-resume**: steps refold from `UnitProposed` under the same freeze, and the raise is
  the count of `UnitFailed` in the log. A speculation group's count stops at its first lane
  `SpawnRequested`, so a group resumed after its winner's lane-break records resolves the
  raise it opened at, and `record_lane_breaks`' keyed write records each broken lane once. A
  crash after the winner's `green` write resumes on the single-lane path, whose spawns read the
  unit's count. A unit that folds to `green` or `verified` there takes `skip_implement`: its
  resumed `green` is keyed `{unit}/green#{attempts}`, `attempts` being `effective_attempts`,
  so it is a no-op when that count is the winner's attempt and otherwise one more `green`
  status, stamped at the single-lane `Capability`; either way it writes no `UnitFailed` and
  raises nothing. A unit folded to `reviewed` resumes at integrate and writes no `green`.
  Under the cli driver the group reads the whole slice (RESOLUTION). `resume-unit` keeps the
  raise. A relaunch forks at the relaunching spawn's capability.
- **Cold start**: the same.
- **Existing data**:
  - `UnitProposed` events with no pair resolve unassessed and record nothing;
  - parked `SpawnRequest`s with no `effort` or `capability` read them as empty;
  - spawn-session marks with no stamps continue a non-table spawn and start a table spawn cold;
  - a canary header with no `capabilities` reads as one with no entries;
  - a pinned project runs unchanged;
  - a healed seed in a no-table project fails the next load naming the key (A CONSUMER THAT
    CHANGES NOTHING).
- **Mid-run table edit**: a definition change through `definition_hash`. Each table spawn whose
  cell moved starts cold. A spawn whose request a process parked before the change, and a
  relaunch on the new definition re-enters, launches from its current `opts`
  (`before_launch`, `crates/rigger-driver/src/driver/claude_code.rs:475`, launches on
  `Requested` as on `Parked`), so it runs at, and (a) and (b) stamp, the newly resolved
  capability. Its (c) record keeps the park-time values, since `read_or_park` parks an id once.

**Out of scope**:

- a scorecard column over the new stamps;
- any check of a host's model or effort vocabulary;
- a per-stage table override;
- noting a helper copy's pins.

## Landing order

1. THE WORK KEY PARSES, THE TABLE PARSES AND HOLDS THE FLOOR, THE CONSIDERATIONS ARE BUILT IN
   (independent).
2. - ONE RESOLUTION needs THE WORK KEY PARSES and THE TABLE PARSES AND HOLDS THE FLOOR (`Work`,
     `AgentDef::effort`, `CapabilityTable`, `Complexity`).
   - THE DECLARATIONS ARE COHERENT needs both of 1's first two (`Work`, and `Config::validate`,
     which both edit).
   - THE PLANNER RECORDS COMPLEXITY needs THE TABLE PARSES AND HOLDS THE FLOOR (`Complexity`).
     - In this repository its `.rigger/agents/planner.md` edit is a definition change. A run's
       own landing re-pins the run (`checkout_definition`, `conductor.rs:4557`), so it needs no
       flag.
     - No planner spawns once the plan is approved (`re_plan`'s one caller is
       `plan_critique_loop`), so the edit reaches a planner only in a later run. The current
       `UnitProposed` derives `Deserialize` without `deny_unknown_fields`, so an older binary
       reads a proposal carrying the two fields as before.
3. - PINS ARE NOTED needs ONE RESOLUTION (it adds `pin_notes` to the module ONE RESOLUTION
     creates).
   - THE CONDUCTOR RESOLVES EVERY SPAWN ONCE needs ONE RESOLUTION (`resolve`) and THE PLANNER
     RECORDS COMPLEXITY (`Stage::complexity`).
4. - EACH SPAWN STATES WHAT IT RAN AT needs THE CONDUCTOR RESOLVES EVERY SPAWN ONCE
     (`SpawnOpts::capability`).
   - A CANARY STATES WHAT EACH PERSONA RAN AT needs THE CONDUCTOR RESOLVES EVERY SPAWN ONCE
     (the `Capability` on `canary_opts`' `SpawnOpts`).
5. - A CHANGED CAPABILITY STARTS COLD needs EACH SPAWN STATES WHAT IT RAN AT (the mark's
     stamps).
   - THE DRIVERS LAUNCH WHAT THEY ARE GIVEN needs EACH SPAWN STATES WHAT IT RAN AT (which
     deletes `agent_model`, a caller of `model_for_attempt`) and ONE RESOLUTION (which moves
     the three `config_store.rs` callers), so `pub(crate)` compiles.
6. - THE SCAFFOLD DECLARES WORK needs THE DECLARATIONS ARE COHERENT (its validate test proves
     the real rule) and THE DRIVERS LAUNCH WHAT THEY ARE GIVEN (a seed with no `model:`
     launches its cell's model only once the drivers read `opts.capability`).
   - HELPERS RUN AT THE READING CELL needs THE DRIVERS LAUNCH WHAT THEY ARE GIVEN (both edit
     `build_args`) and THE SCAFFOLD DECLARES WORK (a fresh scaffold names no helper only once
     `SCAFFOLD_WORKFLOW` carries the table).
     - In this repository it is a definition change: the shipped helper text is this
       checkout's live `.claude/agents/` copies, which `definition_hash` folds as
       `helper:<name>`. A run's own landing re-pins the run (`checkout_definition`,
       `conductor.rs:4557`), so it needs no flag.
     - The running binary resolved its helper text once at start (`resolve_spawn_host`,
       `hooks.rs:83`), so the run's later spawns keep the old helpers and their pins.
     - A relaunch on the old binary would read the landed text and hand the host the `work`
       key, so the hand-off's rebuild comes before any relaunch.
     - Between that rebuild and the hand-off's table write, this repository's helpers run at
       their parent session's model (HELPERS).
     - The operator's own interactive session reads those files directly (HELPERS).
     - Land it last before the docs, with the hand-off straight after.
7. THE DOCS DESCRIBE THE TABLE (last).

## Hand-off (operator, at landing)

1. Rebuild and install the binary, then run `rigger setup`, before any relaunch of the run
   that landed this spec.
2. Write the `defaults.capability` block `SCAFFOLD_WORKFLOW` carries into
   `.rigger/workflow.yml`. This repository adopts the scaffold table unchanged (SCAFFOLD): the
   planner's step already carries how hard a unit is, so a second bump here would count it
   twice. Rewrite its plan-stage comment (`:224-229`) and its `produces: dag` comment (`:232`)
   to the `SCAFFOLD_WORKFLOW` comments THE PLANNER RECORDS COMPLEXITY ships.
3. Replace each persona's `model:` line under `.rigger/agents/` with its `work:`. This retires
   the every-persona-on-opus rule here too: the table decides each persona's model.
   - The evaluator runs at whatever `.rigger/gates/evaluate.sh`'s `$EVALUATE_AGENT` names,
     outside the conductor and the table, so its `work: judging` serves only
     `Config::validate`'s coherence check.
4. Check `.claude/agents/lookup.md` and `.claude/agents/verify.md` carry `work: reading` and no
   `model:` line, as HELPERS RUN AT THE READING CELL ships them. A `model:` line left there is
   a pin.
5. Commit the edits as one definition change.
6. Relaunch with `--rebase-definition`.
7. Run `rigger validate`, then `rigger canary` (ungated: the switch changes aliases and
   efforts, which `--if-model-changed` does not read as drift).

| Persona | work |
|---|---|
| planner | judging |
| adversary | judging |
| adjudicator | judging |
| architecture-reviewer | judging |
| sdet (read-only test reviewer) | judging |
| evaluator | judging |
| rust-engineer | writing |
| sdet-author | writing |
| scribe | writing |
