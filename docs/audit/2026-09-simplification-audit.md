# Simplification Audit - 2026-09

The audit report spec 85 derives the follow-up refactoring specs from. Six sections, each claim citing `file:line`; see `specs/85-simplification-audit.md` for scope.

## 1. Responsibility Map

Every function in `crates/rigger-conductor/src/conductor.rs`, `src/cli/mod.rs` and `crates/rigger-dash/src/dash.rs` (1754 functions total), assigned to a proposed module by `tests/simplification_audit.rs`'s deterministic scanner + rule-table classifier (never by hand). Instrument: the brace-matching scanner over the three named files.

### Proposed module tree

- `conductor::budget` (3 functions)
  - `crates/rigger-conductor/src/conductor.rs:436-438` `compensation_queued_key` - name contains "compensat" (budget accounting); grouped under `conductor::budget`.
  - `crates/rigger-conductor/src/conductor.rs:987-1016` `pending_compensations_from_log` - name contains "compensat" (budget accounting); grouped under `conductor::budget`.
  - `crates/rigger-conductor/src/conductor.rs:13580-13597` `within_budget` - name contains "budget" (budget accounting); grouped under `conductor::budget`.
- `conductor::deps` (2 functions)
  - `crates/rigger-conductor/src/conductor.rs:2051-2058` `folding` - method inside `impl <'a> Deps<'a>`; grouped with its other `Deps` methods.
  - `crates/rigger-conductor/src/conductor.rs:2063-2065` `ingests` - method inside `impl <'a> Deps<'a>`; grouped with its other `Deps` methods.
- `conductor::emit` (3 functions)
  - `crates/rigger-conductor/src/conductor.rs:468-470` `quarantine_record_key` - name contains "record" (event/decision emission); grouped under `conductor::emit`.
  - `crates/rigger-conductor/src/conductor.rs:14728-14750` `recorded_session` - name contains "record" (event/decision emission); grouped under `conductor::emit`.
  - `crates/rigger-conductor/src/conductor.rs:14832-14865` `recorded_adoption` - name contains "record" (event/decision emission); grouped under `conductor::emit`.
- `conductor::frontier` (2 functions)
  - `crates/rigger-conductor/src/conductor.rs:15008-15021` `ready` - method inside `impl Frontier<'_>`; grouped with its other `Frontier` methods.
  - `crates/rigger-conductor/src/conductor.rs:15024-15031` `check` - method inside `impl Frontier<'_>`; grouped with its other `Frontier` methods.
- `conductor::gate` (15 functions)
  - `crates/rigger-conductor/src/conductor.rs:422-429` `gate_key` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `crates/rigger-conductor/src/conductor.rs:478-485` `gate_intersects_radius` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `crates/rigger-conductor/src/conductor.rs:499-501` `unit_of_gate_key` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `crates/rigger-conductor/src/conductor.rs:511-514` `gate_key_attempt` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `crates/rigger-conductor/src/conductor.rs:563-578` `unit_gate_runs` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `crates/rigger-conductor/src/conductor.rs:591-625` `gate_evidence_block` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `crates/rigger-conductor/src/conductor.rs:658-679` `recorded_gate_outcome` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `crates/rigger-conductor/src/conductor.rs:684-686` `deferred_gate_verdict_key` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `crates/rigger-conductor/src/conductor.rs:695-697` `deferred_gate_failed_key` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `crates/rigger-conductor/src/conductor.rs:1761-1768` `no_verdict_line` - name contains "verdict" (gate execution); grouped under `conductor::gate`.
  - `crates/rigger-conductor/src/conductor.rs:1776-1787` `verdictless_reviewer` - name contains "verdict" (gate execution); grouped under `conductor::gate`.
  - `crates/rigger-conductor/src/conductor.rs:13105-13112` `with_gate_hold` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `crates/rigger-conductor/src/conductor.rs:13120-13128` `union_gates` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `crates/rigger-conductor/src/conductor.rs:13235-13242` `gate_failure_cause` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `crates/rigger-conductor/src/conductor.rs:15127-15172` `assert_no_ungated_fanout_unit` - name contains "gate" (gate execution); grouped under `conductor::gate`.
- `conductor::gate_outcome` (3 functions)
  - `crates/rigger-conductor/src/conductor.rs:728-734` `green` - method inside `impl GateOutcome`; grouped with its other `GateOutcome` methods.
  - `crates/rigger-conductor/src/conductor.rs:738-742` `fail` - method inside `impl GateOutcome`; grouped with its other `GateOutcome` methods.
  - `crates/rigger-conductor/src/conductor.rs:746-752` `cause` - method inside `impl GateOutcome`; grouped with its other `GateOutcome` methods.
- `conductor::gate_ratchet` (1 function)
  - `crates/rigger-conductor/src/conductor.rs:885-891` `for_persistent_failure` - method inside `impl GateRatchet`; grouped with its other `GateRatchet` methods.
- `conductor::ground` (1 function)
  - `crates/rigger-conductor/src/conductor.rs:13907-13916` `graph_around_recovery` - name contains "graph" (grounding integration); grouped under `conductor::ground`.
- `conductor::infra_retries` (3 functions)
  - `crates/rigger-conductor/src/conductor.rs:1974-1978` `recorded` - method inside `impl InfraRetries`; grouped with its other `InfraRetries` methods.
  - `crates/rigger-conductor/src/conductor.rs:1984-1996` `after` - method inside `impl InfraRetries`; grouped with its other `InfraRetries` methods.
  - `crates/rigger-conductor/src/conductor.rs:2000-2002` `exhausted` - method inside `impl InfraRetries`; grouped with its other `InfraRetries` methods.
- `conductor::integrated_probe` (1 function)
  - `crates/rigger-conductor/src/conductor.rs:14430-14436` `branch_commit` - method inside `impl IntegratedProbe`; grouped with its other `IntegratedProbe` methods.
- `conductor::integration_approval` (1 function)
  - `crates/rigger-conductor/src/conductor.rs:1287-1289` `approved` - method inside `impl IntegrationApproval`; grouped with its other `IntegrationApproval` methods.
- `conductor::prior_failure` (5 functions)
  - `crates/rigger-conductor/src/conductor.rs:1321-1329` `logged` - method inside `impl PriorFailure`; grouped with its other `PriorFailure` methods.
  - `crates/rigger-conductor/src/conductor.rs:1335-1344` `failed_body` - method inside `impl PriorFailure`; grouped with its other `PriorFailure` methods.
  - `crates/rigger-conductor/src/conductor.rs:1346-1352` `is_empty` - method inside `impl PriorFailure`; grouped with its other `PriorFailure` methods.
  - `crates/rigger-conductor/src/conductor.rs:1356-1374` `summary` - method inside `impl PriorFailure`; grouped with its other `PriorFailure` methods.
  - `crates/rigger-conductor/src/conductor.rs:1381-1436` `block` - method inside `impl PriorFailure`; grouped with its other `PriorFailure` methods.
- `conductor::refused_record` (2 functions)
  - `crates/rigger-conductor/src/conductor.rs:1530-1539` `reruns` - method inside `impl RefusedRecord`; grouped with its other `RefusedRecord` methods.
  - `crates/rigger-conductor/src/conductor.rs:1542-1547` `task` - method inside `impl RefusedRecord`; grouped with its other `RefusedRecord` methods.
- `conductor::review` (9 functions)
  - `crates/rigger-conductor/src/conductor.rs:348-350` `review_round_start_key` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `crates/rigger-conductor/src/conductor.rs:1675-1678` `review_retry_window` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `crates/rigger-conductor/src/conductor.rs:1725-1745` `degenerate_reviewer` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `crates/rigger-conductor/src/conductor.rs:13163-13172` `review_evidence` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `crates/rigger-conductor/src/conductor.rs:13222-13228` `review_failure_cause` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `crates/rigger-conductor/src/conductor.rs:13297-13304` `review_protocol` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `crates/rigger-conductor/src/conductor.rs:13414-13418` `rejects_without_required` - name contains "reject" (review orchestration); grouped under `conductor::review`.
  - `crates/rigger-conductor/src/conductor.rs:14681-14687` `review_round_starts` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `crates/rigger-conductor/src/conductor.rs:14718-14721` `recorded_review_round_start_sha` - name contains "review" (review orchestration); grouped under `conductor::review`.
- `conductor::review_outcome` (3 functions)
  - `crates/rigger-conductor/src/conductor.rs:932-941` `verdict` - method inside `impl ReviewOutcome`; grouped with its other `ReviewOutcome` methods.
  - `crates/rigger-conductor/src/conductor.rs:942-944` `approved` - method inside `impl ReviewOutcome`; grouped with its other `ReviewOutcome` methods.
  - `crates/rigger-conductor/src/conductor.rs:945-947` `rejected` - method inside `impl ReviewOutcome`; grouped with its other `ReviewOutcome` methods.
- `conductor::review_round` (2 functions)
  - `crates/rigger-conductor/src/conductor.rs:13374-13376` `blocks` - method inside `impl ReviewRound`; grouped with its other `ReviewRound` methods.
  - `crates/rigger-conductor/src/conductor.rs:13380-13408` `block` - method inside `impl ReviewRound`; grouped with its other `ReviewRound` methods.
- `conductor::run` (5 functions)
  - `crates/rigger-conductor/src/conductor.rs:3396-3414` `unit_started_payload` - name contains "unit" (unit run loop); grouped under `conductor::run`.
  - `crates/rigger-conductor/src/conductor.rs:13252-13257` `unit_size_cap` - name contains "unit" (unit run loop); grouped under `conductor::run`.
  - `crates/rigger-conductor/src/conductor.rs:14295-14297` `unit_branch` - name contains "unit" (unit run loop); grouped under `conductor::run`.
  - `crates/rigger-conductor/src/conductor.rs:14316-14322` `unit_worktree_dir` - name contains "unit" (unit run loop); grouped under `conductor::run`.
  - `crates/rigger-conductor/src/conductor.rs:14693-14711` `unit_status_marks` - name contains "unit" (unit run loop); grouped under `conductor::run`.
- `conductor::run_ctx` (150 functions)
  - `crates/rigger-conductor/src/conductor.rs:3419-3421` `spawn_host` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3423-3425` `emit` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3437-3452` `append_and_fold` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3466-3484` `append_and_fold_batch` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3488-3495` `stamped` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3500-3506` `emit_with_actor` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3516-3519` `emit_meta` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3529-3539` `emit_agent` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3552-3554` `emit_keyed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3565-3576` `emit_keyed_meta` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3581-3598` `keyed_event` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3604-3618` `emit_keyed_together` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3660-3703` `emit_keyed_batch` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3712-3718` `agent_model` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3743-3746` `effective_attempts` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3755-3765` `counted_attempts` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3773-3778` `logged_prior_failure` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3790-3798` `gate_verdict_high_water` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3808-3811` `attempt_gates_started` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3827-3907` `drain_compensations` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3912-3914` `read_current_run` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3927-3974` `commits_to_compensate` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3984-4016` `emit_gate_skip` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4033-4091` `emit_gate_verdict` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4106-4113` `max_retries_for` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4139-4145` `budget_tripped` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4153-4155` `spawn_is_recorded` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4171-4194` `reserve_spawn` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4203-4233` `trip_budget_breaker` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4248-4257` `halt_reason` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4262-4272` `check_coverage_or_flag` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4299-4369` `run_wave` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4378-4404` `landing_joins` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4409-4508` `settle_stage` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4522-4553` `wave_radii` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4558-4568` `partition_requested` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4570-4606` `start_and_run_stage` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4608-4699` `run_stage` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4704-4721` `stage_paused_for_review` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4733-4744` `select_review_panel` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4756-4778` `log_review_tier` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4792-4810` `assert_isolated_cwd` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4825-4897` `reviewer_spawn_opts` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4906-4921` `continue_prior_session` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4931-4957` `record_session` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4994-5008` `review_round_start_sha` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:5020-5037` `round_delta` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:5116-5318` `review_unit` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:5329-5367` `split_reject` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:5399-5455` `guard_review_round_tree` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:5482-5500` `guard_review_round_tree_on_tier_err` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:5553-5694` `spawn_sdet_author` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:5709-5727` `halted_spawn_checkpoint_permitted` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:5735-5750` `retry_infra_failure` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:5757-5779` `integrate_under_infra_retry` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:5783-5801` `emit_infra_mark` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:5807-6741` `run_single_stage` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:6747-6754` `effective_speculation_width` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:6763-6770` `speculates` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:6779-6795` `speculation_lane_worktree` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:6816-7182` `run_speculation` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:7195-7283` `emit_speculation_winner_status` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:7302-7339` `record_speculation_reject` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:7349-7382` `cancel_speculation_candidates` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:7384-7437` `run_fan_out_stage` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:7447-7654` `run_fan_out_review_loop` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:7689-7763` `run_review_agents_concurrently` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:7794-7834` `run_finding_reviewer` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:7886-8133` `run_reviewer` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:8170-8182` `gating_spawn_emitted_approve` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:8207-8212` `review_spawn_errored` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:8238-8273` `reviewer_result_is_degenerate` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:8295-8355` `run_adjudicator` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:8377-8398` `dag_unit_blast_radii` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:8410-8487` `build_dag_critique_prompt` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:8498-8611` `re_plan` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:8624-8657` `run_plan_critique_gate` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:8676-8873` `plan_critique_loop` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:8885-8915` `stopped_on_spec_defect` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:8943-8954` `build_env` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:8970-8975` `run_base_env` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9007-9013` `spawn_env` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9024-9029` `build_budget` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9054-9062` `shared_build_cache_paths` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9070-9076` `postmerge_unit_dir` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9088-9096` `run_gates` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9104-9349` `run_gates_at` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9356-9372` `classify_gate_failure` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9390-9472` `run_gate_with_taxonomy` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9517-9519` `gc_integrated_branches` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9526-9618` `gc_integrated_branches_logged` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9652-9850` `run_deferred_gates` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9861-9928` `record_gate` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9986-10576` `integrate_and_emit` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10618-10625` `integrate_plan_commits` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10627-10747` `integrate_plan_commits_inner` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10758-10774` `record_plan_intent` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10785-10812` `read_plan_landed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10820-10849` `record_plan_landed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10854-10860` `regenerate_rule_for` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10886-10928` `run_regenerate_command` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10947-10972` `regenerate_conflicted_paths` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11000-11012` `catch_up_owed_regeneration` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11024-11036` `regenerate_and_record` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11041-11047` `regenerate_pending_for` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11054-11062` `union_regenerate_pending` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11089-11117` `record_regenerate_pending` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11126-11140` `record_placeholder_staged` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11150-11167` `record_regenerate_commit` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11177-11191` `record_merge_attempt` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11197-11216` `record_merge_outcome` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11223-11238` `record_landing_intent` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11250-11270` `record_landed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11287-11301` `landed_at` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11312-11330` `record_integrate_row` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11354-11428` `spawn_conflict_resolution_implementer` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11437-11439` `build_system_prompt` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11449-11461` `ground_query` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11467-11477` `implementer_agent` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11481-11506` `plan_protocol` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11532-11534` `grounded_blast_radius` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11539-11572` `grounded_blast_radius_at` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11588-11601` `grounded_seed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11625-11653` `record_blast_radius` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11655-11661` `build_prompt` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11674-11687` `build_review_prompt` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11694-11700` `review_base_prompt` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11707-11714` `review_gate_evidence` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11726-11761` `build_prompt_with_failure` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11766-11771` `seeded_subgraph` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11778-11792` `review_resume_task` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11794-11854` `graph_context` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11866-11877` `ingest_project_into_graph` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11891-11916` `ingest_project_batches` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11921-11923` `ingest_project_into_graph` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11937-11948` `ingest_files_into_graph` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11952-11954` `ingest_files_into_graph` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11964-11986` `emit_lesson` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11994-12014` `record_lesson` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:12027-12068` `land_refused` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:12073-12079` `agent_isolated` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:12084-12090` `read_lifecycle` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:12105-12146` `land_if_landed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:12288-12421` `adopt_prior_criterion_branch` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:12449-12502` `resume_phase` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:12504-12536` `stage_worktree` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:12551-12572` `review_only_worktree` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:12584-12589` `template_gates` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:12591-13032` `harvest_proposed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:13058-13081` `resolve_served_criterion` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
- `conductor::schedule` (5 functions)
  - `crates/rigger-conductor/src/conductor.rs:1829-1835` `plan_landing_failed` - name contains "plan" (unit scheduling); grouped under `conductor::schedule`.
  - `crates/rigger-conductor/src/conductor.rs:13141-13143` `normalize_criterion_id` - name contains "criterion" (unit scheduling); grouped under `conductor::schedule`.
  - `crates/rigger-conductor/src/conductor.rs:14380-14391` `prior_criterion_unit` - name contains "criterion" (unit scheduling); grouped under `conductor::schedule`.
  - `crates/rigger-conductor/src/conductor.rs:14462-14511` `criterion_history` - name contains "criterion" (unit scheduling); grouped under `conductor::schedule`.
  - `crates/rigger-conductor/src/conductor.rs:14542-14570` `landed_criterion_unit` - name contains "criterion" (unit scheduling); grouped under `conductor::schedule`.
- `conductor::spawn` (4 functions)
  - `crates/rigger-conductor/src/conductor.rs:1480-1482` `is_parked` - name contains "parked" (spawn lifecycle); grouped under `conductor::spawn`.
  - `crates/rigger-conductor/src/conductor.rs:1617-1632` `spawn_halt` - name contains "spawn" (spawn lifecycle); grouped under `conductor::spawn`.
  - `crates/rigger-conductor/src/conductor.rs:1653-1655` `is_parked_or_refused` - name contains "parked" (spawn lifecycle); grouped under `conductor::spawn`.
  - `crates/rigger-conductor/src/conductor.rs:1908-1910` `spawn_crash` - name contains "spawn" (spawn lifecycle); grouped under `conductor::spawn`.
- `conductor::support` (17 functions)
  - `crates/rigger-conductor/src/conductor.rs:1073-1106` `conflict_regenerate_pending_from_log` - name contains "from_" (conversion helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:1185-1208` `landings_from_log` - name contains "from_" (conversion helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:1232-1263` `integrate_attempted_from_log` - name contains "from_" (conversion helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:1486-1488` `is_interrupted` - name contains "is_" (predicate helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:1493-1495` `is_store_fault` - name contains "is_" (predicate helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:2984-2986` `has_producer` - name contains "has_" (predicate helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:13216-13218` `is_infra_cause` - name contains "is_" (predicate helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:13729-13868` `write_capped_section` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:13920-13969` `write_code_neighborhood` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:14088-14167` `write_design_intent` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:14173-14188` `write_capped_decisions` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:14193-14210` `write_capped_lessons` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:14218-14243` `write_capped_findings` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:14253-14261` `write_peers_pointer` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:14274-14281` `write_lookup_pointer` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:14640-14654` `branch_is_foreign` - name contains "is_" (predicate helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:14766-14771` `is_documentation_only` - name contains "is_" (predicate helper); grouped under `conductor::support`.
- `conductor::tests` (769 functions)
  - `crates/rigger-conductor/src/conductor.rs:3324-3375` `for_test` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15424-15457` `a_fold_the_run_could_not_make_is_said_through_its_injected_log` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15463-15467` `agent_failure_as_str_round_trips_through_from_category_for_every_category` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15470-15475` `agent_failure_from_category_degrades_an_unrecognized_string_to_unknown` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15478-15480` `agent_failure_default_is_unknown` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15483-15488` `classify_failure_prefers_the_stopfailure_record_over_api_retry` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15491-15496` `classify_failure_falls_back_to_the_last_api_retry_category` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15499-15501` `classify_failure_falls_back_to_unknown_when_neither_source_has_anything` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15504-15518` `strip_failure_marker_drops_the_class_prefix_leaving_only_the_message` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15521-15524` `strip_failure_marker_passes_through_an_unmarked_error_unchanged` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15527-15546` `unit_worktree_dir_derives_deterministically_from_scratch_root_and_unit_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15549-15639` `recorded_gate_outcome_reads_the_latest_gate_run_verdict_per_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15552-15564` `verdict` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15642-15678` `unit_of_gate_key_strips_a_retry_ordinal` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15682-15688` `started_with_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15690-15695` `integrated` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15698-15706` `prior_criterion_unit_finds_a_prior_un_integrated_units_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15709-15735` `prior_criterion_unit_never_returns_an_integrated_units_id_and_never_falls_back_to_an_older_sibling` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15738-15762` `prior_criterion_unit_integration_of_one_criterion_never_masks_an_abandoned_sibling_criterion_sharing_the_same_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15765-15778` `prior_criterion_unit_tie_break_prefers_the_most_recent_of_two_non_integrated_priors` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15781-15786` `prior_criterion_unit_excludes_this_unit_itself` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15789-15801` `prior_criterion_unit_ignores_a_different_criterion_and_an_empty_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15808-15813` `run_started_with_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15818-15820` `compensated` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15825-15831` `plain_failure` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15835-15847` `assert_prior_criterion_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15927-15945` `adoption_status` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15948-15973` `recorded_adoption_ignores_a_same_identity_event_carrying_the_wrong_status` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15976-16009` `recorded_adoption_never_answers_for_a_mismatched_criterion_or_a_mismatched_spec_alone` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16012-16017` `branch_owner_returns_none_for_an_id_that_never_started` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16020-16036` `branch_owner_reads_the_most_recent_started_criterion_and_spec_for_this_bare_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16039-16054` `branch_owner_ignores_a_non_unit_started_event_even_when_it_shares_the_id_field` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16057-16070` `branch_is_foreign_is_false_when_nothing_is_recorded_or_everything_matches` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16073-16095` `branch_is_foreign_is_false_when_the_recorded_owner_has_no_criterion_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16098-16120` `branch_is_foreign_when_only_one_axis_differs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16124-16135` `assert_criterion_less_branch_is_foreign` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16172-16184` `find_unit_started` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16187-16269` `a_fresh_units_own_branch_adopts_a_prior_runs_un_integrated_unit_sharing_the_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16272-16333` `a_fresh_unit_never_adopts_a_criterion_whose_prior_attempt_already_integrated` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16336-16477` `a_halted_spawns_uncommitted_tree_is_captured_as_a_wip_commit_and_named_in_the_next_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16481-16491` `dirty_unit_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16496-16531` `assert_no_wip_recovery_commit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16534-16542` `park_implementer` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16545-16569` `a_dirty_tree_whose_named_spawn_was_never_requested_gets_no_wip_recovery_commit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16572-16607` `a_dirty_tree_gets_no_wip_recovery_commit_while_a_sibling_spawn_of_the_unit_is_still_live` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16610-16644` `a_prior_runs_leftover_spawn_request_for_a_same_named_unit_never_halts_a_new_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16656-16671` `prior_failure_summary_names_only_the_halted_commit_when_it_is_the_sole_failure` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16674-16697` `prior_failure_block_names_only_the_halted_commit_when_it_is_the_sole_failure` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16705-16737` `prior_failure_block_adds_the_generic_preamble_for_review_reject_or_contradiction_alone` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16748-16778` `each_required_pattern_item_names_only_its_shape_and_its_audit_record` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16781-16823` `a_resumed_unit_re_enters_with_the_prior_failure_block_its_retry_carried` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16830-16849` `prompts_around_a_resume` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16852-16883` `review_worktree_dir_and_branch_derive_from_stage_and_attempt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17038-17043` `answering` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17045-17080` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17083-17090` `resume_of` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17093-17095` `prompts_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17098-17100` `targets_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17103-17105` `dirs_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17109-17111` `system_prompt_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17115-17117` `title_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17121-17123` `reviews_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17127-17129` `spawn_ids` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17133-17139` `spawned` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17145-17152` `dir_existed_when_spawned` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17155-17353` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17357-17361` `integrates_a_passing_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17364-17371` `one_stage_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17375-17386` `uncovered_run_log` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17389-17399` `covers_criterion_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17464-17488` `planner_extends_the_dag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17491-17561` `planner_proposed_unit_with_a_coverage_criterion_runs_and_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17564-17648` `conductor_creates_one_baseline_unit_per_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17654-17683` `fan_out_checkin_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17686-17729` `a_stage_needing_the_fan_out_template_becomes_ready_once_every_criterion_unit_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17734-17754` `checkin_fixture_gate_calls` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17757-17798` `checkin_gates_run_only_at_checkin_once_per_spec_on_a_fixture` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17806-17808` `sweep_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17811-17816` `lens_finding` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17824-17885` `run_sweep` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17889-17895` `checkin_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17898-18011` `the_checkin_prompt_carries_each_upheld_note_of_an_integrated_unit_once` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18014-18042` `a_run_with_no_upheld_note_runs_its_checkin_stage_as_before` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18045-18171` `producer_prompt_carries_the_criteria_and_plan_protocol_grounded_on_the_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18176-18178` `baseline_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18183-18189` `supersede_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18195-18198` `append_proposal` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18201-18242` `a_prior_runs_proposal_never_resurrects_in_a_new_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18245-18319` `planner_unit_supersedes_the_matching_baseline` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18322-18398` `a_planner_supersede_of_a_fan_out_member_still_satisfies_its_downstream_needs_edge` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18401-18429` `a_criterion_with_no_planner_unit_keeps_its_baseline` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18432-18497` `planner_refinement_split_is_still_harvested` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18500-18625` `a_same_id_re_emit_updates_the_proposed_unit_in_place` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18632-18662` `seed_refine_dag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18666-18686` `append_proposals` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18689-18755` `a_coverage_retarget_re_emit_preserves_one_unit_per_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18758-18810` `a_re_emit_naming_a_reserved_stage_never_mutates_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18813-18870` `re_folding_a_same_id_re_emit_is_idempotent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18897-18903` `id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18913-19031` `assert_same_id_refine` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19131-19230` `a_real_split_two_distinct_ids_both_survive_the_fold` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19244-19255` `harvest_split_parts` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19258-19281` `a_split_criterions_parts_keep_their_own_owns_sentence_after_the_exact_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19284-19297` `a_split_parts_contract_carries_its_own_owns_sentence` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19300-19322` `an_id_less_split_part_still_supersedes_its_baseline_by_its_criterion_prefix` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19325-19342` `an_id_less_part_resolves_to_the_longest_criterion_its_text_starts_with` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19365-19372` `assert_proposal_stamps_exact_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19376-19384` `proposal_data` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19388-19394` `proposal_event` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19399-19417` `harvest_seeded` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19468-19628` `assert_landed_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19676-19719` `assert_later_episode_supersedes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19787-19813` `a_same_episode_re_seen_on_a_later_fold_still_never_self_supersedes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19822-19871` `assert_refine_survives_its_own_episodes_sibling` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19906-20016` `a_resume_catch_up_over_two_episode_supersession_and_a_split_matches_a_live_incremental_fold` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20019-20133` `a_legacy_history_resume_catch_up_matches_a_live_incremental_fold_mutual_siblings_then_superseded` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20136-20179` `a_legacy_proposal_never_supersedes_an_identified_episodes_owner_even_when_logged_later` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20182-20273` `a_late_re_emit_never_mutates_a_started_or_terminal_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20277-20296` `append_gated_proposal` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20299-20378` `harvest_proposed_gates_every_case_with_the_templates_list_unioned` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20381-20419` `harvest_proposed_gate_inheritance_survives_a_resumed_window` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20422-20439` `plan_protocol_tells_the_planner_gates_come_from_the_template` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20445-20454` `has_unmatched_signal` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20467-20521` `assert_supersedes_criterion_a_once` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20593-20643` `a_genuinely_new_proposal_runs_and_records_an_unmatched_signal` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20646-20703` `a_genuinely_new_proposal_with_no_gates_still_spawns_gated_via_template_inheritance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20706-20829` `resume_dedups_baselines_before_running_them` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20832-20903` `decomposes_the_real_spec_01_into_per_criterion_units` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20906-20936` `ratchet_promotes_a_reliable_gate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20939-20992` `elevated_gate_is_never_promoted_to_silent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20995-21032` `learns_from_escalation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21035-21089` `feeds_graph_decisions_into_the_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21092-21241` `every_unit_prompt_leads_with_its_name_and_verbatim_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21250-21316` `the_lookup_pointer_names_only_the_three_verbs_on_every_slice` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21319-21420` `decisions_prompt_injection_is_capped_under_budget_with_elision_note` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21425-21446` `render_capped_decisions` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21450-21458` `fold_subgraph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21461-21471` `render_capped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21474-21504` `a_superseded_decision_never_outranks_a_current_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21507-21560` `grounding_still_surfaces_a_prior_run_decision_that_peers_labels_historical` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21563-21590` `the_verbatim_count_cap_binds_on_many_small_decisions` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21593-21625` `the_byte_budget_cap_binds_before_the_count_on_chunky_decisions` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21628-21695` `a_kept_decision_restores_the_dropped_dependency_it_supersedes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21701-21711` `render_capped_findings` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21718-21756` `assert_capped_with_elision` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21759-21802` `findings_prompt_injection_is_capped_under_budget_with_elision_note` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21805-21840` `the_verbatim_count_cap_binds_on_many_small_findings` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21846-21875` `a_record_citing_lines_of_a_file_renders_in_the_files_sections` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21878-21987` `grounding_omits_a_resolved_finding_and_keeps_the_open_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21993-22005` `render_capped_lessons` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22008-22065` `the_injected_slice_is_deduplicated_by_normalized_text` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22068-22214` `dedup_and_restore_are_render_only_no_event_no_projection_mutation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22232-22373` `structural_grounding_is_one_seeded_traversal_over_the_unified_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22390-22511` `the_grounding_path_populates_the_unified_graph_from_the_live_project` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22519-22599` `re_ingesting_re_extracts_a_changed_file_and_skips_unchanged_ones` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22610-22698` `ingest_files_into_graph_is_bounded_to_the_named_files_and_reflects_their_live_content` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22742-22749` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22752-22759` `forwarding` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22762-22767` `of_group` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22770-22772` `refuse_next_append` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22777-22790` `append` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22792-22810` `latest_in_group` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24413-24467` `a_landed_unit_whose_reindex_lookup_fails_resumes_to_one_file_touched_and_one_integration` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24474-24541` `a_landing_appends_only_its_keyed_post_merge_verdicts` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24549-24609` `a_regeneration_committed_at_landing_reaches_file_touched_and_the_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24628-24711` `re_excluding_the_same_file_twice_in_one_process_retires_its_middle_generation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24721-24830` `a_second_run_over_an_unchanged_tree_appends_no_derived_index_event` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24841-24843` `walk_entries` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24850-24866` `spec60_cold_rebuild` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24878-24909` `spec60_reachable` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24915-24924` `spec60_reached_entities` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24937-25091` `editing_one_file_between_runs_re_emits_only_that_files_batch_and_supersedes_its_edges` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25103-25232` `a_file_reverted_to_an_earlier_recorded_generation_re_ingests_and_matches_a_cold_rebuild` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25256-25379` `a_prior_runs_non_ingest_replay_key_never_suppresses_this_runs_keyed_emit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25389-25474` `the_ingest_sink_appends_and_folds_once_per_file_batch_never_once_per_event` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25399-25407` `append` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25497-25663` `design_intent_grounding_renders_the_governing_rule_and_specifying_ra_by_traversal` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25666-25736` `a_governing_decision_never_leaks_into_the_design_intent_section` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25739-25795` `the_design_intent_section_renders_the_newest_binding_and_elides_the_oldest` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25800-25820` `assert_renders_no_section` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25876-25902` `render_code_neighborhood` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25905-25941` `the_code_neighborhood_elision_note_names_graph_around_recovery_not_peers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25944-25989` `the_code_neighborhood_byte_cap_elides_before_the_count_cap_is_reached` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25994-26011` `the_code_neighborhood_lists_no_data_member_and_no_declaration` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26014-26055` `lessons_prompt_injection_is_capped_under_budget_with_elision_note` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26058-26095` `the_verbatim_count_cap_binds_on_many_small_lessons` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26101-26114` `render_capped_lessons_scoped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26117-26154` `lessons_rank_by_blast_radius_relevance_over_pure_recency` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26157-26178` `resume_skips_already_integrated_units` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26182-26199` `seed_events_in_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26202-26256` `replay_trajectory_keeps_only_the_world_inputs_and_restrips_the_run_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26261-26263` `commit_on_unit_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26266-26271` `unit_event` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26276-26281` `seed_prior_window_s` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26285-26312` `reviewed_merge_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26317-26327` `approving_panel_stub` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26332-26354` `assert_resume_integrates_s` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26357-26386` `resume_reuses_a_units_branch_instead_of_reimplementing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26393-26425` `assert_branch_gc_resume` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26428-26452` `branch_gc_reclaims_integrated_units_and_retains_escalated_ones_on_resume` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26456-26473` `seed_branch_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26480-26488` `commit_on_named_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26492-26500` `seed_unit_branches` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26503-26509` `started_on_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26512-26517` `escalated` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26521-26537` `integrated_with_straggler` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26540-26547` `resume_in` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26551-26564` `gc_logged` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26579-26615` `branch_gc_falls_back_to_the_derived_branch_when_unitstarted_recorded_no_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26618-26678` `branch_gc_reclaims_the_recorded_branch_not_the_derived_name_when_they_differ` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26689-26713` `seed_lingering_worktree_on_unit_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26719-26724` `worktree_registered_on` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26727-26819` `branch_gc_removes_a_lingering_worktree_before_reclaiming_the_branch_on_resume` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26822-26832` `branch_gc_reclaims_every_integrated_unit_in_one_resume_not_just_the_first` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26835-26880` `branch_gc_fences_reclaim_behind_an_in_flight_straggler_spawn_and_reclaims_once_it_answers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26883-26913` `gc_integrated_branches_logged_prints_kept_evidence_for_an_in_flight_straggler_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26916-26962` `gc_integrated_branches_logged_prints_removing_evidence_for_a_terminal_spawns_decision` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26965-27004` `gc_integrated_branches_logged_stays_silent_for_an_already_gone_worktree_but_still_reclaims_an_orphaned_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27007-27056` `gc_integrated_branches_logged_does_not_repeat_removing_evidence_once_the_real_removal_already_happened` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27059-27105` `review_boundary_events_carry_the_worktree_sha` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27108-27152` `a_review_reject_unitfailed_carries_the_worktree_sha` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27155-27194` `an_exhaustive_gate_failure_on_an_approved_unit_records_a_gate_cause` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27197-27314` `stamps_the_model_alias_and_resolved_id_on_live_lifecycle_events` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27320-27336` `degenerate_reviewer_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27339-27400` `a_degenerate_adjudicator_result_respawns_and_a_substantive_retry_folds_normally` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27408-27412` `charged_an_attempt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27415-27468` `a_verdictless_adjudicator_result_re_drives_the_review_once_and_charges_no_attempt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27471-27524` `a_second_verdictless_adjudicator_result_halts_loudly_and_charges_no_attempt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27527-27578` `a_recorded_verdictless_adjudicator_result_re_parks_the_review_on_the_stepwise_host` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27581-27697` `a_review_stage_error_result_re_parks_a_fresh_attempt_no_charge_then_a_real_verdict_folds` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27737-27827` `assert_a_review_window_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27897-27982` `a_gating_spawn_that_emits_an_approve_verdict_but_returns_no_verdict_line_hard_errors_with_the_result_channel_fix` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27987-28009` `assert_ordinary_reject` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28030-28128` `the_workflow_live_path_correlates_the_approve_by_stamp_not_a_bare_stream_position` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28131-28176` `a_degenerate_lens_result_respawns_the_lens_before_the_review_proceeds` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28179-28232` `a_lens_that_emitted_a_finding_but_reports_empty_stdout_is_not_degenerate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28235-28326` `a_reviewer_that_only_ever_returns_degenerate_output_halts_the_run_loudly_naming_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28330-28335` `approved_but_unmerged_s` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28338-28359` `resume_integrates_an_already_approved_unit_without_re_reviewing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28362-28431` `a_failed_unit_is_not_terminal_and_resumes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28434-28539` `remediation_attempts_accumulate_across_resume_and_escalate_at_the_bound` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28542-28619` `an_escalated_unit_stays_terminal_on_resume` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28622-28672` `a_fresh_unit_with_no_branch_runs_the_full_lifecycle` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28675-28723` `agent_decision_folds_content_but_no_agent_attribution` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28728-28759` `planner_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28762-28776` `scope_creep_refuses_a_criterionless_proposed_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28779-28801` `adjudicator_reject_blocks_the_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28805-28815` `approving_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28819-28841` `assert_three_tier_order` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28844-28872` `adversary_runs_between_the_lenses_and_the_adjudicator` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28875-28914` `adjudicator_reject_gates_even_with_an_adversary_present` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28917-28975` `unit_reviews_itself_within_its_own_lifecycle` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28978-28988` `depth_tiers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28992-29000` `full_panel_with_tiers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29005-29031` `path_is_high_risk_matches_by_prefix_and_by_glob` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29034-29043` `route_review_tier_with_no_policy_is_the_full_panel_unchanged` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29046-29065` `route_review_tier_routes_a_low_risk_unit_to_the_light_panel` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29068-29083` `route_review_tier_forces_full_on_a_high_risk_path_hit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29086-29099` `route_review_tier_forces_full_over_the_size_threshold` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29102-29115` `route_review_tier_forces_full_when_the_gates_flapped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29118-29152` `route_review_tier_fails_safe_to_full_on_an_empty_blast_radius` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29158-29211` `run_tiered_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29215-29226` `logged_review_tier` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29229-29255` `a_low_risk_unit_runs_the_light_panel_and_logs_the_routing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29263-29304` `tiered_review_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29307-29324` `a_low_risk_unit_skips_the_adversary_and_extra_lens` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29327-29353` `a_high_risk_unit_runs_the_full_panel_and_logs_the_routing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29356-29371` `without_a_depth_policy_a_grounded_unit_runs_the_full_panel_and_logs_no_routing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29374-29410` `a_zero_grounding_unit_fails_safe_to_the_full_panel_and_logs_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29413-29526` `a_flapped_unit_escalates_from_light_at_attempt_0_to_full_on_remediation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29529-29549` `a_stage_level_tiers_policy_routes_the_unit_by_risk` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29552-29627` `every_spawn_runs_in_a_worktree_never_the_main_repo_checkout` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29631-29654` `spec_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29657-29744` `speculation_width_2_runs_two_candidates_first_green_wins_rest_cancelled` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29747-29778` `speculation_budget_accounts_for_every_candidate_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29781-29817` `speculation_defaults_off_runs_a_single_candidate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29820-29855` `speculation_parks_all_candidates_together_in_one_step` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29858-29984` `speculation_defers_green_until_the_winner_and_integrates_across_replay_steps` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29990-30000` `unit_has_status` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30004-30006` `branch_present` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30009-30078` `speculation_later_candidate_wins_when_an_earlier_lane_is_review_rejected` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30081-30203` `speculation_low_risk_later_lane_winner_routes_light_not_falsely_flapped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30206-30257` `speculation_rejected_loser_is_visible_to_the_review_quality_fold` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30260-30322` `speculation_escalates_when_every_candidate_is_rejected` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30325-30385` `speculation_crashed_lane_is_absent_and_a_sibling_still_wins` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30388-30450` `speculation_exhaustive_integrate_door_red_blocks_a_candidate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30453-30564` `speculation_stays_fresh_across_parking_until_a_winner_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30579-30648` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30652-30823` `speculation_blocked_winner_captures_evidence_and_a_later_lane_wins` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30831-30840` `lane_0_merge_break` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30843-30849` `approving_speculation_stub` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30852-30908` `a_resumed_speculating_unit_re_enters_every_lane_with_its_logged_post_merge_block` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30911-30977` `a_compensated_speculating_unit_never_re_enters_with_an_earlier_lanes_logged_block` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30980-31021` `assert_isolated_cwd_refuses_empty_or_repo_root_with_a_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31024-31105` `the_conductor_threads_each_agents_persona_to_the_driver` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31108-31142` `the_system_prompt_carries_the_communication_built_in_once` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31145-31182` `an_agent_with_no_persona_threads_an_empty_system_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31185-31259` `operator_instructions_reach_every_spawned_agent_after_every_built_in` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31262-31317` `planner_proposed_unit_inherits_the_default_review_panel` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31320-31408` `a_producer_stage_skips_the_three_tier_review_and_unblocks_its_dependents` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31411-31488` `plan_stage_commit_under_specs_reaches_the_run_branch_before_the_next_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31491-31561` `plan_stage_commit_outside_specs_fails_the_stage_naming_the_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31564-31643` `plan_stage_commit_reverting_its_own_out_of_scope_touch_still_fails_the_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31646-31712` `plan_stage_commit_conflicting_with_a_concurrent_specs_change_escalates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31715-31835` `integrate_plan_commits_is_idempotent_on_a_resumed_already_landed_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31838-31894` `integrate_plan_commits_tolerates_a_pre_existing_intent_record_with_no_git_mutation_yet` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31897-31970` `integrate_plan_commits_keeps_the_earlier_commits_identity_when_the_worktree_grows_between_calls` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31983-31999` `append` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32004-32066` `integrate_plan_commits_wraps_any_hard_error_with_the_plan_landing_marker` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32069-32146` `a_plan_landing_infra_fault_halts_the_run_loudly_with_no_per_unit_lesson_or_attempt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32150-32177` `per_unit_panel_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32181-32183` `adjudicator_at` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32189-32218` `run_review_rounds` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32225-32256` `the_round_two_review_prompt_carries_the_prior_required_list_and_the_delta_base` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32260-32279` `session_stub` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32283-32289` `session_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32292-32294` `run_session_rounds` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32297-32313` `run_session_rounds_of` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32320-32373` `a_relaunched_driver_continues_the_session_the_log_recorded` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32390-32400` `gated_seam_stub` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32403-32408` `keyed_worktree_sha` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32418-32503` `assert_a_gated_attempt_never_re_runs_the_sdet_seam` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32527-32568` `a_later_round_resumes_each_roles_session_with_what_changed_alone` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32572-32581` `repeated_kind_panel` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32588-32602` `listed_panel_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32605-32616` `run_listed_panel` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32622-32672` `a_list_repeating_a_kind_spawns_each_step_under_its_token_with_its_roster` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32678-32694` `a_repeated_list_is_judged_by_its_adjudicator_step_and_stamped_only_by_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32697-32704` `repeated_kind_session_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32710-32735` `a_rejected_list_runs_a_next_round_and_each_step_token_resumes_its_own_session` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32740-32790` `a_step_token_is_the_by_of_its_protocol_and_the_actor_of_its_findings` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32795-32821` `a_degenerate_later_step_respawns_and_halts_under_its_token` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32828-32852` `a_resumed_review_tier_is_sent_the_gate_evidence_of_the_commit_it_now_judges` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32858-32887` `a_gate_only_failure_resumes_the_sdet_authors_session_with_the_gate_evidence` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32894-32938` `a_fallen_back_resume_is_recorded_as_requested_but_not_continued` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32947-33034` `only_a_spawn_answering_as_another_session_is_logged_as_a_fallback` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33041-33061` `a_later_round_continues_the_respawn_that_produced_the_rounds_output` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33068-33121` `each_spawns_session_is_recorded_in_the_log_and_read_back_from_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33126-33144` `a_driver_reporting_no_session_keeps_every_spawn_fresh` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33150-33167` `a_later_round_reject_naming_no_required_item_respawns_the_adjudicator` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33170-33177` `lessons_about` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33184-33214` `a_round_two_reject_on_only_out_of_delta_wording_lands_and_records_operator_lessons` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33220-33247` `a_correctness_item_outside_the_delta_still_rejects` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33253-33279` `a_later_round_infra_fault_reject_naming_no_item_reruns_uncharged` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33285-33311` `a_later_round_infra_fault_reject_is_never_split` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33316-33351` `an_infra_rerun_records_its_own_operator_lessons` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33360-33425` `a_conductor_lesson_that_names_no_file_names_the_runs_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33362-33364` `bare` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33365-33368` `empty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33369-33373` `named` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33374-33384` `pathless_item` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33428-33458` `per_unit_adjudicator_reject_blocks_integration_and_escalates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33461-33512` `an_adjudicator_infra_fault_reject_reruns_the_review_without_charging` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33528-33549` `two_lens_solo_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33560-33634` `assert_a_live_review_crash_reruns_the_review_uncharged` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33682-33765` `an_always_rejecting_adjudicator_escalates_after_exactly_max_retries_cycles` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33768-33829` `a_higher_max_retries_gives_more_attempts_before_escalation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33832-33886` `an_absent_max_retries_preserves_the_default_bound_of_three` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33889-33971` `a_resumed_unit_gets_exactly_its_granted_extra_attempts_before_re_escalating` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33974-34007` `max_retries_for_widens_only_the_resumed_unit_never_a_sibling` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34010-34049` `a_stages_own_max_retries_overrides_the_run_default_for_its_units` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34067-34073` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34076-34110` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34119-34143` `assert_final_attempt_approval_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34146-34160` `approval_on_the_final_permitted_attempt_integrates_a_per_unit_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34163-34246` `a_model_ladder_implementer_escalates_one_rung_per_remediation_attempt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34249-34277` `approval_on_the_final_permitted_attempt_integrates_a_standalone_review_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34281-34287` `failing_gate_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34290-34299` `status_mark_keys` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34302-34361` `an_infra_spawn_crash_retries_under_a_retry_id_and_charges_nothing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34382-34491` `assert_an_outage_past_the_infra_limit_halts` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34524-34532` `gated_by_ok` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34535-34541` `agent_a_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34545-34549` `budget_of_one_over_two_units` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34553-34555` `assert_attention` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34595-34669` `a_budget_halt_does_not_restamp_on_a_later_poll_with_nothing_new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34672-34676` `started_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34680-34684` `replay_step` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34689-34695` `fail_attempt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34698-34762` `a_delayed_budget_halt_after_a_dependency_unlocks_still_stamps` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34765-34810` `compute_attention_leaves_the_hung_liveness_halt_to_the_caller` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34815-34826` `parked_unit_u` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34829-34869` `an_escalation_does_not_restamp_attention_on_a_resumed_process` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34872-34883` `a_clean_step_stamps_no_attention` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34886-34946` `a_third_failure_recurs_and_stalls_the_frontier` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34954-34964` `attention_across` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34967-34977` `u_failed_at` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34980-34989` `u_escalated_and_resumed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35048-35084` `nine_of_ten_budget_crosses_the_final_tenth` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35087-35159` `a_re_step_already_past_the_budget_threshold_does_not_re_stamp` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35162-35166` `budget_of_one_over_a_chain` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35169-35183` `budget_breaker_stops_the_run_after_the_first_wave` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35186-35192` `budget_exhaustion_aborts_the_task` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35195-35205` `a_budget_halt_surfaces_its_reason_on_the_run_state` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35208-35218` `a_run_within_budget_surfaces_no_halt_reason` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35222-35239` `with_budget_two_ctx_over` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35242-35285` `the_spawn_budget_folds_from_recorded_spawn_requests_across_steps` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35288-35309` `the_pre_wave_breaker_trips_only_on_a_new_over_budget_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35312-35375` `a_review_tier_budget_refusal_aborts_with_budgetexhausted_not_a_raw_error` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35390-35399` `run_ungated_fanout_guard` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35402-35430` `ungated_fanout_unit_under_a_gated_template_fails_the_step` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35433-35456` `ungated_fanout_unit_under_an_ungated_template_is_not_flagged` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35459-35480` `gated_fanout_unit_passes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35483-35507` `non_fanout_stage_with_no_gates_is_never_flagged` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35510-35545` `unmatched_fanout_proposal_under_a_gated_template_fails_the_step` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35548-35572` `unmatched_fanout_proposal_under_an_ungated_template_is_not_flagged` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35575-35592` `ungated_fan_out_templates_names_a_gateless_template` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35595-35607` `ungated_fan_out_templates_is_silent_on_a_gated_template` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35610-35623` `planner_covering_every_criterion_passes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35626-35654` `manual_stage_pauses_while_an_auto_stage_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35658-35679` `spawned_in_a_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35682-35700` `isolation_none_agent_gets_no_worktree_even_with_a_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35703-35711` `spawn_opts_isolation_is_set_for_a_worktree_agent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35714-35733` `a_spawned_implementers_title_is_the_unit_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35740-35769` `roster_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35773-35789` `per_unit_roster_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35793-35800` `assert_full_panel_roster` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35825-35837` `a_panel_with_no_adversary_never_fabricates_one_in_the_adjudicators_roster` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35846-35914` `the_roster_reflects_the_actually_routed_light_panel_not_the_full_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35928-35965` `the_fan_out_review_loops_spawns_are_stamped_with_the_routed_roster` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35977-36060` `review_identity` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36066-36069` `assert_review_identity` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36073-36083` `tiered_review_panel` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36182-36245` `stage_autonomy_override_seeds_the_gate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36248-36288` `on_pass_none_runs_gates_but_does_not_integrate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36299-36314` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36323-36352` `two_unit_gate_runs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36357-36386` `assert_one_isolated_dir_per_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36389-36417` `two_units_gate_environments_never_share_a_target_dir` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36420-36436` `two_units_gate_environments_never_share_a_gate_scratch_root` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36439-36488` `a_gate_round_never_creates_the_gate_scratch_root` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36497-36501` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36504-36513` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36521-36644` `one_build_environment_authority_reaches_both_a_gate_build_and_an_agent_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36536-36569` `run_once` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36647-36713` `spawn_env_adds_the_per_unit_cargo_target_dir_only_for_a_real_unit_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36716-36775` `a_units_per_unit_cache_is_reclaimed_when_its_worktree_is_removed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36778-36846` `a_units_worktree_cache_and_branch_are_all_reclaimed_on_a_successful_integrate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36849-36912` `a_units_worktree_is_reclaimed_but_its_branch_survives_a_terminal_escalation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36916-36941` `judged_unit_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36946-36952` `parking_stub` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36955-36961` `parked_judge_stub` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36965-36973` `unit_branch_tip` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36979-36993` `assert_unit_worktree_at` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36996-37045` `a_parked_review_spawn_keeps_the_unit_worktree_registered_and_its_cache` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37048-37088` `review_unit_restores_a_worktree_a_gate_deleted_out_of_band` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37091-37146` `a_second_step_restores_a_still_parked_units_worktree_deleted_out_of_band` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37149-37243` `verified_worktree_sha_is_stamped_after_a_gate_side_deletion_is_restored` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37246-37319` `review_tier_boundary_restores_a_worktree_a_prior_tier_deleted` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37322-37426` `reviewed_worktree_sha_is_stamped_after_the_adjudicators_own_deletion_is_restored` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37429-37433` `has_real_worktree_sha` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37437-37456` `rejecting_deleting_judge_log` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37459-37476` `failed_worktree_sha_is_stamped_after_the_adjudicators_own_deletion_is_restored` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37480-37490` `status_sha` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37496-37520` `assert_residue_named_and_never_merged` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37525-37550` `approving_round_with_residue` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37553-37595` `a_review_rounds_dirty_residue_is_restored_named_and_never_merged` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37598-37622` `a_review_rounds_moved_tip_is_restored_to_the_reviewed_sha_before_merging` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37625-37760` `a_review_rounds_lens_residue_survives_a_later_tiers_genuine_crash_and_is_restored` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37766-37774` `residue_then_park_stub` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37781-37801` `park_then_resume` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37807-37829` `assert_one_log_derived_round_start` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37839-37887` `assert_resumed_round_judges_the_logged_start` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37940-38003` `a_speculation_lanes_log_derived_start_sha_survives_a_cross_call_resume_after_a_park` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38006-38046` `a_resumed_reviewed_unit_restores_a_worktree_the_exhaustive_gate_deleted` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38053-38081` `assert_a_fresh_process_re_enters_s_with_its_logged_gate_evidence` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38084-38180` `a_resumed_reviewed_unit_whose_exhaustive_gate_fails_records_a_gate_cause` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38183-38284` `a_resumed_reviewed_units_infra_red_reruns_the_exhaustive_suite_without_charging` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38287-38420` `a_resumed_reviewed_units_merge_break_records_an_integrate_conflict_cause` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38423-38553` `a_resumed_landed_but_ungated_unit_regates_the_landed_tree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38556-38727` `a_resumed_reviewed_units_genuine_unresolved_conflict_reaches_the_idempotent_merge_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38745-38761` `regenerable_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38764-38796` `regenerate_conflicted_paths_returns_the_real_regeneration_commit_sha` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38802-38829` `a_probe_fault_on_the_regenerate_command_returns_the_halt_and_commits_nothing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38832-38902` `a_live_approved_unit_restores_a_worktree_the_integrate_door_exhaustive_gate_deleted` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38905-39018` `speculation_lenses_restore_a_gate_deleted_worktree_without_racing_and_stamp_the_winner_sha` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39021-39025` `speculating_judged_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39028-39049` `speculation_reject_worktree_sha_is_stamped_after_the_adjudicators_own_deletion_is_restored` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39052-39125` `speculation_winner_reviewed_sha_stays_the_round_start_sha_across_a_post_review_regen_commit_and_integrate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39128-39231` `a_parked_lens_keeps_the_unit_worktree_beside_a_genuine_sibling_crash` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39234-39283` `a_worktree_less_stage_gate_inherits_the_shared_target` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39286-39312` `bounded_pool_completes_every_stage_under_the_cap` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39315-39371` `run_wave_hands_each_freed_slot_to_the_next_ready_stage_under_max_parallel_units` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39379-39419` `a_stage_a_landing_makes_ready_starts_in_that_wave_under_max_parallel_units` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39422-39490` `run_wave_leaves_a_stage_the_width_refuses_neither_terminal_nor_integrated_across_waves` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39493-39566` `occupancy_survives_a_crash_resume_so_a_still_parked_unit_keeps_its_slot_over_a_fresh_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39574-39597` `a_step_that_does_not_ingest_costs_the_run_and_the_typed_carry_over_per_read` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39604-39658` `a_step_that_parks_and_replays_reads_only_the_run_and_the_typed_carry_over` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39667-39728` `a_step_that_starts_a_criterion_unit_in_a_repo_reads_adoption_by_lifecycle_type` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39743-39957` `a_step_that_ingests_seeds_each_identity_by_group_lookup_and_appends_only_what_moved` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39966-40026` `a_step_whose_group_lookup_goes_unanswered_fails_and_appends_nothing_for_that_batch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40029-40192` `a_step_whose_width_is_filled_by_parked_units_returns_instead_of_re_reading_the_stream` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40042-40049` `charge` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40052-40059` `append` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40060-40068` `read_stream` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40069-40076` `read_all` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40077-40084` `read_all_page` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40085-40089` `log_head` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40090-40096` `subscribe_all` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40097-40103` `subscribe_stream` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40104-40110` `last_position` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40111-40119` `read_stream_typed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40120-40130` `read_stream_positions` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40131-40140` `read_stream_batched` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40141-40148` `latest_in_group` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40195-40234` `live_run_folds_no_gate_machinery` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40239-40249` `spawned_parallel` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40252-40275` `agent_stage_runs_the_per_unit_lifecycle_not_the_fan_out_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40278-40298` `standalone_review_stage_still_takes_the_fan_out_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40301-40414` `run_gates_derives_and_injects_the_review_worktrees_store_fence` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40417-40421` `review_lens_ids` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40426-40459` `standalone_review_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40463-40472` `assert_review_worktree_kept` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40476-40483` `assert_genuine_crash_halts` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40486-40507` `a_parked_lens_keeps_the_standalone_review_stages_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40510-40565` `a_parked_lens_keeps_the_review_worktree_even_beside_a_lower_indexed_sibling_crash` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40568-40623` `a_parked_lens_keeps_the_review_worktree_beside_a_sibling_degenerate_halt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40626-40666` `the_swap_to_front_prioritizes_a_genuine_error_at_a_non_zero_chunk_index` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40669-40713` `a_budget_refused_lens_beside_a_genuinely_crashing_sibling_in_one_chunk` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40716-40746` `a_budget_refused_standalone_review_spawn_keeps_its_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40749-40779` `partition_separates_overlapping_blast_radii` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40782-40804` `blast_radius_conflicts_flags_units_that_split_one_file_set` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40807-40820` `blast_radius_conflicts_is_empty_for_a_disjoint_partition` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40823-40875` `partitioned_wave_still_integrates_every_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40887-40911` `run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40915-41028` `lens_finding_reaches_later_tiers_through_the_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:41031-41041` `ok_gate_digest` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:41050-41083` `every_review_tier_prompt_carries_the_gate_evidence_of_the_commit_it_judges` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:41086-41099` `digested_verdict` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:41105-41116` `a_digest_address_is_the_commit_its_gate_ran_on` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:41124-41176` `the_gate_evidence_block_lists_only_the_units_gates_on_the_commit_under_review` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:41184-41222` `a_reviewer_builds_into_the_units_review_cache_never_the_units_own` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:41227-41253` `review_tier_prompts` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:41256-41286` `review_agents_emit_findings_via_the_review_protocol` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:41295-41320` `every_reviewer_prompt_carries_only_the_contract_and_its_system_prompt_the_method` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:41328-41459` `every_task_prompt_text_keeps_its_shape_and_leaves_its_cadence_and_method_to_a_built_in` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:41462-41500` `unparseable_adjudicator_output_blocks_integration` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:41503-41568` `failing_gate_evidence_threaded_into_retry_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:41573-41601` `silent_gate_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:41604-41611` `any_verdict` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:41614-41669` `a_flaky_gate_rerun_is_a_pass_with_warning_that_never_demotes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:41672-41712` `a_product_gate_failure_is_not_rerun_and_demotes_as_before` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:41715-41797` `an_authored_infra_rule_with_limit_zero_at_an_inline_gate_holds_the_ratchet` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:41800-41902` `an_inline_gate_red_across_every_rerun_holds_for_infra_and_demotes_for_flaky` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:41905-41988` `an_infra_gate_red_reruns_the_stage_at_the_same_attempt_without_charging` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:41991-42008` `unit_evidence_is_populated_after_a_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42011-42079` `adjudicator_rejection_reasoning_threaded_into_retry_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42082-42141` `fan_out_lens_does_not_integrate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42144-42185` `review_only_stage_records_no_artifact_truthfully` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42188-42238` `two_erroring_stages_both_leave_a_record` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42241-42297` `single_wide_wave_overruns_budget_and_is_stopped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42300-42314` `validate_acyclic_detects_a_cycle` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42380-42394` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42397-42402` `faulting_at` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42404-42410` `with_side_effect` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42411-42413` `calls` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42414-42416` `targets` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42417-42419` `gate_scratches` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42420-42422` `store_fences` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42423-42425` `build_cache_guards` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42426-42428` `build_cache_dirs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42431-42488` `run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42492-42497` `probe_fault` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42509-42548` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42559-42564` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42565-42567` `calls` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42570-42590` `run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42594-42603` `gate_verdict_event` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42605-42609` `verdict_passed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42615-42645` `identical_tree_attempts` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42648-42678` `a_matching_input_digest_answers_a_gate_as_a_logged_cache_hit_citing_the_prior_green` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42687-42779` `a_fast_forward_landing_whose_commit_was_gated_replays_its_post_merge_gates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42786-42799` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42807-42841` `an_identical_tree_reached_through_different_history_is_not_replayed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42844-42890` `a_content_change_misses_the_cache_and_re_runs_the_gate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42893-42914` `a_red_verdict_is_never_cache_answered_and_the_gate_re_runs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42923-42935` `ground` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42947-42962` `ground` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42963-42965` `blast_radius` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42966-42968` `index_stamp` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42972-42987` `blast_radius_audits` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42990-42999` `review_tier_evidence` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:43004-43027` `tiered_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:43029-43036` `tiered_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:43045-43091` `assert_beyond_cap_high_risk_routes_full` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:43101-43117` `a_structural_grounder_records_the_audit_and_routes_full_on_a_beyond_cap_high_risk_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:43124-43168` `the_empty_radius_fail_safe_still_records_the_audit_and_routes_full` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:43176-43214` `a_non_structural_grounder_records_no_blast_radius_audit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:43226-43291` `confidence_tier_fixture` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:43301-43341` `confidence_tier_radius_splits_the_one_edge_set_into_extracted_precise_and_structural_safe` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:43347-43363` `files_reachable_maps_a_member_to_its_held_file_and_a_placeholder_to_none` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:43369-43379` `files_reachable_reaches_no_file_through_a_module_contains` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:43386-43398` `the_code_neighborhood_lists_each_member_under_its_held_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:43407-43488` `grounded_blast_radius_tier_filters_the_subgraph_and_keeps_the_grounder_radius` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:43496-43534` `grounded_blast_radius_with_a_graph_grounds_only_on_the_code_a_criterion_names` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:43543-43545` `ground` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:43546-43552` `blast_radius` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:43558-43562` `co_scheduled_share` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:43582-43835` `measure_blast_radius_retention_through_grounded_blast_radius` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:43858-43956` `under_symbols_the_audit_emits_and_records_the_prompt_seed_as_precise` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:43971-43981` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:43984-43989` `meeting` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:43992-44004` `peak` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:44007-44014` `window` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:44017-44045` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:44049-44066` `stub_radii` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:44073-44128` `probe_one_wave` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:44135-44177` `run_wave_co_schedules_units_naming_a_hub_when_their_radii_are_disjoint` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:44183-44211` `run_wave_runs_units_side_by_side_only_when_their_radii_are_disjoint` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:44219-44236` `run_wave_keeps_units_apart_on_their_recorded_radii` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:44242-44258` `run_wave_keeps_a_ready_unit_apart_from_a_parked_unit_of_an_earlier_wave` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:44264-44293` `run_wave_gives_a_freed_slot_to_the_next_ready_unit_before_the_wave_ends` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:44302-44313` `speculation_over_a_structural_grounder_records_the_audit_and_routes_full` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:44316-44420` `rule6_conflict_detection_grounds_on_the_safe_superset_so_a_grep_only_shared_reference_conflicts` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:44423-44449` `glob_matches_supports_star_doublestar_and_literals` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:44452-44476` `gate_intersects_radius_runs_unscoped_and_intersecting_gates_only` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:44479-44632` `blast_radius_narrows_the_inner_loop_and_the_integrate_step_runs_the_full_library` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:44635-44767` `a_gate_skipped_inline_but_red_at_the_exhaustive_integrate_door_blocks_the_merge` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:44770-44799` `verdict_compensates_names_a_prior_unit_independent_of_approval` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:44812-44856` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:44860-45026` `a_contradiction_compensates_reverts_and_re_enters_the_integrated_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45030-45048` `with_integrations_ctx` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45051-45081` `commits_to_compensate_reverts_every_sha_a_multi_commit_landing_recorded` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45084-45135` `commits_to_compensate_dedupes_a_repeated_sha_and_skips_an_already_compensated_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45138-45168` `commits_to_compensate_excludes_the_review_only_marker_even_alongside_a_real_sha` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45171-45231` `pending_compensations_from_log_re_derives_only_undrained_marks` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45234-45265` `a_durable_compensation_queued_mark_is_fold_neutral_on_the_target` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45273-45312` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45320-45395` `assert_compensated_unit_re_gates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45424-45471` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45511-45537` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45541-45676` `a_resume_re_drives_a_durably_queued_but_undrained_compensation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45679-45851` `integrate_re_gates_the_merged_tree_and_a_merge_break_blocks_the_second_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45897-46000` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46004-46165` `the_post_merge_re_gate_runs_in_its_own_scratch_worktree_never_the_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46179-46226` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46230-46333` `a_passing_post_merge_re_gate_reclaims_the_gate_scratch_root_its_fallback_names` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46349-46378` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46389-46500` `assert_a_replayed_post_merge_pass_reclaims_the_gate_scratch_root` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46508-46546` `assert_postmerge_err_reaps_its_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46549-46581` `postmerge_run_gates_err_still_reaps_the_throwaway_worktree_and_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46584-46620` `postmerge_worktree_create_err_still_reaps_the_just_created_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46623-46743` `conflict_regenerate_pending_from_log_re_derives_the_union_keyed_by_unit_and_attempt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46751-46775` `conflict_fixture` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46785-46796` `run_conflicting_units` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46801-46827` `conflicting_units_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46837-46872` `assert_both_conflicting_units_land` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46875-46895` `integrate_conflict_re_parks_the_implementer_with_no_attempt_charged_and_both_units_land` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46898-46915` `a_conflict_resolver_whose_record_is_refused_re_runs_once_told_why_and_both_units_land` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46918-47006` `a_resolver_round_ending_on_an_unmarked_error_is_spent_like_one_resolving_nothing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:47009-47125` `consecutive_store_faults_at_the_resolvers_spawn_record_an_infra_retry_mark_charge_nothing_and_return_no_blocked_integration` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:47128-47198` `an_unreviewed_unit_halted_mid_merge_resumes_at_the_resolver_and_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:47201-47225` `integrate_conflict_confined_to_a_regenerable_path_resolves_with_no_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:47228-47327` `integrate_conflict_exhausted_after_the_bound_charges_a_real_attempt_with_the_unresolved_evidence` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:47330-47479` `integrate_conflict_records_regenerate_pending_before_the_accept_incoming_mutation_that_can_fail` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:47365-47409` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:47485-47519` `deferred_gate_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:47522-47524` `names_the_deferred_gate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:47527-47572` `a_deferred_gate_runs_once_at_the_phase_boundary_not_inline` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:47575-47596` `a_failing_deferred_gate_is_surfaced_and_the_run_is_not_done` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:47607-47630` `run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:47634-47667` `a_default_infra_fault_at_a_deferred_gate_does_not_demote` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:47673-47691` `verify_only_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:47701-47754` `assert_probe_fault_halts_the_step` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:47807-47827` `replayed_twice` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:47830-47835` `count_carrying` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:47838-47840` `runs_of` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:47843-47882` `a_replayed_step_re_runs_no_recorded_gate_and_appends_no_duplicate_events` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:47885-47928` `a_re_step_replays_a_recorded_deferred_gate_without_re_running_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:47931-48088` `a_parked_step_never_records_a_partial_tree_deferred_verdict` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:48091-48180` `a_manual_review_paused_unit_defers_the_whole_tree_gate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:48183-48319` `an_escalated_dep_still_runs_the_whole_tree_deferred_gate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:48322-48445` `a_recorded_failing_deferred_verdict_re_surfaces_its_failure_on_replay` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:48448-48548` `a_replayed_fan_out_review_reject_appends_no_duplicate_unitfailed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:48551-48610` `a_fan_out_review_stage_whose_gates_fail_after_approval_records_a_gate_cause` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:48614-48616` `git_head` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:48627-48653` `run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:48657-48720` `gate_measures_the_committed_artifact_not_the_dirty_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:48723-48841` `conductor_spawns_the_sdet_author_at_the_build_seam_so_its_tests_land_in_the_committed_tree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:48849-48890` `sdet_turns_around_a_rejected_round` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:48899-48948` `a_doc_only_round_states_only_the_fact_to_the_sdet_author` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:48952-48964` `round_start` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:48971-48987` `round_delta_base_is_the_latest_review_round_before_the_attempt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:48994-49032` `a_round_delta_takes_no_base_outside_the_unit_branch_history` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:49035-49119` `the_sdet_author_spawn_respects_the_budget_breaker_at_its_own_spawn_site` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:49122-49218` `a_parked_sdet_author_spawn_holds_the_unit_instead_of_integrating_without_its_periphery_tests` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:49221-49325` `speculation_sdet_author_periphery_lands_in_the_committed_tree_the_gates_judge` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:49328-49419` `a_parked_sdet_author_in_a_speculation_candidate_holds_the_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:49426-49431` `sdet_lane_driver` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:49437-49445` `run_in_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:49449-49467` `sdet_lane_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:49471-49473` `run_sdet_lane` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:49476-49484` `unit_gate_verdict_keys` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:49495-49552` `assert_sdet_seam_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:49635-49710` `the_build_seam_spawns_the_declared_test_author_under_the_sdet_author_token` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:49713-49805` `an_escalated_unit_does_not_integrate_its_code` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:49813-49823` `critique_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:49850-49861` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:49867-49873` `rejecting` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:49876-49929` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:49933-49997` `a_blast_radius_overlap_alone_does_not_reject` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:50010-50046` `the_plan_critique_gates_adversary_and_adjudicator_spawns_are_stamped_correctly` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:50049-50081` `a_rule_7_or_8_defect_rejects_then_releases_on_the_revision` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:50084-50130` `a_clean_decomposition_approves_and_releases_the_fan_out` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:50133-50203` `the_gate_critiques_only_not_yet_run_units_and_approves` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:50207-50240` `critique_over` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:50244-50246` `one_unit_critique` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:50252-50272` `the_dag_critique_prompt_is_only_its_framing_and_its_verdict_contract` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:50278-50292` `the_plan_protocol_and_the_dag_critique_carry_one_unit_size_cap` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:50299-50316` `the_planner_protocol_carries_no_planning_policy` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:50319-50344` `the_critique_gate_never_enters_a_wave_even_when_ready` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:50353-50387` `fan_out_needs_template_fixture` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:50390-50472` `a_downstream_stage_needing_the_fan_out_template_stays_unready_until_every_member_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:50475-50579` `a_real_split_pair_must_both_integrate_not_just_the_btreemap_key_first_sibling` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:50582-50664` `a_resolved_plan_critique_gate_does_not_re_run_on_resume` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:50668-50678` `widget_split` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:50683-50685` `critique_step` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:50688-50707` `critique_step_under` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:50710-50776` `a_resumed_step_holds_the_fan_out_while_the_plan_critique_gate_is_escalated` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:50786-50807` `reject_then_approve_critique` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:50810-50847` `a_plan_critique_round_re_entered_in_a_later_process_opens_with_the_logged_reject` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:50856-50861` `refused_decision` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:50864-50869` `refusing_at` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:50873-50883` `decision_seats` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:50902-50913` `sdet_author_refused` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:50917-50932` `re_planner_refused` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:50935-50940` `decision_d_x` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:50947-50966` `adjudicator_refused` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:50973-51044` `assert_a_refused_record_reruns_its_spawn_once` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:51070-51147` `an_implementer_s_refused_record_is_charged_to_the_attempt_that_emitted_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:51153-51184` `a_speculation_candidate_whose_sdet_author_is_refused_again_loses_to_the_next` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:51194-51196` `stop_halt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:51199-51205` `stop_lesson` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:51211-51231` `critique_rounds` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:51235-51245` `critique_round_spawns` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:51249-51253` `launched_on_stop_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:51257-51262` `rejecting_twice_for_spec_ambiguity` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:51267-51269` `stopped_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:51272-51277` `stopped_run_under` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:51284-51310` `the_stop_is_decided_before_an_exhausted_remediation_bound_escalates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:51318-51358` `a_resumed_stopped_gate_re_plans_its_next_spec_ambiguity_reject_as_a_first_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:51361-51417` `a_spec_ambiguity_reject_after_a_re_plan_that_did_not_clear_it_stops_the_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:51422-51437` `assert_re_plans_after_every_reject` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:51440-51445` `a_first_spec_ambiguity_reject_re_plans_as_today` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:51448-51456` `a_spec_ambiguity_reject_after_a_decomposition_conflict_re_plans_again` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:51459-51474` `a_stop_with_no_upheld_finding_and_no_spec_says_so` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:51477-51502` `a_stopped_gate_halts_the_step_before_the_coverage_check` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:51509-51529` `a_later_step_finds_the_stopped_gate_terminal_and_reports_its_halt_appending_nothing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:51532-51561` `a_step_re_entering_a_crashed_stop_completes_it_without_spawning` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:51564-51602` `a_stop_whose_lesson_append_fails_records_nothing_after_it_and_the_next_step_completes_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:51606-51618` `gate_failed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:51621-51623` `re_plan_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:51626-51630` `re_plan_requested` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:51634-51636` `stops_at` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:51640-51646` `two_spec_ambiguity_rejects` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:51649-51689` `the_stop_holds_on_two_spec_ambiguity_rejects_around_a_recorded_re_plan` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:51692-51776` `the_stop_does_not_hold_short_of_its_whole_shape` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:51779-51786` `the_stop_holds_at_any_attempt_its_two_rejects_precede` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:51789-51804` `the_spec_defect_halt_names_the_spec_and_the_upheld_findings` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:51809-51827` `the_budget_halt_takes_precedence_over_the_spec_defect_stop` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:51832-51839` `the_dag_critique_verdict_paragraph_carries_the_spec_defect_cause_contract` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:51842-51893` `an_approved_gate_releases_planner_proposed_units_not_only_baselines` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:51896-51961` `the_re_plan_directive_instructs_reusing_the_existing_unit_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:51974-51979` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:51982-52003` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:52007-52037` `a_resumed_step_holds_the_fan_out_while_the_plan_critique_gate_is_mid_review` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:52040-52097` `a_parked_plan_critique_gate_keeps_its_review_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:52100-52144` `a_budget_refused_plan_critique_gate_keeps_its_review_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:52157-52176` `the_conductors_one_event_authority_reports_a_write_the_store_lost` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
- `conductor::tests::run_sink` (50 functions)
  - `crates/rigger-conductor/src/conductor.rs:22841-22843` `sized` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22846-22848` `sized_hash` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22851-22853` `failing_hash` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22856-22870` `sink_deps` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22874-22887` `recorded` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22890-22892` `entries` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22895-22906` `planted_body` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22910-22917` `walked_entries` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22921-22924` `emit` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22928-22935` `reindexed` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22938-22949` `live_names` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22971-22980` `unless` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22984-22990` `apply` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22991-22999` `apply_generation` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23000-23007` `current_generation` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23008-23010` `rebuild_owed` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23011-23013` `subgraph` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23014-23016` `resolve` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23028-23094` `a_whole_tree_walk_records_one_ledger_entry_per_file_from_its_bytes_and_no_derived_event` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23099-23123` `an_entry_recorded_by_a_run_carries_the_runs_id` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23129-23150` `an_integration_reindex_naming_an_out_of_line_test_module_records_its_flag_set` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23156-23183` `a_file_whose_generation_the_log_holds_and_the_graph_does_not_is_recorded_again` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23190-23237` `a_deleted_gc_file_and_one_outside_the_walks_scope_each_record_an_entry_with_no_blob` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23245-23292` `a_lagging_lowering_records_the_bytes_generation_and_nothing_once_both_sides_hold_it` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23298-23324` `a_tree_that_is_not_a_git_repository_records_the_blob_id_git_hash_object_gives` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23329-23352` `a_batch_whose_key_names_no_identity_fails_the_emit_and_records_nothing` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23356-23376` `a_failing_hash_function_fails_the_emit_and_records_nothing` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23381-23402` `a_read_failing_for_a_reason_other_than_absence_fails_the_emit_and_records_nothing` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23407-23454` `a_failing_read_of_the_log_side_or_of_the_graph_side_fails_the_emit_and_records_nothing` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23459-23478` `an_emit_wired_to_no_graph_fails_and_records_nothing` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23484-23499` `a_gd_file_emptied_after_the_walk_records_nothing_and_the_emit_succeeds` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23506-23553` `a_graph_that_owes_its_rebuild_records_one_entry_per_generation` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23558-23594` `an_entry_whose_ledger_fold_fails_is_recorded_and_the_lost_fold_is_said` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23600-23656` `a_revert_a_b_a_records_three_entries_and_leaves_as_facts` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23662-23693` `a_batch_whose_append_is_refused_at_first_sight_appends_whole_on_the_next_sight` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23700-23754` `a_generation_whose_append_is_refused_leaves_the_process_on_the_recorded_generation` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23763-23831` `a_first_sight_lookup_racing_a_newer_generation_leaves_the_process_on_the_stored_generation` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23838-23871` `a_run_ingest_whose_group_lookup_is_unanswered_fails_appends_nothing_and_walks_again` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23876-23897` `an_integration_reindex_whose_group_lookup_is_unanswered_fails_and_appends_nothing` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23907-23946` `a_current_batch_handed_twice_makes_one_group_lookup` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23955-24005` `an_identity_is_looked_up_once_and_its_later_batches_answer_from_the_entry_recorded` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24012-24059` `a_batch_whose_group_lookup_failed_handed_again_makes_a_second_lookup` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24064-24083` `handed_twice_over_an_empty_store` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24086-24095` `the_same_answer_twice_from_one_lookup` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24106-24172` `an_identity_the_log_holds_no_recording_of_is_still_looked_up_once_when_its_batch_records_nothing` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24186-24229` `a_pre_ledger_identity_takes_its_entrys_generation_so_its_second_walk_asks_no_lookup` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24244-24312` `a_long_lived_run_restores_an_identity_a_rebuild_left_behind_at_its_next_reindex` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24330-24389` `a_revert_after_another_process_recorded_is_recorded` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24392-24394` `a_revert_another_process_staled_is_recorded_over_a_graph_that_owes_its_rebuild` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24397-24401` `a_revert_another_process_staled_is_recorded_over_a_graph_rebuilt_at_that_generation` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
- `conductor::tests::support` (14 functions)
  - `crates/rigger-conductor/src/conductor.rs:15274-15276` `occurrences` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15279-15281` `snapshot` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15286-15304` `stub_deps` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15308-15316` `run_logged` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15319-15323` `stage_map` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15327-15341` `stage_shape` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15345-15360` `grant_resume` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15375-15382` `apply` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15383-15390` `apply_batch` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15391-15393` `subgraph` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15394-15396` `resolve` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15397-15399` `rebuild_owed` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15400-15408` `apply_generation` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15409-15414` `current_generation` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
- `conductor::throwaway` (1 function)
  - `crates/rigger-conductor/src/conductor.rs:14938-14944` `dir_and_branch` - method inside `impl Throwaway`; grouped with its other `Throwaway` methods.
- `dash::dash_marker` (4 functions)
  - `crates/rigger-dash/src/dash.rs:619-621` `serialize` - method inside `impl DashMarker`; grouped with its other `DashMarker` methods.
  - `crates/rigger-dash/src/dash.rs:627-632` `parse` - method inside `impl DashMarker`; grouped with its other `DashMarker` methods.
  - `crates/rigger-dash/src/dash.rs:636-638` `read` - method inside `impl DashMarker`; grouped with its other `DashMarker` methods.
  - `crates/rigger-dash/src/dash.rs:643-645` `write` - method inside `impl DashMarker`; grouped with its other `DashMarker` methods.
- `dash::probe_miss` (1 function)
  - `crates/rigger-dash/src/dash.rs:353-359` `answer` - method inside `impl ProbeMiss`; grouped with its other `ProbeMiss` methods.
- `dash::registry` (6 functions)
  - `crates/rigger-dash/src/dash.rs:436-446` `dash_serving_pid_on` - name contains "pid" (instance registry); grouped under `dash::registry`.
  - `crates/rigger-dash/src/dash.rs:613-615` `displayable_pid` - name contains "pid" (instance registry); grouped under `dash::registry`.
  - `crates/rigger-dash/src/dash.rs:687-709` `pid_holding_port` - name contains "pid" (instance registry); grouped under `dash::registry`.
  - `crates/rigger-dash/src/dash.rs:847-849` `pid_if_port_matches` - name contains "pid" (instance registry); grouped under `dash::registry`.
  - `crates/rigger-dash/src/dash.rs:1095-1116` `instance_views` - name contains "instance" (instance registry); grouped under `dash::registry`.
  - `crates/rigger-dash/src/dash.rs:1121-1123` `instances_json` - name contains "instance" (instance registry); grouped under `dash::registry`.
- `dash::render` (18 functions)
  - `crates/rigger-dash/src/dash.rs:452-482` `header_line_value` - name contains "header" (HTML rendering); grouped under `dash::render`.
  - `crates/rigger-dash/src/dash.rs:488-511` `head_has_header_line` - name contains "header" (HTML rendering); grouped under `dash::render`.
  - `crates/rigger-dash/src/dash.rs:732-747` `format_held_port` - name contains "format" (HTML rendering); grouped under `dash::render`.
  - `crates/rigger-dash/src/dash.rs:763-765` `describe_held_port` - name contains "describe" (HTML rendering); grouped under `dash::render`.
  - `crates/rigger-dash/src/dash.rs:812-814` `describe_held_port_if_confirmed` - name contains "describe" (HTML rendering); grouped under `dash::render`.
  - `crates/rigger-dash/src/dash.rs:1614-1646` `build_graph_view` - name contains "graph" (HTML rendering); grouped under `dash::render`.
  - `crates/rigger-dash/src/dash.rs:1915-1929` `rationale_batch` - name contains "rationale" (HTML rendering); grouped under `dash::render`.
  - `crates/rigger-dash/src/dash.rs:1945-1977` `graph_json` - name contains "graph" (HTML rendering); grouped under `dash::render`.
  - `crates/rigger-dash/src/dash.rs:2015-2027` `call_node_view` - name contains "node" (HTML rendering); grouped under `dash::render`.
  - `crates/rigger-dash/src/dash.rs:2044-2056` `ref_node_view` - name contains "node" (HTML rendering); grouped under `dash::render`.
  - `crates/rigger-dash/src/dash.rs:2343-2424` `unit_node` - name contains "node" (HTML rendering); grouped under `dash::render`.
  - `crates/rigger-dash/src/dash.rs:2428-2487` `role_stage` - name contains "role" (HTML rendering); grouped under `dash::render`.
  - `crates/rigger-dash/src/dash.rs:2536-2544` `stage_of_role` - name contains "role" (HTML rendering); grouped under `dash::render`.
  - `crates/rigger-dash/src/dash.rs:2554-2580` `role_and_agent` - name contains "role" (HTML rendering); grouped under `dash::render`.
  - `crates/rigger-dash/src/dash.rs:2661-2669` `graph_seeds` - name contains "graph" (HTML rendering); grouped under `dash::render`.
  - `crates/rigger-dash/src/dash.rs:2867-2873` `field_str` - name contains "field" (HTML rendering); grouped under `dash::render`.
  - `crates/rigger-dash/src/dash.rs:2877-2888` `field_str_array` - name contains "field" (HTML rendering); grouped under `dash::render`.
  - `crates/rigger-dash/src/dash.rs:3052-3065` `escape_for_script` - name contains "escape" (HTML rendering); grouped under `dash::render`.
- `dash::reproject` (5 functions)
  - `crates/rigger-dash/src/dash.rs:1682-1695` `reproject` - name contains "reproject" (graph reprojection); grouped under `dash::reproject`.
  - `crates/rigger-dash/src/dash.rs:1703-1722` `cap_clusters` - name contains "cluster" (graph reprojection); grouped under `dash::reproject`.
  - `crates/rigger-dash/src/dash.rs:1740-1751` `reprojection_lens_key` - name contains "reproject" (graph reprojection); grouped under `dash::reproject`.
  - `crates/rigger-dash/src/dash.rs:1757-1800` `reproject_derived` - name contains "reproject" (graph reprojection); grouped under `dash::reproject`.
  - `crates/rigger-dash/src/dash.rs:1822-1908` `reproject_files` - name contains "reproject" (graph reprojection); grouped under `dash::reproject`.
- `dash::response` (6 functions)
  - `crates/rigger-dash/src/dash.rs:3087-3093` `new` - method inside `impl Response`; grouped with its other `Response` methods.
  - `crates/rigger-dash/src/dash.rs:3096-3098` `rendered` - method inside `impl Response`; grouped with its other `Response` methods.
  - `crates/rigger-dash/src/dash.rs:3099-3105` `text` - method inside `impl Response`; grouped with its other `Response` methods.
  - `crates/rigger-dash/src/dash.rs:3111-3113` `binary` - method inside `impl Response`; grouped with its other `Response` methods.
  - `crates/rigger-dash/src/dash.rs:3115-3125` `reason` - method inside `impl Response`; grouped with its other `Response` methods.
  - `crates/rigger-dash/src/dash.rs:3135-3151` `write_to` - method inside `impl Response`; grouped with its other `Response` methods.
- `dash::server` (8 functions)
  - `crates/rigger-dash/src/dash.rs:541-553` `bind_singleton` - name contains "bind" (HTTP serving); grouped under `dash::server`.
  - `crates/rigger-dash/src/dash.rs:657-676` `tcp_listen_inode_for_port` - name contains "tcp" (HTTP serving); grouped under `dash::server`.
  - `crates/rigger-dash/src/dash.rs:717-719` `process_state` - name contains "process_" (HTTP serving); grouped under `dash::server`.
  - `crates/rigger-dash/src/dash.rs:2172-2209` `calls_route` - name contains "route" (HTTP serving); grouped under `dash::server`.
  - `crates/rigger-dash/src/dash.rs:3160-3341` `route` - name contains "route" (HTTP serving); grouped under `dash::server`.
  - `crates/rigger-dash/src/dash.rs:3417-3459` `serve_on` - name contains "serve" (HTTP serving); grouped under `dash::server`.
  - `crates/rigger-dash/src/dash.rs:3475-3654` `handle_conn` - name contains "handle" (HTTP serving); grouped under `dash::server`.
  - `crates/rigger-dash/src/dash.rs:3743-3867` `serve_console_stream` - name contains "serve" (HTTP serving); grouped under `dash::server`.
- `dash::tests` (112 functions)
  - `crates/rigger-dash/src/dash.rs:3902-3915` `get_static` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:3919-3925` `assert_console_page_carries` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:3927-3939` `seeded_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:3941-3951` `local_instance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:3959-4010` `instance_views_project_a_sorted_credential_free_landing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4017-4025` `instance_view_age_floors_at_zero_for_a_future_heartbeat` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4032-4064` `api_instances_route_renders_the_landing_list` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4071-4076` `console_core_wasm_route_serves_the_embedded_artifact_as_application_wasm` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4081-4093` `console_core_wasm_artifact_is_under_the_three_megabyte_budget` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4105-4133` `embedded_artifact_matches_a_fresh_independent_nested_build` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4145-4195` `console_route_serves_the_shell_page_with_mock_regions_and_both_theme_token_blocks` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4201-4210` `console_route_never_references_an_external_url` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4217-4253` `console_fonts_route_serves_each_embedded_font_and_its_license_text` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4269-4281` `console_page_wires_the_theme_toggles_persistence_round_trip` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4284-4298` `root_serves_the_embedded_page_with_the_placeholder_resolved` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4309-4359` `gates_status_never_fabricates_passed_for_an_off_linear_unverdicted_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4366-4402` `free_port_from_returns_the_start_port_when_free_and_the_next_free_one_when_it_is_taken` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4409-4458` `bind_singleton_binds_the_exact_port_and_never_searches` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4469-4542` `bind_singleton_short_circuits_on_an_already_serving_rigger_dash` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4550-4560` `serve_reply` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4564-4582` `serve_dribble` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4587-4597` `timed_dash_probe` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4600-4602` `assert_pid_probe` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4609-4614` `dash_serving_on_is_false_for_a_non_dash_listener` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4625-4642` `dash_serving_on_is_bounded_against_a_byte_dribbling_holder` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4665-4687` `dash_serving_on_recognizes_the_header_fast_even_if_the_holder_never_finishes_the_block` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4779-4782` `dash_serving_pid_on_is_none_when_nothing_answers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4790-4819` `bind_singleton_cold_race_loser_resolves_across_the_accept_window` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4830-4865` `the_page_layout_cannot_scroll_the_body_horizontally` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4876-4913` `the_landing_view_lists_instances_and_threads_the_attach_selector` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4918-4927` `css_rule` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4938-4983` `the_dashboard_fits_one_screen_with_internal_scroll` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4997-5031` `cells_fit_or_wrap_and_wide_cells_scroll_in_their_own_container` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5046-5048` `the_decision_history_renders_each_decision_as_a_native_details_with_preview_and_full_body` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5051-5079` `state_endpoint_projects_the_seeded_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5086-5129` `console_event_filter_admits_only_the_named_run_lifecycle_types` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5135-5143` `console_event_wire_matches_console_cores_wire_event_shape` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5151-5157` `console_event_wire_carries_recorded_at` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5163-5170` `console_event_wire_with_a_malformed_body_degrades_to_null_data` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5176-5183` `console_progress_wire_with_a_malformed_body_degrades_to_empty_id_and_activity` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5186-5270` `console_snapshot_endpoint_filters_events_carries_progress_liveness_and_definitions` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5273-5335` `state_carries_the_live_agent_activity` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5338-5405` `state_counts_grep_fallbacks_and_carries_them_in_the_review_outcomes_data` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5414-5601` `run_tree_projects_the_spine_with_collapse_expand_and_driver_lines` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5420-5425` `done` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5608-5667` `every_step_token_reads_as_its_kind` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5672-5718` `the_run_tree_groups_every_step_under_its_kind` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5723-5752` `review_verdicts_come_straight_from_the_metrics_classification` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5755-5765` `events_endpoint_is_since_exclusive` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5771-5800` `no_mutating_endpoint_exists` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5803-5832` `export_inlines_the_snapshot_as_a_static_page` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5840-5887` `export_neutralizes_a_script_breakout_in_the_inlined_state` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5890-5916` `decision_view_strikes_through_superseded_entries` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5927-5992` `cluster_key_folds_paths_by_directory_and_dev_loop_nodes_by_kind` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:6004-6092` `clustered_overview_under_files_lens_admits_only_code_entities_keyed_by_their_own_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:6111-6354` `cluster_detail_drills_a_cluster_to_its_members_and_caps_a_big_one_by_degree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:6363-6408` `cluster_detail_under_files_lens_is_unconditionally_empty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:6425-6618` `code_lens_buckets_code_entities_by_community_excludes_other_kinds_and_reports_underived_grain` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:6632-6867` `concepts_lens_buckets_members_by_concept_excludes_membershipless_nodes_and_reports_underived_grain` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:6873-6890` `tiered_chain_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:6893-6957` `the_graph_route_returns_a_tier_tagged_seeded_neighborhood_as_json` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:6960-7011` `the_graph_route_percent_decodes_the_seed_so_select_to_seed_reaches_ids_with_special_chars` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:7014-7045` `the_graph_route_degrades_gracefully_for_an_unknown_seed_and_an_empty_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:7048-7068` `the_graph_route_is_read_only_a_non_get_is_405` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:7077-7179` `the_graph_route_dispatches_cluster_overview_and_seed_by_parameter` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:7193-7247` `the_overview_route_degrades_gracefully_on_an_empty_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:7250-7304` `neighborhood_bounds_by_depth_follows_both_directions_and_skips_invalidated_edges` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:7307-7340` `neighborhood_flags_god_nodes_by_degree_within_the_returned_neighborhood` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:7343-7415` `path_is_the_shortest_route_between_two_selected_nodes_over_currently_valid_edges` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:7418-7485` `the_graph_route_flags_god_nodes_and_returns_the_query_path_between_two_selected_nodes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:7492-7529` `provenance_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:7532-7579` `explain_returns_a_nodes_incident_edges_as_source_and_tier_tagged_provenance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:7582-7647` `the_graph_route_carries_the_seed_nodes_explain_provenance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:7650-7681` `graph_seeds_enumerate_decisions_findings_and_their_files_never_units` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:7686-7702` `graph_seeds_seed_the_file_a_record_citing_its_lines_names` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:7705-7747` `a_units_seed_lands_on_the_neighborhood_of_its_decisions_and_files` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:7750-7854` `the_run_tree_click_to_seed_route_lands_a_unit_on_a_real_neighborhood` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:7857-7910` `unit_seeds_scope_content_to_the_owning_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:7913-7949` `repoint_seed_passes_a_known_node_and_re_points_a_unit_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:7955-7984` `a_unit_view_carries_the_spec_defect_reason_of_a_gate_stopped_on_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:7990-8009` `the_spec_defect_block_prints_a_stopped_gate_s_reason_before_the_uncovered_text` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8012-8031` `build_state_on_an_empty_run_is_empty_not_a_panic` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8042-8131` `release_ready_is_surfaced_on_the_dash_only_for_a_done_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8149-8204` `release_ready_pr_command_newline_renders_as_a_real_line_break_not_a_collapsed_run_on` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8207-8214` `request_line_parsing_extracts_method_and_target` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8217-8224` `query_param_reads_since` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8232-8310` `endpoints_serve_over_a_real_socket_against_a_seeded_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8316-8367` `a_post_over_a_real_socket_is_refused_without_touching_the_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8378-8490` `the_graph_provider_is_consulted_only_on_graph_requests_not_the_state_poll` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8495-8505` `dash_marker_round_trips_through_its_on_disk_record` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8508-8527` `dash_marker_parse_rejects_a_malformed_record` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8530-8548` `dash_marker_reads_none_for_an_absent_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8553-8565` `format_held_port_always_names_the_address_even_with_no_holder` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8568-8585` `format_held_port_names_the_pid_and_state_for_a_running_holder` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8588-8596` `format_held_port_names_the_pid_alone_when_its_state_is_not_discoverable` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8599-8614` `format_held_port_gives_the_stopped_listener_diagnosis_naming_resume_or_kill` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8617-8629` `pid_holding_port_finds_the_pid_of_a_listener_bound_in_this_process` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8632-8648` `pid_holding_port_is_none_for_a_port_nothing_is_listening_on` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8656-8669` `assert_names_this_process_as_holder` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8698-8714` `describe_held_port_if_confirmed_is_none_when_nothing_holds_the_port` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8717-8734` `dash_start_needed_is_true_when_none_serving_and_false_when_one_serves` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8739-8745` `dash_answer_on_tells_a_silent_holder_from_an_empty_port` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8753-8775` `an_interrupted_probe_read_is_read_again_inside_the_window` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8784-8816` `a_probe_error_reads_as_the_peer_a_silent_holder_or_the_probe_failing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8821-8829` `a_failed_probe_answers_neither_serving_nor_gone` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8832-8906` `dash_status_trusts_a_url_with_no_marker_and_catches_a_marker_that_lies` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8918-8934` `dash_status_never_names_the_unattributed_pid_sentinel_as_a_dead_process` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8937-8958` `url_port_parses_the_recorded_shape_and_rejects_anything_else` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8974-9014` `dash_status_probes_the_urls_own_port_when_the_marker_names_a_different_dash` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9017-9055` `should_reap_singleton_reaps_only_when_no_registered_instance_is_live` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9058-9091` `should_reap_singleton_never_reaps_while_a_fresh_agent_liveness_signal_is_present` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9103-9174` `the_page_carries_the_directed_call_layered_render` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
- `dash::tests::call_target_c4` (1 function)
  - `crates/rigger-dash/src/dash.rs:10530-10559` `reproject_files_beside_a_member` - defined inside `call_target_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
- `dash::tests::calls_route_c4` (10 functions)
  - `crates/rigger-dash/src/dash.rs:9192-9202` `cnode` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9204-9206` `layer_of` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9207-9209` `ids` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9215-9270` `calls_view_down_signs_callees_positive_and_carries_frontier_and_back` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9275-9314` `calls_view_up_negates_callers_and_carries_the_referenced_sidecar` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9320-9368` `calls_view_both_centers_the_seed_with_callees_right_and_callers_left` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9374-9411` `a_plain_neighborhood_omits_every_additive_call_field` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9418-9472` `calls_route_runs_the_traversal_for_view_calls_and_declines_otherwise` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9478-9506` `calls_route_clamps_depth_and_defaults_the_tier_floor` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9511-9529` `calls_route_walks_both_directions_for_dir_both` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
- `dash::tests::file_half_c13` (1 function)
  - `crates/rigger-dash/src/dash.rs:10620-10636` `reproject_files_folds_an_extensionless_files_entity_under_its_file` - defined inside `file_half_c13`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
- `dash::tests::leaf_keys_c1` (1 function)
  - `crates/rigger-dash/src/dash.rs:10591-10605` `reproject_files_resolves_a_placeholder_onto_the_nested_definition_of_its_leaf` - defined inside `leaf_keys_c1`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
- `dash::tests::metadata_card_c2` (12 functions)
  - `crates/rigger-dash/src/dash.rs:10123-10173` `card_graph` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:10176-10219` `card_of_a_code_entity_carries_file_line_degree_community_concepts_and_memory_counts` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:10225-10233` `card_of_a_membership_less_entity_has_no_community_and_no_line` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:10241-10268` `card_of_a_proven_code_entity_carries_proven_by_and_proof_evidence` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:10276-10281` `card_of_an_unproven_code_entity_has_proven_by_zero_and_no_evidence` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:10285-10294` `assert_card_reports_no_proof` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:10321-10358` `card_of_a_file_lists_its_contained_entities_as_top_entities` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:10363-10383` `card_of_a_concept_lists_its_realizing_members_as_top_evidence` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:10392-10417` `card_of_a_concept_carries_each_top_evidence_members_own_kind` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:10424-10438` `card_of_a_file_carries_each_top_entity_members_own_kind` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:10443-10446` `card_of_an_unknown_id_is_none` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:10452-10494` `the_card_route_serves_a_known_subjects_card_and_null_for_an_unknown_one` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
- `dash::tests::rationale_overlay_c3` (12 functions)
  - `crates/rigger-dash/src/dash.rs:9551-9561` `finding_node` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9577-9610` `rationale_graph` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9613-9615` `leaf_fields` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9621-9635` `node_rationale_returns_attached_leaves_ordered_by_kind_then_id` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9641-9664` `a_finding_leaf_carries_content_only_never_the_by_or_unit_machinery` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9670-9683` `a_handbook_rule_and_a_supersedes_edge_are_not_rationale` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9688-9696` `an_invalidated_edge_is_not_live_rationale` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9701-9711` `a_node_without_rationale_returns_none` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9717-9742` `the_batch_covers_the_visible_set_and_keeps_only_nodes_with_rationale` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9747-9764` `the_batch_is_deterministic_across_request_order_and_dedups` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9770-9809` `the_explain_route_returns_the_batch_in_one_request` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9815-9839` `an_absent_explain_leaves_the_graph_route_unchanged` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
- `dash::tests::reprojection_views` (2 functions)
  - `crates/rigger-dash/src/dash.rs:10503-10508` `cluster_counts` - defined inside `reprojection_views`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:10511-10516` `unresolved_frontiers` - defined inside `reprojection_views`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
- `dash::tests::subject_view_c5` (5 functions)
  - `crates/rigger-dash/src/dash.rs:9860-9903` `memory_rail_lists_decisions_findings_and_concepts_excluding_lessons` - defined inside `subject_view_c5`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9908-9915` `memory_rail_is_empty_for_a_node_with_no_governing_memory` - defined inside `subject_view_c5`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9936-10013` `memory_rail_concepts_are_live_from_node_realizes_edges_to_a_concept_target_deduped_by_id` - defined inside `subject_view_c5`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:10021-10071` `the_seeded_route_carries_memory_without_adding_a_single_node_to_the_neighborhood` - defined inside `subject_view_c5`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:10077-10097` `a_cluster_drill_carries_no_memory_field` - defined inside `subject_view_c5`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
- `main::commands` (1 function)
  - `src/cli/mod.rs:2753-2913` `cmd_replay` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
- `main::dash_glue` (1 function)
  - `src/cli/mod.rs:3156-3160` `recorded_dash_url` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
- `main::docs_overlay` (1 function)
  - `src/cli/mod.rs:4943-4953` `apply` - method inside `impl DocsOverlay`; grouped with its other `DocsOverlay` methods.
- `main::liveness` (4 functions)
  - `src/cli/mod.rs:2073-2083` `live_branches_for_sweep` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/cli/mod.rs:3183-3200` `liveness_ages_for_wave` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/cli/mod.rs:3766-3773` `live_slugs` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/cli/mod.rs:3976-3994` `worktree_belongs_to_live` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
- `main::provenance` (12 functions)
  - `src/cli/mod.rs:161-163` `workflow_path` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/cli/mod.rs:296-309` `spec_positional` - name contains "spec_" (workflow/spec loading); grouped under `main::provenance`.
  - `src/cli/mod.rs:2238-2280` `refuse_when_base_lacks_spec_paths` - name contains "spec_" (workflow/spec loading); grouped under `main::provenance`.
  - `src/cli/mod.rs:3611-3616` `spec_lint_warning_lines` - name contains "spec_" (workflow/spec loading); grouped under `main::provenance`.
  - `src/cli/mod.rs:3623-3625` `workflow_provenance_path` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/cli/mod.rs:3630-3638` `installed_workflow_provenance` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/cli/mod.rs:4860-4870` `install_workflow` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/cli/mod.rs:4961-4970` `read_docs_overlay` - name contains "docs_" (docs overlay); grouped under `main::provenance`.
  - `src/cli/mod.rs:5426-5474` `docs_context` - name contains "docs_" (docs overlay); grouped under `main::provenance`.
  - `src/cli/mod.rs:5533-5539` `spec_lint_next_step` - name contains "spec_" (workflow/spec loading); grouped under `main::provenance`.
  - `src/cli/mod.rs:5570-5572` `git_repo` - name contains "git_" (git/version provenance); grouped under `main::provenance`.
  - `src/cli/mod.rs:5580-5588` `git_repo_at` - name contains "git_" (git/version provenance); grouped under `main::provenance`.
- `main::render` (5 functions)
  - `src/cli/mod.rs:2411-2453` `format_stats` - name contains "format_" (human-readable output); grouped under `main::render`.
  - `src/cli/mod.rs:2606-2702` `format_canary_stats` - name contains "format_" (human-readable output); grouped under `main::render`.
  - `src/cli/mod.rs:3107-3117` `format_stats_diff` - name contains "format_" (human-readable output); grouped under `main::render`.
  - `src/cli/mod.rs:4204-4234` `format_residue` - name contains "format_" (human-readable output); grouped under `main::render`.
  - `src/cli/mod.rs:5484-5494` `rendered_docs` - name contains "render" (human-readable output); grouped under `main::render`.
- `main::replay_runner` (1 function)
  - `src/cli/mod.rs:3129-3149` `run` - method inside `impl Runner for ReplayRunner`; grouped with its other `ReplayRunner` methods.
- `main::residue_report` (1 function)
  - `src/cli/mod.rs:3664-3669` `is_empty` - method inside `impl ResidueReport`; grouped with its other `ResidueReport` methods.
- `main::setup` (13 functions)
  - `src/cli/mod.rs:1717-1720` `scratch_defaults` - name contains "scratch" (project setup); grouped under `main::setup`.
  - `src/cli/mod.rs:2295-2313` `locate_shim` - name contains "shim" (project setup); grouped under `main::setup`.
  - `src/cli/mod.rs:4326-4346` `scratch_totals` - name contains "scratch" (project setup); grouped under `main::setup`.
  - `src/cli/mod.rs:4522-4557` `classify_agent_scratch` - name contains "scratch" (project setup); grouped under `main::setup`.
  - `src/cli/mod.rs:4829-4846` `install_file_if_changed` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/cli/mod.rs:4884-4886` `skill_source_rel` - name contains "skill" (project setup); grouped under `main::setup`.
  - `src/cli/mod.rs:4892-4894` `project_skill_path` - name contains "skill" (project setup); grouped under `main::setup`.
  - `src/cli/mod.rs:4903-4908` `skill_install_path` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/cli/mod.rs:4974-4976` `setup_read_error` - name contains "setup" (project setup); grouped under `main::setup`.
  - `src/cli/mod.rs:4990-5006` `project_skill_entries` - name contains "skill" (project setup); grouped under `main::setup`.
  - `src/cli/mod.rs:5011-5028` `read_project_skill` - name contains "skill" (project setup); grouped under `main::setup`.
  - `src/cli/mod.rs:5049-5082` `install_skills` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/cli/mod.rs:5409-5416` `write_shim_files` - name contains "shim" (project setup); grouped under `main::setup`.
- `main::store` (24 functions)
  - `src/cli/mod.rs:272-283` `eventstore_flag` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:686-704` `store_conn_file` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:765-778` `store_backend_kind` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:828-894` `store_selection_at` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:902-908` `store_selection` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:936-938` `registry_store_identity` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:945-952` `store_identity` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:1232-1303` `migrate_project_identity` - name contains "migrate" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:1313-1321` `migrate_local_identity` - name contains "migrate" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:1339-1370` `migrate_identity_at` - name contains "migrate" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:1481-1483` `find_store_dir_from` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:1488-1494` `store_if_any` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:1731-1734` `server_store_location` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:1765-1830` `require_store_dir` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:1848-1855` `over_the_run_store_at` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:1888-1890` `store_file` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:3802-3893` `reclaim_orphan_scratch` - name contains "reclaim_" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:4149-4183` `reclaim_shared_build_cache` - name contains "reclaim_" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:4363-4415` `scratch_footprint` - name contains "footprint" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:4425-4448` `store_and_backup_bytes` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:4572-4655` `footprint_report` - name contains "footprint" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:4664-4684` `footprint_advisories` - name contains "footprint" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:4704-4735` `reclaim_dead_footprint` - name contains "reclaim_" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:4739-4765` `footprint_reclaim_lines` - name contains "reclaim_" (store hygiene); grouped under `main::store`.
- `main::store_location` (5 functions)
  - `src/cli/mod.rs:1641-1647` `graph` - method inside `impl StoreLocation`; grouped with its other `StoreLocation` methods.
  - `src/cli/mod.rs:1651-1653` `file` - method inside `impl StoreLocation`; grouped with its other `StoreLocation` methods.
  - `src/cli/mod.rs:1657-1659` `run_stream` - method inside `impl StoreLocation`; grouped with its other `StoreLocation` methods.
  - `src/cli/mod.rs:1665-1672` `identity` - method inside `impl StoreLocation`; grouped with its other `StoreLocation` methods.
  - `src/cli/mod.rs:1685-1691` `repo_root` - method inside `impl StoreLocation`; grouped with its other `StoreLocation` methods.
- `main::store_selection` (4 functions)
  - `src/cli/mod.rs:405-407` `is_sqlite` - method inside `impl StoreSelection`; grouped with its other `StoreSelection` methods.
  - `src/cli/mod.rs:412-414` `log_absent` - method inside `impl StoreSelection`; grouped with its other `StoreSelection` methods.
  - `src/cli/mod.rs:419-425` `log_file` - method inside `impl StoreSelection`; grouped with its other `StoreSelection` methods.
  - `src/cli/mod.rs:431-436` `handed_env` - method inside `impl StoreSelection`; grouped with its other `StoreSelection` methods.
- `main::support` (18 functions)
  - `src/cli/mod.rs:315-368` `parse_run_args` - name contains "parse_" (argument/input parsing); grouped under `main::support`.
  - `src/cli/mod.rs:379-387` `resolve_run_base` - name contains "resolve" (path/id resolution helper); grouped under `main::support`.
  - `src/cli/mod.rs:711-713` `conn_file_is_group_or_other_readable` - name contains "is_" (predicate helper); grouped under `main::support`.
  - `src/cli/mod.rs:721-743` `warn_if_conn_file_is_exposed` - name contains "is_" (predicate helper); grouped under `main::support`.
  - `src/cli/mod.rs:785-808` `resolve_conn` - name contains "resolve" (path/id resolution helper); grouped under `main::support`.
  - `src/cli/mod.rs:916-926` `resolve_store` - name contains "resolve" (path/id resolution helper); grouped under `main::support`.
  - `src/cli/mod.rs:1107-1116` `read_project_id` - name contains "read_" (generic read helper); grouped under `main::support`.
  - `src/cli/mod.rs:1585-1608` `resolve_main_worktree_or_refuse` - name contains "resolve" (path/id resolution helper); grouped under `main::support`.
  - `src/cli/mod.rs:2139-2141` `read_run_progress` - name contains "read_" (generic read helper); grouped under `main::support`.
  - `src/cli/mod.rs:2149-2158` `read_project_stream` - name contains "read_" (generic read helper); grouped under `main::support`.
  - `src/cli/mod.rs:2712-2720` `read_model_drift` - name contains "read_" (generic read helper); grouped under `main::support`.
  - `src/cli/mod.rs:2990-3022` `parse_replay_args` - name contains "parse_" (argument/input parsing); grouped under `main::support`.
  - `src/cli/mod.rs:3233-3263` `parse_watch_args` - name contains "parse_" (argument/input parsing); grouped under `main::support`.
  - `src/cli/mod.rs:3998-4000` `is_uuid8` - name contains "is_" (predicate helper); grouped under `main::support`.
  - `src/cli/mod.rs:4007-4038` `find_shadow_stores` - name contains "find_" (lookup helper); grouped under `main::support`.
  - `src/cli/mod.rs:4101-4106` `is_build_cache_tombstone` - name contains "is_" (predicate helper); grouped under `main::support`.
  - `src/cli/mod.rs:5312-5317` `find_bytes` - name contains "find_" (lookup helper); grouped under `main::support`.
  - `src/cli/mod.rs:5500-5511` `write_docs` - name contains "write_" (generic write helper); grouped under `main::support`.
- `main::tests` (211 functions)
  - `src/cli/mod.rs:5399-5404` `compose_precommit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:5654-5676` `the_hash_bound_to_no_tree_fails_naming_why_and_writes_into_no_repository` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:5681-5713` `the_rebuild_owed_note_speaks_only_for_a_graph_db_that_owes_it_and_creates_none` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:5720-5766` `the_passed_over_note_counts_by_cause_the_rows_graph_db_holds` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:5770-5784` `test_request` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:5793-5800` `spec_lint_next_step_names_rigger_validate_and_the_given_spec_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:5817-5839` `spec_lint_warning_lines_formats_every_advisory_with_the_spec_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:5844-5847` `spec_lint_warning_lines_is_empty_on_a_clean_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:5858-5896` `compose_precommit_fresh_install_carries_the_managed_block` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:5909-5959` `precommit_block_refuses_on_drift_instead_of_staging` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:5973-6026` `precommit_block_finds_the_relocated_unit_target_with_the_binary_s_own_path_encoding` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6029-6069` `shipped_workflow_driver_tells_a_worker_its_units_build_location` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6072-6173` `precommit_block_resolves_a_tree_built_binary_before_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6180-6192` `compose_precommit_is_idempotent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6203-6232` `compose_precommit_chains_without_clobbering_an_existing_hook` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6242-6256` `compose_precommit_prepends_before_a_terminal_exit_existing_hook` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6263-6286` `compose_precommit_refreshes_a_stale_block_in_place` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6296-6314` `version_line_carries_the_derived_version_and_a_non_empty_build_provenance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6321-6372` `docs_context_reads_every_fact_from_code` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6379-6420` `docs_render_surfaces_known_code_facts_verbatim` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6426-6445` `commands_registry_is_well_formed_and_covers_dispatch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6451-6472` `usage_text_lists_the_docs_verb_on_its_own_line` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6480-6510` `write_docs_writes_every_registry_skill_plus_the_handbook` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6520-6558` `install_and_docs_each_cover_exactly_the_registry_no_more_no_less` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6562-6585` `assert_skills_reference_only_real_subcommands` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6649-6662` `committed_registry_docs_are_in_sync_with_a_fresh_render` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6673-6742` `status_and_dashboard_render_the_same_current_blocker_lines` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6745-6758` `install_workflow_records_the_build_provenance_beside_the_workflow` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6764-6766` `slugs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6771-6783` `init_committed_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6801-6817` `resolve_main_worktree_or_refuse_returns_exactly_git_rev_parse_show_toplevel` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6820-6844` `refuse_when_base_lacks_spec_paths_refuses_on_total_absence_and_names_a_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6848-6866` `assert_base_path_check_proceeds` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6906-6979` `refuse_when_base_unreachable_fails_loudly_only_when_no_reachable_base` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6982-6989` `human_size_formats_bytes_through_gib` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6992-6998` `is_uuid8_accepts_exactly_eight_hex_digits` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7001-7046` `worktree_belongs_to_live_matches_both_naming_shapes_without_prefix_false_match` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7049-7096` `current_run_units_scopes_to_the_current_run_and_splits_live_from_dead` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7099-7136` `current_run_units_spares_a_terminal_units_branch_whose_latest_spawn_is_still_in_flight` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7139-7165` `current_run_units_still_retires_a_terminal_unit_once_its_latest_spawn_answers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7168-7215` `current_run_units_splits_live_spawns_from_answered_ones_scoped_to_the_current_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7227-7253` `live_branches_for_sweep_fails_closed_on_an_unreadable_stream_but_folds_a_readable_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7256-7305` `find_shadow_stores_finds_nested_events_db_and_prunes_build_caches` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7308-7319` `dir_size_bytes_sums_files_recursively` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7324-7340` `reclaim_shared_build_cache_deletes_a_populated_cache_and_reports_its_bytes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7343-7361` `reclaim_shared_build_cache_is_idempotent_zero_report_when_absent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7364-7392` `reclaim_shared_build_cache_refuses_rather_than_waits_when_a_build_holds_the_guard` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7395-7418` `reclaim_shared_build_cache_releases_the_guard_promptly_after_a_successful_reclaim` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7421-7505` `scan_residue_reports_dead_worktrees_caches_shadows_and_branches` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7508-7527` `scan_residue_is_empty_when_everything_is_live_and_no_shadow_stores` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7530-7547` `format_residue_renders_a_sized_warning_block` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7552-7571` `store_and_backup_bytes_splits_live_store_files_from_dot_bak_backups` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7574-7578` `store_and_backup_bytes_is_zero_on_a_missing_directory` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7581-7651` `scratch_footprint_totals_every_entry_and_dead_only_the_non_live_share` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7654-7715` `footprint_report_measures_every_category_on_a_seeded_fixture_tree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7721-7737` `seed_agent_scratch_leaf` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7741-7755` `spawn_scoped_footprint` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7758-7760` `category` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7766-7787` `registered_scratch_roots_with` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7790-7817` `footprint_report_flags_registered_scratch_roots_dead_share_and_spares_a_live_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7830-7838` `footprint_report_keys_agent_scratch_liveness_by_run_id_and_leaf_not_leaf_name_alone` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7841-7846` `looks_like_run_container_true_when_every_direct_child_is_a_directory` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7849-7857` `looks_like_run_container_false_when_a_direct_child_is_a_bare_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7860-7868` `looks_like_run_container_is_true_on_an_empty_or_missing_dir` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7871-7888` `classify_agent_scratch_separates_a_bare_top_level_file_from_a_well_formed_container` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7891-7947` `footprint_report_reports_a_top_level_adhoc_agent_scratch_dir_as_its_own_category_never_folded_into_the_dead_run_bucket` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7950-7962` `footprint_advisories_flags_a_category_whose_dead_share_reaches_the_threshold` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7968-8005` `footprint_advisories_name_reset_build_cache_for_every_class_it_reclaims` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8009-8018` `assert_hinted_category_is_silent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8026-8044` `footprint_advisories_flags_a_category_exactly_at_the_threshold_boundary` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8047-8058` `footprint_advisories_never_flags_a_category_with_no_reclaim_command` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8065-8087` `owning_repo_root_prefers_the_stores_own_root_over_the_git_toplevel` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8092-8242` `reclaim_orphan_scratch_removes_non_live_owned_scratch_and_spares_live_and_shared_areas` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8245-8300` `reclaim_orphan_scratch_spares_a_non_live_worktree_that_is_still_dirty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8303-8342` `reclaim_orphan_scratch_spares_only_a_declared_dirty_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8345-8375` `reclaim_orphan_scratch_reaps_a_stray_build_cache_tombstone_unconditionally` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8380-8385` `find_store_dir_from_returns_the_dir_that_holds_the_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8388-8399` `find_store_dir_from_walks_up_from_a_subdirectory` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8402-8405` `plant_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8408-8420` `add_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8423-8458` `find_store_dir_from_never_escapes_the_repo_into_a_parent_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8461-8489` `reap_then_remove_dir_reaps_processes_rooted_inside_then_removes_the_dir` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8492-8503` `find_store_dir_from_refuses_the_worktree_shape_with_no_events_db` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8506-8534` `find_store_dir_from_walks_past_a_storeless_rigger_to_the_real_store_above` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8537-8566` `find_store_dir_from_resolves_the_owning_repo_even_when_the_worktree_lives_outside_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8569-8598` `find_store_dir_from_never_climbs_a_relocated_worktrees_own_unrelated_ancestors_into_a_foreign_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8601-8626` `walk_stores_from_prefers_the_outermost_store_over_a_nearer_shadow` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8629-8646` `walk_stores_from_reports_no_shadow_for_a_single_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8655-8724` `require_store_dir_pins_to_the_fence_env_and_never_reaches_the_live_store_above_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8658-8661` `drop` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8728-8743` `require_store_dir_fence_is_off_by_default` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8748-8754` `result_advisories_flags_an_orphan_id_with_no_spawn_request` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8757-8782` `result_advisories_orphan_wording_is_plain_on_record_and_conditional_under_if_absent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8785-8795` `result_advisories_is_silent_for_a_parked_unanswered_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8798-8809` `result_advisories_flags_a_supersede_with_the_prior_result_position` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8812-8825` `result_advisories_suppresses_the_supersede_note_when_not_superseding` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8828-8840` `result_advisories_flags_both_orphan_and_supersede` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8848-8867` `result_refuses_a_second_result_without_supersede` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8872-8883` `a_real_result_still_replaces_a_recorded_liveness_fault` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8897-8914` `shipped_workflows_carry_a_non_zero_spawn_budget` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8917-8946` `usage_text_gives_the_jobs_flag_its_own_description_line` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8949-8966` `usage_text_names_the_build_cache_reset_mode` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8972-9004` `an_unclaimed_argument_is_an_unknown_flag_the_spec_or_a_refused_second_positional` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9007-9017` `parse_run_args_defaults_to_cli_and_an_unset_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9020-9031` `parse_run_args_reads_fresh_alongside_a_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9034-9044` `parse_run_args_reads_rebase_definition` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9047-9062` `parse_run_args_reads_driver_eventstore_conn_and_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9065-9073` `assert_every_arg_list_refused` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9092-9118` `parse_run_args_accepts_base_alongside_a_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9126-9148` `resolve_run_base_precedence_flag_then_env_then_default` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9154-9220` `definition_hash_is_stable_and_content_sensitive` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9227-9299` `definition_hash_folds_each_present_helper_copy_between_the_agents_and_the_instructions` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9308-9335` `kurrentdb_is_always_available_and_needs_a_conn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9347-9378` `store_selection_preserves_a_credentialed_tls_conn_verbatim` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9389-9566` `store_selection_precedence_flag_env_secret_file_config_then_default` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9569-9571` `project_identity_is_never_empty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9574-9603` `decide_migration_covers_every_case` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9606-9652` `migrate_project_identity_renames_legacy_history_and_records_a_decision` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9655-9694` `migrate_project_identity_refuses_when_both_namespaces_hold_history` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9699-9722` `pre_mint_deployment` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9727-9734` `migrates_one_stream_folding_into` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9737-9777` `migrate_project_identity_rekeys_graph_rows_so_pre_mint_history_is_not_orphaned` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9780-9860` `migrate_project_identity_rekeys_the_graph_before_the_irreversible_stream_rename` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9863-9933` `migrate_project_identity_recovers_from_a_crash_between_the_rekey_and_the_rename` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9939-9971` `setup_provisions_the_shim_runtime_files` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9978-10008` `the_shim_peers_refresh_keeps_its_noun_and_leaves_its_cadence_to_the_communication_built_in` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10014-10039` `each_script_progress_note_names_the_verb_and_its_arguments_only` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10048-10107` `setup_installs_refreshes_and_is_a_noop_on_the_native_rigger_workflow` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10112-10121` `outcome_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10125-10130` `registry_entry` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10140-10216` `setup_installs_the_using_rigger_skill_distinct_from_the_workflow` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10234-10324` `planning_a_spec_installs_and_renders_through_the_registry_with_no_planning_specific_code` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10334-10370` `setup_skill_install_applies_the_project_overlay` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10376-10394` `assert_installed_planning_skill_carries` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10427-10457` `docs_overlay_overrides_only_declared_fields` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10462-10476` `docs_overlay_malformed_is_a_loud_error` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10493-10588` `assert_skill_shadow` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10645-10679` `a_shadow_rewrites_a_hand_edit_and_once_deleted_gives_way_to_the_shipped_render` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10684-10701` `assert_install_skills_refuses` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10736-10750` `a_group_or_other_readable_secret_file_mode_is_flagged_owner_only_is_not` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10756-10775` `meta_object_body` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10784-10798` `meta_description` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10805-10813` `strip_line_comments` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10827-11051` `workflow_is_a_thin_courier_driver_with_per_unit_phase_labels` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11060-11071` `the_step_schema_admits_the_attention_array` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11087-11143` `phase_of_maps_wave_items_to_the_meta_phase_by_role_and_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11150-11172` `the_driver_reads_every_step_token_through_its_bare_role` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11185-11223` `meta_matches_reality_drops_integrate_and_the_unit_stage_construction` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11243-11333` `the_driver_relays_each_attention_entry_as_a_narrator_log_line` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11351-11399` `workflow_step_courier_prompt_is_foreground_and_honest` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11409-11418` `step_courier_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11436-11503` `workflow_step_courier_waits_on_an_auto_backgrounded_step` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11518-11526` `workflow_driver_guards_a_null_step_before_dereferencing_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11539-11596` `workflow_meta_description_is_a_user_facing_tagline_free_of_plumbing_terms` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11602-11627` `workflow_locates_the_provisioned_shim_or_tells_you_to_run_setup` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11638-11681` `format_stats_prints_all_four_metrics` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11687-11703` `format_stats_handles_zeroed_metrics_without_nan` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11706-11708` `canary_scorecard` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11711-11727` `assert_findings_raised_render` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11742-11747` `assert_empty_scorecard_omits` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11766-11797` `format_canary_stats_renders_na_for_a_tier_with_unattributed_correct_rejects` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11804-11823` `format_canary_stats_renders_the_real_zero_when_attribution_was_fully_measured` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11827-11842` `assert_control_line` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11867-11896` `format_canary_stats_reports_the_model_pinning_header_when_present` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11917-11959` `format_stats_surfaces_parallelism_retention_and_warns_below_the_floor` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11966-12006` `format_stats_surfaces_spawn_timing_and_reports_unpaired_separately` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:12012-12018` `format_stats_spawn_timing_reports_no_spawns_when_none_recorded` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:12025-12052` `format_stats_spawn_timing_no_spawns_line_requires_both_empty_and_zero_unpaired` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:12059-12074` `format_stats_spawn_timing_all_unpaired_is_not_reported_as_no_spawns` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:12082-12109` `parallelism_retention_line_is_single_sourced_and_warns_below_the_floor` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:12114-12136` `parallelism_retention_line_counts_the_units_whose_radius_is_empty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:12140-12155` `sdet_review` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:12158-12160` `stats_text` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:12167-12180` `stats_discloses_when_no_verdict_was_recorded_on_this_driver` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:12191-12221` `stats_discloses_unfed_numerator_when_verdict_recorded_but_findings_unattributed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:12229-12266` `stats_discloses_cause_split_remainder_when_fewer_causes_than_rejects` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:12269-12314` `baseline_run_slice_selects_a_run_by_id_including_a_middle_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:12317-12348` `format_stats_diff_flags_only_the_changed_rows` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:12355-12362` `defaults_max_parallel_units_is_unbounded_when_the_key_is_absent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:12365-12382` `parse_replay_args_requires_a_run_and_a_rev_in_either_order` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:12390-12424` `a_second_concurrent_rigger_step_refuses_and_the_lock_frees_on_release` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:12430-12433` `no_runs_message_points_at_rigger_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:12439-12446` `seed_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:12451-12464` `assert_absent_db_reads_none` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:12482-12498` `stats_lines_existing_db_with_empty_run_stream_returns_none` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:12506-12535` `stats_lines_does_not_read_another_projects_namespaced_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:12542-12573` `stats_lines_existing_run_renders_metric_lines` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:12596-12665` `stats_lines_pairs_recorded_spawn_request_and_result_into_spawn_timing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:12683-12692` `db_with_one_result` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:12698-12707` `result_of_at_unrecorded_spawn_reads_as_unreported` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:12715-12739` `result_of_at_reads_a_self_reported_result_so_it_is_not_clobbered` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:12745-12764` `result_of_at_is_namespace_scoped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:12777-12802` `detach_process_group_places_the_child_in_its_own_process_group` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:12811-12832` `a_child_spawned_without_detachment_inherits_the_parent_process_group` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:12840-12849` `watch_test_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:12857-12928` `watch_reads_a_marker_on_wall_clock_so_a_dead_supervisor_behind_an_open_hold_is_seen` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:12938-12960` `status_progress_and_the_dash_snapshot_read_the_run_from_its_boundary` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:12967-12992` `status_and_the_dash_read_the_runs_progress_from_its_own_stream` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:13000-13050` `check_runs_parse_into_a_ci_probe_by_conclusion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:13056-13065` `the_run_tip_ci_needs_a_run_branch_and_reads_unknown_when_gh_cannot_read_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:13071-13083` `a_watch_poll_reads_the_run_from_its_boundary_and_nothing_else` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:13106-13139` `store_location_repo_root_resolves_the_owning_root_not_the_process_cwd_so_a_real_marker_is_found` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:13153-13184` `scratch_defaults_reads_the_owning_roots_config_with_no_agents_fleet_present` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:13187-13213` `watch_once_on_a_clean_store_reports_no_anomalies` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:13216-13220` `parse_watch_args_defaults_to_streaming_with_the_default_interval` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:13227-13249` `parse_watch_args_accepts_once_and_interval_together_in_either_order` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:13265-13398` `watch_once_on_the_seeded_store_reports_one_line_per_anomaly` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:13405-13424` `the_watchdog_command_signal_set_covers_every_signal_the_watch_skill_names` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:13432-13461` `watch_once_reports_dash_not_serving_when_the_marker_names_a_dead_holder` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:13468-13499` `watch_once_reports_no_anomaly_when_the_dash_marker_names_a_real_serving_holder` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:13505-13526` `tree_root_is_the_top_level_of_the_directory_holding_the_store_else_that_directory` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
- `main::tree_root` (2 functions)
  - `src/cli/mod.rs:5614-5619` `of` - method inside `impl TreeRoot`; grouped with its other `TreeRoot` methods.
  - `src/cli/mod.rs:5622-5624` `get` - method inside `impl TreeRoot`; grouped with its other `TreeRoot` methods.

### Unassigned (176 functions)

- `crates/rigger-conductor/src/conductor.rs:356-358` `failed_key` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:363-365` `verified_key` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:369-371` `spec_defect_lesson_key` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:375-377` `spec_defect_key` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:383-385` `spec_defect_escalated_key` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:453-455` `adoption_provenance_key` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:528-536` `input_digest` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:542-544` `digest_address` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:831-833` `implementer_retry` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:839-856` `conflict_resolution_prompt` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:1111-1113` `conflict_regenerate_key` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:1118-1120` `cached` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:1125-1130` `clear_attempt` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:1139-1155` `integrate_row` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:1217-1222` `evidence_pair` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:1444-1449` `checkpoint_attempt` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:1506-1508` `carries_marker` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:1513-1515` `marked` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:1554-1566` `rerun_once_on_refusal` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:1752-1756` `output_tail` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:1862-1864` `probe_fault_halt` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:1882-1884` `without_halt_markers` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:1915-1922` `infra_crash` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:1939-1941` `infra_mark_key` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:1946-1953` `infra_halt` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:2134-2146` `replay_trajectory` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:2150-2716` `run` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:2724-2810` `settle_run_state` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:2890-2980` `compute_attention` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:3002-3027` `spec_defect_stop` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:3045-3056` `spec_defect_halt` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:3381-3389` `event_with_meta` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:13090-13095` `part_coverage` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:13148-13157` `verified_evidence` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:13339-13356` `notes_block` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:13428-13440` `required_lines` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:13445-13447` `audit_id` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:13507-13517` `task_block` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:13621-13669` `render_capped_section` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:13716-13726` `recency_by_own_edge` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:13872-13875` `plain_node_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:13975-13982` `member_files` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:14018-14029` `confidence_tier_radius` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:14043-14086` `files_reachable` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:14346-14348` `current_run_spec` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:14578-14584` `on_run_branch` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:14607-14624` `branch_owner` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:14669-14675` `quarantine_branch_name` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:14756-14762` `round_delta_base` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:14778-14801` `later_attempt_block` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:14879-14907` `quarantined_branch` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:14952-14957` `same_path` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:14964-14979` `sanitize_for_path` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:14984-14986` `integrates` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:15041-15064` `admissible` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:15092-15098` `implement_slice` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:15182-15219` `validate_acyclic` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:178-187` `console_font_response` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:203-213` `free_port_from` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:271-290` `probe_dash_head` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:297-322` `read_probe_head` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:329-338` `classify_probe_error` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:384-391` `dash_answer_on` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:405-407` `dash_serving_on` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:516-518` `head_block_ended` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:785-788` `held_port_holder` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:830-834` `url_port` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:925-972` `dash_status` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:984-992` `dash_start_needed` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:1046-1057` `should_reap_singleton` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:1494-1608` `build_state` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2001-2007` `parse_call_dir` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2031-2039` `call_edge_view` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2071-2157` `calls_view` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2231-2338` `build_run_tree` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2492-2511` `driver_stage` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2515-2523` `rollup` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2601-2608` `gates_status` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2619-2622` `advanced_past_gates` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2627-2636` `unit_live_status` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2641-2649` `spec_of` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2676-2702` `event_seed_ids` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2714-2737` `unit_seeds` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2752-2765` `repoint_seed` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2810-2812` `is_console_event` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2824-2833` `console_event_wire` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2841-2849` `console_progress_wire` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2853-2864` `event_view` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2890-2892` `now_unix` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2901-2920` `state_json` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2926-2934` `events_json` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2941-2998` `console_snapshot_json` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:3002-3004` `live_page` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:3010-3012` `console_page` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:3019-3039` `render_export` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:3350-3368` `percent_decode` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:3371-3377` `query_param` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:3381-3386` `parse_request_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:3671-3678` `env_duration_ms` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:3693-3704` `write_sse` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:3713-3719` `write_retained_window_gone` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:224-226` `version_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:286-290` `conn_flag` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:440-442` `stderr_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:455-463` `hash_blob_in` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:474-476` `open_sqlite_store` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:493-524` `open_graph_for_write` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:535-554` `reextract_entry` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:561-567` `entry_blobs` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:575-586` `open_graph` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:592-597` `standing_graph` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:603-611` `graph_rebuild_owed_note` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:618-626` `graph_passed_over_note` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:631-641` `passed_over_by_cause` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:650-658` `open_graph_to_read` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:665-667` `env_conn` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:752-754` `config_rigger_dir` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:757-759` `cwd` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:988-1024` `refresh_registry_entry` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:1036-1038` `project_identity` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:1063-1074` `project_identity_at` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:1081-1083` `legacy_identity_at` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:1088-1101` `legacy_identity_from` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:1121-1127` `canonical_definition_text` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:1142-1182` `definition_hash` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:1185-1190` `push_definition_section` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:1210-1224` `decide_migration` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:1372-1377` `db_path` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:1419-1476` `walk_stores_from` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:1501-1506` `project_instruction_layer` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:1515-1522` `instruction_layer_root` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:1525-1535` `worktree_prefix` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:1539-1541` `project_root` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:1547-1566` `main_repo_root` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:1613-1615` `run_stream_of` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:1835-1844` `over_the_run_store` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:1860-1867` `on_the_run` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:1876-1884` `owning_graph_to_read` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:1909-1941` `result_advisories` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:1949-1959` `ended_refusal` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:1965-1975` `result_refusal` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:2042-2054` `acquire_step_lock` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:2094-2106` `reap_then_remove_worktree` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:2121-2134` `result_of_at` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:2163-2175` `with_project_store` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:2199-2213` `refuse_when_base_unreachable` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:2333-2356` `stats_lines` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:2378-2398` `parallelism_retention_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:2461-2482` `append_spawn_timing` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:2487-2489` `fmt_duration` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:2491-2601` `append_review_quality` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:2921-2931` `started_units` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:2952-2985` `candidate_reaches_gate` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:3029-3043` `baseline_run_slice` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:3046-3050` `run_started_id` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:3058-3101` `materialize_config_at_rev` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:3213-3218` `marker_root` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:3282-3290` `watch_poll` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:3297-3323` `run_tip_ci` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:3329-3341` `ci_probe_from_gh_read` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:3348-3384` `ci_probe_from_check_runs` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:3389-3587` `watch_poll_over` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:3591-3600` `json_type_name` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:3716-3762` `current_run_units` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:3901-3964` `scan_residue` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:4043-4062` `dir_size_bytes` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:4084-4094` `build_cache_tombstone_path` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:4186-4199` `human_size` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:4463-4476` `dead_spawn_leaves` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:4486-4493` `looks_like_run_container` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:4773-4785` `owning_repo_root` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:4788-4790` `rigger_path` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:5145-5306` `precommit_block` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:5337-5389` `compose_precommit_bytes` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:5553-5568` `select_grounder` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:5595-5601` `tree_root` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.

## 2. Duplication Catalog

219 clusters (1563 total sites) across `src/` and `tests/`, found by `tests/simplification_audit.rs`'s deterministic normalized-token-shingle Jaccard pass (8-token shingles, threshold 0.72) plus five mandatory mechanical sweeps. Strict definition (spec 85 Goal): any logic present in more than one place anywhere in the codebase is a violation, with no "small enough to duplicate" exemption.

### Mandatory sweeps

- **Command::new call sites**: 94 site(s) - `dup-64397d781d9d`
- **/proc-path string literals**: 60 site(s) - `dup-9d67deeaad9d`
- **sqlite Connection::open call sites**: 66 site(s) - `dup-880060c29e31`
- **.rigger-path string literals**: 643 site(s) - `dup-47e4d1c77944`
- **error-shaping helper functions**: 23 site(s) - `dup-7310d4338924`

### Clusters (56 exact, 137 near, 26 semantic)

#### `dup-49d4d9f335fc` (near, 2 sites)

Proposed home: `blast_radius_eval::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/blast_radius_eval.rs:181-194` `width_threshold_is_the_tier_width_nearest_rank_percentile`
- `crates/rigger-conductor/src/blast_radius_eval.rs:197-205` `full_fraction_spans_all_light_to_collapse`

#### `dup-76144682ea08` (near, 44 sites)

Proposed home: `a new shared module (sites span 26 files: crates/rigger-conductor/src/blast_radius_eval.rs, crates/rigger-conductor/src/conductor.rs, crates/rigger-domain/src/contextgraph.rs, crates/rigger-domain/src/instructions.rs, crates/rigger-domain/src/review.rs, crates/rigger-domain/src/spawn.rs, crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs, crates/rigger-grounder/src/grounder/mod.rs, crates/rigger-grounder/src/grounder/workflowdef.rs, crates/rigger-grounder/src/ingest.rs, crates/rigger-store-sqlite/src/eventstore/namespace.rs, crates/rigger-worktree-git/src/worktree.rs, src/cli/hygiene.rs, src/cli/mod.rs, src/cli/setup.rs, tests/canary_model_drift_periphery.rs, tests/cli.rs, tests/common/cli.rs, tests/common/fixtures/conductor.rs, tests/common/fixtures/fold.rs, tests/common/fixtures/instructions.rs, tests/common/fixtures/spawn.rs, tests/halted_spawn_wip_recovery_periphery.rs, tests/native_driver_pipelining_behavior.rs, tests/regate_landed_on_resume_periphery.rs, tests/spec_critique_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/blast_radius_eval.rs:262-264` `span`
- `crates/rigger-conductor/src/conductor.rs:684-686` `deferred_gate_verdict_key`
- `crates/rigger-conductor/src/conductor.rs:695-697` `deferred_gate_failed_key`
- `crates/rigger-conductor/src/conductor.rs:13297-13304` `review_protocol`
- `crates/rigger-domain/src/contextgraph.rs:824-826` `not_folded`
- `crates/rigger-domain/src/contextgraph.rs:900-902` `rebuild_owed_refusal`
- `crates/rigger-domain/src/instructions.rs:226-228` `operator_path`
- `crates/rigger-domain/src/review.rs:431-433` `critique_unit`
- `crates/rigger-domain/src/spawn.rs:133-135` `lens_role`
- `crates/rigger-domain/src/spawn.rs:349-351` `speculation_group_id`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:2208-2210` `pruned_copy`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:2225-2227` `shadow_of`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:3715-3721` `live_ref_sql`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:5562-5564` `edge_text_sql`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:5955-5957` `file_half_sql`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:6170-6172` `proof_rows_by`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:21956-21960` `refusing`
- `crates/rigger-grounder/src/grounder/mod.rs:207-213` `retired_grounder_error`
- `crates/rigger-grounder/src/grounder/workflowdef.rs:26-28` `stage_id`
- `crates/rigger-grounder/src/grounder/workflowdef.rs:30-32` `gate_id`
- `crates/rigger-grounder/src/grounder/workflowdef.rs:34-36` `agent_id`
- `crates/rigger-grounder/src/ingest.rs:614-616` `code_identity`
- `crates/rigger-store-sqlite/src/eventstore/namespace.rs:98-100` `prefix_for`
- `crates/rigger-worktree-git/src/worktree.rs:1940-1942` `shared_build_cache_guard_path`
- `src/cli/hygiene.rs:739-751` `live_writer_refusal`
- `src/cli/mod.rs:4884-4886` `skill_source_rel`
- `src/cli/mod.rs:5533-5539` `spec_lint_next_step`
- `src/cli/setup.rs:1112-1117` `live_run_rebuild_refusal`
- `tests/canary_model_drift_periphery.rs:113-115` `prose_claiming`
- `tests/cli.rs:7003-7011` `fire_stop_failure_hook`
- `tests/cli.rs:8965-8967` `acquired_by`
- `tests/cli.rs:8970-8974` `keep_awake_unavailable`
- `tests/common/cli.rs:647-652` `emit_refused`
- `tests/common/fixtures/conductor.rs:109-118` `implementer_pattern_block`
- `tests/common/fixtures/conductor.rs:123-133` `review_pattern_list`
- `tests/common/fixtures/fold.rs:399-401` `lock_held_past`
- `tests/common/fixtures/fold.rs:406-408` `catch_up_short_after`
- `tests/common/fixtures/instructions.rs:277-279` `builtin_instruction_source`
- `tests/common/fixtures/spawn.rs:131-136` `cli_progress_frame`
- `tests/common/fixtures/spawn.rs:274-276` `printing`
- `tests/halted_spawn_wip_recovery_periphery.rs:128-130` `unit_branch`
- `tests/native_driver_pipelining_behavior.rs:597-599` `worker_prompts_harness`
- `tests/regate_landed_on_resume_periphery.rs:77-79` `unit_branch`
- `tests/spec_critique_periphery.rs:997-999` `answered`

#### `dup-be7f6094aaff` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-conductor/src/canary_store.rs, crates/rigger-grounder/src/grounder/design/extract.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/canary_store.rs:871-873` `any_finding_is_critical`
- `crates/rigger-grounder/src/grounder/design/extract.rs:162-164` `is_handbook_path`

#### `dup-f11d60ead37b` (near, 17 sites)

Proposed home: `a new shared module (sites span 8 files: crates/rigger-conductor/src/conductor.rs, crates/rigger-domain/src/spawn.rs, crates/rigger-driver/src/driver/claude_code.rs, tests/cli.rs, tests/common/fixtures/fold.rs, tests/common/fixtures/graph.rs, tests/postmerge_gate_error_cleanup_periphery.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:348-350` `review_round_start_key`
- `crates/rigger-conductor/src/conductor.rs:356-358` `failed_key`
- `crates/rigger-conductor/src/conductor.rs:363-365` `verified_key`
- `crates/rigger-conductor/src/conductor.rs:369-371` `spec_defect_lesson_key`
- `crates/rigger-conductor/src/conductor.rs:375-377` `spec_defect_key`
- `crates/rigger-conductor/src/conductor.rs:383-385` `spec_defect_escalated_key`
- `crates/rigger-conductor/src/conductor.rs:436-438` `compensation_queued_key`
- `crates/rigger-conductor/src/conductor.rs:1111-1113` `conflict_regenerate_key`
- `crates/rigger-conductor/src/conductor.rs:13445-13447` `audit_id`
- `crates/rigger-domain/src/spawn.rs:277-279` `spawn_id`
- `crates/rigger-driver/src/driver/claude_code.rs:2033-2041` `stop_message`
- `tests/cli.rs:7920-7933` `rendezvous`
- `tests/common/fixtures/fold.rs:757-759` `code_batch_key`
- `tests/common/fixtures/graph.rs:204-206` `spoke_id`
- `tests/postmerge_gate_error_cleanup_periphery.rs:57-59` `expected_postmerge_dir`
- `tests/postmerge_gate_error_cleanup_periphery.rs:60-62` `expected_postmerge_branch`
- `tests/simplification_audit.rs:5658-5660` `sample_key`

#### `dup-261fedbfbd66` (near, 20 sites)

Proposed home: `a new shared module (sites span 15 files: crates/rigger-conductor/src/conductor.rs, crates/rigger-domain/src/contextgraph/query.rs, crates/rigger-driver/src/driver/mod.rs, crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs, crates/rigger-store-sqlite/src/eventstore/kurrentdb.rs, crates/rigger-store-sqlite/src/spawn_store.rs, src/cli/critique.rs, tests/cli.rs, tests/common/cli.rs, tests/common/fixtures/config.rs, tests/common/fixtures/plan_critique.rs, tests/common/fixtures/spawn.rs, tests/compaction_generations_periphery.rs, tests/new_run_critique_refusal_periphery.rs, tests/no_os_kill_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:453-455` `adoption_provenance_key`
- `crates/rigger-conductor/src/conductor.rs:468-470` `quarantine_record_key`
- `crates/rigger-domain/src/contextgraph/query.rs:241-243` `entity_id`
- `crates/rigger-driver/src/driver/mod.rs:64-66` `progress_framed`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:3938-3943` `retarget_sql`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:5506-5508` `id_range_sql`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:5611-5617` `superseded_edges_sql`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:6663-6671` `declaration_sql`
- `crates/rigger-store-sqlite/src/eventstore/kurrentdb.rs:329-331` `group_stream`
- `crates/rigger-store-sqlite/src/spawn_store.rs:74-76` `what`
- `src/cli/critique.rs:172-174` `head`
- `tests/cli.rs:7333-7339` `api_error_session`
- `tests/common/cli.rs:656-661` `no_result_failure`
- `tests/common/fixtures/config.rs:441-443` `separator_refusal`
- `tests/common/fixtures/plan_critique.rs:35-67` `plan_protocol_text`
- `tests/common/fixtures/plan_critique.rs:118-123` `spec_defect_halt_text`
- `tests/common/fixtures/spawn.rs:235-248` `probe_arms`
- `tests/compaction_generations_periphery.rs:5103-5108` `closed_unit_line`
- `tests/new_run_critique_refusal_periphery.rs:156-158` `no_critic_line`
- `tests/no_os_kill_audit.rs:52-54` `join`

#### `dup-a9c4f3d90dac` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-conductor/src/conductor.rs, crates/rigger-domain/src/spawn.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:499-501` `unit_of_gate_key`
- `crates/rigger-conductor/src/conductor.rs:542-544` `digest_address`
- `crates/rigger-domain/src/spawn.rs:383-385` `unit_of`

#### `dup-b6030639057d` (exact, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:1486-1488` `is_interrupted`
- `crates/rigger-conductor/src/conductor.rs:1493-1495` `is_store_fault`

#### `dup-cb25e49f6e22` (semantic, 2 sites)

Proposed home: `one shared `append_and_fold` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-conductor/src/conductor.rs:3437-3452` `append_and_fold`
- `crates/rigger-grounder/src/ingest.rs:143-156` `append_and_fold`

#### `dup-4e1d0e78e3c6` (semantic, 3 sites)

Proposed home: `one shared `read_current_run` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-conductor/src/conductor.rs:3912-3914` `read_current_run`
- `crates/rigger-dash/src/mcpserver.rs:553-556` `read_current_run`
- `crates/rigger-domain/src/run/read.rs:101-108` `read_current_run`

#### `dup-f13596146362` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-conductor/src/conductor.rs, crates/rigger-domain/src/contextgraph/query.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:4153-4155` `spawn_is_recorded`
- `crates/rigger-domain/src/contextgraph/query.rs:589-591` `is_shared`

#### `dup-7310d4338924` (semantic, 23 sites)

Proposed home: `one error-shaping helper module`

mandatory sweep: error-shaping helper functions - 23 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-conductor/src/conductor.rs:5482-5500` `guard_review_round_tree_on_tier_err`
- `crates/rigger-conductor/src/conductor.rs:32004-32066` `integrate_plan_commits_wraps_any_hard_error_with_the_plan_landing_marker`
- `crates/rigger-conductor/src/conductor.rs:46918-47006` `a_resolver_round_ending_on_an_unmarked_error_is_spent_like_one_resolving_nothing`
- `crates/rigger-conductor/src/held.rs:809-900` `a_marked_error_or_an_ended_spawn_passes_held_or_not`
- `crates/rigger-domain/src/agent.rs:566-568` `no_result_error`
- `crates/rigger-domain/src/agent.rs:640-678` `an_error_answers_the_class_it_carries`
- `crates/rigger-domain/src/ingest.rs:604-649` `a_walk_reaches_every_batch_past_a_failed_one_and_answers_the_first_error`
- `crates/rigger-driver/src/driver/claude_code.rs:2046-2051` `read_agent_stream_error`
- `crates/rigger-grounder/src/grounder/mod.rs:207-213` `retired_grounder_error`
- `crates/rigger-worktree-git/src/worktree.rs:4038-4065` `land_reports_a_generic_error_for_a_refusal_that_is_neither_tip_moved_nor_blocked`
- `crates/rigger-worktree-git/src/worktree.rs:5485-5539` `revert_on_base_aborts_and_errors_on_a_conflicting_revert`
- `src/cli/mod.rs:4974-4976` `setup_read_error`
- `tests/adoption_keys_on_criterion_periphery.rs:2008-2148` `a_quarantine_record_whose_ref_was_since_deleted_hard_errors_instead_of_silently_starting_fresh`
- `tests/batched_fold_cadence.rs:177-273` `append_and_fold_is_best_effort_on_fold_error_and_a_no_op_on_an_empty_batch`
- `tests/canary_item_sharding_jobs_cap_periphery.rs:314-413` `run_canary_runs_every_item_to_completion_even_when_one_items_spawn_errors`
- `tests/cli.rs:7026-7040` `api_error_worker_arm`
- `tests/cli.rs:7333-7339` `api_error_session`
- `tests/cli.rs:7511-7755` `an_is_error_result_reruns_uncharged_and_holds_as_its_class_says`
- `tests/cli.rs:8694-8819` `a_served_error_the_decorator_passes_takes_its_spawn_site_path`
- `tests/cli.rs:13902-13941` `run_workflow_prints_the_budget_halt_reason_on_stderr_when_its_conductor_returns`
- `tests/cli.rs:17146-17265` `a_resolver_round_ending_on_an_error_is_spent_and_a_restart_answers_it_from_the_log`
- `tests/compaction_generations_periphery.rs:4776-4817` `setup_whose_rebuild_meets_a_storage_error_fails_and_the_next_setup_folds_the_event`
- `tests/grounder_name_contract.rs:40-171` `public_name_contract_predicate_error_and_resolver_agree`

#### `dup-6f3fb4f23036` (near, 2 sites)

Proposed home: `conductor::run_ctx`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:11177-11191` `record_merge_attempt`
- `crates/rigger-conductor/src/conductor.rs:11223-11238` `record_landing_intent`

#### `dup-a6427ffb5410` (exact, 4 sites)

Proposed home: `a new shared module (sites span 4 files: crates/rigger-conductor/src/conductor.rs, tests/cli.rs, tests/gate_scratch_root_periphery.rs, tests/spec_critique_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:13252-13257` `unit_size_cap`
- `tests/cli.rs:19213-19218` `sweep_tool_refusal`
- `tests/gate_scratch_root_periphery.rs:58-64` `recording_gate`
- `tests/spec_critique_periphery.rs:983-985` `verb_head`

#### `dup-cde89b281edb` (near, 4 sites)

Proposed home: `a new shared module (sites span 4 files: crates/rigger-conductor/src/conductor.rs, crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs, crates/rigger-store-sqlite/src/eventstore/sqlite.rs, tests/common/fixtures/fold.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:14295-14297` `unit_branch`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:6176-6181` `proof_targets_sql`
- `crates/rigger-store-sqlite/src/eventstore/sqlite.rs:1124-1126` `key_expr`
- `tests/common/fixtures/fold.rs:362-364` `not_folded_suffix`

#### `dup-c6dfc41a628c` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-conductor/src/conductor.rs, tests/plan_stage_commit_landing_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:17038-17043` `answering`
- `tests/plan_stage_commit_landing_periphery.rs:283-288` `new`

#### `dup-c36d696064fd` (near, 4 sites)

Proposed home: `conductor::stub`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:17093-17095` `prompts_for`
- `crates/rigger-conductor/src/conductor.rs:17103-17105` `dirs_for`
- `crates/rigger-conductor/src/conductor.rs:17109-17111` `system_prompt_for`
- `crates/rigger-conductor/src/conductor.rs:17115-17117` `title_for`

#### `dup-ebee743f02de` (near, 14 sites)

Proposed home: `a new shared module (sites span 4 files: crates/rigger-conductor/src/conductor.rs, crates/rigger-domain/src/eventstore.rs, tests/common/fixtures/events.rs, tests/common/real_driver_spy.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:17127-17129` `spawn_ids`
- `crates/rigger-conductor/src/conductor.rs:42411-42413` `calls`
- `crates/rigger-conductor/src/conductor.rs:42414-42416` `targets`
- `crates/rigger-conductor/src/conductor.rs:42417-42419` `gate_scratches`
- `crates/rigger-conductor/src/conductor.rs:42420-42422` `store_fences`
- `crates/rigger-conductor/src/conductor.rs:42423-42425` `build_cache_guards`
- `crates/rigger-conductor/src/conductor.rs:42426-42428` `build_cache_dirs`
- `crates/rigger-conductor/src/conductor.rs:42565-42567` `calls`
- `crates/rigger-domain/src/eventstore.rs:274-276` `last`
- `crates/rigger-domain/src/eventstore.rs:604-606` `recv`
- `crates/rigger-domain/src/eventstore.rs:614-616` `try_recv`
- `crates/rigger-domain/src/eventstore.rs:619-621` `err`
- `tests/common/fixtures/events.rs:974-976` `reads`
- `tests/common/real_driver_spy.rs:36-38` `outputs`

#### `dup-60da41568392` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-conductor/src/conductor.rs, crates/rigger-domain/src/eventstore.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:17133-17139` `spawned`
- `crates/rigger-domain/src/eventstore.rs:504-506` `covers`

#### `dup-64397d781d9d` (semantic, 94 sites)

Proposed home: `a single injected process-spawn port every Command::new site routes through instead of constructing its own Command`

mandatory sweep: Command::new call sites - 94 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-conductor/src/conductor.rs:17280-17280` `Command::new`
- `crates/rigger-conductor/src/conductor.rs:17285-17285` `Command::new`
- `crates/rigger-conductor/src/conductor.rs:30595-30595` `Command::new`
- `crates/rigger-conductor/src/conductor.rs:45949-45949` `Command::new`
- `crates/rigger-conductor/src/conductor.rs:45971-45971` `Command::new`
- `crates/rigger-conductor/src/conductor.rs:47388-47388` `Command::new`
- `crates/rigger-driver/src/reaped_child.rs:137-137` `Command::new`
- `crates/rigger-driver/src/reaped_child.rs:259-259` `Command::new`
- `crates/rigger-gates-shell/src/gate.rs:1633-1633` `Command::new`
- `crates/rigger-grounder/src/grounder/mod.rs:1001-1001` `Command::new`
- `crates/rigger-process/src/budget.rs:198-198` `Command::new`
- `crates/rigger-process/src/budget.rs:236-236` `Command::new`
- `crates/rigger-process/src/budget.rs:247-247` `Command::new`
- `crates/rigger-process/src/subprocess.rs:19-19` `Command::new`
- `crates/rigger-worktree-git/src/worktree.rs:4427-4427` `Command::new`
- `src/cli/mod.rs:6012-6012` `Command::new`
- `src/cli/mod.rs:8410-8410` `Command::new`
- `src/cli/mod.rs:11040-11040` `Command::new`
- `src/cli/mod.rs:12780-12780` `Command::new`
- `src/cli/mod.rs:12814-12814` `Command::new`
- `src/cli/run.rs:3667-3667` `Command::new`
- `src/cli/run.rs:3700-3700` `Command::new`
- `src/cli/run.rs:3744-3744` `Command::new`
- `src/cli/run.rs:3813-3813` `Command::new`
- `src/cli/setup.rs:4002-4002` `Command::new`
- `src/cli/setup.rs:4031-4031` `Command::new`
- `src/cli/validate.rs:1556-1556` `Command::new`
- `src/cli/validate.rs:1877-1877` `Command::new`
- `tests/adaptive_labels_periphery.rs:87-87` `Command::new`
- `tests/adoption_keys_on_criterion_periphery.rs:2067-2067` `Command::new`
- `tests/checkin_mutation_diff_base_periphery.rs:77-77` `Command::new`
- `tests/checkin_mutation_diff_base_periphery.rs:169-169` `Command::new`
- `tests/ci_lanes.rs:383-383` `Command::new`
- `tests/ci_lanes.rs:406-406` `Command::new`
- `tests/claude_code_stream_periphery.rs:1162-1162` `Command::new`
- `tests/cli.rs:1666-1666` `Command::new`
- `tests/cli.rs:5809-5809` `Command::new`
- `tests/cli.rs:20259-20259` `Command::new`
- `tests/cli.rs:20990-20990` `Command::new`
- `tests/cli.rs:26751-26751` `Command::new`
- `tests/cli.rs:31926-31926` `Command::new`
- `tests/cli.rs:35064-35064` `Command::new`
- `tests/common/cli.rs:23-23` `Command::new`
- `tests/common/cli.rs:76-76` `Command::new`
- `tests/common/cli.rs:624-624` `Command::new`
- `tests/common/cli.rs:878-878` `Command::new`
- `tests/common/fixtures/conductor.rs:381-381` `Command::new`
- `tests/common/fixtures/conductor.rs:467-467` `Command::new`
- `tests/common/fixtures/git.rs:25-25` `Command::new`
- `tests/common/fixtures/git.rs:45-45` `Command::new`
- `tests/common/fixtures/host.rs:10-10` `Command::new`
- `tests/common/fixtures/host.rs:92-92` `Command::new`
- `tests/common/fixtures/host.rs:103-103` `Command::new`
- `tests/common/fixtures/host.rs:215-215` `Command::new`
- `tests/common/layer_cli.rs:71-71` `Command::new`
- `tests/common/mod.rs:175-175` `Command::new`
- `tests/common/repo.rs:23-23` `Command::new`
- `tests/common/repo.rs:198-198` `Command::new`
- `tests/common/repo.rs:535-535` `Command::new`
- `tests/common/served.rs:295-295` `Command::new`
- `tests/compiler_pass_stage1_audit.rs:193-193` `Command::new`
- `tests/compiler_pass_stage1_audit.rs:211-211` `Command::new`
- `tests/core_lane_purity_audit.rs:319-319` `Command::new`
- `tests/dash_release_ready.rs:348-348` `Command::new`
- `tests/gitsemver_derivation.rs:86-86` `Command::new`
- `tests/gitsemver_worktree_periphery.rs:111-111` `Command::new`
- `tests/halted_spawn_wip_recovery_periphery.rs:819-819` `Command::new`
- `tests/hermetic_test_git_audit.rs:203-203` `Command::new`
- `tests/hermetic_test_git_audit.rs:235-235` `Command::new`
- `tests/hermetic_test_git_audit.rs:307-307` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:353-353` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:1723-1723` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:1814-1814` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:2026-2026` `Command::new`
- `tests/meta_phases_declaration_periphery.rs:83-83` `Command::new`
- `tests/mutation_runner_pdeathsig_periphery.rs:107-107` `Command::new`
- `tests/mutation_runner_pdeathsig_periphery.rs:167-167` `Command::new`
- `tests/native_driver_pipelining_behavior.rs:293-293` `Command::new`
- `tests/no_os_kill_test_helper_periphery.rs:30-30` `Command::new`
- `tests/no_os_kill_test_helper_periphery.rs:48-48` `Command::new`
- `tests/phase_of_role_mapping_periphery.rs:72-72` `Command::new`
- `tests/principle_gates_wiring.rs:113-113` `Command::new`
- `tests/principle_gates_wiring.rs:456-456` `Command::new`
- `tests/product_binary_authority_periphery.rs:155-155` `Command::new`
- `tests/reset_build_cache_periphery.rs:257-257` `Command::new`
- `tests/reset_build_cache_periphery.rs:269-269` `Command::new`
- `tests/reset_build_cache_periphery.rs:346-346` `Command::new`
- `tests/reset_build_cache_periphery.rs:364-364` `Command::new`
- `tests/revert_on_base_hook_bypass_periphery.rs:168-168` `Command::new`
- `tests/scaffold_grounder_resolves.rs:96-96` `Command::new`
- `tests/step_attention_periphery.rs:544-544` `Command::new`
- `tests/store_flag_precedence.rs:78-78` `Command::new`
- `tests/store_resolution.rs:153-153` `Command::new`
- `tests/validate_behind_the_tree_periphery.rs:128-128` `Command::new`

#### `dup-47e4d1c77944` (semantic, 643 sites)

Proposed home: `one .rigger-relative path-composition helper`

mandatory sweep: .rigger-path string literals - 643 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-conductor/src/conductor.rs:20841-20841` `"the repo's own .rigger config must load"`
- `crates/rigger-conductor/src/conductor.rs:43588-43588` `"{root}/.rigger/project.id"`
- `crates/rigger-conductor/src/conductor.rs:43590-43590` `"{root}/.rigger/graph.db"`
- `crates/rigger-config-files/src/config_store.rs:448-448` `".rigger"`
- `crates/rigger-config-files/src/config_store.rs:449-449` `".rigger"`
- `crates/rigger-config-files/src/config_store.rs:473-473` `"../../examples/demo/.rigger"`
- `crates/rigger-config-files/src/config_store.rs:474-474` `".rigger"`
- `crates/rigger-config-files/src/config_store.rs:1526-1526` `".rigger/agents/sdet-author.md"`
- `crates/rigger-config-files/src/config_store.rs:1527-1527` `"the shipped .rigger/agents/sdet-author.md must exist"`
- `crates/rigger-config-files/src/config_store.rs:1554-1554` `".rigger/agents/sdet.md"`
- `crates/rigger-config-files/src/config_store.rs:1555-1555` `"the shipped .rigger/agents/sdet.md must exist"`
- `crates/rigger-config-files/src/config_store.rs:2168-2168` `".rigger"`
- `crates/rigger-config-files/src/config_store.rs:2169-2169` `"create .rigger dir"`
- `crates/rigger-config-files/src/config_store.rs:2201-2201` `".rigger"`
- `crates/rigger-config-files/src/config_store.rs:2202-2202` `"create .rigger dir"`
- `crates/rigger-config-files/src/config_store.rs:2223-2223` `".rigger"`
- `crates/rigger-config-files/src/config_store.rs:2224-2224` `"create .rigger dir"`
- `crates/rigger-dash/src/dash.rs:3946-3946` `"{root}/.rigger/events.db"`
- `crates/rigger-dash/src/dash.rs:3997-3997` `"/.rigger/events.db"`
- `crates/rigger-domain/src/config.rs:21-21` `".rigger"`
- `crates/rigger-domain/src/docs.rs:228-228` `"The EVENT LOG keeps knowledge, never the derived index: an ingest records one ledger \
         entry per file generation, and the graph re-derives the code entities, inferred edges, \
         design links and doc concepts from your sources. A log recorded before that still holds \
         the derived index itself, every generation of every file, and `rigger reset --derived` \
         is the ONE-TIME MIGRATION that converts it: for each file it rewrites the first row of \
         the file's LATEST derived batch into that generation's ledger entry, in place, and \
         deletes every other derived event, so no derived event is left behind and events.db \
         shrinks on disk. Every other event survives byte-for-byte - lessons, decisions, \
         findings, gate verdicts, and the whole run history `rigger stats` and replay read. It \
         writes nothing to graph.db, so the live graph is unchanged, and a later rebuild \
         re-derives each entry's batch from the repository's objects and the tree. It reports \
         the entries it converted, the derived events it shed and, on its own line, how many of \
         those named no file - no replay key, or one that does not parse; no entry re-asserts \
         what such an event asserted, so the live graph keeps it only until the first rebuild. \
         Run again on a migrated log it says there is no derived event to shed - that is the \
         expected report, not a failure. WHAT IT COSTS TO RUN: the compaction rewrites events.db in full and holds \
         a COMPLETE COPY of the log in the process's MEMORY while it does, never in a temporary \
         directory, so it needs free memory of about the size of your log, no \
         temporary-directory setting changes where the copy goes, and a crash leaves no \
         temporary file behind to clean up. It needs room on disk too, beside the log: \
         events.db is in write-ahead mode, so the rewritten file passes through events.db-wal \
         before it replaces the old pages, and the partition holding .rigger/ needs about the \
         compacted size of the log free while it does. A machine short of that memory or that \
         room fails the rewrite and nothing else: the rewrite rolls back and the log stays as \
         the migration left it. It rewrites only when the FILE is holding reclaimable free \
         pages, which is not the same as this run having shed something: a run with nothing \
         to shed from an already-compact log leaves the file exactly as it found it and says \
         so, while a run that sheds nothing from a log still holding free pages \
         reclaims them. That is what makes the re-run a real remedy - if the rewrite fails after \
         the migration has committed, the command still reports what it converted and shed and \
         names the failure, and because that change is durable and the space it freed is still \
         free in the file, re-running it is both safe and the way to reclaim that space. The two \
         flags COMPOSE \
         and each sheds its own accumulation: `rigger reset --runs --derived` sheds the dead-run \
         graph rows and the derived index in one pass. Both are one-shot maintenance you run \
         BETWEEN runs, never against a live one - and `--derived` ENFORCES that itself: the \
         migration leaves revision gaps by design, and a writer whose cursor was built before it \
         ran could reissue a gap and reorder the log, so it refuses while the run is live - a \
         `rigger step` holds its lock, an in-flight spawn (one with no recorded result, or only \
         the step's liveness fault) has a liveness marker younger than its wall-clock bound, or \
         a driver registration for this store has a heartbeat inside the idle window - naming \
         what it found. A run whose driver died is not live: units it left non-terminal never \
         block the compaction, a spawn with no marker never does, and an in-flight spawn stops \
         blocking once its marker outlives the spawn's bound or a real result is recorded for \
         it. An unbounded spawn's marker never outlives its bound, so record that spawn's \
         result to end it. Every `rigger step`, `run` and `serve` registers as the run's \
         driver, so the last step's stamp counts as a live driver for the idle window; a \
         courier's (`emit`, `result`, `progress`) discovery refresh of that registration never \
         does. `--force-live` overrides the refusal for an operator certain no writer is using \
         the store; it checks nothing.\n"`
- `crates/rigger-domain/src/docs.rs:893-893` `"Store hygiene for rigger's own state - growing .rigger/ disk usage, \
         the bloat advisory from `rigger validate`, or `rigger step`/replay running slow. \
         Read this before running `rigger reset` or touching any store file by hand."`
- `crates/rigger-domain/src/docs.rs:897-897` `"rigger keeps three stores under `.rigger/`, and only one of them holds anything \
             durable:\n"`
- `crates/rigger-domain/src/docs.rs:970-970` `"`rigger graph build` folds the project's source straight into `.rigger/graph.db` - \
             no run, no `RunStarted`, and nothing on the event log but one ledger entry per file \
             batch whose generation moved, since the tree re-derives the batch itself. \
             It CREATES the store when the checkout is cold (`.rigger/` does not exist yet) and \
             REFRESHES an existing store incrementally: an unchanged file re-ingests nothing, and \
             it reuses the exact same walk-and-content-key ingest authority a live run uses, so a \
             standalone build and a run can never fold the same file under two different keys.\n"`
- `crates/rigger-domain/src/docs.rs:981-981` `"Never force a rebuild by deleting `.rigger/graph.db` (or `events.db`) and \
         re-running `rigger graph build` on the empty result. Deleting the log throws away \
         truth that no rebuild can get back, and deleting only the graph is unnecessary work \
         `rigger graph build` already does FOR you, incrementally, without erasing anything \
         first. If lookups are empty, just run `rigger graph build`; only reach for \
         rigger-reset-store if you specifically mean to prune, not rebuild.\n"`
- `crates/rigger-domain/src/docs.rs:1005-1005` `"`rigger reindex <file>...` re-parses ONLY the named files and persists the delta to \
             the project's symbols grounding index at `.rigger/symbols/` - the fast, targeted fix \
             for an index that has drifted from files you just changed (a unit's own commit, a \
             rebase, a branch switch). It is scoped strictly to the symbols index, a DIFFERENT \
             store from the structural context graph, so it costs only the named files, never a \
             walk of the whole tree.\n"`
- `crates/rigger-domain/src/docs.rs:1738-1738` `"the partition holding .rigger/ needs about the compacted size of the log \
                     free"`
- `crates/rigger-gates-shell/src/gate.rs:355-355` `".rigger-cache-probe-{}"`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:8871-8871` `"local:/p/.rigger/events.db"`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:8874-8874` `"local:/q/.rigger/events.db"`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:8894-8894` `"local:/p/.rigger/events.db"`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:10865-10865` `"local:/elsewhere/.rigger/events.db"`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:12610-12610` `".rigger/workflow.yml"`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:12618-12618` `".rigger/workflow.yml"`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:12626-12626` `".rigger/workflow.yml"`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:12634-12634` `".rigger/workflow.yml"`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:12642-12642` `".rigger/workflow.yml"`
- `crates/rigger-grounder/src/grounder/mod.rs:444-444` `".rigger"`
- `crates/rigger-grounder/src/grounder/mod.rs:961-961` `".rigger/workflow.yml"`
- `crates/rigger-grounder/src/grounder/workflowdef.rs:584-584` `".rigger"`
- `crates/rigger-grounder/src/grounder/workflowdef.rs:672-672` `"this project's own .rigger/workflow.yml must extract at least one event"`
- `crates/rigger-grounder/src/ingest.rs:1073-1073` `".rigger"`
- `crates/rigger-grounder/src/ingest.rs:1075-1075` `".rigger"`
- `crates/rigger-grounder/src/ingest.rs:1084-1084` `"gw/.rigger/workflow.yml@"`
- `crates/rigger-grounder/src/ingest.rs:1102-1102` `"one code batch (a.rs) plus one workflow-definition batch (.rigger/workflow.yml) \
             must both advance the shared batch count; got {}"`
- `crates/rigger-grounder/src/ingest.rs:1131-1131` `".rigger"`
- `crates/rigger-grounder/src/ingest.rs:1133-1133` `".rigger"`
- `crates/rigger-grounder/src/ingest.rs:1167-1167` `".rigger"`
- `crates/rigger-grounder/src/ingest.rs:2513-2513` `".rigger"`
- `crates/rigger-grounder/src/ingest.rs:2517-2517` `".rigger/persona.md"`
- `crates/rigger-grounder/src/ingest.rs:2526-2526` `".rigger/persona.md"`
- `crates/rigger-grounder/src/ingest.rs:2539-2539` `"gd/.rigger/"`
- `crates/rigger-grounder/src/ingest.rs:2540-2540` `"premise: the whole walk lowers both docs and nothing under .rigger; got {walked:?}"`
- `crates/rigger-process/src/reap.rs:758-758` `".rigger"`
- `crates/rigger-process/src/reap.rs:1099-1099` `"a relocated/cache-home-style authorized_root with no .rigger/tmp relationship \
             must still authorize the reap"`
- `crates/rigger-store-sqlite/src/registry.rs:419-419` `"/p/.rigger/events.db"`
- `crates/rigger-store-sqlite/src/registry.rs:421-421` `"local:/p/.rigger/events.db"`
- `crates/rigger-store-sqlite/src/registry.rs:460-460` `"/home/dev/proj/.rigger/events.db"`
- `crates/rigger-store-sqlite/src/registry.rs:469-469` `"/home/dev/proj/.rigger/events.db"`
- `crates/rigger-store-sqlite/src/registry.rs:486-486` `"/home/dev/proj/.rigger/events.db"`
- `crates/rigger-store-sqlite/src/registry.rs:493-493` `"/home/dev/proj/.rigger/events.db"`
- `crates/rigger-store-sqlite/src/registry.rs:507-507` `"/home/dev/proj-b/.rigger/events.db"`
- `crates/rigger-store-sqlite/src/registry.rs:525-525` `"/a/.rigger/events.db"`
- `crates/rigger-store-sqlite/src/registry.rs:526-526` `"/b/.rigger/events.db"`
- `crates/rigger-store-sqlite/src/registry.rs:552-552` `"/home/dev/proj/.rigger/events.db"`
- `crates/rigger-store-sqlite/src/registry.rs:570-570` `"/home/dev/proj/.rigger/events.db"`
- `crates/rigger-store-sqlite/src/registry.rs:685-685` `"/home/dev/proj/.rigger/events.db"`
- `crates/rigger-store-sqlite/src/registry.rs:708-708` `r#"{"project":"proj","root":"/home/dev/proj","store":{"kind":"local","path":"/home/dev/proj/.rigger/events.db"},"heartbeat_ms":1000}"#`
- `crates/rigger-store-sqlite/src/registry.rs:729-729` `"/home/dev/proj/.rigger/events.db"`
- `crates/rigger-store-sqlite/src/registry.rs:789-789` `"/home/dev/proj/.rigger/events.db"`
- `crates/rigger-worktree-git/src/worktree.rs:5813-5813` `"{repo_path}/.rigger/tmp"`
- `crates/rigger-worktree-git/src/worktree.rs:5814-5814` `"the default must no longer nest inside the repo's own .rigger: {dflt:?}"`
- `crates/rigger-worktree-git/src/worktree.rs:5817-5817` `"/.rigger/"`
- `crates/rigger-worktree-git/src/worktree.rs:5817-5817` `"/.rigger"`
- `crates/rigger-worktree-git/src/worktree.rs:5818-5818` `"the default must never live under any .rigger: {dflt:?}"`
- `crates/rigger-worktree-git/src/worktree.rs:5837-5837` `"{repo_path}/.rigger/tmp"`
- `crates/rigger-worktree-git/src/worktree.rs:5861-5861` `"~/.rigger-scratch-test"`
- `crates/rigger-worktree-git/src/worktree.rs:5862-5862` `"{home}/.rigger-scratch-test"`
- `crates/rigger-worktree-git/src/worktree.rs:7551-7551` `"{base}..rigger-run"`
- `crates/rigger-worktree-git/src/worktree.rs:7581-7581` `".rigger"`
- `src/cli/critique.rs:318-318` `"{COMMAND}: the critic {:?} the workflow names has no definition under \
                 .rigger/agents"`
- `src/cli/hygiene.rs:689-689` `"a `rigger step` or `rigger run` is running right now (it holds .rigger/step.lock)"`
- `src/cli/mod.rs:803-803` `"the server event store is selected but no connection string is set - provide one via \
         --conn <url>, the KURRENTDB_CONN environment variable, or the .rigger/store.conn \
         secret file"`
- `src/cli/mod.rs:1283-1283` `"migrated project history to the durable identity: renamed {n} stream(s) \
                     from the legacy namespace {legacy:?} to the minted identity {minted:?} \
                     (.rigger/{PROJECT_ID_FILE})"`
- `src/cli/mod.rs:1362-1362` `"rigger: migrated project identity - renamed {n} stream(s) from the legacy \
             namespace {legacy:?} to the minted identity {minted:?} (.rigger/{PROJECT_ID_FILE}); \
             recorded its decision (position {}){}"`
- `src/cli/mod.rs:2307-2307` `"workflow: the per-project JS driver is not provisioned (looked for {}). \
         Run `rigger setup` to write the shim into .rigger/shim/ and install its \
         dependencies, then re-run `rigger workflow`."`
- `src/cli/mod.rs:3624-3624` `".rigger-workflow-provenance"`
- `src/cli/mod.rs:5148-5148` `r#"__BEGIN__
# Check rigger's code-derived docs before THIS commit lands: run `rigger docs --check`, which
# renders every docs output in memory, writes nothing, and prints `fresh <path>` or
# `drifted <path>` for each output on disk. Only the paths this repo TRACKS count, so with a
# binary that can check, the hook refuses nothing in a project that tracks none of them. The
# commit is REFUSED, naming each path, when a tracked output is drifted or its working-tree
# file differs from the index (the commit would carry content the check did not see). The
# hook never writes the working tree and never stages; staging is always the operator's own
# act. A binary is given `docs` only once its `rigger help`, which writes nothing, lists
# `rigger docs --check` (a build that predates the check would ignore the flag and render
# instead): one whose help lacks that line, or fails, has not shown it can check, so the
# commit is refused, tracked paths or not, naming it. No `rigger` binary, or a check that
# fails without reporting drift, warns and lets the commit proceed (`rigger validate` / the
# CI drift check is the hard backstop).
#
# BINARY SELECTION (spec 75): prefer a `rigger` BUILT FROM THIS TREE over whatever happens to
# sit first on PATH, so a worktree whose code legitimately changes a rendered fact is checked
# by a binary that actually reflects that change instead of deadlocking against a stale PATH
# install. Candidate order, most authoritative first: the env-provided cargo target dir
# (release then debug), this working tree's own local target (release then debug), this
# worktree's own unit-derived scratch cargo-target - its unit is read from the worktree
# directory name, `rigger-wt-<unit>` (release then debug), the run's shared step-cache target
# (debug only - unit gates build the debug profile only), and finally PATH. The check is only
# as current as the binary that runs it, which is why the tree-built candidates come first.
git_common_dir=$(git rev-parse --git-common-dir 2>/dev/null)
worktree_top=$(git rev-parse --show-toplevel 2>/dev/null)
worktree_base=$(basename "$worktree_top" 2>/dev/null)
unit=
case "$worktree_base" in
    rigger-wt-*) unit="${worktree_base#rigger-wt-}" ;;
esac
# The relocated per-unit target (spec 89): a unit's build cache is the sibling
# `cargo-target-<unit>` of its worktree under `<cache home>/rigger/<encoded repo root>`,
# where the repo root is encoded byte by byte - alphanumerics and `-` kept, every other
# byte as `_xx` hex - exactly as the binary encodes it, so this shell derivation and the
# Rust one name the same directory.
encode_repo_path() {
    p="$1"; out=""
    while [ -n "$p" ]; do
        c=${p%"${p#?}"}; p=${p#?}
        case "$c" in
            [A-Za-z0-9-]) out="$out$c" ;;
            *) out="$out$(printf '_%02x' "'$c")" ;;
        esac
    done
    printf '%s' "$out"
}
unit_release=
unit_debug=
relocated_release=
relocated_debug=
shared_debug=
if [ -n "$git_common_dir" ]; then
    if [ -n "$unit" ]; then
        unit_release="$git_common_dir/../.rigger/tmp/cargo-target-$unit/release/rigger"
        unit_debug="$git_common_dir/../.rigger/tmp/cargo-target-$unit/debug/rigger"
        repo_root=$(cd "$git_common_dir/.." 2>/dev/null && pwd -P)
        if [ -n "$repo_root" ]; then
            cache_root="${XDG_CACHE_HOME:-$HOME/.cache}/rigger/$(encode_repo_path "$repo_root")"
            relocated_release="$cache_root/cargo-target-$unit/release/rigger"
            relocated_debug="$cache_root/cargo-target-$unit/debug/rigger"
        fi
    fi
    shared_debug="$git_common_dir/../.rigger/tmp/cargo-target/debug/rigger"
fi
rigger_bin=
for candidate in \
    "${CARGO_TARGET_DIR:+$CARGO_TARGET_DIR/release/rigger}" \
    "${CARGO_TARGET_DIR:+$CARGO_TARGET_DIR/debug/rigger}" \
    "./target/release/rigger" \
    "./target/debug/rigger" \
    "$relocated_release" \
    "$relocated_debug" \
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
    # Ask the binary whether it can check BEFORE it is ever given `docs`: `rigger help` prints
    # the usage text and writes nothing on any build, and every build that has the check lists
    # `rigger docs --check` there. A build that predates the check would ignore the flag and
    # render into the working tree, so a binary whose help lacks that line, or fails, has not
    # shown it can check: it never runs `docs` at all and the commit is refused. Only the
    # listed line counts, so neither exit status is read.
    help_out=$("$rigger_bin" help 2>&1) || true
    case "$help_out" in
        *"rigger docs --check"*) ;;
        *)
            rigger_prov=$("$rigger_bin" version 2>/dev/null) || true
            echo "rigger: pre-commit: refusing to commit - $rigger_bin ($rigger_prov) has not shown it can check the docs: its help lists no rigger docs --check, so it is too old to check or does not run" 1>&2
            echo 'rigger: pre-commit: it was never given docs, so nothing was staged or written. Rebuild rigger from this tree or reinstall it, then commit again' 1>&2
            exit 1
            ;;
    esac
    check_status=0
    check_out=$("$rigger_bin" docs --check 2>/dev/null) || check_status=$?
    drift_seen=
    drifted=
    unstaged=
    # One line per output on disk, `fresh <path>` or `drifted <path>`. Read in this shell
    # (a here-document, not a pipe) so the lists below survive the loop.
    while IFS= read -r line; do
        case "$line" in
            'fresh '*) verdict=fresh; doc=${line#fresh } ;;
            'drifted '*) verdict=drifted; doc=${line#drifted }; drift_seen=1 ;;
            *) continue ;;
        esac
        # Only the outputs this repo TRACKS count; an operator project tracks none of them.
        if ! git ls-files --error-unmatch -- "$doc" >/dev/null 2>&1; then
            continue
        fi
        if [ "$verdict" = drifted ]; then
            drifted="${drifted:+$drifted }$doc"
        fi
        # The commit carries the index, so a working-tree file the check saw must equal it.
        if ! git diff --quiet -- "$doc" 2>/dev/null; then
            unstaged="${unstaged:+$unstaged }$doc"
        fi
    done <<RIGGER_DOCS_CHECK
$check_out
RIGGER_DOCS_CHECK
    if [ "$check_status" -ne 0 ] && [ -z "$drift_seen" ]; then
        echo 'rigger: pre-commit: rigger docs --check failed; committing without a docs check (rigger validate is the backstop)' 1>&2
    else
        if [ -n "$drifted" ]; then
            rigger_prov=$("$rigger_bin" version 2>/dev/null)
            echo "rigger: pre-commit: refusing to commit - these tracked docs differ from a fresh render: $drifted" 1>&2
            echo "rigger: pre-commit: checking binary: $rigger_bin ($rigger_prov)" 1>&2
            echo 'rigger: pre-commit: nothing was staged or written. Fix by either re-rendering with the tree-built binary (rigger docs, then git add the result), or reinstalling rigger so PATH points at a binary built from this tree' 1>&2
        fi
        if [ -n "$unstaged" ]; then
            echo "rigger: pre-commit: refusing to commit - these tracked docs have unstaged changes the check saw but the commit would not carry: $unstaged" 1>&2
            echo 'rigger: pre-commit: stage them (git add <path>) or restore them (git checkout <path>), then commit again' 1>&2
        fi
        if [ -n "$drifted" ] || [ -n "$unstaged" ]; then
            exit 1
        fi
    fi
else
    echo 'rigger: pre-commit: no rigger binary found (checked the tree build output and PATH); skipping the docs check (rigger validate is the backstop)' 1>&2
fi
# A passing check, a project that tracks none of the outputs, no `rigger` at all, or a check
# that failed without reporting drift falls through to here and never blocks a commit. Every
# refusal already `exit 1`'d above and never reaches this line.
true
__END__
"#`
- `src/cli/mod.rs:6140-6140` `"/.rigger/tmp/cargo-target/debug/rigger"`
- `src/cli/mod.rs:6141-6141` `"/.rigger/tmp/cargo-target/release/rigger"`
- `src/cli/mod.rs:7261-7261` `".rigger"`
- `src/cli/mod.rs:7265-7265` `".rigger"`
- `src/cli/mod.rs:7300-7300` `"probe/.rigger/events.db"`
- `src/cli/mod.rs:7301-7301` `"rigger-wt-x/.rigger/events.db"`
- `src/cli/mod.rs:7463-7463` `".rigger"`
- `src/cli/mod.rs:7499-7499` `"rigger-wt-unit-99-ghost-12345678/.rigger/events.db"`
- `src/cli/mod.rs:7534-7534` `"probe/.rigger/events.db"`
- `src/cli/mod.rs:7545-7545` `"shadow store: probe/.rigger/events.db (6B)"`
- `src/cli/mod.rs:7658-7658` `".rigger"`
- `src/cli/mod.rs:7748-7748` `".rigger"`
- `src/cli/mod.rs:7983-7983` `".rigger"`
- `src/cli/mod.rs:8436-8436` `".rigger"`
- `src/cli/mod.rs:8475-8475` `".rigger"`
- `src/cli/mod.rs:8521-8521` `".rigger"`
- `src/cli/mod.rs:8532-8532` `"must walk past the storeless worktree `.rigger/` to the repo's real store"`
- `src/cli/mod.rs:9156-9156` `".rigger"`
- `src/cli/mod.rs:9158-9158` `".rigger"`
- `src/cli/mod.rs:9314-9314` `".rigger"`
- `src/cli/mod.rs:9350-9350` `".rigger"`
- `src/cli/mod.rs:9392-9392` `".rigger"`
- `src/cli/mod.rs:9498-9498` `".rigger/store.conn beats the committed config"`
- `src/cli/mod.rs:9946-9946` `"{name} must be written into .rigger/shim/"`
- `src/cli/mod.rs:10600-10600` `".rigger/skills/using-rigger/SKILL.md"`
- `src/cli/mod.rs:10601-10601` `".rigger/skills/using-riggers/SKILL.md"`
- `src/cli/mod.rs:10604-10604` `".rigger/skills/using-riggers"`
- `src/cli/mod.rs:10611-10611` `".rigger/skills/Using-rigger/SKILL.md"`
- `src/cli/mod.rs:10612-10612` `".rigger/skills/using-rigger /SKILL.md"`
- `src/cli/mod.rs:10613-10613` `".rigger/skills/using-riggers/SKILL.md"`
- `src/cli/mod.rs:10617-10617` `".rigger/skills/Using-rigger"`
- `src/cli/mod.rs:10618-10618` `".rigger/skills/using-rigger "`
- `src/cli/mod.rs:10619-10619` `".rigger/skills/using-riggers"`
- `src/cli/mod.rs:10627-10627` `".rigger/skills/planning-a-spec"`
- `src/cli/mod.rs:10628-10628` `".rigger/skills/using-rigger"`
- `src/cli/mod.rs:10629-10629` `".rigger/skills/README.md"`
- `src/cli/mod.rs:10633-10633` `".rigger/skills/README.md"`
- `src/cli/mod.rs:10634-10634` `".rigger/skills/planning-a-spec"`
- `src/cli/mod.rs:10635-10635` `".rigger/skills/using-rigger"`
- `src/cli/mod.rs:10648-10648` `".rigger/skills/planning-a-spec"`
- `src/cli/mod.rs:10708-10708` `".rigger/skills/using-rigger/SKILL.md"`
- `src/cli/mod.rs:10712-10712` `".rigger/skills/using-rigger/SKILL.md"`
- `src/cli/mod.rs:10720-10720` `".rigger/skills"`
- `src/cli/mod.rs:10722-10722` `".rigger/skills"`
- `src/cli/mod.rs:11621-11621` `"locate_shim must return the provisioned .rigger/shim/shim.mjs"`
- `src/cli/setup.rs:268-268` `".rigger/shim/"`
- `src/cli/setup.rs:269-269` `".rigger/dash.url"`
- `src/cli/setup.rs:270-270` `".rigger/dash.marker"`
- `src/cli/setup.rs:271-271` `".rigger/dash.attempt"`
- `src/cli/setup.rs:272-272` `".rigger/store.conn"`
- `src/cli/setup.rs:457-457` `"minted the durable project identity in .rigger/{PROJECT_ID_FILE}: {id} \
             (commit it so a rename never orphans this project's history)"`
- `src/cli/setup.rs:464-464` `"scaffolded .rigger/workflow.yml (gate template set: {key})"`
- `src/cli/setup.rs:467-467` `"scaffolded .rigger/workflow.yml"`
- `src/cli/setup.rs:475-475` `"no gate template set matches this project ({catalogue} at the project \
                     root), so .rigger/workflow.yml declares no gates - declare your own under \
                     gates:"`
- `src/cli/setup.rs:483-483` `"scaffolded .rigger/instructions/README.md"`
- `src/cli/setup.rs:490-490` `"scaffolded .rigger/agents/{{{}}}"`
- `src/cli/setup.rs:743-743` `"imported {} agent {} from {} into .rigger/agents/ ({} kept - already present)"`
- `src/cli/setup.rs:808-808` `"provisioned the JS driver in .rigger/shim/ (wrote shim.mjs + package.json + \
             package-lock.json and ran npm install)"`
- `src/cli/setup.rs:1530-1530` `"kept existing .rigger/agents/{name} (import never overwrites)"`
- `src/cli/setup.rs:1558-1558` `"imported .rigger/agents/{name} (id: {id})"`
- `src/cli/setup.rs:2588-2588` `"nothing is written - no .rigger, no .claude, no .gitignore"`
- `src/cli/setup.rs:2655-2655` `"scaffolded .rigger/workflow.yml"`
- `src/cli/setup.rs:2660-2660` `"no gate template set matches this project (rust: Cargo.toml at the project \
                  root), so .rigger/workflow.yml declares no gates - declare your own under gates:"`
- `src/cli/setup.rs:2686-2686` `"scaffolded .rigger/workflow.yml (gate template set: demo)"`
- `src/cli/setup.rs:2714-2714` `"scaffolded .rigger/workflow.yml"`
- `src/cli/setup.rs:2715-2715` `"no gate template set matches this project (demo: a or b; other: c at the project \
                 root), so .rigger/workflow.yml declares no gates - declare your own under gates:"`
- `src/cli/setup.rs:2732-2732` `".rigger/workflow.yml"`
- `src/cli/setup.rs:3052-3052` `".rigger/agents/"`
- `src/cli/setup.rs:3106-3106` `".rigger/dash.url"`
- `src/cli/setup.rs:3107-3107` `".rigger/dash.marker"`
- `src/cli/setup.rs:3108-3108` `".rigger/dash.attempt"`
- `src/cli/setup.rs:3117-3117` `".rigger/store.conn"`
- `src/cli/setup.rs:3134-3134` `".rigger/\n"`
- `src/cli/setup.rs:3140-3140` `".rigger/dash.url"`
- `src/cli/setup.rs:3143-3143` `".rigger/dash.marker"`
- `src/cli/setup.rs:3146-3146` `".rigger/dash.attempt"`
- `src/cli/setup.rs:3147-3147` `"setup appends the explicit dash lines (including the round-8 attempt breadcrumb) \
             even when .rigger/ broadly covers them, so the committed .gitignore stays \
             self-contained, got: {:?}"`
- `src/cli/setup.rs:3155-3155` `".rigger/dash.url"`
- `src/cli/setup.rs:3156-3156` `".rigger/dash.marker"`
- `src/cli/setup.rs:3157-3157` `".rigger/dash.attempt"`
- `src/cli/setup.rs:3158-3158` `"all three explicit per-file dash ignore lines are present in the committed \
             .gitignore even though .rigger/ already covers them, got:\n{content}"`
- `src/cli/setup.rs:3168-3168` `".rigger/dash.url"`
- `src/cli/setup.rs:3171-3171` `".rigger/dash.marker"`
- `src/cli/setup.rs:3174-3174` `".rigger/dash.attempt"`
- `src/cli/setup.rs:3527-3527` `".rigger/agents/researcher.md"`
- `src/cli/setup.rs:3556-3556` `".rigger/agents/planner.md"`
- `src/cli/setup.rs:3586-3586` `".rigger/agents/newcomer.md"`
- `src/cli/setup.rs:3664-3664` `".rigger/workflow.yml"`
- `src/cli/setup.rs:3977-3977` `".rigger"`
- `src/cli/setup.rs:4333-4333` `".rigger/agents/{persona}"`
- `src/cli/setup.rs:4389-4389` `".rigger/agents/{f}"`
- `src/cli/setup.rs:4473-4473` `".rigger/agents/planner.md"`
- `src/cli/setup.rs:4592-4592` `"`sh .rigger/gates/audit.sh`, then `git status --porcelain` empty"`
- `src/cli/setup.rs:4594-4594` `"`sh .rigger/gates/content.sh style`"`
- `src/cli/setup.rs:4595-4595` `"`sh .rigger/gates/content.sh no-os-kill`"`
- `src/cli/setup.rs:4596-4596` `"`sh .rigger/gates/red-before-green.sh`"`
- `src/cli/setup.rs:4609-4609` `"Once these checks have run, a later commit or an amend re-runs only what its new \
             diff can change: a diff that touches only comments, doc comments or Markdown \
             re-runs `cargo fmt --all --check`, the default-lane clippy line, `sh \
             .rigger/gates/audit.sh` with `git status --porcelain` empty and the root tests that \
             read a checked-in file as text; any other hunk re-runs the full list."`
- `src/cli/setup.rs:4622-4622` `"`src/`, `crates/`, `tests/`, `docs/`, `skills/`, `scaffold/` and `.rigger/`"`
- `src/cli/setup.rs:4625-4625` `"`git ls-files src crates tests docs skills scaffold .rigger | xargs awk"`
- `src/cli/setup.rs:4627-4627` `"`sh .rigger/gates/lanes.sh <lane>`"`
- `src/cli/validate.rs:907-907` `"warning: tracked .rigger/ files have uncommitted modifications:"`
- `src/cli/validate.rs:1396-1396` `" M .rigger/workflow.yml\n\
                         M  .rigger/agents/sdet.md\n\
                         A  .rigger/agents/new.md\n\
                         D  .rigger/agents/gone.md\n\
                         ?? .rigger/events.db\n\
                         !! .rigger/shim/node_modules\n"`
- `src/cli/validate.rs:1406-1406` `".rigger/workflow.yml"`
- `src/cli/validate.rs:1407-1407` `".rigger/agents/sdet.md"`
- `src/cli/validate.rs:1408-1408` `".rigger/agents/new.md"`
- `src/cli/validate.rs:1409-1409` `".rigger/agents/gone.md"`
- `src/cli/validate.rs:2341-2341` `".rigger"`
- `src/cli/validate.rs:2395-2395` `".rigger"`
- `src/main.rs:210-210` `"rigger - a config-driven, event-sourced multi-agent dev-loop harness\n\n\
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
rigger resume-unit <unit>   give an escalated unit of the current run more attempts\n            \
[--attempts <n>]  (default 1) on its same branch: the next `rigger step`\n                              \
re-parks its implementer there. Refuses, recording\n                              \
nothing, when the unit is not escalated or its branch\n                              \
is gone\n  \
rigger hold --reason <text> hold the current run for the operator, the text its\n                              \
reason: no probe ends this hold; `rigger release`, a\n                              \
new run, or `rigger run` reaching done does. Refuses,\n                              \
recording nothing, when no run is live\n  \
rigger release              end the current run's hold, whichever writer declared\n                              \
it; with no hold open it records nothing and says so\n  \
rigger reported <id>        exit 0 iff spawn <id> already has a recorded result in\n                              \
this project's run stream (else non-zero). A read-only check\n                              \
of whether a spawn reported yet; the death courier records\n                              \
atomically instead via `rigger result --if-absent`\n  \
rigger prompt <id>          print the parked spawn's full prompt (persona + task).\n                              \
The step wave is a slim manifest; each worker fetches its\n                              \
own prompt from the log by spawn id (spawn-by-reference);\n                              \
it refuses a spawn that already ended on a real result\n  \
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
(the text half of lookup; resolves a full id, a bare name or a path)\n  \
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
rigger status [--json|--line] present the live per-agent view of the current run: for\n                              \
each in-flight agent, what it is doing (latest progress),\n                              \
its heartbeat age, and how long since its last store event\n                              \
(the blackout). --json prints the shim/dash machine shape;\n                              \
--line prints only the one-line statusline (the same line\n                              \
`rigger setup` registers as the editor's status bar)\n  \
rigger dash [--port <n>]    serve the read-only observability page on 127.0.0.1\n                              \
(default port 7420) with live past/present/future views;\n                              \
--export <path> writes the equivalent static snapshot\n  \
rigger watch [--interval <s>] the driver-independent watchdog: polls the store,\n            \
[--once]                  process table, and status for the five rigger-watch-a-run\n                              \
signals (escalated blockers, heartbeat staleness, dash\n                              \
liveness, reject-recurrence trend, frontier progress) plus\n                              \
store integrity and a red CI check on the run branch's tip\n                              \
(read with gh; a CI gh cannot read is noted on stderr),\n                              \
printing one line per anomaly naming signal, subject,\n                              \
and response. --once prints standing\n                              \
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
(with --error, its failure message). It refuses to replace\n                              \
the result of a spawn that already ended unless\n                              \
--supersede (the explicit repair); a step's liveness\n                              \
fault is always replaceable. --if-absent records only\n                              \
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
DecisionMade before either mode prunes. When no driver is\n                              \
alive and every spawn of the run has ended on a real\n                              \
result, it also closes the current run's units whose\n                              \
branch work is landed on rigger-run, appending the\n                              \
UnitIntegrated a hand landing never recorded; a dead run's\n                              \
hung spawn (answered only by the step's liveness fault)\n                              \
keeps them open and is named with its remedy,\n                              \
`rigger result <id>`. It removes a stale graph.db.pruned,\n                              \
the pruned copy a rebuild's stopped swap\n                              \
left, unless another `rigger setup` or `rigger reset` holds graph.db.lock\n  \
rigger reset --derived      migrate the EVENT LOG, once: rewrite the first row of\n                              \
each file's latest derived batch into that generation's\n                              \
ledger entry, in place, delete every other derived\n                              \
event, and vacuum so the file shrinks on disk.\n                              \
Every other event survives;\n                              \
composes with --runs (each sheds its own accumulation).\n                              \
Refuses while the run is live (a held step lock, an\n                              \
in-flight spawn's marker inside its wall-clock bound,\n                              \
or a driver registration's heartbeat inside the idle\n                              \
window), naming what is live (a dead driver's run is\n                              \
not): the migration\n                              \
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
rigger reset --scratch-orphans\n                              \
reclaim every cache-home scratch root (under\n                              \
$XDG_CACHE_HOME/rigger, else ~/.cache/rigger) whose repo\n                              \
no longer exists: a root keyed on a deleted checkout or a\n                              \
test fixture's tempdir has no owner left to reclaim it.\n                              \
Rigger does this itself whenever it creates a default-\n                              \
placed root; this is the explicit on-demand form. Reports\n                              \
the number of roots reclaimed; composes with the others\n  \
rigger critique <spec>      critique the spec before any run: the workflow's critic\n                              \
(the plan-critique gate's adversary, else\n                              \
an adversary step in defaults.review) reads the spec\n                              \
text pass after pass, each recorded keyed on the\n                              \
text's content hash, until a pass adds no finding or\n                              \
its attempts reach the ceiling (the effective\n                              \
max_retries of the stage it is read from); the union\n                              \
of the passes is printed, and a text exhausted, or at\n                              \
the ceiling with a pass recorded, is answered from\n                              \
the record with no spawn; takes --eventstore/--conn\n                              \
as run does\n  \
rigger validate             load and validate the workflow + agents\n  \
rigger init                 set up a project: scaffold .rigger/ (workflow.yml +\n                              \
an agents/ folder) and install the Claude Code\n                              \
SessionStart hook (it runs `rigger prime`)\n  \
rigger setup                full setup: everything `init` does, PLUS install the\n                              \
native /rigger Claude Code workflow (.claude/workflows/\n                              \
rigger.js) and provision the JS driver (.rigger/shim/ +\n                              \
npm install). After it: run `/rigger <spec>` in Claude\n                              \
Code (primary), or `rigger workflow` as a fallback\n  \
rigger docs                 render the code-derived docs (every registry skill and\n                              \
handbook page) into their committed paths\n  \
rigger docs --check         check every rendered output against a fresh render,\n                              \
writing nothing: prints `fresh <path>` or `drifted <path>`\n                              \
per output on disk; exits 1 when any drifted\n  \
rigger prime [<spec>]       print recent decisions (what the hook runs); given a spec\n                              \
path, also names `rigger validate <spec>` (the pre-launch\n                              \
spec lint) as a next step\n  \
rigger instructions         print the instructions every spawned agent is held to:\n                              \
each built-in with its state (in force, replaced,\n                              \
switched off), then each unshadowed operator file in\n                              \
.rigger/instructions/ (filename order)\n  \
rigger version              print the crate version and the build-provenance id\n                              \
(a git commit/describe embedded at build time) so an\n                              \
agent can identify the exact binary. Also `--version`\n  \
rigger help [<command>]     print this text, or given a command, only its entry: the\n                              \
one `rigger <command> --help` prints for each command\n                              \
whose arguments are its own flags\n\n\
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
- `tests/a_module_row_reaches_its_file_periphery.rs:114-114` `"graph communities: detected 1 community over 2 coupled node(s) at resolution 1 \
         (2 membership event(s) recorded into .rigger/graph.db)\n"`
- `tests/architecture_current_surface.rs:93-93` `".rigger/store.conn"`
- `tests/architecture_current_surface.rs:145-145` `"docs/architecture.md must describe the system that exists today (spec 56, \
         criterion 1): it must name the store-resolution and configuration surface (the \
         committed `store:` selection, the `KURRENTDB_CONN` environment variable, and the \
         per-machine `.rigger/store.conn` secret file) and the graph inspector's real query \
         surface (the three `lens=` names and the directed `view=calls` `dir=` views). \
         Surfaces the document fails to name: {missing:#?}"`
- `tests/boundary_audit.rs:1304-1304` `"let b = \"sh .rigger/gates/mutation.sh\";\n"`
- `tests/canary_model_drift_periphery.rs:63-63` `".rigger"`
- `tests/change_path_revert_periphery.rs:83-83` `".rigger"`
- `tests/checkin_mutation_diff_base_periphery.rs:64-64` `".rigger"`
- `tests/checkpoint_commit_hook_bypass_periphery.rs:59-59` `"{repo_path}/.rigger-test-scratch"`
- `tests/ci_lanes.rs:35-35` `".rigger/gates/lanes.sh"`
- `tests/cli.rs:121-121` `".rigger"`
- `tests/cli.rs:125-125` `".rigger"`
- `tests/cli.rs:343-343` `".rigger"`
- `tests/cli.rs:833-833` `".rigger"`
- `tests/cli.rs:1084-1084` `".rigger"`
- `tests/cli.rs:1241-1241` `".rigger"`
- `tests/cli.rs:1262-1262` `".rigger"`
- `tests/cli.rs:1367-1367` `"/.rigger/"`
- `tests/cli.rs:1368-1368` `"the default must never live under any .rigger, even on the HOME-only fallback \
         rung; got: {stdout:?}"`
- `tests/cli.rs:1418-1418` `"{}/.rigger/tmp"`
- `tests/cli.rs:1430-1430` `"/.rigger/tmp/"`
- `tests/cli.rs:1522-1522` `".rigger"`
- `tests/cli.rs:1532-1532` `".rigger"`
- `tests/cli.rs:1597-1597` `".rigger"`
- `tests/cli.rs:1598-1598` `".rigger"`
- `tests/cli.rs:1610-1610` `".rigger"`
- `tests/cli.rs:1631-1631` `".rigger"`
- `tests/cli.rs:1685-1685` `".rigger"`
- `tests/cli.rs:1725-1725` `".rigger"`
- `tests/cli.rs:1745-1745` `".rigger"`
- `tests/cli.rs:1760-1760` `".rigger"`
- `tests/cli.rs:1876-1876` `".rigger"`
- `tests/cli.rs:2372-2372` `".rigger"`
- `tests/cli.rs:2840-2840` `".rigger"`
- `tests/cli.rs:2841-2841` `"graph build must create .rigger/graph.db even when there is nothing to ingest"`
- `tests/cli.rs:2864-2864` `".rigger"`
- `tests/cli.rs:2944-2944` `".rigger"`
- `tests/cli.rs:2959-2959` `".rigger"`
- `tests/cli.rs:3009-3009` `".rigger"`
- `tests/cli.rs:3034-3034` `".rigger"`
- `tests/cli.rs:3516-3516` `".rigger"`
- `tests/cli.rs:3520-3520` `"grounding via symbols must persist the structural index to .rigger/symbols/"`
- `tests/cli.rs:3692-3692` `".rigger"`
- `tests/cli.rs:4076-4076` `".rigger"`
- `tests/cli.rs:4114-4114` `".rigger"`
- `tests/cli.rs:4311-4311` `"cd ${REPO} && CARGO_TARGET_DIR=${REPO}/.rigger/tmp/cargo-target rigger step"`
- `tests/cli.rs:4359-4359` `"cd ${REPO} && CARGO_TARGET_DIR=${REPO}/.rigger/tmp/cargo-target rigger step"`
- `tests/cli.rs:4546-4546` `".rigger/tmp/agent-scratch/"`
- `tests/cli.rs:4552-4552` `".rigger/tmp/cargo-target"`
- `tests/cli.rs:4560-4560` `"CARGO_TARGET_DIR=${REPO}/.rigger/tmp/cargo-target rigger step"`
- `tests/cli.rs:5030-5030` `".rigger"`
- `tests/cli.rs:5132-5132` `".rigger"`
- `tests/cli.rs:5872-5872` `".rigger"`
- `tests/cli.rs:6072-6072` `".rigger"`
- `tests/cli.rs:6239-6239` `".rigger"`
- `tests/cli.rs:6466-6466` `".rigger"`
- `tests/cli.rs:6616-6616` `".rigger"`
- `tests/cli.rs:11108-11108` `".rigger/workflow.yml"`
- `tests/cli.rs:11589-11589` `".rigger/workflow.yml"`
- `tests/cli.rs:12248-12248` `".rigger/agents/sdet-author.md"`
- `tests/cli.rs:12249-12249` `".rigger/agents/sdet-author.md"`
- `tests/cli.rs:12585-12585` `".rigger"`
- `tests/cli.rs:12736-12736` `".rigger"`
- `tests/cli.rs:12945-12945` `".rigger"`
- `tests/cli.rs:13144-13144` `".rigger"`
- `tests/cli.rs:13232-13232` `".rigger"`
- `tests/cli.rs:13353-13353` `".rigger"`
- `tests/cli.rs:13546-13546` `".rigger/events.db"`
- `tests/cli.rs:14090-14090` `".rigger"`
- `tests/cli.rs:14166-14166` `".rigger"`
- `tests/cli.rs:14197-14197` `".rigger"`
- `tests/cli.rs:14647-14647` `"/.rigger/"`
- `tests/cli.rs:14648-14648` `"the default marker path must never live under any .rigger; got: {marker_str:?}"`
- `tests/cli.rs:14854-14854` `"/.rigger/"`
- `tests/cli.rs:14855-14855` `"the unbounded spawn's marker resolves under the scratch root's agent-live, never under \
         any .rigger; got: {marker_str:?}"`
- `tests/cli.rs:15086-15086` `"/.rigger/tmp/agent-live/"`
- `tests/cli.rs:15087-15087` `"under RIGGER_TMPDIR the marker is not under the repo's .rigger/tmp; got: {marker_str:?}"`
- `tests/cli.rs:15702-15702` `".rigger"`
- `tests/cli.rs:15974-15974` `".rigger"`
- `tests/cli.rs:16073-16073` `".rigger"`
- `tests/cli.rs:16162-16162` `".rigger"`
- `tests/cli.rs:16202-16202` `".rigger"`
- `tests/cli.rs:16430-16430` `"rigger: another `rigger step` is already running in this repo, or a \
     `rigger run` is (lock .rigger/step.lock). Refusing to run concurrently: two would race the \
     run-branch checkout and the unit worktrees branched off HEAD, corrupting the run. Wait for \
     the running one to finish and retry.\n"`
- `tests/cli.rs:16937-16937` `".rigger/workflow.yml"`
- `tests/cli.rs:17206-17206` `".rigger/workflow.yml"`
- `tests/cli.rs:18102-18102` `".rigger/workflow.yml"`
- `tests/cli.rs:18102-18102` `".rigger/agents"`
- `tests/cli.rs:18191-18191` `".rigger/workflow.yml"`
- `tests/cli.rs:18191-18191` `".rigger/agents"`
- `tests/cli.rs:18193-18193` `".rigger"`
- `tests/cli.rs:18194-18194` `".rigger/workflow.yml"`
- `tests/cli.rs:18399-18399` `".rigger/workflow.yml"`
- `tests/cli.rs:18399-18399` `".rigger/agents"`
- `tests/cli.rs:18446-18446` `".rigger/workflow.yml"`
- `tests/cli.rs:18446-18446` `".rigger/agents"`
- `tests/cli.rs:18459-18459` `".rigger"`
- `tests/cli.rs:18465-18465` `".rigger"`
- `tests/cli.rs:18467-18467` `".rigger"`
- `tests/cli.rs:18563-18563` `".rigger/workflow.yml"`
- `tests/cli.rs:18564-18564` `"validate must NOT flag a clean tracked `.rigger/` tree; stderr:\n{err}"`
- `tests/cli.rs:18573-18573` `".rigger"`
- `tests/cli.rs:18580-18580` `"validate must still succeed (exit 0) when it only FLAGS uncommitted `.rigger/` \
         changes; stderr:\n{err}"`
- `tests/cli.rs:18584-18584` `".rigger/workflow.yml"`
- `tests/cli.rs:18585-18585` `"validate must flag the tracked-but-modified `.rigger/workflow.yml` on stderr; \
         stderr:\n{err}"`
- `tests/cli.rs:18601-18601` `".rigger"`
- `tests/cli.rs:18875-18875` `".rigger"`
- `tests/cli.rs:19024-19024` `".rigger"`
- `tests/cli.rs:19060-19060` `".rigger"`
- `tests/cli.rs:19100-19100` `".rigger"`
- `tests/cli.rs:19323-19323` `".rigger"`
- `tests/cli.rs:19425-19425` `".rigger"`
- `tests/cli.rs:19569-19569` `".rigger"`
- `tests/cli.rs:19571-19571` `".rigger"`
- `tests/cli.rs:19573-19573` `".rigger"`
- `tests/cli.rs:19575-19575` `".rigger"`
- `tests/cli.rs:19603-19603` `"probe/.rigger/events.db"`
- `tests/cli.rs:19657-19657` `"validate must warn about residue planted under the relocated cache-home DEFAULT \
         root - a regression that left its residue scan still rooted at the pre-relocation \
         `.rigger/tmp` would silently miss this and print nothing; stderr:\n{err}"`
- `tests/cli.rs:19682-19682` `".rigger"`
- `tests/cli.rs:20127-20127` `".rigger"`
- `tests/cli.rs:20135-20135` `".rigger"`
- `tests/cli.rs:20176-20176` `"\n  shadow store: probe/.rigger/events.db (2B)"`
- `tests/cli.rs:20526-20526` `".rigger"`
- `tests/cli.rs:20623-20623` `"scaffolded .rigger/workflow.yml"`
- `tests/cli.rs:20627-20627` `"scaffolded .rigger/agents/"`
- `tests/cli.rs:20658-20658` `".rigger"`
- `tests/cli.rs:20690-20690` `".rigger/agents/researcher.md"`
- `tests/cli.rs:20691-20691` `"the agent was actually imported into .rigger/agents/"`
- `tests/cli.rs:20737-20737` `".rigger"`
- `tests/cli.rs:20910-20910` `".rigger/dash.url"`
- `tests/cli.rs:20914-20914` `".rigger/dash.marker"`
- `tests/cli.rs:20918-20918` `".rigger/dash.attempt"`
- `tests/cli.rs:20927-20927` `".rigger"`
- `tests/cli.rs:20929-20929` `".rigger"`
- `tests/cli.rs:20933-20933` `".rigger"`
- `tests/cli.rs:20934-20934` `".rigger"`
- `tests/cli.rs:20936-20936` `".rigger/dash.url"`
- `tests/cli.rs:20937-20937` `".rigger/dash.marker"`
- `tests/cli.rs:20938-20938` `".rigger/dash.attempt"`
- `tests/cli.rs:20975-20975` `".claude/\n.rigger/\n"`
- `tests/cli.rs:20991-20991` `".rigger/dash.url"`
- `tests/cli.rs:20999-20999` `"the test's global config must actually ignore .rigger/dash.url (else the regression \
         guard is inconclusive)"`
- `tests/cli.rs:21020-21020` `".rigger/shim"`
- `tests/cli.rs:21021-21021` `".rigger/dash.url"`
- `tests/cli.rs:21022-21022` `".rigger/dash.marker"`
- `tests/cli.rs:21023-21023` `".rigger/dash.attempt"`
- `tests/cli.rs:21155-21155` `".rigger/project.id"`
- `tests/cli.rs:21158-21158` `".rigger/project.id"`
- `tests/cli.rs:21171-21171` `".rigger/project.id"`
- `tests/cli.rs:21309-21309` `".rigger/project.id"`
- `tests/cli.rs:21328-21328` `".rigger"`
- `tests/cli.rs:21556-21556` `".rigger"`
- `tests/cli.rs:22514-22514` `".rigger"`
- `tests/cli.rs:22643-22643` `".rigger"`
- `tests/cli.rs:22790-22790` `".rigger"`
- `tests/cli.rs:23147-23147` `".rigger"`
- `tests/cli.rs:23191-23191` `".rigger"`
- `tests/cli.rs:23631-23631` `".rigger"`
- `tests/cli.rs:23646-23646` `"the driver never recorded a dash URL in .rigger/dash.url; stderr:\n{err}"`
- `tests/cli.rs:23706-23706` `".rigger"`
- `tests/cli.rs:24049-24049` `".rigger"`
- `tests/cli.rs:24873-24873` `".rigger"`
- `tests/cli.rs:24875-24875` `".rigger"`
- `tests/cli.rs:24935-24935` `".rigger/docs-overlay.yml"`
- `tests/cli.rs:25057-25057` `".rigger/shim/node_modules/.package-lock.json"`
- `tests/cli.rs:25078-25078` `".rigger/skills/using-rigger/SKILL.md"`
- `tests/cli.rs:25079-25079` `".rigger/skills/using-riggers/SKILL.md"`
- `tests/cli.rs:25082-25082` `".rigger/skills/planning-a-spec"`
- `tests/cli.rs:25088-25088` `".rigger/skills/planning-a-spec"`
- `tests/cli.rs:25089-25089` `".rigger/skills/using-riggers"`
- `tests/cli.rs:25094-25094` `"{stray} shadows no shipped skill and installs nothing: a project copy is \
                 .rigger/skills/<name>/SKILL.md, <name> a shipped skill's exact name"`
- `tests/cli.rs:25103-25103` `".rigger/skills/"`
- `tests/cli.rs:25171-25171` `".rigger/skills/using-rigger/SKILL.md"`
- `tests/cli.rs:25253-25253` `".rigger/skills/using-rigger"`
- `tests/cli.rs:25296-25296` `".rigger/skills"`
- `tests/cli.rs:26130-26130` `".rigger/dash.marker"`
- `tests/cli.rs:26188-26188` `".rigger/dash.url"`
- `tests/cli.rs:26196-26196` `".rigger/dash.marker"`
- `tests/cli.rs:26418-26418` `".rigger/dash.url"`
- `tests/cli.rs:26420-26420` `".rigger/dash.marker"`
- `tests/cli.rs:26491-26491` `".rigger/dash.marker"`
- `tests/cli.rs:26498-26498` `".rigger/dash.attempt"`
- `tests/cli.rs:27526-27526` `".rigger"`
- `tests/cli.rs:28163-28163` `".rigger"`
- `tests/cli.rs:28489-28489` `".rigger"`
- `tests/cli.rs:28524-28524` `".rigger"`
- `tests/cli.rs:28599-28599` `"the first step must record a dash marker at .rigger/dash.marker; stderr:\n{err1}"`
- `tests/cli.rs:28680-28680` `"the step must record a dash marker at .rigger/dash.marker; stderr:\n{err}"`
- `tests/cli.rs:28694-28694` `"`rigger step` under RIGGER_DASH_PORT={dash_port} must bind its step-path dash at EXACTLY \
         that port and record it in .rigger/dash.marker (proving the override reaches the real \
         bind, not the fixed dash::DEFAULT_PORT); the marker instead recorded {marker_port}"`
- `tests/cli.rs:28755-28755` `".rigger"`
- `tests/cli.rs:28839-28839` `".rigger"`
- `tests/cli.rs:29224-29224` `".rigger"`
- `tests/cli.rs:29236-29236` `".rigger"`
- `tests/cli.rs:29263-29263` `".rigger"`
- `tests/cli.rs:29291-29291` `".rigger"`
- `tests/cli.rs:29302-29302` `".rigger"`
- `tests/cli.rs:29339-29339` `".rigger"`
- `tests/cli.rs:29428-29428` `"the first step must record a dash marker at .rigger/dash.marker; stderr:\n{err1}"`
- `tests/cli.rs:29497-29497` `"{root}/.rigger/events.db"`
- `tests/cli.rs:30110-30110` `"{other_root}/.rigger/events.db"`
- `tests/cli.rs:30497-30497` `"/stale/root/.rigger/events.db"`
- `tests/cli.rs:30519-30519` `"/live/root/.rigger/events.db"`
- `tests/cli.rs:30602-30602` `"the step must record a dash marker at .rigger/dash.marker"`
- `tests/cli.rs:30802-30802` `".rigger"`
- `tests/cli.rs:30985-30985` `".rigger"`
- `tests/cli.rs:31089-31089` `".rigger"`
- `tests/cli.rs:31215-31215` `".rigger"`
- `tests/cli.rs:31895-31895` `".rigger/gates"`
- `tests/cli.rs:31901-31901` `".rigger/gates/{name}"`
- `tests/cli.rs:31942-31942` `"sh .rigger/gates/content.sh {check} \"$RIGGER_RUN_BASE\""`
- `tests/cli.rs:31949-31949` `"sh .rigger/gates/content.sh {check}"`
- `tests/cli.rs:31997-31997` `".rigger/dash.attempt"`
- `tests/cli.rs:31998-31998` `"a real step's own ensure_run_dashboard call must record .rigger/dash.attempt \
         (record_dash_attempt); without it this test cannot exercise the round-8 fact at all"`
- `tests/cli.rs:32073-32073` `".rigger/dash.marker"`
- `tests/cli.rs:32102-32102` `".rigger/dash.url"`
- `tests/cli.rs:32109-32109` `".rigger/dash.attempt"`
- `tests/cli.rs:32169-32169` `".rigger/dash.marker"`
- `tests/cli.rs:32172-32172` `".rigger/dash.url"`
- `tests/cli.rs:32225-32225` `".rigger/dash.marker"`
- `tests/cli.rs:32247-32247` `".rigger/dash.url"`
- `tests/cli.rs:32252-32252` `".rigger/dash.attempt"`
- `tests/cli.rs:32290-32290` `".rigger/dash.url"`
- `tests/cli.rs:32301-32301` `".rigger/dash.marker"`
- `tests/cli.rs:32781-32781` `".rigger"`
- `tests/cli.rs:32934-32934` `".rigger"`
- `tests/cli.rs:33653-33653` `".rigger"`
- `tests/cli.rs:33922-33922` `".rigger"`
- `tests/cli.rs:34013-34013` `".rigger"`
- `tests/cli.rs:34249-34249` `".rigger"`
- `tests/cli.rs:34702-34702` `"the hook must be inert on a project without .rigger/; got:\n{out}"`
- `tests/cli.rs:34708-34708` `".rigger"`
- `tests/cli.rs:34727-34727` `".rigger"`
- `tests/cli.rs:34753-34753` `".rigger"`
- `tests/cli.rs:34789-34789` `".rigger"`
- `tests/cli.rs:34828-34828` `".rigger"`
- `tests/cli.rs:35020-35020` `".rigger"`
- `tests/cli.rs:35053-35053` `".rigger"`
- `tests/cli.rs:35796-35796` `".rigger"`
- `tests/cli.rs:36622-36622` `"scaffolded .rigger/instructions/README.md"`
- `tests/cli.rs:36685-36685` `"gw/.rigger/workflow.yml"`
- `tests/cli.rs:36723-36723` `"gw/.rigger/workflow.yml"`
- `tests/cli.rs:37186-37186` `"gw/.rigger/workflow.yml"`
- `tests/cli.rs:37204-37204` `"gw/.rigger/workflow.yml"`
- `tests/common/cli.rs:102-102` `".rigger"`
- `tests/common/cli.rs:699-699` `".rigger"`
- `tests/common/cli.rs:843-843` `".rigger"`
- `tests/common/cli.rs:854-854` `".rigger"`
- `tests/common/cli.rs:888-888` `".rigger"`
- `tests/common/cli.rs:932-932` `".rigger"`
- `tests/common/cli.rs:1192-1192` `".rigger/events.db"`
- `tests/common/cli.rs:1880-1880` `"reset --derived: the migration rewrites and deletes rows of the event log and vacuums the \
         file, which is a mechanic of the embedded .rigger/events.db store; this project is \
         configured for the server-backed store, where the migration does not run. Re-run it \
         against a project on the sqlite backend. Refusing rather than reporting a migration that \
         did not happen."`
- `tests/common/cli.rs:1971-1971` `"graph build: ingested {events} code-ingest event(s) into .rigger/graph.db{lost}\n"`
- `tests/common/cli.rs:2193-2193` `".rigger"`
- `tests/common/cli.rs:2194-2194` `"create .rigger/agents"`
- `tests/common/cli.rs:2329-2329` `"{why}: a server selection must NOT fabricate a local .rigger/events.db"`
- `tests/common/cli.rs:2426-2426` `".rigger"`
- `tests/common/fixtures/config.rs:528-528` `"{repo_path}/.rigger-test-scratch"`
- `tests/common/fixtures/extraction_tree.rs:76-76` `".rigger/workflow.yml"`
- `tests/common/fixtures/instructions.rs:335-335` `"\nOperator (.rigger/instructions/*.md, filename order):\n"`
- `tests/common/fixtures/instructions.rs:398-398` `"\n## communication (replaced by .rigger/instructions/communication.md)\n{printed}\n"`
- `tests/common/fixtures/instructions.rs:425-425` `"{}\n\nBuilt-in:\n\n## engineering-principles (built-in)\n{}\n\n## working-discipline \
         (switched off by .rigger/instructions/working-discipline.md)\n{communication}\
         {RENDERED_OPERATOR_HEADING}\n## 30-style\n{UNSHADOWING_STYLE_BODY}\n{tail}"`
- `tests/common/fixtures/instructions.rs:456-456` `"warning: built-in instruction working-discipline is switched off by \
     .rigger/instructions/working-discipline.md"`
- `tests/common/fixtures/instructions.rs:458-458` `"warning: built-in instruction communication is replaced by \
     .rigger/instructions/communication.md"`
- `tests/common/fixtures/liveness.rs:7-7` `"a `rigger step` or `rigger run` is running right now (it holds .rigger/step.lock)"`
- `tests/common/layer_cli.rs:28-28` `".rigger"`
- `tests/common/layer_cli.rs:301-301` `".rigger"`
- `tests/common/mod.rs:328-328` `"{}/.rigger-test-scratch"`
- `tests/common/workflow_probe.rs:15-15` `".rigger"`
- `tests/common/workflow_probe.rs:16-16` `"create .rigger"`
- `tests/compaction_generations_periphery.rs:1541-1541` `".rigger"`
- `tests/compaction_generations_periphery.rs:3887-3887` `"rigger: migrated project identity - renamed 1 stream(s) from the legacy namespace \
         {legacy:?} to the minted identity {minted:?} (.rigger/project.id); recorded its \
         decision (position {position}){fold}\n"`
- `tests/config_unknown_key_dotted_path_periphery.rs:138-138` `".rigger"`
- `tests/config_unknown_key_dotted_path_periphery.rs:139-139` `"write the .rigger/{rel} fixture: {e}"`
- `tests/config_unknown_key_dotted_path_periphery.rs:149-149` `".rigger"`
- `tests/config_unknown_key_dotted_path_periphery.rs:423-423` `".rigger"`
- `tests/courier_registry_refresh_boundary_periphery.rs:83-83` `".rigger"`
- `tests/courier_registry_refresh_fence_periphery.rs:51-51` `".rigger"`
- `tests/critique_method_persona_periphery.rs:205-205` `".rigger/agents/planner.md"`
- `tests/dedup_seeding_periphery.rs:342-342` `".rigger"`
- `tests/dedup_seeding_periphery.rs:366-366` `".rigger"`
- `tests/dedup_seeding_periphery.rs:570-570` `".rigger"`
- `tests/duplication_catalog_contract_periphery.rs:102-102` `".rigger-path string literals"`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:123-123` `".rigger"`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:159-159` `".rigger"`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:279-279` `".rigger"`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:573-573` `".rigger"`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:1031-1031` `".rigger"`
- `tests/gate_store_fence_periphery.rs:208-208` `".rigger"`
- `tests/gate_store_fence_periphery.rs:209-209` `".rigger"`
- `tests/gate_store_fence_periphery.rs:214-214` `".rigger"`
- `tests/gate_store_fence_periphery.rs:215-215` `".rigger"`
- `tests/gate_store_fence_periphery.rs:217-217` `".rigger"`
- `tests/gate_store_fence_periphery.rs:222-222` `".rigger"`
- `tests/gate_store_fence_periphery.rs:273-273` `".rigger"`
- `tests/gate_store_fence_periphery.rs:274-274` `".rigger"`
- `tests/gate_store_fence_periphery.rs:281-281` `".rigger"`
- `tests/gate_store_fence_periphery.rs:284-284` `".rigger"`
- `tests/gate_store_fence_periphery.rs:566-566` `".rigger"`
- `tests/gate_store_fence_periphery.rs:589-589` `".rigger"`
- `tests/gate_store_fence_periphery.rs:702-702` `".rigger"`
- `tests/gate_store_fence_periphery.rs:703-703` `".rigger"`
- `tests/gate_store_fence_periphery.rs:713-713` `".rigger"`
- `tests/gate_store_fence_periphery.rs:715-715` `".rigger"`
- `tests/gate_store_fence_periphery.rs:820-820` `".rigger"`
- `tests/gate_store_fence_periphery.rs:821-821` `".rigger"`
- `tests/graph_around_code_first.rs:60-60` `".rigger"`
- `tests/graph_around_governance_boundaries.rs:51-51` `".rigger"`
- `tests/graph_around_governance_boundaries.rs:82-82` `".rigger"`
- `tests/graph_around_governance_boundaries.rs:126-126` `".rigger"`
- `tests/graph_around_governance_boundaries.rs:145-145` `".rigger"`
- `tests/graph_around_governance_boundaries.rs:199-199` `".rigger"`
- `tests/graph_show_surface.rs:79-79` `".rigger"`
- `tests/halted_spawn_wip_recovery_periphery.rs:602-602` `".rigger"`
- `tests/handbook_grounder_accuracy.rs:142-142` `".rigger"`
- `tests/handbook_grounder_accuracy.rs:151-151` `".rigger/workflow.yml must carry a `grounder:` default (spec 57, criterion 4)"`
- `tests/handbook_grounder_accuracy.rs:156-156` `"the repo's own .rigger/workflow.yml must default to the structural `symbols` grounder \
         (spec 57): the vector engine `turbovec` and its `hybrid` composite were retired. Found: \
         grounder: {workflow_value:?}"`
- `tests/handbook_grounder_accuracy.rs:162-162` `"docs/handbook/authoring-loops.md claims its example reproduces the repo's own \
         .rigger/workflow.yml, so its `grounder:` value ({handbook_value:?}) must equal the \
         committed workflow's ({workflow_value:?}). An operator copies this block verbatim - a \
         stale `grounder: turbovec` here pastes the retired engine and hits the loud \
         retired-grounder migration error instead of a working run."`
- `tests/heartbeat_write_read_agree_periphery.rs:129-129` `".rigger"`
- `tests/heartbeat_write_read_agree_periphery.rs:225-225` `".rigger"`
- `tests/heartbeat_write_read_agree_periphery.rs:230-230` `".rigger"`
- `tests/init_setup_unknown_key_agent_fleet_periphery.rs:55-55` `".rigger"`
- `tests/init_setup_unknown_key_agent_fleet_periphery.rs:56-56` `"create .rigger"`
- `tests/init_setup_unknown_key_agent_fleet_periphery.rs:93-93` `".rigger"`
- `tests/init_setup_unknown_key_agent_fleet_periphery.rs:95-95` `"create .rigger/agents"`
- `tests/init_template_sets_periphery.rs:14-14` `"scaffolded .rigger/workflow.yml (gate template set: rust)"`
- `tests/init_template_sets_periphery.rs:17-17` `"scaffolded .rigger/workflow.yml"`
- `tests/init_template_sets_periphery.rs:20-20` `"no gate template set matches this project (rust: Cargo.toml at the \
                           project root), so .rigger/workflow.yml declares no gates - declare \
                           your own under gates:"`
- `tests/init_template_sets_periphery.rs:26-26` `"scaffolded .rigger/gates/red-before-green.sh"`
- `tests/init_template_sets_periphery.rs:27-27` `"scaffolded .rigger/gates/mutation.sh"`
- `tests/init_template_sets_periphery.rs:28-28` `"scaffolded .rigger/gates/container-env.sh"`
- `tests/init_template_sets_periphery.rs:41-41` `"scaffolded .rigger/gates/"`
- `tests/init_template_sets_periphery.rs:74-74` `".rigger/gates"`
- `tests/init_template_sets_periphery.rs:103-103` `".rigger/workflow.yml"`
- `tests/init_template_sets_periphery.rs:124-124` `"sh .rigger/gates/red-before-green.sh"`
- `tests/init_template_sets_periphery.rs:168-168` `".rigger/gates/red-before-green.sh"`
- `tests/init_template_sets_periphery.rs:178-178` `".rigger/gates/mutation.sh"`
- `tests/init_template_sets_periphery.rs:182-182` `"scaffolded .rigger/gates/mutation.sh\n"`
- `tests/init_template_sets_periphery.rs:205-205` `".rigger"`
- `tests/init_template_sets_periphery.rs:284-284` `".rigger/workflow.yml"`
- `tests/integrate_conflict_merge_periphery.rs:475-475` `"the project's own .rigger/workflow.yml must load through the real loader"`
- `tests/ledger_rebuild.rs:457-457` `"gw/.rigger/workflow.yml"`
- `tests/ledger_rebuild.rs:650-650` `"gw/.rigger/workflow.yml"`
- `tests/ledger_rebuild.rs:1238-1238` `"gw/.rigger/workflow.yml"`
- `tests/ledger_rebuild.rs:2380-2380` `"gw/.rigger/workflow.yml"`
- `tests/ledger_rebuild.rs:2410-2410` `"gw/.rigger/workflow.yml"`
- `tests/migration_is_deliberate_periphery.rs:490-490` `".rigger"`
- `tests/migration_is_deliberate_periphery.rs:537-537` `".rigger"`
- `tests/principle_gates_wiring.rs:50-50` `"the scaffolded .rigger/agents: {missing:#?}"`
- `tests/principle_gates_wiring.rs:64-64` `".rigger/gates/red-before-green.sh"`
- `tests/principle_gates_wiring.rs:65-65` `".rigger/gates/mutation.sh"`
- `tests/principle_gates_wiring.rs:66-66` `".rigger/gates/container-env.sh"`
- `tests/principle_gates_wiring.rs:126-126` `".rigger/gates"`
- `tests/principle_gates_wiring.rs:224-224` `".rigger/workflow.yml"`
- `tests/principle_gates_wiring.rs:225-225` `".rigger/gates"`
- `tests/principle_gates_wiring.rs:385-385` `".rigger/agents/sdet-author.md"`
- `tests/principle_gates_wiring.rs:409-409` `".rigger/agents"`
- `tests/principle_gates_wiring.rs:419-419` `".rigger/agents: {missing:#?}"`
- `tests/principle_gates_wiring.rs:423-423` `".rigger/gates/container-env.sh"`
- `tests/projections_stay_local.rs:93-93` `"the graph projection must be opened for write at the resolved store's .rigger/graph.db \
         (`open_graph_for_write(&self.file(\"graph.db\") ...)`); the canonical local construction \
         is gone"`
- `tests/projections_stay_local.rs:108-108` `"the progress projection must be opened by the LOCAL sqlite Store at .rigger/progress.db \
         (`Store::open(&db_path(\"progress.db\") ...)`); the canonical local construction is gone"`
- `tests/projections_stay_local.rs:159-159` `".rigger"`
- `tests/projections_stay_local.rs:162-162` `"graph.db must be created under the LOCAL .rigger/ even when the event store is the \
             server - projections are per-machine and stay local"`
- `tests/projections_stay_local.rs:172-172` `".rigger"`
- `tests/projections_stay_local.rs:173-173` `"a server-configured `graph build` must NOT create a local .rigger/events.db - the \
             event log is the server's; only the projection is local"`
- `tests/projections_stay_local.rs:216-216` `".rigger"`
- `tests/projections_stay_local.rs:219-219` `"progress.db must be created under the LOCAL .rigger/ even when the event store is the \
             server - the progress store is a local projection, not the shared log"`
- `tests/projections_stay_local.rs:242-242` `".rigger"`
- `tests/projections_stay_local.rs:243-243` `"a server-configured `rigger progress` must NOT create a local .rigger/events.db - the \
             run log is the server's; only the progress projection is local"`
- `tests/published_content_key_split_periphery.rs:259-259` `".rigger"`
- `tests/registry_periphery.rs:55-55` `r#"{
  "project": "acme",
  "root": "/home/dev/acme",
  "store": {
    "kind": "local",
    "path": "/home/dev/acme/.rigger/events.db"
  },
  "heartbeat_ms": 1000
}"#`
- `tests/registry_periphery.rs:94-94` `"/home/dev/acme/.rigger/events.db"`
- `tests/registry_periphery.rs:119-119` `"/home/dev/acme/.rigger/events.db"`
- `tests/registry_periphery.rs:186-186` `"/live/.rigger/events.db"`
- `tests/registry_periphery.rs:193-193` `"/dead/.rigger/events.db"`
- `tests/registry_periphery.rs:274-274` `"/home/dev/proj/.rigger/events.db"`
- `tests/relocated_worktree_store_resolution_periphery.rs:57-57` `".rigger"`
- `tests/relocated_worktree_store_resolution_periphery.rs:58-58` `"create .rigger"`
- `tests/relocated_worktree_store_resolution_periphery.rs:103-103` `".rigger"`
- `tests/relocated_worktree_store_resolution_periphery.rs:103-103` `"create the worktree's .rigger"`
- `tests/relocated_worktree_store_resolution_periphery.rs:105-105` `".rigger"`
- `tests/relocated_worktree_store_resolution_periphery.rs:176-176` `".rigger"`
- `tests/relocated_worktree_store_resolution_periphery.rs:219-219` `".rigger"`
- `tests/relocated_worktree_store_resolution_periphery.rs:269-269` `".rigger"`
- `tests/reset_build_cache_periphery.rs:440-440` `".rigger"`
- `tests/reset_derived_compaction_periphery.rs:609-609` `"the partition holding .rigger/ needs about the compacted size of the log free"`
- `tests/reset_derived_compaction_periphery.rs:1022-1022` `".rigger"`
- `tests/reset_derived_compaction_periphery.rs:1022-1022` `"create .rigger"`
- `tests/reset_derived_compaction_periphery.rs:1082-1082` `".rigger"`
- `tests/reset_derived_live_writer_guard_periphery.rs:76-76` `".rigger"`
- `tests/reset_derived_live_writer_guard_periphery.rs:1321-1321` `".rigger"`
- `tests/reset_derived_live_writer_guard_periphery.rs:1428-1428` `".rigger"`
- `tests/review_round_lenses_only_log_derived_resume_periphery.rs:147-147` `"{repo_path}/.rigger-test-scratch"`
- `tests/review_round_no_adjudicator_residue_periphery.rs:95-95` `"{repo_path}/.rigger-test-scratch"`
- `tests/review_round_non_ancestor_residue_names_true_diff_periphery.rs:121-121` `"{repo_path}/.rigger-test-scratch"`
- `tests/simplification_audit.rs:2803-2803` `".rigger-path string literals"`
- `tests/simplification_audit.rs:2988-2988` `".rigger"`
- `tests/simplification_audit.rs:2989-2989` `"one .rigger-relative path-composition helper"`
- `tests/simplification_audit.rs:5104-5104` `"Largest risk-reduction first is read as six tiers, ranked by the KIND of risk \
        each entry retires, highest first:\n\n\
        1. Tier 1 - active correctness risk, PLUS item 0: a use case already depends on the \
        wrong concretion, or two independent implementations of one concern can already \
        drift apart silently (section 3's boundary violation; the one already-drifted \
        `/proc`-reading pair section 2 and section 3 both name) - live gaps, not just size. \
        Item 0 (deleting the dead-code ledger, section 4.3) is placed here too, first of \
        all: not a live-gap risk itself, but the cheapest, zero-behavior-change move \
        available, and it shrinks the files tiers 2 and 3 operate on before either touches \
        them.\n\
        2. Tier 2 - god-file test-module extraction: each of the three god files' own \
        inline `#[cfg(test)] mod tests` holds {tier2_lo}-{tier2_hi}% of that file's mapped \
        functions (section 1's map), and moving it is a pure \
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
        6. Tier 6 - remaining catalog sweep: the {remaining} src-touching clusters section 2 \
        found but tiers 1 and 4 did not individually name. Unlike every other tier, none of \
        these {remaining} have been read and risk-assessed one at a time the way tiers 1-4's named \
        clusters have - they are consumed straight from the catalog - so this tier carries \
        production-correctness exposure tiers 2, 3 and 5 do not, and is ordered last: the \
        follow-up spec must triage each cluster's own production-or-test status before \
        merging it, not assume tier 5's blanket test-only treatment applies here too.\n\n\
        Within a tier, entries are ordered largest-first by the site or line count each \
        retires - the same rule the tiers themselves follow, applied one level down.\n\n"`
- `tests/simplification_audit.rs:5385-5385` `"#### 9. Consolidate the {rigger_n} `.rigger`-path string-literal sites (`{rigger_id}`) - the \
        single largest cluster in the entire catalog by site count\n\n"`
- `tests/simplification_audit.rs:5389-5389` `"- Scope: one `.rigger`-relative path-composition helper (the cluster's own \
        `proposed_home`) every one of the {rigger_n} sites routes through instead of building its \
        own literal.\n\
        - Files: spans dozens of files including `crates/rigger-conductor/src/conductor.rs`, `crates/rigger-config-files/src/config_store.rs`, \
        `crates/rigger-dash/src/dash.rs`, `crates/rigger-domain/src/docs.rs`, `crates/rigger-gates-shell/src/gate.rs`, `crates/rigger-grounder/src/grounder/mod.rs`, \
        `crates/rigger-grounder/src/grounder/symbols/store.rs`, `crates/rigger-grounder/src/ingest.rs`, `src/main.rs`, `crates/rigger-process/src/reap.rs`, \
        `crates/rigger-store-sqlite/src/registry.rs`, `src/worktree.rs` plus many `tests/` files - the full site list \
        is in the committed `docs/audit/duplication-catalog.json` under `{rigger_id}` for the \
        follow-up spec to consume directly, not re-enumerated here.\n\
        - Expected line delta: negative - {rigger_n} literal compositions collapse toward one \
        helper's call sites; the helper itself is small.\n\
        - Risk: medium - the largest surface-area sweep in this plan by site count, even \
        though each individual site is trivial; needs a mechanical rewrite pass plus a \
        full-suite green run, not hand-editing {rigger_n} sites.\n\
        - Unblocks: the biggest single site-count reduction available anywhere in the \
        duplication catalog.\n\n"`
- `tests/simplification_audit.rs:8468-8468` `"fn a() {\n    let _ = \".rigger/tmp\";\n}\n"`
- `tests/simplification_audit.rs:8470-8470` `".rigger"`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:54-54` `".rigger"`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:189-189` `".rigger"`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:200-200` `".rigger"`
- `tests/spec_critique_periphery.rs:507-507` `".rigger/workflow.yml"`
- `tests/statusline_command_periphery.rs:122-122` `".rigger"`
- `tests/step_sheds_the_freshen.rs:67-67` `".rigger/grounding"`
- `tests/step_sheds_the_freshen.rs:216-216` `".rigger/symbols/index.json"`
- `tests/step_sheds_the_freshen.rs:217-217` `"the surviving persisted index is the SYMBOL index under .rigger/symbols/; got {}"`
- `tests/step_sheds_the_freshen.rs:282-282` `"constructing the default grounder builds and persists the SYMBOL index under \
         .rigger/symbols/ - the freshen's real target"`
- `tests/store_content_identity_periphery.rs:840-840` `".rigger"`
- `tests/store_content_identity_periphery.rs:841-841` `"create .rigger"`
- `tests/store_content_identity_periphery.rs:888-888` `".rigger"`
- `tests/store_precedence.rs:49-49` `".rigger"`
- `tests/store_precedence.rs:94-94` `".rigger"`
- `tests/store_precedence.rs:106-106` `".rigger"`
- `tests/store_precedence.rs:118-118` `".rigger"`
- `tests/store_precedence.rs:123-123` `".rigger/store.conn beats the committed store: config"`
- `tests/store_precedence.rs:133-133` `".rigger"`
- `tests/store_precedence.rs:171-171` `"a loud failure for {what} must leave no fabricated local .rigger/events.db behind"`
- `tests/store_precedence.rs:204-204` `".rigger"`
- `tests/store_precedence.rs:221-221` `".rigger"`
- `tests/store_resolution.rs:157-157` `".rigger"`
- `tests/store_resolution.rs:189-189` `".rigger"`
- `tests/store_resolution.rs:190-190` `"a server-configured courier must NOT create a local .rigger/events.db - that is the \
             state-fracture this criterion closes"`
- `tests/store_resolution.rs:243-243` `".rigger"`
- `tests/store_resolution.rs:276-276` `".rigger"`
- `tests/store_resolution.rs:280-280` `".rigger"`
- `tests/store_resolution_cli.rs:87-87` `"a server-configured courier must NOT create a local .rigger/events.db - that is the \
         state-fracture this criterion closes, and it must hold even when the server is down"`
- `tests/store_secrets.rs:55-55` `".rigger"`
- `tests/store_secrets.rs:105-105` `"{why}: a server-configured courier must NOT create a local .rigger/events.db: {stderr}"`
- `tests/store_secrets.rs:129-129` `".rigger/store.conn secret-file channel"`
- `tests/store_secrets.rs:145-145` `".rigger"`
- `tests/validate_advisories.rs:249-249` `".rigger"`
- `tests/validate_advisories.rs:608-608` `".rigger"`
- `tests/validate_footprint_default_scratch_root_periphery.rs:34-34` `".rigger"`
- `tests/validate_footprint_default_scratch_root_periphery.rs:35-35` `".rigger"`
- `tests/validate_footprint_default_scratch_root_periphery.rs:70-70` `".rigger"`
- `tests/watchdog_cli_periphery.rs:151-151` `".rigger"`
- `tests/watchdog_cli_periphery.rs:340-340` `".rigger"`
- `tests/workflow_definition_and_js_constants_periphery.rs:91-91` `".rigger"`
- `tests/workflow_definition_and_js_constants_periphery.rs:93-93` `".rigger"`
- `tests/workflow_definition_and_js_constants_periphery.rs:355-355` `".rigger"`
- `tests/workflow_definition_and_js_constants_periphery.rs:357-357` `".rigger"`
- `tests/workflow_definition_and_js_constants_periphery.rs:596-596` `".rigger"`
- `tests/workflow_driver_resolved_model_periphery.rs:106-106` `".rigger"`
- `tests/worktree_liveness_fence_periphery.rs:110-110` `".rigger"`

#### `dup-97f834a327e5` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-conductor/src/conductor.rs, tests/postmerge_gate_error_cleanup_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:36299-36314` `spawn`
- `tests/postmerge_gate_error_cleanup_periphery.rs:72-83` `spawn`

#### `dup-9d4ce6bab62d` (near, 2 sites)

Proposed home: `conductor::capped_reads`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:40060-40068` `read_stream`
- `crates/rigger-conductor/src/conductor.rs:40111-40119` `read_stream_typed`

#### `dup-05f8d05060a7` (exact, 2 sites)

Proposed home: `conductor::capped_reads`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:40104-40110` `last_position`
- `crates/rigger-conductor/src/conductor.rs:40141-40148` `latest_in_group`

#### `dup-15a59608df13` (near, 4 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-conductor/src/conductor.rs, tests/revert_on_base_hook_bypass_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:44812-44856` `spawn`
- `crates/rigger-conductor/src/conductor.rs:45273-45312` `spawn`
- `crates/rigger-conductor/src/conductor.rs:45424-45471` `spawn`
- `tests/revert_on_base_hook_bypass_periphery.rs:79-114` `spawn`

#### `dup-b91a09b322bf` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-conductor/src/conductor.rs, tests/replan_episode_identity.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:49813-49823` `critique_cfg`
- `tests/replan_episode_identity.rs:286-296` `two_episode_cfg`

#### `dup-da45cd3da239` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: crates/rigger-conductor/src/conductor.rs, crates/rigger-domain/src/metrics.rs, tests/plan_critique_spec_defect_stop_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:51194-51196` `stop_halt`
- `crates/rigger-domain/src/metrics.rs:2950-2952` `canary_run_marker`
- `tests/plan_critique_spec_defect_stop_periphery.rs:65-67` `halt`

#### `dup-5c13daf57f07` (near, 2 sites)

Proposed home: `replay_keys::replay_keys`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/replay_keys.rs:39-41` `insert`
- `crates/rigger-conductor/src/replay_keys.rs:44-46` `contains`

#### `dup-16e1b74fc5bd` (semantic, 2 sites)

Proposed home: `one shared `gating_agent_ids` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-config-files/src/config.rs:22-39` `gating_agent_ids`
- `crates/rigger-domain/src/config.rs:574-585` `gating_agent_ids`

#### `dup-3d575d71e10f` (exact, 2 sites)

Proposed home: `config_store::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-config-files/src/config_store.rs:209-217` `read_store_config`
- `crates/rigger-config-files/src/config_store.rs:274-282` `read_scratch_defaults`

#### `dup-947acd1ae58d` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: crates/rigger-config-files/src/config_store.rs, src/cli/mod.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-config-files/src/config_store.rs:764-797` `literal_is_emit_payload_binds_only_an_abutting_payload_or_emit_word`
- `src/cli/mod.rs:6992-6998` `is_uuid8_accepts_exactly_eight_hex_digits`
- `tests/simplification_audit.rs:8474-8482` `looks_error_shaping_matches_error_and_underscore_bounded_err_but_not_an_incidental_substring`

#### `dup-5c52773cc009` (near, 2 sites)

Proposed home: `config_store::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-config-files/src/config_store.rs:959-966` `parses_agent_frontmatter_and_body`
- `crates/rigger-config-files/src/config_store.rs:1314-1323` `model_ladder_parses_from_frontmatter`

#### `dup-360e8b87398d` (exact, 4 sites)

Proposed home: `a new shared module (sites span 4 files: crates/rigger-config-files/src/config_store.rs, crates/rigger-domain/src/contextgraph/query.rs, crates/rigger-domain/src/spec.rs, src/cli/validate.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-config-files/src/config_store.rs:969-971` `rejects_missing_frontmatter`
- `crates/rigger-domain/src/contextgraph/query.rs:2096-2098` `graph_load_rejects_malformed_json_without_panicking`
- `crates/rigger-domain/src/spec.rs:1275-1277` `empty_when_no_criteria`
- `src/cli/validate.rs:1417-1419` `dirty_tracked_paths_on_a_clean_tree_is_empty`

#### `dup-f61039476fc4` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-console/src/console/map.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-console/src/console/map.rs:879-885` `as_candidate`
- `tests/simplification_audit.rs:1681-1690` `map_entry_wire`
- `tests/simplification_audit.rs:1692-1699` `map_entry_lines`

#### `dup-ce4fa432a6da` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-console/src/console/map.rs, tests/files_lens_view_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-console/src/console/map.rs:1246-1248` `in_community`
- `tests/files_lens_view_periphery.rs:66-68` `refs`

#### `dup-c138bb72172b` (near, 9 sites)

Proposed home: `a new shared module (sites span 4 files: crates/rigger-console/src/console/map.rs, crates/rigger-domain/src/ledger.rs, crates/rigger-domain/src/progress.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-console/src/console/map.rs:1253-1256` `module_of_a_leaf_src_file_is_its_stem_never_the_file_name`
- `crates/rigger-console/src/console/map.rs:1285-1289` `district_purpose_uses_the_curated_table_when_present`
- `crates/rigger-domain/src/ledger.rs:1217-1224` `short_run_id_truncates_to_twelve_chars_and_passes_shorter_ids_through`
- `crates/rigger-domain/src/ledger.rs:1227-1242` `spec_stem_extracts_and_sanitizes_the_file_stem`
- `crates/rigger-domain/src/progress.rs:588-591` `each_run_reports_on_its_own_stream_and_a_run_less_report_on_the_bare_one`
- `tests/simplification_audit.rs:7306-7317` `impl_self_type_strips_a_trailing_where_clause_on_a_non_generic_self_type`
- `tests/simplification_audit.rs:7337-7348` `impl_self_type_strips_a_leading_dyn_token_on_the_self_type`
- `tests/simplification_audit.rs:7360-7366` `strip_trailing_where_clause_is_a_word_boundary_match_not_a_substring_match`
- `tests/simplification_audit.rs:7925-7935` `ident_kind_marker_classifies_by_casing`

#### `dup-cf5c8cd56db9` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-console/src/console/map.rs, crates/rigger-store-sqlite/src/eventstore/mod.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-console/src/console/map.rs:1268-1282` `module_of_never_returns_a_string_carrying_a_dot_or_a_slash`
- `crates/rigger-store-sqlite/src/eventstore/mod.rs:279-292` `no_credential_fragment_ever_survives`

#### `dup-5449e35bae05` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: crates/rigger-console/src/console/map.rs, crates/rigger-domain/src/spec.rs, crates/rigger-store-sqlite/src/eventstore/mod.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-console/src/console/map.rs:1983-1998` `budget_scales_the_step_by_zoom_rather_than_offsetting_it`
- `crates/rigger-domain/src/spec.rs:2202-2230` `strip_inline_code_direct_exact_output_pins_the_one_span_per_kind_rule`
- `crates/rigger-store-sqlite/src/eventstore/mod.rs:258-275` `a_delimiter_inside_the_userinfo_never_leaks_the_credential_head`

#### `dup-b8d5ee4b6f2b` (near, 3 sites)

Proposed home: `map::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-console/src/console/map.rs:2489-2496` `landmarks_ranks_by_whole_map_degree_descending`
- `crates/rigger-console/src/console/map.rs:2593-2598` `search_is_case_insensitive`
- `crates/rigger-console/src/console/map.rs:2607-2615` `search_hit_carries_kind_and_degree_beside_the_name`

#### `dup-3eae43945289` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-dash/src/dash.rs, crates/rigger-domain/src/retention.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-dash/src/dash.rs:619-621` `serialize`
- `crates/rigger-domain/src/retention.rs:50-52` `identity`

#### `dup-9d67deeaad9d` (semantic, 60 sites)

Proposed home: `crates/rigger-process/src/reap.rs as the one /proc-reading module (dash.rs's own /proc readers already duplicate reap.rs's field-after-the-comm's-closing-paren /proc/<pid>/stat parse - see the report's worked example)`

mandatory sweep: /proc-path string literals - 60 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-dash/src/dash.rs:658-658` `"/proc/net/tcp"`
- `crates/rigger-dash/src/dash.rs:689-689` `"/proc"`
- `crates/rigger-dash/src/dash.rs:8618-8618` `"/proc"`
- `crates/rigger-dash/src/dash.rs:8626-8626` `"the /proc scan must find THIS process as the holder of its own listener"`
- `crates/rigger-dash/src/dash.rs:8633-8633` `"/proc"`
- `crates/rigger-dash/src/dash.rs:8657-8657` `"/proc"`
- `crates/rigger-dash/src/dash.rs:8699-8699` `"/proc"`
- `crates/rigger-process/src/holders.rs:25-25` `"/proc"`
- `crates/rigger-process/src/holders.rs:72-72` `"/proc"`
- `crates/rigger-process/src/reap.rs:133-133` `"/proc"`
- `crates/rigger-process/src/reap.rs:215-215` `"/proc/{pid}/stat"`
- `crates/rigger-process/src/reap.rs:243-243` `"/proc/{pid}/status"`
- `crates/rigger-process/src/reap.rs:303-303` `"/proc"`
- `crates/rigger-process/src/reap.rs:380-380` `"/proc/{}/cwd"`
- `crates/rigger-worktree-git/src/worktree.rs:3135-3135` `"/proc"`
- `crates/rigger-worktree-git/src/worktree.rs:3139-3139` `"/proc/{pid}/cmdline"`
- `crates/rigger-worktree-git/src/worktree.rs:3163-3163` `"/proc/{}"`
- `src/cli/run.rs:3805-3805` `"/proc"`
- `src/cli/run.rs:3909-3909` `"/proc"`
- `tests/cli.rs:28782-28782` `"/proc"`
- `tests/cli.rs:32997-32997` `"/proc"`
- `tests/cli.rs:33105-33105` `"the holder pid {holder_pid} never reached the STOPPED (T) state in /proc"`
- `tests/cli.rs:33142-33142` `"/proc"`
- `tests/cli.rs:33193-33193` `"/proc"`
- `tests/cli.rs:33216-33216` `"/proc"`
- `tests/cli.rs:33241-33241` `"/proc"`
- `tests/cli.rs:33305-33305` `"/proc"`
- `tests/cli.rs:33331-33331` `"/proc"`
- `tests/cli.rs:33429-33429` `"/proc"`
- `tests/cli.rs:33467-33467` `"held_port_holder's message half and describe_held_port_if_confirmed's own return \
             must agree (scheduler-state letter normalized away since each call independently \
             re-reads /proc and can observe a state flap) - they are documented as sharing one \
             discovery"`
- `tests/cli.rs:33486-33486` `"/proc"`
- `tests/common/fixtures/host.rs:83-83` `"/proc/{pid}/stat has a pgrp field after comm"`
- `tests/common/fixtures/host.rs:189-189` `"/proc/self/task"`
- `tests/common/fixtures/spawn.rs:292-292` `"/proc/self"`
- `tests/common/fixtures/spawn.rs:293-293` `"{name}: the sleeper check reads /proc, which this host does not mount"`
- `tests/common/fixtures/spawn.rs:302-302` `"/proc"`
- `tests/duplication_catalog_contract_periphery.rs:100-100` `"/proc-path string literals"`
- `tests/mutation_runner_pdeathsig_periphery.rs:170-170` `"cat /proc/self/limits; echo \"MALLOC_ARENA_MAX=${MALLOC_ARENA_MAX-unset}\""`
- `tests/proofs_settle_once_periphery.rs:40-40` `"/proc/thread-self/schedstat"`
- `tests/proofs_settle_once_periphery.rs:41-41` `"/proc/thread-self/schedstat names the thread's CPU time"`
- `tests/simplification_audit.rs:2795-2795` `"/proc/<pid>/stat or /proc/<pid>/status field-extraction functions"`
- `tests/simplification_audit.rs:2801-2801` `"/proc-path string literals"`
- `tests/simplification_audit.rs:2976-2976` `"/proc"`
- `tests/simplification_audit.rs:2977-2977` `"crates/rigger-process/src/reap.rs as the one /proc-reading module (dash.rs's own /proc readers already \
             duplicate reap.rs's field-after-the-comm's-closing-paren /proc/<pid>/stat parse - \
             see the report's worked example)"`
- `tests/simplification_audit.rs:3017-3017` `"/proc"`
- `tests/simplification_audit.rs:3246-3246` `"crates/rigger-process/src/reap.rs as the one /proc/<pid>/stat and /proc/<pid>/status parser, returning \
             whichever field each caller needs, so dash.rs::process_state and \
             reap.rs::pid_starttime/read_ppid stop each re-deriving the pid(comm)state... split"`
- `tests/simplification_audit.rs:3664-3664` `"Two real recall gaps surfaced this way and were closed by widening the mechanical \
         sweep with a new generalizable detector each - not a one-off citation - so the fix \
         catches every present and future instance of its class, each pinned by a real-tree \
         regression test: `find_proc_stat_or_status_readers` (decision \
         `u85c2-proc-stat-worked-example`) groups every function reading a `/proc/<pid>/stat` or \
         `/proc/<pid>/status` literal, closing the spec's own named worked example - \
         `{}` `process_state` next to `{}` `pid_starttime`, the \
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
         extractor, `crates/rigger-grounder/src/grounder/symbols/extract.rs::extract` (its own module doc's claim, \
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
         (see this subsection's opening paragraph); that exclusion still applies unchanged."`
- `tests/simplification_audit.rs:4087-4087` `"A second mutation authority for one domain: the one previously-known \
        instance in this codebase (`crates/rigger-dash/src/dash.rs` reimplementing `crates/rigger-process/src/reap.rs`'s \
        `/proc` pid scan, spec 85's own Goal example, upheld at spec 62's capstone) \
        is a duplicate READ-only reimplementation, not a bypassed MUTATION path - it \
        is section 2's finding (`u85c2-proc-stat-worked-example`, \
        `find_proc_stat_or_status_readers`), not re-counted here to avoid \
        double-charging one defect to two sections. Checked process spawning as the \
        one other plausible second-authority candidate: production code constructs \
        every process through the one process-spawn port (`{PROCESS_SPAWN_PORT}`), so \
        production `conductor.rs` never builds a git command of its own. \
        `crates/rigger-worktree-git/src/worktree.rs` is the sole git-worktree-mutation \
        authority OUTSIDE the \
        composition root. Inside it, `{MAIN}` (exempt from the port-concretion-reach \
        check above, not from this one) holds two more git-worktree-mutation sites: \
        `reap_then_remove_worktree` (`{}`), the sanctioned worktree half of the \
        spec-34/spec-79 orphan-sweep and extensively reviewed across those specs - a \
        deliberate design choice, not a gap; and `materialize_config_at_rev` (`{}`), \
        a real, already-known, non-blocking gap \
        (`arch-u13-config-checkout-bypasses-worktree-authority` / \
        `arch-u2r-config-checkout-shells-git` / \
        `arch-u2r2-replayrunner-and-config-checkout-persist-not-introduced`: the \
        `Worktree` API is branch-creating and exposes no detach-at-rev checkout, so \
        this is a gap in that authority rather than a competing abstraction). No \
        second mutation authority found beyond the already-cited, \
        already-catalogued `/proc` case and this already-dispositioned \
        `materialize_config_at_rev` gap."`
- `tests/simplification_audit.rs:5104-5104` `"Largest risk-reduction first is read as six tiers, ranked by the KIND of risk \
        each entry retires, highest first:\n\n\
        1. Tier 1 - active correctness risk, PLUS item 0: a use case already depends on the \
        wrong concretion, or two independent implementations of one concern can already \
        drift apart silently (section 3's boundary violation; the one already-drifted \
        `/proc`-reading pair section 2 and section 3 both name) - live gaps, not just size. \
        Item 0 (deleting the dead-code ledger, section 4.3) is placed here too, first of \
        all: not a live-gap risk itself, but the cheapest, zero-behavior-change move \
        available, and it shrinks the files tiers 2 and 3 operate on before either touches \
        them.\n\
        2. Tier 2 - god-file test-module extraction: each of the three god files' own \
        inline `#[cfg(test)] mod tests` holds {tier2_lo}-{tier2_hi}% of that file's mapped \
        functions (section 1's map), and moving it is a pure \
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
        6. Tier 6 - remaining catalog sweep: the {remaining} src-touching clusters section 2 \
        found but tiers 1 and 4 did not individually name. Unlike every other tier, none of \
        these {remaining} have been read and risk-assessed one at a time the way tiers 1-4's named \
        clusters have - they are consumed straight from the catalog - so this tier carries \
        production-correctness exposure tiers 2, 3 and 5 do not, and is ordered last: the \
        follow-up spec must triage each cluster's own production-or-test status before \
        merging it, not assume tier 5's blanket test-only treatment applies here too.\n\n\
        Within a tier, entries are ordered largest-first by the site or line count each \
        retires - the same rule the tiers themselves follow, applied one level down.\n\n"`
- `tests/simplification_audit.rs:5199-5199` `"#### 2. Retire the duplicate `/proc`-reading authority (`{}` + `{}`)\n\n"`
- `tests/simplification_audit.rs:5203-5203` `"- Scope: the production half is done - `crates/rigger-dash/src/dash.rs::process_state` and \
        `crates/rigger-process/src/reap.rs::pid_starttime` both read their `/proc/<pid>/stat` field through \
        `crates/rigger-process/src/reap.rs::stat_field_after_comm`, the one parser of the kernel's \
        `pid (comm) state ...` layout (`read_ppid` reads `/status`, a different file). What \
        remains is the test-only readers (`{readers_id}`, {readers_sites} sites across \
        {proc_reader_files}, such as the shared `tests/common/fixtures/host.rs::pgid_of` \
        fixture) and {literal_sites} raw `/proc`-path string literals across {literal_files} \
        files (`{literals_id}`), most of them assertion messages and this audit's own sweep \
        names rather than reads.\n\
        - Files: the test-only readers `{readers_id}` names.\n\
        - Expected line delta: small and negative - a test fixture reads its field through one \
        shared helper instead of re-splitting the stat line.\n\
        - Risk: low - test-only; nothing this touches can signal or end a process, so it \
        carries none of the no-os-kill gate's own risk surface.\n\
        - Unblocks: retires the last copies of the \"duplicate implementation reconciled \
        after the fact\" pattern the operator's strict-DRY rule targets - the concrete \
        precedent spec 85's own Goal cites.\n\n"`
- `tests/simplification_audit.rs:8295-8295` `"fn state_of(pid: u32) -> Option<char> {\n    let stat = std::fs::read_to_string(format!(\"/proc/{pid}/stat\")).ok()?;\n    stat.rsplit_once(')')?.1.split_whitespace().next()?.chars().next()\n}\n"`
- `tests/simplification_audit.rs:8299-8299` `"fn starttime_of(pid: u32) -> Option<u64> {\n    let stat = std::fs::read_to_string(format!(\"/proc/{pid}/stat\")).ok()?;\n    stat.rsplit_once(')')?.1.split_whitespace().nth(19)?.parse().ok()\n}\n"`
- `tests/simplification_audit.rs:8303-8303` `"fn ppid_of(pid: u32) -> Option<u32> {\n    let stat = std::fs::read_to_string(format!(\"/proc/{pid}/stat\")).ok()?;\n    stat.rsplit_once(')')?.1.split_whitespace().nth(1)?.parse().ok()\n}\n"`
- `tests/simplification_audit.rs:8458-8458` `"fn a() {\n    let _ = std::fs::read_to_string(\"/proc/1/stat\");\n    let _ = \"hello\";\n}\n"`
- `tests/simplification_audit.rs:8459-8459` `"/proc"`
- `tests/simplification_audit.rs:8461-8461` `"/proc"`
- `tests/simplification_audit.rs:8526-8526` `"fn state_of(pid: u32) -> Option<char> {\n    let s = std::fs::read_to_string(format!(\"/proc/{pid}/stat\")).ok()?;\n    s.chars().next()\n}\nfn ppid_of(pid: u32) -> Option<u32> {\n    let s = std::fs::read_to_string(format!(\"/proc/{pid}/status\")).ok()?;\n    s.parse().ok()\n}\nfn unrelated() -> u32 {\n    1\n}\n"`
- `tests/simplification_audit.rs:8548-8548` `"the /proc reader sweep is catalogued"`
- `tests/simplification_audit.rs:8554-8554` `"the /proc reader sweep {:?} must carry the one stat parser and neither former \
             re-deriver, found: {names:?}"`

#### `dup-22a325f863b8` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-dash/src/dash.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-dash/src/dash.rs:4071-4076` `console_core_wasm_route_serves_the_embedded_artifact_as_application_wasm`
- `tests/simplification_audit.rs:7008-7015` `a_cfg_test_attribute_directly_on_an_impl_block_is_flagged_test`
- `tests/simplification_audit.rs:7135-7141` `a_cfg_test_pub_fn_is_still_flagged_test_pub_survives_between_attribute_and_keyword`

#### `dup-2a4f88aea011` (near, 4 sites)

Proposed home: `a new shared module (sites span 3 files: crates/rigger-dash/src/dash.rs, tests/console_palette_periphery.rs, tests/store_resolution.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-dash/src/dash.rs:4830-4865` `the_page_layout_cannot_scroll_the_body_horizontally`
- `crates/rigger-dash/src/dash.rs:4876-4913` `the_landing_view_lists_instances_and_threads_the_attach_selector`
- `tests/console_palette_periphery.rs:400-415` `the_served_console_page_sends_the_scrub_position_to_palette_commands_when_not_live`
- `tests/store_resolution.rs:98-114` `the_single_resolver_exists_and_the_old_per_command_helper_is_retired`

#### `dup-9904088ec60d` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-dash/src/dash.rs, tests/common/fixtures/graph.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-dash/src/dash.rs:9204-9206` `layer_of`
- `tests/common/fixtures/graph.rs:276-278` `call_layer`

#### `dup-0e7d0c54553e` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-dash/src/dash.rs, crates/rigger-grounder/src/grounder/workflowdef.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-dash/src/dash.rs:9207-9209` `ids`
- `crates/rigger-grounder/src/grounder/workflowdef.rs:123-128` `light_reviewers_of`

#### `dup-e79afb3298c3` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-dash/src/dash.rs, tests/metadata_card_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-dash/src/dash.rs:9815-9839` `an_absent_explain_leaves_the_graph_route_unchanged`
- `crates/rigger-dash/src/dash.rs:10077-10097` `a_cluster_drill_carries_no_memory_field`
- `tests/metadata_card_periphery.rs:349-366` `the_served_route_degrades_gracefully_for_an_unknown_card_subject`

#### `dup-b3200e581798` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-dash/src/dash.rs, tests/metadata_card_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-dash/src/dash.rs:10123-10173` `card_graph`
- `tests/metadata_card_periphery.rs:49-100` `fixture_graph`

#### `dup-fae663db78c7` (near, 4 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-dash/src/dash.rs, tests/metadata_card_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-dash/src/dash.rs:10452-10494` `the_card_route_serves_a_known_subjects_card_and_null_for_an_unknown_one`
- `tests/metadata_card_periphery.rs:119-166` `the_served_route_carries_a_code_subjects_card`
- `tests/metadata_card_periphery.rs:174-200` `the_served_route_omits_community_and_line_keys_for_a_membership_less_entity`
- `tests/metadata_card_periphery.rs:373-399` `the_card_param_takes_precedence_over_a_stray_seed_or_lens_param`

#### `dup-28ac8c722172` (semantic, 2 sites)

Proposed home: `one shared `current_run_id` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-dash/src/mcpserver.rs:562-564` `current_run_id`
- `crates/rigger-domain/src/run.rs:257-259` `current_run_id`

#### `dup-468c9f4fc26f` (semantic, 2 sites)

Proposed home: `one shared `spawn_request` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-domain/src/agent.rs:227-256` `spawn_request`
- `tests/common/mod.rs:580-594` `spawn_request`

#### `dup-74b161ef523c` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-domain/src/agent.rs, crates/rigger-domain/src/spec.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/agent.rs:473-489` `as_str`
- `crates/rigger-domain/src/spec.rs:167-173` `name`

#### `dup-f6925ead8f1a` (near, 2 sites)

Proposed home: `canary::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/canary.rs:102-117` `to_event`
- `crates/rigger-domain/src/canary.rs:189-198` `to_event`

#### `dup-352c2984faa0` (exact, 9 sites)

Proposed home: `a new shared module (sites span 8 files: crates/rigger-domain/src/community.rs, crates/rigger-domain/src/eventstore.rs, crates/rigger-domain/src/failure.rs, crates/rigger-domain/src/metrics.rs, crates/rigger-domain/src/progress.rs, crates/rigger-driver/src/driver/mod.rs, crates/rigger-driver/src/reaped_child.rs, tests/common/mcp.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/community.rs:245-247` `len`
- `crates/rigger-domain/src/community.rs:264-266` `is_empty`
- `crates/rigger-domain/src/eventstore.rs:262-264` `handed`
- `crates/rigger-domain/src/failure.rs:227-229` `is_empty`
- `crates/rigger-domain/src/metrics.rs:552-554` `adversary_precision`
- `crates/rigger-domain/src/progress.rs:256-258` `is_open`
- `crates/rigger-driver/src/driver/mod.rs:241-243` `resumed`
- `crates/rigger-driver/src/reaped_child.rs:44-46` `id`
- `tests/common/mcp.rs:51-53` `id`

#### `dup-a9ed3e0e9d47` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: crates/rigger-domain/src/community.rs, crates/rigger-domain/src/eventstore.rs, crates/rigger-domain/src/failure.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/community.rs:251-253` `nodes`
- `crates/rigger-domain/src/eventstore.rs:498-500` `types`
- `crates/rigger-domain/src/failure.rs:232-234` `rules`

#### `dup-1065bd3505a8` (near, 5 sites)

Proposed home: `a new shared module (sites span 4 files: crates/rigger-domain/src/concepts.rs, crates/rigger-domain/src/ingest.rs, crates/rigger-grounder/src/grounder/symbols/extract.rs, tests/common/source_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/concepts.rs:74-76` `is_intent_doc`
- `crates/rigger-domain/src/concepts.rs:80-82` `is_label_doc`
- `crates/rigger-domain/src/ingest.rs:48-50` `is_derived_index_type`
- `crates/rigger-grounder/src/grounder/symbols/extract.rs:751-753` `is_std_root`
- `tests/common/source_audit.rs:439-441` `is_keyword`

#### `dup-34d133ea2afb` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-domain/src/config.rs, src/cli/hygiene.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/config.rs:528-530` `is_empty`
- `src/cli/hygiene.rs:840-842` `driver_dead`

#### `dup-5f6fcac3a66f` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-domain/src/config.rs, crates/rigger-domain/src/eventstore.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/config.rs:533-535` `depth`
- `crates/rigger-domain/src/eventstore.rs:430-432` `facts`

#### `dup-af8b91bc2b2a` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: crates/rigger-domain/src/config.rs, tests/common/source_audit.rs, tests/no_os_kill_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/config.rs:1715-1717` `is_word_byte`
- `tests/common/source_audit.rs:202-204` `is_ident_char`
- `tests/no_os_kill_audit.rs:59-61` `is_word_char`

#### `dup-08ca12f5868a` (near, 2 sites)

Proposed home: `docs::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/docs.rs:99-117` `render_using_rigger_skill`
- `crates/rigger-domain/src/docs.rs:121-132` `render_handbook_discipline`

#### `dup-cbeb7fee0352` (near, 4 sites)

Proposed home: `docs::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/docs.rs:962-991` `render_build_graph_skill`
- `crates/rigger-domain/src/docs.rs:996-1023` `render_reindex_skill`
- `crates/rigger-domain/src/docs.rs:1028-1080` `render_resume_a_run_skill`
- `crates/rigger-domain/src/docs.rs:1209-1259` `render_restore_the_dash_skill`

#### `dup-3d5ea11431d3` (near, 2 sites)

Proposed home: `docs::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/docs.rs:1087-1122` `render_handle_an_escalation_skill`
- `crates/rigger-domain/src/docs.rs:1268-1317` `render_diagnose_churn_skill`

#### `dup-e45a74e51e8a` (near, 4 sites)

Proposed home: `a new shared module (sites span 3 files: crates/rigger-domain/src/eventstore.rs, crates/rigger-domain/src/spawn.rs, crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/eventstore.rs:166-169` `with_valid_from`
- `crates/rigger-domain/src/spawn.rs:779-782` `with_meta`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:1068-1071` `with_store_identity`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:1077-1080` `with_lock_ceiling`

#### `dup-cfb679088c89` (exact, 2 sites)

Proposed home: `eventstore::content_identity`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/eventstore.rs:424-427` `with_facts`
- `crates/rigger-domain/src/eventstore.rs:440-443` `with_key_parts`

#### `dup-57c1e8124510` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-domain/src/eventstore.rs, crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/eventstore.rs:493-495` `meta_key`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:385-387` `path`

#### `dup-9a07b087e8f1` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: crates/rigger-domain/src/failure.rs, crates/rigger-domain/src/gate.rs, crates/rigger-domain/src/ledger.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/failure.rs:45-51` `as_str`
- `crates/rigger-domain/src/gate.rs:97-103` `as_str`
- `crates/rigger-domain/src/ledger.rs:50-62` `as_str`

#### `dup-3b3a41e03376` (exact, 3 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-domain/src/failure.rs, crates/rigger-domain/src/gate.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/failure.rs:67-69` `reruns`
- `crates/rigger-domain/src/failure.rs:76-78` `demotes_on_persistent_failure`
- `crates/rigger-domain/src/gate.rs:73-75` `runs_inline`

#### `dup-e4413ba06992` (near, 4 sites)

Proposed home: `a new shared module (sites span 4 files: crates/rigger-domain/src/failure.rs, crates/rigger-driver/src/liveness.rs, crates/rigger-grounder/src/grounder/symbols/grounder.rs, src/cli/mod.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/failure.rs:170-172` `is_any`
- `crates/rigger-driver/src/liveness.rs:800-802` `is_empty`
- `crates/rigger-grounder/src/grounder/symbols/grounder.rs:181-183` `is_empty`
- `src/cli/mod.rs:3664-3669` `is_empty`

#### `dup-e9220ec4784e` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-domain/src/ingest.rs, tests/published_content_key_split_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/ingest.rs:92-95` `derived_key_parts`
- `tests/published_content_key_split_periphery.rs:40-43` `split`

#### `dup-557d6899b868` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-domain/src/metrics.rs, tests/common/fixtures/events.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/metrics.rs:268-270` `total`
- `tests/common/fixtures/events.rs:1192-1194` `cost`

#### `dup-6ffa6e5d1f83` (exact, 7 sites)

Proposed home: `metrics::support (consolidate these 7 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/metrics.rs:448-450` `survival`
- `crates/rigger-domain/src/metrics.rs:546-548` `lens_overlap_rate`
- `crates/rigger-domain/src/metrics.rs:560-562` `first_pass_yield`
- `crates/rigger-domain/src/metrics.rs:566-568` `escalation_rate`
- `crates/rigger-domain/src/metrics.rs:1192-1194` `rate`
- `crates/rigger-domain/src/metrics.rs:1257-1259` `adjudicator_accuracy`
- `crates/rigger-domain/src/metrics.rs:1263-1265` `stability_rate`

#### `dup-c16dbfd88b00` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-domain/src/metrics.rs, crates/rigger-domain/src/spawn.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/metrics.rs:1428-1430` `changed`
- `crates/rigger-domain/src/spawn.rs:785-787` `is_error`

#### `dup-e7182e4e126f` (exact, 3 sites)

Proposed home: `metrics::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/metrics.rs:1507-1512` `started`
- `crates/rigger-domain/src/metrics.rs:1514-1519` `status`
- `crates/rigger-domain/src/metrics.rs:1546-1551` `artifact_verdict`

#### `dup-df893b1b93d7` (near, 6 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-domain/src/metrics.rs, crates/rigger-domain/src/run.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/metrics.rs:1521-1526` `failed`
- `crates/rigger-domain/src/metrics.rs:1528-1533` `integrated`
- `crates/rigger-domain/src/metrics.rs:1535-1537` `escalated`
- `crates/rigger-domain/src/run.rs:721-723` `decision`
- `crates/rigger-domain/src/run.rs:724-726` `finding`
- `crates/rigger-domain/src/run.rs:727-729` `lesson`

#### `dup-d07779dc106d` (near, 2 sites)

Proposed home: `metrics::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/metrics.rs:2401-2424` `finding_survival_is_upheld_over_raised_per_actor`
- `crates/rigger-domain/src/metrics.rs:2729-2757` `cost_per_upheld_finding_is_spawns_over_upheld_per_tier`

#### `dup-6a304442d31e` (near, 2 sites)

Proposed home: `metrics::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/metrics.rs:3178-3197` `project_canary_counts_correctly_rejected_items_with_empty_attribution`
- `crates/rigger-domain/src/metrics.rs:3206-3224` `project_canary_counts_controls_and_false_positives`

#### `dup-0bd924af75c5` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-domain/src/progress.rs, crates/rigger-domain/src/spawn.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/progress.rs:129-133` `agent_progress_events`
- `crates/rigger-domain/src/spawn.rs:971-973` `spawn_requested_events`

#### `dup-e04bda77465d` (exact, 2 sites)

Proposed home: `run::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/run.rs:573-575` `to_event`
- `crates/rigger-domain/src/run.rs:582-584` `to_event`

#### `dup-4b3a37bb0a60` (exact, 2 sites)

Proposed home: `spawn::adjudication`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/spawn.rs:540-542` `is_infra_fault`
- `crates/rigger-domain/src/spawn.rs:546-548` `is_spec_ambiguity`

#### `dup-2b572fcabbd4` (semantic, 2 sites)

Proposed home: `one shared `liveness_fault` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-domain/src/spawn.rs:770-776` `liveness_fault`
- `tests/dash_run_tree_spine.rs:81-85` `liveness_fault`

#### `dup-e3c162b7bd56` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: crates/rigger-domain/src/spec.rs, crates/rigger-grounder/src/grounder/design/extract.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/spec.rs:546-553` `starts_new_element`
- `crates/rigger-grounder/src/grounder/design/extract.rs:406-409` `is_markdown`
- `tests/simplification_audit.rs:2864-2871` `looks_error_shaping`

#### `dup-42b8a5023d8d` (near, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/spec.rs:657-659` `find_word`
- `crates/rigger-domain/src/spec.rs:667-669` `find_word_across_hyphen`

#### `dup-81e8434ac795` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-domain/src/spec.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/spec.rs:2143-2155` `disposition_check_fails_closed_after_a_stray_unmatched_quote_earlier_in_the_paragraph`
- `crates/rigger-domain/src/spec.rs:2360-2369` `disposition_check_resumes_scanning_after_notes_ends`
- `tests/simplification_audit.rs:6955-6959` `a_trait_method_signature_without_a_body_is_not_recorded`

#### `dup-9f14620abab1` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-domain/src/spec.rs, crates/rigger-driver/src/liveness.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/spec.rs:2522-2524` `heading_level_rejects_more_than_six_hashes`
- `crates/rigger-driver/src/liveness.rs:1038-1051` `marker_filename_is_none_only_for_a_truly_empty_input_so_a_join_can_never_be_a_no_op`

#### `dup-3dc74f749a1a` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: crates/rigger-domain/src/spec.rs, tests/common/cli.rs, tests/common/fixtures/cursor.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/spec.rs:2643-2645` `f11`
- `tests/common/cli.rs:1970-1972` `graph_build_line`
- `tests/common/fixtures/cursor.rs:197-199` `catch_up_stopped`

#### `dup-d340e212490f` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-domain/src/watch.rs, crates/rigger-driver/src/driver/workflow.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/watch.rs:670-672` `new`
- `crates/rigger-driver/src/driver/workflow.rs:104-106` `new`

#### `dup-161c827c2a70` (near, 2 sites)

Proposed home: `claude_code::failing_read_store`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-driver/src/driver/claude_code.rs:2346-2353` `read_stream`
- `crates/rigger-driver/src/driver/claude_code.rs:2400-2407` `read_stream_typed`

#### `dup-4979c76ee36f` (near, 2 sites)

Proposed home: `claude_code::failing_read_store`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-driver/src/driver/claude_code.rs:2393-2399` `last_position`
- `crates/rigger-driver/src/driver/claude_code.rs:2427-2433` `latest_in_group`

#### `dup-117928e59e0c` (near, 2 sites)

Proposed home: `claude_code::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-driver/src/driver/claude_code.rs:2691-2712` `build_args_mcp_config_names_the_spawn_bound_server`
- `crates/rigger-driver/src/driver/claude_code.rs:2715-2733` `build_args_mcp_config_uses_the_configured_rigger_bin`

#### `dup-ccd4dd4d7b0d` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-driver/src/driver/mod.rs, tests/claude_code_stream_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-driver/src/driver/mod.rs:280-282` `resumed_from`
- `tests/claude_code_stream_periphery.rs:2786-2788` `launch_dir`

#### `dup-282069e7109f` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: crates/rigger-driver/src/driver/replay.rs, tests/common/fixtures/config.rs, tests/model_pinning_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-driver/src/driver/replay.rs:306-313` `sonnet_agent`
- `tests/common/fixtures/config.rs:599-609` `fan_out_stage`
- `tests/model_pinning_periphery.rs:65-72` `agent`

#### `dup-52a91e1f67e3` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: crates/rigger-driver/src/driver/replay.rs, tests/claude_code_stream_periphery.rs, tests/common/fixtures/conductor.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-driver/src/driver/replay.rs:319-326` `opts_for`
- `tests/claude_code_stream_periphery.rs:212-218` `opts`
- `tests/common/fixtures/conductor.rs:73-80` `implementer_opts`

#### `dup-b43ee3b4b5bb` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-driver/src/driver/replay.rs, crates/rigger-driver/src/liveness.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-driver/src/driver/replay.rs:371-379` `spawn_scratch_path_is_none_rather_than_relative_for_an_empty_scratch_root`
- `crates/rigger-driver/src/liveness.rs:1162-1170` `marker_path_is_none_rather_than_relative_for_an_empty_scratch_root`

#### `dup-8c5011c59ed1` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-driver/src/driver/replay.rs, tests/common/fixtures/config.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-driver/src/driver/replay.rs:962-971` `stage`
- `tests/common/fixtures/config.rs:41-47` `agent_with_prompt`

#### `dup-4cf25824ad00` (near, 2 sites)

Proposed home: `replay::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-driver/src/driver/replay.rs:1748-1796` `an_emit_only_approve_gating_persona_hard_errors_on_the_replay_driver`
- `crates/rigger-driver/src/driver/replay.rs:1871-1920` `a_parked_unanswered_sibling_does_not_suppress_this_units_own_approve_backstop`

#### `dup-02b6d2bbb967` (semantic, 2 sites)

Proposed home: `one shared `install_status_line` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-driver/src/hooks.rs:427-444` `install_status_line`
- `src/cli/setup.rs:1413-1417` `install_status_line`

#### `dup-a5bfac770233` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-driver/src/liveness.rs, src/cli/setup.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-driver/src/liveness.rs:1079-1093` `marker_filename_is_injective_so_two_ids_that_collided_under_a_prior_placeholder_scheme_no_longer_do`
- `src/cli/setup.rs:2877-2891` `normalize_origin_url_separates_distinct_repos_and_lowercases_only_the_host`

#### `dup-880060c29e31` (semantic, 66 sites)

Proposed home: `one sqlite-connection-opening adapter function every caller is injected with`

mandatory sweep: sqlite Connection::open call sites - 66 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:2279-2279` `Connection::open_with_flags`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:2321-2321` `Connection::open_with_flags`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:2346-2346` `Connection::open`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:7609-7609` `Connection::open`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:9045-9045` `Connection::open`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:9054-9054` `Connection::open`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:10744-10744` `Connection::open`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:10891-10891` `Connection::open`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:15396-15396` `Connection::open`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:16145-16145` `Connection::open`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:18658-18658` `Connection::open`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:20271-20271` `Connection::open`
- `crates/rigger-store-sqlite/src/eventstore/sqlite.rs:1661-1661` `Connection::open`
- `crates/rigger-store-sqlite/src/eventstore/sqlite.rs:2284-2284` `Connection::open`
- `crates/rigger-store-sqlite/src/eventstore/sqlite.rs:2306-2306` `Connection::open_with_flags`
- `crates/rigger-store-sqlite/src/eventstore/sqlite.rs:2629-2629` `Connection::open`
- `crates/rigger-store-sqlite/src/run_store.rs:1363-1363` `Connection::open`
- `crates/rigger-store-sqlite/src/sqlite.rs:13-13` `Connection::open`
- `src/cli/mod.rs:5757-5757` `Connection::open`
- `src/cli/mod.rs:13346-13346` `Connection::open`
- `tests/cli.rs:846-846` `Connection::open`
- `tests/cli.rs:949-949` `Connection::open`
- `tests/cli.rs:1052-1052` `Connection::open`
- `tests/cli.rs:1095-1095` `Connection::open`
- `tests/cli.rs:15710-15710` `Connection::open`
- `tests/common/cli.rs:109-109` `Connection::open`
- `tests/common/cli.rs:1405-1405` `Connection::open`
- `tests/common/cli.rs:1628-1628` `Connection::open`
- `tests/common/cli.rs:1638-1638` `Connection::open`
- `tests/common/cli.rs:2437-2437` `Connection::open`
- `tests/common/fixtures/cursor.rs:123-123` `Connection::open`
- `tests/common/fixtures/cursor.rs:135-135` `Connection::open`
- `tests/common/fixtures/cursor.rs:150-150` `Connection::open`
- `tests/common/fixtures/cursor.rs:165-165` `Connection::open`
- `tests/common/fixtures/passed_over.rs:57-57` `Connection::open`
- `tests/common/fixtures/sqlite.rs:8-8` `Connection::open`
- `tests/common/fixtures/sqlite.rs:22-22` `Connection::open`
- `tests/common/fixtures/sqlite.rs:31-31` `Connection::open`
- `tests/common/fixtures/sqlite.rs:39-39` `Connection::open`
- `tests/common/fixtures/sqlite.rs:58-58` `Connection::open`
- `tests/common/fixtures/sqlite.rs:70-70` `Connection::open`
- `tests/common/fixtures/sqlite.rs:99-99` `Connection::open`
- `tests/common/fixtures/sqlite.rs:144-144` `Connection::open`
- `tests/common/fixtures/write_lock.rs:11-11` `Connection::open`
- `tests/compaction_generations_periphery.rs:502-502` `Connection::open`
- `tests/compaction_generations_periphery.rs:975-975` `Connection::open`
- `tests/compaction_generations_periphery.rs:1194-1194` `Connection::open`
- `tests/compaction_generations_periphery.rs:1308-1308` `Connection::open`
- `tests/compaction_generations_periphery.rs:1384-1384` `Connection::open`
- `tests/compaction_generations_periphery.rs:1547-1547` `Connection::open`
- `tests/compaction_generations_periphery.rs:2643-2643` `Connection::open`
- `tests/compaction_generations_periphery.rs:4754-4754` `Connection::open`
- `tests/compaction_generations_periphery.rs:5871-5871` `Connection::open`
- `tests/graph_additive_indexes_persist.rs:86-86` `Connection::open`
- `tests/graph_additive_indexes_persist.rs:100-100` `Connection::open`
- `tests/graph_additive_indexes_persist.rs:213-213` `Connection::open`
- `tests/graph_additive_indexes_persist.rs:237-237` `Connection::open`
- `tests/reclaim_space_periphery.rs:240-240` `Connection::open`
- `tests/reclaim_space_periphery.rs:313-313` `Connection::open`
- `tests/reclaim_space_periphery.rs:399-399` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:61-61` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:760-760` `Connection::open`
- `tests/reset_derived_live_writer_guard_periphery.rs:76-76` `Connection::open`
- `tests/run_boundary_lookup_periphery.rs:72-72` `Connection::open`
- `tests/store_append_order_periphery.rs:60-60` `Connection::open`
- `tests/watchdog_cli_periphery.rs:157-157` `Connection::open`

#### `dup-c650c3e62b50` (exact, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:5764-5776` `calls_out`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:5837-5849` `callers_direct`

#### `dup-1cee568104ee` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:6888-6901` `subgraph_finds_the_governing_decision`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:17494-17514` `recording_proof_never_wipes_the_entitys_own_name_kind_and_line_attrs`

#### `dup-b7996d22b4c4` (near, 3 sites)

Proposed home: `sqlite::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:8175-8177` `without_a_cursor`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:8181-8183` `beside_another_log`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:8187-8189` `beside_no_log`

#### `dup-e45a2413a3f0` (near, 7 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs, tests/code_ingest_events.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:11463-11470` `apply_edge_inferred`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:12349-12356` `apply_doc_concept`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:12459-12466` `apply_doc_link`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:12789-12796` `apply_batch_ref`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:14743-14753` `apply_unit_integrated`
- `tests/code_ingest_events.rs:863-870` `apply_ref_json`
- `tests/code_ingest_events.rs:1000-1007` `apply_ref_fresh`

#### `dup-b5bd50de3e2a` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:11479-11498` `apply_code_entity_partial`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:11533-11551` `apply_community`

#### `dup-84342f753367` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:12469-12592` `design_intent_link_events_fold_into_the_five_design_intent_edges`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:12595-12713` `workflow_definition_events_fold_into_stage_gate_agent_nodes_with_needs_runs_reviews_edges`

#### `dup-0b2669324614` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:14436-14469` `decision_fold_projects_no_agent_node_or_decided_edge`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:14563-14591` `review_finding_projects_no_raised_edge_even_with_an_event_actor`

#### `dup-c76c4d91d8a8` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:15284-15286` `edge_projects`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:16313-16321` `index_names`

#### `dup-554a83a84ac8` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:16048-16089` `the_cross_file_inferred_tier_is_order_independent`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:16092-16108` `the_definition_upgrade_never_demotes_a_same_file_extracted_reference`

#### `dup-3cee929a82ba` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs, src/cli/hygiene.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:16222-16224` `edge_desc`
- `src/cli/hygiene.rs:242-248` `runs_menu_line`
- `src/cli/hygiene.rs:1023-1028` `pruned_line`

#### `dup-233d5e6363a7` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:17538-17569` `an_unresolvable_test_reference_is_staged_and_reconciled_once_its_definition_later_folds`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:17814-17845` `the_empty_boundary_sentinel_never_resolves_records_or_stages_anything_for_its_empty_name`

#### `dup-a34b5b595e15` (near, 2 sites)

Proposed home: `events::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-grounder/src/grounder/design/events.rs:20-37` `concept_events`
- `crates/rigger-grounder/src/grounder/design/events.rs:45-62` `link_events`

#### `dup-f2eee0c78cd6` (semantic, 3 sites)

Proposed home: `one shared `project_batches` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-grounder/src/grounder/design/events.rs:97-99` `project_batches`
- `crates/rigger-grounder/src/grounder/symbols/events.rs:66-68` `project_batches`
- `crates/rigger-grounder/src/grounder/workflowdef.rs:233-240` `project_batches`

#### `dup-4b942d7302a6` (near, 2 sites)

Proposed home: `model::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-grounder/src/grounder/design/model.rs:30-37` `node_kind`
- `crates/rigger-grounder/src/grounder/design/model.rs:81-89` `rel`

#### `dup-877c7e2e92fb` (near, 2 sites)

Proposed home: `mod::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-grounder/src/grounder/mod.rs:274-276` `ground_identifier`
- `crates/rigger-grounder/src/grounder/mod.rs:303-305` `ground`

#### `dup-8c8ae0851675` (semantic, 2 sites)

Proposed home: `one shared `extract_events` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-grounder/src/grounder/symbols/events.rs:223-369` `extract_events`
- `crates/rigger-grounder/src/grounder/workflowdef.rs:169-199` `extract_events`

#### `dup-bc4d3e954c47` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-grounder/src/grounder/symbols/events.rs, crates/rigger-store-sqlite/src/eventstore/sqlite.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-grounder/src/grounder/symbols/events.rs:908-917` `lang_str`
- `crates/rigger-store-sqlite/src/eventstore/sqlite.rs:1548-1553` `direction_sql`

#### `dup-476c04e8fc6f` (semantic, 3 sites)

Proposed home: `crates/rigger-grounder/src/grounder/symbols/extract.rs::extract as the ONE function that touches source parsing (already its own module doc's claim, architecture 5.5.3) - this file's own scan_file/tokenize are ad hoc scanners for the identical job and should route through an injected-grammar extractor rather than re-deriving structure by hand`

mandatory sweep: bespoke source-text lexer/scanner functions duplicating the canonical tree-sitter extractor - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-grounder/src/grounder/symbols/extract.rs:270-531` `extract`
- `tests/common/source_audit.rs:452-558` `tokenize`
- `tests/simplification_audit.rs:214-216` `scan_file`

#### `dup-6741e1f3023b` (semantic, 2 sites)

Proposed home: `one shared `changed_files` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-grounder/src/grounder/symbols/grounder.rs:75-98` `changed_files`
- `crates/rigger-worktree-git/src/worktree.rs:621-624` `changed_files`

#### `dup-9408447d0007` (near, 2 sites)

Proposed home: `grounder::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-grounder/src/grounder/symbols/grounder.rs:316-318` `commonness_map`
- `crates/rigger-grounder/src/grounder/symbols/grounder.rs:331-333` `ambiguity_map`

#### `dup-e36c95e103ac` (near, 2 sites)

Proposed home: `grounder::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-grounder/src/grounder/symbols/grounder.rs:1000-1053` `a_resolved_name_grounds_on_its_definer_and_call_sites_never_on_a_comment_mention`
- `crates/rigger-grounder/src/grounder/symbols/grounder.rs:1452-1489` `structural_view_is_cross_language_a_python_referencer_of_a_rust_symbol_is_included`

#### `dup-9c17d63a062c` (exact, 3 sites)

Proposed home: `grounder::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-grounder/src/grounder/symbols/grounder.rs:1132-1142` `blast_radius_grounds_a_path_the_tree_holds_on_that_file_alone`
- `crates/rigger-grounder/src/grounder/symbols/grounder.rs:1178-1188` `blast_radius_text_search_matches_a_trailing_underscore_span_as_a_prefix`
- `crates/rigger-grounder/src/grounder/symbols/grounder.rs:1193-1203` `blast_radius_text_search_matches_a_span_only_on_identifier_boundaries`

#### `dup-d36198b3982d` (near, 2 sites)

Proposed home: `grounder::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-grounder/src/grounder/symbols/grounder.rs:1592-1603` `a_reference_ranks_below_a_definition_of_the_same_name`
- `crates/rigger-grounder/src/grounder/symbols/grounder.rs:1842-1856` `ground_ranks_an_exact_name_match_above_a_name_that_merely_contains_the_token`

#### `dup-56485f8d4262` (near, 3 sites)

Proposed home: `grounder::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-grounder/src/grounder/symbols/grounder.rs:1759-1805` `a_concurrent_reindex_from_a_second_grounder_is_not_clobbered`
- `crates/rigger-grounder/src/grounder/symbols/grounder.rs:1808-1834` `reindex_replaces_only_a_changed_files_symbols`
- `crates/rigger-grounder/src/grounder/symbols/grounder.rs:2133-2160` `reindex_over_a_deleted_file_stops_grounding_it`

#### `dup-415d2df07b98` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-grounder/src/grounder/symbols/store.rs, crates/rigger-grounder/src/grounder/workflowdef.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-grounder/src/grounder/symbols/store.rs:282-286` `load_is_none_on_a_cold_start`
- `crates/rigger-grounder/src/grounder/workflowdef.rs:656-660` `project_events_on_a_missing_workflow_yields_nothing_never_a_crash`

#### `dup-3eb93069b8f0` (semantic, 2 sites)

Proposed home: `one shared `project_events` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-grounder/src/grounder/workflowdef.rs:207-213` `project_events`
- `tests/common/mod.rs:531-536` `project_events`

#### `dup-e22da6af6331` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-grounder/src/ingest.rs, crates/rigger-store-sqlite/src/eventstore/namespace.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-grounder/src/ingest.rs:271-273` `log_head`
- `crates/rigger-store-sqlite/src/eventstore/namespace.rs:171-173` `log_head`

#### `dup-e10b6988960c` (exact, 2 sites)

Proposed home: `ingest::folding_store`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-grounder/src/ingest.rs:283-285` `last_position`
- `crates/rigger-grounder/src/ingest.rs:315-317` `latest_in_group`

#### `dup-96daf6c3e468` (semantic, 9 sites)

Proposed home: `crates/rigger-process/src/reap.rs as the one /proc/<pid>/stat and /proc/<pid>/status parser, returning whichever field each caller needs, so dash.rs::process_state and reap.rs::pid_starttime/read_ppid stop each re-deriving the pid(comm)state... split`

mandatory sweep: /proc/<pid>/stat or /proc/<pid>/status field-extraction functions - 9 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-process/src/reap.rs:214-218` `stat_field_after_comm`
- `crates/rigger-process/src/reap.rs:242-248` `read_ppid`
- `tests/common/fixtures/host.rs:81-86` `pgid_of`
- `tests/simplification_audit.rs:2966-2997` `build_sweep_clusters`
- `tests/simplification_audit.rs:3241-3262` `build_extra_semantic_clusters`
- `tests/simplification_audit.rs:3591-3708` `render_adversarial_sample`
- `tests/simplification_audit.rs:5060-5614` `render_section_6`
- `tests/simplification_audit.rs:8288-8311` `three_near_but_not_identical_functions_cluster_as_near_with_every_site_and_one_home`
- `tests/simplification_audit.rs:8457-8462` `proc_literal_sweep_finds_a_proc_path_string_and_ignores_an_unrelated_one`

#### `dup-a8049539272a` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-process/src/reap.rs, src/cli/validate.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-process/src/reap.rs:881-888` `processes_rooted_under_is_a_graceful_no_op_when_the_base_is_absent`
- `src/cli/validate.rs:1923-1930` `leaked_process_advisories_is_a_graceful_no_op_when_the_scratch_root_is_absent`

#### `dup-183455104b30` (near, 3 sites)

Proposed home: `contract::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-store-sqlite/src/eventstore/contract.rs:918-938` `append_assigns_revisions`
- `crates/rigger-store-sqlite/src/eventstore/contract.rs:1047-1092` `backward_stream_read_reverses_set`
- `crates/rigger-store-sqlite/src/eventstore/contract.rs:1096-1121` `forward_stream_read_honors_nonzero_from`

#### `dup-4a57ceb2e317` (semantic, 2 sites)

Proposed home: `one shared `read_forward` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-store-sqlite/src/eventstore/kurrentdb.rs:616-623` `read_forward`
- `crates/rigger-store-sqlite/src/eventstore/sqlite.rs:543-560` `read_forward`

#### `dup-8bed086eddda` (exact, 2 sites)

Proposed home: `namespace::namespaced`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-store-sqlite/src/eventstore/namespace.rs:137-145` `read_stream`
- `crates/rigger-store-sqlite/src/eventstore/namespace.rs:189-199` `read_stream_typed`

#### `dup-f5125f873960` (exact, 2 sites)

Proposed home: `namespace::namespaced`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-store-sqlite/src/eventstore/namespace.rs:185-187` `last_position`
- `crates/rigger-store-sqlite/src/eventstore/namespace.rs:224-226` `latest_in_group`

#### `dup-b4990598bfd7` (exact, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-store-sqlite/src/eventstore/sqlite.rs:59-65` `group_index_sql`
- `crates/rigger-store-sqlite/src/eventstore/sqlite.rs:69-76` `latest_in_group_sql`

#### `dup-5cdeb5b1d1dd` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-store-sqlite/src/lockfile.rs, crates/rigger-worktree-git/src/worktree.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-store-sqlite/src/lockfile.rs:52-54` `drop`
- `crates/rigger-worktree-git/src/worktree.rs:1867-1869` `drop`

#### `dup-09fd5c38b65b` (near, 2 sites)

Proposed home: `progress_store::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-store-sqlite/src/progress_store.rs:47-61` `record_launch`
- `crates/rigger-store-sqlite/src/progress_store.rs:113-124` `record_stop_failure`

#### `dup-ad62aafad4f2` (semantic, 2 sites)

Proposed home: `one shared `launch_records` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-store-sqlite/src/progress_store.rs:96-107` `launch_records`
- `tests/claude_code_stream_periphery.rs:179-185` `launch_records`

#### `dup-10504155864a` (near, 2 sites)

Proposed home: `run_store::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-store-sqlite/src/run_store.rs:1574-1588` `ensure_started_adopts_the_same_criteria_run_without_re_minting`
- `crates/rigger-store-sqlite/src/run_store.rs:1643-1658` `ensure_started_mints_a_new_run_when_the_criteria_change`

#### `dup-2c13c330b0e0` (exact, 2 sites)

Proposed home: `spawn_store::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-store-sqlite/src/spawn_store.rs:115-120` `record_result_if_absent`
- `crates/rigger-store-sqlite/src/spawn_store.rs:131-136` `record_result_unless_ended`

#### `dup-90067fabd6a8` (near, 2 sites)

Proposed home: `worktree::worktree`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-worktree-git/src/worktree.rs:645-647` `commit`
- `crates/rigger-worktree-git/src/worktree.rs:663-665` `commit_checkpoint`

#### `dup-4efef9d45e3e` (exact, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-worktree-git/src/worktree.rs:2757-2765` `scratch_root`
- `crates/rigger-worktree-git/src/worktree.rs:2856-2864` `scratch_root_path`

#### `dup-e08d5a3a5d94` (exact, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-worktree-git/src/worktree.rs:2954-2957` `scratch_root_from_env`
- `crates/rigger-worktree-git/src/worktree.rs:2961-2964` `scratch_root_path_from_env`

#### `dup-5a3cb86b6dd7` (near, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-worktree-git/src/worktree.rs:3602-3622` `integrate_lands_work_in_the_repo`
- `crates/rigger-worktree-git/src/worktree.rs:5622-5645` `integrate_lands_a_pre_committed_artifact_unchanged`

#### `dup-4e9eb933164b` (near, 2 sites)

Proposed home: `graph::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/cli/graph.rs:776-800` `cmd_graph_communities`
- `src/cli/graph.rs:879-903` `cmd_graph_concepts`

#### `dup-6b5ddf643eb6` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/cli/guard.rs, src/cli/validate.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/cli/guard.rs:1326-1328` `parse_guard_write_roots_requires_at_least_one`
- `src/cli/validate.rs:2121-2123` `order_signature_advisories_is_empty_when_no_signatures_are_given`

#### `dup-db3e199dd312` (near, 7 sites)

Proposed home: `a new shared module (sites span 3 files: src/cli/hygiene.rs, tests/common/cli.rs, tests/compaction_generations_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/cli/hygiene.rs:257-259` `derived_count_phrase`
- `tests/common/cli.rs:1506-1508` `rebuilt_through_the_head`
- `tests/common/cli.rs:1512-1517` `pruned_from_the_rebuilt_graph`
- `tests/common/cli.rs:1790-1797` `migrated_lines`
- `tests/common/cli.rs:1804-1809` `derived_menu_line_naming`
- `tests/common/cli.rs:2490-2495` `bloat_advisory_naming`
- `tests/compaction_generations_periphery.rs:6561-6566` `runs_prunable`

#### `dup-4a813209194e` (near, 2 sites)

Proposed home: `hygiene::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/cli/hygiene.rs:2010-2030` `reset_modes_parses_force_live_alongside_derived_rejects_duplicates_and_never_implies_a_mode`
- `src/cli/hygiene.rs:2084-2105` `reset_modes_parses_build_cache_alone_and_composed_and_rejects_duplicates`

#### `dup-4b714a54ab5e` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/cli/hygiene.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/cli/hygiene.rs:2146-2155` `derived_count_phrase_names_the_events_first_and_the_file_identities_second`
- `tests/simplification_audit.rs:7405-7409` `pluralize_uses_singular_only_at_exactly_one`

#### `dup-e9f78fd7efcc` (exact, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/cli/mod.rs:1036-1038` `project_identity`
- `src/cli/mod.rs:5570-5572` `git_repo`

#### `dup-b7ec46dde247` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/cli/mod.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/cli/mod.rs:1481-1483` `find_store_dir_from`
- `tests/simplification_audit.rs:864-866` `scan_target_files`

#### `dup-7b4605d9ccc9` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/cli/mod.rs, tests/reset_build_cache_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/cli/mod.rs:4043-4062` `dir_size_bytes`
- `tests/reset_build_cache_periphery.rs:70-87` `dir_bytes`

#### `dup-41d8b1c33d44` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: src/cli/mod.rs, tests/cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/cli/mod.rs:5793-5800` `spec_lint_next_step_names_rigger_validate_and_the_given_spec_path`
- `tests/cli.rs:4756-4779` `native_driver_fixpoint_requires_done_and_an_empty_in_flight_set`
- `tests/cli.rs:4787-4800` `native_driver_drains_in_flight_workers_before_a_loud_stop`

#### `dup-da72fb2d69f3` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/cli/mod.rs:8245-8300` `reclaim_orphan_scratch_spares_a_non_live_worktree_that_is_still_dirty`
- `src/cli/mod.rs:8303-8342` `reclaim_orphan_scratch_spares_only_a_declared_dirty_worktree`

#### `dup-5e94519ad1f0` (exact, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/cli/mod.rs:11060-11071` `the_step_schema_admits_the_attention_array`
- `src/cli/mod.rs:12430-12433` `no_runs_message_points_at_rigger_run`

#### `dup-178d86dd6b19` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/cli/mod.rs, tests/cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/cli/mod.rs:11409-11418` `step_courier_prompt`
- `tests/cli.rs:3807-3816` `cmd_step_source`

#### `dup-03a673792149` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/cli/mod.rs:12698-12707` `result_of_at_unrecorded_spawn_reads_as_unreported`
- `src/cli/mod.rs:12745-12764` `result_of_at_is_namespace_scoped`

#### `dup-bd75ff76a467` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/cli/setup.rs, src/main.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/cli/setup.rs:312-319` `print_scaffold_pointer`
- `src/main.rs:436-438` `usage`

#### `dup-3f04af8603ff` (semantic, 2 sites)

Proposed home: `one shared `catch_up_stopped` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/cli/setup.rs:1275-1277` `catch_up_stopped`
- `tests/common/fixtures/cursor.rs:197-199` `catch_up_stopped`

#### `dup-2cd309ca2b08` (near, 2 sites)

Proposed home: `validate::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/cli/validate.rs:1584-1605` `git_is_ancestor_decides_commit_order_in_a_real_repo`
- `src/cli/validate.rs:1608-1629` `git_commit_distance_counts_commits_ahead_in_a_real_repo`

#### `dup-5f516fce9f0b` (near, 2 sites)

Proposed home: `adoption_keys_on_criterion_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/adoption_keys_on_criterion_periphery.rs:991-1067` `a_compensation_reverted_integration_reopens_adoption_of_its_real_still_existing_branch`
- `tests/adoption_keys_on_criterion_periphery.rs:1075-1137` `a_plain_remediation_failure_after_integration_never_reopens_adoption_even_though_a_same_named_branch_exists`

#### `dup-5dc33b6ca236` (near, 2 sites)

Proposed home: `build_env_authority_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/build_env_authority_periphery.rs:411-456` `one_build_environment_authority_reaches_a_real_gate_subprocess_and_a_real_agent_subprocess`
- `tests/build_env_authority_periphery.rs:581-614` `auto_wrapper_resolves_to_a_real_probed_binary_and_reaches_both_real_subprocesses`

#### `dup-14d812231784` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/canary_false_positives_periphery.rs, tests/canary_unattributed_rejects_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/canary_false_positives_periphery.rs:62-129` `spawn`
- `tests/canary_unattributed_rejects_periphery.rs:61-116` `spawn`

#### `dup-cf375e76ff60` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/canary_false_positives_periphery.rs, tests/canary_unattributed_rejects_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/canary_false_positives_periphery.rs:139-267` `run_canary_scores_false_positive_controls_and_project_canary_counts_them`
- `tests/canary_unattributed_rejects_periphery.rs:126-260` `run_canary_scores_an_unattributed_correct_reject_and_project_canary_counts_only_it`

#### `dup-71aadfa4568e` (near, 14 sites)

Proposed home: `a new shared module (sites span 3 files: tests/cause_wire_periphery.rs, tests/cli.rs, tests/escalation_resume_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cause_wire_periphery.rs:109-130` `a_legacy_causeless_unit_failed_event_survives_a_real_store_round_trip_and_renders_unknown`
- `tests/cause_wire_periphery.rs:137-168` `the_reject_recurrence_line_names_the_latest_of_several_recorded_causes_through_the_real_binary`
- `tests/cli.rs:15650-15687` `stats_reports_the_latest_run_by_default_and_all_for_the_aggregate`
- `tests/cli.rs:15942-15956` `stats_cli_reports_no_recorded_spawns_when_the_run_has_none`
- `tests/cli.rs:28012-28070` `release_ready_handoff_surfaces_on_status_for_a_done_run`
- `tests/cli.rs:28271-28311` `release_ready_is_silent_on_status_for_an_unfinished_run`
- `tests/cli.rs:28381-28413` `release_ready_pluralizes_the_unit_count_on_status_for_a_multi_unit_run`
- `tests/cli.rs:28457-28478` `release_ready_is_silent_on_status_for_a_spec_defective_run`
- `tests/escalation_resume_periphery.rs:85-108` `a_unit_resumed_event_seeded_directly_through_a_real_store_reaches_status_without_the_command`
- `tests/escalation_resume_periphery.rs:119-147` `a_legacy_shaped_unit_resumed_event_missing_both_optional_fields_survives_a_real_store_round_trip`
- `tests/escalation_resume_periphery.rs:235-265` `the_resumed_banner_survives_a_genuinely_in_flight_re_parked_attempt_not_yet_resolved`
- `tests/escalation_resume_periphery.rs:313-333` `resume_unit_rejects_an_unknown_unit_absent_from_the_run`
- `tests/escalation_resume_periphery.rs:340-363` `resume_unit_refuses_when_the_recorded_branch_was_never_created`
- `tests/escalation_resume_periphery.rs:370-392` `resume_unit_refuses_an_already_integrated_unit`

#### `dup-5e768c7156e7` (near, 26 sites)

Proposed home: `a new shared module (sites span 6 files: tests/cli.rs, tests/graph_show_periphery.rs, tests/reset_build_cache_periphery.rs, tests/reset_derived_live_writer_guard_periphery.rs, tests/statusline_command_periphery.rs, tests/watchdog_cli_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:1446-1463` `scratch_requires_exactly_one_spawn_id`
- `tests/cli.rs:1574-1585` `scratch_refuses_a_degenerate_empty_spawn_id`
- `tests/cli.rs:1817-1834` `result_prints_an_orphan_advisory_for_an_unrecorded_id`
- `tests/cli.rs:2831-2843` `graph_build_exits_clean_and_creates_the_store_in_both_lanes`
- `tests/cli.rs:3629-3656` `emit_rejects_bad_json_with_a_nonzero_exit`
- `tests/cli.rs:5721-5742` `resume_unit_defaults_to_granting_one_attempt_when_attempts_is_omitted`
- `tests/cli.rs:5745-5759` `resume_unit_refuses_an_unknown_unit`
- `tests/cli.rs:17364-17374` `run_accepts_a_base_flag`
- `tests/cli.rs:21076-21096` `result_if_absent_records_when_the_spawn_is_unanswered`
- `tests/cli.rs:21629-21638` `stats_canary_on_a_project_with_no_canary_run_says_so`
- `tests/cli.rs:21794-21823` `canary_rejects_unknown_arguments_and_a_missing_corpus`
- `tests/cli.rs:21895-21909` `canary_accepts_a_jobs_flag_alongside_other_flags`
- `tests/cli.rs:22359-22394` `canary_rejects_a_malformed_or_unknown_tier_model_pin`
- `tests/cli.rs:24002-24014` `status_json_appends_no_dashboard_entry_when_none_was_ever_recorded`
- `tests/cli.rs:32343-32360` `prime_given_a_spec_path_names_the_spec_lint_as_a_next_step`
- `tests/cli.rs:32363-32385` `prime_given_a_spec_path_names_the_spec_lint_alongside_recent_decisions`
- `tests/cli.rs:33959-33991` `mcp_rejects_a_malformed_spawn_flag_or_unexpected_arguments`
- `tests/graph_show_periphery.rs:112-129` `graph_show_and_around_with_no_store_print_the_not_found_note`
- `tests/reset_build_cache_periphery.rs:118-144` `reset_build_cache_is_idempotent_zero_report_on_a_project_that_never_built_anything`
- `tests/reset_build_cache_periphery.rs:182-192` `reset_build_cache_flag_is_registered_and_rejects_a_duplicate`
- `tests/reset_derived_live_writer_guard_periphery.rs:1664-1674` `reset_force_live_alone_is_refused_as_no_mode`
- `tests/statusline_command_periphery.rs:95-105` `status_line_on_a_clean_run`
- `tests/statusline_command_periphery.rs:141-151` `status_line_and_json_are_mutually_exclusive`
- `tests/watchdog_cli_periphery.rs:131-141` `watch_once_on_a_freshly_initialized_store_reports_nothing_and_exits_cleanly`
- `tests/watchdog_cli_periphery.rs:222-236` `watch_refuses_a_project_with_no_rigger_store_at_all_through_the_real_binary`
- `tests/watchdog_cli_periphery.rs:243-256` `watch_rejects_an_unknown_flag_through_the_real_binary_with_a_nonzero_exit`

#### `dup-4cd3708b516f` (exact, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:1886-1888` `initialized_project`
- `tests/cli.rs:1907-1909` `initialized_git_project`

#### `dup-9da3f693c2b7` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/cli.rs, tests/worker_persona_label_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:4507-4563` `native_driver_scratch_policy_directs_the_worker_to_its_own_spawn_owned_container`
- `tests/worker_persona_label_periphery.rs:331-352` `workerlabel_is_actually_wired_into_runworkers_agent_call_label`

#### `dup-9418a9e1c36a` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:14221-14254` `a_budget_halt_does_not_restamp_on_a_later_real_step_with_nothing_new`
- `tests/cli.rs:14967-15010` `step_attention_never_restamps_a_hung_unbounded_spawn_when_repo_less`

#### `dup-ea8a80f3da16` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/cli.rs, tests/courier_registry_refresh_boundary_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:15550-15553` `run_teardown_spares_run_level_scratch_at_a_manual_review_pause`
- `tests/courier_registry_refresh_boundary_periphery.rs:194-197` `an_ambient_kurrentdb_conn_never_leaks_into_a_boundary_courier`

#### `dup-edb646ecd5e8` (exact, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:18756-18758` `path_with_fake_sccache`
- `tests/cli.rs:26817-26819` `stage_rigger_shim`

#### `dup-3cfa6389efb7` (near, 4 sites)

Proposed home: `cli::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:20612-20643` `init_reports_the_positive_summary_then_is_a_quiet_noop`
- `tests/cli.rs:24124-24135` `status_prints_no_dashboard_line_when_none_was_ever_recorded`
- `tests/cli.rs:32330-32340` `prime_with_no_spec_path_never_mentions_the_spec_lint`
- `tests/cli.rs:32751-32760` `workflow_with_no_spec_path_never_mentions_the_spec_lint`

#### `dup-ceea6ddc753f` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:24425-24455` `docs_ships_three_verb_lookup_guidance_to_consumers`
- `tests/cli.rs:35915-35965` `docs_installs_the_operator_lookup_rule_text_into_the_shipped_skill_and_handbook`

#### `dup-21b08556e493` (exact, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:27423-27428` `stage_failing_docs_rigger_shim`
- `tests/cli.rs:27450-27471` `stage_stale_rigger_shim`

#### `dup-4fbc57c56116` (near, 2 sites)

Proposed home: `code_ingest_events::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_ingest_events.rs:1063-1104` `a_refs_only_files_re_extraction_supersedes_via_the_reference_batch_boundary`
- `tests/code_ingest_events.rs:1107-1161` `a_re_extraction_supersedes_only_its_own_files_edges_not_another_files_reference`

#### `dup-d46432042e0c` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/code_lens_view_periphery.rs, tests/concepts_lens_view_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_lens_view_periphery.rs:82-108` `lens_graph`
- `tests/concepts_lens_view_periphery.rs:92-119` `lens_graph`

#### `dup-9c8fbf2fd88a` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/code_lens_view_periphery.rs, tests/concepts_lens_view_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_lens_view_periphery.rs:111-115` `code_default`
- `tests/concepts_lens_view_periphery.rs:122-126` `concepts_default`

#### `dup-14f55a46039c` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/code_lens_view_periphery.rs, tests/concepts_lens_view_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_lens_view_periphery.rs:125-158` `lens_from_query_is_a_public_total_selector_that_falls_back_to_files`
- `tests/concepts_lens_view_periphery.rs:137-165` `lens_from_query_is_a_public_total_selector_including_concepts`

#### `dup-2a101767f24d` (near, 4 sites)

Proposed home: `a new shared module (sites span 2 files: tests/common/cli.rs, tests/courier_registry_refresh_boundary_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/common/cli.rs:50-54` `courier_project`
- `tests/common/cli.rs:594-598` `temp_rigger_project`
- `tests/common/cli.rs:602-606` `temp_store_project`
- `tests/courier_registry_refresh_boundary_periphery.rs:40-44` `courier_project_with_commit`

#### `dup-30d8bd7871ae` (near, 5 sites)

Proposed home: `a new shared module (sites span 4 files: tests/common/cli.rs, tests/common/fixtures/cursor.rs, tests/common/fixtures/spawn.rs, tests/spec_critique_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/common/cli.rs:1524-1526` `emit_folded_line`
- `tests/common/cli.rs:1830-1832` `reclaimed_line`
- `tests/common/fixtures/cursor.rs:173-175` `caught_up_line`
- `tests/common/fixtures/spawn.rs:282-284` `sleeping`
- `tests/spec_critique_periphery.rs:2328-2330` `gate_ceiling`

#### `dup-90e32eb50d95` (exact, 4 sites)

Proposed home: `cli::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/common/cli.rs:1815-1817` `derived_menu_lines`
- `tests/common/cli.rs:1820-1822` `decision_lines`
- `tests/common/cli.rs:2484-2486` `index_lag_lines`
- `tests/common/cli.rs:2499-2501` `bloat_lines`

#### `dup-82bff055dbd1` (exact, 5 sites)

Proposed home: `a new shared module (sites span 2 files: tests/common/cli.rs, tests/common/fixtures/cursor.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/common/cli.rs:2008-2010` `plant_stale_marker`
- `tests/common/cli.rs:2214-2216` `declare_grep_guard`
- `tests/common/cli.rs:2220-2222` `declare_sdet_author`
- `tests/common/fixtures/cursor.rs:143-145` `drop_the_cursor`
- `tests/common/fixtures/cursor.rs:159-161` `drop_the_cursor_s_log`

#### `dup-8dd44d643a5c` (semantic, 2 sites)

Proposed home: `one shared `write_workflow` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/common/cli.rs:2275-2282` `write_workflow`
- `tests/common/workflow_probe.rs:21-23` `write_workflow`

#### `dup-b58b0ff72b22` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: tests/common/cli.rs, tests/store_secrets.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/common/cli.rs:2311-2331` `assert_selected_server`
- `tests/common/cli.rs:2335-2351` `assert_selected_sqlite`
- `tests/store_secrets.rs:69-107` `assert_server_reached_and_credentials_redacted`

#### `dup-c82a46fd1fec` (exact, 2 sites)

Proposed home: `events::silent_store`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/common/fixtures/events.rs:293-300` `read_stream`
- `tests/common/fixtures/events.rs:333-340` `read_stream_typed`

#### `dup-3eb116c3c4c8` (exact, 2 sites)

Proposed home: `events::silent_store`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/common/fixtures/events.rs:330-332` `last_position`
- `tests/common/fixtures/events.rs:358-360` `latest_in_group`

#### `dup-033af83f765c` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/common/fixtures/events.rs, tests/store_content_identity_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/common/fixtures/events.rs:341-348` `read_stream_positions`
- `tests/store_content_identity_periphery.rs:262-269` `read_stream_positions`

#### `dup-6ef712aab260` (exact, 2 sites)

Proposed home: `events::read_counting_store`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/common/fixtures/events.rs:1088-1096` `last_position`
- `tests/common/fixtures/events.rs:1155-1163` `latest_in_group`

#### `dup-aa4a3a0dfc34` (near, 2 sites)

Proposed home: `fold::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/common/fixtures/fold.rs:301-303` `alias_json`
- `tests/common/fixtures/fold.rs:1017-1019` `ref_json`

#### `dup-c3c8358993cf` (near, 2 sites)

Proposed home: `fold::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/common/fixtures/fold.rs:1225-1227` `apply_ref`
- `tests/common/fixtures/fold.rs:1255-1257` `apply_call`

#### `dup-9c3b5c80063e` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/common/fixtures/fold.rs, tests/community_resolution_knob.rs, tests/graph_superseded_prune.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/common/fixtures/fold.rs:1575-1584` `live_node_ids`
- `tests/community_resolution_knob.rs:139-148` `live_memberships_of`
- `tests/graph_superseded_prune.rs:32-41` `live_contains`

#### `dup-088ec120e4ac` (near, 2 sites)

Proposed home: `host::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/common/fixtures/host.rs:91-98` `sigterm_ignorer_in`
- `tests/common/fixtures/host.rs:102-108` `sleeper_in`

#### `dup-c99581d3e41e` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/common/workflow_probe.rs, tests/store_precedence.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/common/workflow_probe.rs:21-23` `write_workflow`
- `tests/store_precedence.rs:48-50` `write_store_conn`

#### `dup-6b847cf11fc4` (semantic, 2 sites)

Proposed home: `one shared `seed_coupling` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/community_detection_cli.rs:88-109` `seed_coupling`
- `tests/community_fold_periphery.rs:40-50` `seed_coupling`

#### `dup-c51171ae2133` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_fold_periphery.rs, tests/concepts_fold_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_fold_periphery.rs:170-203` `a_community_assigned_event_without_a_fresh_key_is_a_non_boundary_and_never_supersedes`
- `tests/concepts_fold_periphery.rs:190-235` `a_concept_derived_event_without_a_fresh_key_is_a_non_boundary_and_never_supersedes`

#### `dup-36ebafe7bb7e` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/compaction_generations_periphery.rs, tests/one_shot_reads_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/compaction_generations_periphery.rs:3312-3318` `holds_table`
- `tests/one_shot_reads_periphery.rs:310-312` `poison`

#### `dup-515218e72204` (near, 2 sites)

Proposed home: `concepts_derivation_cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/concepts_derivation_cli.rs:59-67` `doc`
- `tests/concepts_derivation_cli.rs:72-77` `link`

#### `dup-f4a922d43ae9` (semantic, 2 sites)

Proposed home: `one shared `exploration_graph` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/dash_exploration_route_client_contract.rs:67-117` `exploration_graph`
- `tests/dash_kg_graph_route.rs:1141-1152` `exploration_graph`

#### `dup-c49b09543e58` (semantic, 3 sites)

Proposed home: `one shared `fixture_graph` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/dash_kg_graph_route.rs:56-68` `fixture_graph`
- `tests/graph_query_engine_relocation_periphery.rs:53-72` `fixture_graph`
- `tests/metadata_card_periphery.rs:49-100` `fixture_graph`

#### `dup-18409566d0bb` (near, 2 sites)

Proposed home: `dash_run_tree_spine::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dash_run_tree_spine.rs:590-632` `an_escalated_units_gate_failure_is_not_masked_by_a_trailing_passing_gate`
- `tests/dash_run_tree_spine.rs:693-744` `a_regated_green_unit_renders_gates_passed_despite_an_earlier_failed_attempt`

#### `dup-626b09960195` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/dead_code_json_contract_periphery.rs, tests/duplication_catalog_contract_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dead_code_json_contract_periphery.rs:275-293` `every_deserialized_test_only_reference_has_a_non_empty_file_and_content_hash`
- `tests/duplication_catalog_contract_periphery.rs:150-174` `every_deserialized_site_has_a_non_empty_file_name_and_content_hash`

#### `dup-c06e1c41c352` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/dead_code_json_contract_periphery.rs, tests/responsibility_map_contract_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dead_code_json_contract_periphery.rs:620-640` `section_4_3_citations`
- `tests/responsibility_map_contract_periphery.rs:282-303` `section_1_citations`

#### `dup-e4e39c11c4c2` (near, 2 sites)

Proposed home: `graph_denoise_target_project::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_denoise_target_project.rs:36-188` `a_whole_runs_fold_projects_the_content_but_none_of_the_machinery`
- `tests/graph_denoise_target_project.rs:191-258` `the_actor_metadata_on_a_decision_and_finding_never_folds_an_agent_node`

#### `dup-fa251330d936` (near, 4 sites)

Proposed home: `a new shared module (sites span 4 files: tests/graph_density_spread_floor_and_centring.rs, tests/readable_graph_adaptive_labels.rs, tests/readable_graph_density_scaled_spacing.rs, tests/readable_graph_layout_separation.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_density_spread_floor_and_centring.rs:85-92` `the_served_page_wires_the_layered_path_without_the_density_accessors`
- `tests/readable_graph_adaptive_labels.rs:51-77` `the_served_page_ships_the_adaptive_label_declutter`
- `tests/readable_graph_density_scaled_spacing.rs:42-59` `the_served_page_ships_the_density_spread_lever`
- `tests/readable_graph_layout_separation.rs:35-55` `the_served_page_ships_the_collision_separation_pass`

#### `dup-0f2d7b120d84` (near, 2 sites)

Proposed home: `graph_fold_dedup_live_edge::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_fold_dedup_live_edge.rs:35-73` `subgraph_collapses_repeated_governs_to_one_live_edge_keeping_latest_provenance`
- `tests/graph_fold_dedup_live_edge.rs:76-97` `the_collapsed_edge_keeps_the_earliest_fact_time_and_latest_source_regardless_of_arrival_order`

#### `dup-cb84d409b08d` (near, 14 sites)

Proposed home: `a new shared module (sites span 2 files: tests/graph_show_periphery.rs, tests/graph_show_staleness.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_show_periphery.rs:200-255` `graph_show_degrades_to_stale_note_when_location_drifted`
- `tests/graph_show_periphery.rs:265-307` `graph_show_degrades_to_stale_note_when_recorded_line_is_zero`
- `tests/graph_show_periphery.rs:337-371` `graph_show_light_lane_degrades_to_extent_unavailable_note`
- `tests/graph_show_periphery.rs:380-432` `graph_show_bounds_body_at_the_definitions_own_extent`
- `tests/graph_show_periphery.rs:444-525` `graph_show_shows_full_body_past_nested_definition`
- `tests/graph_show_periphery.rs:536-584` `graph_show_shows_full_body_of_a_destructuring_signature`
- `tests/graph_show_periphery.rs:592-634` `graph_show_extent_ignores_braces_in_strings_comments_and_chars`
- `tests/graph_show_periphery.rs:644-718` `graph_show_shows_full_body_of_a_python_nested_def`
- `tests/graph_show_periphery.rs:729-769` `graph_show_does_not_overread_a_js_single_quote_brace_body`
- `tests/graph_show_periphery.rs:840-893` `graph_show_degrades_when_no_grammar_registered_for_the_file_extension`
- `tests/graph_show_periphery.rs:908-968` `graph_show_heals_to_the_live_line_when_the_moved_name_is_unambiguous`
- `tests/graph_show_periphery.rs:978-1030` `graph_show_degrades_when_the_moved_name_is_ambiguous_in_the_file`
- `tests/graph_show_staleness.rs:42-86` `graph_show_degrades_gracefully_when_the_recorded_file_is_missing`
- `tests/graph_show_staleness.rs:104-182` `graph_show_never_presents_a_neighbours_body_when_the_line_drifted`

#### `dup-fbfd8846ef05` (near, 2 sites)

Proposed home: `integrate_conflict_merge_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/integrate_conflict_merge_periphery.rs:518-583` `spawn`
- `tests/integrate_conflict_merge_periphery.rs:1473-1514` `spawn`

#### `dup-333ec555460f` (near, 2 sites)

Proposed home: `integrate_conflict_merge_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/integrate_conflict_merge_periphery.rs:2042-2106` `a_crash_right_after_the_merge_attempt_record_resumes_and_completes_row_1`
- `tests/integrate_conflict_merge_periphery.rs:2110-2174` `a_crash_right_after_the_landing_intent_record_resumes_and_completes_row_4`

#### `dup-8fb3e9b78321` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/migration_is_deliberate_periphery.rs, tests/validate_advisories.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/migration_is_deliberate_periphery.rs:520-529` `validate_is_silent_on_retired_code_entities_when_nothing_has_been_retired`
- `tests/validate_advisories.rs:659-668` `validate_is_silent_on_a_gated_scaffolded_fanout_template`

#### `dup-8f0f2622431b` (exact, 2 sites)

Proposed home: `native_driver_pipelining_behavior::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/native_driver_pipelining_behavior.rs:362-370` `fast_units_review_runs_while_the_slow_sibling_still_builds`
- `tests/native_driver_pipelining_behavior.rs:520-527` `a_worker_settling_during_the_courier_step_wakes_the_loop_and_never_reads_as_a_false_stop`

#### `dup-b4d4329417cc` (exact, 2 sites)

Proposed home: `native_driver_pipelining_behavior::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/native_driver_pipelining_behavior.rs:635-642` `every_spawn_with_a_marker_path_is_told_to_keep_it_fresh`
- `tests/native_driver_pipelining_behavior.rs:669-676` `the_worker_preamble_stops_on_an_ended_spawn`

#### `dup-d5a2e2020223` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/no_os_kill_audit.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_audit.rs:374-377` `never_flagged`
- `tests/simplification_audit.rs:8702-8708` `assert_no_same_named_helper_cluster`

#### `dup-d558d72a9443` (near, 4 sites)

Proposed home: `reap_before_removal_audit::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reap_before_removal_audit.rs:744-757` `bare_remove_dir_all_with_no_coverage_is_caught`
- `tests/reap_before_removal_audit.rs:760-776` `bare_git_worktree_remove_with_no_coverage_is_caught`
- `tests/reap_before_removal_audit.rs:1108-1122` `a_finding_names_its_exact_file_and_line_number`
- `tests/reap_before_removal_audit.rs:1237-1264` `a_worktree_remove_args_array_wrapped_across_multiple_lines_by_rustfmt_is_still_caught`

#### `dup-a199914ac1c3` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/review_round_lenses_only_log_derived_resume_periphery.rs, tests/review_round_non_ancestor_residue_names_true_diff_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/review_round_lenses_only_log_derived_resume_periphery.rs:80-105` `spawn`
- `tests/review_round_non_ancestor_residue_names_true_diff_periphery.rs:72-103` `spawn`

#### `dup-cef19c011c6b` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/simplification_audit.rs, tests/spec_critique_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:1845-1847` `placeholder_section`
- `tests/spec_critique_periphery.rs:2334-2336` `ceiling_notice`

#### `dup-e0f2896fa7de` (exact, 3 sites)

Proposed home: `simplification_audit::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:3399-3402` `real_files`
- `tests/simplification_audit.rs:4907-4910` `real_map`
- `tests/simplification_audit.rs:6813-6816` `real_workspace_files`

#### `dup-23cb89af01e7` (near, 4 sites)

Proposed home: `simplification_audit::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:3457-3474` `dup_cluster_wire`
- `tests/simplification_audit.rs:3476-3489` `dup_cluster_lines`
- `tests/simplification_audit.rs:6766-6783` `dead_code_candidate_wire`
- `tests/simplification_audit.rs:6785-6800` `dead_code_candidate_lines`

#### `dup-adfce3b42b86` (near, 2 sites)

Proposed home: `step_sheds_the_freshen::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/step_sheds_the_freshen.rs:128-163` `fingerprint_subtree`
- `tests/step_sheds_the_freshen.rs:129-159` `walk`

#### `dup-56fda398784b` (semantic, 2 sites)

Proposed home: `one shared `write_store_conn` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/store_precedence.rs:48-50` `write_store_conn`
- `tests/store_secrets.rs:54-63` `write_store_conn`

#### `dup-5f1cbfb1ea59` (near, 3 sites)

Proposed home: `store_resolution_cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/store_resolution_cli.rs:56-90` `a_server_selected_courier_reaches_the_server_and_never_fabricates_local_sqlite`
- `tests/store_resolution_cli.rs:93-120` `a_courier_with_no_server_configured_resolves_the_local_sqlite_log`
- `tests/store_resolution_cli.rs:123-147` `an_empty_kurrentdb_conn_is_treated_as_unset_not_a_server_with_no_address`

#### `dup-498b51115191` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: tests/subject_lens_defined_cells.rs, tests/subject_lens_reprojection_contract.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/subject_lens_defined_cells.rs:73-109` `communities_no_concepts_graph`
- `tests/subject_lens_defined_cells.rs:219-256` `mixed_membership_graph`
- `tests/subject_lens_reprojection_contract.rs:322-373` `file_over_code_graph`

#### `dup-06ffd3c67857` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/subject_lens_defined_cells_contract.rs, tests/subject_lens_reprojection_contract.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/subject_lens_defined_cells_contract.rs:238-254` `full_in_budget_graph`
- `tests/subject_lens_reprojection_contract.rs:87-134` `community_over_concepts_graph`

#### `dup-659554f6286f` (near, 2 sites)

Proposed home: `workflow_definition_and_js_constants_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/workflow_definition_and_js_constants_periphery.rs:90-124` `seed_workflow_yml`
- `tests/workflow_definition_and_js_constants_periphery.rs:132-147` `seed_js_files`

### Adversarial sample

Recall check (spec 85 THOROUGHNESS): 30 functions drawn by seeded rank (seed `85072026`, `sample_indices` over all 11470 functions scanned in `src/` and `tests/`, each ranked by the seeded hash of its own file, name and ordinal so a change elsewhere never reshuffles a drawn row, excluding `tests/prioritized_plan_citation_periphery.rs` - criterion 4's own citation-guard periphery test, whose function count grows as its citation-drift-guard mechanism hardens round over round; excluding it keeps that unrelated growth out of the draw), each read by hand - together with its host file's surrounding context, since a duplicate can live anywhere in the file or a sibling file - to judge whether a duplicate exists that the mechanical pass and the five sweeps above did not already catch.

- `crates/rigger-conductor/src/conductor.rs:16098-16120` `branch_is_foreign_when_only_one_axis_differs` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `crates/rigger-conductor/src/conductor.rs:17083-17090` `resume_of` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `crates/rigger-conductor/src/conductor.rs:18014-18042` `a_run_with_no_upheld_note_runs_its_checkin_stage_as_before` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `crates/rigger-conductor/src/conductor.rs:29118-29152` `route_review_tier_fails_safe_to_full_on_an_empty_blast_radius` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `crates/rigger-conductor/src/conductor.rs:40716-40746` `a_budget_refused_standalone_review_spawn_keeps_its_worktree` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `crates/rigger-domain/src/canary.rs:283-317` `latest_run_scopes_to_the_last_batch_marker` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `crates/rigger-domain/src/review.rs:925-962` `a_finding_line_has_five_pipe_fields_with_the_severity_second` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `crates/rigger-domain/src/spec.rs:2249-2256` `strip_inline_code_direct_exact_output_pins_a_zero_width_quote_pair` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `crates/rigger-driver/src/liveness.rs:2629-2655` `classify_stale_weighs_the_silence_outside_the_hold_intervals` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:7142-7144` `locked` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:17923-17943` `one_transaction_resolves_an_owed_name_once_where_one_per_reference_resolves_it_each_time` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `crates/rigger-grounder/src/grounder/symbols/events.rs:681-693` `normalize_logical_path` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `crates/rigger-grounder/src/grounder/symbols/extract.rs:1280-1285` `class_fields` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `crates/rigger-grounder/src/grounder/symbols/extract.rs:1702-1710` `go_receiver` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `crates/rigger-grounder/src/ingest.rs:1918-1937` `answered` - duplicate found by reading and closed: its view of a sink answer's batch is `wire_owned`; the root sink suite's `Handed::answer` re-rolled that view as its own `typed` and now calls `wire_owned` too
- `crates/rigger-worktree-git/src/worktree.rs:2042-2064` `reclaim_cache_sibling` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `src/cli/mod.rs:4739-4765` `footprint_reclaim_lines` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `src/cli/mod.rs:9034-9044` `parse_run_args_reads_rebase_definition` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `src/cli/run.rs:1597-1599` `pause_a_hold_poll` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `src/cli/run.rs:2486-2510` `start_run_dashboard` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `tests/change_path_revert_periphery.rs:213-323` `a_revert_through_the_shipped_build_records_the_earlier_generations_entry_again` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `tests/code_entity_test_exclusion_periphery.rs:1810-1822` `edges_where` - duplicate found by reading and closed: the design-doc assertion, the dev-only path test and the test-region test each re-rolled its `(from, rel, to)` edge projection inline; all now call `edges_where`
- `tests/common/fixtures/events.rs:957-959` `after_call` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `tests/common/fixtures/events.rs:974-976` `reads` - caught: `dup-ebee743f02de`
- `tests/common/fixtures/events.rs:1225-1227` `reads` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `tests/concepts_labels_membership.rs:240-267` `label_of_the_documentless_hub` - no duplicate found by reading
- `tests/event_classes_are_one_table.rs:390-404` `a_right_side_that_is_neither_a_literal_nor_a_type_alias_fails_the_scan_naming_it` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `tests/one_shot_reads_periphery.rs:87-140` `a_project_namespace_over_a_shared_events_file_reads_its_run_as_one_typed_read_per_selection` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `tests/spec_critique_periphery.rs:1454-1479` `a_default_review_listing_no_adversary_step_has_no_critic` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `tests/worker_persona_label_periphery.rs:246-267` `the_subject_is_the_titles_first_sentence_passed_whole_with_no_truncation` - duplicate found by reading and closed: it re-rolled `assert_worker_label` inline; it now calls it

Reading pass 2026-09-27: 4 of the 30 drawn functions read by hand (a caught row too, to judge whether its duplicate reaches past the cluster); 22 duplicate(s) found by reading, each closed.
- closed before the redraw: `tests/kurrentdb_always_available.rs` `architecture_blueprint_has_no_retired_kurrentdb_feature_reference` re-rolled the manifest-relative read `common::repo::repo_text` owns, as did thirteen sibling reads across eleven suites; every one now calls `repo_text`
- closed before the redraw: `tests/replan_episode_identity.rs` `serving` had two inline copies of its scan in its own file; both now call it
- closed before the redraw: `crates/rigger-conductor/src/conductor.rs` `integrate_and_emit` repeated `catch_up_owed_regeneration`'s regenerate/record/clear sequence twice; all three now call `regenerate_and_record`
- closed before the redraw: `tests/dash_run_tree_spine.rs` `an_off_linear_unit_with_no_gate_verdict_renders_no_phantom_gates_passed` repeated `a_crashed_implementer_renders_no_gates_node`'s crash-and-no-Gates assertions; both now call `assert_crash_at_implement_and_no_gates`
- closed before the redraw: `src/main.rs` `dash_read_liveness` repeated `liveness_ages_for_wave`'s marker-age loop, as did `rigger_activity` in `crates/rigger-dash/src/mcpserver.rs`; all three now call `liveness::marker_ages`
- closed before the redraw: `src/main.rs` `merge_hung_attention_defers_to_an_existing_budget_halt` was a value-only copy of `merge_hung_attention_does_nothing_when_not_newly_hung`; both are now cases of `assert_merge_hung_attention_leaves_untouched`
- closed before the redraw: `tests/code_entity_test_exclusion_periphery.rs` `a_pre_round9_persisted_index_with_no_enclosing_inline_module_path_key_loads_defaulting_to_none` repeated its round-6 and round-7 siblings' legacy fixture; all three now call `legacy_module_def`
- closed before the redraw: `tests/product_binary_location.rs` `no_suite_bakes_the_product_path_at_compile_time_except_the_one_authority` re-rolled the tree walk `common::repo::for_each_rs_file` owns; it now calls it
- closed before the redraw: `tests/simplification_audit.rs` `two_renamed_identical_functions_form_one_exact_cluster` and `build_sweep_clusters_always_returns_exactly_five_named_clusters` re-rolled `on_fixture`, as did eleven sibling tests; all thirteen now call it
- closed before the redraw: `tests/stop_failure_hook_periphery.rs` `hook_refuses_naming` was repeated inline by `hook_stop_failure_rejects_an_unrecognized_class`; it now calls it
- closed before the redraw: `crates/rigger-conductor/src/conductor.rs` `proposal_event` was re-rolled inline by `a_same_episode_re_seen_on_a_later_fold_still_never_self_supersedes`; it now calls it and `harvest_seeded`
- closed before the redraw: `src/contextgraph/mod.rs` `is_false` re-implemented `std::ops::Not::not`, which the symbol model already uses; every flag now skips through `Not::not`
- closed before the redraw: `src/eventstore/mod.rs` `one` was bypassed by the concurrent contract append's `.last().expect(..)`; it now calls `Appended::one`
- closed before the redraw: `crates/rigger-store-sqlite/src/eventstore/sqlite.rs` `a_rerun_reclaims_the_space_a_failed_reclamation_left_behind` carried its own copies of the periphery suite's `plant_free_pages` and `pragma_i64`; both now live once in the shared store fixtures
- closed before the redraw: `crates/rigger-grounder/src/grounder/workflowdef.rs` `full_reviewers_of` repeated the head of `ReviewPanel::agent_ids`; both now call `ReviewPanel::full_roster`, and every adversary/adjudicator pair goes through `config::keyed_steps`
- closed before the redraw: `tests/common/fixtures/graph.rs` `summarized_node` was re-rolled as an inline closure by `tests/rationale_overlay_data.rs`; it now calls it
- closed before the redraw: `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs` `tier_default_matches_the_extracted_const` was repeated inline by `tests/code_ingest_events.rs`; the one test now pins all three tier consts
- closed before the redraw: `src/worktree.rs` `remove_reaps_a_process_rooted_inside_the_worktree_and_spares_one_outside` and `tests/reap_before_removal_periphery.rs` `reaps_before_removing` each re-rolled the spawn/wait/teardown/assert reap proof, as did five siblings in `src/main.rs`, `crates/rigger-process/src/reap.rs`, `src/worktree.rs` and the relocated-scratch periphery suite; all now call `assert_teardown_reaps_what_is_rooted_inside`
- closed before the redraw: `tests/heartbeat_write_read_agree_periphery.rs` `watch_once_suppresses_a_false_dead_driver_when_the_configured_workdir_resolves_from_the_owning_root_with_no_agents_fleet` repeated its two siblings' configured-workdir fixture; all three now call `marker_under_a_configured_workdir`

Two real recall gaps surfaced this way and were closed by widening the mechanical sweep with a new generalizable detector each - not a one-off citation - so the fix catches every present and future instance of its class, each pinned by a real-tree regression test: `find_proc_stat_or_status_readers` (decision `u85c2-proc-stat-worked-example`) groups every function reading a `/proc/<pid>/stat` or `/proc/<pid>/status` literal, closing the spec's own named worked example - `crates/rigger-dash/src/dash.rs:717-719` `process_state` next to `crates/rigger-process/src/reap.rs:225-230` `pid_starttime`, the same job on the same file with a different field/shape, upheld at spec 62's capstone; `find_parallel_constructor_clusters` (decision `u85c2-parallel-constructor-sweep`) groups 2+ non-test functions per `(file, Self type)` that build a `Self { .. }` / `TypeName { .. }` literal, closing `src/spawn.rs`'s `SpawnResult::liveness_fault` reading MISSING from its own `ok`/`failed` cluster even though all three are parallel constructors for one struct. A third worked example, `exploration_graph` (independently defined test-fixture builders in `tests/dash_exploration_route_client_contract.rs` and `tests/dash_kg_graph_route.rs`), was already caught correctly by the plain Jaccard pass with no sweep needed - confirming the mechanical pass itself has real recall, not only the two widened sweeps. Two further real defects, found on review rather than in this draw, were closed the same way: a RECALL gap the architecture lens routed to this criterion by name across two prior review rounds - this file's own bespoke source-text lexer (`scan_file`/`tokenize`) duplicating the codebase's ONE canonical tree-sitter extractor, `crates/rigger-grounder/src/grounder/symbols/extract.rs::extract` (its own module doc's claim, architecture 5.5.3) - closed by `find_bespoke_lexer_vs_canonical_extractor` (decision `u85c2-bespoke-lexer-sweep`), a fourth generalizable sweep; and a PRECISION defect the adversary found by reading every `same-named helper` cluster against `ScannedFn::enclosing_impl` - `find_same_named_helper_functions` was misclassifying REQUIRED trait-impl methods as coincidental duplication (`subscribe_all`/`subscribe_stream` across the `EventStore` trait's three backend adapters plus a test double, `blast_radius` across the `Grounder` trait's own default method, its override, and a test double) - closed by excluding members whose extracted Self type differs across the group when at least one comes from an actual `" for "` trait impl (decision `u85c2-same-named-helper-trait-impl-precision-fix`), mirroring `find_parallel_constructor_clusters`'s own `(file, Self type)` keying one function away. Round 7 (decision `u85c4-r7-exclude-periphery-file-from-adversarial-population`) excluded this criterion's own citation-guard periphery file from the draw's population (see this subsection's opening paragraph); that exclusion still applies unchanged.


## 3. Boundary Violations

Instrument: for each of the five named ports (`eventstore::EventStore`, `contextgraph::Projection`, `conductor::AgentDriver`, `gate::Runner`, `grounder::Grounder`), searched every production (pre-`#[cfg(test)]`) call site of that port's known concrete adapter modules from a NON-adapter, NON-composition-root file, and separately searched every domain-ish file's top-level `use` statements for a direct infrastructure-crate import. `src/main.rs` is exempt from the "reaches a concrete adapter" check: it is the composition root, and wiring concretions together is its designed job. Every citation below is resolved against the tree when the report is rendered.

FOUND, one violation:

Violation 1 (`Grounder`): `crates/rigger-grounder/src/ingest.rs:412-444` (`walk_batches`, called from production `conductor::RunCtx::ingest_project_batches` at `crates/rigger-conductor/src/conductor.rs:11891`, itself called from `crates/rigger-conductor/src/conductor.rs:11873` above the `15221` `#[cfg(test)]` boundary) calls `crate::grounder::symbols::events::project_batches_paced` directly by concrete module path at line 418 to reuse the `symbols` grounder's already-persisted index for a one-time whole-project ingest walk, then at line 424 - same function, same missing-port defect, not a separate third violation - calls `crate::grounder::design::events::project_batches` directly by concrete module path for the design-doc half of the same walk. These two calls are two of the three named sites of section 2's own catalogued duplicate cluster (`dup-f2eee0c78cd6`: `crates/rigger-grounder/src/grounder/design/events.rs:97-99`, `crates/rigger-grounder/src/grounder/symbols/events.rs:66-68`, and `crates/rigger-grounder/src/grounder/workflowdef.rs:233-240` - all three named `project_batches`), so this boundary violation and that duplication finding are two symptoms of one root cause - `ingest.rs` naming each concrete grounder submodule because no port exposes either. The `Grounder` port (`crates/rigger-domain/src/grounder.rs:56`: `ground`, `reindex`, `blast_radius`, `index_stamp`) serves real-time per-query grounding of an agent's prompt; none of its methods exposes "hand me every indexed file's projected events for a whole-project batch ingest," so `ingest.rs` - itself a domain ingest authority, not an adapter and not the composition root - has no port to depend on for either call and reaches the concrete `symbols` module (418) and the concrete `design` module (424) directly. Fix direction for a follow-up spec: add an ingest-shaped port method (e.g. a `Grounder::project_batches` or a standalone `SymbolProjector` trait) covering both concrete modules, so `ingest.rs` depends on one abstraction instead of either concrete grounder module for its whole-project walk.

Also reaching `grounder::symbols::store::content_hash` from `crates/rigger-grounder/src/ingest.rs:667` and `crates/rigger-conductor/src/canary_store.rs:154`: DISPOSITIONED as legitimate shared-primitive reuse, not a third violation. `content_hash` (`crates/rigger-grounder/src/grounder/symbols/store.rs:47-57`) is documented at its own definition as the content-identity primitive the `symbols` grounder's reindex freshening gate keys on, and `canary_store.rs`'s own doc comment (`crates/rigger-conductor/src/canary_store.rs:132`) reuses it by deliberate author intent rather than growing another open-coded FNV-1a copy - a generic hashing utility that happens to live in the `symbols` module, not a grounding operation reached through the port. The broader duplication this primitive is meant to fix (the open-coded FNV-1a copies elsewhere in the crate, per `crates/rigger-domain/src/community.rs:69`'s own comment) is a separately tracked cross-cutting refactor (`arch-u2i-fnv1a-fourth-parallel-copy`), not this section's concern.

CHECKED AND CLEAN (four of five ports fully clean; the fifth, `Grounder`, is this section's violation above - each search recorded so a clean result is not merely assumed):
- `conductor::AgentDriver` concretion reach (`crate::driver::*`): production `conductor.rs` depends only on the port (`crates/rigger-domain/src/agent.rs:269`), held as `dyn AgentDriver` (`crates/rigger-conductor/src/conductor.rs:2008`); every one of its 16 `crate::driver::` hits sits inside `#[cfg(test)] mod tests`, where the tests construct a concrete driver directly.
- `eventstore::EventStore` concretion reach (`rusqlite::Connection::open` outside the SQLite adapters and `crates/rigger-store-sqlite/src/sqlite.rs`, the one opener every store connection goes through): in production, only doc-comment mentions (`src/cli/mod.rs:2145,2326`); the one call is a deliberate, explicitly-commented test-only raw-connection bypass (`src/cli/mod.rs:5757`, inside `#[cfg(test)] mod tests` opened at `src/cli/mod.rs:5635`) that reproduces a pre-append-guard corruption shape `Store::append` itself refuses to construct - a documented test technique, not a boundary violation.
- `contextgraph::Projection` concretion reach (`contextgraph::sqlite::*`): checked whole-tree, not only `crates/rigger-conductor/src/conductor.rs` - every one of `conductor.rs`'s 60 hits sits inside `#[cfg(test)] mod tests` (production `conductor.rs` only ever depends on `dyn Projection`), and the same is true wherever else `contextgraph::sqlite::Projector` is imported (`crates/rigger-graph-sqlite/src/concepts.rs`, `crates/rigger-dash/src/dash.rs`, `crates/rigger-grounder/src/grounder/symbols/events.rs`, `crates/rigger-grounder/src/grounder/design/events.rs` - every import sits after that file's own `#[cfg(test)]` boundary).
- `gate::Runner` concretion reach (`gate::ExecRunner` / `RecordingRunner`): checked whole-tree, not only `crates/rigger-conductor/src/conductor.rs`. In `conductor.rs`, production depends only on `dyn gate::Runner` (`crates/rigger-conductor/src/conductor.rs:2009`); every production mention of a concrete runner is a doc comment (`crates/rigger-conductor/src/conductor.rs:9045,9178,9188,9195`), and the import of `ExecRunner` (`crates/rigger-conductor/src/conductor.rs:15227`) and every one of its 49 uses sit inside `#[cfg(test)] mod tests`. In `crates/rigger-driver/src/driver/replay.rs`, all 15 `ExecRunner` mentions sit inside that file's own `#[cfg(test)] mod tests` too.
- Use cases importing infrastructure: searched the `use` statements of every domain-ish file this audit's own code neighborhood names (`crates/rigger-conductor/src/conductor.rs`, `crates/rigger-domain/src/blocker.rs`, `crates/rigger-domain/src/spec.rs`, `crates/rigger-domain/src/watch.rs`, `crates/rigger-domain/src/community.rs`) for `rusqlite`, `reqwest`, `tonic`, `tokio`, `kurrentdb` - zero hits anywhere. Empty category.

A second mutation authority for one domain: the one previously-known instance in this codebase (`crates/rigger-dash/src/dash.rs` reimplementing `crates/rigger-process/src/reap.rs`'s `/proc` pid scan, spec 85's own Goal example, upheld at spec 62's capstone) is a duplicate READ-only reimplementation, not a bypassed MUTATION path - it is section 2's finding (`u85c2-proc-stat-worked-example`, `find_proc_stat_or_status_readers`), not re-counted here to avoid double-charging one defect to two sections. Checked process spawning as the one other plausible second-authority candidate: production code constructs every process through the one process-spawn port (`crates/rigger-process/src/subprocess.rs`), so production `conductor.rs` never builds a git command of its own. `crates/rigger-worktree-git/src/worktree.rs` is the sole git-worktree-mutation authority OUTSIDE the composition root. Inside it, `src/cli/mod.rs` (exempt from the port-concretion-reach check above, not from this one) holds two more git-worktree-mutation sites: `reap_then_remove_worktree` (`src/cli/mod.rs:2094-2106`), the sanctioned worktree half of the spec-34/spec-79 orphan-sweep and extensively reviewed across those specs - a deliberate design choice, not a gap; and `materialize_config_at_rev` (`src/cli/mod.rs:3058-3101`), a real, already-known, non-blocking gap (`arch-u13-config-checkout-bypasses-worktree-authority` / `arch-u2r-config-checkout-shells-git` / `arch-u2r2-replayrunner-and-config-checkout-persist-not-introduced`: the `Worktree` API is branch-creating and exposes no detach-at-rev checkout, so this is a gap in that authority rather than a competing abstraction). No second mutation authority found beyond the already-cited, already-catalogued `/proc` case and this already-dispositioned `materialize_config_at_rev` gap.

## 4. Dead and Vestigial Code

### 4.0 Stage 1 (spec 87 criterion 1): the compiler-proven pass

Spec 87 redoes this whole section in two stages: STAGE 1, here, is compiler-driven and lands first; STAGE 2 (criterion 2) and STAGE 3 (criterion 3) are a separate reference sweep over the tree this stage leaves behind, and rewrite everything below this subsection in full. This subsection is this stage's own record, additive to and preserved by that later rewrite.

INSTRUMENT ONE (`cargo minify`, tweedegolf's tool): run over the whole crate on the default (`symbols`) feature lane, checking every FUNCTION, CONST, STATIC, STRUCT, ENUM, UNION, TYPE_ALIAS, ASSOCIATED_FUNCTION and MACRO_DEFINITION for zero references. Result: "no unused code that can be minified" - zero items removed. `cargo minify` has no `--no-default-features` flag (verified: `cargo minify --no-default-features` -> `error: unrecognized option`), so the light lane's own picture comes from instrument two below instead.

INSTRUMENT TWO (the promoted-lint build): `cargo rustc --lib` and `cargo rustc --bin rigger`, each on both the default and `--no-default-features` lanes, with `-D dead_code -D unused_imports -D unused_variables -D unreachable_pub` passed as trailing (target-only) flags - four builds total, all clean. `cargo rustc`'s trailing flags were chosen deliberately over a whole-crate `RUSTFLAGS` env var (tried first): `RUSTFLAGS` also strict-lints `build.rs`'s own compilation, which then fails on two items in `build/gitsemver.rs` that are correctly `pub` for their other three `#[path]` inclusion sites (`src/main.rs`, `tests/gitsemver_derivation.rs`, `tests/gitsemver_worktree_periphery.rs`) but register as `unreachable_pub` from `build.rs`'s own isolated crate view - a false positive from the blunt instrument, not a real defect in `build.rs`. `cargo rustc`'s trailing flags apply only to the one named target's own rustc invocation, never to a dependency and never to `build.rs`, so it proves exactly the claim this criterion's Done-when makes (a build of the library and the binary) without that false positive.

FOUND, in `src/`: zero items either instrument could prove unreachable - `delete_compiler` in the companion record (`docs/audit/stage1-compiler-pass.json`) is the empty list. This is the exact known blind spot this spec's own Design section predicts, not an unexplored gap: `rustc`'s `dead_code` lint never fires on a `pub` item in a crate that is both a library and a binary (this report's own prior finding, instrument three below, already established the codebase's src/ is clean under a plain rebuild), and `unreachable_pub` only catches a `pub` item that is provably reachable from NOWHERE outside its own crate - not one that is correctly exported but simply has zero real callers. That second, larger class needs the reference-counting instrument stage 2 builds, not a compiler diagnostic; this stage's near-empty yield is exactly why stage 2 exists.

FOUND, outside `src/` (`build/gitsemver.rs`, spec 74's compile-time version-derivation seam, shared via `#[path]` into four separate compilations - `build.rs`, `src/main.rs`, `tests/gitsemver_derivation.rs`, and `tests/gitsemver_worktree_periphery.rs`): two items, `UNVERSIONED_SUFFIX` (line 45) and `derive_version` (line 129), were `pub` when nothing outside their own defining crate ever reaches them at any of those four inclusion sites - `pub(crate)` satisfies every site independently, since each `#[path]` inclusion recompiles the same source text fresh as part of whichever crate includes it. Applied as the compiler's own suggested fix (`rustc`: "consider restricting its visibility: `pub(crate)`"); both a fast regression test (`tests/compiler_pass_stage1_audit.rs`) and this record hold the exact file:line so a future widening back to `pub` is caught. Not counted in `delete_compiler` above - a visibility narrowing is not a deletion, and `build/` is not `src/` - but disclosed here in full rather than silently folded into either count, mirroring this section's own "disclosed as an instrument limitation rather than silently worked around" discipline below.

VERIFIED STILL GREEN on the tree as this stage leaves it: `tests/no_os_kill_audit.rs` and `tests/reap_before_removal_audit.rs`, both process-lifecycle audits this criterion's Done-when names by name - unaffected, since this stage's only change touches a compile-time version string, never process lifecycle.

### 4.1 Stage 2: the workspace production-reference sweep

A compiler lint never fires on a `pub` item of a crate that is both a library and a binary, so stage 1 cannot see a `pub` function nothing calls. Stage 2 is a reference sweep over the whole workspace instead: the root package's `src/` and every member crate's `crates/<name>/src/`, since a member crate calling into the library is as much a production caller as the library's own binary. Every function defined in that production source is a candidate unless it is test code (a `#[cfg(test)]` span, or a file reached only through a `#[cfg(test)] mod` declaration, transitively), an entry point the language or a foreign caller invokes (a top-level `fn main`, an `extern "C"` export), or a trait-impl method (the language calls `Drop`, formatting, operator and iterator methods with no call site in the text).

THE RULE: a candidate is live when an identifier token equal to its name appears in production code outside its own signature - any token, whatever surrounds it, so a function passed by value, named in an attribute string or used as a path segment counts exactly like a call. `tests/` directories and test spans never count. When several candidates share a name, a reference is attributed to the one it names: a free or associated function by its path qualifier, the referencing file's `use` import, or (for a free function) a bare call in its own file; a method by its receiver's type - the `T` of a `T::name` path, the enclosing impl's type for `self.name` or `Self::name`, or the declared type of a local receiver (a typed parameter, a typed `let`, a struct-literal `let`). A method call whose receiver type cannot be read from the text (a field, a call result, a pattern binding) is credited to every method of that name, so the sweep can miss dead code but never reports live code as dead. Output: `docs/audit/dead-code.json` (line-free, drift-guarded like the duplication catalog) and `docs/audit/dead-code.lines.json` (each entry's line numbers).

### 4.2 The rule: live or deleted

A function is either live or deleted; there is no third state. rigger's library has no consumer outside this workspace - its binary, its integration tests and its member crates all sit inside the sweep's scope - so a function with no production caller has no public surface to protect and no reason to stay: it is deleted together with the tests that exercise only it. Code a future change needs is added by that change, together with its caller. The audit enforces the rule: it fails whenever the ledger below holds any entry, and the failure names each one.

### 4.3 The ledger

The ledger is empty.

### 4.4 Retired-feature remnants and stale doc claims

RETIRED-FEATURE REMNANTS. `turbovec` (spec 57, "Retire turbovec"): grepped the whole tree (`src/`, `tests/`, `docs/`, `specs/`, `Cargo.toml`) for every mention - found only the deliberate migration-error guard code (`crates/rigger-grounder/src/grounder/mod.rs`'s `is_retired_grounder` / the loud `retired_grounder_error`) plus the tests and docs that keep it retired (`tests/turbovec_retired.rs`, `tests/turbovec_retired_cargo_boundary.rs`, `tests/grounder_name_contract.rs`, and several others naming it as a guarded-against name). Zero implementing code, zero cargo feature, zero dependency - confirmed by reading `Cargo.toml`'s `[features]` section in full (one feature, `symbols`, on by default; no `turbovec` entry anywhere). `kurrentdb` build-time feature flag (spec 47, "KurrentDB is always available"): grepped `Cargo.toml` and every file under `src/` for `feature = "kurrentdb"` / `-F kurrentdb` - zero hits outside the tests that guard against its resurrection (`tests/kurrentdb_always_available.rs`); `kurrentdb` and `tokio` are unconditional `[dependencies]` as spec 47 requires, and `testcontainers` (the adapter's contract-test-only dependency) correctly lives under `[dev-dependencies]`, never the production dependency tree. Both named retirements are fully clean - a real, evidenced negative finding, not an assumption.

STALE DOC CLAIMS. Scanned every `docs/*.md`, `README.md`, and `CONTRIBUTING.md` for any `src/**/*.rs` or `tests/**/*.rs` path-shaped substring and checked each cited path still exists on disk. Two misses surfaced (`src/bar.rs` in `docs/architecture-addendum-pit-of-success.md:252,256`; `src/modifier.rs` in `docs/architecture.md:1022,1027-1028`) - both read in context and confirmed generic illustrative examples in unrelated prose (`crates/foo/src/bar.rs` as a spec-criterion example path, `core-schema/src/modifier.rs` as an event-log worked example), never a real claim about this repository's own layout. Zero genuine dangling file references found.

## 5. Test-Suite Shape

Instrument: subsystem grouping is a hand-derived, ordered filename-keyword rule table (mirrors criterion 1's own per-file classification convention: first-match-wins, narrowest first, an explicit residual named rather than silently dropped). The consolidation notes below cross-reference the ALREADY-COMMITTED `docs/audit/duplication-catalog.json` (criterion 2's own generator output, not re-scanned here) filtered to the 63 clusters whose every site sits under `tests/`; every count in them is read from the catalog at render time.

### 5.1 Subsystem grouping and consolidation map

| Subsystem | Consolidation note |
|---|---|
| Dashboard: KG lenses & viz (code/concepts/community/files lenses, graph exploration, overlays, viz layout) |  |
| CLI whole-binary integration (`cli.rs`, `watchdog_cli_periphery.rs`, `ci_lanes.rs`) | split plan at 5.3 |
| Knowledge-graph ingestion & context-graph projections | dedup/fold/identity concerns |
| Conductor orchestration: gates, courier, step/run lifecycle | `courier_registry_refresh_boundary_periphery.rs` + `courier_registry_refresh_fence_periphery.rs` + `courier_registry_refresh_periphery.rs` share 0 duplication clusters confined to just themselves |
| Reset / log compaction / store hygiene |  |
| Worktree & scratch lifecycle |  |
| Simplification-audit generator & its own periphery (this spec) |  |
| Spec/handbook lint & architecture-doc integrity |  |
| Grounding (symbols grounder, turbovec retirement, blast radius) | `kurrentdb_always_available.rs` + `turbovec_retired.rs` share 0 duplication clusters confined to just themselves |
| Process lifecycle: no-os-kill & reap discipline |  |
| Event store & config precedence |  |
| Canary (review-panel judge-the-judges evaluation) | `canary_false_positives_periphery.rs` + `canary_unattributed_rejects_periphery.rs` share 2 duplication clusters confined to just themselves; `canary_item_sharding_jobs_cap_periphery.rs` + `canary_progress_hook_periphery.rs` share 0 duplication clusters confined to just themselves |
| Concepts/community lens derivation & fold (non-viz) | `community_detection_cli.rs` + `concepts_derivation_cli.rs` share 0 duplication clusters confined to just themselves |
| Residual (no natural larger home) | `build_budget_slots_periphery.rs`, `gitsemver_derivation.rs` - named rather than forced into an ill-fitting bucket |

### 5.2 Shared fixtures to extract into `tests/common`

`tests/common/mod.rs` and `tests/common/fixtures/` already hold the shared fixtures - the gap is everything still duplicated OUTSIDE them. The catalog's all-helper-function test-only clusters (34 of the 63 test-only clusters) are the evidence; the 4 widest, by distinct files, are the headline case for extraction:

- `caught_up_line` / `emit_folded_line` / `gate_ceiling` / `reclaimed_line` / `sleeping` - 5 sites across 4 files (`dup-30d8bd7871ae`, near).
- `live_contains` / `live_memberships_of` / `live_node_ids` - 3 sites across 3 files (`dup-9c3b5c80063e`, near).
- `fixture_graph` - 3 sites across 3 files (`dup-c49b09543e58`, semantic).
- `declare_grep_guard` / `declare_sdet_author` / `drop_the_cursor` / `drop_the_cursor_s_log` / `plant_stale_marker` - 5 sites across 2 files (`dup-82bff055dbd1`, exact).

Proposed home for each: `tests/common` (the catalog's own `proposed_home` field says so for each). Consolidating just these collapses roughly 16 duplicate definitions into 4 shared ones - the single largest mechanical simplification this audit identifies anywhere in the test suite.

### 5.3 `tests/cli.rs` split plan

`tests/cli.rs` holds 471 `#[test]` functions. Its existing internal section markers each name the spec and criterion whose tests follow it, not a CLI subcommand or subsystem - ad hoc organization that falls well short of a deliberate, complete per-surface structure. The split proposed below replaces those by-spec markers with a complete, deliberate BY CLI SUBCOMMAND SURFACE organization. A keyword-on-test-name pass (matching each test's dominant CLI verb: `step_`, `run_`, `validate_`, `reset_`, `watch_`/`watchdog_`, `canary_`, `dash_`/`status_`, `store_`/`eventstore_`, `spawn_`/`scratch_`, `review_`/`gate_`, `setup_`/`precommit_`/`hook_`, `courier_`/`registry_`, `spec_`, `replay_`, `worktree_`, `emit_`/`peers_`/`decision_`, `stats_`, `heartbeat_`/`liveness_`, `prime_`/`version_`/`init_`) only cleanly covers 225 of the 471 tests (47%) - disclosed honestly rather than overclaimed, because a real fraction of `cli.rs`'s scenarios are DELIBERATELY end-to-end (a single test legitimately drives `step` + `run` + `validate` + `dash` together to prove a cross-cutting property, e.g. `a_run_driver_auto_starts_a_reachable_dash_with_a_url_shown_in_status` or `docs_ships_graph_hygiene_guidance_to_consumers`), which a bare keyword match cannot and should not force into one bucket. The proposed split is BY CLI SUBCOMMAND SURFACE - `cli.rs`'s own natural organizing concept, since the whole file drives the `rigger` binary end to end - into per-surface files (`tests/cli_step.rs`, `tests/cli_run.rs`, `tests/cli_validate.rs`, `tests/cli_reset.rs`, `tests/cli_watch.rs`, `tests/cli_canary.rs`, `tests/cli_dash.rs`, `tests/cli_store.rs`, `tests/cli_review.rs`, `tests/cli_setup.rs`, plus a residual `tests/cli_misc.rs` for the genuinely cross-cutting scenarios), with each test's home decided by its DOMINANT scenario on a human/AI read, not a mechanical keyword match - the same discipline this audit's own responsibility map applied to unassignable functions (named, never silently forced). Cross-referencing the catalog: `cli.rs` also participates in 4 of the catalog's cross-file test-only duplication clusters, several paired against files that WOULD merge with it under this split (`tests/step_attention_periphery.rs`, paired in 0 clusters; `tests/watchdog_cli_periphery.rs`, paired in 1 clusters) - the split is expected to shrink, not grow, the duplication surface.

### 5.4 Duplicated helpers across test files (beyond 5.2's headline cases)

34 test-only clusters in the committed catalog have every site as an ordinary (non-`#[test]`) helper function - the shared-fixture-extraction candidate class. Beyond the 4 in 5.2, the widest are:

- `courier_project` / `courier_project_with_commit` / `temp_rigger_project` / `temp_store_project` - 4 sites across 2 files (`dup-2a101767f24d`, near).
- `communities_no_concepts_graph` / `file_over_code_graph` / `mixed_membership_graph` - 3 sites across 2 files (`dup-498b51115191`, near).
- `assert_selected_server` / `assert_selected_sqlite` / `assert_server_reached_and_credentials_redacted` - 3 sites across 2 files (`dup-b58b0ff72b22`, near).
- `community_over_concepts_graph` / `full_in_budget_graph` - 2 sites across 2 files (`dup-06ffd3c67857`, near).
- `spawn` - 2 sites across 2 files (`dup-14d812231784`, near).
- `holds_table` / `poison` - 2 sites across 2 files (`dup-36ebafe7bb7e`, near).

Every one of these 34 clusters, with its full site list and the catalog's own `proposed_home`, is already machine-readable in the committed `docs/audit/duplication-catalog.json` for a follow-up consolidation spec to consume directly - not re-enumerated exhaustively here to keep this section a report, not a second copy of the catalog.

### 5.5 Table-driven test families

28 test-only clusters have every site as a `#[test]` function - a literal-differs-only-in-input family, spec 85's own named table-driven-test candidate class. The largest families this audit first found - `tests/spec_lint.rs`'s feed-one-spec-through-`validate` defect tests and `tests/no_os_kill_audit.rs`'s one-termination-pattern-per-test checks - are closed, as are the `tests/reap_before_removal_audit.rs` exemption-coverage family, this generator's own scanner tests and the no-os-kill test helper's pid-refusal tests: their cases run as `test_cases!` rows over shared case helpers. The largest still open:

- none: every all-`#[test]` cluster is closed or dispositioned.

As with 5.4, the full 28-family list lives in the committed catalog by cluster id for a follow-up test-consolidation spec to consume directly.

## 6. Prioritized Plan

Nineteen follow-up refactoring specs, ordered largest risk-reduction first. This section adds no new findings: every citation below points at a claim already recorded in section 1 (`docs/audit/responsibility-map.json`), section 2 (`docs/audit/duplication-catalog.json`), section 4.3 (`docs/audit/dead-code.json`), or sections 3 and 5's own prose. The three committed JSON files ground every count below (queried directly, never re-scanned). Item 0 (Tier 1) deletes the dead-code ledger (section 4.3); six of the remaining eighteen entries split a god file (tiers 2 and 3, two phases times three files); the other twelve retire duplication or close a port gap (tiers 1, 4 and 5) - kept as separate entries throughout, per spec 85's own instruction that "the god-file splits and the duplication removals are separate entries so each can be its own run."

### 6.1 How this plan is ordered

Largest risk-reduction first is read as six tiers, ranked by the KIND of risk each entry retires, highest first:

1. Tier 1 - active correctness risk, PLUS item 0: a use case already depends on the wrong concretion, or two independent implementations of one concern can already drift apart silently (section 3's boundary violation; the one already-drifted `/proc`-reading pair section 2 and section 3 both name) - live gaps, not just size. Item 0 (deleting the dead-code ledger, section 4.3) is placed here too, first of all: not a live-gap risk itself, but the cheapest, zero-behavior-change move available, and it shrinks the files tiers 2 and 3 operate on before either touches them.
2. Tier 2 - god-file test-module extraction: each of the three god files' own inline `#[cfg(test)] mod tests` holds 55-73% of that file's mapped functions (section 1's map), and moving it is a pure relocation with no production-behavior change - the single largest safe line-count reduction in this plan, and the precondition that makes tier 3 tractable.
3. Tier 3 - god-file production splits: section 1's own proposed module tree applied to the (now much smaller) remaining production surface of each god file. Higher execution risk than tier 2 because it touches live orchestration and CLI logic, so it is sequenced after tier 2 shrinks the target first.
4. Tier 4 - named production duplication sweeps: the mechanical mandatory sweeps section 2 ran regardless of the Jaccard pass (`Command::new`, `.rigger`-path literals, sqlite `Connection::open`, error-shaping helpers), each already a single committed cluster with its own proposed home.
5. Tier 5 - test-suite consolidation: section 5's own catalogued test-only duplication. No production-correctness exposure at all (worst case a test regresses, never the product), so it is ordered ahead only of tier 6 despite touching the largest raw line count anywhere in this plan.
6. Tier 6 - remaining catalog sweep: the 149 src-touching clusters section 2 found but tiers 1 and 4 did not individually name. Unlike every other tier, none of these 149 have been read and risk-assessed one at a time the way tiers 1-4's named clusters have - they are consumed straight from the catalog - so this tier carries production-correctness exposure tiers 2, 3 and 5 do not, and is ordered last: the follow-up spec must triage each cluster's own production-or-test status before merging it, not assume tier 5's blanket test-only treatment applies here too.

Within a tier, entries are ordered largest-first by the site or line count each retires - the same rule the tiers themselves follow, applied one level down.

### 6.2 Tier 1: active correctness risk

#### 0. Delete the dead-code set

- Scope: every entry of section 4.3's ledger (`docs/audit/dead-code.json`) and the tests that exercise only it; a test that also exercises live code is trimmed, not deleted. Under section 4.2's rule no entry is kept, and a function whose only caller was a deleted entry is deleted in the same pass.
- Status: complete - the ledger is empty.
- Risk: low. A deletion is a pure subtraction: `cargo build` and `clippy -D warnings` on both feature lanes catch any missed reference immediately.
- Unblocks: shrinks the files tiers 2-4 operate on before they touch them, so it runs first.

#### 1. Close the `Grounder` port gap for whole-project batch ingest (retires dup-f2eee0c78cd6 in the same motion)

- Scope: section 3 violation 1 (`crates/rigger-grounder/src/ingest.rs::walk_batches`, reaching `grounder::symbols::events::project_batches_paced` and `grounder::design::events::project_batches` by concrete module path) and duplication cluster `dup-f2eee0c78cd6` (3 modules' own twin `project_batches` functions, in `crates/rigger-grounder/src/grounder/design/events.rs`, `crates/rigger-grounder/src/grounder/symbols/events.rs`, `crates/rigger-grounder/src/grounder/workflowdef.rs`) are one root cause, not two - fix once. 3 CANDIDATES, ONE HOME (spec 85 CONSTRAINTS WALK): `dup-f2eee0c78cd6`'s own mechanical `proposed_home` suggests relocating into `tests/common`, but every site is production code under `crates/rigger-grounder/src/grounder/` and `src/grounder/`, not a test helper - the mechanical heuristic has no "add a port method" category to route a production duplicate to, so it mis-fires here. This plan follows section 3's own reasoned disposition instead: add a `Grounder::project_batches` port method (or a standalone `SymbolProjector` trait) covering all 3 concrete modules, and point `ingest.rs` at it.
- Files: `crates/rigger-grounder/src/ingest.rs`, `crates/rigger-grounder/src/grounder/mod.rs`, `crates/rigger-grounder/src/grounder/symbols/events.rs`, `crates/rigger-grounder/src/grounder/design/events.rs`, `crates/rigger-grounder/src/grounder/workflowdef.rs`.
- Expected line delta: roughly neutral - one new trait method plus 3 thin impls, minus the 3 duplicate bodies `dup-f2eee0c78cd6` catalogs.
- Risk: medium. `ingest.rs`'s own module doc calls it "the ONE walk-and-content-key authority" - a load-bearing path; needs the existing whole-project-ingest and reindex-freshening coverage to stay green, not just the duplicate sites' own tests.
- Unblocks: retires the one `Grounder` port violation section 3 found and `dup-f2eee0c78cd6` together, rather than as two separately-tracked fixes.

#### 2. Retire the duplicate `/proc`-reading authority (`dup-9d67deeaad9d` + `dup-96daf6c3e468`)

- Scope: the production half is done - `crates/rigger-dash/src/dash.rs::process_state` and `crates/rigger-process/src/reap.rs::pid_starttime` both read their `/proc/<pid>/stat` field through `crates/rigger-process/src/reap.rs::stat_field_after_comm`, the one parser of the kernel's `pid (comm) state ...` layout (`read_ppid` reads `/status`, a different file). What remains is the test-only readers (`dup-96daf6c3e468`, 9 sites across `crates/rigger-process/src/reap.rs`, `tests/common/fixtures/host.rs`, `tests/simplification_audit.rs`, such as the shared `tests/common/fixtures/host.rs::pgid_of` fixture) and 60 raw `/proc`-path string literals across 12 files (`dup-9d67deeaad9d`), most of them assertion messages and this audit's own sweep names rather than reads.
- Files: the test-only readers `dup-96daf6c3e468` names.
- Expected line delta: small and negative - a test fixture reads its field through one shared helper instead of re-splitting the stat line.
- Risk: low - test-only; nothing this touches can signal or end a process, so it carries none of the no-os-kill gate's own risk surface.
- Unblocks: retires the last copies of the "duplicate implementation reconciled after the fact" pattern the operator's strict-DRY rule targets - the concrete precedent spec 85's own Goal cites.

### 6.3 Tier 2: god-file test-module extraction

Each god file's inline test module is the set of its `is_test: true` entries in the committed `docs/audit/responsibility-map.json`. Each entry below moves an already-passing test module with no intended production-behavior change - a `cargo test` pass before and after is the whole verification. The line figures below sum those test functions' own spans, so they exclude the module-level doc comments, `use` statements and blank lines around them.

#### 3. Extract `crates/rigger-conductor/src/conductor.rs`'s inline test module

- Scope: the file's `#[cfg(test)] mod tests` holds 833 of its 1128 mapped functions (73%), roughly 32434 lines of test-function spans. Partition into a `src/conductor/tests/` directory, one file per concern, reusing the same names section 1 already assigned the file's own production buckets (`run_ctx`, `support`, `gate`, `prior_failure`, `schedule`, ...) so the split needs no new naming scheme.
- Files: `crates/rigger-conductor/src/conductor.rs` -> `crates/rigger-conductor/src/conductor.rs` (production only) + `src/conductor/tests/*.rs`.
- Expected line delta: 0 net (repo-wide) - roughly 32434 lines relocated out of `crates/rigger-conductor/src/conductor.rs`.
- Risk: low - mechanical move of passing tests, zero intended behavior change.
- Unblocks: shrinks `conductor.rs` to its production code before tier 3 touches a single production line, cutting the odds that an unrelated future unit's blast radius collides with this file.

#### 4. Extract `src/main.rs`'s inline test module

- Scope: the file's `#[cfg(test)] mod tests` holds 211 of its 378 mapped functions (55%), roughly 6449 lines of test-function spans. Same partition approach as item 3, reusing section 1's own production bucket names (`store`, `support`, `render`, `setup`, `provenance`, ...).
- Files: `src/cli/mod.rs` -> `src/cli/mod.rs` (production only) + `src/main/tests/*.rs`.
- Expected line delta: 0 net (repo-wide) - roughly 6449 lines relocated out of `src/cli/mod.rs`.
- Risk: low, same rationale as item 3.
- Unblocks: shrinks `main.rs` to its production code before tier 3's own main.rs split.

#### 5. Extract `crates/rigger-dash/src/dash.rs`'s inline test module

- Scope: the file's `#[cfg(test)] mod tests` holds 156 of its 248 mapped functions (62%), roughly 5644 lines of test-function spans. Lower effort than items 3-4: section 1's own classifier already found 8 pre-existing sub-boundaries inside this one test module (`dash::tests::call_target_c4`, `dash::tests::calls_route_c4`, `dash::tests::file_half_c13`, `dash::tests::leaf_keys_c1`, `dash::tests::metadata_card_c2`, `dash::tests::rationale_overlay_c3`, `dash::tests::reprojection_views`, `dash::tests::subject_view_c5`), so the partition points already exist and need only become their own files.
- Files: `crates/rigger-dash/src/dash.rs` -> `crates/rigger-dash/src/dash.rs` (production only) + `src/dash/tests/*.rs`.
- Expected line delta: 0 net (repo-wide) - roughly 5644 lines relocated out of `crates/rigger-dash/src/dash.rs`.
- Risk: low - the lowest-effort of the three, for the reason above.
- Unblocks: shrinks `dash.rs` to its production code before tier 3's own dash.rs split.

### 6.4 Tier 3: god-file production splits

Each entry below applies section 1's own proposed module tree to a god file's production surface, sequenced after the matching tier-2 entry removes that file's test bulk first. Every module name and function/line count below is summed directly from the committed `docs/audit/responsibility-map.json` (function-body spans only); a file's remaining non-function production lines - struct/enum/type definitions, `use` statements, module docs - are outside section 1's own function-only scan and move with whichever module they sit beside, without needing their own assignment.

#### 6. Split `crates/rigger-conductor/src/conductor.rs`'s production code into `src/conductor/*.rs`

- Scope: 238 mapped functions across 22 proposed modules (roughly 8781 lines of function bodies) plus 57 unassigned functions (1324 lines, each individually named in the committed map for manual placement, per section 1's own "unassignable functions are named as such, never omitted" rule). Headline buckets (functions/lines): `conductor::run_ctx` (150/7486), `conductor::support` (17/470), `conductor::gate` (15/193), `conductor::prior_failure` (5/101), `conductor::schedule` (5/101).
- Files: `crates/rigger-conductor/src/conductor.rs` -> `src/conductor/mod.rs` + `src/conductor/{run_ctx,support,gate,prior_failure,schedule,review,emit,run,budget,review_round,spawn,frontier,infra_retries,gate_outcome,refused_record,review_outcome,deps,ground,gate_ratchet,integrated_probe,throwaway,integration_approval}.rs`.
- Expected line delta: 0 net - pure relocation of roughly 10105 lines of function bodies.
- The largest bucket, `conductor::run_ctx`, may warrant its own second pass if it does not decompose cleanly into one file.
- Risk: medium-high - conductor.rs is the composition root's own most complex use-case file; every intermediate commit needs the full `cargo test`, no-os-kill and reap audits green, not just the final one.
- Unblocks: the largest reduction in production-code blast-radius collision risk this audit identifies; makes future duplication-spotting against conductor.rs's own logic tractable by a human reviewer, not only by the mechanical scanner.

#### 7. Split `src/main.rs`'s production code into `src/main/*.rs`

- Scope: 92 mapped functions across 14 proposed modules (roughly 1871 lines of function bodies) plus 75 unassigned functions (1573 lines, each individually named in the committed map for manual placement, per section 1's own "unassignable functions are named as such, never omitted" rule). Headline buckets (functions/lines): `main::store` (24/702), `main::support` (18/303), `main::render` (5/193), `main::setup` (13/190), `main::provenance` (12/167).
- Files: `src/cli/mod.rs` -> `src/main.rs` (composition root, thinned) + `src/cli/{store,support,render,setup,provenance,commands,liveness,store_location,replay_runner,store_selection,docs_overlay,tree_root,residue_report,dash_glue}.rs`.
- Expected line delta: 0 net - pure relocation of roughly 3444 lines of function bodies.
- Risk: medium - `main.rs` is the composition root itself; the split must preserve which concretions get wired where, not merely move text.
- Unblocks: shrinks `main.rs` to a genuine composition root plus a `cli/` module tree, matching the ports-and-adapters shape this project already mandates everywhere else.

#### 8. Split `crates/rigger-dash/src/dash.rs`'s production code into `src/dash/*.rs`

- Scope: 48 mapped functions across 7 proposed modules (roughly 1320 lines of function bodies) plus 44 unassigned functions (830 lines, each individually named in the committed map for manual placement, per section 1's own "unassignable functions are named as such, never omitted" rule). Headline buckets (functions/lines): `dash::server` (8/604), `dash::render` (18/404), `dash::reproject` (5/177), `dash::registry` (6/65), `dash::response` (6/48).
- Files: `crates/rigger-dash/src/dash.rs` -> `src/dash/mod.rs` + `src/dash/{server,render,reproject,registry,response,dash_marker,probe_miss}.rs`.
- Expected line delta: 0 net - pure relocation of roughly 2150 lines of function bodies.
- Risk: low-medium - the always-on dash's own contract (loopback-only, zero-new-dependency) is unaffected by a pure module split.
- Unblocks: completes the god-file split trio; the third program-sized file becomes an ordinary module tree.

### 6.5 Tier 4: named production duplication sweeps

Each entry is one of section 2's five named mandatory sweeps - collected mechanically regardless of the Jaccard pass, per spec 85's own Design.

#### 9. Consolidate the 643 `.rigger`-path string-literal sites (`dup-47e4d1c77944`) - the single largest cluster in the entire catalog by site count

- Scope: one `.rigger`-relative path-composition helper (the cluster's own `proposed_home`) every one of the 643 sites routes through instead of building its own literal.
- Files: spans dozens of files including `crates/rigger-conductor/src/conductor.rs`, `crates/rigger-config-files/src/config_store.rs`, `crates/rigger-dash/src/dash.rs`, `crates/rigger-domain/src/docs.rs`, `crates/rigger-gates-shell/src/gate.rs`, `crates/rigger-grounder/src/grounder/mod.rs`, `crates/rigger-grounder/src/grounder/symbols/store.rs`, `crates/rigger-grounder/src/ingest.rs`, `src/main.rs`, `crates/rigger-process/src/reap.rs`, `crates/rigger-store-sqlite/src/registry.rs`, `src/worktree.rs` plus many `tests/` files - the full site list is in the committed `docs/audit/duplication-catalog.json` under `dup-47e4d1c77944` for the follow-up spec to consume directly, not re-enumerated here.
- Expected line delta: negative - 643 literal compositions collapse toward one helper's call sites; the helper itself is small.
- Risk: medium - the largest surface-area sweep in this plan by site count, even though each individual site is trivial; needs a mechanical rewrite pass plus a full-suite green run, not hand-editing 643 sites.
- Unblocks: the biggest single site-count reduction available anywhere in the duplication catalog.

#### 10. The 94 `Command::new` call sites (`dup-64397d781d9d`) - production spawns already route through one process-spawn port

- Scope: every production spawn routes through `crates/rigger-process/src/subprocess.rs` (the cluster's own `proposed_home`), and the audit's `the_process_spawn_port_is_the_only_production_command_new_caller` gate refuses a new direct construction anywhere else in production code. The 1 site(s) in `crates/rigger-process/src/subprocess.rs` are the port itself; the other 93 are test code spawning git, shells and the product binary.
- Files: `crates/rigger-process/src/subprocess.rs` plus test code in `src/` and `tests/` - full site list in `docs/audit/duplication-catalog.json` under `dup-64397d781d9d`.
- Expected line delta: none left in production; a test site that repeats a shared fixture's spawn routes through that fixture instead.
- Risk: low - no production spawn is left to move, and the gate keeps it that way.
- Unblocks: the next process-spawning concern added anywhere in the crate reuses the port instead of constructing its own `Command`.

#### 11. Consolidate the 66 sqlite `Connection::open` call sites (`dup-880060c29e31`)

- Scope: one sqlite-connection-opening adapter function (the cluster's own `proposed_home`) spanning `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs`, `crates/rigger-store-sqlite/src/eventstore/sqlite.rs` and `src/main.rs`, plus several `tests/` files.
- Files: full site list in `docs/audit/duplication-catalog.json` under `dup-880060c29e31`.
- Expected line delta: negative - 66 open calls collapse toward one function.
- Risk: medium - touches the event store and context graph's own connection-lifecycle code; needs the store-identity and store-resolution contract tests green throughout.
- Unblocks: one place to change pragma/timeout/journal-mode settings instead of 66.

#### 12. Consolidate the 23 error-shaping helper sites (`dup-7310d4338924`) - caution, confirm before merging

- Scope: the cluster spans `src/cli/mod.rs` (`setup_read_error`) and 6 unrelated test files - a wide spread for one claimed duplicate. This may be a threshold-gaming false cluster (spec 85's own CONSTRAINTS WALK: "the threshold is a floor for the mechanical pass; the reading pass owns semantic duplicates") rather than one real shared concern - the follow-up spec's first job is confirming by reading whether these 23 sites share actual logic before proposing one helper, not assuming the cluster label proves it.
- Files: the `src/` files above, plus the 6 test files named in `docs/audit/duplication-catalog.json` under `dup-7310d4338924`.
- Expected line delta: unknown pending the confirmation read above - potentially zero if the cluster does not survive a human read.
- Risk: low (the smallest-site-count sweep), but with the stated precondition.
- Unblocks: either a genuine fifth consolidation, or a documented "not a real duplicate" disposition that keeps the catalog honest for whoever reads it next.

### 6.6 Tier 5: test-suite consolidation

Every entry cites section 5's own already-catalogued test-only duplication; none of it carries production-correctness risk.

#### 13. Extract the headline shared test fixtures into `tests/common` (section 5.2)

- Scope: `dup-30d8bd7871ae` (4 files), `dup-9c3b5c80063e` (3 files), `dup-c49b09543e58` (3 files), `dup-82bff055dbd1` (2 files) - roughly 16 duplicate definitions collapsing into 4 shared ones, the single largest mechanical simplification section 5 identifies anywhere in the test suite.
- Files: per-cluster, from the committed catalog, plus `tests/common/`.
- Expected line delta: negative - each fixture's small body survives once instead of once per file.
- Risk: low - test-only, and `tests/common/` already holds the same shape of shared fixture.
- Unblocks: item 16 below (the remaining test-helper clusters) reuses the same `tests/common` home this item establishes.

#### 14. Split `tests/cli.rs` by CLI subcommand surface (section 5.3's plan)

- Scope: 471 tests, split into `tests/cli_{step,run,validate,reset,watch,canary,dash,store,review,setup}.rs` plus a residual `tests/cli_misc.rs` for the genuinely cross-cutting scenarios section 5.3 names, using each test's dominant scenario (a human/AI read, not the 47%-coverage keyword match section 5.3 already disclosed as insufficient alone).
- Files: `tests/cli.rs` and the eleven new files above.
- Expected line delta: 0 net - pure relocation into eleven files.
- Risk: low-medium - a mechanical per-test move with `cargo test`'s full pass count as the verification.
- Unblocks: splits a file in 4 cross-file duplication clusters (section 5.3) and lets item 16's remaining-clusters sweep target smaller, subcommand-scoped files.

#### 15. Convert the largest remaining table-driven test families into parametrized tables (section 5.5)

- Scope, largest first (0 sites across 0 clusters; the spec_lint, no-os-kill, reap-audit exemption, scanner and pid-refusal families are already closed):
- none: every all-`#[test]` cluster is closed or dispositioned.
- Files: the files named above.
- Expected line delta: negative - each family's near-identical test bodies collapse into `test_cases!` rows over one case helper.
- Risk: low - test-only, and each family already shares one body shape (section 5.5's own finding).
- Unblocks: the largest remaining reduction in raw `#[test]` body count.

#### 16. Sweep the remaining 30 test-only helper-duplication clusters (section 5.4, beyond item 13's headline fixtures)

- Scope: the 34 test-only, all-helper-function clusters section 5.4 names, minus the ones item 13 already covers - consumed directly from `docs/audit/duplication-catalog.json`, not re-enumerated here (section 5.4's own stated approach).
- Files: per-cluster, from the committed catalog.
- Expected line delta: negative, cumulative across 30 clusters.
- Risk: low - test-only.
- Unblocks: closes out the helper-duplication half of the test suite's own strict-DRY exposure.

#### 17. Sweep the remaining 28 table-driven test families (section 5.5, beyond item 15's headline families)

- Scope: the 28 test-only, all-`#[test]` clusters section 5.5 names, minus the 0 cluster ids item 15 already covers - consumed directly from `docs/audit/duplication-catalog.json`.
- Files: per-cluster, from the committed catalog.
- Expected line delta: negative, cumulative.
- Risk: low - test-only.
- Unblocks: closes out the table-driven-test half of the test suite's own strict-DRY exposure; combined with items 13 and 15-16, retires all 63 test-only clusters section 2 found.

### 6.7 Tier 6: remaining catalog sweep

Unlike tier 5, this entry's own clusters are NOT known to be test-only - each one needs its own read before merging (see `### 6.1`'s tier 6 rationale above).

#### 18. Sweep the remaining 149 src-touching duplication clusters (section 2, beyond tiers 1 and 4's 7 named clusters)

- Scope: of the catalog's 219 clusters, 63 are test-only (items 13 and 15-17 above) and 7 are the named tier-1/tier-4 items (`dup-64397d781d9d`, `dup-47e4d1c77944`, `dup-880060c29e31`, `dup-9d67deeaad9d`, `dup-96daf6c3e468`, `dup-f2eee0c78cd6`, `dup-7310d4338924`); the remaining 149 clusters touching `src/` - mostly small 2-5-site exact/near matches like the two worked examples section 2 itself opens with (`dup-49d4d9f335fc`, `dup-76144682ea08`) - are swept here, largest exact-duplicate clusters first, consumed directly from `docs/audit/duplication-catalog.json`.
- Files: per-cluster, from the committed catalog.
- Expected line delta: negative, cumulative; the largest single contributor is whichever exact cluster has the most sites (read from the catalog at spec-writing time, not fixed here).
- Risk: low-medium - unlike tier 5, some of these clusters are production code, so each merge needs its own test-coverage check, not a blanket "test-only" pass.
- Unblocks: the last of the catalog's 219 clusters; after items 1-2 and 9-18 all land, a future spec can state and check that the duplication catalog's own drift guard finds zero live clusters left unaddressed.

### 6.8 Dead and vestigial code beyond item 0: no further follow-up

Section 4.2's rule leaves no dead-code category for a later plan item: a function is live or it is deleted by item 0. Both named retirements (`turbovec`, `kurrentdb`) are still fully clean, and the two stale-looking doc paths found remain confirmed generic illustrative examples, not real dangling references.
