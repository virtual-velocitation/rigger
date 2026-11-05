# 123 - The spec critique passes to exhaustion

**Goal:** `rigger critique <spec>` runs the critic over one text pass after pass until a pass adds no
finding, and the critique of that text is the union of every pass, so a spec reaches zero BLOCKING
in few author rounds instead of one shallow pass per round. Today one call is one pass
(`src/cli/critique.rs:105-126`): the critic returns a handful of findings and stops, and the record
of a hash is its single latest result (`review::latest_critique`,
`crates/rigger-domain/src/review.rs:541-547`). Measured on the critique stream
`proj-ee754c06ff306a3a-critique-run` of `.rigger/events.db`, read-only:

- Spec 121 took 26 passes (requests at positions 4466782 to 4467136, 4 h 04 min of wall clock, an
  author round between every two), finding 14, 9, 13, 11, 8, 12, 4, 8, 7, 9, 7, 2, 5, 1, 4, 7, 3, 6,
  7, 1, 3, 5, 2, 5, 3 and 2 lines. Of the 144 findings of passes 2 to 26, 65 (0.451) cite a criterion
  or Design block the author did not change between pass k-1 and pass k: text pass k-1 read and did
  not report on. 28 of the 144 (0.194; 7 of them BLOCKING) state a defect pass 1's own text already
  held. None restates a pass k-1 finding.
- Spec 120 took 35 passes (positions 4466547 to 4466827): 72 of 184 pass-2-onward findings (0.391)
  cite unchanged text and 45 (0.245) hold for pass 1's text. Its last 9 passes ran under the
  adversary persona's spec critique section and show the same profile (17 of 34, 0.500).

Every one of the 61 passes ran at attempt 0 of a fresh hash: no text was ever critiqued twice, so
the depth a second pass over the same bytes would add was never collected.

## Design

**BASE, decided here.** Spec 123 lands on `rigger-run` before spec 121 launches.
`spec_critique_prompt` (`review.rs:343-348`) is the task framing with the voided-duties clause
(`CRITIQUE_FRAMING`), then `CRITIQUE_OUTPUT_CONTRACT`, then the spec text verbatim; this spec
extends that function and changes none of its kept sentences. Code citations are read at
867e811d; the symbol governs and the line is a pointer. Precondition of spec 121, no duty on this
spec's units: 121's registry (its critique prompt group in `review.rs`) lists
`CRITIQUE_RECORDED_FINDINGS`, a row 121's Design carries before 121 launches.

**A PASS AND EXHAUSTION, decided here (criterion 1).** A PASS of a hash is, for one critique spawn
id of the hash, the one recorded `SpawnResult` that `result_of` picks for that id, when the
authority (`critique_of`, `review.rs:508-518`) accepts it as a critique of the hash; two results for
one id are one pass, and a spawn that fails or records no result is not a pass. The PASS ORDER is ascending
attempt, then store position. The hash is EXHAUSTED when one of its passes carries zero finding
lines (`critique_findings`, `review.rs:464-489`); such a pass approves by the output contract, so the
authority accepts it. Exhaustion is read from the recorded `SpawnResult` events on every read, never
from a flag or a counter held by the process: no new event type, and an in-memory pass counter is
NOT an implementation of this. The ATTEMPT COUNT of a hash is the number of parked requests for it,
`review::next_critique_attempt` (`review.rs:594-605`), the count spec 112 already keeps: a spawn
that brings no pass consumes one, and so does a doubled park. The ceiling counts attempts, never
passes. A line that restates a recorded finding is a finding like any other:
it mints a new id and keeps the hash from exhaustion, its attempt counts toward the ceiling, the
`CRITIQUE_RECORDED_FINDINGS` line tells the critic not to restate, and the ceiling bounds the cost. A
`DecisionMade` whose `resolves` names both ids resolves the restated BLOCKING finding with its original.

**THE RECORD IS THE UNION, decided here (criterion 1).** One pass is `review::CritiquePass {
position, attempt, verdict, findings }`, the shape `Critique` has today (`review.rs:493-502`);
`critique_of` and `read_spawned_critique` return it. `review::Critique` becomes the record of every
pass of the hash, holding `Vec<CritiquePass>`, and exposes the union of the findings in pass order (each pass's lines in line order),
the pass count, whether the hash is exhausted, and its DERIVED VERDICT: `reject` when the union
holds a BLOCKING finding, else `approve`. A pass's written verdict remains only the authority's
consistency check (`judged`, `review.rs:531-536`). Finding ids stay `sc-<hash>-<attempt>-<k>`, so an id minted
at a later pass is stable and a `DecisionMade` naming it keeps resolving it. `latest_critique` is
replaced by the union read over the same typed `SpawnResult` read (`critique_results`,
`review.rs:551-557`); `read_critique` returns the union (`None` when the hash has no pass). Two passes
that state one defect are two ids; the union keeps both. `read_spawned_critique` (`review.rs:569-576`,
which reads `latest_critique` today) returns the spawn's own pass, read by `result_of` on the spawn's
id (`critique_spawn_id(hash, attempt)`) and judged by the authority, or why there is none
(`why_no_critique`, `review.rs:581-588`), never the union; this criterion owns that contract at its
landing, and the loop (criterion 2) only calls it.

- *Readers of the union.* `open_findings` (`review.rs:689-716`) returns the union's BLOCKING ids
  minus each id named in the `resolves` of a `DecisionMade` that governs the spec and whose position
  is after the `SpawnResult` of the pass that minted that id (spec 112's "after the critique's
  result", per pass); `copy_findings` (`critique.rs:243-275`) copies every union id the project run
  stream does not yet hold; the verb prints every union finding, `<id> | <summary>`, in pass order,
  then `{"verdict":"<derived verdict>"}`.
- *The verb's surface of the union.* The verb's print of the union, its `copy_findings` call and
  the ANSWERED-FROM LINE are this criterion's; the loop's branches (criterion 2) call them. The
  answered-from line replaces today's `recorded.attempt` line (`cmd_critique`, `critique.rs:108-111`)
  on stderr: `rigger critique: <spec> (hash <hash>): answered from the critique recorded over 1
  pass` for a pass count of 1, and `... recorded over <n> passes` otherwise, n the pass count.

**THE LOOP, decided here (criterion 2).** `cmd_critique` (`critique.rs:67-133`) reads the union of
the hash once the store and graph are open, then:

1. EXHAUSTED: it spawns nothing, prints the answered-from line (criterion 1's), copies and prints
   the union, exits 0.
2. AT THE CEILING (attempt count at or above the ceiling, not exhausted): it spawns nothing. With at
   least one pass it prints the answered-from line (every answered call prints it) and then the
   CEILING NOTICE on stderr, copies and prints the union, exits 0. With no pass it is not answered:
   it prints the CEILING NOTICE naming `no pass recorded` and exits non-zero; raising the key or
   editing the text is the way out. A hash with no pass is never CLEAN and never answered.
3. Otherwise it first runs `copy_findings` over the union read at the call's start, before its
   first spawn, so every recorded pass is copied before any spawn can fail. Then it runs passes:
   each pass parks and spawns the critic through `CriticHost::critique`
   (`critique.rs:156-225`) at `review::next_critique_attempt` (`review.rs:594-605`, the count of
   parked requests for the hash, so an interrupted request still advances it), with the union read
   immediately before that spawn in its prompt; then it judges THAT spawn's own recorded result by
   the authority through `read_spawned_critique` (criterion 1's), never a different attempt's result. Not a pass: the verb errors as today (`critique.rs:219-224`),
   printing nothing, every earlier pass standing and already copied; its attempt still counts. A
   pass: the call re-reads the union and copies its new ids right away through the same
   `copy_findings` call over the re-read union (idempotent by id, `critique.rs:249-260`), so no
   finding waits on the loop's end. Then it checks exhaustion first: it stops when the re-read union
   is exhausted, and otherwise when the attempt count re-read with it is at or above the ceiling, the
   CEILING NOTICE printing only in that second case, never for an exhausted union; both checks
   run against the re-read record, never the call's own pass or a counter, so a pass another call
   recorded in between counts. Otherwise the next pass runs. On a stop it prints the union and exits
   0.

The print, the copy and the answered-from line are criterion 1's; this criterion owns only the
branches above, which call them. Each pass runs the persona's model rung for its attempt
(`AgentDef::model_for_attempt`), as spec 112 decides for every attempt. A spawning
call reaps only the spawns it judged, each by spawn id right after judging it, pass or not: the
spawn's marker file `agent-live/critique-<hash>/<spawn>`, every transcript file
`<spawn>.*.jsonl` under the hash's `agent-stream/critique-<hash>/` directory, and its spawn scratch
directory `agent-scratch/critique-<hash>/<spawn>/`, all three laid out per spawn by
`liveness::scratch_subpath` (`crates/rigger-driver/src/liveness.rs:173`), `<spawn>` being the
sanitized leaf it builds; an empty scratch root reaps nothing, since that function returns `None`
for it. After those three entries it removes each of the hash's `critique-<hash>/` directories
under `agent-live`, `agent-stream` and `agent-scratch` that is then empty; a directory still
holding another spawn's entries stays. That per-spawn reap replaces the every-hash
`remove_critique_scratch` call `CriticHost::critique` makes after `host.spawn` today
(`critique.rs:218`); a spawning call reaps nothing else, at its end or anywhere. When two calls park the same spawn id, the first call to judge the id's
recorded result reaps that id's entries and the other call's still-running critic loses its marker
and scratch; that is accepted, since two calls on one hash are not a supported arrangement. The
outcome is bounded by the per-id pass rule: the id has one real result, the one `result_of` picks,
since the host records a real result through `record_result_unless_ended`, which lands nothing
beside a real result already standing; both calls judge it as their pass, and the cut-short
spawn records nothing that displaces it. The reap of every hash (`remove_critique_scratch`, `critique.rs:233-238`, as
today, no liveness check) runs only on the answered branches 1 and 2, which spawn nothing. So the
loop never widens today's window: a live critic of another hash can be removed only by an answered
call, exactly as today's one reap per call could, and dead scratch of a crashed call is still reaped
by the next answered call on any hash.

This criterion owns the tests the loop changes, in `tests/spec_critique_periphery.rs`:

- `a_text_reverted_to_a_critiqued_hash_is_answered_by_that_critique_with_zero_spawns` (:1011) keeps
  its live `stub(REJECT)` recording under THE SHARED STUB: the first hash runs two passes (the canned
  REJECT at attempt 0, the zero-line approve at attempt 1), the edited text's `stub(APPROVE)` one
  pass, and the reverted call is answered with 0 spawns (`third`), its stdout `reject_out(SPEC, 0)`,
  its copies `reject_copies(SPEC, 0, SPEC_REL)`, its answered-from line `recorded over 2 passes`,
  and `critique_events(root).len()` is 6 (it read 4).
- `a_recorded_critique_whose_copies_were_never_appended_is_completed_with_zero_spawns` (:1107) keeps
  the no-critique result at attempt 1 and seeds a zero-line approve pass at attempt 2 (`APPROVE`):
  the call is answered with 0 spawns, its stdout `reject_out(SPEC, 0)`, its copies
  `reject_copies(SPEC, 0, SPEC_REL)`, its answered-from line `recorded over 2 passes`, and
  `critique_events(root).len()` is 6 (4 seeded plus the 2 of attempt 2; nothing appended).
- `a_launch_that_records_no_result_says_why_and_the_next_call_runs_the_next_attempt` (:1053) uses
  the shared stub: its first call is unchanged (1 event, the request at attempt 0); its second call's
  fresh `stub(REJECT)` answers REJECT at its first spawn (attempt 1) and a zero-line approve at its
  second (attempt 2), so the call exhausts: stdout `reject_out(SPEC, 1)`, copies
  `reject_copies(SPEC, 1, SPEC_REL)`, `critique_stub_spawns(answering.path())` 2 (it read 1), and 5
  critique events, `spawn_events(...)[1..]` the request and result at attempt 1 then at attempt 2.
- `two_requests_parked_under_one_spawn_id_make_the_next_attempt_two` (:1879) is unchanged: its two
  parks and its one pass at attempt 2 bring the attempt count to 3, the ceiling under its workflow
  (`safety::MAX_RETRIES`), so the call stops after that pass with the CEILING NOTICE on stderr.
- THE SHARED STUB. `tests/common/repo.rs::write_critique_stub` (:92, through
  `write_critique_stub_reporting`, :99, and the fixture `tests/fixtures/` critique agent script)
  answers its canned text on its first spawn and a zero-line approve on every later spawn, counted
  by its own `critique-spawns` record, so a call that records a non-exhausting critique stops at its
  next attempt. It writes its argv, task, system-prompt and conn records on its first spawn only,
  so `critique_stub_task`, `critique_stub_record` and `critique_stub_argv`
  (`tests/common/repo.rs:224-237`) read the first spawn of their stub directory; every later spawn
  still reads its stdin line, counts itself and replays the zero-line approve, and a later pass's
  prompt is read from its `SpawnRequested` `prompt` on the critique stream. Every test that records
  a non-exhausting critique through it or through `record_critique` (`tests/common/cli.rs:658`) is
  this criterion's, its id and output assertions kept (except the two verdict lines criterion 1
  moves to the derived reject, see Notes; their id assertions stay), its task, model, conn,
  system-prompt and argv assertions reading the first spawn unchanged, and its spawn-count,
  event-list and event-count assertions taking the one exhausting pass:
  - `tests/spec_critique_periphery.rs`: `a_critique_launch_carries_the_spawn_hooks_on_its_own_settings`
    (:168), `a_critics_stop_failure_hook_leaves_its_record_on_the_run_under_its_own_spawn` (:204),
    `a_spec_is_critiqued_once_per_text_and_answered_from_the_store_after`
    (:235), `under_a_workflow_with_no_critic_the_verb_refuses_naming_both_keys_and_records_nothing`
    (:426), `a_project_with_no_git_repository_critiques_its_spec_against_the_project_root` (:486),
    `a_result_that_is_no_critique_exits_non_zero_saying_why_and_the_next_call_spawns_attempt_one`
    (:699), `the_verb_takes_the_store_flags_rigger_run_takes_and_refuses_malformed_arguments` (:759),
    `the_critique_record_persists_under_the_literal_content_hash_with_the_critics_request` (:895),
    `a_reject_needs_a_blocking_line_and_an_approve_beside_one_is_a_critique_that_still_blocks` (:1155),
    `the_critic_is_the_plan_critique_gates_adversary_else_the_default_review_adversary` (:1207),
    `a_graph_that_owes_its_rebuild_refuses_every_call_answered_or_not_naming_rigger_setup` (:1427),
    `a_server_selected_by_flags_alone_is_handed_to_the_critic_whose_bound_server_reads_it` (:1504),
    `a_store_from_before_the_minted_identity_is_migrated_before_the_verbs_first_append` (:1584),
    `a_result_recorded_first_for_the_critics_spawn_id_is_read_back_never_the_sessions_own` (:1764),
    `a_project_with_no_git_repository_removes_no_critique_directory_under_its_root_or_tmpdir` (:1960),
    `a_configured_server_and_a_flagged_one_over_the_ambient_are_both_handed_to_the_critic` (:2001);
  - `tests/new_run_critique_refusal_periphery.rs`:
    `a_new_step_refuses_until_every_blocking_finding_of_its_text_is_resolved` (:226),
    `a_plain_step_of_a_held_run_prints_the_hold_whatever_the_critique_and_fresh_keeps_its_refusal`
    (:332),
    `an_approving_critique_holding_a_blocking_finding_refuses_a_new_run_on_that_finding` (:567),
    `an_operator_clears_a_refusal_by_running_the_commands_it_prints_as_printed` (:713),
    `a_renamed_spec_keeps_the_critique_of_its_bytes_but_not_its_old_paths_resolutions` (:763),
    `a_resolution_stands_when_a_later_decision_supersedes_it_without_resolves` (:786),
    `each_new_run_follows_the_workflows_critic_and_any_critic_reads_the_recorded_critique` (:809),
    `every_run_entry_adopts_the_specs_run_uncritiqued_and_begins_a_new_one_only_on_a_clean_critique`
    (:931).

  `tests/plan_critique_spec_defect_stop_periphery.rs` records only through `record_clean_critique`
  (:475; `tests/common/cli.rs:678`), a zero-line approve that exhausts at its first pass, and is
  unchanged. Every other test that constructs the stub spawns nothing (it asserts zero spawns, or
  refuses first) and is unchanged.

Spec 112's ANSWERED FROM THE STORE holds for an
exhausted hash and for one at the ceiling; an unexhausted hash below the ceiling with recorded passes
continues at its next attempt. There is still no flag to force a re-critique of an exhausted text.

**THE CEILING JOINS max_retries, decided here (criterion 2).** The ceiling is the effective
`max_retries` of the stage the critic is read from, and that rule has one spelling: a new domain
function `pub fn stage_max_retries(stage: Option<&Stage>, defaults: &Defaults) -> (u32,
RetriesSource)` in `crates/rigger-domain/src/blocker.rs`, beside `effective_max_retries`
(`blocker.rs:173-179`), with `pub enum RetriesSource { Stage, Defaults, BuiltIn }` beside it: the
stage's `max_retries` (`config.rs:884-890`) with `Stage` when the stage is given and its value is
non-zero, else `defaults.max_retries` (`config.rs:779-780`) with `Defaults` when non-zero, else
`safety::MAX_RETRIES` with `BuiltIn`, the bound itself read through `effective_max_retries`. `Stage`
(`config.rs:838`) and `Defaults` (`config.rs:748`) are in the same crate's `config` module, which
does not import `blocker`; `blocker.rs` gains `use crate::config::{Defaults, Stage}`, as `wave.rs:6`
already imports from `crate::config`. Its three readers: `wave::critic_of`; `RunCtx::max_retries_for`'s plain bound
(`crates/rigger-conductor/src/conductor.rs:4083-4094`, the bound of `stage_max_retries(Some(st),
defaults)`, the resume widening kept); and `RunCtx::max_retries` (`conductor.rs:4063-4070`, which
spells the `0 -> safety::MAX_RETRIES` rule inline today), the bound of `stage_max_retries(None,
defaults)`. The conductor lift is part of the ceiling's single spelling and is owned by criterion 2's
unit; criteria 1 and 3 do not touch it. The reader behind the `reject-recurrence #n/max` line
(`blocker::from_events` and `from_state`, `blocker.rs:302-323`, into `classify`, `:244`) has no
stage in scope, one `max` for every unit from `defaults.max_retries` alone, so it shows the value of
`stage_max_retries(None, defaults)`, today's value, through `effective_max_retries`, the function
that bound is read through. A `blocker.rs` test pins `stage_max_retries` for each `RetriesSource`
case. The conductor's two readers stay covered by the existing escalation-depth tests in
`crates/rigger-conductor/src/conductor.rs`: `a_stages_own_max_retries_overrides_the_run_default_for_its_units`
(:33932, `max_retries_for` over a stage override and an unset stage),
`max_retries_for_widens_only_the_resumed_unit_never_a_sibling` (:33896),
`a_higher_max_retries_gives_more_attempts_before_escalation` (:33690) and
`an_absent_max_retries_preserves_the_default_bound_of_three` (:33754), the last two driving
escalation through `RunCtx::max_retries` from `defaults.max_retries` set and unset. One new `wave::critic_of(&Workflow) -> Option<Critic>`,
`Critic { persona, gate, ceiling, source }`, keeps the persona selection `wave::critic`
(`wave.rs:214-229`) has at the launch base, so the branch that picks the critic decides the ceiling
once: when the persona is the plan-critique gate's adversary (`critique_gate_name`,
`wave.rs:195-203`), `gate` is that stage's name and the ceiling and source are
`stage_max_retries(Some(<that stage>), defaults)`; when the persona is the first adversary step of
`defaults.review`, `gate` is `None` and they are `stage_max_retries(None, defaults)`.
Criterion 2's unit moves both callers of `wave::critic`, `cmd_critique` (`critique.rs:72`) and
`refuse_unless_clean` (`run.rs:1991`), to `critic_of` and deletes `wave::critic`, its tests
(`wave.rs:555-644`) moving to `critic_of`. In this repository the ceiling is 6
(`.rigger/workflow.yml:52`; `plan-critique` at :246-249 sets none). Sharing
`stages.plan-critique.max_retries` is intended: the DAG critique's remediation depth and the spec
critique's depth move together under one key. No new key, no new constant. The ceiling bounds the
depth of the critique only; whether a run may start is the BLOCKING count's (CLEAN below). The
CEILING NOTICE, one stderr line, names its bound from `source`: `the ceiling of
stages.<gate>.max_retries`, `the ceiling of defaults.max_retries`, or `the built-in bound of <n>`,
written <bound> below. For a hash with at least one pass, with open BLOCKING ids (`open_findings`): `rigger critique: <spec> (hash
<hash>): not exhausted after <n> attempts, <bound>: open BLOCKING findings: <ids>: amend the spec or
record a DecisionMade resolving them`; with none: `rigger critique: <spec> (hash <hash>): not
exhausted after <n> attempts, <bound>: no open BLOCKING finding, the critique is clean at the
ceiling`; for a hash with no pass: `rigger critique: <spec> (hash <hash>): not exhausted after <n>
attempts, <bound>: no pass recorded: raise the ceiling or edit the text`; n is the attempt count. A ceiling below the attempt count reads as reached. Because
the ceiling counts attempts, a critic that fails the authority on every call still reaches it; from
there a hash with at least one pass is CLEAN or refused by arm (ii), and a hash with no pass is
refused by arm (iii) until the ceiling is raised or the text edited.

**THE PASS FRAMING IS CONTRACT, decided here (criterion 2).** `spec_critique_prompt` gains the
recorded findings as an argument. With none, its output is byte-identical to the function's output
at the launch base, so pass 1's prompt is unchanged. With some, it inserts, after
`CRITIQUE_OUTPUT_CONTRACT` and before the line `The spec under critique, <path>, verbatim:`, the
constant `CRITIQUE_RECORDED_FINDINGS` followed by one line per union finding, its `summary()`
(`review.rs:447-458`) in pass order, then a blank line. Before the text and not after it: the spec
text stays the prompt's verbatim tail, the shape spec 112's prompt test pins (`review.rs:1124`). The constant is a `pub
const` beside `CRITIQUE_OUTPUT_CONTRACT` in `crates/rigger-domain/src/review.rs`, CONTRACT under spec
121's classes (it extends the output the verb parses), its exact text:

    The findings already recorded for this text follow, one per line as <severity> | <location> | <reading> | <fix>. A finding among these lines is recorded and is not reported again: report only findings not among them, and when there is none to add, write no finding line and the verdict line {"verdict":"approve"}.

It carries no method: nothing on how to look or what to hunt. It holds no phrase of spec 121's
phrase list and no MARKERS hit (spec 120's rule).

**CLEAN MEANS EXHAUSTED, decided here (criterion 3).** CLEAN is the hash has at least one pass AND
(it is exhausted OR its attempt count is at or above the ceiling) AND `open_findings` over the union
is empty; a hash with no pass is never CLEAN: the BLOCKING count
decides, the ceiling bounds depth only, so a hash at the ceiling whose union holds only NON-BLOCKING
ids proceeds. `new_run_refusal` (`src/cli/run.rs:2036-2056`) has two arms today, both printing the
line `refusing to begin a new run on <spec>: <why>` and the route `amend the spec and critique it:
rigger critique <spec>`: `None` (no critique) prints the why `not critiqued` and the route `or, once
critiqued, record a resolution: rigger emit DecisionMade '{...,"resolves":[<ids>],...}'`;
`Some(ids)` prints the why `open BLOCKING findings: <ids>` and the route `or record a resolution:
rigger emit DecisionMade '{...,"resolves":<ids>,...}'`. `RunStartCritique::refuse_unless_clean`
(`run.rs:1976-2007`) reads the ceiling from the `wave::critic_of` read criterion 2 put there and
changes only its arms, never that read. A spec outside the repository (`normalize_spec_path`
returns `None`, `run.rs:1989`, so `open` is `None`, `run.rs:1999-2002`) is refused as today, why
`not critiqued` and both of today's routes, before the arms. Then three arms in this order: (i)
CLEAN: the run proceeds; (ii) the union holds an open BLOCKING id: refused on those ids through the
ids arm, printing the route `or record a resolution` only when the hash is exhausted or at the
ceiling, and otherwise only `amend the spec and critique it: rigger critique <spec>`; arm (ii) is
reached first for every hash holding an open BLOCKING id,
at any attempt count, exhausted or not, at the ceiling or not; (iii) otherwise (no pass at any attempt
count, or no open BLOCKING id, not exhausted and below the ceiling):
refused through the no-critique arm, its why `not critiqued to exhaustion` (it read `not
critiqued`), its one route `amend the spec and critique it: rigger critique <spec>`, label kept,
which continues the passes on unchanged text below the ceiling; for a hash with no pass at the
ceiling the route is the same, the amended text being a new hash, while `rigger critique` on
unchanged text prints the no-pass CEILING NOTICE. That arm lists no ids, since in arm (iii) none is
open, and drops the clause `or, once critiqued, record a resolution`, a route that belongs to arm
(ii). A command adopting the spec's existing run proceeds as today,
whatever the critique holds.

**NOTHING MOVES IN THE PERSONA, decided here.** `.rigger/agents/adversary.md` and the scaffold's
adversary template (`src/cli/setup.rs`) are not edited by this spec. The persona's Exhaustion item
(`.rigger/agents/adversary.md:44`) speaks of the critic's own passes within one session, reported
in its one final message; `CRITIQUE_RECORDED_FINDINGS` speaks of the recorded findings of earlier
sessions over the same text; the two do not conflict.

**CONSTRAINTS WALK, decided here.**

| Mechanism | Corner | Decision |
| --- | --- | --- |
| loop | empty | a pass 1 with zero finding lines exhausts the hash: one spawn, as today for a clean text |
| loop | repeated | a call on an exhausted hash spawns nothing; a call at the ceiling spawns nothing and prints the notice |
| loop | reverted | a text reverted to earlier bytes has the earlier hash: its record is read, exhausted or continued |
| loop | DROPPED | a recorded pass is never removed by this spec; a store reset that drops the critique stream while the run stream keeps the copies leaves the hash with no pass, and its new pass 1 can mint ids the run stream already holds, whose copies are then skipped; that is an operator act today and this spec changes nothing about it |
| loop | concurrent calls on two hashes | the two calls share no record but share scratch: a spawning call reaps only the spawns it judged, by spawn id, and then only its own hash's emptied directories, and nothing at its end; only an answered call runs the reap of every hash, and that reap meets a live spawn of another hash as today's does (no liveness check); the loop never widens that window |
| loop | concurrent | two calls on one hash may park the same attempt id; the host's `record_result_unless_ended` lands no second real result for one id, both calls judge that id's one real result as one pass, and the doubled park advances the attempt count, consuming one more attempt toward the ceiling (attempt numbers need not be contiguous); each call re-reads the union after its pass, so it stops once the other call's pass exhausted the hash or brought the attempt count to or above the ceiling; passes recorded after exhaustion join the union, and the hash stays exhausted; a spawning call reaps only the spawns it judged, by spawn id; when both calls park one spawn id, the first to judge its result reaps that id's entries and the other call's still-running critic loses its marker and scratch, accepted since two calls on one hash are not a supported arrangement, the id's one real result (`result_of`) being the pass both calls judge; an answered call's reap of every hash meets a live spawn of the same hash as today's does |
| loop | crash-resume | a call ended mid-pass leaves a request with no result: not a pass; the next call spawns at the next attempt with the union so far; a call ended between passes resumes at the next attempt on the next call |
| loop | cold start | no critique stream: pass 1 |
| loop | existing data | a hash recorded before this spec with one non-empty pass (an approve carrying NON-BLOCKING lines, such as spec 121's committed approval, or a reject) is unexhausted: its next call continues at its next attempt with that pass's findings in the prompt; a recorded pass with zero lines is already exhausted |
| loop | pass fails the authority | not a pass and not exhaustion: the verb errors as today, earlier passes stand, and the failed attempt counts toward the ceiling, so a critic that fails on every call reaches it |
| ceiling | zero passes | every attempt brought no pass: the verb is not answered, prints the CEILING NOTICE naming `no pass recorded` and exits non-zero; a new run is refused by arm (iii); raising the key or editing the text is the way out |
| ceiling | union with zero BLOCKING | clean at the ceiling: the notice says so and a new run proceeds; a union with an open BLOCKING id at the ceiling is refused on those ids (arm ii) |
| ceiling | key raised or lowered between calls | read on every call from the loaded workflow; raised, the next call continues; lowered to or below the attempt count, it reads as reached |
| union | existing resolutions | a `DecisionMade` recorded after a pre-spec pass's result resolves that pass's ids as before; one recorded before a later pass resolves nothing of that later pass |
| union | existing copies | two calls on one hash can each append a `ReviewFinding` for the same id; duplicate copies are tolerated and harmless, since the graph fold's `TYPE_REVIEW_FINDING` arm (`crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:4656-4690`) upserts the finding node by (id, project) (`ensure_node`, :6354) and its ABOUT edges live-upsert (`add_edge`, :6596); a spawning call first copies the union read at its start, so a pass an earlier call recorded but never copied is copied before any spawn can fail; then each judged pass is copied right after it, by id: `ReviewFinding` copies already on the run stream are skipped, only that pass's new ids are appended, and a call that errors between passes leaves every judged pass copied |
| refusal | adopted run | an adopted run is never refused, so runs live at the launch resume unaffected |
| refusal | a critique call on the same hash in flight | the refusal reads the record as stored at its moment, the attempt count counting parked requests, so a new run begun while that call's last attempt is parked can be CLEAN before that pass's lines land; accepted, since beginning a run on a text while critiquing it is not a supported arrangement, as two calls on one hash are not |

## Notes (non-criteria)

OUT OF SCOPE: the in-run DAG plan-critique (the adversary and adjudicator over the planner's DAG,
`.rigger/workflow.yml:246-249`; its reject loop is the planner's, bounded by the same
`max_retries`); the persona text; the critic's tools and denied tools (`review.rs:352-386`); the
preflight skill's Step 4 text (`crates/rigger-domain/src/docs.rs:574-589`, "until it returns no
BLOCKING finding", which stays true).

RENDERED AND HELP TEXT the units change (each by the unit owning the behavior, derived pages
rendered with the tree's own binary, never the installed one):

- criterion 2: the preflight skill's Step 3 sentence (`docs.rs:566-569`, "records its findings and
  verdict keyed on the text's content hash, and prints them; unchanged text is answered from the
  record") becomes: it runs the critic over the text pass after pass until a pass adds no finding
  (or the ceiling, the critique stage's effective `max_retries`), records each pass keyed on the
  text's content hash, and prints the union; an exhausted text is answered from the record. The
  rendered copy `skills/spec-preflight/SKILL.md` and the project's installed copy of that skill are
  re-rendered. The `rigger critique` help entry (`src/main.rs:379-385`) says the same in its column,
  keeping its first line and naming no `defaults.review.adversary`; the block pin
  `the_critique_help_names_an_adversary_step_of_either_panel_form`
  (`tests/spec_critique_periphery.rs:1637`) is re-pinned to it, and the first-line pin
  `critique_is_a_known_command_with_its_usage_line` (:1663) is unchanged. The module doc
  (`critique.rs:1-5`), `cmd_critique`'s doc (`critique.rs:57-66`) and the doc link to
  `wave::critic` on `RunStartCritique` (`run.rs:1954`) follow.
- criterion 1: the docs of `Critique`, `latest_critique`'s replacement, `read_critique`,
  `read_spawned_critique` and `open_findings`, the answered-from wording pins
  (`tests/spec_critique_periphery.rs:1038-1039`, `:1142`), and the two pins of a written approve beside a
  BLOCKING line, which move to the derived `{"verdict":"reject"}` since the printed verdict line is
  the derived verdict: `a_reject_needs_a_blocking_line_and_an_approve_beside_one_is_a_critique_that_still_blocks`
  (`tests/spec_critique_periphery.rs:1188-1194`, `"{} | {summary}\n{{\"verdict\":\"approve\"}}\n"`)
  and `an_approving_critique_holding_a_blocking_finding_refuses_a_new_run_on_that_finding`
  (`tests/new_run_critique_refusal_periphery.rs:572-574`, `out.ends_with("\n{\"verdict\":\"approve\"}\n")`).
  No other test asserts a printed approve beside a BLOCKING line.
- criterion 2 (tests): the three spec-112 tests, the shared stub and every test THE LOOP names.
- criterion 3: the docs of `refuse_unless_clean`, `open_ids` and `new_run_refusal`, and only the wording pins of
  the `not critiqued` why (`tests/common/cli.rs:709`, `tests/new_run_critique_refusal_periphery.rs`,
  `tests/plan_critique_spec_defect_stop_periphery.rs:467`), and the reshaping of the
  `refused_new_run` oracle (`tests/common/cli.rs:706-727`, today two shapes on `Option<&[String]>`)
  into one variant per refusal shape: outside the repository; arm (ii) exhausted or at the ceiling;
  arm (ii) below the ceiling; arm (iii).

`docs/audit/` holds dated audit snapshots and is not edited.

UNIT ORDER (the needs chain): 1, then 2 (needs 1), then 3 (needs 1 and 2: its CLEAN reads the
ceiling from `wave::critic_of`, criterion 2's), then 4.

LANDING-ORDER SIMULATION:

- 1 before 2: `cmd_critique` still makes one pass per uncritiqued hash, judged by
  `read_spawned_critique`'s spawn-own pass, and answers any recorded hash from the store with the
  answered-from line over its pass count; it prints and copies the union of what is recorded -> holds.
- 1 before 3: the refusal reads `open_findings` over the union, exhaustion not yet required ->
  holds.
- 2 before 3: 2 edits `critique.rs`, `review.rs`'s prompt, `wave.rs`, `blocker.rs`,
  `conductor.rs` and the refusal's critic read in `run.rs`
  (`run.rs:1991`); 3 edits the refusal's arms in `run.rs` and its tests. On the tree with 1 and 2,
  the refusal reads `critic_of` and still applies today's clean rule, an empty `open_findings` over
  the union at any attempt count -> holds. A tree with 3 and not 2 does not exist: 3 reads
  `critic_of`'s ceiling.

TEST PLACEMENT: criterion 1 in `review.rs`'s test module over recorded `SpawnResult` fixtures
(`result_at`); criterion 2 in `tests/spec_critique_periphery.rs` with a stub agent binary first on
`PATH` whose output is chosen per attempt, plus `wave.rs`'s, `blocker.rs`'s and `review.rs`'s test
modules for `critic_of`, `stage_max_retries` with its `RetriesSource` (the conductor's two readers under
the escalation-depth tests THE CEILING names), the three re-seeded spec-112 tests and the prompt; criterion 3 in `tests/new_run_critique_refusal_periphery.rs`.

## Global constraints

- Hyphens, never em or en dashes, in every added line.
- No new event type, no new workflow key, no new crate dependency, no size limit.
- Every string the binary puts before the critic is contract or framing; the method stays in the
  persona.
- The operator's installed rigger binary is never replaced or modified by any unit.

## Done when

- [ ] a test proves THE CRITIQUE IS THE UNION OF ITS PASSES: `review::read_critique` returns every recorded pass of the hash with its finding ids unchanged, its verdict derived from the union. This criterion OWNS `Critique`, exhaustion, the union readers, `read_spawned_critique`'s spawn-own pass and the answered-from line; the loop and the conductor lift are criterion 2's and the refusal 3's, NOT this one's.
- [ ] a test proves THE CRITIQUE PASSES TO EXHAUSTION: `cmd_critique` spawns the critic again, the recorded findings in its prompt, until a pass carries no finding line or the `wave::critic_of` ceiling is reached, asserted with a stub agent on `PATH`. This criterion OWNS the loop, the ceiling with its single spelling (the conductor lift included) and `CRITIQUE_RECORDED_FINDINGS`; the union is criterion 1's and the refusal 3's, NOT this one's.
- [ ] a test proves A NEW RUN NEEDS AN EXHAUSTED CRITIQUE: `RunStartCritique::refuse_unless_clean` refuses a new run whose hash holds no open BLOCKING finding and is neither exhausted nor at the ceiling with the not-critiqued `new_run_refusal`. This criterion OWNS that requirement and the refusal's arms; `open_findings` is criterion 1's and the critic read and the conductor lift 2's, NOT this one's.
- [ ] `.rigger/gates/lanes.sh` runs green for both feature lanes on the finished tree, verified by `sh .rigger/gates/lanes.sh no-default` and `sh .rigger/gates/lanes.sh core` each exiting 0. This criterion OWNS only the lanes; every behaviour is criteria 1 to 3's, NOT this one's.
