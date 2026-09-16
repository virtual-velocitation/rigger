# Simplification Audit - 2026-09

The audit report spec 85 derives the follow-up refactoring specs from. Six sections, each claim citing `file:line`; see `specs/85-simplification-audit.md` for scope.

## 1. Responsibility Map

Every function in `src/conductor.rs`, `src/main.rs` and `src/dash.rs` (1669 functions total), assigned to a proposed module by `tests/simplification_audit.rs`'s deterministic scanner + rule-table classifier (never by hand). Instrument: the brace-matching scanner over the three named files.

### Proposed module tree

- `conductor::budget` (5 functions)
  - `src/conductor.rs:359-361` `compensation_queued_key` - name contains "compensat" (budget accounting); grouped under `conductor::budget`.
  - `src/conductor.rs:966-995` `pending_compensations_from_log` - name contains "compensat" (budget accounting); grouped under `conductor::budget`.
  - `src/conductor.rs:1498-1502` `budget_refused` - name contains "budget" (budget accounting); grouped under `conductor::budget`.
  - `src/conductor.rs:1507-1509` `is_budget_refused` - name contains "budget" (budget accounting); grouped under `conductor::budget`.
  - `src/conductor.rs:12429-12439` `mutation_scratch_settled` - name contains "mutation" (budget accounting); grouped under `conductor::budget`.
- `conductor::emit` (2 functions)
  - `src/conductor.rs:389-391` `quarantine_record_key` - name contains "record" (event/decision emission); grouped under `conductor::emit`.
  - `src/conductor.rs:12248-12281` `recorded_adoption` - name contains "record" (event/decision emission); grouped under `conductor::emit`.
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
  - `src/conductor.rs:1613-1623` `verdict_channel_mismatch` - name contains "verdict" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:1628-1630` `is_verdict_channel_mismatch` - name contains "verdict" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:10848-10855` `with_gate_hold` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:10983-10990` `gate_failure_cause` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:11002-11004` `verdict_approves` - name contains "verdict" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:11020-11029` `last_verdict` - name contains "verdict" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:11036-11038` `has_verdict_line` - name contains "verdict" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:11045-11047` `emitted_verdict_approves` - name contains "verdict" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:11057-11069` `verdict_compensates` - name contains "verdict" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:12714-12722` `critique_gate_name` - name contains "gate" (gate execution); grouped under `conductor::gate`.
- `conductor::gate_ratchet` (1 function)
  - `src/conductor.rs:879-885` `for_persistent_failure` - method inside `impl GateRatchet`; grouped with its other `GateRatchet` methods.
- `conductor::ground` (1 function)
  - `src/conductor.rs:11567-11576` `graph_around_recovery` - name contains "graph" (grounding integration); grouped under `conductor::ground`.
- `conductor::integration_approval` (1 function)
  - `src/conductor.rs:1214-1216` `approved` - method inside `impl IntegrationApproval`; grouped with its other `IntegrationApproval` methods.
- `conductor::prior_failure` (3 functions)
  - `src/conductor.rs:1240-1245` `is_empty` - method inside `impl PriorFailure`; grouped with its other `PriorFailure` methods.
  - `src/conductor.rs:1249-1267` `summary` - method inside `impl PriorFailure`; grouped with its other `PriorFailure` methods.
  - `src/conductor.rs:1272-1319` `block` - method inside `impl PriorFailure`; grouped with its other `PriorFailure` methods.
- `conductor::review` (9 functions)
  - `src/conductor.rs:508-555` `route_review_tier` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `src/conductor.rs:1567-1580` `degenerate_reviewer` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `src/conductor.rs:1585-1587` `is_degenerate_reviewer` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `src/conductor.rs:10939-10948` `review_evidence` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `src/conductor.rs:11148-11155` `review_protocol` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `src/conductor.rs:12333-12338` `review_worktree_dir` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `src/conductor.rs:12345-12347` `review_branch` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `src/conductor.rs:12820-12822` `review_roster` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `src/conductor.rs:12828-12834` `adjudicator_roster` - name contains "adjudicat" (review orchestration); grouped under `conductor::review`.
- `conductor::review_outcome` (2 functions)
  - `src/conductor.rs:911-918` `approved` - method inside `impl ReviewOutcome`; grouped with its other `ReviewOutcome` methods.
  - `src/conductor.rs:919-926` `rejected` - method inside `impl ReviewOutcome`; grouped with its other `ReviewOutcome` methods.
- `conductor::run` (6 functions)
  - `src/conductor.rs:11962-11964` `unit_branch` - name contains "unit" (unit run loop); grouped under `conductor::run`.
  - `src/conductor.rs:11974-11980` `unit_worktree_dir` - name contains "unit" (unit run loop); grouped under `conductor::run`.
  - `src/conductor.rs:12622-12635` `stale_units_from_log` - name contains "unit" (unit run loop); grouped under `conductor::run`.
  - `src/conductor.rs:12657-12695` `stale_downstream_units` - name contains "unit" (unit run loop); grouped under `conductor::run`.
  - `src/conductor.rs:12729-12747` `unit_slug` - name contains "unit" (unit run loop); grouped under `conductor::run`.
  - `src/conductor.rs:12757-12796` `baseline_units` - name contains "unit" (unit run loop); grouped under `conductor::run`.
- `conductor::run_ctx` (128 functions)
  - `src/conductor.rs:2944-2946` `emit` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:2955-2970` `append_and_fold` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:2982-3004` `append_and_fold_batch` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3009-3015` `emit_with_actor` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3025-3033` `emit_meta` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3046-3048` `emit_keyed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3059-3084` `emit_keyed_meta` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3119-3158` `emit_keyed_batch` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3167-3173` `agent_model` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3188-3190` `recorded_gate_verdict` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3200-3202` `cached_green_verdict` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3209-3211` `is_stale` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3236-3247` `effective_attempts` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3258-3266` `gate_verdict_high_water` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3274-3288` `mark_stale_downstream` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3304-3382` `drain_compensations` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3395-3444` `commits_to_compensate` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3454-3485` `emit_gate_skip` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3502-3561` `emit_gate_verdict` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3569-3576` `max_retries` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3589-3600` `max_retries_for` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3626-3632` `budget_tripped` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3639-3641` `spawn_is_recorded` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3657-3680` `reserve_spawn` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3683-3685` `budget_broke` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3691-3693` `parked` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3699-3701` `manual_review_pending` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3706-3708` `budget_halted` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3717-3747` `trip_budget_breaker` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3758-3768` `halt_reason` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3773-3783` `check_coverage_or_flag` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3785-3904` `run_wave` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3913-3947` `partition_wave` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3952-3962` `partition_requested` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3967-3991` `run_batch` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3993-4046` `start_and_run_stage` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4048-4156` `run_stage` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4161-4178` `stage_paused_for_review` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4186-4188` `effective_review_panel` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4200-4207` `select_review_panel` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4219-4241` `log_review_tier` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4255-4273` `assert_isolated_cwd` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4288-4344` `reviewer_spawn_opts` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4414-4532` `review_unit` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4569-4630` `spawn_sdet_author` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4636-5377` `run_single_stage` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:5383-5390` `effective_speculation_width` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:5399-5406` `speculates` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:5415-5431` `speculation_lane_worktree` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:5452-5815` `run_speculation` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:5828-5907` `emit_speculation_winner_status` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:5926-5963` `record_speculation_reject` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:5973-6006` `cancel_speculation_candidates` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6008-6061` `run_fan_out_stage` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6071-6237` `run_fan_out_review_loop` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6268-6320` `run_review_agents_concurrently` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6332-6363` `run_lens` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6402-6542` `run_reviewer` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6578-6595` `gating_spawn_emitted_approve` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6617-6627` `review_spawn_errored` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6651-6684` `reviewer_result_is_degenerate` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6699-6729` `run_adversary` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6747-6784` `run_adjudicator` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6806-6827` `dag_unit_blast_radii` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6836-6915` `build_dag_critique_prompt` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6924-7015` `re_plan` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7026-7059` `run_plan_critique_gate` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7069-7235` `plan_critique_loop` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7263-7274` `build_env` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7291-7298` `run_base_env` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7323-7329` `spawn_env` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7340-7345` `build_budget` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7370-7378` `shared_build_cache_paths` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7389-7632` `run_gates` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7648-7739` `run_gate_with_taxonomy` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7796-7803` `gc_integrated_branches` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7810-7913` `gc_integrated_branches_logged` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7949-7956` `reclaim_terminal_unit_mutation_scratch` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7986-8172` `run_deferred_gates` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:8183-8250` `record_gate` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:8308-8781` `integrate_and_emit` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:8823-8830` `integrate_plan_commits` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:8832-8952` `integrate_plan_commits_inner` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:8963-8979` `record_plan_intent` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:8990-9018` `read_plan_landed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9026-9055` `record_plan_landed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9060-9066` `regenerate_rule_for` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9091-9127` `run_regenerate_command` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9146-9171` `regenerate_conflicted_paths` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9199-9213` `catch_up_owed_regeneration` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9218-9225` `regenerate_pending_for` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9236-9241` `clear_regenerate_pending` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9246-9252` `pending_landing_for` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9257-9262` `clear_pending_landing` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9269-9277` `union_regenerate_pending` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9304-9332` `record_regenerate_pending` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9341-9355` `record_placeholder_staged` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9365-9382` `record_regenerate_commit` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9392-9406` `record_merge_attempt` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9412-9431` `record_merge_outcome` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9438-9453` `record_landing_intent` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9458-9466` `record_landed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9477-9495` `record_integrate_row` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9509-9563` `spawn_conflict_resolution_implementer` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9572-9574` `build_system_prompt` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9584-9596` `ground_query` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9602-9612` `implementer_agent` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9616-9640` `plan_protocol` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9668-9702` `grounded_blast_radius` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9718-9731` `grounded_seed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9755-9784` `record_blast_radius` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9786-9792` `build_prompt` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9803-9805` `build_review_prompt` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9813-9847` `build_prompt_with_failure` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9849-9913` `graph_context` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9926-9934` `ingest_project_into_graph` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9942-10001` `ingest_project_batches` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10006-10006` `ingest_project_into_graph` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10020-10028` `ingest_files_into_graph` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10032-10032` `ingest_files_into_graph` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10034-10049` `emit_lesson` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10054-10060` `agent_isolated` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10183-10291` `adopt_prior_criterion_branch` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10313-10354` `resume_phase` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10356-10388` `stage_worktree` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10403-10425` `review_only_worktree` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10427-10793` `harvest_proposed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10815-10837` `resolve_served_criterion` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
- `conductor::schedule` (9 functions)
  - `src/conductor.rs:1661-1667` `plan_landing_failed` - name contains "plan" (unit scheduling); grouped under `conductor::schedule`.
  - `src/conductor.rs:1672-1674` `is_plan_landing_failed` - name contains "plan" (unit scheduling); grouped under `conductor::schedule`.
  - `src/conductor.rs:10879-10881` `normalize_criterion_id` - name contains "criterion" (unit scheduling); grouped under `conductor::schedule`.
  - `src/conductor.rs:10900-10905` `criterion_stable_id` - name contains "criterion" (unit scheduling); grouped under `conductor::schedule`.
  - `src/conductor.rs:12078-12138` `prior_criterion_unit` - name contains "criterion" (unit scheduling); grouped under `conductor::schedule`.
  - `src/conductor.rs:12451-12473` `partition_by_blast_radius` - name contains "partition" (unit scheduling); grouped under `conductor::schedule`.
  - `src/conductor.rs:12491-12509` `partition_with_serialize` - name contains "partition" (unit scheduling); grouped under `conductor::schedule`.
  - `src/conductor.rs:12524-12544` `blast_radius_conflicts` - name contains "blast" (unit scheduling); grouped under `conductor::schedule`.
  - `src/conductor.rs:12963-12982` `ready_stages` - name contains "stage" (unit scheduling); grouped under `conductor::schedule`.
- `conductor::spawn` (4 functions)
  - `src/conductor.rs:1460-1464` `parked_spawn` - name contains "spawn" (spawn lifecycle); grouped under `conductor::spawn`.
  - `src/conductor.rs:1469-1471` `is_parked` - name contains "parked" (spawn lifecycle); grouped under `conductor::spawn`.
  - `src/conductor.rs:1523-1525` `is_parked_or_budget_refused` - name contains "parked" (spawn lifecycle); grouped under `conductor::spawn`.
  - `src/conductor.rs:12884-12895` `wave_ready` - name contains "wave" (spawn lifecycle); grouped under `conductor::spawn`.
- `conductor::support` (19 functions)
  - `src/conductor.rs:450-452` `path_is_high_risk` - name contains "is_" (predicate helper); grouped under `conductor::support`.
  - `src/conductor.rs:1052-1085` `conflict_regenerate_pending_from_log` - name contains "from_" (conversion helper); grouped under `conductor::support`.
  - `src/conductor.rs:1110-1149` `pending_landing_from_log` - name contains "from_" (conversion helper); grouped under `conductor::support`.
  - `src/conductor.rs:1159-1190` `integrate_attempted_from_log` - name contains "from_" (conversion helper); grouped under `conductor::support`.
  - `src/conductor.rs:2570-2572` `has_producer` - name contains "has_" (predicate helper); grouped under `conductor::support`.
  - `src/conductor.rs:10864-10866` `normalize_ws` - name contains "normalize" (normalization helper); grouped under `conductor::support`.
  - `src/conductor.rs:11121-11123` `build_system_prompt` - name contains "build" (generic construction helper); grouped under `conductor::support`.
  - `src/conductor.rs:11383-11527` `write_capped_section` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `src/conductor.rs:11578-11634` `write_code_neighborhood` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `src/conductor.rs:11747-11830` `write_design_intent` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `src/conductor.rs:11836-11851` `write_capped_decisions` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `src/conductor.rs:11856-11873` `write_capped_lessons` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `src/conductor.rs:11881-11906` `write_capped_findings` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `src/conductor.rs:11915-11923` `write_peers_pointer` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `src/conductor.rs:11938-11948` `write_lookup_pointer` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `src/conductor.rs:12190-12203` `branch_is_foreign` - name contains "is_" (predicate helper); grouped under `conductor::support`.
  - `src/conductor.rs:12556-12558` `is_fan_out` - name contains "is_" (predicate helper); grouped under `conductor::support`.
  - `src/conductor.rs:12579-12581` `is_producer` - name contains "is_" (predicate helper); grouped under `conductor::support`.
  - `src/conductor.rs:12840-12842` `has_llm_verifier` - name contains "has_" (predicate helper); grouped under `conductor::support`.
- `conductor::tests` (490 functions)
  - `src/conductor.rs:2890-2940` `for_test` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13041-13060` `unit_worktree_dir_derives_deterministically_from_scratch_root_and_unit_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13063-13150` `recorded_gate_outcome_reads_the_latest_gate_run_verdict_per_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13066-13075` `verdict` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13154-13159` `started_with_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13161-13166` `integrated` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13169-13177` `prior_criterion_unit_finds_a_prior_un_integrated_units_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13180-13206` `prior_criterion_unit_never_returns_an_integrated_units_id_and_never_falls_back_to_an_older_sibling` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13209-13233` `prior_criterion_unit_integration_of_one_criterion_never_masks_an_abandoned_sibling_criterion_sharing_the_same_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13236-13249` `prior_criterion_unit_tie_break_prefers_the_most_recent_of_two_non_integrated_priors` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13252-13257` `prior_criterion_unit_excludes_this_unit_itself` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13260-13272` `prior_criterion_unit_ignores_a_different_criterion_and_an_empty_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13279-13284` `run_started_with_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13289-13295` `compensated` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13300-13305` `plain_failure` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13308-13329` `prior_criterion_unit_never_adopts_across_two_different_specs_sharing_the_same_criterion_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13332-13347` `prior_criterion_unit_still_adopts_across_two_runs_of_the_same_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13350-13368` `prior_criterion_unit_readopts_after_a_compensation_reverts_the_integration` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13371-13388` `prior_criterion_unit_a_plain_non_compensation_failure_never_reopens_an_integrated_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13395-13413` `adoption_status` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13416-13441` `recorded_adoption_ignores_a_same_identity_event_carrying_the_wrong_status` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13444-13477` `recorded_adoption_never_answers_for_a_mismatched_criterion_or_a_mismatched_spec_alone` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13480-13485` `branch_owner_returns_none_for_an_id_that_never_started` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13488-13504` `branch_owner_reads_the_most_recent_started_criterion_and_spec_for_this_bare_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13507-13522` `branch_owner_ignores_a_non_unit_started_event_even_when_it_shares_the_id_field` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13525-13538` `branch_is_foreign_is_false_when_nothing_is_recorded_or_everything_matches` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13541-13563` `branch_is_foreign_is_false_when_the_recorded_owner_has_no_criterion_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13566-13588` `branch_is_foreign_when_only_one_axis_differs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13593-13605` `find_unit_started` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13608-13693` `a_fresh_units_own_branch_adopts_a_prior_runs_un_integrated_unit_sharing_the_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13696-13760` `a_fresh_unit_never_adopts_a_criterion_whose_prior_attempt_already_integrated` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13765-13777` `run_git_test` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13780-13915` `a_halted_spawns_uncommitted_tree_is_captured_as_a_wip_commit_and_named_in_the_next_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13927-13942` `prior_failure_summary_names_only_the_halted_commit_when_it_is_the_sole_failure` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13945-13968` `prior_failure_block_names_only_the_halted_commit_when_it_is_the_sole_failure` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13971-14005` `prior_failure_block_adds_the_generic_preamble_for_review_reject_or_contradiction_alone` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14008-14035` `review_worktree_dir_and_branch_derive_from_stage_and_attempt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14147-14175` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14178-14185` `prompts_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14188-14195` `dirs_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14199-14205` `system_prompt_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14209-14211` `title_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14215-14217` `reviews_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14221-14223` `spawn_ids` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14227-14234` `spawn_count` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14238-14244` `spawned` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14250-14257` `dir_existed_when_spawned` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14260-14400` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14403-14408` `agent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14413-14419` `agent_with_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14421-14427` `gate_def` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14432-14438` `gate_def_inputs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14441-14467` `integrates_a_passing_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14470-14494` `coverage_gate_refuses_an_uncovered_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14497-14529` `planner_extends_the_dag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14532-14598` `planner_proposed_unit_with_a_coverage_criterion_runs_and_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14601-14689` `conductor_creates_one_baseline_unit_per_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14692-14763` `a_stage_needing_the_fan_out_template_becomes_ready_once_every_criterion_unit_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14766-14888` `producer_prompt_carries_the_criteria_and_plan_protocol_grounded_on_the_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14893-14895` `baseline_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14900-14927` `supersede_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14930-14979` `a_prior_runs_proposal_never_resurrects_in_a_new_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14982-15064` `planner_unit_supersedes_the_matching_baseline` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15067-15151` `a_planner_supersede_of_a_fan_out_member_still_satisfies_its_downstream_needs_edge` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15154-15190` `a_criterion_with_no_planner_unit_keeps_its_baseline` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15193-15266` `planner_refinement_split_is_still_harvested` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15269-15402` `a_same_id_re_emit_updates_the_proposed_unit_in_place` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15409-15439` `seed_refine_dag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15443-15463` `append_proposals` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15466-15539` `a_coverage_retarget_re_emit_preserves_one_unit_per_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15542-15602` `a_re_emit_naming_a_reserved_stage_never_mutates_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15605-15670` `re_folding_a_same_id_re_emit_is_idempotent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15673-15780` `a_real_split_two_distinct_ids_both_survive_the_fold` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15783-15879` `a_later_episodes_proposal_supersedes_an_earlier_episodes_planner_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15882-15939` `a_same_episode_re_seen_on_a_later_fold_still_never_self_supersedes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15942-16021` `a_planner_proposal_with_no_data_episode_field_still_supersedes_via_meta_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16024-16151` `a_same_id_refine_restamps_its_episode_so_its_own_episodes_sibling_does_not_reap_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16154-16276` `a_same_id_refine_survives_its_own_episodes_sibling_add_walked_first` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16279-16434` `a_resume_catch_up_over_two_episode_supersession_and_a_split_matches_a_live_incremental_fold` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16313-16331` `append_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16333-16345` `shape` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16437-16608` `a_legacy_history_resume_catch_up_matches_a_live_incremental_fold_mutual_siblings_then_superseded` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16458-16475` `append_legacy` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16477-16495` `append_identified` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16497-16509` `shape` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16611-16705` `a_legacy_proposal_never_supersedes_an_identified_episodes_owner_even_when_logged_later` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16708-16807` `a_late_re_emit_never_mutates_a_started_or_terminal_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16813-16822` `has_unmatched_signal` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16825-16881` `a_verbatim_copy_still_supersedes_its_baseline` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16884-16958` `a_paraphrased_proposal_matches_its_baseline_by_id_not_prose` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16961-17045` `a_bracketed_id_echo_with_a_paraphrase_still_supersedes_exactly_once` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17048-17120` `a_stale_non_matching_id_falls_back_to_the_verbatim_prose_match` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17123-17181` `a_genuinely_new_proposal_runs_and_records_an_unmatched_signal` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17184-17315` `resume_dedups_baselines_before_running_them` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17318-17399` `decomposes_the_real_spec_01_into_per_criterion_units` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17402-17440` `ratchet_promotes_a_reliable_gate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17443-17504` `elevated_gate_is_never_promoted_to_silent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17507-17549` `learns_from_escalation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17552-17602` `feeds_graph_decisions_into_the_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17605-17684` `lookup_pointer_names_all_three_verbs_on_every_slice` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17687-17782` `decisions_prompt_injection_is_capped_under_budget_with_elision_note` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17787-17809` `render_capped_decisions` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17812-17842` `a_superseded_decision_never_outranks_a_current_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17845-17898` `grounding_still_surfaces_a_prior_run_decision_that_peers_labels_historical` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17901-17928` `the_verbatim_count_cap_binds_on_many_small_decisions` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17931-17963` `the_byte_budget_cap_binds_before_the_count_on_chunky_decisions` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17966-18033` `a_kept_decision_restores_the_dropped_dependency_it_supersedes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18039-18059` `render_capped_findings` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18062-18129` `findings_prompt_injection_is_capped_under_budget_with_elision_note` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18132-18167` `the_verbatim_count_cap_binds_on_many_small_findings` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18170-18275` `grounding_omits_a_resolved_finding_and_keeps_the_open_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18281-18300` `render_capped_lessons` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18303-18360` `the_injected_slice_is_deduplicated_by_normalized_text` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18363-18505` `dedup_and_restore_are_render_only_no_event_no_projection_mutation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18523-18659` `structural_grounding_is_one_seeded_traversal_over_the_unified_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18676-18791` `the_grounding_path_populates_the_unified_graph_from_the_live_project` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18806-18895` `re_ingesting_re_extracts_a_changed_file_and_skips_unchanged_ones` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18906-18989` `ingest_files_into_graph_is_bounded_to_the_named_files_and_reflects_their_live_content` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19010-19090` `re_excluding_the_same_file_twice_in_one_process_retires_its_middle_generation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19103-19222` `a_second_run_over_an_unchanged_tree_appends_no_derived_index_event` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19253-19259` `spec60_content_identity` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19264-19268` `spec60_guarded_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19281-19309` `spec60_guard_is_judging` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19317-19335` `spec60_cold_rebuild` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19347-19378` `spec60_reachable` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19384-19393` `spec60_reached_entities` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19410-19602` `editing_one_file_between_runs_re_emits_only_that_files_batch_and_supersedes_its_edges` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19618-19795` `a_file_reverted_to_an_earlier_recorded_generation_re_ingests_and_matches_a_cold_rebuild` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19819-19952` `a_prior_runs_non_ingest_replay_key_never_suppresses_this_runs_keyed_emit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19964-20107` `the_ingest_sink_appends_and_folds_once_per_file_batch_never_once_per_event` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19974-19982` `append` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19983-19990` `read_stream` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19991-19998` `read_all` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19999-20005` `subscribe_all` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20006-20012` `subscribe_stream` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20024-20027` `apply` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20028-20031` `apply_batch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20032-20034` `subgraph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20035-20037` `resolve` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20130-20289` `design_intent_grounding_renders_the_governing_rule_and_specifying_ra_by_traversal` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20292-20362` `a_governing_decision_never_leaks_into_the_design_intent_section` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20365-20421` `the_design_intent_section_renders_the_newest_binding_and_elides_the_oldest` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20424-20464` `a_subgraph_with_no_design_intent_renders_no_design_intent_header` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20471-20497` `render_code_neighborhood` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20500-20536` `the_code_neighborhood_elision_note_names_graph_around_recovery_not_peers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20539-20584` `the_code_neighborhood_byte_cap_elides_before_the_count_cap_is_reached` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20587-20617` `a_subgraph_with_no_code_definitions_renders_no_code_neighborhood_header` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20620-20686` `lessons_prompt_injection_is_capped_under_budget_with_elision_note` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20689-20726` `the_verbatim_count_cap_binds_on_many_small_lessons` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20732-20754` `render_capped_lessons_scoped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20757-20794` `lessons_rank_by_blast_radius_relevance_over_pure_recency` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20797-20837` `resume_skips_already_integrated_units` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20841-20858` `seed_events_in_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20861-20915` `replay_trajectory_keeps_only_the_world_inputs_and_restrips_the_run_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20920-20937` `commit_on_unit_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20940-21035` `resume_reuses_a_units_branch_instead_of_reimplementing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21038-21132` `branch_gc_reclaims_integrated_units_and_retains_escalated_ones_on_resume` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21140-21156` `commit_on_named_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21171-21227` `branch_gc_falls_back_to_the_derived_branch_when_unitstarted_recorded_no_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21230-21295` `branch_gc_reclaims_the_recorded_branch_not_the_derived_name_when_they_differ` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21306-21330` `seed_lingering_worktree_on_unit_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21336-21346` `worktree_registered_on` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21349-21446` `branch_gc_removes_a_lingering_worktree_before_reclaiming_the_branch_on_resume` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21449-21541` `branch_gc_reclaims_every_integrated_unit_in_one_resume_not_just_the_first` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21544-21625` `branch_gc_fences_reclaim_behind_an_in_flight_straggler_spawn_and_reclaims_once_it_answers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21628-21695` `gc_integrated_branches_logged_prints_kept_evidence_for_an_in_flight_straggler_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21698-21797` `gc_integrated_branches_logged_prints_removing_evidence_for_a_terminal_spawns_decision` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21800-21883` `gc_integrated_branches_logged_stays_silent_for_an_already_gone_worktree_but_still_reclaims_an_orphaned_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21886-21989` `gc_integrated_branches_logged_does_not_repeat_removing_evidence_once_the_real_removal_already_happened` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21993-22015` `sha_stamp_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22018-22069` `review_boundary_events_carry_the_worktree_sha` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22072-22121` `a_review_reject_unitfailed_carries_the_worktree_sha` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22124-22168` `an_exhaustive_gate_failure_on_an_approved_unit_records_a_gate_cause` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22171-22296` `stamps_the_model_alias_and_resolved_id_on_live_lifecycle_events` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22302-22322` `degenerate_reviewer_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22324-22329` `has_status` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22332-22401` `a_degenerate_adjudicator_result_respawns_and_a_substantive_retry_folds_normally` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22404-22512` `a_review_stage_error_result_re_parks_a_fresh_attempt_no_charge_then_a_real_verdict_folds` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22515-22646` `a_review_spawn_that_errors_on_every_re_parked_attempt_escalates_through_the_bound` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22649-22742` `a_gating_spawn_that_emits_an_approve_verdict_but_returns_no_verdict_line_hard_errors_with_the_result_channel_fix` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22745-22790` `a_gating_spawn_that_returns_a_reject_verdict_line_is_a_normal_reject_even_if_it_emitted_approve` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22793-22837` `a_gating_spawn_with_no_verdict_line_and_no_emitted_approve_is_an_ordinary_reject` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22840-22945` `the_workflow_live_path_correlates_the_approve_by_stamp_not_a_bare_stream_position` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22948-23001` `a_degenerate_lens_result_respawns_the_lens_before_the_review_proceeds` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23004-23064` `a_lens_that_emitted_a_finding_but_reports_empty_stdout_is_not_degenerate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23067-23164` `a_reviewer_that_only_ever_returns_degenerate_output_halts_the_run_loudly_naming_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23167-23252` `resume_integrates_an_already_approved_unit_without_re_reviewing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23255-23366` `a_failed_unit_is_not_terminal_and_resumes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23369-23484` `remediation_attempts_accumulate_across_resume_and_escalate_at_the_bound` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23487-23572` `an_escalated_unit_stays_terminal_on_resume` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23575-23630` `a_fresh_unit_with_no_branch_runs_the_full_lifecycle` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23633-23688` `agent_decision_folds_content_but_no_agent_attribution` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23691-23733` `scope_creep_refuses_a_criterionless_proposed_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23736-23769` `adjudicator_reject_blocks_the_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23772-23838` `adversary_runs_between_the_lenses_and_the_adjudicator` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23841-23891` `adjudicator_reject_gates_even_with_an_adversary_present` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23894-23987` `unit_reviews_itself_within_its_own_lifecycle` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23990-24000` `depth_tiers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24004-24011` `full_panel_with_tiers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24013-24015` `strs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24018-24044` `path_is_high_risk_matches_by_prefix_and_by_glob` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24047-24056` `route_review_tier_with_no_policy_is_the_full_panel_unchanged` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24059-24078` `route_review_tier_routes_a_low_risk_unit_to_the_light_panel` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24081-24096` `route_review_tier_forces_full_on_a_high_risk_path_hit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24099-24112` `route_review_tier_forces_full_over_the_size_threshold` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24115-24128` `route_review_tier_forces_full_when_the_gates_flapped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24131-24165` `route_review_tier_fails_safe_to_full_on_an_empty_blast_radius` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24171-24223` `run_tiered_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24227-24238` `logged_review_tier` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24241-24267` `a_low_risk_unit_runs_the_light_panel_and_logs_the_routing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24270-24330` `a_low_risk_unit_skips_the_adversary_and_extra_lens` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24333-24403` `a_high_risk_unit_runs_the_full_panel_and_logs_the_routing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24406-24421` `without_a_depth_policy_a_grounded_unit_runs_the_full_panel_and_logs_no_routing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24424-24502` `a_zero_grounding_unit_fails_safe_to_the_full_panel_and_logs_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24505-24614` `a_flapped_unit_escalates_from_light_at_attempt_0_to_full_on_remediation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24617-24681` `a_stage_level_tiers_policy_routes_the_unit_by_risk` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24684-24766` `every_spawn_runs_in_a_worktree_never_the_main_repo_checkout` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24770-24793` `spec_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24796-24888` `speculation_width_2_runs_two_candidates_first_green_wins_rest_cancelled` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24891-24927` `speculation_budget_accounts_for_every_candidate_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24930-24971` `speculation_defaults_off_runs_a_single_candidate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24974-25014` `speculation_parks_all_candidates_together_in_one_step` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25017-25139` `speculation_defers_green_until_the_winner_and_integrates_across_replay_steps` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25145-25155` `unit_has_status` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25159-25172` `branch_present` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25175-25249` `speculation_later_candidate_wins_when_an_earlier_lane_is_review_rejected` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25252-25370` `speculation_low_risk_later_lane_winner_routes_light_not_falsely_flapped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25373-25429` `speculation_rejected_loser_is_visible_to_the_review_quality_fold` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25432-25499` `speculation_escalates_when_every_candidate_is_rejected` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25502-25567` `speculation_crashed_lane_is_absent_and_a_sibling_still_wins` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25570-25637` `speculation_exhaustive_integrate_door_red_blocks_a_candidate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25640-25747` `speculation_stays_fresh_across_parking_until_a_winner_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25762-25829` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25833-25999` `speculation_blocked_winner_captures_evidence_and_a_later_lane_wins` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26002-26056` `assert_isolated_cwd_refuses_empty_or_repo_root_with_a_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26059-26150` `the_conductor_threads_each_agents_persona_to_the_driver` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26153-26207` `the_system_prompt_carries_the_rigger_communication_discipline` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26210-26250` `an_agent_with_no_persona_threads_an_empty_system_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26253-26315` `planner_proposed_unit_inherits_the_default_review_panel` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26318-26413` `a_producer_stage_skips_the_three_tier_review_and_unblocks_its_dependents` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26416-26498` `plan_stage_commit_under_specs_reaches_the_run_branch_before_the_next_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26501-26576` `plan_stage_commit_outside_specs_fails_the_stage_naming_the_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26579-26663` `plan_stage_commit_reverting_its_own_out_of_scope_touch_still_fails_the_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26666-26737` `plan_stage_commit_conflicting_with_a_concurrent_specs_change_escalates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26740-26873` `integrate_plan_commits_is_idempotent_on_a_resumed_already_landed_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26876-26937` `integrate_plan_commits_tolerates_a_pre_existing_intent_record_with_no_git_mutation_yet` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26940-27026` `integrate_plan_commits_keeps_the_earlier_commits_identity_when_the_worktree_grows_between_calls` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27039-27055` `append` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27056-27063` `read_stream` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27064-27071` `read_all` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27072-27078` `subscribe_all` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27079-27085` `subscribe_stream` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27089-27147` `integrate_plan_commits_wraps_any_hard_error_with_the_plan_landing_marker` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27150-27232` `a_plan_landing_infra_fault_halts_the_run_loudly_with_no_per_unit_lesson_or_attempt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27235-27307` `per_unit_adjudicator_reject_blocks_integration_and_escalates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27310-27403` `an_always_rejecting_adjudicator_escalates_after_exactly_max_retries_cycles` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27406-27503` `a_higher_max_retries_gives_more_attempts_before_escalation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27420-27466` `escalation_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27506-27570` `an_absent_max_retries_preserves_the_default_bound_of_three` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27573-27677` `a_resumed_unit_gets_exactly_its_granted_extra_attempts_before_re_escalating` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27680-27721` `max_retries_for_widens_only_the_resumed_unit_never_a_sibling` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27724-27771` `a_stages_own_max_retries_overrides_the_run_default_for_its_units` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27789-27795` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27798-27831` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27835-27908` `approval_on_the_final_permitted_attempt_integrates_a_per_unit_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27911-28001` `a_model_ladder_implementer_escalates_one_rung_per_remediation_attempt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28004-28070` `approval_on_the_final_permitted_attempt_integrates_a_standalone_review_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28073-28107` `mid_spawn_crash_escalates_without_aborting_the_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28110-28161` `a_newly_escalated_unit_stamps_an_attention_entry` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28164-28214` `a_budget_halt_stamps_an_attention_entry` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28217-28306` `a_budget_halt_does_not_restamp_on_a_later_poll_with_nothing_new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28309-28413` `a_delayed_budget_halt_after_a_dependency_unlocks_still_stamps` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28416-28487` `compute_attention_leaves_the_hung_liveness_halt_to_the_caller` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28490-28571` `an_escalation_does_not_restamp_attention_on_a_resumed_process` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28574-28607` `a_clean_step_stamps_no_attention` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28610-28734` `a_second_failure_recurs_and_a_third_also_stalls_the_frontier` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28737-28784` `nine_of_ten_budget_crosses_the_final_tenth` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28787-28874` `a_re_step_already_past_the_budget_threshold_does_not_re_stamp` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28877-28928` `budget_breaker_stops_the_run_after_the_first_wave` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28931-28974` `budget_exhaustion_aborts_the_task` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28977-29015` `a_budget_halt_surfaces_its_reason_on_the_run_state` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29018-29050` `a_run_within_budget_surfaces_no_halt_reason` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29053-29121` `the_spawn_budget_folds_from_recorded_spawn_requests_across_steps` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29124-29165` `the_pre_wave_breaker_trips_only_on_a_new_over_budget_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29168-29238` `a_review_tier_budget_refusal_aborts_with_budgetexhausted_not_a_raw_error` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29241-29273` `coverage_gap_flags_a_spec_defect_and_errors` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29276-29317` `planner_covering_every_criterion_passes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29320-29354` `planner_leaving_a_gap_flags_a_spec_defect` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29357-29391` `gate_only_stage_is_a_coverage_proxy_gap` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29394-29453` `manual_stage_pauses_while_an_auto_stage_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29456-29499` `isolation_none_agent_gets_no_worktree_even_with_a_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29502-29535` `spawn_opts_isolation_is_set_for_a_worktree_agent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29538-29577` `a_spawned_implementers_title_is_the_unit_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29584-29632` `the_adversarys_spawn_is_stamped_with_the_units_lens_roster` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29638-29687` `the_adjudicators_spawn_is_stamped_with_lenses_plus_adversary` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29692-29739` `a_panel_with_no_adversary_never_fabricates_one_in_the_adjudicators_roster` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29748-29811` `the_roster_reflects_the_actually_routed_light_panel_not_the_full_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29825-29882` `the_fan_out_review_loops_adversary_and_adjudicator_spawns_are_stamped_with_the_routed_roster` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29885-29955` `stage_autonomy_override_seeds_the_gate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29958-30005` `on_pass_none_runs_gates_but_does_not_integrate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30016-30031` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30035-30122` `two_units_gate_environments_never_share_a_target_dir` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30125-30191` `two_units_gate_environments_never_share_a_mutants_root` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30194-30245` `an_implement_stage_gate_round_creates_no_mutants_directory` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30254-30258` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30259-30261` `envs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30264-30273` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30281-30408` `one_build_environment_authority_reaches_both_a_gate_build_and_an_agent_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30296-30333` `run_once` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30411-30455` `spawn_env_adds_the_per_unit_cargo_target_dir_only_for_a_real_unit_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30458-30521` `a_units_per_unit_cache_is_reclaimed_when_its_worktree_is_removed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30524-30596` `a_units_worktree_cache_and_branch_are_all_reclaimed_on_a_successful_integrate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30599-30659` `a_units_worktree_is_reclaimed_but_its_branch_survives_a_terminal_escalation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30662-30768` `a_parked_review_spawn_keeps_the_unit_worktree_registered_and_its_cache` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30771-30863` `review_unit_restores_a_worktree_a_gate_deleted_out_of_band` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30866-30968` `a_second_step_restores_a_still_parked_units_worktree_deleted_out_of_band` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30971-31069` `verified_worktree_sha_is_stamped_after_a_gate_side_deletion_is_restored` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31072-31150` `review_tier_boundary_restores_a_worktree_a_prior_tier_deleted` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31153-31267` `reviewed_worktree_sha_is_stamped_after_the_adjudicators_own_deletion_is_restored` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31270-31337` `failed_worktree_sha_is_stamped_after_the_adjudicators_own_deletion_is_restored` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31340-31443` `a_resumed_reviewed_unit_restores_a_worktree_the_exhaustive_gate_deleted` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31446-31531` `a_resumed_reviewed_unit_whose_exhaustive_gate_fails_records_a_gate_cause` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31534-31672` `a_resumed_reviewed_units_merge_break_records_an_integrate_conflict_cause` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31675-31856` `a_resumed_reviewed_units_genuine_unresolved_conflict_reaches_the_idempotent_merge_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31859-31933` `a_live_approved_unit_restores_a_worktree_the_integrate_door_exhaustive_gate_deleted` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31936-32053` `speculation_lenses_restore_a_gate_deleted_worktree_without_racing_and_stamp_the_winner_sha` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32056-32127` `speculation_reject_worktree_sha_is_stamped_after_the_adjudicators_own_deletion_is_restored` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32130-32250` `a_parked_lens_keeps_the_unit_worktree_beside_a_genuine_sibling_crash` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32253-32298` `a_worktree_less_stage_gate_inherits_the_shared_target` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32301-32338` `bounded_pool_completes_every_stage_under_the_cap` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32341-32387` `live_run_folds_no_gate_machinery` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32390-32433` `agent_stage_runs_the_per_unit_lifecycle_not_the_fan_out_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32436-32477` `standalone_review_stage_still_takes_the_fan_out_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32480-32592` `run_gates_derives_and_injects_the_review_worktrees_store_fence` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32595-32648` `a_parked_lens_keeps_the_standalone_review_stages_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32651-32747` `a_parked_lens_keeps_the_review_worktree_even_beside_a_lower_indexed_sibling_crash` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32750-32843` `a_parked_lens_keeps_the_review_worktree_beside_a_sibling_degenerate_halt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32846-32931` `the_swap_to_front_prioritizes_a_genuine_error_at_a_non_zero_chunk_index` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32934-33021` `a_budget_refused_lens_beside_a_genuinely_crashing_sibling_in_one_chunk` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33024-33091` `a_budget_refused_standalone_review_spawn_keeps_its_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33094-33121` `partition_separates_overlapping_blast_radii` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33124-33146` `blast_radius_conflicts_flags_units_that_split_one_file_set` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33149-33162` `blast_radius_conflicts_is_empty_for_a_disjoint_partition` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33165-33213` `partitioned_wave_still_integrates_every_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33225-33249` `run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33253-33362` `lens_finding_reaches_later_tiers_through_the_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33365-33426` `review_agents_emit_findings_via_the_review_protocol` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33429-33474` `unparseable_adjudicator_output_blocks_integration` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33477-33538` `failing_gate_evidence_threaded_into_retry_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33541-33624` `a_flaky_gate_rerun_is_a_pass_with_warning_that_never_demotes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33627-33697` `a_product_gate_failure_is_not_rerun_and_demotes_as_before` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33700-33778` `an_authored_infra_rule_with_limit_zero_at_an_inline_gate_holds_the_ratchet` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33781-33876` `an_inline_gate_red_across_every_rerun_holds_for_infra_and_demotes_for_flaky` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33879-33918` `unit_evidence_is_populated_after_a_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33921-33996` `adjudicator_rejection_reasoning_threaded_into_retry_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33999-34066` `fan_out_lens_does_not_integrate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34069-34121` `review_only_stage_records_no_artifact_truthfully` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34124-34182` `two_erroring_stages_both_leave_a_record` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34185-34249` `single_wide_wave_overruns_budget_and_is_stopped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34252-34298` `validate_acyclic_detects_a_cycle` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34349-34362` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34365-34370` `materializing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34374-34379` `deleting_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34380-34382` `calls` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34383-34385` `targets` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34386-34388` `mutants_dirs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34389-34391` `build_envs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34392-34394` `store_fences` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34395-34397` `build_cache_guards` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34398-34400` `build_cache_dirs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34403-34453` `run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34458-34480` `content_cache_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34484-34490` `attempt_of` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34502-34539` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34550-34555` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34556-34558` `calls` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34561-34581` `run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34585-34594` `gate_verdict_event` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34596-34600` `verdict_passed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34603-34669` `a_matching_input_digest_answers_a_gate_as_a_logged_cache_hit_citing_the_prior_green` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34672-34714` `a_content_change_misses_the_cache_and_re_runs_the_gate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34717-34773` `a_red_verdict_is_never_cache_answered_and_the_gate_re_runs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34782-34794` `ground` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34806-34821` `ground` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34822-34824` `blast_radius` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34825-34827` `index_stamp` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34831-34850` `blast_radius_audits` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34853-34862` `review_tier_evidence` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34867-34890` `tiered_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34892-34899` `tiered_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34909-34982` `a_structural_grounder_records_the_audit_and_routes_full_on_a_beyond_cap_high_risk_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34989-35029` `the_empty_radius_fail_safe_still_records_the_audit_and_routes_full` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35037-35071` `a_non_structural_grounder_records_no_blast_radius_audit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35083-35148` `confidence_tier_fixture` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35159-35210` `confidence_tier_radius_splits_the_one_edge_set_into_extracted_precise_and_all_tier_safe` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35220-35311` `grounded_blast_radius_tier_filters_the_subgraph_and_keeps_the_grep_superset` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35226-35228` `apply` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35229-35235` `subgraph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35236-35238` `resolve` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35334-35427` `under_symbols_the_audit_emits_and_records_the_prompt_seed_as_precise` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35439-35513` `partition_wave_own_batches_an_empty_radius_never_co_scheduling_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35522-35586` `speculation_over_a_structural_grounder_records_the_audit_and_routes_full` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35589-35640` `staleness_flags_only_downstream_units_whose_radius_intersects_the_touched_files` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35643-35695` `staleness_grounds_on_the_safe_superset_so_a_grep_only_reference_still_stales_a_downstream_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35698-35796` `rule6_conflict_detection_grounds_on_the_safe_superset_so_a_grep_only_shared_reference_conflicts` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35799-35829` `a_resume_reseeds_the_stale_set_from_the_prior_unitintegrated_marks` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35838-35879` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35883-36008` `integrating_a_unit_stales_the_intersecting_downstream_units_cached_verdict_not_the_rest` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36011-36037` `glob_matches_supports_star_doublestar_and_literals` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36040-36064` `gate_intersects_radius_runs_unscoped_and_intersecting_gates_only` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36067-36216` `blast_radius_narrows_the_inner_loop_and_the_integrate_step_runs_the_full_library` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36219-36347` `a_gate_skipped_inline_but_red_at_the_exhaustive_integrate_door_blocks_the_merge` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36350-36379` `verdict_compensates_names_a_prior_unit_independent_of_approval` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36392-36434` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36438-36613` `a_contradiction_compensates_reverts_and_re_enters_the_integrated_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36616-36667` `commits_to_compensate_reverts_every_sha_a_multi_commit_landing_recorded` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36670-36729` `commits_to_compensate_dedupes_a_repeated_sha_and_skips_an_already_compensated_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36732-36783` `commits_to_compensate_excludes_the_review_only_marker_even_alongside_a_real_sha` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36786-36846` `pending_compensations_from_log_re_derives_only_undrained_marks` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36849-36880` `a_durable_compensation_queued_mark_is_fold_neutral_on_the_target` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36888-36925` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36929-37012` `a_compensated_unit_re_gates_its_re_implemented_tree_not_the_condemned_verdict` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37023-37068` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37072-37170` `a_compensated_unit_that_remediated_before_integrating_re_gates_at_a_fresh_key` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37180-37204` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37208-37343` `a_resume_re_drives_a_durably_queued_but_undrained_compensation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37363-37411` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37415-37597` `integrate_re_gates_the_merged_tree_and_a_merge_break_blocks_the_second_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37620-37687` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37691-37798` `the_post_merge_re_gate_gets_the_units_mutants_root_though_it_runs_in_the_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37801-37921` `conflict_regenerate_pending_from_log_re_derives_the_union_keyed_by_unit_and_attempt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37929-37955` `conflict_fixture` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37958-38044` `integrate_conflict_re_parks_the_implementer_with_no_attempt_charged_and_both_units_land` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:38047-38127` `integrate_conflict_confined_to_a_regenerable_path_resolves_with_no_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:38130-38234` `integrate_conflict_exhausted_after_the_bound_charges_a_real_attempt_with_the_unresolved_evidence` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:38237-38395` `integrate_conflict_records_regenerate_pending_before_the_accept_incoming_mutation_that_can_fail` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:38277-38320` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:38398-38472` `a_deferred_gate_runs_once_at_the_phase_boundary_not_inline` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:38475-38543` `a_failing_deferred_gate_is_surfaced_and_the_run_is_not_done` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:38554-38577` `run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:38581-38652` `a_default_infra_fault_at_a_deferred_gate_does_not_demote` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:38655-38759` `a_replayed_step_re_runs_no_recorded_gate_and_appends_no_duplicate_events` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:38762-38847` `a_re_step_replays_a_recorded_deferred_gate_without_re_running_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:38850-38994` `a_parked_step_never_records_a_partial_tree_deferred_verdict` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:38997-39081` `a_manual_review_paused_unit_defers_the_whole_tree_gate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39084-39217` `an_escalated_dep_still_runs_the_whole_tree_deferred_gate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39220-39334` `a_recorded_failing_deferred_verdict_re_surfaces_its_failure_on_replay` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39337-39423` `a_replayed_fan_out_review_reject_appends_no_duplicate_unitfailed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39426-39474` `a_fan_out_review_stage_whose_gates_fail_after_approval_records_a_gate_cause` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39480-39494` `run_git` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39498-39506` `git_head` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39508-39525` `init_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39536-39567` `run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39571-39630` `gate_measures_the_committed_artifact_not_the_dirty_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39633-39759` `conductor_spawns_the_sdet_author_at_the_build_seam_so_its_tests_land_in_the_committed_tree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39762-39852` `the_sdet_author_spawn_respects_the_budget_breaker_at_its_own_spawn_site` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39855-39957` `a_parked_sdet_author_spawn_holds_the_unit_instead_of_integrating_without_its_periphery_tests` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39960-40072` `speculation_sdet_author_periphery_lands_in_the_committed_tree_the_gates_judge` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40075-40172` `a_parked_sdet_author_in_a_speculation_candidate_holds_the_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40175-40250` `an_empty_accounting_sdet_author_result_advances_the_build_to_commit_gates_and_integrate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40253-40317` `an_absent_sdet_author_is_a_clean_no_op_and_the_build_still_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40320-40395` `a_crashed_sdet_author_spawn_does_not_block_the_build_the_lifecycle_proceeds` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40398-40498` `an_escalated_unit_does_not_integrate_its_code` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40506-40545` `critique_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40574-40585` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40588-40593` `rejecting` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40596-40601` `always_rejecting` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40602-40609` `count` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40612-40662` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40666-40735` `a_blast_radius_overlap_alone_does_not_reject` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40748-40792` `the_plan_critique_gates_adversary_and_adjudicator_spawns_are_stamped_correctly` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40795-40832` `a_rule_7_or_8_defect_rejects_then_releases_on_the_revision` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40835-40886` `a_clean_decomposition_approves_and_releases_the_fan_out` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40889-40964` `the_gate_critiques_only_not_yet_run_units_and_approves` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40967-41004` `the_plan_critique_prompt_names_the_cross_unit_rules` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:41007-41032` `the_critique_gate_never_enters_a_wave_even_when_ready` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:41041-41075` `fan_out_needs_template_fixture` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:41078-41160` `a_downstream_stage_needing_the_fan_out_template_stays_unready_until_every_member_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:41163-41267` `a_real_split_pair_must_both_integrate_not_just_the_btreemap_key_first_sibling` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:41270-41344` `a_resolved_plan_critique_gate_does_not_re_run_on_resume` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:41347-41445` `a_resumed_step_holds_the_fan_out_while_the_plan_critique_gate_is_escalated` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:41448-41504` `an_approved_gate_releases_planner_proposed_units_not_only_baselines` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:41507-41574` `the_re_plan_directive_instructs_reusing_the_existing_unit_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:41587-41592` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:41593-41600` `count` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:41603-41623` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:41627-41698` `a_resumed_step_holds_the_fan_out_while_the_plan_critique_gate_is_mid_review` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:41701-41755` `a_parked_plan_critique_gate_keeps_its_review_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:41758-41808` `a_budget_refused_plan_critique_gate_keeps_its_review_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:41821-41848` `the_conductors_one_event_authority_reports_a_write_the_store_lost` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
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
  - `src/main.rs:2191-2199` `cmd_run` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:2303-2385` `cmd_resume_unit` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:2387-2950` `cmd_step` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:3180-3212` `cmd_reported` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:3230-3246` `cmd_prompt` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:3270-3301` `cmd_scratch` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:3928-3935` `cmd_serve` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:3980-4049` `cmd_workflow` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:4118-4170` `cmd_graph` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:4293-4313` `cmd_graph_show` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:4617-4700` `cmd_graph_build` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:4721-4780` `cmd_graph_communities` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:4800-4859` `cmd_graph_concepts` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:4878-4916` `cmd_stats` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:5240-5250` `cmd_stats_canary` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:5631-5754` `cmd_canary` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:5764-5800` `cmd_playbooks` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:5833-5985` `cmd_replay` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:6821-7131` `cmd_dash` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:7592-7628` `cmd_ground` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:7644-7665` `cmd_reindex` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:7682-7710` `cmd_symbols_index` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:7718-7794` `cmd_emit` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:7805-7839` `cmd_progress` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:7891-8049` `cmd_status` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:8152-8183` `cmd_watch` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:8439-8520` `cmd_reset` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:9223-9263` `cmd_peers` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:9470-9566` `cmd_result` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:9702-9889` `cmd_validate` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:11977-11996` `cmd_init` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:12644-12798` `cmd_setup` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:13142-13147` `cmd_docs` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:13287-13321` `cmd_prime` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:13413-13432` `cmd_mcp` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:13687-13724` `cmd_grep_guard` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
- `main::dash_glue` (25 functions)
  - `src/main.rs:111-115` `record_dash_attempt` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6331-6333` `dash_marker_serving` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6413-6433` `spawn_dash_child_process` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6458-6479` `wait_for_dash_bind` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6506-6539` `wait_for_dash_bind_or_diagnose` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6692-6694` `dash_ensure_suppressed` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6701-6703` `dash_ensure_port` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6709-6712` `dash_ensure_port_from` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6773-6777` `recorded_dash_url` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6787-6801` `dash_status_line` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6810-6819` `dash_status_json` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7138-7145` `dash_reap_poll` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7154-7162` `dash_reap_idle_window` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7277-7296` `dash_read_run` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7302-7314` `dash_read_graph` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7325-7333` `dash_read_whole_graph` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7342-7359` `dash_read_calls` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7365-7377` `dash_attach_calls` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7382-7399` `dash_read_progress` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7405-7432` `dash_read_liveness` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7455-7471` `dash_resolve_attach` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7496-7506` `dash_read_sqlite_stream_readonly` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7508-7544` `dash_attach_run` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7551-7563` `dash_attach_inputs` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7569-7575` `dash_attach_graph` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
- `main::docs_overlay` (1 function)
  - `src/main.rs:12130-12137` `apply` - method inside `impl DocsOverlay`; grouped with its other `DocsOverlay` methods.
- `main::liveness` (9 functions)
  - `src/main.rs:3004-3014` `live_branches_for_sweep` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:3045-3061` `terminal_and_no_live_worker` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:7855-7881` `liveness_ages_for_wave` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:8919-8957` `live_writer_reasons` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:8963-8975` `live_writer_refusal` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:9139-9149` `superseded_edge_boundary` - name contains "superseded" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:9171-9201` `superseded_graph_nodes` - name contains "superseded" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:10680-10687` `live_slugs` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:10908-10926` `worktree_belongs_to_live` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
- `main::provenance` (23 functions)
  - `src/main.rs:200-202` `workflow_path` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:3527-3569` `refuse_when_base_lacks_spec_paths` - name contains "spec_" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:3782-3926` `run_workflow` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:3941-3972` `parse_workflow_args` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:9695-9700` `spec_lint_warning_lines` - name contains "spec_" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:10138-10143` `installed_workflow_drifted` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:10150-10152` `workflow_provenance_path` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:10157-10165` `installed_workflow_provenance` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:10184-10195` `git_is_ancestor` - name contains "git_" (git/version provenance); grouped under `main::provenance`.
  - `src/main.rs:10203-10213` `git_commit_distance` - name contains "git_" (git/version provenance); grouped under `main::provenance`.
  - `src/main.rs:10340-10374` `workflow_drift_advisory` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:12063-12073` `install_workflow` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:12102-12104` `docs_overlay_path` - name contains "docs_" (docs overlay); grouped under `main::provenance`.
  - `src/main.rs:12145-12154` `read_docs_overlay` - name contains "docs_" (docs overlay); grouped under `main::provenance`.
  - `src/main.rs:12479-12500` `git_hooks_dir` - name contains "git_" (git/version provenance); grouped under `main::provenance`.
  - `src/main.rs:13062-13106` `docs_context` - name contains "docs_" (docs overlay); grouped under `main::provenance`.
  - `src/main.rs:13162-13181` `docs_drift` - name contains "docs_" (docs overlay); grouped under `main::provenance`.
  - `src/main.rs:13190-13206` `docs_drift_failure` - name contains "docs_" (docs overlay); grouped under `main::provenance`.
  - `src/main.rs:13228-13234` `spec_lint_next_step` - name contains "spec_" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:13255-13259` `spec_lint_reminder_suppressed` - name contains "spec_" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:13267-13279` `spec_lint_reminder_should_print` - name contains "spec_" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:13362-13365` `git_repo` - name contains "git_" (git/version provenance); grouped under `main::provenance`.
  - `src/main.rs:13373-13383` `git_repo_at` - name contains "git_" (git/version provenance); grouped under `main::provenance`.
- `main::render` (10 functions)
  - `src/main.rs:4202-4269` `print_around_subgraph` - name contains "print_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:4323-4361` `print_entity_site` - name contains "print_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:4370-4382` `print_site_header` - name contains "print_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:5013-5055` `format_stats` - name contains "format_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:5101-5117` `format_progress_line` - name contains "format_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:5279-5375` `format_canary_stats` - name contains "format_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:6181-6191` `format_stats_diff` - name contains "format_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:11141-11171` `format_residue` - name contains "format_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:11823-11835` `print_orientation` - name contains "print_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:13726-13745` `print_run_state` - name contains "print_" (human-readable output); grouped under `main::render`.
- `main::replay_runner` (1 function)
  - `src/main.rs:6202-6222` `run` - method inside `impl Runner for ReplayRunner`; grouped with its other `ReplayRunner` methods.
- `main::residue_report` (1 function)
  - `src/main.rs:10451-10456` `is_empty` - method inside `impl ResidueReport`; grouped with its other `ResidueReport` methods.
- `main::run_registration` (2 functions)
  - `src/main.rs:760-765` `inert` - method inside `impl RunRegistration`; grouped with its other `RunRegistration` methods.
  - `src/main.rs:769-776` `drop` - method inside `impl Drop for RunRegistration`; grouped with its other `RunRegistration` methods.
- `main::scaffold_report` (1 function)
  - `src/main.rs:11679-11685` `changed` - method inside `impl ScaffoldReport`; grouped with its other `ScaffoldReport` methods.
- `main::setup` (20 functions)
  - `src/main.rs:2003-2006` `scratch_defaults` - name contains "scratch" (project setup); grouped under `main::setup`.
  - `src/main.rs:4064-4082` `locate_shim` - name contains "shim" (project setup); grouped under `main::setup`.
  - `src/main.rs:7181-7185` `foreign_instance_scratch_root` - name contains "scratch" (project setup); grouped under `main::setup`.
  - `src/main.rs:11242-11262` `scratch_totals` - name contains "scratch" (project setup); grouped under `main::setup`.
  - `src/main.rs:11424-11459` `classify_agent_scratch` - name contains "scratch" (project setup); grouped under `main::setup`.
  - `src/main.rs:11807-11814` `print_scaffold_pointer` - name contains "scaffold" (project setup); grouped under `main::setup`.
  - `src/main.rs:11944-11975` `scaffold_summary_lines` - name contains "scaffold" (project setup); grouped under `main::setup`.
  - `src/main.rs:12002-12004` `shim_dir` - name contains "shim" (project setup); grouped under `main::setup`.
  - `src/main.rs:12032-12049` `install_file_if_changed` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/main.rs:12081-12083` `skill_source_rel` - name contains "skill" (project setup); grouped under `main::setup`.
  - `src/main.rs:12092-12097` `skill_install_path` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/main.rs:12167-12180` `install_skills` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/main.rs:12512-12540` `install_precommit_hook` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/main.rs:12557-12565` `provision_shim` - name contains "shim" (project setup); grouped under `main::setup`.
  - `src/main.rs:12579-12588` `shim_is_current` - name contains "shim" (project setup); grouped under `main::setup`.
  - `src/main.rs:12593-12600` `write_shim_files` - name contains "shim" (project setup); grouped under `main::setup`.
  - `src/main.rs:12607-12635` `run_npm_install` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/main.rs:12823-12839` `install_lookup_hook` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/main.rs:12848-12862` `install_operator_mcp` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/main.rs:12874-12890` `parse_setup_args` - name contains "setup" (project setup); grouped under `main::setup`.
- `main::store` (33 functions)
  - `src/main.rs:473-491` `store_conn_file` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:548-561` `store_backend_kind` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:611-677` `store_selection_at` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:685-691` `store_selection` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:719-732` `registry_store_identity` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:1249-1320` `migrate_project_identity` - name contains "migrate" (store hygiene); grouped under `main::store`.
  - `src/main.rs:1329-1334` `migrate_local_identity` - name contains "migrate" (store hygiene); grouped under `main::store`.
  - `src/main.rs:1346-1372` `migrate_identity_at` - name contains "migrate" (store hygiene); grouped under `main::store`.
  - `src/main.rs:1770-1772` `find_store_dir_from` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:2017-2022` `server_store_location` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:2042-2108` `require_store_dir` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:2112-2114` `store_file` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:3107-3113` `reclaim_run_scratch` - name contains "reclaim_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:8539-8565` `reset_menu` - name contains "reset_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:8634-8674` `reset_modes` - name contains "reset_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:8701-8720` `reset_scratch_orphans` - name contains "reset_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:8722-8741` `reset_build_cache` - name contains "reset_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:8746-8759` `build_cache_reclaim_report` - name contains "reclaim_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:8787-8795` `reset_derived` - name contains "reset_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:9006-9067` `refuse_derived_reset_if_live` - name contains "reset_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:9094-9125` `reset_runs` - name contains "reset_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:9599-9614` `reclaim_spawn_scratch` - name contains "reclaim_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:9641-9658` `reclaim_spawn_registered_scratch` - name contains "reclaim_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:10061-10071` `bloat_advisory` - name contains "bloat" (store hygiene); grouped under `main::store`.
  - `src/main.rs:10084-10095` `bloat_advisory_for` - name contains "bloat" (store hygiene); grouped under `main::store`.
  - `src/main.rs:10715-10804` `reclaim_orphan_scratch` - name contains "reclaim_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:11079-11120` `reclaim_shared_build_cache` - name contains "reclaim_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:11279-11322` `scratch_footprint` - name contains "footprint" (store hygiene); grouped under `main::store`.
  - `src/main.rs:11332-11355` `store_and_backup_bytes` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:11476-11553` `footprint_report` - name contains "footprint" (store hygiene); grouped under `main::store`.
  - `src/main.rs:11559-11564` `footprint_report_lines` - name contains "footprint" (store hygiene); grouped under `main::store`.
  - `src/main.rs:11573-11593` `footprint_advisories` - name contains "footprint" (store hygiene); grouped under `main::store`.
  - `src/main.rs:11624-11651` `footprint_report_for` - name contains "footprint" (store hygiene); grouped under `main::store`.
- `main::store_location` (3 functions)
  - `src/main.rs:1946-1948` `file` - method inside `impl StoreLocation`; grouped with its other `StoreLocation` methods.
  - `src/main.rs:1954-1961` `identity` - method inside `impl StoreLocation`; grouped with its other `StoreLocation` methods.
  - `src/main.rs:1974-1980` `repo_root` - method inside `impl StoreLocation`; grouped with its other `StoreLocation` methods.
- `main::store_selection` (1 function)
  - `src/main.rs:434-436` `is_sqlite` - method inside `impl StoreSelection`; grouped with its other `StoreSelection` methods.
- `main::support` (27 functions)
  - `src/main.rs:321-397` `parse_run_args` - name contains "parse_" (argument/input parsing); grouped under `main::support`.
  - `src/main.rs:408-416` `resolve_run_base` - name contains "resolve" (path/id resolution helper); grouped under `main::support`.
  - `src/main.rs:498-500` `conn_file_is_group_or_other_readable` - name contains "is_" (predicate helper); grouped under `main::support`.
  - `src/main.rs:508-530` `warn_if_conn_file_is_exposed` - name contains "is_" (predicate helper); grouped under `main::support`.
  - `src/main.rs:568-591` `resolve_conn` - name contains "resolve" (path/id resolution helper); grouped under `main::support`.
  - `src/main.rs:699-709` `resolve_store` - name contains "resolve" (path/id resolution helper); grouped under `main::support`.
  - `src/main.rs:979-988` `read_project_id` - name contains "read_" (generic read helper); grouped under `main::support`.
  - `src/main.rs:1818-1841` `resolve_main_worktree_or_refuse` - name contains "resolve" (path/id resolution helper); grouped under `main::support`.
  - `src/main.rs:3359-3402` `parse_step_args` - name contains "parse_" (argument/input parsing); grouped under `main::support`.
  - `src/main.rs:5385-5397` `read_model_drift` - name contains "read_" (generic read helper); grouped under `main::support`.
  - `src/main.rs:5406-5419` `read_graph_index_lag` - name contains "read_" (generic read helper); grouped under `main::support`.
  - `src/main.rs:5505-5517` `read_order_signatures` - name contains "read_" (generic read helper); grouped under `main::support`.
  - `src/main.rs:5541-5613` `parse_canary_args` - name contains "parse_" (argument/input parsing); grouped under `main::support`.
  - `src/main.rs:6062-6094` `parse_replay_args` - name contains "parse_" (argument/input parsing); grouped under `main::support`.
  - `src/main.rs:6346-6372` `ensure_run_dashboard_at` - name contains "ensure_" (invariant helper); grouped under `main::support`.
  - `src/main.rs:6732-6767` `ensure_run_dashboard` - name contains "ensure_" (invariant helper); grouped under `main::support`.
  - `src/main.rs:8095-8125` `parse_watch_args` - name contains "parse_" (argument/input parsing); grouped under `main::support`.
  - `src/main.rs:9334-9392` `parse_result_args` - name contains "parse_" (argument/input parsing); grouped under `main::support`.
  - `src/main.rs:9432-9446` `read_outcome_from_stdin` - name contains "read_" (generic read helper); grouped under `main::support`.
  - `src/main.rs:10556-10584` `read_run_units` - name contains "read_" (generic read helper); grouped under `main::support`.
  - `src/main.rs:10930-10932` `is_uuid8` - name contains "is_" (predicate helper); grouped under `main::support`.
  - `src/main.rs:10939-10968` `find_shadow_stores` - name contains "find_" (lookup helper); grouped under `main::support`.
  - `src/main.rs:11031-11036` `is_build_cache_tombstone` - name contains "is_" (predicate helper); grouped under `main::support`.
  - `src/main.rs:11854-11903` `write_gitignore_entries` - name contains "write_" (generic write helper); grouped under `main::support`.
  - `src/main.rs:12381-12386` `find_bytes` - name contains "find_" (lookup helper); grouped under `main::support`.
  - `src/main.rs:13116-13134` `write_docs` - name contains "write_" (generic write helper); grouped under `main::support`.
  - `src/main.rs:13755-13762` `write_if_absent` - name contains "write_" (generic write helper); grouped under `main::support`.
- `main::tests` (365 functions)
  - `src/main.rs:12468-12473` `compose_precommit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:13964-13971` `spec_lint_next_step_names_rigger_validate_and_the_given_spec_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:13979-13981` `spec_lint_reminder_suppressed_when_env_names_the_real_direct_parent_pid` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:13987-14008` `spec_lint_reminder_prints_on_absent_foreign_stale_or_malformed_env` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14025-14047` `spec_lint_warning_lines_formats_every_advisory_with_the_spec_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14052-14055` `spec_lint_warning_lines_is_empty_on_a_clean_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14065-14118` `ensure_run_dashboard_at_starts_once_then_short_circuits_on_a_live_marker` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14127-14133` `dash_marker_serving_reports_false_when_nothing_answers_the_markers_port` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14138-14170` `ensure_run_dashboard_at_restarts_when_the_recorded_dash_is_gone` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14178-14197` `ensure_run_dashboard_at_reports_failed_when_the_start_errors` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14209-14248` `wait_for_dash_bind_confirms_promptly_once_the_port_answers_as_a_dash` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14255-14282` `wait_for_dash_bind_fails_fast_when_the_child_exits_before_confirming` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14296-14338` `wait_for_dash_bind_times_out_against_a_real_held_port` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14361-14405` `wait_for_dash_bind_or_diagnose_never_self_attributes_its_own_still_starting_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14422-14450` `ensure_run_dashboard_at_writes_no_marker_when_the_real_spawn_never_confirms_a_bind` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14465-14510` `ensure_run_dashboard_at_names_the_held_ports_holder_when_the_real_spawn_fails` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14530-14554` `ensure_run_dashboard_at_never_claims_a_phantom_holder_for_an_unheld_port` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14566-14597` `ensure_run_dashboard_at_self_heals_a_marker_naming_a_dead_pid` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14607-14641` `ensure_run_dashboard_at_self_heals_a_marker_naming_a_live_pid_whose_port_is_unserved` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14654-14692` `ensure_run_dashboard_at_never_revisits_a_still_serving_unattributed_pid_marker` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14699-14720` `dash_ensure_is_suppressed_by_either_opt_out_and_proceeds_when_neither_is_set` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14730-14761` `dash_status_line_renders_each_outcome_to_its_exact_text` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14769-14797` `dash_status_json_renders_each_outcome_to_its_exact_shape` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14806-14833` `dash_ensure_port_defaults_to_the_fixed_address_and_only_a_valid_override_relocates_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14843-14876` `compose_precommit_fresh_install_carries_the_managed_block` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14886-14913` `precommit_block_refuses_on_drift_instead_of_staging` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14927-14980` `precommit_block_finds_the_relocated_unit_target_with_the_binary_s_own_path_encoding` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14983-15015` `shipped_workflow_driver_tells_a_worker_its_units_build_location` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15018-15119` `precommit_block_resolves_a_tree_built_binary_before_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15126-15138` `compose_precommit_is_idempotent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15149-15178` `compose_precommit_chains_without_clobbering_an_existing_hook` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15188-15202` `compose_precommit_prepends_before_a_terminal_exit_existing_hook` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15211-15262` `install_precommit_hook_preserves_a_non_utf8_existing_hook` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15269-15292` `compose_precommit_refreshes_a_stale_block_in_place` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15300-15319` `cmd_peers_prints_live_or_historical_per_decision_provenance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15337-15380` `superseded_graph_nodes_drops_dead_runs_and_preboundary_keeping_lessons_active_and_reused_ids` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15339-15341` `ev` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15342-15347` `run_started` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15389-15427` `superseded_edge_boundary_is_the_active_runs_start_or_none_without_a_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15391-15397` `run_started_at` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15398-15404` `decision` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15455-15713` `the_denoise_leaves_metrics_run_pruning_and_blast_radius_unaffected` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15461-15465` `ev` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15723-15741` `version_line_carries_the_derived_version_and_a_non_empty_build_provenance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15748-15792` `docs_context_reads_every_fact_from_code` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15799-15840` `docs_render_surfaces_known_code_facts_verbatim` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15846-15865` `commands_registry_is_well_formed_and_covers_dispatch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15873-15903` `write_docs_writes_every_registry_skill_plus_the_handbook` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15913-15950` `install_and_docs_each_cover_exactly_the_registry_no_more_no_less` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15959-15998` `per_operation_skills_reference_only_real_subcommands` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16008-16036` `watching_discipline_skills_reference_only_real_subcommands` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16044-16111` `docs_drift_flags_a_changed_file_and_skips_absent_or_in_sync_ones` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16122-16148` `committed_registry_docs_are_in_sync_with_a_fresh_render` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16159-16188` `planning_field_guide_page_renders_and_is_linked_from_authoring_loops` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16199-16266` `status_and_dashboard_render_the_same_current_blocker_lines` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16279-16334` `release_ready_lines_surface_only_on_a_done_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16337-16403` `status_and_dash_read_the_runs_persisted_base_not_a_re_resolution` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16408-16429` `dirty_tracked_paths_keeps_tracked_modifications_and_drops_untracked_and_ignored` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16432-16434` `dirty_tracked_paths_on_a_clean_tree_is_empty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16437-16462` `installed_workflow_drifted_is_false_when_absent_or_identical_and_true_on_drift` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16467-16514` `drift_side_names_the_binary_stale_only_when_the_installed_workflow_is_provably_newer` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16517-16565` `workflow_drift_advisory_names_which_side_is_stale_and_never_says_they_differ` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16568-16612` `git_is_ancestor_decides_commit_order_in_a_real_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16615-16661` `git_commit_distance_counts_commits_ahead_in_a_real_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16664-16678` `missing_gitsemver_binary_advisory_fires_only_on_the_unversioned_marker` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16681-16686` `behind_the_tree_message_is_silent_when_versions_already_match` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16689-16699` `behind_the_tree_message_is_silent_when_either_side_is_unversioned` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16702-16711` `behind_the_tree_message_is_silent_on_an_undecidable_or_zero_distance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16714-16721` `behind_the_tree_message_names_both_versions_and_the_commit_distance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16728-16734` `gitsemver_available` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16738-16753` `behind_the_tree_git` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16755-16767` `behind_the_tree_git_output` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16770-16808` `behind_the_tree_advisory_names_the_real_derived_version_ahead_of_the_installed_commit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16811-16833` `behind_the_tree_advisory_is_silent_when_the_checkout_has_not_moved` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16836-16849` `install_workflow_records_the_build_provenance_beside_the_workflow` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16852-16867` `validate_advisories_warns_on_workflow_drift_naming_the_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16873-16875` `slugs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16877-16880` `write_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16885-16913` `init_committed_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16931-16947` `resolve_main_worktree_or_refuse_returns_exactly_git_rev_parse_show_toplevel` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16950-16974` `refuse_when_base_lacks_spec_paths_refuses_on_total_absence_and_names_a_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16977-16994` `refuse_when_base_lacks_spec_paths_proceeds_when_the_base_contains_them` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16997-17015` `refuse_when_base_lacks_spec_paths_partial_match_warns_and_proceeds` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17018-17058` `refuse_when_base_lacks_spec_paths_skips_without_tokens_or_off_a_fresh_from_base_anchor` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17061-17142` `refuse_when_base_unreachable_fails_loudly_only_when_no_reachable_base` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17145-17152` `human_size_formats_bytes_through_gib` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17155-17161` `is_uuid8_accepts_exactly_eight_hex_digits` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17164-17209` `worktree_belongs_to_live_matches_both_naming_shapes_without_prefix_false_match` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17212-17259` `current_run_units_scopes_to_the_current_run_and_splits_live_from_dead` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17262-17299` `current_run_units_spares_a_terminal_units_branch_whose_latest_spawn_is_still_in_flight` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17302-17328` `current_run_units_still_retires_a_terminal_unit_once_its_latest_spawn_answers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17331-17378` `current_run_units_splits_live_spawns_from_answered_ones_scoped_to_the_current_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17390-17416` `live_branches_for_sweep_fails_closed_on_an_unreadable_stream_but_folds_a_readable_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17419-17459` `find_shadow_stores_finds_nested_events_db_and_prunes_build_caches` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17462-17473` `dir_size_bytes_sums_files_recursively` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17478-17494` `reclaim_shared_build_cache_deletes_a_populated_cache_and_reports_its_bytes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17497-17515` `reclaim_shared_build_cache_is_idempotent_zero_report_when_absent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17518-17546` `reclaim_shared_build_cache_refuses_rather_than_waits_when_a_build_holds_the_guard` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17549-17572` `reclaim_shared_build_cache_releases_the_guard_promptly_after_a_successful_reclaim` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17575-17648` `scan_residue_reports_dead_worktrees_caches_shadows_and_branches` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17651-17670` `scan_residue_is_empty_when_everything_is_live_and_no_shadow_stores` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17673-17690` `format_residue_renders_a_sized_warning_block` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17695-17714` `store_and_backup_bytes_splits_live_store_files_from_dot_bak_backups` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17717-17721` `store_and_backup_bytes_is_zero_on_a_missing_directory` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17724-17766` `scratch_footprint_totals_every_entry_and_dead_only_the_non_live_share` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17769-17834` `footprint_report_measures_every_category_on_a_seeded_fixture_tree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17837-17854` `footprint_report_folds_a_none_mutation_root_to_a_zero_contribution` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17857-17939` `footprint_report_flags_registered_scratch_roots_dead_share_and_spares_a_live_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17952-18006` `footprint_report_keys_agent_scratch_liveness_by_run_id_and_leaf_not_leaf_name_alone` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18009-18014` `looks_like_run_container_true_when_every_direct_child_is_a_directory` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18017-18025` `looks_like_run_container_false_when_a_direct_child_is_a_bare_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18028-18036` `looks_like_run_container_is_true_on_an_empty_or_missing_dir` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18039-18052` `classify_agent_scratch_separates_a_bare_top_level_file_from_a_well_formed_container` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18055-18134` `footprint_report_reports_a_top_level_adhoc_agent_scratch_dir_as_its_own_category_never_folded_into_the_dead_run_bucket` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18137-18161` `footprint_report_lines_reports_every_categorys_total_unconditionally` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18164-18175` `footprint_advisories_flags_a_category_whose_dead_share_reaches_the_threshold` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18178-18186` `footprint_advisories_is_silent_below_the_threshold` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18189-18206` `footprint_advisories_flags_a_category_exactly_at_the_threshold_boundary` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18209-18219` `footprint_advisories_never_flags_a_category_with_no_reclaim_command` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18222-18230` `footprint_advisories_is_silent_on_an_empty_category` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18233-18256` `owning_repo_root_prefers_the_stores_own_root_over_the_git_toplevel` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18261-18384` `reclaim_orphan_scratch_removes_non_live_owned_scratch_and_spares_live_and_shared_areas` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18387-18452` `reclaim_orphan_scratch_spares_a_non_live_worktree_that_is_still_dirty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18455-18494` `reclaim_orphan_scratch_spares_only_a_declared_dirty_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18497-18527` `reclaim_orphan_scratch_reaps_a_stray_build_cache_tombstone_unconditionally` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18532-18571` `leaked_process_advisories_name_a_process_rooted_under_the_scratch_root` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18574-18584` `leaked_process_advisories_is_empty_when_no_process_is_rooted_under_the_scratch_root` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18587-18594` `leaked_process_advisories_is_a_graceful_no_op_when_the_scratch_root_is_absent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18599-18605` `parse_result_takes_an_id_and_an_optional_output_arg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18608-18613` `parse_result_with_no_output_defers_to_stdin` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18618-18624` `find_store_dir_from_returns_the_dir_that_holds_the_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18627-18639` `find_store_dir_from_walks_up_from_a_subdirectory` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18642-18648` `git_init_quiet` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18651-18688` `find_store_dir_from_never_escapes_the_repo_into_a_parent_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18691-18766` `reap_then_remove_dir_reaps_processes_rooted_inside_then_removes_the_dir` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18769-18818` `reclaim_run_scratch_removes_the_run_level_areas_and_spares_per_unit_scratch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18821-18854` `reclaim_run_scratch_spares_the_shared_build_cache_while_a_build_holds_its_guard` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18857-18868` `find_store_dir_from_refuses_the_worktree_shape_with_no_events_db` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18871-18900` `find_store_dir_from_walks_past_a_storeless_rigger_to_the_real_store_above` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18903-18956` `find_store_dir_from_resolves_the_owning_repo_even_when_the_worktree_lives_outside_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18959-19013` `find_store_dir_from_never_climbs_a_relocated_worktrees_own_unrelated_ancestors_into_a_foreign_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19016-19043` `walk_stores_from_prefers_the_outermost_store_over_a_nearer_shadow` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19046-19064` `walk_stores_from_reports_no_shadow_for_a_single_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19073-19142` `require_store_dir_pins_to_the_fence_env_and_never_reaches_the_live_store_above_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19076-19079` `drop` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19146-19168` `require_store_dir_fence_is_off_by_default` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19152-19154` `drop` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19173-19179` `result_advisories_flags_an_orphan_id_with_no_spawn_request` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19182-19207` `result_advisories_orphan_wording_is_plain_on_record_and_conditional_under_if_absent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19210-19220` `result_advisories_is_silent_for_a_parked_unanswered_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19223-19234` `result_advisories_flags_a_supersede_with_the_prior_result_position` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19237-19250` `result_advisories_suppresses_the_supersede_note_when_not_superseding` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19253-19265` `result_advisories_flags_both_orphan_and_supersede` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19268-19288` `parse_result_error_flag_is_order_independent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19291-19318` `parse_result_if_absent_is_off_by_default_and_a_bare_order_independent_flag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19321-19356` `parse_result_meta_must_be_a_json_object` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19359-19373` `parse_result_rejects_missing_id_extra_args_and_unknown_flags` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19376-19390` `build_result_shapes_success_and_failure` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19393-19398` `build_result_rejects_a_blank_error_message` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19401-19410` `build_result_attaches_meta` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19413-19451` `a_recorded_result_lets_the_replay_driver_advance_past_the_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19454-19486` `a_recorded_error_result_replays_as_a_failure_not_a_fake_success` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19492-19582` `scaffold_parses_into_a_valid_config` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19596-19613` `shipped_workflows_carry_a_non_zero_spawn_budget` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19616-19628` `parse_canary_args_defaults_corpus_and_jobs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19631-19643` `parse_canary_args_reads_corpus_if_model_changed_and_jobs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19646-19658` `parse_canary_args_rejects_a_non_positive_jobs_value` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19661-19671` `parse_canary_args_rejects_unknown_flags` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19674-19703` `usage_text_gives_the_jobs_flag_its_own_description_line` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19706-19723` `usage_text_names_the_build_cache_reset_mode` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19726-19736` `parse_run_args_defaults_to_cli_and_an_unset_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19739-19750` `parse_run_args_reads_fresh_alongside_a_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19753-19763` `parse_run_args_reads_rebase_definition` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19766-19781` `parse_run_args_reads_driver_eventstore_conn_and_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19784-19789` `parse_run_args_rejects_unknown_flags_and_values` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19796-19822` `parse_run_args_accepts_base_alongside_a_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19830-19852` `resolve_run_base_precedence_flag_then_env_then_default` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19858-19896` `parse_workflow_args_reads_spec_and_base` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19901-19953` `parse_step_args_reads_spec_and_base_with_default` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19959-20003` `definition_hash_is_stable_and_content_sensitive` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20012-20039` `kurrentdb_is_always_available_and_needs_a_conn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20051-20082` `store_selection_preserves_a_credentialed_tls_conn_verbatim` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20093-20270` `store_selection_precedence_flag_env_secret_file_config_then_default` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20273-20275` `project_identity_is_never_empty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20278-20303` `project_identity_reads_the_tracked_id_file_then_falls_back_to_the_basename` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20306-20333` `ssh_https_and_git_suffix_forms_of_one_repo_mint_identical_ids` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20336-20350` `normalize_origin_url_separates_distinct_repos_and_lowercases_only_the_host` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20353-20382` `decide_migration_covers_every_case` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20385-20424` `migrate_project_identity_renames_legacy_history_and_records_a_decision` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20427-20466` `migrate_project_identity_refuses_when_both_namespaces_hold_history` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20469-20534` `migrate_project_identity_rekeys_graph_rows_so_pre_mint_history_is_not_orphaned` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20537-20617` `migrate_project_identity_rekeys_the_graph_before_the_irreversible_stream_rename` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20620-20715` `migrate_project_identity_recovers_from_a_crash_between_the_rekey_and_the_rename` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20724-20829` `dash_graph_provider_reaches_the_whole_projection_when_run_seeds_are_empty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20843-20882` `dash_read_whole_graph_on_an_absent_db_is_empty_and_creates_nothing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20888-20920` `setup_provisions_the_shim_runtime_files` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20929-20988` `setup_installs_refreshes_and_is_a_noop_on_the_native_rigger_workflow` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20993-21002` `outcome_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21006-21011` `registry_entry` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21021-21091` `setup_installs_the_using_rigger_skill_distinct_from_the_workflow` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21109-21193` `planning_a_spec_installs_and_renders_through_the_registry_with_no_planning_specific_code` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21203-21239` `setup_skill_install_applies_the_project_overlay` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21245-21271` `docs_overlay_overrides_only_declared_fields` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21276-21286` `docs_overlay_malformed_is_a_loud_error` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21296-21324` `provision_shim_is_a_silent_noop_when_already_current` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21334-21353` `shim_is_not_current_when_node_modules_is_torn_missing_the_install_marker` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21360-21382` `init_project_is_idempotent_reporting_new_work_only_once` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21392-21450` `scaffold_summary_reports_only_the_gitignore_change_on_a_gitignore_only_repair` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21461-21540` `init_project_gitignores_the_dash_runtime_breadcrumbs_idempotently` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21549-21589` `init_project_gitignores_the_store_conn_secret_file_idempotently` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21598-21612` `a_group_or_other_readable_secret_file_mode_is_flagged_owner_only_is_not` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21624-21672` `init_project_still_writes_the_dash_ignore_lines_when_a_broader_rule_covers_them` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21681-21743` `scaffold_agents_and_workflow_reference_the_same_canonical_set` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21750-21793` `init_scaffolds_only_the_workflow_referenced_agents` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21800-21829` `get_referenced_agent_ids_reads_the_scaffolded_workflows_fleet` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21836-21868` `write_if_absent_wrote_kept_and_errors_naming_the_artifact` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21876-21893` `parse_setup_args_reads_the_agents_directory_flag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21900-21945` `import_agents_copies_and_normalizes_the_identity_field` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21951-21988` `import_agents_refuses_to_overwrite_an_existing_agent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21994-22007` `import_agents_validates_and_rejects_a_malformed_agent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22016-22038` `import_agents_rejects_an_id_colliding_with_an_existing_agent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22044-22064` `import_agents_rejects_a_duplicate_id_within_one_import` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22070-22091` `import_agents_rejects_an_agent_with_a_blank_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22098-22120` `import_agents_runs_full_validation_and_rejects_a_broken_project` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22126-22145` `meta_object_body` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22154-22168` `meta_description` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22175-22183` `strip_line_comments` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22197-22420` `workflow_is_a_thin_courier_driver_with_per_unit_phase_labels` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22429-22440` `the_step_schema_admits_the_attention_array` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22446-22468` `js_function_body` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22484-22539` `phase_of_maps_wave_items_to_the_meta_phase_by_role_and_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22552-22590` `meta_matches_reality_drops_integrate_and_the_unit_stage_construction` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22609-22700` `the_driver_relays_each_attention_entry_as_a_narrator_log_line` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22707-22720` `merge_hung_attention_does_nothing_when_not_newly_hung` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22728-22740` `merge_hung_attention_defers_to_an_existing_budget_halt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22749-22785` `merge_hung_attention_lands_in_canonical_position_alongside_other_signals` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22803-22851` `workflow_step_courier_prompt_is_foreground_and_honest` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22861-22870` `step_courier_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22888-22955` `workflow_step_courier_waits_on_an_auto_backgrounded_step` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22970-23025` `workflow_driver_guards_a_null_step_before_dereferencing_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23038-23095` `workflow_meta_description_is_a_user_facing_tagline_free_of_plumbing_terms` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23102-23126` `setup_runs_npm_install_or_reports_a_clear_error` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23132-23157` `workflow_locates_the_provisioned_shim_or_tells_you_to_run_setup` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23159-23165` `npm_available` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23176-23219` `format_stats_prints_all_four_metrics` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23225-23241` `format_stats_handles_zeroed_metrics_without_nan` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23247-23264` `format_canary_stats_reports_findings_raised_by_tier` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23269-23279` `format_canary_stats_reports_a_zero_findings_count_honestly` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23285-23291` `format_canary_stats_omits_the_findings_volume_section_when_empty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23296-23313` `progress_outcome` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23320-23332` `format_progress_line_names_id_verdict_and_none_when_nothing_caught` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23338-23355` `format_progress_line_reports_a_wrong_verdict_and_every_catching_tier` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23363-23394` `format_canary_stats_renders_na_for_a_tier_with_unattributed_correct_rejects` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23401-23420` `format_canary_stats_renders_the_real_zero_when_attribution_was_fully_measured` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23429-23448` `format_canary_stats_reports_control_items_and_false_positives` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23456-23471` `format_canary_stats_reports_zero_false_positives_honestly` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23478-23507` `format_canary_stats_reports_the_model_pinning_header_when_present` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23513-23519` `format_canary_stats_omits_the_model_pinning_header_when_the_run_never_recorded_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23529-23571` `format_stats_surfaces_parallelism_retention_and_warns_below_the_floor` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23578-23618` `format_stats_surfaces_spawn_timing_and_reports_unpaired_separately` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23624-23630` `format_stats_spawn_timing_reports_no_spawns_when_none_recorded` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23637-23664` `format_stats_spawn_timing_no_spawns_line_requires_both_empty_and_zero_unpaired` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23671-23686` `format_stats_spawn_timing_all_unpaired_is_not_reported_as_no_spawns` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23694-23721` `parallelism_retention_line_is_single_sourced_and_warns_below_the_floor` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23728-23773` `stats_discloses_when_no_verdict_was_recorded_on_this_driver` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23784-23857` `stats_discloses_unfed_numerator_when_verdict_recorded_but_findings_unattributed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23865-23902` `stats_discloses_cause_split_remainder_when_fewer_causes_than_rejects` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23907-23913` `cmd_stats_rejects_extra_arguments` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23916-23961` `baseline_run_slice_selects_a_run_by_id_including_a_middle_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23964-23995` `format_stats_diff_flags_only_the_changed_rows` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24007-24025` `build_environment_report_with_a_wrapper_lists_wrapper_cache_dir_and_budget` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24032-24049` `build_environment_report_with_no_wrapper_omits_cache_dir_but_keeps_budget` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24055-24065` `build_environment_report_zero_max_concurrent_reports_unlimited` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24071-24080` `build_environment_report_reports_mutation_gate_declared` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24086-24095` `build_environment_report_reports_mutation_gate_not_configured` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24113-24132` `order_signature_advisories_names_the_stream_count_range_and_repair_doc` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24136-24138` `order_signature_advisories_is_empty_when_no_signatures_are_given` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24146-24152` `drift_change` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24158-24187` `model_drift_advisory_is_a_soft_note_for_snapshot_only_drift` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24192-24203` `model_drift_advisory_stays_a_warning_when_any_change_is_a_real_model_repoint` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24207-24209` `model_drift_advisory_is_none_when_nothing_changed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24214-24230` `index_staleness_message_names_every_kind_of_disagreement_and_the_fix` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24237-24249` `graph_index_lag_advisory_names_every_lagging_file_and_the_fix` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24252-24254` `graph_index_lag_advisory_is_none_when_the_sample_is_empty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24257-24280` `bloat_advisory_is_none_at_or_below_the_threshold_and_named_above_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24288-24303` `assert_advisory_for_never_fabricates_a_missing_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24306-24308` `bloat_advisory_for_never_fabricates_a_store_that_does_not_exist` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24313-24331` `retired_entities_advisory_is_none_at_zero_and_named_with_correct_pluralization` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24334-24339` `retired_entities_advisory_for_never_fabricates_a_graph_that_does_not_exist` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24342-24376` `retired_entities_advisory_for_reads_the_projectors_own_counting_authority` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24383-24390` `scaffold_workflow_declares_build_wrapper_auto` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24398-24412` `init_project_never_clobbers_an_existing_build_section` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24415-24432` `parse_replay_args_requires_a_run_and_a_rev_in_either_order` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24444-24465` `cmd_stats_on_a_never_run_project_says_no_runs_and_creates_no_db` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24451-24453` `drop` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24473-24520` `a_second_concurrent_rigger_step_refuses_and_the_lock_frees_on_release` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24478-24480` `drop` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24526-24529` `no_runs_message_points_at_rigger_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24535-24542` `seed_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24548-24560` `stats_lines_absent_db_returns_none_and_creates_no_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24567-24583` `stats_lines_existing_db_with_empty_run_stream_returns_none` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24591-24620` `stats_lines_does_not_read_another_projects_namespaced_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24627-24658` `stats_lines_existing_run_renders_metric_lines` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24681-24750` `stats_lines_pairs_recorded_spawn_request_and_result_into_spawn_timing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24758-24770` `result_of_at_absent_db_reads_as_unreported_and_creates_no_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24776-24796` `result_of_at_unrecorded_spawn_reads_as_unreported` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24804-24828` `result_of_at_reads_a_self_reported_result_so_it_is_not_clobbered` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24834-24863` `result_of_at_is_namespace_scoped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24870-24882` `cmd_reported_requires_exactly_one_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24893-24906` `pgid_of` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24917-24942` `detach_process_group_places_the_child_in_its_own_process_group` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24951-24972` `a_child_spawned_without_detachment_inherits_the_parent_process_group` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24986-25006` `spawn_run_dashboard_detached_session_detaches_the_dash` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25021-25041` `report_of` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25052-25075` `a_compaction_that_failed_after_the_deletes_is_reported_beside_the_counts` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25086-25109` `a_prune_that_shed_nothing_is_justified_by_this_log_not_by_when_it_was_written` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25120-25131` `a_pass_that_deleted_nothing_but_reclaimed_space_reports_the_reclamation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25136-25167` `a_prune_that_shed_rows_explains_the_duplication_a_deduplicated_log_still_accumulates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25181-25216` `an_unmeasurable_database_is_not_reported_as_a_checkpoint_a_reader_declined` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25220-25222` `no_live_units` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25226-25244` `live_writer_reasons_is_empty_only_when_all_four_facts_are_quiet` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25250-25269` `refusal_names_a_held_step_lock_and_the_force_live_override_owning_the_risk` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25274-25282` `refusal_names_a_non_terminal_unit_between_spawn_rounds` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25286-25298` `refusal_names_every_in_flight_spawn_id_and_the_count` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25302-25309` `refusal_names_the_driver_registration_count` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25314-25326` `refusal_names_every_applicable_reason_together_not_just_the_first` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25332-25361` `refuse_derived_reset_if_live_fails_safe_on_a_malformed_spawn_event` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25367-25388` `reset_modes_parses_force_live_alongside_derived_rejects_duplicates_and_never_implies_a_mode` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25395-25417` `reset_modes_parses_scratch_orphans_alone_and_composed_and_rejects_duplicates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25420-25437` `reset_modes_parses_build_cache_alone_and_composed_and_rejects_duplicates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25445-25454` `watch_test_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25477-25510` `store_location_repo_root_resolves_the_owning_root_not_the_process_cwd_so_a_real_marker_is_found` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25524-25555` `scratch_defaults_reads_the_owning_roots_config_with_no_agents_fleet_present` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25558-25583` `watch_once_on_a_clean_store_reports_no_anomalies` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25586-25590` `parse_watch_args_defaults_to_streaming_with_the_default_interval` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25597-25619` `parse_watch_args_accepts_once_and_interval_together_in_either_order` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25622-25626` `parse_watch_args_rejects_a_non_integer_interval_a_missing_value_and_an_unknown_flag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25634-25754` `watch_once_on_the_seeded_store_reports_one_line_per_anomaly` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25761-25780` `the_watchdog_command_signal_set_covers_every_signal_the_watch_skill_names` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25788-25816` `watch_once_reports_dash_not_serving_when_the_marker_names_a_dead_holder` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25823-25853` `watch_once_reports_no_anomaly_when_the_dash_marker_names_a_real_serving_holder` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25858-25881` `runs_menu_line_names_the_measured_counts_and_the_flag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25884-25913` `derived_menu_line_sums_the_measured_duplicate_counts_and_names_the_flag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25920-25935` `derived_menu_line_on_a_server_backend_says_so_instead_of_a_fabricated_count` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25954-26047` `implementer_persona_pins_the_checkin_stage_kill_or_justify_contract` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26055-26097` `implementer_persona_pins_the_checkpoint_before_long_work_contract` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26108-26138` `no_persona_under_rigger_agents_invokes_cargo_mutants` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26151-26186` `install_operator_mcp_installs_refreshes_and_is_a_noop` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26192-26230` `install_lookup_hook_installs_refreshes_and_is_a_noop_and_preserves_foreign_hooks` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26238-26262` `assert_allows_with_literal_stripped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26276-26303` `grep_guard_decision_denies_every_bash_grep_target_and_passes_literal` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26309-26322` `grep_guard_decision_allows_non_grep_bash_commands` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26331-26339` `grep_guard_decision_denies_every_grep_tool_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26343-26348` `grep_guard_decision_ignores_other_tools` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26359-26375` `grep_guard_decision_denies_a_shell_metacharacter_fused_grep` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26381-26387` `grep_guard_decision_literal_survives_a_shell_metacharacter_fused_grep` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26398-26411` `grep_guard_decision_denies_a_redirect_metacharacter_fused_grep` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26416-26427` `grep_guard_decision_literal_survives_a_redirect_metacharacter_fused_grep` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26434-26447` `grep_guard_decision_denies_a_quoted_or_escaped_grep` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26453-26466` `grep_guard_decision_literal_survives_a_quoted_literal_on_a_quoted_grep` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26473-26482` `grep_guard_decision_denies_a_grep_split_by_a_line_continuation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26486-26492` `grep_guard_decision_literal_survives_a_line_continuation_split_grep` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26499-26511` `grep_guard_decision_denies_a_path_qualified_grep` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26515-26524` `grep_guard_decision_literal_survives_a_path_qualified_grep` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26529-26538` `grep_guard_decision_allows_a_path_qualified_non_grep_command` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).

### Unassigned (182 functions)

- `src/conductor.rs:376-378` `adoption_provenance_key` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:413-442` `glob_matches` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:596-612` `input_digest` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:833-850` `conflict_resolution_prompt` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:1090-1092` `conflict_regenerate_key` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:1759-1771` `replay_trajectory` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:1775-2395` `run` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:2474-2566` `compute_attention` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:10912-10919` `fnv1a_64` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:10924-10933` `verified_evidence` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:11276-11323` `render_capped_section` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:11370-11380` `recency_by_own_edge` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:11531-11534` `plain_node_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:11665-11681` `confidence_tier_radius` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:11691-11745` `files_reachable` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:12004-12012` `current_run_spec` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:12161-12178` `branch_owner` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:12217-12223` `quarantine_branch_name` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:12295-12323` `quarantined_branch` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:12354-12359` `same_path` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:12366-12381` `sanitize_for_path` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:12386-12388` `integrates` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:12567-12576` `fan_out_template_name` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:12609-12615` `implement_slice` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:12699-12704` `producer_name` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:12803-12811` `fan_out_lenses` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:12850-12872` `coverage_gap` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:12935-12950` `need_satisfied` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:12992-13029` `validate_acyclic` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
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
- `src/main.rs:1657-1659` `usage` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1661-1666` `db_path` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1708-1765` `walk_stores_from` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1778-1799` `main_repo_root` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1874-1923` `refuse_unless_one_root` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:2132-2170` `result_advisories` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:2185-2189` `load_run_config` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:2264-2284` `acquire_step_lock` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:2972-2985` `merge_hung_attention` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3126-3129` `reap_then_remove_dir` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3140-3154` `reap_then_remove_worktree` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3316-3329` `result_of_at` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3418-3442` `warn_on_run_branch_divergence` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3455-3464` `anchor_run_branch` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3488-3502` `refuse_when_base_unreachable` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3574-3700` `run_cli` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3724-3768` `fresh_run_if_requested` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:4099-4116` `load_criteria` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:4437-4490` `definition_body` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:4517-4561` `locate_definition_extent` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:4569-4579` `locate_definition_extent` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:4936-4965` `stats_lines` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:4986-5000` `parallelism_retention_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:5063-5084` `append_spawn_timing` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:5089-5091` `fmt_duration` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:5119-5229` `append_review_quality` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:5258-5274` `canary_stats_lines` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:5426-5470` `model_drift_advisory` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:5479-5496` `order_signature_advisories` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:5993-6003` `started_units` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:6024-6057` `candidate_reaches_gate` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:6101-6115` `baseline_run_slice` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:6118-6122` `run_started_id` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:6129-6175` `materialize_config_at_rev` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:6258-6276` `start_run_dashboard` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:6283-6298` `spawn_run_dashboard` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:6385-6391` `detach_process_group` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:6394-6394` `detach_process_group` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:6628-6683` `spawn_run_dashboard_detached` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:7222-7272` `watch_and_self_reap_on_idle` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:7477-7479` `instance_rigger_dir` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:8057-8065` `status_blocker_lines` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:8074-8080` `release_ready_lines` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:8201-8414` `watch_poll` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:8569-8575` `runs_menu_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:8582-8604` `derived_menu_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:8806-8895` `derived_prune_report` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9208-9212` `graph_node_id` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9271-9281` `peer_decision_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9285-9294` `json_str_array` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9298-9307` `json_type_name` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9403-9424` `build_result` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9672-9684` `fold_recorded_result_into_graph` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9916-9949` `build_environment_report` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9957-9996` `validate_advisories` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10003-10029` `index_staleness_message` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10038-10048` `graph_index_lag_advisory` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10101-10110` `retired_entities_advisory` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10123-10129` `retired_entities_advisory_for` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10224-10235` `missing_gitsemver_binary_advisory` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10247-10267` `behind_the_tree_message` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10286-10298` `behind_the_tree_advisory` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10318-10332` `drift_side` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10381-10402` `uncommitted_rigger_advisory` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10410-10425` `dirty_tracked_paths` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10465-10502` `residue_advisories` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10513-10530` `leaked_process_advisories` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10630-10676` `current_run_units` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10808-10827` `local_unit_branches` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10835-10896` `scan_residue` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10973-10992` `dir_size_bytes` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:11014-11024` `build_cache_tombstone_path` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:11123-11136` `human_size` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:11368-11378` `dead_spawn_leaf_bytes` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:11388-11395` `looks_like_run_container` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:11601-11613` `owning_repo_root` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:11691-11799` `init_project` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:11907-11935` `get_referenced_agent_ids` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:12234-12375` `precommit_block` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:12406-12458` `compose_precommit_bytes` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:12911-13002` `import_agents` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13011-13041` `normalize_identity` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13046-13052` `top_level_key` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13335-13350` `select_grounder` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13358-13360` `select_reindex_grounder` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13473-13498` `grep_guard_decision` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13536-13598` `shell_command_word_spans` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13602-13608` `shell_command_words` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13624-13649` `strip_literal_marker` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13654-13659` `word_basename` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13670-13672` `command_invokes_grep` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.

## 2. Duplication Catalog

800 clusters (3863 total sites) across `src/` and `tests/`, found by `tests/simplification_audit.rs`'s deterministic normalized-token-shingle Jaccard pass (8-token shingles, threshold 0.72) plus five mandatory mechanical sweeps. Strict definition (spec 85 Goal): any logic present in more than one place anywhere in the codebase is a violation, with no "small enough to duplicate" exemption.

### Mandatory sweeps

- **Command::new call sites**: 389 site(s) - `dup-0006`
- **/proc-path string literals**: 60 site(s) - `dup-0148`
- **sqlite Connection::open call sites**: 46 site(s) - `dup-0126`
- **.rigger-path string literals**: 741 site(s) - `dup-0061`
- **error-shaping helper functions**: 8 site(s) - `dup-0080`

### Clusters (257 exact, 459 near, 84 semantic)

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
- `src/main.rs:15339-15341` `ev`
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

#### `dup-0006` (semantic, 389 sites)

Proposed home: `a single injected process-spawn port every Command::new site routes through instead of constructing its own Command`

mandatory sweep: Command::new call sites - 389 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/budget.rs:208-208` `Command::new`
- `src/budget.rs:246-246` `Command::new`
- `src/budget.rs:257-257` `Command::new`
- `src/conductor.rs:13766-13766` `Command::new`
- `src/conductor.rs:13846-13846` `Command::new`
- `src/conductor.rs:13863-13863` `Command::new`
- `src/conductor.rs:13881-13881` `Command::new`
- `src/conductor.rs:14346-14346` `Command::new`
- `src/conductor.rs:14351-14351` `Command::new`
- `src/conductor.rs:21337-21337` `Command::new`
- `src/conductor.rs:25160-25160` `Command::new`
- `src/conductor.rs:25778-25778` `Command::new`
- `src/conductor.rs:25852-25852` `Command::new`
- `src/conductor.rs:26777-26777` `Command::new`
- `src/conductor.rs:26966-26966` `Command::new`
- `src/conductor.rs:30746-30746` `Command::new`
- `src/conductor.rs:30844-30844` `Command::new`
- `src/conductor.rs:30929-30929` `Command::new`
- `src/conductor.rs:31221-31221` `Command::new`
- `src/conductor.rs:31551-31551` `Command::new`
- `src/conductor.rs:31581-31581` `Command::new`
- `src/conductor.rs:31729-31729` `Command::new`
- `src/conductor.rs:32237-32237` `Command::new`
- `src/conductor.rs:35896-35896` `Command::new`
- `src/conductor.rs:36557-36557` `Command::new`
- `src/conductor.rs:37225-37225` `Command::new`
- `src/conductor.rs:37232-37232` `Command::new`
- `src/conductor.rs:37322-37322` `Command::new`
- `src/conductor.rs:37379-37379` `Command::new`
- `src/conductor.rs:37435-37435` `Command::new`
- `src/conductor.rs:37646-37646` `Command::new`
- `src/conductor.rs:37660-37660` `Command::new`
- `src/conductor.rs:37708-37708` `Command::new`
- `src/conductor.rs:37940-37940` `Command::new`
- `src/conductor.rs:38265-38265` `Command::new`
- `src/conductor.rs:38300-38300` `Command::new`
- `src/conductor.rs:39481-39481` `Command::new`
- `src/conductor.rs:39499-39499` `Command::new`
- `src/conductor.rs:39517-39517` `Command::new`
- `src/conductor.rs:39548-39548` `Command::new`
- `src/conductor.rs:39739-39739` `Command::new`
- `src/conductor.rs:40052-40052` `Command::new`
- `src/dash.rs:4961-4961` `Command::new`
- `src/driver/cli.rs:45-45` `Command::new`
- `src/gate.rs:745-745` `Command::new`
- `src/gate.rs:754-754` `Command::new`
- `src/gate.rs:1690-1690` `Command::new`
- `src/main.rs:1181-1181` `Command::new`
- `src/main.rs:1779-1779` `Command::new`
- `src/main.rs:3143-3143` `Command::new`
- `src/main.rs:4016-4016` `Command::new`
- `src/main.rs:6141-6141` `Command::new`
- `src/main.rs:6168-6168` `Command::new`
- `src/main.rs:6286-6286` `Command::new`
- `src/main.rs:6415-6415` `Command::new`
- `src/main.rs:10185-10185` `Command::new`
- `src/main.rs:10204-10204` `Command::new`
- `src/main.rs:10382-10382` `Command::new`
- `src/main.rs:10809-10809` `Command::new`
- `src/main.rs:11872-11872` `Command::new`
- `src/main.rs:12480-12480` `Command::new`
- `src/main.rs:12614-12614` `Command::new`
- `src/main.rs:13374-13374` `Command::new`
- `src/main.rs:14224-14224` `Command::new`
- `src/main.rs:14257-14257` `Command::new`
- `src/main.rs:14301-14301` `Command::new`
- `src/main.rs:14370-14370` `Command::new`
- `src/main.rs:14966-14966` `Command::new`
- `src/main.rs:16572-16572` `Command::new`
- `src/main.rs:16619-16619` `Command::new`
- `src/main.rs:16729-16729` `Command::new`
- `src/main.rs:16739-16739` `Command::new`
- `src/main.rs:16756-16756` `Command::new`
- `src/main.rs:16892-16892` `Command::new`
- `src/main.rs:16904-16904` `Command::new`
- `src/main.rs:17074-17074` `Command::new`
- `src/main.rs:18433-18433` `Command::new`
- `src/main.rs:18439-18439` `Command::new`
- `src/main.rs:18541-18541` `Command::new`
- `src/main.rs:18643-18643` `Command::new`
- `src/main.rs:18709-18709` `Command::new`
- `src/main.rs:18715-18715` `Command::new`
- `src/main.rs:18919-18919` `Command::new`
- `src/main.rs:18925-18925` `Command::new`
- `src/main.rs:18939-18939` `Command::new`
- `src/main.rs:18973-18973` `Command::new`
- `src/main.rs:18979-18979` `Command::new`
- `src/main.rs:18996-18996` `Command::new`
- `src/main.rs:22409-22409` `Command::new`
- `src/main.rs:23160-23160` `Command::new`
- `src/main.rs:24920-24920` `Command::new`
- `src/main.rs:24954-24954` `Command::new`
- `src/worktree.rs:644-644` `Command::new`
- `src/worktree.rs:1199-1199` `Command::new`
- `src/worktree.rs:1210-1210` `Command::new`
- `src/worktree.rs:2400-2400` `Command::new`
- `src/worktree.rs:3074-3074` `Command::new`
- `src/worktree.rs:3912-3912` `Command::new`
- `src/worktree.rs:5337-5337` `Command::new`
- `src/worktree.rs:5670-5670` `Command::new`
- `src/worktree.rs:6460-6460` `Command::new`
- `src/worktree.rs:6466-6466` `Command::new`
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
- `tests/checkpoint_commit_hook_bypass_periphery.rs:56-56` `Command::new`
- `tests/cli.rs:24-24` `Command::new`
- `tests/cli.rs:50-50` `Command::new`
- `tests/cli.rs:113-113` `Command::new`
- `tests/cli.rs:127-127` `Command::new`
- `tests/cli.rs:190-190` `Command::new`
- `tests/cli.rs:1825-1825` `Command::new`
- `tests/cli.rs:1886-1886` `Command::new`
- `tests/cli.rs:5955-5955` `Command::new`
- `tests/cli.rs:6180-6180` `Command::new`
- `tests/cli.rs:6372-6372` `Command::new`
- `tests/cli.rs:6622-6622` `Command::new`
- `tests/cli.rs:6784-6784` `Command::new`
- `tests/cli.rs:12149-12149` `Command::new`
- `tests/cli.rs:12159-12159` `Command::new`
- `tests/cli.rs:12191-12191` `Command::new`
- `tests/cli.rs:14931-14931` `Command::new`
- `tests/cli.rs:15696-15696` `Command::new`
- `tests/cli.rs:15749-15749` `Command::new`
- `tests/cli.rs:20927-20927` `Command::new`
- `tests/cli.rs:20996-20996` `Command::new`
- `tests/cli.rs:21069-21069` `Command::new`
- `tests/cli.rs:21150-21150` `Command::new`
- `tests/cli.rs:21326-21326` `Command::new`
- `tests/cli.rs:21386-21386` `Command::new`
- `tests/cli.rs:21494-21494` `Command::new`
- `tests/cli.rs:21522-21522` `Command::new`
- `tests/cli.rs:21643-21643` `Command::new`
- `tests/cli.rs:21703-21703` `Command::new`
- `tests/cli.rs:21752-21752` `Command::new`
- `tests/cli.rs:21790-21790` `Command::new`
- `tests/cli.rs:21852-21852` `Command::new`
- `tests/cli.rs:21931-21931` `Command::new`
- `tests/cli.rs:21989-21989` `Command::new`
- `tests/cli.rs:22035-22035` `Command::new`
- `tests/cli.rs:29517-29517` `Command::new`
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
- `tests/graph_around_code_first.rs:35-35` `Command::new`
- `tests/graph_around_code_first.rs:53-53` `Command::new`
- `tests/graph_around_governance_boundaries.rs:42-42` `Command::new`
- `tests/graph_around_governance_boundaries.rs:60-60` `Command::new`
- `tests/graph_collision_body_and_tiebreak.rs:45-45` `Command::new`
- `tests/graph_collision_body_and_tiebreak.rs:108-108` `Command::new`
- `tests/graph_density_spread_floor_and_centring.rs:51-51` `Command::new`
- `tests/graph_density_spread_floor_and_centring.rs:114-114` `Command::new`
- `tests/graph_fresh_on_integration_periphery.rs:65-65` `Command::new`
- `tests/graph_show_periphery.rs:57-57` `Command::new`
- `tests/graph_show_periphery.rs:68-68` `Command::new`
- `tests/graph_show_periphery.rs:96-96` `Command::new`
- `tests/graph_show_staleness.rs:42-42` `Command::new`
- `tests/graph_show_staleness.rs:53-53` `Command::new`
- `tests/graph_show_staleness.rs:101-101` `Command::new`
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
- `tests/integrate_conflict_merge_periphery.rs:292-292` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:303-303` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:314-314` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:333-333` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:580-580` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:780-780` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:1040-1040` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:1271-1271` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:1477-1477` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:1744-1744` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:1854-1854` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:2176-2176` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:2312-2312` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:2570-2570` `Command::new`
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
- `tests/reset_derived_compaction_periphery.rs:2600-2600` `Command::new`
- `tests/reset_derived_live_writer_guard_periphery.rs:43-43` `Command::new`
- `tests/reset_derived_live_writer_guard_periphery.rs:58-58` `Command::new`
- `tests/reset_derived_live_writer_guard_periphery.rs:286-286` `Command::new`
- `tests/reset_derived_live_writer_guard_periphery.rs:295-295` `Command::new`
- `tests/reset_menu.rs:39-39` `Command::new`
- `tests/reset_menu.rs:47-47` `Command::new`
- `tests/reset_menu_identity_migration_periphery.rs:34-34` `Command::new`
- `tests/reset_menu_identity_migration_periphery.rs:42-42` `Command::new`
- `tests/revert_on_base_hook_bypass_periphery.rs:66-66` `Command::new`
- `tests/revert_on_base_hook_bypass_periphery.rs:239-239` `Command::new`
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
- `tests/workflow_definition_and_js_constants_periphery.rs:80-80` `Command::new`
- `tests/workflow_definition_and_js_constants_periphery.rs:94-94` `Command::new`
- `tests/workflow_definition_and_js_constants_periphery.rs:118-118` `Command::new`
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

#### `dup-0008` (exact, 15 sites)

Proposed home: `a new shared module (sites span 14 files: src/canary.rs, src/conductor.rs, src/config.rs, tests/canary_false_positives_periphery.rs, tests/canary_findings_volume_periphery.rs, tests/canary_item_sharding_jobs_cap_periphery.rs, tests/canary_lens_fanout_periphery.rs, tests/canary_progress_hook_periphery.rs, tests/canary_tolerant_attribution_periphery.rs, tests/canary_unattributed_rejects_periphery.rs, tests/checkpoint_commit_hook_bypass_periphery.rs, tests/integrate_conflict_merge_periphery.rs, tests/plan_stage_commit_landing_periphery.rs, tests/revert_on_base_hook_bypass_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/canary.rs:928-933` `agent`
- `src/canary.rs:1037-1042` `with_anchor`
- `src/conductor.rs:14403-14408` `agent`
- `src/config.rs:2801-2806` `agent_def`
- `tests/canary_false_positives_periphery.rs:123-128` `agent`
- `tests/canary_findings_volume_periphery.rs:109-114` `agent`
- `tests/canary_item_sharding_jobs_cap_periphery.rs:41-46` `agent`
- `tests/canary_lens_fanout_periphery.rs:107-112` `agent`
- `tests/canary_progress_hook_periphery.rs:47-52` `agent`
- `tests/canary_tolerant_attribution_periphery.rs:88-93` `agent`
- `tests/canary_unattributed_rejects_periphery.rs:110-115` `agent`
- `tests/checkpoint_commit_hook_bypass_periphery.rs:78-83` `agent`
- `tests/integrate_conflict_merge_periphery.rs:348-353` `agent`
- `tests/plan_stage_commit_landing_periphery.rs:267-272` `agent`
- `tests/revert_on_base_hook_bypass_periphery.rs:89-94` `agent`

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
- `src/ingest.rs:437-439` `is_derived_index_type`
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
- `src/conductor.rs:1090-1092` `conflict_regenerate_key`
- `src/spawn.rs:90-92` `spawn_id`

#### `dup-0026` (near, 5 sites)

Proposed home: `a new shared module (sites span 4 files: src/conductor.rs, src/contextgraph/sqlite.rs, src/spawn.rs, tests/no_os_kill_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:376-378` `adoption_provenance_key`
- `src/conductor.rs:389-391` `quarantine_record_key`
- `src/contextgraph/sqlite.rs:1914-1916` `code_entity_id`
- `src/spawn.rs:441-443` `what`
- `tests/no_os_kill_audit.rs:45-47` `join`

#### `dup-0027` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/conductor.rs, src/spawn.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:569-571` `unit_of_gate_key`
- `src/spawn.rs:177-179` `unit_of`

#### `dup-0028` (near, 18 sites)

Proposed home: `a new shared module (sites span 11 files: src/conductor.rs, src/eventstore/namespace.rs, src/eventstore/sqlite.rs, src/grounder/mod.rs, src/grounder/workflowdef.rs, src/main.rs, src/spawn.rs, src/worktree.rs, tests/canary_model_drift_periphery.rs, tests/halted_spawn_wip_recovery_periphery.rs, tests/reset_derived_compaction_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:685-687` `deferred_gate_verdict_key`
- `src/conductor.rs:696-698` `deferred_gate_failed_key`
- `src/conductor.rs:11121-11123` `build_system_prompt`
- `src/conductor.rs:11148-11155` `review_protocol`
- `src/eventstore/namespace.rs:69-71` `prefix_for`
- `src/eventstore/sqlite.rs:1404-1406` `successor`
- `src/grounder/mod.rs:259-265` `retired_grounder_error`
- `src/grounder/workflowdef.rs:27-29` `stage_id`
- `src/grounder/workflowdef.rs:31-33` `gate_id`
- `src/grounder/workflowdef.rs:35-37` `agent_id`
- `src/main.rs:12081-12083` `skill_source_rel`
- `src/main.rs:13228-13234` `spec_lint_next_step`
- `src/spawn.rs:65-67` `lens_role`
- `src/spawn.rs:143-145` `speculation_group_id`
- `src/worktree.rs:1661-1663` `shared_build_cache_guard_path`
- `tests/canary_model_drift_periphery.rs:140-142` `prose_claiming`
- `tests/halted_spawn_wip_recovery_periphery.rs:202-204` `unit_branch`
- `tests/reset_derived_compaction_periphery.rs:2817-2819` `derived_key_for`

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

- `src/conductor.rs:1469-1471` `is_parked`
- `src/conductor.rs:1507-1509` `is_budget_refused`
- `src/conductor.rs:1585-1587` `is_degenerate_reviewer`
- `src/conductor.rs:1628-1630` `is_verdict_channel_mismatch`
- `src/conductor.rs:1672-1674` `is_plan_landing_failed`

#### `dup-0032` (exact, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:1498-1502` `budget_refused`
- `src/conductor.rs:1613-1623` `verdict_channel_mismatch`

#### `dup-0033` (semantic, 2 sites)

Proposed home: `one shared `append_and_fold_batch` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/conductor.rs:2982-3004` `append_and_fold_batch`
- `src/ingest.rs:47-87` `append_and_fold_batch`

#### `dup-0034` (exact, 2 sites)

Proposed home: `conductor::run_ctx`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:3188-3190` `recorded_gate_verdict`
- `src/conductor.rs:3200-3202` `cached_green_verdict`

#### `dup-0035` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/conductor.rs, src/dash.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:3639-3641` `spawn_is_recorded`
- `src/dash.rs:2032-2034` `is_shared`

#### `dup-0036` (exact, 4 sites)

Proposed home: `conductor::run_ctx`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:3683-3685` `budget_broke`
- `src/conductor.rs:3691-3693` `parked`
- `src/conductor.rs:3699-3701` `manual_review_pending`
- `src/conductor.rs:3706-3708` `budget_halted`

#### `dup-0037` (semantic, 2 sites)

Proposed home: `one shared `effective_review_panel` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/conductor.rs:4186-4188` `effective_review_panel`
- `src/config.rs:778-784` `effective_review_panel`

#### `dup-0038` (exact, 2 sites)

Proposed home: `conductor::run_ctx`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:9236-9241` `clear_regenerate_pending`
- `src/conductor.rs:9257-9262` `clear_pending_landing`

#### `dup-0039` (near, 3 sites)

Proposed home: `conductor::run_ctx`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:9392-9406` `record_merge_attempt`
- `src/conductor.rs:9438-9453` `record_landing_intent`
- `src/conductor.rs:9458-9466` `record_landed`

#### `dup-0040` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/conductor.rs, src/distiller.rs, tests/cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:10864-10866` `normalize_ws`
- `src/distiller.rs:60-62` `normalize`
- `tests/cli.rs:26301-26303` `normalize_ws`

#### `dup-0041` (semantic, 2 sites)

Proposed home: `one shared `normalize_ws` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/conductor.rs:10864-10866` `normalize_ws`
- `tests/cli.rs:26301-26303` `normalize_ws`

#### `dup-0042` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/conductor.rs, src/eventstore/sqlite.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:11962-11964` `unit_branch`
- `src/eventstore/sqlite.rs:1323-1325` `key_expr`

#### `dup-0043` (semantic, 2 sites)

Proposed home: `one shared `unit_worktree_dir` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/conductor.rs:11974-11980` `unit_worktree_dir`
- `tests/halted_spawn_wip_recovery_periphery.rs:198-200` `unit_worktree_dir`

#### `dup-0044` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:12333-12338` `review_worktree_dir`
- `src/conductor.rs:12345-12347` `review_branch`

#### `dup-0045` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:13289-13295` `compensated`
- `src/conductor.rs:13300-13305` `plain_failure`

#### `dup-0046` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:13308-13329` `prior_criterion_unit_never_adopts_across_two_different_specs_sharing_the_same_criterion_id`
- `src/conductor.rs:13371-13388` `prior_criterion_unit_a_plain_non_compensation_failure_never_reopens_an_integrated_criterion`

#### `dup-0047` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:13332-13347` `prior_criterion_unit_still_adopts_across_two_runs_of_the_same_spec`
- `src/conductor.rs:13350-13368` `prior_criterion_unit_readopts_after_a_compensation_reverts_the_integration`

#### `dup-0048` (near, 4 sites)

Proposed home: `conductor::stub`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:14178-14185` `prompts_for`
- `src/conductor.rs:14188-14195` `dirs_for`
- `src/conductor.rs:14199-14205` `system_prompt_for`
- `src/conductor.rs:14209-14211` `title_for`

#### `dup-0049` (near, 14 sites)

Proposed home: `a new shared module (sites span 4 files: src/conductor.rs, src/eventstore/mod.rs, tests/build_env_authority_periphery.rs, tests/rigger_run_base_gate_env_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:14221-14223` `spawn_ids`
- `src/conductor.rs:34380-34382` `calls`
- `src/conductor.rs:34383-34385` `targets`
- `src/conductor.rs:34386-34388` `mutants_dirs`
- `src/conductor.rs:34392-34394` `store_fences`
- `src/conductor.rs:34395-34397` `build_cache_guards`
- `src/conductor.rs:34398-34400` `build_cache_dirs`
- `src/conductor.rs:34556-34558` `calls`
- `src/eventstore/mod.rs:202-204` `last`
- `src/eventstore/mod.rs:515-517` `recv`
- `src/eventstore/mod.rs:525-527` `try_recv`
- `src/eventstore/mod.rs:530-532` `err`
- `tests/build_env_authority_periphery.rs:378-380` `outputs`
- `tests/rigger_run_base_gate_env_periphery.rs:129-131` `outputs`

#### `dup-0050` (exact, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:14227-14234` `spawn_count`
- `src/conductor.rs:40602-40609` `count`
- `src/conductor.rs:41593-41600` `count`

#### `dup-0051` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/conductor.rs, src/eventstore/mod.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:14238-14244` `spawned`
- `src/eventstore/mod.rs:375-377` `covers`

#### `dup-0052` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/conductor.rs, src/config.rs, src/driver/replay.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:14413-14419` `agent_with_prompt`
- `src/config.rs:1717-1723` `agent`
- `src/driver/replay.rs:1257-1266` `stage`

#### `dup-0053` (exact, 4 sites)

Proposed home: `a new shared module (sites span 4 files: src/conductor.rs, tests/checkpoint_commit_hook_bypass_periphery.rs, tests/integrate_conflict_merge_periphery.rs, tests/revert_on_base_hook_bypass_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:14421-14427` `gate_def`
- `tests/checkpoint_commit_hook_bypass_periphery.rs:85-91` `gate_def`
- `tests/integrate_conflict_merge_periphery.rs:355-361` `gate_def`
- `tests/revert_on_base_hook_bypass_periphery.rs:96-102` `gate_def`

#### `dup-0054` (near, 4 sites)

Proposed home: `conductor::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:14470-14494` `coverage_gate_refuses_an_uncovered_criterion`
- `src/conductor.rs:29241-29273` `coverage_gap_flags_a_spec_defect_and_errors`
- `src/conductor.rs:29320-29354` `planner_leaving_a_gap_flags_a_spec_defect`
- `src/conductor.rs:29357-29391` `gate_only_stage_is_a_coverage_proxy_gap`

#### `dup-0055` (near, 10 sites)

Proposed home: `a new shared module (sites span 5 files: src/conductor.rs, src/driver/replay.rs, tests/adoption_keys_on_criterion_periphery.rs, tests/replan_episode_identity.rs, tests/scratch_workdir_isolation_leak_guard_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:14900-14927` `supersede_cfg`
- `src/conductor.rs:21993-22015` `sha_stamp_cfg`
- `src/conductor.rs:22302-22322` `degenerate_reviewer_cfg`
- `src/conductor.rs:34458-34480` `content_cache_cfg`
- `src/conductor.rs:40506-40545` `critique_cfg`
- `src/driver/replay.rs:1726-1746` `reviewed_unit_cfg`
- `tests/adoption_keys_on_criterion_periphery.rs:305-336` `baseline_only_cfg`
- `tests/replan_episode_identity.rs:234-296` `two_episode_cfg`
- `tests/replan_episode_identity.rs:1050-1087` `resume_seam_cfg`
- `tests/scratch_workdir_isolation_leak_guard_periphery.rs:86-119` `one_unit_cfg`

#### `dup-0056` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:15783-15879` `a_later_episodes_proposal_supersedes_an_earlier_episodes_planner_unit`
- `src/conductor.rs:15942-16021` `a_planner_proposal_with_no_data_episode_field_still_supersedes_via_meta_spawn`

#### `dup-0057` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:16024-16151` `a_same_id_refine_restamps_its_episode_so_its_own_episodes_sibling_does_not_reap_it`
- `src/conductor.rs:16154-16276` `a_same_id_refine_survives_its_own_episodes_sibling_add_walked_first`
- `src/conductor.rs:16611-16705` `a_legacy_proposal_never_supersedes_an_identified_episodes_owner_even_when_logged_later`

#### `dup-0058` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:16313-16331` `append_one`
- `src/conductor.rs:16458-16475` `append_legacy`
- `src/conductor.rs:16477-16495` `append_identified`

#### `dup-0059` (exact, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:16333-16345` `shape`
- `src/conductor.rs:16497-16509` `shape`

#### `dup-0060` (near, 4 sites)

Proposed home: `conductor::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:16825-16881` `a_verbatim_copy_still_supersedes_its_baseline`
- `src/conductor.rs:16884-16958` `a_paraphrased_proposal_matches_its_baseline_by_id_not_prose`
- `src/conductor.rs:16961-17045` `a_bracketed_id_echo_with_a_paraphrase_still_supersedes_exactly_once`
- `src/conductor.rs:17048-17120` `a_stale_non_matching_id_falls_back_to_the_verbatim_prose_match`

#### `dup-0061` (semantic, 741 sites)

Proposed home: `one .rigger-relative path-composition helper`

mandatory sweep: .rigger-path string literals - 741 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/conductor.rs:17327-17327` `"the repo's own .rigger config must load"`
- `src/config.rs:831-831` `".rigger"`
- `src/config.rs:2310-2310` `".rigger/agents/sdet-author.md"`
- `src/config.rs:2311-2311` `"the shipped .rigger/agents/sdet-author.md must exist"`
- `src/config.rs:2338-2338` `".rigger/agents/sdet.md"`
- `src/config.rs:2339-2339` `"the shipped .rigger/agents/sdet.md must exist"`
- `src/config.rs:2835-2835` `".rigger"`
- `src/config.rs:2836-2836` `"create .rigger dir"`
- `src/config.rs:2868-2868` `".rigger"`
- `src/config.rs:2869-2869` `"create .rigger dir"`
- `src/config.rs:2890-2890` `".rigger"`
- `src/config.rs:2891-2891` `"create .rigger dir"`
- `src/contextgraph/sqlite.rs:4504-4504` `".rigger/workflow.yml"`
- `src/contextgraph/sqlite.rs:4512-4512` `".rigger/workflow.yml"`
- `src/contextgraph/sqlite.rs:4520-4520` `".rigger/workflow.yml"`
- `src/contextgraph/sqlite.rs:4528-4528` `".rigger/workflow.yml"`
- `src/contextgraph/sqlite.rs:4536-4536` `".rigger/workflow.yml"`
- `src/dash.rs:5073-5073` `"{root}/.rigger/events.db"`
- `src/dash.rs:5122-5122` `"/.rigger/events.db"`
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
- `src/docs.rs:630-630` `"description: Store hygiene for rigger's own state - growing .rigger/ disk usage, \
         the bloat advisory from `rigger validate`, or `rigger step`/replay running slow. \
         Read this before running `rigger reset` or touching any store file by hand.\n"`
- `src/docs.rs:638-638` `"rigger keeps three stores under `.rigger/`, and only one of them holds anything \
         durable:\n"`
- `src/docs.rs:723-723` `"`rigger graph build` folds the project's source straight into `.rigger/graph.db` - \
         no run, no `RunStarted`, nothing but the code-ingest events the fold already emits. \
         It CREATES the store when the checkout is cold (`.rigger/` does not exist yet) and \
         REFRESHES an existing store incrementally: an unchanged file re-ingests nothing, and \
         it reuses the exact same walk-and-content-key ingest authority a live run uses, so a \
         standalone build and a run can never fold the same file under two different keys.\n"`
- `src/docs.rs:739-739` `"Never force a rebuild by deleting `.rigger/graph.db` (or `events.db`) and \
         re-running `rigger graph build` on the empty result. Deleting the log throws away \
         truth that no rebuild can get back, and deleting only the graph is unnecessary work \
         `rigger graph build` already does FOR you, incrementally, without erasing anything \
         first. If lookups are empty, just run `rigger graph build`; only reach for \
         rigger-reset-store if you specifically mean to prune, not rebuild.\n"`
- `src/docs.rs:774-774` `"`rigger reindex <file>...` re-parses ONLY the named files and persists the delta to \
         the project's symbols grounding index at `.rigger/symbols/` - the fast, targeted fix \
         for an index that has drifted from files you just changed (a unit's own commit, a \
         rebase, a branch switch). It is scoped strictly to the symbols index, a DIFFERENT \
         store from the structural context graph, so it costs only the named files, never a \
         walk of the whole tree.\n"`
- `src/gate.rs:498-498` `".rigger-cache-probe-{}"`
- `src/grounder/mod.rs:458-458` `".rigger"`
- `src/grounder/symbols/store.rs:25-25` `".rigger"`
- `src/grounder/symbols/store.rs:35-35` `".rigger"`
- `src/grounder/workflowdef.rs:25-25` `".rigger/workflow.yml"`
- `src/grounder/workflowdef.rs:230-230` `".rigger"`
- `src/grounder/workflowdef.rs:499-499` `".rigger"`
- `src/grounder/workflowdef.rs:587-587` `"this project's own .rigger/workflow.yml must extract at least one event"`
- `src/ingest.rs:886-886` `".rigger"`
- `src/ingest.rs:888-888` `".rigger"`
- `src/ingest.rs:897-897` `"gw/.rigger/workflow.yml@"`
- `src/ingest.rs:933-933` `".rigger"`
- `src/ingest.rs:935-935` `".rigger"`
- `src/ingest.rs:967-967` `".rigger"`
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
- `src/main.rs:4076-4076` `"workflow: the per-project JS driver is not provisioned (looked for {}). \
         Run `rigger setup` to write the shim into .rigger/shim/ and install its \
         dependencies, then re-run `rigger workflow`."`
- `src/main.rs:7182-7182` `".rigger"`
- `src/main.rs:8928-8928` `"a `rigger step` is running right now (it holds .rigger/step.lock)"`
- `src/main.rs:10151-10151` `".rigger-workflow-provenance"`
- `src/main.rs:10395-10395` `"warning: tracked .rigger/ files have uncommitted modifications:"`
- `src/main.rs:11781-11781` `".rigger/shim/"`
- `src/main.rs:11782-11782` `".rigger/dash.url"`
- `src/main.rs:11783-11783` `".rigger/dash.marker"`
- `src/main.rs:11784-11784` `".rigger/dash.attempt"`
- `src/main.rs:11785-11785` `".rigger/store.conn"`
- `src/main.rs:11948-11948` `"minted the durable project identity in .rigger/{PROJECT_ID_FILE}: {id} \
             (commit it so a rename never orphans this project's history)"`
- `src/main.rs:11953-11953` `"scaffolded .rigger/workflow.yml"`
- `src/main.rs:11957-11957` `"scaffolded .rigger/agents/{{{}}}"`
- `src/main.rs:12237-12237` `r#"__BEGIN__
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
- `src/main.rs:12679-12679` `"imported {} agent {} from {} into .rigger/agents/ ({} kept - already present)"`
- `src/main.rs:12727-12727` `"provisioned the JS driver in .rigger/shim/ (wrote shim.mjs + package.json + \
             package-lock.json and ran npm install)"`
- `src/main.rs:12962-12962` `"kept existing .rigger/agents/{name} (import never overwrites)"`
- `src/main.rs:12990-12990` `"imported .rigger/agents/{name} (id: {id})"`
- `src/main.rs:13771-13771` `"# Scaffolded by `rigger init`. A worked plan -> implement pipeline where the\n\
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
- `src/main.rs:15086-15086` `"/.rigger/tmp/cargo-target/debug/rigger"`
- `src/main.rs:15087-15087` `"/.rigger/tmp/cargo-target/release/rigger"`
- `src/main.rs:16411-16411` `" M .rigger/workflow.yml\n\
                         M  .rigger/agents/sdet.md\n\
                         A  .rigger/agents/new.md\n\
                         D  .rigger/agents/gone.md\n\
                         ?? .rigger/events.db\n\
                         !! .rigger/shim/node_modules\n"`
- `src/main.rs:16421-16421` `".rigger/workflow.yml"`
- `src/main.rs:16422-16422` `".rigger/agents/sdet.md"`
- `src/main.rs:16423-16423` `".rigger/agents/new.md"`
- `src/main.rs:16424-16424` `".rigger/agents/gone.md"`
- `src/main.rs:17424-17424` `".rigger"`
- `src/main.rs:17428-17428` `".rigger"`
- `src/main.rs:17454-17454` `"probe/.rigger/events.db"`
- `src/main.rs:17455-17455` `"rigger-wt-x/.rigger/events.db"`
- `src/main.rs:17607-17607` `".rigger"`
- `src/main.rs:17642-17642` `"rigger-wt-unit-99-ghost-12345678/.rigger/events.db"`
- `src/main.rs:17677-17677` `"probe/.rigger/events.db"`
- `src/main.rs:17688-17688` `"shadow store: probe/.rigger/events.db (6B)"`
- `src/main.rs:17773-17773` `".rigger"`
- `src/main.rs:17840-17840` `".rigger"`
- `src/main.rs:17908-17908` `".rigger"`
- `src/main.rs:17987-17987` `".rigger"`
- `src/main.rs:18093-18093` `".rigger"`
- `src/main.rs:18665-18665` `".rigger"`
- `src/main.rs:18705-18705` `".rigger"`
- `src/main.rs:18887-18887` `".rigger"`
- `src/main.rs:18898-18898` `"must walk past the storeless worktree `.rigger/` to the repo's real store"`
- `src/main.rs:19961-19961` `".rigger"`
- `src/main.rs:19963-19963` `".rigger"`
- `src/main.rs:20018-20018` `".rigger"`
- `src/main.rs:20054-20054` `".rigger"`
- `src/main.rs:20096-20096` `".rigger"`
- `src/main.rs:20202-20202` `".rigger/store.conn beats the committed config"`
- `src/main.rs:20895-20895` `"{name} must be written into .rigger/shim/"`
- `src/main.rs:21444-21444` `".rigger/agents/"`
- `src/main.rs:21470-21470` `".rigger/dash.url"`
- `src/main.rs:21473-21473` `".rigger/dash.marker"`
- `src/main.rs:21476-21476` `".rigger/dash.attempt"`
- `src/main.rs:21485-21485` `".rigger/dash.url"`
- `src/main.rs:21489-21489` `".rigger/dash.marker"`
- `src/main.rs:21493-21493` `".rigger/dash.attempt"`
- `src/main.rs:21504-21504` `".rigger/dash.url"`
- `src/main.rs:21507-21507` `".rigger/dash.marker"`
- `src/main.rs:21510-21510` `".rigger/dash.attempt"`
- `src/main.rs:21519-21519` `".rigger/dash.url"`
- `src/main.rs:21522-21522` `"exactly one .rigger/dash.url ignore line - no duplicate accrued, got:\n{after}"`
- `src/main.rs:21527-21527` `".rigger/dash.marker"`
- `src/main.rs:21530-21530` `"exactly one .rigger/dash.marker ignore line - no duplicate accrued, got:\n{after}"`
- `src/main.rs:21535-21535` `".rigger/dash.attempt"`
- `src/main.rs:21538-21538` `"exactly one .rigger/dash.attempt ignore line - no duplicate accrued, got:\n{after}"`
- `src/main.rs:21558-21558` `".rigger/store.conn"`
- `src/main.rs:21566-21566` `".rigger/store.conn"`
- `src/main.rs:21575-21575` `".rigger/store.conn"`
- `src/main.rs:21584-21584` `".rigger/store.conn"`
- `src/main.rs:21587-21587` `"exactly one .rigger/store.conn ignore line - no duplicate accrued, got:\n{after}"`
- `src/main.rs:21628-21628` `".rigger/\n"`
- `src/main.rs:21634-21634` `".rigger/dash.url"`
- `src/main.rs:21637-21637` `".rigger/dash.marker"`
- `src/main.rs:21640-21640` `".rigger/dash.attempt"`
- `src/main.rs:21641-21641` `"setup appends the explicit dash lines (including the round-8 attempt breadcrumb) \
             even when .rigger/ broadly covers them, so the committed .gitignore stays \
             self-contained, got: {:?}"`
- `src/main.rs:21649-21649` `".rigger/dash.url"`
- `src/main.rs:21650-21650` `".rigger/dash.marker"`
- `src/main.rs:21651-21651` `".rigger/dash.attempt"`
- `src/main.rs:21652-21652` `"all three explicit per-file dash ignore lines are present in the committed \
             .gitignore even though .rigger/ already covers them, got:\n{content}"`
- `src/main.rs:21662-21662` `".rigger/dash.url"`
- `src/main.rs:21665-21665` `".rigger/dash.marker"`
- `src/main.rs:21668-21668` `".rigger/dash.attempt"`
- `src/main.rs:21928-21928` `".rigger/agents/researcher.md"`
- `src/main.rs:21957-21957` `".rigger/agents/planner.md"`
- `src/main.rs:21987-21987` `".rigger/agents/newcomer.md"`
- `src/main.rs:22035-22035` `".rigger/agents/my-planner.md"`
- `src/main.rs:22060-22060` `".rigger/agents/a-dup.md"`
- `src/main.rs:22061-22061` `".rigger/agents/b-dup.md"`
- `src/main.rs:22088-22088` `".rigger/agents/blank.md"`
- `src/main.rs:22103-22103` `".rigger/workflow.yml"`
- `src/main.rs:23151-23151` `"locate_shim must return the provisioned .rigger/shim/shim.mjs"`
- `src/main.rs:24293-24293` `".rigger"`
- `src/main.rs:24347-24347` `".rigger"`
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
- `src/worktree.rs:1567-1567` `"{}/.rigger/tmp"`
- `src/worktree.rs:4538-4538` `"{repo_path}/.rigger/tmp"`
- `src/worktree.rs:4539-4539` `"the default must no longer nest inside the repo's own .rigger: {dflt:?}"`
- `src/worktree.rs:4542-4542` `"/.rigger/"`
- `src/worktree.rs:4542-4542` `"/.rigger"`
- `src/worktree.rs:4543-4543` `"the default must never live under any .rigger: {dflt:?}"`
- `src/worktree.rs:4562-4562` `"{repo_path}/.rigger/tmp"`
- `src/worktree.rs:4586-4586` `"~/.rigger-scratch-test"`
- `src/worktree.rs:4587-4587` `"{home}/.rigger-scratch-test"`
- `src/worktree.rs:6398-6398` `"{base}..rigger-run"`
- `src/worktree.rs:6447-6447` `".rigger"`
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
- `tests/checkpoint_commit_hook_bypass_periphery.rs:173-173` `"{repo_path}/.rigger-test-scratch"`
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
- `tests/cli.rs:2785-2785` `".rigger"`
- `tests/cli.rs:2826-2826` `".rigger"`
- `tests/cli.rs:2858-2858` `".rigger"`
- `tests/cli.rs:2859-2859` `"graph build must create .rigger/graph.db from a cold checkout"`
- `tests/cli.rs:2909-2909` `".rigger"`
- `tests/cli.rs:2910-2910` `"graph build must create .rigger/graph.db even when there is nothing to ingest"`
- `tests/cli.rs:2933-2933` `".rigger"`
- `tests/cli.rs:3011-3011` `".rigger"`
- `tests/cli.rs:3026-3026` `".rigger"`
- `tests/cli.rs:3074-3074` `".rigger"`
- `tests/cli.rs:3099-3099` `".rigger"`
- `tests/cli.rs:3551-3551` `".rigger"`
- `tests/cli.rs:3555-3555` `"grounding via symbols must persist the structural index to .rigger/symbols/"`
- `tests/cli.rs:3728-3728` `".rigger"`
- `tests/cli.rs:4004-4004` `".rigger"`
- `tests/cli.rs:4149-4149` `".rigger"`
- `tests/cli.rs:4356-4356` `"cd ${REPO} && CARGO_TARGET_DIR=${REPO}/.rigger/tmp/cargo-target rigger step"`
- `tests/cli.rs:4404-4404` `"cd ${REPO} && CARGO_TARGET_DIR=${REPO}/.rigger/tmp/cargo-target rigger step"`
- `tests/cli.rs:4591-4591` `".rigger/tmp/agent-scratch/"`
- `tests/cli.rs:4597-4597` `".rigger/tmp/cargo-target"`
- `tests/cli.rs:4605-4605` `"CARGO_TARGET_DIR=${REPO}/.rigger/tmp/cargo-target rigger step"`
- `tests/cli.rs:4853-4853` `".rigger"`
- `tests/cli.rs:4882-4882` `".rigger"`
- `tests/cli.rs:4992-4992` `".rigger"`
- `tests/cli.rs:5108-5108` `".rigger"`
- `tests/cli.rs:5211-5211` `".rigger"`
- `tests/cli.rs:5403-5403` `".rigger"`
- `tests/cli.rs:5434-5434` `".rigger"`
- `tests/cli.rs:5877-5877` `".rigger"`
- `tests/cli.rs:6047-6047` `".rigger"`
- `tests/cli.rs:6117-6117` `".rigger"`
- `tests/cli.rs:6255-6255` `".rigger"`
- `tests/cli.rs:6312-6312` `".rigger"`
- `tests/cli.rs:6518-6518` `".rigger"`
- `tests/cli.rs:6572-6572` `".rigger"`
- `tests/cli.rs:6742-6742` `".rigger"`
- `tests/cli.rs:6848-6848` `".rigger"`
- `tests/cli.rs:6927-6927` `".rigger"`
- `tests/cli.rs:7060-7060` `".rigger"`
- `tests/cli.rs:7117-7117` `".rigger"`
- `tests/cli.rs:7302-7302` `".rigger"`
- `tests/cli.rs:7375-7375` `".rigger"`
- `tests/cli.rs:7481-7481` `".rigger"`
- `tests/cli.rs:7550-7550` `".rigger"`
- `tests/cli.rs:7652-7652` `".rigger"`
- `tests/cli.rs:7718-7718` `".rigger"`
- `tests/cli.rs:7852-7852` `".rigger"`
- `tests/cli.rs:7920-7920` `".rigger"`
- `tests/cli.rs:8051-8051` `".rigger"`
- `tests/cli.rs:8151-8151` `".rigger/events.db"`
- `tests/cli.rs:8350-8350` `".rigger"`
- `tests/cli.rs:8384-8384` `".rigger"`
- `tests/cli.rs:8612-8612` `".rigger"`
- `tests/cli.rs:8702-8702` `".rigger"`
- `tests/cli.rs:8765-8765` `"/.rigger/"`
- `tests/cli.rs:8766-8766` `"the default marker path must never live under any .rigger; got: {marker_str:?}"`
- `tests/cli.rs:9050-9050` `".rigger"`
- `tests/cli.rs:9350-9350` `"/.rigger/tmp/agent-live/"`
- `tests/cli.rs:9351-9351` `"under RIGGER_TMPDIR the marker is not under the repo's .rigger/tmp; got: {marker_str:?}"`
- `tests/cli.rs:10132-10132` `".rigger"`
- `tests/cli.rs:10295-10295` `".rigger"`
- `tests/cli.rs:10399-10399` `".rigger"`
- `tests/cli.rs:10448-10448` `".rigger"`
- `tests/cli.rs:10550-10550` `".rigger"`
- `tests/cli.rs:10609-10609` `".rigger"`
- `tests/cli.rs:10713-10713` `".rigger"`
- `tests/cli.rs:10780-10780` `".rigger"`
- `tests/cli.rs:10922-10922` `".rigger"`
- `tests/cli.rs:10970-10970` `".rigger"`
- `tests/cli.rs:11088-11088` `".rigger"`
- `tests/cli.rs:11359-11359` `".rigger"`
- `tests/cli.rs:11458-11458` `".rigger"`
- `tests/cli.rs:11547-11547` `".rigger"`
- `tests/cli.rs:11587-11587` `".rigger"`
- `tests/cli.rs:12244-12244` `".rigger"`
- `tests/cli.rs:12406-12406` `".rigger"`
- `tests/cli.rs:12785-12785` `".rigger/workflow.yml"`
- `tests/cli.rs:12785-12785` `".rigger/agents"`
- `tests/cli.rs:12850-12850` `".rigger"`
- `tests/cli.rs:12884-12884` `".rigger/workflow.yml"`
- `tests/cli.rs:12884-12884` `".rigger/agents"`
- `tests/cli.rs:12887-12887` `".rigger/workflow.yml"`
- `tests/cli.rs:12919-12919` `".rigger"`
- `tests/cli.rs:12953-12953` `".rigger/workflow.yml"`
- `tests/cli.rs:12953-12953` `".rigger/agents"`
- `tests/cli.rs:12956-12956` `".rigger/workflow.yml"`
- `tests/cli.rs:12991-12991` `".rigger"`
- `tests/cli.rs:13030-13030` `".rigger/workflow.yml"`
- `tests/cli.rs:13030-13030` `".rigger/agents"`
- `tests/cli.rs:13033-13033` `".rigger/workflow.yml"`
- `tests/cli.rs:13067-13067` `".rigger"`
- `tests/cli.rs:13105-13105` `".rigger/workflow.yml"`
- `tests/cli.rs:13105-13105` `".rigger/agents"`
- `tests/cli.rs:13108-13108` `".rigger/workflow.yml"`
- `tests/cli.rs:13163-13163` `".rigger/workflow.yml"`
- `tests/cli.rs:13163-13163` `".rigger/agents"`
- `tests/cli.rs:13210-13210` `".rigger/workflow.yml"`
- `tests/cli.rs:13210-13210` `".rigger/agents"`
- `tests/cli.rs:13223-13223` `".rigger"`
- `tests/cli.rs:13232-13232` `".rigger"`
- `tests/cli.rs:13234-13234` `".rigger"`
- `tests/cli.rs:13331-13331` `".rigger/workflow.yml"`
- `tests/cli.rs:13332-13332` `"validate must NOT flag a clean tracked `.rigger/` tree; stderr:\n{err}"`
- `tests/cli.rs:13341-13341` `".rigger"`
- `tests/cli.rs:13348-13348` `"validate must still succeed (exit 0) when it only FLAGS uncommitted `.rigger/` \
         changes; stderr:\n{err}"`
- `tests/cli.rs:13352-13352` `".rigger/workflow.yml"`
- `tests/cli.rs:13353-13353` `"validate must flag the tracked-but-modified `.rigger/workflow.yml` on stderr; \
         stderr:\n{err}"`
- `tests/cli.rs:13369-13369` `".rigger"`
- `tests/cli.rs:13614-13614` `".rigger"`
- `tests/cli.rs:14031-14031` `".rigger"`
- `tests/cli.rs:14175-14175` `".rigger"`
- `tests/cli.rs:14177-14177` `".rigger"`
- `tests/cli.rs:14180-14180` `".rigger"`
- `tests/cli.rs:14182-14182` `".rigger"`
- `tests/cli.rs:14210-14210` `"probe/.rigger/events.db"`
- `tests/cli.rs:14274-14274` `"validate must warn about residue planted under the relocated cache-home DEFAULT \
         root - a regression that left its residue scan still rooted at the pre-relocation \
         `.rigger/tmp` would silently miss this and print nothing; stderr:\n{err}"`
- `tests/cli.rs:14304-14304` `".rigger"`
- `tests/cli.rs:15224-15224` `".rigger"`
- `tests/cli.rs:15321-15321` `"scaffolded .rigger/workflow.yml"`
- `tests/cli.rs:15325-15325` `"scaffolded .rigger/agents/"`
- `tests/cli.rs:15357-15357` `".rigger"`
- `tests/cli.rs:15389-15389` `".rigger/agents/researcher.md"`
- `tests/cli.rs:15390-15390` `"the agent was actually imported into .rigger/agents/"`
- `tests/cli.rs:15437-15437` `".rigger"`
- `tests/cli.rs:15666-15666` `".rigger/dash.url"`
- `tests/cli.rs:15670-15670` `".rigger/dash.marker"`
- `tests/cli.rs:15674-15674` `".rigger/dash.attempt"`
- `tests/cli.rs:15683-15683` `".rigger"`
- `tests/cli.rs:15685-15685` `".rigger"`
- `tests/cli.rs:15689-15689` `".rigger"`
- `tests/cli.rs:15690-15690` `".rigger"`
- `tests/cli.rs:15692-15692` `".rigger/dash.url"`
- `tests/cli.rs:15693-15693` `".rigger/dash.marker"`
- `tests/cli.rs:15694-15694` `".rigger/dash.attempt"`
- `tests/cli.rs:15734-15734` `".claude/\n.rigger/\n"`
- `tests/cli.rs:15750-15750` `".rigger/dash.url"`
- `tests/cli.rs:15758-15758` `"the test's global config must actually ignore .rigger/dash.url (else the regression \
         guard is inconclusive)"`
- `tests/cli.rs:15781-15781` `".rigger/shim"`
- `tests/cli.rs:15782-15782` `".rigger/dash.url"`
- `tests/cli.rs:15783-15783` `".rigger/dash.marker"`
- `tests/cli.rs:15784-15784` `".rigger/dash.attempt"`
- `tests/cli.rs:15988-15988` `".rigger/project.id"`
- `tests/cli.rs:15991-15991` `".rigger/project.id"`
- `tests/cli.rs:16004-16004` `".rigger/project.id"`
- `tests/cli.rs:16093-16093` `".rigger/project.id"`
- `tests/cli.rs:16142-16142` `".rigger/project.id"`
- `tests/cli.rs:16147-16147` `".rigger/project.id"`
- `tests/cli.rs:16161-16161` `".rigger"`
- `tests/cli.rs:16304-16304` `".rigger"`
- `tests/cli.rs:16423-16423` `".rigger"`
- `tests/cli.rs:16515-16515` `".rigger"`
- `tests/cli.rs:16580-16580` `".rigger"`
- `tests/cli.rs:16650-16650` `".rigger"`
- `tests/cli.rs:17014-17014` `".rigger"`
- `tests/cli.rs:17082-17082` `".rigger"`
- `tests/cli.rs:17219-17219` `".rigger"`
- `tests/cli.rs:17418-17418` `".rigger"`
- `tests/cli.rs:17608-17608` `".rigger"`
- `tests/cli.rs:17790-17790` `".rigger"`
- `tests/cli.rs:17835-17835` `".rigger"`
- `tests/cli.rs:18273-18273` `".rigger"`
- `tests/cli.rs:18288-18288` `"the driver never recorded a dash URL in .rigger/dash.url; stderr:\n{err}"`
- `tests/cli.rs:18333-18333` `".rigger"`
- `tests/cli.rs:18396-18396` `".rigger"`
- `tests/cli.rs:18471-18471` `".rigger"`
- `tests/cli.rs:18547-18547` `".rigger"`
- `tests/cli.rs:18631-18631` `".rigger"`
- `tests/cli.rs:18738-18738` `".rigger"`
- `tests/cli.rs:19454-19454` `".rigger"`
- `tests/cli.rs:19456-19456` `".rigger"`
- `tests/cli.rs:20153-20153` `".rigger/dash.marker"`
- `tests/cli.rs:20194-20194` `".rigger/dash.marker"`
- `tests/cli.rs:20197-20197` `".rigger/dash.url"`
- `tests/cli.rs:20249-20249` `".rigger/dash.url"`
- `tests/cli.rs:20254-20254` `".rigger/dash.marker"`
- `tests/cli.rs:20310-20310` `".rigger/dash.url"`
- `tests/cli.rs:20312-20312` `".rigger/dash.marker"`
- `tests/cli.rs:20381-20381` `".rigger/dash.marker"`
- `tests/cli.rs:20439-20439` `".rigger/dash.marker"`
- `tests/cli.rs:20544-20544` `".rigger/dash.marker"`
- `tests/cli.rs:20598-20598` `".rigger/dash.marker"`
- `tests/cli.rs:20620-20620` `".rigger/dash.attempt"`
- `tests/cli.rs:20630-20630` `"a marker that LOOKS like it predates this run's own RunStarted must still be reported \
         when .rigger/dash.attempt explicitly names this exact run - proving watch_poll's own \
         file-read-and-match wiring (not merely the pure watch::detect fallback comparison, \
         which alone would suppress this exact shape) is what forced the report; got:\n{out}"`
- `tests/cli.rs:20678-20678` `".rigger/dash.marker"`
- `tests/cli.rs:20700-20700` `".rigger/dash.attempt"`
- `tests/cli.rs:21426-21426` `".rigger"`
- `tests/cli.rs:22241-22241` `".rigger"`
- `tests/cli.rs:22582-22582` `".rigger"`
- `tests/cli.rs:22617-22617` `".rigger"`
- `tests/cli.rs:22692-22692` `"the first step must record a dash marker at .rigger/dash.marker; stderr:\n{err1}"`
- `tests/cli.rs:22773-22773` `"the step must record a dash marker at .rigger/dash.marker; stderr:\n{err}"`
- `tests/cli.rs:22787-22787` `"`rigger step` under RIGGER_DASH_PORT={dash_port} must bind its step-path dash at EXACTLY \
         that port and record it in .rigger/dash.marker (proving the override reaches the real \
         bind, not the fixed dash::DEFAULT_PORT); the marker instead recorded {marker_port}"`
- `tests/cli.rs:22848-22848` `".rigger"`
- `tests/cli.rs:22929-22929` `".rigger"`
- `tests/cli.rs:23000-23000` `".rigger"`
- `tests/cli.rs:23372-23372` `".rigger"`
- `tests/cli.rs:23384-23384` `".rigger"`
- `tests/cli.rs:23411-23411` `".rigger"`
- `tests/cli.rs:23439-23439` `".rigger"`
- `tests/cli.rs:23450-23450` `".rigger"`
- `tests/cli.rs:23487-23487` `".rigger"`
- `tests/cli.rs:23576-23576` `"the first step must record a dash marker at .rigger/dash.marker; stderr:\n{err1}"`
- `tests/cli.rs:23648-23648` `"{root}/.rigger/events.db"`
- `tests/cli.rs:23665-23665` `"{root}/.rigger/events.db"`
- `tests/cli.rs:24556-24556` `"{other_root}/.rigger/events.db"`
- `tests/cli.rs:24935-24935` `"/stale/root/.rigger/events.db"`
- `tests/cli.rs:24956-24956` `"/live/root/.rigger/events.db"`
- `tests/cli.rs:25058-25058` `"the step must record a dash marker at .rigger/dash.marker"`
- `tests/cli.rs:25258-25258` `".rigger"`
- `tests/cli.rs:25440-25440` `".rigger"`
- `tests/cli.rs:25543-25543` `".rigger"`
- `tests/cli.rs:25668-25668` `".rigger"`
- `tests/cli.rs:26310-26310` `".rigger"`
- `tests/cli.rs:26329-26329` `".rigger/workflow.yml must define a `checkin:` stage (spec 91): {text:?}"`
- `tests/cli.rs:26333-26333` `".rigger/workflow.yml must define a `mutation:` gate that invokes cargo mutants \
         (spec 91): {text:?}"`
- `tests/cli.rs:26338-26338` `".rigger/workflow.yml's checkin stage / mutation gate definition must name spec 91, \
         so drift in the committed workflow fails this suite instead of silently diverging \
         from the spec it satisfies: {text:?}"`
- `tests/cli.rs:26364-26364` `"this repository's own .rigger/workflow.yml and agents must load: {e}"`
- `tests/cli.rs:26371-26371` `".rigger/workflow.yml must define a `checkin` stage (spec 91)"`
- `tests/cli.rs:26406-26406` `".rigger/workflow.yml must define a `mutation` gate (spec 91)"`
- `tests/cli.rs:26427-26427` `"this repository's own committed .rigger/workflow.yml must pass Config::validate \
         on a correctly-provisioned machine (cargo-mutants installed)"`
- `tests/cli.rs:26472-26472` `".rigger/dash.attempt"`
- `tests/cli.rs:26473-26473` `"a real step's own ensure_run_dashboard call must record .rigger/dash.attempt \
         (record_dash_attempt); without it this test cannot exercise the round-8 fact at all"`
- `tests/cli.rs:26530-26530` `".rigger/dash.url"`
- `tests/cli.rs:26538-26538` `".rigger/dash.marker"`
- `tests/cli.rs:26602-26602` `".rigger/dash.marker"`
- `tests/cli.rs:26631-26631` `".rigger/dash.url"`
- `tests/cli.rs:26638-26638` `".rigger/dash.attempt"`
- `tests/cli.rs:26699-26699` `".rigger/dash.marker"`
- `tests/cli.rs:26702-26702` `".rigger/dash.url"`
- `tests/cli.rs:26756-26756` `".rigger/dash.marker"`
- `tests/cli.rs:26778-26778` `".rigger/dash.url"`
- `tests/cli.rs:26783-26783` `".rigger/dash.attempt"`
- `tests/cli.rs:26822-26822` `".rigger/dash.url"`
- `tests/cli.rs:26833-26833` `".rigger/dash.marker"`
- `tests/cli.rs:26869-26869` `".rigger/dash.url"`
- `tests/cli.rs:26877-26877` `".rigger/dash.marker"`
- `tests/cli.rs:26929-26929` `".rigger/dash.url"`
- `tests/cli.rs:26934-26934` `".rigger/dash.marker"`
- `tests/cli.rs:26996-26996` `".rigger/dash.url"`
- `tests/cli.rs:27004-27004` `".rigger/dash.marker"`
- `tests/cli.rs:27257-27257` `".rigger"`
- `tests/cli.rs:27965-27965` `".rigger"`
- `tests/cli.rs:28003-28003` `".rigger"`
- `tests/cli.rs:28937-28937` `".rigger"`
- `tests/cli.rs:29069-29069` `"the hook must be inert on a project without .rigger/; got:\n{out}"`
- `tests/cli.rs:29075-29075` `".rigger"`
- `tests/cli.rs:29094-29094` `".rigger"`
- `tests/cli.rs:29124-29124` `".rigger"`
- `tests/cli.rs:29155-29155` `".rigger"`
- `tests/cli.rs:29204-29204` `".rigger"`
- `tests/cli.rs:29242-29242` `".rigger"`
- `tests/cli.rs:29276-29276` `".rigger"`
- `tests/cli.rs:29301-29301` `".rigger"`
- `tests/cli.rs:29332-29332` `".rigger"`
- `tests/cli.rs:29353-29353` `".rigger"`
- `tests/cli.rs:29375-29375` `".rigger"`
- `tests/cli.rs:29393-29393` `".rigger"`
- `tests/cli.rs:29421-29421` `".rigger"`
- `tests/cli.rs:29444-29444` `".rigger"`
- `tests/cli.rs:29468-29468` `".rigger"`
- `tests/cli.rs:29507-29507` `".rigger"`
- `tests/cli.rs:29546-29546` `".rigger"`
- `tests/cli.rs:29854-29854` `".rigger"`
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
- `tests/graph_around_code_first.rs:45-45` `".rigger"`
- `tests/graph_around_code_first.rs:136-136` `".rigger"`
- `tests/graph_around_governance_boundaries.rs:52-52` `".rigger"`
- `tests/graph_around_governance_boundaries.rs:155-155` `".rigger"`
- `tests/graph_around_governance_boundaries.rs:187-187` `".rigger"`
- `tests/graph_around_governance_boundaries.rs:232-232` `".rigger"`
- `tests/graph_around_governance_boundaries.rs:251-251` `".rigger"`
- `tests/graph_around_governance_boundaries.rs:305-305` `".rigger"`
- `tests/graph_show_periphery.rs:87-87` `".rigger"`
- `tests/graph_show_periphery.rs:140-140` `".rigger"`
- `tests/graph_show_staleness.rs:72-72` `".rigger"`
- `tests/graph_show_staleness.rs:78-78` `".rigger"`
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
- `tests/integrate_conflict_merge_periphery.rs:413-413` `".rigger"`
- `tests/integrate_conflict_merge_periphery.rs:414-414` `"create .rigger/agents"`
- `tests/integrate_conflict_merge_periphery.rs:484-484` `"the project's own .rigger/workflow.yml must load through the real loader"`
- `tests/integrate_conflict_merge_periphery.rs:637-637` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:853-853` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:1091-1091` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:1327-1327` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:1568-1568` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:1778-1778` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:1970-1970` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:2206-2206` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:2341-2341` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:2507-2507` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:2716-2716` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:2945-2945` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:3055-3055` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:3208-3208` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:3556-3556` `"{repo_path}/.rigger-test-scratch"`
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
- `tests/reset_derived_compaction_periphery.rs:1360-1360` `".rigger"`
- `tests/reset_derived_compaction_periphery.rs:1366-1366` `".rigger"`
- `tests/reset_derived_compaction_periphery.rs:3291-3291` `".rigger"`
- `tests/reset_derived_compaction_periphery.rs:3291-3291` `"create .rigger"`
- `tests/reset_derived_compaction_periphery.rs:3305-3305` `".rigger"`
- `tests/reset_derived_compaction_periphery.rs:3340-3340` `".rigger"`
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
- `tests/simplification_audit.rs:4155-4155` `"| Conductor orchestration: gates, courier, step/run lifecycle | 19 | 8,257 | \
        the `courier_registry_refresh_{boundary,fence,periphery}` trio (3 files) \
        pair together in 6 clusters confined to just themselves (2-5 sites each; \
        excludes the whole-codebase Command::new/`.rigger`-path mandatory-sweep \
        clusters, section 2, that also happen to intersect them) |\n"`
- `tests/simplification_audit.rs:4469-4469` `"Largest risk-reduction first is read as six tiers, ranked by the KIND of risk \
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
- `tests/simplification_audit.rs:4776-4776` `"#### 10. Consolidate the 655 `.rigger`-path string-literal sites (`dup-0056`) - the \
        single largest cluster in the entire catalog by site count\n\n"`
- `tests/simplification_audit.rs:4780-4780` `"- Scope: one `.rigger`-relative path-composition helper (the cluster's own \
        `proposed_home`) every one of the 662 sites routes through instead of building its \
        own literal.\n\
        - Files: spans dozens of files including `src/conductor.rs`, `src/config.rs`, \
        `src/dash.rs`, `src/docs.rs`, `src/gate.rs`, `src/grounder/mod.rs`, \
        `src/grounder/symbols/store.rs`, `src/ingest.rs`, `src/main.rs`, `src/reap.rs`, \
        `src/registry.rs`, `src/worktree.rs` plus many `tests/` files - the full site list \
        is in the committed `docs/audit/duplication-catalog.json` under `dup-0056` for the \
        follow-up spec to consume directly, not re-enumerated here.\n\
        - Expected line delta: negative - 662 literal compositions collapse toward one \
        helper's call sites; the helper itself is small.\n\
        - Risk: medium - the largest surface-area sweep in this plan by site count, even \
        though each individual site is trivial; needs a mechanical rewrite pass plus a \
        full-suite green run, not hand-editing 662 sites.\n\
        - Unblocks: the biggest single site-count reduction available anywhere in the \
        duplication catalog.\n\n"`
- `tests/simplification_audit.rs:7913-7913` `"fn a() {\n    let _ = \".rigger/tmp\";\n}\n"`
- `tests/simplification_audit.rs:7916-7916` `".rigger"`
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
- `tests/validate_advisories.rs:245-245` `".rigger"`
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
- `tests/workflow_definition_and_js_constants_periphery.rs:154-154` `".rigger"`
- `tests/workflow_definition_and_js_constants_periphery.rs:156-156` `".rigger"`
- `tests/workflow_definition_and_js_constants_periphery.rs:429-429` `".rigger"`
- `tests/workflow_definition_and_js_constants_periphery.rs:431-431` `".rigger"`
- `tests/workflow_definition_and_js_constants_periphery.rs:679-679` `".rigger"`
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

#### `dup-0062` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:18039-18059` `render_capped_findings`
- `src/conductor.rs:18281-18300` `render_capped_lessons`
- `src/conductor.rs:20732-20754` `render_capped_lessons_scoped`

#### `dup-0063` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:18062-18129` `findings_prompt_injection_is_capped_under_budget_with_elision_note`
- `src/conductor.rs:20620-20686` `lessons_prompt_injection_is_capped_under_budget_with_elision_note`

#### `dup-0064` (exact, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:19983-19990` `read_stream`
- `src/conductor.rs:27056-27063` `read_stream`

#### `dup-0065` (exact, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:19991-19998` `read_all`
- `src/conductor.rs:27064-27071` `read_all`

#### `dup-0066` (exact, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:19999-20005` `subscribe_all`
- `src/conductor.rs:27072-27078` `subscribe_all`

#### `dup-0067` (exact, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:20006-20012` `subscribe_stream`
- `src/conductor.rs:27079-27085` `subscribe_stream`

#### `dup-0068` (exact, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:20035-20037` `resolve`
- `src/conductor.rs:35236-35238` `resolve`

#### `dup-0069` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:20424-20464` `a_subgraph_with_no_design_intent_renders_no_design_intent_header`
- `src/conductor.rs:20587-20617` `a_subgraph_with_no_code_definitions_renders_no_code_neighborhood_header`

#### `dup-0070` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:20920-20937` `commit_on_unit_branch`
- `src/conductor.rs:21140-21156` `commit_on_named_branch`

#### `dup-0071` (near, 4 sites)

Proposed home: `conductor::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:20940-21035` `resume_reuses_a_units_branch_instead_of_reimplementing`
- `src/conductor.rs:23167-23252` `resume_integrates_an_already_approved_unit_without_re_reviewing`
- `src/conductor.rs:23255-23366` `a_failed_unit_is_not_terminal_and_resumes`
- `src/conductor.rs:31340-31443` `a_resumed_reviewed_unit_restores_a_worktree_the_exhaustive_gate_deleted`

#### `dup-0072` (near, 4 sites)

Proposed home: `conductor::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:21038-21132` `branch_gc_reclaims_integrated_units_and_retains_escalated_ones_on_resume`
- `src/conductor.rs:21171-21227` `branch_gc_falls_back_to_the_derived_branch_when_unitstarted_recorded_no_branch`
- `src/conductor.rs:21449-21541` `branch_gc_reclaims_every_integrated_unit_in_one_resume_not_just_the_first`
- `src/conductor.rs:21544-21625` `branch_gc_fences_reclaim_behind_an_in_flight_straggler_spawn_and_reclaims_once_it_answers`

#### `dup-0073` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:21628-21695` `gc_integrated_branches_logged_prints_kept_evidence_for_an_in_flight_straggler_spawn`
- `src/conductor.rs:21800-21883` `gc_integrated_branches_logged_stays_silent_for_an_already_gone_worktree_but_still_reclaims_an_orphaned_branch`

#### `dup-0074` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:21698-21797` `gc_integrated_branches_logged_prints_removing_evidence_for_a_terminal_spawns_decision`
- `src/conductor.rs:21886-21989` `gc_integrated_branches_logged_does_not_repeat_removing_evidence_once_the_real_removal_already_happened`

#### `dup-0075` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:22745-22790` `a_gating_spawn_that_returns_a_reject_verdict_line_is_a_normal_reject_even_if_it_emitted_approve`
- `src/conductor.rs:22793-22837` `a_gating_spawn_with_no_verdict_line_and_no_emitted_approve_is_an_ordinary_reject`

#### `dup-0076` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:23691-23733` `scope_creep_refuses_a_criterionless_proposed_unit`
- `src/conductor.rs:29276-29317` `planner_covering_every_criterion_passes`

#### `dup-0077` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:23772-23838` `adversary_runs_between_the_lenses_and_the_adjudicator`
- `src/conductor.rs:23894-23987` `unit_reviews_itself_within_its_own_lifecycle`

#### `dup-0078` (near, 4 sites)

Proposed home: `conductor::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:24270-24330` `a_low_risk_unit_skips_the_adversary_and_extra_lens`
- `src/conductor.rs:24333-24403` `a_high_risk_unit_runs_the_full_panel_and_logs_the_routing`
- `src/conductor.rs:24424-24502` `a_zero_grounding_unit_fails_safe_to_the_full_panel_and_logs_it`
- `src/conductor.rs:24617-24681` `a_stage_level_tiers_policy_routes_the_unit_by_risk`

#### `dup-0079` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:26253-26315` `planner_proposed_unit_inherits_the_default_review_panel`
- `src/conductor.rs:26318-26413` `a_producer_stage_skips_the_three_tier_review_and_unblocks_its_dependents`

#### `dup-0080` (semantic, 8 sites)

Proposed home: `one error-shaping helper module`

mandatory sweep: error-shaping helper functions - 8 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/conductor.rs:27089-27147` `integrate_plan_commits_wraps_any_hard_error_with_the_plan_landing_marker`
- `src/grounder/mod.rs:259-265` `retired_grounder_error`
- `src/worktree.rs:4222-4276` `revert_on_base_aborts_and_errors_on_a_conflicting_revert`
- `tests/adoption_keys_on_criterion_periphery.rs:2425-2597` `a_quarantine_record_whose_ref_was_since_deleted_hard_errors_instead_of_silently_starting_fresh`
- `tests/batched_fold_cadence.rs:184-262` `append_and_fold_batch_is_best_effort_on_fold_error_and_a_no_op_on_an_empty_batch`
- `tests/canary_item_sharding_jobs_cap_periphery.rs:352-440` `run_canary_runs_every_item_to_completion_even_when_one_items_spawn_errors`
- `tests/grounder_name_contract.rs:40-171` `public_name_contract_predicate_error_and_resolver_agree`
- `tests/integrate_conflict_merge_periphery.rs:1759-1867` `a_non_content_merge_failure_surfaces_as_a_run_error_leaving_branches_intact`

#### `dup-0081` (near, 5 sites)

Proposed home: `conductor::support (consolidate these 5 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:27235-27307` `per_unit_adjudicator_reject_blocks_integration_and_escalates`
- `src/conductor.rs:27835-27908` `approval_on_the_final_permitted_attempt_integrates_a_per_unit_stage`
- `src/conductor.rs:28004-28070` `approval_on_the_final_permitted_attempt_integrates_a_standalone_review_stage`
- `src/conductor.rs:29958-30005` `on_pass_none_runs_gates_but_does_not_integrate`
- `src/conductor.rs:33429-33474` `unparseable_adjudicator_output_blocks_integration`

#### `dup-0082` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:27406-27503` `a_higher_max_retries_gives_more_attempts_before_escalation`
- `src/conductor.rs:27420-27466` `escalation_run`

#### `dup-0083` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:28073-28107` `mid_spawn_crash_escalates_without_aborting_the_run`
- `src/conductor.rs:28110-28161` `a_newly_escalated_unit_stamps_an_attention_entry`

#### `dup-0084` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:28164-28214` `a_budget_halt_stamps_an_attention_entry`
- `src/conductor.rs:28977-29015` `a_budget_halt_surfaces_its_reason_on_the_run_state`

#### `dup-0085` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:28309-28413` `a_delayed_budget_halt_after_a_dependency_unlocks_still_stamps`
- `src/conductor.rs:28490-28571` `an_escalation_does_not_restamp_attention_on_a_resumed_process`
- `src/conductor.rs:28610-28734` `a_second_failure_recurs_and_a_third_also_stalls_the_frontier`

#### `dup-0086` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:28877-28928` `budget_breaker_stops_the_run_after_the_first_wave`
- `src/conductor.rs:28931-28974` `budget_exhaustion_aborts_the_task`
- `src/conductor.rs:29394-29453` `manual_stage_pauses_while_an_auto_stage_integrates`

#### `dup-0087` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:29053-29121` `the_spawn_budget_folds_from_recorded_spawn_requests_across_steps`
- `src/conductor.rs:29124-29165` `the_pre_wave_breaker_trips_only_on_a_new_over_budget_spawn`

#### `dup-0088` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:29456-29499` `isolation_none_agent_gets_no_worktree_even_with_a_repo`
- `src/conductor.rs:29502-29535` `spawn_opts_isolation_is_set_for_a_worktree_agent`
- `src/conductor.rs:29538-29577` `a_spawned_implementers_title_is_the_unit_criterion`

#### `dup-0089` (near, 4 sites)

Proposed home: `conductor::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:29584-29632` `the_adversarys_spawn_is_stamped_with_the_units_lens_roster`
- `src/conductor.rs:29638-29687` `the_adjudicators_spawn_is_stamped_with_lenses_plus_adversary`
- `src/conductor.rs:29692-29739` `a_panel_with_no_adversary_never_fabricates_one_in_the_adjudicators_roster`
- `src/conductor.rs:29825-29882` `the_fan_out_review_loops_adversary_and_adjudicator_spawns_are_stamped_with_the_routed_roster`

#### `dup-0090` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:30035-30122` `two_units_gate_environments_never_share_a_target_dir`
- `src/conductor.rs:30125-30191` `two_units_gate_environments_never_share_a_mutants_root`

#### `dup-0091` (exact, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:30259-30261` `envs`
- `src/conductor.rs:34389-34391` `build_envs`

#### `dup-0092` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:30662-30768` `a_parked_review_spawn_keeps_the_unit_worktree_registered_and_its_cache`
- `src/conductor.rs:30771-30863` `review_unit_restores_a_worktree_a_gate_deleted_out_of_band`
- `src/conductor.rs:30866-30968` `a_second_step_restores_a_still_parked_units_worktree_deleted_out_of_band`

#### `dup-0093` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:31270-31337` `failed_worktree_sha_is_stamped_after_the_adjudicators_own_deletion_is_restored`
- `src/conductor.rs:32056-32127` `speculation_reject_worktree_sha_is_stamped_after_the_adjudicators_own_deletion_is_restored`

#### `dup-0094` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:32390-32433` `agent_stage_runs_the_per_unit_lifecycle_not_the_fan_out_path`
- `src/conductor.rs:32436-32477` `standalone_review_stage_still_takes_the_fan_out_path`

#### `dup-0095` (near, 6 sites)

Proposed home: `conductor::support (consolidate these 6 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:32595-32648` `a_parked_lens_keeps_the_standalone_review_stages_worktree`
- `src/conductor.rs:32651-32747` `a_parked_lens_keeps_the_review_worktree_even_beside_a_lower_indexed_sibling_crash`
- `src/conductor.rs:32750-32843` `a_parked_lens_keeps_the_review_worktree_beside_a_sibling_degenerate_halt`
- `src/conductor.rs:32846-32931` `the_swap_to_front_prioritizes_a_genuine_error_at_a_non_zero_chunk_index`
- `src/conductor.rs:32934-33021` `a_budget_refused_lens_beside_a_genuinely_crashing_sibling_in_one_chunk`
- `src/conductor.rs:33024-33091` `a_budget_refused_standalone_review_spawn_keeps_its_worktree`

#### `dup-0096` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:33541-33624` `a_flaky_gate_rerun_is_a_pass_with_warning_that_never_demotes`
- `src/conductor.rs:33627-33697` `a_product_gate_failure_is_not_rerun_and_demotes_as_before`

#### `dup-0097` (exact, 2 sites)

Proposed home: `conductor::recording_runner`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:34365-34370` `materializing`
- `src/conductor.rs:34374-34379` `deleting_worktree`

#### `dup-0098` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/conductor.rs, src/spawn.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:34484-34490` `attempt_of`
- `src/spawn.rs:194-200` `attempt_of`

#### `dup-0099` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:34603-34669` `a_matching_input_digest_answers_a_gate_as_a_logged_cache_hit_citing_the_prior_green`
- `src/conductor.rs:34717-34773` `a_red_verdict_is_never_cache_answered_and_the_gate_re_runs`

#### `dup-0100` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:34909-34982` `a_structural_grounder_records_the_audit_and_routes_full_on_a_beyond_cap_high_risk_file`
- `src/conductor.rs:35522-35586` `speculation_over_a_structural_grounder_records_the_audit_and_routes_full`

#### `dup-0101` (near, 5 sites)

Proposed home: `a new shared module (sites span 2 files: src/conductor.rs, tests/revert_on_base_hook_bypass_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:35838-35879` `spawn`
- `src/conductor.rs:36392-36434` `spawn`
- `src/conductor.rs:36888-36925` `spawn`
- `src/conductor.rs:37023-37068` `spawn`
- `tests/revert_on_base_hook_bypass_periphery.rs:133-166` `spawn`

#### `dup-0102` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:36616-36667` `commits_to_compensate_reverts_every_sha_a_multi_commit_landing_recorded`
- `src/conductor.rs:36732-36783` `commits_to_compensate_excludes_the_review_only_marker_even_alongside_a_real_sha`

#### `dup-0103` (exact, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:36929-37012` `a_compensated_unit_re_gates_its_re_implemented_tree_not_the_condemned_verdict`
- `src/conductor.rs:37072-37170` `a_compensated_unit_that_remediated_before_integrating_re_gates_at_a_fresh_key`

#### `dup-0104` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:37958-38044` `integrate_conflict_re_parks_the_implementer_with_no_attempt_charged_and_both_units_land`
- `src/conductor.rs:38047-38127` `integrate_conflict_confined_to_a_regenerable_path_resolves_with_no_spawn`

#### `dup-0105` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:38475-38543` `a_failing_deferred_gate_is_surfaced_and_the_run_is_not_done`
- `src/conductor.rs:38581-38652` `a_default_infra_fault_at_a_deferred_gate_does_not_demote`

#### `dup-0106` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:38655-38759` `a_replayed_step_re_runs_no_recorded_gate_and_appends_no_duplicate_events`
- `src/conductor.rs:38762-38847` `a_re_step_replays_a_recorded_deferred_gate_without_re_running_it`

#### `dup-0107` (near, 9 sites)

Proposed home: `a new shared module (sites span 9 files: src/conductor.rs, tests/checkpoint_commit_hook_bypass_periphery.rs, tests/gate_store_fence_periphery.rs, tests/graph_fresh_on_integration_periphery.rs, tests/integrate_conflict_merge_periphery.rs, tests/revert_on_base_hook_bypass_periphery.rs, tests/scratch_workdir_isolation_leak_guard_periphery.rs, tests/spawn_target_dir_periphery.rs, tests/unified_traversal_grounding.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:39508-39525` `init_repo`
- `tests/checkpoint_commit_hook_bypass_periphery.rs:47-64` `init_repo`
- `tests/gate_store_fence_periphery.rs:491-507` `init_repo_with_head`
- `tests/graph_fresh_on_integration_periphery.rs:56-73` `init_repo`
- `tests/integrate_conflict_merge_periphery.rs:283-300` `init_repo`
- `tests/revert_on_base_hook_bypass_periphery.rs:57-74` `init_repo`
- `tests/scratch_workdir_isolation_leak_guard_periphery.rs:63-80` `init_repo`
- `tests/spawn_target_dir_periphery.rs:55-71` `init_repo_with_head`
- `tests/unified_traversal_grounding.rs:575-592` `init_seam_repo`

#### `dup-0108` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:40175-40250` `an_empty_accounting_sdet_author_result_advances_the_build_to_commit_gates_and_integrate`
- `src/conductor.rs:40253-40317` `an_absent_sdet_author_is_a_clean_no_op_and_the_build_still_integrates`
- `src/conductor.rs:40320-40395` `a_crashed_sdet_author_spawn_does_not_block_the_build_the_lifecycle_proceeds`

#### `dup-0109` (exact, 2 sites)

Proposed home: `conductor::critique_driver`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:40588-40593` `rejecting`
- `src/conductor.rs:40596-40601` `always_rejecting`

#### `dup-0110` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:40795-40832` `a_rule_7_or_8_defect_rejects_then_releases_on_the_revision`
- `src/conductor.rs:40835-40886` `a_clean_decomposition_approves_and_releases_the_fan_out`

#### `dup-0111` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:41347-41445` `a_resumed_step_holds_the_fan_out_while_the_plan_critique_gate_is_escalated`
- `src/conductor.rs:41448-41504` `an_approved_gate_releases_planner_proposed_units_not_only_baselines`
- `src/conductor.rs:41627-41698` `a_resumed_step_holds_the_fan_out_while_the_plan_critique_gate_is_mid_review`

#### `dup-0112` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/config.rs, src/failure.rs, src/main.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config.rs:221-223` `is_empty`
- `src/failure.rs:168-170` `is_any`
- `src/main.rs:10451-10456` `is_empty`

#### `dup-0113` (near, 2 sites)

Proposed home: `config::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config.rs:973-993` `read_store_config`
- `src/config.rs:1030-1045` `read_scratch_defaults`

#### `dup-0114` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/config.rs, tests/no_os_kill_audit.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config.rs:1665-1667` `is_word_byte`
- `tests/no_os_kill_audit.rs:52-54` `is_word_char`
- `tests/simplification_audit.rs:198-200` `is_ident_char`

#### `dup-0115` (near, 3 sites)

Proposed home: `config::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config.rs:1826-1865` `verdict_line_lint_passes_an_unrelated_same_sentence_emit_before_a_verdict_output_clause`
- `src/config.rs:1877-1909` `verdict_line_lint_passes_a_determiner_verdict_when_a_payload_noun_sits_in_the_span`
- `src/config.rs:1922-1962` `verdict_line_lint_passes_a_determiner_verdict_when_an_unrelated_emit_example_brace_precedes_it`

#### `dup-0116` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/config.rs, src/main.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config.rs:1998-2031` `literal_is_emit_payload_binds_only_an_abutting_payload_or_emit_word`
- `src/main.rs:17155-17161` `is_uuid8_accepts_exactly_eight_hex_digits`
- `tests/simplification_audit.rs:7920-7928` `looks_error_shaping_matches_error_and_underscore_bounded_err_but_not_an_incidental_substring`

#### `dup-0117` (near, 2 sites)

Proposed home: `config::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config.rs:2185-2192` `parses_agent_frontmatter_and_body`
- `src/config.rs:2200-2209` `model_ladder_parses_from_frontmatter`

#### `dup-0118` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/config.rs, src/main.rs, src/spec.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config.rs:2195-2197` `rejects_missing_frontmatter`
- `src/main.rs:16432-16434` `dirty_tracked_paths_on_a_clean_tree_is_empty`
- `src/spec.rs:895-897` `empty_when_no_criteria`

#### `dup-0119` (near, 6 sites)

Proposed home: `config::support (consolidate these 6 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config.rs:2290-2299` `defaults_max_wall_clock_parses_and_is_zero_when_absent`
- `src/config.rs:2809-2825` `max_retries_parses_from_defaults_and_defaults_to_zero_when_absent`
- `src/config.rs:3074-3087` `build_config_parses_wrapper_and_cache_dir_and_defaults_when_omitted`
- `src/config.rs:3096-3111` `build_config_parses_jobs_and_defaults_to_zero_when_omitted`
- `src/config.rs:3120-3135` `build_config_parses_max_concurrent_defaulting_to_four_when_omitted`
- `src/config.rs:3183-3196` `build_config_parses_mutation_and_defaults_to_empty_when_omitted`

#### `dup-0120` (near, 2 sites)

Proposed home: `config::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config.rs:2545-2563` `validate_catches_unknown_ref`
- `src/config.rs:3388-3416` `validate_catches_cycle`

#### `dup-0121` (near, 3 sites)

Proposed home: `config::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config.rs:2655-2678` `validate_catches_an_unknown_light_panel_agent`
- `src/config.rs:2681-2705` `validate_rejects_a_light_panel_with_no_adjudicator`
- `src/config.rs:2736-2763` `validate_rejects_a_tiers_policy_on_a_full_panel_with_no_adjudicator`

#### `dup-0122` (exact, 3 sites)

Proposed home: `a new shared module (sites span 2 files: src/contextgraph/mod.rs, src/dash.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/mod.rs:581-583` `is_false`
- `src/dash.rs:1151-1153` `is_not_back`
- `src/dash.rs:1158-1160` `is_not_shared`

#### `dup-0123` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/contextgraph/mod.rs, tests/calls_down_execution_path_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/mod.rs:916-918` `apply`
- `tests/calls_down_execution_path_periphery.rs:139-141` `apply`

#### `dup-0124` (exact, 5 sites)

Proposed home: `a new shared module (sites span 4 files: src/contextgraph/mod.rs, tests/batched_fold_cadence.rs, tests/calls_down_execution_path_periphery.rs, tests/store_content_identity_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/mod.rs:919-921` `subgraph`
- `tests/batched_fold_cadence.rs:65-67` `subgraph`
- `tests/batched_fold_cadence.rs:91-93` `subgraph`
- `tests/calls_down_execution_path_periphery.rs:142-144` `subgraph`
- `tests/store_content_identity_periphery.rs:104-106` `subgraph`

#### `dup-0125` (exact, 5 sites)

Proposed home: `a new shared module (sites span 4 files: src/contextgraph/mod.rs, tests/batched_fold_cadence.rs, tests/calls_down_execution_path_periphery.rs, tests/store_content_identity_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/mod.rs:922-924` `resolve`
- `tests/batched_fold_cadence.rs:68-70` `resolve`
- `tests/batched_fold_cadence.rs:94-96` `resolve`
- `tests/calls_down_execution_path_periphery.rs:145-147` `resolve`
- `tests/store_content_identity_periphery.rs:107-109` `resolve`

#### `dup-0126` (semantic, 46 sites)

Proposed home: `one sqlite-connection-opening adapter function every caller is injected with`

mandatory sweep: sqlite Connection::open call sites - 46 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/contextgraph/sqlite.rs:94-94` `Connection::open`
- `src/contextgraph/sqlite.rs:6968-6968` `Connection::open`
- `src/contextgraph/sqlite.rs:7778-7778` `Connection::open`
- `src/eventstore/sqlite.rs:159-159` `Connection::open`
- `src/eventstore/sqlite.rs:2792-2792` `Connection::open`
- `src/eventstore/sqlite.rs:3484-3484` `Connection::open`
- `src/eventstore/sqlite.rs:3576-3576` `Connection::open`
- `src/eventstore/sqlite.rs:3833-3833` `Connection::open_with_flags`
- `src/eventstore/sqlite.rs:3851-3851` `Connection::open`
- `src/eventstore/sqlite.rs:3865-3865` `Connection::open`
- `src/main.rs:25714-25714` `Connection::open`
- `tests/cli.rs:901-901` `Connection::open`
- `tests/cli.rs:970-970` `Connection::open`
- `tests/cli.rs:1066-1066` `Connection::open`
- `tests/cli.rs:1159-1159` `Connection::open`
- `tests/cli.rs:1214-1214` `Connection::open`
- `tests/cli.rs:11096-11096` `Connection::open`
- `tests/cli.rs:17230-17230` `Connection::open`
- `tests/graph_additive_indexes_persist.rs:69-69` `Connection::open`
- `tests/graph_additive_indexes_persist.rs:165-165` `Connection::open`
- `tests/graph_additive_indexes_persist.rs:189-189` `Connection::open`
- `tests/heartbeat_write_read_agree_periphery.rs:215-215` `Connection::open`
- `tests/reset_derived_compaction.rs:132-132` `Connection::open`
- `tests/reset_derived_compaction.rs:306-306` `Connection::open`
- `tests/reset_derived_compaction.rs:571-571` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:117-117` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:568-568` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:1373-1373` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:2858-2858` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:3063-3063` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:3939-3939` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:4030-4030` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:4039-4039` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:4647-4647` `Connection::open`
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

#### `dup-0127` (near, 2 sites)

Proposed home: `sqlite::projector`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:503-618` `calls_down`
- `src/contextgraph/sqlite.rs:654-763` `calls_up`

#### `dup-0128` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/contextgraph/sqlite.rs, src/eventstore/kurrentdb.rs, src/eventstore/sqlite.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:880-884` `to_nanos`
- `src/eventstore/kurrentdb.rs:115-119` `to_nanos`
- `src/eventstore/sqlite.rs:1468-1472` `to_nanos`

#### `dup-0129` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/contextgraph/sqlite.rs, src/dash.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:1922-1927` `name_suffix`
- `src/dash.rs:1696-1701` `name_suffix`

#### `dup-0130` (exact, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:1957-1963` `tier_rank`
- `src/contextgraph/sqlite.rs:1969-1975` `tier_floor_rank`

#### `dup-0131` (near, 3 sites)

Proposed home: `sqlite::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:2042-2067` `calls_out`
- `src/contextgraph/sqlite.rs:2100-2125` `callers_direct`
- `src/contextgraph/sqlite.rs:2136-2164` `callers_via_bare`

#### `dup-0132` (near, 5 sites)

Proposed home: `a new shared module (sites span 4 files: src/contextgraph/sqlite.rs, src/dash.rs, tests/calls_down_execution_path_periphery.rs, tests/graph_fold_dedup_live_only_scoping.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:2803-2817` `apply_decision`
- `src/contextgraph/sqlite.rs:4710-4717` `apply_batch_ref_caller`
- `src/dash.rs:9906-9913` `apply_call`
- `tests/calls_down_execution_path_periphery.rs:80-87` `apply_call`
- `tests/graph_fold_dedup_live_only_scoping.rs:38-45` `apply_decision`

#### `dup-0133` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:2892-2905` `subgraph_finds_the_governing_decision`
- `src/contextgraph/sqlite.rs:8135-8155` `recording_proof_never_wipes_the_entitys_own_name_kind_and_line_attrs`

#### `dup-0134` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/contextgraph/sqlite.rs, tests/graph_fold_dedup_live_edge.rs, tests/graph_rebuild_collapses_dupes.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:3036-3044` `apply_governs_at`
- `tests/graph_fold_dedup_live_edge.rs:38-46` `apply_governs`
- `tests/graph_rebuild_collapses_dupes.rs:40-48` `apply_governs`

#### `dup-0135` (near, 10 sites)

Proposed home: `a new shared module (sites span 6 files: src/contextgraph/sqlite.rs, src/dash.rs, tests/calls_down_execution_path_periphery.rs, tests/dash_calls_route_periphery.rs, tests/graph_superseded_prune.rs, tests/reset_menu_previews_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:3327-3345` `apply_code_entity`
- `src/contextgraph/sqlite.rs:3361-3381` `apply_code_entity_partial`
- `src/contextgraph/sqlite.rs:3430-3449` `apply_community`
- `src/contextgraph/sqlite.rs:4684-4695` `apply_batch_def`
- `src/contextgraph/sqlite.rs:4978-4998` `apply_batch_def_at`
- `src/dash.rs:9894-9905` `apply_def`
- `tests/calls_down_execution_path_periphery.rs:63-74` `apply_def`
- `tests/dash_calls_route_periphery.rs:741-751` `apply_def`
- `tests/graph_superseded_prune.rs:36-48` `apply_def`
- `tests/reset_menu_previews_periphery.rs:55-67` `apply_def`

#### `dup-0136` (near, 9 sites)

Proposed home: `a new shared module (sites span 2 files: src/contextgraph/sqlite.rs, tests/dash_calls_route_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:3347-3352` `apply_edge_inferred`
- `src/contextgraph/sqlite.rs:3387-3392` `apply_edge_inferred_evidence`
- `src/contextgraph/sqlite.rs:3420-3426` `apply_ref_caller`
- `src/contextgraph/sqlite.rs:4241-4249` `apply_doc_concept`
- `src/contextgraph/sqlite.rs:4352-4360` `apply_doc_link`
- `src/contextgraph/sqlite.rs:4699-4705` `apply_batch_ref`
- `src/contextgraph/sqlite.rs:6313-6321` `apply_unit_integrated`
- `src/contextgraph/sqlite.rs:7584-7594` `apply_def`
- `tests/dash_calls_route_periphery.rs:755-761` `apply_call`

#### `dup-0137` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:4363-4486` `design_intent_link_events_fold_into_the_five_design_intent_edges`
- `src/contextgraph/sqlite.rs:4489-4607` `workflow_definition_events_fold_into_stage_gate_agent_nodes_with_needs_runs_reviews_edges`

#### `dup-0138` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:4723-4737` `edges_from`
- `src/contextgraph/sqlite.rs:7041-7059` `edges_touching`

#### `dup-0139` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:5401-5541` `calls_down_walks_the_execution_path_as_a_layered_deduped_dag_with_a_back_edge`
- `src/contextgraph/sqlite.rs:5739-5929` `calls_up_walks_the_call_sites_as_a_layered_deduped_dag_and_lists_referenced_but_not_called`

#### `dup-0140` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:6046-6079` `decision_fold_projects_no_agent_node_or_decided_edge`
- `src/contextgraph/sqlite.rs:6133-6161` `review_finding_projects_no_raised_edge_even_with_an_event_actor`

#### `dup-0141` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:6856-6865` `edge_projects`
- `src/contextgraph/sqlite.rs:7943-7955` `index_names`

#### `dup-0142` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:6868-6950` `every_node_and_edge_carries_the_projects_scope_on_fold`
- `src/contextgraph/sqlite.rs:7062-7153` `prune_is_project_scoped_leaving_another_projects_same_id_node_intact`

#### `dup-0143` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/contextgraph/sqlite.rs, tests/calls_down_execution_path_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:7597-7602` `apply_ref`
- `tests/calls_down_execution_path_periphery.rs:94-99` `apply_ref`

#### `dup-0144` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: src/contextgraph/sqlite.rs, tests/code_ingest_events.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:7682-7723` `the_cross_file_inferred_tier_is_order_independent`
- `src/contextgraph/sqlite.rs:7726-7741` `the_definition_upgrade_never_demotes_a_same_file_extracted_reference`
- `tests/code_ingest_events.rs:1013-1045` `a_definition_upgrades_only_the_exact_name_cross_file_reference_never_a_substring`

#### `dup-0145` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/contextgraph/sqlite.rs, src/main.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:7852-7854` `edge_desc`
- `src/main.rs:8569-8575` `runs_menu_line`

#### `dup-0146` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:8053-8072` `a_same_file_test_reference_increments_proven_by_and_records_its_evidence`
- `src/contextgraph/sqlite.rs:8075-8098` `two_test_references_accumulate_proven_by_to_2_with_both_evidence_entries`

#### `dup-0147` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:8177-8208` `an_unresolvable_test_reference_is_staged_and_reconciled_once_its_definition_later_folds`
- `src/contextgraph/sqlite.rs:8453-8484` `the_empty_boundary_sentinel_never_resolves_records_or_stages_anything_for_its_empty_name`

#### `dup-0148` (semantic, 60 sites)

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
- `src/main.rs:14362-14362` `"/proc"`
- `src/main.rs:14466-14466` `"/proc"`
- `src/main.rs:24894-24894` `"/proc/{pid}/stat"`
- `src/main.rs:24895-24895` `"read /proc/{pid}/stat: {e}"`
- `src/main.rs:24898-24898` `"/proc stat has a parenthesised comm field"`
- `src/main.rs:24903-24903` `"/proc stat has a pgrp field after comm"`
- `src/reap.rs:133-133` `"/proc"`
- `src/reap.rs:215-215` `"/proc/{pid}/stat"`
- `src/reap.rs:226-226` `"/proc/{pid}/status"`
- `src/reap.rs:276-276` `"/proc/{}/cwd"`
- `tests/cli.rs:22875-22875` `"/proc"`
- `tests/cli.rs:24991-24991` `"/proc/{pid}/stat"`
- `tests/cli.rs:24992-24992` `"read /proc/{pid}/stat: {e}"`
- `tests/cli.rs:24995-24995` `"/proc stat has a parenthesised comm field"`
- `tests/cli.rs:25000-25000` `"/proc stat has a pgrp field after comm"`
- `tests/cli.rs:28034-28034` `"/proc"`
- `tests/cli.rs:28119-28119` `"/proc"`
- `tests/cli.rs:28166-28166` `"/proc/{holder_pid}/stat"`
- `tests/cli.rs:28181-28181` `"the holder pid {holder_pid} never reached the STOPPED (T) state in /proc"`
- `tests/cli.rs:28247-28247` `"/proc"`
- `tests/cli.rs:28270-28270` `"/proc"`
- `tests/cli.rs:28299-28299` `"/proc"`
- `tests/cli.rs:28349-28349` `"/proc/{holder_pid}/stat"`
- `tests/cli.rs:28364-28364` `"the holder pid {holder_pid} never reached the STOPPED (T) state in /proc"`
- `tests/cli.rs:28438-28438` `"/proc"`
- `tests/cli.rs:28464-28464` `"/proc"`
- `tests/cli.rs:28562-28562` `"/proc"`
- `tests/cli.rs:28600-28600` `"held_port_holder's message half and describe_held_port_if_confirmed's own return \
             must agree (scheduler-state letter normalized away since each call independently \
             re-reads /proc and can observe a state flap) - they are documented as sharing one \
             discovery"`
- `tests/cli.rs:28619-28619` `"/proc"`
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
- `tests/simplification_audit.rs:3690-3690` `"A second mutation authority for one domain: the one previously-known \
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
- `tests/simplification_audit.rs:4469-4469` `"Largest risk-reduction first is read as six tiers, ranked by the KIND of risk \
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
- `tests/simplification_audit.rs:4601-4601` `"#### 3. Retire the duplicate `/proc`-reading authority (`dup-0148` + `dup-0149`)\n\n"`
- `tests/simplification_audit.rs:4604-4604` `"- Scope: `src/dash.rs::process_state` (`src/dash.rs:499-507`) and \
        `src/main.rs::pgid_of` (`src/main.rs:23346-23359`) each independently re-derive \
        `/proc/<pid>/stat` and `/proc/<pid>/status` fields that `src/reap.rs` \
        (`pid_starttime`/`read_ppid`, `src/reap.rs:190-207`) already parses - the exact \
        \"second mutation authority\" example spec 85's own Goal names and spec 62's \
        capstone previously caught (`dup-0149`, 15 sites: `src/dash.rs`, `src/main.rs`, \
        `src/reap.rs`, `tests/cli.rs`, `tests/mutation_runner_pdeathsig_periphery.rs` - spec \
        91's own launcher-exits proving test reads `/proc/<pid>/stat` directly for the same \
        reason `dash.rs::process_state` does, growing this already-known cluster by one site \
        rather than opening a new one), plus 60 raw `/proc`-path string literals scattered \
        across `src/dash.rs`, `src/main.rs`, `src/reap.rs` and four test files with no \
        shared composer (`dup-0148`). Both clusters' own `proposed_home` agree: `src/reap.rs` \
        becomes the one `/proc`-reading module; `dash.rs` and `main.rs` call it instead of \
        re-parsing. NOT SYMMETRIC: `process_state` is reachable from `dash`'s own always-on \
        production server, so it is the actual active-correctness risk this tier-1 placement \
        is about; `pgid_of` sits inside `main.rs`'s `mod tests` (opened at `src/main.rs:12825`) \
        and is called only by `#[test]` fns, so on its own it earns no tier-1 placement - it \
        rides in this same item only because it shares `dup-0148`/`dup-0149`'s one root cause \
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
- `tests/simplification_audit.rs:7798-7798` `"fn state_of(pid: u32) -> Option<char> {\n    let stat = std::fs::read_to_string(format!(\"/proc/{pid}/stat\")).ok()?;\n    stat.rsplit_once(')')?.1.split_whitespace().next()?.chars().next()\n}\n"`
- `tests/simplification_audit.rs:7803-7803` `"fn starttime_of(pid: u32) -> Option<u64> {\n    let stat = std::fs::read_to_string(format!(\"/proc/{pid}/stat\")).ok()?;\n    stat.rsplit_once(')')?.1.split_whitespace().nth(19)?.parse().ok()\n}\n"`
- `tests/simplification_audit.rs:7808-7808` `"fn ppid_of(pid: u32) -> Option<u32> {\n    let stat = std::fs::read_to_string(format!(\"/proc/{pid}/stat\")).ok()?;\n    stat.rsplit_once(')')?.1.split_whitespace().nth(1)?.parse().ok()\n}\n"`
- `tests/simplification_audit.rs:7899-7899` `"fn a() {\n    let _ = std::fs::read_to_string(\"/proc/1/stat\");\n    let _ = \"hello\";\n}\n"`
- `tests/simplification_audit.rs:7902-7902` `"/proc"`
- `tests/simplification_audit.rs:7904-7904` `"/proc"`
- `tests/simplification_audit.rs:7968-7968` `"fn state_of(pid: u32) -> Option<char> {\n    let s = std::fs::read_to_string(format!(\"/proc/{pid}/stat\")).ok()?;\n    s.chars().next()\n}\nfn ppid_of(pid: u32) -> Option<u32> {\n    let s = std::fs::read_to_string(format!(\"/proc/{pid}/status\")).ok()?;\n    s.parse().ok()\n}\nfn unrelated() -> u32 {\n    1\n}\n"`

#### `dup-0149` (semantic, 15 sites)

Proposed home: `src/reap.rs as the one /proc/<pid>/stat and /proc/<pid>/status parser, returning whichever field each caller needs, so dash.rs::process_state and reap.rs::pid_starttime/read_ppid stop each re-deriving the pid(comm)state... split`

mandatory sweep: /proc/<pid>/stat or /proc/<pid>/status field-extraction functions - 15 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/dash.rs:499-507` `process_state`
- `src/main.rs:24893-24906` `pgid_of`
- `src/reap.rs:214-221` `pid_starttime`
- `src/reap.rs:225-231` `read_ppid`
- `tests/cli.rs:24990-25003` `proc_pgid_of`
- `tests/cli.rs:28114-28214` `cmd_dash_gives_the_stopped_listener_diagnosis_naming_resume_or_kill`
- `tests/cli.rs:28293-28408` `step_names_the_stopped_holder_when_the_step_paths_own_auto_start_hits_the_predecessor_scenario`
- `tests/mutation_runner_pdeathsig_periphery.rs:74-85` `is_running`
- `tests/simplification_audit.rs:2849-2880` `build_sweep_clusters`
- `tests/simplification_audit.rs:3119-3140` `build_extra_semantic_clusters`
- `tests/simplification_audit.rs:3385-3490` `render_adversarial_sample`
- `tests/simplification_audit.rs:4447-4996` `render_section_6`
- `tests/simplification_audit.rs:7790-7816` `three_near_but_not_identical_functions_cluster_as_near_with_every_site_and_one_home`
- `tests/simplification_audit.rs:7894-7905` `proc_literal_sweep_finds_a_proc_path_string_and_ignores_an_unrelated_one`
- `tests/simplification_audit.rs:7962-7975` `proc_stat_or_status_reader_sweep_finds_a_stat_reader_and_a_status_reader_but_not_an_unrelated_fn`

#### `dup-0150` (exact, 3 sites)

Proposed home: `dash::buckets`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:1993-1999` `underived_message`
- `src/dash.rs:2010-2016` `no_membership_message`
- `src/dash.rs:2022-2028` `label_kind`

#### `dup-0151` (exact, 2 sites)

Proposed home: `dash::response`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:4371-4377` `html`
- `src/dash.rs:4378-4384` `json`

#### `dup-0152` (semantic, 3 sites)

Proposed home: `dash::response - one canonical constructor, the rest thin variants over it (or a builder), rather than each re-listing every field`

mandatory sweep: parallel constructor functions - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/dash.rs:4371-4377` `html`
- `src/dash.rs:4378-4384` `json`
- `src/dash.rs:4385-4391` `text`

#### `dup-0153` (near, 5 sites)

Proposed home: `dash::support (consolidate these 5 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:5468-5484` `dash_serving_on_is_false_for_a_non_dash_listener`
- `src/dash.rs:5625-5645` `dash_serving_pid_on_reports_the_pid_a_real_dash_response_names`
- `src/dash.rs:5667-5690` `dash_serving_pid_on_assembles_a_head_that_spans_many_read_calls`
- `src/dash.rs:5697-5712` `dash_serving_pid_on_is_none_for_a_non_dash_listener`
- `src/dash.rs:5740-5760` `dash_serving_pid_on_is_none_when_the_pid_header_value_is_not_a_number`

#### `dup-0154` (near, 2 sites)

Proposed home: `dash::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:5495-5540` `dash_serving_on_is_bounded_against_a_byte_dribbling_holder`
- `src/dash.rs:5563-5614` `dash_serving_on_recognizes_the_header_fast_even_if_the_holder_never_finishes_the_block`

#### `dup-0155` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/dash_decisions_progressive_disclosure.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:6024-6088` `the_decision_history_renders_each_decision_as_a_native_details_with_preview_and_full_body`
- `tests/dash_decisions_progressive_disclosure.rs:134-212` `the_served_root_page_ships_the_decisions_progressive_disclosure_region`

#### `dup-0156` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/dash_kg_graph_route.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:7598-7632` `tiered_chain_graph`
- `tests/dash_kg_graph_route.rs:39-69` `fixture_graph`

#### `dup-0157` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/dash_kg_graph_route.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:8088-8113` `star_graph`
- `tests/dash_kg_graph_route.rs:632-657` `star_graph`

#### `dup-0158` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/dash_kg_graph_route.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:8297-8317` `chain_graph_local`
- `tests/dash_kg_graph_route.rs:75-95` `chain_graph`

#### `dup-0159` (near, 2 sites)

Proposed home: `dash::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:9420-9438` `describe_held_port_names_this_process_when_it_holds_the_port_itself`
- `src/dash.rs:9453-9469` `describe_held_port_if_confirmed_names_the_holder_when_independently_confirmed`

#### `dup-0160` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/dash_calls_route_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:9862-9875` `cedge`
- `tests/dash_calls_route_periphery.rs:85-98` `calls_edge`

#### `dup-0161` (exact, 12 sites)

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

#### `dup-0162` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/calls_down_execution_path_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:9885-9887` `layer_of`
- `tests/calls_down_execution_path_periphery.rs:120-122` `layer_of`

#### `dup-0163` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/rationale_overlay_seam.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:10250-10260` `node`
- `tests/rationale_overlay_seam.rs:30-40` `node`

#### `dup-0164` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/dash_graph_exploration_overview.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:10277-10287` `edge`
- `tests/dash_graph_exploration_overview.rs:61-71` `edge`

#### `dup-0165` (exact, 2 sites)

Proposed home: `dash::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:10338-10340` `ids`
- `src/dash.rs:10341-10343` `kinds`

#### `dup-0166` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/metadata_card_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:10543-10567` `an_absent_explain_leaves_the_graph_route_unchanged`
- `src/dash.rs:10832-10852` `a_cluster_drill_carries_no_memory_field`
- `tests/metadata_card_periphery.rs:350-367` `the_served_route_degrades_gracefully_for_an_unknown_card_subject`

#### `dup-0167` (exact, 4 sites)

Proposed home: `a new shared module (sites span 3 files: src/dash.rs, tests/metadata_card_periphery.rs, tests/subject_view_memory_rail_contract.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:10581-10590` `node`
- `src/dash.rs:10868-10877` `node`
- `tests/metadata_card_periphery.rs:41-50` `node`
- `tests/subject_view_memory_rail_contract.rs:37-46` `node`

#### `dup-0168` (near, 4 sites)

Proposed home: `a new shared module (sites span 3 files: src/dash.rs, tests/metadata_card_periphery.rs, tests/subject_view_memory_rail_contract.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:10609-10634` `subject_graph`
- `src/dash.rs:10896-10931` `card_graph`
- `tests/metadata_card_periphery.rs:69-105` `fixture_graph`
- `tests/subject_view_memory_rail_contract.rs:64-89` `subject_graph`

#### `dup-0169` (near, 2 sites)

Proposed home: `dash::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:11045-11058` `card_of_a_file_reports_no_proof_of_its_own`
- `src/dash.rs:11065-11075` `card_tolerates_a_malformed_proof_evidence_attr`

#### `dup-0170` (near, 4 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/metadata_card_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:11197-11239` `the_card_route_serves_a_known_subjects_card_and_null_for_an_unknown_one`
- `tests/metadata_card_periphery.rs:124-171` `the_served_route_carries_a_code_subjects_card`
- `tests/metadata_card_periphery.rs:179-205` `the_served_route_omits_community_and_line_keys_for_a_membership_less_entity`
- `tests/metadata_card_periphery.rs:374-400` `the_card_param_takes_precedence_over_a_stray_seed_or_lens_param`

#### `dup-0171` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/distiller.rs, src/main.rs, src/playbooks.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/distiller.rs:44-53` `fnv1a_64`
- `src/main.rs:1008-1017` `fnv1a_64`
- `src/playbooks.rs:36-45` `fnv1a_64`

#### `dup-0172` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/distiller.rs, src/playbooks.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/distiller.rs:211-223` `render`
- `src/playbooks.rs:125-136` `render`

#### `dup-0173` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/distiller.rs, src/playbooks.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/distiller.rs:231-244` `rebuild`
- `src/playbooks.rs:143-156` `rebuild`

#### `dup-0174` (exact, 3 sites)

Proposed home: `a new shared module (sites span 2 files: src/distiller.rs, src/playbooks.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/distiller.rs:259-269` `decision`
- `src/distiller.rs:271-281` `finding`
- `src/playbooks.rs:163-173` `lesson`

#### `dup-0175` (near, 2 sites)

Proposed home: `docs::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/docs.rs:94-112` `render_using_rigger_skill`
- `src/docs.rs:116-127` `render_handbook_discipline`

#### `dup-0176` (near, 2 sites)

Proposed home: `docs::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/docs.rs:447-449` `render_planning_a_spec_skill`
- `src/docs.rs:618-620` `render_planning_field_guide`

#### `dup-0177` (near, 7 sites)

Proposed home: `docs::support (consolidate these 7 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/docs.rs:625-704` `render_reset_store_skill`
- `src/docs.rs:709-754` `render_build_graph_skill`
- `src/docs.rs:759-803` `render_reindex_skill`
- `src/docs.rs:808-862` `render_resume_a_run_skill`
- `src/docs.rs:869-919` `render_handle_an_escalation_skill`
- `src/docs.rs:1024-1097` `render_restore_the_dash_skill`
- `src/docs.rs:1106-1176` `render_diagnose_churn_skill`

#### `dup-0178` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/docs.rs, tests/cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/docs.rs:2060-2078` `restore_the_dash_carries_the_hung_holder_diagnosis`
- `tests/cli.rs:26324-26342` `rigger_workflow_yml_pins_the_checkin_stage_and_mutation_gate_definition_to_spec_91`

#### `dup-0179` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/driver/cli.rs:403-430` `persona_is_the_system_prompt_task_is_the_prompt`
- `src/driver/cli.rs:433-444` `recurse_false_drops_the_agent_tool_from_allowed_tools`

#### `dup-0180` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/driver/replay.rs, tests/spawn_target_dir_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/driver/replay.rs:376-378` `no_emit`
- `tests/spawn_target_dir_periphery.rs:86-88` `no_emit`

#### `dup-0181` (near, 2 sites)

Proposed home: `replay::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/driver/replay.rs:380-387` `worker`
- `src/driver/replay.rs:1596-1603` `reviewer`

#### `dup-0182` (near, 2 sites)

Proposed home: `replay::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/driver/replay.rs:605-618` `reclaim_unit_mutation_scratch_never_cross_matches_a_unit_id_that_is_a_string_prefix_of_another`
- `src/driver/replay.rs:624-635` `reclaim_unit_mutation_scratch_is_a_no_op_for_an_empty_unit_id`

#### `dup-0183` (near, 4 sites)

Proposed home: `replay::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/driver/replay.rs:1929-2027` `an_emit_only_approve_gating_persona_hard_errors_on_the_replay_driver`
- `src/driver/replay.rs:2030-2127` `a_concurrent_sibling_approve_does_not_hard_error_a_units_genuine_empty_verdict_reject`
- `src/driver/replay.rs:2130-2227` `a_parked_unanswered_sibling_does_not_suppress_this_units_own_approve_backstop`
- `src/driver/replay.rs:2230-2330` `a_closed_sibling_window_overlapping_this_units_own_approve_still_hard_errors`

#### `dup-0184` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/driver/workflow.rs, src/watch.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/driver/workflow.rs:83-85` `new`
- `src/watch.rs:577-579` `new`

#### `dup-0185` (near, 3 sites)

Proposed home: `contract::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/contract.rs:172-192` `append_assigns_revisions`
- `src/eventstore/contract.rs:296-341` `backward_stream_read_reverses_set`
- `src/eventstore/contract.rs:345-370` `forward_stream_read_honors_nonzero_from`

#### `dup-0186` (near, 2 sites)

Proposed home: `contract::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/contract.rs:248-269` `subscription_replays_then_goes_live`
- `src/eventstore/contract.rs:271-292` `stream_subscription_replays_then_goes_live`

#### `dup-0187` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/eventstore/contract.rs, src/spawn.rs, tests/adoption_keys_on_criterion_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/contract.rs:825-832` `read_stream`
- `src/spawn.rs:1510-1517` `read_stream`
- `tests/adoption_keys_on_criterion_periphery.rs:2636-2643` `read_stream`

#### `dup-0188` (exact, 5 sites)

Proposed home: `a new shared module (sites span 4 files: src/eventstore/contract.rs, src/spawn.rs, tests/adoption_keys_on_criterion_periphery.rs, tests/integrate_conflict_merge_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/contract.rs:844-846` `subscribe_stream`
- `src/spawn.rs:1532-1534` `subscribe_stream`
- `tests/adoption_keys_on_criterion_periphery.rs:2658-2660` `subscribe_stream`
- `tests/integrate_conflict_merge_periphery.rs:2125-2127` `subscribe_stream`
- `tests/integrate_conflict_merge_periphery.rs:3539-3541` `subscribe_stream`

#### `dup-0189` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/eventstore/kurrentdb.rs, src/eventstore/sqlite.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/kurrentdb.rs:121-123` `from_nanos`
- `src/eventstore/sqlite.rs:1474-1476` `from_nanos`

#### `dup-0190` (near, 2 sites)

Proposed home: `kurrentdb::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/kurrentdb.rs:612-617` `a_single_event_reports_the_position_the_server_issued`
- `src/eventstore/kurrentdb.rs:647-657` `a_batch_reports_the_revision_span_the_ack_names`

#### `dup-0191` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/eventstore/mod.rs, src/spawn.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/mod.rs:121-124` `with_valid_from`
- `src/spawn.rs:568-571` `with_meta`

#### `dup-0192` (semantic, 2 sites)

Proposed home: `mod::appended - one canonical constructor, the rest thin variants over it (or a builder), rather than each re-listing every field`

mandatory sweep: parallel constructor functions - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/eventstore/mod.rs:158-162` `all`
- `src/eventstore/mod.rs:166-168` `from_placements`

#### `dup-0193` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/eventstore/mod.rs, src/sidecar.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/mod.rs:536-541` `drop`
- `src/sidecar.rs:219-224` `drop`

#### `dup-0194` (exact, 3 sites)

Proposed home: `a new shared module (sites span 2 files: src/eventstore/mod.rs, src/spec.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/mod.rs:797-803` `strips_userinfo_and_query_keeps_scheme_host_port`
- `src/eventstore/mod.rs:822-828` `a_credential_smuggled_after_the_path_is_dropped_with_the_path`
- `src/spec.rs:1938-1945` `strip_inline_code_direct_exact_output_pins_a_zero_width_quote_pair`

#### `dup-0195` (exact, 11 sites)

Proposed home: `a new shared module (sites span 4 files: src/eventstore/mod.rs, src/spec.rs, tests/simplification_audit.rs, tests/store_secrets_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/mod.rs:806-811` `strips_a_bare_user_with_no_password`
- `src/eventstore/mod.rs:814-819` `an_already_credential_free_endpoint_is_unchanged`
- `src/eventstore/mod.rs:891-896` `strips_a_userinfo_with_no_password`
- `src/eventstore/mod.rs:930-935` `plain_text_with_no_url_is_untouched`
- `src/spec.rs:1567-1573` `ownership_check_recognizes_owner_inside_a_hyphenated_compound`
- `tests/simplification_audit.rs:6881-6883` `impl_self_type_still_handles_a_generic_self_type_with_a_where_clause`
- `tests/simplification_audit.rs:6909-6911` `impl_self_type_handles_a_bound_generic_self_type`
- `tests/simplification_audit.rs:6914-6916` `impl_self_type_handles_a_trait_impl_on_a_lifetime_generic_self_type`
- `tests/simplification_audit.rs:6919-6924` `impl_self_type_handles_a_generic_trait_impl_on_a_generic_self_type`
- `tests/simplification_audit.rs:6927-6932` `impl_self_type_handles_a_const_generic_self_type`
- `tests/store_secrets_periphery.rs:92-94` `redact_conn_on_the_empty_string_is_empty`

#### `dup-0196` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/eventstore/mod.rs, src/spec.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/mod.rs:836-853` `a_delimiter_inside_the_userinfo_never_leaks_the_credential_head`
- `src/spec.rs:1898-1919` `strip_inline_code_direct_exact_output_pins_the_one_span_per_kind_rule`

#### `dup-0197` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/eventstore/mod.rs, tests/store_secrets_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/mod.rs:878-888` `strips_user_and_password_but_keeps_scheme_host_and_query`
- `tests/store_secrets_periphery.rs:78-88` `redact_conn_scrubs_the_whole_userinfo_when_the_authority_has_several_at_signs`

#### `dup-0198` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/eventstore/mod.rs, tests/store_secrets_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/mod.rs:899-906` `leaves_a_conn_with_no_userinfo_unchanged`
- `tests/store_secrets_periphery.rs:65-72` `redact_conn_leaves_an_at_sign_in_the_path_alone`

#### `dup-0199` (semantic, 2 sites)

Proposed home: `one shared `content_key_index_name` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/eventstore/sqlite.rs:226-236` `content_key_index_name`
- `tests/store_content_identity_periphery.rs:1753-1774` `content_key_index_name`

#### `dup-0200` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/eventstore/sqlite.rs, src/metrics.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/sqlite.rs:1103-1109` `factor`
- `src/metrics.rs:362-368` `cost_per_upheld`

#### `dup-0201` (near, 4 sites)

Proposed home: `a new shared module (sites span 3 files: src/eventstore/sqlite.rs, src/grounder/symbols/events.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/sqlite.rs:1780-1785` `direction_sql`
- `src/grounder/symbols/events.rs:772-783` `kind_str`
- `src/grounder/symbols/events.rs:787-796` `lang_str`
- `tests/simplification_audit.rs:3776-3782` `disposition_label`

#### `dup-0202` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/eventstore/sqlite.rs, tests/reset_derived_compaction_periphery.rs, tests/store_content_identity_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/sqlite.rs:1812-1827` `subject_of`
- `tests/reset_derived_compaction_periphery.rs:2109-2124` `path_subject_of`
- `tests/store_content_identity_periphery.rs:233-249` `path_subject_of`

#### `dup-0203` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/eventstore/sqlite.rs, src/ingest.rs, tests/published_content_key_split_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/sqlite.rs:1830-1833` `split`
- `src/ingest.rs:496-499` `derived_key_parts`
- `tests/published_content_key_split_periphery.rs:79-82` `split`

#### `dup-0204` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: src/eventstore/sqlite.rs, tests/store_content_identity_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/sqlite.rs:1838-1840` `identity`
- `src/eventstore/sqlite.rs:2705-2707` `other_identity`
- `tests/store_content_identity_periphery.rs:264-266` `project_policy`

#### `dup-0205` (near, 4 sites)

Proposed home: `a new shared module (sites span 3 files: src/eventstore/sqlite.rs, tests/published_content_key_split_periphery.rs, tests/store_content_identity_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/sqlite.rs:1842-1844` `keyed`
- `src/eventstore/sqlite.rs:2711-2713` `other_keyed`
- `tests/published_content_key_split_periphery.rs:86-88` `keyed`
- `tests/store_content_identity_periphery.rs:274-276` `keyed`

#### `dup-0206` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/eventstore/sqlite.rs, tests/store_content_identity_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/sqlite.rs:1847-1852` `batch`
- `tests/store_content_identity_periphery.rs:279-284` `batch`

#### `dup-0207` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/sqlite.rs:2109-2137` `one_files_generations_never_leak_into_another_files_subject`
- `src/eventstore/sqlite.rs:2488-2523` `a_generation_that_is_a_string_prefix_of_a_later_one_is_still_found`

#### `dup-0208` (near, 3 sites)

Proposed home: `sqlite::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/sqlite.rs:3618-3655` `measure_derived_duplication_scopes_to_the_stream_prefix`
- `src/eventstore/sqlite.rs:3658-3686` `measure_derived_duplication_on_a_clean_log_reports_no_duplication`
- `src/eventstore/sqlite.rs:3689-3743` `measure_derived_duplication_treats_the_same_key_under_two_covered_types_as_two_distinct_subjects`

#### `dup-0209` (near, 4 sites)

Proposed home: `a new shared module (sites span 4 files: src/failure.rs, src/gate.rs, src/ledger.rs, src/watch.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/failure.rs:43-49` `as_str`
- `src/gate.rs:77-83` `as_str`
- `src/ledger.rs:47-59` `as_str`
- `src/watch.rs:192-201` `response`

#### `dup-0210` (exact, 3 sites)

Proposed home: `a new shared module (sites span 2 files: src/failure.rs, src/gate.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/failure.rs:65-67` `reruns`
- `src/failure.rs:74-76` `demotes_on_persistent_failure`
- `src/gate.rs:51-53` `runs_inline`

#### `dup-0211` (exact, 2 sites)

Proposed home: `gate::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/gate.rs:31-37` `parse`
- `src/gate.rs:69-75` `parse`

#### `dup-0212` (near, 3 sites)

Proposed home: `gate::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/gate.rs:1172-1208` `exec_runner_exports_cargo_target_dir_only_when_given`
- `src/gate.rs:1211-1239` `exec_runner_forces_cargo_target_dir_onto_build_cache_dir_when_target_dir_is_empty`
- `src/gate.rs:1272-1294` `exec_runner_target_dir_wins_over_build_cache_dir_when_both_are_given`

#### `dup-0213` (near, 2 sites)

Proposed home: `gate::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/gate.rs:1242-1269` `exec_runner_env_vars_reach_the_gate_command_through_the_flock_guard_wrapper`
- `src/gate.rs:1399-1427` `exec_runner_degrades_to_unguarded_when_the_guard_path_cannot_be_opened`

#### `dup-0214` (exact, 6 sites)

Proposed home: `a new shared module (sites span 6 files: src/gate.rs, src/reap.rs, tests/mutation_scratch_reap_base_guard_periphery.rs, tests/reap_before_removal_periphery.rs, tests/spawn_scratch_reap_authorized_root_periphery.rs, tests/worktree_remove_relocated_scratch_base_guard_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/gate.rs:1300-1308` `wait_until`
- `src/reap.rs:558-566` `wait_until`
- `tests/mutation_scratch_reap_base_guard_periphery.rs:65-73` `wait_until`
- `tests/reap_before_removal_periphery.rs:52-60` `wait_until`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:149-157` `wait_until`
- `tests/worktree_remove_relocated_scratch_base_guard_periphery.rs:66-74` `wait_until`

#### `dup-0215` (near, 4 sites)

Proposed home: `gate::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/gate.rs:1596-1615` `build_env_resolves_wrapper_cache_dir_and_incremental_off_when_configured`
- `src/gate.rs:1618-1625` `build_env_derives_the_wrapper_specific_cache_dir_var_name`
- `src/gate.rs:1701-1708` `build_env_jobs_cap_reaches_the_build_when_set`
- `src/gate.rs:1711-1723` `build_env_jobs_cap_is_independent_of_the_wrapper`

#### `dup-0216` (near, 2 sites)

Proposed home: `gate::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/gate.rs:1644-1685` `exec_runner_applies_the_build_env_it_is_given`
- `src/gate.rs:1742-1761` `exec_runner_applies_the_jobs_cap_it_is_given`

#### `dup-0217` (exact, 2 sites)

Proposed home: `gate::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/gate.rs:1799-1808` `resolve_wrapper_name_auto_probes_known_wrappers_and_finds_one_present`
- `src/gate.rs:1823-1833` `resolve_wrapper_name_named_wrapper_present_on_path_resolves_to_itself`

#### `dup-0218` (near, 2 sites)

Proposed home: `gate::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/gate.rs:1978-1994` `resolve_build_layer_named_wrapper_with_an_uncreatable_dir_errors_naming_dir_and_key`
- `src/gate.rs:2062-2081` `resolve_build_layer_named_wrapper_with_a_preexisting_unwritable_dir_errors_naming_dir_and_key`

#### `dup-0219` (exact, 2 sites)

Proposed home: `gate::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/gate.rs:1997-2011` `resolve_build_layer_auto_with_an_uncreatable_dir_skips_the_whole_layer`
- `src/gate.rs:2085-2100` `resolve_build_layer_auto_with_a_preexisting_unwritable_dir_skips_the_whole_layer`

#### `dup-0220` (near, 2 sites)

Proposed home: `events::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/design/events.rs:20-43` `concept_events`
- `src/grounder/design/events.rs:51-74` `link_events`

#### `dup-0221` (semantic, 3 sites)

Proposed home: `one shared `project_batches` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/grounder/design/events.rs:90-114` `project_batches`
- `src/grounder/symbols/events.rs:89-91` `project_batches`
- `src/grounder/workflowdef.rs:242-249` `project_batches`

#### `dup-0222` (near, 2 sites)

Proposed home: `events::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/design/events.rs:200-214` `the_emit_is_deterministic_and_sorts_by_kind_then_id`
- `src/grounder/design/events.rs:320-337` `the_link_emit_is_deterministic_and_sorts_by_rel_then_from_then_to`

#### `dup-0223` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/grounder/design/extract.rs, src/spec.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/design/extract.rs:433-436` `is_markdown`
- `src/spec.rs:541-548` `starts_new_element`
- `tests/simplification_audit.rs:2789-2796` `looks_error_shaping`

#### `dup-0224` (near, 2 sites)

Proposed home: `extract::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/design/extract.rs:532-537` `first_heading`
- `src/grounder/design/extract.rs:540-546` `section_headings`

#### `dup-0225` (near, 4 sites)

Proposed home: `extract::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/design/extract.rs:602-611` `a_load_bearing_decision_doc_becomes_a_single_arch_decision_node`
- `src/grounder/design/extract.rs:614-620` `a_spec_shape_or_loop_discipline_doc_becomes_a_handbook_rule_node`
- `src/grounder/design/extract.rs:623-637` `a_why_comment_in_a_source_file_becomes_a_rationale_node`
- `src/grounder/design/extract.rs:957-966` `a_source_file_is_never_a_usage_doc_and_its_rationale_stays_in_scope`

#### `dup-0226` (near, 2 sites)

Proposed home: `extract::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/design/extract.rs:748-761` `a_rationale_explains_the_file_it_annotates`
- `src/grounder/design/extract.rs:764-780` `a_fenced_code_example_path_is_not_mistaken_for_a_specifies_link`

#### `dup-0227` (near, 2 sites)

Proposed home: `model::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/design/model.rs:30-37` `node_kind`
- `src/grounder/design/model.rs:81-89` `rel`

#### `dup-0228` (semantic, 2 sites)

Proposed home: `one shared `extract_events` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/grounder/symbols/events.rs:204-300` `extract_events`
- `src/grounder/workflowdef.rs:191-221` `extract_events`

#### `dup-0229` (semantic, 3 sites)

Proposed home: `src/grounder/symbols/extract.rs::extract as the ONE function that touches source parsing (already its own module doc's claim, architecture 5.5.3) - this file's own scan_file/tokenize are ad hoc scanners for the identical job and should route through an injected-grammar extractor rather than re-deriving structure by hand`

mandatory sweep: bespoke source-text lexer/scanner functions duplicating the canonical tree-sitter extractor - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/grounder/symbols/extract.rs:34-191` `extract`
- `tests/simplification_audit.rs:207-209` `scan_file`
- `tests/simplification_audit.rs:2163-2269` `tokenize`

#### `dup-0230` (near, 7 sites)

Proposed home: `extract::support (consolidate these 7 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/symbols/extract.rs:909-977` `test_annotated_definitions_and_everything_nested_inside_them_are_marked_is_test`
- `src/grounder/symbols/extract.rs:980-1018` `cfg_predicates_naming_test_are_recognized_and_similarly_spelled_tokens_are_not`
- `src/grounder/symbols/extract.rs:1021-1056` `negated_and_cfg_attr_predicates_naming_test_do_not_mark_the_item_test`
- `src/grounder/symbols/extract.rs:1059-1094` `a_not_wrapping_a_non_test_atom_never_marks_the_item_test`
- `src/grounder/symbols/extract.rs:1097-1132` `compound_predicates_with_nested_negation_or_a_non_test_disjunct_do_not_mark_the_item_test`
- `src/grounder/symbols/extract.rs:1135-1175` `a_trailing_same_line_comment_on_a_cfg_test_attribute_does_not_sever_the_scan`
- `src/grounder/symbols/extract.rs:1178-1231` `an_inner_cfg_test_attribute_marks_its_enclosing_module_and_the_module_marks_its_children`

#### `dup-0231` (near, 2 sites)

Proposed home: `extract::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/symbols/extract.rs:1322-1368` `extent_spans_a_destructuring_or_default_brace_signature_to_the_full_body`
- `src/grounder/symbols/extract.rs:1371-1433` `extent_generalizes_across_grammars_python_nested_def_and_js_brace_string`

#### `dup-0232` (semantic, 2 sites)

Proposed home: `one shared `changed_files` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/grounder/symbols/grounder.rs:75-98` `changed_files`
- `src/worktree.rs:533-536` `changed_files`

#### `dup-0233` (near, 2 sites)

Proposed home: `grounder::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/symbols/grounder.rs:161-163` `commonness_map`
- `src/grounder/symbols/grounder.rs:178-180` `ambiguity_map`

#### `dup-0234` (near, 2 sites)

Proposed home: `grounder::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/symbols/grounder.rs:1062-1073` `a_reference_ranks_below_a_definition_of_the_same_name`
- `src/grounder/symbols/grounder.rs:1314-1328` `ground_ranks_an_exact_name_match_above_a_name_that_merely_contains_the_token`

#### `dup-0235` (near, 2 sites)

Proposed home: `grounder::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/symbols/grounder.rs:1179-1185` `empty_query_or_zero_k_grounds_nothing`
- `src/grounder/symbols/grounder.rs:1594-1604` `has_strong_match_is_true_for_a_contains_tier_match_of_an_unambiguous_entity`

#### `dup-0236` (near, 3 sites)

Proposed home: `grounder::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/symbols/grounder.rs:1231-1277` `a_concurrent_reindex_from_a_second_grounder_is_not_clobbered`
- `src/grounder/symbols/grounder.rs:1280-1306` `reindex_replaces_only_a_changed_files_symbols`
- `src/grounder/symbols/grounder.rs:1607-1634` `reindex_over_a_deleted_file_stops_grounding_it`

#### `dup-0237` (near, 2 sites)

Proposed home: `mod::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/symbols/mod.rs:320-330` `a_file_added_to_the_tree_since_the_index_was_built_is_flagged`
- `src/grounder/symbols/mod.rs:333-348` `a_file_removed_from_the_tree_since_the_index_was_built_is_flagged`

#### `dup-0238` (exact, 2 sites)

Proposed home: `model::symbol_index`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/symbols/model.rs:184-186` `insert_file`
- `src/grounder/symbols/model.rs:204-206` `set_hash`

#### `dup-0239` (near, 2 sites)

Proposed home: `registry::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/symbols/registry.rs:105-115` `javascript_tags_query`
- `src/grounder/symbols/registry.rs:126-137` `typescript_tags_query`

#### `dup-0240` (near, 2 sites)

Proposed home: `store::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/symbols/store.rs:23-28` `index_path`
- `src/grounder/symbols/store.rs:33-38` `lock_path`

#### `dup-0241` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/grounder/symbols/store.rs, src/grounder/workflowdef.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/symbols/store.rs:255-259` `load_is_none_on_a_cold_start`
- `src/grounder/workflowdef.rs:572-576` `project_events_on_a_missing_workflow_yields_nothing_never_a_crash`

#### `dup-0242` (near, 2 sites)

Proposed home: `hooks::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/hooks.rs:133-144` `installs_and_is_idempotent`
- `src/hooks.rs:158-170` `pretooluse_hook_installs_and_is_idempotent`

#### `dup-0243` (near, 2 sites)

Proposed home: `hooks::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/hooks.rs:147-155` `preserves_other_settings`
- `src/hooks.rs:203-218` `pretooluse_hook_composes_with_the_session_start_hook`

#### `dup-0244` (near, 2 sites)

Proposed home: `hooks::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/hooks.rs:235-247` `mcp_server_preserves_other_servers_and_other_top_level_keys`
- `src/hooks.rs:250-259` `mcp_server_self_heals_a_drifted_entry`

#### `dup-0245` (near, 2 sites)

Proposed home: `ingest::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/ingest.rs:320-322` `graph_index_lag`
- `src/ingest.rs:370-372` `graph_index_lag_sample`

#### `dup-0246` (semantic, 2 sites)

Proposed home: `ledger::attention_entry - one canonical constructor, the rest thin variants over it (or a builder), rather than each re-listing every field`

mandatory sweep: parallel constructor functions - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/ledger.rs:209-219` `unit_scoped`
- `src/ledger.rs:222-228` `run_scoped`

#### `dup-0247` (near, 2 sites)

Proposed home: `ledger::run_state`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/ledger.rs:643-648` `is_terminal`
- `src/ledger.rs:651-656` `is_integrated`

#### `dup-0248` (near, 7 sites)

Proposed home: `a new shared module (sites span 2 files: src/ledger.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/ledger.rs:944-951` `short_run_id_truncates_to_twelve_chars_and_passes_shorter_ids_through`
- `src/ledger.rs:954-969` `spec_stem_extracts_and_sanitizes_the_file_stem`
- `tests/simplification_audit.rs:6867-6878` `impl_self_type_strips_a_trailing_where_clause_on_a_non_generic_self_type`
- `tests/simplification_audit.rs:6886-6897` `impl_self_type_strips_a_leading_dyn_token_on_the_self_type`
- `tests/simplification_audit.rs:6935-6941` `strip_trailing_where_clause_is_a_word_boundary_match_not_a_substring_match`
- `tests/simplification_audit.rs:6983-6987` `pluralize_functions_uses_singular_only_at_exactly_one`
- `tests/simplification_audit.rs:7558-7568` `ident_kind_marker_classifies_by_casing`

#### `dup-0249` (near, 2 sites)

Proposed home: `liveness::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/liveness.rs:547-560` `marker_filename_hex_escapes_every_byte_outside_alphanumeric_and_hyphen`
- `src/liveness.rs:563-582` `marker_filename_hex_escapes_dots_so_no_encoded_result_can_ever_be_a_path_traversal_component`

#### `dup-0250` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/liveness.rs, src/spec.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/liveness.rs:585-600` `marker_filename_is_none_only_for_a_truly_empty_input_so_a_join_can_never_be_a_no_op`
- `src/spec.rs:2187-2189` `heading_level_rejects_more_than_six_hashes`

#### `dup-0251` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/liveness.rs, src/main.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/liveness.rs:628-642` `marker_filename_is_injective_so_two_ids_that_collided_under_a_prior_placeholder_scheme_no_longer_do`
- `src/main.rs:20336-20350` `normalize_origin_url_separates_distinct_repos_and_lowercases_only_the_host`

#### `dup-0252` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/liveness.rs, src/worktree.rs, tests/run_scoping_survives_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/liveness.rs:795-797` `read`
- `src/worktree.rs:4756-4758` `read_stream`
- `tests/run_scoping_survives_periphery.rs:133-135` `read`

#### `dup-0253` (near, 2 sites)

Proposed home: `liveness::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/liveness.rs:1043-1056` `any_marker_fresh_finds_a_fresh_marker_nested_under_a_run_id_directory`
- `src/liveness.rs:1059-1073` `any_marker_fresh_is_false_once_every_marker_is_older_than_max_age`

#### `dup-0254` (near, 3 sites)

Proposed home: `main::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:539-542` `config_rigger_dir`
- `src/main.rs:907-910` `project_identity`
- `src/main.rs:13362-13365` `git_repo`

#### `dup-0255` (semantic, 2 sites)

Proposed home: `one shared `project_identity` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/main.rs:907-910` `project_identity`
- `tests/reset_derived_compaction_periphery.rs:616-638` `project_identity`

#### `dup-0256` (exact, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:1657-1659` `usage`
- `src/main.rs:11807-11814` `print_scaffold_pointer`

#### `dup-0257` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/main.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:1770-1772` `find_store_dir_from`
- `tests/simplification_audit.rs:925-927` `scan_target_files`

#### `dup-0258` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:4721-4780` `cmd_graph_communities`
- `src/main.rs:4800-4859` `cmd_graph_concepts`

#### `dup-0259` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:5406-5419` `read_graph_index_lag`
- `src/main.rs:7277-7296` `dash_read_run`

#### `dup-0260` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/main.rs, tests/reset_build_cache_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:10973-10992` `dir_size_bytes`
- `tests/reset_build_cache_periphery.rs:93-110` `dir_bytes`

#### `dup-0261` (exact, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:12002-12004` `shim_dir`
- `src/main.rs:12102-12104` `docs_overlay_path`

#### `dup-0262` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:12823-12839` `install_lookup_hook`
- `src/main.rs:12848-12862` `install_operator_mcp`

#### `dup-0263` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/main.rs, tests/heartbeat_write_read_agree_periphery.rs, tests/reset_derived_live_writer_guard_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:13373-13383` `git_repo_at`
- `tests/heartbeat_write_read_agree_periphery.rs:68-78` `git_toplevel`
- `tests/reset_derived_live_writer_guard_periphery.rs:57-68` `git_toplevel`

#### `dup-0264` (exact, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:15391-15397` `run_started_at`
- `src/main.rs:15398-15404` `decision`

#### `dup-0265` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/main.rs, tests/graph_click_to_seed_repoint.rs, tests/graph_seeds_repoint_denoise.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:15461-15465` `ev`
- `tests/graph_click_to_seed_repoint.rs:26-30` `ev`
- `tests/graph_seeds_repoint_denoise.rs:23-27` `ev`

#### `dup-0266` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:15959-15998` `per_operation_skills_reference_only_real_subcommands`
- `src/main.rs:16008-16036` `watching_discipline_skills_reference_only_real_subcommands`

#### `dup-0267` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:16568-16612` `git_is_ancestor_decides_commit_order_in_a_real_repo`
- `src/main.rs:16615-16661` `git_commit_distance_counts_commits_ahead_in_a_real_repo`

#### `dup-0268` (near, 3 sites)

Proposed home: `main::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:16681-16686` `behind_the_tree_message_is_silent_when_versions_already_match`
- `src/main.rs:16689-16699` `behind_the_tree_message_is_silent_when_either_side_is_unversioned`
- `src/main.rs:16702-16711` `behind_the_tree_message_is_silent_on_an_undecidable_or_zero_distance`

#### `dup-0269` (exact, 25 sites)

Proposed home: `a new shared module (sites span 24 files: src/main.rs, tests/adaptive_labels_periphery.rs, tests/code_lens_overview_collapse_viz.rs, tests/concepts_lens_view_periphery.rs, tests/dash_calls_render_viz.rs, tests/dash_decisions_progressive_disclosure.rs, tests/dash_graph_exploration_viz.rs, tests/dash_kg_graph_route.rs, tests/dash_release_ready.rs, tests/files_lens_directory_hulls_viz.rs, tests/gitsemver_derivation.rs, tests/gitsemver_worktree_periphery.rs, tests/graph_collision_body_and_tiebreak.rs, tests/graph_density_spread_floor_and_centring.rs, tests/metadata_card_handoff_viz.rs, tests/native_driver_pipelining_behavior.rs, tests/proof_row_renders_on_the_card.rs, tests/readable_graph_adaptive_labels.rs, tests/readable_graph_density_scaled_spacing.rs, tests/readable_graph_layout_separation.rs, tests/subject_lens_overlay_client_arms.rs, tests/subject_lens_overlay_served_page.rs, tests/subject_view_memory_rail_client.rs, tests/validate_behind_the_tree_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:16728-16734` `gitsemver_available`
- `src/main.rs:23159-23165` `npm_available`
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

#### `dup-0270` (near, 5 sites)

Proposed home: `a new shared module (sites span 5 files: src/main.rs, tests/build_watch_paths.rs, tests/gitsemver_derivation.rs, tests/gitsemver_worktree_periphery.rs, tests/validate_behind_the_tree_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:16738-16753` `behind_the_tree_git`
- `tests/build_watch_paths.rs:42-53` `git`
- `tests/gitsemver_derivation.rs:43-54` `git`
- `tests/gitsemver_worktree_periphery.rs:56-67` `git`
- `tests/validate_behind_the_tree_periphery.rs:94-109` `git`

#### `dup-0271` (near, 4 sites)

Proposed home: `a new shared module (sites span 4 files: src/main.rs, tests/build_watch_paths.rs, tests/gitsemver_worktree_periphery.rs, tests/validate_behind_the_tree_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:16755-16767` `behind_the_tree_git_output`
- `tests/build_watch_paths.rs:59-74` `git_output`
- `tests/gitsemver_worktree_periphery.rs:75-90` `git_output`
- `tests/validate_behind_the_tree_periphery.rs:113-134` `git_output`

#### `dup-0272` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/main.rs, tests/reset_build_cache_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:16877-16880` `write_file`
- `tests/reset_build_cache_periphery.rs:71-74` `write_file`

#### `dup-0273` (near, 3 sites)

Proposed home: `main::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:16977-16994` `refuse_when_base_lacks_spec_paths_proceeds_when_the_base_contains_them`
- `src/main.rs:16997-17015` `refuse_when_base_lacks_spec_paths_partial_match_warns_and_proceeds`
- `src/main.rs:17018-17058` `refuse_when_base_lacks_spec_paths_skips_without_tokens_or_off_a_fresh_from_base_anchor`

#### `dup-0274` (exact, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:18178-18186` `footprint_advisories_is_silent_below_the_threshold`
- `src/main.rs:18222-18230` `footprint_advisories_is_silent_on_an_empty_category`

#### `dup-0275` (near, 3 sites)

Proposed home: `main::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:18233-18256` `owning_repo_root_prefers_the_stores_own_root_over_the_git_toplevel`
- `src/main.rs:18627-18639` `find_store_dir_from_walks_up_from_a_subdirectory`
- `src/main.rs:18871-18900` `find_store_dir_from_walks_past_a_storeless_rigger_to_the_real_store_above`

#### `dup-0276` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/main.rs, src/reap.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:18587-18594` `leaked_process_advisories_is_a_graceful_no_op_when_the_scratch_root_is_absent`
- `src/reap.rs:630-637` `processes_rooted_under_is_a_graceful_no_op_when_the_base_is_absent`

#### `dup-0277` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:18903-18956` `find_store_dir_from_resolves_the_owning_repo_even_when_the_worktree_lives_outside_it`
- `src/main.rs:18959-19013` `find_store_dir_from_never_climbs_a_relocated_worktrees_own_unrelated_ancestors_into_a_foreign_store`

#### `dup-0278` (exact, 3 sites)

Proposed home: `main::restore`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:19152-19154` `drop`
- `src/main.rs:24451-24453` `drop`
- `src/main.rs:24478-24480` `drop`

#### `dup-0279` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:19413-19451` `a_recorded_result_lets_the_replay_driver_advance_past_the_spawn`
- `src/main.rs:19454-19486` `a_recorded_error_result_replays_as_a_failure_not_a_fake_success`

#### `dup-0280` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:19784-19789` `parse_run_args_rejects_unknown_flags_and_values`
- `src/main.rs:25622-25626` `parse_watch_args_rejects_a_non_integer_interval_a_missing_value_and_an_unknown_flag`

#### `dup-0281` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:20469-20534` `migrate_project_identity_rekeys_graph_rows_so_pre_mint_history_is_not_orphaned`
- `src/main.rs:20620-20715` `migrate_project_identity_recovers_from_a_crash_between_the_rekey_and_the_rename`

#### `dup-0282` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:21461-21540` `init_project_gitignores_the_dash_runtime_breadcrumbs_idempotently`
- `src/main.rs:21549-21589` `init_project_gitignores_the_store_conn_secret_file_idempotently`

#### `dup-0283` (near, 4 sites)

Proposed home: `main::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:21994-22007` `import_agents_validates_and_rejects_a_malformed_agent`
- `src/main.rs:22016-22038` `import_agents_rejects_an_id_colliding_with_an_existing_agent`
- `src/main.rs:22044-22064` `import_agents_rejects_a_duplicate_id_within_one_import`
- `src/main.rs:22070-22091` `import_agents_rejects_an_agent_with_a_blank_id`

#### `dup-0284` (exact, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:22429-22440` `the_step_schema_admits_the_attention_array`
- `src/main.rs:24526-24529` `no_runs_message_points_at_rigger_run`

#### `dup-0285` (exact, 6 sites)

Proposed home: `a new shared module (sites span 6 files: src/main.rs, tests/meta_phases_declaration_periphery.rs, tests/phase_of_role_mapping_periphery.rs, tests/review_tier_roster_periphery.rs, tests/step_attention_periphery.rs, tests/worker_persona_label_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:22446-22468` `js_function_body`
- `tests/meta_phases_declaration_periphery.rs:65-87` `js_declaration`
- `tests/phase_of_role_mapping_periphery.rs:50-72` `js_declaration`
- `tests/review_tier_roster_periphery.rs:18-40` `js_declaration`
- `tests/step_attention_periphery.rs:655-677` `js_declaration`
- `tests/worker_persona_label_periphery.rs:21-43` `js_declaration`

#### `dup-0286` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:23247-23264` `format_canary_stats_reports_findings_raised_by_tier`
- `src/main.rs:23269-23279` `format_canary_stats_reports_a_zero_findings_count_honestly`

#### `dup-0287` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:23285-23291` `format_canary_stats_omits_the_findings_volume_section_when_empty`
- `src/main.rs:23513-23519` `format_canary_stats_omits_the_model_pinning_header_when_the_run_never_recorded_one`

#### `dup-0288` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:23429-23448` `format_canary_stats_reports_control_items_and_false_positives`
- `src/main.rs:23456-23471` `format_canary_stats_reports_zero_false_positives_honestly`

#### `dup-0289` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:23728-23773` `stats_discloses_when_no_verdict_was_recorded_on_this_driver`
- `src/main.rs:23784-23857` `stats_discloses_unfed_numerator_when_verdict_recorded_but_findings_unattributed`

#### `dup-0290` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:24548-24560` `stats_lines_absent_db_returns_none_and_creates_no_file`
- `src/main.rs:24758-24770` `result_of_at_absent_db_reads_as_unreported_and_creates_no_file`

#### `dup-0291` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:24776-24796` `result_of_at_unrecorded_spawn_reads_as_unreported`
- `src/main.rs:24834-24863` `result_of_at_is_namespace_scoped`

#### `dup-0292` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/main.rs, tests/cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:24893-24906` `pgid_of`
- `tests/cli.rs:24990-25003` `proc_pgid_of`

#### `dup-0293` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:25367-25388` `reset_modes_parses_force_live_alongside_derived_rejects_duplicates_and_never_implies_a_mode`
- `src/main.rs:25420-25437` `reset_modes_parses_build_cache_alone_and_composed_and_rejects_duplicates`

#### `dup-0294` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:25954-26047` `implementer_persona_pins_the_checkin_stage_kill_or_justify_contract`
- `src/main.rs:26055-26097` `implementer_persona_pins_the_checkpoint_before_long_work_contract`

#### `dup-0295` (near, 5 sites)

Proposed home: `main::support (consolidate these 5 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:26331-26339` `grep_guard_decision_denies_every_grep_tool_path`
- `src/main.rs:26359-26375` `grep_guard_decision_denies_a_shell_metacharacter_fused_grep`
- `src/main.rs:26398-26411` `grep_guard_decision_denies_a_redirect_metacharacter_fused_grep`
- `src/main.rs:26434-26447` `grep_guard_decision_denies_a_quoted_or_escaped_grep`
- `src/main.rs:26499-26511` `grep_guard_decision_denies_a_path_qualified_grep`

#### `dup-0296` (exact, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:26381-26387` `grep_guard_decision_literal_survives_a_shell_metacharacter_fused_grep`
- `src/main.rs:26486-26492` `grep_guard_decision_literal_survives_a_line_continuation_split_grep`

#### `dup-0297` (exact, 2 sites)

Proposed home: `mcpserver::tool_error`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/mcpserver.rs:30-35` `internal`
- `src/mcpserver.rs:39-44` `invalid_params`

#### `dup-0298` (semantic, 4 sites)

Proposed home: `mcpserver::tool_error - one canonical constructor, the rest thin variants over it (or a builder), rather than each re-listing every field`

mandatory sweep: parallel constructor functions - 4 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/mcpserver.rs:30-35` `internal`
- `src/mcpserver.rs:39-44` `invalid_params`
- `src/mcpserver.rs:48-50` `from`
- `src/mcpserver.rs:54-56` `from`

#### `dup-0299` (exact, 2 sites)

Proposed home: `mcpserver::server`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/mcpserver.rs:149-152` `with_graph`
- `src/mcpserver.rs:161-164` `with_grounder`

#### `dup-0300` (near, 2 sites)

Proposed home: `mcpserver::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/mcpserver.rs:1288-1306` `emit_tool_carries_meta_actor`
- `src/mcpserver.rs:1309-1331` `emit_tool_sets_valid_from_from_nanos`

#### `dup-0301` (near, 2 sites)

Proposed home: `mcpserver::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/mcpserver.rs:1345-1383` `peers_tool_scopes_to_the_files_arg`
- `src/mcpserver.rs:1386-1431` `peers_tool_surfaces_findings_scoped_to_the_files_arg`

#### `dup-0302` (near, 5 sites)

Proposed home: `mcpserver::support (consolidate these 5 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/mcpserver.rs:1483-1502` `rigger_result_for_an_unknown_id_is_an_error`
- `src/mcpserver.rs:1505-1527` `malformed_json_gets_a_parse_error`
- `src/mcpserver.rs:1530-1548` `request_missing_method_gets_an_invalid_request_error`
- `src/mcpserver.rs:1551-1569` `tools_call_missing_name_gets_an_invalid_params_error`
- `src/mcpserver.rs:1701-1720` `workflow_surface_rejects_ground_and_graph_as_unknown_tools`

#### `dup-0303` (exact, 7 sites)

Proposed home: `metrics::support (consolidate these 7 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/metrics.rs:341-343` `survival`
- `src/metrics.rs:435-437` `lens_overlap_rate`
- `src/metrics.rs:449-451` `first_pass_yield`
- `src/metrics.rs:455-457` `escalation_rate`
- `src/metrics.rs:1087-1089` `rate`
- `src/metrics.rs:1152-1154` `adjudicator_accuracy`
- `src/metrics.rs:1158-1160` `stability_rate`

#### `dup-0304` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/metrics.rs, src/run.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/metrics.rs:1009-1011` `gate_verdict`
- `src/run.rs:117-119` `from_event`

#### `dup-0305` (semantic, 2 sites)

Proposed home: `one shared `gate_verdict` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/metrics.rs:1009-1011` `gate_verdict`
- `tests/dash_run_tree_spine.rs:80-86` `gate_verdict`

#### `dup-0306` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/metrics.rs, src/spawn.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/metrics.rs:1323-1325` `changed`
- `src/spawn.rs:574-576` `is_error`

#### `dup-0307` (exact, 3 sites)

Proposed home: `metrics::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/metrics.rs:1404-1409` `started`
- `src/metrics.rs:1411-1416` `status`
- `src/metrics.rs:1443-1448` `artifact_verdict`

#### `dup-0308` (near, 6 sites)

Proposed home: `a new shared module (sites span 2 files: src/metrics.rs, src/run.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/metrics.rs:1418-1423` `failed`
- `src/metrics.rs:1425-1430` `integrated`
- `src/metrics.rs:1432-1434` `escalated`
- `src/run.rs:563-565` `decision`
- `src/run.rs:566-568` `finding`
- `src/run.rs:569-571` `lesson`

#### `dup-0309` (near, 5 sites)

Proposed home: `metrics::support (consolidate these 5 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/metrics.rs:1738-1751` `counts_review_rejects_on_both_per_unit_and_fan_out_paths`
- `src/metrics.rs:1806-1818` `fan_out_reject_then_approve_counts_one_each`
- `src/metrics.rs:1866-1879` `duplicate_unit_started_counts_the_unit_once`
- `src/metrics.rs:1882-1899` `interleaved_units_keep_per_id_review_state`
- `src/metrics.rs:1902-1913` `escalation_is_counted_once_per_unit`

#### `dup-0310` (near, 2 sites)

Proposed home: `metrics::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/metrics.rs:1970-1981` `finding`
- `src/metrics.rs:1986-1996` `courier_finding`

#### `dup-0311` (near, 2 sites)

Proposed home: `metrics::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/metrics.rs:2134-2154` `spawn_timing_never_pairs_a_cross_run_id_collision`
- `src/metrics.rs:2251-2266` `spawn_timing_excludes_a_same_batch_zero_duration_pair_as_suspect`

#### `dup-0312` (near, 2 sites)

Proposed home: `metrics::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/metrics.rs:2309-2332` `finding_survival_is_upheld_over_raised_per_actor`
- `src/metrics.rs:2404-2432` `cost_per_upheld_finding_is_spawns_over_upheld_per_tier`

#### `dup-0313` (near, 2 sites)

Proposed home: `metrics::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/metrics.rs:2853-2872` `project_canary_counts_correctly_rejected_items_with_empty_attribution`
- `src/metrics.rs:2881-2899` `project_canary_counts_controls_and_false_positives`

#### `dup-0314` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/progress.rs, tests/reset_derived_compaction.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/progress.rs:197-200` `event_unit_id`
- `tests/reset_derived_compaction.rs:161-164` `replay_key`

#### `dup-0315` (near, 6 sites)

Proposed home: `a new shared module (sites span 5 files: src/reap.rs, tests/mutation_scratch_reap_base_guard_periphery.rs, tests/reap_before_removal_periphery.rs, tests/spawn_scratch_reap_authorized_root_periphery.rs, tests/worktree_remove_relocated_scratch_base_guard_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/reap.rs:532-538` `sleeper_in`
- `src/reap.rs:542-549` `sigterm_ignorer_in`
- `tests/mutation_scratch_reap_base_guard_periphery.rs:54-61` `sigterm_ignorer_in`
- `tests/reap_before_removal_periphery.rs:41-48` `sigterm_ignorer_in`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:138-145` `sigterm_ignorer_in`
- `tests/worktree_remove_relocated_scratch_base_guard_periphery.rs:55-62` `sigterm_ignorer_in`

#### `dup-0316` (exact, 4 sites)

Proposed home: `a new shared module (sites span 4 files: src/reap.rs, tests/mutation_scratch_reap_base_guard_periphery.rs, tests/no_os_kill_test_helper_periphery.rs, tests/spawn_scratch_reap_authorized_root_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/reap.rs:570-573` `cleanup`
- `tests/mutation_scratch_reap_base_guard_periphery.rs:77-80` `cleanup`
- `tests/no_os_kill_test_helper_periphery.rs:84-87` `cleanup`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:161-164` `cleanup`

#### `dup-0317` (exact, 2 sites)

Proposed home: `reap::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/reap.rs:715-720` `is_reapable_base_refuses_a_dir_that_is_not_under_the_given_authorized_root`
- `src/reap.rs:723-726` `is_reapable_base_refuses_the_authorized_root_itself`

#### `dup-0318` (near, 2 sites)

Proposed home: `reap::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/reap.rs:1071-1097` `signal_if_unchanged_skips_a_starttime_mismatch`
- `src/reap.rs:1100-1127` `signal_if_unchanged_skips_when_cwd_is_outside_the_given_base`

#### `dup-0319` (near, 13 sites)

Proposed home: `a new shared module (sites span 11 files: src/registry.rs, tests/reset_build_cache_periphery.rs, tests/reset_derived_compaction.rs, tests/reset_derived_compaction_periphery.rs, tests/reset_menu.rs, tests/reset_menu_identity_migration_periphery.rs, tests/store_flag_precedence.rs, tests/store_precedence.rs, tests/store_resolution_cli.rs, tests/store_secrets.rs, tests/validate_advisories.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/registry.rs:133-135` `instances_dir`
- `tests/reset_build_cache_periphery.rs:45-47` `event_log`
- `tests/reset_derived_compaction.rs:75-77` `event_log`
- `tests/reset_derived_compaction_periphery.rs:610-612` `event_log`
- `tests/reset_derived_compaction_periphery.rs:1365-1367` `graph_db`
- `tests/reset_menu.rs:70-72` `event_log`
- `tests/reset_menu.rs:74-76` `graph_db`
- `tests/reset_menu_identity_migration_periphery.rs:65-67` `event_log`
- `tests/store_flag_precedence.rs:94-96` `local_event_log`
- `tests/store_precedence.rs:59-61` `local_event_log`
- `tests/store_resolution_cli.rs:66-68` `local_event_log`
- `tests/store_secrets.rs:62-64` `local_event_log`
- `tests/validate_advisories.rs:81-83` `event_log`

#### `dup-0320` (near, 3 sites)

Proposed home: `registry::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/registry.rs:299-313` `write_then_read_round_trips_a_live_entry`
- `src/registry.rs:411-423` `read_live_no_prune_still_returns_a_fresh_entry`
- `src/registry.rs:426-447` `read_all_returns_a_stale_entry_read_live_would_have_pruned_and_never_deletes_it`

#### `dup-0321` (near, 2 sites)

Proposed home: `registry::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/registry.rs:335-346` `two_projects_get_distinct_entries`
- `src/registry.rs:450-462` `read_all_returns_every_registered_root_regardless_of_freshness`

#### `dup-0322` (near, 2 sites)

Proposed home: `registry::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/registry.rs:349-362` `a_reader_prunes_a_stale_heartbeat`
- `src/registry.rs:383-408` `read_live_no_prune_filters_a_stale_heartbeat_but_never_deletes_its_file`

#### `dup-0323` (near, 3 sites)

Proposed home: `run::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/run.rs:701-767` `the_resolved_base_is_persisted_on_the_run_start_and_survives_adopt`
- `src/run.rs:770-841` `the_base_tip_is_persisted_on_the_run_start_and_survives_adopt`
- `src/run.rs:844-909` `the_spec_path_is_persisted_on_the_run_start_and_survives_adopt`

#### `dup-0324` (near, 2 sites)

Proposed home: `run::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/run.rs:935-952` `ensure_started_adopts_the_same_criteria_run_without_re_minting`
- `src/run.rs:1010-1028` `ensure_started_mints_a_new_run_when_the_criteria_change`

#### `dup-0325` (exact, 3 sites)

Proposed home: `sidecar::sidecar`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/sidecar.rs:137-147` `decisions_for`
- `src/sidecar.rs:168-178` `findings_for`
- `src/sidecar.rs:196-206` `lessons_for`

#### `dup-0326` (exact, 2 sites)

Proposed home: `sidecar::sidecar`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/sidecar.rs:155-161` `findings`
- `src/sidecar.rs:184-190` `lessons`

#### `dup-0327` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/sidecar.rs, tests/store_content_identity_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/sidecar.rs:209-211` `len`
- `tests/store_content_identity_periphery.rs:87-89` `batch_calls`

#### `dup-0328` (near, 3 sites)

Proposed home: `sidecar::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/sidecar.rs:268-308` `decisions_for_scopes_to_the_blast_radius`
- `src/sidecar.rs:311-359` `findings_for_scopes_to_the_blast_radius`
- `src/sidecar.rs:362-413` `lessons_for_scopes_to_the_blast_radius`

#### `dup-0329` (exact, 4 sites)

Proposed home: `spawn::spawn_request`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spawn.rs:338-341` `with_system_prompt`
- `src/spawn.rs:344-347` `with_model`
- `src/spawn.rs:356-359` `with_dir`
- `src/spawn.rs:368-371` `with_title`

#### `dup-0330` (exact, 3 sites)

Proposed home: `spawn::spawn_request`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spawn.rs:350-353` `with_tools`
- `src/spawn.rs:362-365` `with_blast_radius`
- `src/spawn.rs:374-377` `with_reviews`

#### `dup-0331` (exact, 2 sites)

Proposed home: `spawn::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spawn.rs:381-383` `to_event`
- `src/spawn.rs:663-665` `to_event`

#### `dup-0332` (exact, 2 sites)

Proposed home: `spawn::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spawn.rs:386-388` `from_event`
- `src/spawn.rs:668-670` `from_event`

#### `dup-0333` (near, 2 sites)

Proposed home: `spawn::spawn_result`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spawn.rs:527-534` `ok`
- `src/spawn.rs:539-546` `failed`

#### `dup-0334` (semantic, 3 sites)

Proposed home: `spawn::spawn_result - one canonical constructor, the rest thin variants over it (or a builder), rather than each re-listing every field`

mandatory sweep: parallel constructor functions - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/spawn.rs:527-534` `ok`
- `src/spawn.rs:539-546` `failed`
- `src/spawn.rs:554-565` `liveness_fault`

#### `dup-0335` (semantic, 2 sites)

Proposed home: `one shared `liveness_fault` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/spawn.rs:554-565` `liveness_fault`
- `tests/dash_run_tree_spine.rs:90-94` `liveness_fault`

#### `dup-0336` (exact, 2 sites)

Proposed home: `spawn::spawn_result`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spawn.rs:587-593` `liveness_class`
- `src/spawn.rs:600-606` `resolved_model`

#### `dup-0337` (near, 2 sites)

Proposed home: `spawn::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spawn.rs:1435-1469` `record_result_if_absent_is_a_noop_that_never_clobbers_an_existing_result`
- `src/spawn.rs:1584-1621` `record_result_if_absent_honors_a_self_report_that_won_the_race`

#### `dup-0338` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/spawn.rs, tests/adoption_keys_on_criterion_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spawn.rs:1519-1526` `read_all`
- `tests/adoption_keys_on_criterion_periphery.rs:2645-2652` `read_all`

#### `dup-0339` (exact, 4 sites)

Proposed home: `a new shared module (sites span 3 files: src/spawn.rs, tests/adoption_keys_on_criterion_periphery.rs, tests/integrate_conflict_merge_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spawn.rs:1528-1530` `subscribe_all`
- `tests/adoption_keys_on_criterion_periphery.rs:2654-2656` `subscribe_all`
- `tests/integrate_conflict_merge_periphery.rs:2122-2124` `subscribe_all`
- `tests/integrate_conflict_merge_periphery.rs:3536-3538` `subscribe_all`

#### `dup-0340` (near, 2 sites)

Proposed home: `spawn::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spawn.rs:1753-1786` `step_wave_is_the_full_pending_frontier_never_answered_spawns`
- `src/spawn.rs:2078-2110` `step_rerun_reprints_unanswered_spawns_so_a_killed_step_orphans_nothing`

#### `dup-0341` (near, 2 sites)

Proposed home: `spawn::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spawn.rs:1853-1880` `spawn_request_carries_a_title_via_builder_and_omits_it_when_empty`
- `src/spawn.rs:1883-1913` `wave_item_copies_the_request_title_so_the_thin_driver_renders_the_work`

#### `dup-0342` (near, 2 sites)

Proposed home: `spawn::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spawn.rs:1977-2017` `spawn_request_carries_a_reviews_roster_via_builder_and_omits_it_when_empty`
- `src/spawn.rs:2020-2055` `wave_item_copies_the_request_reviews_roster`

#### `dup-0343` (near, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:652-654` `find_word`
- `src/spec.rs:662-664` `find_word_across_hyphen`

#### `dup-0344` (exact, 3 sites)

Proposed home: `spec::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:911-925` `extract_criteria_joins_a_three_line_wrap_including_the_owns_sentence_on_line_three`
- `src/spec.rs:931-943` `extract_criteria_stops_a_wrap_at_the_next_checkbox_item_with_no_blank_line_between`
- `src/spec.rs:981-994` `extract_criteria_includes_a_nested_sub_bullet_as_part_of_the_criterion_text`

#### `dup-0345` (exact, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:949-959` `extract_criteria_stops_a_wrap_at_a_blank_line_and_excludes_the_prose_after_it`
- `src/spec.rs:964-974` `extract_criteria_stops_a_wrap_at_a_following_heading`

#### `dup-0346` (exact, 17 sites)

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

#### `dup-0347` (near, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:1041-1050` `a_single_coordinator_does_not_flag_multi_behavior`
- `src/spec.rs:1181-1194` `spec_shape_advisories_ignores_coordinators_added_by_continuation_lines`

#### `dup-0348` (near, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:1197-1206` `path_tokens_extracts_relative_file_paths_and_trims_markdown`
- `src/spec.rs:1222-1228` `path_tokens_dedupes_and_preserves_first_seen_order`

#### `dup-0349` (exact, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:1209-1219` `path_tokens_ignores_prose_flags_versions_types_and_urls`
- `src/spec.rs:1237-1248` `path_tokens_requires_an_alphabetic_extension_and_a_separator`

#### `dup-0350` (near, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:1296-1307` `ownership_check_accepts_the_word_owner`
- `src/spec.rs:1316-1332` `ownership_check_finds_an_owns_sentence_on_a_wrapped_continuation_line`

#### `dup-0351` (near, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:1351-1375` `ownership_check_finds_an_owns_sentence_on_an_unindented_continuation_line`
- `src/spec.rs:1384-1406` `ownership_check_does_not_reattach_prose_after_a_blank_line_to_the_prior_criterion`

#### `dup-0352` (exact, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:1414-1430` `ownership_check_does_not_let_a_dropped_word_boundary_fake_an_owns_sentence`
- `src/spec.rs:1442-1458` `ownership_check_does_not_let_a_dropped_word_boundary_weld_own_and_er_into_owner`

#### `dup-0353` (near, 4 sites)

Proposed home: `spec::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:1484-1497` `ownership_check_flags_ownerless_and_not_owned_as_denials`
- `src/spec.rs:1507-1526` `ownership_check_does_not_match_owns_or_owner_inside_an_unrelated_word`
- `src/spec.rs:1537-1559` `ownership_check_lets_an_affirmative_owns_win_over_an_unrelated_denial_elsewhere`
- `src/spec.rs:2117-2127` `starts_new_element_recognizes_every_prefix_kind_independently`

#### `dup-0354` (exact, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:1733-1742` `disposition_check_does_not_exempt_unsatisfied_either_or`
- `src/spec.rs:1781-1790` `disposition_check_still_flags_a_bare_either_or_with_no_satisfied_word`

#### `dup-0355` (exact, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:1767-1776` `disposition_check_finds_a_genuine_hedge_after_an_earlier_non_disjunctive_either`
- `src/spec.rs:1853-1864` `disposition_check_still_fires_outside_a_balanced_quote_pair`

#### `dup-0356` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/spec.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:1835-1847` `disposition_check_fails_closed_after_a_stray_unmatched_quote_earlier_in_the_paragraph`
- `tests/simplification_audit.rs:6534-6538` `a_trait_method_signature_without_a_body_is_not_recorded`

#### `dup-0357` (exact, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:2092-2102` `disposition_check_attributes_a_hit_inside_a_criterion`
- `src/spec.rs:2145-2155` `hygiene_check_attributes_a_hit_inside_a_criterion`

#### `dup-0358` (near, 4 sites)

Proposed home: `watch::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/watch.rs:673-679` `a_clean_store_detects_no_anomalies`
- `src/watch.rs:737-751` `two_failures_below_threshold_is_not_reported`
- `src/watch.rs:754-775` `a_cause_change_resets_the_streak_so_three_failures_split_across_two_causes_do_not_alert`
- `src/watch.rs:825-831` `a_spawn_answered_twice_is_below_the_frontier_stall_threshold`

#### `dup-0359` (near, 2 sites)

Proposed home: `watch::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/watch.rs:708-734` `a_unit_at_reject_recurrence_three_same_cause_is_reported`
- `src/watch.rs:797-822` `a_spawn_answered_three_times_is_reported_as_a_frontier_stall`

#### `dup-0360` (near, 3 sites)

Proposed home: `watch::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/watch.rs:881-904` `a_fresh_heartbeat_suppresses_the_dead_driver_alert_even_with_a_quiet_store`
- `src/watch.rs:907-935` `a_heartbeat_ten_minutes_stale_does_not_cross_the_thirty_minute_bound`
- `src/watch.rs:938-969` `a_heartbeat_exactly_thirty_minutes_stale_does_not_yet_cross_the_bound`

#### `dup-0361` (near, 3 sites)

Proposed home: `watch::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/watch.rs:1028-1051` `a_dash_marker_naming_a_dead_pid_is_reported`
- `src/watch.rs:1061-1089` `a_dead_dash_url_with_no_marker_is_reported_without_inventing_a_pid`
- `src/watch.rs:1173-1201` `dash_attempted_this_run_overrides_a_breadcrumb_that_looks_like_it_predates_the_run`

#### `dup-0362` (exact, 2 sites)

Proposed home: `watch::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/watch.rs:1092-1106` `no_dash_ever_recorded_is_not_an_anomaly`
- `src/watch.rs:1109-1123` `a_serving_dash_is_not_an_anomaly`

#### `dup-0363` (near, 2 sites)

Proposed home: `watch::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/watch.rs:1452-1464` `dedup_suppresses_a_persisting_anomaly_at_the_same_magnitude`
- `src/watch.rs:1483-1496` `dedup_re_alerts_a_cleared_and_later_recurring_anomaly`

#### `dup-0364` (near, 2 sites)

Proposed home: `worktree::worktree`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:557-559` `commit`
- `src/worktree.rs:575-577` `commit_checkpoint`

#### `dup-0365` (exact, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:1367-1378` `branch_exists`
- `src/worktree.rs:1401-1412` `ref_resolves`

#### `dup-0366` (semantic, 2 sites)

Proposed home: `one shared `branch_exists` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/worktree.rs:1367-1378` `branch_exists`
- `tests/step_root_resolution_periphery.rs:234-241` `branch_exists`

#### `dup-0367` (semantic, 2 sites)

Proposed home: `one shared `current_branch` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/worktree.rs:1430-1435` `current_branch`
- `tests/step_root_resolution_periphery.rs:221-232` `current_branch`

#### `dup-0368` (exact, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:1444-1452` `scratch_root`
- `src/worktree.rs:1543-1551` `scratch_root_path`

#### `dup-0369` (exact, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:1613-1616` `scratch_root_from_env`
- `src/worktree.rs:1620-1623` `scratch_root_path_from_env`

#### `dup-0370` (exact, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:1710-1718` `unit_cache_sibling`
- `src/worktree.rs:1738-1746` `unit_mutants_sibling`

#### `dup-0371` (exact, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:2341-2348` `head_sha_of`
- `src/worktree.rs:2363-2370` `tree_sha_of`

#### `dup-0372` (near, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:2504-2524` `integrate_lands_work_in_the_repo`
- `src/worktree.rs:4323-4356` `commit_cleans_the_tree_so_a_gate_sees_the_committed_artifact`

#### `dup-0373` (near, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:3458-3558` `cherry_pick_onto_run_branch_self_heals_a_leftover_marker_from_a_crash_mid_skip_loop`
- `src/worktree.rs:3561-3670` `cherry_pick_onto_run_branch_self_heals_a_leftover_marker_ahead_of_two_chained_empty_commits`

#### `dup-0374` (near, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:4427-4454` `changed_files_reports_only_the_rename_destination`
- `src/worktree.rs:6423-6435` `changed_files_unquotes_paths_with_spaces`

#### `dup-0375` (near, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:4662-4700` `sweep_terminal_removes_merged_worktrees_and_keeps_inflight_ones`
- `src/worktree.rs:5270-5318` `sweep_terminal_reclaims_a_crash_left_terminal_units_per_unit_build_cache`

#### `dup-0376` (near, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:4974-4978` `requested_and_answered`
- `src/worktree.rs:4982-4986` `requested_and_hung`

#### `dup-0377` (near, 4 sites)

Proposed home: `worktree::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:4989-5022` `sweep_terminal_spares_a_merged_branch_whose_units_latest_spawn_is_still_in_flight`
- `src/worktree.rs:5025-5051` `sweep_terminal_reclaims_a_merged_branch_once_its_latest_spawn_has_a_real_result`
- `src/worktree.rs:5054-5078` `sweep_terminal_reclaims_a_merged_branch_whose_latest_spawn_is_hung`
- `src/worktree.rs:5081-5104` `sweep_terminal_reclaims_a_merged_branch_with_no_spawn_recorded_at_all_unchanged`

#### `dup-0378` (near, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:5190-5230` `sweep_terminal_prints_evidence_for_a_kept_decision_but_not_for_a_removed_no_spawn_one`
- `src/worktree.rs:5233-5267` `sweep_terminal_prints_evidence_for_a_removed_terminal_spawn_decision`

#### `dup-0379` (near, 4 sites)

Proposed home: `worktree::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:5391-5432` `worktree_remove_reclaims_the_sibling_per_unit_cache`
- `src/worktree.rs:5435-5477` `worktree_remove_also_reclaims_the_sibling_mutants_root`
- `src/worktree.rs:5480-5515` `worktree_remove_also_reclaims_the_store_fence_sibling`
- `src/worktree.rs:5535-5564` `worktree_remove_also_reclaims_a_review_worktrees_store_fence_sibling`

#### `dup-0380` (near, 3 sites)

Proposed home: `worktree::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:5518-5532` `review_fence_sibling_maps_a_review_worktree_to_its_fence_sibling_and_ignores_the_rest`
- `src/worktree.rs:5846-5859` `unit_cache_sibling_maps_a_unit_worktree_to_its_cache_and_ignores_the_rest`
- `src/worktree.rs:5862-5874` `unit_mutants_sibling_maps_a_unit_worktree_to_its_mutants_root_and_ignores_the_rest`

#### `dup-0381` (near, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:5575-5636` `reclaim_worktree_on_branch_deregisters_the_lingering_worktree_reclaims_its_cache_and_frees_the_branch`
- `src/worktree.rs:5793-5843` `reclaim_worktree_on_branch_prunes_a_stale_registration_whose_dir_was_deleted_and_frees_the_branch`

#### `dup-0382` (near, 3 sites)

Proposed home: `worktree::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:6524-6600` `create_self_heals_a_corrupt_worktree_admin_entry_and_spares_healthy_ones`
- `src/worktree.rs:6603-6674` `create_heals_a_zero_length_gitdir_marker_the_commondir_case_leaves_untested`
- `src/worktree.rs:6677-6740` `create_heals_a_fully_missing_marker_not_just_a_truncated_one`

#### `dup-0383` (exact, 19 sites)

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

#### `dup-0384` (semantic, 20 sites)

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

#### `dup-0385` (exact, 3 sites)

Proposed home: `adaptive_labels_periphery::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/adaptive_labels_periphery.rs:467-479` `the_live_zoom_handler_toggles_labels_to_match_the_visible_set`
- `tests/adaptive_labels_periphery.rs:484-496` `the_declutter_holds_its_contract_at_the_edges_and_across_scales`
- `tests/adaptive_labels_periphery.rs:502-514` `a_layered_view_stays_byte_identical_and_a_titled_node_still_names_itself_on_hover`

#### `dup-0386` (exact, 4 sites)

Proposed home: `a new shared module (sites span 3 files: tests/adaptive_labels_periphery.rs, tests/subject_lens_overlay_client_arms.rs, tests/subject_view_memory_rail_client.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/adaptive_labels_periphery.rs:522-534` `the_real_concepts_drill_names_its_decluttered_shared_member_on_hover`
- `tests/subject_lens_overlay_client_arms.rs:290-303` `a_lens_flip_with_no_subject_reloads_the_whole_graph_overview`
- `tests/subject_lens_overlay_client_arms.rs:310-323` `a_failed_live_reprojection_fetch_degrades_to_a_message`
- `tests/subject_view_memory_rail_client.rs:207-220` `clicking_a_node_reveals_the_memory_rail_without_touching_the_neighborhood_panel`

#### `dup-0387` (exact, 4 sites)

Proposed home: `a new shared module (sites span 4 files: tests/adoption_keys_on_criterion_periphery.rs, tests/reap_before_removal_periphery.rs, tests/worktree_liveness_fence_periphery.rs, tests/worktree_remove_relocated_scratch_base_guard_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/adoption_keys_on_criterion_periphery.rs:180-195` `init_repo`
- `tests/reap_before_removal_periphery.rs:62-77` `init_repo`
- `tests/worktree_liveness_fence_periphery.rs:120-135` `init_repo`
- `tests/worktree_remove_relocated_scratch_base_guard_periphery.rs:76-91` `init_repo`

#### `dup-0388` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/adoption_keys_on_criterion_periphery.rs, tests/cli.rs, tests/worktree_liveness_fence_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/adoption_keys_on_criterion_periphery.rs:198-207` `git_out`
- `tests/cli.rs:126-136` `git_out`
- `tests/worktree_liveness_fence_periphery.rs:149-159` `git_out`

#### `dup-0389` (near, 2 sites)

Proposed home: `adoption_keys_on_criterion_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/adoption_keys_on_criterion_periphery.rs:405-421` `find_unit_started`
- `tests/adoption_keys_on_criterion_periphery.rs:854-873` `find_unit_integrated_commit`

#### `dup-0390` (near, 2 sites)

Proposed home: `adoption_keys_on_criterion_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/adoption_keys_on_criterion_periphery.rs:882-1027` `spec_scoping_blocks_adoption_across_specs_sharing_a_criterion_id_but_not_across_two_runs_of_the_same_spec`
- `tests/adoption_keys_on_criterion_periphery.rs:1694-1834` `a_reused_planner_slug_never_replays_an_unrelated_specs_recorded_adoption_decision`

#### `dup-0391` (near, 2 sites)

Proposed home: `adoption_keys_on_criterion_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/adoption_keys_on_criterion_periphery.rs:1114-1198` `a_compensation_reverted_integration_reopens_adoption_of_its_real_still_existing_branch`
- `tests/adoption_keys_on_criterion_periphery.rs:1206-1276` `a_plain_remediation_failure_after_integration_never_reopens_adoption_even_though_a_same_named_branch_exists`

#### `dup-0392` (near, 2 sites)

Proposed home: `adoption_keys_on_criterion_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/adoption_keys_on_criterion_periphery.rs:1287-1418` `a_crash_after_the_branch_exists_but_before_unitstarted_lands_recovers_the_recorded_adoption`
- `tests/adoption_keys_on_criterion_periphery.rs:1425-1547` `a_crash_after_the_provenance_record_but_before_the_branch_is_created_still_completes_the_adoption_on_resume`

#### `dup-0393` (near, 2 sites)

Proposed home: `adoption_keys_on_criterion_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/adoption_keys_on_criterion_periphery.rs:1978-2096` `an_escalated_units_unreclaimed_branch_is_never_reused_by_an_unrelated_specs_slug_collision`
- `tests/adoption_keys_on_criterion_periphery.rs:2124-2263` `a_genuine_retry_of_a_quarantined_criterion_adopts_from_the_quarantine_ref`

#### `dup-0394` (exact, 10 sites)

Proposed home: `a new shared module (sites span 8 files: tests/architecture_current_surface.rs, tests/cli.rs, tests/hermetic_test_git_audit.rs, tests/meta_phases_declaration_periphery.rs, tests/phase_of_role_mapping_periphery.rs, tests/review_tier_roster_periphery.rs, tests/step_attention_periphery.rs, tests/worker_persona_label_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/architecture_current_surface.rs:49-54` `architecture_text`
- `tests/cli.rs:3757-3762` `main_rs_source`
- `tests/cli.rs:4294-4299` `rigger_js_source`
- `tests/cli.rs:26308-26313` `rigger_workflow_yml_text`
- `tests/hermetic_test_git_audit.rs:83-89` `runner_script_text`
- `tests/meta_phases_declaration_periphery.rs:54-59` `rigger_js_source`
- `tests/phase_of_role_mapping_periphery.rs:38-43` `rigger_js_source`
- `tests/review_tier_roster_periphery.rs:44-49` `rigger_js_source`
- `tests/step_attention_periphery.rs:964-969` `rigger_js_source`
- `tests/worker_persona_label_periphery.rs:48-53` `rigger_js_source`

#### `dup-0395` (semantic, 2 sites)

Proposed home: `one shared `architecture_text` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/architecture_current_surface.rs:49-54` `architecture_text`
- `tests/architecture_integrity.rs:50-53` `architecture_text`

#### `dup-0396` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/architecture_current_surface.rs, tests/kurrentdb_contract_test_surface.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/architecture_current_surface.rs:57-63` `eventstore_source`
- `tests/kurrentdb_contract_test_surface.rs:38-45` `adapter_source`

#### `dup-0397` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/architecture_current_surface.rs, tests/readme_retirement_rationale.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/architecture_current_surface.rs:152-170` `architecture_names_the_current_store_and_inspector_surface`
- `tests/readme_retirement_rationale.rs:89-107` `readme_records_the_symbols_default_and_the_retirement_rationale`

#### `dup-0398` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/architecture_current_surface.rs, tests/readme_retirement_rationale.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/architecture_current_surface.rs:188-209` `architecture_names_no_retired_or_wrong_default_grounder`
- `tests/readme_retirement_rationale.rs:110-126` `readme_carries_none_of_the_retired_grounder_inversions`

#### `dup-0399` (exact, 4 sites)

Proposed home: `a new shared module (sites span 4 files: tests/build_budget_slots_periphery.rs, tests/build_env_authority_periphery.rs, tests/integrate_conflict_merge_periphery.rs, tests/store_flag_precedence.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/build_budget_slots_periphery.rs:206-221` `write_workflow`
- `tests/build_env_authority_periphery.rs:165-180` `write_workflow`
- `tests/integrate_conflict_merge_periphery.rs:412-427` `write_workflow`
- `tests/store_flag_precedence.rs:75-90` `write_workflow`

#### `dup-0400` (semantic, 6 sites)

Proposed home: `one shared `write_workflow` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 6 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/build_budget_slots_periphery.rs:206-221` `write_workflow`
- `tests/build_env_authority_periphery.rs:165-180` `write_workflow`
- `tests/integrate_conflict_merge_periphery.rs:412-427` `write_workflow`
- `tests/scratch_workdir_config.rs:37-39` `write_workflow`
- `tests/store_config.rs:40-42` `write_workflow`
- `tests/store_flag_precedence.rs:75-90` `write_workflow`

#### `dup-0401` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/build_env_authority_periphery.rs, tests/rigger_run_base_gate_env_periphery.rs, tests/spawn_target_dir_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/build_env_authority_periphery.rs:215-219` `env_test_lock`
- `tests/rigger_run_base_gate_env_periphery.rs:80-84` `env_test_lock`
- `tests/spawn_target_dir_periphery.rs:104-108` `spawn_env_test_lock`

#### `dup-0402` (semantic, 2 sites)

Proposed home: `one shared `env_test_lock` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/build_env_authority_periphery.rs:215-219` `env_test_lock`
- `tests/rigger_run_base_gate_env_periphery.rs:80-84` `env_test_lock`

#### `dup-0403` (exact, 2 sites)

Proposed home: `build_env_authority_periphery::real_driver_spy`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/build_env_authority_periphery.rs:369-376` `new`
- `tests/rigger_run_base_gate_env_periphery.rs:120-127` `new`

#### `dup-0404` (exact, 2 sites)

Proposed home: `build_env_authority_periphery::real_driver_spy`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/build_env_authority_periphery.rs:384-394` `spawn`
- `tests/rigger_run_base_gate_env_periphery.rs:135-145` `spawn`

#### `dup-0405` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/build_env_authority_periphery.rs, tests/rigger_run_base_gate_env_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/build_env_authority_periphery.rs:413-466` `run_once`
- `tests/rigger_run_base_gate_env_periphery.rs:155-206` `run_once`

#### `dup-0406` (near, 3 sites)

Proposed home: `build_env_authority_periphery::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/build_env_authority_periphery.rs:501-565` `one_build_environment_authority_reaches_a_real_gate_subprocess_and_a_real_agent_subprocess`
- `tests/build_env_authority_periphery.rs:641-693` `jobs_cap_coexists_with_a_configured_wrapper_at_both_real_injection_sites`
- `tests/build_env_authority_periphery.rs:720-766` `auto_wrapper_resolves_to_a_real_probed_binary_and_reaches_both_real_subprocesses`

#### `dup-0407` (near, 2 sites)

Proposed home: `build_env_authority_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/build_env_authority_periphery.rs:932-1007` `run_propagates_a_named_wrappers_uncreatable_cache_dir_at_the_library_entry_point`
- `tests/build_env_authority_periphery.rs:1025-1104` `run_propagates_a_named_wrappers_preexisting_unwritable_cache_dir_at_the_library_entry_point`

#### `dup-0408` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/build_watch_paths.rs, tests/gitsemver_worktree_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/build_watch_paths.rs:78-85` `fixture_repo`
- `tests/gitsemver_worktree_periphery.rs:97-112` `fixture_repo`

#### `dup-0409` (semantic, 3 sites)

Proposed home: `one shared `fixture_repo` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/build_watch_paths.rs:78-85` `fixture_repo`
- `tests/gitsemver_derivation.rs:61-76` `fixture_repo`
- `tests/gitsemver_worktree_periphery.rs:97-112` `fixture_repo`

#### `dup-0410` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/calls_down_execution_path_periphery.rs, tests/community_resolution_knob.rs, tests/concepts_fold_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/calls_down_execution_path_periphery.rs:102-106` `node_ids`
- `tests/community_resolution_knob.rs:192-201` `community_nodes`
- `tests/concepts_fold_periphery.rs:80-89` `concept_nodes`

#### `dup-0411` (near, 2 sites)

Proposed home: `calls_down_execution_path_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/calls_down_execution_path_periphery.rs:314-355` `the_depth_bound_clamps_the_layers_the_walk_returns`
- `tests/calls_down_execution_path_periphery.rs:796-864` `the_up_walk_clamps_the_caller_dag_to_the_depth_bound_and_emits_a_deterministic_layered_order`

#### `dup-0412` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/canary_false_positives_periphery.rs, tests/canary_unattributed_rejects_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/canary_false_positives_periphery.rs:55-120` `spawn`
- `tests/canary_unattributed_rejects_periphery.rs:54-107` `spawn`

#### `dup-0413` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/canary_false_positives_periphery.rs, tests/canary_tolerant_attribution_periphery.rs, tests/canary_unattributed_rejects_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/canary_false_positives_periphery.rs:138-145` `panel`
- `tests/canary_tolerant_attribution_periphery.rs:103-110` `panel`
- `tests/canary_unattributed_rejects_periphery.rs:125-132` `panel`

#### `dup-0414` (near, 5 sites)

Proposed home: `a new shared module (sites span 5 files: tests/canary_false_positives_periphery.rs, tests/canary_findings_volume_periphery.rs, tests/canary_item_sharding_jobs_cap_periphery.rs, tests/canary_progress_hook_periphery.rs, tests/canary_unattributed_rejects_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/canary_false_positives_periphery.rs:147-161` `item`
- `tests/canary_findings_volume_periphery.rs:133-147` `item`
- `tests/canary_item_sharding_jobs_cap_periphery.rs:65-79` `item`
- `tests/canary_progress_hook_periphery.rs:71-85` `item`
- `tests/canary_unattributed_rejects_periphery.rs:134-148` `item`

#### `dup-0415` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/canary_false_positives_periphery.rs, tests/canary_unattributed_rejects_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/canary_false_positives_periphery.rs:170-297` `run_canary_scores_false_positive_controls_and_project_canary_counts_them`
- `tests/canary_unattributed_rejects_periphery.rs:157-290` `run_canary_scores_an_unattributed_correct_reject_and_project_canary_counts_only_it`

#### `dup-0416` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/canary_item_sharding_jobs_cap_periphery.rs, tests/canary_progress_hook_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/canary_item_sharding_jobs_cap_periphery.rs:84-90` `anchor_of`
- `tests/canary_progress_hook_periphery.rs:91-97` `anchor_of`

#### `dup-0417` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: tests/canary_item_sharding_jobs_cap_periphery.rs, tests/canary_progress_hook_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/canary_item_sharding_jobs_cap_periphery.rs:202-236` `spawn`
- `tests/canary_item_sharding_jobs_cap_periphery.rs:288-317` `spawn`
- `tests/canary_progress_hook_periphery.rs:107-134` `spawn`

#### `dup-0418` (near, 17 sites)

Proposed home: `a new shared module (sites span 17 files: tests/canary_model_drift_periphery.rs, tests/cause_wire_periphery.rs, tests/cli.rs, tests/escalation_resume_periphery.rs, tests/graph_around_code_first.rs, tests/graph_around_governance_boundaries.rs, tests/graph_show_periphery.rs, tests/graph_show_staleness.rs, tests/graph_show_surface.rs, tests/reset_build_cache_periphery.rs, tests/reset_menu.rs, tests/reset_menu_identity_migration_periphery.rs, tests/spawn_scratch_reap_authorized_root_periphery.rs, tests/spec_lint.rs, tests/validate_footprint_default_scratch_root_periphery.rs, tests/watchdog_cli_periphery.rs, tests/workflow_definition_and_js_constants_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/canary_model_drift_periphery.rs:39-46` `temp_project`
- `tests/cause_wire_periphery.rs:54-61` `temp_project`
- `tests/cli.rs:19-29` `temp_project`
- `tests/escalation_resume_periphery.rs:79-86` `temp_project`
- `tests/graph_around_code_first.rs:33-40` `temp_project`
- `tests/graph_around_governance_boundaries.rs:40-47` `temp_project`
- `tests/graph_show_periphery.rs:55-62` `temp_project`
- `tests/graph_show_staleness.rs:40-47` `temp_project`
- `tests/graph_show_surface.rs:36-43` `temp_project`
- `tests/reset_build_cache_periphery.rs:36-43` `temp_project`
- `tests/reset_menu.rs:37-44` `temp_project`
- `tests/reset_menu_identity_migration_periphery.rs:32-39` `temp_project`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:45-52` `temp_project`
- `tests/spec_lint.rs:23-30` `temp_project`
- `tests/validate_footprint_default_scratch_root_periphery.rs:26-33` `temp_project`
- `tests/watchdog_cli_periphery.rs:44-51` `temp_project`
- `tests/workflow_definition_and_js_constants_periphery.rs:78-85` `temp_project`

#### `dup-0419` (semantic, 24 sites)

Proposed home: `one shared `temp_project` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 24 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/canary_model_drift_periphery.rs:39-46` `temp_project`
- `tests/cause_wire_periphery.rs:54-61` `temp_project`
- `tests/cli.rs:19-29` `temp_project`
- `tests/escalation_resume_periphery.rs:79-86` `temp_project`
- `tests/graph_around_code_first.rs:33-40` `temp_project`
- `tests/graph_around_governance_boundaries.rs:40-47` `temp_project`
- `tests/graph_show_periphery.rs:55-62` `temp_project`
- `tests/graph_show_staleness.rs:40-47` `temp_project`
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
- `tests/workflow_definition_and_js_constants_periphery.rs:78-85` `temp_project`

#### `dup-0420` (exact, 15 sites)

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

#### `dup-0421` (near, 2 sites)

Proposed home: `canary_model_drift_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/canary_model_drift_periphery.rs:151-204` `canary_and_validate_treat_an_unattributed_tier_as_unmeasured_never_defaulted_from_output_prose`
- `tests/canary_model_drift_periphery.rs:213-293` `canary_and_validate_drift_reads_only_metadata_prose_can_neither_mask_nor_forge_it`

#### `dup-0422` (near, 2 sites)

Proposed home: `canary_tolerant_attribution_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/canary_tolerant_attribution_periphery.rs:130-155` `a_tolerant_match_in_a_later_about_entry_still_scores_the_catch`
- `tests/canary_tolerant_attribution_periphery.rs:166-190` `an_empty_about_entry_never_scores_a_catch_even_against_a_trailing_slash_anchor`

#### `dup-0423` (exact, 8 sites)

Proposed home: `a new shared module (sites span 8 files: tests/cause_wire_periphery.rs, tests/cli.rs, tests/escalation_resume_periphery.rs, tests/graph_around_code_first.rs, tests/graph_around_governance_boundaries.rs, tests/heartbeat_write_read_agree_periphery.rs, tests/spawn_scratch_reap_authorized_root_periphery.rs, tests/watchdog_cli_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cause_wire_periphery.rs:65-69` `seed_store`
- `tests/cli.rs:38-42` `seed_store`
- `tests/escalation_resume_periphery.rs:90-94` `seed_store`
- `tests/graph_around_code_first.rs:44-48` `seed_store`
- `tests/graph_around_governance_boundaries.rs:51-55` `seed_store`
- `tests/heartbeat_write_read_agree_periphery.rs:118-122` `seed_store`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:56-60` `seed_store`
- `tests/watchdog_cli_periphery.rs:56-60` `seed_store`

#### `dup-0424` (near, 19 sites)

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

#### `dup-0425` (semantic, 24 sites)

Proposed home: `one shared `run_stream_identity` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 24 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/cause_wire_periphery.rs:75-97` `run_stream_identity`
- `tests/change_path_revert_periphery.rs:130-155` `run_stream_identity`
- `tests/cli.rs:49-71` `run_stream_identity`
- `tests/dedup_seeding_periphery.rs:580-605` `run_stream_identity`
- `tests/escalation_resume_periphery.rs:99-121` `run_stream_identity`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:165-187` `run_stream_identity`
- `tests/graph_around_code_first.rs:52-68` `run_stream_identity`
- `tests/graph_around_governance_boundaries.rs:59-75` `run_stream_identity`
- `tests/graph_show_periphery.rs:67-83` `run_stream_identity`
- `tests/graph_show_staleness.rs:52-68` `run_stream_identity`
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
- `tests/workflow_definition_and_js_constants_periphery.rs:117-133` `run_stream_identity`
- `tests/worktree_liveness_fence_periphery.rs:164-186` `run_stream_identity`

#### `dup-0426` (near, 7 sites)

Proposed home: `a new shared module (sites span 7 files: tests/cause_wire_periphery.rs, tests/escalation_resume_periphery.rs, tests/heartbeat_write_read_agree_periphery.rs, tests/reset_derived_live_writer_guard_periphery.rs, tests/reset_menu.rs, tests/reset_menu_identity_migration_periphery.rs, tests/watchdog_cli_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cause_wire_periphery.rs:103-116` `seed_run_events`
- `tests/escalation_resume_periphery.rs:126-139` `seed_run_events`
- `tests/heartbeat_write_read_agree_periphery.rs:151-164` `seed_run_events`
- `tests/reset_derived_live_writer_guard_periphery.rs:91-103` `seed_run_events`
- `tests/reset_menu.rs:107-119` `seed_run_events`
- `tests/reset_menu_identity_migration_periphery.rs:95-107` `seed_run_events`
- `tests/watchdog_cli_periphery.rs:93-106` `seed_run_events`

#### `dup-0427` (semantic, 11 sites)

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

#### `dup-0428` (near, 13 sites)

Proposed home: `a new shared module (sites span 3 files: tests/cause_wire_periphery.rs, tests/cli.rs, tests/escalation_resume_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cause_wire_periphery.rs:195-217` `a_legacy_causeless_unit_failed_event_survives_a_real_store_round_trip_and_renders_unknown`
- `tests/cause_wire_periphery.rs:224-256` `the_reject_recurrence_line_names_the_latest_of_several_recorded_causes_through_the_real_binary`
- `tests/cli.rs:11035-11073` `stats_reports_the_latest_run_by_default_and_all_for_the_aggregate`
- `tests/cli.rs:22088-22147` `release_ready_handoff_surfaces_on_status_for_a_done_run`
- `tests/cli.rs:22357-22398` `release_ready_is_silent_on_status_for_an_unfinished_run`
- `tests/cli.rs:22469-22502` `release_ready_pluralizes_the_unit_count_on_status_for_a_multi_unit_run`
- `tests/cli.rs:22547-22569` `release_ready_is_silent_on_status_for_a_spec_defective_run`
- `tests/escalation_resume_periphery.rs:183-207` `a_unit_resumed_event_seeded_directly_through_a_real_store_reaches_status_without_the_command`
- `tests/escalation_resume_periphery.rs:218-247` `a_legacy_shaped_unit_resumed_event_missing_both_optional_fields_survives_a_real_store_round_trip`
- `tests/escalation_resume_periphery.rs:336-367` `the_resumed_banner_survives_a_genuinely_in_flight_re_parked_attempt_not_yet_resolved`
- `tests/escalation_resume_periphery.rs:444-465` `resume_unit_rejects_an_unknown_unit_absent_from_the_run`
- `tests/escalation_resume_periphery.rs:472-496` `resume_unit_refuses_when_the_recorded_branch_was_never_created`
- `tests/escalation_resume_periphery.rs:503-526` `resume_unit_refuses_an_already_integrated_unit`

#### `dup-0429` (near, 10 sites)

Proposed home: `a new shared module (sites span 10 files: tests/change_path_revert_periphery.rs, tests/dedup_seeding_periphery.rs, tests/graph_around_code_first.rs, tests/graph_around_governance_boundaries.rs, tests/graph_show_periphery.rs, tests/graph_show_staleness.rs, tests/graph_show_surface.rs, tests/relocated_worktree_store_resolution_periphery.rs, tests/validate_footprint_default_scratch_root_periphery.rs, tests/workflow_definition_and_js_constants_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/change_path_revert_periphery.rs:58-72` `run_rigger`
- `tests/dedup_seeding_periphery.rs:655-671` `run_rigger`
- `tests/graph_around_code_first.rs:79-93` `run_rigger`
- `tests/graph_around_governance_boundaries.rs:86-100` `run_rigger`
- `tests/graph_show_periphery.rs:94-108` `run_rigger`
- `tests/graph_show_staleness.rs:99-113` `run_rigger`
- `tests/graph_show_surface.rs:75-89` `run_rigger`
- `tests/relocated_worktree_store_resolution_periphery.rs:102-116` `run_rigger`
- `tests/validate_footprint_default_scratch_root_periphery.rs:48-62` `run_rigger`
- `tests/workflow_definition_and_js_constants_periphery.rs:92-106` `run_rigger`

#### `dup-0430` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/change_path_revert_periphery.rs, tests/dedup_seeding_periphery.rs, tests/workflow_definition_and_js_constants_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/change_path_revert_periphery.rs:77-83` `ingested_count`
- `tests/dedup_seeding_periphery.rs:676-682` `ingested_count`
- `tests/workflow_definition_and_js_constants_periphery.rs:138-144` `ingested_count`

#### `dup-0431` (semantic, 3 sites)

Proposed home: `one shared `ingested_count` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/change_path_revert_periphery.rs:77-83` `ingested_count`
- `tests/dedup_seeding_periphery.rs:676-682` `ingested_count`
- `tests/workflow_definition_and_js_constants_periphery.rs:138-144` `ingested_count`

#### `dup-0432` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/change_path_revert_periphery.rs, tests/dedup_seeding_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/change_path_revert_periphery.rs:159-175` `read_run_stream`
- `tests/dedup_seeding_periphery.rs:630-646` `read_run_stream`

#### `dup-0433` (semantic, 3 sites)

Proposed home: `one shared `read_run_stream` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/change_path_revert_periphery.rs:159-175` `read_run_stream`
- `tests/dedup_seeding_periphery.rs:630-646` `read_run_stream`
- `tests/reset_derived_compaction_periphery.rs:2836-2840` `read_run_stream`

#### `dup-0434` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/checkin_mutation_diff_base_periphery.rs, tests/halted_spawn_wip_recovery_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/checkin_mutation_diff_base_periphery.rs:83-90` `run_git`
- `tests/halted_spawn_wip_recovery_periphery.rs:89-96` `run_git`

#### `dup-0435` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/checkin_mutation_diff_base_periphery.rs, tests/halted_spawn_wip_recovery_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/checkin_mutation_diff_base_periphery.rs:92-99` `git_ok`
- `tests/halted_spawn_wip_recovery_periphery.rs:98-105` `git_ok`

#### `dup-0436` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/checkin_mutation_diff_base_periphery.rs, tests/halted_spawn_wip_recovery_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/checkin_mutation_diff_base_periphery.rs:101-109` `git_out`
- `tests/halted_spawn_wip_recovery_periphery.rs:107-115` `git_out`

#### `dup-0437` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/checkin_mutation_diff_base_periphery.rs, tests/store_resolution.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/checkin_mutation_diff_base_periphery.rs:123-140` `the_shipped_mutation_gate_guards_on_rigger_run_base_never_a_merge_base`
- `tests/store_resolution.rs:119-135` `the_single_resolver_exists_and_the_old_per_command_helper_is_retired`

#### `dup-0438` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/checkpoint_commit_hook_bypass_periphery.rs, tests/revert_on_base_hook_bypass_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/checkpoint_commit_hook_bypass_periphery.rs:70-76` `install_refusing_hook`
- `tests/revert_on_base_hook_bypass_periphery.rs:81-87` `install_refusing_hook`

#### `dup-0439` (semantic, 2 sites)

Proposed home: `one shared `install_refusing_hook` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/checkpoint_commit_hook_bypass_periphery.rs:70-76` `install_refusing_hook`
- `tests/revert_on_base_hook_bypass_periphery.rs:81-87` `install_refusing_hook`

#### `dup-0440` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/checkpoint_commit_hook_bypass_periphery.rs, tests/integrate_conflict_merge_periphery.rs, tests/revert_on_base_hook_bypass_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/checkpoint_commit_hook_bypass_periphery.rs:93-99` `review_panel`
- `tests/integrate_conflict_merge_periphery.rs:363-369` `review_panel`
- `tests/revert_on_base_hook_bypass_periphery.rs:104-110` `review_panel`

#### `dup-0441` (semantic, 3 sites)

Proposed home: `one shared `review_panel` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/checkpoint_commit_hook_bypass_periphery.rs:93-99` `review_panel`
- `tests/integrate_conflict_merge_periphery.rs:363-369` `review_panel`
- `tests/revert_on_base_hook_bypass_periphery.rs:104-110` `review_panel`

#### `dup-0442` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/checkpoint_commit_hook_bypass_periphery.rs, tests/integrate_conflict_merge_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/checkpoint_commit_hook_bypass_periphery.rs:101-111` `mk_stage`
- `tests/integrate_conflict_merge_periphery.rs:388-398` `mk_stage`

#### `dup-0443` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/checkpoint_commit_hook_bypass_periphery.rs, tests/revert_on_base_hook_bypass_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/checkpoint_commit_hook_bypass_periphery.rs:149-210` `a_pre_gate_attempt_commit_bypasses_an_installed_refusing_hook`
- `tests/revert_on_base_hook_bypass_periphery.rs:170-261` `a_compensation_revert_bypasses_an_installed_refusing_hook`

#### `dup-0444` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/cli.rs, tests/fanout_template_needs_and_stage_retries_periphery.rs, tests/step_attention_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:82-100` `seed_run_events`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:142-160` `seed_run_events`
- `tests/step_attention_periphery.rs:178-196` `seed_run_events`

#### `dup-0445` (semantic, 7 sites)

Proposed home: `one shared `temp_git_project_with_commit` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 7 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/cli.rs:105-122` `temp_git_project_with_commit`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:113-130` `temp_git_project_with_commit`
- `tests/halted_spawn_wip_recovery_periphery.rs:119-127` `temp_git_project_with_commit`
- `tests/step_attention_periphery.rs:464-484` `temp_git_project_with_commit`
- `tests/step_root_resolution_periphery.rs:140-161` `temp_git_project_with_commit`
- `tests/workflow_driver_resolved_model_periphery.rs:57-78` `temp_git_project_with_commit`
- `tests/worktree_liveness_fence_periphery.rs:94-115` `temp_git_project_with_commit`

#### `dup-0446` (exact, 4 sites)

Proposed home: `a new shared module (sites span 4 files: tests/cli.rs, tests/halted_spawn_wip_recovery_periphery.rs, tests/reset_derived_compaction.rs, tests/step_root_resolution_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:139-141` `run_rigger`
- `tests/halted_spawn_wip_recovery_periphery.rs:184-186` `run_rigger`
- `tests/reset_derived_compaction.rs:82-84` `run_rigger`
- `tests/step_root_resolution_periphery.rs:213-215` `run_rigger`

#### `dup-0447` (exact, 6 sites)

Proposed home: `a new shared module (sites span 6 files: tests/cli.rs, tests/halted_spawn_wip_recovery_periphery.rs, tests/reset_derived_compaction.rs, tests/reset_derived_live_writer_guard_periphery.rs, tests/spawn_scratch_reap_authorized_root_periphery.rs, tests/step_root_resolution_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:146-172` `run_rigger_envs`
- `tests/halted_spawn_wip_recovery_periphery.rs:165-180` `run_rigger_envs`
- `tests/reset_derived_compaction.rs:86-101` `run_rigger_envs`
- `tests/reset_derived_live_writer_guard_periphery.rs:108-123` `run_rigger`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:118-133` `run_rigger_envs`
- `tests/step_root_resolution_periphery.rs:196-211` `run_rigger_envs`

#### `dup-0448` (semantic, 5 sites)

Proposed home: `one shared `run_rigger_envs` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 5 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/cli.rs:146-172` `run_rigger_envs`
- `tests/halted_spawn_wip_recovery_periphery.rs:165-180` `run_rigger_envs`
- `tests/reset_derived_compaction.rs:86-101` `run_rigger_envs`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:118-133` `run_rigger_envs`
- `tests/step_root_resolution_periphery.rs:196-211` `run_rigger_envs`

#### `dup-0449` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/cli.rs, tests/worktree_liveness_fence_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:189-197` `git_ok`
- `tests/worktree_liveness_fence_periphery.rs:138-146` `git_ok`

#### `dup-0450` (near, 6 sites)

Proposed home: `cli::support (consolidate these 6 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:352-375` `emit_review_finding_shows_in_peers`
- `tests/cli.rs:9076-9179` `step_surfaces_a_hung_unbounded_spawn_recorded_as_a_liveness_fault_by_the_driver`
- `tests/cli.rs:9204-9267` `step_attention_never_restamps_a_hung_unbounded_spawn_when_repo_less`
- `tests/cli.rs:12548-12612` `a_liveness_fault_on_a_review_spawn_halts_instead_of_re_parking`
- `tests/cli.rs:15889-15919` `result_if_absent_records_when_the_spawn_is_unanswered`
- `tests/cli.rs:15927-15965` `result_if_absent_never_clobbers_a_self_reported_success`

#### `dup-0451` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:871-1008` `reset_runs_compacts_the_on_disk_graph_after_reclaiming_superseded_rows`
- `tests/cli.rs:1038-1183` `reset_runs_reports_nonzero_bytes_reclaimed_then_a_second_pass_is_an_idempotent_no_op`

#### `dup-0452` (semantic, 2 sites)

Proposed home: `one shared `reported_reclaimed_bytes` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/cli.rs:1014-1020` `reported_reclaimed_bytes`
- `tests/reset_derived_compaction_periphery.rs:4410-4423` `reported_reclaimed_bytes`

#### `dup-0453` (exact, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:1372-1389` `prompt_refuses_to_fabricate_a_store_when_none_exists`
- `tests/cli.rs:1561-1578` `scratch_refuses_to_fabricate_a_store_when_none_exists`

#### `dup-0454` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:1813-1864` `result_from_a_relocated_git_worktree_outside_the_repo_records_into_the_repo_stream`
- `tests/cli.rs:1877-1919` `result_from_a_configured_nested_git_worktree_records_into_the_repo_stream`

#### `dup-0455` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:2070-2141` `a_spawns_scratch_is_reclaimed_the_moment_its_result_is_recorded_for_every_outcome`
- `tests/cli.rs:2157-2231` `a_spawns_mutation_scratch_is_reclaimed_the_moment_its_own_result_reports_for_every_outcome`

#### `dup-0456` (near, 4 sites)

Proposed home: `cli::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:2245-2280` `a_reviewers_result_never_reclaims_the_implementers_mutation_scratch`
- `tests/cli.rs:2293-2336` `a_dotdot_spawn_id_never_escapes_the_registered_scratch_roots`
- `tests/cli.rs:2353-2406` `a_dotdot_spawn_id_never_escapes_the_pre_existing_agent_scratch_root_either`
- `tests/cli.rs:2427-2465` `a_leading_slash_spawn_id_never_collapses_the_reclaim_to_its_registered_root`

#### `dup-0457` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:2555-2603` `two_speculation_lanes_of_the_same_unit_get_distinct_mutation_scratch_dirs`
- `tests/cli.rs:9946-10010` `a_terminal_units_registered_mutation_scratch_is_reaped_while_a_live_siblings_survives`
- `tests/cli.rs:10776-10893` `a_resumed_run_reaps_an_escalated_and_an_on_pass_none_settled_units_registered_mutation_scratch_not_just_an_integrated_ones`

#### `dup-0458` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:2611-2622` `write_grounder_workflow`
- `tests/cli.rs:17607-17622` `write_gating_lint_project`

#### `dup-0459` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:3162-3194` `ground_via_symbols_grounder_ranks_a_definition_first`
- `tests/cli.rs:3273-3302` `ground_via_symbols_grounder_ranks_a_genuinely_rare_contains_tier_entity_above_common_ones_sharing_its_substring`

#### `dup-0460` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:3781-3837` `worktree_sweep_completes_before_any_add_within_one_step`
- `tests/cli.rs:9285-9320` `the_hung_cursor_is_persisted_only_after_the_step_that_carries_it_is_printed`

#### `dup-0461` (near, 4 sites)

Proposed home: `cli::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:3960-4007` `step_from_a_linked_worktree_refuses_naming_both_trees`
- `tests/cli.rs:4014-4051` `run_from_a_linked_worktree_refuses_naming_both_trees`
- `tests/cli.rs:4058-4095` `workflow_from_a_linked_worktree_refuses_naming_both_trees`
- `tests/cli.rs:4108-4158` `serve_from_a_linked_worktree_refuses_naming_both_trees`

#### `dup-0462` (semantic, 6 sites)

Proposed home: `one shared `rigger_js_source` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 6 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/cli.rs:4294-4299` `rigger_js_source`
- `tests/meta_phases_declaration_periphery.rs:54-59` `rigger_js_source`
- `tests/phase_of_role_mapping_periphery.rs:38-43` `rigger_js_source`
- `tests/review_tier_roster_periphery.rs:44-49` `rigger_js_source`
- `tests/step_attention_periphery.rs:964-969` `rigger_js_source`
- `tests/worker_persona_label_periphery.rs:48-53` `rigger_js_source`

#### `dup-0463` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/cli.rs, tests/worker_persona_label_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:4552-4608` `native_driver_scratch_policy_directs_the_worker_to_its_own_spawn_owned_container`
- `tests/worker_persona_label_periphery.rs:413-434` `workerlabel_is_actually_wired_into_runworkers_agent_call_label`

#### `dup-0464` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/cli.rs, tests/projections_stay_local.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:4832-4845` `native_driver_drains_in_flight_workers_before_a_loud_stop`
- `tests/projections_stay_local.rs:101-119` `the_graph_and_progress_projections_open_via_the_local_sqlite_constructors`

#### `dup-0465` (near, 17 sites)

Proposed home: `a new shared module (sites span 6 files: tests/cli.rs, tests/halted_spawn_wip_recovery_periphery.rs, tests/step_attention_periphery.rs, tests/step_root_resolution_periphery.rs, tests/workflow_driver_resolved_model_periphery.rs, tests/worktree_liveness_fence_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:4852-4876` `write_two_stage_workflow`
- `tests/cli.rs:4881-4905` `write_budget_one_two_stage_workflow`
- `tests/cli.rs:4991-5011` `write_standalone_review_workflow`
- `tests/cli.rs:5402-5426` `write_reviewless_git_unit_workflow`
- `tests/cli.rs:5433-5458` `write_reviewless_git_escalating_unit_workflow`
- `tests/cli.rs:8349-8374` `write_failing_gate_escalating_workflow`
- `tests/cli.rs:8383-8408` `write_manual_review_workflow`
- `tests/cli.rs:8611-8634` `write_budget_one_dependency_workflow`
- `tests/cli.rs:8701-8714` `write_liveness_workflow`
- `tests/cli.rs:9049-9062` `write_unbounded_liveness_workflow`
- `tests/cli.rs:12243-12274` `write_gated_reviewed_workflow`
- `tests/halted_spawn_wip_recovery_periphery.rs:134-158` `write_solo_unit_workflow`
- `tests/step_attention_periphery.rs:221-246` `write_attention_progression_workflow`
- `tests/step_attention_periphery.rs:492-518` `write_attention_ordering_workflow`
- `tests/step_root_resolution_periphery.rs:167-191` `write_reviewless_git_unit_workflow`
- `tests/workflow_driver_resolved_model_periphery.rs:85-98` `write_one_stage_workflow`
- `tests/worktree_liveness_fence_periphery.rs:234-258` `write_reviewless_git_unit_workflow`

#### `dup-0466` (semantic, 3 sites)

Proposed home: `one shared `write_reviewless_git_unit_workflow` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/cli.rs:5402-5426` `write_reviewless_git_unit_workflow`
- `tests/step_root_resolution_periphery.rs:167-191` `write_reviewless_git_unit_workflow`
- `tests/worktree_liveness_fence_periphery.rs:234-258` `write_reviewless_git_unit_workflow`

#### `dup-0467` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:5473-5548` `step_reclaims_the_units_worktree_and_deletes_its_branch_on_a_clean_integrate`
- `tests/cli.rs:5563-5631` `step_reclaims_the_units_worktree_but_keeps_its_branch_on_a_terminal_escalation`

#### `dup-0468` (exact, 4 sites)

Proposed home: `a new shared module (sites span 2 files: tests/cli.rs, tests/watchdog_cli_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:5784-5798` `resume_unit_refuses_an_unknown_unit`
- `tests/cli.rs:11645-11656` `step_rejects_an_unknown_flag`
- `tests/cli.rs:11695-11706` `step_rejects_base_without_a_value`
- `tests/watchdog_cli_periphery.rs:356-370` `watch_rejects_an_unknown_flag_through_the_real_binary_with_a_nonzero_exit`

#### `dup-0469` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:5870-6079` `step_restores_the_unit_worktree_a_gate_deletes_before_the_review_spawn`
- `tests/cli.rs:6305-6552` `step_stamps_a_real_reviewed_sha_after_repeated_between_step_deletions`
- `tests/cli.rs:6735-6878` `step_stamps_a_real_failed_sha_after_a_deletion_before_the_reject_stamp`

#### `dup-0470` (near, 5 sites)

Proposed home: `cli::support (consolidate these 5 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:6918-7080` `run_end_to_end_restores_a_worktree_a_reviewer_agent_deletes_mid_review`
- `tests/cli.rs:7365-7518` `speculation_reject_worktree_sha_is_stamped_after_the_adjudicators_own_deletion_is_restored_end_to_end`
- `tests/cli.rs:10285-10424` `a_speculation_winners_registered_mutation_scratch_across_all_lanes_is_reaped_by_the_real_winner_integrate_teardown`
- `tests/cli.rs:10438-10576` `a_speculation_escalations_registered_mutation_scratch_across_all_lanes_is_reaped_by_the_real_escalation_tail_teardown`
- `tests/cli.rs:10599-10749` `a_speculation_on_pass_none_winners_registered_mutation_scratch_across_all_lanes_is_reaped_by_the_real_on_pass_none_exit_teardown`

#### `dup-0471` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:7542-7687` `resumed_reviewed_unit_stamps_a_real_failed_sha_after_the_exhaustive_gates_own_deletion_is_restored`
- `tests/cli.rs:7710-7887` `resumed_reviewed_unit_stamps_a_real_failed_sha_after_the_post_merge_re_gates_own_deletion_is_restored`

#### `dup-0472` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:8195-8256` `run_registers_a_credential_free_shared_instance`
- `tests/cli.rs:8276-8341` `run_driver_workflow_registers_a_credential_free_shared_instance`

#### `dup-0473` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/cli.rs, tests/step_attention_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:8418-8510` `step_carries_the_escalated_set_when_a_fixpoint_is_reached_with_a_wedged_unit`
- `tests/step_attention_periphery.rs:252-351` `recurrence_and_stalled_frontier_survive_real_process_boundaries`

#### `dup-0474` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/cli.rs, tests/step_attention_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:8719-8729` `plant_stale_marker`
- `tests/step_attention_periphery.rs:416-426` `plant_stale_marker`

#### `dup-0475` (semantic, 2 sites)

Proposed home: `one shared `plant_stale_marker` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/cli.rs:8719-8729` `plant_stale_marker`
- `tests/step_attention_periphery.rs:416-426` `plant_stale_marker`

#### `dup-0476` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:9621-9669` `run_teardown_reclaims_run_level_scratch_at_a_definition_drift_halt`
- `tests/cli.rs:9686-9742` `run_teardown_spares_run_level_scratch_at_a_drift_halt_while_a_hung_spawn_may_be_alive`

#### `dup-0477` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:9765-9792` `run_teardown_spares_run_level_scratch_at_a_manual_review_pause`
- `tests/cli.rs:9817-9865` `run_teardown_spares_run_level_scratch_at_a_drift_halt_while_a_manual_review_is_pending`
- `tests/cli.rs:9875-9923` `run_teardown_reclaims_run_level_scratch_after_a_manual_review_is_integrated`

#### `dup-0478` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:11120-11194` `stats_cli_renders_exact_per_role_spawn_timing_and_unpaired_disclosure`
- `tests/cli.rs:11268-11319` `stats_cli_excludes_suspect_non_positive_duration_pairs_as_unpaired_not_zero`

#### `dup-0479` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:11662-11690` `step_accepts_base_and_anchors_the_run_branch`
- `tests/cli.rs:11715-11759` `step_creates_run_branch_off_head_when_base_unresolvable`

#### `dup-0480` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:11769-11799` `step_refuses_when_there_is_no_reachable_base`
- `tests/cli.rs:11810-11841` `run_refuses_when_there_is_no_reachable_base`
- `tests/cli.rs:11851-11894` `run_workflow_refuses_when_there_is_no_reachable_base`

#### `dup-0481` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/cli.rs, tests/fanout_template_needs_and_stage_retries_periphery.rs, tests/step_attention_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:12231-12233` `temp_repoless_project`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:134-136` `temp_repoless_project`
- `tests/step_attention_periphery.rs:143-145` `temp_repoless_project`

#### `dup-0482` (semantic, 3 sites)

Proposed home: `one shared `temp_repoless_project` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/cli.rs:12231-12233` `temp_repoless_project`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:134-136` `temp_repoless_project`
- `tests/step_attention_periphery.rs:143-145` `temp_repoless_project`

#### `dup-0483` (exact, 4 sites)

Proposed home: `cli::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:12848-12865` `write_gated_workflow_no_review`
- `tests/cli.rs:12917-12934` `write_reviewed_workflow_no_gate`
- `tests/cli.rs:12989-13009` `write_reviewed_workflow_added_gate`
- `tests/cli.rs:13065-13087` `write_reviewed_workflow_extra_stage`

#### `dup-0484` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:12874-12910` `replay_candidate_column_reacts_to_a_changed_config`
- `tests/cli.rs:12943-12983` `replay_removing_a_gate_lowers_the_candidate_gate_runs`

#### `dup-0485` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:13021-13060` `replay_an_added_gate_fails_safe_and_never_fabricates_a_pass`
- `tests/cli.rs:13096-13128` `replay_an_uncovered_candidate_spawn_parks_and_still_prints_a_partial_column`

#### `dup-0486` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:13454-13478` `validate_fails_at_run_start_when_a_named_build_wrapper_is_absent_from_path`
- `tests/cli.rs:13971-13992` `validate_rejects_an_explicit_build_mutation_value_naming_spec_91_end_to_end`

#### `dup-0487` (near, 7 sites)

Proposed home: `cli::support (consolidate these 7 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:13485-13503` `validate_reports_none_when_auto_finds_no_known_wrapper_on_path`
- `tests/cli.rs:13508-13525` `validate_reports_the_resolved_wrapper_when_auto_finds_a_known_wrapper_on_path`
- `tests/cli.rs:13532-13562` `validate_reports_cache_dir_and_budget_alongside_the_wrapper`
- `tests/cli.rs:13704-13729` `validate_reports_none_when_autos_discovered_wrapper_has_an_uncreatable_cache_dir`
- `tests/cli.rs:13800-13826` `validate_reports_none_when_autos_discovered_wrapper_has_a_preexisting_unwritable_cache_dir`
- `tests/cli.rs:13920-13937` `validate_reports_mutation_gate_declared_when_cargo_mutants_is_resolvable`
- `tests/cli.rs:13944-13961` `validate_reports_mutation_gate_declared_by_default_on_a_fresh_scaffold`

#### `dup-0488` (exact, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:13667-13696` `validate_fails_at_run_start_when_a_named_wrappers_cache_dir_cannot_be_created`
- `tests/cli.rs:13761-13790` `validate_fails_at_run_start_when_a_named_wrappers_cache_dir_is_preexisting_but_unwritable`

#### `dup-0489` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:13857-13878` `validate_fails_at_run_start_when_the_scaffolded_mutation_gate_has_no_cargo_mutants_on_path`
- `tests/cli.rs:13895-13915` `validate_fails_before_any_output_when_the_mutation_gate_has_no_cargo_mutants`

#### `dup-0490` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:14383-14425` `validate_footprint_registered_scratch_roots_measures_the_real_mutation_scratch_root`
- `tests/cli.rs:14801-14911` `validate_footprint_worktrees_and_per_unit_caches_measure_real_dead_and_live_entries_through_the_binary`

#### `dup-0491` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:14447-14573` `validate_flags_registered_scratch_roots_dead_share_scoped_to_real_spawn_liveness_in_the_store`
- `tests/cli.rs:14593-14689` `validate_flags_a_prior_abandoned_runs_orphan_even_when_a_later_run_reuses_the_identical_spawn_id`

#### `dup-0492` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:15467-15535` `installed_workflow_courier_prompt_is_foreground_and_honest`
- `tests/cli.rs:15555-15633` `installed_workflow_courier_waits_on_an_auto_backgrounded_step`
- `tests/cli.rs:15810-15882` `installed_workflow_driver_guards_a_null_step`

#### `dup-0493` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:16172-16239` `step_halts_on_definition_drift_and_rebase_definition_continues`
- `tests/cli.rs:16245-16286` `a_fresh_run_repins_the_current_definition_and_never_halts`

#### `dup-0494` (near, 5 sites)

Proposed home: `cli::support (consolidate these 5 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:16416-16473` `stats_canary_reports_the_findings_raised_total_summed_across_items`
- `tests/cli.rs:16508-16562` `stats_canary_renders_na_for_a_tier_with_an_unattributed_correct_reject`
- `tests/cli.rs:16573-16622` `stats_canary_still_renders_a_genuine_zero_when_every_reject_has_attribution`
- `tests/cli.rs:16643-16698` `stats_canary_reports_the_control_false_positive_line`
- `tests/cli.rs:17007-17068` `stats_canary_reports_the_model_pinning_header_through_a_real_wire_event`

#### `dup-0495` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:16790-16823` `rigger_help_gives_the_jobs_flag_its_own_description_line_through_the_real_binary`
- `tests/cli.rs:16838-16896` `rigger_help_gives_the_model_flag_its_own_description_line_through_the_real_binary`

#### `dup-0496` (near, 5 sites)

Proposed home: `a new shared module (sites span 3 files: tests/cli.rs, tests/migration_is_deliberate_periphery.rs, tests/validate_advisories.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:17123-17172` `validate_warns_when_a_tier_resolved_model_repointed_between_runs`
- `tests/cli.rs:17179-17206` `validate_advises_softly_on_a_snapshot_only_date_suffix_bump`
- `tests/cli.rs:17246-17306` `validate_detects_a_stream_whose_position_order_and_revision_order_disagree`
- `tests/migration_is_deliberate_periphery.rs:566-591` `validate_warns_of_retired_code_entities_with_the_measured_count_and_never_fails`
- `tests/validate_advisories.rs:272-297` `validate_warns_of_log_bloat_with_the_measured_factor_and_names_reset_derived`

#### `dup-0497` (semantic, 2 sites)

Proposed home: `one shared `seed_order_signature` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/cli.rs:17218-17239` `seed_order_signature`
- `tests/watchdog_cli_periphery.rs:224-246` `seed_order_signature`

#### `dup-0498` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:17312-17332` `canary_if_model_changed_skips_when_the_model_is_unchanged`
- `tests/cli.rs:17339-17361` `canary_if_model_changed_runs_when_a_tier_resolved_model_repointed`
- `tests/cli.rs:17370-17404` `canary_if_model_changed_skips_a_snapshot_only_date_suffix_bump_without_running_the_panel`

#### `dup-0499` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:18327-18377` `status_reports_not_serving_when_the_recorded_marker_names_a_dead_dash`
- `tests/cli.rs:18390-18435` `status_never_names_the_unattributed_pid_sentinel_as_a_dead_process`

#### `dup-0500` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:18446-18505` `status_shows_the_url_when_the_recorded_marker_names_a_genuinely_serving_dash`
- `tests/cli.rs:18519-18586` `status_trusts_a_genuinely_alive_url_even_with_a_mismatched_marker`
- `tests/cli.rs:18602-18680` `status_reports_not_serving_when_a_mismatched_marker_leaves_a_dead_url_unverified`

#### `dup-0501` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:19017-19079` `docs_ships_graph_hygiene_guidance_to_consumers`
- `tests/cli.rs:19094-19137` `docs_ships_three_verb_lookup_guidance_to_consumers`
- `tests/cli.rs:29921-29977` `docs_installs_the_operator_lookup_rule_text_into_the_shipped_skill_and_handbook`

#### `dup-0502` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:19148-19220` `validate_fails_when_the_committed_using_rigger_docs_drift_and_passes_when_in_sync`
- `tests/cli.rs:19234-19287` `validate_docs_drift_gate_covers_the_second_registry_entry`
- `tests/cli.rs:19384-19438` `validate_docs_drift_gate_covers_the_planning_field_guide_page`

#### `dup-0503` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:19615-19658` `docs_renders_every_per_operation_skill_through_the_compiled_binary`
- `tests/cli.rs:19838-19933` `docs_renders_every_watching_discipline_skill_through_the_compiled_binary`

#### `dup-0504` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:19672-19731` `validate_docs_drift_gate_covers_each_per_operation_skill`
- `tests/cli.rs:19947-20014` `validate_docs_drift_gate_covers_each_watching_discipline_skill`

#### `dup-0505` (exact, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:19745-19806` `setup_installs_every_per_operation_skill_into_the_consumer_project`
- `tests/cli.rs:20028-20089` `setup_installs_every_watching_discipline_skill_into_the_consumer_project`

#### `dup-0506` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:20182-20220` `watch_once_never_names_the_unattributed_pid_sentinel_when_no_url_is_recorded`
- `tests/cli.rs:26862-26900` `watch_once_never_names_the_unattributed_pid_sentinel_when_the_url_is_unparseable`

#### `dup-0507` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:20240-20273` `watch_once_reports_a_dead_dash_when_only_the_url_breadcrumb_is_recorded_and_no_marker_exists`
- `tests/cli.rs:26922-26951` `watch_once_parses_the_urls_port_past_a_colon_in_the_path_with_no_marker`

#### `dup-0508` (near, 5 sites)

Proposed home: `cli::support (consolidate these 5 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:20353-20395` `watch_once_reports_no_dash_anomaly_for_a_done_run_even_with_a_dead_marker`
- `tests/cli.rs:20412-20472` `watch_once_reports_no_dash_anomaly_for_a_fresh_run_that_inherits_an_earlier_runs_dead_marker`
- `tests/cli.rs:20512-20561` `watch_once_reports_this_runs_own_dead_marker_when_written_after_its_run_started`
- `tests/cli.rs:20583-20635` `watch_once_reports_a_dead_marker_predating_run_started_when_dash_attempt_names_this_run`
- `tests/cli.rs:20664-20716` `watch_once_suppresses_a_predating_marker_when_dash_attempt_names_a_different_run`

#### `dup-0509` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:20798-20814` `stage_rigger_shim`
- `tests/cli.rs:21219-21239` `stage_failing_docs_rigger_shim`
- `tests/cli.rs:21267-21295` `stage_stale_rigger_shim`

#### `dup-0510` (near, 6 sites)

Proposed home: `cli::support (consolidate these 6 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:20982-21024` `setup_precommit_hook_passes_untouched_when_the_render_matches`
- `tests/cli.rs:21039-21110` `setup_precommit_hook_never_drift_checks_or_stages_a_registry_entry_outside_its_scope`
- `tests/cli.rs:21307-21357` `setup_precommit_hook_prefers_the_trees_own_built_binary_over_a_stale_path_rigger`
- `tests/cli.rs:21367-21417` `setup_precommit_hook_refuses_the_same_commit_shape_with_only_a_stale_path_rigger`
- `tests/cli.rs:21744-21775` `setup_precommit_hook_warns_and_proceeds_when_rigger_is_unavailable`
- `tests/cli.rs:21782-21812` `setup_precommit_hook_warns_and_proceeds_when_rigger_docs_errors`

#### `dup-0511` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:21246-21257` `stage_tree_built_binary`
- `tests/cli.rs:21424-21439` `stage_unit_derived_binary`

#### `dup-0512` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:22922-22967` `step_self_heals_a_stale_marker_naming_a_dead_pid`
- `tests/cli.rs:22992-23038` `step_self_heals_a_stale_marker_naming_a_live_pid_whose_port_is_unserved`

#### `dup-0513` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:23642-23653` `write_live_instance`
- `tests/cli.rs:23659-23670` `write_stale_instance`

#### `dup-0514` (near, 7 sites)

Proposed home: `cli::support (consolidate these 7 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:23683-23771` `a_reap_on_idle_singleton_serves_while_an_instance_heartbeats_then_reaps_when_the_registry_empties`
- `tests/cli.rs:23887-23971` `a_reap_on_idle_singleton_does_not_reap_before_any_instance_has_registered`
- `tests/cli.rs:23998-24102` `a_reap_on_idle_singleton_survives_a_fresh_agent_liveness_marker_after_the_registry_ages_out`
- `tests/cli.rs:24120-24225` `a_reap_on_idle_singleton_survives_a_fresh_agent_liveness_marker_with_no_git_repo_at_launch`
- `tests/cli.rs:24240-24354` `a_reap_on_idle_singleton_survives_a_second_registered_projects_fresh_agent_liveness_marker`
- `tests/cli.rs:24377-24500` `a_reap_on_idle_singleton_survives_a_foreign_agent_liveness_marker_whose_own_registry_entry_was_already_stale_before_the_watchers_first_poll`
- `tests/cli.rs:24516-24658` `a_landing_poll_racing_the_watchers_first_tick_does_not_erase_a_foreign_projects_only_route_into_known_roots`

#### `dup-0515` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:24665-24720` `a_dash_without_reap_on_idle_never_self_reaps_on_a_quiet_machine`
- `tests/cli.rs:24735-24799` `a_reap_on_idle_singleton_in_a_homeless_environment_serves_without_a_watcher`

#### `dup-0516` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:26514-26556` `watch_once_never_names_a_mismatched_markers_pid_for_the_recorded_urls_port`
- `tests/cli.rs:26980-27029` `watch_once_never_names_a_mismatched_markers_pid_when_the_urls_path_contains_a_colon`

#### `dup-0517` (near, 4 sites)

Proposed home: `cli::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:27168-27206` `run_given_a_spec_path_names_the_spec_lint_as_a_next_step`
- `tests/cli.rs:27331-27374` `step_given_a_spec_path_names_the_spec_lint_even_when_the_step_then_refuses_for_no_reachable_base`
- `tests/cli.rs:27648-27683` `step_reminder_prints_despite_env_naming_a_foreign_pid`
- `tests/cli.rs:27727-27761` `run_reminder_prints_despite_env_naming_a_foreign_pid`

#### `dup-0518` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:27394-27454` `run_surfaces_the_same_spec_lint_advisory_as_validate_the_in_run_call_site`
- `tests/cli.rs:27463-27502` `step_surfaces_the_same_spec_lint_advisory_as_validate_the_in_run_call_site`
- `tests/cli.rs:27520-27587` `run_driver_workflow_surfaces_the_same_spec_lint_advisory_as_validate_the_in_run_call_site`

#### `dup-0519` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:27607-27643` `step_reminder_is_suppressed_when_env_names_the_real_direct_parent_pid`
- `tests/cli.rs:27687-27722` `run_reminder_is_suppressed_when_env_names_the_real_direct_parent_pid`

#### `dup-0520` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:27825-27874` `run_driver_workflow_prints_the_spec_lint_reminder_and_honors_the_pid_scoped_dedup`
- `tests/cli.rs:27886-27941` `run_driver_workflow_reminder_never_reaches_stdout_in_any_pid_sentinel_direction`

#### `dup-0521` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:27957-27991` `run_driver_workflow_fresh_notice_never_reaches_stdout`
- `tests/cli.rs:27998-28022` `run_driver_cli_fresh_notice_still_prints_on_stdout`

#### `dup-0522` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:28114-28214` `cmd_dash_gives_the_stopped_listener_diagnosis_naming_resume_or_kill`
- `tests/cli.rs:28293-28408` `step_names_the_stopped_holder_when_the_step_paths_own_auto_start_hits_the_predecessor_scenario`

#### `dup-0523` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:28759-28866` `mcp_serves_peers_ground_and_graph_over_stdio`
- `tests/cli.rs:29673-29778` `mcp_survives_a_grounder_resolution_failure_and_still_serves_peers_and_graph`

#### `dup-0524` (exact, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:29121-29135` `grep_guard_never_bounces_a_substring_grep_command_end_to_end`
- `tests/cli.rs:29441-29455` `grep_guard_never_bounces_a_path_qualified_non_grep_command_end_to_end`

#### `dup-0525` (near, 4 sites)

Proposed home: `cli::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:29201-29230` `grep_guard_denies_an_ancestor_target_end_to_end_and_passes_literal`
- `tests/cli.rs:29273-29287` `grep_guard_still_allows_literal_on_a_shell_metacharacter_fused_grep_end_to_end`
- `tests/cli.rs:29329-29343` `grep_guard_still_allows_a_quoted_literal_on_a_quoted_grep_end_to_end`
- `tests/cli.rs:29543-29569` `grep_guard_bounces_an_output_redirect_metacharacter_fused_grep_end_to_end`

#### `dup-0526` (near, 4 sites)

Proposed home: `cli::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:29239-29264` `grep_guard_bounces_a_shell_metacharacter_fused_grep_end_to_end`
- `tests/cli.rs:29298-29321` `grep_guard_bounces_a_quoted_or_escaped_grep_end_to_end`
- `tests/cli.rs:29390-29412` `grep_guard_bounces_a_path_qualified_grep_end_to_end`
- `tests/cli.rs:29418-29436` `grep_guard_still_allows_literal_on_a_path_qualified_grep_end_to_end`

#### `dup-0527` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:29350-29366` `grep_guard_bounces_a_grep_split_by_a_line_continuation_end_to_end`
- `tests/cli.rs:29372-29384` `grep_guard_still_allows_literal_on_a_line_continuation_split_grep_end_to_end`

#### `dup-0528` (near, 5 sites)

Proposed home: `a new shared module (sites span 2 files: tests/code_entity_test_exclusion_periphery.rs, tests/symbol_ref_caller_attribution.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_entity_test_exclusion_periphery.rs:98-137` `is_test_false_serializes_byte_identically_to_the_pre86_form`
- `tests/code_entity_test_exclusion_periphery.rs:264-292` `is_out_of_line_module_false_serializes_byte_identically_to_the_pre_round6_form`
- `tests/code_entity_test_exclusion_periphery.rs:381-409` `path_override_none_serializes_byte_identically_to_the_pre_round7_form`
- `tests/code_entity_test_exclusion_periphery.rs:504-532` `enclosing_inline_module_path_none_serializes_byte_identically_to_the_pre_round9_form`
- `tests/symbol_ref_caller_attribution.rs:32-78` `a_caller_less_reference_serializes_byte_identically_to_the_pre37_form`

#### `dup-0529` (near, 4 sites)

Proposed home: `code_entity_test_exclusion_periphery::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_entity_test_exclusion_periphery.rs:140-195` `is_test_true_serializes_the_key_and_round_trips`
- `tests/code_entity_test_exclusion_periphery.rs:295-333` `is_out_of_line_module_true_serializes_the_key_and_round_trips`
- `tests/code_entity_test_exclusion_periphery.rs:412-451` `path_override_some_serializes_the_key_and_round_trips`
- `tests/code_entity_test_exclusion_periphery.rs:535-574` `enclosing_inline_module_path_some_serializes_the_key_and_round_trips`

#### `dup-0530` (near, 5 sites)

Proposed home: `a new shared module (sites span 2 files: tests/code_entity_test_exclusion_periphery.rs, tests/symbol_ref_caller_attribution.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_entity_test_exclusion_periphery.rs:198-250` `a_pre86_persisted_index_with_no_is_test_key_loads_defaulting_every_item_to_false`
- `tests/code_entity_test_exclusion_periphery.rs:336-372` `a_pre_round6_persisted_index_with_no_is_out_of_line_module_key_loads_defaulting_to_false`
- `tests/code_entity_test_exclusion_periphery.rs:454-492` `a_pre_round7_persisted_index_with_no_path_override_key_loads_defaulting_to_none`
- `tests/code_entity_test_exclusion_periphery.rs:577-620` `a_pre_round9_persisted_index_with_no_enclosing_inline_module_path_key_loads_defaulting_to_none`
- `tests/symbol_ref_caller_attribution.rs:132-182` `a_pre37_persisted_index_loads_folding_references_caller_less`

#### `dup-0531` (near, 4 sites)

Proposed home: `code_entity_test_exclusion_periphery::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_entity_test_exclusion_periphery.rs:760-807` `cfg_not_test_and_cfg_attr_predicates_graph_as_product_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:816-848` `a_not_wrapping_a_non_test_atom_graphs_as_product_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:1032-1067` `a_url_bearing_attribute_between_test_and_the_item_does_not_leak_the_item_into_the_graph`
- `tests/code_entity_test_exclusion_periphery.rs:1148-1181` `a_comment_mentioning_test_attribute_text_does_not_exclude_the_item_through_the_public_api`

#### `dup-0532` (near, 26 sites)

Proposed home: `code_entity_test_exclusion_periphery::support (consolidate these 26 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_entity_test_exclusion_periphery.rs:965-1006` `a_trailing_comment_on_cfg_test_still_excludes_the_module_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:1092-1129` `a_multiline_cfg_test_attribute_still_excludes_the_module_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:1219-1262` `a_trailing_comma_in_a_wrapped_cfg_predicate_still_excludes_the_module_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:1290-1333` `an_inner_cfg_test_attribute_excludes_its_module_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:1375-1417` `an_out_of_line_cfg_test_module_declaration_excludes_its_declared_file_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:1436-1488` `an_out_of_line_cfg_test_module_declaration_in_a_subdirectory_excludes_its_sibling_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:1661-1724` `a_non_test_out_of_line_mod_and_an_inline_test_mod_never_exclude_a_coincidentally_named_sibling_file`
- `tests/code_entity_test_exclusion_periphery.rs:1862-1916` `a_cfg_test_impl_block_excludes_its_methods_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:1950-1985` `a_cfg_test_impl_block_using_the_inner_attribute_form_excludes_its_methods_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:2035-2080` `cfg_test_on_every_other_item_kind_excludes_or_stays_scoped_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:2100-2170` `an_out_of_line_test_mod_declared_inside_a_non_directory_style_file_resolves_correctly`
- `tests/code_entity_test_exclusion_periphery.rs:2181-2246` `an_out_of_line_test_mod_declaration_falls_back_to_a_nested_mod_rs_when_no_flat_sibling_exists`
- `tests/code_entity_test_exclusion_periphery.rs:2258-2329` `a_path_attribute_override_redirects_out_of_line_resolution_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:2342-2418` `an_out_of_line_test_module_files_own_out_of_line_declarations_are_excluded_recursively_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:2486-2555` `an_out_of_line_test_mod_declaration_falls_back_to_a_root_level_nested_mod_rs_when_module_dir_is_empty`
- `tests/code_entity_test_exclusion_periphery.rs:2568-2638` `a_path_attribute_override_resolves_relative_to_a_declaring_files_own_subdirectory`
- `tests/code_entity_test_exclusion_periphery.rs:2666-2720` `a_path_attribute_override_that_walks_upward_with_dotdot_still_excludes_its_target`
- `tests/code_entity_test_exclusion_periphery.rs:2735-2784` `a_path_attribute_override_with_an_explicit_dot_slash_prefix_still_resolves_to_the_same_directory_target`
- `tests/code_entity_test_exclusion_periphery.rs:2803-2855` `a_path_attribute_override_with_chained_dotdot_walks_up_every_popped_level`
- `tests/code_entity_test_exclusion_periphery.rs:2880-2928` `a_path_attribute_override_whose_dotdot_count_overflows_the_declaring_directorys_depth_does_not_silently_collide_with_an_unrelated_real_file`
- `tests/code_entity_test_exclusion_periphery.rs:2954-3032` `a_path_attribute_override_nested_inside_an_inline_module_resolves_under_the_declaring_files_own_module_directory_plus_the_inline_chain`
- `tests/code_entity_test_exclusion_periphery.rs:3053-3152` `a_path_attribute_override_nested_inside_chained_inline_modules_resolves_under_every_enclosing_modules_directory_in_outermost_first_order`
- `tests/code_entity_test_exclusion_periphery.rs:3170-3217` `a_path_attribute_override_that_is_an_absolute_path_does_not_silently_collide_with_an_unrelated_real_file`
- `tests/code_entity_test_exclusion_periphery.rs:3240-3291` `a_path_attribute_override_naming_a_uri_scheme_does_not_silently_collide_with_an_unrelated_real_file`
- `tests/code_entity_test_exclusion_periphery.rs:3309-3357` `a_path_attribute_override_naming_a_windows_drive_letter_does_not_silently_collide_with_an_unrelated_real_file`
- `tests/code_entity_test_exclusion_periphery.rs:3392-3451` `a_path_attribute_override_on_a_mod_declared_inside_a_function_body_is_not_treated_as_nested_in_a_module`

#### `dup-0533` (exact, 6 sites)

Proposed home: `a new shared module (sites span 6 files: tests/code_ingest_events.rs, tests/community_detection_pass.rs, tests/community_fold_periphery.rs, tests/community_resolution_knob.rs, tests/concepts_fold_periphery.rs, tests/design_intent_events.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_ingest_events.rs:30-34` `apply_json`
- `tests/community_detection_pass.rs:31-35` `apply_json`
- `tests/community_fold_periphery.rs:39-43` `apply_json`
- `tests/community_resolution_knob.rs:62-66` `apply_json`
- `tests/concepts_fold_periphery.rs:42-46` `apply_json`
- `tests/design_intent_events.rs:38-42` `apply_json`

#### `dup-0534` (near, 2 sites)

Proposed home: `code_ingest_events::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_ingest_events.rs:241-304` `real_extraction_tiers_every_structural_edge_through_the_emit_fold_pipeline`
- `tests/code_ingest_events.rs:308-378` `real_extraction_folds_caller_attributed_calls_edges_at_every_tier`

#### `dup-0535` (near, 2 sites)

Proposed home: `code_ingest_events::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_ingest_events.rs:382-451` `re_extracting_a_file_that_drops_a_call_supersedes_its_calls_edge_end_to_end`
- `tests/code_ingest_events.rs:711-793` `re_extracting_a_changed_file_supersedes_its_removed_symbols_end_to_end`

#### `dup-0536` (near, 9 sites)

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

#### `dup-0537` (near, 2 sites)

Proposed home: `code_ingest_events::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_ingest_events.rs:1129-1170` `a_refs_only_files_re_extraction_supersedes_via_the_reference_batch_boundary`
- `tests/code_ingest_events.rs:1173-1227` `a_re_extraction_supersedes_only_its_own_files_edges_not_another_files_reference`

#### `dup-0538` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/code_lens_overview_collapse_viz.rs, tests/subject_lens_overlay_served_page.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_lens_overview_collapse_viz.rs:74-88` `build_harness`
- `tests/subject_lens_overlay_served_page.rs:550-564` `build_additive_harness`

#### `dup-0539` (semantic, 5 sites)

Proposed home: `one shared `build_harness` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 5 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/code_lens_overview_collapse_viz.rs:74-88` `build_harness`
- `tests/metadata_card_handoff_viz.rs:123-138` `build_harness`
- `tests/proof_row_renders_on_the_card.rs:76-91` `build_harness`
- `tests/subject_lens_overlay_client_arms.rs:82-97` `build_harness`
- `tests/subject_view_memory_rail_client.rs:100-115` `build_harness`

#### `dup-0540` (exact, 6 sites)

Proposed home: `a new shared module (sites span 6 files: tests/code_lens_overview_collapse_viz.rs, tests/metadata_card_handoff_viz.rs, tests/proof_row_renders_on_the_card.rs, tests/subject_lens_overlay_client_arms.rs, tests/subject_lens_overlay_served_page.rs, tests/subject_view_memory_rail_client.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_lens_overview_collapse_viz.rs:92-118` `run_node_harness`
- `tests/metadata_card_handoff_viz.rs:141-168` `run_node_harness`
- `tests/proof_row_renders_on_the_card.rs:94-121` `run_node_harness`
- `tests/subject_lens_overlay_client_arms.rs:101-127` `run_node_harness`
- `tests/subject_lens_overlay_served_page.rs:59-85` `run_node_harness`
- `tests/subject_view_memory_rail_client.rs:119-145` `run_node_harness`

#### `dup-0541` (semantic, 6 sites)

Proposed home: `one shared `run_node_harness` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 6 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/code_lens_overview_collapse_viz.rs:92-118` `run_node_harness`
- `tests/metadata_card_handoff_viz.rs:141-168` `run_node_harness`
- `tests/proof_row_renders_on_the_card.rs:94-121` `run_node_harness`
- `tests/subject_lens_overlay_client_arms.rs:101-127` `run_node_harness`
- `tests/subject_lens_overlay_served_page.rs:59-85` `run_node_harness`
- `tests/subject_view_memory_rail_client.rs:119-145` `run_node_harness`

#### `dup-0542` (exact, 6 sites)

Proposed home: `a new shared module (sites span 4 files: tests/code_lens_overview_collapse_viz.rs, tests/metadata_card_handoff_viz.rs, tests/proof_row_renders_on_the_card.rs, tests/subject_lens_overlay_served_page.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_lens_overview_collapse_viz.rs:200-213` `the_overview_collapses_to_sized_labelled_community_super_nodes_purely`
- `tests/metadata_card_handoff_viz.rs:297-310` `metadata_card_renders_every_taxonomy_and_chips_hand_off_to_their_own_lens`
- `tests/metadata_card_handoff_viz.rs:383-396` `metadata_card_wiring_fires_at_every_render_and_drill_call_site`
- `tests/proof_row_renders_on_the_card.rs:181-194` `proof_row_renders_count_evidence_and_the_explicit_empty_state`
- `tests/subject_lens_overlay_served_page.rs:777-790` `a_neighborhood_rationale_badge_click_expands_and_does_not_reseed`
- `tests/subject_lens_overlay_served_page.rs:797-810` `the_drill_view_is_byte_identical_with_the_overlay_off`

#### `dup-0543` (near, 8 sites)

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

#### `dup-0544` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/code_lens_view_periphery.rs, tests/concepts_lens_view_periphery.rs, tests/files_lens_view_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_lens_view_periphery.rs:83-89` `plain`
- `tests/concepts_lens_view_periphery.rs:108-114` `plain`
- `tests/files_lens_view_periphery.rs:91-97` `plain`

#### `dup-0545` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/code_lens_view_periphery.rs, tests/concepts_lens_view_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_lens_view_periphery.rs:120-146` `lens_graph`
- `tests/concepts_lens_view_periphery.rs:140-167` `lens_graph`

#### `dup-0546` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/code_lens_view_periphery.rs, tests/concepts_lens_view_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_lens_view_periphery.rs:149-153` `code_default`
- `tests/concepts_lens_view_periphery.rs:170-174` `concepts_default`

#### `dup-0547` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/code_lens_view_periphery.rs, tests/concepts_lens_view_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_lens_view_periphery.rs:163-196` `lens_from_query_is_a_public_total_selector_that_falls_back_to_files`
- `tests/concepts_lens_view_periphery.rs:185-213` `lens_from_query_is_a_public_total_selector_including_concepts`

#### `dup-0548` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/code_lens_view_periphery.rs, tests/concepts_lens_view_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_lens_view_periphery.rs:209-258` `code_lens_overview_buckets_code_entities_by_community_and_excludes_every_other_kind`
- `tests/concepts_lens_view_periphery.rs:226-279` `concepts_lens_overview_buckets_members_by_concept_across_directories_and_excludes_membershipless_nodes`

#### `dup-0549` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/code_lens_view_periphery.rs, tests/concepts_lens_view_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_lens_view_periphery.rs:416-437` `code_lens_at_an_underived_grain_carries_the_documented_empty_state_not_an_error`
- `tests/concepts_lens_view_periphery.rs:463-484` `concepts_lens_at_an_underived_grain_carries_the_documented_empty_state_not_an_error`

#### `dup-0550` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/code_lens_view_periphery.rs, tests/concepts_lens_view_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_lens_view_periphery.rs:558-578` `served`
- `tests/concepts_lens_view_periphery.rs:538-558` `served`

#### `dup-0551` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/code_lens_view_periphery.rs, tests/concepts_lens_view_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_lens_view_periphery.rs:581-585` `served_json`
- `tests/concepts_lens_view_periphery.rs:561-565` `served_json`

#### `dup-0552` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/community_detection_cli.rs, tests/concepts_derivation_cli.rs, tests/projections_stay_local.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_detection_cli.rs:55-67` `project`
- `tests/concepts_derivation_cli.rs:60-72` `project`
- `tests/projections_stay_local.rs:195-206` `server_project`

#### `dup-0553` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_detection_cli.rs, tests/concepts_derivation_cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_detection_cli.rs:70-75` `rigger_db`
- `tests/concepts_derivation_cli.rs:75-80` `rigger_db`

#### `dup-0554` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_detection_cli.rs, tests/concepts_derivation_cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_detection_cli.rs:78-87` `communities`
- `tests/concepts_derivation_cli.rs:83-92` `concepts`

#### `dup-0555` (near, 4 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_detection_cli.rs, tests/concepts_derivation_cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_detection_cli.rs:92-100` `def`
- `tests/community_detection_cli.rs:105-113` `call`
- `tests/concepts_derivation_cli.rs:96-104` `doc`
- `tests/concepts_derivation_cli.rs:109-114` `link`

#### `dup-0556` (semantic, 2 sites)

Proposed home: `one shared `seed_coupling` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/community_detection_cli.rs:122-159` `seed_coupling`
- `tests/community_fold_periphery.rs:100-110` `seed_coupling`

#### `dup-0557` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_detection_cli.rs, tests/concepts_derivation_cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_detection_cli.rs:164-183` `community_layer`
- `tests/concepts_derivation_cli.rs:195-214` `concept_layer`

#### `dup-0558` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_detection_cli.rs, tests/concepts_derivation_cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_detection_cli.rs:186-191` `member_of`
- `tests/concepts_derivation_cli.rs:217-222` `member_of`

#### `dup-0559` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_detection_cli.rs, tests/concepts_derivation_cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_detection_cli.rs:265-299` `an_empty_project_records_no_community_and_still_succeeds`
- `tests/concepts_derivation_cli.rs:317-351` `an_empty_project_records_no_concept_and_still_succeeds`

#### `dup-0560` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_detection_cli.rs, tests/concepts_derivation_cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_detection_cli.rs:302-372` `resolution_grains_coexist_and_a_rerun_supersedes_only_its_own_grain`
- `tests/concepts_derivation_cli.rs:354-424` `resolution_grains_coexist_and_a_rerun_supersedes_only_its_own_grain`

#### `dup-0561` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_detection_cli.rs, tests/concepts_derivation_cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_detection_cli.rs:375-411` `a_malformed_resolution_or_unknown_argument_fails_loudly`
- `tests/concepts_derivation_cli.rs:427-463` `a_malformed_resolution_or_unknown_argument_fails_loudly`

#### `dup-0562` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_detection_cli.rs, tests/concepts_derivation_cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_detection_cli.rs:414-457` `re_running_a_grain_reproduces_the_byte_identical_live_layer`
- `tests/concepts_derivation_cli.rs:466-508` `re_running_a_grain_reproduces_the_byte_identical_live_layer`

#### `dup-0563` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_detection_pass.rs, tests/community_resolution_knob.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_detection_pass.rs:67-95` `seed`
- `tests/community_resolution_knob.rs:96-123` `seed`

#### `dup-0564` (semantic, 2 sites)

Proposed home: `one shared `community_snapshot` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/community_detection_pass.rs:100-115` `community_snapshot`
- `tests/community_fold_periphery.rs:80-95` `community_snapshot`

#### `dup-0565` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_fold_periphery.rs, tests/concepts_fold_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_fold_periphery.rs:69-75` `live_memberships`
- `tests/concepts_fold_periphery.rs:71-77` `live_realizes`

#### `dup-0566` (semantic, 2 sites)

Proposed home: `one shared `live_memberships` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/community_fold_periphery.rs:69-75` `live_memberships`
- `tests/community_resolution_knob.rs:180-189` `live_memberships`

#### `dup-0567` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_fold_periphery.rs, tests/concepts_fold_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_fold_periphery.rs:230-263` `a_community_assigned_event_without_a_fresh_key_is_a_non_boundary_and_never_supersedes`
- `tests/concepts_fold_periphery.rs:218-262` `a_concept_derived_event_without_a_fresh_key_is_a_non_boundary_and_never_supersedes`

#### `dup-0568` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_fold_periphery.rs, tests/concepts_fold_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_fold_periphery.rs:266-300` `the_community_layer_serialized_literals_are_stable_and_the_fold_matches_the_on_log_type`
- `tests/concepts_fold_periphery.rs:265-306` `the_concept_layer_serialized_literals_are_stable_and_the_fold_matches_the_on_log_type`

#### `dup-0569` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_resolution_knob.rs, tests/graph_superseded_prune.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_resolution_knob.rs:214-223` `live_memberships_of`
- `tests/graph_superseded_prune.rs:59-68` `live_contains`

#### `dup-0570` (near, 2 sites)

Proposed home: `concepts_labels_membership::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/concepts_labels_membership.rs:136-178` `label_is_the_most_central_document_by_intent_degree`
- `tests/concepts_labels_membership.rs:181-212` `label_ties_break_to_the_lexicographically_smallest_document`

#### `dup-0571` (near, 2 sites)

Proposed home: `concepts_labels_membership::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/concepts_labels_membership.rs:262-289` `a_documentless_concept_falls_back_to_its_most_central_members_name`
- `tests/concepts_labels_membership.rs:292-317` `a_documentless_concept_with_no_named_member_falls_back_to_the_most_central_members_id`

#### `dup-0572` (near, 13 sites)

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

#### `dup-0573` (near, 4 sites)

Proposed home: `a new shared module (sites span 4 files: tests/confidence_tier_blast_radius.rs, tests/criteria_delivery_periphery.rs, tests/gate_store_fence_periphery.rs, tests/run_scoping_survives_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/confidence_tier_blast_radius.rs:52-63` `spawn`
- `tests/criteria_delivery_periphery.rs:43-54` `spawn`
- `tests/gate_store_fence_periphery.rs:802-813` `spawn`
- `tests/run_scoping_survives_periphery.rs:64-75` `spawn`

#### `dup-0574` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/confidence_tier_blast_radius.rs, tests/unified_traversal_grounding.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/confidence_tier_blast_radius.rs:110-115` `fold`
- `tests/unified_traversal_grounding.rs:182-187` `fold`

#### `dup-0575` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/courier_registry_refresh_boundary_periphery.rs, tests/registry_refresh_driver_courier_convergence_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/courier_registry_refresh_boundary_periphery.rs:37-64` `courier_project_with_commit`
- `tests/registry_refresh_driver_courier_convergence_periphery.rs:38-82` `driver_project`

#### `dup-0576` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/courier_registry_refresh_boundary_periphery.rs, tests/courier_registry_refresh_periphery.rs, tests/registry_refresh_driver_courier_convergence_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/courier_registry_refresh_boundary_periphery.rs:68-77` `run_rigger`
- `tests/courier_registry_refresh_periphery.rs:58-67` `run_rigger`
- `tests/registry_refresh_driver_courier_convergence_periphery.rs:97-107` `run_rigger`

#### `dup-0577` (exact, 4 sites)

Proposed home: `a new shared module (sites span 4 files: tests/courier_registry_refresh_boundary_periphery.rs, tests/courier_registry_refresh_fence_periphery.rs, tests/courier_registry_refresh_periphery.rs, tests/registry_refresh_driver_courier_convergence_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/courier_registry_refresh_boundary_periphery.rs:79-86` `assert_ok`
- `tests/courier_registry_refresh_fence_periphery.rs:73-80` `assert_ok`
- `tests/courier_registry_refresh_periphery.rs:69-76` `assert_ok`
- `tests/registry_refresh_driver_courier_convergence_periphery.rs:109-116` `assert_ok`

#### `dup-0578` (near, 5 sites)

Proposed home: `a new shared module (sites span 5 files: tests/courier_registry_refresh_boundary_periphery.rs, tests/courier_registry_refresh_fence_periphery.rs, tests/courier_registry_refresh_periphery.rs, tests/gate_store_fence_periphery.rs, tests/registry_refresh_driver_courier_convergence_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/courier_registry_refresh_boundary_periphery.rs:91-109` `registry_entries`
- `tests/courier_registry_refresh_fence_periphery.rs:53-71` `registry_entries`
- `tests/courier_registry_refresh_periphery.rs:81-99` `registry_entries`
- `tests/gate_store_fence_periphery.rs:253-271` `registry_entries`
- `tests/registry_refresh_driver_courier_convergence_periphery.rs:122-140` `registry_entries`

#### `dup-0579` (semantic, 5 sites)

Proposed home: `one shared `registry_entries` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 5 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/courier_registry_refresh_boundary_periphery.rs:91-109` `registry_entries`
- `tests/courier_registry_refresh_fence_periphery.rs:53-71` `registry_entries`
- `tests/courier_registry_refresh_periphery.rs:81-99` `registry_entries`
- `tests/gate_store_fence_periphery.rs:253-271` `registry_entries`
- `tests/registry_refresh_driver_courier_convergence_periphery.rs:122-140` `registry_entries`

#### `dup-0580` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/courier_registry_refresh_boundary_periphery.rs, tests/courier_registry_refresh_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/courier_registry_refresh_boundary_periphery.rs:263-283` `an_ambient_kurrentdb_conn_never_leaks_into_a_boundary_courier`
- `tests/courier_registry_refresh_periphery.rs:308-333` `an_ambient_kurrentdb_conn_never_leaks_into_a_courier_spawned_through_the_shared_helper`

#### `dup-0581` (semantic, 2 sites)

Proposed home: `one shared `courier_project` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/courier_registry_refresh_fence_periphery.rs:37-48` `courier_project`
- `tests/courier_registry_refresh_periphery.rs:35-52` `courier_project`

#### `dup-0582` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/courier_registry_refresh_periphery.rs, tests/store_content_identity_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/courier_registry_refresh_periphery.rs:35-52` `courier_project`
- `tests/store_content_identity_periphery.rs:1339-1358` `cli_project`

#### `dup-0583` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/dash_cluster_detail_drill.rs, tests/dash_exploration_route_client_contract.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dash_cluster_detail_drill.rs:107-109` `spoke_id`
- `tests/dash_exploration_route_client_contract.rs:103-105` `spoke`

#### `dup-0584` (near, 6 sites)

Proposed home: `a new shared module (sites span 6 files: tests/dash_decisions_progressive_disclosure.rs, tests/dash_kg_graph_route.rs, tests/dash_whole_projection_reach.rs, tests/proof_lands_on_the_card_periphery.rs, tests/rationale_overlay_data.rs, tests/rationale_overlay_seam.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dash_decisions_progressive_disclosure.rs:45-105` `try_fetch_served_root_page`
- `tests/dash_kg_graph_route.rs:110-164` `try_fetch_served`
- `tests/dash_whole_projection_reach.rs:254-314` `try_fetch_whole_served`
- `tests/proof_lands_on_the_card_periphery.rs:773-831` `try_fetch_served`
- `tests/rationale_overlay_data.rs:103-154` `try_fetch_served`
- `tests/rationale_overlay_seam.rs:65-117` `try_fetch_served`

#### `dup-0585` (semantic, 2 sites)

Proposed home: `one shared `exploration_graph` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/dash_exploration_route_client_contract.rs:116-146` `exploration_graph`
- `tests/dash_kg_graph_route.rs:1415-1463` `exploration_graph`

#### `dup-0586` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/dash_exploration_route_client_contract.rs, tests/subject_lens_reprojection_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dash_exploration_route_client_contract.rs:151-172` `served_body`
- `tests/subject_lens_reprojection_periphery.rs:330-351` `served_json`

#### `dup-0587` (near, 2 sites)

Proposed home: `dash_graph_exploration_fold::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dash_graph_exploration_fold.rs:37-62` `cluster_key_is_reachable_over_the_public_crate_boundary`
- `tests/dash_graph_exploration_fold.rs:69-146` `cluster_key_honors_the_boundary_edges_of_the_names_a_file_predicate`

#### `dup-0588` (semantic, 2 sites)

Proposed home: `one shared `fixture_graph` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/dash_kg_graph_route.rs:39-69` `fixture_graph`
- `tests/metadata_card_periphery.rs:69-105` `fixture_graph`

#### `dup-0589` (semantic, 4 sites)

Proposed home: `one shared `try_fetch_served` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 4 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/dash_kg_graph_route.rs:110-164` `try_fetch_served`
- `tests/proof_lands_on_the_card_periphery.rs:773-831` `try_fetch_served`
- `tests/rationale_overlay_data.rs:103-154` `try_fetch_served`
- `tests/rationale_overlay_seam.rs:65-117` `try_fetch_served`

#### `dup-0590` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/dash_kg_graph_route.rs, tests/rationale_overlay_data.rs, tests/rationale_overlay_seam.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dash_kg_graph_route.rs:170-179` `fetch_served`
- `tests/rationale_overlay_data.rs:158-167` `fetch_served`
- `tests/rationale_overlay_seam.rs:121-130` `fetch_served`

#### `dup-0591` (semantic, 4 sites)

Proposed home: `one shared `fetch_served` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 4 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/dash_kg_graph_route.rs:170-179` `fetch_served`
- `tests/proof_lands_on_the_card_periphery.rs:835-844` `fetch_served`
- `tests/rationale_overlay_data.rs:158-167` `fetch_served`
- `tests/rationale_overlay_seam.rs:121-130` `fetch_served`

#### `dup-0592` (exact, 5 sites)

Proposed home: `a new shared module (sites span 5 files: tests/dash_kg_graph_route.rs, tests/dash_whole_projection_reach.rs, tests/proof_lands_on_the_card_periphery.rs, tests/rationale_overlay_data.rs, tests/rationale_overlay_seam.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dash_kg_graph_route.rs:182-186` `body_of`
- `tests/dash_whole_projection_reach.rs:329-333` `body_of`
- `tests/proof_lands_on_the_card_periphery.rs:848-852` `body_of`
- `tests/rationale_overlay_data.rs:170-174` `body_of`
- `tests/rationale_overlay_seam.rs:135-139` `body_of`

#### `dup-0593` (near, 2 sites)

Proposed home: `dash_kg_graph_route::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dash_kg_graph_route.rs:264-321` `the_served_root_page_ships_the_kg_panel_and_select_to_seed_wiring`
- `tests/dash_kg_graph_route.rs:740-784` `the_served_root_page_renders_god_nodes_and_the_query_path`

#### `dup-0594` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/dash_release_ready.rs, tests/dash_run_tree_spine.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dash_release_ready.rs:104-112` `connect_with_retry`
- `tests/dash_run_tree_spine.rs:282-290` `connect_with_retry`

#### `dup-0595` (semantic, 2 sites)

Proposed home: `one shared `connect_with_retry` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/dash_release_ready.rs:104-112` `connect_with_retry`
- `tests/dash_run_tree_spine.rs:282-290` `connect_with_retry`

#### `dup-0596` (near, 4 sites)

Proposed home: `dash_run_tree_spine::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dash_run_tree_spine.rs:541-581` `an_escalated_unit_renders_gates_failed_and_surfaces_at_the_spec_root`
- `tests/dash_run_tree_spine.rs:591-631` `an_escalated_units_gate_failure_is_not_masked_by_a_trailing_passing_gate`
- `tests/dash_run_tree_spine.rs:710-761` `a_regated_green_unit_renders_gates_passed_despite_an_earlier_failed_attempt`
- `tests/dash_run_tree_spine.rs:775-812` `a_review_rejected_unit_whose_gates_passed_renders_gates_passed_and_surfaces_the_reject`

#### `dup-0597` (near, 2 sites)

Proposed home: `dash_run_tree_spine::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dash_run_tree_spine.rs:820-850` `a_pre_gate_unit_whose_implementer_finished_does_not_render_gates_failed`
- `tests/dash_run_tree_spine.rs:861-898` `a_gates_cleared_unit_with_no_recorded_verdict_still_renders_gates_passed`

#### `dup-0598` (exact, 7 sites)

Proposed home: `a new shared module (sites span 7 files: tests/dead_code_json_contract_periphery.rs, tests/duplication_catalog_contract_periphery.rs, tests/gitsemver_path_inclusion_accounting_periphery.rs, tests/handbook_grounder_accuracy.rs, tests/prioritized_plan_citation_periphery.rs, tests/responsibility_map_contract_periphery.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dead_code_json_contract_periphery.rs:228-230` `repo_root`
- `tests/duplication_catalog_contract_periphery.rs:100-102` `repo_root`
- `tests/gitsemver_path_inclusion_accounting_periphery.rs:52-54` `repo_root`
- `tests/handbook_grounder_accuracy.rs:33-35` `repo_root`
- `tests/prioritized_plan_citation_periphery.rs:65-67` `repo_root`
- `tests/responsibility_map_contract_periphery.rs:76-78` `repo_root`
- `tests/simplification_audit.rs:2010-2012` `repo_root`

#### `dup-0599` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/dead_code_json_contract_periphery.rs, tests/duplication_catalog_contract_periphery.rs, tests/responsibility_map_contract_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dead_code_json_contract_periphery.rs:232-236` `read_committed_dead_code_raw`
- `tests/duplication_catalog_contract_periphery.rs:104-108` `read_committed_catalog_raw`
- `tests/responsibility_map_contract_periphery.rs:80-84` `read_committed_map_raw`

#### `dup-0600` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/dead_code_json_contract_periphery.rs, tests/duplication_catalog_contract_periphery.rs, tests/responsibility_map_contract_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dead_code_json_contract_periphery.rs:238-247` `deserialize_committed_dead_code`
- `tests/duplication_catalog_contract_periphery.rs:110-119` `deserialize_committed_catalog`
- `tests/responsibility_map_contract_periphery.rs:86-94` `deserialize_committed_map`

#### `dup-0601` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/dead_code_json_contract_periphery.rs, tests/duplication_catalog_contract_periphery.rs, tests/responsibility_map_contract_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dead_code_json_contract_periphery.rs:254-261` `the_committed_dead_code_json_deserializes_as_a_downstream_consumer_would`
- `tests/duplication_catalog_contract_periphery.rs:126-134` `the_committed_duplication_catalog_deserializes_as_a_downstream_consumer_would`
- `tests/responsibility_map_contract_periphery.rs:101-108` `the_committed_responsibility_map_deserializes_as_a_downstream_consumer_would`

#### `dup-0602` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/dead_code_json_contract_periphery.rs, tests/duplication_catalog_contract_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dead_code_json_contract_periphery.rs:336-353` `every_deserialized_test_only_reference_has_a_non_empty_file_and_content_hash`
- `tests/duplication_catalog_contract_periphery.rs:171-195` `every_deserialized_site_has_a_non_empty_file_name_and_content_hash`

#### `dup-0603` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/dead_code_json_contract_periphery.rs, tests/duplication_catalog_contract_periphery.rs, tests/responsibility_map_contract_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dead_code_json_contract_periphery.rs:374-381` `deserialize_committed_dead_code_lines`
- `tests/duplication_catalog_contract_periphery.rs:357-364` `deserialize_committed_catalog_lines`
- `tests/responsibility_map_contract_periphery.rs:275-282` `deserialize_committed_map_lines`

#### `dup-0604` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/dead_code_json_contract_periphery.rs, tests/duplication_catalog_contract_periphery.rs, tests/responsibility_map_contract_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dead_code_json_contract_periphery.rs:447-460` `deserializing_then_reserializing_the_committed_dead_code_json_reproduces_the_committed_bytes_exactly`
- `tests/duplication_catalog_contract_periphery.rs:314-326` `deserializing_then_reserializing_the_committed_catalog_reproduces_the_committed_bytes_exactly`
- `tests/responsibility_map_contract_periphery.rs:241-253` `deserializing_then_reserializing_reproduces_the_committed_bytes_exactly`

#### `dup-0605` (near, 4 sites)

Proposed home: `dead_code_json_contract_periphery::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dead_code_json_contract_periphery.rs:593-610` `generic_impl_header_constructors_previously_false_flagged_are_absent_from_the_committed_file`
- `tests/dead_code_json_contract_periphery.rs:629-657` `value_position_and_ufcs_reference_shapes_previously_invisible_are_absent_from_the_committed_file`
- `tests/dead_code_json_contract_periphery.rs:670-683` `the_general_ufcs_method_value_fix_also_closes_previously_unreported_same_class_instances`
- `tests/dead_code_json_contract_periphery.rs:699-715` `getter_methods_kept_alive_only_by_a_same_named_production_field_or_local_are_also_absent`

#### `dup-0606` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/dead_code_json_contract_periphery.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dead_code_json_contract_periphery.rs:780-800` `the_committed_dead_code_json_disposition_split_is_23_delete_3_keep_pending_0_keep_public_surface`
- `tests/simplification_audit.rs:10671-10691` `the_real_tree_disposition_split_matches_this_criterions_research`

#### `dup-0607` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/dead_code_json_contract_periphery.rs, tests/responsibility_map_contract_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dead_code_json_contract_periphery.rs:871-891` `section_4_3_citations`
- `tests/responsibility_map_contract_periphery.rs:318-339` `section_1_citations`

#### `dup-0608` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/dedup_seeding_periphery.rs, tests/published_content_key_split_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dedup_seeding_periphery.rs:390-398` `minted`
- `tests/published_content_key_split_periphery.rs:400-408` `minted`

#### `dup-0609` (near, 2 sites)

Proposed home: `design_intent_events::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/design_intent_events.rs:209-255` `the_public_emit_lowers_every_concept_kind_onto_the_fold_arm_that_matches_it`
- `tests/design_intent_events.rs:413-456` `the_public_link_emit_lowers_every_link_rel_onto_the_fold_arm_that_matches_it`

#### `dup-0610` (near, 2 sites)

Proposed home: `design_intent_events::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/design_intent_events.rs:796-859` `every_recognized_end_user_usage_shape_is_dropped_before_the_fold`
- `tests/design_intent_events.rs:965-1054` `a_design_word_in_a_non_handbook_usage_doc_does_not_leak_the_handbook_content_keep`

#### `dup-0611` (near, 4 sites)

Proposed home: `escalation_resume_periphery::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/escalation_resume_periphery.rs:376-384` `resume_unit_rejects_a_non_numeric_attempts_value`
- `tests/escalation_resume_periphery.rs:388-396` `resume_unit_rejects_a_zero_attempts_value`
- `tests/escalation_resume_periphery.rs:401-409` `resume_unit_rejects_a_dangling_attempts_flag_with_no_value`
- `tests/escalation_resume_periphery.rs:414-422` `resume_unit_rejects_an_unknown_flag`

#### `dup-0612` (exact, 2 sites)

Proposed home: `fanout_template_needs_and_stage_retries_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/fanout_template_needs_and_stage_retries_periphery.rs:210-218` `write_git_worker_agent`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:223-231` `write_repoless_worker_agent`

#### `dup-0613` (near, 2 sites)

Proposed home: `fanout_template_needs_and_stage_retries_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/fanout_template_needs_and_stage_retries_periphery.rs:831-937` `checkin_stays_unready_while_a_real_split_siblings_partner_has_not_integrated_yet`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:946-1031` `checkin_never_becomes_ready_when_a_real_split_siblings_partner_escalates_instead`

#### `dup-0614` (near, 2 sites)

Proposed home: `fanout_template_needs_and_stage_retries_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/fanout_template_needs_and_stage_retries_periphery.rs:1250-1299` `a_stages_own_max_retries_yaml_key_lowers_the_effective_bound_below_a_higher_default`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:1308-1391` `a_stages_own_max_retries_yaml_key_raises_the_effective_bound_above_a_lower_default`

#### `dup-0615` (semantic, 2 sites)

Proposed home: `one shared `init_repo_with_head` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/gate_store_fence_periphery.rs:491-507` `init_repo_with_head`
- `tests/spawn_target_dir_periphery.rs:55-71` `init_repo_with_head`

#### `dup-0616` (near, 2 sites)

Proposed home: `gate_store_fence_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/gate_store_fence_periphery.rs:526-597` `a_real_fenced_couriers_scratch_store_is_reclaimed_when_the_worktree_is_removed`
- `tests/gate_store_fence_periphery.rs:603-700` `a_real_fenced_couriers_scratch_store_is_reclaimed_for_a_review_worktree_too`

#### `dup-0617` (semantic, 3 sites)

Proposed home: `one shared `gitsemver_available` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/gitsemver_derivation.rs:83-89` `gitsemver_available`
- `tests/gitsemver_worktree_periphery.rs:117-123` `gitsemver_available`
- `tests/validate_behind_the_tree_periphery.rs:139-145` `gitsemver_available`

#### `dup-0618` (exact, 2 sites)

Proposed home: `gitsemver_derivation::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/gitsemver_derivation.rs:92-110` `a_plain_commit_after_a_tag_increments_the_patch`
- `tests/gitsemver_derivation.rs:113-131` `a_feat_commit_after_a_tag_increments_the_minor`

#### `dup-0619` (near, 5 sites)

Proposed home: `a new shared module (sites span 5 files: tests/gitsemver_path_inclusion_accounting_periphery.rs, tests/hermetic_test_git_audit.rs, tests/no_os_kill_audit.rs, tests/reap_before_removal_audit.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/gitsemver_path_inclusion_accounting_periphery.rs:58-71` `collect_rs_files`
- `tests/hermetic_test_git_audit.rs:342-355` `collect_rs_files`
- `tests/no_os_kill_audit.rs:324-337` `collect_rs_files`
- `tests/reap_before_removal_audit.rs:761-774` `collect_rs_files`
- `tests/simplification_audit.rs:2069-2082` `collect_rs_files`

#### `dup-0620` (semantic, 5 sites)

Proposed home: `one shared `collect_rs_files` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 5 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/gitsemver_path_inclusion_accounting_periphery.rs:58-71` `collect_rs_files`
- `tests/hermetic_test_git_audit.rs:342-355` `collect_rs_files`
- `tests/no_os_kill_audit.rs:324-337` `collect_rs_files`
- `tests/reap_before_removal_audit.rs:761-774` `collect_rs_files`
- `tests/simplification_audit.rs:2069-2082` `collect_rs_files`

#### `dup-0621` (exact, 2 sites)

Proposed home: `gitsemver_path_inclusion_accounting_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/gitsemver_path_inclusion_accounting_periphery.rs:176-180` `a_real_top_level_path_attribute_line_is_recognized`
- `tests/gitsemver_path_inclusion_accounting_periphery.rs:183-187` `an_indented_path_attribute_line_is_still_recognized`

#### `dup-0622` (exact, 2 sites)

Proposed home: `gitsemver_path_inclusion_accounting_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/gitsemver_path_inclusion_accounting_periphery.rs:190-196` `a_doc_comment_quoting_the_attribute_as_prose_is_not_mistaken_for_a_real_site`
- `tests/gitsemver_path_inclusion_accounting_periphery.rs:199-203` `a_line_comment_quoting_the_attribute_as_prose_is_not_mistaken_for_a_real_site`

#### `dup-0623` (exact, 6 sites)

Proposed home: `a new shared module (sites span 6 files: tests/graph_around_code_first.rs, tests/graph_around_governance_boundaries.rs, tests/graph_show_periphery.rs, tests/graph_show_staleness.rs, tests/graph_show_surface.rs, tests/workflow_definition_and_js_constants_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_around_code_first.rs:52-68` `run_stream_identity`
- `tests/graph_around_governance_boundaries.rs:59-75` `run_stream_identity`
- `tests/graph_show_periphery.rs:67-83` `run_stream_identity`
- `tests/graph_show_staleness.rs:52-68` `run_stream_identity`
- `tests/graph_show_surface.rs:48-64` `run_stream_identity`
- `tests/workflow_definition_and_js_constants_periphery.rs:117-133` `run_stream_identity`

#### `dup-0624` (near, 5 sites)

Proposed home: `a new shared module (sites span 5 files: tests/graph_around_code_first.rs, tests/graph_around_governance_boundaries.rs, tests/graph_show_periphery.rs, tests/graph_show_staleness.rs, tests/graph_show_surface.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_around_code_first.rs:98-105` `seed_def`
- `tests/graph_around_governance_boundaries.rs:105-112` `seed_def`
- `tests/graph_show_periphery.rs:115-130` `seed_def_lang`
- `tests/graph_show_staleness.rs:86-93` `seed_def`
- `tests/graph_show_surface.rs:94-101` `seed_def`

#### `dup-0625` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/graph_around_code_first.rs, tests/graph_around_governance_boundaries.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_around_code_first.rs:229-269` `run_rigger_ignores_an_inherited_ambient_store_fence`
- `tests/graph_around_governance_boundaries.rs:387-424` `run_rigger_ignores_an_inherited_ambient_store_fence`

#### `dup-0626` (exact, 2 sites)

Proposed home: `graph_around_code_first::restore`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_around_code_first.rs:233-235` `drop`
- `tests/graph_around_governance_boundaries.rs:391-393` `drop`

#### `dup-0627` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/graph_collision_body_and_tiebreak.rs, tests/graph_density_spread_floor_and_centring.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_collision_body_and_tiebreak.rs:98-118` `run_driver`
- `tests/graph_density_spread_floor_and_centring.rs:104-124` `run_driver`

#### `dup-0628` (exact, 2 sites)

Proposed home: `graph_collision_body_and_tiebreak::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_collision_body_and_tiebreak.rs:215-236` `the_collision_body_encloses_the_circle_and_its_label`
- `tests/graph_collision_body_and_tiebreak.rs:242-263` `the_separation_pass_resolves_coincident_nodes_deterministically`

#### `dup-0629` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/graph_denoise_content_survives.rs, tests/graph_denoise_target_project.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_denoise_content_survives.rs:35-42` `fold`
- `tests/graph_denoise_target_project.rs:36-43` `fold`

#### `dup-0630` (near, 2 sites)

Proposed home: `graph_denoise_target_project::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_denoise_target_project.rs:46-198` `a_whole_runs_fold_projects_the_content_but_none_of_the_machinery`
- `tests/graph_denoise_target_project.rs:201-268` `the_actor_metadata_on_a_decision_and_finding_never_folds_an_agent_node`

#### `dup-0631` (near, 4 sites)

Proposed home: `a new shared module (sites span 4 files: tests/graph_density_spread_floor_and_centring.rs, tests/readable_graph_adaptive_labels.rs, tests/readable_graph_density_scaled_spacing.rs, tests/readable_graph_layout_separation.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_density_spread_floor_and_centring.rs:131-138` `the_served_page_wires_the_layered_path_without_the_density_accessors`
- `tests/readable_graph_adaptive_labels.rs:72-98` `the_served_page_ships_the_adaptive_label_declutter`
- `tests/readable_graph_density_scaled_spacing.rs:63-80` `the_served_page_ships_the_density_spread_lever`
- `tests/readable_graph_layout_separation.rs:56-76` `the_served_page_ships_the_collision_separation_pass`

#### `dup-0632` (exact, 3 sites)

Proposed home: `graph_density_spread_floor_and_centring::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_density_spread_floor_and_centring.rs:184-199` `the_spread_factor_floors_at_one_off_the_dense_path`
- `tests/graph_density_spread_floor_and_centring.rs:259-275` `the_bare_four_arg_layout_stays_panel_sized_and_the_accessor_path_grows_past_it`
- `tests/graph_density_spread_floor_and_centring.rs:323-339` `the_enlarged_canvas_is_centred_on_the_panel_middle`

#### `dup-0633` (semantic, 2 sites)

Proposed home: `one shared `apply_governs` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/graph_fold_dedup_live_edge.rs:38-46` `apply_governs`
- `tests/graph_rebuild_collapses_dupes.rs:40-48` `apply_governs`

#### `dup-0634` (exact, 4 sites)

Proposed home: `a new shared module (sites span 4 files: tests/graph_fold_dedup_live_edge.rs, tests/graph_rebuild_collapses_dupes.rs, tests/graph_superseded_prune.rs, tests/reset_menu_previews_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_fold_dedup_live_edge.rs:51-53` `nanos`
- `tests/graph_rebuild_collapses_dupes.rs:53-55` `nanos`
- `tests/graph_superseded_prune.rs:53-55` `nanos`
- `tests/reset_menu_previews_periphery.rs:71-73` `nanos`

#### `dup-0635` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/graph_fold_dedup_live_edge.rs, tests/graph_rebuild_collapses_dupes.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_fold_dedup_live_edge.rs:57-65` `governs`
- `tests/graph_rebuild_collapses_dupes.rs:59-67` `governs`

#### `dup-0636` (near, 2 sites)

Proposed home: `graph_fold_dedup_live_edge::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_fold_dedup_live_edge.rs:68-106` `subgraph_collapses_repeated_governs_to_one_live_edge_keeping_latest_provenance`
- `tests/graph_fold_dedup_live_edge.rs:109-130` `the_collapsed_edge_keeps_the_earliest_fact_time_and_latest_source_regardless_of_arrival_order`

#### `dup-0637` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/graph_show_periphery.rs, tests/graph_show_staleness.rs, tests/graph_show_surface.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_show_periphery.rs:86-88` `seed_rigger_dir`
- `tests/graph_show_staleness.rs:71-73` `seed_rigger_dir`
- `tests/graph_show_surface.rs:67-69` `seed_rigger_dir`

#### `dup-0638` (semantic, 3 sites)

Proposed home: `one shared `seed_rigger_dir` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/graph_show_periphery.rs:86-88` `seed_rigger_dir`
- `tests/graph_show_staleness.rs:71-73` `seed_rigger_dir`
- `tests/graph_show_surface.rs:67-69` `seed_rigger_dir`

#### `dup-0639` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/graph_show_periphery.rs, tests/graph_show_staleness.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_show_periphery.rs:138-141` `open_graph`
- `tests/graph_show_staleness.rs:76-79` `open_graph`

#### `dup-0640` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/graph_show_periphery.rs, tests/graph_show_staleness.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_show_periphery.rs:147-156` `body_line_count`
- `tests/graph_show_staleness.rs:119-128` `body_line_count`

#### `dup-0641` (semantic, 2 sites)

Proposed home: `one shared `body_line_count` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/graph_show_periphery.rs:147-156` `body_line_count`
- `tests/graph_show_staleness.rs:119-128` `body_line_count`

#### `dup-0642` (semantic, 2 sites)

Proposed home: `one shared `assert_light_lane_extent_note` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/graph_show_periphery.rs:163-177` `assert_light_lane_extent_note`
- `tests/graph_show_surface.rs:109-118` `assert_light_lane_extent_note`

#### `dup-0643` (near, 14 sites)

Proposed home: `a new shared module (sites span 2 files: tests/graph_show_periphery.rs, tests/graph_show_staleness.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_show_periphery.rs:222-277` `graph_show_degrades_to_stale_note_when_location_drifted`
- `tests/graph_show_periphery.rs:287-329` `graph_show_degrades_to_stale_note_when_recorded_line_is_zero`
- `tests/graph_show_periphery.rs:359-389` `graph_show_light_lane_degrades_to_extent_unavailable_note`
- `tests/graph_show_periphery.rs:398-450` `graph_show_bounds_body_at_the_definitions_own_extent`
- `tests/graph_show_periphery.rs:462-537` `graph_show_shows_full_body_past_nested_definition`
- `tests/graph_show_periphery.rs:548-596` `graph_show_shows_full_body_of_a_destructuring_signature`
- `tests/graph_show_periphery.rs:604-646` `graph_show_extent_ignores_braces_in_strings_comments_and_chars`
- `tests/graph_show_periphery.rs:656-718` `graph_show_shows_full_body_of_a_python_nested_def`
- `tests/graph_show_periphery.rs:729-769` `graph_show_does_not_overread_a_js_single_quote_brace_body`
- `tests/graph_show_periphery.rs:840-893` `graph_show_degrades_when_no_grammar_registered_for_the_file_extension`
- `tests/graph_show_periphery.rs:907-966` `graph_show_heals_to_the_live_line_when_the_moved_name_is_unambiguous`
- `tests/graph_show_periphery.rs:975-1027` `graph_show_degrades_when_the_moved_name_is_ambiguous_in_the_file`
- `tests/graph_show_staleness.rs:135-179` `graph_show_degrades_gracefully_when_the_recorded_file_is_missing`
- `tests/graph_show_staleness.rs:196-274` `graph_show_never_presents_a_neighbours_body_when_the_line_drifted`

#### `dup-0644` (semantic, 2 sites)

Proposed home: `one shared `git_toplevel` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/heartbeat_write_read_agree_periphery.rs:68-78` `git_toplevel`
- `tests/reset_derived_live_writer_guard_periphery.rs:57-68` `git_toplevel`

#### `dup-0645` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/heartbeat_write_read_agree_periphery.rs, tests/relocated_worktree_store_resolution_periphery.rs, tests/reset_derived_live_writer_guard_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/heartbeat_write_read_agree_periphery.rs:128-146` `run_stream_identity`
- `tests/relocated_worktree_store_resolution_periphery.rs:67-79` `stream_identity`
- `tests/reset_derived_live_writer_guard_periphery.rs:73-87` `run_stream_identity`

#### `dup-0646` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/heartbeat_write_read_agree_periphery.rs, tests/watchdog_cli_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/heartbeat_write_read_agree_periphery.rs:189-194` `now_nanos`
- `tests/watchdog_cli_periphery.rs:208-213` `now_nanos`

#### `dup-0647` (near, 2 sites)

Proposed home: `integrate_conflict_merge_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/integrate_conflict_merge_periphery.rs:527-604` `spawn`
- `tests/integrate_conflict_merge_periphery.rs:1444-1501` `spawn`

#### `dup-0648` (near, 2 sites)

Proposed home: `integrate_conflict_merge_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/integrate_conflict_merge_periphery.rs:1026-1062` `spawn`
- `tests/integrate_conflict_merge_periphery.rs:1247-1293` `spawn`

#### `dup-0649` (near, 3 sites)

Proposed home: `integrate_conflict_merge_periphery::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/integrate_conflict_merge_periphery.rs:1887-1912` `spawn`
- `tests/integrate_conflict_merge_periphery.rs:2437-2459` `spawn`
- `tests/integrate_conflict_merge_periphery.rs:2682-2702` `spawn`

#### `dup-0650` (exact, 2 sites)

Proposed home: `integrate_conflict_merge_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/integrate_conflict_merge_periphery.rs:2106-2113` `read_stream`
- `tests/integrate_conflict_merge_periphery.rs:3520-3527` `read_stream`

#### `dup-0651` (exact, 2 sites)

Proposed home: `integrate_conflict_merge_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/integrate_conflict_merge_periphery.rs:2114-2121` `read_all`
- `tests/integrate_conflict_merge_periphery.rs:3528-3535` `read_all`

#### `dup-0652` (near, 2 sites)

Proposed home: `integrate_conflict_merge_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/integrate_conflict_merge_periphery.rs:2132-2137` `has_status_marker`
- `tests/integrate_conflict_merge_periphery.rs:2139-2147` `count_status_marker`

#### `dup-0653` (near, 2 sites)

Proposed home: `integrate_conflict_merge_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/integrate_conflict_merge_periphery.rs:2164-2187` `spawn`
- `tests/integrate_conflict_merge_periphery.rs:2302-2322` `spawn`

#### `dup-0654` (near, 4 sites)

Proposed home: `integrate_conflict_merge_periphery::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/integrate_conflict_merge_periphery.rs:2192-2289` `a_crash_right_after_the_merge_attempt_record_resumes_and_completes_row_1`
- `tests/integrate_conflict_merge_periphery.rs:2327-2423` `a_crash_right_after_the_landing_intent_record_resumes_and_completes_row_4`
- `tests/integrate_conflict_merge_periphery.rs:2931-3031` `a_crash_right_after_the_merge_succeeds_resumes_and_completes_row_1_after_record`
- `tests/integrate_conflict_merge_periphery.rs:3041-3139` `a_crash_right_after_landing_succeeds_resumes_and_completes_row_4_after_record`

#### `dup-0655` (exact, 2 sites)

Proposed home: `integrate_conflict_merge_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/integrate_conflict_merge_periphery.rs:2705-2730` `confined_cfg`
- `tests/integrate_conflict_merge_periphery.rs:3197-3222` `mixed_cfg`

#### `dup-0656` (near, 3 sites)

Proposed home: `integrate_conflict_merge_periphery::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/integrate_conflict_merge_periphery.rs:2737-2897` `a_confined_regenerate_command_failure_and_a_store_failure_each_resume_and_complete_row_3`
- `tests/integrate_conflict_merge_periphery.rs:3231-3333` `a_regenerate_command_failure_right_after_landing_completes_row_3_on_resume_when_row_4_is_already_closed`
- `tests/integrate_conflict_merge_periphery.rs:3346-3466` `a_crash_right_after_landing_succeeds_with_owed_regeneration_completes_row_3_on_resume`

#### `dup-0657` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/kurrentdb_always_available.rs, tests/readme_retirement_rationale.rs, tests/turbovec_retired.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/kurrentdb_always_available.rs:28-32` `manifest_text`
- `tests/readme_retirement_rationale.rs:31-34` `readme_text`
- `tests/turbovec_retired.rs:34-38` `manifest_text`

#### `dup-0658` (semantic, 2 sites)

Proposed home: `one shared `manifest_text` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/kurrentdb_always_available.rs:28-32` `manifest_text`
- `tests/turbovec_retired.rs:34-38` `manifest_text`

#### `dup-0659` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/kurrentdb_always_available.rs, tests/turbovec_retired.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/kurrentdb_always_available.rs:37-52` `table_lines`
- `tests/turbovec_retired.rs:43-58` `table_lines`

#### `dup-0660` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/kurrentdb_always_available.rs, tests/turbovec_retired.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/kurrentdb_always_available.rs:57-65` `table_declares_key`
- `tests/turbovec_retired.rs:63-71` `table_declares_key`

#### `dup-0661` (semantic, 2 sites)

Proposed home: `one shared `table_declares_key` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/kurrentdb_always_available.rs:57-65` `table_declares_key`
- `tests/turbovec_retired.rs:63-71` `table_declares_key`

#### `dup-0662` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/kurrentdb_always_available.rs, tests/turbovec_retired.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/kurrentdb_always_available.rs:164-179` `for_each_rs_file`
- `tests/turbovec_retired.rs:75-90` `for_each_rs_file`

#### `dup-0663` (semantic, 2 sites)

Proposed home: `one shared `for_each_rs_file` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/kurrentdb_always_available.rs:164-179` `for_each_rs_file`
- `tests/turbovec_retired.rs:75-90` `for_each_rs_file`

#### `dup-0664` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/kurrentdb_always_available.rs, tests/turbovec_retired.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/kurrentdb_always_available.rs:190-209` `no_source_still_gates_on_the_retired_kurrentdb_feature`
- `tests/turbovec_retired.rs:168-186` `no_source_still_gates_on_the_retired_turbovec_feature`

#### `dup-0665` (semantic, 5 sites)

Proposed home: `one shared `js_declaration` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 5 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/meta_phases_declaration_periphery.rs:65-87` `js_declaration`
- `tests/phase_of_role_mapping_periphery.rs:50-72` `js_declaration`
- `tests/review_tier_roster_periphery.rs:18-40` `js_declaration`
- `tests/step_attention_periphery.rs:655-677` `js_declaration`
- `tests/worker_persona_label_periphery.rs:21-43` `js_declaration`

#### `dup-0666` (near, 4 sites)

Proposed home: `a new shared module (sites span 4 files: tests/metadata_card_handoff_viz.rs, tests/proof_row_renders_on_the_card.rs, tests/subject_lens_overlay_client_arms.rs, tests/subject_view_memory_rail_client.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/metadata_card_handoff_viz.rs:123-138` `build_harness`
- `tests/proof_row_renders_on_the_card.rs:76-91` `build_harness`
- `tests/subject_lens_overlay_client_arms.rs:82-97` `build_harness`
- `tests/subject_view_memory_rail_client.rs:100-115` `build_harness`

#### `dup-0667` (near, 8 sites)

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

#### `dup-0668` (near, 2 sites)

Proposed home: `migration_is_deliberate_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/migration_is_deliberate_periphery.rs:512-539` `seed_a_retired_entity`
- `tests/migration_is_deliberate_periphery.rs:543-563` `seed_a_live_entity`

#### `dup-0669` (semantic, 4 sites)

Proposed home: `one shared `sigterm_ignorer_in` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 4 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/mutation_scratch_reap_base_guard_periphery.rs:54-61` `sigterm_ignorer_in`
- `tests/reap_before_removal_periphery.rs:41-48` `sigterm_ignorer_in`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:138-145` `sigterm_ignorer_in`
- `tests/worktree_remove_relocated_scratch_base_guard_periphery.rs:55-62` `sigterm_ignorer_in`

#### `dup-0670` (exact, 2 sites)

Proposed home: `no_os_kill_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_audit.rs:65-67` `pkill_word`
- `tests/no_os_kill_audit.rs:71-73` `xkill_word`

#### `dup-0671` (exact, 2 sites)

Proposed home: `no_os_kill_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_audit.rs:68-70` `killall_word`
- `tests/no_os_kill_audit.rs:74-76` `pg_signal_word`

#### `dup-0672` (exact, 2 sites)

Proposed home: `no_os_kill_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_audit.rs:80-82` `libc_kill_open`
- `tests/no_os_kill_audit.rs:83-85` `signal_kill_open`

#### `dup-0673` (exact, 4 sites)

Proposed home: `no_os_kill_audit::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_audit.rs:175-177` `shape_pg_signal`
- `tests/no_os_kill_audit.rs:180-182` `shape_libc_kill`
- `tests/no_os_kill_audit.rs:185-187` `shape_signal_kill`
- `tests/no_os_kill_audit.rs:191-193` `shape_direct_rustix_call`

#### `dup-0674` (near, 2 sites)

Proposed home: `no_os_kill_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_audit.rs:196-216` `shape_arg_dashdash`
- `tests/no_os_kill_audit.rs:220-240` `shape_format_dash_brace`

#### `dup-0675` (exact, 2 sites)

Proposed home: `no_os_kill_audit::finding`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_audit.rs:259-268` `fmt`
- `tests/reap_before_removal_audit.rs:131-140` `fmt`

#### `dup-0676` (near, 2 sites)

Proposed home: `no_os_kill_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_audit.rs:273-303` `general_hits`
- `tests/no_os_kill_audit.rs:309-321` `sanctioned_hits`

#### `dup-0677` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/no_os_kill_audit.rs, tests/reap_before_removal_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_audit.rs:382-388` `write_file`
- `tests/reap_before_removal_audit.rs:845-851` `write_file`

#### `dup-0678` (near, 11 sites)

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

#### `dup-0679` (near, 4 sites)

Proposed home: `no_os_kill_audit::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_audit.rs:495-503` `arg_dashdash_separator_is_caught_outside_the_sanctioned_files`
- `tests/no_os_kill_audit.rs:506-514` `negative_pid_format_is_caught_outside_the_sanctioned_files`
- `tests/no_os_kill_audit.rs:571-583` `a_dashdash_separator_inside_a_sanctioned_file_is_still_caught`
- `tests/no_os_kill_audit.rs:586-598` `a_negative_pid_format_inside_a_sanctioned_file_is_still_caught`

#### `dup-0680` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/no_os_kill_audit.rs, tests/reap_before_removal_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_audit.rs:622-635` `the_real_tree_carries_no_forbidden_pattern`
- `tests/reap_before_removal_audit.rs:1725-1738` `the_real_tree_carries_no_bare_removal`

#### `dup-0681` (exact, 4 sites)

Proposed home: `no_os_kill_test_helper_periphery::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_test_helper_periphery.rs:176-180` `terminate_pid_refuses_pid_zero`
- `tests/no_os_kill_test_helper_periphery.rs:184-194` `terminate_pid_refuses_pid_one`
- `tests/no_os_kill_test_helper_periphery.rs:213-218` `stop_pid_refuses_pid_zero`
- `tests/no_os_kill_test_helper_periphery.rs:222-229` `stop_pid_refuses_pid_one`

#### `dup-0682` (exact, 2 sites)

Proposed home: `no_os_kill_test_helper_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_test_helper_periphery.rs:198-200` `terminate_pid_refuses_its_callers_own_pid`
- `tests/no_os_kill_test_helper_periphery.rs:233-235` `stop_pid_refuses_its_callers_own_pid`

#### `dup-0683` (near, 2 sites)

Proposed home: `parallel_ordered_emit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/parallel_ordered_emit.rs:71-77` `drive_default`
- `tests/parallel_ordered_emit.rs:80-89` `drive_paced`

#### `dup-0684` (near, 3 sites)

Proposed home: `phase_of_role_mapping_periphery::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/phase_of_role_mapping_periphery.rs:123-138` `plan_and_plan_critique_resolve_to_plan_regardless_of_role`
- `tests/phase_of_role_mapping_periphery.rs:148-163` `review_tier_roles_resolve_to_review`
- `tests/phase_of_role_mapping_periphery.rs:172-187` `implementer_and_any_unrecognized_role_resolve_to_the_fail_visible_build_default`

#### `dup-0685` (semantic, 2 sites)

Proposed home: `one shared `production_main_rs` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/projections_stay_local.rs:38-48` `production_main_rs`
- `tests/store_resolution.rs:28-32` `production_main_rs`

#### `dup-0686` (semantic, 2 sites)

Proposed home: `one shared `start_kurrentdb` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/projections_stay_local.rs:161-191` `start_kurrentdb`
- `tests/store_resolution.rs:176-207` `start_kurrentdb`

#### `dup-0687` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/projections_stay_local.rs, tests/store_resolution.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/projections_stay_local.rs:269-346` `progress_against_the_server_keeps_progress_db_local_and_the_log_on_the_server`
- `tests/store_resolution.rs:223-298` `a_courier_in_a_project_configured_for_the_server_resolves_the_server_store`

#### `dup-0688` (near, 3 sites)

Proposed home: `proof_lands_on_the_card_periphery::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/proof_lands_on_the_card_periphery.rs:354-405` `cross_file_proof_survives_a_real_reextraction_of_the_defining_file`
- `tests/proof_lands_on_the_card_periphery.rs:642-685` `a_deleted_test_reference_retracts_its_stale_proof_through_the_real_pipeline`
- `tests/proof_lands_on_the_card_periphery.rs:716-762` `a_reference_free_tests_dir_files_first_extraction_creates_nothing_and_leaves_no_residue`

#### `dup-0689` (exact, 15 sites)

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

#### `dup-0690` (near, 3 sites)

Proposed home: `reap_before_removal_audit::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reap_before_removal_audit.rs:874-890` `bare_remove_dir_all_with_no_coverage_is_caught`
- `tests/reap_before_removal_audit.rs:893-912` `bare_git_worktree_remove_with_no_coverage_is_caught`
- `tests/reap_before_removal_audit.rs:1635-1665` `a_worktree_remove_args_array_wrapped_across_multiple_lines_by_rustfmt_is_still_caught`

#### `dup-0691` (exact, 7 sites)

Proposed home: `reap_before_removal_audit::support (consolidate these 7 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reap_before_removal_audit.rs:942-961` `a_reap_authority_name_in_a_prose_comment_never_covers_the_removal`
- `tests/reap_before_removal_audit.rs:1151-1176` `a_reap_call_covering_one_removal_never_bleeds_onto_a_later_unrelated_removal_in_the_same_function`
- `tests/reap_before_removal_audit.rs:1283-1308` `a_reap_call_only_in_a_sibling_function_never_covers_this_one`
- `tests/reap_before_removal_audit.rs:1333-1355` `a_removal_inside_a_standalone_cfg_test_fn_is_never_scanned_and_a_later_real_fn_still_is`
- `tests/reap_before_removal_audit.rs:1402-1423` `a_semicolon_terminated_cfg_test_item_excludes_only_itself`
- `tests/reap_before_removal_audit.rs:1495-1515` `a_reap_call_after_the_removal_never_covers_it_remove_then_reap_is_still_flagged`
- `tests/reap_before_removal_audit.rs:1549-1573` `an_exemption_marker_attached_to_one_removal_never_covers_an_unrelated_second_removal`

#### `dup-0692` (exact, 7 sites)

Proposed home: `reap_before_removal_audit::support (consolidate these 7 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reap_before_removal_audit.rs:969-989` `a_reap_authority_name_in_a_trailing_comment_never_covers_the_removal`
- `tests/reap_before_removal_audit.rs:1020-1039` `an_exemption_marker_inside_a_non_comment_string_literal_never_covers_the_removal`
- `tests/reap_before_removal_audit.rs:1071-1090` `a_reap_authority_name_inside_a_non_comment_string_literal_never_covers_the_removal`
- `tests/reap_before_removal_audit.rs:1099-1118` `a_reap_authority_name_inside_a_single_line_block_comment_never_covers_the_removal`
- `tests/reap_before_removal_audit.rs:1262-1280` `an_arbitrary_comment_is_never_mistaken_for_the_exemption_marker`
- `tests/reap_before_removal_audit.rs:1381-1399` `a_doc_comment_mentioning_the_cfg_test_attribute_in_prose_is_never_mistaken_for_it`
- `tests/reap_before_removal_audit.rs:1581-1600` `a_literal_empty_string_authorized_root_argument_never_covers_the_removal`

#### `dup-0693` (near, 5 sites)

Proposed home: `reap_before_removal_periphery::support (consolidate these 5 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reap_before_removal_periphery.rs:80-135` `worktree_remove_reaps_a_process_rooted_in_its_sibling_build_cache_before_reclaiming_it`
- `tests/reap_before_removal_periphery.rs:155-203` `worktree_remove_reaps_a_process_rooted_in_its_sibling_mutants_root_before_reclaiming_it`
- `tests/reap_before_removal_periphery.rs:206-266` `discard_reaps_a_process_rooted_in_the_review_worktrees_fence_sibling_before_reclaiming_it`
- `tests/reap_before_removal_periphery.rs:269-329` `worktree_remove_reaps_a_process_rooted_in_the_cache_dirs_own_store_fence_sibling_before_reclaiming_it`
- `tests/reap_before_removal_periphery.rs:332-393` `discard_reaps_a_process_rooted_in_the_review_worktree_itself_before_clearing_it`

#### `dup-0694` (near, 2 sites)

Proposed home: `reminder_dedup_workflow_child_env_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reminder_dedup_workflow_child_env_periphery.rs:83-115` `workflow_stamps_its_own_pid_on_the_spawned_child_with_no_inbound_sentinel`
- `tests/reminder_dedup_workflow_child_env_periphery.rs:124-159` `workflow_still_stamps_a_fresh_own_pid_on_the_child_even_when_its_own_reminder_was_suppressed`

#### `dup-0695` (exact, 2 sites)

Proposed home: `replan_episode_identity::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/replan_episode_identity.rs:141-151` `new`
- `tests/replan_episode_identity.rs:404-414` `new`

#### `dup-0696` (near, 2 sites)

Proposed home: `replan_episode_identity::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/replan_episode_identity.rs:176-231` `spawn`
- `tests/replan_episode_identity.rs:418-469` `spawn`

#### `dup-0697` (near, 2 sites)

Proposed home: `replan_episode_identity::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/replan_episode_identity.rs:305-384` `a_replan_after_a_critique_reject_supersedes_the_initial_episodes_unit`
- `tests/replan_episode_identity.rs:483-561` `a_second_replan_supersedes_both_earlier_episodes_units`

#### `dup-0698` (near, 3 sites)

Proposed home: `replan_episode_identity::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/replan_episode_identity.rs:753-838` `a_same_id_refine_survives_its_own_episodes_new_sibling_through_the_real_write_path`
- `tests/replan_episode_identity.rs:854-939` `a_same_id_refine_survives_its_own_episodes_new_sibling_walked_first_through_the_real_write_path`
- `tests/replan_episode_identity.rs:954-1044` `a_same_id_refine_survives_its_own_episodes_genuinely_new_unmatched_sibling_through_the_real_write_path`

#### `dup-0699` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/reset_build_cache_periphery.rs, tests/reset_menu.rs, tests/reset_menu_identity_migration_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reset_build_cache_periphery.rs:54-57` `seed_store`
- `tests/reset_menu.rs:81-84` `seed_store`
- `tests/reset_menu_identity_migration_periphery.rs:69-72` `seed_store`

#### `dup-0700` (exact, 2 sites)

Proposed home: `reset_build_cache_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reset_build_cache_periphery.rs:63-65` `shared_cache_dir`
- `tests/reset_build_cache_periphery.rs:67-69` `guard_path`

#### `dup-0701` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/reset_derived_compaction.rs, tests/reset_derived_compaction_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reset_derived_compaction.rs:131-157` `rows`
- `tests/reset_derived_compaction_periphery.rs:116-139` `raw_rows`

#### `dup-0702` (semantic, 2 sites)

Proposed home: `one shared `edge_inferred` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/reset_derived_compaction.rs:183-185` `edge_inferred`
- `tests/reset_menu_previews_periphery.rs:88-91` `edge_inferred`

#### `dup-0703` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/reset_derived_compaction.rs, tests/reset_derived_compaction_periphery.rs, tests/reset_menu_previews_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reset_derived_compaction.rs:193-197` `keyed`
- `tests/reset_derived_compaction_periphery.rs:163-167` `keyed`
- `tests/reset_menu_previews_periphery.rs:75-79` `keyed`

#### `dup-0704` (semantic, 2 sites)

Proposed home: `one shared `path_subject_of` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/reset_derived_compaction_periphery.rs:2109-2124` `path_subject_of`
- `tests/store_content_identity_periphery.rs:233-249` `path_subject_of`

#### `dup-0705` (near, 3 sites)

Proposed home: `reset_derived_live_writer_guard_periphery::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reset_derived_live_writer_guard_periphery.rs:158-168` `reset_derived_prunes_when_no_run_has_ever_started`
- `tests/reset_derived_live_writer_guard_periphery.rs:731-741` `reset_force_live_alone_is_refused_as_no_mode`
- `tests/reset_derived_live_writer_guard_periphery.rs:747-762` `the_derived_help_entry_documents_force_live_and_owns_the_risk`

#### `dup-0706` (near, 4 sites)

Proposed home: `reset_derived_live_writer_guard_periphery::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reset_derived_live_writer_guard_periphery.rs:174-197` `reset_derived_prunes_when_every_unit_is_terminal_and_every_spawn_is_answered`
- `tests/reset_derived_live_writer_guard_periphery.rs:203-229` `reset_derived_ignores_a_prior_runs_unanswered_spawn_and_non_terminal_unit`
- `tests/reset_derived_live_writer_guard_periphery.rs:645-665` `reset_derived_force_live_compacts_despite_an_in_flight_spawn`
- `tests/reset_derived_live_writer_guard_periphery.rs:693-722` `runs_composed_with_a_refused_derived_still_completes_its_own_prune`

#### `dup-0707` (near, 2 sites)

Proposed home: `reset_derived_live_writer_guard_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reset_derived_live_writer_guard_periphery.rs:333-364` `reset_derived_refuses_a_non_terminal_unit_between_spawn_rounds_and_prunes_nothing`
- `tests/reset_derived_live_writer_guard_periphery.rs:373-402` `reset_derived_refuses_an_in_flight_spawn_naming_its_id_and_prunes_nothing`

#### `dup-0708` (near, 2 sites)

Proposed home: `reset_derived_live_writer_guard_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reset_derived_live_writer_guard_periphery.rs:531-557` `reset_derived_ignores_a_registration_for_a_different_store`
- `tests/reset_derived_live_writer_guard_periphery.rs:576-616` `reset_derived_never_deletes_a_stale_foreign_registry_entrys_file`

#### `dup-0709` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/reset_menu.rs, tests/reset_menu_identity_migration_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reset_menu.rs:100-103` `emit`
- `tests/reset_menu_identity_migration_periphery.rs:88-91` `emit`

#### `dup-0710` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/reset_menu.rs, tests/reset_menu_identity_migration_periphery.rs, tests/reset_menu_previews_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reset_menu.rs:144-149` `code_entity`
- `tests/reset_menu_identity_migration_periphery.rs:113-118` `code_entity`
- `tests/reset_menu_previews_periphery.rs:81-86` `code_entity`

#### `dup-0711` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/reset_menu.rs, tests/reset_menu_identity_migration_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reset_menu.rs:153-170` `seed_derived_duplicates`
- `tests/reset_menu_identity_migration_periphery.rs:120-137` `seed_derived_duplicates`

#### `dup-0712` (semantic, 2 sites)

Proposed home: `one shared `seed_derived_duplicates` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/reset_menu.rs:153-170` `seed_derived_duplicates`
- `tests/reset_menu_identity_migration_periphery.rs:120-137` `seed_derived_duplicates`

#### `dup-0713` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/review_tier_roster_periphery.rs, tests/worker_persona_label_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/review_tier_roster_periphery.rs:75-129` `run_worker_label_for_unit_and_reviews`
- `tests/worker_persona_label_periphery.rs:72-112` `run_worker_label_for_unit`

#### `dup-0714` (exact, 2 sites)

Proposed home: `review_tier_roster_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/review_tier_roster_periphery.rs:135-148` `the_adversarys_roster_renders_inside_its_action_phrase`
- `tests/review_tier_roster_periphery.rs:154-167` `the_adjudicators_roster_renders_inside_its_action_phrase`

#### `dup-0715` (exact, 2 sites)

Proposed home: `review_tier_roster_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/review_tier_roster_periphery.rs:211-224` `a_roster_on_a_role_with_no_roster_verb_entry_is_never_rendered`
- `tests/review_tier_roster_periphery.rs:229-242` `a_single_entry_roster_renders_with_no_stray_separator`

#### `dup-0716` (near, 2 sites)

Proposed home: `rigger_run_base_gate_env_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/rigger_run_base_gate_env_periphery.rs:209-233` `rigger_run_base_reaches_a_real_inline_gate_subprocess_but_not_a_real_agent_subprocess`
- `tests/rigger_run_base_gate_env_periphery.rs:236-254` `rigger_run_base_reaches_a_real_deferred_gate_subprocess_too`

#### `dup-0717` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/scratch_workdir_config.rs, tests/store_config.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/scratch_workdir_config.rs:30-35` `rigger_dir`
- `tests/store_config.rs:32-37` `rigger_dir`

#### `dup-0718` (near, 4 sites)

Proposed home: `a new shared module (sites span 3 files: tests/scratch_workdir_config.rs, tests/store_config.rs, tests/store_precedence.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/scratch_workdir_config.rs:37-39` `write_workflow`
- `tests/store_config.rs:40-42` `write_workflow`
- `tests/store_precedence.rs:64-66` `write_store_conn`
- `tests/store_precedence.rs:78-80` `write_store_config`

#### `dup-0719` (exact, 2 sites)

Proposed home: `scratch_workdir_config::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/scratch_workdir_config.rs:53-58` `a_present_workdir_deserializes_exactly`
- `tests/scratch_workdir_config.rs:77-83` `a_workflow_with_no_defaults_block_at_all_reads_as_empty`

#### `dup-0720` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:1745-1754` `map_entry_wire`
- `tests/simplification_audit.rs:1756-1763` `map_entry_lines`

#### `dup-0721` (exact, 6 sites)

Proposed home: `simplification_audit::support (consolidate these 6 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:1770-1775` `map_to_json`
- `tests/simplification_audit.rs:1780-1785` `map_lines_to_json`
- `tests/simplification_audit.rs:3268-3273` `catalog_to_json`
- `tests/simplification_audit.rs:3278-3283` `catalog_lines_to_json`
- `tests/simplification_audit.rs:6332-6338` `dead_code_to_json`
- `tests/simplification_audit.rs:6343-6349` `dead_code_lines_to_json`

#### `dup-0722` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:3180-3183` `real_files`
- `tests/simplification_audit.rs:3187-3190` `real_catalog`

#### `dup-0723` (near, 4 sites)

Proposed home: `simplification_audit::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:3231-3247` `dup_cluster_wire`
- `tests/simplification_audit.rs:3249-3262` `dup_cluster_lines`
- `tests/simplification_audit.rs:6290-6309` `dead_code_candidate_wire`
- `tests/simplification_audit.rs:6311-6326` `dead_code_candidate_lines`

#### `dup-0724` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:3554-3718` `render_section_3`
- `tests/simplification_audit.rs:4115-4358` `render_section_5`

#### `dup-0725` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:3921-4108` `render_section_4`
- `tests/simplification_audit.rs:4447-4996` `render_section_6`

#### `dup-0726` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:6416-6425` `a_simple_free_function_is_found_with_its_line_span`
- `tests/simplification_audit.rs:6656-6662` `production_functions_before_a_cfg_test_mod_are_not_flagged_test`

#### `dup-0727` (exact, 11 sites)

Proposed home: `simplification_audit::support (consolidate these 11 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:6446-6451` `fnv1a_is_not_mistaken_for_the_fn_keyword`
- `tests/simplification_audit.rs:6458-6463` `a_brace_inside_a_line_comment_is_ignored`
- `tests/simplification_audit.rs:6466-6471` `a_brace_inside_a_block_comment_is_ignored`
- `tests/simplification_audit.rs:6474-6479` `nested_block_comments_are_handled`
- `tests/simplification_audit.rs:6482-6487` `a_brace_inside_a_string_literal_is_ignored`
- `tests/simplification_audit.rs:6490-6495` `a_brace_inside_a_raw_string_with_hashes_is_ignored`
- `tests/simplification_audit.rs:6498-6503` `a_brace_inside_a_byte_string_is_ignored`
- `tests/simplification_audit.rs:6506-6511` `a_brace_char_literal_is_not_mistaken_for_real_braces`
- `tests/simplification_audit.rs:6514-6519` `a_lifetime_is_not_mistaken_for_a_char_literal`
- `tests/simplification_audit.rs:6522-6527` `an_escaped_quote_char_literal_does_not_confuse_the_scanner`
- `tests/simplification_audit.rs:6541-6546` `a_trait_default_method_with_a_body_is_recorded`

#### `dup-0728` (near, 5 sites)

Proposed home: `simplification_audit::support (consolidate these 5 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:6570-6576` `a_method_inside_an_impl_block_carries_its_header`
- `tests/simplification_audit.rs:6579-6587` `a_trait_impl_header_keeps_the_trait_for_type_text`
- `tests/simplification_audit.rs:6590-6602` `a_method_inside_an_impl_nested_in_a_cfg_test_mod_is_flagged_test`
- `tests/simplification_audit.rs:6605-6614` `a_cfg_test_attribute_directly_on_an_impl_block_is_flagged_test`
- `tests/simplification_audit.rs:6736-6744` `a_cfg_test_pub_fn_is_still_flagged_test_pub_survives_between_attribute_and_keyword`

#### `dup-0729` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:6627-6633` `a_function_directly_in_a_cfg_test_mod_is_flagged_test`
- `tests/simplification_audit.rs:6636-6645` `a_nested_named_test_submodule_is_still_flagged_test_and_named`

#### `dup-0730` (exact, 3 sites)

Proposed home: `simplification_audit::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:6694-6697` `a_free_function_with_no_pub_keyword_is_private`
- `tests/simplification_audit.rs:6707-6710` `a_pub_crate_function_keeps_the_qualifier`
- `tests/simplification_audit.rs:6713-6716` `a_pub_super_function_keeps_the_qualifier`

#### `dup-0731` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:6756-6760` `a_cfg_test_out_of_line_mod_is_flagged_test`
- `tests/simplification_audit.rs:6777-6782` `an_out_of_line_mod_inherits_test_ness_from_an_enclosing_cfg_test_mod`

#### `dup-0732` (near, 3 sites)

Proposed home: `simplification_audit::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:6846-6852` `a_method_is_classified_under_its_impl_self_type`
- `tests/simplification_audit.rs:6855-6864` `a_method_on_a_generic_impl_block_strips_the_impls_own_leading_generics`
- `tests/simplification_audit.rs:6944-6949` `a_trait_impl_method_is_classified_under_the_implementing_type_not_the_trait`

#### `dup-0733` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:7005-7037` `section_1_names_every_module_and_every_unassigned_function`
- `tests/simplification_audit.rs:7040-7054` `section_1_reports_none_unassigned_explicitly_when_everything_is_assigned`

#### `dup-0734` (near, 3 sites)

Proposed home: `simplification_audit::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:7070-7076` `replace_section_1_only_touches_section_1_leaving_later_sections_intact`
- `tests/simplification_audit.rs:8376-8383` `replace_section_2_only_touches_section_2_leaving_neighbors_intact`
- `tests/simplification_audit.rs:9214-9231` `replace_section_6_only_touches_that_span_leaving_earlier_sections_intact`

#### `dup-0735` (near, 3 sites)

Proposed home: `simplification_audit::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:7120-7143` `responsibility_map_json_matches_the_tree_or_is_rewritten`
- `tests/simplification_audit.rs:8522-8549` `duplication_catalog_json_matches_the_tree_or_is_rewritten`
- `tests/simplification_audit.rs:10404-10431` `dead_code_json_matches_the_tree_or_is_rewritten`

#### `dup-0736` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:7182-7206` `a_pin_bump_leaves_the_guarded_responsibility_map_byte_identical`
- `tests/simplification_audit.rs:10496-10523` `a_pin_bump_leaves_the_guarded_dead_code_json_byte_identical`

#### `dup-0737` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:7232-7295` `two_branches_each_adding_an_unrelated_function_to_a_different_target_file_never_perturb_an_existing_responsibility_map_entry`
- `tests/simplification_audit.rs:10533-10611` `two_branches_each_adding_an_unrelated_function_to_a_different_file_never_perturb_an_existing_dead_code_entry`

#### `dup-0738` (exact, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:7512-7520` `string_and_raw_string_literals_are_one_lit_token_each`
- `tests/simplification_audit.rs:7532-7540` `number_literals_including_a_fraction_are_lit_tokens`

#### `dup-0739` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:7790-7816` `three_near_but_not_identical_functions_cluster_as_near_with_every_site_and_one_home`
- `tests/simplification_audit.rs:7819-7837` `cluster_ids_are_assigned_after_deterministic_sort_and_sites_are_sorted_within_a_cluster`

#### `dup-0740` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:7962-7975` `proc_stat_or_status_reader_sweep_finds_a_stat_reader_and_a_status_reader_but_not_an_unrelated_fn`
- `tests/simplification_audit.rs:8222-8239` `bespoke_lexer_sweep_finds_the_named_trio_but_not_an_unrelated_fn`

#### `dup-0741` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:7982-7995` `the_dash_reap_proc_stat_pair_the_spec_names_lands_in_one_real_cluster`
- `tests/simplification_audit.rs:8094-8109` `the_two_exploration_graph_fixture_builders_the_adversarial_sample_found_land_in_one_real_cluster`

#### `dup-0742` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:7998-8005` `constructs_own_type_literal_matches_self_and_the_named_type_but_not_an_unrelated_call`
- `tests/simplification_audit.rs:8008-8016` `constructs_own_type_literal_matches_shorthand_field_init_too`

#### `dup-0743` (exact, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:8042-8060` `the_spawn_result_constructor_triple_the_adversarial_sample_found_lands_in_one_real_cluster`
- `tests/simplification_audit.rs:8246-8263` `the_bespoke_lexer_and_canonical_extractor_the_lens_routed_land_in_one_real_cluster`

#### `dup-0744` (near, 3 sites)

Proposed home: `simplification_audit::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:8112-8140` `same_named_helper_sweep_excludes_required_trait_impl_methods_across_adapters`
- `tests/simplification_audit.rs:8143-8172` `same_named_helper_sweep_excludes_a_trait_default_method_and_its_override`
- `tests/simplification_audit.rs:8175-8199` `same_named_helper_sweep_still_catches_two_inherent_impls_sharing_a_method_name`

#### `dup-0745` (exact, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:8299-8322` `catalog_to_json_round_trips_through_deserialize`
- `tests/simplification_audit.rs:8325-8347` `catalog_lines_to_json_round_trips_and_carries_only_the_line_spans`

#### `dup-0746` (exact, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:8387-8392` `replace_section_2_panics_loudly_when_the_heading_is_entirely_absent`
- `tests/simplification_audit.rs:9244-9249` `replace_section_6_panics_loudly_when_the_heading_is_entirely_absent`

#### `dup-0747` (exact, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:8399-8403` `sample_indices_is_deterministic_for_a_fixed_seed`
- `tests/simplification_audit.rs:8429-8433` `different_seeds_produce_different_draws`

#### `dup-0748` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:8599-8626` `a_pin_bump_that_shifts_every_site_in_a_file_leaves_the_guarded_catalog_byte_identical`
- `tests/simplification_audit.rs:8636-8687` `two_branches_adding_an_unrelated_function_to_different_files_leave_the_guarded_catalog_unaffected`

#### `dup-0749` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:8871-8902` `report_section_2_matches_the_tree_or_is_rewritten`
- `tests/simplification_audit.rs:9064-9109` `report_sections_3_through_5_match_the_tree_or_are_rewritten`

#### `dup-0750` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:8910-8934` `replace_sections_3_to_5_only_touches_that_span_leaving_neighbors_intact`
- `tests/simplification_audit.rs:8937-8954` `replace_sections_3_to_5_falls_back_to_end_of_string_when_no_section_6_heading_exists`

#### `dup-0751` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:8999-9052` `assert_section_4_structurally_matches`
- `tests/simplification_audit.rs:9328-9384` `assert_section_6_structurally_matches`

#### `dup-0752` (near, 5 sites)

Proposed home: `simplification_audit::support (consolidate these 5 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:9457-9467` `resolves_a_same_name_dot_rs_target`
- `tests/simplification_audit.rs:9470-9483` `resolves_a_name_slash_mod_rs_target_when_the_flat_file_does_not_exist`
- `tests/simplification_audit.rs:9486-9502` `resolves_a_path_override_target`
- `tests/simplification_audit.rs:9505-9523` `the_real_eventstore_mod_rs_shape_resolves_contract_rs_as_test`
- `tests/simplification_audit.rs:9526-9547` `transitive_closure_pulls_in_a_second_hop_regardless_of_its_own_local_attribute`

#### `dup-0753` (near, 4 sites)

Proposed home: `simplification_audit::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:9632-9643` `resolvers_agree_on_a_same_name_dot_rs_target`
- `tests/simplification_audit.rs:9647-9658` `resolvers_agree_on_a_path_override_target`
- `tests/simplification_audit.rs:9662-9678` `resolvers_agree_on_a_transitive_second_hop`
- `tests/simplification_audit.rs:9682-9690` `resolvers_agree_on_a_non_test_out_of_line_mod`

#### `dup-0754` (near, 18 sites)

Proposed home: `simplification_audit::support (consolidate these 18 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:9715-9729` `a_fn_referenced_only_by_its_own_test_is_listed`
- `tests/simplification_audit.rs:9732-9746` `a_fn_referenced_from_a_production_caller_does_not_appear`
- `tests/simplification_audit.rs:9795-9807` `a_path_qualified_reference_with_no_call_parens_still_counts`
- `tests/simplification_audit.rs:9810-9820` `a_mention_inside_a_comment_does_not_count_as_a_reference`
- `tests/simplification_audit.rs:9832-9844` `recursion_through_the_fns_own_body_still_counts_as_a_reference`
- `tests/simplification_audit.rs:9847-9864` `a_whole_file_test_via_out_of_line_resolution_is_never_a_candidate`
- `tests/simplification_audit.rs:9884-9903` `a_named_use_import_at_mod_test_top_level_does_not_leak_as_a_production_reference`
- `tests/simplification_audit.rs:9906-9921` `a_serde_default_attribute_string_names_a_real_production_reference`
- `tests/simplification_audit.rs:10108-10121` `a_dot_call_through_an_inherent_method_on_any_receiver_counts_receiver_agnostically`
- `tests/simplification_audit.rs:10142-10157` `an_inherent_associated_function_is_matched_via_path_shape_not_dot_shape`
- `tests/simplification_audit.rs:10232-10246` `a_fn_passed_by_value_as_a_bare_call_argument_counts_as_a_reference`
- `tests/simplification_audit.rs:10263-10276` `a_fn_pointer_used_as_a_struct_literal_field_value_counts_as_a_reference`
- `tests/simplification_audit.rs:10279-10294` `a_ufcs_qualified_value_passed_to_a_combinator_counts_as_a_reference_for_a_method`
- `tests/simplification_audit.rs:10297-10307` `a_fn_named_by_a_let_initializer_counts_as_a_reference`
- `tests/simplification_audit.rs:10310-10321` `a_fn_named_as_an_array_element_counts_as_a_reference`
- `tests/simplification_audit.rs:10324-10335` `a_fn_named_in_a_match_arm_value_counts_as_a_reference`
- `tests/simplification_audit.rs:10338-10348` `a_fn_named_in_a_return_expression_counts_as_a_reference`
- `tests/simplification_audit.rs:10351-10364` `a_fn_named_as_a_generic_argument_to_another_type_counts_as_a_reference`

#### `dup-0755` (near, 4 sites)

Proposed home: `simplification_audit::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:9867-9875` `main_is_exempted_as_an_entry_point`
- `tests/simplification_audit.rs:9924-9940` `an_attribute_reference_to_an_ambiguous_shared_name_credits_every_sharer`
- `tests/simplification_audit.rs:10077-10092` `a_method_name_shared_by_two_impls_with_a_call_site_excludes_both`
- `tests/simplification_audit.rs:10124-10139` `a_trait_impl_method_is_exempted_even_with_zero_textual_call_sites`

#### `dup-0756` (near, 8 sites)

Proposed home: `simplification_audit::support (consolidate these 8 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:9943-9976` `a_free_fn_bare_name_collision_where_only_one_sharer_has_a_real_caller_flags_the_other`
- `tests/simplification_audit.rs:9979-10001` `a_bare_call_in_the_same_file_as_its_definition_attributes_locally_with_no_import_needed`
- `tests/simplification_audit.rs:10004-10030` `a_bare_unqualified_call_from_a_third_unrelated_file_credits_neither_sharer`
- `tests/simplification_audit.rs:10033-10059` `a_bare_call_resolved_through_a_use_import_attributes_to_the_imported_definition`
- `tests/simplification_audit.rs:10062-10074` `a_method_name_shared_by_two_impls_with_zero_calls_is_flagged_ambiguous`
- `tests/simplification_audit.rs:10160-10171` `an_inherent_associated_fn_name_shared_by_two_types_with_zero_calls_is_ambiguous`
- `tests/simplification_audit.rs:10174-10194` `a_qualified_call_site_attributes_only_to_the_sharer_it_names`
- `tests/simplification_audit.rs:10197-10229` `a_qualified_call_site_on_an_impls_own_generic_self_type_attributes_correctly`

#### `dup-0757` (near, 3 sites)

Proposed home: `spawn_scratch_reap_authorized_root_periphery::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/spawn_scratch_reap_authorized_root_periphery.rs:167-230` `rigger_result_reaps_a_live_process_in_the_spawns_registered_agent_scratch_dir`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:233-281` `rigger_result_reaps_a_live_process_in_the_spawns_registered_mutation_scratch_dir`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:415-487` `rigger_result_reaps_a_live_process_whose_registered_mutation_scratch_dir_was_already_removed_before_the_call`

#### `dup-0758` (near, 2 sites)

Proposed home: `spawn_timing_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/spawn_timing_periphery.rs:104-183` `spawn_timing_pairs_real_writer_events_through_a_real_store_by_role`
- `tests/spawn_timing_periphery.rs:280-335` `spawn_timing_excludes_a_real_same_batch_pair_as_suspect_not_a_silent_zero`

#### `dup-0759` (near, 42 sites)

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
- `tests/spec_lint.rs:1172-1201` `validate_still_flags_an_unsatisfied_either_or_as_an_open_hedge`
- `tests/spec_lint.rs:1217-1248` `validate_still_flags_a_genuine_hedge_after_an_earlier_non_disjunctive_either_on_specs_68_shape`
- `tests/spec_lint.rs:1256-1277` `validate_exempts_the_comma_separated_decided_disposition`
- `tests/spec_lint.rs:1284-1305` `validate_still_flags_a_spaced_negation_before_the_decided_idiom`
- `tests/spec_lint.rs:1312-1333` `validate_flags_a_hedge_split_across_hard_wrapped_lines`
- `tests/spec_lint.rs:1345-1368` `validate_does_not_fuse_a_hedge_across_a_heading_boundary`
- `tests/spec_lint.rs:1375-1398` `validate_does_not_fuse_a_hedge_across_a_table_row_boundary`
- `tests/spec_lint.rs:1412-1439` `validate_ignores_a_double_quoted_span_that_crosses_a_hard_wrapped_line`
- `tests/spec_lint.rs:1449-1475` `validate_ignores_a_backtick_span_that_crosses_a_hard_wrapped_line`
- `tests/spec_lint.rs:1490-1515` `validate_still_flags_a_smell_outside_a_balanced_quote_pair`
- `tests/spec_lint.rs:1523-1548` `validate_still_flags_a_smell_outside_a_balanced_backtick_pair`
- `tests/spec_lint.rs:1558-1585` `validate_fails_closed_after_a_stray_unmatched_quote_earlier_in_the_paragraph`
- `tests/spec_lint.rs:1592-1618` `validate_fails_closed_after_a_stray_unmatched_backtick_earlier_in_the_paragraph`
- `tests/spec_lint.rs:1634-1661` `validate_a_stray_unmatched_quote_does_not_unmask_a_later_real_quoted_disposition_phrase`
- `tests/spec_lint.rs:1681-1707` `validate_ignores_a_backtick_span_whose_closing_mark_is_immediately_after_a_digit`
- `tests/spec_lint.rs:1724-1751` `validate_ignores_a_quoted_span_whose_closing_quote_is_immediately_after_a_digit`
- `tests/spec_lint.rs:1761-1787` `validate_a_digit_adjacent_quote_stays_excluded_as_an_opener`
- `tests/spec_lint.rs:1803-1829` `validate_a_quote_at_the_very_start_of_a_paragraph_is_a_valid_opener`
- `tests/spec_lint.rs:1856-1885` `validate_an_embedded_digit_adjacent_mark_does_not_prematurely_close_a_real_quoted_span`
- `tests/spec_lint.rs:1919-1953` `validate_all_four_digit_adjacency_shapes_together_never_false_positive`
- `tests/spec_lint.rs:1981-2008` `validate_a_digit_glued_to_a_quotes_own_opening_mark_still_masks_the_real_span`

#### `dup-0760` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/step_attention_periphery.rs, tests/workflow_driver_resolved_model_periphery.rs, tests/worktree_liveness_fence_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/step_attention_periphery.rs:464-484` `temp_git_project_with_commit`
- `tests/workflow_driver_resolved_model_periphery.rs:57-78` `temp_git_project_with_commit`
- `tests/worktree_liveness_fence_periphery.rs:94-115` `temp_git_project_with_commit`

#### `dup-0761` (near, 2 sites)

Proposed home: `step_sheds_the_freshen::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/step_sheds_the_freshen.rs:128-163` `fingerprint_subtree`
- `tests/step_sheds_the_freshen.rs:129-159` `walk`

#### `dup-0762` (near, 2 sites)

Proposed home: `store_config::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/store_config.rs:63-75` `a_present_store_block_deserializes_backend_and_url`
- `tests/store_config.rs:92-110` `unrelated_workflow_keys_are_ignored_by_the_lightweight_probe`

#### `dup-0763` (exact, 2 sites)

Proposed home: `store_config::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/store_config.rs:78-89` `a_workflow_without_a_store_key_reads_as_the_default`
- `tests/store_config.rs:149-160` `an_empty_store_block_and_empty_values_are_no_opinion`

#### `dup-0764` (semantic, 4 sites)

Proposed home: `store_content_identity_periphery::port_double - one canonical constructor, the rest thin variants over it (or a builder), rather than each re-listing every field`

mandatory sweep: parallel constructor functions - 4 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/store_content_identity_periphery.rs:139-147` `new`
- `tests/store_content_identity_periphery.rs:152-160` `miscounting`
- `tests/store_content_identity_periphery.rs:164-166` `over_an_empty_stream`
- `tests/store_content_identity_periphery.rs:170-178` `over_a_stream`

#### `dup-0765` (exact, 2 sites)

Proposed home: `store_content_identity_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/store_content_identity_periphery.rs:1508-1510` `detached_subject`
- `tests/store_content_identity_periphery.rs:1520-1522` `mid_character`

#### `dup-0766` (near, 2 sites)

Proposed home: `store_content_identity_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/store_content_identity_periphery.rs:1512-1514` `past_the_end`
- `tests/store_content_identity_periphery.rs:1516-1518` `inverted`

#### `dup-0767` (semantic, 4 sites)

Proposed home: `one shared `local_event_log` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 4 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/store_flag_precedence.rs:94-96` `local_event_log`
- `tests/store_precedence.rs:59-61` `local_event_log`
- `tests/store_resolution_cli.rs:66-68` `local_event_log`
- `tests/store_secrets.rs:62-64` `local_event_log`

#### `dup-0768` (near, 4 sites)

Proposed home: `a new shared module (sites span 3 files: tests/store_flag_precedence.rs, tests/store_precedence.rs, tests/store_secrets.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/store_flag_precedence.rs:125-145` `assert_selected_server`
- `tests/store_precedence.rs:119-139` `assert_selected_server`
- `tests/store_precedence.rs:143-159` `assert_selected_sqlite`
- `tests/store_secrets.rs:106-144` `assert_server_reached_and_credentials_redacted`

#### `dup-0769` (semantic, 2 sites)

Proposed home: `one shared `assert_selected_server` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/store_flag_precedence.rs:125-145` `assert_selected_server`
- `tests/store_precedence.rs:119-139` `assert_selected_server`

#### `dup-0770` (exact, 2 sites)

Proposed home: `store_flag_precedence::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/store_flag_precedence.rs:148-165` `run_bare_conn_flag_selects_the_server_never_dropped_to_sqlite`
- `tests/store_flag_precedence.rs:168-183` `run_conn_flag_beats_a_committed_sqlite_store_config`

#### `dup-0771` (semantic, 3 sites)

Proposed home: `one shared `empty_project` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/store_precedence.rs:47-55` `empty_project`
- `tests/store_resolution_cli.rs:54-62` `empty_project`
- `tests/store_secrets.rs:50-58` `empty_project`

#### `dup-0772` (semantic, 2 sites)

Proposed home: `one shared `write_store_conn` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/store_precedence.rs:64-66` `write_store_conn`
- `tests/store_secrets.rs:70-79` `write_store_conn`

#### `dup-0773` (near, 3 sites)

Proposed home: `store_precedence::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/store_precedence.rs:217-252` `a_present_but_unreadable_store_conn_surfaces_loudly_not_a_silent_sqlite_fallback`
- `tests/store_precedence.rs:269-312` `an_unknown_committed_backend_surfaces_loudly_not_a_silent_sqlite_fallback`
- `tests/store_precedence.rs:315-355` `a_committed_kurrentdb_backend_with_no_credential_names_all_three_sources`

#### `dup-0774` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/store_resolution_cli.rs, tests/store_secrets.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/store_resolution_cli.rs:78-90` `run_bare_result`
- `tests/store_secrets.rs:88-100` `run_bare_result`

#### `dup-0775` (semantic, 2 sites)

Proposed home: `one shared `run_bare_result` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/store_resolution_cli.rs:78-90` `run_bare_result`
- `tests/store_secrets.rs:88-100` `run_bare_result`

#### `dup-0776` (near, 3 sites)

Proposed home: `store_resolution_cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/store_resolution_cli.rs:93-127` `a_server_selected_courier_reaches_the_server_and_never_fabricates_local_sqlite`
- `tests/store_resolution_cli.rs:130-157` `a_courier_with_no_server_configured_resolves_the_local_sqlite_log`
- `tests/store_resolution_cli.rs:160-184` `an_empty_kurrentdb_conn_is_treated_as_unset_not_a_server_with_no_address`

#### `dup-0777` (exact, 2 sites)

Proposed home: `store_resolution_cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/store_resolution_cli.rs:278-285` `prime_resolves_the_configured_server_never_the_local_absent_sentinel`
- `tests/store_resolution_cli.rs:288-297` `stats_resolves_the_configured_server_never_the_local_absent_sentinel`

#### `dup-0778` (exact, 2 sites)

Proposed home: `store_secrets_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/store_secrets_periphery.rs:52-59` `redact_conn_is_a_public_symbol_that_scrubs_userinfo_but_keeps_scheme_host_and_query`
- `tests/store_secrets_periphery.rs:248-256` `redact_conn_scrubs_the_credential_but_keeps_a_benign_at_sign_later_in_the_same_url`

#### `dup-0779` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/subject_lens_defined_cells.rs, tests/subject_lens_defined_cells_contract.rs, tests/subject_lens_reprojection_contract.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/subject_lens_defined_cells.rs:60-70` `node`
- `tests/subject_lens_defined_cells_contract.rs:48-58` `node`
- `tests/subject_lens_reprojection_contract.rs:63-73` `node`

#### `dup-0780` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/subject_lens_defined_cells.rs, tests/subject_lens_defined_cells_contract.rs, tests/subject_lens_reprojection_contract.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/subject_lens_defined_cells.rs:73-83` `edge`
- `tests/subject_lens_defined_cells_contract.rs:61-71` `edge`
- `tests/subject_lens_reprojection_contract.rs:88-98` `edge`

#### `dup-0781` (exact, 7 sites)

Proposed home: `a new shared module (sites span 4 files: tests/subject_lens_defined_cells.rs, tests/subject_lens_defined_cells_contract.rs, tests/subject_lens_reprojection_contract.rs, tests/subject_lens_reprojection_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/subject_lens_defined_cells.rs:85-87` `code_lens`
- `tests/subject_lens_defined_cells.rs:89-91` `concepts_lens`
- `tests/subject_lens_defined_cells_contract.rs:73-75` `code_lens`
- `tests/subject_lens_defined_cells_contract.rs:77-79` `concepts_lens`
- `tests/subject_lens_reprojection_contract.rs:101-103` `concepts_lens`
- `tests/subject_lens_reprojection_contract.rs:106-108` `code_lens`
- `tests/subject_lens_reprojection_periphery.rs:315-317` `code_lens`

#### `dup-0782` (semantic, 3 sites)

Proposed home: `one shared `concepts_lens` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/subject_lens_defined_cells.rs:89-91` `concepts_lens`
- `tests/subject_lens_defined_cells_contract.rs:77-79` `concepts_lens`
- `tests/subject_lens_reprojection_contract.rs:101-103` `concepts_lens`

#### `dup-0783` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/subject_lens_defined_cells.rs, tests/subject_lens_defined_cells_contract.rs, tests/subject_lens_reprojection_contract.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/subject_lens_defined_cells.rs:104-121` `served_json`
- `tests/subject_lens_defined_cells_contract.rs:87-104` `served_json`
- `tests/subject_lens_reprojection_contract.rs:122-142` `served_json`

#### `dup-0784` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: tests/subject_lens_defined_cells.rs, tests/subject_lens_reprojection_contract.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/subject_lens_defined_cells.rs:133-149` `communities_no_concepts_graph`
- `tests/subject_lens_defined_cells.rs:255-272` `mixed_membership_graph`
- `tests/subject_lens_reprojection_contract.rs:380-401` `file_over_code_graph`

#### `dup-0785` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/subject_lens_defined_cells.rs, tests/subject_lens_defined_cells_contract.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/subject_lens_defined_cells.rs:510-530` `shared_member_graph`
- `tests/subject_lens_defined_cells_contract.rs:238-256` `shared_member_graph`

#### `dup-0786` (semantic, 2 sites)

Proposed home: `one shared `shared_member_graph` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/subject_lens_defined_cells.rs:510-530` `shared_member_graph`
- `tests/subject_lens_defined_cells_contract.rs:238-256` `shared_member_graph`

#### `dup-0787` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/subject_lens_defined_cells_contract.rs, tests/subject_lens_reprojection_contract.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/subject_lens_defined_cells_contract.rs:318-334` `full_in_budget_graph`
- `tests/subject_lens_reprojection_contract.rs:154-177` `community_over_concepts_graph`

#### `dup-0788` (near, 3 sites)

Proposed home: `subject_lens_reprojection_contract::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/subject_lens_reprojection_contract.rs:250-284` `reprojection_admits_a_realizing_member_of_any_kind_under_the_concepts_lens`
- `tests/subject_lens_reprojection_contract.rs:493-526` `reprojection_excludes_a_non_code_entity_member_entirely_under_the_code_lens`
- `tests/subject_lens_reprojection_contract.rs:543-573` `reprojection_excludes_a_decision_member_even_when_it_carries_a_live_community_membership`

#### `dup-0789` (near, 2 sites)

Proposed home: `subject_lens_reprojection_contract::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/subject_lens_reprojection_contract.rs:297-328` `reprojection_carries_empty_state_when_no_member_realizes_any_concept_under_the_concepts_lens`
- `tests/subject_lens_reprojection_contract.rs:590-621` `reprojection_carries_empty_state_when_the_sole_realizer_is_purity_excluded`

#### `dup-0790` (near, 7 sites)

Proposed home: `unified_traversal_grounding::support (consolidate these 7 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/unified_traversal_grounding.rs:306-341` `a_spawn_prompt_carries_the_unified_traversal_code_neighborhood_not_the_old_structural_stitch`
- `tests/unified_traversal_grounding.rs:359-463` `the_implement_prompt_is_trimmed_to_the_intent_layer_with_a_rigger_peers_pointer`
- `tests/unified_traversal_grounding.rs:477-517` `the_producer_prompt_keeps_the_full_grounding_context_not_the_implement_trim`
- `tests/unified_traversal_grounding.rs:688-792` `the_sdet_author_build_seam_spawn_receives_the_trimmed_implement_slice`
- `tests/unified_traversal_grounding.rs:1231-1361` `a_spawn_prompt_carries_the_design_intent_that_governs_the_touched_files_by_traversal`
- `tests/unified_traversal_grounding.rs:1450-1507` `a_governing_decision_never_leaks_into_the_spawn_prompt_design_intent_section`
- `tests/unified_traversal_grounding.rs:1584-1617` `a_spawn_prompt_with_no_governing_design_intent_renders_no_design_intent_header`

#### `dup-0791` (near, 2 sites)

Proposed home: `unified_traversal_grounding::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/unified_traversal_grounding.rs:1022-1065` `the_code_neighborhood_section_is_budget_capped_with_a_visible_elision_note`
- `tests/unified_traversal_grounding.rs:1084-1125` `the_spawn_prompt_code_neighborhood_elision_note_names_the_honest_graph_around_recovery`

#### `dup-0792` (near, 2 sites)

Proposed home: `unified_traversal_grounding::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/unified_traversal_grounding.rs:1383-1430` `the_design_intent_section_is_budget_capped_and_its_elision_note_names_the_honest_graph_around_recovery`
- `tests/unified_traversal_grounding.rs:1521-1565` `the_spawn_prompt_design_intent_section_renders_the_newest_binding_and_elides_the_oldest`

#### `dup-0793` (near, 2 sites)

Proposed home: `validate_advisories::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/validate_advisories.rs:179-204` `validate_warns_of_index_staleness_and_names_reindex`
- `tests/validate_advisories.rs:428-461` `validate_warns_of_graph_index_lag_and_names_reindex`

#### `dup-0794` (exact, 2 sites)

Proposed home: `validate_advisories::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/validate_advisories.rs:300-313` `validate_is_silent_on_log_bloat_when_every_key_is_recorded_once`
- `tests/validate_advisories.rs:316-344` `validate_is_silent_on_log_bloat_when_the_same_key_recurs_only_across_different_covered_types`

#### `dup-0795` (near, 2 sites)

Proposed home: `validate_advisories::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/validate_advisories.rs:464-481` `validate_is_silent_on_graph_index_lag_when_the_graph_matches_the_tree`
- `tests/validate_advisories.rs:484-499` `validate_is_silent_on_graph_index_lag_when_the_graph_has_recorded_nothing`

#### `dup-0796` (near, 3 sites)

Proposed home: `watchdog_cli_periphery::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/watchdog_cli_periphery.rs:132-182` `watch_once_reports_anomalies_through_the_real_compiled_binary_naming_signal_subject_and_response`
- `tests/watchdog_cli_periphery.rs:299-326` `watch_once_reports_a_store_integrity_anomaly_through_the_real_compiled_binary`
- `tests/watchdog_cli_periphery.rs:573-654` `watch_once_reports_the_criterions_own_multi_anomaly_scenario_through_the_real_compiled_binary`

#### `dup-0797` (near, 3 sites)

Proposed home: `watchdog_cli_periphery::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/watchdog_cli_periphery.rs:382-449` `watch_without_once_streams_and_re_polls_a_live_mutating_store_until_killed`
- `tests/watchdog_cli_periphery.rs:462-545` `watch_streaming_survives_a_transient_store_read_failure_and_recovers`
- `tests/watchdog_cli_periphery.rs:669-771` `watch_streaming_re_alerts_a_reject_recurrence_churn_count_on_each_increment`

#### `dup-0798` (near, 2 sites)

Proposed home: `worker_persona_label_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/worker_persona_label_periphery.rs:241-253` `a_gap_18_respawn_id_still_renders_the_full_persona_led_label`
- `tests/worker_persona_label_periphery.rs:337-348` `internal_whitespace_is_normalized_before_the_sentence_is_cut`

#### `dup-0799` (near, 2 sites)

Proposed home: `workflow_definition_and_js_constants_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/workflow_definition_and_js_constants_periphery.rs:153-186` `seed_workflow_yml`
- `tests/workflow_definition_and_js_constants_periphery.rs:194-209` `seed_js_files`

#### `dup-0800` (near, 2 sites)

Proposed home: `workflow_driver_resolved_model_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/workflow_driver_resolved_model_periphery.rs:163-319` `workflow_driven_rigger_result_meta_resolved_model_reaches_the_persisted_green_event`
- `tests/workflow_driver_resolved_model_periphery.rs:334-473` `workflow_driven_rigger_result_with_no_meta_omits_the_resolved_model_key_and_ignores_a_prose_claim`

### Adversarial sample

Recall check (spec 85 THOROUGHNESS): 30 functions drawn by seeded random index (seed `85072026`, `sample_indices` over all 6902 functions scanned in `src/` and `tests/`, excluding `tests/prioritized_plan_citation_periphery.rs` - criterion 4's own citation-guard periphery test, whose function count grows as its citation-drift-guard mechanism hardens round over round; excluding it keeps that unrelated growth from ever reshuffling this already-verified draw), each read by hand - together with its host file's surrounding context, since a duplicate can live anywhere in the file or a sibling file - to judge whether a duplicate exists that the mechanical pass and the five sweeps above did not already catch.

- `src/community.rs:745-751` `every_community_is_internally_connected` - no duplicate found by reading
- `src/conductor.rs:15605-15670` `re_folding_a_same_id_re_emit_is_idempotent` - no duplicate found by reading
- `src/conductor.rs:27150-27232` `a_plan_landing_infra_fault_halts_the_run_loudly_with_no_per_unit_lesson_or_attempt` - no duplicate found by reading
- `src/conductor.rs:37801-37921` `conflict_regenerate_pending_from_log_re_derives_the_union_keyed_by_unit_and_attempt` - no duplicate found by reading
- `src/driver/replay.rs:219-221` `mutation_scratch_root` - no duplicate found by reading
- `src/driver/replay.rs:389-396` `opts_for` - no duplicate found by reading
- `src/liveness.rs:156-163` `marker_path` - no duplicate found by reading
- `src/main.rs:10340-10374` `workflow_drift_advisory` - no duplicate found by reading
- `src/main.rs:25761-25780` `the_watchdog_command_signal_set_covers_every_signal_the_watch_skill_names` - no duplicate found by reading
- `src/metrics.rs:509-511` `pct` - no duplicate found by reading
- `src/spec.rs:16-35` `extract_criteria` - no duplicate found by reading
- `src/worktree.rs:1444-1452` `scratch_root` - caught: `dup-0368`
- `src/worktree.rs:4974-4978` `requested_and_answered` - caught: `dup-0376`
- `tests/adaptive_labels_periphery.rs:450-462` `assert_ok` - no duplicate found by reading
- `tests/cli.rs:2293-2336` `a_dotdot_spawn_id_never_escapes_the_registered_scratch_roots` - caught: `dup-0456`
- `tests/cli.rs:4108-4158` `serve_from_a_linked_worktree_refuses_naming_both_trees` - caught: `dup-0461`
- `tests/cli.rs:5760-5781` `resume_unit_defaults_to_granting_one_attempt_when_attempts_is_omitted` - no duplicate found by reading
- `tests/code_ingest_events.rs:906-913` `apply_ref_json` - no duplicate found by reading
- `tests/community_detection_cli.rs:375-411` `a_malformed_resolution_or_unknown_argument_fails_loudly` - caught: `dup-0561`
- `tests/concepts_fold_periphery.rs:363-395` `a_fresh_rerun_of_one_grain_leaves_every_other_resolution_grain_untouched` - no duplicate found by reading
- `tests/dash_kg_graph_route.rs:665-733` `the_served_graph_route_flags_god_nodes_and_returns_the_query_path` - no duplicate found by reading
- `tests/dash_run_tree_spine.rs:909-975` `multi_attempt_and_gap18_retry_spawns_render_as_distinct_sibling_agents` - no duplicate found by reading
- `tests/files_lens_view_periphery.rs:518-533` `a_files_contained_entities_are_still_reachable_via_neighborhood_the_cards_own_seam` - no duplicate found by reading
- `tests/graph_denoise_target_project.rs:271-360` `disposition_expiry_still_fires_on_integration_with_no_unit_node` - no duplicate found by reading
- `tests/graph_fresh_on_integration_periphery.rs:56-73` `init_repo` - caught: `dup-0107`
- `tests/hermetic_test_git_audit.rs:236-250` `write_hostile_global_config` - no duplicate found by reading
- `tests/plan_stage_commit_landing_periphery.rs:1097-1166` `plan_stage_commit_reverting_its_own_out_of_scope_touch_still_fails_the_stage_at_the_periphery` - no duplicate found by reading
- `tests/reset_derived_compaction_periphery.rs:452-522` `a_prune_whose_policy_never_declared_the_valid_time_partition_is_refused_untouched` - no duplicate found by reading
- `tests/simplification_audit.rs:7305-7346` `assert_section_1_structurally_matches` - no duplicate found by reading
- `tests/simplification_audit.rs:10310-10321` `a_fn_named_as_an_array_element_counts_as_a_reference` - caught: `dup-0754`

Two real recall gaps surfaced this way and were closed by widening the mechanical sweep with a new generalizable detector each - not a one-off citation - so the fix catches every present and future instance of its class, each pinned by a real-tree regression test: `find_proc_stat_or_status_readers` (decision `u85c2-proc-stat-worked-example`) groups every function reading a `/proc/<pid>/stat` or `/proc/<pid>/status` literal, closing the spec's own named worked example - `src/dash.rs:499-507` `process_state` next to `src/reap.rs:190-197` `pid_starttime`, the same job on the same file with a different field/shape, upheld at spec 62's capstone; `find_parallel_constructor_clusters` (decision `u85c2-parallel-constructor-sweep`) groups 2+ non-test functions per `(file, Self type)` that build a `Self { .. }` / `TypeName { .. }` literal, closing `src/spawn.rs`'s `SpawnResult::liveness_fault` reading MISSING from its own `ok`/`failed` cluster even though all three are parallel constructors for one struct. A third worked example, `exploration_graph` (independently defined test-fixture builders in `tests/dash_exploration_route_client_contract.rs` and `tests/dash_kg_graph_route.rs`), was already caught correctly by the plain Jaccard pass with no sweep needed - confirming the mechanical pass itself has real recall, not only the two widened sweeps. Two further real defects, found on review rather than in this draw, were closed the same way: a RECALL gap the architecture lens routed to this criterion by name across two prior review rounds - this file's own bespoke source-text lexer (`scan_file`/`tokenize`) duplicating the codebase's ONE canonical tree-sitter extractor, `src/grounder/symbols/extract.rs::extract` (its own module doc's claim, architecture 5.5.3) - closed by `find_bespoke_lexer_vs_canonical_extractor` (decision `u85c2-bespoke-lexer-sweep`), a fourth generalizable sweep; and a PRECISION defect the adversary found by reading every `same-named helper` cluster against `ScannedFn::enclosing_impl` - `find_same_named_helper_functions` was misclassifying REQUIRED trait-impl methods as coincidental duplication (`subscribe_all`/`subscribe_stream` across the `EventStore` trait's three backend adapters plus a test double, `blast_radius` across the `Grounder` trait's own default method, its override, and a test double) - closed by excluding members whose extracted Self type differs across the group when at least one comes from an actual `" for "` trait impl (decision `u85c2-same-named-helper-trait-impl-precision-fix`), mirroring `find_parallel_constructor_clusters`'s own `(file, Self type)` keying one function away. Round 7 (decision `u85c4-r7-exclude-periphery-file-from-adversarial-population`) excluded this criterion's own citation-guard periphery file from the draw's population (see this subsection's opening paragraph) and redrew the sample; every one of the 19 functions above marked "no duplicate found by reading" was re-read by hand against its host file's surrounding context, exactly as this THOROUGHNESS check requires whenever the draw changes. 18 of the 19 are genuinely not duplicates; `apply` at `src/conductor.rs:29832-29834` is one shape worth naming so it is not mistaken for a miss - a `Projection` test double's own required trait-impl body, the same port-default/adapter-override/test-double shape `find_same_named_helper_functions`'s trait-impl-precision fix (decision `u85c2-same-named-helper-trait-impl-precision-fix`) already excludes from clustering by design, confirmed to still hold for this draw's own instance of it. The 19th is a genuine small duplicate this catalog's `fn`-only scanner (module doc, THE SCANNER) structurally cannot represent as a cluster: `gate_verdict_event` (`src/conductor.rs:29191-29200`) and the `verdict` closure inside `integrating_a_unit_stales_the_intersecting_downstream_units_cached_verdict_not_the_rest` (`src/conductor.rs:30596-30605`) do the identical job - find the recorded `GateVerdict` for a `"<unit>/gate:g#<attempt>"` replay key, panicking with the same message when none exists - differing only in whether the unit segment is the literal `"s"` or a parameter. A `let`-bound closure is not a `fn` item, so no change to this scanner short of teaching it to see closures could catalog this pair as a cluster; named here, prominently, rather than silently, so a later refactor - or a scanner that learns to see closures - does not miss it.


## 3. Boundary Violations

Instrument: for each of the five named ports (`eventstore::EventStore`, `contextgraph::Projection`, `conductor::AgentDriver`, `gate::Runner`, `grounder::Grounder`), grepped every production (pre-`#[cfg(test)]`) call site of that port's known concrete adapter modules from a NON-adapter, NON-composition-root file, and separately grepped every domain-ish file's top-level `use` statements for a direct infrastructure-crate import. `src/main.rs` is exempt from the "reaches a concrete adapter" check: it is the composition root, and wiring concretions together is its designed job.

FOUND, two violations:

Violation 1 (`AgentDriver`): `src/conductor.rs:7091-7096` (`reclaim_terminal_unit_mutation_scratch`, real production code - well above the `#[cfg(test)] mod tests` boundary this audit's own section 1 identified at `src/conductor.rs:10260`) calls `crate::driver::replay::cache_home_from` and `crate::driver::replay::reclaim_unit_mutation_scratch` directly by concrete module path. Read via `rigger graph --show AgentDriver`: the port `conductor.rs` actually depends on for driving agents is `trait AgentDriver { fn spawn(&self, agent: &AgentDef, prompt: &str, opts: &SpawnOpts, emit: &dyn Fn(&str, Value) -> Result<(), Error>) -> Result<AgentResult, Error>; }` (`src/conductor.rs:1083-1091`) - one method, `spawn`. Neither called function is about driving an agent or replaying a recorded run (the concern `driver::replay` otherwise owns); both are pure, driver-instance-free scratch-lifecycle utilities that happen to live inside that one concrete adapter's module. The port that should have been used: none exists for this concern yet, which is itself the defect - `conductor.rs` (a use-case/orchestration file) should not need to know which concrete `AgentDriver` implementation happens to define its own mutation-scratch cache-home resolution. Fix direction for a follow-up spec: relocate `cache_home_from` and `reclaim_unit_mutation_scratch` out of `driver::replay` into a neutral, adapter-independent module (a `scratch` or `mutation` support module conductor.rs and every driver adapter can depend on alike), so no use-case file reaches into one specific adapter's internals for a concern that adapter does not conceptually own.

Violation 2 (`Grounder`): `src/ingest.rs:187-211` (`walk_batches`, called from production `conductor::RunCtx::ingest_project_batches` at `src/conductor.rs:7903`, itself called from `src/conductor.rs:7894` well above the `10260` `#[cfg(test)]` boundary) calls `crate::grounder::symbols::events::project_batches_paced` directly by concrete module path at line 197 to reuse the `symbols` grounder's already-persisted index for a one-time whole-project ingest walk, then at line 203 - same function, same missing-port defect, not a separate third violation - calls `crate::grounder::design::events::project_batches` directly by concrete module path for the design-doc half of the same walk. These two calls are two of the three named sites of section 2's own catalogued duplicate cluster (`dup-0218`: `src/grounder/design/events.rs:90-114`, `src/grounder/symbols/events.rs:89-91`, and this diff's own new third site, `src/grounder/workflowdef.rs:242-249` - all three named `project_batches`), so this boundary violation and that duplication finding are two symptoms of one root cause - `ingest.rs` naming each concrete grounder submodule because no port exposes either. The `Grounder` port's own methods (`ground`, `reindex`, `blast_radius`, `index_stamp` - its provenance stamp - all at `src/grounder/mod.rs:133-175`) serve real-time per-query grounding of an agent's prompt; none exposes "hand me every indexed file's projected events for a whole-project batch ingest," so `ingest.rs` - itself a domain ingest authority (its own module doc names it "the ONE walk-and-content-key authority"), not an adapter and not the composition root - has no port to depend on for either call and reaches the concrete `symbols` module (197) and the concrete `design` module (203) directly. Same missing-port defect class as violation 1. Fix direction for a follow-up spec: add an ingest-shaped port method (e.g. a `Grounder::project_batches` or a standalone `SymbolProjector` trait) covering both concrete modules, so `ingest.rs` depends on one abstraction instead of either concrete grounder module for its whole-project walk.

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
| `src/ingest.rs` | 3 |
| `src/ledger.rs` | 2 |
| `src/spawn.rs` | 9 |
| `src/worktree.rs` | 1 |
| **Total** | **26** |

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

- **definitions_named** (`src/grounder/symbols/model.rs:216`, `pub`, KG degree 7): `delete`. definitions_named has no production caller - only its own module's assertion-style tests (symbols/mod.rs) use it to check index state after a build/update, never a production edge-resolution path.
- **references_named** (`src/grounder/symbols/model.rs:226`, `pub`, KG degree 10): `delete`. references_named has no production caller - the same test-only accessor shape as its sibling definitions_named immediately above it.

**`src/ingest.rs`**

- **ingest_project** (`src/ingest.rs:126`, `pub` (ambiguous with src/ingest.rs:628), KG degree 3): `delete`. ingest_project - both the #[cfg(feature = "symbols")] single-event lane and the #[cfg(not(feature = "symbols"))] light-lane no-op, one name at two cfg-gated sites - has no production caller on either lane. Production calls ingest_project_batched exclusively (conductor.rs, main.rs), the batched entry point this fn's own doc comment already names as the thing 'existing callers discard [IngestStats] and are unaffected' by, i.e. it documents its own supersession.
- **ingest_project** (`src/ingest.rs:628`, `pub` (ambiguous with src/ingest.rs:126), KG degree 3): `delete`. ingest_project - both the #[cfg(feature = "symbols")] single-event lane and the #[cfg(not(feature = "symbols"))] light-lane no-op, one name at two cfg-gated sites - has no production caller on either lane. Production calls ingest_project_batched exclusively (conductor.rs, main.rs), the batched entry point this fn's own doc comment already names as the thing 'existing callers discard [IngestStats] and are unaffected' by, i.e. it documents its own supersession.
- **record_current_generation** (`src/ingest.rs:994`, `private`, KG degree 3): `delete`. record_current_generation (spec 92 criterion 1, FRESH ON EVERY INTEGRATION) has no production caller - a private fixture helper inside scoped_reindex_tests that appends `graph_index_lag`/`graph_index_lag_sample`'s test-double `prior: Vec<Event>` stream, factored out once graph_index_lag_sample_derives_its_candidates_from_..., graph_index_lag_sample_reports_a_file_that_changed_..., and graph_index_lag_sample_is_bounded_and_stays_silent_... all needed the identical stamping boilerplate (the same shape already inlined once in graph_index_lag_reports_a_changed_file_and_not_an_unchanged_one, its own sibling test above it in this file). Never called outside this test module.

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

- **is_dirty** (`src/worktree.rs:679`, `pub`, KG degree 5): `delete`. is_dirty has no production caller - one of spec 87's own two Goal-cited worked examples ('src/worktree.rs expect_merged and is_dirty'), reconfirmed on the current tree: its 3 references (src/conductor.rs and worktree.rs's own `mod tests`) are all test-only; its own body now delegates to `path_is_dirty` (spec 89 round 3), but that internal call is not a caller of `is_dirty` itself. Line shifted again, this time by spec 89 criterion 1's round 3 fix (`arch-u89c1r2-dirty-check-duplicated-and-diverges-fail-direction`): `is_dirty`'s body was replaced with a one-line delegation to the new shared `path_is_dirty` free fn (built on the pre-existing `git`/`run_git` primitives, and now also called directly by `sweep_terminal_logged` and `main.rs`'s `reclaim_orphan_scratch` so the two no longer risk diverging on how a git-status failure is read), and the doc comment naming that delegation pushes the line down 8 more, 627->635. expect_merged itself (formerly src/worktree.rs:86) is no longer a candidate at all: round 4 moved it, together with `IntegrateOutcome` and the pre-round-4 `integrate` method, into this file's own `#[cfg(test)] mod tests` (a test-only recomposition of the newly-split `merge_into_worktree`/`land`, since production - `integrate_and_emit` - now calls those two split methods directly for its own row-level durable recording and has no caller left for the combined form) - a test-scoped item is not a production dead-code candidate by this scanner's own definition, closing the finding at its root rather than re-dispositioning it in place.

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

- `page_script` - a small JS snippet fixture - independently redefined in 19 different files (`dup-0350`, exact; e.g. `tests/adaptive_labels_periphery.rs:52-61`, `tests/code_lens_overview_collapse_viz.rs:26-35`, `tests/concepts_lens_view_periphery.rs:689-698`, + 16 more), all inside the Dashboard/viz subsystem (5.1) - cross-validates that grouping.
- `node_available` - a viz-fixture predicate - independently redefined in 19 files; the mechanical pass also clusters it together with the `gitsemver_available`/ `npm_available` availability-check helpers (5 more sites across `src/main.rs` and three test files) into one 24-site cluster (`dup-0248`, exact).
- `temp_project` - a scratch-project-directory fixture - independently redefined in 19 files (`dup-0376`, semantic; e.g. `tests/canary_model_drift_periphery.rs:39-46`, `tests/cause_wire_periphery.rs:54-61`, `tests/cli.rs:19-29`), plus a near-identical 12-site variant (`dup-0375`) and a 13-site `run_rigger` companion helper that drives it (`dup-0377`).
- `run_stream_identity` - a store-identity fixture - independently redefined in 18 files (`dup-0382`, semantic).

Proposed home for all four: `tests/common` (the catalog's own `proposed_home` field already says so verbatim for each). Consolidating just these four collapses roughly 72 duplicate definitions into 4 shared ones - the single largest mechanical simplification this audit identifies anywhere in the test suite.

### 5.3 `tests/cli.rs` split plan

27,074 lines, 351 `#[test]` functions, 15 pre-existing internal section markers in the file: 5 full box-style banner-comment pairs (`tests/cli.rs:11256/11258`, `11816/11818`, `11914/11916`, `12372/12374`, `20514/20527`) plus 10 single-line `// --- Spec NN, criterion M` headers (`21054`, `21972`, `22066`, `23403`, `25425`, `25548`, `25632`, `25770`, `25983`, `26418`). So the file carries some existing, ad hoc organization - each single-line header names the spec and criterion whose tests follow it, not a CLI subcommand or subsystem - rather than the "genuinely flat, not internally organized" state a first read might suggest; 15 markers spread across 351 tests still fall well short of a deliberate, complete per-surface structure. This correction does not disturb the split proposed below: it replaces the file's existing ad hoc, by-spec markers with a complete, deliberate BY CLI SUBCOMMAND SURFACE organization instead. A keyword-on-test-name pass (matching each test's dominant CLI verb: `step_`, `run_`, `validate_`, `reset_`, `watch_`/`watchdog_`, `canary_`, `dash_`/`status_`, `store_`/`eventstore_`, `spawn_`/`mutation_scratch_`/`scratch_`, `review_`/`gate_`, `setup_`/`precommit_`/`hook_`, `courier_`/`registry_`, `spec_`, `replay_`, `worktree_`, `emit_`/`peers_`/`decision_`, `stats_`, `heartbeat_`/`liveness_`, `prime_`/`version_`/`init_`) only cleanly covers 274 of the 351 tests (78%) - disclosed honestly rather than overclaimed, because a real fraction of `cli.rs`'s scenarios are DELIBERATELY end-to-end (a single test legitimately drives `step` + `run` + `validate` + `dash` together to prove a cross-cutting property, e.g. `a_run_driver_auto_starts_a_reachable_dash_with_a_url_shown_in_status` or `docs_ships_graph_hygiene_guidance_to_consumers`), which a bare keyword match cannot and should not force into one bucket. The proposed split is BY CLI SUBCOMMAND SURFACE - `cli.rs`'s own natural organizing concept, since the whole file drives the `rigger` binary end to end - into per-surface files (`tests/cli_step.rs`, `tests/cli_run.rs`, `tests/cli_validate.rs`, `tests/cli_reset.rs`, `tests/cli_watch.rs`, `tests/cli_canary.rs`, `tests/cli_dash.rs`, `tests/cli_store.rs`, `tests/cli_review.rs`, `tests/cli_setup.rs`, plus a residual `tests/cli_misc.rs` for the genuinely cross-cutting scenarios), with each test's home decided by its DOMINANT scenario on a human/AI read, not a mechanical keyword match - the same discipline this audit's own responsibility map applied to unassignable functions (named, never silently forced). Cross-referencing the catalog: `cli.rs` also participates in 29 of the catalog's cross-file test-duplication clusters (the most of any single file), several paired against files that WOULD merge with it under this split (`tests/step_attention_periphery.rs`, paired in 6 clusters; `tests/watchdog_cli_periphery.rs`, paired in 2 clusters) - the split is expected to shrink, not grow, the duplication surface.

### 5.4 Duplicated helpers across test files (beyond 5.2's four headline cases)

181 test-only clusters in the committed catalog have every site as an ordinary (non-`#[test]`) helper function - the shared-fixture-extraction candidate class. Beyond the four in 5.2, the widest are: `dup-0354` (`architecture_text` / `eventstore_source` / `main_rs_source` - source-text-loading helpers for doc/architecture-integrity checks, 12 files, 15 sites); `dup-0381` (a companion, 16-file/16-site variant of 5.2's `run_stream_identity` fixture, alongside `dup-0382`'s 18-file version); `dup-0410` (`write_two_stage_workflow` / `write_budget_one_two_stage_workflow` / `write_standalone_review_workflow` - workflow-YAML-literal builders duplicated across `tests/cli.rs` and `tests/step_attention_periphery.rs`, 4 files, 15 sites); `dup-0381`/`dup-0382` (`seed_run_events`, an event-seeding helper, 6-8 files); `dup-0474` (`apply_def_json` / `apply_ref_fresh`-shaped fold-application helpers, 5 files); `dup-0479` (`community` / `concept` / `def`-named single-field constructor helpers, 6 files); `dup-0482` (`code_lens` / `concepts_lens` two-line accessor helpers, 3 files). Every one of these 181 clusters, with its full site list and the catalog's own `proposed_home`, is already machine-readable in the committed `docs/audit/duplication-catalog.json` for a follow-up consolidation spec to consume directly - not re-enumerated exhaustively here to keep this section a report, not a second copy of the catalog.

### 5.5 Table-driven test families

159 test-only clusters have every site as a `#[test]` function - a literal-differs-only-in-input family, spec 85's own named table-driven-test candidate class. The single largest anywhere in the suite: `dup-0667` (near, 42 sites, all in `tests/spec_lint.rs`, e.g. `validate_spec_reports_every_c3_defect_with_its_criterion_and_field_guide_class:54-102`, `validate_spec_attributes_a_prose_level_defect_to_no_criterion:120-163`, `validate_spec_reports_two_simultaneous_defects_on_the_same_criterion:172-209` - 42 near-identical "feed one spec fixture through `validate`, assert one expected defect/advisory line" bodies). Proposed table: `#[test] fn validate_spec_field_guide_defects() { for (fixture, expected) in CASES { ... } }` retiring all 42 named tests into one parametrized loop over a `(&str, &str)` (or richer struct) case table. Other large families: `dup-0608`/`dup-0610` (15+7 sites, `tests/reap_before_removal_audit.rs`, "one fixture function body, one exemption-coverage shape, assert covered/not-covered" - retires into one table keyed by exemption shape); `dup-0641` (11 sites, `tests/simplification_audit.rs` - this very unit's own scanner tests, a `(source, expected_tokens_or_clusters)` table candidate); `dup-0597`/`dup-0598` (11+4 sites, `tests/no_os_kill_audit.rs`, one process-termination-pattern-string per test - a `(pattern, is_caught)` table); `dup-0600` (4 sites, `tests/no_os_kill_test_helper_periphery.rs`, `terminate_pid_refuses_pid_zero` / `_pid_one` x `stop_pid_refuses_pid_zero` / `_pid_one` - a 2x2 `(helper, pid)` table). As with 5.4, the full 157-family list lives in the committed catalog by cluster id for a follow-up test-consolidation spec to consume directly.

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
  - `src/grounder/symbols/model.rs`: `definitions_named` (line 216), `references_named` (line 226)
  - `src/ingest.rs`: `ingest_project` (line 126), `ingest_project` (line 628), `record_current_generation` (line 994)
  - `src/ledger.rs`: `fully_done` (line 574), `is_integrated` (line 651)
  - `src/spawn.rs`: `new` (line 320), `with_system_prompt` (line 338), `with_model` (line 344), `with_tools` (line 350), `with_dir` (line 356), `with_blast_radius` (line 362), `with_title` (line 368), `with_reviews` (line 374), `park` (line 399)
  - `src/worktree.rs`: `is_dirty` (line 679)

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

#### 2. Close the `Grounder` port gap for whole-project batch ingest (retires dup-0206 in the same motion)

- Scope: section 3 violation 2 (`src/ingest.rs:187-211` `walk_batches`, reaching `grounder::symbols::events::project_batches_paced` and `grounder::design::events::project_batches` by concrete module path) and duplication cluster `dup-0206` (the same two modules' own twin `project_batches` functions, `src/grounder/symbols/events.rs:36-38` / `src/grounder/design/events.rs:90-114`) are one root cause, not two - fix once. TWO CANDIDATES, ONE HOME (spec 85 CONSTRAINTS WALK): `dup-0206`'s own mechanical `proposed_home` suggests relocating into `tests/common`, but both sites are production code under `src/grounder/`, not test helpers - the mechanical heuristic has no "add a port method" category to route a production duplicate to, so it mis-fires here. This plan follows section 3's own reasoned disposition instead: add a `Grounder::project_batches` port method (or a standalone `SymbolProjector` trait) covering both concrete modules, and point `ingest.rs` at it.
- Files: `src/ingest.rs`, `src/grounder/mod.rs`, `src/grounder/symbols/events.rs`, `src/grounder/design/events.rs`.
- Expected line delta: roughly neutral - one new trait method plus two thin impls, minus the two duplicate bodies `dup-0206` catalogs.
- Risk: medium. `ingest.rs`'s own module doc calls it "the ONE walk-and-content-key authority" - a load-bearing path; needs the existing whole-project-ingest and reindex-freshening coverage to stay green, not just the two duplicate-site tests.
- Unblocks: retires the one `Grounder` port violation section 3 found and `dup-0206` together, rather than as two separately-tracked fixes.

#### 3. Retire the duplicate `/proc`-reading authority (`dup-0148` + `dup-0149`)

- Scope: `src/dash.rs::process_state` (`src/dash.rs:499-507`) and `src/main.rs::pgid_of` (`src/main.rs:23346-23359`) each independently re-derive `/proc/<pid>/stat` and `/proc/<pid>/status` fields that `src/reap.rs` (`pid_starttime`/`read_ppid`, `src/reap.rs:190-207`) already parses - the exact "second mutation authority" example spec 85's own Goal names and spec 62's capstone previously caught (`dup-0149`, 15 sites: `src/dash.rs`, `src/main.rs`, `src/reap.rs`, `tests/cli.rs`, `tests/mutation_runner_pdeathsig_periphery.rs` - spec 91's own launcher-exits proving test reads `/proc/<pid>/stat` directly for the same reason `dash.rs::process_state` does, growing this already-known cluster by one site rather than opening a new one), plus 60 raw `/proc`-path string literals scattered across `src/dash.rs`, `src/main.rs`, `src/reap.rs` and four test files with no shared composer (`dup-0148`). Both clusters' own `proposed_home` agree: `src/reap.rs` becomes the one `/proc`-reading module; `dash.rs` and `main.rs` call it instead of re-parsing. NOT SYMMETRIC: `process_state` is reachable from `dash`'s own always-on production server, so it is the actual active-correctness risk this tier-1 placement is about; `pgid_of` sits inside `main.rs`'s `mod tests` (opened at `src/main.rs:12825`) and is called only by `#[test]` fns, so on its own it earns no tier-1 placement - it rides in this same item only because it shares `dup-0148`/`dup-0149`'s one root cause and one proposed fix with `process_state`, not because retiring it retires any live risk of its own.
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

#### 10. Consolidate the 655 `.rigger`-path string-literal sites (`dup-0056`) - the single largest cluster in the entire catalog by site count

- Scope: one `.rigger`-relative path-composition helper (the cluster's own `proposed_home`) every one of the 662 sites routes through instead of building its own literal.
- Files: spans dozens of files including `src/conductor.rs`, `src/config.rs`, `src/dash.rs`, `src/docs.rs`, `src/gate.rs`, `src/grounder/mod.rs`, `src/grounder/symbols/store.rs`, `src/ingest.rs`, `src/main.rs`, `src/reap.rs`, `src/registry.rs`, `src/worktree.rs` plus many `tests/` files - the full site list is in the committed `docs/audit/duplication-catalog.json` under `dup-0056` for the follow-up spec to consume directly, not re-enumerated here.
- Expected line delta: negative - 662 literal compositions collapse toward one helper's call sites; the helper itself is small.
- Risk: medium - the largest surface-area sweep in this plan by site count, even though each individual site is trivial; needs a mechanical rewrite pass plus a full-suite green run, not hand-editing 662 sites.
- Unblocks: the biggest single site-count reduction available anywhere in the duplication catalog.

#### 11. Consolidate the 333 `Command::new` call sites (`dup-0006`) behind one injected process-spawn port

- Scope: one process-spawn seam every `Command::new` site routes through (the cluster's own `proposed_home`).
- Files: spans `src/budget.rs`, `src/conductor.rs`, `src/dash.rs`, `src/driver/cli.rs`, `src/gate.rs`, `src/main.rs`, `src/worktree.rs` plus many `tests/` files - full site list in `docs/audit/duplication-catalog.json` under `dup-0006`.
- Expected line delta: negative, though smaller per-site than `dup-0056` since each `Command::new` call already carries real configuration (args, env, cwd) that must move with it, not just a literal.
- Risk: medium-high - several of these 333 sites sit inside `src/budget.rs`'s and `src/conductor.rs`'s already-hardened process-lifecycle code (spec 78's no-os-kill discipline); the follow-up spec must preserve every existing handle-bound-kill invariant at each site it touches, and the no-os-kill gate is the acceptance bar, not merely `cargo test`.
- Unblocks: one seam instead of 333 independent constructions - the next process-spawning concern added anywhere in the crate reuses it instead of adding site 334.

#### 12. Consolidate the 46 sqlite `Connection::open` call sites (`dup-0110`)

- Scope: one sqlite-connection-opening adapter function (the cluster's own `proposed_home`) spanning `src/contextgraph/sqlite.rs`, `src/eventstore/sqlite.rs` and `src/main.rs`, plus several `tests/` files.
- Files: full site list in `docs/audit/duplication-catalog.json` under `dup-0110`.
- Expected line delta: negative - 46 open calls collapse toward one function.
- Risk: medium - touches the event store and context graph's own connection-lifecycle code; needs the store-identity and store-resolution contract tests green throughout.
- Unblocks: one place to change pragma/timeout/journal-mode settings instead of 46.

#### 13. Consolidate the 5 error-shaping helper sites (`dup-0215`) - caution, confirm before merging

- Scope: the cluster spans `src/grounder/mod.rs` (`retired_grounder_error`), `src/worktree.rs` (`revert_on_base_aborts_and_errors_on_a_conflicting_revert`), `src/conductor.rs` (a `mod tests` case, `integrate_plan_commits_wraps_any_hard_error_with_the_plan_landing_marker`) and three unrelated test files, at line counts from 7 to 132 - a wide spread for one claimed duplicate. This may be a threshold-gaming false cluster (spec 85's own CONSTRAINTS WALK: "the threshold is a floor for the mechanical pass; the reading pass owns semantic duplicates") rather than one real shared concern - the follow-up spec's first job is confirming by reading whether these six sites share actual logic before proposing one helper, not assuming the cluster label proves it.
- Files: `src/grounder/mod.rs`, `src/worktree.rs`, plus the three test files named in `docs/audit/duplication-catalog.json` under `dup-0215`.
- Expected line delta: unknown pending the confirmation read above - potentially zero if the cluster does not survive a human read.
- Risk: low (the smallest-site-count sweep), but with the stated precondition.
- Unblocks: either a genuine fifth consolidation, or a documented "not a real duplicate" disposition that keeps the catalog honest for whoever reads it next.

### 6.6 Tier 5: test-suite consolidation

Every entry cites section 5's own already-catalogued test-only duplication; none of it carries production-correctness risk.

#### 14. Extract the four headline shared test fixtures into `tests/common` (section 5.2)

- Scope: `page_script` (`dup-0350`, 19 files), `node_available` (`dup-0248`, 23 files - merged with two related availability-check helpers), `temp_project` (`dup-0376`, 19 files) and `run_stream_identity` (`dup-0382`, 18 files) - roughly 72 duplicate definitions collapsing into four shared ones, the single largest mechanical simplification section 5 identifies anywhere in the test suite.
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

- Scope, largest first: `dup-0667` (42 sites, `tests/spec_lint.rs`), `dup-0608`/`dup-0610` (15+7 sites, `tests/reap_before_removal_audit.rs`), `dup-0597`/`dup-0598` (11+4 sites, `tests/no_os_kill_audit.rs`), `dup-0641` (11 sites, `tests/simplification_audit.rs` - this very generator's own scanner tests) - 90 sites across 6 clusters.
- Files: the four files named above.
- Expected line delta: negative - each family's near-identical test bodies collapse into one parametrized loop over a table.
- Risk: low - test-only, and each family already shares one body shape (section 5.5's own finding).
- Unblocks: the largest reduction in raw `#[test]` count available in the suite (roughly 90 named tests retiring toward 4).

#### 17. Sweep the remaining 177 test-only helper-duplication clusters (section 5.4, beyond item 14's four headline fixtures)

- Scope: the 181 test-only, all-helper-function clusters section 5.4 names, minus the 4 item 14 already covers - consumed directly from `docs/audit/duplication-catalog.json`, not re-enumerated here (section 5.4's own stated approach). Includes the `dup-0375`/`dup-0377` `temp_project` companion and variant clusters section 5.4 itself places in this "beyond the four" bucket.
- Files: per-cluster, from the committed catalog.
- Expected line delta: negative, cumulative across 177 clusters.
- Risk: low - test-only.
- Unblocks: closes out the helper-duplication half of the test suite's own strict-DRY exposure.

#### 18. Sweep the remaining 153 table-driven test families (section 5.5, beyond item 16's four headline families)

- Scope: the 159 test-only, all-`#[test]` clusters section 5.5 names, minus the 6 cluster ids item 16 already covers - consumed directly from `docs/audit/duplication-catalog.json`. Includes `dup-0600` (4 sites, `tests/no_os_kill_test_helper_periphery.rs`), the smallest of section 5.5's own named large families, left here rather than in item 16.
- Files: per-cluster, from the committed catalog.
- Expected line delta: negative, cumulative.
- Risk: low - test-only.
- Unblocks: closes out the table-driven-test half of the test suite's own strict-DRY exposure; combined with item 17, retires all 340 test-only clusters section 2 found.

### 6.7 Tier 6: remaining catalog sweep

Unlike tier 5, this entry's own clusters are NOT known to be test-only - each one needs its own read before merging (see `### 6.1`'s tier 6 rationale above).

#### 19. Sweep the remaining 327 src-touching duplication clusters (section 2, beyond tiers 1 and 4's 7 named clusters)

- Scope: of the catalog's 674 clusters, 340 are test-only (items 14 and 16-18 above) and 7 are the named tier-1/tier-4 items (`dup-0006`, `dup-0056`, `dup-0110`, `dup-0148`, `dup-0149`, `dup-0206`, `dup-0215`); the remaining 327 clusters touching `src/` - mostly small 2-5-site exact/near matches like the two worked examples section 2 itself opens with (`dup-0001`, `dup-0002`) - are swept here, largest exact-duplicate clusters first, consumed directly from `docs/audit/duplication-catalog.json`.
- Files: per-cluster, from the committed catalog.
- Expected line delta: negative, cumulative; the largest single contributor is whichever exact cluster has the most sites (read from the catalog at spec-writing time, not fixed here).
- Risk: low-medium - unlike tier 5, some of these clusters are production code, so each merge needs its own test-coverage check, not a blanket "test-only" pass.
- Unblocks: the last of the catalog's 674 clusters; after items 1-3 and 10-19 all land, a future spec can state and check that the duplication catalog's own drift guard finds zero live clusters left unaddressed.

### 6.8 Dead and vestigial code beyond item 0: no further follow-up

Spec 87 redid section 4 (item 0, Tier 1, above, is that redo's own real follow-up: delete the 23 `delete`-dispositioned candidates). Of section 4.3's remaining 3 candidates, all `keep-pending`, NONE gets a new refactoring-spec stub here: each already cites its own governing, ALREADY-LANDED spec as the thing a future wiring pass would extend - spec 27 for `distiller::rebuild`, spec 32 for `Defaults::sdet_author_enabled`, spec 60 for `Store::with_content_identity` - not a gap this plan should re-propose as a fresh entry - re-litigating an already-landed spec's own scope is out of place in a plan whose own rule is "adds no new findings". `keep-public-surface` is explicitly empty (0 of 26 candidates cite a real MCP/workflow-template/CLI-contract consumer) - stated so with the search that established it, never omitted (spec 85's own CONSTRAINTS WALK), the same discipline spec 85's original all-clean section 4 applied to a scope this redo has since superseded. Both named retirements (`turbovec`, `kurrentdb`, spec 85's own original section 4 finding, unaffected by spec 87's redo since neither is a `src/` production fn) are still fully clean, and the two stale-looking doc paths found remain confirmed generic illustrative examples, not real dangling references - re-verified, not re-scanned, by this criterion's own research.
