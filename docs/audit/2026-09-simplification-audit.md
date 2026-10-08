# Simplification Audit - 2026-09

The audit report spec 85 derives the follow-up refactoring specs from. Six sections, each claim citing `file:line`; see `specs/85-simplification-audit.md` for scope.

## 1. Responsibility Map

Every function in `crates/rigger-conductor/src/conductor.rs`, `src/cli/mod.rs` and `crates/rigger-dash/src/dash.rs` (1584 functions total), assigned to a proposed module by `tests/simplification_audit.rs`'s deterministic scanner + rule-table classifier (never by hand). Instrument: the brace-matching scanner over the three named files.

### Proposed module tree

- `conductor::budget` (2 functions)
  - `crates/rigger-conductor/src/conductor.rs:419-421` `compensation_queued_key` - name contains "compensat" (budget accounting); grouped under `conductor::budget`.
  - `crates/rigger-conductor/src/conductor.rs:969-998` `pending_compensations_from_log` - name contains "compensat" (budget accounting); grouped under `conductor::budget`.
- `conductor::deps` (2 functions)
  - `crates/rigger-conductor/src/conductor.rs:1865-1872` `folding` - method inside `impl <'a> Deps<'a>`; grouped with its other `Deps` methods.
  - `crates/rigger-conductor/src/conductor.rs:1877-1879` `ingests` - method inside `impl <'a> Deps<'a>`; grouped with its other `Deps` methods.
- `conductor::emit` (3 functions)
  - `crates/rigger-conductor/src/conductor.rs:449-451` `quarantine_record_key` - name contains "record" (event/decision emission); grouped under `conductor::emit`.
  - `crates/rigger-conductor/src/conductor.rs:13740-13762` `recorded_session` - name contains "record" (event/decision emission); grouped under `conductor::emit`.
  - `crates/rigger-conductor/src/conductor.rs:13844-13877` `recorded_adoption` - name contains "record" (event/decision emission); grouped under `conductor::emit`.
- `conductor::gate` (15 functions)
  - `crates/rigger-conductor/src/conductor.rs:405-412` `gate_key` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `crates/rigger-conductor/src/conductor.rs:459-466` `gate_intersects_radius` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `crates/rigger-conductor/src/conductor.rs:480-482` `unit_of_gate_key` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `crates/rigger-conductor/src/conductor.rs:492-495` `gate_key_attempt` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `crates/rigger-conductor/src/conductor.rs:544-559` `unit_gate_runs` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `crates/rigger-conductor/src/conductor.rs:570-607` `gate_evidence_block` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `crates/rigger-conductor/src/conductor.rs:640-661` `recorded_gate_outcome` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `crates/rigger-conductor/src/conductor.rs:666-668` `deferred_gate_verdict_key` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `crates/rigger-conductor/src/conductor.rs:677-679` `deferred_gate_failed_key` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `crates/rigger-conductor/src/conductor.rs:1644-1651` `no_verdict_line` - name contains "verdict" (gate execution); grouped under `conductor::gate`.
  - `crates/rigger-conductor/src/conductor.rs:1659-1670` `verdictless_reviewer` - name contains "verdict" (gate execution); grouped under `conductor::gate`.
  - `crates/rigger-conductor/src/conductor.rs:12246-12253` `with_gate_hold` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `crates/rigger-conductor/src/conductor.rs:12261-12269` `union_gates` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `crates/rigger-conductor/src/conductor.rs:12373-12380` `gate_failure_cause` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `crates/rigger-conductor/src/conductor.rs:14065-14110` `assert_no_ungated_fanout_unit` - name contains "gate" (gate execution); grouped under `conductor::gate`.
- `conductor::gate_outcome` (3 functions)
  - `crates/rigger-conductor/src/conductor.rs:710-716` `green` - method inside `impl GateOutcome`; grouped with its other `GateOutcome` methods.
  - `crates/rigger-conductor/src/conductor.rs:720-724` `fail` - method inside `impl GateOutcome`; grouped with its other `GateOutcome` methods.
  - `crates/rigger-conductor/src/conductor.rs:728-734` `cause` - method inside `impl GateOutcome`; grouped with its other `GateOutcome` methods.
- `conductor::gate_ratchet` (1 function)
  - `crates/rigger-conductor/src/conductor.rs:867-873` `for_persistent_failure` - method inside `impl GateRatchet`; grouped with its other `GateRatchet` methods.
- `conductor::ground` (1 function)
  - `crates/rigger-conductor/src/conductor.rs:13028-13037` `graph_around_recovery` - name contains "graph" (grounding integration); grouped under `conductor::ground`.
- `conductor::infra_retries` (3 functions)
  - `crates/rigger-conductor/src/conductor.rs:1806-1810` `recorded` - method inside `impl InfraRetries`; grouped with its other `InfraRetries` methods.
  - `crates/rigger-conductor/src/conductor.rs:1816-1828` `after` - method inside `impl InfraRetries`; grouped with its other `InfraRetries` methods.
  - `crates/rigger-conductor/src/conductor.rs:1832-1834` `exhausted` - method inside `impl InfraRetries`; grouped with its other `InfraRetries` methods.
- `conductor::integration_approval` (1 function)
  - `crates/rigger-conductor/src/conductor.rs:1269-1271` `approved` - method inside `impl IntegrationApproval`; grouped with its other `IntegrationApproval` methods.
- `conductor::prior_failure` (5 functions)
  - `crates/rigger-conductor/src/conductor.rs:1303-1311` `logged` - method inside `impl PriorFailure`; grouped with its other `PriorFailure` methods.
  - `crates/rigger-conductor/src/conductor.rs:1317-1326` `failed_body` - method inside `impl PriorFailure`; grouped with its other `PriorFailure` methods.
  - `crates/rigger-conductor/src/conductor.rs:1328-1334` `is_empty` - method inside `impl PriorFailure`; grouped with its other `PriorFailure` methods.
  - `crates/rigger-conductor/src/conductor.rs:1338-1356` `summary` - method inside `impl PriorFailure`; grouped with its other `PriorFailure` methods.
  - `crates/rigger-conductor/src/conductor.rs:1362-1420` `block` - method inside `impl PriorFailure`; grouped with its other `PriorFailure` methods.
- `conductor::review` (9 functions)
  - `crates/rigger-conductor/src/conductor.rs:338-340` `review_round_start_key` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `crates/rigger-conductor/src/conductor.rs:1558-1561` `review_retry_window` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `crates/rigger-conductor/src/conductor.rs:1608-1628` `degenerate_reviewer` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `crates/rigger-conductor/src/conductor.rs:12304-12313` `review_evidence` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `crates/rigger-conductor/src/conductor.rs:12360-12366` `review_failure_cause` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `crates/rigger-conductor/src/conductor.rs:12479-12486` `review_protocol` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `crates/rigger-conductor/src/conductor.rs:12566-12570` `rejects_without_required` - name contains "reject" (review orchestration); grouped under `conductor::review`.
  - `crates/rigger-conductor/src/conductor.rs:13693-13699` `review_round_starts` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `crates/rigger-conductor/src/conductor.rs:13730-13733` `recorded_review_round_start_sha` - name contains "review" (review orchestration); grouped under `conductor::review`.
- `conductor::review_outcome` (3 functions)
  - `crates/rigger-conductor/src/conductor.rs:914-923` `verdict` - method inside `impl ReviewOutcome`; grouped with its other `ReviewOutcome` methods.
  - `crates/rigger-conductor/src/conductor.rs:924-926` `approved` - method inside `impl ReviewOutcome`; grouped with its other `ReviewOutcome` methods.
  - `crates/rigger-conductor/src/conductor.rs:927-929` `rejected` - method inside `impl ReviewOutcome`; grouped with its other `ReviewOutcome` methods.
- `conductor::review_round` (2 functions)
  - `crates/rigger-conductor/src/conductor.rs:12526-12528` `blocks` - method inside `impl ReviewRound`; grouped with its other `ReviewRound` methods.
  - `crates/rigger-conductor/src/conductor.rs:12532-12560` `block` - method inside `impl ReviewRound`; grouped with its other `ReviewRound` methods.
- `conductor::run` (4 functions)
  - `crates/rigger-conductor/src/conductor.rs:12390-12395` `unit_size_cap` - name contains "unit" (unit run loop); grouped under `conductor::run`.
  - `crates/rigger-conductor/src/conductor.rs:13423-13425` `unit_branch` - name contains "unit" (unit run loop); grouped under `conductor::run`.
  - `crates/rigger-conductor/src/conductor.rs:13444-13450` `unit_worktree_dir` - name contains "unit" (unit run loop); grouped under `conductor::run`.
  - `crates/rigger-conductor/src/conductor.rs:13705-13723` `unit_status_marks` - name contains "unit" (unit run loop); grouped under `conductor::run`.
- `conductor::run_ctx` (140 functions)
  - `crates/rigger-conductor/src/conductor.rs:3178-3180` `emit` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3192-3207` `append_and_fold` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3220-3238` `append_and_fold_batch` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3242-3249` `stamped` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3254-3260` `emit_with_actor` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3270-3278` `emit_meta` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3291-3293` `emit_keyed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3304-3328` `emit_keyed_meta` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3366-3391` `emit_keyed_batch` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3400-3406` `agent_model` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3431-3434` `effective_attempts` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3443-3453` `counted_attempts` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3461-3466` `logged_prior_failure` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3478-3486` `gate_verdict_high_water` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3502-3582` `drain_compensations` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3587-3589` `read_current_run` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3602-3649` `commits_to_compensate` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3659-3691` `emit_gate_skip` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3708-3766` `emit_gate_verdict` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3774-3781` `max_retries` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3794-3805` `max_retries_for` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3831-3837` `budget_tripped` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3844-3846` `spawn_is_recorded` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3862-3885` `reserve_spawn` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3894-3924` `trip_budget_breaker` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3939-3948` `halt_reason` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3953-3963` `check_coverage_or_flag` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3966-4110` `run_wave` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4119-4153` `partition_wave` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4158-4168` `partition_requested` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4173-4197` `run_batch` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4199-4247` `start_and_run_stage` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4249-4340` `run_stage` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4345-4362` `stage_paused_for_review` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4374-4385` `select_review_panel` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4397-4419` `log_review_tier` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4433-4451` `assert_isolated_cwd` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4466-4538` `reviewer_spawn_opts` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4547-4562` `continue_prior_session` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4569-4595` `record_session` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4632-4646` `review_round_start_sha` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4658-4675` `round_delta` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4751-4964` `review_unit` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4974-5015` `split_reject` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:5044-5100` `guard_review_round_tree` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:5127-5145` `guard_review_round_tree_on_tier_err` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:5186-5285` `spawn_sdet_author` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:5300-5318` `halted_spawn_checkpoint_permitted` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:5326-5341` `retry_infra_failure` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:5345-5363` `emit_infra_mark` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:5369-6221` `run_single_stage` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:6227-6234` `effective_speculation_width` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:6243-6250` `speculates` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:6259-6275` `speculation_lane_worktree` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:6296-6654` `run_speculation` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:6667-6758` `emit_speculation_winner_status` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:6777-6814` `record_speculation_reject` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:6824-6857` `cancel_speculation_candidates` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:6859-6912` `run_fan_out_stage` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:6922-7115` `run_fan_out_review_loop` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:7150-7206` `run_review_agents_concurrently` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:7222-7260` `run_lens` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:7299-7505` `run_reviewer` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:7541-7553` `gating_spawn_emitted_approve` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:7575-7580` `review_spawn_errored` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:7606-7640` `reviewer_result_is_degenerate` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:7659-7696` `run_adversary` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:7717-7778` `run_adjudicator` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:7800-7821` `dag_unit_blast_radii` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:7834-7922` `build_dag_critique_prompt` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:7931-8038` `re_plan` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:8050-8083` `run_plan_critique_gate` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:8102-8299` `plan_critique_loop` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:8311-8346` `stopped_on_spec_defect` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:8374-8385` `build_env` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:8402-8407` `run_base_env` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:8439-8445` `spawn_env` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:8456-8461` `build_budget` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:8486-8494` `shared_build_cache_paths` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:8502-8508` `postmerge_unit_dir` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:8520-8528` `run_gates` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:8533-8776` `run_gates_at` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:8783-8799` `classify_gate_failure` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:8815-8897` `run_gate_with_taxonomy` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:8942-8944` `gc_integrated_branches` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:8951-9043` `gc_integrated_branches_logged` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9073-9263` `run_deferred_gates` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9274-9341` `record_gate` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9399-9972` `integrate_and_emit` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10014-10021` `integrate_plan_commits` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10023-10143` `integrate_plan_commits_inner` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10154-10170` `record_plan_intent` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10181-10208` `read_plan_landed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10216-10245` `record_plan_landed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10250-10256` `regenerate_rule_for` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10281-10319` `run_regenerate_command` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10338-10363` `regenerate_conflicted_paths` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10391-10403` `catch_up_owed_regeneration` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10415-10427` `regenerate_and_record` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10432-10438` `regenerate_pending_for` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10445-10453` `union_regenerate_pending` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10480-10508` `record_regenerate_pending` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10517-10531` `record_placeholder_staged` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10541-10558` `record_regenerate_commit` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10568-10582` `record_merge_attempt` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10588-10607` `record_merge_outcome` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10614-10629` `record_landing_intent` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10639-10654` `record_landed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10665-10683` `record_integrate_row` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10697-10760` `spawn_conflict_resolution_implementer` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10770-10772` `build_system_prompt` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10782-10794` `ground_query` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10800-10810` `implementer_agent` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10814-10839` `plan_protocol` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10866-10900` `grounded_blast_radius` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10916-10929` `grounded_seed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10953-10982` `record_blast_radius` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10984-10990` `build_prompt` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11003-11016` `build_review_prompt` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11023-11029` `review_base_prompt` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11036-11043` `review_gate_evidence` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11055-11091` `build_prompt_with_failure` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11095-11100` `seeded_subgraph` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11107-11121` `review_resume_task` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11123-11182` `graph_context` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11194-11205` `ingest_project_into_graph` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11219-11244` `ingest_project_batches` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11249-11251` `ingest_project_into_graph` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11265-11276` `ingest_files_into_graph` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11280-11282` `ingest_files_into_graph` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11291-11317` `emit_lesson` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11330-11371` `land_refused` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11376-11382` `agent_isolated` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11505-11620` `adopt_prior_criterion_branch` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11642-11683` `resume_phase` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11685-11717` `stage_worktree` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11732-11753` `review_only_worktree` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11765-11770` `template_gates` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11772-12175` `harvest_proposed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:12199-12222` `resolve_served_criterion` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
- `conductor::schedule` (3 functions)
  - `crates/rigger-conductor/src/conductor.rs:1712-1718` `plan_landing_failed` - name contains "plan" (unit scheduling); grouped under `conductor::schedule`.
  - `crates/rigger-conductor/src/conductor.rs:12282-12284` `normalize_criterion_id` - name contains "criterion" (unit scheduling); grouped under `conductor::schedule`.
  - `crates/rigger-conductor/src/conductor.rs:13542-13602` `prior_criterion_unit` - name contains "criterion" (unit scheduling); grouped under `conductor::schedule`.
- `conductor::spawn` (3 functions)
  - `crates/rigger-conductor/src/conductor.rs:1449-1451` `is_parked` - name contains "parked" (spawn lifecycle); grouped under `conductor::spawn`.
  - `crates/rigger-conductor/src/conductor.rs:1511-1526` `spawn_halt` - name contains "spawn" (spawn lifecycle); grouped under `conductor::spawn`.
  - `crates/rigger-conductor/src/conductor.rs:1540-1542` `is_parked_or_budget_refused` - name contains "parked" (spawn lifecycle); grouped under `conductor::spawn`.
- `conductor::support` (16 functions)
  - `crates/rigger-conductor/src/conductor.rs:1055-1088` `conflict_regenerate_pending_from_log` - name contains "from_" (conversion helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:1167-1190` `landings_from_log` - name contains "from_" (conversion helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:1214-1245` `integrate_attempted_from_log` - name contains "from_" (conversion helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:2797-2799` `has_producer` - name contains "has_" (predicate helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:12354-12356` `is_infra_cause` - name contains "is_" (predicate helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:12449-12454` `build_system_prompt` - name contains "build" (generic construction helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:12844-12988` `write_capped_section` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:13039-13095` `write_code_neighborhood` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:13208-13291` `write_design_intent` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:13297-13312` `write_capped_decisions` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:13317-13334` `write_capped_lessons` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:13342-13367` `write_capped_findings` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:13376-13384` `write_peers_pointer` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:13399-13409` `write_lookup_pointer` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:13654-13667` `branch_is_foreign` - name contains "is_" (predicate helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:13778-13783` `is_documentation_only` - name contains "is_" (predicate helper); grouped under `conductor::support`.
- `conductor::tests` (679 functions)
  - `crates/rigger-conductor/src/conductor.rs:3123-3174` `for_test` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14349-14382` `a_fold_the_run_could_not_make_is_said_through_its_injected_log` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14388-14392` `agent_failure_as_str_round_trips_through_from_category_for_every_category` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14395-14400` `agent_failure_from_category_degrades_an_unrecognized_string_to_unknown` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14403-14405` `agent_failure_default_is_unknown` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14408-14413` `classify_failure_prefers_the_stopfailure_record_over_api_retry` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14416-14421` `classify_failure_falls_back_to_the_last_api_retry_category` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14424-14426` `classify_failure_falls_back_to_unknown_when_neither_source_has_anything` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14429-14443` `strip_failure_marker_drops_the_class_prefix_leaving_only_the_message` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14446-14449` `strip_failure_marker_passes_through_an_unmarked_error_unchanged` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14452-14471` `unit_worktree_dir_derives_deterministically_from_scratch_root_and_unit_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14474-14564` `recorded_gate_outcome_reads_the_latest_gate_run_verdict_per_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14477-14489` `verdict` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14567-14603` `unit_of_gate_key_strips_a_retry_ordinal` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14607-14613` `started_with_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14615-14620` `integrated` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14623-14631` `prior_criterion_unit_finds_a_prior_un_integrated_units_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14634-14660` `prior_criterion_unit_never_returns_an_integrated_units_id_and_never_falls_back_to_an_older_sibling` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14663-14687` `prior_criterion_unit_integration_of_one_criterion_never_masks_an_abandoned_sibling_criterion_sharing_the_same_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14690-14703` `prior_criterion_unit_tie_break_prefers_the_most_recent_of_two_non_integrated_priors` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14706-14711` `prior_criterion_unit_excludes_this_unit_itself` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14714-14726` `prior_criterion_unit_ignores_a_different_criterion_and_an_empty_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14733-14738` `run_started_with_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14743-14745` `compensated` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14750-14756` `plain_failure` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14760-14772` `assert_prior_criterion_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14852-14870` `adoption_status` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14873-14898` `recorded_adoption_ignores_a_same_identity_event_carrying_the_wrong_status` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14901-14934` `recorded_adoption_never_answers_for_a_mismatched_criterion_or_a_mismatched_spec_alone` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14937-14942` `branch_owner_returns_none_for_an_id_that_never_started` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14945-14961` `branch_owner_reads_the_most_recent_started_criterion_and_spec_for_this_bare_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14964-14979` `branch_owner_ignores_a_non_unit_started_event_even_when_it_shares_the_id_field` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14982-14995` `branch_is_foreign_is_false_when_nothing_is_recorded_or_everything_matches` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14998-15020` `branch_is_foreign_is_false_when_the_recorded_owner_has_no_criterion_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15023-15045` `branch_is_foreign_when_only_one_axis_differs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15050-15062` `find_unit_started` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15065-15147` `a_fresh_units_own_branch_adopts_a_prior_runs_un_integrated_unit_sharing_the_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15150-15211` `a_fresh_unit_never_adopts_a_criterion_whose_prior_attempt_already_integrated` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15214-15355` `a_halted_spawns_uncommitted_tree_is_captured_as_a_wip_commit_and_named_in_the_next_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15359-15369` `dirty_unit_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15374-15409` `assert_no_wip_recovery_commit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15412-15420` `park_implementer` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15423-15447` `a_dirty_tree_whose_named_spawn_was_never_requested_gets_no_wip_recovery_commit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15450-15485` `a_dirty_tree_gets_no_wip_recovery_commit_while_a_sibling_spawn_of_the_unit_is_still_live` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15488-15522` `a_prior_runs_leftover_spawn_request_for_a_same_named_unit_never_halts_a_new_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15534-15549` `prior_failure_summary_names_only_the_halted_commit_when_it_is_the_sole_failure` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15552-15575` `prior_failure_block_names_only_the_halted_commit_when_it_is_the_sole_failure` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15583-15615` `prior_failure_block_adds_the_generic_preamble_for_review_reject_or_contradiction_alone` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15622-15645` `a_pattern_item_asks_the_implementer_for_a_whole_tree_audit_record` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15648-15690` `a_resumed_unit_re_enters_with_the_prior_failure_block_its_retry_carried` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15697-15716` `prompts_around_a_resume` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15719-15750` `review_worktree_dir_and_branch_derive_from_stage_and_attempt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15886-15891` `answering` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15893-15926` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15929-15936` `resume_of` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15939-15941` `prompts_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15944-15946` `targets_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15949-15951` `dirs_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15955-15957` `system_prompt_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15961-15963` `title_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15967-15969` `reviews_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15973-15975` `spawn_ids` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15979-15985` `spawned` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15991-15998` `dir_existed_when_spawned` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16001-16180` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16184-16188` `integrates_a_passing_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16191-16198` `one_stage_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16202-16213` `uncovered_run_log` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16216-16226` `covers_criterion_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16291-16315` `planner_extends_the_dag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16318-16386` `planner_proposed_unit_with_a_coverage_criterion_runs_and_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16389-16473` `conductor_creates_one_baseline_unit_per_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16479-16508` `fan_out_checkin_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16511-16554` `a_stage_needing_the_fan_out_template_becomes_ready_once_every_criterion_unit_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16559-16579` `checkin_fixture_gate_calls` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16582-16623` `checkin_gates_run_only_at_checkin_once_per_spec_on_a_fixture` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16626-16750` `producer_prompt_carries_the_criteria_and_plan_protocol_grounded_on_the_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16755-16757` `baseline_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16762-16768` `supersede_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16774-16777` `append_proposal` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16780-16821` `a_prior_runs_proposal_never_resurrects_in_a_new_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16824-16898` `planner_unit_supersedes_the_matching_baseline` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16901-16977` `a_planner_supersede_of_a_fan_out_member_still_satisfies_its_downstream_needs_edge` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16980-17008` `a_criterion_with_no_planner_unit_keeps_its_baseline` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17011-17076` `planner_refinement_split_is_still_harvested` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17079-17204` `a_same_id_re_emit_updates_the_proposed_unit_in_place` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17211-17241` `seed_refine_dag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17245-17265` `append_proposals` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17268-17333` `a_coverage_retarget_re_emit_preserves_one_unit_per_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17336-17388` `a_re_emit_naming_a_reserved_stage_never_mutates_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17391-17448` `re_folding_a_same_id_re_emit_is_idempotent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17451-17550` `a_real_split_two_distinct_ids_both_survive_the_fold` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17564-17575` `harvest_split_parts` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17578-17601` `a_split_criterions_parts_keep_their_own_owns_sentence_after_the_exact_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17604-17617` `a_split_parts_contract_carries_its_own_owns_sentence` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17620-17642` `an_id_less_split_part_still_supersedes_its_baseline_by_its_criterion_prefix` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17645-17662` `an_id_less_part_resolves_to_the_longest_criterion_its_text_starts_with` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17685-17692` `assert_proposal_stamps_exact_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17696-17704` `proposal_data` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17708-17714` `proposal_event` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17719-17737` `harvest_seeded` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17742-17785` `assert_later_episode_supersedes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17853-17879` `a_same_episode_re_seen_on_a_later_fold_still_never_self_supersedes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17888-17937` `assert_refine_survives_its_own_episodes_sibling` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17972-18077` `a_resume_catch_up_over_two_episode_supersession_and_a_split_matches_a_live_incremental_fold` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18080-18179` `a_legacy_history_resume_catch_up_matches_a_live_incremental_fold_mutual_siblings_then_superseded` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18182-18225` `a_legacy_proposal_never_supersedes_an_identified_episodes_owner_even_when_logged_later` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18228-18319` `a_late_re_emit_never_mutates_a_started_or_terminal_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18323-18342` `append_gated_proposal` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18345-18424` `harvest_proposed_gates_every_case_with_the_templates_list_unioned` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18427-18465` `harvest_proposed_gate_inheritance_survives_a_resumed_window` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18468-18485` `plan_protocol_tells_the_planner_gates_come_from_the_template` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18491-18500` `has_unmatched_signal` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18513-18567` `assert_supersedes_criterion_a_once` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18639-18689` `a_genuinely_new_proposal_runs_and_records_an_unmatched_signal` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18692-18749` `a_genuinely_new_proposal_with_no_gates_still_spawns_gated_via_template_inheritance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18752-18875` `resume_dedups_baselines_before_running_them` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18878-18949` `decomposes_the_real_spec_01_into_per_criterion_units` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18952-18982` `ratchet_promotes_a_reliable_gate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18985-19038` `elevated_gate_is_never_promoted_to_silent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19041-19078` `learns_from_escalation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19081-19133` `feeds_graph_decisions_into_the_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19136-19283` `every_unit_prompt_leads_with_its_name_and_verbatim_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19286-19367` `lookup_pointer_names_all_three_verbs_on_every_slice` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19370-19467` `decisions_prompt_injection_is_capped_under_budget_with_elision_note` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19472-19493` `render_capped_decisions` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19497-19505` `fold_subgraph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19508-19518` `render_capped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19521-19551` `a_superseded_decision_never_outranks_a_current_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19554-19607` `grounding_still_surfaces_a_prior_run_decision_that_peers_labels_historical` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19610-19637` `the_verbatim_count_cap_binds_on_many_small_decisions` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19640-19672` `the_byte_budget_cap_binds_before_the_count_on_chunky_decisions` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19675-19742` `a_kept_decision_restores_the_dropped_dependency_it_supersedes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19748-19758` `render_capped_findings` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19765-19799` `assert_capped_with_elision` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19802-19845` `findings_prompt_injection_is_capped_under_budget_with_elision_note` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19848-19883` `the_verbatim_count_cap_binds_on_many_small_findings` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19886-19993` `grounding_omits_a_resolved_finding_and_keeps_the_open_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19999-20011` `render_capped_lessons` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20014-20071` `the_injected_slice_is_deduplicated_by_normalized_text` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20074-20218` `dedup_and_restore_are_render_only_no_event_no_projection_mutation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20236-20375` `structural_grounding_is_one_seeded_traversal_over_the_unified_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20392-20511` `the_grounding_path_populates_the_unified_graph_from_the_live_project` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20519-20597` `re_ingesting_re_extracts_a_changed_file_and_skips_unchanged_ones` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20608-20694` `ingest_files_into_graph_is_bounded_to_the_named_files_and_reflects_their_live_content` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20738-20745` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20748-20755` `forwarding` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20758-20763` `of_group` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20766-20768` `refuse_next_append` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20773-20786` `append` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20788-20806` `latest_in_group` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22291-22345` `a_landed_unit_whose_reindex_lookup_fails_resumes_to_one_file_touched_and_one_integration` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22352-22419` `a_landing_appends_only_its_keyed_post_merge_verdicts` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22427-22487` `a_regeneration_committed_at_landing_reaches_file_touched_and_the_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22506-22587` `re_excluding_the_same_file_twice_in_one_process_retires_its_middle_generation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22597-22702` `a_second_run_over_an_unchanged_tree_appends_no_derived_index_event` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22713-22715` `walk_entries` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22722-22738` `spec60_cold_rebuild` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22750-22781` `spec60_reachable` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22787-22796` `spec60_reached_entities` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22809-22961` `editing_one_file_between_runs_re_emits_only_that_files_batch_and_supersedes_its_edges` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22973-23100` `a_file_reverted_to_an_earlier_recorded_generation_re_ingests_and_matches_a_cold_rebuild` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23124-23247` `a_prior_runs_non_ingest_replay_key_never_suppresses_this_runs_keyed_emit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23257-23340` `the_ingest_sink_appends_and_folds_once_per_file_batch_never_once_per_event` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23267-23275` `append` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23363-23527` `design_intent_grounding_renders_the_governing_rule_and_specifying_ra_by_traversal` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23530-23600` `a_governing_decision_never_leaks_into_the_design_intent_section` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23603-23659` `the_design_intent_section_renders_the_newest_binding_and_elides_the_oldest` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23664-23684` `assert_renders_no_section` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23740-23766` `render_code_neighborhood` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23769-23805` `the_code_neighborhood_elision_note_names_graph_around_recovery_not_peers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23808-23853` `the_code_neighborhood_byte_cap_elides_before_the_count_cap_is_reached` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23856-23898` `lessons_prompt_injection_is_capped_under_budget_with_elision_note` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23901-23938` `the_verbatim_count_cap_binds_on_many_small_lessons` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23944-23957` `render_capped_lessons_scoped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23960-23997` `lessons_rank_by_blast_radius_relevance_over_pure_recency` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24000-24021` `resume_skips_already_integrated_units` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24025-24042` `seed_events_in_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24045-24099` `replay_trajectory_keeps_only_the_world_inputs_and_restrips_the_run_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24104-24106` `commit_on_unit_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24109-24114` `unit_event` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24119-24124` `seed_prior_window_s` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24128-24154` `reviewed_merge_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24159-24169` `approving_panel_stub` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24174-24196` `assert_resume_integrates_s` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24199-24228` `resume_reuses_a_units_branch_instead_of_reimplementing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24235-24267` `assert_branch_gc_resume` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24270-24294` `branch_gc_reclaims_integrated_units_and_retains_escalated_ones_on_resume` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24298-24315` `seed_branch_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24322-24330` `commit_on_named_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24334-24342` `seed_unit_branches` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24345-24351` `started_on_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24354-24359` `escalated` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24363-24379` `integrated_with_straggler` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24382-24389` `resume_in` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24393-24406` `gc_logged` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24421-24457` `branch_gc_falls_back_to_the_derived_branch_when_unitstarted_recorded_no_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24460-24520` `branch_gc_reclaims_the_recorded_branch_not_the_derived_name_when_they_differ` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24531-24555` `seed_lingering_worktree_on_unit_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24561-24566` `worktree_registered_on` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24569-24661` `branch_gc_removes_a_lingering_worktree_before_reclaiming_the_branch_on_resume` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24664-24674` `branch_gc_reclaims_every_integrated_unit_in_one_resume_not_just_the_first` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24677-24722` `branch_gc_fences_reclaim_behind_an_in_flight_straggler_spawn_and_reclaims_once_it_answers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24725-24755` `gc_integrated_branches_logged_prints_kept_evidence_for_an_in_flight_straggler_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24758-24804` `gc_integrated_branches_logged_prints_removing_evidence_for_a_terminal_spawns_decision` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24807-24846` `gc_integrated_branches_logged_stays_silent_for_an_already_gone_worktree_but_still_reclaims_an_orphaned_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24849-24898` `gc_integrated_branches_logged_does_not_repeat_removing_evidence_once_the_real_removal_already_happened` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24901-24947` `review_boundary_events_carry_the_worktree_sha` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24950-24994` `a_review_reject_unitfailed_carries_the_worktree_sha` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24997-25036` `an_exhaustive_gate_failure_on_an_approved_unit_records_a_gate_cause` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25039-25156` `stamps_the_model_alias_and_resolved_id_on_live_lifecycle_events` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25162-25178` `degenerate_reviewer_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25181-25242` `a_degenerate_adjudicator_result_respawns_and_a_substantive_retry_folds_normally` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25250-25254` `charged_an_attempt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25257-25310` `a_verdictless_adjudicator_result_re_drives_the_review_once_and_charges_no_attempt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25313-25366` `a_second_verdictless_adjudicator_result_halts_loudly_and_charges_no_attempt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25369-25418` `a_recorded_verdictless_adjudicator_result_re_parks_the_review_on_the_stepwise_host` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25421-25535` `a_review_stage_error_result_re_parks_a_fresh_attempt_no_charge_then_a_real_verdict_folds` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25538-25674` `a_review_spawn_that_errors_on_every_re_parked_attempt_escalates_through_the_bound` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25677-25762` `a_gating_spawn_that_emits_an_approve_verdict_but_returns_no_verdict_line_hard_errors_with_the_result_channel_fix` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25767-25789` `assert_ordinary_reject` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25810-25908` `the_workflow_live_path_correlates_the_approve_by_stamp_not_a_bare_stream_position` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25911-25956` `a_degenerate_lens_result_respawns_the_lens_before_the_review_proceeds` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25959-26012` `a_lens_that_emitted_a_finding_but_reports_empty_stdout_is_not_degenerate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26015-26106` `a_reviewer_that_only_ever_returns_degenerate_output_halts_the_run_loudly_naming_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26110-26115` `approved_but_unmerged_s` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26118-26139` `resume_integrates_an_already_approved_unit_without_re_reviewing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26142-26211` `a_failed_unit_is_not_terminal_and_resumes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26214-26318` `remediation_attempts_accumulate_across_resume_and_escalate_at_the_bound` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26321-26398` `an_escalated_unit_stays_terminal_on_resume` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26401-26451` `a_fresh_unit_with_no_branch_runs_the_full_lifecycle` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26454-26500` `agent_decision_folds_content_but_no_agent_attribution` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26505-26536` `planner_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26539-26553` `scope_creep_refuses_a_criterionless_proposed_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26556-26578` `adjudicator_reject_blocks_the_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26582-26592` `approving_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26596-26618` `assert_three_tier_order` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26621-26649` `adversary_runs_between_the_lenses_and_the_adjudicator` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26652-26691` `adjudicator_reject_gates_even_with_an_adversary_present` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26694-26751` `unit_reviews_itself_within_its_own_lifecycle` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26754-26764` `depth_tiers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26768-26775` `full_panel_with_tiers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26777-26779` `strs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26782-26808` `path_is_high_risk_matches_by_prefix_and_by_glob` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26811-26820` `route_review_tier_with_no_policy_is_the_full_panel_unchanged` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26823-26842` `route_review_tier_routes_a_low_risk_unit_to_the_light_panel` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26845-26860` `route_review_tier_forces_full_on_a_high_risk_path_hit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26863-26876` `route_review_tier_forces_full_over_the_size_threshold` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26879-26892` `route_review_tier_forces_full_when_the_gates_flapped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26895-26929` `route_review_tier_fails_safe_to_full_on_an_empty_blast_radius` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26935-26986` `run_tiered_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26990-27001` `logged_review_tier` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27004-27030` `a_low_risk_unit_runs_the_light_panel_and_logs_the_routing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27038-27079` `tiered_review_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27082-27099` `a_low_risk_unit_skips_the_adversary_and_extra_lens` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27102-27128` `a_high_risk_unit_runs_the_full_panel_and_logs_the_routing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27131-27146` `without_a_depth_policy_a_grounded_unit_runs_the_full_panel_and_logs_no_routing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27149-27185` `a_zero_grounding_unit_fails_safe_to_the_full_panel_and_logs_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27188-27299` `a_flapped_unit_escalates_from_light_at_attempt_0_to_full_on_remediation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27302-27322` `a_stage_level_tiers_policy_routes_the_unit_by_risk` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27325-27399` `every_spawn_runs_in_a_worktree_never_the_main_repo_checkout` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27403-27426` `spec_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27429-27516` `speculation_width_2_runs_two_candidates_first_green_wins_rest_cancelled` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27519-27550` `speculation_budget_accounts_for_every_candidate_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27553-27589` `speculation_defaults_off_runs_a_single_candidate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27592-27627` `speculation_parks_all_candidates_together_in_one_step` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27630-27754` `speculation_defers_green_until_the_winner_and_integrates_across_replay_steps` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27760-27770` `unit_has_status` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27774-27776` `branch_present` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27779-27848` `speculation_later_candidate_wins_when_an_earlier_lane_is_review_rejected` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27851-27971` `speculation_low_risk_later_lane_winner_routes_light_not_falsely_flapped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27974-28025` `speculation_rejected_loser_is_visible_to_the_review_quality_fold` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28028-28090` `speculation_escalates_when_every_candidate_is_rejected` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28093-28153` `speculation_crashed_lane_is_absent_and_a_sibling_still_wins` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28156-28218` `speculation_exhaustive_integrate_door_red_blocks_a_candidate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28221-28330` `speculation_stays_fresh_across_parking_until_a_winner_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28345-28414` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28418-28589` `speculation_blocked_winner_captures_evidence_and_a_later_lane_wins` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28597-28606` `lane_0_merge_break` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28609-28615` `approving_speculation_stub` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28618-28674` `a_resumed_speculating_unit_re_enters_every_lane_with_its_logged_post_merge_block` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28677-28743` `a_compensated_speculating_unit_never_re_enters_with_an_earlier_lanes_logged_block` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28746-28787` `assert_isolated_cwd_refuses_empty_or_repo_root_with_a_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28790-28870` `the_conductor_threads_each_agents_persona_to_the_driver` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28873-28919` `the_system_prompt_carries_the_rigger_communication_discipline` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28922-28958` `an_agent_with_no_persona_threads_an_empty_system_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28961-29033` `operator_instructions_reach_every_spawned_agent_between_persona_and_discipline` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29036-29090` `planner_proposed_unit_inherits_the_default_review_panel` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29093-29180` `a_producer_stage_skips_the_three_tier_review_and_unblocks_its_dependents` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29183-29260` `plan_stage_commit_under_specs_reaches_the_run_branch_before_the_next_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29263-29333` `plan_stage_commit_outside_specs_fails_the_stage_naming_the_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29336-29415` `plan_stage_commit_reverting_its_own_out_of_scope_touch_still_fails_the_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29418-29484` `plan_stage_commit_conflicting_with_a_concurrent_specs_change_escalates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29487-29607` `integrate_plan_commits_is_idempotent_on_a_resumed_already_landed_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29610-29666` `integrate_plan_commits_tolerates_a_pre_existing_intent_record_with_no_git_mutation_yet` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29669-29742` `integrate_plan_commits_keeps_the_earlier_commits_identity_when_the_worktree_grows_between_calls` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29755-29771` `append` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29776-29836` `integrate_plan_commits_wraps_any_hard_error_with_the_plan_landing_marker` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29839-29916` `a_plan_landing_infra_fault_halts_the_run_loudly_with_no_per_unit_lesson_or_attempt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29920-29946` `per_unit_panel_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29954-29956` `adjudicator_at` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29962-29991` `run_review_rounds` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29998-30029` `the_round_two_review_prompt_carries_the_prior_required_list_and_the_delta_base` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30033-30052` `session_stub` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30056-30063` `session_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30066-30079` `run_session_rounds` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30086-30139` `a_relaunched_driver_continues_the_session_the_log_recorded` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30147-30188` `a_later_round_resumes_each_roles_session_with_what_changed_alone` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30195-30219` `a_resumed_review_tier_is_sent_the_gate_evidence_of_the_commit_it_now_judges` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30225-30254` `a_gate_only_failure_resumes_the_sdet_authors_session_with_the_gate_evidence` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30261-30305` `a_fallen_back_resume_is_recorded_as_requested_but_not_continued` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30312-30332` `a_later_round_continues_the_respawn_that_produced_the_rounds_output` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30339-30392` `each_spawns_session_is_recorded_in_the_log_and_read_back_from_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30397-30415` `a_driver_reporting_no_session_keeps_every_spawn_fresh` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30421-30438` `a_later_round_reject_naming_no_required_item_respawns_the_adjudicator` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30441-30448` `lessons_about` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30454-30486` `a_round_two_reject_on_only_out_of_delta_wording_lands_and_records_operator_lessons` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30492-30519` `a_correctness_item_outside_the_delta_still_rejects` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30525-30551` `a_later_round_infra_fault_reject_naming_no_item_reruns_uncharged` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30557-30583` `a_later_round_infra_fault_reject_is_never_split` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30588-30623` `an_infra_rerun_records_its_own_operator_lessons` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30626-30656` `per_unit_adjudicator_reject_blocks_integration_and_escalates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30659-30710` `an_adjudicator_infra_fault_reject_reruns_the_review_without_charging` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30713-30795` `an_always_rejecting_adjudicator_escalates_after_exactly_max_retries_cycles` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30798-30859` `a_higher_max_retries_gives_more_attempts_before_escalation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30862-30915` `an_absent_max_retries_preserves_the_default_bound_of_three` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30918-30999` `a_resumed_unit_gets_exactly_its_granted_extra_attempts_before_re_escalating` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31002-31035` `max_retries_for_widens_only_the_resumed_unit_never_a_sibling` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31038-31077` `a_stages_own_max_retries_overrides_the_run_default_for_its_units` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31095-31101` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31104-31138` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31147-31171` `assert_final_attempt_approval_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31174-31188` `approval_on_the_final_permitted_attempt_integrates_a_per_unit_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31191-31273` `a_model_ladder_implementer_escalates_one_rung_per_remediation_attempt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31276-31304` `approval_on_the_final_permitted_attempt_integrates_a_standalone_review_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31308-31314` `failing_gate_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31317-31326` `status_mark_keys` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31329-31388` `an_infra_spawn_crash_retries_under_a_retry_id_and_charges_nothing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31391-31472` `infra_retries_past_the_bound_halt_the_step_and_a_relaunch_gets_a_fresh_bound` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31475-31483` `gated_by_ok` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31486-31492` `agent_a_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31496-31500` `budget_of_one_over_two_units` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31504-31506` `assert_attention` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31553-31627` `a_budget_halt_does_not_restamp_on_a_later_poll_with_nothing_new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31630-31634` `started_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31638-31642` `replay_step` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31647-31653` `fail_attempt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31656-31720` `a_delayed_budget_halt_after_a_dependency_unlocks_still_stamps` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31723-31768` `compute_attention_leaves_the_hung_liveness_halt_to_the_caller` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31773-31784` `parked_unit_u` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31787-31827` `an_escalation_does_not_restamp_attention_on_a_resumed_process` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31830-31841` `a_clean_step_stamps_no_attention` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31844-31918` `a_second_failure_recurs_and_a_third_also_stalls_the_frontier` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31921-31957` `nine_of_ten_budget_crosses_the_final_tenth` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31960-32032` `a_re_step_already_past_the_budget_threshold_does_not_re_stamp` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32035-32039` `budget_of_one_over_a_chain` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32042-32056` `budget_breaker_stops_the_run_after_the_first_wave` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32059-32065` `budget_exhaustion_aborts_the_task` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32068-32078` `a_budget_halt_surfaces_its_reason_on_the_run_state` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32081-32091` `a_run_within_budget_surfaces_no_halt_reason` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32095-32112` `with_budget_two_ctx_over` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32115-32158` `the_spawn_budget_folds_from_recorded_spawn_requests_across_steps` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32161-32182` `the_pre_wave_breaker_trips_only_on_a_new_over_budget_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32185-32248` `a_review_tier_budget_refusal_aborts_with_budgetexhausted_not_a_raw_error` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32263-32272` `run_ungated_fanout_guard` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32275-32303` `ungated_fanout_unit_under_a_gated_template_fails_the_step` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32306-32329` `ungated_fanout_unit_under_an_ungated_template_is_not_flagged` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32332-32353` `gated_fanout_unit_passes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32356-32380` `non_fanout_stage_with_no_gates_is_never_flagged` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32383-32418` `unmatched_fanout_proposal_under_a_gated_template_fails_the_step` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32421-32445` `unmatched_fanout_proposal_under_an_ungated_template_is_not_flagged` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32448-32465` `ungated_fan_out_templates_names_a_gateless_template` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32468-32480` `ungated_fan_out_templates_is_silent_on_a_gated_template` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32483-32496` `planner_covering_every_criterion_passes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32499-32527` `manual_stage_pauses_while_an_auto_stage_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32531-32552` `spawned_in_a_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32555-32573` `isolation_none_agent_gets_no_worktree_even_with_a_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32576-32584` `spawn_opts_isolation_is_set_for_a_worktree_agent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32587-32606` `a_spawned_implementers_title_is_the_unit_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32613-32642` `roster_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32646-32662` `per_unit_roster_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32666-32673` `assert_full_panel_roster` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32698-32710` `a_panel_with_no_adversary_never_fabricates_one_in_the_adjudicators_roster` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32719-32784` `the_roster_reflects_the_actually_routed_light_panel_not_the_full_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32798-32829` `the_fan_out_review_loops_adversary_and_adjudicator_spawns_are_stamped_with_the_routed_roster` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32832-32895` `stage_autonomy_override_seeds_the_gate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32898-32938` `on_pass_none_runs_gates_but_does_not_integrate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32949-32964` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32973-33002` `two_unit_gate_runs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33007-33036` `assert_one_isolated_dir_per_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33039-33067` `two_units_gate_environments_never_share_a_target_dir` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33070-33086` `two_units_gate_environments_never_share_a_gate_scratch_root` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33089-33138` `a_gate_round_never_creates_the_gate_scratch_root` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33147-33151` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33154-33163` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33171-33294` `one_build_environment_authority_reaches_both_a_gate_build_and_an_agent_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33186-33219` `run_once` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33297-33363` `spawn_env_adds_the_per_unit_cargo_target_dir_only_for_a_real_unit_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33366-33425` `a_units_per_unit_cache_is_reclaimed_when_its_worktree_is_removed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33428-33496` `a_units_worktree_cache_and_branch_are_all_reclaimed_on_a_successful_integrate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33499-33560` `a_units_worktree_is_reclaimed_but_its_branch_survives_a_terminal_escalation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33564-33589` `judged_unit_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33594-33600` `parking_stub` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33603-33609` `parked_judge_stub` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33613-33621` `unit_branch_tip` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33627-33641` `assert_unit_worktree_at` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33644-33693` `a_parked_review_spawn_keeps_the_unit_worktree_registered_and_its_cache` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33696-33736` `review_unit_restores_a_worktree_a_gate_deleted_out_of_band` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33739-33794` `a_second_step_restores_a_still_parked_units_worktree_deleted_out_of_band` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33797-33891` `verified_worktree_sha_is_stamped_after_a_gate_side_deletion_is_restored` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33894-33967` `review_tier_boundary_restores_a_worktree_a_prior_tier_deleted` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33970-34074` `reviewed_worktree_sha_is_stamped_after_the_adjudicators_own_deletion_is_restored` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34077-34081` `has_real_worktree_sha` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34085-34104` `rejecting_deleting_judge_log` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34107-34124` `failed_worktree_sha_is_stamped_after_the_adjudicators_own_deletion_is_restored` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34128-34138` `status_sha` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34144-34168` `assert_residue_named_and_never_merged` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34173-34198` `approving_round_with_residue` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34201-34242` `a_review_rounds_dirty_residue_is_restored_named_and_never_merged` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34245-34269` `a_review_rounds_moved_tip_is_restored_to_the_reviewed_sha_before_merging` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34272-34402` `a_review_rounds_lens_residue_survives_a_later_tiers_genuine_crash_and_is_restored` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34408-34416` `residue_then_park_stub` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34423-34443` `park_then_resume` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34449-34471` `assert_one_log_derived_round_start` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34481-34529` `assert_resumed_round_judges_the_logged_start` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34582-34645` `a_speculation_lanes_log_derived_start_sha_survives_a_cross_call_resume_after_a_park` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34648-34688` `a_resumed_reviewed_unit_restores_a_worktree_the_exhaustive_gate_deleted` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34695-34723` `assert_a_fresh_process_re_enters_s_with_its_logged_gate_evidence` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34726-34821` `a_resumed_reviewed_unit_whose_exhaustive_gate_fails_records_a_gate_cause` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34824-34925` `a_resumed_reviewed_units_infra_red_reruns_the_exhaustive_suite_without_charging` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34928-35061` `a_resumed_reviewed_units_merge_break_records_an_integrate_conflict_cause` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35064-35194` `a_resumed_landed_but_ungated_unit_regates_the_landed_tree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35197-35368` `a_resumed_reviewed_units_genuine_unresolved_conflict_reaches_the_idempotent_merge_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35385-35426` `regenerate_conflicted_paths_returns_the_real_regeneration_commit_sha` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35429-35499` `a_live_approved_unit_restores_a_worktree_the_integrate_door_exhaustive_gate_deleted` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35502-35615` `speculation_lenses_restore_a_gate_deleted_worktree_without_racing_and_stamp_the_winner_sha` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35618-35622` `speculating_judged_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35625-35646` `speculation_reject_worktree_sha_is_stamped_after_the_adjudicators_own_deletion_is_restored` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35649-35720` `speculation_winner_reviewed_sha_stays_the_round_start_sha_across_a_post_review_regen_commit_and_integrate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35723-35833` `a_parked_lens_keeps_the_unit_worktree_beside_a_genuine_sibling_crash` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35836-35883` `a_worktree_less_stage_gate_inherits_the_shared_target` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35886-35912` `bounded_pool_completes_every_stage_under_the_cap` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35915-36019` `run_wave_admits_at_most_max_parallel_units_leaving_the_rest_neither_failed_nor_terminal` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36022-36095` `occupancy_survives_a_crash_resume_so_a_still_parked_unit_keeps_its_slot_over_a_fresh_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36103-36126` `a_step_that_does_not_ingest_costs_the_run_and_the_typed_carry_over_per_read` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36133-36187` `a_step_that_parks_and_replays_reads_only_the_run_and_the_typed_carry_over` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36196-36257` `a_step_that_starts_a_criterion_unit_in_a_repo_reads_adoption_by_lifecycle_type` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36271-36460` `a_step_that_ingests_seeds_each_identity_by_group_lookup_and_appends_only_what_moved` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36469-36529` `a_step_whose_group_lookup_goes_unanswered_fails_and_appends_nothing_for_that_batch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36532-36682` `a_step_whose_width_is_filled_by_parked_units_returns_instead_of_re_reading_the_stream` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36545-36552` `charge` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36555-36562` `append` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36563-36571` `read_stream` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36572-36579` `read_all` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36580-36586` `subscribe_all` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36587-36593` `subscribe_stream` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36594-36600` `last_position` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36601-36609` `read_stream_typed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36610-36620` `read_stream_positions` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36621-36630` `read_stream_batched` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36631-36638` `latest_in_group` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36685-36722` `live_run_folds_no_gate_machinery` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36727-36737` `spawned_parallel` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36740-36763` `agent_stage_runs_the_per_unit_lifecycle_not_the_fan_out_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36766-36786` `standalone_review_stage_still_takes_the_fan_out_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36789-36902` `run_gates_derives_and_injects_the_review_worktrees_store_fence` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36905-36909` `review_lens_ids` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36914-36947` `standalone_review_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36951-36960` `assert_review_worktree_kept` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36964-36971` `assert_genuine_crash_halts` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36974-36995` `a_parked_lens_keeps_the_standalone_review_stages_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36998-37053` `a_parked_lens_keeps_the_review_worktree_even_beside_a_lower_indexed_sibling_crash` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37056-37111` `a_parked_lens_keeps_the_review_worktree_beside_a_sibling_degenerate_halt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37114-37154` `the_swap_to_front_prioritizes_a_genuine_error_at_a_non_zero_chunk_index` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37157-37201` `a_budget_refused_lens_beside_a_genuinely_crashing_sibling_in_one_chunk` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37204-37234` `a_budget_refused_standalone_review_spawn_keeps_its_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37237-37264` `partition_separates_overlapping_blast_radii` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37267-37289` `blast_radius_conflicts_flags_units_that_split_one_file_set` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37292-37305` `blast_radius_conflicts_is_empty_for_a_disjoint_partition` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37308-37358` `partitioned_wave_still_integrates_every_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37370-37394` `run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37398-37509` `lens_finding_reaches_later_tiers_through_the_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37512-37522` `ok_gate_digest` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37530-37567` `every_review_tier_prompt_carries_the_gate_evidence_of_the_commit_it_judges` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37570-37583` `digested_verdict` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37589-37600` `a_digest_address_is_the_commit_its_gate_ran_on` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37608-37660` `the_gate_evidence_block_lists_only_the_units_gates_on_the_commit_under_review` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37668-37707` `a_reviewer_builds_into_the_units_review_cache_never_the_units_own` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37712-37733` `review_tier_prompts` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37736-37766` `review_agents_emit_findings_via_the_review_protocol` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37774-37801` `every_reviewer_prompt_leaves_the_gates_and_the_mutation_sweep_to_the_gates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37804-37842` `unparseable_adjudicator_output_blocks_integration` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37845-37908` `failing_gate_evidence_threaded_into_retry_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37913-37941` `silent_gate_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37944-37951` `any_verdict` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37954-38009` `a_flaky_gate_rerun_is_a_pass_with_warning_that_never_demotes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38012-38052` `a_product_gate_failure_is_not_rerun_and_demotes_as_before` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38055-38135` `an_authored_infra_rule_with_limit_zero_at_an_inline_gate_holds_the_ratchet` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38138-38238` `an_inline_gate_red_across_every_rerun_holds_for_infra_and_demotes_for_flaky` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38241-38324` `an_infra_gate_red_reruns_the_stage_at_the_same_attempt_without_charging` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38327-38344` `unit_evidence_is_populated_after_a_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38347-38414` `adjudicator_rejection_reasoning_threaded_into_retry_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38417-38476` `fan_out_lens_does_not_integrate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38479-38520` `review_only_stage_records_no_artifact_truthfully` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38523-38573` `two_erroring_stages_both_leave_a_record` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38576-38632` `single_wide_wave_overruns_budget_and_is_stopped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38635-38649` `validate_acyclic_detects_a_cycle` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38712-38725` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38727-38733` `with_side_effect` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38734-38736` `calls` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38737-38739` `targets` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38740-38742` `gate_scratches` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38743-38745` `store_fences` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38746-38748` `build_cache_guards` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38749-38751` `build_cache_dirs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38754-38804` `run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38817-38856` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38867-38872` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38873-38875` `calls` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38878-38898` `run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38902-38911` `gate_verdict_event` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38913-38917` `verdict_passed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38923-38953` `identical_tree_attempts` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38956-38986` `a_matching_input_digest_answers_a_gate_as_a_logged_cache_hit_citing_the_prior_green` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38995-39085` `a_fast_forward_landing_whose_commit_was_gated_replays_its_post_merge_gates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39092-39105` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39113-39147` `an_identical_tree_reached_through_different_history_is_not_replayed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39150-39194` `a_content_change_misses_the_cache_and_re_runs_the_gate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39197-39218` `a_red_verdict_is_never_cache_answered_and_the_gate_re_runs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39227-39239` `ground` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39251-39266` `ground` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39267-39269` `blast_radius` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39270-39272` `index_stamp` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39276-39295` `blast_radius_audits` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39298-39307` `review_tier_evidence` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39312-39335` `tiered_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39337-39344` `tiered_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39354-39407` `assert_beyond_cap_high_risk_routes_full` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39417-39434` `a_structural_grounder_records_the_audit_and_routes_full_on_a_beyond_cap_high_risk_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39441-39483` `the_empty_radius_fail_safe_still_records_the_audit_and_routes_full` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39491-39527` `a_non_structural_grounder_records_no_blast_radius_audit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39539-39604` `confidence_tier_fixture` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39615-39666` `confidence_tier_radius_splits_the_one_edge_set_into_extracted_precise_and_all_tier_safe` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39676-39757` `grounded_blast_radius_tier_filters_the_subgraph_and_keeps_the_grep_superset` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39780-39875` `under_symbols_the_audit_emits_and_records_the_prompt_seed_as_precise` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39887-39963` `partition_wave_own_batches_an_empty_radius_never_co_scheduling_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39972-39983` `speculation_over_a_structural_grounder_records_the_audit_and_routes_full` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39986-40088` `rule6_conflict_detection_grounds_on_the_safe_superset_so_a_grep_only_shared_reference_conflicts` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40091-40117` `glob_matches_supports_star_doublestar_and_literals` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40120-40144` `gate_intersects_radius_runs_unscoped_and_intersecting_gates_only` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40147-40298` `blast_radius_narrows_the_inner_loop_and_the_integrate_step_runs_the_full_library` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40301-40431` `a_gate_skipped_inline_but_red_at_the_exhaustive_integrate_door_blocks_the_merge` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40434-40463` `verdict_compensates_names_a_prior_unit_independent_of_approval` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40476-40520` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40524-40690` `a_contradiction_compensates_reverts_and_re_enters_the_integrated_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40694-40712` `with_integrations_ctx` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40715-40745` `commits_to_compensate_reverts_every_sha_a_multi_commit_landing_recorded` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40748-40799` `commits_to_compensate_dedupes_a_repeated_sha_and_skips_an_already_compensated_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40802-40832` `commits_to_compensate_excludes_the_review_only_marker_even_alongside_a_real_sha` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40835-40895` `pending_compensations_from_log_re_derives_only_undrained_marks` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40898-40929` `a_durable_compensation_queued_mark_is_fold_neutral_on_the_target` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40937-40976` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:40984-41057` `assert_compensated_unit_re_gates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:41086-41133` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:41173-41199` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:41203-41338` `a_resume_re_drives_a_durably_queued_but_undrained_compensation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:41341-41513` `integrate_re_gates_the_merged_tree_and_a_merge_break_blocks_the_second_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:41536-41605` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:41609-41770` `the_post_merge_re_gate_runs_in_its_own_scratch_worktree_never_the_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:41784-41831` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:41835-41938` `a_passing_post_merge_re_gate_reclaims_the_gate_scratch_root_its_fallback_names` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:41954-41983` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:41994-42105` `assert_a_replayed_post_merge_pass_reclaims_the_gate_scratch_root` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42113-42151` `assert_postmerge_err_reaps_its_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42154-42186` `postmerge_run_gates_err_still_reaps_the_throwaway_worktree_and_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42189-42225` `postmerge_worktree_create_err_still_reaps_the_just_created_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42228-42348` `conflict_regenerate_pending_from_log_re_derives_the_union_keyed_by_unit_and_attempt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42356-42377` `conflict_fixture` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42386-42441` `assert_both_conflicting_units_land` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42444-42464` `integrate_conflict_re_parks_the_implementer_with_no_attempt_charged_and_both_units_land` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42467-42489` `integrate_conflict_confined_to_a_regenerable_path_resolves_with_no_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42492-42591` `integrate_conflict_exhausted_after_the_bound_charges_a_real_attempt_with_the_unresolved_evidence` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42594-42743` `integrate_conflict_records_regenerate_pending_before_the_accept_incoming_mutation_that_can_fail` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42629-42673` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42749-42783` `deferred_gate_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42786-42788` `names_the_deferred_gate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42791-42836` `a_deferred_gate_runs_once_at_the_phase_boundary_not_inline` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42839-42860` `a_failing_deferred_gate_is_surfaced_and_the_run_is_not_done` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42871-42894` `run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42898-42931` `a_default_infra_fault_at_a_deferred_gate_does_not_demote` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42937-42955` `verify_only_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42963-42983` `replayed_twice` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42986-42991` `count_carrying` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42994-42996` `runs_of` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:42999-43038` `a_replayed_step_re_runs_no_recorded_gate_and_appends_no_duplicate_events` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:43041-43084` `a_re_step_replays_a_recorded_deferred_gate_without_re_running_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:43087-43238` `a_parked_step_never_records_a_partial_tree_deferred_verdict` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:43241-43328` `a_manual_review_paused_unit_defers_the_whole_tree_gate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:43331-43464` `an_escalated_dep_still_runs_the_whole_tree_deferred_gate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:43467-43586` `a_recorded_failing_deferred_verdict_re_surfaces_its_failure_on_replay` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:43589-43687` `a_replayed_fan_out_review_reject_appends_no_duplicate_unitfailed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:43690-43749` `a_fan_out_review_stage_whose_gates_fail_after_approval_records_a_gate_cause` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:43753-43755` `git_head` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:43766-43792` `run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:43796-43857` `gate_measures_the_committed_artifact_not_the_dirty_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:43860-43979` `conductor_spawns_the_sdet_author_at_the_build_seam_so_its_tests_land_in_the_committed_tree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:43985-44017` `sdet_prompts_around_a_rejected_round` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:44025-44059` `a_doc_only_round_tells_the_sdet_author_to_extend_the_owner_needle` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:44063-44075` `round_start` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:44082-44098` `round_delta_base_is_the_latest_review_round_before_the_attempt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:44105-44143` `a_round_delta_takes_no_base_outside_the_unit_branch_history` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:44146-44231` `the_sdet_author_spawn_respects_the_budget_breaker_at_its_own_spawn_site` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:44234-44331` `a_parked_sdet_author_spawn_holds_the_unit_instead_of_integrating_without_its_periphery_tests` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:44334-44439` `speculation_sdet_author_periphery_lands_in_the_committed_tree_the_gates_judge` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:44442-44534` `a_parked_sdet_author_in_a_speculation_candidate_holds_the_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:44542-44580` `assert_sdet_seam_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:44661-44753` `an_escalated_unit_does_not_integrate_its_code` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:44761-44771` `critique_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:44800-44811` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:44817-44823` `rejecting` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:44826-44879` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:44883-44954` `a_blast_radius_overlap_alone_does_not_reject` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:44967-45003` `the_plan_critique_gates_adversary_and_adjudicator_spawns_are_stamped_correctly` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45006-45038` `a_rule_7_or_8_defect_rejects_then_releases_on_the_revision` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45041-45087` `a_clean_decomposition_approves_and_releases_the_fan_out` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45090-45167` `the_gate_critiques_only_not_yet_run_units_and_approves` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45171-45197` `one_unit_critique` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45200-45215` `the_plan_critique_prompt_names_the_cross_unit_rules` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45221-45234` `the_dag_critique_prompt_pushes_the_shared_plan_critique_rules_between_opener_and_size` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45241-45264` `the_plan_protocol_and_the_dag_critique_carry_one_unit_size_cap` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45267-45292` `the_critique_gate_never_enters_a_wave_even_when_ready` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45301-45335` `fan_out_needs_template_fixture` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45338-45420` `a_downstream_stage_needing_the_fan_out_template_stays_unready_until_every_member_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45423-45527` `a_real_split_pair_must_both_integrate_not_just_the_btreemap_key_first_sibling` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45530-45608` `a_resolved_plan_critique_gate_does_not_re_run_on_resume` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45615-45625` `widget_split` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45630-45632` `critique_step` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45635-45654` `critique_step_under` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45657-45723` `a_resumed_step_holds_the_fan_out_while_the_plan_critique_gate_is_escalated` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45733-45754` `reject_then_approve_critique` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45757-45796` `a_plan_critique_round_re_entered_in_a_later_process_opens_with_the_logged_reject` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45806-45808` `stop_halt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45811-45817` `stop_lesson` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45823-45843` `critique_rounds` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45847-45857` `critique_round_spawns` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45861-45865` `launched_on_stop_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45869-45874` `rejecting_twice_for_spec_ambiguity` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45879-45881` `stopped_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45884-45889` `stopped_run_under` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45896-45922` `the_stop_is_decided_before_an_exhausted_remediation_bound_escalates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45930-45970` `a_resumed_stopped_gate_re_plans_its_next_spec_ambiguity_reject_as_a_first_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:45973-46036` `a_spec_ambiguity_reject_after_a_re_plan_that_did_not_clear_it_stops_the_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46041-46056` `assert_re_plans_after_every_reject` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46059-46064` `a_first_spec_ambiguity_reject_re_plans_as_today` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46067-46075` `a_spec_ambiguity_reject_after_a_decomposition_conflict_re_plans_again` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46078-46093` `a_stop_with_no_upheld_finding_and_no_spec_says_so` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46096-46121` `a_stopped_gate_halts_the_step_before_the_coverage_check` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46128-46148` `a_later_step_finds_the_stopped_gate_terminal_and_reports_its_halt_appending_nothing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46151-46180` `a_step_re_entering_a_crashed_stop_completes_it_without_spawning` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46183-46221` `a_stop_whose_lesson_append_fails_records_nothing_after_it_and_the_next_step_completes_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46225-46237` `gate_failed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46240-46242` `re_plan_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46245-46249` `re_plan_requested` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46253-46255` `stops_at` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46259-46265` `two_spec_ambiguity_rejects` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46268-46304` `the_stop_holds_on_two_spec_ambiguity_rejects_around_a_recorded_re_plan` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46307-46383` `the_stop_does_not_hold_short_of_its_whole_shape` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46386-46393` `the_stop_holds_at_any_attempt_its_two_rejects_precede` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46396-46411` `the_spec_defect_halt_names_the_spec_and_the_upheld_findings` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46416-46434` `the_budget_halt_takes_precedence_over_the_spec_defect_stop` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46439-46446` `the_dag_critique_verdict_paragraph_carries_the_spec_defect_cause_contract` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46449-46507` `an_approved_gate_releases_planner_proposed_units_not_only_baselines` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46510-46582` `the_re_plan_directive_instructs_reusing_the_existing_unit_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46595-46600` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46603-46624` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46628-46658` `a_resumed_step_holds_the_fan_out_while_the_plan_critique_gate_is_mid_review` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46661-46716` `a_parked_plan_critique_gate_keeps_its_review_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46719-46763` `a_budget_refused_plan_critique_gate_keeps_its_review_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:46776-46795` `the_conductors_one_event_authority_reports_a_write_the_store_lost` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
- `conductor::tests::run_sink` (47 functions)
  - `crates/rigger-conductor/src/conductor.rs:20837-20839` `sized` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20842-20844` `sized_hash` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20847-20849` `failing_hash` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20852-20865` `sink_deps` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20869-20882` `recorded` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20885-20887` `entries` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20890-20901` `planted_body` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20905-20912` `walked_entries` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20916-20919` `emit` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20923-20930` `reindexed` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20933-20944` `live_names` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20966-20975` `unless` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20979-20985` `apply` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20986-20994` `apply_generation` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20995-21002` `current_generation` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21003-21005` `rebuild_owed` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21006-21008` `subgraph` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21009-21011` `resolve` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21023-21089` `a_whole_tree_walk_records_one_ledger_entry_per_file_from_its_bytes_and_no_derived_event` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21094-21118` `an_entry_recorded_by_a_run_carries_the_runs_id` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21124-21145` `an_integration_reindex_naming_an_out_of_line_test_module_records_its_flag_set` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21151-21178` `a_file_whose_generation_the_log_holds_and_the_graph_does_not_is_recorded_again` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21185-21232` `a_deleted_gc_file_and_one_outside_the_walks_scope_each_record_an_entry_with_no_blob` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21240-21287` `a_lagging_lowering_records_the_bytes_generation_and_nothing_once_both_sides_hold_it` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21293-21323` `a_tree_that_is_not_a_git_repository_records_the_blob_id_git_hash_object_gives` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21328-21351` `a_batch_whose_key_names_no_identity_fails_the_emit_and_records_nothing` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21355-21375` `a_failing_hash_function_fails_the_emit_and_records_nothing` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21380-21401` `a_read_failing_for_a_reason_other_than_absence_fails_the_emit_and_records_nothing` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21406-21450` `a_failing_read_of_the_log_side_or_of_the_graph_side_fails_the_emit_and_records_nothing` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21455-21473` `an_emit_wired_to_no_graph_fails_and_records_nothing` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21479-21494` `a_gd_file_emptied_after_the_walk_records_nothing_and_the_emit_succeeds` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21501-21551` `a_graph_that_owes_its_rebuild_records_one_entry_per_generation` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21556-21595` `an_entry_whose_ledger_fold_fails_is_recorded_and_the_lost_fold_is_said` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21601-21657` `a_revert_a_b_a_records_three_entries_and_leaves_as_facts` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21663-21694` `a_batch_whose_append_is_refused_at_first_sight_appends_whole_on_the_next_sight` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21701-21755` `a_generation_whose_append_is_refused_leaves_the_process_on_the_recorded_generation` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21764-21832` `a_first_sight_lookup_racing_a_newer_generation_leaves_the_process_on_the_stored_generation` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21839-21872` `a_run_ingest_whose_group_lookup_is_unanswered_fails_appends_nothing_and_walks_again` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21877-21898` `an_integration_reindex_whose_group_lookup_is_unanswered_fails_and_appends_nothing` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21908-21933` `a_current_batch_handed_twice_makes_one_group_lookup` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21942-21989` `an_identity_is_looked_up_once_and_its_later_batches_answer_from_the_entry_recorded` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21996-22040` `a_batch_whose_group_lookup_failed_handed_again_makes_a_second_lookup` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22045-22060` `handed_twice_over_an_empty_store` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22063-22072` `the_same_answer_twice_from_one_lookup` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22083-22146` `an_identity_the_log_holds_no_recording_of_is_still_looked_up_once_when_its_batch_records_nothing` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22158-22197` `a_pre_ledger_identity_takes_its_entrys_generation_so_its_second_walk_asks_no_lookup` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22212-22279` `a_long_lived_run_restores_an_identity_a_rebuild_left_behind_at_its_next_reindex` - defined inside `run_sink`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
- `conductor::tests::support` (14 functions)
  - `crates/rigger-conductor/src/conductor.rs:14201-14203` `occurrences` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14206-14208` `snapshot` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14213-14229` `stub_deps` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14233-14241` `run_logged` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14244-14248` `stage_map` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14252-14266` `stage_shape` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14270-14285` `grant_resume` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14300-14307` `apply` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14308-14315` `apply_batch` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14316-14318` `subgraph` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14319-14321` `resolve` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14322-14324` `rebuild_owed` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14325-14333` `apply_generation` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14334-14339` `current_generation` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
- `conductor::throwaway` (1 function)
  - `crates/rigger-conductor/src/conductor.rs:13950-13956` `dir_and_branch` - method inside `impl Throwaway`; grouped with its other `Throwaway` methods.
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
  - `crates/rigger-dash/src/dash.rs:1613-1645` `build_graph_view` - name contains "graph" (HTML rendering); grouped under `dash::render`.
  - `crates/rigger-dash/src/dash.rs:1912-1926` `rationale_batch` - name contains "rationale" (HTML rendering); grouped under `dash::render`.
  - `crates/rigger-dash/src/dash.rs:1942-1974` `graph_json` - name contains "graph" (HTML rendering); grouped under `dash::render`.
  - `crates/rigger-dash/src/dash.rs:2012-2024` `call_node_view` - name contains "node" (HTML rendering); grouped under `dash::render`.
  - `crates/rigger-dash/src/dash.rs:2041-2053` `ref_node_view` - name contains "node" (HTML rendering); grouped under `dash::render`.
  - `crates/rigger-dash/src/dash.rs:2340-2421` `unit_node` - name contains "node" (HTML rendering); grouped under `dash::render`.
  - `crates/rigger-dash/src/dash.rs:2425-2484` `role_stage` - name contains "role" (HTML rendering); grouped under `dash::render`.
  - `crates/rigger-dash/src/dash.rs:2532-2543` `stage_of_role` - name contains "role" (HTML rendering); grouped under `dash::render`.
  - `crates/rigger-dash/src/dash.rs:2551-2572` `role_and_agent` - name contains "role" (HTML rendering); grouped under `dash::render`.
  - `crates/rigger-dash/src/dash.rs:2653-2661` `graph_seeds` - name contains "graph" (HTML rendering); grouped under `dash::render`.
  - `crates/rigger-dash/src/dash.rs:2852-2858` `field_str` - name contains "field" (HTML rendering); grouped under `dash::render`.
  - `crates/rigger-dash/src/dash.rs:2862-2873` `field_str_array` - name contains "field" (HTML rendering); grouped under `dash::render`.
  - `crates/rigger-dash/src/dash.rs:3035-3048` `escape_for_script` - name contains "escape" (HTML rendering); grouped under `dash::render`.
- `dash::reproject` (5 functions)
  - `crates/rigger-dash/src/dash.rs:1681-1694` `reproject` - name contains "reproject" (graph reprojection); grouped under `dash::reproject`.
  - `crates/rigger-dash/src/dash.rs:1702-1721` `cap_clusters` - name contains "cluster" (graph reprojection); grouped under `dash::reproject`.
  - `crates/rigger-dash/src/dash.rs:1739-1750` `reprojection_lens_key` - name contains "reproject" (graph reprojection); grouped under `dash::reproject`.
  - `crates/rigger-dash/src/dash.rs:1756-1799` `reproject_derived` - name contains "reproject" (graph reprojection); grouped under `dash::reproject`.
  - `crates/rigger-dash/src/dash.rs:1820-1905` `reproject_files` - name contains "reproject" (graph reprojection); grouped under `dash::reproject`.
- `dash::response` (6 functions)
  - `crates/rigger-dash/src/dash.rs:3070-3076` `new` - method inside `impl Response`; grouped with its other `Response` methods.
  - `crates/rigger-dash/src/dash.rs:3079-3081` `rendered` - method inside `impl Response`; grouped with its other `Response` methods.
  - `crates/rigger-dash/src/dash.rs:3082-3088` `text` - method inside `impl Response`; grouped with its other `Response` methods.
  - `crates/rigger-dash/src/dash.rs:3094-3096` `binary` - method inside `impl Response`; grouped with its other `Response` methods.
  - `crates/rigger-dash/src/dash.rs:3098-3108` `reason` - method inside `impl Response`; grouped with its other `Response` methods.
  - `crates/rigger-dash/src/dash.rs:3118-3134` `write_to` - method inside `impl Response`; grouped with its other `Response` methods.
- `dash::server` (8 functions)
  - `crates/rigger-dash/src/dash.rs:541-553` `bind_singleton` - name contains "bind" (HTTP serving); grouped under `dash::server`.
  - `crates/rigger-dash/src/dash.rs:657-676` `tcp_listen_inode_for_port` - name contains "tcp" (HTTP serving); grouped under `dash::server`.
  - `crates/rigger-dash/src/dash.rs:717-719` `process_state` - name contains "process_" (HTTP serving); grouped under `dash::server`.
  - `crates/rigger-dash/src/dash.rs:2169-2206` `calls_route` - name contains "route" (HTTP serving); grouped under `dash::server`.
  - `crates/rigger-dash/src/dash.rs:3143-3324` `route` - name contains "route" (HTTP serving); grouped under `dash::server`.
  - `crates/rigger-dash/src/dash.rs:3400-3442` `serve_on` - name contains "serve" (HTTP serving); grouped under `dash::server`.
  - `crates/rigger-dash/src/dash.rs:3458-3637` `handle_conn` - name contains "handle" (HTTP serving); grouped under `dash::server`.
  - `crates/rigger-dash/src/dash.rs:3726-3851` `serve_console_stream` - name contains "serve" (HTTP serving); grouped under `dash::server`.
- `dash::tests` (110 functions)
  - `crates/rigger-dash/src/dash.rs:3886-3899` `get_static` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:3903-3909` `assert_console_page_carries` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:3911-3923` `seeded_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:3925-3935` `local_instance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:3943-3994` `instance_views_project_a_sorted_credential_free_landing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4001-4009` `instance_view_age_floors_at_zero_for_a_future_heartbeat` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4016-4048` `api_instances_route_renders_the_landing_list` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4055-4060` `console_core_wasm_route_serves_the_embedded_artifact_as_application_wasm` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4065-4077` `console_core_wasm_artifact_is_under_the_three_megabyte_budget` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4089-4117` `embedded_artifact_matches_a_fresh_independent_nested_build` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4129-4179` `console_route_serves_the_shell_page_with_mock_regions_and_both_theme_token_blocks` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4185-4194` `console_route_never_references_an_external_url` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4201-4237` `console_fonts_route_serves_each_embedded_font_and_its_license_text` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4253-4265` `console_page_wires_the_theme_toggles_persistence_round_trip` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4268-4282` `root_serves_the_embedded_page_with_the_placeholder_resolved` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4293-4343` `gates_status_never_fabricates_passed_for_an_off_linear_unverdicted_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4350-4386` `free_port_from_returns_the_start_port_when_free_and_the_next_free_one_when_it_is_taken` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4393-4442` `bind_singleton_binds_the_exact_port_and_never_searches` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4453-4526` `bind_singleton_short_circuits_on_an_already_serving_rigger_dash` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4534-4544` `serve_reply` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4548-4566` `serve_dribble` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4571-4581` `timed_dash_probe` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4584-4586` `assert_pid_probe` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4593-4598` `dash_serving_on_is_false_for_a_non_dash_listener` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4609-4626` `dash_serving_on_is_bounded_against_a_byte_dribbling_holder` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4649-4671` `dash_serving_on_recognizes_the_header_fast_even_if_the_holder_never_finishes_the_block` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4763-4766` `dash_serving_pid_on_is_none_when_nothing_answers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4774-4803` `bind_singleton_cold_race_loser_resolves_across_the_accept_window` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4814-4849` `the_page_layout_cannot_scroll_the_body_horizontally` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4860-4897` `the_landing_view_lists_instances_and_threads_the_attach_selector` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4902-4911` `css_rule` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4922-4967` `the_dashboard_fits_one_screen_with_internal_scroll` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:4981-5015` `cells_fit_or_wrap_and_wide_cells_scroll_in_their_own_container` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5030-5032` `the_decision_history_renders_each_decision_as_a_native_details_with_preview_and_full_body` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5035-5063` `state_endpoint_projects_the_seeded_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5070-5110` `console_event_filter_admits_only_the_named_run_lifecycle_types` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5116-5124` `console_event_wire_matches_console_cores_wire_event_shape` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5132-5138` `console_event_wire_carries_recorded_at` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5144-5151` `console_event_wire_with_a_malformed_body_degrades_to_null_data` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5157-5164` `console_progress_wire_with_a_malformed_body_degrades_to_empty_id_and_activity` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5167-5248` `console_snapshot_endpoint_filters_events_carries_progress_liveness_and_definitions` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5251-5312` `state_carries_the_live_agent_activity` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5315-5381` `state_counts_grep_fallbacks_and_carries_them_in_the_review_outcomes_data` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5390-5576` `run_tree_projects_the_spine_with_collapse_expand_and_driver_lines` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5396-5401` `done` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5581-5610` `review_verdicts_come_straight_from_the_metrics_classification` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5613-5623` `events_endpoint_is_since_exclusive` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5629-5658` `no_mutating_endpoint_exists` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5661-5690` `export_inlines_the_snapshot_as_a_static_page` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5698-5745` `export_neutralizes_a_script_breakout_in_the_inlined_state` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5748-5774` `decision_view_strikes_through_superseded_entries` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5783-5846` `cluster_key_folds_paths_by_directory_and_dev_loop_nodes_by_kind` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5858-5951` `clustered_overview_under_files_lens_admits_only_code_entities_keyed_by_their_own_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:5970-6213` `cluster_detail_drills_a_cluster_to_its_members_and_caps_a_big_one_by_degree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:6222-6267` `cluster_detail_under_files_lens_is_unconditionally_empty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:6284-6481` `code_lens_buckets_code_entities_by_community_excludes_other_kinds_and_reports_underived_grain` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:6495-6734` `concepts_lens_buckets_members_by_concept_excludes_membershipless_nodes_and_reports_underived_grain` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:6740-6757` `tiered_chain_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:6760-6824` `the_graph_route_returns_a_tier_tagged_seeded_neighborhood_as_json` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:6827-6878` `the_graph_route_percent_decodes_the_seed_so_select_to_seed_reaches_ids_with_special_chars` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:6881-6912` `the_graph_route_degrades_gracefully_for_an_unknown_seed_and_an_empty_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:6915-6935` `the_graph_route_is_read_only_a_non_get_is_405` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:6941-6975` `dispatch_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:6984-7082` `the_graph_route_dispatches_cluster_overview_and_seed_by_parameter` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:7096-7150` `the_overview_route_degrades_gracefully_on_an_empty_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:7153-7207` `neighborhood_bounds_by_depth_follows_both_directions_and_skips_invalidated_edges` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:7210-7243` `neighborhood_flags_god_nodes_by_degree_within_the_returned_neighborhood` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:7246-7318` `path_is_the_shortest_route_between_two_selected_nodes_over_currently_valid_edges` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:7321-7388` `the_graph_route_flags_god_nodes_and_returns_the_query_path_between_two_selected_nodes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:7395-7432` `provenance_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:7435-7482` `explain_returns_a_nodes_incident_edges_as_source_and_tier_tagged_provenance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:7485-7550` `the_graph_route_carries_the_seed_nodes_explain_provenance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:7553-7584` `graph_seeds_enumerate_decisions_findings_and_their_files_never_units` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:7587-7631` `a_units_seed_lands_on_the_neighborhood_of_its_decisions_and_files` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:7634-7740` `the_run_tree_click_to_seed_route_lands_a_unit_on_a_real_neighborhood` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:7743-7796` `unit_seeds_scope_content_to_the_owning_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:7799-7835` `repoint_seed_passes_a_known_node_and_re_points_a_unit_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:7841-7870` `a_unit_view_carries_the_spec_defect_reason_of_a_gate_stopped_on_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:7876-7895` `the_spec_defect_block_prints_a_stopped_gate_s_reason_before_the_uncovered_text` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:7898-7917` `build_state_on_an_empty_run_is_empty_not_a_panic` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:7928-8017` `release_ready_is_surfaced_on_the_dash_only_for_a_done_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8035-8090` `release_ready_pr_command_newline_renders_as_a_real_line_break_not_a_collapsed_run_on` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8093-8100` `request_line_parsing_extracts_method_and_target` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8103-8110` `query_param_reads_since` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8118-8196` `endpoints_serve_over_a_real_socket_against_a_seeded_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8202-8253` `a_post_over_a_real_socket_is_refused_without_touching_the_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8264-8376` `the_graph_provider_is_consulted_only_on_graph_requests_not_the_state_poll` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8381-8391` `dash_marker_round_trips_through_its_on_disk_record` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8394-8413` `dash_marker_parse_rejects_a_malformed_record` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8416-8434` `dash_marker_reads_none_for_an_absent_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8439-8451` `format_held_port_always_names_the_address_even_with_no_holder` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8454-8471` `format_held_port_names_the_pid_and_state_for_a_running_holder` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8474-8482` `format_held_port_names_the_pid_alone_when_its_state_is_not_discoverable` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8485-8500` `format_held_port_gives_the_stopped_listener_diagnosis_naming_resume_or_kill` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8503-8515` `pid_holding_port_finds_the_pid_of_a_listener_bound_in_this_process` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8518-8534` `pid_holding_port_is_none_for_a_port_nothing_is_listening_on` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8542-8555` `assert_names_this_process_as_holder` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8584-8600` `describe_held_port_if_confirmed_is_none_when_nothing_holds_the_port` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8603-8620` `dash_start_needed_is_true_when_none_serving_and_false_when_one_serves` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8625-8631` `dash_answer_on_tells_a_silent_holder_from_an_empty_port` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8639-8661` `an_interrupted_probe_read_is_read_again_inside_the_window` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8670-8702` `a_probe_error_reads_as_the_peer_a_silent_holder_or_the_probe_failing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8707-8715` `a_failed_probe_answers_neither_serving_nor_gone` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8718-8792` `dash_status_trusts_a_url_with_no_marker_and_catches_a_marker_that_lies` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8804-8820` `dash_status_never_names_the_unattributed_pid_sentinel_as_a_dead_process` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8823-8844` `url_port_parses_the_recorded_shape_and_rejects_anything_else` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8860-8900` `dash_status_probes_the_urls_own_port_when_the_marker_names_a_different_dash` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8903-8941` `should_reap_singleton_reaps_only_when_no_registered_instance_is_live` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8944-8977` `should_reap_singleton_never_reaps_while_a_fresh_agent_liveness_signal_is_present` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:8989-9060` `the_page_carries_the_directed_call_layered_render` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
- `dash::tests::calls_route_c4` (10 functions)
  - `crates/rigger-dash/src/dash.rs:9078-9088` `cnode` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9090-9092` `layer_of` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9093-9095` `ids` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9101-9156` `calls_view_down_signs_callees_positive_and_carries_frontier_and_back` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9161-9200` `calls_view_up_negates_callers_and_carries_the_referenced_sidecar` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9206-9254` `calls_view_both_centers_the_seed_with_callees_right_and_callers_left` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9260-9297` `a_plain_neighborhood_omits_every_additive_call_field` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9304-9358` `calls_route_runs_the_traversal_for_view_calls_and_declines_otherwise` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9364-9392` `calls_route_clamps_depth_and_defaults_the_tier_floor` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9397-9415` `calls_route_walks_both_directions_for_dir_both` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
- `dash::tests::metadata_card_c2` (12 functions)
  - `crates/rigger-dash/src/dash.rs:10009-10059` `card_graph` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:10062-10105` `card_of_a_code_entity_carries_file_line_degree_community_concepts_and_memory_counts` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:10111-10119` `card_of_a_membership_less_entity_has_no_community_and_no_line` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:10127-10154` `card_of_a_proven_code_entity_carries_proven_by_and_proof_evidence` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:10162-10167` `card_of_an_unproven_code_entity_has_proven_by_zero_and_no_evidence` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:10171-10180` `assert_card_reports_no_proof` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:10207-10244` `card_of_a_file_lists_its_contained_entities_as_top_entities` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:10249-10269` `card_of_a_concept_lists_its_realizing_members_as_top_evidence` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:10278-10303` `card_of_a_concept_carries_each_top_evidence_members_own_kind` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:10310-10324` `card_of_a_file_carries_each_top_entity_members_own_kind` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:10329-10332` `card_of_an_unknown_id_is_none` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:10338-10380` `the_card_route_serves_a_known_subjects_card_and_null_for_an_unknown_one` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
- `dash::tests::rationale_overlay_c3` (12 functions)
  - `crates/rigger-dash/src/dash.rs:9437-9447` `finding_node` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9463-9496` `rationale_graph` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9499-9501` `leaf_fields` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9507-9521` `node_rationale_returns_attached_leaves_ordered_by_kind_then_id` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9527-9550` `a_finding_leaf_carries_content_only_never_the_by_or_unit_machinery` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9556-9569` `a_handbook_rule_and_a_supersedes_edge_are_not_rationale` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9574-9582` `an_invalidated_edge_is_not_live_rationale` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9587-9597` `a_node_without_rationale_returns_none` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9603-9628` `the_batch_covers_the_visible_set_and_keeps_only_nodes_with_rationale` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9633-9650` `the_batch_is_deterministic_across_request_order_and_dedups` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9656-9695` `the_explain_route_returns_the_batch_in_one_request` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9701-9725` `an_absent_explain_leaves_the_graph_route_unchanged` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
- `dash::tests::subject_view_c5` (5 functions)
  - `crates/rigger-dash/src/dash.rs:9746-9789` `memory_rail_lists_decisions_findings_and_concepts_excluding_lessons` - defined inside `subject_view_c5`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9794-9801` `memory_rail_is_empty_for_a_node_with_no_governing_memory` - defined inside `subject_view_c5`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9822-9899` `memory_rail_concepts_are_live_from_node_realizes_edges_to_a_concept_target_deduped_by_id` - defined inside `subject_view_c5`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9907-9957` `the_seeded_route_carries_memory_without_adding_a_single_node_to_the_neighborhood` - defined inside `subject_view_c5`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-dash/src/dash.rs:9963-9983` `a_cluster_drill_carries_no_memory_field` - defined inside `subject_view_c5`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
- `main::commands` (1 function)
  - `src/cli/mod.rs:2466-2623` `cmd_replay` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
- `main::dash_glue` (1 function)
  - `src/cli/mod.rs:2863-2867` `recorded_dash_url` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
- `main::docs_overlay` (1 function)
  - `src/cli/mod.rs:4592-4599` `apply` - method inside `impl DocsOverlay`; grouped with its other `DocsOverlay` methods.
- `main::liveness` (4 functions)
  - `src/cli/mod.rs:1793-1803` `live_branches_for_sweep` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/cli/mod.rs:2888-2897` `liveness_ages_for_wave` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/cli/mod.rs:3432-3439` `live_slugs` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/cli/mod.rs:3642-3660` `worktree_belongs_to_live` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
- `main::provenance` (12 functions)
  - `src/cli/mod.rs:158-160` `workflow_path` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/cli/mod.rs:293-306` `spec_positional` - name contains "spec_" (workflow/spec loading); grouped under `main::provenance`.
  - `src/cli/mod.rs:1958-2000` `refuse_when_base_lacks_spec_paths` - name contains "spec_" (workflow/spec loading); grouped under `main::provenance`.
  - `src/cli/mod.rs:3277-3282` `spec_lint_warning_lines` - name contains "spec_" (workflow/spec loading); grouped under `main::provenance`.
  - `src/cli/mod.rs:3289-3291` `workflow_provenance_path` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/cli/mod.rs:3296-3304` `installed_workflow_provenance` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/cli/mod.rs:4527-4537` `install_workflow` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/cli/mod.rs:4607-4616` `read_docs_overlay` - name contains "docs_" (docs overlay); grouped under `main::provenance`.
  - `src/cli/mod.rs:4957-5001` `docs_context` - name contains "docs_" (docs overlay); grouped under `main::provenance`.
  - `src/cli/mod.rs:5051-5057` `spec_lint_next_step` - name contains "spec_" (workflow/spec loading); grouped under `main::provenance`.
  - `src/cli/mod.rs:5088-5090` `git_repo` - name contains "git_" (git/version provenance); grouped under `main::provenance`.
  - `src/cli/mod.rs:5098-5106` `git_repo_at` - name contains "git_" (git/version provenance); grouped under `main::provenance`.
- `main::render` (4 functions)
  - `src/cli/mod.rs:2124-2166` `format_stats` - name contains "format_" (human-readable output); grouped under `main::render`.
  - `src/cli/mod.rs:2319-2415` `format_canary_stats` - name contains "format_" (human-readable output); grouped under `main::render`.
  - `src/cli/mod.rs:2815-2825` `format_stats_diff` - name contains "format_" (human-readable output); grouped under `main::render`.
  - `src/cli/mod.rs:3877-3907` `format_residue` - name contains "format_" (human-readable output); grouped under `main::render`.
- `main::replay_runner` (1 function)
  - `src/cli/mod.rs:2836-2856` `run` - method inside `impl Runner for ReplayRunner`; grouped with its other `ReplayRunner` methods.
- `main::residue_report` (1 function)
  - `src/cli/mod.rs:3330-3335` `is_empty` - method inside `impl ResidueReport`; grouped with its other `ResidueReport` methods.
- `main::setup` (9 functions)
  - `src/cli/mod.rs:1463-1466` `scratch_defaults` - name contains "scratch" (project setup); grouped under `main::setup`.
  - `src/cli/mod.rs:2015-2033` `locate_shim` - name contains "shim" (project setup); grouped under `main::setup`.
  - `src/cli/mod.rs:3999-4019` `scratch_totals` - name contains "scratch" (project setup); grouped under `main::setup`.
  - `src/cli/mod.rs:4195-4230` `classify_agent_scratch` - name contains "scratch" (project setup); grouped under `main::setup`.
  - `src/cli/mod.rs:4496-4513` `install_file_if_changed` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/cli/mod.rs:4545-4547` `skill_source_rel` - name contains "skill" (project setup); grouped under `main::setup`.
  - `src/cli/mod.rs:4556-4561` `skill_install_path` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/cli/mod.rs:4629-4642` `install_skills` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/cli/mod.rs:4940-4947` `write_shim_files` - name contains "shim" (project setup); grouped under `main::setup`.
- `main::store` (21 functions)
  - `src/cli/mod.rs:269-280` `eventstore_flag` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:520-538` `store_conn_file` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:600-613` `store_backend_kind` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:663-729` `store_selection_at` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:737-743` `store_selection` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:771-784` `registry_store_identity` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:1064-1135` `migrate_project_identity` - name contains "migrate" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:1144-1149` `migrate_local_identity` - name contains "migrate" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:1161-1191` `migrate_identity_at` - name contains "migrate" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:1302-1304` `find_store_dir_from` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:1477-1482` `server_store_location` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:1513-1578` `require_store_dir` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:1600-1602` `store_file` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:3468-3559` `reclaim_orphan_scratch` - name contains "reclaim_" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:3815-3856` `reclaim_shared_build_cache` - name contains "reclaim_" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:4036-4088` `scratch_footprint` - name contains "footprint" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:4098-4121` `store_and_backup_bytes` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:4245-4328` `footprint_report` - name contains "footprint" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:4337-4357` `footprint_advisories` - name contains "footprint" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:4377-4408` `reclaim_dead_footprint` - name contains "reclaim_" (store hygiene); grouped under `main::store`.
  - `src/cli/mod.rs:4412-4438` `footprint_reclaim_lines` - name contains "reclaim_" (store hygiene); grouped under `main::store`.
- `main::store_location` (4 functions)
  - `src/cli/mod.rs:1396-1399` `graph` - method inside `impl StoreLocation`; grouped with its other `StoreLocation` methods.
  - `src/cli/mod.rs:1403-1405` `file` - method inside `impl StoreLocation`; grouped with its other `StoreLocation` methods.
  - `src/cli/mod.rs:1411-1418` `identity` - method inside `impl StoreLocation`; grouped with its other `StoreLocation` methods.
  - `src/cli/mod.rs:1431-1437` `repo_root` - method inside `impl StoreLocation`; grouped with its other `StoreLocation` methods.
- `main::store_selection` (2 functions)
  - `src/cli/mod.rs:402-404` `is_sqlite` - method inside `impl StoreSelection`; grouped with its other `StoreSelection` methods.
  - `src/cli/mod.rs:410-415` `handed_env` - method inside `impl StoreSelection`; grouped with its other `StoreSelection` methods.
- `main::support` (18 functions)
  - `src/cli/mod.rs:312-365` `parse_run_args` - name contains "parse_" (argument/input parsing); grouped under `main::support`.
  - `src/cli/mod.rs:376-384` `resolve_run_base` - name contains "resolve" (path/id resolution helper); grouped under `main::support`.
  - `src/cli/mod.rs:545-547` `conn_file_is_group_or_other_readable` - name contains "is_" (predicate helper); grouped under `main::support`.
  - `src/cli/mod.rs:555-577` `warn_if_conn_file_is_exposed` - name contains "is_" (predicate helper); grouped under `main::support`.
  - `src/cli/mod.rs:620-643` `resolve_conn` - name contains "resolve" (path/id resolution helper); grouped under `main::support`.
  - `src/cli/mod.rs:751-761` `resolve_store` - name contains "resolve" (path/id resolution helper); grouped under `main::support`.
  - `src/cli/mod.rs:939-948` `read_project_id` - name contains "read_" (generic read helper); grouped under `main::support`.
  - `src/cli/mod.rs:1348-1371` `resolve_main_worktree_or_refuse` - name contains "resolve" (path/id resolution helper); grouped under `main::support`.
  - `src/cli/mod.rs:1859-1861` `read_run_progress` - name contains "read_" (generic read helper); grouped under `main::support`.
  - `src/cli/mod.rs:1869-1878` `read_project_stream` - name contains "read_" (generic read helper); grouped under `main::support`.
  - `src/cli/mod.rs:2425-2433` `read_model_drift` - name contains "read_" (generic read helper); grouped under `main::support`.
  - `src/cli/mod.rs:2700-2732` `parse_replay_args` - name contains "parse_" (argument/input parsing); grouped under `main::support`.
  - `src/cli/mod.rs:2929-2959` `parse_watch_args` - name contains "parse_" (argument/input parsing); grouped under `main::support`.
  - `src/cli/mod.rs:3664-3666` `is_uuid8` - name contains "is_" (predicate helper); grouped under `main::support`.
  - `src/cli/mod.rs:3673-3704` `find_shadow_stores` - name contains "find_" (lookup helper); grouped under `main::support`.
  - `src/cli/mod.rs:3767-3772` `is_build_cache_tombstone` - name contains "is_" (predicate helper); grouped under `main::support`.
  - `src/cli/mod.rs:4843-4848` `find_bytes` - name contains "find_" (lookup helper); grouped under `main::support`.
  - `src/cli/mod.rs:5011-5029` `write_docs` - name contains "write_" (generic write helper); grouped under `main::support`.
- `main::tests` (199 functions)
  - `src/cli/mod.rs:4930-4935` `compose_precommit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:5146-5184` `the_rebuild_owed_note_speaks_only_for_a_graph_db_that_owes_it_and_creates_none` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:5188-5202` `test_request` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:5211-5218` `spec_lint_next_step_names_rigger_validate_and_the_given_spec_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:5235-5257` `spec_lint_warning_lines_formats_every_advisory_with_the_spec_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:5262-5265` `spec_lint_warning_lines_is_empty_on_a_clean_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:5275-5308` `compose_precommit_fresh_install_carries_the_managed_block` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:5318-5345` `precommit_block_refuses_on_drift_instead_of_staging` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:5359-5412` `precommit_block_finds_the_relocated_unit_target_with_the_binary_s_own_path_encoding` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:5415-5455` `shipped_workflow_driver_tells_a_worker_its_units_build_location` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:5458-5559` `precommit_block_resolves_a_tree_built_binary_before_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:5566-5578` `compose_precommit_is_idempotent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:5589-5618` `compose_precommit_chains_without_clobbering_an_existing_hook` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:5628-5642` `compose_precommit_prepends_before_a_terminal_exit_existing_hook` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:5649-5672` `compose_precommit_refreshes_a_stale_block_in_place` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:5682-5700` `version_line_carries_the_derived_version_and_a_non_empty_build_provenance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:5707-5751` `docs_context_reads_every_fact_from_code` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:5758-5799` `docs_render_surfaces_known_code_facts_verbatim` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:5805-5824` `commands_registry_is_well_formed_and_covers_dispatch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:5829-5843` `usage_text_lists_the_docs_verb_on_its_own_line` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:5851-5881` `write_docs_writes_every_registry_skill_plus_the_handbook` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:5891-5928` `install_and_docs_each_cover_exactly_the_registry_no_more_no_less` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:5932-5955` `assert_skills_reference_only_real_subcommands` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6019-6045` `committed_registry_docs_are_in_sync_with_a_fresh_render` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6056-6125` `status_and_dashboard_render_the_same_current_blocker_lines` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6128-6141` `install_workflow_records_the_build_provenance_beside_the_workflow` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6147-6149` `slugs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6154-6166` `init_committed_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6184-6200` `resolve_main_worktree_or_refuse_returns_exactly_git_rev_parse_show_toplevel` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6203-6227` `refuse_when_base_lacks_spec_paths_refuses_on_total_absence_and_names_a_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6231-6249` `assert_base_path_check_proceeds` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6289-6362` `refuse_when_base_unreachable_fails_loudly_only_when_no_reachable_base` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6365-6372` `human_size_formats_bytes_through_gib` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6375-6381` `is_uuid8_accepts_exactly_eight_hex_digits` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6384-6429` `worktree_belongs_to_live_matches_both_naming_shapes_without_prefix_false_match` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6432-6479` `current_run_units_scopes_to_the_current_run_and_splits_live_from_dead` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6482-6519` `current_run_units_spares_a_terminal_units_branch_whose_latest_spawn_is_still_in_flight` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6522-6548` `current_run_units_still_retires_a_terminal_unit_once_its_latest_spawn_answers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6551-6598` `current_run_units_splits_live_spawns_from_answered_ones_scoped_to_the_current_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6610-6636` `live_branches_for_sweep_fails_closed_on_an_unreadable_stream_but_folds_a_readable_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6639-6688` `find_shadow_stores_finds_nested_events_db_and_prunes_build_caches` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6691-6702` `dir_size_bytes_sums_files_recursively` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6707-6723` `reclaim_shared_build_cache_deletes_a_populated_cache_and_reports_its_bytes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6726-6744` `reclaim_shared_build_cache_is_idempotent_zero_report_when_absent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6747-6775` `reclaim_shared_build_cache_refuses_rather_than_waits_when_a_build_holds_the_guard` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6778-6801` `reclaim_shared_build_cache_releases_the_guard_promptly_after_a_successful_reclaim` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6804-6888` `scan_residue_reports_dead_worktrees_caches_shadows_and_branches` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6891-6910` `scan_residue_is_empty_when_everything_is_live_and_no_shadow_stores` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6913-6930` `format_residue_renders_a_sized_warning_block` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6935-6954` `store_and_backup_bytes_splits_live_store_files_from_dot_bak_backups` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6957-6961` `store_and_backup_bytes_is_zero_on_a_missing_directory` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:6964-7034` `scratch_footprint_totals_every_entry_and_dead_only_the_non_live_share` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7037-7098` `footprint_report_measures_every_category_on_a_seeded_fixture_tree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7104-7120` `seed_agent_scratch_leaf` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7124-7138` `spawn_scoped_footprint` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7141-7143` `category` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7149-7170` `registered_scratch_roots_with` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7173-7200` `footprint_report_flags_registered_scratch_roots_dead_share_and_spares_a_live_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7213-7221` `footprint_report_keys_agent_scratch_liveness_by_run_id_and_leaf_not_leaf_name_alone` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7224-7229` `looks_like_run_container_true_when_every_direct_child_is_a_directory` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7232-7240` `looks_like_run_container_false_when_a_direct_child_is_a_bare_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7243-7251` `looks_like_run_container_is_true_on_an_empty_or_missing_dir` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7254-7271` `classify_agent_scratch_separates_a_bare_top_level_file_from_a_well_formed_container` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7274-7330` `footprint_report_reports_a_top_level_adhoc_agent_scratch_dir_as_its_own_category_never_folded_into_the_dead_run_bucket` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7333-7345` `footprint_advisories_flags_a_category_whose_dead_share_reaches_the_threshold` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7351-7388` `footprint_advisories_name_reset_build_cache_for_every_class_it_reclaims` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7392-7401` `assert_hinted_category_is_silent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7409-7427` `footprint_advisories_flags_a_category_exactly_at_the_threshold_boundary` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7430-7441` `footprint_advisories_never_flags_a_category_with_no_reclaim_command` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7448-7470` `owning_repo_root_prefers_the_stores_own_root_over_the_git_toplevel` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7475-7625` `reclaim_orphan_scratch_removes_non_live_owned_scratch_and_spares_live_and_shared_areas` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7628-7683` `reclaim_orphan_scratch_spares_a_non_live_worktree_that_is_still_dirty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7686-7725` `reclaim_orphan_scratch_spares_only_a_declared_dirty_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7728-7758` `reclaim_orphan_scratch_reaps_a_stray_build_cache_tombstone_unconditionally` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7763-7768` `find_store_dir_from_returns_the_dir_that_holds_the_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7771-7782` `find_store_dir_from_walks_up_from_a_subdirectory` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7785-7788` `plant_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7791-7803` `add_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7806-7841` `find_store_dir_from_never_escapes_the_repo_into_a_parent_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7844-7872` `reap_then_remove_dir_reaps_processes_rooted_inside_then_removes_the_dir` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7875-7886` `find_store_dir_from_refuses_the_worktree_shape_with_no_events_db` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7889-7917` `find_store_dir_from_walks_past_a_storeless_rigger_to_the_real_store_above` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7920-7949` `find_store_dir_from_resolves_the_owning_repo_even_when_the_worktree_lives_outside_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7952-7981` `find_store_dir_from_never_climbs_a_relocated_worktrees_own_unrelated_ancestors_into_a_foreign_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:7984-8009` `walk_stores_from_prefers_the_outermost_store_over_a_nearer_shadow` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8012-8029` `walk_stores_from_reports_no_shadow_for_a_single_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8038-8107` `require_store_dir_pins_to_the_fence_env_and_never_reaches_the_live_store_above_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8041-8044` `drop` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8111-8126` `require_store_dir_fence_is_off_by_default` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8131-8137` `result_advisories_flags_an_orphan_id_with_no_spawn_request` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8140-8165` `result_advisories_orphan_wording_is_plain_on_record_and_conditional_under_if_absent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8168-8178` `result_advisories_is_silent_for_a_parked_unanswered_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8181-8192` `result_advisories_flags_a_supersede_with_the_prior_result_position` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8195-8208` `result_advisories_suppresses_the_supersede_note_when_not_superseding` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8211-8223` `result_advisories_flags_both_orphan_and_supersede` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8231-8250` `result_refuses_a_second_result_without_supersede` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8255-8266` `a_real_result_still_replaces_a_recorded_liveness_fault` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8280-8297` `shipped_workflows_carry_a_non_zero_spawn_budget` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8300-8329` `usage_text_gives_the_jobs_flag_its_own_description_line` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8332-8349` `usage_text_names_the_build_cache_reset_mode` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8355-8387` `an_unclaimed_argument_is_an_unknown_flag_the_spec_or_a_refused_second_positional` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8390-8400` `parse_run_args_defaults_to_cli_and_an_unset_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8403-8414` `parse_run_args_reads_fresh_alongside_a_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8417-8427` `parse_run_args_reads_rebase_definition` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8430-8445` `parse_run_args_reads_driver_eventstore_conn_and_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8448-8456` `assert_every_arg_list_refused` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8475-8501` `parse_run_args_accepts_base_alongside_a_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8509-8531` `resolve_run_base_precedence_flag_then_env_then_default` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8537-8604` `definition_hash_is_stable_and_content_sensitive` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8613-8640` `kurrentdb_is_always_available_and_needs_a_conn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8652-8683` `store_selection_preserves_a_credentialed_tls_conn_verbatim` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8694-8871` `store_selection_precedence_flag_env_secret_file_config_then_default` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8874-8876` `project_identity_is_never_empty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8879-8908` `decide_migration_covers_every_case` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8911-8957` `migrate_project_identity_renames_legacy_history_and_records_a_decision` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:8960-8999` `migrate_project_identity_refuses_when_both_namespaces_hold_history` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9004-9027` `pre_mint_deployment` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9032-9039` `migrates_one_stream_folding_into` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9042-9082` `migrate_project_identity_rekeys_graph_rows_so_pre_mint_history_is_not_orphaned` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9085-9165` `migrate_project_identity_rekeys_the_graph_before_the_irreversible_stream_rename` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9168-9238` `migrate_project_identity_recovers_from_a_crash_between_the_rekey_and_the_rename` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9244-9276` `setup_provisions_the_shim_runtime_files` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9285-9344` `setup_installs_refreshes_and_is_a_noop_on_the_native_rigger_workflow` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9349-9358` `outcome_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9362-9367` `registry_entry` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9377-9447` `setup_installs_the_using_rigger_skill_distinct_from_the_workflow` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9465-9549` `planning_a_spec_installs_and_renders_through_the_registry_with_no_planning_specific_code` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9559-9595` `setup_skill_install_applies_the_project_overlay` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9601-9631` `docs_overlay_overrides_only_declared_fields` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9636-9650` `docs_overlay_malformed_is_a_loud_error` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9659-9673` `a_group_or_other_readable_secret_file_mode_is_flagged_owner_only_is_not` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9679-9698` `meta_object_body` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9707-9721` `meta_description` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9728-9736` `strip_line_comments` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9750-9974` `workflow_is_a_thin_courier_driver_with_per_unit_phase_labels` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:9983-9994` `the_step_schema_admits_the_attention_array` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10010-10066` `phase_of_maps_wave_items_to_the_meta_phase_by_role_and_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10079-10117` `meta_matches_reality_drops_integrate_and_the_unit_stage_construction` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10137-10230` `the_driver_relays_each_attention_entry_as_a_narrator_log_line` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10248-10296` `workflow_step_courier_prompt_is_foreground_and_honest` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10306-10315` `step_courier_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10333-10400` `workflow_step_courier_waits_on_an_auto_backgrounded_step` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10415-10423` `workflow_driver_guards_a_null_step_before_dereferencing_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10436-10493` `workflow_meta_description_is_a_user_facing_tagline_free_of_plumbing_terms` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10499-10524` `workflow_locates_the_provisioned_shim_or_tells_you_to_run_setup` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10535-10578` `format_stats_prints_all_four_metrics` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10584-10600` `format_stats_handles_zeroed_metrics_without_nan` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10603-10605` `canary_scorecard` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10608-10624` `assert_findings_raised_render` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10639-10644` `assert_empty_scorecard_omits` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10663-10694` `format_canary_stats_renders_na_for_a_tier_with_unattributed_correct_rejects` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10701-10720` `format_canary_stats_renders_the_real_zero_when_attribution_was_fully_measured` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10724-10739` `assert_control_line` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10764-10793` `format_canary_stats_reports_the_model_pinning_header_when_present` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10814-10856` `format_stats_surfaces_parallelism_retention_and_warns_below_the_floor` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10863-10903` `format_stats_surfaces_spawn_timing_and_reports_unpaired_separately` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10909-10915` `format_stats_spawn_timing_reports_no_spawns_when_none_recorded` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10922-10949` `format_stats_spawn_timing_no_spawns_line_requires_both_empty_and_zero_unpaired` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10956-10971` `format_stats_spawn_timing_all_unpaired_is_not_reported_as_no_spawns` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:10979-11006` `parallelism_retention_line_is_single_sourced_and_warns_below_the_floor` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11010-11025` `sdet_review` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11028-11030` `stats_text` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11037-11050` `stats_discloses_when_no_verdict_was_recorded_on_this_driver` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11061-11091` `stats_discloses_unfed_numerator_when_verdict_recorded_but_findings_unattributed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11099-11136` `stats_discloses_cause_split_remainder_when_fewer_causes_than_rejects` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11139-11184` `baseline_run_slice_selects_a_run_by_id_including_a_middle_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11187-11218` `format_stats_diff_flags_only_the_changed_rows` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11225-11232` `defaults_max_parallel_units_is_unbounded_when_the_key_is_absent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11235-11252` `parse_replay_args_requires_a_run_and_a_rev_in_either_order` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11260-11294` `a_second_concurrent_rigger_step_refuses_and_the_lock_frees_on_release` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11300-11303` `no_runs_message_points_at_rigger_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11309-11316` `seed_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11321-11334` `assert_absent_db_reads_none` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11352-11368` `stats_lines_existing_db_with_empty_run_stream_returns_none` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11376-11405` `stats_lines_does_not_read_another_projects_namespaced_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11412-11443` `stats_lines_existing_run_renders_metric_lines` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11466-11535` `stats_lines_pairs_recorded_spawn_request_and_result_into_spawn_timing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11553-11562` `db_with_one_result` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11568-11577` `result_of_at_unrecorded_spawn_reads_as_unreported` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11585-11609` `result_of_at_reads_a_self_reported_result_so_it_is_not_clobbered` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11615-11634` `result_of_at_is_namespace_scoped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11647-11672` `detach_process_group_places_the_child_in_its_own_process_group` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11681-11702` `a_child_spawned_without_detachment_inherits_the_parent_process_group` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11710-11719` `watch_test_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11729-11751` `status_progress_and_the_dash_snapshot_read_the_run_from_its_boundary` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11758-11783` `status_and_the_dash_read_the_runs_progress_from_its_own_stream` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11791-11841` `check_runs_parse_into_a_ci_probe_by_conclusion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11847-11856` `the_run_tip_ci_needs_a_run_branch_and_reads_unknown_when_gh_cannot_read_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11862-11874` `a_watch_poll_reads_the_run_from_its_boundary_and_nothing_else` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11897-11930` `store_location_repo_root_resolves_the_owning_root_not_the_process_cwd_so_a_real_marker_is_found` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11944-11975` `scratch_defaults_reads_the_owning_roots_config_with_no_agents_fleet_present` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:11978-12004` `watch_once_on_a_clean_store_reports_no_anomalies` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:12007-12011` `parse_watch_args_defaults_to_streaming_with_the_default_interval` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:12018-12040` `parse_watch_args_accepts_once_and_interval_together_in_either_order` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:12056-12193` `watch_once_on_the_seeded_store_reports_one_line_per_anomaly` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:12200-12219` `the_watchdog_command_signal_set_covers_every_signal_the_watch_skill_names` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:12227-12256` `watch_once_reports_dash_not_serving_when_the_marker_names_a_dead_holder` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:12263-12294` `watch_once_reports_no_anomaly_when_the_dash_marker_names_a_real_serving_holder` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/cli/mod.rs:12300-12321` `tree_root_is_the_top_level_of_the_directory_holding_the_store_else_that_directory` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).

### Unassigned (152 functions)

- `crates/rigger-conductor/src/conductor.rs:346-348` `failed_key` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:352-354` `spec_defect_lesson_key` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:358-360` `spec_defect_key` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:366-368` `spec_defect_escalated_key` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:436-438` `adoption_provenance_key` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:509-517` `input_digest` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:523-525` `digest_address` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:813-815` `implementer_retry` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:821-838` `conflict_resolution_prompt` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:1093-1095` `conflict_regenerate_key` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:1100-1102` `cached` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:1107-1112` `clear_attempt` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:1121-1137` `integrate_row` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:1199-1204` `evidence_pair` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:1458-1460` `carries_marker` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:1635-1639` `output_tail` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:1771-1773` `infra_mark_key` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:1778-1785` `infra_halt` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:1948-1960` `replay_trajectory` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:1964-2533` `run` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:2541-2622` `settle_run_state` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:2702-2793` `compute_attention` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:2813-2836` `spec_defect_stop` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:2854-2865` `spec_defect_halt` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:12231-12236` `part_coverage` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:12289-12298` `verified_evidence` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:12579-12591` `required_lines` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:12596-12598` `audit_id` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:12657-12667` `task_block` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:12737-12784` `render_capped_section` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:12831-12841` `recency_by_own_edge` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:12992-12995` `plain_node_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:13126-13142` `confidence_tier_radius` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:13152-13206` `files_reachable` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:13474-13476` `current_run_spec` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:13625-13642` `branch_owner` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:13681-13687` `quarantine_branch_name` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:13768-13774` `round_delta_base` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:13790-13813` `later_attempt_block` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:13891-13919` `quarantined_branch` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:13964-13969` `same_path` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:13976-13991` `sanitize_for_path` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:13996-13998` `integrates` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:14030-14036` `implement_slice` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:14120-14157` `validate_acyclic` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
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
- `crates/rigger-dash/src/dash.rs:1493-1607` `build_state` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:1998-2004` `parse_call_dir` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2028-2036` `call_edge_view` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2068-2154` `calls_view` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2228-2335` `build_run_tree` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2489-2508` `driver_stage` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2512-2520` `rollup` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2593-2600` `gates_status` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2611-2614` `advanced_past_gates` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2619-2628` `unit_live_status` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2633-2641` `spec_of` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2668-2691` `event_seed_ids` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2703-2726` `unit_seeds` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2741-2754` `repoint_seed` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2796-2798` `is_console_event` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2810-2819` `console_event_wire` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2826-2834` `console_progress_wire` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2838-2849` `event_view` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2875-2877` `now_unix` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2886-2905` `state_json` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2911-2919` `events_json` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2926-2981` `console_snapshot_json` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2985-2987` `live_page` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:2993-2995` `console_page` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:3002-3022` `render_export` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:3333-3351` `percent_decode` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:3354-3360` `query_param` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:3364-3369` `parse_request_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:3654-3661` `env_duration_ms` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:3676-3687` `write_sse` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-dash/src/dash.rs:3696-3702` `write_retained_window_gone` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:221-223` `version_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:283-287` `conn_flag` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:419-421` `stderr_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:425-429` `hash_blob_in` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:436-438` `open_sqlite_store` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:444-454` `open_graph` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:460-465` `standing_graph` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:471-479` `graph_rebuild_owed_note` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:484-492` `open_graph_to_read` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:499-501` `env_conn` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:586-589` `config_rigger_dir` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:592-594` `cwd` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:820-856` `refresh_registry_entry` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:868-870` `project_identity` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:895-906` `project_identity_at` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:913-915` `legacy_identity_at` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:920-933` `legacy_identity_from` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:953-959` `canonical_definition_text` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:974-1014` `definition_hash` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:1017-1022` `push_definition_section` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:1042-1056` `decide_migration` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:1193-1198` `db_path` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:1240-1297` `walk_stores_from` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:1310-1329` `main_repo_root` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:1587-1596` `owning_graph_to_read` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:1621-1653` `result_advisories` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:1657-1662` `latest_result_event` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:1670-1680` `ended_refusal` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:1686-1696` `result_refusal` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:1762-1774` `acquire_step_lock` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:1814-1826` `reap_then_remove_worktree` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:1841-1854` `result_of_at` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:1883-1895` `with_project_store` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:1919-1933` `refuse_when_base_unreachable` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:2053-2076` `stats_lines` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:2097-2111` `parallelism_retention_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:2174-2195` `append_spawn_timing` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:2200-2202` `fmt_duration` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:2204-2314` `append_review_quality` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:2631-2641` `started_units` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:2662-2695` `candidate_reaches_gate` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:2739-2753` `baseline_run_slice` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:2756-2760` `run_started_id` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:2767-2809` `materialize_config_at_rev` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:2909-2914` `marker_root` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:2978-2986` `watch_poll` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:2992-3015` `run_tip_ci` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:3022-3058` `ci_probe_from_check_runs` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:3063-3253` `watch_poll_over` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:3257-3266` `json_type_name` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:3382-3428` `current_run_units` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:3567-3630` `scan_residue` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:3709-3728` `dir_size_bytes` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:3750-3760` `build_cache_tombstone_path` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:3859-3872` `human_size` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:4136-4149` `dead_spawn_leaves` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:4159-4166` `looks_like_run_container` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:4446-4458` `owning_repo_root` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:4461-4463` `rigger_path` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:4696-4837` `precommit_block` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:4868-4920` `compose_precommit_bytes` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:5071-5086` `select_grounder` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/cli/mod.rs:5113-5119` `tree_root` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.

## 2. Duplication Catalog

210 clusters (1436 total sites) across `src/` and `tests/`, found by `tests/simplification_audit.rs`'s deterministic normalized-token-shingle Jaccard pass (8-token shingles, threshold 0.72) plus five mandatory mechanical sweeps. Strict definition (spec 85 Goal): any logic present in more than one place anywhere in the codebase is a violation, with no "small enough to duplicate" exemption.

### Mandatory sweeps

- **Command::new call sites**: 89 site(s) - `dup-decc12fbf756`
- **/proc-path string literals**: 55 site(s) - `dup-99a8136efbc4`
- **sqlite Connection::open call sites**: 68 site(s) - `dup-1f66a86debc4`
- **.rigger-path string literals**: 595 site(s) - `dup-e31752c98ad7`
- **error-shaping helper functions**: 13 site(s) - `dup-2b8f802a2dc3`

### Clusters (51 exact, 132 near, 27 semantic)

#### `dup-49d4d9f335fc` (near, 2 sites)

Proposed home: `blast_radius_eval::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/blast_radius_eval.rs:216-231` `width_threshold_is_the_tier_width_nearest_rank_percentile`
- `crates/rigger-conductor/src/blast_radius_eval.rs:234-242` `full_fraction_spans_all_light_to_collapse`

#### `dup-be7f6094aaff` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-conductor/src/canary_store.rs, crates/rigger-grounder/src/grounder/design/extract.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/canary_store.rs:820-822` `any_finding_is_critical`
- `crates/rigger-grounder/src/grounder/design/extract.rs:159-161` `is_handbook_path`

#### `dup-b6dc10b9c376` (near, 14 sites)

Proposed home: `a new shared module (sites span 6 files: crates/rigger-conductor/src/conductor.rs, crates/rigger-domain/src/spawn.rs, crates/rigger-driver/src/driver/claude_code.rs, tests/common/fixtures/graph.rs, tests/postmerge_gate_error_cleanup_periphery.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:338-340` `review_round_start_key`
- `crates/rigger-conductor/src/conductor.rs:346-348` `failed_key`
- `crates/rigger-conductor/src/conductor.rs:352-354` `spec_defect_lesson_key`
- `crates/rigger-conductor/src/conductor.rs:358-360` `spec_defect_key`
- `crates/rigger-conductor/src/conductor.rs:366-368` `spec_defect_escalated_key`
- `crates/rigger-conductor/src/conductor.rs:419-421` `compensation_queued_key`
- `crates/rigger-conductor/src/conductor.rs:1093-1095` `conflict_regenerate_key`
- `crates/rigger-conductor/src/conductor.rs:12596-12598` `audit_id`
- `crates/rigger-domain/src/spawn.rs:140-142` `spawn_id`
- `crates/rigger-driver/src/driver/claude_code.rs:1120-1128` `stop_message`
- `tests/common/fixtures/graph.rs:202-204` `spoke_id`
- `tests/postmerge_gate_error_cleanup_periphery.rs:56-58` `expected_postmerge_dir`
- `tests/postmerge_gate_error_cleanup_periphery.rs:59-61` `expected_postmerge_branch`
- `tests/simplification_audit.rs:5654-5656` `sample_key`

#### `dup-340eefdbe122` (near, 10 sites)

Proposed home: `a new shared module (sites span 9 files: crates/rigger-conductor/src/conductor.rs, crates/rigger-domain/src/review.rs, crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs, crates/rigger-store-sqlite/src/eventstore/kurrentdb.rs, crates/rigger-store-sqlite/src/spawn_store.rs, tests/common/fixtures/plan_critique.rs, tests/compaction_generations_periphery.rs, tests/new_run_critique_refusal_periphery.rs, tests/no_os_kill_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:436-438` `adoption_provenance_key`
- `crates/rigger-conductor/src/conductor.rs:449-451` `quarantine_record_key`
- `crates/rigger-domain/src/review.rs:339-344` `spec_critique_prompt`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:3147-3149` `code_entity_id`
- `crates/rigger-store-sqlite/src/eventstore/kurrentdb.rs:329-331` `group_stream`
- `crates/rigger-store-sqlite/src/spawn_store.rs:73-75` `what`
- `tests/common/fixtures/plan_critique.rs:29-34` `spec_defect_halt_text`
- `tests/compaction_generations_periphery.rs:5152-5157` `closed_unit_line`
- `tests/new_run_critique_refusal_periphery.rs:138-140` `no_critic_line`
- `tests/no_os_kill_audit.rs:52-54` `join`

#### `dup-a9c4f3d90dac` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-conductor/src/conductor.rs, crates/rigger-domain/src/spawn.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:480-482` `unit_of_gate_key`
- `crates/rigger-conductor/src/conductor.rs:523-525` `digest_address`
- `crates/rigger-domain/src/spawn.rs:241-243` `unit_of`

#### `dup-acf2bec10ae7` (near, 24 sites)

Proposed home: `a new shared module (sites span 16 files: crates/rigger-conductor/src/conductor.rs, crates/rigger-domain/src/contextgraph.rs, crates/rigger-domain/src/review.rs, crates/rigger-domain/src/spawn.rs, crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs, crates/rigger-grounder/src/grounder/mod.rs, crates/rigger-grounder/src/grounder/workflowdef.rs, crates/rigger-grounder/src/ingest.rs, crates/rigger-store-sqlite/src/eventstore/namespace.rs, crates/rigger-worktree-git/src/worktree.rs, src/cli/mod.rs, tests/canary_model_drift_periphery.rs, tests/halted_spawn_wip_recovery_periphery.rs, tests/native_driver_pipelining_behavior.rs, tests/regate_landed_on_resume_periphery.rs, tests/reset_derived_compaction_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:666-668` `deferred_gate_verdict_key`
- `crates/rigger-conductor/src/conductor.rs:677-679` `deferred_gate_failed_key`
- `crates/rigger-conductor/src/conductor.rs:12479-12486` `review_protocol`
- `crates/rigger-domain/src/contextgraph.rs:615-617` `not_folded`
- `crates/rigger-domain/src/contextgraph.rs:671-673` `rebuild_owed_refusal`
- `crates/rigger-domain/src/review.rs:387-389` `critique_unit`
- `crates/rigger-domain/src/spawn.rs:115-117` `lens_role`
- `crates/rigger-domain/src/spawn.rs:207-209` `speculation_group_id`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:1293-1295` `pruned_copy`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:1310-1312` `shadow_of`
- `crates/rigger-grounder/src/grounder/mod.rs:207-213` `retired_grounder_error`
- `crates/rigger-grounder/src/grounder/workflowdef.rs:26-28` `stage_id`
- `crates/rigger-grounder/src/grounder/workflowdef.rs:30-32` `gate_id`
- `crates/rigger-grounder/src/grounder/workflowdef.rs:34-36` `agent_id`
- `crates/rigger-grounder/src/ingest.rs:596-598` `code_identity`
- `crates/rigger-store-sqlite/src/eventstore/namespace.rs:64-66` `prefix_for`
- `crates/rigger-worktree-git/src/worktree.rs:1745-1747` `shared_build_cache_guard_path`
- `src/cli/mod.rs:4545-4547` `skill_source_rel`
- `src/cli/mod.rs:5051-5057` `spec_lint_next_step`
- `tests/canary_model_drift_periphery.rs:112-114` `prose_claiming`
- `tests/halted_spawn_wip_recovery_periphery.rs:129-131` `unit_branch`
- `tests/native_driver_pipelining_behavior.rs:597-599` `worker_prompts_harness`
- `tests/regate_landed_on_resume_periphery.rs:76-78` `unit_branch`
- `tests/reset_derived_compaction_periphery.rs:2496-2498` `derived_key_for`

#### `dup-cb25e49f6e22` (semantic, 2 sites)

Proposed home: `one shared `append_and_fold` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-conductor/src/conductor.rs:3192-3207` `append_and_fold`
- `crates/rigger-grounder/src/ingest.rs:140-153` `append_and_fold`

#### `dup-4e1d0e78e3c6` (semantic, 3 sites)

Proposed home: `one shared `read_current_run` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-conductor/src/conductor.rs:3587-3589` `read_current_run`
- `crates/rigger-dash/src/mcpserver.rs:534-537` `read_current_run`
- `crates/rigger-domain/src/run/read.rs:75-82` `read_current_run`

#### `dup-f13596146362` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-conductor/src/conductor.rs, crates/rigger-domain/src/contextgraph/query.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:3844-3846` `spawn_is_recorded`
- `crates/rigger-domain/src/contextgraph/query.rs:456-458` `is_shared`

#### `dup-2b8f802a2dc3` (semantic, 13 sites)

Proposed home: `one error-shaping helper module`

mandatory sweep: error-shaping helper functions - 13 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-conductor/src/conductor.rs:5127-5145` `guard_review_round_tree_on_tier_err`
- `crates/rigger-conductor/src/conductor.rs:29776-29836` `integrate_plan_commits_wraps_any_hard_error_with_the_plan_landing_marker`
- `crates/rigger-domain/src/agent.rs:363-365` `no_result_error`
- `crates/rigger-domain/src/ingest.rs:604-649` `a_walk_reaches_every_batch_past_a_failed_one_and_answers_the_first_error`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:4579-4632` `a_storage_error_in_a_rebuild_propagates_and_the_next_rebuild_resumes_and_folds_it`
- `crates/rigger-grounder/src/grounder/mod.rs:207-213` `retired_grounder_error`
- `crates/rigger-worktree-git/src/worktree.rs:3689-3716` `land_reports_a_generic_error_for_a_refusal_that_is_neither_tip_moved_nor_blocked`
- `crates/rigger-worktree-git/src/worktree.rs:5028-5082` `revert_on_base_aborts_and_errors_on_a_conflicting_revert`
- `tests/adoption_keys_on_criterion_periphery.rs:2008-2148` `a_quarantine_record_whose_ref_was_since_deleted_hard_errors_instead_of_silently_starting_fresh`
- `tests/batched_fold_cadence.rs:176-272` `append_and_fold_is_best_effort_on_fold_error_and_a_no_op_on_an_empty_batch`
- `tests/canary_item_sharding_jobs_cap_periphery.rs:277-367` `run_canary_runs_every_item_to_completion_even_when_one_items_spawn_errors`
- `tests/cli.rs:7544-7583` `run_workflow_prints_the_budget_halt_reason_on_stderr_when_its_conductor_returns`
- `tests/grounder_name_contract.rs:40-171` `public_name_contract_predicate_error_and_resolver_agree`

#### `dup-edcfd444cd88` (near, 3 sites)

Proposed home: `conductor::run_ctx`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:10568-10582` `record_merge_attempt`
- `crates/rigger-conductor/src/conductor.rs:10614-10629` `record_landing_intent`
- `crates/rigger-conductor/src/conductor.rs:10639-10654` `record_landed`

#### `dup-57930b804935` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: crates/rigger-conductor/src/conductor.rs, tests/cli.rs, tests/gate_scratch_root_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:12390-12395` `unit_size_cap`
- `tests/cli.rs:11715-11720` `sweep_tool_refusal`
- `tests/gate_scratch_root_periphery.rs:56-62` `recording_gate`

#### `dup-e2420edc22e9` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-conductor/src/conductor.rs, crates/rigger-store-sqlite/src/eventstore/sqlite.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:13423-13425` `unit_branch`
- `crates/rigger-store-sqlite/src/eventstore/sqlite.rs:1105-1107` `key_expr`

#### `dup-c6dfc41a628c` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-conductor/src/conductor.rs, tests/plan_stage_commit_landing_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:15886-15891` `answering`
- `tests/plan_stage_commit_landing_periphery.rs:282-287` `new`

#### `dup-c36d696064fd` (near, 4 sites)

Proposed home: `conductor::stub`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:15939-15941` `prompts_for`
- `crates/rigger-conductor/src/conductor.rs:15949-15951` `dirs_for`
- `crates/rigger-conductor/src/conductor.rs:15955-15957` `system_prompt_for`
- `crates/rigger-conductor/src/conductor.rs:15961-15963` `title_for`

#### `dup-ebee743f02de` (near, 14 sites)

Proposed home: `a new shared module (sites span 4 files: crates/rigger-conductor/src/conductor.rs, crates/rigger-domain/src/eventstore.rs, tests/common/fixtures/events.rs, tests/common/real_driver_spy.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:15973-15975` `spawn_ids`
- `crates/rigger-conductor/src/conductor.rs:38734-38736` `calls`
- `crates/rigger-conductor/src/conductor.rs:38737-38739` `targets`
- `crates/rigger-conductor/src/conductor.rs:38740-38742` `gate_scratches`
- `crates/rigger-conductor/src/conductor.rs:38743-38745` `store_fences`
- `crates/rigger-conductor/src/conductor.rs:38746-38748` `build_cache_guards`
- `crates/rigger-conductor/src/conductor.rs:38749-38751` `build_cache_dirs`
- `crates/rigger-conductor/src/conductor.rs:38873-38875` `calls`
- `crates/rigger-domain/src/eventstore.rs:266-268` `last`
- `crates/rigger-domain/src/eventstore.rs:587-589` `recv`
- `crates/rigger-domain/src/eventstore.rs:597-599` `try_recv`
- `crates/rigger-domain/src/eventstore.rs:602-604` `err`
- `tests/common/fixtures/events.rs:608-610` `reads`
- `tests/common/real_driver_spy.rs:36-38` `outputs`

#### `dup-60da41568392` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-conductor/src/conductor.rs, crates/rigger-domain/src/eventstore.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:15979-15985` `spawned`
- `crates/rigger-domain/src/eventstore.rs:495-497` `covers`

#### `dup-decc12fbf756` (semantic, 89 sites)

Proposed home: `a single injected process-spawn port every Command::new site routes through instead of constructing its own Command`

mandatory sweep: Command::new call sites - 89 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-conductor/src/conductor.rs:16118-16118` `Command::new`
- `crates/rigger-conductor/src/conductor.rs:16123-16123` `Command::new`
- `crates/rigger-conductor/src/conductor.rs:28361-28361` `Command::new`
- `crates/rigger-conductor/src/conductor.rs:41562-41562` `Command::new`
- `crates/rigger-conductor/src/conductor.rs:41576-41576` `Command::new`
- `crates/rigger-conductor/src/conductor.rs:42652-42652` `Command::new`
- `crates/rigger-driver/src/reaped_child.rs:90-90` `Command::new`
- `crates/rigger-gates-shell/src/gate.rs:1531-1531` `Command::new`
- `crates/rigger-grounder/src/grounder/mod.rs:964-964` `Command::new`
- `crates/rigger-process/src/budget.rs:198-198` `Command::new`
- `crates/rigger-process/src/budget.rs:236-236` `Command::new`
- `crates/rigger-process/src/budget.rs:247-247` `Command::new`
- `crates/rigger-process/src/subprocess.rs:18-18` `Command::new`
- `crates/rigger-worktree-git/src/worktree.rs:3989-3989` `Command::new`
- `src/cli/mod.rs:5398-5398` `Command::new`
- `src/cli/mod.rs:7793-7793` `Command::new`
- `src/cli/mod.rs:9963-9963` `Command::new`
- `src/cli/mod.rs:11650-11650` `Command::new`
- `src/cli/mod.rs:11684-11684` `Command::new`
- `src/cli/run.rs:3254-3254` `Command::new`
- `src/cli/run.rs:3287-3287` `Command::new`
- `src/cli/run.rs:3331-3331` `Command::new`
- `src/cli/run.rs:3400-3400` `Command::new`
- `src/cli/validate.rs:1463-1463` `Command::new`
- `src/cli/validate.rs:1742-1742` `Command::new`
- `tests/adaptive_labels_periphery.rs:87-87` `Command::new`
- `tests/adoption_keys_on_criterion_periphery.rs:2067-2067` `Command::new`
- `tests/checkin_mutation_diff_base_periphery.rs:76-76` `Command::new`
- `tests/checkin_mutation_diff_base_periphery.rs:182-182` `Command::new`
- `tests/ci_lanes.rs:383-383` `Command::new`
- `tests/ci_lanes.rs:406-406` `Command::new`
- `tests/claude_code_stream_periphery.rs:1041-1041` `Command::new`
- `tests/cli.rs:1461-1461` `Command::new`
- `tests/cli.rs:5231-5231` `Command::new`
- `tests/cli.rs:12761-12761` `Command::new`
- `tests/cli.rs:13502-13502` `Command::new`
- `tests/cli.rs:18465-18465` `Command::new`
- `tests/cli.rs:23541-23541` `Command::new`
- `tests/cli.rs:26609-26609` `Command::new`
- `tests/common/cli.rs:18-18` `Command::new`
- `tests/common/cli.rs:71-71` `Command::new`
- `tests/common/cli.rs:270-270` `Command::new`
- `tests/common/fixtures/conductor.rs:288-288` `Command::new`
- `tests/common/fixtures/conductor.rs:372-372` `Command::new`
- `tests/common/fixtures/git.rs:25-25` `Command::new`
- `tests/common/fixtures/git.rs:45-45` `Command::new`
- `tests/common/fixtures/host.rs:10-10` `Command::new`
- `tests/common/fixtures/host.rs:80-80` `Command::new`
- `tests/common/fixtures/host.rs:91-91` `Command::new`
- `tests/common/fixtures/host.rs:178-178` `Command::new`
- `tests/common/layer_cli.rs:71-71` `Command::new`
- `tests/common/mod.rs:159-159` `Command::new`
- `tests/common/repo.rs:356-356` `Command::new`
- `tests/common/served.rs:295-295` `Command::new`
- `tests/compiler_pass_stage1_audit.rs:193-193` `Command::new`
- `tests/compiler_pass_stage1_audit.rs:211-211` `Command::new`
- `tests/core_lane_purity_audit.rs:315-315` `Command::new`
- `tests/dash_release_ready.rs:348-348` `Command::new`
- `tests/gitsemver_derivation.rs:86-86` `Command::new`
- `tests/gitsemver_worktree_periphery.rs:111-111` `Command::new`
- `tests/halted_spawn_wip_recovery_periphery.rs:827-827` `Command::new`
- `tests/hermetic_test_git_audit.rs:203-203` `Command::new`
- `tests/hermetic_test_git_audit.rs:235-235` `Command::new`
- `tests/hermetic_test_git_audit.rs:307-307` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:331-331` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:1557-1557` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:1648-1648` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:1893-1893` `Command::new`
- `tests/meta_phases_declaration_periphery.rs:83-83` `Command::new`
- `tests/mutation_runner_pdeathsig_periphery.rs:107-107` `Command::new`
- `tests/mutation_runner_pdeathsig_periphery.rs:167-167` `Command::new`
- `tests/native_driver_pipelining_behavior.rs:293-293` `Command::new`
- `tests/no_os_kill_test_helper_periphery.rs:29-29` `Command::new`
- `tests/no_os_kill_test_helper_periphery.rs:47-47` `Command::new`
- `tests/phase_of_role_mapping_periphery.rs:59-59` `Command::new`
- `tests/principle_gates_wiring.rs:114-114` `Command::new`
- `tests/principle_gates_wiring.rs:427-427` `Command::new`
- `tests/product_binary_authority_periphery.rs:155-155` `Command::new`
- `tests/reset_build_cache_periphery.rs:242-242` `Command::new`
- `tests/reset_build_cache_periphery.rs:254-254` `Command::new`
- `tests/reset_build_cache_periphery.rs:331-331` `Command::new`
- `tests/reset_build_cache_periphery.rs:349-349` `Command::new`
- `tests/revert_on_base_hook_bypass_periphery.rs:165-165` `Command::new`
- `tests/scaffold_grounder_resolves.rs:96-96` `Command::new`
- `tests/step_attention_periphery.rs:537-537` `Command::new`
- `tests/store_flag_precedence.rs:78-78` `Command::new`
- `tests/store_resolution.rs:153-153` `Command::new`
- `tests/turbovec_retired_cargo_boundary.rs:50-50` `Command::new`
- `tests/validate_behind_the_tree_periphery.rs:128-128` `Command::new`

#### `dup-e31752c98ad7` (semantic, 595 sites)

Proposed home: `one .rigger-relative path-composition helper`

mandatory sweep: .rigger-path string literals - 595 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-conductor/src/conductor.rs:18887-18887` `"the repo's own .rigger config must load"`
- `crates/rigger-config-files/src/config_store.rs:377-377` `".rigger"`
- `crates/rigger-config-files/src/config_store.rs:378-378` `"create .rigger/instructions"`
- `crates/rigger-config-files/src/config_store.rs:431-431` `".rigger"`
- `crates/rigger-config-files/src/config_store.rs:432-432` `".rigger"`
- `crates/rigger-config-files/src/config_store.rs:456-456` `"../../examples/demo/.rigger"`
- `crates/rigger-config-files/src/config_store.rs:457-457` `".rigger"`
- `crates/rigger-config-files/src/config_store.rs:1535-1535` `".rigger/agents/sdet-author.md"`
- `crates/rigger-config-files/src/config_store.rs:1536-1536` `"the shipped .rigger/agents/sdet-author.md must exist"`
- `crates/rigger-config-files/src/config_store.rs:1563-1563` `".rigger/agents/sdet.md"`
- `crates/rigger-config-files/src/config_store.rs:1564-1564` `"the shipped .rigger/agents/sdet.md must exist"`
- `crates/rigger-config-files/src/config_store.rs:2015-2015` `".rigger"`
- `crates/rigger-config-files/src/config_store.rs:2016-2016` `"create .rigger dir"`
- `crates/rigger-config-files/src/config_store.rs:2048-2048` `".rigger"`
- `crates/rigger-config-files/src/config_store.rs:2049-2049` `"create .rigger dir"`
- `crates/rigger-config-files/src/config_store.rs:2070-2070` `".rigger"`
- `crates/rigger-config-files/src/config_store.rs:2071-2071` `"create .rigger dir"`
- `crates/rigger-dash/src/dash.rs:3930-3930` `"{root}/.rigger/events.db"`
- `crates/rigger-dash/src/dash.rs:3981-3981` `"/.rigger/events.db"`
- `crates/rigger-domain/src/config.rs:20-20` `".rigger"`
- `crates/rigger-domain/src/docs.rs:221-221` `"The EVENT LOG accumulates separately from the graph, and has its own prune: `rigger \
         reset --derived`. Each run's project-ingest pass records the project's derived index - \
         the code entities, inferred edges, design links, and doc concepts folded from your \
         sources - and a log written before that pass deduplicated across runs holds the WHOLE \
         index once per run, which is re-derivable duplication rather than history. `rigger reset \
         --derived` keeps, for each file, only the recordings of its LATEST generation - the \
         content the log last recorded for it - and of those the LATEST event per replay key, \
         deletes every superseded generation and re-recording, and compacts the file so \
         events.db shrinks on disk. Every other event survives byte-for-byte - lessons, \
         decisions, findings, gate verdicts, and the whole run history `rigger stats` and replay \
         read. The live graph a rebuild folds is unchanged: a newer generation of a file \
         retires every fact the one before it asserted and it does not, so the whole log \
         already folds to each file's latest generation; nothing reads a shed recording again \
         (a file that returns to an earlier content re-emits its batch); and the prune carries \
         a design fact's EARLIEST valid-time within its unbroken run of generations onto the \
         recording it keeps, so a design fact keeps the date it first became true rather than \
         being re-dated to whichever recording survived. WHAT IT CANNOT RECLAIM, because \
         this decides whether it is worth running at all: it never sheds the index itself. The latest generation of every file stays, so on \
         a log that holds each file once, at one recording per key, `rigger reset --derived` \
         deletes ZERO rows from it and reports so - that is the expected report on a clean log, \
         not a failure, and the derived index remains the bulk of the log by design because it \
         is what the graph is folded from. WHEN A DEDUPLICATED LOG STILL HAS SOMETHING TO SHED, \
         because a non-zero prune is otherwise read as a broken dedup: every edit to a file \
         records a new generation of its batch and leaves the one before it superseded, and a \
         file whose content has RETURNED to a generation the log had already recorded - a \
         revert, a branch switch, a checkout back - re-records that file's whole batch by \
         design, since a dedup that suppressed an already-recorded key would strand the graph \
         on the version the file has since moved past. A prune that sheds rows on such a log is \
         shedding exactly that, not covering for a defect; a log written BEFORE the dedup sheds \
         the whole accumulated pile instead. WHAT IT COSTS TO RUN: the compaction rewrites events.db in full and stages \
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
- `crates/rigger-domain/src/docs.rs:853-853` `"Store hygiene for rigger's own state - growing .rigger/ disk usage, \
         the bloat advisory from `rigger validate`, or `rigger step`/replay running slow. \
         Read this before running `rigger reset` or touching any store file by hand."`
- `crates/rigger-domain/src/docs.rs:857-857` `"rigger keeps three stores under `.rigger/`, and only one of them holds anything \
             durable:\n"`
- `crates/rigger-domain/src/docs.rs:927-927` `"`rigger graph build` folds the project's source straight into `.rigger/graph.db` - \
             no run, no `RunStarted`, nothing but the code-ingest events the fold already emits. \
             It CREATES the store when the checkout is cold (`.rigger/` does not exist yet) and \
             REFRESHES an existing store incrementally: an unchanged file re-ingests nothing, and \
             it reuses the exact same walk-and-content-key ingest authority a live run uses, so a \
             standalone build and a run can never fold the same file under two different keys.\n"`
- `crates/rigger-domain/src/docs.rs:937-937` `"Never force a rebuild by deleting `.rigger/graph.db` (or `events.db`) and \
         re-running `rigger graph build` on the empty result. Deleting the log throws away \
         truth that no rebuild can get back, and deleting only the graph is unnecessary work \
         `rigger graph build` already does FOR you, incrementally, without erasing anything \
         first. If lookups are empty, just run `rigger graph build`; only reach for \
         rigger-reset-store if you specifically mean to prune, not rebuild.\n"`
- `crates/rigger-domain/src/docs.rs:961-961` `"`rigger reindex <file>...` re-parses ONLY the named files and persists the delta to \
             the project's symbols grounding index at `.rigger/symbols/` - the fast, targeted fix \
             for an index that has drifted from files you just changed (a unit's own commit, a \
             rebase, a branch switch). It is scoped strictly to the symbols index, a DIFFERENT \
             store from the structural context graph, so it costs only the named files, never a \
             walk of the whole tree.\n"`
- `crates/rigger-domain/src/instructions.rs:70-70` `"\nOperator (.rigger/instructions/*.md, filename order):\n"`
- `crates/rigger-gates-shell/src/gate.rs:336-336` `".rigger-cache-probe-{}"`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:7311-7311` `".rigger/workflow.yml"`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:7319-7319` `".rigger/workflow.yml"`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:7327-7327` `".rigger/workflow.yml"`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:7335-7335` `".rigger/workflow.yml"`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:7343-7343` `".rigger/workflow.yml"`
- `crates/rigger-grounder/src/grounder/mod.rs:407-407` `".rigger"`
- `crates/rigger-grounder/src/grounder/mod.rs:924-924` `".rigger/workflow.yml"`
- `crates/rigger-grounder/src/grounder/workflowdef.rs:527-527` `".rigger"`
- `crates/rigger-grounder/src/grounder/workflowdef.rs:615-615` `"this project's own .rigger/workflow.yml must extract at least one event"`
- `crates/rigger-grounder/src/ingest.rs:1052-1052` `".rigger"`
- `crates/rigger-grounder/src/ingest.rs:1054-1054` `".rigger"`
- `crates/rigger-grounder/src/ingest.rs:1063-1063` `"gw/.rigger/workflow.yml@"`
- `crates/rigger-grounder/src/ingest.rs:1081-1081` `"one code batch (a.rs) plus one workflow-definition batch (.rigger/workflow.yml) \
             must both advance the shared batch count; got {}"`
- `crates/rigger-grounder/src/ingest.rs:1110-1110` `".rigger"`
- `crates/rigger-grounder/src/ingest.rs:1112-1112` `".rigger"`
- `crates/rigger-grounder/src/ingest.rs:1146-1146` `".rigger"`
- `crates/rigger-grounder/src/ingest.rs:2494-2494` `".rigger"`
- `crates/rigger-grounder/src/ingest.rs:2498-2498` `".rigger/persona.md"`
- `crates/rigger-grounder/src/ingest.rs:2507-2507` `".rigger/persona.md"`
- `crates/rigger-grounder/src/ingest.rs:2520-2520` `"gd/.rigger/"`
- `crates/rigger-grounder/src/ingest.rs:2521-2521` `"premise: the whole walk lowers both docs and nothing under .rigger; got {walked:?}"`
- `crates/rigger-process/src/reap.rs:749-749` `".rigger"`
- `crates/rigger-process/src/reap.rs:1090-1090` `"a relocated/cache-home-style authorized_root with no .rigger/tmp relationship \
             must still authorize the reap"`
- `crates/rigger-store-sqlite/src/registry.rs:420-420` `"/home/dev/proj/.rigger/events.db"`
- `crates/rigger-store-sqlite/src/registry.rs:429-429` `"/home/dev/proj/.rigger/events.db"`
- `crates/rigger-store-sqlite/src/registry.rs:446-446` `"/home/dev/proj/.rigger/events.db"`
- `crates/rigger-store-sqlite/src/registry.rs:453-453` `"/home/dev/proj/.rigger/events.db"`
- `crates/rigger-store-sqlite/src/registry.rs:467-467` `"/home/dev/proj-b/.rigger/events.db"`
- `crates/rigger-store-sqlite/src/registry.rs:485-485` `"/a/.rigger/events.db"`
- `crates/rigger-store-sqlite/src/registry.rs:486-486` `"/b/.rigger/events.db"`
- `crates/rigger-store-sqlite/src/registry.rs:512-512` `"/home/dev/proj/.rigger/events.db"`
- `crates/rigger-store-sqlite/src/registry.rs:530-530` `"/home/dev/proj/.rigger/events.db"`
- `crates/rigger-store-sqlite/src/registry.rs:645-645` `"/home/dev/proj/.rigger/events.db"`
- `crates/rigger-store-sqlite/src/registry.rs:668-668` `r#"{"project":"proj","root":"/home/dev/proj","store":{"kind":"local","path":"/home/dev/proj/.rigger/events.db"},"heartbeat_ms":1000}"#`
- `crates/rigger-store-sqlite/src/registry.rs:689-689` `"/home/dev/proj/.rigger/events.db"`
- `crates/rigger-store-sqlite/src/registry.rs:749-749` `"/home/dev/proj/.rigger/events.db"`
- `crates/rigger-worktree-git/src/worktree.rs:5356-5356` `"{repo_path}/.rigger/tmp"`
- `crates/rigger-worktree-git/src/worktree.rs:5357-5357` `"the default must no longer nest inside the repo's own .rigger: {dflt:?}"`
- `crates/rigger-worktree-git/src/worktree.rs:5360-5360` `"/.rigger/"`
- `crates/rigger-worktree-git/src/worktree.rs:5360-5360` `"/.rigger"`
- `crates/rigger-worktree-git/src/worktree.rs:5361-5361` `"the default must never live under any .rigger: {dflt:?}"`
- `crates/rigger-worktree-git/src/worktree.rs:5380-5380` `"{repo_path}/.rigger/tmp"`
- `crates/rigger-worktree-git/src/worktree.rs:5404-5404` `"~/.rigger-scratch-test"`
- `crates/rigger-worktree-git/src/worktree.rs:5405-5405` `"{home}/.rigger-scratch-test"`
- `crates/rigger-worktree-git/src/worktree.rs:7077-7077` `"{base}..rigger-run"`
- `crates/rigger-worktree-git/src/worktree.rs:7107-7107` `".rigger"`
- `src/cli/critique.rs:168-168` `"{COMMAND}: the critic {:?} the workflow names has no definition under \
                 .rigger/agents"`
- `src/cli/hygiene.rs:628-628` `"a `rigger step` is running right now (it holds .rigger/step.lock)"`
- `src/cli/hygiene.rs:1636-1636` `"a `rigger step` is running right now (it holds .rigger/step.lock)"`
- `src/cli/hygiene.rs:1765-1765` `"/home/dev/proj/.rigger/events.db"`
- `src/cli/mod.rs:638-638` `"the server event store is selected but no connection string is set - provide one via \
         --conn <url>, the KURRENTDB_CONN environment variable, or the .rigger/store.conn \
         secret file"`
- `src/cli/mod.rs:1115-1115` `"migrated project history to the durable identity: renamed {n} stream(s) \
                     from the legacy namespace {legacy:?} to the minted identity {minted:?} \
                     (.rigger/{PROJECT_ID_FILE})"`
- `src/cli/mod.rs:1183-1183` `"rigger: migrated project identity - renamed {n} stream(s) from the legacy \
             namespace {legacy:?} to the minted identity {minted:?} (.rigger/{PROJECT_ID_FILE}); \
             recorded its decision (position {}){}"`
- `src/cli/mod.rs:2027-2027` `"workflow: the per-project JS driver is not provisioned (looked for {}). \
         Run `rigger setup` to write the shim into .rigger/shim/ and install its \
         dependencies, then re-run `rigger workflow`."`
- `src/cli/mod.rs:3290-3290` `".rigger-workflow-provenance"`
- `src/cli/mod.rs:4699-4699` `r#"__BEGIN__
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
- `src/cli/mod.rs:5526-5526` `"/.rigger/tmp/cargo-target/debug/rigger"`
- `src/cli/mod.rs:5527-5527` `"/.rigger/tmp/cargo-target/release/rigger"`
- `src/cli/mod.rs:6644-6644` `".rigger"`
- `src/cli/mod.rs:6648-6648` `".rigger"`
- `src/cli/mod.rs:6683-6683` `"probe/.rigger/events.db"`
- `src/cli/mod.rs:6684-6684` `"rigger-wt-x/.rigger/events.db"`
- `src/cli/mod.rs:6846-6846` `".rigger"`
- `src/cli/mod.rs:6882-6882` `"rigger-wt-unit-99-ghost-12345678/.rigger/events.db"`
- `src/cli/mod.rs:6917-6917` `"probe/.rigger/events.db"`
- `src/cli/mod.rs:6928-6928` `"shadow store: probe/.rigger/events.db (6B)"`
- `src/cli/mod.rs:7041-7041` `".rigger"`
- `src/cli/mod.rs:7131-7131` `".rigger"`
- `src/cli/mod.rs:7366-7366` `".rigger"`
- `src/cli/mod.rs:7819-7819` `".rigger"`
- `src/cli/mod.rs:7858-7858` `".rigger"`
- `src/cli/mod.rs:7904-7904` `".rigger"`
- `src/cli/mod.rs:7915-7915` `"must walk past the storeless worktree `.rigger/` to the repo's real store"`
- `src/cli/mod.rs:8539-8539` `".rigger"`
- `src/cli/mod.rs:8541-8541` `".rigger"`
- `src/cli/mod.rs:8584-8584` `".rigger"`
- `src/cli/mod.rs:8619-8619` `".rigger"`
- `src/cli/mod.rs:8655-8655` `".rigger"`
- `src/cli/mod.rs:8697-8697` `".rigger"`
- `src/cli/mod.rs:8803-8803` `".rigger/store.conn beats the committed config"`
- `src/cli/mod.rs:9251-9251` `"{name} must be written into .rigger/shim/"`
- `src/cli/mod.rs:10518-10518` `"locate_shim must return the provisioned .rigger/shim/shim.mjs"`
- `src/cli/setup.rs:267-267` `".rigger/shim/"`
- `src/cli/setup.rs:268-268` `".rigger/dash.url"`
- `src/cli/setup.rs:269-269` `".rigger/dash.marker"`
- `src/cli/setup.rs:270-270` `".rigger/dash.attempt"`
- `src/cli/setup.rs:271-271` `".rigger/store.conn"`
- `src/cli/setup.rs:456-456` `"minted the durable project identity in .rigger/{PROJECT_ID_FILE}: {id} \
             (commit it so a rename never orphans this project's history)"`
- `src/cli/setup.rs:463-463` `"scaffolded .rigger/workflow.yml (gate template set: {key})"`
- `src/cli/setup.rs:466-466` `"scaffolded .rigger/workflow.yml"`
- `src/cli/setup.rs:474-474` `"no gate template set matches this project ({catalogue} at the project \
                     root), so .rigger/workflow.yml declares no gates - declare your own under \
                     gates:"`
- `src/cli/setup.rs:482-482` `"scaffolded .rigger/instructions/README.md"`
- `src/cli/setup.rs:489-489` `"scaffolded .rigger/agents/{{{}}}"`
- `src/cli/setup.rs:734-734` `"imported {} agent {} from {} into .rigger/agents/ ({} kept - already present)"`
- `src/cli/setup.rs:785-785` `"provisioned the JS driver in .rigger/shim/ (wrote shim.mjs + package.json + \
             package-lock.json and ran npm install)"`
- `src/cli/setup.rs:1316-1316` `"kept existing .rigger/agents/{name} (import never overwrites)"`
- `src/cli/setup.rs:1344-1344` `"imported .rigger/agents/{name} (id: {id})"`
- `src/cli/setup.rs:2107-2107` `"nothing is written - no .rigger, no .claude, no .gitignore"`
- `src/cli/setup.rs:2174-2174` `"scaffolded .rigger/workflow.yml"`
- `src/cli/setup.rs:2179-2179` `"no gate template set matches this project (rust: Cargo.toml at the project \
                  root), so .rigger/workflow.yml declares no gates - declare your own under gates:"`
- `src/cli/setup.rs:2205-2205` `"scaffolded .rigger/workflow.yml (gate template set: demo)"`
- `src/cli/setup.rs:2233-2233` `"scaffolded .rigger/workflow.yml"`
- `src/cli/setup.rs:2234-2234` `"no gate template set matches this project (demo: a or b; other: c at the project \
                 root), so .rigger/workflow.yml declares no gates - declare your own under gates:"`
- `src/cli/setup.rs:2251-2251` `".rigger/workflow.yml"`
- `src/cli/setup.rs:2571-2571` `".rigger/agents/"`
- `src/cli/setup.rs:2625-2625` `".rigger/dash.url"`
- `src/cli/setup.rs:2626-2626` `".rigger/dash.marker"`
- `src/cli/setup.rs:2627-2627` `".rigger/dash.attempt"`
- `src/cli/setup.rs:2636-2636` `".rigger/store.conn"`
- `src/cli/setup.rs:2653-2653` `".rigger/\n"`
- `src/cli/setup.rs:2659-2659` `".rigger/dash.url"`
- `src/cli/setup.rs:2662-2662` `".rigger/dash.marker"`
- `src/cli/setup.rs:2665-2665` `".rigger/dash.attempt"`
- `src/cli/setup.rs:2666-2666` `"setup appends the explicit dash lines (including the round-8 attempt breadcrumb) \
             even when .rigger/ broadly covers them, so the committed .gitignore stays \
             self-contained, got: {:?}"`
- `src/cli/setup.rs:2674-2674` `".rigger/dash.url"`
- `src/cli/setup.rs:2675-2675` `".rigger/dash.marker"`
- `src/cli/setup.rs:2676-2676` `".rigger/dash.attempt"`
- `src/cli/setup.rs:2677-2677` `"all three explicit per-file dash ignore lines are present in the committed \
             .gitignore even though .rigger/ already covers them, got:\n{content}"`
- `src/cli/setup.rs:2687-2687` `".rigger/dash.url"`
- `src/cli/setup.rs:2690-2690` `".rigger/dash.marker"`
- `src/cli/setup.rs:2693-2693` `".rigger/dash.attempt"`
- `src/cli/setup.rs:3046-3046` `".rigger/agents/researcher.md"`
- `src/cli/setup.rs:3075-3075` `".rigger/agents/planner.md"`
- `src/cli/setup.rs:3105-3105` `".rigger/agents/newcomer.md"`
- `src/cli/setup.rs:3183-3183` `".rigger/workflow.yml"`
- `src/cli/setup.rs:3413-3413` `".rigger"`
- `src/cli/setup.rs:3432-3432` `".rigger/agents/{f}"`
- `src/cli/setup.rs:3558-3558` `".rigger/agents/rust-engineer.md"`
- `src/cli/setup.rs:3584-3584` `".rigger/agents/sdet-author.md"`
- `src/cli/validate.rs:861-861` `"warning: tracked .rigger/ files have uncommitted modifications:"`
- `src/cli/validate.rs:1303-1303` `" M .rigger/workflow.yml\n\
                         M  .rigger/agents/sdet.md\n\
                         A  .rigger/agents/new.md\n\
                         D  .rigger/agents/gone.md\n\
                         ?? .rigger/events.db\n\
                         !! .rigger/shim/node_modules\n"`
- `src/cli/validate.rs:1313-1313` `".rigger/workflow.yml"`
- `src/cli/validate.rs:1314-1314` `".rigger/agents/sdet.md"`
- `src/cli/validate.rs:1315-1315` `".rigger/agents/new.md"`
- `src/cli/validate.rs:1316-1316` `".rigger/agents/gone.md"`
- `src/cli/validate.rs:2192-2192` `".rigger"`
- `src/cli/validate.rs:2246-2246` `".rigger"`
- `src/main.rs:181-181` `"rigger - a config-driven, event-sourced multi-agent dev-loop harness\n\n\
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
left, unless a rebuild in progress holds graph.db.lock\n  \
rigger reset --derived      compact the EVENT LOG: keep only each file's latest\n                              \
generation of the derived index, at the latest event per\n                              \
replay key, delete the superseded generations and\n                              \
re-recordings, and vacuum so the file shrinks on disk.\n                              \
Every other event survives. Sheds what edits and the\n                              \
pre-dedup ingest accreted;\n                              \
composes with --runs (each prunes its own accumulation).\n                              \
Refuses while the run is live (a held step lock, an\n                              \
in-flight spawn's marker inside its wall-clock bound,\n                              \
or a driver registration's heartbeat inside the idle\n                              \
window), naming what is live (a dead driver's run is\n                              \
not): compaction\n                              \
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
defaults.review.adversary) reads the spec text and its\n                              \
findings and verdict are recorded keyed on the text's\n                              \
content hash, so unchanged text is answered from the\n                              \
record with no spawn; takes --eventstore/--conn as run does\n  \
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
rigger prime [<spec>]       print recent decisions (what the hook runs); given a spec\n                              \
path, also names `rigger validate <spec>` (the pre-launch\n                              \
spec lint) as a next step\n  \
rigger instructions         print the instructions every spawned agent is held to:\n                              \
the built-in law and working discipline, then each\n                              \
operator file in .rigger/instructions/ (filename\n                              \
order)\n  \
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
- `tests/architecture_current_surface.rs:93-93` `".rigger/store.conn"`
- `tests/architecture_current_surface.rs:145-145` `"docs/architecture.md must describe the system that exists today (spec 56, \
         criterion 1): it must name the store-resolution and configuration surface (the \
         committed `store:` selection, the `KURRENTDB_CONN` environment variable, and the \
         per-machine `.rigger/store.conn` secret file) and the graph inspector's real query \
         surface (the three `lens=` names and the directed `view=calls` `dir=` views). \
         Surfaces the document fails to name: {missing:#?}"`
- `tests/boundary_audit.rs:858-858` `"let b = \"sh .rigger/gates/mutation.sh\";\n"`
- `tests/canary_model_drift_periphery.rs:62-62` `".rigger"`
- `tests/change_path_revert_periphery.rs:81-81` `".rigger"`
- `tests/checkin_mutation_diff_base_periphery.rs:63-63` `".rigger"`
- `tests/checkpoint_commit_hook_bypass_periphery.rs:58-58` `"{repo_path}/.rigger-test-scratch"`
- `tests/ci_lanes.rs:35-35` `".rigger/gates/lanes.sh"`
- `tests/cli.rs:102-102` `".rigger"`
- `tests/cli.rs:106-106` `".rigger"`
- `tests/cli.rs:167-167` `".rigger"`
- `tests/cli.rs:628-628` `".rigger"`
- `tests/cli.rs:879-879` `".rigger"`
- `tests/cli.rs:1036-1036` `".rigger"`
- `tests/cli.rs:1057-1057` `".rigger"`
- `tests/cli.rs:1162-1162` `"/.rigger/"`
- `tests/cli.rs:1163-1163` `"the default must never live under any .rigger, even on the HOME-only fallback \
         rung; got: {stdout:?}"`
- `tests/cli.rs:1213-1213` `"{}/.rigger/tmp"`
- `tests/cli.rs:1225-1225` `"/.rigger/tmp/"`
- `tests/cli.rs:1317-1317` `".rigger"`
- `tests/cli.rs:1327-1327` `".rigger"`
- `tests/cli.rs:1392-1392` `".rigger"`
- `tests/cli.rs:1393-1393` `".rigger"`
- `tests/cli.rs:1405-1405` `".rigger"`
- `tests/cli.rs:1426-1426` `".rigger"`
- `tests/cli.rs:1480-1480` `".rigger"`
- `tests/cli.rs:1520-1520` `".rigger"`
- `tests/cli.rs:1540-1540` `".rigger"`
- `tests/cli.rs:1555-1555` `".rigger"`
- `tests/cli.rs:1671-1671` `".rigger"`
- `tests/cli.rs:2167-2167` `".rigger"`
- `tests/cli.rs:2379-2379` `".rigger"`
- `tests/cli.rs:2380-2380` `"graph build must create .rigger/graph.db even when there is nothing to ingest"`
- `tests/cli.rs:2403-2403` `".rigger"`
- `tests/cli.rs:2483-2483` `".rigger"`
- `tests/cli.rs:2498-2498` `".rigger"`
- `tests/cli.rs:2548-2548` `".rigger"`
- `tests/cli.rs:2573-2573` `".rigger"`
- `tests/cli.rs:3035-3035` `".rigger"`
- `tests/cli.rs:3039-3039` `"grounding via symbols must persist the structural index to .rigger/symbols/"`
- `tests/cli.rs:3211-3211` `".rigger"`
- `tests/cli.rs:3500-3500` `".rigger"`
- `tests/cli.rs:3538-3538` `".rigger"`
- `tests/cli.rs:3733-3733` `"cd ${REPO} && CARGO_TARGET_DIR=${REPO}/.rigger/tmp/cargo-target rigger step"`
- `tests/cli.rs:3781-3781` `"cd ${REPO} && CARGO_TARGET_DIR=${REPO}/.rigger/tmp/cargo-target rigger step"`
- `tests/cli.rs:3968-3968` `".rigger/tmp/agent-scratch/"`
- `tests/cli.rs:3974-3974` `".rigger/tmp/cargo-target"`
- `tests/cli.rs:3982-3982` `"CARGO_TARGET_DIR=${REPO}/.rigger/tmp/cargo-target rigger step"`
- `tests/cli.rs:4452-4452` `".rigger"`
- `tests/cli.rs:4554-4554` `".rigger"`
- `tests/cli.rs:5294-5294` `".rigger"`
- `tests/cli.rs:5494-5494` `".rigger"`
- `tests/cli.rs:5661-5661` `".rigger"`
- `tests/cli.rs:5888-5888` `".rigger"`
- `tests/cli.rs:6038-6038` `".rigger"`
- `tests/cli.rs:6227-6227` `".rigger"`
- `tests/cli.rs:6378-6378` `".rigger"`
- `tests/cli.rs:6587-6587` `".rigger"`
- `tests/cli.rs:6786-6786` `".rigger"`
- `tests/cli.rs:6874-6874` `".rigger"`
- `tests/cli.rs:6995-6995` `".rigger"`
- `tests/cli.rs:7188-7188` `".rigger/events.db"`
- `tests/cli.rs:7710-7710` `".rigger"`
- `tests/cli.rs:7786-7786` `".rigger"`
- `tests/cli.rs:7817-7817` `".rigger"`
- `tests/cli.rs:7996-7996` `"/.rigger/"`
- `tests/cli.rs:7997-7997` `"the default marker path must never live under any .rigger; got: {marker_str:?}"`
- `tests/cli.rs:8203-8203` `"/.rigger/"`
- `tests/cli.rs:8204-8204` `"the unbounded spawn's marker resolves under the scratch root's agent-live, never under \
         any .rigger; got: {marker_str:?}"`
- `tests/cli.rs:8435-8435` `"/.rigger/tmp/agent-live/"`
- `tests/cli.rs:8436-8436` `"under RIGGER_TMPDIR the marker is not under the repo's .rigger/tmp; got: {marker_str:?}"`
- `tests/cli.rs:9050-9050` `".rigger"`
- `tests/cli.rs:9322-9322` `".rigger"`
- `tests/cli.rs:9421-9421` `".rigger"`
- `tests/cli.rs:9510-9510` `".rigger"`
- `tests/cli.rs:9550-9550` `".rigger"`
- `tests/cli.rs:10603-10603` `".rigger/workflow.yml"`
- `tests/cli.rs:10603-10603` `".rigger/agents"`
- `tests/cli.rs:10692-10692` `".rigger/workflow.yml"`
- `tests/cli.rs:10692-10692` `".rigger/agents"`
- `tests/cli.rs:10694-10694` `".rigger"`
- `tests/cli.rs:10695-10695` `".rigger/workflow.yml"`
- `tests/cli.rs:10900-10900` `".rigger/workflow.yml"`
- `tests/cli.rs:10900-10900` `".rigger/agents"`
- `tests/cli.rs:10947-10947` `".rigger/workflow.yml"`
- `tests/cli.rs:10947-10947` `".rigger/agents"`
- `tests/cli.rs:10960-10960` `".rigger"`
- `tests/cli.rs:10966-10966` `".rigger"`
- `tests/cli.rs:10968-10968` `".rigger"`
- `tests/cli.rs:11065-11065` `".rigger/workflow.yml"`
- `tests/cli.rs:11066-11066` `"validate must NOT flag a clean tracked `.rigger/` tree; stderr:\n{err}"`
- `tests/cli.rs:11075-11075` `".rigger"`
- `tests/cli.rs:11082-11082` `"validate must still succeed (exit 0) when it only FLAGS uncommitted `.rigger/` \
         changes; stderr:\n{err}"`
- `tests/cli.rs:11086-11086` `".rigger/workflow.yml"`
- `tests/cli.rs:11087-11087` `"validate must flag the tracked-but-modified `.rigger/workflow.yml` on stderr; \
         stderr:\n{err}"`
- `tests/cli.rs:11103-11103` `".rigger"`
- `tests/cli.rs:11377-11377` `".rigger"`
- `tests/cli.rs:11526-11526` `".rigger"`
- `tests/cli.rs:11562-11562` `".rigger"`
- `tests/cli.rs:11602-11602` `".rigger"`
- `tests/cli.rs:11825-11825` `".rigger"`
- `tests/cli.rs:11927-11927` `".rigger"`
- `tests/cli.rs:12071-12071` `".rigger"`
- `tests/cli.rs:12073-12073` `".rigger"`
- `tests/cli.rs:12075-12075` `".rigger"`
- `tests/cli.rs:12077-12077` `".rigger"`
- `tests/cli.rs:12105-12105` `"probe/.rigger/events.db"`
- `tests/cli.rs:12159-12159` `"validate must warn about residue planted under the relocated cache-home DEFAULT \
         root - a regression that left its residue scan still rooted at the pre-relocation \
         `.rigger/tmp` would silently miss this and print nothing; stderr:\n{err}"`
- `tests/cli.rs:12184-12184` `".rigger"`
- `tests/cli.rs:12629-12629` `".rigger"`
- `tests/cli.rs:12637-12637` `".rigger"`
- `tests/cli.rs:12678-12678` `"\n  shadow store: probe/.rigger/events.db (2B)"`
- `tests/cli.rs:13028-13028` `".rigger"`
- `tests/cli.rs:13125-13125` `"scaffolded .rigger/workflow.yml"`
- `tests/cli.rs:13129-13129` `"scaffolded .rigger/agents/"`
- `tests/cli.rs:13161-13161` `".rigger"`
- `tests/cli.rs:13193-13193` `".rigger/agents/researcher.md"`
- `tests/cli.rs:13194-13194` `"the agent was actually imported into .rigger/agents/"`
- `tests/cli.rs:13241-13241` `".rigger"`
- `tests/cli.rs:13422-13422` `".rigger/dash.url"`
- `tests/cli.rs:13426-13426` `".rigger/dash.marker"`
- `tests/cli.rs:13430-13430` `".rigger/dash.attempt"`
- `tests/cli.rs:13439-13439` `".rigger"`
- `tests/cli.rs:13441-13441` `".rigger"`
- `tests/cli.rs:13445-13445` `".rigger"`
- `tests/cli.rs:13446-13446` `".rigger"`
- `tests/cli.rs:13448-13448` `".rigger/dash.url"`
- `tests/cli.rs:13449-13449` `".rigger/dash.marker"`
- `tests/cli.rs:13450-13450` `".rigger/dash.attempt"`
- `tests/cli.rs:13487-13487` `".claude/\n.rigger/\n"`
- `tests/cli.rs:13503-13503` `".rigger/dash.url"`
- `tests/cli.rs:13511-13511` `"the test's global config must actually ignore .rigger/dash.url (else the regression \
         guard is inconclusive)"`
- `tests/cli.rs:13534-13534` `".rigger/shim"`
- `tests/cli.rs:13535-13535` `".rigger/dash.url"`
- `tests/cli.rs:13536-13536` `".rigger/dash.marker"`
- `tests/cli.rs:13537-13537` `".rigger/dash.attempt"`
- `tests/cli.rs:13669-13669` `".rigger/project.id"`
- `tests/cli.rs:13672-13672` `".rigger/project.id"`
- `tests/cli.rs:13685-13685` `".rigger/project.id"`
- `tests/cli.rs:13774-13774` `".rigger/project.id"`
- `tests/cli.rs:13823-13823` `".rigger/project.id"`
- `tests/cli.rs:13828-13828` `".rigger/project.id"`
- `tests/cli.rs:13842-13842` `".rigger"`
- `tests/cli.rs:14070-14070` `".rigger"`
- `tests/cli.rs:14673-14673` `".rigger"`
- `tests/cli.rs:14802-14802` `".rigger"`
- `tests/cli.rs:14804-14804` `".rigger"`
- `tests/cli.rs:14953-14953` `".rigger"`
- `tests/cli.rs:15310-15310` `".rigger"`
- `tests/cli.rs:15354-15354` `".rigger"`
- `tests/cli.rs:15794-15794` `".rigger"`
- `tests/cli.rs:15809-15809` `"the driver never recorded a dash URL in .rigger/dash.url; stderr:\n{err}"`
- `tests/cli.rs:15869-15869` `".rigger"`
- `tests/cli.rs:16212-16212` `".rigger"`
- `tests/cli.rs:16920-16920` `".rigger"`
- `tests/cli.rs:16922-16922` `".rigger"`
- `tests/cli.rs:17843-17843` `".rigger/dash.marker"`
- `tests/cli.rs:17901-17901` `".rigger/dash.url"`
- `tests/cli.rs:17909-17909` `".rigger/dash.marker"`
- `tests/cli.rs:18131-18131` `".rigger/dash.url"`
- `tests/cli.rs:18133-18133` `".rigger/dash.marker"`
- `tests/cli.rs:18204-18204` `".rigger/dash.marker"`
- `tests/cli.rs:18211-18211` `".rigger/dash.attempt"`
- `tests/cli.rs:19051-19051` `".rigger"`
- `tests/cli.rs:19771-19771` `".rigger"`
- `tests/cli.rs:20097-20097` `".rigger"`
- `tests/cli.rs:20132-20132` `".rigger"`
- `tests/cli.rs:20207-20207` `"the first step must record a dash marker at .rigger/dash.marker; stderr:\n{err1}"`
- `tests/cli.rs:20288-20288` `"the step must record a dash marker at .rigger/dash.marker; stderr:\n{err}"`
- `tests/cli.rs:20302-20302` `"`rigger step` under RIGGER_DASH_PORT={dash_port} must bind its step-path dash at EXACTLY \
         that port and record it in .rigger/dash.marker (proving the override reaches the real \
         bind, not the fixed dash::DEFAULT_PORT); the marker instead recorded {marker_port}"`
- `tests/cli.rs:20363-20363` `".rigger"`
- `tests/cli.rs:20447-20447` `".rigger"`
- `tests/cli.rs:20832-20832` `".rigger"`
- `tests/cli.rs:20844-20844` `".rigger"`
- `tests/cli.rs:20871-20871` `".rigger"`
- `tests/cli.rs:20899-20899` `".rigger"`
- `tests/cli.rs:20910-20910` `".rigger"`
- `tests/cli.rs:20947-20947` `".rigger"`
- `tests/cli.rs:21036-21036` `"the first step must record a dash marker at .rigger/dash.marker; stderr:\n{err1}"`
- `tests/cli.rs:21105-21105` `"{root}/.rigger/events.db"`
- `tests/cli.rs:21725-21725` `"{other_root}/.rigger/events.db"`
- `tests/cli.rs:22112-22112` `"/stale/root/.rigger/events.db"`
- `tests/cli.rs:22134-22134` `"/live/root/.rigger/events.db"`
- `tests/cli.rs:22217-22217` `"the step must record a dash marker at .rigger/dash.marker"`
- `tests/cli.rs:22417-22417` `".rigger"`
- `tests/cli.rs:22600-22600` `".rigger"`
- `tests/cli.rs:22704-22704` `".rigger"`
- `tests/cli.rs:22830-22830` `".rigger"`
- `tests/cli.rs:23510-23510` `".rigger/gates"`
- `tests/cli.rs:23516-23516` `".rigger/gates/{name}"`
- `tests/cli.rs:23557-23557` `"sh .rigger/gates/content.sh {check} \"$RIGGER_RUN_BASE\""`
- `tests/cli.rs:23564-23564` `"sh .rigger/gates/content.sh {check}"`
- `tests/cli.rs:23612-23612` `".rigger/dash.attempt"`
- `tests/cli.rs:23613-23613` `"a real step's own ensure_run_dashboard call must record .rigger/dash.attempt \
         (record_dash_attempt); without it this test cannot exercise the round-8 fact at all"`
- `tests/cli.rs:23688-23688` `".rigger/dash.marker"`
- `tests/cli.rs:23717-23717` `".rigger/dash.url"`
- `tests/cli.rs:23724-23724` `".rigger/dash.attempt"`
- `tests/cli.rs:23784-23784` `".rigger/dash.marker"`
- `tests/cli.rs:23787-23787` `".rigger/dash.url"`
- `tests/cli.rs:23840-23840` `".rigger/dash.marker"`
- `tests/cli.rs:23862-23862` `".rigger/dash.url"`
- `tests/cli.rs:23867-23867` `".rigger/dash.attempt"`
- `tests/cli.rs:23905-23905` `".rigger/dash.url"`
- `tests/cli.rs:23916-23916` `".rigger/dash.marker"`
- `tests/cli.rs:24396-24396` `".rigger"`
- `tests/cli.rs:24549-24549` `".rigger"`
- `tests/cli.rs:25460-25460` `".rigger"`
- `tests/cli.rs:25555-25555` `".rigger"`
- `tests/cli.rs:25791-25791` `".rigger"`
- `tests/cli.rs:26247-26247` `"the hook must be inert on a project without .rigger/; got:\n{out}"`
- `tests/cli.rs:26253-26253` `".rigger"`
- `tests/cli.rs:26272-26272` `".rigger"`
- `tests/cli.rs:26298-26298` `".rigger"`
- `tests/cli.rs:26334-26334` `".rigger"`
- `tests/cli.rs:26373-26373` `".rigger"`
- `tests/cli.rs:26565-26565` `".rigger"`
- `tests/cli.rs:26598-26598` `".rigger"`
- `tests/cli.rs:26750-26750` `".rigger"`
- `tests/cli.rs:26923-26923` `".rigger"`
- `tests/cli.rs:26978-26978` `".rigger"`
- `tests/cli.rs:27009-27009` `"scaffolded .rigger/instructions/README.md"`
- `tests/cli.rs:27013-27013` `".rigger/instructions/README.md"`
- `tests/cli.rs:27083-27083` `"gw/.rigger/workflow.yml"`
- `tests/cli.rs:27121-27121` `"gw/.rigger/workflow.yml"`
- `tests/cli.rs:27587-27587` `"gw/.rigger/workflow.yml"`
- `tests/cli.rs:27605-27605` `"gw/.rigger/workflow.yml"`
- `tests/common/cli.rs:248-248` `".rigger"`
- `tests/common/cli.rs:259-259` `".rigger"`
- `tests/common/cli.rs:280-280` `".rigger"`
- `tests/common/cli.rs:297-297` `".rigger"`
- `tests/common/cli.rs:453-453` `"graph build: ingested {events} code-ingest event(s) into .rigger/graph.db{lost}\n"`
- `tests/common/cli.rs:660-660` `".rigger"`
- `tests/common/cli.rs:661-661` `"create .rigger/agents"`
- `tests/common/cli.rs:774-774` `"{why}: a server selection must NOT fabricate a local .rigger/events.db"`
- `tests/common/cli.rs:871-871` `".rigger"`
- `tests/common/fixtures/config.rs:102-102` `"{repo_path}/.rigger-test-scratch"`
- `tests/common/fixtures/extraction_tree.rs:76-76` `".rigger/workflow.yml"`
- `tests/common/layer_cli.rs:22-22` `".rigger"`
- `tests/common/layer_cli.rs:28-28` `".rigger"`
- `tests/common/layer_cli.rs:301-301` `".rigger"`
- `tests/common/mod.rs:308-308` `"{}/.rigger-test-scratch"`
- `tests/common/workflow_probe.rs:15-15` `".rigger"`
- `tests/common/workflow_probe.rs:16-16` `"create .rigger"`
- `tests/compaction_generations_periphery.rs:2992-2992` `".rigger"`
- `tests/compaction_generations_periphery.rs:3123-3123` `".rigger"`
- `tests/compaction_generations_periphery.rs:4182-4182` `"rigger: migrated project identity - renamed 1 stream(s) from the legacy namespace \
         {legacy:?} to the minted identity {minted:?} (.rigger/project.id); recorded its \
         decision (position {position}){fold}\n"`
- `tests/compaction_generations_periphery.rs:5487-5487` `".rigger"`
- `tests/config_unknown_key_dotted_path_periphery.rs:138-138` `".rigger"`
- `tests/config_unknown_key_dotted_path_periphery.rs:139-139` `"write the .rigger/{rel} fixture: {e}"`
- `tests/config_unknown_key_dotted_path_periphery.rs:149-149` `".rigger"`
- `tests/config_unknown_key_dotted_path_periphery.rs:423-423` `".rigger"`
- `tests/courier_registry_refresh_boundary_periphery.rs:83-83` `".rigger"`
- `tests/courier_registry_refresh_fence_periphery.rs:51-51` `".rigger"`
- `tests/dedup_seeding_periphery.rs:342-342` `".rigger"`
- `tests/dedup_seeding_periphery.rs:366-366` `".rigger"`
- `tests/dedup_seeding_periphery.rs:570-570` `".rigger"`
- `tests/duplication_catalog_contract_periphery.rs:103-103` `".rigger-path string literals"`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:121-121` `".rigger"`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:157-157` `".rigger"`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:277-277` `".rigger"`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:569-569` `".rigger"`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:1045-1045` `".rigger"`
- `tests/gate_store_fence_periphery.rs:207-207` `".rigger"`
- `tests/gate_store_fence_periphery.rs:208-208` `".rigger"`
- `tests/gate_store_fence_periphery.rs:213-213` `".rigger"`
- `tests/gate_store_fence_periphery.rs:214-214` `".rigger"`
- `tests/gate_store_fence_periphery.rs:216-216` `".rigger"`
- `tests/gate_store_fence_periphery.rs:221-221` `".rigger"`
- `tests/gate_store_fence_periphery.rs:272-272` `".rigger"`
- `tests/gate_store_fence_periphery.rs:273-273` `".rigger"`
- `tests/gate_store_fence_periphery.rs:280-280` `".rigger"`
- `tests/gate_store_fence_periphery.rs:283-283` `".rigger"`
- `tests/gate_store_fence_periphery.rs:555-555` `".rigger"`
- `tests/gate_store_fence_periphery.rs:578-578` `".rigger"`
- `tests/gate_store_fence_periphery.rs:691-691` `".rigger"`
- `tests/gate_store_fence_periphery.rs:692-692` `".rigger"`
- `tests/gate_store_fence_periphery.rs:702-702` `".rigger"`
- `tests/gate_store_fence_periphery.rs:704-704` `".rigger"`
- `tests/gate_store_fence_periphery.rs:807-807` `".rigger"`
- `tests/gate_store_fence_periphery.rs:808-808` `".rigger"`
- `tests/graph_around_code_first.rs:59-59` `".rigger"`
- `tests/graph_around_governance_boundaries.rs:79-79` `".rigger"`
- `tests/graph_around_governance_boundaries.rs:110-110` `".rigger"`
- `tests/graph_around_governance_boundaries.rs:154-154` `".rigger"`
- `tests/graph_around_governance_boundaries.rs:173-173` `".rigger"`
- `tests/graph_around_governance_boundaries.rs:226-226` `".rigger"`
- `tests/graph_show_surface.rs:67-67` `".rigger"`
- `tests/halted_spawn_wip_recovery_periphery.rs:610-610` `".rigger"`
- `tests/handbook_grounder_accuracy.rs:131-131` `".rigger"`
- `tests/handbook_grounder_accuracy.rs:140-140` `".rigger/workflow.yml must carry a `grounder:` default (spec 57, criterion 4)"`
- `tests/handbook_grounder_accuracy.rs:145-145` `"the repo's own .rigger/workflow.yml must default to the structural `symbols` grounder \
         (spec 57): the vector engine `turbovec` and its `hybrid` composite were retired. Found: \
         grounder: {workflow_value:?}"`
- `tests/handbook_grounder_accuracy.rs:151-151` `"docs/handbook/authoring-loops.md claims its example reproduces the repo's own \
         .rigger/workflow.yml, so its `grounder:` value ({handbook_value:?}) must equal the \
         committed workflow's ({workflow_value:?}). An operator copies this block verbatim - a \
         stale `grounder: turbovec` here pastes the retired engine and hits the loud \
         retired-grounder migration error instead of a working run."`
- `tests/heartbeat_write_read_agree_periphery.rs:117-117` `".rigger"`
- `tests/heartbeat_write_read_agree_periphery.rs:221-221` `".rigger"`
- `tests/heartbeat_write_read_agree_periphery.rs:226-226` `".rigger"`
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
- `tests/init_template_sets_periphery.rs:286-286` `".rigger/workflow.yml"`
- `tests/integrate_conflict_merge_periphery.rs:453-453` `"the project's own .rigger/workflow.yml must load through the real loader"`
- `tests/ledger_rebuild.rs:473-473` `"gw/.rigger/workflow.yml"`
- `tests/ledger_rebuild.rs:646-646` `"gw/.rigger/workflow.yml"`
- `tests/ledger_rebuild.rs:1218-1218` `"gw/.rigger/workflow.yml"`
- `tests/ledger_rebuild.rs:2260-2260` `"gw/.rigger/workflow.yml"`
- `tests/ledger_rebuild.rs:2290-2290` `"gw/.rigger/workflow.yml"`
- `tests/migration_is_deliberate_periphery.rs:475-475` `".rigger"`
- `tests/migration_is_deliberate_periphery.rs:522-522` `".rigger"`
- `tests/principle_gates_wiring.rs:51-51` `"the scaffolded .rigger/agents: {missing:#?}"`
- `tests/principle_gates_wiring.rs:65-65` `".rigger/gates/red-before-green.sh"`
- `tests/principle_gates_wiring.rs:66-66` `".rigger/gates/mutation.sh"`
- `tests/principle_gates_wiring.rs:67-67` `".rigger/gates/container-env.sh"`
- `tests/principle_gates_wiring.rs:127-127` `".rigger/gates"`
- `tests/principle_gates_wiring.rs:366-366` `".rigger/agents/sdet-author.md"`
- `tests/principle_gates_wiring.rs:380-380` `".rigger/agents"`
- `tests/principle_gates_wiring.rs:390-390` `".rigger/agents: {missing:#?}"`
- `tests/principle_gates_wiring.rs:394-394` `".rigger/gates/container-env.sh"`
- `tests/projections_stay_local.rs:91-91` `"the graph projection must be opened by the LOCAL sqlite Projector at the resolved \
         store's .rigger/graph.db (`Projector::open(&self.file(\"graph.db\") ...)`); the \
         canonical local construction is gone"`
- `tests/projections_stay_local.rs:99-99` `"the progress projection must be opened by the LOCAL sqlite Store at .rigger/progress.db \
         (`Store::open(&db_path(\"progress.db\") ...)`); the canonical local construction is gone"`
- `tests/projections_stay_local.rs:149-149` `".rigger"`
- `tests/projections_stay_local.rs:152-152` `"graph.db must be created under the LOCAL .rigger/ even when the event store is the \
             server - projections are per-machine and stay local"`
- `tests/projections_stay_local.rs:162-162` `".rigger"`
- `tests/projections_stay_local.rs:163-163` `"a server-configured `graph build` must NOT create a local .rigger/events.db - the \
             event log is the server's; only the projection is local"`
- `tests/projections_stay_local.rs:206-206` `".rigger"`
- `tests/projections_stay_local.rs:209-209` `"progress.db must be created under the LOCAL .rigger/ even when the event store is the \
             server - the progress store is a local projection, not the shared log"`
- `tests/projections_stay_local.rs:232-232` `".rigger"`
- `tests/projections_stay_local.rs:233-233` `"a server-configured `rigger progress` must NOT create a local .rigger/events.db - the \
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
- `tests/relocated_worktree_store_resolution_periphery.rs:62-62` `".rigger"`
- `tests/relocated_worktree_store_resolution_periphery.rs:63-63` `"create .rigger"`
- `tests/relocated_worktree_store_resolution_periphery.rs:108-108` `".rigger"`
- `tests/relocated_worktree_store_resolution_periphery.rs:108-108` `"create the worktree's .rigger"`
- `tests/relocated_worktree_store_resolution_periphery.rs:110-110` `".rigger"`
- `tests/relocated_worktree_store_resolution_periphery.rs:143-143` `".rigger"`
- `tests/relocated_worktree_store_resolution_periphery.rs:195-195` `".rigger"`
- `tests/relocated_worktree_store_resolution_periphery.rs:238-238` `".rigger"`
- `tests/relocated_worktree_store_resolution_periphery.rs:288-288` `".rigger"`
- `tests/reset_build_cache_periphery.rs:425-425` `".rigger"`
- `tests/reset_build_cache_periphery.rs:504-504` `".rigger"`
- `tests/reset_derived_compaction_periphery.rs:1277-1277` `".rigger"`
- `tests/reset_derived_compaction_periphery.rs:2970-2970` `".rigger"`
- `tests/reset_derived_compaction_periphery.rs:2970-2970` `"create .rigger"`
- `tests/reset_derived_compaction_periphery.rs:2985-2985` `".rigger"`
- `tests/reset_derived_compaction_periphery.rs:3020-3020` `".rigger"`
- `tests/reset_derived_live_writer_guard_periphery.rs:69-69` `".rigger"`
- `tests/reset_derived_live_writer_guard_periphery.rs:1391-1391` `".rigger"`
- `tests/reset_derived_live_writer_guard_periphery.rs:1498-1498` `".rigger"`
- `tests/review_round_lenses_only_log_derived_resume_periphery.rs:146-146` `"{repo_path}/.rigger-test-scratch"`
- `tests/review_round_no_adjudicator_residue_periphery.rs:94-94` `"{repo_path}/.rigger-test-scratch"`
- `tests/review_round_non_ancestor_residue_names_true_diff_periphery.rs:120-120` `"{repo_path}/.rigger-test-scratch"`
- `tests/simplification_audit.rs:2799-2799` `".rigger-path string literals"`
- `tests/simplification_audit.rs:2984-2984` `".rigger"`
- `tests/simplification_audit.rs:2985-2985` `"one .rigger-relative path-composition helper"`
- `tests/simplification_audit.rs:5100-5100` `"Largest risk-reduction first is read as six tiers, ranked by the KIND of risk \
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
- `tests/simplification_audit.rs:5381-5381` `"#### 9. Consolidate the {rigger_n} `.rigger`-path string-literal sites (`{rigger_id}`) - the \
        single largest cluster in the entire catalog by site count\n\n"`
- `tests/simplification_audit.rs:5385-5385` `"- Scope: one `.rigger`-relative path-composition helper (the cluster's own \
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
- `tests/simplification_audit.rs:8520-8520` `"fn a() {\n    let _ = \".rigger/tmp\";\n}\n"`
- `tests/simplification_audit.rs:8522-8522` `".rigger"`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:54-54` `".rigger"`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:189-189` `".rigger"`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:200-200` `".rigger"`
- `tests/spec_critique_periphery.rs:357-357` `".rigger/workflow.yml"`
- `tests/spec_critique_periphery.rs:603-603` `".rigger"`
- `tests/spec_critique_periphery.rs:1289-1289` `".rigger/project.id"`
- `tests/statusline_command_periphery.rs:118-118` `".rigger"`
- `tests/step_sheds_the_freshen.rs:67-67` `".rigger/grounding"`
- `tests/step_sheds_the_freshen.rs:216-216` `".rigger/symbols/index.json"`
- `tests/step_sheds_the_freshen.rs:217-217` `"the surviving persisted index is the SYMBOL index under .rigger/symbols/; got {}"`
- `tests/step_sheds_the_freshen.rs:282-282` `"constructing the default grounder builds and persists the SYMBOL index under \
         .rigger/symbols/ - the freshen's real target"`
- `tests/stop_failure_hook_periphery.rs:58-58` `".rigger"`
- `tests/stop_failure_hook_periphery.rs:245-245` `".rigger/events.db"`
- `tests/store_content_identity_periphery.rs:722-722` `".rigger"`
- `tests/store_content_identity_periphery.rs:723-723` `"create .rigger"`
- `tests/store_content_identity_periphery.rs:769-769` `".rigger"`
- `tests/store_precedence.rs:48-48` `".rigger"`
- `tests/store_precedence.rs:56-56` `".rigger"`
- `tests/store_precedence.rs:102-102` `".rigger"`
- `tests/store_precedence.rs:114-114` `".rigger"`
- `tests/store_precedence.rs:126-126` `".rigger"`
- `tests/store_precedence.rs:131-131` `".rigger/store.conn beats the committed store: config"`
- `tests/store_precedence.rs:141-141` `".rigger"`
- `tests/store_precedence.rs:179-179` `"a loud failure for {what} must leave no fabricated local .rigger/events.db behind"`
- `tests/store_precedence.rs:212-212` `".rigger"`
- `tests/store_precedence.rs:229-229` `".rigger"`
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
- `tests/validate_advisories.rs:240-240` `".rigger"`
- `tests/validate_advisories.rs:585-585` `".rigger"`
- `tests/validate_footprint_default_scratch_root_periphery.rs:34-34` `".rigger"`
- `tests/validate_footprint_default_scratch_root_periphery.rs:35-35` `".rigger"`
- `tests/validate_footprint_default_scratch_root_periphery.rs:70-70` `".rigger"`
- `tests/watchdog_cli_periphery.rs:165-165` `".rigger"`
- `tests/watchdog_cli_periphery.rs:352-352` `".rigger"`
- `tests/workflow_definition_and_js_constants_periphery.rs:89-89` `".rigger"`
- `tests/workflow_definition_and_js_constants_periphery.rs:91-91` `".rigger"`
- `tests/workflow_definition_and_js_constants_periphery.rs:365-365` `".rigger"`
- `tests/workflow_definition_and_js_constants_periphery.rs:367-367` `".rigger"`
- `tests/workflow_definition_and_js_constants_periphery.rs:615-615` `".rigger"`
- `tests/workflow_driver_resolved_model_periphery.rs:106-106` `".rigger"`
- `tests/worktree_liveness_fence_periphery.rs:110-110` `".rigger"`

#### `dup-97f834a327e5` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-conductor/src/conductor.rs, tests/postmerge_gate_error_cleanup_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:32949-32964` `spawn`
- `tests/postmerge_gate_error_cleanup_periphery.rs:71-82` `spawn`

#### `dup-9d4ce6bab62d` (near, 2 sites)

Proposed home: `conductor::capped_reads`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:36563-36571` `read_stream`
- `crates/rigger-conductor/src/conductor.rs:36601-36609` `read_stream_typed`

#### `dup-05f8d05060a7` (exact, 2 sites)

Proposed home: `conductor::capped_reads`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:36594-36600` `last_position`
- `crates/rigger-conductor/src/conductor.rs:36631-36638` `latest_in_group`

#### `dup-15a59608df13` (near, 4 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-conductor/src/conductor.rs, tests/revert_on_base_hook_bypass_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:40476-40520` `spawn`
- `crates/rigger-conductor/src/conductor.rs:40937-40976` `spawn`
- `crates/rigger-conductor/src/conductor.rs:41086-41133` `spawn`
- `tests/revert_on_base_hook_bypass_periphery.rs:78-113` `spawn`

#### `dup-b91a09b322bf` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-conductor/src/conductor.rs, tests/replan_episode_identity.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:44761-44771` `critique_cfg`
- `tests/replan_episode_identity.rs:278-288` `two_episode_cfg`

#### `dup-da45cd3da239` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: crates/rigger-conductor/src/conductor.rs, crates/rigger-domain/src/metrics.rs, tests/plan_critique_spec_defect_stop_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:45806-45808` `stop_halt`
- `crates/rigger-domain/src/metrics.rs:2733-2735` `canary_run_marker`
- `tests/plan_critique_spec_defect_stop_periphery.rs:66-68` `halt`

#### `dup-5c13daf57f07` (near, 2 sites)

Proposed home: `replay_keys::replay_keys`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/replay_keys.rs:39-41` `insert`
- `crates/rigger-conductor/src/replay_keys.rs:44-46` `contains`

#### `dup-78c48ff4c88e` (semantic, 2 sites)

Proposed home: `one shared `gating_agent_ids` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-config-files/src/config.rs:21-38` `gating_agent_ids`
- `crates/rigger-domain/src/config.rs:419-428` `gating_agent_ids`

#### `dup-3d575d71e10f` (exact, 2 sites)

Proposed home: `config_store::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-config-files/src/config_store.rs:208-216` `read_store_config`
- `crates/rigger-config-files/src/config_store.rs:273-281` `read_scratch_defaults`

#### `dup-947acd1ae58d` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: crates/rigger-config-files/src/config_store.rs, src/cli/mod.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-config-files/src/config_store.rs:754-787` `literal_is_emit_payload_binds_only_an_abutting_payload_or_emit_word`
- `src/cli/mod.rs:6375-6381` `is_uuid8_accepts_exactly_eight_hex_digits`
- `tests/simplification_audit.rs:8526-8534` `looks_error_shaping_matches_error_and_underscore_bounded_err_but_not_an_incidental_substring`

#### `dup-5c52773cc009` (near, 2 sites)

Proposed home: `config_store::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-config-files/src/config_store.rs:949-956` `parses_agent_frontmatter_and_body`
- `crates/rigger-config-files/src/config_store.rs:1304-1313` `model_ladder_parses_from_frontmatter`

#### `dup-360e8b87398d` (exact, 4 sites)

Proposed home: `a new shared module (sites span 4 files: crates/rigger-config-files/src/config_store.rs, crates/rigger-domain/src/contextgraph/query.rs, crates/rigger-domain/src/spec.rs, src/cli/validate.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-config-files/src/config_store.rs:959-961` `rejects_missing_frontmatter`
- `crates/rigger-domain/src/contextgraph/query.rs:1937-1939` `graph_load_rejects_malformed_json_without_panicking`
- `crates/rigger-domain/src/spec.rs:1275-1277` `empty_when_no_criteria`
- `src/cli/validate.rs:1324-1326` `dirty_tracked_paths_on_a_clean_tree_is_empty`

#### `dup-f61039476fc4` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-console/src/console/map.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-console/src/console/map.rs:877-883` `as_candidate`
- `tests/simplification_audit.rs:1683-1692` `map_entry_wire`
- `tests/simplification_audit.rs:1694-1701` `map_entry_lines`

#### `dup-ce4fa432a6da` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-console/src/console/map.rs, tests/files_lens_view_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-console/src/console/map.rs:1243-1245` `in_community`
- `tests/files_lens_view_periphery.rs:72-74` `refs`

#### `dup-c138bb72172b` (near, 9 sites)

Proposed home: `a new shared module (sites span 4 files: crates/rigger-console/src/console/map.rs, crates/rigger-domain/src/ledger.rs, crates/rigger-domain/src/progress.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-console/src/console/map.rs:1250-1253` `module_of_a_leaf_src_file_is_its_stem_never_the_file_name`
- `crates/rigger-console/src/console/map.rs:1282-1286` `district_purpose_uses_the_curated_table_when_present`
- `crates/rigger-domain/src/ledger.rs:1035-1042` `short_run_id_truncates_to_twelve_chars_and_passes_shorter_ids_through`
- `crates/rigger-domain/src/ledger.rs:1045-1060` `spec_stem_extracts_and_sanitizes_the_file_stem`
- `crates/rigger-domain/src/progress.rs:361-364` `each_run_reports_on_its_own_stream_and_a_run_less_report_on_the_bare_one`
- `tests/simplification_audit.rs:7358-7369` `impl_self_type_strips_a_trailing_where_clause_on_a_non_generic_self_type`
- `tests/simplification_audit.rs:7389-7400` `impl_self_type_strips_a_leading_dyn_token_on_the_self_type`
- `tests/simplification_audit.rs:7412-7418` `strip_trailing_where_clause_is_a_word_boundary_match_not_a_substring_match`
- `tests/simplification_audit.rs:7977-7987` `ident_kind_marker_classifies_by_casing`

#### `dup-cf5c8cd56db9` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-console/src/console/map.rs, crates/rigger-store-sqlite/src/eventstore/mod.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-console/src/console/map.rs:1265-1279` `module_of_never_returns_a_string_carrying_a_dot_or_a_slash`
- `crates/rigger-store-sqlite/src/eventstore/mod.rs:264-277` `no_credential_fragment_ever_survives`

#### `dup-5449e35bae05` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: crates/rigger-console/src/console/map.rs, crates/rigger-domain/src/spec.rs, crates/rigger-store-sqlite/src/eventstore/mod.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-console/src/console/map.rs:1952-1967` `budget_scales_the_step_by_zoom_rather_than_offsetting_it`
- `crates/rigger-domain/src/spec.rs:2202-2230` `strip_inline_code_direct_exact_output_pins_the_one_span_per_kind_rule`
- `crates/rigger-store-sqlite/src/eventstore/mod.rs:243-260` `a_delimiter_inside_the_userinfo_never_leaks_the_credential_head`

#### `dup-30a334181642` (near, 2 sites)

Proposed home: `map::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-console/src/console/map.rs:2101-2113` `caller_callee_graph`
- `crates/rigger-console/src/console/map.rs:2441-2455` `cross_district_graph`

#### `dup-b8d5ee4b6f2b` (near, 3 sites)

Proposed home: `map::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-console/src/console/map.rs:2458-2465` `landmarks_ranks_by_whole_map_degree_descending`
- `crates/rigger-console/src/console/map.rs:2562-2567` `search_is_case_insensitive`
- `crates/rigger-console/src/console/map.rs:2576-2584` `search_hit_carries_kind_and_degree_beside_the_name`

#### `dup-3eae43945289` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-dash/src/dash.rs, crates/rigger-domain/src/retention.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-dash/src/dash.rs:619-621` `serialize`
- `crates/rigger-domain/src/retention.rs:44-46` `identity`

#### `dup-99a8136efbc4` (semantic, 55 sites)

Proposed home: `crates/rigger-process/src/reap.rs as the one /proc-reading module (dash.rs's own /proc readers already duplicate reap.rs's field-after-the-comm's-closing-paren /proc/<pid>/stat parse - see the report's worked example)`

mandatory sweep: /proc-path string literals - 55 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-dash/src/dash.rs:658-658` `"/proc/net/tcp"`
- `crates/rigger-dash/src/dash.rs:689-689` `"/proc"`
- `crates/rigger-dash/src/dash.rs:8504-8504` `"/proc"`
- `crates/rigger-dash/src/dash.rs:8512-8512` `"the /proc scan must find THIS process as the holder of its own listener"`
- `crates/rigger-dash/src/dash.rs:8519-8519` `"/proc"`
- `crates/rigger-dash/src/dash.rs:8543-8543` `"/proc"`
- `crates/rigger-dash/src/dash.rs:8585-8585` `"/proc"`
- `crates/rigger-process/src/holders.rs:25-25` `"/proc"`
- `crates/rigger-process/src/holders.rs:72-72` `"/proc"`
- `crates/rigger-process/src/reap.rs:133-133` `"/proc"`
- `crates/rigger-process/src/reap.rs:215-215` `"/proc/{pid}/stat"`
- `crates/rigger-process/src/reap.rs:234-234` `"/proc/{pid}/status"`
- `crates/rigger-process/src/reap.rs:294-294` `"/proc"`
- `crates/rigger-process/src/reap.rs:371-371` `"/proc/{}/cwd"`
- `crates/rigger-worktree-git/src/worktree.rs:2948-2948` `"/proc"`
- `crates/rigger-worktree-git/src/worktree.rs:2952-2952` `"/proc/{pid}/cmdline"`
- `crates/rigger-worktree-git/src/worktree.rs:2976-2976` `"/proc/{}"`
- `src/cli/run.rs:3392-3392` `"/proc"`
- `src/cli/run.rs:3496-3496` `"/proc"`
- `tests/cli.rs:20390-20390` `"/proc"`
- `tests/cli.rs:24612-24612` `"/proc"`
- `tests/cli.rs:24720-24720` `"the holder pid {holder_pid} never reached the STOPPED (T) state in /proc"`
- `tests/cli.rs:24757-24757` `"/proc"`
- `tests/cli.rs:24808-24808` `"/proc"`
- `tests/cli.rs:24831-24831` `"/proc"`
- `tests/cli.rs:24856-24856` `"/proc"`
- `tests/cli.rs:24920-24920` `"/proc"`
- `tests/cli.rs:24946-24946` `"/proc"`
- `tests/cli.rs:25044-25044` `"/proc"`
- `tests/cli.rs:25082-25082` `"held_port_holder's message half and describe_held_port_if_confirmed's own return \
             must agree (scheduler-state letter normalized away since each call independently \
             re-reads /proc and can observe a state flap) - they are documented as sharing one \
             discovery"`
- `tests/cli.rs:25101-25101` `"/proc"`
- `tests/common/fixtures/host.rs:71-71` `"/proc/{pid}/stat has a pgrp field after comm"`
- `tests/duplication_catalog_contract_periphery.rs:101-101` `"/proc-path string literals"`
- `tests/kurrentdb_store_threads.rs:21-21` `"/proc/self/task"`
- `tests/mutation_runner_pdeathsig_periphery.rs:170-170` `"cat /proc/self/limits; echo \"MALLOC_ARENA_MAX=${MALLOC_ARENA_MAX-unset}\""`
- `tests/simplification_audit.rs:2791-2791` `"/proc/<pid>/stat or /proc/<pid>/status field-extraction functions"`
- `tests/simplification_audit.rs:2797-2797` `"/proc-path string literals"`
- `tests/simplification_audit.rs:2972-2972` `"/proc"`
- `tests/simplification_audit.rs:2973-2973` `"crates/rigger-process/src/reap.rs as the one /proc-reading module (dash.rs's own /proc readers already \
             duplicate reap.rs's field-after-the-comm's-closing-paren /proc/<pid>/stat parse - \
             see the report's worked example)"`
- `tests/simplification_audit.rs:3013-3013` `"/proc"`
- `tests/simplification_audit.rs:3240-3240` `"crates/rigger-process/src/reap.rs as the one /proc/<pid>/stat and /proc/<pid>/status parser, returning \
             whichever field each caller needs, so dash.rs::process_state and \
             reap.rs::pid_starttime/read_ppid stop each re-deriving the pid(comm)state... split"`
- `tests/simplification_audit.rs:3659-3659` `"Two real recall gaps surfaced this way and were closed by widening the mechanical \
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
- `tests/simplification_audit.rs:4083-4083` `"A second mutation authority for one domain: the one previously-known \
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
- `tests/simplification_audit.rs:5100-5100` `"Largest risk-reduction first is read as six tiers, ranked by the KIND of risk \
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
- `tests/simplification_audit.rs:5195-5195` `"#### 2. Retire the duplicate `/proc`-reading authority (`{}` + `{}`)\n\n"`
- `tests/simplification_audit.rs:5199-5199` `"- Scope: the production half is done - `crates/rigger-dash/src/dash.rs::process_state` and \
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
- `tests/simplification_audit.rs:8347-8347` `"fn state_of(pid: u32) -> Option<char> {\n    let stat = std::fs::read_to_string(format!(\"/proc/{pid}/stat\")).ok()?;\n    stat.rsplit_once(')')?.1.split_whitespace().next()?.chars().next()\n}\n"`
- `tests/simplification_audit.rs:8351-8351` `"fn starttime_of(pid: u32) -> Option<u64> {\n    let stat = std::fs::read_to_string(format!(\"/proc/{pid}/stat\")).ok()?;\n    stat.rsplit_once(')')?.1.split_whitespace().nth(19)?.parse().ok()\n}\n"`
- `tests/simplification_audit.rs:8355-8355` `"fn ppid_of(pid: u32) -> Option<u32> {\n    let stat = std::fs::read_to_string(format!(\"/proc/{pid}/stat\")).ok()?;\n    stat.rsplit_once(')')?.1.split_whitespace().nth(1)?.parse().ok()\n}\n"`
- `tests/simplification_audit.rs:8510-8510` `"fn a() {\n    let _ = std::fs::read_to_string(\"/proc/1/stat\");\n    let _ = \"hello\";\n}\n"`
- `tests/simplification_audit.rs:8511-8511` `"/proc"`
- `tests/simplification_audit.rs:8513-8513` `"/proc"`
- `tests/simplification_audit.rs:8578-8578` `"fn state_of(pid: u32) -> Option<char> {\n    let s = std::fs::read_to_string(format!(\"/proc/{pid}/stat\")).ok()?;\n    s.chars().next()\n}\nfn ppid_of(pid: u32) -> Option<u32> {\n    let s = std::fs::read_to_string(format!(\"/proc/{pid}/status\")).ok()?;\n    s.parse().ok()\n}\nfn unrelated() -> u32 {\n    1\n}\n"`
- `tests/simplification_audit.rs:8600-8600` `"the /proc reader sweep is catalogued"`
- `tests/simplification_audit.rs:8606-8606` `"the /proc reader sweep {:?} must carry the one stat parser and neither former \
             re-deriver, found: {names:?}"`

#### `dup-22a325f863b8` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-dash/src/dash.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-dash/src/dash.rs:4055-4060` `console_core_wasm_route_serves_the_embedded_artifact_as_application_wasm`
- `tests/simplification_audit.rs:7060-7067` `a_cfg_test_attribute_directly_on_an_impl_block_is_flagged_test`
- `tests/simplification_audit.rs:7187-7193` `a_cfg_test_pub_fn_is_still_flagged_test_pub_survives_between_attribute_and_keyword`

#### `dup-a2973e00918e` (near, 5 sites)

Proposed home: `a new shared module (sites span 4 files: crates/rigger-dash/src/dash.rs, tests/console_palette_periphery.rs, tests/projections_stay_local.rs, tests/store_resolution.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-dash/src/dash.rs:4814-4849` `the_page_layout_cannot_scroll_the_body_horizontally`
- `crates/rigger-dash/src/dash.rs:4860-4897` `the_landing_view_lists_instances_and_threads_the_attach_selector`
- `tests/console_palette_periphery.rs:400-415` `the_served_console_page_sends_the_scrub_position_to_palette_commands_when_not_live`
- `tests/projections_stay_local.rs:82-102` `the_graph_and_progress_projections_open_via_the_local_sqlite_constructors`
- `tests/store_resolution.rs:98-114` `the_single_resolver_exists_and_the_old_per_command_helper_is_retired`

#### `dup-9904088ec60d` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-dash/src/dash.rs, tests/common/fixtures/graph.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-dash/src/dash.rs:9090-9092` `layer_of`
- `tests/common/fixtures/graph.rs:236-238` `call_layer`

#### `dup-0e7d0c54553e` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-dash/src/dash.rs, crates/rigger-grounder/src/grounder/workflowdef.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-dash/src/dash.rs:9093-9095` `ids`
- `crates/rigger-grounder/src/grounder/workflowdef.rs:123-128` `light_reviewers_of`

#### `dup-e79afb3298c3` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-dash/src/dash.rs, tests/metadata_card_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-dash/src/dash.rs:9701-9725` `an_absent_explain_leaves_the_graph_route_unchanged`
- `crates/rigger-dash/src/dash.rs:9963-9983` `a_cluster_drill_carries_no_memory_field`
- `tests/metadata_card_periphery.rs:349-366` `the_served_route_degrades_gracefully_for_an_unknown_card_subject`

#### `dup-b3200e581798` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-dash/src/dash.rs, tests/metadata_card_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-dash/src/dash.rs:10009-10059` `card_graph`
- `tests/metadata_card_periphery.rs:49-100` `fixture_graph`

#### `dup-fae663db78c7` (near, 4 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-dash/src/dash.rs, tests/metadata_card_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-dash/src/dash.rs:10338-10380` `the_card_route_serves_a_known_subjects_card_and_null_for_an_unknown_one`
- `tests/metadata_card_periphery.rs:119-166` `the_served_route_carries_a_code_subjects_card`
- `tests/metadata_card_periphery.rs:174-200` `the_served_route_omits_community_and_line_keys_for_a_membership_less_entity`
- `tests/metadata_card_periphery.rs:373-399` `the_card_param_takes_precedence_over_a_stray_seed_or_lens_param`

#### `dup-17929143626a` (semantic, 3 sites)

Proposed home: `one shared `current_run_id` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-dash/src/mcpserver.rs:543-545` `current_run_id`
- `crates/rigger-domain/src/run.rs:208-210` `current_run_id`
- `tests/halted_spawn_wip_recovery_periphery.rs:586-589` `current_run_id`

#### `dup-468c9f4fc26f` (semantic, 2 sites)

Proposed home: `one shared `spawn_request` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-domain/src/agent.rs:178-207` `spawn_request`
- `tests/common/mod.rs:480-494` `spawn_request`

#### `dup-2a5626cb9627` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-domain/src/agent.rs, crates/rigger-domain/src/spec.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/agent.rs:297-312` `as_str`
- `crates/rigger-domain/src/spec.rs:167-173` `name`

#### `dup-f6925ead8f1a` (near, 2 sites)

Proposed home: `canary::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/canary.rs:104-119` `to_event`
- `crates/rigger-domain/src/canary.rs:191-200` `to_event`

#### `dup-0a701eb6cf8d` (exact, 6 sites)

Proposed home: `a new shared module (sites span 5 files: crates/rigger-domain/src/community.rs, crates/rigger-domain/src/eventstore.rs, crates/rigger-domain/src/failure.rs, crates/rigger-domain/src/metrics.rs, crates/rigger-driver/src/reaped_child.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/community.rs:243-245` `len`
- `crates/rigger-domain/src/community.rs:262-264` `is_empty`
- `crates/rigger-domain/src/eventstore.rs:254-256` `handed`
- `crates/rigger-domain/src/failure.rs:227-229` `is_empty`
- `crates/rigger-domain/src/metrics.rs:573-575` `adversary_precision`
- `crates/rigger-driver/src/reaped_child.rs:42-44` `id`

#### `dup-a9ed3e0e9d47` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: crates/rigger-domain/src/community.rs, crates/rigger-domain/src/eventstore.rs, crates/rigger-domain/src/failure.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/community.rs:249-251` `nodes`
- `crates/rigger-domain/src/eventstore.rs:489-491` `types`
- `crates/rigger-domain/src/failure.rs:232-234` `rules`

#### `dup-582bf6d9907e` (near, 4 sites)

Proposed home: `a new shared module (sites span 3 files: crates/rigger-domain/src/concepts.rs, crates/rigger-domain/src/ingest.rs, tests/common/source_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/concepts.rs:74-76` `is_intent_doc`
- `crates/rigger-domain/src/concepts.rs:80-82` `is_label_doc`
- `crates/rigger-domain/src/ingest.rs:48-50` `is_derived_index_type`
- `tests/common/source_audit.rs:435-437` `is_keyword`

#### `dup-53e62db783ac` (near, 4 sites)

Proposed home: `a new shared module (sites span 4 files: crates/rigger-domain/src/config.rs, crates/rigger-domain/src/failure.rs, crates/rigger-driver/src/liveness.rs, src/cli/mod.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/config.rs:381-383` `is_empty`
- `crates/rigger-domain/src/failure.rs:170-172` `is_any`
- `crates/rigger-driver/src/liveness.rs:649-651` `is_empty`
- `src/cli/mod.rs:3330-3335` `is_empty`

#### `dup-5f6fcac3a66f` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-domain/src/config.rs, crates/rigger-domain/src/eventstore.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/config.rs:386-388` `depth`
- `crates/rigger-domain/src/eventstore.rs:421-423` `facts`

#### `dup-af8b91bc2b2a` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: crates/rigger-domain/src/config.rs, tests/common/source_audit.rs, tests/no_os_kill_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/config.rs:1471-1473` `is_word_byte`
- `tests/common/source_audit.rs:198-200` `is_ident_char`
- `tests/no_os_kill_audit.rs:59-61` `is_word_char`

#### `dup-08ca12f5868a` (near, 2 sites)

Proposed home: `docs::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/docs.rs:94-112` `render_using_rigger_skill`
- `crates/rigger-domain/src/docs.rs:116-127` `render_handbook_discipline`

#### `dup-0c6e804c2770` (near, 4 sites)

Proposed home: `docs::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/docs.rs:919-947` `render_build_graph_skill`
- `crates/rigger-domain/src/docs.rs:952-979` `render_reindex_skill`
- `crates/rigger-domain/src/docs.rs:984-1022` `render_resume_a_run_skill`
- `crates/rigger-domain/src/docs.rs:1151-1201` `render_restore_the_dash_skill`

#### `dup-3d5ea11431d3` (near, 2 sites)

Proposed home: `docs::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/docs.rs:1029-1064` `render_handle_an_escalation_skill`
- `crates/rigger-domain/src/docs.rs:1210-1259` `render_diagnose_churn_skill`

#### `dup-bb7aa56f67d8` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-domain/src/eventstore.rs, crates/rigger-domain/src/spawn.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/eventstore.rs:167-170` `with_valid_from`
- `crates/rigger-domain/src/spawn.rs:615-618` `with_meta`

#### `dup-cfb679088c89` (exact, 2 sites)

Proposed home: `eventstore::content_identity`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/eventstore.rs:415-418` `with_facts`
- `crates/rigger-domain/src/eventstore.rs:431-434` `with_key_parts`

#### `dup-57c1e8124510` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-domain/src/eventstore.rs, crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/eventstore.rs:484-486` `meta_key`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:268-270` `path`

#### `dup-9a07b087e8f1` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: crates/rigger-domain/src/failure.rs, crates/rigger-domain/src/gate.rs, crates/rigger-domain/src/ledger.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/failure.rs:45-51` `as_str`
- `crates/rigger-domain/src/gate.rs:97-103` `as_str`
- `crates/rigger-domain/src/ledger.rs:47-59` `as_str`

#### `dup-3b3a41e03376` (exact, 3 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-domain/src/failure.rs, crates/rigger-domain/src/gate.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/failure.rs:67-69` `reruns`
- `crates/rigger-domain/src/failure.rs:76-78` `demotes_on_persistent_failure`
- `crates/rigger-domain/src/gate.rs:73-75` `runs_inline`

#### `dup-e9220ec4784e` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-domain/src/ingest.rs, tests/published_content_key_split_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/ingest.rs:92-95` `derived_key_parts`
- `tests/published_content_key_split_periphery.rs:40-43` `split`

#### `dup-64eda8b7ffd5` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-domain/src/instructions.rs, crates/rigger-gates-shell/src/gate.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/instructions.rs:92-97` `op`
- `crates/rigger-gates-shell/src/gate.rs:1688-1693` `unavailable`

#### `dup-557d6899b868` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-domain/src/metrics.rs, tests/common/fixtures/events.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/metrics.rs:296-298` `total`
- `tests/common/fixtures/events.rs:818-820` `cost`

#### `dup-6ffa6e5d1f83` (exact, 7 sites)

Proposed home: `metrics::support (consolidate these 7 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/metrics.rs:473-475` `survival`
- `crates/rigger-domain/src/metrics.rs:567-569` `lens_overlap_rate`
- `crates/rigger-domain/src/metrics.rs:581-583` `first_pass_yield`
- `crates/rigger-domain/src/metrics.rs:587-589` `escalation_rate`
- `crates/rigger-domain/src/metrics.rs:1210-1212` `rate`
- `crates/rigger-domain/src/metrics.rs:1275-1277` `adjudicator_accuracy`
- `crates/rigger-domain/src/metrics.rs:1281-1283` `stability_rate`

#### `dup-e7252bf71441` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-domain/src/metrics.rs, crates/rigger-store-sqlite/src/eventstore/sqlite.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/metrics.rs:494-500` `cost_per_upheld`
- `crates/rigger-store-sqlite/src/eventstore/sqlite.rs:886-892` `factor`

#### `dup-c16dbfd88b00` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-domain/src/metrics.rs, crates/rigger-domain/src/spawn.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/metrics.rs:1446-1448` `changed`
- `crates/rigger-domain/src/spawn.rs:621-623` `is_error`

#### `dup-e7182e4e126f` (exact, 3 sites)

Proposed home: `metrics::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/metrics.rs:1524-1529` `started`
- `crates/rigger-domain/src/metrics.rs:1531-1536` `status`
- `crates/rigger-domain/src/metrics.rs:1563-1568` `artifact_verdict`

#### `dup-df893b1b93d7` (near, 6 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-domain/src/metrics.rs, crates/rigger-domain/src/run.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/metrics.rs:1538-1543` `failed`
- `crates/rigger-domain/src/metrics.rs:1545-1550` `integrated`
- `crates/rigger-domain/src/metrics.rs:1552-1554` `escalated`
- `crates/rigger-domain/src/run.rs:635-637` `decision`
- `crates/rigger-domain/src/run.rs:638-640` `finding`
- `crates/rigger-domain/src/run.rs:641-643` `lesson`

#### `dup-d07779dc106d` (near, 2 sites)

Proposed home: `metrics::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/metrics.rs:2417-2440` `finding_survival_is_upheld_over_raised_per_actor`
- `crates/rigger-domain/src/metrics.rs:2512-2540` `cost_per_upheld_finding_is_spawns_over_upheld_per_tier`

#### `dup-6a304442d31e` (near, 2 sites)

Proposed home: `metrics::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/metrics.rs:2961-2980` `project_canary_counts_correctly_rejected_items_with_empty_attribution`
- `crates/rigger-domain/src/metrics.rs:2989-3007` `project_canary_counts_controls_and_false_positives`

#### `dup-4b3a37bb0a60` (exact, 2 sites)

Proposed home: `spawn::adjudication`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/spawn.rs:383-385` `is_infra_fault`
- `crates/rigger-domain/src/spawn.rs:389-391` `is_spec_ambiguity`

#### `dup-2b572fcabbd4` (semantic, 2 sites)

Proposed home: `one shared `liveness_fault` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-domain/src/spawn.rs:606-612` `liveness_fault`
- `tests/dash_run_tree_spine.rs:81-85` `liveness_fault`

#### `dup-e3c162b7bd56` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: crates/rigger-domain/src/spec.rs, crates/rigger-grounder/src/grounder/design/extract.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/spec.rs:546-553` `starts_new_element`
- `crates/rigger-grounder/src/grounder/design/extract.rs:391-394` `is_markdown`
- `tests/simplification_audit.rs:2860-2867` `looks_error_shaping`

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
- `tests/simplification_audit.rs:7007-7011` `a_trait_method_signature_without_a_body_is_not_recorded`

#### `dup-9f14620abab1` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-domain/src/spec.rs, crates/rigger-driver/src/liveness.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/spec.rs:2522-2524` `heading_level_rejects_more_than_six_hashes`
- `crates/rigger-driver/src/liveness.rs:883-896` `marker_filename_is_none_only_for_a_truly_empty_input_so_a_join_can_never_be_a_no_op`

#### `dup-cc1f48bd7d99` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-domain/src/spec.rs, tests/common/cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/spec.rs:2643-2645` `f11`
- `tests/common/cli.rs:452-454` `graph_build_line`

#### `dup-d340e212490f` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-domain/src/watch.rs, crates/rigger-driver/src/driver/workflow.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/watch.rs:672-674` `new`
- `crates/rigger-driver/src/driver/workflow.rs:84-86` `new`

#### `dup-161c827c2a70` (near, 2 sites)

Proposed home: `claude_code::failing_read_store`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-driver/src/driver/claude_code.rs:1357-1364` `read_stream`
- `crates/rigger-driver/src/driver/claude_code.rs:1398-1405` `read_stream_typed`

#### `dup-4979c76ee36f` (near, 2 sites)

Proposed home: `claude_code::failing_read_store`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-driver/src/driver/claude_code.rs:1391-1397` `last_position`
- `crates/rigger-driver/src/driver/claude_code.rs:1425-1431` `latest_in_group`

#### `dup-d082d63b45dc` (near, 2 sites)

Proposed home: `claude_code::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-driver/src/driver/claude_code.rs:1523-1543` `build_args_mcp_config_names_the_spawn_bound_server`
- `crates/rigger-driver/src/driver/claude_code.rs:1546-1563` `build_args_mcp_config_uses_the_configured_rigger_bin`

#### `dup-bada7e276fc1` (semantic, 2 sites)

Proposed home: `mod::launch_session - one canonical constructor, the rest thin variants over it (or a builder), rather than each re-listing every field`

mandatory sweep: parallel constructor functions - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-driver/src/driver/mod.rs:131-140` `of`
- `crates/rigger-driver/src/driver/mod.rs:143-148` `fresh`

#### `dup-282069e7109f` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: crates/rigger-driver/src/driver/replay.rs, tests/common/fixtures/config.rs, tests/model_pinning_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-driver/src/driver/replay.rs:281-288` `sonnet_agent`
- `tests/common/fixtures/config.rs:173-183` `fan_out_stage`
- `tests/model_pinning_periphery.rs:62-69` `agent`

#### `dup-7ce260b96af1` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: crates/rigger-driver/src/driver/replay.rs, tests/claude_code_stream_periphery.rs, tests/common/fixtures/conductor.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-driver/src/driver/replay.rs:294-301` `opts_for`
- `tests/claude_code_stream_periphery.rs:143-149` `opts`
- `tests/common/fixtures/conductor.rs:19-26` `implementer_opts`

#### `dup-b43ee3b4b5bb` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-driver/src/driver/replay.rs, crates/rigger-driver/src/liveness.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-driver/src/driver/replay.rs:346-354` `spawn_scratch_path_is_none_rather_than_relative_for_an_empty_scratch_root`
- `crates/rigger-driver/src/liveness.rs:1007-1015` `marker_path_is_none_rather_than_relative_for_an_empty_scratch_root`

#### `dup-8c5011c59ed1` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-driver/src/driver/replay.rs, tests/common/fixtures/config.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-driver/src/driver/replay.rs:933-942` `stage`
- `tests/common/fixtures/config.rs:14-20` `agent_with_prompt`

#### `dup-4cf25824ad00` (near, 2 sites)

Proposed home: `replay::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-driver/src/driver/replay.rs:1697-1745` `an_emit_only_approve_gating_persona_hard_errors_on_the_replay_driver`
- `crates/rigger-driver/src/driver/replay.rs:1820-1869` `a_parked_unanswered_sibling_does_not_suppress_this_units_own_approve_backstop`

#### `dup-02b6d2bbb967` (semantic, 2 sites)

Proposed home: `one shared `install_status_line` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-driver/src/hooks.rs:255-272` `install_status_line`
- `src/cli/setup.rs:1199-1203` `install_status_line`

#### `dup-a5bfac770233` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-driver/src/liveness.rs, src/cli/setup.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-driver/src/liveness.rs:924-938` `marker_filename_is_injective_so_two_ids_that_collided_under_a_prior_placeholder_scheme_no_longer_do`
- `src/cli/setup.rs:2396-2410` `normalize_origin_url_separates_distinct_repos_and_lowercases_only_the_host`

#### `dup-1f66a86debc4` (semantic, 68 sites)

Proposed home: `one sqlite-connection-opening adapter function every caller is injected with`

mandatory sweep: sqlite Connection::open call sites - 68 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:1338-1338` `Connection::open_with_flags`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:1355-1355` `Connection::open`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:1358-1358` `Connection::open`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:4591-4591` `Connection::open`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:4609-4609` `Connection::open`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:4798-4798` `Connection::open`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:5137-5137` `Connection::open`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:9738-9738` `Connection::open`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:10507-10507` `Connection::open`
- `crates/rigger-store-sqlite/src/eventstore/sqlite.rs:1562-1562` `Connection::open`
- `crates/rigger-store-sqlite/src/eventstore/sqlite.rs:2083-2083` `Connection::open`
- `crates/rigger-store-sqlite/src/eventstore/sqlite.rs:2266-2266` `Connection::open`
- `crates/rigger-store-sqlite/src/eventstore/sqlite.rs:2418-2418` `Connection::open_with_flags`
- `crates/rigger-store-sqlite/src/run_store.rs:417-417` `Connection::open`
- `crates/rigger-store-sqlite/src/sqlite.rs:13-13` `Connection::open`
- `src/cli/mod.rs:5173-5173` `Connection::open`
- `src/cli/mod.rs:12141-12141` `Connection::open`
- `tests/cli.rs:641-641` `Connection::open`
- `tests/cli.rs:744-744` `Connection::open`
- `tests/cli.rs:847-847` `Connection::open`
- `tests/cli.rs:890-890` `Connection::open`
- `tests/cli.rs:9058-9058` `Connection::open`
- `tests/common/cli.rs:342-342` `Connection::open`
- `tests/common/cli.rs:355-355` `Connection::open`
- `tests/common/cli.rs:413-413` `Connection::open`
- `tests/common/cli.rs:423-423` `Connection::open`
- `tests/common/cli.rs:882-882` `Connection::open`
- `tests/common/fixtures/sqlite.rs:8-8` `Connection::open`
- `tests/common/fixtures/sqlite.rs:22-22` `Connection::open`
- `tests/compaction_generations_periphery.rs:105-105` `Connection::open`
- `tests/compaction_generations_periphery.rs:2209-2209` `Connection::open`
- `tests/compaction_generations_periphery.rs:2770-2770` `Connection::open`
- `tests/compaction_generations_periphery.rs:2868-2868` `Connection::open`
- `tests/compaction_generations_periphery.rs:2884-2884` `Connection::open`
- `tests/compaction_generations_periphery.rs:3012-3012` `Connection::open`
- `tests/compaction_generations_periphery.rs:3129-3129` `Connection::open`
- `tests/compaction_generations_periphery.rs:3968-3968` `Connection::open`
- `tests/compaction_generations_periphery.rs:4028-4028` `Connection::open`
- `tests/compaction_generations_periphery.rs:4244-4244` `Connection::open`
- `tests/compaction_generations_periphery.rs:4537-4537` `Connection::open`
- `tests/compaction_generations_periphery.rs:4797-4797` `Connection::open`
- `tests/compaction_generations_periphery.rs:5939-5939` `Connection::open`
- `tests/compaction_generations_periphery.rs:6285-6285` `Connection::open`
- `tests/compaction_generations_periphery.rs:6601-6601` `Connection::open`
- `tests/graph_additive_indexes_persist.rs:69-69` `Connection::open`
- `tests/graph_additive_indexes_persist.rs:165-165` `Connection::open`
- `tests/graph_additive_indexes_persist.rs:189-189` `Connection::open`
- `tests/group_lookup_periphery.rs:216-216` `Connection::open`
- `tests/heartbeat_write_read_agree_periphery.rs:126-126` `Connection::open`
- `tests/one_shot_reads_periphery.rs:293-293` `Connection::open`
- `tests/reset_derived_compaction.rs:75-75` `Connection::open`
- `tests/reset_derived_compaction.rs:230-230` `Connection::open`
- `tests/reset_derived_compaction.rs:496-496` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:112-112` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:542-542` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:1286-1286` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:2537-2537` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:2742-2742` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:3530-3530` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:4170-4170` `Connection::open`
- `tests/reset_derived_live_writer_guard_periphery.rs:69-69` `Connection::open`
- `tests/reset_menu.rs:60-60` `Connection::open`
- `tests/reset_menu.rs:64-64` `Connection::open`
- `tests/reset_menu_identity_migration_periphery.rs:98-98` `Connection::open`
- `tests/run_boundary_lookup_periphery.rs:41-41` `Connection::open`
- `tests/run_boundary_lookup_periphery.rs:80-80` `Connection::open`
- `tests/store_append_order_periphery.rs:58-58` `Connection::open`
- `tests/watchdog_cli_periphery.rs:171-171` `Connection::open`

#### `dup-c650c3e62b50` (exact, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:3264-3276` `calls_out`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:3332-3344` `callers_direct`

#### `dup-1cee568104ee` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:4207-4220` `subgraph_finds_the_governing_decision`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:10860-10880` `recording_proof_never_wipes_the_entitys_own_name_kind_and_line_attrs`

#### `dup-49b57537de27` (near, 8 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs, tests/code_ingest_events.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:6159-6166` `apply_edge_inferred`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:6200-6207` `apply_edge_inferred_evidence`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:7050-7057` `apply_doc_concept`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:7160-7167` `apply_doc_link`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:7490-7497` `apply_batch_ref`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:9099-9109` `apply_unit_integrated`
- `tests/code_ingest_events.rs:841-848` `apply_ref_json`
- `tests/code_ingest_events.rs:993-1000` `apply_ref_fresh`

#### `dup-b5bd50de3e2a` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:6175-6194` `apply_code_entity_partial`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:6236-6254` `apply_community`

#### `dup-84342f753367` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:7170-7293` `design_intent_link_events_fold_into_the_five_design_intent_edges`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:7296-7414` `workflow_definition_events_fold_into_stage_gate_agent_nodes_with_needs_runs_reviews_edges`

#### `dup-0b2669324614` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:8832-8865` `decision_fold_projects_no_agent_node_or_decided_edge`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:8919-8947` `review_finding_projects_no_raised_edge_even_with_an_event_actor`

#### `dup-c76c4d91d8a8` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:9642-9644` `edge_projects`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:10675-10683` `index_names`

#### `dup-554a83a84ac8` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:10411-10452` `the_cross_file_inferred_tier_is_order_independent`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:10455-10470` `the_definition_upgrade_never_demotes_a_same_file_extracted_reference`

#### `dup-3cee929a82ba` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs, src/cli/hygiene.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:10584-10586` `edge_desc`
- `src/cli/hygiene.rs:239-245` `runs_menu_line`
- `src/cli/hygiene.rs:940-945` `pruned_line`

#### `dup-233d5e6363a7` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:10902-10933` `an_unresolvable_test_reference_is_staged_and_reconciled_once_its_definition_later_folds`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:11178-11209` `the_empty_boundary_sentinel_never_resolves_records_or_stages_anything_for_its_empty_name`

#### `dup-a34b5b595e15` (near, 2 sites)

Proposed home: `events::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-grounder/src/grounder/design/events.rs:20-37` `concept_events`
- `crates/rigger-grounder/src/grounder/design/events.rs:45-62` `link_events`

#### `dup-f2eee0c78cd6` (semantic, 3 sites)

Proposed home: `one shared `project_batches` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-grounder/src/grounder/design/events.rs:97-99` `project_batches`
- `crates/rigger-grounder/src/grounder/symbols/events.rs:59-61` `project_batches`
- `crates/rigger-grounder/src/grounder/workflowdef.rs:233-240` `project_batches`

#### `dup-4b942d7302a6` (near, 2 sites)

Proposed home: `model::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-grounder/src/grounder/design/model.rs:30-37` `node_kind`
- `crates/rigger-grounder/src/grounder/design/model.rs:81-89` `rel`

#### `dup-c1d968c10781` (semantic, 2 sites)

Proposed home: `one shared `extract_events` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-grounder/src/grounder/symbols/events.rs:208-304` `extract_events`
- `crates/rigger-grounder/src/grounder/workflowdef.rs:169-199` `extract_events`

#### `dup-207b5aa153cb` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-grounder/src/grounder/symbols/events.rs, crates/rigger-store-sqlite/src/eventstore/sqlite.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-grounder/src/grounder/symbols/events.rs:775-786` `kind_str`
- `crates/rigger-grounder/src/grounder/symbols/events.rs:790-799` `lang_str`
- `crates/rigger-store-sqlite/src/eventstore/sqlite.rs:1502-1507` `direction_sql`

#### `dup-ff3bcd827c2f` (semantic, 3 sites)

Proposed home: `crates/rigger-grounder/src/grounder/symbols/extract.rs::extract as the ONE function that touches source parsing (already its own module doc's claim, architecture 5.5.3) - this file's own scan_file/tokenize are ad hoc scanners for the identical job and should route through an injected-grammar extractor rather than re-deriving structure by hand`

mandatory sweep: bespoke source-text lexer/scanner functions duplicating the canonical tree-sitter extractor - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-grounder/src/grounder/symbols/extract.rs:34-201` `extract`
- `tests/common/source_audit.rs:448-554` `tokenize`
- `tests/simplification_audit.rs:212-214` `scan_file`

#### `dup-6741e1f3023b` (semantic, 2 sites)

Proposed home: `one shared `changed_files` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-grounder/src/grounder/symbols/grounder.rs:75-98` `changed_files`
- `crates/rigger-worktree-git/src/worktree.rs:608-611` `changed_files`

#### `dup-9408447d0007` (near, 2 sites)

Proposed home: `grounder::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-grounder/src/grounder/symbols/grounder.rs:161-163` `commonness_map`
- `crates/rigger-grounder/src/grounder/symbols/grounder.rs:178-180` `ambiguity_map`

#### `dup-d36198b3982d` (near, 2 sites)

Proposed home: `grounder::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-grounder/src/grounder/symbols/grounder.rs:1188-1199` `a_reference_ranks_below_a_definition_of_the_same_name`
- `crates/rigger-grounder/src/grounder/symbols/grounder.rs:1438-1452` `ground_ranks_an_exact_name_match_above_a_name_that_merely_contains_the_token`

#### `dup-56485f8d4262` (near, 3 sites)

Proposed home: `grounder::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-grounder/src/grounder/symbols/grounder.rs:1355-1401` `a_concurrent_reindex_from_a_second_grounder_is_not_clobbered`
- `crates/rigger-grounder/src/grounder/symbols/grounder.rs:1404-1430` `reindex_replaces_only_a_changed_files_symbols`
- `crates/rigger-grounder/src/grounder/symbols/grounder.rs:1729-1756` `reindex_over_a_deleted_file_stops_grounding_it`

#### `dup-415d2df07b98` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-grounder/src/grounder/symbols/store.rs, crates/rigger-grounder/src/grounder/workflowdef.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-grounder/src/grounder/symbols/store.rs:257-261` `load_is_none_on_a_cold_start`
- `crates/rigger-grounder/src/grounder/workflowdef.rs:599-603` `project_events_on_a_missing_workflow_yields_nothing_never_a_crash`

#### `dup-3eb93069b8f0` (semantic, 2 sites)

Proposed home: `one shared `project_events` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-grounder/src/grounder/workflowdef.rs:207-213` `project_events`
- `tests/common/mod.rs:471-476` `project_events`

#### `dup-e10b6988960c` (exact, 2 sites)

Proposed home: `ingest::folding_store`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-grounder/src/ingest.rs:266-268` `last_position`
- `crates/rigger-grounder/src/ingest.rs:298-300` `latest_in_group`

#### `dup-96daf6c3e468` (semantic, 9 sites)

Proposed home: `crates/rigger-process/src/reap.rs as the one /proc/<pid>/stat and /proc/<pid>/status parser, returning whichever field each caller needs, so dash.rs::process_state and reap.rs::pid_starttime/read_ppid stop each re-deriving the pid(comm)state... split`

mandatory sweep: /proc/<pid>/stat or /proc/<pid>/status field-extraction functions - 9 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-process/src/reap.rs:214-218` `stat_field_after_comm`
- `crates/rigger-process/src/reap.rs:233-239` `read_ppid`
- `tests/common/fixtures/host.rs:69-74` `pgid_of`
- `tests/simplification_audit.rs:2962-2993` `build_sweep_clusters`
- `tests/simplification_audit.rs:3235-3256` `build_extra_semantic_clusters`
- `tests/simplification_audit.rs:3586-3703` `render_adversarial_sample`
- `tests/simplification_audit.rs:5056-5610` `render_section_6`
- `tests/simplification_audit.rs:8340-8363` `three_near_but_not_identical_functions_cluster_as_near_with_every_site_and_one_home`
- `tests/simplification_audit.rs:8509-8514` `proc_literal_sweep_finds_a_proc_path_string_and_ignores_an_unrelated_one`

#### `dup-a8049539272a` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-process/src/reap.rs, src/cli/validate.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-process/src/reap.rs:872-879` `processes_rooted_under_is_a_graceful_no_op_when_the_base_is_absent`
- `src/cli/validate.rs:1788-1795` `leaked_process_advisories_is_a_graceful_no_op_when_the_scratch_root_is_absent`

#### `dup-183455104b30` (near, 3 sites)

Proposed home: `contract::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-store-sqlite/src/eventstore/contract.rs:834-854` `append_assigns_revisions`
- `crates/rigger-store-sqlite/src/eventstore/contract.rs:963-1008` `backward_stream_read_reverses_set`
- `crates/rigger-store-sqlite/src/eventstore/contract.rs:1012-1037` `forward_stream_read_honors_nonzero_from`

#### `dup-4a57ceb2e317` (semantic, 2 sites)

Proposed home: `one shared `read_forward` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-store-sqlite/src/eventstore/kurrentdb.rs:569-576` `read_forward`
- `crates/rigger-store-sqlite/src/eventstore/sqlite.rs:582-599` `read_forward`

#### `dup-8bed086eddda` (exact, 2 sites)

Proposed home: `namespace::namespaced`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-store-sqlite/src/eventstore/namespace.rs:103-111` `read_stream`
- `crates/rigger-store-sqlite/src/eventstore/namespace.rs:137-147` `read_stream_typed`

#### `dup-f5125f873960` (exact, 2 sites)

Proposed home: `namespace::namespaced`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-store-sqlite/src/eventstore/namespace.rs:133-135` `last_position`
- `crates/rigger-store-sqlite/src/eventstore/namespace.rs:172-174` `latest_in_group`

#### `dup-b4990598bfd7` (exact, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-store-sqlite/src/eventstore/sqlite.rs:58-64` `group_index_sql`
- `crates/rigger-store-sqlite/src/eventstore/sqlite.rs:68-75` `latest_in_group_sql`

#### `dup-831910355599` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-store-sqlite/src/eventstore/sqlite.rs:2158-2183` `measure_derived_duplication_on_a_clean_log_reports_no_duplication`
- `crates/rigger-store-sqlite/src/eventstore/sqlite.rs:2186-2237` `measure_derived_duplication_treats_the_same_key_under_two_covered_types_as_two_distinct_subjects`

#### `dup-5cdeb5b1d1dd` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-store-sqlite/src/lockfile.rs, crates/rigger-worktree-git/src/worktree.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-store-sqlite/src/lockfile.rs:52-54` `drop`
- `crates/rigger-worktree-git/src/worktree.rs:1672-1674` `drop`

#### `dup-09fd5c38b65b` (near, 2 sites)

Proposed home: `progress_store::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-store-sqlite/src/progress_store.rs:39-53` `record_launch`
- `crates/rigger-store-sqlite/src/progress_store.rs:59-70` `record_stop_failure`

#### `dup-10504155864a` (near, 2 sites)

Proposed home: `run_store::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-store-sqlite/src/run_store.rs:628-642` `ensure_started_adopts_the_same_criteria_run_without_re_minting`
- `crates/rigger-store-sqlite/src/run_store.rs:697-712` `ensure_started_mints_a_new_run_when_the_criteria_change`

#### `dup-90067fabd6a8` (near, 2 sites)

Proposed home: `worktree::worktree`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-worktree-git/src/worktree.rs:632-634` `commit`
- `crates/rigger-worktree-git/src/worktree.rs:650-652` `commit_checkpoint`

#### `dup-4efef9d45e3e` (exact, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-worktree-git/src/worktree.rs:2574-2582` `scratch_root`
- `crates/rigger-worktree-git/src/worktree.rs:2673-2681` `scratch_root_path`

#### `dup-e08d5a3a5d94` (exact, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-worktree-git/src/worktree.rs:2771-2774` `scratch_root_from_env`
- `crates/rigger-worktree-git/src/worktree.rs:2778-2781` `scratch_root_path_from_env`

#### `dup-5a3cb86b6dd7` (near, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-worktree-git/src/worktree.rs:3253-3273` `integrate_lands_work_in_the_repo`
- `crates/rigger-worktree-git/src/worktree.rs:5165-5188` `integrate_lands_a_pre_committed_artifact_unchanged`

#### `dup-4e9eb933164b` (near, 2 sites)

Proposed home: `graph::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/cli/graph.rs:654-678` `cmd_graph_communities`
- `src/cli/graph.rs:755-779` `cmd_graph_concepts`

#### `dup-6b5ddf643eb6` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/cli/guard.rs, src/cli/validate.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/cli/guard.rs:1279-1281` `parse_guard_write_roots_requires_at_least_one`
- `src/cli/validate.rs:1988-1990` `order_signature_advisories_is_empty_when_no_signatures_are_given`

#### `dup-4a813209194e` (near, 2 sites)

Proposed home: `hygiene::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/cli/hygiene.rs:1943-1963` `reset_modes_parses_force_live_alongside_derived_rejects_duplicates_and_never_implies_a_mode`
- `src/cli/hygiene.rs:2017-2038` `reset_modes_parses_build_cache_alone_and_composed_and_rejects_duplicates`

#### `dup-e9f78fd7efcc` (exact, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/cli/mod.rs:868-870` `project_identity`
- `src/cli/mod.rs:5088-5090` `git_repo`

#### `dup-b7ec46dde247` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/cli/mod.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/cli/mod.rs:1302-1304` `find_store_dir_from`
- `tests/simplification_audit.rs:863-865` `scan_target_files`

#### `dup-7b4605d9ccc9` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/cli/mod.rs, tests/reset_build_cache_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/cli/mod.rs:3709-3728` `dir_size_bytes`
- `tests/reset_build_cache_periphery.rs:55-72` `dir_bytes`

#### `dup-41d8b1c33d44` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: src/cli/mod.rs, tests/cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/cli/mod.rs:5211-5218` `spec_lint_next_step_names_rigger_validate_and_the_given_spec_path`
- `tests/cli.rs:4178-4201` `native_driver_fixpoint_requires_done_and_an_empty_in_flight_set`
- `tests/cli.rs:4209-4222` `native_driver_drains_in_flight_workers_before_a_loud_stop`

#### `dup-da72fb2d69f3` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/cli/mod.rs:7628-7683` `reclaim_orphan_scratch_spares_a_non_live_worktree_that_is_still_dirty`
- `src/cli/mod.rs:7686-7725` `reclaim_orphan_scratch_spares_only_a_declared_dirty_worktree`

#### `dup-5e94519ad1f0` (exact, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/cli/mod.rs:9983-9994` `the_step_schema_admits_the_attention_array`
- `src/cli/mod.rs:11300-11303` `no_runs_message_points_at_rigger_run`

#### `dup-178d86dd6b19` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/cli/mod.rs, tests/cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/cli/mod.rs:10306-10315` `step_courier_prompt`
- `tests/cli.rs:3238-3247` `cmd_step_source`

#### `dup-03a673792149` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/cli/mod.rs:11568-11577` `result_of_at_unrecorded_spawn_reads_as_unreported`
- `src/cli/mod.rs:11615-11634` `result_of_at_is_namespace_scoped`

#### `dup-bd75ff76a467` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/cli/setup.rs, src/main.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/cli/setup.rs:311-318` `print_scaffold_pointer`
- `src/main.rs:386-388` `usage`

#### `dup-dc79af9af1c6` (near, 2 sites)

Proposed home: `setup::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/cli/setup.rs:1147-1155` `install_lookup_hook`
- `src/cli/setup.rs:1199-1203` `install_status_line`

#### `dup-2cd309ca2b08` (near, 2 sites)

Proposed home: `validate::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/cli/validate.rs:1491-1512` `git_is_ancestor_decides_commit_order_in_a_real_repo`
- `src/cli/validate.rs:1515-1536` `git_commit_distance_counts_commits_ahead_in_a_real_repo`

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

- `tests/canary_false_positives_periphery.rs:61-128` `spawn`
- `tests/canary_unattributed_rejects_periphery.rs:60-115` `spawn`

#### `dup-d887cb6f63f7` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/canary_false_positives_periphery.rs, tests/canary_unattributed_rejects_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/canary_false_positives_periphery.rs:138-265` `run_canary_scores_false_positive_controls_and_project_canary_counts_them`
- `tests/canary_unattributed_rejects_periphery.rs:125-258` `run_canary_scores_an_unattributed_correct_reject_and_project_canary_counts_only_it`

#### `dup-71aadfa4568e` (near, 14 sites)

Proposed home: `a new shared module (sites span 3 files: tests/cause_wire_periphery.rs, tests/cli.rs, tests/escalation_resume_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cause_wire_periphery.rs:109-130` `a_legacy_causeless_unit_failed_event_survives_a_real_store_round_trip_and_renders_unknown`
- `tests/cause_wire_periphery.rs:137-168` `the_reject_recurrence_line_names_the_latest_of_several_recorded_causes_through_the_real_binary`
- `tests/cli.rs:8998-9035` `stats_reports_the_latest_run_by_default_and_all_for_the_aggregate`
- `tests/cli.rs:9290-9304` `stats_cli_reports_no_recorded_spawns_when_the_run_has_none`
- `tests/cli.rs:19620-19678` `release_ready_handoff_surfaces_on_status_for_a_done_run`
- `tests/cli.rs:19879-19919` `release_ready_is_silent_on_status_for_an_unfinished_run`
- `tests/cli.rs:19989-20021` `release_ready_pluralizes_the_unit_count_on_status_for_a_multi_unit_run`
- `tests/cli.rs:20065-20086` `release_ready_is_silent_on_status_for_a_spec_defective_run`
- `tests/escalation_resume_periphery.rs:85-108` `a_unit_resumed_event_seeded_directly_through_a_real_store_reaches_status_without_the_command`
- `tests/escalation_resume_periphery.rs:119-147` `a_legacy_shaped_unit_resumed_event_missing_both_optional_fields_survives_a_real_store_round_trip`
- `tests/escalation_resume_periphery.rs:235-265` `the_resumed_banner_survives_a_genuinely_in_flight_re_parked_attempt_not_yet_resolved`
- `tests/escalation_resume_periphery.rs:313-333` `resume_unit_rejects_an_unknown_unit_absent_from_the_run`
- `tests/escalation_resume_periphery.rs:340-363` `resume_unit_refuses_when_the_recorded_branch_was_never_created`
- `tests/escalation_resume_periphery.rs:370-392` `resume_unit_refuses_an_already_integrated_unit`

#### `dup-b2e626ed7495` (near, 33 sites)

Proposed home: `a new shared module (sites span 7 files: tests/cli.rs, tests/graph_show_periphery.rs, tests/reset_build_cache_periphery.rs, tests/reset_derived_live_writer_guard_periphery.rs, tests/reset_menu.rs, tests/statusline_command_periphery.rs, tests/watchdog_cli_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:1241-1258` `scratch_requires_exactly_one_spawn_id`
- `tests/cli.rs:1369-1380` `scratch_refuses_a_degenerate_empty_spawn_id`
- `tests/cli.rs:1612-1629` `result_prints_an_orphan_advisory_for_an_unrecorded_id`
- `tests/cli.rs:2370-2382` `graph_build_exits_clean_and_creates_the_store_in_both_lanes`
- `tests/cli.rs:3148-3175` `emit_rejects_bad_json_with_a_nonzero_exit`
- `tests/cli.rs:5143-5164` `resume_unit_defaults_to_granting_one_attempt_when_attempts_is_omitted`
- `tests/cli.rs:5167-5181` `resume_unit_refuses_an_unknown_unit`
- `tests/cli.rs:9865-9875` `run_accepts_a_base_flag`
- `tests/cli.rs:13114-13145` `init_reports_the_positive_summary_then_is_a_quiet_noop`
- `tests/cli.rs:13590-13610` `result_if_absent_records_when_the_spawn_is_unanswered`
- `tests/cli.rs:14143-14152` `stats_canary_on_a_project_with_no_canary_run_says_so`
- `tests/cli.rs:14308-14337` `canary_rejects_unknown_arguments_and_a_missing_corpus`
- `tests/cli.rs:14366-14380` `canary_accepts_a_jobs_flag_alongside_other_flags`
- `tests/cli.rs:14518-14553` `canary_rejects_a_malformed_or_unknown_tier_model_pin`
- `tests/cli.rs:16165-16177` `status_json_appends_no_dashboard_entry_when_none_was_ever_recorded`
- `tests/cli.rs:16287-16298` `status_prints_no_dashboard_line_when_none_was_ever_recorded`
- `tests/cli.rs:23945-23955` `prime_with_no_spec_path_never_mentions_the_spec_lint`
- `tests/cli.rs:23958-23975` `prime_given_a_spec_path_names_the_spec_lint_as_a_next_step`
- `tests/cli.rs:23978-24000` `prime_given_a_spec_path_names_the_spec_lint_alongside_recent_decisions`
- `tests/cli.rs:24366-24375` `workflow_with_no_spec_path_never_mentions_the_spec_lint`
- `tests/cli.rs:25496-25528` `mcp_rejects_a_malformed_spawn_flag_or_unexpected_arguments`
- `tests/cli.rs:27003-27023` `init_scaffolds_the_instructions_readme_and_names_it`
- `tests/graph_show_periphery.rs:96-113` `graph_show_and_around_with_no_store_print_the_not_found_note`
- `tests/reset_build_cache_periphery.rs:103-129` `reset_build_cache_is_idempotent_zero_report_on_a_project_that_never_built_anything`
- `tests/reset_build_cache_periphery.rs:167-177` `reset_build_cache_flag_is_registered_and_rejects_a_duplicate`
- `tests/reset_derived_live_writer_guard_periphery.rs:1720-1730` `reset_force_live_alone_is_refused_as_no_mode`
- `tests/reset_derived_live_writer_guard_periphery.rs:1736-1751` `the_derived_help_entry_documents_force_live_and_owns_the_risk`
- `tests/reset_menu.rs:79-96` `bare_reset_on_an_empty_store_exits_zero_and_reports_nothing_prunable`
- `tests/statusline_command_periphery.rs:90-100` `status_line_on_a_clean_run`
- `tests/statusline_command_periphery.rs:137-147` `status_line_and_json_are_mutually_exclusive`
- `tests/watchdog_cli_periphery.rs:145-155` `watch_once_on_a_freshly_initialized_store_reports_nothing_and_exits_cleanly`
- `tests/watchdog_cli_periphery.rs:236-250` `watch_refuses_a_project_with_no_rigger_store_at_all_through_the_real_binary`
- `tests/watchdog_cli_periphery.rs:257-270` `watch_rejects_an_unknown_flag_through_the_real_binary_with_a_nonzero_exit`

#### `dup-4cd3708b516f` (exact, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:1681-1683` `initialized_project`
- `tests/cli.rs:1702-1704` `initialized_git_project`

#### `dup-9da3f693c2b7` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/cli.rs, tests/worker_persona_label_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:3929-3985` `native_driver_scratch_policy_directs_the_worker_to_its_own_spawn_owned_container`
- `tests/worker_persona_label_periphery.rs:307-328` `workerlabel_is_actually_wired_into_runworkers_agent_call_label`

#### `dup-751f1b83af79` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: tests/cli.rs, tests/step_attention_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:7841-7874` `a_budget_halt_does_not_restamp_on_a_later_real_step_with_nothing_new`
- `tests/cli.rs:8316-8359` `step_attention_never_restamps_a_hung_unbounded_spawn_when_repo_less`
- `tests/step_attention_periphery.rs:176-265` `recurrence_and_stalled_frontier_survive_real_process_boundaries`

#### `dup-ea8a80f3da16` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/cli.rs, tests/courier_registry_refresh_boundary_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:8898-8901` `run_teardown_spares_run_level_scratch_at_a_manual_review_pause`
- `tests/courier_registry_refresh_boundary_periphery.rs:194-197` `an_ambient_kurrentdb_conn_never_leaks_into_a_boundary_courier`

#### `dup-edb646ecd5e8` (exact, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:11258-11260` `path_with_fake_sccache`
- `tests/cli.rs:18531-18533` `stage_rigger_shim`

#### `dup-84fea880d212` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:16586-16616` `docs_ships_three_verb_lookup_guidance_to_consumers`
- `tests/cli.rs:26797-26844` `docs_installs_the_operator_lookup_rule_text_into_the_shipped_skill_and_handbook`

#### `dup-21b08556e493` (exact, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:18954-18959` `stage_failing_docs_rigger_shim`
- `tests/cli.rs:18979-18996` `stage_stale_rigger_shim`

#### `dup-4fbc57c56116` (near, 2 sites)

Proposed home: `code_ingest_events::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_ingest_events.rs:1056-1097` `a_refs_only_files_re_extraction_supersedes_via_the_reference_batch_boundary`
- `tests/code_ingest_events.rs:1100-1154` `a_re_extraction_supersedes_only_its_own_files_edges_not_another_files_reference`

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

- `tests/common/cli.rs:45-49` `courier_project`
- `tests/common/cli.rs:145-149` `temp_rigger_project`
- `tests/common/cli.rs:153-157` `temp_store_project`
- `tests/courier_registry_refresh_boundary_periphery.rs:40-44` `courier_project_with_commit`

#### `dup-8dd44d643a5c` (semantic, 2 sites)

Proposed home: `one shared `write_workflow` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/common/cli.rs:720-727` `write_workflow`
- `tests/common/workflow_probe.rs:21-23` `write_workflow`

#### `dup-b58b0ff72b22` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: tests/common/cli.rs, tests/store_secrets.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/common/cli.rs:756-776` `assert_selected_server`
- `tests/common/cli.rs:780-796` `assert_selected_sqlite`
- `tests/store_secrets.rs:69-107` `assert_server_reached_and_credentials_redacted`

#### `dup-c82a46fd1fec` (exact, 2 sites)

Proposed home: `events::silent_store`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/common/fixtures/events.rs:169-176` `read_stream`
- `tests/common/fixtures/events.rs:198-205` `read_stream_typed`

#### `dup-3eb116c3c4c8` (exact, 2 sites)

Proposed home: `events::silent_store`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/common/fixtures/events.rs:195-197` `last_position`
- `tests/common/fixtures/events.rs:223-225` `latest_in_group`

#### `dup-033af83f765c` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/common/fixtures/events.rs, tests/store_content_identity_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/common/fixtures/events.rs:206-213` `read_stream_positions`
- `tests/store_content_identity_periphery.rs:248-255` `read_stream_positions`

#### `dup-6ef712aab260` (exact, 2 sites)

Proposed home: `events::read_counting_store`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/common/fixtures/events.rs:704-712` `last_position`
- `tests/common/fixtures/events.rs:771-779` `latest_in_group`

#### `dup-c3c8358993cf` (near, 2 sites)

Proposed home: `fold::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/common/fixtures/fold.rs:245-247` `apply_ref`
- `tests/common/fixtures/fold.rs:252-254` `apply_call`

#### `dup-9c3b5c80063e` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/common/fixtures/fold.rs, tests/community_resolution_knob.rs, tests/graph_superseded_prune.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/common/fixtures/fold.rs:343-352` `live_node_ids`
- `tests/community_resolution_knob.rs:139-148` `live_memberships_of`
- `tests/graph_superseded_prune.rs:32-41` `live_contains`

#### `dup-088ec120e4ac` (near, 2 sites)

Proposed home: `host::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/common/fixtures/host.rs:79-86` `sigterm_ignorer_in`
- `tests/common/fixtures/host.rs:90-96` `sleeper_in`

#### `dup-4f6fb68b58e5` (semantic, 3 sites)

Proposed home: `mcp::mcp_session - one canonical constructor, the rest thin variants over it (or a builder), rather than each re-listing every field`

mandatory sweep: parallel constructor functions - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/common/mcp.rs:19-21` `start`
- `tests/common/mcp.rs:26-28` `start_with`
- `tests/common/mcp.rs:31-48` `from_command`

#### `dup-c99581d3e41e` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/common/workflow_probe.rs, tests/store_precedence.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/common/workflow_probe.rs:21-23` `write_workflow`
- `tests/store_precedence.rs:47-49` `write_store_conn`

#### `dup-f0e40001391f` (semantic, 2 sites)

Proposed home: `one shared `seed_coupling` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/community_detection_cli.rs:93-127` `seed_coupling`
- `tests/community_fold_periphery.rs:39-49` `seed_coupling`

#### `dup-c51171ae2133` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_fold_periphery.rs, tests/concepts_fold_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_fold_periphery.rs:169-202` `a_community_assigned_event_without_a_fresh_key_is_a_non_boundary_and_never_supersedes`
- `tests/concepts_fold_periphery.rs:190-235` `a_concept_derived_event_without_a_fresh_key_is_a_non_boundary_and_never_supersedes`

#### `dup-ddaf372936d3` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_fold_periphery.rs, tests/concepts_fold_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_fold_periphery.rs:205-239` `the_community_layer_serialized_literals_are_stable_and_the_fold_matches_the_on_log_type`
- `tests/concepts_fold_periphery.rs:238-280` `the_concept_layer_serialized_literals_are_stable_and_the_fold_matches_the_on_log_type`

#### `dup-19f9cf9e5fab` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/compaction_generations_periphery.rs, tests/reset_derived_compaction_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/compaction_generations_periphery.rs:617-623` `community_of`
- `tests/reset_derived_compaction_periphery.rs:150-156` `entity`

#### `dup-e3a49d3d408e` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/compaction_generations_periphery.rs, tests/product_binary_authority_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/compaction_generations_periphery.rs:3421-3426` `rebuild_owed_note`
- `tests/product_binary_authority_periphery.rs:74-76` `product_file_name`

#### `dup-36ebafe7bb7e` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/compaction_generations_periphery.rs, tests/one_shot_reads_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/compaction_generations_periphery.rs:3840-3846` `holds_table`
- `tests/one_shot_reads_periphery.rs:304-306` `poison`

#### `dup-515218e72204` (near, 2 sites)

Proposed home: `concepts_derivation_cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/concepts_derivation_cli.rs:59-67` `doc`
- `tests/concepts_derivation_cli.rs:72-77` `link`

#### `dup-df1f9764f4e9` (semantic, 2 sites)

Proposed home: `one shared `exploration_graph` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/dash_exploration_route_client_contract.rs:67-117` `exploration_graph`
- `tests/dash_kg_graph_route.rs:1132-1180` `exploration_graph`

#### `dup-c49b09543e58` (semantic, 3 sites)

Proposed home: `one shared `fixture_graph` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/dash_kg_graph_route.rs:47-59` `fixture_graph`
- `tests/graph_query_engine_relocation_periphery.rs:53-72` `fixture_graph`
- `tests/metadata_card_periphery.rs:49-100` `fixture_graph`

#### `dup-18409566d0bb` (near, 2 sites)

Proposed home: `dash_run_tree_spine::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dash_run_tree_spine.rs:589-631` `an_escalated_units_gate_failure_is_not_masked_by_a_trailing_passing_gate`
- `tests/dash_run_tree_spine.rs:692-743` `a_regated_green_unit_renders_gates_passed_despite_an_earlier_failed_attempt`

#### `dup-626b09960195` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/dead_code_json_contract_periphery.rs, tests/duplication_catalog_contract_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dead_code_json_contract_periphery.rs:276-294` `every_deserialized_test_only_reference_has_a_non_empty_file_and_content_hash`
- `tests/duplication_catalog_contract_periphery.rs:151-175` `every_deserialized_site_has_a_non_empty_file_name_and_content_hash`

#### `dup-c06e1c41c352` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/dead_code_json_contract_periphery.rs, tests/responsibility_map_contract_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dead_code_json_contract_periphery.rs:621-641` `section_4_3_citations`
- `tests/responsibility_map_contract_periphery.rs:283-304` `section_1_citations`

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

#### `dup-269f626ea28e` (near, 14 sites)

Proposed home: `a new shared module (sites span 2 files: tests/graph_show_periphery.rs, tests/graph_show_staleness.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_show_periphery.rs:122-177` `graph_show_degrades_to_stale_note_when_location_drifted`
- `tests/graph_show_periphery.rs:187-229` `graph_show_degrades_to_stale_note_when_recorded_line_is_zero`
- `tests/graph_show_periphery.rs:259-293` `graph_show_light_lane_degrades_to_extent_unavailable_note`
- `tests/graph_show_periphery.rs:302-354` `graph_show_bounds_body_at_the_definitions_own_extent`
- `tests/graph_show_periphery.rs:366-441` `graph_show_shows_full_body_past_nested_definition`
- `tests/graph_show_periphery.rs:452-500` `graph_show_shows_full_body_of_a_destructuring_signature`
- `tests/graph_show_periphery.rs:508-550` `graph_show_extent_ignores_braces_in_strings_comments_and_chars`
- `tests/graph_show_periphery.rs:560-622` `graph_show_shows_full_body_of_a_python_nested_def`
- `tests/graph_show_periphery.rs:633-673` `graph_show_does_not_overread_a_js_single_quote_brace_body`
- `tests/graph_show_periphery.rs:744-797` `graph_show_degrades_when_no_grammar_registered_for_the_file_extension`
- `tests/graph_show_periphery.rs:811-870` `graph_show_heals_to_the_live_line_when_the_moved_name_is_unambiguous`
- `tests/graph_show_periphery.rs:879-931` `graph_show_degrades_when_the_moved_name_is_ambiguous_in_the_file`
- `tests/graph_show_staleness.rs:42-86` `graph_show_degrades_gracefully_when_the_recorded_file_is_missing`
- `tests/graph_show_staleness.rs:103-181` `graph_show_never_presents_a_neighbours_body_when_the_line_drifted`

#### `dup-8743cc5db2b4` (near, 2 sites)

Proposed home: `integrate_conflict_merge_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/integrate_conflict_merge_periphery.rs:496-561` `spawn`
- `tests/integrate_conflict_merge_periphery.rs:1302-1347` `spawn`

#### `dup-333ec555460f` (near, 2 sites)

Proposed home: `integrate_conflict_merge_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/integrate_conflict_merge_periphery.rs:1909-1973` `a_crash_right_after_the_merge_attempt_record_resumes_and_completes_row_1`
- `tests/integrate_conflict_merge_periphery.rs:1977-2041` `a_crash_right_after_the_landing_intent_record_resumes_and_completes_row_4`

#### `dup-8fb3e9b78321` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/migration_is_deliberate_periphery.rs, tests/validate_advisories.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/migration_is_deliberate_periphery.rs:505-514` `validate_is_silent_on_retired_code_entities_when_nothing_has_been_retired`
- `tests/validate_advisories.rs:636-645` `validate_is_silent_on_a_gated_scaffolded_fanout_template`

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
- `tests/simplification_audit.rs:8745-8751` `assert_no_same_named_helper_cluster`

#### `dup-d558d72a9443` (near, 4 sites)

Proposed home: `reap_before_removal_audit::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reap_before_removal_audit.rs:744-757` `bare_remove_dir_all_with_no_coverage_is_caught`
- `tests/reap_before_removal_audit.rs:760-776` `bare_git_worktree_remove_with_no_coverage_is_caught`
- `tests/reap_before_removal_audit.rs:1108-1122` `a_finding_names_its_exact_file_and_line_number`
- `tests/reap_before_removal_audit.rs:1237-1264` `a_worktree_remove_args_array_wrapped_across_multiple_lines_by_rustfmt_is_still_caught`

#### `dup-cb850da85ff9` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/reset_derived_compaction.rs, tests/reset_derived_compaction_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reset_derived_compaction.rs:74-100` `rows`
- `tests/reset_derived_compaction_periphery.rs:111-134` `raw_rows`

#### `dup-a199914ac1c3` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/review_round_lenses_only_log_derived_resume_periphery.rs, tests/review_round_non_ancestor_residue_names_true_diff_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/review_round_lenses_only_log_derived_resume_periphery.rs:79-104` `spawn`
- `tests/review_round_non_ancestor_residue_names_true_diff_periphery.rs:71-102` `spawn`

#### `dup-e0f2896fa7de` (exact, 3 sites)

Proposed home: `simplification_audit::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:3394-3397` `real_files`
- `tests/simplification_audit.rs:4903-4906` `real_map`
- `tests/simplification_audit.rs:6865-6868` `real_workspace_files`

#### `dup-23cb89af01e7` (near, 4 sites)

Proposed home: `simplification_audit::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:3452-3469` `dup_cluster_wire`
- `tests/simplification_audit.rs:3471-3484` `dup_cluster_lines`
- `tests/simplification_audit.rs:6818-6835` `dead_code_candidate_wire`
- `tests/simplification_audit.rs:6837-6852` `dead_code_candidate_lines`

#### `dup-adfce3b42b86` (near, 2 sites)

Proposed home: `step_sheds_the_freshen::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/step_sheds_the_freshen.rs:128-163` `fingerprint_subtree`
- `tests/step_sheds_the_freshen.rs:129-159` `walk`

#### `dup-ece604793e18` (semantic, 5 sites)

Proposed home: `store_content_identity_periphery::port_double - one canonical constructor, the rest thin variants over it (or a builder), rather than each re-listing every field`

mandatory sweep: parallel constructor functions - 5 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/store_content_identity_periphery.rs:128-130` `new`
- `tests/store_content_identity_periphery.rs:134-142` `built`
- `tests/store_content_identity_periphery.rs:147-149` `miscounting`
- `tests/store_content_identity_periphery.rs:153-155` `over_an_empty_stream`
- `tests/store_content_identity_periphery.rs:159-161` `over_a_stream`

#### `dup-56fda398784b` (semantic, 2 sites)

Proposed home: `one shared `write_store_conn` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/store_precedence.rs:47-49` `write_store_conn`
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

- `tests/subject_lens_defined_cells.rs:72-108` `communities_no_concepts_graph`
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

- `tests/workflow_definition_and_js_constants_periphery.rs:88-122` `seed_workflow_yml`
- `tests/workflow_definition_and_js_constants_periphery.rs:130-145` `seed_js_files`

### Adversarial sample

Recall check (spec 85 THOROUGHNESS): 30 functions drawn by seeded rank (seed `85072026`, `sample_indices` over all 8923 functions scanned in `src/` and `tests/`, each ranked by the seeded hash of its own file, name and ordinal so a change elsewhere never reshuffles a drawn row, excluding `tests/prioritized_plan_citation_periphery.rs` - criterion 4's own citation-guard periphery test, whose function count grows as its citation-drift-guard mechanism hardens round over round; excluding it keeps that unrelated growth out of the draw), each read by hand - together with its host file's surrounding context, since a duplicate can live anywhere in the file or a sibling file - to judge whether a duplicate exists that the mechanical pass and the five sweeps above did not already catch.

- `crates/rigger-conductor/src/conductor.rs:4974-5015` `split_reject` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `crates/rigger-conductor/src/conductor.rs:15023-15045` `branch_is_foreign_when_only_one_axis_differs` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `crates/rigger-conductor/src/conductor.rs:15929-15936` `resume_of` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `crates/rigger-conductor/src/conductor.rs:26895-26929` `route_review_tier_fails_safe_to_full_on_an_empty_blast_radius` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `crates/rigger-conductor/src/conductor.rs:37204-37234` `a_budget_refused_standalone_review_spawn_keeps_its_worktree` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `crates/rigger-domain/src/canary.rs:285-319` `latest_run_scopes_to_the_last_batch_marker` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `crates/rigger-domain/src/config.rs:381-383` `is_empty` - caught: `dup-53e62db783ac`
- `crates/rigger-domain/src/contextgraph/query.rs:2056-2060` `assert_sole_member_is_w` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `crates/rigger-domain/src/metrics.rs:1417-1424` `model_id_base` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `crates/rigger-domain/src/review.rs:772-809` `a_finding_line_has_five_pipe_fields_with_the_severity_second` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `crates/rigger-domain/src/spec.rs:2249-2256` `strip_inline_code_direct_exact_output_pins_a_zero_width_quote_pair` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:4453-4455` `locked` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `crates/rigger-grounder/src/grounder/symbols/events.rs:571-583` `normalize_logical_path` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `crates/rigger-grounder/src/ingest.rs:1897-1916` `answered` - duplicate found by reading and closed: its view of a sink answer's batch is `wire_owned`; the root sink suite's `Handed::answer` re-rolled that view as its own `typed` and now calls `wire_owned` too
- `crates/rigger-worktree-git/src/worktree.rs:1847-1869` `reclaim_cache_sibling` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `src/cli/mod.rs:4412-4438` `footprint_reclaim_lines` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `src/cli/mod.rs:8417-8427` `parse_run_args_reads_rebase_definition` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `src/cli/run.rs:2085-2109` `start_run_dashboard` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `tests/change_path_revert_periphery.rs:211-321` `a_revert_through_the_shipped_build_records_the_earlier_generations_entry_again` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `tests/cli.rs:9834-9858` `workflow_accepts_a_spec_and_a_base_flag` - no duplicate found by reading
- `tests/cli.rs:19075-19158` `setup_precommit_hook_prefers_a_unit_derived_binary_in_a_real_linked_worktree_over_a_stale_path_rigger` - duplicate found by reading and closed: it re-rolled `fresh_committed_skill`, `committed_skill` and `commit_a_code_change` inline; it now calls them, with the commit half split out as `commit_staged`
- `tests/common/fixtures/events.rs:608-610` `reads` - caught: `dup-ebee743f02de`
- `tests/common/fixtures/events.rs:851-853` `reads` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `tests/common/repo.rs:103-106` `critique_stub_spawns` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `tests/concepts_labels_membership.rs:245-272` `label_of_the_documentless_hub` - no duplicate found by reading
- `tests/event_classes_are_one_table.rs:387-401` `a_right_side_that_is_neither_a_literal_nor_a_type_alias_fails_the_scan_naming_it` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `tests/one_shot_reads_periphery.rs:85-134` `a_project_namespace_over_a_shared_events_file_reads_its_run_as_one_typed_read_per_selection` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `tests/reset_derived_compaction_periphery.rs:3687-3691` `a_prune_with_nothing_to_reclaim_leaves_the_file_unrewritten` - duplicate found by reading and closed: `the_rewrite_flag_follows_the_file_and_not_this_passs_delete_count` repeated its settled-file fixture and skipped-rewrite assertions; both now call `settled_clean_store` and `assert_prune_skips_the_rewrite`
- `tests/simplification_audit.rs:7097-7101` `a_bare_test_attribute_on_a_free_function_marks_it_test_without_a_cfg_test_mod` - duplicate found by reading and closed: it and five sibling scanner tests re-rolled `scan_single`; all now call it or its name and span assertions
- `tests/worker_persona_label_periphery.rs:222-243` `the_subject_is_the_titles_first_sentence_passed_whole_with_no_truncation` - duplicate found by reading and closed: it re-rolled `assert_worker_label` inline; it now calls it

Reading pass 2026-09-27: 7 of the 30 drawn functions read by hand (a caught row too, to judge whether its duplicate reaches past the cluster); 24 duplicate(s) found by reading, each closed.
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
- closed before the redraw: `crates/rigger-grounder/src/grounder/workflowdef.rs` `full_reviewers_of` repeated the head of `ReviewPanel::agent_ids`; both now call `ReviewPanel::full_roster`, and every adversary/adjudicator pair goes through `config::push_reviewers`
- closed before the redraw: `tests/common/fixtures/graph.rs` `summarized_node` was re-rolled as an inline closure by `tests/rationale_overlay_data.rs`; it now calls it
- closed before the redraw: `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs` `tier_default_matches_the_extracted_const` was repeated inline by `tests/code_ingest_events.rs`; the one test now pins all three tier consts
- closed before the redraw: `src/worktree.rs` `remove_reaps_a_process_rooted_inside_the_worktree_and_spares_one_outside` and `tests/reap_before_removal_periphery.rs` `reaps_before_removing` each re-rolled the spawn/wait/teardown/assert reap proof, as did five siblings in `src/main.rs`, `crates/rigger-process/src/reap.rs`, `src/worktree.rs` and the relocated-scratch periphery suite; all now call `assert_teardown_reaps_what_is_rooted_inside`
- closed before the redraw: `tests/heartbeat_write_read_agree_periphery.rs` `watch_once_suppresses_a_false_dead_driver_when_the_configured_workdir_resolves_from_the_owning_root_with_no_agents_fleet` repeated its two siblings' configured-workdir fixture; all three now call `marker_under_a_configured_workdir`

Two real recall gaps surfaced this way and were closed by widening the mechanical sweep with a new generalizable detector each - not a one-off citation - so the fix catches every present and future instance of its class, each pinned by a real-tree regression test: `find_proc_stat_or_status_readers` (decision `u85c2-proc-stat-worked-example`) groups every function reading a `/proc/<pid>/stat` or `/proc/<pid>/status` literal, closing the spec's own named worked example - `crates/rigger-dash/src/dash.rs:717-719` `process_state` next to `crates/rigger-process/src/reap.rs:224-229` `pid_starttime`, the same job on the same file with a different field/shape, upheld at spec 62's capstone; `find_parallel_constructor_clusters` (decision `u85c2-parallel-constructor-sweep`) groups 2+ non-test functions per `(file, Self type)` that build a `Self { .. }` / `TypeName { .. }` literal, closing `src/spawn.rs`'s `SpawnResult::liveness_fault` reading MISSING from its own `ok`/`failed` cluster even though all three are parallel constructors for one struct. A third worked example, `exploration_graph` (independently defined test-fixture builders in `tests/dash_exploration_route_client_contract.rs` and `tests/dash_kg_graph_route.rs`), was already caught correctly by the plain Jaccard pass with no sweep needed - confirming the mechanical pass itself has real recall, not only the two widened sweeps. Two further real defects, found on review rather than in this draw, were closed the same way: a RECALL gap the architecture lens routed to this criterion by name across two prior review rounds - this file's own bespoke source-text lexer (`scan_file`/`tokenize`) duplicating the codebase's ONE canonical tree-sitter extractor, `crates/rigger-grounder/src/grounder/symbols/extract.rs::extract` (its own module doc's claim, architecture 5.5.3) - closed by `find_bespoke_lexer_vs_canonical_extractor` (decision `u85c2-bespoke-lexer-sweep`), a fourth generalizable sweep; and a PRECISION defect the adversary found by reading every `same-named helper` cluster against `ScannedFn::enclosing_impl` - `find_same_named_helper_functions` was misclassifying REQUIRED trait-impl methods as coincidental duplication (`subscribe_all`/`subscribe_stream` across the `EventStore` trait's three backend adapters plus a test double, `blast_radius` across the `Grounder` trait's own default method, its override, and a test double) - closed by excluding members whose extracted Self type differs across the group when at least one comes from an actual `" for "` trait impl (decision `u85c2-same-named-helper-trait-impl-precision-fix`), mirroring `find_parallel_constructor_clusters`'s own `(file, Self type)` keying one function away. Round 7 (decision `u85c4-r7-exclude-periphery-file-from-adversarial-population`) excluded this criterion's own citation-guard periphery file from the draw's population (see this subsection's opening paragraph); that exclusion still applies unchanged.


## 3. Boundary Violations

Instrument: for each of the five named ports (`eventstore::EventStore`, `contextgraph::Projection`, `conductor::AgentDriver`, `gate::Runner`, `grounder::Grounder`), searched every production (pre-`#[cfg(test)]`) call site of that port's known concrete adapter modules from a NON-adapter, NON-composition-root file, and separately searched every domain-ish file's top-level `use` statements for a direct infrastructure-crate import. `src/main.rs` is exempt from the "reaches a concrete adapter" check: it is the composition root, and wiring concretions together is its designed job. Every citation below is resolved against the tree when the report is rendered.

FOUND, one violation:

Violation 1 (`Grounder`): `crates/rigger-grounder/src/ingest.rs:394-426` (`walk_batches`, called from production `conductor::RunCtx::ingest_project_batches` at `crates/rigger-conductor/src/conductor.rs:11219`, itself called from `crates/rigger-conductor/src/conductor.rs:11201` above the `14159` `#[cfg(test)]` boundary) calls `crate::grounder::symbols::events::project_batches_paced` directly by concrete module path at line 400 to reuse the `symbols` grounder's already-persisted index for a one-time whole-project ingest walk, then at line 406 - same function, same missing-port defect, not a separate third violation - calls `crate::grounder::design::events::project_batches` directly by concrete module path for the design-doc half of the same walk. These two calls are two of the three named sites of section 2's own catalogued duplicate cluster (`dup-f2eee0c78cd6`: `crates/rigger-grounder/src/grounder/design/events.rs:97-99`, `crates/rigger-grounder/src/grounder/symbols/events.rs:59-61`, and `crates/rigger-grounder/src/grounder/workflowdef.rs:233-240` - all three named `project_batches`), so this boundary violation and that duplication finding are two symptoms of one root cause - `ingest.rs` naming each concrete grounder submodule because no port exposes either. The `Grounder` port (`crates/rigger-domain/src/grounder.rs:64`: `ground`, `reindex`, `blast_radius`, `index_stamp`) serves real-time per-query grounding of an agent's prompt; none of its methods exposes "hand me every indexed file's projected events for a whole-project batch ingest," so `ingest.rs` - itself a domain ingest authority, not an adapter and not the composition root - has no port to depend on for either call and reaches the concrete `symbols` module (400) and the concrete `design` module (406) directly. Fix direction for a follow-up spec: add an ingest-shaped port method (e.g. a `Grounder::project_batches` or a standalone `SymbolProjector` trait) covering both concrete modules, so `ingest.rs` depends on one abstraction instead of either concrete grounder module for its whole-project walk.

Also reaching `grounder::symbols::store::content_hash` from `crates/rigger-grounder/src/ingest.rs:649` and `crates/rigger-conductor/src/canary_store.rs:153`: DISPOSITIONED as legitimate shared-primitive reuse, not a third violation. `content_hash` (`crates/rigger-grounder/src/grounder/symbols/store.rs:47-57`) is documented at its own definition as the content-identity primitive the `symbols` grounder's reindex freshening gate keys on, and `canary_store.rs`'s own doc comment (`crates/rigger-conductor/src/canary_store.rs:131`) reuses it by deliberate author intent rather than growing another open-coded FNV-1a copy - a generic hashing utility that happens to live in the `symbols` module, not a grounding operation reached through the port. The broader duplication this primitive is meant to fix (the open-coded FNV-1a copies elsewhere in the crate, per `crates/rigger-domain/src/community.rs:67`'s own comment) is a separately tracked cross-cutting refactor (`arch-u2i-fnv1a-fourth-parallel-copy`), not this section's concern.

CHECKED AND CLEAN (four of five ports fully clean; the fifth, `Grounder`, is this section's violation above - each search recorded so a clean result is not merely assumed):
- `conductor::AgentDriver` concretion reach (`crate::driver::*`): production `conductor.rs` depends only on the port (`crates/rigger-domain/src/agent.rs:212`), held as `dyn AgentDriver` (`crates/rigger-conductor/src/conductor.rs:1840`); every one of its 16 `crate::driver::` hits sits inside `#[cfg(test)] mod tests`, where the tests construct a concrete driver directly.
- `eventstore::EventStore` concretion reach (`rusqlite::Connection::open` outside the SQLite adapters and `crates/rigger-store-sqlite/src/sqlite.rs`, the one opener every store connection goes through): in production, only doc-comment mentions (`src/cli/mod.rs:1865,2046`); the one call is a deliberate, explicitly-commented test-only raw-connection bypass (`src/cli/mod.rs:5173`, inside `#[cfg(test)] mod tests` opened at `src/cli/mod.rs:5129`) that reproduces a pre-append-guard corruption shape `Store::append` itself refuses to construct - a documented test technique, not a boundary violation.
- `contextgraph::Projection` concretion reach (`contextgraph::sqlite::*`): checked whole-tree, not only `crates/rigger-conductor/src/conductor.rs` - every one of `conductor.rs`'s 59 hits sits inside `#[cfg(test)] mod tests` (production `conductor.rs` only ever depends on `dyn Projection`), and the same is true wherever else `contextgraph::sqlite::Projector` is imported (`crates/rigger-graph-sqlite/src/concepts.rs`, `crates/rigger-dash/src/dash.rs`, `crates/rigger-grounder/src/grounder/symbols/events.rs`, `crates/rigger-grounder/src/grounder/design/events.rs` - every import sits after that file's own `#[cfg(test)]` boundary).
- `gate::Runner` concretion reach (`gate::ExecRunner` / `RecordingRunner`): checked whole-tree, not only `crates/rigger-conductor/src/conductor.rs`. In `conductor.rs`, production depends only on `dyn gate::Runner` (`crates/rigger-conductor/src/conductor.rs:1841`); every production mention of a concrete runner is a doc comment (`crates/rigger-conductor/src/conductor.rs:8477,8607,8617,8624`), and the import of `ExecRunner` (`crates/rigger-conductor/src/conductor.rs:14165`) and every one of its 55 uses sit inside `#[cfg(test)] mod tests`. In `crates/rigger-driver/src/driver/replay.rs`, all 15 `ExecRunner` mentions sit inside that file's own `#[cfg(test)] mod tests` too.
- Use cases importing infrastructure: searched the `use` statements of every domain-ish file this audit's own code neighborhood names (`crates/rigger-conductor/src/conductor.rs`, `crates/rigger-domain/src/blocker.rs`, `crates/rigger-domain/src/spec.rs`, `crates/rigger-domain/src/watch.rs`, `crates/rigger-domain/src/community.rs`) for `rusqlite`, `reqwest`, `tonic`, `tokio`, `kurrentdb` - zero hits anywhere. Empty category.

A second mutation authority for one domain: the one previously-known instance in this codebase (`crates/rigger-dash/src/dash.rs` reimplementing `crates/rigger-process/src/reap.rs`'s `/proc` pid scan, spec 85's own Goal example, upheld at spec 62's capstone) is a duplicate READ-only reimplementation, not a bypassed MUTATION path - it is section 2's finding (`u85c2-proc-stat-worked-example`, `find_proc_stat_or_status_readers`), not re-counted here to avoid double-charging one defect to two sections. Checked process spawning as the one other plausible second-authority candidate: production code constructs every process through the one process-spawn port (`crates/rigger-process/src/subprocess.rs`), so production `conductor.rs` never builds a git command of its own. `crates/rigger-worktree-git/src/worktree.rs` is the sole git-worktree-mutation authority OUTSIDE the composition root. Inside it, `src/cli/mod.rs` (exempt from the port-concretion-reach check above, not from this one) holds two more git-worktree-mutation sites: `reap_then_remove_worktree` (`src/cli/mod.rs:1814-1826`), the sanctioned worktree half of the spec-34/spec-79 orphan-sweep and extensively reviewed across those specs - a deliberate design choice, not a gap; and `materialize_config_at_rev` (`src/cli/mod.rs:2767-2809`), a real, already-known, non-blocking gap (`arch-u13-config-checkout-bypasses-worktree-authority` / `arch-u2r-config-checkout-shells-git` / `arch-u2r2-replayrunner-and-config-checkout-persist-not-introduced`: the `Worktree` API is branch-creating and exposes no detach-at-rev checkout, so this is a gap in that authority rather than a competing abstraction). No second mutation authority found beyond the already-cited, already-catalogued `/proc` case and this already-dispositioned `materialize_config_at_rev` gap.

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

- `live_contains` / `live_memberships_of` / `live_node_ids` - 3 sites across 3 files (`dup-9c3b5c80063e`, near).
- `fixture_graph` - 3 sites across 3 files (`dup-c49b09543e58`, semantic).
- `courier_project` / `courier_project_with_commit` / `temp_rigger_project` / `temp_store_project` - 4 sites across 2 files (`dup-2a101767f24d`, near).
- `communities_no_concepts_graph` / `file_over_code_graph` / `mixed_membership_graph` - 3 sites across 2 files (`dup-498b51115191`, near).

Proposed home for each: `tests/common` (the catalog's own `proposed_home` field says so for each). Consolidating just these collapses roughly 13 duplicate definitions into 4 shared ones - the single largest mechanical simplification this audit identifies anywhere in the test suite.

### 5.3 `tests/cli.rs` split plan

`tests/cli.rs` holds 362 `#[test]` functions. Its existing internal section markers each name the spec and criterion whose tests follow it, not a CLI subcommand or subsystem - ad hoc organization that falls well short of a deliberate, complete per-surface structure. The split proposed below replaces those by-spec markers with a complete, deliberate BY CLI SUBCOMMAND SURFACE organization. A keyword-on-test-name pass (matching each test's dominant CLI verb: `step_`, `run_`, `validate_`, `reset_`, `watch_`/`watchdog_`, `canary_`, `dash_`/`status_`, `store_`/`eventstore_`, `spawn_`/`scratch_`, `review_`/`gate_`, `setup_`/`precommit_`/`hook_`, `courier_`/`registry_`, `spec_`, `replay_`, `worktree_`, `emit_`/`peers_`/`decision_`, `stats_`, `heartbeat_`/`liveness_`, `prime_`/`version_`/`init_`) only cleanly covers 203 of the 362 tests (56%) - disclosed honestly rather than overclaimed, because a real fraction of `cli.rs`'s scenarios are DELIBERATELY end-to-end (a single test legitimately drives `step` + `run` + `validate` + `dash` together to prove a cross-cutting property, e.g. `a_run_driver_auto_starts_a_reachable_dash_with_a_url_shown_in_status` or `docs_ships_graph_hygiene_guidance_to_consumers`), which a bare keyword match cannot and should not force into one bucket. The proposed split is BY CLI SUBCOMMAND SURFACE - `cli.rs`'s own natural organizing concept, since the whole file drives the `rigger` binary end to end - into per-surface files (`tests/cli_step.rs`, `tests/cli_run.rs`, `tests/cli_validate.rs`, `tests/cli_reset.rs`, `tests/cli_watch.rs`, `tests/cli_canary.rs`, `tests/cli_dash.rs`, `tests/cli_store.rs`, `tests/cli_review.rs`, `tests/cli_setup.rs`, plus a residual `tests/cli_misc.rs` for the genuinely cross-cutting scenarios), with each test's home decided by its DOMINANT scenario on a human/AI read, not a mechanical keyword match - the same discipline this audit's own responsibility map applied to unassignable functions (named, never silently forced). Cross-referencing the catalog: `cli.rs` also participates in 5 of the catalog's cross-file test-only duplication clusters, several paired against files that WOULD merge with it under this split (`tests/step_attention_periphery.rs`, paired in 1 clusters; `tests/watchdog_cli_periphery.rs`, paired in 1 clusters) - the split is expected to shrink, not grow, the duplication surface.

### 5.4 Duplicated helpers across test files (beyond 5.2's headline cases)

34 test-only clusters in the committed catalog have every site as an ordinary (non-`#[test]`) helper function - the shared-fixture-extraction candidate class. Beyond the 4 in 5.2, the widest are:

- `assert_selected_server` / `assert_selected_sqlite` / `assert_server_reached_and_credentials_redacted` - 3 sites across 2 files (`dup-b58b0ff72b22`, near).
- `community_over_concepts_graph` / `full_in_budget_graph` - 2 sites across 2 files (`dup-06ffd3c67857`, near).
- `spawn` - 2 sites across 2 files (`dup-14d812231784`, near).
- `community_of` / `entity` - 2 sites across 2 files (`dup-19f9cf9e5fab`, near).
- `holds_table` / `poison` - 2 sites across 2 files (`dup-36ebafe7bb7e`, near).
- `write_store_conn` - 2 sites across 2 files (`dup-56fda398784b`, semantic).

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
2. Tier 2 - god-file test-module extraction: each of the three god files' own inline `#[cfg(test)] mod tests` holds 58-73% of that file's mapped functions (section 1's map), and moving it is a pure relocation with no production-behavior change - the single largest safe line-count reduction in this plan, and the precondition that makes tier 3 tractable.
3. Tier 3 - god-file production splits: section 1's own proposed module tree applied to the (now much smaller) remaining production surface of each god file. Higher execution risk than tier 2 because it touches live orchestration and CLI logic, so it is sequenced after tier 2 shrinks the target first.
4. Tier 4 - named production duplication sweeps: the mechanical mandatory sweeps section 2 ran regardless of the Jaccard pass (`Command::new`, `.rigger`-path literals, sqlite `Connection::open`, error-shaping helpers), each already a single committed cluster with its own proposed home.
5. Tier 5 - test-suite consolidation: section 5's own catalogued test-only duplication. No production-correctness exposure at all (worst case a test regresses, never the product), so it is ordered ahead only of tier 6 despite touching the largest raw line count anywhere in this plan.
6. Tier 6 - remaining catalog sweep: the 140 src-touching clusters section 2 found but tiers 1 and 4 did not individually name. Unlike every other tier, none of these 140 have been read and risk-assessed one at a time the way tiers 1-4's named clusters have - they are consumed straight from the catalog - so this tier carries production-correctness exposure tiers 2, 3 and 5 do not, and is ordered last: the follow-up spec must triage each cluster's own production-or-test status before merging it, not assume tier 5's blanket test-only treatment applies here too.

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

#### 2. Retire the duplicate `/proc`-reading authority (`dup-99a8136efbc4` + `dup-96daf6c3e468`)

- Scope: the production half is done - `crates/rigger-dash/src/dash.rs::process_state` and `crates/rigger-process/src/reap.rs::pid_starttime` both read their `/proc/<pid>/stat` field through `crates/rigger-process/src/reap.rs::stat_field_after_comm`, the one parser of the kernel's `pid (comm) state ...` layout (`read_ppid` reads `/status`, a different file). What remains is the test-only readers (`dup-96daf6c3e468`, 9 sites across `crates/rigger-process/src/reap.rs`, `tests/common/fixtures/host.rs`, `tests/simplification_audit.rs`, such as the shared `tests/common/fixtures/host.rs::pgid_of` fixture) and 55 raw `/proc`-path string literals across 11 files (`dup-99a8136efbc4`), most of them assertion messages and this audit's own sweep names rather than reads.
- Files: the test-only readers `dup-96daf6c3e468` names.
- Expected line delta: small and negative - a test fixture reads its field through one shared helper instead of re-splitting the stat line.
- Risk: low - test-only; nothing this touches can signal or end a process, so it carries none of the no-os-kill gate's own risk surface.
- Unblocks: retires the last copies of the "duplicate implementation reconciled after the fact" pattern the operator's strict-DRY rule targets - the concrete precedent spec 85's own Goal cites.

### 6.3 Tier 2: god-file test-module extraction

Each god file's inline test module is the set of its `is_test: true` entries in the committed `docs/audit/responsibility-map.json`. Each entry below moves an already-passing test module with no intended production-behavior change - a `cargo test` pass before and after is the whole verification. The line figures below sum those test functions' own spans, so they exclude the module-level doc comments, `use` statements and blank lines around them.

#### 3. Extract `crates/rigger-conductor/src/conductor.rs`'s inline test module

- Scope: the file's `#[cfg(test)] mod tests` holds 740 of its 1002 mapped functions (73%), roughly 29267 lines of test-function spans. Partition into a `src/conductor/tests/` directory, one file per concern, reusing the same names section 1 already assigned the file's own production buckets (`run_ctx`, `support`, `gate`, `prior_failure`, `schedule`, ...) so the split needs no new naming scheme.
- Files: `crates/rigger-conductor/src/conductor.rs` -> `crates/rigger-conductor/src/conductor.rs` (production only) + `src/conductor/tests/*.rs`.
- Expected line delta: 0 net (repo-wide) - roughly 29267 lines relocated out of `crates/rigger-conductor/src/conductor.rs`.
- Risk: low - mechanical move of passing tests, zero intended behavior change.
- Unblocks: shrinks `conductor.rs` to its production code before tier 3 touches a single production line, cutting the odds that an unrelated future unit's blast radius collides with this file.

#### 4. Extract `src/main.rs`'s inline test module

- Scope: the file's `#[cfg(test)] mod tests` holds 199 of its 341 mapped functions (58%), roughly 5935 lines of test-function spans. Same partition approach as item 3, reusing section 1's own production bucket names (`store`, `support`, `render`, `provenance`, `commands`, ...).
- Files: `src/cli/mod.rs` -> `src/cli/mod.rs` (production only) + `src/main/tests/*.rs`.
- Expected line delta: 0 net (repo-wide) - roughly 5935 lines relocated out of `src/cli/mod.rs`.
- Risk: low, same rationale as item 3.
- Unblocks: shrinks `main.rs` to its production code before tier 3's own main.rs split.

#### 5. Extract `crates/rigger-dash/src/dash.rs`'s inline test module

- Scope: the file's `#[cfg(test)] mod tests` holds 149 of its 241 mapped functions (61%), roughly 5483 lines of test-function spans. Lower effort than items 3-4: section 1's own classifier already found 4 pre-existing sub-boundaries inside this one test module (`dash::tests::calls_route_c4`, `dash::tests::metadata_card_c2`, `dash::tests::rationale_overlay_c3`, `dash::tests::subject_view_c5`), so the partition points already exist and need only become their own files.
- Files: `crates/rigger-dash/src/dash.rs` -> `crates/rigger-dash/src/dash.rs` (production only) + `src/dash/tests/*.rs`.
- Expected line delta: 0 net (repo-wide) - roughly 5483 lines relocated out of `crates/rigger-dash/src/dash.rs`.
- Risk: low - the lowest-effort of the three, for the reason above.
- Unblocks: shrinks `dash.rs` to its production code before tier 3's own dash.rs split.

### 6.4 Tier 3: god-file production splits

Each entry below applies section 1's own proposed module tree to a god file's production surface, sequenced after the matching tier-2 entry removes that file's test bulk first. Every module name and function/line count below is summed directly from the committed `docs/audit/responsibility-map.json` (function-body spans only); a file's remaining non-function production lines - struct/enum/type definitions, `use` statements, module docs - are outside section 1's own function-only scan and move with whichever module they sit beside, without needing their own assignment.

#### 6. Split `crates/rigger-conductor/src/conductor.rs`'s production code into `src/conductor/*.rs`

- Scope: 217 mapped functions across 19 proposed modules (roughly 8276 lines of function bodies) plus 45 unassigned functions (1231 lines, each individually named in the committed map for manual placement, per section 1's own "unassignable functions are named as such, never omitted" rule). Headline buckets (functions/lines): `conductor::run_ctx` (140/7072), `conductor::support` (16/488), `conductor::gate` (15/196), `conductor::prior_failure` (5/104), `conductor::schedule` (3/71).
- Files: `crates/rigger-conductor/src/conductor.rs` -> `src/conductor/mod.rs` + `src/conductor/{run_ctx,support,gate,prior_failure,schedule,review,emit,run,budget,review_round,spawn,infra_retries,gate_outcome,review_outcome,deps,ground,gate_ratchet,throwaway,integration_approval}.rs`.
- Expected line delta: 0 net - pure relocation of roughly 9507 lines of function bodies.
- The largest bucket, `conductor::run_ctx`, may warrant its own second pass if it does not decompose cleanly into one file.
- Risk: medium-high - conductor.rs is the composition root's own most complex use-case file; every intermediate commit needs the full `cargo test`, no-os-kill and reap audits green, not just the final one.
- Unblocks: the largest reduction in production-code blast-radius collision risk this audit identifies; makes future duplication-spotting against conductor.rs's own logic tractable by a human reviewer, not only by the mechanical scanner.

#### 7. Split `src/main.rs`'s production code into `src/main/*.rs`

- Scope: 79 mapped functions across 13 proposed modules (roughly 1756 lines of function bodies) plus 63 unassigned functions (1397 lines, each individually named in the committed map for manual placement, per section 1's own "unassignable functions are named as such, never omitted" rule). Headline buckets (functions/lines): `main::store` (21/695), `main::support` (18/310), `main::render` (4/182), `main::provenance` (12/163), `main::commands` (1/158).
- Files: `src/cli/mod.rs` -> `src/main.rs` (composition root, thinned) + `src/cli/{store,support,render,provenance,commands,setup,liveness,store_location,replay_runner,store_selection,docs_overlay,residue_report,dash_glue}.rs`.
- Expected line delta: 0 net - pure relocation of roughly 3153 lines of function bodies.
- Risk: medium - `main.rs` is the composition root itself; the split must preserve which concretions get wired where, not merely move text.
- Unblocks: shrinks `main.rs` to a genuine composition root plus a `cli/` module tree, matching the ports-and-adapters shape this project already mandates everywhere else.

#### 8. Split `crates/rigger-dash/src/dash.rs`'s production code into `src/dash/*.rs`

- Scope: 48 mapped functions across 7 proposed modules (roughly 1318 lines of function bodies) plus 44 unassigned functions (825 lines, each individually named in the committed map for manual placement, per section 1's own "unassignable functions are named as such, never omitted" rule). Headline buckets (functions/lines): `dash::server` (8/605), `dash::render` (18/402), `dash::reproject` (5/176), `dash::registry` (6/65), `dash::response` (6/48).
- Files: `crates/rigger-dash/src/dash.rs` -> `src/dash/mod.rs` + `src/dash/{server,render,reproject,registry,response,dash_marker,probe_miss}.rs`.
- Expected line delta: 0 net - pure relocation of roughly 2143 lines of function bodies.
- Risk: low-medium - the always-on dash's own contract (loopback-only, zero-new-dependency) is unaffected by a pure module split.
- Unblocks: completes the god-file split trio; the third program-sized file becomes an ordinary module tree.

### 6.5 Tier 4: named production duplication sweeps

Each entry is one of section 2's five named mandatory sweeps - collected mechanically regardless of the Jaccard pass, per spec 85's own Design.

#### 9. Consolidate the 595 `.rigger`-path string-literal sites (`dup-e31752c98ad7`) - the single largest cluster in the entire catalog by site count

- Scope: one `.rigger`-relative path-composition helper (the cluster's own `proposed_home`) every one of the 595 sites routes through instead of building its own literal.
- Files: spans dozens of files including `crates/rigger-conductor/src/conductor.rs`, `crates/rigger-config-files/src/config_store.rs`, `crates/rigger-dash/src/dash.rs`, `crates/rigger-domain/src/docs.rs`, `crates/rigger-gates-shell/src/gate.rs`, `crates/rigger-grounder/src/grounder/mod.rs`, `crates/rigger-grounder/src/grounder/symbols/store.rs`, `crates/rigger-grounder/src/ingest.rs`, `src/main.rs`, `crates/rigger-process/src/reap.rs`, `crates/rigger-store-sqlite/src/registry.rs`, `src/worktree.rs` plus many `tests/` files - the full site list is in the committed `docs/audit/duplication-catalog.json` under `dup-e31752c98ad7` for the follow-up spec to consume directly, not re-enumerated here.
- Expected line delta: negative - 595 literal compositions collapse toward one helper's call sites; the helper itself is small.
- Risk: medium - the largest surface-area sweep in this plan by site count, even though each individual site is trivial; needs a mechanical rewrite pass plus a full-suite green run, not hand-editing 595 sites.
- Unblocks: the biggest single site-count reduction available anywhere in the duplication catalog.

#### 10. The 89 `Command::new` call sites (`dup-decc12fbf756`) - production spawns already route through one process-spawn port

- Scope: every production spawn routes through `crates/rigger-process/src/subprocess.rs` (the cluster's own `proposed_home`), and the audit's `the_process_spawn_port_is_the_only_production_command_new_caller` gate refuses a new direct construction anywhere else in production code. The 1 site(s) in `crates/rigger-process/src/subprocess.rs` are the port itself; the other 88 are test code spawning git, shells and the product binary.
- Files: `crates/rigger-process/src/subprocess.rs` plus test code in `src/` and `tests/` - full site list in `docs/audit/duplication-catalog.json` under `dup-decc12fbf756`.
- Expected line delta: none left in production; a test site that repeats a shared fixture's spawn routes through that fixture instead.
- Risk: low - no production spawn is left to move, and the gate keeps it that way.
- Unblocks: the next process-spawning concern added anywhere in the crate reuses the port instead of constructing its own `Command`.

#### 11. Consolidate the 68 sqlite `Connection::open` call sites (`dup-1f66a86debc4`)

- Scope: one sqlite-connection-opening adapter function (the cluster's own `proposed_home`) spanning `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs`, `crates/rigger-store-sqlite/src/eventstore/sqlite.rs` and `src/main.rs`, plus several `tests/` files.
- Files: full site list in `docs/audit/duplication-catalog.json` under `dup-1f66a86debc4`.
- Expected line delta: negative - 68 open calls collapse toward one function.
- Risk: medium - touches the event store and context graph's own connection-lifecycle code; needs the store-identity and store-resolution contract tests green throughout.
- Unblocks: one place to change pragma/timeout/journal-mode settings instead of 68.

#### 12. Consolidate the 13 error-shaping helper sites (`dup-2b8f802a2dc3`) - caution, confirm before merging

- Scope: the cluster spans  and 5 unrelated test files - a wide spread for one claimed duplicate. This may be a threshold-gaming false cluster (spec 85's own CONSTRAINTS WALK: "the threshold is a floor for the mechanical pass; the reading pass owns semantic duplicates") rather than one real shared concern - the follow-up spec's first job is confirming by reading whether these 13 sites share actual logic before proposing one helper, not assuming the cluster label proves it.
- Files: the `src/` files above, plus the 5 test files named in `docs/audit/duplication-catalog.json` under `dup-2b8f802a2dc3`.
- Expected line delta: unknown pending the confirmation read above - potentially zero if the cluster does not survive a human read.
- Risk: low (the smallest-site-count sweep), but with the stated precondition.
- Unblocks: either a genuine fifth consolidation, or a documented "not a real duplicate" disposition that keeps the catalog honest for whoever reads it next.

### 6.6 Tier 5: test-suite consolidation

Every entry cites section 5's own already-catalogued test-only duplication; none of it carries production-correctness risk.

#### 13. Extract the headline shared test fixtures into `tests/common` (section 5.2)

- Scope: `dup-9c3b5c80063e` (3 files), `dup-c49b09543e58` (3 files), `dup-2a101767f24d` (2 files), `dup-498b51115191` (2 files) - roughly 13 duplicate definitions collapsing into 4 shared ones, the single largest mechanical simplification section 5 identifies anywhere in the test suite.
- Files: per-cluster, from the committed catalog, plus `tests/common/`.
- Expected line delta: negative - each fixture's small body survives once instead of once per file.
- Risk: low - test-only, and `tests/common/` already holds the same shape of shared fixture.
- Unblocks: item 16 below (the remaining test-helper clusters) reuses the same `tests/common` home this item establishes.

#### 14. Split `tests/cli.rs` by CLI subcommand surface (section 5.3's plan)

- Scope: 362 tests, split into `tests/cli_{step,run,validate,reset,watch,canary,dash,store,review,setup}.rs` plus a residual `tests/cli_misc.rs` for the genuinely cross-cutting scenarios section 5.3 names, using each test's dominant scenario (a human/AI read, not the 56%-coverage keyword match section 5.3 already disclosed as insufficient alone).
- Files: `tests/cli.rs` and the eleven new files above.
- Expected line delta: 0 net - pure relocation into eleven files.
- Risk: low-medium - a mechanical per-test move with `cargo test`'s full pass count as the verification.
- Unblocks: splits a file in 5 cross-file duplication clusters (section 5.3) and lets item 16's remaining-clusters sweep target smaller, subcommand-scoped files.

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

#### 18. Sweep the remaining 140 src-touching duplication clusters (section 2, beyond tiers 1 and 4's 7 named clusters)

- Scope: of the catalog's 210 clusters, 63 are test-only (items 13 and 15-17 above) and 7 are the named tier-1/tier-4 items (`dup-decc12fbf756`, `dup-e31752c98ad7`, `dup-1f66a86debc4`, `dup-99a8136efbc4`, `dup-96daf6c3e468`, `dup-f2eee0c78cd6`, `dup-2b8f802a2dc3`); the remaining 140 clusters touching `src/` - mostly small 2-5-site exact/near matches like the two worked examples section 2 itself opens with (`dup-49d4d9f335fc`, `dup-be7f6094aaff`) - are swept here, largest exact-duplicate clusters first, consumed directly from `docs/audit/duplication-catalog.json`.
- Files: per-cluster, from the committed catalog.
- Expected line delta: negative, cumulative; the largest single contributor is whichever exact cluster has the most sites (read from the catalog at spec-writing time, not fixed here).
- Risk: low-medium - unlike tier 5, some of these clusters are production code, so each merge needs its own test-coverage check, not a blanket "test-only" pass.
- Unblocks: the last of the catalog's 210 clusters; after items 1-2 and 9-18 all land, a future spec can state and check that the duplication catalog's own drift guard finds zero live clusters left unaddressed.

### 6.8 Dead and vestigial code beyond item 0: no further follow-up

Section 4.2's rule leaves no dead-code category for a later plan item: a function is live or it is deleted by item 0. Both named retirements (`turbovec`, `kurrentdb`) are still fully clean, and the two stale-looking doc paths found remain confirmed generic illustrative examples, not real dangling references.
