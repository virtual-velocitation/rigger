# Simplification Audit - 2026-09

The audit report spec 85 derives the follow-up refactoring specs from. Six sections, each claim citing `file:line`; see `specs/85-simplification-audit.md` for scope.

## 1. Responsibility Map

Every function in `crates/rigger-conductor/src/conductor.rs`, `src/main.rs` and `src/dash.rs` (1698 functions total), assigned to a proposed module by `tests/simplification_audit.rs`'s deterministic scanner + rule-table classifier (never by hand). Instrument: the brace-matching scanner over the three named files.

### Proposed module tree

- `conductor::budget` (3 functions)
  - `crates/rigger-conductor/src/conductor.rs:369-371` `compensation_queued_key` - name contains "compensat" (budget accounting); grouped under `conductor::budget`.
  - `crates/rigger-conductor/src/conductor.rs:798-827` `pending_compensations_from_log` - name contains "compensat" (budget accounting); grouped under `conductor::budget`.
  - `crates/rigger-conductor/src/conductor.rs:12771-12781` `mutation_scratch_settled` - name contains "mutation" (budget accounting); grouped under `conductor::budget`.
- `conductor::emit` (2 functions)
  - `crates/rigger-conductor/src/conductor.rs:399-401` `quarantine_record_key` - name contains "record" (event/decision emission); grouped under `conductor::emit`.
  - `crates/rigger-conductor/src/conductor.rs:12576-12609` `recorded_adoption` - name contains "record" (event/decision emission); grouped under `conductor::emit`.
- `conductor::gate` (11 functions)
  - `crates/rigger-conductor/src/conductor.rs:355-362` `gate_key` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `crates/rigger-conductor/src/conductor.rs:409-416` `gate_intersects_radius` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `crates/rigger-conductor/src/conductor.rs:430-432` `unit_of_gate_key` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `crates/rigger-conductor/src/conductor.rs:440-444` `gate_key_attempt` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `crates/rigger-conductor/src/conductor.rs:503-533` `recorded_gate_outcome` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `crates/rigger-conductor/src/conductor.rs:538-540` `deferred_gate_verdict_key` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `crates/rigger-conductor/src/conductor.rs:549-551` `deferred_gate_failed_key` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `crates/rigger-conductor/src/conductor.rs:11241-11248` `with_gate_hold` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `crates/rigger-conductor/src/conductor.rs:11256-11264` `union_gates` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `crates/rigger-conductor/src/conductor.rs:11343-11350` `gate_failure_cause` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `crates/rigger-conductor/src/conductor.rs:12928-12973` `assert_no_ungated_fanout_unit` - name contains "gate" (gate execution); grouped under `conductor::gate`.
- `conductor::gate_ratchet` (1 function)
  - `crates/rigger-conductor/src/conductor.rs:700-706` `for_persistent_failure` - method inside `impl GateRatchet`; grouped with its other `GateRatchet` methods.
- `conductor::ground` (1 function)
  - `crates/rigger-conductor/src/conductor.rs:11863-11872` `graph_around_recovery` - name contains "graph" (grounding integration); grouped under `conductor::ground`.
- `conductor::integration_approval` (1 function)
  - `crates/rigger-conductor/src/conductor.rs:1098-1100` `approved` - method inside `impl IntegrationApproval`; grouped with its other `IntegrationApproval` methods.
- `conductor::prior_failure` (3 functions)
  - `crates/rigger-conductor/src/conductor.rs:1124-1129` `is_empty` - method inside `impl PriorFailure`; grouped with its other `PriorFailure` methods.
  - `crates/rigger-conductor/src/conductor.rs:1133-1151` `summary` - method inside `impl PriorFailure`; grouped with its other `PriorFailure` methods.
  - `crates/rigger-conductor/src/conductor.rs:1156-1203` `block` - method inside `impl PriorFailure`; grouped with its other `PriorFailure` methods.
- `conductor::review` (5 functions)
  - `crates/rigger-conductor/src/conductor.rs:321-323` `review_round_start_key` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `crates/rigger-conductor/src/conductor.rs:1367-1380` `degenerate_reviewer` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `crates/rigger-conductor/src/conductor.rs:11299-11308` `review_evidence` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `crates/rigger-conductor/src/conductor.rs:11433-11440` `review_protocol` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `crates/rigger-conductor/src/conductor.rs:12537-12551` `recorded_review_round_start_sha` - name contains "review" (review orchestration); grouped under `conductor::review`.
- `conductor::review_outcome` (3 functions)
  - `crates/rigger-conductor/src/conductor.rs:744-752` `verdict` - method inside `impl ReviewOutcome`; grouped with its other `ReviewOutcome` methods.
  - `crates/rigger-conductor/src/conductor.rs:753-755` `approved` - method inside `impl ReviewOutcome`; grouped with its other `ReviewOutcome` methods.
  - `crates/rigger-conductor/src/conductor.rs:756-758` `rejected` - method inside `impl ReviewOutcome`; grouped with its other `ReviewOutcome` methods.
- `conductor::run` (4 functions)
  - `crates/rigger-conductor/src/conductor.rs:12258-12260` `unit_branch` - name contains "unit" (unit run loop); grouped under `conductor::run`.
  - `crates/rigger-conductor/src/conductor.rs:12279-12285` `unit_worktree_dir` - name contains "unit" (unit run loop); grouped under `conductor::run`.
  - `crates/rigger-conductor/src/conductor.rs:12826-12839` `stale_units_from_log` - name contains "unit" (unit run loop); grouped under `conductor::run`.
  - `crates/rigger-conductor/src/conductor.rs:12861-12899` `stale_downstream_units` - name contains "unit" (unit run loop); grouped under `conductor::run`.
- `conductor::run_ctx` (125 functions)
  - `crates/rigger-conductor/src/conductor.rs:2778-2780` `emit` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:2789-2804` `append_and_fold` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:2816-2838` `append_and_fold_batch` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:2843-2849` `emit_with_actor` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:2859-2867` `emit_meta` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:2880-2882` `emit_keyed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:2893-2918` `emit_keyed_meta` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:2953-2992` `emit_keyed_batch` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3001-3007` `agent_model` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3014-3016` `is_stale` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3041-3052` `effective_attempts` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3063-3071` `gate_verdict_high_water` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3079-3093` `mark_stale_downstream` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3109-3187` `drain_compensations` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3200-3249` `commits_to_compensate` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3259-3290` `emit_gate_skip` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3307-3366` `emit_gate_verdict` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3374-3381` `max_retries` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3394-3405` `max_retries_for` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3431-3437` `budget_tripped` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3444-3446` `spawn_is_recorded` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3462-3485` `reserve_spawn` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3494-3524` `trip_budget_breaker` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3535-3545` `halt_reason` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3550-3560` `check_coverage_or_flag` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3562-3742` `run_wave` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3751-3785` `partition_wave` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3790-3800` `partition_requested` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3805-3829` `run_batch` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3831-3884` `start_and_run_stage` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3886-3994` `run_stage` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:3999-4016` `stage_paused_for_review` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4028-4039` `select_review_panel` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4051-4073` `log_review_tier` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4087-4105` `assert_isolated_cwd` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4120-4181` `reviewer_spawn_opts` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4218-4233` `review_round_start_sha` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4303-4464` `review_unit` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4493-4547` `guard_review_round_tree` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4574-4592` `guard_review_round_tree_on_tier_err` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4629-4690` `spawn_sdet_author` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4701-4725` `halted_spawn_checkpoint_permitted` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:4731-5497` `run_single_stage` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:5503-5510` `effective_speculation_width` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:5519-5526` `speculates` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:5535-5551` `speculation_lane_worktree` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:5572-5940` `run_speculation` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:5953-6044` `emit_speculation_winner_status` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:6063-6100` `record_speculation_reject` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:6110-6143` `cancel_speculation_candidates` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:6145-6198` `run_fan_out_stage` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:6208-6374` `run_fan_out_review_loop` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:6405-6457` `run_review_agents_concurrently` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:6469-6500` `run_lens` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:6539-6694` `run_reviewer` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:6730-6747` `gating_spawn_emitted_approve` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:6769-6779` `review_spawn_errored` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:6803-6836` `reviewer_result_is_degenerate` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:6851-6881` `run_adversary` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:6899-6944` `run_adjudicator` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:6966-6987` `dag_unit_blast_radii` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:6996-7075` `build_dag_critique_prompt` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:7084-7190` `re_plan` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:7201-7234` `run_plan_critique_gate` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:7244-7410` `plan_critique_loop` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:7438-7449` `build_env` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:7466-7473` `run_base_env` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:7498-7504` `spawn_env` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:7515-7520` `build_budget` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:7545-7553` `shared_build_cache_paths` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:7564-7820` `run_gates` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:7836-7927` `run_gate_with_taxonomy` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:7984-7991` `gc_integrated_branches` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:7998-8101` `gc_integrated_branches_logged` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:8137-8144` `reclaim_terminal_unit_mutation_scratch` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:8174-8360` `run_deferred_gates` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:8371-8438` `record_gate` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:8496-9063` `integrate_and_emit` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9105-9112` `integrate_plan_commits` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9114-9234` `integrate_plan_commits_inner` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9245-9261` `record_plan_intent` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9272-9300` `read_plan_landed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9308-9337` `record_plan_landed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9342-9348` `regenerate_rule_for` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9373-9410` `run_regenerate_command` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9429-9454` `regenerate_conflicted_paths` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9482-9494` `catch_up_owed_regeneration` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9506-9518` `regenerate_and_record` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9523-9529` `regenerate_pending_for` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9536-9544` `union_regenerate_pending` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9571-9599` `record_regenerate_pending` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9608-9622` `record_placeholder_staged` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9632-9649` `record_regenerate_commit` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9659-9673` `record_merge_attempt` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9679-9698` `record_merge_outcome` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9705-9720` `record_landing_intent` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9730-9745` `record_landed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9756-9774` `record_integrate_row` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9788-9850` `spawn_conflict_resolution_implementer` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9860-9862` `build_system_prompt` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9872-9884` `ground_query` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9890-9900` `implementer_agent` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9904-9928` `plan_protocol` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:9956-9990` `grounded_blast_radius` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10006-10019` `grounded_seed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10043-10072` `record_blast_radius` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10074-10080` `build_prompt` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10091-10093` `build_review_prompt` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10101-10135` `build_prompt_with_failure` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10137-10201` `graph_context` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10214-10222` `ingest_project_into_graph` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10230-10289` `ingest_project_batches` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10294-10294` `ingest_project_into_graph` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10308-10316` `ingest_files_into_graph` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10320-10320` `ingest_files_into_graph` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10322-10337` `emit_lesson` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10350-10391` `land_refused` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10396-10402` `agent_isolated` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10525-10633` `adopt_prior_criterion_branch` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10655-10696` `resume_phase` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10698-10730` `stage_worktree` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10745-10766` `review_only_worktree` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10778-10783` `template_gates` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:10785-11186` `harvest_proposed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `crates/rigger-conductor/src/conductor.rs:11208-11230` `resolve_served_criterion` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
- `conductor::schedule` (3 functions)
  - `crates/rigger-conductor/src/conductor.rs:1421-1427` `plan_landing_failed` - name contains "plan" (unit scheduling); grouped under `conductor::schedule`.
  - `crates/rigger-conductor/src/conductor.rs:11277-11279` `normalize_criterion_id` - name contains "criterion" (unit scheduling); grouped under `conductor::schedule`.
  - `crates/rigger-conductor/src/conductor.rs:12383-12443` `prior_criterion_unit` - name contains "criterion" (unit scheduling); grouped under `conductor::schedule`.
- `conductor::spawn` (3 functions)
  - `crates/rigger-conductor/src/conductor.rs:1232-1234` `is_parked` - name contains "parked" (spawn lifecycle); grouped under `conductor::spawn`.
  - `crates/rigger-conductor/src/conductor.rs:1294-1309` `spawn_halt` - name contains "spawn" (spawn lifecycle); grouped under `conductor::spawn`.
  - `crates/rigger-conductor/src/conductor.rs:1323-1325` `is_parked_or_budget_refused` - name contains "parked" (spawn lifecycle); grouped under `conductor::spawn`.
- `conductor::support` (14 functions)
  - `crates/rigger-conductor/src/conductor.rs:884-917` `conflict_regenerate_pending_from_log` - name contains "from_" (conversion helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:996-1019` `landings_from_log` - name contains "from_" (conversion helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:1043-1074` `integrate_attempted_from_log` - name contains "from_" (conversion helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:2391-2393` `has_producer` - name contains "has_" (predicate helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:11403-11408` `build_system_prompt` - name contains "build" (generic construction helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:11679-11823` `write_capped_section` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:11874-11930` `write_code_neighborhood` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:12043-12126` `write_design_intent` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:12132-12147` `write_capped_decisions` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:12152-12169` `write_capped_lessons` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:12177-12202` `write_capped_findings` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:12211-12219` `write_peers_pointer` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:12234-12244` `write_lookup_pointer` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `crates/rigger-conductor/src/conductor.rs:12495-12508` `branch_is_foreign` - name contains "is_" (predicate helper); grouped under `conductor::support`.
- `conductor::tests` (550 functions)
  - `crates/rigger-conductor/src/conductor.rs:2723-2774` `for_test` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13157-13161` `agent_failure_as_str_round_trips_through_from_category_for_every_category` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13164-13169` `agent_failure_from_category_degrades_an_unrecognized_string_to_unknown` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13172-13174` `agent_failure_default_is_unknown` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13177-13182` `classify_failure_prefers_the_stopfailure_record_over_api_retry` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13185-13190` `classify_failure_falls_back_to_the_last_api_retry_category` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13193-13195` `classify_failure_falls_back_to_unknown_when_neither_source_has_anything` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13198-13212` `strip_failure_marker_drops_the_class_prefix_leaving_only_the_message` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13215-13218` `strip_failure_marker_passes_through_an_unmarked_error_unchanged` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13221-13240` `unit_worktree_dir_derives_deterministically_from_scratch_root_and_unit_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13243-13333` `recorded_gate_outcome_reads_the_latest_gate_run_verdict_per_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13246-13258` `verdict` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13337-13343` `started_with_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13345-13350` `integrated` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13353-13361` `prior_criterion_unit_finds_a_prior_un_integrated_units_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13364-13390` `prior_criterion_unit_never_returns_an_integrated_units_id_and_never_falls_back_to_an_older_sibling` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13393-13417` `prior_criterion_unit_integration_of_one_criterion_never_masks_an_abandoned_sibling_criterion_sharing_the_same_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13420-13433` `prior_criterion_unit_tie_break_prefers_the_most_recent_of_two_non_integrated_priors` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13436-13441` `prior_criterion_unit_excludes_this_unit_itself` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13444-13456` `prior_criterion_unit_ignores_a_different_criterion_and_an_empty_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13463-13468` `run_started_with_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13473-13475` `compensated` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13480-13486` `plain_failure` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13490-13502` `assert_prior_criterion_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13582-13600` `adoption_status` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13603-13628` `recorded_adoption_ignores_a_same_identity_event_carrying_the_wrong_status` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13631-13664` `recorded_adoption_never_answers_for_a_mismatched_criterion_or_a_mismatched_spec_alone` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13667-13672` `branch_owner_returns_none_for_an_id_that_never_started` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13675-13691` `branch_owner_reads_the_most_recent_started_criterion_and_spec_for_this_bare_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13694-13709` `branch_owner_ignores_a_non_unit_started_event_even_when_it_shares_the_id_field` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13712-13725` `branch_is_foreign_is_false_when_nothing_is_recorded_or_everything_matches` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13728-13750` `branch_is_foreign_is_false_when_the_recorded_owner_has_no_criterion_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13753-13775` `branch_is_foreign_when_only_one_axis_differs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13780-13792` `find_unit_started` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13795-13881` `a_fresh_units_own_branch_adopts_a_prior_runs_un_integrated_unit_sharing_the_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13884-13949` `a_fresh_unit_never_adopts_a_criterion_whose_prior_attempt_already_integrated` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13952-14096` `a_halted_spawns_uncommitted_tree_is_captured_as_a_wip_commit_and_named_in_the_next_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14100-14110` `dirty_unit_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14115-14150` `assert_no_wip_recovery_commit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14153-14160` `park_implementer` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14163-14187` `a_dirty_tree_whose_named_spawn_was_never_requested_gets_no_wip_recovery_commit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14190-14225` `a_dirty_tree_gets_no_wip_recovery_commit_while_a_sibling_spawn_of_the_unit_is_still_live` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14228-14262` `a_prior_runs_leftover_spawn_request_for_a_same_named_unit_never_halts_a_new_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14274-14289` `prior_failure_summary_names_only_the_halted_commit_when_it_is_the_sole_failure` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14292-14315` `prior_failure_block_names_only_the_halted_commit_when_it_is_the_sole_failure` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14318-14352` `prior_failure_block_adds_the_generic_preamble_for_review_reject_or_contradiction_alone` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14355-14386` `review_worktree_dir_and_branch_derive_from_stage_and_attempt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14499-14504` `answering` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14506-14534` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14537-14539` `prompts_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14542-14544` `dirs_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14548-14550` `system_prompt_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14554-14556` `title_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14560-14562` `reviews_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14566-14568` `spawn_ids` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14572-14578` `spawned` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14584-14591` `dir_existed_when_spawned` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14594-14734` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14738-14742` `integrates_a_passing_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14745-14752` `one_stage_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14756-14767` `uncovered_run_log` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14770-14780` `covers_criterion_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14845-14869` `planner_extends_the_dag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14872-14938` `planner_proposed_unit_with_a_coverage_criterion_runs_and_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:14941-15025` `conductor_creates_one_baseline_unit_per_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15028-15095` `a_stage_needing_the_fan_out_template_becomes_ready_once_every_criterion_unit_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15098-15220` `producer_prompt_carries_the_criteria_and_plan_protocol_grounded_on_the_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15225-15227` `baseline_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15232-15238` `supersede_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15244-15247` `append_proposal` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15250-15291` `a_prior_runs_proposal_never_resurrects_in_a_new_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15294-15368` `planner_unit_supersedes_the_matching_baseline` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15371-15447` `a_planner_supersede_of_a_fan_out_member_still_satisfies_its_downstream_needs_edge` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15450-15478` `a_criterion_with_no_planner_unit_keeps_its_baseline` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15481-15546` `planner_refinement_split_is_still_harvested` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15549-15674` `a_same_id_re_emit_updates_the_proposed_unit_in_place` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15681-15711` `seed_refine_dag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15715-15735` `append_proposals` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15738-15803` `a_coverage_retarget_re_emit_preserves_one_unit_per_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15806-15858` `a_re_emit_naming_a_reserved_stage_never_mutates_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15861-15918` `re_folding_a_same_id_re_emit_is_idempotent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:15921-16020` `a_real_split_two_distinct_ids_both_survive_the_fold` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16024-16032` `proposal_data` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16036-16042` `proposal_event` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16047-16065` `harvest_seeded` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16070-16113` `assert_later_episode_supersedes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16181-16207` `a_same_episode_re_seen_on_a_later_fold_still_never_self_supersedes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16216-16265` `assert_refine_survives_its_own_episodes_sibling` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16300-16405` `a_resume_catch_up_over_two_episode_supersession_and_a_split_matches_a_live_incremental_fold` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16408-16507` `a_legacy_history_resume_catch_up_matches_a_live_incremental_fold_mutual_siblings_then_superseded` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16510-16553` `a_legacy_proposal_never_supersedes_an_identified_episodes_owner_even_when_logged_later` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16556-16647` `a_late_re_emit_never_mutates_a_started_or_terminal_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16651-16670` `append_gated_proposal` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16673-16752` `harvest_proposed_gates_every_case_with_the_templates_list_unioned` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16755-16793` `harvest_proposed_gate_inheritance_survives_a_resumed_window` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16796-16813` `plan_protocol_tells_the_planner_gates_come_from_the_template` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16819-16828` `has_unmatched_signal` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16841-16895` `assert_supersedes_criterion_a_once` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:16967-17017` `a_genuinely_new_proposal_runs_and_records_an_unmatched_signal` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17020-17077` `a_genuinely_new_proposal_with_no_gates_still_spawns_gated_via_template_inheritance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17080-17203` `resume_dedups_baselines_before_running_them` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17206-17277` `decomposes_the_real_spec_01_into_per_criterion_units` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17280-17310` `ratchet_promotes_a_reliable_gate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17313-17366` `elevated_gate_is_never_promoted_to_silent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17369-17406` `learns_from_escalation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17409-17459` `feeds_graph_decisions_into_the_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17462-17541` `lookup_pointer_names_all_three_verbs_on_every_slice` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17544-17639` `decisions_prompt_injection_is_capped_under_budget_with_elision_note` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17644-17665` `render_capped_decisions` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17669-17677` `fold_subgraph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17680-17690` `render_capped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17693-17723` `a_superseded_decision_never_outranks_a_current_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17726-17779` `grounding_still_surfaces_a_prior_run_decision_that_peers_labels_historical` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17782-17809` `the_verbatim_count_cap_binds_on_many_small_decisions` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17812-17844` `the_byte_budget_cap_binds_before_the_count_on_chunky_decisions` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17847-17914` `a_kept_decision_restores_the_dropped_dependency_it_supersedes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17920-17930` `render_capped_findings` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17937-17971` `assert_capped_with_elision` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:17974-18017` `findings_prompt_injection_is_capped_under_budget_with_elision_note` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18020-18055` `the_verbatim_count_cap_binds_on_many_small_findings` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18058-18163` `grounding_omits_a_resolved_finding_and_keeps_the_open_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18169-18181` `render_capped_lessons` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18184-18241` `the_injected_slice_is_deduplicated_by_normalized_text` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18244-18386` `dedup_and_restore_are_render_only_no_event_no_projection_mutation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18404-18540` `structural_grounding_is_one_seeded_traversal_over_the_unified_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18557-18671` `the_grounding_path_populates_the_unified_graph_from_the_live_project` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18686-18775` `re_ingesting_re_extracts_a_changed_file_and_skips_unchanged_ones` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18786-18869` `ingest_files_into_graph_is_bounded_to_the_named_files_and_reflects_their_live_content` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18890-18970` `re_excluding_the_same_file_twice_in_one_process_retires_its_middle_generation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:18983-19100` `a_second_run_over_an_unchanged_tree_appends_no_derived_index_event` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19118-19136` `spec60_cold_rebuild` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19148-19179` `spec60_reachable` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19185-19194` `spec60_reached_entities` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19209-19388` `editing_one_file_between_runs_re_emits_only_that_files_batch_and_supersedes_its_edges` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19401-19557` `a_file_reverted_to_an_earlier_recorded_generation_re_ingests_and_matches_a_cold_rebuild` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19581-19704` `a_prior_runs_non_ingest_replay_key_never_suppresses_this_runs_keyed_emit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19716-19805` `the_ingest_sink_appends_and_folds_once_per_file_batch_never_once_per_event` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19726-19734` `append` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19828-19987` `design_intent_grounding_renders_the_governing_rule_and_specifying_ra_by_traversal` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:19990-20060` `a_governing_decision_never_leaks_into_the_design_intent_section` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20063-20119` `the_design_intent_section_renders_the_newest_binding_and_elides_the_oldest` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20124-20144` `assert_renders_no_section` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20200-20226` `render_code_neighborhood` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20229-20265` `the_code_neighborhood_elision_note_names_graph_around_recovery_not_peers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20268-20313` `the_code_neighborhood_byte_cap_elides_before_the_count_cap_is_reached` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20316-20358` `lessons_prompt_injection_is_capped_under_budget_with_elision_note` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20361-20398` `the_verbatim_count_cap_binds_on_many_small_lessons` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20404-20417` `render_capped_lessons_scoped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20420-20457` `lessons_rank_by_blast_radius_relevance_over_pure_recency` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20460-20481` `resume_skips_already_integrated_units` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20485-20502` `seed_events_in_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20505-20559` `replay_trajectory_keeps_only_the_world_inputs_and_restrips_the_run_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20564-20566` `commit_on_unit_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20569-20574` `unit_event` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20579-20584` `seed_prior_window_s` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20588-20614` `reviewed_merge_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20619-20629` `approving_panel_stub` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20634-20656` `assert_resume_integrates_s` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20659-20688` `resume_reuses_a_units_branch_instead_of_reimplementing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20695-20727` `assert_branch_gc_resume` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20730-20754` `branch_gc_reclaims_integrated_units_and_retains_escalated_ones_on_resume` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20758-20775` `seed_branch_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20782-20790` `commit_on_named_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20794-20802` `seed_unit_branches` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20805-20811` `started_on_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20814-20819` `escalated` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20823-20839` `integrated_with_straggler` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20842-20849` `resume_in` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20853-20868` `gc_logged` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20883-20919` `branch_gc_falls_back_to_the_derived_branch_when_unitstarted_recorded_no_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20922-20982` `branch_gc_reclaims_the_recorded_branch_not_the_derived_name_when_they_differ` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:20993-21017` `seed_lingering_worktree_on_unit_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21023-21028` `worktree_registered_on` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21031-21123` `branch_gc_removes_a_lingering_worktree_before_reclaiming_the_branch_on_resume` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21126-21136` `branch_gc_reclaims_every_integrated_unit_in_one_resume_not_just_the_first` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21139-21184` `branch_gc_fences_reclaim_behind_an_in_flight_straggler_spawn_and_reclaims_once_it_answers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21187-21217` `gc_integrated_branches_logged_prints_kept_evidence_for_an_in_flight_straggler_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21220-21266` `gc_integrated_branches_logged_prints_removing_evidence_for_a_terminal_spawns_decision` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21269-21308` `gc_integrated_branches_logged_stays_silent_for_an_already_gone_worktree_but_still_reclaims_an_orphaned_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21311-21360` `gc_integrated_branches_logged_does_not_repeat_removing_evidence_once_the_real_removal_already_happened` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21363-21409` `review_boundary_events_carry_the_worktree_sha` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21412-21456` `a_review_reject_unitfailed_carries_the_worktree_sha` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21459-21498` `an_exhaustive_gate_failure_on_an_approved_unit_records_a_gate_cause` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21501-21618` `stamps_the_model_alias_and_resolved_id_on_live_lifecycle_events` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21624-21640` `degenerate_reviewer_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21643-21704` `a_degenerate_adjudicator_result_respawns_and_a_substantive_retry_folds_normally` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21707-21819` `a_review_stage_error_result_re_parks_a_fresh_attempt_no_charge_then_a_real_verdict_folds` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21822-21956` `a_review_spawn_that_errors_on_every_re_parked_attempt_escalates_through_the_bound` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:21959-22044` `a_gating_spawn_that_emits_an_approve_verdict_but_returns_no_verdict_line_hard_errors_with_the_result_channel_fix` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22049-22071` `assert_ordinary_reject` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22105-22203` `the_workflow_live_path_correlates_the_approve_by_stamp_not_a_bare_stream_position` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22206-22251` `a_degenerate_lens_result_respawns_the_lens_before_the_review_proceeds` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22254-22307` `a_lens_that_emitted_a_finding_but_reports_empty_stdout_is_not_degenerate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22310-22399` `a_reviewer_that_only_ever_returns_degenerate_output_halts_the_run_loudly_naming_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22403-22408` `approved_but_unmerged_s` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22411-22432` `resume_integrates_an_already_approved_unit_without_re_reviewing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22435-22504` `a_failed_unit_is_not_terminal_and_resumes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22507-22611` `remediation_attempts_accumulate_across_resume_and_escalate_at_the_bound` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22614-22691` `an_escalated_unit_stays_terminal_on_resume` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22694-22744` `a_fresh_unit_with_no_branch_runs_the_full_lifecycle` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22747-22791` `agent_decision_folds_content_but_no_agent_attribution` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22796-22827` `planner_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22830-22844` `scope_creep_refuses_a_criterionless_proposed_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22847-22869` `adjudicator_reject_blocks_the_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22873-22883` `approving_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22887-22909` `assert_three_tier_order` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22912-22940` `adversary_runs_between_the_lenses_and_the_adjudicator` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22943-22982` `adjudicator_reject_gates_even_with_an_adversary_present` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:22985-23042` `unit_reviews_itself_within_its_own_lifecycle` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23045-23055` `depth_tiers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23059-23066` `full_panel_with_tiers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23068-23070` `strs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23073-23099` `path_is_high_risk_matches_by_prefix_and_by_glob` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23102-23111` `route_review_tier_with_no_policy_is_the_full_panel_unchanged` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23114-23133` `route_review_tier_routes_a_low_risk_unit_to_the_light_panel` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23136-23151` `route_review_tier_forces_full_on_a_high_risk_path_hit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23154-23167` `route_review_tier_forces_full_over_the_size_threshold` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23170-23183` `route_review_tier_forces_full_when_the_gates_flapped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23186-23220` `route_review_tier_fails_safe_to_full_on_an_empty_blast_radius` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23226-23275` `run_tiered_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23279-23290` `logged_review_tier` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23293-23319` `a_low_risk_unit_runs_the_light_panel_and_logs_the_routing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23327-23368` `tiered_review_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23371-23388` `a_low_risk_unit_skips_the_adversary_and_extra_lens` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23391-23417` `a_high_risk_unit_runs_the_full_panel_and_logs_the_routing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23420-23435` `without_a_depth_policy_a_grounded_unit_runs_the_full_panel_and_logs_no_routing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23438-23474` `a_zero_grounding_unit_fails_safe_to_the_full_panel_and_logs_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23477-23586` `a_flapped_unit_escalates_from_light_at_attempt_0_to_full_on_remediation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23589-23609` `a_stage_level_tiers_policy_routes_the_unit_by_risk` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23612-23686` `every_spawn_runs_in_a_worktree_never_the_main_repo_checkout` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23690-23713` `spec_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23716-23803` `speculation_width_2_runs_two_candidates_first_green_wins_rest_cancelled` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23806-23837` `speculation_budget_accounts_for_every_candidate_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23840-23876` `speculation_defaults_off_runs_a_single_candidate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23879-23914` `speculation_parks_all_candidates_together_in_one_step` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:23917-24039` `speculation_defers_green_until_the_winner_and_integrates_across_replay_steps` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24045-24055` `unit_has_status` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24059-24061` `branch_present` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24064-24133` `speculation_later_candidate_wins_when_an_earlier_lane_is_review_rejected` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24136-24254` `speculation_low_risk_later_lane_winner_routes_light_not_falsely_flapped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24257-24308` `speculation_rejected_loser_is_visible_to_the_review_quality_fold` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24311-24373` `speculation_escalates_when_every_candidate_is_rejected` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24376-24436` `speculation_crashed_lane_is_absent_and_a_sibling_still_wins` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24439-24501` `speculation_exhaustive_integrate_door_red_blocks_a_candidate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24504-24611` `speculation_stays_fresh_across_parking_until_a_winner_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24626-24693` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24697-24853` `speculation_blocked_winner_captures_evidence_and_a_later_lane_wins` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24856-24897` `assert_isolated_cwd_refuses_empty_or_repo_root_with_a_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24900-24980` `the_conductor_threads_each_agents_persona_to_the_driver` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:24983-25029` `the_system_prompt_carries_the_rigger_communication_discipline` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25032-25068` `an_agent_with_no_persona_threads_an_empty_system_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25071-25137` `operator_instructions_reach_every_spawned_agent_between_persona_and_discipline` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25140-25194` `planner_proposed_unit_inherits_the_default_review_panel` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25197-25284` `a_producer_stage_skips_the_three_tier_review_and_unblocks_its_dependents` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25287-25364` `plan_stage_commit_under_specs_reaches_the_run_branch_before_the_next_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25367-25437` `plan_stage_commit_outside_specs_fails_the_stage_naming_the_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25440-25519` `plan_stage_commit_reverting_its_own_out_of_scope_touch_still_fails_the_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25522-25588` `plan_stage_commit_conflicting_with_a_concurrent_specs_change_escalates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25591-25711` `integrate_plan_commits_is_idempotent_on_a_resumed_already_landed_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25714-25770` `integrate_plan_commits_tolerates_a_pre_existing_intent_record_with_no_git_mutation_yet` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25773-25846` `integrate_plan_commits_keeps_the_earlier_commits_identity_when_the_worktree_grows_between_calls` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25859-25875` `append` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25880-25938` `integrate_plan_commits_wraps_any_hard_error_with_the_plan_landing_marker` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:25941-26018` `a_plan_landing_infra_fault_halts_the_run_loudly_with_no_per_unit_lesson_or_attempt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26022-26048` `per_unit_panel_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26051-26081` `per_unit_adjudicator_reject_blocks_integration_and_escalates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26084-26166` `an_always_rejecting_adjudicator_escalates_after_exactly_max_retries_cycles` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26169-26230` `a_higher_max_retries_gives_more_attempts_before_escalation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26233-26286` `an_absent_max_retries_preserves_the_default_bound_of_three` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26289-26382` `a_resumed_unit_gets_exactly_its_granted_extra_attempts_before_re_escalating` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26385-26418` `max_retries_for_widens_only_the_resumed_unit_never_a_sibling` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26421-26460` `a_stages_own_max_retries_overrides_the_run_default_for_its_units` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26478-26484` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26487-26520` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26529-26553` `assert_final_attempt_approval_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26556-26570` `approval_on_the_final_permitted_attempt_integrates_a_per_unit_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26573-26655` `a_model_ladder_implementer_escalates_one_rung_per_remediation_attempt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26658-26686` `approval_on_the_final_permitted_attempt_integrates_a_standalone_review_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26690-26708` `crashing_spawn_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26711-26719` `mid_spawn_crash_escalates_without_aborting_the_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26722-26730` `gated_by_ok` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26733-26739` `agent_a_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26743-26747` `budget_of_one_over_two_units` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26751-26753` `assert_attention` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26800-26874` `a_budget_halt_does_not_restamp_on_a_later_poll_with_nothing_new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26877-26881` `started_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26885-26889` `replay_step` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26892-26898` `fail_implementer` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26901-26965` `a_delayed_budget_halt_after_a_dependency_unlocks_still_stamps` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:26968-27013` `compute_attention_leaves_the_hung_liveness_halt_to_the_caller` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27017-27027` `parked_unit_u` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27030-27070` `an_escalation_does_not_restamp_attention_on_a_resumed_process` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27073-27084` `a_clean_step_stamps_no_attention` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27087-27161` `a_second_failure_recurs_and_a_third_also_stalls_the_frontier` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27164-27200` `nine_of_ten_budget_crosses_the_final_tenth` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27203-27275` `a_re_step_already_past_the_budget_threshold_does_not_re_stamp` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27278-27282` `budget_of_one_over_a_chain` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27285-27299` `budget_breaker_stops_the_run_after_the_first_wave` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27302-27308` `budget_exhaustion_aborts_the_task` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27311-27321` `a_budget_halt_surfaces_its_reason_on_the_run_state` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27324-27334` `a_run_within_budget_surfaces_no_halt_reason` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27338-27354` `with_budget_two_ctx_over` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27357-27400` `the_spawn_budget_folds_from_recorded_spawn_requests_across_steps` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27403-27424` `the_pre_wave_breaker_trips_only_on_a_new_over_budget_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27427-27490` `a_review_tier_budget_refusal_aborts_with_budgetexhausted_not_a_raw_error` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27505-27514` `run_ungated_fanout_guard` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27517-27545` `ungated_fanout_unit_under_a_gated_template_fails_the_step` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27548-27571` `ungated_fanout_unit_under_an_ungated_template_is_not_flagged` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27574-27595` `gated_fanout_unit_passes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27598-27622` `non_fanout_stage_with_no_gates_is_never_flagged` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27625-27660` `unmatched_fanout_proposal_under_a_gated_template_fails_the_step` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27663-27687` `unmatched_fanout_proposal_under_an_ungated_template_is_not_flagged` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27690-27707` `ungated_fan_out_templates_names_a_gateless_template` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27710-27722` `ungated_fan_out_templates_is_silent_on_a_gated_template` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27725-27738` `planner_covering_every_criterion_passes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27741-27769` `manual_stage_pauses_while_an_auto_stage_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27773-27794` `spawned_in_a_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27797-27815` `isolation_none_agent_gets_no_worktree_even_with_a_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27818-27826` `spawn_opts_isolation_is_set_for_a_worktree_agent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27829-27848` `a_spawned_implementers_title_is_the_unit_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27855-27884` `roster_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27888-27904` `per_unit_roster_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27908-27915` `assert_full_panel_roster` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27940-27952` `a_panel_with_no_adversary_never_fabricates_one_in_the_adjudicators_roster` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:27961-28024` `the_roster_reflects_the_actually_routed_light_panel_not_the_full_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28038-28069` `the_fan_out_review_loops_adversary_and_adjudicator_spawns_are_stamped_with_the_routed_roster` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28072-28135` `stage_autonomy_override_seeds_the_gate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28138-28178` `on_pass_none_runs_gates_but_does_not_integrate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28189-28204` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28213-28242` `two_unit_gate_runs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28247-28276` `assert_one_isolated_dir_per_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28279-28307` `two_units_gate_environments_never_share_a_target_dir` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28310-28322` `two_units_gate_environments_never_share_a_mutants_root` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28325-28371` `an_implement_stage_gate_round_creates_no_mutants_directory` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28380-28384` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28387-28396` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28404-28531` `one_build_environment_authority_reaches_both_a_gate_build_and_an_agent_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28419-28456` `run_once` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28534-28578` `spawn_env_adds_the_per_unit_cargo_target_dir_only_for_a_real_unit_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28581-28644` `a_units_per_unit_cache_is_reclaimed_when_its_worktree_is_removed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28647-28719` `a_units_worktree_cache_and_branch_are_all_reclaimed_on_a_successful_integrate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28722-28782` `a_units_worktree_is_reclaimed_but_its_branch_survives_a_terminal_escalation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28786-28811` `judged_unit_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28816-28822` `parking_stub` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28825-28831` `parked_judge_stub` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28835-28843` `unit_branch_tip` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28849-28863` `assert_unit_worktree_at` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28866-28915` `a_parked_review_spawn_keeps_the_unit_worktree_registered_and_its_cache` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28918-28958` `review_unit_restores_a_worktree_a_gate_deleted_out_of_band` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:28961-29016` `a_second_step_restores_a_still_parked_units_worktree_deleted_out_of_band` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29019-29117` `verified_worktree_sha_is_stamped_after_a_gate_side_deletion_is_restored` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29120-29193` `review_tier_boundary_restores_a_worktree_a_prior_tier_deleted` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29196-29300` `reviewed_worktree_sha_is_stamped_after_the_adjudicators_own_deletion_is_restored` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29303-29307` `has_real_worktree_sha` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29311-29333` `rejecting_deleting_judge_log` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29336-29353` `failed_worktree_sha_is_stamped_after_the_adjudicators_own_deletion_is_restored` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29357-29367` `status_sha` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29373-29397` `assert_residue_named_and_never_merged` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29402-29427` `approving_round_with_residue` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29430-29471` `a_review_rounds_dirty_residue_is_restored_named_and_never_merged` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29474-29498` `a_review_rounds_moved_tip_is_restored_to_the_reviewed_sha_before_merging` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29501-29631` `a_review_rounds_lens_residue_survives_a_later_tiers_genuine_crash_and_is_restored` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29637-29645` `residue_then_park_stub` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29652-29672` `park_then_resume` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29678-29700` `assert_one_log_derived_round_start` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29710-29758` `assert_resumed_round_judges_the_logged_start` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29811-29874` `a_speculation_lanes_log_derived_start_sha_survives_a_cross_call_resume_after_a_park` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29877-29917` `a_resumed_reviewed_unit_restores_a_worktree_the_exhaustive_gate_deleted` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:29920-30000` `a_resumed_reviewed_unit_whose_exhaustive_gate_fails_records_a_gate_cause` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30003-30126` `a_resumed_reviewed_units_merge_break_records_an_integrate_conflict_cause` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30129-30259` `a_resumed_landed_but_ungated_unit_regates_the_landed_tree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30262-30433` `a_resumed_reviewed_units_genuine_unresolved_conflict_reaches_the_idempotent_merge_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30450-30491` `regenerate_conflicted_paths_returns_the_real_regeneration_commit_sha` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30494-30568` `a_live_approved_unit_restores_a_worktree_the_integrate_door_exhaustive_gate_deleted` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30571-30688` `speculation_lenses_restore_a_gate_deleted_worktree_without_racing_and_stamp_the_winner_sha` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30691-30695` `speculating_judged_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30698-30719` `speculation_reject_worktree_sha_is_stamped_after_the_adjudicators_own_deletion_is_restored` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30722-30791` `speculation_winner_reviewed_sha_stays_the_round_start_sha_across_a_post_review_regen_commit_and_integrate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30794-30904` `a_parked_lens_keeps_the_unit_worktree_beside_a_genuine_sibling_crash` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30907-30952` `a_worktree_less_stage_gate_inherits_the_shared_target` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30955-30981` `bounded_pool_completes_every_stage_under_the_cap` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:30984-31088` `run_wave_admits_at_most_max_parallel_units_leaving_the_rest_neither_failed_nor_terminal` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31091-31164` `occupancy_survives_a_crash_resume_so_a_still_parked_unit_keeps_its_slot_over_a_fresh_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31167-31202` `live_run_folds_no_gate_machinery` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31207-31217` `spawned_parallel` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31220-31243` `agent_stage_runs_the_per_unit_lifecycle_not_the_fan_out_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31246-31266` `standalone_review_stage_still_takes_the_fan_out_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31269-31378` `run_gates_derives_and_injects_the_review_worktrees_store_fence` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31381-31385` `review_lens_ids` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31390-31423` `standalone_review_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31427-31436` `assert_review_worktree_kept` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31440-31447` `assert_genuine_crash_halts` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31450-31471` `a_parked_lens_keeps_the_standalone_review_stages_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31474-31529` `a_parked_lens_keeps_the_review_worktree_even_beside_a_lower_indexed_sibling_crash` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31532-31587` `a_parked_lens_keeps_the_review_worktree_beside_a_sibling_degenerate_halt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31590-31630` `the_swap_to_front_prioritizes_a_genuine_error_at_a_non_zero_chunk_index` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31633-31677` `a_budget_refused_lens_beside_a_genuinely_crashing_sibling_in_one_chunk` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31680-31710` `a_budget_refused_standalone_review_spawn_keeps_its_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31713-31740` `partition_separates_overlapping_blast_radii` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31743-31765` `blast_radius_conflicts_flags_units_that_split_one_file_set` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31768-31781` `blast_radius_conflicts_is_empty_for_a_disjoint_partition` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31784-31832` `partitioned_wave_still_integrates_every_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31844-31868` `run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31872-31981` `lens_finding_reaches_later_tiers_through_the_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:31984-32034` `review_agents_emit_findings_via_the_review_protocol` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32037-32067` `unparseable_adjudicator_output_blocks_integration` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32070-32131` `failing_gate_evidence_threaded_into_retry_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32136-32164` `silent_gate_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32167-32174` `any_verdict` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32177-32232` `a_flaky_gate_rerun_is_a_pass_with_warning_that_never_demotes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32235-32275` `a_product_gate_failure_is_not_rerun_and_demotes_as_before` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32278-32356` `an_authored_infra_rule_with_limit_zero_at_an_inline_gate_holds_the_ratchet` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32359-32454` `an_inline_gate_red_across_every_rerun_holds_for_infra_and_demotes_for_flaky` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32457-32474` `unit_evidence_is_populated_after_a_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32477-32544` `adjudicator_rejection_reasoning_threaded_into_retry_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32547-32606` `fan_out_lens_does_not_integrate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32609-32650` `review_only_stage_records_no_artifact_truthfully` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32653-32703` `two_erroring_stages_both_leave_a_record` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32706-32762` `single_wide_wave_overruns_budget_and_is_stopped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32765-32779` `validate_acyclic_detects_a_cycle` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32841-32854` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32856-32862` `with_side_effect` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32863-32865` `calls` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32866-32868` `targets` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32869-32871` `mutants_dirs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32872-32874` `store_fences` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32875-32877` `build_cache_guards` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32878-32880` `build_cache_dirs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32883-32933` `run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32946-32983` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:32994-32999` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33000-33002` `calls` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33005-33025` `run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33029-33038` `gate_verdict_event` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33040-33044` `verdict_passed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33050-33080` `identical_tree_attempts` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33083-33113` `a_matching_input_digest_answers_a_gate_as_a_logged_cache_hit_citing_the_prior_green` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33116-33158` `a_content_change_misses_the_cache_and_re_runs_the_gate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33161-33182` `a_red_verdict_is_never_cache_answered_and_the_gate_re_runs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33191-33203` `ground` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33215-33230` `ground` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33231-33233` `blast_radius` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33234-33236` `index_stamp` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33240-33259` `blast_radius_audits` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33262-33271` `review_tier_evidence` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33276-33299` `tiered_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33301-33308` `tiered_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33318-33371` `assert_beyond_cap_high_risk_routes_full` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33381-33398` `a_structural_grounder_records_the_audit_and_routes_full_on_a_beyond_cap_high_risk_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33405-33445` `the_empty_radius_fail_safe_still_records_the_audit_and_routes_full` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33453-33487` `a_non_structural_grounder_records_no_blast_radius_audit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33499-33564` `confidence_tier_fixture` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33575-33626` `confidence_tier_radius_splits_the_one_edge_set_into_extracted_precise_and_all_tier_safe` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33636-33713` `grounded_blast_radius_tier_filters_the_subgraph_and_keeps_the_grep_superset` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33736-33829` `under_symbols_the_audit_emits_and_records_the_prompt_seed_as_precise` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33841-33915` `partition_wave_own_batches_an_empty_radius_never_co_scheduling_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33924-33935` `speculation_over_a_structural_grounder_records_the_audit_and_routes_full` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33938-33989` `staleness_flags_only_downstream_units_whose_radius_intersects_the_touched_files` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:33992-34044` `staleness_grounds_on_the_safe_superset_so_a_grep_only_reference_still_stales_a_downstream_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34047-34145` `rule6_conflict_detection_grounds_on_the_safe_superset_so_a_grep_only_shared_reference_conflicts` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34148-34178` `a_resume_reseeds_the_stale_set_from_the_prior_unitintegrated_marks` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34187-34228` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34232-34346` `integrating_a_unit_stales_the_intersecting_downstream_units_cached_verdict_not_the_rest` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34349-34375` `glob_matches_supports_star_doublestar_and_literals` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34378-34402` `gate_intersects_radius_runs_unscoped_and_intersecting_gates_only` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34405-34554` `blast_radius_narrows_the_inner_loop_and_the_integrate_step_runs_the_full_library` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34557-34685` `a_gate_skipped_inline_but_red_at_the_exhaustive_integrate_door_blocks_the_merge` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34688-34717` `verdict_compensates_names_a_prior_unit_independent_of_approval` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34730-34772` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34776-34946` `a_contradiction_compensates_reverts_and_re_enters_the_integrated_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34950-34968` `with_integrations_ctx` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:34971-35001` `commits_to_compensate_reverts_every_sha_a_multi_commit_landing_recorded` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35004-35055` `commits_to_compensate_dedupes_a_repeated_sha_and_skips_an_already_compensated_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35058-35088` `commits_to_compensate_excludes_the_review_only_marker_even_alongside_a_real_sha` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35091-35151` `pending_compensations_from_log_re_derives_only_undrained_marks` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35154-35185` `a_durable_compensation_queued_mark_is_fold_neutral_on_the_target` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35193-35230` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35238-35309` `assert_compensated_unit_re_gates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35338-35383` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35423-35447` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35451-35571` `a_resume_re_drives_a_durably_queued_but_undrained_compensation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35574-35746` `integrate_re_gates_the_merged_tree_and_a_merge_break_blocks_the_second_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35769-35836` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:35840-36000` `the_post_merge_re_gate_runs_in_its_own_scratch_worktree_never_the_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36017-36033` `append` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36043-36081` `assert_postmerge_err_reaps_its_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36084-36116` `postmerge_run_gates_err_still_reaps_the_throwaway_worktree_and_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36119-36155` `postmerge_worktree_create_err_still_reaps_the_just_created_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36158-36278` `conflict_regenerate_pending_from_log_re_derives_the_union_keyed_by_unit_and_attempt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36286-36307` `conflict_fixture` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36316-36371` `assert_both_conflicting_units_land` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36374-36394` `integrate_conflict_re_parks_the_implementer_with_no_attempt_charged_and_both_units_land` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36397-36419` `integrate_conflict_confined_to_a_regenerable_path_resolves_with_no_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36422-36521` `integrate_conflict_exhausted_after_the_bound_charges_a_real_attempt_with_the_unresolved_evidence` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36524-36672` `integrate_conflict_records_regenerate_pending_before_the_accept_incoming_mutation_that_can_fail` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36559-36602` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36678-36711` `deferred_gate_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36714-36716` `names_the_deferred_gate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36719-36752` `a_deferred_gate_runs_once_at_the_phase_boundary_not_inline` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36755-36776` `a_failing_deferred_gate_is_surfaced_and_the_run_is_not_done` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36787-36810` `run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36814-36847` `a_default_infra_fault_at_a_deferred_gate_does_not_demote` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36853-36871` `verify_only_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36879-36899` `replayed_twice` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36902-36907` `count_carrying` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36910-36912` `runs_of` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36915-36954` `a_replayed_step_re_runs_no_recorded_gate_and_appends_no_duplicate_events` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:36957-36999` `a_re_step_replays_a_recorded_deferred_gate_without_re_running_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37002-37146` `a_parked_step_never_records_a_partial_tree_deferred_verdict` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37149-37233` `a_manual_review_paused_unit_defers_the_whole_tree_gate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37236-37366` `an_escalated_dep_still_runs_the_whole_tree_deferred_gate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37369-37483` `a_recorded_failing_deferred_verdict_re_surfaces_its_failure_on_replay` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37486-37572` `a_replayed_fan_out_review_reject_appends_no_duplicate_unitfailed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37575-37618` `a_fan_out_review_stage_whose_gates_fail_after_approval_records_a_gate_cause` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37622-37624` `git_head` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37635-37661` `run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37665-37724` `gate_measures_the_committed_artifact_not_the_dirty_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37727-37846` `conductor_spawns_the_sdet_author_at_the_build_seam_so_its_tests_land_in_the_committed_tree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37849-37934` `the_sdet_author_spawn_respects_the_budget_breaker_at_its_own_spawn_site` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:37937-38034` `a_parked_sdet_author_spawn_holds_the_unit_instead_of_integrating_without_its_periphery_tests` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38037-38142` `speculation_sdet_author_periphery_lands_in_the_committed_tree_the_gates_judge` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38145-38237` `a_parked_sdet_author_in_a_speculation_candidate_holds_the_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38245-38283` `assert_sdet_seam_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38364-38459` `an_escalated_unit_does_not_integrate_its_code` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38467-38477` `critique_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38506-38517` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38523-38529` `rejecting` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38532-38582` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38586-38655` `a_blast_radius_overlap_alone_does_not_reject` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38668-38704` `the_plan_critique_gates_adversary_and_adjudicator_spawns_are_stamped_correctly` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38707-38744` `a_rule_7_or_8_defect_rejects_then_releases_on_the_revision` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38747-38798` `a_clean_decomposition_approves_and_releases_the_fan_out` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38801-38876` `the_gate_critiques_only_not_yet_run_units_and_approves` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38879-38916` `the_plan_critique_prompt_names_the_cross_unit_rules` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38919-38944` `the_critique_gate_never_enters_a_wave_even_when_ready` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38953-38987` `fan_out_needs_template_fixture` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:38990-39072` `a_downstream_stage_needing_the_fan_out_template_stays_unready_until_every_member_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39075-39179` `a_real_split_pair_must_both_integrate_not_just_the_btreemap_key_first_sibling` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39182-39256` `a_resolved_plan_critique_gate_does_not_re_run_on_resume` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39263-39273` `widget_split` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39278-39293` `critique_step` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39296-39351` `a_resumed_step_holds_the_fan_out_while_the_plan_critique_gate_is_escalated` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39354-39410` `an_approved_gate_releases_planner_proposed_units_not_only_baselines` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39413-39483` `the_re_plan_directive_instructs_reusing_the_existing_unit_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39496-39501` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39504-39524` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39528-39558` `a_resumed_step_holds_the_fan_out_while_the_plan_critique_gate_is_mid_review` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39561-39614` `a_parked_plan_critique_gate_keeps_its_review_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39617-39661` `a_budget_refused_plan_critique_gate_keeps_its_review_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:39674-39693` `the_conductors_one_event_authority_reports_a_write_the_store_lost` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
- `conductor::tests::support` (10 functions)
  - `crates/rigger-conductor/src/conductor.rs:13059-13061` `occurrences` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13064-13066` `snapshot` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13071-13085` `stub_deps` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13089-13097` `run_logged` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13100-13104` `stage_map` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13108-13122` `stage_shape` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13135-13138` `apply` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13139-13142` `apply_batch` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13143-13145` `subgraph` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `crates/rigger-conductor/src/conductor.rs:13146-13148` `resolve` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
- `conductor::throwaway` (1 function)
  - `crates/rigger-conductor/src/conductor.rs:12682-12688` `dir_and_branch` - method inside `impl Throwaway`; grouped with its other `Throwaway` methods.
- `dash::dash_marker` (4 functions)
  - `src/dash.rs:521-523` `serialize` - method inside `impl DashMarker`; grouped with its other `DashMarker` methods.
  - `src/dash.rs:529-534` `parse` - method inside `impl DashMarker`; grouped with its other `DashMarker` methods.
  - `src/dash.rs:538-540` `read` - method inside `impl DashMarker`; grouped with its other `DashMarker` methods.
  - `src/dash.rs:545-547` `write` - method inside `impl DashMarker`; grouped with its other `DashMarker` methods.
- `dash::registry` (6 functions)
  - `src/dash.rs:338-348` `dash_serving_pid_on` - name contains "pid" (instance registry); grouped under `dash::registry`.
  - `src/dash.rs:515-517` `displayable_pid` - name contains "pid" (instance registry); grouped under `dash::registry`.
  - `src/dash.rs:589-611` `pid_holding_port` - name contains "pid" (instance registry); grouped under `dash::registry`.
  - `src/dash.rs:749-751` `pid_if_port_matches` - name contains "pid" (instance registry); grouped under `dash::registry`.
  - `src/dash.rs:969-990` `instance_views` - name contains "instance" (instance registry); grouped under `dash::registry`.
  - `src/dash.rs:995-997` `instances_json` - name contains "instance" (instance registry); grouped under `dash::registry`.
- `dash::render` (18 functions)
  - `src/dash.rs:354-384` `header_line_value` - name contains "header" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:390-413` `head_has_header_line` - name contains "header" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:634-649` `format_held_port` - name contains "format" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:665-667` `describe_held_port` - name contains "describe" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:714-716` `describe_held_port_if_confirmed` - name contains "describe" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:1483-1515` `build_graph_view` - name contains "graph" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:1782-1796` `rationale_batch` - name contains "rationale" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:1812-1844` `graph_json` - name contains "graph" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:1882-1894` `call_node_view` - name contains "node" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:1911-1923` `ref_node_view` - name contains "node" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:2214-2295` `unit_node` - name contains "node" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:2299-2358` `role_stage` - name contains "role" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:2406-2417` `stage_of_role` - name contains "role" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:2425-2446` `role_and_agent` - name contains "role" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:2527-2535` `graph_seeds` - name contains "graph" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:2726-2732` `field_str` - name contains "field" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:2736-2747` `field_str_array` - name contains "field" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:2915-2928` `escape_for_script` - name contains "escape" (HTML rendering); grouped under `dash::render`.
- `dash::reproject` (5 functions)
  - `src/dash.rs:1551-1564` `reproject` - name contains "reproject" (graph reprojection); grouped under `dash::reproject`.
  - `src/dash.rs:1572-1591` `cap_clusters` - name contains "cluster" (graph reprojection); grouped under `dash::reproject`.
  - `src/dash.rs:1609-1620` `reprojection_lens_key` - name contains "reproject" (graph reprojection); grouped under `dash::reproject`.
  - `src/dash.rs:1626-1669` `reproject_derived` - name contains "reproject" (graph reprojection); grouped under `dash::reproject`.
  - `src/dash.rs:1690-1775` `reproject_files` - name contains "reproject" (graph reprojection); grouped under `dash::reproject`.
- `dash::response` (6 functions)
  - `src/dash.rs:2950-2956` `new` - method inside `impl Response`; grouped with its other `Response` methods.
  - `src/dash.rs:2959-2961` `rendered` - method inside `impl Response`; grouped with its other `Response` methods.
  - `src/dash.rs:2962-2968` `text` - method inside `impl Response`; grouped with its other `Response` methods.
  - `src/dash.rs:2974-2976` `binary` - method inside `impl Response`; grouped with its other `Response` methods.
  - `src/dash.rs:2978-2988` `reason` - method inside `impl Response`; grouped with its other `Response` methods.
  - `src/dash.rs:2998-3014` `write_to` - method inside `impl Response`; grouped with its other `Response` methods.
- `dash::server` (8 functions)
  - `src/dash.rs:443-455` `bind_singleton` - name contains "bind" (HTTP serving); grouped under `dash::server`.
  - `src/dash.rs:559-578` `tcp_listen_inode_for_port` - name contains "tcp" (HTTP serving); grouped under `dash::server`.
  - `src/dash.rs:619-621` `process_state` - name contains "process_" (HTTP serving); grouped under `dash::server`.
  - `src/dash.rs:2039-2076` `calls_route` - name contains "route" (HTTP serving); grouped under `dash::server`.
  - `src/dash.rs:3023-3204` `route` - name contains "route" (HTTP serving); grouped under `dash::server`.
  - `src/dash.rs:3280-3322` `serve_on` - name contains "serve" (HTTP serving); grouped under `dash::server`.
  - `src/dash.rs:3338-3517` `handle_conn` - name contains "handle" (HTTP serving); grouped under `dash::server`.
  - `src/dash.rs:3606-3731` `serve_console_stream` - name contains "serve" (HTTP serving); grouped under `dash::server`.
- `dash::tests` (104 functions)
  - `src/dash.rs:3766-3779` `get_static` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:3783-3789` `assert_console_page_carries` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:3791-3803` `seeded_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:3805-3814` `local_instance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:3822-3872` `instance_views_project_a_sorted_credential_free_landing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:3879-3887` `instance_view_age_floors_at_zero_for_a_future_heartbeat` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:3894-3926` `api_instances_route_renders_the_landing_list` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:3933-3938` `console_core_wasm_route_serves_the_embedded_artifact_as_application_wasm` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:3943-3955` `console_core_wasm_artifact_is_under_the_three_megabyte_budget` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:3967-3995` `embedded_artifact_matches_a_fresh_independent_nested_build` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4007-4057` `console_route_serves_the_shell_page_with_mock_regions_and_both_theme_token_blocks` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4063-4072` `console_route_never_references_an_external_url` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4079-4115` `console_fonts_route_serves_each_embedded_font_and_its_license_text` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4131-4143` `console_page_wires_the_theme_toggles_persistence_round_trip` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4146-4160` `root_serves_the_embedded_page_with_the_placeholder_resolved` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4171-4221` `gates_status_never_fabricates_passed_for_an_off_linear_unverdicted_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4228-4264` `free_port_from_returns_the_start_port_when_free_and_the_next_free_one_when_it_is_taken` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4271-4320` `bind_singleton_binds_the_exact_port_and_never_searches` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4331-4404` `bind_singleton_short_circuits_on_an_already_serving_rigger_dash` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4412-4422` `serve_reply` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4426-4444` `serve_dribble` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4449-4459` `timed_dash_probe` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4462-4464` `assert_pid_probe` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4471-4476` `dash_serving_on_is_false_for_a_non_dash_listener` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4487-4504` `dash_serving_on_is_bounded_against_a_byte_dribbling_holder` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4527-4549` `dash_serving_on_recognizes_the_header_fast_even_if_the_holder_never_finishes_the_block` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4640-4643` `dash_serving_pid_on_is_none_when_nothing_answers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4651-4680` `bind_singleton_cold_race_loser_resolves_across_the_accept_window` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4691-4726` `the_page_layout_cannot_scroll_the_body_horizontally` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4737-4774` `the_landing_view_lists_instances_and_threads_the_attach_selector` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4779-4788` `css_rule` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4799-4844` `the_dashboard_fits_one_screen_with_internal_scroll` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4858-4892` `cells_fit_or_wrap_and_wide_cells_scroll_in_their_own_container` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4907-4909` `the_decision_history_renders_each_decision_as_a_native_details_with_preview_and_full_body` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4912-4940` `state_endpoint_projects_the_seeded_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4947-4987` `console_event_filter_admits_only_the_named_run_lifecycle_types` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4993-5001` `console_event_wire_matches_console_cores_wire_event_shape` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5009-5015` `console_event_wire_carries_recorded_at` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5021-5028` `console_event_wire_with_a_malformed_body_degrades_to_null_data` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5034-5041` `console_progress_wire_with_a_malformed_body_degrades_to_empty_id_and_activity` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5044-5125` `console_snapshot_endpoint_filters_events_carries_progress_liveness_and_definitions` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5128-5189` `state_carries_the_live_agent_activity` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5192-5258` `state_counts_grep_fallbacks_and_carries_them_in_the_review_outcomes_data` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5267-5453` `run_tree_projects_the_spine_with_collapse_expand_and_driver_lines` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5273-5278` `done` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5458-5487` `review_verdicts_come_straight_from_the_metrics_classification` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5490-5500` `events_endpoint_is_since_exclusive` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5506-5535` `no_mutating_endpoint_exists` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5538-5567` `export_inlines_the_snapshot_as_a_static_page` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5575-5622` `export_neutralizes_a_script_breakout_in_the_inlined_state` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5625-5651` `decision_view_strikes_through_superseded_entries` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5660-5723` `cluster_key_folds_paths_by_directory_and_dev_loop_nodes_by_kind` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5735-5828` `clustered_overview_under_files_lens_admits_only_code_entities_keyed_by_their_own_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5847-6090` `cluster_detail_drills_a_cluster_to_its_members_and_caps_a_big_one_by_degree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:6099-6144` `cluster_detail_under_files_lens_is_unconditionally_empty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:6161-6358` `code_lens_buckets_code_entities_by_community_excludes_other_kinds_and_reports_underived_grain` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:6372-6611` `concepts_lens_buckets_members_by_concept_excludes_membershipless_nodes_and_reports_underived_grain` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:6617-6634` `tiered_chain_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:6637-6701` `the_graph_route_returns_a_tier_tagged_seeded_neighborhood_as_json` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:6704-6755` `the_graph_route_percent_decodes_the_seed_so_select_to_seed_reaches_ids_with_special_chars` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:6758-6789` `the_graph_route_degrades_gracefully_for_an_unknown_seed_and_an_empty_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:6792-6812` `the_graph_route_is_read_only_a_non_get_is_405` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:6818-6852` `dispatch_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:6861-6959` `the_graph_route_dispatches_cluster_overview_and_seed_by_parameter` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:6973-7027` `the_overview_route_degrades_gracefully_on_an_empty_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7030-7084` `neighborhood_bounds_by_depth_follows_both_directions_and_skips_invalidated_edges` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7087-7120` `neighborhood_flags_god_nodes_by_degree_within_the_returned_neighborhood` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7123-7195` `path_is_the_shortest_route_between_two_selected_nodes_over_currently_valid_edges` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7198-7265` `the_graph_route_flags_god_nodes_and_returns_the_query_path_between_two_selected_nodes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7272-7309` `provenance_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7312-7359` `explain_returns_a_nodes_incident_edges_as_source_and_tier_tagged_provenance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7362-7427` `the_graph_route_carries_the_seed_nodes_explain_provenance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7430-7461` `graph_seeds_enumerate_decisions_findings_and_their_files_never_units` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7464-7508` `a_units_seed_lands_on_the_neighborhood_of_its_decisions_and_files` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7511-7617` `the_run_tree_click_to_seed_route_lands_a_unit_on_a_real_neighborhood` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7620-7673` `unit_seeds_scope_content_to_the_owning_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7676-7712` `repoint_seed_passes_a_known_node_and_re_points_a_unit_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7715-7734` `build_state_on_an_empty_run_is_empty_not_a_panic` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7745-7834` `release_ready_is_surfaced_on_the_dash_only_for_a_done_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7852-7907` `release_ready_pr_command_newline_renders_as_a_real_line_break_not_a_collapsed_run_on` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7910-7917` `request_line_parsing_extracts_method_and_target` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7920-7927` `query_param_reads_since` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7935-8013` `endpoints_serve_over_a_real_socket_against_a_seeded_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8019-8070` `a_post_over_a_real_socket_is_refused_without_touching_the_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8081-8193` `the_graph_provider_is_consulted_only_on_graph_requests_not_the_state_poll` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8198-8208` `dash_marker_round_trips_through_its_on_disk_record` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8211-8230` `dash_marker_parse_rejects_a_malformed_record` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8233-8251` `dash_marker_reads_none_for_an_absent_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8256-8268` `format_held_port_always_names_the_address_even_with_no_holder` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8271-8288` `format_held_port_names_the_pid_and_state_for_a_running_holder` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8291-8299` `format_held_port_names_the_pid_alone_when_its_state_is_not_discoverable` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8302-8317` `format_held_port_gives_the_stopped_listener_diagnosis_naming_resume_or_kill` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8320-8332` `pid_holding_port_finds_the_pid_of_a_listener_bound_in_this_process` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8335-8351` `pid_holding_port_is_none_for_a_port_nothing_is_listening_on` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8359-8372` `assert_names_this_process_as_holder` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8401-8417` `describe_held_port_if_confirmed_is_none_when_nothing_holds_the_port` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8420-8437` `dash_start_needed_is_true_when_none_serving_and_false_when_one_serves` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8440-8488` `dash_status_trusts_a_url_with_no_marker_and_catches_a_marker_that_lies` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8500-8516` `dash_status_never_names_the_unattributed_pid_sentinel_as_a_dead_process` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8519-8540` `url_port_parses_the_recorded_shape_and_rejects_anything_else` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8556-8596` `dash_status_probes_the_urls_own_port_when_the_marker_names_a_different_dash` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8599-8637` `should_reap_singleton_reaps_only_when_no_registered_instance_is_live` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8640-8673` `should_reap_singleton_never_reaps_while_a_fresh_agent_liveness_signal_is_present` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8685-8756` `the_page_carries_the_directed_call_layered_render` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
- `dash::tests::calls_route_c4` (10 functions)
  - `src/dash.rs:8774-8784` `cnode` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:8786-8788` `layer_of` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:8789-8791` `ids` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:8797-8852` `calls_view_down_signs_callees_positive_and_carries_frontier_and_back` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:8857-8896` `calls_view_up_negates_callers_and_carries_the_referenced_sidecar` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:8902-8950` `calls_view_both_centers_the_seed_with_callees_right_and_callers_left` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:8956-8993` `a_plain_neighborhood_omits_every_additive_call_field` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9000-9054` `calls_route_runs_the_traversal_for_view_calls_and_declines_otherwise` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9060-9088` `calls_route_clamps_depth_and_defaults_the_tier_floor` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9093-9111` `calls_route_walks_both_directions_for_dir_both` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
- `dash::tests::metadata_card_c2` (12 functions)
  - `src/dash.rs:9705-9755` `card_graph` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9758-9801` `card_of_a_code_entity_carries_file_line_degree_community_concepts_and_memory_counts` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9807-9815` `card_of_a_membership_less_entity_has_no_community_and_no_line` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9823-9850` `card_of_a_proven_code_entity_carries_proven_by_and_proof_evidence` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9858-9863` `card_of_an_unproven_code_entity_has_proven_by_zero_and_no_evidence` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9867-9876` `assert_card_reports_no_proof` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9903-9940` `card_of_a_file_lists_its_contained_entities_as_top_entities` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9945-9965` `card_of_a_concept_lists_its_realizing_members_as_top_evidence` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9974-9999` `card_of_a_concept_carries_each_top_evidence_members_own_kind` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10006-10020` `card_of_a_file_carries_each_top_entity_members_own_kind` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10025-10028` `card_of_an_unknown_id_is_none` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10034-10076` `the_card_route_serves_a_known_subjects_card_and_null_for_an_unknown_one` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
- `dash::tests::rationale_overlay_c3` (12 functions)
  - `src/dash.rs:9133-9143` `finding_node` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9159-9192` `rationale_graph` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9195-9197` `leaf_fields` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9203-9217` `node_rationale_returns_attached_leaves_ordered_by_kind_then_id` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9223-9246` `a_finding_leaf_carries_content_only_never_the_by_or_unit_machinery` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9252-9265` `a_handbook_rule_and_a_supersedes_edge_are_not_rationale` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9270-9278` `an_invalidated_edge_is_not_live_rationale` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9283-9293` `a_node_without_rationale_returns_none` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9299-9324` `the_batch_covers_the_visible_set_and_keeps_only_nodes_with_rationale` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9329-9346` `the_batch_is_deterministic_across_request_order_and_dedups` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9352-9391` `the_explain_route_returns_the_batch_in_one_request` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9397-9421` `an_absent_explain_leaves_the_graph_route_unchanged` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
- `dash::tests::subject_view_c5` (5 functions)
  - `src/dash.rs:9442-9485` `memory_rail_lists_decisions_findings_and_concepts_excluding_lessons` - defined inside `subject_view_c5`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9490-9497` `memory_rail_is_empty_for_a_node_with_no_governing_memory` - defined inside `subject_view_c5`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9518-9595` `memory_rail_concepts_are_live_from_node_realizes_edges_to_a_concept_target_deduped_by_id` - defined inside `subject_view_c5`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9603-9653` `the_seeded_route_carries_memory_without_adding_a_single_node_to_the_neighborhood` - defined inside `subject_view_c5`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9659-9679` `a_cluster_drill_carries_no_memory_field` - defined inside `subject_view_c5`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
- `main::commands` (41 functions)
  - `src/main.rs:279-282` `cmd_version` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:2219-2227` `cmd_run` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:2331-2413` `cmd_resume_unit` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:2415-2978` `cmd_step` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:3206-3238` `cmd_reported` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:3256-3272` `cmd_prompt` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:3296-3327` `cmd_scratch` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:3982-3989` `cmd_serve` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:4034-4103` `cmd_workflow` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:4172-4224` `cmd_graph` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:4347-4367` `cmd_graph_show` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:4671-4754` `cmd_graph_build` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:4775-4799` `cmd_graph_communities` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:4873-4897` `cmd_graph_concepts` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:4916-4954` `cmd_stats` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:5272-5282` `cmd_stats_canary` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:5650-5773` `cmd_canary` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:5783-5819` `cmd_playbooks` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:5852-6004` `cmd_replay` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:6814-7124` `cmd_dash` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:7572-7608` `cmd_ground` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:7624-7645` `cmd_reindex` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:7662-7690` `cmd_symbols_index` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:7698-7774` `cmd_emit` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:7785-7819` `cmd_progress` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:7869-8101` `cmd_status` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:8188-8219` `cmd_watch` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:8475-8556` `cmd_reset` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:9357-9397` `cmd_peers` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:9604-9700` `cmd_result` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:9836-10036` `cmd_validate` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:12126-12145` `cmd_init` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:12791-12963` `cmd_setup` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:13339-13344` `cmd_docs` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:13499-13503` `cmd_instructions` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:13511-13549` `cmd_prime` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:13666-13714` `cmd_mcp` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:13969-14006` `cmd_grep_guard` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:14271-14303` `cmd_guard_write` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:14308-14320` `cmd_hook` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:14334-14401` `cmd_hook_stop_failure` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
- `main::dash_glue` (25 functions)
  - `src/main.rs:120-124` `record_dash_attempt` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6346-6348` `dash_marker_serving` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6406-6426` `spawn_dash_child_process` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6451-6472` `wait_for_dash_bind` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6499-6532` `wait_for_dash_bind_or_diagnose` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6685-6687` `dash_ensure_suppressed` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6694-6696` `dash_ensure_port` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6702-6705` `dash_ensure_port_from` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6766-6770` `recorded_dash_url` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6780-6794` `dash_status_line` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6803-6812` `dash_status_json` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7131-7138` `dash_reap_poll` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7147-7155` `dash_reap_idle_window` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7270-7287` `dash_read_run` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7293-7305` `dash_read_graph` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7316-7324` `dash_read_whole_graph` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7333-7350` `dash_read_calls` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7356-7368` `dash_attach_calls` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7373-7390` `dash_read_progress` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7396-7412` `dash_read_liveness` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7435-7451` `dash_resolve_attach` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7476-7486` `dash_read_sqlite_stream_readonly` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7488-7524` `dash_attach_run` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7531-7543` `dash_attach_inputs` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7549-7555` `dash_attach_graph` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
- `main::docs_overlay` (1 function)
  - `src/main.rs:12279-12286` `apply` - method inside `impl DocsOverlay`; grouped with its other `DocsOverlay` methods.
- `main::live_writer_facts` (2 functions)
  - `src/main.rs:9068-9075` `reasons` - method inside `impl LiveWriterFacts`; grouped with its other `LiveWriterFacts` methods.
  - `src/main.rs:9080-9084` `driver_dead` - method inside `impl LiveWriterFacts`; grouped with its other `LiveWriterFacts` methods.
- `main::liveness` (10 functions)
  - `src/main.rs:3032-3042` `live_branches_for_sweep` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:3073-3089` `terminal_and_no_live_worker` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:7835-7849` `liveness_ages_for_wave` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:8955-8993` `live_writer_reasons` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:8999-9011` `live_writer_refusal` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:9089-9144` `live_writer_facts` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:9273-9283` `superseded_edge_boundary` - name contains "superseded" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:9305-9335` `superseded_graph_nodes` - name contains "superseded" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:10827-10834` `live_slugs` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:11032-11050` `worktree_belongs_to_live` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
- `main::provenance` (22 functions)
  - `src/main.rs:209-211` `workflow_path` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:3570-3612` `refuse_when_base_lacks_spec_paths` - name contains "spec_" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:3836-3980` `run_workflow` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:3995-4026` `parse_workflow_args` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:9829-9834` `spec_lint_warning_lines` - name contains "spec_" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:10285-10290` `installed_workflow_drifted` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:10297-10299` `workflow_provenance_path` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:10304-10312` `installed_workflow_provenance` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:10331-10342` `git_is_ancestor` - name contains "git_" (git/version provenance); grouped under `main::provenance`.
  - `src/main.rs:10350-10360` `git_commit_distance` - name contains "git_" (git/version provenance); grouped under `main::provenance`.
  - `src/main.rs:10487-10521` `workflow_drift_advisory` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:12214-12224` `install_workflow` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:12294-12303` `read_docs_overlay` - name contains "docs_" (docs overlay); grouped under `main::provenance`.
  - `src/main.rs:12628-12647` `git_hooks_dir` - name contains "git_" (git/version provenance); grouped under `main::provenance`.
  - `src/main.rs:13259-13303` `docs_context` - name contains "docs_" (docs overlay); grouped under `main::provenance`.
  - `src/main.rs:13359-13378` `docs_drift` - name contains "docs_" (docs overlay); grouped under `main::provenance`.
  - `src/main.rs:13387-13403` `docs_drift_failure` - name contains "docs_" (docs overlay); grouped under `main::provenance`.
  - `src/main.rs:13425-13431` `spec_lint_next_step` - name contains "spec_" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:13452-13456` `spec_lint_reminder_suppressed` - name contains "spec_" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:13464-13476` `spec_lint_reminder_should_print` - name contains "spec_" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:13590-13592` `git_repo` - name contains "git_" (git/version provenance); grouped under `main::provenance`.
  - `src/main.rs:13600-13608` `git_repo_at` - name contains "git_" (git/version provenance); grouped under `main::provenance`.
- `main::render` (10 functions)
  - `src/main.rs:4256-4323` `print_around_subgraph` - name contains "print_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:4377-4415` `print_entity_site` - name contains "print_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:4424-4436` `print_site_header` - name contains "print_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:5045-5087` `format_stats` - name contains "format_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:5133-5149` `format_progress_line` - name contains "format_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:5306-5402` `format_canary_stats` - name contains "format_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:6196-6206` `format_stats_diff` - name contains "format_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:11265-11295` `format_residue` - name contains "format_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:11965-11977` `print_orientation` - name contains "print_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:14403-14422` `print_run_state` - name contains "print_" (human-readable output); grouped under `main::render`.
- `main::replay_runner` (1 function)
  - `src/main.rs:6217-6237` `run` - method inside `impl Runner for ReplayRunner`; grouped with its other `ReplayRunner` methods.
- `main::residue_report` (1 function)
  - `src/main.rs:10598-10603` `is_empty` - method inside `impl ResidueReport`; grouped with its other `ResidueReport` methods.
- `main::run_registration` (2 functions)
  - `src/main.rs:774-779` `inert` - method inside `impl RunRegistration`; grouped with its other `RunRegistration` methods.
  - `src/main.rs:783-790` `drop` - method inside `impl Drop for RunRegistration`; grouped with its other `RunRegistration` methods.
- `main::scaffold_report` (1 function)
  - `src/main.rs:11805-11812` `changed` - method inside `impl ScaffoldReport`; grouped with its other `ScaffoldReport` methods.
- `main::setup` (22 functions)
  - `src/main.rs:2021-2024` `scratch_defaults` - name contains "scratch" (project setup); grouped under `main::setup`.
  - `src/main.rs:4118-4136` `locate_shim` - name contains "shim" (project setup); grouped under `main::setup`.
  - `src/main.rs:7174-7178` `foreign_instance_scratch_root` - name contains "scratch" (project setup); grouped under `main::setup`.
  - `src/main.rs:11366-11386` `scratch_totals` - name contains "scratch" (project setup); grouped under `main::setup`.
  - `src/main.rs:11548-11583` `classify_agent_scratch` - name contains "scratch" (project setup); grouped under `main::setup`.
  - `src/main.rs:11949-11956` `print_scaffold_pointer` - name contains "scaffold" (project setup); grouped under `main::setup`.
  - `src/main.rs:12090-12124` `scaffold_summary_lines` - name contains "scaffold" (project setup); grouped under `main::setup`.
  - `src/main.rs:12183-12200` `install_file_if_changed` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/main.rs:12232-12234` `skill_source_rel` - name contains "skill" (project setup); grouped under `main::setup`.
  - `src/main.rs:12243-12248` `skill_install_path` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/main.rs:12316-12329` `install_skills` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/main.rs:12659-12687` `install_precommit_hook` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/main.rs:12704-12712` `provision_shim` - name contains "shim" (project setup); grouped under `main::setup`.
  - `src/main.rs:12726-12735` `shim_is_current` - name contains "shim" (project setup); grouped under `main::setup`.
  - `src/main.rs:12740-12747` `write_shim_files` - name contains "shim" (project setup); grouped under `main::setup`.
  - `src/main.rs:12754-12782` `run_npm_install` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/main.rs:12988-12992` `install_lookup_hook` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/main.rs:12996-13003` `install_into_claude_settings` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/main.rs:13008-13024` `install_into` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/main.rs:13042-13046` `install_status_line` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/main.rs:13055-13059` `install_operator_mcp` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/main.rs:13071-13087` `parse_setup_args` - name contains "setup" (project setup); grouped under `main::setup`.
- `main::store` (33 functions)
  - `src/main.rs:482-500` `store_conn_file` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:562-575` `store_backend_kind` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:625-691` `store_selection_at` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:699-705` `store_selection` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:733-746` `registry_store_identity` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:1255-1326` `migrate_project_identity` - name contains "migrate" (store hygiene); grouped under `main::store`.
  - `src/main.rs:1335-1340` `migrate_local_identity` - name contains "migrate" (store hygiene); grouped under `main::store`.
  - `src/main.rs:1352-1378` `migrate_identity_at` - name contains "migrate" (store hygiene); grouped under `main::store`.
  - `src/main.rs:1790-1792` `find_store_dir_from` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:2035-2040` `server_store_location` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:2071-2136` `require_store_dir` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:2140-2142` `store_file` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:3135-3141` `reclaim_run_scratch` - name contains "reclaim_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:8575-8601` `reset_menu` - name contains "reset_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:8670-8710` `reset_modes` - name contains "reset_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:8737-8756` `reset_scratch_orphans` - name contains "reset_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:8758-8777` `reset_build_cache` - name contains "reset_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:8782-8795` `build_cache_reclaim_report` - name contains "reclaim_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:8823-8831` `reset_derived` - name contains "reset_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:9042-9055` `refuse_derived_reset_if_live` - name contains "reset_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:9171-9204` `reset_runs` - name contains "reset_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:9733-9748` `reclaim_spawn_scratch` - name contains "reclaim_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:9775-9792` `reclaim_spawn_registered_scratch` - name contains "reclaim_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:10208-10218` `bloat_advisory` - name contains "bloat" (store hygiene); grouped under `main::store`.
  - `src/main.rs:10231-10242` `bloat_advisory_for` - name contains "bloat" (store hygiene); grouped under `main::store`.
  - `src/main.rs:10862-10951` `reclaim_orphan_scratch` - name contains "reclaim_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:11203-11244` `reclaim_shared_build_cache` - name contains "reclaim_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:11403-11446` `scratch_footprint` - name contains "footprint" (store hygiene); grouped under `main::store`.
  - `src/main.rs:11456-11479` `store_and_backup_bytes` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:11600-11677` `footprint_report` - name contains "footprint" (store hygiene); grouped under `main::store`.
  - `src/main.rs:11683-11688` `footprint_report_lines` - name contains "footprint" (store hygiene); grouped under `main::store`.
  - `src/main.rs:11697-11717` `footprint_advisories` - name contains "footprint" (store hygiene); grouped under `main::store`.
  - `src/main.rs:11748-11775` `footprint_report_for` - name contains "footprint" (store hygiene); grouped under `main::store`.
- `main::store_location` (3 functions)
  - `src/main.rs:1964-1966` `file` - method inside `impl StoreLocation`; grouped with its other `StoreLocation` methods.
  - `src/main.rs:1972-1979` `identity` - method inside `impl StoreLocation`; grouped with its other `StoreLocation` methods.
  - `src/main.rs:1992-1998` `repo_root` - method inside `impl StoreLocation`; grouped with its other `StoreLocation` methods.
- `main::store_selection` (1 function)
  - `src/main.rs:443-445` `is_sqlite` - method inside `impl StoreSelection`; grouped with its other `StoreSelection` methods.
- `main::support` (37 functions)
  - `src/main.rs:330-406` `parse_run_args` - name contains "parse_" (argument/input parsing); grouped under `main::support`.
  - `src/main.rs:417-425` `resolve_run_base` - name contains "resolve" (path/id resolution helper); grouped under `main::support`.
  - `src/main.rs:507-509` `conn_file_is_group_or_other_readable` - name contains "is_" (predicate helper); grouped under `main::support`.
  - `src/main.rs:517-539` `warn_if_conn_file_is_exposed` - name contains "is_" (predicate helper); grouped under `main::support`.
  - `src/main.rs:582-605` `resolve_conn` - name contains "resolve" (path/id resolution helper); grouped under `main::support`.
  - `src/main.rs:713-723` `resolve_store` - name contains "resolve" (path/id resolution helper); grouped under `main::support`.
  - `src/main.rs:992-1001` `read_project_id` - name contains "read_" (generic read helper); grouped under `main::support`.
  - `src/main.rs:1836-1859` `resolve_main_worktree_or_refuse` - name contains "resolve" (path/id resolution helper); grouped under `main::support`.
  - `src/main.rs:3360-3372` `read_project_stream` - name contains "read_" (generic read helper); grouped under `main::support`.
  - `src/main.rs:3402-3445` `parse_step_args` - name contains "parse_" (argument/input parsing); grouped under `main::support`.
  - `src/main.rs:5412-5420` `read_model_drift` - name contains "read_" (generic read helper); grouped under `main::support`.
  - `src/main.rs:5429-5438` `read_graph_index_lag` - name contains "read_" (generic read helper); grouped under `main::support`.
  - `src/main.rs:5524-5536` `read_order_signatures` - name contains "read_" (generic read helper); grouped under `main::support`.
  - `src/main.rs:5560-5632` `parse_canary_args` - name contains "parse_" (argument/input parsing); grouped under `main::support`.
  - `src/main.rs:6081-6113` `parse_replay_args` - name contains "parse_" (argument/input parsing); grouped under `main::support`.
  - `src/main.rs:6361-6387` `ensure_run_dashboard_at` - name contains "ensure_" (invariant helper); grouped under `main::support`.
  - `src/main.rs:6725-6760` `ensure_run_dashboard` - name contains "ensure_" (invariant helper); grouped under `main::support`.
  - `src/main.rs:8131-8161` `parse_watch_args` - name contains "parse_" (argument/input parsing); grouped under `main::support`.
  - `src/main.rs:9468-9526` `parse_result_args` - name contains "parse_" (argument/input parsing); grouped under `main::support`.
  - `src/main.rs:9566-9580` `read_outcome_from_stdin` - name contains "read_" (generic read helper); grouped under `main::support`.
  - `src/main.rs:10703-10731` `read_run_units` - name contains "read_" (generic read helper); grouped under `main::support`.
  - `src/main.rs:11054-11056` `is_uuid8` - name contains "is_" (predicate helper); grouped under `main::support`.
  - `src/main.rs:11063-11092` `find_shadow_stores` - name contains "find_" (lookup helper); grouped under `main::support`.
  - `src/main.rs:11155-11160` `is_build_cache_tombstone` - name contains "is_" (predicate helper); grouped under `main::support`.
  - `src/main.rs:11996-12045` `write_gitignore_entries` - name contains "write_" (generic write helper); grouped under `main::support`.
  - `src/main.rs:12530-12535` `find_bytes` - name contains "find_" (lookup helper); grouped under `main::support`.
  - `src/main.rs:13313-13331` `write_docs` - name contains "write_" (generic write helper); grouped under `main::support`.
  - `src/main.rs:13643-13657` `parse_mcp_spawn_flag` - name contains "parse_" (argument/input parsing); grouped under `main::support`.
  - `src/main.rs:14013-14040` `parse_guard_write_roots` - name contains "parse_" (argument/input parsing); grouped under `main::support`.
  - `src/main.rs:14075-14085` `guard_write_target` - name contains "write_" (generic write helper); grouped under `main::support`.
  - `src/main.rs:14101-14120` `guard_write_read_target` - name contains "read_" (generic read helper); grouped under `main::support`.
  - `src/main.rs:14128-14160` `resolve_lexical_realpath` - name contains "resolve" (path/id resolution helper); grouped under `main::support`.
  - `src/main.rs:14177-14186` `resolve_write_target` - name contains "resolve" (path/id resolution helper); grouped under `main::support`.
  - `src/main.rs:14192-14196` `write_target_under_root` - name contains "write_" (generic write helper); grouped under `main::support`.
  - `src/main.rs:14203-14208` `guard_write_deny_outside_first_root` - name contains "write_" (generic write helper); grouped under `main::support`.
  - `src/main.rs:14219-14234` `guard_write_decision` - name contains "write_" (generic write helper); grouped under `main::support`.
  - `src/main.rs:14432-14439` `write_if_absent` - name contains "write_" (generic write helper); grouped under `main::support`.
- `main::tests` (374 functions)
  - `src/main.rs:12617-12622` `compose_precommit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14688-14702` `test_request` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14711-14718` `spec_lint_next_step_names_rigger_validate_and_the_given_spec_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14726-14728` `spec_lint_reminder_suppressed_when_env_names_the_real_direct_parent_pid` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14734-14755` `spec_lint_reminder_prints_on_absent_foreign_stale_or_malformed_env` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14772-14794` `spec_lint_warning_lines_formats_every_advisory_with_the_spec_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14799-14802` `spec_lint_warning_lines_is_empty_on_a_clean_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14812-14865` `ensure_run_dashboard_at_starts_once_then_short_circuits_on_a_live_marker` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14874-14880` `dash_marker_serving_reports_false_when_nothing_answers_the_markers_port` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14885-14917` `ensure_run_dashboard_at_restarts_when_the_recorded_dash_is_gone` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14925-14944` `ensure_run_dashboard_at_reports_failed_when_the_start_errors` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14956-14995` `wait_for_dash_bind_confirms_promptly_once_the_port_answers_as_a_dash` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15002-15029` `wait_for_dash_bind_fails_fast_when_the_child_exits_before_confirming` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15043-15085` `wait_for_dash_bind_times_out_against_a_real_held_port` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15108-15152` `wait_for_dash_bind_or_diagnose_never_self_attributes_its_own_still_starting_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15169-15197` `ensure_run_dashboard_at_writes_no_marker_when_the_real_spawn_never_confirms_a_bind` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15212-15257` `ensure_run_dashboard_at_names_the_held_ports_holder_when_the_real_spawn_fails` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15277-15301` `ensure_run_dashboard_at_never_claims_a_phantom_holder_for_an_unheld_port` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15313-15344` `ensure_run_dashboard_at_self_heals_a_marker_naming_a_dead_pid` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15354-15384` `ensure_run_dashboard_at_self_heals_a_marker_naming_a_live_pid_whose_port_is_unserved` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15397-15435` `ensure_run_dashboard_at_never_revisits_a_still_serving_unattributed_pid_marker` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15442-15463` `dash_ensure_is_suppressed_by_either_opt_out_and_proceeds_when_neither_is_set` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15473-15504` `dash_status_line_renders_each_outcome_to_its_exact_text` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15512-15540` `dash_status_json_renders_each_outcome_to_its_exact_shape` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15549-15576` `dash_ensure_port_defaults_to_the_fixed_address_and_only_a_valid_override_relocates_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15586-15619` `compose_precommit_fresh_install_carries_the_managed_block` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15629-15656` `precommit_block_refuses_on_drift_instead_of_staging` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15670-15723` `precommit_block_finds_the_relocated_unit_target_with_the_binary_s_own_path_encoding` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15726-15758` `shipped_workflow_driver_tells_a_worker_its_units_build_location` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15761-15862` `precommit_block_resolves_a_tree_built_binary_before_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15869-15881` `compose_precommit_is_idempotent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15892-15921` `compose_precommit_chains_without_clobbering_an_existing_hook` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15931-15945` `compose_precommit_prepends_before_a_terminal_exit_existing_hook` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15954-16005` `install_precommit_hook_preserves_a_non_utf8_existing_hook` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16012-16035` `compose_precommit_refreshes_a_stale_block_in_place` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16043-16062` `cmd_peers_prints_live_or_historical_per_decision_provenance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16080-16120` `superseded_graph_nodes_drops_dead_runs_and_preboundary_keeping_lessons_active_and_reused_ids` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16082-16087` `run_started` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16129-16159` `superseded_edge_boundary_is_the_active_runs_start_or_none_without_a_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16131-16134` `event_at` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16187-16441` `the_denoise_leaves_metrics_run_pruning_and_blast_radius_unaffected` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16451-16469` `version_line_carries_the_derived_version_and_a_non_empty_build_provenance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16476-16520` `docs_context_reads_every_fact_from_code` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16527-16568` `docs_render_surfaces_known_code_facts_verbatim` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16574-16593` `commands_registry_is_well_formed_and_covers_dispatch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16601-16631` `write_docs_writes_every_registry_skill_plus_the_handbook` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16641-16678` `install_and_docs_each_cover_exactly_the_registry_no_more_no_less` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16682-16705` `assert_skills_reference_only_real_subcommands` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16755-16822` `docs_drift_flags_a_changed_file_and_skips_absent_or_in_sync_ones` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16833-16859` `committed_registry_docs_are_in_sync_with_a_fresh_render` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16870-16899` `planning_field_guide_page_renders_and_is_linked_from_authoring_loops` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16910-16979` `status_and_dashboard_render_the_same_current_blocker_lines` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16992-17047` `release_ready_lines_surface_only_on_a_done_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17050-17116` `status_and_dash_read_the_runs_persisted_base_not_a_re_resolution` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17121-17142` `dirty_tracked_paths_keeps_tracked_modifications_and_drops_untracked_and_ignored` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17145-17147` `dirty_tracked_paths_on_a_clean_tree_is_empty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17150-17175` `installed_workflow_drifted_is_false_when_absent_or_identical_and_true_on_drift` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17180-17227` `drift_side_names_the_binary_stale_only_when_the_installed_workflow_is_provably_newer` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17230-17278` `workflow_drift_advisory_names_which_side_is_stale_and_never_says_they_differ` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17282-17309` `linear_history` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17312-17333` `git_is_ancestor_decides_commit_order_in_a_real_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17336-17357` `git_commit_distance_counts_commits_ahead_in_a_real_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17360-17374` `missing_gitsemver_binary_advisory_fires_only_on_the_unversioned_marker` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17378-17385` `assert_behind_the_tree_silent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17429-17436` `behind_the_tree_message_names_both_versions_and_the_commit_distance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17439-17477` `behind_the_tree_advisory_names_the_real_derived_version_ahead_of_the_installed_commit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17480-17502` `behind_the_tree_advisory_is_silent_when_the_checkout_has_not_moved` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17505-17518` `install_workflow_records_the_build_provenance_beside_the_workflow` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17521-17536` `validate_advisories_warns_on_workflow_drift_naming_the_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17542-17544` `slugs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17549-17561` `init_committed_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17579-17595` `resolve_main_worktree_or_refuse_returns_exactly_git_rev_parse_show_toplevel` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17598-17622` `refuse_when_base_lacks_spec_paths_refuses_on_total_absence_and_names_a_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17626-17644` `assert_base_path_check_proceeds` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17684-17757` `refuse_when_base_unreachable_fails_loudly_only_when_no_reachable_base` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17760-17767` `human_size_formats_bytes_through_gib` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17770-17776` `is_uuid8_accepts_exactly_eight_hex_digits` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17779-17824` `worktree_belongs_to_live_matches_both_naming_shapes_without_prefix_false_match` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17827-17874` `current_run_units_scopes_to_the_current_run_and_splits_live_from_dead` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17877-17914` `current_run_units_spares_a_terminal_units_branch_whose_latest_spawn_is_still_in_flight` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17917-17943` `current_run_units_still_retires_a_terminal_unit_once_its_latest_spawn_answers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17946-17993` `current_run_units_splits_live_spawns_from_answered_ones_scoped_to_the_current_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18005-18031` `live_branches_for_sweep_fails_closed_on_an_unreadable_stream_but_folds_a_readable_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18034-18074` `find_shadow_stores_finds_nested_events_db_and_prunes_build_caches` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18077-18088` `dir_size_bytes_sums_files_recursively` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18093-18109` `reclaim_shared_build_cache_deletes_a_populated_cache_and_reports_its_bytes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18112-18130` `reclaim_shared_build_cache_is_idempotent_zero_report_when_absent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18133-18161` `reclaim_shared_build_cache_refuses_rather_than_waits_when_a_build_holds_the_guard` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18164-18187` `reclaim_shared_build_cache_releases_the_guard_promptly_after_a_successful_reclaim` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18190-18263` `scan_residue_reports_dead_worktrees_caches_shadows_and_branches` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18266-18285` `scan_residue_is_empty_when_everything_is_live_and_no_shadow_stores` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18288-18305` `format_residue_renders_a_sized_warning_block` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18310-18329` `store_and_backup_bytes_splits_live_store_files_from_dot_bak_backups` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18332-18336` `store_and_backup_bytes_is_zero_on_a_missing_directory` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18339-18381` `scratch_footprint_totals_every_entry_and_dead_only_the_non_live_share` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18384-18449` `footprint_report_measures_every_category_on_a_seeded_fixture_tree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18452-18469` `footprint_report_folds_a_none_mutation_root_to_a_zero_contribution` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18472-18554` `footprint_report_flags_registered_scratch_roots_dead_share_and_spares_a_live_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18567-18621` `footprint_report_keys_agent_scratch_liveness_by_run_id_and_leaf_not_leaf_name_alone` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18624-18629` `looks_like_run_container_true_when_every_direct_child_is_a_directory` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18632-18640` `looks_like_run_container_false_when_a_direct_child_is_a_bare_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18643-18651` `looks_like_run_container_is_true_on_an_empty_or_missing_dir` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18654-18667` `classify_agent_scratch_separates_a_bare_top_level_file_from_a_well_formed_container` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18670-18749` `footprint_report_reports_a_top_level_adhoc_agent_scratch_dir_as_its_own_category_never_folded_into_the_dead_run_bucket` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18752-18776` `footprint_report_lines_reports_every_categorys_total_unconditionally` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18779-18790` `footprint_advisories_flags_a_category_whose_dead_share_reaches_the_threshold` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18794-18802` `assert_hinted_category_is_silent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18810-18827` `footprint_advisories_flags_a_category_exactly_at_the_threshold_boundary` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18830-18840` `footprint_advisories_never_flags_a_category_with_no_reclaim_command` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18847-18869` `owning_repo_root_prefers_the_stores_own_root_over_the_git_toplevel` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18874-18997` `reclaim_orphan_scratch_removes_non_live_owned_scratch_and_spares_live_and_shared_areas` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19000-19055` `reclaim_orphan_scratch_spares_a_non_live_worktree_that_is_still_dirty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19058-19097` `reclaim_orphan_scratch_spares_only_a_declared_dirty_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19100-19130` `reclaim_orphan_scratch_reaps_a_stray_build_cache_tombstone_unconditionally` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19135-19174` `leaked_process_advisories_name_a_process_rooted_under_the_scratch_root` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19177-19187` `leaked_process_advisories_is_empty_when_no_process_is_rooted_under_the_scratch_root` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19190-19197` `leaked_process_advisories_is_a_graceful_no_op_when_the_scratch_root_is_absent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19202-19208` `parse_result_takes_an_id_and_an_optional_output_arg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19211-19216` `parse_result_with_no_output_defers_to_stdin` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19221-19226` `find_store_dir_from_returns_the_dir_that_holds_the_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19229-19240` `find_store_dir_from_walks_up_from_a_subdirectory` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19243-19246` `plant_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19249-19261` `add_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19264-19299` `find_store_dir_from_never_escapes_the_repo_into_a_parent_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19302-19330` `reap_then_remove_dir_reaps_processes_rooted_inside_then_removes_the_dir` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19333-19382` `reclaim_run_scratch_removes_the_run_level_areas_and_spares_per_unit_scratch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19385-19418` `reclaim_run_scratch_spares_the_shared_build_cache_while_a_build_holds_its_guard` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19421-19432` `find_store_dir_from_refuses_the_worktree_shape_with_no_events_db` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19435-19463` `find_store_dir_from_walks_past_a_storeless_rigger_to_the_real_store_above` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19466-19495` `find_store_dir_from_resolves_the_owning_repo_even_when_the_worktree_lives_outside_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19498-19527` `find_store_dir_from_never_climbs_a_relocated_worktrees_own_unrelated_ancestors_into_a_foreign_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19530-19555` `walk_stores_from_prefers_the_outermost_store_over_a_nearer_shadow` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19558-19575` `walk_stores_from_reports_no_shadow_for_a_single_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19584-19653` `require_store_dir_pins_to_the_fence_env_and_never_reaches_the_live_store_above_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19587-19590` `drop` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19657-19672` `require_store_dir_fence_is_off_by_default` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19677-19683` `result_advisories_flags_an_orphan_id_with_no_spawn_request` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19686-19711` `result_advisories_orphan_wording_is_plain_on_record_and_conditional_under_if_absent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19714-19724` `result_advisories_is_silent_for_a_parked_unanswered_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19727-19738` `result_advisories_flags_a_supersede_with_the_prior_result_position` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19741-19754` `result_advisories_suppresses_the_supersede_note_when_not_superseding` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19757-19769` `result_advisories_flags_both_orphan_and_supersede` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19772-19792` `parse_result_error_flag_is_order_independent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19795-19822` `parse_result_if_absent_is_off_by_default_and_a_bare_order_independent_flag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19825-19860` `parse_result_meta_must_be_a_json_object` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19863-19877` `parse_result_rejects_missing_id_extra_args_and_unknown_flags` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19880-19894` `build_result_shapes_success_and_failure` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19897-19902` `build_result_rejects_a_blank_error_message` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19905-19914` `build_result_attaches_meta` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19918-19936` `replay_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19939-19960` `a_recorded_result_lets_the_replay_driver_advance_past_the_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19963-19979` `a_recorded_error_result_replays_as_a_failure_not_a_fake_success` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19985-20077` `scaffold_parses_into_a_valid_config` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20091-20108` `shipped_workflows_carry_a_non_zero_spawn_budget` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20111-20123` `parse_canary_args_defaults_corpus_and_jobs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20126-20138` `parse_canary_args_reads_corpus_if_model_changed_and_jobs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20141-20153` `parse_canary_args_rejects_a_non_positive_jobs_value` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20156-20166` `parse_canary_args_rejects_unknown_flags` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20169-20198` `usage_text_gives_the_jobs_flag_its_own_description_line` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20201-20218` `usage_text_names_the_build_cache_reset_mode` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20221-20231` `parse_run_args_defaults_to_cli_and_an_unset_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20234-20245` `parse_run_args_reads_fresh_alongside_a_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20248-20258` `parse_run_args_reads_rebase_definition` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20261-20276` `parse_run_args_reads_driver_eventstore_conn_and_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20279-20287` `assert_every_arg_list_refused` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20306-20332` `parse_run_args_accepts_base_alongside_a_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20340-20362` `resolve_run_base_precedence_flag_then_env_then_default` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20368-20406` `parse_workflow_args_reads_spec_and_base` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20411-20463` `parse_step_args_reads_spec_and_base_with_default` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20469-20536` `definition_hash_is_stable_and_content_sensitive` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20545-20572` `kurrentdb_is_always_available_and_needs_a_conn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20584-20615` `store_selection_preserves_a_credentialed_tls_conn_verbatim` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20626-20803` `store_selection_precedence_flag_env_secret_file_config_then_default` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20806-20808` `project_identity_is_never_empty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20811-20836` `project_identity_reads_the_tracked_id_file_then_falls_back_to_the_basename` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20839-20866` `ssh_https_and_git_suffix_forms_of_one_repo_mint_identical_ids` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20869-20883` `normalize_origin_url_separates_distinct_repos_and_lowercases_only_the_host` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20886-20915` `decide_migration_covers_every_case` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20918-20957` `migrate_project_identity_renames_legacy_history_and_records_a_decision` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20960-20999` `migrate_project_identity_refuses_when_both_namespaces_hold_history` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21004-21027` `pre_mint_deployment` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21030-21071` `migrate_project_identity_rekeys_graph_rows_so_pre_mint_history_is_not_orphaned` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21074-21154` `migrate_project_identity_rekeys_the_graph_before_the_irreversible_stream_rename` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21157-21228` `migrate_project_identity_recovers_from_a_crash_between_the_rekey_and_the_rename` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21237-21342` `dash_graph_provider_reaches_the_whole_projection_when_run_seeds_are_empty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21356-21395` `dash_read_whole_graph_on_an_absent_db_is_empty_and_creates_nothing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21401-21433` `setup_provisions_the_shim_runtime_files` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21442-21501` `setup_installs_refreshes_and_is_a_noop_on_the_native_rigger_workflow` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21506-21515` `outcome_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21519-21524` `registry_entry` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21534-21604` `setup_installs_the_using_rigger_skill_distinct_from_the_workflow` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21622-21706` `planning_a_spec_installs_and_renders_through_the_registry_with_no_planning_specific_code` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21716-21752` `setup_skill_install_applies_the_project_overlay` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21758-21788` `docs_overlay_overrides_only_declared_fields` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21793-21807` `docs_overlay_malformed_is_a_loud_error` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21817-21845` `provision_shim_is_a_silent_noop_when_already_current` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21855-21874` `shim_is_not_current_when_node_modules_is_torn_missing_the_install_marker` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21881-21903` `init_project_is_idempotent_reporting_new_work_only_once` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21913-21971` `scaffold_summary_reports_only_the_gitignore_change_on_a_gitignore_only_repair` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21975-22003` `assert_init_gitignores_idempotently` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22039-22053` `a_group_or_other_readable_secret_file_mode_is_flagged_owner_only_is_not` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22065-22113` `init_project_still_writes_the_dash_ignore_lines_when_a_broader_rule_covers_them` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22122-22184` `scaffold_agents_and_workflow_reference_the_same_canonical_set` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22191-22234` `init_scaffolds_only_the_workflow_referenced_agents` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22242-22289` `init_project_errors_loudly_on_an_unknown_key_and_does_not_scaffold_agents` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22296-22325` `get_referenced_agent_ids_reads_the_scaffolded_workflows_fleet` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22336-22354` `get_referenced_agent_ids_errors_loudly_on_an_unknown_key_instead_of_returning_empty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22361-22393` `write_if_absent_wrote_kept_and_errors_naming_the_artifact` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22401-22418` `parse_setup_args_reads_the_agents_directory_flag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22425-22470` `import_agents_copies_and_normalizes_the_identity_field` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22476-22513` `import_agents_refuses_to_overwrite_an_existing_agent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22517-22533` `assert_import_refused` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22585-22607` `import_agents_runs_full_validation_and_rejects_a_broken_project` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22613-22632` `meta_object_body` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22641-22655` `meta_description` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22662-22670` `strip_line_comments` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22684-22907` `workflow_is_a_thin_courier_driver_with_per_unit_phase_labels` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22916-22927` `the_step_schema_admits_the_attention_array` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22943-22999` `phase_of_maps_wave_items_to_the_meta_phase_by_role_and_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23012-23050` `meta_matches_reality_drops_integrate_and_the_unit_stage_construction` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23069-23161` `the_driver_relays_each_attention_entry_as_a_narrator_log_line` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23165-23174` `assert_merge_hung_attention_leaves_untouched` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23214-23250` `merge_hung_attention_lands_in_canonical_position_alongside_other_signals` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23268-23316` `workflow_step_courier_prompt_is_foreground_and_honest` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23326-23335` `step_courier_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23353-23420` `workflow_step_courier_waits_on_an_auto_backgrounded_step` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23435-23443` `workflow_driver_guards_a_null_step_before_dereferencing_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23456-23513` `workflow_meta_description_is_a_user_facing_tagline_free_of_plumbing_terms` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23520-23544` `setup_runs_npm_install_or_reports_a_clear_error` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23550-23575` `workflow_locates_the_provisioned_shim_or_tells_you_to_run_setup` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23586-23629` `format_stats_prints_all_four_metrics` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23635-23651` `format_stats_handles_zeroed_metrics_without_nan` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23654-23656` `canary_scorecard` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23659-23675` `assert_findings_raised_render` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23690-23695` `assert_empty_scorecard_omits` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23711-23728` `progress_outcome` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23735-23747` `format_progress_line_names_id_verdict_and_none_when_nothing_caught` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23753-23770` `format_progress_line_reports_a_wrong_verdict_and_every_catching_tier` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23778-23809` `format_canary_stats_renders_na_for_a_tier_with_unattributed_correct_rejects` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23816-23835` `format_canary_stats_renders_the_real_zero_when_attribution_was_fully_measured` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23839-23854` `assert_control_line` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23879-23908` `format_canary_stats_reports_the_model_pinning_header_when_present` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23929-23971` `format_stats_surfaces_parallelism_retention_and_warns_below_the_floor` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23978-24018` `format_stats_surfaces_spawn_timing_and_reports_unpaired_separately` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24024-24030` `format_stats_spawn_timing_reports_no_spawns_when_none_recorded` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24037-24064` `format_stats_spawn_timing_no_spawns_line_requires_both_empty_and_zero_unpaired` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24071-24086` `format_stats_spawn_timing_all_unpaired_is_not_reported_as_no_spawns` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24094-24121` `parallelism_retention_line_is_single_sourced_and_warns_below_the_floor` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24125-24140` `sdet_review` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24143-24145` `stats_text` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24152-24165` `stats_discloses_when_no_verdict_was_recorded_on_this_driver` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24176-24206` `stats_discloses_unfed_numerator_when_verdict_recorded_but_findings_unattributed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24214-24251` `stats_discloses_cause_split_remainder_when_fewer_causes_than_rejects` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24256-24262` `cmd_stats_rejects_extra_arguments` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24265-24310` `baseline_run_slice_selects_a_run_by_id_including_a_middle_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24313-24344` `format_stats_diff_flags_only_the_changed_rows` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24356-24374` `build_environment_report_with_a_wrapper_lists_wrapper_cache_dir_and_budget` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24381-24398` `build_environment_report_with_no_wrapper_omits_cache_dir_but_keeps_budget` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24404-24414` `build_environment_report_zero_max_concurrent_reports_unlimited` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24420-24429` `build_environment_report_reports_mutation_gate_declared` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24435-24444` `build_environment_report_reports_mutation_gate_not_configured` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24462-24481` `order_signature_advisories_names_the_stream_count_range_and_repair_doc` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24485-24487` `order_signature_advisories_is_empty_when_no_signatures_are_given` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24495-24501` `drift_change` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24507-24536` `model_drift_advisory_is_a_soft_note_for_snapshot_only_drift` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24541-24552` `model_drift_advisory_stays_a_warning_when_any_change_is_a_real_model_repoint` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24556-24558` `model_drift_advisory_is_none_when_nothing_changed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24563-24579` `index_staleness_message_names_every_kind_of_disagreement_and_the_fix` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24586-24598` `graph_index_lag_advisory_names_every_lagging_file_and_the_fix` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24601-24603` `graph_index_lag_advisory_is_none_when_the_sample_is_empty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24606-24629` `bloat_advisory_is_none_at_or_below_the_threshold_and_named_above_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24637-24652` `assert_advisory_for_never_fabricates_a_missing_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24655-24657` `bloat_advisory_for_never_fabricates_a_store_that_does_not_exist` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24662-24680` `retired_entities_advisory_is_none_at_zero_and_named_with_correct_pluralization` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24683-24688` `retired_entities_advisory_for_never_fabricates_a_graph_that_does_not_exist` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24691-24725` `retired_entities_advisory_for_reads_the_projectors_own_counting_authority` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24732-24739` `scaffold_workflow_declares_build_wrapper_auto` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24747-24760` `scaffold_workflow_declares_max_parallel_units_two_with_a_sizing_comment` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24766-24779` `init_project_writes_max_parallel_units_with_its_sizing_comment` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24786-24793` `defaults_max_parallel_units_is_unbounded_when_the_key_is_absent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24801-24815` `init_project_never_clobbers_an_existing_build_section` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24818-24835` `parse_replay_args_requires_a_run_and_a_rev_in_either_order` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24847-24862` `cmd_stats_on_a_never_run_project_says_no_runs_and_creates_no_db` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24870-24911` `a_second_concurrent_rigger_step_refuses_and_the_lock_frees_on_release` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24917-24920` `no_runs_message_points_at_rigger_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24926-24933` `seed_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24938-24951` `assert_absent_db_reads_none` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24969-24985` `stats_lines_existing_db_with_empty_run_stream_returns_none` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24993-25022` `stats_lines_does_not_read_another_projects_namespaced_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25029-25060` `stats_lines_existing_run_renders_metric_lines` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25083-25152` `stats_lines_pairs_recorded_spawn_request_and_result_into_spawn_timing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25170-25179` `db_with_one_result` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25185-25194` `result_of_at_unrecorded_spawn_reads_as_unreported` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25202-25226` `result_of_at_reads_a_self_reported_result_so_it_is_not_clobbered` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25232-25251` `result_of_at_is_namespace_scoped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25258-25270` `cmd_reported_requires_exactly_one_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25283-25308` `detach_process_group_places_the_child_in_its_own_process_group` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25317-25338` `a_child_spawned_without_detachment_inherits_the_parent_process_group` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25352-25372` `spawn_run_dashboard_detached_session_detaches_the_dash` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25387-25407` `report_of` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25418-25441` `a_compaction_that_failed_after_the_deletes_is_reported_beside_the_counts` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25452-25475` `a_prune_that_shed_nothing_is_justified_by_this_log_not_by_when_it_was_written` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25486-25497` `a_pass_that_deleted_nothing_but_reclaimed_space_reports_the_reclamation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25502-25533` `a_prune_that_shed_rows_explains_the_duplication_a_deduplicated_log_still_accumulates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25547-25582` `an_unmeasurable_database_is_not_reported_as_a_checkpoint_a_reader_declined` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25586-25588` `no_live_units` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25592-25610` `live_writer_reasons_is_empty_only_when_all_four_facts_are_quiet` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25616-25635` `refusal_names_a_held_step_lock_and_the_force_live_override_owning_the_risk` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25640-25648` `refusal_names_a_non_terminal_unit_between_spawn_rounds` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25652-25664` `refusal_names_every_in_flight_spawn_id_and_the_count` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25668-25675` `refusal_names_the_driver_registration_count` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25680-25692` `refusal_names_every_applicable_reason_together_not_just_the_first` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25698-25727` `refuse_derived_reset_if_live_fails_safe_on_a_malformed_spawn_event` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25730-25733` `reset_modes_of` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25736-25742` `reset_refusal` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25748-25768` `reset_modes_parses_force_live_alongside_derived_rejects_duplicates_and_never_implies_a_mode` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25775-25797` `reset_modes_parses_scratch_orphans_alone_and_composed_and_rejects_duplicates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25800-25821` `reset_modes_parses_build_cache_alone_and_composed_and_rejects_duplicates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25829-25838` `watch_test_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25861-25894` `store_location_repo_root_resolves_the_owning_root_not_the_process_cwd_so_a_real_marker_is_found` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25908-25939` `scratch_defaults_reads_the_owning_roots_config_with_no_agents_fleet_present` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25942-25967` `watch_once_on_a_clean_store_reports_no_anomalies` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25970-25974` `parse_watch_args_defaults_to_streaming_with_the_default_interval` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25981-26003` `parse_watch_args_accepts_once_and_interval_together_in_either_order` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26019-26139` `watch_once_on_the_seeded_store_reports_one_line_per_anomaly` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26146-26165` `the_watchdog_command_signal_set_covers_every_signal_the_watch_skill_names` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26173-26201` `watch_once_reports_dash_not_serving_when_the_marker_names_a_dead_holder` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26208-26238` `watch_once_reports_no_anomaly_when_the_dash_marker_names_a_real_serving_holder` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26243-26266` `runs_menu_line_names_the_measured_counts_and_the_flag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26269-26298` `derived_menu_line_sums_the_measured_duplicate_counts_and_names_the_flag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26305-26320` `derived_menu_line_on_a_server_backend_says_so_instead_of_a_fabricated_count` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26325-26333` `implementer_persona_normalized` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26337-26346` `assert_implementer_persona_pins` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26501-26531` `no_persona_under_rigger_agents_invokes_cargo_mutants` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26544-26579` `install_operator_mcp_installs_refreshes_and_is_a_noop` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26585-26623` `install_lookup_hook_installs_refreshes_and_is_a_noop_and_preserves_foreign_hooks` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26631-26655` `assert_allows_with_literal_stripped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26669-26696` `grep_guard_decision_denies_every_bash_grep_target_and_passes_literal` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26702-26715` `grep_guard_decision_allows_non_grep_bash_commands` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26719-26727` `assert_grep_guard_denies` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26820-26825` `grep_guard_decision_ignores_other_tools` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26829-26832` `assert_literal_grep_passes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26846-26857` `grep_guard_decision_literal_survives_a_redirect_metacharacter_fused_grep` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26863-26876` `grep_guard_decision_literal_survives_a_quoted_literal_on_a_quoted_grep` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26883-26892` `grep_guard_decision_denies_a_grep_split_by_a_line_continuation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26903-26912` `grep_guard_decision_literal_survives_a_path_qualified_grep` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26917-26926` `grep_guard_decision_allows_a_path_qualified_non_grep_command` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26931-26939` `guard_write_target_reads_edit_and_write_file_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26942-26950` `guard_write_target_reads_notebook_edit_notebook_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26953-26961` `guard_write_target_ignores_every_other_tool` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26967-26976` `guard_write_target_reports_target_unreadable_for_a_covered_tool_missing_the_field` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26979-26993` `guard_write_target_reports_target_unreadable_for_a_non_string_field` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27002-27017` `guard_write_read_target_denies_a_non_object_top_level_value` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27020-27046` `guard_write_read_target_denies_an_absent_or_non_string_tool_name` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27049-27060` `guard_write_read_target_reaches_not_covered_only_through_a_real_string_tool_name` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27063-27078` `guard_write_read_target_denies_a_covered_tool_with_missing_or_non_object_tool_input` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27081-27093` `guard_write_read_target_reads_a_well_formed_covered_payload` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27096-27104` `guard_write_read_target_defaults_an_absent_cwd_to_empty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27107-27119` `resolve_write_target_passes_an_absolute_path_through` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27122-27127` `assert_resolves_under_cwd` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27135-27147` `resolve_write_target_walks_dot_dot_past_the_process_root_without_panicking` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27150-27168` `resolve_write_target_follows_a_symlinked_ancestor_directory` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27171-27184` `write_target_under_root_rejects_a_sibling_whose_name_merely_shares_a_prefix` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27187-27195` `write_target_under_root_treats_equal_as_under` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27198-27207` `write_target_under_root_accepts_a_descendant_and_rejects_a_sibling` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27210-27227` `guard_write_decision_allows_a_target_under_any_configured_root` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27230-27251` `guard_write_decision_denies_outside_every_root_naming_the_first` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27254-27265` `guard_write_decision_denies_a_dot_dot_escape` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27268-27274` `guard_write_decision_allows_a_tool_this_guard_does_not_cover` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27280-27290` `guard_write_decision_denies_target_unreadable_naming_the_first_root` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27293-27302` `parse_guard_write_roots_collects_every_root_in_order` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27305-27307` `parse_guard_write_roots_requires_at_least_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27310-27313` `parse_guard_write_roots_rejects_a_dangling_flag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27316-27319` `parse_guard_write_roots_rejects_an_unknown_argument` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).

### Unassigned (182 functions)

- `crates/rigger-conductor/src/conductor.rs:386-388` `adoption_provenance_key` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:457-465` `input_digest` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:654-671` `conflict_resolution_prompt` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:922-924` `conflict_regenerate_key` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:929-931` `cached` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:936-941` `clear_attempt` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:950-966` `integrate_row` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:1028-1033` `evidence_pair` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:1241-1243` `carries_marker` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:1528-1540` `replay_trajectory` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:1544-2216` `run` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:2295-2387` `compute_attention` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:11284-11293` `verified_evidence` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:11572-11619` `render_capped_section` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:11666-11676` `recency_by_own_edge` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:11827-11830` `plain_node_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:11961-11977` `confidence_tier_radius` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:11987-12041` `files_reachable` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:12309-12317` `current_run_spec` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:12466-12483` `branch_owner` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:12522-12528` `quarantine_branch_name` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:12623-12651` `quarantined_branch` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:12696-12701` `same_path` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:12708-12723` `sanitize_for_path` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:12728-12730` `integrates` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:12813-12819` `implement_slice` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `crates/rigger-conductor/src/conductor.rs:12983-13020` `validate_acyclic` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:168-177` `console_font_response` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:193-203` `free_port_from` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:259-291` `probe_dash_head` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:305-309` `dash_serving_on` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:418-420` `head_block_ended` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:687-690` `held_port_holder` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:732-736` `url_port` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:809-846` `dash_status` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:858-866` `dash_start_needed` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:920-931` `should_reap_singleton` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:1364-1477` `build_state` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:1868-1874` `parse_call_dir` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:1898-1906` `call_edge_view` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:1938-2024` `calls_view` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2098-2209` `build_run_tree` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2363-2382` `driver_stage` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2386-2394` `rollup` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2467-2474` `gates_status` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2485-2488` `advanced_past_gates` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2493-2502` `unit_live_status` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2507-2515` `spec_of` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2542-2565` `event_seed_ids` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2577-2600` `unit_seeds` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2615-2628` `repoint_seed` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2670-2672` `is_console_event` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2684-2693` `console_event_wire` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2700-2708` `console_progress_wire` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2712-2723` `event_view` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2749-2751` `now_unix` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2760-2779` `state_json` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2785-2793` `events_json` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2800-2861` `console_snapshot_json` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2865-2867` `live_page` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2873-2875` `console_page` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2882-2902` `render_export` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:3213-3231` `percent_decode` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:3234-3240` `query_param` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:3244-3249` `parse_request_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:3534-3541` `env_duration_ms` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:3556-3567` `write_sse` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:3576-3582` `write_retained_window_gone` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:273-275` `version_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:453-455` `open_sqlite_store` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:459-463` `env_conn` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:548-551` `config_rigger_dir` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:554-556` `cwd` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:752-754` `registry_heartbeat_interval` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:806-845` `register_run_instance` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:876-909` `refresh_registry_entry` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:921-923` `project_identity` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:948-959` `project_identity_at` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:966-968` `legacy_identity_at` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:973-986` `legacy_identity_from` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1007-1015` `has_tracked_project_id` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1020-1026` `canonical_definition_text` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1041-1081` `definition_hash` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1084-1089` `push_definition_section` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1112-1149` `enforce_definition_pin` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1158-1183` `normalize_origin_url` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1188-1202` `origin_url_at` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1208-1213` `mint_project_id` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1233-1247` `decide_migration` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1426-1489` `main` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1677-1679` `usage` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1681-1686` `db_path` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1728-1785` `walk_stores_from` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1798-1817` `main_repo_root` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1892-1941` `refuse_unless_one_root` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:2160-2198` `result_advisories` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:2213-2217` `load_run_config` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:2292-2312` `acquire_step_lock` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3000-3013` `merge_hung_attention` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3154-3157` `reap_then_remove_dir` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3168-3180` `reap_then_remove_worktree` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3342-3352` `result_of_at` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3461-3485` `warn_on_run_branch_divergence` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3498-3507` `anchor_run_branch` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3531-3545` `refuse_when_base_unreachable` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3617-3754` `run_cli` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3778-3822` `fresh_run_if_requested` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:4153-4170` `load_criteria` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:4491-4544` `definition_body` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:4571-4615` `locate_definition_extent` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:4623-4633` `locate_definition_extent` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:4808-4853` `run_graph_pass` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:4974-4997` `stats_lines` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:5018-5032` `parallelism_retention_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:5095-5116` `append_spawn_timing` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:5121-5123` `fmt_duration` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:5151-5261` `append_review_quality` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:5290-5301` `canary_stats_lines` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:5445-5489` `model_drift_advisory` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:5498-5515` `order_signature_advisories` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:6012-6022` `started_units` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:6043-6076` `candidate_reaches_gate` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:6120-6134` `baseline_run_slice` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:6137-6141` `run_started_id` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:6148-6190` `materialize_config_at_rev` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:6273-6291` `start_run_dashboard` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:6298-6313` `spawn_run_dashboard` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:6621-6676` `spawn_run_dashboard_detached` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:7215-7265` `watch_and_self_reap_on_idle` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:7457-7459` `instance_rigger_dir` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:8110-8116` `release_ready_lines` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:8237-8450` `watch_poll` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:8605-8611` `runs_menu_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:8618-8640` `derived_menu_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:8842-8931` `derived_prune_report` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9213-9246` `close_landed_units` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9250-9259` `landed_units` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9342-9346` `graph_node_id` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9405-9415` `peer_decision_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9419-9428` `json_str_array` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9432-9441` `json_type_name` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9537-9558` `build_result` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9806-9818` `fold_recorded_result_into_graph` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10063-10096` `build_environment_report` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10104-10143` `validate_advisories` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10150-10176` `index_staleness_message` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10185-10195` `graph_index_lag_advisory` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10248-10257` `retired_entities_advisory` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10270-10276` `retired_entities_advisory_for` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10371-10382` `missing_gitsemver_binary_advisory` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10394-10414` `behind_the_tree_message` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10433-10445` `behind_the_tree_advisory` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10465-10479` `drift_side` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10528-10549` `uncommitted_rigger_advisory` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10557-10572` `dirty_tracked_paths` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10612-10649` `residue_advisories` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10660-10677` `leaked_process_advisories` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10777-10823` `current_run_units` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10959-11020` `scan_residue` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:11097-11116` `dir_size_bytes` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:11138-11148` `build_cache_tombstone_path` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:11247-11260` `human_size` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:11492-11502` `dead_spawn_leaf_bytes` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:11512-11519` `looks_like_run_container` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:11725-11737` `owning_repo_root` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:11818-11941` `init_project` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:12054-12081` `get_referenced_agent_ids` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:12148-12150` `rigger_path` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:12383-12524` `precommit_block` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:12555-12607` `compose_precommit_bytes` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13108-13199` `import_agents` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13208-13238` `normalize_identity` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13243-13249` `top_level_key` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13480-13495` `instructions_in_force_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13563-13578` `select_grounder` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13586-13588` `select_reindex_grounder` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13755-13780` `grep_guard_decision` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13818-13880` `shell_command_word_spans` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13884-13890` `shell_command_words` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13906-13931` `strip_literal_marker` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13936-13941` `word_basename` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13952-13954` `command_invokes_grep` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.

## 2. Duplication Catalog

182 clusters (1253 total sites) across `src/` and `tests/`, found by `tests/simplification_audit.rs`'s deterministic normalized-token-shingle Jaccard pass (8-token shingles, threshold 0.72) plus five mandatory mechanical sweeps. Strict definition (spec 85 Goal): any logic present in more than one place anywhere in the codebase is a violation, with no "small enough to duplicate" exemption.

### Mandatory sweeps

- **Command::new call sites**: 83 site(s) - `dup-9f07a7bf395a`
- **/proc-path string literals**: 48 site(s) - `dup-af816213608d`
- **sqlite Connection::open call sites**: 34 site(s) - `dup-f4557659a212`
- **.rigger-path string literals**: 539 site(s) - `dup-f4f077a18177`
- **error-shaping helper functions**: 10 site(s) - `dup-be3f13e37fd5`

### Clusters (33 exact, 124 near, 25 semantic)

#### `dup-252bfd2c826c` (near, 9 sites)

Proposed home: `a new shared module (sites span 6 files: crates/rigger-conductor/src/conductor.rs, crates/rigger-domain/src/spawn.rs, crates/rigger-driver/src/driver/claude_code.rs, tests/common/fixtures/graph.rs, tests/postmerge_gate_error_cleanup_periphery.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:321-323` `review_round_start_key`
- `crates/rigger-conductor/src/conductor.rs:369-371` `compensation_queued_key`
- `crates/rigger-conductor/src/conductor.rs:922-924` `conflict_regenerate_key`
- `crates/rigger-domain/src/spawn.rs:118-120` `spawn_id`
- `crates/rigger-driver/src/driver/claude_code.rs:983-991` `stop_message`
- `tests/common/fixtures/graph.rs:152-154` `spoke_id`
- `tests/postmerge_gate_error_cleanup_periphery.rs:56-58` `expected_postmerge_dir`
- `tests/postmerge_gate_error_cleanup_periphery.rs:59-61` `expected_postmerge_branch`
- `tests/simplification_audit.rs:5996-5998` `sample_key`

#### `dup-59ab8e8cdb6e` (near, 5 sites)

Proposed home: `a new shared module (sites span 4 files: crates/rigger-conductor/src/conductor.rs, crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs, crates/rigger-store-sqlite/src/spawn_store.rs, tests/no_os_kill_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:386-388` `adoption_provenance_key`
- `crates/rigger-conductor/src/conductor.rs:399-401` `quarantine_record_key`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:1846-1848` `code_entity_id`
- `crates/rigger-store-sqlite/src/spawn_store.rs:63-65` `what`
- `tests/no_os_kill_audit.rs:52-54` `join`

#### `dup-27610bbbcb28` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-conductor/src/conductor.rs, crates/rigger-domain/src/spawn.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:430-432` `unit_of_gate_key`
- `crates/rigger-domain/src/spawn.rs:205-207` `unit_of`

#### `dup-d82ba3558c88` (near, 17 sites)

Proposed home: `a new shared module (sites span 11 files: crates/rigger-conductor/src/conductor.rs, crates/rigger-domain/src/spawn.rs, crates/rigger-grounder/src/grounder/mod.rs, crates/rigger-grounder/src/grounder/workflowdef.rs, crates/rigger-store-sqlite/src/eventstore/namespace.rs, crates/rigger-worktree-git/src/worktree.rs, src/main.rs, tests/canary_model_drift_periphery.rs, tests/halted_spawn_wip_recovery_periphery.rs, tests/regate_landed_on_resume_periphery.rs, tests/reset_derived_compaction_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:538-540` `deferred_gate_verdict_key`
- `crates/rigger-conductor/src/conductor.rs:549-551` `deferred_gate_failed_key`
- `crates/rigger-conductor/src/conductor.rs:11433-11440` `review_protocol`
- `crates/rigger-domain/src/spawn.rs:93-95` `lens_role`
- `crates/rigger-domain/src/spawn.rs:171-173` `speculation_group_id`
- `crates/rigger-grounder/src/grounder/mod.rs:134-140` `retired_grounder_error`
- `crates/rigger-grounder/src/grounder/workflowdef.rs:32-34` `stage_id`
- `crates/rigger-grounder/src/grounder/workflowdef.rs:36-38` `gate_id`
- `crates/rigger-grounder/src/grounder/workflowdef.rs:40-42` `agent_id`
- `crates/rigger-store-sqlite/src/eventstore/namespace.rs:69-71` `prefix_for`
- `crates/rigger-worktree-git/src/worktree.rs:1618-1620` `shared_build_cache_guard_path`
- `src/main.rs:12232-12234` `skill_source_rel`
- `src/main.rs:13425-13431` `spec_lint_next_step`
- `tests/canary_model_drift_periphery.rs:112-114` `prose_claiming`
- `tests/halted_spawn_wip_recovery_periphery.rs:129-131` `unit_branch`
- `tests/regate_landed_on_resume_periphery.rs:73-75` `unit_branch`
- `tests/reset_derived_compaction_periphery.rs:2490-2492` `derived_key_for`

#### `dup-7a935e08967c` (semantic, 2 sites)

Proposed home: `one shared `append_and_fold_batch` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-conductor/src/conductor.rs:2816-2838` `append_and_fold_batch`
- `crates/rigger-grounder/src/ingest.rs:48-88` `append_and_fold_batch`

#### `dup-f13596146362` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-conductor/src/conductor.rs, crates/rigger-domain/src/contextgraph/query.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:3444-3446` `spawn_is_recorded`
- `crates/rigger-domain/src/contextgraph/query.rs:456-458` `is_shared`

#### `dup-be3f13e37fd5` (semantic, 10 sites)

Proposed home: `one error-shaping helper module`

mandatory sweep: error-shaping helper functions - 10 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-conductor/src/conductor.rs:4574-4592` `guard_review_round_tree_on_tier_err`
- `crates/rigger-conductor/src/conductor.rs:25880-25938` `integrate_plan_commits_wraps_any_hard_error_with_the_plan_landing_marker`
- `crates/rigger-domain/src/agent.rs:301-303` `no_result_error`
- `crates/rigger-grounder/src/grounder/mod.rs:134-140` `retired_grounder_error`
- `src/worktree.rs:622-649` `land_reports_a_generic_error_for_a_refusal_that_is_neither_tip_moved_nor_blocked`
- `src/worktree.rs:1961-2015` `revert_on_base_aborts_and_errors_on_a_conflicting_revert`
- `tests/adoption_keys_on_criterion_periphery.rs:2004-2144` `a_quarantine_record_whose_ref_was_since_deleted_hard_errors_instead_of_silently_starting_fresh`
- `tests/batched_fold_cadence.rs:174-252` `append_and_fold_batch_is_best_effort_on_fold_error_and_a_no_op_on_an_empty_batch`
- `tests/canary_item_sharding_jobs_cap_periphery.rs:276-364` `run_canary_runs_every_item_to_completion_even_when_one_items_spawn_errors`
- `tests/grounder_name_contract.rs:40-171` `public_name_contract_predicate_error_and_resolver_agree`

#### `dup-edcfd444cd88` (near, 3 sites)

Proposed home: `conductor::run_ctx`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:9659-9673` `record_merge_attempt`
- `crates/rigger-conductor/src/conductor.rs:9705-9720` `record_landing_intent`
- `crates/rigger-conductor/src/conductor.rs:9730-9745` `record_landed`

#### `dup-e2420edc22e9` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-conductor/src/conductor.rs, crates/rigger-store-sqlite/src/eventstore/sqlite.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:12258-12260` `unit_branch`
- `crates/rigger-store-sqlite/src/eventstore/sqlite.rs:718-720` `key_expr`

#### `dup-c6dfc41a628c` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-conductor/src/conductor.rs, tests/plan_stage_commit_landing_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:14499-14504` `answering`
- `tests/plan_stage_commit_landing_periphery.rs:282-287` `new`

#### `dup-c36d696064fd` (near, 4 sites)

Proposed home: `conductor::stub`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:14537-14539` `prompts_for`
- `crates/rigger-conductor/src/conductor.rs:14542-14544` `dirs_for`
- `crates/rigger-conductor/src/conductor.rs:14548-14550` `system_prompt_for`
- `crates/rigger-conductor/src/conductor.rs:14554-14556` `title_for`

#### `dup-f5ed8c200bec` (near, 13 sites)

Proposed home: `a new shared module (sites span 3 files: crates/rigger-conductor/src/conductor.rs, crates/rigger-domain/src/eventstore.rs, tests/common/real_driver_spy.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:14566-14568` `spawn_ids`
- `crates/rigger-conductor/src/conductor.rs:32863-32865` `calls`
- `crates/rigger-conductor/src/conductor.rs:32866-32868` `targets`
- `crates/rigger-conductor/src/conductor.rs:32869-32871` `mutants_dirs`
- `crates/rigger-conductor/src/conductor.rs:32872-32874` `store_fences`
- `crates/rigger-conductor/src/conductor.rs:32875-32877` `build_cache_guards`
- `crates/rigger-conductor/src/conductor.rs:32878-32880` `build_cache_dirs`
- `crates/rigger-conductor/src/conductor.rs:33000-33002` `calls`
- `crates/rigger-domain/src/eventstore.rs:230-232` `last`
- `crates/rigger-domain/src/eventstore.rs:453-455` `recv`
- `crates/rigger-domain/src/eventstore.rs:463-465` `try_recv`
- `crates/rigger-domain/src/eventstore.rs:468-470` `err`
- `tests/common/real_driver_spy.rs:35-37` `outputs`

#### `dup-60da41568392` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-conductor/src/conductor.rs, crates/rigger-domain/src/eventstore.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:14572-14578` `spawned`
- `crates/rigger-domain/src/eventstore.rs:361-363` `covers`

#### `dup-9f07a7bf395a` (semantic, 83 sites)

Proposed home: `a single injected process-spawn port every Command::new site routes through instead of constructing its own Command`

mandatory sweep: Command::new call sites - 83 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-conductor/src/conductor.rs:14680-14680` `Command::new`
- `crates/rigger-conductor/src/conductor.rs:14685-14685` `Command::new`
- `crates/rigger-conductor/src/conductor.rs:24642-24642` `Command::new`
- `crates/rigger-conductor/src/conductor.rs:35795-35795` `Command::new`
- `crates/rigger-conductor/src/conductor.rs:35809-35809` `Command::new`
- `crates/rigger-conductor/src/conductor.rs:36582-36582` `Command::new`
- `crates/rigger-driver/src/reaped_child.rs:93-93` `Command::new`
- `crates/rigger-gates-shell/src/gate.rs:1445-1445` `Command::new`
- `crates/rigger-process/src/budget.rs:198-198` `Command::new`
- `crates/rigger-process/src/budget.rs:236-236` `Command::new`
- `crates/rigger-process/src/budget.rs:247-247` `Command::new`
- `crates/rigger-process/src/subprocess.rs:18-18` `Command::new`
- `src/main.rs:14971-14971` `Command::new`
- `src/main.rs:15004-15004` `Command::new`
- `src/main.rs:15048-15048` `Command::new`
- `src/main.rs:15117-15117` `Command::new`
- `src/main.rs:15709-15709` `Command::new`
- `src/main.rs:17284-17284` `Command::new`
- `src/main.rs:19144-19144` `Command::new`
- `src/main.rs:19251-19251` `Command::new`
- `src/main.rs:22896-22896` `Command::new`
- `src/main.rs:25286-25286` `Command::new`
- `src/main.rs:25320-25320` `Command::new`
- `src/worktree.rs:922-922` `Command::new`
- `tests/adaptive_labels_periphery.rs:87-87` `Command::new`
- `tests/adoption_keys_on_criterion_periphery.rs:2063-2063` `Command::new`
- `tests/checkin_mutation_diff_base_periphery.rs:158-158` `Command::new`
- `tests/checkin_mutation_diff_base_periphery.rs:203-203` `Command::new`
- `tests/claude_code_stream_periphery.rs:880-880` `Command::new`
- `tests/cli.rs:1460-1460` `Command::new`
- `tests/cli.rs:5290-5290` `Command::new`
- `tests/cli.rs:12964-12964` `Command::new`
- `tests/cli.rs:13705-13705` `Command::new`
- `tests/cli.rs:18102-18102` `Command::new`
- `tests/cli.rs:26376-26376` `Command::new`
- `tests/common/cli.rs:18-18` `Command::new`
- `tests/common/cli.rs:54-54` `Command::new`
- `tests/common/cli.rs:177-177` `Command::new`
- `tests/common/fixtures/conductor.rs:283-283` `Command::new`
- `tests/common/fixtures/conductor.rs:365-365` `Command::new`
- `tests/common/fixtures/git.rs:25-25` `Command::new`
- `tests/common/fixtures/git.rs:45-45` `Command::new`
- `tests/common/fixtures/host.rs:9-9` `Command::new`
- `tests/common/fixtures/host.rs:54-54` `Command::new`
- `tests/common/fixtures/host.rs:65-65` `Command::new`
- `tests/common/layer_cli.rs:71-71` `Command::new`
- `tests/common/mod.rs:143-143` `Command::new`
- `tests/common/repo.rs:210-210` `Command::new`
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
- `tests/integrate_conflict_merge_periphery.rs:329-329` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:1522-1522` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:1613-1613` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:1858-1858` `Command::new`
- `tests/meta_phases_declaration_periphery.rs:83-83` `Command::new`
- `tests/mutation_runner_pdeathsig_periphery.rs:107-107` `Command::new`
- `tests/native_driver_pipelining_behavior.rs:293-293` `Command::new`
- `tests/no_os_kill_test_helper_periphery.rs:29-29` `Command::new`
- `tests/no_os_kill_test_helper_periphery.rs:47-47` `Command::new`
- `tests/phase_of_role_mapping_periphery.rs:59-59` `Command::new`
- `tests/principle_gates_wiring.rs:146-146` `Command::new`
- `tests/principle_gates_wiring.rs:216-216` `Command::new`
- `tests/product_binary_authority_periphery.rs:155-155` `Command::new`
- `tests/reset_build_cache_periphery.rs:242-242` `Command::new`
- `tests/reset_build_cache_periphery.rs:254-254` `Command::new`
- `tests/reset_build_cache_periphery.rs:331-331` `Command::new`
- `tests/reset_build_cache_periphery.rs:349-349` `Command::new`
- `tests/revert_on_base_hook_bypass_periphery.rs:161-161` `Command::new`
- `tests/scaffold_grounder_resolves.rs:96-96` `Command::new`
- `tests/step_attention_periphery.rs:526-526` `Command::new`
- `tests/store_flag_precedence.rs:78-78` `Command::new`
- `tests/store_resolution.rs:193-193` `Command::new`
- `tests/turbovec_retired_cargo_boundary.rs:50-50` `Command::new`
- `tests/validate_behind_the_tree_periphery.rs:128-128` `Command::new`

#### `dup-f4f077a18177` (semantic, 539 sites)

Proposed home: `one .rigger-relative path-composition helper`

mandatory sweep: .rigger-path string literals - 539 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-conductor/src/conductor.rs:17215-17215` `"the repo's own .rigger config must load"`
- `crates/rigger-config-files/src/config_store.rs:362-362` `".rigger"`
- `crates/rigger-config-files/src/config_store.rs:363-363` `"create .rigger/instructions"`
- `crates/rigger-config-files/src/config_store.rs:416-416` `".rigger"`
- `crates/rigger-config-files/src/config_store.rs:417-417` `".rigger"`
- `crates/rigger-config-files/src/config_store.rs:441-441` `"../../examples/demo/.rigger"`
- `crates/rigger-config-files/src/config_store.rs:442-442` `".rigger"`
- `crates/rigger-config-files/src/config_store.rs:1520-1520` `".rigger/agents/sdet-author.md"`
- `crates/rigger-config-files/src/config_store.rs:1521-1521` `"the shipped .rigger/agents/sdet-author.md must exist"`
- `crates/rigger-config-files/src/config_store.rs:1548-1548` `".rigger/agents/sdet.md"`
- `crates/rigger-config-files/src/config_store.rs:1549-1549` `"the shipped .rigger/agents/sdet.md must exist"`
- `crates/rigger-config-files/src/config_store.rs:2000-2000` `".rigger"`
- `crates/rigger-config-files/src/config_store.rs:2001-2001` `"create .rigger dir"`
- `crates/rigger-config-files/src/config_store.rs:2033-2033` `".rigger"`
- `crates/rigger-config-files/src/config_store.rs:2034-2034` `"create .rigger dir"`
- `crates/rigger-config-files/src/config_store.rs:2055-2055` `".rigger"`
- `crates/rigger-config-files/src/config_store.rs:2056-2056` `"create .rigger dir"`
- `crates/rigger-domain/src/config.rs:20-20` `".rigger"`
- `crates/rigger-domain/src/instructions.rs:61-61` `"\nOperator (.rigger/instructions/*.md, filename order):\n"`
- `crates/rigger-gates-shell/src/gate.rs:319-319` `".rigger-cache-probe-{}"`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:4352-4352` `".rigger/workflow.yml"`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:4360-4360` `".rigger/workflow.yml"`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:4368-4368` `".rigger/workflow.yml"`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:4376-4376` `".rigger/workflow.yml"`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:4384-4384` `".rigger/workflow.yml"`
- `crates/rigger-grounder/src/grounder/mod.rs:333-333` `".rigger"`
- `crates/rigger-grounder/src/grounder/workflowdef.rs:481-481` `".rigger"`
- `crates/rigger-grounder/src/grounder/workflowdef.rs:568-568` `"this project's own .rigger/workflow.yml must extract at least one event"`
- `crates/rigger-grounder/src/ingest.rs:475-475` `".rigger"`
- `crates/rigger-grounder/src/ingest.rs:477-477` `".rigger"`
- `crates/rigger-grounder/src/ingest.rs:486-486` `"gw/.rigger/workflow.yml@"`
- `crates/rigger-grounder/src/ingest.rs:504-504` `"one code batch (a.rs) plus one workflow-definition batch (.rigger/workflow.yml) \
             must both advance the shared batch count; got {}"`
- `crates/rigger-grounder/src/ingest.rs:533-533` `".rigger"`
- `crates/rigger-grounder/src/ingest.rs:535-535` `".rigger"`
- `crates/rigger-grounder/src/ingest.rs:569-569` `".rigger"`
- `crates/rigger-process/src/reap.rs:731-731` `".rigger"`
- `crates/rigger-process/src/reap.rs:1073-1073` `"a relocated/cache-home-style authorized_root with no .rigger/tmp relationship \
             must still authorize the reap"`
- `crates/rigger-store-sqlite/src/registry.rs:323-323` `"/home/dev/proj/.rigger/events.db"`
- `crates/rigger-store-sqlite/src/registry.rs:332-332` `"/home/dev/proj/.rigger/events.db"`
- `crates/rigger-store-sqlite/src/registry.rs:349-349` `"/home/dev/proj/.rigger/events.db"`
- `crates/rigger-store-sqlite/src/registry.rs:356-356` `"/home/dev/proj/.rigger/events.db"`
- `crates/rigger-store-sqlite/src/registry.rs:370-370` `"/home/dev/proj-b/.rigger/events.db"`
- `crates/rigger-store-sqlite/src/registry.rs:388-388` `"/a/.rigger/events.db"`
- `crates/rigger-store-sqlite/src/registry.rs:389-389` `"/b/.rigger/events.db"`
- `crates/rigger-store-sqlite/src/registry.rs:415-415` `"/home/dev/proj/.rigger/events.db"`
- `crates/rigger-store-sqlite/src/registry.rs:433-433` `"/home/dev/proj/.rigger/events.db"`
- `src/dash.rs:3810-3810` `"{root}/.rigger/events.db"`
- `src/dash.rs:3859-3859` `"/.rigger/events.db"`
- `src/docs.rs:220-220` `"The EVENT LOG accumulates separately from the graph, and has its own prune: `rigger \
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
- `src/docs.rs:662-662` `"Store hygiene for rigger's own state - growing .rigger/ disk usage, \
         the bloat advisory from `rigger validate`, or `rigger step`/replay running slow. \
         Read this before running `rigger reset` or touching any store file by hand."`
- `src/docs.rs:666-666` `"rigger keeps three stores under `.rigger/`, and only one of them holds anything \
             durable:\n"`
- `src/docs.rs:722-722` `"`rigger graph build` folds the project's source straight into `.rigger/graph.db` - \
             no run, no `RunStarted`, nothing but the code-ingest events the fold already emits. \
             It CREATES the store when the checkout is cold (`.rigger/` does not exist yet) and \
             REFRESHES an existing store incrementally: an unchanged file re-ingests nothing, and \
             it reuses the exact same walk-and-content-key ingest authority a live run uses, so a \
             standalone build and a run can never fold the same file under two different keys.\n"`
- `src/docs.rs:732-732` `"Never force a rebuild by deleting `.rigger/graph.db` (or `events.db`) and \
         re-running `rigger graph build` on the empty result. Deleting the log throws away \
         truth that no rebuild can get back, and deleting only the graph is unnecessary work \
         `rigger graph build` already does FOR you, incrementally, without erasing anything \
         first. If lookups are empty, just run `rigger graph build`; only reach for \
         rigger-reset-store if you specifically mean to prune, not rebuild.\n"`
- `src/docs.rs:756-756` `"`rigger reindex <file>...` re-parses ONLY the named files and persists the delta to \
             the project's symbols grounding index at `.rigger/symbols/` - the fast, targeted fix \
             for an index that has drifted from files you just changed (a unit's own commit, a \
             rebase, a branch switch). It is scoped strictly to the symbols index, a DIFFERENT \
             store from the structural context graph, so it costs only the named files, never a \
             walk of the whole tree.\n"`
- `src/main.rs:600-600` `"the server event store is selected but no connection string is set - provide one via \
         --conn <url>, the KURRENTDB_CONN environment variable, or the .rigger/store.conn \
         secret file"`
- `src/main.rs:1306-1306` `"migrated project history to the durable identity: renamed {n} stream(s) \
                     from the legacy namespace {legacy:?} to the minted identity {minted:?} \
                     (.rigger/{PROJECT_ID_FILE})"`
- `src/main.rs:1373-1373` `"rigger: migrated project identity - renamed {n} stream(s) from the legacy \
             namespace {legacy:?} to the minted identity {minted:?} (.rigger/{PROJECT_ID_FILE})"`
- `src/main.rs:1496-1496` `"rigger - a config-driven, event-sourced multi-agent dev-loop harness\n\n\
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
DecisionMade before either mode prunes. When no driver is\n                              \
alive, it also closes the current run's units whose branch\n                              \
work is landed on rigger-run, appending the UnitIntegrated\n                              \
a hand landing never recorded\n  \
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
rigger reset --scratch-orphans\n                              \
reclaim every cache-home scratch root (under\n                              \
$XDG_CACHE_HOME/rigger, else ~/.cache/rigger) whose repo\n                              \
no longer exists: a root keyed on a deleted checkout or a\n                              \
test fixture's tempdir has no owner left to reclaim it.\n                              \
Rigger does this itself whenever it creates a default-\n                              \
placed root; this is the explicit on-demand form. Reports\n                              \
the number of roots reclaimed; composes with the others\n  \
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
rigger instructions         print the instructions every spawned agent is held to:\n                              \
the built-in engineering law, then each operator file\n                              \
in .rigger/instructions/ (filename order)\n  \
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
- `src/main.rs:4130-4130` `"workflow: the per-project JS driver is not provisioned (looked for {}). \
         Run `rigger setup` to write the shim into .rigger/shim/ and install its \
         dependencies, then re-run `rigger workflow`."`
- `src/main.rs:8964-8964` `"a `rigger step` is running right now (it holds .rigger/step.lock)"`
- `src/main.rs:10298-10298` `".rigger-workflow-provenance"`
- `src/main.rs:10542-10542` `"warning: tracked .rigger/ files have uncommitted modifications:"`
- `src/main.rs:11922-11922` `".rigger/shim/"`
- `src/main.rs:11923-11923` `".rigger/dash.url"`
- `src/main.rs:11924-11924` `".rigger/dash.marker"`
- `src/main.rs:11925-11925` `".rigger/dash.attempt"`
- `src/main.rs:11926-11926` `".rigger/store.conn"`
- `src/main.rs:12094-12094` `"minted the durable project identity in .rigger/{PROJECT_ID_FILE}: {id} \
             (commit it so a rename never orphans this project's history)"`
- `src/main.rs:12099-12099` `"scaffolded .rigger/workflow.yml"`
- `src/main.rs:12102-12102` `"scaffolded .rigger/instructions/README.md"`
- `src/main.rs:12106-12106` `"scaffolded .rigger/agents/{{{}}}"`
- `src/main.rs:12386-12386` `r#"__BEGIN__
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
- `src/main.rs:12830-12830` `"imported {} agent {} from {} into .rigger/agents/ ({} kept - already present)"`
- `src/main.rs:12880-12880` `"provisioned the JS driver in .rigger/shim/ (wrote shim.mjs + package.json + \
             package-lock.json and ran npm install)"`
- `src/main.rs:13159-13159` `"kept existing .rigger/agents/{name} (import never overwrites)"`
- `src/main.rs:13187-13187` `"imported .rigger/agents/{name} (id: {id})"`
- `src/main.rs:14464-14464` `"# Scaffolded by `rigger init`. A worked plan -> implement pipeline where the\n\
# review is PER UNIT: each unit implements, three-tier-reviews ITSELF (lenses ->\n\
# adversary -> adjudicator via defaults.review), and integrates in one lifecycle.\n\
# Replace the gate commands with your own.\n\
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
# How many implement units build at once (spec 102, per-unit pipelining). Each\n  \
# live unit owns its own build cache, and unbounded parallelism can fill disk on\n  \
# a wide fan-out. 0 (the default) is unbounded, unchanged until you set this; 2\n  \
# bounds the worst case to two live per-unit build caches at a time.\n  \
max_parallel_units: 2\n  \
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
# The boundary gate: Clean Architecture made mechanical. Replace with your\n  \
# project's own check that dependencies point inward and adapters are constructed\n  \
# only in the composition root (see this crate's tests/boundary_audit.rs for the\n  \
# worked example). A red boundary gate is non-negotiable: the adjudicator\n  \
# rejects, never balances it against other evidence.\n  \
boundary: { run: \"echo boundary ok; true\", kind: core }\n\
# The audit gate: DRY and YAGNI as a red gate. Replace with your project's own\n  \
# duplication and dead-code check; if it keeps a generated catalog, regenerate it\n  \
# before asserting so a unit is never red on a stale catalog alone.\n  \
audit: { run: \"echo audit ok; true\", kind: core }\n\
# The red-before-green gate: TDD made mechanical. Replace with a check that a\n  \
# unit's first source commit is preceded by (or carries) a test change (see this\n  \
# crate's .rigger/gates/red-before-green.sh for the worked example).\n  \
red-before-green: { run: \"echo red-before-green ok; true\", kind: core }\n\
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
gates: [build, audit, test, lint, boundary, red-before-green]  # red -> green enforced around the change\n    \
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
gates: [build, audit, test, lint, boundary, mutation]\n    \
on_pass: merge\n    \
coverage: \"mutation efficacy of the whole spec diff\"\n"`
- `src/main.rs:15829-15829` `"/.rigger/tmp/cargo-target/debug/rigger"`
- `src/main.rs:15830-15830` `"/.rigger/tmp/cargo-target/release/rigger"`
- `src/main.rs:17124-17124` `" M .rigger/workflow.yml\n\
                         M  .rigger/agents/sdet.md\n\
                         A  .rigger/agents/new.md\n\
                         D  .rigger/agents/gone.md\n\
                         ?? .rigger/events.db\n\
                         !! .rigger/shim/node_modules\n"`
- `src/main.rs:17134-17134` `".rigger/workflow.yml"`
- `src/main.rs:17135-17135` `".rigger/agents/sdet.md"`
- `src/main.rs:17136-17136` `".rigger/agents/new.md"`
- `src/main.rs:17137-17137` `".rigger/agents/gone.md"`
- `src/main.rs:18039-18039` `".rigger"`
- `src/main.rs:18043-18043` `".rigger"`
- `src/main.rs:18069-18069` `"probe/.rigger/events.db"`
- `src/main.rs:18070-18070` `"rigger-wt-x/.rigger/events.db"`
- `src/main.rs:18222-18222` `".rigger"`
- `src/main.rs:18257-18257` `"rigger-wt-unit-99-ghost-12345678/.rigger/events.db"`
- `src/main.rs:18292-18292` `"probe/.rigger/events.db"`
- `src/main.rs:18303-18303` `"shadow store: probe/.rigger/events.db (6B)"`
- `src/main.rs:18388-18388` `".rigger"`
- `src/main.rs:18455-18455` `".rigger"`
- `src/main.rs:18523-18523` `".rigger"`
- `src/main.rs:18602-18602` `".rigger"`
- `src/main.rs:18708-18708` `".rigger"`
- `src/main.rs:19277-19277` `".rigger"`
- `src/main.rs:19316-19316` `".rigger"`
- `src/main.rs:19450-19450` `".rigger"`
- `src/main.rs:19461-19461` `"must walk past the storeless worktree `.rigger/` to the repo's real store"`
- `src/main.rs:20471-20471` `".rigger"`
- `src/main.rs:20473-20473` `".rigger"`
- `src/main.rs:20516-20516` `".rigger"`
- `src/main.rs:20551-20551` `".rigger"`
- `src/main.rs:20587-20587` `".rigger"`
- `src/main.rs:20629-20629` `".rigger"`
- `src/main.rs:20735-20735` `".rigger/store.conn beats the committed config"`
- `src/main.rs:21408-21408` `"{name} must be written into .rigger/shim/"`
- `src/main.rs:21965-21965` `".rigger/agents/"`
- `src/main.rs:22018-22018` `".rigger/dash.url"`
- `src/main.rs:22019-22019` `".rigger/dash.marker"`
- `src/main.rs:22020-22020` `".rigger/dash.attempt"`
- `src/main.rs:22029-22029` `".rigger/store.conn"`
- `src/main.rs:22069-22069` `".rigger/\n"`
- `src/main.rs:22075-22075` `".rigger/dash.url"`
- `src/main.rs:22078-22078` `".rigger/dash.marker"`
- `src/main.rs:22081-22081` `".rigger/dash.attempt"`
- `src/main.rs:22082-22082` `"setup appends the explicit dash lines (including the round-8 attempt breadcrumb) \
             even when .rigger/ broadly covers them, so the committed .gitignore stays \
             self-contained, got: {:?}"`
- `src/main.rs:22090-22090` `".rigger/dash.url"`
- `src/main.rs:22091-22091` `".rigger/dash.marker"`
- `src/main.rs:22092-22092` `".rigger/dash.attempt"`
- `src/main.rs:22093-22093` `"all three explicit per-file dash ignore lines are present in the committed \
             .gitignore even though .rigger/ already covers them, got:\n{content}"`
- `src/main.rs:22103-22103` `".rigger/dash.url"`
- `src/main.rs:22106-22106` `".rigger/dash.marker"`
- `src/main.rs:22109-22109` `".rigger/dash.attempt"`
- `src/main.rs:22453-22453` `".rigger/agents/researcher.md"`
- `src/main.rs:22482-22482` `".rigger/agents/planner.md"`
- `src/main.rs:22512-22512` `".rigger/agents/newcomer.md"`
- `src/main.rs:22590-22590` `".rigger/workflow.yml"`
- `src/main.rs:23569-23569` `"locate_shim must return the provisioned .rigger/shim/shim.mjs"`
- `src/main.rs:24642-24642` `".rigger"`
- `src/main.rs:24696-24696` `".rigger"`
- `src/worktree.rs:2331-2331` `"{repo_path}/.rigger/tmp"`
- `src/worktree.rs:2332-2332` `"the default must no longer nest inside the repo's own .rigger: {dflt:?}"`
- `src/worktree.rs:2335-2335` `"/.rigger/"`
- `src/worktree.rs:2335-2335` `"/.rigger"`
- `src/worktree.rs:2336-2336` `"the default must never live under any .rigger: {dflt:?}"`
- `src/worktree.rs:2355-2355` `"{repo_path}/.rigger/tmp"`
- `src/worktree.rs:2379-2379` `"~/.rigger-scratch-test"`
- `src/worktree.rs:2380-2380` `"{home}/.rigger-scratch-test"`
- `src/worktree.rs:3952-3952` `"{base}..rigger-run"`
- `src/worktree.rs:3986-3986` `".rigger"`
- `tests/architecture_current_surface.rs:93-93` `".rigger/store.conn"`
- `tests/architecture_current_surface.rs:145-145` `"docs/architecture.md must describe the system that exists today (spec 56, \
         criterion 1): it must name the store-resolution and configuration surface (the \
         committed `store:` selection, the `KURRENTDB_CONN` environment variable, and the \
         per-machine `.rigger/store.conn` secret file) and the graph inspector's real query \
         surface (the three `lens=` names and the directed `view=calls` `dir=` views). \
         Surfaces the document fails to name: {missing:#?}"`
- `tests/canary_model_drift_periphery.rs:62-62` `".rigger"`
- `tests/change_path_revert_periphery.rs:81-81` `".rigger"`
- `tests/checkin_mutation_diff_base_periphery.rs:53-53` `".rigger"`
- `tests/checkpoint_commit_hook_bypass_periphery.rs:58-58` `"{repo_path}/.rigger-test-scratch"`
- `tests/cli.rs:99-99` `".rigger"`
- `tests/cli.rs:103-103` `".rigger"`
- `tests/cli.rs:164-164` `".rigger"`
- `tests/cli.rs:625-625` `".rigger"`
- `tests/cli.rs:876-876` `".rigger"`
- `tests/cli.rs:1033-1033` `".rigger"`
- `tests/cli.rs:1054-1054` `".rigger"`
- `tests/cli.rs:1159-1159` `"/.rigger/"`
- `tests/cli.rs:1160-1160` `"the default must never live under any .rigger, even on the HOME-only fallback \
         rung; got: {stdout:?}"`
- `tests/cli.rs:1212-1212` `"{}/.rigger/tmp"`
- `tests/cli.rs:1224-1224` `"/.rigger/tmp/"`
- `tests/cli.rs:1316-1316` `".rigger"`
- `tests/cli.rs:1326-1326` `".rigger"`
- `tests/cli.rs:1391-1391` `".rigger"`
- `tests/cli.rs:1392-1392` `".rigger"`
- `tests/cli.rs:1404-1404` `".rigger"`
- `tests/cli.rs:1425-1425` `".rigger"`
- `tests/cli.rs:1479-1479` `".rigger"`
- `tests/cli.rs:1519-1519` `".rigger"`
- `tests/cli.rs:1539-1539` `".rigger"`
- `tests/cli.rs:1554-1554` `".rigger"`
- `tests/cli.rs:1673-1673` `".rigger"`
- `tests/cli.rs:2413-2413` `".rigger"`
- `tests/cli.rs:2454-2454` `".rigger"`
- `tests/cli.rs:2486-2486` `".rigger"`
- `tests/cli.rs:2487-2487` `"graph build must create .rigger/graph.db from a cold checkout"`
- `tests/cli.rs:2537-2537` `".rigger"`
- `tests/cli.rs:2538-2538` `"graph build must create .rigger/graph.db even when there is nothing to ingest"`
- `tests/cli.rs:2561-2561` `".rigger"`
- `tests/cli.rs:2641-2641` `".rigger"`
- `tests/cli.rs:2656-2656` `".rigger"`
- `tests/cli.rs:2706-2706` `".rigger"`
- `tests/cli.rs:2731-2731` `".rigger"`
- `tests/cli.rs:3193-3193` `".rigger"`
- `tests/cli.rs:3197-3197` `"grounding via symbols must persist the structural index to .rigger/symbols/"`
- `tests/cli.rs:3369-3369` `".rigger"`
- `tests/cli.rs:3658-3658` `".rigger"`
- `tests/cli.rs:3696-3696` `".rigger"`
- `tests/cli.rs:3891-3891` `"cd ${REPO} && CARGO_TARGET_DIR=${REPO}/.rigger/tmp/cargo-target rigger step"`
- `tests/cli.rs:3939-3939` `"cd ${REPO} && CARGO_TARGET_DIR=${REPO}/.rigger/tmp/cargo-target rigger step"`
- `tests/cli.rs:4126-4126` `".rigger/tmp/agent-scratch/"`
- `tests/cli.rs:4132-4132` `".rigger/tmp/cargo-target"`
- `tests/cli.rs:4140-4140` `"CARGO_TARGET_DIR=${REPO}/.rigger/tmp/cargo-target rigger step"`
- `tests/cli.rs:4610-4610` `".rigger"`
- `tests/cli.rs:4712-4712` `".rigger"`
- `tests/cli.rs:5353-5353` `".rigger"`
- `tests/cli.rs:5553-5553` `".rigger"`
- `tests/cli.rs:5720-5720` `".rigger"`
- `tests/cli.rs:5947-5947` `".rigger"`
- `tests/cli.rs:6097-6097` `".rigger"`
- `tests/cli.rs:6285-6285` `".rigger"`
- `tests/cli.rs:6430-6430` `".rigger"`
- `tests/cli.rs:6639-6639` `".rigger"`
- `tests/cli.rs:6838-6838` `".rigger"`
- `tests/cli.rs:6926-6926` `".rigger"`
- `tests/cli.rs:7047-7047` `".rigger"`
- `tests/cli.rs:7240-7240` `".rigger/events.db"`
- `tests/cli.rs:7721-7721` `"/.rigger/"`
- `tests/cli.rs:7722-7722` `"the default marker path must never live under any .rigger; got: {marker_str:?}"`
- `tests/cli.rs:8230-8230` `"/.rigger/tmp/agent-live/"`
- `tests/cli.rs:8231-8231` `"under RIGGER_TMPDIR the marker is not under the repo's .rigger/tmp; got: {marker_str:?}"`
- `tests/cli.rs:8936-8936` `".rigger"`
- `tests/cli.rs:9099-9099` `".rigger"`
- `tests/cli.rs:9274-9274` `".rigger"`
- `tests/cli.rs:9403-9403` `".rigger"`
- `tests/cli.rs:9450-9450` `".rigger"`
- `tests/cli.rs:9564-9564` `".rigger"`
- `tests/cli.rs:9836-9836` `".rigger"`
- `tests/cli.rs:9935-9935` `".rigger"`
- `tests/cli.rs:10024-10024` `".rigger"`
- `tests/cli.rs:10064-10064` `".rigger"`
- `tests/cli.rs:10759-10759` `".rigger"`
- `tests/cli.rs:11128-11128` `".rigger/workflow.yml"`
- `tests/cli.rs:11128-11128` `".rigger/agents"`
- `tests/cli.rs:11216-11216` `".rigger/workflow.yml"`
- `tests/cli.rs:11216-11216` `".rigger/agents"`
- `tests/cli.rs:11218-11218` `".rigger"`
- `tests/cli.rs:11219-11219` `".rigger/workflow.yml"`
- `tests/cli.rs:11424-11424` `".rigger/workflow.yml"`
- `tests/cli.rs:11424-11424` `".rigger/agents"`
- `tests/cli.rs:11471-11471` `".rigger/workflow.yml"`
- `tests/cli.rs:11471-11471` `".rigger/agents"`
- `tests/cli.rs:11484-11484` `".rigger"`
- `tests/cli.rs:11490-11490` `".rigger"`
- `tests/cli.rs:11492-11492` `".rigger"`
- `tests/cli.rs:11589-11589` `".rigger/workflow.yml"`
- `tests/cli.rs:11590-11590` `"validate must NOT flag a clean tracked `.rigger/` tree; stderr:\n{err}"`
- `tests/cli.rs:11599-11599` `".rigger"`
- `tests/cli.rs:11606-11606` `"validate must still succeed (exit 0) when it only FLAGS uncommitted `.rigger/` \
         changes; stderr:\n{err}"`
- `tests/cli.rs:11610-11610` `".rigger/workflow.yml"`
- `tests/cli.rs:11611-11611` `"validate must flag the tracked-but-modified `.rigger/workflow.yml` on stderr; \
         stderr:\n{err}"`
- `tests/cli.rs:11627-11627` `".rigger"`
- `tests/cli.rs:11952-11952` `".rigger"`
- `tests/cli.rs:12101-12101` `".rigger"`
- `tests/cli.rs:12137-12137` `".rigger"`
- `tests/cli.rs:12239-12239` `".rigger"`
- `tests/cli.rs:12383-12383` `".rigger"`
- `tests/cli.rs:12385-12385` `".rigger"`
- `tests/cli.rs:12387-12387` `".rigger"`
- `tests/cli.rs:12389-12389` `".rigger"`
- `tests/cli.rs:12417-12417` `"probe/.rigger/events.db"`
- `tests/cli.rs:12471-12471` `"validate must warn about residue planted under the relocated cache-home DEFAULT \
         root - a regression that left its residue scan still rooted at the pre-relocation \
         `.rigger/tmp` would silently miss this and print nothing; stderr:\n{err}"`
- `tests/cli.rs:12496-12496` `".rigger"`
- `tests/cli.rs:13231-13231` `".rigger"`
- `tests/cli.rs:13328-13328` `"scaffolded .rigger/workflow.yml"`
- `tests/cli.rs:13332-13332` `"scaffolded .rigger/agents/"`
- `tests/cli.rs:13364-13364` `".rigger"`
- `tests/cli.rs:13396-13396` `".rigger/agents/researcher.md"`
- `tests/cli.rs:13397-13397` `"the agent was actually imported into .rigger/agents/"`
- `tests/cli.rs:13444-13444` `".rigger"`
- `tests/cli.rs:13625-13625` `".rigger/dash.url"`
- `tests/cli.rs:13629-13629` `".rigger/dash.marker"`
- `tests/cli.rs:13633-13633` `".rigger/dash.attempt"`
- `tests/cli.rs:13642-13642` `".rigger"`
- `tests/cli.rs:13644-13644` `".rigger"`
- `tests/cli.rs:13648-13648` `".rigger"`
- `tests/cli.rs:13649-13649` `".rigger"`
- `tests/cli.rs:13651-13651` `".rigger/dash.url"`
- `tests/cli.rs:13652-13652` `".rigger/dash.marker"`
- `tests/cli.rs:13653-13653` `".rigger/dash.attempt"`
- `tests/cli.rs:13690-13690` `".claude/\n.rigger/\n"`
- `tests/cli.rs:13706-13706` `".rigger/dash.url"`
- `tests/cli.rs:13714-13714` `"the test's global config must actually ignore .rigger/dash.url (else the regression \
         guard is inconclusive)"`
- `tests/cli.rs:13737-13737` `".rigger/shim"`
- `tests/cli.rs:13738-13738` `".rigger/dash.url"`
- `tests/cli.rs:13739-13739` `".rigger/dash.marker"`
- `tests/cli.rs:13740-13740` `".rigger/dash.attempt"`
- `tests/cli.rs:13872-13872` `".rigger/project.id"`
- `tests/cli.rs:13875-13875` `".rigger/project.id"`
- `tests/cli.rs:13888-13888` `".rigger/project.id"`
- `tests/cli.rs:13977-13977` `".rigger/project.id"`
- `tests/cli.rs:14026-14026` `".rigger/project.id"`
- `tests/cli.rs:14031-14031` `".rigger/project.id"`
- `tests/cli.rs:14045-14045` `".rigger"`
- `tests/cli.rs:14273-14273` `".rigger"`
- `tests/cli.rs:14876-14876` `".rigger"`
- `tests/cli.rs:15005-15005` `".rigger"`
- `tests/cli.rs:15007-15007` `".rigger"`
- `tests/cli.rs:15156-15156` `".rigger"`
- `tests/cli.rs:15513-15513` `".rigger"`
- `tests/cli.rs:15557-15557` `".rigger"`
- `tests/cli.rs:15994-15994` `".rigger"`
- `tests/cli.rs:16009-16009` `"the driver never recorded a dash URL in .rigger/dash.url; stderr:\n{err}"`
- `tests/cli.rs:16069-16069` `".rigger"`
- `tests/cli.rs:16376-16376` `".rigger"`
- `tests/cli.rs:17054-17054` `".rigger"`
- `tests/cli.rs:17056-17056` `".rigger"`
- `tests/cli.rs:17512-17512` `".rigger/dash.marker"`
- `tests/cli.rs:17538-17538` `".rigger/dash.url"`
- `tests/cli.rs:17546-17546` `".rigger/dash.marker"`
- `tests/cli.rs:17768-17768` `".rigger/dash.url"`
- `tests/cli.rs:17770-17770` `".rigger/dash.marker"`
- `tests/cli.rs:17841-17841` `".rigger/dash.marker"`
- `tests/cli.rs:17848-17848` `".rigger/dash.attempt"`
- `tests/cli.rs:18688-18688` `".rigger"`
- `tests/cli.rs:19409-19409` `".rigger"`
- `tests/cli.rs:19743-19743` `".rigger"`
- `tests/cli.rs:19778-19778` `".rigger"`
- `tests/cli.rs:19853-19853` `"the first step must record a dash marker at .rigger/dash.marker; stderr:\n{err1}"`
- `tests/cli.rs:19934-19934` `"the step must record a dash marker at .rigger/dash.marker; stderr:\n{err}"`
- `tests/cli.rs:19948-19948` `"`rigger step` under RIGGER_DASH_PORT={dash_port} must bind its step-path dash at EXACTLY \
         that port and record it in .rigger/dash.marker (proving the override reaches the real \
         bind, not the fixed dash::DEFAULT_PORT); the marker instead recorded {marker_port}"`
- `tests/cli.rs:20009-20009` `".rigger"`
- `tests/cli.rs:20093-20093` `".rigger"`
- `tests/cli.rs:20478-20478` `".rigger"`
- `tests/cli.rs:20490-20490` `".rigger"`
- `tests/cli.rs:20517-20517` `".rigger"`
- `tests/cli.rs:20545-20545` `".rigger"`
- `tests/cli.rs:20556-20556` `".rigger"`
- `tests/cli.rs:20593-20593` `".rigger"`
- `tests/cli.rs:20682-20682` `"the first step must record a dash marker at .rigger/dash.marker; stderr:\n{err1}"`
- `tests/cli.rs:20751-20751` `"{root}/.rigger/events.db"`
- `tests/cli.rs:21370-21370` `"{other_root}/.rigger/events.db"`
- `tests/cli.rs:21756-21756` `"/stale/root/.rigger/events.db"`
- `tests/cli.rs:21777-21777` `"/live/root/.rigger/events.db"`
- `tests/cli.rs:21859-21859` `"the step must record a dash marker at .rigger/dash.marker"`
- `tests/cli.rs:22059-22059` `".rigger"`
- `tests/cli.rs:22241-22241` `".rigger"`
- `tests/cli.rs:22344-22344` `".rigger"`
- `tests/cli.rs:22469-22469` `".rigger"`
- `tests/cli.rs:23096-23096` `".rigger/workflow.yml"`
- `tests/cli.rs:23100-23100` `".rigger/workflow.yml must define a `checkin:` stage (spec 91): {text:?}"`
- `tests/cli.rs:23104-23104` `".rigger/workflow.yml must define a `mutation:` gate that invokes cargo mutants \
         (spec 91): {text:?}"`
- `tests/cli.rs:23109-23109` `".rigger/workflow.yml's checkin stage / mutation gate definition must name spec 91, \
         so drift in the committed workflow fails this suite instead of silently diverging \
         from the spec it satisfies: {text:?}"`
- `tests/cli.rs:23135-23135` `"this repository's own .rigger/workflow.yml and agents must load: {e}"`
- `tests/cli.rs:23142-23142` `".rigger/workflow.yml must define a `checkin` stage (spec 91)"`
- `tests/cli.rs:23177-23177` `".rigger/workflow.yml must define a `mutation` gate (spec 91)"`
- `tests/cli.rs:23198-23198` `"this repository's own committed .rigger/workflow.yml must pass Config::validate \
         on a correctly-provisioned machine (cargo-mutants installed)"`
- `tests/cli.rs:23243-23243` `".rigger/dash.attempt"`
- `tests/cli.rs:23244-23244` `"a real step's own ensure_run_dashboard call must record .rigger/dash.attempt \
         (record_dash_attempt); without it this test cannot exercise the round-8 fact at all"`
- `tests/cli.rs:23319-23319` `".rigger/dash.marker"`
- `tests/cli.rs:23348-23348` `".rigger/dash.url"`
- `tests/cli.rs:23355-23355` `".rigger/dash.attempt"`
- `tests/cli.rs:23415-23415` `".rigger/dash.marker"`
- `tests/cli.rs:23418-23418` `".rigger/dash.url"`
- `tests/cli.rs:23471-23471` `".rigger/dash.marker"`
- `tests/cli.rs:23493-23493` `".rigger/dash.url"`
- `tests/cli.rs:23498-23498` `".rigger/dash.attempt"`
- `tests/cli.rs:23536-23536` `".rigger/dash.url"`
- `tests/cli.rs:23547-23547` `".rigger/dash.marker"`
- `tests/cli.rs:24027-24027` `".rigger"`
- `tests/cli.rs:24180-24180` `".rigger"`
- `tests/cli.rs:25179-25179` `".rigger"`
- `tests/cli.rs:25204-25204` `".rigger"`
- `tests/cli.rs:25299-25299` `".rigger"`
- `tests/cli.rs:25536-25536` `".rigger"`
- `tests/cli.rs:25997-25997` `"the hook must be inert on a project without .rigger/; got:\n{out}"`
- `tests/cli.rs:26003-26003` `".rigger"`
- `tests/cli.rs:26022-26022` `".rigger"`
- `tests/cli.rs:26048-26048` `".rigger"`
- `tests/cli.rs:26084-26084` `".rigger"`
- `tests/cli.rs:26123-26123` `".rigger"`
- `tests/cli.rs:26327-26327` `".rigger"`
- `tests/cli.rs:26366-26366` `".rigger"`
- `tests/cli.rs:26518-26518` `".rigger"`
- `tests/cli.rs:26691-26691` `".rigger"`
- `tests/cli.rs:26740-26740` `".rigger"`
- `tests/cli.rs:26771-26771` `"scaffolded .rigger/instructions/README.md"`
- `tests/cli.rs:26775-26775` `".rigger/instructions/README.md"`
- `tests/common/cli.rs:155-155` `".rigger"`
- `tests/common/cli.rs:166-166` `".rigger"`
- `tests/common/cli.rs:187-187` `".rigger"`
- `tests/common/cli.rs:204-204` `".rigger"`
- `tests/common/cli.rs:371-371` `".rigger"`
- `tests/common/cli.rs:372-372` `"create .rigger/agents"`
- `tests/common/cli.rs:436-436` `"{why}: a server selection must NOT fabricate a local .rigger/events.db"`
- `tests/common/cli.rs:486-486` `".rigger"`
- `tests/common/fixtures/config.rs:101-101` `"{repo_path}/.rigger-test-scratch"`
- `tests/common/layer_cli.rs:22-22` `".rigger"`
- `tests/common/layer_cli.rs:28-28` `".rigger"`
- `tests/common/mod.rs:281-281` `"{}/.rigger-test-scratch"`
- `tests/common/workflow_probe.rs:15-15` `".rigger"`
- `tests/common/workflow_probe.rs:16-16` `"create .rigger"`
- `tests/config_unknown_key_dotted_path_periphery.rs:138-138` `".rigger"`
- `tests/config_unknown_key_dotted_path_periphery.rs:139-139` `"write the .rigger/{rel} fixture: {e}"`
- `tests/config_unknown_key_dotted_path_periphery.rs:149-149` `".rigger"`
- `tests/config_unknown_key_dotted_path_periphery.rs:423-423` `".rigger"`
- `tests/courier_registry_refresh_boundary_periphery.rs:83-83` `".rigger"`
- `tests/courier_registry_refresh_fence_periphery.rs:51-51` `".rigger"`
- `tests/dedup_seeding_periphery.rs:342-342` `".rigger"`
- `tests/dedup_seeding_periphery.rs:366-366` `".rigger"`
- `tests/dedup_seeding_periphery.rs:576-576` `".rigger"`
- `tests/duplication_catalog_contract_periphery.rs:103-103` `".rigger-path string literals"`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:121-121` `".rigger"`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:157-157` `".rigger"`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:277-277` `".rigger"`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:566-566` `".rigger"`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:1042-1042` `".rigger"`
- `tests/gate_store_fence_periphery.rs:201-201` `".rigger"`
- `tests/gate_store_fence_periphery.rs:202-202` `".rigger"`
- `tests/gate_store_fence_periphery.rs:207-207` `".rigger"`
- `tests/gate_store_fence_periphery.rs:208-208` `".rigger"`
- `tests/gate_store_fence_periphery.rs:210-210` `".rigger"`
- `tests/gate_store_fence_periphery.rs:215-215` `".rigger"`
- `tests/gate_store_fence_periphery.rs:266-266` `".rigger"`
- `tests/gate_store_fence_periphery.rs:267-267` `".rigger"`
- `tests/gate_store_fence_periphery.rs:274-274` `".rigger"`
- `tests/gate_store_fence_periphery.rs:277-277` `".rigger"`
- `tests/gate_store_fence_periphery.rs:510-510` `".rigger"`
- `tests/gate_store_fence_periphery.rs:533-533` `".rigger"`
- `tests/gate_store_fence_periphery.rs:646-646` `".rigger"`
- `tests/gate_store_fence_periphery.rs:647-647` `".rigger"`
- `tests/gate_store_fence_periphery.rs:657-657` `".rigger"`
- `tests/gate_store_fence_periphery.rs:659-659` `".rigger"`
- `tests/gate_store_fence_periphery.rs:761-761` `".rigger"`
- `tests/gate_store_fence_periphery.rs:762-762` `".rigger"`
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
- `tests/heartbeat_write_read_agree_periphery.rs:92-92` `".rigger"`
- `tests/heartbeat_write_read_agree_periphery.rs:128-128` `".rigger"`
- `tests/heartbeat_write_read_agree_periphery.rs:232-232` `".rigger"`
- `tests/heartbeat_write_read_agree_periphery.rs:237-237` `".rigger"`
- `tests/init_setup_unknown_key_agent_fleet_periphery.rs:55-55` `".rigger"`
- `tests/init_setup_unknown_key_agent_fleet_periphery.rs:56-56` `"create .rigger"`
- `tests/init_setup_unknown_key_agent_fleet_periphery.rs:93-93` `".rigger"`
- `tests/init_setup_unknown_key_agent_fleet_periphery.rs:95-95` `"create .rigger/agents"`
- `tests/integrate_conflict_merge_periphery.rs:451-451` `"the project's own .rigger/workflow.yml must load through the real loader"`
- `tests/migration_is_deliberate_periphery.rs:475-475` `".rigger"`
- `tests/migration_is_deliberate_periphery.rs:522-522` `".rigger"`
- `tests/mutation_scratch_reap_base_guard_periphery.rs:86-86` `"reclaim_unit_mutation_scratch's own doc comment promises every process rooted in a \
         matched registered mutation-scratch dir is reaped BEFORE the dir is removed - a \
         SIGTERM-ignoring process here must still be SIGKILLed. Round 1 broke this: \
         is_reapable_base's <repo>/.rigger/tmp containment requirement refused this real, \
         ALWAYS-outside-any-repo registered root (see this file's header doc comment and \
         decision u78c2-mutation-scratch-reap-now-refused-flagging-for-review), so \
         reap_processes_rooted_under silently no-opped. Round 2 (decision \
         u78c2r2-authorized-root-caller-supplied) fixed it by passing the registered \
         mutation-scratch root itself as authorized_root - a regression back to the round-1 \
         shape would fail this assertion again."`
- `tests/mutation_scratch_reap_base_guard_periphery.rs:114-114` `".rigger"`
- `tests/mutation_scratch_reap_base_guard_periphery.rs:141-141` `"control: when the registered scratch root legitimately lies under a real repo's \
         .rigger/tmp, reclaim_unit_mutation_scratch must still reap a live process rooted in \
         it - proving the sibling test's failure is specifically the base-guard's new scope, \
         not a defect in this file's own mechanics"`
- `tests/principle_gates_wiring.rs:43-43` `"sh .rigger/gates/red-before-green.sh"`
- `tests/principle_gates_wiring.rs:94-94` `".rigger/workflow.yml: {missing:#?}"`
- `tests/principle_gates_wiring.rs:106-106` `"the scaffolded .rigger/: {missing:#?}"`
- `tests/principle_gates_wiring.rs:215-215` `".rigger/gates/red-before-green.sh"`
- `tests/principle_gates_wiring.rs:316-316` `".rigger/agents"`
- `tests/principle_gates_wiring.rs:326-326` `".rigger/agents: {missing:#?}"`
- `tests/projections_stay_local.rs:90-90` `"the graph projection must be opened by the LOCAL sqlite Projector at .rigger/graph.db \
         (`Projector::open(&db_path(\"graph.db\") ...)`); the canonical local construction is gone"`
- `tests/projections_stay_local.rs:97-97` `"the progress projection must be opened by the LOCAL sqlite Store at .rigger/progress.db \
         (`Store::open(&db_path(\"progress.db\") ...)`); the canonical local construction is gone"`
- `tests/projections_stay_local.rs:186-186` `".rigger"`
- `tests/projections_stay_local.rs:189-189` `"graph.db must be created under the LOCAL .rigger/ even when the event store is the \
             server - projections are per-machine and stay local"`
- `tests/projections_stay_local.rs:199-199` `".rigger"`
- `tests/projections_stay_local.rs:200-200` `"a server-configured `graph build` must NOT create a local .rigger/events.db - the \
             event log is the server's; only the projection is local"`
- `tests/projections_stay_local.rs:253-253` `".rigger"`
- `tests/projections_stay_local.rs:256-256` `"progress.db must be created under the LOCAL .rigger/ even when the event store is the \
             server - the progress store is a local projection, not the shared log"`
- `tests/projections_stay_local.rs:279-279` `".rigger"`
- `tests/projections_stay_local.rs:280-280` `"a server-configured `rigger progress` must NOT create a local .rigger/events.db - the \
             run log is the server's; only the progress projection is local"`
- `tests/published_content_key_split_periphery.rs:258-258` `".rigger"`
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
- `tests/relocated_worktree_store_resolution_periphery.rs:55-55` `".rigger"`
- `tests/relocated_worktree_store_resolution_periphery.rs:56-56` `"create .rigger"`
- `tests/relocated_worktree_store_resolution_periphery.rs:65-65` `".rigger"`
- `tests/relocated_worktree_store_resolution_periphery.rs:132-132` `".rigger"`
- `tests/reset_build_cache_periphery.rs:425-425` `".rigger"`
- `tests/reset_build_cache_periphery.rs:504-504` `".rigger"`
- `tests/reset_derived_compaction_periphery.rs:1271-1271` `".rigger"`
- `tests/reset_derived_compaction_periphery.rs:2964-2964` `".rigger"`
- `tests/reset_derived_compaction_periphery.rs:2964-2964` `"create .rigger"`
- `tests/reset_derived_compaction_periphery.rs:2979-2979` `".rigger"`
- `tests/reset_derived_compaction_periphery.rs:3014-3014` `".rigger"`
- `tests/reset_derived_live_writer_guard_periphery.rs:39-39` `".rigger"`
- `tests/reset_derived_live_writer_guard_periphery.rs:48-48` `".rigger"`
- `tests/reset_derived_live_writer_guard_periphery.rs:124-124` `"{root}/.rigger/events.db"`
- `tests/reset_derived_live_writer_guard_periphery.rs:259-259` `".rigger"`
- `tests/reset_derived_live_writer_guard_periphery.rs:360-360` `".rigger"`
- `tests/review_round_lenses_only_log_derived_resume_periphery.rs:144-144` `"{repo_path}/.rigger-test-scratch"`
- `tests/review_round_no_adjudicator_residue_periphery.rs:93-93` `"{repo_path}/.rigger-test-scratch"`
- `tests/review_round_non_ancestor_residue_names_true_diff_periphery.rs:119-119` `"{repo_path}/.rigger-test-scratch"`
- `tests/simplification_audit.rs:3111-3111` `".rigger-path string literals"`
- `tests/simplification_audit.rs:3296-3296` `".rigger"`
- `tests/simplification_audit.rs:3297-3297` `"one .rigger-relative path-composition helper"`
- `tests/simplification_audit.rs:5425-5425` `"Largest risk-reduction first is read as six tiers, ranked by the KIND of risk \
        each entry retires, highest first:\n\n\
        1. Tier 1 - active correctness risk, PLUS item 0: a use case already depends on the \
        wrong concretion, or two independent implementations of one concern can already \
        drift apart silently (section 3's two boundary violations; the one already-drifted \
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
- `tests/simplification_audit.rs:5723-5723` `"#### 10. Consolidate the {rigger_n} `.rigger`-path string-literal sites (`{rigger_id}`) - the \
        single largest cluster in the entire catalog by site count\n\n"`
- `tests/simplification_audit.rs:5727-5727` `"- Scope: one `.rigger`-relative path-composition helper (the cluster's own \
        `proposed_home`) every one of the {rigger_n} sites routes through instead of building its \
        own literal.\n\
        - Files: spans dozens of files including `crates/rigger-conductor/src/conductor.rs`, `crates/rigger-config-files/src/config_store.rs`, \
        `src/dash.rs`, `src/docs.rs`, `crates/rigger-gates-shell/src/gate.rs`, `crates/rigger-grounder/src/grounder/mod.rs`, \
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
- `tests/simplification_audit.rs:8862-8862` `"fn a() {\n    let _ = \".rigger/tmp\";\n}\n"`
- `tests/simplification_audit.rs:8864-8864` `".rigger"`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:59-59` `".rigger"`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:222-222` `".rigger"`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:233-233` `".rigger"`
- `tests/statusline_command_periphery.rs:118-118` `".rigger"`
- `tests/step_sheds_the_freshen.rs:67-67` `".rigger/grounding"`
- `tests/step_sheds_the_freshen.rs:216-216` `".rigger/symbols/index.json"`
- `tests/step_sheds_the_freshen.rs:217-217` `"the surviving persisted index is the SYMBOL index under .rigger/symbols/; got {}"`
- `tests/step_sheds_the_freshen.rs:282-282` `"constructing the default grounder builds and persists the SYMBOL index under \
         .rigger/symbols/ - the freshen's real target"`
- `tests/stop_failure_hook_periphery.rs:56-56` `".rigger/progress.db"`
- `tests/stop_failure_hook_periphery.rs:226-226` `".rigger/events.db"`
- `tests/stop_failure_hook_periphery.rs:241-241` `".rigger/progress.db"`
- `tests/store_content_identity_periphery.rs:628-628` `".rigger"`
- `tests/store_content_identity_periphery.rs:629-629` `"create .rigger"`
- `tests/store_content_identity_periphery.rs:675-675` `".rigger"`
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
- `tests/store_resolution.rs:197-197` `".rigger"`
- `tests/store_resolution.rs:229-229` `".rigger"`
- `tests/store_resolution.rs:230-230` `"a server-configured courier must NOT create a local .rigger/events.db - that is the \
             state-fracture this criterion closes"`
- `tests/store_resolution.rs:293-293` `".rigger"`
- `tests/store_resolution.rs:326-326` `".rigger"`
- `tests/store_resolution.rs:330-330` `".rigger"`
- `tests/store_resolution_cli.rs:86-86` `"a server-configured courier must NOT create a local .rigger/events.db - that is the \
         state-fracture this criterion closes, and it must hold even when the server is down"`
- `tests/store_secrets.rs:55-55` `".rigger"`
- `tests/store_secrets.rs:105-105` `"{why}: a server-configured courier must NOT create a local .rigger/events.db: {stderr}"`
- `tests/store_secrets.rs:129-129` `".rigger/store.conn secret-file channel"`
- `tests/store_secrets.rs:145-145` `".rigger"`
- `tests/validate_advisories.rs:236-236` `".rigger"`
- `tests/validate_advisories.rs:439-439` `".rigger"`
- `tests/validate_footprint_default_scratch_root_periphery.rs:34-34` `".rigger"`
- `tests/validate_footprint_default_scratch_root_periphery.rs:35-35` `".rigger"`
- `tests/validate_footprint_default_scratch_root_periphery.rs:70-70` `".rigger"`
- `tests/watchdog_cli_periphery.rs:176-176` `".rigger"`
- `tests/watchdog_cli_periphery.rs:364-364` `".rigger"`
- `tests/workflow_definition_and_js_constants_periphery.rs:89-89` `".rigger"`
- `tests/workflow_definition_and_js_constants_periphery.rs:91-91` `".rigger"`
- `tests/workflow_definition_and_js_constants_periphery.rs:364-364` `".rigger"`
- `tests/workflow_definition_and_js_constants_periphery.rs:366-366` `".rigger"`
- `tests/workflow_definition_and_js_constants_periphery.rs:614-614` `".rigger"`
- `tests/workflow_driver_resolved_model_periphery.rs:237-237` `".rigger"`
- `tests/worktree_liveness_fence_periphery.rs:110-110` `".rigger"`

#### `dup-97f834a327e5` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-conductor/src/conductor.rs, tests/postmerge_gate_error_cleanup_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:28189-28204` `spawn`
- `tests/postmerge_gate_error_cleanup_periphery.rs:71-82` `spawn`

#### `dup-ae7ef878296c` (near, 5 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-conductor/src/conductor.rs, tests/revert_on_base_hook_bypass_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:34187-34228` `spawn`
- `crates/rigger-conductor/src/conductor.rs:34730-34772` `spawn`
- `crates/rigger-conductor/src/conductor.rs:35193-35230` `spawn`
- `crates/rigger-conductor/src/conductor.rs:35338-35383` `spawn`
- `tests/revert_on_base_hook_bypass_periphery.rs:78-111` `spawn`

#### `dup-b91a09b322bf` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-conductor/src/conductor.rs, tests/replan_episode_identity.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-conductor/src/conductor.rs:38467-38477` `critique_cfg`
- `tests/replan_episode_identity.rs:273-283` `two_episode_cfg`

#### `dup-78c48ff4c88e` (semantic, 2 sites)

Proposed home: `one shared `gating_agent_ids` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-config-files/src/config.rs:21-38` `gating_agent_ids`
- `crates/rigger-domain/src/config.rs:413-422` `gating_agent_ids`

#### `dup-3d575d71e10f` (exact, 2 sites)

Proposed home: `config_store::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-config-files/src/config_store.rs:181-189` `read_store_config`
- `crates/rigger-config-files/src/config_store.rs:245-253` `read_scratch_defaults`

#### `dup-6f0fcb54c987` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: crates/rigger-config-files/src/config_store.rs, src/main.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-config-files/src/config_store.rs:739-772` `literal_is_emit_payload_binds_only_an_abutting_payload_or_emit_word`
- `src/main.rs:17770-17776` `is_uuid8_accepts_exactly_eight_hex_digits`
- `tests/simplification_audit.rs:8868-8876` `looks_error_shaping_matches_error_and_underscore_bounded_err_but_not_an_incidental_substring`

#### `dup-5c52773cc009` (near, 2 sites)

Proposed home: `config_store::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-config-files/src/config_store.rs:934-941` `parses_agent_frontmatter_and_body`
- `crates/rigger-config-files/src/config_store.rs:1289-1298` `model_ladder_parses_from_frontmatter`

#### `dup-2b2924a65629` (exact, 4 sites)

Proposed home: `a new shared module (sites span 4 files: crates/rigger-config-files/src/config_store.rs, crates/rigger-domain/src/contextgraph/query.rs, crates/rigger-domain/src/spec.rs, src/main.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-config-files/src/config_store.rs:944-946` `rejects_missing_frontmatter`
- `crates/rigger-domain/src/contextgraph/query.rs:1937-1939` `graph_load_rejects_malformed_json_without_panicking`
- `crates/rigger-domain/src/spec.rs:986-988` `empty_when_no_criteria`
- `src/main.rs:17145-17147` `dirty_tracked_paths_on_a_clean_tree_is_empty`

#### `dup-2a5626cb9627` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-domain/src/agent.rs, crates/rigger-domain/src/spec.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/agent.rs:235-250` `as_str`
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
- `crates/rigger-domain/src/eventstore.rs:218-220` `handed`
- `crates/rigger-domain/src/failure.rs:225-227` `is_empty`
- `crates/rigger-domain/src/metrics.rs:573-575` `adversary_precision`
- `crates/rigger-driver/src/reaped_child.rs:44-46` `id`

#### `dup-a9ed3e0e9d47` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: crates/rigger-domain/src/community.rs, crates/rigger-domain/src/eventstore.rs, crates/rigger-domain/src/failure.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/community.rs:249-251` `nodes`
- `crates/rigger-domain/src/eventstore.rs:355-357` `types`
- `crates/rigger-domain/src/failure.rs:230-232` `rules`

#### `dup-781b5348543b` (near, 4 sites)

Proposed home: `a new shared module (sites span 3 files: crates/rigger-domain/src/concepts.rs, crates/rigger-domain/src/ingest.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/concepts.rs:74-76` `is_intent_doc`
- `crates/rigger-domain/src/concepts.rs:80-82` `is_label_doc`
- `crates/rigger-domain/src/ingest.rs:45-47` `is_derived_index_type`
- `tests/simplification_audit.rs:2421-2423` `is_keyword`

#### `dup-1f030cd524f7` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: crates/rigger-domain/src/config.rs, crates/rigger-domain/src/failure.rs, src/main.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/config.rs:375-377` `is_empty`
- `crates/rigger-domain/src/failure.rs:168-170` `is_any`
- `src/main.rs:10598-10603` `is_empty`

#### `dup-619325cc4dc0` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: crates/rigger-domain/src/config.rs, tests/no_os_kill_audit.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/config.rs:1460-1462` `is_word_byte`
- `tests/no_os_kill_audit.rs:59-61` `is_word_char`
- `tests/simplification_audit.rs:204-206` `is_ident_char`

#### `dup-bb7aa56f67d8` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-domain/src/eventstore.rs, crates/rigger-domain/src/spawn.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/eventstore.rs:131-134` `with_valid_from`
- `crates/rigger-domain/src/spawn.rs:453-456` `with_meta`

#### `dup-cab8f470b99c` (near, 4 sites)

Proposed home: `a new shared module (sites span 4 files: crates/rigger-domain/src/failure.rs, crates/rigger-domain/src/gate.rs, crates/rigger-domain/src/ledger.rs, crates/rigger-domain/src/watch.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/failure.rs:43-49` `as_str`
- `crates/rigger-domain/src/gate.rs:97-103` `as_str`
- `crates/rigger-domain/src/ledger.rs:47-59` `as_str`
- `crates/rigger-domain/src/watch.rs:193-202` `response`

#### `dup-3b3a41e03376` (exact, 3 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-domain/src/failure.rs, crates/rigger-domain/src/gate.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/failure.rs:65-67` `reruns`
- `crates/rigger-domain/src/failure.rs:74-76` `demotes_on_persistent_failure`
- `crates/rigger-domain/src/gate.rs:73-75` `runs_inline`

#### `dup-e9220ec4784e` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-domain/src/ingest.rs, tests/published_content_key_split_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/ingest.rs:92-95` `derived_key_parts`
- `tests/published_content_key_split_periphery.rs:39-42` `split`

#### `dup-65d6d5073566` (near, 8 sites)

Proposed home: `a new shared module (sites span 3 files: crates/rigger-domain/src/ledger.rs, src/console/map.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/ledger.rs:874-881` `short_run_id_truncates_to_twelve_chars_and_passes_shorter_ids_through`
- `crates/rigger-domain/src/ledger.rs:884-899` `spec_stem_extracts_and_sanitizes_the_file_stem`
- `src/console/map.rs:1250-1253` `module_of_a_leaf_src_file_is_its_stem_never_the_file_name`
- `src/console/map.rs:1282-1286` `district_purpose_uses_the_curated_table_when_present`
- `tests/simplification_audit.rs:7700-7711` `impl_self_type_strips_a_trailing_where_clause_on_a_non_generic_self_type`
- `tests/simplification_audit.rs:7731-7742` `impl_self_type_strips_a_leading_dyn_token_on_the_self_type`
- `tests/simplification_audit.rs:7754-7760` `strip_trailing_where_clause_is_a_word_boundary_match_not_a_substring_match`
- `tests/simplification_audit.rs:8319-8329` `ident_kind_marker_classifies_by_casing`

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
- `crates/rigger-store-sqlite/src/eventstore/sqlite.rs:503-509` `factor`

#### `dup-c16dbfd88b00` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-domain/src/metrics.rs, crates/rigger-domain/src/spawn.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/metrics.rs:1446-1448` `changed`
- `crates/rigger-domain/src/spawn.rs:459-461` `is_error`

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
- `crates/rigger-domain/src/run.rs:359-361` `decision`
- `crates/rigger-domain/src/run.rs:362-364` `finding`
- `crates/rigger-domain/src/run.rs:365-367` `lesson`

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

#### `dup-3885489aa2a0` (semantic, 3 sites)

Proposed home: `one shared `current_run_id` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-domain/src/run.rs:164-166` `current_run_id`
- `src/mcpserver.rs:532-538` `current_run_id`
- `tests/halted_spawn_wip_recovery_periphery.rs:586-589` `current_run_id`

#### `dup-2b572fcabbd4` (semantic, 2 sites)

Proposed home: `one shared `liveness_fault` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-domain/src/spawn.rs:444-450` `liveness_fault`
- `tests/dash_run_tree_spine.rs:81-85` `liveness_fault`

#### `dup-e3c162b7bd56` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: crates/rigger-domain/src/spec.rs, crates/rigger-grounder/src/grounder/design/extract.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/spec.rs:541-548` `starts_new_element`
- `crates/rigger-grounder/src/grounder/design/extract.rs:433-436` `is_markdown`
- `tests/simplification_audit.rs:3172-3179` `looks_error_shaping`

#### `dup-42b8a5023d8d` (near, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/spec.rs:652-654` `find_word`
- `crates/rigger-domain/src/spec.rs:662-664` `find_word_across_hyphen`

#### `dup-81e8434ac795` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-domain/src/spec.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/spec.rs:1854-1866` `disposition_check_fails_closed_after_a_stray_unmatched_quote_earlier_in_the_paragraph`
- `crates/rigger-domain/src/spec.rs:2064-2073` `disposition_check_resumes_scanning_after_notes_ends`
- `tests/simplification_audit.rs:7349-7353` `a_trait_method_signature_without_a_body_is_not_recorded`

#### `dup-ee034ee789c2` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: crates/rigger-domain/src/spec.rs, crates/rigger-store-sqlite/src/eventstore/mod.rs, src/console/map.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/spec.rs:1913-1934` `strip_inline_code_direct_exact_output_pins_the_one_span_per_kind_rule`
- `crates/rigger-store-sqlite/src/eventstore/mod.rs:218-235` `a_delimiter_inside_the_userinfo_never_leaks_the_credential_head`
- `src/console/map.rs:1952-1967` `budget_scales_the_step_by_zoom_rather_than_offsetting_it`

#### `dup-9f14620abab1` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-domain/src/spec.rs, crates/rigger-driver/src/liveness.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/spec.rs:2203-2205` `heading_level_rejects_more_than_six_hashes`
- `crates/rigger-driver/src/liveness.rs:719-734` `marker_filename_is_none_only_for_a_truly_empty_input_so_a_join_can_never_be_a_no_op`

#### `dup-d340e212490f` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-domain/src/watch.rs, crates/rigger-driver/src/driver/workflow.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-domain/src/watch.rs:578-580` `new`
- `crates/rigger-driver/src/driver/workflow.rs:83-85` `new`

#### `dup-cd5c22f2a701` (semantic, 2 sites)

Proposed home: `one shared `spawn_request` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-driver/src/driver/replay.rs:257-278` `spawn_request`
- `tests/common/mod.rs:453-467` `spawn_request`

#### `dup-282069e7109f` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: crates/rigger-driver/src/driver/replay.rs, tests/common/fixtures/config.rs, tests/model_pinning_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-driver/src/driver/replay.rs:384-391` `sonnet_agent`
- `tests/common/fixtures/config.rs:172-182` `fan_out_stage`
- `tests/model_pinning_periphery.rs:61-68` `agent`

#### `dup-7ce260b96af1` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: crates/rigger-driver/src/driver/replay.rs, tests/claude_code_stream_periphery.rs, tests/common/fixtures/conductor.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-driver/src/driver/replay.rs:397-404` `opts_for`
- `tests/claude_code_stream_periphery.rs:132-138` `opts`
- `tests/common/fixtures/conductor.rs:19-26` `implementer_opts`

#### `dup-b43ee3b4b5bb` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-driver/src/driver/replay.rs, crates/rigger-driver/src/liveness.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-driver/src/driver/replay.rs:540-548` `spawn_scratch_path_is_none_rather_than_relative_for_an_empty_scratch_root`
- `crates/rigger-driver/src/liveness.rs:845-853` `marker_path_is_none_rather_than_relative_for_an_empty_scratch_root`

#### `dup-8c5011c59ed1` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-driver/src/driver/replay.rs, tests/common/fixtures/config.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-driver/src/driver/replay.rs:1262-1271` `stage`
- `tests/common/fixtures/config.rs:14-20` `agent_with_prompt`

#### `dup-4cf25824ad00` (near, 2 sites)

Proposed home: `replay::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-driver/src/driver/replay.rs:2003-2051` `an_emit_only_approve_gating_persona_hard_errors_on_the_replay_driver`
- `crates/rigger-driver/src/driver/replay.rs:2127-2176` `a_parked_unanswered_sibling_does_not_suppress_this_units_own_approve_backstop`

#### `dup-e33cc9f09356` (semantic, 2 sites)

Proposed home: `one shared `install_status_line` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-driver/src/hooks.rs:137-154` `install_status_line`
- `src/main.rs:13042-13046` `install_status_line`

#### `dup-1a78461a4b17` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-driver/src/liveness.rs, src/main.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-driver/src/liveness.rs:762-776` `marker_filename_is_injective_so_two_ids_that_collided_under_a_prior_placeholder_scheme_no_longer_do`
- `src/main.rs:20869-20883` `normalize_origin_url_separates_distinct_repos_and_lowercases_only_the_host`

#### `dup-c0f477204b6e` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-driver/src/sidecar.rs, tests/store_content_identity_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-driver/src/sidecar.rs:188-190` `len`
- `tests/store_content_identity_periphery.rs:77-79` `batch_calls`

#### `dup-c650c3e62b50` (exact, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:1963-1975` `calls_out`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:2031-2043` `callers_direct`

#### `dup-1cee568104ee` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:2787-2800` `subgraph_finds_the_governing_decision`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:7817-7837` `recording_proof_never_wipes_the_entitys_own_name_kind_and_line_attrs`

#### `dup-cc43c38b38ca` (near, 9 sites)

Proposed home: `a new shared module (sites span 3 files: crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs, tests/code_ingest_events.rs, tests/concepts_fold_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:3204-3211` `apply_edge_inferred`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:3245-3252` `apply_edge_inferred_evidence`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:4091-4098` `apply_doc_concept`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:4201-4208` `apply_doc_link`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:4531-4538` `apply_batch_ref`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:6060-6070` `apply_unit_integrated`
- `tests/code_ingest_events.rs:841-848` `apply_ref_json`
- `tests/code_ingest_events.rs:993-1000` `apply_ref_fresh`
- `tests/concepts_fold_periphery.rs:52-59` `realized`

#### `dup-b5bd50de3e2a` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:3220-3239` `apply_code_entity_partial`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:3281-3299` `apply_community`

#### `dup-84342f753367` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:4211-4334` `design_intent_link_events_fold_into_the_five_design_intent_edges`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:4337-4455` `workflow_definition_events_fold_into_stage_gate_agent_nodes_with_needs_runs_reviews_edges`

#### `dup-3e472366e265` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:5793-5826` `decision_fold_projects_no_agent_node_or_decided_edge`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:5880-5908` `review_finding_projects_no_raised_edge_even_with_an_event_actor`

#### `dup-c76c4d91d8a8` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:6603-6605` `edge_projects`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:7637-7645` `index_names`

#### `dup-f4557659a212` (semantic, 34 sites)

Proposed home: `one sqlite-connection-opening adapter function every caller is injected with`

mandatory sweep: sqlite Connection::open call sites - 34 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:6699-6699` `Connection::open`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:7469-7469` `Connection::open`
- `crates/rigger-store-sqlite/src/eventstore/sqlite.rs:1459-1459` `Connection::open`
- `crates/rigger-store-sqlite/src/eventstore/sqlite.rs:1706-1706` `Connection::open_with_flags`
- `crates/rigger-store-sqlite/src/sqlite.rs:13-13` `Connection::open`
- `src/main.rs:26099-26099` `Connection::open`
- `tests/cli.rs:638-638` `Connection::open`
- `tests/cli.rs:741-741` `Connection::open`
- `tests/cli.rs:844-844` `Connection::open`
- `tests/cli.rs:887-887` `Connection::open`
- `tests/cli.rs:9572-9572` `Connection::open`
- `tests/common/cli.rs:497-497` `Connection::open`
- `tests/common/fixtures/sqlite.rs:8-8` `Connection::open`
- `tests/common/fixtures/sqlite.rs:22-22` `Connection::open`
- `tests/graph_additive_indexes_persist.rs:69-69` `Connection::open`
- `tests/graph_additive_indexes_persist.rs:165-165` `Connection::open`
- `tests/graph_additive_indexes_persist.rs:189-189` `Connection::open`
- `tests/heartbeat_write_read_agree_periphery.rs:137-137` `Connection::open`
- `tests/reset_derived_compaction.rs:73-73` `Connection::open`
- `tests/reset_derived_compaction.rs:229-229` `Connection::open`
- `tests/reset_derived_compaction.rs:494-494` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:113-113` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:543-543` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:1280-1280` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:2531-2531` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:2736-2736` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:3524-3524` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:4164-4164` `Connection::open`
- `tests/reset_derived_live_writer_guard_periphery.rs:39-39` `Connection::open`
- `tests/reset_menu.rs:60-60` `Connection::open`
- `tests/reset_menu.rs:64-64` `Connection::open`
- `tests/reset_menu_identity_migration_periphery.rs:95-95` `Connection::open`
- `tests/store_append_order_periphery.rs:58-58` `Connection::open`
- `tests/watchdog_cli_periphery.rs:185-185` `Connection::open`

#### `dup-554a83a84ac8` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:7373-7414` `the_cross_file_inferred_tier_is_order_independent`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:7417-7432` `the_definition_upgrade_never_demotes_a_same_file_extracted_reference`

#### `dup-941c45a14b87` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs, src/main.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:7546-7548` `edge_desc`
- `src/main.rs:8605-8611` `runs_menu_line`

#### `dup-233d5e6363a7` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:7859-7890` `an_unresolvable_test_reference_is_staged_and_reconciled_once_its_definition_later_folds`
- `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:8135-8166` `the_empty_boundary_sentinel_never_resolves_records_or_stages_anything_for_its_empty_name`

#### `dup-a34b5b595e15` (near, 2 sites)

Proposed home: `events::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-grounder/src/grounder/design/events.rs:20-37` `concept_events`
- `crates/rigger-grounder/src/grounder/design/events.rs:45-62` `link_events`

#### `dup-28bd24c1f93d` (semantic, 3 sites)

Proposed home: `one shared `project_batches` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-grounder/src/grounder/design/events.rs:99-123` `project_batches`
- `crates/rigger-grounder/src/grounder/symbols/events.rs:56-58` `project_batches`
- `crates/rigger-grounder/src/grounder/workflowdef.rs:224-231` `project_batches`

#### `dup-7727a9cc5ba0` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-grounder/src/grounder/design/extract.rs, src/canary_store.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-grounder/src/grounder/design/extract.rs:157-159` `is_handbook_path`
- `src/canary_store.rs:822-824` `any_finding_is_critical`

#### `dup-4b942d7302a6` (near, 2 sites)

Proposed home: `model::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-grounder/src/grounder/design/model.rs:30-37` `node_kind`
- `crates/rigger-grounder/src/grounder/design/model.rs:81-89` `rel`

#### `dup-c1d968c10781` (semantic, 2 sites)

Proposed home: `one shared `extract_events` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-grounder/src/grounder/symbols/events.rs:171-267` `extract_events`
- `crates/rigger-grounder/src/grounder/workflowdef.rs:173-203` `extract_events`

#### `dup-207b5aa153cb` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-grounder/src/grounder/symbols/events.rs, crates/rigger-store-sqlite/src/eventstore/sqlite.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-grounder/src/grounder/symbols/events.rs:739-750` `kind_str`
- `crates/rigger-grounder/src/grounder/symbols/events.rs:754-763` `lang_str`
- `crates/rigger-store-sqlite/src/eventstore/sqlite.rs:1022-1027` `direction_sql`

#### `dup-9b727c38b8a1` (semantic, 3 sites)

Proposed home: `crates/rigger-grounder/src/grounder/symbols/extract.rs::extract as the ONE function that touches source parsing (already its own module doc's claim, architecture 5.5.3) - this file's own scan_file/tokenize are ad hoc scanners for the identical job and should route through an injected-grammar extractor rather than re-deriving structure by hand`

mandatory sweep: bespoke source-text lexer/scanner functions duplicating the canonical tree-sitter extractor - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-grounder/src/grounder/symbols/extract.rs:34-191` `extract`
- `tests/simplification_audit.rs:213-215` `scan_file`
- `tests/simplification_audit.rs:2468-2574` `tokenize`

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

- `crates/rigger-grounder/src/grounder/symbols/grounder.rs:1081-1092` `a_reference_ranks_below_a_definition_of_the_same_name`
- `crates/rigger-grounder/src/grounder/symbols/grounder.rs:1331-1345` `ground_ranks_an_exact_name_match_above_a_name_that_merely_contains_the_token`

#### `dup-56485f8d4262` (near, 3 sites)

Proposed home: `grounder::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-grounder/src/grounder/symbols/grounder.rs:1248-1294` `a_concurrent_reindex_from_a_second_grounder_is_not_clobbered`
- `crates/rigger-grounder/src/grounder/symbols/grounder.rs:1297-1323` `reindex_replaces_only_a_changed_files_symbols`
- `crates/rigger-grounder/src/grounder/symbols/grounder.rs:1622-1649` `reindex_over_a_deleted_file_stops_grounding_it`

#### `dup-415d2df07b98` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-grounder/src/grounder/symbols/store.rs, crates/rigger-grounder/src/grounder/workflowdef.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-grounder/src/grounder/symbols/store.rs:253-257` `load_is_none_on_a_cold_start`
- `crates/rigger-grounder/src/grounder/workflowdef.rs:553-557` `project_events_on_a_missing_workflow_yields_nothing_never_a_crash`

#### `dup-24db461612c2` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-grounder/src/grounder/workflowdef.rs, src/dash.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-grounder/src/grounder/workflowdef.rs:128-133` `light_reviewers_of`
- `src/dash.rs:8789-8791` `ids`

#### `dup-03666ae1d685` (semantic, 2 sites)

Proposed home: `one shared `project_events` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-grounder/src/grounder/workflowdef.rs:211-217` `project_events`
- `tests/common/mod.rs:444-449` `project_events`

#### `dup-6bdc4cb72e31` (near, 2 sites)

Proposed home: `ingest::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-grounder/src/ingest.rs:288-290` `graph_index_lag`
- `crates/rigger-grounder/src/ingest.rs:338-340` `graph_index_lag_sample`

#### `dup-af816213608d` (semantic, 48 sites)

Proposed home: `crates/rigger-process/src/reap.rs as the one /proc-reading module (dash.rs's own /proc readers already duplicate reap.rs's field-after-the-comm's-closing-paren /proc/<pid>/stat parse - see the report's worked example)`

mandatory sweep: /proc-path string literals - 48 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-process/src/reap.rs:133-133` `"/proc"`
- `crates/rigger-process/src/reap.rs:215-215` `"/proc/{pid}/stat"`
- `crates/rigger-process/src/reap.rs:234-234` `"/proc/{pid}/status"`
- `crates/rigger-process/src/reap.rs:294-294` `"/proc"`
- `crates/rigger-process/src/reap.rs:371-371` `"/proc/{}/cwd"`
- `src/dash.rs:560-560` `"/proc/net/tcp"`
- `src/dash.rs:591-591` `"/proc"`
- `src/dash.rs:8321-8321` `"/proc"`
- `src/dash.rs:8329-8329` `"the /proc scan must find THIS process as the holder of its own listener"`
- `src/dash.rs:8336-8336` `"/proc"`
- `src/dash.rs:8360-8360` `"/proc"`
- `src/dash.rs:8402-8402` `"/proc"`
- `src/main.rs:15109-15109` `"/proc"`
- `src/main.rs:15213-15213` `"/proc"`
- `tests/cli.rs:20036-20036` `"/proc"`
- `tests/cli.rs:24246-24246` `"/proc"`
- `tests/cli.rs:24354-24354` `"the holder pid {holder_pid} never reached the STOPPED (T) state in /proc"`
- `tests/cli.rs:24391-24391` `"/proc"`
- `tests/cli.rs:24442-24442` `"/proc"`
- `tests/cli.rs:24465-24465` `"/proc"`
- `tests/cli.rs:24490-24490` `"/proc"`
- `tests/cli.rs:24554-24554` `"/proc"`
- `tests/cli.rs:24580-24580` `"/proc"`
- `tests/cli.rs:24678-24678` `"/proc"`
- `tests/cli.rs:24716-24716` `"held_port_holder's message half and describe_held_port_if_confirmed's own return \
             must agree (scheduler-state letter normalized away since each call independently \
             re-reads /proc and can observe a state flap) - they are documented as sharing one \
             discovery"`
- `tests/cli.rs:24735-24735` `"/proc"`
- `tests/common/fixtures/host.rs:45-45` `"/proc/{pid}/stat has a pgrp field after comm"`
- `tests/duplication_catalog_contract_periphery.rs:101-101` `"/proc-path string literals"`
- `tests/simplification_audit.rs:3103-3103` `"/proc/<pid>/stat or /proc/<pid>/status field-extraction functions"`
- `tests/simplification_audit.rs:3109-3109` `"/proc-path string literals"`
- `tests/simplification_audit.rs:3284-3284` `"/proc"`
- `tests/simplification_audit.rs:3285-3285` `"crates/rigger-process/src/reap.rs as the one /proc-reading module (dash.rs's own /proc readers already \
             duplicate reap.rs's field-after-the-comm's-closing-paren /proc/<pid>/stat parse - \
             see the report's worked example)"`
- `tests/simplification_audit.rs:3325-3325` `"/proc"`
- `tests/simplification_audit.rs:3550-3550` `"crates/rigger-process/src/reap.rs as the one /proc/<pid>/stat and /proc/<pid>/status parser, returning \
             whichever field each caller needs, so dash.rs::process_state and \
             reap.rs::pid_starttime/read_ppid stop each re-deriving the pid(comm)state... split"`
- `tests/simplification_audit.rs:3969-3969` `"Two real recall gaps surfaced this way and were closed by widening the mechanical \
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
- `tests/simplification_audit.rs:4408-4408` `"A second mutation authority for one domain: the one previously-known \
        instance in this codebase (`src/dash.rs` reimplementing `crates/rigger-process/src/reap.rs`'s \
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
- `tests/simplification_audit.rs:5425-5425` `"Largest risk-reduction first is read as six tiers, ranked by the KIND of risk \
        each entry retires, highest first:\n\n\
        1. Tier 1 - active correctness risk, PLUS item 0: a use case already depends on the \
        wrong concretion, or two independent implementations of one concern can already \
        drift apart silently (section 3's two boundary violations; the one already-drifted \
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
- `tests/simplification_audit.rs:5539-5539` `"#### 3. Retire the duplicate `/proc`-reading authority (`{}` + `{}`)\n\n"`
- `tests/simplification_audit.rs:5543-5543` `"- Scope: the production half is done - `src/dash.rs::process_state` and \
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
- `tests/simplification_audit.rs:8689-8689` `"fn state_of(pid: u32) -> Option<char> {\n    let stat = std::fs::read_to_string(format!(\"/proc/{pid}/stat\")).ok()?;\n    stat.rsplit_once(')')?.1.split_whitespace().next()?.chars().next()\n}\n"`
- `tests/simplification_audit.rs:8693-8693` `"fn starttime_of(pid: u32) -> Option<u64> {\n    let stat = std::fs::read_to_string(format!(\"/proc/{pid}/stat\")).ok()?;\n    stat.rsplit_once(')')?.1.split_whitespace().nth(19)?.parse().ok()\n}\n"`
- `tests/simplification_audit.rs:8697-8697` `"fn ppid_of(pid: u32) -> Option<u32> {\n    let stat = std::fs::read_to_string(format!(\"/proc/{pid}/stat\")).ok()?;\n    stat.rsplit_once(')')?.1.split_whitespace().nth(1)?.parse().ok()\n}\n"`
- `tests/simplification_audit.rs:8852-8852` `"fn a() {\n    let _ = std::fs::read_to_string(\"/proc/1/stat\");\n    let _ = \"hello\";\n}\n"`
- `tests/simplification_audit.rs:8853-8853` `"/proc"`
- `tests/simplification_audit.rs:8855-8855` `"/proc"`
- `tests/simplification_audit.rs:8920-8920` `"fn state_of(pid: u32) -> Option<char> {\n    let s = std::fs::read_to_string(format!(\"/proc/{pid}/stat\")).ok()?;\n    s.chars().next()\n}\nfn ppid_of(pid: u32) -> Option<u32> {\n    let s = std::fs::read_to_string(format!(\"/proc/{pid}/status\")).ok()?;\n    s.parse().ok()\n}\nfn unrelated() -> u32 {\n    1\n}\n"`
- `tests/simplification_audit.rs:8942-8942` `"the /proc reader sweep is catalogued"`
- `tests/simplification_audit.rs:8948-8948` `"the /proc reader sweep {:?} must carry the one stat parser and neither former \
             re-deriver, found: {names:?}"`

#### `dup-202133ba5e92` (semantic, 9 sites)

Proposed home: `crates/rigger-process/src/reap.rs as the one /proc/<pid>/stat and /proc/<pid>/status parser, returning whichever field each caller needs, so dash.rs::process_state and reap.rs::pid_starttime/read_ppid stop each re-deriving the pid(comm)state... split`

mandatory sweep: /proc/<pid>/stat or /proc/<pid>/status field-extraction functions - 9 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `crates/rigger-process/src/reap.rs:214-218` `stat_field_after_comm`
- `crates/rigger-process/src/reap.rs:233-239` `read_ppid`
- `tests/common/fixtures/host.rs:43-48` `pgid_of`
- `tests/simplification_audit.rs:3274-3305` `build_sweep_clusters`
- `tests/simplification_audit.rs:3545-3566` `build_extra_semantic_clusters`
- `tests/simplification_audit.rs:3896-4013` `render_adversarial_sample`
- `tests/simplification_audit.rs:5381-5952` `render_section_6`
- `tests/simplification_audit.rs:8682-8705` `three_near_but_not_identical_functions_cluster_as_near_with_every_site_and_one_home`
- `tests/simplification_audit.rs:8851-8856` `proc_literal_sweep_finds_a_proc_path_string_and_ignores_an_unrelated_one`

#### `dup-abc5b2cd47c2` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-process/src/reap.rs, src/main.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-process/src/reap.rs:854-861` `processes_rooted_under_is_a_graceful_no_op_when_the_base_is_absent`
- `src/main.rs:19190-19197` `leaked_process_advisories_is_a_graceful_no_op_when_the_scratch_root_is_absent`

#### `dup-183455104b30` (near, 3 sites)

Proposed home: `contract::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-store-sqlite/src/eventstore/contract.rs:172-192` `append_assigns_revisions`
- `crates/rigger-store-sqlite/src/eventstore/contract.rs:301-346` `backward_stream_read_reverses_set`
- `crates/rigger-store-sqlite/src/eventstore/contract.rs:350-375` `forward_stream_read_honors_nonzero_from`

#### `dup-d32a420335cb` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: crates/rigger-store-sqlite/src/eventstore/mod.rs, src/console/map.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-store-sqlite/src/eventstore/mod.rs:239-252` `no_credential_fragment_ever_survives`
- `src/console/map.rs:1265-1279` `module_of_never_returns_a_string_carrying_a_dot_or_a_slash`

#### `dup-831910355599` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-store-sqlite/src/eventstore/sqlite.rs:1537-1562` `measure_derived_duplication_on_a_clean_log_reports_no_duplication`
- `crates/rigger-store-sqlite/src/eventstore/sqlite.rs:1565-1616` `measure_derived_duplication_treats_the_same_key_under_two_covered_types_as_two_distinct_subjects`

#### `dup-09fd5c38b65b` (near, 2 sites)

Proposed home: `progress_store::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-store-sqlite/src/progress_store.rs:39-53` `record_launch`
- `crates/rigger-store-sqlite/src/progress_store.rs:59-70` `record_stop_failure`

#### `dup-10504155864a` (near, 2 sites)

Proposed home: `run_store::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-store-sqlite/src/run_store.rs:380-394` `ensure_started_adopts_the_same_criteria_run_without_re_minting`
- `crates/rigger-store-sqlite/src/run_store.rs:449-464` `ensure_started_mints_a_new_run_when_the_criteria_change`

#### `dup-90067fabd6a8` (near, 2 sites)

Proposed home: `worktree::worktree`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-worktree-git/src/worktree.rs:632-634` `commit`
- `crates/rigger-worktree-git/src/worktree.rs:650-652` `commit_checkpoint`

#### `dup-4efef9d45e3e` (exact, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-worktree-git/src/worktree.rs:2449-2457` `scratch_root`
- `crates/rigger-worktree-git/src/worktree.rs:2548-2556` `scratch_root_path`

#### `dup-e08d5a3a5d94` (exact, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `crates/rigger-worktree-git/src/worktree.rs:2623-2626` `scratch_root_from_env`
- `crates/rigger-worktree-git/src/worktree.rs:2630-2633` `scratch_root_path_from_env`

#### `dup-7dfa0dd1a6a8` (near, 2 sites)

Proposed home: `blast_radius_eval::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/blast_radius_eval.rs:216-231` `width_threshold_is_the_tier_width_nearest_rank_percentile`
- `src/blast_radius_eval.rs:234-242` `full_fraction_spans_all_light_to_collapse`

#### `dup-be97ce907814` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: src/console/map.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/console/map.rs:877-883` `as_candidate`
- `tests/simplification_audit.rs:1817-1826` `map_entry_wire`
- `tests/simplification_audit.rs:1828-1835` `map_entry_lines`

#### `dup-73fd1b3e0b03` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/console/map.rs, tests/files_lens_view_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/console/map.rs:1243-1245` `in_community`
- `tests/files_lens_view_periphery.rs:72-74` `refs`

#### `dup-0e618ec2d345` (near, 2 sites)

Proposed home: `map::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/console/map.rs:2101-2113` `caller_callee_graph`
- `src/console/map.rs:2441-2455` `cross_district_graph`

#### `dup-0f11f47627d7` (near, 3 sites)

Proposed home: `map::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/console/map.rs:2458-2465` `landmarks_ranks_by_whole_map_degree_descending`
- `src/console/map.rs:2562-2567` `search_is_case_insensitive`
- `src/console/map.rs:2576-2584` `search_hit_carries_kind_and_degree_beside_the_name`

#### `dup-4ccd568cae24` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:3933-3938` `console_core_wasm_route_serves_the_embedded_artifact_as_application_wasm`
- `tests/simplification_audit.rs:7402-7409` `a_cfg_test_attribute_directly_on_an_impl_block_is_flagged_test`
- `tests/simplification_audit.rs:7529-7535` `a_cfg_test_pub_fn_is_still_flagged_test_pub_survives_between_attribute_and_keyword`

#### `dup-3f680da704c9` (near, 6 sites)

Proposed home: `a new shared module (sites span 5 files: src/dash.rs, tests/checkin_mutation_diff_base_periphery.rs, tests/console_palette_periphery.rs, tests/projections_stay_local.rs, tests/store_resolution.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:4691-4726` `the_page_layout_cannot_scroll_the_body_horizontally`
- `src/dash.rs:4737-4774` `the_landing_view_lists_instances_and_threads_the_attach_selector`
- `tests/checkin_mutation_diff_base_periphery.rs:105-122` `the_shipped_mutation_gate_guards_on_rigger_run_base_never_a_merge_base`
- `tests/console_palette_periphery.rs:400-415` `the_served_console_page_sends_the_scrub_position_to_palette_commands_when_not_live`
- `tests/projections_stay_local.rs:82-100` `the_graph_and_progress_projections_open_via_the_local_sqlite_constructors`
- `tests/store_resolution.rs:98-114` `the_single_resolver_exists_and_the_old_per_command_helper_is_retired`

#### `dup-232528c40d46` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/ci_lanes.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:4858-4892` `cells_fit_or_wrap_and_wide_cells_scroll_in_their_own_container`
- `tests/ci_lanes.rs:301-317` `kurrentdb_job_carries_no_retired_feature_flag_and_still_runs_the_contract_test`

#### `dup-59fa698ee47b` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/common/fixtures/graph.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:8786-8788` `layer_of`
- `tests/common/fixtures/graph.rs:186-188` `call_layer`

#### `dup-4d77d0fdce5a` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/metadata_card_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:9397-9421` `an_absent_explain_leaves_the_graph_route_unchanged`
- `src/dash.rs:9659-9679` `a_cluster_drill_carries_no_memory_field`
- `tests/metadata_card_periphery.rs:349-366` `the_served_route_degrades_gracefully_for_an_unknown_card_subject`

#### `dup-20e69e434836` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/metadata_card_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:9705-9755` `card_graph`
- `tests/metadata_card_periphery.rs:49-100` `fixture_graph`

#### `dup-776fd9e153e2` (near, 4 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/metadata_card_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:10034-10076` `the_card_route_serves_a_known_subjects_card_and_null_for_an_unknown_one`
- `tests/metadata_card_periphery.rs:119-166` `the_served_route_carries_a_code_subjects_card`
- `tests/metadata_card_periphery.rs:174-200` `the_served_route_omits_community_and_line_keys_for_a_membership_less_entity`
- `tests/metadata_card_periphery.rs:373-399` `the_card_param_takes_precedence_over_a_stray_seed_or_lens_param`

#### `dup-6a2acacd5cf0` (near, 2 sites)

Proposed home: `docs::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/docs.rs:94-112` `render_using_rigger_skill`
- `src/docs.rs:116-127` `render_handbook_discipline`

#### `dup-bcc850364281` (near, 2 sites)

Proposed home: `docs::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/docs.rs:447-449` `render_planning_a_spec_skill`
- `src/docs.rs:618-620` `render_planning_field_guide`

#### `dup-fd7990caad07` (near, 4 sites)

Proposed home: `docs::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/docs.rs:714-742` `render_build_graph_skill`
- `src/docs.rs:747-774` `render_reindex_skill`
- `src/docs.rs:779-813` `render_resume_a_run_skill`
- `src/docs.rs:942-992` `render_restore_the_dash_skill`

#### `dup-3e586352db43` (near, 2 sites)

Proposed home: `docs::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/docs.rs:820-855` `render_handle_an_escalation_skill`
- `src/docs.rs:1001-1050` `render_diagnose_churn_skill`

#### `dup-ca8ccf1b9f66` (exact, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:921-923` `project_identity`
- `src/main.rs:13590-13592` `git_repo`

#### `dup-4841cf90306e` (exact, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:1677-1679` `usage`
- `src/main.rs:11949-11956` `print_scaffold_pointer`

#### `dup-e110574b66dc` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/main.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:1790-1792` `find_store_dir_from`
- `tests/simplification_audit.rs:997-999` `scan_target_files`

#### `dup-10bf3f3faab0` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:4775-4799` `cmd_graph_communities`
- `src/main.rs:4873-4897` `cmd_graph_concepts`

#### `dup-3dfc1455ab14` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/main.rs, tests/reset_build_cache_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:11097-11116` `dir_size_bytes`
- `tests/reset_build_cache_periphery.rs:55-72` `dir_bytes`

#### `dup-f5d0a37113e0` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:12988-12992` `install_lookup_hook`
- `src/main.rs:13042-13046` `install_status_line`

#### `dup-66f6002153cf` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: src/main.rs, tests/cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:14711-14718` `spec_lint_next_step_names_rigger_validate_and_the_given_spec_path`
- `tests/cli.rs:4336-4359` `native_driver_fixpoint_requires_done_and_an_empty_in_flight_set`
- `tests/cli.rs:4367-4380` `native_driver_drains_in_flight_workers_before_a_loud_stop`

#### `dup-39046faa7f9f` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:17312-17333` `git_is_ancestor_decides_commit_order_in_a_real_repo`
- `src/main.rs:17336-17357` `git_commit_distance_counts_commits_ahead_in_a_real_repo`

#### `dup-f43cc4afc6a2` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:19000-19055` `reclaim_orphan_scratch_spares_a_non_live_worktree_that_is_still_dirty`
- `src/main.rs:19058-19097` `reclaim_orphan_scratch_spares_only_a_declared_dirty_worktree`

#### `dup-8aae77675253` (exact, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:22916-22927` `the_step_schema_admits_the_attention_array`
- `src/main.rs:24917-24920` `no_runs_message_points_at_rigger_run`

#### `dup-3aa9191927e8` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/main.rs, tests/cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:23326-23335` `step_courier_prompt`
- `tests/cli.rs:3396-3405` `cmd_step_source`

#### `dup-b3f279a36f39` (exact, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:24485-24487` `order_signature_advisories_is_empty_when_no_signatures_are_given`
- `src/main.rs:27305-27307` `parse_guard_write_roots_requires_at_least_one`

#### `dup-2e3d71bbeb25` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:25185-25194` `result_of_at_unrecorded_spawn_reads_as_unreported`
- `src/main.rs:25232-25251` `result_of_at_is_namespace_scoped`

#### `dup-0214889fec22` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:25748-25768` `reset_modes_parses_force_live_alongside_derived_rejects_duplicates_and_never_implies_a_mode`
- `src/main.rs:25800-25821` `reset_modes_parses_build_cache_alone_and_composed_and_rejects_duplicates`

#### `dup-ad4ebfe4d855` (near, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:186-206` `integrate_lands_work_in_the_repo`
- `src/worktree.rs:2140-2163` `integrate_lands_a_pre_committed_artifact_unchanged`

#### `dup-5f516fce9f0b` (near, 2 sites)

Proposed home: `adoption_keys_on_criterion_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/adoption_keys_on_criterion_periphery.rs:987-1063` `a_compensation_reverted_integration_reopens_adoption_of_its_real_still_existing_branch`
- `tests/adoption_keys_on_criterion_periphery.rs:1071-1133` `a_plain_remediation_failure_after_integration_never_reopens_adoption_even_though_a_same_named_branch_exists`

#### `dup-5dc33b6ca236` (near, 2 sites)

Proposed home: `build_env_authority_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/build_env_authority_periphery.rs:411-456` `one_build_environment_authority_reaches_a_real_gate_subprocess_and_a_real_agent_subprocess`
- `tests/build_env_authority_periphery.rs:581-614` `auto_wrapper_resolves_to_a_real_probed_binary_and_reaches_both_real_subprocesses`

#### `dup-58e746ca5c35` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/canary_false_positives_periphery.rs, tests/canary_unattributed_rejects_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/canary_false_positives_periphery.rs:61-126` `spawn`
- `tests/canary_unattributed_rejects_periphery.rs:60-113` `spawn`

#### `dup-d887cb6f63f7` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/canary_false_positives_periphery.rs, tests/canary_unattributed_rejects_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/canary_false_positives_periphery.rs:136-263` `run_canary_scores_false_positive_controls_and_project_canary_counts_them`
- `tests/canary_unattributed_rejects_periphery.rs:123-256` `run_canary_scores_an_unattributed_correct_reject_and_project_canary_counts_only_it`

#### `dup-71aadfa4568e` (near, 14 sites)

Proposed home: `a new shared module (sites span 3 files: tests/cause_wire_periphery.rs, tests/cli.rs, tests/escalation_resume_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cause_wire_periphery.rs:109-130` `a_legacy_causeless_unit_failed_event_survives_a_real_store_round_trip_and_renders_unknown`
- `tests/cause_wire_periphery.rs:137-168` `the_reject_recurrence_line_names_the_latest_of_several_recorded_causes_through_the_real_binary`
- `tests/cli.rs:9512-9549` `stats_reports_the_latest_run_by_default_and_all_for_the_aggregate`
- `tests/cli.rs:9804-9818` `stats_cli_reports_no_recorded_spawns_when_the_run_has_none`
- `tests/cli.rs:19257-19315` `release_ready_handoff_surfaces_on_status_for_a_done_run`
- `tests/cli.rs:19523-19563` `release_ready_is_silent_on_status_for_an_unfinished_run`
- `tests/cli.rs:19633-19665` `release_ready_pluralizes_the_unit_count_on_status_for_a_multi_unit_run`
- `tests/cli.rs:19709-19730` `release_ready_is_silent_on_status_for_a_spec_defective_run`
- `tests/escalation_resume_periphery.rs:85-108` `a_unit_resumed_event_seeded_directly_through_a_real_store_reaches_status_without_the_command`
- `tests/escalation_resume_periphery.rs:119-147` `a_legacy_shaped_unit_resumed_event_missing_both_optional_fields_survives_a_real_store_round_trip`
- `tests/escalation_resume_periphery.rs:235-265` `the_resumed_banner_survives_a_genuinely_in_flight_re_parked_attempt_not_yet_resolved`
- `tests/escalation_resume_periphery.rs:313-333` `resume_unit_rejects_an_unknown_unit_absent_from_the_run`
- `tests/escalation_resume_periphery.rs:340-363` `resume_unit_refuses_when_the_recorded_branch_was_never_created`
- `tests/escalation_resume_periphery.rs:370-392` `resume_unit_refuses_an_already_integrated_unit`

#### `dup-6193e0f04ade` (near, 34 sites)

Proposed home: `a new shared module (sites span 6 files: tests/cli.rs, tests/reset_build_cache_periphery.rs, tests/reset_derived_live_writer_guard_periphery.rs, tests/reset_menu.rs, tests/statusline_command_periphery.rs, tests/watchdog_cli_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:211-233` `emit_review_finding_shows_in_peers`
- `tests/cli.rs:1240-1257` `scratch_requires_exactly_one_spawn_id`
- `tests/cli.rs:1368-1379` `scratch_refuses_a_degenerate_empty_spawn_id`
- `tests/cli.rs:1611-1628` `result_prints_an_orphan_advisory_for_an_unrecorded_id`
- `tests/cli.rs:1633-1653` `result_prints_a_supersede_advisory_when_a_result_already_exists`
- `tests/cli.rs:2528-2540` `graph_build_exits_clean_and_creates_the_store_in_both_lanes`
- `tests/cli.rs:3306-3333` `emit_rejects_bad_json_with_a_nonzero_exit`
- `tests/cli.rs:5202-5223` `resume_unit_defaults_to_granting_one_attempt_when_attempts_is_omitted`
- `tests/cli.rs:5226-5240` `resume_unit_refuses_an_unknown_unit`
- `tests/cli.rs:10379-10389` `run_accepts_a_base_flag`
- `tests/cli.rs:13317-13348` `init_reports_the_positive_summary_then_is_a_quiet_noop`
- `tests/cli.rs:13793-13813` `result_if_absent_records_when_the_spawn_is_unanswered`
- `tests/cli.rs:14346-14355` `stats_canary_on_a_project_with_no_canary_run_says_so`
- `tests/cli.rs:14511-14540` `canary_rejects_unknown_arguments_and_a_missing_corpus`
- `tests/cli.rs:14569-14583` `canary_accepts_a_jobs_flag_alongside_other_flags`
- `tests/cli.rs:14721-14756` `canary_rejects_a_malformed_or_unknown_tier_model_pin`
- `tests/cli.rs:16329-16341` `status_json_appends_no_dashboard_entry_when_none_was_ever_recorded`
- `tests/cli.rs:16451-16462` `status_prints_no_dashboard_line_when_none_was_ever_recorded`
- `tests/cli.rs:23576-23586` `prime_with_no_spec_path_never_mentions_the_spec_lint`
- `tests/cli.rs:23589-23606` `prime_given_a_spec_path_names_the_spec_lint_as_a_next_step`
- `tests/cli.rs:23609-23631` `prime_given_a_spec_path_names_the_spec_lint_alongside_recent_decisions`
- `tests/cli.rs:23997-24006` `workflow_with_no_spec_path_never_mentions_the_spec_lint`
- `tests/cli.rs:25240-25272` `mcp_rejects_a_malformed_spawn_flag_or_unexpected_arguments`
- `tests/cli.rs:26765-26785` `init_scaffolds_the_instructions_readme_and_names_it`
- `tests/reset_build_cache_periphery.rs:103-129` `reset_build_cache_is_idempotent_zero_report_on_a_project_that_never_built_anything`
- `tests/reset_build_cache_periphery.rs:167-177` `reset_build_cache_flag_is_registered_and_rejects_a_duplicate`
- `tests/reset_derived_live_writer_guard_periphery.rs:566-576` `reset_force_live_alone_is_refused_as_no_mode`
- `tests/reset_derived_live_writer_guard_periphery.rs:582-597` `the_derived_help_entry_documents_force_live_and_owns_the_risk`
- `tests/reset_menu.rs:79-96` `bare_reset_on_an_empty_store_exits_zero_and_reports_nothing_prunable`
- `tests/statusline_command_periphery.rs:90-100` `status_line_on_a_clean_run`
- `tests/statusline_command_periphery.rs:137-147` `status_line_and_json_are_mutually_exclusive`
- `tests/watchdog_cli_periphery.rs:149-159` `watch_once_on_a_freshly_initialized_store_reports_nothing_and_exits_cleanly`
- `tests/watchdog_cli_periphery.rs:248-262` `watch_refuses_a_project_with_no_rigger_store_at_all_through_the_real_binary`
- `tests/watchdog_cli_periphery.rs:269-282` `watch_rejects_an_unknown_flag_through_the_real_binary_with_a_nonzero_exit`

#### `dup-4cd3708b516f` (exact, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:1683-1685` `initialized_project`
- `tests/cli.rs:1704-1706` `initialized_git_project`

#### `dup-9da3f693c2b7` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/cli.rs, tests/worker_persona_label_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:4087-4143` `native_driver_scratch_policy_directs_the_worker_to_its_own_spawn_owned_container`
- `tests/worker_persona_label_periphery.rs:307-328` `workerlabel_is_actually_wired_into_runworkers_agent_call_label`

#### `dup-751f1b83af79` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: tests/cli.rs, tests/step_attention_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:7566-7599` `a_budget_halt_does_not_restamp_on_a_later_real_step_with_nothing_new`
- `tests/cli.rs:8111-8154` `step_attention_never_restamps_a_hung_unbounded_spawn_when_repo_less`
- `tests/step_attention_periphery.rs:175-264` `recurrence_and_stalled_frontier_survive_real_process_boundaries`

#### `dup-ea8a80f3da16` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/cli.rs, tests/courier_registry_refresh_boundary_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:8646-8649` `run_teardown_spares_run_level_scratch_at_a_manual_review_pause`
- `tests/courier_registry_refresh_boundary_periphery.rs:194-197` `an_ambient_kurrentdb_conn_never_leaks_into_a_boundary_courier`

#### `dup-edb646ecd5e8` (exact, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:11825-11827` `path_with_fake_sccache`
- `tests/cli.rs:18168-18170` `stage_rigger_shim`

#### `dup-84fea880d212` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:16739-16769` `docs_ships_three_verb_lookup_guidance_to_consumers`
- `tests/cli.rs:26565-26612` `docs_installs_the_operator_lookup_rule_text_into_the_shipped_skill_and_handbook`

#### `dup-21b08556e493` (exact, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:18591-18596` `stage_failing_docs_rigger_shim`
- `tests/cli.rs:18616-18633` `stage_stale_rigger_shim`

#### `dup-20d63cdc7260` (semantic, 2 sites)

Proposed home: `cli::mcp_session - one canonical constructor, the rest thin variants over it (or a builder), rather than each re-listing every field`

mandatory sweep: parallel constructor functions - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/cli.rs:24878-24880` `start`
- `tests/cli.rs:24883-24902` `start_with`

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

- `tests/common/cli.rs:28-32` `courier_project`
- `tests/common/cli.rs:119-123` `temp_rigger_project`
- `tests/common/cli.rs:127-131` `temp_store_project`
- `tests/courier_registry_refresh_boundary_periphery.rs:40-44` `courier_project_with_commit`

#### `dup-8dd44d643a5c` (semantic, 2 sites)

Proposed home: `one shared `write_workflow` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/common/cli.rs:382-389` `write_workflow`
- `tests/common/workflow_probe.rs:21-23` `write_workflow`

#### `dup-b58b0ff72b22` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: tests/common/cli.rs, tests/store_secrets.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/common/cli.rs:418-438` `assert_selected_server`
- `tests/common/cli.rs:442-458` `assert_selected_sqlite`
- `tests/store_secrets.rs:69-107` `assert_server_reached_and_credentials_redacted`

#### `dup-c3c8358993cf` (near, 2 sites)

Proposed home: `fold::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/common/fixtures/fold.rs:173-175` `apply_ref`
- `tests/common/fixtures/fold.rs:180-182` `apply_call`

#### `dup-9c3b5c80063e` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/common/fixtures/fold.rs, tests/community_resolution_knob.rs, tests/graph_superseded_prune.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/common/fixtures/fold.rs:264-273` `live_node_ids`
- `tests/community_resolution_knob.rs:137-146` `live_memberships_of`
- `tests/graph_superseded_prune.rs:32-41` `live_contains`

#### `dup-088ec120e4ac` (near, 2 sites)

Proposed home: `host::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/common/fixtures/host.rs:53-60` `sigterm_ignorer_in`
- `tests/common/fixtures/host.rs:64-70` `sleeper_in`

#### `dup-c99581d3e41e` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/common/workflow_probe.rs, tests/store_precedence.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/common/workflow_probe.rs:21-23` `write_workflow`
- `tests/store_precedence.rs:47-49` `write_store_conn`

#### `dup-b02e5a0dd9cd` (semantic, 2 sites)

Proposed home: `one shared `seed_coupling` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/community_detection_cli.rs:93-130` `seed_coupling`
- `tests/community_fold_periphery.rs:39-49` `seed_coupling`

#### `dup-38bcd5cba1f7` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_fold_periphery.rs, tests/concepts_fold_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_fold_periphery.rs:169-202` `a_community_assigned_event_without_a_fresh_key_is_a_non_boundary_and_never_supersedes`
- `tests/concepts_fold_periphery.rs:188-232` `a_concept_derived_event_without_a_fresh_key_is_a_non_boundary_and_never_supersedes`

#### `dup-29092fe5ff01` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_fold_periphery.rs, tests/concepts_fold_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_fold_periphery.rs:205-239` `the_community_layer_serialized_literals_are_stable_and_the_fold_matches_the_on_log_type`
- `tests/concepts_fold_periphery.rs:235-276` `the_concept_layer_serialized_literals_are_stable_and_the_fold_matches_the_on_log_type`

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

- `tests/graph_show_periphery.rs:97-152` `graph_show_degrades_to_stale_note_when_location_drifted`
- `tests/graph_show_periphery.rs:162-204` `graph_show_degrades_to_stale_note_when_recorded_line_is_zero`
- `tests/graph_show_periphery.rs:234-268` `graph_show_light_lane_degrades_to_extent_unavailable_note`
- `tests/graph_show_periphery.rs:277-329` `graph_show_bounds_body_at_the_definitions_own_extent`
- `tests/graph_show_periphery.rs:341-416` `graph_show_shows_full_body_past_nested_definition`
- `tests/graph_show_periphery.rs:427-475` `graph_show_shows_full_body_of_a_destructuring_signature`
- `tests/graph_show_periphery.rs:483-525` `graph_show_extent_ignores_braces_in_strings_comments_and_chars`
- `tests/graph_show_periphery.rs:535-597` `graph_show_shows_full_body_of_a_python_nested_def`
- `tests/graph_show_periphery.rs:608-648` `graph_show_does_not_overread_a_js_single_quote_brace_body`
- `tests/graph_show_periphery.rs:719-772` `graph_show_degrades_when_no_grammar_registered_for_the_file_extension`
- `tests/graph_show_periphery.rs:786-845` `graph_show_heals_to_the_live_line_when_the_moved_name_is_unambiguous`
- `tests/graph_show_periphery.rs:854-906` `graph_show_degrades_when_the_moved_name_is_ambiguous_in_the_file`
- `tests/graph_show_staleness.rs:42-86` `graph_show_degrades_gracefully_when_the_recorded_file_is_missing`
- `tests/graph_show_staleness.rs:103-181` `graph_show_never_presents_a_neighbours_body_when_the_line_drifted`

#### `dup-8743cc5db2b4` (near, 2 sites)

Proposed home: `integrate_conflict_merge_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/integrate_conflict_merge_periphery.rs:494-559` `spawn`
- `tests/integrate_conflict_merge_periphery.rs:1267-1312` `spawn`

#### `dup-333ec555460f` (near, 2 sites)

Proposed home: `integrate_conflict_merge_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/integrate_conflict_merge_periphery.rs:1874-1938` `a_crash_right_after_the_merge_attempt_record_resumes_and_completes_row_1`
- `tests/integrate_conflict_merge_periphery.rs:1942-2006` `a_crash_right_after_the_landing_intent_record_resumes_and_completes_row_4`

#### `dup-d5a2e2020223` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/no_os_kill_audit.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_audit.rs:374-377` `never_flagged`
- `tests/simplification_audit.rs:9087-9093` `assert_no_same_named_helper_cluster`

#### `dup-b39b9c66964a` (semantic, 2 sites)

Proposed home: `one shared `start_kurrentdb` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/projections_stay_local.rs:119-149` `start_kurrentdb`
- `tests/store_resolution.rs:132-163` `start_kurrentdb`

#### `dup-398370d93ff5` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/projections_stay_local.rs, tests/store_resolution.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/projections_stay_local.rs:212-289` `progress_against_the_server_keeps_progress_db_local_and_the_log_on_the_server`
- `tests/store_resolution.rs:179-254` `a_courier_in_a_project_configured_for_the_server_resolves_the_server_store`

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

- `tests/reset_derived_compaction.rs:72-98` `rows`
- `tests/reset_derived_compaction_periphery.rs:112-135` `raw_rows`

#### `dup-6adbc1efd3a6` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/review_round_lenses_only_log_derived_resume_periphery.rs, tests/review_round_non_ancestor_residue_names_true_diff_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/review_round_lenses_only_log_derived_resume_periphery.rs:79-103` `spawn`
- `tests/review_round_non_ancestor_residue_names_true_diff_periphery.rs:71-101` `spawn`

#### `dup-e0f2896fa7de` (exact, 3 sites)

Proposed home: `simplification_audit::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:3704-3707` `real_files`
- `tests/simplification_audit.rs:5228-5231` `real_map`
- `tests/simplification_audit.rs:7207-7210` `real_workspace_files`

#### `dup-23cb89af01e7` (near, 4 sites)

Proposed home: `simplification_audit::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:3762-3779` `dup_cluster_wire`
- `tests/simplification_audit.rs:3781-3794` `dup_cluster_lines`
- `tests/simplification_audit.rs:7160-7177` `dead_code_candidate_wire`
- `tests/simplification_audit.rs:7179-7194` `dead_code_candidate_lines`

#### `dup-adfce3b42b86` (near, 2 sites)

Proposed home: `step_sheds_the_freshen::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/step_sheds_the_freshen.rs:128-163` `fingerprint_subtree`
- `tests/step_sheds_the_freshen.rs:129-159` `walk`

#### `dup-ece604793e18` (semantic, 5 sites)

Proposed home: `store_content_identity_periphery::port_double - one canonical constructor, the rest thin variants over it (or a builder), rather than each re-listing every field`

mandatory sweep: parallel constructor functions - 5 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/store_content_identity_periphery.rs:124-126` `new`
- `tests/store_content_identity_periphery.rs:130-138` `built`
- `tests/store_content_identity_periphery.rs:143-145` `miscounting`
- `tests/store_content_identity_periphery.rs:149-151` `over_an_empty_stream`
- `tests/store_content_identity_periphery.rs:155-157` `over_a_stream`

#### `dup-56fda398784b` (semantic, 2 sites)

Proposed home: `one shared `write_store_conn` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/store_precedence.rs:47-49` `write_store_conn`
- `tests/store_secrets.rs:54-63` `write_store_conn`

#### `dup-5f1cbfb1ea59` (near, 3 sites)

Proposed home: `store_resolution_cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/store_resolution_cli.rs:55-89` `a_server_selected_courier_reaches_the_server_and_never_fabricates_local_sqlite`
- `tests/store_resolution_cli.rs:92-119` `a_courier_with_no_server_configured_resolves_the_local_sqlite_log`
- `tests/store_resolution_cli.rs:122-146` `an_empty_kurrentdb_conn_is_treated_as_unset_not_a_server_with_no_address`

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

- `tests/workflow_definition_and_js_constants_periphery.rs:88-121` `seed_workflow_yml`
- `tests/workflow_definition_and_js_constants_periphery.rs:129-144` `seed_js_files`

### Adversarial sample

Recall check (spec 85 THOROUGHNESS): 30 functions drawn by seeded rank (seed `85072026`, `sample_indices` over all 6845 functions scanned in `src/` and `tests/`, each ranked by the seeded hash of its own file, name and ordinal so a change elsewhere never reshuffles a drawn row, excluding `tests/prioritized_plan_citation_periphery.rs` - criterion 4's own citation-guard periphery test, whose function count grows as its citation-drift-guard mechanism hardens round over round; excluding it keeps that unrelated growth out of the draw), each read by hand - together with its host file's surrounding context, since a duplicate can live anywhere in the file or a sibling file - to judge whether a duplicate exists that the mechanical pass and the five sweeps above did not already catch.

- `crates/rigger-conductor/src/conductor.rs:13753-13775` `branch_is_foreign_when_only_one_axis_differs` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `crates/rigger-conductor/src/conductor.rs:17726-17779` `grounding_still_surfaces_a_prior_run_decision_that_peers_labels_historical` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `crates/rigger-conductor/src/conductor.rs:23186-23220` `route_review_tier_fails_safe_to_full_on_an_empty_blast_radius` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `crates/rigger-conductor/src/conductor.rs:31680-31710` `a_budget_refused_standalone_review_spawn_keeps_its_worktree` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `crates/rigger-config-files/src/config_store.rs:2377-2398` `validate_rejects_a_named_wrapper_with_an_uncreatable_cache_dir` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `crates/rigger-domain/src/canary.rs:285-319` `latest_run_scopes_to_the_last_batch_marker` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `crates/rigger-domain/src/config.rs:375-377` `is_empty` - caught: `dup-1f030cd524f7`
- `crates/rigger-domain/src/contextgraph/query.rs:2056-2060` `assert_sole_member_is_w` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `crates/rigger-domain/src/metrics.rs:1417-1424` `model_id_base` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `crates/rigger-domain/src/spec.rs:1953-1960` `strip_inline_code_direct_exact_output_pins_a_zero_width_quote_pair` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `crates/rigger-grounder/src/grounder/symbols/events.rs:535-547` `normalize_logical_path` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `crates/rigger-grounder/src/ingest.rs:48-88` `append_and_fold_batch` - caught: `dup-7a935e08967c`
- `crates/rigger-store-sqlite/src/eventstore/kurrentdb.rs:333-352` `placement_of_ack` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `crates/rigger-worktree-git/src/worktree.rs:1720-1744` `reclaim_cache_sibling` - NOT READ - drawn after the 2026-09-27 reading pass; read it and record its verdict in `ADVERSARIAL_SAMPLE_VERDICTS`
- `src/canary_store.rs:822-824` `any_finding_is_critical` - caught: `dup-7727a9cc5ba0`
- `src/canary_store.rs:1761-1777` `score_item_reports_no_resolved_model_for_a_tier_whose_driver_leaves_it_empty` - no duplicate found by reading
- `src/dash.rs:5575-5622` `export_neutralizes_a_script_breakout_in_the_inlined_state` - no duplicate found by reading
- `src/main.rs:8605-8611` `runs_menu_line` - caught: `dup-941c45a14b87`
- `src/main.rs:9733-9748` `reclaim_spawn_scratch` - no duplicate found by reading
- `src/main.rs:10487-10521` `workflow_drift_advisory` - no duplicate found by reading
- `tests/cli.rs:10348-10372` `workflow_accepts_a_spec_and_a_base_flag` - no duplicate found by reading
- `tests/cli.rs:18712-18795` `setup_precommit_hook_prefers_a_unit_derived_binary_in_a_real_linked_worktree_over_a_stale_path_rigger` - duplicate found by reading and closed: it re-rolled `fresh_committed_skill`, `committed_skill` and `commit_a_code_change` inline; it now calls them, with the commit half split out as `commit_staged`
- `tests/cli.rs:25791-25808` `guard_write_exits_the_blocking_code_on_every_transport_failure` - duplicate found by reading and closed: it and `guard_write_without_a_root_fails_loudly_rather_than_allowing_everything` re-rolled `run_hook_verb`'s piped-stdin spawn; all three now call `pipe_into_rigger`, the blocking checks through `assert_blocks`
- `tests/common/audit_record.rs:8-13` `read_audit_record` - duplicate found by reading and closed: `tests/gitsemver_path_inclusion_accounting_periphery.rs` re-rolled it to read the stage1 record; it now includes and calls it
- `tests/compiler_pass_stage1_audit.rs:50-104` `stage1_record_has_the_shape_every_consumer_relies_on` - no duplicate found by reading
- `tests/concepts_labels_membership.rs:245-272` `label_of_the_documentless_hub` - no duplicate found by reading
- `tests/reset_derived_compaction_periphery.rs:3681-3685` `a_prune_with_nothing_to_reclaim_leaves_the_file_unrewritten` - duplicate found by reading and closed: `the_rewrite_flag_follows_the_file_and_not_this_passs_delete_count` repeated its settled-file fixture and skipped-rewrite assertions; both now call `settled_clean_store` and `assert_prune_skips_the_rewrite`
- `tests/simplification_audit.rs:7439-7443` `a_bare_test_attribute_on_a_free_function_marks_it_test_without_a_cfg_test_mod` - duplicate found by reading and closed: it and five sibling scanner tests re-rolled `scan_single`; all now call it or its name and span assertions
- `tests/step_attention_periphery.rs:89-135` `hung_cursor_functions_are_a_working_public_contract_across_the_crate_boundary` - no duplicate found by reading
- `tests/worker_persona_label_periphery.rs:222-243` `the_subject_is_the_titles_first_sentence_passed_whole_with_no_truncation` - duplicate found by reading and closed: it re-rolled `assert_worker_label` inline; it now calls it

Reading pass 2026-09-27: 16 of the 30 drawn functions read by hand (a caught row too, to judge whether its duplicate reaches past the cluster); 25 duplicate(s) found by reading, each closed.
- closed before the redraw: `tests/kurrentdb_always_available.rs` `architecture_blueprint_has_no_retired_kurrentdb_feature_reference` re-rolled the manifest-relative read `common::repo::repo_text` owns, as did thirteen sibling reads across eleven suites; every one now calls `repo_text`
- closed before the redraw: `tests/replan_episode_identity.rs` `serving` had two inline copies of its scan in its own file; both now call it
- closed before the redraw: `crates/rigger-conductor/src/conductor.rs` `integrate_and_emit` repeated `catch_up_owed_regeneration`'s regenerate/record/clear sequence twice; all three now call `regenerate_and_record`
- closed before the redraw: `tests/dash_run_tree_spine.rs` `an_off_linear_unit_with_no_gate_verdict_renders_no_phantom_gates_passed` repeated `a_crashed_implementer_renders_no_gates_node`'s crash-and-no-Gates assertions; both now call `assert_crash_at_implement_and_no_gates`
- closed before the redraw: `src/main.rs` `dash_read_liveness` repeated `liveness_ages_for_wave`'s marker-age loop, as did `rigger_activity` in `src/mcpserver.rs`; all three now call `liveness::marker_ages`
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

Two real recall gaps surfaced this way and were closed by widening the mechanical sweep with a new generalizable detector each - not a one-off citation - so the fix catches every present and future instance of its class, each pinned by a real-tree regression test: `find_proc_stat_or_status_readers` (decision `u85c2-proc-stat-worked-example`) groups every function reading a `/proc/<pid>/stat` or `/proc/<pid>/status` literal, closing the spec's own named worked example - `src/dash.rs:619-621` `process_state` next to `crates/rigger-process/src/reap.rs:224-229` `pid_starttime`, the same job on the same file with a different field/shape, upheld at spec 62's capstone; `find_parallel_constructor_clusters` (decision `u85c2-parallel-constructor-sweep`) groups 2+ non-test functions per `(file, Self type)` that build a `Self { .. }` / `TypeName { .. }` literal, closing `src/spawn.rs`'s `SpawnResult::liveness_fault` reading MISSING from its own `ok`/`failed` cluster even though all three are parallel constructors for one struct. A third worked example, `exploration_graph` (independently defined test-fixture builders in `tests/dash_exploration_route_client_contract.rs` and `tests/dash_kg_graph_route.rs`), was already caught correctly by the plain Jaccard pass with no sweep needed - confirming the mechanical pass itself has real recall, not only the two widened sweeps. Two further real defects, found on review rather than in this draw, were closed the same way: a RECALL gap the architecture lens routed to this criterion by name across two prior review rounds - this file's own bespoke source-text lexer (`scan_file`/`tokenize`) duplicating the codebase's ONE canonical tree-sitter extractor, `crates/rigger-grounder/src/grounder/symbols/extract.rs::extract` (its own module doc's claim, architecture 5.5.3) - closed by `find_bespoke_lexer_vs_canonical_extractor` (decision `u85c2-bespoke-lexer-sweep`), a fourth generalizable sweep; and a PRECISION defect the adversary found by reading every `same-named helper` cluster against `ScannedFn::enclosing_impl` - `find_same_named_helper_functions` was misclassifying REQUIRED trait-impl methods as coincidental duplication (`subscribe_all`/`subscribe_stream` across the `EventStore` trait's three backend adapters plus a test double, `blast_radius` across the `Grounder` trait's own default method, its override, and a test double) - closed by excluding members whose extracted Self type differs across the group when at least one comes from an actual `" for "` trait impl (decision `u85c2-same-named-helper-trait-impl-precision-fix`), mirroring `find_parallel_constructor_clusters`'s own `(file, Self type)` keying one function away. Round 7 (decision `u85c4-r7-exclude-periphery-file-from-adversarial-population`) excluded this criterion's own citation-guard periphery file from the draw's population (see this subsection's opening paragraph); that exclusion still applies unchanged.


## 3. Boundary Violations

Instrument: for each of the five named ports (`eventstore::EventStore`, `contextgraph::Projection`, `conductor::AgentDriver`, `gate::Runner`, `grounder::Grounder`), searched every production (pre-`#[cfg(test)]`) call site of that port's known concrete adapter modules from a NON-adapter, NON-composition-root file, and separately searched every domain-ish file's top-level `use` statements for a direct infrastructure-crate import. `src/main.rs` is exempt from the "reaches a concrete adapter" check: it is the composition root, and wiring concretions together is its designed job. Every citation below is resolved against the tree when the report is rendered.

FOUND, two violations:

Violation 1 (`AgentDriver`): `crates/rigger-conductor/src/conductor.rs:8137-8144` (`reclaim_terminal_unit_mutation_scratch`, real production code - above the `#[cfg(test)] mod tests` boundary at `crates/rigger-conductor/src/conductor.rs:13022`) calls `crate::driver::replay::cache_home_from` and `crate::driver::replay::reclaim_unit_mutation_scratch` directly by concrete module path. The port `conductor.rs` actually depends on for driving agents is `trait AgentDriver` (`crates/rigger-domain/src/agent.rs:150`) - one method, `spawn`. Neither called function is about driving an agent or replaying a recorded run (the concern `driver::replay` otherwise owns); both are pure, driver-instance-free scratch-lifecycle utilities that happen to live inside that one concrete adapter's module. The port that should have been used: none exists for this concern yet, which is itself the defect - `conductor.rs` (a use-case/orchestration file) should not need to know which concrete `AgentDriver` implementation happens to define its own mutation-scratch cache-home resolution. Fix direction for a follow-up spec: relocate `cache_home_from` and `reclaim_unit_mutation_scratch` out of `driver::replay` into a neutral, adapter-independent module (a `scratch` or `mutation` support module conductor.rs and every driver adapter can depend on alike), so no use-case file reaches into one specific adapter's internals for a concern that adapter does not conceptually own.

Violation 2 (`Grounder`): `crates/rigger-grounder/src/ingest.rs:157-193` (`walk_batches`, called from production `conductor::RunCtx::ingest_project_batches` at `crates/rigger-conductor/src/conductor.rs:10230`, itself called from `crates/rigger-conductor/src/conductor.rs:10221` above the `13022` `#[cfg(test)]` boundary) calls `crate::grounder::symbols::events::project_batches_paced` directly by concrete module path at line 167 to reuse the `symbols` grounder's already-persisted index for a one-time whole-project ingest walk, then at line 173 - same function, same missing-port defect, not a separate third violation - calls `crate::grounder::design::events::project_batches` directly by concrete module path for the design-doc half of the same walk. These two calls are two of the three named sites of section 2's own catalogued duplicate cluster (`dup-28bd24c1f93d`: `crates/rigger-grounder/src/grounder/design/events.rs:99-123`, `crates/rigger-grounder/src/grounder/symbols/events.rs:56-58`, and `crates/rigger-grounder/src/grounder/workflowdef.rs:224-231` - all three named `project_batches`), so this boundary violation and that duplication finding are two symptoms of one root cause - `ingest.rs` naming each concrete grounder submodule because no port exposes either. The `Grounder` port (`crates/rigger-domain/src/grounder.rs:64`: `ground`, `reindex`, `blast_radius`, `index_stamp`) serves real-time per-query grounding of an agent's prompt; none of its methods exposes "hand me every indexed file's projected events for a whole-project batch ingest," so `ingest.rs` - itself a domain ingest authority, not an adapter and not the composition root - has no port to depend on for either call and reaches the concrete `symbols` module (167) and the concrete `design` module (173) directly. Same missing-port defect class as violation 1. Fix direction for a follow-up spec: add an ingest-shaped port method (e.g. a `Grounder::project_batches` or a standalone `SymbolProjector` trait) covering both concrete modules, so `ingest.rs` depends on one abstraction instead of either concrete grounder module for its whole-project walk.

Also reaching `grounder::symbols::store::content_hash` from `crates/rigger-grounder/src/ingest.rs:358` and `src/canary_store.rs:153`: DISPOSITIONED as legitimate shared-primitive reuse, not a third violation. `content_hash` (`crates/rigger-grounder/src/grounder/symbols/store.rs:47-57`) is documented at its own definition as the content-identity primitive the `symbols` grounder's reindex freshening gate keys on, and `canary_store.rs`'s own doc comment (`src/canary_store.rs:131`) reuses it by deliberate author intent rather than growing another open-coded FNV-1a copy - a generic hashing utility that happens to live in the `symbols` module, not a grounding operation reached through the port. The broader duplication this primitive is meant to fix (the open-coded FNV-1a copies elsewhere in the crate, per `crates/rigger-domain/src/community.rs:67`'s own comment) is a separately tracked cross-cutting refactor (`arch-u2i-fnv1a-fourth-parallel-copy`), not this section's concern.

CHECKED AND CLEAN (three of five ports fully clean; the other two, `AgentDriver` and `Grounder`, are this section's two violations above - each search recorded so a clean result is not merely assumed):
- `eventstore::EventStore` concretion reach (`rusqlite::Connection::open` outside the SQLite adapters and `crates/rigger-store-sqlite/src/sqlite.rs`, the one opener every store connection goes through): in production, only doc-comment mentions (`src/main.rs:3356,4967`); the one call is a deliberate, explicitly-commented test-only raw-connection bypass (`src/main.rs:26099`, inside `#[cfg(test)] mod tests` opened at `src/main.rs:14668`) that reproduces a pre-append-guard corruption shape `Store::append` itself refuses to construct - a documented test technique, not a boundary violation.
- `contextgraph::Projection` concretion reach (`contextgraph::sqlite::*`): checked whole-tree, not only `crates/rigger-conductor/src/conductor.rs` - every one of `conductor.rs`'s 25 hits sits inside `#[cfg(test)] mod tests` (production `conductor.rs` only ever depends on `dyn Projection`), and the same is true wherever else `contextgraph::sqlite::Projector` is imported (`src/concepts.rs`, `src/dash.rs`, `crates/rigger-grounder/src/grounder/symbols/events.rs`, `crates/rigger-grounder/src/grounder/design/events.rs` - every import sits after that file's own `#[cfg(test)]` boundary).
- `gate::Runner` concretion reach (`gate::ExecRunner` / `RecordingRunner`): checked whole-tree, not only `crates/rigger-conductor/src/conductor.rs`. In `conductor.rs`, production depends only on `dyn gate::Runner` (`crates/rigger-conductor/src/conductor.rs:1449`); every production mention of a concrete runner is a doc comment (`crates/rigger-conductor/src/conductor.rs:7536,7648,7658,7665`), and the import of `ExecRunner` (`crates/rigger-conductor/src/conductor.rs:13028`) and every one of its 54 uses sit inside `#[cfg(test)] mod tests`. In `crates/rigger-driver/src/driver/replay.rs`, all 14 `ExecRunner` mentions sit inside that file's own `#[cfg(test)] mod tests` too.
- Use cases importing infrastructure: searched the `use` statements of every domain-ish file this audit's own code neighborhood names (`crates/rigger-conductor/src/conductor.rs`, `crates/rigger-domain/src/blocker.rs`, `crates/rigger-domain/src/spec.rs`, `crates/rigger-domain/src/watch.rs`, `crates/rigger-domain/src/community.rs`) for `rusqlite`, `reqwest`, `tonic`, `tokio`, `kurrentdb` - zero hits anywhere. Empty category.

A second mutation authority for one domain: the one previously-known instance in this codebase (`src/dash.rs` reimplementing `crates/rigger-process/src/reap.rs`'s `/proc` pid scan, spec 85's own Goal example, upheld at spec 62's capstone) is a duplicate READ-only reimplementation, not a bypassed MUTATION path - it is section 2's finding (`u85c2-proc-stat-worked-example`, `find_proc_stat_or_status_readers`), not re-counted here to avoid double-charging one defect to two sections. Checked process spawning as the one other plausible second-authority candidate: production code constructs every process through the one process-spawn port (`crates/rigger-process/src/subprocess.rs`), so production `conductor.rs` never builds a git command of its own. `crates/rigger-worktree-git/src/worktree.rs` is the sole git-worktree-mutation authority OUTSIDE the composition root. Inside it, `src/main.rs` (exempt from the port-concretion-reach check above, not from this one) holds two more git-worktree-mutation sites: `reap_then_remove_worktree` (`src/main.rs:3168-3180`), the sanctioned worktree half of the spec-34/spec-79 orphan-sweep and extensively reviewed across those specs - a deliberate design choice, not a gap; and `materialize_config_at_rev` (`src/main.rs:6148-6190`), a real, already-known, non-blocking gap (`arch-u13-config-checkout-bypasses-worktree-authority` / `arch-u2r-config-checkout-shells-git` / `arch-u2r2-replayrunner-and-config-checkout-persist-not-introduced`: the `Worktree` API is branch-creating and exposes no detach-at-rev checkout, so this is a gap in that authority rather than a competing abstraction). No second mutation authority found beyond the already-cited, already-catalogued `/proc` case and this already-dispositioned `materialize_config_at_rev` gap.

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

Instrument: subsystem grouping is a hand-derived, ordered filename-keyword rule table (mirrors criterion 1's own per-file classification convention: first-match-wins, narrowest first, an explicit residual named rather than silently dropped). The consolidation notes below cross-reference the ALREADY-COMMITTED `docs/audit/duplication-catalog.json` (criterion 2's own generator output, not re-scanned here) filtered to the 55 clusters whose every site sits under `tests/`; every count in them is read from the catalog at render time.

### 5.1 Subsystem grouping and consolidation map

| Subsystem | Consolidation note |
|---|---|
| Dashboard: KG lenses & viz (code/concepts/community/files lenses, graph exploration, overlays, viz layout) |  |
| CLI whole-binary integration (`cli.rs`, `watchdog_cli_periphery.rs`, `ci_lanes.rs`) | split plan at 5.3 |
| Knowledge-graph ingestion & context-graph projections | dedup/fold/identity concerns |
| Conductor orchestration: gates, courier, step/run lifecycle | `courier_registry_refresh_boundary_periphery.rs` + `courier_registry_refresh_fence_periphery.rs` + `courier_registry_refresh_periphery.rs` share 0 duplication clusters confined to just themselves |
| Reset / log compaction / store hygiene |  |
| Worktree & scratch/mutation-scratch lifecycle |  |
| Simplification-audit generator & its own periphery (this spec) |  |
| Spec/handbook lint & architecture-doc integrity |  |
| Grounding (symbols grounder, turbovec retirement, blast radius) | `kurrentdb_always_available.rs` + `turbovec_retired.rs` share 0 duplication clusters confined to just themselves |
| Process lifecycle: no-os-kill & reap discipline |  |
| Event store & config precedence |  |
| Canary (review-panel judge-the-judges evaluation) | `canary_false_positives_periphery.rs` + `canary_unattributed_rejects_periphery.rs` share 2 duplication clusters confined to just themselves; `canary_item_sharding_jobs_cap_periphery.rs` + `canary_progress_hook_periphery.rs` share 0 duplication clusters confined to just themselves |
| Concepts/community lens derivation & fold (non-viz) | `community_detection_cli.rs` + `concepts_derivation_cli.rs` share 0 duplication clusters confined to just themselves |
| Residual (no natural larger home) | `build_budget_slots_periphery.rs`, `gitsemver_derivation.rs` - named rather than forced into an ill-fitting bucket |

### 5.2 Shared fixtures to extract into `tests/common`

`tests/common/mod.rs` and `tests/common/fixtures/` already hold the shared fixtures - the gap is everything still duplicated OUTSIDE them. The catalog's all-helper-function test-only clusters (31 of the 55 test-only clusters) are the evidence; the 4 widest, by distinct files, are the headline case for extraction:

- `live_contains` / `live_memberships_of` / `live_node_ids` - 3 sites across 3 files (`dup-9c3b5c80063e`, near).
- `fixture_graph` - 3 sites across 3 files (`dup-c49b09543e58`, semantic).
- `courier_project` / `courier_project_with_commit` / `temp_rigger_project` / `temp_store_project` - 4 sites across 2 files (`dup-2a101767f24d`, near).
- `communities_no_concepts_graph` / `file_over_code_graph` / `mixed_membership_graph` - 3 sites across 2 files (`dup-498b51115191`, near).

Proposed home for each: `tests/common` (the catalog's own `proposed_home` field says so for each). Consolidating just these collapses roughly 13 duplicate definitions into 4 shared ones - the single largest mechanical simplification this audit identifies anywhere in the test suite.

### 5.3 `tests/cli.rs` split plan

`tests/cli.rs` holds 335 `#[test]` functions. Its existing internal section markers each name the spec and criterion whose tests follow it, not a CLI subcommand or subsystem - ad hoc organization that falls well short of a deliberate, complete per-surface structure. The split proposed below replaces those by-spec markers with a complete, deliberate BY CLI SUBCOMMAND SURFACE organization. A keyword-on-test-name pass (matching each test's dominant CLI verb: `step_`, `run_`, `validate_`, `reset_`, `watch_`/`watchdog_`, `canary_`, `dash_`/`status_`, `store_`/`eventstore_`, `spawn_`/`mutation_scratch_`/`scratch_`, `review_`/`gate_`, `setup_`/`precommit_`/`hook_`, `courier_`/`registry_`, `spec_`, `replay_`, `worktree_`, `emit_`/`peers_`/`decision_`, `stats_`, `heartbeat_`/`liveness_`, `prime_`/`version_`/`init_`) only cleanly covers 179 of the 335 tests (53%) - disclosed honestly rather than overclaimed, because a real fraction of `cli.rs`'s scenarios are DELIBERATELY end-to-end (a single test legitimately drives `step` + `run` + `validate` + `dash` together to prove a cross-cutting property, e.g. `a_run_driver_auto_starts_a_reachable_dash_with_a_url_shown_in_status` or `docs_ships_graph_hygiene_guidance_to_consumers`), which a bare keyword match cannot and should not force into one bucket. The proposed split is BY CLI SUBCOMMAND SURFACE - `cli.rs`'s own natural organizing concept, since the whole file drives the `rigger` binary end to end - into per-surface files (`tests/cli_step.rs`, `tests/cli_run.rs`, `tests/cli_validate.rs`, `tests/cli_reset.rs`, `tests/cli_watch.rs`, `tests/cli_canary.rs`, `tests/cli_dash.rs`, `tests/cli_store.rs`, `tests/cli_review.rs`, `tests/cli_setup.rs`, plus a residual `tests/cli_misc.rs` for the genuinely cross-cutting scenarios), with each test's home decided by its DOMINANT scenario on a human/AI read, not a mechanical keyword match - the same discipline this audit's own responsibility map applied to unassignable functions (named, never silently forced). Cross-referencing the catalog: `cli.rs` also participates in 5 of the catalog's cross-file test-only duplication clusters, several paired against files that WOULD merge with it under this split (`tests/step_attention_periphery.rs`, paired in 1 clusters; `tests/watchdog_cli_periphery.rs`, paired in 1 clusters) - the split is expected to shrink, not grow, the duplication surface.

### 5.4 Duplicated helpers across test files (beyond 5.2's headline cases)

31 test-only clusters in the committed catalog have every site as an ordinary (non-`#[test]`) helper function - the shared-fixture-extraction candidate class. Beyond the 4 in 5.2, the widest are:

- `assert_selected_server` / `assert_selected_sqlite` / `assert_server_reached_and_credentials_redacted` - 3 sites across 2 files (`dup-b58b0ff72b22`, near).
- `community_over_concepts_graph` / `full_in_budget_graph` - 2 sites across 2 files (`dup-06ffd3c67857`, near).
- `write_store_conn` - 2 sites across 2 files (`dup-56fda398784b`, semantic).
- `spawn` - 2 sites across 2 files (`dup-58e746ca5c35`, near).
- `spawn` - 2 sites across 2 files (`dup-6adbc1efd3a6`, near).
- `write_workflow` - 2 sites across 2 files (`dup-8dd44d643a5c`, semantic).

Every one of these 31 clusters, with its full site list and the catalog's own `proposed_home`, is already machine-readable in the committed `docs/audit/duplication-catalog.json` for a follow-up consolidation spec to consume directly - not re-enumerated exhaustively here to keep this section a report, not a second copy of the catalog.

### 5.5 Table-driven test families

24 test-only clusters have every site as a `#[test]` function - a literal-differs-only-in-input family, spec 85's own named table-driven-test candidate class. The largest families this audit first found - `tests/spec_lint.rs`'s feed-one-spec-through-`validate` defect tests and `tests/no_os_kill_audit.rs`'s one-termination-pattern-per-test checks - are closed, as are the `tests/reap_before_removal_audit.rs` exemption-coverage family, this generator's own scanner tests and the no-os-kill test helper's pid-refusal tests: their cases run as `test_cases!` rows over shared case helpers. The largest still open:

- none: every all-`#[test]` cluster is closed or dispositioned.

As with 5.4, the full 24-family list lives in the committed catalog by cluster id for a follow-up test-consolidation spec to consume directly.

## 6. Prioritized Plan

Twenty follow-up refactoring specs, ordered largest risk-reduction first. This section adds no new findings: every citation below points at a claim already recorded in section 1 (`docs/audit/responsibility-map.json`), section 2 (`docs/audit/duplication-catalog.json`), section 4.3 (`docs/audit/dead-code.json`), or sections 3 and 5's own prose. The three committed JSON files ground every count below (queried directly, never re-scanned). Item 0 (Tier 1) deletes the dead-code ledger (section 4.3); six of the remaining nineteen entries split a god file (tiers 2 and 3, two phases times three files); the other thirteen retire duplication or close a port gap (tiers 1, 4 and 5) - kept as separate entries throughout, per spec 85's own instruction that "the god-file splits and the duplication removals are separate entries so each can be its own run."

### 6.1 How this plan is ordered

Largest risk-reduction first is read as six tiers, ranked by the KIND of risk each entry retires, highest first:

1. Tier 1 - active correctness risk, PLUS item 0: a use case already depends on the wrong concretion, or two independent implementations of one concern can already drift apart silently (section 3's two boundary violations; the one already-drifted `/proc`-reading pair section 2 and section 3 both name) - live gaps, not just size. Item 0 (deleting the dead-code ledger, section 4.3) is placed here too, first of all: not a live-gap risk itself, but the cheapest, zero-behavior-change move available, and it shrinks the files tiers 2 and 3 operate on before either touches them.
2. Tier 2 - god-file test-module extraction: each of the three god files' own inline `#[cfg(test)] mod tests` holds 53-73% of that file's mapped functions (section 1's map), and moving it is a pure relocation with no production-behavior change - the single largest safe line-count reduction in this plan, and the precondition that makes tier 3 tractable.
3. Tier 3 - god-file production splits: section 1's own proposed module tree applied to the (now much smaller) remaining production surface of each god file. Higher execution risk than tier 2 because it touches live orchestration and CLI logic, so it is sequenced after tier 2 shrinks the target first.
4. Tier 4 - named production duplication sweeps: the mechanical mandatory sweeps section 2 ran regardless of the Jaccard pass (`Command::new`, `.rigger`-path literals, sqlite `Connection::open`, error-shaping helpers), each already a single committed cluster with its own proposed home.
5. Tier 5 - test-suite consolidation: section 5's own catalogued test-only duplication. No production-correctness exposure at all (worst case a test regresses, never the product), so it is ordered ahead only of tier 6 despite touching the largest raw line count anywhere in this plan.
6. Tier 6 - remaining catalog sweep: the 120 src-touching clusters section 2 found but tiers 1 and 4 did not individually name. Unlike every other tier, none of these 120 have been read and risk-assessed one at a time the way tiers 1-4's named clusters have - they are consumed straight from the catalog - so this tier carries production-correctness exposure tiers 2, 3 and 5 do not, and is ordered last: the follow-up spec must triage each cluster's own production-or-test status before merging it, not assume tier 5's blanket test-only treatment applies here too.

Within a tier, entries are ordered largest-first by the site or line count each retires - the same rule the tiers themselves follow, applied one level down.

### 6.2 Tier 1: active correctness risk

#### 0. Delete the dead-code set

- Scope: every entry of section 4.3's ledger (`docs/audit/dead-code.json`) and the tests that exercise only it; a test that also exercises live code is trimmed, not deleted. Under section 4.2's rule no entry is kept, and a function whose only caller was a deleted entry is deleted in the same pass.
- Status: complete - the ledger is empty.
- Risk: low. A deletion is a pure subtraction: `cargo build` and `clippy -D warnings` on both feature lanes catch any missed reference immediately.
- Unblocks: shrinks the files tiers 2-4 operate on before they touch them, so it runs first.

#### 1. Close the `AgentDriver` port gap around mutation-scratch reclaim

- Scope: `conductor.rs`'s production `reclaim_terminal_unit_mutation_scratch` (section 3 violation 1) calls `crate::driver::replay::cache_home_from` and `crate::driver::replay::reclaim_unit_mutation_scratch` by concrete module path - two pure, driver-instance-free scratch-lifecycle utilities that do not conceptually belong to the `driver::replay` concern they currently live inside. Relocate both into a neutral module every `AgentDriver` adapter and `conductor.rs` can depend on alike (no new trait needed - neither function takes a driver instance, so this is a home fix, not a port-method fix).
- Files: `crates/rigger-conductor/src/conductor.rs`, `crates/rigger-driver/src/driver/replay.rs`, a new home for the two relocated functions.
- Expected line delta: near zero net - a pure move of two functions.
- Risk: low-medium. The reclaim path is covered by spec 83's worktree-lifetime-fenced-by-spawn-liveness contract tests; those tests move with the functions, not get rewritten.
- Unblocks: retires the only `AgentDriver` port violation section 3 found.

#### 2. Close the `Grounder` port gap for whole-project batch ingest (retires dup-28bd24c1f93d in the same motion)

- Scope: section 3 violation 2 (`crates/rigger-grounder/src/ingest.rs::walk_batches`, reaching `grounder::symbols::events::project_batches_paced` and `grounder::design::events::project_batches` by concrete module path) and duplication cluster `dup-28bd24c1f93d` (3 modules' own twin `project_batches` functions, in `crates/rigger-grounder/src/grounder/design/events.rs`, `crates/rigger-grounder/src/grounder/symbols/events.rs`, `crates/rigger-grounder/src/grounder/workflowdef.rs`) are one root cause, not two - fix once. 3 CANDIDATES, ONE HOME (spec 85 CONSTRAINTS WALK): `dup-28bd24c1f93d`'s own mechanical `proposed_home` suggests relocating into `tests/common`, but every site is production code under `crates/rigger-grounder/src/grounder/` and `src/grounder/`, not a test helper - the mechanical heuristic has no "add a port method" category to route a production duplicate to, so it mis-fires here. This plan follows section 3's own reasoned disposition instead: add a `Grounder::project_batches` port method (or a standalone `SymbolProjector` trait) covering all 3 concrete modules, and point `ingest.rs` at it.
- Files: `crates/rigger-grounder/src/ingest.rs`, `crates/rigger-grounder/src/grounder/mod.rs`, `crates/rigger-grounder/src/grounder/symbols/events.rs`, `crates/rigger-grounder/src/grounder/design/events.rs`, `crates/rigger-grounder/src/grounder/workflowdef.rs`.
- Expected line delta: roughly neutral - one new trait method plus 3 thin impls, minus the 3 duplicate bodies `dup-28bd24c1f93d` catalogs.
- Risk: medium. `ingest.rs`'s own module doc calls it "the ONE walk-and-content-key authority" - a load-bearing path; needs the existing whole-project-ingest and reindex-freshening coverage to stay green, not just the duplicate sites' own tests.
- Unblocks: retires the one `Grounder` port violation section 3 found and `dup-28bd24c1f93d` together, rather than as two separately-tracked fixes.

#### 3. Retire the duplicate `/proc`-reading authority (`dup-af816213608d` + `dup-202133ba5e92`)

- Scope: the production half is done - `src/dash.rs::process_state` and `crates/rigger-process/src/reap.rs::pid_starttime` both read their `/proc/<pid>/stat` field through `crates/rigger-process/src/reap.rs::stat_field_after_comm`, the one parser of the kernel's `pid (comm) state ...` layout (`read_ppid` reads `/status`, a different file). What remains is the test-only readers (`dup-202133ba5e92`, 9 sites across `crates/rigger-process/src/reap.rs`, `tests/common/fixtures/host.rs`, `tests/simplification_audit.rs`, such as the shared `tests/common/fixtures/host.rs::pgid_of` fixture) and 48 raw `/proc`-path string literals across 7 files (`dup-af816213608d`), most of them assertion messages and this audit's own sweep names rather than reads.
- Files: the test-only readers `dup-202133ba5e92` names.
- Expected line delta: small and negative - a test fixture reads its field through one shared helper instead of re-splitting the stat line.
- Risk: low - test-only; nothing this touches can signal or end a process, so it carries none of the no-os-kill gate's own risk surface.
- Unblocks: retires the last copies of the "duplicate implementation reconciled after the fact" pattern the operator's strict-DRY rule targets - the concrete precedent spec 85's own Goal cites.

### 6.3 Tier 2: god-file test-module extraction

Each god file's inline test module is the set of its `is_test: true` entries in the committed `docs/audit/responsibility-map.json`. Each entry below moves an already-passing test module with no intended production-behavior change - a `cargo test` pass before and after is the whole verification. The line figures below sum those test functions' own spans, so they exclude the module-level doc comments, `use` statements and blank lines around them.

#### 4. Extract `crates/rigger-conductor/src/conductor.rs`'s inline test module

- Scope: the file's `#[cfg(test)] mod tests` holds 560 of its 767 mapped functions (73%), roughly 24045 lines of test-function spans. Partition into a `src/conductor/tests/` directory, one file per concern, reusing the same names section 1 already assigned the file's own production buckets (`run_ctx`, `support`, `gate`, `prior_failure`, `schedule`, ...) so the split needs no new naming scheme.
- Files: `crates/rigger-conductor/src/conductor.rs` -> `crates/rigger-conductor/src/conductor.rs` (production only) + `src/conductor/tests/*.rs`.
- Expected line delta: 0 net (repo-wide) - roughly 24045 lines relocated out of `crates/rigger-conductor/src/conductor.rs`.
- Risk: low - mechanical move of passing tests, zero intended behavior change.
- Unblocks: shrinks `conductor.rs` to its production code before tier 3 touches a single production line, cutting the odds that an unrelated future unit's blast radius collides with this file.

#### 5. Extract `src/main.rs`'s inline test module

- Scope: the file's `#[cfg(test)] mod tests` holds 374 of its 700 mapped functions (53%), roughly 10084 lines of test-function spans. Same partition approach as item 4, reusing section 1's own production bucket names (`commands`, `store`, `support`, `provenance`, `render`, ...).
- Files: `src/main.rs` -> `src/main.rs` (production only) + `src/main/tests/*.rs`.
- Expected line delta: 0 net (repo-wide) - roughly 10084 lines relocated out of `src/main.rs`.
- Risk: low, same rationale as item 4.
- Unblocks: shrinks `main.rs` to its production code before tier 3's own main.rs split.

#### 6. Extract `src/dash.rs`'s inline test module

- Scope: the file's `#[cfg(test)] mod tests` holds 143 of its 231 mapped functions (61%), roughly 5333 lines of test-function spans. Lower effort than items 4-5: section 1's own classifier already found 4 pre-existing sub-boundaries inside this one test module (`dash::tests::calls_route_c4`, `dash::tests::metadata_card_c2`, `dash::tests::rationale_overlay_c3`, `dash::tests::subject_view_c5`), so the partition points already exist and need only become their own files.
- Files: `src/dash.rs` -> `src/dash.rs` (production only) + `src/dash/tests/*.rs`.
- Expected line delta: 0 net (repo-wide) - roughly 5333 lines relocated out of `src/dash.rs`.
- Risk: low - the lowest-effort of the three, for the reason above.
- Unblocks: shrinks `dash.rs` to its production code before tier 3's own dash.rs split.

### 6.4 Tier 3: god-file production splits

Each entry below applies section 1's own proposed module tree to a god file's production surface, sequenced after the matching tier-2 entry removes that file's test bulk first. Every module name and function/line count below is summed directly from the committed `docs/audit/responsibility-map.json` (function-body spans only); a file's remaining non-function production lines - struct/enum/type definitions, `use` statements, module docs - are outside section 1's own function-only scan and move with whichever module they sit beside, without needing their own assignment.

#### 7. Split `crates/rigger-conductor/src/conductor.rs`'s production code into `src/conductor/*.rs`

- Scope: 180 mapped functions across 15 proposed modules (roughly 7633 lines of function bodies) plus 27 unassigned functions (1125 lines, each individually named in the committed map for manual placement, per section 1's own "unassignable functions are named as such, never omitted" rule). Headline buckets (functions/lines): `conductor::run_ctx` (125/6620), `conductor::support` (14/479), `conductor::gate` (11/132), `conductor::prior_failure` (3/73), `conductor::schedule` (3/71).
- Files: `crates/rigger-conductor/src/conductor.rs` -> `src/conductor/mod.rs` + `src/conductor/{run_ctx,support,gate,prior_failure,schedule,run,review,budget,emit,spawn,review_outcome,ground,gate_ratchet,throwaway,integration_approval}.rs`.
- Expected line delta: 0 net - pure relocation of roughly 8758 lines of function bodies.
- The largest bucket, `conductor::run_ctx`, may warrant its own second pass if it does not decompose cleanly into one file.
- Risk: medium-high - conductor.rs is the composition root's own most complex use-case file; every intermediate commit needs the full `cargo test`, no-os-kill and reap audits green, not just the final one.
- Unblocks: the largest reduction in production-code blast-radius collision risk this audit identifies; makes future duplication-spotting against conductor.rs's own logic tractable by a human reviewer, not only by the mechanical scanner.

#### 8. Split `src/main.rs`'s production code into `src/main/*.rs`

- Scope: 212 mapped functions across 16 proposed modules (roughly 6524 lines of function bodies) plus 114 unassigned functions (3080 lines, each individually named in the committed map for manual placement, per section 1's own "unassignable functions are named as such, never omitted" rule). Headline buckets (functions/lines): `main::commands` (41/3033), `main::store` (33/880), `main::support` (37/839), `main::provenance` (22/465), `main::render` (10/352).
- Files: `src/main.rs` -> `src/main.rs` (composition root, thinned) + `src/cli/{commands,store,support,provenance,render,dash_glue,setup,liveness,replay_runner,store_location,run_registration,live_writer_facts,docs_overlay,scaffold_report,residue_report,store_selection}.rs`.
- Expected line delta: 0 net - pure relocation of roughly 9604 lines of function bodies.
- Risk: medium - `main.rs` is the composition root itself; the split must preserve which concretions get wired where, not merely move text.
- Unblocks: shrinks `main.rs` to a genuine composition root plus a `cli/` module tree, matching the ports-and-adapters shape this project already mandates everywhere else.

#### 9. Split `src/dash.rs`'s production code into `src/dash/*.rs`

- Scope: 47 mapped functions across 6 proposed modules (roughly 1311 lines of function bodies) plus 41 unassigned functions (795 lines, each individually named in the committed map for manual placement, per section 1's own "unassignable functions are named as such, never omitted" rule). Headline buckets (functions/lines): `dash::server` (8/605), `dash::render` (18/402), `dash::reproject` (5/176), `dash::registry` (6/65), `dash::response` (6/48).
- Files: `src/dash.rs` -> `src/dash/mod.rs` + `src/dash/{server,render,reproject,registry,response,dash_marker}.rs`.
- Expected line delta: 0 net - pure relocation of roughly 2106 lines of function bodies.
- Risk: low-medium - the always-on dash's own contract (loopback-only, zero-new-dependency) is unaffected by a pure module split.
- Unblocks: completes the god-file split trio; the third program-sized file becomes an ordinary module tree.

### 6.5 Tier 4: named production duplication sweeps

Each entry is one of section 2's five named mandatory sweeps - collected mechanically regardless of the Jaccard pass, per spec 85's own Design.

#### 10. Consolidate the 539 `.rigger`-path string-literal sites (`dup-f4f077a18177`) - the single largest cluster in the entire catalog by site count

- Scope: one `.rigger`-relative path-composition helper (the cluster's own `proposed_home`) every one of the 539 sites routes through instead of building its own literal.
- Files: spans dozens of files including `crates/rigger-conductor/src/conductor.rs`, `crates/rigger-config-files/src/config_store.rs`, `src/dash.rs`, `src/docs.rs`, `crates/rigger-gates-shell/src/gate.rs`, `crates/rigger-grounder/src/grounder/mod.rs`, `crates/rigger-grounder/src/grounder/symbols/store.rs`, `crates/rigger-grounder/src/ingest.rs`, `src/main.rs`, `crates/rigger-process/src/reap.rs`, `crates/rigger-store-sqlite/src/registry.rs`, `src/worktree.rs` plus many `tests/` files - the full site list is in the committed `docs/audit/duplication-catalog.json` under `dup-f4f077a18177` for the follow-up spec to consume directly, not re-enumerated here.
- Expected line delta: negative - 539 literal compositions collapse toward one helper's call sites; the helper itself is small.
- Risk: medium - the largest surface-area sweep in this plan by site count, even though each individual site is trivial; needs a mechanical rewrite pass plus a full-suite green run, not hand-editing 539 sites.
- Unblocks: the biggest single site-count reduction available anywhere in the duplication catalog.

#### 11. The 83 `Command::new` call sites (`dup-9f07a7bf395a`) - production spawns already route through one process-spawn port

- Scope: every production spawn routes through `crates/rigger-process/src/subprocess.rs` (the cluster's own `proposed_home`), and the audit's `the_process_spawn_port_is_the_only_production_command_new_caller` gate refuses a new direct construction anywhere else in production code. The 1 site(s) in `crates/rigger-process/src/subprocess.rs` are the port itself; the other 82 are test code spawning git, shells and the product binary.
- Files: `crates/rigger-process/src/subprocess.rs` plus test code in `src/` and `tests/` - full site list in `docs/audit/duplication-catalog.json` under `dup-9f07a7bf395a`.
- Expected line delta: none left in production; a test site that repeats a shared fixture's spawn routes through that fixture instead.
- Risk: low - no production spawn is left to move, and the gate keeps it that way.
- Unblocks: the next process-spawning concern added anywhere in the crate reuses the port instead of constructing its own `Command`.

#### 12. Consolidate the 34 sqlite `Connection::open` call sites (`dup-f4557659a212`)

- Scope: one sqlite-connection-opening adapter function (the cluster's own `proposed_home`) spanning `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs`, `crates/rigger-store-sqlite/src/eventstore/sqlite.rs` and `src/main.rs`, plus several `tests/` files.
- Files: full site list in `docs/audit/duplication-catalog.json` under `dup-f4557659a212`.
- Expected line delta: negative - 34 open calls collapse toward one function.
- Risk: medium - touches the event store and context graph's own connection-lifecycle code; needs the store-identity and store-resolution contract tests green throughout.
- Unblocks: one place to change pragma/timeout/journal-mode settings instead of 34.

#### 13. Consolidate the 10 error-shaping helper sites (`dup-be3f13e37fd5`) - caution, confirm before merging

- Scope: the cluster spans `src/worktree.rs` (`land_reports_a_generic_error_for_a_refusal_that_is_neither_tip_moved_nor_blocked`, `revert_on_base_aborts_and_errors_on_a_conflicting_revert`) and 4 unrelated test files - a wide spread for one claimed duplicate. This may be a threshold-gaming false cluster (spec 85's own CONSTRAINTS WALK: "the threshold is a floor for the mechanical pass; the reading pass owns semantic duplicates") rather than one real shared concern - the follow-up spec's first job is confirming by reading whether these 10 sites share actual logic before proposing one helper, not assuming the cluster label proves it.
- Files: the `src/` files above, plus the 4 test files named in `docs/audit/duplication-catalog.json` under `dup-be3f13e37fd5`.
- Expected line delta: unknown pending the confirmation read above - potentially zero if the cluster does not survive a human read.
- Risk: low (the smallest-site-count sweep), but with the stated precondition.
- Unblocks: either a genuine fifth consolidation, or a documented "not a real duplicate" disposition that keeps the catalog honest for whoever reads it next.

### 6.6 Tier 5: test-suite consolidation

Every entry cites section 5's own already-catalogued test-only duplication; none of it carries production-correctness risk.

#### 14. Extract the headline shared test fixtures into `tests/common` (section 5.2)

- Scope: `dup-9c3b5c80063e` (3 files), `dup-c49b09543e58` (3 files), `dup-2a101767f24d` (2 files), `dup-498b51115191` (2 files) - roughly 13 duplicate definitions collapsing into 4 shared ones, the single largest mechanical simplification section 5 identifies anywhere in the test suite.
- Files: per-cluster, from the committed catalog, plus `tests/common/`.
- Expected line delta: negative - each fixture's small body survives once instead of once per file.
- Risk: low - test-only, and `tests/common/` already holds the same shape of shared fixture.
- Unblocks: item 17 below (the remaining test-helper clusters) reuses the same `tests/common` home this item establishes.

#### 15. Split `tests/cli.rs` by CLI subcommand surface (section 5.3's plan)

- Scope: 335 tests, split into `tests/cli_{step,run,validate,reset,watch,canary,dash,store,review,setup}.rs` plus a residual `tests/cli_misc.rs` for the genuinely cross-cutting scenarios section 5.3 names, using each test's dominant scenario (a human/AI read, not the 53%-coverage keyword match section 5.3 already disclosed as insufficient alone).
- Files: `tests/cli.rs` and the eleven new files above.
- Expected line delta: 0 net - pure relocation into eleven files.
- Risk: low-medium - a mechanical per-test move with `cargo test`'s full pass count as the verification.
- Unblocks: splits a file in 5 cross-file duplication clusters (section 5.3) and lets item 17's remaining-clusters sweep target smaller, subcommand-scoped files.

#### 16. Convert the largest remaining table-driven test families into parametrized tables (section 5.5)

- Scope, largest first (0 sites across 0 clusters; the spec_lint, no-os-kill, reap-audit exemption, scanner and pid-refusal families are already closed):
- none: every all-`#[test]` cluster is closed or dispositioned.
- Files: the files named above.
- Expected line delta: negative - each family's near-identical test bodies collapse into `test_cases!` rows over one case helper.
- Risk: low - test-only, and each family already shares one body shape (section 5.5's own finding).
- Unblocks: the largest remaining reduction in raw `#[test]` body count.

#### 17. Sweep the remaining 27 test-only helper-duplication clusters (section 5.4, beyond item 14's headline fixtures)

- Scope: the 31 test-only, all-helper-function clusters section 5.4 names, minus the ones item 14 already covers - consumed directly from `docs/audit/duplication-catalog.json`, not re-enumerated here (section 5.4's own stated approach).
- Files: per-cluster, from the committed catalog.
- Expected line delta: negative, cumulative across 27 clusters.
- Risk: low - test-only.
- Unblocks: closes out the helper-duplication half of the test suite's own strict-DRY exposure.

#### 18. Sweep the remaining 24 table-driven test families (section 5.5, beyond item 16's headline families)

- Scope: the 24 test-only, all-`#[test]` clusters section 5.5 names, minus the 0 cluster ids item 16 already covers - consumed directly from `docs/audit/duplication-catalog.json`.
- Files: per-cluster, from the committed catalog.
- Expected line delta: negative, cumulative.
- Risk: low - test-only.
- Unblocks: closes out the table-driven-test half of the test suite's own strict-DRY exposure; combined with items 14 and 16-17, retires all 55 test-only clusters section 2 found.

### 6.7 Tier 6: remaining catalog sweep

Unlike tier 5, this entry's own clusters are NOT known to be test-only - each one needs its own read before merging (see `### 6.1`'s tier 6 rationale above).

#### 19. Sweep the remaining 120 src-touching duplication clusters (section 2, beyond tiers 1 and 4's 7 named clusters)

- Scope: of the catalog's 182 clusters, 55 are test-only (items 14 and 16-18 above) and 7 are the named tier-1/tier-4 items (`dup-9f07a7bf395a`, `dup-f4f077a18177`, `dup-f4557659a212`, `dup-af816213608d`, `dup-202133ba5e92`, `dup-28bd24c1f93d`, `dup-be3f13e37fd5`); the remaining 120 clusters touching `src/` - mostly small 2-5-site exact/near matches like the two worked examples section 2 itself opens with (`dup-252bfd2c826c`, `dup-59ab8e8cdb6e`) - are swept here, largest exact-duplicate clusters first, consumed directly from `docs/audit/duplication-catalog.json`.
- Files: per-cluster, from the committed catalog.
- Expected line delta: negative, cumulative; the largest single contributor is whichever exact cluster has the most sites (read from the catalog at spec-writing time, not fixed here).
- Risk: low-medium - unlike tier 5, some of these clusters are production code, so each merge needs its own test-coverage check, not a blanket "test-only" pass.
- Unblocks: the last of the catalog's 182 clusters; after items 1-3 and 10-19 all land, a future spec can state and check that the duplication catalog's own drift guard finds zero live clusters left unaddressed.

### 6.8 Dead and vestigial code beyond item 0: no further follow-up

Section 4.2's rule leaves no dead-code category for a later plan item: a function is live or it is deleted by item 0. Both named retirements (`turbovec`, `kurrentdb`) are still fully clean, and the two stale-looking doc paths found remain confirmed generic illustrative examples, not real dangling references.
