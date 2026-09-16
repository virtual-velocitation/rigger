# Simplification Audit - 2026-09

The audit report spec 85 derives the follow-up refactoring specs from. Six sections, each claim citing `file:line`; see `specs/85-simplification-audit.md` for scope.

## 1. Responsibility Map

Every function in `src/conductor.rs`, `src/main.rs` and `src/dash.rs` (1670 functions total), assigned to a proposed module by `tests/simplification_audit.rs`'s deterministic scanner + rule-table classifier (never by hand). Instrument: the brace-matching scanner over the three named files.

### Proposed module tree

- `conductor::budget` (5 functions)
  - `src/conductor.rs:359-361` `compensation_queued_key` - name contains "compensat" (budget accounting); grouped under `conductor::budget`.
  - `src/conductor.rs:966-995` `pending_compensations_from_log` - name contains "compensat" (budget accounting); grouped under `conductor::budget`.
  - `src/conductor.rs:1492-1496` `budget_refused` - name contains "budget" (budget accounting); grouped under `conductor::budget`.
  - `src/conductor.rs:1501-1503` `is_budget_refused` - name contains "budget" (budget accounting); grouped under `conductor::budget`.
  - `src/conductor.rs:12333-12343` `mutation_scratch_settled` - name contains "mutation" (budget accounting); grouped under `conductor::budget`.
- `conductor::emit` (2 functions)
  - `src/conductor.rs:389-391` `quarantine_record_key` - name contains "record" (event/decision emission); grouped under `conductor::emit`.
  - `src/conductor.rs:12152-12185` `recorded_adoption` - name contains "record" (event/decision emission); grouped under `conductor::emit`.
- `conductor::error` (3 functions)
  - `src/conductor.rs:719-721` `from` - method inside `impl From<crate::eventstore::Error> for Error`; grouped with its other `Error` methods.
  - `src/conductor.rs:724-726` `from` - method inside `impl From<crate::worktree::Error> for Error`; grouped with its other `Error` methods.
  - `src/conductor.rs:729-731` `from` - method inside `impl From<serde_json::Error> for Error`; grouped with its other `Error` methods.
- `conductor::gate` (19 functions)
  - `src/conductor.rs:325-327` `gate_verdict_key` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:337-339` `gate_skip_key` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:350-352` `postmerge_gate_verdict_key` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:399-406` `gate_intersects_radius` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:569-571` `unit_of_gate_key` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:579-583` `gate_key_attempt` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:650-680` `recorded_gate_outcome` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:685-687` `deferred_gate_verdict_key` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:696-698` `deferred_gate_failed_key` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:1607-1617` `verdict_channel_mismatch` - name contains "verdict" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:1622-1624` `is_verdict_channel_mismatch` - name contains "verdict" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:10779-10786` `with_gate_hold` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:10914-10921` `gate_failure_cause` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:10933-10935` `verdict_approves` - name contains "verdict" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:10951-10960` `last_verdict` - name contains "verdict" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:10967-10969` `has_verdict_line` - name contains "verdict" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:10976-10978` `emitted_verdict_approves` - name contains "verdict" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:10988-11000` `verdict_compensates` - name contains "verdict" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:12618-12626` `critique_gate_name` - name contains "gate" (gate execution); grouped under `conductor::gate`.
- `conductor::gate_ratchet` (1 function)
  - `src/conductor.rs:879-885` `for_persistent_failure` - method inside `impl GateRatchet`; grouped with its other `GateRatchet` methods.
- `conductor::ground` (1 function)
  - `src/conductor.rs:11471-11480` `graph_around_recovery` - name contains "graph" (grounding integration); grouped under `conductor::ground`.
- `conductor::integration_approval` (1 function)
  - `src/conductor.rs:1208-1210` `approved` - method inside `impl IntegrationApproval`; grouped with its other `IntegrationApproval` methods.
- `conductor::prior_failure` (3 functions)
  - `src/conductor.rs:1234-1239` `is_empty` - method inside `impl PriorFailure`; grouped with its other `PriorFailure` methods.
  - `src/conductor.rs:1243-1261` `summary` - method inside `impl PriorFailure`; grouped with its other `PriorFailure` methods.
  - `src/conductor.rs:1266-1313` `block` - method inside `impl PriorFailure`; grouped with its other `PriorFailure` methods.
- `conductor::review` (9 functions)
  - `src/conductor.rs:508-555` `route_review_tier` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `src/conductor.rs:1561-1574` `degenerate_reviewer` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `src/conductor.rs:1579-1581` `is_degenerate_reviewer` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `src/conductor.rs:10870-10879` `review_evidence` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `src/conductor.rs:11079-11086` `review_protocol` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `src/conductor.rs:12237-12242` `review_worktree_dir` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `src/conductor.rs:12249-12251` `review_branch` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `src/conductor.rs:12724-12726` `review_roster` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `src/conductor.rs:12732-12738` `adjudicator_roster` - name contains "adjudicat" (review orchestration); grouped under `conductor::review`.
- `conductor::review_outcome` (2 functions)
  - `src/conductor.rs:911-918` `approved` - method inside `impl ReviewOutcome`; grouped with its other `ReviewOutcome` methods.
  - `src/conductor.rs:919-926` `rejected` - method inside `impl ReviewOutcome`; grouped with its other `ReviewOutcome` methods.
- `conductor::run` (6 functions)
  - `src/conductor.rs:11866-11868` `unit_branch` - name contains "unit" (unit run loop); grouped under `conductor::run`.
  - `src/conductor.rs:11878-11884` `unit_worktree_dir` - name contains "unit" (unit run loop); grouped under `conductor::run`.
  - `src/conductor.rs:12526-12539` `stale_units_from_log` - name contains "unit" (unit run loop); grouped under `conductor::run`.
  - `src/conductor.rs:12561-12599` `stale_downstream_units` - name contains "unit" (unit run loop); grouped under `conductor::run`.
  - `src/conductor.rs:12633-12651` `unit_slug` - name contains "unit" (unit run loop); grouped under `conductor::run`.
  - `src/conductor.rs:12661-12700` `baseline_units` - name contains "unit" (unit run loop); grouped under `conductor::run`.
- `conductor::run_ctx` (126 functions)
  - `src/conductor.rs:2938-2940` `emit` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:2949-2964` `append_and_fold` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:2976-2998` `append_and_fold_batch` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3003-3009` `emit_with_actor` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3019-3027` `emit_meta` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3040-3042` `emit_keyed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3053-3078` `emit_keyed_meta` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3113-3152` `emit_keyed_batch` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3161-3167` `agent_model` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3182-3184` `recorded_gate_verdict` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3194-3196` `cached_green_verdict` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3203-3205` `is_stale` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3230-3241` `effective_attempts` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3252-3260` `gate_verdict_high_water` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3268-3282` `mark_stale_downstream` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3298-3376` `drain_compensations` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3389-3438` `commits_to_compensate` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3448-3479` `emit_gate_skip` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3496-3555` `emit_gate_verdict` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3563-3570` `max_retries` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3583-3594` `max_retries_for` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3620-3626` `budget_tripped` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3633-3635` `spawn_is_recorded` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3651-3674` `reserve_spawn` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3677-3679` `budget_broke` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3685-3687` `parked` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3693-3695` `manual_review_pending` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3700-3702` `budget_halted` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3711-3741` `trip_budget_breaker` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3752-3762` `halt_reason` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3767-3777` `check_coverage_or_flag` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3779-3898` `run_wave` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3907-3941` `partition_wave` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3946-3956` `partition_requested` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3961-3985` `run_batch` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3987-4040` `start_and_run_stage` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4042-4150` `run_stage` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4155-4172` `stage_paused_for_review` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4178-4184` `effective_review_panel` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4196-4203` `select_review_panel` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4215-4237` `log_review_tier` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4251-4269` `assert_isolated_cwd` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4284-4340` `reviewer_spawn_opts` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4410-4528` `review_unit` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4565-4626` `spawn_sdet_author` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4632-5369` `run_single_stage` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:5375-5382` `effective_speculation_width` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:5391-5398` `speculates` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:5407-5423` `speculation_lane_worktree` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:5444-5804` `run_speculation` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:5817-5896` `emit_speculation_winner_status` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:5915-5952` `record_speculation_reject` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:5962-5995` `cancel_speculation_candidates` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:5997-6050` `run_fan_out_stage` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6060-6226` `run_fan_out_review_loop` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6257-6309` `run_review_agents_concurrently` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6321-6352` `run_lens` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6391-6531` `run_reviewer` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6567-6584` `gating_spawn_emitted_approve` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6606-6616` `review_spawn_errored` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6640-6673` `reviewer_result_is_degenerate` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6688-6718` `run_adversary` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6736-6773` `run_adjudicator` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6795-6816` `dag_unit_blast_radii` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6825-6904` `build_dag_critique_prompt` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6913-7004` `re_plan` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7015-7048` `run_plan_critique_gate` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7058-7224` `plan_critique_loop` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7252-7263` `build_env` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7280-7287` `run_base_env` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7312-7318` `spawn_env` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7329-7334` `build_budget` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7359-7367` `shared_build_cache_paths` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7378-7621` `run_gates` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7637-7728` `run_gate_with_taxonomy` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7785-7792` `gc_integrated_branches` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7799-7902` `gc_integrated_branches_logged` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7938-7945` `reclaim_terminal_unit_mutation_scratch` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7975-8161` `run_deferred_gates` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:8172-8239` `record_gate` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:8297-8738` `integrate_and_emit` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:8780-8787` `integrate_plan_commits` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:8789-8909` `integrate_plan_commits_inner` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:8920-8936` `record_plan_intent` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:8947-8975` `read_plan_landed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:8983-9012` `record_plan_landed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9017-9023` `regenerate_rule_for` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9048-9084` `run_regenerate_command` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9103-9128` `regenerate_conflicted_paths` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9156-9170` `catch_up_owed_regeneration` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9175-9182` `regenerate_pending_for` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9193-9198` `clear_regenerate_pending` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9203-9209` `pending_landing_for` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9214-9219` `clear_pending_landing` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9226-9234` `union_regenerate_pending` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9261-9289` `record_regenerate_pending` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9298-9312` `record_placeholder_staged` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9322-9339` `record_regenerate_commit` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9349-9363` `record_merge_attempt` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9369-9388` `record_merge_outcome` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9395-9410` `record_landing_intent` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9415-9423` `record_landed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9434-9452` `record_integrate_row` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9466-9520` `spawn_conflict_resolution_implementer` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9529-9531` `build_system_prompt` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9541-9553` `ground_query` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9559-9569` `implementer_agent` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9573-9597` `plan_protocol` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9625-9659` `grounded_blast_radius` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9675-9688` `grounded_seed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9712-9741` `record_blast_radius` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9743-9749` `build_prompt` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9760-9762` `build_review_prompt` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9770-9804` `build_prompt_with_failure` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9806-9870` `graph_context` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9883-9891` `ingest_project_into_graph` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9899-9958` `ingest_project_batches` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9963-9963` `ingest_project_into_graph` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9965-9980` `emit_lesson` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9985-9991` `agent_isolated` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10114-10222` `adopt_prior_criterion_branch` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10244-10285` `resume_phase` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10287-10319` `stage_worktree` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10334-10356` `review_only_worktree` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10358-10724` `harvest_proposed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10746-10768` `resolve_served_criterion` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
- `conductor::schedule` (9 functions)
  - `src/conductor.rs:1655-1661` `plan_landing_failed` - name contains "plan" (unit scheduling); grouped under `conductor::schedule`.
  - `src/conductor.rs:1666-1668` `is_plan_landing_failed` - name contains "plan" (unit scheduling); grouped under `conductor::schedule`.
  - `src/conductor.rs:10810-10812` `normalize_criterion_id` - name contains "criterion" (unit scheduling); grouped under `conductor::schedule`.
  - `src/conductor.rs:10831-10836` `criterion_stable_id` - name contains "criterion" (unit scheduling); grouped under `conductor::schedule`.
  - `src/conductor.rs:11982-12042` `prior_criterion_unit` - name contains "criterion" (unit scheduling); grouped under `conductor::schedule`.
  - `src/conductor.rs:12355-12377` `partition_by_blast_radius` - name contains "partition" (unit scheduling); grouped under `conductor::schedule`.
  - `src/conductor.rs:12395-12413` `partition_with_serialize` - name contains "partition" (unit scheduling); grouped under `conductor::schedule`.
  - `src/conductor.rs:12428-12448` `blast_radius_conflicts` - name contains "blast" (unit scheduling); grouped under `conductor::schedule`.
  - `src/conductor.rs:12867-12886` `ready_stages` - name contains "stage" (unit scheduling); grouped under `conductor::schedule`.
- `conductor::spawn` (4 functions)
  - `src/conductor.rs:1454-1458` `parked_spawn` - name contains "spawn" (spawn lifecycle); grouped under `conductor::spawn`.
  - `src/conductor.rs:1463-1465` `is_parked` - name contains "parked" (spawn lifecycle); grouped under `conductor::spawn`.
  - `src/conductor.rs:1517-1519` `is_parked_or_budget_refused` - name contains "parked" (spawn lifecycle); grouped under `conductor::spawn`.
  - `src/conductor.rs:12788-12799` `wave_ready` - name contains "wave" (spawn lifecycle); grouped under `conductor::spawn`.
- `conductor::support` (19 functions)
  - `src/conductor.rs:450-452` `path_is_high_risk` - name contains "is_" (predicate helper); grouped under `conductor::support`.
  - `src/conductor.rs:1046-1079` `conflict_regenerate_pending_from_log` - name contains "from_" (conversion helper); grouped under `conductor::support`.
  - `src/conductor.rs:1104-1143` `pending_landing_from_log` - name contains "from_" (conversion helper); grouped under `conductor::support`.
  - `src/conductor.rs:1153-1184` `integrate_attempted_from_log` - name contains "from_" (conversion helper); grouped under `conductor::support`.
  - `src/conductor.rs:2564-2566` `has_producer` - name contains "has_" (predicate helper); grouped under `conductor::support`.
  - `src/conductor.rs:10795-10797` `normalize_ws` - name contains "normalize" (normalization helper); grouped under `conductor::support`.
  - `src/conductor.rs:11052-11054` `build_system_prompt` - name contains "build" (generic construction helper); grouped under `conductor::support`.
  - `src/conductor.rs:11287-11431` `write_capped_section` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `src/conductor.rs:11482-11538` `write_code_neighborhood` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `src/conductor.rs:11651-11734` `write_design_intent` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `src/conductor.rs:11740-11755` `write_capped_decisions` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `src/conductor.rs:11760-11777` `write_capped_lessons` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `src/conductor.rs:11785-11810` `write_capped_findings` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `src/conductor.rs:11819-11827` `write_peers_pointer` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `src/conductor.rs:11842-11852` `write_lookup_pointer` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `src/conductor.rs:12094-12107` `branch_is_foreign` - name contains "is_" (predicate helper); grouped under `conductor::support`.
  - `src/conductor.rs:12460-12462` `is_fan_out` - name contains "is_" (predicate helper); grouped under `conductor::support`.
  - `src/conductor.rs:12483-12485` `is_producer` - name contains "is_" (predicate helper); grouped under `conductor::support`.
  - `src/conductor.rs:12744-12746` `has_llm_verifier` - name contains "has_" (predicate helper); grouped under `conductor::support`.
- `conductor::tests` (489 functions)
  - `src/conductor.rs:2884-2934` `for_test` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:12945-12964` `unit_worktree_dir_derives_deterministically_from_scratch_root_and_unit_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:12967-13054` `recorded_gate_outcome_reads_the_latest_gate_run_verdict_per_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:12970-12979` `verdict` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13058-13063` `started_with_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13065-13070` `integrated` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13073-13081` `prior_criterion_unit_finds_a_prior_un_integrated_units_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13084-13110` `prior_criterion_unit_never_returns_an_integrated_units_id_and_never_falls_back_to_an_older_sibling` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13113-13137` `prior_criterion_unit_integration_of_one_criterion_never_masks_an_abandoned_sibling_criterion_sharing_the_same_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13140-13153` `prior_criterion_unit_tie_break_prefers_the_most_recent_of_two_non_integrated_priors` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13156-13161` `prior_criterion_unit_excludes_this_unit_itself` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13164-13176` `prior_criterion_unit_ignores_a_different_criterion_and_an_empty_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13183-13188` `run_started_with_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13193-13199` `compensated` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13204-13209` `plain_failure` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13212-13233` `prior_criterion_unit_never_adopts_across_two_different_specs_sharing_the_same_criterion_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13236-13251` `prior_criterion_unit_still_adopts_across_two_runs_of_the_same_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13254-13272` `prior_criterion_unit_readopts_after_a_compensation_reverts_the_integration` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13275-13292` `prior_criterion_unit_a_plain_non_compensation_failure_never_reopens_an_integrated_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13299-13317` `adoption_status` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13320-13345` `recorded_adoption_ignores_a_same_identity_event_carrying_the_wrong_status` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13348-13381` `recorded_adoption_never_answers_for_a_mismatched_criterion_or_a_mismatched_spec_alone` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13384-13389` `branch_owner_returns_none_for_an_id_that_never_started` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13392-13408` `branch_owner_reads_the_most_recent_started_criterion_and_spec_for_this_bare_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13411-13426` `branch_owner_ignores_a_non_unit_started_event_even_when_it_shares_the_id_field` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13429-13442` `branch_is_foreign_is_false_when_nothing_is_recorded_or_everything_matches` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13445-13467` `branch_is_foreign_is_false_when_the_recorded_owner_has_no_criterion_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13470-13492` `branch_is_foreign_when_only_one_axis_differs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13497-13509` `find_unit_started` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13512-13597` `a_fresh_units_own_branch_adopts_a_prior_runs_un_integrated_unit_sharing_the_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13600-13664` `a_fresh_unit_never_adopts_a_criterion_whose_prior_attempt_already_integrated` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13669-13681` `run_git_test` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13684-13819` `a_halted_spawns_uncommitted_tree_is_captured_as_a_wip_commit_and_named_in_the_next_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13831-13846` `prior_failure_summary_names_only_the_halted_commit_when_it_is_the_sole_failure` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13849-13872` `prior_failure_block_names_only_the_halted_commit_when_it_is_the_sole_failure` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13875-13909` `prior_failure_block_adds_the_generic_preamble_for_review_reject_or_contradiction_alone` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13912-13939` `review_worktree_dir_and_branch_derive_from_stage_and_attempt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14051-14079` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14082-14089` `prompts_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14092-14099` `dirs_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14103-14109` `system_prompt_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14113-14115` `title_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14119-14121` `reviews_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14125-14127` `spawn_ids` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14131-14138` `spawn_count` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14142-14148` `spawned` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14154-14161` `dir_existed_when_spawned` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14164-14304` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14307-14312` `agent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14317-14323` `agent_with_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14325-14331` `gate_def` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14336-14342` `gate_def_inputs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14345-14371` `integrates_a_passing_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14374-14398` `coverage_gate_refuses_an_uncovered_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14401-14433` `planner_extends_the_dag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14436-14502` `planner_proposed_unit_with_a_coverage_criterion_runs_and_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14505-14593` `conductor_creates_one_baseline_unit_per_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14596-14667` `a_stage_needing_the_fan_out_template_becomes_ready_once_every_criterion_unit_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14670-14792` `producer_prompt_carries_the_criteria_and_plan_protocol_grounded_on_the_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14797-14799` `baseline_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14804-14831` `supersede_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14834-14883` `a_prior_runs_proposal_never_resurrects_in_a_new_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14886-14968` `planner_unit_supersedes_the_matching_baseline` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14971-15055` `a_planner_supersede_of_a_fan_out_member_still_satisfies_its_downstream_needs_edge` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15058-15094` `a_criterion_with_no_planner_unit_keeps_its_baseline` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15097-15170` `planner_refinement_split_is_still_harvested` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15173-15306` `a_same_id_re_emit_updates_the_proposed_unit_in_place` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15313-15343` `seed_refine_dag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15347-15367` `append_proposals` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15370-15443` `a_coverage_retarget_re_emit_preserves_one_unit_per_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15446-15506` `a_re_emit_naming_a_reserved_stage_never_mutates_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15509-15574` `re_folding_a_same_id_re_emit_is_idempotent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15577-15684` `a_real_split_two_distinct_ids_both_survive_the_fold` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15687-15783` `a_later_episodes_proposal_supersedes_an_earlier_episodes_planner_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15786-15843` `a_same_episode_re_seen_on_a_later_fold_still_never_self_supersedes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15846-15925` `a_planner_proposal_with_no_data_episode_field_still_supersedes_via_meta_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15928-16055` `a_same_id_refine_restamps_its_episode_so_its_own_episodes_sibling_does_not_reap_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16058-16180` `a_same_id_refine_survives_its_own_episodes_sibling_add_walked_first` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16183-16338` `a_resume_catch_up_over_two_episode_supersession_and_a_split_matches_a_live_incremental_fold` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16217-16235` `append_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16237-16249` `shape` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16341-16512` `a_legacy_history_resume_catch_up_matches_a_live_incremental_fold_mutual_siblings_then_superseded` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16362-16379` `append_legacy` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16381-16399` `append_identified` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16401-16413` `shape` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16515-16609` `a_legacy_proposal_never_supersedes_an_identified_episodes_owner_even_when_logged_later` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16612-16711` `a_late_re_emit_never_mutates_a_started_or_terminal_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16717-16726` `has_unmatched_signal` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16729-16785` `a_verbatim_copy_still_supersedes_its_baseline` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16788-16862` `a_paraphrased_proposal_matches_its_baseline_by_id_not_prose` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16865-16949` `a_bracketed_id_echo_with_a_paraphrase_still_supersedes_exactly_once` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16952-17024` `a_stale_non_matching_id_falls_back_to_the_verbatim_prose_match` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17027-17085` `a_genuinely_new_proposal_runs_and_records_an_unmatched_signal` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17088-17219` `resume_dedups_baselines_before_running_them` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17222-17303` `decomposes_the_real_spec_01_into_per_criterion_units` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17306-17344` `ratchet_promotes_a_reliable_gate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17347-17408` `elevated_gate_is_never_promoted_to_silent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17411-17453` `learns_from_escalation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17456-17506` `feeds_graph_decisions_into_the_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17509-17588` `lookup_pointer_names_all_three_verbs_on_every_slice` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17591-17686` `decisions_prompt_injection_is_capped_under_budget_with_elision_note` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17691-17713` `render_capped_decisions` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17716-17746` `a_superseded_decision_never_outranks_a_current_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17749-17802` `grounding_still_surfaces_a_prior_run_decision_that_peers_labels_historical` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17805-17832` `the_verbatim_count_cap_binds_on_many_small_decisions` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17835-17867` `the_byte_budget_cap_binds_before_the_count_on_chunky_decisions` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17870-17937` `a_kept_decision_restores_the_dropped_dependency_it_supersedes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17943-17963` `render_capped_findings` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17966-18033` `findings_prompt_injection_is_capped_under_budget_with_elision_note` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18036-18071` `the_verbatim_count_cap_binds_on_many_small_findings` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18074-18179` `grounding_omits_a_resolved_finding_and_keeps_the_open_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18185-18204` `render_capped_lessons` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18207-18264` `the_injected_slice_is_deduplicated_by_normalized_text` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18267-18409` `dedup_and_restore_are_render_only_no_event_no_projection_mutation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18427-18563` `structural_grounding_is_one_seeded_traversal_over_the_unified_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18580-18695` `the_grounding_path_populates_the_unified_graph_from_the_live_project` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18710-18799` `re_ingesting_re_extracts_a_changed_file_and_skips_unchanged_ones` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18820-18900` `re_excluding_the_same_file_twice_in_one_process_retires_its_middle_generation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18913-19032` `a_second_run_over_an_unchanged_tree_appends_no_derived_index_event` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19063-19069` `spec60_content_identity` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19074-19078` `spec60_guarded_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19091-19119` `spec60_guard_is_judging` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19127-19145` `spec60_cold_rebuild` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19157-19188` `spec60_reachable` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19194-19203` `spec60_reached_entities` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19220-19412` `editing_one_file_between_runs_re_emits_only_that_files_batch_and_supersedes_its_edges` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19428-19605` `a_file_reverted_to_an_earlier_recorded_generation_re_ingests_and_matches_a_cold_rebuild` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19629-19762` `a_prior_runs_non_ingest_replay_key_never_suppresses_this_runs_keyed_emit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19774-19917` `the_ingest_sink_appends_and_folds_once_per_file_batch_never_once_per_event` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19784-19792` `append` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19793-19800` `read_stream` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19801-19808` `read_all` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19809-19815` `subscribe_all` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19816-19822` `subscribe_stream` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19834-19837` `apply` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19838-19841` `apply_batch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19842-19844` `subgraph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19845-19847` `resolve` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19940-20099` `design_intent_grounding_renders_the_governing_rule_and_specifying_ra_by_traversal` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20102-20172` `a_governing_decision_never_leaks_into_the_design_intent_section` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20175-20231` `the_design_intent_section_renders_the_newest_binding_and_elides_the_oldest` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20234-20274` `a_subgraph_with_no_design_intent_renders_no_design_intent_header` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20281-20307` `render_code_neighborhood` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20310-20346` `the_code_neighborhood_elision_note_names_graph_around_recovery_not_peers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20349-20394` `the_code_neighborhood_byte_cap_elides_before_the_count_cap_is_reached` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20397-20427` `a_subgraph_with_no_code_definitions_renders_no_code_neighborhood_header` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20430-20496` `lessons_prompt_injection_is_capped_under_budget_with_elision_note` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20499-20536` `the_verbatim_count_cap_binds_on_many_small_lessons` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20542-20564` `render_capped_lessons_scoped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20567-20604` `lessons_rank_by_blast_radius_relevance_over_pure_recency` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20607-20647` `resume_skips_already_integrated_units` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20651-20668` `seed_events_in_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20671-20725` `replay_trajectory_keeps_only_the_world_inputs_and_restrips_the_run_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20730-20747` `commit_on_unit_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20750-20845` `resume_reuses_a_units_branch_instead_of_reimplementing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20848-20942` `branch_gc_reclaims_integrated_units_and_retains_escalated_ones_on_resume` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20950-20966` `commit_on_named_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20981-21037` `branch_gc_falls_back_to_the_derived_branch_when_unitstarted_recorded_no_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21040-21105` `branch_gc_reclaims_the_recorded_branch_not_the_derived_name_when_they_differ` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21116-21140` `seed_lingering_worktree_on_unit_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21146-21156` `worktree_registered_on` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21159-21256` `branch_gc_removes_a_lingering_worktree_before_reclaiming_the_branch_on_resume` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21259-21351` `branch_gc_reclaims_every_integrated_unit_in_one_resume_not_just_the_first` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21354-21435` `branch_gc_fences_reclaim_behind_an_in_flight_straggler_spawn_and_reclaims_once_it_answers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21438-21505` `gc_integrated_branches_logged_prints_kept_evidence_for_an_in_flight_straggler_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21508-21607` `gc_integrated_branches_logged_prints_removing_evidence_for_a_terminal_spawns_decision` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21610-21693` `gc_integrated_branches_logged_stays_silent_for_an_already_gone_worktree_but_still_reclaims_an_orphaned_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21696-21799` `gc_integrated_branches_logged_does_not_repeat_removing_evidence_once_the_real_removal_already_happened` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21803-21825` `sha_stamp_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21828-21879` `review_boundary_events_carry_the_worktree_sha` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21882-21931` `a_review_reject_unitfailed_carries_the_worktree_sha` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21934-21978` `an_exhaustive_gate_failure_on_an_approved_unit_records_a_gate_cause` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21981-22106` `stamps_the_model_alias_and_resolved_id_on_live_lifecycle_events` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22112-22132` `degenerate_reviewer_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22134-22139` `has_status` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22142-22211` `a_degenerate_adjudicator_result_respawns_and_a_substantive_retry_folds_normally` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22214-22322` `a_review_stage_error_result_re_parks_a_fresh_attempt_no_charge_then_a_real_verdict_folds` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22325-22456` `a_review_spawn_that_errors_on_every_re_parked_attempt_escalates_through_the_bound` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22459-22552` `a_gating_spawn_that_emits_an_approve_verdict_but_returns_no_verdict_line_hard_errors_with_the_result_channel_fix` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22555-22600` `a_gating_spawn_that_returns_a_reject_verdict_line_is_a_normal_reject_even_if_it_emitted_approve` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22603-22647` `a_gating_spawn_with_no_verdict_line_and_no_emitted_approve_is_an_ordinary_reject` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22650-22755` `the_workflow_live_path_correlates_the_approve_by_stamp_not_a_bare_stream_position` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22758-22811` `a_degenerate_lens_result_respawns_the_lens_before_the_review_proceeds` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22814-22874` `a_lens_that_emitted_a_finding_but_reports_empty_stdout_is_not_degenerate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22877-22974` `a_reviewer_that_only_ever_returns_degenerate_output_halts_the_run_loudly_naming_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22977-23062` `resume_integrates_an_already_approved_unit_without_re_reviewing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23065-23176` `a_failed_unit_is_not_terminal_and_resumes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23179-23294` `remediation_attempts_accumulate_across_resume_and_escalate_at_the_bound` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23297-23382` `an_escalated_unit_stays_terminal_on_resume` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23385-23440` `a_fresh_unit_with_no_branch_runs_the_full_lifecycle` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23443-23498` `agent_decision_folds_content_but_no_agent_attribution` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23501-23543` `scope_creep_refuses_a_criterionless_proposed_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23546-23579` `adjudicator_reject_blocks_the_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23582-23648` `adversary_runs_between_the_lenses_and_the_adjudicator` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23651-23701` `adjudicator_reject_gates_even_with_an_adversary_present` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23704-23797` `unit_reviews_itself_within_its_own_lifecycle` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23800-23810` `depth_tiers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23814-23821` `full_panel_with_tiers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23823-23825` `strs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23828-23854` `path_is_high_risk_matches_by_prefix_and_by_glob` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23857-23866` `route_review_tier_with_no_policy_is_the_full_panel_unchanged` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23869-23888` `route_review_tier_routes_a_low_risk_unit_to_the_light_panel` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23891-23906` `route_review_tier_forces_full_on_a_high_risk_path_hit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23909-23922` `route_review_tier_forces_full_over_the_size_threshold` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23925-23938` `route_review_tier_forces_full_when_the_gates_flapped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23941-23975` `route_review_tier_fails_safe_to_full_on_an_empty_blast_radius` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23981-24033` `run_tiered_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24037-24048` `logged_review_tier` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24051-24077` `a_low_risk_unit_runs_the_light_panel_and_logs_the_routing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24080-24140` `a_low_risk_unit_skips_the_adversary_and_extra_lens` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24143-24213` `a_high_risk_unit_runs_the_full_panel_and_logs_the_routing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24216-24231` `without_a_depth_policy_a_grounded_unit_runs_the_full_panel_and_logs_no_routing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24234-24312` `a_zero_grounding_unit_fails_safe_to_the_full_panel_and_logs_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24315-24424` `a_flapped_unit_escalates_from_light_at_attempt_0_to_full_on_remediation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24427-24491` `a_stage_level_tiers_policy_routes_the_unit_by_risk` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24494-24576` `every_spawn_runs_in_a_worktree_never_the_main_repo_checkout` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24580-24603` `spec_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24606-24698` `speculation_width_2_runs_two_candidates_first_green_wins_rest_cancelled` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24701-24737` `speculation_budget_accounts_for_every_candidate_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24740-24781` `speculation_defaults_off_runs_a_single_candidate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24784-24824` `speculation_parks_all_candidates_together_in_one_step` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24827-24949` `speculation_defers_green_until_the_winner_and_integrates_across_replay_steps` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24955-24965` `unit_has_status` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24969-24982` `branch_present` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24985-25059` `speculation_later_candidate_wins_when_an_earlier_lane_is_review_rejected` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25062-25180` `speculation_low_risk_later_lane_winner_routes_light_not_falsely_flapped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25183-25239` `speculation_rejected_loser_is_visible_to_the_review_quality_fold` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25242-25309` `speculation_escalates_when_every_candidate_is_rejected` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25312-25377` `speculation_crashed_lane_is_absent_and_a_sibling_still_wins` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25380-25447` `speculation_exhaustive_integrate_door_red_blocks_a_candidate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25450-25557` `speculation_stays_fresh_across_parking_until_a_winner_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25572-25639` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25643-25809` `speculation_blocked_winner_captures_evidence_and_a_later_lane_wins` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25812-25866` `assert_isolated_cwd_refuses_empty_or_repo_root_with_a_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25869-25960` `the_conductor_threads_each_agents_persona_to_the_driver` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25963-26017` `the_system_prompt_carries_the_rigger_communication_discipline` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26020-26060` `an_agent_with_no_persona_threads_an_empty_system_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26063-26125` `planner_proposed_unit_inherits_the_default_review_panel` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26128-26223` `a_producer_stage_skips_the_three_tier_review_and_unblocks_its_dependents` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26226-26308` `plan_stage_commit_under_specs_reaches_the_run_branch_before_the_next_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26311-26386` `plan_stage_commit_outside_specs_fails_the_stage_naming_the_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26389-26473` `plan_stage_commit_reverting_its_own_out_of_scope_touch_still_fails_the_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26476-26547` `plan_stage_commit_conflicting_with_a_concurrent_specs_change_escalates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26550-26683` `integrate_plan_commits_is_idempotent_on_a_resumed_already_landed_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26686-26747` `integrate_plan_commits_tolerates_a_pre_existing_intent_record_with_no_git_mutation_yet` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26750-26836` `integrate_plan_commits_keeps_the_earlier_commits_identity_when_the_worktree_grows_between_calls` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26849-26865` `append` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26866-26873` `read_stream` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26874-26881` `read_all` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26882-26888` `subscribe_all` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26889-26895` `subscribe_stream` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26899-26957` `integrate_plan_commits_wraps_any_hard_error_with_the_plan_landing_marker` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26960-27042` `a_plan_landing_infra_fault_halts_the_run_loudly_with_no_per_unit_lesson_or_attempt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27045-27117` `per_unit_adjudicator_reject_blocks_integration_and_escalates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27120-27213` `an_always_rejecting_adjudicator_escalates_after_exactly_max_retries_cycles` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27216-27313` `a_higher_max_retries_gives_more_attempts_before_escalation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27230-27276` `escalation_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27316-27380` `an_absent_max_retries_preserves_the_default_bound_of_three` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27383-27487` `a_resumed_unit_gets_exactly_its_granted_extra_attempts_before_re_escalating` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27490-27531` `max_retries_for_widens_only_the_resumed_unit_never_a_sibling` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27534-27581` `a_stages_own_max_retries_overrides_the_run_default_for_its_units` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27599-27605` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27608-27641` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27645-27718` `approval_on_the_final_permitted_attempt_integrates_a_per_unit_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27721-27811` `a_model_ladder_implementer_escalates_one_rung_per_remediation_attempt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27814-27880` `approval_on_the_final_permitted_attempt_integrates_a_standalone_review_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27883-27917` `mid_spawn_crash_escalates_without_aborting_the_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27920-27971` `a_newly_escalated_unit_stamps_an_attention_entry` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27974-28024` `a_budget_halt_stamps_an_attention_entry` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28027-28116` `a_budget_halt_does_not_restamp_on_a_later_poll_with_nothing_new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28119-28223` `a_delayed_budget_halt_after_a_dependency_unlocks_still_stamps` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28226-28297` `compute_attention_leaves_the_hung_liveness_halt_to_the_caller` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28300-28381` `an_escalation_does_not_restamp_attention_on_a_resumed_process` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28384-28417` `a_clean_step_stamps_no_attention` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28420-28544` `a_second_failure_recurs_and_a_third_also_stalls_the_frontier` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28547-28594` `nine_of_ten_budget_crosses_the_final_tenth` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28597-28684` `a_re_step_already_past_the_budget_threshold_does_not_re_stamp` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28687-28738` `budget_breaker_stops_the_run_after_the_first_wave` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28741-28784` `budget_exhaustion_aborts_the_task` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28787-28825` `a_budget_halt_surfaces_its_reason_on_the_run_state` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28828-28860` `a_run_within_budget_surfaces_no_halt_reason` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28863-28931` `the_spawn_budget_folds_from_recorded_spawn_requests_across_steps` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28934-28975` `the_pre_wave_breaker_trips_only_on_a_new_over_budget_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28978-29048` `a_review_tier_budget_refusal_aborts_with_budgetexhausted_not_a_raw_error` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29051-29083` `coverage_gap_flags_a_spec_defect_and_errors` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29086-29127` `planner_covering_every_criterion_passes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29130-29164` `planner_leaving_a_gap_flags_a_spec_defect` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29167-29201` `gate_only_stage_is_a_coverage_proxy_gap` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29204-29263` `manual_stage_pauses_while_an_auto_stage_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29266-29309` `isolation_none_agent_gets_no_worktree_even_with_a_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29312-29345` `spawn_opts_isolation_is_set_for_a_worktree_agent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29348-29387` `a_spawned_implementers_title_is_the_unit_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29394-29442` `the_adversarys_spawn_is_stamped_with_the_units_lens_roster` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29448-29497` `the_adjudicators_spawn_is_stamped_with_lenses_plus_adversary` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29502-29549` `a_panel_with_no_adversary_never_fabricates_one_in_the_adjudicators_roster` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29558-29621` `the_roster_reflects_the_actually_routed_light_panel_not_the_full_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29635-29692` `the_fan_out_review_loops_adversary_and_adjudicator_spawns_are_stamped_with_the_routed_roster` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29695-29765` `stage_autonomy_override_seeds_the_gate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29768-29815` `on_pass_none_runs_gates_but_does_not_integrate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29826-29841` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29845-29932` `two_units_gate_environments_never_share_a_target_dir` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29935-30001` `two_units_gate_environments_never_share_a_mutants_root` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30004-30055` `an_implement_stage_gate_round_creates_no_mutants_directory` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30064-30068` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30069-30071` `envs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30074-30083` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30091-30218` `one_build_environment_authority_reaches_both_a_gate_build_and_an_agent_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30106-30143` `run_once` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30221-30265` `spawn_env_adds_the_per_unit_cargo_target_dir_only_for_a_real_unit_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30268-30331` `a_units_per_unit_cache_is_reclaimed_when_its_worktree_is_removed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30334-30406` `a_units_worktree_cache_and_branch_are_all_reclaimed_on_a_successful_integrate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30409-30469` `a_units_worktree_is_reclaimed_but_its_branch_survives_a_terminal_escalation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30472-30578` `a_parked_review_spawn_keeps_the_unit_worktree_registered_and_its_cache` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30581-30673` `review_unit_restores_a_worktree_a_gate_deleted_out_of_band` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30676-30778` `a_second_step_restores_a_still_parked_units_worktree_deleted_out_of_band` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30781-30879` `verified_worktree_sha_is_stamped_after_a_gate_side_deletion_is_restored` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30882-30960` `review_tier_boundary_restores_a_worktree_a_prior_tier_deleted` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30963-31077` `reviewed_worktree_sha_is_stamped_after_the_adjudicators_own_deletion_is_restored` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31080-31147` `failed_worktree_sha_is_stamped_after_the_adjudicators_own_deletion_is_restored` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31150-31253` `a_resumed_reviewed_unit_restores_a_worktree_the_exhaustive_gate_deleted` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31256-31341` `a_resumed_reviewed_unit_whose_exhaustive_gate_fails_records_a_gate_cause` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31344-31482` `a_resumed_reviewed_units_merge_break_records_an_integrate_conflict_cause` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31485-31666` `a_resumed_reviewed_units_genuine_unresolved_conflict_reaches_the_idempotent_merge_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31669-31743` `a_live_approved_unit_restores_a_worktree_the_integrate_door_exhaustive_gate_deleted` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31746-31863` `speculation_lenses_restore_a_gate_deleted_worktree_without_racing_and_stamp_the_winner_sha` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31866-31937` `speculation_reject_worktree_sha_is_stamped_after_the_adjudicators_own_deletion_is_restored` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31940-32060` `a_parked_lens_keeps_the_unit_worktree_beside_a_genuine_sibling_crash` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32063-32108` `a_worktree_less_stage_gate_inherits_the_shared_target` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32111-32148` `bounded_pool_completes_every_stage_under_the_cap` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32151-32197` `live_run_folds_no_gate_machinery` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32200-32243` `agent_stage_runs_the_per_unit_lifecycle_not_the_fan_out_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32246-32287` `standalone_review_stage_still_takes_the_fan_out_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32290-32402` `run_gates_derives_and_injects_the_review_worktrees_store_fence` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32405-32458` `a_parked_lens_keeps_the_standalone_review_stages_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32461-32557` `a_parked_lens_keeps_the_review_worktree_even_beside_a_lower_indexed_sibling_crash` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32560-32653` `a_parked_lens_keeps_the_review_worktree_beside_a_sibling_degenerate_halt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32656-32741` `the_swap_to_front_prioritizes_a_genuine_error_at_a_non_zero_chunk_index` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32744-32831` `a_budget_refused_lens_beside_a_genuinely_crashing_sibling_in_one_chunk` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32834-32901` `a_budget_refused_standalone_review_spawn_keeps_its_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32904-32931` `partition_separates_overlapping_blast_radii` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32934-32956` `blast_radius_conflicts_flags_units_that_split_one_file_set` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32959-32972` `blast_radius_conflicts_is_empty_for_a_disjoint_partition` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32975-33023` `partitioned_wave_still_integrates_every_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33035-33059` `run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33063-33172` `lens_finding_reaches_later_tiers_through_the_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33175-33236` `review_agents_emit_findings_via_the_review_protocol` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33239-33284` `unparseable_adjudicator_output_blocks_integration` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33287-33348` `failing_gate_evidence_threaded_into_retry_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33351-33434` `a_flaky_gate_rerun_is_a_pass_with_warning_that_never_demotes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33437-33507` `a_product_gate_failure_is_not_rerun_and_demotes_as_before` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33510-33588` `an_authored_infra_rule_with_limit_zero_at_an_inline_gate_holds_the_ratchet` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33591-33686` `an_inline_gate_red_across_every_rerun_holds_for_infra_and_demotes_for_flaky` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33689-33728` `unit_evidence_is_populated_after_a_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33731-33806` `adjudicator_rejection_reasoning_threaded_into_retry_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33809-33876` `fan_out_lens_does_not_integrate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33879-33931` `review_only_stage_records_no_artifact_truthfully` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33934-33992` `two_erroring_stages_both_leave_a_record` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33995-34059` `single_wide_wave_overruns_budget_and_is_stopped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34062-34108` `validate_acyclic_detects_a_cycle` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34159-34172` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34175-34180` `materializing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34184-34189` `deleting_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34190-34192` `calls` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34193-34195` `targets` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34196-34198` `mutants_dirs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34199-34201` `build_envs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34202-34204` `store_fences` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34205-34207` `build_cache_guards` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34208-34210` `build_cache_dirs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34213-34263` `run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34268-34290` `content_cache_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34294-34300` `attempt_of` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34312-34349` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34360-34365` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34366-34368` `calls` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34371-34391` `run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34395-34404` `gate_verdict_event` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34406-34410` `verdict_passed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34413-34479` `a_matching_input_digest_answers_a_gate_as_a_logged_cache_hit_citing_the_prior_green` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34482-34524` `a_content_change_misses_the_cache_and_re_runs_the_gate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34527-34583` `a_red_verdict_is_never_cache_answered_and_the_gate_re_runs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34592-34604` `ground` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34616-34631` `ground` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34632-34634` `blast_radius` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34635-34637` `index_stamp` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34641-34660` `blast_radius_audits` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34663-34672` `review_tier_evidence` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34677-34700` `tiered_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34702-34709` `tiered_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34719-34792` `a_structural_grounder_records_the_audit_and_routes_full_on_a_beyond_cap_high_risk_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34799-34839` `the_empty_radius_fail_safe_still_records_the_audit_and_routes_full` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34847-34881` `a_non_structural_grounder_records_no_blast_radius_audit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34893-34958` `confidence_tier_fixture` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34969-35020` `confidence_tier_radius_splits_the_one_edge_set_into_extracted_precise_and_all_tier_safe` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35030-35121` `grounded_blast_radius_tier_filters_the_subgraph_and_keeps_the_grep_superset` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35036-35038` `apply` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35039-35045` `subgraph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35046-35048` `resolve` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35144-35237` `under_symbols_the_audit_emits_and_records_the_prompt_seed_as_precise` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35249-35323` `partition_wave_own_batches_an_empty_radius_never_co_scheduling_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35332-35396` `speculation_over_a_structural_grounder_records_the_audit_and_routes_full` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35399-35450` `staleness_flags_only_downstream_units_whose_radius_intersects_the_touched_files` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35453-35505` `staleness_grounds_on_the_safe_superset_so_a_grep_only_reference_still_stales_a_downstream_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35508-35606` `rule6_conflict_detection_grounds_on_the_safe_superset_so_a_grep_only_shared_reference_conflicts` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35609-35639` `a_resume_reseeds_the_stale_set_from_the_prior_unitintegrated_marks` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35648-35689` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35693-35818` `integrating_a_unit_stales_the_intersecting_downstream_units_cached_verdict_not_the_rest` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35821-35847` `glob_matches_supports_star_doublestar_and_literals` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35850-35874` `gate_intersects_radius_runs_unscoped_and_intersecting_gates_only` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35877-36026` `blast_radius_narrows_the_inner_loop_and_the_integrate_step_runs_the_full_library` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36029-36157` `a_gate_skipped_inline_but_red_at_the_exhaustive_integrate_door_blocks_the_merge` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36160-36189` `verdict_compensates_names_a_prior_unit_independent_of_approval` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36202-36244` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36248-36423` `a_contradiction_compensates_reverts_and_re_enters_the_integrated_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36426-36477` `commits_to_compensate_reverts_every_sha_a_multi_commit_landing_recorded` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36480-36539` `commits_to_compensate_dedupes_a_repeated_sha_and_skips_an_already_compensated_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36542-36593` `commits_to_compensate_excludes_the_review_only_marker_even_alongside_a_real_sha` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36596-36656` `pending_compensations_from_log_re_derives_only_undrained_marks` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36659-36690` `a_durable_compensation_queued_mark_is_fold_neutral_on_the_target` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36698-36735` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36739-36822` `a_compensated_unit_re_gates_its_re_implemented_tree_not_the_condemned_verdict` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36833-36878` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36882-36980` `a_compensated_unit_that_remediated_before_integrating_re_gates_at_a_fresh_key` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36990-37014` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37018-37153` `a_resume_re_drives_a_durably_queued_but_undrained_compensation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37173-37221` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37225-37407` `integrate_re_gates_the_merged_tree_and_a_merge_break_blocks_the_second_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37430-37497` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37501-37608` `the_post_merge_re_gate_gets_the_units_mutants_root_though_it_runs_in_the_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37611-37731` `conflict_regenerate_pending_from_log_re_derives_the_union_keyed_by_unit_and_attempt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37739-37765` `conflict_fixture` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37768-37854` `integrate_conflict_re_parks_the_implementer_with_no_attempt_charged_and_both_units_land` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37857-37937` `integrate_conflict_confined_to_a_regenerable_path_resolves_with_no_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37940-38044` `integrate_conflict_exhausted_after_the_bound_charges_a_real_attempt_with_the_unresolved_evidence` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:38047-38205` `integrate_conflict_records_regenerate_pending_before_the_accept_incoming_mutation_that_can_fail` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:38087-38130` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:38208-38282` `a_deferred_gate_runs_once_at_the_phase_boundary_not_inline` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:38285-38353` `a_failing_deferred_gate_is_surfaced_and_the_run_is_not_done` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:38364-38387` `run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:38391-38462` `a_default_infra_fault_at_a_deferred_gate_does_not_demote` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:38465-38569` `a_replayed_step_re_runs_no_recorded_gate_and_appends_no_duplicate_events` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:38572-38657` `a_re_step_replays_a_recorded_deferred_gate_without_re_running_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:38660-38804` `a_parked_step_never_records_a_partial_tree_deferred_verdict` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:38807-38891` `a_manual_review_paused_unit_defers_the_whole_tree_gate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:38894-39027` `an_escalated_dep_still_runs_the_whole_tree_deferred_gate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39030-39144` `a_recorded_failing_deferred_verdict_re_surfaces_its_failure_on_replay` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39147-39233` `a_replayed_fan_out_review_reject_appends_no_duplicate_unitfailed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39236-39284` `a_fan_out_review_stage_whose_gates_fail_after_approval_records_a_gate_cause` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39290-39304` `run_git` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39308-39316` `git_head` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39318-39335` `init_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39346-39377` `run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39381-39440` `gate_measures_the_committed_artifact_not_the_dirty_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39443-39569` `conductor_spawns_the_sdet_author_at_the_build_seam_so_its_tests_land_in_the_committed_tree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39572-39662` `the_sdet_author_spawn_respects_the_budget_breaker_at_its_own_spawn_site` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39665-39767` `a_parked_sdet_author_spawn_holds_the_unit_instead_of_integrating_without_its_periphery_tests` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39770-39882` `speculation_sdet_author_periphery_lands_in_the_committed_tree_the_gates_judge` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39885-39982` `a_parked_sdet_author_in_a_speculation_candidate_holds_the_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39985-40060` `an_empty_accounting_sdet_author_result_advances_the_build_to_commit_gates_and_integrate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40063-40127` `an_absent_sdet_author_is_a_clean_no_op_and_the_build_still_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40130-40205` `a_crashed_sdet_author_spawn_does_not_block_the_build_the_lifecycle_proceeds` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40208-40308` `an_escalated_unit_does_not_integrate_its_code` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40316-40355` `critique_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40384-40395` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40398-40403` `rejecting` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40406-40411` `always_rejecting` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40412-40419` `count` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40422-40472` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40476-40545` `a_blast_radius_overlap_alone_does_not_reject` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40558-40602` `the_plan_critique_gates_adversary_and_adjudicator_spawns_are_stamped_correctly` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40605-40642` `a_rule_7_or_8_defect_rejects_then_releases_on_the_revision` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40645-40696` `a_clean_decomposition_approves_and_releases_the_fan_out` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40699-40774` `the_gate_critiques_only_not_yet_run_units_and_approves` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40777-40814` `the_plan_critique_prompt_names_the_cross_unit_rules` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40817-40842` `the_critique_gate_never_enters_a_wave_even_when_ready` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40851-40885` `fan_out_needs_template_fixture` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40888-40970` `a_downstream_stage_needing_the_fan_out_template_stays_unready_until_every_member_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40973-41077` `a_real_split_pair_must_both_integrate_not_just_the_btreemap_key_first_sibling` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:41080-41154` `a_resolved_plan_critique_gate_does_not_re_run_on_resume` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:41157-41255` `a_resumed_step_holds_the_fan_out_while_the_plan_critique_gate_is_escalated` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:41258-41314` `an_approved_gate_releases_planner_proposed_units_not_only_baselines` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:41317-41384` `the_re_plan_directive_instructs_reusing_the_existing_unit_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:41397-41402` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:41403-41410` `count` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:41413-41433` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:41437-41508` `a_resumed_step_holds_the_fan_out_while_the_plan_critique_gate_is_mid_review` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:41511-41565` `a_parked_plan_critique_gate_keeps_its_review_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:41568-41618` `a_budget_refused_plan_critique_gate_keeps_its_review_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:41631-41658` `the_conductors_one_event_authority_reports_a_write_the_store_lost` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
- `dash::buckets` (8 functions)
  - `src/dash.rs:1885-1952` `new` - method inside `impl <'g> Buckets<'g>`; grouped with its other `Buckets` methods.
  - `src/dash.rs:1959-1972` `key` - method inside `impl <'g> Buckets<'g>`; grouped with its other `Buckets` methods.
  - `src/dash.rs:1977-1983` `excludes_super_node` - method inside `impl <'g> Buckets<'g>`; grouped with its other `Buckets` methods.
  - `src/dash.rs:1987-1989` `underived` - method inside `impl <'g> Buckets<'g>`; grouped with its other `Buckets` methods.
  - `src/dash.rs:1993-1999` `underived_message` - method inside `impl <'g> Buckets<'g>`; grouped with its other `Buckets` methods.
  - `src/dash.rs:2010-2016` `no_membership_message` - method inside `impl <'g> Buckets<'g>`; grouped with its other `Buckets` methods.
  - `src/dash.rs:2022-2028` `label_kind` - method inside `impl <'g> Buckets<'g>`; grouped with its other `Buckets` methods.
  - `src/dash.rs:2032-2034` `is_shared` - method inside `impl <'g> Buckets<'g>`; grouped with its other `Buckets` methods.
- `dash::dash_marker` (4 functions)
  - `src/dash.rs:390-392` `serialize` - method inside `impl DashMarker`; grouped with its other `DashMarker` methods.
  - `src/dash.rs:398-403` `parse` - method inside `impl DashMarker`; grouped with its other `DashMarker` methods.
  - `src/dash.rs:407-409` `read` - method inside `impl DashMarker`; grouped with its other `DashMarker` methods.
  - `src/dash.rs:414-416` `write` - method inside `impl DashMarker`; grouped with its other `DashMarker` methods.
- `dash::lens` (1 function)
  - `src/dash.rs:1832-1848` `from_query` - method inside `impl Lens`; grouped with its other `Lens` methods.
- `dash::reaped_child` (3 functions)
  - `src/dash.rs:4917-4919` `new` - method inside `impl ReapedChild`; grouped with its other `ReapedChild` methods.
  - `src/dash.rs:4923-4925` `id` - method inside `impl ReapedChild`; grouped with its other `ReapedChild` methods.
  - `src/dash.rs:4929-4940` `drop` - method inside `impl Drop for ReapedChild`; grouped with its other `ReapedChild` methods.
- `dash::registry` (7 functions)
  - `src/dash.rs:207-217` `dash_serving_pid_on` - name contains "pid" (instance registry); grouped under `dash::registry`.
  - `src/dash.rs:384-386` `displayable_pid` - name contains "pid" (instance registry); grouped under `dash::registry`.
  - `src/dash.rs:425-427` `pid_is_alive` - name contains "pid" (instance registry); grouped under `dash::registry`.
  - `src/dash.rs:468-490` `pid_holding_port` - name contains "pid" (instance registry); grouped under `dash::registry`.
  - `src/dash.rs:635-637` `pid_if_port_matches` - name contains "pid" (instance registry); grouped under `dash::registry`.
  - `src/dash.rs:855-876` `instance_views` - name contains "instance" (instance registry); grouped under `dash::registry`.
  - `src/dash.rs:881-883` `instances_json` - name contains "instance" (instance registry); grouped under `dash::registry`.
- `dash::render` (26 functions)
  - `src/dash.rs:223-253` `header_line_value` - name contains "header" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:259-282` `head_has_header_line` - name contains "header" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:520-535` `format_held_port` - name contains "format" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:551-553` `describe_held_port` - name contains "describe" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:600-602` `describe_held_port_if_confirmed` - name contains "describe" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:1556-1588` `build_graph_view` - name contains "graph" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:1618-1627` `node_label` - name contains "node" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:2087-2126` `whole_graph_lens_key` - name contains "graph" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:2815-2817` `neighborhood` - name contains "neighborhood" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:2826-2926` `neighborhood_of` - name contains "neighborhood" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:2978-3008` `node_rationale` - name contains "node" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:3015-3029` `rationale_batch` - name contains "rationale" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:3226-3232` `whole_graph_degree` - name contains "graph" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:3264-3270` `card_ref` - name contains "card" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:3277-3336` `card` - name contains "card" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:3393-3425` `graph_json` - name contains "graph" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:3463-3475` `call_node_view` - name contains "node" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:3492-3504` `ref_node_view` - name contains "node" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:3795-3876` `unit_node` - name contains "node" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:3880-3939` `role_stage` - name contains "role" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:3987-3998` `stage_of_role` - name contains "role" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:4006-4027` `role_and_agent` - name contains "role" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:4108-4116` `graph_seeds` - name contains "graph" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:4227-4233` `field_str` - name contains "field" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:4237-4248` `field_str_array` - name contains "field" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:4343-4356` `escape_for_script` - name contains "escape" (HTML rendering); grouped under `dash::render`.
- `dash::reproject` (12 functions)
  - `src/dash.rs:1658-1669` `cluster_key` - name contains "cluster" (graph reprojection); grouped under `dash::reproject`.
  - `src/dash.rs:1710-1725` `defs_by_entity_suffix` - name contains "defs" (graph reprojection); grouped under `dash::reproject`.
  - `src/dash.rs:2149-2216` `clustered_overview` - name contains "cluster" (graph reprojection); grouped under `dash::reproject`.
  - `src/dash.rs:2223-2238` `bucket_label_index` - name contains "bucket" (graph reprojection); grouped under `dash::reproject`.
  - `src/dash.rs:2254-2331` `fold_buckets` - name contains "bucket" (graph reprojection); grouped under `dash::reproject`.
  - `src/dash.rs:2377-2510` `cluster_detail` - name contains "cluster" (graph reprojection); grouped under `dash::reproject`.
  - `src/dash.rs:2536-2549` `reproject` - name contains "reproject" (graph reprojection); grouped under `dash::reproject`.
  - `src/dash.rs:2557-2576` `cap_clusters` - name contains "cluster" (graph reprojection); grouped under `dash::reproject`.
  - `src/dash.rs:2639-2650` `reprojection_lens_key` - name contains "reproject" (graph reprojection); grouped under `dash::reproject`.
  - `src/dash.rs:2656-2699` `reproject_derived` - name contains "reproject" (graph reprojection); grouped under `dash::reproject`.
  - `src/dash.rs:2720-2805` `reproject_files` - name contains "reproject" (graph reprojection); grouped under `dash::reproject`.
  - `src/dash.rs:3239-3257` `community_label` - name contains "community" (graph reprojection); grouped under `dash::reproject`.
- `dash::response` (5 functions)
  - `src/dash.rs:4371-4377` `html` - method inside `impl Response`; grouped with its other `Response` methods.
  - `src/dash.rs:4378-4384` `json` - method inside `impl Response`; grouped with its other `Response` methods.
  - `src/dash.rs:4385-4391` `text` - method inside `impl Response`; grouped with its other `Response` methods.
  - `src/dash.rs:4393-4402` `reason` - method inside `impl Response`; grouped with its other `Response` methods.
  - `src/dash.rs:4412-4428` `write_to` - method inside `impl Response`; grouped with its other `Response` methods.
- `dash::server` (8 functions)
  - `src/dash.rs:312-324` `bind_singleton` - name contains "bind" (HTTP serving); grouped under `dash::server`.
  - `src/dash.rs:438-457` `tcp_listen_inode_for_port` - name contains "tcp" (HTTP serving); grouped under `dash::server`.
  - `src/dash.rs:499-507` `process_state` - name contains "process_" (HTTP serving); grouped under `dash::server`.
  - `src/dash.rs:3620-3657` `calls_route` - name contains "route" (HTTP serving); grouped under `dash::server`.
  - `src/dash.rs:4437-4595` `route` - name contains "route" (HTTP serving); grouped under `dash::server`.
  - `src/dash.rs:4665-4692` `serve` - name contains "serve" (HTTP serving); grouped under `dash::server`.
  - `src/dash.rs:4709-4747` `serve_on` - name contains "serve" (HTTP serving); grouped under `dash::server`.
  - `src/dash.rs:4763-4882` `handle_conn` - name contains "handle" (HTTP serving); grouped under `dash::server`.
- `dash::tests` (96 functions)
  - `src/dash.rs:5041-5043` `ev` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5047-5052` `positioned` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5054-5066` `seeded_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5068-5077` `local_instance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5085-5135` `instance_views_project_a_sorted_credential_free_landing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5142-5150` `instance_view_age_floors_at_zero_for_a_future_heartbeat` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5157-5189` `api_instances_route_renders_the_landing_list` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5192-5217` `root_serves_the_embedded_page_with_the_placeholder_resolved` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5228-5278` `gates_status_never_fabricates_passed_for_an_off_linear_unverdicted_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5285-5321` `free_port_from_returns_the_start_port_when_free_and_the_next_free_one_when_it_is_taken` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5328-5377` `bind_singleton_binds_the_exact_port_and_never_searches` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5388-5461` `bind_singleton_short_circuits_on_an_already_serving_rigger_dash` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5468-5484` `dash_serving_on_is_false_for_a_non_dash_listener` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5495-5540` `dash_serving_on_is_bounded_against_a_byte_dribbling_holder` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5563-5614` `dash_serving_on_recognizes_the_header_fast_even_if_the_holder_never_finishes_the_block` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5625-5645` `dash_serving_pid_on_reports_the_pid_a_real_dash_response_names` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5667-5690` `dash_serving_pid_on_assembles_a_head_that_spans_many_read_calls` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5697-5712` `dash_serving_pid_on_is_none_for_a_non_dash_listener` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5717-5720` `dash_serving_pid_on_is_none_when_nothing_answers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5740-5760` `dash_serving_pid_on_is_none_when_the_pid_header_value_is_not_a_number` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5768-5797` `bind_singleton_cold_race_loser_resolves_across_the_accept_window` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5808-5843` `the_page_layout_cannot_scroll_the_body_horizontally` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5854-5891` `the_landing_view_lists_instances_and_threads_the_attach_selector` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5896-5905` `css_rule` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5916-5961` `the_dashboard_fits_one_screen_with_internal_scroll` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5975-6009` `cells_fit_or_wrap_and_wide_cells_scroll_in_their_own_container` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:6024-6088` `the_decision_history_renders_each_decision_as_a_native_details_with_preview_and_full_body` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:6091-6119` `state_endpoint_projects_the_seeded_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:6122-6184` `state_carries_the_live_agent_activity` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:6187-6254` `state_counts_grep_fallbacks_and_carries_them_in_the_review_outcomes_data` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:6263-6445` `run_tree_projects_the_spine_with_collapse_expand_and_driver_lines` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:6269-6274` `done` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:6450-6479` `review_verdicts_come_straight_from_the_metrics_classification` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:6482-6492` `events_endpoint_is_since_exclusive` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:6498-6527` `no_mutating_endpoint_exists` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:6530-6544` `unknown_get_path_is_404` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:6547-6576` `export_inlines_the_snapshot_as_a_static_page` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:6584-6631` `export_neutralizes_a_script_breakout_in_the_inlined_state` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:6634-6660` `decision_view_strikes_through_superseded_entries` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:6669-6732` `cluster_key_folds_paths_by_directory_and_dev_loop_nodes_by_kind` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:6744-6837` `clustered_overview_under_files_lens_admits_only_code_entities_keyed_by_their_own_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:6856-7071` `cluster_detail_drills_a_cluster_to_its_members_and_caps_a_big_one_by_degree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7080-7125` `cluster_detail_under_files_lens_is_unconditionally_empty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7142-7339` `code_lens_buckets_code_entities_by_community_excludes_other_kinds_and_reports_underived_grain` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7353-7592` `concepts_lens_buckets_members_by_concept_excludes_membershipless_nodes_and_reports_underived_grain` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7598-7632` `tiered_chain_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7635-7699` `the_graph_route_returns_a_tier_tagged_seeded_neighborhood_as_json` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7702-7753` `the_graph_route_percent_decodes_the_seed_so_select_to_seed_reaches_ids_with_special_chars` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7756-7787` `the_graph_route_degrades_gracefully_for_an_unknown_seed_and_an_empty_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7790-7810` `the_graph_route_is_read_only_a_non_get_is_405` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7816-7850` `dispatch_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7859-7957` `the_graph_route_dispatches_cluster_overview_and_seed_by_parameter` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7971-8025` `the_overview_route_degrades_gracefully_on_an_empty_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8028-8082` `neighborhood_bounds_by_depth_follows_both_directions_and_skips_invalidated_edges` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8088-8113` `star_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8116-8149` `neighborhood_flags_god_nodes_by_degree_within_the_returned_neighborhood` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8152-8224` `path_is_the_shortest_route_between_two_selected_nodes_over_currently_valid_edges` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8227-8294` `the_graph_route_flags_god_nodes_and_returns_the_query_path_between_two_selected_nodes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8297-8317` `chain_graph_local` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8324-8361` `provenance_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8364-8411` `explain_returns_a_nodes_incident_edges_as_source_and_tier_tagged_provenance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8414-8479` `the_graph_route_carries_the_seed_nodes_explain_provenance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8482-8513` `graph_seeds_enumerate_decisions_findings_and_their_files_never_units` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8516-8560` `a_units_seed_lands_on_the_neighborhood_of_its_decisions_and_files` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8563-8669` `the_run_tree_click_to_seed_route_lands_a_unit_on_a_real_neighborhood` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8672-8725` `unit_seeds_scope_content_to_the_owning_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8728-8764` `repoint_seed_passes_a_known_node_and_re_points_a_unit_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8767-8786` `build_state_on_an_empty_run_is_empty_not_a_panic` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8797-8886` `release_ready_is_surfaced_on_the_dash_only_for_a_done_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8904-8959` `release_ready_pr_command_newline_renders_as_a_real_line_break_not_a_collapsed_run_on` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8962-8969` `request_line_parsing_extracts_method_and_target` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8972-8979` `query_param_reads_since` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8987-9064` `endpoints_serve_over_a_real_socket_against_a_seeded_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:9070-9120` `a_post_over_a_real_socket_is_refused_without_touching_the_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:9131-9243` `the_graph_provider_is_consulted_only_on_graph_requests_not_the_state_poll` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:9248-9258` `dash_marker_round_trips_through_its_on_disk_record` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:9261-9280` `dash_marker_parse_rejects_a_malformed_record` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:9283-9301` `dash_marker_reads_none_for_an_absent_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:9304-9317` `pid_is_alive_reports_self_and_rejects_an_impossible_pid` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:9322-9334` `format_held_port_always_names_the_address_even_with_no_holder` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:9337-9354` `format_held_port_names_the_pid_and_state_for_a_running_holder` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:9357-9365` `format_held_port_names_the_pid_alone_when_its_state_is_not_discoverable` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:9368-9383` `format_held_port_gives_the_stopped_listener_diagnosis_naming_resume_or_kill` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:9386-9398` `pid_holding_port_finds_the_pid_of_a_listener_bound_in_this_process` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:9401-9417` `pid_holding_port_is_none_for_a_port_nothing_is_listening_on` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:9420-9438` `describe_held_port_names_this_process_when_it_holds_the_port_itself` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:9453-9469` `describe_held_port_if_confirmed_names_the_holder_when_independently_confirmed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:9476-9492` `describe_held_port_if_confirmed_is_none_when_nothing_holds_the_port` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:9495-9512` `dash_start_needed_is_true_when_none_serving_and_false_when_one_serves` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:9515-9563` `dash_status_trusts_a_url_with_no_marker_and_catches_a_marker_that_lies` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:9575-9591` `dash_status_never_names_the_unattributed_pid_sentinel_as_a_dead_process` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:9594-9615` `url_port_parses_the_recorded_shape_and_rejects_anything_else` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:9631-9671` `dash_status_probes_the_urls_own_port_when_the_marker_names_a_different_dash` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:9674-9712` `should_reap_singleton_reaps_only_when_no_registered_instance_is_live` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:9715-9748` `should_reap_singleton_never_reaps_while_a_fresh_agent_liveness_signal_is_present` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:9760-9831` `the_page_carries_the_directed_call_layered_render` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
- `dash::tests::calls_route_c4` (14 functions)
  - `src/dash.rs:9849-9859` `cnode` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9862-9875` `cedge` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9877-9883` `file_node` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9885-9887` `layer_of` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9888-9890` `ids` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9894-9905` `apply_def` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9906-9913` `apply_call` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9919-9974` `calls_view_down_signs_callees_positive_and_carries_frontier_and_back` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9979-10018` `calls_view_up_negates_callers_and_carries_the_referenced_sidecar` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10024-10072` `calls_view_both_centers_the_seed_with_callees_right_and_callers_left` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10078-10115` `a_plain_neighborhood_omits_every_additive_call_field` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10122-10176` `calls_route_runs_the_traversal_for_view_calls_and_declines_otherwise` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10182-10210` `calls_route_clamps_depth_and_defaults_the_tier_floor` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10215-10233` `calls_route_walks_both_directions_for_dir_both` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
- `dash::tests::metadata_card_c2` (15 functions)
  - `src/dash.rs:10868-10877` `node` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10879-10889` `edge` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10896-10931` `card_graph` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10934-10977` `card_of_a_code_entity_carries_file_line_degree_community_concepts_and_memory_counts` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10983-10991` `card_of_a_membership_less_entity_has_no_community_and_no_line` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10999-11026` `card_of_a_proven_code_entity_carries_proven_by_and_proof_evidence` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:11034-11039` `card_of_an_unproven_code_entity_has_proven_by_zero_and_no_evidence` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:11045-11058` `card_of_a_file_reports_no_proof_of_its_own` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:11065-11075` `card_tolerates_a_malformed_proof_evidence_attr` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:11082-11111` `card_of_a_file_lists_its_contained_entities_as_top_entities` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:11116-11136` `card_of_a_concept_lists_its_realizing_members_as_top_evidence` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:11145-11166` `card_of_a_concept_carries_each_top_evidence_members_own_kind` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:11173-11183` `card_of_a_file_carries_each_top_entity_members_own_kind` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:11188-11191` `card_of_an_unknown_id_is_none` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:11197-11239` `the_card_route_serves_a_known_subjects_card_and_null_for_an_unknown_one` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
- `dash::tests::rationale_overlay_c3` (15 functions)
  - `src/dash.rs:10250-10260` `node` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10265-10275` `finding_node` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10277-10287` `edge` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10303-10336` `rationale_graph` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10338-10340` `ids` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10341-10343` `kinds` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10349-10363` `node_rationale_returns_attached_leaves_ordered_by_kind_then_id` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10369-10392` `a_finding_leaf_carries_content_only_never_the_by_or_unit_machinery` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10398-10411` `a_handbook_rule_and_a_supersedes_edge_are_not_rationale` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10416-10424` `an_invalidated_edge_is_not_live_rationale` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10429-10439` `a_node_without_rationale_returns_none` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10445-10470` `the_batch_covers_the_visible_set_and_keeps_only_nodes_with_rationale` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10475-10492` `the_batch_is_deterministic_across_request_order_and_dedups` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10498-10537` `the_explain_route_returns_the_batch_in_one_request` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10543-10567` `an_absent_explain_leaves_the_graph_route_unchanged` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
- `dash::tests::subject_view_c5` (8 functions)
  - `src/dash.rs:10581-10590` `node` - defined inside `subject_view_c5`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10592-10602` `edge` - defined inside `subject_view_c5`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10609-10634` `subject_graph` - defined inside `subject_view_c5`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10640-10683` `memory_rail_lists_decisions_findings_and_concepts_excluding_lessons` - defined inside `subject_view_c5`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10688-10695` `memory_rail_is_empty_for_a_node_with_no_governing_memory` - defined inside `subject_view_c5`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10716-10768` `memory_rail_concepts_are_live_from_node_realizes_edges_to_a_concept_target_deduped_by_id` - defined inside `subject_view_c5`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10776-10826` `the_seeded_route_carries_memory_without_adding_a_single_node_to_the_neighborhood` - defined inside `subject_view_c5`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10832-10852` `a_cluster_drill_carries_no_memory_field` - defined inside `subject_view_c5`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
- `dash::tests::supervised_lifecycle` (4 functions)
  - `src/dash.rs:4959-4968` `spawn_blocking_child` - defined inside `supervised_lifecycle`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:4973-4982` `watch_for_exit` - defined inside `supervised_lifecycle`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:4985-5001` `reaped_child_reaps_even_when_the_driver_panics` - defined inside `supervised_lifecycle`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:5004-5028` `dropping_the_peers_sidecar_reaps_its_collector_thread` - defined inside `supervised_lifecycle`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
- `main::commands` (37 functions)
  - `src/main.rs:270-273` `cmd_version` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:2183-2191` `cmd_run` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:2295-2377` `cmd_resume_unit` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:2379-2942` `cmd_step` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:3172-3204` `cmd_reported` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:3222-3238` `cmd_prompt` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:3262-3293` `cmd_scratch` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:3920-3927` `cmd_serve` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:3972-4041` `cmd_workflow` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:4110-4170` `cmd_graph` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:4194-4214` `cmd_graph_show` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:4421-4504` `cmd_graph_build` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:4525-4584` `cmd_graph_communities` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:4604-4663` `cmd_graph_concepts` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:4682-4720` `cmd_stats` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:5044-5054` `cmd_stats_canary` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:5413-5536` `cmd_canary` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:5546-5582` `cmd_playbooks` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:5615-5767` `cmd_replay` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:6603-6910` `cmd_dash` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:7362-7391` `cmd_ground` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:7407-7428` `cmd_reindex` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:7445-7473` `cmd_symbols_index` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:7481-7557` `cmd_emit` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:7568-7602` `cmd_progress` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:7652-7810` `cmd_status` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:7913-7944` `cmd_watch` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:8200-8278` `cmd_reset` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:8947-8987` `cmd_peers` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:9194-9290` `cmd_result` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:9426-9597` `cmd_validate` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:11666-11685` `cmd_init` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:12307-12461` `cmd_setup` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:12806-12811` `cmd_docs` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:12951-12985` `cmd_prime` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:13077-13096` `cmd_mcp` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:13476-13516` `cmd_grep_guard` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
- `main::dash_glue` (25 functions)
  - `src/main.rs:111-115` `record_dash_attempt` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6113-6115` `dash_marker_serving` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6195-6215` `spawn_dash_child_process` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6240-6261` `wait_for_dash_bind` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6288-6321` `wait_for_dash_bind_or_diagnose` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6474-6476` `dash_ensure_suppressed` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6483-6485` `dash_ensure_port` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6491-6494` `dash_ensure_port_from` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6555-6559` `recorded_dash_url` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6569-6583` `dash_status_line` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6592-6601` `dash_status_json` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6917-6924` `dash_reap_poll` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6933-6941` `dash_reap_idle_window` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7056-7075` `dash_read_run` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7081-7093` `dash_read_graph` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7104-7112` `dash_read_whole_graph` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7121-7138` `dash_read_calls` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7144-7156` `dash_attach_calls` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7161-7178` `dash_read_progress` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7184-7211` `dash_read_liveness` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7234-7250` `dash_resolve_attach` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7275-7285` `dash_read_sqlite_stream_readonly` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7287-7323` `dash_attach_run` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7330-7342` `dash_attach_inputs` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7348-7354` `dash_attach_graph` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
- `main::docs_overlay` (1 function)
  - `src/main.rs:11819-11826` `apply` - method inside `impl DocsOverlay`; grouped with its other `DocsOverlay` methods.
- `main::liveness` (9 functions)
  - `src/main.rs:2996-3006` `live_branches_for_sweep` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:3037-3053` `terminal_and_no_live_worker` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:7618-7642` `liveness_ages_for_wave` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:8643-8681` `live_writer_reasons` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:8687-8699` `live_writer_refusal` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:8863-8873` `superseded_edge_boundary` - name contains "superseded" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:8895-8925` `superseded_graph_nodes` - name contains "superseded" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:10369-10376` `live_slugs` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:10597-10615` `worktree_belongs_to_live` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
- `main::provenance` (23 functions)
  - `src/main.rs:200-202` `workflow_path` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:3519-3561` `refuse_when_base_lacks_spec_paths` - name contains "spec_" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:3774-3918` `run_workflow` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:3933-3964` `parse_workflow_args` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:9419-9424` `spec_lint_warning_lines` - name contains "spec_" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:9827-9832` `installed_workflow_drifted` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:9839-9841` `workflow_provenance_path` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:9846-9854` `installed_workflow_provenance` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:9873-9884` `git_is_ancestor` - name contains "git_" (git/version provenance); grouped under `main::provenance`.
  - `src/main.rs:9892-9902` `git_commit_distance` - name contains "git_" (git/version provenance); grouped under `main::provenance`.
  - `src/main.rs:10029-10063` `workflow_drift_advisory` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:11752-11762` `install_workflow` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:11791-11793` `docs_overlay_path` - name contains "docs_" (docs overlay); grouped under `main::provenance`.
  - `src/main.rs:11834-11843` `read_docs_overlay` - name contains "docs_" (docs overlay); grouped under `main::provenance`.
  - `src/main.rs:12142-12163` `git_hooks_dir` - name contains "git_" (git/version provenance); grouped under `main::provenance`.
  - `src/main.rs:12725-12770` `docs_context` - name contains "docs_" (docs overlay); grouped under `main::provenance`.
  - `src/main.rs:12826-12845` `docs_drift` - name contains "docs_" (docs overlay); grouped under `main::provenance`.
  - `src/main.rs:12854-12870` `docs_drift_failure` - name contains "docs_" (docs overlay); grouped under `main::provenance`.
  - `src/main.rs:12892-12898` `spec_lint_next_step` - name contains "spec_" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:12919-12923` `spec_lint_reminder_suppressed` - name contains "spec_" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:12931-12943` `spec_lint_reminder_should_print` - name contains "spec_" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:13026-13029` `git_repo` - name contains "git_" (git/version provenance); grouped under `main::provenance`.
  - `src/main.rs:13037-13047` `git_repo_at` - name contains "git_" (git/version provenance); grouped under `main::provenance`.
- `main::render` (8 functions)
  - `src/main.rs:4220-4257` `print_entity_site` - name contains "print_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:4817-4859` `format_stats` - name contains "format_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:4905-4921` `format_progress_line` - name contains "format_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:5083-5179` `format_canary_stats` - name contains "format_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:5963-5973` `format_stats_diff` - name contains "format_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:10830-10860` `format_residue` - name contains "format_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:11512-11524` `print_orientation` - name contains "print_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:13518-13537` `print_run_state` - name contains "print_" (human-readable output); grouped under `main::render`.
- `main::replay_runner` (1 function)
  - `src/main.rs:5984-6004` `run` - method inside `impl Runner for ReplayRunner`; grouped with its other `ReplayRunner` methods.
- `main::residue_report` (1 function)
  - `src/main.rs:10140-10145` `is_empty` - method inside `impl ResidueReport`; grouped with its other `ResidueReport` methods.
- `main::run_registration` (2 functions)
  - `src/main.rs:760-765` `inert` - method inside `impl RunRegistration`; grouped with its other `RunRegistration` methods.
  - `src/main.rs:769-776` `drop` - method inside `impl Drop for RunRegistration`; grouped with its other `RunRegistration` methods.
- `main::scaffold_report` (1 function)
  - `src/main.rs:11368-11374` `changed` - method inside `impl ScaffoldReport`; grouped with its other `ScaffoldReport` methods.
- `main::setup` (20 functions)
  - `src/main.rs:1995-1998` `scratch_defaults` - name contains "scratch" (project setup); grouped under `main::setup`.
  - `src/main.rs:4056-4074` `locate_shim` - name contains "shim" (project setup); grouped under `main::setup`.
  - `src/main.rs:6960-6964` `foreign_instance_scratch_root` - name contains "scratch" (project setup); grouped under `main::setup`.
  - `src/main.rs:10931-10951` `scratch_totals` - name contains "scratch" (project setup); grouped under `main::setup`.
  - `src/main.rs:11113-11148` `classify_agent_scratch` - name contains "scratch" (project setup); grouped under `main::setup`.
  - `src/main.rs:11496-11503` `print_scaffold_pointer` - name contains "scaffold" (project setup); grouped under `main::setup`.
  - `src/main.rs:11633-11664` `scaffold_summary_lines` - name contains "scaffold" (project setup); grouped under `main::setup`.
  - `src/main.rs:11691-11693` `shim_dir` - name contains "shim" (project setup); grouped under `main::setup`.
  - `src/main.rs:11721-11738` `install_file_if_changed` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/main.rs:11770-11772` `skill_source_rel` - name contains "skill" (project setup); grouped under `main::setup`.
  - `src/main.rs:11781-11786` `skill_install_path` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/main.rs:11856-11869` `install_skills` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/main.rs:12175-12203` `install_precommit_hook` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/main.rs:12220-12228` `provision_shim` - name contains "shim" (project setup); grouped under `main::setup`.
  - `src/main.rs:12242-12251` `shim_is_current` - name contains "shim" (project setup); grouped under `main::setup`.
  - `src/main.rs:12256-12263` `write_shim_files` - name contains "shim" (project setup); grouped under `main::setup`.
  - `src/main.rs:12270-12298` `run_npm_install` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/main.rs:12486-12502` `install_lookup_hook` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/main.rs:12511-12525` `install_operator_mcp` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/main.rs:12537-12553` `parse_setup_args` - name contains "setup" (project setup); grouped under `main::setup`.
- `main::store` (32 functions)
  - `src/main.rs:473-491` `store_conn_file` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:548-561` `store_backend_kind` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:611-677` `store_selection_at` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:685-691` `store_selection` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:719-732` `registry_store_identity` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:1249-1320` `migrate_project_identity` - name contains "migrate" (store hygiene); grouped under `main::store`.
  - `src/main.rs:1329-1334` `migrate_local_identity` - name contains "migrate" (store hygiene); grouped under `main::store`.
  - `src/main.rs:1346-1372` `migrate_identity_at` - name contains "migrate" (store hygiene); grouped under `main::store`.
  - `src/main.rs:1762-1764` `find_store_dir_from` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:2009-2014` `server_store_location` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:2034-2100` `require_store_dir` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:2104-2106` `store_file` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:3099-3105` `reclaim_run_scratch` - name contains "reclaim_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:8297-8323` `reset_menu` - name contains "reset_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:8388-8424` `reset_modes` - name contains "reset_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:8446-8465` `reset_build_cache` - name contains "reset_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:8470-8483` `build_cache_reclaim_report` - name contains "reclaim_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:8511-8519` `reset_derived` - name contains "reset_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:8730-8791` `refuse_derived_reset_if_live` - name contains "reset_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:8818-8849` `reset_runs` - name contains "reset_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:9323-9338` `reclaim_spawn_scratch` - name contains "reclaim_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:9365-9382` `reclaim_spawn_registered_scratch` - name contains "reclaim_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:9750-9760` `bloat_advisory` - name contains "bloat" (store hygiene); grouped under `main::store`.
  - `src/main.rs:9773-9784` `bloat_advisory_for` - name contains "bloat" (store hygiene); grouped under `main::store`.
  - `src/main.rs:10404-10493` `reclaim_orphan_scratch` - name contains "reclaim_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:10768-10809` `reclaim_shared_build_cache` - name contains "reclaim_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:10968-11011` `scratch_footprint` - name contains "footprint" (store hygiene); grouped under `main::store`.
  - `src/main.rs:11021-11044` `store_and_backup_bytes` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:11165-11242` `footprint_report` - name contains "footprint" (store hygiene); grouped under `main::store`.
  - `src/main.rs:11248-11253` `footprint_report_lines` - name contains "footprint" (store hygiene); grouped under `main::store`.
  - `src/main.rs:11262-11282` `footprint_advisories` - name contains "footprint" (store hygiene); grouped under `main::store`.
  - `src/main.rs:11313-11340` `footprint_report_for` - name contains "footprint" (store hygiene); grouped under `main::store`.
- `main::store_location` (3 functions)
  - `src/main.rs:1938-1940` `file` - method inside `impl StoreLocation`; grouped with its other `StoreLocation` methods.
  - `src/main.rs:1946-1953` `identity` - method inside `impl StoreLocation`; grouped with its other `StoreLocation` methods.
  - `src/main.rs:1966-1972` `repo_root` - method inside `impl StoreLocation`; grouped with its other `StoreLocation` methods.
- `main::store_selection` (1 function)
  - `src/main.rs:434-436` `is_sqlite` - method inside `impl StoreSelection`; grouped with its other `StoreSelection` methods.
- `main::support` (28 functions)
  - `src/main.rs:321-397` `parse_run_args` - name contains "parse_" (argument/input parsing); grouped under `main::support`.
  - `src/main.rs:408-416` `resolve_run_base` - name contains "resolve" (path/id resolution helper); grouped under `main::support`.
  - `src/main.rs:498-500` `conn_file_is_group_or_other_readable` - name contains "is_" (predicate helper); grouped under `main::support`.
  - `src/main.rs:508-530` `warn_if_conn_file_is_exposed` - name contains "is_" (predicate helper); grouped under `main::support`.
  - `src/main.rs:568-591` `resolve_conn` - name contains "resolve" (path/id resolution helper); grouped under `main::support`.
  - `src/main.rs:699-709` `resolve_store` - name contains "resolve" (path/id resolution helper); grouped under `main::support`.
  - `src/main.rs:979-988` `read_project_id` - name contains "read_" (generic read helper); grouped under `main::support`.
  - `src/main.rs:1810-1833` `resolve_main_worktree_or_refuse` - name contains "resolve" (path/id resolution helper); grouped under `main::support`.
  - `src/main.rs:3351-3394` `parse_step_args` - name contains "parse_" (argument/input parsing); grouped under `main::support`.
  - `src/main.rs:5189-5201` `read_model_drift` - name contains "read_" (generic read helper); grouped under `main::support`.
  - `src/main.rs:5287-5299` `read_order_signatures` - name contains "read_" (generic read helper); grouped under `main::support`.
  - `src/main.rs:5323-5395` `parse_canary_args` - name contains "parse_" (argument/input parsing); grouped under `main::support`.
  - `src/main.rs:5844-5876` `parse_replay_args` - name contains "parse_" (argument/input parsing); grouped under `main::support`.
  - `src/main.rs:6128-6154` `ensure_run_dashboard_at` - name contains "ensure_" (invariant helper); grouped under `main::support`.
  - `src/main.rs:6514-6549` `ensure_run_dashboard` - name contains "ensure_" (invariant helper); grouped under `main::support`.
  - `src/main.rs:7856-7886` `parse_watch_args` - name contains "parse_" (argument/input parsing); grouped under `main::support`.
  - `src/main.rs:9058-9116` `parse_result_args` - name contains "parse_" (argument/input parsing); grouped under `main::support`.
  - `src/main.rs:9156-9170` `read_outcome_from_stdin` - name contains "read_" (generic read helper); grouped under `main::support`.
  - `src/main.rs:10245-10273` `read_run_units` - name contains "read_" (generic read helper); grouped under `main::support`.
  - `src/main.rs:10619-10621` `is_uuid8` - name contains "is_" (predicate helper); grouped under `main::support`.
  - `src/main.rs:10628-10657` `find_shadow_stores` - name contains "find_" (lookup helper); grouped under `main::support`.
  - `src/main.rs:10720-10725` `is_build_cache_tombstone` - name contains "is_" (predicate helper); grouped under `main::support`.
  - `src/main.rs:11543-11592` `write_gitignore_entries` - name contains "write_" (generic write helper); grouped under `main::support`.
  - `src/main.rs:12044-12049` `find_bytes` - name contains "find_" (lookup helper); grouped under `main::support`.
  - `src/main.rs:12780-12798` `write_docs` - name contains "write_" (generic write helper); grouped under `main::support`.
  - `src/main.rs:13344-13356` `resolve_path_segments` - name contains "resolve" (path/id resolution helper); grouped under `main::support`.
  - `src/main.rs:13367-13377` `path_is_root_or_ancestor` - name contains "is_" (predicate helper); grouped under `main::support`.
  - `src/main.rs:13547-13554` `write_if_absent` - name contains "write_" (generic write helper); grouped under `main::support`.
- `main::tests` (370 functions)
  - `src/main.rs:12131-12136` `compose_precommit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:13756-13763` `spec_lint_next_step_names_rigger_validate_and_the_given_spec_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:13771-13773` `spec_lint_reminder_suppressed_when_env_names_the_real_direct_parent_pid` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:13779-13800` `spec_lint_reminder_prints_on_absent_foreign_stale_or_malformed_env` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:13817-13839` `spec_lint_warning_lines_formats_every_advisory_with_the_spec_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:13844-13847` `spec_lint_warning_lines_is_empty_on_a_clean_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:13857-13910` `ensure_run_dashboard_at_starts_once_then_short_circuits_on_a_live_marker` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:13919-13925` `dash_marker_serving_reports_false_when_nothing_answers_the_markers_port` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:13930-13962` `ensure_run_dashboard_at_restarts_when_the_recorded_dash_is_gone` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:13970-13989` `ensure_run_dashboard_at_reports_failed_when_the_start_errors` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14001-14040` `wait_for_dash_bind_confirms_promptly_once_the_port_answers_as_a_dash` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14047-14074` `wait_for_dash_bind_fails_fast_when_the_child_exits_before_confirming` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14088-14130` `wait_for_dash_bind_times_out_against_a_real_held_port` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14153-14197` `wait_for_dash_bind_or_diagnose_never_self_attributes_its_own_still_starting_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14214-14242` `ensure_run_dashboard_at_writes_no_marker_when_the_real_spawn_never_confirms_a_bind` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14257-14302` `ensure_run_dashboard_at_names_the_held_ports_holder_when_the_real_spawn_fails` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14322-14346` `ensure_run_dashboard_at_never_claims_a_phantom_holder_for_an_unheld_port` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14358-14389` `ensure_run_dashboard_at_self_heals_a_marker_naming_a_dead_pid` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14399-14433` `ensure_run_dashboard_at_self_heals_a_marker_naming_a_live_pid_whose_port_is_unserved` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14446-14484` `ensure_run_dashboard_at_never_revisits_a_still_serving_unattributed_pid_marker` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14491-14512` `dash_ensure_is_suppressed_by_either_opt_out_and_proceeds_when_neither_is_set` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14522-14553` `dash_status_line_renders_each_outcome_to_its_exact_text` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14561-14589` `dash_status_json_renders_each_outcome_to_its_exact_shape` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14598-14625` `dash_ensure_port_defaults_to_the_fixed_address_and_only_a_valid_override_relocates_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14635-14668` `compose_precommit_fresh_install_carries_the_managed_block` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14678-14705` `precommit_block_refuses_on_drift_instead_of_staging` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14719-14820` `precommit_block_resolves_a_tree_built_binary_before_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14827-14839` `compose_precommit_is_idempotent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14850-14879` `compose_precommit_chains_without_clobbering_an_existing_hook` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14889-14903` `compose_precommit_prepends_before_a_terminal_exit_existing_hook` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14912-14963` `install_precommit_hook_preserves_a_non_utf8_existing_hook` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14970-14993` `compose_precommit_refreshes_a_stale_block_in_place` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15001-15020` `cmd_peers_prints_live_or_historical_per_decision_provenance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15038-15081` `superseded_graph_nodes_drops_dead_runs_and_preboundary_keeping_lessons_active_and_reused_ids` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15040-15042` `ev` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15043-15048` `run_started` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15090-15128` `superseded_edge_boundary_is_the_active_runs_start_or_none_without_a_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15092-15098` `run_started_at` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15099-15105` `decision` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15156-15414` `the_denoise_leaves_metrics_run_pruning_and_blast_radius_unaffected` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15162-15166` `ev` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15424-15442` `version_line_carries_the_derived_version_and_a_non_empty_build_provenance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15449-15500` `docs_context_reads_every_fact_from_code` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15507-15548` `docs_render_surfaces_known_code_facts_verbatim` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15554-15573` `commands_registry_is_well_formed_and_covers_dispatch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15581-15611` `write_docs_writes_every_registry_skill_plus_the_handbook` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15621-15658` `install_and_docs_each_cover_exactly_the_registry_no_more_no_less` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15667-15706` `per_operation_skills_reference_only_real_subcommands` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15716-15744` `watching_discipline_skills_reference_only_real_subcommands` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15752-15819` `docs_drift_flags_a_changed_file_and_skips_absent_or_in_sync_ones` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15830-15856` `committed_registry_docs_are_in_sync_with_a_fresh_render` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15867-15896` `planning_field_guide_page_renders_and_is_linked_from_authoring_loops` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15907-15974` `status_and_dashboard_render_the_same_current_blocker_lines` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15987-16042` `release_ready_lines_surface_only_on_a_done_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16045-16111` `status_and_dash_read_the_runs_persisted_base_not_a_re_resolution` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16116-16137` `dirty_tracked_paths_keeps_tracked_modifications_and_drops_untracked_and_ignored` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16140-16142` `dirty_tracked_paths_on_a_clean_tree_is_empty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16145-16170` `installed_workflow_drifted_is_false_when_absent_or_identical_and_true_on_drift` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16175-16222` `drift_side_names_the_binary_stale_only_when_the_installed_workflow_is_provably_newer` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16225-16273` `workflow_drift_advisory_names_which_side_is_stale_and_never_says_they_differ` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16276-16320` `git_is_ancestor_decides_commit_order_in_a_real_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16323-16369` `git_commit_distance_counts_commits_ahead_in_a_real_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16372-16386` `missing_gitsemver_binary_advisory_fires_only_on_the_unversioned_marker` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16389-16394` `behind_the_tree_message_is_silent_when_versions_already_match` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16397-16407` `behind_the_tree_message_is_silent_when_either_side_is_unversioned` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16410-16419` `behind_the_tree_message_is_silent_on_an_undecidable_or_zero_distance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16422-16429` `behind_the_tree_message_names_both_versions_and_the_commit_distance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16436-16442` `gitsemver_available` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16446-16461` `behind_the_tree_git` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16463-16475` `behind_the_tree_git_output` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16478-16516` `behind_the_tree_advisory_names_the_real_derived_version_ahead_of_the_installed_commit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16519-16541` `behind_the_tree_advisory_is_silent_when_the_checkout_has_not_moved` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16544-16557` `install_workflow_records_the_build_provenance_beside_the_workflow` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16560-16575` `validate_advisories_warns_on_workflow_drift_naming_the_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16581-16583` `slugs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16585-16588` `write_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16593-16621` `init_committed_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16639-16655` `resolve_main_worktree_or_refuse_returns_exactly_git_rev_parse_show_toplevel` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16658-16682` `refuse_when_base_lacks_spec_paths_refuses_on_total_absence_and_names_a_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16685-16702` `refuse_when_base_lacks_spec_paths_proceeds_when_the_base_contains_them` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16705-16723` `refuse_when_base_lacks_spec_paths_partial_match_warns_and_proceeds` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16726-16766` `refuse_when_base_lacks_spec_paths_skips_without_tokens_or_off_a_fresh_from_base_anchor` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16769-16850` `refuse_when_base_unreachable_fails_loudly_only_when_no_reachable_base` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16853-16860` `human_size_formats_bytes_through_gib` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16863-16869` `is_uuid8_accepts_exactly_eight_hex_digits` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16872-16917` `worktree_belongs_to_live_matches_both_naming_shapes_without_prefix_false_match` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16920-16967` `current_run_units_scopes_to_the_current_run_and_splits_live_from_dead` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16970-17007` `current_run_units_spares_a_terminal_units_branch_whose_latest_spawn_is_still_in_flight` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17010-17036` `current_run_units_still_retires_a_terminal_unit_once_its_latest_spawn_answers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17039-17086` `current_run_units_splits_live_spawns_from_answered_ones_scoped_to_the_current_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17098-17124` `live_branches_for_sweep_fails_closed_on_an_unreadable_stream_but_folds_a_readable_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17127-17167` `find_shadow_stores_finds_nested_events_db_and_prunes_build_caches` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17170-17181` `dir_size_bytes_sums_files_recursively` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17186-17202` `reclaim_shared_build_cache_deletes_a_populated_cache_and_reports_its_bytes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17205-17223` `reclaim_shared_build_cache_is_idempotent_zero_report_when_absent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17226-17254` `reclaim_shared_build_cache_refuses_rather_than_waits_when_a_build_holds_the_guard` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17257-17280` `reclaim_shared_build_cache_releases_the_guard_promptly_after_a_successful_reclaim` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17283-17356` `scan_residue_reports_dead_worktrees_caches_shadows_and_branches` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17359-17378` `scan_residue_is_empty_when_everything_is_live_and_no_shadow_stores` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17381-17398` `format_residue_renders_a_sized_warning_block` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17403-17422` `store_and_backup_bytes_splits_live_store_files_from_dot_bak_backups` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17425-17429` `store_and_backup_bytes_is_zero_on_a_missing_directory` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17432-17474` `scratch_footprint_totals_every_entry_and_dead_only_the_non_live_share` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17477-17542` `footprint_report_measures_every_category_on_a_seeded_fixture_tree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17545-17562` `footprint_report_folds_a_none_mutation_root_to_a_zero_contribution` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17565-17647` `footprint_report_flags_registered_scratch_roots_dead_share_and_spares_a_live_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17660-17714` `footprint_report_keys_agent_scratch_liveness_by_run_id_and_leaf_not_leaf_name_alone` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17717-17722` `looks_like_run_container_true_when_every_direct_child_is_a_directory` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17725-17733` `looks_like_run_container_false_when_a_direct_child_is_a_bare_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17736-17744` `looks_like_run_container_is_true_on_an_empty_or_missing_dir` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17747-17760` `classify_agent_scratch_separates_a_bare_top_level_file_from_a_well_formed_container` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17763-17842` `footprint_report_reports_a_top_level_adhoc_agent_scratch_dir_as_its_own_category_never_folded_into_the_dead_run_bucket` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17845-17869` `footprint_report_lines_reports_every_categorys_total_unconditionally` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17872-17883` `footprint_advisories_flags_a_category_whose_dead_share_reaches_the_threshold` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17886-17894` `footprint_advisories_is_silent_below_the_threshold` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17897-17914` `footprint_advisories_flags_a_category_exactly_at_the_threshold_boundary` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17917-17927` `footprint_advisories_never_flags_a_category_with_no_reclaim_command` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17930-17938` `footprint_advisories_is_silent_on_an_empty_category` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17941-17964` `owning_repo_root_prefers_the_stores_own_root_over_the_git_toplevel` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17969-18092` `reclaim_orphan_scratch_removes_non_live_owned_scratch_and_spares_live_and_shared_areas` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18095-18160` `reclaim_orphan_scratch_spares_a_non_live_worktree_that_is_still_dirty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18163-18202` `reclaim_orphan_scratch_spares_only_a_declared_dirty_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18205-18235` `reclaim_orphan_scratch_reaps_a_stray_build_cache_tombstone_unconditionally` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18240-18279` `leaked_process_advisories_name_a_process_rooted_under_the_scratch_root` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18282-18292` `leaked_process_advisories_is_empty_when_no_process_is_rooted_under_the_scratch_root` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18295-18302` `leaked_process_advisories_is_a_graceful_no_op_when_the_scratch_root_is_absent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18307-18313` `parse_result_takes_an_id_and_an_optional_output_arg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18316-18321` `parse_result_with_no_output_defers_to_stdin` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18326-18332` `find_store_dir_from_returns_the_dir_that_holds_the_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18335-18347` `find_store_dir_from_walks_up_from_a_subdirectory` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18350-18356` `git_init_quiet` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18359-18396` `find_store_dir_from_never_escapes_the_repo_into_a_parent_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18399-18474` `reap_then_remove_dir_reaps_processes_rooted_inside_then_removes_the_dir` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18477-18526` `reclaim_run_scratch_removes_the_run_level_areas_and_spares_per_unit_scratch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18529-18562` `reclaim_run_scratch_spares_the_shared_build_cache_while_a_build_holds_its_guard` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18565-18576` `find_store_dir_from_refuses_the_worktree_shape_with_no_events_db` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18579-18608` `find_store_dir_from_walks_past_a_storeless_rigger_to_the_real_store_above` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18611-18664` `find_store_dir_from_resolves_the_owning_repo_even_when_the_worktree_lives_outside_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18667-18721` `find_store_dir_from_never_climbs_a_relocated_worktrees_own_unrelated_ancestors_into_a_foreign_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18724-18751` `walk_stores_from_prefers_the_outermost_store_over_a_nearer_shadow` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18754-18772` `walk_stores_from_reports_no_shadow_for_a_single_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18781-18850` `require_store_dir_pins_to_the_fence_env_and_never_reaches_the_live_store_above_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18784-18787` `drop` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18854-18876` `require_store_dir_fence_is_off_by_default` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18860-18862` `drop` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18881-18887` `result_advisories_flags_an_orphan_id_with_no_spawn_request` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18890-18915` `result_advisories_orphan_wording_is_plain_on_record_and_conditional_under_if_absent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18918-18928` `result_advisories_is_silent_for_a_parked_unanswered_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18931-18942` `result_advisories_flags_a_supersede_with_the_prior_result_position` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18945-18958` `result_advisories_suppresses_the_supersede_note_when_not_superseding` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18961-18973` `result_advisories_flags_both_orphan_and_supersede` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18976-18996` `parse_result_error_flag_is_order_independent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18999-19026` `parse_result_if_absent_is_off_by_default_and_a_bare_order_independent_flag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19029-19064` `parse_result_meta_must_be_a_json_object` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19067-19081` `parse_result_rejects_missing_id_extra_args_and_unknown_flags` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19084-19098` `build_result_shapes_success_and_failure` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19101-19106` `build_result_rejects_a_blank_error_message` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19109-19118` `build_result_attaches_meta` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19121-19159` `a_recorded_result_lets_the_replay_driver_advance_past_the_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19162-19194` `a_recorded_error_result_replays_as_a_failure_not_a_fake_success` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19200-19290` `scaffold_parses_into_a_valid_config` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19304-19321` `shipped_workflows_carry_a_non_zero_spawn_budget` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19324-19336` `parse_canary_args_defaults_corpus_and_jobs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19339-19351` `parse_canary_args_reads_corpus_if_model_changed_and_jobs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19354-19366` `parse_canary_args_rejects_a_non_positive_jobs_value` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19369-19379` `parse_canary_args_rejects_unknown_flags` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19382-19411` `usage_text_gives_the_jobs_flag_its_own_description_line` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19414-19431` `usage_text_names_the_build_cache_reset_mode` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19434-19444` `parse_run_args_defaults_to_cli_and_an_unset_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19447-19458` `parse_run_args_reads_fresh_alongside_a_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19461-19471` `parse_run_args_reads_rebase_definition` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19474-19489` `parse_run_args_reads_driver_eventstore_conn_and_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19492-19497` `parse_run_args_rejects_unknown_flags_and_values` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19504-19530` `parse_run_args_accepts_base_alongside_a_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19538-19560` `resolve_run_base_precedence_flag_then_env_then_default` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19566-19604` `parse_workflow_args_reads_spec_and_base` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19609-19661` `parse_step_args_reads_spec_and_base_with_default` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19667-19711` `definition_hash_is_stable_and_content_sensitive` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19720-19747` `kurrentdb_is_always_available_and_needs_a_conn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19759-19790` `store_selection_preserves_a_credentialed_tls_conn_verbatim` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19801-19978` `store_selection_precedence_flag_env_secret_file_config_then_default` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19981-19983` `project_identity_is_never_empty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19986-20011` `project_identity_reads_the_tracked_id_file_then_falls_back_to_the_basename` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20014-20041` `ssh_https_and_git_suffix_forms_of_one_repo_mint_identical_ids` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20044-20058` `normalize_origin_url_separates_distinct_repos_and_lowercases_only_the_host` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20061-20090` `decide_migration_covers_every_case` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20093-20132` `migrate_project_identity_renames_legacy_history_and_records_a_decision` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20135-20174` `migrate_project_identity_refuses_when_both_namespaces_hold_history` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20177-20242` `migrate_project_identity_rekeys_graph_rows_so_pre_mint_history_is_not_orphaned` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20245-20325` `migrate_project_identity_rekeys_the_graph_before_the_irreversible_stream_rename` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20328-20423` `migrate_project_identity_recovers_from_a_crash_between_the_rekey_and_the_rename` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20432-20537` `dash_graph_provider_reaches_the_whole_projection_when_run_seeds_are_empty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20551-20590` `dash_read_whole_graph_on_an_absent_db_is_empty_and_creates_nothing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20596-20628` `setup_provisions_the_shim_runtime_files` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20637-20696` `setup_installs_refreshes_and_is_a_noop_on_the_native_rigger_workflow` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20701-20710` `outcome_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20714-20719` `registry_entry` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20729-20799` `setup_installs_the_using_rigger_skill_distinct_from_the_workflow` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20817-20901` `planning_a_spec_installs_and_renders_through_the_registry_with_no_planning_specific_code` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20911-20947` `setup_skill_install_applies_the_project_overlay` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20953-20979` `docs_overlay_overrides_only_declared_fields` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20984-20994` `docs_overlay_malformed_is_a_loud_error` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21004-21032` `provision_shim_is_a_silent_noop_when_already_current` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21042-21061` `shim_is_not_current_when_node_modules_is_torn_missing_the_install_marker` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21068-21090` `init_project_is_idempotent_reporting_new_work_only_once` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21100-21158` `scaffold_summary_reports_only_the_gitignore_change_on_a_gitignore_only_repair` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21169-21248` `init_project_gitignores_the_dash_runtime_breadcrumbs_idempotently` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21257-21297` `init_project_gitignores_the_store_conn_secret_file_idempotently` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21306-21320` `a_group_or_other_readable_secret_file_mode_is_flagged_owner_only_is_not` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21332-21380` `init_project_still_writes_the_dash_ignore_lines_when_a_broader_rule_covers_them` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21389-21451` `scaffold_agents_and_workflow_reference_the_same_canonical_set` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21458-21501` `init_scaffolds_only_the_workflow_referenced_agents` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21508-21537` `get_referenced_agent_ids_reads_the_scaffolded_workflows_fleet` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21544-21576` `write_if_absent_wrote_kept_and_errors_naming_the_artifact` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21584-21601` `parse_setup_args_reads_the_agents_directory_flag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21608-21653` `import_agents_copies_and_normalizes_the_identity_field` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21659-21696` `import_agents_refuses_to_overwrite_an_existing_agent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21702-21715` `import_agents_validates_and_rejects_a_malformed_agent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21724-21746` `import_agents_rejects_an_id_colliding_with_an_existing_agent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21752-21772` `import_agents_rejects_a_duplicate_id_within_one_import` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21778-21799` `import_agents_rejects_an_agent_with_a_blank_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21806-21828` `import_agents_runs_full_validation_and_rejects_a_broken_project` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21834-21853` `meta_object_body` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21862-21876` `meta_description` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21883-21891` `strip_line_comments` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21905-22128` `workflow_is_a_thin_courier_driver_with_per_unit_phase_labels` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22137-22148` `the_step_schema_admits_the_attention_array` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22154-22176` `js_function_body` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22192-22247` `phase_of_maps_wave_items_to_the_meta_phase_by_role_and_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22260-22298` `meta_matches_reality_drops_integrate_and_the_unit_stage_construction` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22317-22408` `the_driver_relays_each_attention_entry_as_a_narrator_log_line` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22415-22428` `merge_hung_attention_does_nothing_when_not_newly_hung` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22436-22448` `merge_hung_attention_defers_to_an_existing_budget_halt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22457-22493` `merge_hung_attention_lands_in_canonical_position_alongside_other_signals` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22511-22559` `workflow_step_courier_prompt_is_foreground_and_honest` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22569-22578` `step_courier_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22596-22663` `workflow_step_courier_waits_on_an_auto_backgrounded_step` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22678-22733` `workflow_driver_guards_a_null_step_before_dereferencing_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22746-22803` `workflow_meta_description_is_a_user_facing_tagline_free_of_plumbing_terms` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22810-22834` `setup_runs_npm_install_or_reports_a_clear_error` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22840-22865` `workflow_locates_the_provisioned_shim_or_tells_you_to_run_setup` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22867-22873` `npm_available` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22884-22927` `format_stats_prints_all_four_metrics` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22933-22949` `format_stats_handles_zeroed_metrics_without_nan` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22955-22972` `format_canary_stats_reports_findings_raised_by_tier` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22977-22987` `format_canary_stats_reports_a_zero_findings_count_honestly` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22993-22999` `format_canary_stats_omits_the_findings_volume_section_when_empty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23004-23021` `progress_outcome` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23028-23040` `format_progress_line_names_id_verdict_and_none_when_nothing_caught` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23046-23063` `format_progress_line_reports_a_wrong_verdict_and_every_catching_tier` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23071-23102` `format_canary_stats_renders_na_for_a_tier_with_unattributed_correct_rejects` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23109-23128` `format_canary_stats_renders_the_real_zero_when_attribution_was_fully_measured` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23137-23156` `format_canary_stats_reports_control_items_and_false_positives` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23164-23179` `format_canary_stats_reports_zero_false_positives_honestly` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23186-23215` `format_canary_stats_reports_the_model_pinning_header_when_present` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23221-23227` `format_canary_stats_omits_the_model_pinning_header_when_the_run_never_recorded_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23237-23279` `format_stats_surfaces_parallelism_retention_and_warns_below_the_floor` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23286-23326` `format_stats_surfaces_spawn_timing_and_reports_unpaired_separately` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23332-23338` `format_stats_spawn_timing_reports_no_spawns_when_none_recorded` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23345-23372` `format_stats_spawn_timing_no_spawns_line_requires_both_empty_and_zero_unpaired` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23379-23394` `format_stats_spawn_timing_all_unpaired_is_not_reported_as_no_spawns` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23402-23429` `parallelism_retention_line_is_single_sourced_and_warns_below_the_floor` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23436-23481` `stats_discloses_when_no_verdict_was_recorded_on_this_driver` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23492-23565` `stats_discloses_unfed_numerator_when_verdict_recorded_but_findings_unattributed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23573-23610` `stats_discloses_cause_split_remainder_when_fewer_causes_than_rejects` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23615-23621` `cmd_stats_rejects_extra_arguments` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23624-23669` `baseline_run_slice_selects_a_run_by_id_including_a_middle_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23672-23703` `format_stats_diff_flags_only_the_changed_rows` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23715-23733` `build_environment_report_with_a_wrapper_lists_wrapper_cache_dir_and_budget` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23740-23757` `build_environment_report_with_no_wrapper_omits_cache_dir_but_keeps_budget` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23763-23773` `build_environment_report_zero_max_concurrent_reports_unlimited` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23779-23788` `build_environment_report_reports_mutation_gate_declared` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23794-23803` `build_environment_report_reports_mutation_gate_not_configured` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23821-23840` `order_signature_advisories_names_the_stream_count_range_and_repair_doc` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23844-23846` `order_signature_advisories_is_empty_when_no_signatures_are_given` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23854-23860` `drift_change` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23866-23895` `model_drift_advisory_is_a_soft_note_for_snapshot_only_drift` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23900-23911` `model_drift_advisory_stays_a_warning_when_any_change_is_a_real_model_repoint` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23915-23917` `model_drift_advisory_is_none_when_nothing_changed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23922-23938` `index_staleness_message_names_every_kind_of_disagreement_and_the_fix` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23941-23964` `bloat_advisory_is_none_at_or_below_the_threshold_and_named_above_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23972-23987` `assert_advisory_for_never_fabricates_a_missing_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23990-23992` `bloat_advisory_for_never_fabricates_a_store_that_does_not_exist` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23997-24015` `retired_entities_advisory_is_none_at_zero_and_named_with_correct_pluralization` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24018-24023` `retired_entities_advisory_for_never_fabricates_a_graph_that_does_not_exist` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24026-24060` `retired_entities_advisory_for_reads_the_projectors_own_counting_authority` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24067-24074` `scaffold_workflow_declares_build_wrapper_auto` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24082-24096` `init_project_never_clobbers_an_existing_build_section` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24099-24116` `parse_replay_args_requires_a_run_and_a_rev_in_either_order` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24128-24149` `cmd_stats_on_a_never_run_project_says_no_runs_and_creates_no_db` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24135-24137` `drop` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24157-24204` `a_second_concurrent_rigger_step_refuses_and_the_lock_frees_on_release` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24162-24164` `drop` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24210-24213` `no_runs_message_points_at_rigger_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24219-24226` `seed_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24232-24244` `stats_lines_absent_db_returns_none_and_creates_no_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24251-24267` `stats_lines_existing_db_with_empty_run_stream_returns_none` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24275-24304` `stats_lines_does_not_read_another_projects_namespaced_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24311-24342` `stats_lines_existing_run_renders_metric_lines` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24365-24434` `stats_lines_pairs_recorded_spawn_request_and_result_into_spawn_timing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24442-24454` `result_of_at_absent_db_reads_as_unreported_and_creates_no_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24460-24480` `result_of_at_unrecorded_spawn_reads_as_unreported` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24488-24512` `result_of_at_reads_a_self_reported_result_so_it_is_not_clobbered` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24518-24547` `result_of_at_is_namespace_scoped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24554-24566` `cmd_reported_requires_exactly_one_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24577-24590` `pgid_of` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24601-24626` `detach_process_group_places_the_child_in_its_own_process_group` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24635-24656` `a_child_spawned_without_detachment_inherits_the_parent_process_group` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24670-24690` `spawn_run_dashboard_detached_session_detaches_the_dash` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24705-24725` `report_of` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24736-24759` `a_compaction_that_failed_after_the_deletes_is_reported_beside_the_counts` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24770-24793` `a_prune_that_shed_nothing_is_justified_by_this_log_not_by_when_it_was_written` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24804-24815` `a_pass_that_deleted_nothing_but_reclaimed_space_reports_the_reclamation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24820-24851` `a_prune_that_shed_rows_explains_the_duplication_a_deduplicated_log_still_accumulates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24865-24900` `an_unmeasurable_database_is_not_reported_as_a_checkpoint_a_reader_declined` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24904-24906` `no_live_units` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24910-24928` `live_writer_reasons_is_empty_only_when_all_four_facts_are_quiet` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24934-24953` `refusal_names_a_held_step_lock_and_the_force_live_override_owning_the_risk` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24958-24966` `refusal_names_a_non_terminal_unit_between_spawn_rounds` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24970-24982` `refusal_names_every_in_flight_spawn_id_and_the_count` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24986-24993` `refusal_names_the_driver_registration_count` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24998-25010` `refusal_names_every_applicable_reason_together_not_just_the_first` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25016-25045` `refuse_derived_reset_if_live_fails_safe_on_a_malformed_spawn_event` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25051-25072` `reset_modes_parses_force_live_alongside_derived_rejects_duplicates_and_never_implies_a_mode` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25079-25096` `reset_modes_parses_build_cache_alone_and_composed_and_rejects_duplicates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25104-25113` `watch_test_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25136-25169` `store_location_repo_root_resolves_the_owning_root_not_the_process_cwd_so_a_real_marker_is_found` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25183-25214` `scratch_defaults_reads_the_owning_roots_config_with_no_agents_fleet_present` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25217-25242` `watch_once_on_a_clean_store_reports_no_anomalies` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25245-25249` `parse_watch_args_defaults_to_streaming_with_the_default_interval` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25256-25278` `parse_watch_args_accepts_once_and_interval_together_in_either_order` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25281-25285` `parse_watch_args_rejects_a_non_integer_interval_a_missing_value_and_an_unknown_flag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25293-25413` `watch_once_on_the_seeded_store_reports_one_line_per_anomaly` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25420-25439` `the_watchdog_command_signal_set_covers_every_signal_the_watch_skill_names` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25447-25475` `watch_once_reports_dash_not_serving_when_the_marker_names_a_dead_holder` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25482-25512` `watch_once_reports_no_anomaly_when_the_dash_marker_names_a_real_serving_holder` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25517-25540` `runs_menu_line_names_the_measured_counts_and_the_flag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25543-25572` `derived_menu_line_sums_the_measured_duplicate_counts_and_names_the_flag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25579-25594` `derived_menu_line_on_a_server_backend_says_so_instead_of_a_fabricated_count` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25613-25706` `implementer_persona_pins_the_checkin_stage_kill_or_justify_contract` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25714-25756` `implementer_persona_pins_the_checkpoint_before_long_work_contract` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25767-25797` `no_persona_under_rigger_agents_invokes_cargo_mutants` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25810-25845` `install_operator_mcp_installs_refreshes_and_is_a_noop` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25851-25889` `install_lookup_hook_installs_refreshes_and_is_a_noop_and_preserves_foreign_hooks` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25897-25921` `assert_allows_with_literal_stripped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25929-25950` `grep_guard_decision_bounces_bash_grep_over_guarded_trees_and_passes_literal` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25956-25974` `grep_guard_decision_covers_whole_project_grep_and_ignores_unrelated_targets` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25980-25994` `grep_guard_decision_allows_non_grep_bash_commands` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26000-26014` `grep_guard_decision_bounces_the_grep_tool_over_guarded_trees` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26018-26023` `grep_guard_decision_ignores_other_tools` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26032-26046` `grep_guard_decision_bounces_a_bare_tree_name_with_no_trailing_slash` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26055-26083` `grep_guard_decision_bounces_an_absolute_path_under_a_guarded_tree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26089-26097` `grep_guard_decision_ignores_a_merely_prefixed_or_unrelated_segment` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26109-26125` `grep_guard_decision_bounces_a_shell_metacharacter_fused_grep` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26131-26138` `grep_guard_decision_still_allows_literal_on_a_shell_metacharacter_fused_grep` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26148-26161` `grep_guard_decision_bounces_a_redirect_metacharacter_fused_grep` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26166-26178` `grep_guard_decision_still_allows_literal_on_a_redirect_metacharacter_fused_grep` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26188-26201` `grep_guard_decision_bounces_a_quoted_or_escaped_grep` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26207-26221` `grep_guard_decision_still_allows_a_quoted_literal_on_a_quoted_grep` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26231-26241` `grep_guard_decision_bounces_a_grep_split_by_a_line_continuation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26245-26252` `grep_guard_decision_still_allows_literal_on_a_line_continuation_split_grep` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26260-26272` `grep_guard_decision_bounces_a_path_qualified_grep` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26276-26286` `grep_guard_decision_still_allows_literal_on_a_path_qualified_grep` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26291-26301` `grep_guard_decision_allows_a_path_qualified_non_grep_command` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26312-26330` `grep_guard_decision_bounces_a_relative_ancestor_of_the_project_root` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26337-26352` `grep_guard_decision_allows_a_relative_sibling_reached_via_dot_dot` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26363-26382` `grep_guard_decision_bounces_the_absolute_project_root_itself` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26388-26398` `grep_guard_decision_bounces_an_absolute_ancestor_of_the_project_root` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26405-26425` `grep_guard_decision_allows_an_absolute_path_unrelated_to_the_project_root` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26432-26438` `grep_guard_decision_with_unknown_project_root_falls_back_to_segment_matching` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).

### Unassigned (183 functions)

- `src/conductor.rs:376-378` `adoption_provenance_key` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:413-442` `glob_matches` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:596-612` `input_digest` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:833-850` `conflict_resolution_prompt` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:1084-1086` `conflict_regenerate_key` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:1753-1765` `replay_trajectory` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:1769-2389` `run` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:2468-2560` `compute_attention` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:10843-10850` `fnv1a_64` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:10855-10864` `verified_evidence` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:11207-11254` `render_capped_section` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:11435-11438` `plain_node_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:11569-11585` `confidence_tier_radius` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:11595-11649` `files_reachable` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:11908-11916` `current_run_spec` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:12065-12082` `branch_owner` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:12121-12127` `quarantine_branch_name` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:12199-12227` `quarantined_branch` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:12258-12263` `same_path` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:12270-12285` `sanitize_for_path` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:12290-12292` `integrates` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:12471-12480` `fan_out_template_name` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:12513-12519` `implement_slice` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:12603-12608` `producer_name` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:12707-12715` `fan_out_lenses` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:12754-12776` `coverage_gap` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:12839-12854` `need_satisfied` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:12896-12933` `validate_acyclic` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:62-72` `free_port_from` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:128-160` `probe_dash_head` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:174-178` `dash_serving_on` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:287-289` `head_block_ended` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:573-576` `held_port_holder` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:618-622` `url_port` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:695-732` `dash_status` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:744-752` `dash_start_needed` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:806-817` `should_reap_singleton` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:1151-1153` `is_not_back` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:1158-1160` `is_not_shared` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:1437-1550` `build_state` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:1682-1690` `file_of` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:1696-1701` `name_suffix` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2583-2621` `member_set` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2937-2957` `explain` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:3067-3069` `memory_rail` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:3090-3137` `memory_rail_of` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:3345-3386` `path` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:3449-3455` `parse_call_dir` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:3479-3487` `call_edge_view` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:3519-3605` `calls_view` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:3679-3790` `build_run_tree` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:3944-3963` `driver_stage` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:3967-3975` `rollup` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:4048-4055` `gates_status` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:4066-4069` `advanced_past_gates` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:4074-4083` `unit_live_status` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:4088-4096` `spec_of` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:4123-4146` `event_seed_ids` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:4158-4181` `unit_seeds` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:4196-4209` `repoint_seed` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:4213-4224` `event_view` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:4250-4255` `now_unix` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:4264-4283` `state_json` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:4289-4297` `events_json` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:4301-4303` `live_page` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:4310-4330` `render_export` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:4604-4622` `percent_decode` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:4625-4631` `query_param` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:4635-4640` `parse_request_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:264-266` `version_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:444-446` `open_sqlite_store` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:450-454` `env_conn` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:539-542` `config_rigger_dir` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:738-740` `registry_heartbeat_interval` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:792-831` `register_run_instance` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:862-895` `refresh_registry_entry` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:907-910` `project_identity` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:935-946` `project_identity_at` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:953-955` `legacy_identity_at` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:960-973` `legacy_identity_from` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:994-1002` `has_tracked_project_id` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1008-1017` `fnv1a_64` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1022-1028` `canonical_definition_text` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1042-1081` `definition_hash` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1104-1141` `enforce_definition_pin` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1150-1175` `normalize_origin_url` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1180-1196` `origin_url_at` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1202-1207` `mint_project_id` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1227-1241` `decide_migration` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1417-1477` `main` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1649-1651` `usage` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1653-1658` `db_path` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1700-1757` `walk_stores_from` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1770-1791` `main_repo_root` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1866-1915` `refuse_unless_one_root` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:2124-2162` `result_advisories` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:2177-2181` `load_run_config` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:2256-2276` `acquire_step_lock` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:2964-2977` `merge_hung_attention` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3118-3121` `reap_then_remove_dir` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3132-3146` `reap_then_remove_worktree` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3308-3321` `result_of_at` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3410-3434` `warn_on_run_branch_divergence` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3447-3456` `anchor_run_branch` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3480-3494` `refuse_when_base_unreachable` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3566-3692` `run_cli` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3716-3760` `fresh_run_if_requested` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:4091-4108` `load_criteria` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:4294-4339` `definition_body` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:4353-4371` `derive_extent_end` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:4378-4383` `derive_extent_end` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:4740-4769` `stats_lines` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:4790-4804` `parallelism_retention_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:4867-4888` `append_spawn_timing` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:4893-4895` `fmt_duration` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:4923-5033` `append_review_quality` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:5062-5078` `canary_stats_lines` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:5208-5252` `model_drift_advisory` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:5261-5278` `order_signature_advisories` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:5775-5785` `started_units` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:5806-5839` `candidate_reaches_gate` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:5883-5897` `baseline_run_slice` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:5900-5904` `run_started_id` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:5911-5957` `materialize_config_at_rev` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:6040-6058` `start_run_dashboard` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:6065-6080` `spawn_run_dashboard` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:6167-6173` `detach_process_group` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:6176-6176` `detach_process_group` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:6410-6465` `spawn_run_dashboard_detached` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:7001-7051` `watch_and_self_reap_on_idle` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:7256-7258` `instance_rigger_dir` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:7818-7826` `status_blocker_lines` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:7835-7841` `release_ready_lines` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:7962-8175` `watch_poll` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:8327-8333` `runs_menu_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:8340-8362` `derived_menu_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:8530-8619` `derived_prune_report` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:8932-8936` `graph_node_id` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:8995-9005` `peer_decision_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9009-9018` `json_str_array` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9022-9031` `json_type_name` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9127-9148` `build_result` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9396-9408` `fold_recorded_result_into_graph` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9624-9657` `build_environment_report` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9665-9704` `validate_advisories` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9711-9737` `index_staleness_message` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9790-9799` `retired_entities_advisory` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9812-9818` `retired_entities_advisory_for` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9913-9924` `missing_gitsemver_binary_advisory` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9936-9956` `behind_the_tree_message` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9975-9987` `behind_the_tree_advisory` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10007-10021` `drift_side` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10070-10091` `uncommitted_rigger_advisory` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10099-10114` `dirty_tracked_paths` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10154-10191` `residue_advisories` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10202-10219` `leaked_process_advisories` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10319-10365` `current_run_units` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10497-10516` `local_unit_branches` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10524-10585` `scan_residue` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10662-10681` `dir_size_bytes` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10703-10713` `build_cache_tombstone_path` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10812-10825` `human_size` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:11057-11067` `dead_spawn_leaf_bytes` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:11077-11084` `looks_like_run_container` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:11290-11302` `owning_repo_root` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:11380-11488` `init_project` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:11596-11624` `get_referenced_agent_ids` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:11923-12038` `precommit_block` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:12069-12121` `compose_precommit_bytes` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:12574-12665` `import_agents` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:12674-12704` `normalize_identity` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:12709-12715` `top_level_key` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:12999-13014` `select_grounder` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13022-13024` `select_reindex_grounder` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13139-13182` `grep_guard_decision` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13220-13282` `shell_command_word_spans` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13286-13292` `shell_command_words` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13308-13333` `strip_literal_marker` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13400-13411` `path_has_guarded_segment` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13417-13419` `guarded_path` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13435-13437` `guarded_command` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13442-13447` `word_basename` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13458-13460` `command_invokes_grep` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.

## 2. Duplication Catalog

778 clusters (3749 total sites) across `src/` and `tests/`, found by `tests/simplification_audit.rs`'s deterministic normalized-token-shingle Jaccard pass (8-token shingles, threshold 0.72) plus five mandatory mechanical sweeps. Strict definition (spec 85 Goal): any logic present in more than one place anywhere in the codebase is a violation, with no "small enough to duplicate" exemption.

### Mandatory sweeps

- **Command::new call sites**: 377 site(s) - `dup-0006`
- **/proc-path string literals**: 60 site(s) - `dup-0146`
- **sqlite Connection::open call sites**: 46 site(s) - `dup-0125`
- **.rigger-path string literals**: 717 site(s) - `dup-0060`
- **error-shaping helper functions**: 8 site(s) - `dup-0079`

### Clusters (249 exact, 449 near, 80 semantic)

#### `dup-0001` (near, 2 sites)

Proposed home: `blast_radius_eval::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/blast_radius_eval.rs:216-231` `width_threshold_is_the_tier_width_nearest_rank_percentile`
- `src/blast_radius_eval.rs:234-242` `full_fraction_spans_all_light_to_collapse`

#### `dup-0002` (exact, 9 sites)

Proposed home: `a new shared module (sites span 9 files: src/blocker.rs, src/dash.rs, src/ledger.rs, src/main.rs, src/metrics.rs, src/run.rs, src/watch.rs, tests/dash_run_tree_spine.rs, tests/grep_fallback_metric_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/blocker.rs:306-308` `ev`
- `src/dash.rs:5041-5043` `ev`
- `src/ledger.rs:719-721` `ev`
- `src/main.rs:15040-15042` `ev`
- `src/metrics.rs:1400-1402` `ev`
- `src/run.rs:549-551` `ev`
- `src/watch.rs:607-609` `ev`
- `tests/dash_run_tree_spine.rs:51-53` `ev`
- `tests/grep_fallback_metric_periphery.rs:50-52` `run_ev`

#### `dup-0003` (near, 4 sites)

Proposed home: `a new shared module (sites span 4 files: src/blocker.rs, src/dash.rs, tests/dash_run_tree_spine.rs, tests/grep_fallback_metric_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/blocker.rs:310-315` `positioned`
- `src/dash.rs:5047-5052` `positioned`
- `tests/dash_run_tree_spine.rs:57-62` `positioned`
- `tests/grep_fallback_metric_periphery.rs:69-74` `positioned`

#### `dup-0004` (near, 3 sites)

Proposed home: `blocker::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/blocker.rs:392-405` `reject_recurrence_line_shows_n_over_max`
- `src/blocker.rs:408-424` `reject_recurrence_line_carries_the_recorded_cause`
- `src/blocker.rs:427-447` `reject_recurrence_line_carries_the_latest_of_several_causes`

#### `dup-0005` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/budget.rs, tests/no_os_kill_test_helper_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/budget.rs:128-136` `wait_until`
- `tests/no_os_kill_test_helper_periphery.rs:64-72` `wait_until`

#### `dup-0006` (semantic, 377 sites)

Proposed home: `a single injected process-spawn port every Command::new site routes through instead of constructing its own Command`

mandatory sweep: Command::new call sites - 377 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/budget.rs:208-208` `Command::new`
- `src/budget.rs:246-246` `Command::new`
- `src/budget.rs:257-257` `Command::new`
- `src/conductor.rs:13670-13670` `Command::new`
- `src/conductor.rs:13750-13750` `Command::new`
- `src/conductor.rs:13767-13767` `Command::new`
- `src/conductor.rs:13785-13785` `Command::new`
- `src/conductor.rs:14250-14250` `Command::new`
- `src/conductor.rs:14255-14255` `Command::new`
- `src/conductor.rs:21147-21147` `Command::new`
- `src/conductor.rs:24970-24970` `Command::new`
- `src/conductor.rs:25588-25588` `Command::new`
- `src/conductor.rs:25662-25662` `Command::new`
- `src/conductor.rs:26587-26587` `Command::new`
- `src/conductor.rs:26776-26776` `Command::new`
- `src/conductor.rs:30556-30556` `Command::new`
- `src/conductor.rs:30654-30654` `Command::new`
- `src/conductor.rs:30739-30739` `Command::new`
- `src/conductor.rs:31031-31031` `Command::new`
- `src/conductor.rs:31361-31361` `Command::new`
- `src/conductor.rs:31391-31391` `Command::new`
- `src/conductor.rs:31539-31539` `Command::new`
- `src/conductor.rs:32047-32047` `Command::new`
- `src/conductor.rs:35706-35706` `Command::new`
- `src/conductor.rs:36367-36367` `Command::new`
- `src/conductor.rs:37035-37035` `Command::new`
- `src/conductor.rs:37042-37042` `Command::new`
- `src/conductor.rs:37132-37132` `Command::new`
- `src/conductor.rs:37189-37189` `Command::new`
- `src/conductor.rs:37245-37245` `Command::new`
- `src/conductor.rs:37456-37456` `Command::new`
- `src/conductor.rs:37470-37470` `Command::new`
- `src/conductor.rs:37518-37518` `Command::new`
- `src/conductor.rs:37750-37750` `Command::new`
- `src/conductor.rs:38075-38075` `Command::new`
- `src/conductor.rs:38110-38110` `Command::new`
- `src/conductor.rs:39291-39291` `Command::new`
- `src/conductor.rs:39309-39309` `Command::new`
- `src/conductor.rs:39327-39327` `Command::new`
- `src/conductor.rs:39358-39358` `Command::new`
- `src/conductor.rs:39549-39549` `Command::new`
- `src/conductor.rs:39862-39862` `Command::new`
- `src/dash.rs:4961-4961` `Command::new`
- `src/driver/cli.rs:45-45` `Command::new`
- `src/gate.rs:745-745` `Command::new`
- `src/gate.rs:754-754` `Command::new`
- `src/gate.rs:1690-1690` `Command::new`
- `src/main.rs:1181-1181` `Command::new`
- `src/main.rs:1771-1771` `Command::new`
- `src/main.rs:3135-3135` `Command::new`
- `src/main.rs:4008-4008` `Command::new`
- `src/main.rs:5923-5923` `Command::new`
- `src/main.rs:5950-5950` `Command::new`
- `src/main.rs:6068-6068` `Command::new`
- `src/main.rs:6197-6197` `Command::new`
- `src/main.rs:9874-9874` `Command::new`
- `src/main.rs:9893-9893` `Command::new`
- `src/main.rs:10071-10071` `Command::new`
- `src/main.rs:10498-10498` `Command::new`
- `src/main.rs:11561-11561` `Command::new`
- `src/main.rs:12143-12143` `Command::new`
- `src/main.rs:12277-12277` `Command::new`
- `src/main.rs:13038-13038` `Command::new`
- `src/main.rs:14016-14016` `Command::new`
- `src/main.rs:14049-14049` `Command::new`
- `src/main.rs:14093-14093` `Command::new`
- `src/main.rs:14162-14162` `Command::new`
- `src/main.rs:16280-16280` `Command::new`
- `src/main.rs:16327-16327` `Command::new`
- `src/main.rs:16437-16437` `Command::new`
- `src/main.rs:16447-16447` `Command::new`
- `src/main.rs:16464-16464` `Command::new`
- `src/main.rs:16600-16600` `Command::new`
- `src/main.rs:16612-16612` `Command::new`
- `src/main.rs:16782-16782` `Command::new`
- `src/main.rs:18141-18141` `Command::new`
- `src/main.rs:18147-18147` `Command::new`
- `src/main.rs:18249-18249` `Command::new`
- `src/main.rs:18351-18351` `Command::new`
- `src/main.rs:18417-18417` `Command::new`
- `src/main.rs:18423-18423` `Command::new`
- `src/main.rs:18627-18627` `Command::new`
- `src/main.rs:18633-18633` `Command::new`
- `src/main.rs:18647-18647` `Command::new`
- `src/main.rs:18681-18681` `Command::new`
- `src/main.rs:18687-18687` `Command::new`
- `src/main.rs:18704-18704` `Command::new`
- `src/main.rs:22117-22117` `Command::new`
- `src/main.rs:22868-22868` `Command::new`
- `src/main.rs:24604-24604` `Command::new`
- `src/main.rs:24638-24638` `Command::new`
- `src/worktree.rs:600-600` `Command::new`
- `src/worktree.rs:1131-1131` `Command::new`
- `src/worktree.rs:1142-1142` `Command::new`
- `src/worktree.rs:2205-2205` `Command::new`
- `src/worktree.rs:2666-2666` `Command::new`
- `src/worktree.rs:3491-3491` `Command::new`
- `src/worktree.rs:4916-4916` `Command::new`
- `src/worktree.rs:5249-5249` `Command::new`
- `src/worktree.rs:6039-6039` `Command::new`
- `src/worktree.rs:6045-6045` `Command::new`
- `tests/adaptive_labels_periphery.rs:66-66` `Command::new`
- `tests/adaptive_labels_periphery.rs:105-105` `Command::new`
- `tests/adoption_keys_on_criterion_periphery.rs:187-187` `Command::new`
- `tests/adoption_keys_on_criterion_periphery.rs:199-199` `Command::new`
- `tests/adoption_keys_on_criterion_periphery.rs:1594-1594` `Command::new`
- `tests/adoption_keys_on_criterion_periphery.rs:2508-2508` `Command::new`
- `tests/build_watch_paths.rs:43-43` `Command::new`
- `tests/build_watch_paths.rs:60-60` `Command::new`
- `tests/canary_model_drift_periphery.rs:41-41` `Command::new`
- `tests/cause_wire_periphery.rs:56-56` `Command::new`
- `tests/cause_wire_periphery.rs:76-76` `Command::new`
- `tests/change_path_revert_periphery.rs:60-60` `Command::new`
- `tests/change_path_revert_periphery.rs:104-104` `Command::new`
- `tests/change_path_revert_periphery.rs:131-131` `Command::new`
- `tests/checkin_mutation_diff_base_periphery.rs:84-84` `Command::new`
- `tests/checkin_mutation_diff_base_periphery.rs:176-176` `Command::new`
- `tests/checkin_mutation_diff_base_periphery.rs:221-221` `Command::new`
- `tests/cli.rs:24-24` `Command::new`
- `tests/cli.rs:50-50` `Command::new`
- `tests/cli.rs:113-113` `Command::new`
- `tests/cli.rs:127-127` `Command::new`
- `tests/cli.rs:190-190` `Command::new`
- `tests/cli.rs:1825-1825` `Command::new`
- `tests/cli.rs:1886-1886` `Command::new`
- `tests/cli.rs:5547-5547` `Command::new`
- `tests/cli.rs:5772-5772` `Command::new`
- `tests/cli.rs:5964-5964` `Command::new`
- `tests/cli.rs:6214-6214` `Command::new`
- `tests/cli.rs:6376-6376` `Command::new`
- `tests/cli.rs:11741-11741` `Command::new`
- `tests/cli.rs:11751-11751` `Command::new`
- `tests/cli.rs:11783-11783` `Command::new`
- `tests/cli.rs:14523-14523` `Command::new`
- `tests/cli.rs:15288-15288` `Command::new`
- `tests/cli.rs:15341-15341` `Command::new`
- `tests/cli.rs:20519-20519` `Command::new`
- `tests/cli.rs:20588-20588` `Command::new`
- `tests/cli.rs:20661-20661` `Command::new`
- `tests/cli.rs:20742-20742` `Command::new`
- `tests/cli.rs:20918-20918` `Command::new`
- `tests/cli.rs:20978-20978` `Command::new`
- `tests/cli.rs:21086-21086` `Command::new`
- `tests/cli.rs:21114-21114` `Command::new`
- `tests/cli.rs:21235-21235` `Command::new`
- `tests/cli.rs:21295-21295` `Command::new`
- `tests/cli.rs:21344-21344` `Command::new`
- `tests/cli.rs:21382-21382` `Command::new`
- `tests/cli.rs:21444-21444` `Command::new`
- `tests/cli.rs:21523-21523` `Command::new`
- `tests/cli.rs:21581-21581` `Command::new`
- `tests/cli.rs:21627-21627` `Command::new`
- `tests/cli.rs:29217-29217` `Command::new`
- `tests/code_lens_overview_collapse_viz.rs:40-40` `Command::new`
- `tests/code_lens_overview_collapse_viz.rs:102-102` `Command::new`
- `tests/common/mod.rs:130-130` `Command::new`
- `tests/community_detection_cli.rs:59-59` `Command::new`
- `tests/community_detection_cli.rs:81-81` `Command::new`
- `tests/compiler_pass_stage1_audit.rs:195-195` `Command::new`
- `tests/compiler_pass_stage1_audit.rs:213-213` `Command::new`
- `tests/concepts_derivation_cli.rs:64-64` `Command::new`
- `tests/concepts_derivation_cli.rs:86-86` `Command::new`
- `tests/concepts_lens_view_periphery.rs:703-703` `Command::new`
- `tests/concepts_lens_view_periphery.rs:827-827` `Command::new`
- `tests/courier_registry_refresh_boundary_periphery.rs:40-40` `Command::new`
- `tests/courier_registry_refresh_boundary_periphery.rs:52-52` `Command::new`
- `tests/courier_registry_refresh_boundary_periphery.rs:150-150` `Command::new`
- `tests/courier_registry_refresh_fence_periphery.rs:40-40` `Command::new`
- `tests/courier_registry_refresh_periphery.rs:38-38` `Command::new`
- `tests/dash_calls_render_viz.rs:49-49` `Command::new`
- `tests/dash_calls_render_viz.rs:448-448` `Command::new`
- `tests/dash_decisions_progressive_disclosure.rs:328-328` `Command::new`
- `tests/dash_decisions_progressive_disclosure.rs:365-365` `Command::new`
- `tests/dash_decisions_progressive_disclosure.rs:475-475` `Command::new`
- `tests/dash_graph_exploration_viz.rs:43-43` `Command::new`
- `tests/dash_graph_exploration_viz.rs:377-377` `Command::new`
- `tests/dash_kg_graph_route.rs:326-326` `Command::new`
- `tests/dash_kg_graph_route.rs:462-462` `Command::new`
- `tests/dash_kg_graph_route.rs:903-903` `Command::new`
- `tests/dash_kg_graph_route.rs:1327-1327` `Command::new`
- `tests/dash_release_ready.rs:232-232` `Command::new`
- `tests/dash_release_ready.rs:376-376` `Command::new`
- `tests/dedup_seeding_periphery.rs:339-339` `Command::new`
- `tests/dedup_seeding_periphery.rs:368-368` `Command::new`
- `tests/dedup_seeding_periphery.rs:581-581` `Command::new`
- `tests/dedup_seeding_periphery.rs:657-657` `Command::new`
- `tests/escalation_resume_periphery.rs:81-81` `Command::new`
- `tests/escalation_resume_periphery.rs:100-100` `Command::new`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:121-121` `Command::new`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:166-166` `Command::new`
- `tests/files_lens_directory_hulls_viz.rs:50-50` `Command::new`
- `tests/files_lens_directory_hulls_viz.rs:175-175` `Command::new`
- `tests/files_lens_directory_hulls_viz.rs:309-309` `Command::new`
- `tests/gate_store_fence_periphery.rs:180-180` `Command::new`
- `tests/gate_store_fence_periphery.rs:500-500` `Command::new`
- `tests/gitsemver_derivation.rs:44-44` `Command::new`
- `tests/gitsemver_derivation.rs:84-84` `Command::new`
- `tests/gitsemver_derivation.rs:143-143` `Command::new`
- `tests/gitsemver_worktree_periphery.rs:57-57` `Command::new`
- `tests/gitsemver_worktree_periphery.rs:76-76` `Command::new`
- `tests/gitsemver_worktree_periphery.rs:118-118` `Command::new`
- `tests/gitsemver_worktree_periphery.rs:182-182` `Command::new`
- `tests/graph_collision_body_and_tiebreak.rs:45-45` `Command::new`
- `tests/graph_collision_body_and_tiebreak.rs:108-108` `Command::new`
- `tests/graph_density_spread_floor_and_centring.rs:51-51` `Command::new`
- `tests/graph_density_spread_floor_and_centring.rs:114-114` `Command::new`
- `tests/graph_show_periphery.rs:51-51` `Command::new`
- `tests/graph_show_periphery.rs:62-62` `Command::new`
- `tests/graph_show_periphery.rs:90-90` `Command::new`
- `tests/graph_show_staleness.rs:39-39` `Command::new`
- `tests/graph_show_staleness.rs:50-50` `Command::new`
- `tests/graph_show_staleness.rs:98-98` `Command::new`
- `tests/graph_show_surface.rs:38-38` `Command::new`
- `tests/graph_show_surface.rs:49-49` `Command::new`
- `tests/graph_show_surface.rs:77-77` `Command::new`
- `tests/halted_spawn_wip_recovery_periphery.rs:90-90` `Command::new`
- `tests/halted_spawn_wip_recovery_periphery.rs:437-437` `Command::new`
- `tests/halted_spawn_wip_recovery_periphery.rs:721-721` `Command::new`
- `tests/heartbeat_write_read_agree_periphery.rs:55-55` `Command::new`
- `tests/heartbeat_write_read_agree_periphery.rs:69-69` `Command::new`
- `tests/hermetic_test_git_audit.rs:221-221` `Command::new`
- `tests/hermetic_test_git_audit.rs:253-253` `Command::new`
- `tests/hermetic_test_git_audit.rs:325-325` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:269-269` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:280-280` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:291-291` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:310-310` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:557-557` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:757-757` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:1017-1017` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:1248-1248` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:1454-1454` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:1721-1721` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:1831-1831` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:2153-2153` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:2289-2289` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:2547-2547` `Command::new`
- `tests/meta_phases_declaration_periphery.rs:119-119` `Command::new`
- `tests/metadata_card_handoff_viz.rs:43-43` `Command::new`
- `tests/metadata_card_handoff_viz.rs:151-151` `Command::new`
- `tests/migration_is_deliberate_periphery.rs:450-450` `Command::new`
- `tests/migration_is_deliberate_periphery.rs:464-464` `Command::new`
- `tests/mutation_runner_pdeathsig_periphery.rs:133-133` `Command::new`
- `tests/mutation_scratch_reap_base_guard_periphery.rs:55-55` `Command::new`
- `tests/mutation_scratch_reap_base_guard_periphery.rs:141-141` `Command::new`
- `tests/native_driver_pipelining_behavior.rs:46-46` `Command::new`
- `tests/native_driver_pipelining_behavior.rs:305-305` `Command::new`
- `tests/no_os_kill_test_helper_periphery.rs:28-28` `Command::new`
- `tests/no_os_kill_test_helper_periphery.rs:46-46` `Command::new`
- `tests/phase_of_role_mapping_periphery.rs:95-95` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:230-230` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:249-249` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:388-388` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:401-401` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:408-408` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:769-769` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:776-776` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:793-793` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:800-800` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:1047-1047` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:1202-1202` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:1215-1215` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:1344-1344` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:1404-1404` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:1419-1419` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:1455-1455` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:1462-1462` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:1577-1577` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:1589-1589` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:1625-1625` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:1632-1632` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:1649-1649` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:1656-1656` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:1833-1833` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:1842-1842` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:1874-1874` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:2014-2014` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:2021-2021` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:2036-2036` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:2055-2055` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:2309-2309` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:2316-2316` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:2328-2328` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:2344-2344` `Command::new`
- `tests/product_binary_authority_periphery.rs:155-155` `Command::new`
- `tests/projections_stay_local.rs:135-135` `Command::new`
- `tests/projections_stay_local.rs:199-199` `Command::new`
- `tests/proof_row_renders_on_the_card.rs:34-34` `Command::new`
- `tests/proof_row_renders_on_the_card.rs:104-104` `Command::new`
- `tests/readable_graph_adaptive_labels.rs:60-60` `Command::new`
- `tests/readable_graph_adaptive_labels.rs:276-276` `Command::new`
- `tests/readable_graph_density_scaled_spacing.rs:51-51` `Command::new`
- `tests/readable_graph_density_scaled_spacing.rs:238-238` `Command::new`
- `tests/readable_graph_layout_separation.rs:45-45` `Command::new`
- `tests/readable_graph_layout_separation.rs:225-225` `Command::new`
- `tests/reap_before_removal_periphery.rs:42-42` `Command::new`
- `tests/reap_before_removal_periphery.rs:69-69` `Command::new`
- `tests/reap_before_removal_periphery.rs:407-407` `Command::new`
- `tests/registry_refresh_driver_courier_convergence_periphery.rs:41-41` `Command::new`
- `tests/registry_refresh_driver_courier_convergence_periphery.rs:53-53` `Command::new`
- `tests/relocated_worktree_store_resolution_periphery.rs:37-37` `Command::new`
- `tests/relocated_worktree_store_resolution_periphery.rs:138-138` `Command::new`
- `tests/relocated_worktree_store_resolution_periphery.rs:208-208` `Command::new`
- `tests/reset_build_cache_periphery.rs:38-38` `Command::new`
- `tests/reset_build_cache_periphery.rs:280-280` `Command::new`
- `tests/reset_build_cache_periphery.rs:292-292` `Command::new`
- `tests/reset_build_cache_periphery.rs:370-370` `Command::new`
- `tests/reset_build_cache_periphery.rs:388-388` `Command::new`
- `tests/reset_derived_compaction.rs:41-41` `Command::new`
- `tests/reset_derived_compaction.rs:52-52` `Command::new`
- `tests/reset_derived_compaction_periphery.rs:602-602` `Command::new`
- `tests/reset_derived_compaction_periphery.rs:617-617` `Command::new`
- `tests/reset_derived_compaction_periphery.rs:2601-2601` `Command::new`
- `tests/reset_derived_live_writer_guard_periphery.rs:43-43` `Command::new`
- `tests/reset_derived_live_writer_guard_periphery.rs:58-58` `Command::new`
- `tests/reset_derived_live_writer_guard_periphery.rs:286-286` `Command::new`
- `tests/reset_derived_live_writer_guard_periphery.rs:295-295` `Command::new`
- `tests/reset_menu.rs:39-39` `Command::new`
- `tests/reset_menu.rs:47-47` `Command::new`
- `tests/reset_menu_identity_migration_periphery.rs:34-34` `Command::new`
- `tests/reset_menu_identity_migration_periphery.rs:42-42` `Command::new`
- `tests/review_tier_roster_periphery.rs:111-111` `Command::new`
- `tests/scaffold_grounder_resolves.rs:93-93` `Command::new`
- `tests/scaffold_grounder_resolves.rs:98-98` `Command::new`
- `tests/scratch_workdir_isolation_leak_guard_periphery.rs:72-72` `Command::new`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:47-47` `Command::new`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:66-66` `Command::new`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:139-139` `Command::new`
- `tests/spawn_target_dir_periphery.rs:64-64` `Command::new`
- `tests/spec_lint.rs:25-25` `Command::new`
- `tests/step_attention_periphery.rs:151-151` `Command::new`
- `tests/step_attention_periphery.rs:466-466` `Command::new`
- `tests/step_attention_periphery.rs:475-475` `Command::new`
- `tests/step_attention_periphery.rs:701-701` `Command::new`
- `tests/step_root_resolution_periphery.rs:143-143` `Command::new`
- `tests/step_root_resolution_periphery.rs:222-222` `Command::new`
- `tests/step_root_resolution_periphery.rs:235-235` `Command::new`
- `tests/store_content_identity_periphery.rs:1342-1342` `Command::new`
- `tests/store_flag_precedence.rs:60-60` `Command::new`
- `tests/store_flag_precedence.rs:106-106` `Command::new`
- `tests/store_precedence.rs:49-49` `Command::new`
- `tests/store_resolution.rs:150-150` `Command::new`
- `tests/store_resolution.rs:237-237` `Command::new`
- `tests/store_resolution.rs:316-316` `Command::new`
- `tests/store_resolution.rs:344-344` `Command::new`
- `tests/store_resolution_cli.rs:56-56` `Command::new`
- `tests/store_secrets.rs:52-52` `Command::new`
- `tests/subject_lens_overlay_client_arms.rs:44-44` `Command::new`
- `tests/subject_lens_overlay_client_arms.rs:111-111` `Command::new`
- `tests/subject_lens_overlay_served_page.rs:49-49` `Command::new`
- `tests/subject_lens_overlay_served_page.rs:69-69` `Command::new`
- `tests/subject_view_memory_rail_client.rs:35-35` `Command::new`
- `tests/subject_view_memory_rail_client.rs:129-129` `Command::new`
- `tests/turbovec_retired_cargo_boundary.rs:50-50` `Command::new`
- `tests/unified_traversal_grounding.rs:584-584` `Command::new`
- `tests/validate_advisories.rs:49-49` `Command::new`
- `tests/validate_advisories.rs:58-58` `Command::new`
- `tests/validate_behind_the_tree_periphery.rs:68-68` `Command::new`
- `tests/validate_behind_the_tree_periphery.rs:95-95` `Command::new`
- `tests/validate_behind_the_tree_periphery.rs:114-114` `Command::new`
- `tests/validate_behind_the_tree_periphery.rs:140-140` `Command::new`
- `tests/validate_behind_the_tree_periphery.rs:202-202` `Command::new`
- `tests/validate_footprint_default_scratch_root_periphery.rs:28-28` `Command::new`
- `tests/validate_footprint_default_scratch_root_periphery.rs:88-88` `Command::new`
- `tests/watchdog_cli_periphery.rs:46-46` `Command::new`
- `tests/watchdog_cli_periphery.rs:67-67` `Command::new`
- `tests/worker_persona_label_periphery.rs:94-94` `Command::new`
- `tests/workflow_driver_resolved_model_periphery.rs:60-60` `Command::new`
- `tests/workflow_driver_resolved_model_periphery.rs:69-69` `Command::new`
- `tests/worktree_liveness_fence_periphery.rs:97-97` `Command::new`
- `tests/worktree_liveness_fence_periphery.rs:106-106` `Command::new`
- `tests/worktree_liveness_fence_periphery.rs:127-127` `Command::new`
- `tests/worktree_liveness_fence_periphery.rs:139-139` `Command::new`
- `tests/worktree_liveness_fence_periphery.rs:150-150` `Command::new`
- `tests/worktree_liveness_fence_periphery.rs:165-165` `Command::new`
- `tests/worktree_remove_relocated_scratch_base_guard_periphery.rs:56-56` `Command::new`
- `tests/worktree_remove_relocated_scratch_base_guard_periphery.rs:83-83` `Command::new`

#### `dup-0007` (near, 2 sites)

Proposed home: `canary::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/canary.rs:288-307` `to_event`
- `src/canary.rs:393-406` `to_event`

#### `dup-0008` (exact, 13 sites)

Proposed home: `a new shared module (sites span 12 files: src/canary.rs, src/conductor.rs, src/config.rs, tests/canary_false_positives_periphery.rs, tests/canary_findings_volume_periphery.rs, tests/canary_item_sharding_jobs_cap_periphery.rs, tests/canary_lens_fanout_periphery.rs, tests/canary_progress_hook_periphery.rs, tests/canary_tolerant_attribution_periphery.rs, tests/canary_unattributed_rejects_periphery.rs, tests/integrate_conflict_merge_periphery.rs, tests/plan_stage_commit_landing_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/canary.rs:928-933` `agent`
- `src/canary.rs:1037-1042` `with_anchor`
- `src/conductor.rs:14307-14312` `agent`
- `src/config.rs:2777-2782` `agent_def`
- `tests/canary_false_positives_periphery.rs:123-128` `agent`
- `tests/canary_findings_volume_periphery.rs:109-114` `agent`
- `tests/canary_item_sharding_jobs_cap_periphery.rs:41-46` `agent`
- `tests/canary_lens_fanout_periphery.rs:107-112` `agent`
- `tests/canary_progress_hook_periphery.rs:47-52` `agent`
- `tests/canary_tolerant_attribution_periphery.rs:88-93` `agent`
- `tests/canary_unattributed_rejects_periphery.rs:110-115` `agent`
- `tests/integrate_conflict_merge_periphery.rs:325-330` `agent`
- `tests/plan_stage_commit_landing_periphery.rs:267-272` `agent`

#### `dup-0009` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/canary.rs, tests/canary_tolerant_attribution_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/canary.rs:944-950` `cfg`
- `tests/canary_tolerant_attribution_periphery.rs:95-101` `cfg`

#### `dup-0010` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/canary.rs, src/grounder/design/extract.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/canary.rs:1021-1023` `any_finding_is_critical`
- `src/grounder/design/extract.rs:157-159` `is_handbook_path`

#### `dup-0011` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/canary.rs, tests/canary_lens_fanout_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/canary.rs:1025-1035` `item`
- `tests/canary_lens_fanout_periphery.rs:131-141` `item`

#### `dup-0012` (near, 2 sites)

Proposed home: `canary::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/canary.rs:1052-1058` `catches_matches_an_absolute_path_spelling_of_a_repo_relative_anchor`
- `src/canary.rs:1061-1075` `catches_matches_a_segment_boundary_path_suffix_in_either_direction`

#### `dup-0013` (exact, 2 sites)

Proposed home: `canary::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/canary.rs:1188-1199` `spawn`
- `src/canary.rs:1593-1604` `spawn`

#### `dup-0014` (exact, 7 sites)

Proposed home: `a new shared module (sites span 7 files: src/canary.rs, tests/canary_false_positives_periphery.rs, tests/canary_findings_volume_periphery.rs, tests/canary_item_sharding_jobs_cap_periphery.rs, tests/canary_lens_fanout_periphery.rs, tests/canary_progress_hook_periphery.rs, tests/canary_unattributed_rejects_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/canary.rs:1202-1208` `cfg_for`
- `tests/canary_false_positives_periphery.rs:130-136` `cfg`
- `tests/canary_findings_volume_periphery.rs:116-122` `cfg`
- `tests/canary_item_sharding_jobs_cap_periphery.rs:48-54` `cfg`
- `tests/canary_lens_fanout_periphery.rs:114-120` `cfg`
- `tests/canary_progress_hook_periphery.rs:54-60` `cfg`
- `tests/canary_unattributed_rejects_periphery.rs:117-123` `cfg`

#### `dup-0015` (exact, 5 sites)

Proposed home: `a new shared module (sites span 5 files: src/canary.rs, tests/canary_findings_volume_periphery.rs, tests/canary_item_sharding_jobs_cap_periphery.rs, tests/canary_lens_fanout_periphery.rs, tests/canary_progress_hook_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/canary.rs:1210-1217` `panel_with_lenses`
- `tests/canary_findings_volume_periphery.rs:124-131` `panel`
- `tests/canary_item_sharding_jobs_cap_periphery.rs:56-63` `panel`
- `tests/canary_lens_fanout_periphery.rs:122-129` `panel`
- `tests/canary_progress_hook_periphery.rs:62-69` `panel`

#### `dup-0016` (near, 2 sites)

Proposed home: `canary::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/canary.rs:1608-1637` `run_canary_shards_independent_items_concurrently_at_the_scheduling_seam`
- `src/canary.rs:1640-1673` `run_canary_jobs_cap_bounds_total_concurrent_spawns_across_both_dimensions`

#### `dup-0017` (exact, 6 sites)

Proposed home: `a new shared module (sites span 5 files: src/community.rs, src/dash.rs, src/eventstore/mod.rs, src/failure.rs, src/metrics.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/community.rs:243-245` `len`
- `src/community.rs:262-264` `is_empty`
- `src/dash.rs:4923-4925` `id`
- `src/eventstore/mod.rs:190-192` `handed`
- `src/failure.rs:225-227` `is_empty`
- `src/metrics.rs:441-443` `adversary_precision`

#### `dup-0018` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/community.rs, src/eventstore/mod.rs, src/failure.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/community.rs:249-251` `nodes`
- `src/eventstore/mod.rs:359-361` `types`
- `src/failure.rs:230-232` `rules`

#### `dup-0019` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/community.rs, src/concepts.rs, tests/concepts_labels_membership.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/community.rs:543-553` `node`
- `src/concepts.rs:295-305` `node`
- `tests/concepts_labels_membership.rs:40-53` `node`

#### `dup-0020` (exact, 4 sites)

Proposed home: `a new shared module (sites span 4 files: src/community.rs, tests/code_lens_view_periphery.rs, tests/concepts_lens_view_periphery.rs, tests/subject_lens_reprojection_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/community.rs:556-566` `edge`
- `tests/code_lens_view_periphery.rs:92-102` `edge`
- `tests/concepts_lens_view_periphery.rs:117-127` `edge`
- `tests/subject_lens_reprojection_periphery.rs:94-104` `edge`

#### `dup-0021` (exact, 4 sites)

Proposed home: `a new shared module (sites span 3 files: src/community.rs, src/concepts.rs, tests/concepts_labels_membership.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/community.rs:609-611` `assignment_map`
- `src/concepts.rs:383-385` `membership`
- `tests/concepts_labels_membership.rs:74-76` `membership`
- `tests/concepts_labels_membership.rs:79-81` `labels`

#### `dup-0022` (near, 4 sites)

Proposed home: `a new shared module (sites span 3 files: src/concepts.rs, src/ingest.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/concepts.rs:74-76` `is_intent_doc`
- `src/concepts.rs:80-82` `is_label_doc`
- `src/ingest.rs:277-279` `is_derived_index_type`
- `tests/simplification_audit.rs:2116-2118` `is_keyword`

#### `dup-0023` (near, 11 sites)

Proposed home: `a new shared module (sites span 9 files: src/concepts.rs, src/dash.rs, tests/concepts_labels_membership.rs, tests/dash_cluster_detail_drill.rs, tests/dash_exploration_route_client_contract.rs, tests/files_lens_view_periphery.rs, tests/metadata_card_periphery.rs, tests/rationale_overlay_seam.rs, tests/subject_view_memory_rail_contract.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/concepts.rs:308-318` `edge`
- `src/dash.rs:10592-10602` `edge`
- `src/dash.rs:10879-10889` `edge`
- `tests/concepts_labels_membership.rs:56-66` `edge`
- `tests/dash_cluster_detail_drill.rs:93-103` `edge`
- `tests/dash_exploration_route_client_contract.rs:75-85` `refs`
- `tests/dash_exploration_route_client_contract.rs:89-99` `membership`
- `tests/files_lens_view_periphery.rs:100-110` `edge`
- `tests/metadata_card_periphery.rs:52-62` `edge`
- `tests/rationale_overlay_seam.rs:42-52` `edge`
- `tests/subject_view_memory_rail_contract.rs:48-58` `edge`

#### `dup-0024` (exact, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:325-327` `gate_verdict_key`
- `src/conductor.rs:337-339` `gate_skip_key`
- `src/conductor.rs:350-352` `postmerge_gate_verdict_key`

#### `dup-0025` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: src/conductor.rs, src/spawn.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:359-361` `compensation_queued_key`
- `src/conductor.rs:1084-1086` `conflict_regenerate_key`
- `src/spawn.rs:90-92` `spawn_id`

#### `dup-0026` (near, 5 sites)

Proposed home: `a new shared module (sites span 4 files: src/conductor.rs, src/contextgraph/sqlite.rs, src/spawn.rs, tests/no_os_kill_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:376-378` `adoption_provenance_key`
- `src/conductor.rs:389-391` `quarantine_record_key`
- `src/contextgraph/sqlite.rs:1885-1887` `code_entity_id`
- `src/spawn.rs:441-443` `what`
- `tests/no_os_kill_audit.rs:45-47` `join`

#### `dup-0027` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/conductor.rs, src/spawn.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:569-571` `unit_of_gate_key`
- `src/spawn.rs:177-179` `unit_of`

#### `dup-0028` (near, 15 sites)

Proposed home: `a new shared module (sites span 10 files: src/conductor.rs, src/eventstore/namespace.rs, src/eventstore/sqlite.rs, src/grounder/mod.rs, src/main.rs, src/spawn.rs, src/worktree.rs, tests/canary_model_drift_periphery.rs, tests/halted_spawn_wip_recovery_periphery.rs, tests/reset_derived_compaction_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:685-687` `deferred_gate_verdict_key`
- `src/conductor.rs:696-698` `deferred_gate_failed_key`
- `src/conductor.rs:11052-11054` `build_system_prompt`
- `src/conductor.rs:11079-11086` `review_protocol`
- `src/eventstore/namespace.rs:69-71` `prefix_for`
- `src/eventstore/sqlite.rs:1404-1406` `successor`
- `src/grounder/mod.rs:207-213` `retired_grounder_error`
- `src/main.rs:11770-11772` `skill_source_rel`
- `src/main.rs:12892-12898` `spec_lint_next_step`
- `src/spawn.rs:65-67` `lens_role`
- `src/spawn.rs:143-145` `speculation_group_id`
- `src/worktree.rs:1503-1505` `shared_build_cache_guard_path`
- `tests/canary_model_drift_periphery.rs:140-142` `prose_claiming`
- `tests/halted_spawn_wip_recovery_periphery.rs:202-204` `unit_branch`
- `tests/reset_derived_compaction_periphery.rs:2818-2820` `derived_key_for`

#### `dup-0029` (exact, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:719-721` `from`
- `src/conductor.rs:724-726` `from`

#### `dup-0030` (semantic, 2 sites)

Proposed home: `conductor::review_outcome - one canonical constructor, the rest thin variants over it (or a builder), rather than each re-listing every field`

mandatory sweep: parallel constructor functions - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/conductor.rs:911-918` `approved`
- `src/conductor.rs:919-926` `rejected`

#### `dup-0031` (near, 5 sites)

Proposed home: `conductor::support (consolidate these 5 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:1463-1465` `is_parked`
- `src/conductor.rs:1501-1503` `is_budget_refused`
- `src/conductor.rs:1579-1581` `is_degenerate_reviewer`
- `src/conductor.rs:1622-1624` `is_verdict_channel_mismatch`
- `src/conductor.rs:1666-1668` `is_plan_landing_failed`

#### `dup-0032` (exact, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:1492-1496` `budget_refused`
- `src/conductor.rs:1607-1617` `verdict_channel_mismatch`

#### `dup-0033` (semantic, 2 sites)

Proposed home: `one shared `append_and_fold_batch` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/conductor.rs:2976-2998` `append_and_fold_batch`
- `src/ingest.rs:47-87` `append_and_fold_batch`

#### `dup-0034` (exact, 2 sites)

Proposed home: `conductor::run_ctx`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:3182-3184` `recorded_gate_verdict`
- `src/conductor.rs:3194-3196` `cached_green_verdict`

#### `dup-0035` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/conductor.rs, src/dash.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:3633-3635` `spawn_is_recorded`
- `src/dash.rs:2032-2034` `is_shared`

#### `dup-0036` (exact, 4 sites)

Proposed home: `conductor::run_ctx`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:3677-3679` `budget_broke`
- `src/conductor.rs:3685-3687` `parked`
- `src/conductor.rs:3693-3695` `manual_review_pending`
- `src/conductor.rs:3700-3702` `budget_halted`

#### `dup-0037` (exact, 2 sites)

Proposed home: `conductor::run_ctx`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:9193-9198` `clear_regenerate_pending`
- `src/conductor.rs:9214-9219` `clear_pending_landing`

#### `dup-0038` (near, 3 sites)

Proposed home: `conductor::run_ctx`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:9349-9363` `record_merge_attempt`
- `src/conductor.rs:9395-9410` `record_landing_intent`
- `src/conductor.rs:9415-9423` `record_landed`

#### `dup-0039` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/conductor.rs, src/distiller.rs, tests/cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:10795-10797` `normalize_ws`
- `src/distiller.rs:60-62` `normalize`
- `tests/cli.rs:25893-25895` `normalize_ws`

#### `dup-0040` (semantic, 2 sites)

Proposed home: `one shared `normalize_ws` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/conductor.rs:10795-10797` `normalize_ws`
- `tests/cli.rs:25893-25895` `normalize_ws`

#### `dup-0041` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/conductor.rs, src/eventstore/sqlite.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:11866-11868` `unit_branch`
- `src/eventstore/sqlite.rs:1323-1325` `key_expr`

#### `dup-0042` (semantic, 2 sites)

Proposed home: `one shared `unit_worktree_dir` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/conductor.rs:11878-11884` `unit_worktree_dir`
- `tests/halted_spawn_wip_recovery_periphery.rs:198-200` `unit_worktree_dir`

#### `dup-0043` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:12237-12242` `review_worktree_dir`
- `src/conductor.rs:12249-12251` `review_branch`

#### `dup-0044` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:13193-13199` `compensated`
- `src/conductor.rs:13204-13209` `plain_failure`

#### `dup-0045` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:13212-13233` `prior_criterion_unit_never_adopts_across_two_different_specs_sharing_the_same_criterion_id`
- `src/conductor.rs:13275-13292` `prior_criterion_unit_a_plain_non_compensation_failure_never_reopens_an_integrated_criterion`

#### `dup-0046` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:13236-13251` `prior_criterion_unit_still_adopts_across_two_runs_of_the_same_spec`
- `src/conductor.rs:13254-13272` `prior_criterion_unit_readopts_after_a_compensation_reverts_the_integration`

#### `dup-0047` (near, 4 sites)

Proposed home: `conductor::stub`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:14082-14089` `prompts_for`
- `src/conductor.rs:14092-14099` `dirs_for`
- `src/conductor.rs:14103-14109` `system_prompt_for`
- `src/conductor.rs:14113-14115` `title_for`

#### `dup-0048` (near, 14 sites)

Proposed home: `a new shared module (sites span 4 files: src/conductor.rs, src/eventstore/mod.rs, tests/build_env_authority_periphery.rs, tests/rigger_run_base_gate_env_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:14125-14127` `spawn_ids`
- `src/conductor.rs:34190-34192` `calls`
- `src/conductor.rs:34193-34195` `targets`
- `src/conductor.rs:34196-34198` `mutants_dirs`
- `src/conductor.rs:34202-34204` `store_fences`
- `src/conductor.rs:34205-34207` `build_cache_guards`
- `src/conductor.rs:34208-34210` `build_cache_dirs`
- `src/conductor.rs:34366-34368` `calls`
- `src/eventstore/mod.rs:202-204` `last`
- `src/eventstore/mod.rs:515-517` `recv`
- `src/eventstore/mod.rs:525-527` `try_recv`
- `src/eventstore/mod.rs:530-532` `err`
- `tests/build_env_authority_periphery.rs:378-380` `outputs`
- `tests/rigger_run_base_gate_env_periphery.rs:129-131` `outputs`

#### `dup-0049` (exact, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:14131-14138` `spawn_count`
- `src/conductor.rs:40412-40419` `count`
- `src/conductor.rs:41403-41410` `count`

#### `dup-0050` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/conductor.rs, src/eventstore/mod.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:14142-14148` `spawned`
- `src/eventstore/mod.rs:375-377` `covers`

#### `dup-0051` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/conductor.rs, src/config.rs, src/driver/replay.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:14317-14323` `agent_with_prompt`
- `src/config.rs:1693-1699` `agent`
- `src/driver/replay.rs:1257-1266` `stage`

#### `dup-0052` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/conductor.rs, tests/integrate_conflict_merge_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:14325-14331` `gate_def`
- `tests/integrate_conflict_merge_periphery.rs:332-338` `gate_def`

#### `dup-0053` (near, 4 sites)

Proposed home: `conductor::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:14374-14398` `coverage_gate_refuses_an_uncovered_criterion`
- `src/conductor.rs:29051-29083` `coverage_gap_flags_a_spec_defect_and_errors`
- `src/conductor.rs:29130-29164` `planner_leaving_a_gap_flags_a_spec_defect`
- `src/conductor.rs:29167-29201` `gate_only_stage_is_a_coverage_proxy_gap`

#### `dup-0054` (near, 10 sites)

Proposed home: `a new shared module (sites span 5 files: src/conductor.rs, src/driver/replay.rs, tests/adoption_keys_on_criterion_periphery.rs, tests/replan_episode_identity.rs, tests/scratch_workdir_isolation_leak_guard_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:14804-14831` `supersede_cfg`
- `src/conductor.rs:21803-21825` `sha_stamp_cfg`
- `src/conductor.rs:22112-22132` `degenerate_reviewer_cfg`
- `src/conductor.rs:34268-34290` `content_cache_cfg`
- `src/conductor.rs:40316-40355` `critique_cfg`
- `src/driver/replay.rs:1726-1746` `reviewed_unit_cfg`
- `tests/adoption_keys_on_criterion_periphery.rs:305-336` `baseline_only_cfg`
- `tests/replan_episode_identity.rs:234-296` `two_episode_cfg`
- `tests/replan_episode_identity.rs:1050-1087` `resume_seam_cfg`
- `tests/scratch_workdir_isolation_leak_guard_periphery.rs:86-119` `one_unit_cfg`

#### `dup-0055` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:15687-15783` `a_later_episodes_proposal_supersedes_an_earlier_episodes_planner_unit`
- `src/conductor.rs:15846-15925` `a_planner_proposal_with_no_data_episode_field_still_supersedes_via_meta_spawn`

#### `dup-0056` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:15928-16055` `a_same_id_refine_restamps_its_episode_so_its_own_episodes_sibling_does_not_reap_it`
- `src/conductor.rs:16058-16180` `a_same_id_refine_survives_its_own_episodes_sibling_add_walked_first`
- `src/conductor.rs:16515-16609` `a_legacy_proposal_never_supersedes_an_identified_episodes_owner_even_when_logged_later`

#### `dup-0057` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:16217-16235` `append_one`
- `src/conductor.rs:16362-16379` `append_legacy`
- `src/conductor.rs:16381-16399` `append_identified`

#### `dup-0058` (exact, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:16237-16249` `shape`
- `src/conductor.rs:16401-16413` `shape`

#### `dup-0059` (near, 4 sites)

Proposed home: `conductor::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:16729-16785` `a_verbatim_copy_still_supersedes_its_baseline`
- `src/conductor.rs:16788-16862` `a_paraphrased_proposal_matches_its_baseline_by_id_not_prose`
- `src/conductor.rs:16865-16949` `a_bracketed_id_echo_with_a_paraphrase_still_supersedes_exactly_once`
- `src/conductor.rs:16952-17024` `a_stale_non_matching_id_falls_back_to_the_verbatim_prose_match`

#### `dup-0060` (semantic, 717 sites)

Proposed home: `one .rigger-relative path-composition helper`

mandatory sweep: .rigger-path string literals - 717 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/conductor.rs:17231-17231` `"the repo's own .rigger config must load"`
- `src/config.rs:814-814` `".rigger"`
- `src/config.rs:2286-2286` `".rigger/agents/sdet-author.md"`
- `src/config.rs:2287-2287` `"the shipped .rigger/agents/sdet-author.md must exist"`
- `src/config.rs:2314-2314` `".rigger/agents/sdet.md"`
- `src/config.rs:2315-2315` `"the shipped .rigger/agents/sdet.md must exist"`
- `src/config.rs:2811-2811` `".rigger"`
- `src/config.rs:2812-2812` `"create .rigger dir"`
- `src/config.rs:2844-2844` `".rigger"`
- `src/config.rs:2845-2845` `"create .rigger dir"`
- `src/config.rs:2866-2866` `".rigger"`
- `src/config.rs:2867-2867` `"create .rigger dir"`
- `src/dash.rs:5073-5073` `"{root}/.rigger/events.db"`
- `src/dash.rs:5122-5122` `"/.rigger/events.db"`
- `src/docs.rs:223-223` `"The EVENT LOG accumulates separately from the graph, and has its own prune: `rigger \
         reset --derived`. Each run's project-ingest pass records the project's derived index - \
         the code entities, inferred edges, design links, and doc concepts folded from your \
         sources - and a log written before that pass deduplicated across runs holds the WHOLE \
         index once per run, which is re-derivable duplication rather than history. `rigger reset \
         --derived` keeps the LATEST event per replay key of each derived index type, deletes the \
         superseded re-recordings, and compacts the file so events.db shrinks on disk. Every \
         other event survives byte-for-byte - lessons, decisions, findings, gate verdicts, and \
         the whole run history `rigger stats` and replay read. The live graph is unchanged: every \
         recording of one key folds to the same rows, and the prune carries a pruned key's \
         EARLIEST recorded valid-time onto the recording it keeps, so a design fact keeps the \
         date it first became true rather than being re-dated to whichever recording survived. \
         WHAT IT CANNOT RECLAIM, because this decides whether it is worth running at all: it only \
         ever sheds DUPLICATE recordings of one key, never the index itself. The last recording \
         of every key stays, so on a log that holds no key twice `rigger reset --derived` deletes \
         ZERO rows from it and reports so - that is the expected report on a clean log, not a \
         failure, and the derived index remains the bulk of the log by design because it is what \
         the graph is folded from. WHEN A DEDUPLICATED LOG STILL HAS SOMETHING TO SHED, because \
         a non-zero prune is otherwise read as a broken dedup: a log written since the dedup \
         existed holds one recording per distinct fact EXCEPT where a file's content has \
         RETURNED to a generation the log had already recorded - a revert, a branch switch, a \
         checkout back - which re-records that file's whole batch by design, since a dedup that \
         suppressed an already-recorded key would strand the graph on the version the file has \
         since moved past. A prune that sheds rows on such a log is shedding that duplication, \
         not covering for a defect; a log written BEFORE the dedup sheds the whole accumulated \
         pile instead. WHAT IT COSTS TO RUN: the compaction rewrites events.db in full and stages \
         a COMPLETE COPY of the log in SQLite's temporary directory while it does, so the free \
         space it needs is on whichever filesystem that resolves to rather than on the partition \
         holding .rigger/ - SQLITE_TMPDIR if you set it, else TMPDIR, else the first of /var/tmp, \
         /usr/tmp, /tmp that exists and is writable, which on a Linux box with TMPDIR unset means \
         /var/tmp and NOT /tmp. Set TMPDIR yourself if the default lands somewhere too small for \
         a second copy of your log. It rewrites only when the FILE is holding reclaimable free \
         pages, which is not the same as this run having deleted something: a prune with nothing \
         to shed from an already-compact log leaves the file exactly as it found it and reports \
         reclaiming zero, while a prune that sheds nothing from a log still holding free pages \
         reclaims them. That is what makes the re-run a real remedy - if the rewrite fails after \
         the deletes have committed, the command still reports what it removed and names the \
         failure, and because the deletes are durable and the space they freed is still free in \
         the file, re-running it is both safe and the way to reclaim that space. The two flags \
         COMPOSE \
         and each prunes its own accumulation: `rigger reset --runs --derived` sheds the dead-run \
         graph rows and the duplicated index in one pass. Both are one-shot maintenance you run \
         BETWEEN runs, never against a live one - and `--derived` ENFORCES that itself: a \
         compaction leaves revision gaps by design, and a writer whose cursor was built before it \
         ran could reissue a gap and reorder the log, so it refuses while a `rigger step` holds \
         its lock, a unit in the current run is not yet terminal, a spawn is in flight, or a \
         driver registration for this store is still live, naming what it found. `--force-live` \
         overrides the refusal for an operator certain no writer is using the store; it checks \
         nothing.\n"`
- `src/docs.rs:633-633` `"description: Store hygiene for rigger's own state - growing .rigger/ disk usage, \
         the bloat advisory from `rigger validate`, or `rigger step`/replay running slow. \
         Read this before running `rigger reset` or touching any store file by hand.\n"`
- `src/docs.rs:641-641` `"rigger keeps three stores under `.rigger/`, and only one of them holds anything \
         durable:\n"`
- `src/docs.rs:726-726` `"`rigger graph build` folds the project's source straight into `.rigger/graph.db` - \
         no run, no `RunStarted`, nothing but the code-ingest events the fold already emits. \
         It CREATES the store when the checkout is cold (`.rigger/` does not exist yet) and \
         REFRESHES an existing store incrementally: an unchanged file re-ingests nothing, and \
         it reuses the exact same walk-and-content-key ingest authority a live run uses, so a \
         standalone build and a run can never fold the same file under two different keys.\n"`
- `src/docs.rs:742-742` `"Never force a rebuild by deleting `.rigger/graph.db` (or `events.db`) and \
         re-running `rigger graph build` on the empty result. Deleting the log throws away \
         truth that no rebuild can get back, and deleting only the graph is unnecessary work \
         `rigger graph build` already does FOR you, incrementally, without erasing anything \
         first. If lookups are empty, just run `rigger graph build`; only reach for \
         rigger-reset-store if you specifically mean to prune, not rebuild.\n"`
- `src/docs.rs:777-777` `"`rigger reindex <file>...` re-parses ONLY the named files and persists the delta to \
         the project's symbols grounding index at `.rigger/symbols/` - the fast, targeted fix \
         for an index that has drifted from files you just changed (a unit's own commit, a \
         rebase, a branch switch). It is scoped strictly to the symbols index, a DIFFERENT \
         store from the structural context graph, so it costs only the named files, never a \
         walk of the whole tree.\n"`
- `src/gate.rs:498-498` `".rigger-cache-probe-{}"`
- `src/grounder/mod.rs:406-406` `".rigger"`
- `src/grounder/symbols/store.rs:25-25` `".rigger"`
- `src/grounder/symbols/store.rs:35-35` `".rigger"`
- `src/ingest.rs:741-741` `".rigger"`
- `src/ingest.rs:743-743` `".rigger"`
- `src/ingest.rs:775-775` `".rigger"`
- `src/main.rs:63-63` `".rigger"`
- `src/main.rs:586-586` `"the server event store is selected but no connection string is set - provide one via \
         --conn <url>, the KURRENTDB_CONN environment variable, or the .rigger/store.conn \
         secret file"`
- `src/main.rs:1300-1300` `"migrated project history to the durable identity: renamed {n} stream(s) \
                     from the legacy namespace {legacy:?} to the minted identity {minted:?} \
                     (.rigger/{PROJECT_ID_FILE})"`
- `src/main.rs:1367-1367` `"rigger: migrated project identity - renamed {n} stream(s) from the legacy \
             namespace {legacy:?} to the minted identity {minted:?} (.rigger/{PROJECT_ID_FILE})"`
- `src/main.rs:1484-1484` `"rigger - a config-driven, event-sourced multi-agent dev-loop harness\n\n\
usage:\n  \
rigger run [spec] [opts]    run the workflow (opts below)\n  \
rigger step [--spec <path>]      advance the run one frontier via the replay driver\n            \
[--base <ref>]        and print the newly parked spawn wave + a done flag\n                              \
as JSON. --base (default origin/main) anchors a NEW run\n                              \
branch; if it is unresolvable the branch is created off\n                              \
HEAD. An existing run branch is reused, never reset.\n                              \
--fresh begins a NEW run for the spec even if the latest\n                              \
matches (pass on the first step to restart a wedged run);\n                              \
--rebase-definition accepts a drifted definition and\n                              \
continues, else a live-run step HALTS on definition drift\n  \
rigger reported <id>        exit 0 iff spawn <id> already has a recorded result in\n                              \
this project's run stream (else non-zero). A read-only check\n                              \
of whether a spawn reported yet; the death courier records\n                              \
atomically instead via `rigger result --if-absent`\n  \
rigger prompt <id>          print the parked spawn's full prompt (persona + task).\n                              \
The step wave is a slim manifest; each worker fetches its\n                              \
own prompt from the log by spawn id (spawn-by-reference)\n  \
rigger scratch <id>         print spawn <id>'s own rigger-assigned scratch container\n                              \
(the exact dir the per-spawn reclaim reaps at its terminus);\n                              \
a worker points agent-created scratch and manual\n                              \
CARGO_TARGET_DIR there instead of an unbucketed literal\n  \
rigger workflow [spec]      turn-key: launch the per-project Node driver, which\n                              \
spawns `rigger serve`, runs each agent via the Agent\n                              \
SDK, and drives the loop (one command; run `rigger\n                              \
setup` first - it provisions the driver in .rigger/shim/)\n  \
rigger serve [opts]         run as an MCP server the driver connects to\n  \
rigger graph --around <id>  print the context subgraph around a node\n  \
rigger graph --show <entity> print an entity's definition site + line-numbered body\n                              \
(the text half of lookup; resolves a full id or a bare name)\n  \
rigger graph build          fold the project's source into the graph from a cold\n                              \
checkout (no run required)\n  \
rigger graph communities    derive the code lens's coupling communities offline and\n                              \
[--resolution <r>]          record them as events (deterministic; default r=1.0)\n  \
rigger graph concepts       derive the concepts lens's intent-layer grouping offline\n                              \
[--resolution <r>]          and record them as events (deterministic; default r=1.0)\n  \
rigger stats                print the run's operator metrics: first-pass yield,\n                              \
per-gate remediation counts, escalation rate, and\n                              \
review approve/reject counts. --canary reports the\n                              \
latest canary run's judge-the-judges recall scorecard\n  \
rigger canary               run the review panel against the seeded-defect corpus\n            \
[--corpus <dir>]         (default ./canaries) and score per-tier catch rate,\n                              \
adjudicator correctness, and verdict stability under\n                              \
finding-order shuffle, into the project's canary stream\n                              \
(read back with `rigger stats --canary`)\n            \
[--jobs <n>]              caps the total concurrent review-panel spawns\n                              \
across sharded items and each item's lens fan-out\n                              \
together (default: the crate's default worker\n                              \
width, floored at 2)\n            \
[--model <tier>=<id>]    pins a tier's (lens/adversary/adjudicator)\n                              \
model for this run only, repeatable; the scorecard\n                              \
header prints its resolved id\n  \
rigger playbooks --rebuild  reconstruct the distilled playbook pool under\n                              \
.rigger/playbooks/ from the recorded LessonLearned\n                              \
stream: deduplicated, trigger-scoped agent-files the\n                              \
lessons injector ranks by blast-radius relevance (a\n                              \
rebuildable projection of the log, never hand-edited)\n  \
rigger replay <run|latest>  re-drive a completed run's recorded trajectory under a\n            \
--against <rev>          candidate config (workflow + prompts at git <rev>) in an\n                              \
isolated scratch namespace, and print the stats diff\n                              \
vs the recorded baseline. Never writes the real run\n                              \
stream - past runs become a regression corpus for a\n                              \
config edit (\"did that change regress first-pass yield?\")\n  \
rigger status [--json]      present the live per-agent view of the current run: for\n                              \
each in-flight agent, what it is doing (latest progress),\n                              \
its heartbeat age, and how long since its last store event\n                              \
(the blackout). --json prints the shim/dash machine shape\n  \
rigger dash [--port <n>]    serve the read-only observability page on 127.0.0.1\n                              \
(default port 7420) with live past/present/future views;\n                              \
--export <path> writes the equivalent static snapshot\n  \
rigger watch [--interval <s>] the driver-independent watchdog: polls the store,\n            \
[--once]                  process table, and status for the five rigger-watch-a-run\n                              \
signals (escalated blockers, heartbeat staleness, dash\n                              \
liveness, reject-recurrence trend, frontier progress) plus\n                              \
store integrity, printing one line per anomaly naming\n                              \
signal, subject, and response skill. --once prints standing\n                              \
anomalies and exits (cron/CI); default streams (poll every\n                              \
180s, dedup'd) and never talks to the driver - it works\n                              \
with the driver dead\n  \
rigger ground <query> [k]   print up to k (default 8) repo references the project's\n                              \
configured grounder finds for <query>, as `file:line: text`\n  \
rigger reindex <file>...    incrementally re-index the named files in the project's\n                              \
persisted grounding index (the grounder's reindex), so a\n                              \
later `rigger ground` reflects just-landed changes\n  \
rigger symbols-index [dir]  build + persist the structural symbol index over [dir]\n                              \
(default .) and print its path + file count - the fresh-\n                              \
process determinism harness for the symbols grounder (spec 15)\n  \
rigger emit <type> <json>   append {{type, data:<json>}} to the event store and fold\n                              \
it into the context graph (the CLI form of rigger_emit)\n  \
rigger progress <id> <act>  record one live progress line for spawn <id> to the\n                              \
separate .rigger/progress.db (never the run stream), so an\n                              \
observer can see what a working agent is doing between\n                              \
milestones - `rigger status` and the dash present it\n  \
rigger result <id> [out]    record a parked spawn's outcome to the run log so the next\n                              \
step advances past it: <out> (or stdin) is the agent's output\n                              \
(with --error, its failure message); --if-absent records only\n                              \
if the id has no result; --meta <json> adds bookkeeping\n  \
rigger peers [file ...]     print peer decisions, lessons, and findings from the\n                              \
context graph, scoped to the given files (the CLI form of\n                              \
rigger_peers)\n  \
rigger reset --runs         drop every superseded / dead run's decisions and\n                              \
findings from the context graph, keeping every lesson and\n                              \
the active run's own decisions/findings. Sheds dead-run\n                              \
grounding noise without wiping the store: it deletes no\n                              \
event. reset itself does write the log once, on a store\n                              \
still under the legacy basename namespace: the one-time\n                              \
identity migration renames those streams and records one\n                              \
DecisionMade before either mode prunes\n  \
rigger reset --derived      compact the EVENT LOG: keep the latest event per\n                              \
replay key of each derived index type, delete the\n                              \
superseded re-recordings, and vacuum so the file shrinks\n                              \
on disk. Every other event survives. Sheds the\n                              \
duplication a log accreted before the ingest dedup;\n                              \
composes with --runs (each prunes its own accumulation).\n                              \
Refuses while run machinery looks live (a held step\n                              \
lock, a non-terminal unit, an in-flight spawn, or a live\n                              \
driver registration), naming what is live: compaction\n                              \
leaves revision gaps by design, and a stale writer can\n                              \
reissue one and reorder the log - the corruption this\n                              \
guard exists to prevent. --force-live skips the check\n                              \
entirely and checks nothing: pass it only once you are\n                              \
certain no writer is using this store, since forcing\n                              \
past a genuinely live writer is exactly that corruption\n  \
rigger reset --build-cache  reclaim the SHARED gate build cache (a pure cache,\n                              \
always safe to cold-rebuild) under the scratch root.\n                              \
Composes freely with --runs/--derived (each sheds its\n                              \
own accumulation); reports the exact bytes reclaimed,\n                              \
or 0 when there was nothing to reclaim. Refuses loudly\n                              \
- never waiting - while a rigger-launched build still\n                              \
holds the cache's guard lock; retry once it is idle\n  \
rigger validate             load and validate the workflow + agents\n  \
rigger init                 set up a project: scaffold .rigger/ (workflow.yml +\n                              \
an agents/ folder) and install the Claude Code\n                              \
SessionStart hook (it runs `rigger prime`)\n  \
rigger setup                full setup: everything `init` does, PLUS install the\n                              \
native /rigger Claude Code workflow (.claude/workflows/\n                              \
rigger.js) and provision the JS driver (.rigger/shim/ +\n                              \
npm install). After it: run `/rigger <spec>` in Claude\n                              \
Code (primary), or `rigger workflow` as a fallback\n  \
rigger prime [<spec>]       print recent decisions (what the hook runs); given a spec\n                              \
path, also names `rigger validate <spec>` (the pre-launch\n                              \
spec lint) as a next step\n  \
rigger version              print the crate version and the build-provenance id\n                              \
(a git commit/describe embedded at build time) so an\n                              \
agent can identify the exact binary. Also `--version`\n\n\
run/serve options:\n  \
--driver <cli|workflow>          cli (default): standalone claude subprocess;\n                                   \
workflow: in-Claude-Code MCP server\n  \
--eventstore <sqlite|kurrentdb>  sqlite (default): embedded file in .rigger/;\n                                   \
kurrentdb: shared server backend, always available\n  \
--conn <url>                     KurrentDB connection url (or set KURRENTDB_CONN)\n  \
--fresh                          begin a NEW run even if the latest run matches this\n                                   \
spec (which is otherwise adopted/resumed). The evented\n                                   \
restart for a run wedged in a terminal state (e.g. an\n                                   \
escalated plan-critique) whose spec is unchanged; the\n                                   \
prior run stays in the log as history and context\n  \
--rebase-definition              accept an on-disk definition (workflow.yml + agent\n                                   \
prompts) that drifted from what this live run pinned at\n                                   \
start: record the supersession and continue instead of\n                                   \
halting. The explicit mid-campaign-edit escape (a live\n                                   \
run otherwise HALTS loudly on definition drift)\n\n\
storage and graph live in ./.rigger/ (per project, like .git/), scoped to the\n\
project identity so one backend can hold many projects without their data mixing.\n"`
- `src/main.rs:4068-4068` `"workflow: the per-project JS driver is not provisioned (looked for {}). \
         Run `rigger setup` to write the shim into .rigger/shim/ and install its \
         dependencies, then re-run `rigger workflow`."`
- `src/main.rs:6961-6961` `".rigger"`
- `src/main.rs:8652-8652` `"a `rigger step` is running right now (it holds .rigger/step.lock)"`
- `src/main.rs:9840-9840` `".rigger-workflow-provenance"`
- `src/main.rs:10084-10084` `"warning: tracked .rigger/ files have uncommitted modifications:"`
- `src/main.rs:11470-11470` `".rigger/shim/"`
- `src/main.rs:11471-11471` `".rigger/dash.url"`
- `src/main.rs:11472-11472` `".rigger/dash.marker"`
- `src/main.rs:11473-11473` `".rigger/dash.attempt"`
- `src/main.rs:11474-11474` `".rigger/store.conn"`
- `src/main.rs:11637-11637` `"minted the durable project identity in .rigger/{PROJECT_ID_FILE}: {id} \
             (commit it so a rename never orphans this project's history)"`
- `src/main.rs:11642-11642` `"scaffolded .rigger/workflow.yml"`
- `src/main.rs:11646-11646` `"scaffolded .rigger/agents/{{{}}}"`
- `src/main.rs:11926-11926` `r#"__BEGIN__
# Check rigger's code-derived docs against a fresh render before THIS commit lands, so a
# commit that changes a documented code fact can only land carrying freshly rendered docs.
# SAFE to share: it reads and compares ONLY the two rendered outputs (never other working-tree
# files) and NEVER stages anything itself - staging is always the operator's own act. It acts
# ONLY where the repo already tracks those docs (inert in an operator project that does not
# carry them). A missing or failing `rigger` warns and lets the commit proceed (`rigger
# validate` / the CI drift check is the hard backstop for that case) - but once a render
# succeeds and DIFFERS from what is already staged, the commit is REFUSED rather than
# silently rewritten: a stale binary on PATH must never launder its own re-render into a
# commit over the operator's correctly staged content.
#
# BINARY SELECTION (spec 75): prefer a `rigger` BUILT FROM THIS TREE over whatever happens to
# sit first on PATH, so a worktree whose code legitimately changes a rendered fact renders with
# a binary that actually reflects that change instead of deadlocking against a stale PATH
# install. Candidate order, most authoritative first: the env-provided cargo target dir
# (release then debug), this working tree's own local target (release then debug), this
# worktree's own unit-derived scratch cargo-target - its unit is read from the worktree
# directory name, `rigger-wt-<unit>` (release then debug), the run's shared step-cache target
# (debug only - unit gates build the debug profile only), and finally PATH. SAFE-CLOSED: a
# wrong candidate can only ever convert a false refusal into a pass when its render genuinely
# matches what is already staged - never the reverse - so this can only make the hook MORE
# correct, never less.
git_common_dir=$(git rev-parse --git-common-dir 2>/dev/null)
worktree_top=$(git rev-parse --show-toplevel 2>/dev/null)
worktree_base=$(basename "$worktree_top" 2>/dev/null)
unit=
case "$worktree_base" in
    rigger-wt-*) unit="${worktree_base#rigger-wt-}" ;;
esac
unit_release=
unit_debug=
shared_debug=
if [ -n "$git_common_dir" ]; then
    if [ -n "$unit" ]; then
        unit_release="$git_common_dir/../.rigger/tmp/cargo-target-$unit/release/rigger"
        unit_debug="$git_common_dir/../.rigger/tmp/cargo-target-$unit/debug/rigger"
    fi
    shared_debug="$git_common_dir/../.rigger/tmp/cargo-target/debug/rigger"
fi
rigger_bin=
for candidate in \
    "${CARGO_TARGET_DIR:+$CARGO_TARGET_DIR/release/rigger}" \
    "${CARGO_TARGET_DIR:+$CARGO_TARGET_DIR/debug/rigger}" \
    "./target/release/rigger" \
    "./target/debug/rigger" \
    "$unit_release" \
    "$unit_debug" \
    "$shared_debug" \
; do
    if [ -n "$candidate" ] && [ -x "$candidate" ]; then
        rigger_bin="$candidate"
        break
    fi
done
if [ -z "$rigger_bin" ] && command -v rigger >/dev/null 2>&1; then
    rigger_bin=$(command -v rigger)
fi
if [ -n "$rigger_bin" ]; then
    # Only check in a repo that ALREADY TRACKS these rendered docs (rigger's own self-hosting
    # repo). An operator project never carries them, so leave it untouched.
    tracked=
    untracked=
    for doc in "__SKILL__" "__HANDBOOK__"; do
        if git ls-files --error-unmatch -- "$doc" >/dev/null 2>&1; then
            tracked="${tracked:+$tracked }$doc"
        else
            untracked=1
        fi
    done
    # Check ONLY when EVERY rendered output is already tracked (rigger's own self-hosting
    # repo). If any is untracked - an operator project, or a partial-tracking state - stay inert
    # so `rigger docs` never runs and never creates a stray untracked doc file the operator did
    # not ask for.
    if [ -z "$untracked" ] && [ -n "$tracked" ]; then
        if "$rigger_bin" docs >/dev/null 2>&1; then
            # `rigger docs` just wrote a fresh render into the working tree. Compare it against
            # what is ALREADY STAGED (the index) - never stage the fresh render itself. A doc
            # that differs is drifted: either the staged content is genuinely stale, or this
            # invocation's `rigger` is stale relative to the tree; the hook cannot tell which,
            # so it refuses rather than guessing.
            drifted=
            for doc in $tracked; do
                if [ -f "$doc" ] && ! git diff --quiet -- "$doc" 2>/dev/null; then
                    drifted="${drifted:+$drifted }$doc"
                fi
            done
            if [ -n "$drifted" ]; then
                rigger_prov=$("$rigger_bin" version 2>/dev/null)
                echo "rigger: pre-commit: refusing to commit - the committed docs have drifted from a fresh render: $drifted" 1>&2
                echo "rigger: pre-commit: rendering binary: $rigger_bin ($rigger_prov)" 1>&2
                echo 'rigger: pre-commit: nothing was staged. Fix by either re-rendering with the tree-built binary (rigger docs, then git add the result), or reinstalling rigger so PATH points at a binary built from this tree' 1>&2
                exit 1
            fi
        else
            echo 'rigger: pre-commit: rigger docs failed; committing without regenerated docs (rigger validate is the backstop)' 1>&2
        fi
    fi
else
    echo 'rigger: pre-commit: no rigger binary found (checked the tree build output and PATH); skipping docs regeneration (rigger validate is the backstop)' 1>&2
fi
# Best-effort on unavailability only (the drift check is the hard backstop for that case): a
# matching render (or a `rigger` that could not even attempt one) falls through to here and
# never blocks a commit. A DETECTED drift already `exit 1`'d above and never reaches this line.
true
__END__
"#`
- `src/main.rs:12342-12342` `"imported {} agent {} from {} into .rigger/agents/ ({} kept - already present)"`
- `src/main.rs:12390-12390` `"provisioned the JS driver in .rigger/shim/ (wrote shim.mjs + package.json + \
             package-lock.json and ran npm install)"`
- `src/main.rs:12625-12625` `"kept existing .rigger/agents/{name} (import never overwrites)"`
- `src/main.rs:12653-12653` `"imported .rigger/agents/{name} (id: {id})"`
- `src/main.rs:13563-13563` `"# Scaffolded by `rigger init`. A worked plan -> implement pipeline where the\n\
# review is PER UNIT: each unit implements, three-tier-reviews ITSELF (lenses ->\n\
# adversary -> adjudicator via defaults.review), and integrates in one lifecycle.\n\
# Replace the gate commands with your own.\n\
name: example\n\
\n\
defaults:\n  \
autonomy: auto_notify   # manual | auto_notify | silent\n  \
grounder: symbols       # symbols (default; the structural symbol index) | grep | nop\n  \
# The spawn-budget circuit-breaker: the hard cap on agent spawns one unattended\n  \
# run may make. At the cap the breaker emits BudgetExhausted and aborts the run,\n  \
# so a runaway can never spawn unboundedly. NON-ZERO on purpose - 0 = unlimited.\n  \
budget: 60\n  \
# The remediation depth: how many attempts a failed unit gets before it escalates\n  \
# to a human. This is the REFINEMENT-depth knob, not a review-rigor one - raise it\n  \
# to give a subtle unit room to CONVERGE under the full strict review instead of\n  \
# escalating prematurely. It loosens the depth limit, never the review bar. Absent\n  \
# falls back to 3 (the historical default); bounded by `budget` above.\n  \
max_retries: 3\n  \
# The three-tier review panel applied to EVERY implement unit. Declared once\n  \
# here, inherited by the implement stage and every planner-proposed unit.\n  \
review:\n    \
lenses: [architecture-reviewer, sdet]   # tier 1: the expert lenses\n    \
adversary: adversary           # tier 2: reviews the lenses and refutes them\n    \
adjudicator: adjudicator   # tier 3: neutral judge; its verdict gates the unit\n\
\n\
# The compilation-cache wrapper (spec 65): `auto` probes PATH for a known wrapper\n\
# (sccache, ccache) and uses it when present, so a machine that already has one\n\
# installed benefits with no further config; `off` disables the shared-cache\n\
# layer entirely. See `rigger validate` for the resolved wrapper/cache dir/budget.\n\
build:\n  \
wrapper: auto\n\
\n\
gates:                    # a reusable library of commands, referenced by name\n  \
build: { run: \"echo build ok; true\", kind: core }\n  \
test:  { run: \"echo test ok; true\",  kind: core }\n  \
lint:  { run: \"echo lint ok; true\",  kind: elevated }\n  \
# The check-in-stage mutation sweep (spec 91): runs ONCE, after every implement\n  \
# unit has integrated - never per implementer round. Replace with a real\n  \
# `cargo mutants --in-diff` invocation for a Rust project (see this crate's own\n  \
# .rigger/workflow.yml for the worked example); declaring a gate under this\n  \
# exact id requires `cargo-mutants` on PATH (rigger validate checks at run start).\n  \
mutation: { run: \"echo mutation ok; true\", kind: core }\n\
\n\
stages:\n  \
# The conductor creates one baseline implement unit per acceptance criterion (the\n  \
# deterministic decomposition); this planner REFINES that baseline via UnitProposed.\n  \
# A produces stage decomposes the whole spec, so it has no single coverage criterion\n  \
# - it grounds on the spec's acceptance criteria, not a `coverage` label.\n  \
plan:\n    \
agent: planner\n    \
produces: dag           # refine the spec's unit DAG at runtime\n\
\n  \
# The adversarial plan-critique gate: BEFORE any implementer spawns, the adversary +\n  \
# adjudicator review the PROPOSED unit DAG for the cross-unit hazards per-unit review\n  \
# cannot see: ambiguous mitigation ownership and open dispositions (a shared blast\n  \
# radius is informational only - partition: by-blast-radius serializes it). A reject\n  \
# feeds back to the\n  \
# planner (bounded by max_retries); an approve releases the fan-out. Review-only (no\n  \
# agent) - it critiques the plan, it does not implement.\n  \
plan-critique:\n    \
needs: [plan]\n    \
adversary: adversary        # tier 2: reviews the DAG and refutes it\n    \
adjudicator: adjudicator    # tier 3: its approve/reject gates the fan-out\n\
\n  \
# Each unit implements, three-tier-reviews ITSELF (via defaults.review), and\n  \
# integrates in one lifecycle. A reject or a gate failure feeds back into that\n  \
# same unit's remediation loop; it does NOT integrate until approved + green.\n  \
implement:\n    \
needs: [plan-critique]\n    \
agent: rust-engineer\n    \
strategy: fan-out       # one worker per ready unit, in isolated worktrees\n    \
partition: by-blast-radius\n    \
gates: [build, test, lint]  # red -> green enforced around the change\n    \
on_pass: merge          # land + reindex + record, per unit, once reviewed\n    \
coverage: \"each unit is implemented, reviews itself, and integrates green\"\n\
\n  \
# 3. Check in ONCE, after every implement unit has integrated (spec 91): a\n  \
# `needs` entry naming the fan-out `implement` TEMPLATE is satisfied exactly when\n  \
# every unit it expanded into has integrated - never per implementer round, and\n  \
# never before every unit has landed. Re-verifies the whole gate suite against\n  \
# the merged tree, then sweeps mutants; one remediation round (max_retries: 2),\n  \
# then integrate or escalate with the accounting already on record.\n  \
checkin:\n    \
needs: [implement]\n    \
agent: rust-engineer\n    \
max_retries: 2          # attempt bound: the sweep, one remediation round, the sweep again\n    \
gates: [build, test, lint, mutation]\n    \
on_pass: merge\n    \
coverage: \"mutation efficacy of the whole spec diff\"\n"`
- `src/main.rs:14787-14787` `"/.rigger/tmp/cargo-target/debug/rigger"`
- `src/main.rs:14788-14788` `"/.rigger/tmp/cargo-target/release/rigger"`
- `src/main.rs:16119-16119` `" M .rigger/workflow.yml\n\
                         M  .rigger/agents/sdet.md\n\
                         A  .rigger/agents/new.md\n\
                         D  .rigger/agents/gone.md\n\
                         ?? .rigger/events.db\n\
                         !! .rigger/shim/node_modules\n"`
- `src/main.rs:16129-16129` `".rigger/workflow.yml"`
- `src/main.rs:16130-16130` `".rigger/agents/sdet.md"`
- `src/main.rs:16131-16131` `".rigger/agents/new.md"`
- `src/main.rs:16132-16132` `".rigger/agents/gone.md"`
- `src/main.rs:17132-17132` `".rigger"`
- `src/main.rs:17136-17136` `".rigger"`
- `src/main.rs:17162-17162` `"probe/.rigger/events.db"`
- `src/main.rs:17163-17163` `"rigger-wt-x/.rigger/events.db"`
- `src/main.rs:17315-17315` `".rigger"`
- `src/main.rs:17350-17350` `"rigger-wt-unit-99-ghost-12345678/.rigger/events.db"`
- `src/main.rs:17385-17385` `"probe/.rigger/events.db"`
- `src/main.rs:17396-17396` `"shadow store: probe/.rigger/events.db (6B)"`
- `src/main.rs:17481-17481` `".rigger"`
- `src/main.rs:17548-17548` `".rigger"`
- `src/main.rs:17616-17616` `".rigger"`
- `src/main.rs:17695-17695` `".rigger"`
- `src/main.rs:17801-17801` `".rigger"`
- `src/main.rs:18373-18373` `".rigger"`
- `src/main.rs:18413-18413` `".rigger"`
- `src/main.rs:18595-18595` `".rigger"`
- `src/main.rs:18606-18606` `"must walk past the storeless worktree `.rigger/` to the repo's real store"`
- `src/main.rs:19669-19669` `".rigger"`
- `src/main.rs:19671-19671` `".rigger"`
- `src/main.rs:19726-19726` `".rigger"`
- `src/main.rs:19762-19762` `".rigger"`
- `src/main.rs:19804-19804` `".rigger"`
- `src/main.rs:19910-19910` `".rigger/store.conn beats the committed config"`
- `src/main.rs:20603-20603` `"{name} must be written into .rigger/shim/"`
- `src/main.rs:21152-21152` `".rigger/agents/"`
- `src/main.rs:21178-21178` `".rigger/dash.url"`
- `src/main.rs:21181-21181` `".rigger/dash.marker"`
- `src/main.rs:21184-21184` `".rigger/dash.attempt"`
- `src/main.rs:21193-21193` `".rigger/dash.url"`
- `src/main.rs:21197-21197` `".rigger/dash.marker"`
- `src/main.rs:21201-21201` `".rigger/dash.attempt"`
- `src/main.rs:21212-21212` `".rigger/dash.url"`
- `src/main.rs:21215-21215` `".rigger/dash.marker"`
- `src/main.rs:21218-21218` `".rigger/dash.attempt"`
- `src/main.rs:21227-21227` `".rigger/dash.url"`
- `src/main.rs:21230-21230` `"exactly one .rigger/dash.url ignore line - no duplicate accrued, got:\n{after}"`
- `src/main.rs:21235-21235` `".rigger/dash.marker"`
- `src/main.rs:21238-21238` `"exactly one .rigger/dash.marker ignore line - no duplicate accrued, got:\n{after}"`
- `src/main.rs:21243-21243` `".rigger/dash.attempt"`
- `src/main.rs:21246-21246` `"exactly one .rigger/dash.attempt ignore line - no duplicate accrued, got:\n{after}"`
- `src/main.rs:21266-21266` `".rigger/store.conn"`
- `src/main.rs:21274-21274` `".rigger/store.conn"`
- `src/main.rs:21283-21283` `".rigger/store.conn"`
- `src/main.rs:21292-21292` `".rigger/store.conn"`
- `src/main.rs:21295-21295` `"exactly one .rigger/store.conn ignore line - no duplicate accrued, got:\n{after}"`
- `src/main.rs:21336-21336` `".rigger/\n"`
- `src/main.rs:21342-21342` `".rigger/dash.url"`
- `src/main.rs:21345-21345` `".rigger/dash.marker"`
- `src/main.rs:21348-21348` `".rigger/dash.attempt"`
- `src/main.rs:21349-21349` `"setup appends the explicit dash lines (including the round-8 attempt breadcrumb) \
             even when .rigger/ broadly covers them, so the committed .gitignore stays \
             self-contained, got: {:?}"`
- `src/main.rs:21357-21357` `".rigger/dash.url"`
- `src/main.rs:21358-21358` `".rigger/dash.marker"`
- `src/main.rs:21359-21359` `".rigger/dash.attempt"`
- `src/main.rs:21360-21360` `"all three explicit per-file dash ignore lines are present in the committed \
             .gitignore even though .rigger/ already covers them, got:\n{content}"`
- `src/main.rs:21370-21370` `".rigger/dash.url"`
- `src/main.rs:21373-21373` `".rigger/dash.marker"`
- `src/main.rs:21376-21376` `".rigger/dash.attempt"`
- `src/main.rs:21636-21636` `".rigger/agents/researcher.md"`
- `src/main.rs:21665-21665` `".rigger/agents/planner.md"`
- `src/main.rs:21695-21695` `".rigger/agents/newcomer.md"`
- `src/main.rs:21743-21743` `".rigger/agents/my-planner.md"`
- `src/main.rs:21768-21768` `".rigger/agents/a-dup.md"`
- `src/main.rs:21769-21769` `".rigger/agents/b-dup.md"`
- `src/main.rs:21796-21796` `".rigger/agents/blank.md"`
- `src/main.rs:21811-21811` `".rigger/workflow.yml"`
- `src/main.rs:22859-22859` `"locate_shim must return the provisioned .rigger/shim/shim.mjs"`
- `src/main.rs:23977-23977` `".rigger"`
- `src/main.rs:24031-24031` `".rigger"`
- `src/reap.rs:514-514` `".rigger"`
- `src/reap.rs:871-871` `"a relocated/cache-home-style authorized_root with no .rigger/tmp relationship \
             must still authorize the reap"`
- `src/registry.rs:302-302` `"/home/dev/proj/.rigger/events.db"`
- `src/registry.rs:320-320` `"/home/dev/proj/.rigger/events.db"`
- `src/registry.rs:338-338` `"/a/.rigger/events.db"`
- `src/registry.rs:339-339` `"/b/.rigger/events.db"`
- `src/registry.rs:352-352` `"/home/dev/proj/.rigger/events.db"`
- `src/registry.rs:368-368` `"/home/dev/proj/.rigger/events.db"`
- `src/registry.rs:394-394` `"/home/dev/proj/.rigger/events.db"`
- `src/registry.rs:414-414` `"/home/dev/proj/.rigger/events.db"`
- `src/registry.rs:433-433` `"/home/dev/proj-b/.rigger/events.db"`
- `src/registry.rs:453-453` `"/a/.rigger/events.db"`
- `src/registry.rs:454-454` `"/b/.rigger/events.db"`
- `src/worktree.rs:1409-1409` `"{}/.rigger/tmp"`
- `src/worktree.rs:4117-4117` `"{repo_path}/.rigger/tmp"`
- `src/worktree.rs:4118-4118` `"the default must no longer nest inside the repo's own .rigger: {dflt:?}"`
- `src/worktree.rs:4121-4121` `"/.rigger/"`
- `src/worktree.rs:4121-4121` `"/.rigger"`
- `src/worktree.rs:4122-4122` `"the default must never live under any .rigger: {dflt:?}"`
- `src/worktree.rs:4141-4141` `"{repo_path}/.rigger/tmp"`
- `src/worktree.rs:4165-4165` `"~/.rigger-scratch-test"`
- `src/worktree.rs:4166-4166` `"{home}/.rigger-scratch-test"`
- `src/worktree.rs:5977-5977` `"{base}..rigger-run"`
- `src/worktree.rs:6026-6026` `".rigger"`
- `tests/architecture_current_surface.rs:107-107` `".rigger/store.conn"`
- `tests/architecture_current_surface.rs:163-163` `"docs/architecture.md must describe the system that exists today (spec 56, \
         criterion 1): it must name the store-resolution and configuration surface (the \
         committed `store:` selection, the `KURRENTDB_CONN` environment variable, and the \
         per-machine `.rigger/store.conn` secret file) and the graph inspector's real query \
         surface (the three `lens=` names and the directed `view=calls` `dir=` views). \
         Surfaces the document fails to name: {missing:#?}"`
- `tests/build_budget_slots_periphery.rs:207-207` `".rigger"`
- `tests/build_budget_slots_periphery.rs:208-208` `"create .rigger/agents"`
- `tests/build_env_authority_periphery.rs:166-166` `".rigger"`
- `tests/build_env_authority_periphery.rs:167-167` `"create .rigger/agents"`
- `tests/canary_model_drift_periphery.rs:90-90` `".rigger"`
- `tests/cause_wire_periphery.rs:66-66` `".rigger"`
- `tests/cause_wire_periphery.rs:86-86` `".rigger"`
- `tests/cause_wire_periphery.rs:104-104` `".rigger"`
- `tests/change_path_revert_periphery.rs:110-110` `".rigger"`
- `tests/change_path_revert_periphery.rs:144-144` `".rigger"`
- `tests/change_path_revert_periphery.rs:165-165` `".rigger"`
- `tests/checkin_mutation_diff_base_periphery.rs:43-43` `".rigger"`
- `tests/cli.rs:39-39` `".rigger"`
- `tests/cli.rs:60-60` `".rigger"`
- `tests/cli.rs:87-87` `".rigger"`
- `tests/cli.rs:239-239` `".rigger"`
- `tests/cli.rs:243-243` `".rigger"`
- `tests/cli.rs:305-305` `".rigger"`
- `tests/cli.rs:425-425` `".rigger"`
- `tests/cli.rs:436-436` `".rigger"`
- `tests/cli.rs:887-887` `".rigger"`
- `tests/cli.rs:998-998` `".rigger"`
- `tests/cli.rs:1053-1053` `".rigger"`
- `tests/cli.rs:1173-1173` `".rigger"`
- `tests/cli.rs:1203-1203` `".rigger"`
- `tests/cli.rs:1360-1360` `".rigger"`
- `tests/cli.rs:1386-1386` `".rigger"`
- `tests/cli.rs:1484-1484` `"/.rigger/"`
- `tests/cli.rs:1485-1485` `"the default must never live under any .rigger, even on the HOME-only fallback \
         rung; got: {stdout:?}"`
- `tests/cli.rs:1539-1539` `"{}/.rigger/tmp"`
- `tests/cli.rs:1551-1551` `"/.rigger/tmp/"`
- `tests/cli.rs:1575-1575` `".rigger"`
- `tests/cli.rs:1665-1665` `".rigger"`
- `tests/cli.rs:1675-1675` `".rigger"`
- `tests/cli.rs:1742-1742` `".rigger"`
- `tests/cli.rs:1743-1743` `".rigger"`
- `tests/cli.rs:1755-1755` `".rigger"`
- `tests/cli.rs:1777-1777` `".rigger"`
- `tests/cli.rs:1846-1846` `".rigger"`
- `tests/cli.rs:1884-1884` `".rigger"`
- `tests/cli.rs:1904-1904` `".rigger"`
- `tests/cli.rs:1937-1937` `".rigger"`
- `tests/cli.rs:1952-1952` `".rigger"`
- `tests/cli.rs:2612-2612` `".rigger"`
- `tests/cli.rs:2710-2710` `".rigger"`
- `tests/cli.rs:2751-2751` `".rigger"`
- `tests/cli.rs:2783-2783` `".rigger"`
- `tests/cli.rs:2784-2784` `"graph build must create .rigger/graph.db from a cold checkout"`
- `tests/cli.rs:2834-2834` `".rigger"`
- `tests/cli.rs:2835-2835` `"graph build must create .rigger/graph.db even when there is nothing to ingest"`
- `tests/cli.rs:2858-2858` `".rigger"`
- `tests/cli.rs:2936-2936` `".rigger"`
- `tests/cli.rs:2951-2951` `".rigger"`
- `tests/cli.rs:2999-2999` `".rigger"`
- `tests/cli.rs:3024-3024` `".rigger"`
- `tests/cli.rs:3143-3143` `".rigger"`
- `tests/cli.rs:3147-3147` `"grounding via symbols must persist the structural index to .rigger/symbols/"`
- `tests/cli.rs:3320-3320` `".rigger"`
- `tests/cli.rs:3596-3596` `".rigger"`
- `tests/cli.rs:3741-3741` `".rigger"`
- `tests/cli.rs:3948-3948` `"cd ${REPO} && CARGO_TARGET_DIR=${REPO}/.rigger/tmp/cargo-target rigger step"`
- `tests/cli.rs:3996-3996` `"cd ${REPO} && CARGO_TARGET_DIR=${REPO}/.rigger/tmp/cargo-target rigger step"`
- `tests/cli.rs:4183-4183` `".rigger/tmp/agent-scratch/"`
- `tests/cli.rs:4189-4189` `".rigger/tmp/cargo-target"`
- `tests/cli.rs:4197-4197` `"CARGO_TARGET_DIR=${REPO}/.rigger/tmp/cargo-target rigger step"`
- `tests/cli.rs:4445-4445` `".rigger"`
- `tests/cli.rs:4474-4474` `".rigger"`
- `tests/cli.rs:4584-4584` `".rigger"`
- `tests/cli.rs:4700-4700` `".rigger"`
- `tests/cli.rs:4803-4803` `".rigger"`
- `tests/cli.rs:4995-4995` `".rigger"`
- `tests/cli.rs:5026-5026` `".rigger"`
- `tests/cli.rs:5469-5469` `".rigger"`
- `tests/cli.rs:5639-5639` `".rigger"`
- `tests/cli.rs:5709-5709` `".rigger"`
- `tests/cli.rs:5847-5847` `".rigger"`
- `tests/cli.rs:5904-5904` `".rigger"`
- `tests/cli.rs:6110-6110` `".rigger"`
- `tests/cli.rs:6164-6164` `".rigger"`
- `tests/cli.rs:6334-6334` `".rigger"`
- `tests/cli.rs:6440-6440` `".rigger"`
- `tests/cli.rs:6519-6519` `".rigger"`
- `tests/cli.rs:6652-6652` `".rigger"`
- `tests/cli.rs:6709-6709` `".rigger"`
- `tests/cli.rs:6894-6894` `".rigger"`
- `tests/cli.rs:6967-6967` `".rigger"`
- `tests/cli.rs:7073-7073` `".rigger"`
- `tests/cli.rs:7142-7142` `".rigger"`
- `tests/cli.rs:7244-7244` `".rigger"`
- `tests/cli.rs:7310-7310` `".rigger"`
- `tests/cli.rs:7444-7444` `".rigger"`
- `tests/cli.rs:7512-7512` `".rigger"`
- `tests/cli.rs:7643-7643` `".rigger"`
- `tests/cli.rs:7743-7743` `".rigger/events.db"`
- `tests/cli.rs:7942-7942` `".rigger"`
- `tests/cli.rs:7976-7976` `".rigger"`
- `tests/cli.rs:8204-8204` `".rigger"`
- `tests/cli.rs:8294-8294` `".rigger"`
- `tests/cli.rs:8357-8357` `"/.rigger/"`
- `tests/cli.rs:8358-8358` `"the default marker path must never live under any .rigger; got: {marker_str:?}"`
- `tests/cli.rs:8642-8642` `".rigger"`
- `tests/cli.rs:8942-8942` `"/.rigger/tmp/agent-live/"`
- `tests/cli.rs:8943-8943` `"under RIGGER_TMPDIR the marker is not under the repo's .rigger/tmp; got: {marker_str:?}"`
- `tests/cli.rs:9724-9724` `".rigger"`
- `tests/cli.rs:9887-9887` `".rigger"`
- `tests/cli.rs:9991-9991` `".rigger"`
- `tests/cli.rs:10040-10040` `".rigger"`
- `tests/cli.rs:10142-10142` `".rigger"`
- `tests/cli.rs:10201-10201` `".rigger"`
- `tests/cli.rs:10305-10305` `".rigger"`
- `tests/cli.rs:10372-10372` `".rigger"`
- `tests/cli.rs:10514-10514` `".rigger"`
- `tests/cli.rs:10562-10562` `".rigger"`
- `tests/cli.rs:10680-10680` `".rigger"`
- `tests/cli.rs:10951-10951` `".rigger"`
- `tests/cli.rs:11050-11050` `".rigger"`
- `tests/cli.rs:11139-11139` `".rigger"`
- `tests/cli.rs:11179-11179` `".rigger"`
- `tests/cli.rs:11836-11836` `".rigger"`
- `tests/cli.rs:11998-11998` `".rigger"`
- `tests/cli.rs:12377-12377` `".rigger/workflow.yml"`
- `tests/cli.rs:12377-12377` `".rigger/agents"`
- `tests/cli.rs:12442-12442` `".rigger"`
- `tests/cli.rs:12476-12476` `".rigger/workflow.yml"`
- `tests/cli.rs:12476-12476` `".rigger/agents"`
- `tests/cli.rs:12479-12479` `".rigger/workflow.yml"`
- `tests/cli.rs:12511-12511` `".rigger"`
- `tests/cli.rs:12545-12545` `".rigger/workflow.yml"`
- `tests/cli.rs:12545-12545` `".rigger/agents"`
- `tests/cli.rs:12548-12548` `".rigger/workflow.yml"`
- `tests/cli.rs:12583-12583` `".rigger"`
- `tests/cli.rs:12622-12622` `".rigger/workflow.yml"`
- `tests/cli.rs:12622-12622` `".rigger/agents"`
- `tests/cli.rs:12625-12625` `".rigger/workflow.yml"`
- `tests/cli.rs:12659-12659` `".rigger"`
- `tests/cli.rs:12697-12697` `".rigger/workflow.yml"`
- `tests/cli.rs:12697-12697` `".rigger/agents"`
- `tests/cli.rs:12700-12700` `".rigger/workflow.yml"`
- `tests/cli.rs:12755-12755` `".rigger/workflow.yml"`
- `tests/cli.rs:12755-12755` `".rigger/agents"`
- `tests/cli.rs:12802-12802` `".rigger/workflow.yml"`
- `tests/cli.rs:12802-12802` `".rigger/agents"`
- `tests/cli.rs:12815-12815` `".rigger"`
- `tests/cli.rs:12824-12824` `".rigger"`
- `tests/cli.rs:12826-12826` `".rigger"`
- `tests/cli.rs:12923-12923` `".rigger/workflow.yml"`
- `tests/cli.rs:12924-12924` `"validate must NOT flag a clean tracked `.rigger/` tree; stderr:\n{err}"`
- `tests/cli.rs:12933-12933` `".rigger"`
- `tests/cli.rs:12940-12940` `"validate must still succeed (exit 0) when it only FLAGS uncommitted `.rigger/` \
         changes; stderr:\n{err}"`
- `tests/cli.rs:12944-12944` `".rigger/workflow.yml"`
- `tests/cli.rs:12945-12945` `"validate must flag the tracked-but-modified `.rigger/workflow.yml` on stderr; \
         stderr:\n{err}"`
- `tests/cli.rs:12961-12961` `".rigger"`
- `tests/cli.rs:13206-13206` `".rigger"`
- `tests/cli.rs:13623-13623` `".rigger"`
- `tests/cli.rs:13767-13767` `".rigger"`
- `tests/cli.rs:13769-13769` `".rigger"`
- `tests/cli.rs:13772-13772` `".rigger"`
- `tests/cli.rs:13774-13774` `".rigger"`
- `tests/cli.rs:13802-13802` `"probe/.rigger/events.db"`
- `tests/cli.rs:13866-13866` `"validate must warn about residue planted under the relocated cache-home DEFAULT \
         root - a regression that left its residue scan still rooted at the pre-relocation \
         `.rigger/tmp` would silently miss this and print nothing; stderr:\n{err}"`
- `tests/cli.rs:13896-13896` `".rigger"`
- `tests/cli.rs:14816-14816` `".rigger"`
- `tests/cli.rs:14913-14913` `"scaffolded .rigger/workflow.yml"`
- `tests/cli.rs:14917-14917` `"scaffolded .rigger/agents/"`
- `tests/cli.rs:14949-14949` `".rigger"`
- `tests/cli.rs:14981-14981` `".rigger/agents/researcher.md"`
- `tests/cli.rs:14982-14982` `"the agent was actually imported into .rigger/agents/"`
- `tests/cli.rs:15029-15029` `".rigger"`
- `tests/cli.rs:15258-15258` `".rigger/dash.url"`
- `tests/cli.rs:15262-15262` `".rigger/dash.marker"`
- `tests/cli.rs:15266-15266` `".rigger/dash.attempt"`
- `tests/cli.rs:15275-15275` `".rigger"`
- `tests/cli.rs:15277-15277` `".rigger"`
- `tests/cli.rs:15281-15281` `".rigger"`
- `tests/cli.rs:15282-15282` `".rigger"`
- `tests/cli.rs:15284-15284` `".rigger/dash.url"`
- `tests/cli.rs:15285-15285` `".rigger/dash.marker"`
- `tests/cli.rs:15286-15286` `".rigger/dash.attempt"`
- `tests/cli.rs:15326-15326` `".claude/\n.rigger/\n"`
- `tests/cli.rs:15342-15342` `".rigger/dash.url"`
- `tests/cli.rs:15350-15350` `"the test's global config must actually ignore .rigger/dash.url (else the regression \
         guard is inconclusive)"`
- `tests/cli.rs:15373-15373` `".rigger/shim"`
- `tests/cli.rs:15374-15374` `".rigger/dash.url"`
- `tests/cli.rs:15375-15375` `".rigger/dash.marker"`
- `tests/cli.rs:15376-15376` `".rigger/dash.attempt"`
- `tests/cli.rs:15580-15580` `".rigger/project.id"`
- `tests/cli.rs:15583-15583` `".rigger/project.id"`
- `tests/cli.rs:15596-15596` `".rigger/project.id"`
- `tests/cli.rs:15685-15685` `".rigger/project.id"`
- `tests/cli.rs:15734-15734` `".rigger/project.id"`
- `tests/cli.rs:15739-15739` `".rigger/project.id"`
- `tests/cli.rs:15753-15753` `".rigger"`
- `tests/cli.rs:15896-15896` `".rigger"`
- `tests/cli.rs:16015-16015` `".rigger"`
- `tests/cli.rs:16107-16107` `".rigger"`
- `tests/cli.rs:16172-16172` `".rigger"`
- `tests/cli.rs:16242-16242` `".rigger"`
- `tests/cli.rs:16606-16606` `".rigger"`
- `tests/cli.rs:16674-16674` `".rigger"`
- `tests/cli.rs:16811-16811` `".rigger"`
- `tests/cli.rs:17010-17010` `".rigger"`
- `tests/cli.rs:17200-17200` `".rigger"`
- `tests/cli.rs:17382-17382` `".rigger"`
- `tests/cli.rs:17427-17427` `".rigger"`
- `tests/cli.rs:17865-17865` `".rigger"`
- `tests/cli.rs:17880-17880` `"the driver never recorded a dash URL in .rigger/dash.url; stderr:\n{err}"`
- `tests/cli.rs:17925-17925` `".rigger"`
- `tests/cli.rs:17988-17988` `".rigger"`
- `tests/cli.rs:18063-18063` `".rigger"`
- `tests/cli.rs:18139-18139` `".rigger"`
- `tests/cli.rs:18223-18223` `".rigger"`
- `tests/cli.rs:18330-18330` `".rigger"`
- `tests/cli.rs:19046-19046` `".rigger"`
- `tests/cli.rs:19048-19048` `".rigger"`
- `tests/cli.rs:19745-19745` `".rigger/dash.marker"`
- `tests/cli.rs:19786-19786` `".rigger/dash.marker"`
- `tests/cli.rs:19789-19789` `".rigger/dash.url"`
- `tests/cli.rs:19841-19841` `".rigger/dash.url"`
- `tests/cli.rs:19846-19846` `".rigger/dash.marker"`
- `tests/cli.rs:19902-19902` `".rigger/dash.url"`
- `tests/cli.rs:19904-19904` `".rigger/dash.marker"`
- `tests/cli.rs:19973-19973` `".rigger/dash.marker"`
- `tests/cli.rs:20031-20031` `".rigger/dash.marker"`
- `tests/cli.rs:20136-20136` `".rigger/dash.marker"`
- `tests/cli.rs:20190-20190` `".rigger/dash.marker"`
- `tests/cli.rs:20212-20212` `".rigger/dash.attempt"`
- `tests/cli.rs:20222-20222` `"a marker that LOOKS like it predates this run's own RunStarted must still be reported \
         when .rigger/dash.attempt explicitly names this exact run - proving watch_poll's own \
         file-read-and-match wiring (not merely the pure watch::detect fallback comparison, \
         which alone would suppress this exact shape) is what forced the report; got:\n{out}"`
- `tests/cli.rs:20270-20270` `".rigger/dash.marker"`
- `tests/cli.rs:20292-20292` `".rigger/dash.attempt"`
- `tests/cli.rs:21018-21018` `".rigger"`
- `tests/cli.rs:21833-21833` `".rigger"`
- `tests/cli.rs:22174-22174` `".rigger"`
- `tests/cli.rs:22209-22209` `".rigger"`
- `tests/cli.rs:22284-22284` `"the first step must record a dash marker at .rigger/dash.marker; stderr:\n{err1}"`
- `tests/cli.rs:22365-22365` `"the step must record a dash marker at .rigger/dash.marker; stderr:\n{err}"`
- `tests/cli.rs:22379-22379` `"`rigger step` under RIGGER_DASH_PORT={dash_port} must bind its step-path dash at EXACTLY \
         that port and record it in .rigger/dash.marker (proving the override reaches the real \
         bind, not the fixed dash::DEFAULT_PORT); the marker instead recorded {marker_port}"`
- `tests/cli.rs:22440-22440` `".rigger"`
- `tests/cli.rs:22521-22521` `".rigger"`
- `tests/cli.rs:22592-22592` `".rigger"`
- `tests/cli.rs:22964-22964` `".rigger"`
- `tests/cli.rs:22976-22976` `".rigger"`
- `tests/cli.rs:23003-23003` `".rigger"`
- `tests/cli.rs:23031-23031` `".rigger"`
- `tests/cli.rs:23042-23042` `".rigger"`
- `tests/cli.rs:23079-23079` `".rigger"`
- `tests/cli.rs:23168-23168` `"the first step must record a dash marker at .rigger/dash.marker; stderr:\n{err1}"`
- `tests/cli.rs:23240-23240` `"{root}/.rigger/events.db"`
- `tests/cli.rs:23257-23257` `"{root}/.rigger/events.db"`
- `tests/cli.rs:24148-24148` `"{other_root}/.rigger/events.db"`
- `tests/cli.rs:24527-24527` `"/stale/root/.rigger/events.db"`
- `tests/cli.rs:24548-24548` `"/live/root/.rigger/events.db"`
- `tests/cli.rs:24650-24650` `"the step must record a dash marker at .rigger/dash.marker"`
- `tests/cli.rs:24850-24850` `".rigger"`
- `tests/cli.rs:25032-25032` `".rigger"`
- `tests/cli.rs:25135-25135` `".rigger"`
- `tests/cli.rs:25260-25260` `".rigger"`
- `tests/cli.rs:25902-25902` `".rigger"`
- `tests/cli.rs:25921-25921` `".rigger/workflow.yml must define a `checkin:` stage (spec 91): {text:?}"`
- `tests/cli.rs:25925-25925` `".rigger/workflow.yml must define a `mutation:` gate that invokes cargo mutants \
         (spec 91): {text:?}"`
- `tests/cli.rs:25930-25930` `".rigger/workflow.yml's checkin stage / mutation gate definition must name spec 91, \
         so drift in the committed workflow fails this suite instead of silently diverging \
         from the spec it satisfies: {text:?}"`
- `tests/cli.rs:25956-25956` `"this repository's own .rigger/workflow.yml and agents must load: {e}"`
- `tests/cli.rs:25963-25963` `".rigger/workflow.yml must define a `checkin` stage (spec 91)"`
- `tests/cli.rs:25998-25998` `".rigger/workflow.yml must define a `mutation` gate (spec 91)"`
- `tests/cli.rs:26019-26019` `"this repository's own committed .rigger/workflow.yml must pass Config::validate \
         on a correctly-provisioned machine (cargo-mutants installed)"`
- `tests/cli.rs:26064-26064` `".rigger/dash.attempt"`
- `tests/cli.rs:26065-26065` `"a real step's own ensure_run_dashboard call must record .rigger/dash.attempt \
         (record_dash_attempt); without it this test cannot exercise the round-8 fact at all"`
- `tests/cli.rs:26122-26122` `".rigger/dash.url"`
- `tests/cli.rs:26130-26130` `".rigger/dash.marker"`
- `tests/cli.rs:26194-26194` `".rigger/dash.marker"`
- `tests/cli.rs:26223-26223` `".rigger/dash.url"`
- `tests/cli.rs:26230-26230` `".rigger/dash.attempt"`
- `tests/cli.rs:26291-26291` `".rigger/dash.marker"`
- `tests/cli.rs:26294-26294` `".rigger/dash.url"`
- `tests/cli.rs:26348-26348` `".rigger/dash.marker"`
- `tests/cli.rs:26370-26370` `".rigger/dash.url"`
- `tests/cli.rs:26375-26375` `".rigger/dash.attempt"`
- `tests/cli.rs:26414-26414` `".rigger/dash.url"`
- `tests/cli.rs:26425-26425` `".rigger/dash.marker"`
- `tests/cli.rs:26461-26461` `".rigger/dash.url"`
- `tests/cli.rs:26469-26469` `".rigger/dash.marker"`
- `tests/cli.rs:26521-26521` `".rigger/dash.url"`
- `tests/cli.rs:26526-26526` `".rigger/dash.marker"`
- `tests/cli.rs:26588-26588` `".rigger/dash.url"`
- `tests/cli.rs:26596-26596` `".rigger/dash.marker"`
- `tests/cli.rs:26849-26849` `".rigger"`
- `tests/cli.rs:27557-27557` `".rigger"`
- `tests/cli.rs:27595-27595` `".rigger"`
- `tests/cli.rs:28526-28526` `".rigger"`
- `tests/cli.rs:28651-28651` `"the hook must be inert on a project without .rigger/; got:\n{out}"`
- `tests/cli.rs:28657-28657` `".rigger"`
- `tests/cli.rs:28675-28675` `".rigger"`
- `tests/cli.rs:28703-28703` `".rigger"`
- `tests/cli.rs:28748-28748` `".rigger"`
- `tests/cli.rs:28789-28789` `".rigger"`
- `tests/cli.rs:28813-28813` `".rigger"`
- `tests/cli.rs:28857-28857` `".rigger"`
- `tests/cli.rs:28888-28888` `".rigger"`
- `tests/cli.rs:28915-28915` `".rigger"`
- `tests/cli.rs:28948-28948` `".rigger"`
- `tests/cli.rs:28973-28973` `".rigger"`
- `tests/cli.rs:28995-28995` `".rigger"`
- `tests/cli.rs:29017-29017` `".rigger"`
- `tests/cli.rs:29045-29045` `".rigger"`
- `tests/cli.rs:29071-29071` `".rigger"`
- `tests/cli.rs:29106-29106` `".rigger"`
- `tests/cli.rs:29153-29153` `".rigger"`
- `tests/cli.rs:29207-29207` `".rigger"`
- `tests/cli.rs:29249-29249` `".rigger"`
- `tests/cli.rs:29306-29306` `".rigger"`
- `tests/cli.rs:29637-29637` `".rigger"`
- `tests/common/mod.rs:225-225` `"{}/.rigger-test-scratch"`
- `tests/community_detection_cli.rs:64-64` `".rigger"`
- `tests/community_detection_cli.rs:65-65` `".rigger"`
- `tests/community_detection_cli.rs:71-71` `".rigger"`
- `tests/concepts_derivation_cli.rs:69-69` `".rigger"`
- `tests/concepts_derivation_cli.rs:70-70` `".rigger"`
- `tests/concepts_derivation_cli.rs:76-76` `".rigger"`
- `tests/courier_registry_refresh_boundary_periphery.rs:60-60` `".rigger"`
- `tests/courier_registry_refresh_boundary_periphery.rs:61-61` `"create .rigger"`
- `tests/courier_registry_refresh_boundary_periphery.rs:148-148` `".rigger"`
- `tests/courier_registry_refresh_fence_periphery.rs:44-44` `".rigger"`
- `tests/courier_registry_refresh_fence_periphery.rs:45-45` `"create .rigger"`
- `tests/courier_registry_refresh_fence_periphery.rs:98-98` `".rigger"`
- `tests/courier_registry_refresh_periphery.rs:42-42` `".rigger"`
- `tests/courier_registry_refresh_periphery.rs:43-43` `"create .rigger"`
- `tests/dedup_seeding_periphery.rs:345-345` `".rigger"`
- `tests/dedup_seeding_periphery.rs:372-372` `".rigger"`
- `tests/dedup_seeding_periphery.rs:594-594` `".rigger"`
- `tests/dedup_seeding_periphery.rs:616-616` `".rigger"`
- `tests/dedup_seeding_periphery.rs:636-636` `".rigger"`
- `tests/duplication_catalog_contract_periphery.rs:93-93` `".rigger-path string literals"`
- `tests/escalation_resume_periphery.rs:91-91` `".rigger"`
- `tests/escalation_resume_periphery.rs:110-110` `".rigger"`
- `tests/escalation_resume_periphery.rs:127-127` `".rigger"`
- `tests/escalation_resume_periphery.rs:145-145` `".rigger"`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:147-147` `".rigger"`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:176-176` `".rigger"`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:211-211` `".rigger"`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:224-224` `".rigger"`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:252-252` `".rigger"`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:373-373` `".rigger"`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:715-715` `".rigger"`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:1255-1255` `".rigger"`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:1313-1313` `".rigger"`
- `tests/gate_store_fence_periphery.rs:208-208` `".rigger"`
- `tests/gate_store_fence_periphery.rs:209-209` `".rigger"`
- `tests/gate_store_fence_periphery.rs:214-214` `".rigger"`
- `tests/gate_store_fence_periphery.rs:215-215` `".rigger"`
- `tests/gate_store_fence_periphery.rs:217-217` `".rigger"`
- `tests/gate_store_fence_periphery.rs:222-222` `".rigger"`
- `tests/gate_store_fence_periphery.rs:455-455` `".rigger"`
- `tests/gate_store_fence_periphery.rs:478-478` `".rigger"`
- `tests/gate_store_fence_periphery.rs:529-529` `".rigger"`
- `tests/gate_store_fence_periphery.rs:530-530` `".rigger"`
- `tests/gate_store_fence_periphery.rs:543-543` `".rigger"`
- `tests/gate_store_fence_periphery.rs:546-546` `".rigger"`
- `tests/gate_store_fence_periphery.rs:619-619` `".rigger"`
- `tests/gate_store_fence_periphery.rs:620-620` `".rigger"`
- `tests/gate_store_fence_periphery.rs:637-637` `".rigger"`
- `tests/gate_store_fence_periphery.rs:639-639` `".rigger"`
- `tests/gate_store_fence_periphery.rs:720-720` `".rigger"`
- `tests/gate_store_fence_periphery.rs:721-721` `".rigger"`
- `tests/gate_store_fence_periphery.rs:731-731` `".rigger"`
- `tests/gate_store_fence_periphery.rs:733-733` `".rigger"`
- `tests/gate_store_fence_periphery.rs:835-835` `".rigger"`
- `tests/gate_store_fence_periphery.rs:836-836` `".rigger"`
- `tests/graph_show_periphery.rs:81-81` `".rigger"`
- `tests/graph_show_periphery.rs:134-134` `".rigger"`
- `tests/graph_show_staleness.rs:69-69` `".rigger"`
- `tests/graph_show_staleness.rs:75-75` `".rigger"`
- `tests/graph_show_surface.rs:68-68` `".rigger"`
- `tests/graph_show_surface.rs:150-150` `".rigger"`
- `tests/halted_spawn_wip_recovery_periphery.rs:135-135` `".rigger"`
- `tests/halted_spawn_wip_recovery_periphery.rs:447-447` `".rigger"`
- `tests/halted_spawn_wip_recovery_periphery.rs:470-470` `".rigger"`
- `tests/halted_spawn_wip_recovery_periphery.rs:504-504` `".rigger"`
- `tests/handbook_grounder_accuracy.rs:134-134` `".rigger"`
- `tests/handbook_grounder_accuracy.rs:143-143` `".rigger/workflow.yml must carry a `grounder:` default (spec 57, criterion 4)"`
- `tests/handbook_grounder_accuracy.rs:148-148` `"the repo's own .rigger/workflow.yml must default to the structural `symbols` grounder \
         (spec 57): the vector engine `turbovec` and its `hybrid` composite were retired. Found: \
         grounder: {workflow_value:?}"`
- `tests/handbook_grounder_accuracy.rs:154-154` `"docs/handbook/authoring-loops.md claims its example reproduces the repo's own \
         .rigger/workflow.yml, so its `grounder:` value ({handbook_value:?}) must equal the \
         committed workflow's ({workflow_value:?}). An operator copies this block verbatim - a \
         stale `grounder: turbovec` here pastes the retired engine and hits the loud \
         retired-grounder migration error instead of a working run."`
- `tests/heartbeat_write_read_agree_periphery.rs:109-109` `".rigger"`
- `tests/heartbeat_write_read_agree_periphery.rs:119-119` `".rigger"`
- `tests/heartbeat_write_read_agree_periphery.rs:135-135` `".rigger"`
- `tests/heartbeat_write_read_agree_periphery.rs:152-152` `".rigger"`
- `tests/heartbeat_write_read_agree_periphery.rs:206-206` `".rigger"`
- `tests/heartbeat_write_read_agree_periphery.rs:432-432` `".rigger"`
- `tests/heartbeat_write_read_agree_periphery.rs:443-443` `".rigger"`
- `tests/heartbeat_write_read_agree_periphery.rs:528-528` `".rigger"`
- `tests/heartbeat_write_read_agree_periphery.rs:539-539` `".rigger"`
- `tests/heartbeat_write_read_agree_periphery.rs:611-611` `".rigger"`
- `tests/heartbeat_write_read_agree_periphery.rs:621-621` `".rigger"`
- `tests/integrate_conflict_merge_periphery.rs:390-390` `".rigger"`
- `tests/integrate_conflict_merge_periphery.rs:391-391` `"create .rigger/agents"`
- `tests/integrate_conflict_merge_periphery.rs:461-461` `"the project's own .rigger/workflow.yml must load through the real loader"`
- `tests/integrate_conflict_merge_periphery.rs:614-614` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:830-830` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:1068-1068` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:1304-1304` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:1545-1545` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:1755-1755` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:1947-1947` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:2183-2183` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:2318-2318` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:2484-2484` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:2693-2693` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:2922-2922` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:3032-3032` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:3185-3185` `"{repo_path}/.rigger-test-scratch"`
- `tests/migration_is_deliberate_periphery.rs:454-454` `".rigger"`
- `tests/migration_is_deliberate_periphery.rs:454-454` `"create .rigger"`
- `tests/migration_is_deliberate_periphery.rs:474-474` `".rigger"`
- `tests/migration_is_deliberate_periphery.rs:518-518` `".rigger"`
- `tests/migration_is_deliberate_periphery.rs:549-549` `".rigger"`
- `tests/migration_is_deliberate_periphery.rs:616-616` `".rigger"`
- `tests/mutation_scratch_reap_base_guard_periphery.rs:115-115` `"reclaim_unit_mutation_scratch's own doc comment promises every process rooted in a \
         matched registered mutation-scratch dir is reaped BEFORE the dir is removed - a \
         SIGTERM-ignoring process here must still be SIGKILLed. Round 1 broke this: \
         is_reapable_base's <repo>/.rigger/tmp containment requirement refused this real, \
         ALWAYS-outside-any-repo registered root (see this file's header doc comment and \
         decision u78c2-mutation-scratch-reap-now-refused-flagging-for-review), so \
         reap_processes_rooted_under silently no-opped. Round 2 (decision \
         u78c2r2-authorized-root-caller-supplied) fixed it by passing the registered \
         mutation-scratch root itself as authorized_root - a regression back to the round-1 \
         shape would fail this assertion again."`
- `tests/mutation_scratch_reap_base_guard_periphery.rs:149-149` `".rigger"`
- `tests/mutation_scratch_reap_base_guard_periphery.rs:176-176` `"control: when the registered scratch root legitimately lies under a real repo's \
         .rigger/tmp, reclaim_unit_mutation_scratch must still reap a live process rooted in \
         it - proving the sibling test's failure is specifically the base-guard's new scope, \
         not a defect in this file's own mechanics"`
- `tests/projections_stay_local.rs:109-109` `"the graph projection must be opened by the LOCAL sqlite Projector at .rigger/graph.db \
         (`Projector::open(&db_path(\"graph.db\") ...)`); the canonical local construction is gone"`
- `tests/projections_stay_local.rs:116-116` `"the progress projection must be opened by the LOCAL sqlite Store at .rigger/progress.db \
         (`Store::open(&db_path(\"progress.db\") ...)`); the canonical local construction is gone"`
- `tests/projections_stay_local.rs:145-145` `".rigger"`
- `tests/projections_stay_local.rs:204-204` `".rigger"`
- `tests/projections_stay_local.rs:243-243` `".rigger"`
- `tests/projections_stay_local.rs:246-246` `"graph.db must be created under the LOCAL .rigger/ even when the event store is the \
             server - projections are per-machine and stay local"`
- `tests/projections_stay_local.rs:256-256` `".rigger"`
- `tests/projections_stay_local.rs:257-257` `"a server-configured `graph build` must NOT create a local .rigger/events.db - the \
             event log is the server's; only the projection is local"`
- `tests/projections_stay_local.rs:310-310` `".rigger"`
- `tests/projections_stay_local.rs:313-313` `"progress.db must be created under the LOCAL .rigger/ even when the event store is the \
             server - the progress store is a local projection, not the shared log"`
- `tests/projections_stay_local.rs:336-336` `".rigger"`
- `tests/projections_stay_local.rs:337-337` `"a server-configured `rigger progress` must NOT create a local .rigger/events.db - the \
             run log is the server's; only the progress projection is local"`
- `tests/published_content_key_split_periphery.rs:376-376` `".rigger"`
- `tests/registry_periphery.rs:54-54` `r#"{
  "project": "acme",
  "root": "/home/dev/acme",
  "store": {
    "kind": "local",
    "path": "/home/dev/acme/.rigger/events.db"
  },
  "heartbeat_ms": 1000
}"#`
- `tests/registry_periphery.rs:93-93` `"/home/dev/acme/.rigger/events.db"`
- `tests/registry_periphery.rs:118-118` `"/home/dev/acme/.rigger/events.db"`
- `tests/registry_periphery.rs:184-184` `"/live/.rigger/events.db"`
- `tests/registry_periphery.rs:191-191` `"/dead/.rigger/events.db"`
- `tests/registry_periphery.rs:272-272` `"/home/dev/proj/.rigger/events.db"`
- `tests/registry_refresh_driver_courier_convergence_periphery.rs:61-61` `".rigger"`
- `tests/registry_refresh_driver_courier_convergence_periphery.rs:62-62` `"create .rigger/agents"`
- `tests/relocated_worktree_store_resolution_periphery.rs:58-58` `".rigger"`
- `tests/relocated_worktree_store_resolution_periphery.rs:59-59` `"create .rigger"`
- `tests/relocated_worktree_store_resolution_periphery.rs:68-68` `".rigger"`
- `tests/relocated_worktree_store_resolution_periphery.rs:86-86` `".rigger"`
- `tests/relocated_worktree_store_resolution_periphery.rs:168-168` `".rigger"`
- `tests/reset_build_cache_periphery.rs:46-46` `".rigger"`
- `tests/reset_build_cache_periphery.rs:55-55` `".rigger"`
- `tests/reset_build_cache_periphery.rs:465-465` `".rigger"`
- `tests/reset_build_cache_periphery.rs:542-542` `".rigger"`
- `tests/reset_derived_compaction.rs:45-45` `".rigger"`
- `tests/reset_derived_compaction.rs:45-45` `"create .rigger"`
- `tests/reset_derived_compaction.rs:62-62` `".rigger"`
- `tests/reset_derived_compaction.rs:76-76` `".rigger"`
- `tests/reset_derived_compaction_periphery.rs:606-606` `".rigger"`
- `tests/reset_derived_compaction_periphery.rs:606-606` `"create .rigger"`
- `tests/reset_derived_compaction_periphery.rs:611-611` `".rigger"`
- `tests/reset_derived_compaction_periphery.rs:627-627` `".rigger"`
- `tests/reset_derived_compaction_periphery.rs:1361-1361` `".rigger"`
- `tests/reset_derived_compaction_periphery.rs:1367-1367` `".rigger"`
- `tests/reset_derived_compaction_periphery.rs:3292-3292` `".rigger"`
- `tests/reset_derived_compaction_periphery.rs:3292-3292` `"create .rigger"`
- `tests/reset_derived_compaction_periphery.rs:3306-3306` `".rigger"`
- `tests/reset_derived_compaction_periphery.rs:3341-3341` `".rigger"`
- `tests/reset_derived_live_writer_guard_periphery.rs:47-47` `".rigger"`
- `tests/reset_derived_live_writer_guard_periphery.rs:48-48` `"create .rigger"`
- `tests/reset_derived_live_writer_guard_periphery.rs:76-76` `".rigger"`
- `tests/reset_derived_live_writer_guard_periphery.rs:92-92` `".rigger"`
- `tests/reset_derived_live_writer_guard_periphery.rs:128-128` `".rigger"`
- `tests/reset_derived_live_writer_guard_periphery.rs:137-137` `".rigger"`
- `tests/reset_derived_live_writer_guard_periphery.rs:303-303` `".rigger"`
- `tests/reset_derived_live_writer_guard_periphery.rs:462-462` `".rigger"`
- `tests/reset_derived_live_writer_guard_periphery.rs:507-507` `"{toplevel}/.rigger/events.db"`
- `tests/reset_derived_live_writer_guard_periphery.rs:541-541` `"/tmp/some-other-project/.rigger/events.db"`
- `tests/reset_derived_live_writer_guard_periphery.rs:586-586` `"/tmp/some-other-project/.rigger/events.db"`
- `tests/reset_menu.rs:57-57` `".rigger"`
- `tests/reset_menu.rs:71-71` `".rigger"`
- `tests/reset_menu.rs:75-75` `".rigger"`
- `tests/reset_menu.rs:82-82` `".rigger"`
- `tests/reset_menu_identity_migration_periphery.rs:52-52` `".rigger"`
- `tests/reset_menu_identity_migration_periphery.rs:66-66` `".rigger"`
- `tests/reset_menu_identity_migration_periphery.rs:70-70` `".rigger"`
- `tests/scratch_workdir_config.rs:32-32` `".rigger"`
- `tests/scratch_workdir_config.rs:33-33` `"create .rigger"`
- `tests/simplification_audit.rs:2728-2728` `".rigger-path string literals"`
- `tests/simplification_audit.rs:2871-2871` `".rigger"`
- `tests/simplification_audit.rs:2872-2872` `"one .rigger-relative path-composition helper"`
- `tests/simplification_audit.rs:4153-4153` `"| Conductor orchestration: gates, courier, step/run lifecycle | 19 | 8,257 | \
        the `courier_registry_refresh_{boundary,fence,periphery}` trio (3 files) \
        pair together in 6 clusters confined to just themselves (2-5 sites each; \
        excludes the whole-codebase Command::new/`.rigger`-path mandatory-sweep \
        clusters, section 2, that also happen to intersect them) |\n"`
- `tests/simplification_audit.rs:4467-4467` `"Largest risk-reduction first is read as six tiers, ranked by the KIND of risk \
        each entry retires, highest first:\n\n\
        1. Tier 1 - active correctness risk, PLUS item 0: a use case already depends on the \
        wrong concretion, or two independent implementations of one concern can already \
        drift apart silently (section 3's two boundary violations; the one already-drifted \
        `/proc`-reading pair section 2 and section 3 both name) - live gaps, not just size. \
        Item 0 (deleting the 23-function dead-code set, section 4.3) is placed here too, \
        first of all: not a live-gap risk itself, but the cheapest, zero-behavior-change, \
        guaranteed-safe move available (spec 87's own Done-when: \"Tier 1 item 0\"), and it \
        shrinks the exact three god files tiers 2 and 3 operate on before either touches \
        them - ordered before items 1 and 2 for that reason, per this section's own \
        largest-first-within-a-tier rule (item 0's line delta exceeds either boundary-fix \
        item's, section 4.3).\n\
        2. Tier 2 - god-file test-module extraction: each of the three god files' own \
        inline `#[cfg(test)] mod tests` is the majority of that file's bulk (56-70% by \
        boundary-line count, per a direct read of each file), and moving it is a pure \
        relocation with no production-behavior change - the single largest safe line-count \
        reduction in this plan, and the precondition that makes tier 3 tractable.\n\
        3. Tier 3 - god-file production splits: section 1's own proposed module tree \
        applied to the (now much smaller) remaining production surface of each god file. \
        Higher execution risk than tier 2 because it touches live orchestration and CLI \
        logic, so it is sequenced after tier 2 shrinks the target first.\n\
        4. Tier 4 - named production duplication sweeps: the mechanical mandatory sweeps \
        section 2 ran regardless of the Jaccard pass (`Command::new`, `.rigger`-path \
        literals, sqlite `Connection::open`, error-shaping helpers), each already a single \
        committed cluster with its own proposed home.\n\
        5. Tier 5 - test-suite consolidation: section 5's own catalogued test-only \
        duplication. No production-correctness exposure at all (worst case a test \
        regresses, never the product), so it is ordered ahead only of tier 6 despite \
        touching the largest raw line count anywhere in this plan.\n\
        6. Tier 6 - remaining catalog sweep: the 327 src-touching clusters section 2 found \
        but tiers 1 and 4 did not individually name. Unlike every other tier, none of these \
        327 have been read and risk-assessed one at a time the way tiers 1-4's named \
        clusters have - they are consumed straight from the catalog - so this tier carries \
        production-correctness exposure tiers 2, 3 and 5 do not, and is ordered last: the \
        follow-up spec must triage each cluster's own production-or-test status before \
        merging it, not assume tier 5's blanket test-only treatment applies here too.\n\n\
        Within a tier, entries are ordered largest-first by the site or line count each \
        retires - the same rule the tiers themselves follow, applied one level down.\n\n"`
- `tests/simplification_audit.rs:4774-4774` `"#### 10. Consolidate the 655 `.rigger`-path string-literal sites (`dup-0055`) - the \
        single largest cluster in the entire catalog by site count\n\n"`
- `tests/simplification_audit.rs:4778-4778` `"- Scope: one `.rigger`-relative path-composition helper (the cluster's own \
        `proposed_home`) every one of the 662 sites routes through instead of building its \
        own literal.\n\
        - Files: spans dozens of files including `src/conductor.rs`, `src/config.rs`, \
        `src/dash.rs`, `src/docs.rs`, `src/gate.rs`, `src/grounder/mod.rs`, \
        `src/grounder/symbols/store.rs`, `src/ingest.rs`, `src/main.rs`, `src/reap.rs`, \
        `src/registry.rs`, `src/worktree.rs` plus many `tests/` files - the full site list \
        is in the committed `docs/audit/duplication-catalog.json` under `dup-0055` for the \
        follow-up spec to consume directly, not re-enumerated here.\n\
        - Expected line delta: negative - 662 literal compositions collapse toward one \
        helper's call sites; the helper itself is small.\n\
        - Risk: medium - the largest surface-area sweep in this plan by site count, even \
        though each individual site is trivial; needs a mechanical rewrite pass plus a \
        full-suite green run, not hand-editing 662 sites.\n\
        - Unblocks: the biggest single site-count reduction available anywhere in the \
        duplication catalog.\n\n"`
- `tests/simplification_audit.rs:7908-7908` `"fn a() {\n    let _ = \".rigger/tmp\";\n}\n"`
- `tests/simplification_audit.rs:7911-7911` `".rigger"`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:57-57` `".rigger"`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:76-76` `".rigger"`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:98-98` `".rigger"`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:305-305` `".rigger"`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:316-316` `".rigger"`
- `tests/step_attention_periphery.rs:161-161` `".rigger"`
- `tests/step_attention_periphery.rs:183-183` `".rigger"`
- `tests/step_attention_periphery.rs:222-222` `".rigger"`
- `tests/step_attention_periphery.rs:493-493` `".rigger"`
- `tests/step_root_resolution_periphery.rs:168-168` `".rigger"`
- `tests/step_sheds_the_freshen.rs:67-67` `".rigger/grounding"`
- `tests/step_sheds_the_freshen.rs:216-216` `".rigger/symbols/index.json"`
- `tests/step_sheds_the_freshen.rs:217-217` `"the surviving persisted index is the SYMBOL index under .rigger/symbols/; got {}"`
- `tests/step_sheds_the_freshen.rs:282-282` `"constructing the default grounder builds and persists the SYMBOL index under \
         .rigger/symbols/ - the freshen's real target"`
- `tests/store_config.rs:34-34` `".rigger"`
- `tests/store_config.rs:35-35` `"create .rigger"`
- `tests/store_content_identity_periphery.rs:1346-1346` `".rigger"`
- `tests/store_content_identity_periphery.rs:1347-1347` `"create .rigger"`
- `tests/store_content_identity_periphery.rs:1398-1398` `".rigger"`
- `tests/store_flag_precedence.rs:76-76` `".rigger"`
- `tests/store_flag_precedence.rs:77-77` `"create .rigger/agents"`
- `tests/store_flag_precedence.rs:95-95` `".rigger"`
- `tests/store_flag_precedence.rs:143-143` `"{why}: a server selection must NOT fabricate a local .rigger/events.db"`
- `tests/store_precedence.rs:53-53` `".rigger"`
- `tests/store_precedence.rs:53-53` `"create .rigger"`
- `tests/store_precedence.rs:60-60` `".rigger"`
- `tests/store_precedence.rs:65-65` `".rigger"`
- `tests/store_precedence.rs:73-73` `".rigger"`
- `tests/store_precedence.rs:79-79` `".rigger"`
- `tests/store_precedence.rs:137-137` `"{why}: a server selection must NOT fabricate a local .rigger/events.db"`
- `tests/store_precedence.rs:197-197` `".rigger/store.conn beats the committed store: config"`
- `tests/store_precedence.rs:250-250` `"a loud read failure must leave no fabricated local .rigger/events.db behind"`
- `tests/store_precedence.rs:310-310` `"a loud config-validation failure must leave no fabricated local .rigger/events.db behind"`
- `tests/store_precedence.rs:353-353` `"a loud missing-connection failure must leave no fabricated local .rigger/events.db behind"`
- `tests/store_resolution.rs:160-160` `".rigger"`
- `tests/store_resolution.rs:241-241` `".rigger"`
- `tests/store_resolution.rs:273-273` `".rigger"`
- `tests/store_resolution.rs:274-274` `"a server-configured courier must NOT create a local .rigger/events.db - that is the \
             state-fracture this criterion closes"`
- `tests/store_resolution.rs:342-342` `".rigger"`
- `tests/store_resolution.rs:378-378` `".rigger"`
- `tests/store_resolution.rs:382-382` `".rigger"`
- `tests/store_resolution_cli.rs:60-60` `".rigger"`
- `tests/store_resolution_cli.rs:60-60` `"create .rigger"`
- `tests/store_resolution_cli.rs:67-67` `".rigger"`
- `tests/store_resolution_cli.rs:124-124` `"a server-configured courier must NOT create a local .rigger/events.db - that is the \
         state-fracture this criterion closes, and it must hold even when the server is down"`
- `tests/store_secrets.rs:56-56` `".rigger"`
- `tests/store_secrets.rs:56-56` `"create .rigger"`
- `tests/store_secrets.rs:63-63` `".rigger"`
- `tests/store_secrets.rs:71-71` `".rigger"`
- `tests/store_secrets.rs:142-142` `"{why}: a server-configured courier must NOT create a local .rigger/events.db: {stderr}"`
- `tests/store_secrets.rs:166-166` `".rigger/store.conn secret-file channel"`
- `tests/store_secrets.rs:182-182` `".rigger"`
- `tests/validate_advisories.rs:53-53` `".rigger"`
- `tests/validate_advisories.rs:53-53` `"create .rigger"`
- `tests/validate_advisories.rs:68-68` `".rigger"`
- `tests/validate_advisories.rs:82-82` `".rigger"`
- `tests/validate_advisories.rs:244-244` `".rigger"`
- `tests/validate_behind_the_tree_periphery.rs:72-72` `".rigger"`
- `tests/validate_behind_the_tree_periphery.rs:72-72` `"create .rigger"`
- `tests/validate_footprint_default_scratch_root_periphery.rs:40-40` `".rigger"`
- `tests/validate_footprint_default_scratch_root_periphery.rs:41-41` `".rigger"`
- `tests/validate_footprint_default_scratch_root_periphery.rs:101-101` `".rigger"`
- `tests/watchdog_cli_periphery.rs:57-57` `".rigger"`
- `tests/watchdog_cli_periphery.rs:77-77` `".rigger"`
- `tests/watchdog_cli_periphery.rs:94-94` `".rigger"`
- `tests/watchdog_cli_periphery.rs:225-225` `".rigger"`
- `tests/watchdog_cli_periphery.rs:263-263` `".rigger"`
- `tests/watchdog_cli_periphery.rs:466-466` `".rigger"`
- `tests/workflow_driver_resolved_model_periphery.rs:86-86` `".rigger"`
- `tests/workflow_driver_resolved_model_periphery.rs:278-278` `".rigger"`
- `tests/workflow_driver_resolved_model_periphery.rs:434-434` `".rigger"`
- `tests/worktree_liveness_fence_periphery.rs:175-175` `".rigger"`
- `tests/worktree_liveness_fence_periphery.rs:199-199` `".rigger"`
- `tests/worktree_liveness_fence_periphery.rs:235-235` `".rigger"`
- `tests/worktree_remove_relocated_scratch_base_guard_periphery.rs:139-139` `"Worktree::remove's own doc comment promises every process rooted in the worktree is \
         reaped BEFORE the dir is removed, unconditionally - a SIGTERM-ignoring process here \
         must still be SIGKILLed, exactly as the DEFAULT (unrelocated) shape already is (see \
         worktree::tests::remove_reaps_a_process_rooted_inside_the_worktree_and_spares_one_outside, \
         independently re-run and confirmed green). Round 1 broke this for a scratch root \
         relocated via defaults.workdir/RIGGER_TMPDIR to a location outside the project repo \
         (a real, tested configuration surface - see tests/scratch_workdir_config.rs): \
         is_reapable_base's <repo>/.rigger/tmp containment requirement could never accept such \
         a dir, so reap_processes_rooted_under silently no-opped (adj-u78c2-verdict-reject- \
         reap-authority-conflict). Round 2 (decision u78c2r2-worktree-remove-identity-not-tree) \
         fixed it by authorizing the reap via GIT IDENTITY (is self.dir a real, currently \
         checked-out worktree of self.branch?) instead of path containment, calling \
         reap_authorized directly - a regression back to the round-1 shape would fail this \
         assertion again."`

#### `dup-0061` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:17943-17963` `render_capped_findings`
- `src/conductor.rs:18185-18204` `render_capped_lessons`
- `src/conductor.rs:20542-20564` `render_capped_lessons_scoped`

#### `dup-0062` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:17966-18033` `findings_prompt_injection_is_capped_under_budget_with_elision_note`
- `src/conductor.rs:20430-20496` `lessons_prompt_injection_is_capped_under_budget_with_elision_note`

#### `dup-0063` (exact, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:19793-19800` `read_stream`
- `src/conductor.rs:26866-26873` `read_stream`

#### `dup-0064` (exact, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:19801-19808` `read_all`
- `src/conductor.rs:26874-26881` `read_all`

#### `dup-0065` (exact, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:19809-19815` `subscribe_all`
- `src/conductor.rs:26882-26888` `subscribe_all`

#### `dup-0066` (exact, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:19816-19822` `subscribe_stream`
- `src/conductor.rs:26889-26895` `subscribe_stream`

#### `dup-0067` (exact, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:19845-19847` `resolve`
- `src/conductor.rs:35046-35048` `resolve`

#### `dup-0068` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:20234-20274` `a_subgraph_with_no_design_intent_renders_no_design_intent_header`
- `src/conductor.rs:20397-20427` `a_subgraph_with_no_code_definitions_renders_no_code_neighborhood_header`

#### `dup-0069` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:20730-20747` `commit_on_unit_branch`
- `src/conductor.rs:20950-20966` `commit_on_named_branch`

#### `dup-0070` (near, 4 sites)

Proposed home: `conductor::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:20750-20845` `resume_reuses_a_units_branch_instead_of_reimplementing`
- `src/conductor.rs:22977-23062` `resume_integrates_an_already_approved_unit_without_re_reviewing`
- `src/conductor.rs:23065-23176` `a_failed_unit_is_not_terminal_and_resumes`
- `src/conductor.rs:31150-31253` `a_resumed_reviewed_unit_restores_a_worktree_the_exhaustive_gate_deleted`

#### `dup-0071` (near, 4 sites)

Proposed home: `conductor::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:20848-20942` `branch_gc_reclaims_integrated_units_and_retains_escalated_ones_on_resume`
- `src/conductor.rs:20981-21037` `branch_gc_falls_back_to_the_derived_branch_when_unitstarted_recorded_no_branch`
- `src/conductor.rs:21259-21351` `branch_gc_reclaims_every_integrated_unit_in_one_resume_not_just_the_first`
- `src/conductor.rs:21354-21435` `branch_gc_fences_reclaim_behind_an_in_flight_straggler_spawn_and_reclaims_once_it_answers`

#### `dup-0072` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:21438-21505` `gc_integrated_branches_logged_prints_kept_evidence_for_an_in_flight_straggler_spawn`
- `src/conductor.rs:21610-21693` `gc_integrated_branches_logged_stays_silent_for_an_already_gone_worktree_but_still_reclaims_an_orphaned_branch`

#### `dup-0073` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:21508-21607` `gc_integrated_branches_logged_prints_removing_evidence_for_a_terminal_spawns_decision`
- `src/conductor.rs:21696-21799` `gc_integrated_branches_logged_does_not_repeat_removing_evidence_once_the_real_removal_already_happened`

#### `dup-0074` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:22555-22600` `a_gating_spawn_that_returns_a_reject_verdict_line_is_a_normal_reject_even_if_it_emitted_approve`
- `src/conductor.rs:22603-22647` `a_gating_spawn_with_no_verdict_line_and_no_emitted_approve_is_an_ordinary_reject`

#### `dup-0075` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:23501-23543` `scope_creep_refuses_a_criterionless_proposed_unit`
- `src/conductor.rs:29086-29127` `planner_covering_every_criterion_passes`

#### `dup-0076` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:23582-23648` `adversary_runs_between_the_lenses_and_the_adjudicator`
- `src/conductor.rs:23704-23797` `unit_reviews_itself_within_its_own_lifecycle`

#### `dup-0077` (near, 4 sites)

Proposed home: `conductor::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:24080-24140` `a_low_risk_unit_skips_the_adversary_and_extra_lens`
- `src/conductor.rs:24143-24213` `a_high_risk_unit_runs_the_full_panel_and_logs_the_routing`
- `src/conductor.rs:24234-24312` `a_zero_grounding_unit_fails_safe_to_the_full_panel_and_logs_it`
- `src/conductor.rs:24427-24491` `a_stage_level_tiers_policy_routes_the_unit_by_risk`

#### `dup-0078` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:26063-26125` `planner_proposed_unit_inherits_the_default_review_panel`
- `src/conductor.rs:26128-26223` `a_producer_stage_skips_the_three_tier_review_and_unblocks_its_dependents`

#### `dup-0079` (semantic, 8 sites)

Proposed home: `one error-shaping helper module`

mandatory sweep: error-shaping helper functions - 8 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/conductor.rs:26899-26957` `integrate_plan_commits_wraps_any_hard_error_with_the_plan_landing_marker`
- `src/grounder/mod.rs:207-213` `retired_grounder_error`
- `src/worktree.rs:3801-3855` `revert_on_base_aborts_and_errors_on_a_conflicting_revert`
- `tests/adoption_keys_on_criterion_periphery.rs:2425-2597` `a_quarantine_record_whose_ref_was_since_deleted_hard_errors_instead_of_silently_starting_fresh`
- `tests/batched_fold_cadence.rs:184-262` `append_and_fold_batch_is_best_effort_on_fold_error_and_a_no_op_on_an_empty_batch`
- `tests/canary_item_sharding_jobs_cap_periphery.rs:352-440` `run_canary_runs_every_item_to_completion_even_when_one_items_spawn_errors`
- `tests/grounder_name_contract.rs:40-171` `public_name_contract_predicate_error_and_resolver_agree`
- `tests/integrate_conflict_merge_periphery.rs:1736-1844` `a_non_content_merge_failure_surfaces_as_a_run_error_leaving_branches_intact`

#### `dup-0080` (near, 5 sites)

Proposed home: `conductor::support (consolidate these 5 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:27045-27117` `per_unit_adjudicator_reject_blocks_integration_and_escalates`
- `src/conductor.rs:27645-27718` `approval_on_the_final_permitted_attempt_integrates_a_per_unit_stage`
- `src/conductor.rs:27814-27880` `approval_on_the_final_permitted_attempt_integrates_a_standalone_review_stage`
- `src/conductor.rs:29768-29815` `on_pass_none_runs_gates_but_does_not_integrate`
- `src/conductor.rs:33239-33284` `unparseable_adjudicator_output_blocks_integration`

#### `dup-0081` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:27216-27313` `a_higher_max_retries_gives_more_attempts_before_escalation`
- `src/conductor.rs:27230-27276` `escalation_run`

#### `dup-0082` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:27883-27917` `mid_spawn_crash_escalates_without_aborting_the_run`
- `src/conductor.rs:27920-27971` `a_newly_escalated_unit_stamps_an_attention_entry`

#### `dup-0083` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:27974-28024` `a_budget_halt_stamps_an_attention_entry`
- `src/conductor.rs:28787-28825` `a_budget_halt_surfaces_its_reason_on_the_run_state`

#### `dup-0084` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:28119-28223` `a_delayed_budget_halt_after_a_dependency_unlocks_still_stamps`
- `src/conductor.rs:28300-28381` `an_escalation_does_not_restamp_attention_on_a_resumed_process`
- `src/conductor.rs:28420-28544` `a_second_failure_recurs_and_a_third_also_stalls_the_frontier`

#### `dup-0085` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:28687-28738` `budget_breaker_stops_the_run_after_the_first_wave`
- `src/conductor.rs:28741-28784` `budget_exhaustion_aborts_the_task`
- `src/conductor.rs:29204-29263` `manual_stage_pauses_while_an_auto_stage_integrates`

#### `dup-0086` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:28863-28931` `the_spawn_budget_folds_from_recorded_spawn_requests_across_steps`
- `src/conductor.rs:28934-28975` `the_pre_wave_breaker_trips_only_on_a_new_over_budget_spawn`

#### `dup-0087` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:29266-29309` `isolation_none_agent_gets_no_worktree_even_with_a_repo`
- `src/conductor.rs:29312-29345` `spawn_opts_isolation_is_set_for_a_worktree_agent`
- `src/conductor.rs:29348-29387` `a_spawned_implementers_title_is_the_unit_criterion`

#### `dup-0088` (near, 4 sites)

Proposed home: `conductor::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:29394-29442` `the_adversarys_spawn_is_stamped_with_the_units_lens_roster`
- `src/conductor.rs:29448-29497` `the_adjudicators_spawn_is_stamped_with_lenses_plus_adversary`
- `src/conductor.rs:29502-29549` `a_panel_with_no_adversary_never_fabricates_one_in_the_adjudicators_roster`
- `src/conductor.rs:29635-29692` `the_fan_out_review_loops_adversary_and_adjudicator_spawns_are_stamped_with_the_routed_roster`

#### `dup-0089` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:29845-29932` `two_units_gate_environments_never_share_a_target_dir`
- `src/conductor.rs:29935-30001` `two_units_gate_environments_never_share_a_mutants_root`

#### `dup-0090` (exact, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:30069-30071` `envs`
- `src/conductor.rs:34199-34201` `build_envs`

#### `dup-0091` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:30472-30578` `a_parked_review_spawn_keeps_the_unit_worktree_registered_and_its_cache`
- `src/conductor.rs:30581-30673` `review_unit_restores_a_worktree_a_gate_deleted_out_of_band`
- `src/conductor.rs:30676-30778` `a_second_step_restores_a_still_parked_units_worktree_deleted_out_of_band`

#### `dup-0092` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:31080-31147` `failed_worktree_sha_is_stamped_after_the_adjudicators_own_deletion_is_restored`
- `src/conductor.rs:31866-31937` `speculation_reject_worktree_sha_is_stamped_after_the_adjudicators_own_deletion_is_restored`

#### `dup-0093` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:32200-32243` `agent_stage_runs_the_per_unit_lifecycle_not_the_fan_out_path`
- `src/conductor.rs:32246-32287` `standalone_review_stage_still_takes_the_fan_out_path`

#### `dup-0094` (near, 6 sites)

Proposed home: `conductor::support (consolidate these 6 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:32405-32458` `a_parked_lens_keeps_the_standalone_review_stages_worktree`
- `src/conductor.rs:32461-32557` `a_parked_lens_keeps_the_review_worktree_even_beside_a_lower_indexed_sibling_crash`
- `src/conductor.rs:32560-32653` `a_parked_lens_keeps_the_review_worktree_beside_a_sibling_degenerate_halt`
- `src/conductor.rs:32656-32741` `the_swap_to_front_prioritizes_a_genuine_error_at_a_non_zero_chunk_index`
- `src/conductor.rs:32744-32831` `a_budget_refused_lens_beside_a_genuinely_crashing_sibling_in_one_chunk`
- `src/conductor.rs:32834-32901` `a_budget_refused_standalone_review_spawn_keeps_its_worktree`

#### `dup-0095` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:33351-33434` `a_flaky_gate_rerun_is_a_pass_with_warning_that_never_demotes`
- `src/conductor.rs:33437-33507` `a_product_gate_failure_is_not_rerun_and_demotes_as_before`

#### `dup-0096` (exact, 2 sites)

Proposed home: `conductor::recording_runner`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:34175-34180` `materializing`
- `src/conductor.rs:34184-34189` `deleting_worktree`

#### `dup-0097` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/conductor.rs, src/spawn.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:34294-34300` `attempt_of`
- `src/spawn.rs:194-200` `attempt_of`

#### `dup-0098` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:34413-34479` `a_matching_input_digest_answers_a_gate_as_a_logged_cache_hit_citing_the_prior_green`
- `src/conductor.rs:34527-34583` `a_red_verdict_is_never_cache_answered_and_the_gate_re_runs`

#### `dup-0099` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:34719-34792` `a_structural_grounder_records_the_audit_and_routes_full_on_a_beyond_cap_high_risk_file`
- `src/conductor.rs:35332-35396` `speculation_over_a_structural_grounder_records_the_audit_and_routes_full`

#### `dup-0100` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:35648-35689` `spawn`
- `src/conductor.rs:36698-36735` `spawn`
- `src/conductor.rs:36833-36878` `spawn`

#### `dup-0101` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:36426-36477` `commits_to_compensate_reverts_every_sha_a_multi_commit_landing_recorded`
- `src/conductor.rs:36542-36593` `commits_to_compensate_excludes_the_review_only_marker_even_alongside_a_real_sha`

#### `dup-0102` (exact, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:36739-36822` `a_compensated_unit_re_gates_its_re_implemented_tree_not_the_condemned_verdict`
- `src/conductor.rs:36882-36980` `a_compensated_unit_that_remediated_before_integrating_re_gates_at_a_fresh_key`

#### `dup-0103` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:37768-37854` `integrate_conflict_re_parks_the_implementer_with_no_attempt_charged_and_both_units_land`
- `src/conductor.rs:37857-37937` `integrate_conflict_confined_to_a_regenerable_path_resolves_with_no_spawn`

#### `dup-0104` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:38285-38353` `a_failing_deferred_gate_is_surfaced_and_the_run_is_not_done`
- `src/conductor.rs:38391-38462` `a_default_infra_fault_at_a_deferred_gate_does_not_demote`

#### `dup-0105` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:38465-38569` `a_replayed_step_re_runs_no_recorded_gate_and_appends_no_duplicate_events`
- `src/conductor.rs:38572-38657` `a_re_step_replays_a_recorded_deferred_gate_without_re_running_it`

#### `dup-0106` (near, 6 sites)

Proposed home: `a new shared module (sites span 6 files: src/conductor.rs, tests/gate_store_fence_periphery.rs, tests/integrate_conflict_merge_periphery.rs, tests/scratch_workdir_isolation_leak_guard_periphery.rs, tests/spawn_target_dir_periphery.rs, tests/unified_traversal_grounding.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:39318-39335` `init_repo`
- `tests/gate_store_fence_periphery.rs:491-507` `init_repo_with_head`
- `tests/integrate_conflict_merge_periphery.rs:260-277` `init_repo`
- `tests/scratch_workdir_isolation_leak_guard_periphery.rs:63-80` `init_repo`
- `tests/spawn_target_dir_periphery.rs:55-71` `init_repo_with_head`
- `tests/unified_traversal_grounding.rs:575-592` `init_seam_repo`

#### `dup-0107` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:39985-40060` `an_empty_accounting_sdet_author_result_advances_the_build_to_commit_gates_and_integrate`
- `src/conductor.rs:40063-40127` `an_absent_sdet_author_is_a_clean_no_op_and_the_build_still_integrates`
- `src/conductor.rs:40130-40205` `a_crashed_sdet_author_spawn_does_not_block_the_build_the_lifecycle_proceeds`

#### `dup-0108` (exact, 2 sites)

Proposed home: `conductor::critique_driver`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:40398-40403` `rejecting`
- `src/conductor.rs:40406-40411` `always_rejecting`

#### `dup-0109` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:40605-40642` `a_rule_7_or_8_defect_rejects_then_releases_on_the_revision`
- `src/conductor.rs:40645-40696` `a_clean_decomposition_approves_and_releases_the_fan_out`

#### `dup-0110` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:41157-41255` `a_resumed_step_holds_the_fan_out_while_the_plan_critique_gate_is_escalated`
- `src/conductor.rs:41258-41314` `an_approved_gate_releases_planner_proposed_units_not_only_baselines`
- `src/conductor.rs:41437-41508` `a_resumed_step_holds_the_fan_out_while_the_plan_critique_gate_is_mid_review`

#### `dup-0111` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/config.rs, src/failure.rs, src/main.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config.rs:221-223` `is_empty`
- `src/failure.rs:168-170` `is_any`
- `src/main.rs:10140-10145` `is_empty`

#### `dup-0112` (near, 2 sites)

Proposed home: `config::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config.rs:949-969` `read_store_config`
- `src/config.rs:1006-1021` `read_scratch_defaults`

#### `dup-0113` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/config.rs, tests/no_os_kill_audit.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config.rs:1641-1643` `is_word_byte`
- `tests/no_os_kill_audit.rs:52-54` `is_word_char`
- `tests/simplification_audit.rs:198-200` `is_ident_char`

#### `dup-0114` (near, 3 sites)

Proposed home: `config::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config.rs:1802-1841` `verdict_line_lint_passes_an_unrelated_same_sentence_emit_before_a_verdict_output_clause`
- `src/config.rs:1853-1885` `verdict_line_lint_passes_a_determiner_verdict_when_a_payload_noun_sits_in_the_span`
- `src/config.rs:1898-1938` `verdict_line_lint_passes_a_determiner_verdict_when_an_unrelated_emit_example_brace_precedes_it`

#### `dup-0115` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/config.rs, src/main.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config.rs:1974-2007` `literal_is_emit_payload_binds_only_an_abutting_payload_or_emit_word`
- `src/main.rs:16863-16869` `is_uuid8_accepts_exactly_eight_hex_digits`
- `tests/simplification_audit.rs:7915-7923` `looks_error_shaping_matches_error_and_underscore_bounded_err_but_not_an_incidental_substring`

#### `dup-0116` (near, 2 sites)

Proposed home: `config::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config.rs:2161-2168` `parses_agent_frontmatter_and_body`
- `src/config.rs:2176-2185` `model_ladder_parses_from_frontmatter`

#### `dup-0117` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/config.rs, src/main.rs, src/spec.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config.rs:2171-2173` `rejects_missing_frontmatter`
- `src/main.rs:16140-16142` `dirty_tracked_paths_on_a_clean_tree_is_empty`
- `src/spec.rs:895-897` `empty_when_no_criteria`

#### `dup-0118` (near, 6 sites)

Proposed home: `config::support (consolidate these 6 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config.rs:2266-2275` `defaults_max_wall_clock_parses_and_is_zero_when_absent`
- `src/config.rs:2785-2801` `max_retries_parses_from_defaults_and_defaults_to_zero_when_absent`
- `src/config.rs:3050-3063` `build_config_parses_wrapper_and_cache_dir_and_defaults_when_omitted`
- `src/config.rs:3072-3087` `build_config_parses_jobs_and_defaults_to_zero_when_omitted`
- `src/config.rs:3096-3111` `build_config_parses_max_concurrent_defaulting_to_four_when_omitted`
- `src/config.rs:3159-3172` `build_config_parses_mutation_and_defaults_to_empty_when_omitted`

#### `dup-0119` (near, 2 sites)

Proposed home: `config::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config.rs:2521-2539` `validate_catches_unknown_ref`
- `src/config.rs:3364-3392` `validate_catches_cycle`

#### `dup-0120` (near, 3 sites)

Proposed home: `config::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config.rs:2631-2654` `validate_catches_an_unknown_light_panel_agent`
- `src/config.rs:2657-2681` `validate_rejects_a_light_panel_with_no_adjudicator`
- `src/config.rs:2712-2739` `validate_rejects_a_tiers_policy_on_a_full_panel_with_no_adjudicator`

#### `dup-0121` (exact, 3 sites)

Proposed home: `a new shared module (sites span 2 files: src/contextgraph/mod.rs, src/dash.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/mod.rs:522-524` `is_false`
- `src/dash.rs:1151-1153` `is_not_back`
- `src/dash.rs:1158-1160` `is_not_shared`

#### `dup-0122` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/contextgraph/mod.rs, tests/calls_down_execution_path_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/mod.rs:845-847` `apply`
- `tests/calls_down_execution_path_periphery.rs:139-141` `apply`

#### `dup-0123` (exact, 5 sites)

Proposed home: `a new shared module (sites span 4 files: src/contextgraph/mod.rs, tests/batched_fold_cadence.rs, tests/calls_down_execution_path_periphery.rs, tests/store_content_identity_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/mod.rs:848-850` `subgraph`
- `tests/batched_fold_cadence.rs:65-67` `subgraph`
- `tests/batched_fold_cadence.rs:91-93` `subgraph`
- `tests/calls_down_execution_path_periphery.rs:142-144` `subgraph`
- `tests/store_content_identity_periphery.rs:104-106` `subgraph`

#### `dup-0124` (exact, 5 sites)

Proposed home: `a new shared module (sites span 4 files: src/contextgraph/mod.rs, tests/batched_fold_cadence.rs, tests/calls_down_execution_path_periphery.rs, tests/store_content_identity_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/mod.rs:851-853` `resolve`
- `tests/batched_fold_cadence.rs:68-70` `resolve`
- `tests/batched_fold_cadence.rs:94-96` `resolve`
- `tests/calls_down_execution_path_periphery.rs:145-147` `resolve`
- `tests/store_content_identity_periphery.rs:107-109` `resolve`

#### `dup-0125` (semantic, 46 sites)

Proposed home: `one sqlite-connection-opening adapter function every caller is injected with`

mandatory sweep: sqlite Connection::open call sites - 46 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/contextgraph/sqlite.rs:93-93` `Connection::open`
- `src/contextgraph/sqlite.rs:6727-6727` `Connection::open`
- `src/contextgraph/sqlite.rs:7537-7537` `Connection::open`
- `src/eventstore/sqlite.rs:159-159` `Connection::open`
- `src/eventstore/sqlite.rs:2792-2792` `Connection::open`
- `src/eventstore/sqlite.rs:3484-3484` `Connection::open`
- `src/eventstore/sqlite.rs:3576-3576` `Connection::open`
- `src/eventstore/sqlite.rs:3833-3833` `Connection::open_with_flags`
- `src/eventstore/sqlite.rs:3851-3851` `Connection::open`
- `src/eventstore/sqlite.rs:3865-3865` `Connection::open`
- `src/main.rs:25373-25373` `Connection::open`
- `tests/cli.rs:901-901` `Connection::open`
- `tests/cli.rs:970-970` `Connection::open`
- `tests/cli.rs:1066-1066` `Connection::open`
- `tests/cli.rs:1159-1159` `Connection::open`
- `tests/cli.rs:1214-1214` `Connection::open`
- `tests/cli.rs:10688-10688` `Connection::open`
- `tests/cli.rs:16822-16822` `Connection::open`
- `tests/graph_additive_indexes_persist.rs:69-69` `Connection::open`
- `tests/graph_additive_indexes_persist.rs:165-165` `Connection::open`
- `tests/graph_additive_indexes_persist.rs:189-189` `Connection::open`
- `tests/heartbeat_write_read_agree_periphery.rs:215-215` `Connection::open`
- `tests/reset_derived_compaction.rs:132-132` `Connection::open`
- `tests/reset_derived_compaction.rs:306-306` `Connection::open`
- `tests/reset_derived_compaction.rs:571-571` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:117-117` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:568-568` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:1374-1374` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:2859-2859` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:3064-3064` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:3940-3940` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:4031-4031` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:4040-4040` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:4648-4648` `Connection::open`
- `tests/reset_derived_live_writer_guard_periphery.rs:128-128` `Connection::open`
- `tests/reset_menu.rs:175-175` `Connection::open`
- `tests/reset_menu.rs:179-179` `Connection::open`
- `tests/reset_menu_identity_migration_periphery.rs:201-201` `Connection::open`
- `tests/store_append_order_periphery.rs:58-58` `Connection::open`
- `tests/store_content_identity_periphery.rs:905-905` `Connection::open`
- `tests/store_content_identity_periphery.rs:1754-1754` `Connection::open`
- `tests/store_content_identity_periphery.rs:1780-1780` `Connection::open`
- `tests/store_content_identity_periphery.rs:1789-1789` `Connection::open`
- `tests/store_content_identity_periphery.rs:2089-2089` `Connection::open`
- `tests/watchdog_cli_periphery.rs:236-236` `Connection::open`
- `tests/watchdog_cli_periphery.rs:272-272` `Connection::open`

#### `dup-0126` (near, 2 sites)

Proposed home: `sqlite::projector`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:502-617` `calls_down`
- `src/contextgraph/sqlite.rs:653-762` `calls_up`

#### `dup-0127` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/contextgraph/sqlite.rs, src/eventstore/kurrentdb.rs, src/eventstore/sqlite.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:879-883` `to_nanos`
- `src/eventstore/kurrentdb.rs:115-119` `to_nanos`
- `src/eventstore/sqlite.rs:1468-1472` `to_nanos`

#### `dup-0128` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/contextgraph/sqlite.rs, src/dash.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:1893-1898` `name_suffix`
- `src/dash.rs:1696-1701` `name_suffix`

#### `dup-0129` (exact, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:1928-1934` `tier_rank`
- `src/contextgraph/sqlite.rs:1940-1946` `tier_floor_rank`

#### `dup-0130` (near, 3 sites)

Proposed home: `sqlite::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:2013-2038` `calls_out`
- `src/contextgraph/sqlite.rs:2071-2096` `callers_direct`
- `src/contextgraph/sqlite.rs:2107-2135` `callers_via_bare`

#### `dup-0131` (near, 5 sites)

Proposed home: `a new shared module (sites span 4 files: src/contextgraph/sqlite.rs, src/dash.rs, tests/calls_down_execution_path_periphery.rs, tests/graph_fold_dedup_live_only_scoping.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:2774-2788` `apply_decision`
- `src/contextgraph/sqlite.rs:4469-4476` `apply_batch_ref_caller`
- `src/dash.rs:9906-9913` `apply_call`
- `tests/calls_down_execution_path_periphery.rs:80-87` `apply_call`
- `tests/graph_fold_dedup_live_only_scoping.rs:38-45` `apply_decision`

#### `dup-0132` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:2863-2876` `subgraph_finds_the_governing_decision`
- `src/contextgraph/sqlite.rs:7894-7914` `recording_proof_never_wipes_the_entitys_own_name_kind_and_line_attrs`

#### `dup-0133` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/contextgraph/sqlite.rs, tests/graph_fold_dedup_live_edge.rs, tests/graph_rebuild_collapses_dupes.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:3007-3015` `apply_governs_at`
- `tests/graph_fold_dedup_live_edge.rs:38-46` `apply_governs`
- `tests/graph_rebuild_collapses_dupes.rs:40-48` `apply_governs`

#### `dup-0134` (near, 9 sites)

Proposed home: `a new shared module (sites span 6 files: src/contextgraph/sqlite.rs, src/dash.rs, tests/calls_down_execution_path_periphery.rs, tests/dash_calls_route_periphery.rs, tests/graph_superseded_prune.rs, tests/reset_menu_previews_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:3298-3316` `apply_code_entity`
- `src/contextgraph/sqlite.rs:3372-3391` `apply_community`
- `src/contextgraph/sqlite.rs:4443-4454` `apply_batch_def`
- `src/contextgraph/sqlite.rs:4737-4757` `apply_batch_def_at`
- `src/dash.rs:9894-9905` `apply_def`
- `tests/calls_down_execution_path_periphery.rs:63-74` `apply_def`
- `tests/dash_calls_route_periphery.rs:741-751` `apply_def`
- `tests/graph_superseded_prune.rs:36-48` `apply_def`
- `tests/reset_menu_previews_periphery.rs:55-67` `apply_def`

#### `dup-0135` (near, 9 sites)

Proposed home: `a new shared module (sites span 2 files: src/contextgraph/sqlite.rs, tests/dash_calls_route_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:3318-3323` `apply_edge_inferred`
- `src/contextgraph/sqlite.rs:3329-3334` `apply_edge_inferred_evidence`
- `src/contextgraph/sqlite.rs:3362-3368` `apply_ref_caller`
- `src/contextgraph/sqlite.rs:4121-4129` `apply_doc_concept`
- `src/contextgraph/sqlite.rs:4232-4240` `apply_doc_link`
- `src/contextgraph/sqlite.rs:4458-4464` `apply_batch_ref`
- `src/contextgraph/sqlite.rs:6072-6080` `apply_unit_integrated`
- `src/contextgraph/sqlite.rs:7343-7353` `apply_def`
- `tests/dash_calls_route_periphery.rs:755-761` `apply_call`

#### `dup-0136` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:4482-4496` `edges_from`
- `src/contextgraph/sqlite.rs:6800-6818` `edges_touching`

#### `dup-0137` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:5160-5300` `calls_down_walks_the_execution_path_as_a_layered_deduped_dag_with_a_back_edge`
- `src/contextgraph/sqlite.rs:5498-5688` `calls_up_walks_the_call_sites_as_a_layered_deduped_dag_and_lists_referenced_but_not_called`

#### `dup-0138` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:5805-5838` `decision_fold_projects_no_agent_node_or_decided_edge`
- `src/contextgraph/sqlite.rs:5892-5920` `review_finding_projects_no_raised_edge_even_with_an_event_actor`

#### `dup-0139` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:6615-6624` `edge_projects`
- `src/contextgraph/sqlite.rs:7702-7714` `index_names`

#### `dup-0140` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:6627-6709` `every_node_and_edge_carries_the_projects_scope_on_fold`
- `src/contextgraph/sqlite.rs:6821-6912` `prune_is_project_scoped_leaving_another_projects_same_id_node_intact`

#### `dup-0141` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/contextgraph/sqlite.rs, tests/calls_down_execution_path_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:7356-7361` `apply_ref`
- `tests/calls_down_execution_path_periphery.rs:94-99` `apply_ref`

#### `dup-0142` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: src/contextgraph/sqlite.rs, tests/code_ingest_events.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:7441-7482` `the_cross_file_inferred_tier_is_order_independent`
- `src/contextgraph/sqlite.rs:7485-7500` `the_definition_upgrade_never_demotes_a_same_file_extracted_reference`
- `tests/code_ingest_events.rs:1013-1045` `a_definition_upgrades_only_the_exact_name_cross_file_reference_never_a_substring`

#### `dup-0143` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/contextgraph/sqlite.rs, src/main.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:7611-7613` `edge_desc`
- `src/main.rs:8327-8333` `runs_menu_line`

#### `dup-0144` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:7812-7831` `a_same_file_test_reference_increments_proven_by_and_records_its_evidence`
- `src/contextgraph/sqlite.rs:7834-7857` `two_test_references_accumulate_proven_by_to_2_with_both_evidence_entries`

#### `dup-0145` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:7936-7967` `an_unresolvable_test_reference_is_staged_and_reconciled_once_its_definition_later_folds`
- `src/contextgraph/sqlite.rs:8212-8243` `the_empty_boundary_sentinel_never_resolves_records_or_stages_anything_for_its_empty_name`

#### `dup-0146` (semantic, 60 sites)

Proposed home: `src/reap.rs as the one /proc-reading module (dash.rs's own /proc readers already duplicate reap.rs's field-after-the-comm's-closing-paren /proc/<pid>/stat parse - see the report's worked example)`

mandatory sweep: /proc-path string literals - 60 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/dash.rs:426-426` `"/proc"`
- `src/dash.rs:439-439` `"/proc/net/tcp"`
- `src/dash.rs:470-470` `"/proc"`
- `src/dash.rs:500-500` `"/proc/{pid}/stat"`
- `src/dash.rs:9306-9306` `"/proc"`
- `src/dash.rs:9387-9387` `"/proc"`
- `src/dash.rs:9395-9395` `"the /proc scan must find THIS process as the holder of its own listener"`
- `src/dash.rs:9402-9402` `"/proc"`
- `src/dash.rs:9421-9421` `"/proc"`
- `src/dash.rs:9454-9454` `"/proc"`
- `src/dash.rs:9477-9477` `"/proc"`
- `src/main.rs:14154-14154` `"/proc"`
- `src/main.rs:14258-14258` `"/proc"`
- `src/main.rs:24578-24578` `"/proc/{pid}/stat"`
- `src/main.rs:24579-24579` `"read /proc/{pid}/stat: {e}"`
- `src/main.rs:24582-24582` `"/proc stat has a parenthesised comm field"`
- `src/main.rs:24587-24587` `"/proc stat has a pgrp field after comm"`
- `src/reap.rs:133-133` `"/proc"`
- `src/reap.rs:215-215` `"/proc/{pid}/stat"`
- `src/reap.rs:226-226` `"/proc/{pid}/status"`
- `src/reap.rs:276-276` `"/proc/{}/cwd"`
- `tests/cli.rs:22467-22467` `"/proc"`
- `tests/cli.rs:24583-24583` `"/proc/{pid}/stat"`
- `tests/cli.rs:24584-24584` `"read /proc/{pid}/stat: {e}"`
- `tests/cli.rs:24587-24587` `"/proc stat has a parenthesised comm field"`
- `tests/cli.rs:24592-24592` `"/proc stat has a pgrp field after comm"`
- `tests/cli.rs:27626-27626` `"/proc"`
- `tests/cli.rs:27711-27711` `"/proc"`
- `tests/cli.rs:27758-27758` `"/proc/{holder_pid}/stat"`
- `tests/cli.rs:27773-27773` `"the holder pid {holder_pid} never reached the STOPPED (T) state in /proc"`
- `tests/cli.rs:27839-27839` `"/proc"`
- `tests/cli.rs:27862-27862` `"/proc"`
- `tests/cli.rs:27891-27891` `"/proc"`
- `tests/cli.rs:27941-27941` `"/proc/{holder_pid}/stat"`
- `tests/cli.rs:27956-27956` `"the holder pid {holder_pid} never reached the STOPPED (T) state in /proc"`
- `tests/cli.rs:28030-28030` `"/proc"`
- `tests/cli.rs:28056-28056` `"/proc"`
- `tests/cli.rs:28154-28154` `"/proc"`
- `tests/cli.rs:28192-28192` `"held_port_holder's message half and describe_held_port_if_confirmed's own return \
             must agree (scheduler-state letter normalized away since each call independently \
             re-reads /proc and can observe a state flap) - they are documented as sharing one \
             discovery"`
- `tests/cli.rs:28211-28211` `"/proc"`
- `tests/duplication_catalog_contract_periphery.rs:91-91` `"/proc-path string literals"`
- `tests/mutation_runner_pdeathsig_periphery.rs:75-75` `"/proc/{pid}/stat"`
- `tests/simplification_audit.rs:2726-2726` `"/proc-path string literals"`
- `tests/simplification_audit.rs:2859-2859` `"/proc"`
- `tests/simplification_audit.rs:2860-2860` `"src/reap.rs as the one /proc-reading module (dash.rs's own /proc readers already \
             duplicate reap.rs's field-after-the-comm's-closing-paren /proc/<pid>/stat parse - \
             see the report's worked example)"`
- `tests/simplification_audit.rs:2900-2900` `"/proc"`
- `tests/simplification_audit.rs:3122-3122` `"/proc/<pid>/stat or /proc/<pid>/status field-extraction functions"`
- `tests/simplification_audit.rs:3124-3124` `"src/reap.rs as the one /proc/<pid>/stat and /proc/<pid>/status parser, returning \
             whichever field each caller needs, so dash.rs::process_state and \
             reap.rs::pid_starttime/read_ppid stop each re-deriving the pid(comm)state... split"`
- `tests/simplification_audit.rs:3433-3433` `"Two real recall gaps surfaced this way and were closed by widening the mechanical \
         sweep with a new generalizable detector each - not a one-off citation - so the fix \
         catches every present and future instance of its class, each pinned by a real-tree \
         regression test: `find_proc_stat_or_status_readers` (decision \
         `u85c2-proc-stat-worked-example`) groups every function reading a `/proc/<pid>/stat` or \
         `/proc/<pid>/status` literal, closing the spec's own named worked example - \
         `src/dash.rs:499-507` `process_state` next to `src/reap.rs:190-197` `pid_starttime`, the \
         same job on the same file with a different field/shape, upheld at spec 62's capstone; \
         `find_parallel_constructor_clusters` (decision `u85c2-parallel-constructor-sweep`) \
         groups 2+ non-test functions per `(file, Self type)` that build a `Self {{ .. }}` / \
         `TypeName {{ .. }}` literal, closing `src/spawn.rs`'s `SpawnResult::liveness_fault` \
         reading MISSING from its own `ok`/`failed` cluster even though all three are parallel \
         constructors for one struct. A third worked example, `exploration_graph` (independently \
         defined test-fixture builders in `tests/dash_exploration_route_client_contract.rs` and \
         `tests/dash_kg_graph_route.rs`), was already caught correctly by the plain Jaccard pass \
         with no sweep needed - confirming the mechanical pass itself has real recall, not only \
         the two widened sweeps. Two further real defects, found on review rather than in this \
         draw, were closed the same way: a RECALL gap the architecture lens routed to this \
         criterion by name across two prior review rounds - this file's own bespoke source-text \
         lexer (`scan_file`/`tokenize`) duplicating the codebase's ONE canonical tree-sitter \
         extractor, `src/grounder/symbols/extract.rs::extract` (its own module doc's claim, \
         architecture 5.5.3) - closed by `find_bespoke_lexer_vs_canonical_extractor` (decision \
         `u85c2-bespoke-lexer-sweep`), a fourth generalizable sweep; and a PRECISION defect the \
         adversary found by reading every `same-named helper` cluster against \
         `ScannedFn::enclosing_impl` - `find_same_named_helper_functions` was misclassifying \
         REQUIRED trait-impl methods as coincidental duplication (`subscribe_all`/\
         `subscribe_stream` across the `EventStore` trait's three backend adapters plus a test \
         double, `blast_radius` across the `Grounder` trait's own default method, its override, \
         and a test double) - closed by excluding members whose extracted Self type differs \
         across the group when at least one comes from an actual `\" for \"` trait impl (decision \
         `u85c2-same-named-helper-trait-impl-precision-fix`), mirroring \
         `find_parallel_constructor_clusters`'s own `(file, Self type)` keying one function away. \
         Round 7 (decision `u85c4-r7-exclude-periphery-file-from-adversarial-population`) \
         excluded this criterion's own citation-guard periphery file from the draw's population \
         (see this subsection's opening paragraph) and redrew the sample; every one of the 19 \
         functions above marked \"no duplicate found by reading\" was re-read by hand against \
         its host file's surrounding context, exactly as this THOROUGHNESS check requires \
         whenever the draw changes. 18 of the 19 are genuinely not duplicates; `apply` at \
         `src/conductor.rs:29832-29834` is one shape worth naming so it is not mistaken for a \
         miss - a `Projection` test double's own required trait-impl body, the same \
         port-default/adapter-override/test-double shape `find_same_named_helper_functions`'s \
         trait-impl-precision fix (decision `u85c2-same-named-helper-trait-impl-precision-fix`) \
         already excludes from clustering by design, confirmed to still hold for this draw's own \
         instance of it. The 19th is a genuine small duplicate this catalog's `fn`-only scanner \
         (module doc, THE SCANNER) structurally cannot represent as a cluster: `gate_verdict_event` \
         (`src/conductor.rs:29191-29200`) and the `verdict` closure inside \
         `integrating_a_unit_stales_the_intersecting_downstream_units_cached_verdict_not_the_rest` \
         (`src/conductor.rs:30596-30605`) do the identical job - find the recorded `GateVerdict` \
         for a `\"<unit>/gate:g#<attempt>\"` replay key, panicking with the same message when none \
         exists - differing only in whether the unit segment is the literal `\"s\"` or a \
         parameter. A `let`-bound closure is not a `fn` item, so no change to this scanner short \
         of teaching it to see closures could catalog this pair as a cluster; named here, \
         prominently, rather than silently, so a later refactor - or a scanner that learns to see \
         closures - does not miss it."`
- `tests/simplification_audit.rs:3688-3688` `"A second mutation authority for one domain: the one previously-known \
        instance in this codebase (`src/dash.rs` reimplementing `src/reap.rs`'s \
        `/proc` pid scan, spec 85's own Goal example, upheld at spec 62's capstone) \
        is a duplicate READ-only reimplementation, not a bypassed MUTATION path - it \
        is section 2's finding (`u85c2-proc-stat-worked-example`, \
        `find_proc_stat_or_status_readers`), not re-counted here to avoid \
        double-charging one defect to two sections. Checked git as the one other \
        plausible second-authority candidate: every `Command::new(\"git\")` call \
        site in `src/conductor.rs` (23 sites) is at line >= 17297, inside \
        `#[cfg(test)] mod tests` - production `conductor.rs` never shells to git \
        directly. `src/worktree.rs` is the sole git-worktree-mutation authority \
        OUTSIDE the composition root. Inside it, `src/main.rs` (exempt from the \
        port-concretion-reach check above, not from this one) holds two more \
        git-worktree-mutation sites: `reap_then_remove_worktree` \
        (`main.rs:2791-2805`), the sanctioned worktree half of the spec-34/spec-79 \
        orphan-sweep and extensively reviewed across those specs - a deliberate \
        design choice, not a gap; and `materialize_config_at_rev` \
        (`main.rs:5510-5556`), a real, already-known, non-blocking gap \
        (`arch-u13-config-checkout-bypasses-worktree-authority` / \
        `arch-u2r-config-checkout-shells-git` / \
        `arch-u2r2-replayrunner-and-config-checkout-persist-not-introduced`: the \
        `Worktree` API is branch-creating and exposes no detach-at-rev checkout, so \
        this is a gap in that authority rather than a competing abstraction). No \
        second mutation authority found beyond the already-cited, \
        already-catalogued `/proc` case and this already-dispositioned \
        `materialize_config_at_rev` gap.\n"`
- `tests/simplification_audit.rs:4467-4467` `"Largest risk-reduction first is read as six tiers, ranked by the KIND of risk \
        each entry retires, highest first:\n\n\
        1. Tier 1 - active correctness risk, PLUS item 0: a use case already depends on the \
        wrong concretion, or two independent implementations of one concern can already \
        drift apart silently (section 3's two boundary violations; the one already-drifted \
        `/proc`-reading pair section 2 and section 3 both name) - live gaps, not just size. \
        Item 0 (deleting the 23-function dead-code set, section 4.3) is placed here too, \
        first of all: not a live-gap risk itself, but the cheapest, zero-behavior-change, \
        guaranteed-safe move available (spec 87's own Done-when: \"Tier 1 item 0\"), and it \
        shrinks the exact three god files tiers 2 and 3 operate on before either touches \
        them - ordered before items 1 and 2 for that reason, per this section's own \
        largest-first-within-a-tier rule (item 0's line delta exceeds either boundary-fix \
        item's, section 4.3).\n\
        2. Tier 2 - god-file test-module extraction: each of the three god files' own \
        inline `#[cfg(test)] mod tests` is the majority of that file's bulk (56-70% by \
        boundary-line count, per a direct read of each file), and moving it is a pure \
        relocation with no production-behavior change - the single largest safe line-count \
        reduction in this plan, and the precondition that makes tier 3 tractable.\n\
        3. Tier 3 - god-file production splits: section 1's own proposed module tree \
        applied to the (now much smaller) remaining production surface of each god file. \
        Higher execution risk than tier 2 because it touches live orchestration and CLI \
        logic, so it is sequenced after tier 2 shrinks the target first.\n\
        4. Tier 4 - named production duplication sweeps: the mechanical mandatory sweeps \
        section 2 ran regardless of the Jaccard pass (`Command::new`, `.rigger`-path \
        literals, sqlite `Connection::open`, error-shaping helpers), each already a single \
        committed cluster with its own proposed home.\n\
        5. Tier 5 - test-suite consolidation: section 5's own catalogued test-only \
        duplication. No production-correctness exposure at all (worst case a test \
        regresses, never the product), so it is ordered ahead only of tier 6 despite \
        touching the largest raw line count anywhere in this plan.\n\
        6. Tier 6 - remaining catalog sweep: the 327 src-touching clusters section 2 found \
        but tiers 1 and 4 did not individually name. Unlike every other tier, none of these \
        327 have been read and risk-assessed one at a time the way tiers 1-4's named \
        clusters have - they are consumed straight from the catalog - so this tier carries \
        production-correctness exposure tiers 2, 3 and 5 do not, and is ordered last: the \
        follow-up spec must triage each cluster's own production-or-test status before \
        merging it, not assume tier 5's blanket test-only treatment applies here too.\n\n\
        Within a tier, entries are ordered largest-first by the site or line count each \
        retires - the same rule the tiers themselves follow, applied one level down.\n\n"`
- `tests/simplification_audit.rs:4599-4599` `"#### 3. Retire the duplicate `/proc`-reading authority (`dup-0146` + `dup-0147`)\n\n"`
- `tests/simplification_audit.rs:4602-4602` `"- Scope: `src/dash.rs::process_state` (`src/dash.rs:499-507`) and \
        `src/main.rs::pgid_of` (`src/main.rs:23346-23359`) each independently re-derive \
        `/proc/<pid>/stat` and `/proc/<pid>/status` fields that `src/reap.rs` \
        (`pid_starttime`/`read_ppid`, `src/reap.rs:190-207`) already parses - the exact \
        \"second mutation authority\" example spec 85's own Goal names and spec 62's \
        capstone previously caught (`dup-0147`, 15 sites: `src/dash.rs`, `src/main.rs`, \
        `src/reap.rs`, `tests/cli.rs`, `tests/mutation_runner_pdeathsig_periphery.rs` - spec \
        91's own launcher-exits proving test reads `/proc/<pid>/stat` directly for the same \
        reason `dash.rs::process_state` does, growing this already-known cluster by one site \
        rather than opening a new one), plus 60 raw `/proc`-path string literals scattered \
        across `src/dash.rs`, `src/main.rs`, `src/reap.rs` and four test files with no \
        shared composer (`dup-0146`). Both clusters' own `proposed_home` agree: `src/reap.rs` \
        becomes the one `/proc`-reading module; `dash.rs` and `main.rs` call it instead of \
        re-parsing. NOT SYMMETRIC: `process_state` is reachable from `dash`'s own always-on \
        production server, so it is the actual active-correctness risk this tier-1 placement \
        is about; `pgid_of` sits inside `main.rs`'s `mod tests` (opened at `src/main.rs:12825`) \
        and is called only by `#[test]` fns, so on its own it earns no tier-1 placement - it \
        rides in this same item only because it shares `dup-0146`/`dup-0147`'s one root cause \
        and one proposed fix with `process_state`, not because retiring it retires any live \
        risk of its own.\n\
        - Files: `src/dash.rs`, `src/main.rs`, `src/reap.rs`, `tests/cli.rs` (`proc_pgid_of`, \
        `tests/cli.rs:23650-23663`, re-points at the same call).\n\
        - Expected line delta: negative - retires `process_state`'s and `pgid_of`'s own \
        parsing bodies in favor of calling `reap.rs`'s existing parser.\n\
        - Risk: low for both halves, for two different reasons. Section 3's own disposition \
        already establishes `process_state` as a duplicate READ-only reimplementation, never a \
        bypassed mutation path - nothing this touches can signal or kill a process, so it \
        carries none of the no-os-kill gate's own risk surface. `pgid_of`'s own risk is lower \
        still: being test-only, retiring it is ordinary test cleanup, not a \
        correctness-risk retirement - it is sequenced here for shared-fix convenience, not \
        because it independently needed tier-1 urgency.\n\
        - Unblocks: retires the codebase's only currently-known live instance of the \
        \"duplicate implementation reconciled after the fact\" pattern the operator's \
        strict-DRY rule targets - the concrete precedent spec 85's own Goal cites - and, as a \
        free byproduct, `main.rs`'s own test-only duplicate parser.\n\n"`
- `tests/simplification_audit.rs:7793-7793` `"fn state_of(pid: u32) -> Option<char> {\n    let stat = std::fs::read_to_string(format!(\"/proc/{pid}/stat\")).ok()?;\n    stat.rsplit_once(')')?.1.split_whitespace().next()?.chars().next()\n}\n"`
- `tests/simplification_audit.rs:7798-7798` `"fn starttime_of(pid: u32) -> Option<u64> {\n    let stat = std::fs::read_to_string(format!(\"/proc/{pid}/stat\")).ok()?;\n    stat.rsplit_once(')')?.1.split_whitespace().nth(19)?.parse().ok()\n}\n"`
- `tests/simplification_audit.rs:7803-7803` `"fn ppid_of(pid: u32) -> Option<u32> {\n    let stat = std::fs::read_to_string(format!(\"/proc/{pid}/stat\")).ok()?;\n    stat.rsplit_once(')')?.1.split_whitespace().nth(1)?.parse().ok()\n}\n"`
- `tests/simplification_audit.rs:7894-7894` `"fn a() {\n    let _ = std::fs::read_to_string(\"/proc/1/stat\");\n    let _ = \"hello\";\n}\n"`
- `tests/simplification_audit.rs:7897-7897` `"/proc"`
- `tests/simplification_audit.rs:7899-7899` `"/proc"`
- `tests/simplification_audit.rs:7963-7963` `"fn state_of(pid: u32) -> Option<char> {\n    let s = std::fs::read_to_string(format!(\"/proc/{pid}/stat\")).ok()?;\n    s.chars().next()\n}\nfn ppid_of(pid: u32) -> Option<u32> {\n    let s = std::fs::read_to_string(format!(\"/proc/{pid}/status\")).ok()?;\n    s.parse().ok()\n}\nfn unrelated() -> u32 {\n    1\n}\n"`

#### `dup-0147` (semantic, 15 sites)

Proposed home: `src/reap.rs as the one /proc/<pid>/stat and /proc/<pid>/status parser, returning whichever field each caller needs, so dash.rs::process_state and reap.rs::pid_starttime/read_ppid stop each re-deriving the pid(comm)state... split`

mandatory sweep: /proc/<pid>/stat or /proc/<pid>/status field-extraction functions - 15 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/dash.rs:499-507` `process_state`
- `src/main.rs:24577-24590` `pgid_of`
- `src/reap.rs:214-221` `pid_starttime`
- `src/reap.rs:225-231` `read_ppid`
- `tests/cli.rs:24582-24595` `proc_pgid_of`
- `tests/cli.rs:27706-27806` `cmd_dash_gives_the_stopped_listener_diagnosis_naming_resume_or_kill`
- `tests/cli.rs:27885-28000` `step_names_the_stopped_holder_when_the_step_paths_own_auto_start_hits_the_predecessor_scenario`
- `tests/mutation_runner_pdeathsig_periphery.rs:74-85` `is_running`
- `tests/simplification_audit.rs:2849-2880` `build_sweep_clusters`
- `tests/simplification_audit.rs:3119-3140` `build_extra_semantic_clusters`
- `tests/simplification_audit.rs:3385-3490` `render_adversarial_sample`
- `tests/simplification_audit.rs:4445-4994` `render_section_6`
- `tests/simplification_audit.rs:7785-7811` `three_near_but_not_identical_functions_cluster_as_near_with_every_site_and_one_home`
- `tests/simplification_audit.rs:7889-7900` `proc_literal_sweep_finds_a_proc_path_string_and_ignores_an_unrelated_one`
- `tests/simplification_audit.rs:7957-7970` `proc_stat_or_status_reader_sweep_finds_a_stat_reader_and_a_status_reader_but_not_an_unrelated_fn`

#### `dup-0148` (exact, 3 sites)

Proposed home: `dash::buckets`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:1993-1999` `underived_message`
- `src/dash.rs:2010-2016` `no_membership_message`
- `src/dash.rs:2022-2028` `label_kind`

#### `dup-0149` (exact, 2 sites)

Proposed home: `dash::response`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:4371-4377` `html`
- `src/dash.rs:4378-4384` `json`

#### `dup-0150` (semantic, 3 sites)

Proposed home: `dash::response - one canonical constructor, the rest thin variants over it (or a builder), rather than each re-listing every field`

mandatory sweep: parallel constructor functions - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/dash.rs:4371-4377` `html`
- `src/dash.rs:4378-4384` `json`
- `src/dash.rs:4385-4391` `text`

#### `dup-0151` (near, 5 sites)

Proposed home: `dash::support (consolidate these 5 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:5468-5484` `dash_serving_on_is_false_for_a_non_dash_listener`
- `src/dash.rs:5625-5645` `dash_serving_pid_on_reports_the_pid_a_real_dash_response_names`
- `src/dash.rs:5667-5690` `dash_serving_pid_on_assembles_a_head_that_spans_many_read_calls`
- `src/dash.rs:5697-5712` `dash_serving_pid_on_is_none_for_a_non_dash_listener`
- `src/dash.rs:5740-5760` `dash_serving_pid_on_is_none_when_the_pid_header_value_is_not_a_number`

#### `dup-0152` (near, 2 sites)

Proposed home: `dash::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:5495-5540` `dash_serving_on_is_bounded_against_a_byte_dribbling_holder`
- `src/dash.rs:5563-5614` `dash_serving_on_recognizes_the_header_fast_even_if_the_holder_never_finishes_the_block`

#### `dup-0153` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/dash_decisions_progressive_disclosure.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:6024-6088` `the_decision_history_renders_each_decision_as_a_native_details_with_preview_and_full_body`
- `tests/dash_decisions_progressive_disclosure.rs:134-212` `the_served_root_page_ships_the_decisions_progressive_disclosure_region`

#### `dup-0154` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/dash_kg_graph_route.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:7598-7632` `tiered_chain_graph`
- `tests/dash_kg_graph_route.rs:39-69` `fixture_graph`

#### `dup-0155` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/dash_kg_graph_route.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:8088-8113` `star_graph`
- `tests/dash_kg_graph_route.rs:632-657` `star_graph`

#### `dup-0156` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/dash_kg_graph_route.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:8297-8317` `chain_graph_local`
- `tests/dash_kg_graph_route.rs:75-95` `chain_graph`

#### `dup-0157` (near, 2 sites)

Proposed home: `dash::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:9420-9438` `describe_held_port_names_this_process_when_it_holds_the_port_itself`
- `src/dash.rs:9453-9469` `describe_held_port_if_confirmed_names_the_holder_when_independently_confirmed`

#### `dup-0158` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/dash_calls_route_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:9862-9875` `cedge`
- `tests/dash_calls_route_periphery.rs:85-98` `calls_edge`

#### `dup-0159` (exact, 12 sites)

Proposed home: `a new shared module (sites span 8 files: src/dash.rs, tests/code_lens_view_periphery.rs, tests/concepts_lens_view_periphery.rs, tests/dash_calls_route_periphery.rs, tests/dash_exploration_route_client_contract.rs, tests/files_lens_view_periphery.rs, tests/subject_lens_reprojection_contract.rs, tests/subject_lens_reprojection_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:9877-9883` `file_node`
- `tests/code_lens_view_periphery.rs:60-66` `ce`
- `tests/concepts_lens_view_periphery.rs:75-81` `ce`
- `tests/concepts_lens_view_periphery.rs:85-91` `doc`
- `tests/dash_calls_route_periphery.rs:61-67` `code_node`
- `tests/dash_calls_route_periphery.rs:69-75` `file_node`
- `tests/dash_exploration_route_client_contract.rs:54-60` `ce`
- `tests/dash_exploration_route_client_contract.rs:66-72` `dec`
- `tests/files_lens_view_periphery.rs:70-76` `bare_ce`
- `tests/files_lens_view_periphery.rs:81-87` `file_node`
- `tests/subject_lens_reprojection_contract.rs:53-59` `bare`
- `tests/subject_lens_reprojection_periphery.rs:73-79` `bare`

#### `dup-0160` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/calls_down_execution_path_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:9885-9887` `layer_of`
- `tests/calls_down_execution_path_periphery.rs:120-122` `layer_of`

#### `dup-0161` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/rationale_overlay_seam.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:10250-10260` `node`
- `tests/rationale_overlay_seam.rs:30-40` `node`

#### `dup-0162` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/dash_graph_exploration_overview.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:10277-10287` `edge`
- `tests/dash_graph_exploration_overview.rs:61-71` `edge`

#### `dup-0163` (exact, 2 sites)

Proposed home: `dash::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:10338-10340` `ids`
- `src/dash.rs:10341-10343` `kinds`

#### `dup-0164` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/metadata_card_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:10543-10567` `an_absent_explain_leaves_the_graph_route_unchanged`
- `src/dash.rs:10832-10852` `a_cluster_drill_carries_no_memory_field`
- `tests/metadata_card_periphery.rs:350-367` `the_served_route_degrades_gracefully_for_an_unknown_card_subject`

#### `dup-0165` (exact, 4 sites)

Proposed home: `a new shared module (sites span 3 files: src/dash.rs, tests/metadata_card_periphery.rs, tests/subject_view_memory_rail_contract.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:10581-10590` `node`
- `src/dash.rs:10868-10877` `node`
- `tests/metadata_card_periphery.rs:41-50` `node`
- `tests/subject_view_memory_rail_contract.rs:37-46` `node`

#### `dup-0166` (near, 4 sites)

Proposed home: `a new shared module (sites span 3 files: src/dash.rs, tests/metadata_card_periphery.rs, tests/subject_view_memory_rail_contract.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:10609-10634` `subject_graph`
- `src/dash.rs:10896-10931` `card_graph`
- `tests/metadata_card_periphery.rs:69-105` `fixture_graph`
- `tests/subject_view_memory_rail_contract.rs:64-89` `subject_graph`

#### `dup-0167` (near, 2 sites)

Proposed home: `dash::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:11045-11058` `card_of_a_file_reports_no_proof_of_its_own`
- `src/dash.rs:11065-11075` `card_tolerates_a_malformed_proof_evidence_attr`

#### `dup-0168` (near, 4 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/metadata_card_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:11197-11239` `the_card_route_serves_a_known_subjects_card_and_null_for_an_unknown_one`
- `tests/metadata_card_periphery.rs:124-171` `the_served_route_carries_a_code_subjects_card`
- `tests/metadata_card_periphery.rs:179-205` `the_served_route_omits_community_and_line_keys_for_a_membership_less_entity`
- `tests/metadata_card_periphery.rs:374-400` `the_card_param_takes_precedence_over_a_stray_seed_or_lens_param`

#### `dup-0169` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/distiller.rs, src/main.rs, src/playbooks.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/distiller.rs:44-53` `fnv1a_64`
- `src/main.rs:1008-1017` `fnv1a_64`
- `src/playbooks.rs:36-45` `fnv1a_64`

#### `dup-0170` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/distiller.rs, src/playbooks.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/distiller.rs:211-223` `render`
- `src/playbooks.rs:125-136` `render`

#### `dup-0171` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/distiller.rs, src/playbooks.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/distiller.rs:231-244` `rebuild`
- `src/playbooks.rs:143-156` `rebuild`

#### `dup-0172` (exact, 3 sites)

Proposed home: `a new shared module (sites span 2 files: src/distiller.rs, src/playbooks.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/distiller.rs:259-269` `decision`
- `src/distiller.rs:271-281` `finding`
- `src/playbooks.rs:163-173` `lesson`

#### `dup-0173` (near, 2 sites)

Proposed home: `docs::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/docs.rs:97-115` `render_using_rigger_skill`
- `src/docs.rs:119-130` `render_handbook_discipline`

#### `dup-0174` (near, 2 sites)

Proposed home: `docs::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/docs.rs:450-452` `render_planning_a_spec_skill`
- `src/docs.rs:621-623` `render_planning_field_guide`

#### `dup-0175` (near, 7 sites)

Proposed home: `docs::support (consolidate these 7 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/docs.rs:628-707` `render_reset_store_skill`
- `src/docs.rs:712-757` `render_build_graph_skill`
- `src/docs.rs:762-806` `render_reindex_skill`
- `src/docs.rs:811-865` `render_resume_a_run_skill`
- `src/docs.rs:872-922` `render_handle_an_escalation_skill`
- `src/docs.rs:1027-1100` `render_restore_the_dash_skill`
- `src/docs.rs:1109-1179` `render_diagnose_churn_skill`

#### `dup-0176` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/docs.rs, tests/cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/docs.rs:2062-2080` `restore_the_dash_carries_the_hung_holder_diagnosis`
- `tests/cli.rs:25916-25934` `rigger_workflow_yml_pins_the_checkin_stage_and_mutation_gate_definition_to_spec_91`

#### `dup-0177` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/driver/cli.rs:403-430` `persona_is_the_system_prompt_task_is_the_prompt`
- `src/driver/cli.rs:433-444` `recurse_false_drops_the_agent_tool_from_allowed_tools`

#### `dup-0178` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/driver/replay.rs, tests/spawn_target_dir_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/driver/replay.rs:376-378` `no_emit`
- `tests/spawn_target_dir_periphery.rs:86-88` `no_emit`

#### `dup-0179` (near, 2 sites)

Proposed home: `replay::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/driver/replay.rs:380-387` `worker`
- `src/driver/replay.rs:1596-1603` `reviewer`

#### `dup-0180` (near, 2 sites)

Proposed home: `replay::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/driver/replay.rs:605-618` `reclaim_unit_mutation_scratch_never_cross_matches_a_unit_id_that_is_a_string_prefix_of_another`
- `src/driver/replay.rs:624-635` `reclaim_unit_mutation_scratch_is_a_no_op_for_an_empty_unit_id`

#### `dup-0181` (near, 4 sites)

Proposed home: `replay::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/driver/replay.rs:1929-2027` `an_emit_only_approve_gating_persona_hard_errors_on_the_replay_driver`
- `src/driver/replay.rs:2030-2127` `a_concurrent_sibling_approve_does_not_hard_error_a_units_genuine_empty_verdict_reject`
- `src/driver/replay.rs:2130-2227` `a_parked_unanswered_sibling_does_not_suppress_this_units_own_approve_backstop`
- `src/driver/replay.rs:2230-2330` `a_closed_sibling_window_overlapping_this_units_own_approve_still_hard_errors`

#### `dup-0182` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/driver/workflow.rs, src/watch.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/driver/workflow.rs:83-85` `new`
- `src/watch.rs:577-579` `new`

#### `dup-0183` (near, 3 sites)

Proposed home: `contract::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/contract.rs:172-192` `append_assigns_revisions`
- `src/eventstore/contract.rs:296-341` `backward_stream_read_reverses_set`
- `src/eventstore/contract.rs:345-370` `forward_stream_read_honors_nonzero_from`

#### `dup-0184` (near, 2 sites)

Proposed home: `contract::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/contract.rs:248-269` `subscription_replays_then_goes_live`
- `src/eventstore/contract.rs:271-292` `stream_subscription_replays_then_goes_live`

#### `dup-0185` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/eventstore/contract.rs, src/spawn.rs, tests/adoption_keys_on_criterion_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/contract.rs:825-832` `read_stream`
- `src/spawn.rs:1492-1499` `read_stream`
- `tests/adoption_keys_on_criterion_periphery.rs:2636-2643` `read_stream`

#### `dup-0186` (exact, 4 sites)

Proposed home: `a new shared module (sites span 4 files: src/eventstore/contract.rs, src/spawn.rs, tests/adoption_keys_on_criterion_periphery.rs, tests/integrate_conflict_merge_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/contract.rs:844-846` `subscribe_stream`
- `src/spawn.rs:1514-1516` `subscribe_stream`
- `tests/adoption_keys_on_criterion_periphery.rs:2658-2660` `subscribe_stream`
- `tests/integrate_conflict_merge_periphery.rs:2102-2104` `subscribe_stream`

#### `dup-0187` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/eventstore/kurrentdb.rs, src/eventstore/sqlite.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/kurrentdb.rs:121-123` `from_nanos`
- `src/eventstore/sqlite.rs:1474-1476` `from_nanos`

#### `dup-0188` (near, 2 sites)

Proposed home: `kurrentdb::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/kurrentdb.rs:612-617` `a_single_event_reports_the_position_the_server_issued`
- `src/eventstore/kurrentdb.rs:647-657` `a_batch_reports_the_revision_span_the_ack_names`

#### `dup-0189` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/eventstore/mod.rs, src/spawn.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/mod.rs:121-124` `with_valid_from`
- `src/spawn.rs:568-571` `with_meta`

#### `dup-0190` (semantic, 2 sites)

Proposed home: `mod::appended - one canonical constructor, the rest thin variants over it (or a builder), rather than each re-listing every field`

mandatory sweep: parallel constructor functions - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/eventstore/mod.rs:158-162` `all`
- `src/eventstore/mod.rs:166-168` `from_placements`

#### `dup-0191` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/eventstore/mod.rs, src/sidecar.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/mod.rs:536-541` `drop`
- `src/sidecar.rs:219-224` `drop`

#### `dup-0192` (exact, 3 sites)

Proposed home: `a new shared module (sites span 2 files: src/eventstore/mod.rs, src/spec.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/mod.rs:797-803` `strips_userinfo_and_query_keeps_scheme_host_port`
- `src/eventstore/mod.rs:822-828` `a_credential_smuggled_after_the_path_is_dropped_with_the_path`
- `src/spec.rs:1938-1945` `strip_inline_code_direct_exact_output_pins_a_zero_width_quote_pair`

#### `dup-0193` (exact, 11 sites)

Proposed home: `a new shared module (sites span 4 files: src/eventstore/mod.rs, src/spec.rs, tests/simplification_audit.rs, tests/store_secrets_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/mod.rs:806-811` `strips_a_bare_user_with_no_password`
- `src/eventstore/mod.rs:814-819` `an_already_credential_free_endpoint_is_unchanged`
- `src/eventstore/mod.rs:891-896` `strips_a_userinfo_with_no_password`
- `src/eventstore/mod.rs:930-935` `plain_text_with_no_url_is_untouched`
- `src/spec.rs:1567-1573` `ownership_check_recognizes_owner_inside_a_hyphenated_compound`
- `tests/simplification_audit.rs:6876-6878` `impl_self_type_still_handles_a_generic_self_type_with_a_where_clause`
- `tests/simplification_audit.rs:6904-6906` `impl_self_type_handles_a_bound_generic_self_type`
- `tests/simplification_audit.rs:6909-6911` `impl_self_type_handles_a_trait_impl_on_a_lifetime_generic_self_type`
- `tests/simplification_audit.rs:6914-6919` `impl_self_type_handles_a_generic_trait_impl_on_a_generic_self_type`
- `tests/simplification_audit.rs:6922-6927` `impl_self_type_handles_a_const_generic_self_type`
- `tests/store_secrets_periphery.rs:92-94` `redact_conn_on_the_empty_string_is_empty`

#### `dup-0194` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/eventstore/mod.rs, src/spec.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/mod.rs:836-853` `a_delimiter_inside_the_userinfo_never_leaks_the_credential_head`
- `src/spec.rs:1898-1919` `strip_inline_code_direct_exact_output_pins_the_one_span_per_kind_rule`

#### `dup-0195` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/eventstore/mod.rs, tests/store_secrets_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/mod.rs:878-888` `strips_user_and_password_but_keeps_scheme_host_and_query`
- `tests/store_secrets_periphery.rs:78-88` `redact_conn_scrubs_the_whole_userinfo_when_the_authority_has_several_at_signs`

#### `dup-0196` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/eventstore/mod.rs, tests/store_secrets_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/mod.rs:899-906` `leaves_a_conn_with_no_userinfo_unchanged`
- `tests/store_secrets_periphery.rs:65-72` `redact_conn_leaves_an_at_sign_in_the_path_alone`

#### `dup-0197` (semantic, 2 sites)

Proposed home: `one shared `content_key_index_name` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/eventstore/sqlite.rs:226-236` `content_key_index_name`
- `tests/store_content_identity_periphery.rs:1753-1774` `content_key_index_name`

#### `dup-0198` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/eventstore/sqlite.rs, src/metrics.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/sqlite.rs:1103-1109` `factor`
- `src/metrics.rs:362-368` `cost_per_upheld`

#### `dup-0199` (near, 4 sites)

Proposed home: `a new shared module (sites span 3 files: src/eventstore/sqlite.rs, src/grounder/symbols/events.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/sqlite.rs:1780-1785` `direction_sql`
- `src/grounder/symbols/events.rs:713-724` `kind_str`
- `src/grounder/symbols/events.rs:728-737` `lang_str`
- `tests/simplification_audit.rs:3774-3780` `disposition_label`

#### `dup-0200` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/eventstore/sqlite.rs, tests/reset_derived_compaction_periphery.rs, tests/store_content_identity_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/sqlite.rs:1812-1827` `subject_of`
- `tests/reset_derived_compaction_periphery.rs:2110-2125` `path_subject_of`
- `tests/store_content_identity_periphery.rs:233-249` `path_subject_of`

#### `dup-0201` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/eventstore/sqlite.rs, src/ingest.rs, tests/published_content_key_split_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/sqlite.rs:1830-1833` `split`
- `src/ingest.rs:336-339` `derived_key_parts`
- `tests/published_content_key_split_periphery.rs:79-82` `split`

#### `dup-0202` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: src/eventstore/sqlite.rs, tests/store_content_identity_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/sqlite.rs:1838-1840` `identity`
- `src/eventstore/sqlite.rs:2705-2707` `other_identity`
- `tests/store_content_identity_periphery.rs:264-266` `project_policy`

#### `dup-0203` (near, 4 sites)

Proposed home: `a new shared module (sites span 3 files: src/eventstore/sqlite.rs, tests/published_content_key_split_periphery.rs, tests/store_content_identity_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/sqlite.rs:1842-1844` `keyed`
- `src/eventstore/sqlite.rs:2711-2713` `other_keyed`
- `tests/published_content_key_split_periphery.rs:86-88` `keyed`
- `tests/store_content_identity_periphery.rs:274-276` `keyed`

#### `dup-0204` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/eventstore/sqlite.rs, tests/store_content_identity_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/sqlite.rs:1847-1852` `batch`
- `tests/store_content_identity_periphery.rs:279-284` `batch`

#### `dup-0205` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/sqlite.rs:2109-2137` `one_files_generations_never_leak_into_another_files_subject`
- `src/eventstore/sqlite.rs:2488-2523` `a_generation_that_is_a_string_prefix_of_a_later_one_is_still_found`

#### `dup-0206` (near, 3 sites)

Proposed home: `sqlite::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/sqlite.rs:3618-3655` `measure_derived_duplication_scopes_to_the_stream_prefix`
- `src/eventstore/sqlite.rs:3658-3686` `measure_derived_duplication_on_a_clean_log_reports_no_duplication`
- `src/eventstore/sqlite.rs:3689-3743` `measure_derived_duplication_treats_the_same_key_under_two_covered_types_as_two_distinct_subjects`

#### `dup-0207` (near, 4 sites)

Proposed home: `a new shared module (sites span 4 files: src/failure.rs, src/gate.rs, src/ledger.rs, src/watch.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/failure.rs:43-49` `as_str`
- `src/gate.rs:77-83` `as_str`
- `src/ledger.rs:47-59` `as_str`
- `src/watch.rs:192-201` `response`

#### `dup-0208` (exact, 3 sites)

Proposed home: `a new shared module (sites span 2 files: src/failure.rs, src/gate.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/failure.rs:65-67` `reruns`
- `src/failure.rs:74-76` `demotes_on_persistent_failure`
- `src/gate.rs:51-53` `runs_inline`

#### `dup-0209` (exact, 2 sites)

Proposed home: `gate::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/gate.rs:31-37` `parse`
- `src/gate.rs:69-75` `parse`

#### `dup-0210` (near, 3 sites)

Proposed home: `gate::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/gate.rs:1172-1208` `exec_runner_exports_cargo_target_dir_only_when_given`
- `src/gate.rs:1211-1239` `exec_runner_forces_cargo_target_dir_onto_build_cache_dir_when_target_dir_is_empty`
- `src/gate.rs:1272-1294` `exec_runner_target_dir_wins_over_build_cache_dir_when_both_are_given`

#### `dup-0211` (near, 2 sites)

Proposed home: `gate::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/gate.rs:1242-1269` `exec_runner_env_vars_reach_the_gate_command_through_the_flock_guard_wrapper`
- `src/gate.rs:1399-1427` `exec_runner_degrades_to_unguarded_when_the_guard_path_cannot_be_opened`

#### `dup-0212` (exact, 6 sites)

Proposed home: `a new shared module (sites span 6 files: src/gate.rs, src/reap.rs, tests/mutation_scratch_reap_base_guard_periphery.rs, tests/reap_before_removal_periphery.rs, tests/spawn_scratch_reap_authorized_root_periphery.rs, tests/worktree_remove_relocated_scratch_base_guard_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/gate.rs:1300-1308` `wait_until`
- `src/reap.rs:558-566` `wait_until`
- `tests/mutation_scratch_reap_base_guard_periphery.rs:65-73` `wait_until`
- `tests/reap_before_removal_periphery.rs:52-60` `wait_until`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:149-157` `wait_until`
- `tests/worktree_remove_relocated_scratch_base_guard_periphery.rs:66-74` `wait_until`

#### `dup-0213` (near, 4 sites)

Proposed home: `gate::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/gate.rs:1596-1615` `build_env_resolves_wrapper_cache_dir_and_incremental_off_when_configured`
- `src/gate.rs:1618-1625` `build_env_derives_the_wrapper_specific_cache_dir_var_name`
- `src/gate.rs:1701-1708` `build_env_jobs_cap_reaches_the_build_when_set`
- `src/gate.rs:1711-1723` `build_env_jobs_cap_is_independent_of_the_wrapper`

#### `dup-0214` (near, 2 sites)

Proposed home: `gate::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/gate.rs:1644-1685` `exec_runner_applies_the_build_env_it_is_given`
- `src/gate.rs:1742-1761` `exec_runner_applies_the_jobs_cap_it_is_given`

#### `dup-0215` (exact, 2 sites)

Proposed home: `gate::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/gate.rs:1799-1808` `resolve_wrapper_name_auto_probes_known_wrappers_and_finds_one_present`
- `src/gate.rs:1823-1833` `resolve_wrapper_name_named_wrapper_present_on_path_resolves_to_itself`

#### `dup-0216` (near, 2 sites)

Proposed home: `gate::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/gate.rs:1978-1994` `resolve_build_layer_named_wrapper_with_an_uncreatable_dir_errors_naming_dir_and_key`
- `src/gate.rs:2062-2081` `resolve_build_layer_named_wrapper_with_a_preexisting_unwritable_dir_errors_naming_dir_and_key`

#### `dup-0217` (exact, 2 sites)

Proposed home: `gate::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/gate.rs:1997-2011` `resolve_build_layer_auto_with_an_uncreatable_dir_skips_the_whole_layer`
- `src/gate.rs:2085-2100` `resolve_build_layer_auto_with_a_preexisting_unwritable_dir_skips_the_whole_layer`

#### `dup-0218` (near, 2 sites)

Proposed home: `events::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/design/events.rs:20-43` `concept_events`
- `src/grounder/design/events.rs:51-74` `link_events`

#### `dup-0219` (semantic, 2 sites)

Proposed home: `one shared `project_batches` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/grounder/design/events.rs:90-114` `project_batches`
- `src/grounder/symbols/events.rs:88-90` `project_batches`

#### `dup-0220` (near, 2 sites)

Proposed home: `events::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/design/events.rs:200-214` `the_emit_is_deterministic_and_sorts_by_kind_then_id`
- `src/grounder/design/events.rs:320-337` `the_link_emit_is_deterministic_and_sorts_by_rel_then_from_then_to`

#### `dup-0221` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/grounder/design/extract.rs, src/spec.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/design/extract.rs:433-436` `is_markdown`
- `src/spec.rs:541-548` `starts_new_element`
- `tests/simplification_audit.rs:2789-2796` `looks_error_shaping`

#### `dup-0222` (near, 2 sites)

Proposed home: `extract::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/design/extract.rs:532-537` `first_heading`
- `src/grounder/design/extract.rs:540-546` `section_headings`

#### `dup-0223` (near, 4 sites)

Proposed home: `extract::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/design/extract.rs:602-611` `a_load_bearing_decision_doc_becomes_a_single_arch_decision_node`
- `src/grounder/design/extract.rs:614-620` `a_spec_shape_or_loop_discipline_doc_becomes_a_handbook_rule_node`
- `src/grounder/design/extract.rs:623-637` `a_why_comment_in_a_source_file_becomes_a_rationale_node`
- `src/grounder/design/extract.rs:957-966` `a_source_file_is_never_a_usage_doc_and_its_rationale_stays_in_scope`

#### `dup-0224` (near, 2 sites)

Proposed home: `extract::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/design/extract.rs:748-761` `a_rationale_explains_the_file_it_annotates`
- `src/grounder/design/extract.rs:764-780` `a_fenced_code_example_path_is_not_mistaken_for_a_specifies_link`

#### `dup-0225` (near, 2 sites)

Proposed home: `model::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/design/model.rs:30-37` `node_kind`
- `src/grounder/design/model.rs:81-89` `rel`

#### `dup-0226` (near, 2 sites)

Proposed home: `events::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/symbols/events.rs:276-290` `empty_structural_boundary_event`
- `src/grounder/symbols/events.rs:421-435` `empty_evidence_boundary_event`

#### `dup-0227` (semantic, 3 sites)

Proposed home: `src/grounder/symbols/extract.rs::extract as the ONE function that touches source parsing (already its own module doc's claim, architecture 5.5.3) - this file's own scan_file/tokenize are ad hoc scanners for the identical job and should route through an injected-grammar extractor rather than re-deriving structure by hand`

mandatory sweep: bespoke source-text lexer/scanner functions duplicating the canonical tree-sitter extractor - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/grounder/symbols/extract.rs:34-178` `extract`
- `tests/simplification_audit.rs:207-209` `scan_file`
- `tests/simplification_audit.rs:2163-2269` `tokenize`

#### `dup-0228` (near, 7 sites)

Proposed home: `extract::support (consolidate these 7 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/symbols/extract.rs:860-928` `test_annotated_definitions_and_everything_nested_inside_them_are_marked_is_test`
- `src/grounder/symbols/extract.rs:931-969` `cfg_predicates_naming_test_are_recognized_and_similarly_spelled_tokens_are_not`
- `src/grounder/symbols/extract.rs:972-1007` `negated_and_cfg_attr_predicates_naming_test_do_not_mark_the_item_test`
- `src/grounder/symbols/extract.rs:1010-1045` `a_not_wrapping_a_non_test_atom_never_marks_the_item_test`
- `src/grounder/symbols/extract.rs:1048-1083` `compound_predicates_with_nested_negation_or_a_non_test_disjunct_do_not_mark_the_item_test`
- `src/grounder/symbols/extract.rs:1086-1126` `a_trailing_same_line_comment_on_a_cfg_test_attribute_does_not_sever_the_scan`
- `src/grounder/symbols/extract.rs:1129-1182` `an_inner_cfg_test_attribute_marks_its_enclosing_module_and_the_module_marks_its_children`

#### `dup-0229` (near, 2 sites)

Proposed home: `extract::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/symbols/extract.rs:1273-1319` `extent_spans_a_destructuring_or_default_brace_signature_to_the_full_body`
- `src/grounder/symbols/extract.rs:1322-1384` `extent_generalizes_across_grammars_python_nested_def_and_js_brace_string`

#### `dup-0230` (semantic, 2 sites)

Proposed home: `one shared `changed_files` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/grounder/symbols/grounder.rs:75-98` `changed_files`
- `src/worktree.rs:518-521` `changed_files`

#### `dup-0231` (near, 3 sites)

Proposed home: `grounder::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/symbols/grounder.rs:807-847` `a_concurrent_reindex_from_a_second_grounder_is_not_clobbered`
- `src/grounder/symbols/grounder.rs:850-873` `reindex_replaces_only_a_changed_files_symbols`
- `src/grounder/symbols/grounder.rs:876-903` `reindex_over_a_deleted_file_stops_grounding_it`

#### `dup-0232` (near, 2 sites)

Proposed home: `mod::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/symbols/mod.rs:319-329` `a_file_added_to_the_tree_since_the_index_was_built_is_flagged`
- `src/grounder/symbols/mod.rs:332-347` `a_file_removed_from_the_tree_since_the_index_was_built_is_flagged`

#### `dup-0233` (exact, 2 sites)

Proposed home: `model::symbol_index`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/symbols/model.rs:169-171` `insert_file`
- `src/grounder/symbols/model.rs:189-191` `set_hash`

#### `dup-0234` (near, 2 sites)

Proposed home: `store::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/symbols/store.rs:23-28` `index_path`
- `src/grounder/symbols/store.rs:33-38` `lock_path`

#### `dup-0235` (near, 2 sites)

Proposed home: `hooks::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/hooks.rs:133-144` `installs_and_is_idempotent`
- `src/hooks.rs:158-170` `pretooluse_hook_installs_and_is_idempotent`

#### `dup-0236` (near, 2 sites)

Proposed home: `hooks::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/hooks.rs:147-155` `preserves_other_settings`
- `src/hooks.rs:203-218` `pretooluse_hook_composes_with_the_session_start_hook`

#### `dup-0237` (near, 2 sites)

Proposed home: `hooks::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/hooks.rs:235-247` `mcp_server_preserves_other_servers_and_other_top_level_keys`
- `src/hooks.rs:250-259` `mcp_server_self_heals_a_drifted_entry`

#### `dup-0238` (semantic, 2 sites)

Proposed home: `ledger::attention_entry - one canonical constructor, the rest thin variants over it (or a builder), rather than each re-listing every field`

mandatory sweep: parallel constructor functions - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/ledger.rs:209-219` `unit_scoped`
- `src/ledger.rs:222-228` `run_scoped`

#### `dup-0239` (near, 2 sites)

Proposed home: `ledger::run_state`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/ledger.rs:643-648` `is_terminal`
- `src/ledger.rs:651-656` `is_integrated`

#### `dup-0240` (near, 7 sites)

Proposed home: `a new shared module (sites span 2 files: src/ledger.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/ledger.rs:944-951` `short_run_id_truncates_to_twelve_chars_and_passes_shorter_ids_through`
- `src/ledger.rs:954-969` `spec_stem_extracts_and_sanitizes_the_file_stem`
- `tests/simplification_audit.rs:6862-6873` `impl_self_type_strips_a_trailing_where_clause_on_a_non_generic_self_type`
- `tests/simplification_audit.rs:6881-6892` `impl_self_type_strips_a_leading_dyn_token_on_the_self_type`
- `tests/simplification_audit.rs:6930-6936` `strip_trailing_where_clause_is_a_word_boundary_match_not_a_substring_match`
- `tests/simplification_audit.rs:6978-6982` `pluralize_functions_uses_singular_only_at_exactly_one`
- `tests/simplification_audit.rs:7553-7563` `ident_kind_marker_classifies_by_casing`

#### `dup-0241` (near, 2 sites)

Proposed home: `liveness::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/liveness.rs:486-499` `marker_filename_hex_escapes_every_byte_outside_alphanumeric_and_hyphen`
- `src/liveness.rs:502-521` `marker_filename_hex_escapes_dots_so_no_encoded_result_can_ever_be_a_path_traversal_component`

#### `dup-0242` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/liveness.rs, src/spec.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/liveness.rs:524-539` `marker_filename_is_none_only_for_a_truly_empty_input_so_a_join_can_never_be_a_no_op`
- `src/spec.rs:2187-2189` `heading_level_rejects_more_than_six_hashes`

#### `dup-0243` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/liveness.rs, src/main.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/liveness.rs:567-581` `marker_filename_is_injective_so_two_ids_that_collided_under_a_prior_placeholder_scheme_no_longer_do`
- `src/main.rs:20044-20058` `normalize_origin_url_separates_distinct_repos_and_lowercases_only_the_host`

#### `dup-0244` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/liveness.rs, src/worktree.rs, tests/run_scoping_survives_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/liveness.rs:734-736` `read`
- `src/worktree.rs:4335-4337` `read_stream`
- `tests/run_scoping_survives_periphery.rs:133-135` `read`

#### `dup-0245` (near, 2 sites)

Proposed home: `liveness::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/liveness.rs:982-995` `any_marker_fresh_finds_a_fresh_marker_nested_under_a_run_id_directory`
- `src/liveness.rs:998-1012` `any_marker_fresh_is_false_once_every_marker_is_older_than_max_age`

#### `dup-0246` (near, 3 sites)

Proposed home: `main::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:539-542` `config_rigger_dir`
- `src/main.rs:907-910` `project_identity`
- `src/main.rs:13026-13029` `git_repo`

#### `dup-0247` (semantic, 2 sites)

Proposed home: `one shared `project_identity` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/main.rs:907-910` `project_identity`
- `tests/reset_derived_compaction_periphery.rs:616-638` `project_identity`

#### `dup-0248` (exact, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:1649-1651` `usage`
- `src/main.rs:11496-11503` `print_scaffold_pointer`

#### `dup-0249` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/main.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:1762-1764` `find_store_dir_from`
- `tests/simplification_audit.rs:925-927` `scan_target_files`

#### `dup-0250` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:4525-4584` `cmd_graph_communities`
- `src/main.rs:4604-4663` `cmd_graph_concepts`

#### `dup-0251` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/main.rs, tests/reset_build_cache_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:10662-10681` `dir_size_bytes`
- `tests/reset_build_cache_periphery.rs:93-110` `dir_bytes`

#### `dup-0252` (exact, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:11691-11693` `shim_dir`
- `src/main.rs:11791-11793` `docs_overlay_path`

#### `dup-0253` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:12486-12502` `install_lookup_hook`
- `src/main.rs:12511-12525` `install_operator_mcp`

#### `dup-0254` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/main.rs, tests/heartbeat_write_read_agree_periphery.rs, tests/reset_derived_live_writer_guard_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:13037-13047` `git_repo_at`
- `tests/heartbeat_write_read_agree_periphery.rs:68-78` `git_toplevel`
- `tests/reset_derived_live_writer_guard_periphery.rs:57-68` `git_toplevel`

#### `dup-0255` (exact, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:15092-15098` `run_started_at`
- `src/main.rs:15099-15105` `decision`

#### `dup-0256` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/main.rs, tests/graph_click_to_seed_repoint.rs, tests/graph_seeds_repoint_denoise.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:15162-15166` `ev`
- `tests/graph_click_to_seed_repoint.rs:26-30` `ev`
- `tests/graph_seeds_repoint_denoise.rs:23-27` `ev`

#### `dup-0257` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:15667-15706` `per_operation_skills_reference_only_real_subcommands`
- `src/main.rs:15716-15744` `watching_discipline_skills_reference_only_real_subcommands`

#### `dup-0258` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:16276-16320` `git_is_ancestor_decides_commit_order_in_a_real_repo`
- `src/main.rs:16323-16369` `git_commit_distance_counts_commits_ahead_in_a_real_repo`

#### `dup-0259` (near, 3 sites)

Proposed home: `main::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:16389-16394` `behind_the_tree_message_is_silent_when_versions_already_match`
- `src/main.rs:16397-16407` `behind_the_tree_message_is_silent_when_either_side_is_unversioned`
- `src/main.rs:16410-16419` `behind_the_tree_message_is_silent_on_an_undecidable_or_zero_distance`

#### `dup-0260` (exact, 25 sites)

Proposed home: `a new shared module (sites span 24 files: src/main.rs, tests/adaptive_labels_periphery.rs, tests/code_lens_overview_collapse_viz.rs, tests/concepts_lens_view_periphery.rs, tests/dash_calls_render_viz.rs, tests/dash_decisions_progressive_disclosure.rs, tests/dash_graph_exploration_viz.rs, tests/dash_kg_graph_route.rs, tests/dash_release_ready.rs, tests/files_lens_directory_hulls_viz.rs, tests/gitsemver_derivation.rs, tests/gitsemver_worktree_periphery.rs, tests/graph_collision_body_and_tiebreak.rs, tests/graph_density_spread_floor_and_centring.rs, tests/metadata_card_handoff_viz.rs, tests/native_driver_pipelining_behavior.rs, tests/proof_row_renders_on_the_card.rs, tests/readable_graph_adaptive_labels.rs, tests/readable_graph_density_scaled_spacing.rs, tests/readable_graph_layout_separation.rs, tests/subject_lens_overlay_client_arms.rs, tests/subject_lens_overlay_served_page.rs, tests/subject_view_memory_rail_client.rs, tests/validate_behind_the_tree_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:16436-16442` `gitsemver_available`
- `src/main.rs:22867-22873` `npm_available`
- `tests/adaptive_labels_periphery.rs:65-71` `node_available`
- `tests/code_lens_overview_collapse_viz.rs:39-45` `node_available`
- `tests/concepts_lens_view_periphery.rs:702-708` `node_available`
- `tests/dash_calls_render_viz.rs:48-54` `node_available`
- `tests/dash_decisions_progressive_disclosure.rs:327-333` `node_available`
- `tests/dash_graph_exploration_viz.rs:42-48` `node_available`
- `tests/dash_kg_graph_route.rs:325-331` `node_available`
- `tests/dash_release_ready.rs:231-237` `node_available`
- `tests/files_lens_directory_hulls_viz.rs:49-55` `node_available`
- `tests/gitsemver_derivation.rs:83-89` `gitsemver_available`
- `tests/gitsemver_worktree_periphery.rs:117-123` `gitsemver_available`
- `tests/graph_collision_body_and_tiebreak.rs:44-50` `node_available`
- `tests/graph_density_spread_floor_and_centring.rs:50-56` `node_available`
- `tests/metadata_card_handoff_viz.rs:42-48` `node_available`
- `tests/native_driver_pipelining_behavior.rs:45-51` `node_available`
- `tests/proof_row_renders_on_the_card.rs:33-39` `node_available`
- `tests/readable_graph_adaptive_labels.rs:59-65` `node_available`
- `tests/readable_graph_density_scaled_spacing.rs:50-56` `node_available`
- `tests/readable_graph_layout_separation.rs:44-50` `node_available`
- `tests/subject_lens_overlay_client_arms.rs:43-49` `node_available`
- `tests/subject_lens_overlay_served_page.rs:48-54` `node_available`
- `tests/subject_view_memory_rail_client.rs:34-40` `node_available`
- `tests/validate_behind_the_tree_periphery.rs:139-145` `gitsemver_available`

#### `dup-0261` (near, 5 sites)

Proposed home: `a new shared module (sites span 5 files: src/main.rs, tests/build_watch_paths.rs, tests/gitsemver_derivation.rs, tests/gitsemver_worktree_periphery.rs, tests/validate_behind_the_tree_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:16446-16461` `behind_the_tree_git`
- `tests/build_watch_paths.rs:42-53` `git`
- `tests/gitsemver_derivation.rs:43-54` `git`
- `tests/gitsemver_worktree_periphery.rs:56-67` `git`
- `tests/validate_behind_the_tree_periphery.rs:94-109` `git`

#### `dup-0262` (near, 4 sites)

Proposed home: `a new shared module (sites span 4 files: src/main.rs, tests/build_watch_paths.rs, tests/gitsemver_worktree_periphery.rs, tests/validate_behind_the_tree_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:16463-16475` `behind_the_tree_git_output`
- `tests/build_watch_paths.rs:59-74` `git_output`
- `tests/gitsemver_worktree_periphery.rs:75-90` `git_output`
- `tests/validate_behind_the_tree_periphery.rs:113-134` `git_output`

#### `dup-0263` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/main.rs, tests/reset_build_cache_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:16585-16588` `write_file`
- `tests/reset_build_cache_periphery.rs:71-74` `write_file`

#### `dup-0264` (near, 3 sites)

Proposed home: `main::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:16685-16702` `refuse_when_base_lacks_spec_paths_proceeds_when_the_base_contains_them`
- `src/main.rs:16705-16723` `refuse_when_base_lacks_spec_paths_partial_match_warns_and_proceeds`
- `src/main.rs:16726-16766` `refuse_when_base_lacks_spec_paths_skips_without_tokens_or_off_a_fresh_from_base_anchor`

#### `dup-0265` (exact, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:17886-17894` `footprint_advisories_is_silent_below_the_threshold`
- `src/main.rs:17930-17938` `footprint_advisories_is_silent_on_an_empty_category`

#### `dup-0266` (near, 3 sites)

Proposed home: `main::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:17941-17964` `owning_repo_root_prefers_the_stores_own_root_over_the_git_toplevel`
- `src/main.rs:18335-18347` `find_store_dir_from_walks_up_from_a_subdirectory`
- `src/main.rs:18579-18608` `find_store_dir_from_walks_past_a_storeless_rigger_to_the_real_store_above`

#### `dup-0267` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/main.rs, src/reap.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:18295-18302` `leaked_process_advisories_is_a_graceful_no_op_when_the_scratch_root_is_absent`
- `src/reap.rs:630-637` `processes_rooted_under_is_a_graceful_no_op_when_the_base_is_absent`

#### `dup-0268` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:18611-18664` `find_store_dir_from_resolves_the_owning_repo_even_when_the_worktree_lives_outside_it`
- `src/main.rs:18667-18721` `find_store_dir_from_never_climbs_a_relocated_worktrees_own_unrelated_ancestors_into_a_foreign_store`

#### `dup-0269` (exact, 3 sites)

Proposed home: `main::restore`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:18860-18862` `drop`
- `src/main.rs:24135-24137` `drop`
- `src/main.rs:24162-24164` `drop`

#### `dup-0270` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:19121-19159` `a_recorded_result_lets_the_replay_driver_advance_past_the_spawn`
- `src/main.rs:19162-19194` `a_recorded_error_result_replays_as_a_failure_not_a_fake_success`

#### `dup-0271` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:19492-19497` `parse_run_args_rejects_unknown_flags_and_values`
- `src/main.rs:25281-25285` `parse_watch_args_rejects_a_non_integer_interval_a_missing_value_and_an_unknown_flag`

#### `dup-0272` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:20177-20242` `migrate_project_identity_rekeys_graph_rows_so_pre_mint_history_is_not_orphaned`
- `src/main.rs:20328-20423` `migrate_project_identity_recovers_from_a_crash_between_the_rekey_and_the_rename`

#### `dup-0273` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:21169-21248` `init_project_gitignores_the_dash_runtime_breadcrumbs_idempotently`
- `src/main.rs:21257-21297` `init_project_gitignores_the_store_conn_secret_file_idempotently`

#### `dup-0274` (near, 4 sites)

Proposed home: `main::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:21702-21715` `import_agents_validates_and_rejects_a_malformed_agent`
- `src/main.rs:21724-21746` `import_agents_rejects_an_id_colliding_with_an_existing_agent`
- `src/main.rs:21752-21772` `import_agents_rejects_a_duplicate_id_within_one_import`
- `src/main.rs:21778-21799` `import_agents_rejects_an_agent_with_a_blank_id`

#### `dup-0275` (exact, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:22137-22148` `the_step_schema_admits_the_attention_array`
- `src/main.rs:24210-24213` `no_runs_message_points_at_rigger_run`

#### `dup-0276` (exact, 6 sites)

Proposed home: `a new shared module (sites span 6 files: src/main.rs, tests/meta_phases_declaration_periphery.rs, tests/phase_of_role_mapping_periphery.rs, tests/review_tier_roster_periphery.rs, tests/step_attention_periphery.rs, tests/worker_persona_label_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:22154-22176` `js_function_body`
- `tests/meta_phases_declaration_periphery.rs:65-87` `js_declaration`
- `tests/phase_of_role_mapping_periphery.rs:50-72` `js_declaration`
- `tests/review_tier_roster_periphery.rs:18-40` `js_declaration`
- `tests/step_attention_periphery.rs:655-677` `js_declaration`
- `tests/worker_persona_label_periphery.rs:21-43` `js_declaration`

#### `dup-0277` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:22955-22972` `format_canary_stats_reports_findings_raised_by_tier`
- `src/main.rs:22977-22987` `format_canary_stats_reports_a_zero_findings_count_honestly`

#### `dup-0278` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:22993-22999` `format_canary_stats_omits_the_findings_volume_section_when_empty`
- `src/main.rs:23221-23227` `format_canary_stats_omits_the_model_pinning_header_when_the_run_never_recorded_one`

#### `dup-0279` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:23137-23156` `format_canary_stats_reports_control_items_and_false_positives`
- `src/main.rs:23164-23179` `format_canary_stats_reports_zero_false_positives_honestly`

#### `dup-0280` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:23436-23481` `stats_discloses_when_no_verdict_was_recorded_on_this_driver`
- `src/main.rs:23492-23565` `stats_discloses_unfed_numerator_when_verdict_recorded_but_findings_unattributed`

#### `dup-0281` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:24232-24244` `stats_lines_absent_db_returns_none_and_creates_no_file`
- `src/main.rs:24442-24454` `result_of_at_absent_db_reads_as_unreported_and_creates_no_file`

#### `dup-0282` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:24460-24480` `result_of_at_unrecorded_spawn_reads_as_unreported`
- `src/main.rs:24518-24547` `result_of_at_is_namespace_scoped`

#### `dup-0283` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/main.rs, tests/cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:24577-24590` `pgid_of`
- `tests/cli.rs:24582-24595` `proc_pgid_of`

#### `dup-0284` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:25051-25072` `reset_modes_parses_force_live_alongside_derived_rejects_duplicates_and_never_implies_a_mode`
- `src/main.rs:25079-25096` `reset_modes_parses_build_cache_alone_and_composed_and_rejects_duplicates`

#### `dup-0285` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:25613-25706` `implementer_persona_pins_the_checkin_stage_kill_or_justify_contract`
- `src/main.rs:25714-25756` `implementer_persona_pins_the_checkpoint_before_long_work_contract`

#### `dup-0286` (near, 5 sites)

Proposed home: `main::support (consolidate these 5 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:25980-25994` `grep_guard_decision_allows_non_grep_bash_commands`
- `src/main.rs:26018-26023` `grep_guard_decision_ignores_other_tools`
- `src/main.rs:26291-26301` `grep_guard_decision_allows_a_path_qualified_non_grep_command`
- `src/main.rs:26337-26352` `grep_guard_decision_allows_a_relative_sibling_reached_via_dot_dot`
- `src/main.rs:26432-26438` `grep_guard_decision_with_unknown_project_root_falls_back_to_segment_matching`

#### `dup-0287` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:26055-26083` `grep_guard_decision_bounces_an_absolute_path_under_a_guarded_tree`
- `src/main.rs:26231-26241` `grep_guard_decision_bounces_a_grep_split_by_a_line_continuation`

#### `dup-0288` (near, 4 sites)

Proposed home: `main::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:26109-26125` `grep_guard_decision_bounces_a_shell_metacharacter_fused_grep`
- `src/main.rs:26148-26161` `grep_guard_decision_bounces_a_redirect_metacharacter_fused_grep`
- `src/main.rs:26188-26201` `grep_guard_decision_bounces_a_quoted_or_escaped_grep`
- `src/main.rs:26260-26272` `grep_guard_decision_bounces_a_path_qualified_grep`

#### `dup-0289` (exact, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:26131-26138` `grep_guard_decision_still_allows_literal_on_a_shell_metacharacter_fused_grep`
- `src/main.rs:26245-26252` `grep_guard_decision_still_allows_literal_on_a_line_continuation_split_grep`

#### `dup-0290` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:26363-26382` `grep_guard_decision_bounces_the_absolute_project_root_itself`
- `src/main.rs:26388-26398` `grep_guard_decision_bounces_an_absolute_ancestor_of_the_project_root`

#### `dup-0291` (exact, 2 sites)

Proposed home: `mcpserver::tool_error`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/mcpserver.rs:30-35` `internal`
- `src/mcpserver.rs:39-44` `invalid_params`

#### `dup-0292` (semantic, 4 sites)

Proposed home: `mcpserver::tool_error - one canonical constructor, the rest thin variants over it (or a builder), rather than each re-listing every field`

mandatory sweep: parallel constructor functions - 4 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/mcpserver.rs:30-35` `internal`
- `src/mcpserver.rs:39-44` `invalid_params`
- `src/mcpserver.rs:48-50` `from`
- `src/mcpserver.rs:54-56` `from`

#### `dup-0293` (exact, 2 sites)

Proposed home: `mcpserver::server`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/mcpserver.rs:149-152` `with_graph`
- `src/mcpserver.rs:161-164` `with_grounder`

#### `dup-0294` (near, 2 sites)

Proposed home: `mcpserver::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/mcpserver.rs:1288-1306` `emit_tool_carries_meta_actor`
- `src/mcpserver.rs:1309-1331` `emit_tool_sets_valid_from_from_nanos`

#### `dup-0295` (near, 2 sites)

Proposed home: `mcpserver::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/mcpserver.rs:1345-1383` `peers_tool_scopes_to_the_files_arg`
- `src/mcpserver.rs:1386-1431` `peers_tool_surfaces_findings_scoped_to_the_files_arg`

#### `dup-0296` (near, 5 sites)

Proposed home: `mcpserver::support (consolidate these 5 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/mcpserver.rs:1483-1502` `rigger_result_for_an_unknown_id_is_an_error`
- `src/mcpserver.rs:1505-1527` `malformed_json_gets_a_parse_error`
- `src/mcpserver.rs:1530-1548` `request_missing_method_gets_an_invalid_request_error`
- `src/mcpserver.rs:1551-1569` `tools_call_missing_name_gets_an_invalid_params_error`
- `src/mcpserver.rs:1701-1720` `workflow_surface_rejects_ground_and_graph_as_unknown_tools`

#### `dup-0297` (exact, 7 sites)

Proposed home: `metrics::support (consolidate these 7 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/metrics.rs:341-343` `survival`
- `src/metrics.rs:435-437` `lens_overlap_rate`
- `src/metrics.rs:449-451` `first_pass_yield`
- `src/metrics.rs:455-457` `escalation_rate`
- `src/metrics.rs:1087-1089` `rate`
- `src/metrics.rs:1152-1154` `adjudicator_accuracy`
- `src/metrics.rs:1158-1160` `stability_rate`

#### `dup-0298` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/metrics.rs, src/run.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/metrics.rs:1009-1011` `gate_verdict`
- `src/run.rs:117-119` `from_event`

#### `dup-0299` (semantic, 2 sites)

Proposed home: `one shared `gate_verdict` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/metrics.rs:1009-1011` `gate_verdict`
- `tests/dash_run_tree_spine.rs:80-86` `gate_verdict`

#### `dup-0300` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/metrics.rs, src/spawn.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/metrics.rs:1323-1325` `changed`
- `src/spawn.rs:574-576` `is_error`

#### `dup-0301` (exact, 3 sites)

Proposed home: `metrics::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/metrics.rs:1404-1409` `started`
- `src/metrics.rs:1411-1416` `status`
- `src/metrics.rs:1443-1448` `artifact_verdict`

#### `dup-0302` (near, 6 sites)

Proposed home: `a new shared module (sites span 2 files: src/metrics.rs, src/run.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/metrics.rs:1418-1423` `failed`
- `src/metrics.rs:1425-1430` `integrated`
- `src/metrics.rs:1432-1434` `escalated`
- `src/run.rs:563-565` `decision`
- `src/run.rs:566-568` `finding`
- `src/run.rs:569-571` `lesson`

#### `dup-0303` (near, 5 sites)

Proposed home: `metrics::support (consolidate these 5 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/metrics.rs:1738-1751` `counts_review_rejects_on_both_per_unit_and_fan_out_paths`
- `src/metrics.rs:1806-1818` `fan_out_reject_then_approve_counts_one_each`
- `src/metrics.rs:1866-1879` `duplicate_unit_started_counts_the_unit_once`
- `src/metrics.rs:1882-1899` `interleaved_units_keep_per_id_review_state`
- `src/metrics.rs:1902-1913` `escalation_is_counted_once_per_unit`

#### `dup-0304` (near, 2 sites)

Proposed home: `metrics::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/metrics.rs:1970-1981` `finding`
- `src/metrics.rs:1986-1996` `courier_finding`

#### `dup-0305` (near, 2 sites)

Proposed home: `metrics::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/metrics.rs:2134-2154` `spawn_timing_never_pairs_a_cross_run_id_collision`
- `src/metrics.rs:2251-2266` `spawn_timing_excludes_a_same_batch_zero_duration_pair_as_suspect`

#### `dup-0306` (near, 2 sites)

Proposed home: `metrics::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/metrics.rs:2309-2332` `finding_survival_is_upheld_over_raised_per_actor`
- `src/metrics.rs:2404-2432` `cost_per_upheld_finding_is_spawns_over_upheld_per_tier`

#### `dup-0307` (near, 2 sites)

Proposed home: `metrics::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/metrics.rs:2853-2872` `project_canary_counts_correctly_rejected_items_with_empty_attribution`
- `src/metrics.rs:2881-2899` `project_canary_counts_controls_and_false_positives`

#### `dup-0308` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/progress.rs, tests/reset_derived_compaction.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/progress.rs:197-200` `event_unit_id`
- `tests/reset_derived_compaction.rs:161-164` `replay_key`

#### `dup-0309` (near, 6 sites)

Proposed home: `a new shared module (sites span 5 files: src/reap.rs, tests/mutation_scratch_reap_base_guard_periphery.rs, tests/reap_before_removal_periphery.rs, tests/spawn_scratch_reap_authorized_root_periphery.rs, tests/worktree_remove_relocated_scratch_base_guard_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/reap.rs:532-538` `sleeper_in`
- `src/reap.rs:542-549` `sigterm_ignorer_in`
- `tests/mutation_scratch_reap_base_guard_periphery.rs:54-61` `sigterm_ignorer_in`
- `tests/reap_before_removal_periphery.rs:41-48` `sigterm_ignorer_in`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:138-145` `sigterm_ignorer_in`
- `tests/worktree_remove_relocated_scratch_base_guard_periphery.rs:55-62` `sigterm_ignorer_in`

#### `dup-0310` (exact, 4 sites)

Proposed home: `a new shared module (sites span 4 files: src/reap.rs, tests/mutation_scratch_reap_base_guard_periphery.rs, tests/no_os_kill_test_helper_periphery.rs, tests/spawn_scratch_reap_authorized_root_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/reap.rs:570-573` `cleanup`
- `tests/mutation_scratch_reap_base_guard_periphery.rs:77-80` `cleanup`
- `tests/no_os_kill_test_helper_periphery.rs:84-87` `cleanup`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:161-164` `cleanup`

#### `dup-0311` (exact, 2 sites)

Proposed home: `reap::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/reap.rs:715-720` `is_reapable_base_refuses_a_dir_that_is_not_under_the_given_authorized_root`
- `src/reap.rs:723-726` `is_reapable_base_refuses_the_authorized_root_itself`

#### `dup-0312` (near, 2 sites)

Proposed home: `reap::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/reap.rs:1071-1097` `signal_if_unchanged_skips_a_starttime_mismatch`
- `src/reap.rs:1100-1127` `signal_if_unchanged_skips_when_cwd_is_outside_the_given_base`

#### `dup-0313` (near, 13 sites)

Proposed home: `a new shared module (sites span 11 files: src/registry.rs, tests/reset_build_cache_periphery.rs, tests/reset_derived_compaction.rs, tests/reset_derived_compaction_periphery.rs, tests/reset_menu.rs, tests/reset_menu_identity_migration_periphery.rs, tests/store_flag_precedence.rs, tests/store_precedence.rs, tests/store_resolution_cli.rs, tests/store_secrets.rs, tests/validate_advisories.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/registry.rs:133-135` `instances_dir`
- `tests/reset_build_cache_periphery.rs:45-47` `event_log`
- `tests/reset_derived_compaction.rs:75-77` `event_log`
- `tests/reset_derived_compaction_periphery.rs:610-612` `event_log`
- `tests/reset_derived_compaction_periphery.rs:1366-1368` `graph_db`
- `tests/reset_menu.rs:70-72` `event_log`
- `tests/reset_menu.rs:74-76` `graph_db`
- `tests/reset_menu_identity_migration_periphery.rs:65-67` `event_log`
- `tests/store_flag_precedence.rs:94-96` `local_event_log`
- `tests/store_precedence.rs:59-61` `local_event_log`
- `tests/store_resolution_cli.rs:66-68` `local_event_log`
- `tests/store_secrets.rs:62-64` `local_event_log`
- `tests/validate_advisories.rs:81-83` `event_log`

#### `dup-0314` (near, 3 sites)

Proposed home: `registry::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/registry.rs:299-313` `write_then_read_round_trips_a_live_entry`
- `src/registry.rs:411-423` `read_live_no_prune_still_returns_a_fresh_entry`
- `src/registry.rs:426-447` `read_all_returns_a_stale_entry_read_live_would_have_pruned_and_never_deletes_it`

#### `dup-0315` (near, 2 sites)

Proposed home: `registry::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/registry.rs:335-346` `two_projects_get_distinct_entries`
- `src/registry.rs:450-462` `read_all_returns_every_registered_root_regardless_of_freshness`

#### `dup-0316` (near, 2 sites)

Proposed home: `registry::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/registry.rs:349-362` `a_reader_prunes_a_stale_heartbeat`
- `src/registry.rs:383-408` `read_live_no_prune_filters_a_stale_heartbeat_but_never_deletes_its_file`

#### `dup-0317` (near, 3 sites)

Proposed home: `run::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/run.rs:701-767` `the_resolved_base_is_persisted_on_the_run_start_and_survives_adopt`
- `src/run.rs:770-841` `the_base_tip_is_persisted_on_the_run_start_and_survives_adopt`
- `src/run.rs:844-909` `the_spec_path_is_persisted_on_the_run_start_and_survives_adopt`

#### `dup-0318` (near, 2 sites)

Proposed home: `run::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/run.rs:935-952` `ensure_started_adopts_the_same_criteria_run_without_re_minting`
- `src/run.rs:1010-1028` `ensure_started_mints_a_new_run_when_the_criteria_change`

#### `dup-0319` (exact, 3 sites)

Proposed home: `sidecar::sidecar`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/sidecar.rs:137-147` `decisions_for`
- `src/sidecar.rs:168-178` `findings_for`
- `src/sidecar.rs:196-206` `lessons_for`

#### `dup-0320` (exact, 2 sites)

Proposed home: `sidecar::sidecar`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/sidecar.rs:155-161` `findings`
- `src/sidecar.rs:184-190` `lessons`

#### `dup-0321` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/sidecar.rs, tests/store_content_identity_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/sidecar.rs:209-211` `len`
- `tests/store_content_identity_periphery.rs:87-89` `batch_calls`

#### `dup-0322` (near, 3 sites)

Proposed home: `sidecar::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/sidecar.rs:268-308` `decisions_for_scopes_to_the_blast_radius`
- `src/sidecar.rs:311-359` `findings_for_scopes_to_the_blast_radius`
- `src/sidecar.rs:362-413` `lessons_for_scopes_to_the_blast_radius`

#### `dup-0323` (exact, 4 sites)

Proposed home: `spawn::spawn_request`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spawn.rs:338-341` `with_system_prompt`
- `src/spawn.rs:344-347` `with_model`
- `src/spawn.rs:356-359` `with_dir`
- `src/spawn.rs:368-371` `with_title`

#### `dup-0324` (exact, 3 sites)

Proposed home: `spawn::spawn_request`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spawn.rs:350-353` `with_tools`
- `src/spawn.rs:362-365` `with_blast_radius`
- `src/spawn.rs:374-377` `with_reviews`

#### `dup-0325` (exact, 2 sites)

Proposed home: `spawn::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spawn.rs:381-383` `to_event`
- `src/spawn.rs:663-665` `to_event`

#### `dup-0326` (exact, 2 sites)

Proposed home: `spawn::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spawn.rs:386-388` `from_event`
- `src/spawn.rs:668-670` `from_event`

#### `dup-0327` (near, 2 sites)

Proposed home: `spawn::spawn_result`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spawn.rs:527-534` `ok`
- `src/spawn.rs:539-546` `failed`

#### `dup-0328` (semantic, 3 sites)

Proposed home: `spawn::spawn_result - one canonical constructor, the rest thin variants over it (or a builder), rather than each re-listing every field`

mandatory sweep: parallel constructor functions - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/spawn.rs:527-534` `ok`
- `src/spawn.rs:539-546` `failed`
- `src/spawn.rs:554-565` `liveness_fault`

#### `dup-0329` (semantic, 2 sites)

Proposed home: `one shared `liveness_fault` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/spawn.rs:554-565` `liveness_fault`
- `tests/dash_run_tree_spine.rs:90-94` `liveness_fault`

#### `dup-0330` (exact, 2 sites)

Proposed home: `spawn::spawn_result`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spawn.rs:587-593` `liveness_class`
- `src/spawn.rs:600-606` `resolved_model`

#### `dup-0331` (near, 2 sites)

Proposed home: `spawn::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spawn.rs:1417-1451` `record_result_if_absent_is_a_noop_that_never_clobbers_an_existing_result`
- `src/spawn.rs:1566-1603` `record_result_if_absent_honors_a_self_report_that_won_the_race`

#### `dup-0332` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/spawn.rs, tests/adoption_keys_on_criterion_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spawn.rs:1501-1508` `read_all`
- `tests/adoption_keys_on_criterion_periphery.rs:2645-2652` `read_all`

#### `dup-0333` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/spawn.rs, tests/adoption_keys_on_criterion_periphery.rs, tests/integrate_conflict_merge_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spawn.rs:1510-1512` `subscribe_all`
- `tests/adoption_keys_on_criterion_periphery.rs:2654-2656` `subscribe_all`
- `tests/integrate_conflict_merge_periphery.rs:2099-2101` `subscribe_all`

#### `dup-0334` (near, 2 sites)

Proposed home: `spawn::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spawn.rs:1735-1768` `step_wave_is_the_full_pending_frontier_never_answered_spawns`
- `src/spawn.rs:2022-2054` `step_rerun_reprints_unanswered_spawns_so_a_killed_step_orphans_nothing`

#### `dup-0335` (near, 2 sites)

Proposed home: `spawn::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spawn.rs:1797-1824` `spawn_request_carries_a_title_via_builder_and_omits_it_when_empty`
- `src/spawn.rs:1827-1857` `wave_item_copies_the_request_title_so_the_thin_driver_renders_the_work`

#### `dup-0336` (near, 2 sites)

Proposed home: `spawn::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spawn.rs:1921-1961` `spawn_request_carries_a_reviews_roster_via_builder_and_omits_it_when_empty`
- `src/spawn.rs:1964-1999` `wave_item_copies_the_request_reviews_roster`

#### `dup-0337` (near, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:652-654` `find_word`
- `src/spec.rs:662-664` `find_word_across_hyphen`

#### `dup-0338` (exact, 3 sites)

Proposed home: `spec::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:911-925` `extract_criteria_joins_a_three_line_wrap_including_the_owns_sentence_on_line_three`
- `src/spec.rs:931-943` `extract_criteria_stops_a_wrap_at_the_next_checkbox_item_with_no_blank_line_between`
- `src/spec.rs:981-994` `extract_criteria_includes_a_nested_sub_bullet_as_part_of_the_criterion_text`

#### `dup-0339` (exact, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:949-959` `extract_criteria_stops_a_wrap_at_a_blank_line_and_excludes_the_prose_after_it`
- `src/spec.rs:964-974` `extract_criteria_stops_a_wrap_at_a_following_heading`

#### `dup-0340` (exact, 17 sites)

Proposed home: `spec::support (consolidate these 17 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:999-1009` `clean_single_behavior_spec_is_silent`
- `src/spec.rs:1260-1269` `ownership_check_is_silent_below_three_criteria`
- `src/spec.rs:1611-1619` `disposition_check_does_not_match_either_inside_neither`
- `src/spec.rs:1626-1635` `disposition_check_does_not_match_or_inside_a_later_word`
- `src/spec.rs:1643-1653` `disposition_check_does_not_match_worth_considering_across_a_hyphenated_compound`
- `src/spec.rs:1660-1669` `disposition_check_ignores_a_phrase_named_in_double_quotes`
- `src/spec.rs:1695-1707` `disposition_check_does_not_match_a_satisfied_either_or_decided_disposition`
- `src/spec.rs:1752-1761` `disposition_check_does_not_pair_a_non_disjunctive_either_with_a_faraway_unrelated_or`
- `src/spec.rs:1797-1805` `disposition_check_ignores_a_wide_double_quoted_span`
- `src/spec.rs:1814-1824` `disposition_check_ignores_a_double_quoted_span_that_crosses_a_line_wrap`
- `src/spec.rs:1948-1960` `disposition_check_a_stray_unmatched_quote_does_not_unmask_a_later_real_quoted_phrase`
- `src/spec.rs:1982-1994` `disposition_check_a_real_quoted_phrase_whose_closing_quote_follows_a_digit_stays_exempt`
- `src/spec.rs:2006-2017` `disposition_check_digit_adjacent_quote_still_excluded_as_opener`
- `src/spec.rs:2033-2044` `disposition_check_a_quote_at_the_very_start_of_a_paragraph_is_a_valid_opener`
- `src/spec.rs:2049-2059` `disposition_check_is_silent_inside_notes`
- `src/spec.rs:2078-2087` `disposition_check_ignores_fenced_and_inline_code`
- `src/spec.rs:2249-2262` `spec_lint_advisories_is_silent_on_a_clean_fixture`

#### `dup-0341` (near, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:1041-1050` `a_single_coordinator_does_not_flag_multi_behavior`
- `src/spec.rs:1181-1194` `spec_shape_advisories_ignores_coordinators_added_by_continuation_lines`

#### `dup-0342` (near, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:1197-1206` `path_tokens_extracts_relative_file_paths_and_trims_markdown`
- `src/spec.rs:1222-1228` `path_tokens_dedupes_and_preserves_first_seen_order`

#### `dup-0343` (exact, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:1209-1219` `path_tokens_ignores_prose_flags_versions_types_and_urls`
- `src/spec.rs:1237-1248` `path_tokens_requires_an_alphabetic_extension_and_a_separator`

#### `dup-0344` (near, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:1296-1307` `ownership_check_accepts_the_word_owner`
- `src/spec.rs:1316-1332` `ownership_check_finds_an_owns_sentence_on_a_wrapped_continuation_line`

#### `dup-0345` (near, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:1351-1375` `ownership_check_finds_an_owns_sentence_on_an_unindented_continuation_line`
- `src/spec.rs:1384-1406` `ownership_check_does_not_reattach_prose_after_a_blank_line_to_the_prior_criterion`

#### `dup-0346` (exact, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:1414-1430` `ownership_check_does_not_let_a_dropped_word_boundary_fake_an_owns_sentence`
- `src/spec.rs:1442-1458` `ownership_check_does_not_let_a_dropped_word_boundary_weld_own_and_er_into_owner`

#### `dup-0347` (near, 4 sites)

Proposed home: `spec::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:1484-1497` `ownership_check_flags_ownerless_and_not_owned_as_denials`
- `src/spec.rs:1507-1526` `ownership_check_does_not_match_owns_or_owner_inside_an_unrelated_word`
- `src/spec.rs:1537-1559` `ownership_check_lets_an_affirmative_owns_win_over_an_unrelated_denial_elsewhere`
- `src/spec.rs:2117-2127` `starts_new_element_recognizes_every_prefix_kind_independently`

#### `dup-0348` (exact, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:1733-1742` `disposition_check_does_not_exempt_unsatisfied_either_or`
- `src/spec.rs:1781-1790` `disposition_check_still_flags_a_bare_either_or_with_no_satisfied_word`

#### `dup-0349` (exact, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:1767-1776` `disposition_check_finds_a_genuine_hedge_after_an_earlier_non_disjunctive_either`
- `src/spec.rs:1853-1864` `disposition_check_still_fires_outside_a_balanced_quote_pair`

#### `dup-0350` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/spec.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:1835-1847` `disposition_check_fails_closed_after_a_stray_unmatched_quote_earlier_in_the_paragraph`
- `tests/simplification_audit.rs:6529-6533` `a_trait_method_signature_without_a_body_is_not_recorded`

#### `dup-0351` (exact, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:2092-2102` `disposition_check_attributes_a_hit_inside_a_criterion`
- `src/spec.rs:2145-2155` `hygiene_check_attributes_a_hit_inside_a_criterion`

#### `dup-0352` (near, 4 sites)

Proposed home: `watch::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/watch.rs:673-679` `a_clean_store_detects_no_anomalies`
- `src/watch.rs:737-751` `two_failures_below_threshold_is_not_reported`
- `src/watch.rs:754-775` `a_cause_change_resets_the_streak_so_three_failures_split_across_two_causes_do_not_alert`
- `src/watch.rs:825-831` `a_spawn_answered_twice_is_below_the_frontier_stall_threshold`

#### `dup-0353` (near, 2 sites)

Proposed home: `watch::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/watch.rs:708-734` `a_unit_at_reject_recurrence_three_same_cause_is_reported`
- `src/watch.rs:797-822` `a_spawn_answered_three_times_is_reported_as_a_frontier_stall`

#### `dup-0354` (near, 3 sites)

Proposed home: `watch::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/watch.rs:881-904` `a_fresh_heartbeat_suppresses_the_dead_driver_alert_even_with_a_quiet_store`
- `src/watch.rs:907-935` `a_heartbeat_ten_minutes_stale_does_not_cross_the_thirty_minute_bound`
- `src/watch.rs:938-969` `a_heartbeat_exactly_thirty_minutes_stale_does_not_yet_cross_the_bound`

#### `dup-0355` (near, 3 sites)

Proposed home: `watch::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/watch.rs:1028-1051` `a_dash_marker_naming_a_dead_pid_is_reported`
- `src/watch.rs:1061-1089` `a_dead_dash_url_with_no_marker_is_reported_without_inventing_a_pid`
- `src/watch.rs:1173-1201` `dash_attempted_this_run_overrides_a_breadcrumb_that_looks_like_it_predates_the_run`

#### `dup-0356` (exact, 2 sites)

Proposed home: `watch::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/watch.rs:1092-1106` `no_dash_ever_recorded_is_not_an_anomaly`
- `src/watch.rs:1109-1123` `a_serving_dash_is_not_an_anomaly`

#### `dup-0357` (near, 2 sites)

Proposed home: `watch::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/watch.rs:1452-1464` `dedup_suppresses_a_persisting_anomaly_at_the_same_magnitude`
- `src/watch.rs:1483-1496` `dedup_re_alerts_a_cleared_and_later_recurring_anomaly`

#### `dup-0358` (exact, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:1299-1310` `branch_exists`
- `src/worktree.rs:1333-1344` `ref_resolves`

#### `dup-0359` (semantic, 2 sites)

Proposed home: `one shared `branch_exists` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/worktree.rs:1299-1310` `branch_exists`
- `tests/step_root_resolution_periphery.rs:234-241` `branch_exists`

#### `dup-0360` (semantic, 2 sites)

Proposed home: `one shared `current_branch` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/worktree.rs:1362-1367` `current_branch`
- `tests/step_root_resolution_periphery.rs:221-232` `current_branch`

#### `dup-0361` (exact, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:1455-1458` `scratch_root_from_env`
- `src/worktree.rs:1462-1465` `scratch_root_path_from_env`

#### `dup-0362` (exact, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:1515-1523` `unit_cache_sibling`
- `src/worktree.rs:1543-1551` `unit_mutants_sibling`

#### `dup-0363` (exact, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:2146-2153` `head_sha_of`
- `src/worktree.rs:2168-2175` `tree_sha_of`

#### `dup-0364` (near, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:2309-2329` `integrate_lands_work_in_the_repo`
- `src/worktree.rs:3902-3935` `commit_cleans_the_tree_so_a_gate_sees_the_committed_artifact`

#### `dup-0365` (near, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:3037-3137` `cherry_pick_onto_run_branch_self_heals_a_leftover_marker_from_a_crash_mid_skip_loop`
- `src/worktree.rs:3140-3249` `cherry_pick_onto_run_branch_self_heals_a_leftover_marker_ahead_of_two_chained_empty_commits`

#### `dup-0366` (near, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:4006-4033` `changed_files_reports_only_the_rename_destination`
- `src/worktree.rs:6002-6014` `changed_files_unquotes_paths_with_spaces`

#### `dup-0367` (near, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:4241-4279` `sweep_terminal_removes_merged_worktrees_and_keeps_inflight_ones`
- `src/worktree.rs:4849-4897` `sweep_terminal_reclaims_a_crash_left_terminal_units_per_unit_build_cache`

#### `dup-0368` (near, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:4553-4557` `requested_and_answered`
- `src/worktree.rs:4561-4565` `requested_and_hung`

#### `dup-0369` (near, 4 sites)

Proposed home: `worktree::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:4568-4601` `sweep_terminal_spares_a_merged_branch_whose_units_latest_spawn_is_still_in_flight`
- `src/worktree.rs:4604-4630` `sweep_terminal_reclaims_a_merged_branch_once_its_latest_spawn_has_a_real_result`
- `src/worktree.rs:4633-4657` `sweep_terminal_reclaims_a_merged_branch_whose_latest_spawn_is_hung`
- `src/worktree.rs:4660-4683` `sweep_terminal_reclaims_a_merged_branch_with_no_spawn_recorded_at_all_unchanged`

#### `dup-0370` (near, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:4769-4809` `sweep_terminal_prints_evidence_for_a_kept_decision_but_not_for_a_removed_no_spawn_one`
- `src/worktree.rs:4812-4846` `sweep_terminal_prints_evidence_for_a_removed_terminal_spawn_decision`

#### `dup-0371` (near, 4 sites)

Proposed home: `worktree::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:4970-5011` `worktree_remove_reclaims_the_sibling_per_unit_cache`
- `src/worktree.rs:5014-5056` `worktree_remove_also_reclaims_the_sibling_mutants_root`
- `src/worktree.rs:5059-5094` `worktree_remove_also_reclaims_the_store_fence_sibling`
- `src/worktree.rs:5114-5143` `worktree_remove_also_reclaims_a_review_worktrees_store_fence_sibling`

#### `dup-0372` (near, 3 sites)

Proposed home: `worktree::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:5097-5111` `review_fence_sibling_maps_a_review_worktree_to_its_fence_sibling_and_ignores_the_rest`
- `src/worktree.rs:5425-5438` `unit_cache_sibling_maps_a_unit_worktree_to_its_cache_and_ignores_the_rest`
- `src/worktree.rs:5441-5453` `unit_mutants_sibling_maps_a_unit_worktree_to_its_mutants_root_and_ignores_the_rest`

#### `dup-0373` (near, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:5154-5215` `reclaim_worktree_on_branch_deregisters_the_lingering_worktree_reclaims_its_cache_and_frees_the_branch`
- `src/worktree.rs:5372-5422` `reclaim_worktree_on_branch_prunes_a_stale_registration_whose_dir_was_deleted_and_frees_the_branch`

#### `dup-0374` (near, 3 sites)

Proposed home: `worktree::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:6103-6179` `create_self_heals_a_corrupt_worktree_admin_entry_and_spares_healthy_ones`
- `src/worktree.rs:6182-6253` `create_heals_a_zero_length_gitdir_marker_the_commondir_case_leaves_untested`
- `src/worktree.rs:6256-6319` `create_heals_a_fully_missing_marker_not_just_a_truncated_one`

#### `dup-0375` (exact, 19 sites)

Proposed home: `a new shared module (sites span 19 files: tests/adaptive_labels_periphery.rs, tests/code_lens_overview_collapse_viz.rs, tests/concepts_lens_view_periphery.rs, tests/dash_calls_render_viz.rs, tests/dash_decisions_progressive_disclosure.rs, tests/dash_graph_exploration_viz.rs, tests/dash_kg_graph_route.rs, tests/dash_release_ready.rs, tests/files_lens_directory_hulls_viz.rs, tests/graph_collision_body_and_tiebreak.rs, tests/graph_density_spread_floor_and_centring.rs, tests/metadata_card_handoff_viz.rs, tests/proof_row_renders_on_the_card.rs, tests/readable_graph_adaptive_labels.rs, tests/readable_graph_density_scaled_spacing.rs, tests/readable_graph_layout_separation.rs, tests/subject_lens_overlay_client_arms.rs, tests/subject_lens_overlay_served_page.rs, tests/subject_view_memory_rail_client.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/adaptive_labels_periphery.rs:52-61` `page_script`
- `tests/code_lens_overview_collapse_viz.rs:26-35` `page_script`
- `tests/concepts_lens_view_periphery.rs:689-698` `page_script`
- `tests/dash_calls_render_viz.rs:35-44` `page_script`
- `tests/dash_decisions_progressive_disclosure.rs:314-323` `page_script`
- `tests/dash_graph_exploration_viz.rs:29-38` `page_script`
- `tests/dash_kg_graph_route.rs:334-343` `page_script`
- `tests/dash_release_ready.rs:218-227` `page_script`
- `tests/files_lens_directory_hulls_viz.rs:36-45` `page_script`
- `tests/graph_collision_body_and_tiebreak.rs:31-40` `page_script`
- `tests/graph_density_spread_floor_and_centring.rs:37-46` `page_script`
- `tests/metadata_card_handoff_viz.rs:29-38` `page_script`
- `tests/proof_row_renders_on_the_card.rs:20-29` `page_script`
- `tests/readable_graph_adaptive_labels.rs:46-55` `page_script`
- `tests/readable_graph_density_scaled_spacing.rs:37-46` `page_script`
- `tests/readable_graph_layout_separation.rs:31-40` `page_script`
- `tests/subject_lens_overlay_client_arms.rs:30-39` `page_script`
- `tests/subject_lens_overlay_served_page.rs:35-44` `page_script`
- `tests/subject_view_memory_rail_client.rs:21-30` `page_script`

#### `dup-0376` (semantic, 20 sites)

Proposed home: `one shared `node_available` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 20 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/adaptive_labels_periphery.rs:65-71` `node_available`
- `tests/code_lens_overview_collapse_viz.rs:39-45` `node_available`
- `tests/concepts_lens_view_periphery.rs:702-708` `node_available`
- `tests/dash_calls_render_viz.rs:48-54` `node_available`
- `tests/dash_decisions_progressive_disclosure.rs:327-333` `node_available`
- `tests/dash_graph_exploration_viz.rs:42-48` `node_available`
- `tests/dash_kg_graph_route.rs:325-331` `node_available`
- `tests/dash_release_ready.rs:231-237` `node_available`
- `tests/files_lens_directory_hulls_viz.rs:49-55` `node_available`
- `tests/graph_collision_body_and_tiebreak.rs:44-50` `node_available`
- `tests/graph_density_spread_floor_and_centring.rs:50-56` `node_available`
- `tests/metadata_card_handoff_viz.rs:42-48` `node_available`
- `tests/native_driver_pipelining_behavior.rs:45-51` `node_available`
- `tests/proof_row_renders_on_the_card.rs:33-39` `node_available`
- `tests/readable_graph_adaptive_labels.rs:59-65` `node_available`
- `tests/readable_graph_density_scaled_spacing.rs:50-56` `node_available`
- `tests/readable_graph_layout_separation.rs:44-50` `node_available`
- `tests/subject_lens_overlay_client_arms.rs:43-49` `node_available`
- `tests/subject_lens_overlay_served_page.rs:48-54` `node_available`
- `tests/subject_view_memory_rail_client.rs:34-40` `node_available`

#### `dup-0377` (exact, 3 sites)

Proposed home: `adaptive_labels_periphery::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/adaptive_labels_periphery.rs:467-479` `the_live_zoom_handler_toggles_labels_to_match_the_visible_set`
- `tests/adaptive_labels_periphery.rs:484-496` `the_declutter_holds_its_contract_at_the_edges_and_across_scales`
- `tests/adaptive_labels_periphery.rs:502-514` `a_layered_view_stays_byte_identical_and_a_titled_node_still_names_itself_on_hover`

#### `dup-0378` (exact, 4 sites)

Proposed home: `a new shared module (sites span 3 files: tests/adaptive_labels_periphery.rs, tests/subject_lens_overlay_client_arms.rs, tests/subject_view_memory_rail_client.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/adaptive_labels_periphery.rs:522-534` `the_real_concepts_drill_names_its_decluttered_shared_member_on_hover`
- `tests/subject_lens_overlay_client_arms.rs:290-303` `a_lens_flip_with_no_subject_reloads_the_whole_graph_overview`
- `tests/subject_lens_overlay_client_arms.rs:310-323` `a_failed_live_reprojection_fetch_degrades_to_a_message`
- `tests/subject_view_memory_rail_client.rs:207-220` `clicking_a_node_reveals_the_memory_rail_without_touching_the_neighborhood_panel`

#### `dup-0379` (exact, 4 sites)

Proposed home: `a new shared module (sites span 4 files: tests/adoption_keys_on_criterion_periphery.rs, tests/reap_before_removal_periphery.rs, tests/worktree_liveness_fence_periphery.rs, tests/worktree_remove_relocated_scratch_base_guard_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/adoption_keys_on_criterion_periphery.rs:180-195` `init_repo`
- `tests/reap_before_removal_periphery.rs:62-77` `init_repo`
- `tests/worktree_liveness_fence_periphery.rs:120-135` `init_repo`
- `tests/worktree_remove_relocated_scratch_base_guard_periphery.rs:76-91` `init_repo`

#### `dup-0380` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/adoption_keys_on_criterion_periphery.rs, tests/cli.rs, tests/worktree_liveness_fence_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/adoption_keys_on_criterion_periphery.rs:198-207` `git_out`
- `tests/cli.rs:126-136` `git_out`
- `tests/worktree_liveness_fence_periphery.rs:149-159` `git_out`

#### `dup-0381` (near, 2 sites)

Proposed home: `adoption_keys_on_criterion_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/adoption_keys_on_criterion_periphery.rs:405-421` `find_unit_started`
- `tests/adoption_keys_on_criterion_periphery.rs:854-873` `find_unit_integrated_commit`

#### `dup-0382` (near, 2 sites)

Proposed home: `adoption_keys_on_criterion_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/adoption_keys_on_criterion_periphery.rs:882-1027` `spec_scoping_blocks_adoption_across_specs_sharing_a_criterion_id_but_not_across_two_runs_of_the_same_spec`
- `tests/adoption_keys_on_criterion_periphery.rs:1694-1834` `a_reused_planner_slug_never_replays_an_unrelated_specs_recorded_adoption_decision`

#### `dup-0383` (near, 2 sites)

Proposed home: `adoption_keys_on_criterion_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/adoption_keys_on_criterion_periphery.rs:1114-1198` `a_compensation_reverted_integration_reopens_adoption_of_its_real_still_existing_branch`
- `tests/adoption_keys_on_criterion_periphery.rs:1206-1276` `a_plain_remediation_failure_after_integration_never_reopens_adoption_even_though_a_same_named_branch_exists`

#### `dup-0384` (near, 2 sites)

Proposed home: `adoption_keys_on_criterion_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/adoption_keys_on_criterion_periphery.rs:1287-1418` `a_crash_after_the_branch_exists_but_before_unitstarted_lands_recovers_the_recorded_adoption`
- `tests/adoption_keys_on_criterion_periphery.rs:1425-1547` `a_crash_after_the_provenance_record_but_before_the_branch_is_created_still_completes_the_adoption_on_resume`

#### `dup-0385` (near, 2 sites)

Proposed home: `adoption_keys_on_criterion_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/adoption_keys_on_criterion_periphery.rs:1978-2096` `an_escalated_units_unreclaimed_branch_is_never_reused_by_an_unrelated_specs_slug_collision`
- `tests/adoption_keys_on_criterion_periphery.rs:2124-2263` `a_genuine_retry_of_a_quarantined_criterion_adopts_from_the_quarantine_ref`

#### `dup-0386` (exact, 10 sites)

Proposed home: `a new shared module (sites span 8 files: tests/architecture_current_surface.rs, tests/cli.rs, tests/hermetic_test_git_audit.rs, tests/meta_phases_declaration_periphery.rs, tests/phase_of_role_mapping_periphery.rs, tests/review_tier_roster_periphery.rs, tests/step_attention_periphery.rs, tests/worker_persona_label_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/architecture_current_surface.rs:49-54` `architecture_text`
- `tests/cli.rs:3349-3354` `main_rs_source`
- `tests/cli.rs:3886-3891` `rigger_js_source`
- `tests/cli.rs:25900-25905` `rigger_workflow_yml_text`
- `tests/hermetic_test_git_audit.rs:83-89` `runner_script_text`
- `tests/meta_phases_declaration_periphery.rs:54-59` `rigger_js_source`
- `tests/phase_of_role_mapping_periphery.rs:38-43` `rigger_js_source`
- `tests/review_tier_roster_periphery.rs:44-49` `rigger_js_source`
- `tests/step_attention_periphery.rs:964-969` `rigger_js_source`
- `tests/worker_persona_label_periphery.rs:48-53` `rigger_js_source`

#### `dup-0387` (semantic, 2 sites)

Proposed home: `one shared `architecture_text` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/architecture_current_surface.rs:49-54` `architecture_text`
- `tests/architecture_integrity.rs:50-53` `architecture_text`

#### `dup-0388` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/architecture_current_surface.rs, tests/kurrentdb_contract_test_surface.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/architecture_current_surface.rs:57-63` `eventstore_source`
- `tests/kurrentdb_contract_test_surface.rs:38-45` `adapter_source`

#### `dup-0389` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/architecture_current_surface.rs, tests/readme_retirement_rationale.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/architecture_current_surface.rs:152-170` `architecture_names_the_current_store_and_inspector_surface`
- `tests/readme_retirement_rationale.rs:89-107` `readme_records_the_symbols_default_and_the_retirement_rationale`

#### `dup-0390` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/architecture_current_surface.rs, tests/readme_retirement_rationale.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/architecture_current_surface.rs:188-209` `architecture_names_no_retired_or_wrong_default_grounder`
- `tests/readme_retirement_rationale.rs:110-126` `readme_carries_none_of_the_retired_grounder_inversions`

#### `dup-0391` (exact, 4 sites)

Proposed home: `a new shared module (sites span 4 files: tests/build_budget_slots_periphery.rs, tests/build_env_authority_periphery.rs, tests/integrate_conflict_merge_periphery.rs, tests/store_flag_precedence.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/build_budget_slots_periphery.rs:206-221` `write_workflow`
- `tests/build_env_authority_periphery.rs:165-180` `write_workflow`
- `tests/integrate_conflict_merge_periphery.rs:389-404` `write_workflow`
- `tests/store_flag_precedence.rs:75-90` `write_workflow`

#### `dup-0392` (semantic, 6 sites)

Proposed home: `one shared `write_workflow` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 6 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/build_budget_slots_periphery.rs:206-221` `write_workflow`
- `tests/build_env_authority_periphery.rs:165-180` `write_workflow`
- `tests/integrate_conflict_merge_periphery.rs:389-404` `write_workflow`
- `tests/scratch_workdir_config.rs:37-39` `write_workflow`
- `tests/store_config.rs:40-42` `write_workflow`
- `tests/store_flag_precedence.rs:75-90` `write_workflow`

#### `dup-0393` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/build_env_authority_periphery.rs, tests/rigger_run_base_gate_env_periphery.rs, tests/spawn_target_dir_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/build_env_authority_periphery.rs:215-219` `env_test_lock`
- `tests/rigger_run_base_gate_env_periphery.rs:80-84` `env_test_lock`
- `tests/spawn_target_dir_periphery.rs:104-108` `spawn_env_test_lock`

#### `dup-0394` (semantic, 2 sites)

Proposed home: `one shared `env_test_lock` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/build_env_authority_periphery.rs:215-219` `env_test_lock`
- `tests/rigger_run_base_gate_env_periphery.rs:80-84` `env_test_lock`

#### `dup-0395` (exact, 2 sites)

Proposed home: `build_env_authority_periphery::real_driver_spy`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/build_env_authority_periphery.rs:369-376` `new`
- `tests/rigger_run_base_gate_env_periphery.rs:120-127` `new`

#### `dup-0396` (exact, 2 sites)

Proposed home: `build_env_authority_periphery::real_driver_spy`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/build_env_authority_periphery.rs:384-394` `spawn`
- `tests/rigger_run_base_gate_env_periphery.rs:135-145` `spawn`

#### `dup-0397` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/build_env_authority_periphery.rs, tests/rigger_run_base_gate_env_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/build_env_authority_periphery.rs:413-466` `run_once`
- `tests/rigger_run_base_gate_env_periphery.rs:155-206` `run_once`

#### `dup-0398` (near, 3 sites)

Proposed home: `build_env_authority_periphery::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/build_env_authority_periphery.rs:501-565` `one_build_environment_authority_reaches_a_real_gate_subprocess_and_a_real_agent_subprocess`
- `tests/build_env_authority_periphery.rs:641-693` `jobs_cap_coexists_with_a_configured_wrapper_at_both_real_injection_sites`
- `tests/build_env_authority_periphery.rs:720-766` `auto_wrapper_resolves_to_a_real_probed_binary_and_reaches_both_real_subprocesses`

#### `dup-0399` (near, 2 sites)

Proposed home: `build_env_authority_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/build_env_authority_periphery.rs:932-1007` `run_propagates_a_named_wrappers_uncreatable_cache_dir_at_the_library_entry_point`
- `tests/build_env_authority_periphery.rs:1025-1104` `run_propagates_a_named_wrappers_preexisting_unwritable_cache_dir_at_the_library_entry_point`

#### `dup-0400` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/build_watch_paths.rs, tests/gitsemver_worktree_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/build_watch_paths.rs:78-85` `fixture_repo`
- `tests/gitsemver_worktree_periphery.rs:97-112` `fixture_repo`

#### `dup-0401` (semantic, 3 sites)

Proposed home: `one shared `fixture_repo` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/build_watch_paths.rs:78-85` `fixture_repo`
- `tests/gitsemver_derivation.rs:61-76` `fixture_repo`
- `tests/gitsemver_worktree_periphery.rs:97-112` `fixture_repo`

#### `dup-0402` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/calls_down_execution_path_periphery.rs, tests/community_resolution_knob.rs, tests/concepts_fold_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/calls_down_execution_path_periphery.rs:102-106` `node_ids`
- `tests/community_resolution_knob.rs:192-201` `community_nodes`
- `tests/concepts_fold_periphery.rs:80-89` `concept_nodes`

#### `dup-0403` (near, 2 sites)

Proposed home: `calls_down_execution_path_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/calls_down_execution_path_periphery.rs:314-355` `the_depth_bound_clamps_the_layers_the_walk_returns`
- `tests/calls_down_execution_path_periphery.rs:796-864` `the_up_walk_clamps_the_caller_dag_to_the_depth_bound_and_emits_a_deterministic_layered_order`

#### `dup-0404` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/canary_false_positives_periphery.rs, tests/canary_unattributed_rejects_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/canary_false_positives_periphery.rs:55-120` `spawn`
- `tests/canary_unattributed_rejects_periphery.rs:54-107` `spawn`

#### `dup-0405` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/canary_false_positives_periphery.rs, tests/canary_tolerant_attribution_periphery.rs, tests/canary_unattributed_rejects_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/canary_false_positives_periphery.rs:138-145` `panel`
- `tests/canary_tolerant_attribution_periphery.rs:103-110` `panel`
- `tests/canary_unattributed_rejects_periphery.rs:125-132` `panel`

#### `dup-0406` (near, 5 sites)

Proposed home: `a new shared module (sites span 5 files: tests/canary_false_positives_periphery.rs, tests/canary_findings_volume_periphery.rs, tests/canary_item_sharding_jobs_cap_periphery.rs, tests/canary_progress_hook_periphery.rs, tests/canary_unattributed_rejects_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/canary_false_positives_periphery.rs:147-161` `item`
- `tests/canary_findings_volume_periphery.rs:133-147` `item`
- `tests/canary_item_sharding_jobs_cap_periphery.rs:65-79` `item`
- `tests/canary_progress_hook_periphery.rs:71-85` `item`
- `tests/canary_unattributed_rejects_periphery.rs:134-148` `item`

#### `dup-0407` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/canary_false_positives_periphery.rs, tests/canary_unattributed_rejects_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/canary_false_positives_periphery.rs:170-297` `run_canary_scores_false_positive_controls_and_project_canary_counts_them`
- `tests/canary_unattributed_rejects_periphery.rs:157-290` `run_canary_scores_an_unattributed_correct_reject_and_project_canary_counts_only_it`

#### `dup-0408` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/canary_item_sharding_jobs_cap_periphery.rs, tests/canary_progress_hook_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/canary_item_sharding_jobs_cap_periphery.rs:84-90` `anchor_of`
- `tests/canary_progress_hook_periphery.rs:91-97` `anchor_of`

#### `dup-0409` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: tests/canary_item_sharding_jobs_cap_periphery.rs, tests/canary_progress_hook_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/canary_item_sharding_jobs_cap_periphery.rs:202-236` `spawn`
- `tests/canary_item_sharding_jobs_cap_periphery.rs:288-317` `spawn`
- `tests/canary_progress_hook_periphery.rs:107-134` `spawn`

#### `dup-0410` (near, 14 sites)

Proposed home: `a new shared module (sites span 14 files: tests/canary_model_drift_periphery.rs, tests/cause_wire_periphery.rs, tests/cli.rs, tests/escalation_resume_periphery.rs, tests/graph_show_periphery.rs, tests/graph_show_staleness.rs, tests/graph_show_surface.rs, tests/reset_build_cache_periphery.rs, tests/reset_menu.rs, tests/reset_menu_identity_migration_periphery.rs, tests/spawn_scratch_reap_authorized_root_periphery.rs, tests/spec_lint.rs, tests/validate_footprint_default_scratch_root_periphery.rs, tests/watchdog_cli_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/canary_model_drift_periphery.rs:39-46` `temp_project`
- `tests/cause_wire_periphery.rs:54-61` `temp_project`
- `tests/cli.rs:19-29` `temp_project`
- `tests/escalation_resume_periphery.rs:79-86` `temp_project`
- `tests/graph_show_periphery.rs:49-56` `temp_project`
- `tests/graph_show_staleness.rs:37-44` `temp_project`
- `tests/graph_show_surface.rs:36-43` `temp_project`
- `tests/reset_build_cache_periphery.rs:36-43` `temp_project`
- `tests/reset_menu.rs:37-44` `temp_project`
- `tests/reset_menu_identity_migration_periphery.rs:32-39` `temp_project`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:45-52` `temp_project`
- `tests/spec_lint.rs:23-30` `temp_project`
- `tests/validate_footprint_default_scratch_root_periphery.rs:26-33` `temp_project`
- `tests/watchdog_cli_periphery.rs:44-51` `temp_project`

#### `dup-0411` (semantic, 21 sites)

Proposed home: `one shared `temp_project` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 21 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/canary_model_drift_periphery.rs:39-46` `temp_project`
- `tests/cause_wire_periphery.rs:54-61` `temp_project`
- `tests/cli.rs:19-29` `temp_project`
- `tests/escalation_resume_periphery.rs:79-86` `temp_project`
- `tests/graph_show_periphery.rs:49-56` `temp_project`
- `tests/graph_show_staleness.rs:37-44` `temp_project`
- `tests/graph_show_surface.rs:36-43` `temp_project`
- `tests/migration_is_deliberate_periphery.rs:448-456` `temp_project`
- `tests/published_content_key_split_periphery.rs:373-395` `temp_project`
- `tests/reset_build_cache_periphery.rs:36-43` `temp_project`
- `tests/reset_derived_compaction.rs:39-47` `temp_project`
- `tests/reset_derived_compaction_periphery.rs:600-608` `temp_project`
- `tests/reset_derived_live_writer_guard_periphery.rs:41-51` `temp_project`
- `tests/reset_menu.rs:37-44` `temp_project`
- `tests/reset_menu_identity_migration_periphery.rs:32-39` `temp_project`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:45-52` `temp_project`
- `tests/spec_lint.rs:23-30` `temp_project`
- `tests/validate_advisories.rs:47-55` `temp_project`
- `tests/validate_behind_the_tree_periphery.rs:66-74` `temp_project`
- `tests/validate_footprint_default_scratch_root_periphery.rs:26-33` `temp_project`
- `tests/watchdog_cli_periphery.rs:44-51` `temp_project`

#### `dup-0412` (exact, 15 sites)

Proposed home: `a new shared module (sites span 15 files: tests/canary_model_drift_periphery.rs, tests/cause_wire_periphery.rs, tests/escalation_resume_periphery.rs, tests/fanout_template_needs_and_stage_retries_periphery.rs, tests/migration_is_deliberate_periphery.rs, tests/reset_build_cache_periphery.rs, tests/reset_derived_compaction_periphery.rs, tests/reset_menu.rs, tests/reset_menu_identity_migration_periphery.rs, tests/spec_lint.rs, tests/step_attention_periphery.rs, tests/validate_advisories.rs, tests/validate_behind_the_tree_periphery.rs, tests/watchdog_cli_periphery.rs, tests/worktree_liveness_fence_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/canary_model_drift_periphery.rs:50-65` `run_rigger`
- `tests/cause_wire_periphery.rs:120-132` `run_rigger`
- `tests/escalation_resume_periphery.rs:159-171` `run_rigger`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:193-205` `run_rigger`
- `tests/migration_is_deliberate_periphery.rs:491-503` `run_rigger`
- `tests/reset_build_cache_periphery.rs:76-88` `run_rigger`
- `tests/reset_derived_compaction_periphery.rs:642-654` `run_rigger`
- `tests/reset_menu.rs:86-98` `run_rigger`
- `tests/reset_menu_identity_migration_periphery.rs:74-86` `run_rigger`
- `tests/spec_lint.rs:35-47` `run_rigger`
- `tests/step_attention_periphery.rs:202-214` `run_rigger`
- `tests/validate_advisories.rs:88-100` `run_rigger`
- `tests/validate_behind_the_tree_periphery.rs:78-90` `run_rigger`
- `tests/watchdog_cli_periphery.rs:110-122` `run_rigger`
- `tests/worktree_liveness_fence_periphery.rs:214-226` `run_rigger`

#### `dup-0413` (near, 2 sites)

Proposed home: `canary_model_drift_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/canary_model_drift_periphery.rs:151-204` `canary_and_validate_treat_an_unattributed_tier_as_unmeasured_never_defaulted_from_output_prose`
- `tests/canary_model_drift_periphery.rs:213-293` `canary_and_validate_drift_reads_only_metadata_prose_can_neither_mask_nor_forge_it`

#### `dup-0414` (near, 2 sites)

Proposed home: `canary_tolerant_attribution_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/canary_tolerant_attribution_periphery.rs:130-155` `a_tolerant_match_in_a_later_about_entry_still_scores_the_catch`
- `tests/canary_tolerant_attribution_periphery.rs:166-190` `an_empty_about_entry_never_scores_a_catch_even_against_a_trailing_slash_anchor`

#### `dup-0415` (exact, 6 sites)

Proposed home: `a new shared module (sites span 6 files: tests/cause_wire_periphery.rs, tests/cli.rs, tests/escalation_resume_periphery.rs, tests/heartbeat_write_read_agree_periphery.rs, tests/spawn_scratch_reap_authorized_root_periphery.rs, tests/watchdog_cli_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cause_wire_periphery.rs:65-69` `seed_store`
- `tests/cli.rs:38-42` `seed_store`
- `tests/escalation_resume_periphery.rs:90-94` `seed_store`
- `tests/heartbeat_write_read_agree_periphery.rs:118-122` `seed_store`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:56-60` `seed_store`
- `tests/watchdog_cli_periphery.rs:56-60` `seed_store`

#### `dup-0416` (near, 19 sites)

Proposed home: `a new shared module (sites span 19 files: tests/cause_wire_periphery.rs, tests/change_path_revert_periphery.rs, tests/cli.rs, tests/dedup_seeding_periphery.rs, tests/escalation_resume_periphery.rs, tests/fanout_template_needs_and_stage_retries_periphery.rs, tests/halted_spawn_wip_recovery_periphery.rs, tests/migration_is_deliberate_periphery.rs, tests/projections_stay_local.rs, tests/reset_derived_compaction.rs, tests/reset_derived_compaction_periphery.rs, tests/reset_menu.rs, tests/reset_menu_identity_migration_periphery.rs, tests/spawn_scratch_reap_authorized_root_periphery.rs, tests/step_attention_periphery.rs, tests/store_resolution.rs, tests/validate_advisories.rs, tests/watchdog_cli_periphery.rs, tests/worktree_liveness_fence_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cause_wire_periphery.rs:75-97` `run_stream_identity`
- `tests/change_path_revert_periphery.rs:130-155` `run_stream_identity`
- `tests/cli.rs:49-71` `run_stream_identity`
- `tests/dedup_seeding_periphery.rs:580-605` `run_stream_identity`
- `tests/escalation_resume_periphery.rs:99-121` `run_stream_identity`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:165-187` `run_stream_identity`
- `tests/halted_spawn_wip_recovery_periphery.rs:436-458` `run_stream_identity`
- `tests/migration_is_deliberate_periphery.rs:463-485` `project_identity_of`
- `tests/projections_stay_local.rs:134-156` `store_identity`
- `tests/reset_derived_compaction.rs:51-73` `run_stream_identity`
- `tests/reset_derived_compaction_periphery.rs:616-638` `project_identity`
- `tests/reset_menu.rs:46-68` `run_stream_identity`
- `tests/reset_menu_identity_migration_periphery.rs:41-63` `run_stream_identity`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:65-87` `run_stream_identity`
- `tests/step_attention_periphery.rs:150-172` `run_stream_identity`
- `tests/store_resolution.rs:149-171` `run_stream_identity`
- `tests/validate_advisories.rs:57-79` `run_stream_identity`
- `tests/watchdog_cli_periphery.rs:66-88` `run_stream_identity`
- `tests/worktree_liveness_fence_periphery.rs:164-186` `run_stream_identity`

#### `dup-0417` (semantic, 21 sites)

Proposed home: `one shared `run_stream_identity` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 21 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/cause_wire_periphery.rs:75-97` `run_stream_identity`
- `tests/change_path_revert_periphery.rs:130-155` `run_stream_identity`
- `tests/cli.rs:49-71` `run_stream_identity`
- `tests/dedup_seeding_periphery.rs:580-605` `run_stream_identity`
- `tests/escalation_resume_periphery.rs:99-121` `run_stream_identity`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:165-187` `run_stream_identity`
- `tests/graph_show_periphery.rs:61-77` `run_stream_identity`
- `tests/graph_show_staleness.rs:49-65` `run_stream_identity`
- `tests/graph_show_surface.rs:48-64` `run_stream_identity`
- `tests/halted_spawn_wip_recovery_periphery.rs:436-458` `run_stream_identity`
- `tests/heartbeat_write_read_agree_periphery.rs:128-146` `run_stream_identity`
- `tests/reset_derived_compaction.rs:51-73` `run_stream_identity`
- `tests/reset_derived_live_writer_guard_periphery.rs:73-87` `run_stream_identity`
- `tests/reset_menu.rs:46-68` `run_stream_identity`
- `tests/reset_menu_identity_migration_periphery.rs:41-63` `run_stream_identity`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:65-87` `run_stream_identity`
- `tests/step_attention_periphery.rs:150-172` `run_stream_identity`
- `tests/store_resolution.rs:149-171` `run_stream_identity`
- `tests/validate_advisories.rs:57-79` `run_stream_identity`
- `tests/watchdog_cli_periphery.rs:66-88` `run_stream_identity`
- `tests/worktree_liveness_fence_periphery.rs:164-186` `run_stream_identity`

#### `dup-0418` (near, 7 sites)

Proposed home: `a new shared module (sites span 7 files: tests/cause_wire_periphery.rs, tests/escalation_resume_periphery.rs, tests/heartbeat_write_read_agree_periphery.rs, tests/reset_derived_live_writer_guard_periphery.rs, tests/reset_menu.rs, tests/reset_menu_identity_migration_periphery.rs, tests/watchdog_cli_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cause_wire_periphery.rs:103-116` `seed_run_events`
- `tests/escalation_resume_periphery.rs:126-139` `seed_run_events`
- `tests/heartbeat_write_read_agree_periphery.rs:151-164` `seed_run_events`
- `tests/reset_derived_live_writer_guard_periphery.rs:91-103` `seed_run_events`
- `tests/reset_menu.rs:107-119` `seed_run_events`
- `tests/reset_menu_identity_migration_periphery.rs:95-107` `seed_run_events`
- `tests/watchdog_cli_periphery.rs:93-106` `seed_run_events`

#### `dup-0419` (semantic, 11 sites)

Proposed home: `one shared `seed_run_events` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 11 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/cause_wire_periphery.rs:103-116` `seed_run_events`
- `tests/cli.rs:82-100` `seed_run_events`
- `tests/escalation_resume_periphery.rs:126-139` `seed_run_events`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:142-160` `seed_run_events`
- `tests/halted_spawn_wip_recovery_periphery.rs:465-483` `seed_run_events`
- `tests/heartbeat_write_read_agree_periphery.rs:151-164` `seed_run_events`
- `tests/reset_derived_live_writer_guard_periphery.rs:91-103` `seed_run_events`
- `tests/reset_menu.rs:107-119` `seed_run_events`
- `tests/reset_menu_identity_migration_periphery.rs:95-107` `seed_run_events`
- `tests/step_attention_periphery.rs:178-196` `seed_run_events`
- `tests/watchdog_cli_periphery.rs:93-106` `seed_run_events`

#### `dup-0420` (near, 13 sites)

Proposed home: `a new shared module (sites span 3 files: tests/cause_wire_periphery.rs, tests/cli.rs, tests/escalation_resume_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cause_wire_periphery.rs:195-217` `a_legacy_causeless_unit_failed_event_survives_a_real_store_round_trip_and_renders_unknown`
- `tests/cause_wire_periphery.rs:224-256` `the_reject_recurrence_line_names_the_latest_of_several_recorded_causes_through_the_real_binary`
- `tests/cli.rs:10627-10665` `stats_reports_the_latest_run_by_default_and_all_for_the_aggregate`
- `tests/cli.rs:21680-21739` `release_ready_handoff_surfaces_on_status_for_a_done_run`
- `tests/cli.rs:21949-21990` `release_ready_is_silent_on_status_for_an_unfinished_run`
- `tests/cli.rs:22061-22094` `release_ready_pluralizes_the_unit_count_on_status_for_a_multi_unit_run`
- `tests/cli.rs:22139-22161` `release_ready_is_silent_on_status_for_a_spec_defective_run`
- `tests/escalation_resume_periphery.rs:183-207` `a_unit_resumed_event_seeded_directly_through_a_real_store_reaches_status_without_the_command`
- `tests/escalation_resume_periphery.rs:218-247` `a_legacy_shaped_unit_resumed_event_missing_both_optional_fields_survives_a_real_store_round_trip`
- `tests/escalation_resume_periphery.rs:336-367` `the_resumed_banner_survives_a_genuinely_in_flight_re_parked_attempt_not_yet_resolved`
- `tests/escalation_resume_periphery.rs:444-465` `resume_unit_rejects_an_unknown_unit_absent_from_the_run`
- `tests/escalation_resume_periphery.rs:472-496` `resume_unit_refuses_when_the_recorded_branch_was_never_created`
- `tests/escalation_resume_periphery.rs:503-526` `resume_unit_refuses_an_already_integrated_unit`

#### `dup-0421` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/change_path_revert_periphery.rs, tests/dedup_seeding_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/change_path_revert_periphery.rs:58-72` `run_rigger`
- `tests/dedup_seeding_periphery.rs:655-671` `run_rigger`

#### `dup-0422` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/change_path_revert_periphery.rs, tests/dedup_seeding_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/change_path_revert_periphery.rs:77-83` `ingested_count`
- `tests/dedup_seeding_periphery.rs:676-682` `ingested_count`

#### `dup-0423` (semantic, 2 sites)

Proposed home: `one shared `ingested_count` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/change_path_revert_periphery.rs:77-83` `ingested_count`
- `tests/dedup_seeding_periphery.rs:676-682` `ingested_count`

#### `dup-0424` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/change_path_revert_periphery.rs, tests/dedup_seeding_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/change_path_revert_periphery.rs:159-175` `read_run_stream`
- `tests/dedup_seeding_periphery.rs:630-646` `read_run_stream`

#### `dup-0425` (semantic, 3 sites)

Proposed home: `one shared `read_run_stream` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/change_path_revert_periphery.rs:159-175` `read_run_stream`
- `tests/dedup_seeding_periphery.rs:630-646` `read_run_stream`
- `tests/reset_derived_compaction_periphery.rs:2837-2841` `read_run_stream`

#### `dup-0426` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/checkin_mutation_diff_base_periphery.rs, tests/halted_spawn_wip_recovery_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/checkin_mutation_diff_base_periphery.rs:83-90` `run_git`
- `tests/halted_spawn_wip_recovery_periphery.rs:89-96` `run_git`

#### `dup-0427` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/checkin_mutation_diff_base_periphery.rs, tests/halted_spawn_wip_recovery_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/checkin_mutation_diff_base_periphery.rs:92-99` `git_ok`
- `tests/halted_spawn_wip_recovery_periphery.rs:98-105` `git_ok`

#### `dup-0428` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/checkin_mutation_diff_base_periphery.rs, tests/halted_spawn_wip_recovery_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/checkin_mutation_diff_base_periphery.rs:101-109` `git_out`
- `tests/halted_spawn_wip_recovery_periphery.rs:107-115` `git_out`

#### `dup-0429` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/checkin_mutation_diff_base_periphery.rs, tests/store_resolution.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/checkin_mutation_diff_base_periphery.rs:123-140` `the_shipped_mutation_gate_guards_on_rigger_run_base_never_a_merge_base`
- `tests/store_resolution.rs:119-135` `the_single_resolver_exists_and_the_old_per_command_helper_is_retired`

#### `dup-0430` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/cli.rs, tests/fanout_template_needs_and_stage_retries_periphery.rs, tests/step_attention_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:82-100` `seed_run_events`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:142-160` `seed_run_events`
- `tests/step_attention_periphery.rs:178-196` `seed_run_events`

#### `dup-0431` (semantic, 7 sites)

Proposed home: `one shared `temp_git_project_with_commit` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 7 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/cli.rs:105-122` `temp_git_project_with_commit`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:113-130` `temp_git_project_with_commit`
- `tests/halted_spawn_wip_recovery_periphery.rs:119-127` `temp_git_project_with_commit`
- `tests/step_attention_periphery.rs:464-484` `temp_git_project_with_commit`
- `tests/step_root_resolution_periphery.rs:140-161` `temp_git_project_with_commit`
- `tests/workflow_driver_resolved_model_periphery.rs:57-78` `temp_git_project_with_commit`
- `tests/worktree_liveness_fence_periphery.rs:94-115` `temp_git_project_with_commit`

#### `dup-0432` (exact, 4 sites)

Proposed home: `a new shared module (sites span 4 files: tests/cli.rs, tests/halted_spawn_wip_recovery_periphery.rs, tests/reset_derived_compaction.rs, tests/step_root_resolution_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:139-141` `run_rigger`
- `tests/halted_spawn_wip_recovery_periphery.rs:184-186` `run_rigger`
- `tests/reset_derived_compaction.rs:82-84` `run_rigger`
- `tests/step_root_resolution_periphery.rs:213-215` `run_rigger`

#### `dup-0433` (exact, 6 sites)

Proposed home: `a new shared module (sites span 6 files: tests/cli.rs, tests/halted_spawn_wip_recovery_periphery.rs, tests/reset_derived_compaction.rs, tests/reset_derived_live_writer_guard_periphery.rs, tests/spawn_scratch_reap_authorized_root_periphery.rs, tests/step_root_resolution_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:146-172` `run_rigger_envs`
- `tests/halted_spawn_wip_recovery_periphery.rs:165-180` `run_rigger_envs`
- `tests/reset_derived_compaction.rs:86-101` `run_rigger_envs`
- `tests/reset_derived_live_writer_guard_periphery.rs:108-123` `run_rigger`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:118-133` `run_rigger_envs`
- `tests/step_root_resolution_periphery.rs:196-211` `run_rigger_envs`

#### `dup-0434` (semantic, 5 sites)

Proposed home: `one shared `run_rigger_envs` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 5 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/cli.rs:146-172` `run_rigger_envs`
- `tests/halted_spawn_wip_recovery_periphery.rs:165-180` `run_rigger_envs`
- `tests/reset_derived_compaction.rs:86-101` `run_rigger_envs`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:118-133` `run_rigger_envs`
- `tests/step_root_resolution_periphery.rs:196-211` `run_rigger_envs`

#### `dup-0435` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/cli.rs, tests/worktree_liveness_fence_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:189-197` `git_ok`
- `tests/worktree_liveness_fence_periphery.rs:138-146` `git_ok`

#### `dup-0436` (near, 6 sites)

Proposed home: `cli::support (consolidate these 6 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:352-375` `emit_review_finding_shows_in_peers`
- `tests/cli.rs:8668-8771` `step_surfaces_a_hung_unbounded_spawn_recorded_as_a_liveness_fault_by_the_driver`
- `tests/cli.rs:8796-8859` `step_attention_never_restamps_a_hung_unbounded_spawn_when_repo_less`
- `tests/cli.rs:12140-12204` `a_liveness_fault_on_a_review_spawn_halts_instead_of_re_parking`
- `tests/cli.rs:15481-15511` `result_if_absent_records_when_the_spawn_is_unanswered`
- `tests/cli.rs:15519-15557` `result_if_absent_never_clobbers_a_self_reported_success`

#### `dup-0437` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:871-1008` `reset_runs_compacts_the_on_disk_graph_after_reclaiming_superseded_rows`
- `tests/cli.rs:1038-1183` `reset_runs_reports_nonzero_bytes_reclaimed_then_a_second_pass_is_an_idempotent_no_op`

#### `dup-0438` (semantic, 2 sites)

Proposed home: `one shared `reported_reclaimed_bytes` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/cli.rs:1014-1020` `reported_reclaimed_bytes`
- `tests/reset_derived_compaction_periphery.rs:4411-4424` `reported_reclaimed_bytes`

#### `dup-0439` (exact, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:1372-1389` `prompt_refuses_to_fabricate_a_store_when_none_exists`
- `tests/cli.rs:1561-1578` `scratch_refuses_to_fabricate_a_store_when_none_exists`

#### `dup-0440` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:1813-1864` `result_from_a_relocated_git_worktree_outside_the_repo_records_into_the_repo_stream`
- `tests/cli.rs:1877-1919` `result_from_a_configured_nested_git_worktree_records_into_the_repo_stream`

#### `dup-0441` (near, 4 sites)

Proposed home: `cli::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:1982-2005` `result_if_absent_orphan_advisory_states_the_conditional_not_a_recording`
- `tests/cli.rs:16904-16924` `canary_if_model_changed_skips_when_the_model_is_unchanged`
- `tests/cli.rs:16931-16953` `canary_if_model_changed_runs_when_a_tier_resolved_model_repointed`
- `tests/cli.rs:16962-16996` `canary_if_model_changed_skips_a_snapshot_only_date_suffix_bump_without_running_the_panel`

#### `dup-0442` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:2070-2141` `a_spawns_scratch_is_reclaimed_the_moment_its_result_is_recorded_for_every_outcome`
- `tests/cli.rs:2157-2231` `a_spawns_mutation_scratch_is_reclaimed_the_moment_its_own_result_reports_for_every_outcome`

#### `dup-0443` (near, 4 sites)

Proposed home: `cli::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:2245-2280` `a_reviewers_result_never_reclaims_the_implementers_mutation_scratch`
- `tests/cli.rs:2293-2336` `a_dotdot_spawn_id_never_escapes_the_registered_scratch_roots`
- `tests/cli.rs:2353-2406` `a_dotdot_spawn_id_never_escapes_the_pre_existing_agent_scratch_root_either`
- `tests/cli.rs:2427-2465` `a_leading_slash_spawn_id_never_collapses_the_reclaim_to_its_registered_root`

#### `dup-0444` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:2555-2603` `two_speculation_lanes_of_the_same_unit_get_distinct_mutation_scratch_dirs`
- `tests/cli.rs:9538-9602` `a_terminal_units_registered_mutation_scratch_is_reaped_while_a_live_siblings_survives`
- `tests/cli.rs:10368-10485` `a_resumed_run_reaps_an_escalated_and_an_on_pass_none_settled_units_registered_mutation_scratch_not_just_an_integrated_ones`

#### `dup-0445` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:2611-2622` `write_grounder_workflow`
- `tests/cli.rs:17199-17214` `write_gating_lint_project`

#### `dup-0446` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:3373-3429` `worktree_sweep_completes_before_any_add_within_one_step`
- `tests/cli.rs:8877-8912` `the_hung_cursor_is_persisted_only_after_the_step_that_carries_it_is_printed`

#### `dup-0447` (near, 4 sites)

Proposed home: `cli::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:3552-3599` `step_from_a_linked_worktree_refuses_naming_both_trees`
- `tests/cli.rs:3606-3643` `run_from_a_linked_worktree_refuses_naming_both_trees`
- `tests/cli.rs:3650-3687` `workflow_from_a_linked_worktree_refuses_naming_both_trees`
- `tests/cli.rs:3700-3750` `serve_from_a_linked_worktree_refuses_naming_both_trees`

#### `dup-0448` (semantic, 6 sites)

Proposed home: `one shared `rigger_js_source` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 6 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/cli.rs:3886-3891` `rigger_js_source`
- `tests/meta_phases_declaration_periphery.rs:54-59` `rigger_js_source`
- `tests/phase_of_role_mapping_periphery.rs:38-43` `rigger_js_source`
- `tests/review_tier_roster_periphery.rs:44-49` `rigger_js_source`
- `tests/step_attention_periphery.rs:964-969` `rigger_js_source`
- `tests/worker_persona_label_periphery.rs:48-53` `rigger_js_source`

#### `dup-0449` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/cli.rs, tests/worker_persona_label_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:4144-4200` `native_driver_scratch_policy_directs_the_worker_to_its_own_spawn_owned_container`
- `tests/worker_persona_label_periphery.rs:413-434` `workerlabel_is_actually_wired_into_runworkers_agent_call_label`

#### `dup-0450` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/cli.rs, tests/projections_stay_local.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:4424-4437` `native_driver_drains_in_flight_workers_before_a_loud_stop`
- `tests/projections_stay_local.rs:101-119` `the_graph_and_progress_projections_open_via_the_local_sqlite_constructors`

#### `dup-0451` (near, 17 sites)

Proposed home: `a new shared module (sites span 6 files: tests/cli.rs, tests/halted_spawn_wip_recovery_periphery.rs, tests/step_attention_periphery.rs, tests/step_root_resolution_periphery.rs, tests/workflow_driver_resolved_model_periphery.rs, tests/worktree_liveness_fence_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:4444-4468` `write_two_stage_workflow`
- `tests/cli.rs:4473-4497` `write_budget_one_two_stage_workflow`
- `tests/cli.rs:4583-4603` `write_standalone_review_workflow`
- `tests/cli.rs:4994-5018` `write_reviewless_git_unit_workflow`
- `tests/cli.rs:5025-5050` `write_reviewless_git_escalating_unit_workflow`
- `tests/cli.rs:7941-7966` `write_failing_gate_escalating_workflow`
- `tests/cli.rs:7975-8000` `write_manual_review_workflow`
- `tests/cli.rs:8203-8226` `write_budget_one_dependency_workflow`
- `tests/cli.rs:8293-8306` `write_liveness_workflow`
- `tests/cli.rs:8641-8654` `write_unbounded_liveness_workflow`
- `tests/cli.rs:11835-11866` `write_gated_reviewed_workflow`
- `tests/halted_spawn_wip_recovery_periphery.rs:134-158` `write_solo_unit_workflow`
- `tests/step_attention_periphery.rs:221-246` `write_attention_progression_workflow`
- `tests/step_attention_periphery.rs:492-518` `write_attention_ordering_workflow`
- `tests/step_root_resolution_periphery.rs:167-191` `write_reviewless_git_unit_workflow`
- `tests/workflow_driver_resolved_model_periphery.rs:85-98` `write_one_stage_workflow`
- `tests/worktree_liveness_fence_periphery.rs:234-258` `write_reviewless_git_unit_workflow`

#### `dup-0452` (semantic, 3 sites)

Proposed home: `one shared `write_reviewless_git_unit_workflow` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/cli.rs:4994-5018` `write_reviewless_git_unit_workflow`
- `tests/step_root_resolution_periphery.rs:167-191` `write_reviewless_git_unit_workflow`
- `tests/worktree_liveness_fence_periphery.rs:234-258` `write_reviewless_git_unit_workflow`

#### `dup-0453` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:5065-5140` `step_reclaims_the_units_worktree_and_deletes_its_branch_on_a_clean_integrate`
- `tests/cli.rs:5155-5223` `step_reclaims_the_units_worktree_but_keeps_its_branch_on_a_terminal_escalation`

#### `dup-0454` (exact, 4 sites)

Proposed home: `a new shared module (sites span 2 files: tests/cli.rs, tests/watchdog_cli_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:5376-5390` `resume_unit_refuses_an_unknown_unit`
- `tests/cli.rs:11237-11248` `step_rejects_an_unknown_flag`
- `tests/cli.rs:11287-11298` `step_rejects_base_without_a_value`
- `tests/watchdog_cli_periphery.rs:356-370` `watch_rejects_an_unknown_flag_through_the_real_binary_with_a_nonzero_exit`

#### `dup-0455` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:5462-5671` `step_restores_the_unit_worktree_a_gate_deletes_before_the_review_spawn`
- `tests/cli.rs:5897-6144` `step_stamps_a_real_reviewed_sha_after_repeated_between_step_deletions`
- `tests/cli.rs:6327-6470` `step_stamps_a_real_failed_sha_after_a_deletion_before_the_reject_stamp`

#### `dup-0456` (near, 5 sites)

Proposed home: `cli::support (consolidate these 5 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:6510-6672` `run_end_to_end_restores_a_worktree_a_reviewer_agent_deletes_mid_review`
- `tests/cli.rs:6957-7110` `speculation_reject_worktree_sha_is_stamped_after_the_adjudicators_own_deletion_is_restored_end_to_end`
- `tests/cli.rs:9877-10016` `a_speculation_winners_registered_mutation_scratch_across_all_lanes_is_reaped_by_the_real_winner_integrate_teardown`
- `tests/cli.rs:10030-10168` `a_speculation_escalations_registered_mutation_scratch_across_all_lanes_is_reaped_by_the_real_escalation_tail_teardown`
- `tests/cli.rs:10191-10341` `a_speculation_on_pass_none_winners_registered_mutation_scratch_across_all_lanes_is_reaped_by_the_real_on_pass_none_exit_teardown`

#### `dup-0457` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:7134-7279` `resumed_reviewed_unit_stamps_a_real_failed_sha_after_the_exhaustive_gates_own_deletion_is_restored`
- `tests/cli.rs:7302-7479` `resumed_reviewed_unit_stamps_a_real_failed_sha_after_the_post_merge_re_gates_own_deletion_is_restored`

#### `dup-0458` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:7787-7848` `run_registers_a_credential_free_shared_instance`
- `tests/cli.rs:7868-7933` `run_driver_workflow_registers_a_credential_free_shared_instance`

#### `dup-0459` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/cli.rs, tests/step_attention_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:8010-8102` `step_carries_the_escalated_set_when_a_fixpoint_is_reached_with_a_wedged_unit`
- `tests/step_attention_periphery.rs:252-351` `recurrence_and_stalled_frontier_survive_real_process_boundaries`

#### `dup-0460` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/cli.rs, tests/step_attention_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:8311-8321` `plant_stale_marker`
- `tests/step_attention_periphery.rs:416-426` `plant_stale_marker`

#### `dup-0461` (semantic, 2 sites)

Proposed home: `one shared `plant_stale_marker` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/cli.rs:8311-8321` `plant_stale_marker`
- `tests/step_attention_periphery.rs:416-426` `plant_stale_marker`

#### `dup-0462` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:9213-9261` `run_teardown_reclaims_run_level_scratch_at_a_definition_drift_halt`
- `tests/cli.rs:9278-9334` `run_teardown_spares_run_level_scratch_at_a_drift_halt_while_a_hung_spawn_may_be_alive`

#### `dup-0463` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:9357-9384` `run_teardown_spares_run_level_scratch_at_a_manual_review_pause`
- `tests/cli.rs:9409-9457` `run_teardown_spares_run_level_scratch_at_a_drift_halt_while_a_manual_review_is_pending`
- `tests/cli.rs:9467-9515` `run_teardown_reclaims_run_level_scratch_after_a_manual_review_is_integrated`

#### `dup-0464` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:10712-10786` `stats_cli_renders_exact_per_role_spawn_timing_and_unpaired_disclosure`
- `tests/cli.rs:10860-10911` `stats_cli_excludes_suspect_non_positive_duration_pairs_as_unpaired_not_zero`

#### `dup-0465` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:11254-11282` `step_accepts_base_and_anchors_the_run_branch`
- `tests/cli.rs:11307-11351` `step_creates_run_branch_off_head_when_base_unresolvable`

#### `dup-0466` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:11361-11391` `step_refuses_when_there_is_no_reachable_base`
- `tests/cli.rs:11402-11433` `run_refuses_when_there_is_no_reachable_base`
- `tests/cli.rs:11443-11486` `run_workflow_refuses_when_there_is_no_reachable_base`

#### `dup-0467` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/cli.rs, tests/fanout_template_needs_and_stage_retries_periphery.rs, tests/step_attention_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:11823-11825` `temp_repoless_project`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:134-136` `temp_repoless_project`
- `tests/step_attention_periphery.rs:143-145` `temp_repoless_project`

#### `dup-0468` (semantic, 3 sites)

Proposed home: `one shared `temp_repoless_project` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/cli.rs:11823-11825` `temp_repoless_project`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:134-136` `temp_repoless_project`
- `tests/step_attention_periphery.rs:143-145` `temp_repoless_project`

#### `dup-0469` (exact, 4 sites)

Proposed home: `cli::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:12440-12457` `write_gated_workflow_no_review`
- `tests/cli.rs:12509-12526` `write_reviewed_workflow_no_gate`
- `tests/cli.rs:12581-12601` `write_reviewed_workflow_added_gate`
- `tests/cli.rs:12657-12679` `write_reviewed_workflow_extra_stage`

#### `dup-0470` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:12466-12502` `replay_candidate_column_reacts_to_a_changed_config`
- `tests/cli.rs:12535-12575` `replay_removing_a_gate_lowers_the_candidate_gate_runs`

#### `dup-0471` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:12613-12652` `replay_an_added_gate_fails_safe_and_never_fabricates_a_pass`
- `tests/cli.rs:12688-12720` `replay_an_uncovered_candidate_spawn_parks_and_still_prints_a_partial_column`

#### `dup-0472` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:13046-13070` `validate_fails_at_run_start_when_a_named_build_wrapper_is_absent_from_path`
- `tests/cli.rs:13563-13584` `validate_rejects_an_explicit_build_mutation_value_naming_spec_91_end_to_end`

#### `dup-0473` (near, 7 sites)

Proposed home: `cli::support (consolidate these 7 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:13077-13095` `validate_reports_none_when_auto_finds_no_known_wrapper_on_path`
- `tests/cli.rs:13100-13117` `validate_reports_the_resolved_wrapper_when_auto_finds_a_known_wrapper_on_path`
- `tests/cli.rs:13124-13154` `validate_reports_cache_dir_and_budget_alongside_the_wrapper`
- `tests/cli.rs:13296-13321` `validate_reports_none_when_autos_discovered_wrapper_has_an_uncreatable_cache_dir`
- `tests/cli.rs:13392-13418` `validate_reports_none_when_autos_discovered_wrapper_has_a_preexisting_unwritable_cache_dir`
- `tests/cli.rs:13512-13529` `validate_reports_mutation_gate_declared_when_cargo_mutants_is_resolvable`
- `tests/cli.rs:13536-13553` `validate_reports_mutation_gate_declared_by_default_on_a_fresh_scaffold`

#### `dup-0474` (exact, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:13259-13288` `validate_fails_at_run_start_when_a_named_wrappers_cache_dir_cannot_be_created`
- `tests/cli.rs:13353-13382` `validate_fails_at_run_start_when_a_named_wrappers_cache_dir_is_preexisting_but_unwritable`

#### `dup-0475` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:13449-13470` `validate_fails_at_run_start_when_the_scaffolded_mutation_gate_has_no_cargo_mutants_on_path`
- `tests/cli.rs:13487-13507` `validate_fails_before_any_output_when_the_mutation_gate_has_no_cargo_mutants`

#### `dup-0476` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:13975-14017` `validate_footprint_registered_scratch_roots_measures_the_real_mutation_scratch_root`
- `tests/cli.rs:14393-14503` `validate_footprint_worktrees_and_per_unit_caches_measure_real_dead_and_live_entries_through_the_binary`

#### `dup-0477` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:14039-14165` `validate_flags_registered_scratch_roots_dead_share_scoped_to_real_spawn_liveness_in_the_store`
- `tests/cli.rs:14185-14281` `validate_flags_a_prior_abandoned_runs_orphan_even_when_a_later_run_reuses_the_identical_spawn_id`

#### `dup-0478` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:15059-15127` `installed_workflow_courier_prompt_is_foreground_and_honest`
- `tests/cli.rs:15147-15225` `installed_workflow_courier_waits_on_an_auto_backgrounded_step`
- `tests/cli.rs:15402-15474` `installed_workflow_driver_guards_a_null_step`

#### `dup-0479` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:15764-15831` `step_halts_on_definition_drift_and_rebase_definition_continues`
- `tests/cli.rs:15837-15878` `a_fresh_run_repins_the_current_definition_and_never_halts`

#### `dup-0480` (near, 5 sites)

Proposed home: `cli::support (consolidate these 5 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:16008-16065` `stats_canary_reports_the_findings_raised_total_summed_across_items`
- `tests/cli.rs:16100-16154` `stats_canary_renders_na_for_a_tier_with_an_unattributed_correct_reject`
- `tests/cli.rs:16165-16214` `stats_canary_still_renders_a_genuine_zero_when_every_reject_has_attribution`
- `tests/cli.rs:16235-16290` `stats_canary_reports_the_control_false_positive_line`
- `tests/cli.rs:16599-16660` `stats_canary_reports_the_model_pinning_header_through_a_real_wire_event`

#### `dup-0481` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:16382-16415` `rigger_help_gives_the_jobs_flag_its_own_description_line_through_the_real_binary`
- `tests/cli.rs:16430-16488` `rigger_help_gives_the_model_flag_its_own_description_line_through_the_real_binary`

#### `dup-0482` (near, 5 sites)

Proposed home: `a new shared module (sites span 3 files: tests/cli.rs, tests/migration_is_deliberate_periphery.rs, tests/validate_advisories.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:16715-16764` `validate_warns_when_a_tier_resolved_model_repointed_between_runs`
- `tests/cli.rs:16771-16798` `validate_advises_softly_on_a_snapshot_only_date_suffix_bump`
- `tests/cli.rs:16838-16898` `validate_detects_a_stream_whose_position_order_and_revision_order_disagree`
- `tests/migration_is_deliberate_periphery.rs:566-591` `validate_warns_of_retired_code_entities_with_the_measured_count_and_never_fails`
- `tests/validate_advisories.rs:271-296` `validate_warns_of_log_bloat_with_the_measured_factor_and_names_reset_derived`

#### `dup-0483` (semantic, 2 sites)

Proposed home: `one shared `seed_order_signature` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/cli.rs:16810-16831` `seed_order_signature`
- `tests/watchdog_cli_periphery.rs:224-246` `seed_order_signature`

#### `dup-0484` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:17919-17969` `status_reports_not_serving_when_the_recorded_marker_names_a_dead_dash`
- `tests/cli.rs:17982-18027` `status_never_names_the_unattributed_pid_sentinel_as_a_dead_process`

#### `dup-0485` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:18038-18097` `status_shows_the_url_when_the_recorded_marker_names_a_genuinely_serving_dash`
- `tests/cli.rs:18111-18178` `status_trusts_a_genuinely_alive_url_even_with_a_mismatched_marker`
- `tests/cli.rs:18194-18272` `status_reports_not_serving_when_a_mismatched_marker_leaves_a_dead_url_unverified`

#### `dup-0486` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:18609-18671` `docs_ships_graph_hygiene_guidance_to_consumers`
- `tests/cli.rs:18686-18729` `docs_ships_three_verb_lookup_guidance_to_consumers`
- `tests/cli.rs:29704-29744` `docs_installs_the_operator_lookup_rule_text_into_the_shipped_skill_and_handbook`

#### `dup-0487` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:18740-18812` `validate_fails_when_the_committed_using_rigger_docs_drift_and_passes_when_in_sync`
- `tests/cli.rs:18826-18879` `validate_docs_drift_gate_covers_the_second_registry_entry`
- `tests/cli.rs:18976-19030` `validate_docs_drift_gate_covers_the_planning_field_guide_page`

#### `dup-0488` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:19207-19250` `docs_renders_every_per_operation_skill_through_the_compiled_binary`
- `tests/cli.rs:19430-19525` `docs_renders_every_watching_discipline_skill_through_the_compiled_binary`

#### `dup-0489` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:19264-19323` `validate_docs_drift_gate_covers_each_per_operation_skill`
- `tests/cli.rs:19539-19606` `validate_docs_drift_gate_covers_each_watching_discipline_skill`

#### `dup-0490` (exact, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:19337-19398` `setup_installs_every_per_operation_skill_into_the_consumer_project`
- `tests/cli.rs:19620-19681` `setup_installs_every_watching_discipline_skill_into_the_consumer_project`

#### `dup-0491` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:19774-19812` `watch_once_never_names_the_unattributed_pid_sentinel_when_no_url_is_recorded`
- `tests/cli.rs:26454-26492` `watch_once_never_names_the_unattributed_pid_sentinel_when_the_url_is_unparseable`

#### `dup-0492` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:19832-19865` `watch_once_reports_a_dead_dash_when_only_the_url_breadcrumb_is_recorded_and_no_marker_exists`
- `tests/cli.rs:26514-26543` `watch_once_parses_the_urls_port_past_a_colon_in_the_path_with_no_marker`

#### `dup-0493` (near, 5 sites)

Proposed home: `cli::support (consolidate these 5 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:19945-19987` `watch_once_reports_no_dash_anomaly_for_a_done_run_even_with_a_dead_marker`
- `tests/cli.rs:20004-20064` `watch_once_reports_no_dash_anomaly_for_a_fresh_run_that_inherits_an_earlier_runs_dead_marker`
- `tests/cli.rs:20104-20153` `watch_once_reports_this_runs_own_dead_marker_when_written_after_its_run_started`
- `tests/cli.rs:20175-20227` `watch_once_reports_a_dead_marker_predating_run_started_when_dash_attempt_names_this_run`
- `tests/cli.rs:20256-20308` `watch_once_suppresses_a_predating_marker_when_dash_attempt_names_a_different_run`

#### `dup-0494` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:20390-20406` `stage_rigger_shim`
- `tests/cli.rs:20811-20831` `stage_failing_docs_rigger_shim`
- `tests/cli.rs:20859-20887` `stage_stale_rigger_shim`

#### `dup-0495` (near, 6 sites)

Proposed home: `cli::support (consolidate these 6 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:20574-20616` `setup_precommit_hook_passes_untouched_when_the_render_matches`
- `tests/cli.rs:20631-20702` `setup_precommit_hook_never_drift_checks_or_stages_a_registry_entry_outside_its_scope`
- `tests/cli.rs:20899-20949` `setup_precommit_hook_prefers_the_trees_own_built_binary_over_a_stale_path_rigger`
- `tests/cli.rs:20959-21009` `setup_precommit_hook_refuses_the_same_commit_shape_with_only_a_stale_path_rigger`
- `tests/cli.rs:21336-21367` `setup_precommit_hook_warns_and_proceeds_when_rigger_is_unavailable`
- `tests/cli.rs:21374-21404` `setup_precommit_hook_warns_and_proceeds_when_rigger_docs_errors`

#### `dup-0496` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:20838-20849` `stage_tree_built_binary`
- `tests/cli.rs:21016-21031` `stage_unit_derived_binary`

#### `dup-0497` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:22514-22559` `step_self_heals_a_stale_marker_naming_a_dead_pid`
- `tests/cli.rs:22584-22630` `step_self_heals_a_stale_marker_naming_a_live_pid_whose_port_is_unserved`

#### `dup-0498` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:23234-23245` `write_live_instance`
- `tests/cli.rs:23251-23262` `write_stale_instance`

#### `dup-0499` (near, 7 sites)

Proposed home: `cli::support (consolidate these 7 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:23275-23363` `a_reap_on_idle_singleton_serves_while_an_instance_heartbeats_then_reaps_when_the_registry_empties`
- `tests/cli.rs:23479-23563` `a_reap_on_idle_singleton_does_not_reap_before_any_instance_has_registered`
- `tests/cli.rs:23590-23694` `a_reap_on_idle_singleton_survives_a_fresh_agent_liveness_marker_after_the_registry_ages_out`
- `tests/cli.rs:23712-23817` `a_reap_on_idle_singleton_survives_a_fresh_agent_liveness_marker_with_no_git_repo_at_launch`
- `tests/cli.rs:23832-23946` `a_reap_on_idle_singleton_survives_a_second_registered_projects_fresh_agent_liveness_marker`
- `tests/cli.rs:23969-24092` `a_reap_on_idle_singleton_survives_a_foreign_agent_liveness_marker_whose_own_registry_entry_was_already_stale_before_the_watchers_first_poll`
- `tests/cli.rs:24108-24250` `a_landing_poll_racing_the_watchers_first_tick_does_not_erase_a_foreign_projects_only_route_into_known_roots`

#### `dup-0500` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:24257-24312` `a_dash_without_reap_on_idle_never_self_reaps_on_a_quiet_machine`
- `tests/cli.rs:24327-24391` `a_reap_on_idle_singleton_in_a_homeless_environment_serves_without_a_watcher`

#### `dup-0501` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:26106-26148` `watch_once_never_names_a_mismatched_markers_pid_for_the_recorded_urls_port`
- `tests/cli.rs:26572-26621` `watch_once_never_names_a_mismatched_markers_pid_when_the_urls_path_contains_a_colon`

#### `dup-0502` (near, 4 sites)

Proposed home: `cli::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:26760-26798` `run_given_a_spec_path_names_the_spec_lint_as_a_next_step`
- `tests/cli.rs:26923-26966` `step_given_a_spec_path_names_the_spec_lint_even_when_the_step_then_refuses_for_no_reachable_base`
- `tests/cli.rs:27240-27275` `step_reminder_prints_despite_env_naming_a_foreign_pid`
- `tests/cli.rs:27319-27353` `run_reminder_prints_despite_env_naming_a_foreign_pid`

#### `dup-0503` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:26986-27046` `run_surfaces_the_same_spec_lint_advisory_as_validate_the_in_run_call_site`
- `tests/cli.rs:27055-27094` `step_surfaces_the_same_spec_lint_advisory_as_validate_the_in_run_call_site`
- `tests/cli.rs:27112-27179` `run_driver_workflow_surfaces_the_same_spec_lint_advisory_as_validate_the_in_run_call_site`

#### `dup-0504` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:27199-27235` `step_reminder_is_suppressed_when_env_names_the_real_direct_parent_pid`
- `tests/cli.rs:27279-27314` `run_reminder_is_suppressed_when_env_names_the_real_direct_parent_pid`

#### `dup-0505` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:27417-27466` `run_driver_workflow_prints_the_spec_lint_reminder_and_honors_the_pid_scoped_dedup`
- `tests/cli.rs:27478-27533` `run_driver_workflow_reminder_never_reaches_stdout_in_any_pid_sentinel_direction`

#### `dup-0506` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:27549-27583` `run_driver_workflow_fresh_notice_never_reaches_stdout`
- `tests/cli.rs:27590-27614` `run_driver_cli_fresh_notice_still_prints_on_stdout`

#### `dup-0507` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:27706-27806` `cmd_dash_gives_the_stopped_listener_diagnosis_naming_resume_or_kill`
- `tests/cli.rs:27885-28000` `step_names_the_stopped_holder_when_the_step_paths_own_auto_start_hits_the_predecessor_scenario`

#### `dup-0508` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:28351-28458` `mcp_serves_peers_ground_and_graph_over_stdio`
- `tests/cli.rs:29456-29561` `mcp_survives_a_grounder_resolution_failure_and_still_serves_peers_and_graph`

#### `dup-0509` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:28672-28692` `grep_guard_bounces_the_built_in_grep_tool_call_end_to_end`
- `tests/cli.rs:28745-28779` `grep_guard_never_bounces_a_substring_grep_or_a_merely_prefixed_tree_name_end_to_end`
- `tests/cli.rs:29068-29082` `grep_guard_never_bounces_a_path_qualified_non_grep_command_end_to_end`

#### `dup-0510` (near, 7 sites)

Proposed home: `cli::support (consolidate these 7 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:28810-28842` `grep_guard_bounces_a_shell_metacharacter_fused_grep_end_to_end`
- `tests/cli.rs:28854-28876` `grep_guard_bounces_the_remaining_shell_metacharacter_fusions_end_to_end`
- `tests/cli.rs:28912-28935` `grep_guard_bounces_a_quoted_or_escaped_grep_end_to_end`
- `tests/cli.rs:29014-29036` `grep_guard_bounces_a_path_qualified_grep_end_to_end`
- `tests/cli.rs:29042-29060` `grep_guard_still_allows_literal_on_a_path_qualified_grep_end_to_end`
- `tests/cli.rs:29103-29138` `grep_guard_bounces_a_redirect_metacharacter_fused_grep_end_to_end`
- `tests/cli.rs:29246-29290` `grep_guard_bounces_an_output_redirect_metacharacter_fused_grep_end_to_end`

#### `dup-0511` (exact, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:28885-28899` `grep_guard_still_allows_literal_on_a_shell_metacharacter_fused_grep_end_to_end`
- `tests/cli.rs:28945-28959` `grep_guard_still_allows_a_quoted_literal_on_a_quoted_grep_end_to_end`

#### `dup-0512` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:28970-28986` `grep_guard_bounces_a_grep_split_by_a_line_continuation_end_to_end`
- `tests/cli.rs:28992-29004` `grep_guard_still_allows_literal_on_a_line_continuation_split_grep_end_to_end`

#### `dup-0513` (near, 5 sites)

Proposed home: `a new shared module (sites span 2 files: tests/code_entity_test_exclusion_periphery.rs, tests/symbol_ref_caller_attribution.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_entity_test_exclusion_periphery.rs:98-136` `is_test_false_serializes_byte_identically_to_the_pre86_form`
- `tests/code_entity_test_exclusion_periphery.rs:262-289` `is_out_of_line_module_false_serializes_byte_identically_to_the_pre_round6_form`
- `tests/code_entity_test_exclusion_periphery.rs:377-404` `path_override_none_serializes_byte_identically_to_the_pre_round7_form`
- `tests/code_entity_test_exclusion_periphery.rs:498-525` `enclosing_inline_module_path_none_serializes_byte_identically_to_the_pre_round9_form`
- `tests/symbol_ref_caller_attribution.rs:32-77` `a_caller_less_reference_serializes_byte_identically_to_the_pre37_form`

#### `dup-0514` (near, 4 sites)

Proposed home: `code_entity_test_exclusion_periphery::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_entity_test_exclusion_periphery.rs:139-193` `is_test_true_serializes_the_key_and_round_trips`
- `tests/code_entity_test_exclusion_periphery.rs:292-329` `is_out_of_line_module_true_serializes_the_key_and_round_trips`
- `tests/code_entity_test_exclusion_periphery.rs:407-445` `path_override_some_serializes_the_key_and_round_trips`
- `tests/code_entity_test_exclusion_periphery.rs:528-566` `enclosing_inline_module_path_some_serializes_the_key_and_round_trips`

#### `dup-0515` (near, 5 sites)

Proposed home: `a new shared module (sites span 2 files: tests/code_entity_test_exclusion_periphery.rs, tests/symbol_ref_caller_attribution.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_entity_test_exclusion_periphery.rs:196-248` `a_pre86_persisted_index_with_no_is_test_key_loads_defaulting_every_item_to_false`
- `tests/code_entity_test_exclusion_periphery.rs:332-368` `a_pre_round6_persisted_index_with_no_is_out_of_line_module_key_loads_defaulting_to_false`
- `tests/code_entity_test_exclusion_periphery.rs:448-486` `a_pre_round7_persisted_index_with_no_path_override_key_loads_defaulting_to_none`
- `tests/code_entity_test_exclusion_periphery.rs:569-612` `a_pre_round9_persisted_index_with_no_enclosing_inline_module_path_key_loads_defaulting_to_none`
- `tests/symbol_ref_caller_attribution.rs:130-180` `a_pre37_persisted_index_loads_folding_references_caller_less`

#### `dup-0516` (near, 4 sites)

Proposed home: `code_entity_test_exclusion_periphery::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_entity_test_exclusion_periphery.rs:752-799` `cfg_not_test_and_cfg_attr_predicates_graph_as_product_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:808-840` `a_not_wrapping_a_non_test_atom_graphs_as_product_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:1024-1059` `a_url_bearing_attribute_between_test_and_the_item_does_not_leak_the_item_into_the_graph`
- `tests/code_entity_test_exclusion_periphery.rs:1140-1173` `a_comment_mentioning_test_attribute_text_does_not_exclude_the_item_through_the_public_api`

#### `dup-0517` (near, 26 sites)

Proposed home: `code_entity_test_exclusion_periphery::support (consolidate these 26 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_entity_test_exclusion_periphery.rs:957-998` `a_trailing_comment_on_cfg_test_still_excludes_the_module_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:1084-1121` `a_multiline_cfg_test_attribute_still_excludes_the_module_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:1211-1254` `a_trailing_comma_in_a_wrapped_cfg_predicate_still_excludes_the_module_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:1282-1325` `an_inner_cfg_test_attribute_excludes_its_module_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:1367-1409` `an_out_of_line_cfg_test_module_declaration_excludes_its_declared_file_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:1428-1480` `an_out_of_line_cfg_test_module_declaration_in_a_subdirectory_excludes_its_sibling_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:1653-1716` `a_non_test_out_of_line_mod_and_an_inline_test_mod_never_exclude_a_coincidentally_named_sibling_file`
- `tests/code_entity_test_exclusion_periphery.rs:1853-1907` `a_cfg_test_impl_block_excludes_its_methods_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:1941-1976` `a_cfg_test_impl_block_using_the_inner_attribute_form_excludes_its_methods_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:2026-2071` `cfg_test_on_every_other_item_kind_excludes_or_stays_scoped_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:2091-2161` `an_out_of_line_test_mod_declared_inside_a_non_directory_style_file_resolves_correctly`
- `tests/code_entity_test_exclusion_periphery.rs:2172-2237` `an_out_of_line_test_mod_declaration_falls_back_to_a_nested_mod_rs_when_no_flat_sibling_exists`
- `tests/code_entity_test_exclusion_periphery.rs:2249-2320` `a_path_attribute_override_redirects_out_of_line_resolution_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:2333-2409` `an_out_of_line_test_module_files_own_out_of_line_declarations_are_excluded_recursively_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:2477-2546` `an_out_of_line_test_mod_declaration_falls_back_to_a_root_level_nested_mod_rs_when_module_dir_is_empty`
- `tests/code_entity_test_exclusion_periphery.rs:2559-2629` `a_path_attribute_override_resolves_relative_to_a_declaring_files_own_subdirectory`
- `tests/code_entity_test_exclusion_periphery.rs:2657-2711` `a_path_attribute_override_that_walks_upward_with_dotdot_still_excludes_its_target`
- `tests/code_entity_test_exclusion_periphery.rs:2726-2775` `a_path_attribute_override_with_an_explicit_dot_slash_prefix_still_resolves_to_the_same_directory_target`
- `tests/code_entity_test_exclusion_periphery.rs:2794-2846` `a_path_attribute_override_with_chained_dotdot_walks_up_every_popped_level`
- `tests/code_entity_test_exclusion_periphery.rs:2871-2919` `a_path_attribute_override_whose_dotdot_count_overflows_the_declaring_directorys_depth_does_not_silently_collide_with_an_unrelated_real_file`
- `tests/code_entity_test_exclusion_periphery.rs:2945-3023` `a_path_attribute_override_nested_inside_an_inline_module_resolves_under_the_declaring_files_own_module_directory_plus_the_inline_chain`
- `tests/code_entity_test_exclusion_periphery.rs:3044-3143` `a_path_attribute_override_nested_inside_chained_inline_modules_resolves_under_every_enclosing_modules_directory_in_outermost_first_order`
- `tests/code_entity_test_exclusion_periphery.rs:3161-3208` `a_path_attribute_override_that_is_an_absolute_path_does_not_silently_collide_with_an_unrelated_real_file`
- `tests/code_entity_test_exclusion_periphery.rs:3231-3282` `a_path_attribute_override_naming_a_uri_scheme_does_not_silently_collide_with_an_unrelated_real_file`
- `tests/code_entity_test_exclusion_periphery.rs:3300-3348` `a_path_attribute_override_naming_a_windows_drive_letter_does_not_silently_collide_with_an_unrelated_real_file`
- `tests/code_entity_test_exclusion_periphery.rs:3383-3442` `a_path_attribute_override_on_a_mod_declared_inside_a_function_body_is_not_treated_as_nested_in_a_module`

#### `dup-0518` (exact, 6 sites)

Proposed home: `a new shared module (sites span 6 files: tests/code_ingest_events.rs, tests/community_detection_pass.rs, tests/community_fold_periphery.rs, tests/community_resolution_knob.rs, tests/concepts_fold_periphery.rs, tests/design_intent_events.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_ingest_events.rs:30-34` `apply_json`
- `tests/community_detection_pass.rs:31-35` `apply_json`
- `tests/community_fold_periphery.rs:39-43` `apply_json`
- `tests/community_resolution_knob.rs:62-66` `apply_json`
- `tests/concepts_fold_periphery.rs:42-46` `apply_json`
- `tests/design_intent_events.rs:38-42` `apply_json`

#### `dup-0519` (near, 2 sites)

Proposed home: `code_ingest_events::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_ingest_events.rs:241-304` `real_extraction_tiers_every_structural_edge_through_the_emit_fold_pipeline`
- `tests/code_ingest_events.rs:308-378` `real_extraction_folds_caller_attributed_calls_edges_at_every_tier`

#### `dup-0520` (near, 2 sites)

Proposed home: `code_ingest_events::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_ingest_events.rs:382-451` `re_extracting_a_file_that_drops_a_call_supersedes_its_calls_edge_end_to_end`
- `tests/code_ingest_events.rs:711-793` `re_extracting_a_changed_file_supersedes_its_removed_symbols_end_to_end`

#### `dup-0521` (near, 9 sites)

Proposed home: `a new shared module (sites span 5 files: tests/code_ingest_events.rs, tests/community_detection_pass.rs, tests/community_fold_periphery.rs, tests/community_resolution_knob.rs, tests/concepts_fold_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_ingest_events.rs:896-903` `apply_def_json`
- `tests/code_ingest_events.rs:1066-1073` `apply_ref_fresh`
- `tests/community_detection_pass.rs:39-46` `def`
- `tests/community_detection_pass.rs:51-58` `call`
- `tests/community_fold_periphery.rs:48-55` `entity`
- `tests/community_fold_periphery.rs:59-66` `call`
- `tests/community_resolution_knob.rs:70-77` `def`
- `tests/community_resolution_knob.rs:82-89` `call`
- `tests/concepts_fold_periphery.rs:61-68` `realized`

#### `dup-0522` (near, 2 sites)

Proposed home: `code_ingest_events::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_ingest_events.rs:1129-1170` `a_refs_only_files_re_extraction_supersedes_via_the_reference_batch_boundary`
- `tests/code_ingest_events.rs:1173-1227` `a_re_extraction_supersedes_only_its_own_files_edges_not_another_files_reference`

#### `dup-0523` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/code_lens_overview_collapse_viz.rs, tests/subject_lens_overlay_served_page.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_lens_overview_collapse_viz.rs:74-88` `build_harness`
- `tests/subject_lens_overlay_served_page.rs:550-564` `build_additive_harness`

#### `dup-0524` (semantic, 5 sites)

Proposed home: `one shared `build_harness` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 5 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/code_lens_overview_collapse_viz.rs:74-88` `build_harness`
- `tests/metadata_card_handoff_viz.rs:123-138` `build_harness`
- `tests/proof_row_renders_on_the_card.rs:76-91` `build_harness`
- `tests/subject_lens_overlay_client_arms.rs:82-97` `build_harness`
- `tests/subject_view_memory_rail_client.rs:100-115` `build_harness`

#### `dup-0525` (exact, 6 sites)

Proposed home: `a new shared module (sites span 6 files: tests/code_lens_overview_collapse_viz.rs, tests/metadata_card_handoff_viz.rs, tests/proof_row_renders_on_the_card.rs, tests/subject_lens_overlay_client_arms.rs, tests/subject_lens_overlay_served_page.rs, tests/subject_view_memory_rail_client.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_lens_overview_collapse_viz.rs:92-118` `run_node_harness`
- `tests/metadata_card_handoff_viz.rs:141-168` `run_node_harness`
- `tests/proof_row_renders_on_the_card.rs:94-121` `run_node_harness`
- `tests/subject_lens_overlay_client_arms.rs:101-127` `run_node_harness`
- `tests/subject_lens_overlay_served_page.rs:59-85` `run_node_harness`
- `tests/subject_view_memory_rail_client.rs:119-145` `run_node_harness`

#### `dup-0526` (semantic, 6 sites)

Proposed home: `one shared `run_node_harness` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 6 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/code_lens_overview_collapse_viz.rs:92-118` `run_node_harness`
- `tests/metadata_card_handoff_viz.rs:141-168` `run_node_harness`
- `tests/proof_row_renders_on_the_card.rs:94-121` `run_node_harness`
- `tests/subject_lens_overlay_client_arms.rs:101-127` `run_node_harness`
- `tests/subject_lens_overlay_served_page.rs:59-85` `run_node_harness`
- `tests/subject_view_memory_rail_client.rs:119-145` `run_node_harness`

#### `dup-0527` (exact, 6 sites)

Proposed home: `a new shared module (sites span 4 files: tests/code_lens_overview_collapse_viz.rs, tests/metadata_card_handoff_viz.rs, tests/proof_row_renders_on_the_card.rs, tests/subject_lens_overlay_served_page.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_lens_overview_collapse_viz.rs:200-213` `the_overview_collapses_to_sized_labelled_community_super_nodes_purely`
- `tests/metadata_card_handoff_viz.rs:297-310` `metadata_card_renders_every_taxonomy_and_chips_hand_off_to_their_own_lens`
- `tests/metadata_card_handoff_viz.rs:383-396` `metadata_card_wiring_fires_at_every_render_and_drill_call_site`
- `tests/proof_row_renders_on_the_card.rs:181-194` `proof_row_renders_count_evidence_and_the_explicit_empty_state`
- `tests/subject_lens_overlay_served_page.rs:777-790` `a_neighborhood_rationale_badge_click_expands_and_does_not_reseed`
- `tests/subject_lens_overlay_served_page.rs:797-810` `the_drill_view_is_byte_identical_with_the_overlay_off`

#### `dup-0528` (near, 8 sites)

Proposed home: `a new shared module (sites span 6 files: tests/code_lens_view_periphery.rs, tests/concepts_lens_view_periphery.rs, tests/subject_lens_defined_cells.rs, tests/subject_lens_defined_cells_contract.rs, tests/subject_lens_reprojection_contract.rs, tests/subject_lens_reprojection_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_lens_view_periphery.rs:71-79` `community`
- `tests/concepts_lens_view_periphery.rs:96-104` `concept`
- `tests/subject_lens_defined_cells.rs:48-56` `def`
- `tests/subject_lens_defined_cells_contract.rs:37-45` `def`
- `tests/subject_lens_reprojection_contract.rs:41-49` `def`
- `tests/subject_lens_reprojection_contract.rs:77-85` `decision`
- `tests/subject_lens_reprojection_periphery.rs:61-69` `def`
- `tests/subject_lens_reprojection_periphery.rs:83-91` `super_node`

#### `dup-0529` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/code_lens_view_periphery.rs, tests/concepts_lens_view_periphery.rs, tests/files_lens_view_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_lens_view_periphery.rs:83-89` `plain`
- `tests/concepts_lens_view_periphery.rs:108-114` `plain`
- `tests/files_lens_view_periphery.rs:91-97` `plain`

#### `dup-0530` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/code_lens_view_periphery.rs, tests/concepts_lens_view_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_lens_view_periphery.rs:120-146` `lens_graph`
- `tests/concepts_lens_view_periphery.rs:140-167` `lens_graph`

#### `dup-0531` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/code_lens_view_periphery.rs, tests/concepts_lens_view_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_lens_view_periphery.rs:149-153` `code_default`
- `tests/concepts_lens_view_periphery.rs:170-174` `concepts_default`

#### `dup-0532` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/code_lens_view_periphery.rs, tests/concepts_lens_view_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_lens_view_periphery.rs:163-196` `lens_from_query_is_a_public_total_selector_that_falls_back_to_files`
- `tests/concepts_lens_view_periphery.rs:185-213` `lens_from_query_is_a_public_total_selector_including_concepts`

#### `dup-0533` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/code_lens_view_periphery.rs, tests/concepts_lens_view_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_lens_view_periphery.rs:209-258` `code_lens_overview_buckets_code_entities_by_community_and_excludes_every_other_kind`
- `tests/concepts_lens_view_periphery.rs:226-279` `concepts_lens_overview_buckets_members_by_concept_across_directories_and_excludes_membershipless_nodes`

#### `dup-0534` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/code_lens_view_periphery.rs, tests/concepts_lens_view_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_lens_view_periphery.rs:416-437` `code_lens_at_an_underived_grain_carries_the_documented_empty_state_not_an_error`
- `tests/concepts_lens_view_periphery.rs:463-484` `concepts_lens_at_an_underived_grain_carries_the_documented_empty_state_not_an_error`

#### `dup-0535` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/code_lens_view_periphery.rs, tests/concepts_lens_view_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_lens_view_periphery.rs:558-578` `served`
- `tests/concepts_lens_view_periphery.rs:538-558` `served`

#### `dup-0536` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/code_lens_view_periphery.rs, tests/concepts_lens_view_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_lens_view_periphery.rs:581-585` `served_json`
- `tests/concepts_lens_view_periphery.rs:561-565` `served_json`

#### `dup-0537` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/community_detection_cli.rs, tests/concepts_derivation_cli.rs, tests/projections_stay_local.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_detection_cli.rs:55-67` `project`
- `tests/concepts_derivation_cli.rs:60-72` `project`
- `tests/projections_stay_local.rs:195-206` `server_project`

#### `dup-0538` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_detection_cli.rs, tests/concepts_derivation_cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_detection_cli.rs:70-75` `rigger_db`
- `tests/concepts_derivation_cli.rs:75-80` `rigger_db`

#### `dup-0539` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_detection_cli.rs, tests/concepts_derivation_cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_detection_cli.rs:78-87` `communities`
- `tests/concepts_derivation_cli.rs:83-92` `concepts`

#### `dup-0540` (near, 4 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_detection_cli.rs, tests/concepts_derivation_cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_detection_cli.rs:92-100` `def`
- `tests/community_detection_cli.rs:105-113` `call`
- `tests/concepts_derivation_cli.rs:96-104` `doc`
- `tests/concepts_derivation_cli.rs:109-114` `link`

#### `dup-0541` (semantic, 2 sites)

Proposed home: `one shared `seed_coupling` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/community_detection_cli.rs:122-159` `seed_coupling`
- `tests/community_fold_periphery.rs:100-110` `seed_coupling`

#### `dup-0542` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_detection_cli.rs, tests/concepts_derivation_cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_detection_cli.rs:164-183` `community_layer`
- `tests/concepts_derivation_cli.rs:195-214` `concept_layer`

#### `dup-0543` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_detection_cli.rs, tests/concepts_derivation_cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_detection_cli.rs:186-191` `member_of`
- `tests/concepts_derivation_cli.rs:217-222` `member_of`

#### `dup-0544` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_detection_cli.rs, tests/concepts_derivation_cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_detection_cli.rs:265-299` `an_empty_project_records_no_community_and_still_succeeds`
- `tests/concepts_derivation_cli.rs:317-351` `an_empty_project_records_no_concept_and_still_succeeds`

#### `dup-0545` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_detection_cli.rs, tests/concepts_derivation_cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_detection_cli.rs:302-372` `resolution_grains_coexist_and_a_rerun_supersedes_only_its_own_grain`
- `tests/concepts_derivation_cli.rs:354-424` `resolution_grains_coexist_and_a_rerun_supersedes_only_its_own_grain`

#### `dup-0546` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_detection_cli.rs, tests/concepts_derivation_cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_detection_cli.rs:375-411` `a_malformed_resolution_or_unknown_argument_fails_loudly`
- `tests/concepts_derivation_cli.rs:427-463` `a_malformed_resolution_or_unknown_argument_fails_loudly`

#### `dup-0547` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_detection_cli.rs, tests/concepts_derivation_cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_detection_cli.rs:414-457` `re_running_a_grain_reproduces_the_byte_identical_live_layer`
- `tests/concepts_derivation_cli.rs:466-508` `re_running_a_grain_reproduces_the_byte_identical_live_layer`

#### `dup-0548` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_detection_pass.rs, tests/community_resolution_knob.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_detection_pass.rs:67-95` `seed`
- `tests/community_resolution_knob.rs:96-123` `seed`

#### `dup-0549` (semantic, 2 sites)

Proposed home: `one shared `community_snapshot` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/community_detection_pass.rs:100-115` `community_snapshot`
- `tests/community_fold_periphery.rs:80-95` `community_snapshot`

#### `dup-0550` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_fold_periphery.rs, tests/concepts_fold_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_fold_periphery.rs:69-75` `live_memberships`
- `tests/concepts_fold_periphery.rs:71-77` `live_realizes`

#### `dup-0551` (semantic, 2 sites)

Proposed home: `one shared `live_memberships` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/community_fold_periphery.rs:69-75` `live_memberships`
- `tests/community_resolution_knob.rs:180-189` `live_memberships`

#### `dup-0552` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_fold_periphery.rs, tests/concepts_fold_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_fold_periphery.rs:230-263` `a_community_assigned_event_without_a_fresh_key_is_a_non_boundary_and_never_supersedes`
- `tests/concepts_fold_periphery.rs:218-262` `a_concept_derived_event_without_a_fresh_key_is_a_non_boundary_and_never_supersedes`

#### `dup-0553` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_fold_periphery.rs, tests/concepts_fold_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_fold_periphery.rs:266-300` `the_community_layer_serialized_literals_are_stable_and_the_fold_matches_the_on_log_type`
- `tests/concepts_fold_periphery.rs:265-306` `the_concept_layer_serialized_literals_are_stable_and_the_fold_matches_the_on_log_type`

#### `dup-0554` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_resolution_knob.rs, tests/graph_superseded_prune.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_resolution_knob.rs:214-223` `live_memberships_of`
- `tests/graph_superseded_prune.rs:59-68` `live_contains`

#### `dup-0555` (near, 2 sites)

Proposed home: `concepts_labels_membership::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/concepts_labels_membership.rs:136-178` `label_is_the_most_central_document_by_intent_degree`
- `tests/concepts_labels_membership.rs:181-212` `label_ties_break_to_the_lexicographically_smallest_document`

#### `dup-0556` (near, 2 sites)

Proposed home: `concepts_labels_membership::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/concepts_labels_membership.rs:262-289` `a_documentless_concept_falls_back_to_its_most_central_members_name`
- `tests/concepts_labels_membership.rs:292-317` `a_documentless_concept_with_no_named_member_falls_back_to_the_most_central_members_id`

#### `dup-0557` (near, 13 sites)

Proposed home: `a new shared module (sites span 9 files: tests/concepts_lens_view_periphery.rs, tests/dash_calls_render_viz.rs, tests/dash_decisions_progressive_disclosure.rs, tests/dash_graph_exploration_viz.rs, tests/dash_kg_graph_route.rs, tests/files_lens_directory_hulls_viz.rs, tests/readable_graph_adaptive_labels.rs, tests/readable_graph_density_scaled_spacing.rs, tests/readable_graph_layout_separation.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/concepts_lens_view_periphery.rs:809-843` `the_concepts_drill_renders_the_shared_marker_to_the_human`
- `tests/dash_calls_render_viz.rs:429-465` `the_directed_call_render_lays_out_and_dispatches_the_layered_dag`
- `tests/dash_decisions_progressive_disclosure.rs:346-382` `an_operator_expanded_decision_survives_the_live_poll_re_render`
- `tests/dash_decisions_progressive_disclosure.rs:456-492` `the_summary_preview_collapses_a_multiline_summary_to_one_truncated_line`
- `tests/dash_graph_exploration_viz.rs:358-394` `the_exploration_viz_lays_out_and_dispatches_overview_drill_and_back`
- `tests/dash_kg_graph_route.rs:443-479` `selecting_a_node_seeds_the_kg_panel_and_it_survives_the_live_poll`
- `tests/dash_kg_graph_route.rs:884-920` `a_god_node_renders_a_badge_and_a_shift_click_traces_the_query_path`
- `tests/dash_kg_graph_route.rs:1308-1344` `toggling_a_tier_hides_that_tiers_edges_and_the_explain_provenance_renders`
- `tests/files_lens_directory_hulls_viz.rs:156-192` `the_files_lens_draws_directory_hulls_behind_its_file_nodes`
- `tests/files_lens_directory_hulls_viz.rs:290-326` `the_reprojection_view_draws_directory_hulls_behind_its_file_clusters`
- `tests/readable_graph_adaptive_labels.rs:257-295` `adaptive_labels_declutter_by_importance_and_reveal_on_zoom`
- `tests/readable_graph_density_scaled_spacing.rs:219-255` `the_layout_extent_scales_with_density_and_edges_are_drawable`
- `tests/readable_graph_layout_separation.rs:206-242` `the_layout_leaves_no_collision_body_overlap_at_density`

#### `dup-0558` (near, 4 sites)

Proposed home: `a new shared module (sites span 4 files: tests/confidence_tier_blast_radius.rs, tests/criteria_delivery_periphery.rs, tests/gate_store_fence_periphery.rs, tests/run_scoping_survives_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/confidence_tier_blast_radius.rs:52-63` `spawn`
- `tests/criteria_delivery_periphery.rs:43-54` `spawn`
- `tests/gate_store_fence_periphery.rs:802-813` `spawn`
- `tests/run_scoping_survives_periphery.rs:64-75` `spawn`

#### `dup-0559` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/confidence_tier_blast_radius.rs, tests/unified_traversal_grounding.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/confidence_tier_blast_radius.rs:110-115` `fold`
- `tests/unified_traversal_grounding.rs:182-187` `fold`

#### `dup-0560` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/courier_registry_refresh_boundary_periphery.rs, tests/registry_refresh_driver_courier_convergence_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/courier_registry_refresh_boundary_periphery.rs:37-64` `courier_project_with_commit`
- `tests/registry_refresh_driver_courier_convergence_periphery.rs:38-82` `driver_project`

#### `dup-0561` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/courier_registry_refresh_boundary_periphery.rs, tests/courier_registry_refresh_periphery.rs, tests/registry_refresh_driver_courier_convergence_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/courier_registry_refresh_boundary_periphery.rs:68-77` `run_rigger`
- `tests/courier_registry_refresh_periphery.rs:58-67` `run_rigger`
- `tests/registry_refresh_driver_courier_convergence_periphery.rs:97-107` `run_rigger`

#### `dup-0562` (exact, 4 sites)

Proposed home: `a new shared module (sites span 4 files: tests/courier_registry_refresh_boundary_periphery.rs, tests/courier_registry_refresh_fence_periphery.rs, tests/courier_registry_refresh_periphery.rs, tests/registry_refresh_driver_courier_convergence_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/courier_registry_refresh_boundary_periphery.rs:79-86` `assert_ok`
- `tests/courier_registry_refresh_fence_periphery.rs:73-80` `assert_ok`
- `tests/courier_registry_refresh_periphery.rs:69-76` `assert_ok`
- `tests/registry_refresh_driver_courier_convergence_periphery.rs:109-116` `assert_ok`

#### `dup-0563` (near, 5 sites)

Proposed home: `a new shared module (sites span 5 files: tests/courier_registry_refresh_boundary_periphery.rs, tests/courier_registry_refresh_fence_periphery.rs, tests/courier_registry_refresh_periphery.rs, tests/gate_store_fence_periphery.rs, tests/registry_refresh_driver_courier_convergence_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/courier_registry_refresh_boundary_periphery.rs:91-109` `registry_entries`
- `tests/courier_registry_refresh_fence_periphery.rs:53-71` `registry_entries`
- `tests/courier_registry_refresh_periphery.rs:81-99` `registry_entries`
- `tests/gate_store_fence_periphery.rs:253-271` `registry_entries`
- `tests/registry_refresh_driver_courier_convergence_periphery.rs:122-140` `registry_entries`

#### `dup-0564` (semantic, 5 sites)

Proposed home: `one shared `registry_entries` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 5 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/courier_registry_refresh_boundary_periphery.rs:91-109` `registry_entries`
- `tests/courier_registry_refresh_fence_periphery.rs:53-71` `registry_entries`
- `tests/courier_registry_refresh_periphery.rs:81-99` `registry_entries`
- `tests/gate_store_fence_periphery.rs:253-271` `registry_entries`
- `tests/registry_refresh_driver_courier_convergence_periphery.rs:122-140` `registry_entries`

#### `dup-0565` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/courier_registry_refresh_boundary_periphery.rs, tests/courier_registry_refresh_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/courier_registry_refresh_boundary_periphery.rs:263-283` `an_ambient_kurrentdb_conn_never_leaks_into_a_boundary_courier`
- `tests/courier_registry_refresh_periphery.rs:308-333` `an_ambient_kurrentdb_conn_never_leaks_into_a_courier_spawned_through_the_shared_helper`

#### `dup-0566` (semantic, 2 sites)

Proposed home: `one shared `courier_project` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/courier_registry_refresh_fence_periphery.rs:37-48` `courier_project`
- `tests/courier_registry_refresh_periphery.rs:35-52` `courier_project`

#### `dup-0567` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/courier_registry_refresh_periphery.rs, tests/store_content_identity_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/courier_registry_refresh_periphery.rs:35-52` `courier_project`
- `tests/store_content_identity_periphery.rs:1339-1358` `cli_project`

#### `dup-0568` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/dash_cluster_detail_drill.rs, tests/dash_exploration_route_client_contract.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dash_cluster_detail_drill.rs:107-109` `spoke_id`
- `tests/dash_exploration_route_client_contract.rs:103-105` `spoke`

#### `dup-0569` (near, 6 sites)

Proposed home: `a new shared module (sites span 6 files: tests/dash_decisions_progressive_disclosure.rs, tests/dash_kg_graph_route.rs, tests/dash_whole_projection_reach.rs, tests/proof_lands_on_the_card_periphery.rs, tests/rationale_overlay_data.rs, tests/rationale_overlay_seam.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dash_decisions_progressive_disclosure.rs:45-105` `try_fetch_served_root_page`
- `tests/dash_kg_graph_route.rs:110-164` `try_fetch_served`
- `tests/dash_whole_projection_reach.rs:254-314` `try_fetch_whole_served`
- `tests/proof_lands_on_the_card_periphery.rs:773-831` `try_fetch_served`
- `tests/rationale_overlay_data.rs:103-154` `try_fetch_served`
- `tests/rationale_overlay_seam.rs:65-117` `try_fetch_served`

#### `dup-0570` (semantic, 2 sites)

Proposed home: `one shared `exploration_graph` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/dash_exploration_route_client_contract.rs:116-146` `exploration_graph`
- `tests/dash_kg_graph_route.rs:1415-1463` `exploration_graph`

#### `dup-0571` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/dash_exploration_route_client_contract.rs, tests/subject_lens_reprojection_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dash_exploration_route_client_contract.rs:151-172` `served_body`
- `tests/subject_lens_reprojection_periphery.rs:330-351` `served_json`

#### `dup-0572` (near, 2 sites)

Proposed home: `dash_graph_exploration_fold::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dash_graph_exploration_fold.rs:37-62` `cluster_key_is_reachable_over_the_public_crate_boundary`
- `tests/dash_graph_exploration_fold.rs:69-146` `cluster_key_honors_the_boundary_edges_of_the_names_a_file_predicate`

#### `dup-0573` (semantic, 2 sites)

Proposed home: `one shared `fixture_graph` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/dash_kg_graph_route.rs:39-69` `fixture_graph`
- `tests/metadata_card_periphery.rs:69-105` `fixture_graph`

#### `dup-0574` (semantic, 4 sites)

Proposed home: `one shared `try_fetch_served` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 4 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/dash_kg_graph_route.rs:110-164` `try_fetch_served`
- `tests/proof_lands_on_the_card_periphery.rs:773-831` `try_fetch_served`
- `tests/rationale_overlay_data.rs:103-154` `try_fetch_served`
- `tests/rationale_overlay_seam.rs:65-117` `try_fetch_served`

#### `dup-0575` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/dash_kg_graph_route.rs, tests/rationale_overlay_data.rs, tests/rationale_overlay_seam.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dash_kg_graph_route.rs:170-179` `fetch_served`
- `tests/rationale_overlay_data.rs:158-167` `fetch_served`
- `tests/rationale_overlay_seam.rs:121-130` `fetch_served`

#### `dup-0576` (semantic, 4 sites)

Proposed home: `one shared `fetch_served` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 4 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/dash_kg_graph_route.rs:170-179` `fetch_served`
- `tests/proof_lands_on_the_card_periphery.rs:835-844` `fetch_served`
- `tests/rationale_overlay_data.rs:158-167` `fetch_served`
- `tests/rationale_overlay_seam.rs:121-130` `fetch_served`

#### `dup-0577` (exact, 5 sites)

Proposed home: `a new shared module (sites span 5 files: tests/dash_kg_graph_route.rs, tests/dash_whole_projection_reach.rs, tests/proof_lands_on_the_card_periphery.rs, tests/rationale_overlay_data.rs, tests/rationale_overlay_seam.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dash_kg_graph_route.rs:182-186` `body_of`
- `tests/dash_whole_projection_reach.rs:329-333` `body_of`
- `tests/proof_lands_on_the_card_periphery.rs:848-852` `body_of`
- `tests/rationale_overlay_data.rs:170-174` `body_of`
- `tests/rationale_overlay_seam.rs:135-139` `body_of`

#### `dup-0578` (near, 2 sites)

Proposed home: `dash_kg_graph_route::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dash_kg_graph_route.rs:264-321` `the_served_root_page_ships_the_kg_panel_and_select_to_seed_wiring`
- `tests/dash_kg_graph_route.rs:740-784` `the_served_root_page_renders_god_nodes_and_the_query_path`

#### `dup-0579` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/dash_release_ready.rs, tests/dash_run_tree_spine.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dash_release_ready.rs:104-112` `connect_with_retry`
- `tests/dash_run_tree_spine.rs:282-290` `connect_with_retry`

#### `dup-0580` (semantic, 2 sites)

Proposed home: `one shared `connect_with_retry` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/dash_release_ready.rs:104-112` `connect_with_retry`
- `tests/dash_run_tree_spine.rs:282-290` `connect_with_retry`

#### `dup-0581` (near, 4 sites)

Proposed home: `dash_run_tree_spine::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dash_run_tree_spine.rs:541-581` `an_escalated_unit_renders_gates_failed_and_surfaces_at_the_spec_root`
- `tests/dash_run_tree_spine.rs:591-631` `an_escalated_units_gate_failure_is_not_masked_by_a_trailing_passing_gate`
- `tests/dash_run_tree_spine.rs:710-761` `a_regated_green_unit_renders_gates_passed_despite_an_earlier_failed_attempt`
- `tests/dash_run_tree_spine.rs:775-812` `a_review_rejected_unit_whose_gates_passed_renders_gates_passed_and_surfaces_the_reject`

#### `dup-0582` (near, 2 sites)

Proposed home: `dash_run_tree_spine::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dash_run_tree_spine.rs:820-850` `a_pre_gate_unit_whose_implementer_finished_does_not_render_gates_failed`
- `tests/dash_run_tree_spine.rs:861-898` `a_gates_cleared_unit_with_no_recorded_verdict_still_renders_gates_passed`

#### `dup-0583` (exact, 7 sites)

Proposed home: `a new shared module (sites span 7 files: tests/dead_code_json_contract_periphery.rs, tests/duplication_catalog_contract_periphery.rs, tests/gitsemver_path_inclusion_accounting_periphery.rs, tests/handbook_grounder_accuracy.rs, tests/prioritized_plan_citation_periphery.rs, tests/responsibility_map_contract_periphery.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dead_code_json_contract_periphery.rs:228-230` `repo_root`
- `tests/duplication_catalog_contract_periphery.rs:100-102` `repo_root`
- `tests/gitsemver_path_inclusion_accounting_periphery.rs:52-54` `repo_root`
- `tests/handbook_grounder_accuracy.rs:33-35` `repo_root`
- `tests/prioritized_plan_citation_periphery.rs:65-67` `repo_root`
- `tests/responsibility_map_contract_periphery.rs:76-78` `repo_root`
- `tests/simplification_audit.rs:2010-2012` `repo_root`

#### `dup-0584` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/dead_code_json_contract_periphery.rs, tests/duplication_catalog_contract_periphery.rs, tests/responsibility_map_contract_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dead_code_json_contract_periphery.rs:232-236` `read_committed_dead_code_raw`
- `tests/duplication_catalog_contract_periphery.rs:104-108` `read_committed_catalog_raw`
- `tests/responsibility_map_contract_periphery.rs:80-84` `read_committed_map_raw`

#### `dup-0585` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/dead_code_json_contract_periphery.rs, tests/duplication_catalog_contract_periphery.rs, tests/responsibility_map_contract_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dead_code_json_contract_periphery.rs:238-247` `deserialize_committed_dead_code`
- `tests/duplication_catalog_contract_periphery.rs:110-119` `deserialize_committed_catalog`
- `tests/responsibility_map_contract_periphery.rs:86-94` `deserialize_committed_map`

#### `dup-0586` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/dead_code_json_contract_periphery.rs, tests/duplication_catalog_contract_periphery.rs, tests/responsibility_map_contract_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dead_code_json_contract_periphery.rs:254-261` `the_committed_dead_code_json_deserializes_as_a_downstream_consumer_would`
- `tests/duplication_catalog_contract_periphery.rs:126-134` `the_committed_duplication_catalog_deserializes_as_a_downstream_consumer_would`
- `tests/responsibility_map_contract_periphery.rs:101-108` `the_committed_responsibility_map_deserializes_as_a_downstream_consumer_would`

#### `dup-0587` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/dead_code_json_contract_periphery.rs, tests/duplication_catalog_contract_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dead_code_json_contract_periphery.rs:336-353` `every_deserialized_test_only_reference_has_a_non_empty_file_and_content_hash`
- `tests/duplication_catalog_contract_periphery.rs:171-195` `every_deserialized_site_has_a_non_empty_file_name_and_content_hash`

#### `dup-0588` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/dead_code_json_contract_periphery.rs, tests/duplication_catalog_contract_periphery.rs, tests/responsibility_map_contract_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dead_code_json_contract_periphery.rs:374-381` `deserialize_committed_dead_code_lines`
- `tests/duplication_catalog_contract_periphery.rs:357-364` `deserialize_committed_catalog_lines`
- `tests/responsibility_map_contract_periphery.rs:275-282` `deserialize_committed_map_lines`

#### `dup-0589` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/dead_code_json_contract_periphery.rs, tests/duplication_catalog_contract_periphery.rs, tests/responsibility_map_contract_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dead_code_json_contract_periphery.rs:447-460` `deserializing_then_reserializing_the_committed_dead_code_json_reproduces_the_committed_bytes_exactly`
- `tests/duplication_catalog_contract_periphery.rs:314-326` `deserializing_then_reserializing_the_committed_catalog_reproduces_the_committed_bytes_exactly`
- `tests/responsibility_map_contract_periphery.rs:241-253` `deserializing_then_reserializing_reproduces_the_committed_bytes_exactly`

#### `dup-0590` (near, 4 sites)

Proposed home: `dead_code_json_contract_periphery::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dead_code_json_contract_periphery.rs:593-610` `generic_impl_header_constructors_previously_false_flagged_are_absent_from_the_committed_file`
- `tests/dead_code_json_contract_periphery.rs:629-657` `value_position_and_ufcs_reference_shapes_previously_invisible_are_absent_from_the_committed_file`
- `tests/dead_code_json_contract_periphery.rs:670-683` `the_general_ufcs_method_value_fix_also_closes_previously_unreported_same_class_instances`
- `tests/dead_code_json_contract_periphery.rs:699-715` `getter_methods_kept_alive_only_by_a_same_named_production_field_or_local_are_also_absent`

#### `dup-0591` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/dead_code_json_contract_periphery.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dead_code_json_contract_periphery.rs:776-796` `the_committed_dead_code_json_disposition_split_is_22_delete_3_keep_pending_0_keep_public_surface`
- `tests/simplification_audit.rs:10666-10686` `the_real_tree_disposition_split_matches_this_criterions_research`

#### `dup-0592` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/dead_code_json_contract_periphery.rs, tests/responsibility_map_contract_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dead_code_json_contract_periphery.rs:867-887` `section_4_3_citations`
- `tests/responsibility_map_contract_periphery.rs:318-339` `section_1_citations`

#### `dup-0593` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/dedup_seeding_periphery.rs, tests/published_content_key_split_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dedup_seeding_periphery.rs:390-398` `minted`
- `tests/published_content_key_split_periphery.rs:400-408` `minted`

#### `dup-0594` (near, 2 sites)

Proposed home: `design_intent_events::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/design_intent_events.rs:209-255` `the_public_emit_lowers_every_concept_kind_onto_the_fold_arm_that_matches_it`
- `tests/design_intent_events.rs:413-456` `the_public_link_emit_lowers_every_link_rel_onto_the_fold_arm_that_matches_it`

#### `dup-0595` (near, 2 sites)

Proposed home: `design_intent_events::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/design_intent_events.rs:796-859` `every_recognized_end_user_usage_shape_is_dropped_before_the_fold`
- `tests/design_intent_events.rs:965-1054` `a_design_word_in_a_non_handbook_usage_doc_does_not_leak_the_handbook_content_keep`

#### `dup-0596` (near, 4 sites)

Proposed home: `escalation_resume_periphery::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/escalation_resume_periphery.rs:376-384` `resume_unit_rejects_a_non_numeric_attempts_value`
- `tests/escalation_resume_periphery.rs:388-396` `resume_unit_rejects_a_zero_attempts_value`
- `tests/escalation_resume_periphery.rs:401-409` `resume_unit_rejects_a_dangling_attempts_flag_with_no_value`
- `tests/escalation_resume_periphery.rs:414-422` `resume_unit_rejects_an_unknown_flag`

#### `dup-0597` (exact, 2 sites)

Proposed home: `fanout_template_needs_and_stage_retries_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/fanout_template_needs_and_stage_retries_periphery.rs:210-218` `write_git_worker_agent`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:223-231` `write_repoless_worker_agent`

#### `dup-0598` (near, 2 sites)

Proposed home: `fanout_template_needs_and_stage_retries_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/fanout_template_needs_and_stage_retries_periphery.rs:831-937` `checkin_stays_unready_while_a_real_split_siblings_partner_has_not_integrated_yet`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:946-1031` `checkin_never_becomes_ready_when_a_real_split_siblings_partner_escalates_instead`

#### `dup-0599` (near, 2 sites)

Proposed home: `fanout_template_needs_and_stage_retries_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/fanout_template_needs_and_stage_retries_periphery.rs:1250-1299` `a_stages_own_max_retries_yaml_key_lowers_the_effective_bound_below_a_higher_default`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:1308-1391` `a_stages_own_max_retries_yaml_key_raises_the_effective_bound_above_a_lower_default`

#### `dup-0600` (semantic, 2 sites)

Proposed home: `one shared `init_repo_with_head` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/gate_store_fence_periphery.rs:491-507` `init_repo_with_head`
- `tests/spawn_target_dir_periphery.rs:55-71` `init_repo_with_head`

#### `dup-0601` (near, 2 sites)

Proposed home: `gate_store_fence_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/gate_store_fence_periphery.rs:526-597` `a_real_fenced_couriers_scratch_store_is_reclaimed_when_the_worktree_is_removed`
- `tests/gate_store_fence_periphery.rs:603-700` `a_real_fenced_couriers_scratch_store_is_reclaimed_for_a_review_worktree_too`

#### `dup-0602` (semantic, 3 sites)

Proposed home: `one shared `gitsemver_available` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/gitsemver_derivation.rs:83-89` `gitsemver_available`
- `tests/gitsemver_worktree_periphery.rs:117-123` `gitsemver_available`
- `tests/validate_behind_the_tree_periphery.rs:139-145` `gitsemver_available`

#### `dup-0603` (exact, 2 sites)

Proposed home: `gitsemver_derivation::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/gitsemver_derivation.rs:92-110` `a_plain_commit_after_a_tag_increments_the_patch`
- `tests/gitsemver_derivation.rs:113-131` `a_feat_commit_after_a_tag_increments_the_minor`

#### `dup-0604` (near, 5 sites)

Proposed home: `a new shared module (sites span 5 files: tests/gitsemver_path_inclusion_accounting_periphery.rs, tests/hermetic_test_git_audit.rs, tests/no_os_kill_audit.rs, tests/reap_before_removal_audit.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/gitsemver_path_inclusion_accounting_periphery.rs:58-71` `collect_rs_files`
- `tests/hermetic_test_git_audit.rs:342-355` `collect_rs_files`
- `tests/no_os_kill_audit.rs:324-337` `collect_rs_files`
- `tests/reap_before_removal_audit.rs:761-774` `collect_rs_files`
- `tests/simplification_audit.rs:2069-2082` `collect_rs_files`

#### `dup-0605` (semantic, 5 sites)

Proposed home: `one shared `collect_rs_files` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 5 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/gitsemver_path_inclusion_accounting_periphery.rs:58-71` `collect_rs_files`
- `tests/hermetic_test_git_audit.rs:342-355` `collect_rs_files`
- `tests/no_os_kill_audit.rs:324-337` `collect_rs_files`
- `tests/reap_before_removal_audit.rs:761-774` `collect_rs_files`
- `tests/simplification_audit.rs:2069-2082` `collect_rs_files`

#### `dup-0606` (exact, 2 sites)

Proposed home: `gitsemver_path_inclusion_accounting_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/gitsemver_path_inclusion_accounting_periphery.rs:176-180` `a_real_top_level_path_attribute_line_is_recognized`
- `tests/gitsemver_path_inclusion_accounting_periphery.rs:183-187` `an_indented_path_attribute_line_is_still_recognized`

#### `dup-0607` (exact, 2 sites)

Proposed home: `gitsemver_path_inclusion_accounting_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/gitsemver_path_inclusion_accounting_periphery.rs:190-196` `a_doc_comment_quoting_the_attribute_as_prose_is_not_mistaken_for_a_real_site`
- `tests/gitsemver_path_inclusion_accounting_periphery.rs:199-203` `a_line_comment_quoting_the_attribute_as_prose_is_not_mistaken_for_a_real_site`

#### `dup-0608` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/graph_collision_body_and_tiebreak.rs, tests/graph_density_spread_floor_and_centring.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_collision_body_and_tiebreak.rs:98-118` `run_driver`
- `tests/graph_density_spread_floor_and_centring.rs:104-124` `run_driver`

#### `dup-0609` (exact, 2 sites)

Proposed home: `graph_collision_body_and_tiebreak::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_collision_body_and_tiebreak.rs:215-236` `the_collision_body_encloses_the_circle_and_its_label`
- `tests/graph_collision_body_and_tiebreak.rs:242-263` `the_separation_pass_resolves_coincident_nodes_deterministically`

#### `dup-0610` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/graph_denoise_content_survives.rs, tests/graph_denoise_target_project.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_denoise_content_survives.rs:35-42` `fold`
- `tests/graph_denoise_target_project.rs:36-43` `fold`

#### `dup-0611` (near, 2 sites)

Proposed home: `graph_denoise_target_project::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_denoise_target_project.rs:46-198` `a_whole_runs_fold_projects_the_content_but_none_of_the_machinery`
- `tests/graph_denoise_target_project.rs:201-268` `the_actor_metadata_on_a_decision_and_finding_never_folds_an_agent_node`

#### `dup-0612` (near, 4 sites)

Proposed home: `a new shared module (sites span 4 files: tests/graph_density_spread_floor_and_centring.rs, tests/readable_graph_adaptive_labels.rs, tests/readable_graph_density_scaled_spacing.rs, tests/readable_graph_layout_separation.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_density_spread_floor_and_centring.rs:131-138` `the_served_page_wires_the_layered_path_without_the_density_accessors`
- `tests/readable_graph_adaptive_labels.rs:72-98` `the_served_page_ships_the_adaptive_label_declutter`
- `tests/readable_graph_density_scaled_spacing.rs:63-80` `the_served_page_ships_the_density_spread_lever`
- `tests/readable_graph_layout_separation.rs:56-76` `the_served_page_ships_the_collision_separation_pass`

#### `dup-0613` (exact, 3 sites)

Proposed home: `graph_density_spread_floor_and_centring::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_density_spread_floor_and_centring.rs:184-199` `the_spread_factor_floors_at_one_off_the_dense_path`
- `tests/graph_density_spread_floor_and_centring.rs:259-275` `the_bare_four_arg_layout_stays_panel_sized_and_the_accessor_path_grows_past_it`
- `tests/graph_density_spread_floor_and_centring.rs:323-339` `the_enlarged_canvas_is_centred_on_the_panel_middle`

#### `dup-0614` (semantic, 2 sites)

Proposed home: `one shared `apply_governs` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/graph_fold_dedup_live_edge.rs:38-46` `apply_governs`
- `tests/graph_rebuild_collapses_dupes.rs:40-48` `apply_governs`

#### `dup-0615` (exact, 4 sites)

Proposed home: `a new shared module (sites span 4 files: tests/graph_fold_dedup_live_edge.rs, tests/graph_rebuild_collapses_dupes.rs, tests/graph_superseded_prune.rs, tests/reset_menu_previews_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_fold_dedup_live_edge.rs:51-53` `nanos`
- `tests/graph_rebuild_collapses_dupes.rs:53-55` `nanos`
- `tests/graph_superseded_prune.rs:53-55` `nanos`
- `tests/reset_menu_previews_periphery.rs:71-73` `nanos`

#### `dup-0616` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/graph_fold_dedup_live_edge.rs, tests/graph_rebuild_collapses_dupes.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_fold_dedup_live_edge.rs:57-65` `governs`
- `tests/graph_rebuild_collapses_dupes.rs:59-67` `governs`

#### `dup-0617` (near, 2 sites)

Proposed home: `graph_fold_dedup_live_edge::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_fold_dedup_live_edge.rs:68-106` `subgraph_collapses_repeated_governs_to_one_live_edge_keeping_latest_provenance`
- `tests/graph_fold_dedup_live_edge.rs:109-130` `the_collapsed_edge_keeps_the_earliest_fact_time_and_latest_source_regardless_of_arrival_order`

#### `dup-0618` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/graph_show_periphery.rs, tests/graph_show_staleness.rs, tests/graph_show_surface.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_show_periphery.rs:61-77` `run_stream_identity`
- `tests/graph_show_staleness.rs:49-65` `run_stream_identity`
- `tests/graph_show_surface.rs:48-64` `run_stream_identity`

#### `dup-0619` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/graph_show_periphery.rs, tests/graph_show_staleness.rs, tests/graph_show_surface.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_show_periphery.rs:80-82` `seed_rigger_dir`
- `tests/graph_show_staleness.rs:68-70` `seed_rigger_dir`
- `tests/graph_show_surface.rs:67-69` `seed_rigger_dir`

#### `dup-0620` (semantic, 3 sites)

Proposed home: `one shared `seed_rigger_dir` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/graph_show_periphery.rs:80-82` `seed_rigger_dir`
- `tests/graph_show_staleness.rs:68-70` `seed_rigger_dir`
- `tests/graph_show_surface.rs:67-69` `seed_rigger_dir`

#### `dup-0621` (near, 5 sites)

Proposed home: `a new shared module (sites span 5 files: tests/graph_show_periphery.rs, tests/graph_show_staleness.rs, tests/graph_show_surface.rs, tests/relocated_worktree_store_resolution_periphery.rs, tests/validate_footprint_default_scratch_root_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_show_periphery.rs:88-102` `run_rigger`
- `tests/graph_show_staleness.rs:96-110` `run_rigger`
- `tests/graph_show_surface.rs:75-89` `run_rigger`
- `tests/relocated_worktree_store_resolution_periphery.rs:102-116` `run_rigger`
- `tests/validate_footprint_default_scratch_root_periphery.rs:48-62` `run_rigger`

#### `dup-0622` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/graph_show_periphery.rs, tests/graph_show_staleness.rs, tests/graph_show_surface.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_show_periphery.rs:109-124` `seed_def_lang`
- `tests/graph_show_staleness.rs:83-90` `seed_def`
- `tests/graph_show_surface.rs:94-101` `seed_def`

#### `dup-0623` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/graph_show_periphery.rs, tests/graph_show_staleness.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_show_periphery.rs:132-135` `open_graph`
- `tests/graph_show_staleness.rs:73-76` `open_graph`

#### `dup-0624` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/graph_show_periphery.rs, tests/graph_show_staleness.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_show_periphery.rs:141-150` `body_line_count`
- `tests/graph_show_staleness.rs:116-125` `body_line_count`

#### `dup-0625` (semantic, 2 sites)

Proposed home: `one shared `body_line_count` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/graph_show_periphery.rs:141-150` `body_line_count`
- `tests/graph_show_staleness.rs:116-125` `body_line_count`

#### `dup-0626` (semantic, 2 sites)

Proposed home: `one shared `assert_light_lane_extent_note` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/graph_show_periphery.rs:157-171` `assert_light_lane_extent_note`
- `tests/graph_show_surface.rs:109-118` `assert_light_lane_extent_note`

#### `dup-0627` (near, 13 sites)

Proposed home: `a new shared module (sites span 2 files: tests/graph_show_periphery.rs, tests/graph_show_staleness.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_show_periphery.rs:216-271` `graph_show_degrades_to_stale_note_when_location_drifted`
- `tests/graph_show_periphery.rs:281-323` `graph_show_degrades_to_stale_note_when_recorded_line_is_zero`
- `tests/graph_show_periphery.rs:353-383` `graph_show_light_lane_degrades_to_extent_unavailable_note`
- `tests/graph_show_periphery.rs:392-444` `graph_show_bounds_body_at_the_definitions_own_extent`
- `tests/graph_show_periphery.rs:456-531` `graph_show_shows_full_body_past_nested_definition`
- `tests/graph_show_periphery.rs:542-590` `graph_show_shows_full_body_of_a_destructuring_signature`
- `tests/graph_show_periphery.rs:598-640` `graph_show_extent_ignores_braces_in_strings_comments_and_chars`
- `tests/graph_show_periphery.rs:650-712` `graph_show_shows_full_body_of_a_python_nested_def`
- `tests/graph_show_periphery.rs:723-763` `graph_show_does_not_overread_a_js_single_quote_brace_body`
- `tests/graph_show_periphery.rs:833-886` `graph_show_degrades_when_no_grammar_registered_for_the_file_extension`
- `tests/graph_show_periphery.rs:899-959` `graph_show_degrades_when_no_definition_of_that_name_at_the_recorded_line`
- `tests/graph_show_staleness.rs:132-176` `graph_show_degrades_gracefully_when_the_recorded_file_is_missing`
- `tests/graph_show_staleness.rs:190-260` `graph_show_never_presents_a_neighbours_body_when_the_line_drifted`

#### `dup-0628` (semantic, 2 sites)

Proposed home: `one shared `git_toplevel` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/heartbeat_write_read_agree_periphery.rs:68-78` `git_toplevel`
- `tests/reset_derived_live_writer_guard_periphery.rs:57-68` `git_toplevel`

#### `dup-0629` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/heartbeat_write_read_agree_periphery.rs, tests/relocated_worktree_store_resolution_periphery.rs, tests/reset_derived_live_writer_guard_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/heartbeat_write_read_agree_periphery.rs:128-146` `run_stream_identity`
- `tests/relocated_worktree_store_resolution_periphery.rs:67-79` `stream_identity`
- `tests/reset_derived_live_writer_guard_periphery.rs:73-87` `run_stream_identity`

#### `dup-0630` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/heartbeat_write_read_agree_periphery.rs, tests/watchdog_cli_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/heartbeat_write_read_agree_periphery.rs:189-194` `now_nanos`
- `tests/watchdog_cli_periphery.rs:208-213` `now_nanos`

#### `dup-0631` (near, 2 sites)

Proposed home: `integrate_conflict_merge_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/integrate_conflict_merge_periphery.rs:504-581` `spawn`
- `tests/integrate_conflict_merge_periphery.rs:1421-1478` `spawn`

#### `dup-0632` (near, 2 sites)

Proposed home: `integrate_conflict_merge_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/integrate_conflict_merge_periphery.rs:1003-1039` `spawn`
- `tests/integrate_conflict_merge_periphery.rs:1224-1270` `spawn`

#### `dup-0633` (near, 3 sites)

Proposed home: `integrate_conflict_merge_periphery::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/integrate_conflict_merge_periphery.rs:1864-1889` `spawn`
- `tests/integrate_conflict_merge_periphery.rs:2414-2436` `spawn`
- `tests/integrate_conflict_merge_periphery.rs:2659-2679` `spawn`

#### `dup-0634` (near, 2 sites)

Proposed home: `integrate_conflict_merge_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/integrate_conflict_merge_periphery.rs:2109-2114` `has_status_marker`
- `tests/integrate_conflict_merge_periphery.rs:2116-2124` `count_status_marker`

#### `dup-0635` (near, 2 sites)

Proposed home: `integrate_conflict_merge_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/integrate_conflict_merge_periphery.rs:2141-2164` `spawn`
- `tests/integrate_conflict_merge_periphery.rs:2279-2299` `spawn`

#### `dup-0636` (near, 4 sites)

Proposed home: `integrate_conflict_merge_periphery::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/integrate_conflict_merge_periphery.rs:2169-2266` `a_crash_right_after_the_merge_attempt_record_resumes_and_completes_row_1`
- `tests/integrate_conflict_merge_periphery.rs:2304-2400` `a_crash_right_after_the_landing_intent_record_resumes_and_completes_row_4`
- `tests/integrate_conflict_merge_periphery.rs:2908-3008` `a_crash_right_after_the_merge_succeeds_resumes_and_completes_row_1_after_record`
- `tests/integrate_conflict_merge_periphery.rs:3018-3116` `a_crash_right_after_landing_succeeds_resumes_and_completes_row_4_after_record`

#### `dup-0637` (exact, 2 sites)

Proposed home: `integrate_conflict_merge_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/integrate_conflict_merge_periphery.rs:2682-2707` `confined_cfg`
- `tests/integrate_conflict_merge_periphery.rs:3174-3199` `mixed_cfg`

#### `dup-0638` (near, 3 sites)

Proposed home: `integrate_conflict_merge_periphery::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/integrate_conflict_merge_periphery.rs:2714-2874` `a_confined_regenerate_command_failure_and_a_store_failure_each_resume_and_complete_row_3`
- `tests/integrate_conflict_merge_periphery.rs:3208-3310` `a_regenerate_command_failure_right_after_landing_completes_row_3_on_resume_when_row_4_is_already_closed`
- `tests/integrate_conflict_merge_periphery.rs:3323-3443` `a_crash_right_after_landing_succeeds_with_owed_regeneration_completes_row_3_on_resume`

#### `dup-0639` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/kurrentdb_always_available.rs, tests/readme_retirement_rationale.rs, tests/turbovec_retired.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/kurrentdb_always_available.rs:28-32` `manifest_text`
- `tests/readme_retirement_rationale.rs:31-34` `readme_text`
- `tests/turbovec_retired.rs:34-38` `manifest_text`

#### `dup-0640` (semantic, 2 sites)

Proposed home: `one shared `manifest_text` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/kurrentdb_always_available.rs:28-32` `manifest_text`
- `tests/turbovec_retired.rs:34-38` `manifest_text`

#### `dup-0641` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/kurrentdb_always_available.rs, tests/turbovec_retired.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/kurrentdb_always_available.rs:37-52` `table_lines`
- `tests/turbovec_retired.rs:43-58` `table_lines`

#### `dup-0642` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/kurrentdb_always_available.rs, tests/turbovec_retired.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/kurrentdb_always_available.rs:57-65` `table_declares_key`
- `tests/turbovec_retired.rs:63-71` `table_declares_key`

#### `dup-0643` (semantic, 2 sites)

Proposed home: `one shared `table_declares_key` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/kurrentdb_always_available.rs:57-65` `table_declares_key`
- `tests/turbovec_retired.rs:63-71` `table_declares_key`

#### `dup-0644` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/kurrentdb_always_available.rs, tests/turbovec_retired.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/kurrentdb_always_available.rs:164-179` `for_each_rs_file`
- `tests/turbovec_retired.rs:75-90` `for_each_rs_file`

#### `dup-0645` (semantic, 2 sites)

Proposed home: `one shared `for_each_rs_file` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/kurrentdb_always_available.rs:164-179` `for_each_rs_file`
- `tests/turbovec_retired.rs:75-90` `for_each_rs_file`

#### `dup-0646` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/kurrentdb_always_available.rs, tests/turbovec_retired.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/kurrentdb_always_available.rs:190-209` `no_source_still_gates_on_the_retired_kurrentdb_feature`
- `tests/turbovec_retired.rs:168-186` `no_source_still_gates_on_the_retired_turbovec_feature`

#### `dup-0647` (semantic, 5 sites)

Proposed home: `one shared `js_declaration` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 5 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/meta_phases_declaration_periphery.rs:65-87` `js_declaration`
- `tests/phase_of_role_mapping_periphery.rs:50-72` `js_declaration`
- `tests/review_tier_roster_periphery.rs:18-40` `js_declaration`
- `tests/step_attention_periphery.rs:655-677` `js_declaration`
- `tests/worker_persona_label_periphery.rs:21-43` `js_declaration`

#### `dup-0648` (near, 4 sites)

Proposed home: `a new shared module (sites span 4 files: tests/metadata_card_handoff_viz.rs, tests/proof_row_renders_on_the_card.rs, tests/subject_lens_overlay_client_arms.rs, tests/subject_view_memory_rail_client.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/metadata_card_handoff_viz.rs:123-138` `build_harness`
- `tests/proof_row_renders_on_the_card.rs:76-91` `build_harness`
- `tests/subject_lens_overlay_client_arms.rs:82-97` `build_harness`
- `tests/subject_view_memory_rail_client.rs:100-115` `build_harness`

#### `dup-0649` (near, 8 sites)

Proposed home: `a new shared module (sites span 8 files: tests/migration_is_deliberate_periphery.rs, tests/reset_derived_compaction.rs, tests/reset_derived_compaction_periphery.rs, tests/store_precedence.rs, tests/store_resolution_cli.rs, tests/store_secrets.rs, tests/validate_advisories.rs, tests/validate_behind_the_tree_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/migration_is_deliberate_periphery.rs:448-456` `temp_project`
- `tests/reset_derived_compaction.rs:39-47` `temp_project`
- `tests/reset_derived_compaction_periphery.rs:600-608` `temp_project`
- `tests/store_precedence.rs:47-55` `empty_project`
- `tests/store_resolution_cli.rs:54-62` `empty_project`
- `tests/store_secrets.rs:50-58` `empty_project`
- `tests/validate_advisories.rs:47-55` `temp_project`
- `tests/validate_behind_the_tree_periphery.rs:66-74` `temp_project`

#### `dup-0650` (near, 2 sites)

Proposed home: `migration_is_deliberate_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/migration_is_deliberate_periphery.rs:512-539` `seed_a_retired_entity`
- `tests/migration_is_deliberate_periphery.rs:543-563` `seed_a_live_entity`

#### `dup-0651` (semantic, 4 sites)

Proposed home: `one shared `sigterm_ignorer_in` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 4 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/mutation_scratch_reap_base_guard_periphery.rs:54-61` `sigterm_ignorer_in`
- `tests/reap_before_removal_periphery.rs:41-48` `sigterm_ignorer_in`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:138-145` `sigterm_ignorer_in`
- `tests/worktree_remove_relocated_scratch_base_guard_periphery.rs:55-62` `sigterm_ignorer_in`

#### `dup-0652` (exact, 2 sites)

Proposed home: `no_os_kill_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_audit.rs:65-67` `pkill_word`
- `tests/no_os_kill_audit.rs:71-73` `xkill_word`

#### `dup-0653` (exact, 2 sites)

Proposed home: `no_os_kill_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_audit.rs:68-70` `killall_word`
- `tests/no_os_kill_audit.rs:74-76` `pg_signal_word`

#### `dup-0654` (exact, 2 sites)

Proposed home: `no_os_kill_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_audit.rs:80-82` `libc_kill_open`
- `tests/no_os_kill_audit.rs:83-85` `signal_kill_open`

#### `dup-0655` (exact, 4 sites)

Proposed home: `no_os_kill_audit::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_audit.rs:175-177` `shape_pg_signal`
- `tests/no_os_kill_audit.rs:180-182` `shape_libc_kill`
- `tests/no_os_kill_audit.rs:185-187` `shape_signal_kill`
- `tests/no_os_kill_audit.rs:191-193` `shape_direct_rustix_call`

#### `dup-0656` (near, 2 sites)

Proposed home: `no_os_kill_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_audit.rs:196-216` `shape_arg_dashdash`
- `tests/no_os_kill_audit.rs:220-240` `shape_format_dash_brace`

#### `dup-0657` (exact, 2 sites)

Proposed home: `no_os_kill_audit::finding`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_audit.rs:259-268` `fmt`
- `tests/reap_before_removal_audit.rs:131-140` `fmt`

#### `dup-0658` (near, 2 sites)

Proposed home: `no_os_kill_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_audit.rs:273-303` `general_hits`
- `tests/no_os_kill_audit.rs:309-321` `sanctioned_hits`

#### `dup-0659` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/no_os_kill_audit.rs, tests/reap_before_removal_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_audit.rs:382-388` `write_file`
- `tests/reap_before_removal_audit.rs:845-851` `write_file`

#### `dup-0660` (near, 11 sites)

Proposed home: `no_os_kill_audit::support (consolidate these 11 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_audit.rs:412-426` `command_new_shell_out_is_caught_outside_the_sanctioned_files`
- `tests/no_os_kill_audit.rs:429-436` `bare_shell_kill_dash_form_is_caught`
- `tests/no_os_kill_audit.rs:439-446` `standalone_pkill_token_is_caught`
- `tests/no_os_kill_audit.rs:449-456` `pg_signal_call_name_is_caught`
- `tests/no_os_kill_audit.rs:459-466` `libc_kill_call_is_caught_outside_the_sanctioned_files`
- `tests/no_os_kill_audit.rs:469-476` `signal_kill_call_is_caught_outside_the_sanctioned_files`
- `tests/no_os_kill_audit.rs:479-492` `kill_process_call_is_caught_outside_the_sanctioned_files`
- `tests/no_os_kill_audit.rs:517-531` `a_finding_names_its_exact_file_and_line_number`
- `tests/no_os_kill_audit.rs:534-551` `kill_process_is_never_flagged_inside_either_sanctioned_file`
- `tests/no_os_kill_audit.rs:554-568` `a_shell_out_inside_a_sanctioned_file_is_still_caught`
- `tests/no_os_kill_audit.rs:601-614` `a_shape_outside_src_and_tests_is_never_scanned`

#### `dup-0661` (near, 4 sites)

Proposed home: `no_os_kill_audit::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_audit.rs:495-503` `arg_dashdash_separator_is_caught_outside_the_sanctioned_files`
- `tests/no_os_kill_audit.rs:506-514` `negative_pid_format_is_caught_outside_the_sanctioned_files`
- `tests/no_os_kill_audit.rs:571-583` `a_dashdash_separator_inside_a_sanctioned_file_is_still_caught`
- `tests/no_os_kill_audit.rs:586-598` `a_negative_pid_format_inside_a_sanctioned_file_is_still_caught`

#### `dup-0662` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/no_os_kill_audit.rs, tests/reap_before_removal_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_audit.rs:622-635` `the_real_tree_carries_no_forbidden_pattern`
- `tests/reap_before_removal_audit.rs:1725-1738` `the_real_tree_carries_no_bare_removal`

#### `dup-0663` (exact, 4 sites)

Proposed home: `no_os_kill_test_helper_periphery::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_test_helper_periphery.rs:176-180` `terminate_pid_refuses_pid_zero`
- `tests/no_os_kill_test_helper_periphery.rs:184-194` `terminate_pid_refuses_pid_one`
- `tests/no_os_kill_test_helper_periphery.rs:213-218` `stop_pid_refuses_pid_zero`
- `tests/no_os_kill_test_helper_periphery.rs:222-229` `stop_pid_refuses_pid_one`

#### `dup-0664` (exact, 2 sites)

Proposed home: `no_os_kill_test_helper_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_test_helper_periphery.rs:198-200` `terminate_pid_refuses_its_callers_own_pid`
- `tests/no_os_kill_test_helper_periphery.rs:233-235` `stop_pid_refuses_its_callers_own_pid`

#### `dup-0665` (near, 2 sites)

Proposed home: `parallel_ordered_emit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/parallel_ordered_emit.rs:71-77` `drive_default`
- `tests/parallel_ordered_emit.rs:80-89` `drive_paced`

#### `dup-0666` (near, 3 sites)

Proposed home: `phase_of_role_mapping_periphery::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/phase_of_role_mapping_periphery.rs:123-138` `plan_and_plan_critique_resolve_to_plan_regardless_of_role`
- `tests/phase_of_role_mapping_periphery.rs:148-163` `review_tier_roles_resolve_to_review`
- `tests/phase_of_role_mapping_periphery.rs:172-187` `implementer_and_any_unrecognized_role_resolve_to_the_fail_visible_build_default`

#### `dup-0667` (semantic, 2 sites)

Proposed home: `one shared `production_main_rs` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/projections_stay_local.rs:38-48` `production_main_rs`
- `tests/store_resolution.rs:28-32` `production_main_rs`

#### `dup-0668` (semantic, 2 sites)

Proposed home: `one shared `start_kurrentdb` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/projections_stay_local.rs:161-191` `start_kurrentdb`
- `tests/store_resolution.rs:176-207` `start_kurrentdb`

#### `dup-0669` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/projections_stay_local.rs, tests/store_resolution.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/projections_stay_local.rs:269-346` `progress_against_the_server_keeps_progress_db_local_and_the_log_on_the_server`
- `tests/store_resolution.rs:223-298` `a_courier_in_a_project_configured_for_the_server_resolves_the_server_store`

#### `dup-0670` (near, 3 sites)

Proposed home: `proof_lands_on_the_card_periphery::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/proof_lands_on_the_card_periphery.rs:354-405` `cross_file_proof_survives_a_real_reextraction_of_the_defining_file`
- `tests/proof_lands_on_the_card_periphery.rs:642-685` `a_deleted_test_reference_retracts_its_stale_proof_through_the_real_pipeline`
- `tests/proof_lands_on_the_card_periphery.rs:716-762` `a_reference_free_tests_dir_files_first_extraction_creates_nothing_and_leaves_no_residue`

#### `dup-0671` (exact, 15 sites)

Proposed home: `reap_before_removal_audit::support (consolidate these 15 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reap_before_removal_audit.rs:854-871` `a_clean_fixture_tree_yields_no_findings`
- `tests/reap_before_removal_audit.rs:915-935` `a_reap_call_anywhere_earlier_in_the_enclosing_function_covers_the_removal`
- `tests/reap_before_removal_audit.rs:995-1012` `a_real_reap_call_with_a_trailing_comment_on_the_same_line_still_covers`
- `tests/reap_before_removal_audit.rs:1045-1062` `an_exemption_marker_in_a_trailing_comment_after_real_code_still_covers`
- `tests/reap_before_removal_audit.rs:1124-1142` `a_real_reap_call_with_a_single_line_block_comment_on_the_same_line_still_covers`
- `tests/reap_before_removal_audit.rs:1190-1217` `a_reap_call_still_covers_a_bare_fallback_removal_across_an_intervening_worktree_remove_attempt_of_the_same_dir`
- `tests/reap_before_removal_audit.rs:1220-1238` `the_exemption_marker_covers_a_removal_with_no_reap_call`
- `tests/reap_before_removal_audit.rs:1241-1259` `the_exemption_marker_in_the_functions_own_doc_comment_above_the_signature_also_covers_it`
- `tests/reap_before_removal_audit.rs:1311-1330` `a_removal_inside_a_cfg_test_mod_block_is_never_scanned`
- `tests/reap_before_removal_audit.rs:1358-1378` `stacked_cfg_attributes_before_a_test_mod_still_exclude_it`
- `tests/reap_before_removal_audit.rs:1426-1442` `a_bare_removal_under_tests_is_never_scanned`
- `tests/reap_before_removal_audit.rs:1465-1486` `a_multi_line_fn_signature_still_resolves_its_own_closing_brace`
- `tests/reap_before_removal_audit.rs:1522-1541` `an_exemption_marker_after_the_removal_still_covers_it`
- `tests/reap_before_removal_audit.rs:1607-1627` `a_non_empty_authorized_root_argument_still_covers_even_when_the_call_wraps_across_lines`
- `tests/reap_before_removal_audit.rs:1672-1691` `a_worktree_token_with_no_nearby_remove_token_is_never_mistaken_for_the_shape`

#### `dup-0672` (near, 11 sites)

Proposed home: `reap_before_removal_audit::support (consolidate these 11 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reap_before_removal_audit.rs:874-890` `bare_remove_dir_all_with_no_coverage_is_caught`
- `tests/reap_before_removal_audit.rs:893-912` `bare_git_worktree_remove_with_no_coverage_is_caught`
- `tests/reap_before_removal_audit.rs:942-961` `a_reap_authority_name_in_a_prose_comment_never_covers_the_removal`
- `tests/reap_before_removal_audit.rs:1151-1176` `a_reap_call_covering_one_removal_never_bleeds_onto_a_later_unrelated_removal_in_the_same_function`
- `tests/reap_before_removal_audit.rs:1283-1308` `a_reap_call_only_in_a_sibling_function_never_covers_this_one`
- `tests/reap_before_removal_audit.rs:1333-1355` `a_removal_inside_a_standalone_cfg_test_fn_is_never_scanned_and_a_later_real_fn_still_is`
- `tests/reap_before_removal_audit.rs:1402-1423` `a_semicolon_terminated_cfg_test_item_excludes_only_itself`
- `tests/reap_before_removal_audit.rs:1445-1462` `a_finding_names_its_exact_file_and_line_number`
- `tests/reap_before_removal_audit.rs:1495-1515` `a_reap_call_after_the_removal_never_covers_it_remove_then_reap_is_still_flagged`
- `tests/reap_before_removal_audit.rs:1549-1573` `an_exemption_marker_attached_to_one_removal_never_covers_an_unrelated_second_removal`
- `tests/reap_before_removal_audit.rs:1635-1665` `a_worktree_remove_args_array_wrapped_across_multiple_lines_by_rustfmt_is_still_caught`

#### `dup-0673` (exact, 7 sites)

Proposed home: `reap_before_removal_audit::support (consolidate these 7 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reap_before_removal_audit.rs:969-989` `a_reap_authority_name_in_a_trailing_comment_never_covers_the_removal`
- `tests/reap_before_removal_audit.rs:1020-1039` `an_exemption_marker_inside_a_non_comment_string_literal_never_covers_the_removal`
- `tests/reap_before_removal_audit.rs:1071-1090` `a_reap_authority_name_inside_a_non_comment_string_literal_never_covers_the_removal`
- `tests/reap_before_removal_audit.rs:1099-1118` `a_reap_authority_name_inside_a_single_line_block_comment_never_covers_the_removal`
- `tests/reap_before_removal_audit.rs:1262-1280` `an_arbitrary_comment_is_never_mistaken_for_the_exemption_marker`
- `tests/reap_before_removal_audit.rs:1381-1399` `a_doc_comment_mentioning_the_cfg_test_attribute_in_prose_is_never_mistaken_for_it`
- `tests/reap_before_removal_audit.rs:1581-1600` `a_literal_empty_string_authorized_root_argument_never_covers_the_removal`

#### `dup-0674` (near, 5 sites)

Proposed home: `reap_before_removal_periphery::support (consolidate these 5 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reap_before_removal_periphery.rs:80-135` `worktree_remove_reaps_a_process_rooted_in_its_sibling_build_cache_before_reclaiming_it`
- `tests/reap_before_removal_periphery.rs:155-203` `worktree_remove_reaps_a_process_rooted_in_its_sibling_mutants_root_before_reclaiming_it`
- `tests/reap_before_removal_periphery.rs:206-266` `discard_reaps_a_process_rooted_in_the_review_worktrees_fence_sibling_before_reclaiming_it`
- `tests/reap_before_removal_periphery.rs:269-329` `worktree_remove_reaps_a_process_rooted_in_the_cache_dirs_own_store_fence_sibling_before_reclaiming_it`
- `tests/reap_before_removal_periphery.rs:332-393` `discard_reaps_a_process_rooted_in_the_review_worktree_itself_before_clearing_it`

#### `dup-0675` (near, 2 sites)

Proposed home: `reminder_dedup_workflow_child_env_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reminder_dedup_workflow_child_env_periphery.rs:83-115` `workflow_stamps_its_own_pid_on_the_spawned_child_with_no_inbound_sentinel`
- `tests/reminder_dedup_workflow_child_env_periphery.rs:124-159` `workflow_still_stamps_a_fresh_own_pid_on_the_child_even_when_its_own_reminder_was_suppressed`

#### `dup-0676` (exact, 2 sites)

Proposed home: `replan_episode_identity::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/replan_episode_identity.rs:141-151` `new`
- `tests/replan_episode_identity.rs:404-414` `new`

#### `dup-0677` (near, 2 sites)

Proposed home: `replan_episode_identity::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/replan_episode_identity.rs:176-231` `spawn`
- `tests/replan_episode_identity.rs:418-469` `spawn`

#### `dup-0678` (near, 2 sites)

Proposed home: `replan_episode_identity::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/replan_episode_identity.rs:305-384` `a_replan_after_a_critique_reject_supersedes_the_initial_episodes_unit`
- `tests/replan_episode_identity.rs:483-561` `a_second_replan_supersedes_both_earlier_episodes_units`

#### `dup-0679` (near, 3 sites)

Proposed home: `replan_episode_identity::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/replan_episode_identity.rs:753-838` `a_same_id_refine_survives_its_own_episodes_new_sibling_through_the_real_write_path`
- `tests/replan_episode_identity.rs:854-939` `a_same_id_refine_survives_its_own_episodes_new_sibling_walked_first_through_the_real_write_path`
- `tests/replan_episode_identity.rs:954-1044` `a_same_id_refine_survives_its_own_episodes_genuinely_new_unmatched_sibling_through_the_real_write_path`

#### `dup-0680` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/reset_build_cache_periphery.rs, tests/reset_menu.rs, tests/reset_menu_identity_migration_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reset_build_cache_periphery.rs:54-57` `seed_store`
- `tests/reset_menu.rs:81-84` `seed_store`
- `tests/reset_menu_identity_migration_periphery.rs:69-72` `seed_store`

#### `dup-0681` (exact, 2 sites)

Proposed home: `reset_build_cache_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reset_build_cache_periphery.rs:63-65` `shared_cache_dir`
- `tests/reset_build_cache_periphery.rs:67-69` `guard_path`

#### `dup-0682` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/reset_derived_compaction.rs, tests/reset_derived_compaction_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reset_derived_compaction.rs:131-157` `rows`
- `tests/reset_derived_compaction_periphery.rs:116-139` `raw_rows`

#### `dup-0683` (semantic, 2 sites)

Proposed home: `one shared `edge_inferred` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/reset_derived_compaction.rs:183-185` `edge_inferred`
- `tests/reset_menu_previews_periphery.rs:88-91` `edge_inferred`

#### `dup-0684` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/reset_derived_compaction.rs, tests/reset_derived_compaction_periphery.rs, tests/reset_menu_previews_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reset_derived_compaction.rs:193-197` `keyed`
- `tests/reset_derived_compaction_periphery.rs:163-167` `keyed`
- `tests/reset_menu_previews_periphery.rs:75-79` `keyed`

#### `dup-0685` (semantic, 2 sites)

Proposed home: `one shared `path_subject_of` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/reset_derived_compaction_periphery.rs:2110-2125` `path_subject_of`
- `tests/store_content_identity_periphery.rs:233-249` `path_subject_of`

#### `dup-0686` (near, 3 sites)

Proposed home: `reset_derived_live_writer_guard_periphery::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reset_derived_live_writer_guard_periphery.rs:158-168` `reset_derived_prunes_when_no_run_has_ever_started`
- `tests/reset_derived_live_writer_guard_periphery.rs:731-741` `reset_force_live_alone_is_refused_as_no_mode`
- `tests/reset_derived_live_writer_guard_periphery.rs:747-762` `the_derived_help_entry_documents_force_live_and_owns_the_risk`

#### `dup-0687` (near, 4 sites)

Proposed home: `reset_derived_live_writer_guard_periphery::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reset_derived_live_writer_guard_periphery.rs:174-197` `reset_derived_prunes_when_every_unit_is_terminal_and_every_spawn_is_answered`
- `tests/reset_derived_live_writer_guard_periphery.rs:203-229` `reset_derived_ignores_a_prior_runs_unanswered_spawn_and_non_terminal_unit`
- `tests/reset_derived_live_writer_guard_periphery.rs:645-665` `reset_derived_force_live_compacts_despite_an_in_flight_spawn`
- `tests/reset_derived_live_writer_guard_periphery.rs:693-722` `runs_composed_with_a_refused_derived_still_completes_its_own_prune`

#### `dup-0688` (near, 2 sites)

Proposed home: `reset_derived_live_writer_guard_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reset_derived_live_writer_guard_periphery.rs:333-364` `reset_derived_refuses_a_non_terminal_unit_between_spawn_rounds_and_prunes_nothing`
- `tests/reset_derived_live_writer_guard_periphery.rs:373-402` `reset_derived_refuses_an_in_flight_spawn_naming_its_id_and_prunes_nothing`

#### `dup-0689` (near, 2 sites)

Proposed home: `reset_derived_live_writer_guard_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reset_derived_live_writer_guard_periphery.rs:531-557` `reset_derived_ignores_a_registration_for_a_different_store`
- `tests/reset_derived_live_writer_guard_periphery.rs:576-616` `reset_derived_never_deletes_a_stale_foreign_registry_entrys_file`

#### `dup-0690` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/reset_menu.rs, tests/reset_menu_identity_migration_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reset_menu.rs:100-103` `emit`
- `tests/reset_menu_identity_migration_periphery.rs:88-91` `emit`

#### `dup-0691` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/reset_menu.rs, tests/reset_menu_identity_migration_periphery.rs, tests/reset_menu_previews_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reset_menu.rs:144-149` `code_entity`
- `tests/reset_menu_identity_migration_periphery.rs:113-118` `code_entity`
- `tests/reset_menu_previews_periphery.rs:81-86` `code_entity`

#### `dup-0692` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/reset_menu.rs, tests/reset_menu_identity_migration_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reset_menu.rs:153-170` `seed_derived_duplicates`
- `tests/reset_menu_identity_migration_periphery.rs:120-137` `seed_derived_duplicates`

#### `dup-0693` (semantic, 2 sites)

Proposed home: `one shared `seed_derived_duplicates` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/reset_menu.rs:153-170` `seed_derived_duplicates`
- `tests/reset_menu_identity_migration_periphery.rs:120-137` `seed_derived_duplicates`

#### `dup-0694` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/review_tier_roster_periphery.rs, tests/worker_persona_label_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/review_tier_roster_periphery.rs:75-129` `run_worker_label_for_unit_and_reviews`
- `tests/worker_persona_label_periphery.rs:72-112` `run_worker_label_for_unit`

#### `dup-0695` (exact, 2 sites)

Proposed home: `review_tier_roster_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/review_tier_roster_periphery.rs:135-148` `the_adversarys_roster_renders_inside_its_action_phrase`
- `tests/review_tier_roster_periphery.rs:154-167` `the_adjudicators_roster_renders_inside_its_action_phrase`

#### `dup-0696` (exact, 2 sites)

Proposed home: `review_tier_roster_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/review_tier_roster_periphery.rs:211-224` `a_roster_on_a_role_with_no_roster_verb_entry_is_never_rendered`
- `tests/review_tier_roster_periphery.rs:229-242` `a_single_entry_roster_renders_with_no_stray_separator`

#### `dup-0697` (near, 2 sites)

Proposed home: `rigger_run_base_gate_env_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/rigger_run_base_gate_env_periphery.rs:209-233` `rigger_run_base_reaches_a_real_inline_gate_subprocess_but_not_a_real_agent_subprocess`
- `tests/rigger_run_base_gate_env_periphery.rs:236-254` `rigger_run_base_reaches_a_real_deferred_gate_subprocess_too`

#### `dup-0698` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/scratch_workdir_config.rs, tests/store_config.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/scratch_workdir_config.rs:30-35` `rigger_dir`
- `tests/store_config.rs:32-37` `rigger_dir`

#### `dup-0699` (near, 4 sites)

Proposed home: `a new shared module (sites span 3 files: tests/scratch_workdir_config.rs, tests/store_config.rs, tests/store_precedence.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/scratch_workdir_config.rs:37-39` `write_workflow`
- `tests/store_config.rs:40-42` `write_workflow`
- `tests/store_precedence.rs:64-66` `write_store_conn`
- `tests/store_precedence.rs:78-80` `write_store_config`

#### `dup-0700` (exact, 2 sites)

Proposed home: `scratch_workdir_config::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/scratch_workdir_config.rs:53-58` `a_present_workdir_deserializes_exactly`
- `tests/scratch_workdir_config.rs:77-83` `a_workflow_with_no_defaults_block_at_all_reads_as_empty`

#### `dup-0701` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:1745-1754` `map_entry_wire`
- `tests/simplification_audit.rs:1756-1763` `map_entry_lines`

#### `dup-0702` (exact, 6 sites)

Proposed home: `simplification_audit::support (consolidate these 6 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:1770-1775` `map_to_json`
- `tests/simplification_audit.rs:1780-1785` `map_lines_to_json`
- `tests/simplification_audit.rs:3268-3273` `catalog_to_json`
- `tests/simplification_audit.rs:3278-3283` `catalog_lines_to_json`
- `tests/simplification_audit.rs:6327-6333` `dead_code_to_json`
- `tests/simplification_audit.rs:6338-6344` `dead_code_lines_to_json`

#### `dup-0703` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:3180-3183` `real_files`
- `tests/simplification_audit.rs:3187-3190` `real_catalog`

#### `dup-0704` (near, 4 sites)

Proposed home: `simplification_audit::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:3231-3247` `dup_cluster_wire`
- `tests/simplification_audit.rs:3249-3262` `dup_cluster_lines`
- `tests/simplification_audit.rs:6285-6304` `dead_code_candidate_wire`
- `tests/simplification_audit.rs:6306-6321` `dead_code_candidate_lines`

#### `dup-0705` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:3554-3716` `render_section_3`
- `tests/simplification_audit.rs:4113-4356` `render_section_5`

#### `dup-0706` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:3919-4106` `render_section_4`
- `tests/simplification_audit.rs:4445-4994` `render_section_6`

#### `dup-0707` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:6411-6420` `a_simple_free_function_is_found_with_its_line_span`
- `tests/simplification_audit.rs:6651-6657` `production_functions_before_a_cfg_test_mod_are_not_flagged_test`

#### `dup-0708` (exact, 11 sites)

Proposed home: `simplification_audit::support (consolidate these 11 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:6441-6446` `fnv1a_is_not_mistaken_for_the_fn_keyword`
- `tests/simplification_audit.rs:6453-6458` `a_brace_inside_a_line_comment_is_ignored`
- `tests/simplification_audit.rs:6461-6466` `a_brace_inside_a_block_comment_is_ignored`
- `tests/simplification_audit.rs:6469-6474` `nested_block_comments_are_handled`
- `tests/simplification_audit.rs:6477-6482` `a_brace_inside_a_string_literal_is_ignored`
- `tests/simplification_audit.rs:6485-6490` `a_brace_inside_a_raw_string_with_hashes_is_ignored`
- `tests/simplification_audit.rs:6493-6498` `a_brace_inside_a_byte_string_is_ignored`
- `tests/simplification_audit.rs:6501-6506` `a_brace_char_literal_is_not_mistaken_for_real_braces`
- `tests/simplification_audit.rs:6509-6514` `a_lifetime_is_not_mistaken_for_a_char_literal`
- `tests/simplification_audit.rs:6517-6522` `an_escaped_quote_char_literal_does_not_confuse_the_scanner`
- `tests/simplification_audit.rs:6536-6541` `a_trait_default_method_with_a_body_is_recorded`

#### `dup-0709` (near, 5 sites)

Proposed home: `simplification_audit::support (consolidate these 5 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:6565-6571` `a_method_inside_an_impl_block_carries_its_header`
- `tests/simplification_audit.rs:6574-6582` `a_trait_impl_header_keeps_the_trait_for_type_text`
- `tests/simplification_audit.rs:6585-6597` `a_method_inside_an_impl_nested_in_a_cfg_test_mod_is_flagged_test`
- `tests/simplification_audit.rs:6600-6609` `a_cfg_test_attribute_directly_on_an_impl_block_is_flagged_test`
- `tests/simplification_audit.rs:6731-6739` `a_cfg_test_pub_fn_is_still_flagged_test_pub_survives_between_attribute_and_keyword`

#### `dup-0710` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:6622-6628` `a_function_directly_in_a_cfg_test_mod_is_flagged_test`
- `tests/simplification_audit.rs:6631-6640` `a_nested_named_test_submodule_is_still_flagged_test_and_named`

#### `dup-0711` (exact, 3 sites)

Proposed home: `simplification_audit::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:6689-6692` `a_free_function_with_no_pub_keyword_is_private`
- `tests/simplification_audit.rs:6702-6705` `a_pub_crate_function_keeps_the_qualifier`
- `tests/simplification_audit.rs:6708-6711` `a_pub_super_function_keeps_the_qualifier`

#### `dup-0712` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:6751-6755` `a_cfg_test_out_of_line_mod_is_flagged_test`
- `tests/simplification_audit.rs:6772-6777` `an_out_of_line_mod_inherits_test_ness_from_an_enclosing_cfg_test_mod`

#### `dup-0713` (near, 3 sites)

Proposed home: `simplification_audit::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:6841-6847` `a_method_is_classified_under_its_impl_self_type`
- `tests/simplification_audit.rs:6850-6859` `a_method_on_a_generic_impl_block_strips_the_impls_own_leading_generics`
- `tests/simplification_audit.rs:6939-6944` `a_trait_impl_method_is_classified_under_the_implementing_type_not_the_trait`

#### `dup-0714` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:7000-7032` `section_1_names_every_module_and_every_unassigned_function`
- `tests/simplification_audit.rs:7035-7049` `section_1_reports_none_unassigned_explicitly_when_everything_is_assigned`

#### `dup-0715` (near, 3 sites)

Proposed home: `simplification_audit::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:7065-7071` `replace_section_1_only_touches_section_1_leaving_later_sections_intact`
- `tests/simplification_audit.rs:8371-8378` `replace_section_2_only_touches_section_2_leaving_neighbors_intact`
- `tests/simplification_audit.rs:9209-9226` `replace_section_6_only_touches_that_span_leaving_earlier_sections_intact`

#### `dup-0716` (near, 3 sites)

Proposed home: `simplification_audit::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:7115-7138` `responsibility_map_json_matches_the_tree_or_is_rewritten`
- `tests/simplification_audit.rs:8517-8544` `duplication_catalog_json_matches_the_tree_or_is_rewritten`
- `tests/simplification_audit.rs:10399-10426` `dead_code_json_matches_the_tree_or_is_rewritten`

#### `dup-0717` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:7177-7201` `a_pin_bump_leaves_the_guarded_responsibility_map_byte_identical`
- `tests/simplification_audit.rs:10491-10518` `a_pin_bump_leaves_the_guarded_dead_code_json_byte_identical`

#### `dup-0718` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:7227-7290` `two_branches_each_adding_an_unrelated_function_to_a_different_target_file_never_perturb_an_existing_responsibility_map_entry`
- `tests/simplification_audit.rs:10528-10606` `two_branches_each_adding_an_unrelated_function_to_a_different_file_never_perturb_an_existing_dead_code_entry`

#### `dup-0719` (exact, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:7507-7515` `string_and_raw_string_literals_are_one_lit_token_each`
- `tests/simplification_audit.rs:7527-7535` `number_literals_including_a_fraction_are_lit_tokens`

#### `dup-0720` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:7785-7811` `three_near_but_not_identical_functions_cluster_as_near_with_every_site_and_one_home`
- `tests/simplification_audit.rs:7814-7832` `cluster_ids_are_assigned_after_deterministic_sort_and_sites_are_sorted_within_a_cluster`

#### `dup-0721` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:7957-7970` `proc_stat_or_status_reader_sweep_finds_a_stat_reader_and_a_status_reader_but_not_an_unrelated_fn`
- `tests/simplification_audit.rs:8217-8234` `bespoke_lexer_sweep_finds_the_named_trio_but_not_an_unrelated_fn`

#### `dup-0722` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:7977-7990` `the_dash_reap_proc_stat_pair_the_spec_names_lands_in_one_real_cluster`
- `tests/simplification_audit.rs:8089-8104` `the_two_exploration_graph_fixture_builders_the_adversarial_sample_found_land_in_one_real_cluster`

#### `dup-0723` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:7993-8000` `constructs_own_type_literal_matches_self_and_the_named_type_but_not_an_unrelated_call`
- `tests/simplification_audit.rs:8003-8011` `constructs_own_type_literal_matches_shorthand_field_init_too`

#### `dup-0724` (exact, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:8037-8055` `the_spawn_result_constructor_triple_the_adversarial_sample_found_lands_in_one_real_cluster`
- `tests/simplification_audit.rs:8241-8258` `the_bespoke_lexer_and_canonical_extractor_the_lens_routed_land_in_one_real_cluster`

#### `dup-0725` (near, 3 sites)

Proposed home: `simplification_audit::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:8107-8135` `same_named_helper_sweep_excludes_required_trait_impl_methods_across_adapters`
- `tests/simplification_audit.rs:8138-8167` `same_named_helper_sweep_excludes_a_trait_default_method_and_its_override`
- `tests/simplification_audit.rs:8170-8194` `same_named_helper_sweep_still_catches_two_inherent_impls_sharing_a_method_name`

#### `dup-0726` (exact, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:8294-8317` `catalog_to_json_round_trips_through_deserialize`
- `tests/simplification_audit.rs:8320-8342` `catalog_lines_to_json_round_trips_and_carries_only_the_line_spans`

#### `dup-0727` (exact, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:8382-8387` `replace_section_2_panics_loudly_when_the_heading_is_entirely_absent`
- `tests/simplification_audit.rs:9239-9244` `replace_section_6_panics_loudly_when_the_heading_is_entirely_absent`

#### `dup-0728` (exact, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:8394-8398` `sample_indices_is_deterministic_for_a_fixed_seed`
- `tests/simplification_audit.rs:8424-8428` `different_seeds_produce_different_draws`

#### `dup-0729` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:8594-8621` `a_pin_bump_that_shifts_every_site_in_a_file_leaves_the_guarded_catalog_byte_identical`
- `tests/simplification_audit.rs:8631-8682` `two_branches_adding_an_unrelated_function_to_different_files_leave_the_guarded_catalog_unaffected`

#### `dup-0730` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:8866-8897` `report_section_2_matches_the_tree_or_is_rewritten`
- `tests/simplification_audit.rs:9059-9104` `report_sections_3_through_5_match_the_tree_or_are_rewritten`

#### `dup-0731` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:8905-8929` `replace_sections_3_to_5_only_touches_that_span_leaving_neighbors_intact`
- `tests/simplification_audit.rs:8932-8949` `replace_sections_3_to_5_falls_back_to_end_of_string_when_no_section_6_heading_exists`

#### `dup-0732` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:8994-9047` `assert_section_4_structurally_matches`
- `tests/simplification_audit.rs:9323-9379` `assert_section_6_structurally_matches`

#### `dup-0733` (near, 5 sites)

Proposed home: `simplification_audit::support (consolidate these 5 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:9452-9462` `resolves_a_same_name_dot_rs_target`
- `tests/simplification_audit.rs:9465-9478` `resolves_a_name_slash_mod_rs_target_when_the_flat_file_does_not_exist`
- `tests/simplification_audit.rs:9481-9497` `resolves_a_path_override_target`
- `tests/simplification_audit.rs:9500-9518` `the_real_eventstore_mod_rs_shape_resolves_contract_rs_as_test`
- `tests/simplification_audit.rs:9521-9542` `transitive_closure_pulls_in_a_second_hop_regardless_of_its_own_local_attribute`

#### `dup-0734` (near, 4 sites)

Proposed home: `simplification_audit::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:9627-9638` `resolvers_agree_on_a_same_name_dot_rs_target`
- `tests/simplification_audit.rs:9642-9653` `resolvers_agree_on_a_path_override_target`
- `tests/simplification_audit.rs:9657-9673` `resolvers_agree_on_a_transitive_second_hop`
- `tests/simplification_audit.rs:9677-9685` `resolvers_agree_on_a_non_test_out_of_line_mod`

#### `dup-0735` (near, 18 sites)

Proposed home: `simplification_audit::support (consolidate these 18 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:9710-9724` `a_fn_referenced_only_by_its_own_test_is_listed`
- `tests/simplification_audit.rs:9727-9741` `a_fn_referenced_from_a_production_caller_does_not_appear`
- `tests/simplification_audit.rs:9790-9802` `a_path_qualified_reference_with_no_call_parens_still_counts`
- `tests/simplification_audit.rs:9805-9815` `a_mention_inside_a_comment_does_not_count_as_a_reference`
- `tests/simplification_audit.rs:9827-9839` `recursion_through_the_fns_own_body_still_counts_as_a_reference`
- `tests/simplification_audit.rs:9842-9859` `a_whole_file_test_via_out_of_line_resolution_is_never_a_candidate`
- `tests/simplification_audit.rs:9879-9898` `a_named_use_import_at_mod_test_top_level_does_not_leak_as_a_production_reference`
- `tests/simplification_audit.rs:9901-9916` `a_serde_default_attribute_string_names_a_real_production_reference`
- `tests/simplification_audit.rs:10103-10116` `a_dot_call_through_an_inherent_method_on_any_receiver_counts_receiver_agnostically`
- `tests/simplification_audit.rs:10137-10152` `an_inherent_associated_function_is_matched_via_path_shape_not_dot_shape`
- `tests/simplification_audit.rs:10227-10241` `a_fn_passed_by_value_as_a_bare_call_argument_counts_as_a_reference`
- `tests/simplification_audit.rs:10258-10271` `a_fn_pointer_used_as_a_struct_literal_field_value_counts_as_a_reference`
- `tests/simplification_audit.rs:10274-10289` `a_ufcs_qualified_value_passed_to_a_combinator_counts_as_a_reference_for_a_method`
- `tests/simplification_audit.rs:10292-10302` `a_fn_named_by_a_let_initializer_counts_as_a_reference`
- `tests/simplification_audit.rs:10305-10316` `a_fn_named_as_an_array_element_counts_as_a_reference`
- `tests/simplification_audit.rs:10319-10330` `a_fn_named_in_a_match_arm_value_counts_as_a_reference`
- `tests/simplification_audit.rs:10333-10343` `a_fn_named_in_a_return_expression_counts_as_a_reference`
- `tests/simplification_audit.rs:10346-10359` `a_fn_named_as_a_generic_argument_to_another_type_counts_as_a_reference`

#### `dup-0736` (near, 4 sites)

Proposed home: `simplification_audit::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:9862-9870` `main_is_exempted_as_an_entry_point`
- `tests/simplification_audit.rs:9919-9935` `an_attribute_reference_to_an_ambiguous_shared_name_credits_every_sharer`
- `tests/simplification_audit.rs:10072-10087` `a_method_name_shared_by_two_impls_with_a_call_site_excludes_both`
- `tests/simplification_audit.rs:10119-10134` `a_trait_impl_method_is_exempted_even_with_zero_textual_call_sites`

#### `dup-0737` (near, 8 sites)

Proposed home: `simplification_audit::support (consolidate these 8 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:9938-9971` `a_free_fn_bare_name_collision_where_only_one_sharer_has_a_real_caller_flags_the_other`
- `tests/simplification_audit.rs:9974-9996` `a_bare_call_in_the_same_file_as_its_definition_attributes_locally_with_no_import_needed`
- `tests/simplification_audit.rs:9999-10025` `a_bare_unqualified_call_from_a_third_unrelated_file_credits_neither_sharer`
- `tests/simplification_audit.rs:10028-10054` `a_bare_call_resolved_through_a_use_import_attributes_to_the_imported_definition`
- `tests/simplification_audit.rs:10057-10069` `a_method_name_shared_by_two_impls_with_zero_calls_is_flagged_ambiguous`
- `tests/simplification_audit.rs:10155-10166` `an_inherent_associated_fn_name_shared_by_two_types_with_zero_calls_is_ambiguous`
- `tests/simplification_audit.rs:10169-10189` `a_qualified_call_site_attributes_only_to_the_sharer_it_names`
- `tests/simplification_audit.rs:10192-10224` `a_qualified_call_site_on_an_impls_own_generic_self_type_attributes_correctly`

#### `dup-0738` (near, 3 sites)

Proposed home: `spawn_scratch_reap_authorized_root_periphery::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/spawn_scratch_reap_authorized_root_periphery.rs:167-230` `rigger_result_reaps_a_live_process_in_the_spawns_registered_agent_scratch_dir`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:233-281` `rigger_result_reaps_a_live_process_in_the_spawns_registered_mutation_scratch_dir`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:415-487` `rigger_result_reaps_a_live_process_whose_registered_mutation_scratch_dir_was_already_removed_before_the_call`

#### `dup-0739` (near, 2 sites)

Proposed home: `spawn_timing_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/spawn_timing_periphery.rs:104-183` `spawn_timing_pairs_real_writer_events_through_a_real_store_by_role`
- `tests/spawn_timing_periphery.rs:280-335` `spawn_timing_excludes_a_real_same_batch_pair_as_suspect_not_a_silent_zero`

#### `dup-0740` (near, 42 sites)

Proposed home: `spec_lint::support (consolidate these 42 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/spec_lint.rs:54-102` `validate_spec_reports_every_c3_defect_with_its_criterion_and_field_guide_class`
- `tests/spec_lint.rs:120-163` `validate_spec_attributes_a_prose_level_defect_to_no_criterion`
- `tests/spec_lint.rs:172-209` `validate_spec_reports_two_simultaneous_defects_on_the_same_criterion`
- `tests/spec_lint.rs:218-249` `validate_spec_finds_an_owns_sentence_on_a_wrapped_continuation_line`
- `tests/spec_lint.rs:256-289` `validate_spec_does_not_misread_neither_or_as_either_or`
- `tests/spec_lint.rs:298-332` `validate_spec_ignores_a_smell_phrase_named_in_double_quotes`
- `tests/spec_lint.rs:341-375` `validate_spec_does_not_misread_a_later_or_prefixed_word_as_either_or`
- `tests/spec_lint.rs:384-415` `validate_spec_flags_an_explicit_ownership_denial_as_twin_risk`
- `tests/spec_lint.rs:424-458` `validate_spec_does_not_misread_worth_considering_across_a_hyphenated_compound`
- `tests/spec_lint.rs:467-507` `validate_spec_flags_ownerless_and_not_owned_denials_as_twin_risk`
- `tests/spec_lint.rs:519-558` `validate_spec_does_not_misread_owns_or_owner_inside_an_unrelated_word_as_ownership`
- `tests/spec_lint.rs:570-602` `validate_spec_finds_an_owns_sentence_on_an_unindented_continuation_line`
- `tests/spec_lint.rs:612-650` `validate_spec_does_not_reattach_prose_after_a_blank_line_to_the_prior_criterion`
- `tests/spec_lint.rs:656-683` `validate_reports_a_clean_spec_clean`
- `tests/spec_lint.rs:695-745` `validate_spec_lets_an_affirmative_owns_win_over_an_unrelated_denial_elsewhere`
- `tests/spec_lint.rs:751-782` `validate_spec_recognizes_owner_inside_a_hyphenated_compound`
- `tests/spec_lint.rs:796-829` `validate_spec_lets_a_standalone_owner_win_over_an_unrelated_denial_elsewhere`
- `tests/spec_lint.rs:843-875` `validate_spec_does_not_weld_own_and_er_into_owner_across_a_wrapped_continuation_line`
- `tests/spec_lint.rs:888-913` `validate_does_not_flag_specs_68_own_satisfied_either_or_disposition_clause`
- `tests/spec_lint.rs:921-946` `validate_still_flags_specs_57_genuine_either_or_hedge`
- `tests/spec_lint.rs:956-981` `validate_does_not_pair_a_non_disjunctive_either_with_a_faraway_or_on_specs_68`
- `tests/spec_lint.rs:1167-1196` `validate_still_flags_an_unsatisfied_either_or_as_an_open_hedge`
- `tests/spec_lint.rs:1212-1243` `validate_still_flags_a_genuine_hedge_after_an_earlier_non_disjunctive_either_on_specs_68_shape`
- `tests/spec_lint.rs:1251-1272` `validate_exempts_the_comma_separated_decided_disposition`
- `tests/spec_lint.rs:1279-1300` `validate_still_flags_a_spaced_negation_before_the_decided_idiom`
- `tests/spec_lint.rs:1307-1328` `validate_flags_a_hedge_split_across_hard_wrapped_lines`
- `tests/spec_lint.rs:1340-1363` `validate_does_not_fuse_a_hedge_across_a_heading_boundary`
- `tests/spec_lint.rs:1370-1393` `validate_does_not_fuse_a_hedge_across_a_table_row_boundary`
- `tests/spec_lint.rs:1407-1434` `validate_ignores_a_double_quoted_span_that_crosses_a_hard_wrapped_line`
- `tests/spec_lint.rs:1444-1470` `validate_ignores_a_backtick_span_that_crosses_a_hard_wrapped_line`
- `tests/spec_lint.rs:1485-1510` `validate_still_flags_a_smell_outside_a_balanced_quote_pair`
- `tests/spec_lint.rs:1518-1543` `validate_still_flags_a_smell_outside_a_balanced_backtick_pair`
- `tests/spec_lint.rs:1553-1580` `validate_fails_closed_after_a_stray_unmatched_quote_earlier_in_the_paragraph`
- `tests/spec_lint.rs:1587-1613` `validate_fails_closed_after_a_stray_unmatched_backtick_earlier_in_the_paragraph`
- `tests/spec_lint.rs:1629-1656` `validate_a_stray_unmatched_quote_does_not_unmask_a_later_real_quoted_disposition_phrase`
- `tests/spec_lint.rs:1676-1702` `validate_ignores_a_backtick_span_whose_closing_mark_is_immediately_after_a_digit`
- `tests/spec_lint.rs:1719-1746` `validate_ignores_a_quoted_span_whose_closing_quote_is_immediately_after_a_digit`
- `tests/spec_lint.rs:1756-1782` `validate_a_digit_adjacent_quote_stays_excluded_as_an_opener`
- `tests/spec_lint.rs:1798-1824` `validate_a_quote_at_the_very_start_of_a_paragraph_is_a_valid_opener`
- `tests/spec_lint.rs:1851-1880` `validate_an_embedded_digit_adjacent_mark_does_not_prematurely_close_a_real_quoted_span`
- `tests/spec_lint.rs:1914-1948` `validate_all_four_digit_adjacency_shapes_together_never_false_positive`
- `tests/spec_lint.rs:1976-2003` `validate_a_digit_glued_to_a_quotes_own_opening_mark_still_masks_the_real_span`

#### `dup-0741` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/step_attention_periphery.rs, tests/workflow_driver_resolved_model_periphery.rs, tests/worktree_liveness_fence_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/step_attention_periphery.rs:464-484` `temp_git_project_with_commit`
- `tests/workflow_driver_resolved_model_periphery.rs:57-78` `temp_git_project_with_commit`
- `tests/worktree_liveness_fence_periphery.rs:94-115` `temp_git_project_with_commit`

#### `dup-0742` (near, 2 sites)

Proposed home: `step_sheds_the_freshen::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/step_sheds_the_freshen.rs:128-163` `fingerprint_subtree`
- `tests/step_sheds_the_freshen.rs:129-159` `walk`

#### `dup-0743` (near, 2 sites)

Proposed home: `store_config::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/store_config.rs:63-75` `a_present_store_block_deserializes_backend_and_url`
- `tests/store_config.rs:92-110` `unrelated_workflow_keys_are_ignored_by_the_lightweight_probe`

#### `dup-0744` (exact, 2 sites)

Proposed home: `store_config::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/store_config.rs:78-89` `a_workflow_without_a_store_key_reads_as_the_default`
- `tests/store_config.rs:149-160` `an_empty_store_block_and_empty_values_are_no_opinion`

#### `dup-0745` (semantic, 4 sites)

Proposed home: `store_content_identity_periphery::port_double - one canonical constructor, the rest thin variants over it (or a builder), rather than each re-listing every field`

mandatory sweep: parallel constructor functions - 4 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/store_content_identity_periphery.rs:139-147` `new`
- `tests/store_content_identity_periphery.rs:152-160` `miscounting`
- `tests/store_content_identity_periphery.rs:164-166` `over_an_empty_stream`
- `tests/store_content_identity_periphery.rs:170-178` `over_a_stream`

#### `dup-0746` (exact, 2 sites)

Proposed home: `store_content_identity_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/store_content_identity_periphery.rs:1508-1510` `detached_subject`
- `tests/store_content_identity_periphery.rs:1520-1522` `mid_character`

#### `dup-0747` (near, 2 sites)

Proposed home: `store_content_identity_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/store_content_identity_periphery.rs:1512-1514` `past_the_end`
- `tests/store_content_identity_periphery.rs:1516-1518` `inverted`

#### `dup-0748` (semantic, 4 sites)

Proposed home: `one shared `local_event_log` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 4 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/store_flag_precedence.rs:94-96` `local_event_log`
- `tests/store_precedence.rs:59-61` `local_event_log`
- `tests/store_resolution_cli.rs:66-68` `local_event_log`
- `tests/store_secrets.rs:62-64` `local_event_log`

#### `dup-0749` (near, 4 sites)

Proposed home: `a new shared module (sites span 3 files: tests/store_flag_precedence.rs, tests/store_precedence.rs, tests/store_secrets.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/store_flag_precedence.rs:125-145` `assert_selected_server`
- `tests/store_precedence.rs:119-139` `assert_selected_server`
- `tests/store_precedence.rs:143-159` `assert_selected_sqlite`
- `tests/store_secrets.rs:106-144` `assert_server_reached_and_credentials_redacted`

#### `dup-0750` (semantic, 2 sites)

Proposed home: `one shared `assert_selected_server` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/store_flag_precedence.rs:125-145` `assert_selected_server`
- `tests/store_precedence.rs:119-139` `assert_selected_server`

#### `dup-0751` (exact, 2 sites)

Proposed home: `store_flag_precedence::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/store_flag_precedence.rs:148-165` `run_bare_conn_flag_selects_the_server_never_dropped_to_sqlite`
- `tests/store_flag_precedence.rs:168-183` `run_conn_flag_beats_a_committed_sqlite_store_config`

#### `dup-0752` (semantic, 3 sites)

Proposed home: `one shared `empty_project` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/store_precedence.rs:47-55` `empty_project`
- `tests/store_resolution_cli.rs:54-62` `empty_project`
- `tests/store_secrets.rs:50-58` `empty_project`

#### `dup-0753` (semantic, 2 sites)

Proposed home: `one shared `write_store_conn` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/store_precedence.rs:64-66` `write_store_conn`
- `tests/store_secrets.rs:70-79` `write_store_conn`

#### `dup-0754` (near, 3 sites)

Proposed home: `store_precedence::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/store_precedence.rs:217-252` `a_present_but_unreadable_store_conn_surfaces_loudly_not_a_silent_sqlite_fallback`
- `tests/store_precedence.rs:269-312` `an_unknown_committed_backend_surfaces_loudly_not_a_silent_sqlite_fallback`
- `tests/store_precedence.rs:315-355` `a_committed_kurrentdb_backend_with_no_credential_names_all_three_sources`

#### `dup-0755` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/store_resolution_cli.rs, tests/store_secrets.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/store_resolution_cli.rs:78-90` `run_bare_result`
- `tests/store_secrets.rs:88-100` `run_bare_result`

#### `dup-0756` (semantic, 2 sites)

Proposed home: `one shared `run_bare_result` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/store_resolution_cli.rs:78-90` `run_bare_result`
- `tests/store_secrets.rs:88-100` `run_bare_result`

#### `dup-0757` (near, 3 sites)

Proposed home: `store_resolution_cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/store_resolution_cli.rs:93-127` `a_server_selected_courier_reaches_the_server_and_never_fabricates_local_sqlite`
- `tests/store_resolution_cli.rs:130-157` `a_courier_with_no_server_configured_resolves_the_local_sqlite_log`
- `tests/store_resolution_cli.rs:160-184` `an_empty_kurrentdb_conn_is_treated_as_unset_not_a_server_with_no_address`

#### `dup-0758` (exact, 2 sites)

Proposed home: `store_resolution_cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/store_resolution_cli.rs:278-285` `prime_resolves_the_configured_server_never_the_local_absent_sentinel`
- `tests/store_resolution_cli.rs:288-297` `stats_resolves_the_configured_server_never_the_local_absent_sentinel`

#### `dup-0759` (exact, 2 sites)

Proposed home: `store_secrets_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/store_secrets_periphery.rs:52-59` `redact_conn_is_a_public_symbol_that_scrubs_userinfo_but_keeps_scheme_host_and_query`
- `tests/store_secrets_periphery.rs:248-256` `redact_conn_scrubs_the_credential_but_keeps_a_benign_at_sign_later_in_the_same_url`

#### `dup-0760` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/subject_lens_defined_cells.rs, tests/subject_lens_defined_cells_contract.rs, tests/subject_lens_reprojection_contract.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/subject_lens_defined_cells.rs:60-70` `node`
- `tests/subject_lens_defined_cells_contract.rs:48-58` `node`
- `tests/subject_lens_reprojection_contract.rs:63-73` `node`

#### `dup-0761` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/subject_lens_defined_cells.rs, tests/subject_lens_defined_cells_contract.rs, tests/subject_lens_reprojection_contract.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/subject_lens_defined_cells.rs:73-83` `edge`
- `tests/subject_lens_defined_cells_contract.rs:61-71` `edge`
- `tests/subject_lens_reprojection_contract.rs:88-98` `edge`

#### `dup-0762` (exact, 7 sites)

Proposed home: `a new shared module (sites span 4 files: tests/subject_lens_defined_cells.rs, tests/subject_lens_defined_cells_contract.rs, tests/subject_lens_reprojection_contract.rs, tests/subject_lens_reprojection_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/subject_lens_defined_cells.rs:85-87` `code_lens`
- `tests/subject_lens_defined_cells.rs:89-91` `concepts_lens`
- `tests/subject_lens_defined_cells_contract.rs:73-75` `code_lens`
- `tests/subject_lens_defined_cells_contract.rs:77-79` `concepts_lens`
- `tests/subject_lens_reprojection_contract.rs:101-103` `concepts_lens`
- `tests/subject_lens_reprojection_contract.rs:106-108` `code_lens`
- `tests/subject_lens_reprojection_periphery.rs:315-317` `code_lens`

#### `dup-0763` (semantic, 3 sites)

Proposed home: `one shared `concepts_lens` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/subject_lens_defined_cells.rs:89-91` `concepts_lens`
- `tests/subject_lens_defined_cells_contract.rs:77-79` `concepts_lens`
- `tests/subject_lens_reprojection_contract.rs:101-103` `concepts_lens`

#### `dup-0764` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/subject_lens_defined_cells.rs, tests/subject_lens_defined_cells_contract.rs, tests/subject_lens_reprojection_contract.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/subject_lens_defined_cells.rs:104-121` `served_json`
- `tests/subject_lens_defined_cells_contract.rs:87-104` `served_json`
- `tests/subject_lens_reprojection_contract.rs:122-142` `served_json`

#### `dup-0765` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: tests/subject_lens_defined_cells.rs, tests/subject_lens_reprojection_contract.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/subject_lens_defined_cells.rs:133-149` `communities_no_concepts_graph`
- `tests/subject_lens_defined_cells.rs:255-272` `mixed_membership_graph`
- `tests/subject_lens_reprojection_contract.rs:380-401` `file_over_code_graph`

#### `dup-0766` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/subject_lens_defined_cells.rs, tests/subject_lens_defined_cells_contract.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/subject_lens_defined_cells.rs:510-530` `shared_member_graph`
- `tests/subject_lens_defined_cells_contract.rs:238-256` `shared_member_graph`

#### `dup-0767` (semantic, 2 sites)

Proposed home: `one shared `shared_member_graph` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/subject_lens_defined_cells.rs:510-530` `shared_member_graph`
- `tests/subject_lens_defined_cells_contract.rs:238-256` `shared_member_graph`

#### `dup-0768` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/subject_lens_defined_cells_contract.rs, tests/subject_lens_reprojection_contract.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/subject_lens_defined_cells_contract.rs:318-334` `full_in_budget_graph`
- `tests/subject_lens_reprojection_contract.rs:154-177` `community_over_concepts_graph`

#### `dup-0769` (near, 3 sites)

Proposed home: `subject_lens_reprojection_contract::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/subject_lens_reprojection_contract.rs:250-284` `reprojection_admits_a_realizing_member_of_any_kind_under_the_concepts_lens`
- `tests/subject_lens_reprojection_contract.rs:493-526` `reprojection_excludes_a_non_code_entity_member_entirely_under_the_code_lens`
- `tests/subject_lens_reprojection_contract.rs:543-573` `reprojection_excludes_a_decision_member_even_when_it_carries_a_live_community_membership`

#### `dup-0770` (near, 2 sites)

Proposed home: `subject_lens_reprojection_contract::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/subject_lens_reprojection_contract.rs:297-328` `reprojection_carries_empty_state_when_no_member_realizes_any_concept_under_the_concepts_lens`
- `tests/subject_lens_reprojection_contract.rs:590-621` `reprojection_carries_empty_state_when_the_sole_realizer_is_purity_excluded`

#### `dup-0771` (near, 7 sites)

Proposed home: `unified_traversal_grounding::support (consolidate these 7 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/unified_traversal_grounding.rs:306-341` `a_spawn_prompt_carries_the_unified_traversal_code_neighborhood_not_the_old_structural_stitch`
- `tests/unified_traversal_grounding.rs:359-463` `the_implement_prompt_is_trimmed_to_the_intent_layer_with_a_rigger_peers_pointer`
- `tests/unified_traversal_grounding.rs:477-517` `the_producer_prompt_keeps_the_full_grounding_context_not_the_implement_trim`
- `tests/unified_traversal_grounding.rs:688-792` `the_sdet_author_build_seam_spawn_receives_the_trimmed_implement_slice`
- `tests/unified_traversal_grounding.rs:1231-1361` `a_spawn_prompt_carries_the_design_intent_that_governs_the_touched_files_by_traversal`
- `tests/unified_traversal_grounding.rs:1450-1507` `a_governing_decision_never_leaks_into_the_spawn_prompt_design_intent_section`
- `tests/unified_traversal_grounding.rs:1584-1617` `a_spawn_prompt_with_no_governing_design_intent_renders_no_design_intent_header`

#### `dup-0772` (near, 2 sites)

Proposed home: `unified_traversal_grounding::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/unified_traversal_grounding.rs:1022-1065` `the_code_neighborhood_section_is_budget_capped_with_a_visible_elision_note`
- `tests/unified_traversal_grounding.rs:1084-1125` `the_spawn_prompt_code_neighborhood_elision_note_names_the_honest_graph_around_recovery`

#### `dup-0773` (near, 2 sites)

Proposed home: `unified_traversal_grounding::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/unified_traversal_grounding.rs:1383-1430` `the_design_intent_section_is_budget_capped_and_its_elision_note_names_the_honest_graph_around_recovery`
- `tests/unified_traversal_grounding.rs:1521-1565` `the_spawn_prompt_design_intent_section_renders_the_newest_binding_and_elides_the_oldest`

#### `dup-0774` (exact, 2 sites)

Proposed home: `validate_advisories::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/validate_advisories.rs:299-312` `validate_is_silent_on_log_bloat_when_every_key_is_recorded_once`
- `tests/validate_advisories.rs:315-343` `validate_is_silent_on_log_bloat_when_the_same_key_recurs_only_across_different_covered_types`

#### `dup-0775` (near, 3 sites)

Proposed home: `watchdog_cli_periphery::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/watchdog_cli_periphery.rs:132-182` `watch_once_reports_anomalies_through_the_real_compiled_binary_naming_signal_subject_and_response`
- `tests/watchdog_cli_periphery.rs:299-326` `watch_once_reports_a_store_integrity_anomaly_through_the_real_compiled_binary`
- `tests/watchdog_cli_periphery.rs:573-654` `watch_once_reports_the_criterions_own_multi_anomaly_scenario_through_the_real_compiled_binary`

#### `dup-0776` (near, 3 sites)

Proposed home: `watchdog_cli_periphery::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/watchdog_cli_periphery.rs:382-449` `watch_without_once_streams_and_re_polls_a_live_mutating_store_until_killed`
- `tests/watchdog_cli_periphery.rs:462-545` `watch_streaming_survives_a_transient_store_read_failure_and_recovers`
- `tests/watchdog_cli_periphery.rs:669-771` `watch_streaming_re_alerts_a_reject_recurrence_churn_count_on_each_increment`

#### `dup-0777` (near, 2 sites)

Proposed home: `worker_persona_label_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/worker_persona_label_periphery.rs:241-253` `a_gap_18_respawn_id_still_renders_the_full_persona_led_label`
- `tests/worker_persona_label_periphery.rs:337-348` `internal_whitespace_is_normalized_before_the_sentence_is_cut`

#### `dup-0778` (near, 2 sites)

Proposed home: `workflow_driver_resolved_model_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/workflow_driver_resolved_model_periphery.rs:163-319` `workflow_driven_rigger_result_meta_resolved_model_reaches_the_persisted_green_event`
- `tests/workflow_driver_resolved_model_periphery.rs:334-473` `workflow_driven_rigger_result_with_no_meta_omits_the_resolved_model_key_and_ignores_a_prose_claim`

### Adversarial sample

Recall check (spec 85 THOROUGHNESS): 30 functions drawn by seeded random index (seed `85072026`, `sample_indices` over all 6749 functions scanned in `src/` and `tests/`, excluding `tests/prioritized_plan_citation_periphery.rs` - criterion 4's own citation-guard periphery test, whose function count grows as its citation-drift-guard mechanism hardens round over round; excluding it keeps that unrelated growth from ever reshuffling this already-verified draw), each read by hand - together with its host file's surrounding context, since a duplicate can live anywhere in the file or a sibling file - to judge whether a duplicate exists that the mechanical pass and the five sweeps above did not already catch.

- `src/canary.rs:1949-1957` `apply_model_pins_with_no_pins_changes_nothing` - no duplicate found by reading
- `src/conductor.rs:32904-32931` `partition_separates_overlapping_blast_radii` - no duplicate found by reading
- `src/contextgraph/sqlite.rs:2447-2484` `resolve_proof_target` - no duplicate found by reading
- `src/contextgraph/sqlite.rs:2511-2541` `record_proof` - no duplicate found by reading
- `src/dash.rs:6482-6492` `events_endpoint_is_since_exclusive` - no duplicate found by reading
- `src/distiller.rs:60-62` `normalize` - caught: `dup-0039`
- `src/eventstore/mod.rs:354-356` `meta_key` - no duplicate found by reading
- `src/main.rs:5615-5767` `cmd_replay` - no duplicate found by reading
- `src/main.rs:8470-8483` `build_cache_reclaim_report` - no duplicate found by reading
- `src/main.rs:13037-13047` `git_repo_at` - caught: `dup-0254`
- `src/mcpserver.rs:1095-1122` `emit_event_core_matches_the_mcp_tool` - no duplicate found by reading
- `src/spawn.rs:663-665` `to_event` - caught: `dup-0325`
- `src/spawn.rs:748-759` `result_of` - no duplicate found by reading
- `src/spec.rs:1733-1742` `disposition_check_does_not_exempt_unsatisfied_either_or` - caught: `dup-0348`
- `tests/architecture_integrity.rs:110-116` `is_intra_repo_link` - no duplicate found by reading
- `tests/cli.rs:3650-3687` `workflow_from_a_linked_worktree_refuses_naming_both_trees` - caught: `dup-0447`
- `tests/cli.rs:4444-4468` `write_two_stage_workflow` - caught: `dup-0451`
- `tests/code_entity_test_exclusion_periphery.rs:407-445` `path_override_some_serializes_the_key_and_round_trips` - caught: `dup-0514`
- `tests/code_lens_view_periphery.rs:149-153` `code_default` - caught: `dup-0531`
- `tests/dead_code_json_contract_periphery.rs:699-715` `getter_methods_kept_alive_only_by_a_same_named_production_field_or_local_are_also_absent` - caught: `dup-0590`
- `tests/metadata_card_periphery.rs:52-62` `edge` - caught: `dup-0023`
- `tests/parallel_ordered_emit.rs:196-208` `norm` - no duplicate found by reading
- `tests/rationale_overlay_seam.rs:121-130` `fetch_served` - caught: `dup-0575`
- `tests/reset_derived_compaction_periphery.rs:1366-1368` `graph_db` - caught: `dup-0313`
- `tests/reset_derived_compaction_periphery.rs:4060-4109` `a_prune_with_nothing_to_reclaim_leaves_the_file_unrewritten` - no duplicate found by reading
- `tests/simplification_audit.rs:7492-7496` `line_comments_and_whitespace_produce_no_token` - no duplicate found by reading
- `tests/simplification_audit.rs:7768-7782` `two_unrelated_functions_form_no_cluster` - no duplicate found by reading
- `tests/simplification_audit.rs:9247-9312` `render_section_6_cites_every_tier_and_the_explicit_none_needed_category` - no duplicate found by reading
- `tests/store_precedence.rs:59-61` `local_event_log` - caught: `dup-0313`
- `tests/turbovec_retired.rs:99-115` `turbovec_cargo_feature_is_retired` - no duplicate found by reading

Two real recall gaps surfaced this way and were closed by widening the mechanical sweep with a new generalizable detector each - not a one-off citation - so the fix catches every present and future instance of its class, each pinned by a real-tree regression test: `find_proc_stat_or_status_readers` (decision `u85c2-proc-stat-worked-example`) groups every function reading a `/proc/<pid>/stat` or `/proc/<pid>/status` literal, closing the spec's own named worked example - `src/dash.rs:499-507` `process_state` next to `src/reap.rs:190-197` `pid_starttime`, the same job on the same file with a different field/shape, upheld at spec 62's capstone; `find_parallel_constructor_clusters` (decision `u85c2-parallel-constructor-sweep`) groups 2+ non-test functions per `(file, Self type)` that build a `Self { .. }` / `TypeName { .. }` literal, closing `src/spawn.rs`'s `SpawnResult::liveness_fault` reading MISSING from its own `ok`/`failed` cluster even though all three are parallel constructors for one struct. A third worked example, `exploration_graph` (independently defined test-fixture builders in `tests/dash_exploration_route_client_contract.rs` and `tests/dash_kg_graph_route.rs`), was already caught correctly by the plain Jaccard pass with no sweep needed - confirming the mechanical pass itself has real recall, not only the two widened sweeps. Two further real defects, found on review rather than in this draw, were closed the same way: a RECALL gap the architecture lens routed to this criterion by name across two prior review rounds - this file's own bespoke source-text lexer (`scan_file`/`tokenize`) duplicating the codebase's ONE canonical tree-sitter extractor, `src/grounder/symbols/extract.rs::extract` (its own module doc's claim, architecture 5.5.3) - closed by `find_bespoke_lexer_vs_canonical_extractor` (decision `u85c2-bespoke-lexer-sweep`), a fourth generalizable sweep; and a PRECISION defect the adversary found by reading every `same-named helper` cluster against `ScannedFn::enclosing_impl` - `find_same_named_helper_functions` was misclassifying REQUIRED trait-impl methods as coincidental duplication (`subscribe_all`/`subscribe_stream` across the `EventStore` trait's three backend adapters plus a test double, `blast_radius` across the `Grounder` trait's own default method, its override, and a test double) - closed by excluding members whose extracted Self type differs across the group when at least one comes from an actual `" for "` trait impl (decision `u85c2-same-named-helper-trait-impl-precision-fix`), mirroring `find_parallel_constructor_clusters`'s own `(file, Self type)` keying one function away. Round 7 (decision `u85c4-r7-exclude-periphery-file-from-adversarial-population`) excluded this criterion's own citation-guard periphery file from the draw's population (see this subsection's opening paragraph) and redrew the sample; every one of the 19 functions above marked "no duplicate found by reading" was re-read by hand against its host file's surrounding context, exactly as this THOROUGHNESS check requires whenever the draw changes. 18 of the 19 are genuinely not duplicates; `apply` at `src/conductor.rs:29832-29834` is one shape worth naming so it is not mistaken for a miss - a `Projection` test double's own required trait-impl body, the same port-default/adapter-override/test-double shape `find_same_named_helper_functions`'s trait-impl-precision fix (decision `u85c2-same-named-helper-trait-impl-precision-fix`) already excludes from clustering by design, confirmed to still hold for this draw's own instance of it. The 19th is a genuine small duplicate this catalog's `fn`-only scanner (module doc, THE SCANNER) structurally cannot represent as a cluster: `gate_verdict_event` (`src/conductor.rs:29191-29200`) and the `verdict` closure inside `integrating_a_unit_stales_the_intersecting_downstream_units_cached_verdict_not_the_rest` (`src/conductor.rs:30596-30605`) do the identical job - find the recorded `GateVerdict` for a `"<unit>/gate:g#<attempt>"` replay key, panicking with the same message when none exists - differing only in whether the unit segment is the literal `"s"` or a parameter. A `let`-bound closure is not a `fn` item, so no change to this scanner short of teaching it to see closures could catalog this pair as a cluster; named here, prominently, rather than silently, so a later refactor - or a scanner that learns to see closures - does not miss it.


## 3. Boundary Violations

Instrument: for each of the five named ports (`eventstore::EventStore`, `contextgraph::Projection`, `conductor::AgentDriver`, `gate::Runner`, `grounder::Grounder`), grepped every production (pre-`#[cfg(test)]`) call site of that port's known concrete adapter modules from a NON-adapter, NON-composition-root file, and separately grepped every domain-ish file's top-level `use` statements for a direct infrastructure-crate import. `src/main.rs` is exempt from the "reaches a concrete adapter" check: it is the composition root, and wiring concretions together is its designed job.

FOUND, two violations:

Violation 1 (`AgentDriver`): `src/conductor.rs:7091-7096` (`reclaim_terminal_unit_mutation_scratch`, real production code - well above the `#[cfg(test)] mod tests` boundary this audit's own section 1 identified at `src/conductor.rs:10260`) calls `crate::driver::replay::cache_home_from` and `crate::driver::replay::reclaim_unit_mutation_scratch` directly by concrete module path. Read via `rigger graph --show AgentDriver`: the port `conductor.rs` actually depends on for driving agents is `trait AgentDriver { fn spawn(&self, agent: &AgentDef, prompt: &str, opts: &SpawnOpts, emit: &dyn Fn(&str, Value) -> Result<(), Error>) -> Result<AgentResult, Error>; }` (`src/conductor.rs:1083-1091`) - one method, `spawn`. Neither called function is about driving an agent or replaying a recorded run (the concern `driver::replay` otherwise owns); both are pure, driver-instance-free scratch-lifecycle utilities that happen to live inside that one concrete adapter's module. The port that should have been used: none exists for this concern yet, which is itself the defect - `conductor.rs` (a use-case/orchestration file) should not need to know which concrete `AgentDriver` implementation happens to define its own mutation-scratch cache-home resolution. Fix direction for a follow-up spec: relocate `cache_home_from` and `reclaim_unit_mutation_scratch` out of `driver::replay` into a neutral, adapter-independent module (a `scratch` or `mutation` support module conductor.rs and every driver adapter can depend on alike), so no use-case file reaches into one specific adapter's internals for a concern that adapter does not conceptually own.

Violation 2 (`Grounder`): `src/ingest.rs:187-211` (`walk_batches`, called from production `conductor::RunCtx::ingest_project_batches` at `src/conductor.rs:7903`, itself called from `src/conductor.rs:7894` well above the `10260` `#[cfg(test)]` boundary) calls `crate::grounder::symbols::events::project_batches_paced` directly by concrete module path at line 197 to reuse the `symbols` grounder's already-persisted index for a one-time whole-project ingest walk, then at line 203 - same function, same missing-port defect, not a separate third violation - calls `crate::grounder::design::events::project_batches` directly by concrete module path for the design-doc half of the same walk. These two calls are the two named sites of section 2's own catalogued twin duplicate pair (`dup-0201`: `src/grounder/symbols/events.rs:36-38` and `src/grounder/design/events.rs:90-114`, both named `project_batches`), so this boundary violation and that duplication finding are two symptoms of one root cause - `ingest.rs` naming each concrete grounder submodule because no port exposes either. The `Grounder` port's own methods (`ground`, `reindex`, `blast_radius`, `index_stamp` - its provenance stamp - all at `src/grounder/mod.rs:133-175`) serve real-time per-query grounding of an agent's prompt; none exposes "hand me every indexed file's projected events for a whole-project batch ingest," so `ingest.rs` - itself a domain ingest authority (its own module doc names it "the ONE walk-and-content-key authority"), not an adapter and not the composition root - has no port to depend on for either call and reaches the concrete `symbols` module (197) and the concrete `design` module (203) directly. Same missing-port defect class as violation 1. Fix direction for a follow-up spec: add an ingest-shaped port method (e.g. a `Grounder::project_batches` or a standalone `SymbolProjector` trait) covering both concrete modules, so `ingest.rs` depends on one abstraction instead of either concrete grounder module for its whole-project walk.

Also reaching `grounder::symbols::store::content_hash` from the same two call sites' neighborhood (`src/ingest.rs:230`, `src/canary.rs:213`): DISPOSITIONED as legitimate shared-primitive reuse, not a third violation. `content_hash` (`src/grounder/symbols/store.rs:49`) is documented at its own definition as "the content-identity primitive" the `symbols` grounder's own reindex-freshening gate keys on, and `canary.rs`'s own doc comment (`canary.rs:191`) separately calls it "the crate's ONE stable content-hash primitive", reused there by deliberate author intent rather than adding yet another open-coded FNV-1a copy - a generic hashing utility that happens to live in the `symbols` module, not a grounding operation reached through the port. The broader duplication this primitive is meant to fix (several open-coded FNV-1a copies elsewhere in the crate, per `src/community.rs`'s own comment at line 67) is a separately tracked cross-cutting refactor (`arch-u2i-fnv1a-fourth-parallel-copy`), not this section's concern.

CHECKED AND CLEAN (three of five ports fully clean; the other two, `AgentDriver` and `Grounder`, are this section's two violations above - each search recorded so a clean result is not merely assumed):
- `eventstore::EventStore` concretion reach (`rusqlite::Connection::open` outside `src/eventstore/sqlite.rs` / `src/eventstore/kurrentdb.rs` / `src/contextgraph/sqlite.rs`): two hits in all of `src/`, one a doc-comment mention (`src/main.rs:4332`) and one a deliberate, explicitly-commented test-only raw-connection bypass (`src/main.rs:23860`, inside `#[cfg(test)] mod tests` opened at `src/main.rs:12650`) that reproduces a pre-append-guard corruption shape `Store::append` itself refuses to construct - a documented test technique, not a boundary violation.
- `contextgraph::Projection` concretion reach (`contextgraph::sqlite::*`): checked whole-tree, not only `src/conductor.rs` - every one of `conductor.rs`'s 28 hits sits inside `#[cfg(test)] mod tests` (production `conductor.rs` only ever depends on `dyn Projection`), and the same is true wherever else `contextgraph::sqlite::Projector` is imported (`src/concepts.rs`, `src/dash.rs`, `src/grounder/symbols/events.rs`, `src/grounder/design/events.rs` - every import sits after that file's own `#[cfg(test)]` boundary); `src/community.rs`'s one mention is a doc comment.
- `gate::Runner` concretion reach (`gate::ExecRunner` / `RecordingRunner`): checked whole-tree, not only `src/conductor.rs`. In `conductor.rs`, production depends only on `dyn gate::Runner` (`conductor.rs:1288`); every mention of a concrete runner before that is a doc comment (`conductor.rs:6498,6545,6615,6621,6625`), and the only actual import and use of `ExecRunner` (`conductor.rs:10266` onward) plus the test-only `RecordingRunner` impl (`conductor.rs:28923,29014`) sit inside `#[cfg(test)] mod tests`, well past the `10260` boundary. Every other source-level `ExecRunner` mention in `src/` is either a doc comment (`src/worktree.rs`, `src/config.rs`, `src/budget.rs`, `src/driver/cli.rs`, `src/lib.rs`) or, in `src/driver/replay.rs`, an import and 13 parameter types that all sit inside that file's own `#[cfg(test)] mod tests` too.
- Use cases importing infrastructure: grepped the top-level `use` statements of every domain-ish file this audit's own code neighborhood names (`src/conductor.rs`, `src/blocker.rs`, `src/spec.rs`, `src/watch.rs`, `src/community.rs`) for `rusqlite`, `reqwest`, `tonic`, `tokio`, `kurrentdb` - zero hits anywhere. Empty category.

A second mutation authority for one domain: the one previously-known instance in this codebase (`src/dash.rs` reimplementing `src/reap.rs`'s `/proc` pid scan, spec 85's own Goal example, upheld at spec 62's capstone) is a duplicate READ-only reimplementation, not a bypassed MUTATION path - it is section 2's finding (`u85c2-proc-stat-worked-example`, `find_proc_stat_or_status_readers`), not re-counted here to avoid double-charging one defect to two sections. Checked git as the one other plausible second-authority candidate: every `Command::new("git")` call site in `src/conductor.rs` (23 sites) is at line >= 17297, inside `#[cfg(test)] mod tests` - production `conductor.rs` never shells to git directly. `src/worktree.rs` is the sole git-worktree-mutation authority OUTSIDE the composition root. Inside it, `src/main.rs` (exempt from the port-concretion-reach check above, not from this one) holds two more git-worktree-mutation sites: `reap_then_remove_worktree` (`main.rs:2791-2805`), the sanctioned worktree half of the spec-34/spec-79 orphan-sweep and extensively reviewed across those specs - a deliberate design choice, not a gap; and `materialize_config_at_rev` (`main.rs:5510-5556`), a real, already-known, non-blocking gap (`arch-u13-config-checkout-bypasses-worktree-authority` / `arch-u2r-config-checkout-shells-git` / `arch-u2r2-replayrunner-and-config-checkout-persist-not-introduced`: the `Worktree` API is branch-creating and exposes no detach-at-rev checkout, so this is a gap in that authority rather than a competing abstraction). No second mutation authority found beyond the already-cited, already-catalogued `/proc` case and this already-dispositioned `materialize_config_at_rev` gap.

## 4. Dead and Vestigial Code

### 4.0 Stage 1 (spec 87 criterion 1): the compiler-proven pass

Spec 87 redoes this whole section in two stages: STAGE 1, here, is compiler-driven and lands first; STAGE 2 (criterion 2) and STAGE 3 (criterion 3) are a separate reference sweep over the tree this stage leaves behind, and rewrite everything below this subsection in full. This subsection is this stage's own record, additive to and preserved by that later rewrite.

INSTRUMENT ONE (`cargo minify`, tweedegolf's tool): run over the whole crate on the default (`symbols`) feature lane, checking every FUNCTION, CONST, STATIC, STRUCT, ENUM, UNION, TYPE_ALIAS, ASSOCIATED_FUNCTION and MACRO_DEFINITION for zero references. Result: "no unused code that can be minified" - zero items removed. `cargo minify` has no `--no-default-features` flag (verified: `cargo minify --no-default-features` -> `error: unrecognized option`), so the light lane's own picture comes from instrument two below instead.

INSTRUMENT TWO (the promoted-lint build): `cargo rustc --lib` and `cargo rustc --bin rigger`, each on both the default and `--no-default-features` lanes, with `-D dead_code -D unused_imports -D unused_variables -D unreachable_pub` passed as trailing (target-only) flags - four builds total, all clean. `cargo rustc`'s trailing flags were chosen deliberately over a whole-crate `RUSTFLAGS` env var (tried first): `RUSTFLAGS` also strict-lints `build.rs`'s own compilation, which then fails on two items in `build/gitsemver.rs` that are correctly `pub` for their other three `#[path]` inclusion sites (`src/main.rs`, `tests/gitsemver_derivation.rs`, `tests/gitsemver_worktree_periphery.rs`) but register as `unreachable_pub` from `build.rs`'s own isolated crate view - a false positive from the blunt instrument, not a real defect in `build.rs`. `cargo rustc`'s trailing flags apply only to the one named target's own rustc invocation, never to a dependency and never to `build.rs`, so it proves exactly the claim this criterion's Done-when makes (a build of the library and the binary) without that false positive.

FOUND, in `src/`: zero items either instrument could prove unreachable - `delete_compiler` in the companion record (`docs/audit/stage1-compiler-pass.json`) is the empty list. This is the exact known blind spot this spec's own Design section predicts, not an unexplored gap: `rustc`'s `dead_code` lint never fires on a `pub` item in a crate that is both a library and a binary (this report's own prior finding, instrument three below, already established the codebase's src/ is clean under a plain rebuild), and `unreachable_pub` only catches a `pub` item that is provably reachable from NOWHERE outside its own crate - not one that is correctly exported but simply has zero real callers. That second, larger class needs the reference-counting instrument stage 2 builds, not a compiler diagnostic; this stage's near-empty yield is exactly why stage 2 exists.

FOUND, outside `src/` (`build/gitsemver.rs`, spec 74's compile-time version-derivation seam, shared via `#[path]` into four separate compilations - `build.rs`, `src/main.rs`, `tests/gitsemver_derivation.rs`, and `tests/gitsemver_worktree_periphery.rs`): two items, `UNVERSIONED_SUFFIX` (line 45) and `derive_version` (line 129), were `pub` when nothing outside their own defining crate ever reaches them at any of those four inclusion sites - `pub(crate)` satisfies every site independently, since each `#[path]` inclusion recompiles the same source text fresh as part of whichever crate includes it. Applied as the compiler's own suggested fix (`rustc`: "consider restricting its visibility: `pub(crate)`"); both a fast regression test (`tests/compiler_pass_stage1_audit.rs`) and this record hold the exact file:line so a future widening back to `pub` is caught. Not counted in `delete_compiler` above - a visibility narrowing is not a deletion, and `build/` is not `src/` - but disclosed here in full rather than silently folded into either count, mirroring this section's own "disclosed as an instrument limitation rather than silently worked around" discipline below.

VERIFIED STILL GREEN on the tree as this stage leaves it: `tests/no_os_kill_audit.rs` and `tests/reap_before_removal_audit.rs`, both process-lifecycle audits this criterion's Done-when names by name - unaffected, since this stage's only change touches a compile-time version string, never process lifecycle.

### 4.1 Stage 2 (spec 87 criterion 2): the whole-tree production-reference sweep

Stage 1's own near-empty yield (`delete_compiler` is the empty list) is the exact known blind spot the Design section predicts: `rustc`'s `dead_code` lint never fires on a `pub` item in a crate that is both a library and a binary. STAGE 2, criterion 2's own unit (`u87c2`, four review rounds, approved `adj-u87c2-r3-verdict-approve`), replaces the prior report's name-reference sweep - scoped to only the 596 production fns of the three god files and counting WHOLE-TREE name occurrences including every `#[cfg(test)]` body - with a whole-`src/`-tree, FILE-AWARE (an out-of-line `#[cfg(test)] mod name;` target is test code in full, transitively) production-only reference sweep, extending `tests/simplification_audit.rs`'s existing scanner rather than adding a third lexer. Four rounds each found and closed a genuine false-positive-class defect (a mod-body test leak, a `serde(default = "..")` attribute string, an impl-generics-dropping qualifier, a struct-literal-field-value/UFCS-value invisible shape) before converging on THE RULE: a production reference to fn F is any identifier token equal to F's name in production code, whatever token precedes or follows it - no reference SHAPE is enumerated at all any more, closing the whole class of "the next invisible shape" false positives the first three rounds kept finding one at a time. Output: `docs/audit/dead-code.json`, one entry per production fn with zero production references (name, file:line, visibility, ambiguity, the test-only references that kept it looking alive), drift-guarded exactly like the duplication catalog.

### 4.2 Stage 3 (spec 87 criterion 3, THIS): the dispositions

This criterion (`u87c3`) OWNS section 4's text, the dispositions, and section 6's item 0 - not a new instrument. While researching dispositions it found and fixed one real defect IN the stage-2 instrument, disclosed rather than silently folded in: an ambiguous `ImplAssoc` fn (ambiguity = a bare name shared by >= 2 production fns of the same kind, e.g. `parse`, shared here by `dash.rs`/`gate.rs`/`ledger.rs` x2/`failure.rs`) referenced ONLY via `Self::name(` from within its OWN impl block was never attributed correctly - the qualifier-resolution step captures the literal token text in front of `::`, and for `Self::parse(` that text is the keyword `Self`, never the enclosing type's own name, so the `resolved == my_qualifier` match always failed. Concretely: `DashMarker::parse` (`src/dash.rs:398`) is called only via `Self::parse(s)` inside `DashMarker::read` (`dash.rs:408`) - itself called from real production code at `main.rs:5731`, `7313` and `7629` - so it read as dead when it is genuinely alive: the dangerous false-positive direction this whole unit's own precision discipline forbids ("a false-positive dead verdict is the dangerous direction"), and one this criterion's own disposition work would have shipped as a recommended deletion of live code had it gone unnoticed. FIX (same shared instrument, not a parallel one): a `Self::name(` occurrence in the SAME FILE as the candidate's own definition now resolves to it directly - the identical same-file lexical-scope approximation the sweep already applies to a bare `Free`-fn sibling call, widened to cover the keyword `Self` too. Verified against every OTHER ambiguous name in the final candidate set (`rebuild`/`distiller.rs`, `ingest_project` x2 /`ingest.rs`, `new`/`spawn.rs`): none has a same-file `Self::name(` call, so the fix's blast radius is exactly the one candidate it removes. The committed candidate count drops from 27 to 26 as a result.

THE COUNT AND PER-FILE DISTRIBUTION (26 candidates across 12 files):

| File | Count |
|---|---|
| `src/canary.rs` | 1 |
| `src/config.rs` | 1 |
| `src/dash.rs` | 3 |
| `src/distiller.rs` | 1 |
| `src/eventstore/sqlite.rs` | 1 |
| `src/gate.rs` | 1 |
| `src/grounder/symbols/events.rs` | 1 |
| `src/grounder/symbols/model.rs` | 2 |
| `src/ingest.rs` | 2 |
| `src/ledger.rs` | 2 |
| `src/spawn.rs` | 9 |
| `src/worktree.rs` | 1 |
| **Total** | **25** |

METHODOLOGY: every one of the 26 was independently re-verified by hand (NOT taken on the instrument's word alone, per this whole audit's own "green was never sufficient" discipline) - a recursive `grep -rn` for the fn's own call-shaped name across the WHOLE `src/` tree (never scoped to a single file or glob, closing the exact `src/*.rs`-vs-`src/**/*.rs` gap this criterion's own research hit once and fixed before it could hide a real caller) to confirm no production caller exists anywhere, then a read of the doc-claimed OR actually-wired real caller/consumer to establish why. Every entry carries exactly one of three dispositions (spec 87 DISPOSITIONS): `delete` (23 entries) - the fn and the tests that reference only it, safe to remove because a genuinely EQUIVALENT, ACTUALLY-WIRED replacement already exists in production (a batched entry point, a multi-seed core, a real network probe, a direct struct literal - never "nothing needs this" alone); `keep-public-surface` (0 entries today - none of the 26 cites a real MCP/workflow-template/CLI-contract consumer, the only citations this disposition accepts); `keep-pending` (3 entries) - a shipped, spec-tested mechanism with no call site wired in yet, each citing the ALREADY-LANDED spec whose Done-when criteria the candidate's own tests prove (spec 27's digest-pool rebuild, spec 32's SDET-author toggle, spec 60's storage-level idempotency guard) - none invents a future spec number that does not exist. THE KNOWLEDGE GRAPH CROSS-CHECK (spec 85's second instrument for this section, spec 87 Design's own explicit follow-up): `rigger graph --show <entity>` degree, run for all 26 real candidates and reported per-entry in the full list below, does NOT cleanly read as "agrees (zero)" the way spec 87's Design anticipated post-spec-86 - every one of the 26 shows a non-zero degree, because the graph's `degree` counts every edge touching the node, GOVERNS edges from this very audit's own `DecisionMade`/ `ReviewFinding` events about the host FILE included, not code-call edges alone (the same depth-2-traversal GOVERNS-edge inflation this report's own section 4.0 already disclosed for `rigger graph --around`). Disclosed plainly rather than silently worked around, matching this section's own established discipline: the cross-check's real, useful signal here is not "zero" but "no candidate's degree is disproportionately large relative to its file's decision/finding volume in a way that would suggest a hidden real code-structure edge the reference sweep missed" - none does; `src/eventstore/sqlite.rs`'s `with_content_identity` (degree 32, the highest) is explained entirely by its 44 own test-only references and spec 60's own extensive documentation, not by an undiscovered caller.

### 4.3 The full list, dispositioned


**`src/canary.rs`**

- **cataloged_classes** (`src/canary.rs:180`, `pub`, KG degree 12): `delete`. cataloged_classes has no production caller anywhere in src/ (checked whole-tree, recursively). Its only two references are its own inline tests (canary.rs:1419/1433); the one behavior it exists to prove - the shipped corpus catalogs >= 3 defect classes (spec 13 unit 5) - is asserted by the_shipped_corpus_loads_and_catalogs_at_least_three_defect_classes, which is the ONLY caller and would go dead with it. run_canary (the real production review-panel loop) never consults it: corpus diversity is a load-time authoring check, not a runtime one.

**`src/config.rs`**

- **sdet_author_enabled** (`src/config.rs:452`, `pub`, KG degree 3): `keep-pending`. sdet_author_enabled has zero production callers - a real, disclosed wiring gap, not a false positive: spawn_sdet_author (conductor.rs:3980) gates the always-on SDET periphery-test author ONLY on whether an sdet-author agent is configured (self.cfg.agents.get(ROLE_SDET_AUTHOR)) and never consults self.cfg.defaults.sdet_author_enabled() at all, so today an explicit sdet_author: false in a workflow's defaults: block does nothing - the on-by-default opt-out spec 32 documents and this very method's own doc comment claims ('the conductor's build-seam reads through here') is not actually true of the shipped code. keep-pending, citing spec 32 (the sdet-author feature this toggle governs, already landed): deleting the method would delete the documented resolution authority for an already-shipped, still-referenced config field (Defaults::sdet_author) before a follow-up wires spawn_sdet_author to call it - the correct fix is wiring the call site, out of scope here (no production code changes this criterion).

**`src/dash.rs`**

- **pid_is_alive** (`src/dash.rs:425`, `pub`, KG degree 5): `delete`. pid_is_alive has no production caller. It is the RETIRED predecessor of the real marker-serving check: dash_marker_serving (main.rs:5711, the production predicate ensure_run_dashboard_at is called with) calls dash::dash_serving_on - a REAL network probe of the port - and its own doc comment explains why by name: 'A REAL network probe of the port, never a bare pid-liveness check: a marker left by a self-reaped or pid-recycled dash must never masquerade as still serving just because its pid happens to be alive'. pid_is_alive's only references are its own unit test (dash.rs:9304) and an older fixture test (main.rs:12795-12841) that injects it as a simplified closure for readability, not a claim about production behavior.
- **neighborhood** (`src/dash.rs:2815`, `pub`, KG degree 7): `delete`. neighborhood (the single-seed convenience wrapper) has no production caller. The route handler (dash.rs:3404) calls neighborhood_of (the multi-seed core neighborhood delegates to) DIRECTLY, per its own doc comment: 'the re-pointed run-tree click (spec 43) uses it to seed from a unit's several decision/finding content nodes at once'. neighborhood's 13 references are all its own tests exercising the single-seed case directly.
- **serve** (`src/dash.rs:4665`, `pub`, KG degree 3): `delete`. serve (the blocking, self-binding accept-loop entry point) has no production caller. The real dash-serving CLI path (main.rs::cmd_dash) calls dash::serve_on (main.rs:6493) with the listener bind_singleton already bound, never dash::serve, which would bind its own listener and so cannot participate in spec 62's singleton-bind-then-serve flow. (main.rs:6574's doc comment still names '[`dash::serve`]' as the accept loop that terminates - a stale intra-doc reference worth a follow-up fixing it to name serve_on, disclosed here rather than silently left.) serve's 2 references are its own smoke tests.

**`src/distiller.rs`**

- **rebuild** (`src/distiller.rs:231`, `pub` (ambiguous with src/playbooks.rs:143), KG degree 15): `keep-pending`. rebuild (the digest-pool projection rebuild, spec 27) has no production caller anywhere - src/lib.rs's mod distiller; declaration is the only mention of the module outside distiller.rs itself, confirmed by a whole-tree recursive grep. Unlike a mere convenience wrapper, rebuild's own tested behavior IS spec 27's already-landed Done-when contract (clearing and re-deriving the digest pool, run-boundary scoping, determinism) - deleting it would delete the shipped mechanism those criteria proved, not an unused alternative to one. keep-pending, citing spec 27: it is modeled directly on playbooks::rebuild, which IS wired to a CLI surface (rigger playbooks --rebuild); distiller::rebuild is the equivalent primitive awaiting its own call site (a rigger distill command, or a hook into rigger reset), never spec'd as a Done-when criterion of spec 27 itself, which is why it shipped unwired.

**`src/eventstore/sqlite.rs`**

- **with_content_identity** (`src/eventstore/sqlite.rs:192`, `pub`, KG degree 32): `keep-pending`. with_content_identity (the storage-level idempotency append-guard builder, spec 60) has no production caller: open_sqlite_store (main.rs:444), the crate's ONE sqlite event-log constructor, is a bare Store::open(path) with no guard chained, and derived_index_identity() (ingest.rs:368, the production ContentIdentity value) is only ever passed to maintenance operations (count_derived_duplicates, prune_derived_index) that take it as a query-time parameter, never to with_content_identity itself. Its 44 references are all test fixtures exercising the guard directly against a Store they construct, per spec 60 criterion 4's own Done-when text: 'it DOES prove itself by driving the STORE PORT directly'. keep-pending, citing spec 60 (already landed): this is the documented, deliberately injected-at-the-composition-root 'defense in depth so a regression upstream can never re-bloat the log' criterion 4 shipped and tested - deleting it removes a proven backstop the composition root is architecturally meant to wire in, not dead weight. Disclosed plainly: whether main.rs's real store-open path SHOULD chain it is an open question this criterion does not resolve (no production code changes here) - a follow-up spec should either wire it into open_sqlite_store or explicitly retire it, rather than let it sit silently unwired indefinitely.

**`src/gate.rs`**

- **resolve_wrapper_name** (`src/gate.rs:463`, `pub`, KG degree 3): `delete`. resolve_wrapper_name has no production caller - independently confirmed (u87c2's own decision u87c2-three-precision-fixes-from-real-tree-spot-check already found this by hand: 'resolve_build_layer duplicates its ambient-PATH read inline rather than calling it'). resolve_build_layer (gate.rs:581), its own doc comment's named sole intended caller ('kept pub as the wrapper-only building block ... resolve_build_layer composes'), instead reads std::env::var_os("PATH") itself (gate.rs:588) rather than calling resolve_wrapper_name(wrapper) - a small, confirmed duplicate-glue defect (worth folding into section 2's duplication catalog on a follow-up pass), not a sign the function is unneeded. resolve_wrapper_name's 4 references are its own tests.

**`src/grounder/symbols/events.rs`**

- **index_events** (`src/grounder/symbols/events.rs:29`, `pub`, KG degree 17): `delete`. index_events is spec 87's own Goal-worked example of the false negative this whole spec exists to fix ('a function whose only caller is its own test reads as alive: the false negative the operator predicted... e.g. src/grounder/symbols/events.rs:17 index_events, 21 test references'). Confirmed again on the current tree (now 64 test references at line 29, having grown with the test suite): no production caller anywhere.

**`src/grounder/symbols/model.rs`**

- **definitions_named** (`src/grounder/symbols/model.rs:201`, `pub`, KG degree 7): `delete`. definitions_named has no production caller - only its own module's assertion-style tests (symbols/mod.rs) use it to check index state after a build/update, never a production edge-resolution path.
- **references_named** (`src/grounder/symbols/model.rs:211`, `pub`, KG degree 10): `delete`. references_named has no production caller - the same test-only accessor shape as its sibling definitions_named immediately above it.

**`src/ingest.rs`**

- **ingest_project** (`src/ingest.rs:124`, `pub` (ambiguous with src/ingest.rs:468), KG degree 3): `delete`. ingest_project (the #[cfg(feature = "symbols")] single-event lane) has no production caller. Production calls ingest_project_batched (conductor.rs:8050/15473, main.rs:4067) exclusively - the batched entry point this fn's own doc comment already names as the thing 'existing callers discard [IngestStats] and are unaffected' by, i.e. it documents its own supersession. Ambiguous with its #[cfg(not(feature = "symbols"))] sibling at ingest.rs:468 (the textual scanner sees two same-named definitions where rustc, feature-gated, sees one); both carry the identical finding and disposition.
- **ingest_project** (`src/ingest.rs:468`, `pub` (ambiguous with src/ingest.rs:124), KG degree 3): `delete`. ingest_project (the #[cfg(not(feature = "symbols"))] light-lane no-op) has no production caller, for the identical reason as its #[cfg(feature = "symbols")] sibling at ingest.rs:124: production calls ingest_project_batched exclusively on both lanes.

**`src/ledger.rs`**

- **fully_done** (`src/ledger.rs:574`, `pub`, KG degree 6): `delete`. fully_done has no production caller. Its own doc comment's three-conjunct completion check is subsumed elsewhere: nothing in conductor.rs or main.rs calls it (confirmed whole-tree, recursively) - the wired run-completion checks it was apparently meant to serve use done() and the per-unit is_terminal predicate instead. (Line shifted 500 -> 506 -> 574: spec 88 round 3's prior_criterion_unit spec-scoping fix added 6 lines above spec_stem, then merging rigger-run's spec 88 criterion 3 (ESCALATION RESUMES) added resume_bound/ResumeGrant/UnitResumed earlier in this same file.)
- **is_integrated** (`src/ledger.rs:651`, `pub`, KG degree 3): `delete`. is_integrated has no production caller, though its doc comment claims one ('used by resume to skip completed work'): the real resume-skip logic uses is_terminal (Integrated OR Escalated - confirmed live at conductor.rs:1621/9829, ledger.rs:643, main.rs:9927/9932), which correctly subsumes is_integrated's narrower Integrated-only check (a resume must also skip an Escalated unit, which is_integrated alone would wrongly re-attempt). Superseded, not merely unused. (Line shifted 577 -> 583 -> 651: same round-3 doc-comment addition, then the same rigger-run merge shift above.)

**`src/spawn.rs`**

- **new** (`src/spawn.rs:320`, `pub` (ambiguous with src/budget.rs:57, src/dash.rs:1885, src/dash.rs:4917, src/driver/replay.rs:236, src/driver/workflow.rs:83, src/eventstore/mod.rs:99, src/eventstore/mod.rs:299, src/eventstore/mod.rs:500, src/eventstore/namespace.rs:28, src/failure.rs:218, src/ledger.rs:441, src/mcpserver.rs:116, src/watch.rs:577), KG degree 5): `delete`. SpawnRequest::new and its 7 builder methods (with_system_prompt/with_model/ with_tools/with_dir/with_blast_radius/with_title/with_reviews) plus the park convenience wrapper are ALL dead together, same root cause: the real production spawn path (driver/replay.rs:251-252 fn spawn_request(...) -> SpawnRequest { SpawnRequest { ... } }) constructs the struct via a direct struct literal and calls park_in_run(store, &req, &opts.run_id) directly (driver/replay.rs:338) - it never touches the builder or the zero-run-id park() wrapper at all. Every one of these 9 fns' references is a test fixture building a SpawnRequest by hand; spec 87's own Goal text names four of the seven builders (with_title/with_reviews/with_model/ with_blast_radius) as its worked example of confirmed dead code.
- **with_system_prompt** (`src/spawn.rs:338`, `pub`, KG degree 6): `delete`. with_system_prompt - part of the SpawnRequest builder family; see the disposition on SpawnRequest::new (spawn.rs:320) for the shared root cause and citation.
- **with_model** (`src/spawn.rs:344`, `pub`, KG degree 7): `delete`. with_model - part of the SpawnRequest builder family; see the disposition on SpawnRequest::new (spawn.rs:320) for the shared root cause and citation.
- **with_tools** (`src/spawn.rs:350`, `pub`, KG degree 5): `delete`. with_tools - part of the SpawnRequest builder family; see the disposition on SpawnRequest::new (spawn.rs:320) for the shared root cause and citation.
- **with_dir** (`src/spawn.rs:356`, `pub`, KG degree 6): `delete`. with_dir - part of the SpawnRequest builder family; see the disposition on SpawnRequest::new (spawn.rs:320) for the shared root cause and citation.
- **with_blast_radius** (`src/spawn.rs:362`, `pub`, KG degree 6): `delete`. with_blast_radius - part of the SpawnRequest builder family (one of the four spec 87 Goal names by name); see the disposition on SpawnRequest::new (spawn.rs:320) for the shared root cause and citation.
- **with_title** (`src/spawn.rs:368`, `pub`, KG degree 7): `delete`. with_title - part of the SpawnRequest builder family (one of the four spec 87 Goal names by name); see the disposition on SpawnRequest::new (spawn.rs:320) for the shared root cause and citation.
- **with_reviews** (`src/spawn.rs:374`, `pub`, KG degree 5): `delete`. with_reviews - part of the SpawnRequest builder family (one of the four spec 87 Goal names by name); see the disposition on SpawnRequest::new (spawn.rs:320) for the shared root cause and citation.
- **park** (`src/spawn.rs:399`, `pub`, KG degree 14): `delete`. park (the zero-run-id convenience wrapper around park_in_run) - part of the SpawnRequest-construction dead set; see the disposition on SpawnRequest::new (spawn.rs:320) for the shared root cause and citation. park_in_run itself (spawn.rs:410, the real park authority) is correctly NOT a candidate: its own body is the one real production reference driver/replay.rs:338 needs - this scanner counts direct references, not reachability, so park_in_run reads alive even though its only OTHER caller (park) is itself dead.

**`src/worktree.rs`**

- **is_dirty** (`src/worktree.rs:635`, `pub`, KG degree 5): `delete`. is_dirty has no production caller - one of spec 87's own two Goal-cited worked examples ('src/worktree.rs expect_merged and is_dirty'), reconfirmed on the current tree: its 3 references (src/conductor.rs and worktree.rs's own `mod tests`) are all test-only; its own body now delegates to `path_is_dirty` (spec 89 round 3), but that internal call is not a caller of `is_dirty` itself. Line shifted again, this time by spec 89 criterion 1's round 3 fix (`arch-u89c1r2-dirty-check-duplicated-and-diverges-fail-direction`): `is_dirty`'s body was replaced with a one-line delegation to the new shared `path_is_dirty` free fn (built on the pre-existing `git`/`run_git` primitives, and now also called directly by `sweep_terminal_logged` and `main.rs`'s `reclaim_orphan_scratch` so the two no longer risk diverging on how a git-status failure is read), and the doc comment naming that delegation pushes the line down 8 more, 627->635. expect_merged itself (formerly src/worktree.rs:86) is no longer a candidate at all: round 4 moved it, together with `IntegrateOutcome` and the pre-round-4 `integrate` method, into this file's own `#[cfg(test)] mod tests` (a test-only recomposition of the newly-split `merge_into_worktree`/`land`, since production - `integrate_and_emit` - now calls those two split methods directly for its own row-level durable recording and has no caller left for the combined form) - a test-scoped item is not a production dead-code candidate by this scanner's own definition, closing the finding at its root rather than re-dispositioning it in place.

### 4.4 Retired-feature remnants and stale doc claims

RETIRED-FEATURE REMNANTS. `turbovec` (spec 57, "Retire turbovec"): grepped the whole tree (`src/`, `tests/`, `docs/`, `specs/`, `Cargo.toml`) for every mention - found only the deliberate migration-error guard code (`src/grounder/mod.rs`'s `is_retired_grounder` / the loud `retired_grounder_error`) plus the tests and docs that keep it retired (`tests/turbovec_retired.rs`, `tests/turbovec_retired_cargo_boundary.rs`, `tests/grounder_name_contract.rs`, and several others naming it as a guarded-against name). Zero implementing code, zero cargo feature, zero dependency - confirmed by reading `Cargo.toml`'s `[features]` section in full (one feature, `symbols`, on by default; no `turbovec` entry anywhere). `kurrentdb` build-time feature flag (spec 47, "KurrentDB is always available"): grepped `Cargo.toml` and every file under `src/` for `feature = "kurrentdb"` / `-F kurrentdb` - zero hits outside the tests that guard against its resurrection (`tests/kurrentdb_always_available.rs`); `kurrentdb` and `tokio` are unconditional `[dependencies]` as spec 47 requires, and `testcontainers` (the adapter's contract-test-only dependency) correctly lives under `[dev-dependencies]`, never the production dependency tree. Both named retirements are fully clean - a real, evidenced negative finding, not an assumption.

STALE DOC CLAIMS. Scanned every `docs/*.md`, `README.md`, and `CONTRIBUTING.md` for any `src/**/*.rs` or `tests/**/*.rs` path-shaped substring and checked each cited path still exists on disk. Two misses surfaced (`src/bar.rs` in `docs/architecture-addendum-pit-of-success.md:252,256`; `src/modifier.rs` in `docs/architecture.md:1022,1027-1028`) - both read in context and confirmed generic illustrative examples in unrelated prose (`crates/foo/src/bar.rs` as a spec-criterion example path, `core-schema/src/modifier.rs` as an event-log worked example), never a real claim about this repository's own layout. Zero genuine dangling file references found.

## 5. Test-Suite Shape

Instrument: `tests/` holds 156 files today (spec 85's Goal cites 153 - this criterion's own two periphery files plus criterion 2's own periphery file, all landed since the Goal text was written, account for the +3), 104,685 lines by `wc -l` (this section's own prose lives inside `simplification_audit.rs`, one of the 156 files this instrument counts, so this criterion's own edits land inside this same file and this figure grows with each such edit - accurate as of this section's most recent revision, not guaranteed to remain so after further edits to this file). Subsystem grouping is a hand-derived, ordered filename-keyword rule table (mirrors criterion 1's own per-file classification convention: first-match-wins, narrowest first, an explicit residual named rather than silently dropped). The consolidation-candidate columns below cross-reference the ALREADY-COMMITTED `docs/audit/duplication-catalog.json` (criterion 2's own generator output, not re-scanned here) filtered to the 340 clusters whose every site sits under `tests/`.

### 5.1 Subsystem grouping and consolidation map

| Subsystem | Files | Lines | Consolidation note |
|---|---|---|---|
| Dashboard: KG lenses & viz (code/concepts/community/files lenses, graph exploration, overlays, viz layout) | 53 | 23,510 | Largest group by file count; owns the single strongest shared-fixture evidence in the whole suite (5.2) |
| CLI whole-binary integration (`cli.rs`, `watchdog_cli_periphery.rs`, `ci_lanes.rs`) | 3 | 28,098 | `cli.rs` alone is 27,074 of these lines; split plan at 5.3 |
| Knowledge-graph ingestion & context-graph projections | 14 | 8,952 | dedup/fold/identity concerns, several already cross-clustered with the dash/viz group |
| Conductor orchestration: gates, courier, step/run lifecycle | 19 | 8,257 | the `courier_registry_refresh_{boundary,fence,periphery}` trio (3 files) pair together in 6 clusters confined to just themselves (2-5 sites each; excludes the whole-codebase Command::new/`.rigger`-path mandatory-sweep clusters, section 2, that also happen to intersect them) |
| Reset / log compaction / store hygiene | 7 | 8,021 | `reset_menu.rs` and `reset_menu_identity_migration_periphery.rs` pair together in 3 duplication clusters |
| Worktree & scratch/mutation-scratch lifecycle | 14 | 4,592 | |
| Simplification-audit generator & its own periphery (this spec) | 3 | 5,829 | `simplification_audit.rs` is itself the single largest test file after `cli.rs` |
| Spec/handbook lint & architecture-doc integrity | 6 | 3,344 | |
| Grounding (symbols grounder, turbovec retirement, blast radius) | 8 | 3,348 | `kurrentdb_always_available.rs` and `turbovec_retired.rs` independently redefine the same 4 Cargo.toml-reading helpers plus a near-identical retired-feature-guard test (7 clusters, section 5.4/5.5) |
| Process lifecycle: no-os-kill & reap discipline | 4 | 3,320 | `no_os_kill_audit.rs` and `reap_before_removal_audit.rs` pair together in 3 clusters; each also has large internal near-duplicate families (5.5) |
| Event store & config precedence | 11 | 2,780 | |
| Canary (review-panel judge-the-judges evaluation) | 8 | 2,455 | two pairs share a near-identical fixture shape: `canary_false_positives_periphery.rs`/`canary_unattributed_rejects_periphery.rs` and `canary_item_sharding_jobs_cap_periphery.rs`/`canary_progress_hook_periphery.rs` (2 clusters each) |
| Concepts/community lens derivation & fold (non-viz) | 4 | 1,536 | `community_detection_cli.rs` and `concepts_derivation_cli.rs` pair together in 9 clusters, the densest single file-pair in the whole catalog |
| Residual (no natural larger home) | 2 | 683 | `build_budget_slots_periphery.rs`, `gitsemver_derivation.rs` - named rather than forced into an ill-fitting bucket |

Total: 156 files, 104,725 lines by this table's own per-file count (156 files summed here) against 104,685 by a fresh `wc -l` above - the ~40-line gap is `simplification_audit.rs`'s own line count moving as this section's prose is written into it (the same self-measurement the instrument paragraph above names), not a missing file; the per-file counts in the table itself are not re-derived on every such move, since doing so for all 156 files on every edit is outside this criterion's own no-new-generator-code scope.

### 5.2 Shared fixtures to extract into `tests/common`

`tests/common/mod.rs` already exists (`product_binary_from`, `rigger_bin`, `rigger_courier`, `terminate_pid`, `stop_pid`, `is_alive`, `RestoreEnvVars`) - the gap is everything duplicated OUTSIDE it. The catalog's cross-file (2+ distinct files), all-helper-function clusters (181 of the 340 test-only clusters) are the evidence; the four widest are the headline case for extraction:

- `page_script` - a small JS snippet fixture - independently redefined in 19 different files (`dup-0345`, exact; e.g. `tests/adaptive_labels_periphery.rs:52-61`, `tests/code_lens_overview_collapse_viz.rs:26-35`, `tests/concepts_lens_view_periphery.rs:689-698`, + 16 more), all inside the Dashboard/viz subsystem (5.1) - cross-validates that grouping.
- `node_available` - a viz-fixture predicate - independently redefined in 19 files; the mechanical pass also clusters it together with the `gitsemver_available`/ `npm_available` availability-check helpers (5 more sites across `src/main.rs` and three test files) into one 24-site cluster (`dup-0243`, exact).
- `temp_project` - a scratch-project-directory fixture - independently redefined in 19 files (`dup-0371`, semantic; e.g. `tests/canary_model_drift_periphery.rs:39-46`, `tests/cause_wire_periphery.rs:54-61`, `tests/cli.rs:19-29`), plus a near-identical 12-site variant (`dup-0370`) and a 13-site `run_rigger` companion helper that drives it (`dup-0372`).
- `run_stream_identity` - a store-identity fixture - independently redefined in 18 files (`dup-0377`, semantic).

Proposed home for all four: `tests/common` (the catalog's own `proposed_home` field already says so verbatim for each). Consolidating just these four collapses roughly 72 duplicate definitions into 4 shared ones - the single largest mechanical simplification this audit identifies anywhere in the test suite.

### 5.3 `tests/cli.rs` split plan

27,074 lines, 351 `#[test]` functions, 15 pre-existing internal section markers in the file: 5 full box-style banner-comment pairs (`tests/cli.rs:11256/11258`, `11816/11818`, `11914/11916`, `12372/12374`, `20514/20527`) plus 10 single-line `// --- Spec NN, criterion M` headers (`21054`, `21972`, `22066`, `23403`, `25425`, `25548`, `25632`, `25770`, `25983`, `26418`). So the file carries some existing, ad hoc organization - each single-line header names the spec and criterion whose tests follow it, not a CLI subcommand or subsystem - rather than the "genuinely flat, not internally organized" state a first read might suggest; 15 markers spread across 351 tests still fall well short of a deliberate, complete per-surface structure. This correction does not disturb the split proposed below: it replaces the file's existing ad hoc, by-spec markers with a complete, deliberate BY CLI SUBCOMMAND SURFACE organization instead. A keyword-on-test-name pass (matching each test's dominant CLI verb: `step_`, `run_`, `validate_`, `reset_`, `watch_`/`watchdog_`, `canary_`, `dash_`/`status_`, `store_`/`eventstore_`, `spawn_`/`mutation_scratch_`/`scratch_`, `review_`/`gate_`, `setup_`/`precommit_`/`hook_`, `courier_`/`registry_`, `spec_`, `replay_`, `worktree_`, `emit_`/`peers_`/`decision_`, `stats_`, `heartbeat_`/`liveness_`, `prime_`/`version_`/`init_`) only cleanly covers 274 of the 351 tests (78%) - disclosed honestly rather than overclaimed, because a real fraction of `cli.rs`'s scenarios are DELIBERATELY end-to-end (a single test legitimately drives `step` + `run` + `validate` + `dash` together to prove a cross-cutting property, e.g. `a_run_driver_auto_starts_a_reachable_dash_with_a_url_shown_in_status` or `docs_ships_graph_hygiene_guidance_to_consumers`), which a bare keyword match cannot and should not force into one bucket. The proposed split is BY CLI SUBCOMMAND SURFACE - `cli.rs`'s own natural organizing concept, since the whole file drives the `rigger` binary end to end - into per-surface files (`tests/cli_step.rs`, `tests/cli_run.rs`, `tests/cli_validate.rs`, `tests/cli_reset.rs`, `tests/cli_watch.rs`, `tests/cli_canary.rs`, `tests/cli_dash.rs`, `tests/cli_store.rs`, `tests/cli_review.rs`, `tests/cli_setup.rs`, plus a residual `tests/cli_misc.rs` for the genuinely cross-cutting scenarios), with each test's home decided by its DOMINANT scenario on a human/AI read, not a mechanical keyword match - the same discipline this audit's own responsibility map applied to unassignable functions (named, never silently forced). Cross-referencing the catalog: `cli.rs` also participates in 29 of the catalog's cross-file test-duplication clusters (the most of any single file), several paired against files that WOULD merge with it under this split (`tests/step_attention_periphery.rs`, paired in 6 clusters; `tests/watchdog_cli_periphery.rs`, paired in 2 clusters) - the split is expected to shrink, not grow, the duplication surface.

### 5.4 Duplicated helpers across test files (beyond 5.2's four headline cases)

181 test-only clusters in the committed catalog have every site as an ordinary (non-`#[test]`) helper function - the shared-fixture-extraction candidate class. Beyond the four in 5.2, the widest are: `dup-0349` (`architecture_text` / `eventstore_source` / `main_rs_source` - source-text-loading helpers for doc/architecture-integrity checks, 12 files, 15 sites); `dup-0376` (a companion, 16-file/16-site variant of 5.2's `run_stream_identity` fixture, alongside `dup-0377`'s 18-file version); `dup-0405` (`write_two_stage_workflow` / `write_budget_one_two_stage_workflow` / `write_standalone_review_workflow` - workflow-YAML-literal builders duplicated across `tests/cli.rs` and `tests/step_attention_periphery.rs`, 4 files, 15 sites); `dup-0376`/`dup-0377` (`seed_run_events`, an event-seeding helper, 6-8 files); `dup-0469` (`apply_def_json` / `apply_ref_fresh`-shaped fold-application helpers, 5 files); `dup-0474` (`community` / `concept` / `def`-named single-field constructor helpers, 6 files); `dup-0477` (`code_lens` / `concepts_lens` two-line accessor helpers, 3 files). Every one of these 181 clusters, with its full site list and the catalog's own `proposed_home`, is already machine-readable in the committed `docs/audit/duplication-catalog.json` for a follow-up consolidation spec to consume directly - not re-enumerated exhaustively here to keep this section a report, not a second copy of the catalog.

### 5.5 Table-driven test families

159 test-only clusters have every site as a `#[test]` function - a literal-differs-only-in-input family, spec 85's own named table-driven-test candidate class. The single largest anywhere in the suite: `dup-0662` (near, 42 sites, all in `tests/spec_lint.rs`, e.g. `validate_spec_reports_every_c3_defect_with_its_criterion_and_field_guide_class:54-102`, `validate_spec_attributes_a_prose_level_defect_to_no_criterion:120-163`, `validate_spec_reports_two_simultaneous_defects_on_the_same_criterion:172-209` - 42 near-identical "feed one spec fixture through `validate`, assert one expected defect/advisory line" bodies). Proposed table: `#[test] fn validate_spec_field_guide_defects() { for (fixture, expected) in CASES { ... } }` retiring all 42 named tests into one parametrized loop over a `(&str, &str)` (or richer struct) case table. Other large families: `dup-0603`/`dup-0605` (15+7 sites, `tests/reap_before_removal_audit.rs`, "one fixture function body, one exemption-coverage shape, assert covered/not-covered" - retires into one table keyed by exemption shape); `dup-0636` (11 sites, `tests/simplification_audit.rs` - this very unit's own scanner tests, a `(source, expected_tokens_or_clusters)` table candidate); `dup-0592`/`dup-0593` (11+4 sites, `tests/no_os_kill_audit.rs`, one process-termination-pattern-string per test - a `(pattern, is_caught)` table); `dup-0595` (4 sites, `tests/no_os_kill_test_helper_periphery.rs`, `terminate_pid_refuses_pid_zero` / `_pid_one` x `stop_pid_refuses_pid_zero` / `_pid_one` - a 2x2 `(helper, pid)` table). As with 5.4, the full 157-family list lives in the committed catalog by cluster id for a follow-up test-consolidation spec to consume directly.

## 6. Prioritized Plan

Twenty follow-up refactoring specs, ordered largest risk-reduction first. This section adds no new findings: every citation below points at a claim already recorded in section 1 (`docs/audit/responsibility-map.json`), section 2 (`docs/audit/duplication-catalog.json`), section 4.3 (`docs/audit/dead-code.json`, dispositioned), or sections 3 and 5's own prose. Three instruments ground every count below: the three committed JSON files (queried directly, never re-scanned) and, where a god file's own `#[cfg(test)] mod tests` boundary line is cited, a direct read of that file - the boundary line itself is not a scanner output, it is where in the file the earliest `is_test: true` entry begins. Item 0 (Tier 1) deletes 23 dead functions across 12 files (section 4.3); six of the remaining nineteen entries split a god file (tiers 2 and 3, two phases times three files); the other thirteen retire duplication or close a port gap (tiers 1, 4 and 5) - kept as separate entries throughout, per spec 85's own instruction that "the god-file splits and the duplication removals are separate entries so each can be its own run."

### 6.1 How this plan is ordered

Largest risk-reduction first is read as six tiers, ranked by the KIND of risk each entry retires, highest first:

1. Tier 1 - active correctness risk, PLUS item 0: a use case already depends on the wrong concretion, or two independent implementations of one concern can already drift apart silently (section 3's two boundary violations; the one already-drifted `/proc`-reading pair section 2 and section 3 both name) - live gaps, not just size. Item 0 (deleting the 23-function dead-code set, section 4.3) is placed here too, first of all: not a live-gap risk itself, but the cheapest, zero-behavior-change, guaranteed-safe move available (spec 87's own Done-when: "Tier 1 item 0"), and it shrinks the exact three god files tiers 2 and 3 operate on before either touches them - ordered before items 1 and 2 for that reason, per this section's own largest-first-within-a-tier rule (item 0's line delta exceeds either boundary-fix item's, section 4.3).
2. Tier 2 - god-file test-module extraction: each of the three god files' own inline `#[cfg(test)] mod tests` is the majority of that file's bulk (56-70% by boundary-line count, per a direct read of each file), and moving it is a pure relocation with no production-behavior change - the single largest safe line-count reduction in this plan, and the precondition that makes tier 3 tractable.
3. Tier 3 - god-file production splits: section 1's own proposed module tree applied to the (now much smaller) remaining production surface of each god file. Higher execution risk than tier 2 because it touches live orchestration and CLI logic, so it is sequenced after tier 2 shrinks the target first.
4. Tier 4 - named production duplication sweeps: the mechanical mandatory sweeps section 2 ran regardless of the Jaccard pass (`Command::new`, `.rigger`-path literals, sqlite `Connection::open`, error-shaping helpers), each already a single committed cluster with its own proposed home.
5. Tier 5 - test-suite consolidation: section 5's own catalogued test-only duplication. No production-correctness exposure at all (worst case a test regresses, never the product), so it is ordered ahead only of tier 6 despite touching the largest raw line count anywhere in this plan.
6. Tier 6 - remaining catalog sweep: the 327 src-touching clusters section 2 found but tiers 1 and 4 did not individually name. Unlike every other tier, none of these 327 have been read and risk-assessed one at a time the way tiers 1-4's named clusters have - they are consumed straight from the catalog - so this tier carries production-correctness exposure tiers 2, 3 and 5 do not, and is ordered last: the follow-up spec must triage each cluster's own production-or-test status before merging it, not assume tier 5's blanket test-only treatment applies here too.

Within a tier, entries are ordered largest-first by the site or line count each retires - the same rule the tiers themselves follow, applied one level down.

### 6.2 Tier 1: active correctness risk

#### 0. Delete the dead-code set

- Scope: the 23 `delete`-dispositioned entries of section 4.3 (`docs/audit/dead-code.json`) and the tests that reference only them. Every entry was independently re-verified by hand to have a real, ACTUALLY-WIRED replacement already in production - never removed merely for having zero references - so this is a pure subtraction, not a behavior change: `pid_is_alive` -> `dash_serving_on`, `neighborhood`/both `ingest_project` lanes -> their already-wired batched/multi-seed cores, `serve` -> `serve_on`, the whole `SpawnRequest` builder family (`new` + 7 `with_*` + `park`) -> `driver/replay.rs`'s direct struct literal, `is_integrated` -> `is_terminal`, `fully_done` -> `done()`, `resolve_wrapper_name`/`cataloged_classes`/`definitions_named`/`references_named`/ `expect_merged`/`is_dirty` -> confirmed genuinely unreferenced anywhere. The full per-file deletion list, by name:

  - `src/canary.rs`: `cataloged_classes` (line 180)
  - `src/dash.rs`: `pid_is_alive` (line 425), `neighborhood` (line 2815), `serve` (line 4665)
  - `src/gate.rs`: `resolve_wrapper_name` (line 463)
  - `src/grounder/symbols/events.rs`: `index_events` (line 29)
  - `src/grounder/symbols/model.rs`: `definitions_named` (line 201), `references_named` (line 211)
  - `src/ingest.rs`: `ingest_project` (line 124), `ingest_project` (line 468)
  - `src/ledger.rs`: `fully_done` (line 574), `is_integrated` (line 651)
  - `src/spawn.rs`: `new` (line 320), `with_system_prompt` (line 338), `with_model` (line 344), `with_tools` (line 350), `with_dir` (line 356), `with_blast_radius` (line 362), `with_title` (line 368), `with_reviews` (line 374), `park` (line 399)
  - `src/worktree.rs`: `is_dirty` (line 635)

- Files: `src/canary.rs`, `src/dash.rs`, `src/gate.rs`, `src/grounder/symbols/events.rs`, `src/grounder/symbols/model.rs`, `src/ingest.rs`, `src/ledger.rs`, `src/spawn.rs`, `src/worktree.rs`, plus every test file that references ONLY a deleted fn (each entry's own `test_only_references` in `docs/audit/dead-code.json` names them precisely).
- Expected line delta: negative, at least -284 production lines (the 23 function definitions measured directly - signature, doc comment, and body, walking upward over contiguous `///`/`#[...]`/blank lines the same way this audit's own scanner excludes a definition's own signature span - `src/spawn.rs` alone accounts for 74 of the 284 across its 9-function builder family), MORE negative once each entry's orphaned tests are removed too - deliberately NOT measured here: a test whose ONLY purpose is exercising a deleted fn is removed outright, but several of the 156 test files touch a candidate as ONE part of a larger fixture (e.g. `pid_is_alive` injected as a closure inside `ensure_run_dashboard_at`'s own idempotency tests, which test OTHER production behavior too) and need editing, not deletion - that per-test triage is this item's own first task, not a number this criterion's own no-production-code-changes scope should estimate.
- Risk: low. Every deletion target already has a confirmed, wired, tested replacement in production (never "nothing needs this" alone - the disposition citations name the replacement or the confirming grep); `cargo build`/`clippy -D warnings` catches any missed reference immediately (a still-referenced item cannot compile away silently), and the ambiguous `ingest_project` pair's two `#[cfg(feature)]` lanes must both be edited together or a lane-specific build breaks.
- Unblocks: shrinks `src/dash.rs`, `src/spawn.rs`, `src/ledger.rs` and the other six touched files before tiers 2-3 (god-file splits) and tier 4 (duplication sweeps, several of which touch these SAME files) operate on them; the largest, safest, zero-behavior-change line-count reduction available anywhere in this plan, so it runs first.

#### 1. Close the `AgentDriver` port gap around mutation-scratch reclaim

- Scope: `conductor.rs`'s production `reclaim_terminal_unit_mutation_scratch` (`src/conductor.rs:7091-7096`, section 3 violation 1) calls `crate::driver::replay::cache_home_from` and `crate::driver::replay::reclaim_unit_mutation_scratch` by concrete module path - two pure, driver-instance-free scratch-lifecycle utilities that do not conceptually belong to the `driver::replay` concern they currently live inside. Relocate both into a neutral module every `AgentDriver` adapter and `conductor.rs` can depend on alike (no new trait needed - neither function takes a driver instance, so this is a home fix, not a port-method fix).
- Files: `src/conductor.rs`, `src/driver/replay.rs`, a new home for the two relocated functions.
- Expected line delta: near zero net - a pure move of two functions.
- Risk: low-medium. The reclaim path is covered by spec 83's worktree-lifetime-fenced-by-spawn-liveness contract tests; those tests move with the functions, not get rewritten.
- Unblocks: retires the only `AgentDriver` port violation section 3 found.

#### 2. Close the `Grounder` port gap for whole-project batch ingest (retires dup-0204 in the same motion)

- Scope: section 3 violation 2 (`src/ingest.rs:187-211` `walk_batches`, reaching `grounder::symbols::events::project_batches_paced` and `grounder::design::events::project_batches` by concrete module path) and duplication cluster `dup-0204` (the same two modules' own twin `project_batches` functions, `src/grounder/symbols/events.rs:36-38` / `src/grounder/design/events.rs:90-114`) are one root cause, not two - fix once. TWO CANDIDATES, ONE HOME (spec 85 CONSTRAINTS WALK): `dup-0204`'s own mechanical `proposed_home` suggests relocating into `tests/common`, but both sites are production code under `src/grounder/`, not test helpers - the mechanical heuristic has no "add a port method" category to route a production duplicate to, so it mis-fires here. This plan follows section 3's own reasoned disposition instead: add a `Grounder::project_batches` port method (or a standalone `SymbolProjector` trait) covering both concrete modules, and point `ingest.rs` at it.
- Files: `src/ingest.rs`, `src/grounder/mod.rs`, `src/grounder/symbols/events.rs`, `src/grounder/design/events.rs`.
- Expected line delta: roughly neutral - one new trait method plus two thin impls, minus the two duplicate bodies `dup-0204` catalogs.
- Risk: medium. `ingest.rs`'s own module doc calls it "the ONE walk-and-content-key authority" - a load-bearing path; needs the existing whole-project-ingest and reindex-freshening coverage to stay green, not just the two duplicate-site tests.
- Unblocks: retires the one `Grounder` port violation section 3 found and `dup-0204` together, rather than as two separately-tracked fixes.

#### 3. Retire the duplicate `/proc`-reading authority (`dup-0146` + `dup-0147`)

- Scope: `src/dash.rs::process_state` (`src/dash.rs:499-507`) and `src/main.rs::pgid_of` (`src/main.rs:23346-23359`) each independently re-derive `/proc/<pid>/stat` and `/proc/<pid>/status` fields that `src/reap.rs` (`pid_starttime`/`read_ppid`, `src/reap.rs:190-207`) already parses - the exact "second mutation authority" example spec 85's own Goal names and spec 62's capstone previously caught (`dup-0147`, 15 sites: `src/dash.rs`, `src/main.rs`, `src/reap.rs`, `tests/cli.rs`, `tests/mutation_runner_pdeathsig_periphery.rs` - spec 91's own launcher-exits proving test reads `/proc/<pid>/stat` directly for the same reason `dash.rs::process_state` does, growing this already-known cluster by one site rather than opening a new one), plus 60 raw `/proc`-path string literals scattered across `src/dash.rs`, `src/main.rs`, `src/reap.rs` and four test files with no shared composer (`dup-0146`). Both clusters' own `proposed_home` agree: `src/reap.rs` becomes the one `/proc`-reading module; `dash.rs` and `main.rs` call it instead of re-parsing. NOT SYMMETRIC: `process_state` is reachable from `dash`'s own always-on production server, so it is the actual active-correctness risk this tier-1 placement is about; `pgid_of` sits inside `main.rs`'s `mod tests` (opened at `src/main.rs:12825`) and is called only by `#[test]` fns, so on its own it earns no tier-1 placement - it rides in this same item only because it shares `dup-0146`/`dup-0147`'s one root cause and one proposed fix with `process_state`, not because retiring it retires any live risk of its own.
- Files: `src/dash.rs`, `src/main.rs`, `src/reap.rs`, `tests/cli.rs` (`proc_pgid_of`, `tests/cli.rs:23650-23663`, re-points at the same call).
- Expected line delta: negative - retires `process_state`'s and `pgid_of`'s own parsing bodies in favor of calling `reap.rs`'s existing parser.
- Risk: low for both halves, for two different reasons. Section 3's own disposition already establishes `process_state` as a duplicate READ-only reimplementation, never a bypassed mutation path - nothing this touches can signal or kill a process, so it carries none of the no-os-kill gate's own risk surface. `pgid_of`'s own risk is lower still: being test-only, retiring it is ordinary test cleanup, not a correctness-risk retirement - it is sequenced here for shared-fix convenience, not because it independently needed tier-1 urgency.
- Unblocks: retires the codebase's only currently-known live instance of the "duplicate implementation reconciled after the fact" pattern the operator's strict-DRY rule targets - the concrete precedent spec 85's own Goal cites - and, as a free byproduct, `main.rs`'s own test-only duplicate parser.

### 6.3 Tier 2: god-file test-module extraction

Each god file's inline test-module boundary is the earliest `is_test: true` entry's `start_line` in the committed `docs/audit/responsibility-map.json`, cross-checked against a direct read of the file's own `#[cfg(test)]` markers. All three checks agree no file is fully flat before this boundary: conductor.rs has one small early exception (`for_test`, `src/conductor.rs:2327-2371`) and main.rs has one (`compose_precommit`, `src/main.rs:11630-11635`); dash.rs has none. Each entry below moves an already-passing test module with no intended production-behavior change - a `cargo test` pass before and after is the whole verification. The line-delta figures below are file-length minus the boundary's own start line (a direct-read fact, not a function-span sum), so they include the module-level doc comments, `use` statements and blank lines a per-function span sum would miss.

#### 4. Extract `src/conductor.rs`'s inline test module

- Scope: the file's `#[cfg(test)] mod tests` opens at `src/conductor.rs:10260` and runs to end of file - roughly 24,400 of the file's 34,677 lines (70%), 417 of its 612 mapped functions. On its own it is nearly as large as all of `tests/cli.rs` (27,074 lines). Partition into a `src/conductor/tests/` directory, one file per concern, reusing the same names section 1 already assigned the file's own production buckets (`budget`, `gate`, `review`, `run_ctx`, `schedule`, `spawn`, ...) so the split needs no new naming scheme.
- Files: `src/conductor.rs` -> `src/conductor.rs` (production only) + `src/conductor/tests/*.rs`.
- Expected line delta: 0 net (repo-wide) - roughly 24,400 lines relocated out of `conductor.rs`.
- Risk: low - mechanical move of passing tests, zero intended behavior change.
- Unblocks: shrinks `conductor.rs` from 34,677 to roughly 10,260 lines before tier 3 touches a single production line - the single largest reduction in this whole plan to the odds that an unrelated future unit's blast radius collides with this file.

#### 5. Extract `src/main.rs`'s inline test module

- Scope: the file's `#[cfg(test)] mod tests` opens at `src/main.rs:12650` (the same boundary section 3 cites for its own test-only raw-connection-bypass finding) and runs to end of file - roughly 11,374 of the file's 24,024 lines (47%), 332 of its 618 mapped functions. Same partition approach as item 4, reusing section 1's own production bucket names (`commands`, `store`, `support`, `provenance`, `dash_glue`, `setup`, ...).
- Files: `src/main.rs` -> `src/main.rs` (production only) + `src/main/tests/*.rs`.
- Expected line delta: 0 net - roughly 11,374 lines relocated.
- Risk: low, same rationale as item 4.
- Unblocks: shrinks `main.rs` to roughly 12,650 lines before tier 3's own main.rs split.

#### 6. Extract `src/dash.rs`'s inline test module

- Scope: the file's `#[cfg(test)] mod tests` opens at `src/dash.rs:4907` and runs to end of file - roughly 6,213 of the file's 11,120 lines (56%), 148 of its 263 mapped functions. Lower effort than items 4-5: section 1's own classifier already found five pre-existing sub-boundaries inside this one test module (`dash::tests::calls_route_c4`, `metadata_card_c2`, `rationale_overlay_c3`, `subject_view_c5`, `supervised_lifecycle`), so the partition points already exist and need only become their own files.
- Files: `src/dash.rs` -> `src/dash.rs` (production only) + `src/dash/tests/*.rs`.
- Expected line delta: 0 net - roughly 6,213 lines relocated.
- Risk: low - the lowest-effort of the three, for the reason above.
- Unblocks: shrinks `dash.rs` to roughly 4,907 lines before tier 3's own dash.rs split.

### 6.4 Tier 3: god-file production splits

Each entry below applies section 1's own proposed module tree to a god file's production surface, sequenced after the matching tier-2 entry removes that file's test bulk first. Every module name and function/line count below is summed directly from the committed `docs/audit/responsibility-map.json` (function-body spans only); a file's remaining non-function production lines - struct/enum/type definitions, `use` statements, module docs - are outside section 1's own function-only scan and move with whichever module they sit beside, without needing their own assignment.

#### 7. Split `src/conductor.rs`'s production code into `src/conductor/*.rs`

- Scope: 175 mapped functions across 14 proposed modules (roughly 6,850 lines of function bodies) plus 20 unassigned functions (952 lines, each individually named in the committed map for manual placement, per section 1's own "unassignable functions are named as such, never omitted" rule). Headline buckets: `conductor::run_ctx` (100 functions, 4,901 lines - more than half this remaining surface on its own), `conductor::support` (15/387), `conductor::gate` (19/133), `conductor::run` (6/122), `conductor::review` (9/102); the other nine buckets are each five functions or fewer.
- Files: `src/conductor.rs` -> `src/conductor/mod.rs` + `src/conductor/{run_ctx,support,gate,run,review,schedule,budget,prior_failure,spawn,review_outcome,ground,error,gate_ratchet,integration_approval}.rs`.
- Expected line delta: 0 net - pure relocation of roughly 7,800 lines; `run_ctx` alone may warrant its own second pass if it does not decompose cleanly into one file.
- Risk: medium-high - conductor.rs is the composition root's own most complex use-case file; every intermediate commit needs the full `cargo test`, no-os-kill and reap audits green, not just the final one.
- Unblocks: the largest reduction in production-code blast-radius collision risk this audit identifies; makes future duplication-spotting against conductor.rs's own logic tractable by a human reviewer, not only by the mechanical scanner.

#### 8. Split `src/main.rs`'s production code into `src/main/*.rs`

- Scope: 286 mapped functions across 15 proposed modules (roughly 8,340 lines of function bodies) plus 102 unassigned functions (2,674 lines). Headline buckets: `main::commands` (34/2,568), `main::store` (32/862), `main::support` (25/652), `main::provenance` (23/446), `main::dash_glue` (25/346), `main::setup` (18/271).
- Files: `src/main.rs` -> `src/main.rs` (composition root, thinned) + `src/cli/{commands,store,support,provenance,dash_glue,setup,render,liveness,store_location,run_registration,docs_overlay,scaffold_report,residue_report,store_selection,replay_runner}.rs`.
- Expected line delta: 0 net - pure relocation of roughly 11,000 lines.
- Risk: medium - `main.rs` is the composition root itself; the split must preserve which concretions get wired where, not merely move text.
- Unblocks: shrinks the second-largest god file to a genuine composition root plus a `cli/` module tree, matching the ports-and-adapters shape this project already mandates everywhere else.

#### 9. Split `src/dash.rs`'s production code into `src/dash/*.rs`

- Scope: 115 mapped functions across 9 proposed modules (roughly 2,720 lines of function bodies) plus 41 unassigned functions (848 lines). Headline buckets: `dash::render` (26/642), `dash::reproject` (12/519), `dash::server` (8/426), `dash::buckets` (8/116).
- Files: `src/dash.rs` -> `src/dash/mod.rs` + `src/dash/{render,reproject,server,buckets,registry,response,reaped_child,lens,dash_marker}.rs`.
- Expected line delta: 0 net - pure relocation of roughly 3,600 lines.
- Risk: low-medium - the smallest of the three god files by production surface, and the always-on dash's own contract (loopback-only, zero-new-dependency) is unaffected by a pure module split.
- Unblocks: completes the god-file split trio; the third program-sized file becomes an ordinary module tree.

### 6.5 Tier 4: named production duplication sweeps

Each entry is one of section 2's five named mandatory sweeps - collected mechanically regardless of the Jaccard pass, per spec 85's own Design.

#### 10. Consolidate the 655 `.rigger`-path string-literal sites (`dup-0055`) - the single largest cluster in the entire catalog by site count

- Scope: one `.rigger`-relative path-composition helper (the cluster's own `proposed_home`) every one of the 662 sites routes through instead of building its own literal.
- Files: spans dozens of files including `src/conductor.rs`, `src/config.rs`, `src/dash.rs`, `src/docs.rs`, `src/gate.rs`, `src/grounder/mod.rs`, `src/grounder/symbols/store.rs`, `src/ingest.rs`, `src/main.rs`, `src/reap.rs`, `src/registry.rs`, `src/worktree.rs` plus many `tests/` files - the full site list is in the committed `docs/audit/duplication-catalog.json` under `dup-0055` for the follow-up spec to consume directly, not re-enumerated here.
- Expected line delta: negative - 662 literal compositions collapse toward one helper's call sites; the helper itself is small.
- Risk: medium - the largest surface-area sweep in this plan by site count, even though each individual site is trivial; needs a mechanical rewrite pass plus a full-suite green run, not hand-editing 662 sites.
- Unblocks: the biggest single site-count reduction available anywhere in the duplication catalog.

#### 11. Consolidate the 333 `Command::new` call sites (`dup-0006`) behind one injected process-spawn port

- Scope: one process-spawn seam every `Command::new` site routes through (the cluster's own `proposed_home`).
- Files: spans `src/budget.rs`, `src/conductor.rs`, `src/dash.rs`, `src/driver/cli.rs`, `src/gate.rs`, `src/main.rs`, `src/worktree.rs` plus many `tests/` files - full site list in `docs/audit/duplication-catalog.json` under `dup-0006`.
- Expected line delta: negative, though smaller per-site than `dup-0055` since each `Command::new` call already carries real configuration (args, env, cwd) that must move with it, not just a literal.
- Risk: medium-high - several of these 333 sites sit inside `src/budget.rs`'s and `src/conductor.rs`'s already-hardened process-lifecycle code (spec 78's no-os-kill discipline); the follow-up spec must preserve every existing handle-bound-kill invariant at each site it touches, and the no-os-kill gate is the acceptance bar, not merely `cargo test`.
- Unblocks: one seam instead of 333 independent constructions - the next process-spawning concern added anywhere in the crate reuses it instead of adding site 334.

#### 12. Consolidate the 46 sqlite `Connection::open` call sites (`dup-0109`)

- Scope: one sqlite-connection-opening adapter function (the cluster's own `proposed_home`) spanning `src/contextgraph/sqlite.rs`, `src/eventstore/sqlite.rs` and `src/main.rs`, plus several `tests/` files.
- Files: full site list in `docs/audit/duplication-catalog.json` under `dup-0109`.
- Expected line delta: negative - 46 open calls collapse toward one function.
- Risk: medium - touches the event store and context graph's own connection-lifecycle code; needs the store-identity and store-resolution contract tests green throughout.
- Unblocks: one place to change pragma/timeout/journal-mode settings instead of 46.

#### 13. Consolidate the 5 error-shaping helper sites (`dup-0213`) - caution, confirm before merging

- Scope: the cluster spans `src/grounder/mod.rs` (`retired_grounder_error`), `src/worktree.rs` (`revert_on_base_aborts_and_errors_on_a_conflicting_revert`), `src/conductor.rs` (a `mod tests` case, `integrate_plan_commits_wraps_any_hard_error_with_the_plan_landing_marker`) and three unrelated test files, at line counts from 7 to 132 - a wide spread for one claimed duplicate. This may be a threshold-gaming false cluster (spec 85's own CONSTRAINTS WALK: "the threshold is a floor for the mechanical pass; the reading pass owns semantic duplicates") rather than one real shared concern - the follow-up spec's first job is confirming by reading whether these six sites share actual logic before proposing one helper, not assuming the cluster label proves it.
- Files: `src/grounder/mod.rs`, `src/worktree.rs`, plus the three test files named in `docs/audit/duplication-catalog.json` under `dup-0213`.
- Expected line delta: unknown pending the confirmation read above - potentially zero if the cluster does not survive a human read.
- Risk: low (the smallest-site-count sweep), but with the stated precondition.
- Unblocks: either a genuine fifth consolidation, or a documented "not a real duplicate" disposition that keeps the catalog honest for whoever reads it next.

### 6.6 Tier 5: test-suite consolidation

Every entry cites section 5's own already-catalogued test-only duplication; none of it carries production-correctness risk.

#### 14. Extract the four headline shared test fixtures into `tests/common` (section 5.2)

- Scope: `page_script` (`dup-0345`, 19 files), `node_available` (`dup-0243`, 23 files - merged with two related availability-check helpers), `temp_project` (`dup-0371`, 19 files) and `run_stream_identity` (`dup-0377`, 18 files) - roughly 72 duplicate definitions collapsing into four shared ones, the single largest mechanical simplification section 5 identifies anywhere in the test suite.
- Files: the 18 dashboard/viz test files section 5.1 already groups together, plus `tests/common/mod.rs`.
- Expected line delta: negative - each fixture's small body survives once instead of up to 18 times.
- Risk: low - test-only, and `tests/common/mod.rs` already exists with the same shape of helper (`product_binary_from`, `rigger_bin`, ...).
- Unblocks: item 17 below (the remaining test-helper clusters) reuses the same `tests/common` home this item establishes.

#### 15. Split `tests/cli.rs` by CLI subcommand surface (section 5.3's plan)

- Scope: 27,074 lines, 351 tests, split into `tests/cli_{step,run,validate,reset,watch,canary,dash,store,review,setup}.rs` plus a residual `tests/cli_misc.rs` for the genuinely cross-cutting scenarios section 5.3 names, using each test's dominant scenario (a human/AI read, not the 78%-coverage keyword match section 5.3 already disclosed as insufficient alone).
- Files: `tests/cli.rs` and the eleven new files above.
- Expected line delta: 0 net - pure relocation of 27,074 lines into eleven files.
- Risk: low-medium - the largest single test file in the repo, but a mechanical per-test move with `cargo test`'s full pass count as the verification.
- Unblocks: shrinks the catalog's most cross-clustered single file (29 duplication clusters per section 5.3) and lets item 17's remaining-clusters sweep target smaller, subcommand-scoped files.

#### 16. Convert the four largest table-driven test families into parametrized tables (section 5.5)

- Scope, largest first: `dup-0662` (42 sites, `tests/spec_lint.rs`), `dup-0603`/`dup-0605` (15+7 sites, `tests/reap_before_removal_audit.rs`), `dup-0592`/`dup-0593` (11+4 sites, `tests/no_os_kill_audit.rs`), `dup-0636` (11 sites, `tests/simplification_audit.rs` - this very generator's own scanner tests) - 90 sites across 6 clusters.
- Files: the four files named above.
- Expected line delta: negative - each family's near-identical test bodies collapse into one parametrized loop over a table.
- Risk: low - test-only, and each family already shares one body shape (section 5.5's own finding).
- Unblocks: the largest reduction in raw `#[test]` count available in the suite (roughly 90 named tests retiring toward 4).

#### 17. Sweep the remaining 177 test-only helper-duplication clusters (section 5.4, beyond item 14's four headline fixtures)

- Scope: the 181 test-only, all-helper-function clusters section 5.4 names, minus the 4 item 14 already covers - consumed directly from `docs/audit/duplication-catalog.json`, not re-enumerated here (section 5.4's own stated approach). Includes the `dup-0370`/`dup-0372` `temp_project` companion and variant clusters section 5.4 itself places in this "beyond the four" bucket.
- Files: per-cluster, from the committed catalog.
- Expected line delta: negative, cumulative across 177 clusters.
- Risk: low - test-only.
- Unblocks: closes out the helper-duplication half of the test suite's own strict-DRY exposure.

#### 18. Sweep the remaining 153 table-driven test families (section 5.5, beyond item 16's four headline families)

- Scope: the 159 test-only, all-`#[test]` clusters section 5.5 names, minus the 6 cluster ids item 16 already covers - consumed directly from `docs/audit/duplication-catalog.json`. Includes `dup-0595` (4 sites, `tests/no_os_kill_test_helper_periphery.rs`), the smallest of section 5.5's own named large families, left here rather than in item 16.
- Files: per-cluster, from the committed catalog.
- Expected line delta: negative, cumulative.
- Risk: low - test-only.
- Unblocks: closes out the table-driven-test half of the test suite's own strict-DRY exposure; combined with item 17, retires all 340 test-only clusters section 2 found.

### 6.7 Tier 6: remaining catalog sweep

Unlike tier 5, this entry's own clusters are NOT known to be test-only - each one needs its own read before merging (see `### 6.1`'s tier 6 rationale above).

#### 19. Sweep the remaining 327 src-touching duplication clusters (section 2, beyond tiers 1 and 4's 7 named clusters)

- Scope: of the catalog's 674 clusters, 340 are test-only (items 14 and 16-18 above) and 7 are the named tier-1/tier-4 items (`dup-0006`, `dup-0055`, `dup-0109`, `dup-0146`, `dup-0147`, `dup-0204`, `dup-0213`); the remaining 327 clusters touching `src/` - mostly small 2-5-site exact/near matches like the two worked examples section 2 itself opens with (`dup-0001`, `dup-0002`) - are swept here, largest exact-duplicate clusters first, consumed directly from `docs/audit/duplication-catalog.json`.
- Files: per-cluster, from the committed catalog.
- Expected line delta: negative, cumulative; the largest single contributor is whichever exact cluster has the most sites (read from the catalog at spec-writing time, not fixed here).
- Risk: low-medium - unlike tier 5, some of these clusters are production code, so each merge needs its own test-coverage check, not a blanket "test-only" pass.
- Unblocks: the last of the catalog's 674 clusters; after items 1-3 and 10-19 all land, a future spec can state and check that the duplication catalog's own drift guard finds zero live clusters left unaddressed.

### 6.8 Dead and vestigial code beyond item 0: no further follow-up

Spec 87 redid section 4 (item 0, Tier 1, above, is that redo's own real follow-up: delete the 23 `delete`-dispositioned candidates). Of section 4.3's remaining 3 candidates, all `keep-pending`, NONE gets a new refactoring-spec stub here: each already cites its own governing, ALREADY-LANDED spec as the thing a future wiring pass would extend - spec 27 for `distiller::rebuild`, spec 32 for `Defaults::sdet_author_enabled`, spec 60 for `Store::with_content_identity` - not a gap this plan should re-propose as a fresh entry - re-litigating an already-landed spec's own scope is out of place in a plan whose own rule is "adds no new findings". `keep-public-surface` is explicitly empty (0 of 26 candidates cite a real MCP/workflow-template/CLI-contract consumer) - stated so with the search that established it, never omitted (spec 85's own CONSTRAINTS WALK), the same discipline spec 85's original all-clean section 4 applied to a scope this redo has since superseded. Both named retirements (`turbovec`, `kurrentdb`, spec 85's own original section 4 finding, unaffected by spec 87's redo since neither is a `src/` production fn) are still fully clean, and the two stale-looking doc paths found remain confirmed generic illustrative examples, not real dangling references - re-verified, not re-scanned, by this criterion's own research.
